//! One accepted note and one folder choice for a frozen Desktop selection.
//! Each member uses the existing recoverable single-file move and Undo.
use super::{catalog, finish, host, settings, stage, start_target, Host};
use crate::{cleaner_native::Native, AppState};
use richos_core::cleaner::{self, Catalog, Files, FilingChoice, Identity, Phase};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use tauri::{AppHandle, Manager};

#[derive(Deserialize)]
struct Target {
    path: PathBuf,
    identity: Identity,
}

pub(super) struct ActiveBatch {
    pending: HashSet<String>,
}

impl ActiveBatch {
    pub(super) fn contains(&self, id: &str) -> bool {
        self.pending.contains(id)
    }
}

/// Called with the mutation lock held so an edit or Undo wins over a later move.
pub(super) fn skip(host: &Host, id: &str) -> bool {
    host.active
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .and_then(|active| active.batch.as_mut())
        .is_some_and(|batch| batch.pending.remove(id))
}

fn targets(payload: &Value) -> Result<Vec<Target>, String> {
    let targets: Vec<Target> = serde_json::from_value(payload["targets"].clone())
        .map_err(|_| "Could not read the selected files".to_string())?;
    if !(2..=cleaner::MAX_BATCH_FILES).contains(&targets.len()) {
        return Err(format!(
            "Select up to {} Desktop files",
            cleaner::MAX_BATCH_FILES
        ));
    }
    let mut objects = HashSet::new();
    if targets
        .iter()
        .any(|file| !objects.insert((file.identity.volume, file.identity.object)))
    {
        return Err("Select each original only once".into());
    }
    Ok(targets)
}

pub(super) fn begin(app: &AppHandle, id: &str, payload: Value) -> Result<(), String> {
    let targets = targets(&payload)?;
    let db = catalog(app)?;
    let mut names = Vec::with_capacity(targets.len());
    for file in &targets {
        if let Some(entry) = db.by_identity(file.identity.volume, file.identity.object)? {
            entry.check_batch_eligibility()?;
        }
        let name = file
            .path
            .file_name()
            .ok_or("A selected filename is unavailable")?;
        names.push(name.to_string_lossy().into_owned());
    }
    start_target(
        app,
        id,
        payload,
        json!({"batch":{"total":names.len(),"names":names}}),
    )
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Outcome {
    filed: usize,
    inbox: usize,
    failed: usize,
    skipped: usize,
    draft: usize,
    local_note: usize,
}

#[derive(Clone, Copy)]
enum ResultKind {
    Filed,
    Inbox,
    Failed,
    Skipped,
    Draft,
}

impl Outcome {
    fn add(&mut self, result: ResultKind) {
        match result {
            ResultKind::Filed => self.filed += 1,
            ResultKind::Inbox => self.inbox += 1,
            ResultKind::Failed => self.failed += 1,
            ResultKind::Skipped => self.skipped += 1,
            ResultKind::Draft => self.draft += 1,
        }
    }
}

pub(super) fn accept(
    app: &AppHandle,
    id: &str,
    payload: &Value,
    note: String,
) -> Result<(), String> {
    let began = Instant::now();
    let files = targets(payload)?;
    let h = host(app);
    let mutation = h
        .mutation
        .lock()
        .map_err(|_| "File operations unavailable")?;
    let mut active = h.active.lock().map_err(|_| "Capture state unavailable")?;
    let Some(current) = active
        .as_mut()
        .filter(|op| op.id == id && op.stage == "writing")
    else {
        return Ok(());
    };
    if current.cancel.load(Ordering::Acquire) {
        return Ok(());
    }
    let mut db = catalog(app)?;
    let entries = db.enroll_batch(
        id,
        files
            .into_iter()
            .map(|file| (file.path, file.identity))
            .collect(),
        &note,
    )?;
    let entry_ids: Vec<_> = entries.iter().map(|entry| entry.id.clone()).collect();
    current.batch = Some(ActiveBatch {
        pending: entry_ids.iter().cloned().collect(),
    });
    current.stage = "choosing".into();
    let chooser = current.chooser.clone();
    let cancel = current.cancel.clone();
    drop(active);
    drop(mutation);
    let total = entries.len();
    let state = app.state::<AppState>();
    let fast = state
        .config
        .lock()
        .map_err(|_| "Settings unavailable")?
        .cleaner_inbox_first();
    let mut record = json!({"id":id,"build":app.package_info().version.to_string(),"count":total,"inbox_first":fast});
    let outcome = (|| {
        stage(app, id, "saved", json!({"noteSaved":true,"batchNote":note}));
        record["drafts_saved_ms"] = json!(began.elapsed().as_millis() as u64);
        let root = settings(app)?.1.ok_or("Choose a filing folder")?;
        let native = Native::bundled()?;
        native.request(json!({"action":"validate-root","path":root}))?;
        let mut outcomes = Outcome::default();
        let mut ready = Vec::with_capacity(total);
        for entry in entries {
            if fast {
                let _mutation = h
                    .mutation
                    .lock()
                    .map_err(|_| "File operations unavailable")?;
                if let Some(result) = pending_status(&h, id, &entry.id, &cancel) {
                    outcomes.add(result);
                    continue;
                }
                if let Err(reason) = cleaner::store_inbox(&mut db, &native, &entry.id, &root) {
                    fail_member(&mut db, &native, &entry.id, reason)?;
                    skip(&h, &entry.id);
                    outcomes.add(ResultKind::Failed);
                    continue;
                }
            }
            ready.push(entry);
        }
        if fast {
            record["inbox_moves_ms"] = json!(began.elapsed().as_millis() as u64);
            if !ready.is_empty() {
                stage(
                    app,
                    id,
                    "inbox-pending",
                    json!({"batchStored":ready.len(),"noteSaved":true}),
                );
            }
        }
        stage(
            app,
            id,
            "choosing",
            json!({"noteSaved":true,"force":false,"message":null}),
        );
        let choice_started = Instant::now();
        let choice = if ready.is_empty() || cancel.load(Ordering::Acquire) {
            None
        } else {
            let names = ready
                .iter()
                .map(|entry| {
                    entry
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect::<Vec<_>>();
            Some(super::folder_context(&db, &root, id).and_then(|folders| {
                chooser
                    .as_ref()
                    .ok_or("The model connection is unavailable".to_string())?
                    .choose_batch(&names, &note, &folders)
            }))
        };
        let choice = super::folder_outcome(choice.unwrap_or_else(|| Err("Folder choice canceled".into())));
        let folder = choice.folder();
        record["choice_ms"] = json!(choice_started.elapsed().as_millis() as u64);
        record["choice"] = json!(if cancel.load(Ordering::Acquire) {
            "canceled"
        } else if ready.is_empty() {
            "unavailable"
        } else if matches!(&choice, FilingChoice::Inbox { .. }) {
            "inbox"
        } else {
            "category"
        });
        if enter_filing(&h, id)? && !ready.is_empty() {
            stage(
                app,
                id,
                "filing",
                json!({"folder":folder,"noteSaved":true,"force":false,"message":null}),
            );
        }
        for entry in ready {
            let _mutation = h
                .mutation
                .lock()
                .map_err(|_| "File operations unavailable")?;
            let result = match pending_status(&h, id, &entry.id, &cancel) {
                Some(ResultKind::Draft) if fast => {
                    let result = settle_canceled_member(app, &mut db, &native, &entry.id)?;
                    if matches!(result, ResultKind::Inbox) {
                        outcomes.local_note += 1;
                    }
                    result
                }
                Some(result) => result,
                None => match cleaner::file(
                    &mut db,
                    &native,
                    &entry.id,
                    &root,
                    &choice,
                ) {
                    Ok(saved) => {
                        if saved.projection.mode != "embedded" {
                            outcomes.local_note += 1;
                        }
                        if saved.inbox {
                            ResultKind::Inbox
                        } else {
                            ResultKind::Filed
                        }
                    }
                    Err(reason) => {
                        fail_member(&mut db, &native, &entry.id, reason)?;
                        ResultKind::Failed
                    }
                },
            };
            skip(&h, &entry.id);
            outcomes.add(result);
        }
        Ok::<_, String>((folder.to_owned(), outcomes))
    })();
    if outcome.is_err() {
        let mut unresolved = 0;
        if let (Ok(_mutation), Ok(native)) = (h.mutation.lock(), Native::bundled()) {
            for entry_id in &entry_ids {
                if skip(&h, entry_id) && cleaner::recover_entry(&mut db, &native, entry_id).is_err()
                {
                    unresolved += 1;
                }
            }
        } else {
            unresolved = total;
        }
        record["recovery_failures"] = json!(unresolved);
    }
    record["accepted_note_to_end_ms"] = json!(began.elapsed().as_millis() as u64);
    record["outcome"] = match &outcome {
        Ok((_, result)) => json!(result),
        Err(_) => json!("interrupted"),
    };
    crate::dictation::log::line(&format!("RichCleaner batch {record}"));
    let (folder, result) = outcome?;
    finish(
        app,
        id,
        json!({"id":id,"stage":"batch-result","noteSaved":true,
        "batch":{"total":total},"batchNote":note,"folder":(result.filed > 0).then_some(folder),
        "force":result.inbox > 0 || result.failed > 0 || result.draft > 0 || result.skipped > 0 || result.local_note > 0,
        "batchResult":result}),
    );
    Ok(())
}

fn pending_status(h: &Host, id: &str, entry: &str, cancel: &AtomicBool) -> Option<ResultKind> {
    let active = h.active.lock().unwrap_or_else(|e| e.into_inner());
    let pending = active
        .as_ref()
        .filter(|op| op.id == id)
        .and_then(|op| op.batch.as_ref())
        .is_some_and(|batch| batch.contains(entry));
    if !pending {
        Some(ResultKind::Skipped)
    } else if cancel.load(Ordering::Acquire) {
        Some(ResultKind::Draft)
    } else {
        None
    }
}

fn enter_filing(h: &Host, id: &str) -> Result<bool, String> {
    let mut active = h.active.lock().map_err(|_| "Operation state unavailable")?;
    let current = active
        .as_mut()
        .filter(|op| op.id == id)
        .ok_or("The operation ended")?;
    if current.cancel.load(Ordering::Acquire) {
        return Ok(false);
    }
    current.stage = "filing".into();
    Ok(true)
}

fn fail_member(db: &mut Catalog, native: &Native, id: &str, reason: String) -> Result<(), String> {
    cleaner::recover_entry(db, native, id)?;
    let mut entry = db.get(id)?;
    entry.problem = Some(reason);
    db.save(&entry)
}

fn settle_canceled_member(
    app: &AppHandle,
    db: &mut Catalog,
    native: &Native,
    id: &str,
) -> Result<ResultKind, String> {
    let mut saved = db.get(id)?;
    if !matches!(saved.phase, Phase::Draft | Phase::Filed) {
        // A failed Undo can have moved the file before its native reply failed.
        // Reconcile that intent before settling the batch's sorting flag.
        cleaner::recover_entry(db, native, id)?;
        saved = db.get(id)?;
    }
    match saved.phase {
        Phase::Draft => return Ok(ResultKind::Skipped),
        Phase::Filed => {}
        _ => return Ok(ResultKind::Failed),
    }
    if saved.pending_sort {
        saved = cleaner::cancel_sort(db, id)?;
    }
    match native.resolve(&saved.path, &saved.identity) {
        Ok(path) => {
            if path != saved.path {
                super::located(app, &mut saved, path);
                db.save(&saved)?;
            }
            Ok(if saved.inbox {
                ResultKind::Inbox
            } else {
                ResultKind::Skipped
            })
        }
        Err(reason) => {
            saved.problem = Some(reason);
            db.save(&saved)?;
            Ok(ResultKind::Failed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cleaner_host::Active;
    use std::sync::Arc;

    #[test]
    fn batch_rejects_repeated_originals_and_oversized_selections() {
        let file = |object: u64, name: &str| {
            json!({"path":format!("/Desktop/{name}"),
                "identity":{"volume":7,"object":object,"size":1,
                    "modified":"1","changed":"1","bookmark":""}})
        };
        let valid = json!({"targets":[file(1,"one"),file(2,"two")]});
        assert_eq!(targets(&valid).unwrap().len(), 2);
        let repeated = json!({"targets":[file(1,"one"),file(1,"alias")]});
        assert!(targets(&repeated).is_err());
        let too_many: Vec<_> = (0..=cleaner::MAX_BATCH_FILES as u64)
            .map(|i| file(i, &i.to_string()))
            .collect();
        assert!(targets(&json!({"targets":too_many})).is_err());
    }

    #[test]
    fn editing_or_undoing_one_member_does_not_cancel_its_sibling() {
        let host = Host::default();
        let cancel = Arc::new(AtomicBool::new(false));
        *host.active.lock().unwrap() = Some(Active {
            id: "operation".into(),
            target: None,
            entry: None,
            batch: Some(ActiveBatch {
                pending: ["one".into(), "two".into()].into(),
            }),
            stage: "choosing".into(),
            cancel: cancel.clone(),
            chooser: None,
            timings: Default::default(),
        });
        assert!(skip(&host, "one"));
        assert!(matches!(
            pending_status(&host, "operation", "one", &cancel),
            Some(ResultKind::Skipped)
        ));
        assert!(pending_status(&host, "operation", "two", &cancel).is_none());
        cancel.store(true, Ordering::Release);
        assert!(!enter_filing(&host, "operation").unwrap());
        assert!(matches!(
            pending_status(&host, "operation", "two", &cancel),
            Some(ResultKind::Draft)
        ));
    }
}
