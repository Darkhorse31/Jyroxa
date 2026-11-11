// Utility formatters and file category detection

export type FileCategory =
  | "video"
  | "audio"
  | "archive"
  | "image"
  | "document"
  | "executable"
  | "code"
  | "other";

const VIDEO_EXTS = new Set([
  "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "ts", "3gp", "ogv",
]);
const AUDIO_EXTS = new Set([
  "mp3", "wav", "flac", "aac", "ogg", "m4a", "wma", "alac", "opus", "aiff",
]);
const ARCHIVE_EXTS = new Set([
  "zip", "tar", "gz", "bz2", "xz", "7z", "rar", "tgz", "iso", "dmg", "pkg", "zst",
]);
const IMAGE_EXTS = new Set([
  "jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "tiff", "ico", "avif",
]);
const DOCUMENT_EXTS = new Set([
  "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "txt", "rtf", "epub", "csv",
]);
const EXECUTABLE_EXTS = new Set([
  "exe", "msi", "appimage", "deb", "rpm", "apk", "bat", "cmd", "sh", "bin",
]);
const CODE_EXTS = new Set([
  "js", "ts", "jsx", "tsx", "py", "rs", "go", "cpp", "c", "h", "java", "json", "xml", "html", "css", "yaml", "yml", "sql",
]);

export function getFileCategory(filenameOrUrl: string): FileCategory {
  try {
    const raw = filenameOrUrl.split("?")[0].split("#")[0];
    const parts = raw.split(".");
    if (parts.length > 1) {
      const ext = parts.pop()!.toLowerCase().trim();
      if (VIDEO_EXTS.has(ext)) return "video";
      if (AUDIO_EXTS.has(ext)) return "audio";
      if (ARCHIVE_EXTS.has(ext)) return "archive";
      if (IMAGE_EXTS.has(ext)) return "image";
      if (DOCUMENT_EXTS.has(ext)) return "document";
      if (EXECUTABLE_EXTS.has(ext)) return "executable";
      if (CODE_EXTS.has(ext)) return "code";
    }
  } catch {
    // fallback
  }
  return "other";
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
    // If not a full URL, split by /
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
