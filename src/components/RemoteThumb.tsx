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

/**
 * Async thumbnail. When a preview item is part of a shared sprite montage
 * (`x`/`y` crop offsets + clip width/height), renders the region cut from the
 * sprite so each page shows its own thumbnail instead of reusing the first.
 */
export function RemoteThumb({ url, alt, className, loading, x, y, w, h }: RemoteThumbProps) {
  const src = useRemoteImage(url);
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
  const dims = w && h ? { width: w, height: h } : undefined;
  return <img src={src} alt={alt ?? ""} className={className} style={dims} loading={loading} />;
}
