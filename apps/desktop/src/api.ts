import { invoke } from "@tauri-apps/api/core";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
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
  global_speed_limit?: number;
  default_folder: string;
  clipboard_monitor: boolean;
  autotune: boolean;
}

export interface AddDownloadArgs {
  urls: string[];
  output_dir: string;
  filename: string;
  connections: number;
  connections_auto: boolean;
  sha256?: string;
  md5?: string;
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
  grabLinks: (input: string, options: GrabOptionsDto) =>
    invoke<Package[]>("grab_links", { input, options }),
  probeLinks: (urls: string[]) =>
    invoke<LinkInfo[]>("probe_links", { urls }),
};

export async function openFolder(filePath: string): Promise<boolean> {
  try {
    await revealItemInDir(filePath);
    return true;
  } catch (_e1) {
    try {
      const parent = filePath.replace(/[/\\][^/\\]+$/, "");
      await openPath(parent || filePath);
      return true;
    } catch (_e2) {
      console.warn("Could not reveal file or open directory:", filePath);
      return false;
    }
  }
}

export async function copyToClipboard(text: string): Promise<boolean> {
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
