import type { CSSProperties } from "react";
import { useRemoteImage } from "./RemoteImg";

interface RemoteThumbProps {
  url: string | undefined | null;
  alt?: string;
  className?: string;
  loading?: "lazy" | "eager";
  x?: number;
  y?: number;
  w?: number;
  h?: number;
}

/** Async thumbnail. When a preview item is part of a shared sprite montage
 * (`x`/`y` crop offsets + clip width/height), renders the region cut from the
 * sprite so each page shows its own thumbnail instead of reusing the first. On a
 * backend failure shows a clickable "加载失败 · 重试" fallback instead of a blank. */
export function RemoteThumb({ url, alt, className, loading, x, y, w, h }: RemoteThumbProps) {
  const { src, error, retry } = useRemoteImage(url);
  const dims = w && h ? { width: w, height: h } : undefined;

  if (error) {
    const style: CSSProperties = {
      ...(dims ?? { minHeight: 60 }),
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      boxSizing: "border-box",
      border: "1px dashed rgba(128,128,128,.5)",
      borderRadius: 4,
      color: "var(--text-secondary, #9aa0a6)",
      fontSize: 11,
      cursor: "pointer",
    };
    return (
      <div className={className} style={style} title={error} onClick={retry} role="button">
        加载失败 · 重试
      </div>
    );
  }

  const crop = src && x !== undefined && w && h;
  if (crop) {
    const style: CSSProperties = {
      width: w,
      height: h,
      backgroundImage: `url("${src}")`,
      backgroundPosition: `${-(x ?? 0)}px ${-(y ?? 0)}px`,
      backgroundRepeat: "no-repeat",
    };
    return <div className={className} style={style} role="img" aria-label={alt ?? ""} />;
  }
  return <img src={src} alt={alt ?? ""} className={className} style={dims} loading={loading} />;
}
