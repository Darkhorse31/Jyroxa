import { open } from "@tauri-apps/plugin-dialog";
import { api, type AppSettings } from "./api";
import { icons } from "./icons";
import { showToast } from "./toast";

export class SettingsView {
  private container: HTMLElement;
  private settings: AppSettings | null = null;
  private onSettingsSaved: (s: AppSettings) => void;

  constructor(container: HTMLElement, onSettingsSaved: (s: AppSettings) => void) {
    this.container = container;
    this.onSettingsSaved = onSettingsSaved;
  }

  public setSettings(s: AppSettings) {
    this.settings = { ...s };
    this.render();
  }

  private render() {
    if (!this.settings) return;
    const s = this.settings;

    this.container.innerHTML = `
      <div class="panel-section-head">
        <div class="panel-section-title">
          ${icons.settings(18)}
          <h3>Settings</h3>
        </div>
        <p class="panel-section-subtitle">
          Configure queue behavior, download directory, and speed controls.
        </p>
      </div>

      <div class="settings-card">
        <div class="setting-item">
          <div class="setting-info">
            <label class="setting-title" for="set-concurrent">Max concurrent downloads</label>
            <span class="setting-desc">Number of active downloads executing in parallel (1–16).</span>
          </div>
          <div class="setting-control">
            <input
              type="number"
              id="set-concurrent"
              class="form-input input-number"
              min="1"
              max="16"
              value="${s.max_concurrent}"
            />
          </div>
        </div>

        <div class="setting-item">
          <div class="setting-info">
            <label class="setting-title" for="set-limit">Global speed limit</label>
            <span class="setting-desc">Throttle all downloads combined (bytes/s, leave empty for unlimited).</span>
          </div>
          <div class="setting-control">
            <input
              type="number"
              id="set-limit"
              class="form-input"
              min="0"
              placeholder="Unlimited"
              value="${s.global_speed_limit ?? ""}"
            />
            <div class="speed-presets">
              <button type="button" class="btn-preset" data-bytes="0">Unlimited</button>
              <button type="button" class="btn-preset" data-bytes="1048576">1 MB/s</button>
              <button type="button" class="btn-preset" data-bytes="5242880">5 MB/s</button>
              <button type="button" class="btn-preset" data-bytes="10485760">10 MB/s</button>
            </div>
          </div>
        </div>

        <div class="setting-item">
          <div class="setting-info">
            <label class="setting-title" for="set-folder">Default save folder</label>
            <span class="setting-desc">Where files will be saved by default.</span>
          </div>
          <div class="setting-control full-width-control">
            <div class="input-with-btn">
              <input
                type="text"
                id="set-folder"
                class="form-input"
                value="${this.escapeHtml(s.default_folder)}"
              />
              <button type="button" id="btn-browse-default-folder" class="btn secondary">
                ${icons.folder(15)} Browse
              </button>
            </div>
          </div>
        </div>

        <div class="setting-item">
          <div class="setting-info">
            <label class="setting-title" for="set-clip">Clipboard monitor</label>
            <span class="setting-desc">Automatically detect copied download links and prompt to queue them.</span>
          </div>
          <div class="setting-control">
            <label class="switch-toggle">
              <input type="checkbox" id="set-clip" ${s.clipboard_monitor ? "checked" : ""} />
              <span class="switch-slider"></span>
            </label>
          </div>
        </div>

        <div class="setting-item">
          <div class="setting-info">
            <label class="setting-title" for="set-autotune">Auto-tune connections</label>
            <span class="setting-desc">Dynamically calibrate concurrent TCP streams per host for optimal throughput.</span>
          </div>
          <div class="setting-control">
            <label class="switch-toggle">
              <input type="checkbox" id="set-autotune" ${s.autotune ? "checked" : ""} />
              <span class="switch-slider"></span>
            </label>
          </div>
        </div>

        <div class="settings-actions">
          <button type="button" id="btn-save-settings" class="btn primary">
            ${icons.check(15)} Save settings
          </button>
        </div>
      </div>
    `;

    this.wireEvents();
  }

  private wireEvents() {
    const browseBtn = this.container.querySelector("#btn-browse-default-folder") as HTMLButtonElement;
    const saveBtn = this.container.querySelector("#btn-save-settings") as HTMLButtonElement;
    const limitInput = this.container.querySelector("#set-limit") as HTMLInputElement;

    browseBtn?.addEventListener("click", async () => {
      const picked = await open({ directory: true, multiple: false });
      if (picked) {
        const folderInput = this.container.querySelector("#set-folder") as HTMLInputElement;
        if (folderInput) folderInput.value = picked as string;
      }
    });

    this.container.querySelectorAll(".btn-preset").forEach((btn) => {
      btn.addEventListener("click", () => {
        const bytes = Number((btn as HTMLElement).dataset.bytes || "0");
        if (limitInput) {
          limitInput.value = bytes > 0 ? String(bytes) : "";
        }
      });
    });

    saveBtn?.addEventListener("click", async () => {
      saveBtn.disabled = true;
      saveBtn.innerHTML = `${icons.refresh(15)} Saving...`;

      const limitVal = (this.container.querySelector("#set-limit") as HTMLInputElement).value.trim();
      const next: AppSettings = {
        max_concurrent: Math.min(
          16,
          Math.max(1, Number((this.container.querySelector("#set-concurrent") as HTMLInputElement).value) || 3),
        ),
        global_speed_limit: limitVal ? Number(limitVal) : undefined,
        default_folder: (this.container.querySelector("#set-folder") as HTMLInputElement).value.trim(),
        clipboard_monitor: (this.container.querySelector("#set-clip") as HTMLInputElement).checked,
        autotune: (this.container.querySelector("#set-autotune") as HTMLInputElement).checked,
      };

      try {
        await api.setSettings(next);
        this.settings = next;
        this.onSettingsSaved(next);
        showToast("Settings saved successfully!");
      } catch (e) {
        showToast(`Failed to save settings: ${e}`);
      } finally {
        saveBtn.disabled = false;
        saveBtn.innerHTML = `${icons.check(15)} Save settings`;
      }
    });
  }

  private escapeHtml(text: string): string {
    const div = document.createElement("div");
    div.textContent = text;
    return div.innerHTML;
  }
}
