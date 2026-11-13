const api = globalThis.browser ?? globalThis.chrome;
const HOST = "com.jyroxa.browser";

const DEFAULT_SETTINGS = {
  enabled: true,
  minSizeBytes: 0,
  fileTypes: {
    archives: true,
    programs: true,
    video: true,
    audio: true,
    documents: true,
    iso: true,
  },
  silentAdd: false,
  disabledHosts: [],
};

let cachedSettings = { ...DEFAULT_SETTINGS, disabledHosts: [] };
let lastSettingsFetch = 0;

function mapSettings(raw) {
  if (!raw) return { ...DEFAULT_SETTINGS, disabledHosts: [] };
  const ft = raw.file_types || raw.fileTypes || {};
  return {
    enabled: raw.enabled !== false,
    minSizeBytes: raw.min_size_bytes ?? raw.minSizeBytes ?? 0,
    fileTypes: {
      archives: ft.archives !== false,
      programs: ft.programs !== false,
      video: ft.video !== false,
      audio: ft.audio !== false,
      documents: ft.documents !== false,
      iso: ft.iso !== false,
    },
    silentAdd: Boolean(raw.silent_add ?? raw.silentAdd),
    disabledHosts: raw.disabled_hosts || raw.disabledHosts || [],
  };
}

export async function sendNative(payload) {
  return new Promise((resolve) => {
    const port = api.runtime.connectNative(HOST);
    const timer = setTimeout(() => {
      try {
        port.disconnect();
      } catch {
      }
      resolve({ ok: false, error: "timeout" });
    }, 12_000);

    port.onMessage.addListener((msg) => {
      clearTimeout(timer);
      try {
        port.disconnect();
      } catch {
      }
      resolve(msg);
    });
    port.onDisconnect.addListener(() => {
      clearTimeout(timer);
      const err = api.runtime.lastError?.message;
      resolve({ ok: false, error: err || "disconnected" });
    });
    port.postMessage(payload);
  });
}

export async function refreshSettingsFromApp() {
  if (Date.now() - lastSettingsFetch < 5000) {
    return cachedSettings;
  }
  const local = await api.storage.local.get(["jyroxaEnabled", "jyroxaSettings"]);
  if (local.jyroxaSettings) {
    cachedSettings = mapSettings(local.jyroxaSettings);
  }
  const res = await sendNative({ type: "get_settings" });
  if (res?.ok && res.settings) {
    cachedSettings = mapSettings(res.settings);
    await api.storage.local.set({ jyroxaSettings: res.settings });
    lastSettingsFetch = Date.now();
  }
  if (local.jyroxaEnabled === false) {
    cachedSettings.enabled = false;
  }
  return cachedSettings;
}

export async function pingApp() {
  const res = await sendNative({ type: "ping" });
  return Boolean(res?.ok && res.connected);
}

export function extEnabled() {
  return cachedSettings.enabled !== false;
}

export function getCachedSettings() {
  return cachedSettings;
}
