//! One tool-free folder choice. Startup overlaps speech; one request follows durable note save.
use super::{folder_parts, FilingChoice, Result};
use crate::{engine_profile::EngineProfile, request_classifier::Provider};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU8, Ordering},
        mpsc, Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub folder: String,
    pub keywords: Vec<String>,
    pub aliases: Vec<String>,
    pub clear: bool,
}

pub struct FolderContext {
    pub labels: Vec<String>,
    pub complete: bool,
}

const MAX_REQUEST_BYTES: usize = 60 * 1024;
const INCOMPLETE: &str =
    "The folder list was incomplete and no listed folder fit. Edit the note or choose a smaller filing folder, then use File again";
const INVALID: &str = "The folder model returned an invalid choice. Use File again to retry";

fn wire(input: &Value) -> String {
    json!({"type":"user","message":{"role":"user","content":input.to_string()}}).to_string()
}

fn request(mut input: Value, folders: &FolderContext) -> Result<(String, usize, bool)> {
    input["folders"] = json!([]);
    input["foldersComplete"] = json!(false);
    let mut remaining = MAX_REQUEST_BYTES.checked_sub(wire(&input).len())
        .ok_or("The note and filenames exceed the folder model's input limit. Shorten the note, then use File again")?;
    let mut sent = 0;
    for label in &folders.labels {
        // Each label is a JSON string inside the provider's JSON content string.
        let encoded = json!(label).to_string();
        let cost = json!(encoded).to_string().len() - 2 + usize::from(sent > 0);
        let Some(rest) = remaining.checked_sub(cost) else { break };
        remaining = rest;
        sent += 1;
    }
    let complete = folders.complete && sent == folders.labels.len();
    input["folders"] = json!(folders.labels[..sent]);
    input["foldersComplete"] = json!(complete);
    Ok((wire(&input), sent, complete))
}

fn selection(choice: Choice, sent: &[String], complete: bool) -> FilingChoice {
    if !choice.clear || choice.folder == "Inbox" {
        return FilingChoice::Inbox {
            reason: if complete {
                "The note did not make the right folder clear. Edit the note, then use File again"
            } else {
                INCOMPLETE
            }.into(),
        };
    }
    let existing_only = sent.contains(&choice.folder);
    if !complete && !existing_only {
        return FilingChoice::Inbox { reason: INCOMPLETE.into() };
    }
    let mut labels = choice.keywords;
    labels.extend(choice.aliases);
    FilingChoice::category(choice.folder, labels, existing_only)
}

struct Request {
    body: String,
    deadline: Instant,
    reply: mpsc::Sender<Result<Choice>>,
}
pub struct Prepared {
    started: Instant,
    send: Option<mpsc::Sender<Request>>,
    state: Arc<AtomicU8>,
    worker: Option<JoinHandle<()>>,
}
const LAUNCHING: u8 = 1;
const CANCELED: u8 = 2;

impl Prepared {
    pub fn start(
        prepare: impl FnOnce() -> Result<(PathBuf, EngineProfile)> + Send + 'static,
    ) -> Self {
        let started = Instant::now();
        let state = Arc::new(AtomicU8::new(0));
        let worker_state = state.clone();
        let (send, receive) = mpsc::channel::<Request>();
        let worker = std::thread::spawn(move || {
            let canceled = || worker_state.load(Ordering::Acquire) & CANCELED != 0;
            if canceled() {
                return;
            }
            let setup = prepare();
            let mut live = setup
                .as_ref()
                .map_err(String::clone)
                .and_then(|(bin, profile)| {
                    // Cancellation during profile preparation must prevent a late provider launch.
                    worker_state
                        .compare_exchange(0, LAUNCHING, Ordering::AcqRel, Ordering::Acquire)
                        .map_err(|_| "Folder choice canceled".to_string())?;
                    Provider::new(bin, profile, "app/cleaner.md", "cleaner").start()
                });
            while !canceled() {
                match receive.recv_timeout(Duration::from_millis(50)) {
                    Ok(request) => {
                        let answer = live
                            .as_mut()
                            .map_err(|e| e.clone())
                            .and_then(|live| {
                                live.exchange(request.body, request.deadline, canceled, answer)
                            })
                            .and_then(|text| serde_json::from_str(&text).map_err(|_| INVALID.into()))
                            .and_then(validate);
                        if request.reply.send(answer).is_err() {
                            eprintln!(
                                "[richos] RichCleaner folder answer no longer has a receiver"
                            );
                        }
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => break,
                }
            }
            drop(live);
            if let Ok((_, profile)) = setup {
                profile.remove_session_files();
            }
        });
        Self {
            started,
            send: Some(send),
            state,
            worker: Some(worker),
        }
    }
    pub fn choose(&self, name: &str, note: &str, folders: &FolderContext) -> Result<FilingChoice> {
        self.choose_from(json!({"filename":name,"note":note}), folders)
    }
    pub fn choose_batch(&self, names: &[String], note: &str, folders: &FolderContext) -> Result<FilingChoice> {
        self.choose_from(json!({"filenames":names,"note":note}), folders)
    }
    fn choose_from(&self, input: Value, folders: &FolderContext) -> Result<FilingChoice> {
        let (body, sent, complete) = request(input, folders)?;
        let deadline =
            (self.started + Duration::from_secs(30)).max(Instant::now() + Duration::from_secs(5));
        let (reply, receive) = mpsc::channel();
        self.send
            .as_ref()
            .ok_or("Folder choice is closed")?
            .send(Request {
                body,
                deadline,
                reply,
            })
            .map_err(|e| e.to_string())?;
        while self.state.load(Ordering::Acquire) & CANCELED == 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("Folder choice timed out".into());
            }
            match receive.recv_timeout(remaining.min(Duration::from_millis(50))) {
                Ok(answer) => return answer.map(|choice| selection(choice, &folders.labels[..sent], complete)),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err("Folder helper stopped".into()),
            }
        }
        Err("Folder choice canceled".into())
    }
    pub fn cancel(&self) {
        self.state.fetch_or(CANCELED, Ordering::AcqRel);
    }
}
impl Drop for Prepared {
    fn drop(&mut self) {
        self.cancel();
        drop(self.send.take());
        // A worker still preparing its profile can clean it up without blocking the
        // caller. Once launch has begun, keep the existing synchronous child reaping.
        if self.state.load(Ordering::Acquire) & LAUNCHING != 0
            && self
                .worker
                .take()
                .is_some_and(|worker| worker.join().is_err())
        {
            eprintln!("[richos] RichCleaner folder helper failed while stopping");
        }
    }
}
fn answer(frame: &Value) -> Result<String> {
    Ok(frame["result"]
        .as_str()
        .ok_or("No folder choice")?
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .into())
}
fn validate(choice: Choice) -> Result<Choice> {
    let parts = folder_parts(&choice.folder).map_err(|_| INVALID)?;
    if parts.join("/") != choice.folder
        || choice.keywords.len() + choice.aliases.len() > 20
        || choice
            .keywords
            .iter()
            .chain(&choice.aliases)
            .any(|v| v.len() > 160 || v.chars().any(char::is_control))
    {
        return Err(INVALID.into());
    }
    Ok(choice)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canceled_preparation_cleans_up_without_launching_a_provider() {
        let (root, bin, profile, _) = crate::request_classifier::tests::fixture("");
        let plugin = profile.plugin.clone();
        std::fs::create_dir(&plugin).unwrap();
        let provider_dir = profile.state.join("cleaner");
        let (entered, preparing) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let mut chooser = Prepared::start(move || {
            entered.send(()).unwrap();
            // load-bound: hang guard for the fixture barrier; the verdict uses filesystem state.
            released.recv_timeout(Duration::from_secs(30)).unwrap();
            Ok((bin, profile))
        });
        // load-bound: hang guard for the fixture barrier; the verdict uses filesystem state.
        preparing.recv_timeout(Duration::from_secs(30)).unwrap();
        chooser.cancel();
        release.send(()).unwrap();
        chooser.worker.take().unwrap().join().unwrap();
        assert!(!provider_dir.exists());
        assert!(!plugin.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn model_paths_are_data_and_invalid_paths_are_refused() {
        let parse = |folder: &str| {
            validate(Choice {
                folder: folder.into(),
                keywords: vec![],
                aliases: vec![],
                clear: true,
            })
        };
        assert!(parse("Home/Kitchen").is_ok());
        assert!(parse("/Desktop").is_err());
        assert!(parse("../../elsewhere").is_err());
        assert!(parse("Home//Kitchen").is_err());
    }

    #[test]
    fn wire_budget_preserves_recent_prefix_and_only_offers_what_was_sent() {
        let recent = "Company/Board".to_string();
        let mut labels = vec![recent.clone()];
        labels.extend((0..500).map(|n| format!("{} {n}", "\"📷".repeat(70))));
        let folders = FolderContext { labels, complete: true };
        let input = json!({"filenames":["a.pdf","b.pdf"],"note":"Quote \"café\""});
        let (body, sent, complete) = request(input, &folders).unwrap();
        assert!(body.len() <= MAX_REQUEST_BYTES);
        assert!(sent > 1 && sent < folders.labels.len() && !complete);
        let frame: Value = serde_json::from_str(&body).unwrap();
        let fields: Value = serde_json::from_str(frame["message"]["content"].as_str().unwrap()).unwrap();
        assert_eq!(fields["foldersComplete"], false);
        assert_eq!(fields["folders"], json!(folders.labels[..sent]));
        let selected = |folder: String, complete| selection(
            Choice { folder, keywords: vec![], aliases: vec![], clear: true },
            &folders.labels[..sent],
            complete,
        );
        assert!(matches!(selected(recent, false), FilingChoice::Category { existing_only: true, .. }));
        assert!(matches!(selected(folders.labels[sent].clone(), false), FilingChoice::Inbox { .. }));
        assert!(matches!(selected("New Category".into(), true), FilingChoice::Category { existing_only: false, .. }));
    }

    #[test]
    fn scan_incompleteness_reaches_provider_even_if_all_collected_labels_fit() {
        let folders = FolderContext { labels: vec!["Company".into()], complete: false };
        let (body, sent, complete) = request(json!({"filename":"a.pdf","note":"quote"}), &folders).unwrap();
        assert_eq!(sent, 1);
        assert!(!complete);
        let frame: Value = serde_json::from_str(&body).unwrap();
        let fields: Value = serde_json::from_str(frame["message"]["content"].as_str().unwrap()).unwrap();
        assert_eq!(fields["foldersComplete"], false);
    }
}
