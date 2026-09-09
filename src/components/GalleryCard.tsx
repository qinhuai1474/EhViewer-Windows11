import type { GalleryItem } from "../types";
import { RemoteImg } from "./RemoteImg";
import "./gallery.css";

interface Props {
  item: GalleryItem;
  onClick: (item: GalleryItem) => void;
}

export function GalleryCard({ item, onClick }: Props) {
  const cat = item.category || "unknown";
  return (
    <button type="button" className="gcard" onClick={() => onClick(item)}>
      <div className="gcard-thumb">
        {item.thumb ? (
          <RemoteImg url={item.thumb} alt="" loading="lazy" />
        ) : (
          <div className="gcard-thumb-placeholder" />
        )}
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
