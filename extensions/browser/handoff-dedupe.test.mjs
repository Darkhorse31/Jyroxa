import test from "node:test";
import assert from "node:assert/strict";
import {
  createHandoffTracker,
  urlHasKnownExtension,
  extOf,
} from "./handoff-dedupe.js";

test("extOf reads path segment before query", () => {
  assert.equal(extOf("https://x/y/file.zip?dl=1"), "zip");
  assert.equal(extOf("https://pixeldrain.dev/u/id"), "");
});

test("urlHasKnownExtension", () => {
  assert.equal(urlHasKnownExtension("https://cdn/a.bin"), true);
  assert.equal(urlHasKnownExtension("https://pixeldrain.dev/api/file/x?download"), false);
});

test("handoff tracker dedupes by download id", () => {
  const t = createHandoffTracker();
  assert.equal(t.tryClaim(7), true);
  assert.equal(t.tryClaim(7), false);
  assert.equal(t.isClaimed(7), true);
  t.release(7);
  assert.equal(t.tryClaim(7), true);
});