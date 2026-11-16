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

const api = globalThis.browser ?? globalThis.chrome;

registerContextMenus();
api.contextMenus.onClicked.addListener(onContextMenuClick);

let bypassUntil = 0;
const handoffTracker = createHandoffTracker();

api.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg?.type === "alt-bypass") {
    bypassUntil = Date.now() + 30_000;
    sendResponse({ ok: true });
    return false;
  }
  return false;
});

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