/** Inputs accepted by the download engine, including local torrent paths. */
export function isDownloadSource(value: string): boolean {
  if (/^(https?:\/\/|tg:\/\/)/i.test(value)) {
    try { return Boolean(new URL(value).hostname); } catch { return false; }
  }
  return /^magnet:\?/i.test(value) ||
    (/\.torrent$/i.test(value) && /^(?:[a-z]:[\\/]|\\\\|\/)/i.test(value));
}

export function downloadSources(text: string): string[] {
  return text.split(/\r?\n/).map((line) => line.trim()).filter(isDownloadSource);
}

export function remainingTime(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds)) return "Calculating time left";
  const rounded = Math.max(1, Math.ceil(seconds));
  if (rounded < 60) return `${rounded} ${rounded === 1 ? "second" : "seconds"} left`;
  const minutes = Math.ceil(rounded / 60);
  if (minutes < 60) return `${minutes} ${minutes === 1 ? "minute" : "minutes"} left`;
  const hours = Math.ceil(minutes / 60);
  return `${hours} ${hours === 1 ? "hour" : "hours"} left`;
}
