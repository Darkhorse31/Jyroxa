import type { VideoProgress } from "./api";

// Keep video phase/size presentation independent of HTTP and torrent progress.
export function videoProgressText(p: VideoProgress, bytes: (n: number) => string, time: (n: number | null) => string) {
  const total = p.total && p.total > 0 ? p.total : null;
  const estimated = total && p.total_is_estimate ? "~" : "";
  const size = `${bytes(p.downloaded)}${total ? ` of ${estimated}${bytes(total)}` : " downloaded"}`;
  const percent = total ? `${estimated}${Math.min(100, p.downloaded / total * 100).toFixed(1)}%` : "—";
  if (p.phase === "fetching") return { summary: "Fetching video information…", percent, size };
  if (p.phase === "merging" || p.phase === "processing") return {
    summary: `${p.phase === "merging" ? "Merging audio and video…" : "Processing video…"} · ${size}`,
    percent, size,
  };
  const remaining = total ? `, ${estimated}${bytes(Math.max(0, total - p.downloaded))} remaining` : ", calculating total size";
  return { summary: `${size}${remaining}, ${time(p.eta_secs)}`, percent, size };
}
