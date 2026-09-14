import type { ListNav } from "../types";
import "./pagination.css";

interface Props {
  nav: ListNav;
  page: number;
  /** Rebuild a list URL at a page number (legacy `/page=` mode; used for 末页). */
  onPage: (page: number) => void;
  /** Follow a server-provided cursor href (Next/Prev/First/Last). */
  onHref: (href: string, page: number) => void;
}

// E-Hentai gallery list default page size. Used to derive the last page number
// from the "Found N results" count when the server no longer reports page count.
const PAGE_SIZE = 25;

export function Pagination({ nav, page, onPage, onHref }: Props) {
  const knownTotal = nav.pages > 0;

  // Cursor-paginated mode exposes no total page count, so estimate the last page
  // from the result count (`Found N results`) and the fixed per-page size.
  let lastPages = knownTotal ? nav.pages : 0;
  let approximate = false;
  if (!knownTotal) {
    const est = computeLastPages(nav.resultCount);
    if (est) {
      lastPages = est.pages;
      approximate = est.approx;
    }
  }

  const current = knownTotal
    ? Math.min(Math.max(0, page), nav.pages - 1) + 1
    : page + 1;
  const hasTotal = lastPages > 0;

  return (
    <div className="pagination">
      <span className="pagination-count">{nav.resultCount || ""}</span>
      <div className="pagination-buttons">
        <button disabled={!nav.first} onClick={() => nav.first && onHref(nav.first, 0)} title="首页">«</button>
        <button disabled={!nav.prev} onClick={() => nav.prev && onHref(nav.prev, Math.max(0, page - 1))} title="上一页">‹</button>
        <span className="pagination-pos">
          {hasTotal
            ? `第 ${current} / ${approximate ? "约 " : ""}${lastPages} 页`
            : `第 ${current} 页`}
        </span>
        <button disabled={!nav.next} onClick={() => nav.next && onHref(nav.next, page + 1)} title="下一页">›</button>
        <button
          disabled={!nav.last && !knownTotal}
          onClick={() => {
            if (knownTotal) {
              onPage(lastPages - 1);
            } else if (nav.last) {
              // Land on the last page via the server cursor, and sync the on-screen
              // page number to the computed last page (N) instead of leaving it at 1.
              onHref(nav.last, lastPages > 0 ? lastPages - 1 : page);
            }
          }}
          title="末页"
        >»</button>
      </div>
    </div>
  );
}

/** Parse `countText` (e.g. "Found 4,123 results" / "4,123" / "1,000+") into a page
 *  estimate and whether the count is approximate ("about" / "+"). */
function computeLastPages(countText: string): { pages: number; approx: boolean } | null {
  if (!countText) return null;
  const plain = countText.replace(/,/g, "");
  const m = plain.match(/(\d+)/);
  if (!m) return null;
  const count = Number(m[1]);
  if (!Number.isFinite(count) || count <= 0) return null;
  const approx = /\babout\b|\+|~/.test(countText);
  return { pages: Math.max(1, Math.ceil(count / PAGE_SIZE)), approx };
}
