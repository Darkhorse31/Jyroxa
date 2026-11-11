import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  api,
  type AddDownloadArgs,
  type AppSettings,
  type ProgressPayload,
} from "./api";
import { DownloadManager, type FilterTab } from "./download-manager";
import { basenameFromUrl } from "./formatters";
import { LinkGrabberView } from "./linkgrabber-view";
import { SettingsView } from "./settings-view";
import { SpeedGraph } from "./speed-graph";
import { showToast } from "./toast";

const THEME_KEY = "swiftget-theme";

// Theme Controller
function getInitialTheme(): "dark" | "light" {
  const saved = localStorage.getItem(THEME_KEY);
  if (saved === "light" || saved === "dark") return saved;
  if (window.matchMedia && window.matchMedia("(prefers-color-scheme: light)").matches) {
    return "light";
  }
  return "dark";
}

function setTheme(theme: "dark" | "light", graph?: SpeedGraph) {
  document.body.setAttribute("data-theme", theme);
  localStorage.setItem(THEME_KEY, theme);
  if (graph) {
    graph.draw();
  }
}

function toggleTheme(graph: SpeedGraph) {
  const current = document.body.getAttribute("data-theme") === "light" ? "light" : "dark";
  const next = current === "dark" ? "light" : "dark";
  setTheme(next, graph);
}

// App Entry Point
async function boot() {
  // Elements
  const speedGraphCanvas = document.getElementById("speed-graph") as HTMLCanvasElement;
  const downloadListEl = document.getElementById("download-list")!;
  const downloadEmptyEl = document.getElementById("download-empty")!;
  const sidebarEl = document.getElementById("sidebar")!;
  const sidebarTitleEl = document.getElementById("sidebar-title")!;
  const sidebarCloseBtn = document.getElementById("sidebar-close")!;
  const grabberPanelEl = document.getElementById("grabber-panel")!;
  const settingsPanelEl = document.getElementById("settings-panel")!;
  const layoutEl = document.querySelector(".layout")!;

  const btnAdd = document.getElementById("btn-add")!;
  const btnGrabber = document.getElementById("btn-grabber")!;
  const btnSettings = document.getElementById("btn-settings")!;
  const btnTheme = document.getElementById("btn-theme")!;

  const dlgAdd = document.getElementById("dlg-add") as HTMLDialogElement;
  const formAdd = document.getElementById("form-add") as HTMLFormElement;
  const addCloseBtn = document.getElementById("add-close-btn")!;
  const addCancelBtn = document.getElementById("add-cancel")!;
  const addUrlsInput = document.getElementById("add-urls") as HTMLTextAreaElement;
  const addFilenameInput = document.getElementById("add-filename") as HTMLInputElement;
  const addFolderInput = document.getElementById("add-folder") as HTMLInputElement;
  const btnPickFolder = document.getElementById("btn-pick-folder")!;
  const addAutoConnCb = document.getElementById("add-auto-conn") as HTMLInputElement;
  const addConnectionsInput = document.getElementById("add-connections") as HTMLInputElement;
  const addSha256Input = document.getElementById("add-sha256") as HTMLInputElement;
  const addMd5Input = document.getElementById("add-md5") as HTMLInputElement;

  const dlgClip = document.getElementById("dlg-clipboard") as HTMLDialogElement;
  const clipTextEl = document.getElementById("clipboard-url-text")!;
  const clipGrabBtn = document.getElementById("clipboard-grab")!;
  const clipQueueBtn = document.getElementById("clipboard-queue")!;
  const clipDismissBtn = document.getElementById("clipboard-dismiss")!;
  const clipCloseBtn = document.getElementById("clip-close-btn")!;

  // Initialize Speed Graph & Managers
  const graph = new SpeedGraph(speedGraphCanvas);
  setTheme(getInitialTheme(), graph);

  // Listen to OS theme changes if user hasn't explicitly set preference
  if (window.matchMedia) {
    window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", (e) => {
      if (!localStorage.getItem(THEME_KEY)) {
        setTheme(e.matches ? "light" : "dark", graph);
      }
    });
  }

  const downloadManager = new DownloadManager(downloadListEl, downloadEmptyEl, graph);

  let currentSettings: AppSettings | null = null;

  const linkGrabberView = new LinkGrabberView(grabberPanelEl, async () => {
    await downloadManager.reload();
  });

  const settingsView = new SettingsView(settingsPanelEl, (updated) => {
    currentSettings = updated;
    linkGrabberView.setSettings(updated);
    addFolderInput.value = updated.default_folder;
  });

  // Load Settings
  try {
    currentSettings = await api.getSettings();
    settingsView.setSettings(currentSettings);
    linkGrabberView.setSettings(currentSettings);
    addFolderInput.value = currentSettings.default_folder;
  } catch (err) {
    console.error("Failed to load settings:", err);
  }

  // Load initial downloads
  await downloadManager.reload();

  // Sidebar management
  let activeSidebar: "grabber" | "settings" | null = null;

  function setSidebar(which: "grabber" | "settings" | null) {
    activeSidebar = which;
    const isOpen = which !== null;
    sidebarEl.classList.toggle("hidden", !isOpen);
    layoutEl.classList.toggle("with-sidebar", isOpen);
    grabberPanelEl.classList.toggle("hidden", which !== "grabber");
    settingsPanelEl.classList.toggle("hidden", which !== "settings");

    if (which === "grabber") {
      sidebarTitleEl.textContent = "LinkGrabber";
      btnGrabber.classList.add("primary");
      btnGrabber.classList.remove("secondary");
      btnSettings.classList.remove("primary");
      btnSettings.classList.add("secondary");
    } else if (which === "settings") {
      sidebarTitleEl.textContent = "Settings";
      btnSettings.classList.add("primary");
      btnSettings.classList.remove("secondary");
      btnGrabber.classList.remove("primary");
      btnGrabber.classList.add("secondary");
    } else {
      btnGrabber.classList.remove("primary");
      btnGrabber.classList.add("secondary");
      btnSettings.classList.remove("primary");
      btnSettings.classList.add("secondary");
    }

    // Trigger graph redraw since layout dimensions may change
    setTimeout(() => graph.draw(), 50);
  }

  btnGrabber.addEventListener("click", () => {
    setSidebar(activeSidebar === "grabber" ? null : "grabber");
  });

  btnSettings.addEventListener("click", () => {
    setSidebar(activeSidebar === "settings" ? null : "settings");
  });

  sidebarCloseBtn.addEventListener("click", () => {
    setSidebar(null);
  });

  btnTheme.addEventListener("click", () => {
    toggleTheme(graph);
  });

  // Filter Tabs & Search
  document.querySelectorAll<HTMLButtonElement>(".tab-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll(".tab-btn").forEach((b) => {
        b.classList.remove("active");
        b.setAttribute("aria-selected", "false");
      });
      btn.classList.add("active");
      btn.setAttribute("aria-selected", "true");
      downloadManager.setFilter((btn.dataset.tab as FilterTab) || "all");
    });
  });

  const searchInput = document.getElementById("downloads-search") as HTMLInputElement;
  searchInput?.addEventListener("input", (e) => {
    downloadManager.setSearch((e.target as HTMLInputElement).value);
  });

  // Add Download Modal
  function openAddModal(prefillUrl?: string) {
    if (prefillUrl) {
      addUrlsInput.value = prefillUrl;
      addFilenameInput.value = basenameFromUrl(prefillUrl);
    } else {
      addUrlsInput.value = "";
      addFilenameInput.value = "";
    }

    if (currentSettings?.default_folder) {
      addFolderInput.value = currentSettings.default_folder;
    }

    addAutoConnCb.checked = true;
    addConnectionsInput.disabled = true;
    addConnectionsInput.value = "8";
    addSha256Input.value = "";
    addMd5Input.value = "";

    dlgAdd.showModal();
    addUrlsInput.focus();
  }

  btnAdd.addEventListener("click", () => openAddModal());
  addCloseBtn.addEventListener("click", () => dlgAdd.close());
  addCancelBtn.addEventListener("click", () => dlgAdd.close());

  addAutoConnCb.addEventListener("change", () => {
    addConnectionsInput.disabled = addAutoConnCb.checked;
  });

  addUrlsInput.addEventListener("blur", () => {
    if (!addFilenameInput.value.trim() && addUrlsInput.value.trim()) {
      const firstUrl = addUrlsInput.value.split("\n")[0].trim();
      if (firstUrl) {
        addFilenameInput.value = basenameFromUrl(firstUrl);
      }
    }
  });

  btnPickFolder.addEventListener("click", async () => {
    const picked = await open({ directory: true, multiple: false });
    if (picked) {
      addFolderInput.value = picked as string;
    }
  });

  formAdd.addEventListener("submit", async (e) => {
    e.preventDefault();
    const urls = addUrlsInput.value
      .split("\n")
      .map((s) => s.trim())
      .filter(Boolean);

    if (urls.length === 0) {
      showToast("Please specify at least one valid URL");
      return;
    }

    const folder = addFolderInput.value.trim() || currentSettings?.default_folder || ".";
    const filename = addFilenameInput.value.trim() || basenameFromUrl(urls[0]);
    const connections = Number(addConnectionsInput.value) || 8;
    const connections_auto = addAutoConnCb.checked;
    const sha256 = addSha256Input.value.trim() || undefined;
    const md5 = addMd5Input.value.trim() || undefined;

    const args: AddDownloadArgs = {
      urls,
      output_dir: folder,
      filename,
      connections,
      connections_auto,
      sha256,
      md5,
    };

    try {
      await api.addDownload(args);
      dlgAdd.close();
      showToast(`Download queued: ${filename}`);
      await downloadManager.reload();
    } catch (err) {
      showToast(`Error adding download: ${err}`);
    }
  });

  // Clipboard Dialog
  clipDismissBtn.addEventListener("click", () => dlgClip.close());
  clipCloseBtn.addEventListener("click", () => dlgClip.close());

  clipGrabBtn.addEventListener("click", () => {
    const url = clipTextEl.textContent || "";
    dlgClip.close();
    setSidebar("grabber");
    linkGrabberView.setInputUrl(url);
  });

  clipQueueBtn.addEventListener("click", () => {
    const url = clipTextEl.textContent || "";
    dlgClip.close();
    openAddModal(url);
  });

  // Global Keyboard Shortcuts
  window.addEventListener("keydown", (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") {
      e.preventDefault();
      openAddModal();
    } else if (e.key === "Escape") {
      if (dlgAdd.open) dlgAdd.close();
      else if (dlgClip.open) dlgClip.close();
      else if (activeSidebar !== null) setSidebar(null);
    }
  });

  // Listen to Tauri Backend Events
  await listen("downloads-changed", async () => {
    await downloadManager.reload();
  });

  await listen<ProgressPayload>("download-progress", (ev) => {
    const { id, event } = ev.payload;
    downloadManager.handleProgress(id, event);
  });

  await listen<string>("clipboard-url", (ev) => {
    clipTextEl.textContent = ev.payload;
    dlgClip.showModal();
  });
}

void boot();
