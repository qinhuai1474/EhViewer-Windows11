import { invoke } from "@tauri-apps/api/core";
import type { ListUrlParams, GalleryListResult } from "../types";
import type { GalleryDetail, PreviewItem, GalleryToken } from "./types";
import {
  fixtureDetail,
  fixtureList,
  fixtureOnlinePage,
  fixturePreviews,
  fixtureToken,
  isMock,
} from "./fixture";

export function getGalleryList(url: string): Promise<GalleryListResult> {
  if (isMock()) return Promise.resolve(fixtureList);
  return invoke<GalleryListResult>("get_gallery_list", { url });
}

export type NetDiagStep = { label: string; status: string; detail: string };
export type NetDiag = { ok: boolean; proxy: string; steps: NetDiagStep[] };

export function networkDiag(): Promise<NetDiag> {
  if (isMock())
    return Promise.resolve({
      ok: true,
      proxy: "",
      steps: [{ label: "网络诊断", status: "跳过", detail: "mock 环境不执行真实诊断" }],
    });
  return invoke<NetDiag>("network_diag");
}

export function buildListUrl(p: ListUrlParams): Promise<string> {
  return invoke<string>("build_list_url", {
    site: p.site,
    mode: p.mode,
    keyword: p.keyword,
    category: p.category,
    page: p.page,
    advance: p.advance,
    minRating: p.minRating,
    pageFrom: p.pageFrom,
    pageTo: p.pageTo,
  });
}

export function getGalleryDetail(
  site: number,
  gid: number,
  token: string,
  index = 0,
): Promise<GalleryDetail> {
  if (isMock()) return Promise.resolve(fixtureDetail(gid));
  return invoke<GalleryDetail>("get_gallery_detail", { site, gid, token, index });
}

export function getPreviewSet(
  site: number,
  gid: number,
  token: string,
  index: number,
): Promise<PreviewItem[]> {
  if (isMock()) return Promise.resolve(fixturePreviews(index));
  return invoke<PreviewItem[]>("get_preview_set", { site, gid, token, index });
}

export function getOnlinePage(
  site: number,
  gid: number,
  token: string,
  index: number,
  pToken: string,
): Promise<{ gid: number; index: number; imageUrl: string; originImageUrl?: string | null; showKey?: string | null }> {
  if (isMock()) return Promise.resolve(fixtureOnlinePage(gid, index));
  return invoke("get_online_page", { site, gid, token, index, pToken });
}

export function resolveShowpage(
  site: number,
  gids: number[],
  page: number,
): Promise<GalleryToken> {
  if (isMock()) return Promise.resolve(fixtureToken(gids[0]));
  return invoke<GalleryToken>("resolve_showpage", { site, gids, page });
}

export function saveReadingProgress(gid: number, page: number): Promise<void> {
  if (isMock()) return Promise.resolve();
  return invoke("save_reading_progress", { gid, page });
}

export function getReadingProgress(gid: number): Promise<number> {
  if (isMock()) return Promise.resolve(0);
  return invoke<number>("get_reading_progress", { gid });
}

// ----- downloads -----
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DownloadItem, DownloadProgress } from "./types";
import { fixtureDownloads } from "./fixture";

export function downloadStart(
  gid: number,
  token: string,
  title: string,
  label: string,
  total: number,
  url: string,
): Promise<void> {
  if (isMock()) return mockToggle(gid, title, "downloading");
  return invoke("download_start", { gid, token, title, label, total, url });
}

export function downloadStop(gid: number): Promise<void> {
  if (isMock()) return mockToggle(gid, "", "wait");
  return invoke<null>("download_stop", { gid }).then(() => undefined);
}

export function downloadDelete(gid: number, erase = true): Promise<void> {
  if (isMock()) return mockDelete(gid);
  return invoke<null>("download_delete", { gid, erase }).then(() => undefined);
}

export function downloadRelabel(gid: number, label: string): Promise<void> {
  if (isMock()) return mockRelabel(gid, label);
  return invoke<null>("download_relabel", { gid, label }).then(() => undefined);
}

export function downloadList(): Promise<DownloadItem[]> {
  if (isMock()) return Promise.resolve(fixtureDownloads());
  return invoke<DownloadItem[]>("download_list");
}

/** A full download list, or a sparse `{ gid }` signal that a gallery changed. */
export type DownloadChanged = DownloadItem[] | { gid: number };

/** Subscribes to download changes: a full list, or a `{ gid }` change signal. */
export function onDownloadChanged(cb: (payload: DownloadChanged) => void): Promise<UnlistenFn> {
  if (isMock()) return Promise.resolve(() => undefined);
  return listen<DownloadChanged>("download-changed", (e) => cb(e.payload));
}

/** Subscribes to per-page download progress. */
export function onDownloadProgress(cb: (p: DownloadProgress) => void): Promise<UnlistenFn> {
  if (isMock()) return Promise.resolve(() => undefined);
  return listen<DownloadProgress>("download-progress", (e) => cb(e.payload));
}

/** Imports an EhTagDatabase binary so gallery tags get translated. */
export function importTagTranslation(path: string): Promise<{ count: number; path: string }> {
  if (isMock()) return Promise.resolve({ count: 0, path });
  return invoke("import_tag_translation", { path });
}

// Mock-mode helpers (mutate an in-memory mirror so the Downloads page stays usable).
let mockStore: DownloadItem[] | null = null;

function mockLoad(): DownloadItem[] {
  if (!mockStore) mockStore = fixtureDownloads();
  return mockStore;
}

async function mockToggle(gid: number, title: string, state: string): Promise<void> {
  const items = mockLoad();
  const existing = items.find((i) => i.gid === gid);
  if (existing) {
    existing.state = state;
  } else {
    items.unshift({
      gid,
      token: `token${gid}`,
      title: title || `示例画廊 ${gid - 999}`,
      label: "",
      state,
      total: 24,
      complete: 0,
      dir: "",
      url: "",
    });
  }
}

async function mockDelete(gid: number): Promise<void> {
  mockLoad();
  mockStore = (mockStore ?? []).filter((i) => i.gid !== gid);
}

async function mockRelabel(gid: number, label: string): Promise<void> {
  const it = mockLoad().find((i) => i.gid === gid);
  if (it) it.label = label;
}

// ----- settings -----
import type { AppSettings, CookiePair } from "./types";

export function settingsGet(): Promise<AppSettings> {
  if (isMock()) return Promise.resolve(fixtureSettings());
  return invoke<AppSettings>("settings_get");
}

export function settingsSet(key: string, value: unknown): Promise<AppSettings> {
  if (isMock()) return Promise.resolve(fixtureSettings());
  return invoke<AppSettings>("settings_set", { key, value });
}

export function cookieSave(site: number, pairs: CookiePair[]): Promise<AppSettings> {
  if (isMock()) return Promise.resolve(fixtureSettings());
  return invoke<AppSettings>("cookie_save", { site, pairs });
}

export function cookieVerify(site: number): Promise<{ ok: boolean; message: string }> {
  if (isMock()) return Promise.resolve({ ok: true, message: "（mock 环境，跳过真实验证）" });
  return invoke<{ ok: boolean; message: string }>("cookie_verify", { site });
}

export function exportData(): Promise<string> {
  if (isMock()) return Promise.resolve(JSON.stringify({ version: 1, mock: true }, null, 2));
  return invoke<object>("export_data").then((v) => JSON.stringify(v, null, 2));
}

export function importData(json: string): Promise<{ importedDownloads: number; importedProgress: number }> {
  if (isMock()) return Promise.resolve({ importedDownloads: 0, importedProgress: 0 });
  const payload = JSON.parse(json);
  return invoke<{ imported_downloads: number; imported_progress: number }>("import_data", { payload }).then(
    (r) => ({ importedDownloads: r.imported_downloads, importedProgress: r.imported_progress }),
  );
}

export function fixtureSettings(): AppSettings {
  return {
    site: 0,
    config: {},
    session_cookies: [],
    tag_translation_enabled: false,
  tag_translation_file: null,
  thumb_resolution: 0,
    reading_direction: "ltr",
    zoom_mode: "fit_width",
    reading_start_position: "resume",
    download_dir: null,
    download_threads: 3,
    download_always_original: false,
    download_list_page_size: 12,
    download_interval_secs: 5,
    proxy_type: 0,
    proxy_url: null,
    hosts_override: "",
    timeout_secs: 30,
    max_retries: 3,
    image_cache_size_mb: 100,
    custom_host: null,
    doh_url: "",
    use_builtin_hosts: true,
  };
}
