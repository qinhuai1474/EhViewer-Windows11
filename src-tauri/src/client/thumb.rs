//! Thumbnail resolution rewrite (port of SXJ `EhUtils.handleThumbUrlResolution`).
//!
//! EH thumbnail URLs end with a trailing `_NNN._ext` max-size token, e.g.
//! `...-250-350-jpg_250.jpg`. The site serves a small default; rewriting the
//! trailing number to 250/300 requests a larger preview for crisper grids.

use std::sync::OnceLock;
use std::sync::RwLock;

/// Active resolution: 0 = default (unchanged), 1 = 250, 2 = 300.
static RESOLUTION: OnceLock<RwLock<u8>> = OnceLock::new();

/// Sets the active thumbnail resolution (values > 2 clamp to 0).
pub fn set_resolution(v: u8) {
    let slot = RESOLUTION.get_or_init(|| RwLock::new(0));
    *slot.write().unwrap() = if v > 2 { 0 } else { v };
}

pub fn resolution() -> u8 {
    RESOLUTION.get().map(|l| *l.read().unwrap()).unwrap_or(0)
}

/// Applies the currently-configured resolution to a thumbnail URL.
pub fn apply(url: &str) -> String {
    let res = match resolution() {
        1 => "250",
        2 => "300",
        _ => return url.to_string(),
    };
    handle(url, res)
}

/// Pure rewrite: replaces the trailing `_NNN` before the extension with `res`.
pub fn handle(url: &str, res: &str) -> String {
    match (url.rfind('_'), url.rfind('.')) {
        (Some(i1), Some(i2)) if i1 < i2 => {
            let mut out = String::with_capacity(url.len());
            out.push_str(&url[..i1 + 1]);
            out.push_str(res);
            out.push_str(&url[i2..]);
            out
        }
        _ => url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn res_300_rewrites_trailing_token() {
        assert_eq!(
            handle("https://ehgt.org/31/7a/abc-1-250-350-jpg_250.jpg", "300"),
            "https://ehgt.org/31/7a/abc-1-250-350-jpg_300.jpg"
        );
        assert_eq!(
            handle("https://ehgt.org/x/some-cover-640-480-jpg_250.jpg", "250"),
            "https://ehgt.org/x/some-cover-640-480-jpg_250.jpg"
        );
    }

    #[test]
    fn no_trailing_max_leaves_unchanged() {
        // No `_` before the extension -> untouched.
        assert_eq!(handle("https://ehgt.org/0/x/plain.jpg", "300"), "https://ehgt.org/0/x/plain.jpg");
        assert_eq!(handle("https://ehgt.org/0/x/plain.png", "250"), "https://ehgt.org/0/x/plain.png");
        // Underscore but no extension -> untouched.
        assert_eq!(handle("https://ehgt.org/_no_ext", "250"), "https://ehgt.org/_no_ext");
    }

    #[test]
    fn default_resolution_is_pass_through() {
        set_resolution(0);
        assert_eq!(apply("https://ehgt.org/a_250.jpg"), "https://ehgt.org/a_250.jpg");
        set_resolution(1);
        assert_eq!(apply("https://ehgt.org/a_250.jpg"), "https://ehgt.org/a_250.jpg");
        set_resolution(2);
        assert_eq!(apply("https://ehgt.org/a_250.jpg"), "https://ehgt.org/a_300.jpg");
        set_resolution(0);
    }
}
