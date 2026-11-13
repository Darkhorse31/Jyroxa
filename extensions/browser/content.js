document.addEventListener(
  "keydown",
  (e) => {
    if (e.altKey) {
      const api = globalThis.browser ?? globalThis.chrome;
      api.runtime.sendMessage({ type: "alt-bypass" });
    }
  },
  true,
);

document.addEventListener(
  "click",
  (e) => {
    if (e.altKey) {
      const api = globalThis.browser ?? globalThis.chrome;
      api.runtime.sendMessage({ type: "alt-bypass" });
    }
  },
  true,
);
