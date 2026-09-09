//! Parsers for the JSON endpoints (`api.php` method=gdata / showpage), ported from
//! SXJ `GalleryApiParser` + `GalleryTokenApiParser`.

use serde::Deserialize;
use serde_json::Value;

use super::super::data::{GalleryInfo, GalleryToken};
use super::super::err::{EhError, EhResult};
use super::gallery_list::canonical_category;

#[derive(Debug, Deserialize)]
struct GdataMeta {
    #[serde(default)]
    gid: u64,
    #[serde(default)]
    token: String,
    title: Option<String>,
    #[serde(rename = "title_jpn", default)]
    title_jpn: Option<String>,
    category: Option<String>,
    thumb: Option<String>,
    uploader: Option<String>,
    posted: Option<String>,
    rating: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(rename = "filecount", default)]
    filecount: Option<String>,
}

/// Converts a unix-seconds timestamp (as string) to a `YYYY-MM-DD HH:MM` string.
fn format_posted(raw: &str) -> String {
    let Ok(secs) = raw.parse::<i64>() else {
        return String::new();
    };
    let dt = match chrono_local(secs) {
        Some(s) => s,
        None => return String::new(),
    };
    dt
}

// Use a lightweight manual conversion to avoid pulling in chrono/timezone data.
fn chrono_local(secs: i64) -> Option<String> {
    // seconds -> (year, month, day, hh, mm) in local time via a small civil algorithm.
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    Some(format!("{:04}-{:02}-{:02} {:02}:{:02}", y, m, d, hh, mm))
}

/// Howard Hinnant's `civil_from_days` implementation (public domain).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Parses a `gdata` response: `{"gmetadata": [ {...}, ... ]}`.
pub fn parse_gdata(json: &str) -> EhResult<Vec<GalleryInfo>> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| EhError::Parse(format!("gdata JSON: {e}")))?;
    let metas = value
        .get("gmetadata")
        .and_then(|v| v.as_array())
        .ok_or_else(|| EhError::Parse("gdata: missing gmetadata array".into()))?;

    let mut out = Vec::new();
    for m in metas {
        let meta: GdataMeta = serde_json::from_value(m.clone())
            .map_err(|e| EhError::Parse(format!("gdata meta: {e}")))?;
        let posted_ts = meta.posted.unwrap_or_default();
        out.push(GalleryInfo {
            gid: meta.gid,
            token: meta.token,
            title: meta.title.unwrap_or_default(),
            title_jpn: if meta.title_jpn.as_deref().unwrap_or("").is_empty() {
                None
            } else {
                meta.title_jpn
            },
            category: canonical_category(meta.category.as_deref().unwrap_or("")),
            thumb: meta.thumb,
            uploader: meta.uploader.unwrap_or_default(),
            posted: format_posted(&posted_ts),
            rating: meta.rating.as_deref().and_then(|r| r.parse().ok()).unwrap_or(0.0),
            pages: meta.filecount.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0),
            tags: meta.tags,
            ..Default::default()
        });
    }
    Ok(out)
}

/// Parses a `showpage` response into a per-gallery token, e.g.
/// `{ "token_12345": "abc123" }`.
pub fn parse_showpage(json: &str) -> EhResult<GalleryToken> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| EhError::Parse(format!("showpage JSON: {e}")))?;
    let obj = value
        .as_object()
        .ok_or_else(|| EhError::Parse("showpage: not an object".into()))?;
    for (k, v) in obj {
        if let Some(gid_str) = k.strip_prefix("token_") {
            if let Ok(gid) = gid_str.parse::<u64>() {
                if let Some(tok) = v.as_str() {
                    return Ok(GalleryToken {
                        gid,
                        token: tok.to_string(),
                    });
                }
            }
        }
    }
    Err(EhError::Parse("showpage: no token found".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gdata_basic() {
        let json = r#"{"gmetadata":[
            {"gid":1001,"token":"aaabbb","title":"My Gallery","title_jpn":"MyJ",
             "category":"doujinshi","thumb":"https://ehgt.org/x/1.jpg","uploader":"u1",
             "posted":"1700000000","rating":"4.5","tags":["language:english","artist:x"],
             "filecount":"40"}
        ]}"#;
        let v = parse_gdata(json).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].gid, 1001);
        assert_eq!(v[0].token, "aaabbb");
        assert_eq!(v[0].category, "doujinshi");
        assert_eq!(v[0].pages, 40);
        assert_eq!(v[0].tags.len(), 2);
        assert_eq!(v[0].title_jpn.as_deref(), Some("MyJ"));
    }

    #[test]
    fn showpage_token() {
        let json = r#"{"token_4242":"deadbeef"}"#;
        let t = parse_showpage(json).unwrap();
        assert_eq!(t.gid, 4242);
        assert_eq!(t.token, "deadbeef");
    }

    #[test]
    fn civil() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(18423), (2020, 6, 10));
    }
}
