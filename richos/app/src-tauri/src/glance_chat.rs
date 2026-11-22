//! Bridge from the native RichGlance bundle to the existing Mac composer.
//! The socket and draft bytes stay under the user's account. Only a small
//! one-use request ID crosses into the webview.
use crate::{mac_attachments, AppState};
use base64::Engine as _;
use richos_core::read_view::SpineView;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{mpsc, Mutex};
use tauri::{AppHandle, Manager, State};

const MISSING: &str = "This screenshot is no longer waiting to be attached.";

#[derive(Default)]
pub struct Host {
    pending: Mutex<HashMap<String, Pending>>,
}

struct Pending {
    thread: Option<String>,
    png: Vec<u8>,
    preview: Vec<u8>,
    answer: mpsc::SyncSender<bool>,
}

#[derive(Serialize)]
pub struct StagedCapture {
    #[serde(flatten)]
    attached: mac_attachments::AttachedView,
    preview: String,
}

#[richos_command::command]
pub fn glance_stage_attachment(
    host: State<'_, Host>,
    app: State<'_, AppState>,
    desk: State<'_, mac_attachments::MacAttachments>,
    request_id: String,
    chat_key: String,
    draft_id: String,
    attachment_id: String,
) -> Result<StagedCapture, String> {
    let ticket = host
        .pending
        .lock()
        .unwrap()
        .remove(&request_id)
        .ok_or_else(|| MISSING.to_string())?;
    let valid = match &ticket.thread {
        Some(thread) => {
            chat_key == *thread
                && app
                    .reader
                    .snapshot()
                    .threads()
                    .iter()
                    .any(|row| row.id == *thread && row.entity_id.is_some())
        }
        None => chat_key.starts_with("entity:"),
    };
    let answer = if valid {
        desk.attach_bytes(
            &draft_id,
            &attachment_id,
            "RichGlance Capture.png",
            "image/png",
            &ticket.png,
        )
    } else {
        Err("The selected chat could not receive this screenshot.".into())
    };
    let _result = ticket.answer.send(answer.is_ok());
    answer.map(|attached| StagedCapture {
        attached,
        preview: base64::engine::general_purpose::STANDARD.encode(&ticket.preview),
    })
}

#[richos_command::command]
pub fn glance_cancel_attachment(host: State<'_, Host>, request_id: String) {
    if let Some(ticket) = host.pending.lock().unwrap().remove(&request_id) {
        let _result = ticket.answer.send(false);
    }
}

pub fn start(app: AppHandle) {
    #[cfg(target_os = "macos")]
    native::start(app);
    #[cfg(not(target_os = "macos"))]
    drop(app);
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use crate::phone::attachments::MAX_FILE_BYTES;
    use ring::rand::{SecureRandom, SystemRandom};
    use serde::{Deserialize, Serialize};
    use serde_json::{json, Value};
    use std::fs;
    use std::io::{self, BufRead, BufReader, Read, Write};
    use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tauri::Emitter;

    const DRAFT_EVENT: &str = "rich://glance-draft";
    const MAX_HEADER: u64 = 4_096;

    #[derive(Deserialize)]
    #[serde(tag = "action", rename_all = "snake_case")]
    enum Command {
        List,
        Attach {
            length: usize,
            #[serde(default)]
            preview_length: usize,
            thread: Option<String>,
        },
    }

    #[derive(Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Draft {
        request_id: String,
        thread: Option<String>,
    }

    pub fn start(app: AppHandle) {
        std::thread::spawn(move || {
            if let Err(error) = serve(app) {
                eprintln!(
                    "[richos] RichGlance draft handoff unavailable ({:?})",
                    error.kind()
                );
            }
        });
    }

    fn serve(app: AppHandle) -> io::Result<()> {
        let home = app.path().home_dir().map_err(io::Error::other)?;
        let root = home.join("Library/Application Support/com.richos.app/Glance Chat");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&root)?;
        let info = fs::symlink_metadata(&root)?;
        // The path is used by the companion as well, before any image bytes are written.
        if !info.is_dir() || info.uid() != unsafe { libc::geteuid() } || info.mode() & 0o077 != 0 {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        let endpoint = root.join(format!("{}.sock", std::process::id()));
        if fs::symlink_metadata(&endpoint).is_ok_and(|info| info.file_type().is_socket()) {
            fs::remove_file(&endpoint)?;
        }
        let listener = UnixListener::bind(&endpoint)?;
        let endpoint = Endpoint(endpoint);
        fs::set_permissions(&endpoint.0, fs::Permissions::from_mode(0o600))?;
        let active = Arc::new(AtomicUsize::new(0));
        for stream in listener.incoming() {
            let stream = stream?;
            if active.fetch_add(1, Ordering::Relaxed) >= 4 {
                active.fetch_sub(1, Ordering::Relaxed);
                continue;
            }
            let app = app.clone();
            let active = active.clone();
            std::thread::spawn(move || {
                let _permit = Permit(active);
                let mut stream = stream;
                let _result = stream.set_read_timeout(Some(Duration::from_secs(10)));
                let _result = stream.set_write_timeout(Some(Duration::from_secs(5)));
                let reply = answer(&app, &stream).unwrap_or_else(
                    |_| json!({"error": "RichOS could not receive this screenshot."}),
                );
                if let Ok(mut data) = serde_json::to_vec(&reply) {
                    data.push(b'\n');
                    let _result = stream.write_all(&data);
                }
            });
        }
        Ok(())
    }

    struct Endpoint(PathBuf);
    impl Drop for Endpoint {
        fn drop(&mut self) {
            let _result = fs::remove_file(&self.0);
        }
    }

    struct Permit(Arc<AtomicUsize>);
    impl Drop for Permit {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::Relaxed);
        }
    }

    fn answer(app: &AppHandle, stream: &UnixStream) -> io::Result<Value> {
        let mut stream = BufReader::new(stream);
        let command = header(&mut stream)?;
        let state = app.state::<AppState>();
        let active = state.reader.snapshot().active_thread().map(str::to_string);
        let mut threads = state.reader.snapshot().threads();
        threads.retain(|thread| thread.entity_id.is_some());
        match command {
            Command::List => {
                threads.sort_by_key(|thread| std::cmp::Reverse(thread.last_activity));
                let choices: Vec<_> = threads.iter().take(40).map(|thread| {
                    json!({"id": thread.id, "title": thread.title.chars().take(160).collect::<String>(),
                           "current": active.as_deref() == Some(&thread.id)})
                }).collect();
                Ok(json!({"chats": choices}))
            }
            Command::Attach {
                length,
                preview_length,
                thread,
            } => {
                if !(8..=MAX_FILE_BYTES).contains(&length) || preview_length > 65_536 {
                    return Ok(
                        json!({"error": "This screenshot is too large to attach to a message."}),
                    );
                }
                if thread
                    .as_ref()
                    .is_some_and(|id| !threads.iter().any(|row| &row.id == id))
                {
                    return Ok(json!({"error": "That chat is unavailable. Choose another chat."}));
                }
                let mut png = vec![0; length];
                stream.read_exact(&mut png)?;
                if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
                    return Err(io::Error::from(io::ErrorKind::InvalidData));
                }
                let mut preview = vec![0; preview_length];
                stream.read_exact(&mut preview)?;
                if !preview.is_empty() && !preview.starts_with(b"\x89PNG\r\n\x1a\n") {
                    return Err(io::Error::from(io::ErrorKind::InvalidData));
                }
                Ok(offer(app, thread, png, preview))
            }
        }
    }

    fn header(stream: &mut impl BufRead) -> io::Result<Command> {
        let mut bytes = Vec::new();
        stream.take(MAX_HEADER + 1).read_until(b'\n', &mut bytes)?;
        if bytes.len() as u64 > MAX_HEADER || bytes.last() != Some(&b'\n') {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        serde_json::from_slice(&bytes).map_err(io::Error::other)
    }

    fn offer(app: &AppHandle, thread: Option<String>, png: Vec<u8>, preview: Vec<u8>) -> Value {
        let Some(window) = crate::conversation_window(app) else {
            return json!({"error": "There is no chat to attach to. Open RichOS and try again."});
        };
        let mut id = [0u8; 16];
        if SystemRandom::new().fill(&mut id).is_err() {
            return json!({"error": "RichOS could not prepare this draft."});
        }
        let request_id: String = id.iter().map(|byte| format!("{byte:02x}")).collect();
        let (answer, reply) = mpsc::sync_channel(1);
        let host = app.state::<Host>();
        host.pending.lock().unwrap().insert(
            request_id.clone(),
            Pending {
                thread: thread.clone(),
                png,
                preview,
                answer,
            },
        );
        let _result = window.show();
        let _result = window.unminimize();
        let _result = window.set_focus();
        let sent = window
            .emit_to(
                window.label(),
                DRAFT_EVENT,
                Draft {
                    request_id: request_id.clone(),
                    thread,
                },
            )
            .is_ok();
        let result = if sent {
            reply.recv_timeout(Duration::from_secs(55)).ok()
        } else {
            None
        };
        host.pending.lock().unwrap().remove(&request_id);
        match result {
            Some(true) => json!({"attached": true}),
            Some(false) => json!({"error": "The screenshot was not added to a draft."}),
            None => json!({"error": "RichOS did not finish opening the draft. Try again."}),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn bounds_header_and_leaves_the_png_unread() {
            let mut wire = BufReader::new(
                &b"{\"action\":\"attach\",\"length\":8,\"thread\":null}\n\x89PNG\r\n\x1a\n"[..],
            );
            assert!(matches!(
                header(&mut wire).unwrap(),
                Command::Attach {
                    length: 8,
                    preview_length: 0,
                    thread: None
                }
            ));
            let mut bytes = Vec::new();
            wire.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, b"\x89PNG\r\n\x1a\n");
            assert!(header(&mut BufReader::new(vec![b'x'; 4097].as_slice())).is_err());
        }
    }
}
