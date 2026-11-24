/**
 * Video menu: local mp4/HLS fixture + optional yt-dlp probe via native host + desktop.
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { startEngine, existingChromiumExecutable } from "./headless-engine.mjs";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "../../..");
const extDir = path.resolve(__dirname, "..");
const targetDir =
  process.env.CARGO_TARGET_DIR || path.join(repoRoot, "target");
const fixture = path.join(__dirname, "fixtures", "video-test.html");

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
  executablePath: existingChromiumExecutable(),
  headless: true,
  env: {...process.env, JYROXA_NATIVE_HOST_NO_LAUNCH:"1", JYROXA_DATA_DIR:userDataDir},
  args: [
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
  executablePath: existingChromiumExecutable(),
  headless: true,
  env: {...process.env, JYROXA_NATIVE_HOST_NO_LAUNCH:"1", JYROXA_DATA_DIR:ud2},
  args: [
    `--disable-extensions-except=${extDir}`,
    `--load-extension=${extDir}`,
    "--headless=new",
    "--no-sandbox",
  ],
});
const p2 = ctx2.pages()[0] || (await ctx2.newPage());
await ctx2.waitForEvent("serviceworker", { timeout: 25000 }).catch(() => null);

await p2.goto(hlsBase, { waitUntil: "domcontentloaded" });
await p2.evaluate((u) => fetch(u).catch(() => {}), `${hlsBase}master.m3u8`);
await p2.locator("#v").evaluate((v) => {
  v.removeAttribute("src");
  v.load();
});
await p2.waitForTimeout(500);
await p2.locator(".jx-vgrab-btn").click({ timeout: 15000 });
const menu2 = p2.locator(".jx-vgrab-menu");
await menu2.waitFor({state:"visible"});
await p2.waitForFunction(() => /720|kbps|HLS/i.test(document.querySelector(".jx-vgrab-menu")?.textContent || ""));
results.fixture_hls = "PASS";
await ctx2.close();
server2.close();

const engine = await startEngine();
try {
  for (const [label, url] of [["youtube", "https://www.youtube.com/watch?v=jNQXAC9IVRw"], ["dailymotion", "https://www.dailymotion.com/video/x89eyek"]]) {
    const reply = engine.native({type:"probe_video_formats",page_url:url});
    assert.equal(reply.ok, true, JSON.stringify(reply));
    assert.ok(reply.formats.length > 0);
    results[label] = "PASS";
  }
} finally { await engine.close(); }
console.log(JSON.stringify(results, null, 2));
console.log("video-menu-e2e.spec.mjs OK");
