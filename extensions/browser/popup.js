import {
  sendNative,
  pingFailureFromResponse,
  refreshSettingsFromApp,
  getCachedSettings,
} from "./native.js";

const api = globalThis.browser ?? globalThis.chrome;
const statusEl = document.getElementById("status-text");
const statusDetailEl = document.getElementById("status-detail");
const toggle = document.getElementById("toggle-enabled");
const toggleSilent = document.getElementById("toggle-silent");
const toggleVideoGrab = document.getElementById("toggle-video-grab");
const refreshBtn = document.getElementById("btn-refresh");
const extIdEl = document.getElementById("ext-id");

if (extIdEl) {
  try {
    extIdEl.textContent = api.runtime.id;
  } catch {
    extIdEl.textContent = "";
  }
}

async function pingWithRetry() {
  const deadline = Date.now() + 4500;
  let last = null;
  while (Date.now() < deadline) {
    last = await sendNative({ type: "ping" });
    if (last?.ok && last.connected) return last;
    await new Promise((r) => setTimeout(r, 600));
  }
  return last;
}

async function refreshStatus() {
  statusEl.textContent = "Checking connection…";
  statusEl.className = "muted";
  if (statusDetailEl) {
    statusDetailEl.textContent = "";
    statusDetailEl.className = "muted";
  }
  const res = await pingWithRetry();
  const ok = Boolean(res?.ok && res.connected);
  if (ok) {
    statusEl.textContent = "Connected to Jyroxa";
    statusEl.className = "muted connected";
    if (statusDetailEl) statusDetailEl.textContent = "";
  } else {
    const reason = pingFailureFromResponse(res);
    statusEl.textContent = reason?.short ?? "Jyroxa not reachable";
    statusEl.className = "muted disconnected";
    const extra =
      " Reload this extension on chrome://extensions, then fully quit Chrome (all windows) and reopen. Your extension ID is shown below — add it in Jyroxa Settings → Browser if needed.";
    if (statusDetailEl) {
      statusDetailEl.textContent = `${reason?.detail || ""}${extra}`.trim();
      statusDetailEl.className = "muted disconnected";
    }
  }
  await refreshSettingsFromApp();
  const s = getCachedSettings();
  toggle.checked = s.enabled !== false;
  if (toggleSilent) toggleSilent.checked = Boolean(s.silentAdd);
  if (toggleVideoGrab) {
    const local = await api.storage.local.get("jyroxaVideoGrab");
    const fromApp = s.videoGrabEnabled !== false;
    toggleVideoGrab.checked = local.jyroxaVideoGrab !== false && fromApp;
  }
}

toggle.addEventListener("change", async () => {
  const enabled = toggle.checked;
  await api.storage.local.set({ jyroxaEnabled: enabled });
  const local = await api.storage.local.get("jyroxaSettings");
  const merged = { ...(local.jyroxaSettings || {}), enabled };
  await api.storage.local.set({ jyroxaSettings: merged });
});

toggleVideoGrab?.addEventListener("change", async () => {
  const on = toggleVideoGrab.checked;
  await api.storage.local.set({ jyroxaVideoGrab: on });
  const tabs = await api.tabs.query({});
  for (const t of tabs) {
    if (t.id != null) {
      try {
        await api.tabs.sendMessage(t.id, { type: "video-grab-refresh" });
      } catch {
      }
    }
  }
});

toggleSilent?.addEventListener("change", async () => {
  const silent_add = toggleSilent.checked;
  const res = await sendNative({ type: "set_silent_add", silent_add });
  if (!res?.ok) {
    toggleSilent.checked = !silent_add;
    if (statusDetailEl) {
      statusDetailEl.textContent = res?.error || "Could not update Jyroxa setting.";
      statusDetailEl.className = "muted disconnected";
    }
    return;
  }
  const local = await api.storage.local.get("jyroxaSettings");
  const merged = { ...(local.jyroxaSettings || {}), silent_add };
  await api.storage.local.set({ jyroxaSettings: merged });
});

refreshBtn.addEventListener("click", () => {
  void refreshStatus();
});

void refreshStatus();