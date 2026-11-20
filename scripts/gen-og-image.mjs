import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const siteRoot = path.resolve(__dirname, "..");
const out = path.join(siteRoot, "assets", "og-image.png");

const logo = fs.readFileSync(path.join(siteRoot, "assets", "logo.svg"), "utf8");
const html = `<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"></head>
<body style="margin:0;width:1200px;height:630px;display:flex;flex-direction:column;align-items:center;justify-content:center;background:#14100d;font-family:Segoe UI,system-ui,sans-serif;color:#f3eadf;text-align:center;">
  <div style="width:128px;height:128px;margin-bottom:28px;">${logo.replace("<svg", '<svg style="width:100%;height:100%"')}</div>
  <div style="font-size:56px;font-weight:650;letter-spacing:0.02em;margin-bottom:12px;">Jyroxa</div>
  <div style="font-size:28px;color:#b8a898;max-width:900px;line-height:1.4;">Open-source download manager for Windows and Android</div>
</body>
</html>`;

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1200, height: 630 } });
await page.setContent(html, { waitUntil: "networkidle" });
await page.screenshot({ path: out });
await browser.close();
console.log(`Wrote ${out}`);