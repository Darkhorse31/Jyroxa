import assert from "node:assert/strict";
import { test } from "node:test";
import { isYoutubePage } from "../src/video-url.ts";
import fs from "node:fs";

test("recognizes YouTube watch, short, live and embed pages on exact hosts", () => {
  for (const url of ["https://youtube.com/watch?v=abc", "https://youtu.be/abc?t=1", "https://www.youtube.com/shorts/abc", "https://m.youtube.com/live/abc", "https://music.youtube.com/watch?v=abc", "https://youtube.com/embed/abc"]) assert.ok(isYoutubePage(url), url);
  for (const url of ["https://youtube.com/", "https://youtube.com/watch?list=abc", "https://youtube.com/shorts/", "https://youtube.com.evil/watch?v=abc", "https://example.com/?url=https://youtu.be/abc", "file://youtube.com/watch?v=abc"]) assert.ok(!isYoutubePage(url), url);
});

test("Add Download exposes quality, opt-in cookies and Retry controls", () => {
  const html = fs.readFileSync(new URL("../index.html", import.meta.url), "utf8");
  for (const id of ["video-quality", "video-quality-row", "video-cookies", "video-cookies-row", "btn-retry-probe"]) assert.equal((html.match(new RegExp(`id="${id}"`, "g")) || []).length, 1, id);
});
