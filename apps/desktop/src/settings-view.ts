import { open } from "@tauri-apps/plugin-dialog";
import { api, type AppSettings, type TelegramAccountView } from "./api";
import { APP_NAME } from "./brand";
import { icons } from "./icons";
import { requestNotificationPermission, showToast } from "./toast";

export type SettingsTab =
  | "general"
  | "connections"
  | "speed"
  | "scheduler"
  | "clipboard"
  | "theme"
  | "telegram";

export class SettingsView {
  private container: HTMLElement;
  private settings: AppSettings | null = null;
  private onSettingsSaved: (s: AppSettings) => void;
  private activeTab: SettingsTab = "general";

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
          Configure queue behavior, connections, scheduler, and system integration.
        </p>
      </div>

      <div class="settings-tabs-bar" role="tablist">
        <button type="button" class="settings-tab-btn ${this.activeTab === "general" ? "active" : ""}" data-tab="general" role="tab">
          ${icons.folder(14)}
          <span>General</span>
        </button>
        <button type="button" class="settings-tab-btn ${this.activeTab === "connections" ? "active" : ""}" data-tab="connections" role="tab">
          ${icons.wifi(14)}
          <span>Connections</span>
        </button>
        <button type="button" class="settings-tab-btn ${this.activeTab === "speed" ? "active" : ""}" data-tab="speed" role="tab">
          ${icons.gauge(14)}
          <span>Speed limiter</span>
        </button>
        <button type="button" class="settings-tab-btn ${this.activeTab === "scheduler" ? "active" : ""}" data-tab="scheduler" role="tab">
          ${icons.clock(14)}
          <span>Scheduler</span>
        </button>
        <button type="button" class="settings-tab-btn ${this.activeTab === "clipboard" ? "active" : ""}" data-tab="clipboard" role="tab">
          ${icons.clipboard(14)}
          <span>Clipboard</span>
        </button>
        <button type="button" class="settings-tab-btn ${this.activeTab === "theme" ? "active" : ""}" data-tab="theme" role="tab">
          ${icons.sun(14)}
          <span>Theme</span>
        </button>
        <button type="button" class="settings-tab-btn ${this.activeTab === "telegram" ? "active" : ""}" data-tab="telegram" role="tab">
          ${icons.clipboard(14)}
          <span>Telegram</span>
        </button>
      </div>

      <div class="settings-tab-content">
        <!-- GENERAL SECTION -->
        <div class="settings-section ${this.activeTab === "general" ? "" : "hidden"}" id="sec-general">
          <div class="settings-card">
            <h4 class="settings-group-title">Directories</h4>

            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-title" for="set-folder">Default save folder</label>
                <span class="setting-desc">Primary folder for incoming downloads.</span>
              </div>
              <div class="setting-control full-width-control">
                <div class="input-with-btn">
                  <input
                    type="text"
                    id="set-folder"
                    class="form-input"
                    value="${this.escapeHtml(s.download_dir || s.default_folder || "")}"
                  />
                  <button type="button" id="btn-browse-default-folder" class="btn secondary">
                    ${icons.folder(15)} Browse
                  </button>
                </div>
              </div>
            </div>

            <div class="category-dirs-group">
              <span class="setting-title">Category download subfolders</span>
              <span class="setting-desc">Files automatically route to category folders when probed.</span>
              <div class="category-dirs-grid">
                ${["Video", "Music", "Documents", "Compressed", "Programs", "Other"]
                  .map(
                    (cat) => `
                  <div class="cat-dir-row">
                    <span class="cat-dir-label">${cat}</span>
                    <input type="text" class="form-input input-sm cat-dir-input" data-category="${cat}" value="${this.escapeHtml(s.category_dirs?.[cat] || "")}" placeholder="Default folder" />
                    <button type="button" class="btn-mini btn-pick-cat-dir" data-category="${cat}">Browse</button>
                  </div>
                `
                  )
                  .join("")}
              </div>
            </div>
          </div>

          <div class="settings-card">
            <h4 class="settings-group-title">Background & System</h4>

            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-title" for="set-tray">Close to system tray</label>
                <span class="setting-desc">Closing window keeps background downloads active in system tray.</span>
              </div>
              <div class="setting-control">
                <label class="switch-toggle">
                  <input type="checkbox" id="set-tray" ${s.close_to_tray ? "checked" : ""} />
                  <span class="switch-slider"></span>
                </label>
              </div>
            </div>

            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-title" for="set-startup">Start with Windows</label>
                <span class="setting-desc">Launch ${APP_NAME} automatically when logging into Windows.</span>
              </div>
              <div class="setting-control">
                <label class="switch-toggle">
                  <input type="checkbox" id="set-startup" ${s.start_with_windows ? "checked" : ""} />
                  <span class="switch-slider"></span>
                </label>
              </div>
            </div>

            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-title" for="set-minimized">Start minimized</label>
                <span class="setting-desc">Start quietly in the system tray on startup without showing main window.</span>
              </div>
              <div class="setting-control">
                <label class="switch-toggle">
                  <input type="checkbox" id="set-minimized" ${s.start_minimized ? "checked" : ""} />
                  <span class="switch-slider"></span>
                </label>
              </div>
            </div>

            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-title" for="set-notifications">Notifications on finish/failure</label>
                <span class="setting-desc">Display desktop toast and notification when download completes or errors.</span>
              </div>
              <div class="setting-control">
                <label class="switch-toggle">
                  <input type="checkbox" id="set-notifications" ${s.notifications ? "checked" : ""} />
                  <span class="switch-slider"></span>
                </label>
              </div>
            </div>

            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-title" for="set-prevent-sleep">Prevent sleep while downloading</label>
                <span class="setting-desc">Keep operating system awake until all active downloads finish.</span>
              </div>
              <div class="setting-control">
                <label class="switch-toggle">
                  <input type="checkbox" id="set-prevent-sleep" ${s.prevent_sleep ? "checked" : ""} />
                  <span class="switch-slider"></span>
                </label>
              </div>
            </div>

            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-title" for="set-auto-resume">Auto-resume interrupted downloads</label>
                <span class="setting-desc">Automatically resume uncompleted downloads when ${APP_NAME} starts.</span>
              </div>
              <div class="setting-control">
                <label class="switch-toggle">
                  <input type="checkbox" id="set-auto-resume" ${s.auto_resume ? "checked" : ""} />
                  <span class="switch-slider"></span>
                </label>
              </div>
            </div>
          </div>
        </div>

        <!-- CONNECTIONS SECTION -->
        <div class="settings-section ${this.activeTab === "connections" ? "" : "hidden"}" id="sec-connections">
          <div class="settings-card">
            <h4 class="settings-group-title">Concurrency & Sockets</h4>

            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-title" for="set-concurrent">Max concurrent downloads</label>
                <span class="setting-desc">Number of active file downloads executing in parallel (1–16).</span>
              </div>
              <div class="setting-control">
                <input
                  type="number"
                  id="set-concurrent"
                  class="form-input input-number"
                  min="1"
                  max="16"
                  value="${s.max_concurrent || 3}"
                />
              </div>
            </div>

            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-title" for="set-conn-per-download">Connections per download</label>
                <span class="setting-desc">Max HTTP TCP segment streams per file (1–32).</span>
              </div>
              <div class="setting-control">
                <input
                  type="number"
                  id="set-conn-per-download"
                  class="form-input input-number"
                  min="1"
                  max="32"
                  value="${s.max_connections_per_download || 8}"
                />
              </div>
            </div>

            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-title" for="set-per-host-cap">Per-host connection limit</label>
                <span class="setting-desc">Prevent servers from throttling by capping concurrent streams per host (1–64).</span>
              </div>
              <div class="setting-control">
                <input
                  type="number"
                  id="set-per-host-cap"
                  class="form-input input-number"
                  min="1"
                  max="64"
                  value="${s.per_host_cap || 16}"
                />
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
          </div>
        </div>

        <!-- SPEED LIMITER SECTION -->
        <div class="settings-section ${this.activeTab === "speed" ? "" : "hidden"}" id="sec-speed">
          <div class="settings-card">
            <h4 class="settings-group-title">Bandwidth Throttling</h4>

            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-title" for="set-limit">Global speed limit (bytes/sec)</label>
                <span class="setting-desc">Throttle total bandwidth consumed across all downloads. 0 or empty for unlimited.</span>
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
                  <button type="button" class="btn-preset" data-bytes="26214400">25 MB/s</button>
                  <button type="button" class="btn-preset" data-bytes="52428800">50 MB/s</button>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- SCHEDULER SECTION -->
        <div class="settings-section ${this.activeTab === "scheduler" ? "" : "hidden"}" id="sec-scheduler">
          <div class="settings-card">
            <h4 class="settings-group-title">Download Schedule</h4>
            <p class="form-hint">Automatically start and pause the download queue at designated hours and days.</p>

            <div class="form-row-split">
              <div class="setting-item">
                <div class="setting-info">
                  <label class="setting-title" for="set-sched-start">Start time</label>
                  <span class="setting-desc">Time of day to resume downloads.</span>
                </div>
                <div class="setting-control">
                  <input type="time" id="set-sched-start" class="form-input" value="${s.scheduler_start || ""}" />
                </div>
              </div>

              <div class="setting-item">
                <div class="setting-info">
                  <label class="setting-title" for="set-sched-stop">Stop time</label>
                  <span class="setting-desc">Time of day to pause downloads.</span>
                </div>
                <div class="setting-control">
                  <input type="time" id="set-sched-stop" class="form-input" value="${s.scheduler_stop || ""}" />
                </div>
              </div>
            </div>

            <div class="setting-item">
              <div class="setting-info">
                <span class="setting-title">Active Days</span>
                <span class="setting-desc">Days of week when the scheduler applies.</span>
              </div>
              <div class="days-picker-row">
                ${["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
                  .map((dayName, dayIdx) => {
                    const isActive = s.scheduler_days?.includes(dayIdx) ?? true;
                    return `
                    <button type="button" class="day-chip ${isActive ? "active" : ""}" data-day="${dayIdx}">
                      ${dayName}
                    </button>
                  `;
                  })
                  .join("")}
              </div>
            </div>
          </div>
        </div>

        <!-- CLIPBOARD SECTION -->
        <div class="settings-section ${this.activeTab === "clipboard" ? "" : "hidden"}" id="sec-clipboard">
          <div class="settings-card">
            <h4 class="settings-group-title">Clipboard URL Monitor</h4>

            <div class="setting-item">
              <div class="setting-info">
                <label class="setting-title" for="set-clip">Monitor clipboard for download URLs</label>
                <span class="setting-desc">Detect copied links automatically and prompt to probe or queue them.</span>
              </div>
              <div class="setting-control">
                <label class="switch-toggle">
                  <input type="checkbox" id="set-clip" ${s.clipboard_monitor ? "checked" : ""} />
                  <span class="switch-slider"></span>
                </label>
              </div>
            </div>
          </div>
        </div>

        <!-- THEME SECTION -->
        <div class="settings-section ${this.activeTab === "theme" ? "" : "hidden"}" id="sec-theme">
          <div class="settings-card">
            <h4 class="settings-group-title">Interface Theme</h4>

            <div class="theme-picker-row">
              <label class="theme-option ${s.theme === "system" ? "active" : ""}">
                <input type="radio" name="set-theme-radio" value="system" ${s.theme === "system" ? "checked" : ""} />
                <span class="theme-option-box">
                  ${icons.settings(22)}
                  <span class="theme-label">System default</span>
                </span>
              </label>

              <label class="theme-option ${s.theme === "dark" ? "active" : ""}">
                <input type="radio" name="set-theme-radio" value="dark" ${s.theme === "dark" ? "checked" : ""} />
                <span class="theme-option-box">
                  ${icons.moon(22)}
                  <span class="theme-label">Dark</span>
                </span>
              </label>

              <label class="theme-option ${s.theme === "light" ? "active" : ""}">
                <input type="radio" name="set-theme-radio" value="light" ${s.theme === "light" ? "checked" : ""} />
                <span class="theme-option-box">
                  ${icons.sun(22)}
                  <span class="theme-label">Light</span>
                </span>
              </label>
            </div>
          </div>
        </div>

        <div class="settings-section ${this.activeTab === "telegram" ? "" : "hidden"}" id="sec-telegram">
          <div class="settings-card">
            <h4 class="settings-group-title">Telegram</h4>
            <p class="setting-desc">
              Public <code>t.me/channel/id</code> posts download without login. Private <code>t.me/c/…</code> posts use your own account (API credentials from <a href="https://my.telegram.org" target="_blank" rel="noreferrer">my.telegram.org</a>).
            </p>
            <p id="tg-status-line" class="setting-desc">Checking connection…</p>

            <div class="setting-item">
              <label class="setting-title" for="tg-api-id">API ID</label>
              <input type="number" id="tg-api-id" class="form-input" placeholder="12345678" />
            </div>
            <div class="setting-item">
              <label class="setting-title" for="tg-api-hash">API hash</label>
              <input type="text" id="tg-api-hash" class="form-input" placeholder="0123456789abcdef0123456789abcdef" />
            </div>
            <button type="button" id="tg-save-creds" class="btn secondary">Save API credentials</button>

            <div class="setting-item">
              <label class="setting-title" for="tg-phone">Phone (international)</label>
              <input type="text" id="tg-phone" class="form-input" placeholder="+1234567890" />
            </div>
            <button type="button" id="tg-send-code" class="btn secondary">Send login code</button>

            <div class="setting-item">
              <label class="setting-title" for="tg-code">Login code</label>
              <input type="text" id="tg-code" class="form-input" placeholder="12345" />
            </div>
            <button type="button" id="tg-verify-code" class="btn secondary">Verify code</button>

            <div class="setting-item">
              <label class="setting-title" for="tg-password">2FA password (if enabled)</label>
              <input type="password" id="tg-password" class="form-input" />
            </div>
            <button type="button" id="tg-verify-password" class="btn secondary">Submit 2FA password</button>

            <button type="button" id="tg-sign-out" class="btn danger" style="margin-top: 12px;">Sign out</button>
          </div>
        </div>
      </div>

      <div class="settings-footer">
        <button type="button" id="btn-save-settings" class="btn primary">
          ${icons.check(15)} Save settings
        </button>
      </div>
    `;

    this.wireListeners();
    if (this.activeTab === "telegram") {
      void this.refreshTelegramStatus();
    }
  }

  private async refreshTelegramStatus() {
    const line = this.container.querySelector("#tg-status-line");
    if (!line) return;
    try {
      const st: TelegramAccountView = await api.telegramStatus();
      if (st.connected) {
        const who = st.display_name || st.phone || "Connected";
        line.textContent = `Connected as ${who}`;
      } else {
        line.textContent = "Not connected — required for private t.me/c/… links only.";
      }
      if (st.api_id) {
        const apiId = this.container.querySelector("#tg-api-id") as HTMLInputElement;
        if (apiId && !apiId.value) apiId.value = String(st.api_id);
      }
      if (st.phone) {
        const phone = this.container.querySelector("#tg-phone") as HTMLInputElement;
        if (phone && !phone.value) phone.value = st.phone;
      }
    } catch (e) {
      line.textContent = `Telegram status error: ${e}`;
    }
  }

  private wireListeners() {
    // Tabs switching
    this.container.querySelectorAll(".settings-tab-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        const tab = (btn as HTMLElement).dataset.tab as SettingsTab;
        this.activeTab = tab;
        this.render();
      });
    });

    // Speed presets
    this.container.querySelectorAll(".btn-preset").forEach((btn) => {
      btn.addEventListener("click", () => {
        const bytes = (btn as HTMLElement).dataset.bytes;
        const limitInput = this.container.querySelector("#set-limit") as HTMLInputElement;
        if (limitInput) {
          limitInput.value = bytes === "0" ? "" : bytes || "";
        }
      });
    });

    // Browse default folder
    const btnBrowse = this.container.querySelector("#btn-browse-default-folder") as HTMLButtonElement;
    btnBrowse?.addEventListener("click", async () => {
      const picked = await open({ directory: true, multiple: false });
      if (picked) {
        const folderInput = this.container.querySelector("#set-folder") as HTMLInputElement;
        if (folderInput) folderInput.value = picked as string;
      }
    });

    // Browse category folders
    this.container.querySelectorAll(".btn-pick-cat-dir").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const cat = (btn as HTMLElement).dataset.category;
        const picked = await open({ directory: true, multiple: false });
        if (picked && cat) {
          const input = this.container.querySelector(`.cat-dir-input[data-category="${cat}"]`) as HTMLInputElement;
          if (input) input.value = picked as string;
        }
      });
    });

    // Days picker
    this.container.querySelectorAll(".day-chip").forEach((chip) => {
      chip.addEventListener("click", () => {
        chip.classList.toggle("active");
      });
    });

    // Notifications permission check
    const notifToggle = this.container.querySelector("#set-notifications") as HTMLInputElement;
    notifToggle?.addEventListener("change", async () => {
      if (notifToggle.checked) {
        const granted = await requestNotificationPermission();
        if (!granted) {
          showToast("Desktop notification permission was denied or dismissed");
        }
      }
    });

    // Save button
    const saveBtn = this.container.querySelector("#btn-save-settings") as HTMLButtonElement;
    saveBtn?.addEventListener("click", () => this.handleSave());

    const tgSaveCreds = this.container.querySelector("#tg-save-creds");
    tgSaveCreds?.addEventListener("click", async () => {
      const apiId = Number((this.container.querySelector("#tg-api-id") as HTMLInputElement)?.value);
      const apiHash = (this.container.querySelector("#tg-api-hash") as HTMLInputElement)?.value.trim();
      if (!apiId || !apiHash) {
        showToast("Enter API ID and API hash from my.telegram.org");
        return;
      }
      try {
        await api.telegramSetCredentials(apiId, apiHash);
        showToast("Telegram API credentials saved");
      } catch (e) {
        showToast(`Failed: ${e}`);
      }
    });

    this.container.querySelector("#tg-send-code")?.addEventListener("click", async () => {
      const phone = (this.container.querySelector("#tg-phone") as HTMLInputElement)?.value.trim();
      if (!phone) {
        showToast("Enter phone number with country code");
        return;
      }
      try {
        const step = await api.telegramBeginLogin(phone);
        showToast(`Telegram: ${JSON.stringify(step)}`);
      } catch (e) {
        showToast(`Login failed: ${e}`);
      }
    });

    this.container.querySelector("#tg-verify-code")?.addEventListener("click", async () => {
      const code = (this.container.querySelector("#tg-code") as HTMLInputElement)?.value.trim();
      if (!code) {
        showToast("Enter the SMS/Telegram code");
        return;
      }
      try {
        const step = await api.telegramSubmitCode(code);
        showToast(`Telegram: ${JSON.stringify(step)}`);
        await this.refreshTelegramStatus();
      } catch (e) {
        showToast(`Code rejected: ${e}`);
      }
    });

    this.container.querySelector("#tg-verify-password")?.addEventListener("click", async () => {
      const password = (this.container.querySelector("#tg-password") as HTMLInputElement)?.value;
      if (!password) {
        showToast("Enter your 2FA password");
        return;
      }
      try {
        const step = await api.telegramSubmitPassword(password);
        showToast(`Telegram: ${JSON.stringify(step)}`);
        await this.refreshTelegramStatus();
      } catch (e) {
        showToast(`2FA failed: ${e}`);
      }
    });

    this.container.querySelector("#tg-sign-out")?.addEventListener("click", async () => {
      try {
        await api.telegramSignOut();
        showToast("Signed out of Telegram");
        await this.refreshTelegramStatus();
      } catch (e) {
        showToast(`Sign out failed: ${e}`);
      }
    });
  }

  private async handleSave() {
    if (!this.settings) return;

    const folderInput = this.container.querySelector("#set-folder") as HTMLInputElement;
    const concurrentInput = this.container.querySelector("#set-concurrent") as HTMLInputElement;
    const connPerDlInput = this.container.querySelector("#set-conn-per-download") as HTMLInputElement;
    const perHostInput = this.container.querySelector("#set-per-host-cap") as HTMLInputElement;
    const limitInput = this.container.querySelector("#set-limit") as HTMLInputElement;
    const clipToggle = this.container.querySelector("#set-clip") as HTMLInputElement;
    const autotuneToggle = this.container.querySelector("#set-autotune") as HTMLInputElement;
    const trayToggle = this.container.querySelector("#set-tray") as HTMLInputElement;
    const startupToggle = this.container.querySelector("#set-startup") as HTMLInputElement;
    const minToggle = this.container.querySelector("#set-minimized") as HTMLInputElement;
    const notifToggle = this.container.querySelector("#set-notifications") as HTMLInputElement;
    const sleepToggle = this.container.querySelector("#set-prevent-sleep") as HTMLInputElement;
    const autoResumeToggle = this.container.querySelector("#set-auto-resume") as HTMLInputElement;
    const schedStartInput = this.container.querySelector("#set-sched-start") as HTMLInputElement;
    const schedStopInput = this.container.querySelector("#set-sched-stop") as HTMLInputElement;

    const selectedThemeRadio = this.container.querySelector('input[name="set-theme-radio"]:checked') as HTMLInputElement;

    // Collect category dirs
    const category_dirs: Record<string, string> = {};
    this.container.querySelectorAll<HTMLInputElement>(".cat-dir-input").forEach((inp) => {
      const cat = inp.dataset.category;
      if (cat && inp.value.trim()) {
        category_dirs[cat] = inp.value.trim();
      }
    });

    // Collect active days
    const activeDays: number[] = [];
    this.container.querySelectorAll<HTMLButtonElement>(".day-chip.active").forEach((chip) => {
      const d = Number(chip.dataset.day);
      if (!isNaN(d)) activeDays.push(d);
    });

    const folderVal = folderInput ? folderInput.value.trim() : this.settings.download_dir;

    const updated: AppSettings = {
      ...this.settings,
      max_concurrent: concurrentInput ? Math.max(1, Math.min(16, Number(concurrentInput.value) || 3)) : this.settings.max_concurrent,
      max_connections_per_download: connPerDlInput ? Math.max(1, Math.min(32, Number(connPerDlInput.value) || 8)) : this.settings.max_connections_per_download,
      per_host_cap: perHostInput ? Math.max(1, Math.min(64, Number(perHostInput.value) || 16)) : this.settings.per_host_cap,
      global_speed_limit: limitInput?.value.trim() ? Number(limitInput.value) : null,
      download_dir: folderVal,
      category_dirs,
      clipboard_monitor: clipToggle ? clipToggle.checked : this.settings.clipboard_monitor,
      autotune: autotuneToggle ? autotuneToggle.checked : this.settings.autotune,
      close_to_tray: trayToggle ? trayToggle.checked : this.settings.close_to_tray,
      start_with_windows: startupToggle ? startupToggle.checked : this.settings.start_with_windows,
      start_minimized: minToggle ? minToggle.checked : this.settings.start_minimized,
      notifications: notifToggle ? notifToggle.checked : this.settings.notifications,
      prevent_sleep: sleepToggle ? sleepToggle.checked : this.settings.prevent_sleep,
      auto_resume: autoResumeToggle ? autoResumeToggle.checked : this.settings.auto_resume,
      scheduler_start: schedStartInput?.value ? schedStartInput.value : null,
      scheduler_stop: schedStopInput?.value ? schedStopInput.value : null,
      scheduler_days: activeDays,
      theme: selectedThemeRadio?.value || this.settings.theme || "system",
    };

    try {
      const { default_folder: _legacy, ...settingsForBackend } = updated;
      await api.setSettings(settingsForBackend as AppSettings);
      this.settings = { ...settingsForBackend, default_folder: folderVal };
      this.onSettingsSaved(this.settings);
      showToast("Settings saved successfully");
    } catch (err) {
      showToast(`Failed to save settings: ${err}`);
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
