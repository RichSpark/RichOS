use super::{Entry, Phase, Result};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex, OnceLock};
pub struct Catalog {
    db: Connection,
    path: PathBuf,
}

type Change = Vec<PathBuf>;
type Listeners = BTreeMap<PathBuf, Vec<mpsc::Sender<Change>>>;
static LISTENERS: OnceLock<Mutex<Listeners>> = OnceLock::new();

fn notify(path: &Path, changed: Change) {
    if changed.is_empty() {
        return;
    }
    if let Some(listeners) = LISTENERS.get() {
        let mut listeners = listeners.lock().unwrap();
        if let Some(watchers) = listeners.get_mut(path) {
            watchers.retain(|watcher| watcher.send(changed.clone()).is_ok());
            if watchers.is_empty() {
                listeners.remove(path);
            }
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub entries: Vec<Entry>,
    pub total: u64,
    pub next: Option<u64>,
}
impl Catalog {
    #[cfg(test)]
    pub(super) fn db_for_test_version(&self, version: i64) {
        self.db
            .pragma_update(None, "user_version", version)
            .unwrap();
    }

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| e.to_string())?;
        db.busy_timeout(std::time::Duration::from_secs(3))
            .map_err(|e| e.to_string())?;
        // Changing a new database to WAL can return SQLITE_BUSY immediately,
        // without invoking SQLite's busy handler, when first openers overlap.
        // WAL persists: an established library only needs its mode read.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let mode = db.query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .and_then(|mode| if mode == "wal" { Ok(mode) } else {
                    db.query_row("PRAGMA journal_mode=WAL", [], |row| row.get::<_, String>(0))
                });
            match mode {
                Ok(mode) if mode == "wal" => break,
                Ok(_) => return Err("The library could not enable durable journal storage".into()),
                Err(rusqlite::Error::SqliteFailure(code, _))
                    if matches!(code.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
                        && std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => return Err(format!("The library journal could not be opened: {error}")),
            }
        }
        db.execute_batch("PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")
            .map_err(|e| e.to_string())?;
        let version: i64 = db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if version > 2 {
            return Err("This library needs a newer RichOS".into());
        }
        if version < 2 {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|e|e.to_string())?;
            // Another connection may have initialized while this one waited.
            // Decide only while holding the migration transaction's write lock.
            let current:i64 = tx.query_row("PRAGMA user_version",[],|r|r.get(0)).map_err(|e|e.to_string())?;
            match current {
                0 => tx.execute_batch("CREATE TABLE files(id TEXT PRIMARY KEY,volume TEXT NOT NULL,object TEXT NOT NULL,phase TEXT NOT NULL,updated INTEGER NOT NULL,record TEXT NOT NULL,UNIQUE(volume,object)); CREATE VIRTUAL TABLE search USING fts5(id UNINDEXED,name,note,labels,tokenize='unicode61 remove_diacritics 2'); CREATE TABLE cards(id TEXT PRIMARY KEY,entity TEXT NOT NULL,thread TEXT NOT NULL,turn TEXT NOT NULL,file TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,at INTEGER NOT NULL,UNIQUE(entity,thread,turn,file)); PRAGMA user_version=2;").map_err(|e|e.to_string())?,
                1 => tx.execute_batch("CREATE TABLE cards(id TEXT PRIMARY KEY,entity TEXT NOT NULL,thread TEXT NOT NULL,turn TEXT NOT NULL,file TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,at INTEGER NOT NULL,UNIQUE(entity,thread,turn,file)); PRAGMA user_version=2;").map_err(|e|e.to_string())?,
                2 => {},
                _ => return Err("This library needs a newer RichOS".into()),
            }
            tx.commit().map_err(|e|e.to_string())?;
        }
        Ok(Self { db, path: path.to_owned() })
    }

    /// File changes from the local app writer. Card activity is independent of
    /// file metadata and never sends a notification here.
    pub fn watch_filed_context(path: &Path) -> mpsc::Receiver<Vec<PathBuf>> {
        let (sender, receiver) = mpsc::channel();
        LISTENERS.get_or_init(Mutex::default).lock().unwrap()
            .entry(path.to_owned()).or_default().push(sender);
        receiver
    }
    pub fn get(&self, id: &str) -> Result<Entry> {
        let text: String = self
            .db
            .query_row("SELECT record FROM files WHERE id=?1", [id], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        serde_json::from_str(&text)
            .map_err(|e| format!("The saved file record could not be read: {e}"))
    }
    pub fn by_identity(&self, volume: u64, object: u64) -> Result<Option<Entry>> {
        let id: Option<String> = self
            .db
            .query_row(
                "SELECT id FROM files WHERE volume=?1 AND object=?2",
                params![volume.to_string(), object.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        id.map(|id| self.get(&id)).transpose()
    }

    /// Read the settled file record without creating or migrating the library.
    /// A caller still has to check the live file and its own search grant.
    pub fn filed_context(path: &Path, volume: u64, object: u64) -> Result<Option<Entry>> {
        if !path.try_exists().map_err(|e| e.to_string())? {
            return Ok(None);
        }
        let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
            .map_err(|e| e.to_string())?;
        db.busy_timeout(std::time::Duration::from_millis(250)).map_err(|e| e.to_string())?;
        let text: Option<String> = db.query_row(
            "SELECT record FROM files WHERE volume=?1 AND object=?2 AND phase='\"filed\"'",
            params![volume.to_string(), object.to_string()],
            |row| row.get(0),
        ).optional().map_err(|e| e.to_string())?;
        text.map(|text| serde_json::from_str(&text).map_err(|e| e.to_string())).transpose()
    }
    pub fn save(&mut self, e: &Entry) -> Result<()> {
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        let observed = LISTENERS.get().is_some_and(|listeners| listeners.lock().unwrap().contains_key(&self.path));
        let before: Option<Entry> = if observed {
            let text: Option<String> = tx.query_row("SELECT record FROM files WHERE id=?1", [&e.id], |row| row.get(0))
                .optional().map_err(|e| e.to_string())?;
            text.map(|text| serde_json::from_str(&text).map_err(|e| e.to_string())).transpose()?
        } else {
            None
        };
        let record = serde_json::to_string(e).map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO files VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET volume=excluded.volume,object=excluded.object,phase=excluded.phase,updated=excluded.updated,record=excluded.record",params![e.id,e.identity.volume.to_string(),e.identity.object.to_string(),serde_json::to_string(&e.phase).map_err(|e|e.to_string())?,e.updated,record]).map_err(|e|e.to_string())?;
        tx.execute("DELETE FROM search WHERE id=?1", [&e.id])
            .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO search VALUES(?1,?2,?3,?4)",
            params![
                e.id,
                e.path.file_name().unwrap_or_default().to_string_lossy(),
                e.note,
                format!("{} {}", e.folder, e.labels.join(" "))
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        if observed {
            let old = before.as_ref().filter(|old| old.phase == Phase::Filed);
            let new = (e.phase == Phase::Filed).then_some(e);
            let same = match (old, new) {
                (Some(a), Some(b)) => a.path == b.path && a.note == b.note && a.labels == b.labels && a.identity.unchanged(&b.identity),
                (None, None) => true,
                _ => false,
            };
            if !same {
                let mut paths: Vec<_> = old.into_iter().chain(new).map(|entry| entry.path.clone()).collect();
                paths.dedup();
                notify(&self.path, paths);
            }
        }
        Ok(())
    }
    pub fn remove(&mut self, id: &str) -> Result<()> {
        let e = self.get(id)?;
        if !matches!(e.phase, Phase::Draft | Phase::Filed | Phase::Unresolved) {
            return Err("Wait for this file operation to settle first".into());
        }
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM search WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM files WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        if e.phase == Phase::Filed {
            notify(&self.path, vec![e.path]);
        }
        Ok(())
    }
    pub fn pending(&self) -> Result<Vec<Entry>> {
        let mut query = self
            .db
            .prepare("SELECT record FROM files WHERE phase NOT IN ('\"draft\"','\"filed\"') OR json_extract(record,'$.staged') IS NOT NULL")
            .map_err(|e| e.to_string())?;
        let rows = query
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|row| {
            serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }
    pub fn show(&self, entity: &str, thread: &str, turn: &str, file: &str) -> Result<String> {
        self.get(file)?;
        let id = format!("cleaner-{}", uuid::Uuid::new_v4());
        self.db
            .execute(
                "INSERT OR IGNORE INTO cards VALUES(?1,?2,?3,?4,?5,?6)",
                params![id, entity, thread, turn, file, crate::util::now_millis()],
            )
            .map_err(|e| e.to_string())?;
        self.db
            .query_row(
                "SELECT id FROM cards WHERE entity=?1 AND thread=?2 AND turn=?3 AND file=?4",
                params![entity, thread, turn, file],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
    }
    pub fn cards(&self) -> Result<Vec<super::tools::Card>> {
        let mut query = self
            .db
            .prepare("SELECT id,entity,thread,turn,file,at FROM cards ORDER BY at,id")
            .map_err(|e| e.to_string())?;
        let rows = query
            .query_map([], |r| {
                Ok(super::tools::Card {
                    id: r.get(0)?,
                    entity: r.get(1)?,
                    thread: r.get(2)?,
                    turn: r.get(3)?,
                    file: r.get(4)?,
                    at: r.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.map(|r| r.map_err(|e| e.to_string())).collect()
    }
    pub fn interrupted_inbox(&self) -> Result<Vec<Entry>> {
        let mut query = self
            .db
            .prepare("SELECT record FROM files WHERE phase='\"filed\"'")
            .map_err(|e| e.to_string())?;
        let rows = query
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut entries = vec![];
        for row in rows {
            let entry: Entry = serde_json::from_str(&row.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if entry.pending_sort {
                entries.push(entry);
            }
        }
        Ok(entries)
    }
    pub fn search(&self, text: &str, offset: u64, byte_limit: usize) -> Result<Page> {
        if text.len() > 4096 || offset > i64::MAX as u64 {
            return Err("Search is too long".into());
        }
        let literal = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(" AND ");
        let (sql, count) = if literal.is_empty() {
            (
                "SELECT record FROM files WHERE ?1='' ORDER BY updated DESC,id LIMIT 256 OFFSET ?2",
                "SELECT count(*) FROM files",
            )
        } else {
            ("SELECT files.record FROM search JOIN files ON files.id=search.id WHERE search MATCH ?1 ORDER BY rank,updated DESC LIMIT 256 OFFSET ?2","SELECT count(*) FROM search WHERE search MATCH ?1")
        };
        let total: u64 = if literal.is_empty() {
            self.db.query_row(count, [], |r| r.get(0))
        } else {
            self.db.query_row(count, [&literal], |r| r.get(0))
        }
        .map_err(|e| e.to_string())?;
        let mut statement = self.db.prepare(sql).map_err(|e| e.to_string())?;
        let rows = statement
            .query_map(params![literal, offset], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut entries = vec![];
        let mut bytes = 128;
        for row in rows {
            let record = row.map_err(|e| e.to_string())?;
            if bytes + record.len() + 2 > byte_limit {
                if entries.is_empty() {
                    return Err("One file record exceeds this response budget".into());
                }
                break;
            }
            bytes += record.len() + 2;
            entries.push(serde_json::from_str(&record).map_err(|e| e.to_string())?);
        }
        let end = offset + entries.len() as u64;
        Ok(Page {
            entries,
            total,
            next: (end < total).then_some(end),
        })
    }
}
