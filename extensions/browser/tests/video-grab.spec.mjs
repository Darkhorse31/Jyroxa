import assert from "node:assert/strict";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import os from "node:os";
import { chromium } from "playwright";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const extDir = path.resolve(__dirname, "..");
const fixture = path.join(__dirname, "fixtures", "video-test.html");

function serve(dir) {
  const master = `#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=800000
/media.m3u8
`;
  const media = `#EXTM3U
#EXTINF:1,
/seg0.bin
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
    if (req.url === "/media.m3u8") {
      res.writeHead(200, { "Content-Type": "application/vnd.apple.mpegurl" });
      res.end(media);
      return;
    }
    if (req.url === "/clip.mp4") {
      res.writeHead(200, { "Content-Type": "video/mp4", "Content-Length": "128" });
      res.end(Buffer.alloc(128, 1));
      return;
    }
    res.writeHead(404).end();
  });
}

const server = serve();
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const port = server.address().port;
const base = `http://127.0.0.1:${port}/`;

const userDataDir = fs.mkdtempSync(path.join(os.tmpdir(), "jyroxa-vgrab-"));
const context = await chromium.launchPersistentContext(userDataDir, {
  channel: "chromium",
  headless: true,
  env: {...process.env, JYROXA_NATIVE_HOST_NO_LAUNCH:"1", JYROXA_DATA_DIR:userDataDir},
  args: [
    "--headless=new",
    "--no-sandbox",
  ],
});
const page = context.pages()[0] || (await context.newPage());
await context.waitForEvent("serviceworker", { timeout: 20000 }).catch(() => null);

await page.addInitScript(() => {
  globalThis.__jxHandoffs = [];
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
                sizeLabel: "128 B",
                filename: "clip.mp4",
              },
              {
                url: `${location.origin}/master.m3u8`,
                kind: "hls",
                label: "HLS",
                sizeLabel: "",
                filename: "master.m3u8",
              },
              {
                url: `${location.origin}/manifest.mpd`,
                kind: "dash",
                label: "DASH",
                sizeLabel: "",
                filename: "manifest.mpd",
              },
            ],
          };
        }
        if (msg?.type === "video-grab-handoff") {
          globalThis.__jxHandoffs.push(msg.payload);
          return { ok: true };
        }
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
await btn.waitFor({ state: "visible", timeout: 15000 });
await btn.click();
const menu = page.locator(".jx-vgrab-menu");
await menu.waitFor({ state: "visible" });
const items = await page.locator(".jx-vgrab-item").count();
assert.ok(items >= 3, "expected MP4, HLS, and DASH media rows");
for (const [label, suffix] of [["clip.mp4", "/clip.mp4"], ["HLS", "/master.m3u8"], ["DASH", "/manifest.mpd"]]) {
  if (!(await menu.isVisible())) await btn.click();
  await page.locator(".jx-vgrab-item").filter({ hasText: label }).click();
  const handedOff = await page.evaluate(() => globalThis.__jxHandoffs.at(-1));
  assert.equal(handedOff.url, `${base.slice(0, -1)}${suffix}`);
  assert.equal(handedOff.page_url, base);
}
await context.close();
server.close();
console.log("video-grab.spec.mjs OK");
