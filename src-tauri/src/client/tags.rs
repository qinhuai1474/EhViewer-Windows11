//! Tag translation database (port of SXJ `EhTagDatabase`) + namespace/prefix map.
//!
//! File format (official EhTagDatabase binary): a big-endian `int32` blob length,
//! then a UTF-8 blob whose lines are `<key>\r<base64-utf8(translation)>\n`, sorted
//! by `key` bytes. `key` is usually `<prefix>:<name>` (e.g. `l:english`); tags in
//! the `misc` namespace have no prefix. Translation lookups only happen when a DB
//! has been imported AND tag translation is enabled.

use std::sync::OnceLock;
use std::sync::RwLock;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;

/// SXJ `NAMESPACE_TO_PREFIX`. `misc` maps to the empty prefix.
const NAMESPACE_TO_PREFIX: &[(&str, &str)] = &[
    ("rows", "n:"),
    ("artist", "a:"),
    ("cosplayer", "cos:"),
    ("character", "c:"),
    ("female", "f:"),
    ("group", "g:"),
    ("language", "l:"),
    ("location", "loc:"),
    ("male", "m:"),
    ("misc", ""),
    ("mixed", "x:"),
    ("other", "o:"),
    ("parody", "p:"),
    ("reclass", "r:"),
];

pub fn namespace_to_prefix(namespace: &str) -> Option<&'static str> {
    NAMESPACE_TO_PREFIX
        .iter()
        .find(|(n, _)| *n == namespace)
        .map(|(_, p)| *p)
}

/// A parsed translation database: `(key, translation)` sorted by key bytes.
#[derive(Debug, Clone, Default)]
pub struct TagDb {
    entries: Vec<(Vec<u8>, String)>,
}

impl TagDb {
    /// Parses the text blob (after the leading `int32` length).
    pub fn parse_blob(blob: &[u8]) -> TagDb {
        let text = String::from_utf8_lossy(blob).into_owned();
        let mut entries = Vec::new();
        for line in text.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let Some((key, b64)) = line.split_once('\r') else {
                continue;
            };
            if key.is_empty() {
                continue;
            }
            if let Ok(bytes) = B64.decode(b64) {
                let translation = String::from_utf8_lossy(&bytes).into_owned();
                entries.push((key.as_bytes().to_vec(), translation));
            }
        }
        // The official file is pre-sorted, but re-sort defensively so binary
        // search in `translate` stays correct for hand-made fixtures too.
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        TagDb { entries }
    }

    /// Builds a `TagDb` from the full official file bytes (int32 length + blob).
    pub fn load_file(bytes: &[u8]) -> Result<TagDb, String> {
        if bytes.len() < 4 {
            return Err("文件过短，不是有效的 EhTagDatabase".into());
        }
        let blob_len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let blob = bytes
            .get(4..4 + blob_len)
            .ok_or_else(|| "文件不完整（长度字段超出实际数据）".to_string())?;
        Ok(TagDb::parse_blob(blob))
    }

    /// Number of loaded entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Looks up `key` (prefix form like `l:english`) and returns its translation.
    pub fn translate(&self, key: &[u8]) -> Option<&str> {
        let idx = self
            .entries
            .binary_search_by(|(ek, _)| ek.as_slice().cmp(key))
            .ok()?;
        Some(self.entries[idx].1.as_str())
    }
}

/// Installed DB + enabled flag (mirrors SXJ's singleton `EhTagDatabase.instance`
/// plus the "tag translation enabled" setting).
fn db_slot() -> &'static RwLock<TagDb> {
    static DB: OnceLock<RwLock<TagDb>> = OnceLock::new();
    DB.get_or_init(|| RwLock::new(TagDb::default()))
}

fn enabled_slot() -> &'static RwLock<bool> {
    static ENABLED: OnceLock<RwLock<bool>> = OnceLock::new();
    ENABLED.get_or_init(|| RwLock::new(false))
}

/// Installs a parsed database replacing the previous one (if any).
pub fn install(db: TagDb) {
    *db_slot().write().unwrap() = db;
}

/// `true` when translation is currently enabled (toggled from settings).
pub fn set_enabled(enabled: bool) {
    *enabled_slot().write().unwrap() = enabled;
}

pub fn enabled() -> bool {
    *enabled_slot().read().unwrap()
}

/// Looks up a full prefix key; returns the translation when found.
pub fn lookup(key: &str) -> Option<String> {
    if !enabled() {
        return None;
    }
    db_slot()
        .read()
        .unwrap()
        .translate(key.as_bytes())
        .map(str::to_string)
}

/// Builds the query key for a detail-page tag given its group namespace.
fn build_query_with_namespace(namespace: &str, tag: &str) -> Option<String> {
    let ns = namespace.trim_end_matches(':').trim().to_ascii_lowercase();
    match namespace_to_prefix(&ns) {
        Some("") => Some(tag.to_string()),
        Some(prefix) => Some(format!("{prefix}{tag}")),
        None => None,
    }
}

/// Builds the query key for a raw tag (list `title` attributes are either
/// `namespace:name` or already-prefixed, e.g. `l:source`).
fn build_query_raw(tag: &str) -> Option<String> {
    if let Some((ns, name)) = tag.split_once(':') {
        // Site list tags are full namespace ("language:english"); prefixed keys
        // ("l:english") also resolve through the same table.
        let prefix = namespace_to_prefix(ns.trim().to_ascii_lowercase().as_str())?;
        Some(format!("{prefix}{name}"))
    } else {
        Some(tag.to_string())
    }
}

/// Translates a detail tag given its group display namespace; returns the raw
/// tag unchanged when disabled, unmapped, or not found.
pub fn translate_detail_tag(namespace: &str, tag: &str) -> String {
    if !enabled() {
        return tag.to_string();
    }
    build_query_with_namespace(namespace, tag)
        .and_then(|q| {
            db_slot()
                .read()
                .unwrap()
                .translate(q.as_bytes())
                .map(str::to_string)
        })
        .unwrap_or_else(|| tag.to_string())
}

/// Translates a raw list tag (from `title` attributes); returns it unchanged
/// when disabled, unmapped, or not found.
pub fn translate_list_tag(tag: &str) -> String {
    if !enabled() {
        return tag.to_string();
    }
    build_query_raw(tag)
        .and_then(|q| {
            db_slot()
                .read()
                .unwrap()
                .translate(q.as_bytes())
                .map(str::to_string)
        })
        .unwrap_or_else(|| tag.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a synthetic official-format file from `(key, translation)` pairs.
    fn build_file(pairs: &[(&str, &str)]) -> Vec<u8> {
        let mut lines: Vec<String> = pairs
            .iter()
            .map(|(k, v)| {
                let b64 = B64.encode(v.as_bytes());
                format!("{k}\r{b64}")
            })
            .collect();
        lines.sort();
        let blob = lines.join("\n").into_bytes();
        let len = (blob.len() as u32).to_be_bytes();
        let mut out = Vec::new();
        out.extend_from_slice(&len);
        out.extend_from_slice(&blob);
        out
    }

    #[test]
    fn parses_and_translates_sxj_fixture() {
        // Mirrors EhTagDatabaseTest.readTheList: sorted keys "1","12",...
        let pairs = [
            ("1", "a"),
            ("12", "ab"),
            ("123", "abc"),
            ("1234", "abcd"),
            ("a", "1"),
            ("ab", "12"),
            ("abc", "123"),
            ("abcd", "1234"),
        ];
        let bytes = build_file(&pairs);
        let db = TagDb::load_file(&bytes).unwrap();
        assert_eq!(db.len(), 8);
        assert_eq!(db.translate(b"1"), Some("a"));
        assert_eq!(db.translate(b"1234"), Some("abcd"));
        assert_eq!(db.translate(b"abcd"), Some("1234"));
        assert_eq!(db.translate(b"21"), None);
    }

    #[test]
    fn location_prefix_lookup() {
        let bytes = build_file(&[("loc:beach", "沙滩")]);
        let db = TagDb::load_file(&bytes).unwrap();
        assert_eq!(db.translate(b"loc:beach"), Some("沙滩"));
    }

    #[test]
    fn misc_namespace_resolves_without_prefix() {
        let bytes = build_file(&[("源", "source"), ("parody:hunter", "猎人")]);
        let db = TagDb::load_file(&bytes).unwrap();
        assert_eq!(db.translate("源".as_bytes()), Some("source"));
        assert_eq!(db.translate(b"parody:hunter"), Some("猎人"));
    }

    #[test]
    fn disabled_returns_raw() {
        install(TagDb::load_file(&build_file(&[("l:english", "英语")])).unwrap());
        set_enabled(false);
        assert_eq!(translate_detail_tag("Language", "english"), "english");
        set_enabled(true);
        assert_eq!(translate_detail_tag("Language", "english"), "英语");
        assert_eq!(translate_list_tag("language:english"), "英语");
    }

    #[test]
    fn unmapped_or_missing_returns_raw() {
        install(TagDb::load_file(&build_file(&[("a:bob", "鲍勃")])).unwrap());
        set_enabled(true);
        assert_eq!(translate_detail_tag("Unknown", "zzz"), "zzz");
        assert_eq!(translate_list_tag("language:english"), "language:english");
        assert_eq!(translate_detail_tag("Artist", "noentry"), "noentry");
    }
}
