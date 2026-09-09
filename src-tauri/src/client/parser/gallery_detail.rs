//! Port of SXJ `GalleryDetailParser`: parses the gallery detail page and its
//! preview set.

use scraper::{ElementRef, Html, Selector};
use std::sync::OnceLock;

use super::super::data::{GalleryComment, GalleryDetail, PreviewItem, TagGroup};
use super::super::err::{EhError, EhResult};
use super::gallery_list::canonical_category;

const OFFENSIVE_STRINGS: [&str; 2] = ["This gallery has been removed", "Offensive Content"];
const PINING_STRING: &str = "the artist has requested that this gallery be pining";
const UNAVAILABLE: &str =
    "Sorry, but the gallery this image belongs to is not available";

fn sel(css: &'static str) -> &'static Selector {
    static CACHE: OnceLock<Vec<(&'static str, Selector)>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            const SELS: [&str; 15] = [
                "#gd1", "#gn", "#gj", "#gdc", "#gdn", "#gdd", "#gdf",
                "#rating_count", "#rating_label", "#taglist", "#cdiv",
                ".cn", ".cs", ".c1", "tr",
            ];
            SELS.iter().map(|s| (*s, Selector::parse(s).expect("sel"))).collect()
        })
        .iter()
        .find(|(s, _)| *s == css)
        .map(|(_, s)| s)
        .expect("unknown selector")
}

fn text_of(el: &ElementRef) -> String {
    el.text().collect::<String>().trim().to_string()
}

static RE_DETAIL: OnceLock<regex::Regex> = OnceLock::new();
static RE_COVER: OnceLock<regex::Regex> = OnceLock::new();
static RE_PAGES: OnceLock<regex::Regex> = OnceLock::new();

fn re(cache: &'static OnceLock<regex::Regex>, pat: &'static str) -> &'static regex::Regex {
    cache.get_or_init(move || regex::Regex::new(pat).expect("valid regex"))
}

fn parse_script_meta(body: &str) -> EhResult<(u64, String)> {
    let rex = re(
        &RE_DETAIL,
        r#"(?s)var gid = (\d+);.+?var token = "([a-f0-9]+)";"#,
    );
    let caps = rex
        .captures(body)
        .ok_or_else(|| EhError::Parse("Can't parse gallery detail (no gid/token script)".into()))?;
    Ok((
        caps[1].parse().map_err(|_| EhError::Parse("bad gid".into()))?,
        caps[2].to_string(),
    ))
}

fn parse_cover(style: &str) -> String {
    let rex = re(&RE_COVER, r"url\((.+?)\)");
    rex.captures(style).map(|c| c[1].to_string()).unwrap_or_default()
}

fn parse_pages(body: &str) -> u32 {
    let rex = re(&RE_PAGES, r"Length:</td><td[^<>]*>([\d,]+) pages</td>");
    rex.captures(body)
        .and_then(|c| c[1].replace(',', "").parse().ok())
        .unwrap_or(0)
}

fn parse_tag_groups(doc: &Html) -> Vec<TagGroup> {
    let Some(taglist) = doc.select(sel("#taglist")).next() else {
        return Vec::new();
    };
    let mut groups = Vec::new();
    for row in taglist.select(sel("tr")) {
        let cells: Vec<ElementRef> = row.child_elements().collect();
        if cells.len() < 2 {
            continue;
        }
        let name = text_of(&cells[0]);
        if name.is_empty() {
            continue;
        }
        let mut tags = Vec::new();
        for a in cells[1].select(super::gallery_list::sel("a")) {
            let t = text_of(&a);
            if !t.is_empty() && !tags.contains(&t) {
                tags.push(t);
            }
        }
        groups.push(TagGroup { name, tags });
    }
    groups
}

fn parse_comments(doc: &Html) -> Vec<GalleryComment> {
    let Some(cdiv) = doc.select(sel("#cdiv")).next() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for c1 in cdiv.select(sel(".c1")) {
        let mut c = GalleryComment {
            user: c1
                .select(super::gallery_list::sel("a"))
                .next()
                .map(|a| text_of(&a))
                .unwrap_or_default(),
            ..Default::default()
        };
        for e in c1.child_elements() {
            let cls = e.value().attr("class").unwrap_or("");
            if cls.split_whitespace().any(|x| x == "c6") {
                c.comment = e.text().collect::<String>().trim().to_string();
            } else if cls.split_whitespace().any(|x| x == "c5") {
                c.score = e.text().collect::<String>().trim().parse().unwrap_or(0);
            } else if cls.split_whitespace().any(|x| x == "c3") {
                c.time = e.text().collect::<String>().trim().to_string();
            }
        }
        out.push(c);
    }
    out
}

/// Parses the gallery detail page.
pub fn parse(body: &str) -> EhResult<GalleryDetail> {
    for s in OFFENSIVE_STRINGS {
        if body.contains(s) {
            return Err(EhError::GalleryNotFound);
        }
    }
    if body.contains(PINING_STRING) {
        return Err(EhError::Validation("画廊已 pining".into()));
    }
    if body.contains(UNAVAILABLE) {
        return Err(EhError::GalleryNotFound);
    }

    let (gid, token) = parse_script_meta(body)?;
    let doc = Html::parse_document(body);
    let mut detail = GalleryDetail { gid, token, ..Default::default() };

    if let Some(gd1) = doc.select(sel("#gd1")).next() {
        detail.thumb = Some(parse_cover(
            gd1.child_elements()
                .next()
                .and_then(|e| e.value().attr("style"))
                .unwrap_or(""),
        ));
    }
    if let Some(gn) = doc.select(sel("#gn")).next() {
        detail.title = text_of(&gn);
    }
    if let Some(gj) = doc.select(sel("#gj")).next() {
        let t = text_of(&gj);
        detail.title_jpn = if t.is_empty() { None } else { Some(t) };
    }
    if let Some(gdc) = doc.select(sel("#gdc")).next() {
        let ce = gdc.select(sel(".cn")).next().or_else(|| gdc.select(sel(".cs")).next());
        if let Some(c) = ce {
            detail.category = canonical_category(&text_of(&c));
        }
    }
    if let Some(gdn) = doc.select(sel("#gdn")).next() {
        detail.uploader = text_of(&gdn);
    }
    if let Some(gdd) = doc.select(sel("#gdd")).next() {
        for row in gdd.select(sel("tr")) {
            let cells: Vec<ElementRef> = row.child_elements().collect();
            if cells.len() < 2 {
                continue;
            }
            let key = text_of(&cells[0]);
            let value = cells[1].text().collect::<String>().trim().to_string();
            if key.starts_with("Posted") {
                detail.posted = value;
            } else if key.starts_with("Language") {
                detail.language = value;
            } else if key.starts_with("File Size") {
                detail.size = value;
            } else if key.starts_with("Length") {
                detail.pages = value
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.replace(',', "").parse().ok())
                    .unwrap_or(1);
            } else if key.starts_with("Favorited") {
                detail.favorite_count = match value.as_str() {
                    "Never" => 0,
                    "Once" => 1,
                    _ => value
                        .split_whitespace()
                        .next()
                        .and_then(|v| v.replace(',', "").parse().ok())
                        .unwrap_or(0),
                };
            }
        }
    }

    if let Some(rc) = doc.select(sel("#rating_count")).next() {
        detail.rating_count = text_of(&rc).parse().unwrap_or(0);
    }
    if let Some(rl) = doc.select(sel("#rating_label")).next() {
        let s = text_of(&rl);
        if s.contains("Not Yet Rated") || s.is_empty() {
            detail.rating = -1.0;
        } else {
            detail.rating = s
                .split_whitespace()
                .last()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
        }
    }
    if let Some(gdf) = doc.select(sel("#gdf")).next() {
        let t = text_of(&gdf);
        if !t.is_empty() {
            detail.is_favorited = t != "Add to Favorites";
            detail.favorite_name = if detail.is_favorited { Some(t) } else { None };
        }
    }

    detail.tags = parse_tag_groups(&doc);
    detail.comments = parse_comments(&doc);
    detail.pages = if detail.pages == 0 { parse_pages(body) } else { detail.pages };

    if let Some(ptt) = doc.select(super::gallery_list::sel("div.ptt")).next() {
        let items: Vec<ElementRef> = ptt.child_elements().collect();
        if items.len() >= 2 {
            if let Ok(n) = items[items.len() - 2].text().collect::<String>().trim().parse::<i32>() {
                detail.preview_pages = n;
            }
        }
    }

    Ok(detail)
}

/// Parses preview thumbnails from a preview-set page (normal, sprite or large).
///
/// Mirrors SXJ `GalleryDetailParser`. Previews may be served either as per-page
/// images, or as a shared sprite/montage where each page is a crop region. We
/// parse the per-page `Page N` label (global gallery index), the image URL and
/// any `-NNNpx` crop offsets so the frontend can show each page's own thumbnail.
/// Extracts the per-page image URL out of a preview div `style` (the `url(...)`
/// content, stripping any surrounding quotes).
fn preview_image_url(style: &str) -> String {
    super::gallery_list::extract_style_url(style).unwrap_or_else(|| {
        style
            .trim()
            .trim_start_matches('"')
            .trim_start_matches('\'')
            .trim_end_matches('\'')
            .trim_end_matches('"')
            .to_string()
    })
}

/// Width/height of a preview cell as given by its div `style`.
fn preview_dimensions(style: &str) -> (u32, u32) {
    static RE_W: OnceLock<regex::Regex> = OnceLock::new();
    static RE_H: OnceLock<regex::Regex> = OnceLock::new();
    let w = RE_W
        .get_or_init(|| regex::Regex::new(r"width:(\d+)px").unwrap())
        .captures(style)
        .and_then(|c| c[1].parse().ok())
        .unwrap_or(0);
    let h = RE_H
        .get_or_init(|| regex::Regex::new(r"height:(\d+)px").unwrap())
        .captures(style)
        .and_then(|c| c[1].parse().ok())
        .unwrap_or(0);
    (w, h)
}

/// Extracts sprite crop info (negative `background-position` tokens) from a
/// preview div `style`. Returns `(x_offset, y_offset, clip_width, clip_height)`.
fn preview_crop(
    style: &str,
    width: u32,
    height: u32,
) -> (
    Option<i32>,
    Option<i32>,
    Option<u32>,
    Option<u32>,
) {
    static RE_POS: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE_POS.get_or_init(|| regex::Regex::new(r"-(\d+)px").unwrap());
    let offsets: Vec<i32> = re
        .captures_iter(style)
        .filter_map(|c| c[1].parse().ok())
        .collect();
    let w = (width > 0).then_some(width);
    let h = (height > 0).then_some(height);
    match offsets.len() {
        0 => (None, None, w, h),
        1 => (Some(offsets[0]), None, w, h),
        _ => (Some(offsets[0]), Some(offsets[1]), w, h),
    }
}

/// Parses preview thumbnails from a preview-set page (normal, sprite or large).
///
/// Mirrors SXJ `GalleryDetailParser`. Previews may be served either as per-page
/// images, or as a shared sprite/montage where each page is a crop region. We
/// parse the per-page `Page N` label (global gallery index), the image URL and
/// any `-NNNpx` crop offsets so the frontend can show each page's own thumbnail.
pub fn parse_preview_set(body: &str) -> EhResult<Vec<PreviewItem>> {
    thread_local! {
        // A preview anchor wrapping a styled div (per-page or sprite thumbnails).
        static RE_DIV: regex::Regex = regex::Regex::new(
            r#"(?s)<a\s+href="([^"]+)"[^>]*>\s*<div(?P<attrs>[^>]*?)>"#,
        ).expect("preview div");
        static RE_TITLE: regex::Regex =
            regex::Regex::new(r#"title="Page (\d+):"#).unwrap();
        static RE_STYLE: regex::Regex =
            regex::Regex::new(r#"style="([^"]*url\([^"]*\)[^"]*)""#).unwrap();
    }

    fn push_item(
        set: &mut Vec<PreviewItem>,
        index: u32,
        page_url: String,
        style: String,
    ) {
        let img = preview_image_url(&style);
        if img.is_empty() {
            return;
        }
        let (width, height) = preview_dimensions(&style);
        let (ox, oy, cw, ch) = preview_crop(&style, width, height);
        set.push(PreviewItem {
            index,
            image_url: img,
            page_url,
            width: (width > 0).then_some(width),
            height: (height > 0).then_some(height),
            x_offset: ox,
            y_offset: oy,
            clip_width: cw,
            clip_height: ch,
        });
    }

    // Pass 1: divs with an explicit "Page N" label -> global gallery index.
    let mut set: Vec<PreviewItem> = Vec::new();
    RE_DIV.with(|rex| {
        for cap in rex.captures_iter(body) {
            let page_url = cap[1].to_string();
            let attrs = cap
                .name("attrs")
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let idx = RE_TITLE.with(|t| t.captures(&attrs).and_then(|c| c[1].parse::<u32>().ok()));
            let Some(idx) = idx else { continue };
            if idx == 0 {
                continue;
            }
            let style = RE_STYLE.with(|st| st.captures(&attrs).map(|c| c[1].to_string()));
            let Some(style) = style else { continue };
            push_item(&mut set, idx - 1, page_url, style);
        }
    });

    // Pass 2: large previews carry no explicit number -> document order.
    if set.is_empty() {
        RE_DIV.with(|rex| {
            for (order, cap) in rex.captures_iter(body).enumerate() {
                let page_url = cap[1].to_string();
                let attrs = cap
                    .name("attrs")
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let style = RE_STYLE.with(|st| st.captures(&attrs).map(|c| c[1].to_string()));
                let Some(style) = style else { continue };
                push_item(&mut set, order as u32, page_url, style);
            }
        });
    }

    // If this is a sprite set (any crop offset found), normalise crops to 0 so
    // every page is cut from the same montage rather than reusing the first.
    if set.iter().any(|p| p.x_offset.is_some() || p.y_offset.is_some()) {
        for p in set.iter_mut() {
            p.x_offset.get_or_insert(0);
            p.y_offset.get_or_insert(0);
            if p.clip_width.is_none() {
                p.clip_width = p.width;
            }
            if p.clip_height.is_none() {
                p.clip_height = p.height;
            }
        }
    }

    set.sort_by_key(|i| i.index);
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_detail() {
        let body = r#"
<script>
var gid = 424242;
var token = "abcd1234ef56";
var apiuid = 99; var apikey = "ff00ee";
</script>
<div id="gd1"><div style="width:250px; height:356px; background:transparent url(https://ehgt.org/31/7a/317-cover-640-480-jpg_250.jpg) 0 0 no-repeat"></div></div>
<div id="gd2"><h1 id="gn">Super Gallery</h1><h1 id="gj">Supa Gyararii</h1>
  <div id="gdc"><div class="cn">Doujinshi</div></div>
  <div id="gdn"><a href="/uploader/someone">someone</a></div>
  <div id="gdd"><table><tr><td>Posted:</td><td>20 June 2020</td></tr>
    <tr><td>Length:</td><td>40 pages</td></tr>
    <tr><td>Language:</td><td>English</td></tr></table></div>
  <div id="rating_count">23</div>
  <div id="rating_label">Average: 4.67</div>
  <div id="gdf">Add to Favorites</div>
</div>
<div id="taglist"><table><tbody>
  <tr><td>Language:</td><td><div class="gtl"><a>english</a></div></td></tr>
  <tr><td>Artist:</td><td><div class="gtl"><a>bob</a><a>alice</a></div></td></tr>
</tbody></table></div>
<div id="cdiv"><div class="c1">
  <div class="c3">Posted on 01 Jan 2020 by: <a>user1</a></div>
  <div class="c5"><span>3</span></div>
  <div class="c6">Great gallery</div>
</div></div>
<div class="ptt"><table><tr><td><a>1</a></td><td><a>2</a></td><td><a>3</a></td></tr></table></div>
"#;
        let d = parse(body).unwrap();
        assert_eq!(d.gid, 424242);
        assert_eq!(d.token, "abcd1234ef56");
        assert_eq!(d.title, "Super Gallery");
        assert_eq!(d.title_jpn.as_deref(), Some("Supa Gyararii"));
        assert_eq!(d.category, "doujinshi");
        assert_eq!(d.uploader, "someone");
        assert_eq!(d.pages, 40);
        assert_eq!(d.language, "English");
        assert_eq!(d.rating_count, 23);
        assert!((d.rating - 4.67).abs() < 0.01);
        assert!(!d.is_favorited);
        assert_eq!(d.tags.len(), 2);
        assert_eq!(d.tags[0].tags, vec!["english"]);
        assert_eq!(d.tags[1].tags, vec!["bob", "alice"]);
        assert_eq!(d.comments.len(), 1);
        assert_eq!(d.comments[0].comment, "Great gallery");
        assert!(d.thumb.as_deref().unwrap_or("").contains("ehgt.org"));
    }

    #[test]
    fn preview_set_large() {
        let body = r#"<div class="gt200">
<div class="gdtl"><a href="https://e-hentai.org/s/p1/424242-1"><div style="width:250px;height:350px;background:transparent url(https://ehgt.org/31/7a/abc-1-250-350-jpg_250.jpg) 0 0"></div></a></div>
<div class="gdtl"><a href="https://e-hentai.org/s/p2/424242-2"><div style="width:250px;height:350px;background:transparent url(https://ehgt.org/31/7a/def-2-250-350-jpg_250.jpg) 0 0"></div></a></div>
</div>"#;
        let set = parse_preview_set(body).unwrap();
        assert_eq!(set.len(), 2);
        assert_eq!(set[0].index, 0);
        assert_eq!(set[0].image_url, "https://ehgt.org/31/7a/abc-1-250-350-jpg_250.jpg");
        assert_eq!(set[1].index, 1);
    }


    #[test]
    fn preview_set_sprite_crops_per_page() {
        // All three pages share ONE montage/sprite URL; each page differs only by
        // its background-position `-NNNpx` offset. The parser must keep the crop
        // offsets so the frontend renders distinct thumbnails.
        let body = r#"<div class="gdtm">
<a href="https://e-hentai.org/s/p1/424242-1"><div style="width:250px;height:350px;background:transparent url(https://ehgt.org/31/7a/317-montage-sprite.jpg) 0 0 no-repeat" title="Page 1: uploader"></div></a>
<a href="https://e-hentai.org/s/p2/424242-2"><div style="width:250px;height:350px;background:transparent url(https://ehgt.org/31/7a/317-montage-sprite.jpg) -250px 0 no-repeat" title="Page 2: uploader"></div></a>
<a href="https://e-hentai.org/s/p3/424242-3"><div style="width:250px;height:350px;background:transparent url(https://ehgt.org/31/7a/317-montage-sprite.jpg) -500px 0 no-repeat" title="Page 3: uploader"></div></a>
</div>"#;
        let set = parse_preview_set(body).unwrap();
        assert_eq!(set.len(), 3);
        assert_eq!(set[0].index, 0);
        assert_eq!(set[1].index, 1);
        assert_eq!(set[2].index, 2);
        // Same sprite URL, but each page gets its own crop offset.
        assert_eq!(set[0].image_url, set[1].image_url);
        assert_eq!(set[0].x_offset, Some(0));
        assert_eq!(set[1].x_offset, Some(250));
        assert_eq!(set[2].x_offset, Some(500));
        assert_eq!(set[0].clip_width, Some(250));
        assert_eq!(set[0].clip_height, Some(350));
        assert_eq!(set[1].image_url, "https://ehgt.org/31/7a/317-montage-sprite.jpg");
    }
}
