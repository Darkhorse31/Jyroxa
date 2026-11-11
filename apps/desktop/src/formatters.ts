// Utility formatters and file category detection
import type { FileCategory } from "./api";
import { icons } from "./icons";

const VIDEO_EXTS = new Set([
  "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "ts", "m2ts", "3gp", "ogv",
]);
const MUSIC_EXTS = new Set([
  "mp3", "wav", "flac", "aac", "ogg", "m4a", "wma", "alac", "opus", "aiff",
]);
const COMPRESSED_EXTS = new Set([
  "zip", "tar", "gz", "bz2", "xz", "7z", "rar", "tgz", "iso", "dmg", "pkg", "zst",
]);
const DOCUMENT_EXTS = new Set([
  "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "txt", "rtf", "epub", "csv",
]);
const PROGRAM_EXTS = new Set([
  "exe", "msi", "appimage", "deb", "rpm", "apk", "bat", "cmd", "sh", "bin",
]);

export function getFileCategory(filenameOrUrl: string, mime?: string | null): FileCategory {
  if (mime) {
    const m = mime.toLowerCase();
    if (m.startsWith("video/")) return "Video";
    if (m.startsWith("audio/")) return "Music";
    if (
      m.startsWith("application/pdf") ||
      m.includes("document") ||
      m.includes("spreadsheet") ||
      m.includes("presentation") ||
      m.startsWith("text/")
    ) {
      return "Documents";
    }
    if (
      m.includes("zip") ||
      m.includes("compressed") ||
      m.includes("tar") ||
      m.includes("gzip") ||
      m.includes("rar")
    ) {
      return "Compressed";
    }
    if (
      m.includes("octet-stream") ||
      m.includes("x-msdownload") ||
      m.includes("android.package-archive")
    ) {
      // Check extension below
    }
  }

  try {
    const raw = filenameOrUrl.split("?")[0].split("#")[0];
    const parts = raw.split(".");
    if (parts.length > 1) {
      const ext = parts.pop()!.toLowerCase().trim();
      if (VIDEO_EXTS.has(ext)) return "Video";
      if (MUSIC_EXTS.has(ext)) return "Music";
      if (DOCUMENT_EXTS.has(ext)) return "Documents";
      if (COMPRESSED_EXTS.has(ext)) return "Compressed";
      if (PROGRAM_EXTS.has(ext)) return "Programs";
    }
  } catch {
    // fallback
  }

  return "Other";
}

export function getCategoryIcon(category: FileCategory, size = 18): string {
  switch (category) {
    case "Video":
      return icons.video(size);
    case "Music":
      return icons.music(size);
    case "Documents":
      return icons.fileText(size);
    case "Compressed":
      return icons.archive(size);
    case "Programs":
      return icons.programs(size);
    case "Other":
    default:
      return icons.other(size);
  }
}

export function fmtBytes(n: number, decimals?: number): string {
  if (n <= 0 || !Number.isFinite(n)) return "0 B";
  if (n < 1024) return `${Math.round(n)} B`;
  const units = ["KB", "MB", "GB", "TB", "PB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const d = decimals !== undefined ? decimals : v >= 100 ? 0 : v >= 10 ? 1 : 2;
  return `${v.toFixed(d)} ${units[i]}`;
}

export function fmtSpeed(bps: number): string {
  if (bps <= 0 || !Number.isFinite(bps)) return "0 B/s";
  return `${fmtBytes(bps)}/s`;
}

export function fmtDuration(secs: number): string {
  if (secs <= 0 || !Number.isFinite(secs)) return "0s";
  const s = Math.round(secs);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const remS = s % 60;
  if (m < 60) {
    return remS > 0 ? `${m}m ${remS}s` : `${m}m`;
  }
  const h = Math.floor(m / 60);
  const remM = m % 60;
  return remM > 0 ? `${h}h ${remM}m` : `${h}h`;
}

export function fmtEta(secs: number | null | undefined): string {
  if (secs === null || secs === undefined || !Number.isFinite(secs) || secs < 0) {
    return "—";
  }
  if (secs === 0) return "Done";
  const s = Math.round(secs);
  if (s < 60) return `${s}s left`;
  const m = Math.floor(s / 60);
  const remS = s % 60;
  if (m < 60) {
    return `${m}m ${remS}s left`;
  }
  const h = Math.floor(m / 60);
  const remM = m % 60;
  return `${h}h ${remM}m left`;
}

export function basenameFromUrl(url: string): string {
  try {
    const parsed = new URL(url);
    const pathname = parsed.pathname;
    const clean = pathname.split("/").filter(Boolean).pop();
    if (clean) return decodeURIComponent(clean);
  } catch {
    const clean = url.split("?")[0].split("#")[0].split("/").filter(Boolean).pop();
    if (clean) return decodeURIComponent(clean);
  }
  return "download.bin";
}

export function extractHost(url: string): string {
  try {
    return new URL(url).host || "—";
  } catch {
    return "—";
  }
}

export function extractProtocol(url: string): string {
  try {
    const proto = new URL(url).protocol.replace(":", "").toUpperCase();
    return proto || "—";
  } catch {
    return "—";
  }
}
