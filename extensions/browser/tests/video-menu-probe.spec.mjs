/**
 * probe_video_formats via native host + jyroxa-desktop (headless console, temp data dir).
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "../../..");
const targetDir =
  process.env.CARGO_TARGET_DIR || path.join(repoRoot, "target");
const desktop = path.join(targetDir, "release", "jyroxa-desktop.exe");
const hostExe = path.join(targetDir, "release", "jyroxa-native-host.exe");

if (!fs.existsSync(desktop) || !fs.existsSync(hostExe)) {
  console.error("skip: build release jyroxa-desktop and native-host first");
  process.exit(0);
}

const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), "jyroxa-vmenu-probe-"));
const dlDir = path.join(dataDir, "downloads");
fs.mkdirSync(dlDir, { recursive: true });
const release = path.dirname(desktop);
for (const name of ["yt-dlp.exe", "ffmpeg.exe"]) {
  const src = path.join(repoRoot, "apps", "desktop", "src-tauri", name);
  if (fs.existsSync(src)) {
    fs.copyFileSync(src, path.join(release, name));
  }
}

const child = spawn(desktop, ["--minimized"], {
  env: { ...process.env, JYROXA_DATA_DIR: dataDir },
  stdio: "ignore",
  windowsHide: true,
});

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const tokenPath = path.join(dataDir, "browser-integration.token");
for (let i = 0; i < 60 && !fs.existsSync(tokenPath); i += 1) {
  await sleep(500);
}
assert.ok(fs.existsSync(tokenPath), "browser token should exist after desktop start");
const token = fs.readFileSync(tokenPath, "utf8").trim();

function buildFrame(obj) {
  const body = Buffer.from(JSON.stringify(obj), "utf8");
  const len = Buffer.alloc(4);
  len.writeUInt32LE(body.length, 0);
  return Buffer.concat([len, body]);
}

function parseFrame(buf) {
  const n = buf.readUInt32LE(0);
  return JSON.parse(buf.subarray(4, 4 + n).toString("utf8"));
}

function probe(pageUrl) {
  const env = {
    ...process.env,
    JYROXA_DATA_DIR: dataDir,
    JYROXA_NATIVE_HOST_NO_LAUNCH: "1",
  };
  const childProbe = spawnSync(
    hostExe,
    [],
    {
      env,
      input: buildFrame({ type: "probe_video_formats", page_url: pageUrl }),
      maxBuffer: 16 * 1024 * 1024,
      timeout: 180_000,
    },
  );
  assert.equal(childProbe.status, 0, childProbe.stderr?.toString());
  return parseFrame(childProbe.stdout);
}

const yt = probe("https://www.youtube.com/watch?v=aqz-KE-bpKQ");
assert.equal(yt.ok, true, JSON.stringify(yt));
assert.ok((yt.formats?.length || 0) >= 2, "expected multiple YouTube qualities");

let dm = probe("https://www.dailymotion.com/video/x89eyek");
if (!dm.ok) {
  await sleep(2000);
  dm = probe("https://www.dailymotion.com/video/x89eyek");
}
assert.equal(dm.ok, true, JSON.stringify(dm));
assert.ok((dm.formats?.length || 0) >= 1, "expected Dailymotion qualities");

spawnSync(hostExe, [], {
  env: {
    ...process.env,
    JYROXA_DATA_DIR: dataDir,
    JYROXA_NATIVE_HOST_NO_LAUNCH: "1",
  },
  input: buildFrame({ type: "set_silent_add", silent_add: true }),
  maxBuffer: 1024 * 1024,
});

const handoff = spawnSync(
  hostExe,
  [],
  {
    env: {
      ...process.env,
      JYROXA_DATA_DIR: dataDir,
      JYROXA_NATIVE_HOST_NO_LAUNCH: "1",
    },
    input: buildFrame({
      type: "handoff",
      url: "https://www.youtube.com/watch?v=aqz-KE-bpKQ",
      page_url: "https://www.youtube.com/watch?v=aqz-KE-bpKQ",
      video_page: true,
      format_id:
        yt.formats.find((f) => /audio|360p|240p/i.test(f.label))?.format_id ||
        yt.formats[yt.formats.length - 1].format_id,
    }),
    maxBuffer: 4 * 1024 * 1024,
    timeout: 300_000,
  },
);
const handoffReply = parseFrame(handoff.stdout);
assert.equal(handoffReply.ok, true, JSON.stringify(handoffReply));

function largeMediaUnder(dir) {
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, ent.name);
    if (ent.isDirectory()) {
      if (largeMediaUnder(p)) return true;
    } else if (/\.(mp4|mkv|webm|m4a)$/i.test(ent.name) && !ent.name.endsWith(".part")) {
      if (fs.statSync(p).size > 10_000) return true;
    }
  }
  return false;
}

const downloadsDir = path.join(process.env.USERPROFILE || "", "Downloads");
const started = Date.now();
for (let i = 0; i < 180; i += 1) {
  if (largeMediaUnder(dataDir) || recentMediaIn(downloadsDir, started)) break;
  await sleep(1000);
}
assert.ok(
  largeMediaUnder(dataDir) || recentMediaIn(downloadsDir, started),
  `expected completed download; handoff=${JSON.stringify(handoffReply)}`,
);

function recentMediaIn(dir, sinceMs) {
  if (!dir || !fs.existsSync(dir)) return false;
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    if (!ent.isFile()) continue;
    if (!/\.(mp4|mkv|webm|m4a)$/i.test(ent.name)) continue;
    const p = path.join(dir, ent.name);
    const st = fs.statSync(p);
    if (st.mtimeMs >= sinceMs - 5000 && st.size > 10_000) return true;
  }
  return false;
}

try {
  child.kill();
} catch {
}

console.log("video-menu-probe.spec.mjs OK");