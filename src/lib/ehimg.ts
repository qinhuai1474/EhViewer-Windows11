import { invoke } from "@tauri-apps/api/core";

const imgCache = new Map<string, Promise<string>>();

/**
 * Fetches an image through the Rust backend (proper UA/Referer/cookies) and
 * returns a `data:` URL. Cached per target so repeated renders don't re-fetch.
 * Avoids the custom `ehimg://` scheme, which is unreliable in the packaged app.
 */
export function remoteImage(url: string): Promise<string> {
  let p = imgCache.get(url);
  if (!p) {
    p = invoke<string>("fetch_image", { url }).catch((e) => {
      imgCache.delete(url);
      console.error("fetch_image failed:", url, e);
      return "";
    });
    imgCache.set(url, p);
  }
  return p;
}
