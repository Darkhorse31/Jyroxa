import {
  api,
  copyToClipboard,
  openFolder,
  type BackendProgressEvent,
  type DownloadView,
  type TaskStatus,
} from "./api";
import {
  basenameFromUrl,
  extractHost,
  extractProtocol,
  fmtBytes,
  fmtDuration,
  fmtEta,
  fmtSpeed,
  getFileCategory,
} from "./formatters";
import { icons } from "./icons";
import { showToast } from "./toast";

export class DownloadCard {
  public readonly id: number;
  public root: HTMLElement;
  private view: DownloadView;

  // Smoothing & Metrics
  private smoothedSpeed = 0;
  private smoothedEta: number | null = null;
  private avgSpeedBps: number | null = null;
  private elapsedSecs = 0;
  private lastProgressTimestamp: number = Date.now();
  private activeConnections: number;
  private resumable: boolean | null = null;
  private serverHost = "—";
  private protocol = "—";

  // Optimistic UI states
  private isPausing = false;
  private isResuming = false;
  private isCancelling = false;

  // UI state
  private isExpanded = false;

  // DOM node references
  private iconContainer!: HTMLElement;
  private filenameEl!: HTMLElement;
  private statusPill!: HTMLElement;
  private progressFill!: HTMLElement;
  private pctText!: HTMLElement;
  private sizeText!: HTMLElement;
  private speedText!: HTMLElement;
  private etaText!: HTMLElement;
  private elapsedText!: HTMLElement;
  private connText!: HTMLElement;
  private resumableBadge!: HTMLElement;
  private btnPause!: HTMLButtonElement;
  private btnResume!: HTMLButtonElement;
  private btnCancel!: HTMLButtonElement;
  private btnToggleDetails!: HTMLButtonElement;
  private detailsPanel!: HTMLElement;
  private segBarsContainer!: HTMLElement;
  private segListContainer!: HTMLElement;
  private hostValueEl!: HTMLElement;
  private protoValueEl!: HTMLElement;
  private urlValueEl!: HTMLElement;
  private checksumValueEl!: HTMLElement;
  private checksumRow!: HTMLElement;
  private errorBox!: HTMLElement;
  private pathTextEl!: HTMLElement;

  constructor(view: DownloadView) {
    this.id = view.id;
    this.view = { ...view };
    this.activeConnections = view.connections;
    this.smoothedSpeed = view.speed_bps;

    if (view.urls.length > 0) {
      this.serverHost = extractHost(view.urls[0]);
      this.protocol = extractProtocol(view.urls[0]);
    }

    this.root = document.createElement("article");
    this.root.className = "dl-card";
    this.root.dataset.id = String(view.id);

    this.buildDom();
    this.updateDom();
  }

  private buildDom() {
    this.root.innerHTML = `
      <div class="dl-main-row">
        <div class="dl-icon-col" data-slot="icon"></div>

        <div class="dl-content-col">
          <div class="dl-title-row">
            <span class="dl-name" data-slot="name" title="${this.escapeHtml(this.view.filename)}"></span>
            <span class="status-pill" data-slot="status"></span>
          </div>

          <div class="dl-progress-row">
            <div class="progress-track" role="progressbar" aria-valuenow="0" aria-valuemin="0" aria-valuemax="100">
              <span class="progress-fill" data-slot="progress-fill"></span>
            </div>
            <span class="progress-pct" data-slot="pct">0%</span>
          </div>

          <div class="dl-stats-row">
            <span class="stat-item stat-size" data-slot="size">0 B / 0 B</span>
            <span class="stat-divider">·</span>
            <span class="stat-item stat-speed" data-slot="speed">0 B/s</span>
            <span class="stat-divider">·</span>
            <span class="stat-item stat-eta" data-slot="eta">—</span>
            <span class="stat-divider">·</span>
            <span class="stat-item stat-elapsed" data-slot="elapsed">0s</span>
            <span class="stat-divider">·</span>
            <span class="stat-item stat-conn" data-slot="conn">8 conns</span>
            <span class="stat-divider">·</span>
            <span class="badge-resumable" data-slot="resumable">Resumable</span>
          </div>
        </div>

        <div class="dl-actions-col">
          <button type="button" class="btn-icon btn-pause" data-slot="btn-pause" title="Pause download" aria-label="Pause download">
            ${icons.pause(16)}
          </button>
          <button type="button" class="btn-icon btn-resume" data-slot="btn-resume" title="Resume download" aria-label="Resume download">
            ${icons.play(16)}
          </button>
          <button type="button" class="btn-icon btn-cancel" data-slot="btn-cancel" title="Cancel download" aria-label="Cancel download">
            ${icons.cancel(16)}
          </button>
          <button type="button" class="btn-icon btn-details-toggle" data-slot="btn-toggle" title="Show details" aria-label="Show details" aria-expanded="false">
            ${icons.chevronDown(16)}
          </button>
        </div>
      </div>

      <div class="dl-details-panel hidden" data-slot="details-panel">
        <div class="details-section">
          <div class="details-section-header">
            <span class="details-section-title">Segment Progress</span>
            <span class="details-section-badge" data-slot="seg-count">0 segments</span>
          </div>
          <div class="seg-visualizer-bar" data-slot="seg-bars"></div>
          <div class="seg-grid" data-slot="seg-list"></div>
        </div>

        <div class="details-section meta-grid">
          <div class="meta-item">
            <span class="meta-label">${icons.server(13)} Server Host</span>
            <span class="meta-value" data-slot="server-val">—</span>
          </div>
          <div class="meta-item">
            <span class="meta-label">${icons.zap(13)} Protocol</span>
            <span class="meta-value" data-slot="proto-val">—</span>
          </div>
          <div class="meta-item full-width">
            <span class="meta-label">${icons.link(13)} Source URL</span>
            <div class="meta-value-row">
              <span class="meta-value code url-text" data-slot="url-val">—</span>
              <button type="button" class="btn-mini btn-copy-url" title="Copy URL">${icons.copy(13)} Copy</button>
            </div>
          </div>
          <div class="meta-item full-width">
            <span class="meta-label">${icons.folder(13)} Save Location</span>
            <div class="meta-value-row">
              <span class="meta-value code path-text" data-slot="path-val"></span>
              <div class="meta-actions">
                <button type="button" class="btn-mini btn-open-folder" title="Open containing folder">
                  ${icons.folder(13)} Open folder
                </button>
                <button type="button" class="btn-mini btn-copy-path" title="Copy file path">
                  ${icons.copy(13)} Copy path
                </button>
              </div>
            </div>
          </div>
          <div class="meta-item full-width checksum-row" data-slot="checksum-row">
            <span class="meta-label">${icons.shield(13)} Checksum</span>
            <div class="meta-value-row">
              <span class="meta-value code" data-slot="checksum-val">—</span>
              <button type="button" class="btn-mini btn-copy-checksum" title="Copy checksum">${icons.copy(13)} Copy</button>
            </div>
          </div>
          <div class="meta-error-box hidden" data-slot="error-box">
            ${icons.alertCircle(15)}
            <span class="error-text"></span>
          </div>
        </div>
      </div>
    `;

    // Cache element references
    this.iconContainer = this.root.querySelector('[data-slot="icon"]')!;
    this.filenameEl = this.root.querySelector('[data-slot="name"]')!;
    this.statusPill = this.root.querySelector('[data-slot="status"]')!;
    this.progressFill = this.root.querySelector('[data-slot="progress-fill"]')!;
    this.pctText = this.root.querySelector('[data-slot="pct"]')!;
    this.sizeText = this.root.querySelector('[data-slot="size"]')!;
    this.speedText = this.root.querySelector('[data-slot="speed"]')!;
    this.etaText = this.root.querySelector('[data-slot="eta"]')!;
    this.elapsedText = this.root.querySelector('[data-slot="elapsed"]')!;
    this.connText = this.root.querySelector('[data-slot="conn"]')!;
    this.resumableBadge = this.root.querySelector('[data-slot="resumable"]')!;
    this.btnPause = this.root.querySelector('[data-slot="btn-pause"]')!;
    this.btnResume = this.root.querySelector('[data-slot="btn-resume"]')!;
    this.btnCancel = this.root.querySelector('[data-slot="btn-cancel"]')!;
    this.btnToggleDetails = this.root.querySelector('[data-slot="btn-toggle"]')!;
    this.detailsPanel = this.root.querySelector('[data-slot="details-panel"]')!;
    this.segBarsContainer = this.root.querySelector('[data-slot="seg-bars"]')!;
    this.segListContainer = this.root.querySelector('[data-slot="seg-list"]')!;
    this.hostValueEl = this.root.querySelector('[data-slot="server-val"]')!;
    this.protoValueEl = this.root.querySelector('[data-slot="proto-val"]')!;
    this.urlValueEl = this.root.querySelector('[data-slot="url-val"]')!;
    this.checksumValueEl = this.root.querySelector('[data-slot="checksum-val"]')!;
    this.checksumRow = this.root.querySelector('[data-slot="checksum-row"]')!;
    this.errorBox = this.root.querySelector('[data-slot="error-box"]')!;
    this.pathTextEl = this.root.querySelector('[data-slot="path-val"]')!;

    this.wireListeners();
  }

  private wireListeners() {
    // Optimistic Pause
    this.btnPause.addEventListener("click", async () => {
      if (this.isPausing) return;
      this.isPausing = true;
      this.btnPause.disabled = true;
      this.btnPause.title = "Pausing...";
      this.updateStatusPill("paused", "Pausing...");
      try {
        await api.pauseDownload(this.id);
      } catch (e) {
        console.error("Pause failed:", e);
      } finally {
        this.isPausing = false;
      }
    });

    // Optimistic Resume
    this.btnResume.addEventListener("click", async () => {
      if (this.isResuming) return;
      this.isResuming = true;
      this.btnResume.disabled = true;
      this.btnResume.title = "Resuming...";
      this.updateStatusPill("running", "Resuming...");
      try {
        await api.resumeDownload(this.id);
      } catch (e) {
        console.error("Resume failed:", e);
      } finally {
        this.isResuming = false;
      }
    });

    // Optimistic Cancel
    this.btnCancel.addEventListener("click", async () => {
      if (this.isCancelling) return;
      this.isCancelling = true;
      this.btnCancel.disabled = true;
      this.btnCancel.title = "Cancelling...";
      this.updateStatusPill("cancelled", "Cancelling...");
      try {
        await api.cancelDownload(this.id);
      } catch (e) {
        console.error("Cancel failed:", e);
      } finally {
        this.isCancelling = false;
      }
    });

    // Toggle Details
    this.btnToggleDetails.addEventListener("click", () => {
      this.setExpanded(!this.isExpanded);
    });

    // Open Folder
    const btnOpenFolder = this.root.querySelector(".btn-open-folder") as HTMLButtonElement;
    if (btnOpenFolder) {
      btnOpenFolder.addEventListener("click", async () => {
        const ok = await openFolder(this.view.output);
        if (!ok) {
          await copyToClipboard(this.view.output);
          showToast("Saved path copied to clipboard");
        }
      });
    }

    // Copy Path
    const btnCopyPath = this.root.querySelector(".btn-copy-path") as HTMLButtonElement;
    if (btnCopyPath) {
      btnCopyPath.addEventListener("click", async () => {
        await copyToClipboard(this.view.output);
        showToast("Path copied to clipboard");
      });
    }

    // Copy URL
    const btnCopyUrl = this.root.querySelector(".btn-copy-url") as HTMLButtonElement;
    if (btnCopyUrl) {
      btnCopyUrl.addEventListener("click", async () => {
        const url = this.view.urls[0] || "";
        if (url) {
          await copyToClipboard(url);
          showToast("URL copied to clipboard");
        }
      });
    }

    // Copy Checksum
    const btnCopyChecksum = this.root.querySelector(".btn-copy-checksum") as HTMLButtonElement;
    if (btnCopyChecksum) {
      btnCopyChecksum.addEventListener("click", async () => {
        const hash = this.view.sha256 || this.view.md5 || "";
        if (hash) {
          await copyToClipboard(hash);
          showToast("Checksum copied to clipboard");
        }
      });
    }
  }

  public setExpanded(expanded: boolean) {
    this.isExpanded = expanded;
    this.detailsPanel.classList.toggle("hidden", !expanded);
    this.btnToggleDetails.setAttribute("aria-expanded", String(expanded));
    this.btnToggleDetails.innerHTML = expanded
      ? icons.chevronUp(16)
      : icons.chevronDown(16);
    this.btnToggleDetails.title = expanded ? "Hide details" : "Show details";

    if (expanded) {
      this.renderSegments();
    }
  }

  public updateView(newView: DownloadView) {
    this.view = { ...newView };
    if (!this.serverHost || this.serverHost === "—") {
      this.serverHost = extractHost(this.view.urls[0]);
    }
    if (!this.protocol || this.protocol === "—") {
      this.protocol = extractProtocol(this.view.urls[0]);
    }
    this.updateDom();
  }

  public applyProgressEvent(ev: BackendProgressEvent) {
    const now = Date.now();
    const dt = Math.max(0.001, (now - this.lastProgressTimestamp) / 1000);
    this.lastProgressTimestamp = now;

    if (ev.type === "started") {
      if (ev.total_size !== undefined) this.view.total = ev.total_size;
      if (ev.connections !== undefined) {
        this.view.connections = ev.connections;
        this.activeConnections = ev.connections;
      }
      if (ev.supports_ranges !== undefined) this.resumable = ev.supports_ranges;
      this.view.status = "downloading";
    } else if (ev.type === "progress") {
      if (ev.downloaded !== undefined) this.view.downloaded = ev.downloaded;
      if (ev.total !== undefined) this.view.total = ev.total;

      // Speed with Exponential Moving Average (alpha = 0.25)
      const currentBps = ev.speed_bps ?? 0;
      this.view.speed_bps = currentBps;
      if (this.smoothedSpeed === 0) {
        this.smoothedSpeed = currentBps;
      } else {
        this.smoothedSpeed = 0.25 * currentBps + 0.75 * this.smoothedSpeed;
      }

      // Backend provided ETA or frontend fallback
      if (ev.eta_secs !== undefined && ev.eta_secs !== null) {
        if (this.smoothedEta === null) {
          this.smoothedEta = ev.eta_secs;
        } else {
          this.smoothedEta = 0.25 * ev.eta_secs + 0.75 * this.smoothedEta;
        }
      } else {
        // Fallback EMA ETA calculation
        const total = this.view.total ?? 0;
        const remaining = Math.max(0, total - this.view.downloaded);
        if (total > 0 && this.smoothedSpeed > 500 && remaining > 0) {
          const rawEta = remaining / this.smoothedSpeed;
          this.smoothedEta =
            this.smoothedEta === null
              ? rawEta
              : 0.2 * rawEta + 0.8 * this.smoothedEta;
        } else if (this.view.status === "completed" || this.view.status === "done") {
          this.smoothedEta = 0;
        } else {
          this.smoothedEta = null;
        }
      }

      // Average speed
      if (ev.avg_speed_bps !== undefined) {
        this.avgSpeedBps = ev.avg_speed_bps;
      }

      // Elapsed time
      if (ev.elapsed_secs !== undefined) {
        this.elapsedSecs = ev.elapsed_secs;
      } else if (this.view.status === "running" || this.view.status === "downloading") {
        this.elapsedSecs += dt;
      }

      // Active connections
      if (ev.active_connections !== undefined) {
        this.activeConnections = ev.active_connections;
      } else if (ev.connections !== undefined) {
        this.activeConnections = ev.connections;
      }

      // Resumable
      if (ev.resumable !== undefined) {
        this.resumable = ev.resumable;
      }

      // Server & Protocol
      if (ev.server) this.serverHost = ev.server;
      if (ev.protocol) this.protocol = ev.protocol;

      // Segments snapshot
      if (ev.segments) {
        this.view.segments = ev.segments;
      }

      // Status override from backend event
      if (ev.status) {
        this.view.status = ev.status as TaskStatus;
      } else {
        this.view.status = "downloading";
      }
    } else if (ev.type === "segment_progress" && ev.index !== undefined) {
      const seg = this.view.segments.find((s) => s.index === ev.index);
      if (seg) {
        seg.downloaded = ev.current ?? seg.downloaded;
      } else if (ev.start !== undefined && ev.end !== undefined) {
        this.view.segments.push({
          index: ev.index,
          start: ev.start,
          end: ev.end,
          downloaded: ev.current ?? 0,
        });
        this.view.segments.sort((a, b) => a.index - b.index);
      }
    } else if (ev.type === "connections_changed") {
      if (ev.count !== undefined) {
        this.activeConnections = ev.count;
        this.view.connections = ev.count;
      }
    } else if (ev.type === "retrying") {
      this.view.status = "retrying";
      if (ev.error) this.view.error = `Retrying (attempt ${ev.attempt ?? 1}): ${ev.error}`;
    } else if (ev.type === "completed") {
      this.view.status = "completed";
      this.view.downloaded = ev.total_size ?? this.view.downloaded;
      this.view.total = ev.total_size ?? this.view.total;
      this.view.speed_bps = 0;
      this.smoothedSpeed = 0;
      this.smoothedEta = 0;
      if (ev.elapsed_secs !== undefined) this.elapsedSecs = ev.elapsed_secs;
      if (ev.average_bps !== undefined) this.avgSpeedBps = ev.average_bps;
    } else if (ev.type === "failed") {
      this.view.status = "failed";
      this.view.error = ev.error ?? "Download failed";
      this.view.speed_bps = 0;
      this.smoothedSpeed = 0;
      this.smoothedEta = null;
    }

    this.updateDom();
  }

  public updateDom() {
    const d = this.view;

    // File icon
    const cat = getFileCategory(d.filename);
    const iconFn = (icons as Record<string, (size?: number) => string>)[cat] || icons.other;
    this.iconContainer.innerHTML = iconFn(22);
    this.iconContainer.className = `dl-icon-col cat-${cat}`;

    // Filename
    this.filenameEl.textContent = d.filename || basenameFromUrl(d.urls[0] || "");

    // Status Pill
    this.applyStatusPill();

    // Progress bar & percentage
    const total = d.total ?? 0;
    const downloaded = d.downloaded ?? 0;
    let pct = 0;
    if (total > 0) {
      pct = Math.min(100, Math.max(0, (downloaded / total) * 100));
    } else if (d.status === "completed" || d.status === "done") {
      pct = 100;
    }

    this.progressFill.style.width = `${pct.toFixed(1)}%`;
    this.pctText.textContent = total > 0 ? `${pct.toFixed(1)}%` : d.status === "completed" ? "100%" : "—";
    this.root.querySelector(".progress-track")?.setAttribute("aria-valuenow", pct.toFixed(0));

    // Size stats
    this.sizeText.textContent = `${fmtBytes(downloaded)}${total > 0 ? ` / ${fmtBytes(total)}` : ""}`;

    // Speed stats (current + average)
    const isRunning = d.status === "running" || d.status === "downloading" || d.status === "starting" || d.status === "connecting";
    if (isRunning && this.smoothedSpeed > 0) {
      const avgPart = this.avgSpeedBps && this.avgSpeedBps > 0 ? ` · avg ${fmtSpeed(this.avgSpeedBps)}` : "";
      this.speedText.textContent = `${fmtSpeed(this.smoothedSpeed)}${avgPart}`;
    } else if (d.status === "completed" || d.status === "done") {
      this.speedText.textContent = this.avgSpeedBps ? `Avg ${fmtSpeed(this.avgSpeedBps)}` : "Finished";
    } else {
      this.speedText.textContent = "0 B/s";
    }

    // ETA
    if (isRunning) {
      this.etaText.textContent = fmtEta(this.smoothedEta);
    } else if (d.status === "completed" || d.status === "done") {
      this.etaText.textContent = "Done";
    } else {
      this.etaText.textContent = "—";
    }

    // Elapsed time
    this.elapsedText.textContent = `${fmtDuration(this.elapsedSecs)} elapsed`;

    // Connections count
    const connCount = this.activeConnections || d.connections || 8;
    this.connText.textContent = d.connections_auto
      ? `${connCount} conn (auto)`
      : `${connCount} conn`;

    // Resumable badge
    const isResumable = this.resumable ?? (d.segments.length > 1 || (total > 0 && downloaded > 0));
    this.resumableBadge.textContent = isResumable ? "Resumable" : "Non-resumable";
    this.resumableBadge.className = isResumable ? "badge-resumable yes" : "badge-resumable no";

    // Path display
    this.pathTextEl.textContent = d.output;
    this.pathTextEl.title = d.output;

    // Action button states (respecting optimistic locks)
    if (!this.isPausing && !this.isResuming && !this.isCancelling) {
      const canPause = isRunning;
      const canResume = d.status === "paused";
      const canCancel = d.status !== "completed" && d.status !== "done" && d.status !== "cancelled";

      this.btnPause.disabled = !canPause;
      this.btnPause.title = canPause ? "Pause download" : "Cannot pause";

      this.btnResume.disabled = !canResume;
      this.btnResume.title = canResume ? "Resume download" : "Cannot resume";

      this.btnCancel.disabled = !canCancel;
      this.btnCancel.title = canCancel ? "Cancel download" : "Already finished";
    }

    // Details panel metadata
    this.hostValueEl.textContent = this.serverHost || "—";
    this.protoValueEl.textContent = this.protocol || "—";
    this.urlValueEl.textContent = d.urls[0] || "—";
    this.urlValueEl.title = d.urls[0] || "";

    // Checksums
    const hash = d.sha256 ? `SHA-256: ${d.sha256}` : d.md5 ? `MD5: ${d.md5}` : "";
    if (hash) {
      this.checksumRow.classList.remove("hidden");
      this.checksumValueEl.textContent = hash;
    } else {
      this.checksumRow.classList.add("hidden");
    }

    // Error box
    if (d.error) {
      this.errorBox.classList.remove("hidden");
      const errText = this.errorBox.querySelector(".error-text");
      if (errText) errText.textContent = d.error;
    } else {
      this.errorBox.classList.add("hidden");
    }

    // Re-render segments if details panel is open
    if (this.isExpanded) {
      this.renderSegments();
    }
  }

  private applyStatusPill() {
    const rawStatus = (this.view.status || "queued").toLowerCase();
    let display = rawStatus;
    let colorClass = "queued";

    if (rawStatus === "downloading" || rawStatus === "running") {
      display = "Downloading";
      colorClass = "downloading";
    } else if (rawStatus === "connecting" || rawStatus === "starting") {
      display = "Connecting";
      colorClass = "connecting";
    } else if (rawStatus === "paused") {
      display = "Paused";
      colorClass = "paused";
    } else if (rawStatus === "retrying") {
      display = "Retrying";
      colorClass = "retrying";
    } else if (rawStatus === "verifying") {
      display = "Verifying";
      colorClass = "verifying";
    } else if (rawStatus === "completed" || rawStatus === "done") {
      display = "Done";
      colorClass = "completed";
    } else if (rawStatus === "failed") {
      display = "Failed";
      colorClass = "failed";
    } else if (rawStatus === "cancelled") {
      display = "Cancelled";
      colorClass = "cancelled";
    } else if (rawStatus === "queued") {
      display = "Queued";
      colorClass = "queued";
    }

    this.updateStatusPill(colorClass, display);
  }

  private updateStatusPill(colorClass: string, text: string) {
    this.statusPill.className = `status-pill status-${colorClass}`;
    this.statusPill.textContent = text;
  }

  private renderSegments() {
    const segs = this.view.segments;
    const segCountEl = this.root.querySelector('[data-slot="seg-count"]');
    if (segCountEl) {
      segCountEl.textContent = `${segs.length} active segments`;
    }

    if (segs.length === 0) {
      this.segBarsContainer.innerHTML = `<span class="seg-empty">Single stream or waiting for segment initialization...</span>`;
      this.segListContainer.innerHTML = "";
      return;
    }

    // Render continuous segmented progress bar
    const barsHtml = segs
      .map((s) => {
        const span = Math.max(1, s.end - s.start + 1);
        const pct = Math.min(100, Math.max(0, (s.downloaded / span) * 100));
        return `<div class="seg-chunk" title="Segment #${s.index + 1}: ${pct.toFixed(0)}% (${fmtBytes(s.downloaded)} / ${fmtBytes(span)})">
          <span class="seg-chunk-fill" style="width: ${pct.toFixed(1)}%"></span>
        </div>`;
      })
      .join("");
    this.segBarsContainer.innerHTML = barsHtml;

    // Render detailed segment cards
    const listHtml = segs
      .map((s) => {
        const span = Math.max(1, s.end - s.start + 1);
        const pct = Math.min(100, Math.max(0, (s.downloaded / span) * 100));
        return `
          <div class="seg-card">
            <div class="seg-card-head">
              <span class="seg-num">#${s.index + 1}</span>
              <span class="seg-pct">${pct.toFixed(1)}%</span>
            </div>
            <div class="seg-mini-track">
              <span class="seg-mini-fill" style="width: ${pct.toFixed(1)}%"></span>
            </div>
            <div class="seg-card-foot">
              <span>${fmtBytes(s.downloaded)} / ${fmtBytes(span)}</span>
              <span class="seg-range">${fmtBytes(s.start)}–${fmtBytes(s.end)}</span>
            </div>
          </div>`;
      })
      .join("");
    this.segListContainer.innerHTML = listHtml;
  }

  public getRawSpeed(): number {
    return this.view.status === "running" || this.view.status === "downloading"
      ? this.view.speed_bps
      : 0;
  }

  public getStatus(): TaskStatus {
    return this.view.status;
  }

  private escapeHtml(text: string): string {
    const div = document.createElement("div");
    div.textContent = text;
    return div.innerHTML;
  }
}
