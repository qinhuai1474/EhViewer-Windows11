//! Disk layout (port of SXJ `SpiderDen`) and the `.ehviewer` index (SXJ `SpiderInfo`).
//! Gallery folder: `<gid>-<sanitized title>`, page files `%08d.<ext>`, plus `.ehviewer`.

use std::path::{Path, PathBuf};

pub const SPIDER_INFO_FILENAME: &str = ".ehviewer";

/// Extensions recognised when scanning for already-downloaded pages.
pub const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp"];

/// Keeps filename-safe characters, replacing everything else (mirrors SXJ `sanitizeFilename`).
pub fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(|c| c == ' ' || c == '.').to_string();
    if trimmed.is_empty() {
        "gallery".to_string()
    } else {
        trimmed
    }
}

/// Gallery download folder name: `<gid>-<sanitized title>`.
pub fn gallery_dir_name(gid: u64, title: &str) -> String {
    format!("{}-{}", gid, sanitize_filename(title))
}

/// Page file name (1-based, zero padded to 8), e.g. `00000001.jpg`.
pub fn image_filename(index: u32, ext: &str) -> String {
    format!("{:08}.{}", index + 1, ext.trim_start_matches('.'))
}

/// Whether the image for `index` already exists on disk under `dir`.
pub fn page_exists(dir: &Path, index: u32) -> bool {
    IMAGE_EXTS
        .iter()
        .any(|e| dir.join(image_filename(index, e)).is_file())
}

/// Counts pages `0..total` that already have a file on disk.
pub fn scan_completed_pages(dir: &Path, total: u32) -> u32 {
    let mut n = 0;
    for i in 0..total {
        if page_exists(dir, i) {
            n += 1;
        }
    }
    n
}

/// First page index with no file on disk, or `total` when all are present.
pub fn first_missing_page(dir: &Path, total: u32) -> u32 {
    for i in 0..total {
        if !page_exists(dir, i) {
            return i;
        }
    }
    total
}

/// Writes a downloaded page to disk (temp file + rename for atomicity).
pub fn write_image(dir: &Path, index: u32, bytes: &[u8]) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let ext = detect_image_ext(bytes).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "downloaded payload is not a valid image")
    })?;
    let path = dir.join(image_filename(index, ext));
    let tmp = dir.join(format!(".tmp-{:08}", index));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Best-effort image-type detection by magic bytes. HTML / unrecognizable
/// payloads return `None` so callers never save a `.bin` file.
pub fn detect_image_ext(bytes: &[u8]) -> Option<&'static str> {
    if looks_like_html(bytes) {
        return None;
    }
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        Some("jpg")
    } else if bytes.len() >= 8
        && bytes[0] == 0x89
        && bytes[1] == b'P'
        && bytes[2] == b'N'
        && bytes[3] == b'G'
    {
        Some("png")
    } else if bytes.len() >= 4 && &bytes[0..4] == b"GIF8" {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.len() >= 2 && &bytes[0..2] == b"BM" {
        Some("bmp")
    } else {
        None
    }
}

/// HTML pages (invalid image responses) start with a `<` tag when served raw.
fn looks_like_html(bytes: &[u8]) -> bool {
    let head = bytes.get(..256).unwrap_or(bytes);
    if head.first() != Some(&b'<') {
        return false;
    }
    let text = String::from_utf8_lossy(head).to_ascii_lowercase();
    text.contains("<html") || text.contains("<!doctype") || text.contains("<body")
}

/// The `.ehviewer` index file: SXJ `SpiderInfo` format (VERSION 2 header + pToken map).
#[derive(Debug, Clone)]
pub struct SpiderInfo {
    pub start_page: u32,
    pub gid: u64,
    pub token: String,
    pub preview_pages: i32,
    pub preview_per_page: i32,
    pub pages: i32,
    pub p_tokens: Vec<(u32, String)>,
}

impl Default for SpiderInfo {
    fn default() -> Self {
        Self {
            start_page: 0,
            gid: 0,
            token: String::new(),
            preview_pages: 0,
            preview_per_page: 0,
            pages: 0,
            p_tokens: Vec::new(),
        }
    }
}

impl SpiderInfo {
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let mut s = String::new();
        s.push_str("VERSION2\n");
        s.push_str(&format!("{:08x}\n", self.start_page));
        s.push_str(&format!("{}\n", self.gid));
        s.push_str(&self.token);
        s.push('\n');
        s.push_str("1\n");
        s.push_str(&format!("{}\n", self.preview_pages));
        s.push_str(&format!("{}\n", self.preview_per_page));
        s.push_str(&format!("{}\n", self.pages));
        for (idx, tok) in &self.p_tokens {
            if tok.is_empty() {
                continue;
            }
            s.push_str(&format!("{} {}\n", idx, tok));
        }
        std::fs::write(path, s)
    }

    pub fn read(path: &Path) -> Option<SpiderInfo> {
        let raw = std::fs::read_to_string(path).ok()?;
        let mut lines = raw.lines();
        lines.next()?; // VERSION1/VERSION2
        let mut info = SpiderInfo::default();
        let start_line = lines.next()?;
        info.start_page = u32::from_str_radix(start_line.trim(), 16).ok()?;
        info.gid = lines.next()?.trim().parse().ok()?;
        info.token = lines.next()?.trim().to_string();
        let _ = lines.next()?; // always "1"
        info.preview_pages = lines.next()?.trim().parse().ok()?;
        info.preview_per_page = lines.next()?.trim().parse().ok()?;
        info.pages = lines.next()?.trim().parse().ok()?;
        for line in lines {
            let mut it = line.trim().split_whitespace();
            if let (Some(i), Some(t)) = (it.next(), it.next()) {
                if let Ok(ix) = i.parse::<u32>() {
                    info.p_tokens.push((ix, t.to_string()));
                }
            }
        }
        Some(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_filename("My: Gallery / v1"), "My_ Gallery _ v1");
        assert_eq!(sanitize_filename("   "), "gallery");
    }

    #[test]
    fn dir_and_file_names() {
        assert_eq!(gallery_dir_name(42, "Hello World!"), "42-Hello World_");
        assert_eq!(image_filename(0, "jpg"), "00000001.jpg");
        assert_eq!(image_filename(11, ".png"), "00000012.png");
    }

    #[test]
    fn roundtrip_spider_info() {
        let dir = std::env::temp_dir().join(format!("ehv-si-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SPIDER_INFO_FILENAME);
        let info = SpiderInfo {
            start_page: 3,
            gid: 987654321,
            token: "abc123def".into(),
            preview_pages: 4,
            preview_per_page: 20,
            pages: 80,
            p_tokens: vec![(0, "tok0".into()), (19, "tok19".into())],
        };
        info.write(&path).unwrap();
        let read = SpiderInfo::read(&path).unwrap();
        assert_eq!(read.gid, info.gid);
        assert_eq!(read.token, info.token);
        assert_eq!(read.pages, 80);
        assert_eq!(read.p_tokens, vec![(0, "tok0".into()), (19, "tok19".into())]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scan_completed() {
        let dir = std::env::temp_dir().join(format!("ehv-scan-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write_image(&dir, 0, &[0xFF, 0xD8, 0xFF, 0x00]).unwrap();
        write_image(&dir, 2, &b"GIF89a".to_vec()).unwrap();
        assert_eq!(scan_completed_pages(&dir, 4), 2);
        assert_eq!(first_missing_page(&dir, 4), 1);
        assert_eq!(image_filename(0, "bin"), "00000001.bin");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ext_detection() {
        assert_eq!(detect_image_ext(&[0xFF, 0xD8, 0xFF]), Some("jpg"));
        assert_eq!(detect_image_ext(b"\x89PNG\r\n\x1a\n"), Some("png"));
        assert_eq!(detect_image_ext(b"RIFF....WEBPVP8 "), Some("webp"));
        assert_eq!(detect_image_ext(&[1, 2, 3]), None);
        assert_eq!(detect_image_ext(b"<html><body>error</body></html>"), None);
    }
}

