import { useState } from "react";
import type { GalleryItem } from "../types";
import { downloadStart } from "../lib/api";
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

  const runQuickDownload = () => {
    // Already queued (or in flight / showing feedback): no-op.
    if (queued || dl !== "idle") return;
    setDl("busy");
    const canonical = `https://e-hentai.org/g/${item.gid}/${item.token}/`;
    downloadStart(item.gid, item.token, item.title, "", item.uploader || "", "", "", item.pages, canonical)
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
    ? "已在下载队列"
    : dl === "done"
      ? "✓ 已加入"
      : dl === "error"
        ? "失败"
        : "快捷下载";

  return (
    <button type="button" className="gcard" onClick={() => onClick(item)}>
      <div className="gcard-thumb">
        {item.thumb ? (
          <RemoteImg url={item.thumb} alt="" loading="lazy" />
        ) : (
          <div className="gcard-thumb-placeholder" />
        )}
        <span
          role="button"
          tabIndex={queued ? -1 : 0}
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
  );
}

function formatRating(r: number): string {
  if (r < 0) return "未评分";
  return "★ " + r.toFixed(2).replace(/0$/, "");
}
