#![cfg_attr(mobile, tauri::mobile_entry_point)]

pub mod client;
pub mod db;
pub mod download;
pub mod dpapi;
pub mod ehimg;
pub mod settings;

use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};

use db::Db;
use download::DownloadManager;

/// Managed application state shared with commands.
pub struct AppState {
    pub db: Arc<Db>,
    pub settings_path: PathBuf,
    pub settings: Arc<std::sync::Mutex<settings::Settings>>,
}

/// Returns app metadata for the frontend.
#[tauri::command]
fn app_meta() -> serde_json::Value {
    serde_json::json!({
        "name": "EhViewer for Windows 11",
        "version": env!("CARGO_PKG_VERSION"),
        "modules": ["home", "popular", "downloads", "settings"],
    })
}

/// Runs an end-to-end network diagnostic for the current machine (proxy / DNS /
/// TCP / HTTPS probe) so connectivity trouble like "error sending request for
/// url (https://e-hentai.org/popular)" can be traced to its real cause.
#[tauri::command]
async fn network_diag() -> client::diag::NetDiag {
    client::diag::run().await
}

/// Fetches a gallery list page (normal or popular) and returns parsed items + navigation.
#[tauri::command]
async fn get_gallery_list(url: String) -> Result<client::data::GalleryListResult, String> {
    if !client::url::is_allowed_url(&url) {
        return Err("仅允许访问 E-Hentai / ExHentai 站点".to_string());
    }
    client::engine::get_gallery_list(&url)
        .await
        .map_err(|e| e.to_string())
}

/// Fetches gallery detail metadata.
#[tauri::command]
async fn get_gallery_detail(
    site: u8,
    gid: u64,
    token: String,
    index: u32,
) -> Result<client::data::GalleryDetail, String> {
    client::engine::get_gallery_detail(site, gid, &token, index)
        .await
        .map_err(|e| e.to_string())
}

/// Fetches a preview thumbnail set page.
#[tauri::command]
async fn get_preview_set(
    site: u8,
    gid: u64,
    token: String,
    index: u32,
) -> Result<Vec<client::data::PreviewItem>, String> {
    client::engine::get_preview_set(site, gid, &token, index)
        .await
        .map_err(|e| e.to_string())
}

/// Fetches one online reading page image.
#[tauri::command]
async fn get_online_page(
    site: u8,
    gid: u64,
    token: String,
    index: u32,
    p_token: String,
) -> Result<client::data::OnlinePage, String> {
    client::engine::get_online_page(site, gid, &token, index, &p_token)
        .await
        .map_err(|e| e.to_string())
}

/// Bulk gallery metadata via the gdata API.
#[tauri::command]
async fn get_gallery_metadata(
    site: u8,
    pairs: Vec<(u64, String)>,
) -> Result<Vec<client::data::GalleryInfo>, String> {
    client::engine::get_gallery_metadata(site, pairs)
        .await
        .map_err(|e| e.to_string())
}

/// Resolves a page token via the showpage API.
#[tauri::command]
async fn resolve_showpage(
    site: u8,
    gids: Vec<u64>,
    page: u32,
) -> Result<client::data::GalleryToken, String> {
    client::engine::resolve_showpage(site, gids, page)
        .await
        .map_err(|e| e.to_string())
}

/// Builds a list/search/popular URL from structured query parameters.
#[tauri::command]
fn build_list_url(
    site: u8,
    mode: u8,
    keyword: Option<String>,
    category: i64,
    page: u32,
    advance: i32,
    min_rating: i32,
    page_from: i32,
    page_to: i32,
) -> String {
    let q = client::list_url::ListQuery {
        mode,
        site,
        keyword,
        category,
        page,
        advance,
        min_rating,
        page_from,
        page_to,
        ..Default::default()
    };
    q.build()
}

/// Fetches an image via the backend (with proper UA/Referer/cookies) and
/// returns a `data:` URL the webview can render directly.
#[tauri::command]
async fn fetch_image(url: String) -> Result<String, String> {
    ehimg::data_url(&url).await
}

// ----- download commands -----

/// Creates (or resumes) a download for a gallery.
#[tauri::command]
async fn download_start(
    app: tauri::AppHandle,
    gid: u64,
    token: String,
    title: String,
    label: String,
    total: u32,
    url: String,
) -> Result<(), String> {
    let m = app.state::<Arc<DownloadManager>>();
    m.start_download(gid, token, title, label, total, url).await
}

/// Pauses a download, keeping its progress.
#[tauri::command]
async fn download_stop(app: tauri::AppHandle, gid: u64) -> Result<(), String> {
    let m = app.state::<Arc<DownloadManager>>();
    m.stop_download(gid).await
}

/// Removes a download; `erase` also deletes the files on disk.
#[tauri::command]
async fn download_delete(app: tauri::AppHandle, gid: u64, erase: bool) -> Result<(), String> {
    let m = app.state::<Arc<DownloadManager>>();
    m.delete_download(gid, erase).await
}

/// Renames a download's label.
#[tauri::command]
async fn download_relabel(app: tauri::AppHandle, gid: u64, label: String) -> Result<(), String> {
    let m = app.state::<Arc<DownloadManager>>();
    m.relabel_download(gid, &label)
}

/// Returns the full download list.
#[tauri::command]
async fn download_list(app: tauri::AppHandle) -> Result<Vec<db::DownloadDto>, String> {
    let m = app.state::<Arc<DownloadManager>>();
    m.list_downloads()
}


// ----- settings commands -----

const EV_SETTINGS_CHANGED: &str = "settings-changed";

/// Applies tag-translation state (enabled flag + imported file) to the singleton.
fn apply_tag_settings(s: &settings::Settings) {
    client::tags::set_enabled(s.tag_translation_enabled);
    if let Some(path) = &s.tag_translation_file {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(db) = client::tags::TagDb::load_file(&bytes) {
                client::tags::install(db);
            }
        }
    }
}

/// Returns the current settings (session cookies are decrypted in memory).
#[tauri::command]
fn settings_get(app: tauri::AppHandle) -> Result<settings::Settings, String> {
    let state = app.state::<AppState>();
    let guard = state.settings.lock().unwrap();
    Ok(guard.clone())
}

/// Updates a single settings key, persists, and applies it to the HTTP client.
#[tauri::command]
fn settings_set(
    app: tauri::AppHandle,
    key: String,
    value: serde_json::Value,
) -> Result<settings::Settings, String> {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut guard = state.settings.lock().unwrap();
        set_setting_field(&mut *guard, &key, &value)?;
        guard.save(&state.settings_path).map_err(|e| e.to_string())?;
        let snap = guard.clone();
        client::engine::apply_settings(&snap);
        ehimg::set_cache_max_mb(snap.image_cache_size_mb);
        client::client::set_max_retries(snap.max_retries);
        client::thumb::set_resolution(snap.thumb_resolution);
        apply_tag_settings(&snap);
        snap
    };
    let _ = app.emit(EV_SETTINGS_CHANGED, ());
    Ok(snapshot)
}

/// Replaces the session cookies (ipb_* / igneous); stored DPAPI-encrypted on disk.
#[tauri::command]
fn cookie_save(
    app: tauri::AppHandle,
    site: u8,
    pairs: Vec<settings::CookiePair>,
) -> Result<settings::Settings, String> {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut guard = state.settings.lock().unwrap();
        guard.site = site;
        guard.session_cookies = pairs.into_iter().filter(|c| !c.value.trim().is_empty()).collect();
        guard.save(&state.settings_path).map_err(|e| e.to_string())?;
        let snap = guard.clone();
        client::engine::apply_settings(&snap);
        ehimg::set_cache_max_mb(snap.image_cache_size_mb);
        client::client::set_max_retries(snap.max_retries);
        client::thumb::set_resolution(snap.thumb_resolution);
        snap
    };
    let _ = app.emit(EV_SETTINGS_CHANGED, ());
    Ok(snapshot)
}

/// Best-effort verification that `site` responds without a hard block.
#[tauri::command]
async fn cookie_verify(app: tauri::AppHandle, site: u8) -> Result<serde_json::Value, String> {
    let state = app.state::<AppState>();
    {
        let guard = state.settings.lock().unwrap();
        let mut preview = guard.clone();
        preview.site = site;
        client::engine::apply_settings(&preview);
    }
    let url = client::url::home_url(site);
    match client::client::get_text(&url, None).await {
        Ok(_) => Ok(serde_json::json!({ "ok": true, "message": "站点连接正常" })),
        Err(e) => Ok(serde_json::json!({ "ok": false, "message": format!("验证失败：{e}") })),
    }
}

/// Exports settings + downloads + reading progress as a JSON bundle.
#[tauri::command]
fn export_data(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    let downloads = state.db.list_downloads().map_err(|e| e.to_string())?;
    let reading = state.db.all_reading_progress().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "version": 1,
        "settings": settings,
        "downloads": downloads.iter().map(db::DownloadDto::from).collect::<Vec<_>>(),
        "reading_progress": reading
            .iter()
            .map(|(g, p)| serde_json::json!({ "gid": g, "page": p }))
            .collect::<Vec<_>>(),
    }))
}

/// Imports an EhTagDatabase binary so gallery tags get translated.
#[tauri::command]
fn import_tag_translation(app: tauri::AppHandle, path: String) -> Result<serde_json::Value, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("读取文件失败: {e}"))?;
    let db = client::tags::TagDb::load_file(&bytes)?;
    let count = db.len();
    client::tags::install(db);
    let state = app.state::<AppState>();
    {
        let mut guard = state.settings.lock().unwrap();
        guard.tag_translation_file = Some(path.clone());
        guard.save(&state.settings_path).map_err(|e| e.to_string())?;
    }
    apply_tag_settings(&state.settings.lock().unwrap().clone());
    let _ = app.emit(EV_SETTINGS_CHANGED, ());
    Ok(serde_json::json!({ "count": count, "path": path }))
}

/// Imports a bundle produced by `export_data`.
#[tauri::command]
fn import_data(
    app: tauri::AppHandle,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let state = app.state::<AppState>();
    let mut imported_downloads = 0usize;
    let mut imported_progress = 0usize;

    if let Some(dls) = payload.get("downloads").and_then(|v| v.as_array()) {
        let recs = dls.iter().filter_map(record_from_json).collect::<Vec<db::DownloadRecord>>();
        state.db.clear_downloads().map_err(|e| e.to_string())?;
        for r in &recs {
            state.db.upsert_download(r).map_err(|e| e.to_string())?;
        }
        imported_downloads = recs.len();
    }
    if let Some(ps) = payload.get("reading_progress").and_then(|v| v.as_array()) {
        for p in ps {
            let gid = p.get("gid").and_then(|v| v.as_u64()).unwrap_or(0);
            let page = p.get("page").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            if gid != 0 {
                state
                    .db
                    .save_reading_progress(gid, page)
                    .map_err(|e| e.to_string())?;
                imported_progress += 1;
            }
        }
    }
    if let Some(s) = payload.get("settings") {
        if let Ok(imported) = serde_json::from_value::<settings::Settings>(s.clone()) {
            let mut guard = state.settings.lock().unwrap();
            *guard = imported;
            guard.save(&state.settings_path).map_err(|e| e.to_string())?;
            let snap = guard.clone();
            client::engine::apply_settings(&snap);
            ehimg::set_cache_max_mb(snap.image_cache_size_mb);
        client::client::set_max_retries(snap.max_retries);
        client::thumb::set_resolution(snap.thumb_resolution);
            apply_tag_settings(&snap);
        }
    }
    let _ = app.emit(EV_SETTINGS_CHANGED, ());
    Ok(serde_json::json!({
        "imported_downloads": imported_downloads,
        "imported_progress": imported_progress,
    }))
}

/// Maps a settings key + value onto the Settings struct.
fn set_setting_field(s: &mut settings::Settings, key: &str, value: &serde_json::Value) -> Result<(), String> {
    let str_v = value.as_str().map(str::to_string);
    let some_str = str_v.clone().filter(|v| !v.is_empty());
    match key {
        "site" => s.site = value.as_u64().unwrap_or(0) as u8,
        "tag_translation_enabled" => s.tag_translation_enabled = value.as_bool().unwrap_or(false),
        "tag_translation_file" => s.tag_translation_file = some_str,
        "thumb_resolution" => s.thumb_resolution = value.as_u64().unwrap_or(0) as u8,
        "reading_direction" => s.reading_direction = str_v.unwrap_or_else(|| s.reading_direction.clone()),
        "zoom_mode" => s.zoom_mode = str_v.unwrap_or_else(|| s.zoom_mode.clone()),
        "reading_start_position" => s.reading_start_position = str_v.unwrap_or_else(|| s.reading_start_position.clone()),
        "download_dir" => s.download_dir = some_str,
        "download_threads" => s.download_threads = value.as_u64().unwrap_or(1) as u32,
        "download_always_original" => s.download_always_original = value.as_bool().unwrap_or(false),
        "download_list_page_size" => s.download_list_page_size = value.as_u64().unwrap_or(12) as u32,
        "download_interval_secs" => s.download_interval_secs = value.as_u64().unwrap_or(5) as u64,
        "proxy_type" => s.proxy_type = (value.as_u64().unwrap_or(0) as u8).min(3),
        "proxy_url" => s.proxy_url = some_str,
        "hosts_override" => s.hosts_override = str_v.unwrap_or_default(),
        "timeout_secs" => s.timeout_secs = value.as_u64().unwrap_or(30),
        "max_retries" => s.max_retries = value.as_u64().unwrap_or(3) as u32,
        "image_cache_size_mb" => s.image_cache_size_mb = value.as_u64().unwrap_or(100) as u32,
        "custom_host" => s.custom_host = some_str,
        "doh_url" => s.doh_url = str_v.unwrap_or_else(|| s.doh_url.clone()),
        "use_builtin_hosts" => s.use_builtin_hosts = value.as_bool().unwrap_or(true),
        _ => return Err(format!("未知设置项: {key}")),
    }
    Ok(())
}

/// Builds a download record from the export JSON shape (state may be a number or
/// the string form emitted by `DownloadDto`).
fn record_from_json(v: &serde_json::Value) -> Option<db::DownloadRecord> {
    Some(db::DownloadRecord {
        gid: v.get("gid")?.as_u64()?,
        token: v.get("token").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        title: v.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        label: v.get("label").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        state: state_from_json(v.get("state")),
        total: v.get("total").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
        complete: v.get("complete").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
        dir: v.get("dir").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        url: v.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string(),
    })
}

fn state_from_json(v: Option<&serde_json::Value>) -> i32 {
    match v {
        Some(val) => {
            if let Some(n) = val.as_i64() {
                return n as i32;
            }
            if let Some(s) = val.as_str() {
                return match s {
                    "wait" => 1,
                    "downloading" => 2,
                    "finished" => 3,
                    "failed" => 4,
                    _ => 0,
                };
            }
            0
        }
        None => 0,
    }
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    ehimg::register(tauri::Builder::default())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir().unwrap_or_else(|_| {
                // Never depend on a developer-local path: key off the per-user
                // AppData / HOME directory instead.
                let base = std::env::var_os("LOCALAPPDATA")
                    .or_else(|| std::env::var_os("APPDATA"))
                    .or_else(|| std::env::var_os("HOME"))
                    .unwrap_or_else(|| ".".into());
                std::path::PathBuf::from(base).join("EhViewer-Windows11")
            });
            std::fs::create_dir_all(&config_dir).ok();

            let settings_path = settings::Settings::default_path(&config_dir);
            let settings = settings::Settings::load(&settings_path).unwrap_or_default();
            client::engine::apply_settings(&settings);
            apply_tag_settings(&settings);
            let db_path = config_dir.join("ehviewer.db");
            let db = Db::open(&db_path).map_err(|e| {
                log::error!("failed to open database: {}", e);
                e.to_string()
            })?;
            let cache_dir = app
                .path()
                .app_cache_dir()
                .unwrap_or_else(|_| config_dir.clone());
            ehimg::set_cache_dir(cache_dir);
            ehimg::set_cache_max_mb(settings.image_cache_size_mb);
            client::client::set_max_retries(settings.max_retries);
            client::thumb::set_resolution(settings.thumb_resolution);

            let db = Arc::new(db);
            let settings = Arc::new(std::sync::Mutex::new(settings));
            let manager = Arc::new(DownloadManager::new(
                db.clone(),
                settings.clone(),
                app.handle().clone(),
            ));
            let manager_clone = manager.clone();
            manager_clone.start();

            app.manage(AppState {
                db,
                settings_path,
                settings,
            });
            app.manage(manager);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_meta,
            get_gallery_list,
            get_gallery_detail,
            get_preview_set,
            get_online_page,
            get_gallery_metadata,
            resolve_showpage,
            fetch_image,
            build_list_url,
            save_reading_progress,
            get_reading_progress,
            download_start,
            download_stop,
            download_delete,
            download_relabel,
            download_list,
            settings_get,
            settings_set,
            cookie_save,
            cookie_verify,
            export_data,
            import_data,
            import_tag_translation,
            network_diag
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Saves the reading position of a gallery so the reader can resume later.
#[tauri::command]
async fn save_reading_progress(app: tauri::AppHandle, gid: u64, page: u32) -> Result<(), String> {
    let state = app.state::<AppState>();
    state
        .db
        .save_reading_progress(gid, page)
        .map_err(|e| e.to_string())
}

/// Returns the last-read page for a gallery (0 if never read), for resume.
#[tauri::command]
fn get_reading_progress(app: tauri::AppHandle, gid: u64) -> Result<u32, String> {
    let state = app.state::<AppState>();
    state
        .db
        .get_reading_progress(gid)
        .map_err(|e| e.to_string())
}






#[cfg(test)]
mod tests {
    use super::*;
    use db::Db;

    #[test]
    fn import_roundtrips_export_shape() {
        // Shape mirrors what `export_data` emits (DownloadDto serialize_all = camelCase,
        // state as string).
        let export = serde_json::json!([
            {
                "gid": 42,
                "token": "tok42",
                "title": "示例画廊",
                "label": "收藏",
                "state": "downloading",
                "total": 12,
                "complete": 5,
                "dir": "C:\\Downloads\\EhViewer\\42-示例画廊",
                "url": "https://e-hentai.org/g/42/tok42/"
            }
        ]);
        let rec = record_from_json(&export[0]).unwrap();
        assert_eq!(rec.gid, 42);
        assert_eq!(rec.state, 2); // downloading
        assert_eq!(rec.total, 12);
        assert_eq!(rec.complete, 5);
        assert_eq!(rec.token, "tok42");

        let path = std::env::temp_dir().join(format!("ehv-import-test-{}", std::process::id()));
        let db = Db::open(&path).unwrap();
        db.upsert_download(&rec).unwrap();
        let list = db.list_downloads().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].gid, 42);
        assert_eq!(list[0].complete, 5);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn state_from_json_accepts_number_and_string() {
        assert_eq!(state_from_json(Some(&serde_json::json!(3))), 3);
        assert_eq!(state_from_json(Some(&serde_json::json!("failed"))), 4);
        assert_eq!(state_from_json(None), 0);
    }
}


