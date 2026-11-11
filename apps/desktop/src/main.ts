import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  api,
  type AddDownloadArgs,
  type AppSettings,
  type DownloadView,
  type LinkInfo,
  type Package,
} from "./api";

const THEME_KEY = "swiftget-theme";

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const u = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 10 ? 0 : 1)} ${u[i]}`;
}

function fmtSpeed(bps: number): string {
  return `${fmtBytes(bps)}/s`;
}

function basenameFromUrl(url: string): string {
  try {
    const p = new URL(url).pathname.split("/").pop() || "download.bin";
    return decodeURIComponent(p);
  } catch {
    return "download.bin";
  }
}

class SpeedGraph {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private samples: number[] = [];
  private maxPoints = 120;

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d")!;
  }

  push(totalBps: number) {
    this.samples.push(totalBps);
    if (this.samples.length > this.maxPoints) this.samples.shift();
    this.draw();
  }

  draw() {
    const { width, height } = this.canvas;
    const style = getComputedStyle(document.body);
    const line = style.getPropertyValue("--graph").trim() || "#3b82f6";
    const bg = style.getPropertyValue("--bg").trim() || "#111";
    this.ctx.fillStyle = bg;
    this.ctx.fillRect(0, 0, width, height);
    if (this.samples.length < 2) return;
    const max = Math.max(...this.samples, 1);
    this.ctx.strokeStyle = line;
    this.ctx.lineWidth = 2;
    this.ctx.beginPath();
    this.samples.forEach((v, i) => {
      const x = (i / (this.maxPoints - 1)) * width;
      const y = height - (v / max) * (height - 4) - 2;
      if (i === 0) this.ctx.moveTo(x, y);
      else this.ctx.lineTo(x, y);
    });
    this.ctx.stroke();
  }
}

let downloads: DownloadView[] = [];
let settings: AppSettings | null = null;
const graph = new SpeedGraph(document.getElementById("speed-graph") as HTMLCanvasElement);

const listEl = document.getElementById("download-list")!;
const sidebar = document.getElementById("sidebar")!;
const grabberPanel = document.getElementById("grabber-panel")!;
const settingsPanel = document.getElementById("settings-panel")!;

function setTheme(theme: "dark" | "light") {
  document.body.setAttribute("data-theme", theme);
  localStorage.setItem(THEME_KEY, theme);
  graph.draw();
}

function toggleTheme() {
  const cur = document.body.getAttribute("data-theme") === "light" ? "light" : "dark";
  setTheme(cur === "dark" ? "light" : "dark");
}

function renderDownloads() {
  if (downloads.length === 0) {
    listEl.innerHTML = `<div class="empty">No downloads yet. Add a URL or use LinkGrabber.</div>`;
    return;
  }
  listEl.innerHTML = downloads
    .map((d) => {
      const total = d.total ?? 0;
      const pct = total > 0 ? Math.min(100, (d.downloaded / total) * 100) : 0;
      const segs =
        d.segments.length > 0
          ? `<div class="seg-bars">${d.segments
              .map((s) => {
                const len = Math.max(1, s.end - s.start + 1);
                const sp = Math.min(100, (s.downloaded / len) * 100);
                return `<div class="seg"><i style="width:${sp}%"></i></div>`;
              })
              .join("")}</div>`
          : "";
      const err = d.error ? `<div class="dl-meta">${d.error}</div>` : "";
      return `
      <article class="dl-card" data-id="${d.id}">
        <div class="dl-head">
          <div>
            <div class="dl-name">${d.filename}</div>
            <div class="dl-meta">${d.urls[0]}</div>
            <span class="status ${d.status}">${d.status}</span>
            ${err}
          </div>
          <div class="dl-actions">
            <button data-act="pause" ${d.status !== "running" ? "disabled" : ""}>Pause</button>
            <button data-act="resume" ${d.status !== "paused" ? "disabled" : ""}>Resume</button>
            <button data-act="cancel" ${["completed", "cancelled"].includes(d.status) ? "disabled" : ""}>Cancel</button>
          </div>
        </div>
        <div class="progress-overall"><span style="width:${pct}%"></span></div>
        <div class="dl-meta">${fmtBytes(d.downloaded)}${total ? ` / ${fmtBytes(total)}` : ""} · ${fmtSpeed(d.speed_bps)} · ${d.connections} conn</div>
        ${segs}
      </article>`;
    })
    .join("");

  listEl.querySelectorAll(".dl-card button").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const card = btn.closest(".dl-card") as HTMLElement;
      const id = Number(card.dataset.id);
      const act = (btn as HTMLButtonElement).dataset.act;
      if (act === "pause") await api.pauseDownload(id);
      if (act === "resume") await api.resumeDownload(id);
      if (act === "cancel") await api.cancelDownload(id);
      await refreshDownloads();
    });
  });

  const totalSpeed = downloads
    .filter((d) => d.status === "running")
    .reduce((a, d) => a + d.speed_bps, 0);
  graph.push(totalSpeed);
}

async function refreshDownloads() {
  downloads = await api.listDownloads();
  renderDownloads();
}

function showSidebar(which: "grabber" | "settings" | null) {
  sidebar.classList.toggle("hidden", which === null);
  document.querySelector(".layout")!.classList.toggle("with-sidebar", which !== null);
  grabberPanel.classList.toggle("hidden", which !== "grabber");
  settingsPanel.classList.toggle("hidden", which !== "settings");
}

function buildGrabberUi() {
  grabberPanel.innerHTML = `
    <h2>LinkGrabber</h2>
    <p class="hint">Paste text or a page URL. Toggle deep crawl to follow HTML links one level.</p>
    <textarea id="grab-input" rows="5" style="width:100%"></textarea>
    <div class="grabber-filters">
      <label class="check row"><input type="checkbox" id="grab-deep" /> Deep crawl</label>
      <label>Host filter <input type="text" id="grab-host" placeholder="example.com" /></label>
      <label>Extensions (comma) <input type="text" id="grab-ext" placeholder="zip,mp4,pdf" /></label>
      <label>Min size (bytes) <input type="number" id="grab-min" min="0" /></label>
      <label>Max size (bytes) <input type="number" id="grab-max" min="0" /></label>
    </div>
    <button type="button" id="grab-run" class="primary">Probe links</button>
    <div id="grab-results" style="margin-top:1rem;max-height:360px;overflow:auto"></div>
  `;

  document.getElementById("grab-run")!.addEventListener("click", async () => {
    const input = (document.getElementById("grab-input") as HTMLTextAreaElement).value.trim();
    if (!input) return;
    const deep = (document.getElementById("grab-deep") as HTMLInputElement).checked;
    const host = (document.getElementById("grab-host") as HTMLInputElement).value.trim();
    const extRaw = (document.getElementById("grab-ext") as HTMLInputElement).value.trim();
    const min = Number((document.getElementById("grab-min") as HTMLInputElement).value) || undefined;
    const max = Number((document.getElementById("grab-max") as HTMLInputElement).value) || undefined;
    const options = {
      deep,
      host_filter: host || undefined,
      include_ext: extRaw ? extRaw.split(",").map((s) => s.trim().toLowerCase()) : undefined,
      min_size: min,
      max_size: max,
      no_probe: false,
    };
    const res = document.getElementById("grab-results")!;
    res.innerHTML = `<p class="hint">Probing…</p>`;
    try {
      const packages = await api.grabLinks(input, options);
      renderGrabResults(packages);
    } catch (e) {
      res.innerHTML = `<p class="status failed">${e}</p>`;
    }
  });
}

let grabSelection = new Set<string>();

function renderGrabResults(packages: Package[]) {
  const links: LinkInfo[] = packages.flatMap((p) => p.links);
  grabSelection = new Set(links.map((l) => l.url));
  const res = document.getElementById("grab-results")!;
  if (links.length === 0) {
    res.innerHTML = `<p class="empty">No links matched filters.</p>`;
    return;
  }
  const rows = links
    .map((l) => {
      const host = (() => {
        try {
          return new URL(l.url).host;
        } catch {
          return "";
        }
      })();
      const sz = l.size != null ? fmtBytes(l.size) : "—";
      const name = l.filename || basenameFromUrl(l.url);
      return `<tr data-url="${l.url}" data-name="${name}">
        <td><input type="checkbox" checked class="grab-pick" /></td>
        <td>${name}</td>
        <td>${host}</td>
        <td>${l.mime || "—"}</td>
        <td>${sz}</td>
      </tr>`;
    })
    .join("");
  res.innerHTML = `
    <table class="grabber-table">
      <thead><tr><th></th><th>Name</th><th>Host</th><th>Type</th><th>Size</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
    <label>Save folder <input type="text" id="grab-folder" value="${settings?.default_folder ?? ""}" style="width:100%"/></label>
    <button type="button" id="grab-queue" class="primary" style="margin-top:0.5rem">Add selected to queue</button>
  `;
  res.querySelectorAll(".grab-pick").forEach((cb) => {
    cb.addEventListener("change", () => {
      const tr = cb.closest("tr") as HTMLTableRowElement;
      const url = tr.dataset.url!;
      if ((cb as HTMLInputElement).checked) grabSelection.add(url);
      else grabSelection.delete(url);
    });
  });
  document.getElementById("grab-queue")!.addEventListener("click", async () => {
    const folder = (document.getElementById("grab-folder") as HTMLInputElement).value.trim();
    for (const tr of res.querySelectorAll("tbody tr")) {
      const url = (tr as HTMLTableRowElement).dataset.url!;
      if (!grabSelection.has(url)) continue;
      const name = (tr as HTMLTableRowElement).dataset.name || basenameFromUrl(url);
      const args: AddDownloadArgs = {
        urls: [url],
        output_dir: folder,
        filename: name,
        connections: 8,
        connections_auto: true,
      };
      await api.addDownload(args);
    }
    await refreshDownloads();
  });
}

function buildSettingsUi() {
  if (!settings) return;
  settingsPanel.innerHTML = `
    <h2>Settings</h2>
    <div class="settings-grid">
      <label>Max concurrent downloads
        <input type="number" id="set-concurrent" min="1" max="16" value="${settings.max_concurrent}" />
      </label>
      <label>Global speed limit (bytes/s, empty = unlimited)
        <input type="number" id="set-limit" min="0" value="${settings.global_speed_limit ?? ""}" />
      </label>
      <label>Default folder
        <input type="text" id="set-folder" value="${settings.default_folder}" />
      </label>
      <label class="row check"><input type="checkbox" id="set-clip" ${settings.clipboard_monitor ? "checked" : ""}/> Clipboard monitor</label>
      <label class="row check"><input type="checkbox" id="set-autotune" ${settings.autotune ? "checked" : ""}/> Auto-tune connections</label>
    </div>
    <button type="button" id="set-save" class="primary">Save</button>
  `;
  document.getElementById("set-save")!.addEventListener("click", async () => {
    const next: AppSettings = {
      max_concurrent: Number((document.getElementById("set-concurrent") as HTMLInputElement).value) || 3,
      global_speed_limit: (() => {
        const v = (document.getElementById("set-limit") as HTMLInputElement).value;
        return v ? Number(v) : undefined;
      })(),
      default_folder: (document.getElementById("set-folder") as HTMLInputElement).value.trim(),
      clipboard_monitor: (document.getElementById("set-clip") as HTMLInputElement).checked,
      autotune: (document.getElementById("set-autotune") as HTMLInputElement).checked,
    };
    await api.setSettings(next);
    settings = next;
    (document.getElementById("add-folder") as HTMLInputElement).value = next.default_folder;
  });
}

const dlgAdd = document.getElementById("dlg-add") as HTMLDialogElement;
const dlgClip = document.getElementById("dlg-clipboard") as HTMLDialogElement;

function openAddDialog(prefillUrl?: string) {
  const urls = document.getElementById("add-urls") as HTMLTextAreaElement;
  const fname = document.getElementById("add-filename") as HTMLInputElement;
  const folder = document.getElementById("add-folder") as HTMLInputElement;
  folder.value = settings?.default_folder ?? folder.value;
  if (prefillUrl) {
    urls.value = prefillUrl;
    fname.value = basenameFromUrl(prefillUrl);
  }
  dlgAdd.showModal();
}

document.getElementById("btn-add")!.addEventListener("click", () => openAddDialog());
document.getElementById("add-cancel")!.addEventListener("click", () => dlgAdd.close());

document.getElementById("add-auto-conn")!.addEventListener("change", (e) => {
  const on = (e.target as HTMLInputElement).checked;
  (document.getElementById("add-connections") as HTMLInputElement).disabled = on;
});

document.getElementById("btn-pick-folder")!.addEventListener("click", async () => {
  const picked = await open({ directory: true, multiple: false });
  if (picked) (document.getElementById("add-folder") as HTMLInputElement).value = picked as string;
});

document.getElementById("form-add")!.addEventListener("submit", async (e) => {
  e.preventDefault();
  const urls = (document.getElementById("add-urls") as HTMLTextAreaElement).value
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
  const auto = (document.getElementById("add-auto-conn") as HTMLInputElement).checked;
  const args: AddDownloadArgs = {
    urls,
    output_dir: (document.getElementById("add-folder") as HTMLInputElement).value.trim(),
    filename: (document.getElementById("add-filename") as HTMLInputElement).value.trim(),
    connections: Number((document.getElementById("add-connections") as HTMLInputElement).value) || 8,
    connections_auto: auto,
    sha256: (document.getElementById("add-sha256") as HTMLInputElement).value.trim() || undefined,
    md5: (document.getElementById("add-md5") as HTMLInputElement).value.trim() || undefined,
  };
  await api.addDownload(args);
  dlgAdd.close();
  await refreshDownloads();
});

let sidebarMode: "grabber" | "settings" | null = null;
document.getElementById("btn-grabber")!.addEventListener("click", () => {
  sidebarMode = sidebarMode === "grabber" ? null : "grabber";
  showSidebar(sidebarMode);
  if (sidebarMode === "grabber") buildGrabberUi();
});
document.getElementById("btn-settings")!.addEventListener("click", () => {
  sidebarMode = sidebarMode === "settings" ? null : "settings";
  showSidebar(sidebarMode);
  if (sidebarMode === "settings") buildSettingsUi();
});

document.getElementById("btn-theme")!.addEventListener("click", toggleTheme);

document.getElementById("clipboard-dismiss")!.addEventListener("click", () => dlgClip.close());
document.getElementById("clipboard-grab")!.addEventListener("click", () => {
  const url = (document.getElementById("clipboard-url-text") as HTMLElement).textContent ?? "";
  dlgClip.close();
  sidebarMode = "grabber";
  showSidebar("grabber");
  buildGrabberUi();
  (document.getElementById("grab-input") as HTMLTextAreaElement).value = url;
});
document.getElementById("clipboard-queue")!.addEventListener("click", () => {
  const url = (document.getElementById("clipboard-url-text") as HTMLElement).textContent ?? "";
  dlgClip.close();
  openAddDialog(url);
});

async function boot() {
  setTheme((localStorage.getItem(THEME_KEY) as "dark" | "light") || "dark");
  settings = await api.getSettings();
  (document.getElementById("add-folder") as HTMLInputElement).value = settings.default_folder;

  await refreshDownloads();

  await listen("downloads-changed", () => void refreshDownloads());
  await listen<{ id: number; event: { type: string; downloaded?: number; total?: number; speed_bps?: number; index?: number; start?: number; current?: number; end?: number } }>(
    "download-progress",
    (ev) => {
      const { id, event } = ev.payload;
      const d = downloads.find((x) => x.id === id);
      if (!d) return;
      if (event.type === "progress") {
        d.downloaded = event.downloaded ?? d.downloaded;
        d.total = event.total ?? d.total;
        d.speed_bps = event.speed_bps ?? d.speed_bps;
        d.status = "running";
      }
      if (event.type === "segment_progress" && event.index != null) {
        const seg = d.segments.find((s) => s.index === event.index);
        if (seg) {
          seg.downloaded = event.current ?? seg.downloaded;
        } else if (event.start != null && event.end != null) {
          d.segments.push({
            index: event.index,
            start: event.start,
            end: event.end,
            downloaded: event.current ?? 0,
          });
        }
      }
      renderDownloads();
    },
  );

  await listen<string>("clipboard-url", (ev) => {
    (document.getElementById("clipboard-url-text") as HTMLElement).textContent = ev.payload;
    dlgClip.showModal();
  });
}

void boot();
