/** Headless release verification against the desktop engine and packaged sidecars. */
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import readline from "node:readline";
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath, pathToFileURL } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const target = process.env.CARGO_TARGET_DIR || path.join(repo, "target");
const sidecars = process.env.JYROXA_VERIFIED_SIDECARS_DIR || path.join(target, "release");
const stage = fs.mkdtempSync(path.join(os.tmpdir(), "jyroxa-release-0112-"));
console.log(`Evidence: ${stage}`);
const data = path.join(stage, "data");
fs.mkdirSync(data);
for (const name of ["jyroxa-release-gate.exe", "jyroxa.exe", "jyroxa-native-host.exe", "WebView2Loader.dll", "yt-dlp.exe", "ffmpeg.exe"]) {
  const source = name === "jyroxa-release-gate.exe" ? path.join(target, "release") : sidecars;
  fs.copyFileSync(path.join(source, name), path.join(stage, name));
}
const env = { ...process.env, JYROXA_DATA_DIR: data, JYROXA_CLI: path.join(stage, "jyroxa.exe"), JYROXA_NATIVE_HOST_NO_LAUNCH: "1" };
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const rows = [];
const ownedSidecars = new Map();
async function processInfo(filter) {
  const envScript = path.join(process.env.JYROXA_TOOLS || path.join(os.homedir(), ".jyroxa-tools"), "env.ps1");
  const script = `. '${envScript.replaceAll("'", "''")}'; @(Get-CimInstance Win32_Process -Filter "${filter}") | Select-Object ProcessId,CreationDate,ExecutablePath | ConvertTo-Json -Compress`;
  const { stdout } = await promisify(execFile)("powershell.exe", ["-NoProfile", "-Command", script], { windowsHide: true, timeout: 30_000 });
  return stdout.trim() ? [JSON.parse(stdout)].flat() : [];
}
async function torrentChild(gatePid) {
  const children = await processInfo(`Name='jyroxa.exe' AND ParentProcessId=${gatePid}`);
  assert.equal(children.length, 1, "exactly one torrent sidecar required");
  const child = children[0];
  assert.equal(child.ExecutablePath.toLowerCase(), path.join(stage, "jyroxa.exe").toLowerCase());
  ownedSidecars.set(child.ProcessId, child);
  return child.ProcessId;
}
async function killOwned(pid) {
  const expected = ownedSidecars.get(pid);
  const [current] = await processInfo(`ProcessId=${pid}`);
  if (current && current.CreationDate === expected?.CreationDate && current.ExecutablePath === expected.ExecutablePath) process.kill(pid);
}
function alive(pid) {
  try { process.kill(pid, 0); return true; } catch (error) {
    if (error.code === "ESRCH") return false;
    throw error;
  }
}
function pass(name, evidence) {
  rows.push({ name, result: "PASS", evidence });
  fs.writeFileSync(path.join(stage, "results.json"), JSON.stringify(rows, null, 2));
  console.log(JSON.stringify(rows.at(-1)));
}
function start() {
  const child = spawn(path.join(stage, "jyroxa-release-gate.exe"), [], { env, windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
  child.stderr.pipe(fs.createWriteStream(path.join(stage, `desktop-${child.pid}.log`)));
  const waiting = [];
  readline.createInterface({ input: child.stdout }).on("line", line => {
    const pending = waiting.shift();
    if (!pending) return;
    try {
      const reply = JSON.parse(line);
      reply.ok ? pending.resolve(reply.result) : pending.reject(new Error(reply.error));
    } catch (error) { pending.reject(error); }
  });
  child.on("error", error => { for (const p of waiting.splice(0)) p.reject(error); });
  child.on("exit", (code, signal) => { for (const p of waiting.splice(0)) p.reject(new Error(`desktop exited code=${code} signal=${signal}`)); });
  return {
    child,
    async request(command, args = {}, timeout = 180_000) {
      let timer;
      const reply = new Promise((resolve, reject) => {
        waiting.push({ resolve, reject });
        child.stdin.write(JSON.stringify({ command, ...args }) + "\n");
        timer = setTimeout(() => reject(new Error(`command timed out: ${command}`)), timeout);
      });
      try { return await reply; } finally { clearTimeout(timer); }
    },
  };
}
async function native(message) {
  const body = Buffer.from(JSON.stringify(message));
  const header = Buffer.alloc(4); header.writeUInt32LE(body.length);
  const child = spawn(path.join(stage, "jyroxa-native-host.exe"), [], { env, windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
  const chunks = [];
  child.stdout.on("data", chunk => chunks.push(chunk));
  const done = new Promise((resolve, reject) => {
    child.on("error", reject);
    child.on("exit", code => code === 0 ? resolve() : reject(new Error(`native host exit ${code}`)));
  });
  child.stdin.end(Buffer.concat([header, body]));
  const timer = setTimeout(() => child.kill(), 180_000);
  try {
    await done;
    const output = Buffer.concat(chunks);
    assert.ok(output.length >= 4, "native host must return a framed reply");
    const reply = JSON.parse(output.subarray(4, 4 + output.readUInt32LE()).toString());
    assert.equal(reply.ok, true, JSON.stringify(reply));
    return reply;
  } finally { clearTimeout(timer); }
}
async function waitFor(gate, id, predicate, seconds, label) {
  const deadline = Date.now() + seconds * 1000;
  let row;
  while (Date.now() < deadline) {
    row = (await gate.request("list")).find(item => item.id === id);
    assert.ok(row, `missing task ${id}`);
    if (predicate(row)) return row;
    assert.notEqual(row.status, "failed", JSON.stringify(row));
    await sleep(200);
  }
  throw new Error(`${label} deadline: ${JSON.stringify(row)}`);
}
function args(url, directory, filename, sha256 = null) {
  return { urls: [url], output_dir: directory, filename, connections: 8, connections_auto: true, sha256, md5: null, http_headers: null };
}
async function hashFile(filename) {
  const hash = crypto.createHash("sha256");
  for await (const chunk of fs.createReadStream(filename)) hash.update(chunk);
  return hash.digest("hex");
}
// Capture the raw info dictionary so a magnet has the independently verified hash.
function metainfo(buffer) {
  let offset = 0;
  let rawInfo;
  function read() {
    const tag = buffer[offset];
    if (tag === 105) { const end = buffer.indexOf(101, ++offset); const n = Number(buffer.subarray(offset, end)); offset = end + 1; return n; }
    if (tag === 100 || tag === 108) {
      offset++;
      const value = tag === 100 ? {} : [];
      while (buffer[offset] !== 101) {
        if (tag === 108) value.push(read());
        else {
          const key = read().toString(); const start = offset; value[key] = read();
          if (key === "info") rawInfo = buffer.subarray(start, offset);
        }
      }
      offset++; return value;
    }
    const colon = buffer.indexOf(58, offset);
    const length = Number(buffer.subarray(offset, colon));
    assert.ok(Number.isSafeInteger(length) && length >= 0);
    offset = colon + 1 + length;
    return buffer.subarray(colon + 1, offset);
  }
  const value = read();
  assert.ok(rawInfo);
  return { name: value.info.name.toString(), hash: crypto.createHash("sha1").update(rawInfo).digest("hex") };
}

const payload = crypto.randomBytes(32 * 1024 * 1024);
const digest = crypto.createHash("sha256").update(payload).digest("hex");
const server = http.createServer((req, res) => {
  const range = /^bytes=(\d+)-(\d*)$/.exec(req.headers.range || "");
  let cursor = range ? Number(range[1]) : 0;
  const end = range?.[2] ? Math.min(Number(range[2]), payload.length - 1) : payload.length - 1;
  res.writeHead(range ? 206 : 200, { "Content-Length": end - cursor + 1, "Accept-Ranges": "bytes", "Content-Range": `bytes ${cursor}-${end}/${payload.length}`, "ETag": '"gate-v1"' });
  if (req.method === "HEAD") return res.end();
  const timer = setInterval(() => {
    const next = Math.min(cursor + 64 * 1024, end + 1);
    res.write(payload.subarray(cursor, next)); cursor = next;
    if (cursor > end) { clearInterval(timer); res.end(); }
  }, 20);
  res.on("close", () => clearInterval(timer));
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
let gate;
try {
  const binaries = {};
  for (const name of ["jyroxa-release-gate.exe", "jyroxa.exe", "jyroxa-native-host.exe", "WebView2Loader.dll", "yt-dlp.exe", "ffmpeg.exe"]) {
    binaries[name] = await hashFile(path.join(stage, name));
  }
  pass("staged release binaries", binaries);
  gate = start();
  await gate.request("settings", { settings: { download_dir: path.join(stage, "downloads"), max_concurrent: 3, clipboard_monitor: false } });
  await native({ type: "ping" });
  pass("extension native-host connection", "framed ping through real desktop named pipe from isolated stage");

  const url = `http://127.0.0.1:${server.address().port}/file`;
  const output = path.join(stage, "downloads");
  const id = await gate.request("add", { args: args(url, output, "normal.bin", digest) });
  await waitFor(gate, id, row => row.downloaded > 0, 10, "HTTP start");
  assert.equal(await gate.request("pause", { id }), true);
  const paused = (await gate.request("list")).find(row => row.id === id);
  assert.equal(paused.status, "paused");
  await sleep(1300);
  const checkpoint = JSON.parse(fs.readFileSync(path.join(output, "normal.bin.jxstate")));
  assert.ok(checkpoint.segments.reduce((sum, part) => sum + part.downloaded, 0) > 0, "pause must preserve committed resume bytes");
  await gate.request("save");
  const queue = JSON.parse(fs.readFileSync(path.join(data, "queue.json")));
  assert.ok(queue.items.some(row => row.status === "paused"));
  const old = gate.child;
  old.kill();
  await new Promise(resolve => old.once("exit", resolve));
  gate = start();
  await gate.request("restore");
  const restored = (await gate.request("list")).find(row => row.filename === "normal.bin");
  assert.ok(restored && restored.status === "paused", "paused state must survive restart");
  assert.ok(restored.downloaded > 0, "restart must recover saved byte cursors");
  assert.equal(await gate.request("resume", { id: restored.id }), true);
  await waitFor(gate, restored.id, row => row.status === "completed", 90, "HTTP resume");
  assert.equal(await hashFile(path.join(output, "normal.bin")), digest);
  pass("normal HTTP pause/resume/restart", "32 MiB, SHA-256 verified, paused queue.json survives process restart");

  const publicUrl = "https://www.gnu.org/licenses/gpl-3.0.txt";
  const publicResponse = await fetch(publicUrl, {
    signal: AbortSignal.timeout(30_000),
    headers: { "User-Agent": "Jyroxa/0.1.13" },
  });
  assert.equal(publicResponse.ok, true);
  const publicDigest = crypto.createHash("sha256").update(Buffer.from(await publicResponse.arrayBuffer())).digest("hex");
  const publicId = await gate.request("add", { args: args(publicUrl, output, "gpl-3.0.txt", publicDigest) });
  await waitFor(gate, publicId, row => row.status === "completed", 60, "public HTTPS link");
  assert.equal(await hashFile(path.join(output, "gpl-3.0.txt")), publicDigest);
  pass("public HTTPS download", { url: publicUrl, sha256: publicDigest });

  const base = "https://cdimage.debian.org/debian-cd/current/amd64/bt-cd/";
  const listing = await (await fetch(base, { signal: AbortSignal.timeout(30_000) })).text();
  const sums = await (await fetch(base + "SHA256SUMS", { signal: AbortSignal.timeout(30_000) })).text();
  const torrents = [...new Set([...listing.matchAll(/href="(debian-(?:edu-|mac-)?[0-9.]+-amd64-netinst\.iso\.torrent)"/g)].map(match => match[1]))];
  assert.ok(torrents.length >= 3, "three official torrent fixtures required");
  const standard = torrents.find(name => /^debian-[0-9]/.test(name));
  assert.ok(standard, "standard Debian netinst is required for the DHT-only case");
  const fixtures = [...torrents.filter(name => name !== standard).slice(0, 2), standard];
  for (const [index, filename] of fixtures.entries()) {
    const response = await fetch(base + filename, { signal: AbortSignal.timeout(30_000) });
    assert.equal(response.ok, true);
    const bytes = Buffer.from(await response.arrayBuffer());
    const info = metainfo(bytes);
    const local = path.join(stage, filename); fs.writeFileSync(local, bytes);
    const source = index === 0 ? pathToFileURL(local).href : index === 1 ? base + filename : `magnet:?xt=urn:btih:${info.hash}`;
    const dir = path.join(output, `torrent-${index}`);
    await gate.request("settings", { settings: {
      torrent_merge_trackers: index !== 2,
      download_dir: dir,
      browser_integration: { silent_add: true },
    } });
    const torrentArgs = { ...args(source, dir, info.name), torrent_info_hash: info.hash };
    const handoff = await native({ type: "handoff", url: source, filename: info.name });
    let torrentId = handoff.download_id;
    assert.ok(Number.isSafeInteger(torrentId), "native handoff must return its queued task ID");
    assert.equal(await gate.request("add", { args: torrentArgs }), torrentId, "duplicate torrent must reuse its task");
    await waitFor(gate, torrentId, row => row.downloaded > 0, 10, `torrent ${index} start`);
    if (index === 0) {
      const runningSidecar = await torrentChild(gate.child.pid);
      await gate.request("save");
      const runningOutput = (await gate.request("list")).find(row => row.id === torrentId).output;
      const runningApp = gate.child;
      runningApp.kill();
      await new Promise(resolve => runningApp.once("exit", resolve));
      for (let i = 0; i < 25 && alive(runningSidecar); i++) await sleep(200);
      assert.equal(alive(runningSidecar), false, "app exit must kill its active torrent process tree");
      ownedSidecars.delete(runningSidecar);
      gate = start();
      await gate.request("restore");
      const resumed = (await gate.request("list")).find(row => row.filename === info.name);
      assert.ok(resumed, "running torrent must survive app restart");
      assert.equal(resumed.output, runningOutput);
      torrentId = resumed.id;
      await waitFor(gate, torrentId, row => row.status === "running" && row.downloaded > 0, 10, "torrent restart");
      pass("app exit kills active sidecar and restores torrent", { terminated_pid: runningSidecar, output: resumed.output });
      assert.equal(await gate.request("pause", { id: torrentId }), true);
      await waitFor(gate, torrentId, row => row.status === "paused", 5, "torrent pause");
      await sleep(1500);
      await gate.request("save");
      const beforeRestart = (await gate.request("list")).find(row => row.id === torrentId);
      const old = gate.child;
      old.kill();
      await new Promise(resolve => old.once("exit", resolve));
      gate = start();
      await gate.request("restore");
      const restored = (await gate.request("list")).find(row => row.filename === info.name);
      assert.ok(restored && restored.status === "paused", "torrent pause must survive restart");
      assert.equal(restored.output, beforeRestart.output, "torrent payload path must survive task renumbering");
      torrentId = restored.id;
      assert.equal(await gate.request("resume", { id: torrentId }), true);
      await waitFor(gate, torrentId, row => row.status === "running", 10, "torrent resume");
      const sidecar = await torrentChild(gate.child.pid);
      await killOwned(sidecar);
      const failed = await waitFor(gate, torrentId, row => row.status === "failed", 10, "killed sidecar failure");
      assert.ok(failed.error, "sidecar death must expose a useful error");
      ownedSidecars.delete(sidecar);
      assert.equal(await gate.request("retry", { id: torrentId }), true);
      pass("torrent pause/resume and sidecar recovery", { error: failed.error, killed_pid: sidecar });
    }
    const completed = await waitFor(gate, torrentId, row => row.status === "completed", 600, `torrent ${index} completion`);
    const expected = sums.split(/\r?\n/).find(line => line.endsWith(info.name))?.split(/\s+/)[0];
    assert.match(expected || "", /^[0-9a-f]{64}$/);
    assert.equal(await hashFile(path.join(dir, info.name)), expected);
    pass(`torrent ${index + 1}`, { source, info_hash: info.hash, bytes: completed.downloaded, sha256: expected });
  }

  for (const url of ["https://www.youtube.com/watch?v=aqz-KE-bpKQ", "https://www.dailymotion.com/video/x89eyek"]) {
    const reply = await native({ type: "probe_video_formats", page_url: url });
    assert.ok(reply.formats?.length >= (url.includes("youtube") ? 2 : 1), `qualities missing: ${url}`);
    assert.ok(reply.formats.every(format => format.format_id !== "bv*+ba/b"), `placeholder qualities: ${url}`);
    const format = reply.formats.find(format => /360p|240p/.test(format.label)) || reply.formats.find(format => /\d+p/.test(format.label));
    assert.ok(format, `a real video quality is required: ${url}`);
    const filename = url.includes("youtube") ? "youtube.mp4" : "dailymotion.mp4";
    const videoArgs = args(url, output, filename);
    videoArgs.http_headers = { "X-Jyroxa-Ytdlp-Page": "1", "X-Jyroxa-Ytdlp-Format": format.format_id };
    const videoId = await gate.request("add", { args: videoArgs });
    await waitFor(gate, videoId, row => row.status === "completed", 600, `video ${url}`);
    const videoFile = path.join(output, filename);
    assert.ok(fs.statSync(videoFile).size > 10_000);
    await promisify(execFile)(path.join(stage, "ffmpeg.exe"), ["-v", "error", "-i", videoFile, "-map", "0:v:0", "-map", "0:a:0", "-f", "null", "-"], { windowsHide: true, timeout: 180_000, maxBuffer: 4 * 1024 * 1024 });
    pass("video qualities and completed download", { url, qualities: reply.formats.length, format: format.format_id, decoded_audio_and_video: true });
  }
  await gate.request("save");
  pass("headless session", "all checks completed");
} catch (error) {
  rows.push({ name: "release gate", result: "FAIL", evidence: error.stack });
  console.error(error);
  process.exitCode = 1;
} finally {
  gate?.child.kill();
  for (const pid of ownedSidecars.keys()) {
    try { await killOwned(pid); } catch (error) { console.error(`sidecar cleanup: ${error.message}`); }
  }
  server.closeAllConnections(); server.close();
  fs.writeFileSync(path.join(stage, "results.json"), JSON.stringify(rows, null, 2));
  console.log(`Evidence: ${stage}`);
}
