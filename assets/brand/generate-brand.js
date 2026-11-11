// Jyroxa brand v2 generator: flat Teal + Coral, no gradients.
const fs = require('fs'), path = require('path');
const { Resvg } = require('@resvg/resvg-js');
const opentype = require('opentype.js');
const OUT = process.argv[2] || 'E:/SwiftGet/assets/brand';
const C = { teal: '#0F766E', tealLight: '#2DD4BF', coral: '#FF5A3C', navy: '#0B1020', white: '#FFFFFF' };
const RX = 186, RY = 62, R = 112, ROT = -26, SW = 30, GAP = 50;
let uid = 0;
// Mark with real transparency (gaps cut by masks), drawn in 512 space.
function mark(ring, orbit, dot, sw = SW, gap = GAP) {
  const u = ++uid, c = 256, tr = `rotate(${ROT} ${c} ${c})`;
  const ell = (col, w) => `<ellipse cx="${c}" cy="${c}" rx="${RX}" ry="${RY}" fill="none" stroke="${col}" stroke-width="${w}"/>`;
  return `<defs>
<clipPath id="lo${u}"><rect x="-200" y="${c}" width="912" height="400"/></clipPath>
<mask id="mb${u}" maskUnits="userSpaceOnUse" x="0" y="0" width="512" height="512"><rect width="512" height="512" fill="#fff"/><circle cx="${c}" cy="${c}" r="${R}" fill="none" stroke="#000" stroke-width="${gap}"/></mask>
<mask id="mr${u}" maskUnits="userSpaceOnUse" x="0" y="0" width="512" height="512"><rect width="512" height="512" fill="#fff"/><g transform="${tr}"><g clip-path="url(#lo${u})">${ell('#000', gap)}</g></g></mask>
</defs>
<g mask="url(#mb${u})"><g transform="${tr}">${ell(orbit, sw)}</g></g>
<g mask="url(#mr${u})"><circle cx="${c}" cy="${c}" r="${R}" fill="none" stroke="${ring}" stroke-width="${sw}"/></g>
<g transform="${tr}"><g clip-path="url(#lo${u})">${ell(orbit, sw)}</g></g>
<circle cx="${c}" cy="${c}" r="${sw === SW ? 42 : 52}" fill="${dot}"/>`;
}
const svg = (w, h, body, vb) => `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="${vb || `0 0 ${w} ${h}`}">${body}</svg>`;
const tile = (bg) => `<rect width="512" height="512" rx="112" fill="${bg}"/>`;
const files = {};
files['jyroxa-mark.svg'] = svg(512, 512, mark(C.teal, C.coral, C.teal));                       // transparent, for light surfaces
files['jyroxa-mark-on-dark.svg'] = svg(512, 512, mark(C.tealLight, C.coral, C.tealLight));       // transparent, for dark surfaces
files['jyroxa-icon-light.svg'] = svg(512, 512, tile(C.white) + mark(C.teal, C.coral, C.teal));    // primary app icon
files['jyroxa-icon-dark.svg'] = svg(512, 512, tile(C.navy) + mark(C.tealLight, C.coral, C.tealLight));
files['jyroxa-mark-mono.svg'] = svg(512, 512, mark('#FFFFFF', '#FFFFFF', '#FFFFFF'));
// Tray: bolder strokes and no inner gap detail lost at 16px; white tile for contrast on any taskbar.
files['jyroxa-tray.svg'] = svg(512, 512, tile(C.white) + mark(C.teal, C.coral, C.teal, 46, 74));
// Android adaptive foreground: 108dp canvas, mark inside the 66dp safe zone.
const s = 66 / 430, off = (108 - 512 * s) / 2;
files['jyroxa-android-foreground.svg'] = svg(108, 108, `<g transform="translate(${off} ${off}) scale(${s})">${mark(C.teal, C.coral, C.teal)}</g>`);
files['jyroxa-android-monochrome.svg'] = svg(108, 108, `<g transform="translate(${off} ${off}) scale(${s})">${mark('#000000', '#000000', '#000000')}</g>`);
// Wordmark: mark + "Jyroxa" in Outfit Bold (SIL OFL), converted to paths.
const font = (() => { const b = fs.readFileSync('E:/.swiftget-tools/brandtool/Outfit-Bold.ttf'); return opentype.parse(b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength)); })();
function word(color) {
  const p = new opentype.Path(); let x = 0, prev = null; const k = 200 / font.unitsPerEm; for (const ch of 'Jyroxa') { const g = font.charToGlyph(ch); if (prev) { const kv = Number(font.getKerningValue(prev, g)); if (Number.isFinite(kv)) x += kv * k; } { const gp = g.getPath(0, 0, 200); for (const c of gp.commands) { const n = Object.assign({}, c); for (const kx of ['x','x1','x2']) if (n[kx] !== undefined) n[kx] += x; p.commands.push(n); } } x += g.advanceWidth * k; prev = g; } const bb = p.getBoundingBox();
  const h = bb.y2 - bb.y1, w = bb.x2 - bb.x1, scale = 300 / h;
  const tx = 460 - bb.x1 * scale, ty = 256 - (bb.y1 + h / 2) * scale;
  return { d: p.commands.map(c => { const f = v => (+v).toFixed(2); switch (c.type) { case 'M': case 'L': return c.type + f(c.x) + ' ' + f(c.y); case 'Q': return 'Q' + f(c.x1) + ' ' + f(c.y1) + ' ' + f(c.x) + ' ' + f(c.y); case 'C': return 'C' + f(c.x1) + ' ' + f(c.y1) + ' ' + f(c.x2) + ' ' + f(c.y2) + ' ' + f(c.x) + ' ' + f(c.y); default: return 'Z'; } }).join(''), tx, ty, scale, width: 460 + w * scale + 40 };
}
for (const [name, col, ring] of [['jyroxa-wordmark.svg', C.navy, C.teal], ['jyroxa-wordmark-light.svg', C.navy, C.teal], ['jyroxa-wordmark-dark.svg', C.white, C.tealLight]]) {
  const wd = word(col);
  files[name] = svg(Math.ceil(wd.width), 512, `<g transform="translate(16 56) scale(0.78)">${mark(ring, C.coral, ring)}</g><path d="${wd.d}" fill="${col}" transform="translate(${wd.tx} ${wd.ty}) scale(${wd.scale})"/>`);
}
fs.mkdirSync(path.join(OUT, 'png'), { recursive: true });
for (const f of fs.readdirSync(OUT)) if (f.endsWith('.svg')) fs.unlinkSync(path.join(OUT, f));
for (const [n, body] of Object.entries(files)) fs.writeFileSync(path.join(OUT, n), body);
const render = (svgText, w) => new Resvg(svgText, { fitTo: { mode: 'width', value: w }, font: { loadSystemFonts: false } }).render().asPng();
for (const f of fs.readdirSync(path.join(OUT, 'png'))) fs.unlinkSync(path.join(OUT, 'png', f));
for (const sz of [1024, 512, 256, 128, 64, 48, 32, 24, 16]) {
  fs.writeFileSync(path.join(OUT, 'png', `jyroxa-icon-light-${sz}.png`), render(sz <= 24 ? files['jyroxa-tray.svg'] : files['jyroxa-icon-light.svg'], sz));
  fs.writeFileSync(path.join(OUT, 'png', `jyroxa-icon-dark-${sz}.png`), render(files['jyroxa-icon-dark.svg'], sz));
  fs.writeFileSync(path.join(OUT, 'png', `jyroxa-mark-${sz}.png`), render(files['jyroxa-mark.svg'], sz));
}
for (const sz of [16, 32, 64]) fs.writeFileSync(path.join(OUT, 'png', `jyroxa-tray-${sz}.png`), render(files['jyroxa-tray.svg'], sz));
fs.writeFileSync(path.join(OUT, 'png', 'jyroxa-wordmark.png'), render(files['jyroxa-wordmark.svg'], 1200));
fs.writeFileSync(path.join(OUT, 'png', 'jyroxa-wordmark-dark.png'), render(files['jyroxa-wordmark-dark.svg'], 1200));
// ICO (PNG-compressed entries).
function ico(list) {
  const imgs = list.map(([sz, buf]) => ({ sz, buf })); const head = Buffer.alloc(6 + 16 * imgs.length);
  head.writeUInt16LE(0, 0); head.writeUInt16LE(1, 2); head.writeUInt16LE(imgs.length, 4);
  let off = head.length; imgs.forEach((im, i) => { const o = 6 + 16 * i; head.writeUInt8(im.sz >= 256 ? 0 : im.sz, o); head.writeUInt8(im.sz >= 256 ? 0 : im.sz, o + 1);
    head.writeUInt16LE(1, o + 4); head.writeUInt16LE(32, o + 6); head.writeUInt32LE(im.buf.length, o + 8); head.writeUInt32LE(off, o + 12); off += im.buf.length; });
  return Buffer.concat([head, ...imgs.map(i => i.buf)]);
}
const icoSizes = [16, 24, 32, 48, 64, 128, 256];
fs.writeFileSync(path.join(OUT, 'png', 'jyroxa.ico'), ico(icoSizes.map(sz => [sz, render(sz <= 24 ? files['jyroxa-tray.svg'] : files['jyroxa-icon-light.svg'], sz)])));
fs.writeFileSync(path.join(OUT, 'png', 'jyroxa-tray.ico'), ico([16, 24, 32].map(sz => [sz, render(files['jyroxa-tray.svg'], sz)])));
// Preview sheet.
const t = (x, y, w, s) => `<svg x="${x}" y="${y}" width="${w}" height="${w}" viewBox="0 0 512 512">${s.replace(/^<svg[^>]*>|<\/svg>$/g, '')}</svg>`;
const inner = (n) => files[n].replace(/^<svg[^>]*>|<\/svg>$/g, '');
const wl = word(C.navy), wdk = word(C.white);
let sheet = `<rect width="1200" height="760" fill="#F3F4F6"/>
<g fill="${C.navy}">${''}</g>
${t(40, 40, 260, files['jyroxa-icon-light.svg'])}${t(330, 40, 260, files['jyroxa-icon-dark.svg'])}
<rect x="620" y="40" width="260" height="260" rx="40" fill="${C.navy}"/>${t(650, 70, 200, files['jyroxa-mark-mono.svg'])}
<rect x="910" y="40" width="250" height="260" rx="40" fill="#FFFFFF"/>
${[64, 48, 32, 24, 16].map((sz, i) => t(930 + [0, 80, 140, 185, 220][i], 140 - sz / 2, sz, sz <= 24 ? files['jyroxa-tray.svg'] : files['jyroxa-icon-light.svg'])).join('')}
<rect x="40" y="340" width="1120" height="180" rx="40" fill="#FFFFFF"/>
<svg x="70" y="355" width="${Math.ceil(wl.width) * 150 / 512}" height="150" viewBox="0 0 ${Math.ceil(wl.width)} 512">${inner('jyroxa-wordmark.svg')}</svg>
<rect x="40" y="550" width="1120" height="180" rx="40" fill="${C.navy}"/>
<svg x="70" y="565" width="${Math.ceil(wdk.width) * 150 / 512}" height="150" viewBox="0 0 ${Math.ceil(wdk.width)} 512">${inner('jyroxa-wordmark-dark.svg')}</svg>
<rect x="760" y="390" width="80" height="80" rx="16" fill="${C.teal}"/><rect x="860" y="390" width="80" height="80" rx="16" fill="${C.tealLight}"/><rect x="960" y="390" width="80" height="80" rx="16" fill="${C.coral}"/><rect x="1060" y="390" width="80" height="80" rx="16" fill="${C.navy}"/>`;
fs.writeFileSync(path.join(OUT, 'preview.png'), render(svg(1200, 760, sheet), 1200));
console.log('brand v2 written to', OUT, Object.keys(files));









