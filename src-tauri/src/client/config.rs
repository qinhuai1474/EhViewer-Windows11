//! Port of SXJ `EhConfig`: configurable E-Hentai settings that are encoded
//! into the `uconfig` cookie value.

use serde::{Deserialize, Serialize};

/// Cookie key that holds the uconfig value.
pub const KEY_UCONFIG: &str = "uconfig";

// Value constants (kept identical to SXJ).
pub const LOAD_FROM_HAH_YES: &str = "y";
pub const LOAD_FROM_HAH_NO: &str = "n";
pub const IMAGE_SIZE_AUTO: &str = "a";
pub const IMAGE_SIZE_780X: &str = "780";
pub const IMAGE_SIZE_980X: &str = "980";
pub const IMAGE_SIZE_1280X: &str = "1280";
pub const IMAGE_SIZE_1600X: &str = "1600";
pub const IMAGE_SIZE_2400X: &str = "2400";
pub const GALLERY_TITLE_DEFAULT: &str = "r";
pub const GALLERY_TITLE_JAPANESE: &str = "j";
pub const ARCHIVER_DOWNLOAD_MAMS: &str = "0";
pub const ARCHIVER_DOWNLOAD_AAMS: &str = "1";
pub const ARCHIVER_DOWNLOAD_MAAS: &str = "2";
pub const ARCHIVER_DOWNLOAD_AAAS: &str = "3";
pub const LAYOUT_MODE_LIST: &str = "l";
pub const LAYOUT_MODE_THUMB: &str = "t";
pub const POPULAR_YES: &str = "y";
pub const POPULAR_NO: &str = "n";
pub const FAVORITES_SORT_GALLERY_UPDATE_TIME: &str = "p";
pub const FAVORITES_SORT_FAVORITED_TIME: &str = "f";
pub const RESULT_COUNT_25: &str = "0";
pub const RESULT_COUNT_50: &str = "1";
pub const RESULT_COUNT_100: &str = "2";
pub const RESULT_COUNT_200: &str = "3";
pub const MOUSE_OVER_YES: &str = "m";
pub const MOUSE_OVER_NO: &str = "p";
pub const PREVIEW_SIZE_NORMAL: &str = "m";
pub const PREVIEW_SIZE_LARGE: &str = "l";
pub const PREVIEW_ROW_4: &str = "2";
pub const COMMENTS_SORT_OLDEST_FIRST: &str = "a";
pub const COMMENTS_SORT_RECENT_FIRST: &str = "d";
pub const COMMENTS_VOTES_POP: &str = "0";
pub const COMMENTS_VOTES_ALWAYS: &str = "1";
pub const TAGS_SORT_ALPHABETICAL: &str = "a";
pub const TAGS_SORT_POWER: &str = "p";
pub const SHOW_GALLERY_INDEX_YES: &str = "1";
pub const SHOW_GALLERY_INDEX_NO: &str = "0";
pub const ENABLE_TAG_FLAGGING_YES: &str = "y";
pub const ENABLE_TAG_FLAGGING_NO: &str = "n";
pub const ALWAYS_ORIGINAL_YES: &str = "y";
pub const ALWAYS_ORIGINAL_NO: &str = "n";
pub const MULTI_PAGE_YES: &str = "y";
pub const MULTI_PAGE_NO: &str = "n";
pub const MULTI_PAGE_STYLE_N: &str = "n";
pub const MULTI_PAGE_THUMB_SHOW: &str = "n";

// Category flags.
pub const CAT_MISC: u64 = 0x1;
pub const CAT_DOUJINSHI: u64 = 0x2;
pub const CAT_MANGA: u64 = 0x4;
pub const CAT_ARTIST_CG: u64 = 0x8;
pub const CAT_GAME_CG: u64 = 0x10;
pub const CAT_IMAGE_SET: u64 = 0x20;
pub const CAT_COSPLAY: u64 = 0x40;
pub const CAT_ASIAN_PORN: u64 = 0x80;
pub const CAT_NON_H: u64 = 0x100;
pub const CAT_WESTERN: u64 = 0x200;
pub const CAT_ALL: u64 = 0x3ff;

/// The uconfig fields, in the same order SXJ serialises them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EhConfig {
    pub load_from_hah: String,
    pub image_size: String,
    pub scale_width: i32,
    pub scale_height: i32,
    pub gallery_title: String,
    pub archiver_download: String,
    pub layout_mode: String,
    pub popular: String,
    pub default_categories: i64,
    pub favorites_sort: String,
    pub excluded_namespaces: i64,
    pub excluded_languages: String,
    pub result_count: String,
    pub mouse_over: String,
    pub preview_size: String,
    pub preview_row: String,
    pub comment_sort: String,
    pub comment_votes: String,
    pub tag_sort: String,
    pub show_gallery_index: String,
    pub hah_client_ip: String,
    pub hah_client_port: i32,
    pub hah_client_passkey: String,
    pub enable_tag_flagging: String,
    pub always_original: String,
    pub multi_page: String,
    pub multi_page_style: String,
    pub multi_page_thumb: String,
}

impl Default for EhConfig {
    fn default() -> Self {
        Self {
            load_from_hah: LOAD_FROM_HAH_YES.into(),
            image_size: IMAGE_SIZE_AUTO.into(),
            scale_width: 0,
            scale_height: 0,
            gallery_title: GALLERY_TITLE_DEFAULT.into(),
            archiver_download: ARCHIVER_DOWNLOAD_MAMS.into(),
            layout_mode: LAYOUT_MODE_LIST.into(),
            popular: POPULAR_YES.into(),
            default_categories: 0,
            favorites_sort: FAVORITES_SORT_FAVORITED_TIME.into(),
            excluded_namespaces: 0,
            excluded_languages: String::new(),
            result_count: RESULT_COUNT_25.into(),
            mouse_over: MOUSE_OVER_YES.into(),
            preview_size: PREVIEW_SIZE_LARGE.into(),
            preview_row: PREVIEW_ROW_4.into(),
            comment_sort: COMMENTS_SORT_OLDEST_FIRST.into(),
            comment_votes: COMMENTS_VOTES_POP.into(),
            tag_sort: TAGS_SORT_ALPHABETICAL.into(),
            show_gallery_index: SHOW_GALLERY_INDEX_YES.into(),
            hah_client_ip: String::new(),
            hah_client_port: -1,
            hah_client_passkey: String::new(),
            enable_tag_flagging: ENABLE_TAG_FLAGGING_NO.into(),
            always_original: ALWAYS_ORIGINAL_NO.into(),
            multi_page: MULTI_PAGE_NO.into(),
            multi_page_style: MULTI_PAGE_STYLE_N.into(),
            multi_page_thumb: MULTI_PAGE_THUMB_SHOW.into(),
        }
    }
}

impl EhConfig {
    /// Builds the `uconfig` cookie value, byte-identical to SXJ `updateUconfig()`.
    pub fn uconfig(&self) -> String {
        let hah_ip_port = if !self.hah_client_ip.is_empty() && self.hah_client_port > 0 {
            // SXJ percent-encodes ':' as %3A.
            format!("{}%3A{}", self.hah_client_ip, self.hah_client_port)
        } else {
            String::new()
        };
        [
            ("uh", &self.load_from_hah),
            ("xr", &self.image_size),
            ("rx", &self.scale_width.to_string()),
            ("ry", &self.scale_height.to_string()),
            ("tl", &self.gallery_title),
            ("ar", &self.archiver_download),
            ("dm", &self.layout_mode),
            ("prn", &self.popular),
            ("cats", &self.default_categories.to_string()),
            ("fs", &self.favorites_sort),
            ("xns", &self.excluded_namespaces.to_string()),
            ("xl", &self.excluded_languages),
            ("rc", &self.result_count),
            ("lt", &self.mouse_over),
            ("ts", &self.preview_size),
            ("tr", &self.preview_row),
            ("cs", &self.comment_sort),
            ("sc", &self.comment_votes),
            ("to", &self.tag_sort),
            ("pn", &self.show_gallery_index),
            ("hp", &hah_ip_port),
            ("hk", &self.hah_client_passkey),
            ("tf", &self.enable_tag_flagging),
            ("oi", &self.always_original),
            ("qb", &self.multi_page),
            ("ms", &self.multi_page_style),
            ("mt", &self.multi_page_thumb),
        ]
        .iter()
        .map(|(k, v)| format!("{}_{}", k, v))
        .collect::<Vec<_>>()
        .join("-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_uconfig() {
        let cfg = EhConfig::default();
        let u = cfg.uconfig();
        assert!(u.starts_with("uh_y-xr_a-rx_0-ry_0-tl_r-ar_0-dm_l-prn_y-"));
        assert!(u.ends_with("-tf_n-oi_n-qb_n-ms_n-mt_n"));
    }

    #[test]
    fn hah_encoded() {
        let mut cfg = EhConfig::default();
        cfg.hah_client_ip = "192.168.1.1".into();
        cfg.hah_client_port = 10001;
        let u = cfg.uconfig();
        assert!(u.contains("-hp_192.168.1.1%3A10001-"), "got: {}", u);
    }
}
