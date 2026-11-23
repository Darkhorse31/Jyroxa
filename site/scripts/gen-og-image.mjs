import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const siteRoot = path.resolve(__dirname, "..");
const out = path.join(siteRoot, "assets", "og-image.png");

const logo = fs.readFileSync(path.join(siteRoot, "assets", "logo.svg"), "utf8");
const hero = fs
  .readFileSync(path.join(siteRoot, "assets", "dark-main.png"))
  .toString("base64");

const html = `<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"></head>
<body style="margin:0;width:1200px;height:630px;display:flex;align-items:center;gap:36px;padding:0 48px;box-sizing:border-box;background:#14100d;font-family:Segoe UI,system-ui,sans-serif;color:#f3eadf;">
  <div style="flex:0 0 380px;display:flex;flex-direction:column;align-items:flex-start;">
    <div style="width:96px;height:96px;margin-bottom:24px;">${logo.replace("<svg", '<svg style="width:100%;height:100%"')}</div>
    <div style="font-size:52px;font-weight:650;letter-spacing:0.02em;margin-bottom:12px;">Jyroxa</div>
    <div style="font-size:24px;color:#b8a898;line-height:1.45;max-width:360px;">Open-source download manager for Windows and Android</div>
  </div>
  <div style="flex:1;display:flex;justify-content:center;align-items:center;">
    <img src="data:image/png;base64,${hero}" width="680" height="425" alt="" style="border-radius:12px;box-shadow:0 18px 48px rgba(0,0,0,0.45);border:1px solid #3a2f28;object-fit:cover;object-position:top;" />
  </div>
</body>
</html>`;

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1200, height: 630 } });
await page.setContent(html, { waitUntil: "networkidle" });
await page.screenshot({ path: out });
await browser.close();
console.log(`Wrote ${out}`);