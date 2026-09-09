import { useCallback, useEffect, useMemo, useState } from "react";
import {
  downloadDelete,
  downloadList,
  downloadRelabel,
  downloadStart,
  downloadStop,
  onDownloadChanged,
  onDownloadProgress,
} from "../lib/api";
import type { DownloadItem } from "../lib/types";
import "./downloads.css";

const PAGE_SIZE = 12;
const DEFAULT_LABEL = "默认";

const STATE_TEXT: Record<string, string> = {
  none: "未开始",
  wait: "等待",
  downloading: "下载中",
  finished: "已完成",
  failed: "失败",
};

export function DownloadsPage() {
  const [items, setItems] = useState<DownloadItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [labelFilter, setLabelFilter] = useState<string>(DEFAULT_LABEL);
  const [keyword, setKeyword] = useState("");
  const [page, setPage] = useState(0);
  const [busy, setBusy] = useState<number | null>(null);

  const refresh = useCallback(() => {
    downloadList()
      .then((list) => {
        setItems(list);
        setError(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    let un: (() => void) | undefined;
    let unP: (() => void) | undefined;
    onDownloadChanged((payload) => {
      // Full-list refresh on real state changes; a sparse `{ gid }` signal just
      // means "something changed", so re-pull the list.
      if (Array.isArray(payload)) {
        setItems(payload);
      } else {
        refresh();
      }
      setError(null);
    }).then((u) => (un = u));
    onDownloadProgress((p) => {
      // Incremental per-page update without re-pulling the whole list.
      setItems((prev) =>
        prev.map((it) =>
          it.gid === p.gid ? { ...it, complete: p.complete, total: p.total } : it,
        ),
      );
    }).then((u) => (unP = u));
    return () => {
      un?.();
      unP?.();
    };
  }, [refresh]);

  const labels = useMemo(() => {
    const set = new Set(items.map((i) => i.label || DEFAULT_LABEL));
    return Array.from(set).sort();
  }, [items]);

  useEffect(() => {
    if (!labels.includes(labelFilter)) setLabelFilter(DEFAULT_LABEL);
  }, [labels, labelFilter]);

  const filtered = useMemo(() => {
    const kw = keyword.trim().toLowerCase();
    return items.filter((i) => {
      const inLabel = (i.label || DEFAULT_LABEL) === labelFilter;
      if (!inLabel) return false;
      if (!kw) return true;
      return (
        i.title.toLowerCase().includes(kw) ||
        i.label.toLowerCase().includes(kw) ||
        String(i.gid).includes(kw)
      );
    });
  }, [items, labelFilter, keyword]);

  const pageCount = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const safePage = Math.min(page, pageCount - 1);
  const visible = useMemo(
    () => filtered.slice(safePage * PAGE_SIZE, safePage * PAGE_SIZE + PAGE_SIZE),
    [filtered, safePage],
  );

  const run = useCallback((gid: number, p: Promise<unknown>) => {
    setBusy(gid);
    p.catch((e) => setError(String(e))).finally(() => setBusy(null));
  }, []);

  const stats = useMemo(() => {
    const total = items.length;
    const completed = items.filter((i) => i.state === "finished").length;
    const active = items.filter((i) => i.state === "downloading").length;
    const waiting = items.filter((i) => i.state === "wait").length;
    return { total, completed, active, waiting };
  }, [items]);

  return (
    <section className="page downloads">
      <h1 className="page-title">下载</h1>

      <div className="dl-stats">
        <span>共 {stats.total}</span>
        <span>已完成 {stats.completed}</span>
        <span>下载中 {stats.active}</span>
        <span>等待 {stats.waiting}</span>
      </div>

      <div className="dl-toolbar">
        <div className="dl-labels">
          <button
            className={`dl-label ${labelFilter === DEFAULT_LABEL && !keyword ? "active" : ""}`}
            onClick={() => setLabelFilter(DEFAULT_LABEL)}
          >
            {DEFAULT_LABEL}
            <span className="dl-count">
              {items.filter((i) => (i.label || DEFAULT_LABEL) === DEFAULT_LABEL).length}
            </span>
          </button>
          {labels
            .filter((l) => l !== DEFAULT_LABEL)
            .map((l) => (
              <button
                key={l}
                className={`dl-label ${labelFilter === l ? "active" : ""}`}
                onClick={() => setLabelFilter(l)}
              >
                {l}
                <span className="dl-count">
                  {items.filter((i) => (i.label || DEFAULT_LABEL) === l).length}
                </span>
              </button>
            ))}
        </div>
        <input
          className="dl-search"
          placeholder="搜索标题 / 标签 / gid…"
          value={keyword}
          onChange={(e) => {
            setKeyword(e.target.value);
            setPage(0);
          }}
        />
      </div>

      {error && (
        <div className="state">
          <p>{error}</p>
          <button onClick={refresh}>重试</button>
        </div>
      )}
      {!error && loading && <div className="state">加载下载列表…</div>}
      {!error && !loading && filtered.length === 0 && (
        <div className="state">
          <p>{keyword || labelFilter !== DEFAULT_LABEL ? "没有匹配的下载" : "还没有下载。可在画廊详情页点击「下载」加入队列。"}</p>
        </div>
      )}

      <div className="dl-list">
        {visible.map((d) => (
          <DownloadRow
            key={d.gid}
            item={d}
            busy={busy === d.gid}
            onStart={() =>
              run(d.gid, downloadStart(d.gid, d.token, d.title, d.label, d.total, d.url))
            }
            onStop={() => run(d.gid, downloadStop(d.gid))}
            onDelete={(erase) => run(d.gid, downloadDelete(d.gid, erase))}
            onRelabel={(label) => run(d.gid, downloadRelabel(d.gid, label))}
          />
        ))}
      </div>

      {pageCount > 1 && (
        <div className="pagination">
          <div className="pagination-buttons">
            <button disabled={safePage <= 0} onClick={() => setPage(0)}>«</button>
            <button disabled={safePage <= 0} onClick={() => setPage(safePage - 1)}>‹</button>
            <span className="pagination-pos">{safePage + 1} / {pageCount}</span>
            <button disabled={safePage >= pageCount - 1} onClick={() => setPage(safePage + 1)}>›</button>
            <button disabled={safePage >= pageCount - 1} onClick={() => setPage(pageCount - 1)}>»</button>
          </div>
        </div>
      )}
    </section>
  );
}

function DownloadRow({
  item,
  busy,
  onStart,
  onStop,
  onDelete,
  onRelabel,
}: {
  item: DownloadItem;
  busy: boolean;
  onStart: () => void;
  onStop: () => void;
  onDelete: (erase: boolean) => void;
  onRelabel: (label: string) => void;
}) {
  const pct = item.total > 0 ? Math.round((item.complete / item.total) * 100) : 0;
  const active = item.state === "downloading";

  const relabel = () => {
    const next = window.prompt("设置新的标签（留空表示「默认」）", item.label || "");
    if (next !== null) onRelabel(next.trim());
  };

  const remove = () => {
    const erase = window.confirm(
      `删除「${item.title}」？\n\n「确定」同时删除已下载的文件，「取消」仅从列表移除。`,
    );
    onDelete(erase);
  };

  return (
    <div className="dl-row">
      <div className="dl-row-main">
        <div className="dl-row-title" title={item.title}>{item.title}</div>
        <div className={`dl-badge ${active ? "active" : item.state}`}>
          {STATE_TEXT[item.state] ?? item.state}
        </div>
      </div>
      <div className="dl-progress">
        <div className="dl-progress-bar">
          <div
            className={`dl-progress-fill ${active ? "active" : ""}`}
            style={{ width: `${pct}%` }}
          />
        </div>
        <span className="dl-progress-text">
          {item.complete} / {item.total} 页（{pct}%）
        </span>
      </div>
      <div className="dl-row-actions">
        {item.state === "failed" || item.state === "wait" || item.state === "none" ? (
          <button className="primary" onClick={onStart} disabled={busy}>继续</button>
        ) : active ? (
          <button onClick={onStop} disabled={busy}>暂停</button>
        ) : item.state === "finished" ? (
          <span className="dl-dir" title={item.dir}>已完成</span>
        ) : null}
        <button onClick={relabel} disabled={busy}>改标签</button>
        <button onClick={remove} disabled={busy}>删除</button>
      </div>
    </div>
  );
}


