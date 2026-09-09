//! Port of SXJ `GalleryListParser` + `GalleryDetailUrlParser`.
//! Parses the gallery list HTML (itg table / thumbnail mode) into GalleryInfo rows.

use scraper::{ElementRef, Html, Selector};
use std::sync::OnceLock;

use super::super::data::{GalleryInfo, GalleryListResult, ListNav};
use super::super::err::{EhError, EhResult};
use super::super::url::{fixed_preview_thumb_url, SITE_E};

pub(crate) fn sel(css: &'static str) -> &'static Selector {
    static CACHE: OnceLock<Vec<(&'static str, Selector)>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            const SELS: [&str; 17] = [
                "div.itg",
                "div.ptt",
                "div.searchnav",
                "div.searchtext",
                "[class~=\"glname\"]",
                "a",
                "div.gt",
                "div.gtl",
                "div.cn",
                "div.cs",
                ".glthumb",
                "img",
                "div.ir",
                "[class~=\"glhide\"]",
                "div.gl3e",
                "div.gl3t",
                "tr",
            ];
            SELS.iter()
                .map(|s| (*s, Selector::parse(s).expect("valid selector")))
                .collect()
        })
        .iter()
        .find(|(s, _)| *s == css)
        .map(|(_, selector)| selector)
        .expect("unknown selector")
}

/// Extracts `(gid, token)` from a gallery detail href like `/g/12345/abcd/`.
pub fn parse_detail_url(href: &str) -> Option<(u64, String)> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"/g/(\d+)/([^/]+)/").expect("detail url regex")
    });
    let caps = re.captures(href)?;
    Some((caps[1].parse().ok()?, caps[2].to_string()))
}

/// Canonical category key: lowercase, non-alphanumeric removed (e.g. "Artist CG" -> "artistcg").
pub(crate) fn canonical_category(text: &str) -> String {
    let t = text.to_lowercase();
    t.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn parse_rating(style: &str) -> Option<f32> {
    // Extract only `\d+px` tokens (positive), mirroring SXJ's PATTERN_RATING.
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(\d+)px").unwrap());
    let mut sizes: Vec<i32> = re
        .captures_iter(style)
        .filter_map(|c| c[1].parse().ok())
        .collect();
    sizes.sort_unstable();
    if sizes.len() < 2 {
        return None;
    }
    let a = *sizes.last().unwrap();
    let b = *sizes.first().unwrap();
    let base = 5 - a / 16;
    if b == 21 {
        Some(base as f32 - 0.5)
    } else {
        Some(base as f32)
    }
}

fn parse_pages_from_text(text: &str) -> Option<u32> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(\d+) page").unwrap());
    re.captures(text).and_then(|c| c[1].parse().ok())
}

fn element_text(el: &ElementRef) -> String {
    el.text().collect::<String>().trim().to_string()
}

/// Deepest descendant text (mirrors SXJ's walking to the innermost node).
fn deepest_text(el: &ElementRef) -> String {
    let mut cur = el.clone();
    loop {
        let children: Vec<ElementRef> = cur.child_elements().collect();
        if children.is_empty() {
            return cur.text().collect::<String>().trim().to_string();
        }
        cur = children[0];
    }
}

fn parse_row(row: &ElementRef) -> Option<GalleryInfo> {
    let mut gi = GalleryInfo::default();

    // Title / gid / token from .glname a
    let glname = row.select(sel("[class~=\"glname\"]")).next()?;
    let link = glname.select(sel("a")).next()
        .or_else(|| {
            row.child_elements()
                .next()
                .filter(|p| p.value().name() == "a")
        });
    let mut href = String::new();
    if let Some(a) = link {
        href = a.value().attr("href").unwrap_or("").to_string();
        if let Some((gid, token)) = parse_detail_url(&href) {
            gi.gid = gid;
            gi.token = token;
        }
    }
    let title = deepest_text(&glname);
    if title.is_empty() && href.is_empty() {
        return None; // header row / non-gallery row
    }
    gi.title = title;
    gi.token = if gi.token.is_empty() {
        href.rsplit('/').nth(1).unwrap_or("").to_string()
    } else {
        gi.token.clone()
    };

    // Tags
    for t in row.select(sel("div.gt")) {
        if let Some(v) = t.value().attr("title") {
            if !gi.tags.contains(&v.to_string()) {
                gi.tags.push(v.to_string());
            }
        }
    }
    for t in row.select(sel("div.gtl")) {
        if let Some(v) = t.value().attr("title") {
            if !gi.tags.contains(&v.to_string()) {
                gi.tags.push(v.to_string());
            }
        }
    }

    // Category
    gi.category = row
        .select(sel("div.cn"))
        .next()
        .or_else(|| row.select(sel("div.cs")).next())
        .map(|e| canonical_category(&element_text(&e)))
        .unwrap_or_default();

    // Thumb
    let glthumb = row.select(sel(".glthumb")).next();
    if let Some(gt) = glthumb {
        let img = gt
            .child_elements()
            .next()
            .and_then(|div| div.select(sel("img")).next())
            .or_else(|| gt.select(sel("img")).next());
        if let Some(img) = img {
            let style = img.value().attr("style").unwrap_or("");
            if let Some((w, h)) = parse_thumb_size(style) {
                gi.thumb_width = w;
                gi.thumb_height = h;
            }
            let url = img
                .value()
                .attr("data-src")
                .or_else(|| img.value().attr("src"));
            if let Some(u) = url {
                gi.thumb = Some(fixed_preview_thumb_url(u, SITE_E));
            }
        }
    }

    // Rotated / lazy thumb container (gl1e / gl3t)
    if gi.thumb.is_none() {
        let alt = row
            .select(sel("div.gl1e")).next()
            .or_else(|| row.select(sel("div.gl3t")).next());
        if let Some(elt) = alt {
            if let Some(img) = alt_img(elt) {
                gi.thumb_width = 0;
                gi.thumb_height = 0;
                if let Some(u) = img.value().attr("src") {
                    gi.thumb = Some(fixed_preview_thumb_url(u, SITE_E));
                }
            }
        }
    }

    // Posted + favorite slot color
    let posted = row.child_elements().find(|e| e.value().id().map(|i| i == format!("posted_{}", gi.gid)).unwrap_or(false));
    if let Some(p) = posted {
        gi.posted = element_text(&p);
        gi.favorite_slot = parse_favorite_slot(p.value().attr("style").unwrap_or(""));
    }

    // Rating
    if let Some(ir) = row.select(sel("div.ir")).next() {
        let style = ir.value().attr("style").unwrap_or("");
        if let Some(r) = parse_rating(style) {
            gi.rating = r;
        }
        let classes = ir.value().attr("class").unwrap_or("");
        gi.rated = ["irr", "irg", "irb"].iter().any(|c| classes.split_whitespace().any(|x| x == *c));
    }

    // Uploader and pages from .glhide (list) / .gl3e (extended)
    let mut uploader_index = 0usize;
    let mut pages_index = 1usize;
    let uploader_pages = row
        .select(sel("[class~=\"glhide\"]")).next()
        .or_else(|| {
            uploader_index = 3;
            pages_index = 4;
            row.select(sel("div.gl3e")).next()
        });
    if let Some(gl) = uploader_pages {
        let children: Vec<ElementRef> = gl.child_elements().collect();
        if let Some(u) = children.get(uploader_index).and_then(|c| c.select(sel("a")).next()) {
            gi.uploader = element_text(&u);
        }
        if let Some(p) = children.get(pages_index) {
            if let Some(pages) = parse_pages_from_text(&element_text(p)) {
                gi.pages = pages;
            }
        }
    }

    // Pages from thumbnail container
    if let Some(gt) = glthumb {
        let all: Vec<ElementRef> = gt.child_elements().collect();
        if let Some(second) = all.get(1) {
            let inner: Vec<ElementRef> = second.child_elements().collect();
            if let Some(third) = inner.get(1) {
                let deep: Vec<ElementRef> = third.child_elements().collect();
                if let Some(d) = deep.get(1).or_else(|| deep.last()) {
                    if let Some(pages) = parse_pages_from_text(&element_text(d)) {
                        gi.pages = pages;
                    }
                }
            }
        }
    }

    Some(gi)
}

fn alt_img(elt: ElementRef) -> Option<ElementRef> {
    elt.select(sel("img")).next()
}

fn parse_thumb_size(style: &str) -> Option<(u32, u32)> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"height:(\d+)px;width:(\d+)px").unwrap()
    });
    let caps = re.captures(style)?;
    Some((caps[2].parse().ok()?, caps[1].parse().ok()?))
}

fn parse_favorite_slot(style: &str) -> i32 {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"rgba\((\d+),(\d+),(\d+),").unwrap()
    });
    const SLOT_RGB: [[u16; 3]; 10] = [
        [0, 0, 0],
        [240, 0, 0],
        [240, 160, 0],
        [208, 208, 0],
        [0, 128, 0],
        [144, 240, 64],
        [64, 176, 240],
        [0, 0, 240],
        [80, 0, 128],
        [224, 128, 224],
    ];
    let Some(caps) = re.captures(style) else { return -2 };
    let rgb: [u16; 3] = [caps[1].parse().unwrap_or(0), caps[2].parse().unwrap_or(0), caps[3].parse().unwrap_or(0)];
    SLOT_RGB
        .iter()
        .position(|slot| *slot == rgb)
        .map(|i| i as i32)
        .unwrap_or(-2)
}

/// Extracts the `url(...)` target out of a CSS `style` string.
pub(crate) fn extract_style_url(style: &str) -> Option<String> {
    let rest = style.split("url(").nth(1)?;
    let end = rest.find(')')?;
    Some(rest[..end].trim().to_string())
}

fn parse_nav(doc: &Html) -> ListNav {
    let mut nav = ListNav::default();
    if let Some(p) = parse_pages(doc) {
        nav.pages = p as i32;
    }

    // Legacy `div.ptt` block: "last" page number + last href. Kept so older /
    // non-cursor layouts still yield a page count and a "last" link.
    if let Some(ptt) = doc.select(sel("div.ptt")).next() {
        let items: Vec<ElementRef> = ptt.select(sel("a")).collect();
        // Last-but-one <a> is the "last" page number.
        if items.len() >= 2 {
            if let Ok(n) = items[items.len() - 2].text().collect::<String>().trim().parse::<u32>() {
                nav.pages = n as i32;
            }
        }
        if nav.last.is_none() {
            if let Some(last) = items.last() {
                nav.last = last.value().attr("href").map(|s| s.to_string());
            }
        }
    }

    // Cursor-based pagination (`div.searchnav`), the current E-Hentai scheme:
    // live links `a#unext` / `a#ulast` (and `dfirst/dprev/dnext/dlast` variants)
    // carry `?next=<gid>` / `?prev=<gid>` hrefs. Disabled prev/first are rendered
    // as `<span>` (no `a`), which we skip, letting the UI disable those buttons.
    if let Some(searchnav) = doc.select(sel("div.searchnav")).next() {
        for elt in searchnav.select(sel("a")) {
            let id = elt.value().id().map(|s| s.to_lowercase());
            let href = elt.value().attr("href").map(|s| s.to_string());
            match id.as_deref() {
                Some("ufirst") | Some("dfirst") => nav.first = href,
                Some("uprev") | Some("dprev") => nav.prev = href,
                Some("unext") | Some("dnext") => nav.next = href,
                Some("ulast") | Some("dlast") => nav.last = href,
                _ => {}
            }
        }
    }

    // next page cursor number (legacy `page=` or new `next=` cursor).
    static CURSOR_RE: OnceLock<regex::Regex> = OnceLock::new();
    let cursor_re = CURSOR_RE.get_or_init(|| regex::Regex::new(r"(?:page|next)=(\d+)").unwrap());
    if let Some(nx) = &nav.next {
        if let Some(c) = cursor_re.captures(nx) {
            if let Ok(n) = c[1].parse() {
                nav.next_page = n;
            }
        }
    }

    // result count ("Found ... results")
    if let Some(st) = doc.select(sel("div.searchtext")).next() {
        let text = element_text(&st);
        if text.contains(" results") {
            nav.result_count = text.trim().to_string();
        }
    }

    nav
}

fn parse_pages(doc: &Html) -> Option<u32> {
    let ptt = doc.select(sel("div.ptt")).next()?;
    // second-last immediate child's text is the total pages
    let children: Vec<ElementRef> = ptt.child_elements().collect();
    if children.len() >= 2 {
        children.get(children.len() - 2).and_then(|e| {
            e.text().collect::<String>().trim().parse::<u32>().ok()
        })
    } else {
        None
    }
}

/// Parses a gallery list HTML body.
pub fn parse(body: &str) -> EhResult<GalleryListResult> {
    let doc = Html::parse_document(body);

    // Empty / "no hits" handling
    if body.contains("No hits found") {
        let mut out = GalleryListResult::default();
        out.nav.pages = 0;
        return Ok(out);
    }
    if body.contains("You do not have any watched tags") {
        // not an error; empty list
        return Ok(GalleryListResult::default());
    }
    if let Some(err) = EhError::classify(body) {
        return Err(err);
    }

    let mut items = Vec::new();
    let table_rows: Vec<ElementRef> = doc.select(sel("tr")).collect();
    let container_rows: Vec<ElementRef> = doc.select(sel("div.itg")).next()
        .map(|itg| itg.child_elements().collect())
        .unwrap_or_default();

    let rows = if !table_rows.is_empty() {
        table_rows
    } else {
        container_rows
    };
    // If container rows are header/table rows that aren't tr divs, still attempt.
    let parsed: Vec<GalleryInfo> = rows
        .iter()
        .filter_map(parse_row)
        .collect();
    if !parsed.is_empty() {
        items = parsed;
    } else {
        // Fallback: iterate .itg direct children as gallery blocks (thumbnail grid mode)
        if let Some(itg) = doc.select(sel("div.itg")).next() {
            for child in itg.child_elements() {
                if let Some(gi) = parse_row(&child) {
                    items.push(gi);
                }
            }
        }
    }

    let nav = parse_nav(&doc);
    Ok(GalleryListResult {
        items,
        nav,
        requested_url: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
<html><body>
<div class="itg gltc">
<table>
<tr>
  <td class="gl1c glthumb" style="width:250px">
    <div><img style="height:177px;width:250px" data-src="https://ehgt.org/31/7a/317-sample-640-480-png_300.jpg" /></div>
    <div><div><div><div>133 page</div></div></div></div>
  </td>
  <td class="gl2c">
    <div class="glname">
      <a href="/g/123456/abcdefabcdef/sample-title/"><span>Sample Gallery Title</span></a>
    </div>
    <div class="gt" title="language:turkish"></div>
    <div class="gt" title="female:none"></div>
  </td>
  <td class="gl3c glhidden">
    <div class="cn">Doujinshi</div>
    <div class="ir" style="background-position:-32px -21px">Rating</div>
    <div class="glhide">
      <div><a href="">uploader_x</a></div>
      <div>133 page</div>
    </div>
  </td>
  <td class="gl4c"><div id="posted_123456" style="color:#0a0">20 June 2020 10:00</div></td>
</tr>
</table>
</div>
</body></html>
"#;

    #[test]
    fn parses_list() {
        let result = parse(FIXTURE).unwrap();
        assert_eq!(result.items.len(), 1);
        let item = &result.items[0];
        assert_eq!(item.gid, 123456);
        assert_eq!(item.token, "abcdefabcdef");
        assert_eq!(item.title, "Sample Gallery Title");
        assert_eq!(item.pages, 133);
        assert_eq!(item.category, "doujinshi");
        assert_eq!(item.uploader, "uploader_x");
        assert!(item.rating > 0.0);
        assert_eq!(item.tags.len(), 2);
    }

    #[test]
    fn detail_url_parse() {
        let r = parse_detail_url("/g/12345/ab12/some-title/").unwrap();
        assert_eq!(r, (12345, "ab12".to_string()));
    }

    /// Regression: the live E-Hentai list uses `<table class="itg">` and puts
    /// `glname` / `glhide` on `<td>` (not nested `<div>`), so selectors must be
    /// class-word matches, not element+class.
    #[test]
    fn parses_real_table_markup() {
        let body = r#"<html><body>
<table class="itg gltc"><tr><th></th><th>Published</th><th>Title</th><th>Uploader</th></tr>
<tr>
  <td class="gl1c glcat"><div class="cn ct1">Misc</div></td>
  <td class="gl2c"><div class="glthumb" style="height:380px"><div><img style="height:334px;width:250px" data-src="https://ehgt.org/w/02/634/79783-sfv6uut8.webp" /></div></div></td>
  <td class="gl3c glname" onmouseover="show_image_pane(4178078)"><a href="https://e-hentai.org/g/4178078/f39786b197/"><div class="glink">Some Title [AI Generated]</div><div><div class="gt" title="parody:kakegurui">kakegurui</div></div></a></td>
  <td class="gl4c glhide"><div><a href="https://e-hentai.org/uploader/TTT2317">TTT2317</a></div><div>46 pages</div></td>
</tr>
</table></body></html>"#;
        let result = parse(body).unwrap();
        assert_eq!(result.items.len(), 1, "expected 1 real-markup gallery row");
        let it = &result.items[0];
        assert_eq!(it.gid, 4178078);
        assert_eq!(it.token, "f39786b197");
        assert_eq!(it.title, "Some Title [AI Generated]");
        assert_eq!(it.pages, 46);
        assert_eq!(it.uploader, "TTT2317");
        assert_eq!(it.tags, vec!["parody:kakegurui"]);
    }
    /// Regression: the live E-Hentai list now uses cursor-based navigation in
    /// `div.searchnav` (`?next=<gid>` / `?prev=<gid>`, ids `unext`/`ulast`) and
    /// no longer emits `div.ptt`, so `nav.pages` stays 0 while next/last hrefs
    /// are populated; disabled prev/first (rendered as `<span>`) leave them None.
    #[test]
    fn parses_cursor_searchnav() {
        let body = r#"<html><body>
        <div class="searchnav">
        <div><span id="ufirst">&lt;&lt; First</span></div>
        <div><span id="uprev">&lt; Prev</span></div>
        <div id="ujumpbox" class="jumpbox"><a id="ujump" href="javascript:enable_jump_mode('u')">Jump/Seek</a></div>
        <div><a id="unext" href="https://e-hentai.org/?next=4178052">Next &gt;</a></div>
        <div><a id="ulast" href="https://e-hentai.org/?prev=1">Last &gt;&gt;</a></div>
        </div>
        <div class="searchtext">Found 200 results</div>
        <div class="itg gltc"><table><tr>
        <td class="gl2c"><div class="glthumb"><div><img style="height:334px;width:250px" src="https://ehgt.org/w/02/634/79783-sfv6uut8.webp" /></div></div></td>
        <td class="gl3c glname"><a href="https://e-hentai.org/g/4178078/f39786b197/"><div class="glink">Title</div></a></td>
        </tr></table></div>
        </body></html>"#;
        let result = parse(body).unwrap();
        assert_eq!(result.nav.pages, 0);
        assert_eq!(result.nav.next, Some("https://e-hentai.org/?next=4178052".into()));
        assert_eq!(result.nav.last, Some("https://e-hentai.org/?prev=1".into()));
        assert_eq!(result.nav.prev, None);
        assert_eq!(result.nav.first, None);
        assert_eq!(result.nav.next_page, 4178052);
        assert!(result.nav.result_count.contains("200 results"));
    }

}