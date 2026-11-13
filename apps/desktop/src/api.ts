import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

export type TaskStatus =
  | "queued"
  | "starting"
  | "connecting"
  | "running"
  | "downloading"
  | "paused"
  | "retrying"
  | "verifying"
  | "completed"
  | "done"
  | "failed"
  | "cancelled";

export type FileCategory =
  | "Video"
  | "Music"
  | "Documents"
  | "Compressed"
  | "Programs"
  | "Other";

export interface SegmentState {
  index: number;
  start: number;
  end: number;
  downloaded: number;
}

export interface DownloadView {
  id: number;
  urls: string[];
  filename: string;
  output: string;
  connections: number;
  connections_auto: boolean;
  sha256?: string;
  md5?: string;
  status: TaskStatus;
  downloaded: number;
  total?: number;
  speed_bps: number;
  segments: SegmentState[];
  error?: string;
}

export interface AppSettings {
  max_concurrent: number;
  max_connections_per_download: number;
  per_host_cap: number;
  global_speed_limit?: number | null;
  download_dir: string;
  default_folder?: string;
  category_dirs: Record<string, string>;
  clipboard_monitor: boolean;
  autotune: boolean;
  start_minimized: boolean;
  notifications: boolean;
  close_to_tray: boolean;
  start_with_windows: boolean;
  prevent_sleep: boolean;
  auto_resume: boolean;
  theme: string;
  scheduler_start?: string | null;
  scheduler_stop?: string | null;
  scheduler_days: number[];
  browser_integration: BrowserIntegrationSettings;
}

export interface BrowserFileTypes {
  archives: boolean;
  programs: boolean;
  video: boolean;
  audio: boolean;
  documents: boolean;
  iso: boolean;
}

export interface BrowserIntegrationSettings {
  enabled: boolean;
  min_size_bytes: number;
  file_types: BrowserFileTypes;
  silent_add: boolean;
  disabled_hosts: string[];
  chrome_extension_id: string;
  edge_extension_id: string;
}

export interface BrowserIntegrationStatus {
  token_present: boolean;
  host_manifest_present: boolean;
  chrome_registry: boolean;
  edge_registry: boolean;
  firefox_registry: boolean;
  native_host_installed: boolean;
}

export interface AddDownloadArgs {
  urls: string[];
  output_dir: string;
  filename: string;
  connections: number;
  connections_auto: boolean;
  sha256?: string;
  md5?: string;
  http_headers?: Record<string, string>;
}

export interface GrabOptionsDto {
  deep: boolean;
  include_ext?: string[];
  min_size?: number;
  max_size?: number;
  host_filter?: string;
  no_probe: boolean;
}

export interface LinkInfo {
  url: string;
  filename?: string;
  size?: number;
  mime?: string;
  resumable: boolean;
}

export interface Package {
  name: string;
  links: LinkInfo[];
}

export interface ProbeInfo {
  final_url: string;
  suggested_name: string;
  size?: number | null;
  mime?: string | null;
  resumable: boolean;
  category: FileCategory;
  server: string;
  protocol: string;
}

export interface DownloadFinishedPayload {
  id: number;
  path: string;
  filename: string;
  total: number;
}

export interface BackendProgressEvent {
  type: string;
  downloaded?: number;
  total?: number;
  speed_bps?: number;
  connections?: number;
  eta_secs?: number;
  avg_speed_bps?: number;
  elapsed_secs?: number;
  active_connections?: number;
  status?: string;
  resumable?: boolean;
  server?: string;
  protocol?: string;
  segments?: SegmentState[];
  index?: number;
  start?: number;
  current?: number;
  end?: number;
  count?: number;
  reason?: string;
  attempt?: number;
  error?: string;
  total_size?: number;
  average_bps?: number;
  supports_ranges?: boolean;
  resumed_from?: number;
}

export interface ProgressPayload {
  id: number;
  event: BackendProgressEvent;
}

export const api = {
  getSettings: () => invoke<AppSettings>("get_settings"),
  setSettings: (s: AppSettings) => invoke<void>("set_settings", { settings: s }),
  listDownloads: () => invoke<DownloadView[]>("list_downloads"),
  addDownload: (args: AddDownloadArgs) => invoke<number>("add_download", { args }),
  pauseDownload: (id: number) => invoke<boolean>("pause_download", { id }),
  resumeDownload: (id: number) => invoke<boolean>("resume_download", { id }),
  cancelDownload: (id: number) => invoke<boolean>("cancel_download", { id }),
  pauseMany: (ids: number[]) => invoke<number>("pause_many", { ids }),
  resumeMany: (ids: number[]) => invoke<number>("resume_many", { ids }),
  cancelMany: (ids: number[]) => invoke<number>("cancel_many", { ids }),
  removeDownload: (id: number, deleteFile: boolean) =>
    invoke<boolean>("remove_download", { id, deleteFile: Boolean(deleteFile) }),
  removeMany: (ids: number[], deleteFile: boolean) =>
    invoke<number>("remove_many", { ids, deleteFile: Boolean(deleteFile) }),
  retryDownload: (id: number) => invoke<boolean>("retry_download", { id }),
  grabLinks: (input: string, options: GrabOptionsDto) =>
    invoke<Package[]>("grab_links", { input, options }),
  probeLinks: (urls: string[]) =>
    invoke<LinkInfo[]>("probe_links", { urls }),
  probeUrl: (url: string) =>
    invoke<ProbeInfo>("probe_url", { url }),
  suggestSavePath: (dir: string, name: string) =>
    invoke<string>("suggest_save_path", { dir, name }),
  defaultCategoryDirs: () =>
    invoke<Record<string, string>>("default_category_dirs"),
  openFolderCmd: (path: string) =>
    invoke<void>("open_folder_cmd", { path }),
  openFile: (path: string) =>
    invoke<void>("open_file", { path }),
  getBrowserIntegrationStatus: () =>
    invoke<BrowserIntegrationStatus>("get_browser_integration_status"),
  installBrowserIntegration: () =>
    invoke<void>("install_browser_integration_cmd"),
  uninstallBrowserIntegration: () =>
    invoke<void>("uninstall_browser_integration_cmd"),
};

export async function openFolder(filePath: string): Promise<boolean> {
  try {
    await api.openFolderCmd(filePath);
    return true;
  } catch (e) {
    console.warn("Could not open folder:", filePath, e);
    return false;
  }
}

export async function openDownloadedFile(filePath: string): Promise<boolean> {
  try {
    await api.openFile(filePath);
    return true;
  } catch (e) {
    console.warn("Could not open file:", filePath, e);
    return false;
  }
}

// Text the app itself put on the clipboard (Copy URL / Copy path). The
// backend clipboard monitor must not offer it back as a new download.
let lastSelfCopied: string | null = null;

export function isSelfCopied(text: string): boolean {
  return lastSelfCopied !== null && text.trim() === lastSelfCopied.trim();
}

export async function copyToClipboard(text: string): Promise<boolean> {
  lastSelfCopied = text;
  try {
    await writeText(text);
    return true;
  } catch (_e1) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch (_e2) {
      console.warn("Could not copy to clipboard:", text);
      return false;
    }
  }
}
