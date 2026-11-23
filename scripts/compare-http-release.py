"""Compare two published/build CLI binaries against identical hashed local payloads."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import statistics
import subprocess
import threading
import time


def run_cli(command, cwd, process_flags):
    started = time.perf_counter()
    marks = {}
    captured = {"stdout": [], "stderr": []}

    def read_stream(name, stream):
        for line in iter(stream.readline, b""):
            captured[name].append(line)
            if name == "stdout" and b"downloading to" in line:
                marks["download_command_seconds"] = time.perf_counter() - started
            elif name == "stderr" and b"resumed_from=" in line:
                marks["transfer_start_seconds"] = time.perf_counter() - started
            elif name == "stdout" and b"done:" in line:
                marks["completion_seconds"] = time.perf_counter() - started

    with subprocess.Popen(command, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          creationflags=process_flags) as process:
        readers = [threading.Thread(target=read_stream, args=(name, stream))
                   for name, stream in [("stdout", process.stdout), ("stderr", process.stderr)]]
        for reader in readers:
            reader.start()
        try:
            process.wait(timeout=180)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
            raise
        finally:
            for reader in readers:
                reader.join()
        seconds = time.perf_counter() - started
        result = subprocess.CompletedProcess(command, process.returncode,
                                             b"".join(captured["stdout"]), b"".join(captured["stderr"]))
    return result, seconds, marks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", required=True, type=Path)
    parser.add_argument("--after", required=True, type=Path)
    parser.add_argument("--dir", required=True, type=Path)
    parser.add_argument("--repeat", type=int, default=8)
    args = parser.parse_args()
    assert args.repeat >= 3, "release comparison requires at least three runs"
    assert args.repeat % 2 == 0, "use an even count to balance output files and ordering"
    spec = importlib.util.spec_from_file_location("http_benchmark", Path(__file__).with_name("http-benchmark.py"))
    benchmark = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(benchmark)
    root = args.dir.resolve()
    root.mkdir(parents=True, exist_ok=True)
    server = benchmark.BenchmarkServer(("127.0.0.1", 0), benchmark.Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    records = []
    outputs = [root / f"payload-{position}.bin" for position in range(2)]
    assert not any(output.exists() for output in outputs), "comparison outputs must be new"
    try:
        for case in ["unlimited", "capped"]:
            # Warm both output paths before timing. Reusing their allocation
            # avoids comparing fresh-file allocation and scanner startup noise.
            # Each binary then occupies each path/order position equally, while
            # each invocation still has a fresh host-memory working directory.
            for run in range(-1, args.repeat):
                # Alternate ordering and use fresh host memory for each binary.
                clients = [("before", args.before), ("after", args.after)]
                if run >= 0 and run % 2:
                    clients.reverse()
                for position, (label, exe) in enumerate(clients):
                    runtime = root / f"{case}-{run + 1}-{label}"
                    runtime.mkdir(exist_ok=False)
                    output = outputs[position]
                    command = [str(exe.resolve()), "get", f"http://127.0.0.1:{server.server_port}/{case}",
                               "-c", "32", "-o", str(output), "--sha256", benchmark.EXPECTED]
                    result, seconds, marks = run_cli(command, runtime, benchmark.PROCESS_FLAGS)
                    (runtime / "stdout.log").write_bytes(result.stdout)
                    (runtime / "stderr.log").write_bytes(result.stderr)
                    if result.returncode:
                        raise RuntimeError(f"{label}/{case} exited {result.returncode}: {result.stderr[-2000:]!r}")
                    digest = hashlib.sha256()
                    with output.open("rb") as payload:
                        for chunk in iter(lambda: payload.read(1024 * 1024), b""):
                            digest.update(chunk)
                    assert output.stat().st_size == benchmark.SIZE
                    assert digest.hexdigest() == benchmark.EXPECTED
                    record = dict(label=label, case=case, run=run + 1, timed=run >= 0,
                                  output_position=position, seconds=seconds,
                                  phase_marks=marks,
                                  mbps=benchmark.SIZE / seconds / 1_000_000, sha256=digest.hexdigest())
                    records.append(record)
                    with (root / "measurements.jsonl").open("a", encoding="utf-8") as log:
                        log.write(json.dumps(record) + "\n")
                    print(json.dumps(record), flush=True)
                    time.sleep(0.5)
        comparisons = []
        for case in ["unlimited", "capped"]:
            medians = {label: statistics.median(row["mbps"] for row in records
                                              if row["timed"] and row["case"] == case
                                              and row["label"] == label)
                       for label in ["before", "after"]}
            comparisons.append(dict(case=case, before_mbps=medians["before"], after_mbps=medians["after"],
                                    passed=medians["after"] >= medians["before"]))
        (root / "comparison.json").write_text(json.dumps(comparisons, indent=2), encoding="utf-8")
        print(json.dumps(comparisons), flush=True)
        if not all(row["passed"] for row in comparisons):
            raise RuntimeError("HTTP throughput regressed against the baseline release")
    finally:
        server.shutdown()
        for output in outputs:
            output.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
