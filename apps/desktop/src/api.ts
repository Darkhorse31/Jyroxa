import { invoke } from "@tauri-apps/api/core";

export type TaskStatus =
  | "queued"
  | "starting"
  | "running"
  | "paused"
  | "completed"
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
};
