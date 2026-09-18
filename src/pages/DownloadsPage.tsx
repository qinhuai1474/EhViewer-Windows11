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
  const [busyAll, setBusyAll] = useState(false);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [relabelFor, setRelabelFor] = useState<number[] | null>(null);
  const [deleteFor, setDeleteFor] = useState<number[] | null>(null);

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
      if (Array.isArray(payload)) {
        setItems(payload);
      } else {
        refresh();
      }
      setError(null);
    }).then((u) => (un = u));
    onDownloadProgress((p) => {
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

  const runBatch = useCallback((ops: Array<() => Promise<unknown>>) => {
    if (!ops.length) return;
    setBusyAll(true);
    Promise.all(ops.map((fn) => Promise.resolve().then(fn)))
      .catch((e) => setError(String(e)))
      .finally(() => setBusyAll(false));
  }, []);

  const stats = useMemo(() => {
    const total = items.length;
    const completed = items.filter((i) => i.state === "finished").length;
    const active = items.filter((i) => i.state === "downloading").length;
    const waiting = items.filter((i) => i.state === "wait").length;
    return { total, completed, active, waiting };
  }, [items]);

  const selectedGids = useMemo(
    () => items.filter((i) => selected.has(i.gid)),
    [items, selected],
  );

  const toggle = (gid: number) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(gid)) next.delete(gid);
      else next.add(gid);
      return next;
    });

  const selectAll = () => {
    const next = new Set(selected);
    filtered.forEach((i) => next.add(i.gid));
    setSelected(next);
  };

  const clearSel = () => setSelected(new Set());

  const batchStart = () => {
    setError(null);
    runBatch(
      selectedGids
        .filter((i) => i.state === "wait" || i.state === "failed" || i.state === "none")
        .map((i) => () => downloadStart(i.gid, i.token, i.title, i.label, "", "", "", i.total, i.url)),
    );
  };
  const batchStop = () => {
    setError(null);
    runBatch(
      selectedGids
        .filter((i) => i.state === "downloading" || i.state === "wait")
        .map((i) => () => downloadStop(i.gid)),
    );
  };
  const batchDelete = () => {
    if (!selectedGids.length) return;
    setDeleteFor(selectedGids.map((i) => i.gid));
  };
  const confirmDelete = (erase: boolean) => {
    if (!deleteFor?.length) return;
    setError(null);
    runBatch(deleteFor.map((gid) => () => downloadDelete(gid, erase)));
    setDeleteFor(null);
    setSelected(new Set());
  };
  const batchRelabel = () => {
    if (selectedGids.length) setRelabelFor(selectedGids.map((i) => i.gid));
  };

  const applyRelabel = (gids: number[], label: string) => {
    runBatch(gids.map((g) => () => downloadRelabel(g, label)));
    setRelabelFor(null);
    setSelected(new Set());
  };

  const relabelCurrent =
    relabelFor && relabelFor.length === 1
      ? (items.find((i) => i.gid === relabelFor[0])?.label ?? "")
      : null;

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

      {selected.size > 0 && (
        <div className="dl-batchbar">
          <span className="dl-batch-count">已选 {selected.size} 项</span>
          <button disabled={busyAll} onClick={batchStart}>继续</button>
          <button disabled={busyAll} onClick={batchStop}>暂停</button>
          <button disabled={busyAll || !selectedGids.length} onClick={batchRelabel}>改标签</button>
          <button className="danger" disabled={busyAll || !selectedGids.length} onClick={batchDelete}>删除</button>
          <button className="ghost" disabled={busyAll} onClick={selectAll}>全选</button>
          <button className="ghost" disabled={busyAll} onClick={clearSel}>取消选择</button>
        </div>
      )}

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
            busy={busy === d.gid || busyAll}
            checked={selected.has(d.gid)}
            onToggle={() => toggle(d.gid)}
            onStart={() =>
              run(d.gid, downloadStart(d.gid, d.token, d.title, d.label, "", "", "", d.total, d.url))
            }
            onStop={() => run(d.gid, downloadStop(d.gid))}
            onDelete={() => setDeleteFor([d.gid])}
            onRelabel={() => setRelabelFor([d.gid])}
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

      <RelabelDialog
        open={relabelFor !== null}
        labels={[DEFAULT_LABEL, ...labels.filter((l) => l !== DEFAULT_LABEL)]}
        current={relabelCurrent}
        onSubmit={(label) => relabelFor && applyRelabel(relabelFor, label)}
        onClose={() => setRelabelFor(null)}
      />
      <DeleteDialog
        open={deleteFor !== null}
        count={deleteFor?.length ?? 0}
        onConfirm={confirmDelete}
        onClose={() => setDeleteFor(null)}
      />
    </section>
  );
}

function DownloadRow({
  item,
  busy,
  checked,
  onToggle,
  onStart,
  onStop,
  onDelete,
  onRelabel,
}: {
  item: DownloadItem;
  busy: boolean;
  checked: boolean;
  onToggle: () => void;
  onStart: () => void;
  onStop: () => void;
  onDelete: () => void;
  onRelabel: () => void;
}) {
  const pct = item.total > 0 ? Math.round((item.complete / item.total) * 100) : 0;
  const active = item.state === "downloading";

  return (
    <div className={`dl-row${checked ? " selected" : ""}`}>
      <label className="dl-check">
        <input type="checkbox" checked={checked} onChange={onToggle} />
      </label>
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
        <button onClick={onRelabel} disabled={busy}>改标签</button>
        <button onClick={onDelete} disabled={busy}>删除</button>
      </div>
    </div>
  );
}

function DeleteDialog({
  open,
  count,
  onConfirm,
  onClose,
}: {
  open: boolean;
  count: number;
  onConfirm: (erase: boolean) => void;
  onClose: () => void;
}) {
  const [erase, setErase] = useState(false);
  useEffect(() => {
    if (open) setErase(false);
  }, [open]);
  if (!open) return null;
  return (
    <div className="dl-modal-backdrop" onMouseDown={onClose}>
      <div className="dl-modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="dl-modal-title">删除{count > 1 ? `（${count} 项）` : "下载"}</div>
        <p className="dl-delete-hint">确认后将从列表中移除记录；勾选下方选项可一并删除已下载文件。</p>
        <div className="dl-delete-footer">
          <label className="dl-delete-erase">
            <input type="checkbox" checked={erase} onChange={(e) => setErase(e.target.checked)} />
            同时删除已下载文件
          </label>
          <div className="dl-delete-actions">
            <button onClick={onClose}>取消</button>
            <button className="danger" onClick={() => onConfirm(erase)}>确认</button>
          </div>
        </div>
      </div>
    </div>
  );
}

function RelabelDialog({
  open,
  labels,
  current,
  onSubmit,
  onClose,
}: {
  open: boolean;
  labels: string[];
  current: string | null;
  onSubmit: (label: string) => void;
  onClose: () => void;
}) {
  const [text, setText] = useState("");
  useEffect(() => {
    if (open) setText("");
  }, [open]);
  if (!open) return null;
  const trimmed = text.trim();
  const apply = (label: string) => {
    onSubmit(label);
    onClose();
  };
  const create = () => {
    if (trimmed) apply(trimmed);
  };
  return (
    <div className="dl-modal-backdrop" onMouseDown={onClose}>
      <div className="dl-modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="dl-modal-title">修改标签</div>
        <div className="dl-modal-labels">
          {labels.map((l) => (
            <button
              key={l}
              className={`dl-modal-label${l === current ? " active" : ""}`}
              onClick={() => apply(l)}
            >
              {l}
            </button>
          ))}
        </div>
        <div className="dl-modal-new">
          <input
            autoFocus
            className="dl-modal-input"
            placeholder="新建标签…"
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && create()}
          />
          <button onClick={create} disabled={!trimmed}>新建</button>
        </div>
        <button className="dl-modal-close" onClick={onClose}>关闭</button>
      </div>
    </div>
  );
}
