#!/usr/bin/env node
/**
 * Headless design-v3 screenshots (Playwright + vite preview). No visible window.
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..", "..");
const outDir = path.join(repoRoot, "reports", "design-v3");
const previewUrl = process.env.DESIGN_PREVIEW_URL || "http://127.0.0.1:4173/";

fs.mkdirSync(outDir, { recursive: true });

const SAMPLE_CARD = `
<article class="dl-card" data-id="9001">
  <div class="dl-main-row">
    <div class="dl-check-col"><input type="checkbox" class="form-checkbox dl-select-checkbox" aria-label="Select" /></div>
    <div class="dl-icon-col cat-video"></div>
    <div class="dl-content-col">
      <div class="dl-title-row">
        <span class="dl-name">ubuntu-24.04.1-desktop-amd64.iso</span>
        <div class="dl-pills-wrap">
          <span class="category-pill cat-video">Video</span>
          <span class="status-pill status-downloading">Downloading</span>
        </div>
      </div>
      <div class="dl-progress-row">
        <div class="progress-track"><span class="progress-fill" style="width:42%"></span></div>
        <span class="progress-pct">42%</span>
      </div>
      <div class="dl-stats-row">
        <span class="stat-item stat-size">2.1 GB / 5.0 GB</span>
        <span class="stat-divider">·</span>
        <span class="stat-item stat-speed">12.4 MB/s</span>
      </div>
    </div>
    <div class="dl-actions-col"></div>
  </div>
</article>
<article class="dl-card" data-id="9002">
  <div class="dl-main-row">
    <div class="dl-check-col"><input type="checkbox" class="form-checkbox dl-select-checkbox" aria-label="Select" /></div>
    <div class="dl-icon-col cat-documents"></div>
    <div class="dl-content-col">
      <div class="dl-title-row">
        <span class="dl-name">project-release-notes.pdf</span>
        <div class="dl-pills-wrap">
          <span class="category-pill cat-documents">Documents</span>
          <span class="status-pill status-completed">Completed</span>
        </div>
      </div>
      <div class="dl-progress-row">
        <div class="progress-track"><span class="progress-fill" style="width:100%"></span></div>
        <span class="progress-pct">100%</span>
      </div>
      <div class="dl-stats-row"><span class="stat-item">1.2 MB</span></div>
    </div>
    <div class="dl-actions-col"></div>
  </div>
</article>`;

async function applyTheme(page, theme) {
  await page.evaluate((t) => {
    document.documentElement.setAttribute("data-theme", t);
    document.body.setAttribute("data-theme", t);
  }, theme);
}

async function seedMainList(page) {
  await page.evaluate((html) => {
    const list = document.getElementById("download-list");
    const empty = document.getElementById("download-empty");
    if (!list) return;
    list.innerHTML = html;
    empty?.classList.add("hidden");
  }, SAMPLE_CARD);
}

async function assertReadable(page, label) {
  const bad = await page.evaluate(() => {
    const card = document.querySelector(".dl-card");
    const btn = document.getElementById("btn-settings");
  if (!card || !btn) return "missing elements";
    const cardStyle = getComputedStyle(card);
    const btnStyle = getComputedStyle(btn);
    const parse = (bg, fg) => {
      const m = (s) => s.match(/[\d.]+/g)?.map(Number) ?? [128, 128, 128];
      const b = m(bg);
      const f = m(fg);
      const lum = (c) => 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
      return Math.abs(lum(b) - lum(f)) < 40;
    };
    if (parse(cardStyle.backgroundColor, cardStyle.color)) return "low card contrast";
    if (parse(btnStyle.backgroundColor, btnStyle.color)) return "low button contrast";
    return null;
  });
  if (bad) throw new Error(`${label}: ${bad}`);
}

async function shot(page, name) {
  await page.screenshot({ path: path.join(outDir, name), fullPage: false });
}

async function captureSet(page, theme) {
  const prefix = `after-${theme}`;
  await page.goto(previewUrl, { waitUntil: "networkidle", timeout: 60_000 });
  await applyTheme(page, theme);
  await seedMainList(page);
  await assertReadable(page, `${prefix}-main`);
  await shot(page, `${prefix}-main.png`);

  await page.locator("#btn-add").click();
  await page.waitForSelector("#dlg-add[open]");
  await assertReadable(page, `${prefix}-add`);
  await shot(page, `${prefix}-add.png`);
  await page.evaluate(() => document.getElementById("dlg-add")?.close());

  await page.locator("#btn-grabber").click();
  await page.waitForSelector("#grabber-panel:not(.hidden)");
  await assertReadable(page, `${prefix}-linkgrabber`);
  await shot(page, `${prefix}-linkgrabber.png`);

  await page.locator("#btn-settings").click();
  await page.waitForSelector("#settings-panel:not(.hidden)");
  await assertReadable(page, `${prefix}-settings`);
  await shot(page, `${prefix}-settings.png`);

  const sidebarClose = page.locator("#sidebar-close");
  if (await sidebarClose.isVisible()) await sidebarClose.click();
}

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({
  viewport: { width: 1180, height: 760 },
  deviceScaleFactor: 1,
});
const page = await context.newPage();

await captureSet(page, "light");
await captureSet(page, "dark");

await browser.close();
console.log(`Wrote screenshots to ${outDir}`);
