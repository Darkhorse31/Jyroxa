import {
  api,
  copyToClipboard,
  openFolder,
  openDownloadedFile,
  type BackendProgressEvent,
  type DownloadView,
  type FileCategory,
} from "./api";
import { DownloadCard } from "./download-card";
import { fmtSpeed } from "./formatters";
import { icons } from "./icons";
import type { SpeedGraph } from "./speed-graph";
import { promptRemoveDownloads } from "./remove-dialog";
import { showToast } from "./toast";

export type StatusFilter = "all" | "downloading" | "completed" | "queued" | "failed";
export type CategoryFilter = "all" | FileCategory;
export type SortField = "date" | "name" | "size" | "progress" | "speed";
export type SortOrder = "asc" | "desc";

export class DownloadManager {
  private container: HTMLElement;
  private emptyStateEl: HTMLElement;
  private bulkBarEl: HTMLElement | null;
  private contextMenuEl: HTMLElement | null;
  private graph: SpeedGraph;
  private cards = new Map<number, DownloadCard>();

  // Filter & Search & Sort states
  private currentStatusFilter: StatusFilter = "all";
  private currentCategoryFilter: CategoryFilter = "all";
  private searchQuery = "";
  private sortField: SortField = "date";
  private sortOrder: SortOrder = "desc";

  // Selection state
  private selectedIds = new Set<number>();
  private lastSelectedId: number | null = null;

  // Active context menu target
  private contextTargetView: DownloadView | null = null;

  // Throttling for high frequency events
  private pendingRaf = false;
  private lastFlushTime = 0;
  private readonly THROTTLE_MS = 100;

  // Global metric elements
  private statActiveEl: HTMLElement | null = null;
  private statSpeedEl: HTMLElement | null = null;
  private statDoneEl: HTMLElement | null = null;

  // Callbacks
  private onAddClick?: () => void;

  constructor(
    container: HTMLElement,
    emptyStateEl: HTMLElement,
    graph: SpeedGraph,
    onAddClick?: () => void
  ) {
    this.container = container;
    this.emptyStateEl = emptyStateEl;
    this.graph = graph;
    this.onAddClick = onAddClick;

    this.statActiveEl = document.getElementById("stat-active");
    this.statSpeedEl = document.getElementById("stat-speed");
    this.statDoneEl = document.getElementById("stat-done");
    this.bulkBarEl = document.getElementById("bulk-bar");
    this.contextMenuEl = document.getElementById("context-menu");

    this.wireGlobalListeners();
  }

  private wireGlobalListeners() {
    // Click outside to close context menu
    window.addEventListener("click", () => {
      this.closeContextMenu();
    });

    // Bulk bar buttons
    const btnBulkPause = document.getElementById("bulk-pause");
    const btnBulkResume = document.getElementById("bulk-resume");
    const btnBulkCancel = document.getElementById("bulk-cancel");
    const btnBulkRemove = document.getElementById("bulk-remove");
    const btnBulkOpen = document.getElementById("bulk-open");
    const btnBulkDeselect = document.getElementById("bulk-deselect");

    btnBulkPause?.addEventListener("click", async () => {
      const ids = Array.from(this.selectedIds);
      if (ids.length === 0) return;
      try {
        await api.pauseMany(ids);
        showToast(`Paused ${ids.length} downloads`);
        await this.reload();
      } catch (e) {
        showToast(`Pause failed: ${e}`);
      }
    });

    btnBulkResume?.addEventListener("click", async () => {
      const ids = Array.from(this.selectedIds);
      if (ids.length === 0) return;
      try {
        await api.resumeMany(ids);
        showToast(`Resumed ${ids.length} downloads`);
        await this.reload();
      } catch (e) {
        showToast(`Resume failed: ${e}`);
      }
    });

    btnBulkCancel?.addEventListener("click", async () => {
      const ids = Array.from(this.selectedIds);
      if (ids.length === 0) return;
      try {
        await api.cancelMany(ids);
        showToast(`Cancelled ${ids.length} downloads`);
        await this.reload();
      } catch (e) {
        showToast(`Cancel failed: ${e}`);
      }
    });

    btnBulkRemove?.addEventListener("click", async () => {
      const ids = Array.from(this.selectedIds);
      if (ids.length === 0) return;
      this.promptRemoveMany(ids);
    });

    btnBulkOpen?.addEventListener("click", async () => {
      const firstId = Array.from(this.selectedIds)[0];
      const card = this.cards.get(firstId);
      if (card) {
        await openFolder(card.getView().output);
      }
    });

    btnBulkDeselect?.addEventListener("click", () => {
      this.clearSelection();
    });

    // Wire Context Menu actions
    this.wireContextMenuActions();
  }

  private wireContextMenuActions() {
    const menu = this.contextMenuEl;
    if (!menu) return;

    menu.querySelector('[data-action="open-file"]')?.addEventListener("click", async () => {
      if (this.contextTargetView) {
        await openDownloadedFile(this.contextTargetView.output);
      }
      this.closeContextMenu();
    });

    menu.querySelector('[data-action="open-folder"]')?.addEventListener("click", async () => {
      if (this.contextTargetView) {
        await openFolder(this.contextTargetView.output);
      }
      this.closeContextMenu();
    });

    menu.querySelector('[data-action="pause"]')?.addEventListener("click", async () => {
      if (this.contextTargetView) {
        await api.pauseDownload(this.contextTargetView.id);
        await this.reload();
      }
      this.closeContextMenu();
    });

    menu.querySelector('[data-action="resume"]')?.addEventListener("click", async () => {
      if (this.contextTargetView) {
        await api.resumeDownload(this.contextTargetView.id);
        await this.reload();
      }
      this.closeContextMenu();
    });

    menu.querySelector('[data-action="retry"]')?.addEventListener("click", async () => {
      if (this.contextTargetView) {
        await api.retryDownload(this.contextTargetView.id);
        await this.reload();
      }
      this.closeContextMenu();
    });

    menu.querySelector('[data-action="cancel"]')?.addEventListener("click", async () => {
      if (this.contextTargetView) {
        await api.cancelDownload(this.contextTargetView.id);
        await this.reload();
      }
      this.closeContextMenu();
    });

    menu.querySelector('[data-action="copy-url"]')?.addEventListener("click", async () => {
      if (this.contextTargetView && this.contextTargetView.urls[0]) {
        await copyToClipboard(this.contextTargetView.urls[0]);
        showToast("Download URL copied");
      }
      this.closeContextMenu();
    });

    menu.querySelector('[data-action="copy-path"]')?.addEventListener("click", async () => {
      if (this.contextTargetView) {
        await copyToClipboard(this.contextTargetView.output);
        showToast("File path copied");
      }
      this.closeContextMenu();
    });

    // The remove dialog is modal, and clicking it closes the context menu, which
    // clears contextTargetView; capture the id before awaiting the user's choice.
    const removeFromMenu = async () => {
      const id = this.contextTargetView?.id;
      this.closeContextMenu();
      if (id === undefined) return;
      const choice = await promptRemoveDownloads(1);
      if (choice !== "cancel") await this.removeDownloadItem(id, choice === "remove-delete");
    };
    menu.querySelector('[data-action="remove-list"]')?.addEventListener("click", removeFromMenu);
    menu.querySelector('[data-action="remove-file"]')?.addEventListener("click", removeFromMenu);
  }

  public async reload(): Promise<void> {
    try {
      const views = await api.listDownloads();
      this.reconcile(views);
    } catch (e) {
      console.error("Failed to list downloads:", e);
    }
  }

  public handleProgress(id: number, event: BackendProgressEvent): void {
    const card = this.cards.get(id);
    if (card) {
      card.applyProgressEvent(event);
    }
    this.scheduleFlush();
  }

  private scheduleFlush(): void {
    if (this.pendingRaf) return;
    this.pendingRaf = true;

    requestAnimationFrame(() => {
      const now = performance.now();
      const elapsed = now - this.lastFlushTime;
      if (elapsed < this.THROTTLE_MS) {
        setTimeout(() => {
          this.pendingRaf = false;
          this.flush();
        }, this.THROTTLE_MS - elapsed);
      } else {
        this.pendingRaf = false;
        this.flush();
      }
    });
  }

  private flush(): void {
    this.lastFlushTime = performance.now();

    let totalSpeed = 0;
    let activeCount = 0;
    let doneCount = 0;
    let queuedCount = 0;
    let failedCount = 0;

    // Counts by category
    const catCounts: Record<FileCategory, number> = {
      Video: 0,
      Music: 0,
      Documents: 0,
      Compressed: 0,
      Programs: 0,
      Other: 0,
    };

    for (const card of this.cards.values()) {
      const status = card.getStatus();
      const isRunning =
        status === "running" ||
        status === "downloading" ||
        status === "starting" ||
        status === "connecting";

      if (isRunning) {
        totalSpeed += card.getRawSpeed();
        activeCount++;
      } else if (status === "completed" || status === "done") {
        doneCount++;
      } else if (status === "queued") {
        queuedCount++;
      } else if (status === "failed") {
        failedCount++;
      }

      const cat = card.getCategory();
      if (catCounts[cat] !== undefined) {
        catCounts[cat]++;
      }
    }

    // Push aggregate speed to canvas chart
    this.graph.push(totalSpeed);

    // Update global status counters in UI
    if (this.statActiveEl) this.statActiveEl.textContent = `${activeCount} active`;
    if (this.statSpeedEl) this.statSpeedEl.textContent = fmtSpeed(totalSpeed);
    if (this.statDoneEl) this.statDoneEl.textContent = `${doneCount} completed`;

    this.updateFilterCounts({
      all: this.cards.size,
      downloading: activeCount,
      completed: doneCount,
      queued: queuedCount,
      failed: failedCount,
      categories: catCounts,
    });

    this.applyVisibilityAndSorting();
    this.updateBulkActionBar();
  }

  public reconcile(views: DownloadView[]): void {
    const nextIds = new Set(views.map((v) => v.id));

    // Remove deleted cards
    for (const [id, card] of this.cards.entries()) {
      if (!nextIds.has(id)) {
        if (card.root.parentNode) {
          card.root.parentNode.removeChild(card.root);
        }
        this.cards.delete(id);
        this.selectedIds.delete(id);
      }
    }

    // Add new or update existing cards
    for (const view of views) {
      const existing = this.cards.get(view.id);
      if (existing) {
        existing.updateView(view);
      } else {
        const newCard = new DownloadCard(view, {
          onSelect: (id, selected, shiftKey) => this.handleCardSelect(id, selected, shiftKey),
          onContextMenu: (e, v) => this.showContextMenu(e, v),
          onChanged: () => this.scheduleFlush(),
        });
        this.cards.set(view.id, newCard);
        this.container.appendChild(newCard.root);
      }
    }

    this.flush();
  }

  // Multi-select & Shift-click handling
  private handleCardSelect(id: number, selected: boolean, shiftKey: boolean) {
    if (shiftKey && this.lastSelectedId !== null && this.lastSelectedId !== id) {
      // Range selection among visible items
      const visibleIds = this.getVisibleCardIds();
      const idxA = visibleIds.indexOf(this.lastSelectedId);
      const idxB = visibleIds.indexOf(id);
      if (idxA !== -1 && idxB !== -1) {
        const start = Math.min(idxA, idxB);
        const end = Math.max(idxA, idxB);
        for (let i = start; i <= end; i++) {
          const rangeId = visibleIds[i];
          this.selectedIds.add(rangeId);
          this.cards.get(rangeId)?.setSelected(true);
        }
      }
    } else {
      if (selected) {
        this.selectedIds.add(id);
      } else {
        this.selectedIds.delete(id);
      }
      this.lastSelectedId = id;
    }

    this.updateBulkActionBar();
  }

  public selectAll(): void {
    const visibleIds = this.getVisibleCardIds();
    visibleIds.forEach((id) => {
      this.selectedIds.add(id);
      this.cards.get(id)?.setSelected(true);
    });
    this.updateBulkActionBar();
  }

  public clearSelection(): void {
    this.selectedIds.clear();
    this.lastSelectedId = null;
    for (const card of this.cards.values()) {
      card.setSelected(false);
    }
    this.updateBulkActionBar();
  }

  private updateBulkActionBar(): void {
    if (!this.bulkBarEl) return;
    const count = this.selectedIds.size;
    const isVisible = count > 0;
    this.bulkBarEl.classList.toggle("hidden", !isVisible);

    const countLabel = this.bulkBarEl.querySelector("#bulk-count-label");
    if (countLabel) {
      countLabel.textContent = `${count} selected`;
    }
  }

  // Right-click Context Menu
  public showContextMenu(e: MouseEvent, view: DownloadView) {
    const menu = this.contextMenuEl;
    if (!menu) return;
    this.contextTargetView = view;

    const isRunning =
      view.status === "running" ||
      view.status === "downloading" ||
      view.status === "starting" ||
      view.status === "connecting";
    const isPaused = view.status === "paused";
    const isFailedOrCancelled = view.status === "failed" || view.status === "cancelled";

    // Toggle menu items based on status
    const itemPause = menu.querySelector('[data-action="pause"]') as HTMLElement;
    const itemResume = menu.querySelector('[data-action="resume"]') as HTMLElement;
    const itemRetry = menu.querySelector('[data-action="retry"]') as HTMLElement;

    if (itemPause) itemPause.classList.toggle("hidden", !isRunning);
    if (itemResume) itemResume.classList.toggle("hidden", !isPaused);
    if (itemRetry) itemRetry.classList.toggle("hidden", !isFailedOrCancelled);

    menu.classList.remove("hidden");

    // Position menu within viewport boundaries
    const menuWidth = 220;
    const menuHeight = 280;
    let x = e.clientX;
    let y = e.clientY;

    if (x + menuWidth > window.innerWidth) {
      x = window.innerWidth - menuWidth - 12;
    }
    if (y + menuHeight > window.innerHeight) {
      y = window.innerHeight - menuHeight - 12;
    }

    menu.style.left = `${Math.max(10, x)}px`;
    menu.style.top = `${Math.max(10, y)}px`;
  }

  public closeContextMenu(): void {
    if (this.contextMenuEl) {
      this.contextMenuEl.classList.add("hidden");
    }
    this.contextTargetView = null;
  }

  // Single remove
  public async removeDownloadItem(id: number, deleteFile: boolean): Promise<void> {
    try {
      await api.removeDownload(id, deleteFile);
      this.selectedIds.delete(id);
      showToast(deleteFile ? "Removed & file deleted" : "Removed from list");
      await this.reload();
    } catch (e) {
      showToast(`Error removing download: ${e}`);
    }
  }

  private async promptRemoveMany(ids: number[]) {
    const choice = await promptRemoveDownloads(ids.length);
    if (choice === "cancel") return;

    const deleteFiles = choice === "remove-delete";
    try {
      const n = await api.removeMany(ids, deleteFiles);
      showToast(deleteFiles ? `Removed ${n} items and deleted files` : `Removed ${n} items`);
      this.clearSelection();
      await this.reload();
    } catch (err) {
      showToast(`Remove failed: ${err}`);
    }
  }

  // Filter setters
  public setStatusFilter(filter: StatusFilter): void {
    this.currentStatusFilter = filter;
    this.applyVisibilityAndSorting();
  }

  public setCategoryFilter(category: CategoryFilter): void {
    this.currentCategoryFilter = category;
    this.applyVisibilityAndSorting();
  }

  public setSearch(query: string): void {
    this.searchQuery = query.toLowerCase().trim();
    this.applyVisibilityAndSorting();
  }

  public setSorting(field: SortField, order: SortOrder): void {
    this.sortField = field;
    this.sortOrder = order;
    this.applyVisibilityAndSorting();
  }

  private getVisibleCardIds(): number[] {
    const list: number[] = [];
    for (const [id, card] of this.cards.entries()) {
      if (!card.root.classList.contains("hidden")) {
        list.push(id);
      }
    }
    return list;
  }

  private applyVisibilityAndSorting(): void {
    let visibleCount = 0;
    const cardsArray = Array.from(this.cards.values());

    for (const card of cardsArray) {
      const status = card.getStatus();
      const isRunning =
        status === "running" ||
        status === "downloading" ||
        status === "starting" ||
        status === "connecting";

      // Status check
      let matchesStatus = true;
      if (this.currentStatusFilter === "downloading") {
        matchesStatus = isRunning;
      } else if (this.currentStatusFilter === "completed") {
        matchesStatus = status === "completed" || status === "done";
      } else if (this.currentStatusFilter === "queued") {
        matchesStatus = status === "queued";
      } else if (this.currentStatusFilter === "failed") {
        matchesStatus = status === "failed";
      }

      // Category check
      let matchesCat = true;
      if (this.currentCategoryFilter !== "all") {
        matchesCat = card.getCategory() === this.currentCategoryFilter;
      }

      // Search check
      let matchesSearch = true;
      if (this.searchQuery) {
        const text = card.root.textContent?.toLowerCase() || "";
        matchesSearch = text.includes(this.searchQuery);
      }

      const visible = matchesStatus && matchesCat && matchesSearch;
      card.root.classList.toggle("hidden", !visible);
      if (visible) visibleCount++;
    }

    // Sort visible cards
    cardsArray.sort((a, b) => {
      let valA: any = 0;
      let valB: any = 0;

      switch (this.sortField) {
        case "name":
          valA = a.getFilename().toLowerCase();
          valB = b.getFilename().toLowerCase();
          break;
        case "size":
          valA = a.getTotalSize();
          valB = b.getTotalSize();
          break;
        case "progress":
          valA = a.getProgressPct();
          valB = b.getProgressPct();
          break;
        case "speed":
          valA = a.getRawSpeed();
          valB = b.getRawSpeed();
          break;
        case "date":
        default:
          valA = a.id;
          valB = b.id;
          break;
      }

      let cmp = 0;
      if (valA < valB) cmp = -1;
      else if (valA > valB) cmp = 1;

      return this.sortOrder === "asc" ? cmp : -cmp;
    });

    // Reorder DOM nodes smoothly
    cardsArray.forEach((card) => {
      this.container.appendChild(card.root);
    });

    // Empty state illustrations & CTA
    this.renderEmptyState(visibleCount);
  }

  private renderEmptyState(visibleCount: number): void {
    if (this.cards.size === 0) {
      this.emptyStateEl.classList.remove("hidden");
      this.emptyStateEl.innerHTML = `
        <div class="empty-graphic">${icons.download(48)}</div>
        <h3 class="empty-title">No downloads yet</h3>
        <p class="empty-desc">Add a direct URL, import links via LinkGrabber, or drag and drop links anywhere.</p>
        <div class="empty-actions">
          <button type="button" class="btn primary" id="empty-add-btn">
            ${icons.plus(15)} Add download
          </button>
        </div>
      `;
      this.emptyStateEl.querySelector("#empty-add-btn")?.addEventListener("click", () => {
        if (this.onAddClick) this.onAddClick();
      });
    } else if (visibleCount === 0) {
      this.emptyStateEl.classList.remove("hidden");
      this.emptyStateEl.innerHTML = `
        <div class="empty-graphic">${icons.search(48)}</div>
        <h3 class="empty-title">No downloads match filters</h3>
        <p class="empty-desc">No downloads match the selected status, category, or search term.</p>
        <div class="empty-actions">
          <button type="button" class="btn secondary" id="empty-clear-btn">
            ${icons.refresh(15)} Clear filters
          </button>
        </div>
      `;
      this.emptyStateEl.querySelector("#empty-clear-btn")?.addEventListener("click", () => {
        this.resetFilters();
      });
    } else {
      this.emptyStateEl.classList.add("hidden");
    }
  }

  private resetFilters(): void {
    this.currentStatusFilter = "all";
    this.currentCategoryFilter = "all";
    this.searchQuery = "";

    const searchInput = document.getElementById("downloads-search") as HTMLInputElement;
    if (searchInput) searchInput.value = "";

    document.querySelectorAll(".sidebar-item").forEach((item) => {
      item.classList.remove("active");
    });
    document.querySelector('.sidebar-item[data-status="all"]')?.classList.add("active");
    document.querySelector('.sidebar-item[data-category="all"]')?.classList.add("active");

    this.applyVisibilityAndSorting();
  }

  private updateFilterCounts(counts: {
    all: number;
    downloading: number;
    completed: number;
    queued: number;
    failed: number;
    categories: Record<FileCategory, number>;
  }): void {
    // Status badges
    const setBadge = (id: string, n: number) => {
      const el = document.getElementById(id);
      if (el) el.textContent = String(n);
    };

    setBadge("badge-status-all", counts.all);
    setBadge("badge-status-downloading", counts.downloading);
    setBadge("badge-status-completed", counts.completed);
    setBadge("badge-status-queued", counts.queued);
    setBadge("badge-status-failed", counts.failed);

    // Category badges
    setBadge("badge-cat-all", counts.all);
    setBadge("badge-cat-video", counts.categories.Video);
    setBadge("badge-cat-music", counts.categories.Music);
    setBadge("badge-cat-documents", counts.categories.Documents);
    setBadge("badge-cat-compressed", counts.categories.Compressed);
    setBadge("badge-cat-programs", counts.categories.Programs);
    setBadge("badge-cat-other", counts.categories.Other);
  }

  public getSelectedIds(): number[] {
    return Array.from(this.selectedIds);
  }

  public pauseSelected(): void {
    const ids = this.getSelectedIds();
    if (ids.length > 0) {
      api.pauseMany(ids).then(() => this.reload());
    }
  }

  public resumeSelected(): void {
    const ids = this.getSelectedIds();
    if (ids.length > 0) {
      api.resumeMany(ids).then(() => this.reload());
    }
  }

  public async removeSelectedWithPrompt(): Promise<void> {
    const ids = this.getSelectedIds();
    if (ids.length > 0) {
      await this.promptRemoveMany(ids);
    }
  }
}
