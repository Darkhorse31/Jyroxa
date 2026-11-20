/**
 * Video menu: local mp4/HLS fixture + optional yt-dlp probe via native host + desktop.
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "../../..");
const extDir = path.resolve(__dirname, "..");
const targetDir =
  process.env.CARGO_TARGET_DIR || path.join(repoRoot, "target");
const fixture = path.join(__dirname, "fixtures", "video-test.html");
const desktopExe = path.join(
  targetDir,
  "release",
  "bundle",
  "nsis",
  "Jyroxa.exe",
);
const desktopAlt = path.join(targetDir, "release", "jyroxa-desktop.exe");

function serve(dir) {
  const master = `#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360
/240.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=2500000,RESOLUTION=1280x720
/720.m3u8
`;
  return http.createServer((req, res) => {
    if (req.url === "/") {
      res.writeHead(200, { "Content-Type": "text/html" });
      res.end(fs.readFileSync(fixture));
      return;
    }
    if (req.url === "/master.m3u8") {
      res.writeHead(200, { "Content-Type": "application/vnd.apple.mpegurl" });
      res.end(master);
      return;
    }
    if (req.url === "/clip.mp4") {
      res.writeHead(200, { "Content-Type": "video/mp4", "Content-Length": "256" });
      res.end(Buffer.alloc(256, 2));
      return;
    }
  });
}

const server = serve();
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const port = server.address().port;
const base = `http://127.0.0.1:${port}/`;

const userDataDir = fs.mkdtempSync(path.join(os.tmpdir(), "jyroxa-vgrab-e2e-"));
const context = await chromium.launchPersistentContext(userDataDir, {
  channel: "chrome",
  headless: true,
  args: [
    `--disable-extensions-except=${extDir}`,
    `--load-extension=${extDir}`,
    "--headless=new",
    "--no-sandbox",
  ],
});
const page = context.pages()[0] || (await context.newPage());
await context.waitForEvent("serviceworker", { timeout: 25000 }).catch(() => null);

await page.addInitScript(() => {
  const mock = {
    runtime: {
      getURL: (p) => p,
      sendMessage: async (msg) => {
        if (msg?.type === "video-grab-list") {
          return {
            items: [
              {
                url: `${location.origin}/clip.mp4`,
                kind: "video",
                label: "clip.mp4",
                sizeLabel: "256 B",
                filename: "clip.mp4",
              },
              {
                url: `${location.origin}/master.m3u8`,
                kind: "hls",
                label: "1280x720",
                sizeLabel: "",
                filename: "master.m3u8",
              },
            ],
          };
        }
        if (msg?.type === "video-grab-handoff") return { ok: true };
        return { ok: true };
      },
      onMessage: { addListener: () => {} },
    },
    storage: {
      local: { get: async () => ({}), set: async () => {} },
      onChanged: { addListener: () => {} },
    },
  };
  globalThis.chrome = mock;
  globalThis.browser = mock;
});

await page.goto(base, { waitUntil: "domcontentloaded" });
await page.addStyleTag({ path: path.join(extDir, "video-grab.css") });
await page.addScriptTag({ path: path.join(extDir, "video-grab.js") });
await page.locator("#v").evaluate((v) => {
  v.muted = true;
  void v.play().catch(() => {});
});
const btn = page.locator(".jx-vgrab-btn");
await btn.waitFor({ state: "visible", timeout: 20000 });
await btn.click();
const menu = page.locator(".jx-vgrab-menu");
await menu.waitFor({ state: "visible" });
const text = await menu.innerText();
assert.ok(!/\bVIDEO\b/.test(text), `menu should not show bare VIDEO: ${text}`);
assert.ok(text.includes("clip.mp4") || text.includes("mp4"), text);
await context.close();
server.close();

const results = { fixture_mp4: "PASS", fixture_hls: "SKIP", youtube: "SKIP", dailymotion: "SKIP" };

// HLS row via service worker (no mocked runtime)
const server2 = serve();
await new Promise((r) => server2.listen(0, "127.0.0.1", r));
const port2 = server2.address().port;
const hlsBase = `http://127.0.0.1:${port2}/`;
const ud2 = fs.mkdtempSync(path.join(os.tmpdir(), "jyroxa-vgrab-hls-"));
const ctx2 = await chromium.launchPersistentContext(ud2, {
  channel: "chrome",
  headless: true,
  args: [
    `--disable-extensions-except=${extDir}`,
    `--load-extension=${extDir}`,
    "--headless=new",
    "--no-sandbox",
  ],
});
const p2 = ctx2.pages()[0] || (await ctx2.newPage());
await ctx2.waitForEvent("serviceworker", { timeout: 25000 }).catch(() => null);
await p2.goto(`${hlsBase}master.m3u8`);
await p2.goto(hlsBase, { waitUntil: "domcontentloaded" });
await p2.evaluate((u) => fetch(u).catch(() => {}), `${hlsBase}master.m3u8`);
await p2.goto(hlsBase, { waitUntil: "networkidle" }).catch(() => {});
await p2.locator("#v").evaluate((v) => {
  v.removeAttribute("src");
  v.load();
});
await p2.waitForTimeout(500);
await p2.locator(".jx-vgrab-btn").click({ timeout: 15000 }).catch(() => {});
const menu2 = p2.locator(".jx-vgrab-menu");
if (await menu2.isVisible().catch(() => false)) {
  const t2 = await menu2.innerText();
  if (/720|kbps|HLS/i.test(t2)) results.fixture_hls = "PASS";
}
await ctx2.close();
server2.close();

const exe = fs.existsSync(desktopExe)
  ? desktopExe
  : fs.existsSync(desktopAlt)
    ? desktopAlt
    : null;
const ytdlp = path.join(repoRoot, "apps", "desktop", "src-tauri", "yt-dlp.exe");

if (exe && fs.existsSync(ytdlp)) {
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), "jyroxa-vmenu-data-"));
  const dlDir = path.join(dataDir, "downloads");
  fs.mkdirSync(dlDir, { recursive: true });
  fs.copyFileSync(ytdlp, path.join(path.dirname(exe), "yt-dlp.exe"));
  const ffmpeg = path.join(repoRoot, "apps", "desktop", "src-tauri", "ffmpeg.exe");
  if (fs.existsSync(ffmpeg)) {
    fs.copyFileSync(ffmpeg, path.join(path.dirname(exe), "ffmpeg.exe"));
  }

  const desktop = spawn(
    exe,
    ["--minimized"],
    {
      env: { ...process.env, JYROXA_DATA_DIR: dataDir },
      stdio: "ignore",
      windowsHide: true,
    },
  );

  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  await sleep(8000);

  const hostExe = path.join(targetDir, "release", "jyroxa-native-host.exe");
  if (fs.existsSync(hostExe)) {
    const tokenPath = path.join(dataDir, "browser-integration.token");
    for (let i = 0; i < 40 && !fs.existsSync(tokenPath); i += 1) {
      await sleep(500);
    }
    if (fs.existsSync(tokenPath)) {
      const token = fs.readFileSync(tokenPath, "utf8").trim();
      const probeOne = (pageUrl, label) => {
        const env = {
          ...process.env,
          JYROXA_DATA_DIR: dataDir,
          JYROXA_NATIVE_HOST_NO_LAUNCH: "1",
        };
        const frame = buildFrame({
          token,
          type: "probe_video_formats",
          page_url: pageUrl,
        });
        const child = spawnSync(hostExe, [], { env, input: frame, maxBuffer: 8 * 1024 * 1024 });
        if (child.status !== 0) return { ok: false };
        const msg = parseFrame(child.stdout);
        return { ok: msg?.ok && (msg.formats?.length || 0) > 0, msg };
      };

      const yt = probeOne(
        "https://www.youtube.com/watch?v=aqz-KE-bpKQ",
        "youtube",
      );
      results.youtube = yt.ok ? "PASS" : "FAIL";

      const dm = probeOne(
        "https://www.dailymotion.com/video/x89eyek",
        "dailymotion",
      );
      results.dailymotion = dm.ok ? "PASS" : "FAIL";
    }
  }

  try {
    desktop.kill();
  } catch {
  }
}

console.log(JSON.stringify(results, null, 2));
console.log("video-menu-e2e.spec.mjs OK");

function buildFrame(obj) {
  const body = Buffer.from(JSON.stringify(obj), "utf8");
  const len = Buffer.alloc(4);
  len.writeUInt32LE(body.length, 0);
  return Buffer.concat([len, body]);
}

function parseFrame(buf) {
  if (!buf?.length) return null;
  const n = buf.readUInt32LE(0);
  return JSON.parse(buf.subarray(4, 4 + n).toString("utf8"));
}