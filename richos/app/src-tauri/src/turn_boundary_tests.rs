//! **WORK HE GIVES ALOUD OR FROM THE PHONE STARTS LIKE TYPED WORK** — the shell's turn
//! boundary, entrance by entrance.
//!
//! CEO ruling §88: *"The user should be able to answer on their phone just as well as they
//! can answer on the desktop app."* Until 2026-09-26 only `send_message` adopted the work Rich
//! wrote down during a turn; a spoken turn and the phone's drain adopted nothing, so an
//! assignment he gave either way sat `Registered` until he next typed
//! (esc-20260926T083231Z-23865924).
//!
//! The fixture is the real pair: a real `Spine` over a real ledger, and a real `WorkHost` over
//! a state directory beside it, exactly as `main.rs` lays them out (`<data>/conversation-
//! ledger.jsonl`, `<data>/engine-state`). The conversation's lease does what the real one does
//! when Rich calls `richos_assignments.record`: it writes a `Registered` assignment, scoped by
//! the host to the turn he gave it in, and ends the turn on the receipt. The work host's
//! factory records every work lease it is asked for, which is the moment the assignment has
//! left `Registered` and is running — and then refuses, so nothing real is started.

use super::*;
use richos_core::assignment::{self, AssignmentState, Registration};
use richos_core::cognition::TurnItem;
use richos_core::entity::ThreadBinding;
use richos_core::permissions::PermissionDesk;
use richos_core::work_host::{SilentNotifier, WorkHost};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The conversation's lease, registering what he asked for the way the assignment tool does.
struct WritesItDown {
    state: PathBuf,
    scope: Option<(ThreadBinding, String, String)>,
}

impl Cognition for WritesItDown {
    fn session_id(&self) -> &str {
        "writes-it-down"
    }
    fn reprime(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        Ok(())
    }
    /// The host's scope for the visible turn: the binding, the turn and his exact words —
    /// the same three facts the real tool's scope file carries.
    fn prepare_work_turn(
        &mut self,
        binding: &ThreadBinding,
        turn: &str,
        _source: Source,
        text: &str,
        _on_item: &mut dyn FnMut(TurnItem),
    ) -> Result<(), CognitionError> {
        self.scope = Some((binding.clone(), turn.to_string(), text.to_string()));
        Ok(())
    }
    fn prompt(&mut self, _: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        let (binding, turn, text) = self.scope.take().expect("the host scopes every visible turn");
        let digest = ring::digest::digest(&ring::digest::SHA256, text.as_bytes());
        let request = Registration {
            entity_id: binding.entity_id().to_string(),
            thread_id: binding.thread_id().to_string(),
            obligation_id: format!("obligation-{turn}"),
            instruction_ledger_ref: format!("ledger:{}:{turn}", binding.thread_id()),
            instruction_sha256: digest.as_ref().iter().map(|b| format!("{b:02x}")).collect(),
            title: text,
            repositories: Vec::new(),
            needs_screen: false,
        };
        assignment::register(&self.state, &request).map_err(|e| CognitionError::Protocol(e.to_string()))?;
        on_item(TurnItem::Text { seq: 0, text: "On it!" });
        Ok("end_turn".into())
    }
}

/// The conversation's factory: every front desk is a `WritesItDown`.
struct ConversationDesks(PathBuf);

impl LeaseFactory for ConversationDesks {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        Ok(Box::new(WritesItDown { state: self.0.clone(), scope: None }))
    }
}

/// The work host's factory: records the conversation every work lease is asked for, then
/// refuses, so the observation is the start and nothing else happens.
struct RecordsWorkStarts(Arc<Mutex<Vec<String>>>);

impl LeaseFactory for RecordsWorkStarts {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        Err(CognitionError::Protocol("not a conversation factory".into()))
    }
    fn spawn_work(&self, binding: &ThreadBinding) -> Result<Box<dyn Cognition>, CognitionError> {
        self.0.lock().unwrap().push(binding.thread_id().to_string());
        Err(CognitionError::Protocol("the test opens no work lease".into()))
    }
}

struct Mac {
    root: PathBuf,
    state: PathBuf,
    spine: Mutex<Spine>,
    work: Arc<WorkHost>,
    started: Arc<Mutex<Vec<String>>>,
    control: TurnControl,
    shown: String,
    other: String,
}

fn a_mac(tag: &str) -> Mac {
    let root = std::env::temp_dir().join(format!(
        "richos-turn-boundary-{tag}-{}-{}",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let state = root.join("engine-state");
    std::fs::create_dir_all(&state).unwrap();
    let mut spine = Spine::new(Ledger::open(root.join("conversation-ledger.jsonl")).unwrap());
    spine.set_entity_registry(
        EntityRegistry::new(vec![Entity::new("alpha", "Alpha", &["/fixture/alpha"]).unwrap()]).unwrap(),
    );
    let alpha = EntityId::parse("alpha").unwrap();
    let shown = spine.create_thread("The proposal", &alpha).unwrap();
    let other = spine.create_thread("The hiring plan", &alpha).unwrap();
    spine.switch_thread(&shown).unwrap();
    spine.set_lease_factory(Box::new(ConversationDesks(state.clone())));
    let control = TurnControl::open(root.join("intake.jsonl")).unwrap();
    spine.set_turn_control(control.clone());
    let started = Arc::new(Mutex::new(Vec::new()));
    let work = WorkHost::new(
        &state,
        Box::new(RecordsWorkStarts(started.clone())),
        Arc::new(SilentNotifier),
        Arc::new(PermissionDesk::default()),
    );
    work.start();
    Mac { root, state, spine: Mutex::new(spine), work, started, control, shown, other }
}

impl Mac {
    fn states(&self, thread: &str) -> Vec<AssignmentState> {
        assignment::read_all(&self.state, "alpha", thread)
            .unwrap_or_default()
            .into_iter()
            .map(|row| row.state)
            .collect()
    }
    /// Bounded: the runner is a thread of its own, so the start is awaited, never assumed.
    fn work_started_on(&self, thread: &str, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        loop {
            if self.started.lock().unwrap().iter().any(|t| t == thread) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn finish(self) {
        self.work.shutdown();
        std::fs::remove_dir_all(&self.root).expect("the test's own directory could not be removed");
    }
}

const HIS_TASK: &str = "land the three branches";

#[test]
fn work_he_gives_aloud_starts_without_a_typed_message() {
    let mac = a_mac("spoken");
    run_the_spoken_turn(mac.spine.lock().unwrap(), &mac.work, HIS_TASK, false);
    assert_eq!(mac.states(&mac.shown).len(), 1, "positive control: the spoken turn wrote the task down");
    assert!(
        mac.work_started_on(&mac.shown, Duration::from_secs(10)),
        "a task he gave aloud is still Registered with nothing started: {:?}",
        mac.states(&mac.shown)
    );
    mac.finish();
}

#[test]
fn work_he_gives_from_the_phone_starts_without_a_typed_message() {
    let mac = a_mac("phone");
    // His phone is on the OTHER conversation, not the one the Mac shows.
    mac.control
        .submit_from_channel(&mac.other, Some(EntityId::parse("alpha").unwrap()), HIS_TASK, "phone")
        .unwrap();
    drain_incoming_messages(&mac.spine, &mac.work).unwrap();
    assert_eq!(mac.states(&mac.other).len(), 1, "positive control: the phone's turn wrote the task down");
    assert!(
        mac.work_started_on(&mac.other, Duration::from_secs(10)),
        "a task he gave from the phone is still Registered with nothing started: {:?}",
        mac.states(&mac.other)
    );
    assert!(mac.states(&mac.shown).is_empty(), "nothing was given on the conversation the Mac shows");
    mac.finish();
}

/// **The door is what starts it, and nothing else does.** The same turn run straight through
/// the spine, with no boundary, leaves the task `Registered` and nothing asked for: the host
/// has no timer that would have picked it up, so the two tests above are measuring the door.
#[test]
fn without_the_boundary_the_task_stays_registered_and_the_door_starts_it() {
    let mac = a_mac("door");
    let mut spine = mac.spine.lock().unwrap();
    spine.submit_prompt_spoken(HIS_TASK, Source::Jam, false).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(mac.states(&mac.shown), vec![AssignmentState::Registered]);
    assert!(mac.started.lock().unwrap().is_empty(), "nothing may start work but a turn boundary");
    assert_eq!(adopt_at_the_turn_boundary(spine, &mac.work), 1);
    assert!(mac.work_started_on(&mac.shown, Duration::from_secs(10)));
    mac.finish();
}

// ---------------------------------------------------------------------------------------
// EVERY ENTRANCE, BY NAME
// ---------------------------------------------------------------------------------------

/// The calls that run one of his turns. A shell line that makes one must be followed by a
/// boundary door before the next function begins.
const RUNS_HIS_TURNS: [&str; 6] = [
    ".submit_prompt(",
    ".submit_prompt_to(",
    ".submit_prompt_spoken(",
    ".poll_intake(",
    ".reconcile_intake(",
    ".prime_front_desk(",
];

/// The doors: the one function that adopts, and the two entrance functions that end in it.
const DOORS: [&str; 3] = ["adopt_at_the_turn_boundary(", "run_the_spoken_turn(", "drain_incoming_messages("];

/// The shell's shipped source with every `#[cfg(test)]` item removed, as `(file, line no, line)`.
/// Braces are counted per line outside `//` comments; test items here are ordinary modules.
fn shipped_lines(file: &str, source: &str) -> Vec<(String, usize, String)> {
    let mut out = Vec::new();
    let mut skipping = false;
    let mut depth: i64 = 0;
    let mut entered = false;
    for (n, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        if !skipping && line.trim() == "#[cfg(test)]" {
            skipping = true;
            depth = 0;
            entered = false;
            continue;
        }
        if skipping {
            for c in code.chars() {
                match c {
                    '{' => {
                        depth += 1;
                        entered = true;
                    }
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            // `mod x;` (a file module) ends at its semicolon; a block ends when it closes.
            if (!entered && code.trim_end().ends_with(';')) || (entered && depth == 0) {
                skipping = false;
            }
            continue;
        }
        out.push((file.to_string(), n + 1, line.to_string()));
    }
    out
}

fn is_fn_start(line: &str) -> bool {
    let t = line.trim_start();
    let t = t.strip_prefix("pub(crate) ").or_else(|| t.strip_prefix("pub ")).unwrap_or(t);
    let t = t.strip_prefix("async ").unwrap_or(t);
    t.starts_with("fn ")
}

/// Every shipped line that runs his turn, and whether a door follows it inside its function.
fn entrances() -> Vec<(String, usize, bool)> {
    let files = [
        ("src/main.rs", include_str!("main.rs")),
        ("src/phone/bridge.rs", include_str!("phone/bridge.rs")),
    ];
    let mut found = Vec::new();
    for (file, source) in files {
        let lines = shipped_lines(file, source);
        for (i, (_, n, line)) in lines.iter().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if !RUNS_HIS_TURNS.iter().any(|call| code.contains(call)) {
                continue;
            }
            let door = lines[i..]
                .iter()
                .skip(1)
                .take_while(|(_, _, next)| !is_fn_start(next))
                .any(|(_, _, next)| DOORS.iter().any(|d| next.split("//").next().unwrap_or("").contains(d)));
            found.push((file.to_string(), *n, door));
        }
    }
    found
}

#[test]
fn every_entrance_that_runs_his_turns_adopts_at_its_boundary() {
    let found = entrances();
    eprintln!("entrances that run his turns: {found:?}");
    // Positive control: the scan sees every entrance there is. Typed (two roads), spoken,
    // phone, priming and boot — six lines. A new entrance changes this count on purpose.
    assert_eq!(
        found.len(),
        6,
        "the shell's entrances that run his turns changed; each needs a turn-boundary door: {found:?}"
    );
    let missing: Vec<_> = found.iter().filter(|(_, _, door)| !door).collect();
    assert!(missing.is_empty(), "these run his turns and never adopt what they wrote down: {missing:?}");
}

#[test]
fn the_voice_callback_and_the_phone_drain_go_through_their_doors() {
    let main = shipped_lines("src/main.rs", include_str!("main.rs"));
    let bridge = shipped_lines("src/phone/bridge.rs", include_str!("phone/bridge.rs"));
    let calls = |lines: &[(String, usize, String)], door: &str| {
        lines.iter().filter(|(_, _, l)| l.split("//").next().unwrap_or("").contains(door)).count()
    };
    // The voice callback's one call, and the phone drain's.
    assert_eq!(calls(&main, "run_the_spoken_turn(spine, &state.work"), 1, "the voice callback must end in its door");
    assert_eq!(calls(&bridge, "crate::dispatch_incoming_messages(&self.app, &thread)"), 1, "the phone must drain through its door");
    // The scan's own negative control: a door removed from the text is seen as missing.
    let broken = include_str!("main.rs").replace("adopt_at_the_turn_boundary(&mut spine, &work);", "");
    let lines = shipped_lines("src/main.rs", &broken);
    let reconcile = lines.iter().position(|(_, _, l)| l.contains("spine.reconcile_intake()")).unwrap();
    let adopted = lines[reconcile..]
        .iter()
        .skip(1)
        .take_while(|(_, _, l)| !is_fn_start(l))
        .any(|(_, _, l)| DOORS.iter().any(|d| l.split("//").next().unwrap_or("").contains(d)));
    assert!(!adopted, "the scan must notice a missing door, or it proves nothing");
}
