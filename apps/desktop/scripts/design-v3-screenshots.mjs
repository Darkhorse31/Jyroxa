import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "..", "..", "..");
const outDirs = [
  path.join(repoRoot, "reports", "design-v3"),
  "C:/Users/lenovo/.jyroxa-tools/reports/design-v3"
];

for (const dir of outDirs) {
  fs.mkdirSync(dir, { recursive: true });
}

const previewUrl = process.env.DESIGN_PREVIEW_URL || "http://127.0.0.1:4173/";

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({
  viewport: { width: 1280, height: 800 },
  deviceScaleFactor: 1
});

await context.addInitScript(() => {
  const downloads = [
    {
      id: 1,
      urls: ["https://releases.ubuntu.com/24.04/ubuntu-24.04-desktop-amd64.iso"],
      filename: "ubuntu-24.04-desktop-amd64.iso",
      output: "C:\\Downloads\\ubuntu-24.04-desktop-amd64.iso",
      connections: 8,
      connections_auto: true,
      sha256: "ed24b8ec80dd3a54d3345862d51fb7de5462cf69c73523f2f8489c67fc7f40cf",
      status: "downloading",
      downloaded: 2516582400,
      total: 5033164800,
      speed_bps: 15500000,
      segments: [
        { index: 0, start: 0, end: 629145600, downloaded: 629145600 },
        { index: 1, start: 629145600, end: 1258291200, downloaded: 629145600 },
        { index: 2, start: 1258291200, end: 1887436800, downloaded: 629145600 },
        { index: 3, start: 1887436800, end: 2516582400, downloaded: 629145600 },
        { index: 4, start: 2516582400, end: 3145728000, downloaded: 300000000 },
        { index: 5, start: 3145728000, end: 3774873600, downloaded: 120000000 },
        { index: 6, start: 3774873600, end: 4404019200, downloaded: 0 },
        { index: 7, start: 4404019200, end: 5033164800, downloaded: 0 }
      ]
    },
    {
      id: 2,
      urls: ["https://download.blender.org/release/Blender4.2/blender-4.2.0-windows-x64.msi"],
      filename: "blender-4.2.0-windows-x64.msi",
      output: "C:\\Downloads\\Programs\\blender-4.2.0-windows-x64.msi",
      connections: 8,
      connections_auto: true,
      status: "completed",
      downloaded: 345000000,
      total: 345000000,
      speed_bps: 0,
      segments: []
    },
    {
      id: 3,
      urls: ["https://nodejs.org/dist/v22.11.0/node-v22.11.0-x64.msi"],
      filename: "node-v22.11.0-x64.msi",
      output: "C:\\Downloads\\Programs\\node-v22.11.0-x64.msi",
      connections: 4,
      connections_auto: false,
      status: "paused",
      downloaded: 15000000,
      total: 35000000,
      speed_bps: 0,
      segments: []
    },
    {
      id: 4,
      urls: ["https://example.com/damaged-archive.tar.gz"],
      filename: "damaged-archive.tar.gz",
      output: "C:\\Downloads\\Compressed\\damaged-archive.tar.gz",
      connections: 2,
      connections_auto: true,
      status: "failed",
      downloaded: 4096000,
      total: 50000000,
      speed_bps: 0,
      error: "HTTP 502 Bad Gateway from origin server",
      segments: []
    },
    {
      id: 5,
      urls: ["https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.11.tar.xz"],
      filename: "linux-6.11.tar.xz",
      output: "C:\\Downloads\\Compressed\\linux-6.11.tar.xz",
      connections: 8,
      connections_auto: true,
      status: "queued",
      downloaded: 0,
      total: 145000000,
      speed_bps: 0,
      segments: []
    }
  ];

  const settings = {
    max_concurrent: 4,
    max_connections_per_download: 8,
    per_host_cap: 8,
    global_speed_limit: null,
    download_dir: "C:\\Downloads",
    default_folder: "C:\\Downloads",
    category_dirs: {
      Video: "C:\\Downloads\\Video",
      Music: "C:\\Downloads\\Music",
      Documents: "C:\\Downloads\\Documents",
      Compressed: "C:\\Downloads\\Compressed",
      Programs: "C:\\Downloads\\Programs",
      Other: "C:\\Downloads\\Other"
    },
    clipboard_monitor: true,
    autotune: true,
    start_minimized: false,
    notifications: true,
    close_to_tray: true,
    start_with_windows: false,
    prevent_sleep: true,
    auto_resume: true,
    theme: "light",
    scheduler_start: null,
    scheduler_stop: null,
    scheduler_days: [1, 2, 3, 4, 5]
  };

  window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = window.__TAURI_EVENT_PLUGIN_INTERNALS__ || {};
  const callbacks = new Map();
  window.__TAURI_INTERNALS__.transformCallback = (callback, once = false) => {
    const id = Math.floor(Math.random() * 1000000);
    callbacks.set(id, callback);
    return id;
  };
  window.__TAURI_INTERNALS__.unregisterCallback = (id) => callbacks.delete(id);
  window.__TAURI_INTERNALS__.callbacks = callbacks;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener = () => {};

  window.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
    if (cmd === "plugin:app|version") return "0.1.2";
    if (cmd === "plugin:event|listen") return 1;
    if (cmd === "get_settings") return settings;
    if (cmd === "set_settings") { Object.assign(settings, args.settings); return; }
    if (cmd === "list_downloads") return downloads;
    if (cmd === "default_category_dirs") return settings.category_dirs;
    if (cmd === "suggest_save_path") return (args.dir || settings.download_dir) + "\\" + args.name;
    if (cmd === "probe_url") return {
      final_url: args.url,
      suggested_name: "ubuntu-24.04-desktop-amd64.iso",
      size: 5033164800,
      mime: "application/x-iso9660-image",
      resumable: true,
      category: "Other",
      server: "releases.ubuntu.com",
      protocol: "HTTP/2"
    };
    if (cmd === "probe_links") return args.urls.map(u => ({ url: u, filename: "package.zip", size: 104857600, mime: "application/zip", resumable: true }));
    if (cmd === "grab_links") return [{ name: "Ubuntu Release", links: [{ url: "https://releases.ubuntu.com/24.04/ubuntu-24.04-desktop-amd64.iso", filename: "ubuntu-24.04-desktop-amd64.iso", size: 5033164800, resumable: true }] }];
    return true;
  };
});

const page = await context.newPage();
await page.goto(previewUrl, { waitUntil: "networkidle" });
await page.waitForTimeout(600);

async function saveShot(name) {
  for (const dir of outDirs) {
    await page.screenshot({ path: path.join(dir, name), fullPage: false });
  }
}

async function setVisualTheme(theme) {
  await page.evaluate((t) => {
    document.documentElement.style.setProperty("--dur-fast", "0ms");
    document.documentElement.style.setProperty("--dur-normal", "0ms");
    document.documentElement.style.setProperty("--dur-smooth", "0ms");
    document.documentElement.setAttribute("data-theme", t);
    document.body.setAttribute("data-theme", t);
  }, theme);
  await page.waitForTimeout(300);
}

await setVisualTheme("dark");
await saveShot("after-dark-main.png");

await setVisualTheme("light");
await saveShot("after-light-main.png");

await setVisualTheme("dark");
await page.locator("#btn-add").click();
await page.waitForSelector("#dlg-add[open]");
await page.waitForTimeout(300);
await saveShot("after-dark-add.png");

await setVisualTheme("light");
await saveShot("after-light-add.png");

await page.evaluate(() => document.getElementById("add-close-btn")?.click());
await page.waitForTimeout(300);

await setVisualTheme("dark");
await page.locator("#btn-grabber").click();
await page.waitForSelector("#grabber-panel:not(.hidden)");
await page.waitForTimeout(300);
await saveShot("after-dark-linkgrabber.png");

await setVisualTheme("light");
await saveShot("after-light-linkgrabber.png");

await page.evaluate(() => document.getElementById("sidebar-close")?.click());
await page.waitForTimeout(300);

await setVisualTheme("dark");
await page.locator("#btn-settings").click();
await page.waitForSelector("#settings-panel:not(.hidden)");
await page.waitForTimeout(300);
await saveShot("after-dark-settings.png");

await setVisualTheme("light");
await saveShot("after-light-settings.png");

await page.evaluate(() => document.getElementById("sidebar-close")?.click());
await setVisualTheme("light");
await page.waitForTimeout(200);

await page.setViewportSize({ width: 1024, height: 768 });
await page.waitForTimeout(300);
await saveShot("responsive-1024.png");

await page.setViewportSize({ width: 1920, height: 1080 });
await page.waitForTimeout(300);
await saveShot("responsive-1920.png");

await page.setViewportSize({ width: 1280, height: 800 });

await browser.close();
