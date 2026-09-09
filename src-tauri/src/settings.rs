//! JSON settings file persistence (mirrors SXJ `Settings` for the desktop client).
//!
//! Session cookies are encrypted with Windows DPAPI before being written to disk;
//! every other field is stored as plain JSON.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::client::config::EhConfig;

/// Prefix marking a DPAPI-encrypted cookie value in the JSON file.
pub const DPAPI_PREFIX: &str = "dpapi:";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Gallery site: 0 = E-Hentai, 1 = ExHentai.
    pub site: u8,
    /// The active EH uconfig group.
    pub config: EhConfig,
    /// Session / member cookies that unlock galleries (values plaintext in memory,
    /// DPAPI-encrypted bytes on disk, see `save`/`load`).
    pub session_cookies: Vec<CookiePair>,
    /// Whether tag translation is enabled.
    pub tag_translation_enabled: bool,
    /// Path of the imported EhTagDatabase binary (None = not imported).
    pub tag_translation_file: Option<String>,
    /// Thumbnail resolution: 0 = default, 1 = 250, 2 = 300.
    pub thumb_resolution: u8,

    // ----- reading -----
    /// Reading direction: ltr / rtl / vertical.
    pub reading_direction: String,
    /// Zoom mode: original / fit_width / fit_height / fit_screen / custom.
    pub zoom_mode: String,
    /// Where the reader starts: first / resume.
    pub reading_start_position: String,

    // ----- download -----
    /// Download directory override.
    pub download_dir: Option<String>,
    pub download_threads: u32,
    /// Always fetch the original (full-size) image rather than the auto-scaled one.
    pub download_always_original: bool,
    /// Downloads page size.
    pub download_list_page_size: u32,
    /// Seconds to wait between consecutive queue downloads (0 = no wait).
    pub download_interval_secs: u64,

    // ----- advanced -----
    /// Proxy type: 0 = direct, 1 = system, 2 = HTTP, 3 = SOCKS5 (mirrors SXJ).
    pub proxy_type: u8,
    /// Proxy address `host:port` (used when `proxy_type` is HTTP or SOCKS).
    pub proxy_url: Option<String>,
    /// Multi-line user host override table (`host ip[,ip...]`), SXJ `Hosts`.
    pub hosts_override: String,
    /// Per-request timeout in seconds.
    pub timeout_secs: u64,
    /// Download retry count for transient failures (509 / timeout).
    pub max_retries: u32,
    /// `ehimg://` disk cache size hint in MB (documented; enforced on the backend).
    pub image_cache_size_mb: u32,
    /// Optional custom EH host override (for self-hosted / mirror setups).
    pub custom_host: Option<String>,
    /// DNS-over-HTTPS resolver host (empty = default AliDNS endpoint).
    pub doh_url: String,
    /// Prefer the bundled E-Hentai IP table before DoH resolution.
    pub use_builtin_hosts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookiePair {
    pub name: String,
    pub value: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            site: 0,
            config: EhConfig::default(),
            session_cookies: Vec::new(),
            tag_translation_enabled: false,
            tag_translation_file: None,
            thumb_resolution: 0,
            reading_direction: "ltr".into(),
            zoom_mode: "fit_width".into(),
            reading_start_position: "resume".into(),
            download_dir: None,
            download_threads: 3,
            download_always_original: false,
            download_list_page_size: 12,
            download_interval_secs: 5,
            proxy_type: 0,
            proxy_url: None,
            hosts_override: String::new(),
            timeout_secs: 30,
            max_retries: 3,
            image_cache_size_mb: 100,
            custom_host: None,
            doh_url: String::new(),
            use_builtin_hosts: true,
        }
    }
}

impl Settings {
    /// Path of the settings JSON under the app config dir.
    pub fn default_path(config_dir: &std::path::Path) -> PathBuf {
        config_dir.join("settings.json")
    }

    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path)?;
        let mut settings: Settings = serde_json::from_str(&raw).unwrap_or_default();
        settings.decrypt_cookies();
        Ok(settings)
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let mut clone = self.clone();
        clone.encrypt_cookies();
        // Never leave encrypted bytes if a cookie is absent or already handled.
        let raw = serde_json::to_string_pretty(&clone)?;
        std::fs::write(path, raw)?;
        Ok(())
    }

    /// Cookies are held plaintext in memory; swizzle to DPAPI for disk writes.
    fn encrypt_cookies(&mut self) {
        for c in &mut self.session_cookies {
            c.value = encrypt_cookie_value(&c.value);
        }
    }

    fn decrypt_cookies(&mut self) {
        for c in &mut self.session_cookies {
            c.value = decrypt_cookie_value(&c.value);
        }
    }
}

/// Encrypts a cookie value with DPAPI (idempotent for already-encrypted values).
pub fn encrypt_cookie_value(value: &str) -> String {
    if value.starts_with(DPAPI_PREFIX) {
        return value.to_string();
    }
    match crate::dpapi::protect(value.as_bytes()) {
        Ok(enc) => format!("{}{}", DPAPI_PREFIX, B64.encode(enc)),
        Err(_) => value.to_string(), // fall back to plaintext if DPAPI is unavailable
    }
}

/// Decrypts a DPAPI-encrypted cookie value; legacy plaintext passes through.
pub fn decrypt_cookie_value(value: &str) -> String {
    match value.strip_prefix(DPAPI_PREFIX) {
        Some(b64) => {
            let Ok(bytes) = B64.decode(b64) else {
                return value.to_string();
            };
            crate::dpapi::unprotect(&bytes)
                .map(|raw| String::from_utf8_lossy(&raw).into_owned())
                .unwrap_or_else(|_| value.to_string())
        }
        None => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_encrypt_decrypt_roundtrip() {
        let raw = "ipb_pass_hash=abc123";
        let enc = encrypt_cookie_value(raw);
        // Idempotent: encrypting an already-encrypted value is a no-op.
        assert_eq!(encrypt_cookie_value(&enc), enc);
        // Decrypt always restores the original value (DPAPI path or plaintext fallback).
        assert_eq!(decrypt_cookie_value(&enc), raw);
    }

    #[test]
    fn legacy_plaintext_passes_through() {
        assert_eq!(decrypt_cookie_value("plain"), "plain");
    }

    #[test]
    fn save_load_persists_fields_and_cookies() {
        let dir = std::env::temp_dir().join(format!("ehv-set-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let mut s = Settings::default();
        s.site = 1;
        s.session_cookies = vec![CookiePair { name: "igneous".into(), value: "xyz".into() }];
        s.download_threads = 7;
        s.download_always_original = true;
        s.save(&path).unwrap();
        let loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.site, 1);
        assert_eq!(loaded.session_cookies[0].value, "xyz");
        assert_eq!(loaded.download_threads, 7);
        assert!(loaded.download_always_original);
        std::fs::remove_dir_all(&dir).ok();
    }
}
