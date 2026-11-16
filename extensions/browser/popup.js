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
const refreshBtn = document.getElementById("btn-refresh");

async function refreshStatus() {
  statusEl.textContent = "Checking connection…";
  statusEl.className = "muted";
  if (statusDetailEl) {
    statusDetailEl.textContent = "";
    statusDetailEl.className = "muted";
  }
  const res = await sendNative({ type: "ping" });
  const ok = Boolean(res?.ok && res.connected);
  if (ok) {
    statusEl.textContent = "Connected to Jyroxa";
    statusEl.className = "muted connected";
    if (statusDetailEl) statusDetailEl.textContent = "";
  } else {
    const reason = pingFailureFromResponse(res);
    statusEl.textContent = reason?.short ?? "Jyroxa not reachable";
    statusEl.className = "muted disconnected";
    if (statusDetailEl && reason?.detail) {
      statusDetailEl.textContent = reason.detail;
      statusDetailEl.className = "muted disconnected";
    }
  }
  await refreshSettingsFromApp();
  const s = getCachedSettings();
  toggle.checked = s.enabled !== false;
}

toggle.addEventListener("change", async () => {
  const enabled = toggle.checked;
  await api.storage.local.set({ jyroxaEnabled: enabled });
  const local = await api.storage.local.get("jyroxaSettings");
  const merged = { ...(local.jyroxaSettings || {}), enabled };
  await api.storage.local.set({ jyroxaSettings: merged });
});

refreshBtn.addEventListener("click", () => {
  void refreshStatus();
});

void refreshStatus();