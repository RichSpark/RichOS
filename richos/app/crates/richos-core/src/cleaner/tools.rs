//! Rich's read-only personal library search and an id-bound conversation card. Never file tools.
use super::{Catalog, Result};
use crate::question_tools::Scope;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
pub struct Card {
    pub id: String,
    pub entity: String,
    pub thread: String,
    pub turn: String,
    pub file: String,
    pub at: u64,
}
pub fn catalog_path(engine_state: &Path) -> Result<PathBuf> {
    if engine_state.file_name() != Some(std::ffi::OsStr::new("engine-state")) {
        return Err("The personal library scope is unavailable".into());
    }
    Ok(engine_state
        .parent()
        .ok_or("No application data folder")?
        .join("RichCleaner/library.sqlite"))
}
pub fn allowed(scope: &Scope) -> bool {
    scope.context.asker == "front_desk"
        && scope.surface == "mac"
        && scope.context.obligation_id.is_none()
}
pub fn definitions() -> Vec<Value> {
    vec![
        json!({"name":"library_search","description":"Search the user's RichCleaner notes, names and folder labels. Use ordinary search words, never FTS syntax. Empty query lists saved metadata for meaning-based inspection when lexical matches are weak. A noteTruncated preview retains its full note through library_show. Each answer names total matches, total saved and continuation; inspect remaining batches before claiming that no file exists. Never reads file contents or moves files.","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"offset":{"type":"integer","minimum":0}},"additionalProperties":false}}),
        json!({"name":"library_show","description":"Put one actual saved original in front of the user as a RichCleaner file card. Give an id returned by library_search. Opening resolves the current original on the host; no model-supplied file path is accepted. Does not move the file.","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}}),
    ]
}
pub fn call(scope: &Scope, name: &str, args: &Value) -> Result<Value> {
    if !allowed(scope) || !scope.actions_allowed {
        return Err(
            "The personal library is available only to Rich during the user's current conversation"
                .into(),
        );
    }
    let db = Catalog::open(&catalog_path(&scope.context.root)?)?;
    let keys = args.as_object().ok_or("Expected library arguments")?;
    match name {
        "library_search" => {
            if keys.keys().any(|k| k != "query" && k != "offset") {
                return Err("Unknown search argument".into());
            }
            let query = args
                .get("query")
                .map(|v| v.as_str().ok_or("Search text must be a string"))
                .transpose()?
                .unwrap_or_default();
            let offset = args
                .get("offset")
                .map(|v| v.as_u64().ok_or("Offset must be a nonnegative integer"))
                .transpose()?
                .unwrap_or(0);
            let page = db.search(query, offset, 512 * 1024)?;
            let total = db.search("", 0, 512 * 1024)?.total;
            let mut entries = vec![];
            let mut bytes = 1024;
            for e in page.entries {
                // Search carries a bounded preview. A valid maximum-length note can
                // exceed the entire response budget after JSON escaping; returning no
                // items with the same continuation would hide it and every later item.
                let mut end = e.note.len().min(16 * 1024);
                while !e.note.is_char_boundary(end) { end -= 1; }
                let mut item = json!({"id":e.id,"name":e.path.file_name().map(|v|v.to_string_lossy()),"note":&e.note[..end],"folder":e.folder,"aliases":e.labels,"inbox":e.inbox,"sorting":e.pending_sort});
                if end < e.note.len() { item["noteTruncated"] = json!(true); }
                let size = serde_json::to_vec(&item).map_err(|e| e.to_string())?.len();
                if bytes + size > 64 * 1024 {
                    break;
                }
                bytes += size;
                entries.push(item);
            }
            let end = offset + entries.len() as u64;
            Ok(
                json!({"entries":entries,"totalMatches":page.total,"totalSaved":total,"next":if end<page.total{Some(end)}else{None},"query":query,"fileContentsRead":false}),
            )
        }
        "library_show" => {
            if keys.keys().any(|k| k != "id") {
                return Err("Give only the saved file id".into());
            }
            let id = args["id"].as_str().ok_or("Give a saved file id")?;
            let entry = db.get(id)?;
            let s = &scope.context;
            let card = db.show(&s.entity_id, &s.thread_id, &s.turn_id, id)?;
            Ok(
                json!({"shown":true,"cardId":card,"id":id,"name":entry.path.file_name().map(|v|v.to_string_lossy()),"folder":entry.folder,"note":entry.note}),
            )
        }
        _ => Err("Unknown library tool".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_teammates_have_no_personal_library() {
        let mut scope:Scope=serde_json::from_value(json!({"context":{"root":"/app/engine-state","entity_id":"company","thread_id":"thread","turn_id":"turn","asker":"stu","session_id":"session"},"actions_allowed":true,"answer_method":"typed","surface":"mac"})).unwrap();
        assert!(!allowed(&scope));
        scope.context.asker = "front_desk".into();
        assert!(allowed(&scope));
        scope.surface = "phone".into();
        assert!(!allowed(&scope));
    }
    #[test]
    fn maximum_escaped_note_still_advances_and_show_retains_the_full_note() {
        let root = std::env::temp_dir().join(format!("richcleaner-long-note-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let scope:Scope=serde_json::from_value(json!({"context":{"root":root.join("engine-state"),"entity_id":"company","thread_id":"thread","turn_id":"turn","asker":"front_desk","session_id":"session"},"actions_allowed":true,"answer_method":"typed","surface":"mac"})).unwrap();
        let mut db = Catalog::open(&catalog_path(&scope.context.root).unwrap()).unwrap();
        let note = "\"".repeat(32 * 1024);
        let entry = super::super::Entry::new(root.join("large.png"), super::super::Identity { volume: 1, object: 1, size: 12, modified: "1".into(), changed: "1".into(), bookmark: String::new() }, note.clone()).unwrap();
        db.save(&entry).unwrap();
        let page = call(&scope, "library_search", &json!({"query":""})).unwrap();
        assert_eq!(page["entries"][0]["id"], entry.id);
        assert_eq!(page["entries"][0]["noteTruncated"], true);
        assert_eq!(page["next"], Value::Null);
        assert!(serde_json::to_vec(&page).unwrap().len() <= 64 * 1024);
        assert_eq!(call(&scope,"library_show",&json!({"id":entry.id})).unwrap()["note"],note);
        assert_eq!(db.get(&entry.id).unwrap().note,note);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn bounded_search_lists_every_saved_original_and_show_uses_only_its_id() {
        let root = std::env::temp_dir().join(format!("richcleaner-tools-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let mut scope:Scope=serde_json::from_value(json!({"context":{"root":root.join("engine-state"),"entity_id":"company","thread_id":"thread","turn_id":"turn","asker":"front_desk","session_id":"session"},"actions_allowed":true,"answer_method":"typed","surface":"mac"})).unwrap();
        let mut db = Catalog::open(&catalog_path(&scope.context.root).unwrap()).unwrap();
        let mut wanted = String::new();
        for i in 0..1000 {
            let note = format!(
                "Kitchen quote {i}. {}",
                "Context for finding this original. ".repeat(24)
            );
            let e = super::super::Entry::new(
                root.join(format!("quote-{i}.png")),
                super::super::Identity {
                    volume: 1,
                    object: i + 1,
                    size: 12,
                    modified: "1".into(),
                    changed: "1".into(),
                    bookmark: "native-bookmark-not-for-the-model".into(),
                },
                note,
            )
            .unwrap();
            wanted = e.id.clone();
            db.save(&e).unwrap();
        }
        let first = call(&scope, "library_search", &json!({"query":"Kitchen"})).unwrap();
        assert_eq!(first["totalMatches"], 1000);
        assert_eq!(first["totalSaved"], 1000);
        assert!(serde_json::to_vec(&first).unwrap().len() <= 64 * 1024);
        assert!(!serde_json::to_string(&first)
            .unwrap()
            .contains("native-bookmark"));
        assert!(first["entries"][0]["name"].is_string());
        let mut seen = std::collections::HashSet::new();
        let mut page = first;
        loop {
            for entry in page["entries"].as_array().unwrap() {
                assert!(seen.insert(entry["id"].as_str().unwrap().to_string()));
            }
            let Some(next) = page["next"].as_u64() else {
                break;
            };
            page = call(
                &scope,
                "library_search",
                &json!({"query":"Kitchen","offset":next}),
            )
            .unwrap();
        }
        assert_eq!(seen.len(), 1000);
        assert_eq!(
            call(&scope, "library_search", &json!({"query":"unmentioned"})).unwrap()
                ["totalMatches"],
            0
        );
        let shown = call(&scope, "library_show", &json!({"id":wanted})).unwrap();
        assert_eq!(shown["id"], wanted);
        assert_eq!(db.cards().unwrap().len(), 1);
        assert!(call(&scope, "library_show", &json!({"id":"/Desktop/anything"})).is_err());
        scope.actions_allowed = false;
        assert!(call(&scope, "library_search", &json!({})).is_err());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
