import { open } from "@tauri-apps/plugin-dialog";
import {
  api,
  type AddDownloadArgs,
  type AppSettings,
  type FileCategory,
  type GrabOptionsDto,
  type LinkInfo,
} from "./api";
import {
  basenameFromUrl,
  extractHost,
  fmtBytes,
  getCategoryIcon,
  getFileCategory,
} from "./formatters";
import { icons } from "./icons";
import { showToast } from "./toast";

interface LinkItemState {
  link: LinkInfo;
  packageName: string;
  selected: boolean;
  filename: string;
  category: FileCategory;
  isDirty: boolean;
}

/** A probed size, or null when the server did not report one. */
function linkSize(link: LinkInfo): number | null {
  return typeof link.size === "number" && link.size > 0 ? link.size : null;
}

function fmtLinkSize(link: LinkInfo): string {
  const n = linkSize(link);
  return n == null ? "Unknown size" : fmtBytes(n);
}

/** Sum of the known sizes; "+" marks a total that leaves out unknown sizes. */
function fmtTotalSize(links: LinkInfo[]): string {
  const known = links.map(linkSize).filter((n): n is number => n != null);
  if (known.length === 0) return links.length ? "Unknown size" : fmtBytes(0);
  const total = fmtBytes(known.reduce((a, b) => a + b, 0));
  return known.length < links.length ? `${total}+` : total;
}

export class LinkGrabberView {
  private container: HTMLElement;
  private settings: AppSettings | null = null;
  private onDownloadsChanged: () => Promise<void>;

  private itemStates: LinkItemState[] = [];
  private activeCategoryFilter: string = "all";
  private resultSearchQuery = "";
  private isProbing = false;

  constructor(container: HTMLElement, onDownloadsChanged: () => Promise<void>) {
    this.container = container;
    this.onDownloadsChanged = onDownloadsChanged;
    this.renderInitial();
  }

  public setSettings(s: AppSettings) {
    this.settings = s;
    const folderInput = this.container.querySelector("#grab-folder") as HTMLInputElement;
    if (folderInput && !folderInput.value) {
      folderInput.value = s.download_dir || s.default_folder || "";
    }
  }

  public setInputUrl(url: string) {
    const inputEl = this.container.querySelector("#grab-input") as HTMLTextAreaElement;
    if (inputEl) {
      inputEl.value = url;
    }
  }

  private renderInitial() {
    this.container.innerHTML = `
      <div class="panel-section-head">
        <div class="panel-section-title">
          ${icons.link(18)}
          <h3>LinkGrabber</h3>
        </div>
        <p class="panel-section-subtitle">
          Paste URLs, raw links, or web page source to probe and download multiple files.
        </p>
      </div>

      <div class="grabber-input-card">
        <label class="input-label" for="grab-input">URLs or Text to crawl</label>
        <textarea
          id="grab-input"
          class="form-textarea"
          rows="4"
          placeholder="https://example.com/file.zip&#10;https://example.com/downloads/&#10;Paste text or HTML here..."
        ></textarea>

        <div class="grabber-filters-grid">
          <div class="filter-col">
            <label class="check-label">
              <input type="checkbox" id="grab-deep" class="form-checkbox" />
              <span>Deep crawl (follow 1 level)</span>
            </label>
          </div>

          <div class="filter-col">
            <label class="input-label" for="grab-host">Host filter</label>
            <input type="text" id="grab-host" class="form-input" placeholder="e.g. example.com" />
          </div>

          <div class="filter-col">
            <label class="input-label" for="grab-ext">Extensions</label>
            <input type="text" id="grab-ext" class="form-input" placeholder="zip, mp4, iso, mkv" />
          </div>

          <div class="filter-col">
            <label class="input-label" for="grab-min">Min size (bytes)</label>
            <input type="number" id="grab-min" class="form-input" min="0" placeholder="0" />
          </div>

          <div class="filter-col">
            <label class="input-label" for="grab-max">Max size (bytes)</label>
            <input type="number" id="grab-max" class="form-input" min="0" placeholder="Optional" />
          </div>
        </div>

        <div class="grabber-form-actions">
          <button type="button" id="grab-run" class="btn primary">
            ${icons.search(15)} Probe links
          </button>
          <button type="button" id="grab-clear" class="btn secondary">Clear</button>
        </div>
      </div>

      <div id="grab-results-area" class="grab-results-area hidden">
        <div class="results-toolbar">
          <div class="filter-chips-row" id="grab-chips-row"></div>
          <div class="results-search-wrap">
            ${icons.search(14)}
            <input type="text" id="grab-filter-search" class="form-input input-sm" placeholder="Filter links..." />
          </div>
        </div>

        <div class="results-meta-summary">
          <div class="selection-ctrls">
            <button type="button" class="btn-mini" id="grab-sel-all">Select all</button>
            <button type="button" class="btn-mini" id="grab-sel-none">Deselect all</button>
          </div>
          <span class="selection-count-badge" id="grab-selection-badge">0 links selected</span>
        </div>

        <div class="packages-list" id="grab-packages-list"></div>

        <div class="grabber-destination-card">
          <div class="dest-row">
            <label class="input-label" for="grab-folder">Download destination</label>
            <div class="input-with-btn">
              <input type="text" id="grab-folder" class="form-input" value="${this.settings?.download_dir || this.settings?.default_folder || ""}" />
              <button type="button" id="grab-pick-folder" class="btn secondary">
                ${icons.folder(15)} Browse
              </button>
            </div>
          </div>

          <div class="dest-actions-row">
            <button type="button" id="grab-queue-selected" class="btn primary">
              ${icons.download(15)} Add selected to queue
            </button>
            <button type="button" id="grab-clear-results" class="btn secondary">Clear results</button>
          </div>
        </div>
      </div>
    `;

    this.wireListeners();
  }

  private wireListeners() {
    const runBtn = this.container.querySelector("#grab-run") as HTMLButtonElement;
    const clearBtn = this.container.querySelector("#grab-clear") as HTMLButtonElement;
    const inputEl = this.container.querySelector("#grab-input") as HTMLTextAreaElement;

    const filterSearch = this.container.querySelector("#grab-filter-search") as HTMLInputElement;
    const selAllBtn = this.container.querySelector("#grab-sel-all") as HTMLButtonElement;
    const selNoneBtn = this.container.querySelector("#grab-sel-none") as HTMLButtonElement;
    const pickFolderBtn = this.container.querySelector("#grab-pick-folder") as HTMLButtonElement;
    const queueBtn = this.container.querySelector("#grab-queue-selected") as HTMLButtonElement;
    const clearResultsBtn = this.container.querySelector("#grab-clear-results") as HTMLButtonElement;

    runBtn?.addEventListener("click", () => this.runProbe());
    clearBtn?.addEventListener("click", () => {
      inputEl.value = "";
      this.clearResults();
    });

    filterSearch?.addEventListener("input", (e) => {
      this.resultSearchQuery = (e.target as HTMLInputElement).value.toLowerCase().trim();
      this.renderPackageList();
    });

    selAllBtn?.addEventListener("click", () => {
      this.itemStates.forEach((i) => (i.selected = true));
      this.renderPackageList();
      this.updateSelectionSummary();
    });

    selNoneBtn?.addEventListener("click", () => {
      this.itemStates.forEach((i) => (i.selected = false));
      this.renderPackageList();
      this.updateSelectionSummary();
    });

    pickFolderBtn?.addEventListener("click", async () => {
      const picked = await open({ directory: true, multiple: false });
      if (picked) {
        const folderInput = this.container.querySelector("#grab-folder") as HTMLInputElement;
        if (folderInput) folderInput.value = picked as string;
      }
    });

    clearResultsBtn?.addEventListener("click", () => this.clearResults());

    queueBtn?.addEventListener("click", async () => {
      const selected = this.itemStates.filter((i) => i.selected);
      if (selected.length === 0) {
        showToast("No links selected");
        return;
      }

      const folderInput = this.container.querySelector("#grab-folder") as HTMLInputElement;
      const folder = folderInput.value.trim() || this.settings?.download_dir || this.settings?.default_folder || ".";

      queueBtn.disabled = true;
      let queuedCount = 0;

      for (const item of selected) {
        const args: AddDownloadArgs = {
          urls: [item.link.url],
          output_dir: folder,
          filename: item.filename,
          connections: 8,
          connections_auto: true,
        };

        try {
          await api.addDownload(args);
          queuedCount++;
        } catch (err) {
          console.error(`Failed to queue ${item.link.url}:`, err);
        }
      }

      queueBtn.disabled = false;
      showToast(`Added ${queuedCount} downloads to queue`);
      await this.onDownloadsChanged();

      // Remove queued items from results
      this.itemStates = this.itemStates.filter((i) => !i.selected);
      this.renderFilterChips();
      this.renderPackageList();
      this.updateSelectionSummary();

      if (this.itemStates.length === 0) {
        this.clearResults();
      }
    });
  }

  private async runProbe() {
    if (this.isProbing) return;
    const inputEl = this.container.querySelector("#grab-input") as HTMLTextAreaElement;
    const deepCb = this.container.querySelector("#grab-deep") as HTMLInputElement;
    const hostInput = this.container.querySelector("#grab-host") as HTMLInputElement;
    const extInput = this.container.querySelector("#grab-ext") as HTMLInputElement;
    const minInput = this.container.querySelector("#grab-min") as HTMLInputElement;
    const maxInput = this.container.querySelector("#grab-max") as HTMLInputElement;
    const runBtn = this.container.querySelector("#grab-run") as HTMLButtonElement;

    const text = inputEl.value.trim();
    if (!text) {
      showToast("Please enter URLs or webpage content to probe");
      return;
    }

    const exts = extInput.value
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);

    const options: GrabOptionsDto = {
      deep: deepCb.checked,
      include_ext: exts.length > 0 ? exts : undefined,
      min_size: minInput.value ? Number(minInput.value) : undefined,
      max_size: maxInput.value ? Number(maxInput.value) : undefined,
      host_filter: hostInput.value.trim() || undefined,
      no_probe: false,
    };

    this.isProbing = true;
    runBtn.disabled = true;
    runBtn.innerHTML = `${icons.refresh(15)} Probing links...`;

    const resultsArea = this.container.querySelector("#grab-results-area") as HTMLElement;
    const packagesList = this.container.querySelector("#grab-packages-list") as HTMLElement;
    resultsArea.classList.remove("hidden");
    packagesList.innerHTML = `<div class="grab-loading-state">${icons.spinner(24)}<span>Probing and analyzing links...</span></div>`;

    try {
      const packages = await api.grabLinks(text, options);
      const folderInput = this.container.querySelector("#grab-folder") as HTMLInputElement;
      const destDir = folderInput?.value.trim() || this.settings?.download_dir || this.settings?.default_folder || ".";

      this.itemStates = [];

      for (const pkg of packages) {
        for (const link of pkg.links) {
          const rawName = link.filename || basenameFromUrl(link.url);
          const cat = getFileCategory(rawName, link.mime);

          // Auto-dedupe with backend suggest_save_path if available
          let dedupedName = rawName;
          try {
            const suggested = await api.suggestSavePath(destDir, rawName);
            const base = suggested.replace(/^.*[\\\/]/, "");
            if (base) dedupedName = base;
          } catch {
            // graceful fallback
          }

          this.itemStates.push({
            link,
            packageName: pkg.name || "Default",
            selected: true,
            filename: dedupedName,
            category: cat,
            isDirty: false,
          });
        }
      }

      this.renderFilterChips();
      this.renderPackageList();
      this.updateSelectionSummary();
    } catch (e) {
      packagesList.innerHTML = `<div class="probe-error">${icons.alertCircle(18)} Failed to probe links: ${e}</div>`;
    } finally {
      this.isProbing = false;
      runBtn.disabled = false;
      runBtn.innerHTML = `${icons.search(15)} Probe links`;
    }
  }

  private clearResults() {
    this.itemStates = [];
    const resultsArea = this.container.querySelector("#grab-results-area") as HTMLElement;
    resultsArea.classList.add("hidden");
  }

  private renderFilterChips() {
    const chipsRow = this.container.querySelector("#grab-chips-row") as HTMLElement;
    if (!chipsRow) return;

    const counts: Record<string, number> = {
      all: this.itemStates.length,
      Video: 0,
      Music: 0,
      Documents: 0,
      Compressed: 0,
      Programs: 0,
      Other: 0,
    };

    for (const item of this.itemStates) {
      if (counts[item.category] !== undefined) {
        counts[item.category]++;
      } else {
        counts.Other++;
      }
    }

    const categories: { id: string; label: string }[] = [
      { id: "all", label: "All" },
      { id: "Video", label: "Video" },
      { id: "Music", label: "Music" },
      { id: "Documents", label: "Documents" },
      { id: "Compressed", label: "Compressed" },
      { id: "Programs", label: "Programs" },
      { id: "Other", label: "Other" },
    ];

    chipsRow.innerHTML = categories
      .filter((c) => c.id === "all" || counts[c.id] > 0)
      .map(
        (c) => `
        <button
          type="button"
          class="filter-chip ${this.activeCategoryFilter === c.id ? "active" : ""}"
          data-category="${c.id}"
        >
          <span>${c.label}</span>
          <span class="chip-count">${counts[c.id]}</span>
        </button>
      `
      )
      .join("");

    chipsRow.querySelectorAll(".filter-chip").forEach((btn) => {
      btn.addEventListener("click", () => {
        const cat = (btn as HTMLElement).dataset.category || "all";
        this.activeCategoryFilter = cat;
        this.renderFilterChips();
        this.renderPackageList();
      });
    });
  }

  private renderPackageList() {
    const listEl = this.container.querySelector("#grab-packages-list") as HTMLElement;
    if (!listEl) return;

    if (this.itemStates.length === 0) {
      listEl.innerHTML = `<div class="empty">No links found matching your criteria.</div>`;
      return;
    }

    const grouped = new Map<string, LinkItemState[]>();
    for (const item of this.itemStates) {
      if (this.activeCategoryFilter !== "all" && item.category !== this.activeCategoryFilter) {
        continue;
      }
      if (this.resultSearchQuery) {
        const match =
          item.filename.toLowerCase().includes(this.resultSearchQuery) ||
          item.link.url.toLowerCase().includes(this.resultSearchQuery) ||
          item.packageName.toLowerCase().includes(this.resultSearchQuery);
        if (!match) continue;
      }

      const list = grouped.get(item.packageName) || [];
      list.push(item);
      grouped.set(item.packageName, list);
    }

    if (grouped.size === 0) {
      listEl.innerHTML = `<div class="empty">No links match current filter or search.</div>`;
      return;
    }

    let html = "";
    for (const [pkgName, items] of grouped.entries()) {
      const allSelected = items.every((i) => i.selected);

      const rowsHtml = items
        .map((item) => {
          const cat = item.category;
          const host = extractHost(item.link.url);
          const resumablePill = item.link.resumable
            ? `<span class="pill-mini yes" title="Resume supported">${icons.check(11)} Resumable</span>`
            : `<span class="pill-mini no" title="Resume unsupported">Non-resumable</span>`;

          return `
            <div class="grab-row ${item.selected ? "selected" : ""}" data-url="${this.escapeHtml(item.link.url)}">
              <input type="checkbox" class="form-checkbox item-checkbox" ${item.selected ? "checked" : ""} aria-label="Select ${this.escapeHtml(item.filename)}" />
              <span class="grab-row-icon">${getCategoryIcon(cat, 18)}</span>
              <div class="grab-row-main">
                <input type="text" class="form-input input-filename-edit" value="${this.escapeHtml(item.filename)}" title="${this.escapeHtml(item.filename)}" />
                <div class="grab-row-meta">
                  <span class="category-pill-mini cat-${cat.toLowerCase()}">${cat}</span>
                  <span class="grab-size ${linkSize(item.link) == null ? "unknown" : ""}">${fmtLinkSize(item.link)}</span>
                  ${resumablePill}
                  <span class="grab-host code" title="${this.escapeHtml(item.link.url)}">${this.escapeHtml(host)}</span>
                </div>
              </div>
              <button type="button" class="btn-icon btn-remove-item" title="Remove link" aria-label="Remove link">${icons.x(14)}</button>
            </div>
          `;
        })
        .join("");

      html += `
        <div class="package-card">
          <div class="package-header">
            <label class="pkg-header-left">
              <input type="checkbox" class="form-checkbox pkg-checkbox" ${allSelected ? "checked" : ""} />
              <span class="pkg-title">${this.escapeHtml(pkgName)}</span>
              <span class="pkg-count">${items.length} file${items.length === 1 ? "" : "s"}</span>
            </label>
            <span class="pkg-total-size">${fmtTotalSize(items.map((i) => i.link))}</span>
          </div>
          <div class="package-rows">${rowsHtml}</div>
        </div>
      `;
    }

    listEl.innerHTML = html;

    // Attach listeners for checkboxes and inputs
    listEl.querySelectorAll<HTMLInputElement>(".pkg-checkbox").forEach((pkgCb, pkgIdx) => {
      pkgCb.addEventListener("change", () => {
        const pkgNames = Array.from(grouped.keys());
        const pkgName = pkgNames[pkgIdx];
        const items = grouped.get(pkgName) || [];
        items.forEach((i) => (i.selected = pkgCb.checked));
        this.renderPackageList();
        this.updateSelectionSummary();
      });
    });

    listEl.querySelectorAll<HTMLElement>(".grab-row").forEach((row) => {
      const url = row.dataset.url;
      const item = this.itemStates.find((i) => i.link.url === url);
      if (!item) return;

      const cb = row.querySelector(".item-checkbox") as HTMLInputElement;
      cb?.addEventListener("change", () => {
        item.selected = cb.checked;
        row.classList.toggle("selected", cb.checked);
        this.updateSelectionSummary();
      });

      const fnInput = row.querySelector(".input-filename-edit") as HTMLInputElement;
      fnInput?.addEventListener("input", () => {
        item.filename = fnInput.value.trim() || basenameFromUrl(item.link.url);
        item.isDirty = true;
      });

      const removeBtn = row.querySelector(".btn-remove-item");
      removeBtn?.addEventListener("click", () => {
        this.itemStates = this.itemStates.filter((i) => i !== item);
        this.renderFilterChips();
        this.renderPackageList();
        this.updateSelectionSummary();
      });
    });
  }

  private updateSelectionSummary() {
    const selected = this.itemStates.filter((i) => i.selected);
    const badge = this.container.querySelector("#grab-selection-badge");
    if (badge) {
      badge.textContent = `${selected.length} of ${this.itemStates.length} selected (${fmtTotalSize(selected.map((i) => i.link))})`;
    }

    const queueBtn = this.container.querySelector("#grab-queue-selected") as HTMLButtonElement;
    if (queueBtn) {
      queueBtn.disabled = selected.length === 0;
      queueBtn.textContent = `Add ${selected.length} download${selected.length === 1 ? "" : "s"} to queue`;
    }
  }

  private escapeHtml(str: string): string {
    return (str || "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#039;");
  }
}
