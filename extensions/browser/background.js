import { sendNative, refreshSettingsFromApp, connectionFailureReason } from "./native.js";
import { shouldInterceptDownload, extEnabled } from "./intercept.js";
import {
  registerContextMenus,
  onContextMenuClick,
} from "./context-menus.js";
import {
  createHandoffTracker,
  urlHasKnownExtension,
} from "./handoff-dedupe.js";
import {
  recordMedia,
  listMediaForTab,
  clearTab,
  mediaLabel,
  formatBytes,
} from "./media-monitor.js";

const api = globalThis.browser ?? globalThis.chrome;

registerContextMenus();
api.contextMenus.onClicked.addListener(onContextMenuClick);

let bypassUntil = 0;
const handoffTracker = createHandoffTracker();
const recentMagnetHandoffs = new Map();
const MAGNET_DEDUPE_MS = 3000;

function claimMagnetUrl(url) {
  const now = Date.now();
  const prev = recentMagnetHandoffs.get(url);
  if (prev && now - prev < MAGNET_DEDUPE_MS) return false;
  recentMagnetHandoffs.set(url, now);
  return true;
}

try {
  api.webRequest.onCompleted.addListener(
    (details) => recordMedia(details),
    { urls: ["<all_urls>"] },
    ["responseHeaders"],
  );
} catch {
}

api.tabs.onRemoved.addListener((tabId) => clearTab(tabId));

api.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg?.type === "alt-bypass") {
    bypassUntil = Date.now() + 30_000;
    sendResponse({ ok: true });
    return false;
  }
  if (msg?.type === "video-grab-list") {
    void (async () => {
      const tabId = sender.tab?.id ?? -1;
      let items = tabId >= 0 ? listMediaForTab(tabId) : [];
      if (!msg.blobLike && msg.elementSrc?.startsWith("http")) {
        const dup = items.some((i) => i.url === msg.elementSrc);
        if (!dup) {
          items = [
            {
              url: msg.elementSrc,
              kind: "video",
              size: null,
            },
            ...items,
          ];
        }
      }
      if (msg.blobLike) {
        items = [
          ...items,
          {
            url: msg.pageUrl,
            kind: "page",
            video_page: true,
          },
        ];
      }
      sendResponse({
        items: items.map((it) => ({
          url: it.url,
          kind: it.kind,
          label: mediaLabel(it),
          sizeLabel: it.size ? formatBytes(it.size) : "",
          filename: guessFilename(it.url),
          format_id: it.format_id,
          video_page: it.video_page,
        })),
      });
    })();
    return true;
  }
  if (msg?.type === "video-grab-handoff") {
    void (async () => {
      const payload = { ...msg.payload };
      const tabId = sender.tab?.id;
      if (tabId != null) {
        const items = listMediaForTab(tabId);
        const hit = items.find((i) => i.url === payload.url);
        if (hit?.initiator && !payload.referrer) {
          payload.referrer = hit.initiator;
        }
      }
      if (!payload.referrer && msg.payload?.page_url) {
        payload.referrer = msg.payload.page_url;
      }
      if (payload.video_page) {
        payload.type = "handoff";
        payload.url = payload.page_url || payload.url;
        payload.video_page = true;
      }
      const cookies = await cookiesForUrl(payload.url);
      payload.cookies = cookies || undefined;
      payload.user_agent = navigator.userAgent;
      const res = await sendNative(payload);
      if (!res?.ok) notifyHandoffFailure(res);
      sendResponse(res);
    })();
    return true;
  }
  if (msg?.type === "magnet-handoff" && msg.url?.startsWith("magnet:")) {
    if (!extEnabled() || Date.now() < bypassUntil) {
      sendResponse({ ok: false, error: "extension disabled" });
      return false;
    }
    if (!claimMagnetUrl(msg.url)) {
      sendResponse({ ok: true, deduped: true });
      return false;
    }
    void (async () => {
      const res = await sendNative({
        type: "handoff",
        url: msg.url,
        user_agent: navigator.userAgent,
      });
      sendResponse(res);
    })();
    return true;
  }
  return false;
});

function guessFilename(url) {
  try {
    const p = new URL(url).pathname.split("/").pop() || "video";
    return p.includes(".") ? p : "video.mp4";
  } catch {
    return "video.mp4";
  }
}

async function cookiesForUrl(url) {
  try {
    const list = await api.cookies.getAll({ url });
    if (!list?.length) return "";
    return list.map((c) => `${c.name}=${c.value}`).join("; ");
  } catch {
    return "";
  }
}

async function buildHandoffPayload(details) {
  const url = details.url || details.finalUrl;
  if (!url) return null;
  const referer = details.referrer || "";
  const pageUrl = details.finalUrl || url;
  const filename = details.filename || details.suggestedFilename || "";
  const cookies = await cookiesForUrl(url);
  const userAgent = navigator.userAgent;

  return {
    type: "handoff",
    url,
    referrer: referer || undefined,
    page_url: pageUrl,
    filename: filename || undefined,
    cookies: cookies || undefined,
    user_agent: userAgent,
  };
}

async function performHandoff(details) {
  const payload = await buildHandoffPayload(details);
  if (!payload) return { ok: false, error: "missing url" };
  return sendNative(payload);
}

function notifyHandoffFailure(res) {
  const reason = connectionFailureReason(res?.error || "handoff failed");
  const id = `handoff-fail-${Date.now()}`;
  try {
    api.notifications.create(id, {
      type: "basic",
      iconUrl: "icons/icon-128.png",
      title: reason.short,
      message: reason.detail || "Chrome will continue the download.",
    });
  } catch {
  }
}

function hostFromUrl(url) {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

async function interceptDownload(item, { viaCreated }) {
  if (!extEnabled()) return;
  if (Date.now() < bypassUntil) return;
  if (item.url?.startsWith("magnet:")) {
    if (!claimMagnetUrl(item.url)) return;
    await sendNative({
      type: "handoff",
      url: item.url,
      user_agent: navigator.userAgent,
    });
    try {
      await api.downloads.cancel(item.id);
    } catch {
    }
    return;
  }
  if (!item.url?.startsWith("http")) return;

  const settings = await refreshSettingsFromApp();
  const host = hostFromUrl(item.url);
  if (settings.disabledHosts?.includes(host)) return;

  if (!shouldInterceptDownload(item, settings)) return;

  if (viaCreated) {
    if (handoffTracker.isClaimed(item.id)) return;
    if (!urlHasKnownExtension(item.url)) return;
    if (!handoffTracker.tryClaim(item.id)) return;
  } else if (!handoffTracker.tryClaim(item.id)) {
    return;
  }

  const res = await performHandoff(item);
  if (res?.ok) {
    try {
      await api.downloads.cancel(item.id);
    } catch {
    }
    return;
  }

  handoffTracker.release(item.id);
  if (!viaCreated) {
    try {
      await api.downloads.resume(item.id);
    } catch {
    }
  }
  notifyHandoffFailure(res);
}

api.downloads.onCreated.addListener((item) => {
  void interceptDownload(item, { viaCreated: true });
});

api.downloads.onDeterminingFilename.addListener((item, suggest) => {
  if (!extEnabled() || Date.now() < bypassUntil) {
    suggest();
    return;
  }
  void (async () => {
    const settings = await refreshSettingsFromApp();
    const host = hostFromUrl(item.url || item.finalUrl || "");
    if (settings.disabledHosts?.includes(host)) {
      suggest();
      return;
    }
    if (!shouldInterceptDownload(item, settings)) {
      suggest();
      return;
    }
    if (handoffTracker.isClaimed(item.id)) {
      suggest();
      return;
    }
    if (!handoffTracker.tryClaim(item.id)) {
      suggest();
      return;
    }

    const res = await performHandoff(item);
    if (res?.ok) {
      try {
        await api.downloads.cancel(item.id);
      } catch {
      }
      return;
    }

    handoffTracker.release(item.id);
    suggest();
    try {
      await api.downloads.resume(item.id);
    } catch {
    }
    notifyHandoffFailure(res);
  })();
});

api.runtime.onInstalled.addListener(() => {
  registerContextMenus();
});