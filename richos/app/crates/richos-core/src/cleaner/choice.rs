//! One tool-free folder choice. Startup overlaps speech; one request follows durable note save.
use super::{folder_parts, Result};
use crate::{engine_profile::EngineProfile, request_classifier::Provider};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
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
struct Request {
    body: String,
    deadline: Instant,
    reply: mpsc::Sender<Result<Choice>>,
}
pub struct Prepared {
    started: Instant,
    send: Option<mpsc::Sender<Request>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Prepared {
    pub fn start(bin: &Path, profile: &EngineProfile) -> Self {
        let started = Instant::now();
        let stop = Arc::new(AtomicBool::new(false));
        let canceled = stop.clone();
        let (send, receive) = mpsc::channel::<Request>();
        let profile = profile.clone();
        let bin = bin.to_path_buf();
        let worker = std::thread::spawn(move || {
            let mut live = Provider::new(&bin, &profile, "app/cleaner.md", "cleaner").start();
            loop {
                if canceled.load(Ordering::Acquire) {
                    break;
                }
                match receive.recv_timeout(Duration::from_millis(50)) {
                    Ok(request) => {
                        let answer = live
                            .as_mut()
                            .map_err(|e| e.clone())
                            .and_then(|live| {
                                live.exchange(
                                    request.body,
                                    request.deadline,
                                    || canceled.load(Ordering::Acquire),
                                    answer,
                                )
                            })
                            .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
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
            profile.remove_session_files();
        });
        Self {
            started,
            send: Some(send),
            stop,
            worker: Some(worker),
        }
    }
    pub fn choose(&self, name: &str, note: &str, folders: &[String]) -> Result<Choice> {
        self.choose_from(json!({"filename":name,"note":note,"folders":folders}))
    }
    pub fn choose_batch(&self, names: &[String], note: &str, folders: &[String]) -> Result<Choice> {
        self.choose_from(json!({"filenames":names,"note":note,"folders":folders}))
    }
    fn choose_from(&self, input: Value) -> Result<Choice> {
        let body = json!({"type":"user","message":{"role":"user","content":input.to_string()}})
            .to_string();
        if body.len() > 60 * 1024 {
            return Err("Folder context exceeds its metadata budget".into());
        }
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
        while !self.stop.load(Ordering::Acquire) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("Folder choice timed out".into());
            }
            match receive.recv_timeout(remaining.min(Duration::from_millis(50))) {
                Ok(answer) => return answer,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err("Folder helper stopped".into()),
            }
        }
        Err("Folder choice canceled".into())
    }
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Release);
    }
}
impl Drop for Prepared {
    fn drop(&mut self) {
        self.cancel();
        drop(self.send.take());
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                eprintln!("[richos] RichCleaner folder helper failed while stopping");
            }
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
    folder_parts(&choice.folder)?;
    if choice.keywords.len() + choice.aliases.len() > 20
        || choice
            .keywords
            .iter()
            .chain(&choice.aliases)
            .any(|v| v.len() > 160 || v.chars().any(char::is_control))
    {
        return Err("Invalid search aliases".into());
    }
    Ok(choice)
}
#[cfg(test)]
mod tests {
    use super::*;
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
    }
}
