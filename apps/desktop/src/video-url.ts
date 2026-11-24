/** Match playable YouTube pages, never lookalike hosts or arbitrary web pages. */
export function isYoutubePage(input: string): boolean {
  try {
    const url = new URL(input);
    if (!["http:", "https:"].includes(url.protocol)) return false;
    if (url.hostname === "youtu.be") return url.pathname.length > 1;
    return ["youtube.com", "www.youtube.com", "m.youtube.com", "music.youtube.com"].includes(url.hostname)
      && ((url.pathname === "/watch" && Boolean(url.searchParams.get("v")))
        || ["/shorts/", "/live/", "/embed/"].some(p => url.pathname.startsWith(p) && url.pathname.length > p.length));
  } catch { return false; }
}
