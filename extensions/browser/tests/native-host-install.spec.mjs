/**
 * Temp-dir native host install + manifest origins + framed ping (no user registry).
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "../../..");
const targetDir =
  process.env.CARGO_TARGET_DIR ||
  path.join(repoRoot, "target");
const hostExe = path.join(targetDir, "release/jyroxa-native-host.exe");
const registerExe = path.join(targetDir, "release/jyroxa-browser-register.exe");

for (const exe of [hostExe, registerExe]) {
  if (!fs.existsSync(exe)) {
    console.error(`skip native-host-install.spec.mjs: build release bins first (missing ${exe})`);
    process.exit(0);
  }
}

const stage = fs.mkdtempSync(path.join(os.tmpdir(), "jyroxa-nh-stage-"));
const dataDir = path.join(stage, "data");
fs.mkdirSync(dataDir, { recursive: true });
const desktop = path.join(stage, "jyroxa-desktop.exe");
fs.writeFileSync(desktop, "stub");
fs.copyFileSync(hostExe, path.join(stage, "jyroxa-native-host.exe"));
fs.copyFileSync(registerExe, path.join(stage, "jyroxa-browser-register.exe"));

const env = {
  ...process.env,
  JYROXA_DATA_DIR: dataDir,
  JYROXA_REGISTRY_PREFIX: "JyroxaHostVideoTest",
  JYROXA_NATIVE_HOST_NO_LAUNCH: "1",
};

const install = spawnSync(registerExe, ["install", desktop], { env, stdio: "pipe" });
assert.equal(install.status, 0, install.stderr?.toString() || install.stdout?.toString());

const manifestPath = path.join(
  dataDir,
  "native-host",
  "com.jyroxa.browser.json",
);
const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const origins = manifest.allowed_origins || [];
for (const id of [
  "njgbdpacpmohafoefmcmnmhdjpjmjipe",
  "hehgieooaeknhlodfpkommnhinaahkjf",
  "lkcpmllaclbmggbphpaojdklcjgmjaad",
]) {
  assert.ok(
    origins.includes(`chrome-extension://${id}/`),
    `missing origin ${id}`,
  );
}

function framedPing(exe) {
  const child = spawnSync(exe, [], {
    env,
    input: buildFrame({ type: "ping" }),
    maxBuffer: 1024 * 1024,
  });
  assert.equal(child.status, 0, child.stderr?.toString());
  return parseFrame(child.stdout);
}

function buildFrame(obj) {
  const body = Buffer.from(JSON.stringify(obj), "utf8");
  const len = Buffer.alloc(4);
  len.writeUInt32LE(body.length, 0);
  return Buffer.concat([len, body]);
}

function parseFrame(buf) {
  const n = buf.readUInt32LE(0);
  const body = buf.subarray(4, 4 + n);
  return JSON.parse(body.toString("utf8"));
}

const hostCopy = path.join(dataDir, "jyroxa-native-host.exe");
assert.ok(fs.existsSync(hostCopy), "host should be copied into data dir");

const reply = framedPing(hostCopy);
assert.ok(reply && typeof reply === "object");
assert.ok(
  reply.ok === true || typeof reply.error === "string",
  `unexpected ping reply ${JSON.stringify(reply)}`,
);

spawnSync(registerExe, ["uninstall"], { env, stdio: "pipe" });

console.log("native-host-install.spec.mjs OK");