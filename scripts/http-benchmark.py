"""Repeat headless HTTP measurements; keep raw JSON and verify every payload hash.

Build crates/jyroxa-core/examples/http_measure.rs in release mode, preserve the
baseline executable, then run this script once per executable with --label.
The first-50MB sample uses progress events (up to one 500 ms tick of error).
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import threading
import time

BLOCK = bytes(range(256)) * 1024
SIZE = 256 * 1024 * 1024
_digest = hashlib.sha256()
for _ in range(SIZE // len(BLOCK)):
    _digest.update(BLOCK)
EXPECTED = _digest.hexdigest()
PROCESS_FLAGS = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0


class BenchmarkServer(http.server.ThreadingHTTPServer):
    request_queue_size = 128
    daemon_threads = True


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def do_HEAD(self):
        self.send_response(200)
        self.send_header("Content-Length", str(SIZE))
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("ETag", '"deterministic-v1"')
        self.end_headers()

    def do_GET(self):
        first, last = 0, SIZE - 1
        ranged = "Range" in self.headers
        if ranged:
            left, right = self.headers["Range"].removeprefix("bytes=").split("-")
            first, last = int(left), min(int(right) if right else SIZE - 1, SIZE - 1)
        self.send_response(206 if ranged else 200)
        self.send_header("Content-Length", str(last - first + 1))
        self.send_header("Content-Range", f"bytes {first}-{last}/{SIZE}")
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("ETag", '"deterministic-v1"')
        self.end_headers()
        started = time.perf_counter()
        sent = 0
        try:
            while first <= last:
                offset = first % len(BLOCK)
                data = BLOCK[offset:offset + min(last - first + 1, len(BLOCK) - offset)]
                self.wfile.write(data)
                first += len(data)
                sent += len(data)
                if self.path == "/capped":
                    delay = sent / 8_000_000 - (time.perf_counter() - started)
                    if delay > 0:
                        time.sleep(delay)
        except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError):
            pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", required=True, type=Path)
    parser.add_argument("--label", required=True)
    parser.add_argument("--dir", type=Path, default=Path(".jyroxa-bench"))
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--run-offset", type=int, default=0)
    parser.add_argument("--url", action="append", default=[])
    parser.add_argument("--hashes", type=Path, help="JSON mapping public URLs to independently obtained SHA-256 hashes")
    parser.add_argument("--local", action="store_true")
    parser.add_argument("--resume-check", action="store_true")
    parser.add_argument("--timeout", type=float, default=180)
    args = parser.parse_args()
    args.dir.mkdir(parents=True, exist_ok=True)
    hashes = json.loads(args.hashes.read_text(encoding="utf-8")) if args.hashes else {}
    cases = [(f"public-{i + 1}", url, hashes.get(url)) for i, url in enumerate(args.url)]
    server = None
    if args.local or args.resume_check:
        server = BenchmarkServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        if args.local:
            cases[:0] = [(name, f"http://127.0.0.1:{server.server_port}/{path}", EXPECTED)
                         for name, path in [("local-unlimited", "unlimited"), ("local-capped", "capped")]]
    try:
        for name, url, expected in cases:
            for run in range(1 + args.run_offset, args.repeat + 1 + args.run_offset):
                output = args.dir / f"{args.label}-{name}-{run}.bin"
                command = [str(args.exe.resolve()), url, str(output.resolve()), "32"]
                if expected:
                    command.append(expected)
                try:
                    result = subprocess.run(command, capture_output=True, text=True, timeout=args.timeout,
                                            creationflags=PROCESS_FLAGS)
                    if result.returncode:
                        raise RuntimeError(result.stderr[-2000:])
                    record = json.loads(result.stdout.strip().splitlines()[-1])
                    record.update(label=args.label, case=name, run=run)
                    if expected and record["sha256"] != expected:
                        raise RuntimeError("local payload hash mismatch")
                except (subprocess.TimeoutExpired, RuntimeError, ValueError) as error:
                    record = dict(label=args.label, case=name, run=run, url=url, error=str(error))
                with (args.dir / "measurements.jsonl").open("a", encoding="utf-8") as log:
                    log.write(json.dumps(record) + "\n")
                print(json.dumps(record), flush=True)
                # Delete only this run's disposable payload and checkpoint.
                output.unlink(missing_ok=True)
                Path(str(output) + ".jxstate").unlink(missing_ok=True)
                time.sleep(0.5)
        if args.resume_check:
            output = args.dir / f"{args.label}-resume.bin"
            checkpoint = Path(str(output) + ".jxstate")
            command = [str(args.exe.resolve()), f"http://127.0.0.1:{server.server_port}/capped",
                       str(output.resolve()), "8", EXPECTED]
            process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                       text=True, creationflags=PROCESS_FLAGS)
            try:
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    if process.poll() is not None:
                        raise RuntimeError("download exited before kill checkpoint")
                    try:
                        state = json.loads(checkpoint.read_text(encoding="utf-8"))
                        if sum(s["downloaded"] for s in state["segments"]) >= 8_000_000:
                            break
                    except (OSError, ValueError):
                        pass
                    time.sleep(0.02)
                else:
                    raise RuntimeError("no committed progress before kill")
            finally:
                if process.poll() is None:
                    process.kill()
                process.communicate(timeout=10)
            result = subprocess.run(command, capture_output=True, text=True, timeout=args.timeout,
                                    creationflags=PROCESS_FLAGS)
            if result.returncode:
                raise RuntimeError(result.stderr[-2000:])
            record = json.loads(result.stdout.strip().splitlines()[-1])
            assert record["resumed"] and record["resumed_from"] > 0, record
            assert record["sha256"] == EXPECTED, record
            record.update(label=args.label, case="resume-after-kill", run=1)
            with (args.dir / "measurements.jsonl").open("a", encoding="utf-8") as log:
                log.write(json.dumps(record) + "\n")
            print(json.dumps(record), flush=True)
            output.unlink()
    finally:
        if server:
            server.shutdown()


if __name__ == "__main__":
    main()
