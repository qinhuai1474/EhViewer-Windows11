//! SQLite persistence (rusqlite, bundled): download items, tags, reading progress.
//! Mirrors the SXJ DAO layer.

use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

/// Download state machine, kept identical to SXJ's DownloadState.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadState {
    None,
    Wait,
    Download,
    Finish,
    Failed,
}

impl DownloadState {
    pub fn as_str(&self) -> &'static str {
        match self {
            DownloadState::None => "none",
            DownloadState::Wait => "wait",
            DownloadState::Download => "downloading",
            DownloadState::Finish => "finished",
            DownloadState::Failed => "failed",
        }
    }
}

/// A single SQLite-backed download entry.
#[derive(Debug, Clone)]
pub struct DownloadRecord {
    pub gid: u64,
    pub token: String,
    pub title: String,
    pub label: String,
    pub state: i32,
    pub total: u32,
    pub complete: u32,
    pub dir: String,
    pub url: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadDto {
    pub gid: u64,
    pub token: String,
    pub title: String,
    pub label: String,
    pub state: String,
    pub total: u32,
    pub complete: u32,
    pub dir: String,
    pub url: String,
}

impl From<&DownloadRecord> for DownloadDto {
    fn from(rec: &DownloadRecord) -> Self {
        Self {
            gid: rec.gid,
            token: rec.token.clone(),
            title: rec.title.clone(),
            label: rec.label.clone(),
            state: DownloadState::from_i32(rec.state).as_str().to_string(),
            total: rec.total,
            complete: rec.complete,
            dir: rec.dir.clone(),
            url: rec.url.clone(),
        }
    }
}

impl DownloadState {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => DownloadState::Wait,
            2 => DownloadState::Download,
            3 => DownloadState::Finish,
            4 => DownloadState::Failed,
            _ => DownloadState::None,
        }
    }

    pub fn as_i32(&self) -> i32 {
        match self {
            DownloadState::None => 0,
            DownloadState::Wait => 1,
            DownloadState::Download => 2,
            DownloadState::Finish => 3,
            DownloadState::Failed => 4,
        }
    }
}

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    /// Opens (or creates) the database at `path` and applies the schema.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS downloads (
                gid      INTEGER PRIMARY KEY,
                token    TEXT NOT NULL,
                title    TEXT NOT NULL DEFAULT '',
                label    TEXT NOT NULL DEFAULT '',
                state    INTEGER NOT NULL DEFAULT 0,
                total    INTEGER NOT NULL DEFAULT 0,
                complete INTEGER NOT NULL DEFAULT 0,
                dir      TEXT NOT NULL DEFAULT '',
                url      TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS reading_progress (
                gid    INTEGER PRIMARY KEY,
                page   INTEGER NOT NULL DEFAULT 1,
                updated INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS tags (
                id   INTEGER PRIMARY KEY AUTOINCREMENT,
                key  TEXT NOT NULL,
                name TEXT NOT NULL,
                UNIQUE(key, name)
            );
            "#,
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn upsert_download(&self, rec: &DownloadRecord) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO downloads(gid,token,title,label,state,total,complete,dir,url)
             VALUES(?1,?2,?3,?4,?5,?6,?7,'','')
             ON CONFLICT(gid) DO UPDATE SET
               token=excluded.token, title=excluded.title, label=excluded.label,
               state=excluded.state, total=excluded.total, complete=excluded.complete",
            rusqlite::params![
                rec.gid,
                rec.token,
                rec.title,
                rec.label,
                rec.state,
                rec.total,
                rec.complete
            ],
        )?;
        Ok(())
    }

    pub fn list_downloads(&self) -> rusqlite::Result<Vec<DownloadRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT gid,token,title,label,state,total,complete,dir,url FROM downloads ORDER BY gid",
        )?;
        let rows = stmt
            .query_map([], map_record)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_download(&self, gid: u64) -> rusqlite::Result<Option<DownloadRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT gid,token,title,label,state,total,complete,dir,url FROM downloads WHERE gid=?1")
            .unwrap();
        let mut rows = stmt.query_map([gid], map_record)?;
        rows.next().transpose()
    }

    pub fn relabel_download(&self, gid: u64, label: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET label=?2 WHERE gid=?1",
            rusqlite::params![gid, label],
        )?;
        Ok(())
    }

    pub fn set_download_total(&self, gid: u64, total: u32) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET total=?2 WHERE gid=?1",
            rusqlite::params![gid, total],
        )?;
        Ok(())
    }

    pub fn set_download_progress(&self, gid: u64, complete: u32) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET complete=?2 WHERE gid=?1",
            rusqlite::params![gid, complete],
        )?;
        Ok(())
    }

    pub fn set_download_dir(&self, gid: u64, dir: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET dir=?2 WHERE gid=?1",
            rusqlite::params![gid, dir],
        )?;
        Ok(())
    }

    pub fn delete_download(&self, gid: u64) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM downloads WHERE gid=?1", rusqlite::params![gid])?;
        Ok(())
    }

    pub fn save_reading_progress(&self, gid: u64, page: u32) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO reading_progress(gid,page,updated) VALUES(?1,?2,unixepoch())
             ON CONFLICT(gid) DO UPDATE SET page=excluded.page, updated=excluded.updated",
            rusqlite::params![gid, page],
        )?;
        Ok(())
    }

    pub fn clear_downloads(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM downloads", [])?;
        Ok(())
    }

    pub fn all_reading_progress(&self) -> rusqlite::Result<Vec<(u64, u32)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT gid, page FROM reading_progress")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Returns the last-read page for a gallery (0 if never read).
    pub fn get_reading_progress(&self, gid: u64) -> rusqlite::Result<u32> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT page FROM reading_progress WHERE gid=?1",
            rusqlite::params![gid],
            |r| r.get(0),
        )
        .or_else(|_| Ok(0))
    }

    pub fn set_download_state(&self, gid: u64, state: i32) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET state=?2 WHERE gid=?1",
            rusqlite::params![gid, state],
        )?;
        Ok(())
    }
}

fn map_record(r: &rusqlite::Row<'_>) -> rusqlite::Result<DownloadRecord> {
    Ok(DownloadRecord {
        gid: r.get(0)?,
        token: r.get(1)?,
        title: r.get(2)?,
        label: r.get(3)?,
        state: r.get(4)?,
        total: r.get(5)?,
        complete: r.get(6)?,
        dir: r.get(7)?,
        url: r.get(8)?,
    })
}


