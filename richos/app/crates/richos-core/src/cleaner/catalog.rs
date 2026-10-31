use super::{Entry, Phase, Result};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use std::path::Path;
pub struct Catalog {
    db: Connection,
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
        let db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| e.to_string())?;
        db.busy_timeout(std::time::Duration::from_secs(3))
            .map_err(|e| e.to_string())?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;",
        )
        .map_err(|e| e.to_string())?;
        let version: i64 = db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if version > 1 {
            return Err("This library needs a newer RichOS".into());
        }
        db.execute_batch("BEGIN IMMEDIATE; CREATE TABLE IF NOT EXISTS files(id TEXT PRIMARY KEY,volume TEXT NOT NULL,object TEXT NOT NULL,phase TEXT NOT NULL,updated INTEGER NOT NULL,record TEXT NOT NULL,UNIQUE(volume,object)); CREATE VIRTUAL TABLE IF NOT EXISTS search USING fts5(id UNINDEXED,name,note,labels,tokenize='unicode61 remove_diacritics 2'); PRAGMA user_version=1; COMMIT;").map_err(|e|e.to_string())?;
        Ok(Self { db })
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
    pub fn save(&mut self, e: &Entry) -> Result<()> {
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
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
        tx.commit().map_err(|e| e.to_string())
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
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn pending(&self) -> Result<Vec<Entry>> {
        let mut query = self
            .db
            .prepare("SELECT record FROM files WHERE phase NOT IN ('\"draft\"','\"filed\"')")
            .map_err(|e| e.to_string())?;
        let rows = query
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|row| {
            serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
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
