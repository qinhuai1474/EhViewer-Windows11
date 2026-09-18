//! Download manager & Tauri integration (port of SXJ `DownloadManager`).
//!
//! One gallery = one serial `SpiderQueen` task pulled from a FIFO wait queue.
//! State lives in SQLite (mirrors SXJ Dao); the manager translates queue / cancel /
//! progress into `download-changed` and `download-progress` Tauri events.

pub mod queen;
pub mod spider;
pub mod naming;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex as TokioMutex, Notify};

use crate::db::{Db, DownloadDto, DownloadRecord, DownloadState};
use crate::settings::Settings;

use queen::{ImageFetcher, NetworkImageFetcher, QueenResult, SpiderQueen};
use spider::{gallery_dir_name, gallery_download_path, SpiderInfo, SPIDER_INFO_FILENAME};

/// Emitted with the full download list whenever a download changes.
pub const EV_DOWNLOAD_CHANGED: &str = "download-changed";
/// Emitted with `{ gid, complete, total }` on each page finished.
pub const EV_DOWNLOAD_PROGRESS: &str = "download-progress";

/// Shared, mostly-immutable manager handle; individual gallery work is isolated
/// in `SpiderQueen` tasks so a stop only affects its own gallery.
pub struct DownloadManager {
    db: Arc<Db>,
    settings: Arc<std::sync::Mutex<Settings>>,
    handle: AppHandle,
    state: TokioMutex<AsyncState>,
    notify: Arc<Notify>,
}

struct AsyncState {
    queue: VecDeque<u64>,
    running: Option<u64>,
    cancels: HashMap<u64, Arc<AtomicBool>>,
    /// gid -> target dir for labels changed while the gallery was still
    /// downloading; the move is applied once the active worker completes.
    pending_moves: HashMap<u64, PathBuf>,
    /// Gallery-level retry counter so a failed task is retried (up to
    /// `max_retries`) and the queue still drains to full completion.
    retries: HashMap<u64, u32>,
}

/// Result of processing one gallery inside the queue loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GalleryOutcome {
    /// All pages were downloaded.
    Done,
    /// Aborted by the user (pause / delete).
    Cancelled,
    /// Some pages failed after per-page retries.
    Failed,
}

impl DownloadManager {
    pub fn new(
        db: Arc<Db>,
        settings: Arc<std::sync::Mutex<Settings>>,
        handle: AppHandle,
    ) -> Self {
        Self {
            db,
            settings,
            handle,
            state: TokioMutex::new(AsyncState {
                queue: VecDeque::new(),
                running: None,
                cancels: HashMap::new(),
                pending_moves: HashMap::new(),
                retries: HashMap::new(),
            }),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Spawns the queue worker and (re)queues any incomplete downloads from disk.
    pub fn start(self: Arc<Self>) {
        let me = self.clone();
        tauri::async_runtime::spawn(async move { me.run_loop().await });
        let me = self.clone();
        tauri::async_runtime::spawn(async move { me.restore().await });
    }

    /// Adds (or resumes) a gallery download.
    pub async fn start_download(
        &self,
        gid: u64,
        token: String,
        title: String,
        label: String,
        artist: String,
        language: String,
        group: String,
        total: u32,
        url: String,
    ) -> Result<(), String> {
        let rec = DownloadRecord {
            gid,
            token,
            title,
            label,
            artist,
            language,
            group,
            state: DownloadState::Wait.as_i32(),
            total: total.max(1),
            complete: 0,
            dir: String::new(),
            url,
        };
        self.db.upsert_download(&rec).map_err(|e| e.to_string())?;
        {
            let mut st = self.state.lock().await;
            if st.running == Some(gid) || st.queue.contains(&gid) {
                return Ok(());
            }
            st.queue.push_back(gid);
        }
        self.emit_changed();
        self.notify.notify_one();
        Ok(())
    }

    /// Pauses (cancels) or un-queues a download; the record keeps its progress.
    pub async fn stop_download(&self, gid: u64) -> Result<(), String> {
        {
            let mut st = self.state.lock().await;
            if st.running == Some(gid) {
                if let Some(c) = st.cancels.get(&gid) {
                    c.store(true, Ordering::Relaxed);
                }
            } else {
                st.queue.retain(|g| *g != gid);
            }
        }
        self.emit_changed();
        Ok(())
    }

    /// Removes a download from the DB and (optionally) its files on disk.
    pub async fn delete_download(&self, gid: u64, erase: bool) -> Result<(), String> {
        {
            let mut st = self.state.lock().await;
            if st.running == Some(gid) {
                if let Some(c) = st.cancels.get(&gid) {
                    c.store(true, Ordering::Relaxed);
                }
            }
            st.queue.retain(|g| *g != gid);
        }
        if erase {
            if let Some(rec) = self.db.get_download(gid).map_err(|e| e.to_string())? {
                let dir = if !rec.dir.is_empty() {
                    PathBuf::from(&rec.dir)
                } else {
                    gallery_download_path(&self.download_root(), &rec.label, gid, &rec.title)
                };
                std::fs::remove_dir_all(&dir).ok();
            }
        }
        self.db.delete_download(gid).map_err(|e| e.to_string())?;
        self.emit_changed();
        Ok(())
    }

    /// Renames a download's label and moves its on-disk folder to the new
    /// label sub-folder. When the gallery is still downloading the move is
    /// deferred until the active worker finishes to avoid writing to a path
    /// that is being relocated beneath it.
    pub async fn relabel_download(&self, gid: u64, label: &str) -> Result<(), String> {
        let old = self.db.get_download(gid).map_err(|e| e.to_string())?;
        self.db
            .relabel_download(gid, label)
            .map_err(|e| e.to_string())?;
        if let Some(rec) = old {
            let root = self.download_root();
            let new_dir = gallery_download_path(&root, label, gid, &rec.title);
            let old_dir = if !rec.dir.is_empty() {
                PathBuf::from(&rec.dir)
            } else {
                gallery_download_path(&root, &rec.label, gid, &rec.title)
            };
            if old_dir != new_dir && old_dir.is_dir() {
                if DownloadState::from_i32(rec.state) == DownloadState::Download {
                    self.state.lock().await.pending_moves.insert(gid, new_dir);
                } else {
                    self.move_gallery_folder(gid, &old_dir, &new_dir);
                }
            }
        }
        self.emit_changed();
        Ok(())
    }

    /// Returns the full download list.
    pub fn list_downloads(&self) -> Result<Vec<DownloadDto>, String> {
        let recs = self.db.list_downloads().map_err(|e| e.to_string())?;
        Ok(recs.iter().map(DownloadDto::from).collect())
    }

    /// Recursively scans the download root for `<gid>-<title>` gallery folders and
    /// renames them to `[Author] Title`, removing each renamed gallery from the
    /// download queue (files stay on disk). `preview = true` computes the plan in
    /// memory without touching the filesystem or the database, so the UI can show
    /// before/after names and the user can cancel with zero side effects.
    /// Scans the configured scan root (the fixed `rename_scan_dir` setting, or
    /// the download root when empty) for `<gid>-<title>` gallery folders and
    /// renames them to `[Author] Title`, removing each renamed gallery from the
    /// download queue (files stay on disk). Only folders that actually contain
    /// the `.ehviewer` marker are touched, so nothing outside the scan root is
    /// ever renamed.
    pub async fn rename_dirs(&self, preview: bool) -> Result<naming::RenameReport, String> {
        let mut report = naming::RenameReport::default();
        let root = self.rename_scan_root();
        if !root.is_dir() {
            return Ok(report);
        }
        let recs = self.db.list_downloads().map_err(|e| e.to_string())?;
        let by_gid: HashMap<u64, &DownloadRecord> = recs.iter().map(|r| (r.gid, r)).collect();
        let terms = self.rename_filter_terms();
        let mut used = HashSet::new();
        self.scan_for_rename(&root, &by_gid, &terms, &mut used, &mut report, preview);
        if !preview && report.renamed > 0 {
            self.emit_changed();
        }
        Ok(report)
    }

    // ----- internal helpers -----

    /// The directory the rename tool operates on: the fixed `rename_scan_dir`
    /// when set, otherwise the download root.
    fn rename_scan_root(&self) -> PathBuf {
        let scan = self.settings.lock().unwrap().rename_scan_dir.clone();
        if !scan.trim().is_empty() {
            PathBuf::from(scan)
        } else {
            self.download_root()
        }
    }

    /// The effective tag blocklist: user-configured terms when present, otherwise
    /// the built-in defaults.
    fn rename_filter_terms(&self) -> Vec<String> {
        let terms = self.settings.lock().unwrap().rename_filter_terms.clone();
        if terms.is_empty() {
            naming::default_filter_terms()
        } else {
            terms
        }
    }

    fn scan_for_rename(
        &self,
        dir: &Path,
        by_gid: &HashMap<u64, &DownloadRecord>,
        terms: &[String],
        used: &mut HashSet<String>,
        report: &mut naming::RenameReport,
        preview: bool,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        paths.sort();
        for p in paths {
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if let Some((gid, _)) = naming::parse_old_folder_name(&name) {
                // A folder is only a renameable gallery if it carries the
                // `.ehviewer` index; anything else with a gid-like name is left
                // strictly untouched (and not recursed into) so renames never
                // spill into unrelated directories.
                if p.join(SPIDER_INFO_FILENAME).is_file() {
                    self.rename_one(&p, gid, by_gid, terms, used, report, preview);
                }
            } else {
                self.scan_for_rename(&p, by_gid, terms, used, report, preview);
            }
        }
    }

    fn rename_one(
        &self,
        p: &Path,
        gid: u64,
        by_gid: &HashMap<u64, &DownloadRecord>,
        terms: &[String],
        used: &mut HashSet<String>,
        report: &mut naming::RenameReport,
        preview: bool,
    ) {
        let old_name = p
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(&rec) = by_gid.get(&gid) else {
            self.push_report(report, gid, old_name, String::new(), "skipped", Some("不在下载队列"));
            return;
        };
        if matches!(
            DownloadState::from_i32(rec.state),
            DownloadState::Wait | DownloadState::Download
        ) {
            self.push_report(report, gid, old_name, String::new(), "skipped", Some("下载中，跳过"));
            return;
        }
        let artist = opt_str(rec.artist.as_str());
        let language = opt_str(rec.language.as_str());
        match naming::plan_rename(&rec.title, artist, language, terms) {
            Err(skip) => {
                self.push_report(
                    report,
                    gid,
                    old_name,
                    String::new(),
                    "skipped",
                    Some(&skip.to_string()),
                );
            }
            Ok(new_name) => {
                let parent = p.parent().unwrap_or(p);
                let mut candidate = new_name.clone();
                let mut n = 0u64;
                while used.contains(&candidate) || parent.join(&candidate).exists() {
                    n += 1;
                    candidate = format!("{} {}", new_name, n);
                }
                used.insert(candidate.clone());
                if preview {
                    self.push_report(report, gid, old_name, candidate, "renamed", None);
                } else {
                    let new_path = parent.join(&candidate);
                    match std::fs::rename(p, &new_path) {
                        Ok(()) => {
                            let _ = self.db.set_download_dir(gid, &new_path.to_string_lossy());
                            let _ = self.db.delete_download(gid);
                            self.push_report(report, gid, old_name, candidate, "renamed", None);
                        }
                        Err(e) => self.push_report(
                            report,
                            gid,
                            old_name,
                            candidate,
                            "failed",
                            Some(&format!("重命名失败：{e}")),
                        ),
                    }
                }
            }
        }
    }

    fn push_report(
        &self,
        report: &mut naming::RenameReport,
        gid: u64,
        old_name: String,
        new_name: String,
        status: &str,
        reason: Option<&str>,
    ) {
        report.entries.push(naming::RenameEntry {
            gid,
            old_name,
            new_name,
            status: status.to_string(),
            reason: reason.map(str::to_string),
        });
        match status {
            "renamed" => report.renamed += 1,
            "skipped" => report.skipped += 1,
            _ => report.failed += 1,
        }
    }

    /// Single background worker: pulls the next waiting gallery in FIFO order,
    /// runs it to completion, then rests the configured interval before the next
    /// task. A gallery that fails is re-queued (auto-retry) up to `max_retries`
    /// so the whole queue drains to completion; once a task exceeds its limit it
    /// is dropped and the worker advances. The loop only idles when empty.
    async fn run_loop(&self) {
        loop {
            self.notify.notified().await;
            let gid = {
                let mut st = self.state.lock().await;
                if st.running.is_some() {
                    continue;
                }
                match st.queue.pop_front() {
                    Some(g) => g,
                    None => continue,
                }
            };
            let cancel = Arc::new(AtomicBool::new(false));
            {
                let mut st = self.state.lock().await;
                st.cancels.insert(gid, cancel.clone());
                st.running = Some(gid);
            }
            let outcome = self.run_gallery(gid, cancel).await;
            {
                let mut st = self.state.lock().await;
                st.running = None;
                st.cancels.remove(&gid);
                if outcome != GalleryOutcome::Failed {
                    // Success/cancel resets the gallery-level retry counter so a
                    // later manual resume starts from a clean slate.
                    st.retries.remove(&gid);
                }
            }
            self.apply_pending_move(gid).await;

            let interval = self.download_interval();
            if outcome == GalleryOutcome::Failed && self.maybe_requeue(gid).await {
                // Re-queued for retry: space it out, then signal the loop.
                if !interval.is_zero() {
                    tokio::time::sleep(interval).await;
                }
                self.notify.notify_one();
                continue;
            }
            // Between consecutive downloads (and before the next task) rest the
            // configured interval, unless the task was cancelled by the user.
            if outcome != GalleryOutcome::Cancelled && !interval.is_zero() {
                tokio::time::sleep(interval).await;
            }
            self.notify.notify_one();
        }
    }

    async fn run_gallery(&self, gid: u64, cancel: Arc<AtomicBool>) -> GalleryOutcome {
        let Some(rec) = self.db.get_download(gid).ok().flatten() else {
            return GalleryOutcome::Failed;
        };
        let root = self.download_root();
        let dir = gallery_download_path(&root, &rec.label, gid, &rec.title);
        if std::fs::create_dir_all(&dir).is_err() {
            let _ = self.db.set_download_state(gid, DownloadState::Failed.as_i32());
            self.emit_changed();
            return GalleryOutcome::Failed;
        }
        let _ = self.db.set_download_dir(gid, &dir.to_string_lossy());
        let _ = self.db.set_download_total(gid, rec.total.max(1));
        let _ = self.db.set_download_state(gid, DownloadState::Download.as_i32());
        self.emit_changed();

        let fetch: Arc<dyn ImageFetcher> =
            Arc::new(NetworkImageFetcher::new(
            self.site(),
            gid,
            rec.token.clone(),
            self.download_always_original(),
        ));

        let db = self.db.clone();
        let handle = self.handle.clone();
        let queen = SpiderQueen::new(
            rec.total,
            self.download_threads(),
            self.idle_timeout(),
            self.max_retries(),
            dir.clone(),
            fetch,
            cancel,
        )
        .on_page(move |complete, total| {
            let _ = db.set_download_progress(gid, complete);
            // Per-page progress is delivered via the lightweight event only;
            // the full-list `download-changed` event fires on real state changes
            // (start/stop/delete/finish) via `emit_changed`.
            let _ = handle.emit(
                EV_DOWNLOAD_PROGRESS,
                serde_json::json!({ "gid": gid, "complete": complete, "total": total }),
            );
        });

        let result = queen.run().await;
        let total = rec.total;

        let outcome = match result {
            QueenResult::Done { complete } => {
                let info = SpiderInfo {
                    gid,
                    token: rec.token,
                    pages: total as i32,
                    ..Default::default()
                };
                let _ = info.write(&dir.join(SPIDER_INFO_FILENAME));
                let _ = self.db.set_download_progress(gid, complete);
                let _ = self.db.set_download_state(gid, DownloadState::Finish.as_i32());
                GalleryOutcome::Done
            }
            QueenResult::Cancelled { complete } => {
                let _ = self.db.set_download_progress(gid, complete);
                // Paused: keep progress, resumable via start_download.
                let _ = self.db.set_download_state(gid, DownloadState::Wait.as_i32());
                GalleryOutcome::Cancelled
            }
            QueenResult::Failed { complete, .. } => {
                let _ = self.db.set_download_progress(gid, complete);
                let _ = self.db.set_download_state(gid, DownloadState::Failed.as_i32());
                GalleryOutcome::Failed
            }
        };
        let _ = self.handle.emit(
            EV_DOWNLOAD_PROGRESS,
            serde_json::json!({ "gid": gid, "complete": total, "total": total }),
        );
        self.emit_changed();
        outcome
    }

    /// Re-queues incomplete galleries saved in WAIT/DOWNLOAD/FAILED on startup,
    /// and re-syncs their complete count from disk.
    async fn restore(&self) {
        self.migrate_flat_dirs().await;
        let recs = self.db.list_downloads().unwrap_or_default();
        let mut any = false;
        for rec in &recs {
            let st = DownloadState::from_i32(rec.state);
            if rec.total == 0 || rec.complete >= rec.total {
                continue;
            }
            if !matches!(
                st,
                DownloadState::Wait | DownloadState::Download | DownloadState::Failed
            ) {
                continue;
            }
            let complete = if rec.dir.is_empty() {
                rec.complete
            } else {
                spider::scan_completed_pages(&PathBuf::from(&rec.dir), rec.total)
            };
            let _ = self.db.set_download_progress(rec.gid, complete);
            if complete < rec.total {
                let _ = self.db.set_download_state(rec.gid, DownloadState::Wait.as_i32());
                let mut q = self.state.lock().await;
                if !q.queue.contains(&rec.gid) && q.running != Some(rec.gid) {
                    q.queue.push_back(rec.gid);
                    any = true;
                }
            }
        }
        if any {
            self.notify.notify_one();
        }
        self.emit_changed();
    }

    /// Migrates galleries still sitting flat under the download root (the old
    /// `<root>/<gid>-<title>` layout) into their label sub-folder. Idempotent:
    /// after the first run the record's `dir` points inside the label folder,
    /// so nothing is moved again.
    async fn migrate_flat_dirs(&self) {
        let root = self.download_root();
        let recs = self.db.list_downloads().unwrap_or_default();
        for rec in &recs {
            let target = gallery_download_path(&root, &rec.label, rec.gid, &rec.title);
            if target.is_dir() {
                continue;
            }
            let old = if rec.dir.is_empty() {
                root.join(gallery_dir_name(rec.gid, &rec.title))
            } else {
                PathBuf::from(&rec.dir)
            };
            if is_flat_under(&root, &old) && old.is_dir() {
                self.move_gallery_folder(rec.gid, &old, &target);
            }
        }
    }

    /// Applies a deferred label move recorded while a gallery was downloading
    /// (safe now that that gallery is no longer being written to).
    async fn apply_pending_move(&self, gid: u64) {
        let new_dir = self.state.lock().await.pending_moves.remove(&gid);
        if let Some(new_dir) = new_dir {
            if let Some(rec) = self.db.get_download(gid).ok().flatten() {
                if !rec.dir.is_empty() {
                    let old = PathBuf::from(&rec.dir);
                    if old != new_dir && self.move_gallery_folder(gid, &old, &new_dir) {
                        self.emit_changed();
                    }
                }
            }
        }
    }

    /// Renames a gallery folder (creating the destination parent) and keeps the
    /// stored `dir` in sync. Returns whether a move actually happened.
    fn move_gallery_folder(&self, gid: u64, old: &Path, new: &Path) -> bool {
        if relocate_gallery_dir(&self.db, gid, old, new) {
            true
        } else {
            false
        }
    }

    fn download_root(&self) -> PathBuf {
        let guard = self.settings.lock().unwrap();
        if let Some(d) = &guard.download_dir {
            if !d.is_empty() {
                return PathBuf::from(d);
            }
        }
        drop(guard);
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            PathBuf::from(profile).join("Pictures").join("EhViewer")
        } else {
            std::env::temp_dir().join("ehviewer")
        }
    }

    fn site(&self) -> u8 {
        self.settings.lock().unwrap().site
    }

    fn download_always_original(&self) -> bool {
        self.settings.lock().unwrap().download_always_original
    }

    fn download_threads(&self) -> usize {
        self.settings
            .lock()
            .unwrap()
            .download_threads
            .clamp(1, 10) as usize
    }

    fn idle_timeout(&self) -> Duration {
        Duration::from_secs(self.settings.lock().unwrap().timeout_secs.max(1))
    }

    fn max_retries(&self) -> u32 {
        self.settings.lock().unwrap().max_retries
    }

    fn download_interval(&self) -> Duration {
        Duration::from_secs(self.settings.lock().unwrap().download_interval_secs)
    }

    /// Increments the gallery-level retry counter for `gid`. While the count is
    /// within the configured `max_retries` the gallery is re-queued at the FIFO
    /// tail and `true` is returned; past the limit its counter is cleared and
    /// `false` is returned (the download stays in the DB as `Failed`).
    async fn maybe_requeue(&self, gid: u64) -> bool {
        let mut st = self.state.lock().await;
        let max = self.max_retries();
        let n = st.retries.entry(gid).or_insert(0);
        *n += 1;
        if may_retry(*n, max) {
            if !st.queue.contains(&gid) {
                st.queue.push_back(gid);
            }
            true
        } else {
            st.retries.remove(&gid);
            false
        }
    }

    fn emit_changed(&self) {
        let list = self.list_downloads().unwrap_or_default();
        let _ = self.handle.emit(EV_DOWNLOAD_CHANGED, list);
    }
}

/// Whether attempt `n` (1-based) is still permitted under `max` allowed retries.
fn may_retry(attempt: u32, max: u32) -> bool {
    attempt <= max
}

/// True when `path` sits directly under `root` (the old flat layout) rather
/// than under a label sub-folder.
fn is_flat_under(root: &Path, path: &Path) -> bool {
    path.parent().map_or(false, |p| p == root)
}

/// `Some(s)` when the stored field is non-empty, else `None` (used to pass
/// optional author / language metadata into the rename pipeline).
fn opt_str(s: &str) -> Option<&str> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Moves the gallery folder `old` -> `new` (creating `new`'s parent) and, on
/// success, updates the stored download `dir`. Must be called while the gallery
/// is not being written to, to avoid racing the active spider.
fn relocate_gallery_dir(db: &Db, gid: u64, old: &Path, new: &Path) -> bool {
    if old == new || !old.is_dir() {
        return false;
    }
    if let Some(parent) = new.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let moved = std::fs::rename(old, new).is_ok();
    if moved {
        let _ = db.set_download_dir(gid, &new.to_string_lossy());
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gallery_retry_gates_on_max_retries() {
        // 0 = no retries: the very first failure is the end of the line.
        assert!(!may_retry(1, 0));
        // Up to `max` retries are allowed after the initial failure.
        assert!(may_retry(1, 3));
        assert!(may_retry(3, 3));
        assert!(!may_retry(4, 3));
    }

    #[test]
    fn interval_helper_reads_configurable_setting() {
        let s = Settings::default();
        assert_eq!(s.download_interval_secs, 5, "default interval is 5s");
        let d = Duration::from_secs(s.download_interval_secs);
        assert_eq!(d, Duration::from_secs(5));
    }

    #[test]
    fn flat_detection() {
        let root = Path::new("R");
        assert!(is_flat_under(root, &root.join("1-Gal")));
        assert!(!is_flat_under(root, &root.join("Fav").join("1-Gal")));
        assert!(!is_flat_under(root, Path::new("other")));
    }

    #[test]
    fn relocate_moves_folder_and_updates_dir() {
        let root = std::env::temp_dir().join(format!("ehv-mv-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let db = Db::open(&root.join("test.sqlite")).unwrap();
        db.upsert_download(&DownloadRecord {
            gid: 1,
            token: String::new(),
            title: "Gal".into(),
            label: String::new(),
            artist: String::new(),
            language: String::new(),
            group: String::new(),
            state: DownloadState::Finish.as_i32(),
            total: 10,
            complete: 10,
            dir: String::new(),
            url: String::new(),
        })
        .unwrap();
        let flat = root.join("1-Gal");
        let wrapped = root.join("Fav").join("1-Gal");
        std::fs::create_dir_all(&flat).unwrap();
        std::fs::write(flat.join("00000001.jpg"), b"x").unwrap();

        assert!(relocate_gallery_dir(&db, 1, &flat, &wrapped));
        assert!(!flat.exists());
        assert!(wrapped.join("00000001.jpg").is_file());
        let rec = db.get_download(1).unwrap().unwrap();
        assert_eq!(rec.dir, wrapped.to_string_lossy());

        // Relocating again to the same place is a no-op.
        assert!(!relocate_gallery_dir(&db, 1, &wrapped, &wrapped));
        std::fs::remove_dir_all(&root).ok();
    }
}
