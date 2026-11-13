import { sendNative } from "./native.js";

const api = globalThis.browser ?? globalThis.chrome;

export function registerContextMenus() {
  api.contextMenus.removeAll(() => {
    api.contextMenus.create({
      id: "jyroxa-download-link",
      title: "Download with Jyroxa",
      contexts: ["link", "video", "audio"],
    });
    api.contextMenus.create({
      id: "jyroxa-grab-page",
      title: "Download all links with Jyroxa",
      contexts: ["page", "frame", "selection"],
    });
  });
}

export function onContextMenuClick(info, tab) {
  if (info.menuItemId === "jyroxa-download-link" && info.linkUrl) {
    void sendNative({
      type: "handoff",
      url: info.linkUrl,
      referrer: tab?.url,
      page_url: tab?.url,
    });
    return;
  }
  if (info.menuItemId === "jyroxa-grab-page") {
    void grabPageLinks(tab);
  }
}

async function grabPageLinks(tab) {
  const pageUrl = tab?.url || "";
  let html = "";
  if (tab?.id != null) {
    try {
      const [{ result }] = await api.scripting.executeScript({
        target: { tabId: tab.id },
        func: () => document.documentElement.outerHTML,
      });
      html = result || "";
    } catch {
      html = "";
    }
  }
  await sendNative({
    type: "grab_links",
    html,
    page_url: pageUrl,
  });
}
