use super::{validate_note, Entry, Identity, Phase, Result, MAX_BATCH_FILES};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
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

const SCHEMA_VERSION: i64 = 4;
const BATCH_PAGE_SIZE: u32 = 32;

pub(super) struct SavedBatch {
    pub id: String,
    pub note: Option<String>,
    pub created_at: Option<u64>,
    pub file_count: u64,
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
        if version > SCHEMA_VERSION {
            return Err("This library needs a newer RichOS".into());
        }
        if version < SCHEMA_VERSION {
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|e|e.to_string())?;
            // Another connection may have initialized while this one waited.
            // Decide only while holding the migration transaction's write lock.
            let current:i64 = tx.query_row("PRAGMA user_version",[],|r|r.get(0)).map_err(|e|e.to_string())?;
            match current {
                0 => tx.execute_batch("CREATE TABLE files(id TEXT PRIMARY KEY,volume TEXT NOT NULL,object TEXT NOT NULL,phase TEXT NOT NULL,updated INTEGER NOT NULL,record TEXT NOT NULL,UNIQUE(volume,object)); CREATE VIRTUAL TABLE search USING fts5(id UNINDEXED,name,note,labels,tokenize='unicode61 remove_diacritics 2'); CREATE TABLE cards(id TEXT PRIMARY KEY,entity TEXT NOT NULL,thread TEXT NOT NULL,turn TEXT NOT NULL,file TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,at INTEGER NOT NULL,UNIQUE(entity,thread,turn,file));").map_err(|e|e.to_string())?,
                1 => tx.execute_batch("CREATE TABLE cards(id TEXT PRIMARY KEY,entity TEXT NOT NULL,thread TEXT NOT NULL,turn TEXT NOT NULL,file TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,at INTEGER NOT NULL,UNIQUE(entity,thread,turn,file));").map_err(|e|e.to_string())?,
                2..=SCHEMA_VERSION => {},
                _ => return Err("This library needs a newer RichOS".into()),
            }
            if current < SCHEMA_VERSION {
                add_batch_history(&tx, current)?;
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)
                    .map_err(|e| e.to_string())?;
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
        get_entry(&self.db, id)
    }
    pub fn by_identity(&self, volume: u64, object: u64) -> Result<Option<Entry>> {
        entry_by_identity(&self.db, volume, object)
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
        save_record(&tx, e)?;
        tx.commit().map_err(|e| e.to_string())?;
        if observed {
            let old = before.as_ref().filter(|old| old.phase == Phase::Filed);
            let new = (e.phase == Phase::Filed).then_some(e);
            let same = match (old, new) {
                (Some(a), Some(b)) => a.path == b.path && a.note == b.note && a.labels == b.labels && a.identity.unchanged(&b.identity),
                (None, None) => true,
                _ => false,
            };
            let relocated = before.as_ref().filter(|prior| {
                matches!(prior.phase, Phase::Moving | Phase::Restoring)
                    && matches!(e.phase, Phase::Draft | Phase::Filed)
                    && prior.path != e.path
            });
            if let Some(prior) = relocated {
                // A move can finish after search already saw the old location.
                notify(&self.path, vec![prior.path.clone(), e.path.clone()]);
            } else if !same {
                let mut paths: Vec<_> = old.into_iter().chain(new).map(|entry| entry.path.clone()).collect();
                paths.dedup();
                notify(&self.path, paths);
            }
        }
        Ok(())
    }

    /// Commit every selected file's draft and search row before any original can move.
    pub fn enroll_batch(
        &mut self,
        batch_id: &str,
        files: Vec<(PathBuf, Identity)>,
        shared_note: &str,
    ) -> Result<Vec<Entry>> {
        if !(2..=MAX_BATCH_FILES).contains(&files.len()) {
            return Err(format!("Select between 2 and {MAX_BATCH_FILES} files for one note"));
        }
        if uuid::Uuid::parse_str(batch_id).is_err() {
            return Err("The selected files have no valid shared batch ID".into());
        }
        validate_note(shared_note)?;

        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let now = crate::util::now_millis();
        let created = tx
            .execute(
                "INSERT INTO cleaner_batches(id,note,created_at) VALUES(?1,?2,?3)
                 ON CONFLICT(id) DO NOTHING",
                params![batch_id, shared_note, now],
            )
            .map_err(|e| e.to_string())?;
        if created == 0 {
            return Err("This batch is already saved".into());
        }
        let mut enrolled = Vec::with_capacity(files.len());
        let mut objects = HashSet::with_capacity(files.len());
        let mut former_locations = Vec::new();
        for (path, identity) in files {
            if !objects.insert((identity.volume, identity.object)) {
                return Err("Select each original only once".into());
            }
            let mut entry = match entry_by_identity(&tx, identity.volume, identity.object)? {
                Some(mut entry) => {
                    entry.check_batch_eligibility()?;
                    if entry.phase == Phase::Filed {
                        former_locations.push(entry.path);
                    }
                    if entry.note != shared_note
                        && !entry.note.strip_suffix(shared_note).is_some_and(|p| p.ends_with("\n\n"))
                    {
                        let combined = format!("{}\n\n{shared_note}", entry.note);
                        validate_note(&combined).map_err(|_| {
                            "A selected file's combined note is too long. Use a shorter shared note"
                        })?;
                        entry.note = combined;
                        if entry.projection.mode == "embedded" {
                            entry.projection.mode = "older".into();
                        }
                    }
                    entry.path = path;
                    entry.identity = identity;
                    entry.folder.clear();
                    entry.phase = Phase::Draft;
                    entry.undo = None;
                    entry.inbox = false;
                    entry.problem = None;
                    entry
                }
                None => Entry::new(path, identity, shared_note.into())?,
            };
            entry.batch_id = Some(batch_id.into());
            entry.updated = now;
            save_record(&tx, &entry)?;
            tx.execute(
                "INSERT INTO cleaner_batch_members(batch,file) VALUES(?1,?2)",
                params![batch_id, entry.id],
            )
            .map_err(|e| e.to_string())?;
            enrolled.push(entry);
        }
        tx.commit().map_err(|e| e.to_string())?;
        notify(&self.path, former_locations);
        Ok(enrolled)
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
        save_card(&self.db, entity, thread, turn, file)
    }

    pub(super) fn batch_count(&self, file: &str) -> Result<u64> {
        count_saved_batches(&self.db, file)
    }

    pub(super) fn batch_page(&self, file: &str, offset: u64) -> Result<(Vec<SavedBatch>, u64)> {
        if offset > i64::MAX as u64 {
            return Err("Search offset is too large".into());
        }
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        get_entry(&tx, file)?;
        let total = count_saved_batches(&tx, file)?;
        let mut query = tx
            .prepare(
                "SELECT b.id,b.note,b.created_at,
                        (SELECT count(*) FROM cleaner_batch_members WHERE batch=b.id)
                 FROM cleaner_batch_members m JOIN cleaner_batches b ON b.id=m.batch
                 WHERE m.file=?1 ORDER BY b.created_at DESC,b.id LIMIT ?2 OFFSET ?3",
            )
            .map_err(|e| e.to_string())?;
        let rows = query
            .query_map(params![file, BATCH_PAGE_SIZE, offset], |row| {
                Ok(SavedBatch {
                    id: row.get(0)?,
                    note: row.get(1)?,
                    created_at: row.get(2)?,
                    file_count: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let batches = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        Ok((batches, total))
    }

    pub(super) fn show_batch(
        &mut self,
        entity: &str,
        thread: &str,
        turn: &str,
        file: &str,
        batch_id: Option<&str>,
    ) -> Result<Vec<(String, String)>> {
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let batch = choose_saved_batch(&tx, file, batch_id)?;
        let members = {
            let mut query = tx
                .prepare("SELECT file FROM cleaner_batch_members WHERE batch=?1 ORDER BY file LIMIT ?2")
                .map_err(|e| e.to_string())?;
            let rows = query
                .query_map(params![batch, (MAX_BATCH_FILES + 1) as i64], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?
        };
        if members.len() > MAX_BATCH_FILES {
            return Err("The saved batch exceeds the file limit".into());
        }
        let mut cards = Vec::with_capacity(members.len());
        for member in members {
            let card = save_card(&tx, entity, thread, turn, &member)?;
            cards.push((member, card));
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(cards)
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
    pub(super) fn total_files(&self) -> Result<u64> {
        self.db.query_row("SELECT count(*) FROM files", [], |r| r.get(0))
            .map_err(|e| e.to_string())
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
        let (sql, total) = if literal.is_empty() {
            ("SELECT record FROM files WHERE ?1='' ORDER BY updated DESC,id LIMIT 256 OFFSET ?2", self.total_files()?)
        } else {
            let total = self.db.query_row("SELECT count(*) FROM search WHERE search MATCH ?1", [&literal], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            ("SELECT files.record FROM search JOIN files ON files.id=search.id WHERE search MATCH ?1 ORDER BY rank,updated DESC LIMIT 256 OFFSET ?2", total)
        };
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

fn add_batch_history(db: &Connection, previous_version: i64) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE cleaner_batches(id TEXT PRIMARY KEY,note TEXT,created_at INTEGER);
         CREATE TABLE cleaner_batch_members(
             batch TEXT NOT NULL REFERENCES cleaner_batches(id) ON DELETE CASCADE,
             file TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
             PRIMARY KEY(batch,file)
         );
         CREATE INDEX cleaner_batches_by_file ON cleaner_batch_members(file,batch);
         CREATE TRIGGER cleaner_prune_empty_batch AFTER DELETE ON cleaner_batch_members
         WHEN NOT EXISTS (SELECT 1 FROM cleaner_batch_members WHERE batch=OLD.batch)
         BEGIN DELETE FROM cleaner_batches WHERE id=OLD.batch; END;",
    )
    .map_err(|e| e.to_string())?;
    if previous_version == 3 {
        // V3 kept only the latest ID on each file. Preserve the memberships still
        // present. Its file timestamps and diverging notes do not identify the
        // original capture time or shared note.
        db.execute_batch(
            "INSERT INTO cleaner_batches(id,note,created_at)
             SELECT json_extract(record,'$.batchId'),
                    CASE WHEN count(*)>1 AND min(json_extract(record,'$.note'))=max(json_extract(record,'$.note'))
                         THEN min(json_extract(record,'$.note')) END,
                    NULL
             FROM files WHERE json_extract(record,'$.batchId') IS NOT NULL
             GROUP BY json_extract(record,'$.batchId');
             INSERT INTO cleaner_batch_members(batch,file)
             SELECT json_extract(record,'$.batchId'),id
             FROM files WHERE json_extract(record,'$.batchId') IS NOT NULL;
             DROP INDEX IF EXISTS files_batch;",
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn choose_saved_batch(db: &Connection, file: &str, batch_id: Option<&str>) -> Result<String> {
    if let Some(batch) = batch_id {
        let member: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM cleaner_batch_members WHERE batch=?1 AND file=?2)",
                params![batch, file],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        return member
            .then(|| batch.into())
            .ok_or_else(|| "The selected file is not in that saved set".into());
    }
    let mut query = db
        .prepare("SELECT batch FROM cleaner_batch_members WHERE file=?1 LIMIT 2")
        .map_err(|e| e.to_string())?;
    let rows = query
        .query_map([file], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let batches = rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?;
    match batches.as_slice() {
        [batch] => Ok(batch.clone()),
        [] => Err("This original was saved individually".into()),
        _ => Err("This file belongs to several saved sets. Use library_search with batchFor and choose a batchId".into()),
    }
}

fn count_saved_batches(db: &Connection, file: &str) -> Result<u64> {
    db.prepare_cached("SELECT count(*) FROM cleaner_batch_members WHERE file=?1")
        .and_then(|mut query| query.query_row([file], |row| row.get(0)))
        .map_err(|e| e.to_string())
}

fn get_entry(db: &Connection, id: &str) -> Result<Entry> {
    let text: String = db
        .query_row("SELECT record FROM files WHERE id=?1", [id], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("The saved file record could not be read: {e}"))
}

fn entry_by_identity(db: &Connection, volume: u64, object: u64) -> Result<Option<Entry>> {
    let id: Option<String> = db
        .query_row(
            "SELECT id FROM files WHERE volume=?1 AND object=?2",
            params![volume.to_string(), object.to_string()],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    id.map(|id| get_entry(db, &id)).transpose()
}

fn save_record(db: &Connection, e: &Entry) -> Result<()> {
    let record = serde_json::to_string(e).map_err(|e| e.to_string())?;
    db.execute("INSERT INTO files VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET volume=excluded.volume,object=excluded.object,phase=excluded.phase,updated=excluded.updated,record=excluded.record",params![e.id,e.identity.volume.to_string(),e.identity.object.to_string(),serde_json::to_string(&e.phase).map_err(|e|e.to_string())?,e.updated,record]).map_err(|e|e.to_string())?;
    db.execute("DELETE FROM search WHERE id=?1", [&e.id])
        .map_err(|e| e.to_string())?;
    db.execute(
        "INSERT INTO search VALUES(?1,?2,?3,?4)",
        params![
            e.id,
            e.path.file_name().unwrap_or_default().to_string_lossy(),
            e.note,
            format!("{} {}", e.folder, e.labels.join(" "))
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn save_card(db: &Connection, entity: &str, thread: &str, turn: &str, file: &str) -> Result<String> {
    let id = format!("cleaner-{}", uuid::Uuid::new_v4());
    db.execute(
        "INSERT OR IGNORE INTO cards VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, entity, thread, turn, file, crate::util::now_millis()],
    )
    .map_err(|e| e.to_string())?;
    db.query_row(
        "SELECT id FROM cards WHERE entity=?1 AND thread=?2 AND turn=?3 AND file=?4",
        params![entity, thread, turn, file],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}
