import { useCallback, useEffect, useState } from "react";
import type { GalleryItem, GalleryListResult } from "../types";
import { AS_SNAME, AS_STAGS, MODE_NORMAL, MODE_WHATS_HOT } from "../types";
import { buildListUrl, getGalleryList } from "../lib/api";
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

interface Props {
  showSearch: boolean;
  onOpen?: (item: GalleryItem) => void;
}

export function GalleryBrowser({ showSearch, onOpen }: Props) {
  const [keyword, setKeyword] = useState("");
  const [excluded, setExcluded] = useState<Set<number>>(new Set());
  const [minRating, setMinRating] = useState(0);
  const [pageFrom, setPageFrom] = useState("");
  const [pageTo, setPageTo] = useState("");
  const [page, setPage] = useState(0);
  const [data, setData] = useState<GalleryListResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const advanceFlags = useCallback(() => AS_SNAME | AS_STAGS, []);

  const categoryMask = useCallback(
    () => [...excluded].reduce((a, b) => a + b, 0),
    [excluded],
  );

  const load = useCallback(
    async (targetPage: number) => {
      setLoading(true);
      setError(null);
      try {
        const params = {
          mode: showSearch ? MODE_NORMAL : MODE_WHATS_HOT,
          site: 0,
          keyword: keyword.trim() || null,
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

  // Initial + reactive load.
  useEffect(() => {
    load(0);
  }, [load]);

  const submitSearch = (e: React.FormEvent) => {
    e.preventDefault();
    load(0);
  };
  const toggleExcluded = (bit: number) => {
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
          <input
            value={keyword}
            onChange={(e) => setKeyword(e.currentTarget.value)}
            placeholder="搜索画廊（标题/标签）…"
            className="search-input"
          />
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
                <select value={minRating} onChange={(e) => setMinRating(Number(e.currentTarget.value))}>
                  <option value={0}>不限</option>
                  {[2, 3, 4, 5].map((r) => <option key={r} value={r}>{r}.0</option>)}
                </select>
              </label>
              <label>页数
                <input type="number" value={pageFrom} onChange={(e) => setPageFrom(e.currentTarget.value)} placeholder="≥" className="num" />
                —
                <input type="number" value={pageTo} onChange={(e) => setPageTo(e.currentTarget.value)} placeholder="≤" className="num" />
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
          <GalleryGrid items={data.items} onOpen={(it) => onOpen?.(it)} />
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
