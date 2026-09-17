import { Channel, invoke } from "@tauri-apps/api/core";

const imgCache = new Map<string, Promise<string>>();
/** Cap on cached image data-URL promises (LRU). Keeps rendered thumbnails warm
 * while bounding memory; evicted URLs re-fetch through the backend's disk cache. */
const MAX_IMG_CACHE = 40;

function cachePut(url: string, value: Promise<string>): void {
  // Delete first so re-fetches move to the most-recently-used end.
  imgCache.delete(url);
  imgCache.set(url, value);
  if (imgCache.size > MAX_IMG_CACHE) {
    const oldest = imgCache.keys().next().value as string | undefined;
    if (oldest !== undefined) imgCache.delete(oldest);
  }
}

interface ProgressPacket {
  received: number;
  total: number;
}

function errMessage(e: unknown): string {
  if (typeof e === "string" && e.trim()) return e;
  if (e instanceof Error && e.message) return e.message;
  return "图片加载失败";
}

/**
 * Fetches an image through the Rust backend (proper UA/Referer/cookies) and
 * returns a `data:` URL. Cached per target so repeated renders don't re-fetch.
 * When `onProgress` is given, streams `0..1` fraction while the body downloads
 * (`total == 0` on the backend means unknown length, so no progress events fire
 * and the caller stays indeterminate). Mirrors SXJ's reader page progress.
 *
 * On failure the promise REJECTS with a user-facing message (never returns an
 * empty string) so callers can render a diagnosable error/retry state; the cache
 * entry is dropped so a later retry re-invokes the backend. Avoids the custom
 * `ehimg://` scheme, which is unreliable in the packaged app.
 */
export function remoteImage(url: string, onProgress?: (percent: number) => void): Promise<string> {
  const cached = imgCache.get(url);
  if (cached) {
    onProgress?.(1);
    return cached;
  }
  const req = onProgress ? invokeWithProgress(url, onProgress) : invoke<string>("fetch_image", { url });
  const p = req.catch((e) => {
    imgCache.delete(url);
    const msg = errMessage(e);
    console.error("fetch_image failed:", url, msg);
    throw new Error(msg);
  });
  cachePut(url, p);
  return p;
}

/** Streams image download progress over a Tauri channel into `onProgress`. */
function invokeWithProgress(
  url: string,
  onProgress: (percent: number) => void,
): Promise<string> {
  const channel = new Channel<ProgressPacket>();
  channel.onmessage = (packet) => {
    if (packet.total > 0) {
      onProgress(Math.min(1, packet.received / packet.total));
    }
  };
  return invoke<string>("fetch_image_progress", { url, onEvent: channel });
}
