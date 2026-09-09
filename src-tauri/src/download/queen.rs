//! Per-gallery download worker (port of SXJ `SpiderQueen`).
//!
//! One gallery = one `SpiderQueen` task. Pages are resolved and written with a
//! bounded worker pool (`download_threads`), already-downloaded files are skipped
//! (resume), transient failures/509s retry with backoff, and an idle timeout / stop
//! flag aborts the task. The disk layout lives in [`super::spider`].

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;


use crate::client::client;
use crate::client::data::PreviewItem;
use crate::client::engine;
use crate::client::err::{EhError, EhResult};
use crate::client::url;

use super::spider::{first_missing_page, page_exists, scan_completed_pages, write_image};

/// Result of a completed `SpiderQueen.run()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueenResult {
    /// All pages finished.
    Done { complete: u32 },
    /// Aborted by user / stop.
    Cancelled { complete: u32 },
    /// Some pages failed after retries; resumable.
    Failed { complete: u32, failed: u32 },
}

/// Abstraction over how a single page image is obtained, so the worker state
/// machine can be tested without a live network.
pub trait ImageFetcher: Send + Sync {
    fn fetch(&self, index: u32) -> Pin<Box<dyn Future<Output = EhResult<Vec<u8>>> + Send + '_>>;
}

/// Resolves each page's `p_token` (from preview sets) and downloads the full-size
/// image with the correct Referer/Cookie, exactly like the reader flow does.
pub struct NetworkImageFetcher {
    site: u8,
    gid: u64,
    token: String,
    always_original: bool,
    ptok: RwLock<std::collections::HashMap<u32, String>>,
    per_page: std::sync::atomic::AtomicU32,
}

impl NetworkImageFetcher {
    pub fn new(site: u8, gid: u64, token: String, always_original: bool) -> Self {
        Self {
            site,
            gid,
            token,
            always_original,
            ptok: RwLock::new(std::collections::HashMap::new()),
            per_page: std::sync::atomic::AtomicU32::new(0),
        }
    }

    async fn resolve_ptoken(&self, index: u32) -> EhResult<String> {
        {
            let cache = self.ptok.read().unwrap();
            if let Some(tok) = cache.get(&index) {
                if !tok.is_empty() {
                    return Ok(tok.clone());
                }
            }
        }
        let per_page = self.known_per_page();
        let preview_page = index / per_page;
        let items = engine::get_preview_set(self.site, self.gid, &self.token, preview_page).await?;
        self.record_items(items);
        let cache = self.ptok.read().unwrap();
        cache
            .get(&index)
            .filter(|t| !t.is_empty())
            .cloned()
            .ok_or_else(|| EhError::Parse("无法解析该页 p_token".into()))
    }

    fn known_per_page(&self) -> u32 {
        let pp = self.per_page.load(Ordering::Relaxed);
        if pp > 0 {
            pp
        } else {
            20
        }
    }

    fn record_items(&self, items: Vec<PreviewItem>) {
        let mut cache = self.ptok.write().unwrap();
        if self.per_page.load(Ordering::Relaxed) == 0 && !items.is_empty() {
            self.per_page.store(items.len() as u32, Ordering::Relaxed);
        }
        for it in items {
            if let Some(tok) = url::page_token_from(&it.page_url) {
                let entry = cache.entry(it.index).or_default();
                *entry = tok;
            }
        }
    }
}

impl ImageFetcher for NetworkImageFetcher {
    fn fetch(&self, index: u32) -> Pin<Box<dyn Future<Output = EhResult<Vec<u8>>> + Send + '_>> {
        // `self` is borrowed (lifetime `'_`), so the future may reuse the shared
        // p_token cache without owning any state.
        let referer = url::gallery_detail_url(self.site, self.gid, &self.token, 0, false);
        Box::pin(async move {
            let p_tok = self.resolve_ptoken(index).await?;
            let online = engine::get_online_page(self.site, self.gid, &self.token, index, &p_tok).await?;
            // Default to the direct reading-page image (a real webp/jpg). Only
            // download the `/fullimg` original when the user enabled it — the
            // origin link needs a session showkey and often returns an HTML
            // interstitial otherwise (which used to be saved as `.bin`).
            let url = if self.always_original {
                pick_fullsize_url(&online)
            } else if !online.image_url.trim().is_empty() {
                online.image_url.clone()
            } else {
                online.origin_image_url.clone().unwrap_or_default()
            };
            if url.is_empty() {
                return Err(EhError::Parse("empty image url".into()));
            }
            client::get_bytes(&url, Some(&referer)).await
        })
    }
}

fn pick_fullsize_url(online: &crate::client::data::OnlinePage) -> String {
    if let Some(o) = online.origin_image_url.as_deref() {
        if !o.is_empty() {
            return o.to_string();
        }
    }
    online.image_url.clone()
}

struct SpiderQueenInner {
    total: u32,
    threads: usize,
    idle_timeout: Duration,
    max_retries: u32,
    dir: PathBuf,
    fetcher: Arc<dyn ImageFetcher>,
    cancel: Arc<AtomicBool>,
    on_page: Option<Arc<dyn Fn(u32, u32) + Send + Sync>>,
}

/// A single gallery download task.
#[derive(Clone)]
pub struct SpiderQueen {
    inner: Arc<SpiderQueenInner>,
}

impl SpiderQueen {
    pub fn new(
        total: u32,
        threads: usize,
        idle_timeout: Duration,
        max_retries: u32,
        dir: PathBuf,
        fetcher: Arc<dyn ImageFetcher>,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        Self {
            inner: Arc::new(SpiderQueenInner {
                total,
                threads: threads.max(1),
                idle_timeout,
                max_retries,
                dir,
                fetcher,
                cancel,
                on_page: None,
            }),
        }
    }

    pub fn on_page<F: Fn(u32, u32) + Send + Sync + 'static>(mut self, f: F) -> Self {
        let inner = Arc::get_mut(&mut self.inner).unwrap();
        inner.on_page = Some(Arc::new(f));
        self
    }

    pub fn cancel(&self) -> Arc<AtomicBool> {
        self.inner.cancel.clone()
    }

    /// Runs the download task to completion, cancellation, or failure.
    pub async fn run(&self) -> QueenResult {
        let inner = self.inner.as_ref();
        std::fs::create_dir_all(&inner.dir).ok();

        let base = scan_completed_pages(&inner.dir, inner.total);
        if base >= inner.total {
            return QueenResult::Done { complete: base };
        }
        let start = first_missing_page(&inner.dir, inner.total);

        let next = Arc::new(AtomicU32::new(start));
        let complete = Arc::new(AtomicU32::new(base));
        let failed = Arc::new(AtomicU32::new(0));

        let mut set = tokio::task::JoinSet::new();
        for _ in 0..inner.threads {
            let q = self.clone();
            let next = next.clone();
            let complete = complete.clone();
            let failed = failed.clone();
            set.spawn(async move { q.inner_worker(next, complete, failed).await });
        }
        while set.join_next().await.is_some() {}

        let done = complete.load(Ordering::SeqCst);
        let f = failed.load(Ordering::SeqCst);

        if inner.cancel.load(Ordering::Relaxed) {
            QueenResult::Cancelled { complete: done }
        } else if done >= inner.total {
            QueenResult::Done { complete: done }
        } else {
            QueenResult::Failed { complete: done, failed: f }
        }
    }

    async fn inner_worker(
        &self,
        next: Arc<AtomicU32>,
        complete: Arc<AtomicU32>,
        failed: Arc<AtomicU32>,
    ) {
        let inner = self.inner.as_ref();
        loop {
            if inner.cancel.load(Ordering::Relaxed) {
                return;
            }
            let i = next.fetch_add(1, Ordering::SeqCst);
            if i >= inner.total {
                return;
            }
            if page_exists(&inner.dir, i) {
                continue;
            }
            match fetch_with_retry(
                inner.fetcher.as_ref(),
                i,
                inner.max_retries,
                inner.idle_timeout,
                inner.cancel.clone(),
            )
            .await
            {
                Ok(bytes) => {
                    if inner.cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    if write_image(&inner.dir, i, &bytes).is_ok() {
                        let c = complete.fetch_add(1, Ordering::SeqCst) + 1;
                        if let Some(cb) = inner.on_page.as_ref() {
                            cb(c, inner.total);
                        }
                    } else {
                        failed.fetch_add(1, Ordering::SeqCst);
                    }
                }
                Err(_) => {
                    if inner.cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    failed.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
    }
}

/// Fetches one page with exponential-backoff retry and an idle timeout.
async fn fetch_with_retry(
    fetcher: &dyn ImageFetcher,
    index: u32,
    max_retries: u32,
    idle: Duration,
    cancel: Arc<AtomicBool>,
) -> EhResult<Vec<u8>> {
    let mut attempt = 0u32;
    let mut last: EhError;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(EhError::Network("已取消".into()));
        }
        let fut = fetcher.fetch(index);
        match tokio::time::timeout(idle, fut).await {
            Ok(Ok(bytes)) if !bytes.is_empty() => return Ok(bytes),
            Ok(Ok(_)) => last = EhError::Internal("图片内容为空".into()),
            Ok(Err(e)) => last = e,
            Err(_) => last = EhError::Network("请求超时".into()),
        }
        attempt += 1;
        if attempt > max_retries {
            return Err(last);
        }
        let backoff = Duration::from_secs(1u64 << attempt.min(3));
        tokio::time::sleep(backoff).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockFetcher {
        #[allow(dead_code)]
        tag: &'static str,
        fail_first: std::collections::HashMap<u32, u32>,
        delay_ms: u64,
    }

    impl ImageFetcher for MockFetcher {
        fn fetch(&self, index: u32) -> Pin<Box<dyn Future<Output = EhResult<Vec<u8>>> + Send + '_>> {
            let tag = self.tag;
            let fails = self.fail_first.get(&index).copied().unwrap_or(0);
            let delay = self.delay_ms;
            Box::pin(async move {
                if tag == "pending" {
                    // Never returns; exercises idle timeout / cancel paths.
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    return Ok(Vec::new());
                }
                if fails > 0 {
                    return Err(EhError::Network("boom".into()));
                }
                tokio::time::sleep(Duration::from_millis(delay)).await;
                Ok(vec![0xFF, 0xD8, 0xFF, index as u8, 0, 0, 0, 0])
            })
        }
    }

    fn tmp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ehv-{}-{}", name, std::process::id()))
    }

    #[tokio::test]
    async fn downloads_all_pages_and_emits_progress() {
        let dir = tmp_dir("queen-all");
        std::fs::create_dir_all(&dir).unwrap();
        let fetcher = Arc::new(MockFetcher {
            tag: "ok",
            fail_first: Default::default(),
            delay_ms: 1,
        });
        let q = SpiderQueen::new(
            6,
            3,
            Duration::from_secs(5),
            2,
            dir.clone(),
            fetcher,
            Arc::new(AtomicBool::new(false)),
        );
        use std::sync::Mutex;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        let q2 = q.on_page(move |c, _t| seen2.lock().unwrap().push(c));
        let res = q2.run().await;
        assert_eq!(res, QueenResult::Done { complete: 6 });
        assert_eq!(scan_completed_pages(&dir, 6), 6);
        assert_eq!(seen.lock().unwrap().iter().copied().max(), Some(6));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn skips_existing_files_on_resume() {
        let dir = tmp_dir("queen-resume");
        std::fs::create_dir_all(&dir).unwrap();
        // Pre-seed pages 0 and 2.
        write_image(&dir, 0, &[0xFF, 0xD8, 0xFF, 0x00]).unwrap();
        let fetcher = Arc::new(MockFetcher {
            tag: "ok",
            fail_first: Default::default(),
            delay_ms: 1,
        });
        let q = SpiderQueen::new(
            4,
            2,
            Duration::from_secs(5),
            2,
            dir.clone(),
            fetcher,
            Arc::new(AtomicBool::new(false)),
        );
        let res = q.run().await;
        assert_eq!(res, QueenResult::Done { complete: 4 });
        assert_eq!(scan_completed_pages(&dir, 4), 4);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn retries_transient_failure_then_succeeds() {
        // Global fetch counter to make the first calls for index 0 fail, then succeed.
        static CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        CALLS.store(0, Ordering::SeqCst);
        struct RetryMock;
        impl ImageFetcher for RetryMock {
            fn fetch(&self, index: u32) -> Pin<Box<dyn Future<Output = EhResult<Vec<u8>>> + Send + '_>> {
                Box::pin(async move {
                    let n = CALLS.fetch_add(1, Ordering::SeqCst);
                    if index == 0 && n < 2 {
                        Err(EhError::Network("boom".into()))
                    } else {
                        Ok(vec![0xFF, 0xD8, 0xFF, index as u8, 0])
                    }
                })
            }
        }
        let dir = tmp_dir("queen-retry");
        std::fs::create_dir_all(&dir).unwrap();
        let q = SpiderQueen::new(
            3,
            1,
            Duration::from_secs(2),
            3,
            dir.clone(),
            Arc::new(RetryMock),
            Arc::new(AtomicBool::new(false)),
        );
        let res = q.run().await;
        assert_eq!(res, QueenResult::Done { complete: 3 });
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn cancellation_returns_cancelled() {
        let dir = tmp_dir("queen-cancel");
        std::fs::create_dir_all(&dir).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let q = SpiderQueen::new(
            5,
            2,
            Duration::from_secs(60),
            0,
            dir.clone(),
            Arc::new(MockFetcher {
                tag: "pending",
                fail_first: Default::default(),
                delay_ms: 0,
            }),
            cancel.clone(),
        );
        let q2 = q.clone();
        let handle = tokio::spawn(async move { q2.run().await });
        tokio::time::sleep(Duration::from_millis(150)).await;
        cancel.store(true, Ordering::Relaxed);
        let res = handle.await.unwrap();
        assert_eq!(res, QueenResult::Cancelled { complete: 0 });
        std::fs::remove_dir_all(&dir).ok();
    }
}




