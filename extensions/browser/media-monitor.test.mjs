import assert from "node:assert/strict";
import { recordMedia, listMediaForTab, mediaLabel, formatBytes } from "./media-monitor.js";

recordMedia({
  url: "https://cdn.example/clip.m3u8",
  tabId: 1,
  responseHeaders: [{ name: "Content-Type", value: "application/vnd.apple.mpegurl" }],
});

const items = listMediaForTab(1);
assert.equal(items.length, 1);
assert.equal(items[0].kind, "hls");
assert.match(mediaLabel(items[0]), /HLS|hls|kbps|\d/i);
assert.equal(formatBytes(1536), "1.5 KB");
console.log("media-monitor.test.mjs OK");