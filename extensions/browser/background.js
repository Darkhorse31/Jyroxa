import { sendNative, refreshSettingsFromApp } from "./native.js";
import { shouldInterceptDownload, extEnabled } from "./intercept.js";
import {
  registerContextMenus,
  onContextMenuClick,
} from "./context-menus.js";

const api = globalThis.browser ?? globalThis.chrome;

registerContextMenus();
api.contextMenus.onClicked.addListener(onContextMenuClick);

let bypassUntil = 0;

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

async function handoffDownload(details) {
  const url = details.url || details.finalUrl;
  if (!url) return;
  const referer = details.referrer || details.referrer || "";
  const pageUrl = details.finalUrl || url;
  const filename = details.filename || details.suggestedFilename || "";
  const cookies = await cookiesForUrl(url);
  const userAgent = navigator.userAgent;

  await sendNative({
    type: "handoff",
    url,
    referrer: referer || undefined,
    page_url: pageUrl,
    filename: filename || undefined,
    cookies: cookies || undefined,
    user_agent: userAgent,
  });
}

function hostFromUrl(url) {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

async function onDownloadCreated(item) {
  if (!extEnabled()) return;
  if (Date.now() < bypassUntil) return;
  if (!item.url?.startsWith("http")) return;

  const settings = await refreshSettingsFromApp();
  const host = hostFromUrl(item.url);
  if (settings.disabledHosts?.includes(host)) return;

  const take = shouldInterceptDownload(item, settings);
  if (!take) return;

  try {
    await api.downloads.cancel(item.id);
  } catch {
  }

  await handoffDownload({
    url: item.url,
    referrer: item.referrer,
    filename: item.filename,
    finalUrl: item.finalUrl,
  });
}

api.downloads.onCreated.addListener((item) => {
  void onDownloadCreated(item);
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
    try {
      await api.downloads.cancel(item.id);
    } catch {
    }
    await handoffDownload(item);
  })();
});

api.runtime.onInstalled.addListener(() => {
  registerContextMenus();
});
