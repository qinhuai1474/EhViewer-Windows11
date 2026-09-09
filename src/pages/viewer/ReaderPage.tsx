import { useCallback, useEffect, useRef, useState } from "react";
import type { GalleryItem } from "../../types";
import { getOnlinePage, getReadingProgress, saveReadingProgress } from "../../lib/api";
import { remoteImage } from "../../lib/ehimg";
import "./viewer.css";

type Direction = "ltr" | "rtl" | "vertical";
type Zoom = "original" | "fit_width" | "fit_height" | "fit_screen" | "custom";

interface Props {
  item: GalleryItem;
  startIndex?: number;
  onBack: () => void;
}

const PORTS: Record<Direction, string> = {
  ltr: "LTR",
  rtl: "RTL",
  vertical: "纵式滚动",
};

const STATUS_TEXT: Record<string, string> = {
  loading: "加载中",
  error: "加载失败",
  ok: "",
};

export function ReaderPage({ item, startIndex = 0, onBack }: Props) {
  const [index, setIndex] = useState(startIndex);
  const [images, setImages] = useState<Record<number, string>>({});
  const [direction, setDirection] = useState<Direction>("ltr");
  const [zoom, setZoom] = useState<Zoom>("fit_width");
  const [scale, setScale] = useState(100);
  const [fullscreen, setFullscreen] = useState(false);
  const [status, setStatus] = useState<"loading" | "error" | "ok">("loading");
  const listRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; sl: number; st: number } | null>(null);
  const loadedRef = useRef<Set<number>>(new Set());
  const failedRef = useRef<Set<number>>(new Set());
  const [failed, setFailed] = useState<Set<number>>(new Set());

  // Track the WebView native fullscreen state so the toolbar reflects it.
  useEffect(() => {
    const onFs = () => setFullscreen(Boolean(document.fullscreenElement));
    document.addEventListener("fullscreenchange", onFs);
    return () => document.removeEventListener("fullscreenchange", onFs);
  }, []);

  const toggleFs = useCallback(() => {
    if (document.fullscreenElement) {
      document.exitFullscreen().catch(() => undefined);
    } else {
      document.documentElement.requestFullscreen().catch(() => undefined);
    }
  }, []);

  // Resume from disk when no explicit start index was requested.
  useEffect(() => {
    if (startIndex === undefined) {
      getReadingProgress(item.gid)
        .then((p) => {
          if (p > 0 && p < item.pages) setIndex(p);
        })
        .catch(() => undefined);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item.gid]);

  const loadRange = useCallback(
    async (center: number) => {
      const min = Math.max(0, center - 2);
      const max = Math.min(item.pages, center + 3);
      const jobs: number[] = [];
      // Only request pages not yet loaded and not already failed, so this stable
      // callback never re-fetches; failures can be retried explicitly below.
      for (let i = min; i < max; i++) {
        if (!loadedRef.current.has(i) && !failedRef.current.has(i)) jobs.push(i);
      }
      if (jobs.length === 0) return;

      for (const i of jobs) {
        try {
          const page = await getOnlinePage(0, item.gid, item.token, i, "");
          if (page.imageUrl) {
            const src = await remoteImage(page.imageUrl);
            if (src) {
              loadedRef.current.add(i);
              failedRef.current.delete(i);
              setImages((prev) => (prev[i] ? prev : { ...prev, [i]: src }));
            } else {
              failedRef.current.add(i);
            }
          } else {
            failedRef.current.add(i);
          }
        } catch {
          // Record the failure so the UI shows a retryable error state.
          failedRef.current.add(i);
        }
      }
      setFailed(new Set(failedRef.current));
      // eslint-disable-next-line react-hooks/exhaustive-deps
    },
    [item.gid, item.token, item.pages],
  );

  const retry = useCallback(() => {
    failedRef.current.clear();
    setFailed(new Set());
    loadedRef.current.clear();
    setImages({});
    loadRange(index);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loadRange, index]);

  useEffect(() => {
    setStatus(images[index] ? "ok" : failed.has(index) ? "error" : "loading");
    if (!images[index]) {
      loadRange(index);
      saveReadingProgress(item.gid, index).catch(() => undefined);
    }
  }, [index, images, failed, loadRange, item.gid]);

  const goto = useCallback(
    (p: number) => {
      const next = Math.max(0, Math.min(item.pages - 1, p));
      setIndex(next);
    },
    [item.pages],
  );

  // Keyboard navigation.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      switch (e.key) {
        case "ArrowRight":
          if (direction === "rtl") goto(index - 1);
          else if (direction !== "vertical") goto(index + 1);
          break;
        case "ArrowLeft":
          if (direction === "rtl") goto(index + 1);
          else if (direction !== "vertical") goto(index - 1);
          break;
        case "ArrowDown":
        case "PageDown":
        case " ":
          if (direction === "vertical") { e.preventDefault(); goto(index + 1); }
          break;
        case "ArrowUp":
        case "PageUp":
          if (direction === "vertical") { e.preventDefault(); goto(index - 1); }
          break;
        case "f":
        case "F":
          toggleFs();
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [direction, index, goto, toggleFs]);

  const contentImg = images[index];
  const vertical = direction === "vertical";

  function imgStyleFor(zoomMode: Zoom): React.CSSProperties {
    switch (zoomMode) {
      case "original":
        return { width: "auto", maxWidth: "none", height: "auto" };
      case "fit_height":
        return { maxHeight: "100%", maxWidth: "100%", height: "100%", objectFit: "contain" };
      case "fit_screen":
        return { maxWidth: "100%", maxHeight: "100%", width: "100%", height: "100%", objectFit: "contain" };
      case "custom":
        return { width: `${scale}%`, height: "auto", maxWidth: "none" };
      default:
        return { maxWidth: "100%", maxHeight: "100%", height: "auto" };
    }
  }

  const onWheel = (e: React.WheelEvent) => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    setZoom("custom");
    setScale((s) => Math.max(20, Math.min(500, s - (e.deltaY > 0 ? 10 : -10))));
  };

  const onPointer = (e: React.PointerEvent) => {
    const el = listRef.current;
    if (!el) return;
    if (e.buttons === 1) {
      drag.current = { x: e.clientX, y: e.clientY, sl: el.scrollLeft, st: el.scrollTop };
      el.setPointerCapture(e.pointerId);
    }
  };
  const onMove = (e: React.PointerEvent) => {
    const el = listRef.current;
    if (!el || !drag.current) return;
    el.scrollLeft = drag.current.sl - (e.clientX - drag.current.x);
    el.scrollTop = drag.current.st - (e.clientY - drag.current.y);
  };
  const onUp = () => (drag.current = null);

  return (
    <div className={fullscreen ? "reader reader-fullscreen" : "reader"}>
      <div className="reader-toolbar">
        <button onClick={onBack}>← 退出</button>
        <b>{index + 1} / {item.pages}</b>
        <select value={direction} onChange={(e) => setDirection(e.currentTarget.value as Direction)}>
          {Object.entries(PORTS).map(([v, l]) => <option key={v} value={v}>{l}</option>)}
        </select>
        <select value={zoom} onChange={(e) => setZoom(e.currentTarget.value as Zoom)}>
          <option value="original">原尺寸</option>
          <option value="fit_width">适配宽</option>
          <option value="fit_height">适配高</option>
          <option value="fit_screen">适配屏幕</option>
          <option value="custom">固定缩放</option>
        </select>
        {zoom === "custom" && (
          <label className="reader-scale">
            <input type="range" min={20} max={500} value={scale} onChange={(e) => setScale(Number(e.currentTarget.value))} />
            {scale}%
          </label>
        )}
        <button onClick={toggleFs}>{fullscreen ? "退出全屏" : "全屏"}</button>
        <span className="reader-status">{STATUS_TEXT[status]}</span>
      </div>

      <div
        ref={listRef}
        className={"reader-viewport" + (vertical ? " reader-vertical" : "")}
        onWheel={onWheel}
        onPointerDown={onPointer}
        onPointerMove={onMove}
        onPointerUp={onUp}
      >
        {vertical ? (
          Object.keys(images)
            .map(Number)
            .sort((a, b) => a - b)
            .map((i) => (
              <img key={i} src={images[i]} alt={`第 ${i + 1} 页`} className="reader-img-v" style={imgStyleFor(zoom)} />
            ))
        ) : (
          <>
            <img src={contentImg} alt={`第 ${index + 1} 页`} className="reader-img" style={imgStyleFor(zoom)} />
            {status === "error" && (
              <div className="reader-fail">
                <p>图片加载失败（该页可能受限或网络异常）</p>
                <button onClick={retry}>重试</button>
              </div>
            )}
          </>
        )}
      </div>
      <div className="reader-progress"><div style={{ width: `${((index + 1) / item.pages) * 100}%` }} /></div>
    </div>
  );
}
