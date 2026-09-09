//! Port of SXJ `EhEngine`: high-level gallery operations used by Tauri commands.

use std::sync::OnceLock;
use std::sync::RwLock;

use super::client;
use super::config::EhConfig;
use super::dns;
use super::data::{GalleryDetail, GalleryInfo, GalleryListResult, OnlinePage, PreviewItem, GalleryToken};
use super::err::{EhError, EhResult};
use super::parser::{api, gallery_detail, gallery_list, gallery_page};
use super::tags;
use super::url;

/// The active uconfig settings, mirroring the app-wide EhConfig singleton.
static CONFIG: OnceLock<RwLock<EhConfig>> = OnceLock::new();

pub fn config() -> &'static RwLock<EhConfig> {
    CONFIG.get_or_init(|| RwLock::new(EhConfig::default()))
}

/// Applies site, session cookies, uconfig, proxy, custom host and the hosts
/// override table to the shared HTTP client (mirrors the singleton settings of
/// SXJ). Called at startup and whenever settings change so all network options
/// (DNS / proxy / hosts) take effect immediately.
pub fn apply_settings(s: &crate::settings::Settings) {
    client::set_site(s.site);
    client::set_session_cookies(
        s.session_cookies
            .iter()
            .map(|c| (c.name.clone(), c.value.clone()))
            .collect(),
    );
    client::set_extra_cookies(vec![("uconfig".to_string(), s.config.uconfig())]);
    client::apply_proxy(s.proxy_type, s.proxy_url.clone());
    client::apply_network(
        s.timeout_secs,
        s.use_builtin_hosts,
        if s.doh_url.trim().is_empty() {
            None
        } else {
            Some(s.doh_url.clone())
        },
    );
    url::set_host_override(s.custom_host.clone());
    dns::apply_hosts_text(&s.hosts_override);
}
/// Refreshes the uconfig cookie on the client from the current settings.
pub fn apply_uconfig() {
    let cfg = config().read().unwrap();
    client::set_extra_cookies(vec![
        ("uconfig".to_string(), cfg.uconfig()),
    ]);
}

/// Applies the configured thumbnail resolution to an optional URL.
fn apply_thumb(opt: Option<String>) -> Option<String> {
    opt.map(|u| super::thumb::apply(&u))
}

/// Fetches a gallery list page and parses it.
pub async fn get_gallery_list(url: &str) -> EhResult<GalleryListResult> {
    apply_uconfig();
    if site_switch_needed(&url) {
        client::set_site(if url.contains("exhentai") { 1 } else { 0 });
    }
    let body = client::get_text(url, None).await?;
    let mut result = gallery_list::parse(&body)?;
    for item in result.items.iter_mut() {
        for tag in item.tags.iter_mut() {
            *tag = tags::translate_list_tag(tag);
        }
        item.thumb = apply_thumb(item.thumb.take());
    }
    result.requested_url = Some(url.to_string());
    Ok(result)
}

/// Detects whether the given URL implies an EX site switch.
fn site_switch_needed(url: &str) -> bool {
    let contains_ex = url.contains("exhentai.org");
    (contains_ex as u8) != client::site()
}

/// Ensures the client is on `site` (0 = E, 1 = EX).
fn ensure_site(site: u8) {
    if client::site() != site {
        client::set_site(site);
    }
    apply_uconfig();
}

/// Fetches a gallery detail page and parses it.
pub async fn get_gallery_detail(
    site: u8,
    gid: u64,
    token: &str,
    index: u32,
) -> EhResult<GalleryDetail> {
    ensure_site(site);
    let u = url::gallery_detail_url(site, gid, token, index, false);
    let body = client::get_text(&u, None).await?;
    let mut detail = gallery_detail::parse(&body)?;
    for group in detail.tags.iter_mut() {
        let namespace = group.name.clone();
        for tag in group.tags.iter_mut() {
            *tag = tags::translate_detail_tag(&namespace, tag);
        }
    }
    detail.thumb = apply_thumb(detail.thumb.take());
    Ok(detail)
}

/// Fetches one preview-set page (thumbnails) for a gallery.
pub async fn get_preview_set(
    site: u8,
    gid: u64,
    token: &str,
    index: u32,
) -> EhResult<Vec<PreviewItem>> {
    ensure_site(site);
    let u = url::gallery_detail_url(site, gid, token, index, false);
    let body = client::get_text(&u, None).await?;
    let mut set = gallery_detail::parse_preview_set(&body)?;
    for p in set.iter_mut() {
        p.image_url = super::thumb::apply(&p.image_url);
    }
    Ok(set)
}

/// Fetches a single reading/online page. When `p_token` is empty, the correct
/// per-page `/s/` token is resolved from the covering preview set (consistent
/// with the download engine), so a caller can pass `""` and get the right page.
pub async fn get_online_page(
    site: u8,
    gid: u64,
    token: &str,
    index: u32,
    p_token: &str,
) -> EhResult<OnlinePage> {
    ensure_site(site);
    let p_token = if p_token.is_empty() {
        resolve_online_token(site, gid, token, index).await?
    } else {
        p_token.to_string()
    };
    let u = url::gallery_page_url(site, gid, index, &p_token);
    let body = client::get_text(&u, Some(&url::gallery_detail_url(site, gid, token, 0, false))).await?;
    gallery_page::parse(&body, gid, index)
}

/// Preview page index that covers gallery page `index`. `per_page` is the number
/// of thumbnails per preview set (typically 20).
pub fn preview_page_of(index: u32, per_page: u32) -> u32 {
    if per_page == 0 { 0 } else { index / per_page }
}

/// Finds the per-page `/s/` token for `index` inside a preview set, if present.
pub fn token_for_index(items: &[PreviewItem], index: u32) -> Option<String> {
    items
        .iter()
        .find(|p| p.index == index)
        .and_then(|p| url::page_token_from(&p.page_url))
}

/// Resolves the per-page `/s/` token for `index` from the preview sets that
/// cover it, falling back to preview page 0 to infer the thumbnails-per-preview
/// count when the default (20) assumption misses.
pub async fn resolve_online_token(
    site: u8,
    gid: u64,
    token: &str,
    index: u32,
) -> EhResult<String> {
    const DEFAULT_PER_PAGE: u32 = 20;
    let items = get_preview_set(site, gid, token, preview_page_of(index, DEFAULT_PER_PAGE)).await?;
    if let Some(t) = token_for_index(&items, index) {
        return Ok(t);
    }
    let first = get_preview_set(site, gid, token, 0).await?;
    if let Some(t) = token_for_index(&first, index) {
        return Ok(t);
    }
    let per_page = if first.is_empty() {
        DEFAULT_PER_PAGE
    } else {
        first.len() as u32
    };
    if per_page != DEFAULT_PER_PAGE {
        let items = get_preview_set(site, gid, token, preview_page_of(index, per_page)).await?;
        if let Some(t) = token_for_index(&items, index) {
            return Ok(t);
        }
    }
    Err(EhError::Parse(format!("无法解析第 {index} 页的 p_token")))
}


/// Fetches gallery metadata in bulk via the `gdata` API.
pub async fn get_gallery_metadata(
    site: u8,
    pairs: Vec<(u64, String)>,
) -> EhResult<Vec<GalleryInfo>> {
    ensure_site(site);
    let body = client::post_json(
        &url::api_url(site),
        &serde_json::json!({
            "method": "gdata",
            "gidlist": pairs.iter().map(|(g, t)| serde_json::json!([g, t])).collect::<Vec<_>>(),
            "namespace": 1,
        }),
    )
    .await?;
    let mut items = api::parse_gdata(&body)?;
    for item in items.iter_mut() {
        item.thumb = apply_thumb(item.thumb.take());
    }
    Ok(items)
}

/// Resolves a per-page token via the `showpage` API.
pub async fn resolve_showpage(
    site: u8,
    gids: Vec<u64>,
    page: u32,
) -> EhResult<GalleryToken> {
    ensure_site(site);
    let body = client::post_json(
        &url::api_url(site),
        &serde_json::json!({
            "method": "showpage",
            "gidlist": gids.iter().map(|g| vec![*g]).collect::<Vec<_>>(),
            "page": page + 1,
        }),
    )
    .await?;
    api::parse_showpage(&body)
}





#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_page_of_floors_index() {
        assert_eq!(preview_page_of(0, 20), 0);
        assert_eq!(preview_page_of(19, 20), 0);
        assert_eq!(preview_page_of(20, 20), 1);
        assert_eq!(preview_page_of(21, 20), 1);
        assert_eq!(preview_page_of(5, 0), 0);
    }

    #[test]
    fn token_for_index_finds_matching_item() {
        let items = vec![
            PreviewItem { index: 0, image_url: "".into(), page_url: "https://e-hentai.org/s/aaaaaaaaaa/12345-1".into(), width: None, height: None, x_offset: None, y_offset: None, clip_width: None, clip_height: None },
            PreviewItem { index: 1, image_url: "".into(), page_url: "https://e-hentai.org/s/bbbbbbbbbb/12345-2".into(), width: None, height: None, x_offset: None, y_offset: None, clip_width: None, clip_height: None },
        ];
        assert_eq!(token_for_index(&items, 0), Some("aaaaaaaaaa".into()));
        assert_eq!(token_for_index(&items, 1), Some("bbbbbbbbbb".into()));
        assert_eq!(token_for_index(&items, 2), None);
    }
}
