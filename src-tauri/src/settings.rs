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
pub const CURRENT_SETTINGS_VERSION: u32 = 2;

fn legacy_settings_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Internal settings schema version. Old files without this field are v1.
    #[serde(default = "legacy_settings_version")]
    pub settings_version: u32,
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
    /// Vertical reader pages prefetched ahead of the current one (0-20).
    pub reading_vertical_preload: u32,

    // ----- download -----
    /// Download directory override.
    pub download_dir: Option<String>,
    pub download_threads: u32,
    /// Always fetch the original (full-size) image rather than the auto-scaled one.
    pub download_always_original: bool,
    /// Downloads page size.
    pub download_list_page_size: u32,
    /// Show each download cover thumbnail on the Downloads page.
    pub download_show_thumb: bool,
    /// Move finished downloads from the default label to the "已完成" label.
    pub download_archive_completed: bool,
    /// Seconds to wait between consecutive queue downloads (0 = no wait).
    pub download_interval_secs: u64,
    /// User-defined tags stripped when renaming gallery folders. Empty = the
    /// built-in default list (see `download::naming::default_filter_terms`).
    pub rename_filter_terms: Vec<String>,
    /// Fixed folder scanned by the gallery rename tool. Empty = the configured
    /// download root. Never falls back to or touches anything outside this path.
    pub rename_scan_dir: String,

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
    /// Extra image CDN/host allowlist (comma/newline separated), trusted by
    /// `fetch_image` in addition to the EH site family. Mirrors/custom mirrors.
    pub image_host_allowlist: String,
    /// DNS-over-HTTPS resolver host (empty = default AliDNS endpoint).
    pub doh_url: String,
    /// Prefer the bundled E-Hentai IP table before DoH resolution.
    pub use_builtin_hosts: bool,
    /// Close the main window to the system tray instead of quitting.
    pub close_to_tray: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookiePair {
    pub name: String,
    pub value: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            settings_version: CURRENT_SETTINGS_VERSION,
            site: 0,
            config: EhConfig::default(),
            session_cookies: Vec::new(),
            tag_translation_enabled: false,
            tag_translation_file: None,
            thumb_resolution: 0,
            reading_direction: "ltr".into(),
            zoom_mode: "fit_width".into(),
            reading_start_position: "resume".into(),
            reading_vertical_preload: 20,
            download_dir: None,
            download_threads: 3,
            download_always_original: false,
            download_list_page_size: 9,
            download_show_thumb: false,
            download_archive_completed: false,
            download_interval_secs: 5,
            rename_filter_terms: Vec::new(),
            rename_scan_dir: String::new(),
            proxy_type: 0,
            proxy_url: None,
            hosts_override: String::new(),
            timeout_secs: 30,
            max_retries: 3,
            image_cache_size_mb: 100,
            custom_host: None,
            image_host_allowlist: String::new(),
            doh_url: String::new(),
            use_builtin_hosts: true,
            close_to_tray: true,
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
        let plaintext_cookies = settings.session_cookies.iter().any(|c| {
            !c.value.is_empty() && !c.value.starts_with(DPAPI_PREFIX)
        });
        settings.decrypt_cookies();
        let migrated = settings.apply_migrations();
        if migrated || plaintext_cookies {
            // Rewriting legacy plaintext cookies is best-effort during load so a
            // DPAPI failure does not discard otherwise valid settings. New writes
            // go through `save`, which fails closed instead of storing plaintext.
            if let Err(e) = settings.save(path) {
                log::warn!("failed to secure settings file during load: {e}");
            }
        }
        Ok(settings)
    }

    /// Applies non-destructive schema/default migrations and returns whether the
    /// settings should be written back. The page-size migration runs only once,
    /// so deliberately changing it back to 12 later remains respected.
    pub(crate) fn apply_migrations(&mut self) -> bool {
        let mut changed = false;
        if self.settings_version < CURRENT_SETTINGS_VERSION {
            if self.download_list_page_size == 12 {
                self.download_list_page_size = 9;
            }
            self.settings_version = CURRENT_SETTINGS_VERSION;
            changed = true;
        }
        if self.download_list_page_size == 0 {
            self.download_list_page_size = 9;
            changed = true;
        }
        changed
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let mut clone = self.clone();
        clone.encrypt_cookies()?;
        // Never leave encrypted bytes if a cookie is absent or already handled.
        let raw = serde_json::to_string_pretty(&clone)?;
        std::fs::write(path, raw)?;
        Ok(())
    }

    /// Cookies are held plaintext in memory; swizzle to DPAPI for disk writes.
    fn encrypt_cookies(&mut self) -> anyhow::Result<()> {
        for c in &mut self.session_cookies {
            c.value = encrypt_cookie_value(&c.value)?;
        }
        Ok(())
    }

    fn decrypt_cookies(&mut self) {
        for c in &mut self.session_cookies {
            c.value = decrypt_cookie_value(&c.value);
        }
    }
}

/// Encrypts a cookie value with DPAPI (idempotent for already-encrypted values).
/// Fails closed: a persistence operation must never silently store a plaintext
/// E-Hentai session credential.
pub fn encrypt_cookie_value(value: &str) -> anyhow::Result<String> {
    if value.starts_with(DPAPI_PREFIX) {
        return Ok(value.to_string());
    }
    match crate::dpapi::protect(value.as_bytes()) {
        Ok(enc) => Ok(format!("{}{}", DPAPI_PREFIX, B64.encode(enc))),
        Err(e) => anyhow::bail!("Windows DPAPI 加密失败，已拒绝保存明文 Cookie: {e}"),
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
    fn close_to_tray_defaults_to_enabled() {
        let s = Settings::default();
        assert!(s.close_to_tray, "closing to tray should default to on");
        assert_eq!(s.download_list_page_size, 9, "download page defaults to 9 items");
        assert!(!s.download_archive_completed, "auto-archive defaults to off");
    }

    #[test]
    fn cookie_encrypt_decrypt_roundtrip() {
        let raw = "ipb_pass_hash=abc123";
        let enc = encrypt_cookie_value(raw).unwrap();
        // Idempotent: encrypting an already-encrypted value is a no-op.
        assert_eq!(encrypt_cookie_value(&enc).unwrap(), enc);
        // Decrypt always restores the original value (DPAPI path or plaintext fallback).
        assert_eq!(decrypt_cookie_value(&enc), raw);
    }

    #[test]
    fn legacy_plaintext_passes_through() {
        assert_eq!(decrypt_cookie_value("plain"), "plain");
    }

    #[test]
    fn legacy_plaintext_cookie_is_rewritten_with_dpapi_on_load() {
        let dir = std::env::temp_dir().join(format!("ehv-set-cookie-migrate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{"session_cookies":[{"name":"ipb_pass_hash","value":"legacy-secret"}]}"#,
        )
        .unwrap();

        let loaded = Settings::load(&path).unwrap();
        assert_eq!(loaded.session_cookies[0].value, "legacy-secret");
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains(DPAPI_PREFIX));
        assert!(!on_disk.contains("legacy-secret"));

        std::fs::remove_dir_all(&dir).ok();
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

    #[test]
    fn legacy_page_size_migrates_once() {
        let dir = std::env::temp_dir().join(format!("ehv-set-migrate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{"download_list_page_size":12,"download_threads":5}"#,
        )
        .unwrap();

        let migrated = Settings::load(&path).unwrap();
        assert_eq!(migrated.download_list_page_size, 9);
        assert_eq!(migrated.settings_version, CURRENT_SETTINGS_VERSION);

        let mut edited = migrated;
        edited.download_list_page_size = 12;
        edited.save(&path).unwrap();
        let reloaded = Settings::load(&path).unwrap();
        assert_eq!(reloaded.download_list_page_size, 12, "migration must only run once");

        std::fs::remove_dir_all(&dir).ok();
    }
}
