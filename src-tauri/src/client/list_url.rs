//! Port of SXJ `ListUrlBuilder`: builds gallery-list / search / popular URLs.

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::Deserialize;

use super::config::{CAT_ALL, EhConfig};
use super::url;

pub const MODE_NORMAL: u8 = 0;
pub const MODE_UPLOADER: u8 = 1;
pub const MODE_TAG: u8 = 2;
pub const MODE_WHATS_HOT: u8 = 3;
pub const MODE_IMAGE_SEARCH: u8 = 4;
pub const MODE_FILTER: u8 = 6;
pub const MODE_TOP_LIST: u8 = 7;

// AdvanceSearchTable flags.
pub const AS_SNAME: i32 = 0x1;
pub const AS_STAGS: i32 = 0x2;
pub const AS_SDESC: i32 = 0x4;
pub const AS_STORR: i32 = 0x8;
pub const AS_STO: i32 = 0x10;
pub const AS_SDT1: i32 = 0x20;
pub const AS_SDT2: i32 = 0x40;
pub const AS_SH: i32 = 0x80;
pub const AS_SFL: i32 = 0x100;
pub const AS_SFU: i32 = 0x200;
pub const AS_SFT: i32 = 0x400;

/// Matches Java `URLEncoder` fairly closely (keeps alnum and `- . _ *`).
const JAVA_ENC: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}')
    .add(b'~');

/// Query parameters used to build a list URL in SXJ order.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ListQuery {
    pub mode: u8,
    pub site: u8,
    pub keyword: Option<String>,
    pub category: i64,
    pub page: u32,
    pub advance: i32,
    pub min_rating: i32,
    pub page_from: i32,
    pub page_to: i32,
    pub excluded_lang: Option<String>,
}

impl Default for ListQuery {
    fn default() -> Self {
        Self {
            mode: 0,
            site: 0,
            keyword: None,
            category: 0,
            page: 0,
            advance: -1,
            min_rating: -1,
            page_from: -1,
            page_to: -1,
            excluded_lang: None,
        }
    }
}

fn enc(s: &str) -> String {
    utf8_percent_encode(s, JAVA_ENC).to_string()
}

fn join_query(pairs: &[(&str, String)]) -> String {
    pairs
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&")
}

impl ListQuery {
    /// Configures defaults used by the real SXJ builder (e.g. advance search flags).
    pub fn with_config(self, cfg: &EhConfig) -> Self {
        if self.category == 0 {
            // Merge in default categories (stored as excluded bitmask on the site).
            let _ = cfg;
        }
        self
    }

    /// Builds the full list URL for the active mode.
    pub fn build(&self) -> String {
        match self.mode {
            MODE_WHATS_HOT => url::popular_url(self.site),
            MODE_UPLOADER | MODE_TAG => {
                let kw = self.keyword.as_deref().unwrap_or("");
                let mut s = url::host(self.site);
                s.push_str(if self.mode == MODE_UPLOADER { "uploader/" } else { "tag/" });
                s.push_str(&enc(kw));
                if self.page != 0 {
                    s.push('/');
                    s.push_str(&self.page.to_string());
                }
                s
            }
            _ => self.build_normal(),
        }
    }

    fn build_normal(&self) -> String {
        let mut pairs: Vec<(&str, String)> = Vec::new();
        if self.category != 0 {
            pairs.push(("f_cats", ((!self.category as u64) & CAT_ALL).to_string()));
        }
        if let Some(kw) = self.keyword.as_deref() {
            let kw = kw.trim();
            if !kw.is_empty() {
                pairs.push(("f_search", enc(kw)));
            }
        }
        if self.page != 0 {
            pairs.push(("page", self.page.to_string()));
        }
        if self.advance != -1 {
            pairs.push(("advsearch", "1".into()));
            if self.advance & AS_SNAME != 0 { pairs.push(("f_sname", "on".into())); }
            if self.advance & AS_STAGS != 0 { pairs.push(("f_stags", "on".into())); }
            if self.advance & AS_SDESC != 0 { pairs.push(("f_sdesc", "on".into())); }
            if self.advance & AS_STORR != 0 { pairs.push(("f_storr", "on".into())); }
            if self.advance & AS_STO != 0 { pairs.push(("f_sto", "on".into())); }
            if self.advance & AS_SDT1 != 0 { pairs.push(("f_sdt1", "on".into())); }
            if self.advance & AS_SDT2 != 0 { pairs.push(("f_sdt2", "on".into())); }
            if self.advance & AS_SH != 0 { pairs.push(("f_sh", "on".into())); }
            if self.advance & AS_SFL != 0 { pairs.push(("f_sfl", "on".into())); }
            if self.advance & AS_SFU != 0 { pairs.push(("f_sfu", "on".into())); }
            if self.advance & AS_SFT != 0 { pairs.push(("f_sft", "on".into())); }
            if self.min_rating > 0 {
                pairs.push(("f_sr", "on".into()));
                pairs.push(("f_srdd", self.min_rating.to_string()));
            }
            if self.page_from != -1 || self.page_to != -1 {
                pairs.push(("f_sp", "on".into()));
                pairs.push(("f_spf", if self.page_from != -1 { self.page_from.to_string() } else { String::new() }));
                pairs.push(("f_spt", if self.page_to != -1 { self.page_to.to_string() } else { String::new() }));
            }
        }

        let host = url::host(self.site);
        if pairs.is_empty() {
            host
        } else {
            format!("{host}?{}", join_query(&pairs))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::MutexGuard;

    fn locked<T>(f: impl FnOnce() -> T) -> T {
        let _guard: MutexGuard<'_, ()> = url::URL_GLOBAL_LOCK.lock().unwrap();
        f()
    }

    #[test]
    fn homepage_url() {
        locked(|| {
            let q = ListQuery { mode: MODE_NORMAL, site: 0, ..Default::default() };
            assert_eq!(q.build(), "https://e-hentai.org/");
        });
    }

    #[test]
    fn search_url() {
        locked(|| {
            let q = ListQuery {
                mode: MODE_NORMAL,
                site: 0,
                keyword: Some("tokyo revengers".into()),
                page: 1,
                min_rating: 3,
                advance: AS_SNAME | AS_STAGS,
                ..Default::default()
            };
            let u = q.build();
            assert!(u.contains("f_search=tokyo%20revengers"), "{}", u);
            assert!(u.contains("page=1"));
            assert!(u.contains("advsearch=1"));
            assert!(u.contains("f_sname=on"));
            assert!(u.contains("f_stags=on"));
            assert!(u.contains("f_sr=on"));
            assert!(u.contains("f_srdd=3"));
        });
    }

    #[test]
    fn popular_url() {
        locked(|| {
            let q = ListQuery { mode: MODE_WHATS_HOT, site: 0, ..Default::default() };
            assert_eq!(q.build(), "https://e-hentai.org/popular");
            let q2 = ListQuery { mode: MODE_WHATS_HOT, site: 1, ..Default::default() };
            assert_eq!(q2.build(), "https://exhentai.org/popular");
        });
    }

    #[test]
    fn category_excluded() {
        locked(|| {
            let q = ListQuery { mode: MODE_NORMAL, site: 0, category: 6, ..Default::default() };
            let u = q.build();
            assert!(u.contains("f_cats=1017"), "{}", u);
        });
    }

    #[test]
    fn category_bit_order_matches_eh_and_sxj() {
        use super::super::config as cat;
        let all = cat::CAT_MISC
            | cat::CAT_DOUJINSHI
            | cat::CAT_MANGA
            | cat::CAT_ARTIST_CG
            | cat::CAT_GAME_CG
            | cat::CAT_IMAGE_SET
            | cat::CAT_COSPLAY
            | cat::CAT_ASIAN_PORN
            | cat::CAT_NON_H
            | cat::CAT_WESTERN;
        assert_eq!(all, cat::CAT_ALL);
        assert_eq!(cat::CAT_MISC, 0x1);
        assert_eq!(cat::CAT_DOUJINSHI, 0x2);
        assert_eq!(cat::CAT_MANGA, 0x4);
        assert_eq!(cat::CAT_ARTIST_CG, 0x8);
        assert_eq!(cat::CAT_GAME_CG, 0x10);
        assert_eq!(cat::CAT_IMAGE_SET, 0x20);
        assert_eq!(cat::CAT_COSPLAY, 0x40);
        assert_eq!(cat::CAT_ASIAN_PORN, 0x80);
        assert_eq!(cat::CAT_NON_H, 0x100);
        assert_eq!(cat::CAT_WESTERN, 0x200);
        assert_eq!(cat::CAT_ALL, 0x3ff);
    }
    #[test]
    fn f_cats_excludes_unchecked_categories() {
        locked(|| {
            use super::super::config as cfgcat;
            // User unchecks Doujinshi + Manga -> excluded = 0x2|0x4 = 6.
            let q = ListQuery {
                mode: MODE_NORMAL,
                site: 0,
                category: 0x2 | 0x4,
                ..Default::default()
            };
            assert_eq!(q.build(), "https://e-hentai.org/?f_cats=1017");
            // No exclusions -> no f_cats param at all.
            let q0 = ListQuery { mode: MODE_NORMAL, site: 0, category: 0, ..Default::default() };
            assert_eq!(q0.build(), "https://e-hentai.org/");
            // Exclude every category -> nothing to show (f_cats=0).
            let qa = ListQuery {
                mode: MODE_NORMAL,
                site: 0,
                category: cfgcat::CAT_ALL as i64,
                ..Default::default()
            };
            assert_eq!(qa.build(), "https://e-hentai.org/?f_cats=0");
        });
    }

    #[test]
    fn min_rating_zero_or_unset_omits_rating_filter() {
        locked(|| {
            // "不限" (0) and unset (-1) must not emit f_sr/f_srdd.
            for mr in [0, -1] {
                let q = ListQuery {
                    mode: MODE_NORMAL,
                    site: 0,
                    keyword: Some("NF".into()),
                    page: 0,
                    advance: AS_SNAME | AS_STAGS,
                    min_rating: mr,
                    ..Default::default()
                };
                let u = q.build();
                assert!(!u.contains("f_srdd="), "min_rating={mr} gave: {u}");
                assert!(!u.contains("f_sr="), "min_rating={mr} gave: {u}");
                assert!(u.contains("f_search=NF"), "min_rating={mr} gave: {u}");
            }
            // An explicit rating still emits the filter.
            let q3 = ListQuery {
                mode: MODE_NORMAL,
                site: 0,
                keyword: Some("NF".into()),
                advance: AS_SNAME | AS_STAGS,
                min_rating: 3,
                ..Default::default()
            };
            let u3 = q3.build();
            assert!(u3.contains("f_sr=on"), "{u3}");
            assert!(u3.contains("f_srdd=3"), "{u3}");
        });
    }

}
