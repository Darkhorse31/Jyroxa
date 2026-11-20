/** Per-tab media URL capture via webRequest (imported from service worker). */

const MEDIA_EXT = /\.(m3u8|mpd|mp4|webm|m4v|mov|mp3|m4a|aac|ogg)(\?|$)/i;

function mediaKind(url, mime) {
  const u = url.toLowerCase();
  if (u.includes(".m3u8")) return "hls";
  if (u.includes(".mpd")) return "dash";
  if (mime?.startsWith("audio/")) return "audio";
  return "video";
}

function basenameFromUrl(url) {
  try {
    const p = new URL(url).pathname.split("/").pop() || "";
    return p.includes(".") ? decodeURIComponent(p) : "";
  } catch {
    return "";
  }
}

function labelFor(entry) {
  if (entry.label) return entry.label;
  if (entry.resolution) return entry.resolution;
  const base = basenameFromUrl(entry.url);
  if (entry.kind === "hls") {
    if (entry.bandwidth) {
      return `${entry.resolution || "HLS"} · ${Math.round(entry.bandwidth / 1000)} kbps`;
    }
    return base ? `HLS · ${base}` : "HLS stream";
  }
  if (entry.kind === "dash") {
    return base ? `DASH · ${base}` : "DASH stream";
  }
  if (entry.kind === "audio") {
    return base || "Audio";
  }
  if (base) return base;
  if (entry.bandwidth) return `${Math.round(entry.bandwidth / 1000)} kbps`;
  if (entry.size) return `${formatBytes(entry.size)} file`;
  const ext = entry.url?.match(/\.(\w+)(\?|$)/i)?.[1];
  if (ext) return `${ext.toUpperCase()} video`;
  return "Media stream";
}

export function formatBytes(n) {
  if (!n || n <= 0) return "";
  const units = ["B", "KB", "MB", "GB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

/** @type {Map<number, Map<string, object>>} */
const byTab = new Map();

function tabMap(tabId) {
  if (!byTab.has(tabId)) byTab.set(tabId, new Map());
  return byTab.get(tabId);
}

export function recordMedia(details) {
  if (!details?.url?.startsWith("http")) return;
  const mime = details.responseHeaders?.find((h) => h.name.toLowerCase() === "content-type")
    ?.value;
  if (!MEDIA_EXT.test(details.url) && !mime?.startsWith("video/") && !mime?.startsWith("audio/")) {
    return;
  }
  const sizeHeader = details.responseHeaders?.find((h) => h.name.toLowerCase() === "content-length");
  const size = sizeHeader ? Number(sizeHeader.value) : undefined;
  const tabId = details.tabId ?? -1;
  if (tabId < 0) return;
  const kind = mediaKind(details.url, mime);
  const map = tabMap(tabId);
  const key = `${kind}:${details.url}`;
  map.set(key, {
    url: details.url,
    kind,
    mime: mime || null,
    size: Number.isFinite(size) ? size : null,
    bandwidth: null,
    resolution: null,
    initiator: details.initiator || details.documentUrl || null,
  });
}

export function listMediaForTab(tabId) {
  const map = byTab.get(tabId);
  if (!map) return [];
  return [...map.values()].sort((a, b) => (b.bandwidth || 0) - (a.bandwidth || 0));
}

export function clearTab(tabId) {
  byTab.delete(tabId);
}

export function mediaLabel(entry) {
  return labelFor(entry);
}