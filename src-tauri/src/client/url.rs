//! Port of SXJ `EhUrl`: host / reference / url construction for the two sites.

use std::sync::{OnceLock, RwLock};

pub const SITE_E: u8 = 0;
pub const SITE_EX: u8 = 1;

pub const DOMAIN_EX: &str = "exhentai.org";
pub const DOMAIN_E: &str = "e-hentai.org";

/// Optional custom host override (e.g. a self-hosted mirror). When set, every
/// site-specific URL is built on this host instead of `DOMAIN_EX`/`DOMAIN_E`.
static HOST_OVERRIDE: OnceLock<RwLock<Option<String>>> = OnceLock::new();

/// Serialises tests that build host-dependent URLs: the default Rust test
/// harness runs `#[test]` fns on multiple threads, and `set_host_override`
/// mutates a process-global. Shared with `client::list_url` tests.
/// Parses the per-page `/s/` token out of a preview item URL
/// (`https://<host>/s/<token>/<gid>-<page>`). Shared by the reader and download
/// engine; mirrors SXJ's `GalleryPageUrlParser` path shape.
pub fn page_token_from(preview_url: &str) -> Option<String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE
        .get_or_init(|| regex::Regex::new(r#"/s/([0-9a-f]{10})(?:/[^/]*)?/"#).unwrap());
    re.captures(preview_url).map(|c| c[1].to_string())
}


#[cfg(test)]
pub(crate) static URL_GLOBAL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Sets/clears the process-wide host override. A leading scheme and trailing
/// slashes are normalised; empty values restore the default E/EX host.
pub fn set_host_override(host: Option<String>) {
    let cleaned = host
        .map(|h| {
            h.trim()
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_matches('/')
                .trim()
                .to_string()
        })
        .filter(|h| !h.is_empty());
    let lock = HOST_OVERRIDE.get_or_init(|| RwLock::new(None));
    *lock.write().unwrap() = cleaned;
}

/// Returns the currently configured host override, if any.
pub fn host_override() -> Option<String> {
    HOST_OVERRIDE.get().and_then(|l| l.read().unwrap().clone())
}

/// Whether `host` belongs to the allowed EH site family (the two sites + image
/// CDN) or the configured custom host override (mirrors).
pub fn is_eh_host(host: &str) -> bool {
    let host = host.trim();
    for suffix in [DOMAIN_E, DOMAIN_EX, "ehgt.org"] {
        if host == suffix || host.ends_with(&format!(".{suffix}")) {
            return true;
        }
    }
    if let Some(oh) = host_override() {
        let oh = oh
            .trim()
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_matches('/');
        if oh == host || host.ends_with(&format!(".{oh}")) {
            return true;
        }
    }
    false
}

/// Whether `url` points at a host the client is allowed to contact with session
/// cookies and fetch (EH sites/CDN, or the configured custom host override).
pub fn is_allowed_url(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(is_eh_host))
        .unwrap_or(false)
}

/// Returns the active domain: the host override if set, else the default for `site`.
pub fn domain(site: u8) -> String {
    if let Some(h) = HOST_OVERRIDE.get().and_then(|l| l.read().unwrap().clone()) {
        return h;
    }
    match site {
        SITE_EX => DOMAIN_EX.into(),
        _ => DOMAIN_E.into(),
    }
}

pub fn referer(site: u8) -> String {
    format!("https://{}", domain(site))
}

pub fn host(site: u8) -> String {
    format!("{}/", referer(site))
}

pub fn api_url(site: u8) -> String {
    format!("{}api.php", host(site))
}

pub fn home_url(site: u8) -> String {
    format!("{}home.php", host(site))
}

pub fn popular_url(site: u8) -> String {
    format!("https://{}/popular", domain(site))
}

pub fn uconfig_url(site: u8) -> String {
    format!("{}uconfig.php", host(site))
}

/// Gallery detail URL, e.g. `https://e-hentai.org/g/<gid>/<token>/[?p=<index>][&hc=1]`.
pub fn gallery_detail_url(site: u8, gid: u64, token: &str, index: u32, all_comment: bool) -> String {
    let mut out = format!("{}g/{}/{}/", host(site), gid, token);
    let mut sep = '?';
    if index != 0 {
        out.push(sep);
        out.push_str(&format!("p={}", index));
        sep = '&';
    }
    if all_comment {
        out.push(sep);
        out.push_str("hc=1");
    }
    out
}

/// Per-image page URL: `https://<host>/s/<p_token>/<gid>-<index+1>`.
pub fn gallery_page_url(site: u8, gid: u64, index: u32, p_token: &str) -> String {
    format!("{}s/{}/{}-{}", host(site), p_token, gid, index + 1)
}

/// Original thumbnail URL prefix on E* (ehgt.org CDN).
pub const THUMB_PREFIX_E: &str = "https://ehgt.org/";
pub const THUMB_PREFIX_EX: &str = "https://e-hentai.org/t/";

pub fn thumb_url_prefix(_site: u8) -> String {
    // SXJ always returns the E prefix for thumbnails (ehgt CDN).
    THUMB_PREFIX_E.into()
}

/// Fix the gallery preview thumb url, ported from `getFixedPreviewThumbUrl`.
pub fn fixed_preview_thumb_url(origin: &str, site: u8) -> String {
    let Ok(parsed) = url::Url::parse(origin) else {
        return origin.into();
    };
    let segments: Vec<String> = parsed
        .path_segments()
        .map(|s| s.map(|v| v.to_owned()).collect())
        .unwrap_or_default();
    if segments.len() < 3 {
        return origin.into();
    }
    let last = segments.last().unwrap();
    let second_last = segments[segments.len() - 2].clone();
    let third_last = segments[segments.len() - 3].clone();
    if last.starts_with(&third_last)
        && last.as_str()[third_last.len()..].starts_with(&second_last)
    {
        format!("{}{}/{}/{}", thumb_url_prefix(site), third_last, second_last, last)
    } else {
        origin.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::MutexGuard;

    fn locked<T>(f: impl FnOnce() -> T) -> T {
        let _guard: MutexGuard<'_, ()> = URL_GLOBAL_LOCK.lock().unwrap();
        f()
    }

    #[test]
    fn detail_url_index() {
        locked(|| {
            let u = gallery_detail_url(SITE_E, 12345, "abcd", 2, false);
            assert_eq!(u, "https://e-hentai.org/g/12345/abcd/?p=2");
        });
    }

    #[test]
    fn detail_url_no_index() {
        locked(|| {
            let u = gallery_detail_url(SITE_E, 12345, "abcd", 0, false);
            assert_eq!(u, "https://e-hentai.org/g/12345/abcd/");
        });
    }

    #[test]
    fn page_url() {
        locked(|| {
            let u = gallery_page_url(SITE_E, 12345, 0, "ptoken");
            assert_eq!(u, "https://e-hentai.org/s/ptoken/12345-1");
        });
    }

    #[test]
    fn fixed_thumb_url() {
        locked(|| {
            let url = "https://ehgt.org/31/7a/317a1a254cd9c3269e71b2aa2671fe8d28c91097-260198-640-480-png.jpg";
            let got = fixed_preview_thumb_url(url, SITE_E);
            assert_eq!(got, url);
        });
    }

    #[test]
    fn host_override() {
        locked(|| {
            // Default hosts.
            assert_eq!(host(SITE_E), "https://e-hentai.org/");
            assert_eq!(host(SITE_EX), "https://exhentai.org/");

            // Apply an override written like a settings value (scheme + trailing slash).
            set_host_override(Some("http://mirror.example.com/".into()));
            assert_eq!(referer(SITE_E), "https://mirror.example.com");
            assert_eq!(host(SITE_E), "https://mirror.example.com/");
            assert_eq!(host(SITE_EX), "https://mirror.example.com/");
            assert_eq!(popular_url(SITE_EX), "https://mirror.example.com/popular");
            assert_eq!(
                gallery_detail_url(SITE_E, 12345, "abcd", 0, false),
                "https://mirror.example.com/g/12345/abcd/"
            );

            // Empty/None restores the defaults.
            set_host_override(Some("".into()));
            assert_eq!(host(SITE_E), "https://e-hentai.org/");
            set_host_override(None);
            assert_eq!(host(SITE_EX), "https://exhentai.org/");
        });
    }

    #[test]
    fn page_token_strict() {
        let u = "https://e-hentai.org/s/1234567890/12345-1";
        assert_eq!(page_token_from(u), Some("1234567890".into()));
        let u2 = "https://e-hentai.org/s/abcdef0123/abc/12345-1/";
        assert_eq!(page_token_from(u2), Some("abcdef0123".into()));
        assert_eq!(page_token_from("https://e-hentai.org/g/12345/abcd/"), None);
        // Too short: only 5 hex chars.
        assert_eq!(page_token_from("https://e-hentai.org/s/12345/12345-1"), None);
        // Not hex.
        assert_eq!(page_token_from("https://e-hentai.org/s/ZZZZZZZZZZ/12345-1"), None);
    }

    #[test]
    fn host_allowlist() {
        locked(|| {
            set_host_override(None);
            assert!(is_allowed_url("https://e-hentai.org/g/1/2/"));
            assert!(is_allowed_url("https://ht0.ehgt.org/31/7a/x.jpg"));
            assert!(is_allowed_url("https://exhentai.org/"));
            assert!(!is_allowed_url("https://evil.example.com/x"));

            // A configured custom-host mirror stays allowed.
            set_host_override(Some("mirror.example.com".into()));
            assert!(is_allowed_url("https://mirror.example.com/g/1/2/"));

            set_host_override(None);
        });
    }
}
