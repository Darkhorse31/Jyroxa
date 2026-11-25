(function () {
  const api = globalThis.browser ?? globalThis.chrome;
  const TEAL = "#7a5c2e";
  const HOST_ID = "jx-vgrab-host";

  let enabled = true;
  let siteHidden = false;
  const controls = new Map();
  let activeMenu = null;
  let placementFrame = 0;

  function closeMenu(restoreFocus = false) {
    if (!activeMenu) return;
    const { menu, btn } = activeMenu;
    activeMenu = null;
    menu.remove();
    btn.setAttribute("aria-expanded", "false");
    if (restoreFocus && btn.isConnected) btn.focus();
  }

  function schedulePlacement() {
    if (placementFrame) return;
    placementFrame = requestAnimationFrame(() => {
      placementFrame = 0;
      for (const [video, host] of controls) {
        if (!video.isConnected || !host.isConnected) {
          if (activeMenu?.host === host) closeMenu();
          host.remove();
          resizeObserver.unobserve(video);
          controls.delete(video);
          delete video.__jxVgrabHost;
          continue;
        }
        placeHost(video);
      }
      // Track player/sidebar animations while the menu is open; a resize event
      // alone does not catch transforms or position-only layout changes.
      if (activeMenu) schedulePlacement();
    });
  }

  function positionMenu() {
    if (!activeMenu) return;
    const {menu, btn} = activeMenu;
    const viewport = window.visualViewport;
    const left = viewport?.offsetLeft || 0;
    const top = viewport?.offsetTop || 0;
    const width = viewport?.width || document.documentElement.clientWidth;
    const height = viewport?.height || document.documentElement.clientHeight;
    const rect = btn.getBoundingClientRect();
    menu.style.maxWidth = `${Math.max(0, width - 16)}px`;
    menu.style.minWidth = `${Math.min(180, Math.max(0, width - 16))}px`;
    menu.style.maxHeight = `${Math.max(0, height - 16)}px`;
    const bounds = menu.getBoundingClientRect();
    const x = Math.max(left + 8, Math.min(rect.right - bounds.width, left + width - bounds.width - 8));
    const below = rect.bottom + 4;
    const y = below + bounds.height <= top + height - 8 ? below : Math.max(top + 8, rect.top - bounds.height - 4);
    menu.style.left = `${x}px`;
    menu.style.top = `${Math.min(y, top + height - bounds.height - 8)}px`;
  }

  const resizeObserver = new ResizeObserver(schedulePlacement);
  window.addEventListener("resize", schedulePlacement);
  document.addEventListener("scroll", schedulePlacement, true);
  window.visualViewport?.addEventListener("resize", schedulePlacement);
  window.visualViewport?.addEventListener("scroll", schedulePlacement);
  document.addEventListener("fullscreenchange", schedulePlacement);
  document.addEventListener("pointerdown", event => {
    if (activeMenu && !activeMenu.host.contains(event.target)) closeMenu();
  }, true);
  document.addEventListener("keydown", event => {
    if (!activeMenu) return;
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); closeMenu(true); }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      const buttons = [...activeMenu.menu.querySelectorAll("button")];
      const index = buttons.indexOf(document.activeElement);
      const next = event.key === "ArrowDown" ? index + 1 : (index < 0 ? 0 : index) - 1;
      if (buttons.length) { event.preventDefault(); buttons[(next + buttons.length) % buttons.length].focus(); }
    }
  }, true);

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
    if (!host || !host.isConnected) {
      host = document.createElement("div");
      host.className = HOST_ID;
      host.dataset.jxVgrab = "1";
      host.setAttribute("popover", "manual");
      document.documentElement.appendChild(host);
      video.__jxVgrabHost = host;
      controls.set(video, host);
      resizeObserver.observe(video);
    }
    const fullscreen = document.fullscreenElement;
    const target = fullscreen && fullscreen.contains(video) && fullscreen !== video
      ? fullscreen : document.documentElement;
    if (host.parentElement !== target) {
      if (typeof host.hidePopover === "function" && host.matches(":popover-open")) host.hidePopover();
      target.appendChild(host);
    }
    const rect = video.getBoundingClientRect();
    const visible = enabled && !siteHidden && rect.width > 0 && rect.height > 0
      && rect.bottom > 0 && rect.top < innerHeight
      && rect.right > 0 && rect.left < innerWidth
      && (!fullscreen || fullscreen.contains(video));
    host.style.display = visible ? "block" : "none";
    if (!visible) {
      if (activeMenu?.host === host) closeMenu();
      if (typeof host.hidePopover === "function" && host.matches(":popover-open")) host.hidePopover();
    }
    if (visible && typeof host.showPopover === "function" && !host.matches(":popover-open")) {
      try { host.showPopover(); } catch { /* fixed-position fallback */ }
    }
    host.style.top = `${Math.max(6, Math.min(rect.top + 6, innerHeight - 32))}px`;
    host.style.left = `${Math.max(6, Math.min(rect.right - 58, innerWidth - 40))}px`;
    positionMenu();
    return host;
  }

  function buildUi(video) {
    cssLink();
    if (!enabled || siteHidden || controls.has(video)) return;
    const host = placeHost(video);

    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "jx-vgrab-btn";
    btn.title = "Download with Jyroxa";
    btn.textContent = "↓";
    btn.style.background = TEAL;
    btn.setAttribute("aria-haspopup", "true");
    btn.setAttribute("aria-expanded", "false");

    btn.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      void openMenu(host, video, btn);
    });

    host.appendChild(btn);

    const show = () => {
      if (enabled && !siteHidden && video.__jxVgrabHost?.isConnected) placeHost(video);
    };
    if (!video.__jxVgrabWired) {
      video.__jxVgrabWired = true;
      video.addEventListener("mouseenter", show);
      video.addEventListener("play", show);
    }
    show();
  }

  async function openMenu(host, video, btn) {
    if (activeMenu?.host === host) { closeMenu(true); return; }
    closeMenu();
    const menu = document.createElement("div");
    menu.className = "jx-vgrab-menu";
    menu.setAttribute("role", "menu");
    menu.setAttribute("aria-label", "Download quality");
    menu.innerHTML = '<div class="jx-vgrab-muted">Finding qualities…</div>';
    host.appendChild(menu);
    activeMenu = {host, menu, btn};
    schedulePlacement();
    btn.setAttribute("aria-expanded", "true");
    positionMenu();

  const loadMenu = async () => {
    menu.innerHTML = '<div class="jx-vgrab-muted">Finding qualities…</div>';
    const pageUrl = location.href;
    const blobLike = isBlobOrEmptySrc(video);
    const res = await api.runtime.sendMessage({
      type: "video-grab-list",
      pageUrl,
      blobLike,
      elementSrc: video.currentSrc || video.src || "",
    }).catch(error => ({error: error.message || "Could not fetch qualities. Click Retry."}));
    if (activeMenu?.menu !== menu) return;

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
        row.setAttribute("role", "menuitem");
        const size = it.sizeLabel ? ` (${it.sizeLabel})` : "";
        row.textContent = `${it.label}${size}`;
        row.addEventListener("click", () => void handoffDirect(it, pageUrl, menu));
        menu.appendChild(row);
      }
    }

    const hideSite = document.createElement("button");
    hideSite.type = "button";
    hideSite.className = "jx-vgrab-hide";
    hideSite.setAttribute("role", "menuitem");
    hideSite.textContent = "Hide on this site";
    hideSite.addEventListener("click", () => void hideOnSite(menu));
    menu.appendChild(hideSite);
    positionMenu();
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
    closeMenu();
    document.querySelectorAll(`.${HOST_ID}`).forEach((el) => el.remove());
  }

  async function handoffPage(pageUrl, menu) {
    closeMenu();
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
    closeMenu();
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
      buildUi(el);
    });
  }

  function init() {
    void loadPrefs().then(() => {
      scan(document);
      const mo = new MutationObserver((mut) => {
        if (mut.some(m => !m.target.closest?.(`.${HOST_ID}`))) schedulePlacement();
        for (const m of mut) {
          m.addedNodes.forEach((node) => {
            if (node.nodeType !== 1) return;
            if (node.matches?.("video,audio")) buildUi(node);
            else scan(node);
          });
        }
      });
      mo.observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ["style", "class", "hidden"] });
    });
  }

  api.storage.onChanged.addListener((chg) => {
    if (chg.jyroxaVideoGrab) {
      enabled = chg.jyroxaVideoGrab.newValue !== false;
      document.querySelectorAll(`.${HOST_ID}`).forEach((el) => el.remove());
      controls.clear();
      resizeObserver.disconnect();
      closeMenu();
      scan(document);
    }
  });

  api.runtime.onMessage.addListener((msg) => {
    if (msg?.type === "video-grab-refresh") {
      void loadPrefs().then(() => {
        closeMenu();
        for (const host of controls.values()) host.remove();
        controls.clear();
        resizeObserver.disconnect();
        scan(document);
      });
    }
  });

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init, { once: true });
  } else {
    init();
  }
})();
