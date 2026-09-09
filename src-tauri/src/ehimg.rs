//! `ehimg://` custom protocol: serves gallery images through Rust so the webview
//! never hits CORS / Referer limits. Backend fetches with correct Referer+Cookie
//! and caches images on disk.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use http::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderValue};
use http::{Response, StatusCode};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::client::client;

const SCHEME: &str = "ehimg";

static CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
/// Disk-cache size cap in bytes (0 = unlimited). Mirrors `image_cache_size_mb`.
static MAX_BYTES: OnceLock<AtomicU64> = OnceLock::new();

/// Set the disk-cache directory. Called during setup.
pub fn set_cache_dir(dir: PathBuf) {
    let _ = CACHE_DIR.set(dir.join("ehimg"));
}

/// Sets the cache cap in bytes (0 = unlimited). Called at startup and whenever
/// the "缓存大小" setting changes so the cap takes effect immediately.
pub fn set_cache_max_bytes(bytes: u64) {
    let slot = MAX_BYTES.get_or_init(|| AtomicU64::new(0));
    slot.store(bytes, Ordering::Relaxed);
}

/// `image_cache_size_mb` -> bytes; `set_cache_max_mb(0)` disables the cap.
pub fn set_cache_max_mb(mb: u32) {
    set_cache_max_bytes(mb as u64 * 1024 * 1024);
}

/// Register the custom protocol on the builder (custom protocols are builder-level in Tauri v2).
pub fn register<R: tauri::Runtime>(
    builder: tauri::Builder<R>,
) -> tauri::Builder<R> {
    builder.register_uri_scheme_protocol(SCHEME, |_ctx, request| {
        let uri = request.uri().clone();
        let target = decode_target(&uri.path());
        match target {
            None => err_response("invalid ehimg uri"),
            Some(url) => runtime()
                .block_on(serve(&cache_dir(), &url))
                .unwrap_or_else(|_| err_response("server error")),
        }
    })
}

/// Shared multi-thread runtime used to drive the async fetch from the sync handler.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Runtime::new().expect("tokio runtime"))
}

fn cache_dir() -> PathBuf {
    CACHE_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| std::env::temp_dir().join("ehviewer-ehimg"))
}

/// The frontend sends `ehimg://i/<base64url(target)>`.
fn decode_target(path: &str) -> Option<String> {
    let b64 = path.trim_start_matches('/');
    if b64.is_empty() {
        return None;
    }
    let bytes = B64.decode(b64).ok()?;
    String::from_utf8(bytes).ok()
}

fn err_response(msg: &str) -> Response<std::borrow::Cow<'static, [u8]>> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(msg.as_bytes().to_vec().into())
        .unwrap()
}

async fn load_bytes(cache_dir: &Path, url: &str) -> Vec<u8> {
    if !crate::client::url::is_allowed_url(url) {
        return Vec::new();
    }
    let key = cache_key(url);
    let ext = guess_ext(url);
    let file = cache_dir.join(format!("{key}.{ext}"));
    if file.exists() {
        return std::fs::read(&file).unwrap_or_default();
    }
    match client::get_bytes(url, None).await {
        Ok(bytes) => {
            let _ = std::fs::write(&file, &bytes);
            enforce_cache_cap(cache_dir);
            bytes
        }
        Err(_) => Vec::new(),
    }
}

/// Fetches (and caches) an image and returns a `data:` URL. Used as a command so
/// the frontend can render images without depending on the custom `ehimg://`
/// scheme, which is unreliable in the packaged WebView2.
pub async fn data_url(url: &str) -> Result<String, String> {
    if !crate::client::url::is_allowed_url(url) {
        return Err("图片地址不在允许的站点列表内".to_string());
    }
    let body = load_bytes(&cache_dir(), url).await;
    if body.is_empty() {
        return Err("图片获取失败（网络或已失效）".to_string());
    }
    let ctype = content_type(guess_ext(url));
    let b64 = base64::engine::general_purpose::STANDARD.encode(body);
    Ok(format!("data:{ctype};base64,{b64}"))
}

async fn serve(cache_dir: &Path, url: &str) -> Result<Response<std::borrow::Cow<'static, [u8]>>, http::Error> {
    let ext = guess_ext(url);
    let ctype = content_type(ext);
    let body = load_bytes(cache_dir, url).await;

    if body.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .body(Vec::new().into());
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, HeaderValue::from_str(ctype).unwrap())
        .header(CACHE_CONTROL, HeaderValue::from_static("private, max-age=31536000, immutable"))
        .body(body.into())
}

fn cache_key(url: &str) -> String {
    // Stable hash: identical across builds/processes (DefaultHasher is not).
    let mut h = Sha256::new();
    h.update(url.as_bytes());
    format!("{:x}", h.finalize())
}

/// Evicts the oldest cache entries until the total stays under the configured
/// cap. A no-op when the cap is disabled or the directory is unreadable.
fn enforce_cache_cap(dir: &Path) {
    let cap = MAX_BYTES.get().map(|c| c.load(Ordering::Relaxed)).unwrap_or(0);
    if cap == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf, u64)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            let m = e.metadata().ok()?;
            Some((m.modified().unwrap_or(std::time::UNIX_EPOCH), p, m.len()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|(_, _, len)| len).sum();
    if total <= cap {
        return;
    }
    // Oldest first; drop entries until the directory fits under the cap.
    files.sort_by_key(|(t, _, _)| *t);
    for (_, path, len) in files {
        if total <= cap {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

fn guess_ext(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    for ext in ["jpg", "jpeg", "png", "webp", "gif", "bmp"] {
        if lower.contains(&format!(".{ext}")) {
            return if ext == "jpeg" { "jpg" } else { ext };
        }
    }
    "bin"
}

fn content_type(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

/// Builds an `ehimg://` URL for a target http(s) image URL, for use in `<img src=...>`.
pub fn to_ehimg_url(target: &str) -> String {
    let b64 = B64.encode(target.as_bytes());
    format!("ehimg://i/{b64}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let target = "https://ehgt.org/31/7a/sample-250.jpg?x=1&y=2";
        let img = to_ehimg_url(target);
        let decoded = decode_target(img.trim_start_matches("ehimg://i/")).unwrap();
        assert_eq!(decoded, target);
    }

    #[test]
    fn extension() {
        assert_eq!(guess_ext("https://x/1.jpeg"), "jpg");
        assert_eq!(guess_ext("https://x/2.webp"), "webp");
    }

    #[test]
    fn cache_cap_evicts_oldest_until_under() {
        use std::thread::sleep;
        use std::time::Duration;

        let dir = std::env::temp_dir().join(format!("ehimg-cap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // Distinct mtimes so eviction order is deterministic (newest survives).
        let f1 = dir.join("1.bin");
        let f2 = dir.join("2.bin");
        let f3 = dir.join("3.bin");
        std::fs::write(&f1, vec![0u8; 300]).unwrap();
        sleep(Duration::from_millis(20));
        std::fs::write(&f2, vec![0u8; 300]).unwrap();
        sleep(Duration::from_millis(20));
        std::fs::write(&f3, vec![0u8; 300]).unwrap();

        // 900 bytes total > 500 cap -> keep only the newest (~300 bytes).
        set_cache_max_bytes(500);
        enforce_cache_cap(&dir);
        assert!(!f1.exists() && !f2.exists(), "oldest files should be evicted");
        assert!(f3.exists(), "newest file should survive");
        let remaining: u64 = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok().map(|m| m.len()))
            .sum();
        assert!(remaining <= 500, "cache over cap: {remaining}");

        // Re-run must be idempotent / not panic when already under cap.
        enforce_cache_cap(&dir);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cache_cap_disabled_is_noop() {
        let dir = std::env::temp_dir().join(format!("ehimg-nocap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.bin");
        std::fs::write(&f, vec![0u8; 5000]).unwrap();
        set_cache_max_bytes(0);
        enforce_cache_cap(&dir);
        assert!(f.exists(), "no-cap must not evict");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cache_key_is_stable_and_unique() {
        let a = cache_key("https://ehgt.org/a.jpg");
        let b = cache_key("https://ehgt.org/a.jpg");
        let c2 = cache_key("https://ehgt.org/b.jpg");
        // 64 hex chars (SHA-256) so cache keys are stable across builds/processes.
        assert_eq!(a.len(), 64, "sha256 hex should be 64 chars");
        assert!(a.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_eq!(a, b, "same url must hash identically");
        assert_ne!(a, c2, "different urls must hash differently");
        // Known fixed vector proves cross-build determinism.
        assert_eq!(a, "0144283b959a835f4792d07583410462446c9fde328683f66405f287de83f26b");
    }
}
