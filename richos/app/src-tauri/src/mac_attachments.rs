//! **SCREENSHOTS AND FILES INTO THE MAC COMPOSER** — CEO ruling §86 (2026-09-24, "his
//! daily-driver app runs his existing team"), and the one thing the terminal gives him that the
//! Mac window did not: `richos-hq/docs/plans/2026-09-24-daily-driver-readiness.md` §4 X3
//! ("Screenshots and files he hands over") and §6 step 3 ("drop and paste onto the composer,
//! stored and described exactly as the phone's are").
//!
//! # One storage path, not two
//!
//! Everything that decides what a file IS — the accepted types, the byte sniffing, the 25 MiB
//! ceiling, ten files and 100 MiB per message, the name sanitizing, the private modes, the
//! atomic writes and the block Rich reads — is `phone/attachments.rs`, called unchanged. This
//! file only answers the two questions the phone never had: how bytes get from the window to
//! that desk, and what the window may name.
//!
//! * **Staged** under device `mac` ([`MAC_DEVICE`]), with the composer's draft id as the
//!   message id. A phone's device id is always `dev_…` (`phone/device.rs`), so the folders
//!   can never meet.
//! * **Committed** into `<app data>/attachments/<conversation>/<draft id>/` at Send, and
//!   described by [`attachments::describe_from`] with [`Origin::Mac`]. The words Rich reads
//!   are the phone's shape, headed `Attached on this Mac (…)`.
//!
//! # How bytes reach it
//!
//! * **A drop never hands the window a path it can choose.** Tauri's drag-and-drop handler is
//!   on for the window (the default), so the webview never receives the files; the WINDOW does,
//!   as paths. [`listen_for_drops`] records each drop's paths on a shelf here and tells the page
//!   only a drop number and each file's display name (`rich://file-drop`). The page asks for
//!   `(drop, index)`, and this file reads the path it recorded. A page that asks for a path
//!   the OS never dropped gets nothing: there is no command that takes a path.
//! * **A paste** (a screenshot on the clipboard, an image copied from Preview) arrives in the
//!   page as a `File`, and its bytes cross the IPC once, as a raw body
//!   ([`attach_pasted_file`]), never base64 in JSON.
//! * **A file the thread produced** (*Add to chat* in the Output panel, richos-hq
//!   `docs/prds/2026-10-05-output-side-panel.md` §5.4 and slice S7, §12.7). The page names an
//!   OUTPUT ID, never a path: [`output_attach`] resolves it against the active thread's record
//!   through `output_files.rs` (§5.1), runs its checks (§5.2: no link, no swapped file, a regular
//!   file), and reads the canonical path with `O_NOFOLLOW`. From there it is the drop's road
//!   exactly — the same kind by name, the same size refusal before the read, the same
//!   [`AttachmentDesk::stage_from`] and the same sentences. There is no second desk.
//!
//! # What this does NOT do, said rather than left to be discovered
//!
//! * **No file picker.** The brief is drop and paste; a picker needs a dialog plugin this
//!   build does not carry.
//! * **No thumbnails from disk.** A pasted image is previewed from the bytes the page already
//!   holds; a dropped one is shown as a named chip, because reading it back to the page would
//!   be a second copy of up to 25 MiB for a picture.
//! * Thread deletion does not reach these files, for the reason the phone's module gives.

use crate::output_files::{self, OutputFiles};
use crate::phone::attachments::{self, AttachmentDesk, Origin, Upload};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager, State};

/// The staging "device" for the Mac's own composer. See the module doc.
pub const MAC_DEVICE: &str = "mac";

/// While a drag is over the window, and when it leaves or lands: `{ "phase": "enter" |
/// "leave", "count": n }`. The page uses it only to show where a drop will go.
pub const EVENT_FILE_DRAG: &str = "rich://file-drag";
/// A drop landed: `{ "drop": n, "files": [{ "index": i, "name": "…" }] }`.
pub const EVENT_FILE_DROP: &str = "rich://file-drop";

/// A drop is kept long enough for the page to ask for every file in it, one after another,
/// and not for ever. 25 MiB off an SSD is milliseconds; ten of them is well under a second.
/// Ten minutes is far past that and still bounded.
const DROP_TTL: Duration = Duration::from_secs(10 * 60);
/// At most this many drops are remembered; the oldest is forgotten first.
const DROPS_KEPT: usize = 16;

struct Dropped {
    id: u64,
    at: Instant,
    /// `None` once the page has taken that file: a drop is read once.
    paths: Vec<Option<PathBuf>>,
}

#[derive(Default)]
struct DropShelf {
    next: u64,
    drops: VecDeque<Dropped>,
}

pub struct MacAttachments {
    desk: AttachmentDesk,
    shelf: Mutex<DropShelf>,
}

/// One file as the composer shows it before Send.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttachedView {
    pub id: String,
    pub name: String,
    pub media_type: String,
    /// "PDF", "PNG image": the accepted kind's own label, so the chip and the refusals use
    /// one vocabulary.
    pub label: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct DroppedFile {
    pub index: usize,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct DropView {
    pub drop: u64,
    pub files: Vec<DroppedFile>,
}

#[derive(Clone, Debug, Serialize)]
struct DragView {
    phase: &'static str,
    count: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Wanted {
    pub id: String,
    pub sha256: String,
}

/// What Send gets back. `text` is what goes to Rich; `missing` names files this Mac no longer
/// holds (the seven-day sweep, or room made for newer files), and when it is not empty
/// NOTHING was committed and `text` is `None`.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Committed {
    pub text: Option<String>,
    pub files: Vec<AttachedView>,
    pub missing: Vec<String>,
}

const NOT_A_FILE: &str = "This is a folder, not a file. Drop the files inside it instead. Nothing was attached.";
const UNREADABLE: &str = "RichOS couldn't read this file. Nothing was attached.";
const DROP_GONE: &str = "That drop is no longer available. Drop the file again.";
const NOT_SAVED: &str = "RichOS couldn't save this file just now. Nothing was attached; try again.";

fn label_of(media_type: &str) -> String {
    attachments::kind_of(media_type).map(|k| k.label.to_string()).unwrap_or_default()
}

/// The last path component, for display only. The stored name is `sanitize_name`'s.
fn display_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A recorded file's bytes, never through a link at the last component, whatever happened
/// since the §5.2 checks (Output side panel PRD §5.2 step 5). At most one byte over the desk's
/// ceiling is read, which is enough to refuse a file that grew.
fn read_nofollow(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(path)?;
    let mut out = Vec::new();
    file.take(attachments::MAX_FILE_BYTES as u64 + 1).read_to_end(&mut out)?;
    Ok(out)
}

/// `%XX` decoding for the one header that carries a name. Refuses a malformed escape and
/// anything that does not decode to UTF-8, rather than guessing.
fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = value.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

impl MacAttachments {
    /// No I/O, like the desk it wraps.
    pub fn open(data_dir: &Path) -> Self {
        MacAttachments { desk: AttachmentDesk::open(data_dir), shelf: Mutex::new(DropShelf::default()) }
    }

    /// Remember a drop's paths and say what the page may know about them.
    pub fn record_drop(&self, paths: Vec<PathBuf>) -> DropView {
        let mut shelf = self.shelf.lock().unwrap();
        shelf.drops.retain(|d| d.at.elapsed() < DROP_TTL);
        while shelf.drops.len() >= DROPS_KEPT {
            shelf.drops.pop_front();
        }
        shelf.next += 1;
        let id = shelf.next;
        let files = paths.iter().enumerate().map(|(index, p)| DroppedFile { index, name: display_name(p) }).collect();
        shelf.drops.push_back(Dropped { id, at: Instant::now(), paths: paths.into_iter().map(Some).collect() });
        DropView { drop: id, files }
    }

    /// The path the OS dropped at `(drop, index)`, once.
    fn take_dropped(&self, drop: u64, index: usize) -> Option<PathBuf> {
        let mut shelf = self.shelf.lock().unwrap();
        let found = shelf.drops.iter_mut().find(|d| d.id == drop && d.at.elapsed() < DROP_TTL)?;
        found.paths.get_mut(index)?.take()
    }

    fn answer(&self, upload: Upload) -> Result<AttachedView, String> {
        match upload {
            Upload::Stored(s) | Upload::Duplicate(s) => Ok(AttachedView {
                label: label_of(&s.media_type),
                id: s.id,
                name: s.name,
                media_type: s.media_type,
                size: s.size,
                sha256: s.sha256,
            }),
            Upload::Refused(sentence) | Upload::Limit(sentence) => Err(sentence),
            // The page mints a fresh id for every file, so this is not a state it can reach;
            // it is answered as a save failure rather than trusted.
            Upload::Conflict => Err(NOT_SAVED.into()),
        }
    }

    fn check_ids(draft: &str, id: &str) -> Result<(), String> {
        if attachments::valid_id(draft) && attachments::valid_id(id) {
            Ok(())
        } else {
            Err(NOT_SAVED.into())
        }
    }

    /// **Attach the file at a path the OS dropped.** Typed by its name, checked by its bytes.
    pub fn attach_path(&self, draft: &str, id: &str, path: &Path) -> Result<AttachedView, String> {
        Self::check_ids(draft, id)?;
        let meta = std::fs::metadata(path).map_err(|_| UNREADABLE.to_string())?;
        if meta.is_dir() {
            return Err(NOT_A_FILE.into());
        }
        self.stage_file(draft, id, &display_name(path), meta.len(), || std::fs::read(path))
    }

    /// **The one road from a file on this Mac to the desk**, for a drop and for a recorded file
    /// alike: the kind from the name, the size refused BEFORE the read, then the desk's own
    /// `stage_from`. `len` is what `stat` said; `read` reads the bytes.
    fn stage_file(
        &self,
        draft: &str,
        id: &str,
        name: &str,
        len: u64,
        read: impl FnOnce() -> std::io::Result<Vec<u8>>,
    ) -> Result<AttachedView, String> {
        let Some(kind) = attachments::kind_for_name(name) else {
            return Err(Origin::Mac.unknown_type());
        };
        // Refused BEFORE it is read: a 4 GB video dropped by mistake must not be pulled into
        // memory just to be told it is too big.
        if len > attachments::MAX_FILE_BYTES as u64 {
            return Err(Origin::Mac.too_large());
        }
        let bytes = read().map_err(|_| UNREADABLE.to_string())?;
        // A file that grew between the `stat` and the read is held to the same ceiling.
        if bytes.len() > attachments::MAX_FILE_BYTES {
            return Err(Origin::Mac.too_large());
        }
        let upload = self
            .desk
            .stage_from(Origin::Mac, MAC_DEVICE, draft, id, Some(name), kind.media_type, &bytes)
            .map_err(|e| {
                eprintln!("[richos] a file for the composer could not be staged: {e}");
                NOT_SAVED.to_string()
            })?;
        self.answer(upload)
    }

    /// **Attach a file this thread produced, by its output id** (Output side panel PRD §5.4,
    /// slice S7). The id is resolved against the ACTIVE thread's record by `output_files.rs`
    /// (§5.1: an id it does not hold is *"I don't have that file in this thread's output."*),
    /// the §5.2 checks run (a link, a swapped file, a folder or a missing file is refused with
    /// that file's own sentence), and the canonical path is read with `O_NOFOLLOW`. Everything
    /// after that is [`Self::stage_file`], the drop's road: the desk decides, in its words.
    pub fn attach_recorded(&self, files: &OutputFiles, draft: &str, id: &str, output_id: &str) -> Result<AttachedView, String> {
        Self::check_ids(draft, id)?;
        let located = files.locate(output_id)?;
        let checked = output_files::check(&located).map_err(|problem| problem.sentence())?;
        // The name he sees in the panel is the recorded path's last component; the canonical
        // path differs from it only by the folders above it.
        let name = display_name(&located.path);
        let path = checked.path.clone();
        self.stage_file(draft, id, &name, checked.bytes, move || read_nofollow(&path))
    }

    /// **Attach bytes the page already holds** (a paste). The declared type is used when it is
    /// one this Mac takes; otherwise the name's extension chooses; otherwise it is refused by
    /// the desk with the unknown-type sentence.
    pub fn attach_bytes(&self, draft: &str, id: &str, name: &str, declared: &str, bytes: &[u8]) -> Result<AttachedView, String> {
        Self::check_ids(draft, id)?;
        let media_type = attachments::kind_of(declared)
            .or_else(|| attachments::kind_for_name(name))
            .map(|k| k.media_type)
            .unwrap_or(declared);
        let upload = self
            .desk
            .stage_from(Origin::Mac, MAC_DEVICE, draft, id, Some(name), media_type, bytes)
            .map_err(|e| {
                eprintln!("[richos] a file pasted into the composer could not be staged: {e}");
                NOT_SAVED.to_string()
            })?;
        self.answer(upload)
    }

    pub fn discard(&self, draft: &str, id: &str) -> Result<bool, String> {
        if !attachments::valid_id(draft) {
            return Ok(false);
        }
        self.desk.discard(MAC_DEVICE, draft, id).map_err(|e| {
            eprintln!("[richos] a removed attachment could not be deleted: {e}");
            "RichOS couldn't remove this file just now.".to_string()
        })
    }

    /// **Send's half.** Every named file must be here with the bytes it was attached with;
    /// then they move into the conversation's folder and the words Rich reads are built.
    pub fn commit(&self, thread: &str, draft: &str, text: &str, wanted: &[Wanted]) -> Result<Committed, String> {
        if thread.is_empty() || !attachments::valid_id(draft) || wanted.is_empty() || wanted.len() > attachments::MAX_FILES_PER_MESSAGE {
            return Err(NOT_SAVED.into());
        }
        let pairs: Vec<(String, String)> = wanted.iter().map(|w| (w.id.clone(), w.sha256.clone())).collect();
        let files = match self.desk.staged(MAC_DEVICE, draft, &pairs) {
            Ok(files) => files,
            Err(attachments::Missing(missing)) => return Ok(Committed { text: None, files: Vec::new(), missing }),
        };
        let stored = self.desk.commit(MAC_DEVICE, draft, thread, &files).map_err(|e| {
            eprintln!("[richos] attached files could not be moved into the conversation: {e}");
            "RichOS couldn't save your files into this conversation just now. They are still attached; try Send again.".to_string()
        })?;
        let described = attachments::describe_from(Origin::Mac, text.trim(), &stored);
        let files = stored
            .iter()
            .zip(files.iter())
            .map(|(s, staged)| AttachedView {
                id: s.id.clone(),
                name: s.name.clone(),
                media_type: s.media_type.clone(),
                label: label_of(&s.media_type),
                size: s.size,
                sha256: staged.sha256.clone(),
            })
            .collect();
        Ok(Committed { text: Some(described), files, missing: Vec::new() })
    }
}

/// Emit to the window, and say so in the log when it could not be delivered: a drop that
/// never reached the page is otherwise a file he dropped and nothing happened.
fn tell<S: Serialize + Clone>(app: &tauri::AppHandle, event: &str, payload: S) {
    if let Err(e) = app.emit(event, payload) {
        eprintln!("[richos] {event} could not reach the window: {e}");
    }
}

/// **The drop listener, one per window.** Installed beside `remember_window_geometry` at both
/// places a main window is built, so a window re-opened from the Dock takes drops too.
pub fn listen_for_drops(window: &tauri::WebviewWindow) {
    let app = window.app_handle().clone();
    window.on_window_event(move |event| {
        let tauri::WindowEvent::DragDrop(drag) = event else { return };
        match drag {
            tauri::DragDropEvent::Enter { paths, .. } => {
                tell(&app, EVENT_FILE_DRAG, DragView { phase: "enter", count: paths.len() });
            }
            tauri::DragDropEvent::Leave => {
                tell(&app, EVENT_FILE_DRAG, DragView { phase: "leave", count: 0 });
            }
            tauri::DragDropEvent::Drop { paths, .. } => {
                tell(&app, EVENT_FILE_DRAG, DragView { phase: "leave", count: 0 });
                if paths.is_empty() {
                    return;
                }
                let Some(state) = app.try_state::<MacAttachments>() else { return };
                let view = state.record_drop(paths.clone());
                tell(&app, EVENT_FILE_DROP, view);
            }
            _ => {}
        }
    });
}

// ---- the commands ----------------------------------------------------------------------

#[richos_command::command]
pub fn attach_dropped_file(
    state: State<'_, MacAttachments>,
    draft_id: String,
    attachment_id: String,
    drop: u64,
    index: usize,
) -> Result<AttachedView, String> {
    let Some(path) = state.take_dropped(drop, index) else { return Err(DROP_GONE.into()) };
    state.attach_path(&draft_id, &attachment_id, &path)
}

/// The body is the file's raw bytes. Headers: `x-richos-draft`, `x-richos-attachment`,
/// `x-richos-name` (percent-encoded) and `x-richos-type`.
#[richos_command::command]
pub fn attach_pasted_file(state: State<'_, MacAttachments>, request: tauri::ipc::Request<'_>) -> Result<AttachedView, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err(NOT_SAVED.into()) };
    let header = |name: &str| request.headers().get(name).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let name = percent_decode(&header("x-richos-name")).unwrap_or_default();
    if name.len() > attachments::MAX_RAW_NAME_BYTES {
        return Err(NOT_SAVED.into());
    }
    state.attach_bytes(&header("x-richos-draft"), &header("x-richos-attachment"), &name, &header("x-richos-type"), bytes)
}

/// *Add to chat* from the Output panel (Output side panel PRD §5.4, slice S7). The page names
/// its draft, the attachment id it minted for the chip (as for a drop or a paste) and an
/// output id — never a path.
#[richos_command::command]
pub fn output_attach(
    state: State<'_, MacAttachments>,
    files: State<'_, OutputFiles>,
    draft_id: String,
    attachment_id: String,
    output_id: String,
) -> Result<AttachedView, String> {
    state.attach_recorded(&files, &draft_id, &attachment_id, &output_id)
}

#[richos_command::command]
pub fn discard_attachment(state: State<'_, MacAttachments>, draft_id: String, attachment_id: String) -> Result<bool, String> {
    state.discard(&draft_id, &attachment_id)
}

#[richos_command::command]
pub fn commit_attachments(
    state: State<'_, MacAttachments>,
    thread_id: String,
    draft_id: String,
    text: String,
    attachments: Vec<Wanted>,
) -> Result<Committed, String> {
    state.commit(&thread_id, &draft_id, &text, &attachments)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!("mac-attachments-{tag}-{}-{}", std::process::id(), crate::phone::hex(&crate::phone::random_bytes(6).unwrap())));
            std::fs::create_dir_all(&p).unwrap();
            Scratch(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            if let Err(e) = std::fs::remove_dir_all(&self.0) {
                eprintln!("test scratch {} was not removed: {e}", self.0.display());
            }
        }
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    const PDF: &[u8] = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n1 0 obj\n";

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn a_png_and_a_pdf_are_attached_committed_and_described_the_way_the_phone_does_it() {
        let data = Scratch::new("data");
        let desktop = Scratch::new("desktop");
        let mac = MacAttachments::open(&data.0);
        let shot = write(&desktop.0, "Screenshot 2026-09-24 at 10.02.11.png", PNG);
        let brief = write(&desktop.0, "brief.pdf", PDF);

        let drop = mac.record_drop(vec![shot.clone(), brief.clone()]);
        assert_eq!(drop.files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Screenshot 2026-09-24 at 10.02.11.png", "brief.pdf"]);
        let a = mac.attach_path("draft-1", "a1", &mac.take_dropped(drop.drop, 0).unwrap()).unwrap();
        let b = mac.attach_bytes("draft-1", "b1", "brief.pdf", "application/pdf", PDF).unwrap();
        assert_eq!((a.media_type.as_str(), a.label.as_str(), a.size), ("image/png", "PNG image", PNG.len() as u64));
        assert_eq!((b.media_type.as_str(), b.label.as_str()), ("application/pdf", "PDF"));

        let wanted = vec![Wanted { id: a.id.clone(), sha256: a.sha256.clone() }, Wanted { id: b.id.clone(), sha256: b.sha256.clone() }];
        let done = mac.commit("thr_5c1e", "draft-1", "  What do you make of these?  ", &wanted).unwrap();
        assert!(done.missing.is_empty());
        let text = done.text.unwrap();
        let folder = data.0.join("attachments").join("thr_5c1e").join("draft-1");
        assert_eq!(
            text,
            format!(
                "What do you make of these?\n\nAttached on this Mac (2 files, saved by RichOS):\n\
                 - {} (image/png, {} bytes)\n\
                 - {} (application/pdf, {} bytes)",
                folder.join("Screenshot 2026-09-24 at 10.02.11.png").display(), PNG.len(),
                folder.join("brief.pdf").display(), PDF.len()
            )
        );
        // The folder the desk wrote is the folder the answering session is given to read
        // (`EngineProfile::attachments_folder`): one name, from one rule.
        assert_eq!(folder.parent().unwrap(), richos_core::attachments::conversation_folder(&data.0, "thr_5c1e"));
        // The bytes Rich will open are the bytes that were dropped, in the phone's folder shape.
        assert_eq!(std::fs::read(folder.join("brief.pdf")).unwrap(), PDF);
        assert_eq!(std::fs::read(folder.join("Screenshot 2026-09-24 at 10.02.11.png")).unwrap(), PNG);
        // The originals were read, never moved.
        assert!(shot.exists() && brief.exists());
        // Staging is empty again.
        assert!(!data.0.join("phone").join("attachment-staging").join("mac").join("draft-1").exists());
    }

    #[test]
    fn the_desk_and_the_session_name_every_conversation_folder_the_same_way() {
        // The desk writes `<data>/attachments/<segment(thread)>`; the answering session is given
        // `richos_core::attachments::conversation_folder` to read. Two copies of one rule (the
        // desk's is compiled into the conformance verifier without richos_core), so they are
        // held equal here, on safe ids, hostile ids and the pinned SHA-256 vector.
        for id in ["thr_6ac252bf8292433c918493055f4167d0", "0f8e1c2a-9b1d-4c3e-8a7f-1234567890ab", "..", "../x", "a/b", "", ".", "x.y", &"a".repeat(64), &"a".repeat(65), "Grüße"] {
            assert_eq!(attachments::segment(id), richos_core::attachments::segment(id), "{id:?}");
        }
        assert_eq!(attachments::segment("a/b"), "x.c14cddc033f64b9dea80ea675cf280a0");
        let data = Path::new("/data");
        assert_eq!(
            richos_core::attachments::conversation_folder(data, "thr_1"),
            data.join("attachments").join(attachments::segment("thr_1"))
        );
    }

    #[test]
    fn a_drop_is_read_once_and_the_page_can_never_name_a_path() {
        let data = Scratch::new("data");
        let desktop = Scratch::new("desktop");
        let mac = MacAttachments::open(&data.0);
        let file = write(&desktop.0, "a.png", PNG);
        let drop = mac.record_drop(vec![file]);
        assert!(mac.take_dropped(drop.drop, 0).is_some());
        assert!(mac.take_dropped(drop.drop, 0).is_none(), "a dropped path was handed out twice");
        assert!(mac.take_dropped(drop.drop, 1).is_none());
        assert!(mac.take_dropped(drop.drop + 1, 0).is_none());
        // Only DROPS_KEPT drops are remembered; the oldest goes first.
        let first = mac.record_drop(vec![desktop.0.join("x.png")]);
        for _ in 0..DROPS_KEPT {
            mac.record_drop(vec![desktop.0.join("y.png")]);
        }
        assert!(mac.take_dropped(first.drop, 0).is_none());
    }

    #[test]
    fn refusals_are_sentences_and_nothing_refused_is_staged() {
        let data = Scratch::new("data");
        let desktop = Scratch::new("desktop");
        let mac = MacAttachments::open(&data.0);
        let zip = write(&desktop.0, "archive.zip", b"PK\x03\x04");
        let fake = write(&desktop.0, "not-really.pdf", PNG);
        let empty = write(&desktop.0, "empty.txt", b"");
        std::fs::create_dir(desktop.0.join("Folder.pdf")).unwrap();
        assert_eq!(mac.attach_path("d", "z", &zip).unwrap_err(), Origin::Mac.unknown_type());
        assert_eq!(mac.attach_path("d", "f", &fake).unwrap_err(), "This file does not look like a PDF. Nothing was attached.");
        assert_eq!(mac.attach_path("d", "e", &empty).unwrap_err(), "This file is empty. Nothing was attached.");
        assert_eq!(mac.attach_path("d", "g", &desktop.0.join("Folder.pdf")).unwrap_err(), NOT_A_FILE);
        assert_eq!(mac.attach_path("d", "m", &desktop.0.join("missing.pdf")).unwrap_err(), UNREADABLE);
        // A file over the ceiling is refused from its size, before it is read.
        let big = desktop.0.join("big.pdf");
        let handle = std::fs::File::create(&big).unwrap();
        handle.set_len(attachments::MAX_FILE_BYTES as u64 + 1).unwrap();
        assert_eq!(mac.attach_path("d", "b", &big).unwrap_err(), Origin::Mac.too_large());
        // Hostile ids never reach the file system.
        assert!(mac.attach_bytes("../d", "a", "a.png", "image/png", PNG).is_err());
        assert!(mac.attach_bytes("d", "a/b", "a.png", "image/png", PNG).is_err());
        assert!(!data.0.join("phone").join("attachment-staging").join("mac").join("d").exists(), "a refused file left something behind");
    }

    #[test]
    fn a_pasted_screenshot_with_no_declared_type_is_typed_by_its_name() {
        let data = Scratch::new("data");
        let mac = MacAttachments::open(&data.0);
        assert_eq!(mac.attach_bytes("d", "p", "image.png", "", PNG).unwrap().media_type, "image/png");
        // An unknown declared type does not override a known extension...
        assert_eq!(mac.attach_bytes("d", "q", "scan.pdf", "application/octet-stream", PDF).unwrap().media_type, "application/pdf");
        // ...and neither can make a disguised file acceptable.
        assert!(mac.attach_bytes("d", "r", "evil.png", "image/png", b"#!/bin/sh").is_err());
        assert_eq!(mac.attach_bytes("d", "s", "tool", "application/x-sh", b"#!/bin/sh").unwrap_err(), Origin::Mac.unknown_type());
    }

    #[test]
    fn a_file_removed_before_send_is_reported_missing_and_nothing_is_committed() {
        let data = Scratch::new("data");
        let mac = MacAttachments::open(&data.0);
        let a = mac.attach_bytes("draft-2", "a", "a.png", "image/png", PNG).unwrap();
        let b = mac.attach_bytes("draft-2", "b", "b.pdf", "application/pdf", PDF).unwrap();
        assert!(mac.discard("draft-2", "b").unwrap());
        let wanted = vec![Wanted { id: a.id.clone(), sha256: a.sha256.clone() }, Wanted { id: b.id, sha256: b.sha256 }];
        let answer = mac.commit("thr_1", "draft-2", "hi", &wanted).unwrap();
        assert_eq!(answer, Committed { text: None, files: Vec::new(), missing: vec!["b".into()] });
        // The file that was still there was NOT moved: a partial send is never made.
        assert!(!data.0.join("attachments").join("thr_1").exists());
        let again = mac.commit("thr_1", "draft-2", "", &wanted[..1]).unwrap();
        assert_eq!(again.text.unwrap().lines().next().unwrap(), "Attached on this Mac (1 file, saved by RichOS):");
    }

    // ---- Add to chat: a recorded file through the same desk (Output side panel PRD §12.7) ----

    use richos_core::output::{entry_id, Actor, OutputStore, WriteRow, WriteSource};

    const THREAD: &str = "thr_s7";

    /// A scratch folder by its canonical name (`/var` is `/private/var` on macOS), so a recorded
    /// path and its canonical form agree the way they do for a file Rich wrote.
    fn canonical_scratch(tag: &str) -> Scratch {
        let mut s = Scratch::new(tag);
        s.0 = std::fs::canonicalize(&s.0).unwrap();
        s
    }

    /// Record each path as one witnessed write of [`THREAD`], and answer the file commands'
    /// state for that thread with each path's output id.
    fn recorded(data: &Path, paths: &[&Path]) -> (OutputFiles, Vec<String>) {
        let rows: Vec<WriteRow> = paths
            .iter()
            .enumerate()
            .map(|(i, p)| WriteRow::witnessed(format!("mach:s7-{i}"), THREAD, Some("turn_1"), &p.to_string_lossy(), Actor::Rich, WriteSource::Tool, 1))
            .collect();
        let ids = rows.iter().map(|r| entry_id(THREAD, r.canonical.as_deref().unwrap_or(&r.path))).collect();
        OutputStore::for_data_dir(data).append(THREAD, &rows).unwrap();
        (OutputFiles::for_thread(OutputStore::for_data_dir(data), THREAD, &data.join("cache")), ids)
    }

    fn staged_nothing(data: &Path, draft: &str) -> bool {
        !data.join("phone").join("attachment-staging").join("mac").join(draft).exists()
    }

    #[test]
    fn add_to_chat_stages_a_recorded_file_through_the_desk_and_send_carries_it() {
        let data = canonical_scratch("data");
        let acme = canonical_scratch("acme");
        let brief = write(&acme.0, "brief.md", b"# Brief\n\nHold at list minus 3%.\n");
        let (files, ids) = recorded(&data.0, &[&brief]);
        let mac = MacAttachments::open(&data.0);

        let chip = mac.attach_recorded(&files, "draft-7", "c1", &ids[0]).unwrap();
        assert_eq!(
            (chip.name.as_str(), chip.media_type.as_str(), chip.label.as_str(), chip.size),
            ("brief.md", "text/markdown", "Markdown file", std::fs::metadata(&brief).unwrap().len())
        );
        // Staged exactly where a drop or a paste is: device `mac`, the draft as the message.
        assert!(!staged_nothing(&data.0, "draft-7"));

        let done = mac.commit("thr_s7", "draft-7", "Send this brief to Priya.", &[Wanted { id: chip.id.clone(), sha256: chip.sha256.clone() }]).unwrap();
        let folder = data.0.join("attachments").join("thr_s7").join("draft-7");
        assert_eq!(
            done.text.unwrap(),
            format!(
                "Send this brief to Priya.\n\nAttached on this Mac (1 file, saved by RichOS):\n- {} (text/markdown, {} bytes)",
                folder.join("brief.md").display(),
                chip.size
            )
        );
        assert_eq!(std::fs::read(folder.join("brief.md")).unwrap(), std::fs::read(&brief).unwrap());
        // The thread's file was read, never moved: the panel still lists it where it was written.
        assert!(brief.exists());
    }

    #[test]
    fn add_to_chat_a_30_mib_recorded_file_is_refused_with_the_desks_sentence_verbatim() {
        let data = canonical_scratch("data");
        let acme = canonical_scratch("acme");
        let big = acme.0.join("board-pack.pdf");
        std::fs::File::create(&big).unwrap().set_len(30 * 1024 * 1024).unwrap();
        let (files, ids) = recorded(&data.0, &[&big]);
        let mac = MacAttachments::open(&data.0);
        let refused = mac.attach_recorded(&files, "draft-big", "c1", &ids[0]).unwrap_err();
        assert_eq!(refused, Origin::Mac.too_large());
        assert_eq!(refused, "This file is larger than 25 MB, the most RichOS takes in one file. Nothing was attached.");
        assert!(staged_nothing(&data.0, "draft-big"), "a refused file left something behind");
    }

    #[test]
    fn add_to_chat_a_kind_the_desk_does_not_take_is_refused_in_the_desks_own_words() {
        let data = canonical_scratch("data");
        let acme = canonical_scratch("acme");
        // Kinds the panel lists (an image family member, text by extension, a video) that the
        // desk has never taken; a disguised file and an empty one, which the desk also refuses.
        let svg = write(&acme.0, "chart.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
        let json = write(&acme.0, "comps.json", b"{\"deals\":2}");
        let mp4 = write(&acme.0, "walkthrough.mp4", b"\0\0\0\x18ftypmp42");
        let fake = write(&acme.0, "counter.pdf", PNG);
        let empty = write(&acme.0, "empty.md", b"");
        let (files, ids) = recorded(&data.0, &[&svg, &json, &mp4, &fake, &empty]);
        let mac = MacAttachments::open(&data.0);
        let unknown = "RichOS can't take this kind of file yet. Photos, PDFs, text and Office documents work. Nothing was attached.";
        assert_eq!(Origin::Mac.unknown_type(), unknown);
        for id in &ids[..3] {
            assert_eq!(mac.attach_recorded(&files, "draft-kinds", "k", id).unwrap_err(), unknown);
        }
        assert_eq!(mac.attach_recorded(&files, "draft-kinds", "f", &ids[3]).unwrap_err(), "This file does not look like a PDF. Nothing was attached.");
        assert_eq!(mac.attach_recorded(&files, "draft-kinds", "e", &ids[4]).unwrap_err(), "This file is empty. Nothing was attached.");
        assert!(staged_nothing(&data.0, "draft-kinds"), "a refused file left something behind");
    }

    #[test]
    fn add_to_chat_takes_only_an_output_id_of_the_active_thread_and_never_through_a_link() {
        let data = canonical_scratch("data");
        let acme = canonical_scratch("acme");
        let gone = write(&acme.0, "gone.md", b"# Gone\n");
        let real = write(&acme.0, "real.md", b"# Real\n");
        let link = acme.0.join("link.md");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let swapped = write(&acme.0, "swapped.md", b"# Swapped\n");
        std::fs::create_dir(acme.0.join("Reports.md")).unwrap();
        let folder = acme.0.join("Reports.md");
        let (files, ids) = recorded(&data.0, &[&gone, &link, &swapped, &folder]);
        // After the witness: one deleted, one replaced by a link to another file.
        std::fs::remove_file(&gone).unwrap();
        std::fs::remove_file(&swapped).unwrap();
        std::os::unix::fs::symlink(&real, &swapped).unwrap();
        let mac = MacAttachments::open(&data.0);

        assert_eq!(mac.attach_recorded(&files, "d7", "a", &ids[0]).unwrap_err(), output_files::MISSING);
        assert_eq!(mac.attach_recorded(&files, "d7", "b", &ids[1]).unwrap_err(), output_files::LINKED);
        assert_eq!(mac.attach_recorded(&files, "d7", "c", &ids[2]).unwrap_err(), output_files::LINKED);
        assert_eq!(mac.attach_recorded(&files, "d7", "d", &ids[3]).unwrap_err(), output_files::NOT_A_FILE);
        // An id the record does not hold, a path where an id belongs, and another thread's id.
        assert_eq!(mac.attach_recorded(&files, "d7", "e", "out_0000000000000000").unwrap_err(), output_files::NOT_IN_RECORD);
        assert_eq!(mac.attach_recorded(&files, "d7", "f", &real.to_string_lossy()).unwrap_err(), output_files::NOT_IN_RECORD);
        let elsewhere = OutputFiles::for_thread(OutputStore::for_data_dir(&data.0), "thr_other", &data.0.join("cache"));
        assert_eq!(mac.attach_recorded(&elsewhere, "d7", "g", &ids[2]).unwrap_err(), output_files::NOT_IN_RECORD);
        // Hostile ids never reach the file system.
        assert_eq!(mac.attach_recorded(&files, "../d7", "h", &ids[1]).unwrap_err(), NOT_SAVED);
        assert_eq!(mac.attach_recorded(&files, "d7", "a/b", &ids[1]).unwrap_err(), NOT_SAVED);
        assert!(staged_nothing(&data.0, "d7"), "a refused file left something behind");
    }

    #[test]
    fn names_travel_percent_encoded_and_a_broken_escape_is_refused() {
        assert_eq!(percent_decode("Screenshot%202026-09-24%20at%2010.02.11.png").unwrap(), "Screenshot 2026-09-24 at 10.02.11.png");
        assert_eq!(percent_decode("Gr%C3%BC%C3%9Fe.pdf").unwrap(), "Grüße.pdf");
        assert!(percent_decode("bad%2").is_none());
        assert!(percent_decode("bad%zz.pdf").is_none());
        assert!(percent_decode("%C3%28").is_none());
    }
}
