import { useEffect, useState } from "react";
import type { CSSProperties, SyntheticEvent } from "react";
import { remoteImage } from "../lib/ehimg";

/** Async source for a remote image URL (empty while loading / on failure). */
export function useRemoteImage(url: string | undefined | null): string | undefined {
  const [src, setSrc] = useState<string | undefined>(undefined);
  useEffect(() => {
    let alive = true;
    setSrc(undefined);
    if (!url) return;
    remoteImage(url).then((d) => {
      if (alive && d) setSrc(d);
    });
    return () => {
      alive = false;
    };
  }, [url]);
  return src;
}

interface RemoteImgProps {
  url: string | undefined | null;
  alt?: string;
  className?: string;
  style?: CSSProperties;
  loading?: "lazy" | "eager";
  onLoad?: (e: SyntheticEvent<HTMLImageElement>) => void;
}

/** `<img>` that resolves its source via the backend `fetch_image` command. */
export function RemoteImg({ url, alt, className, style, loading, onLoad }: RemoteImgProps) {
  const src = useRemoteImage(url);
  return <img src={src} alt={alt ?? ""} className={className} style={style} loading={loading} onLoad={onLoad} />;
}
