//! **THE ONE RULE FOR A MODEL TURN THE APP STARTS BY ITSELF** (CEO 2026-11-26: "Everything
//! was either stopped, finished, ended or paused in the RichOS app. So, unless I'm missing
//! something major, there should have been absolutely zero usage after me just opening the
//! app.").
//!
//! A turn of Rich's, of Stu's, or a review starts only for one of three reasons:
//!
//! - **the user acted** in this run: his own words, or his own press in that chat;
//! - **the input arrived while the app is open**: a Codex report written now, an order he
//!   filed whose condition came true now, a report of work running now. These reach the team
//!   by themselves, as they always did;
//! - **a turn that was actually running when the app last ended carries on** (an update
//!   restart, a crash, a quit mid-turn): Rich's turn still in flight in the ledger, Stu's input
//!   with its delivery attempt still open.
//!
//! Everything saved from before the app opened that was not running then (a Codex report or
//! reply written while it was closed, a review handover, a Stop, an account notice, an input
//! or answer saved and never started, stopped, finished, ended or paused work) waits until the
//! user is next active in that chat, and is then delivered in order. A job record from an
//! earlier run that still says "running", "waiting" or "paused" opens nothing: a quit never
//! rewrites it.
//!
//! What counts as the user acting in a chat: his own words in it since launch (a
//! `PromptReceived` row in the conversation ledger whose source is `text` or `jam`, by
//! whichever road: the composer, the phone, voice), or one of his own presses there
//! ([`TurnGate::user_acted`]: answering a question, stopping a job). An agent's message, an
//! internal re-prime and Rich speaking unprompted are not his acting. Once a chat is open it
//! stays open until the app quits.
//!
//! "Keep chat alive", the user's own setting, is not asked here: it fires only for a kept chat
//! whose session made a request within the hour (`chat_cache::heartbeat_due`), which is a chat
//! it was already keeping alive when the app last ended.
//!
//! Every send call that reaches a model asks [`TurnGate::may_start`] with what it knows about
//! the input ([`Origin`]); the list of those calls is pinned by richos-core `tests/turn_gate_callers.rs`,
//! which fails when a new one appears. The app's review watcher, a process of its own, reads
//! the chats a turn has been let through in from [`RECORD`], which this writes as they open.

use crate::entity::ThreadBinding;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The chats a turn has been let through in this run, for the app's review watcher
/// (`review_watch.py` `AppWorld.may_start`), in the app's engine state folder. Written empty
/// when the app opens.
pub const RECORD: &str = "turn-gate.json";

/// What the send call asking knows about the input it is about to send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The user's own words or press in this run, or input that arrived while the app is open.
    Now,
    /// A turn that was actually running when the app last ended carries on.
    CarriesOn,
    /// Saved before the app opened and not running then: it waits until the user acts in its
    /// chat.
    FromBefore,
}

pub struct TurnGate {
    since_ms: u64,
    ledger: PathBuf,
    record: PathBuf,
    /// How long each chat's inbox (`<state>/messages/<company>/<chat>/<role>.jsonl`) was when
    /// the app opened ([`Self::arrived_while_open`]).
    inboxes: HashMap<PathBuf, u64>,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// How far into the ledger has been read.
    read: u64,
    /// Chats with his own words since launch, read from the ledger.
    asked: HashSet<String>,
    /// Chats with one of his own presses since launch.
    pressed: HashSet<String>,
    /// Chats a turn has been let through in, with their company, as [`RECORD`] lists them.
    open: BTreeSet<(String, String)>,
}

impl TurnGate {
    /// **The app is opening now.** `state` is the app's engine state folder; the conversation
    /// ledger sits beside it. Nothing is open yet, and the record says so before any watcher
    /// starts. Every inbox's length is read once: a few dozen `stat` calls.
    pub fn at_launch(state: &Path) -> Self {
        let mut inboxes = HashMap::new();
        let mut folders = vec![state.join("messages")];
        for _ in 0..2 {
            folders = folders.iter().filter_map(|folder| std::fs::read_dir(folder).ok())
                .flat_map(|entries| entries.flatten().map(|entry| entry.path()))
                .filter(|path| path.is_dir()).collect();
        }
        for folder in folders {
            for role in ["rich", "stu"] {
                let inbox = folder.join(format!("{role}.jsonl"));
                if let Ok(length) = std::fs::metadata(&inbox).map(|file| file.len()) {
                    inboxes.insert(inbox, length);
                }
            }
        }
        let gate = Self {
            since_ms: crate::util::now_millis(),
            ledger: state.parent().unwrap_or(state).join("conversation-ledger.jsonl"),
            record: state.join(RECORD),
            inboxes,
            inner: Mutex::new(Inner::default()),
        };
        gate.write(&BTreeSet::new());
        gate
    }

    /// When the app opened, in the ledger's clock.
    pub fn since_ms(&self) -> u64 {
        self.since_ms
    }

    /// **Was the inbox row ending at `through` written while the app is open?** A row within
    /// the inbox's length at launch was saved before; an inbox made since holds only new rows.
    pub fn arrived_while_open(&self, inbox: &Path, through: u64) -> bool {
        through > self.inboxes.get(inbox).copied().unwrap_or(0)
    }

    /// **The user pressed something of his own in this chat** (answered a question, stopped
    /// a job): it is open from now on.
    pub fn user_acted(&self, binding: &ThreadBinding) {
        self.inner.lock().unwrap().pressed.insert(binding.thread_id().to_string());
        self.may_start(binding, Origin::Now);
    }

    /// **THE RULE. May the app start this model turn?** The user's own input, input that
    /// arrived while the app is open, and a turn that was running when the app last ended:
    /// yes. Anything saved from before: only once the user has acted in its chat during this
    /// run. An unreadable ledger answers no: what waits can still be given later, a model turn
    /// cannot be taken back.
    pub fn may_start(&self, binding: &ThreadBinding, origin: Origin) -> bool {
        let thread = binding.thread_id();
        let mut inner = self.inner.lock().unwrap();
        if origin == Origin::FromBefore && !inner.pressed.contains(thread) && !inner.asked.contains(thread) {
            self.read_ledger(&mut inner);
            if !inner.asked.contains(thread) {
                return false;
            }
        }
        if inner.open.insert((binding.entity_id().as_str().to_string(), thread.to_string())) {
            let open = inner.open.clone();
            drop(inner);
            self.write(&open);
        }
        true
    }

    /// Read what was appended to the ledger since the last look: a look costs one `stat`
    /// while nothing was written.
    fn read_ledger(&self, inner: &mut Inner) {
        use std::io::{BufRead, Seek, SeekFrom};
        let Ok(mut file) = std::fs::File::open(&self.ledger) else { return };
        let length = file.metadata().map(|file| file.len()).unwrap_or(0);
        // A ledger shorter than what was read was rewritten: read it again from its start.
        if length < inner.read {
            inner.read = 0;
        }
        if length == inner.read || file.seek(SeekFrom::Start(inner.read)).is_err() {
            return;
        }
        let mut reader = std::io::BufReader::new(file);
        let mut line = String::new();
        loop {
            line.clear();
            // A concurrent append may not have completed its last record yet.
            match reader.read_line(&mut line) {
                Ok(count) if count > 0 && line.ends_with('\n') => inner.read += count as u64,
                _ => break,
            }
            if !line.contains("PromptReceived") {
                continue;
            }
            let Ok(row) = serde_json::from_str::<Value>(&line) else { continue };
            if row["event"] == "PromptReceived"
                && matches!(row["source"].as_str(), Some("text" | "jam"))
                && row["at"].as_u64().is_some_and(|at| at >= self.since_ms)
            {
                if let Some(thread) = row["thread_id"].as_str() {
                    inner.asked.insert(thread.to_string());
                }
            }
        }
    }

    /// The record is replaced whole, so the watcher never reads half of one.
    fn write(&self, open: &BTreeSet<(String, String)>) {
        let body = json!({
            "version": 1,
            "since_ms": self.since_ms,
            "open": open.iter().map(|(entity, thread)| json!({"entity_id": entity, "thread_id": thread})).collect::<Vec<_>>(),
        });
        let partial = self.record.with_extension("json.partial");
        let written = std::fs::create_dir_all(self.record.parent().unwrap_or(Path::new(".")))
            .and_then(|()| std::fs::write(&partial, body.to_string()))
            .and_then(|()| std::fs::rename(&partial, &self.record));
        if let Err(error) = written {
            // The watcher then starts no review in a chat it cannot see open: it waits.
            eprintln!("[richos] the open chats could not be written for the review watcher: {error}");
        }
    }
}

/// For the places that are given a gate after they are made: none set (a test's host) lets
/// every turn through, as before this rule existed. The app sets one at launch, before any
/// of them can start a turn (`tests/turn_gate_callers.rs` pins that too).
pub type Slot = std::sync::OnceLock<std::sync::Arc<TurnGate>>;

pub fn allows(slot: &Slot, binding: &ThreadBinding, origin: Origin) -> bool {
    slot.get().is_none_or(|gate| gate.may_start(binding, origin))
}

/// [`TurnGate::arrived_while_open`], or yes with no gate set.
pub fn arrived(slot: &Slot, inbox: &Path, through: u64) -> bool {
    slot.get().is_none_or(|gate| gate.arrived_while_open(inbox, through))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{Ledger, Source};

    fn fixture() -> (PathBuf, ThreadBinding, ThreadBinding) {
        let root = std::env::temp_dir().join(format!("richos-turn-gate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("engine-state")).unwrap();
        let mut ledger = Ledger::open(root.join("conversation-ledger.jsonl")).unwrap();
        let entity = crate::EntityId::parse("company-a").unwrap();
        let one = ledger.create_thread("One", &entity).unwrap();
        let two = ledger.create_thread("Two", &entity).unwrap();
        let (one, two) = (ledger.thread_binding(&one).unwrap(), ledger.thread_binding(&two).unwrap());
        ledger.record_prompt_received(&one, "Yesterday's request", Source::Text).unwrap();
        (root, one, two)
    }

    fn said(root: &Path, binding: &ThreadBinding, source: Source) {
        let mut ledger = Ledger::open(root.join("conversation-ledger.jsonl")).unwrap();
        ledger.record_prompt_received(binding, "{\"from\":\"stu\"}", source).unwrap();
    }

    fn record(root: &Path) -> Value {
        serde_json::from_slice(&std::fs::read(root.join("engine-state").join(RECORD)).unwrap()).unwrap()
    }

    /// Opening the app opens no chat: his words from before it opened, an agent's message, a
    /// re-prime and Rich speaking unprompted leave every chat shut to what was saved before.
    #[test]
    fn nothing_saved_starts_at_launch_and_only_his_own_words_since_open_a_chat() {
        let (root, one, two) = fixture();
        // The ledger's clock is in milliseconds: what was written before is strictly earlier.
        // load-bound: this only moves the millisecond clock past what was saved; host load makes the gap larger, never smaller.
        std::thread::sleep(std::time::Duration::from_millis(3));
        let gate = TurnGate::at_launch(&root.join("engine-state"));
        assert_eq!(record(&root)["open"], json!([]));
        for source in [Source::Agent, Source::Internal, Source::Proactive] {
            said(&root, &one, source);
            assert!(!gate.may_start(&one, Origin::FromBefore), "{source:?} opened the chat");
        }
        assert_eq!(record(&root)["open"], json!([]), "a refusal recorded the chat as open");
        said(&root, &two, Source::Jam);
        assert!(gate.may_start(&two, Origin::FromBefore), "his spoken words did not open the chat");
        assert!(!gate.may_start(&one, Origin::FromBefore), "his words in one chat opened another");
        assert_eq!(record(&root)["open"], json!([{"entity_id":"company-a","thread_id":two.thread_id()}]));
        said(&root, &one, Source::Text);
        assert!(gate.may_start(&one, Origin::FromBefore));
        assert_eq!(record(&root)["open"].as_array().unwrap().len(), 2);
    }

    /// Input that arrived while the app is open, and a turn that was running when it ended,
    /// start in a chat he has not touched; the chat is then recorded for the review watcher.
    #[test]
    fn new_input_and_a_turn_that_was_running_start_by_themselves() {
        let (root, one, two) = fixture();
        let gate = TurnGate::at_launch(&root.join("engine-state"));
        assert!(gate.may_start(&one, Origin::Now));
        assert!(gate.may_start(&two, Origin::CarriesOn));
        assert_eq!(record(&root)["open"].as_array().unwrap().len(), 2);
    }

    /// His own press opens the chat it was in, and nothing else.
    #[test]
    fn his_press_opens_its_chat() {
        let (root, one, two) = fixture();
        let gate = TurnGate::at_launch(&root.join("engine-state"));
        gate.user_acted(&two);
        assert!(gate.may_start(&two, Origin::FromBefore));
        assert!(!gate.may_start(&one, Origin::FromBefore));
    }

    /// A row within an inbox's length at launch was saved before; a row past it, or in an
    /// inbox made since, arrived while the app is open.
    #[test]
    fn an_inbox_row_is_new_when_it_ends_past_the_inbox_length_at_launch() {
        let (root, _, _) = fixture();
        let inbox = root.join("engine-state/messages/company-a/thr_x/stu.jsonl");
        std::fs::create_dir_all(inbox.parent().unwrap()).unwrap();
        std::fs::write(&inbox, [b'\n'; 40]).unwrap();
        let gate = TurnGate::at_launch(&root.join("engine-state"));
        assert!(!gate.arrived_while_open(&inbox, 40));
        assert!(gate.arrived_while_open(&inbox, 41));
        assert!(gate.arrived_while_open(&inbox.with_file_name("rich.jsonl"), 1));
    }

    /// A host with no gate set lets every turn through, as before.
    #[test]
    fn an_empty_slot_lets_every_turn_through() {
        let (root, one, _) = fixture();
        let slot = Slot::new();
        assert!(allows(&slot, &one, Origin::FromBefore));
        slot.set(std::sync::Arc::new(TurnGate::at_launch(&root.join("engine-state")))).ok().unwrap();
        assert!(!allows(&slot, &one, Origin::FromBefore));
    }
}
