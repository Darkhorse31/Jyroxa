import assert from "node:assert/strict";

let mode;
globalThis.chrome = { runtime: { connectNative: () => {
  if (mode === "connect-throws") throw new Error("Failed to start native messaging host.");
  const listeners = {};
  return {
    onMessage: { addListener: fn => listeners.message = fn },
    onDisconnect: { addListener: fn => listeners.disconnect = fn },
    postMessage: () => {
      if (mode === "post-throws") throw new Error("Disconnected from port");
      if (mode === "success") listeners.message({ok:true,connected:true});
      if (mode === "forbidden") { chrome.runtime.lastError = {message:"Access to the specified native messaging host is forbidden."}; listeners.disconnect(); }
    },
    disconnect: () => listeners.disconnect?.(),
  };
} } };
const { sendNative, pingFailureFromResponse } = await import("../native.js");
for (const [testMode, expected] of [["connect-throws","Host failed to start"],["post-throws","Connection lost"],["forbidden","Extension not allowed"],["timeout","No response"]]) {
  mode = testMode; delete chrome.runtime.lastError;
  const result = await sendNative({type:"ping"}, 10);
  assert.equal(result.ok, false);
  assert.equal(pingFailureFromResponse(result).short, expected);
}
mode = "success"; delete chrome.runtime.lastError;
assert.deepEqual(await sendNative({type:"ping"}), {ok:true,connected:true});
console.log("send-native.spec.mjs OK");
