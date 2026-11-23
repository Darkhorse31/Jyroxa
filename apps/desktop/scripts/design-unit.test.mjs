import assert from "node:assert/strict";
import fs from "node:fs";
import { test } from "node:test";
import { downloadSources, isDownloadSource, remainingTime } from "../src/sources.ts";

test("download entry accepts supported URLs, magnets and full torrent paths", () => {
  for (const source of ["https://example.org/a.zip", "http://example.org/a", "magnet:?xt=urn:btih:abc", "C:\\Downloads\\release.torrent", "\\\\server\\share\\release.torrent", "/tmp/release.torrent", "tg://resolve?domain=example"]) {
    assert.ok(isDownloadSource(source), source);
  }
  for (const source of ["", "http://", "https://bad host/file", "javascript:alert(1)", "random text", "release.torrent", "file:///C:/secret", "C:\\Downloads\\program.exe"]) assert.ok(!isDownloadSource(source), source);
  assert.deepEqual(downloadSources("# comment\r\nhttps://example.org/a\r\nmagnet:?xt=abc\ninvalid"), ["https://example.org/a", "magnet:?xt=abc"]);
});

test("time remaining is readable and handles unknown and invalid estimates", () => {
  assert.equal(remainingTime(22), "22 seconds left");
  assert.equal(remainingTime(1), "1 second left");
  assert.equal(remainingTime(60), "1 minute left");
  assert.equal(remainingTime(61), "2 minutes left");
  assert.equal(remainingTime(3600), "1 hour left");
  assert.equal(remainingTime(3601), "2 hours left");
  for (const value of [null, NaN, Infinity]) assert.equal(remainingTime(value), "Calculating time left");
});

function luminance(hex) {
  const channels = hex.replace("#", "").match(/../g).map(value => parseInt(value, 16) / 255).map(value => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4);
  return channels[0] * .2126 + channels[1] * .7152 + channels[2] * .0722;
}
function contrast(a, b) {
  const values = [luminance(a), luminance(b)].sort((x,y) => y-x);
  return (values[0] + .05) / (values[1] + .05);
}
test("actual light and dark text tokens meet WCAG AA contrast", () => {
  const css = fs.readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
  const [light, dark] = css.split(':root[data-theme="dark"]');
  for (const [name, block] of [["light", light], ["dark", dark]]) {
    const get = token => block.match(new RegExp(`--${token}:\\s*(#[a-fA-F0-9]{6})`))[1];
    for (const text of ["text-main", "text-secondary", "text-muted", "text-faint", "accent"]) {
      for (const background of ["bg-surface", "bg-app", "bg-card-inner", "bg-hover"]) {
        const ratio = contrast(get(text), get(background));
        assert.ok(ratio >= 4.5, `${name} ${text} / ${background}: ${ratio.toFixed(2)}`);
      }
    }
    const button = contrast(get("btn-primary-text"), get("accent"));
    assert.ok(button >= 4.5, `${name} primary button: ${button}`);
  }
});

test("Windows ICO includes every required resolution and old brand stays available", () => {
  const ico = fs.readFileSync(new URL("../../../assets/brand/v3/jyroxa.ico", import.meta.url));
  assert.equal(ico.readUInt16LE(2), 1);
  const count = ico.readUInt16LE(4);
  const sizes = Array.from({ length: count }, (_, i) => ico[6 + i * 16] || 256);
  assert.deepEqual(sizes, [16,24,32,48,64,128,256]);
  assert.ok(fs.existsSync(new URL("../../../assets/brand/jyroxa-mark.svg", import.meta.url)));
});
