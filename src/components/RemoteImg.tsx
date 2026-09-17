import { useCallback, useEffect, useState } from "react";
import type { CSSProperties, SyntheticEvent } from "react";
import { remoteImage } from "../lib/ehimg";

export interface ImageState {
  src?: string;
  error?: string;
}

/** Async source for a remote image URL, plus a retry handle. On failure `error`
 * is set to a user-facing backend message instead of silently blanking. */
export function useRemoteImage(
  url: string | undefined | null,
): ImageState & { retry: () => void } {
  const [state, setState] = useState<ImageState>({});
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let alive = true;
    setState({});
    if (!url) return;
    remoteImage(url)
      .then((d) => {
        if (alive && d) setState({ src: d });
      })
      .catch((e: Error) => {
        if (alive) setState({ error: e.message || "图片加载失败" });
      });
    return () => {
      alive = false;
    };
  }, [url, tick]);
  const retry = useCallback(() => setTick((t) => t + 1), []);
  return { ...state, retry };
}

interface RemoteImgProps {
  url: string | undefined | null;
  alt?: string;
  className?: string;
  style?: CSSProperties;
  loading?: "lazy" | "eager";
  onLoad?: (e: SyntheticEvent<HTMLImageElement>) => void;
}

function fallbackStyle(style?: CSSProperties): CSSProperties {
  return {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    minHeight: 48,
    margin: "auto",
    padding: 8,
    boxSizing: "border-box",
    background: "transparent",
    border: "1px dashed rgba(128,128,128,.5)",
    borderRadius: 6,
    color: "var(--text-secondary, #9aa0a6)",
    fontSize: 12,
    cursor: "pointer",
    ...style,
  };
}

/** `<img>` that resolves its source via the backend `fetch_image` command. When
 * the backend reports an error (e.g. blocked image host / fetch failure) shows a
 * clickable "加载失败 · 重试" fallback with the real reason in the tooltip. */
export function RemoteImg({ url, alt, className, style, loading, onLoad }: RemoteImgProps) {
  const { src, error, retry } = useRemoteImage(url);
  if (error) {
    return (
      <div
        className={className}
        style={fallbackStyle(style)}
        title={error}
        onClick={retry}
        role="button"
      >
        加载失败 · 重试
      </div>
    );
  }
  if (!src) {
    return <div className={className} style={style ?? { minHeight: 48 }} aria-label={alt ?? "加载中"} />;
  }
  return <img src={src} alt={alt ?? ""} className={className} style={style} loading={loading} onLoad={onLoad} />;
}
