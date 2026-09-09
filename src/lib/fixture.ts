// Fixture data used only when `localStorage.ehMock === "1"`, so the whole
// detail → preview → reader flow can be exercised without a live E-Hentai
// connection (which is Cloudflare-gated on the dev network).

import type { GalleryListResult } from "../types";
import type { GalleryDetail, PreviewItem, GalleryToken } from "./types";

export const isMock = () =>
  typeof localStorage !== "undefined" && localStorage.getItem("ehMock") === "1";

function placeholder(label: string, w: number, h: number): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}"><rect width="100%" height="100%" fill="#2a2e35"/><text x="50%" y="50%" fill="#8895a7" font-family="sans-serif" font-size="16" text-anchor="middle">${label}</text></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

export const fixtureList: GalleryListResult = {
  items: Array.from({ length: 24 }, (_, i) => ({
    gid: 1000 + i,
    token: `token${i}`,
    title: `示例画廊 ${i + 1}（fixture）`,
    titleJpn: `サンプル ${i + 1}`,
    thumb: placeholder(`封面${i + 1}`, 250, 350),
    category: ["doujinshi", "manga", "western"][i % 3],
    posted: `20${20 + (i % 4)}-0${(i % 9) + 1}-1${i % 9} 00:00`,
    uploader: `uploader_${i % 5}`,
    rating: 1 + (i % 5) * 0.9,
    pages: 20 + i * 3,
    thumbWidth: 250,
    thumbHeight: 350,
    tags: ["language:english", "artist:fake"],
    url: `fixture://gallery/${1000 + i}`,
  })),
  nav: {
    pages: 4,
    nextPage: 1,
    resultCount: "Found 96 results",
    first: "fixture://home",
    prev: "fixture://home?prev=1",
    next: "fixture://home?next=2",
    last: "fixture://home?prev=1",
  },
  requestedUrl: "fixture://home",
};

export function fixtureDetail(gid: number): GalleryDetail {
  return {
    gid,
    token: `token${gid}`,
    title: `示例画廊 ${gid - 999}`,
    titleJpn: `サンプル ${gid - 999}`,
    thumb: placeholder(`封面${gid}`, 250, 350),
    category: "doujinshi",
    uploader: "uploader_fixture",
    posted: "2023-05-10 10:00",
    language: "English",
    size: "23 MB",
    pages: 64,
    favoriteCount: 128,
    rating: 4.3,
    ratingCount: 320,
    torrentCount: 5,
    torrentUrl: "",
    archiveUrl: "",
    isFavorited: false,
    tags: [
      { name: "Language:", tags: ["english", "translated"] },
      { name: "Artist:", tags: ["anime", "kedo"] },
      { name: "Characters:", tags: ["hina", "yuno"] },
    ],
    comments: [
      { id: 1, user: "reader_one", score: 3, time: "2023-05-11", comment: "Great gallery, thanks for sharing." },
    ],
    previewPages: 6,
  };
}

export function fixturePreviews(page: number): PreviewItem[] {
  return Array.from({ length: 20 }, (_, k) => {
    const idx = page * 20 + k;
    return {
      index: idx,
      imageUrl: placeholder(`p${idx + 1}`, 250, 350),
      pageUrl: `fixture://${idx}`,
      width: 250,
      height: 350,
    };
  });
}

export function fixtureOnlinePage(gid: number, index: number) {
  return {
    gid,
    index,
    imageUrl: placeholder(`第 ${index + 1} 页`, 800, 1100),
    originImageUrl: null,
    showKey: null,
  };
}

export function fixtureToken(gid: number): GalleryToken {
  return { gid, token: `token${gid}` };
}


// Mock download list for offline (ehMock=1) development of the Downloads page.
import type { DownloadItem } from "./types";

export function fixtureDownloads(): DownloadItem[] {
  return [
    {
      gid: 1001,
      token: "token0",
      title: "示例画廊 2（fixture）",
      label: "",
      state: "downloading",
      total: 24,
      complete: 7,
      dir: "C:\\Downloads\\EhViewer\\1001-示例画廊 2",
      url: "fixture://gallery/1001",
    },
    {
      gid: 1002,
      token: "token1",
      title: "示例画廊 3（fixture）",
      label: "",
      state: "wait",
      total: 24,
      complete: 0,
      dir: "",
      url: "fixture://gallery/1002",
    },
    {
      gid: 1003,
      token: "token2",
      title: "示例画廊 4（fixture）",
      label: "收藏",
      state: "finished",
      total: 16,
      complete: 16,
      dir: "C:\\Downloads\\EhViewer\\1003-示例画廊 4",
      url: "fixture://gallery/1003",
    },
    {
      gid: 1004,
      token: "token3",
      title: "示例画廊 5（fixture）",
      label: "收藏",
      state: "failed",
      total: 40,
      complete: 21,
      dir: "C:\\Downloads\\EhViewer\\1004-示例画廊 5",
      url: "fixture://gallery/1004",
    },
  ];
}
