import assert from "node:assert/strict";
import { connectionFailureReason } from "../native.js";

const cases = [
  {
    in: "Specified native messaging host not found.",
    short: "Host unavailable",
  },
  {
    in: "desktop pipe not available: Access is denied.",
    short: "Jyroxa not running",
  },
  {
    in: "desktop pipe not available: The system cannot find the file specified. (os error 2)",
    short: "Jyroxa not running",
  },
  {
    in: "Access to the specified native messaging host is forbidden.",
    short: "Extension not allowed",
  },
  {
    in: "native host not installed",
    short: "Host not configured",
  },
  {
    in: "The system cannot find the file specified.",
    short: "Host failed to start",
  },
  {
    in: "Failed to start native messaging host.",
    short: "Host failed to start",
  },
  {
    in: "Error when communicating with the native messaging host.",
    short: "Host connection failed",
  },
  {
    in: "Native host has exited.",
    short: "Host connection failed",
  },
  {
    in: "disconnected",
    short: "Connection lost",
  },
];

for (const { in: err, short } of cases) {
  const r = connectionFailureReason(err);
  assert.equal(r.short, short, `for ${JSON.stringify(err)}`);
}

console.log("native.spec.mjs OK");
