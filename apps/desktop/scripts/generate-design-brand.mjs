import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Resvg } from "@resvg/resvg-js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const brand = path.join(root, "assets/brand/v3");
fs.mkdirSync(brand, { recursive: true });
// A continuous square path forms a J, with its open top becoming a download arrow.
const shape = '<path d="M11 2h3v6h3l-4.5 5L8 8h3V2ZM2 4h6v3H5v7h9v-2h3v5H2V4Z"/>';
const svg = color => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 19 19" fill="${color}">${shape}</svg>`;
const light = svg("#7a5c2e");
const dark = svg("#c9a46a");
fs.writeFileSync(path.join(brand, "jyroxa.svg"), light);
fs.writeFileSync(path.join(brand, "jyroxa-dark.svg"), dark);
fs.writeFileSync(path.join(brand, "jyroxa-mono.svg"), svg("currentColor"));
const tray = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 19 19"><path fill="#f4f0eb" d="M0 0h19v19H0z"/><g fill="#7a5c2e">${shape}</g></svg>`;
fs.writeFileSync(path.join(brand, "jyroxa-tray.svg"), tray);
const darkApp = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 19 19"><path fill="#14100d" d="M0 0h19v19H0z"/><g fill="#c9a46a">${shape}</g></svg>`;
fs.writeFileSync(path.join(brand, "jyroxa-app.svg"), tray);
fs.writeFileSync(path.join(brand, "jyroxa-app-dark.svg"), darkApp);
const png = (source, size, fonts = false) =>
  new Resvg(source, { fitTo: { mode: "width", value: size }, font: { loadSystemFonts: fonts } }).render().asPng();
const sizes = [16, 20, 24, 32, 48, 64, 128, 256, 512, 1024];
for (const size of sizes) {
  fs.writeFileSync(path.join(brand, `jyroxa-${size}.png`), png(light, size));
  fs.writeFileSync(path.join(brand, `jyroxa-dark-${size}.png`), png(dark, size));
}
function ico(entries) {
  const header = Buffer.alloc(6 + entries.length * 16);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(entries.length, 4);
  let offset = header.length;
  entries.forEach(([size, data], index) => {
    const start = 6 + index * 16;
    header[start] = header[start + 1] = size === 256 ? 0 : size;
    header.writeUInt16LE(1, start + 4);
    header.writeUInt16LE(32, start + 6);
    header.writeUInt32LE(data.length, start + 8);
    header.writeUInt32LE(offset, start + 12);
    offset += data.length;
  });
  return Buffer.concat([header, ...entries.map(([, data]) => data)]);
}
const icon = ico([16, 24, 32, 48, 64, 128, 256].map(size => [size, png(tray, size)]));
fs.writeFileSync(path.join(brand, "jyroxa.ico"), icon);
const desktop = path.join(root, "apps/desktop/src-tauri/icons");
const originals = path.join(brand, "previous");
fs.mkdirSync(originals, { recursive: true });
function replace(file, data) {
  const relative = path.relative(root, file).replaceAll(path.sep, "_");
  const old = path.join(originals, relative);
  if (fs.existsSync(file) && !fs.existsSync(old)) fs.copyFileSync(file, old);
  fs.writeFileSync(file, data);
}
replace(path.join(root, "apps/desktop/src/assets/logo.svg"), light);
replace(path.join(root, "apps/desktop/src/assets/logo-dark.svg"), dark);
replace(path.join(desktop, "icon.ico"), icon);
for (const [name, size] of [["32x32.png",32],["64x64.png",64],["128x128.png",128],["128x128@2x.png",256],["icon.png",512]]) {
  replace(path.join(desktop, name), png(tray, size));
}
replace(path.join(desktop, "tray.png"), png(tray,32));
fs.writeFileSync(path.join(brand, "jyroxa-tray.png"), png(tray,32));
for (const name of fs.readdirSync(desktop)) {
  const square = name.match(/^Square(\d+)x\d+Logo\.png$/);
  if (square || name === "StoreLogo.png") replace(path.join(desktop,name),png(tray,square?Number(square[1]):50));
}
const chunks = [["ic07",128],["ic08",256],["ic09",512],["ic10",1024]].map(([type,size]) => {
  const data = png(tray,size), header=Buffer.alloc(8);
  header.write(type);header.writeUInt32BE(data.length+8,4);
  return Buffer.concat([header,data]);
});
const icnsHeader=Buffer.alloc(8);icnsHeader.write("icns");icnsHeader.writeUInt32BE(8+chunks.reduce((sum,chunk)=>sum+chunk.length,0),4);
replace(path.join(desktop,"icon.icns"),Buffer.concat([icnsHeader,...chunks]));
for (const size of [16,32,48,128]) replace(path.join(root, `extensions/browser/icons/icon-${size}.png`), png(tray,size));
// The Windows installer artwork uses the same flat mark. Preserve the previous bitmaps.
function bitmap(width, height) {
  const stride = (width * 3 + 3) & ~3;
  const pixels = Buffer.alloc(stride * height, 255);
  const markSize = Math.min(width - 32, height - 28, 72);
  const rendered = new Resvg(light, { fitTo: { mode: "width", value: markSize }, font: { loadSystemFonts: false } }).render();
  const rgba = rendered.pixels;
  const left = 16, top = 14;
  for (let y = 0; y < markSize; y++) for (let x = 0; x < markSize; x++) {
    const src = (y * markSize + x) * 4;
    const dst = (height - top - y - 1) * stride + (left + x) * 3;
    const alpha = rgba[src + 3] / 255;
    for (let c = 0; c < 3; c++) pixels[dst + c] = Math.round(rgba[src + 2 - c] * alpha + 255 * (1 - alpha));
  }
  const header = Buffer.alloc(54);
  header.write("BM"); header.writeUInt32LE(54 + pixels.length, 2); header.writeUInt32LE(54, 10);
  header.writeUInt32LE(40, 14); header.writeInt32LE(width, 18); header.writeInt32LE(height, 22);
  header.writeUInt16LE(1, 26); header.writeUInt16LE(24, 28); header.writeUInt32LE(pixels.length, 34);
  return Buffer.concat([header, pixels]);
}
replace(path.join(desktop, "header.bmp"), bitmap(150,57));
replace(path.join(desktop, "sidebar.bmp"), bitmap(164,314));

const brandRoot = path.join(root, "assets/brand");
const pngDir = path.join(brandRoot, "png");
fs.mkdirSync(pngDir, { recursive: true });
const markBody = shape;
fs.writeFileSync(path.join(brandRoot, "jyroxa-mark.svg"), light);
fs.writeFileSync(path.join(brandRoot, "jyroxa-mark-on-dark.svg"), dark);
fs.writeFileSync(path.join(brandRoot, "jyroxa-mark-mono.svg"), svg("#FFFFFF"));
fs.writeFileSync(path.join(brandRoot, "jyroxa-icon-light.svg"), tray);
fs.writeFileSync(path.join(brandRoot, "jyroxa-icon-dark.svg"), darkApp);
fs.writeFileSync(path.join(brandRoot, "jyroxa-tray.svg"), tray);
const androidFg = `<svg xmlns="http://www.w3.org/2000/svg" width="108" height="108" viewBox="0 0 108 108"><g transform="translate(21 21) scale(3.47368421)" fill="#7a5c2e">${markBody}</g></svg>`;
const androidMono = `<svg xmlns="http://www.w3.org/2000/svg" width="108" height="108" viewBox="0 0 108 108"><g transform="translate(21 21) scale(3.47368421)" fill="#000000">${markBody}</g></svg>`;
fs.writeFileSync(path.join(brandRoot, "jyroxa-android-foreground.svg"), androidFg);
fs.writeFileSync(path.join(brandRoot, "jyroxa-android-monochrome.svg"), androidMono);
const wordmarkSvg = (fg, bg) =>
  `<svg xmlns="http://www.w3.org/2000/svg" width="520" height="128" viewBox="0 0 520 128"><rect width="520" height="128" fill="${bg}"/><g transform="translate(20 22) scale(4.6)" fill="${fg}">${markBody}</g><text x="116" y="84" font-family="Segoe UI,system-ui,sans-serif" font-size="60" font-weight="700" fill="${fg}">Jyroxa</text></svg>`;
fs.writeFileSync(path.join(brandRoot, "jyroxa-wordmark.svg"), wordmarkSvg("#3d2f1a", "#f4f0eb"));
fs.writeFileSync(path.join(brandRoot, "jyroxa-wordmark-light.svg"), wordmarkSvg("#3d2f1a", "#f4f0eb"));
fs.writeFileSync(path.join(brandRoot, "jyroxa-wordmark-dark.svg"), wordmarkSvg("#f3eadf", "#14100d"));
const pubSizes = [1024, 512, 256, 128, 64, 48, 32, 24, 16];
for (const sz of pubSizes) {
  const iconLight = sz <= 24 ? tray : tray;
  fs.writeFileSync(path.join(pngDir, `jyroxa-icon-light-${sz}.png`), png(iconLight, sz));
  fs.writeFileSync(path.join(pngDir, `jyroxa-icon-dark-${sz}.png`), png(darkApp, sz));
  fs.writeFileSync(path.join(pngDir, `jyroxa-mark-${sz}.png`), png(light, sz));
}
for (const sz of [16, 32, 64]) fs.writeFileSync(path.join(pngDir, `jyroxa-tray-${sz}.png`), png(tray, sz));
fs.writeFileSync(path.join(pngDir, "jyroxa-wordmark.png"), png(fs.readFileSync(path.join(brandRoot, "jyroxa-wordmark.svg"), "utf8"), 1200, true));
fs.writeFileSync(path.join(pngDir, "jyroxa-wordmark-dark.png"), png(fs.readFileSync(path.join(brandRoot, "jyroxa-wordmark-dark.svg"), "utf8"), 1200, true));
fs.writeFileSync(path.join(pngDir, "jyroxa.ico"), icon);
fs.writeFileSync(path.join(pngDir, "jyroxa-tray.ico"), ico([16, 24, 32].map(size => [size, png(tray, size)])));
const previewBody = `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="760" viewBox="0 0 1200 760">
<rect width="1200" height="760" fill="#f4f0eb"/>
<rect x="40" y="40" width="260" height="260" rx="24" fill="#ffffff"/>
<rect x="330" y="40" width="260" height="260" rx="24" fill="#14100d"/>
<g transform="translate(95 95) scale(12.6)" fill="#7a5c2e">${markBody}</g>
<g transform="translate(385 95) scale(12.6)" fill="#c9a46a">${markBody}</g>
<text x="640" y="96" font-family="Segoe UI,system-ui,sans-serif" font-size="36" font-weight="650" fill="#3d2f1a">Jyroxa</text>
${[
  [640, 140, 13.5],
  [910, 160, 6.8],
  [780, 340, 3.4],
  [880, 360, 2.5],
  [960, 380, 1.7],
  [1005, 395, 1.25],
  [1035, 405, 0.85],
].map(([x, y, scale], i) => `<g transform="translate(${x} ${y}) scale(${scale})" fill="#7a5c2e">${markBody}</g>`).join("")}
</svg>`;
fs.writeFileSync(path.join(brandRoot, "preview.png"), png(previewBody, 1200, true));

const androidDensities = { mdpi: 48, hdpi: 72, xhdpi: 96, xxhdpi: 144, xxxhdpi: 192 };
for (const [density, size] of Object.entries(androidDensities)) {
  const launcher = png(tray, size);
  for (const base of [
    path.join(root, "android/app/src/main/res", `mipmap-${density}`),
    path.join(desktop, "android", `mipmap-${density}`),
  ]) {
    if (!fs.existsSync(base)) continue;
    replace(path.join(base, "ic_launcher.png"), launcher);
    replace(path.join(base, "ic_launcher_round.png"), launcher);
    const fg = path.join(base, "ic_launcher_foreground.png");
    if (fs.existsSync(fg)) replace(fg, png(light, size));
  }
}

console.log(
  `Wrote ${sizes.length * 2} PNG variants, SVG sources, multi-resolution ICO, desktop, installer, tray, browser, assets/brand/png, Android mipmaps.`,
);
