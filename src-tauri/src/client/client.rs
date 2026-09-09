//! HTTP client singleton (port of SXJ `EhClient` + `EhRequestBuilder`).
//! Uses reqwest (rustls) with a persistent cookie store, plus per-request
//! Referer / Origin / uconfig headers so E-Hentai serves the right content.

use std::sync::atomic::{AtomicU8, Ordering};

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, ORIGIN, REFERER};

use super::dns;
use super::err::{EhError, EhResult};

pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

/// Which EH site is active (E or EX), mirroring Settings.getGallerySite().
static SITE: AtomicU8 = AtomicU8::new(0);

/// Mutable extra cookie pairs sent on every request (uconfig, session).
static EXTRA_COOKIES: OnceLock<std::sync::RwLock<Vec<(String, String)>>> = OnceLock::new();

/// Max automatic retries for transient list/detail GET failures (509/network).
static MAX_RETRIES: OnceLock<std::sync::RwLock<u32>> = OnceLock::new();

/// Sets the retry count (mirrors `Settings.max_retries`); applied on settings change.
pub fn set_max_retries(n: u32) {
    let lock = MAX_RETRIES.get_or_init(|| std::sync::RwLock::new(3));
    *lock.write().unwrap() = n;
}

fn max_retries() -> u32 {
    MAX_RETRIES.get().map(|l| *l.read().unwrap()).unwrap_or(3)
}

pub fn set_site(site: u8) {
    SITE.store(if site == 1 { 1 } else { 0 }, Ordering::Relaxed);
}

pub fn site() -> u8 {
    SITE.load(Ordering::Relaxed)
}

/// Replaces the extra cookie set (e.g. the `uconfig` value).
pub fn set_extra_cookies(pairs: Vec<(String, String)>) {
    let lock = EXTRA_COOKIES.get_or_init(|| std::sync::RwLock::new(Vec::new()));
    let mut w = lock.write().unwrap();
    *w = pairs;
}

/// Session cookies (ipb_member_id / ipb_pass_hash / igneous) that unlock galleries.
static SESSION_COOKIES: OnceLock<std::sync::RwLock<Vec<(String, String)>>> = OnceLock::new();

pub fn set_session_cookies(pairs: Vec<(String, String)>) {
    let lock = SESSION_COOKIES.get_or_init(|| std::sync::RwLock::new(Vec::new()));
    let mut w = lock.write().unwrap();
    *w = pairs;
}

fn extra_cookie_header() -> HeaderValue {
    let session = SESSION_COOKIES
        .get_or_init(|| std::sync::RwLock::new(Vec::new()));
    let session = session.read().unwrap();
    let extra = EXTRA_COOKIES.get_or_init(|| std::sync::RwLock::new(Vec::new()));
    let extra = extra.read().unwrap();
    let joined = session
        .iter()
        .chain(extra.iter())
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("; ");
    HeaderValue::from_str(&joined).unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn build_headers(url: &str, referer: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        reqwest::header::USER_AGENT,
        HeaderValue::from_static(USER_AGENT),
    );
    h.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        ),
    );
    h.insert(
        reqwest::header::ACCEPT_LANGUAGE,
        HeaderValue::from_static("zh-CN,zh;q=0.9,en;q=0.8"),
    );
    h.insert(
        reqwest::header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    );
    h.insert(
        reqwest::header::HeaderName::from_static("sec-fetch-site"),
        HeaderValue::from_static("same-origin"),
    );
    h.insert(
        reqwest::header::HeaderName::from_static("sec-fetch-mode"),
        HeaderValue::from_static("navigate"),
    );
    // Never forward session cookies / Referer to a host outside the allowed EH
    // site family (the two sites + CDN or the configured custom host override).
    if super::url::is_allowed_url(url) {
        if !referer.is_empty() {
            h.insert(REFERER, HeaderValue::from_str(referer).unwrap());
            h.insert(ORIGIN, HeaderValue::from_str(referer).unwrap());
        }
        let cookie = extra_cookie_header();
        if !cookie.is_empty() {
            h.insert(COOKIE, cookie);
        }
    }
    h
}

/// Proxy setting: `(type, addr)` where type is 0=direct, 1=system, 2=HTTP,
/// 3=SOCKS5 (mirrors SXJ `EhProxySelector`) and `addr` is `host:port`.
static PROXY: OnceLock<RwLock<(u8, Option<String>)>> = OnceLock::new();

/// The lazily-built client; set to `None` by `apply_proxy` to force a rebuild.
static CLIENT: OnceLock<Mutex<Option<Arc<reqwest::Client>>>> = OnceLock::new();

/// Sets/clears the proxy and invalidates the cached client so it is rebuilt
/// lazily on the next request (making settings changes take effect immediately).
/// Read-only snapshot of the current proxy address (used by diagnostics).
pub fn proxy_url() -> Option<String> {
    PROXY.get().and_then(|l| l.read().unwrap().1.clone())
}

pub fn apply_proxy(ty: u8, addr: Option<String>) {
    let cleaned = addr.filter(|p| !p.trim().is_empty());
    let lock = PROXY.get_or_init(|| RwLock::new((0, None)));
    *lock.write().unwrap() = (ty, cleaned);
    if let Some(slot) = CLIENT.get() {
        *slot.lock().unwrap() = None;
    }
}

/// Best-effort resolution of the Windows system proxy from the environment
/// (HTTPS_PROXY / HTTP_PROXY). Returns `None` when unset so the client falls
/// back to a direct connection.
fn system_proxy() -> Option<String> {
    for key in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        if let Ok(v) = std::env::var(key) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn proxy_http(addr: &str) -> String {
    if addr.contains("://") { addr.to_string() } else { format!("http://{addr}") }
}

fn proxy_socks(addr: &str) -> String {
    // `socks5h` resolves the host on the proxy side (avoids local DNS pollution).
    if addr.contains("://") { addr.to_string() } else { format!("socks5h://{addr}") }
}

/// Request timeouts (seconds) used when (re)building the client.
static CONNECT_TIMEOUT: OnceLock<RwLock<u64>> = OnceLock::new();
static REQUEST_TIMEOUT: OnceLock<RwLock<u64>> = OnceLock::new();

fn stored_timeout(slot: &OnceLock<RwLock<u64>>, default: u64) -> u64 {
    slot.get().map(|l| *l.read().unwrap()).unwrap_or(default)
}
fn connect_timeout() -> u64 { stored_timeout(&CONNECT_TIMEOUT, 15) }
fn request_timeout() -> u64 { stored_timeout(&REQUEST_TIMEOUT, 90) }

/// Applies DNS + timeout settings and forces the client to be rebuilt so the
/// new values take effect immediately (mirrors SXJ's network config flow).
pub fn apply_network(timeout_secs: u64, use_builtin: bool, doh: Option<String>) {
    dns::set_use_builtin(use_builtin);
    dns::set_doh_url(doh);
    let slot = CONNECT_TIMEOUT.get_or_init(|| RwLock::new(15));
    *slot.write().unwrap() = timeout_secs.min(15);
    let slot = REQUEST_TIMEOUT.get_or_init(|| RwLock::new(90));
    *slot.write().unwrap() = (timeout_secs * 3).max(60);
    if let Some(sl) = CLIENT.get() {
        *sl.lock().unwrap() = None;
    }
}

/// Drops the cached client so the next request re-resolves and reconnects
/// (used to escalate DNS resolution after a transient connect failure).
pub fn invalidate_client() {
    if let Some(sl) = CLIENT.get() {
        *sl.lock().unwrap() = None;
    }
}

/// Builds a fresh client honouring the current `PROXY`/timeout/DNS settings. The
/// default path keeps the loopback-friendly direct connection used by the
/// mock-server test. EH hosts are pinned to bundled/resolved IPs (SNI intact).
async fn build_client() -> reqwest::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .cookie_store(true)
        .connect_timeout(std::time::Duration::from_secs(connect_timeout()))
        .timeout(std::time::Duration::from_secs(request_timeout()));
    let (pty, paddr) = PROXY.get().map(|l| l.read().unwrap().clone()).unwrap_or((0u8, None));
    match pty {
        2 => match paddr {
            Some(a) => builder = builder.proxy(reqwest::Proxy::all(&proxy_http(&a))?),
            None => builder = builder.no_proxy(),
        },
        3 => match paddr {
            Some(a) => builder = builder.proxy(reqwest::Proxy::all(&proxy_socks(&a))?),
            None => builder = builder.no_proxy(),
        },
        1 => match system_proxy() {
            Some(u) => builder = builder.proxy(reqwest::Proxy::all(&u)?),
            None => builder = builder.no_proxy(),
        },
        _ => builder = builder.no_proxy(),
    }
    // Only pin EH hosts to resolved IPs after a transient failure (escalated
    // resolution); otherwise use the plain system DNS to avoid stale-pin issues.
    if dns::should_pin() {
        for host in dns::pinned_hosts() {
            let ips = dns::resolve(&host).await;
            for ip in ips {
                if ip.is_unspecified() {
                    continue;
                }
                builder = builder.resolve(&host, SocketAddr::new(ip, 443));
            }
        }
    }
    builder.build()
}

async fn client() -> Arc<reqwest::Client> {
    // Read the cached client without holding the lock across any await.
    {
        let slot = CLIENT.get_or_init(|| Mutex::new(None));
        let guard = slot.lock().unwrap();
        if let Some(cached) = guard.as_ref() {
            return Arc::clone(cached);
        }
    }
    let built = match build_client().await {
        Ok(c) => Arc::new(c),
        // A malformed proxy must never brick the singleton: fall back to a
        // direct connection (the bad setting is cached as-is and will be
        // repaired the next time the client is rebuilt).
        Err(_) => Arc::new(
            reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .cookie_store(true)
                .no_proxy()
                .build()
                .expect("failed to build HTTP client"),
        ),
    };
    let slot = CLIENT.get_or_init(|| Mutex::new(None));
    let mut guard = slot.lock().unwrap();
    *guard = Some(Arc::clone(&built));
    built
}

/// Default referer for the active site.
pub fn default_referer() -> String {
    super::url::referer(site())
}

/// Exponential backoff with ±25% jitter for transient retries.
fn backoff_delay(attempt: u64) -> std::time::Duration {
    let base_ms = 800u64.saturating_mul(1u64 << attempt.min(6));
    let jitter = base_ms / 4;
    let low = base_ms.saturating_sub(jitter);
    let span = jitter.max(1).saturating_mul(2).saturating_add(1);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let offset = (nanos ^ attempt.wrapping_mul(0x9E3779B97F4A7C15)) % span;
    std::time::Duration::from_millis(low + offset)
}

/// Converts an HTTP status into an Err for the transient codes we retry.
fn check_status(status: reqwest::StatusCode) -> EhResult<()> {
    match status.as_u16() {
        509 => Err(EhError::BandwidthExceeded),
        429 | 502 | 503 | 504 => Err(EhError::Network(format!(
            "server returned HTTP {}",
            status.as_u16()
        ))),
        _ => Ok(()),
    }
}

/// GET a URL and return its body as text, classifying known EH errors. Transient
/// failures (509 / network) are retried a few times with jittered backoff so a
/// bandwidth hiccup doesn't immediately fail list/detail loads.
pub async fn get_text(url: &str, referer: Option<&str>) -> EhResult<String> {
    let max = max_retries();
    let mut attempt = 0u32;
    loop {
        match get_text_once(url, referer).await {
            Ok(body) => return Ok(body),
            Err(e) if e.is_transient() && attempt < max => {
                attempt += 1;
                // Escalate DNS resolution (DoH + system DNS) and force a fresh
                // client build so a stale/blocked pinned IP doesn't keep failing.
                dns::set_allow_network_resolve(true);
                invalidate_client();
                tokio::time::sleep(backoff_delay(attempt as u64)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

async fn get_text_once(url: &str, referer: Option<&str>) -> EhResult<String> {
    let headers = build_headers(url, referer.map(str::to_owned).unwrap_or_else(default_referer).as_str());
    let resp = client().await
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| EhError::Network(e.to_string()))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| EhError::Network(e.to_string()))?;

    check_status(status)?;
    if let Some(err) = EhError::classify(&body) {
        return Err(err);
    }
    Ok(body)
}

/// GET binary bytes (used for images / downloads). Mirrors `get_text`: transient
/// network failures (Cloudflare resets, etc.) are retried so a single hiccup
/// doesn't break thumbnails/images.
pub async fn get_bytes(url: &str, referer: Option<&str>) -> EhResult<Vec<u8>> {
    let max = max_retries();
    let mut attempt = 0u32;
    loop {
        match get_bytes_once(url, referer).await {
            Ok(bytes) => return Ok(bytes),
            Err(e) if e.is_transient() && attempt < max => {
                attempt += 1;
                tokio::time::sleep(backoff_delay(attempt as u64)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

async fn get_bytes_once(url: &str, referer: Option<&str>) -> EhResult<Vec<u8>> {
    let headers = build_headers(url, referer.map(str::to_owned).unwrap_or_else(default_referer).as_str());
    let resp = client().await
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| EhError::Network(e.to_string()))?;
    let status = resp.status();
    check_status(status)?;
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| EhError::Network(e.to_string()))?;
    Ok(bytes.to_vec())
}

/// POST a JSON body (e.g. `api.php` gdata/showpage) to `url` and return text.
pub async fn post_json(url: &str, body: &serde_json::Value) -> EhResult<String> {
    let headers = build_headers(url, &default_referer());
    let resp = client().await
        .post(url)
        .headers(headers)
        .json(body)
        .send()
        .await
        .map_err(|e| EhError::Network(e.to_string()))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| EhError::Network(e.to_string()))?;
    check_status(status)?;
    if let Some(err) = EhError::classify(&text) {
        return Err(err);
    }
    Ok(text)
}




#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::MutexGuard;

    static PROXY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    async fn locked_async<F>(f: F) -> F::Output
    where
        F: std::future::Future,
    {
        let _guard: MutexGuard<'_, ()> = PROXY_LOCK.lock().unwrap();
        f.await
    }

    #[tokio::test]
    async fn build_client_direct_ok() {
        locked_async(async {
            apply_proxy(0, None);
            apply_network(30, true, None);
            assert!(build_client().await.is_ok());
        })
        .await;
    }

    #[tokio::test]
    async fn build_client_with_proxy_ok() {
        locked_async(async {
            apply_proxy(2, Some("127.0.0.1:9999".into()));
            assert!(build_client().await.is_ok());
            apply_proxy(0, None);
        })
        .await;
    }

    #[tokio::test]
    async fn build_client_invalid_proxy_errors() {
        locked_async(async {
            apply_proxy(2, Some("::not a url::".into()));
            assert!(build_client().await.is_err());
            apply_proxy(0, None);
        })
        .await;
    }

    #[tokio::test]
    async fn build_client_socks_ok() {
        locked_async(async {
            apply_proxy(3, Some("127.0.0.1:1080".into()));
            assert!(build_client().await.is_ok());
            apply_proxy(0, None);
        })
        .await;
    }

    /// Regression: the UA string must be the header VALUE under the real
    /// `User-Agent` key, never used as a header NAME (which Cloudflare rejects).
    #[test]
    fn build_headers_sets_real_user_agent() {
        let h = build_headers("https://e-hentai.org", "https://e-hentai.org");
        assert_eq!(
            h.get(reqwest::header::USER_AGENT).and_then(|v| v.to_str().ok()),
            Some(USER_AGENT)
        );
        // The UA string must not appear as a header name.
        assert!(!h.contains_key(USER_AGENT), "UA string must not be a header name");
        // referer/origin present.
        assert_eq!(
            h.get(reqwest::header::REFERER).and_then(|v| v.to_str().ok()),
            Some("https://e-hentai.org")
        );
    }

    #[test]
    fn build_headers_omits_cookies_for_foreign_hosts() {
        set_session_cookies(vec![("ipb_member_id".to_string(), "s3cr3t".to_string())]);
        let h = build_headers("https://img.evil.example.com/x.jpg", "https://e-hentai.org");
        // Credentials must never be forwarded to a host outside the EH site family.
        assert!(!h.contains_key(reqwest::header::COOKIE));
        assert!(!h.contains_key(reqwest::header::REFERER));
        assert!(h.contains_key(reqwest::header::USER_AGENT));
        // Reset the shared global so parallel tests are unaffected.
        set_session_cookies(vec![]);
    }
}
