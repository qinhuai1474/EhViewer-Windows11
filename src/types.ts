export type PageRoute = "home" | "popular" | "downloads" | "settings";

export interface GalleryItem {
  gid: number;
  token: string;
  title: string;
  titleJpn?: string;
  thumb?: string;
  category: string;
  posted: string;
  uploader: string;
  rating: number;
  pages: number;
  thumbWidth: number;
  thumbHeight: number;
  tags: string[];
  url: string;
}

export interface ListNav {
  pages: number;
  nextPage: number;
  resultCount: string;
  first?: string | null;
  prev?: string | null;
  next?: string | null;
  last?: string | null;
}

export interface GalleryListResult {
  items: GalleryItem[];
  nav: ListNav;
  requestedUrl?: string | null;
}

export interface ListUrlParams {
  mode: number;
  site: number;
  keyword?: string | null;
  category: number;
  page: number;
  advance: number;
  minRating: number;
  pageFrom: number;
  pageTo: number;
}

// ListUrlBuilder modes (mirrors Rust).
export const MODE_NORMAL = 0;
export const MODE_UPLOADER = 1;
export const MODE_TAG = 2;
export const MODE_WHATS_HOT = 3;
export const MODE_FILTER = 6;

// AdvanceSearchTable flags.
export const AS_SNAME = 0x1;
export const AS_STAGS = 0x2;
export const AS_SDESC = 0x4;
