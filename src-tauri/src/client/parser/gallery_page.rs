//! Port of SXJ `GalleryPageParser`: extracts the image URL from an online reading page.

use super::super::data::OnlinePage;
use super::super::err::{EhError, EhResult};

/// Parses the online image page for gallery `gid` at (0-based) `index`.
pub fn parse(body: &str, gid: u64, index: u32) -> EhResult<OnlinePage> {
    let mut page = OnlinePage {
        gid,
        index,
        ..Default::default()
    };

    if let Some(err) = EhError::classify(body) {
        return Err(err);
    }
    if body.contains("this image belongs to is not available") {
        return Err(EhError::GalleryNotFound);
    }

    // <img src="..." style
    if let Some(m) = find_img_src(body) {
        page.image_url = m;
    }
    // <a href="...fullimg...">
    page.origin_image_url = find_origin_img(body);
    // var showkey="..."
    page.show_key = find_show_key(body);

    if page.image_url.is_empty() {
        return Err(EhError::Parse("Cannot parse image url from online page".into()));
    }
    Ok(page)
}

fn find_img_src(body: &str) -> Option<String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r#"<img[^>]*src="([^"]+)" style"#).unwrap());
    re.captures(body).map(|c| xml_unescape(c[1].trim()))
}

fn find_origin_img(body: &str) -> Option<String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r#"<a href="([^"]+)fullimg([^"]+)">"#).unwrap());
    if let Some(c) = re.captures(body) {
        Some(format!("{}fullimg{}", xml_unescape(c[1].trim()), xml_unescape(c[2].trim())))
    } else {
        None
    }
}

fn find_show_key(body: &str) -> Option<String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r#"var showkey="([0-9a-z]+)";"#).unwrap());
    re.captures(body).map(|c| c[1].to_string())
}

fn xml_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_online_page() {
        let body = r#"
<script>var showkey="abc123def";</script>
<div id="i7"><img src="https://rs.example.com/0/x/y_z.jpg" style="max-width:1600px"></div>
<a href="https://rs.example.com/0/x/y_z" onclick="return nl('deadbeef')">for full-size image</a>
"#;
        let p = parse(body, 424242, 4).unwrap();
        assert_eq!(p.gid, 424242);
        assert_eq!(p.index, 4);
        assert_eq!(p.image_url, "https://rs.example.com/0/x/y_z.jpg");
        assert_eq!(p.show_key.as_deref(), Some("abc123def"));
    }

    #[test]
    fn rejects_missing_image() {
        let r = parse("<html><body>no image</body></html>", 1, 0);
        assert!(r.is_err());
    }
}
