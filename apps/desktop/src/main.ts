import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import {
  api,
  openFolder,
  openDownloadedFile,
  type AddDownloadArgs,
  type AppSettings,
  type DownloadFinishedPayload,
  type ProgressPayload,
} from "./api";
import { APP_NAME } from "./brand";
import {
  DownloadManager,
  type CategoryFilter,
  type SortField,
  type SortOrder,
  type StatusFilter,
} from "./download-manager";
import { basenameFromUrl, fmtBytes } from "./formatters";
import { LinkGrabberView } from "./linkgrabber-view";
import { SettingsView } from "./settings-view";
import { SpeedGraph } from "./speed-graph";
import { dispatchSystemNotification, showToast } from "./toast";

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
  // Set window & brand titles
  document.title = APP_NAME;
  const brandTitleEl = document.getElementById("brand-title");
  if (brandTitleEl) brandTitleEl.textContent = APP_NAME;
  const aboutAppNameEl = document.getElementById("about-app-name");
  if (aboutAppNameEl) aboutAppNameEl.textContent = APP_NAME;

  // Try retrieving backend version
  try {
    const { getVersion } = await import("@tauri-apps/api/app");
    const v = await getVersion();
    const aboutVersionEl = document.getElementById("about-version");
    if (aboutVersionEl && v) aboutVersionEl.textContent = `v${v}`;
  } catch {
    // fallback
  }

  // DOM Elements
  const speedGraphCanvas = document.getElementById("speed-graph") as HTMLCanvasElement;
  const downloadListEl = document.getElementById("download-list")!;
  const downloadEmptyEl = document.getElementById("download-empty")!;
  const sidebarEl = document.getElementById("sidebar")!;
  const sidebarTitleEl = document.getElementById("sidebar-title")!;
  const sidebarCloseBtn = document.getElementById("sidebar-close")!;
  const grabberPanelEl = document.getElementById("grabber-panel")!;
  const settingsPanelEl = document.getElementById("settings-panel")!;

  const btnAdd = document.getElementById("btn-add")!;
  const btnGrabber = document.getElementById("btn-grabber")!;
  const btnSettings = document.getElementById("btn-settings")!;
  const btnShortcuts = document.getElementById("btn-shortcuts")!;
  const btnAbout = document.getElementById("btn-about")!;
  const btnTheme = document.getElementById("btn-theme")!;

  const searchInput = document.getElementById("downloads-search") as HTMLInputElement;
  const sortSelect = document.getElementById("sort-select") as HTMLSelectElement;
  const btnSortDir = document.getElementById("btn-sort-dir") as HTMLButtonElement;

  // Add Dialog Elements
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

  // Live probe elements in Add Dialog
  const addProbeCard = document.getElementById("add-probe-card")!;
  const probeLoadingEl = document.getElementById("probe-loading")!;
  const probeDetailsEl = document.getElementById("probe-details")!;
  const probeSizeEl = document.getElementById("probe-size")!;
  const probeMimeEl = document.getElementById("probe-mime")!;
  const probeResumableEl = document.getElementById("probe-resumable")!;
  const probeCategoryEl = document.getElementById("probe-category")!;
  const probeErrorEl = document.getElementById("probe-error")!;
  const probeErrorMsg = document.getElementById("probe-error-msg")!;
  const btnRetryProbe = document.getElementById("btn-retry-probe")!;

  // Clipboard Dialog
  const dlgClip = document.getElementById("dlg-clipboard") as HTMLDialogElement;
  const clipTextEl = document.getElementById("clipboard-url-text")!;
  const clipGrabBtn = document.getElementById("clipboard-grab")!;
  const clipQueueBtn = document.getElementById("clipboard-queue")!;
  const clipDismissBtn = document.getElementById("clipboard-dismiss")!;
  const clipCloseBtn = document.getElementById("clip-close-btn")!;

  // Shortcuts Dialog
  const dlgShortcuts = document.getElementById("dlg-shortcuts") as HTMLDialogElement;
  const shortcutsCloseBtn = document.getElementById("shortcuts-close-btn")!;
  const shortcutsOkBtn = document.getElementById("shortcuts-ok-btn")!;

  // About Dialog
  const dlgAbout = document.getElementById("dlg-about") as HTMLDialogElement;
  const aboutCloseBtn = document.getElementById("about-close-btn")!;
  const aboutOkBtn = document.getElementById("about-ok-btn")!;

  // Drag and drop overlay
  const dragOverlay = document.getElementById("drag-overlay")!;

  // Initialize Speed Graph & Theme
  const graph = new SpeedGraph(speedGraphCanvas);
  setTheme(getInitialTheme(), graph);

  if (window.matchMedia) {
    window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", (e) => {
      if (!localStorage.getItem(THEME_KEY)) {
        setTheme(e.matches ? "light" : "dark", graph);
      }
    });
  }

  // Add-download dirty flags
  let filenameUserEdited = false;
  let folderUserEdited = false;
  let probeDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let lastProbedUrl = "";

  // Initialize Download Manager
  const downloadManager = new DownloadManager(
    downloadListEl,
    downloadEmptyEl,
    graph,
    () => openAddModal()
  );

  let currentSettings: AppSettings | null = null;
  let categoryDirsMap: Record<string, string> = {};

  const linkGrabberView = new LinkGrabberView(grabberPanelEl, async () => {
    await downloadManager.reload();
  });

  const settingsView = new SettingsView(settingsPanelEl, (updated) => {
    currentSettings = updated;
    linkGrabberView.setSettings(updated);
    categoryDirsMap = updated.category_dirs || {};
    if (!folderUserEdited) {
      addFolderInput.value = updated.download_dir || updated.default_folder || "";
    }
  });

  // Load Settings and Default Category Dirs
  try {
    currentSettings = await api.getSettings();
    settingsView.setSettings(currentSettings);
    linkGrabberView.setSettings(currentSettings);
    categoryDirsMap = currentSettings.category_dirs || {};

    try {
      const defDirs = await api.defaultCategoryDirs();
      if (defDirs && Object.keys(defDirs).length > 0) {
        categoryDirsMap = { ...defDirs, ...categoryDirsMap };
      }
    } catch {
      // optional
    }

    addFolderInput.value = currentSettings.download_dir || currentSettings.default_folder || "";
  } catch (err) {
    console.error("Failed to load settings:", err);
  }

  // Load initial downloads
  await downloadManager.reload();

  // Sidebar Drawer (LinkGrabber & Settings)
  let activeDrawer: "grabber" | "settings" | null = null;

  function setDrawer(which: "grabber" | "settings" | null) {
    activeDrawer = which;
    const isOpen = which !== null;
    sidebarEl.classList.toggle("hidden", !isOpen);
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

    setTimeout(() => graph.draw(), 50);
  }

  btnGrabber.addEventListener("click", () => {
    setDrawer(activeDrawer === "grabber" ? null : "grabber");
  });

  btnSettings.addEventListener("click", () => {
    setDrawer(activeDrawer === "settings" ? null : "settings");
  });

  sidebarCloseBtn.addEventListener("click", () => {
    setDrawer(null);
  });

  btnTheme.addEventListener("click", () => {
    toggleTheme(graph);
  });

  btnShortcuts.addEventListener("click", () => {
    dlgShortcuts.showModal();
  });
  shortcutsCloseBtn.addEventListener("click", () => dlgShortcuts.close());
  shortcutsOkBtn.addEventListener("click", () => dlgShortcuts.close());

  btnAbout.addEventListener("click", () => {
    dlgAbout.showModal();
  });
  aboutCloseBtn.addEventListener("click", () => dlgAbout.close());
  aboutOkBtn.addEventListener("click", () => dlgAbout.close());

  // Left Sidebar Filter Clicks
  document.querySelectorAll<HTMLButtonElement>(".sidebar-item[data-status]").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll(".sidebar-item[data-status]").forEach((b) => {
        b.classList.remove("active");
        b.setAttribute("aria-selected", "false");
      });
      btn.classList.add("active");
      btn.setAttribute("aria-selected", "true");
      downloadManager.setStatusFilter((btn.dataset.status as StatusFilter) || "all");
    });
  });

  document.querySelectorAll<HTMLButtonElement>(".sidebar-item[data-category]").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll(".sidebar-item[data-category]").forEach((b) => {
        b.classList.remove("active");
        b.setAttribute("aria-selected", "false");
      });
      btn.classList.add("active");
      btn.setAttribute("aria-selected", "true");
      downloadManager.setCategoryFilter((btn.dataset.category as CategoryFilter) || "all");
    });
  });

  // Search & Sort
  searchInput?.addEventListener("input", (e) => {
    downloadManager.setSearch((e.target as HTMLInputElement).value);
  });

  let currentSortOrder: SortOrder = "desc";
  sortSelect?.addEventListener("change", () => {
    downloadManager.setSorting(sortSelect.value as SortField, currentSortOrder);
  });

  btnSortDir?.addEventListener("click", () => {
    currentSortOrder = currentSortOrder === "asc" ? "desc" : "asc";
    btnSortDir.title = currentSortOrder === "asc" ? "Sort ascending" : "Sort descending";
    btnSortDir.innerHTML = currentSortOrder === "asc"
      ? `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="19" x2="12" y2="5"></line><polyline points="5 12 12 5 19 12"></polyline></svg>`
      : `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"></line><polyline points="19 12 12 19 5 12"></polyline></svg>`;
    downloadManager.setSorting(sortSelect.value as SortField, currentSortOrder);
  });

  // Live URL Probing and Auto-naming in Add Modal
  async function triggerProbe(url: string) {
    const trimmed = url.trim();
    if (!trimmed || !trimmed.startsWith("http")) {
      addProbeCard.classList.add("hidden");
      return;
    }

    lastProbedUrl = trimmed;
    addProbeCard.classList.remove("hidden");
    probeLoadingEl.classList.remove("hidden");
    probeDetailsEl.classList.add("hidden");
    probeErrorEl.classList.add("hidden");

    try {
      const info = await api.probeUrl(trimmed);
      probeLoadingEl.classList.add("hidden");
      probeDetailsEl.classList.remove("hidden");

      // Show detected metadata
      probeSizeEl.textContent = info.size ? `Size: ${fmtBytes(info.size)}` : "Size: Unknown";
      probeMimeEl.textContent = info.mime ? `Type: ${info.mime}` : `Type: ${info.category}`;
      probeResumableEl.textContent = info.resumable ? "Resumable" : "Non-resumable";
      probeResumableEl.className = info.resumable ? "probe-badge yes" : "probe-badge no";
      probeCategoryEl.textContent = info.category;
      probeCategoryEl.className = `probe-badge cat cat-${info.category.toLowerCase()}`;

      // Auto-fill filename unless user edited it
      if (!filenameUserEdited && info.suggested_name) {
        let finalName = info.suggested_name;
        const currentFolder = addFolderInput.value.trim() || currentSettings?.download_dir || ".";
        try {
          const suggested = await api.suggestSavePath(currentFolder, info.suggested_name);
          const base = suggested.replace(/^.*[\\\/]/, "");
          if (base) finalName = base;
        } catch {
          // fallback
        }
        addFilenameInput.value = finalName;
      }

      // Auto-select category folder unless user manually edited folder
      if (!folderUserEdited && info.category) {
        const catFolder = categoryDirsMap[info.category];
        if (catFolder) {
          addFolderInput.value = catFolder;
        }
      }
    } catch (err: any) {
      probeLoadingEl.classList.add("hidden");
      probeErrorEl.classList.remove("hidden");
      probeErrorMsg.textContent = String(err?.message || err || "Probing failed");

      // Fallback filename from url
      if (!filenameUserEdited && !addFilenameInput.value.trim()) {
        addFilenameInput.value = basenameFromUrl(trimmed);
      }
    }
  }

  function debouncedProbe(url: string) {
    if (probeDebounceTimer) clearTimeout(probeDebounceTimer);
    probeDebounceTimer = setTimeout(() => {
      triggerProbe(url);
    }, 400);
  }

  addUrlsInput.addEventListener("input", () => {
    const firstUrl = addUrlsInput.value.split("\n")[0].trim();
    if (firstUrl) {
      debouncedProbe(firstUrl);
    } else {
      addProbeCard.classList.add("hidden");
    }
  });

  addFilenameInput.addEventListener("input", () => {
    filenameUserEdited = true;
  });

  addFolderInput.addEventListener("input", () => {
    folderUserEdited = true;
  });

  btnRetryProbe.addEventListener("click", () => {
    if (lastProbedUrl) triggerProbe(lastProbedUrl);
  });

  // Quick category folder buttons in Add dialog
  document.querySelectorAll<HTMLButtonElement>(".btn-quick-cat").forEach((btn) => {
    btn.addEventListener("click", () => {
      const cat = btn.dataset.cat;
      if (cat && categoryDirsMap[cat]) {
        addFolderInput.value = categoryDirsMap[cat];
        folderUserEdited = true;
      }
    });
  });

  // Open Add Modal
  function openAddModal(prefillUrl?: string) {
    filenameUserEdited = false;
    folderUserEdited = false;
    addProbeCard.classList.add("hidden");

    if (prefillUrl) {
      addUrlsInput.value = prefillUrl;
      addFilenameInput.value = basenameFromUrl(prefillUrl);
      debouncedProbe(prefillUrl);
    } else {
      addUrlsInput.value = "";
      addFilenameInput.value = "";
    }

    if (currentSettings) {
      addFolderInput.value = currentSettings.download_dir || currentSettings.default_folder || "";
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

  btnPickFolder.addEventListener("click", async () => {
    const picked = await open({ directory: true, multiple: false });
    if (picked) {
      addFolderInput.value = picked as string;
      folderUserEdited = true;
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

    const folder = addFolderInput.value.trim() || currentSettings?.download_dir || ".";
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

  // Clipboard Dialog Handlers
  clipDismissBtn.addEventListener("click", () => dlgClip.close());
  clipCloseBtn.addEventListener("click", () => dlgClip.close());

  clipGrabBtn.addEventListener("click", () => {
    const url = clipTextEl.textContent || "";
    dlgClip.close();
    setDrawer("grabber");
    linkGrabberView.setInputUrl(url);
  });

  clipQueueBtn.addEventListener("click", () => {
    const url = clipTextEl.textContent || "";
    dlgClip.close();
    openAddModal(url);
  });

  // Drag and Drop Handling
  let dragCounter = 0;
  window.addEventListener("dragenter", (e) => {
    e.preventDefault();
    dragCounter++;
    dragOverlay.classList.remove("hidden");
  });

  window.addEventListener("dragleave", (e) => {
    e.preventDefault();
    dragCounter--;
    if (dragCounter <= 0) {
      dragCounter = 0;
      dragOverlay.classList.add("hidden");
    }
  });

  window.addEventListener("dragover", (e) => {
    e.preventDefault();
  });

  window.addEventListener("drop", (e) => {
    e.preventDefault();
    dragCounter = 0;
    dragOverlay.classList.add("hidden");

    const text =
      e.dataTransfer?.getData("text/uri-list") ||
      e.dataTransfer?.getData("text/plain") ||
      "";

    if (text) {
      const urls = text.split("\n").map((s) => s.trim()).filter((s) => s.startsWith("http"));
      if (urls.length === 1) {
        openAddModal(urls[0]);
      } else if (urls.length > 1) {
        setDrawer("grabber");
        linkGrabberView.setInputUrl(text);
      }
    }
  });

  // Global Keyboard Shortcuts
  window.addEventListener("keydown", async (e) => {
    const activeEl = document.activeElement;
    const isInputActive =
      activeEl instanceof HTMLInputElement ||
      activeEl instanceof HTMLTextAreaElement ||
      activeEl instanceof HTMLSelectElement;

    // Ctrl+N: New download
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") {
      e.preventDefault();
      openAddModal();
    }
    // Ctrl+V: Paste URL when outside input
    else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "v" && !isInputActive) {
      try {
        const text = await readText();
        if (text && (text.startsWith("http://") || text.startsWith("https://"))) {
          e.preventDefault();
          openAddModal(text.trim());
        }
      } catch {
        // clipboard access
      }
    }
    // Ctrl+F: Focus search
    else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
      e.preventDefault();
      searchInput.focus();
      searchInput.select();
    }
    // Ctrl+, : Settings
    else if ((e.ctrlKey || e.metaKey) && e.key === ",") {
      e.preventDefault();
      setDrawer("settings");
    }
    // Ctrl+A: Select all downloads
    else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a" && !isInputActive) {
      e.preventDefault();
      downloadManager.selectAll();
    }
    // Space: Pause/Resume selected downloads
    else if (e.key === " " && !isInputActive) {
      e.preventDefault();
      const selectedIds = downloadManager.getSelectedIds();
      if (selectedIds.length > 0) {
        downloadManager.pauseSelected();
      }
    }
    // Delete: Remove selected downloads
    else if (e.key === "Delete" && !isInputActive) {
      e.preventDefault();
      const selectedIds = downloadManager.getSelectedIds();
      if (selectedIds.length > 0) {
        downloadManager.removeSelected(false);
      }
    }
    // ?: Help shortcuts
    else if (e.key === "?" && !isInputActive) {
      e.preventDefault();
      dlgShortcuts.showModal();
    }
    // Escape: Close dialogs / clear selection / close drawer
    else if (e.key === "Escape") {
      if (dlgAdd.open) dlgAdd.close();
      else if (dlgClip.open) dlgClip.close();
      else if (dlgShortcuts.open) dlgShortcuts.close();
      else if (dlgAbout.open) dlgAbout.close();
      else if (downloadManager.getSelectedIds().length > 0) {
        downloadManager.clearSelection();
      } else if (activeDrawer !== null) {
        setDrawer(null);
      }
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

  // Download finished event: show interactive toast + web notification
  await listen<DownloadFinishedPayload>("download-finished", (ev) => {
    const { path, filename, total } = ev.payload;

    // Interactive in-app toast with "Open" and "Show in folder"
    showToast({
      title: "Download Finished",
      message: `${filename} (${fmtBytes(total)})`,
      type: "success",
      duration: 6000,
      actions: [
        {
          label: "Open",
          primary: true,
          onClick: async () => {
            await openDownloadedFile(path);
          },
        },
        {
          label: "Show in folder",
          onClick: async () => {
            await openFolder(path);
          },
        },
      ],
    });

    // Native / Web Notification
    if (currentSettings?.notifications) {
      dispatchSystemNotification(APP_NAME, {
        body: `Finished downloading ${filename}`,
        onClick: () => {
          openFolder(path);
        },
      });
    }
  });

  // Clipboard detected event
  await listen<string>("clipboard-url", (ev) => {
    clipTextEl.textContent = ev.payload;
    dlgClip.showModal();
  });
}

void boot();
