import { open } from "@tauri-apps/plugin-dialog";
import {
  api,
  type AddDownloadArgs,
  type AppSettings,
  type GrabOptionsDto,
  type LinkInfo,
} from "./api";
import {
  basenameFromUrl,
  extractHost,
  fmtBytes,
  getFileCategory,
} from "./formatters";
import { icons } from "./icons";
import { showToast } from "./toast";

interface LinkItemState {
  link: LinkInfo;
  packageName: string;
  selected: boolean;
  filename: string;
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
      folderInput.value = s.default_folder;
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

        <div class="results-batch-actions">
          <div class="batch-select-btns">
            <button type="button" id="grab-select-all" class="btn-mini">Select all</button>
            <button type="button" id="grab-deselect-all" class="btn-mini">Deselect all</button>
          </div>
          <span class="batch-selection-count" id="grab-selection-count">0 links selected</span>
        </div>

        <div id="grab-packages-list" class="grab-packages-list"></div>

        <div class="grab-queue-bar">
          <div class="queue-folder-row">
            <label class="input-label" for="grab-folder">Save to folder</label>
            <div class="input-with-btn">
              <input type="text" id="grab-folder" class="form-input" value="${this.settings?.default_folder ?? ""}" />
              <button type="button" id="btn-grab-pick-folder" class="btn secondary">
                ${icons.folder(15)} Browse
              </button>
            </div>
          </div>
          <div class="queue-actions-row">
            <button type="button" id="btn-grab-queue-selected" class="btn primary">
              ${icons.download(15)} Add selected to queue
            </button>
            <button type="button" id="btn-grab-queue-all" class="btn secondary">
              Add all to queue
            </button>
          </div>
        </div>
      </div>
    `;

    this.wireEvents();
  }

  private wireEvents() {
    const runBtn = this.container.querySelector("#grab-run") as HTMLButtonElement;
    const clearBtn = this.container.querySelector("#grab-clear") as HTMLButtonElement;
    const browseBtn = this.container.querySelector("#btn-grab-pick-folder") as HTMLButtonElement;
    const selectAllBtn = this.container.querySelector("#grab-select-all") as HTMLButtonElement;
    const deselectAllBtn = this.container.querySelector("#grab-deselect-all") as HTMLButtonElement;
    const queueSelectedBtn = this.container.querySelector("#btn-grab-queue-selected") as HTMLButtonElement;
    const queueAllBtn = this.container.querySelector("#btn-grab-queue-all") as HTMLButtonElement;
    const searchInput = this.container.querySelector("#grab-filter-search") as HTMLInputElement;

    runBtn?.addEventListener("click", () => void this.runProbe());
    clearBtn?.addEventListener("click", () => {
      const inputEl = this.container.querySelector("#grab-input") as HTMLTextAreaElement;
      if (inputEl) inputEl.value = "";
      this.clearResults();
    });

    browseBtn?.addEventListener("click", async () => {
      const picked = await open({ directory: true, multiple: false });
      if (picked) {
        const folderInput = this.container.querySelector("#grab-folder") as HTMLInputElement;
        if (folderInput) folderInput.value = picked as string;
      }
    });

    selectAllBtn?.addEventListener("click", () => this.toggleSelectAll(true));
    deselectAllBtn?.addEventListener("click", () => this.toggleSelectAll(false));

    queueSelectedBtn?.addEventListener("click", () => void this.queueSelected(false));
    queueAllBtn?.addEventListener("click", () => void this.queueSelected(true));

    searchInput?.addEventListener("input", (e) => {
      this.resultSearchQuery = (e.target as HTMLInputElement).value.toLowerCase().trim();
      this.renderPackageList();
    });
  }

  private async runProbe() {
    if (this.isProbing) return;
    const input = (this.container.querySelector("#grab-input") as HTMLTextAreaElement).value.trim();
    if (!input) {
      showToast("Please enter at least one URL or text to probe");
      return;
    }

    const runBtn = this.container.querySelector("#grab-run") as HTMLButtonElement;
    this.isProbing = true;
    runBtn.disabled = true;
    runBtn.innerHTML = `${icons.refresh(15)} Probing links...`;

    const deep = (this.container.querySelector("#grab-deep") as HTMLInputElement).checked;
    const host = (this.container.querySelector("#grab-host") as HTMLInputElement).value.trim();
    const extRaw = (this.container.querySelector("#grab-ext") as HTMLInputElement).value.trim();
    const min = Number((this.container.querySelector("#grab-min") as HTMLInputElement).value) || undefined;
    const max = Number((this.container.querySelector("#grab-max") as HTMLInputElement).value) || undefined;

    const options: GrabOptionsDto = {
      deep,
      host_filter: host || undefined,
      include_ext: extRaw
        ? extRaw.split(",").map((s) => s.trim().toLowerCase().replace(/^\./, "")).filter(Boolean)
        : undefined,
      min_size: min,
      max_size: max,
      no_probe: false,
    };

    const resultsArea = this.container.querySelector("#grab-results-area") as HTMLElement;
    resultsArea.classList.remove("hidden");

    const packagesList = this.container.querySelector("#grab-packages-list") as HTMLElement;
    packagesList.innerHTML = `<div class="probe-loading">${icons.refresh(20)} Probing and extracting metadata...</div>`;

    try {
      const packages = await api.grabLinks(input, options);
      this.itemStates = [];

      for (const pkg of packages) {
        for (const link of pkg.links) {
          this.itemStates.push({
            link,
            packageName: pkg.name || "Default Package",
            selected: true,
            filename: link.filename || basenameFromUrl(link.url),
          });
        }
      }

      this.renderFilterChips();
      this.renderPackageList();
      this.updateSelectionStats();
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

    // Count categories
    const counts: Record<string, number> = {
      all: this.itemStates.length,
      video: 0,
      audio: 0,
      archive: 0,
      image: 0,
      document: 0,
      executable: 0,
      other: 0,
    };

    for (const item of this.itemStates) {
      const cat = getFileCategory(item.filename);
      if (counts[cat] !== undefined) {
        counts[cat]++;
      } else {
        counts.other++;
      }
    }

    const categories: { id: string; label: string }[] = [
      { id: "all", label: "All" },
      { id: "video", label: "Video" },
      { id: "audio", label: "Audio" },
      { id: "archive", label: "Archive" },
      { id: "document", label: "Docs" },
      { id: "image", label: "Image" },
      { id: "executable", label: "Apps" },
      { id: "other", label: "Other" },
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
      `,
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
      listEl.innerHTML = `<div class="empty">No links found matching your filters.</div>`;
      return;
    }

    // Group items by package name
    const grouped = new Map<string, LinkItemState[]>();
    for (const item of this.itemStates) {
      // Apply category filter
      if (this.activeCategoryFilter !== "all") {
        const cat = getFileCategory(item.filename);
        if (cat !== this.activeCategoryFilter) continue;
      }
      // Apply search filter
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
      const someSelected = items.some((i) => i.selected) && !allSelected;
      const totalPkgSize = items.reduce((sum, i) => sum + (i.link.size ?? 0), 0);

      const rowsHtml = items
        .map((item, idx) => {
          const cat = getFileCategory(item.filename);
          const iconFn = (icons as Record<string, (size?: number) => string>)[cat] || icons.other;
          const host = extractHost(item.link.url);
          const sz = item.link.size != null ? fmtBytes(item.link.size) : "—";
          const resumablePill = item.link.resumable
            ? `<span class="pill-mini yes" title="Resume supported">${icons.check(11)} Resumable</span>`
            : `<span class="pill-mini no" title="Resume unknown or unsupported">Non-resumable</span>`;

          return `
            <tr class="grab-row ${item.selected ? "selected" : ""}" data-url="${this.escapeHtml(item.link.url)}">
              <td class="col-check">
                <input type="checkbox" class="form-checkbox link-cb" ${item.selected ? "checked" : ""} data-idx="${idx}" />
              </td>
              <td class="col-icon">
                <div class="grab-item-icon cat-${cat}">${iconFn(16)}</div>
              </td>
              <td class="col-name">
                <div class="editable-name-wrap">
                  <input type="text" class="form-input input-inline link-name-input" value="${this.escapeHtml(item.filename)}" title="Edit filename" />
                </div>
              </td>
              <td class="col-host"><span class="host-pill">${this.escapeHtml(host)}</span></td>
              <td class="col-mime"><span class="mime-text">${this.escapeHtml(item.link.mime || "—")}</span></td>
              <td class="col-size"><span class="size-text">${sz}</span></td>
              <td class="col-resumable">${resumablePill}</td>
            </tr>
          `;
        })
        .join("");

      html += `
        <div class="grab-package-card" data-package="${this.escapeHtml(pkgName)}">
          <div class="package-card-header">
            <div class="package-title-group">
              <input
                type="checkbox"
                class="form-checkbox pkg-cb"
                ${allSelected ? "checked" : ""}
                ${someSelected ? 'data-indeterminate="true"' : ""}
                title="Select / deselect all in this package"
              />
              <span class="package-icon">${icons.package(16)}</span>
              <span class="package-name">${this.escapeHtml(pkgName)}</span>
              <span class="package-meta">${items.length} files · ${fmtBytes(totalPkgSize)}</span>
            </div>
            <button type="button" class="btn-icon btn-pkg-toggle" title="Toggle package view">
              ${icons.chevronDown(15)}
            </button>
          </div>
          <div class="package-card-body">
            <table class="grab-table">
              <thead>
                <tr>
                  <th class="col-check"></th>
                  <th class="col-icon"></th>
                  <th class="col-name">File Name</th>
                  <th class="col-host">Host</th>
                  <th class="col-mime">Type</th>
                  <th class="col-size">Size</th>
                  <th class="col-resumable">Resume</th>
                </tr>
              </thead>
              <tbody>${rowsHtml}</tbody>
            </table>
          </div>
        </div>
      `;
    }

    listEl.innerHTML = html;
    this.wirePackageEvents(listEl);
  }

  private wirePackageEvents(listEl: HTMLElement) {
    // Set indeterminate state on checkboxes
    listEl.querySelectorAll<HTMLInputElement>(".pkg-cb[data-indeterminate]").forEach((cb) => {
      cb.indeterminate = true;
    });

    // Package checkbox toggle
    listEl.querySelectorAll<HTMLInputElement>(".pkg-cb").forEach((cb) => {
      cb.addEventListener("change", (e) => {
        const card = (e.target as HTMLElement).closest(".grab-package-card")!;
        const pkgName = card.getAttribute("data-package")!;
        const checked = (e.target as HTMLInputElement).checked;

        for (const item of this.itemStates) {
          if (item.packageName === pkgName) {
            item.selected = checked;
          }
        }
        this.renderPackageList();
        this.updateSelectionStats();
      });
    });

    // Individual link checkbox toggle
    listEl.querySelectorAll<HTMLInputElement>(".link-cb").forEach((cb) => {
      cb.addEventListener("change", (e) => {
        const row = (e.target as HTMLElement).closest("tr")!;
        const url = row.getAttribute("data-url")!;
        const checked = (e.target as HTMLInputElement).checked;

        const found = this.itemStates.find((i) => i.link.url === url);
        if (found) {
          found.selected = checked;
        }
        this.updateSelectionStats();
        // Update package checkbox state
        const card = row.closest(".grab-package-card")!;
        const pkgName = card.getAttribute("data-package")!;
        const pkgItems = this.itemStates.filter((i) => i.packageName === pkgName);
        const pkgCb = card.querySelector(".pkg-cb") as HTMLInputElement;
        if (pkgCb) {
          const all = pkgItems.every((i) => i.selected);
          const some = pkgItems.some((i) => i.selected) && !all;
          pkgCb.checked = all;
          pkgCb.indeterminate = some;
        }
        row.classList.toggle("selected", checked);
      });
    });

    // Editable filename input
    listEl.querySelectorAll<HTMLInputElement>(".link-name-input").forEach((inp) => {
      inp.addEventListener("change", (e) => {
        const row = (e.target as HTMLElement).closest("tr")!;
        const url = row.getAttribute("data-url")!;
        const val = (e.target as HTMLInputElement).value.trim();
        const found = this.itemStates.find((i) => i.link.url === url);
        if (found && val) {
          found.filename = val;
        }
      });
    });

    // Toggle package collapse
    listEl.querySelectorAll(".btn-pkg-toggle").forEach((btn) => {
      btn.addEventListener("click", () => {
        const card = btn.closest(".grab-package-card")!;
        const body = card.querySelector(".package-card-body")!;
        body.classList.toggle("hidden");
        btn.innerHTML = body.classList.contains("hidden")
          ? icons.chevronDown(15)
          : icons.chevronUp(15);
      });
    });
  }

  private toggleSelectAll(select: boolean) {
    for (const item of this.itemStates) {
      item.selected = select;
    }
    this.renderPackageList();
    this.updateSelectionStats();
  }

  private updateSelectionStats() {
    const selected = this.itemStates.filter((i) => i.selected);
    const totalSize = selected.reduce((sum, i) => sum + (i.link.size ?? 0), 0);
    const countEl = this.container.querySelector("#grab-selection-count");
    if (countEl) {
      countEl.textContent = `${selected.length} of ${this.itemStates.length} links selected (${fmtBytes(totalSize)})`;
    }

    const queueBtn = this.container.querySelector("#btn-grab-queue-selected") as HTMLButtonElement;
    if (queueBtn) {
      queueBtn.disabled = selected.length === 0;
      queueBtn.innerHTML = `${icons.download(15)} Add ${selected.length} to queue`;
    }
  }

  private async queueSelected(all: boolean) {
    const folderInput = this.container.querySelector("#grab-folder") as HTMLInputElement;
    const folder = folderInput.value.trim() || this.settings?.default_folder || ".";

    const targets = all ? this.itemStates : this.itemStates.filter((i) => i.selected);
    if (targets.length === 0) {
      showToast("No links selected to queue");
      return;
    }

    const queueSelectedBtn = this.container.querySelector("#btn-grab-queue-selected") as HTMLButtonElement;
    const queueAllBtn = this.container.querySelector("#btn-grab-queue-all") as HTMLButtonElement;
    if (queueSelectedBtn) queueSelectedBtn.disabled = true;
    if (queueAllBtn) queueAllBtn.disabled = true;

    try {
      for (const item of targets) {
        const args: AddDownloadArgs = {
          urls: [item.link.url],
          output_dir: folder,
          filename: item.filename,
          connections: 8,
          connections_auto: true,
        };
        await api.addDownload(args);
      }
      showToast(`Added ${targets.length} downloads to queue!`);
      await this.onDownloadsChanged();
    } catch (e) {
      showToast(`Failed to add downloads: ${e}`);
    } finally {
      if (queueSelectedBtn) queueSelectedBtn.disabled = false;
      if (queueAllBtn) queueAllBtn.disabled = false;
    }
  }

  private escapeHtml(text: string): string {
    const div = document.createElement("div");
    div.textContent = text;
    return div.innerHTML;
  }
}
