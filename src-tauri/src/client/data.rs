//! Data models shared by parsers and commands (mirrors SXJ `data/*`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryInfo {
    pub gid: u64,
    pub token: String,
    pub title: String,
    pub title_jpn: Option<String>,
    pub thumb: Option<String>,
    pub category: String,
    pub posted: String,
    pub uploader: String,
    pub rating: f32,
    pub pages: u32,
    pub thumb_width: u32,
    pub thumb_height: u32,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "is_false", default)]
    pub rated: bool,
    /// Tracks whether this item is already in a local download list / favorite.
    #[serde(skip)]
    pub favorite_slot: i32,
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// Pagination links, extracted from the list navigation UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListNav {
    pub pages: i32,
    pub next_page: i32,
    pub result_count: String,
    pub first: Option<String>,
    pub prev: Option<String>,
    pub next: Option<String>,
    pub last: Option<String>,
}

/// Full result of `get_gallery_list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryListResult {
    pub items: Vec<GalleryInfo>,
    pub nav: ListNav,
    /// The absolute URL that produced this page, so the UI can reload it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_url: Option<String>,
}

/// A namespace group of tags on the detail page (e.g. "language", "artist").
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagGroup {
    pub name: String,
    pub tags: Vec<String>,
}

/// A single comment on the detail page.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryComment {
    pub id: Option<u32>,
    pub user: String,
    pub avatar: Option<String>,
    pub score: i32,
    pub time: String,
    pub comment: String,
}

/// Full gallery detail parsed from the detail page.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryDetail {
    pub gid: u64,
    pub token: String,
    pub title: String,
    pub title_jpn: Option<String>,
    pub thumb: Option<String>,
    pub category: String,
    pub uploader: String,
    pub posted: String,
    pub language: String,
    pub size: String,
    pub pages: u32,
    pub favorite_count: u32,
    pub rating: f32,
    pub rating_count: u32,
    pub torrent_count: u32,
    pub torrent_url: String,
    pub archive_url: String,
    pub is_favorited: bool,
    pub favorite_name: Option<String>,
    #[serde(default)]
    pub tags: Vec<TagGroup>,
    #[serde(default)]
    pub comments: Vec<GalleryComment>,
    pub preview_pages: i32,
}

/// A preview thumbnail item within a preview set.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewItem {
    pub index: u32,
    pub image_url: String,
    pub page_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// Sprite cut-out: pixel offset from the image top-left (when thumbnails are
    /// served as a shared montage / sprite, each page is a crop region).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_offset: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y_offset: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_height: Option<u32>,
}

/// The online page for a single gallery image.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlinePage {
    pub gid: u64,
    pub index: u32,
    pub image_url: String,
    pub origin_image_url: Option<String>,
    pub show_key: Option<String>,
}

/// One row of the `gdata` token response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryToken {
    pub gid: u64,
    pub token: String,
}
