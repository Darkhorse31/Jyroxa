(function () {
  const api = globalThis.browser ?? globalThis.chrome;
  const TEAL = "#0f766e";
  const HOST_ID = "jx-vgrab-host";

  let enabled = true;
  let siteHidden = false;

  function hostFromLocation() {
    try {
      return location.hostname;
    } catch {
      return "";
    }
  }

  async function loadPrefs() {
    const host = hostFromLocation();
    const local = await api.storage.local.get([
      "jyroxaVideoGrab",
      "jyroxaVideoHiddenHosts",
    ]);
    enabled = local.jyroxaVideoGrab !== false;
    const hidden = local.jyroxaVideoHiddenHosts || [];
    siteHidden = hidden.includes(host);
  }

  function cssLink() {
    if (document.getElementById("jx-vgrab-css")) return;
    const link = document.createElement("link");
    link.id = "jx-vgrab-css";
    link.rel = "stylesheet";
    link.href = api.runtime.getURL("video-grab.css");
    document.documentElement.appendChild(link);
  }

  function isBlobOrEmptySrc(video) {
    const src = video.currentSrc || video.src || "";
    return !src || src.startsWith("blob:");
  }

  function placeHost(video) {
    let host = video.__jxVgrabHost;
    if (!host) {
      host = document.createElement("div");
      host.className = HOST_ID;
      host.dataset.jxVgrab = "1";
      const parent = video.parentElement;
      if (parent && getComputedStyle(parent).position === "static") {
        parent.style.position = "relative";
      }
      (parent || document.documentElement).appendChild(host);
      video.__jxVgrabHost = host;
    }
    const rect = video.getBoundingClientRect();
    const parentRect = host.offsetParent?.getBoundingClientRect() || { top: 0, left: 0 };
    host.style.top = `${rect.top - parentRect.top + 6}px`;
    host.style.left = `${rect.left - parentRect.left + rect.width - 58}px`;
    return host;
  }

  function buildUi(video) {
    cssLink();
    const host = placeHost(video);
    host.innerHTML = "";
    if (!enabled || siteHidden) return;

    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "jx-vgrab-btn";
    btn.title = "Download with Jyroxa";
    btn.textContent = "↓";
    btn.style.background = TEAL;

    btn.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      void openMenu(host, video, btn);
    });

    host.appendChild(btn);

    const show = () => {
      host.style.display = "block";
      placeHost(video);
    };
    const hide = () => {
      if (!host.querySelector(".jx-vgrab-menu")) {
        host.style.display = "none";
      }
    };
    video.addEventListener("mouseenter", show);
    video.addEventListener("mouseleave", hide);
    video.addEventListener("play", show);
    show();
  }

  async function openMenu(host, video, btn) {
    host.querySelector(".jx-vgrab-menu")?.remove();
    const menu = document.createElement("div");
    menu.className = "jx-vgrab-menu";
    menu.innerHTML = '<div class="jx-vgrab-muted">Finding qualities…</div>';
    host.appendChild(menu);

  const loadMenu = async () => {
    menu.innerHTML = '<div class="jx-vgrab-muted">Finding qualities…</div>';
    const pageUrl = location.href;
    const blobLike = isBlobOrEmptySrc(video);
    const res = await api.runtime.sendMessage({
      type: "video-grab-list",
      pageUrl,
      blobLike,
      elementSrc: video.currentSrc || video.src || "",
    });

    menu.innerHTML = "";
    const items = (res?.items || []).filter(
      (it) => it.label && it.label !== "VIDEO" && it.label !== "video",
    );
    if (res?.error && !items.length) {
      const err = document.createElement("div");
      err.className = "jx-vgrab-error";
      err.textContent = res.error;
      menu.appendChild(err);
      const retry = document.createElement("button");
      retry.type = "button";
      retry.className = "jx-vgrab-item";
      retry.textContent = "Retry";
      retry.addEventListener("click", () => void loadMenu());
      menu.appendChild(retry);
    } else if (!items.length) {
      const note = document.createElement("div");
      note.className = "jx-vgrab-muted";
      note.textContent =
        res?.error ||
        (blobLike
          ? "No qualities found for this page"
          : "No media URLs captured yet");
      menu.appendChild(note);
      const retry = document.createElement("button");
      retry.type = "button";
      retry.className = "jx-vgrab-item";
      retry.textContent = "Retry";
      retry.addEventListener("click", () => void loadMenu());
      menu.appendChild(retry);
    } else {
      for (const it of items) {
        const row = document.createElement("button");
        row.type = "button";
        row.className = "jx-vgrab-item";
        const size = it.sizeLabel ? ` (${it.sizeLabel})` : "";
        row.textContent = `${it.label}${size}`;
        row.addEventListener("click", () => void handoffDirect(it, pageUrl, menu));
        menu.appendChild(row);
      }
    }

    const hideSite = document.createElement("button");
    hideSite.type = "button";
    hideSite.className = "jx-vgrab-hide";
    hideSite.textContent = "Hide on this site";
    hideSite.addEventListener("click", () => void hideOnSite(menu));
    menu.appendChild(hideSite);
  };

    await loadMenu();
  }

  async function hideOnSite(menu) {
    const host = hostFromLocation();
    const local = await api.storage.local.get("jyroxaVideoHiddenHosts");
    const list = new Set(local.jyroxaVideoHiddenHosts || []);
    list.add(host);
    await api.storage.local.set({ jyroxaVideoHiddenHosts: [...list] });
    siteHidden = true;
    menu.remove();
    document.querySelectorAll(`.${HOST_ID}`).forEach((el) => el.remove());
  }

  async function handoffPage(pageUrl, menu) {
    menu.remove();
    await api.runtime.sendMessage({
      type: "video-grab-handoff",
      payload: {
        type: "handoff",
        url: pageUrl,
        page_url: pageUrl,
        video_page: true,
      },
    });
  }

  async function handoffDirect(item, pageUrl, menu) {
    menu.remove();
    await api.runtime.sendMessage({
      type: "video-grab-handoff",
      payload: {
        type: "handoff",
        url: item.video_page ? pageUrl : item.url,
        page_url: pageUrl,
        filename: item.filename,
        format_id: item.format_id,
        video_page: item.video_page || undefined,
      },
    });
  }

  function scan(root) {
    root.querySelectorAll("video,audio").forEach((el) => {
      if (el.__jxVgrabWired) return;
      el.__jxVgrabWired = true;
      buildUi(el);
    });
  }

  function init() {
    void loadPrefs().then(() => {
      scan(document);
      const mo = new MutationObserver((mut) => {
        for (const m of mut) {
          m.addedNodes.forEach((node) => {
            if (node.nodeType !== 1) return;
            if (node.matches?.("video,audio")) buildUi(node);
            else scan(node);
          });
        }
      });
      mo.observe(document.documentElement, { childList: true, subtree: true });
    });
  }

  api.storage.onChanged.addListener((chg) => {
    if (chg.jyroxaVideoGrab) {
      enabled = chg.jyroxaVideoGrab.newValue !== false;
      document.querySelectorAll(`.${HOST_ID}`).forEach((el) => el.remove());
      scan(document);
    }
  });

  api.runtime.onMessage.addListener((msg) => {
    if (msg?.type === "video-grab-refresh") {
      void loadPrefs().then(() => scan(document));
    }
  });

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init, { once: true });
  } else {
    init();
  }
})();