import {
  api,
  type BackendProgressEvent,
  type DownloadView,
} from "./api";
import { DownloadCard } from "./download-card";
import { fmtSpeed } from "./formatters";
import type { SpeedGraph } from "./speed-graph";

export type FilterTab = "all" | "active" | "paused" | "completed";

export class DownloadManager {
  private container: HTMLElement;
  private emptyStateEl: HTMLElement;
  private graph: SpeedGraph;
  private cards = new Map<number, DownloadCard>();
  private currentFilter: FilterTab = "all";
  private searchQuery = "";

  // Throttling for high frequency events
  private pendingRaf = false;
  private lastFlushTime = 0;
  private readonly THROTTLE_MS = 100; // max ~10 DOM flushes per second

  // Stats elements
  private statActiveEl: HTMLElement | null = null;
  private statSpeedEl: HTMLElement | null = null;
  private statDoneEl: HTMLElement | null = null;

  constructor(container: HTMLElement, emptyStateEl: HTMLElement, graph: SpeedGraph) {
    this.container = container;
    this.emptyStateEl = emptyStateEl;
    this.graph = graph;

    this.statActiveEl = document.getElementById("stat-active");
    this.statSpeedEl = document.getElementById("stat-speed");
    this.statDoneEl = document.getElementById("stat-done");
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

    // Calculate aggregate metrics
    let totalSpeed = 0;
    let activeCount = 0;
    let doneCount = 0;
    let pausedCount = 0;

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
      } else if (status === "paused") {
        pausedCount++;
      } else if (status === "completed" || status === "done") {
        doneCount++;
      }
    }

    // Push aggregate speed to canvas chart
    this.graph.push(totalSpeed);

    // Update global status counters in UI
    if (this.statActiveEl) this.statActiveEl.textContent = `${activeCount} active`;
    if (this.statSpeedEl) this.statSpeedEl.textContent = fmtSpeed(totalSpeed);
    if (this.statDoneEl) this.statDoneEl.textContent = `${doneCount} completed`;

    this.updateTabBadges(this.cards.size, activeCount, pausedCount, doneCount);
    this.applyVisibilityFilters();
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
      }
    }

    // Add new or update existing cards
    for (const view of views) {
      const existing = this.cards.get(view.id);
      if (existing) {
        existing.updateView(view);
      } else {
        const newCard = new DownloadCard(view);
        this.cards.set(view.id, newCard);
        this.container.appendChild(newCard.root);
      }
    }

    this.flush();
  }

  public setFilter(filter: FilterTab): void {
    this.currentFilter = filter;
    this.applyVisibilityFilters();
  }

  public setSearch(query: string): void {
    this.searchQuery = query.toLowerCase().trim();
    this.applyVisibilityFilters();
  }

  private applyVisibilityFilters(): void {
    let visibleCount = 0;

    for (const card of this.cards.values()) {
      const status = card.getStatus();
      const isRunning =
        status === "running" ||
        status === "downloading" ||
        status === "starting" ||
        status === "connecting";

      let matchesFilter = true;
      if (this.currentFilter === "active") matchesFilter = isRunning;
      else if (this.currentFilter === "paused") matchesFilter = status === "paused";
      else if (this.currentFilter === "completed") matchesFilter = status === "completed" || status === "done";

      let matchesSearch = true;
      if (this.searchQuery) {
        const text = card.root.textContent?.toLowerCase() || "";
        matchesSearch = text.includes(this.searchQuery);
      }

      const visible = matchesFilter && matchesSearch;
      card.root.classList.toggle("hidden", !visible);
      if (visible) visibleCount++;
    }

    // Empty state
    if (this.cards.size === 0) {
      this.emptyStateEl.classList.remove("hidden");
      this.emptyStateEl.textContent = "No downloads yet. Add a URL or use LinkGrabber.";
    } else if (visibleCount === 0) {
      this.emptyStateEl.classList.remove("hidden");
      this.emptyStateEl.textContent = "No downloads match the current filter or search.";
    } else {
      this.emptyStateEl.classList.add("hidden");
    }
  }

  private updateTabBadges(total: number, active: number, paused: number, completed: number): void {
    const badgeAll = document.getElementById("badge-tab-all");
    const badgeActive = document.getElementById("badge-tab-active");
    const badgePaused = document.getElementById("badge-tab-paused");
    const badgeCompleted = document.getElementById("badge-tab-completed");

    if (badgeAll) badgeAll.textContent = String(total);
    if (badgeActive) badgeActive.textContent = String(active);
    if (badgePaused) badgePaused.textContent = String(paused);
    if (badgeCompleted) badgeCompleted.textContent = String(completed);
  }
}
