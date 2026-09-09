import type { ListNav } from "../types";
import "./pagination.css";

interface Props {
  nav: ListNav;
  page: number;
  /** Rebuild a list URL from the current filter params at a page number (legacy/fallback). */
  onPage: (page: number) => void;
  /** Follow a server-provided cursor href (Next/Prev/First/Last). */
  onHref: (href: string, page: number) => void;
}

export function Pagination({ nav, page, onPage, onHref }: Props) {
  const total = nav.pages > 0 ? nav.pages : 0;
  const showTotal = total > 0;
  const cap = showTotal ? Math.min(page, total - 1) : page;

  const jump = (target: number) => {
    let t = target;
    if (showTotal) t = Math.max(0, Math.min(total - 1, t));
    if (showTotal) onPage(t);
    else if (nav.last) onHref(nav.last, t);
  };

  return (
    <div className="pagination">
      <span className="pagination-count">{nav.resultCount || ""}</span>
      <div className="pagination-buttons">
        <button disabled={!nav.first} onClick={() => nav.first && onHref(nav.first, 0)} title="首页">«</button>
        <button disabled={!nav.prev} onClick={() => nav.prev && onHref(nav.prev, Math.max(0, page - 1))} title="上一页">‹</button>
        <span className="pagination-pos">{showTotal ? `${cap + 1} / ${total}` : `第 ${cap + 1} 页`}</span>
        <button disabled={!nav.next} onClick={() => nav.next && onHref(nav.next, page + 1)} title="下一页">›</button>
        <button
          disabled={!nav.last && !showTotal}
          onClick={() => {
            if (nav.last) onHref(nav.last, showTotal ? total - 1 : page);
            else if (showTotal) onPage(total - 1);
          }}
          title="末页"
        >»</button>
      </div>
      <span className="pagination-jump">
        跳至
        <PageJump showTotal={showTotal} total={total} onJump={jump} />
      </span>
    </div>
  );
}

function PageJump({ showTotal, total, onJump }: {
  showTotal: boolean;
  total: number;
  onJump: (target: number) => void;
}) {
  const submit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const v = Number(new FormData(e.currentTarget).get("page"));
    let p = Math.max(0, (isNaN(v) ? 1 : v) - 1);
    if (showTotal) p = Math.min(total - 1, p);
    onJump(p);
  };
  return (
    <form onSubmit={submit} className="pagination-jump-form">
      <input name="page" type="number" min={1} max={showTotal ? total : undefined} defaultValue={1} size={4} />
    </form>
  );
}
