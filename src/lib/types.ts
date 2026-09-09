// API-facing DTOs returned by the Rust commands.

export interface GalleryDetail {
  gid: number;
  token: string;
  title: string;
  titleJpn?: string;
  thumb?: string;
  category: string;
  uploader: string;
  posted: string;
  language: string;
  size: string;
  pages: number;
  favoriteCount: number;
  rating: number;
  ratingCount: number;
  torrentCount: number;
  torrentUrl: string;
  archiveUrl: string;
  isFavorited: boolean;
  favoriteName?: string;
  tags: { name: string; tags: string[] }[];
  comments: {
    id?: number;
    user: string;
    avatar?: string;
    score: number;
    time: string;
    comment: string;
  }[];
  previewPages: number;
}

export interface GalleryToken {
  gid: number;
  token: string;
}

export interface PreviewItem {
  index: number;
  imageUrl: string;
  pageUrl: string;
  width?: number;
  height?: number;
  /** Sprite cut-out (shared montage): pixel offsets + clip size. */
  xOffset?: number;
  yOffset?: number;
  clipWidth?: number;
  clipHeight?: number;
}

export type DownloadState = "none" | "wait" | "downloading" | "finished" | "failed";

export interface DownloadItem {
  gid: number;
  token: string;
  title: string;
  label: string;
  state: string;
  total: number;
  complete: number;
  dir: string;
  url: string;
}

export interface DownloadProgress {
  gid: number;
  complete: number;
  total: number;
}

// ----- settings -----
export interface CookiePair {
  name: string;
  value: string;
}

export interface AppSettings {
  site: number;
  config: Record<string, unknown>;
  session_cookies: CookiePair[];
  tag_translation_enabled: boolean;
  tag_translation_file: string | null;
  thumb_resolution: number;
  reading_direction: string;
  zoom_mode: string;
  reading_start_position: string;
  download_dir: string | null;
  download_threads: number;
  download_always_original: boolean;
  download_list_page_size: number;
  download_interval_secs: number;
  proxy_type: number;
  proxy_url: string | null;
  hosts_override: string;
  timeout_secs: number;
  max_retries: number;
  image_cache_size_mb: number;
  custom_host: string | null;
  doh_url: string;
  use_builtin_hosts: boolean;
}
