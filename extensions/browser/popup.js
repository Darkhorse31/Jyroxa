import { pingApp, refreshSettingsFromApp, getCachedSettings } from "./native.js";

const api = globalThis.browser ?? globalThis.chrome;
const statusEl = document.getElementById("status-text");
const toggle = document.getElementById("toggle-enabled");
const refreshBtn = document.getElementById("btn-refresh");

async function refreshStatus() {
  statusEl.textContent = "Checking connection…";
  statusEl.className = "muted";
  const ok = await pingApp();
  statusEl.textContent = ok ? "Connected to Jyroxa" : "Jyroxa not reachable";
  statusEl.className = ok ? "muted connected" : "muted disconnected";
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
