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

document.addEventListener(
  "click",
  (e) => {
    const anchor = e.target?.closest?.('a[href^="magnet:"]');
    if (!anchor) return;
    const href = anchor.href || anchor.getAttribute("href") || "";
    if (!href.startsWith("magnet:")) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    const api = globalThis.browser ?? globalThis.chrome;
    void api.runtime.sendMessage({ type: "magnet-handoff", url: href });
  },
  true,
);
