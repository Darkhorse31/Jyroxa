/** Dedupe Chrome download handoffs (onCreated vs onDeterminingFilename). */

export function extOf(nameOrUrl) {
  let base = (nameOrUrl || "").split(/[?#]/)[0];
  try {
    if (/^https?:\/\//i.test(base)) {
      base = new URL(base).pathname;
    }
  } catch {
    /* plain filename */
  }
  const slash = base.lastIndexOf("/");
  const leaf = slash >= 0 ? base.slice(slash + 1) : base;
  const dot = leaf.lastIndexOf(".");
  if (dot <= 0) return "";
  return leaf.slice(dot + 1).toLowerCase();
}

export function urlHasKnownExtension(url) {
  return extOf(url) !== "";
}

export function createHandoffTracker() {
  const claimed = new Set();
  return {
    tryClaim(downloadId) {
      if (claimed.has(downloadId)) return false;
      claimed.add(downloadId);
      return true;
    },
    isClaimed(downloadId) {
      return claimed.has(downloadId);
    },
    release(downloadId) {
      claimed.delete(downloadId);
    },
  };
}