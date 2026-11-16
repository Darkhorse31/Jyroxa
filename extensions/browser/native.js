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

export function connectionFailureReason(errText) {
  const m = (errText || "").toLowerCase();
  if (!m) {
    return {
      short: "Jyroxa not reachable",
      detail: "No response from the native messaging host.",
    };
  }
  if (
    m.includes("native messaging host not found") ||
    m.includes("specify native messaging host") ||
    m.includes("not found")
  ) {
    return {
      short: "Host not registered",
      detail:
        "Chrome cannot find com.jyroxa.browser. Install Jyroxa or restart the desktop app to register the host.",
    };
  }
  if (m.includes("forbidden") || m.includes("access to the specified native messaging host")) {
    return {
      short: "Extension not allowed",
      detail:
        "The native host rejected this extension ID. Load the official Jyroxa extension or add your ID in Jyroxa settings.",
    };
  }
  if (m.includes("native host not installed")) {
    return {
      short: "Host not configured",
      detail: "Jyroxa has not written the native messaging manifest yet. Start the desktop app once.",
    };
  }
  if (m.includes("timeout")) {
    return {
      short: "No response",
      detail: "The native host did not answer in time. Jyroxa may not be running.",
    };
  }
  if (m.includes("pipe") || m.includes("unavailable") || m.includes("disconnected")) {
    return {
      short: "Jyroxa not running",
      detail: "Start Jyroxa on this PC, then refresh the connection.",
    };
  }
  return {
    short: "Jyroxa not reachable",
    detail: errText,
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

export function pingFailureFromResponse(res) {
  if (res?.ok && res.connected) {
    return null;
  }
  if (res?.error) {
    return connectionFailureReason(res.error);
  }
  return connectionFailureReason("disconnected");
}

export function extEnabled() {
  return cachedSettings.enabled !== false;
}

export function getCachedSettings() {
  return cachedSettings;
}