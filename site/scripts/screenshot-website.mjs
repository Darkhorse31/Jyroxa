import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import http from "node:http";
import { chromium } from "playwright";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const siteRoot = path.resolve(__dirname, "..");
const repoRoot = path.resolve(siteRoot, "..");
const outDir = path.join(repoRoot, "reports", "website");
fs.mkdirSync(outDir, { recursive: true });

const mime = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
};

function serveFile(filePath, res) {
  const ext = path.extname(filePath);
  const type = mime[ext] || "application/octet-stream";
  res.writeHead(200, { "Content-Type": type });
  fs.createReadStream(filePath).pipe(res);
}

const server = http.createServer((req, res) => {
  let urlPath = decodeURIComponent(req.url.split("?")[0]);
  if (urlPath === "/") urlPath = "/index.html";
  const safe = path.normalize(urlPath).replace(/^(\.\.(\/|\\|$))+/, "");
  const filePath = path.join(siteRoot, safe);
  if (!filePath.startsWith(siteRoot) || !fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
    res.writeHead(404);
    res.end("Not found");
    return;
  }
  serveFile(filePath, res);
});

await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const port = server.address().port;
const base = `http://127.0.0.1:${port}/`;

const viewports = [
  { name: "375", width: 375, height: 812 },
  { name: "1440", width: 1440, height: 900 },
];

const themes = ["dark", "light"];

const browser = await chromium.launch({ headless: true });

for (const theme of themes) {
  const context = await browser.newContext();
  await context.addInitScript((t) => {
    localStorage.setItem("jyroxa-site-theme", t);
    document.documentElement.setAttribute("data-theme", t);
  }, theme);
  const page = await context.newPage();

  for (const vp of viewports) {
    await page.setViewportSize({ width: vp.width, height: vp.height });
    await page.goto(base, { waitUntil: "networkidle" });
    await page.screenshot({
      path: path.join(outDir, `home-${vp.name}-${theme}.png`),
      fullPage: true,
    });
    if (theme === "dark") {
      fs.copyFileSync(
        path.join(outDir, `home-${vp.name}-dark.png`),
        path.join(outDir, `home-${vp.name}.png`)
      );
    }
  }

  await context.close();
}

await browser.close();
server.close();
console.log(`Screenshots written to ${outDir}`);