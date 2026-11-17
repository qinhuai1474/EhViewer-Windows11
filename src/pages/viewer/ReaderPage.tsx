import { useCallback, useEffect, useRef, useState } from "react";
import type { GalleryItem } from "../../types";
import {
  getOnlinePage,
  getReadingProgress,
  saveReadingProgress,
  settingsGet,
  settingsSet,
} from "../../lib/api";
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

// ----- Phase 2 memory bounds -----
const MAX_PAGER_PAGES = 60; // single-page (ltr / rtl) image ceiling
const VERT_BEHIND = 30; // vertical pages kept behind the current one (fixed)
const VERT_AHEAD_DEFAULT = 20; // default ahead-prefetch window (vertical + pager)
const VERT_AHEAD_MAX = 20; // user-customisable ceiling for ahead-prefetch
const MAX_RETRY = 3; // automatic per-page load attempts before marking failed
const CLICK_DRAG_THRESHOLD = 8; // px of pointer travel that counts as a drag
const DOUBLE_TAP_MS = 350; // double-click / double-tap window

const ADJ_STORAGE_KEY = (gid: number) => `ehReaderAdj:${gid}`;

function isDirection(v: string): v is Direction {
  return v === "ltr" || v === "rtl" || v === "vertical";
}
function isZoom(v: string): v is Zoom {
  return (
    v === "original" || v === "fit_width" || v === "fit_height" || v === "fit_screen" || v === "custom"
  );
}

/** Reader loading indicator — a determinate progress ring with a percent label
 * while `percent` is known, otherwise an indeterminate spinning circle. Mirrors
 * SXJ's gallery page loader (receivedSize / contentLength when Content-Length is
 * provided, else the generic spinner). */
function LoadingIndicator({
  className,
  percent,
  label = "加载中",
  page,
}: {
  className: string;
  percent?: number;
  label?: string;
  page?: number;
}) {
  const dataProp = page !== undefined ? ({ "data-index": String(page) } as Record<string, string>) : undefined;
  if (percent !== undefined) {
    const pct = Math.round(percent * 100);
    return (
      <div
        {...(dataProp ?? {})}
        className={`${className} reader-loading-det`}
        role="status"
        aria-label={`${label} ${pct}%`}
        style={{ background: `conic-gradient(var(--color-accent) ${pct}%, rgba(91,141,239,0.25) ${pct}%)` }}
      >
        <span>{pct}%</span>
      </div>
    );
  }
  return <div {...(dataProp ?? {})} className={className} role="status" aria-label={label} />;
}

export function ReaderPage({ item, startIndex = 0, onBack }: Props) {
  const [index, setIndex] = useState(startIndex);
  const [images, setImages] = useState<Record<number, string>>({});
  const [direction, setDirection] = useState<Direction>("ltr");
  const [zoom, setZoom] = useState<Zoom>("fit_width");
  const [scale, setScale] = useState(100);
  const [fullscreen, setFullscreen] = useState(false);
  const [status, setStatus] = useState<"loading" | "error" | "ok">("loading");
  // Reading prefs loaded once from global settings.
  const [prefs, setPrefs] = useState<{ loaded: boolean; first: boolean }>({ loaded: false, first: false });

  const listRef = useRef<HTMLDivElement>(null);
  const sentinelRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; sl: number; st: number } | null>(null);
  const clickStart = useRef<{ x: number; y: number } | null>(null);
  const scrollRaf = useRef<number | null>(null);
  const loadedRef = useRef<Set<number>>(new Set());
  const failedRef = useRef<Set<number>>(new Set());
  const isSeekingRef = useRef(false);
  const attemptRef = useRef<Record<number, number>>({});
  const idleTimerRef = useRef<number | null>(null);
  const pagerPrefetchRef = useRef(false);
  const [failed, setFailed] = useState<Set<number>>(new Set());
  const [progress, setProgress] = useState<Record<number, number>>({});
  const [forwardPreload, setForwardPreload] = useState(VERT_AHEAD_DEFAULT);
  const [norm, setNorm] = useState(0); // vertical scroll fraction 0..1 (slider + progress bar)
  const [thumbOpen, setThumbOpen] = useState(false);
  const [barHidden, setBarHidden] = useState(false);
  const [adjMap, setAdjMap] = useState<Record<number, { rot: number; flip: boolean }>>({});

  const verticalRef = useRef(false);
  const indexRef = useRef(index);
  const lastTapRef = useRef(0);
  const turnTimerRef = useRef<number | null>(null);
  const pendingTargetRef = useRef<"prev" | "next" | null>(null);
  verticalRef.current = direction === "vertical";
  indexRef.current = index;

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

  // Load reading prefs once: direction, zoom, and whether to start from page 1.
  useEffect(() => {
    let alive = true;
    settingsGet()
      .then((s) => {
        if (!alive) return;
        if (isDirection(s.reading_direction)) setDirection(s.reading_direction);
        if (isZoom(s.zoom_mode)) setZoom(s.zoom_mode);
        const pre = typeof s.reading_vertical_preload === "number" ? s.reading_vertical_preload : VERT_AHEAD_DEFAULT;
        setForwardPreload(Math.max(0, Math.min(VERT_AHEAD_MAX, Math.round(pre))));
        try {
          const raw = localStorage.getItem(ADJ_STORAGE_KEY(item.gid));
          if (raw) setAdjMap((JSON.parse(raw) as Record<number, { rot: number; flip: boolean }>) ?? {});
        } catch {
          /* ignore corrupt rotation value */
        }
        setPrefs({ loaded: true, first: s.reading_start_position === "first" });
      })
      .catch(() => {
        if (alive) setPrefs((p) => ({ ...p, loaded: true }));
      });
    return () => {
      alive = false;
    };
  }, []);

  // Resume from disk unless the user asked to start from page 1 or an explicit
  // page was chosen (e.g. from the preview grid).
  useEffect(() => {
    if (startIndex !== undefined || !prefs.loaded || prefs.first) return;
    getReadingProgress(item.gid)
      .then((p) => {
        if (p > 0 && p < item.pages) setIndex(p);
      })
      .catch(() => undefined);
  }, [item.gid, item.pages, startIndex, prefs.loaded, prefs.first]);

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
        let done = false;
        // Automatic retry: attempt up to MAX_RETRY times per page; on the final
        // attempt fall back to the original full-res link when it is available.
        for (let attempt = 0; attempt < MAX_RETRY && !done; attempt++) {
          attemptRef.current[i] = (attemptRef.current[i] ?? 0) + 1;
          let page: { imageUrl: string; originImageUrl?: string | null } | null = null;
          try {
            page = await getOnlinePage(0, item.gid, item.token, i, "");
          } catch {
            page = null;
          }
          const url = page?.imageUrl;
          if (!url) continue;
          const target = attempt === MAX_RETRY - 1 && page?.originImageUrl ? page.originImageUrl : url;
          try {
            const src = await remoteImage(target, (p) =>
              setProgress((prev) => (prev[i] === p ? prev : { ...prev, [i]: p })),
            );
            setProgress((prev) => {
              if (!(i in prev)) return prev;
              const next = { ...prev };
              delete next[i];
              return next;
            });
            if (src) {
              loadedRef.current.add(i);
              failedRef.current.delete(i);
              setImages((prev) => (prev[i] ? prev : { ...prev, [i]: src }));
              done = true;
            }
          } catch {
            setProgress((prev) => {
              if (!(i in prev)) return prev;
              const next = { ...prev };
              delete next[i];
              return next;
            });
          }
        }
        if (!done) failedRef.current.add(i);
      }
      setFailed(new Set(failedRef.current));
      // eslint-disable-next-line react-hooks/exhaustive-deps
    },
    [item.gid, item.token, item.pages],
  );

  const retry = useCallback(() => {
    failedRef.current.clear();
    setFailed(new Set());
    setProgress({});
    attemptRef.current = {};
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [index, images, failed, loadRange, item.gid]);

  const scrollToIndex = useCallback((p: number) => {
    const el = listRef.current;
    if (!el) return;
    el.querySelector<HTMLElement>(`[data-index="${p}"]`)?.scrollIntoView({ block: "start" });
  }, []);

  const goto = useCallback(
    (p: number) => {
      const next = Math.max(0, Math.min(item.pages - 1, p));
      setIndex(next);
      if (direction === "vertical") scrollToIndex(next);
    },
    [item.pages, direction, scrollToIndex],
  );

  // Per-page orientation correction (rotate / mirror), persisted to localStorage.
  const rotateCurrent = useCallback((d: 90 | -90 | 180) => {
    setAdjMap((prev) => {
      const c = prev[index] ?? { rot: 0, flip: false };
      return { ...prev, [index]: { rot: (c.rot + d + 360) % 360, flip: c.flip } };
    });
  }, [index]);
  const flipCurrent = useCallback(() => {
    setAdjMap((prev) => {
      const c = prev[index] ?? { rot: 0, flip: false };
      return { ...prev, [index]: { rot: c.rot, flip: !c.flip } };
    });
  }, [index]);

  // Continuous seek: write a scroll fraction (0..1) directly to the viewport.
  const applyNorm = useCallback((k: number) => {
    const el = listRef.current;
    if (!el) return;
    const maxScroll = el.scrollHeight - el.clientHeight;
    if (maxScroll > 0) el.scrollTop = Math.min(maxScroll, Math.max(0, k * maxScroll));
  }, []);

  // Keyboard navigation.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Never intercept while the user is interacting with a form control.
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "SELECT" || t.tagName === "TEXTAREA")) return;
      switch (e.key) {
        case "ArrowRight":
        case "d":
        case "D":
          if (direction === "rtl") goto(index - 1);
          else if (direction !== "vertical") goto(index + 1);
          break;
        case "ArrowLeft":
        case "a":
        case "A":
          if (direction === "rtl") goto(index + 1);
          else if (direction !== "vertical") goto(index - 1);
          break;
        case "ArrowDown":
        case "PageDown":
        case " ":
          if (direction === "vertical") {
            e.preventDefault();
            goto(index + 1);
          }
          break;
        case "ArrowUp":
        case "PageUp":
          if (direction === "vertical") {
            e.preventDefault();
            goto(index - 1);
          }
          break;
        case "[":
          rotateCurrent(-90);
          break;
        case "]":
          rotateCurrent(90);
          break;
        case "0":
          rotateCurrent(180);
          break;
        case "\\":
          flipCurrent();
          break;
        case "t":
        case "T":
          setBarHidden((h) => !h);
          if (idleTimerRef.current != null) clearTimeout(idleTimerRef.current);
          idleTimerRef.current = null;
          break;
        case "f":
        case "F":
          toggleFs();
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [direction, index, goto, toggleFs, rotateCurrent, flipCurrent]);

  // Vertical scroll: keep current page, progress and prefetch synced to the page
  // nearest the viewport centre (rAF-throttled, pager modes are skipped).
  const onScroll = useCallback(() => {
    if (!verticalRef.current) return;
    if (scrollRaf.current != null) return;
    scrollRaf.current = requestAnimationFrame(() => {
      scrollRaf.current = null;
      const el = listRef.current;
      if (!el) return;
      const mid = el.scrollTop + el.clientHeight / 2;
      let best = -1;
      let bestDist = Number.POSITIVE_INFINITY;
      for (const c of Array.from(el.children)) {
        const i = Number((c as HTMLElement).dataset?.index);
        if (!Number.isFinite(i)) continue;
        const rect = (c as HTMLElement).getBoundingClientRect();
        const d = Math.abs(rect.top + rect.height / 2 - mid);
        if (d < bestDist) {
          bestDist = d;
          best = i;
        }
      }
      if (best >= 0) setIndex((cur) => (cur === best ? cur : best));
      if (!isSeekingRef.current) {
        const maxScroll = el.scrollHeight - el.clientHeight;
        const n = maxScroll > 0 ? Math.min(1, Math.max(0, el.scrollTop / maxScroll)) : 0;
        setNorm(n);
      }
    });
  }, []);

  // Sync the shared scroll fraction when (re)entering vertical mode.
  useEffect(() => {
    if (!verticalRef.current) return;
    const el = listRef.current;
    if (el) {
      const maxScroll = el.scrollHeight - el.clientHeight;
      const n = maxScroll > 0 ? Math.min(1, Math.max(0, el.scrollTop / maxScroll)) : 0;
      setNorm(n);
    }
  }, [direction]);

  // Persist per-page orientation corrections for this gallery.
  useEffect(() => {
    if (!Object.keys(adjMap).length) return;
    try {
      localStorage.setItem(ADJ_STORAGE_KEY(item.gid), JSON.stringify(adjMap));
    } catch {
      /* ignore quota / storage errors */
    }
  }, [adjMap, item.gid]);

  // Immersive toolbar: fade the bar out after 1s of idle in vertical fullscreen;
  // pointer / wheel activity (or the T key) brings it back.
  useEffect(() => {
    if (!verticalRef.current) return;
    const reveal = () => {
      setBarHidden(false);
      if (idleTimerRef.current != null) clearTimeout(idleTimerRef.current);
      if (document.fullscreenElement) {
        idleTimerRef.current = window.setTimeout(() => setBarHidden(true), 1000);
      } else {
        idleTimerRef.current = null;
      }
    };
    window.addEventListener("pointermove", reveal);
    window.addEventListener("wheel", reveal);
    reveal();
    return () => {
      if (idleTimerRef.current != null) clearTimeout(idleTimerRef.current);
      window.removeEventListener("pointermove", reveal);
      window.removeEventListener("wheel", reveal);
    };
  }, [direction]);

  const onWheel = useCallback((e: React.WheelEvent) => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    setZoom("custom");
    setScale((s) => Math.max(20, Math.min(500, s - (e.deltaY > 0 ? 10 : -10))));
  }, []);

  const onPointerDown = useCallback((e: React.PointerEvent) => {
    const el = listRef.current;
    if (!el) return;
    if (e.buttons === 1) {
      clickStart.current = { x: e.clientX, y: e.clientY };
      drag.current = { x: e.clientX, y: e.clientY, sl: el.scrollLeft, st: el.scrollTop };
      el.setPointerCapture(e.pointerId);
    }
  }, []);

  const onPointerMove = useCallback((e: React.PointerEvent) => {
    const el = listRef.current;
    if (!el || !drag.current) return;
    el.scrollLeft = drag.current.sl - (e.clientX - drag.current.x);
    el.scrollTop = drag.current.st - (e.clientY - drag.current.y);
  }, []);

  // A single-click on the left/right third turns the page in pager modes; two
  // clicks within DOUBLE_TAP_MS toggle fit_width <-> fit_screen (SXJ-style).
  // The first turn is deferred so a follow-up click can upgrade to a double-click.
  const onPointerUp = useCallback(
    (e: React.PointerEvent) => {
      const start = clickStart.current;
      clickStart.current = null;
      const wasDragging = Boolean(drag.current);
      drag.current = null;

      let target: "prev" | "next" | null = null;
      if (!verticalRef.current && start && !wasDragging) {
        const dx = e.clientX - start.x;
        const dy = e.clientY - start.y;
        if (Math.hypot(dx, dy) <= CLICK_DRAG_THRESHOLD) {
          const el = listRef.current;
          if (el) {
            const rect = el.getBoundingClientRect();
            const relX = (e.clientX - rect.left) / rect.width;
            if (relX < 1 / 3) target = direction === "rtl" ? "next" : "prev";
            else if (relX > 2 / 3) target = direction === "rtl" ? "prev" : "next";
          }
        }
      }
      if (!target) return;

      const now = Date.now();
      if (now - lastTapRef.current < DOUBLE_TAP_MS) {
        // Second tap of a double-click: cancel the pending turn, zoom instead.
        lastTapRef.current = 0;
        if (turnTimerRef.current != null) {
          clearTimeout(turnTimerRef.current);
          turnTimerRef.current = null;
        }
        setZoom((z) => (z === "fit_width" ? "fit_screen" : "fit_width"));
        return;
      }

      lastTapRef.current = now;
      pendingTargetRef.current = target;
      if (turnTimerRef.current != null) clearTimeout(turnTimerRef.current);
      turnTimerRef.current = window.setTimeout(() => {
        turnTimerRef.current = null;
        lastTapRef.current = 0;
        const t = pendingTargetRef.current;
        pendingTargetRef.current = null;
        if (!t || verticalRef.current) return;
        goto(t === "prev" ? indexRef.current - 1 : indexRef.current + 1);
      }, DOUBLE_TAP_MS + 40);
    },
    [direction, goto],
  );

  const onDirectionChange = useCallback((e: React.ChangeEvent<HTMLSelectElement>) => {
    const v = e.currentTarget.value as Direction;
    setDirection(v);
    settingsSet("reading_direction", v).catch(() => undefined);
  }, []);

  const onZoomChange = useCallback((e: React.ChangeEvent<HTMLSelectElement>) => {
    const v = e.currentTarget.value as Zoom;
    setZoom(v);
    settingsSet("zoom_mode", v).catch(() => undefined);
  }, []);

  // Clean up rAF + pending turn timers on unmount.
  useEffect(
    () => () => {
      if (scrollRaf.current != null) cancelAnimationFrame(scrollRaf.current);
      if (turnTimerRef.current != null) clearTimeout(turnTimerRef.current);
    },
    [],
  );

  const contentImg = images[index];
  const vertical = direction === "vertical";

  // Vertical renders a sliding, contiguous window around the current page so the
  // DOM count stays bounded no matter how far into a long gallery we scroll.
  const verticalPages = (() => {
    if (!vertical) return [];
    const loadedIdx = Object.keys(images).map(Number);
    const maxLoaded = loadedIdx.length ? Math.max(...loadedIdx) : index;
    const lo = Math.max(0, index - VERT_BEHIND);
    const hi = Math.min(item.pages - 1, Math.max(index + forwardPreload, maxLoaded + 7));
    const out: { i: number; state: "loaded" | "failed" | "loading" }[] = [];
    for (let i = lo; i <= hi; i++) {
      const state = images[i] !== undefined ? "loaded" : failed.has(i) ? "failed" : "loading";
      out.push({ i, state });
    }
    return out;
  })();

  // Bound memory: evict far images in pager mode; in vertical prune to the
  // rendered window.
  useEffect(() => {
    if (verticalRef.current) {
      const loadedIdx = Object.keys(images).map(Number);
      // Act as a memory guard: only prune once the window has grown well past
      // its target size, so normal/asymptotic reading never shifts the layout.
      if (loadedIdx.length <= VERT_BEHIND + forwardPreload + 24) return;
      const maxLoaded = loadedIdx.length ? Math.max(...loadedIdx) : index;
      const lo = Math.max(0, index - VERT_BEHIND);
      const hi = Math.min(item.pages - 1, Math.max(index + forwardPreload, maxLoaded + 7));
      setImages((prev) => {
        let changed = false;
        const next: Record<number, string> = {};
        for (const k of Object.keys(prev)) {
          const n = Number(k);
          if (n >= lo && n <= hi) next[n] = prev[n];
          else changed = true;
        }
        return changed ? next : prev;
      });
    } else {
      setImages((prev) => {
        const keys = Object.keys(prev).map(Number);
        if (keys.length <= MAX_PAGER_PAGES) return prev;
        const sorted = [...keys].sort((a, b) => Math.abs(b - index) - Math.abs(a - index));
        const keep = new Set(sorted.slice(0, MAX_PAGER_PAGES));
        const next: Record<number, string> = {};
        for (const k of keys) if (keep.has(k)) next[k] = prev[k];
        return next;
      });
    }
  }, [item.pages, index, images, forwardPreload]); // eslint-disable-line react-hooks/exhaustive-deps

  // Drop `loadedRef` entries whose image was evicted so a revisit re-fetches.
  useEffect(() => {
    const cur = new Set(Object.keys(images).map(Number));
    let changed = false;
    for (const v of [...loadedRef.current]) {
      if (!cur.has(v)) {
        loadedRef.current.delete(v);
        changed = true;
      }
    }
    if (changed) setFailed(new Set(failedRef.current));
  }, [images]);

  // Vertical forward prefetch: when the reader nears the bottom of the loaded
  // window, fetch the next pages so scrolling stays continuous.
  useEffect(() => {
    if (!verticalRef.current) return;
    const sentinel = sentinelRef.current;
    const el = listRef.current;
    if (!sentinel || !el) return;
    const io = new IntersectionObserver(
      () => loadRange(Math.max(0, indexRef.current + forwardPreload - 2)),
      { root: el, rootMargin: "600px 0px" },
    );
    io.observe(sentinel);
    return () => io.disconnect();
  }, [loadRange, item.pages, vertical, forwardPreload]); // eslint-disable-line react-hooks/exhaustive-deps

  // Pager (ltr/rtl) forward prefetch: keep a warm run of pages ahead of the
  // current one using the same shared `forwardPreload` budget as vertical, so
  // turning pages stays smooth. Self-limits to the loaded head.
  useEffect(() => {
    if (verticalRef.current || pagerPrefetchRef.current) return;
    let head = index;
    for (const v of loadedRef.current) if (v > head) head = v;
    // Let the index-following load do its job first; prefetch the next budget
    // only once the current page has caught up to the loaded head.
    if (head - index >= 1) return;
    const from = index + 1;
    const to = Math.min(item.pages - 1, index + forwardPreload + 2);
    pagerPrefetchRef.current = true;
    (async () => {
      try {
        for (let c = from; c <= to; c += 3) {
          await loadRange(c);
        }
      } finally {
        pagerPrefetchRef.current = false;
      }
    })();
  }, [index, item.pages, forwardPreload, loadRange]); // eslint-disable-line react-hooks/exhaustive-deps

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

  // Merge per-page orientation correction (rotate / mirror) into image styles.
  function adjStyleFor(adj: { rot: number; flip: boolean } | undefined): React.CSSProperties {
    if (!adj || (!adj.rot && !adj.flip)) return {};
    return { transform: `rotate(${adj.rot}deg)${adj.flip ? " scaleX(-1)" : ""}`, transformOrigin: "center" };
  }

  return (
    <div className={fullscreen ? "reader reader-fullscreen" : "reader"}>
      <div
        className={"reader-toolbar" + (barHidden ? " reader-toolbar-hidden" : "")}
        onMouseEnter={() => setBarHidden(false)}
      >
        <button onClick={onBack}>← 退出</button>
        <b>{index + 1} / {item.pages}</b>
        <input
          className={"reader-slider" + (vertical ? " reader-slider-cont" : "")}
          type="range"
          min={0}
          max={vertical ? 1000 : Math.max(0, item.pages - 1)}
          value={vertical ? Math.round(norm * 1000) : index}
          onInput={(e) => {
            const v = Number(e.currentTarget.value);
            if (vertical) {
              isSeekingRef.current = true;
              const k = Math.min(1, Math.max(0, v / 1000));
              setNorm(k);
              applyNorm(k);
            } else {
              goto(v);
            }
          }}
          onPointerUp={() => {
            isSeekingRef.current = false;
          }}
          onPointerCancel={() => {
            isSeekingRef.current = false;
          }}
          aria-label={vertical ? "连续定位" : "跳转页码"}
        />
        <select value={direction} onChange={onDirectionChange}>
          {Object.entries(PORTS).map(([v, l]) => (
            <option key={v} value={v}>
              {l}
            </option>
          ))}
        </select>
        <select value={zoom} onChange={onZoomChange}>
          <option value="original">原尺寸</option>
          <option value="fit_width">适配宽</option>
          <option value="fit_height">适配高</option>
          <option value="fit_screen">适配屏幕</option>
          <option value="custom">固定缩放</option>
        </select>
        {zoom === "custom" && (
          <label className="reader-scale">
            <input
              type="range"
              min={20}
              max={500}
              value={scale}
              onChange={(e) => setScale(Number(e.currentTarget.value))}
            />
            {scale}%
          </label>
        )}
        <button onClick={toggleFs}>{fullscreen ? "退出全屏" : "全屏"}</button>
        {status === "error" && <span className="reader-status">{STATUS_TEXT[status]}</span>}
      </div>

      <div
        ref={listRef}
        className={"reader-viewport" + (vertical ? " reader-vertical" : "")}
        onScroll={onScroll}
        onWheel={onWheel}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
      >
        {vertical ? (
          <>
            {verticalPages.map(({ i, state }) =>
              state === "loaded" ? (
                <img
                  key={i}
                  data-index={i}
                  src={images[i]}
                  alt={`第 ${i + 1} 页`}
                  className="reader-img-v"
                  style={{ ...imgStyleFor(zoom), ...adjStyleFor(adjMap[i]) }}
                />
              ) : state === "failed" ? (
                <div key={i} data-index={i} className="reader-fail-block">
                  <p>第 {i + 1} 页加载失败</p>
                  <button onClick={retry}>重试</button>
                </div>
              ) : (
                <LoadingIndicator
                  key={i}
                  className="reader-loading-block"
                  page={i}
                  percent={progress[i]}
                  label={`第 ${i + 1} 页加载中`}
                />
              ),
            )}
            <div ref={sentinelRef} className="reader-sentinel" aria-hidden="true" />
          </>
        ) : contentImg ? (
          <img src={contentImg} alt={`第 ${index + 1} 页`} className="reader-img" style={{ ...imgStyleFor(zoom), ...adjStyleFor(adjMap[index]) }} />
        ) : failed.has(index) ? (
          <div className="reader-fail">
            <p>图片加载失败（该页可能受限或网络异常）</p>
            <button onClick={retry}>重试</button>
          </div>
        ) : (
          <LoadingIndicator className="reader-loading" percent={progress[index]} />
        )}
      </div>
      {vertical && (
        <div className="reader-thumbstrip">
          <button
            className="reader-thumb-toggle"
            onClick={() => setThumbOpen((o) => !o)}
            title={thumbOpen ? "收起缩略图" : "展开缩略图"}
          >
            {thumbOpen ? "›" : "‹"}
          </button>
          {thumbOpen && (
            <div className="reader-thumb-list">
              {Array.from({ length: item.pages }, (_, i) => (
                <button
                  key={i}
                  className={"reader-thumb" + (i === index ? " reader-thumb-cur" : "")}
                  title={`第 ${i + 1} 页`}
                  onMouseEnter={() => {
                    if (!images[i] && !failed.has(i)) loadRange(i);
                  }}
                  onClick={() => goto(i)}
                >
                  {images[i] ? <img src={images[i]} alt="" loading="lazy" /> : <span>{i + 1}</span>}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
      <div className="reader-progress">
        <div style={{ width: `${(vertical ? norm : (index + 1) / item.pages) * 100}%` }} />
        {vertical && <span className="reader-progress-label">{Math.round(norm * 100)}%</span>}
      </div>
    </div>
  );
}
