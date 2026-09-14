import { useCallback, useEffect, useRef, useState } from "react";
import type { GalleryItem, GalleryListResult } from "../types";
import { AS_SNAME, AS_STAGS, MODE_NORMAL, MODE_WHATS_HOT } from "../types";
import { buildListUrl, downloadList, getGalleryList, onDownloadChanged } from "../lib/api";
import { GalleryGrid } from "./GalleryGrid";
import { Pagination } from "./Pagination";
import "./browser.css";

const CATEGORIES = [
  { label: "Misc", bit: 0x1 },
  { label: "Doujinshi", bit: 0x2 },
  { label: "Manga", bit: 0x4 },
  { label: "Artist CG", bit: 0x8 },
  { label: "Game CG", bit: 0x10 },
  { label: "Image Set", bit: 0x20 },
  { label: "Cosplay", bit: 0x40 },
  { label: "Asian Porn", bit: 0x80 },
  { label: "Non-H", bit: 0x100 },
  { label: "Western", bit: 0x200 },
] as const;

const HISTORY_KEY = "ehviewer.search.history";
const HISTORY_MAX = 20;

interface Props {
  showSearch: boolean;
  onOpen?: (item: GalleryItem) => void;
}

// The gallery view replaces this page while a gallery is open, unmounting the
// browser. Keep the last search/browse state per mode so returning from a
// detail page (or switching sections) restores the original interface instead
// of clearing the search bar and reloading from page 0.
interface SavedState {
  keyword: string;
  excluded: number[];
  minRating: number;
  pageFrom: string;
  pageTo: string;
  page: number;
  data: GalleryListResult | null;
  scrollTop: number;
}
const SAVED: Partial<Record<"search" | "popular", SavedState>> = {};

function loadHistory(): string[] {
  try {
    const raw = localStorage.getItem(HISTORY_KEY);
    if (!raw) return [];
    const arr: unknown = JSON.parse(raw);
    return Array.isArray(arr)
      ? arr.filter((x): x is string => typeof x === "string" && x.trim() !== "")
      : [];
  } catch {
    return [];
  }
}

function saveHistory(history: string[]) {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(history));
  } catch {
    /* storage unavailable or full — ignore */
  }
}

function setContentY(y: number) {
  const el = document.querySelector<HTMLElement>(".content");
  if (el) el.scrollTop = y;
}

export function GalleryBrowser({ showSearch, onOpen }: Props) {
  const cacheKey = showSearch ? "search" : "popular";
  const saved = SAVED[cacheKey];
  // True while the browser is in "restored on remount" mode: the reactive load
  // effect must NOT re-pull page 0 (that would wipe the restored results and
  // scroll position). It is cleared after the mount settles and again on any
  // user search interaction, so live-typing / filters still work. Being a ref
  // (not consumed inside the effect) makes this safe under React StrictMode's
  // dev double-mount.
  const restoreModeRef = useRef(saved !== undefined);
  // Latest `.content` scrollTop, tracked live so it survives unmount even
  // though React has already swapped the detail view in by cleanup time.
  const scrollRef = useRef(0);

  const [keyword, setKeyword] = useState(saved?.keyword ?? "");
  const [excluded, setExcluded] = useState<Set<number>>(
    () => new Set(saved?.excluded ?? []),
  );
  const [minRating, setMinRating] = useState(saved?.minRating ?? 0);
  const [pageFrom, setPageFrom] = useState(saved?.pageFrom ?? "");
  const [pageTo, setPageTo] = useState(saved?.pageTo ?? "");
  const [page, setPage] = useState(saved?.page ?? 0);
  const [data, setData] = useState<GalleryListResult | null>(saved?.data ?? null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [history, setHistory] = useState<string[]>(loadHistory);
  const [historyOpen, setHistoryOpen] = useState(false);

  // Gallery gids that are already in the download queue (authoritative source
  // so the quick-download icon stays visible once a gallery is queued).
  const [downloaded, setDownloaded] = useState<Set<number>>(new Set());

  const advanceFlags = useCallback(() => AS_SNAME | AS_STAGS, []);

  const categoryMask = useCallback(
    () => [...excluded].reduce((a, b) => a + b, 0),
    [excluded],
  );

  const load = useCallback(
    async (targetPage: number, kwOverride?: string) => {
      setLoading(true);
      setError(null);
      try {
        const params = {
          mode: showSearch ? MODE_NORMAL : MODE_WHATS_HOT,
          site: 0,
          keyword: (kwOverride ?? keyword).trim() || null,
          category: categoryMask(),
          page: targetPage,
          advance: advanceFlags(),
          minRating: minRating,
          pageFrom: safeInt(pageFrom),
          pageTo: safeInt(pageTo),
        };
        const url = await buildListUrl(params);
        const result = await getGalleryList(url);
        setData(result);
        setPage(targetPage);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    },
    [showSearch, keyword, categoryMask, advanceFlags, minRating, pageFrom, pageTo],
  );

  // Follow a server-provided cursor href (current E-Hentai ?next=/?prev= scheme),
  // preserving whatever filters the server re-encoded into the href.
  const gotoHref = useCallback(
    async (href: string, targetPage: number) => {
      setLoading(true);
      setError(null);
      try {
        const result = await getGalleryList(href);
        setData(result);
        setPage(targetPage);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  // Initial + reactive load. While in restore mode (came back from a detail page
  // / another section) keep the restored results instead of forcing page 0.
  useEffect(() => {
    if (restoreModeRef.current) return;
    load(0);
  }, [load]);

  // End restore mode once the restored screen has settled, re-enabling the
  // reactive live search. StrictMode-safe: the timed cleanup is re-armed.
  useEffect(() => {
    const id = setTimeout(() => {
      restoreModeRef.current = false;
    }, 0);
    return () => clearTimeout(id);
  }, []);

  // Track the live scroll position of the list so it can be restored on return.
  useEffect(() => {
    const el = document.querySelector<HTMLElement>(".content");
    if (!el) return;
    const onScroll = () => {
      scrollRef.current = el.scrollTop;
    };
    onScroll();
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  // Returning from a detail page: put the scroll position back where it was.
  useEffect(() => {
    if (!saved) return;
    const y = SAVED[cacheKey]?.scrollTop ?? 0;
    if (y > 0) requestAnimationFrame(() => setContentY(y));
    // Run once per mount; grid height is stable (fixed card aspect ratios).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Seed + keep the set of queued gallery gids.
  useEffect(() => {
    let alive = true;
    let un: (() => void) | undefined;
    downloadList()
      .then((list) => alive && setDownloaded(new Set(list.map((i) => i.gid))))
      .catch(() => {});
    onDownloadChanged((payload) => {
      if (Array.isArray(payload)) {
        if (alive) setDownloaded(new Set(payload.map((i) => i.gid)));
      } else {
        downloadList()
          .then((list) => alive && setDownloaded(new Set(list.map((i) => i.gid))))
          .catch(() => {});
      }
    }).then((fn) => (un = fn));
    return () => {
      alive = false;
      un?.();
    };
  }, []);

  // Persist the latest browse state (incl. scroll) so it survives unmount.
  useEffect(() => {
    return () => {
      SAVED[cacheKey] = {
        keyword,
        excluded: [...excluded],
        minRating,
        pageFrom,
        pageTo,
        page,
        data,
        scrollTop: scrollRef.current,
      };
    };
  }, [cacheKey, keyword, excluded, minRating, pageFrom, pageTo, page, data]);

  const addHistory = (kw: string) => {
    const k = kw.trim();
    if (!k) return;
    setHistory((prev) => {
      const next = [k, ...prev.filter((x) => x !== k)].slice(0, HISTORY_MAX);
      saveHistory(next);
      return next;
    });
  };

  const clearHistory = () => {
    setHistory([]);
    saveHistory([]);
  };

  const applyHistory = (k: string) => {
    restoreModeRef.current = false;
    setHistoryOpen(false);
    setKeyword(k);
    addHistory(k);
    load(0, k);
  };

  const submitSearch = (e: React.FormEvent) => {
    e.preventDefault();
    restoreModeRef.current = false;
    addHistory(keyword);
    load(0);
  };
  const toggleExcluded = (bit: number) => {
    restoreModeRef.current = false;
    setExcluded((prev) => {
      const next = new Set(prev);
      if (next.has(bit)) next.delete(bit);
      else next.add(bit);
      return next;
    });
  };

  return (
    <div className="browser">
      {showSearch ? (
        <form className="search-bar" onSubmit={submitSearch}>
          <div className="search-field">
            <input
              value={keyword}
              onChange={(e) => {
                restoreModeRef.current = false;
                setKeyword(e.currentTarget.value);
              }}
              onFocus={() => setHistoryOpen(true)}
              onBlur={() => setTimeout(() => setHistoryOpen(false), 150)}
              placeholder="搜索画廊（标题/标签）…"
              className="search-input"
            />
            {historyOpen && history.length > 0 && (
              <div
                className="search-history"
                onMouseDown={(e) => e.preventDefault()}
              >
                <div className="history-head">
                  <span>搜索记录</span>
                  <button
                    type="button"
                    className="history-clear"
                    onClick={clearHistory}
                  >
                    清空
                  </button>
                </div>
                <ul>
                  {history.map((h) => (
                    <li key={h}>
                      <button
                        type="button"
                        className="history-item"
                        onClick={() => applyHistory(h)}
                      >
                        {h}
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
          <button type="submit" className="search-go">搜索</button>
          <details className="filter-panel">
            <summary>高级筛选</summary>
            <div className="filter-row">
              {CATEGORIES.map((c) => (
                <label key={c.bit} className="filter-check">
                  <input
                    type="checkbox"
                    checked={!excluded.has(c.bit)}
                    onChange={() => toggleExcluded(c.bit)}
                  />
                  {c.label}
                </label>
              ))}
            </div>
            <div className="filter-row">
              <label>最低评分
                <select
                  value={minRating}
                  onChange={(e) => {
                    restoreModeRef.current = false;
                    setMinRating(Number(e.currentTarget.value));
                  }}
                >
                  <option value={0}>不限</option>
                  {[2, 3, 4, 5].map((r) => <option key={r} value={r}>{r}.0</option>)}
                </select>
              </label>
              <label>页数
                <input type="number" value={pageFrom} onChange={(e) => { restoreModeRef.current = false; setPageFrom(e.currentTarget.value); }} placeholder="≥" className="num" />
                —
                <input type="number" value={pageTo} onChange={(e) => { restoreModeRef.current = false; setPageTo(e.currentTarget.value); }} placeholder="≤" className="num" />
              </label>
            </div>
            <div className="filter-row">
              <button type="submit">应用</button>
            </div>
          </details>
        </form>
      ) : (
        <div className="browser-title">热门画廊</div>
      )}

      {error ? (
        <div className="state">
          <p>{error}</p>
          <button onClick={() => load(page)}>重试</button>
        </div>
      ) : loading ? (
        <div className="state">加载中…</div>
      ) : !data?.items?.length ? (
        <div className="state">没有结果</div>
      ) : (
        <>
          <GalleryGrid items={data.items} onOpen={(it) => onOpen?.(it)} downloadedGids={downloaded} />
          <Pagination nav={data.nav} page={page} onPage={load} onHref={gotoHref} />
        </>
      )}
    </div>
  );
}

function safeInt(s: string): number {
  const n = Number(s);
  return Number.isFinite(n) && n > 0 ? n : -1;
}
