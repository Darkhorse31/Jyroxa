import { getCachedSettings } from "./native.js";

const EXT = {
  archives: [
    "zip",
    "rar",
    "7z",
    "tar",
    "gz",
    "bz2",
    "xz",
    "tgz",
    "tbz",
    "tar.gz",
    "tar.bz2",
    "tar.xz",
  ],
  programs: ["exe", "msi", "msix", "apk", "dmg", "pkg", "deb", "rpm", "appx"],
  video: ["mp4", "mkv", "webm", "avi", "mov", "m4v", "flv", "wmv"],
  audio: ["mp3", "flac", "wav", "aac", "ogg", "m4a", "wma"],
  documents: ["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "csv"],
  iso: ["iso", "img", "bin", "dmg"],
};

function extOf(nameOrUrl) {
  const base = (nameOrUrl || "").split(/[?#]/)[0];
  const dot = base.lastIndexOf(".");
  if (dot < 0) return "";
  return base.slice(dot + 1).toLowerCase();
}

function matchesCategory(ext, settings) {
  const ft = settings.fileTypes || {};
  for (const [cat, list] of Object.entries(EXT)) {
    if (!ft[cat]) continue;
    if (list.includes(ext)) return true;
  }
  return false;
}

export function shouldInterceptDownload(item, settings) {
  if (!settings?.enabled) return false;
  const name = item.filename || item.suggestedFilename || item.url || "";
  const ext = extOf(name);
  if (!ext) {
    return true;
  }
  if (!matchesCategory(ext, settings)) return false;
  const bytes = item.totalBytes ?? item.bytesReceived ?? 0;
  const min = settings.minSizeBytes ?? 0;
  if (min > 0 && bytes > 0 && bytes < min) return false;
  return true;
}

export function extEnabled() {
  return getCachedSettings().enabled !== false;
}
