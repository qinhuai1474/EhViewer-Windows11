import { useState } from "react";
import type { GalleryItem } from "../types";
import { downloadDelete, downloadStart } from "../lib/api";
import { RemoteImg } from "./RemoteImg";
import "./gallery.css";

interface Props {
  item: GalleryItem;
  onClick: (item: GalleryItem) => void;
  /** True when this gallery is already in the download queue. */
  queued?: boolean;
}

export function GalleryCard({ item, onClick, queued = false }: Props) {
  const cat = item.category || "unknown";
  // Quick-download feedback: idle | busy | done | error.
  const [dl, setDl] = useState<"idle" | "busy" | "done" | "error">("idle");
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [erase, setErase] = useState(false);

  const runQuickDownload = () => {
    // Already queued: open the remove-from-queue confirm instead.
    if (queued) {
      setDeleteOpen(true);
      return;
    }
    // In flight / showing feedback: no-op.
    if (dl !== "idle") return;
    setDl("busy");
    const canonical = `https://e-hentai.org/g/${item.gid}/${item.token}/`;
    downloadStart(item.gid, item.token, item.title, "", item.uploader || "", "", "", item.pages, canonical, item.thumb || "")
      .then(() => {
        setDl("done");
        setTimeout(() => setDl("idle"), 1600);
      })
      .catch(() => {
        setDl("error");
        setTimeout(() => setDl("idle"), 1600);
      });
  };

  const quickDownload = (e: React.MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
    runQuickDownload();
  };

  const quickDownloadKey = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      e.stopPropagation();
      runQuickDownload();
    }
  };

  const dlClass = [
    "gcard-dl",
    queued ? "gcard-dl--queued" : "",
    dl === "done" || dl === "error" ? "gcard-dl--text" : "",
    dl === "done" ? "gcard-dl--done" : "",
    dl === "error" ? "gcard-dl--error" : "",
  ]
    .filter(Boolean)
    .join(" ");

  const label = queued
    ? "点击移除下载"
    : dl === "done"
      ? "✓ 已加入"
      : dl === "error"
        ? "失败"
        : "快捷下载";

  return (
    <>
      <button type="button" className="gcard" onClick={() => onClick(item)}>
      <div className="gcard-thumb">
        {item.thumb ? (
          <RemoteImg url={item.thumb} alt="" loading="lazy" />
        ) : (
          <div className="gcard-thumb-placeholder" />
        )}
        <span
          role="button"
          tabIndex={0}
          title={label}
          aria-label={label}
          className={dlClass}
          onClick={quickDownload}
          onKeyDown={quickDownloadKey}
        >
          {queued ? (
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.4"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <path d="M5 13l4 4 10-10" />
            </svg>
          ) : dl === "done" ? (
            "✓ 已加入"
          ) : dl === "error" ? (
            "失败"
          ) : (
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <path d="M12 5v10m0 0 4-4m-4 4-4-4" />
              <path d="M5 19h14" />
            </svg>
          )}
        </span>
        <span className="gcard-pages">{item.pages} 页</span>
      </div>
      <div className="gcard-body">
        <span className={`gcard-cat cat-${cat}`}>{cat}</span>
        <h3 className="gcard-title">{item.title}</h3>
        <div className="gcard-meta">
          <span>{formatRating(item.rating)}</span>
          {item.uploader ? <span>· {item.uploader}</span> : null}
        </div>
        <div className="gcard-posted">{item.posted}</div>
      </div>
      </button>
      <DlConfirm
        open={deleteOpen}
        erase={erase}
        onErase={setErase}
        onConfirm={() =>
          downloadDelete(item.gid, erase)
            .then(() => setDeleteOpen(false))
            .catch(() => setDeleteOpen(false))
        }
        onClose={() => setDeleteOpen(false)}
      />
    </>
  );
}

function formatRating(r: number): string {
  if (r < 0) return "未评分";
  return "★ " + r.toFixed(2).replace(/0$/, "");
}

function DlConfirm({
  open,
  erase,
  onErase,
  onConfirm,
  onClose,
}: {
  open: boolean;
  erase: boolean;
  onErase: (v: boolean) => void;
  onConfirm: () => void;
  onClose: () => void;
}) {
  if (!open) return null;
  return (
    <div className="gdl-backdrop" onMouseDown={onClose}>
      <div className="gdl-modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="gdl-modal-title">移除下载</div>
        <p className="gdl-hint">确认后将从下载队列移除该画廊；勾选下方选项可一并删除已下载文件。</p>
        <div className="gdl-delete-erase">
          <label>
            <input type="checkbox" checked={erase} onChange={(e) => onErase(e.target.checked)} />
            同时删除已下载文件
          </label>
        </div>
        <div className="gdl-actions">
          <button onClick={onClose}>取消</button>
          <button className="gdl-confirm" onClick={onConfirm}>确认</button>
        </div>
      </div>
    </div>
  );
}
