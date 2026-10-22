// RichOS desktop shell (Tauri). A THIN surface over the richos-core spine.
//
// Doctrine: clean output (only Rich's assistant text renders), one conversation with
// Rich, optional multi-thread topic organization. All runtime intelligence — the native
// client, the crash-safe ledger, threads, re-prime continuity — lives in richos-core;
// this file is just the window + the Tauri command bridge to the web UI in ../ui.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod question_host;
mod peer_messages;
mod activation;
mod lifecycle;
mod events;
// WHAT THE PERSON SEES WHEN THIS PROCESS DIES BEFORE THE WINDOW EXISTS. Declared next to
// `activation` because it is armed by activation's own three-fact rule and by nothing else.
mod startup_alert;
mod update_startup;
/// The relaunch into a downloaded update once nothing is running (CEO feedback 2026-10-06, item 1).
mod update_relaunch;
// HOW BIG THE WINDOW OPENS AND WHERE — derived from the display it opens on, never from a
// constant. `docs/hardware-choices-2026-09-10.md` D2. Deliberately free of every Tauri type
// so the arithmetic is unit-testable and can be dry-run with no window created at all
// (`examples/window_placement.rs`); the two functions that talk to the runtime,
// `read_displays` and `remember_window_geometry`, live next to the window they serve below.
mod window_geometry;

// READING THE MAC'S SCREEN LOCK — the CEO's ruling §56's shell half. The syscall lives here
// because the frameworks do; the rule about what waits lives in `richos_core::screen` because
// the test suite does. Same split `work_gate.rs` documents for the update gate.
mod screen;

use richos_core::read_view::{SpineReader, SpineView};
use richos_core::native::{resolve_claude_bin, resolve_claude_bin_checked, NativeCognition};
use richos_core::cognition::{Cognition, CognitionError, LeaseFactory};
use richos_core::config::{Assertiveness, ConfigStore, RetentionChoice, TechyMode, TechyScope};
use richos_core::launch::{LaunchCounts, LaunchKind, LaunchStore, PriorRun};
use richos_core::correction::{
    CliLoroWriter, CorrectionDesk, Proposal, ProposalObserver, ProposedWrite,
    SharedCorrectionDesk, WriteOutput, EVENT_LORO_PROPOSED,
};
use richos_core::entity::{Entity, EntityId, EntityRegistry, RegistrySource, ENTITY_ID_MAX_LEN};
use richos_core::feedback::{
    ContributingCondition, DiagnosisTerm, Disclosure, FailureClass, FeedbackEntry, FeedbackPayload,
    FeedbackStore, Occurrences, PromptOutcome, Rating, ReportDecision, DISCLOSURE_HEADING,
    PROMPT_OPTIONS, PROMPT_QUESTION, REPORT_OFFER, TAXONOMY_VERSION,
};
use richos_core::home_field;
use richos_core::journal::{MachineryJournal, RawRetention};
use richos_core::ledger::{AttentionTier, Ledger, Message, Source};
use richos_core::loro::{SharedSliceProvenance, SliceProvenance};
use richos_core::machinery::{MachineryObserver, MachineryRecord, EVENT_MACHINERY};
use richos_core::spine::{Spine, WorkerEventsSource};
use richos_core::heard::{DictationJournal, HeardSource};
use richos_core::staging::{
    Candidate, CandidateDesk, CliVocabulary, CorrectionObserver, LearnOutcome, SharedCandidateDesk,
    Staged, EVENT_CORRECTION_STAGED,
};
use richos_core::steering::{IntakeRecord, StopOutcome, TurnControl, ConversationControl};
use richos_core::stream::{StreamEvent, TurnObserver};
use richos_core::thread::ThreadSummary;
use richos_core::worker_status::WorkerStatusView;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

/// WHERE THE ENGINE DIRECTORY IS. Its own file because the answer is an ORDER of seven
/// candidates that has to be readable and testable — the one-expression default it replaced
/// was a `cargo run` assumption that resolved to a nonexistent path under a Finder launch,
/// and nothing tested it because there was nothing to test.
mod engine;

/// Durable left-navigation view state (pin, rename, archive, rail width). See nav.rs
/// for why these are shell state and not ledger events.
mod nav;

/// Check, download, VERIFY, install, relaunch — the update path. Its own file because it
/// is the one surface where a mistake ships code to the CEO's machine, and because the
/// signature-failure classification in it is unit-tested without a webview.
mod updates;

/// The `get_timeline` command body, in its own file so `examples/timeline_payload.rs`
/// can include the SAME source and print the exact JSON the webview receives.
mod timeline_view;

/// The `get_machinery` command body — techy mode's read path. Its own file for the same
/// reason `timeline_view` is: an example includes the SAME source and prints the exact
/// JSON the webview receives.
mod machinery_view;
use machinery_view::machinery_payload;

/// COMPANY MEMORY: where it is, whether it is readable, and — new on 2026-09-01 — how a
/// machine that has none gets one. Its own file because `wire_company_memory` runs twice:
/// once at boot, and once more the moment the CEO answers the first-run question.
mod memory;
use memory::MemoryStatus;

/// FIRST-RUN SETUP — Option D's surface. Detects the two executables a customer's Mac may not
/// have (`claude`, and the engine directory), asks once, and installs them.
/// `richos_core::setup` holds the decisions; this file holds the window's side of them.
mod setup_view;

/// RICH'S ONE OFFER OF DICTATION after setup (round 19, state 4): whether it may be shown, and
/// the record that it was, as `offered` in `<data dir>/dictation.json`.
mod dictation_offer;

/// GETTING THE SPEECH MODEL, so ".github/README.md: Voice does not work yet" can stop being
/// true. The transport only — every rule about what the bytes are, whether there is room and
/// whether what arrived is the model RichOS pinned lives in `richos_voice::provision`, which is
/// pure enough to be tested without a network and is.
mod voice_provision;

/// **THE PHONE CHANNEL — this app's first inbound network listener** (CEO decision §57; plan
/// `richos-hq/docs/plans/richos-phone-client-2026-09-18.md`; wire contract
/// `docs/architecture/phone-channel.md`).
///
/// A module rather than more of `main.rs`, and a deliberately wide one: a certificate
/// authority, a TLS listener, four routes and a push sender is an organ, not a feature flag.
/// Everything it is allowed to reach is handed to it — it holds no path to the ledger, the raw
/// event stream or a `Timeline`, which is what makes plan §4.2 (iii) structural rather than a
/// rule somebody has to remember.
mod phone;

/// SCREENSHOTS AND FILES DROPPED OR PASTED ONTO THE MAC COMPOSER (CEO §86). The phone's
/// attachment desk, reached from the window: same storage, same limits, same words for Rich.
mod mac_attachments;

/// THE FILES A THREAD PRODUCED, REACHED SAFELY (Output side panel PRD §5): the commands take
/// an output id, never a path, and the `richos-output` scheme serves only what the active
/// thread's record holds.
mod output_files;
/// BUST A BUG (CEO §115, round 21): Rich writes the report, the reporting account's token is
/// read from the keychain at send time, and a report that could not go waits on this Mac.
mod bug_report;
/// Bust a bug's picture of RichOS's own window, for the user's own Claude to look at
/// (WKWebView's snapshot: no screen-recording permission).
mod window_picture;
/// The web view's own right-click menu: only Cut, Copy, Paste and Select All in a text field,
/// nothing anywhere else, the PDF preview included (CEO, nightly 65).
mod webview_menu;

// Headless integration harness; absent from the shipped executable.
#[cfg(test)]
#[path = "../../../mobile/dev/mac-server.rs"]
mod mobile_mac_server;

/// **Opening one of five known addresses, and nothing else** (CEO §61.1). An allowlist rather
/// than a URL opener: the how-to screens need five fixed destinations, so the command takes the
/// address as a key into a table rather than as data to act on.
mod opener;
mod emission_trace;

/// **DICTATION IN ANY APP** (`richos-hq/docs/plans/2026-10-08-dictation-anywhere.md`, rev 2).
/// `dictation/` is the tool's macOS edges (the same executable started with `--richos-dictation`);
/// `dictation_app` is this app's side of it: starting the tool, the socket client, the voice-mode
/// yield. Every decision is in `richos_voice::dictation`.
#[cfg(target_os = "macos")]
mod dictation;
#[cfg(target_os = "macos")]
mod dictation_app;

/// The live UI sink: forwards each spine turn event to the webview as a Tauri event.
/// This is the ONLY place spine events become UI events — clean output is guaranteed by
/// the spine (assistant text only), so this layer just relays name + payload verbatim.
struct TauriEmitter {
    app: AppHandle,
}

impl TurnObserver for TauriEmitter {
    fn on_event(&self, event: &StreamEvent) {
        // Best-effort: a dropped/absent webview never affects the turn (ledger is truth).
        let _ = emission_trace::emit(
            event,
            hop_trace_is_on(),
            || self.app.emit(event.event_name(), event.payload()),
            |receipt| eprintln!("[richos] ui-emit {receipt}"),
        );
        if let StreamEvent::TurnCompleted { thread_id, turn_id, .. } = event {
            if let Some(runtime) = self.app.try_state::<Arc<phone::PhoneRuntime>>() {
                let runtime=Arc::clone(&runtime);
                let (thread_id,turn_id)=(thread_id.clone(),turn_id.clone());
                tauri::async_runtime::spawn_blocking(move || {
                    runtime.bridge_refresh();
                    runtime.push_reply(&thread_id,&turn_id);
                });
            }
        }
    }
}

/// The SPOKEN-CORRECTION sink (`staging.rs`): forwards §7's ask to the webview on
/// `rich://correction-staged`.
///
/// A FOURTH observer, separate from the other three for the reason the machinery sink is
/// separate: a surface's subscription list is the proof of what it carries, and the calm
/// conversation view must be able to ignore this entirely. Best-effort, like the others —
/// the question is already durable on disk before this runs, so a webview that is not
/// listening loses a prompt and never a record.
struct TauriCorrectionEmitter {
    app: AppHandle,
}

impl CorrectionObserver for TauriCorrectionEmitter {
    fn on_correction_staged(&self, staged: &Staged) {
        let _ = self.app.emit(EVENT_CORRECTION_STAGED, staged);
    }
}

/// The LORO-PROPOSAL sink (`belief.rs` -> `correction.rs`): forwards a filed proposal to
/// the webview on `rich://loro-proposed`.
///
/// A FIFTH observer, and a separate event from `rich://correction-staged` because the two
/// carry different payloads and a subscription list is the proof of what a surface renders.
/// Best-effort like the others: the proposal is durable on the desk's own log before this
/// runs, so a webview that is not listening loses a badge update and never a record.
struct TauriProposalEmitter {
    app: AppHandle,
}

impl ProposalObserver for TauriProposalEmitter {
    fn on_correction_proposed(&self, proposal: &Proposal) {
        let _ = self.app.emit(EVENT_LORO_PROPOSED, proposal);
    }
}

/// The MACHINERY sink: forwards each machinery record to the webview on ONE event name,
/// `rich://machinery` (techy-mode design §1.3 — one name plus a `kind` field, because the
/// agent's frame set is the vendor's and open, so a new kind must not need a new event).
///
/// A SEPARATE observer from `TauriEmitter` on purpose. The clean-output invariant is
/// structural, not a convention: machinery is not a `StreamEvent`, so a webview that
/// subscribes only to the four calm events cannot receive it. The default conversation
/// view does not subscribe to this event and must not (§3.3) — see `app/STREAMING.md`.
struct TauriMachineryEmitter {
    app: AppHandle,
}

impl MachineryObserver for TauriMachineryEmitter {
    fn on_machinery(&self, record: &MachineryRecord) {
        // Best-effort, and weaker than the ledger by design (§2.2): a webview that is not
        // listening never stalls or fails a turn, and machinery is not truth.
        let _ = self.app.emit(EVENT_MACHINERY, record.event_payload());
    }
}

/// `rich://output` (Output side panel PRD §6.6): a thread's output record gained files.
/// Emitted after rows are appended, by the spine and by the work host through the ONE store.
struct TauriOutputEmitter {
    app: AppHandle,
}

impl richos_core::output::OutputObserver for TauriOutputEmitter {
    fn on_output(&self, thread_id: &str, added: &[richos_core::output::Entry], count: usize) {
        // Best-effort, like machinery: the record on disk is what a reopened panel reads.
        if let Err(e) = self.app.emit(richos_core::output::EVENT_OUTPUT, richos_core::output::event_payload(thread_id, added, count)) {
            eprintln!("[richos] output: the list could not be told about new files: {e}");
        }
    }
}

/// **The push half of the background-work return path** (spec §3.4).
///
/// The UI's three-second work poll is gated on `mainView === "conversation" &&
/// !document.hidden` (`ui/main.js`), so a result that lands while he is elsewhere would
/// otherwise wait for him to come back and look. This is how it arrives when he IS there.
///
/// **It is best-effort and nothing depends on it.** The durable record is the assignment's
/// own notice, held until delivered (`assignment.rs`), and the surface reads it at launch
/// as well — which is what makes a result survive him not being there at all, and is why
/// dropping this event costs a moment rather than a message.
///
/// The event name follows `rich://voice-notice`, the existing precedent for a line that
/// arrives from outside a turn and lands on the calm timeline (spec §3.8).
/// **`pub` so the documentation gate can SEE it.** `ui/tests/docs-claims.js` builds its
/// inventory from `pub const NAME: &str = "rich://…"` and asserts `app/STREAMING.md`
/// documents every one. A private constant here would have been a shipped event outside
/// that inventory — declared, emitted, and invisible to the check that exists to stop
/// exactly that. Nothing else reads it; the visibility is the gate.
pub const EVENT_WORK_NOTICE: &str = "rich://work-notice";

/// **His team said something** — an operator install's push lane (the operator-client record's
/// §7 item 1). Best-effort like `rich://work-notice`: the notice is on disk before this is
/// emitted, and `take_operator_notices` reads the durable copy. `pub` for the documentation gate.
pub const EVENT_OPERATOR_NOTICE: &str = "rich://operator-notice";

/// **One request to the PINNED engine's continuity store**, the one the register opens an
/// assignment's obligation in (`assignment_tools.rs`), through a bridge made on first use and
/// kept. Closing an obligation goes through it: his team's settlement on an operator install,
/// and an assignment the product's back end handled itself (`work_host::AnsweredClose`).
fn engine_store_call(engine_cell: Arc<Mutex<PathBuf>>, data_dir: &Path) -> richos_core::operator_runtime::EcsCall {
    let bridge: Arc<Mutex<Option<richos_core::ecs::EcsBridge>>> = Arc::new(Mutex::new(None));
    let ecs_state = data_dir.join("ecs");
    Box::new(move |command, fields| {
        let made = {
            let mut held = bridge.lock().map_err(|_| "the engine's store could not be reached".to_string())?;
            if held.is_none() {
                let dir = engine_cell.lock().map(|d| d.clone()).map_err(|_| "the engine could not be located".to_string())?;
                let runtime = richos_core::runtime::verify_engine(&dir).map_err(|e| e.to_string())?;
                *held = Some(richos_core::ecs::EcsBridge::new(&runtime.python, &dir, &ecs_state).map_err(|e| e.0)?);
            }
            held.clone().expect("made above")
        };
        made.request(command, fields).map_err(|e| e.0)
    })
}

/// **His team's desk, built once at launch on an operator install** (the operator-client
/// record's §7 items 1-3). Returns the desk, the socket its front-desk tools reach it through,
/// and the access the front desk's lease is given. A socket that cannot be served costs the
/// front desk its three tools and is said on the boot log; the desk itself still takes work.
/// **THE SECOND REVIEW STARTS BY ITSELF, IN THE APP** (automatic second review, slice 4;
/// richos-hq `docs/plans/2026-10-09-automatic-second-review-and-t3-ideas.md` §4 row 4 and Sage's
/// check beside it). The engine's review-watch runs as this host's own child
/// (`richos_core::review_watch`): mid-job reviews of the app's running workers, whose verdicts
/// reach the worker through the app's hook. Started at launch when the engine verifies, and
/// again (a no-op while it runs) whenever a work lease opens, which covers an engine first-run
/// setup installs after launch. Ended by the quit path. Quiet when already running.
fn ensure_app_review_watch(engine: &Path, data_dir: &Path, claude: &Path, at_launch: bool) {
    let runtime = match richos_core::runtime::verify_engine(engine) {
        Ok(runtime) => runtime,
        Err(e) => {
            if at_launch {
                eprintln!("[richos] second review: not watching yet, the engine is not verified ({e})");
            }
            return;
        }
    };
    let launch = richos_core::review_watch::Launch {
        python: runtime.python.clone(),
        engine: engine.to_path_buf(),
        state: data_dir.join("engine-state"),
        // Codex is looked for where the Settings row looks (codex_reviews::search_path), so the
        // reviewer the row shows is the one the reviews run (the real second review of cf3c4482f).
        environment: richos_core::review_watch::app_environment(&runtime.path(), &richos_core::codex_reviews::search_path()),
        mode: richos_core::review_watch::Mode::App {
            claude: claude.is_absolute().then(|| claude.to_path_buf()),
            // The list `quota::Service` opens from this same folder: each review runs on the
            // account in use when its reviewer starts, as a work lease does.
            accounts: data_dir.join(richos_core::claude_accounts::LIST_FILE),
            // The Settings switch "Let Codex review your team's work" (round 20.2), which the
            // row's commands below write: read by the watcher when each review starts.
            codex_reviews: data_dir.join(richos_core::codex_reviews::SETTING_FILE),
        },
    };
    match richos_core::review_watch::ensure(&launch) {
        Ok(pid) if at_launch => eprintln!("[richos] second review: monitoring started (process {pid})"),
        Ok(_) => {}
        Err(e) => eprintln!("[richos] second review: the app's review-watch could not start ({e})"),
    }
}

/// **His team's install: the same watcher over his team's registry, its notices delivered by
/// this host** (Sage's check §1.3). Each notice goes to the lead whose session started the work,
/// as a message the host sends ([`richos_core::operator_desk::OperatorDesk::tell_lead`]), so the
/// turn it starts is one the host can attribute.
fn ensure_operator_review_watch(desk: &Arc<richos_core::operator_desk::OperatorDesk>, data_dir: &Path) {
    let declaration = desk.declaration().clone();
    let weak = Arc::downgrade(desk);
    // True only once the lead was told: the watcher records a verdict delivered on that
    // acknowledgment alone, and tells a refused one again at its next look.
    let sink: richos_core::review_watch::NoticeSink = Arc::new(move |notice: richos_core::review_watch::Notice| {
        let Some(desk) = weak.upgrade() else { return false };
        match desk.tell_lead(&notice.session, &notice.text) {
            Ok(_) => true,
            Err(e) => {
                eprintln!("[richos] his team: a second-review notice was not delivered ({e}); it is told again at the next look");
                false
            }
        }
    });
    let launch = richos_core::review_watch::Launch {
        python: PathBuf::from("python3"),
        engine: declaration.engine_root.clone(),
        state: data_dir.join("engine-state"),
        environment: declaration.environment.clone(),
        mode: richos_core::review_watch::Mode::Operator {
            config: declaration.entity_root.join("orchestration.config"),
            sink,
        },
    };
    match richos_core::review_watch::ensure(&launch) {
        Ok(pid) => eprintln!("[richos] his team: the second review's watcher runs as pid {pid}"),
        Err(e) => eprintln!("[richos] his team: the second review's watcher could not start ({e})"),
    }
}

/// **His team's install: quota-watch, stall-watch and codex-watch, run the same way**
/// (`richos_core::team_watch`): the host's own children from launch to quit, each notice sent
/// to his team's lead as a message the host sends
/// ([`richos_core::operator_desk::OperatorDesk::tell_team`]), or kept until a lead starts.
fn ensure_operator_team_watch(desk: &Arc<richos_core::operator_desk::OperatorDesk>, data_dir: &Path) {
    let declaration = desk.declaration().clone();
    let weak = Arc::downgrade(desk);
    let sink: richos_core::team_watch::NoticeSink = Arc::new(move |watcher: richos_core::team_watch::Watcher, text: String| {
        let Some(desk) = weak.upgrade() else {
            eprintln!("[richos] his team: a {} notice came after his team was closed", watcher.name());
            return;
        };
        if let Err(e) = desk.tell_team(&text) {
            eprintln!("[richos] his team: a {} notice was neither delivered nor kept ({e})", watcher.name());
        }
    });
    let launch = richos_core::team_watch::Launch {
        engine: declaration.engine_root.clone(),
        entity: declaration.entity_root.clone(),
        state: data_dir.join("engine-state"),
        lead: desk.team_watch_lead(),
        environment: richos_core::operator_declaration::script_environment(&declaration),
        sink,
    };
    for watcher in richos_core::team_watch::Watcher::ALL {
        match richos_core::team_watch::ensure(&launch, watcher) {
            Ok(pid) => eprintln!("[richos] his team: {} runs as pid {pid}", watcher.name()),
            Err(e) => eprintln!("[richos] his team: {} could not start ({e})", watcher.name()),
        }
    }
}

fn operator_desk_at_boot(
    declaration: richos_core::operator_declaration::Declaration,
    data_dir: &Path,
    app: AppHandle,
    engine_cell: Arc<Mutex<PathBuf>>,
) -> (
    Option<Arc<richos_core::operator_desk::OperatorDesk>>,
    Option<richos_core::operator_desk_tools::DeskSocket>,
    Option<richos_core::operator_desk_tools::DeskAccess>,
) {
    let push: richos_core::operator_runtime::NoticePush = Box::new(move |key, notice| {
        // Best-effort, like rich://work-notice: the notice is on disk before this runs.
        if let Err(e) = app.emit(EVENT_OPERATOR_NOTICE, serde_json::json!({"threadId": key.thread_id, "notice": notice})) {
            eprintln!("[richos] his team: a notice could not be pushed to the window ({e}); it is held on disk");
        }
    });
    let settle = Arc::new(richos_core::operator_runtime::EcsSettle::with(engine_store_call(engine_cell, data_dir)));
    let origins = Arc::new(richos_core::operator_desk::LedgerOrigins::new(&data_dir.join("conversation-ledger.jsonl")));
    let executable = std::env::current_exe().unwrap_or_else(|e| {
        eprintln!("[richos] his team: this app's own path could not be read ({e}); his leads' report tool will not start");
        PathBuf::from("/nonexistent/RichOS")
    });
    // (h): the lead's `can_use_tool` has nowhere to go but a refusal until the permission desk
    // route is built; in the declared bypass mode P14 measured nothing reaching it at all.
    let listed = declaration.origins.clone();
    let token = richos_core::operator_desk_tools::new_token();
    // F6: the leads' own token, good for a named stop and nothing else, so a stop he gives to
    // whichever lead is in front of him is executed there, once.
    let lead_token = richos_core::operator_desk_tools::new_token();
    let path = richos_core::operator_desk_tools::socket_path(&data_dir.join("operator"));
    let lead_desk = richos_core::operator_report::LeadDesk { socket: path.clone(), token: lead_token.clone() };
    let desk = richos_core::operator_desk::OperatorDesk::for_app(declaration, data_dir, &executable, settle, origins, Some(push),
                                                                 Arc::new(richos_core::operator_lead::NoPermissionDesk),
                                                                 Some(lead_desk));
    desk.start_default_retirement();
    eprintln!("[richos] his team: operator install; each conversation's lead starts on its next assignment (log {})",
              desk.log_path().display());
    match richos_core::operator_desk_tools::DeskSocket::serve_with_lead(desk.clone(), &path, &token, Some(&lead_token)) {
        Ok(socket) => (Some(desk), Some(socket),
                       Some(richos_core::operator_desk_tools::DeskAccess { socket: path, token, origins: listed })),
        Err(e) => {
            eprintln!("[richos] his team: the front desk's stop and read could not be offered ({e}) at {}", path.display());
            (Some(desk), None, None)
        }
    }
}

struct WorkNotice {
    app: AppHandle,
}

impl richos_core::work_host::WorkNotifier for WorkNotice {
    fn raised(&self, thread_id: &str, notice: &richos_core::assignment::PendingNotice) {
        let _ = self.app.emit(
            EVENT_WORK_NOTICE,
            serde_json::json!({"threadId": thread_id, "notice": notice}),
        );
    }

    /// Stu's overload wait reaches his conversation the way the app's quota notices do, as the
    /// app's own line (`Spine::raise_proactive`), on its own thread so the work host never waits
    /// for a turn that holds the spine.
    fn overload_retry(&self, thread_id: &str, sentence: &str) {
        let (app, thread, sentence) = (self.app.clone(), thread_id.to_string(), sentence.to_string());
        std::thread::spawn(move || {
            let Some(state) = app.try_state::<AppState>() else { return };
            let shown = take_the_spine(&state.spine).raise_proactive(Some(&thread), AttentionTier::Digest, &sentence);
            if let Err(error) = shown {
                eprintln!("[richos] the back end's overload wait could not be shown ({error})");
            }
        });
    }


    /// **§2.4a — the settle-while-closed end state.** The last registered assignment has
    /// ended and no window is open, so the app quits itself.
    ///
    /// **Why anything has to act at all:** `ExitRequested` is raised only when the last
    /// window is destroyed (`tauri-runtime-wry-2.11.4/src/lib.rs:4310-4316`), and nothing
    /// re-raises it when work finishes. Without this the app would sit there indefinitely —
    /// *"a background process the user did not ask for"*, which §2.4's own rule refuses.
    ///
    /// **The window half is decided HERE and not in the host**, because only the shell can
    /// see a window. With one open, this does nothing: an app he is looking at does not
    /// close itself because a background job finished.
    ///
    /// It exits through `app.exit(0)`, the preventable path, so the same arm that decided
    /// "work is registered, prevent" decides "nothing is registered, allow" — one decision,
    /// one place. The notice he finds on his next launch is the durable one already written
    /// on the assignment (§3.4), which is what makes this safe to do while he is away.
    fn nothing_left_to_do(&self) {
        if !self.app.webview_windows().is_empty() {
            return;
        }
        if self.app.try_state::<AppState>().is_some_and(|state| questions_pending(&state)
            || state.control.has_active_turns() || state.work.peer_work_pending() || !state.control.pending_intake().is_empty()) {
            return;
        }
        // **A command Rich started outlives its assignment** (reap gap C5): quitting would end
        // it, so the app stays until the command ends, then closes itself exactly as below.
        // Read from each lease's supervisor state (a positive reading, once every 5 s), never
        // inferred from silence; one waiter at a time.
        static WAITING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let commands = || richos_core::lease_commands::LiveLeases::process().reading();
        let busy = |reading: richos_core::lease_commands::CommandsReading| reading.running > 0 || reading.unreadable > 0;
        if busy(commands()) {
            if WAITING.swap(true, std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            eprintln!("[richos] the last assignment ended with no window open and a command Rich started still running: RichOS stays until it ends.");
            let app = self.app.clone();
            std::thread::spawn(move || {
                while busy(commands()) {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                }
                WAITING.store(false, std::sync::atomic::Ordering::SeqCst);
                let quiet = app.try_state::<AppState>().is_some_and(|state| !registered_work(&state).anything() && !questions_pending(&state));
                if app.webview_windows().is_empty() && quiet {
                    eprintln!("[richos] the last command Rich started has ended with no window open: RichOS is closing itself.");
                    app.exit(0);
                }
            });
            return;
        }
        eprintln!("[richos] the last assignment ended with no window open: RichOS is closing itself.");
        self.app.exit(0);
    }
}

/// The rotation/recovery seam (richos_core::LeaseFactory): spawns a fresh, un-primed
/// lease exactly like the boot path (`NativeCognition::start`), so the spine can rotate at
/// a context watermark or recover from a mid-turn crash without knowing anything about the
/// wire — richos-core stays IO-agnostic (continuity §3.3 step 4).
///
/// **BOTH FIELDS MOVE, and that is deliberate.** Until 2026-09-01 `engine_dir` was a `PathBuf`
/// fixed at boot, which was correct while nothing could put an engine on the machine.
/// `setup_view::run` now can (`setup.rs`, Option D), so the directory a lease is started in has
/// to be able to change without a relaunch — the same property `provision_memory` already
/// establishes for the corpus, and for the same reason: a customer who has just watched
/// something be installed must not be told to quit and reopen before it works.
///
/// **`claude_bin` JOINED IT ON 2026-09-04, and the reason is the same defect one layer down.**
/// It was resolved once in `setup` and frozen here. On a fresh install that resolution happens
/// BEFORE first-run setup, when `~/.local/bin/claude` does not exist yet, so
/// [`resolve_claude_bin`] falls through to the bare name `claude` and this factory keeps it for
/// the life of the process. A Finder launch's `PATH` is `/usr/bin:/bin:/usr/sbin:/sbin` — it
/// does not contain `~/.local/bin` and never will — so every rotation and every crash recovery
/// in that customer's whole first session would have failed to spawn, on a machine whose
/// `claude` was installed, working, and being driven successfully by the lease `run_setup`
/// attached with a freshly resolved path.
///
/// That is the same shape as the `lease_ready` snapshot fixed earlier the same day: an answer
/// cached before the thing it describes existed. `run_setup` writes both cells.
struct EngineLeaseFactory {
    quota: Arc<richos_core::quota::Service>,
    claude_bin: Arc<Mutex<PathBuf>>,
    engine_dir: Arc<Mutex<PathBuf>>,
    /// Where RichOS keeps its own files — `app_data_dir()`, the same directory the ledger,
    /// `config.json` and `entities.json` live in.
    ///
    /// **Not a rendered path frozen at boot, and that distinction is the point.** The standing
    /// instruction is re-rendered and re-verified on EVERY lease this factory spawns
    /// (`doctrine::ensure_rendered`), so a doctrine that was edited, truncated or deleted since
    /// the last rotation is replaced before the successor sees it, and a CEO who has just typed
    /// his name into settings is addressed by it at the next rotation rather than at the next
    /// relaunch. A cached path would be an answer computed before the thing it describes could
    /// change — the defect `claude_bin` above joined this struct to fix.
    ///
    /// **The CEO's name is read from `config.json` ON DISK, not from `AppState::config`, and
    /// that is deadlock avoidance rather than a shortcut.** This factory is called from inside
    /// the spine's mutex (rotation and crash recovery happen mid-turn), and the lock order
    /// stated three times in this file is config → registry → entity → spine. Taking the config
    /// lock here would take two of them in the opposite order. `ConfigStore::open` reads and
    /// never writes (`config.rs`), and `set_user_name` persists immediately, so the file is as
    /// current as the lock would have been.
    data_dir: PathBuf,
    /// **Did an operator NAME this engine?** `$RICHOS_ENGINE_DIR` / `$RICHOS_ENGINE_ROOT`, read
    /// once at boot with everything else the launch is made of.
    ///
    /// It is here because the release gate in `spawn_chat` honors the same rule the resolver
    /// does (`locate-engine.sh` rule 1): a named directory is a statement, and a gate that
    /// refused what the operator named would overrule it. Read at boot rather than in
    /// `spawn_chat` because a lease is spawned mid-turn, from inside the spine's mutex, and
    /// reading global state there is what this file keeps off the hot path on purpose.
    explicit_engine: bool,
    /// **His team's desk, for the front desk's `richos_operator` tools** — `Some` only on an
    /// install whose `operator.json` passed the gate at launch (the operator-client record's
    /// §7 item 2). Given to the CONVERSATION lease only (`spawn_chat`); `None` everywhere else,
    /// which leaves the front desk's tools exactly what they were.
    operator_desk: Option<richos_core::operator_desk_tools::DeskAccess>,
}

impl LeaseFactory for EngineLeaseFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        self.spawn_chat(None, None)
    }
    fn spawn_scoped(&self, binding: &richos_core::entity::ThreadBinding, control: &ConversationControl) -> Result<Box<dyn Cognition>, CognitionError> {
        self.spawn_chat(Some(control), Some(binding))
    }
    fn spawn_cancellable(&self, control: &ConversationControl) -> Result<Box<dyn Cognition>, CognitionError> {
        self.spawn_chat(Some(control), None)
    }



    /// Fill-first: the spine reads the accounts and the freshest readings through this.
    fn quota(&self) -> Option<Arc<richos_core::quota::Service>> {
        Some(self.quota.clone())
    }

    /// **The team the next work lease would register** (proto-teammate shelf slice 5): the
    /// same engine folder and app data `spawn_work` hands `EngineProfile::prepare`, read with
    /// the same function it registers from. A folder that cannot be read is `None`, which
    /// renews nothing.
    fn team_digest(&self) -> Option<String> {
        let engine = self.engine_dir.lock().ok()?.clone();
        richos_core::engine_profile::team_digest(&engine, &self.data_dir).ok()
    }

    /// **The models the next leases would start on** (§119): what `EngineProfile::prepare`
    /// reads at every spawn, read the same way — off `config.json` on disk, never the config
    /// lock, for the deadlock reason on `data_dir` above. The spine and the work host compare
    /// it with the lease in the chair at each boundary.
    fn lease_models(&self) -> Option<richos_core::config::LeaseModels> {
        Some(richos_core::config::lease_models_from_config(&self.data_dir))
    }

    /// **The user's own settings files as the next lease would read them** (build plan item 0):
    /// the central folder `EngineProfile::prepare` reads, digested the same way. The spine and
    /// the work host compare it with the lease in the chair at each boundary.
    fn user_settings_digest(&self) -> Option<String> {
        richos_core::company::install_central_root().map(|folder| richos_core::user_settings::digest(&folder))
    }

    /// **The second lease** — the background-work spec §2.1's work lease, in this same
    /// process, owned by the work host.
    ///
    /// It goes through the same release gate, the same runtime verification and the same
    /// engine profile as the conversation's, because a work lease that skipped any of them
    /// would be the wrong engine writing into the CEO's corpus with fewer eyes on it, not
    /// more. What it does NOT get is a `TurnControl` — §4.2: the work lease is never
    /// attached to the conversation's cancel slot, and the shape of `spawn_work` is where
    /// that is enforced, because there is nothing here to pass one through.
    ///
    /// **A refusal here is a failed registration** (spec §1.4 and §2.1): the gate can
    /// refuse before any lease exists, and that has nothing to do with the work. The work
    /// host reports it in those words rather than leaving it to surface later as a worker
    /// that never speaks.
    fn spawn_work(&self, binding: &richos_core::entity::ThreadBinding) -> Result<Box<dyn Cognition>, CognitionError> {
        // **THE OPERATOR GATE** (operator back-end spec r2 (f); CEO ruling §86). No
        // `operator.json` in this install's data folder is the product path, byte for byte
        // what ran before this line existed. A present file never falls back to the customer
        // worker (Frank's B8): broken refuses with its one sentence.
        //
        // **A valid one never reaches this far on an install that launched with it.** His
        // team's desk is built at launch (`operator_desk_at_boot`) and the work host hands it
        // every assignment (`WorkHost::set_operator`), so no work lease is ever asked for. This
        // arm is reached only by a declaration that appeared AFTER launch, and it refuses with
        // the sentence that says so rather than half-honoring it.
        match richos_core::operator_declaration::gate(&self.data_dir) {
            richos_core::operator_declaration::Gate::Product => {}
            richos_core::operator_declaration::Gate::Refused(refusal) => {
                return Err(CognitionError::Io(refusal.sentence()));
            }
            richos_core::operator_declaration::Gate::Operator(_) => {
                return Err(CognitionError::Io(richos_core::operator_declaration::SWITCHED_ON_AFTER_LAUNCH.to_string()));
            }
        }
        let dir = self
            .engine_dir
            .lock()
            .map(|d| d.clone())
            .unwrap_or_else(|_| PathBuf::from("/nonexistent/richos-engine"));
        let bin = self
            .claude_bin
            .lock()
            .map(|b| b.clone())
            .unwrap_or_else(|_| PathBuf::from("claude"));
        let skills = dir.join("app");
        let doctrine = skills.join("agents/stu.md");
        let executable = std::env::current_exe().map_err(|e| CognitionError::Io(e.to_string()))?;
        // **AND THE CONTENT, not just the release** (2026-09-18). `EngineDemand::pinned` carries
        // the digest of the asset this build was built against, so the directory a stale nightly
        // left behind is refused HERE as well as at resolution — resolution refusing is not the
        // same as nothing running, since `resolve_engine` hands this factory the last place it
        // looked. An operator's explicit statement still outranks both halves.
        if let Some(why) = richos_core::setup::engine_boot_refusal_demand(
            &dir,
            self.explicit_engine,
            richos_core::setup::EngineDemand::pinned(richos_core::setup::engine_pin().as_ref()),
        ) {
            return Err(CognitionError::Io(why));
        }
        let runtime = richos_core::runtime::verify_engine(&dir)
            .map_err(|e| CognitionError::Io(e.to_string()))?;
        // The second review's watcher, if launch could not start it (an engine set up since).
        ensure_app_review_watch(&dir, &self.data_dir, &bin, false);
        let mut profile = richos_core::engine_profile::EngineProfile::prepare(&dir, &self.data_dir, runtime.clone())
            .map_err(|e| CognitionError::Io(e.to_string()))?;
        profile.scope_to(binding).map_err(|e| CognitionError::Io(e.to_string()))?;
        profile.install_quota_gate(&executable).map_err(|e| CognitionError::Io(e.to_string()))?;

        // Fill-first: the work lease runs under the Claude account in use now.
        profile.claude_account = Some(self.quota.lease_account());
        profile.token_sink = Some(self.quota.token_sink()); // what the lease spends, counted on that account
        profile.speech_model = speech_model_for_rich();
        let bridge = richos_core::ecs::EcsBridge::new(&runtime.python, &dir, &self.data_dir.join("ecs"))
            .map_err(|e| CognitionError::Io(e.to_string()))?;
        self.quota.request_refresh();
        self.quota.lease_started(); // Several at once: an expected rise (fill-first).
        // Stu starts from the user's own `stu.md` (`user_settings.rs`), read now.
        let skills = profile.standing_plugin(&skills);
        let doctrine = doctrine.file_name().map(|name| skills.join("agents").join(name)).unwrap_or(doctrine);
        let cog = richos_core::native::NativeCognition::start_work_lease(&bin, &doctrine, &skills, &executable, bridge, profile)?;
        Ok(Box::new(cog))
    }
}

impl EngineLeaseFactory {
    fn spawn_chat(&self, control: Option<&ConversationControl>, binding: Option<&richos_core::entity::ThreadBinding>) -> Result<Box<dyn Cognition>, CognitionError> {
        let dir = self
            .engine_dir
            .lock()
            .map(|d| d.clone())
            // A poisoned lock is not a reason to guess a directory. `/nonexistent/…` is the
            // same sentinel the boot uses, and `native.rs::preflight` reports it as exactly
            // what it is: a working directory that does not exist, NOT a missing binary.
            .unwrap_or_else(|_| PathBuf::from("/nonexistent/richos-engine"));
        // Same rule for the binary: a poisoned lock falls back to the bare name, which is
        // exactly what an unresolvable machine gets at boot, and `native.rs::preflight`
        // leaves a bare name to the spawn where `ENOENT` is unambiguous because the working
        // directory above has already been cleared.
        let bin = self
            .claude_bin
            .lock()
            .map(|b| b.clone())
            .unwrap_or_else(|_| PathBuf::from("claude"));
        // The standing instruction, rendered and verified before the successor exists. A
        // failure here is a failure to spawn: a lease with no doctrine is a generic Claude
        // wearing the product's window, and `doctrine.rs` has no fallback for that on purpose.
        let skills = dir.join("app");
        let doctrine = skills.join("agents/rich.md");
        let executable = std::env::current_exe().map_err(|e| CognitionError::Io(e.to_string()))?;
        // **THE RELEASE GATE — spec point 22, and the half of it that stops the wrong engine
        // WRITING.** A refused resolution does not end the launch: `resolve_engine` hands the
        // lease factory the last place it looked, and on macOS that is
        // `~/Library/Application Support/RichOS/engine` — the one directory RichOS writes, and
        // exactly where a newer engine sits after the app has been rolled back. Everything
        // below this line would then succeed, and the engine the CEO rolled away from would be
        // the one writing into his corpus. An operator who NAMED the directory is honored, as
        // everywhere else (`setup::engine_boot_refusal`).
        // **AND THE CONTENT, not just the release** (2026-09-18). `EngineDemand::pinned` carries
        // the digest of the asset this build was built against, so the directory a stale nightly
        // left behind is refused HERE as well as at resolution — resolution refusing is not the
        // same as nothing running, since `resolve_engine` hands this factory the last place it
        // looked. An operator's explicit statement still outranks both halves.
        if let Some(why) = richos_core::setup::engine_boot_refusal_demand(
            &dir,
            self.explicit_engine,
            richos_core::setup::EngineDemand::pinned(richos_core::setup::engine_pin().as_ref()),
        ) {
            return Err(CognitionError::Io(why));
        }
        let runtime = richos_core::runtime::verify_engine(&dir)
            .map_err(|e| CognitionError::Io(e.to_string()))?;
        let mut profile = richos_core::engine_profile::EngineProfile::prepare(&dir, &self.data_dir, runtime.clone())
            .map_err(|e| CognitionError::Io(e.to_string()))?;
        if let Some(binding) = binding { profile.scope_to(binding).map_err(|e| CognitionError::Io(e.to_string()))?; }
        profile.install_quota_gate(&executable).map_err(|e| CognitionError::Io(e.to_string()))?;

        profile.operator_desk = self.operator_desk.clone();
        // Fill-first: the conversation lease runs under the Claude account in use now.
        profile.claude_account = Some(self.quota.lease_account());
        profile.token_sink = Some(self.quota.token_sink()); // what the lease spends, counted on that account
        profile.speech_model = speech_model_for_rich();
        let bridge = richos_core::ecs::EcsBridge::new(&runtime.python, &dir, &self.data_dir.join("ecs"))
            .map_err(|e| CognitionError::Io(e.to_string()))?;
        self.quota.request_refresh();
        self.quota.lease_started(); // Several at once: an expected rise (fill-first).
        // Rich starts from the user's own `rich.md` (`user_settings.rs`), read now.
        let skills = profile.standing_plugin(&skills);
        let doctrine = doctrine.file_name().map(|name| skills.join("agents").join(name)).unwrap_or(doctrine);
        let cog = NativeCognition::start_with_engine(&bin, &doctrine, &skills, &executable, bridge, profile, control)?;
        Ok(Box::new(cog))
    }
}

/// The durable Rich, guarded for cross-invocation access. `Spine` is `Send` (its
/// compute lease is `Box<dyn Cognition + Send>`), so `Mutex<Spine>` is valid Tauri state.
struct AppState {
    quota: Arc<richos_core::quota::Service>,

    /// **The second compute lease and the actor that owns it** — the background-work spec
    /// §2.1's work host.
    ///
    /// It is beside the spine rather than inside it, and it holds no reference to it: a
    /// background assignment must not be able to take the mutex `send_message` holds for
    /// the whole of a turn (`:561`, still held at `:582`), which is the whole of §0 row 3.
    /// Read WITHOUT that lock everywhere it is read, like `control`.
    work: Arc<richos_core::work_host::WorkHost>,
    /// **His team, on an operator install** (the operator-client record's §7 item 1): the desk
    /// that relays each assignment to its conversation's lead. `None` on every product install,
    /// and decided once, at launch.
    operator: Option<Arc<richos_core::operator_desk::OperatorDesk>>,
    /// The socket the front desk's `richos_operator` tools reach that desk through. Kept here
    /// so it lives exactly as long as the app, and closed at exit.
    operator_socket: Option<richos_core::operator_desk_tools::DeskSocket>,
    /// **He has been asked about the running work and said quit** (background-work spec
    /// §2.5a). The `ExitRequested` arm reads it on the second pass; without it the same arm
    /// would prevent the same quit for ever.
    ///
    /// An `AtomicBool` rather than a `Mutex` because it is read INSIDE the exit callback,
    /// whose answer the runtime takes with `try_recv` the instant that callback returns
    /// (`tauri-runtime-wry-2.11.4/src/lib.rs:4318`): a lock that happened to be held by the
    /// thread asking the question would make the read block and the app quit.
    quit_confirmed: std::sync::atomic::AtomicBool,
    /// Whether this launch has a Dock icon to come back through — `Regular`, not
    /// `Accessory` (`activation.rs:1-18, 304-314`). Fixed for the life of the process,
    /// like the decision itself.
    can_come_back: bool,
    provider_auth: Mutex<richos_core::provider_auth::ProviderAuth>,
    /// Fill-first: the sign-in of an added Claude account, while it runs (account id, login).
    account_auth: Mutex<Option<(String, richos_core::provider_auth::ProviderAuth)>>,
    /// **The thread whose front desk this launch has already asked to be made ready** — the
    /// CEO's §55, and the one-shot that keeps it to one attempt per thread opened.
    ///
    /// The timeline read that opens a thread is also polled, so without this a poll would spawn
    /// a thread per call to ask a question whose answer is already `AlreadyReady`. It holds the
    /// thread id rather than a bool because he has more than one conversation and each has its
    /// own desk (`spine.rs`'s residency).
    front_desk_primed_for: Mutex<std::collections::HashSet<String>>,
    spine: Mutex<Spine>,
    reader: SpineReader,
    /// Durable CEO-facing preferences (company name, the assertiveness dial) — stored
    /// alongside the ledger in the app data dir, same durability posture.
    config: Mutex<ConfigStore>,
    /// The entity area this launch is bound to (ECS §3.3). `None` fails closed: threads
    /// cannot be created and the loro desk is scoped to nothing, rather than defaulting to
    /// an entity nobody chose.
    ///
    /// **BEHIND A MUTEX SINCE SLICE 4, and that is the whole shape of the fix.** It used to
    /// be a plain `Option` fixed at boot, with a comment saying the CEO-facing picker was
    /// slice 4 — which meant a launch that could not resolve a root stayed unresolved for
    /// its whole life, and a double-clicked bundle (working directory `/`) can never
    /// resolve one. The picker writes here, at runtime, so answering the question does not
    /// cost a relaunch.
    ///
    /// It is NOT inside `config`'s mutex even though `choose_entity` writes both: this is
    /// read on the `create_thread` and loro paths, and `config` is held by the settings
    /// surface. Lock order everywhere is config, then entity, then spine.
    entity: Mutex<Option<EntityId>>,
    /// Where the company list's file is, resolved once at boot and carried — never
    /// re-derived, for the reason `data_dir` is carried rather than re-asked. The list itself
    /// has one copy, the spine's (`Spine::entity_registry`). Changes take the idle spine;
    /// reads take `reader.snapshot()`, which the spine republishes on every change and which
    /// takes no lock, so reading it never waits for Rich.
    registry_path: PathBuf,
    /// Whether the file was absent, read, or PRESENT AND UNREADABLE. The third is why this
    /// is carried at all: "you have not told me your companies yet" and "you told me and I
    /// could not read it" call for opposite responses, and a surface that answered the
    /// second with the first would invite somebody to re-enter a list already on disk one
    /// typo away from working.
    registry_source: RegistrySource,
    /// Whether `RICHOS_ENTITY` decided the answer. Fixed for the life of the process,
    /// because the variable is: nothing the CEO does in the window can change it, so the
    /// settings surface must render a statement rather than a control (§21's own rule —
    /// a state he cannot change has to say who can).
    entity_pinned_by_env: bool,
    /// Where the entity in force came from, so the settings surface can say so. Moves when
    /// `choose_entity` writes, which is why it is behind the same kind of lock as `entity`.
    entity_source: Mutex<Option<EntitySource>>,
    /// `<app-data>/machinery` — the Tier-B store the retention setting governs.
    ///
    /// Held here as a PATH rather than reached through `spine.machinery_journal()`
    /// deliberately: `send_message` holds the spine's mutex for the whole of a turn, so a
    /// settings command that locked it would sit there until Rich finished talking. A
    /// `MachineryJournal` is a path and nothing else, so building one per call costs
    /// nothing and the gear popover answers while a turn is running.
    machinery_root: PathBuf,
    /// Durable navigation view state (UX §3.1/§25: pin, rename, archive, rail width).
    /// Separate from `config` because it is view state, not a CEO preference about how
    /// Rich behaves — and separate from the ledger because it is not evidence (nav.rs).
    nav: Mutex<nav::NavStore>,
    /// THE CEO'S TWO MID-TURN CONTROLS (UX §9.2/§9.3), and the one thing here that is
    /// DELIBERATELY NOT behind `spine`'s mutex.
    ///
    /// `send_message` holds that mutex for the entire turn — `Spine::submit_prompt` takes
    /// `&mut self` and does not return until the lease is finished — so a stop command
    /// that locked the spine would fire only after the work it meant to interrupt had
    /// already ended. This is the same `Arc` the spine holds, reached without the lock.
    control: TurnControl,
    /// THE LORO CORRECTION DESK (open-items 3.5). Its writer comes from the SAME
    /// `LoroInstall` the read half was built from (`memory::wire_company_memory`), so the
    /// record the CEO is shown and the record his confirmation writes to cannot be two
    /// different corpora.
    ///
    /// **IT MOVES, and until 2026-09-01 it did not.** The field was a plain `Option` fixed
    /// at boot, so `provision_memory` re-wired the READ half into the running spine and
    /// could not re-wire this one: a genuinely fresh user answered "set my memory up", got a
    /// corpus, and then had a Rich that could read it and not correct it until he quit and
    /// reopened. That was printed rather than hidden, which was the right call for a limit
    /// nobody had time to fix — and it is not a limit, it is a `Mutex`. The CEO's own
    /// install never saw it (his corpus predates the app); every customer's first five
    /// minutes did.
    ///
    /// `None` when no corpus is configured, which is an ordinary install and not an error —
    /// the commands then say so in words instead of failing obscurely.
    ///
    /// TWO LOCKS, NEVER HELD TOGETHER AS A PAIR BY ANY READER. The outer `Mutex` guards
    /// WHETHER there is a desk; the inner one (inside `SharedCorrectionDesk`) guards the
    /// desk itself. [`desk`] takes the outer lock, clones the `Arc`, RELEASES it, and only
    /// then locks the desk — so a confirmation that takes ten seconds inside `loro-write`
    /// never blocks `loro_available`, and `provision_memory` can install a desk while one is
    /// being read.
    ///
    /// Behind its OWN mutex and not the spine's, for the same reason `control` is: reading
    /// what loro believes and confirming a correction are things the CEO does while Rich
    /// may be mid-turn, and `send_message` holds the spine lock for the whole of a turn.
    /// A correction UI that froze until Rich finished would be a UI nobody uses.
    correction: Mutex<Option<SharedCorrectionDesk>>,
    /// THE SPOKEN-CORRECTION DESK — the flywheel's automatic trigger (`spoken.rs` +
    /// `staging.rs`). The SAME `Arc` the spine holds, reached WITHOUT the spine lock, for
    /// exactly the reason `control` is: `send_message` holds the spine mutex for the whole
    /// of a turn, and answering "Add \"Kestrel\" to your vocabulary?" is something the CEO
    /// does WHILE Rich is working. A HUD that froze until the turn finished would be a HUD
    /// nobody answers, and an unanswered ask is a lost correction.
    ///
    /// `None` when the desk's own log could not be opened — the commands then say so
    /// rather than failing obscurely.
    spoken: Option<SharedCandidateDesk>,
    /// THE FEEDBACK CHANNEL'S LOCAL STORE (`feedback.rs`). One file beside the ledger, and
    /// that is the whole storage layer this feature has.
    ///
    /// Behind its OWN mutex and NOT the spine's, for the third time and the same reason:
    /// `send_message` holds the spine lock for the whole of a turn, and "how is RichOS
    /// doing this session?" is a question about a session that may still be running.
    ///
    /// `None` when the file could not be opened. The commands then REFUSE rather than
    /// pretend — asking the CEO what he thinks and dropping the answer on the floor is
    /// worse than saying the capability is not available.
    feedback: Option<Mutex<FeedbackStore>>,
    /// THE LAUNCH RECORD (`richos_core::launch`). What a start is, the log of starts, the
    /// recency ring, the install date and which rewards have fired.
    ///
    /// Behind its own mutex and NOT inside `config` for the reason `config.rs`'s own first
    /// paragraph gives: that file holds mutable point-in-time preference, and this is an
    /// append-only event log. They share a directory and a durability posture, nothing else.
    launch: Mutex<LaunchStore>,
    /// WHERE HIS MEMORY IS, AND WHETHER THIS INSTALL CAN READ IT (`memory.rs`).
    ///
    /// Behind its own mutex and NOT the spine's, for the fourth time and the same reason:
    /// `send_message` holds the spine lock for a whole turn, and "set my memory up" is
    /// something the CEO does before he has sent anything at all — but the surface that
    /// asks it also re-reads this after a turn may have started.
    ///
    /// It MOVES: `provision_memory` rewrites it after wiring a corpus that did not exist
    /// when the process started. That is the whole point — the answer must not cost a
    /// relaunch, exactly as `choose_entity`'s answer does not.
    memory: Mutex<MemoryStatus>,
    /// THE RESOLVED CORPUS, for the read that must not take the spine's lock
    /// (`home_field_data`). The SAME `LoroInstall` `wire_company_memory` built the reader and
    /// the writer from — never a second `LoroInstall::locate`, for the reason `memory.rs`
    /// gives at length: two resolutions of one question is how the read path came to resolve
    /// the CEO's corpus while the write path resolved nothing at all.
    ///
    /// Behind its own mutex and NOT the spine's, for the fifth time in this struct and the
    /// same reason: `send_message` holds the spine lock for a whole turn, and the home screen
    /// is drawn while Rich may be mid-turn. It MOVES for the reason `correction` and `memory`
    /// move — `provision_memory` wires a corpus that did not exist when the process started,
    /// and the picture must not cost a relaunch either.
    ///
    /// `None` on every install with no corpus, which is an ordinary state and not an error:
    /// the home screen then keeps drawing the demonstration, with its banner up.
    loro_install: Mutex<Option<richos_core::loro::LoroInstall>>,
    /// The same `Arc` the spine holds. Kept here so provisioning can re-run
    /// `wire_company_memory` with the provenance sink the correction desk reads from — a
    /// second `SliceProvenance` would mean the desk could not propose against a slice the
    /// compiler had just accepted.
    loro_provenance: SharedSliceProvenance,
    /// WHERE THE DURABLE STORES LIVE — `app_data_dir()`, resolved once in `setup` and
    /// carried rather than re-asked.
    ///
    /// `provision_memory` needs it to open the correction desk's log, and it must be the
    /// SAME directory this boot used: a command that re-resolved it would be a second
    /// answer to a question already answered, which is the shape of every defect this
    /// evening has been about. The desk's log is `<data_dir>/loro-corrections.jsonl` and
    /// there is exactly one expression for that path.
    data_dir: PathBuf,
    /// WHICH ENGINE DIRECTORY THIS INSTALL RUNS `claude` IN — **shared with the lease factory,
    /// and mutable.**
    ///
    /// It is an `Arc<Mutex<_>>` and not a `PathBuf` because `run_setup` can now put an engine
    /// on a machine that had none (`setup_view`, Option D). Rewriting this cell re-points
    /// every future lease — a rotation, a crash recovery, and the attach `run_setup` performs
    /// itself — at what was just installed, with no relaunch. Fixing it at boot would have
    /// meant an app that downloaded, verified and installed an engine and then kept starting
    /// `claude` in the directory it had already failed to find.
    engine_dir: Arc<Mutex<PathBuf>>,
    /// THE SAME CELL THE LEASE FACTORY HOLDS for the `claude` binary, and held here for the
    /// one reason `engine_dir` is: `run_setup` can put Claude Code on a machine that had
    /// none, and the factory must start driving the binary that was just installed rather
    /// than the bare name a pre-install boot fell back to.
    claude_bin: Arc<Mutex<PathBuf>>,
    /// The engine the BOOT resolved, when it resolved a real one. Held so `setup_view::detect`
    /// asks the same question the boot asked and gets the same answer — including the
    /// repo-ancestor candidates `engine.rs` searches and `setup.rs` deliberately does not
    /// reimplement. A developer running from the checkout is therefore never asked to install
    /// an engine that is three directories away.
    boot_engine: Option<PathBuf>,
}

// ================================================================
// THE PHONE CHANNEL'S THREE COMMANDS (see `src/phone/`)
// ================================================================
//
// Three, and only three, because the channel is a thing he switches ON and OFF rather than a thing
// he configures: what state is it in, open a pairing window, forget the phone. Everything else the
// screen needs is in [`phone::PhoneStatus`].
//
// **None of them takes the spine lock**, which is the same reason `stop_turn` and `steer_message`
// do not: `send_message` holds that lock for the whole of a turn, and a settings screen that froze
// until Rich finished would be a settings screen nobody opens.

/// What the "Use Rich from your phone" screen draws itself from.
#[richos_command::command]
fn phone_status(runtime: State<std::sync::Arc<phone::PhoneRuntime>>) -> phone::PhoneStatus {
    runtime.status()
}

/// Open a sixty-second pairing window, starting the channel if this is the first time.
///
/// On a first call this mints the certificate authority, issues the leaf, mints the VAPID key and
/// binds the socket — which is why it can fail in ways he needs to read, and why the error is a
/// sentence rather than a code.
///
/// **IT TAKES NO ARGUMENT, AND THAT IS A RETURN RATHER THAN A REGRESSION** (CEO §61,
/// 2026-09-18). For one day it carried `route`, the user's answer to *"Where do you want to use
/// it?"* — Urban's N1 blocker, signoff 2026-09-19 10.40, where the sheet asked the question and
/// the answer never crossed this bridge. §61 removes the question: *"any mobile app or PWA is
/// utterly useless within the home network … The Tailscale setup is where we start now."*
///
/// **What CAN fail here is new.** A Mac that Tailscale will not certify used to be served the
/// other path silently; there is no other path, so `phone::PhoneError::TailnetNotReady` comes
/// back and the sentence below is what he reads.
#[richos_command::command]
fn phone_begin_pairing(
    app: AppHandle,
    runtime: State<std::sync::Arc<phone::PhoneRuntime>>,
) -> Result<phone::PhoneStatus, String> {
    runtime.begin_pairing(app).map_err(|e| {
        // THE DETAIL GOES TO THE LOG AND THE SENTENCE GOES TO HIM. `Display` says what failed
        // and is the right thing for whoever can act on it; `ceo_sentence` is what he reads.
        eprintln!("[richos] the phone channel could not start: {e}");
        e.ceo_sentence()
    })
}

/// Managed access uses the same pairing authority and phone actions as Tailscale.
#[richos_command::command]
fn phone_connect_enable(app: AppHandle, runtime: State<std::sync::Arc<phone::PhoneRuntime>>) -> Result<phone::PhoneStatus, String> {
    runtime.begin_connect(app).map_err(|e| e.ceo_sentence())
}
#[richos_command::command]
fn phone_connect_disable(runtime: State<std::sync::Arc<phone::PhoneRuntime>>) -> Result<phone::PhoneStatus, String> {
    runtime.disable_connect().map_err(|e| e.ceo_sentence())
}

/// **"Stop and go back", pressed while a code is live** — Urban's G2, under the name it took
/// when CEO §61 removed the chooser it used to point at.
///
/// Closes the open pairing window and, unless a phone is already paired, stops serving. It
/// cannot fail in a way he can act on — there is no key to mint and no port to bind, only
/// things to put down — so it returns the status rather than a `Result`, and the screen it
/// returns to is `This Mac is ready`.
#[richos_command::command]
fn phone_stop_pairing(runtime: State<std::sync::Arc<phone::PhoneRuntime>>) -> phone::PhoneStatus {
    runtime.stop_pairing()
}

/// **"They match", pressed on this Mac** — Sage's pairing review F1. The only act that lets a
/// phone which redeemed the code reach Rich; until it happens the phone's key can answer the six
/// words and do nothing else.
#[richos_command::command]
fn phone_confirm_on_mac(runtime: State<std::sync::Arc<phone::PhoneRuntime>>) -> Result<phone::PhoneStatus, String> {
    runtime.confirm_on_mac().map_err(|e| {
        eprintln!("[richos] the press on this Mac could not be recorded: {e}");
        e.ceo_sentence()
    })
}

/// **"They do not match", pressed on this Mac** — the same teardown the phone's own answer runs:
/// the phone is forgotten, the socket closes and the sheet says who stopped it.
#[richos_command::command]
fn phone_reject_on_mac(runtime: State<std::sync::Arc<phone::PhoneRuntime>>) -> phone::PhoneStatus {
    runtime.reject_on_mac()
}

/// **Open one of the addresses the how-to screens print** (CEO §61.1).
///
/// `target` is the address exactly as the screen prints it, and it is a KEY rather than a URL:
/// `opener::resolve` matches the whole string against a fixed table and refuses anything
/// else. See `opener.rs` for why that is the shape and not a general URL opener.
///
/// The screens keep the address written out beside every one of these controls, so a Mac where
/// `open` does not work is a Mac where the user reads the address — not one where the step is
/// impossible.
#[richos_command::command]
fn open_external(target: String) -> Result<(), String> {
    opener::open(&target)
}

/// "Forget this phone": the socket, the device record and the Keychain keys, all of it.
///
/// The one thing it cannot do is remove the configuration profile from his phone, so the screen
/// says where that is. A cleanup the user has to know to do is a cleanup that does not happen.
#[richos_command::command]
fn phone_forget(runtime: State<std::sync::Arc<phone::PhoneRuntime>>) -> Result<(), String> {
    runtime.forget().map_err(|e| {
        eprintln!("[richos] forgetting the phone did not complete: {e}");
        e.ceo_sentence()
    })
}

#[richos_command::command]
fn list_threads(state: State<AppState>) -> Vec<ThreadSummary> {
    state.reader.snapshot().threads()
}

#[richos_command::command]
fn active_thread(state: State<AppState>) -> Option<String> {
    state.reader.snapshot().active_thread().map(|s| s.to_string())
}

#[richos_command::command]
fn create_thread(app: AppHandle, state: State<AppState>, title: String) -> Result<String, String> {
    let title = if title.trim().is_empty() { "New thread".to_string() } else { title };
    // A thread cannot exist without an immutable entity home (ECS §3.2). Until the entity
    // picker lands (slice 4) the entity comes from deterministic root resolution, and an
    // unresolved root refuses rather than guessing.
    let entity = state.entity.lock().unwrap().clone().ok_or_else(|| ENTITY_UNRESOLVED_MESSAGE.to_string())?;
    let id = take_the_spine(&state.spine).create_thread(&title, &entity).map_err(|e| e.to_string())?;
    tell_the_phone_the_threads(&app);
    Ok(id)
}

/// A conversation was created, renamed or titled by Haiku: a paired phone hears its name now
/// rather than at the next turn's end or its next `hello` (`phone::PhoneRuntime::threads_changed`).
/// Off the caller's thread, like the push after a turn, so it never waits on the channel.
fn tell_the_phone_the_threads(app: &AppHandle) {
    if let Some(runtime) = app.try_state::<Arc<phone::PhoneRuntime>>() {
        let runtime = Arc::clone(&runtime);
        tauri::async_runtime::spawn_blocking(move || runtime.threads_changed());
    }
}

#[richos_command::command]
fn switch_thread(state: State<AppState>, thread_id: String) -> Result<(), String> {
    switch_thread_in(&state.spine, &state.reader, &state.control, &thread_id)
}

fn switch_thread_in(spine: &Mutex<Spine>, reader: &SpineReader, control: &TurnControl,
    thread_id: &str) -> Result<(), String> {
    // Startup is already preparing this exact active conversation independently.
    // Reopening it is a read, not another switch. Let the window paint while preparation
    // runs. A different thread takes only the short state-switching lock.
    if control.for_thread(thread_id).front_desk_priming().is_some()
        && reader.snapshot().active_thread() == Some(thread_id) {
        return Ok(());
    }
    take_the_spine(spine).switch_thread(thread_id).map_err(|e| e.to_string())
}

/// Scoped read. Now fallible: a thread written before entity scoping existed has no
/// entity home, and `LedgerError::UnboundThread` is returned rather than an empty list —
/// "I will not serve this" and "there is nothing here" are different statements.
#[richos_command::command]
fn get_messages(state: State<AppState>, thread_id: String) -> Result<Vec<Message>, String> {
    state.reader.snapshot().messages(&thread_id).map_err(|e| e.to_string())
}

/// One thread's TYPED TIMELINE (UX §12), gated to the CEO view (§5.3).
///
/// The RELOAD half of the §13 live family. `rich://*` events carry a turn while it runs;
/// this is how a cold reopen — or a thread switch back to a thread that finished while
/// the CEO was elsewhere — gets the same items with the same ids. §13's *"reconnect uses a
/// full snapshot followed by events after a cursor"*, and §14's *"load the selected thread
/// snapshot"*.
///
/// **THE GATE IS NOT RE-IMPLEMENTED HERE, AND MUST NOT BE.** `Timeline` deliberately does
/// not implement `Serialize`, so there is no ungated path from this function to the
/// webview; `view(ViewMode::Ceo)` is the only way to obtain a payload, and it REMOVES
/// technical items and technical detail rather than masking them (timeline.rs). This
/// command therefore cannot leak a raw command, a file path or an internal item even if it
/// wanted to — it never holds those bytes.
///
/// Fails closed on an unbound thread, exactly like `get_messages`.
#[richos_command::command]
fn get_timeline(app: AppHandle, state: State<AppState>, thread_id: String) -> Result<serde_json::Value, String> {
    let payload = question_host::payload(&state, &thread_id);
    // **HIS FRONT DESK IS MADE READY HERE, AFTER THE SNAPSHOT AND NEVER BEFORE IT** — the CEO's
    // §55. See [`ready_the_front_desk`] for why this command is the hook.
    ready_the_front_desk(&app, &thread_id);
    payload
}

/// Prepare a conversation on a background thread, once per opened thread. Startup
/// and company confirmation ask before the window opens the conversation. The exact
/// active conversation can be read through its snapshot while preparation runs.
/// Switching conversations takes only the short state lock.
///
/// The priming intake marker makes messages durable while the provider is preparing.
/// Context and company permissions retain the thread's immutable binding. Failure is
/// logged and the next real request keeps the existing recovery path.
/// Start the already selected conversation while the user is still opening the app
/// or completing setup. No company or conversation is invented for this optimization.
fn ready_selected_front_desk(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let active = state.reader.snapshot().active_thread().map(str::to_string);
    if let Some(thread) = active {
        ready_the_front_desk(app, &thread);
    } else if let Some(entity) = state.entity.lock().unwrap().clone() {
        ready_a_spare_front_desk(app, entity);
    }
}

fn ready_the_front_desk(app: &AppHandle, thread_id: &str) {
    {
        let Some(state) = app.try_state::<AppState>() else { return };
        let mut asked = state.front_desk_primed_for.lock().unwrap();
        if !asked.insert(thread_id.to_string()) {return;}
    }
    let app = app.clone();
    let thread_id = thread_id.to_string();
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        let verdict = richos_core::spine::prime_front_desk_without_the_spine(&state.spine,&thread_id);
        // Priming drains durable input; its detached replies run outside shared state.
        if let Err(error)=richos_core::spine::drive_shared_turns(&state.spine) {eprintln!("[richos] queued reply: {error}");}
        let spine=take_the_spine(&state.spine);
        adopt_at_the_turn_boundary(spine, &state.work);
        match verdict {
            richos_core::spine::FrontDeskReady::Ready { millis, spawned } => eprintln!(
                "[richos] conversation preparation finished in {millis} ms{}",
                if spawned { ", including connection startup" } else { "" }
            ),
            richos_core::spine::FrontDeskReady::AlreadyReady => {}
            // Both of these are ordinary and neither is a failure of anything: a turn may be in
            // flight when he switches threads, and a thread with no entity home fails closed
            // everywhere else too. His first message primes as it always did.
            other => eprintln!("[richos] the front desk was not made ready before he types: {other:?}"),
        }
        // **AND THEN THE ONE FOR THE THREAD AFTER THIS ONE** (CEO §55). Opening a conversation
        // is the best evidence the app gets that he is here and working, and the next thing he
        // starts may be a brand-new thread, which is the case `ready_the_front_desk` cannot
        // help with. The entity comes from THIS thread's immutable home rather than from the
        // active context, because the active context is whatever was switched to last and a
        // spare primed for the wrong company can never be adopted.
        //
        // Prepare the selected conversation first, then its optional next-thread spare.
        // Neither provider operation holds shared conversation state.
        let entity = take_the_spine(&state.spine).ledger().thread_binding(&thread_id).map(|b| b.entity_id().clone());
        if let Ok(entity) = entity {
            ready_a_spare_front_desk(&app, entity);
        }
    });
}

/// **Make a desk ready for the thread he has NOT started yet** — the CEO's §55, and the half
/// [`ready_the_front_desk`] structurally cannot reach.
///
/// # Why a second function, and why the timeline read is not the hook for this one
///
/// `ready_the_front_desk` primes the desk of a thread that EXISTS. On a brand-new thread that
/// is always too late, and the reason is in `ui/main.js` rather than in anything here:
/// `create_thread_in` is called inside the Send handler (`ui/main.js:1688`, the `draftEntityId`
/// branch — *"NOTHING was persisted when the CEO opened the new-thread screen"*), and
/// `openThread` -> `get_timeline` -> `ready_the_front_desk` runs microseconds later with
/// `send_message` right behind it. So his first sentence into a new thread does not wait for a
/// REMAINDER of the prime, it waits for all of it. `Spine::ready_a_spare_front_desk` spends that
/// turn earlier, against a thread id that has been reserved and not written.
///
/// # WHERE IT IS CALLED, and the one place it should be and is not
///
/// Three hooks, all of them Rust: at the end of the boot, after a thread's own pre-prime, and
/// after a thread is created (which consumes the spare, so the next one is prepared). Together
/// they cover the measured case — the first brand-new thread of a session, which is Ray's
/// measurement 1 — and every new thread started after he has opened or created another.
///
/// **The hook that is MISSING is the entity's new-thread screen itself, and that is a gap, not
/// a design.** `showEntityView` in `ui/main.js` makes no bridge call at all: it is pure local
/// rendering, so nothing in Rust can know he is sitting on that screen. One `Bridge.invoke` there
/// would close it and would be the exactly-right hook (it is the window in which he is typing his
/// first sentence). It is not taken here because `app/ui/**` is another agent's live surface
/// today. Until it is, the uncovered case is: he uses eight threads, comes back to the entity
/// screen much later, and starts a ninth — the spare readied after his last thread open is still
/// standing, so even that case is usually covered; it fails only if the entity changed in
/// between.
///
/// **One attempt per call and nothing is surfaced to him.** The verdict goes to stderr like its
/// sibling's: this is an optimization of WHEN the waiting happens, so a failure is not his
/// business, and a UI that said anything about it would be telling him about the machinery §55
/// exists to hide.
fn ready_a_spare_front_desk(app: &AppHandle, entity: richos_core::EntityId) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        // **WITHOUT THE SPINE, and that is the whole of the CEO's six seconds.** This used to
        // be `take_the_spine(&state.spine).ready_a_spare_front_desk(&entity)`, which held the
        // mutex across a process spawn AND a model turn. `create_thread_in` calls this
        // microseconds before the window issues eight more bridge calls, so every one of them
        // queued behind it: measured on the real window on 2026-09-19, `a window command waited
        // 18314 ms for the spine` on a debug build with the turn's own header starting its
        // clock 18.6 s after the keystroke, and on the shipped candidate the same shape is
        // Ray's 5378 ms spare and ~6 s of pre-turn overhead. The five steps are unchanged; the
        // mutex is now taken only for the three that are microseconds long
        // (`spine.rs`'s `ready_a_spare_front_desk_without_the_spine`).
        let verdict = richos_core::spine::ready_a_spare_front_desk_without_the_spine(&state.spine, &entity);
        match verdict {
            richos_core::spine::SpareReady::Ready { millis } => eprintln!(
                "[richos] next conversation ready for {entity}: preparation took {millis} ms"
            ),
            richos_core::spine::SpareReady::AlreadyReady => {}
            other => eprintln!("[richos] no spare front desk was made ready for {entity}: {other:?}"),
        }
    });
}

// =========================================================================================
// EVERY HOP BETWEEN HIS KEYSTROKE AND THE TURN'S CLOCK — the CEO's §55, measured at the LOCK
// =========================================================================================
//
// **What was here before, and what its narrowness cost.** This was `send_wait_notice`, and it
// measured exactly one lock take: [`send_message`]'s. Ray's candidate-.12 re-walk then found
// "On it!" on the real window at **14.24 s** with the turn's own header reading **"Working for
// 8s"** (`docs/verification/2026-09-19-nightly-1.2.0-nightly.20260919.1-mac-and-android-rewalk-audit.md`
// §2), so about six seconds were spent BEFORE the turn's clock started — and this log line
// never printed, because by the time `send_message` asked for the mutex it was free. The wait
// had been paid in full by the bridge calls the window makes IN FRONT of the Send
// (`ui/main.js`'s `send()` on a brand-new thread: `create_thread_in`, `navigation_tree`,
// `switch_thread`, `active_context`, `techy_mode`, `get_timeline`, `onboarding_view`,
// `take_work_notices`), not one of which measured anything at all.
//
// So the measurement moved from ONE call site to the LOCK — the only thing all of them have in
// common — and the site is named by `#[track_caller]` rather than typed, so a hop added later
// is measured without anybody remembering to measure it. That is the hand-maintained-inventory
// drift `scripts/run-tests.sh` warns about in its own words, refused by construction.

/// **The line any window command writes when it had to WAIT for the spine** — `None` when it
/// did not, which is every uncontended take.
///
/// A pure function of the measured wait, so the rule is a unit test rather than something only
/// a contended mutex on a real machine can show, exactly as `native::tool_residency_env` and
/// `native::child_args` are. The caller measures; this decides what, if anything, is true
/// enough to say. **Silent by ARITHMETIC and not by a chosen threshold**: an uncontended
/// `Mutex::lock` returns in well under a millisecond and rounds to 0.
/// **Keeps yt-dlp at the newest nightly** (media-tools plan §3, `richos_core::media_tools`): one
/// check three seconds after launch, then every six hours, or every five minutes while no copy is
/// installed (review m3). A plain thread, because each check is a blocking `curl`. A failed check
/// changes nothing on disk; it is one operator line and the next tick tries again.
fn spawn_media_tools_refresh() {
    use richos_core::media_tools;
    let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from) else {
        eprintln!("[richos] yt-dlp: no HOME, so no tools folder; not checking");
        return;
    };
    let tools = media_tools::tools_dir(&home);
    let spawned = std::thread::Builder::new().name("yt-dlp-refresh".into()).spawn(move || {
        std::thread::sleep(media_tools::FIRST_CHECK_AFTER);
        loop {
            match media_tools::refresh_with_curl(&tools) {
                Ok(media_tools::Refreshed::Unchanged { tag }) => eprintln!("[richos] yt-dlp: nightly {tag} is the newest"),
                Ok(media_tools::Refreshed::Installed { tag, previous, .. }) => eprintln!(
                    "[richos] yt-dlp: installed nightly {tag}{}",
                    previous.map(|p| format!(" (replaces {p})")).unwrap_or_default()),
                Err(e) => eprintln!("[richos] yt-dlp: check failed ({}): {e}", e.kind()),
            }
            std::thread::sleep(media_tools::next_check(&tools));
        }
    });
    if let Err(e) = spawned { eprintln!("[richos] yt-dlp: could not start its check: {e}"); }
}

fn spine_wait_notice(site: &str, waited: std::time::Duration) -> Option<String> {
    let millis = waited.as_millis();
    (millis > 0).then(|| format!(
        "[richos] a window command waited {millis} ms for the conversation at {site}. \
         The screen shows \"Sending your message / Waiting for Rich to accept it\" or \
         \"Opening conversation\" during this wait."
    ))
}

/// Milliseconds since this process first reached for the spine — the hop trace's clock.
/// Lazily anchored, which on every real launch is the boot's own first read.
fn since_the_launch() -> u128 {
    static ANCHOR: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    ANCHOR.get_or_init(std::time::Instant::now).elapsed().as_millis()
}

/// `RICHOS_HOP_TRACE=1` — every take and every release of the spine, timestamped.
///
/// **Off by default, and it stays off**, because the line above is the one that is true for
/// HIM: he cares that he waited, not that a lock changed hands. The trace is what an engineer
/// needs in order to say where six seconds went, it is read once per measurement, and a
/// `gui-boot` log carrying a line per lock take would be accounting noise. So it is a switch
/// rather than a level.
fn hop_trace_is_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("RICHOS_HOP_TRACE").is_ok_and(|v| v == "1"))
}

/// `src/main.rs:1234` — the caller's site, without this machine's checkout path in it.
fn spine_site(site: &std::panic::Location<'static>) -> String {
    let file = site.file();
    let short = file.rfind("src/").map(|at| &file[at..]).unwrap_or(file);
    format!("{short}:{}", site.line())
}

/// The spine, held — and a measurement of what it cost to get it and what it cost to keep it.
///
/// `Deref`/`DerefMut` to [`Spine`], so every call site reads exactly as it did when this was a
/// bare `MutexGuard` and `&*hold` still coerces where a `&Spine` is wanted.
struct SpineHold<'a> {
    guard: std::sync::MutexGuard<'a, Spine>,
    site: String,
    taken: std::time::Instant,
}

impl std::ops::Deref for SpineHold<'_> {
    type Target = Spine;
    fn deref(&self) -> &Spine { &self.guard }
}

impl std::ops::DerefMut for SpineHold<'_> {
    fn deref_mut(&mut self) -> &mut Spine { &mut self.guard }
}

impl Drop for SpineHold<'_> {
    fn drop(&mut self) {
        if hop_trace_is_on() {
            eprintln!(
                "[richos] hop-trace +{} ms  spine RELEASED at {} after holding {} ms",
                since_the_launch(), self.site, self.taken.elapsed().as_millis()
            );
        }
    }
}

/// **Take the spine, and say so when he waited for it.**
///
/// The one way this file reaches `AppState::spine`. `#[track_caller]` puts the CALLER's
/// `file:line` into the line, which is why there is no name argument that could get out of
/// step with the site it names.
#[track_caller]
fn take_the_spine(spine: &std::sync::Mutex<Spine>) -> SpineHold<'_> {
    let site = spine_site(std::panic::Location::caller());
    let asked = std::time::Instant::now();
    let guard = spine.lock().unwrap();
    hold_the_spine(guard, site, asked)
}

/// **The same door for the one hop that must not `unwrap`** — the spoken-submit callback,
/// which runs on the voice thread and walks away from a poisoned spine rather than panicking
/// inside it. His spoken sentence is a hop on the same clock as his typed one, so it is
/// measured by the same function; only the failure is different.
#[track_caller]
fn take_the_spine_or_give_up(spine: &std::sync::Mutex<Spine>) -> Option<SpineHold<'_>> {
    let site = spine_site(std::panic::Location::caller());
    let asked = std::time::Instant::now();
    let guard = spine.lock().ok()?;
    Some(hold_the_spine(guard, site, asked))
}

/// Where the wait is written down. Both doors end here, so there is one place that decides
/// what a wait is worth saying and one place a reader has to check.
fn hold_the_spine<'a>(
    guard: std::sync::MutexGuard<'a, Spine>,
    site: String,
    asked: std::time::Instant,
) -> SpineHold<'a> {
    let waited = asked.elapsed();
    if let Some(line) = spine_wait_notice(&site, waited) {
        eprintln!("{line}");
    }
    if hop_trace_is_on() {
        eprintln!(
            "[richos] hop-trace +{} ms  spine TAKEN at {site} after waiting {} ms",
            since_the_launch(), waited.as_millis()
        );
    }
    SpineHold { guard, site, taken: std::time::Instant::now() }
}

/// The "talk to Rich" loop. Persists the prompt (crash-safe) + runs the turn. While the
/// turn runs, the spine streams live events to the webview — `rich://turn-started`, a
/// sequence of `rich://chunk` deltas, then `rich://turn-completed` (or `rich://turn-error`)
/// — so the UI renders Rich's reply token-by-token and shows the "Rich is working" state.
/// The returned message view is the final, reconciled snapshot (a UI can rely on either
/// the stream or this return; both agree because the ledger backs both).
/// **`(async)` is load-bearing, not decoration.** A plain `#[tauri::command]` on a
/// non-async fn is dispatched by the macro as `ExecutionContext::Blocking`, which runs it
/// inline on the IPC/main thread (`tauri-macros`'s own `kind` string for that arm is
/// `"sync"`; the `async` attribute on a sync fn makes it `"sync_threadpool"`). A turn can
/// last hours, so blocking there would freeze the webview AND queue `stop_turn` behind the
/// very turn it is meant to interrupt — the stop would be structurally impossible no
/// matter how the rest of the plumbing is written. `#[richos_command::command]` emits that
/// `(async)` and also runs the body off the async runtime's worker threads
/// (`crates/richos-command/src/lib.rs`), so an hours-long turn does not hold one of them either.
#[richos_command::command]
fn send_message(
    state: State<AppState>,
    // **THE PHONE, INJECTED RATHER THAN REACHED FOR.** It is managed unconditionally at boot
    // (`app.manage(phone_runtime)`) and is inert until a phone is paired, so this costs a Mac
    // with no phone nothing at all and saves this function an `AppHandle` it has no other use
    // for.
    phone: State<std::sync::Arc<phone::PhoneRuntime>>,
    text: String,
    thread_id: String,
    // **The app is sending these words because he pressed a button** ("Start the questions"),
    // and they are never shown as his (product hunt 2026-10-15, finding 14). They take the
    // blocking road below, never the intake log, whose whole purpose is HIS words reaching
    // his phone before the spine is free.
    pressed: Option<bool>,
) -> Result<Vec<Message>, String> {
    let pressed = pressed == Some(true);
    // ===================================================================================
    // HOW LONG HE WAITED BEFORE HIS TURN EVEN STARTED — and why this line exists
    // ===================================================================================
    //
    // `ready_the_front_desk` holds THIS mutex for the whole of `prime_front_desk`. A Send
    // issued while the pre-prime was still running USED TO block here for the remainder of
    // it, with the UI honestly showing "Sending your message / Waiting for Rich to accept it"
    // (`ui/main.js`) the entire time. That wait was his, it was charged against §55's clock,
    // and until this line NOTHING wrote it down: `app.log` carried the prime's own duration
    // and never the part of it he paid for. The pre-prime no longer reaches this line at all
    // — see the block below — and the measurement stays, because it is now the only thing
    // that would name a DIFFERENT holder of this lock.
    //
    // **What that cost.** On candidate .10, Ray's first measurement bracketed "On it!" at
    // 11-12 s on the window while the same turn measured ~6 s in process
    // (`docs/verification/first-words-2026-09-18-toolsearch.md`, run E). The turn's own
    // counter read `Working for 7s` at the frame where the words appeared, so about five of
    // those seconds were spent before the turn started at all — on a brand-new thread, whose
    // prime had been logged at 4412 ms. With no timestamp at this boundary that had to be
    // inferred from screenshots, and a different line in the same log was blamed instead.
    //
    // **No threshold and no tuning.** An uncontended `Mutex::lock` returns in well under a
    // millisecond and rounds to 0, so the ordinary send stays silent by arithmetic rather
    // than by a number somebody picked.
    // ===================================================================================
    // AND THE ROAD THAT DOES NOT WAIT FOR IT
    // ===================================================================================
    //
    // **THIS USED TO BE `defer_send`, WHICH ANSWERED `Some` ONLY WHILE THE FRONT DESK WAS
    // BEING PRIMED. It is taken for every typed sentence now, and Ray's `.20260920.1` walk
    // is why.**
    //
    // He typed on the Mac with his phone beside the keyboard. His own message reached the
    // phone **1.3-6.1 s later on four of five turns**, while Rich's REPLY to the same message
    // crossed in under a second
    // (`docs/verification/2026-09-20-nightly-1.2.0-nightly.20260920.1-mac-to-phone-in-the-vm-audit.md`,
    // step 3). Two legs of one turn, so it was never the network.
    //
    // **THE CAUSE, MEASURED RATHER THAN REASONED** (`phone::listen`'s two timing tests, over
    // real TLS through the shipped listener):
    //
    //   * his row and Rich's reply leave the Mac on ONE path, and it costs **5 ms**;
    //   * with the spine held for 1,500 ms, that same 5 ms leg becomes **1,507 ms**.
    //
    // His sentence cannot reach a phone until it reaches the ledger, and it cannot reach the
    // ledger until this function owns the spine: `Ledger::record_prompt_received` is
    // `&mut self` over in-memory projections. Meanwhile `ui/main.js` has ALREADY painted his
    // bubble from its own model (`:1814-1821`, before it calls this command at `:1824`), so
    // the wait is invisible on the Mac and is the whole of what the phone shows. The biggest
    // holder of that mutex is an ordinary turn, which this function keeps holding well past
    // the moment the window is told the turn is over and the composer goes live again.
    //
    // **So his words stop waiting for the spine at all.** They go to the durable intake log —
    // one `fsync`, no lock, the road the PHONE's own messages have taken since `bridge.rs` —
    // and the spine picks them up with `poll_intake` below, in order, through the same
    // `accept_prompt` the direct road uses (`spine.rs`'s `IntakeRecord::Desk` arm). Nothing
    // about the turn changes; only WHEN his sentence becomes a durable fact.
    //
    // **AND IT IS ONLY TAKEN WHEN NOTHING CAN REFUSE HIM.** Every refusal below hinges on
    // `spine.has_lease()`, and `control.lease_session()` answers that question WITHOUT the
    // lock. With no lease session this falls through to the old road untouched, so the
    // first-run sentences Ray fought for in candidate .15 are reached exactly as before — and
    // a phone can never be shown a sentence the Mac then refuses, which is plan §6's refusal
    // and the one shape this must not have.
    //
    // A write that FAILS is not a refusal of his message either: it falls through to the same
    // old road, and the only thing lost is the wait.
    let destination = state.reader.snapshot().ledger().thread_binding(&thread_id).ok();
    let his_words_are_on_the_log = if !pressed && destination.is_some() && desk_input_can_be_logged(&state.control, &thread_id) {
        match state.control.submit_from_desk(&thread_id, destination.map(|binding| binding.entity_id().clone()), &text)
        {
            Ok(record) => {
                // THE PHONE, BEFORE THE LOCK. Inert when nothing is paired, so this costs a
                // Mac with no phone one atomic read.
                let announced = phone.announce_his_words(
                    &thread_id,
                    &format!("intake_{}", record.id()),
                    &text,
                );
                if announced {
                    eprintln!(
                        "[richos] his Send is durable (intake {}) and is on his phone before \
                         the spine has been asked for anything",
                        record.id()
                    );
                }
                Some(record.id())
            }
            Err(e) => {
                eprintln!(
                    "[richos] his Send could not be written to the intake log ({e}); it takes \
                     the blocking road the way it did before, which is the only cost"
                );
                None
            }
        }
    } else {
        None
    };
    // `take_the_spine` is what writes the wait down now, for THIS site and for every other
    // one — see the block above it. The measurement did not move; it stopped being unique
    // to this line, which is what let six seconds go unattributed on candidate .12.
    let mut spine = take_the_spine(&state.spine);
    // ===================================================================================
    // A FACTORY IS NOT AN ENGINE — Ray's candidate .15, `esc-20260919T152225Z-2e44d112`
    // ===================================================================================
    //
    // **The first-run arm used to live inside the factory gate below, and was therefore
    // unreachable on every build that has ever shipped.** `set_lease_factory` is called
    // unconditionally at boot — its own comment says so, *"REGARDLESS of initial boot
    // success"*, and that is right: a later sign-in or a crash recovery needs a respawn path.
    // So `has_lease_factory()` is ALWAYS true, `&&` never let the first arm run, and
    // `SETUP_INCOMPLETE_ENGINE` — the sentence that names the missing piece and reopens the
    // sheet — has never once reached a screen from here.
    //
    // **What the CEO got instead**, measured on his Mac at 16:0x on 2026-09-19 with an engine
    // installed from `3313945b26b7` against a build pinning `adece4c069e4`: the turn STARTED,
    // the factory then failed to make a lease, and the failure arrived as a transient
    // interruption — *"I lost my connection to the part of me that thinks, partway through…
    // asking again is worth a try."* Asking again could not work. Nothing about that machine
    // was going to change by being asked twice, and the one thing that would have fixed it in
    // a single press was never offered. His `app.log` carries the same line four times over.
    //
    // **So the question is asked of the DISK, before a turn is started**, which is what the
    // arm was always written to do. `send()` in `ui/main.js` then re-reads `setup_status` and
    // reopens the sheet behind this sentence, so the promise it makes — *"I've put the setting
    // up back on your screen: press Set it up"* — is true rather than a claim.
    //
    // **AND IT COSTS NOTHING ON A HEALTHY MAC, which is why it is gated on `boot_engine`.**
    // `setup_view::detect` calls `engine_is_usable`, which hashes the delivered runtime — 322
    // MB in 6,530 files, 0.81 s warm and 1.70 s cold (measured 2026-09-17). Paying that in
    // front of the first send of every launch would be a §55 regression to fix a §19 one.
    // `boot_engine` is `Some` exactly when this launch RESOLVED a usable, correctly-pinned
    // engine, and a launch that did cannot be first-run-incomplete — so the disk is read only
    // on the launches that already know something is wrong, where every candidate is refused
    // on its stamp and nothing is hashed at all.
    if !spine.has_lease() && state.boot_engine.is_none() {
        let disk = setup_view::detect(None);
        if let Some(sentence) = setup_view::incomplete_message(&disk) {
            return Err(refused_send("first-run setup is incomplete", sentence.into()));
        }
    }
    // A configured factory connects inside the tracked turn. This covers first launch,
    // a later sign-in and replacement of a session retired by Stop.
    if !spine.has_lease() && !spine.has_lease_factory() {
        return Err(refused_send("no compute lease and no factory", LEASE_UNAVAILABLE_MESSAGE.to_string()));
    }
    // **AND IT IS NOT ASKED OF THE ROAD THAT DOES NOT NEED IT.** This gate is about
    // `submit_prompt_to` below, which submits to whatever is active after it activates.
    // `drain_intake` resolves the thread from the RECORD instead (`spine.rs`'s
    // `IntakeRecord::Desk` arm: `fence_binding(&thread_id)`), so a Mac with a lease, a
    // durable record and no active thread is a case that road handles and this one cannot.
    // Asking it there would refuse a sentence that is already on disk and already on his
    // phone, which is the one shape the road above must not have.
    if his_words_are_on_the_log.is_none() && spine.active_thread().is_none() {
        return Err(refused_send("no active thread", "Open a conversation first.".into()));
    }
    // ===================================================================================
    // A MESSAGE TO ANY CONVERSATION IS ANSWERED, NEVER REFUSED
    // ===================================================================================
    //
    // This used to compare `thread_id` against the one active thread and refuse anything
    // else: *"The conversation changed before your message was sent. Open the original
    // conversation to try again."* The CEO's Two Riches page is *"a CEO can create a
    // conversation thread (i.e. any number of conversation threads)"* and *"the CEO could
    // open and run multiple things in parallel"*, and an app that throws a typed sentence
    // back because he had moved is the opposite of that.
    //
    // `submit_prompt_to` makes that thread active and submits there. If a turn is running,
    // his message is durably recorded and queued with ITS OWN binding, and the turn
    // boundary delivers it on its own thread — the front desk that answers it is that
    // thread's own, still alive from the last time he spoke to it (`spine.rs`'s `Resident`).
    //
    // **AND WHEN HIS WORDS ARE ALREADY ON THE LOG, THE SPINE TAKES THEM OFF IT INSTEAD** — the
    // same call the phone's drain makes, so there is one road out of the log and not two.
    // `drain_intake` goes through `accept_prompt` for a `Desk` record, which is the whole of
    // the acceptance `submit_prompt_inner` performs (corrections staged, his row emitted, the
    // turn queued), then `drain_queue` runs it. Records ahead of his are drained first, which
    // is the ORDER he typed things in and is exactly what a second road would have broken.
    if let Some(intake_id) = his_words_are_on_the_log {
        spine.poll_intake().map_err(|e| {
            refused_send(
                &format!("the spine could not take intake {intake_id} off the log"),
                e.to_string(),
            )
        })?;
    } else if pressed {
        spine
            .submit_pressed_prompt_to(&thread_id, &text)
            .map_err(|e| refused_send("the spine refused the prompt", e.to_string()))?;
    } else {
        spine
            .submit_prompt_to(&thread_id, &text, Source::Text)
            .map_err(|e| refused_send("the spine refused the prompt", e.to_string()))?;
    }
    drop(spine);
    richos_core::spine::drive_shared_turns(&state.spine).map_err(|e|e.to_string())?;
    let spine=take_the_spine(&state.spine);
    let messages=spine.messages(&thread_id).map_err(|e|e.to_string())?;
    adopt_at_the_turn_boundary(spine, &state.work);
    Ok(messages)
}

// =====================================================================================
// THE TURN BOUNDARY'S ADOPTION — ONE DOOR, EVERY ENTRANCE THAT RUNS HIS TURNS
// =====================================================================================
//
// CEO ruling §88: *"The user should be able to answer on their phone just as well as they
// can answer on the desktop app."* Until this door existed only the typed send adopted the
// work Rich wrote down during a turn; a task given aloud or from the phone sat `Registered`
// until he next typed in that conversation (esc-20260926T083231Z-23865924). Each entrance
// below runs his turns and then hands the spine to this function, which asks the spine
// which conversations' turns ended (`Spine::take_ended_turns`) and adopts for each.
//
// The entrances, all of them (a new one that runs a turn must call this too, and
// `turn_boundary_tests::every_entrance_that_runs_his_turns_adopts_at_its_boundary` names each):
//   * typed      — `send_message`
//   * spoken     — the voice submit callback, through `run_the_spoken_turn`
//   * phone      — `phone::bridge`'s drain, through `drain_incoming_messages`
//   * priming    — `ready_the_front_desk` (`prime_front_desk` drains what was sent during it)
//   * boot       — `reconcile_intake`, which runs what outlived the last process

/// **Release the spine, then start the work his ended turns wrote down.** Returns how many
/// assignments were adopted; zero is the ordinary answer.
///
/// **It takes the hold BY VALUE, and that is the enforcement of §0 row 3**: the work host is
/// never reached while the spine is held, and a caller cannot keep a guard it has given
/// away. `&mut Spine` also satisfies the bound, for the boot, where the spine is not behind
/// its mutex yet.
fn adopt_at_the_turn_boundary<G>(mut spine: G, work: &Arc<richos_core::work_host::WorkHost>) -> usize
where
    G: std::ops::DerefMut<Target = Spine>,
{
    let ended = spine.take_ended_turns();
    drop(spine);
    ended.iter().map(|binding| work.adopt_registered(binding)).sum()
}

/// **His spoken sentence, as a turn, and then its boundary** — the tail of the voice submit
/// callback, a function so the adoption it ends with is tested rather than assumed.
fn run_the_spoken_turn(
    shared: &Mutex<Spine>,
    work: &Arc<richos_core::work_host::WorkHost>,
    text: &str,
    rich_audible: bool,
) {
    let mut spine=shared.lock().unwrap();
    if let Err(e) = spine.submit_prompt_spoken(text, Source::Jam, rich_audible) {
        eprintln!("[richos] voice turn failed: {e}");
    }
    drop(spine);
    if let Err(error)=richos_core::spine::drive_shared_turns(shared) {eprintln!("[richos] spoken reply: {error}");}
    let spine=shared.lock().unwrap();
    // A failed turn still reaches here: what it wrote down before failing is a receipt.
    adopt_at_the_turn_boundary(spine, work);
}

/// Receive durable input and run its model I/O outside the shared conversation lock.
fn drain_incoming_messages(
    shared: &std::sync::Mutex<Spine>,
    work: &Arc<richos_core::work_host::WorkHost>,
) -> Result<(), richos_core::spine::SpineError> {
    let outcome=shared.lock().unwrap().poll_intake();
    let driven=richos_core::spine::drive_shared_turns(shared);
    let spine=shared.lock().unwrap();
    adopt_at_the_turn_boundary(spine, work);
    outcome.and(driven)
}

/// A cold prime starts before its provider has a session id. Its intake marker
/// already identifies the exact conversation, so its messages must be durable too.
/// The send handler always drains intake under the spine after writing it, including
/// when priming finishes between this observation and the write.
fn desk_input_can_be_logged(control: &TurnControl, thread_id: &str) -> bool {
    control.for_thread(thread_id).lease_session().is_some() || control.for_thread(thread_id).front_desk_priming().is_some()
}

/// **A TYPED MESSAGE THAT NEVER BECAME A TURN, ON THE OPERATOR'S LOG** — the nightly's D4.
///
/// `docs/verification/2026-09-17-nightly-1.2.0-20260917.1-onscreen-audit.md` §D4: *"The
/// failed voice turn logged `[richos] voice turn failed: cognition io: …`. The failed text
/// turn logged NOTHING at all — the log sat at 44 lines across it. The `gui-boot` log is the
/// one artifact meant to hold every line of a launch to account, and the most common failure
/// path writes nothing to it."*
///
/// **Half of that is now fixed elsewhere and this closes the other half.** A text turn that
/// STARTS and then fails is logged by `spine.rs::record_interruption`, on the shared path,
/// with its cause class — so the audit's own case (a turn that reached the screen as
/// "Stopped after 1s") already produces `[richos] turn interrupted [not-signed-in]: …`
/// whichever surface it came from. What remained is the case where the message never became
/// a turn at all: `send_message` returned its sentence to the window and said nothing to the
/// log, while the voice path beside it logged every refusal.
///
/// It returns the CEO's sentence UNCHANGED. The operator's reason is a separate, shorter
/// string that never reaches the window — the same split `voice_readiness` keeps between its
/// boot line and its `reason`.
fn refused_send(why: &str, ceo_sentence: String) -> String {
    eprintln!("[richos] text turn refused before it started ({why})");
    ceo_sentence
}

/// What the CEO is told when there is no lease to think with.
///
/// WHAT IT USED TO SAY, AND WHY IT IS THE EXACT DEFECT THIS PASS EXISTS TO REMOVE:
///
///     "I'm not connected to my thinking right now — check that the Claude CLI is signed
///      in, then restart me."
///
/// "The Claude CLI" is a thing the CEO has never seen. "Restart me" names no control, and
/// there is no restart control in this app to name. So the app reported a condition that
/// required a human action while rendering neither the action nor the actor — a request
/// wearing a status's clothes, which is precisely how a landed-but-inert guard got reported
/// as a date instead of a five-second fix.
///
/// It now offers the one thing he can genuinely do (quit and reopen — no key combination is
/// quoted, because Windows packaging is still an open gap in the architecture doc §4.4 and
/// a hard-coded ⌘Q would become a lie the day it ships), and names who owns the rest.
///
/// THREE COPIES EXIST and that is one problem this commit only halves: the two Rust sites
/// now share this const, and `app/ui/mock.js` carries a fourth-wall copy for the browser
/// preview's `_notConnected` switch. The affordance suite asserts the two are identical, so
/// they cannot drift silently.
const LEASE_UNAVAILABLE_MESSAGE: &str =
    "RichOS couldn't reach Claude Code, which Rich runs on. Quit RichOS and open it again, then send your message again.";

/// What the CEO is told when there is no corpus this install could write a correction to.
///
/// **A CONST, and it became one because it being prose inside a closure cost two test
/// suites.** `49e2cd4` rewrote this sentence in place — it had said *"No loro corpus is
/// configured for this install…"*, a word that is false when the corpus is sitting at a path
/// nobody looked at — and left `app/ui/mock.js` and `tests/lib/state-registry.js` holding the
/// old one. `affordances.js` and `corrections.js` have been failing ever since and NOBODY
/// SAW IT, because `app/ui/tests` had no `node_modules` and eighteen of the nineteen suites
/// could not start.
///
/// The affordance scrape (`tests/lib/state-strings.js::RUST_CEO_CONTEXT`) finds a CEO-facing
/// sentence by the shape of the code around it — `Err(`, `ok_or`, `.into()`, or a
/// `const … : &str =` on the line above. Three lines below an `ok_or_else(|| {`, inside a
/// `String::from(`, it matched none of them, so the string was invisible to the very
/// machinery that exists to notice it drifting. As a const it is found the same way
/// [`LEASE_UNAVAILABLE_MESSAGE`] is, and `mock.js` copies it the way `_notConnected` copies
/// that one.
///
/// The candidate list `desk()` appends is deliberately NOT part of it: the list is a fact
/// about one machine, this is the sentence.
const LORO_DESK_ABSENT_MESSAGE: &str =
    "This install has no company memory it can write to, so there is nothing to read or \
     correct here. That is a statement about this install, not about what is recorded.";

/// What the CEO is told when the repository root does not deterministically select one
/// entity. UX §21 "Entity binding failure": state that Rich cannot safely determine which
/// entity the work belongs to, and require an explicit choice. Never default.
///
/// THAT DAY IS TODAY. This sentence used to end "Set RICHOS_ENTITY to one of femcboost,
/// deeply, prospects or richos, or launch me from that entity's repository root", and it
/// carried its own note saying it was safe only because nothing in `app/ui/main.js`
/// invoked a command that raises it — *"the day a slice wires one of those commands to a
/// button, this sentence becomes a terminal instruction addressed to a man who does not
/// open terminals"*. The correction desk wires three of them
/// (`loro_pending_corrections`, `loro_propose_correction`, `loro_confirm_correction`), so
/// the sentence is now rewritten to the register `LEASE_UNAVAILABLE_MESSAGE` already uses:
/// it says what it will not do and why, it names who owns the fix, and it invents no
/// control — because there genuinely is none in the app. An environment variable is not an
/// action the CEO can take, and telling him to take it would be the same request-wearing-a-
/// status's-clothes defect one more time.
///
/// The operator has NOT lost the instruction. `RICHOS_ENTITY` is still exactly right for
/// whoever set RichOS up, and it is still printed at boot — beside this sentence, at the
/// one site whose audience is a terminal (see the `eprintln!` in `setup`). Two audiences,
/// two sentences, one condition.
///
/// THE LAST CLAUSE CHANGED ON 2026-09-01, AND IT HAD TO. It read *"It isn't something you
/// can set from in here: whoever set RichOS up has to tell me which company this copy of me
/// works for"* — true when it was written, and false the moment the picker landed. It is
/// now something he sets from in here, in two places (the launch picker and the
/// preferences popover), so the sentence names the control instead of naming a party who
/// no longer owns it. A state's copy going stale in the direction of "you cannot do this"
/// is the worse direction: it teaches him not to look for a control that is right there.
const ENTITY_UNRESOLVED_MESSAGE: &str =
    "I can't tell which company this work belongs to, so I won't guess. Filing it under \
     the wrong one would mix two companies' records together and that's not a mistake \
     worth risking to save you a question. Pick the company and I'll keep everything under \
     it from then on.";

/// WHERE A RESOLVED ENTITY CAME FROM. Reported to the CEO's settings surface and printed
/// at boot, because "RichOS is filing this under RichOS" and "somebody set an environment
/// variable that RichOS cannot change from in here" are different facts about the same
/// entity id, and a surface that renders a control over the second one is lying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntitySource {
    /// `RICHOS_ENTITY` — an explicit operator statement, made outside the app.
    Environment,
    /// The answer the CEO gave the picker, kept in `config.json`.
    SavedChoice,
    /// Deterministic repository-root containment (ECS §3.3/§10.2) — the dogfood path.
    WorkingDirectory,
}

impl EntitySource {
    /// The wire string. Kebab-case and stable: `app/ui/main.js` switches on it.
    fn as_str(&self) -> &'static str {
        match self {
            EntitySource::Environment => "environment",
            EntitySource::SavedChoice => "saved-choice",
            EntitySource::WorkingDirectory => "working-directory",
        }
    }

    /// How the boot log names it, for the operator reading a terminal.
    fn describe(&self) -> &'static str {
        match self {
            EntitySource::Environment => "RICHOS_ENTITY",
            EntitySource::SavedChoice => "the saved choice",
            EntitySource::WorkingDirectory => "the working directory",
        }
    }
}

/// The outcome of entity resolution: what was resolved, where it came from, and every
/// line the operator should see about it.
///
/// `notes` rather than `eprintln!` inside the resolver so the ORDER is a pure function of
/// its inputs and can be asserted by a test. The resolver that this replaces read
/// `std::env::var` and `std::env::current_dir()` directly, which is precisely why nothing
/// ever tested it under the one condition that matters — a Finder launch, where both are
/// answers no developer's shell ever produces.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BootEntity {
    entity: Option<EntityId>,
    source: Option<EntitySource>,
    notes: Vec<String>,
}

/// Resolve this launch's entity area (ECS §3.3/§10.2), deterministically and fail-closed.
///
/// **THE ORDER, and why each step is where it is.** This is slice 4 — the picker the
/// comment at `create_thread` has named since slice 1 — so there is now a fourth step, and
/// the fourth step is the one that makes the other three safe to keep fail-closed.
///
///   1. **`RICHOS_ENTITY`** — an explicit operator statement, made deliberately, from
///      outside the app. It wins, it is still validated against the registry, and an
///      unregistered value STILL REFUSES rather than falling through: someone who names an
///      entity meant it, and quietly resolving a different one would be worse than
///      stopping.
///   2. **The saved choice** — the answer the CEO gave the picker, read from `config.json`.
///      It outranks the working directory because it is a person's stated answer and a
///      working directory is an accident of how the process was started. A value that no
///      longer parses or is no longer registered is STALE DATA rather than a statement, so
///      it is named in a note and falls through to (3), which is deterministic.
///   3. **Working-directory containment** — unchanged. Every dogfood launch behaves today
///      exactly as it did yesterday, because on those machines nothing has ever been saved
///      at (2): the `entity` key did not exist until this pass.
///   4. **Nothing** — `None`, which still blocks every send. The difference this pass makes
///      is not that an unresolved entity is now allowed through; it is that the CEO can
///      always REACH one, through the picker, which writes (2).
///
/// Pure on purpose: every input is an argument, so the GUI condition (`cwd = /`, no
/// environment, empty config) is a test rather than a build-and-double-click.
fn resolve_boot_entity(
    registry: &EntityRegistry,
    env_value: Option<&str>,
    saved: Option<&str>,
    cwd: Option<&Path>,
) -> BootEntity {
    let mut notes: Vec<String> = Vec::new();

    if let Some(explicit) = env_value {
        return match EntityId::parse(explicit.trim()) {
            Ok(id) if registry.contains(&id) => {
                BootEntity { entity: Some(id), source: Some(EntitySource::Environment), notes }
            }
            _ => {
                notes.push(format!(
                    "RICHOS_ENTITY={explicit:?} is not a registered entity — refusing it"
                ));
                BootEntity { entity: None, source: None, notes }
            }
        };
    }

    if let Some(raw) = saved {
        match EntityId::parse(raw.trim()) {
            Ok(id) if registry.contains(&id) => {
                return BootEntity {
                    entity: Some(id),
                    source: Some(EntitySource::SavedChoice),
                    notes,
                };
            }
            // NAMED, NOT DROPPED. A saved choice that no longer resolves is the one state
            // in which the CEO believes he has answered this question and has not.
            _ => notes.push(format!(
                "the saved company {raw:?} is not a registered entity any more — ignoring it and asking again rather than filing work under a guess"
            )),
        }
    }

    match cwd {
        Some(dir) => match registry.resolve_root(dir) {
            Ok(entity) => BootEntity {
                entity: Some(entity.id.clone()),
                source: Some(EntitySource::WorkingDirectory),
                notes,
            },
            Err(e) => {
                notes.push(format!("entity not resolved from {}: {e}", dir.display()));
                BootEntity { entity: None, source: None, notes }
            }
        },
        None => {
            notes.push("no working directory could be read".to_string());
            BootEntity { entity: None, source: None, notes }
        }
    }
}

/// The entity ids that ALREADY OWN THREADS in this install's ledger, in first-seen order.
///
/// The migration's only input, and the reason it asserts nothing about anybody's business:
/// it reads what is durably on this machine. `ThreadSummary::entity_id` is `None` for a
/// pre-entity legacy thread, which is quarantined by construction (`ThreadEntity::Unbound`)
/// and contributes nothing here.
fn entity_ids_already_in_the_ledger(spine: &Spine) -> Vec<EntityId> {
    let mut seen: Vec<EntityId> = Vec::new();
    for summary in spine.threads() {
        let Some(raw) = summary.entity_id.as_deref() else { continue };
        let Ok(id) = EntityId::parse(raw) else { continue };
        if !seen.contains(&id) {
            seen.push(id);
        }
    }
    seen
}

/// The process-reading wrapper around [`resolve_boot_entity`]. Everything it knows it reads
/// here and hands over as an argument; it holds no logic of its own.
fn boot_entity(registry: &EntityRegistry, config: &ConfigStore) -> BootEntity {
    let env_value = std::env::var("RICHOS_ENTITY").ok();
    let cwd = std::env::current_dir().ok();
    let resolved = resolve_boot_entity(
        registry,
        env_value.as_deref(),
        config.entity_raw(),
        cwd.as_deref(),
    );
    for note in &resolved.notes {
        eprintln!("[richos] {note}");
    }
    resolved
}

/// Where this launch's engine directory is — the working directory `claude` is started in.
///
/// The seven-candidate order, and the reasoning behind it, is `engine.rs`. This wrapper holds
/// the one decision the resolver deliberately does not make: what to hand
/// `NativeCognition::start` when NOTHING was found.
///
/// **It hands over the last place it looked, and it never invents a plausible path.** A launch
/// with no engine has to fail, and it has to fail naming a real candidate an operator can act
/// on — `native.rs::preflight` turns that into `the engine directory <path> does not exist`,
/// which is a true sentence about a path that was genuinely checked. Synthesizing a default
/// instead is exactly how `cwd/../engine` became `/../engine` under a Finder launch and got
/// reported as a missing `claude` binary.
fn resolve_engine() -> engine::EngineResolution {
    let mut resolution = engine::resolve_engine_dir(&engine::LaunchPaths::from_process());
    if resolution.dir.is_none() {
        resolution.dir = resolution.tried.last().map(|(_, path)| path.clone());
    }
    resolution
}

/// OPEN THE LORO CORRECTION DESK AND WIRE IT IN — the only place that does, called from
/// both `setup` and [`provision_memory`].
///
/// **It exists because it was called from one place and needed to be called from two.**
/// `AppState::correction` used to be fixed at boot, so a customer who answered "set my
/// memory up" got a corpus, a working read half, and a correction desk that stayed shut
/// until he quit and reopened. The read half already re-wired without a relaunch, through
/// `memory::wire_company_memory`, for exactly the reason this function now exists: the
/// alternative is two copies of the same wiring, and the one that drifts is always the one
/// a new customer hits first.
///
/// Four things happen here and all four have to happen together, which is the other reason
/// this is a function and not a comment:
///
///   1. the desk's own fsync'd log is opened beside the ledger — failure REFUSES rather
///      than pretending, because a desk that cannot record a proposal durably would lose
///      the CEO's answer across a relaunch;
///   2. the spine gets the desk, so the belief trigger has somewhere to file;
///   3. the spine gets the proposal observer, so a proposal raised inside a two-hour turn
///      moves the badge during it rather than at the next open;
///   4. a desk with no context compiler says so — nothing was ever put in front of Rich, so
///      no correction can name a record and the trigger would sit silent looking wired.
fn install_correction_desk(
    spine: &mut Spine,
    app: &tauri::AppHandle,
    data_dir: &Path,
    writer: CliLoroWriter,
) -> Option<SharedCorrectionDesk> {
    let desk = match CorrectionDesk::open(data_dir.join("loro-corrections.jsonl"), Box::new(writer)) {
        Ok(d) => d.shared(),
        Err(e) => {
            eprintln!(
                "[richos] loro correction desk unavailable, corrections will refuse rather than \
                 pretend: {e}"
            );
            return None;
        }
    };

    // WHAT HE ALREADY DECIDED AND THIS BUILD COULD NOT READ, ON THE BOOT LINE.
    //
    // The same shape as the ledger's pair above and the intake log's below, and for the
    // same reason: `CorrectionDesk::replay` prints the operator's account (a line per
    // skipped record plus a summary), and this is the CEO's, in the sentences a surface
    // would render — so a boot log and a screen say the same thing rather than two versions
    // of it.
    //
    // IT IS A THIRD STATEMENT, not a variant of either. The ledger's says part of the
    // conversation on screen did not load. The intake log's says something he TYPED never
    // reached Rich. This one says a decision he was asked to make, and the answer he gave,
    // could not be read — and the live consequence is the one worth the extra pair of
    // lines: without the hold `CorrectionDesk` now applies, a correction he CONFIRMED
    // reverts to pending and is put in front of him a second time.
    //
    // NO NEW SURFACE IS BUILT HERE, by the same ruling that governs the intake log's line:
    // one more sentence inside the notice that already exists, not a section of its own.
    // `CorrectionDesk::desk_health`, `::unresolved` and `::held_records` are the seams a
    // surface would read when there is a decision about what that surface should be.
    {
        let h = desk.lock().unwrap().desk_health();
        if !h.is_clean() {
            eprintln!("[richos] {}", h.headline);
            eprintln!("[richos] {}", h.detail);
        }
    }

    spine.set_correction_desk(std::sync::Arc::clone(&desk));
    spine.set_proposal_observer(Box::new(TauriProposalEmitter { app: app.clone() }));
    if !spine.has_loro_context_compiler() {
        eprintln!(
            "[richos] loro correction desk is open but no context compiler is configured — \
             nothing was ever put in front of Rich, so no correction can name a record and the \
             trigger will stay silent"
        );
    }
    Some(desk)
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some(richos_core::quota::terminal::HEADLESS_PROTOCOL) {
        std::process::exit(richos_core::quota::terminal::run(std::env::args().skip(2).collect()));
    }
    if std::env::args().nth(1).as_deref() == Some("--claude-reset-mcp") {
        let args: Vec<_> = std::env::args_os().skip(2).collect();
        let result = if args.len() == 3 {
            richos_core::quota::reset_tools::run_stdio(Path::new(&args[0]), Path::new(&args[1]), Path::new(&args[2]))
        } else { Err(std::io::Error::other("quota MCP requires a scope, data directory and Claude executable")) };
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    if std::env::args().nth(1).as_deref() == Some("--claude-quota-gate") {
        std::process::exit(richos_core::quota::gate::run_cli());
    }
    if std::env::args().nth(1).as_deref() == Some("--richos-connect-guard") {
        std::process::exit(phone::connect::supervisor::guard_main());
    }
    // The update relaunch helper: waits for the app to quit, then starts it again so the
    // downloaded update activates. Before any lease or alert, like the guard above.
    if let Some(code) = update_relaunch::helper_main() {
        std::process::exit(code);
    }
    // This is compile-generated metadata, before any application runtime exists.
    // Use the same merged build version for the probe, startup and final app.
    let context = tauri::generate_context!();
    let compiled_version = context.package_info().version.to_string();
    // =====================================================================================
    // THE FIRST LINE THAT CAN SPEAK — BECAUSE EVERYTHING BELOW IT COULD DIE IN SILENCE
    // =====================================================================================
    //
    // Until this line existed, every failure between here and the window was an `eprintln!`
    // and a `return`. Launched from Finder or the Dock there is no stderr anybody reads, so
    // the whole of the user's experience was an icon that bounced once and stopped.
    //
    // `startup_alert` installs a panic hook and arms a native alert, and its header carries
    // the enumeration of all twelve sites in the class, the five alternatives that were
    // rejected, and the measurement behind the one that was chosen. It is FIRST because a
    // hook installed after the thing it must catch catches nothing.
    //
    // `generate_context!()` on the line above is compile-generated construction with nothing
    // to fail at run time, which is why one line of `main` is deliberately outside the net.
    startup_alert::install(&context.config().identifier, &compiled_version);
    // Before any window, and before the dictation tool branches off, so both processes have it.
    webview_menu::install();
    // THE DICTATION TOOL (dictation plan section 6): after the alert's panic hook, so a panic is
    // still recorded, and BEFORE `update_startup::prepare`, so the tool never takes the update
    // lease, never opens a launch record and never builds the main window.
    #[cfg(target_os = "macos")]
    if std::env::args().nth(1).as_deref() == Some(dictation::tool::ARG) {
        let rest: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(dictation::tool::main(context, &rest));
    }
    if update_startup::identity_probe(&compiled_version) {
        return;
    }
    // Keep the lease alive through all Tauri state and worker lifetimes. This
    // runs before any runtime thread exists, so exec cannot abandon accepted work.
    let _update_session = match update_startup::prepare(&compiled_version) {
        Ok(session) => session,
        Err(error) => {
            // SHAPE 1 of the class `startup_alert` enumerates, and the one `tom-opus-ui1`
            // reported: nine real failure strings that no surface could render, because
            // this runs before Tauri exists and a Finder launch has no stderr.
            //
            // The person's sentence is deliberately the SAME for all nine. Every one of them
            // is this process deciding which copy of RichOS is the right one to run and being
            // unable to finish that decision — so one honest sentence covers them, and the
            // error itself, which is an engineer's sentence, goes to the log verbatim.
            startup_alert::cannot_start(
                &format!("application startup: {error}"),
                "RichOS stopped before it could open a window. It was working out which copy \
                 of itself to run and that step did not finish, so it closed itself rather \
                 than start in a state it could not vouch for.\n\nOpening RichOS again is \
                 worth one try.",
            );
            // AND IT EXITS NON-ZERO, which the bare `return` here did not.
            //
            // The silence had a second half nobody had looked at: `return` from `main` is
            // exit 0, so a script that ran this binary and checked `$?` was told the app
            // started. MEASURED before the change — the same damaged bundle, run with a
            // parent holding it: `exit=0 elapsed=0s`, one line on stderr, and a success code.
            // Neither `make-release.sh` nor `rebuild-survival.sh` nor `gui-boot.test.sh` reads
            // this code (checked 2026-09-10; gui-boot kills the process it boots and
            // make-release greps the executable rather than running it), so 1 costs nothing
            // and stops the next harness being lied to.
            std::process::exit(1);
        }
    };
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    // The ASSIGNMENT REGISTER's stdio server (`richos-core`'s `assignment_tools.rs`), in
    // the same shape and for the same reason as the onboarding one below it: this
    // executable, spawned as a child of the running app, reading a scope the app wrote for
    // the current turn. It is how a conversation turn ENDS on a receipt (background-work
    // spec §1.1), and a failure here is stderr its parent captures plus the startup log —
    // never an alert, because `activation.rs`'s parent-pid condition is false by
    // construction in a child.
    if first.as_deref() == Some(std::ffi::OsStr::new("--questions-mcp")) {
        let result=args.next().ok_or_else(||"Missing question scope".to_string())
            .and_then(|scope|richos_core::question_tools::run_stdio(Path::new(&scope)).map_err(|e|e.to_string()));
        if let Err(error)=result {eprintln!("Question tool server: {error}");std::process::exit(1);}
        return;
    }
    if first.as_deref() == Some(std::ffi::OsStr::new("--assignments-mcp")) {
        let result = args.next().ok_or_else(|| "Missing assignment scope".to_string())
            .and_then(|scope| richos_core::assignment_tools::run_stdio(Path::new(&scope))
                .map_err(|e| e.to_string()));
        if let Err(error) = result {
            startup_alert::cannot_start(
                &format!("assignment tool server: {error}"),
                "RichOS could not start the helper it uses to write down work you have asked for.",
            );
            std::process::exit(1);
        }
        return;
    }
    // The FRONT DESK'S READ (`richos-core`'s `status_tools.rs`), in the same shape as the
    // two beside it. The CEO's Two Riches page takes the orchestration tools off the
    // conversation lease; this is what it answers "what is running", "what is waiting for
    // me" and "what finished" with instead, and it reads the record on disk without calling
    // the back end or changing anything.
    if first.as_deref() == Some(std::ffi::OsStr::new("--status-mcp")) {
        let result = args.next().ok_or_else(|| "Missing status scope".to_string())
            // **The screen reader goes with it** — the CEO's ruling §56. This arm IS the app's
            // own executable, so the same `MacScreen` the work host uses is available here,
            // and the front desk's answer carries the screen as it is right now beside the
            // work that is waiting for it. A short-lived child of the GUI session reads the
            // same session dictionary the app does.
            .and_then(|scope| richos_core::status_tools::run_stdio(Path::new(&scope), &screen::MacScreen)
                .map_err(|e| e.to_string()));
        if let Err(error) = result {
            startup_alert::cannot_start(
                &format!("status tool server: {error}"),
                "RichOS could not start the helper it uses to look at work that is already running.",
            );
            std::process::exit(1);
        }
        return;
    }
    // HIS LEAD'S REPORT TOOL (`richos-core`'s `operator_report.rs`; operator back-end spec r2
    // (c)), the same shape as the three servers around it. Only an operator lead is ever given
    // it (`operator_profile::mcp_config`), and only on an install whose `operator.json` passed
    // the gate; nothing in the product path names this argument.
    if first.as_deref() == Some(std::ffi::OsStr::new("--operator-mcp")) {
        let result = args.next().ok_or_else(|| "Missing report scope".to_string())
            .and_then(|scope| richos_core::operator_report::run_stdio(Path::new(&scope))
                .map_err(|e| e.to_string()));
        if let Err(error) = result {
            startup_alert::cannot_start(
                &format!("operator report server: {error}"),
                "RichOS could not start the helper your team reports to you through.",
            );
            std::process::exit(1);
        }
        return;
    }
    // HIS TEAM'S TOOLS ON THE FRONT DESK (`richos-core`'s `operator_desk_tools.rs`; the
    // operator-client record's §7 item 2): stop named agents, stop his team's turn, read what it
    // is doing. Only a front desk on an install whose `operator.json` passed the gate at launch
    // is ever given it (`EngineProfile::operator_desk`); nothing in the product path names it.
    if first.as_deref() == Some(std::ffi::OsStr::new("--operator-desk-mcp")) {
        let result = args.next().ok_or_else(|| "Missing operator scope".to_string())
            .and_then(|scope| richos_core::operator_desk_tools::run_stdio(Path::new(&scope))
                .map_err(|e| e.to_string()));
        if let Err(error) = result {
            startup_alert::cannot_start(
                &format!("operator desk tool server: {error}"),
                "RichOS could not start the helper it uses to stop or check on your team.",
            );
            std::process::exit(1);
        }
        return;
    }
    if first.as_deref() == Some(std::ffi::OsStr::new("--onboarding-mcp")) {
        let result = args.next().ok_or_else(|| "Missing onboarding scope".to_string())
            .and_then(|scope| richos_core::onboarding_tools::run_stdio(Path::new(&scope))
                .map_err(|e| e.to_string()));
        if let Err(error) = result {
            // The second SHAPE 1 site. This one is a stdio tool server the RUNNING app spawns
            // as a child, so `activation`'s P condition (parent pid 1) is false by
            // construction and the alert is never armed here — stderr, which its parent
            // captures, stays the right audience and it goes on being the audience.
            //
            // It is routed through the same call anyway, and that is the point: the report
            // now also reaches `~/Library/Logs/RichOS/startup.log`, where a failure of the
            // onboarding helper can be read after the fact instead of dying inside a pipe.
            // No site in this class gets to decide for itself whether it is worth recording.
            startup_alert::cannot_start(
                &format!("onboarding tool server: {error}"),
                "RichOS could not start the helper it uses to set up a new company.",
            );
            std::process::exit(1);
        }
        return;
    }
    tauri::Builder::default()
        // =================================================================================
        // THE MENU, AND THE ONE ITEM IN IT THIS APP HAS TO OWN — background-work spec §2.5
        // =================================================================================
        //
        // **Why a menu at all, when this app has never built one.** §2.5 needs Quit to be
        // preventable, and today's Quit is not: with no menu set, Tauri installs
        // `Menu::default` (`tauri-2.11.5/src/app.rs:2244-2249`), whose app submenu ends in
        // `PredefinedMenuItem::quit` (`menu/menu.rs:194`), which muda maps to
        // `sel!(terminate:)` (`muda-0.19.3/src/platform_impl/macos/mod.rs:994`). That
        // selector goes to `applicationWillTerminate`, which cannot cancel — so a Cmd-Q
        // arrives already committed and the work dies without a word.
        //
        // **So exactly one item changes and everything else is the platform's.** The About,
        // Services, Hide, Edit, View, Window and Help items are the same predefined ones
        // `Menu::default` builds, in the same order — copying them is not decoration: an app
        // that sets a menu REPLACES the default entirely, and a webview without the Edit
        // submenu loses Cmd-C and Cmd-V, which would be a far worse regression than the one
        // being fixed. Only Quit becomes ours, keeping its name and its Cmd-Q accelerator.
        .menu(|handle| {
            use tauri::menu::{AboutMetadata, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
            let name = handle.package_info().name.clone();
            let about = AboutMetadata {
                name: Some(name.clone()),
                version: Some(handle.package_info().version.to_string()),
                ..Default::default()
            };
            // OURS. The id is what `on_menu_event` matches, and the label and accelerator
            // are what AppKit's own Quit had, so nothing about it looks different to him.
            let quit = MenuItemBuilder::with_id(MENU_QUIT, format!("Quit {name}"))
                .accelerator("CmdOrCtrl+Q")
                .build(handle)?;
            let app_menu = SubmenuBuilder::new(handle, &name)
                .item(&PredefinedMenuItem::about(handle, None, Some(about))?)
                .separator()
                .services()
                .separator()
                .hide()
                .hide_others()
                .separator()
                .item(&quit)
                .build()?;
            let edit = SubmenuBuilder::new(handle, "Edit")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;
            let view = SubmenuBuilder::new(handle, "View").fullscreen().build()?;
            let window = SubmenuBuilder::new(handle, "Window")
                .minimize()
                .maximize()
                .separator()
                .close_window()
                .build()?;
            MenuBuilder::new(handle)
                .items(&[&app_menu, &edit, &view, &window])
                .build()
        })
        .on_menu_event(|app, event| {
            if event.id() == MENU_QUIT {
                // **STEP ONE OF TWO (§2.5a).** Nothing exits here. This asks the register,
                // and either raises the app's own exit — which re-enters the `ExitRequested`
                // arm below, the preventable path — or puts the question on his screen.
                request_quit(app);
            }
        })
        // THE OUTPUT PANEL'S PREVIEW SCHEME (Output side panel PRD §5.3):
        // `richos-output://<output id>`, resolved against the active thread's record and
        // checked before a byte is served (`output_files.rs`). Asynchronous, so a file is never
        // read on the main thread.
        .register_asynchronous_uri_scheme_protocol(output_files::SCHEME, output_files::scheme_handler)
        .setup(|app| {
            // The user's terminal environment, read once from the login shell for every lease
            // (build plan item 4), started now so the first lease does not wait for it.
            richos_core::login_environment::warm();
            // Durable ledger lives in the app data dir (survives restart + rotation).
            let data_dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir());
            std::fs::create_dir_all(&data_dir).ok();

            // Synthesis is independent of recognition and video transcription. Start it before
            // the window and the rest of setup, even when no speech model is installed yet.
            if updates::selftest_mode().is_none() {
                voice_provision::fetch_rich_voice_in_background();
            }

            // =====================================================================
            // WHETHER THIS LAUNCH MAY TAKE THE SCREEN — BEFORE THE WINDOW EXISTS
            // =====================================================================
            //
            // THE RULE, and `activation.rs` is where it is argued: RichOS activates, takes
            // the keyboard and appears in the Dock ONLY when it can positively establish
            // that it is an installed launch by the person who installed it. Everything
            // else is accessory — a real, driveable window, with no Dock icon, no
            // activation and nothing taken from what he is typing into.
            //
            // THE COMPLAINT THIS ANSWERS, in the CEO's words on 2026-09-06: *"It opens in
            // the foreground on the main monitor AND takes away focus from the current app.
            // So, if I'm typing something here, it takes away focus from here and focuses
            // on that app."* — several times in a row, because a harness that proves an
            // assignment survives a restart has to boot, kill and reboot the binary.
            //
            // AND WHY IT IS THE INVERSE OF "DETECT A TEST", which is what it was asked for
            // first. His second sentence is the design: *"GENERAL issue if and when the
            // same or similar tests are run by others in the future."* Detecting a test
            // means keeping a list of test shapes, and the author of the next harness is
            // not on it and never will be. Identifying the INSTALLED LAUNCH positively
            // needs no list, and a harness nobody has written yet is simply one more thing
            // that is not one.
            //
            // IT IS HERE, AND NOT LOWER DOWN, for the same reason the window is first:
            // `set_activation_policy` has to be in force before the window is built, and
            // the window has to be built knowing whether it may take focus. It is also
            // AFTER `data_dir` is final, because "the data directory this boot will
            // actually write to" is one of the three facts, and reading it from anywhere
            // but the variable in use would be a second copy that can disagree.
            let activation = activation::decide(&activation::gather(&data_dir, &app.config().identifier));
            eprintln!("[richos] {}", activation.log_message());
            // THE SAME DECISION, NOW AUTHORITATIVE, RE-ARMS THE FAILURE ALERT.
            //
            // `startup_alert::install` armed itself before Tauri existed and could only
            // APPROXIMATE condition D — it computed the data directory `app_data_dir()` was
            // going to return instead of reading it. This line is the first moment the real
            // `data_dir` is known, so the approximation is replaced rather than left standing.
            // One decision, two readings, and the later one wins.
            startup_alert::rearm(activation.presentation);
            #[cfg(target_os = "macos")]
            {
                // `Accessory` is `NSApplicationActivationPolicyAccessory`: no Dock icon and
                // no menu bar. LaunchServices reports it as `type="UIElement"` where a
                // normal launch reports `type="Foreground"` — measured both ways on
                // 2026-09-06, `docs/verification/activation-2026-09-06/`.
                //
                // IT IS ONE OF TWO LEVERS AND IT IS NOT THE ONE THAT SAVES HIS KEYSTROKES.
                // Setting this policy and building the window unfocused was the obvious fix
                // and it DOES NOT WORK: measured, `lsappinfo front` sampled every 250 ms
                // still reported `richos-tauri` frontmost for 22 of 24 samples. tao takes
                // the screen twice at `applicationDidFinishLaunching`, and neither call
                // consults the policy or the window's requested focus
                // (tao-0.35.3/src/platform_impl/macos/app_state.rs:284-299):
                //
                //   * `window_activation_hack` (`app_state.rs:432-453`) sends
                //     `makeKeyAndOrderFront:` to EVERY VISIBLE window it finds — which is
                //     what defeats `.focused(false)`. It skips invisible ones by name
                //     ("Skipping activating invisible window"), and that is the seam the
                //     `.visible(...)` line below uses.
                //   * `ns_app.activateIgnoringOtherApps(ignore)` runs unconditionally, with
                //     `ignore` defaulting to `true` (`app_delegate.rs:107`). It is settable
                //     only through tao's `EventLoopExtMacOS::set_activate_ignoring_other_apps`
                //     (`platform/macos.rs:336`), which Tauri does not expose — so there is
                //     no way to ask it not to from here.
                //
                // The two levers were ABLATED against each other rather than both adopted
                // on faith. Hidden window, policy NOT set: focus stayed put for 32 of 32
                // samples but LaunchServices reported `type="Foreground"` — a Dock icon.
                // Hidden window, policy set: 32 of 32 and `type="UIElement"`. So the hidden
                // window is what keeps his keyboard and this policy is what keeps the Dock
                // clean, and neither is decoration.
                //
                // Only macOS has a policy to set; a port must decide its own equivalent
                // rather than inherit silence (`activation.rs`, WHAT THIS FILE DOES NOT
                // COVER).
                if activation.presentation == activation::Presentation::Accessory {
                    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                }
            }

            // =====================================================================
            // THE LAUNCH RECORD, AND THE WINDOW — FIRST, AND FOR ONE REASON
            // =====================================================================
            //
            // `app/ui/splash.js` decides whether to draw the opening screen on its FIRST
            // synchronous line, before anything can be awaited. So the one fact it needs —
            // is this a fresh launch, a crash-restart, or a second window — has to already
            // be in the page. Tauri creates config-declared windows BEFORE this hook runs
            // (tauri-2.11.5 `src/app.rs:2524`), which is why `tauri.conf.json` now carries
            // `"create": false` and the window is built here instead: `from_config` keeps
            // every property the config declares and this adds exactly one line of script.
            //
            // IT IS FIRST IN THIS FUNCTION, NOT WHEREVER IT FITTED. Everything below —
            // ledger replay, machinery eviction, loro — used to happen with the window
            // already on screen. Moving window creation into `setup` puts all of it AHEAD
            // of first paint unless the window goes first, so the window goes first and
            // what precedes it is one small file read and one small file write. Measured on
            // this machine, `launches.json` at 100 starts is under 4 KB
            // (`launch.rs::a_hundred_launches_stay_in_order_and_cost_a_few_kilobytes`).
            //
            // `PriorRun::Unknown` is the honest answer and not a placeholder: this shell
            // does not check whether the process that left a marker behind is still alive,
            // so it says so rather than guessing, and `launch.rs` reads Unknown as a
            // crash-restart — not counted, no splash, back where he was.
            let mut launch_store = LaunchStore::open(data_dir.join("launches.json"), richos_core::util::now_millis())
                .expect("open launch record");
            if let Some(why) = launch_store.unreadable_reason() {
                eprintln!("[richos] launch record: {why}");
            }
            let launch_kind = launch_store
                .begin_run(richos_core::util::now_millis(), std::process::id().to_string(), PriorRun::Unknown)
                .unwrap_or(LaunchKind::Fresh);
            // WHICH START THIS IS, for the splash's selection rule. Read from the ledger
            // that was just written, one line above — never a second counter. `None` when
            // this is not a start or when the record would not parse, and the webview reads
            // a missing ordinal as the first start, which shows splash #1.
            let start_ordinal = launch_store.start_ordinal();
            let window_configs = app.config().app.windows.clone();

            // =====================================================================
            // HOW BIG IT OPENS, AND WHERE — ASKED OF THE DISPLAY
            // =====================================================================
            //
            // `docs/hardware-choices-2026-09-10.md` D2. The window used to open at the
            // 1400 x 880 written in `tauri.conf.json`, chosen for a screen nobody measured,
            // on a desk where one of the three displays is 1080 points wide:
            //
            //     1400 - 1080 = 320 points of the window past the edge
            //
            // and `minWidth: 1024` did not help, because it constrains dragging rather than
            // the size the window opens at. The audit's own sweep is what makes this the
            // real fix and not a smaller number: `app/src-tauri/src/` contained ZERO calls
            // that read anything about the machine. These two lines are the first.
            //
            // THE GEOMETRY IS DECIDED BEFORE THE WINDOW EXISTS AND APPLIED AT CONSTRUCTION,
            // for the same reason `.focused()` is asked for at construction thirty lines
            // below rather than corrected afterwards: `set_size`/`set_position` after
            // `build()` would put the wrong window on his screen first and then move it. On
            // an installed launch the window is visible from the instant it is built, so
            // there must be no instant in which the wrong geometry is on it.
            //
            // The saved geometry is offered to the FIRST window only. A second window that
            // restored the same rect would open exactly on top of the first.
            let displays = read_displays(app.handle());
            let geometry_path = data_dir.join("window.json");
            let mut saved = window_geometry::GeometryStore::new(&geometry_path).load();
            for window_config in &window_configs {
                let kind = launch_store.next_window_kind();
                let preference = window_geometry::Preference {
                    width: window_config.width,
                    height: window_config.height,
                    min_width: window_config
                        .min_width
                        .unwrap_or(window_geometry::PREFERRED_MIN_WIDTH),
                    min_height: window_config
                        .min_height
                        .unwrap_or(window_geometry::PREFERRED_MIN_HEIGHT),
                };
                let placement =
                    window_geometry::decide_with(&displays, saved.take().as_ref(), preference);
                eprintln!("[richos] window: {}", placement.describe());
                let mut builder = tauri::WebviewWindowBuilder::from_config(app.handle(), window_config)?
                    // THE SIZE THE DISPLAY CAN HOLD, not the size the config asked for. On
                    // his 1920-wide displays these are still 1400 x 880 — the constant was
                    // right there and stays right; on the 1080-wide portrait panel they are
                    // 1032 x 880 and the window is fully on screen.
                    .inner_size(placement.width, placement.height)
                    // AND THE FLOOR IS DERIVED TOO. `minWidth: 1024` is a preference, and a
                    // preference wider than the screen is the same defect one level down: a
                    // window that cannot be made small enough to fit is a window that does
                    // not fit.
                    .min_inner_size(placement.min_width, placement.min_height)
                    // The durable opening-screen answer travels with the launch kind, because
                    // the curtain decides in the `<head>` and no command can answer by then
                    // (audit-8 row 5 — see `launch_init_script`).
                    .initialization_script(launch_init_script(
                        kind,
                        start_ordinal,
                        durable_splash_enabled(&data_dir),
                    ))
                    // The right-click guard in every frame, the PDF preview's too (nightly 66).
                    .initialization_script_for_all_frames(webview_menu::EVERY_FRAME)
                    // FOCUS IS ASKED FOR AT CONSTRUCTION, not corrected afterwards, so
                    // there is no instant in which the keyboard moved and came back.
                    .focused(activation.presentation == activation::Presentation::Regular)
                    // AND THE WINDOW ITSELF DOES NOT GO ON HIS SCREEN, which is the half
                    // that actually works. `.focused(false)` alone is overridden a moment
                    // later by tao's `window_activation_hack`, which sends
                    // `makeKeyAndOrderFront:` to every VISIBLE window (measurement and
                    // citations at the `set_activation_policy` block above). It skips
                    // invisible windows, so an unattended boot builds one that is never
                    // ordered on screen — no window in the foreground on his main monitor,
                    // which was the first half of his complaint, and no keystroke taken,
                    // which was the second.
                    //
                    // THE WINDOW IS STILL REAL AND STILL DRIVEABLE, and that is measured
                    // rather than asserted: the boot log of an unattended run still carries
                    // `[richos] voice: not offered on this machine — …`, which is printed
                    // by `voice_readiness` — a `#[tauri::command]` the PAGE invokes before
                    // it renders its greeting. Its presence means the WKWebView was
                    // created, the frontend loaded, JavaScript ran and an IPC round trip
                    // completed into Rust. LaunchServices shows the three WebKit XPC
                    // services ("Web Content", "Graphics and Media", "Networking") running
                    // alongside it. A harness that needs the window on screen calls
                    // `show()`, or asks for the whole normal treatment with
                    // `RICHOS_ACTIVATION=regular`.
                    //
                    // Everything else `from_config` declared — the size, the centered
                    // position, the minimums, `zoomHotkeysEnabled` — is untouched, and on
                    // an installed launch this line reduces to the config's own `true`.
                    .visible(activation.presentation == activation::Presentation::Regular);
                // WHERE, and it is the position that decides WHICH DISPLAY: tao resolves the
                // screen a new window belongs to from the requested position
                // (`tao-0.35.3/src/platform_impl/macos/window.rs:340-355`,
                // `screen_from_position`). It is `None` on exactly one path — nothing about
                // the displays could be read — and there the platform's own centering beats
                // a coordinate invented against no screen.
                //
                // `"center": true` is GONE from `tauri.conf.json` and its absence is pinned
                // by a test, because the runtime treats it as the last word: with `center`
                // set it recomputes the position itself and overwrites this one
                // (`tauri-runtime-wry-2.11.4/src/lib.rs:4595-4599`), which would quietly
                // throw away a restored geometry every launch.
                if let Some((x, y)) = placement.position {
                    builder = builder.position(x, y);
                }
                let window = builder.build()?;
                // WHERE HE LEFT IT, FOR NEXT TIME — and only if it is still reachable then.
                //
                // ONLY AN INSTALLED LAUNCH RECORDS ANYTHING, and that is the same rule the
                // activation block above argues, applied to the same question. An accessory
                // launch is a harness: its window is built INVISIBLE and never ordered on
                // screen, so its geometry is not a fact about where he put anything. A
                // harness that recorded it would move the CEO's window on his next start
                // from a boot he never saw — the 2026-09-06 complaint in a slower form. The
                // saved record is still OFFERED to such a launch; only the writing back is
                // withheld.
                if activation.presentation == activation::Presentation::Regular {
                    remember_window_geometry(&window, geometry_path.clone());
                }
                // Files dropped on the window reach the composer (CEO §86).
                mac_attachments::listen_for_drops(&window);
                // COME TO THE FRONT. Measured by ray-opus-a1 on published v1.0.0,
                // 2026-09-04: the window opened BEHIND other windows, twice, on a first
                // launch from Finder — an app a stranger has just double-clicked and cannot
                // see is an app that did not start, as far as he is concerned.
                //
                // `set_focus` is `makeKeyAndOrderFront:` plus
                // `activateIgnoringOtherApps: YES` on macOS
                // (tao-0.35.3/src/platform_impl/macos/util/async.rs:231-238), which is the
                // pair that raises the whole application and not just this window. tao
                // guards it on the window being visible and not miniaturized, and
                // `tauri.conf.json` declares `"visible": true`, so it runs.
                //
                // A FAILURE HERE IS NOT FATAL, and that is deliberate: `?` would turn "the
                // window did not come forward" into "the app refused to start", which is
                // strictly worse than the defect being fixed.
                //
                // AND IT IS ASKED FOR ONLY ON AN INSTALLED LAUNCH. `activateIgnoringOtherApps`
                // is precisely the call the CEO felt on 2026-09-06 — it takes the keyboard
                // out of the window he is typing in, from a process he did not start. The
                // reason above still holds for his own double-click, which still runs this
                // line; a boot that could not establish it is his does not.
                if activation.presentation == activation::Presentation::Regular {
                    if let Err(e) = window.set_focus() {
                        eprintln!("[richos] window did not come to the front: {e}");
                    }
                }
            }
            eprintln!(
                "[richos] launch: {} (start {}, {} window(s))",
                launch_kind.as_str(),
                start_ordinal.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string()),
                window_configs.len()
            );
            let ledger_path = data_dir.join("conversation-ledger.jsonl");
            let ledger = Ledger::open(&ledger_path).expect("open ledger");

            // WHAT DID NOT LOAD, ON THE BOOT LINE, BEFORE ANYTHING ELSE USES THE LEDGER.
            //
            // `Ledger::replay` prints a line per skipped record and a summary of its own
            // (`report_skipped`) — that is the operator's account. This is the CEO's, in
            // the words the window will render, so an operator reading a boot log and a CEO
            // reading his own screen are told the SAME thing rather than two versions of it.
            //
            // The `expect` above is the reason this matters at all: until 2026-09-05 a
            // single record this build could not parse came out of `Ledger::open` as `Err`
            // and this line panicked inside the Tauri setup hook — the window never opened.
            // The reader survives such a record now; this is what stops it doing so quietly.
            let history_health = ledger.history_health();
            if !history_health.is_clean() {
                eprintln!("[richos] {}", history_health.headline);
                eprintln!("[richos] {}", history_health.detail);
            }

            let mut spine = Spine::new(ledger);

            // Attach the live UI sink: streamed reply deltas + turn-state events flow to
            // the webview via Tauri events (see app/STREAMING.md for the contract).
            spine.set_observer(Box::new(TauriEmitter { app: app.handle().clone() }));
            // Haiku's title for a new conversation (`thread_title.rs`) arrives here, off the
            // spine, after the first message. The sidebar is told to read its rows again, and a
            // connected phone gets the list with the new name on its live stream.
            let titles = app.handle().clone();
            spine.set_title_observer(Arc::new(move |thread_id: &str, title: &str| {
                let Some(state) = titles.try_state::<AppState>() else { return };
                if show_haiku_title(&state.nav, thread_id, title) {
                    let summary = serde_json::json!({ "threadId": thread_id });
                    if let Err(e) = titles.emit(richos_core::live::EVENT_THREAD_SUMMARY_UPDATED, summary) {
                        eprintln!("[richos] the sidebar was not told about the new title ({e})");
                    }
                    tell_the_phone_the_threads(&titles);
                }
            }));

            // Durable CEO preferences (company name, assertiveness dial, the raw-retention
            // window) — same app data dir as the ledger, same durability posture, survives
            // restart.
            //
            // OPENED HERE, ahead of the machinery block below, rather than where it used to
            // sit fifty lines further down: boot eviction now reads its window out of this
            // store (§7.2), and a store opened after the eviction it governs would leave the
            // CEO's setting unread for exactly the one moment it matters.
            let config_path = data_dir.join("config.json");
            // ConfigStore::open never fails on a corrupt/missing file (it degrades to
            // defaults internally — see config.rs) — expect() here only guards the
            // genuinely-unexpected io error creating the parent dir.
            let config = ConfigStore::open(&config_path).expect("open config store");

            // =====================================================================
            // WHICH COMPANY THIS COPY OF RICH WORKS FOR — resolved HERE, and here for
            // one reason: step 2 of the order is the saved choice, and the saved
            // choice lives in the store opened on the line above.
            // =====================================================================
            //
            // It used to run fifty lines further up, before the config file was open,
            // which is exactly why there were only two steps and why a Finder launch
            // had no route to an entity at all. A resolver that runs before the store
            // holding the answer cannot read the answer.
            //
            // A thread requires an entity home, so boot still conjures none out of
            // nowhere. If an entity resolves, the default thread is created/activated
            // in it; if none does, the app launches with NO active context, every send
            // is still refused — and `entity_choice`/`choose_entity` are how the CEO
            // reaches one from inside the window, which is the part that was missing.
            // =====================================================================
            // WHICH COMPANIES THIS COPY OF RICH KNOWS — read from disk, not compiled in
            // =====================================================================
            //
            // Until 2026-09-04 this was `EntityRegistry::ceos_companies()`: a `const` table
            // of one man's six companies, bound to absolute roots under his own home
            // directory, shipped inside every binary. On his machine it worked. On anybody
            // else's it published a private list into the company picker AND locked the app,
            // because no path a second person works in was a registered root.
            //
            // It is his own file now. Absent is the ordinary first-run state and yields an
            // EMPTY registry, which resolves nothing, refuses every root, and makes the app
            // ASK — the same shape `entity_choice`'s `chosen: None` already had.
            let registry_path = richos_core::entity::entity_registry_path(&data_dir);
            let mut registry_load = EntityRegistry::load(&registry_path);
            for note in &registry_load.notes {
                eprintln!("[richos] {note}");
            }
            // THE NO-ORPHAN MIGRATION, and it runs exactly once per install.
            //
            // An install that ran under the compiled-in table has a ledger full of threads
            // bound to `femcboost`, `richos` and the rest. Those ids stop being registered
            // the moment the table leaves the binary, and an unregistered id fails every
            // scoped read and write — the threads would still be on disk and would be
            // unreachable, which for the person reading the screen is the same thing.
            //
            // So the ids that ALREADY OWN RECORDS HERE are restored. That is a fact about
            // this machine, read off this machine's own ledger, and it is the only thing
            // asserted: no root is invented, because nothing here knows where those
            // companies live and a guessed root is a wrong entity waiting to happen.
            //
            // It is gated on `Absent` rather than on emptiness: a file the owner has
            // deliberately emptied is an answer, and re-seeding it would overwrite him.
            if registry_load.source == RegistrySource::Absent {
                let existing = entity_ids_already_in_the_ledger(&spine);
                if !existing.is_empty() {
                    let migrated = EntityRegistry::from_existing_ids(&existing);
                    match migrated.save(&registry_path) {
                        Ok(()) => {
                            eprintln!(
                                "[richos] company registry: this install already had threads under \
                                 {} compan(ies) ({}). They have been written to {} so they keep \
                                 resolving. No folder was guessed for any of them — open the \
                                 company settings to add one.",
                                migrated.len(),
                                migrated.entities().iter().map(|e| e.id.to_string()).collect::<Vec<_>>().join(", "),
                                registry_path.display()
                            );
                            registry_load = EntityRegistry::load(&registry_path);
                        }
                        // A FAILED MIGRATION IS NOT A FAILED BOOT. The app comes up with an
                        // empty registry and asks, which is recoverable; refusing to start
                        // would not be.
                        Err(e) => eprintln!(
                            "[richos] company registry: could not write {} ({e}). Existing threads \
                             will not resolve until a company is registered in the window.",
                            registry_path.display()
                        ),
                    }
                }
            }
            let registry = registry_load.registry.clone();
            eprintln!(
                "[richos] company registry: {} compan(ies), {} ({})",
                registry.len(),
                registry_path.display(),
                registry_load.source.as_str()
            );
            spine.set_entity_registry(registry.clone());

            // THE CENTRAL FOLDER AND THE ONBOARDING RECORD — the two halves of `onboarding.rs`.
            //
            // This is the caller the onboarding document says does not exist. Its whole finding
            // is that `engine/CLAUDE.md.template:25` carries a correct instruction in a file
            // nothing renders, and that `provision-claude-md.sh` has a test suite and no caller;
            // a company layer wired to nothing would be the third instance of the same defect in
            // the same evening.
            //
            // The central root is REPORTED, never created. `~/myrichos` and everything under it
            // belong to the central-folder work, and an app that helpfully created its own
            // source could never report the source missing — which is exactly how this machine
            // ended up with four `corpus.*` symlinks pointing at a directory that is not there
            // (`richos-central-folder-2026-09-06.md` §1.3).
            match richos_core::company::install_central_root() {
                Some(root) => {
                    eprintln!(
                        "[richos] central folder: {} ({})",
                        root.display(),
                        if root.is_dir() { "present" } else { "NOT THERE — no company has anything on file" }
                    );
                    spine.set_central_root(root);
                }
                None => eprintln!("[richos] central folder: no home directory — no company layer this launch"),
            }
            spine.set_onboarding_record(richos_core::onboarding::record_path(&data_dir));

            let boot = boot_entity(&registry, &config);
            match &boot.entity {
                Some(entity) => {
                    eprintln!("[richos] company: {entity} (via {})", boot.source.map(|s| s.describe()).unwrap_or("resolution"));
                    let binding = spine.ensure_active_thread_in(entity).expect("ensure thread");
                    if let Err(error) = spine.migrate_legacy_onboarding_declination() {
                        eprintln!("[richos] onboarding answer migration failed: {error}");
                    }
                    // WHAT THIS LAUNCH WILL ACTUALLY DO ABOUT ONBOARDING, said out loud.
                    //
                    // Every state prints, including the good one, for the reason the lease line
                    // above it prints its success: before that line a working boot was silent
                    // and a reader had to infer it from a failure line not appearing. A CEO who
                    // will never be asked about his business is the single failure this work
                    // exists to remove, and a boot that says nothing about it cannot be checked.
                    eprintln!("[richos] {}", spine.describe_onboarding(&binding));
                }
                // THE OPERATOR'S HALF of the same condition. `ENTITY_UNRESOLVED_MESSAGE`
                // is written for the CEO and deliberately names no environment variable;
                // this line is read by whoever it names, in a terminal, where
                // `RICHOS_ENTITY` is the correct and actionable instruction.
                //
                // IT NO LONGER ENDS THERE, and that is the substantive change: the CEO is
                // now asked in the window, so this says what he will see rather than
                // implying a terminal is the only way through.
                // THE LIST IS DERIVED, NOT TYPED. It was typed, it said "femcboost,
                // deeply, prospects or richos", and on 2026-09-01 the registry grew to the
                // CEO's real six — at which point a hand-written list becomes an operator
                // instruction that omits two of the valid answers and reads as if they are
                // invalid. Reading it off the registry is one expression and cannot drift.
                // THE OPERATOR'S HALF, and since 2026-09-04 it has two shapes, because
                // the condition has two causes and they need different sentences. An install
                // with companies registered and none resolved is a CHOICE waiting to be made;
                // an install with NO companies registered has nothing to choose between, and
                // telling that operator to set `RICHOS_ENTITY` to "one of " an empty list
                // would be an instruction he cannot follow.
                None if registry.is_empty() => eprintln!(
                    "[richos] no company is registered on this install yet — RichOS will ask in \
                     the window and save the company details to {}.",
                    registry_path.display()
                ),
                None => eprintln!(
                    "[richos] choose a company in the window. Available companies: {}.",
                    registry.entities().iter().map(|e| e.id.to_string()).collect::<Vec<_>>().join(", ")
                ),
            }
            // ONBOARDING'S OTHER HALF, and it was found by running this rather than by
            // reading it: with no company resolved, the line inside the arm above never
            // prints, so a boot said nothing at all about onboarding — which is the exact
            // silence this work exists to remove, reproduced by the work removing it.
            //
            // It is not an error. There is nothing to interview about until he has said
            // which company this is, and the company sheet asks that before this could
            // matter (`richos-central-folder-2026-09-06.md` §2.2: the picker writes the row,
            // the interview writes the row's contents). The boot says so rather than
            // leaving a reader to work out why a line is missing.
            if boot.entity.is_none() {
                eprintln!(
                    "[richos] onboarding: no company resolved, so nothing to ask about yet — \
                     the question comes after he says which company this is"
                );
            }

            // The MACHINERY journal + its live sink (techy-mode design §2.1). Its own
            // directory beside the ledger and config, per §2.1: NOT loro (it would poison
            // the context compiler) and NOT the conversation ledger (which replays whole
            // at every boot, and whose `messages()` is one missing filter away from the
            // CEO's conversation).
            //
            // Retention is attached UNCONDITIONALLY and has no flag gating it (§3.2:
            // "Routing and retention run ALWAYS"). That is what makes "flip a thread I
            // already had" possible at all, and it is why this line is here rather than
            // behind a setting. The accepted cost is ~1-2 MB/day of machinery an owner may
            // never look at — named as a cost, not hidden.
            let machinery_root = data_dir.join("machinery");
            let journal = MachineryJournal::new(&machinery_root);
            // Tier-B eviction at boot (§2.4), against THE CEO'S OWN WINDOW (§7.2) rather
            // than against two constants. `raw_retention()` on an install that has never
            // set it is `RAW_RETENTION_DAYS` / `RAW_MAX_TOTAL_BYTES` — the same 14 days and
            // 2 GB this line passed before — so nothing about an existing machine changes.
            // Tier A — the normalized record — is never touched at ANY setting, so an
            // evicted day still renders its structure, titles, statuses and paths. Boot is
            // the right moment: it is off the streaming hot path entirely.
            let evicted = journal.evict_raw_within(richos_core::util::now_millis(), config.raw_retention());
            if evicted > 0 {
                eprintln!("[richos] machinery: evicted {evicted} raw shard(s) past the retention window");
            }
            spine.set_machinery_journal(journal);
            spine.set_machinery_observer(Box::new(TauriMachineryEmitter { app: app.handle().clone() }));

            // THE OUTPUT RECORD (Output side panel PRD §3-§4): `<data>/output/<thread>.jsonl`,
            // written by witnesses — Rich's own completed write-tool calls live, and at every
            // turn end the hook's `writes.jsonl` for this lease (a front-desk worker's writes,
            // the files a command made). The SAME store goes to the work host below.
            let output_store = richos_core::output::OutputStore::for_data_dir(&data_dir)
                .with_scratch(richos_core::output::ScratchRoots::standard())
                .with_observer(Arc::new(TauriOutputEmitter { app: app.handle().clone() }));
            spine.set_output_store(output_store.clone(), Some(data_dir.join("engine-state").join("evidence")));

            // The ADDITIVE §13 family (UX brief slice 3) — see `events.rs`. A THIRD sink
            // beside the two above, so the four events the shipping UI listens to are
            // untouched; `crates/richos-core/tests/live_event_tests.rs` asserts their
            // payloads are byte-identical with and without this line.
            //
            // TWO SINKS ON ONE STREAM SINCE THE PHONE CHANNEL (`src/phone/`). `set_live_observer`
            // takes exactly one observer, so the two are fanned out rather than the spine being
            // reshaped — and the fan-out hands both the SAME `&LiveEvent`, after the SAME gate, so
            // the phone can never see anything the calm view cannot. The phone's half is INERT
            // until he pairs a phone: it drops everything it is given while no listener runs,
            // which is why it can be installed here, once, rather than reaching into the spine
            // later (plan §2.5 item 1).
            let (phone_runtime, phone_emitter) = phone::PhoneRuntime::install(data_dir.clone());
            spine.set_live_observer(Box::new(phone::stream::FanOutLiveEmitter::new(vec![
                Box::new(events::TauriLiveEmitter { app: app.handle().clone() }),
                phone_emitter,
            ])));

            // THE WORKER-LIFECYCLE STREAM (UX §7), 2026-08-29.
            //
            // Without this line `Spine::timeline` supplies an EMPTY worker stream and a
            // delegated `Task` call reaches the CEO as one nameless activity row reading
            // "Worked" — which is what shipped, and why `TimelineItem::WorkerActivity`
            // existed, was fully tested, and had never once occurred on the wire.
            //
            // `CurrentTeamDir` re-resolves on every read rather than binding a path at boot:
            // the engine's hooks are snapshotted at SESSION start, so the directory this
            // machine's current session writes to changes without RichOS relaunching. The
            // home fallback is deliberately never read — it accumulates across sessions and
            // cannot be session-scoped (`worker_events.rs`).
            spine.set_worker_events(WorkerEventsSource::CurrentTeamDir);

            // TIER C — COMPANY MEMORY (continuity §2.1 #8 / §4, open-items 3.5).
            //
            // Without this line the seam that has existed since the continuity foundation
            // stays unset and every re-prime asserts a fresh Rich into existence with no
            // company memory at all, while the compiler sits complete and versioned one
            // directory away. That is what shipped, and it is why `LoroContextCompiler`
            // was fully specified, fully documented and had never once been called.
            //
            // Nothing is inferred. `LORO_CORPUS`/`LORO_ROOT` names the corpus and
            // `RICHOS_LORO_DIR` names the tools; with either unset the app boots with
            // `LoroTier::NotWired`, which the priming prompt states as a fact about THIS
            // INSTALL rather than as a claim about what is recorded.
            //
            // PROVENANCE. The compiler retains the ITEMS of every accepted slice here, and
            // the spine reads them when the CEO says a record is wrong — see `belief.rs`.
            // Without this the loro desk has no proposer, because `propose` takes a record
            // reference and nothing in the app could supply one honestly.
            let loro_provenance: SharedSliceProvenance =
                std::sync::Arc::new(Mutex::new(SliceProvenance::new()));
            // RESOLVED FOR A LAUNCH WITH NO TERMINAL, since 2026-09-01. `from_env` is
            // still the whole of the explicit path and is still exclusive; what changed is
            // that a Finder launch — launchd's environment, no LORO_* anything — now also
            // reaches the two per-user pointers an operator can put in place. Measured
            // before and after on the signed bundle:
            // `docs/verification/installed-app-2026-09-01/`.
            // MOVED, UNCHANGED, TO `memory.rs` ON 2026-09-01, and moved for one reason:
            // first-run provisioning has to run all of it a SECOND time. The CEO answers
            // "set my memory up", a corpus appears on disk, and the app has to start using
            // it without being relaunched — so the wiring cannot be a hundred lines that
            // only exist inside `setup`. Two copies would drift and the drifted one would
            // be the one he hits.
            //
            // BOTH HALVES COME BACK. Until 2026-09-01 this call produced only the READ half
            // and the correction desk below built its own writer out of the environment,
            // ninety lines later — which is how a GUI launch came to resolve the CEO's
            // corpus for reading and nothing at all for writing. `WiredMemory` carries the
            // writer that the SAME `LoroInstall` produced, so the two cannot disagree.
            let resolution = resolve_engine();
            let engine = resolution.dir.clone().unwrap_or_else(|| std::path::PathBuf::from("/nonexistent/richos-engine"));
            let wired = memory::wire_company_memory(&mut spine, &loro_provenance, &registry, &engine);
            let memory_status = wired.status;
            // AND THE RESOLUTION, kept whole. `home_field_data` compiles the home screen's
            // picture out of this and must not lock the spine to reach it.
            let loro_install = wired.install;

            // Attach the compute lease. A boot with no Claude auth, no `claude` binary, or a
            // binary that rejects our flags does NOT silently degrade: no lease is attached
            // and EVERY send is refused with `LEASE_UNAVAILABLE_MESSAGE` — a calm,
            // Rich-voiced "not connected" that the CEO cannot mistake for a working app
            // (`send_message`, and the voice submit callback, both ask `Spine::has_lease`
            // at the moment of the send, never a boolean cached here).
            //
            // **The DIAGNOSIS is printed verbatim, and that is the loud half §16 demands.**
            // `NativeError` carries the child's own stderr, so a binary that stopped
            // accepting `--permission-prompt-tool stdio` announces itself here as
            // `error: unknown option '--permission-prompt-tool'` rather than as a mystery.
            // It is not shown to the CEO — a flag name is not CEO copy — but it is the first
            // thing whoever set RichOS up will read, and the sentence he DOES see points at
            // exactly that person.
            // NAMED AT BOOT, ALWAYS. Seven candidates resolve this now (engine.rs), so
            // "which one answered" is a fact an operator needs and cannot otherwise obtain —
            // the same reason the success line below names the binary instead of leaving a
            // working boot silent. When nothing answered, every place looked is printed:
            // "not found" without the list is what sends someone hunting.
            // WHICH BUILD IS THIS — first, and on every launch. It is one line and it is the
            // one a walk, an audit or a bug report needs before any other fact about the
            // process is worth anything (`engine::source_commit`). Until 2026-09-18 a signed,
            // notarized candidate carried no commit anywhere in it.
            eprintln!("[richos] this app: {}", engine::source_commit_note());
            eprintln!("[richos] engine directory: {}", resolution.describe());
            if resolution.source.is_none() {
                for (source, path) in &resolution.tried {
                    eprintln!("[richos]   looked in {} ({})", path.display(), source.as_str());
                }
            }
            // AN ENGINE THAT IS THERE AND IS NOT THIS BUILD'S — one line each, with both
            // versions in it. Printed whether or not the walk went on to find the right
            // engine elsewhere: "there is no engine" and "there is an engine and it belongs to
            // a different release of this app" are different problems with different repairs,
            // and a rollback produces the second one on a machine where everything else is
            // healthy (spec point 22).
            for (source, path, why) in &resolution.rejected {
                eprintln!("[richos]   passed over {} ({}) — {why}", path.display(), source.as_str());
            }
            // THE ENGINE THE BOOT ACTUALLY FOUND, as opposed to the sentinel above. First-run
            // setup asks the same question through this value, so it never offers to install
            // something the boot already resolved — the dogfood checkout being the case that
            // would otherwise be asked every launch.
            let boot_engine: Option<PathBuf> =
                resolution.source.is_some().then(|| engine.clone());
            // THE SPEECH DECODER THE ENGINE DELIVERED. `whisper-cli` is in the verified runtime
            // since 2026-10-07 (build-runtimes.py); voice resolves it before PATH or Homebrew.
            // Set here, before the window can ask `voice_readiness`, and again by first-run
            // setup when it installs an engine (`setup_view::run`). The full hash this costs is
            // not paid twice: the recovery block below verifies the same runtime and
            // `runtime.rs` re-reads only files whose stamp changed.
            richos_voice::stt::set_delivered_runtime_bin(
                boot_engine
                    .as_deref()
                    .and_then(|dir| richos_core::runtime::verify_engine(dir).ok())
                    .map(|runtime| runtime.root.join("bin")),
            );
            // THE CELL THE LEASE FACTORY READS. Shared, so a successful first-run install can
            // re-point it without a relaunch (`setup_view::run`).
            let engine_cell: Arc<Mutex<PathBuf>> = Arc::new(Mutex::new(engine.clone()));
            // RESOLVED OR REFUSED, NEVER SILENTLY DEGRADED — the 2026-09-17 candidate-walk
            // fix. `resolve_claude_bin` used to fall through to the bare name `claude` here,
            // which this launch's own boot went on to hand a provider whose `PATH` gets
            // replaced by `runtime.rs::EngineRuntime::path` (`engine_profile.rs`); a bare name
            // that PATH can never resolve. The failure then surfaced two turns later,
            // misclassified by `interruption.rs` as "usually clears on its own" — false for a
            // binary that was never there. `resolve_claude_bin_checked` names every place
            // looked when nothing answers, exactly like the "first-run setup" block below.
            let claude_bin = match resolve_claude_bin_checked() {
                Ok(path) => path,
                Err(e) => {
                    eprintln!("[richos] compute connection: {e}");
                    std::path::PathBuf::from("claude")
                }
            };
            // THE CELL THE LEASE FACTORY READS, beside `engine_cell` and for the same reason
            // (see `EngineLeaseFactory`). A successful first-run install rewrites it.
            let claude_bin_cell: Arc<Mutex<PathBuf>> = Arc::new(Mutex::new(claude_bin.clone()));
            // Connect on the first accepted request. The spine publishes Working and
            // installs Stop before spawning, so startup cannot block the window or leave
            // a CEO staring at an inert application during the CLI handshake.
            // Attach the rotation/recovery seam REGARDLESS of initial boot success — even
            // if Claude wasn't signed in at launch, wiring the factory means a later sign-in
            // + retry (or a crash recovery attempt) has a real respawn path rather than none.

            let quota = Arc::new(richos_core::quota::Service::open_account_wide(&data_dir)?);
            // THE FIRST READ OF THE WHOLE HISTORY, off the launch path (§118): every account's
            // transcripts priced once, so the Token spend screen opens on its figure. 9.28 GB took
            // 15.97 s on the CEO's Mac (2026-10-11); it waits 20 s so the launch has the machine.
            // A screen opened before it ends says it is counting; later reads take only new lines.
            let spend_weak = Arc::downgrade(&quota);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(20));
                if let Some(quota) = spend_weak.upgrade() { quota.spend_update(); }
            });
            // Read immediately at startup, regardless of the pause switch. Session
            // starts and account changes wake this same reader. Otherwise its cache
            // enforces five-minute polling, with reset deadlines and error backoff.
            let quota_weak = Arc::downgrade(&quota);
            let quota_bin = claude_bin_cell.clone();
            let quota_app = app.handle().clone();
            std::thread::spawn(move || {
                let mut wait = std::time::Duration::ZERO;
                let mut counted: Option<std::time::Instant> = None;
                loop {
                    let Some(quota) = quota_weak.upgrade() else { break };
                    let force = quota.wait_for_refresh(wait);
                    if quota.is_shutdown() { break; }
                    // **How many agents are working**, for round 16's "15 agents reading at
                    // once" and "3 agents are holding their place": the provider's own launch
                    // rows with no end observed, in every open lease session (the drill chip's
                    // "N working", summed). Counted here, every 5 seconds, so a turn boundary
                    // that says a line never takes the work host's locks to count.
                    if counted.is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(5)) {
                        if let Some(state) = quota_app.try_state::<AppState>() {
                            let mut sessions: Vec<String> = state.control.lease_sessions().into_iter().chain(state.work.lease_sessions()).collect();
                            sessions.sort(); sessions.dedup();
                            let engine_state = state.data_dir.join("engine-state");
                            quota.set_agents_working(sessions.iter()
                                .map(|session| richos_core::app_workers::status(&engine_state, Some(session)).active).sum());
                            counted = Some(std::time::Instant::now());
                        }
                    }
                    let bin = quota_bin.lock().unwrap().clone();
                    quota.run_approved_weekly_reset(&bin);
                    quota.refresh(&bin, force);
                    // **Fill-first's notices, the moment they are read** (plan §15 answer 9:
                    // the high-speed alert "at once, in the conversation"). Only when no turn
                    // holds the spine: a running turn says them at its own end instead
                    // (`Spine::raise_quota_notices`), and this never waits on one.
                    if let Some(state) = quota_app.try_state::<AppState>() {
                        if let Ok(mut spine) = state.spine.try_lock() {
                            spine.raise_quota_notices(None);
                        }
                    }
                    wait = std::time::Duration::from_secs(1);
                }
            });
            // Whether THIS launch was told which engine to use. `EnvEngineDir` and
            // `EnvEngineRoot` are the only two sources that are a statement rather than a
            // search (`engine.rs` candidates 1 and 2).
            let explicit_engine = matches!(
                resolution.source,
                Some(engine::EngineSource::EnvEngineDir) | Some(engine::EngineSource::EnvEngineRoot)
            );
            // ==============================================================================
            // HIS TEAM, ON AN OPERATOR INSTALL — decided once, here (operator back-end spec r3
            // (f); the operator-client record's §7 items 1-3)
            //
            // No `operator.json` (every customer, and his app today): nothing below exists and
            // this launch is the product, byte for byte. A broken one: nothing below exists
            // either, and `spawn_work` refuses every assignment with its one sentence. A valid
            // one: his team's desk, the socket its front-desk tools reach it through, and the
            // spine keeping the mouth each sentence came through so a phone assignment can be
            // told from one given at the desk.
            // ==============================================================================
            let (operator_desk, operator_socket, operator_access) =
                match richos_core::operator_declaration::gate(&data_dir) {
                    richos_core::operator_declaration::Gate::Operator(declaration) => {
                        operator_desk_at_boot(*declaration, &data_dir, app.handle().clone(), Arc::clone(&engine_cell))
                    }
                    _ => (None, None, None),
                };
            if operator_desk.is_some() {
                spine.keep_intake_channel(true);
            }
            let lease_factory = || EngineLeaseFactory {
                quota: quota.clone(),

                claude_bin: Arc::clone(&claude_bin_cell),
                engine_dir: Arc::clone(&engine_cell),
                data_dir: data_dir.clone(),
                explicit_engine,
                operator_desk: operator_access.clone(),
            };
            spine.set_lease_factory(Box::new(lease_factory()));
            // **THE WORK HOST — the background-work spec §2.1's second lease, and a SECOND
            // factory instance rather than a shared one.** The factory is stateless except
            // for the two cells it reads, and both are `Arc`s, so the work host sees the
            // same engine directory and the same `claude` path the moment first-run setup
            // rewrites them. Giving the spine's factory away instead would have coupled the
            // two leases' lifetimes, which is the thing §2.1 exists to separate.
            //
            // It is started here and runs for the life of the process. With nothing
            // registered it is a thread parked on a condition variable; it never polls.
            let work = richos_core::work_host::WorkHost::new(
                &data_dir.join("engine-state"),
                Box::new(lease_factory()),
                Arc::new(WorkNotice { app: app.handle().clone() }),
            );
            work.set_quota(quota.clone());
            // The back end's written files — its own, its workers', its commands' — reach the
            // assignment's thread's output list after every back-end turn (PRD §4.1 (b)).
            work.set_output_store(output_store.clone());
            if let Some(desk) = &operator_desk {
                work.set_operator(desk.clone());
            }
            work.start();
            eprintln!("[richos] compute connection: starts with the first cancellable request over {}", claude_bin.display());
            // The second review's watcher, the host's own child, from launch to quit (slice 4).
            if let Some(dir) = boot_engine.as_deref() {
                ensure_app_review_watch(dir, &data_dir, &claude_bin, true);
            }
            if let Some(desk) = &operator_desk {
                ensure_operator_review_watch(desk, &data_dir);
                ensure_operator_team_watch(desk, &data_dir);
            }

            // Release grants from the old process. Stu owns job recovery through his inbox.
            work.set_screen(std::sync::Arc::new(screen::MacScreen));
            let recovery=richos_core::recovery::reconcile(&data_dir.join("engine-state"));
            eprintln!("[richos] {}",recovery.log_message());

            // ==============================================================================
            // FIRST-RUN SETUP — what this machine is missing, named at boot
            //
            // §19: "Today RichOS runs on his Mac and would not run on anyone else's." The
            // boot line below is the honest statement of that, printed on EVERY launch so
            // the condition is visible before a customer discovers it as a broken app. On the
            // CEO's own machine it says nothing is missing and costs two `stat` calls.
            // ==============================================================================
            {
                let s = setup_view::detect(boot_engine.as_deref());
                if s.complete() {
                    eprintln!("[richos] first-run setup: nothing missing.");
                } else {
                    for c in s.needs() {
                        let what = match c {
                            richos_core::setup::Component::ClaudeCode => &s.claude,
                            richos_core::setup::Component::Engine => &s.engine,
                            richos_core::setup::Component::MediaTools => &s.media_tools,
                        };
                        // ONE PLACE PER LINE, and never a trailing colon with nothing after
                        // it. On 2026-09-17 the published nightly printed exactly that —
                        // `the RichOS engine is NOT installed — looked in: ` — on four
                        // consecutive launches, because the list was empty and `join("; ")`
                        // on an empty Vec is the empty string. A detector that names no
                        // candidates reads as one that can never find anything, which is
                        // what it was; `setup.rs` now guarantees the list is non-empty, and
                        // this prints it a place at a time so a rejection reason (which
                        // carries its own em dash) is legible rather than run together with
                        // the next path. It names the component by its machine name
                        // (`setup_missing_line`), never by the name on the sheet.
                        eprintln!("{}", setup_missing_line(c, what.looked_in.len()));
                        for place in &what.looked_in {
                            eprintln!("[richos]   looked in {place}");
                        }
                    }
                    // WHETHER THIS BUILD CAN FIX IT. An unpinned build must say so here, not
                    // discover it when he presses the button.
                    match s.engine_pin_version.as_deref() {
                        Some(v) if !s.engine.present => {
                            eprintln!("[richos] first-run setup: this build installs engine {v}.")
                        }
                        None if !s.engine.present => eprintln!(
                            "[richos] first-run setup: this build carries NO engine pin, so it \
                             cannot install one. Build with RICHOS_ENGINE_VERSION / \
                             RICHOS_ENGINE_URL / RICHOS_ENGINE_SHA256 set."
                        ),
                        _ => {}
                    }
                    // THE VIDEO TOOLS START DOWNLOADING NOW, in the background, before the window
                    // asks anything (the CEO, 2026-10-07: "The speech models and video tools begin
                    // downloading in the background at first launch, while the user does the rest
                    // of setup."). Nothing in first setup waits for them (`setup_view`, the video
                    // tools block). Not under the updater selftest, which runs headless.
                    if s.needs().contains(&richos_core::setup::Component::MediaTools)
                        && updates::selftest_mode().is_none()
                    {
                        setup_view::start_video_tools_download(app.handle());
                    }
                }
            }

            // Left-navigation view state — same app data dir, same durability posture as
            // the ledger and config, and never fatal: a corrupt file degrades to defaults
            // rather than refusing the launch (nav.rs).
            let nav_store = nav::NavStore::open(data_dir.join("navigation.json"));

            // THE STOP/STEER CONTROL (UX §9.2/§9.3). Its intake log sits beside the
            // ledger, same app data dir, same durability posture — and deliberately NOT
            // inside the ledger, because a request that has not been acted on yet is not
            // evidence of anything that happened.
            //
            // A control that cannot be opened degrades to `detached()`, which REFUSES to
            // stop rather than acting on a request it cannot record. An unrecorded stop is
            // indistinguishable from a crash on the next boot, and §6.1's "You stopped
            // after {duration}" would be back to attributing a crash to the CEO.
            let control = match TurnControl::open(data_dir.join("intake.jsonl")) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[richos] steering intake unavailable, stop will refuse rather than pretend: {e}");
                    TurnControl::detached()
                }
            };
            spine.set_turn_control(control.clone());
            // A card answer keeps the input order it was accepted with until it reaches the
            // ledger; never hand that order to a new input after a restart.
            match richos_core::questions::Store::new(data_dir.join("engine-state")).highest_order() {
                Ok(Some(order)) => control.advance_input_order(order),
                Ok(None) => {}
                Err(e) => eprintln!("[richos] saved answer order unreadable: {e}"),
            }

            // WHAT HE ASKED FOR AND THIS BUILD COULD NOT READ, ON THE BOOT LINE.
            //
            // The same shape as the ledger's line above and for the same reason:
            // `IntakeLog::open` prints the operator's account (one line per skipped record
            // plus a summary), and this is the CEO's, in the sentences a surface would
            // render — so a boot log and a screen say the same thing rather than two
            // versions of it.
            //
            // IT IS A DIFFERENT STATEMENT FROM THE LEDGER'S, which is why it is a separate
            // pair of lines rather than folded into `history_health`. The ledger's says part
            // of the conversation on screen did not load. This one says something he TYPED
            // while Rich was working never reached Rich at all — the intake log holds his
            // words before they become turns, so a record lost here is a message that was
            // never answered and never shown.
            //
            // NO NEW SURFACE IS BUILT HERE. There is no window notice for the intake log
            // today; `TurnControl::intake_health()` is the seam one would read, and what
            // the smallest honest surface is remains an open question rather than something
            // invented in this change. Silent, however, it is no longer.
            match control.intake_health() {
                Some(h) if !h.is_clean() => {
                    eprintln!("[richos] {}", h.headline);
                    eprintln!("[richos] {}", h.detail);
                }
                // Clean, or detached — and those are different facts. A detached control has
                // no file to read, which is not the same as a file with nothing wrong in it;
                // the line above has already said why it is detached.
                _ => {}
            }

            // Apply any stop request that outlived the process. The window is one line
            // wide — request fsync'd, process dies, terminal event never written — and
            // without this the turn is `in_flight` forever and crash-replay would re-run
            // work the CEO explicitly stopped.
            if let Err(e) = spine.reconcile_intake() {
                eprintln!("[richos] intake reconciliation at boot: {e}");
            }
            // What it ran — a phone message that outlived the last process — ended here, so
            // the work those turns wrote down starts here (`adopt_at_the_turn_boundary`). The
            // recovery above has already settled every assignment the LAST process left, so
            // only what these turns registered can be adopted (spec §6.3: nothing restarts).
            adopt_at_the_turn_boundary(&mut spine, &work);

            // THE CORRECTION DESK (open-items 3.5). Its log sits beside the ledger and the
            // intake log, same durability posture, and deliberately NOT in the ledger: a
            // proposal the CEO has not answered yet is not evidence of anything that
            // happened — the same rule `TurnControl`'s intake log follows.
            //
            // THE WRITER IS NOT RESOLVED HERE. It arrives from `wire_company_memory` above,
            // out of the one `LoroInstall` the read half was also built from. What used to
            // be on this line was `CliLoroWriter::from_env()`, and it is the third instance
            // in one day of the same premise failure — a component reading its configuration
            // from environment variables that a Finder launch does not have. `ps eww` on a
            // real double-click carries `HOME`, `USER` and
            // `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, and nothing else, so on the CEO's
            // installed app this was `Ok(None)` on every boot and the desk was dead: he
            // could be shown a proposal, confirm it, and reach a writer with no corpus.
            //
            // OPENED AND WIRED BY `install_correction_desk`, which `provision_memory` also
            // calls. ONE function, because the boot path and the first-run path have to
            // produce the same desk wired to the same seams — and two copies of "open it,
            // hand it to the spine, attach the observer" would drift, with the copy that
            // drifted being the one a new customer hits in his first five minutes.
            let correction = match wired.writer {
                Some(writer) => install_correction_desk(&mut spine, app.handle(), &data_dir, writer),
                // NOT A SILENT NO-OP. `wire_company_memory` has already printed the reason
                // and, for the no-corpus case, every candidate it looked at — the same
                // sentences the read path prints, because it is the same resolution. This
                // line says what it MEANS for the desk, so nobody has to join the two facts
                // up themselves.
                None => {
                    eprintln!(
                        "[richos] loro correction desk: CLOSED — {}. Confirming a correction \
                         will refuse and say so rather than appearing to write.",
                        match memory_status.state.as_str() {
                            "none" => "no corpus resolved (candidates listed above)",
                            "no-compiler" => "a corpus resolved but loro-write.mjs is not installed",
                            other => other,
                        }
                    );
                    None
                }
            };

            // THE FLYWHEEL'S AUTOMATIC TRIGGER (`spoken.rs` + `staging.rs`). Its log sits
            // beside the ledger, the intake log and the loro desk, same durability posture
            // and deliberately NOT in the ledger: a question the CEO has not answered is
            // not evidence of anything that happened.
            //
            // The desk is attached to the SPINE (so every utterance is examined as it is
            // submitted, with no command typed) and kept in `AppState` as the same `Arc`
            // (so answering it never waits on the turn lock).
            let spoken = match CandidateDesk::open(data_dir.join("spoken-corrections.jsonl")) {
                Ok(mut desk) => {
                    // The vocabulary writer. Absent on an ordinary install with no local
                    // service configured — detection and staging still work, and only the
                    // CONFIRM step reports that it has nowhere to write. It never claims a
                    // term was learned when nothing wrote it.
                    match CliVocabulary::from_env() {
                        Some(v) => desk.set_vocabulary(Box::new(v)),
                        None => eprintln!(
                            "[richos] no RICHOS_SERVICE_BIN — spoken corrections will be recorded \
                             and asked, and confirming one will report that there is no vocabulary \
                             to write to"
                        ),
                    }
                    let shared = desk.shared();
                    Some(shared)
                }
                Err(e) => {
                    // Refuse rather than pretend, exactly as the loro desk does above: a
                    // desk that cannot record durably would lose the CEO's correction
                    // across a relaunch, which is worse than saying so.
                    eprintln!(
                        "[richos] spoken-correction desk unavailable, corrections will not be \
                         recorded: {e}"
                    );
                    None
                }
            };
            // THE SPOKEN TRIGGER, wired at the same seam the belief one is
            // (`install_correction_desk`): the desk the Tauri commands answer through is
            // the desk the turn path files into, one `Arc`, so a question raised inside a
            // two-hour turn is answerable during it.
            if let Some(desk) = &spoken {
                spine.set_candidate_desk(desk.clone());
                spine.set_correction_observer(Box::new(TauriCorrectionEmitter {
                    app: app.handle().clone(),
                }));

                // THE THIRD TRIGGER — heard vs sent (`heard.rs`), the one that watches
                // rather than listens, into the SAME desk. Opt-in, and this is the only
                // place that decides it.
                //
                // WHY IT IS NOT ON BY DEFAULT, when the other two are. It measured
                // precision 0.972 rather than 1.000 (156 invented pairs,
                // `docs/measurements/heard-vs-sent-trigger-2026-08-30/`), and its one false
                // positive is a pair that would rewrite an ordinary English word in every
                // future dictation. It also fires on an edit the CEO never volunteered —
                // he fixed his own text and moved on — so a wrong question here costs more
                // than after a sentence he just spoke. Until the defect is repaired in
                // `capture.js`'s shared expansion rule where it actually lives, the CEO
                // turns this on deliberately or not at all. **No default is a judgement
                // this file gets to make on his behalf.**
                match std::env::var("RICHOS_HEARD_TRIGGER").as_deref() {
                    Ok("on") | Ok("1") | Ok("true") => match DictationJournal::from_env() {
                        Some(j) if j.present() => {
                            eprintln!(
                                "[richos] heard-vs-sent trigger ON — reading the dictation \
                                 journal at {}",
                                j.describe()
                            );
                            spine.set_heard_source(Box::new(j));
                        }
                        Some(j) => eprintln!(
                            "[richos] heard-vs-sent trigger was switched on, but there is no \
                             dictation journal at {} — the trigger has no \"heard\" side and \
                             stays silent. Install the flywheel patch \
                             (tools/richos-hud/dictation-flywheel.patch) or point \
                             RICHOS_DICTATION_JOURNAL at the journal.",
                            j.describe()
                        ),
                        None => eprintln!(
                            "[richos] heard-vs-sent trigger was switched on, but no journal \
                             location could be resolved (no $HOME and no \
                             RICHOS_DICTATION_JOURNAL) — the trigger stays silent"
                        ),
                    },
                    _ => {}
                }
            }

            // THE FEEDBACK CHANNEL'S ONE FILE (`feedback.rs`), beside the ledger, the intake
            // log and the two correction desks — same durability posture, and deliberately
            // NOT in the ledger: what the CEO thinks of a session is not evidence of
            // anything that happened in it.
            //
            // ONE FILE, NOT A DIRECTORY, and that is the module's own testable property: a
            // directory invites a second file beside the first — a spool, an "unsent"
            // shard, a marker — and `feedback_no_outbound_tests.rs` asserts that recording
            // produces exactly one file and nothing else.
            let feedback = match FeedbackStore::open(data_dir.join("feedback.jsonl")) {
                Ok(store) => Some(Mutex::new(store)),
                Err(e) => {
                    eprintln!(
                        "[richos] feedback store unavailable, the feedback surface will refuse \
                         rather than take an answer it cannot keep: {e}"
                    );
                    None
                }
            };

            spine.detach_conversation_io();
            app.manage(AppState {
                quota: quota.clone(),

                work,
                operator: operator_desk,
                operator_socket,
                quit_confirmed: std::sync::atomic::AtomicBool::new(false),
                // The SAME decision the window was built from, carried rather than
                // re-derived: two readings of "may this launch take the screen" is two
                // answers waiting to disagree.
                can_come_back: activation.presentation == activation::Presentation::Regular,
                provider_auth: Mutex::new(Default::default()),
                account_auth: Mutex::new(None),
                front_desk_primed_for: Mutex::new(Default::default()),
                reader: spine.reader(),
                spine: Mutex::new(spine),
                config: Mutex::new(config),
                machinery_root,
                entity: Mutex::new(boot.entity),
                registry_path,
                registry_source: registry_load.source,
                // PRESENCE, not success. A `RICHOS_ENTITY` that names nothing resolves to
                // `None` and short-circuits BEFORE the saved choice — so a CEO who answered
                // the picker under one would have his answer written to disk and never read
                // again, and be asked at every launch for ever. Keying this on the variable
                // being SET means the surface tells him it was decided outside the window and
                // names who owns it, instead of silently swallowing his answer.
                entity_pinned_by_env: std::env::var_os("RICHOS_ENTITY").is_some(),
                entity_source: Mutex::new(boot.source),
                nav: Mutex::new(nav_store),
                control,
                correction: Mutex::new(correction),
                spoken,
                feedback,
                launch: Mutex::new(launch_store),
                memory: Mutex::new(memory_status),
                loro_install: Mutex::new(loro_install),
                loro_provenance,
                // KEPT, not re-derived. `provision_memory` opens the correction desk's log
                // beside the ledger, and it must be the SAME directory this boot used —
                // re-asking `app_data_dir()` in the command would be a second resolution of
                // a question already answered, which is the shape of defect this whole
                // evening has been about.
                data_dir,
                // THE SAME CELL THE LEASE FACTORY HOLDS, not a copy of the path. A copy
                // would let `run_setup` update one and leave the other pointing at the
                // directory the boot failed to find.
                engine_dir: engine_cell,
                claude_bin: claude_bin_cell.clone(),
                boot_engine,
            });

            // Resume only after AppState exists: PhoneBridge::new immediately reads
            // the spine to prime the paired phone's conversation. The window was
            // created above for first paint, but setup has not returned to the event
            // loop yet. Its commands cannot use the app until this setup completes.
            // A persisted pairing resumes without opening a new pairing window.
            phone_runtime.resume_if_paired(app.handle().clone());
            app.manage(phone_runtime);
            question_host::start(app.handle().clone());
            peer_messages::start(app.handle().clone())?;
            // Readiness starts before the window's setup/navigation reads, with the
            // selected thread's immutable company scope. Reader snapshots stay available.
            ready_selected_front_desk(app.handle());
            // The composer's attachments, in the SAME app-data directory the phone's
            // attachment desk writes to (`phone/attachments.rs`). No I/O until a file arrives.
            let attachments_home = app.state::<AppState>().data_dir.clone();
            app.manage(mac_attachments::MacAttachments::open(&attachments_home));
            // The Output panel's file commands and preview scheme (PRD §5), over the SAME store
            // the spine and the work host write, and the published spine for the active thread.
            // QuickLook renditions go in the app's cache directory (§4.8).
            let output_cache = app.path().app_cache_dir().unwrap_or_else(|_| attachments_home.join("cache"));
            let output_reader = app.state::<AppState>().reader.clone();
            app.manage(output_files::OutputFiles::for_app(output_store, output_reader, &attachments_home, &output_cache));
            // Bust a bug (round 21): the reports on this Mac, and the loop that sends a waiting
            // one by itself once it can go, from this launch on.
            app.manage(bug_report::BugReports::for_app(&attachments_home, &app.package_info().version.to_string()));
            bug_report::spawn_retry(app.handle().clone());
            // DICTATION (plan slice 1): when dictation.json says on, the tool starts as this
            // app's child, on a thread of its own so first paint never waits for it.
            #[cfg(target_os = "macos")]
            {
                ensure_voice_state(app.handle());
                let link = Arc::new(dictation_app::Link::default());
                app.manage(link.clone());
                // The Dictation sheet and the Settings row (slice 2) hear the tool through
                // `rich://dictation`: its state moved, or key capture answered.
                let window = app.handle().clone();
                link.set_listener(Box::new(move |message| {
                    if let Err(e) = window.emit(dictation_app::EVENT, dictation_app::event_payload(message)) {
                        eprintln!("[richos] dictation: the window could not be told: {e}");
                    }
                }));
                let host = dictation_app::Host::new(
                    link,
                    attachments_home.clone(),
                    app.package_info().version.to_string(),
                    Arc::new(AppVoice { app: app.handle().clone() }),
                );
                app.manage(host.clone());
                dictation_app::boot(host);
            }

            // ================================================================
            // THE UPDATE PATH — last in setup, and last for a reason
            // ================================================================
            //
            // `updates::init` is pure (one `app.manage`, no I/O, no network), so its
            // position costs first paint nothing — but the LAUNCH CHECK it arms must come
            // after everything above, because a TLS handshake started before the ledger
            // has replayed competes with the one thing the CEO is actually waiting for.
            // The check itself then waits a further three seconds on its own task.
            //
            // The selftest branch is `app/scripts/updater-e2e.sh`'s entry point. It runs
            // the app EXACTLY as it boots — same ledger, same window, same everything —
            // and then drives the same `check`/`install` functions the two commands drive.
            // A harness that skipped the boot would be proving a different program.
            updates::init(app.handle());
            match updates::selftest_mode() {
                Some(mode) => {
                    eprintln!("[richos] update selftest: {mode}");
                    updates::spawn_selftest(app.handle().clone(), mode);
                }
                None => updates::spawn_launch_check(app.handle().clone()),
            }
            // THE WORK GATE'S WATCHER (CEO, 2026-09-05). It is what makes the update control
            // DISAPPEAR while RichOS is doing something and COME BACK when it stops — the
            // only source that can see workers is a file another process writes, so it has
            // to be read rather than waited for. Costs a single mutex read per tick in every
            // state except the two where the CEO has something to act on; see
            // `updates::spawn_work_watcher`.
            //
            // NOT armed under the selftest, deliberately: `updater-e2e.sh` runs headless with
            // no spine and no lease, and a watcher emitting into a webview that does not
            // exist would add a moving part to the one harness that proves the signature
            // refusal.
            if updates::selftest_mode().is_none() {
                updates::spawn_work_watcher(app.handle().clone());
                // THE NEWEST yt-dlp (CEO, 2026-10-07: "the most up-to-date version of that tool
                // is always available on the user's machine"; media-tools plan §3). Same three-
                // second delay as the update check, then every six hours, or every five
                // minutes while no copy is installed. Not under the selftest, for the same
                // reason as the watcher above.
                spawn_media_tools_refresh();
            }

            // ================================================================
            // THE END OF RESOLUTION, SAID OUT LOUD
            // ================================================================
            //
            // Everything above this line is this launch answering "where is my
            // configuration?" — the engine directory, the compute lease, the corpus, both
            // halves of loro, the company, the four durable stores. Everything below it and
            // after it is conduct rather than resolution: `spawn_launch_check` sleeps three
            // seconds and then talks to the network, and the window is already up.
            //
            // IT IS HERE SO A CHECK CAN KNOW WHEN TO STOP READING, and that is not a
            // convenience. `gui-boot.test.sh` boots this binary under launchd's environment
            // and holds every line it printed to account; without a terminator, "the boot
            // log" would be "whatever had appeared after N seconds", and a check whose input
            // depends on a sleep is a check that goes red for no reason on a slow morning
            // and gets ignored by the third time. The marker makes the boundary a FACT the
            // program states rather than a duration the harness guesses.
            //
            // It is also the honest answer to an operator reading a terminal: the lines
            // above are all of it, so nothing further is coming and a missing line is
            // missing rather than late.
            eprintln!("[richos] boot complete — every line above is what this launch resolved");
            // AND THE FAILURE ALERT STANDS DOWN ON THE SAME LINE, for the reason the marker
            // above exists at all: this is the program stating where "starting up" ends.
            // Past it there is a window, the app owns its own surfaces, and a modal system
            // alert raised by a background thread that panicked would be an interruption
            // rather than a rescue. The log keeps taking everything either way.
            startup_alert::disarm();
            // The first window is up. Optional private starter I/O stays off startup.
            if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
                std::thread::spawn(move || {
                    if let Err(error) = richos_core::machine_settings::create_starter(&home) {
                        eprintln!("[richos] machine settings starter could not be created: {error}");
                    }
                });
            }
            Ok(())
        })
        .plugin(tauri_plugin_updater::Builder::new().build())
        // The Output panel's save sheet (output side-panel PRD §5.4, S6). Run from Rust inside
        // `output_files::output_save_copy` only; its commands are not granted to the page.
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            question_host::answer_question, question_host::question_shown, question_host::open_question_counts,
            claude_quota,
            claude_quota_activity,
            spend_view,
            spend_live,
            set_claude_quota_policy,
            claude_account_add,
            claude_account_sign_in,
            claude_account_sign_in_poll,
            claude_account_remove,
            claude_account_use_first,
            claude_account_discard,
            claude_account_email,
            claude_account_nudge_answer,
            set_claude_at_threshold,
            approve_claude_reset,
            revoke_claude_reset,
            phone_status,
            phone_begin_pairing,
            phone_connect_enable,
            phone_connect_disable,
            phone_stop_pairing,
            phone_confirm_on_mac,
            phone_reject_on_mac,
            phone_forget,
            list_threads,
            active_thread,
            create_thread,
            switch_thread,
            get_messages,
            send_message,
            get_company_name,
            set_company_name,
            get_assertiveness,
            set_assertiveness,
            get_lease_models,
            set_lease_models,
            get_worker_status,
            get_work_status,
            get_assignments,
            take_work_notices,
            take_operator_notices,
            stop_assignment,
            raise_proactive_message,
            // --- loro (2026-08-29) — appended, never reordered ---
            loro_available,
            loro_show_record,
            loro_pending_corrections,
            loro_propose_correction,
            loro_confirm_correction,
            loro_decline_correction,
            loro_suppressed_records,
            loro_unsuppress_record,
            // --- voice mode (2026-08-24) — appended, never reordered ---
            voice_readiness,
            start_voice_capture,
            stop_voice_capture,
            voice_speak_delta,
            voice_speak_end,
            voice_turn_started,
            voice_turn_ended,
            voice_barge_in,
            voice_diagnostics,
            // --- row 3.30 (2026-09-05) — appended, never reordered ---
            voice_turn_cut_off,
            // --- first-run speech model (2026-09-17) — appended, never reordered ---
            provision_speech_model,
            cancel_speech_model_download,
            // --- spoken corrections (2026-08-30) — appended, never reordered ---
            spoken_corrections_available,
            spoken_pending_corrections,
            spoken_confirm_correction,
            spoken_decline_correction,
            spoken_suppressed_terms,
            spoken_unsuppress_term,
            // --- entity + thread navigation (Codex-UX slice 4, 2026-08-29) ---
            // Appended at the END so a parallel timeline branch appending its own glue
            // merges without touching a line this branch also touched.
            navigation_tree,
            active_context,
            thread_scope,
            create_thread_in,
            search_nav,
            nav_state,
            set_sidebar_width,
            set_sidebar_collapsed,
            set_entity_collapsed,
            set_thread_pinned,
            set_thread_archived,
            rename_thread,
            // --- which company this copy of Rich works for (slice 4, 2026-09-01) ---
            entity_choice,
            choose_entity,
            register_entity,


            repository_connections,
            connect_repository,
            pick_folder,
            memory_status,
            provision_memory,
            // --- Codex-UX slice 5 (2026-08-29): the timeline reload path ---
            get_timeline,
            // --- Codex-UX slice 7 (2026-08-29): the read-only worker inspector ---
            set_inspector_width,
            // --- the Output panel's split width (output side-panel PRD §9.5, S9), beside its
            //     sibling divider's rather than at the end, where parallel slices append ---
            set_output_width,
            // --- Codex-UX slice 6 (2026-08-29): steering and stop (§9.2/§9.3) ---
            stop_turn,
            steer_message,
            running_turn,
            // --- the feedback channel, local half (2026-08-30) — appended, never reordered ---
            feedback_available,
            feedback_wording,
            feedback_taxonomy,
            feedback_preview,
            feedback_record,
            feedback_history,
            // --- the how-to screens' links (CEO §61.1, 2026-09-19) — appended, never reordered ---
            open_external,
            // --- techy mode Phase 2 (2026-08-30) — appended, never reordered ---
            get_machinery,
            get_machinery_raw,
            techy_mode,
            set_techy_mode,
            set_techy_default,
            // §7.1's three-way scope choice (CEO, 2026-09-18) — appended here for the
            // same reason the three above it are: this list is append-only.
            set_techy_scope,
            // --- the raw-retention window as a setting (2026-08-30) — appended, never reordered ---
            raw_retention,
            set_raw_retention,
            // --- the opening screen and its off switch (2026-08-30) — appended, never reordered ---
            splash_enabled,
            set_splash_enabled,
            splash_note_shown,
            // --- appearance and the person at the foot of the rail (§15) — appended, never reordered ---
            get_appearance,
            set_theme,
            set_font_scale,
            get_user_identity,
            set_user_name,
            // --- the launch record (2026-08-31) — appended, never reordered ---
            launch_state,
            launch_note_splash_shown,
            // --- the home screen's entity row (2026-09-01) — appended, never reordered ---
            home_entity_row,
            set_home_entity_label,
            set_home_entity_visible,
            // --- the update path (2026-08-31) — appended, never reordered.
            //     These four are the ONLY updater surface the webview has: no
            //     `plugin:updater|*` command is granted to it (capabilities/default.json).
            updates::update_state,
            updates::update_check,
            updates::update_install,
            updates::update_rollback,
            updates::update_relaunch,
            // --- first-run setup, Option D (2026-09-01) — appended, never reordered ---
            setup_status,
            run_setup,
            provider_auth_status,
            provider_auth_start,
            provider_auth_poll,
            provider_auth_cancel,
            // --- what did not load out of the ledger (2026-09-05) — appended, never reordered ---
            history_health,
            // --- the home screen's picture, out of his own corpus (2026-09-06) — appended,
            //     never reordered ---
            home_field_data,
            // --- the first-run notice: the VISIBLE half of the onboarding offer
            //     (2026-09-06) — appended, never reordered ---
            onboarding_view,
            decline_onboarding,
            // --- row 5: the window closes and the work keeps going (2026-09-17) —
            //     appended, never reordered ---
            confirm_quit_and_stop,
            cancel_quit,
            // --- screenshots and files into the composer, CEO §86 (2026-09-24) —
            //     appended, never reordered ---
            mac_attachments::attach_dropped_file,
            mac_attachments::attach_pasted_file,
            mac_attachments::discard_attachment,
            mac_attachments::commit_attachments,
            // Add to chat from the Output panel (output side-panel PRD §12.7): the same desk.
            mac_attachments::output_attach,
            output_files::list_output,
            output_files::output_file,
            output_files::output_preview,
            output_files::output_open,
            output_files::output_reveal,
            output_files::output_save_copy,
            // --- Rich's one offer of dictation (dictation plan slice 4) — appended ---
            dictation_offer,
            dictation_offer_shown,
            dictation_feature_thread,
            // --- the Dictation sheet and the Settings row (dictation plan slice 2) — appended ---
            #[cfg(target_os = "macos")]
            dictation_app::dictation_status,
            #[cfg(target_os = "macos")]
            dictation_app::dictation_set_on,
            #[cfg(target_os = "macos")]
            dictation_app::dictation_set_key,
            #[cfg(target_os = "macos")]
            dictation_app::dictation_set_accuracy,
            #[cfg(target_os = "macos")]
            dictation_app::dictation_ask_microphone,
            #[cfg(target_os = "macos")]
            dictation_app::dictation_ask_accessibility,
            #[cfg(target_os = "macos")]
            dictation_app::dictation_permissions_changed,
            #[cfg(target_os = "macos")]
            dictation_app::dictation_open_settings,
            dictation_take_sheet_request,
            #[cfg(target_os = "macos")]
            dictation_app::dictation_capture_key,
            // --- Bust a bug (round 21, CEO §115) — appended ---
            bug_report::bug_report_context,
            bug_report::bug_report_private_in,
            bug_report::bug_report_look,
            bug_report::bug_report_write,
            bug_report::bug_report_take_unchecked,
            bug_report::bug_report_change,
            bug_report::bug_report_send,
            bug_report::bug_report_try_now,
            bug_report::bug_report_cancel,
            bug_report::bug_report_voice,
            bug_report::bug_report_open_issue,
            // --- "Let Codex review your team's work" (round 20.2, ruling §114) — appended ---
            codex_reviews_status,
            codex_reviews_set,
            // --- the user's own team on the home screen (product hunt 2026-10-15, finding 7)
            // — appended ---
            home_team,
            // --- a job's result said on its own (2026-10-15) — appended ---
            // --- the Oxford comma in what Claude writes (CEO 2026-10-19) — appended ---
            set_oxford_comma,
        ])
        .build(context)
        .expect("error while building RichOS")
        // THE CLEAN-EXIT MARKER, CLEARED HERE AND NOWHERE ELSE.
        //
        // This is the whole of what makes the NEXT launch read as fresh rather than as a
        // crash-restart, and `.run(context)` — which this replaces — gave no place to put
        // it. `RunEvent::Exit` is the last thing Tauri emits before the process ends, so it
        // fires on a quit and, by construction, cannot fire on a kill or a power cut. That
        // is exactly the semantics the record wants: absent marker means he quit, present
        // marker means the app did not get to say so.
        //
        // A write failure here is logged and never fatal. The cost of one is that the next
        // launch is read as a crash-restart — an undercount by one, no splash once — which
        // is not worth refusing to close the app over.
        .run(|handle, event| {
            // =========================================================================
            // ROW 5 — "You close the window. The work keeps going. Quit ends it."
            // =========================================================================
            //
            // **The answer is read the instant this returns** (§2.5a): the runtime does
            // `let recv = rx.try_recv();` immediately after the callback
            // (`tauri-runtime-wry-2.11.4/src/lib.rs:4318` for a window closing, `:4361`
            // for `app.exit`), and an empty channel is NOT `Prevent`. So this arm decides
            // and returns; nothing here waits for him, and the question — when there is
            // one — is asked afterwards, from `ask_before_quitting`.
            if let tauri::RunEvent::ExitRequested { code, api, .. } = &event {
                if let Some(state) = handle.try_state::<AppState>() {
                    let request = lifecycle::ExitRequest {
                        questions_pending: questions_pending(&state),
                        programmatic: code.is_some(),
                        confirmed: state.quit_confirmed.load(std::sync::atomic::Ordering::SeqCst),
                        registered: registered_work(&state),
                        can_come_back: state.can_come_back,
                    };
                    match lifecycle::decide(&request) {
                        lifecycle::ExitDecision::Allow => {
                            // AUDIT-10 ROW 5: THE OTHER HALF OF THE RED BUTTON NOW SAYS SO
                            // TOO. Ray closed the window with nothing registered, the app
                            // quit, and `app.log` carried no line at all — while the
                            // work-registered path below has written one since .9. One
                            // control, two outcomes, and only one of them readable
                            // afterwards, so a walk could not tell a quit-as-designed from a
                            // death. The DECISION is unchanged and is not in question: §2.4
                            // is "idle must still quit". Only the silence was.
                            if let Some(why) = lifecycle::allow_reason(&request) {
                                eprintln!("[richos] {why}");
                            }
                        }
                        lifecycle::ExitDecision::StayResident => {
                            api.prevent_exit();
                            if request.questions_pending {
                                eprintln!("[richos] window closed with pending questions: the phone service stays available. Quit ends it; the questions are saved.");
                            } else {
                                eprintln!(
                                    "[richos] window closed with work registered: RichOS stays \
                                     running with no window. The Dock icon brings it back."
                                );
                            }
                        }
                        lifecycle::ExitDecision::AskBeforeQuitting => {
                            // PREVENT FIRST, UNCONDITIONALLY, THEN ASK. The other order is
                            // a quit (§2.5a).
                            api.prevent_exit();
                            ask_before_quitting(handle.clone());
                        }
                    }
                }
            }
            // The Dock icon, the way back in (§2.4). `Reopen` is emitted from
            // `applicationShouldHandleReopen` through tao and wry
            // (`tauri-runtime-wry-2.11.4/src/lib.rs:4391-4394`).
            if let tauri::RunEvent::Reopen { has_visible_windows, .. } = &event {
                if !has_visible_windows {
                    // **OFF THE EVENT-LOOP CALLBACK, deliberately.** Building a window is a
                    // runtime message, and issuing one from inside the callback the runtime
                    // is currently dispatching is the shape that deadlocks; a spawned thread
                    // is the path Tauri documents for window creation from anywhere. The
                    // Dock click has already happened, so nothing is waiting on this return.
                    let handle = handle.clone();
                    std::thread::spawn(move || reopen_window(&handle));
                }
            }
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = handle.try_state::<AppState>() {
                    // ONE bound for the whole quit, shared by the work lease's runners below and
                    // every lease's supervisor after them (reap gap 1.3(c)).
                    let quit_bound = std::time::Instant::now() + std::time::Duration::from_secs(2);
                    state.quota.shutdown();
                    let _ = state.control.request_stop_all();
                    state.control.shutdown_leases();
                    // **The WORK lease is stopped BY NAME, because nothing else reaches it**
                    // (background-work spec §2.5). The front-desk shutdown above covers
                    // every conversation; the separate Stu work lease is
                    // deliberately not in it (§4.2). Destructors are not guaranteed to run
                    // on this path (tao's event loop ends in `process::exit`), and the
                    // supervisor's 100 ms parent-death poll is a backstop, not the promise.
                    // Every assignment still open becomes `interrupted`; nothing becomes
                    // `settled` on the way out.
                    // **HIS TEAM ENDS WITH THE APP, by its own quit path** (r3 (q) item 2): each
                    // lead's supervisor gets SIGTERM and reaps the lead's whole tree, and only
                    // after every lead has gone is the claim given up (e item 3), so his
                    // terminal can take his team back the moment this process is gone.
                    if let Some(desk) = &state.operator {
                        let quits = desk.quit();
                        eprintln!("[richos] his team: {} lead(s) ended by the quit path; the claim is released.", quits.len());
                    }
                    if let Some(socket) = &state.operator_socket {
                        socket.close();
                    }
                    state.work.shutdown();
                    // **EVERY LEASE'S TOOL COMMANDS END WITH THE APP, by its supervisor's reap**
                    // (reap gap C1, 1.3(c)): each supervisor gets SIGTERM, ends its tool shells
                    // and background commands, and exits; one still running at the bound has its
                    // group killed as before. Waited for here, because destructors and detached
                    // threads are not guaranteed on this path.
                    let settled = richos_core::owned_process::SupervisedSet::process().settle(quit_bound);
                    eprintln!("[richos] quit: {} lease(s) ended their tool commands; {} stopped at the bound.",
                              settled.leases, settled.escalated);
                    // **THE SECOND REVIEW'S WATCHERS END WITH THE APP** (slice 4): each stops the
                    // reviews it started, then exits; its recorded pid is reaped and the record
                    // removed, within `review_watch::STOP_BOUND`.
                    let watchers = richos_core::review_watch::stop_all();
                    if !watchers.is_empty() {
                        eprintln!("[richos] quit: the second review's watcher(s) {watchers:?} ended.");
                    }
                    // **HIS TEAM'S QUOTA, STALL AND CODEX WATCHERS END WITH THE APP** the same way,
                    // within `team_watch::STOP_BOUND`; a notice they print after his team quit is
                    // kept for its next lead.
                    let watchers = richos_core::team_watch::stop_all();
                    if !watchers.is_empty() {
                        eprintln!("[richos] quit: his team's watcher(s) {watchers:?} ended.");
                    }
                    if let Err(e) = state.launch.lock().unwrap().note_clean_exit() {
                        eprintln!("[richos] launch record: could not mark a clean exit: {e}");
                    }
                }
            }
        });
}

// ---------------------------------------------------------------------------------------
// Seam commands (the session-continuity design / the UX direction doc).
// Appended at the END of the file (and the END of generate_handler! above) so a parallel
// voice-crate branch appending its own Tauri commands here merges cleanly.
// ---------------------------------------------------------------------------------------

/// UX §2.1: "Rail header = the company/CEO identity, not RichOS." Configurable,
/// persisted, sensible fallback when unset — the fallback lives in richos-core's
/// config.rs so this thin shell layer never has to know the placeholder string.
#[richos_command::command]
fn get_company_name(state: State<AppState>) -> String {
    state.config.lock().unwrap().company_name_or_default()
}

#[richos_command::command]
fn set_company_name(state: State<AppState>, name: String) -> Result<(), String> {
    state.config.lock().unwrap().set_company_name(&name).map_err(|e| e.to_string())
}

/// UX §5.2: the CEO's one plain 3-way proactive-attention dial. Default = Quiet,
/// survives restart (config.rs).
#[richos_command::command]
fn get_assertiveness(state: State<AppState>) -> String {
    state.config.lock().unwrap().assertiveness().as_str().to_string()
}

#[richos_command::command]
fn set_assertiveness(state: State<AppState>, level: String) -> Result<(), String> {
    let parsed = Assertiveness::parse(&level).ok_or_else(|| format!("unknown assertiveness level: {level}"))?;
    state.config.lock().unwrap().set_assertiveness(parsed).map_err(|e| e.to_string())
}

/// **Rich's model, as Preferences shows it** (§119; round 23): `{ "rich": "opus"|"fable",
/// "stu": "opus"|"fable" }`. Rich is the conversation, Stu the team's back end, named in the
/// app only by what each does (§110). A value another build wrote reads as Opus.
#[richos_command::command]
fn get_lease_models(state: State<AppState>) -> serde_json::Value {
    let models = state.config.lock().unwrap().lease_models();
    serde_json::json!({ "rich": models.rich.as_str(), "stu": models.stu.as_str() })
}

/// **Both rows at once, as Preferences closes** (the CEO's addendum to §119: *"Closing the
/// preferences panel IS the confirmation that the settings chosen there now need to be saved
/// and implemented."*). Save both choices together, then apply native set_model controls
/// to the existing Rich and Stu sessions. Each control returns the provider's confirmation.
/// An unknown name refuses both choices before either is written.
#[richos_command::command]
fn set_lease_models(state: State<AppState>, rich: String, stu: String) -> Result<(), String> {
    use richos_core::config::{LeaseModels, ModelChoice};
    let rich = ModelChoice::parse(&rich).ok_or_else(|| format!("unknown model: {rich}"))?;
    let stu = ModelChoice::parse(&stu).ok_or_else(|| format!("unknown model: {stu}"))?;
    state.config.lock().unwrap().set_lease_models(LeaseModels { rich, stu }).map_err(|e| e.to_string())?;
    let rich_result=state.control.set_model(rich);
    let stu_result=state.work.set_model(stu);
    rich_result.and(stu_result)
}

/// UX §3.2 / architecture P3.2: the optional AI-worker drill-down.
///
/// **Attributed, not guessed.** The team-session directory is derived from the session id
/// of the lease this app is actually serving; when there is no lease, or that session has
/// no team directory, the answer is an honest zero carrying the reason
/// (`WorkerStatusView::unattributed`). It is never the most-recently-touched directory
/// under `~/.claude/teams`, which is what shipped and which counted whoever's session had
/// been busiest — see richos-core's worker_status.rs, "WHOSE workers these are".
///
/// The session id is read from `TurnControl`, NOT from the spine, and that is deliberate:
/// `send_message` holds the spine mutex for the entire turn, and `app/ui/main.js` polls
/// this command on TURN START. Taking the spine lock here would make the worker chip block
/// until Rich finished — the same reason the stop control lives on this handle (§9.3).
#[richos_command::command]
fn get_worker_status(state: State<AppState>, thread_id: Option<String>) -> WorkerStatusView {
    let thread=thread_id.or_else(||state.reader.snapshot().active_thread().map(str::to_string));
    thread.map(|thread|state.control.for_thread(&thread).worker_status()).unwrap_or_else(||
        WorkerStatusView::unattributed(richos_core::worker_status::Unattributed::NoSession))
}

/// Saved work belongs to the requested ledger thread. Do not wait on a running
/// reasoning turn or fall back to another company's most recent receipts.
#[richos_command::command]
fn get_work_status(state: State<AppState>, thread_id: String) -> Result<richos_core::work_status::WorkSummary, String> {
    let entity = if let Some(active) = state.control.for_thread(&thread_id).active_turn() {
        active.entity_id.ok_or("This conversation has no company binding.")?
    } else {
        let spine = state.spine.try_lock().map_err(|_| "Work status is changing. Please try again.")?;
        spine.ledger().thread_binding(&thread_id).map_err(|e| e.to_string())?.entity_id().clone()
    };
    richos_core::work_status::read(&state.data_dir.join("engine-state"), entity.as_str(), &thread_id)
}

// =======================================================================================
// BACKGROUND WORK — the return path, and the per-assignment stop
//
// Background-work spec §3 and §4.2. All three read the work host and the assignment
// register, and NONE of them takes the spine's lock: they are the surface for work that
// runs between turns, and `get_worker_status` is empty in exactly that window
// (`:2300-2306`), which §7 names as the reason four acceptance steps cannot be walked
// today.
// =======================================================================================

/// The assignments on this conversation, running or finished, oldest first.
///
/// **It answers between turns, which is the whole point.** The one surface that shows
/// running workers today returns an empty view whenever no turn is open on that thread,
/// and background work lives entirely in that window.
#[richos_command::command]
fn get_assignments(state: State<AppState>, thread_id: String) -> Result<serde_json::Value, String> {
    let entity = assignment_scope(&state, &thread_id)?;
    let rows = richos_core::assignment::read_all(
        &state.data_dir.join("engine-state"),
        entity.as_str(),
        &thread_id,
    )
    .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "assignments": rows.iter().map(|row| {
            serde_json::json!({
                "id": row.id,
                "title": row.title,
                // **WORK HE ASKED FOR, OR A QUESTION HE ASKED** (CEO ruling §58, 2026-09-18).
                // The surface needs both: the timer beside his reply reads "checking" or
                // "investigating" rather than "working", and the saved-work pane must not
                // call an answered question "Finished."
                "kind": row.kind.as_str(),
                // **WHICH TURN OF HIS THIS ANSWERED**, so the timer can sit beside the reply
                // he actually got rather than at the bottom of the thread. The whole
                // reference travels rather than the turn alone: the surface compares
                // `"ledger:" + threadId + ":" + turn.id` against it, which is an equality
                // test on a string this app composed, and no parsing of a separator that
                // might one day appear in an id.
                //
                // It is not an identifier he is ever shown — it never reaches a sentence,
                // the same way `id` above never does.
                "turnRef": row.instruction_ledger_ref,
                "state": row.reported_state().unwrap_or(richos_core::assignment::AssignmentState::Interrupted).as_str(),
                "detail": row.reported_state().map(|_|row.detail.as_str()),
                // **THE DETAIL TELLS HIM WHAT TO DO** (sign in, a helper he may stop, an answer
                // he owes), so Under the hood shows it without Technical view (hunt 57, finding 11).
                // So does Rich's waiting sentence while a teammate works (review of 3b91f1223).
                "forYou": richos_core::work_host::for_him(row),
                // **CLOSED BY AN ANSWER ON HIS TIMELINE, NOT BY WORK THAT FINISHED** — a question
                // of his, or a task the back end did itself and reported in its own words
                // (esc-20260927T093052Z-85f3303f). The pane says "it's in your conversation"
                // for both and never "Finished.", which the host never witnessed.
                "answered": row.was_answered(),
                "repositories": row.repositories,
                "registeredAtMs": row.registered_at_ms,
                "canStop": row.reported_state().is_some() && row.accepts_input(),
            })
        }).collect::<Vec<_>>()
    }))
}

/// Compatibility endpoint. Work observations reach Rich through durable intake;
/// application-written notice prose is never rendered as his conversation.
#[richos_command::command]
fn take_work_notices(
    state: State<AppState>,
    thread_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    assignment_scope(&state, &thread_id)?;
    Ok(Vec::new())
}

/// All asynchronous incoming messages use the same turn, stream and phone delivery.
fn dispatch_incoming_messages(app: &AppHandle, _thread_id: &str) -> Result<(), String> {
    let app=app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state=app.state::<AppState>();
        if let Err(error)=drain_incoming_messages(&state.spine,&state.work) { eprintln!("[richos] incoming message: {error}"); }
    });
    Ok(())
}

/// **What his team said on this conversation that the window has not shown yet** — the seam
/// the operator notice surface reads when a conversation opens (the operator-client record's
/// §7 item 1: *"`take_pending` feeds the notice surface on launch"*; the surface itself is
/// item 4, Art's). Reads AND marks, like `take_work_notices`. Empty on a product install.
#[richos_command::command]
fn take_operator_notices(
    state: State<AppState>,
    thread_id: String,
) -> Result<Vec<richos_core::operator_runtime::OperatorNotice>, String> {
    let Some(desk) = &state.operator else { return Ok(Vec::new()) };
    let entity = assignment_scope(&state, &thread_id)?;
    desk.take_pending(&richos_core::operator_host::ConversationKey { entity_id: entity.as_str().to_string(), thread_id })
}

/// **The per-assignment stop** (spec §4.2). Not `stop_turn`, which is the conversation's
/// and stays the conversation's — *"OK, go with recommended"*, the CEO, 2026-09-17.
#[richos_command::command]
fn stop_assignment(state: State<AppState>, thread_id: String, assignment_id: String) -> Result<(), String> {
    let entity = assignment_scope(&state, &thread_id)?;
    state.work.stop_assignment(entity.as_str(), &thread_id, &assignment_id)
}

/// Read the requested conversation's company. A different running conversation
/// neither supplies its binding nor prevents this lookup.
const ASSIGNMENTS_CHANGING: &str = "Your assignments are changing. They will refresh in a moment.";

fn assignment_scope(state: &State<AppState>, thread_id: &str) -> Result<EntityId, String> {
    if let Some(active) = state.control.for_thread(thread_id).active_turn() {
        return active.entity_id.ok_or_else(|| "This conversation has no company binding.".into());
    }
    let spine = state.spine.try_lock().map_err(|_| ASSIGNMENTS_CHANGING)?;
    Ok(spine.ledger().thread_binding(thread_id).map_err(|e| e.to_string())?.entity_id().clone())
}


#[richos_command::command]
fn raise_proactive_message(
    state: State<AppState>,
    thread_id: Option<String>,
    tier: String,
    text: String,
) -> Result<String, String> {
    let parsed_tier = AttentionTier::parse(&tier).ok_or_else(|| format!("unknown attention tier: {tier}"))?;
    let mut spine = take_the_spine(&state.spine);
    spine
        .raise_proactive(thread_id.as_deref(), parsed_tier, &text)
        .map_err(|e| e.to_string())
}

// =====================================================================================
// VOICE MODE — appended block (2026-08-24)
//
// Voice is a MODE of the one persistent conversation, never a room: a recognized
// utterance goes through `Spine::submit_prompt` exactly like typed text (only the
// `Source` differs — `Jam` instead of `Text`), so it lands in the SAME thread, the SAME
// durable ledger, and streams back through the SAME `rich://` events. There is no second
// conversation path anywhere in this file.
//
// Rich's reply is spoken by feeding the UI's own `rich://chunk` deltas back down through
// `voice_speak_delta`. That is deliberate: the webview already renders exactly the text
// the CEO is allowed to see, so TTS inherits the clean-output guarantee instead of
// re-deriving it, and the spine's single observer slot needs no change.
//
// Everything below is APPEND-ONLY — no existing item is reordered or edited — so it
// merges cleanly alongside parallel work in `spine.rs` / `reprime.rs`.
// =====================================================================================

use richos_voice::capture::AudioSource;
use richos_voice::controller::{VoiceController, VoiceOptions};
use richos_voice::event::{VoiceEvent, VoiceObserver};
// (`Arc` is imported at the top of this file.)

/// Live voice mode, or `None` when the `◉` toggle is off. Managed lazily (see
/// `ensure_voice_state`) so this whole feature stays one appended block.
#[derive(Default)]
struct VoiceHandle {
    controller: Mutex<Option<VoiceController>>,
    /// This voice session's gate on its turns: closed when dictation takes the microphone, so
    /// the utterance in progress is discarded, never sent (dictation plan section 2).
    #[cfg(target_os = "macos")]
    gate: Mutex<Option<dictation_app::TurnGate>>,
}

/// Voice mode, as dictation's yield sees it: ended the moment a dictation starts listening.
#[cfg(target_os = "macos")]
struct AppVoice {
    app: AppHandle,
}

#[cfg(target_os = "macos")]
impl dictation_app::VoiceMode for AppVoice {
    fn end_for_dictation(&self) -> bool {
        let Some(handle) = self.app.try_state::<VoiceHandle>() else { return false };
        // The gate FIRST: dropping the controller drains its recognizer, and whatever it still
        // held must reach a closed gate.
        if let Some(gate) = handle.gate.lock().unwrap_or_else(|p| p.into_inner()).take() {
            gate.close();
        }
        let controller = handle.controller.lock().unwrap_or_else(|p| p.into_inner()).take();
        let was_on = controller.is_some();
        drop(controller); // closes the microphone and joins the voice threads
        if was_on {
            // The window leaves voice mode the way it does when the pipeline stops on its own.
            if let Err(e) = self.app.emit(
                richos_voice::event::EVENT_VOICE_STATE,
                serde_json::json!({ "state": "off", "at": richos_voice::controller::now_millis() }),
            ) {
                eprintln!("[richos] voice: the window could not be told voice mode ended: {e}");
            }
        }
        was_on
    }

    /// **Open RichOS**, **Dictation settings…** and **Fix it** from the dictation tool (plan
    /// section 6): the window comes forward the way a Dock click brings it (`reopen_window`,
    /// which builds it if it was closed and takes the front), and with `sheet` the page is told
    /// to open the Dictation sheet (`dictation-open-sheet`; the sheet is slice 2's).
    fn come_forward(&self, sheet: bool) {
        let app = self.app.clone();
        if sheet {
            // Kept until a page takes it: a window rebuilt here (RichOS kept running by its
            // work with the window closed) has no page subscribed yet when the event below
            // goes out, so the page asks for it as it boots (third review, finding 4).
            DICTATION_SHEET_REQUEST.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        let ran = self.app.run_on_main_thread(move || {
            reopen_window(&app);
            if sheet {
                if let Err(e) = app.emit(DICTATION_OPEN_SHEET, ()) {
                    eprintln!("[richos] dictation: the window could not be told to open the Dictation sheet: {e}");
                }
            }
        });
        if let Err(e) = ran {
            eprintln!("[richos] dictation: the window could not be brought forward: {e}");
        }
    }

    /// The tool's menu changed `dictation.json`: the page re-reads it (`dictation-settings-changed`).
    fn settings_changed(&self) {
        if let Err(e) = self.app.emit(DICTATION_SETTINGS_CHANGED, ()) {
            eprintln!("[richos] dictation: the window could not be told dictation.json changed: {e}");
        }
    }
}

/// The page opens the Dictation sheet (the tool's **Dictation settings…** or **Fix it**).
#[cfg(target_os = "macos")]
const DICTATION_OPEN_SHEET: &str = "dictation-open-sheet";

/// A sheet request not yet taken by a page (third review, finding 4): set by `come_forward`,
/// taken once by `dictation_take_sheet_request`, which the page calls as it boots and when it
/// hears the event, so a request made before the page existed still opens the sheet.
static DICTATION_SHEET_REQUEST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `true` once per request: the page takes the pending Dictation sheet request. Async dispatch
/// like every command here (`ipc_responsiveness_tests`): one atomic swap never needs the native
/// event loop, and a synchronous command is the one shape that could hold it.
#[richos_command::command]
fn dictation_take_sheet_request() -> bool {
    DICTATION_SHEET_REQUEST.swap(false, std::sync::atomic::Ordering::SeqCst)
}

#[cfg(test)]
mod dictation_sheet_request_tests {
    use super::*;

    /// INVARIANT (third review, finding 4): a sheet request is kept until one page takes it,
    /// and taken once; with none pending, a page booting opens nothing.
    #[test]
    fn a_sheet_request_is_taken_once() {
        assert!(!dictation_take_sheet_request(), "nothing pending");
        DICTATION_SHEET_REQUEST.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(dictation_take_sheet_request(), "the page that boots takes it");
        assert!(!dictation_take_sheet_request(), "and it is gone");
    }
}
/// The tool's menu bar menu changed `dictation.json`; what the page shows of it is re-read.
#[cfg(target_os = "macos")]
const DICTATION_SETTINGS_CHANGED: &str = "dictation-settings-changed";

/// Forwards voice events to the webview. Same shape as `TauriEmitter` above — the voice
/// crate is UI-agnostic and names its own events; this just relays them verbatim.
struct TauriVoiceEmitter {
    app: AppHandle,
}

impl VoiceObserver for TauriVoiceEmitter {
    fn on_voice_event(&self, event: &VoiceEvent) {
        let _ = self.app.emit(event.event_name(), event.payload());
    }
}

fn ensure_voice_state(app: &AppHandle) {
    // `manage` is a no-op if the type is already managed, so this is idempotent.
    app.manage(VoiceHandle::default());
}

/// The one in-flight model download, if there is one. Registered the same idempotent way, and
/// SEPARATE from `VoiceHandle` on purpose: a model download has nothing to do with whether the
/// microphone is open, happens when it is deliberately NOT open, and must survive the voice
/// controller being torn down under it.
fn ensure_model_fetch_state(app: &AppHandle) -> std::sync::Arc<voice_provision::ModelFetchState> {
    app.manage(std::sync::Arc::new(voice_provision::ModelFetchState::default()));
    app.state::<std::sync::Arc<voice_provision::ModelFetchState>>().inner().clone()
}

/// **CAN THIS MACHINE TURN SPEECH INTO WORDS?** Asked WITHOUT touching the microphone.
///
/// `Recognizer::resolve` is `stt.rs`'s own resolution — `RICHOS_WHISPER_BIN`, then the engine
/// runtime's `bin/whisper-cli`, then `PATH`, then the Homebrew prefixes for the binary;
/// `RICHOS_VOICE_WHISPER_MODEL`/
/// `RICHOS_WHISPER_MODEL`, then `RICHOS_MODEL_DIR`, then FOUR per-user directories for the
/// model — `~/.config/richos/models` joined the walk on 2026-09-17 and is where
/// `provision.rs` installs what it downloads. It reads paths and runs `command -v`; it opens
/// no device, records nothing and asks macOS for no permission. The resolved recognizer is
/// dropped: this is the question, not the answer's use.
///
/// **ONE RESOLUTION, TWO CALLERS**, deliberately. [`voice_readiness`] answers it so the
/// window can decide whether to OFFER voice at all, and [`start_voice_capture`] asks it
/// again before it opens anything, so the two can never disagree about the same machine —
/// the discipline `wire_company_memory` already establishes for the corpus.
///
/// **The shipping bundle carries no whisper binary and no model** (`tauri.conf.json`
/// declares no `resources` and no `externalBin`), so on a customer's fresh Mac, before
/// first-run setup has installed the engine, this is `Err`, and it is `Err` before the
/// microphone is ever asked for.
///
/// **WHAT CHANGED ON 2026-09-17.** It is still `Err` on a fresh Mac, and it must be — a hot mic
/// on a machine that cannot transcribe is the defect `voice_readiness` exists to prevent. What
/// is new is that one of the two reasons for that `Err` is now something RichOS can fix by
/// itself: `provision_speech_model` fetches the pinned weights this machine resolved to.
///
/// **WHAT CHANGED ON 2026-10-07.** The other reason is gone on any Mac that has an engine: the
/// engine's runtime carries `whisper-cli`, built from pinned whisper.cpp 1.9.1 source
/// (`scripts/build-runtimes.py`) and installed by first-run setup, so the decoder arrives with the
/// engine and the app download does not grow. `stt::readiness` still separates the two gaps, and
/// only the model gap is offered for download here.
fn speech_preflight() -> Result<(), String> {
    let recognizer = richos_voice::stt::Recognizer::resolve().map_err(|e| e.ceo_message())?;
    recognizer.warm(&std::env::temp_dir()).map_err(|e| e.ceo_message())
}

/// **The speech model Rich's video skill transcribes with** (media-tools plan section 3): the
/// pinned transcription model setup installs (`stt::TRANSCRIPTION_MODEL_ID`, large-v3-turbo-q5_0)
/// when `stt::model_verified` passes, otherwise the file voice mode's own resolution picks
/// (`richos_core::engine_profile::speech_model_for_video`). Read at every lease spawn, so a model
/// setup installs later reaches the next lease; the verification is a stat once setup has hashed
/// the file (`toolchain::check` caches by file identity). `None` when neither is ready; the skill
/// then says it watched the pictures only.
fn speech_model_for_rich() -> Option<PathBuf> {
    use richos_voice::stt;
    richos_core::engine_profile::speech_model_for_video(
        stt::model_verified(stt::TRANSCRIPTION_MODEL_ID),
        || match stt::readiness() {
            stt::SpeechReadiness::Ready(recognizer) => Some(recognizer.model_path().to_path_buf()),
            _ => None,
        },
    )
}

/// **WHETHER TO OFFER VOICE AT ALL.** Read by the window at launch, once.
///
/// THE DEFECT THIS EXISTS TO REMOVE, measured by ray-opus-a1 on published v1.0.0,
/// 2026-09-04: the talk button asked for the microphone, the panel said *"listening…"*, the
/// level meter rendered and macOS lit its orange recording indicator — for 25+ seconds. It
/// never transcribed and it never said it could not. To someone who has never seen this app
/// that does not read as "not ready yet"; it reads as *"I am listening to you and ignoring
/// you"*, and the app's own first-run greeting invited it: *"You can type, or tap ◉ to talk
/// to me."*
///
/// So the window asks this before it renders that greeting. `available: false` removes the
/// talk button and drops the voice half of the greeting, and `start_voice_capture` refuses
/// with the same sentence for anything that reaches it by another route. An unfinished
/// feature that is not offered costs a stranger nothing; one that is offered and pretends
/// costs him the demo.
///
/// `reason` is [`richos_voice::stt::SttError::ceo_message`] — *"My ears aren't installed on
/// this machine yet. Whoever set RichOS up adds those. I can still read what you type."*
/// It names the party, and it is already in the affordance suite's state registry.
///
/// `(async)` so the resolution's one `command -v` subprocess never runs on the IPC thread.
#[richos_command::command]
fn voice_readiness() -> serde_json::Value {
    let readiness = richos_voice::stt::readiness();
    let available = matches!(readiness, richos_voice::stt::SpeechReadiness::Ready(_));
    let reason = readiness.ceo_message();
    // SAID OUT LOUD EITHER WAY, and the ready branch is the one that was missing.
    //
    // Until 2026-09-17 this printed only on a refusal, so "voice works on this machine" was
    // reported by SILENCE — in the one log `gui-boot.test.sh` exists to hold every line of to
    // account, and on the one question a boot log most needs to answer after a model is
    // installed. An absence is not a signal: it reads identically to a command that was never
    // invoked, a frontend that never loaded, and a window that died before `init()`. The same
    // rule the crash watchdog keeps about a worker's death applies to a capability's life.
    //
    // The ready line names the MODEL and why it was chosen, because "voice is on" and "voice is
    // on with the weakest recognizer this machine could have taken" are different facts.
    match (&readiness, &reason) {
        // Log the observed state. The UI offer is not a prediction about a future
        // Talk tap: setup may already be downloading these weights.
        (_, Some(_)) => eprintln!("[richos] voice: not ready on this machine ({}) — {readiness:?}", readiness.tag()),
        (richos_voice::stt::SpeechReadiness::Ready(rec), None) => {
            eprintln!(
                "[richos] voice: ready on this machine ({}) — {}",
                readiness.tag(),
                rec.provenance_full()
            );
            // Voice works here, so Rich will speak: make his voice ready now, at launch and in the
            // background (installed if it is not on this Mac yet, then loaded), never at Talk.
            voice_provision::fetch_rich_voice_in_background();
            let recognizer = rec.as_ref().clone();
            std::thread::spawn(move || {
                if let Err(error) = recognizer.warm(&std::env::temp_dir()) {
                    eprintln!("[richos] voice: speech decoder initialization failed: {error}");
                }
            });
        }
        (_, None) => {}
    }
    // THE OFFER IS PART OF THE READINESS ANSWER, not a second command the window has to know to
    // ask. One round trip, one moment, one machine — the same reason `voice_readiness` and
    // `start_voice_capture` share a resolution rather than each running their own.
    let offer = if readiness.provisionable() { voice_provision::offer() } else { None };
    serde_json::json!({
        // UNCHANGED, and deliberately still means "can speech happen RIGHT NOW". The mock
        // harness, the state registry and `app/ui/tests/setup.js` all read this key, and a model
        // that has not been downloaded yet is not a machine that can hear.
        "available": available,
        "reason": reason,
        // `ready` | `toolchain-missing` | `model-missing` | `refused` — a stable tag, never a
        // sentence, because a UI that branches on prose breaks the day the prose improves.
        "state": readiness.tag(),
        // Present ONLY when RichOS can actually close the gap itself. Its absence is the
        // instruction: no offer, no button.
        "offer": offer,
    })
}

/// **DOWNLOAD THE SPEECH MODEL THIS MACHINE RESOLVED TO.** Asked for by the CEO, never by a timer.
///
/// `(async)` because it is a half-gigabyte transfer: on the IPC thread it would freeze the window
/// for the whole download. Progress, verification and the outcome arrive on `rich://voice-model`.
#[richos_command::command]
async fn provision_speech_model(app: AppHandle) -> Result<serde_json::Value, String> {
    let state = ensure_model_fetch_state(&app);
    let emitter = voice_provision::TauriModelEmitter { app };
    voice_provision::fetch_model(&emitter, state).await
}

/// Stop the download that is running. A stop the CEO asked for is not a failure and does not
/// wear one: what arrived is kept, so asking again resumes rather than starting over.
#[richos_command::command]
fn cancel_speech_model_download(app: AppHandle) -> serde_json::Value {
    let state = ensure_model_fetch_state(&app);
    let was_running = state.in_flight.load(std::sync::atomic::Ordering::SeqCst);
    state.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
    serde_json::json!({ "stopping": was_running })
}

/// Enter voice mode: open the mic and start listening. Returns the resolved audio
/// configuration for developer eyes; the CEO-facing UI only renders `rich://voice-state`.
#[richos_command::command]
fn start_voice_capture(app: AppHandle, thread_id: Option<String>) -> Result<serde_json::Value, String> {
    let _ = thread_id; // voice rides the ACTIVE thread — there is no per-thread voice.
    // REFUSE BEFORE THE MICROPHONE, NOT AFTER IT. `VoiceController::start` resolves the
    // recognizer first today and would raise the same sentence, but it does so eight lines
    // and one `EchoCanceller` into a function whose next act is `capture::start` — and the
    // ordering of two lines inside another crate is not a property this command should be
    // relying on to keep a hot mic off a machine that cannot transcribe. Asked here, the
    // guarantee is local and structural: no permission dialog, no orange indicator, no
    // "listening…" panel in a build with no speech model.
    speech_preflight()?;
    // Rich's voice has been loaded since launch; this only installs it if it is somehow missing
    // (his sentences wait for it). Pressing Talk loads nothing.
    voice_provision::fetch_rich_voice_in_background();
    ensure_voice_state(&app);
    // A dictation listening right now is written first, as if its key had been tapped, so voice
    // mode and dictation never hold the microphone at once (dictation plan section 2). A
    // dictation that does not stop keeps the microphone: voice mode does not open, rather than
    // opening beside it and sending his dictated words to Rich (review finding 2).
    #[cfg(target_os = "macos")]
    if let Some(link) = app.try_state::<Arc<dictation_app::Link>>() {
        if !link.finish() {
            return Err("RichScribe is still listening; tap its key to finish, then try again".into());
        }
    }
    let handle = app.state::<VoiceHandle>();
    let mut slot = handle.controller.lock().map_err(|_| "voice state poisoned")?;
    if slot.is_some() {
        return Ok(serde_json::json!({ "already": true }));
    }

    let observer: Arc<dyn VoiceObserver> = Arc::new(TauriVoiceEmitter { app: app.clone() });

    // The submit callback: a recognized utterance takes the SAME path typed text takes.
    let submit_app = app.clone();
    // `Fn(String, bool)`, not `Fn(String)`: the second argument is what the capture path
    // measured about Rich's own audible window while this audio was recorded. It exists because
    // `source: Source::Jam` was the WHOLE of a spoken turn's provenance, so an echo-born turn
    // and a genuine one were indistinguishable in the ledger after the fact — candidate .5 left
    // Rich's own counting in the CEO's thread as the CEO's message and nothing recorded which it
    // was. See `richos_voice::controller::AdmittedUtterance::rich_audible`.
    let submit: Arc<dyn Fn(String, bool) + Send + Sync> =
        Arc::new(move |text: String, rich_audible: bool| {
            // BUST A BUG (round 21): while a report waits on the user's words, what they say is
            // the report's. The window takes it from `rich://voice-transcript`; it is not a turn.
            if bug_report::holding_voice(&submit_app) {
                eprintln!("[richos] a spoken answer went to the bug report, not to the conversation");
                return;
            }
            let state = submit_app.state::<AppState>();
            let Some(spine) = take_the_spine_or_give_up(&state.spine) else { return };
            // HIS WHOLE THOUGHT IS ONE REQUEST, even after a wait here: earlier work held the
            // spine, he went on talking meanwhile, and his next turn already carries these words.
            if richos_voice::controller::spoken_turn_withdrawn() {
                drop(spine);
                eprintln!("[richos] voice: a spoken turn was withdrawn while it waited for earlier work; his next turn carries its words");
                return;
            }
            // ===========================================================================
            // A FACTORY IS NOT AN ENGINE — the spoken half of the same arm
            // ===========================================================================
            //
            // **The typed path got this on 2026-09-19 (`05735cac`) and the spoken path did
            // not.** Identical conjunction, identical consequence: `set_lease_factory` is
            // called unconditionally at boot, so `has_lease_factory()` is always true, `&&`
            // short-circuits, and the arm below can only ever be reached on a machine that
            // has no factory at all. A spoken sentence on a Mac whose engine is stale
            // therefore started a turn that could not finish and came back as
            // `turn interrupted [transient]` — *"I lost my connection to the part of me that
            // thinks… asking again is worth a try."* Asking again cannot work. Nothing about
            // that machine changes by being asked twice, and the one press that fixes it was
            // never offered.
            //
            // **VOICE AND TEXT ARE ONE CONVERSATION, so they are one answer.** The whole
            // design of this closure is that a recognized utterance takes the SAME path typed
            // text takes; a refusal that differs between the two is the place that promise
            // breaks. This is `send_message`'s arm, word for word, emitting on voice's own
            // channel because that is the only difference between the two paths.
            //
            // **AND IT COSTS NOTHING ON A HEALTHY MAC**, for the same reason: `boot_engine` is
            // `Some` exactly when this launch resolved a usable, correctly-pinned engine, so
            // the 322 MB hash `setup_view::detect` pays for is only ever paid on a launch that
            // already knows something is wrong.
            if !spine.has_lease() && state.boot_engine.is_none() {
                let disk = setup_view::detect(None);
                if let Some(sentence) = setup_view::incomplete_message(&disk) {
                    drop(spine);
                    eprintln!("[richos] spoken turn refused before it started (first-run setup is incomplete)");
                    let _ = submit_app.emit(
                        richos_voice::event::EVENT_VOICE_ERROR,
                        serde_json::json!({
                            "message": sentence,
                            "at": richos_voice::controller::now_millis(),
                        }),
                    );
                    return;
                }
            }
            // THE LIVE LEASE, for the reason `send_message` reads it live — a spoken sentence
            // must not be refused by a boot-time snapshot that a completed first-run setup has
            // already made false. Same question, same moment, one answer.
            if !spine.has_lease() && !spine.has_lease_factory() {
                drop(spine);
                let _ = submit_app.emit(
                    richos_voice::event::EVENT_VOICE_ERROR,
                    serde_json::json!({
                        "message": LEASE_UNAVAILABLE_MESSAGE.to_string(),
                        "at": richos_voice::controller::now_millis(),
                    }),
                );
                return;
            }
            // The turn, and then its boundary: work he gave aloud starts the way typed work
            // does (`adopt_at_the_turn_boundary`, CEO ruling §88).
            drop(spine);
            run_the_spoken_turn(&state.spine, &state.work, &text, rich_audible);
        });
    // Behind this session's gate, which dictation closes when it takes the microphone.
    #[cfg(target_os = "macos")]
    let submit = {
        let gate = dictation_app::TurnGate::new();
        *handle.gate.lock().map_err(|_| "voice state poisoned")? = Some(gate.clone());
        dictation_app::gated_submit(gate, submit)
    };

    let scratch_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("voice-scratch");

    let opts = VoiceOptions { source: AudioSource::from_env(), scratch_dir };
    match VoiceController::start(opts, observer, submit) {
        Ok(ctl) => {
            // HIS WHOLE THOUGHT IS ONE REQUEST (review rv-20261013T184256Z, finding 4). When he
            // goes on after a pause longer than the end of turn, before Rich has said a word,
            // voice carries the earlier words into his next turn and asks for the partial turn to
            // be stopped: the same stop as his Esc (`stop_turn`), so it is recorded the same way.
            let stop_app = app.clone();
            ctl.on_withdraw(Arc::new(move || {
                let state = stop_app.state::<AppState>();
                let view=state.reader.snapshot();
                let outcome=view.active_thread().map(|thread|state.control.for_thread(thread).request_stop())
                    .unwrap_or(Ok(StopOutcome::NothingRunning));
                match outcome {
                    Ok(StopOutcome::Requested { turn_id, .. }) => eprintln!(
                        "[richos] voice: he went on before Rich answered; stopped {turn_id} so his whole thought is one request"
                    ),
                    Ok(StopOutcome::NothingRunning) => {
                        eprintln!("[richos] voice: he went on before Rich answered; nothing was running to stop")
                    }
                    Err(e) => eprintln!("[richos] voice: the partial turn could not be stopped: {e}"),
                }
            }));
            let d = ctl.diagnostics().clone();
            *slot = Some(ctl);
            Ok(serde_json::json!({
                "inputSource": d.input_source,
                "inputRate": d.input_rate,
                "outputDevice": d.output_device,
                "outputRate": d.output_rate,
                "sttModel": d.stt_model,
                "echoCancellation": d.echo_cancellation,
                "bargeInFrames": d.barge_in_frames,
                "bargeInSecs": d.barge_in_secs,
            }))
        }
        Err(e) => {
            eprintln!("[richos] voice mode failed to start: {e}");
            Err(e.ceo_message())
        }
    }
}

/// Leave voice mode. Dropping the controller closes the microphone — "off" means the mic
/// is genuinely closed, not merely ignored.
#[richos_command::command]
fn stop_voice_capture(app: AppHandle, thread_id: Option<String>) -> Result<(), String> {
    let _ = thread_id;
    ensure_voice_state(&app);
    let handle = app.state::<VoiceHandle>();
    let mut slot = handle.controller.lock().map_err(|_| "voice state poisoned")?;
    slot.take(); // Drop closes the device and joins the threads.
    Ok(())
}

/// One `rich://chunk` delta, relayed by the UI while voice mode is on. Completed sentences
/// are synthesized and queued immediately — this is the gapless pipelining.
#[richos_command::command]
fn voice_speak_delta(app: AppHandle, text: String) {
    if let Some(handle) = app.try_state::<VoiceHandle>() {
        if let Ok(slot) = handle.controller.lock() {
            if let Some(ctl) = slot.as_ref() {
                ctl.speak_delta(&text);
            }
        }
    }
}

/// The turn's terminal event: speak the unterminated tail so Rich never swallows his last
/// words.
#[richos_command::command]
fn voice_speak_end(app: AppHandle) {
    if let Some(handle) = app.try_state::<VoiceHandle>() {
        if let Ok(slot) = handle.controller.lock() {
            if let Some(ctl) = slot.as_ref() {
                ctl.speak_end();
            }
        }
    }
}



/// `rich://turn-started`, relayed from the UI.
#[richos_command::command]
fn voice_turn_started(app: AppHandle) {
    if let Some(handle) = app.try_state::<VoiceHandle>() {
        if let Ok(slot) = handle.controller.lock() {
            if let Some(ctl) = slot.as_ref() {
                ctl.turn_started();
            }
        }
    }
}

/// `rich://turn-completed` / `rich://turn-error`, relayed from the UI.
#[richos_command::command]
fn voice_turn_ended(app: AppHandle) {
    if let Some(handle) = app.try_state::<VoiceHandle>() {
        if let Ok(slot) = handle.controller.lock() {
            if let Some(ctl) = slot.as_ref() {
                ctl.turn_ended();
            }
        }
    }
}

/// **`rich://turn-error` WHILE VOICE MODE IS ON — say it out loud** (`open-items.md` row
/// 3.30, answer 1).
///
/// **This replaces `voice_speak_end` on the error path, and that is the whole fix.**
/// `speak_end` FLUSHES the sentence chunker's tail. On a turn that died mid-sentence the
/// tail is half a sentence that will never be completed, so the shipping behavior was: Rich
/// speaks half a sentence aloud, trails off, and says nothing further. In voice mode the
/// CEO's eyes are not on the screen — trailing off is exactly what a person does while
/// thinking, so he waits for the rest of an answer that is not coming.
///
/// `reason` is the `reason` field of the `rich://turn-error` payload, which for an upstream
/// failure is the sentence `richos-core`'s `upstream.rs` authored (`UpstreamFault::
/// ceo_message` plus the loss statement). It is relayed VERBATIM and never parsed here:
/// richos-voice knows nothing about ledgers or HTTP statuses, and one classifier owning that
/// decision is why it stays right.
///
/// Nothing queued is silenced. Sentences Rich already completed are real answer and they
/// finish; the notice follows them. See `richos_voice::controller::CutOffDesk`.
#[richos_command::command]
fn voice_turn_cut_off(app: AppHandle, reason: Option<String>) {
    if let Some(handle) = app.try_state::<VoiceHandle>() {
        if let Ok(slot) = handle.controller.lock() {
            if let Some(ctl) = slot.as_ref() {
                let observer = TauriVoiceEmitter { app: app.clone() };
                ctl.turn_cut_off(reason.as_deref(), &observer);
            }
        }
    }
}

/// The UI's "tap to stop" — the CEO's instant interrupt while AEC is missing. Returns the
/// seconds of queued speech dropped and the measured stop latency, so an interruption is
/// reported in real units rather than as an adjective.
#[richos_command::command]
fn voice_barge_in(app: AppHandle) -> serde_json::Value {
    if let Some(handle) = app.try_state::<VoiceHandle>() {
        if let Ok(slot) = handle.controller.lock() {
            if let Some(ctl) = slot.as_ref() {
                let rate = ctl.diagnostics().output_rate.max(1) as f32;
                let dropped = ctl.force_barge_in();
                return serde_json::json!({
                    "droppedSamples": dropped,
                    "droppedSecs": dropped as f32 / rate,
                    "stopLatencySecs": ctl.stop_latency_secs(),
                });
            }
        }
    }
    serde_json::json!({ "droppedSamples": 0, "droppedSecs": 0.0, "stopLatencySecs": 0.0 })
}

/// Developer-facing audio facts. Never rendered to the CEO.
#[richos_command::command]
fn voice_diagnostics(app: AppHandle) -> Option<String> {
    let handle = app.try_state::<VoiceHandle>()?;
    let slot = handle.controller.lock().ok()?;
    let ctl = slot.as_ref()?;
    Some(ctl.diagnostics().summary())
}

// =====================================================================================
// ENTITY + THREAD NAVIGATION — appended block (Codex-UX slice 4, 2026-08-29)
//
// `docs/design/richos-codex-inspired-conversation-ux-2026-08-28.md` §3 (left navigation),
// §25 (Navigation acceptance criteria).
//
// THE POINT OF THIS BLOCK, in one sentence: grouping is done HERE, by the authority that
// owns the binding, and never by the renderer filtering a flat list.
//
// §1 says an entity is *"also a hard scope and privacy boundary"*, and the brief for this
// slice is explicit that selecting an entity must actually scope what follows rather than
// filter a list on screen. A renderer that receives every thread and sorts them into
// buckets by an `entity_id` string is one `if` away from putting a thread in the wrong
// bucket, and nothing would catch it. So `navigation_tree` resolves each thread through
// the ledger's immutable binding and emits threads already inside their entity's group. A
// thread whose binding cannot be produced is not placed anywhere: it goes to `unbound`,
// which is a separate list and not an entity.
//
// Everything below is APPEND-ONLY — nothing above is reordered or edited except four
// mechanical seams (the `mod nav;` line, the `nav` field on `AppState`, its construction
// in `setup`, and the command names at the END of `generate_handler!`).
// =====================================================================================

use richos_core::entity::EntityStatus;
use richos_core::ledger::TurnState;

/// One registered entity area, as the rail renders it (§3.1 "Each entity row").
#[derive(serde::Serialize)]
struct EntityView {
    id: String,
    display_name: String,
    /// "active" | "archived"
    status: String,
    /// The bound source root(s) — §3.1's entity overflow shows *"bound source root or
    /// primary workspace when one exists"*. Empty when the entity has none registered.
    roots: Vec<String>,
}

/// One thread row. `title` is the LEDGER's title (evidence); `display_title` is what the
/// rail shows, which is the CEO's rename override when one exists. Both are sent so the
/// rename UI can offer the original back without a second round trip.
#[derive(serde::Serialize)]
struct ThreadRow {
    id: String,
    title: String,
    display_title: String,
    entity_id: Option<String>,
    binding_revision: u64,
    created_at: u64,
    last_activity: u64,
    message_count: usize,
    pinned: bool,
    archived: bool,
    /// The state of this thread's most recent CEO-visible turn in the DURABLE ledger:
    /// "completed" | "interrupted" | "received" | "in_flight", or `None` when the thread
    /// has never taken a turn. `None` for an unbound thread too — its turns are not read.
    last_turn_state: Option<String>,
    /// A turn is still non-terminal on disk. Combined with "this session has seen no live
    /// turn for this thread", that is an outcome nobody knows — which the rail renders as
    /// UNKNOWN rather than as idle. §22: *"If the source signal does not exist, build the
    /// signal first or show unknown."*
    has_pending_turn: bool,
}

/// One entity group: the entity, and the threads whose IMMUTABLE home is that entity.
#[derive(serde::Serialize)]
struct NavGroup {
    entity: EntityView,
    threads: Vec<ThreadRow>,
}

/// The authoritative active scope. The main-pane header renders from THIS, never from the
/// renderer's own idea of what is selected — so a UI bug can show the wrong thread but can
/// never mislabel which entity the CEO is talking to.
#[derive(serde::Serialize, Clone)]
struct ActiveContext {
    thread_id: String,
    entity_id: String,
    binding_revision: u64,
}

#[derive(serde::Serialize)]
struct NavigationTree {
    groups: Vec<NavGroup>,
    /// Pre-entity records (`ThreadEntity::Unbound`). Their own top-level list — never
    /// folded into an entity, because choosing one would be exactly the guess slice 1
    /// refused to make.
    unbound: Vec<ThreadRow>,
    active: Option<ActiveContext>,
    /// The verbatim core explanation for an unbound thread, so the UI renders the SAME
    /// sentence the ledger raises rather than a paraphrase that can drift from it.
    unbound_explanation: String,
}

fn entity_view(e: &richos_core::entity::Entity) -> EntityView {
    EntityView {
        id: e.id.to_string(),
        display_name: e.display_name.clone(),
        status: match e.status {
            EntityStatus::Active => "active".to_string(),
            EntityStatus::Archived => "archived".to_string(),
        },
        roots: e.roots.iter().map(|p| p.display().to_string()).collect(),
    }
}

fn turn_state_str(s: TurnState) -> &'static str {
    match s {
        TurnState::Received => "received",
        TurnState::InFlight => "in_flight",
        TurnState::Completed => "completed",
        TurnState::Interrupted => "interrupted",
        // Distinct from `interrupted` all the way to the rail: §6.1's "You stopped after
        // {duration}" is an attribution to the CEO, and the sidebar must not describe work
        // he ended as work that broke.
        TurnState::Stopped => "stopped",
    }
}

/// The durable, per-thread turn facts the rail is allowed to show.
///
/// Read through `Ledger::thread_turns`, the SCOPED accessor — not the unscoped
/// `Ledger::turns()`. That matters: `thread_turns` refuses an unbound thread and drops
/// quarantined cross-entity turns, so this function structurally cannot report a turn that
/// does not belong to the thread it is describing. An unbound thread therefore reports
/// `(None, false)` — no state, no pending flag — which is the truth: its turns are not
/// readable, so their outcome is not knowable from here.
fn thread_turn_facts(ledger: &Ledger, thread_id: &str) -> (Option<String>, bool) {
    let Ok(turns) = ledger.thread_turns(thread_id) else { return (None, false) };
    // Internal turns (re-prime, handoff-summary) are machinery, never the CEO's work — a
    // completed re-prime must not make a thread look like it finished something.
    let visible: Vec<_> = turns.into_iter().filter(|t| t.source != Source::Internal).collect();
    let pending = visible.iter().any(|t| matches!(t.state, TurnState::Received | TurnState::InFlight));
    let last = visible.iter().max_by_key(|t| t.created_at).map(|t| turn_state_str(t.state).to_string());
    (last, pending)
}

/// The sentence an unbound thread is explained with. It is the CORE error's own wording
/// (`LedgerError::UnboundThread`, ledger.rs), lifted deliberately rather than re-written:
/// §21 "Entity binding failure" wants one honest statement, and two independently-authored
/// versions of it would drift the moment either side is edited.
const UNBOUND_THREAD_EXPLANATION: &str =
    "This thread has no entity home: it predates entity scoping and Rich will not guess \
     which entity this work belongs to. An operator must bind it explicitly.";

fn active_binding_view(spine: &dyn SpineView) -> Option<ActiveContext> {
    spine.active_binding().map(|b| ActiveContext {
        thread_id: b.thread_id().to_string(),
        entity_id: b.entity_id().to_string(),
        binding_revision: b.binding_revision(),
    })
}

/// THE navigation query. One call returns the whole rail: every registered entity as its
/// own group, each group's threads resolved through the immutable binding, the unbound
/// quarantine list, and the authoritative active scope.
#[richos_command::command]
fn navigation_tree(state: State<AppState>) -> NavigationTree {
    let spine = state.reader.snapshot();
    let nav = state.nav.lock().unwrap();
    build_navigation_tree(&*spine, nav.state())
}

/// The command's whole body, taking plain references instead of Tauri state — so the rail's
/// grouping can be tested against a REAL ledger file rather than only exercised by hand.
fn build_navigation_tree(spine: &dyn SpineView, nav_state: &nav::NavState) -> NavigationTree {
    let ledger = spine.ledger();
    let registry = spine.entity_registry();

    let summaries = spine.threads();
    let row = |s: &ThreadSummary| -> ThreadRow {
        let (last_turn_state, has_pending_turn) = thread_turn_facts(ledger, &s.id);
        let display_title = nav_state.display_title(&s.id, &s.title);
        ThreadRow {
            id: s.id.clone(),
            title: s.title.clone(),
            display_title,
            entity_id: s.entity_id.clone(),
            binding_revision: s.binding_revision,
            created_at: s.created_at,
            last_activity: s.last_activity,
            message_count: s.message_count,
            pinned: nav_state.pinned_threads.iter().any(|t| t == &s.id),
            archived: nav_state.archived_threads.iter().any(|t| t == &s.id),
            last_turn_state,
            has_pending_turn,
        }
    };

    let mut groups: Vec<NavGroup> = Vec::new();
    for entity in registry.entities() {
        let id = entity.id.to_string();
        let mut threads: Vec<ThreadRow> = summaries
            .iter()
            // The binding, not a label: `ThreadSummary::entity_id` is filled in by
            // `thread::summaries` from `Ledger::thread_binding`, which reads the immutable
            // record and fails closed for an unbound thread.
            .filter(|s| s.entity_id.as_deref() == Some(id.as_str()))
            .map(&row)
            .collect();
        // Most recent first; the renderer never re-sorts, so ordering is one decision in
        // one place.
        threads.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        groups.push(NavGroup { entity: entity_view(entity), threads });
    }

    let mut unbound: Vec<ThreadRow> =
        summaries.iter().filter(|s| s.entity_id.is_none()).map(&row).collect();
    unbound.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));

    NavigationTree {
        groups,
        unbound,
        active: active_binding_view(spine),
        unbound_explanation: UNBOUND_THREAD_EXPLANATION.to_string(),
    }
}

/// **WHAT DID NOT LOAD OUT OF HIS HISTORY**, and why.
///
/// Empty `headline`/`detail` with `skipped: 0` is the ordinary answer and the signal for
/// the window to render nothing at all. Non-empty means `Ledger::replay` found a record on
/// disk it could not fold: one written by a newer RichOS, a damaged one, or one it cannot
/// tell apart — three different states, counted separately, never merged into "some
/// records failed".
///
/// The counts describe the LOAD, not the running session: they are taken at replay and do
/// not move as this session appends. "20 of 21 records" is an answer about what came off
/// disk when the app opened, which is the question being asked.
///
/// The sentences are composed in richos-core and rendered verbatim, so this shell never
/// gets to phrase a claim about a customer's history.
#[richos_command::command]
fn history_health(state: State<AppState>) -> richos_core::ledger::HistoryHealth {
    state.reader.snapshot().ledger().history_health()
}

// ---------------------------------------------------------------------------------------
// THE FIRST-RUN NOTICE — the VISIBLE half of the onboarding offer
//
// THE GAP THIS CLOSES, in the words of the engineer who left it: *"The first-run sheet is
// not built. Rich makes the offer in conversation; a CEO who does not read the first reply
// never sees it."* (`docs/verification/onboarding-honesty-2026-09-06/README.md`, "What is
// NOT built", item 2.) The offer reaches him ONLY as part of a reply, and only after he has
// typed something first — measured cell D1. A person who opens RichOS, looks at the screen
// and types nothing is told nothing at all, and the screen he lands on is an empty
// conversation.
//
// WHY TWO COMMANDS AND NOT ONE. `onboarding_view` reads and `decline_onboarding` writes,
// and the read is DERIVED from the same two facts on disk that the priming block is derived
// from — so the notice on screen and the block in the priming turn cannot disagree about
// whether he has been asked. There is no third command and no state that says "the notice
// was shown": `onboarding.rs`'s module doc bans it by name, because a flag recording that
// the app SHOWED something is exactly the failure this whole line of work exists to remove.
//
// WHY THE WRITE HAS TO EXIST AT ALL. `OnboardingRecord::record_declination` shipped with no
// caller anywhere in the product, so `OnboardingState::Declined` and `DECLINED_BLOCK` were
// unreachable outside their own tests and "not now" could only ever be said into a
// conversation that ends. That is the M5 nag with nothing holding it back.
// ---------------------------------------------------------------------------------------

/// Where this install stands on onboarding, for the conversation the CEO is looking at.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OnboardingView {
    /// `no-central-folder` | `not-yet` | `declined` | `described` | `unusable`.
    ///
    /// Five states and not a boolean, because the window must not collapse them: "nothing
    /// has looked" and "it looked and there is nothing" are different statements, and only
    /// one of them is an invitation to spend twenty minutes. `unusable` is the only one that
    /// needs a person, and it is the only one that gets said differently.
    state: String,
    /// The company the answer is about, so a notice can never be shown against a company it
    /// was not derived for. `None` when no thread is bound, which is itself a reason to
    /// render nothing.
    entity_id: Option<String>,
    /// What the CEO is told when his notes exist and cannot be used — the scannable fact.
    /// `None` in every other state, and the window renders it verbatim.
    headline: Option<String>,
    /// The consequence and the party, under that headline. `None` in every other state.
    ///
    /// **BOTH ARE COMPOSED HERE, and neither is `CompanyLayer::describe`**, which is the
    /// operator's sentence and carries a path and a byte count
    /// (`"…/company.md is 40961 bytes, over the 40960-byte budget"`). `setup_view.rs` sets
    /// the register the CEO's screen owes — whole sentences, no paths, no digits — and the
    /// boot log already prints the operator's half, so the two readers each get the one
    /// written for them instead of sharing one written for neither.
    ///
    /// They live here rather than in the window for the reason `history_health` composes both
    /// of its own: "I could not read it" and "there was nothing to read" are different
    /// statements, and a renderer able to phrase either is a renderer able to substitute one
    /// for the other.
    message: Option<String>,
}

/// What he is told when there are notes about his company and they could not be read.
///
/// `UNUSABLE_BLOCK`'s instruction to Rich is *"Do not guess at what it said. If he asks about
/// it, tell him plainly that his notes about this company could not be read and that whoever
/// set RichOS up will need to look at it."* These are the same three claims on the screen —
/// could not read, will not guess, and whose job it is — so the window and the conversation
/// say one thing rather than two versions of one thing.
const ONBOARDING_UNUSABLE_HEADLINE: &str = "I couldn't read your notes about this company.";
const ONBOARDING_UNUSABLE_MESSAGE: &str =
    "I'm working without them and I won't guess at what they said. Whoever set RichOS up \
     will need to look at that.";

fn onboarding_view_in(reader: &SpineReader) -> OnboardingView {
    let spine = reader.snapshot();
    let Some(binding) = spine.active_binding() else {
        return OnboardingView {
            state: "no-central-folder".to_string(),
            entity_id: None,
            headline: None,
            message: None,
        };
    };
    let entity_id = Some(binding.entity_id().to_string());
    let (name, headline, message) = match spine.onboarding_state(binding) {
        richos_core::onboarding::OnboardingState::NoCentralFolder => ("no-central-folder", None, None),
        richos_core::onboarding::OnboardingState::NotYet => ("not-yet", None, None),
        richos_core::onboarding::OnboardingState::Declined { .. } => ("declined", None, None),
        richos_core::onboarding::OnboardingState::Partial => ("partial", None, None),
        richos_core::onboarding::OnboardingState::Described => ("described", None, None),
        richos_core::onboarding::OnboardingState::Unusable { why } => {
            // The operator's half goes to the operator's channel and nowhere else — it is the
            // only place the path and the byte count belong.
            eprintln!("[richos] onboarding: company notes unusable — {why}");
            (
                "unusable",
                Some(ONBOARDING_UNUSABLE_HEADLINE.to_string()),
                Some(ONBOARDING_UNUSABLE_MESSAGE.to_string()),
            )
        }
    };
    OnboardingView { state: name.to_string(), entity_id, headline, message }
}

/// Read where onboarding stands. The window calls it at boot and after every company change,
/// and `not-yet` is what makes it show the offer.
#[richos_command::command]
fn onboarding_view(state: State<AppState>) -> OnboardingView {
    onboarding_view_in(&state.reader)
}

/// **HE PRESSED "Not now".** Record it, and return the state that results.
///
/// Three properties this has and must keep:
///
///   1. **It records an ANSWER, never a viewing.** The only thing written is that he was
///      asked and declined. There is no counter and no timestamp of an offer — `onboarding.rs`
///      carries a test whose only job is to refuse a second field on that record.
///   2. **A failure to write is REPORTED, never swallowed.** The window renders the refusal
///      rather than closing the notice, because a declination that silently went nowhere puts
///      him back in the state where he is asked again forever, and he would have no way to
///      know it.
///   3. **It is reversible by asking.** `DECLINED_BLOCK` tells Rich the interview is still
///      there if the CEO brings it up. The surface says the same thing in the CEO's own
///      words, so the button's effect and the product's behavior are one claim.
#[richos_command::command]
fn decline_onboarding(state: State<AppState>, entity_id: String) -> Result<OnboardingView, String> {
    let now_millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    decline_onboarding_in(&state.reader, &entity_id, now_millis)
}

/// The body of [`decline_onboarding`], without Tauri's `State`, so a test can drive it.
///
/// **IT NEVER WAITS FOR THE SPINE** (a user's tap is answered at once; nothing he just did
/// waits behind work the app does for itself). It used to take the spine for the whole of
/// it, and at boot the spine is held by `prime_front_desk` for a model turn: measured
/// 2026-10-11 in the built app, the decline asked at +7898 ms and got the spine at
/// +98797 ms, so the card stayed on screen 90899 ms after he pressed "Not now"
/// (`docs/verification/2026-10-11-preferences-close-walk/README.md`, step 17).
///
/// So the answer is checked and written from ONE published snapshot (the record's path and
/// the open company are both published, `read_view.rs`), and the card gets its answer from
/// the same read the window already uses.
///
/// **NOTHING ELSE IS OWED TO THE DESKS.** Every desk (the one in use, each parked one, the
/// spare, and a spare still being primed) carries the onboarding block it was primed with,
/// and before every message `prime_lease_if_needed` compares that block with what the record
/// says now and re-primes the desk if they differ. So the next message on any desk carries
/// the decline, and a desk that already carries it (primed after the answer) is not primed
/// again. A separate step that un-primed desks here cost a redundant hidden model turn in
/// two orderings (reviews rv-20261011T101519Z-e4ab1771-98bb, rv-20261011T104029Z-c8b2a74a-37e4).
fn decline_onboarding_in(
    reader: &SpineReader,
    entity_id: &str,
    now_millis: u64,
) -> Result<OnboardingView, String> {
    let outcome = {
        let view = reader.snapshot();
        if view.active_binding().map(|b| b.entity_id().as_str()) != Some(entity_id) {
            return Err("The company changed. Please use the offer for the company now open.".into());
        }
        view.record_onboarding_declination(now_millis)
    };
    if let Err(why) = outcome {
        // THE ERROR THE CEO SEES IS NOT THE ERROR THE CRATE RAISED, and that is deliberate
        // rather than a wrapper for its own sake. `record_declination` writes through
        // `doctrine::write_verified`, so a failure comes back as `DoctrineError::Unwritable`,
        // whose own words are *"the standing instruction could not be written to <path> —
        // RichOS will not start Claude without it"*. Every clause of that is false about this
        // file: it is not the standing instruction, it carries an absolute path, and nothing
        // about starting Claude depends on it. Put on his screen — and it was, the first time
        // this surface rendered a refusal — it is an alarming sentence about the wrong
        // subject. The technical half goes to the operator's channel where it is true and
        // useful.
        eprintln!("[richos] onboarding: declination NOT recorded — {why}");
        return Err(ONBOARDING_DECLINE_REFUSED.to_string());
    }
    Ok(onboarding_view_in(reader))
}

/// What he is told when "Not now" could not be written down.
///
/// It says the thing did not happen, in his register, and it says why that matters: the
/// alternative — closing the notice over a write that failed — is this project's own named
/// failure mode, reporting success over work that never happened.
const ONBOARDING_DECLINE_REFUSED: &str =
    "I couldn't write that down, so it isn't recorded and I'll ask again next time. I'd \
     rather say so than let you think it was settled. Whoever set RichOS up will need to \
     look at that.";

/// The authoritative answer to "which entity and thread is the CEO actually talking to?".
#[richos_command::command]
fn active_context(state: State<AppState>) -> Option<ActiveContext> {
    active_binding_view(&*state.reader.snapshot())
}

/// One thread's durable scope. Fallible on purpose: an unbound thread returns the core's
/// own `UnboundThread` message, which is what the UI renders in the binding-failure state.
#[richos_command::command]
fn thread_scope(state: State<AppState>, thread_id: String) -> Result<ActiveContext, String> {
    let spine = state.reader.snapshot();
    spine
        .ledger()
        .thread_binding(&thread_id)
        .map(|b| ActiveContext {
            thread_id: b.thread_id().to_string(),
            entity_id: b.entity_id().to_string(),
            binding_revision: b.binding_revision(),
        })
        .map_err(|e| e.to_string())
}

/// Create a thread inside an EXPLICITLY CHOSEN entity and make it the active context.
///
/// This is the command that makes entity selection real rather than cosmetic. The older
/// `create_thread` takes its entity from launch-time root resolution and cannot express
/// "the CEO picked Deeply in the picker"; this one takes the choice as an argument and
/// hands it to `Spine::create_thread`, which refuses an unregistered entity
/// (`SpineError::UnknownEntity`) rather than inventing one.
///
/// §3.3: *"no pre-created thread record until the CEO sends the first message"* — so the
/// UI holds a draft with no record and calls this on first send, not on picker open.
#[richos_command::command]
fn create_thread_in(app: AppHandle, state: State<AppState>, entity_id: String, title: String) -> Result<String, String> {
    let entity = EntityId::parse(entity_id.trim()).map_err(|e| e.to_string())?;
    let title = if title.trim().is_empty() { "New thread".to_string() } else { title.trim().to_string() };
    {
        let mut spine = take_the_spine(&state.spine);
        // **AND THIS IS WHERE A SPARE FRONT DESK IS SPENT** — `Spine::create_thread` gives the
        // thread the id the spare was already scoped to and files the spare as its desk, primed.
        // `get_timeline` -> `ready_the_front_desk` then finds nothing left to do, which is the
        // whole of the saving (CEO §55).
        let id = spine.create_thread(&title, &entity).map_err(|e| e.to_string())?;
        // Activate it: `Spine::create_thread` only auto-activates when nothing is active, and
        // a thread the CEO just started must become the scope every subsequent send runs under.
        spine.switch_thread(&id).map_err(|e| e.to_string())?;
        // THE LOCK IS RELEASED BEFORE THE NEXT SPARE IS ASKED FOR, and the scope block is what
        // guarantees it: the spare's priming is a model turn, and starting one while this
        // command still held the mutex would put it in front of the `send_message` that is
        // microseconds behind this return. The ordering is the point, not the thread.
        drop(spine);
        tell_the_phone_the_threads(&app);
        ready_a_spare_front_desk(&app, entity);
        Ok(id)
    }
}

// ---- WHICH COMPANY THIS COPY OF RICH WORKS FOR (slice 4) ------------------------------
//
// The two commands that close the double-click blocker. `create_thread_in` above already
// made an entity choice REAL for a new thread; what did not exist was a way for the CEO to
// answer the question ONCE, durably, for the copy of RichOS on his machine — so a Finder
// launch, whose working directory is `/`, reached no entity at all and refused his first
// sentence.
//
// WHAT IS DELIBERATELY NOT HERE: a default. Nothing below ever picks an entity. `choose_entity`
// refuses an unregistered id, refuses while the environment pins one, and writes only what
// the CEO clicked.

/// One company the CEO can pick, as the picker and the settings row render it.
#[derive(serde::Serialize)]
struct EntityOption {
    id: String,
    display_name: String,
    /// The bound source root(s), so the row can say which is which when two companies have
    /// similar names. Empty when the entity has none registered.
    roots: Vec<String>,
    /// How many threads are already filed here. `0` is a real and renderable answer.
    thread_count: usize,
}

/// The whole state of the question "which company is this copy of Rich for?".
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EntityChoiceView {
    /// The entity in force right now, or `None` — which is the state that makes the app ask.
    chosen: Option<String>,
    /// Where `chosen` came from: `environment` | `saved-choice` | `working-directory`.
    source: Option<String>,
    /// `RICHOS_ENTITY` decided it, so nothing in this window can change it. The surface
    /// renders a statement and names who owns the fix, rather than a control that does not
    /// work (§21: a state he cannot change has to say who can).
    pinned_by_environment: bool,
    /// Every registered company, in registry order. Never filtered, never re-sorted.
    ///
    /// **EMPTY IS A REAL ANSWER since 2026-09-04**, and it is what a first launch gets. The
    /// list used to be six compiled-in companies belonging to the app's author, so this
    /// could never be empty and the surface never had to render the state where the honest
    /// thing to say is "you have not told me about any company yet".
    options: Vec<EntityOption>,
    /// Where the registry came from: `absent` | `file` | `unreadable`.
    ///
    /// The third is the one the surface must not collapse into the first. "You have not told
    /// me your companies yet" is a question; "you told me and I could not read it" is a
    /// repair, and answering the second with the first invites the owner to re-enter a list
    /// that is already on disk one typo away from working.
    registry_source: String,
    /// The file the answer is written to. Shown so the owner (or whoever set RichOS up) can
    /// find it, and so a `unreadable` state names the file to fix rather than describing it.
    registry_path: String,
    /// The scope in force, so the caller does not need a second round trip after choosing.
    active: Option<ActiveContext>,
}

/// What the CEO is told when the environment has pinned the company. Same register as
/// `LEASE_UNAVAILABLE_MESSAGE` and `ENTITY_UNRESOLVED_MESSAGE`: it says what it will not
/// do, why, and who owns the fix, and it invents no control.
const ENTITY_PINNED_MESSAGE: &str =
    "This copy of me was told which company it works for when it was started up, from \
     outside this window, so I can't move it from in here. Whoever set RichOS up is the \
     one who changes that.";

/// What he is told when a chosen id is not one of the companies this build knows about.
/// It cannot happen from the picker (the picker's rows ARE the registry); it can happen
/// from a stale saved value or a hand-driven call, and refusing is the fail-closed half of
/// ECS §3.3.
fn unknown_company_message(id: &str) -> String {
    format!(
        "I don't have a company called \"{id}\" on file, so I won't file anything under it. \
         Pick one of the companies I do have or whoever set RichOS up can add it."
    )
}

fn entity_choice_view(state: &State<AppState>) -> EntityChoiceView {
    // Lock order, everywhere in this file: config, then entity, then spine.
    let chosen = state.entity.lock().unwrap().clone();
    let spine = state.reader.snapshot();
    let registry = spine.entity_registry();
    let summaries = spine.threads();
    let options = registry
        .entities()
        .iter()
        .map(|e| {
            let id = e.id.to_string();
            EntityOption {
                thread_count: summaries
                    .iter()
                    .filter(|s| s.entity_id.as_deref() == Some(id.as_str()))
                    .count(),
                display_name: e.display_name.clone(),
                roots: e.roots.iter().map(|r| r.display().to_string()).collect(),
                id,
            }
        })
        .collect();
    EntityChoiceView {
        chosen: chosen.map(|e| e.to_string()),
        source: state.entity_source.lock().unwrap().map(|s| s.as_str().to_string()),
        pinned_by_environment: state.entity_pinned_by_env,
        options,
        registry_source: state.registry_source.as_str().to_string(),
        registry_path: state.registry_path.display().to_string(),
        active: active_binding_view(&*spine),
    }
}

// ---------------------------------------------------------------------------------------
// FIRST-RUN PROVISIONING — the CEO's memory, on a machine that has none
//
// THE DEFECT THIS CLOSES, in the words of the person who found it: RichOS is installed and
// signed and it reaches his memory ONLY because an engineer created a symlink by hand
// (`docs/verification/installed-app-2026-09-01/README.md` §6). Delete
// `~/Library/Application Support/RichOS/loro-root` and his company memory is gone — measured
// on the installed bundle, in `docs/verification/first-run-provisioning-2026-09-01/`. No
// installer, no first-run flow, nothing in the product creates it.
//
// HIS PART IS ONE CLICK AND NO PATH. `memory_status` carries `offered_location`
// (`~/RichOS/corpus`, pre-filled), the window renders it as a sentence with a button, and
// `provision_memory` takes the target it is GIVEN. There is no branch anywhere below that
// picks a location when nobody named one: `provision` returns `NoTarget` for that, which is
// `loro-structure.md`'s "no silent default" enforced at the only door that can create a
// corpus.
// ---------------------------------------------------------------------------------------

/// Read the state of his memory. The window calls it at boot, and a `state` of `none` is
/// what makes it ask — the same shape `entity_choice`'s `chosen: None` already uses.
#[richos_command::command]
fn memory_status(state: State<AppState>) -> MemoryStatus {
    let status = state.memory.lock().unwrap().clone();
    memory::with_offered_location(status, std::env::var_os("HOME").map(PathBuf::from).as_deref())
}

/// THE CEO ANSWERS "yes, set it up". Provision, then wire it into the running spine.
///
/// Four properties this has and must keep:
///
///   1. **It creates nothing outside what it was told.** The target arrives from the caller.
///      A missing one is refused by `provision` rather than defaulted, and the window's
///      pre-filled value is the only place `~/RichOS/corpus` is chosen — by him.
///   2. **It never touches a corpus that exists.** `AlreadyACorpus` is a refusal, not a
///      merge. His live pointer at `richos-hq` with 626 records has to survive this command
///      existing, including being called by accident.
///   3. **It never puts a corpus in the product repo**, matching the refusal
///      `loro/lib/layout.js:441` already makes for reading one.
///   4. **It re-wires BOTH HALVES without a relaunch**, through the same
///      `wire_company_memory` and the same `install_correction_desk` the boot ran, so what
///      he gets after answering is what he would have got by restarting.
///
/// Property 4 said "the read half" until 2026-09-01, and the write half was a printed
/// apology: `AppState::correction` was fixed at boot, so a fresh user provisioned his corpus
/// and then had a Rich that could read it and not correct it until he quit and reopened. The
/// window meanwhile told him *"From now on I'll keep what you tell me in that folder"*,
/// which was not true for the rest of that session. It never showed on the CEO's install —
/// his corpus predates the app — and it showed on every customer's first five minutes. The
/// field is a `Mutex` now and this command installs the desk.
///
/// `(async)` for the reason `choose_entity` is async: it takes the spine's mutex, which
/// `send_message` holds for a whole turn.
#[richos_command::command]
fn provision_memory(
    app: tauri::AppHandle,
    state: State<AppState>,
    location: Option<String>,
) -> Result<MemoryStatus, String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    // The caller's value, and NOTHING ELSE IS SUBSTITUTED FOR IT. An absent one reaches
    // `provision` as an empty target and is refused there by name.
    let target = PathBuf::from(location.unwrap_or_default().trim());

    // THE COMPANIES THIS BUILD KNOWS. `create-company` is given the registry rather than a
    // list invented here, so the partitions a fresh corpus gets are exactly the entity areas
    // the rail renders — the condition `entities_with_no_lane` warns about at boot is
    // therefore satisfied on the first launch instead of after a manual pass.
    let registry = state.reader.snapshot().entity_registry().clone();
    let companies: Vec<(String, String)> = registry
        .entities()
        .iter()
        .map(|e| (e.id.to_string(), e.display_name.clone()))
        .collect();

    let report = richos_core::provision::provision(&richos_core::provision::ProvisionRequest {
        target,
        home: home.clone(),
        companies,
        compiler_source: Some(state.engine_dir.lock().unwrap().join("loro")),
    })
    .map_err(|e| e.to_string())?;

    // WHAT ACTUALLY HAPPENED, on the operator's line, before anything is claimed to the CEO.
    eprintln!("[richos] provisioned a corpus at {}", report.root.display());
    match &report.git {
        richos_core::provision::GitOutcome::Committed { sha, branch } => {
            eprintln!("[richos] corpus git: {branch} @ {sha}, no remote")
        }
        richos_core::provision::GitOutcome::Unavailable(why) => eprintln!(
            "[richos] corpus git: NOT a git repository — {why}. The record is on disk and is his; \
             what is missing is the history under it."
        ),
    }
    for c in &report.companies {
        if let Some(problem) = &c.problem {
            eprintln!("[richos] corpus company {}: not created — {problem}", c.id);
        }
    }
    if let richos_core::provision::CompilerOutcome::NoSource { looked_in } = &report.compiler {
        eprintln!(
            "[richos] corpus compiler: NOT installed — looked in: {}. The corpus exists and is his; \
             what is missing is the program that reads it.",
            looked_in.join("; ")
        );
    }

    // RE-WIRE, through the same function the boot ran. Lock order everywhere in this file is
    // config, then entity, then spine — nothing above holds any of them.
    let mut spine = take_the_spine(&state.spine);
    // BOTH HALVES, from one resolution, exactly as the boot does it. `wire_company_memory`
    // installs the reader into the spine and hands back the writer; the writer becomes the
    // desk through the same `install_correction_desk` the boot calls.
    let registry_now = spine.entity_registry().clone();
    let wired = memory::wire_company_memory(&mut spine, &state.loro_provenance, &registry_now, &state.engine_dir.lock().unwrap());
    let mut status = wired.status;

    // AND THE HOME SCREEN'S PICTURE, IN THE SAME SESSION. Property 4 of this command is
    // "it re-wires BOTH HALVES without a relaunch"; `home_field_data` reads a third thing out
    // of the same resolution, and a cell left at `None` here would mean a customer who has
    // just answered "set my memory up" keeps looking at the demonstration until he quits and
    // reopens — which is precisely the defect the correction desk had until 2026-09-01, one
    // surface over.
    *state.loro_install.lock().unwrap() = wired.install;

    // THE WRITE HALF, INTO THE RUNNING APP. The outer lock is taken here and inside the
    // same statement as the spine lock, which is safe because nothing else in this file ever
    // takes `correction` and then `spine` — `desk()` takes `correction`, clones out, and
    // releases before touching anything.
    if let Some(writer) = wired.writer {
        let mut held = state.correction.lock().unwrap();
        if held.is_none() {
            // A CUSTOMER'S FIRST FIVE MINUTES. Until 2026-09-01 this branch printed
            // "corrections become available on the next launch" and left the desk shut.
            match install_correction_desk(&mut spine, &app, &state.data_dir, writer) {
                Some(desk) => {
                    eprintln!(
                        "[richos] loro correction desk: OPEN at {} — installed by provisioning, \
                         no relaunch. Corrections work in this session.",
                        status.root.as_deref().unwrap_or("the corpus just created")
                    );
                    *held = Some(desk);
                }
                // `install_correction_desk` has already said why on its own line. This one
                // says what it MEANS, the same split the boot's CLOSED line uses.
                None => eprintln!(
                    "[richos] loro correction desk: still closed after provisioning — the corpus \
                     exists and the desk's own log could not be opened. Confirming a correction \
                     will refuse and say so rather than appearing to write."
                ),
            }
        }
        // A desk that is ALREADY open is left exactly as it is, and that is not laziness:
        // `provision` refuses `AlreadyACorpus`, so reaching this line with a live desk means
        // the corpus this writer names is the corpus that desk is already writing to.
        // Replacing it would throw away the proposals sitting in its in-memory queue.
    }
    drop(spine);
    status.provisioned_now = true;
    *state.memory.lock().unwrap() = status.clone();
    Ok(memory::with_offered_location(status, home.as_deref()))
}

// ---------------------------------------------------------------------------------------
// FIRST-RUN SETUP — Option D (`setup.rs`, `setup_view.rs`)
//
// THE LAUNCH BLOCKER, in the CEO's own record (§19): "today RichOS runs on his Mac and would
// not run on anyone else's". A customer needs Claude Code AND the engine directory, and the
// engine "ships in no payload and has no route onto another machine at all".
//
// HIS PART IS ONE CONSENT STEP. No terminal, no path, no version number — and the sheet says
// he still needs his own Anthropic account, because row 3.14's second condition is that D
// must not be sold as zero-touch.
// ---------------------------------------------------------------------------------------

#[richos_command::command]
fn provider_auth_status(state: State<AppState>) -> richos_core::provider_auth::AuthView {
    let bin = resolve_claude_bin();
    let view = state.provider_auth.lock().unwrap().refresh(&bin);
    state.quota.set_connecting(view.state == richos_core::provider_auth::AuthState::Connecting);
    // The startup check's answer is the account's verdict (quota.rs `Service::sign_in_known`).
    state.quota.sign_in_known(richos_core::claude_accounts::ACCOUNT_ONE, &view.state);
    view
}
#[richos_command::command]
fn provider_auth_start(state: State<AppState>) -> richos_core::provider_auth::AuthView {
    state.quota.set_connecting(true);
    let bin = resolve_claude_bin();
    let view = state.provider_auth.lock().unwrap().start(&bin);
    state.quota.set_connecting(view.state == richos_core::provider_auth::AuthState::Connecting);
    view
}
#[richos_command::command]
fn provider_auth_poll(state: State<AppState>) -> richos_core::provider_auth::AuthView {
    let bin = resolve_claude_bin();
    let view = state.provider_auth.lock().unwrap().poll(&bin);
    state.quota.set_connecting(view.state == richos_core::provider_auth::AuthState::Connecting);
    // A sign-in has just completed (the window polls only while one runs): a queue held for
    // sign-in starts its first held job now, not at its next look (T3 idea 1, finished).
    // Signed in from this moment, not from the next usage read (quota.rs `Service::sign_in_known`).
    state.quota.sign_in_known(richos_core::claude_accounts::ACCOUNT_ONE, &view.state);
    if view.state == richos_core::provider_auth::AuthState::Connected {
        state.work.signed_in();
    }
    view
}
#[richos_command::command]
fn provider_auth_cancel(state: State<AppState>) -> richos_core::provider_auth::AuthView {
    let view = state.provider_auth.lock().unwrap().cancel();
    state.quota.set_connecting(false);
    view
}

#[richos_command::command]
fn claude_quota(state: State<AppState>, refresh: bool) -> richos_core::quota::View {
    let bin = state.claude_bin.lock().unwrap().clone();
    state.quota.refresh(&bin, refresh)
}

/// **API-equivalent token spend** (§118, round 22.1): the Token spend screen's snapshot of every
/// request at or after `from` (Unix ms: the webview's local midnight, Monday or first of the
/// month; 0 for all time), priced at the published API rates (`richos_core::spend`).
#[richos_command::command]
fn spend_view(state: State<AppState>, from: u64) -> richos_core::spend::ledger::SpendView {
    state.quota.spend_view(from)
}

/// The Token spend screen's live line: the agents working, the latest request in dollars, and
/// how many requests have been counted (more than the snapshot's puts the dot on Refresh).
#[richos_command::command]
fn spend_live(state: State<AppState>) -> richos_core::spend::ledger::SpendLive {
    state.quota.spend_live()
}

#[richos_command::command]
fn claude_quota_activity(state: State<AppState>, thread_id: Option<String>) -> Result<richos_core::quota::holds::Activity, String> {
    let entity = thread_id.as_deref().map(|thread| assignment_scope(&state, thread)).transpose()?;
    let scope = entity.as_ref().zip(thread_id.as_ref()).map(|(entity, thread)| (entity.as_str(), thread.as_str()));
    let mut activity = richos_core::quota::holds::read(&state.data_dir.join("engine-state"), scope).map_err(|e| e.to_string())?;
    let quota = state.quota.view();
    if let richos_core::quota::Admission::Held { resets_at } = quota.admission {
        let weekly = quota.windows.iter().any(|w| w.id == "seven_day"
            && w.used_percent >= richos_core::quota::resets::WEEKLY_THRESHOLD
            && w.resets_at == Some(resets_at));
        // The 20-minute exception belongs only to the five-hour window.
        activity.resumes_at = Some(if weekly { resets_at }
            else { resets_at.saturating_sub(20 * 60_000).saturating_add(1) });
    }
    // Round 16's working row names the account in use once there are two or more.
    activity.account = quota.accounts.iter().find(|a| a.in_use).filter(|_| quota.accounts.len() > 1).map(|a| a.label.clone());
    Ok(activity)
}

#[richos_command::command]
fn approve_claude_reset(state: State<AppState>, offer: richos_core::quota::resets::Offer) -> Result<richos_core::quota::resets::View, String> {
    let view = state.quota.resets.approve(&offer)?;
    state.quota.request_refresh();
    Ok(view)
}
#[richos_command::command]
fn revoke_claude_reset(state: State<AppState>) -> Result<richos_core::quota::resets::View, String> {
    state.quota.resets.revoke()
}

#[richos_command::command]
fn set_claude_quota_policy(state: State<AppState>, policy: richos_core::quota::Policy) -> Result<richos_core::quota::View, String> {
    state.quota.set_policy(policy).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------
// FILL-FIRST: SEVERAL CLAUDE ACCOUNTS (richos-hq docs/plans/2026-10-04-multi-subscription-
// fill-first.md, §15). One Claude Code folder per account; each signed in with the stock
// `claude auth login` under its folder. RichOS never sees a credential.
// ---------------------------------------------------------------------------------------

/// **Add account**: a new, empty Claude Code folder, and the stock browser sign-in started
/// for it at once (the browser opens; the account's row reads it on the next refresh).
#[richos_command::command]
fn claude_account_add(state: State<AppState>, label: String, current_label: Option<String>) -> Result<richos_core::quota::View, String> {
    let view = state.quota.add_account(&label, current_label.as_deref()).map_err(|e| e.to_string())?;
    if let Some(added) = state.quota.accounts.list().last().cloned() {
        if let Some(folder) = added.folder {
            let bin = resolve_claude_bin();
            let mut auth = richos_core::provider_auth::ProviderAuth::for_folder(folder);
            auth.start(&bin);
            *state.account_auth.lock().unwrap() = Some((added.id, auth));
        }
    }
    Ok(view)
}

/// Sign an added account in again (its row's "Sign in").
#[richos_command::command]
fn claude_account_sign_in(state: State<AppState>, id: String) -> Result<richos_core::provider_auth::AuthView, String> {
    let folder = state.quota.accounts.folder(&id).ok_or("Account 1 signs in from the main sign-in.")?;
    let bin = resolve_claude_bin();
    let mut auth = richos_core::provider_auth::ProviderAuth::for_folder(folder);
    let view = auth.start(&bin);
    *state.account_auth.lock().unwrap() = Some((id, auth));
    Ok(view)
}

/// The added account's sign-in, while it runs. `None` when none is running. When it ends, the
/// accounts are read again at once.
///
/// **Round 18's likeliest mistake: the same Claude account signed in again.** When the sign-in
/// comes back connected, the folder is compared with every other account's
/// (`provider_auth::same_account`: what Claude Code says each is signed in as, compared in
/// memory and dropped). The same one is signed out of the new folder at once, so it never runs
/// work twice on one account, and the answer is `sameAccount` with the name of the account it
/// already is (round 18: "You signed in as Home again."). A different one is "added" in Recent
/// changes. The third element is that name.
#[richos_command::command]
fn claude_account_sign_in_poll(state: State<AppState>) -> Option<(String, richos_core::provider_auth::AuthView, Option<String>)> {
    use richos_core::provider_auth::{AuthState, AuthView};
    let bin = resolve_claude_bin();
    let (id, view) = {
        let mut slot = state.account_auth.lock().unwrap();
        let (id, auth) = slot.as_mut()?;
        let view = auth.poll(&bin);
        let id = id.clone();
        if view.state != AuthState::Connecting { *slot = None; }
        (id, view)
    };
    if view.state == AuthState::Connecting { return Some((id, view, None)); }
    let mut same_as = None;
    if view.state == AuthState::Connected {
        if let Some(folder) = state.quota.accounts.folder(&id) {
            same_as = state.quota.accounts.list().into_iter().filter(|a| a.id != id)
                .find(|a| richos_core::provider_auth::same_account(&bin, Some(&folder), a.folder.as_deref()) == Some(true))
                .map(|a| a.label);
            if same_as.is_some() {
                richos_core::provider_auth::logout_in(&bin, &folder);
            } else {
                if let Err(error) = state.quota.accounts.signed_in(&id, richos_core::util::now_millis()) {
                    eprintln!("[richos] claude accounts: the adding could not be recorded ({error})");
                }
                // Signed in from this moment, not from the next usage read (quota.rs `Service::sign_in_known`).
                state.quota.sign_in_known(&id, &view.state);
                // As for Account 1's sign-in: a queue held for sign-in tries its first job now.
                state.work.signed_in();
            }
        }
    }
    state.quota.request_refresh();
    let view = if same_as.is_some() { AuthView::same_account() } else { view };
    Some((id, view, same_as))
}

/// **Cancel** in round 18's add flow, or after "That is the account you already use": the
/// sign-in that is running for `id` is stopped, the folder signed out and the account removed.
/// An adding that never finished leaves no row in Recent changes (`Accounts::remove`).
#[richos_command::command]
fn claude_account_discard(state: State<AppState>, id: String) -> Result<richos_core::quota::View, String> {
    {
        let mut slot = state.account_auth.lock().unwrap();
        if slot.as_ref().is_some_and(|(running, _)| *running == id) {
            if let Some((_, mut auth)) = slot.take() { auth.cancel(); }
        }
    }
    if let Some(folder) = state.quota.accounts.folder(&id) {
        richos_core::provider_auth::logout_in(&resolve_claude_bin(), &folder);
    }
    state.quota.remove_account(&id).map_err(|e| e.to_string())
}

/// **Show email** on an account's card (round 31, the CEO 2026-10-18: "an extra click there
/// somehow to temporarily reveal which email is connected to which account name"). Read at the
/// click from Claude Code's own `claude auth status --json` under that account's folder
/// (`provider_auth::signed_in_as`), returned, and not kept anywhere.
#[richos_command::command]
fn claude_account_email(state: State<AppState>, id: String) -> Result<richos_core::provider_auth::SignedInAs, String> {
    let folder = state.quota.accounts.sign_in_folder(&id).ok_or("That account is no longer on this Mac.")?;
    Ok(richos_core::provider_auth::signed_in_as(&resolve_claude_bin(), folder.as_deref()))
}

/// **Not now** on Rich's one-time suggestion of a second account (round 18, `one-nudge`).
#[richos_command::command]
fn claude_account_nudge_answer(state: State<AppState>, answer: String) -> Result<richos_core::quota::View, String> {
    state.quota.accounts.answer_nudge(&answer).map_err(|e| e.to_string())?;
    Ok(state.quota.view())
}

/// **Remove** an added account: its folder signed out with the stock `claude auth logout`,
/// then deleted. Account 1 is the user's own Claude Code sign-in and is never removed here.
#[richos_command::command]
fn claude_account_remove(state: State<AppState>, id: String) -> Result<richos_core::quota::View, String> {
    if let Some(folder) = state.quota.accounts.folder(&id) {
        richos_core::provider_auth::logout_in(&resolve_claude_bin(), &folder);
    }
    state.quota.remove_account(&id).map_err(|e| e.to_string())
}

/// **Use first** (the CEO 2026-10-06, feedback item 8): he picks the account that drains first.
/// Saved, so it survives a relaunch; the switch at 99% of its week works from it as from any.
#[richos_command::command]
fn claude_account_use_first(state: State<AppState>, id: String) -> Result<richos_core::quota::View, String> {
    state.quota.use_account_first(&id).map_err(|e| e.to_string())
}

/// The one setting (his answer 2): at 93% of the five-hour window, Pause or Switch.
#[richos_command::command]
fn set_claude_at_threshold(state: State<AppState>, value: richos_core::claude_accounts::AtThreshold) -> Result<richos_core::quota::View, String> {
    state.quota.set_at_threshold(value).map_err(|e| e.to_string())
}

/// What this machine is missing, and what the sheet should say about it.
///
/// Called at boot by the window, exactly as `memory_status` is: a `needs` that is non-empty is
/// what makes it ask, and it is the only signal it needs.
#[richos_command::command]
fn setup_status(state: State<AppState>) -> serde_json::Value {
    setup_view::view(&setup_view::detect(Some(&state.engine_dir.lock().unwrap())))
}

/// **May Rich offer dictation now?** (round 19, state 4) Dictation is there, the offer was never
/// made, and which key to name. The window asks after Start, once voice is ready.
#[richos_command::command]
fn dictation_offer(state: State<AppState>) -> dictation_offer::OfferView {
    dictation_offer::view(&state.data_dir, richos_core::dictation_ready())
}

/// **The offer was shown**: `offered: true` in `<data dir>/dictation.json`, so it is never made
/// again.
#[richos_command::command]
fn dictation_offer_shown(state: State<AppState>) -> Result<(), String> {
    dictation_offer::mark_shown(&state.data_dir).inspect_err(|why| {
        eprintln!("[richos] dictation offer: the offer was shown and could not be recorded: {why}")
    })
}

#[richos_command::command]
fn dictation_feature_thread(state: State<AppState>, create: bool) -> Result<Option<String>, String> {
    let key = dictation_offer::view(&state.data_dir, true).key;
    let text = dictation_offer::feature_text(key);
    let record = dictation_offer::feature_record(&state.data_dir);
    let old_key = record.as_ref().ok().and_then(Option::as_ref).and_then(|r| r.key);
    let (thread, message) = match dictation_offer::feature_action(record, key, create) {
        dictation_offer::FeatureAction::Nothing => return Ok(None),
        dictation_offer::FeatureAction::Create => {
            let mut spine = take_the_spine(&state.spine);
            let entity = spine.active_entity().cloned().ok_or_else(|| ENTITY_UNRESOLVED_MESSAGE.to_string())?;
            spine
                .open_conversation_with_message(&entity, dictation_offer::FEATURE_TITLE, &text)
                .map_err(|e| e.to_string())?
        }
        dictation_offer::FeatureAction::Replace { thread, message } => {
            let new_message = take_the_spine(&state.spine)
                .replace_app_message(&thread, message.as_deref(), &text, &dictation_offer::key_change_fact(key, old_key))
                .map_err(|e| e.to_string())?;
            (thread, new_message)
        }
    };
    dictation_offer::mark_feature(&state.data_dir, &thread, &message, key).inspect_err(|why| {
        eprintln!("[richos] dictation feature conversation {thread}: written and could not be recorded: {why}")
    })?;
    Ok(Some(thread))
}



/// **THE CEO PRESSES "Set it up".** Install what is missing, reporting each step on
/// `richos://setup`, and re-point this session at what was installed.
///
/// Three properties it has and must keep:
///
///   1. **A failed step stops the run.** Installing an engine for a Claude that is not there
///      produces a machine that reports two successes and works for nothing.
///   2. **The final status is RE-READ FROM DISK**, not assembled from "every step returned
///      Ok". The same rule `install_claude_code` applies to Anthropic's own exit code.
///   3. **No relaunch.** A successful engine install rewrites `AppState::engine_dir`, which is
///      the same `Arc` the lease factory reads. The next accepted request connects with
///      visible progress and Stop. `provision_memory` set that standard on 2026-09-01 after a
///      customer's first five minutes were spent with a corpus he had just created and a desk
///      that would not open until he quit.
///
/// `(async)` keeps installation downloads off the UI thread. Setup does not acquire the
/// running spine or start an untracked model handshake. The idle spine stays locked through replacement.
#[richos_command::command]
fn run_setup(app: tauri::AppHandle, state: State<AppState>) -> Result<serde_json::Value, String> {
    let selected = state.engine_dir.lock().unwrap().clone();
    let mut spine = state.spine.try_lock().map_err(|_| "Rich is working. Stop or finish the current turn before changing the engine.".to_string())?;
    let registry = spine.entity_registry().clone();
    let workers:Vec<_>=state.control.lease_sessions().iter().map(|session|
        richos_core::app_workers::status(&state.data_dir.join("engine-state"),Some(session))).collect();
    let (worker_state, gap) = richos_core::work_gate::all_workers(&workers);
    if !worker_state.permits_action() { return Err(gap.unwrap_or_else(|| "Worker state is not settled.".into())); }
    let desk = state.correction.lock().unwrap().clone();
    let mut held_desk = match &desk {
        Some(desk) => Some(desk.try_lock().map_err(|_| "A memory correction is being written. Try setup again after it finishes.".to_string())?),
        None => None,
    };
    // Keep the idle spine and writer locked throughout replacement. New requests
    // cannot start against a changing engine and cached child hooks are retired first.
    spine.retire_idle_lease()?;
    let installed = setup_view::run(&app, Some(&selected), &state.engine_dir);
    *state.claude_bin.lock().map_err(|_| "The connection settings could not be updated. Please reopen RichOS.")? = resolve_claude_bin();
    state.quota.disconnect(); // Recheck after installation or a changed Claude binary.
    let engine = state.engine_dir.lock().unwrap().clone();
    spine.clear_memory_wiring();
    let wired = memory::wire_company_memory(&mut spine, &state.loro_provenance, &registry, &engine);
    *state.loro_install.lock().unwrap() = wired.install;
    *state.memory.lock().unwrap() = wired.status;
    if let Some(writer) = wired.writer {
        if let Some(held) = held_desk.as_mut() {
            held.replace_writer(Box::new(writer));
            spine.set_correction_desk(desk.as_ref().unwrap().clone());
        } else {
            *state.correction.lock().unwrap() = install_correction_desk(&mut spine, &app, &state.data_dir, writer);
        }
    } else { *state.correction.lock().unwrap() = None; }
    // **WHICH CONVERSATION IS ON SCREEN**, read while the guard is still in hand so the answer
    // is the one this replacement happened under. Used after the two locks are released.
    let on_screen = spine.active_thread().map(|thread| thread.to_string());
    let repaired = installed.is_ok();
    let view = installed.map(|status| setup_view::view(&status));
    // ===================================================================================
    // AND THE DESK IS MADE READY NOW, NOT ON HIS NEXT MESSAGE (CEO §55)
    // ===================================================================================
    //
    // **Ray's candidate-.11 defect 2.1, measured on the real window.** He pressed "Set it up",
    // the engine installed cleanly at 23:23:40Z, and the log carried NO priming line between
    // that install and his first message at 23:27:41Z — the two `ready_*` lines appear only
    // AFTER it. The back end did not start his turn until ~+11 s and "On it!" reached the
    // screen at +18.3 s. His SECOND message started its turn at +1.0 s, which is the positive
    // control: priming works, it had simply never been asked for.
    //
    // The cause is structural rather than a missed call. `ready_the_front_desk` is hooked to
    // `get_timeline` — the read that opens a thread — and `ready_a_spare_front_desk` to the
    // boot and to thread creation. Before the install there is no engine, so every one of
    // those hooks reached `spawn_scoped` and failed; the install fixes the CAUSE and fires
    // none of the hooks again. So the person who does exactly what the app asked him to do
    // pays the full prime on his very next message.
    //
    // **The two locks are released FIRST, and that is load-bearing** for the reason
    // `create_thread_in` drops its guard before the same call: readying a desk is a MODEL TURN
    // that begins by taking the spine's mutex, and starting it under this function's guard
    // would hold the spine for the install AND the prime with his Send arriving in the middle.
    //
    // **Only on a successful run.** A failed install leaves the same machine it found, and a
    // priming turn against an engine that is still missing is a spawn that fails again.
    drop(held_desk);
    drop(spine);
    if repaired {
        match on_screen {
            // A thread is open: its own desk, and then — inside `ready_the_front_desk` — the
            // spare for the company that thread is filed under.
            Some(thread) => ready_the_front_desk_after_a_repair(&app, &thread),
            // No conversation open yet, which is the FIRST-RUN shape and the one Ray walked:
            // the offer appears on entry, he answers it, and the first thing he does next is
            // type into a brand-new thread. A spare is the only desk that can be ready for a
            // thread that does not exist yet (`ready_a_spare_front_desk`).
            None => {
                if let Some(entity) = state.entity.lock().unwrap().clone() {
                    ready_a_spare_front_desk(&app, entity);
                }
            }
        }
    }
    view
}

/// **A REPAIR ASKS FOR THE DESK AGAIN, AND THE MEMO MUST NOT REFUSE IT** — the CEO's §55.
///
/// `ready_the_front_desk` asks once per thread and remembers it in `front_desk_primed_for`.
/// That memo is right for its own purpose — a thread opened three times must not spend three
/// model turns — and it is exactly wrong here: the ask it remembers is the one that FAILED,
/// because the engine it needed was not on the machine yet. Clearing it is what makes the
/// call below do anything at all.
fn ready_the_front_desk_after_a_repair(app: &AppHandle, thread_id: &str) {
    if let Some(state) = app.try_state::<AppState>() {
        state.front_desk_primed_for.lock().unwrap().clear();
    }
    ready_the_front_desk(app, thread_id);
}

/// Explicit repository access is separate from choosing a company folder.
#[richos_command::command]
fn repository_connections(state: State<AppState>) -> serde_json::Value {
    let view = state.reader.snapshot();
    let registry = view.entity_registry();
    serde_json::json!({"companies":registry.entities().iter().map(|e| serde_json::json!({
        "id":e.id, "name":e.display_name, "repositories":e.connected_repositories
    })).collect::<Vec<_>>()})
}

/// THE SYSTEM FOLDER CHOOSER, for every field where he gives a folder (CEO feedback
/// 2026-10-06, item 2c, and 2026-10-06_03: a click on the field opens a Finder window). One
/// command for all of them, run HERE from Rust exactly as `output_save_copy` runs the save
/// sheet: no `dialog:` permission is granted to the page, which can only ask for a folder
/// and receive the path he chose, or `None` when he cancels.
///
/// Settings chosen, not inherited: parent is the asking window, so it is a sheet on the app;
/// the title is the asking field's (`title`), so the sheet says what the folder is for;
/// creating a folder is allowed, as every Mac chooser allows; the starting folder is NOT
/// set, so macOS opens where he last chose in this app. Waited for on the blocking pool,
/// never on the main thread (the plugin's own rule).
#[richos_command::command]
async fn pick_folder(app: tauri::AppHandle, window: tauri::WebviewWindow, title: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        app.dialog()
            .file()
            .set_parent(&window)
            .set_title(title)
            .set_can_create_directories(true)
            .blocking_pick_folder()
            .and_then(|chosen| chosen.into_path().ok())
            .map(|path| path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| format!("The folder chooser did not open: {e}"))
}

#[richos_command::command]
fn connect_repository(state: State<AppState>, entity_id: String, folder: String) -> Result<serde_json::Value, String> {
    let id = EntityId::parse(&entity_id).map_err(|e|e.to_string())?;
    let engine = state.engine_dir.lock().unwrap().clone();
    let runtime = richos_core::runtime::verify_engine(&engine).map_err(|e|e.to_string())?;
    // Hold the idle spine while the company list changes: its copy is the only one.
    let mut spine = state.spine.try_lock().map_err(|_| "Wait for the current turn to stop before connecting a repository.".to_string())?;
    let (next, repository) = richos_core::repositories::connect_folder(spine.entity_registry(), &id,
        Path::new(folder.trim()), &runtime, &[&engine, &state.data_dir])?;
    next.save(&state.registry_path).map_err(|e| format!("The repository was verified but its connection could not be saved: {e}"))?;
    spine.set_entity_registry(next);
    Ok(serde_json::json!({"entity_id":id,"repository":repository}))
}

/// Read the state of the question. The UI calls this at boot: a `chosen` of `None` is what
/// makes it ask, and it is the only signal it needs.
#[richos_command::command]
fn entity_choice(state: State<AppState>) -> EntityChoiceView {
    entity_choice_view(&state)
}

/// THE CEO ANSWERS. Validate, remember, and let the app carry on without a relaunch.
///
/// Three properties this has and must keep:
///
///   1. **It never re-homes a thread.** A thread's entity is immutable after creation (ECS
///      §3.2, enforced by `ThreadBinding`'s private fields), so this only ever changes what
///      NEW work is filed under. If a conversation is already open it is left exactly where
///      it is — changing the setting must not move a record and must not yank him out of
///      what he is reading.
///   2. **It writes the durable answer BEFORE it activates anything.** A crash between the
///      two costs an activation, which the next boot redoes; the other order would cost the
///      answer, and he would be asked again having already answered.
///   3. **It refuses rather than guesses.** An unregistered id is refused; an environment
///      pin is refused with the sentence that names who owns it.
/// **`(async)` is load-bearing here for the same reason it is on `send_message`.** A plain
/// `#[tauri::command]` on a non-async fn is dispatched as `ExecutionContext::Blocking` and
/// runs inline on the IPC thread; this one takes the spine's mutex, which `send_message`
/// holds for the whole of a turn. Answering "which company is this copy for?" is something
/// the CEO does WHILE Rich may be working, and a settings row that froze the entire IPC
/// channel until the turn finished would be a settings row nobody touches. The DURABLE
/// write happens before the spine is touched at all, so the answer is never lost to a wait.
#[richos_command::command]
fn choose_entity(app: AppHandle, state: State<AppState>, entity_id: String) -> Result<EntityChoiceView, String> {
    if state.entity_pinned_by_env {
        return Err(ENTITY_PINNED_MESSAGE.to_string());
    }
    // AGAINST THE REGISTRY IN FORCE, not against a constant. Lock order is config, then
    // entity, then spine — the company list is read off the spine's published snapshot, which
    // takes no lock at all.
    let id = EntityId::parse(entity_id.trim()).map_err(|_| unknown_company_message(entity_id.trim()))?;
    if !state.reader.snapshot().entity_registry().contains(&id) {
        return Err(unknown_company_message(id.as_str()));
    }

    state
        .config
        .lock()
        .unwrap()
        .set_entity(&id)
        .map_err(|e| format!("I couldn't write that down, so I haven't taken it as your answer: {e}"))?;
    *state.entity.lock().unwrap() = Some(id.clone());
    *state.entity_source.lock().unwrap() = Some(EntitySource::SavedChoice);

    apply_company_choice(&mut take_the_spine(&state.spine), &id)?;
    ready_selected_front_desk(&app);
    Ok(entity_choice_view(&state))
}

/// The spine half of `choose_entity`, as a free function so property 1 above is a TEST
/// rather than a sentence in a doc comment.
///
/// **Activate only when nothing is open.** A thread's entity home is immutable after
/// creation (ECS §3.2), so this could not re-home a conversation even if it tried — but it
/// could switch the CEO out of the one he is reading, mid-sentence, because a setting moved.
/// It does not. With a conversation open, the choice governs the NEXT thread and nothing
/// else; with nothing open — the launch case, which is the one this whole pass exists for —
/// it puts him in a thread in the company he just named, without a relaunch.
fn apply_company_choice(spine: &mut Spine, id: &EntityId) -> Result<(), String> {
    if spine.active_thread().is_some() {
        return Ok(());
    }
    spine.ensure_active_thread_in(id).map_err(|e| e.to_string())?;
    spine.migrate_legacy_onboarding_declination().map_err(|e| e.to_string())?;
    Ok(())
}

// ---- ADDING A COMPANY (2026-09-04) -----------------------------------------------------
//
// THE DEFECT THIS CLOSES: the picker's rows were the registry, the registry was a `const`
// table of the app author's six companies, and there was no door anywhere in the product
// for a second person to add a seventh. He could refuse every send or file his work under
// FemcBoost. `choose_entity` above answers "which of my companies is this?"; nothing
// answered "this one is mine and it is not on your list."
//
// WHAT IS DELIBERATELY NOT HERE, for the second time on this surface: a default. Nothing
// below invents a name, a folder or an id the CEO did not supply. The id is DERIVED from
// the name he typed, which is not an invention — it is the same string, in the character
// class an id is allowed to use.

/// What he is told when the name is blank.
const COMPANY_NAME_REQUIRED_MESSAGE: &str =
    "I need a name for the company before I can file anything under it. Anything you'd \
     recognize on a button is fine. You can change it later.";

/// What he is told when a folder is named and is not one.
fn company_folder_message(folder: &Path, why: &str) -> String {
    format!(
        "I couldn't use \"{}\" as this company's folder: {why}. Give me a folder that's \
         already on this Mac or leave it blank: a company works without one.",
        folder.display()
    )
}

/// Derive a valid, unused entity id from the name the CEO typed.
///
/// **A derivation, not an invention.** `EntityId`'s character class is narrow on purpose
/// (`entity.rs`: an id reaches the filesystem), and asking a non-technical CEO to supply one
/// would be asking him to learn what a path component is. So the id is his own name in that
/// character class: lowercased, every run of anything else collapsed to a single `-`, ends
/// trimmed, bounded at [`ENTITY_ID_MAX_LEN`].
///
/// Collision is resolved by SUFFIX rather than by refusing, because two companies can
/// legitimately produce one slug ("Harbor Analytics" and "Harbor, Analytics") and a person
/// who has just typed a name should not have to guess why it was rejected. The suffix is
/// bounded: after 99 tries it gives up rather than looping.
fn entity_id_from_name(name: &str, registry: &EntityRegistry) -> Result<EntityId, String> {
    let mut slug = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let base: String = slug.trim_matches('-').chars().take(ENTITY_ID_MAX_LEN).collect();
    let base = base.trim_end_matches('-').to_string();
    if base.is_empty() {
        return Err(
            "That name doesn't have any letters or numbers in it, so I can't make a file-safe \
             label out of it. Try a name with a word in it."
                .to_string(),
        );
    }
    for n in 1..=99u32 {
        let candidate = if n == 1 {
            base.clone()
        } else {
            // Keep the WHOLE thing inside the bound, suffix included.
            let suffix = format!("-{n}");
            let keep = ENTITY_ID_MAX_LEN.saturating_sub(suffix.len());
            format!("{}{suffix}", base.chars().take(keep).collect::<String>().trim_end_matches('-'))
        };
        if let Ok(id) = EntityId::parse(&candidate) {
            if !registry.contains(&id) {
                return Ok(id);
            }
        }
    }
    Err(format!(
        "I already have a lot of companies with names like \"{name}\" and I couldn't make a \
         distinct label for another one. Try a name that's a bit more specific."
    ))
}

/// Resolve the folder the CEO typed, or refuse in words he can act on.
///
/// `~` is expanded because he may well type it, and a leading `~` is the one shape that
/// looks absolute to a person and is not absolute to `Path`. Everything else is checked
/// rather than assumed: the path must be absolute (lexical resolution can never match a
/// relative root, so storing one would be a dead entry that looks live) and it must EXIST
/// and be a directory — a typo that names nothing would otherwise register a company that
/// can never be selected by launching from it, and the failure would surface weeks later as
/// "Rich keeps asking me which company this is".
fn resolve_company_folder(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    let expanded = match trimmed.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(rest),
            None => PathBuf::from(trimmed),
        },
        None => PathBuf::from(trimmed),
    };
    if !expanded.is_absolute() {
        return Err(company_folder_message(&expanded, "it isn't a full path from the top of the disk"));
    }
    match std::fs::metadata(&expanded) {
        Ok(m) if m.is_dir() => Ok(expanded),
        Ok(_) => Err(company_folder_message(&expanded, "that's a file, not a folder")),
        Err(_) => Err(company_folder_message(&expanded, "there's nothing at that path on this Mac")),
    }
}

/// THE CEO ADDS ONE OF HIS OWN COMPANIES.
///
/// Five properties this has and must keep:
///
///   1. **It writes the file before it changes anything in memory.** The registry is a
///      privacy boundary; a boundary that moved in this process and not on disk would be
///      back where it started at the next launch, having filed a thread under a company that
///      no longer exists. The `save` runs against a CLONE, and the clone is committed only
///      after the write succeeds.
///   2. **It refuses rather than guesses**, at every step: a blank name, a name with nothing
///      file-safe in it, a folder that is not absolute, a folder that is not there, a folder
///      that overlaps a company he already has.
///   3. **It never re-homes a thread.** A thread's entity is immutable after creation (ECS
///      §3.2). Adding a company changes what NEW work can be filed under, and nothing else.
///   4. **It activates only when nothing is open** — through the same `apply_company_choice`
///      the picker uses, so the first company a first-run user adds puts him straight into a
///      working thread, and a company added later does not yank him out of what he is
///      reading.
///   5. **It leaves an environment pin alone.** `RICHOS_ENTITY` is a statement made from
///      outside the window; registering a company does not override it, and the sentence
///      says who owns that.
///
/// `(async)` for the reason `choose_entity` is: it takes the spine's mutex, which
/// `send_message` holds for the whole of a turn.
#[richos_command::command]
fn register_entity(
    app: AppHandle,
    state: State<AppState>,
    display_name: String,
    folder: Option<String>,
) -> Result<EntityChoiceView, String> {
    let name = display_name.trim().to_string();
    if name.is_empty() {
        return Err(COMPANY_NAME_REQUIRED_MESSAGE.to_string());
    }
    // A folder is OPTIONAL. An entity with no root is legal — it simply cannot be selected
    // by launching from a folder — and it is exactly what the no-orphan migration produces.
    let roots = match folder.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        Some(raw) => vec![resolve_company_folder(raw)?],
        None => Vec::new(),
    };

    // A selected company folder is also its connected project folder. Reuse the
    // Settings connection path, including scope checks and Git initialization.
    let engine = state.engine_dir.lock().unwrap().clone();
    let runtime = if roots.is_empty() { None } else {
        Some(richos_core::runtime::verify_engine(&engine).map_err(|e| e.to_string())?)
    };
    // Lock order everywhere in this file: config, then entity, then spine. The idle spine is
    // held while the company list changes: its copy is the only one.
    let mut registry_spine = state.spine.try_lock().map_err(|_| "Wait for the current turn to stop before adding a company.".to_string())?;
    let (id, mut next) = {
        let current = registry_spine.entity_registry();
        let id = entity_id_from_name(&name, current)?;
        let mut next = current.clone();
        next.register(Entity::try_new(id.as_str(), &name, roots.clone()).map_err(|e| {
            // These are the same refusals `register` makes, in the voice of the surface.
            match e {
                richos_core::entity::EntityError::EmptyDisplayName(_) => {
                    COMPANY_NAME_REQUIRED_MESSAGE.to_string()
                }
                other => format!("I couldn't add that company: {other}"),
            }
        })?)
        .map_err(|e| match e {
            richos_core::entity::EntityError::OverlappingRoot { other, other_root, .. } => format!(
                "That folder is inside (or contains) the folder I already have for \"{other}\" \
                 ({}). If I kept both I wouldn't be able to tell which company work in there \
                 belongs to and I won't guess. Pick a folder that isn't shared or leave it \
                 blank.",
                other_root.display()
            ),
            other => format!("I couldn't add that company: {other}"),
        })?;
        (id, next)
    };

    if let (Some(root), Some(runtime)) = (roots.first(), runtime.as_ref()) {
        next = richos_core::repositories::connect_folder(&next, &id, root, runtime,
            &[&engine, &state.data_dir])?.0;
    }
    // PROPERTY 1: durable first. Nothing in memory has moved yet.
    next.save(&state.registry_path).map_err(|e| {
        format!(
            "I couldn't write that down, so I haven't taken it as your answer. \
             (Writing {} failed: {e})",
            state.registry_path.display()
        )
    })?;
    registry_spine.set_entity_registry(next);
    drop(registry_spine);

    // PROPERTY 4/5: the first company he adds becomes the one in force, unless the
    // environment already decided or he has already answered. Both of those are statements
    // that outrank a side effect of adding a row.
    let already_chosen = state.entity.lock().unwrap().is_some();
    if !already_chosen && !state.entity_pinned_by_env {
        state
            .config
            .lock()
            .unwrap()
            .set_entity(&id)
            .map_err(|e| format!("I added the company but couldn't remember that it's the one in force: {e}"))?;
        *state.entity.lock().unwrap() = Some(id.clone());
        *state.entity_source.lock().unwrap() = Some(EntitySource::SavedChoice);
        apply_company_choice(&mut take_the_spine(&state.spine), &id)?;
    }

    eprintln!(
        "[richos] company registry: added {} (\"{}\"), {} root(s) -> {}",
        id,
        name,
        roots.len(),
        state.registry_path.display()
    );
    ready_selected_front_desk(&app);
    Ok(entity_choice_view(&state))
}

// ---- search (§3.4) --------------------------------------------------------------------

#[derive(serde::Serialize)]
struct SearchHit {
    /// "entity" | "thread" | "message"
    kind: String,
    entity_id: Option<String>,
    entity_label: String,
    thread_id: Option<String>,
    thread_title: Option<String>,
    excerpt: String,
    at: u64,
}

/// Default result cap. §3.4: *"Search must not load all thread bodies into the renderer."*
/// The match runs HERE, against the ledger, and only bounded excerpts cross the IPC
/// boundary — so a long history costs the renderer a few dozen short strings.
const SEARCH_DEFAULT_LIMIT: usize = 40;
/// Per-thread cap on message hits, so one long thread cannot fill the palette and hide
/// every other entity's results.
const SEARCH_HITS_PER_THREAD: usize = 3;
/// Characters of context either side of a match in an excerpt.
const SEARCH_EXCERPT_RADIUS: usize = 60;

/// Case-fold to a `Vec<char>` that is INDEX-ALIGNED with the original's chars.
///
/// `str::to_lowercase` is not usable for locating a match: it can change the character
/// count (`ß` -> `ss`), so an index found in the folded string can point at the wrong
/// character of the original and slice an excerpt in the wrong place. Taking only the
/// first char of each `char::to_lowercase()` keeps the 1:1 mapping the excerpt maths
/// depends on.
fn fold_chars(s: &str) -> Vec<char> {
    s.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect()
}

fn find_folded(hay: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == *needle)
}

fn excerpt_at(original: &[char], at: usize, needle_len: usize) -> String {
    let start = at.saturating_sub(SEARCH_EXCERPT_RADIUS);
    let end = (at + needle_len + SEARCH_EXCERPT_RADIUS).min(original.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(original[start..end].iter());
    if end < original.len() {
        out.push('…');
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Command-palette search over entity names, thread titles and message text (§3.4).
///
/// Scope-safe by construction: message bodies are read with `Ledger::messages`, which is
/// the scoped accessor and refuses an unbound thread — so search can surface an unbound
/// thread's TITLE (already listed in the rail, and a title is navigation metadata) but can
/// never surface its contents. Each hit carries its entity so the renderer groups by entity
/// without having to work out which entity a result came from.
#[richos_command::command]
fn search_nav(state: State<AppState>, query: String, limit: Option<usize>) -> Vec<SearchHit> {
    let spine = state.reader.snapshot();
    let nav = state.nav.lock().unwrap();
    run_search(&*spine, nav.state(), &query, limit.unwrap_or(SEARCH_DEFAULT_LIMIT))
}

fn run_search(spine: &dyn SpineView, nav_state: &nav::NavState, query: &str, limit: usize) -> Vec<SearchHit> {
    let needle = fold_chars(query.trim());
    if needle.is_empty() {
        return Vec::new();
    }
    let limit = limit.clamp(1, 200);
    let ledger = spine.ledger();
    let registry = spine.entity_registry();

    let label_of = |id: Option<&str>| -> String {
        match id.and_then(|i| EntityId::parse(i).ok()).and_then(|i| registry.get(&i)) {
            Some(e) => e.display_name.clone(),
            None => "No entity".to_string(),
        }
    };

    let mut hits: Vec<SearchHit> = Vec::new();

    // 1. Entity names.
    for e in registry.entities() {
        let hay_name = fold_chars(&e.display_name);
        let hay_id = fold_chars(e.id.as_str());
        if find_folded(&hay_name, &needle).is_some() || find_folded(&hay_id, &needle).is_some() {
            hits.push(SearchHit {
                kind: "entity".into(),
                entity_id: Some(e.id.to_string()),
                entity_label: e.display_name.clone(),
                thread_id: None,
                thread_title: None,
                excerpt: e.display_name.clone(),
                at: 0,
            });
        }
    }

    let summaries = spine.threads();

    // 2. Thread titles (ledger title AND the CEO's rename override — searching for what
    //    you can see on screen has to work).
    for s in &summaries {
        let display = nav_state.display_title(&s.id, &s.title);
        let matched = find_folded(&fold_chars(&s.title), &needle).is_some()
            || find_folded(&fold_chars(&display), &needle).is_some();
        if matched {
            hits.push(SearchHit {
                kind: "thread".into(),
                entity_id: s.entity_id.clone(),
                entity_label: label_of(s.entity_id.as_deref()),
                thread_id: Some(s.id.clone()),
                thread_title: Some(display),
                excerpt: String::new(),
                at: s.last_activity,
            });
        }
    }

    // 3. Message bodies — scoped read only; an unbound thread is skipped by the `Err` arm.
    for s in &summaries {
        if hits.len() >= limit {
            break;
        }
        let Ok(messages) = ledger.messages(&s.id) else { continue };
        let display = nav_state.display_title(&s.id, &s.title);
        let mut per_thread = 0usize;
        for m in messages.iter().rev() {
            if per_thread >= SEARCH_HITS_PER_THREAD || hits.len() >= limit {
                break;
            }
            let chars: Vec<char> = m.text.chars().collect();
            let folded = fold_chars(&m.text);
            if let Some(at) = find_folded(&folded, &needle) {
                let excerpt = excerpt_at(&chars, at, needle.len());
                // A message whose whole text IS the thread title (the common case for the
                // first message, since §3.3 derives the provisional title from it) would
                // render as a second identical row under the title hit. One match, one row.
                if excerpt == display {
                    continue;
                }
                hits.push(SearchHit {
                    kind: "message".into(),
                    entity_id: s.entity_id.clone(),
                    entity_label: label_of(s.entity_id.as_deref()),
                    thread_id: Some(s.id.clone()),
                    thread_title: Some(display.clone()),
                    excerpt,
                    at: m.at,
                });
                per_thread += 1;
            }
        }
    }

    hits.truncate(limit);
    hits
}

// ---- durable navigation view state (nav.rs) -------------------------------------------

#[richos_command::command]
fn nav_state(state: State<AppState>) -> nav::NavState {
    state.nav.lock().unwrap().state().clone()
}

/// Returns the width the store ACCEPTED (clamped to UX §2.1's 224–420px), not the width
/// requested — so the rail renders what was persisted and the two cannot disagree.
#[richos_command::command]
fn set_sidebar_width(state: State<AppState>, width: f64) -> Result<f64, String> {
    state.nav.lock().unwrap().set_sidebar_width(width).map_err(|e| e.to_string())
}

/// Returns the width the store ACCEPTED (clamped to nav.rs's 280-520px, whose maximum is
/// derived from §20's 620px conversation floor). UX §7.2 / §25.
#[richos_command::command]
fn set_inspector_width(state: State<AppState>, width: f64) -> Result<f64, String> {
    state.nav.lock().unwrap().set_inspector_width(width).map_err(|e| e.to_string())
}

/// Returns the width the store ACCEPTED: floored at 320px, never capped, because the Output
/// panel's ceiling is the live stop the page measures (output side-panel PRD §9.5, nav.rs).
#[richos_command::command]
fn set_output_width(state: State<AppState>, width: f64) -> Result<f64, String> {
    state.nav.lock().unwrap().set_output_width(width).map_err(|e| e.to_string())
}

#[richos_command::command]
fn set_sidebar_collapsed(state: State<AppState>, collapsed: bool) -> Result<(), String> {
    state.nav.lock().unwrap().set_sidebar_collapsed(collapsed).map_err(|e| e.to_string())
}

#[richos_command::command]
fn set_entity_collapsed(state: State<AppState>, entity_id: String, collapsed: bool) -> Result<(), String> {
    state.nav.lock().unwrap().set_entity_collapsed(&entity_id, collapsed).map_err(|e| e.to_string())
}

#[richos_command::command]
fn set_thread_pinned(state: State<AppState>, thread_id: String, pinned: bool) -> Result<(), String> {
    state.nav.lock().unwrap().set_thread_pinned(&thread_id, pinned).map_err(|e| e.to_string())
}

/// Archive REMOVES FROM THE NORMAL LIST (§3.2) — it does not delete, and it does not touch
/// the thread's entity home. An archived thread is still bound to exactly the entity it was
/// always bound to and is still fully readable from the archive view.
#[richos_command::command]
fn set_thread_archived(state: State<AppState>, thread_id: String, archived: bool) -> Result<(), String> {
    state.nav.lock().unwrap().set_thread_archived(&thread_id, archived).map_err(|e| e.to_string())
}

/// **Haiku's title for a new conversation** (CEO, 2026-10-19), kept as a display title beside
/// his renames. Returns whether it is now what the conversation is called: never when he has
/// named the conversation himself (`nav::NavStore::set_auto_title`).
fn show_haiku_title(nav: &Mutex<nav::NavStore>, thread_id: &str, title: &str) -> bool {
    match nav.lock().unwrap().set_auto_title(thread_id, title) {
        Ok(kept) => kept,
        Err(e) => {
            eprintln!("[richos] Haiku's conversation title was not saved ({e})");
            false
        }
    }
}

/// A DISPLAY override. The ledger's title is evidence and is left exactly as written — see
/// nav.rs's module doc for why a rename is not a ledger edit.
#[richos_command::command]
fn rename_thread(app: AppHandle, state: State<AppState>, thread_id: String, title: String) -> Result<(), String> {
    state.nav.lock().unwrap().rename_thread(&thread_id, &title).map_err(|e| e.to_string())?;
    tell_the_phone_the_threads(&app);
    Ok(())
}

#[cfg(test)]
mod operator_gate_tests {
    //! **N1, the negative control, at the seam where the back end opens** (operator back-end
    //! spec r2 §6, verification step 3; CEO ruling §86: off by default for everyone).
    //!
    //! The work lease factory is driven for real, against an engine directory that does not
    //! exist, so the product path runs until its own first refusal and no process is
    //! spawned. Without `operator.json` that refusal is the product path's own, word for
    //! word, which is what "starts exactly as it does today" can be measured as here. With a
    //! broken or a valid file the gate answers first and the product path never starts.
    use super::*;
    use richos_core::entity::{Entity, EntityId, EntityRegistry};

    fn fixture(tag: &str) -> (PathBuf, richos_core::entity::ThreadBinding, EngineLeaseFactory) {
        let root = std::env::temp_dir().join(format!("richos-operator-gate-{tag}-{}-{}", std::process::id(), richos_core::util::now_millis()));
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let ledger = Ledger::open(root.join("ledger.jsonl")).expect("open ledger");
        let mut spine = Spine::new(ledger);
        spine.set_entity_registry(EntityRegistry::new(vec![
            Entity::new("femcboost", "FemcBoost", &["/fixture/ab/femcboost"]).unwrap(),
        ]).unwrap());
        let thread = spine.create_thread("Gate", &EntityId::parse("femcboost").unwrap()).unwrap();
        let binding = spine.ledger().thread_binding(&thread).unwrap();
        let factory = EngineLeaseFactory {
            quota: Arc::new(richos_core::quota::Service::open(&data).unwrap()),
            claude_bin: Arc::new(Mutex::new(PathBuf::from("/nonexistent/claude"))),
            engine_dir: Arc::new(Mutex::new(root.join("no-engine-here"))),
            data_dir: data,
            explicit_engine: false,
            operator_desk: None,
        };
        (root, binding, factory)
    }

    fn io_error(result: Result<Box<dyn Cognition>, CognitionError>) -> String {
        match result {
            Err(CognitionError::Io(why)) => why,
            Err(other) => panic!("expected an Io refusal, got {other:?}"),
            Ok(_) => panic!("nothing can start against an engine that is not there"),
        }
    }

    #[test]
    fn without_the_declaration_the_work_lease_takes_exactly_todays_path() {
        let (root, binding, factory) = fixture("absent");
        let why = io_error(factory.spawn_work(&binding));
        // The product path's own first refusal for this engine directory, computed by the
        // same two calls the factory makes in the same order (the release gate, which a
        // development build without a pin passes, then the runtime check) — so the gate added
        // nothing and took nothing away.
        let dir = root.join("no-engine-here");
        let expected = richos_core::setup::engine_boot_refusal_demand(
            &dir,
            false,
            richos_core::setup::EngineDemand::pinned(richos_core::setup::engine_pin().as_ref()),
        ).unwrap_or_else(|| match richos_core::runtime::verify_engine(&dir) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("a missing engine cannot verify"),
        });
        assert_eq!(why, expected);
        assert!(!why.contains("Your team"), "the operator gate spoke on an install with no declaration: {why}");
        // And the product path really started: it renders the standing instruction first.
        assert!(!richos_core::doctrine::doctrine_path(&factory.data_dir).exists(),
            "the product must not generate or compile role instructions");
        assert!(!factory.data_dir.join(richos_core::operator_declaration::DECLARATION_FILE).exists());
        if let Err(error) = std::fs::remove_dir_all(&root) { eprintln!("fixture cleanup: {error}"); }
    }

    #[test]
    fn a_broken_declaration_refuses_before_the_product_path_starts() {
        let (root, binding, factory) = fixture("broken");
        std::fs::write(factory.data_dir.join(richos_core::operator_declaration::DECLARATION_FILE), "{").unwrap();
        // Quota initialization already published its snapshot. The gate must leave
        // that existing file untouched and must not render the product doctrine.
        let snapshot = factory.data_dir.join("engine-state/claude-quota.json");
        let before = std::fs::read(&snapshot).unwrap();
        let why = io_error(factory.spawn_work(&binding));
        assert!(why.starts_with("Your team is switched off on this Mac because "), "{why}");
        // The product path renders the standing instruction first thing; it never ran.
        assert!(!richos_core::doctrine::doctrine_path(&factory.data_dir).exists());
        assert_eq!(std::fs::read(&snapshot).unwrap(), before);
        let written: std::collections::BTreeSet<_> = std::fs::read_dir(&factory.data_dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(written, [std::ffi::OsString::from("engine-state"), std::ffi::OsString::from(richos_core::operator_declaration::DECLARATION_FILE)].into_iter().collect());
        if let Err(error) = std::fs::remove_dir_all(&root) { eprintln!("fixture cleanup: {error}"); }
    }
}

#[cfg(test)]
mod send_wait_tests {
    //! **The boundary that nothing wrote down, and what it cost.** See
    //! [`super::spine_wait_notice`] and the block above it.
    //!
    //! NOT IN A CI GATE, for the reason `lease_gate_tests` below gives: the Tauri shell is a
    //! deliberately detached workspace. Run it with
    //! `cargo test --manifest-path app/src-tauri/Cargo.toml`.

    use std::time::Duration;

    #[test]
    fn a_send_during_cold_priming_is_durable_before_the_provider_has_a_session() {
        let root = std::env::temp_dir().join(format!("cold-prime-intake-{}-{}",
            std::process::id(), richos_core::util::now_millis()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("intake.jsonl");
        let control = super::TurnControl::open(&path).unwrap();
        assert!(control.lease_sessions().is_empty());
        assert!(!super::desk_input_can_be_logged(&control, "thread-a"));
        control.for_thread("thread-a").begin_front_desk_prime();
        assert!(super::desk_input_can_be_logged(&control, "thread-a"));
        assert!(!super::desk_input_can_be_logged(&control, "thread-b"));
        control.submit_from_desk("thread-a", Some(super::EntityId::parse("alpha").unwrap()),
            "Keep this first message").unwrap();
        control.for_thread("thread-a").end_front_desk_prime();
        assert!(!super::desk_input_can_be_logged(&control, "thread-a"));
        let reopened = super::TurnControl::open(&path).unwrap();
        assert_eq!(reopened.pending_intake().len(), 1, "a cold-start message did not survive reopening its intake log");
        assert!(std::fs::read_to_string(&path).unwrap().contains("Keep this first message"));
        drop(reopened);
        for thread in ["thread-a","thread-b"] {
            control.for_thread(thread).set_lease_session(Some(format!("session-{thread}")));
            assert!(super::desk_input_can_be_logged(&control,thread));
        }
        assert_eq!(control.lease_sessions().len(),2);
        assert!(!super::desk_input_can_be_logged(&control,"thread-c"));
        drop(control);
        std::fs::remove_dir_all(root).unwrap();
    }


    #[test]
    fn an_ordinary_hop_says_nothing_and_a_hop_that_waited_says_how_long_and_where() {
        // The silent case, and it is silent by ARITHMETIC rather than by a chosen threshold:
        // an uncontended `Mutex::lock` is sub-microsecond and rounds to 0 ms.
        assert_eq!(super::spine_wait_notice("src/main.rs:1", Duration::ZERO), None);
        assert_eq!(super::spine_wait_notice("src/main.rs:1", Duration::from_micros(999)), None);

        // The case this exists for. 4412 ms is candidate .10's own measured pre-prime, so
        // the number in the test is the number off the machine and not an invented one.
        let line = super::spine_wait_notice("src/main.rs:1780", Duration::from_millis(4412))
            .expect("a four-second wait is his, and it has to be written down");
        assert!(line.contains("4412 ms"), "{line}");
        assert!(!line.contains("§55"), "internal rulings do not belong in the log: {line}");
        // WHICH HOP PAID IT. This is the half candidate .12 did not have: the wait was real
        // and the line that would have named it was wired to one call site out of nine.
        assert!(line.contains("src/main.rs:1780"), "the site has to be in it: {line}");
        // The exact sentence the UI shows him while this is happening, so whoever reads the
        // log can join it to the screen without guessing (`ui/main.js`).
        assert!(line.contains("Waiting for Rich to accept it"), "{line}");

        // The smallest wait that is still a wait — the boundary itself, stated.
        assert!(super::spine_wait_notice("src/main.rs:1", Duration::from_millis(1)).is_some());
    }

    /// INVARIANT: every window hop that takes the spine is measured, because there is exactly
    /// ONE way to take it.
    ///
    /// **A scrape, deliberately**, and for the same reason the deferred-road ordering below is
    /// one: a hop added tomorrow that writes `state.spine.lock()` by hand would compile, work,
    /// and be invisible in the log — which is precisely the condition that left six seconds
    /// unattributed on candidate .12. The needles are assembled so they do not match
    /// themselves, exactly as `the_lease_gate_is_never_a_cached_boolean` assembles its own.
    #[test]
    fn there_is_one_door_to_the_spine_and_it_measures_the_wait() {
        let source = include_str!("main.rs");
        let by_hand = concat!("state.spine.", "lock()");
        // CODE ONLY. The phrase is quoted in three doc comments that explain why the door
        // exists, and a scrape that counted prose would forbid the explanation of its own rule.
        let in_code = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .filter(|line| line.contains(by_hand))
            .count();
        assert_eq!(
            in_code, 0,
            "a hop that takes the spine by hand is a hop whose wait nobody can attribute — \
             use take_the_spine(&state.spine), which names the site with #[track_caller]"
        );
        // And the door itself still measures, rather than having quietly become a wrapper.
        let door = source
            .split_once(concat!("fn hold_the_", "spine<'a>("))
            .expect("hold_the_spine must exist").1;
        let body = &door[..door.find("\n}\n").expect("a body")];
        assert!(body.contains("spine_wait_notice"), "the door must write the wait down");
        // Both entrances name the caller's site rather than a typed label. The needle is
        // assembled so it does not match itself — `SOURCE` is this file.
        let caller = concat!("Location::", "caller()");
        assert_eq!(source.matches(caller).count(), 2,
            "take_the_spine and take_the_spine_or_give_up each name their own caller");
    }

    /// INVARIANT: the company list has ONE copy, the spine's (follow-up 1.11, 2026-10-18).
    ///
    /// `AppState` kept a second `EntityRegistry` while `send_message` held the spine for a
    /// whole turn. Turns now run outside that lock (`spine::drive_shared_turns`), and two
    /// copies of one list can disagree. A scrape, for the reason the door test above is one:
    /// a second copy would compile and pass every behavioral test.
    #[test]
    fn the_company_list_has_one_copy() {
        let source = include_str!("main.rs");
        let start = source.find(concat!("struct App", "State {")).expect("AppState must exist");
        let end = start + source[start..].find("\n}\n").expect("a body");
        assert!(!source[start..end].contains(concat!("Mutex<Entity", "Registry>")),
            "AppState must not keep its own copy of the company list; read the spine's");
    }

    /// INVARIANT: `send_message` asks for the deferred road BEFORE it takes the spine's
    /// mutex, and the pre-prime is no longer a reason for that mutex to be held against him.
    ///
    /// **Why this is a scrape of the source rather than a call.** The ordering is the whole
    /// property — `defer_send` after `spine.lock()` would compile, pass every behavioral test
    /// in `richos-core`, and do exactly nothing, because by the time it ran the wait it exists
    /// to avoid would already have been paid. There is no return value that distinguishes the
    /// two orders, so the only honest place to check it is where it is written. The BEHAVIOR —
    /// the handover, the durability, the exactly-once — is pinned on real spines in
    /// `crates/richos-core/tests/deferred_send_tests.rs`; this pins the wiring that reaches it.
    ///
    /// The needles are assembled rather than written for the reason `lease_gate_tests` gives:
    /// `SOURCE` is this file, and a literal needle would match itself.
    #[test]
    fn the_send_asks_for_the_deferred_road_before_it_takes_the_spine() {
        const SOURCE: &str = include_str!("main.rs");
        let start = SOURCE.find(concat!("fn send_", "message(")).unwrap();
        let end = SOURCE[start..].find(concat!("fn refused_", "send(")).unwrap() + start;
        let body = &SOURCE[start..end];

        // **THE NEEDLE MOVED ON 2026-09-20 AND THE INVARIANT DID NOT.** It read
        // `state.control.defer_send(` — the road that answered only while the front desk was
        // being primed. Ray's `.20260920.1` walk measured his own typed message reaching his
        // phone 1.3-6.1 s after he pressed Send while Rich's reply to it crossed in under a
        // second, and a prime was never the cause: an ordinary turn holds the same mutex, and
        // his row cannot be built until his words are in the ledger behind it. So the window
        // takes `submit_from_desk` for EVERY typed sentence now. Same order, same reason, wider
        // door.
        let durable = concat!("state.control.submit_from_", "desk(");
        // The one door to the spine since 2026-09-19 (`take_the_spine`) — the needle follows
        // the spelling, because the ORDER is the property and a needle that stopped matching
        // would pass this test over a send that had started blocking first.
        let lock = concat!("take_the_", "spine(&state.spine)");
        let durable_at = body.find(durable).expect("send_message must write his words down at all");
        let lock_at = body.find(lock).expect("send_message still takes the spine on the ordinary path");
        assert!(
            durable_at < lock_at,
            "his words must be written down BEFORE the mutex — after it, the write can only \
             happen once the wait it exists to avoid has already been paid"
        );

        // **AND THE PHONE IS TOLD IN THE SAME WINDOW.** This is the half Ray measured. The
        // announcement is built from the durable record and published to a hub that is inert
        // with no phone paired, so it costs nothing and cannot precede the record.
        let announce = concat!("phone.announce_his_", "words(");
        let announce_at = body.find(announce).expect("nothing tells his phone what he just typed");
        assert!(
            durable_at < announce_at && announce_at < lock_at,
            "the phone must be told AFTER his words are durable and BEFORE the mutex: durable \
             at {durable_at}, announced at {announce_at}, lock at {lock_at}"
        );

        // The thread he typed into, not the active one. A record filed against whatever thread
        // happened to be active would launder his words across an entity boundary, which is the
        // thing `IntakeRecord::Steer`'s own comment refuses.
        let call = &body[durable_at..];
        assert!(
            call[..call.find(')').unwrap()].contains("&thread_id"),
            "the durable record must name the thread he typed into: {}",
            &call[..80]
        );
    }

    /// INVARIANT: `create_thread_in` RELEASES the spine's mutex before it asks for the next
    /// spare front desk.
    ///
    /// **Why this is a scrape of the source rather than a call, for the same reason as above.**
    /// Readying a spare is a MODEL TURN. `create_thread_in` returns into `openThread` and then
    /// straight into `send_message`, microseconds later — so asking for the spare while this
    /// command still held the mutex would put a fresh multi-second hold in front of the very
    /// Send this whole slice exists to stop making him wait for. The version that does that
    /// compiles, passes every behavioral test in `richos-core`, and is the defect.
    ///
    /// The `drop(spine)` is therefore load-bearing and not tidiness: `ready_a_spare_front_desk`
    /// spawns a thread whose first act is `state.spine.lock()`, and the lock would otherwise be
    /// held until the end of the function.
    ///
    /// The needles are assembled rather than written for the reason `lease_gate_tests` gives:
    /// `SOURCE` is this file, and a literal needle would match itself.
    #[test]
    fn creating_a_thread_lets_go_of_the_spine_before_it_asks_for_the_next_spare() {
        const SOURCE: &str = include_str!("main.rs");
        let start = SOURCE.find(concat!("fn create_thread_", "in(app: AppHandle")).unwrap();
        let end = SOURCE[start..].find("// ---- WHICH COMPANY THIS COPY OF RICH").unwrap() + start;
        let body = &SOURCE[start..end];

        let release = concat!("drop(", "spine)");
        let ask = concat!("ready_a_spare_front_", "desk(&app");
        let release_at = body.find(release).expect(
            "create_thread_in must let go of the spine explicitly — the guard lives to the end \
             of the function otherwise",
        );
        let ask_at = body.find(ask).expect("create_thread_in must ready the NEXT spare at all");
        assert!(
            release_at < ask_at,
            "the spine must be released BEFORE the next spare is asked for — a model turn \
             started under this lock lands in front of the Send that is microseconds behind \
             this return"
        );

        // And the spare is for the company he just filed the thread under, never the active
        // context: a spare primed for the wrong company can never be adopted, so asking for one
        // would spend a model turn on a desk that is guaranteed to be thrown away.
        let call = &body[ask_at..];
        assert!(
            call[..call.find(')').unwrap()].contains("entity"),
            "the spare must be asked for by entity: {}",
            &call[..80]
        );
    }
}

#[cfg(test)]
mod repair_priming_tests {
    //! **THE FIRST MESSAGE AFTER THE ENGINE REFRESH STILL PAID THE FULL PRIME** — Ray's
    //! candidate-.11 defect 2.1, and the CEO's §55.
    //!
    //! Measured on the real window, `docs/verification/2026-09-19-nightly-1.2.0-nightly.20260918.6-mac-and-android-audit.md`
    //! §2: the engine installed cleanly at 23:23:40Z, his first message went in at 23:27:41.06Z,
    //! his turn did not start until ~+11 s and "On it!" reached the screen at **+18.3 s**. The
    //! log carried no priming line at all between the install and that message; the two
    //! [`super::ready_the_front_desk`] / [`super::ready_a_spare_front_desk`] lines appear only
    //! after it. His SECOND message started its turn at +1.0 s — priming works; it had simply
    //! never been asked for.
    //!
    //! **THESE ARE SCRAPES OF THIS FILE, for the reason `send_wait_tests` gives at length.**
    //! The property is an ORDERING and a side effect on another thread: a `run_setup` that
    //! readied the desk while still holding the spine's guard, or that never cleared the memo,
    //! compiles, returns the same value, and passes every behavioral test in `richos-core`
    //! while doing nothing. There is no return value that distinguishes them. The BEHAVIOR the
    //! wiring reaches — a desk whose priming failed for want of a lease is ready before his
    //! next message once it can be primed — is pinned on a real spine in
    //! `crates/richos-core/tests/front_desk_priming_tests.rs`.
    //!
    //! The needles are assembled rather than written, for the reason `lease_gate_tests` gives:
    //! `SOURCE` is this file, and a literal needle would match itself.
    //!
    //! NOT IN A CI GATE — the Tauri shell is a deliberately detached workspace. Run it with
    //! `cargo test --manifest-path app/src-tauri/Cargo.toml`.

    const SOURCE: &str = include_str!("main.rs");

    fn run_setup_body() -> &'static str {
        let start = SOURCE.find(concat!("fn run_", "setup(app: tauri::AppHandle")).unwrap();
        let end = SOURCE[start..].find(concat!("fn ready_the_front_desk_after_a_", "repair(")).unwrap() + start;
        &SOURCE[start..end]
    }

    /// INVARIANT: a completed repair readies the front desk, and it does so only AFTER both
    /// guards it was holding are released.
    ///
    /// Readying a desk is a model turn whose first act is `state.spine.lock()`. Asked for
    /// under this function's own guard it would hold the spine for the install AND the prime,
    /// with his Send arriving in the middle — the same defect `create_thread_in` drops its
    /// guard to avoid, which is why that one has a test of its own directly above.
    #[test]
    fn a_completed_repair_readies_the_desk_after_it_lets_go_of_the_spine() {
        let body = run_setup_body();

        let release = concat!("drop(", "spine)");
        let ready = concat!("ready_the_front_desk_after_a_", "repair(&app");
        let spare = concat!("ready_a_spare_front_", "desk(&app");

        let release_at = body.find(release).expect(
            "run_setup must let go of the spine explicitly — the guard lives to the end of the \
             function otherwise, and the priming turn would start under it",
        );
        let ready_at = body.find(ready).expect(
            "run_setup must ready this thread's front desk after the install — without it the \
             install fixes the cause and nothing primes, which is Ray's candidate-.11 §2.1",
        );
        let spare_at = body.find(spare).expect(
            "run_setup must ready a SPARE when no thread is open — the first-run shape, where \
             the next thing he does is type into a thread that does not exist yet",
        );
        assert!(release_at < ready_at, "the spine must be released BEFORE the desk is readied");
        assert!(release_at < spare_at, "the spine must be released BEFORE the spare is readied");

        // The correction desk's guard too: it is held across the same replacement, and
        // `install_correction_desk` runs on the spine.
        let release_desk = concat!("drop(held_", "desk)");
        let desk_at = body.find(release_desk).expect("run_setup must let go of the writer's guard");
        assert!(desk_at < ready_at, "the writer's guard must be released before the priming turn");
    }

    /// INVARIANT: nothing is readied when the install FAILED, and the thread it readies is the
    /// one that was on screen.
    #[test]
    fn a_failed_install_readies_nothing_and_the_thread_is_the_one_on_screen() {
        let body = run_setup_body();

        let repaired = body
            .find(concat!("if ", "repaired {"))
            .expect("the readying must be conditional on the install having succeeded");
        let ready_at = body.find(concat!("ready_the_front_desk_after_a_", "repair(&app")).unwrap();
        assert!(
            repaired < ready_at,
            "a failed install leaves the machine it found; priming against an engine that is \
             still missing is a spawn that fails again"
        );
        assert!(
            body.contains(concat!("spine.active_", "thread()")),
            "the thread readied must be the one on screen, read while the guard is still held"
        );
    }

    /// INVARIANT: the repair CLEARS the "already asked" memo before asking again.
    ///
    /// **Without this line the fix is inert and looks correct.** `ready_the_front_desk`
    /// returns immediately when `front_desk_primed_for` already names the thread — and after a
    /// failed prime it does name it, because the memo records the ASK and not the outcome. So
    /// the call would be made, the guard would swallow it, and the log would be as silent as
    /// it was on candidate .11.
    #[test]
    fn the_repair_forgets_the_ask_that_failed_before_it_asks_again() {
        let start = SOURCE
            .find(concat!("fn ready_the_front_desk_after_a_", "repair("))
            .expect("the repair's own entry point must exist");
        let body = &SOURCE[start..];
        let end = body.find("\n}\n").unwrap();
        let body = &body[..end];

        let memo = concat!("front_desk_primed_", "for");
        let cleared = body.find(memo).expect("the memo must be cleared by name");
        let asked = body.find(concat!("ready_the_front_", "desk(app")).expect("and then the desk asked for");
        assert!(cleared < asked, "the memo must be cleared BEFORE the ask, or the ask does nothing");
        assert!(body.contains(".clear()"), "the conversation memo is cleared, not rewritten: {body}");
    }
}

#[cfg(test)]
mod lease_gate_tests {
    //! **THE FIRST MESSAGE ON A FRESH INSTALL.** One test, and it reads this file.
    //!
    //! The defect, measured on a published v1.0.0 install on an empty machine on
    //! 2026-09-04: setup ran, Claude Code and the engine were installed, `run_setup`
    //! attached a real lease over a real `claude` child with the right working directory —
    //! and the very first message the customer typed was refused with
    //! [`LEASE_UNAVAILABLE_MESSAGE`]. Quitting and reopening fixed it permanently.
    //!
    //! The whole of it was one word: the gate read `state.lease_ready`, an `AppState` field
    //! written once in `setup`, before there was an engine on the machine to start `claude`
    //! in. Every developer's machine already had both components, so the snapshot was true
    //! at boot and correct for ever, and the defect was structurally unreachable by anyone
    //! who could have found it.
    //!
    //! A cached answer to a question that changes is the shape being kept out, not a
    //! spelling. So this test asserts the SHAPE: no `lease_ready` field, and both gates ask
    //! `has_lease()`. It scrapes this file because the gates take `State<AppState>` and a
    //! `tauri::AppHandle`, neither of which a unit test can build — a source assertion that
    //! runs is worth more than an integration test that does not exist.
    //!
    //! NOT IN A CI GATE. `app-spine-ci` runs `cargo test -p richos-core`; the Tauri shell is
    //! a deliberately detached workspace and nothing runs `cargo test` inside it. Run it with
    //! `cargo test --manifest-path app/src-tauri/Cargo.toml`.


    const SOURCE: &str = include_str!("main.rs");

    /// INVARIANT: the connectivity gate is asked of the live spine, never of a boot-time
    /// snapshot — so a lease attached after first-run setup is usable without a relaunch.
    ///
    /// **The two needles are assembled rather than written**, because `SOURCE` is this file
    /// and a literal needle would match itself. The first version of this test failed for
    /// exactly that reason, which is a small proof that the scrape really does read the
    /// shipping source and not a copy of it.
    #[test]
    fn the_lease_gate_is_never_a_cached_boolean() {
        // The field is gone, and it does not come back under its own name.
        let cached_field = concat!("lease_", "ready: bool");
        assert!(
            !SOURCE.contains(cached_field),
            "AppState must not cache whether a lease exists — it changes at runtime \
             (run_setup attaches one), and the cached copy is what refused a customer's \
             first message on 2026-09-04"
        );
        let factory_gate = concat!("if !spine.has_lease() && !spine.", "has_lease_factory() {");
        assert_eq!(SOURCE.matches(factory_gate).count(), 2,
            "typed and spoken requests must allow a configured factory to reconnect");
        let setup_start = SOURCE.find(concat!("fn run_", "setup(")).unwrap();
        let setup_end = SOURCE[setup_start..].find(concat!("fn entity_", "choice(")).unwrap() + setup_start;
        let setup = &SOURCE[setup_start..setup_end];
        // `take_the_spine(` is named here as well as `spine.lock()`: the one door replaced the
        // other on 2026-09-19, and a needle that still only knew the old spelling would have
        // gone on passing over a `run_setup` that had started BLOCKING on the spine.
        assert!(
            !setup.contains("start_with_onboarding")
                && !setup.contains("spine.lock()")
                && !setup.contains(concat!("take_the_", "spine(")),
            "setup must leave connection to the next tracked cancellable request, and must \
             never take the spine blocking — it uses try_lock so a running turn refuses it");
        assert!(setup.contains("resolve_claude_bin()") && setup.contains("state.claude_bin.lock()"),
            "setup must refresh the factory with the newly installed executable");
        // And the sentence they refuse with is still the one the CEO was written for.
        assert!(crate::LEASE_UNAVAILABLE_MESSAGE.starts_with("RichOS couldn't reach Claude Code"));
    }

    /// INVARIANT: **the first-run arm is never conjoined with `has_lease_factory()`.**
    ///
    /// That conjunction is what made it dead code on every build that has ever shipped.
    /// `set_lease_factory` is called unconditionally at boot — deliberately, so a later sign-in
    /// or a crash recovery has a respawn path — so `has_lease_factory()` is always true, `&&`
    /// short-circuited, and [`setup_view::SETUP_INCOMPLETE_ENGINE`] never once reached a
    /// screen. Ray's candidate .15 typed into a Mac whose engine was stale, watched the turn
    /// start and die, and read *"I lost my connection to the part of me that thinks"* about a
    /// machine that needed one press (`esc-20260919T152225Z-2e44d112`).
    ///
    /// Asserted on the SOURCE for the same reason as the test above it: the gate takes
    /// `State<AppState>`, which a unit test cannot build. The behavior the gate then produces
    /// is asserted for real in `stale_engine_send_gate_tests` below.
    #[test]
    fn the_first_run_arm_is_asked_before_a_turn_starts_and_not_behind_the_factory() {
        let arm = concat!("if !spine.has_lease() && state.boot_", "engine.is_none() {");
        // TWO SITES, AND THE SECOND ONE IS WHY THIS NUMBER IS ASSERTED RATHER THAN >= 1.
        // `send_message` got this arm on 2026-09-19 and `start_voice_capture`'s submit
        // closure did not, so a SPOKEN sentence on a Mac with a stale engine still started a
        // turn that could not finish and came back as a transient interruption. Voice and
        // text are one conversation; a refusal that differs between them is where that
        // promise breaks. One is a regression in the typed path, three is a copy nobody
        // needed.
        assert_eq!(
            SOURCE.matches(arm).count(),
            2,
            "the typed AND spoken paths must each ask the DISK whether the setting up is the \
             reason, on a launch that resolved no engine, BEFORE starting a turn that cannot \
             finish"
        );
        // And each of the two is the FIRST thing its path does with the spine, ahead of the
        // factory gate — the ordering is the whole fix, not the presence of the line.
        for at in SOURCE.match_indices(arm).map(|(at, _)| at) {
            let gate = concat!("if !spine.has_lease() && !spine.", "has_lease_factory() {");
            let after = &SOURCE[at..];
            let next_gate = after.find(gate).expect("a first-run arm with no factory gate after it");
            assert!(
                next_gate < 1200,
                "a first-run arm is more than 1,200 bytes from the factory gate it must \
                 precede — one of the two paths has drifted"
            );
        }
        // The dead shape, by name: the first-run arm inside the factory gate.
        let factory_gate = concat!("if !spine.has_lease() && !spine.", "has_lease_factory() {");
        for at in SOURCE.match_indices(factory_gate).map(|(at, _)| at) {
            let body = &SOURCE[at..(at + 400).min(SOURCE.len())];
            assert!(
                !body.contains(concat!("incomplete_", "message")),
                "the first-run sentence is behind `&& !has_lease_factory()` again, where a \
                 factory that is always configured means it can never be said"
            );
        }
    }
}

/// The boot's line for one missing setup component. **The machine name, never the on-screen
/// one** (dictation plan revision 2, M8). It printed `display_name()`, so renaming "my video
/// tools" on the sheet would have broken the nightly gate (`gui-boot.test.sh` B6a and B9a) and
/// every walk that reads this log. `as_str()` ("claude-code", "engine", "media-tools") is fixed,
/// so on-screen copy never again reaches a harness through this line.
fn setup_missing_line(c: richos_core::setup::Component, places: usize) -> String {
    format!("[richos] first-run setup: {} is NOT installed — {places} place(s) looked:", c.as_str())
}

#[cfg(test)]
mod setup_missing_line_tests {
    use super::setup_missing_line;
    use richos_core::setup::Component;

    /// INVARIANT: the boot names each missing component by `Component::as_str`, which no copy
    /// change can move, and never by the name the sheet shows.
    #[test]
    fn the_boot_line_names_the_component_by_its_machine_name() {
        assert_eq!(
            setup_missing_line(Component::MediaTools, 2),
            "[richos] first-run setup: media-tools is NOT installed — 2 place(s) looked:"
        );
        assert_eq!(
            setup_missing_line(Component::ClaudeCode, 1),
            "[richos] first-run setup: claude-code is NOT installed — 1 place(s) looked:"
        );
        for c in [Component::ClaudeCode, Component::Engine, Component::MediaTools] {
            assert!(!setup_missing_line(c, 1).contains(c.display_name()), "{}", c.display_name());
        }
    }
}

#[cfg(test)]
mod stale_engine_send_gate_tests {
    //! **A MAC WHOSE ENGINE IS STALE IS OFFERED THE REFRESH, NOT A DEAD TURN.**
    //!
    //! Ray, candidate .15 (`v1.2.0-nightly.20260919.4`, source `dcebed09`, build `9375f30d`),
    //! escalation `esc-20260919T152225Z-2e44d112`, 2026-09-19. His scratch HOME carried an
    //! engine an earlier candidate had installed from `3313945b26b7`; this build pins
    //! `adece4c069e4`. The boot log was right about every part of it:
    //!
    //! ```text
    //! [richos] first-run setup: the RichOS engine is NOT installed — 3 place(s) looked:
    //! [richos]   looked in …/RichOS/engine — the engine there carries the right version and
    //!                      different contents — installed from 3313945b26b7, and this build
    //!                      pins adece4c069e4
    //! [richos] first-run setup: this build installs engine 1.2.0.
    //! ```
    //!
    //! And his message started a turn anyway, which then died twice with `turn interrupted
    //! [transient]` and once with `text turn refused before it started`. On screen: *"I lost my
    //! connection to the part of me that thinks, partway through… asking again is worth a
    //! try."* Asking again could not work — nothing about that machine changes by being asked
    //! twice — and the one press that fixes it was never offered.
    //!
    //! This is the DECISION the gate makes, exercised against a real directory on disk: the
    //! detection and the sentence, in one place, the way `send_message` asks them.

    use super::setup_view;
    use richos_core::setup::{self, Component, SetupPaths};
    use std::path::{Path, PathBuf};

    /// The two digests are Ray's, not invented: what was installed, and what .15 pins.
    const STALE_SOURCE: &str = "3313945b26b7000000000000000000000000000000000000000000000000cafe";
    const PINNED_SOURCE: &str = "adece4c069e4000000000000000000000000000000000000000000000000beef";

    /// Removed however this test ends — a panic included (CEO §54: every scratch location is
    /// cleaned up by its maker, or somebody has to go and find it).
    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Scratch {
        let at = std::env::temp_dir().join(format!(
            "richos-stale-engine-gate-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&at);
        std::fs::create_dir_all(&at).unwrap();
        Scratch(at)
    }

    /// The shape `engine_looks_valid` asks for, and the stamp `install_engine` writes — the
    /// writer's own format, so the reader under test is not fed a test's idea of it.
    fn engine_at(dir: &Path, version: &str, installed_from: &str) {
        std::fs::create_dir_all(dir.join("scripts/hooks")).unwrap();
        std::fs::write(dir.join("VERSION"), format!("{version}\n")).unwrap();
        std::fs::write(
            dir.join("INSTALLED-FROM"),
            format!("engine {version}\nsha256 {installed_from}\nbytes 119463136\nfrom https://example.invalid/e.tar.gz\n"),
        )
        .unwrap();
    }

    /// A `claude` that is present, so the only thing missing is the engine and the sentence
    /// under test is the ENGINE one rather than the both-missing one.
    fn a_claude(root: &Path) -> PathBuf {
        let bin = root.join("claude");
        std::fs::write(&bin, "#!/bin/sh\nexit 0\n").unwrap();
        bin
    }

    fn paths(root: &Path) -> SetupPaths {
        SetupPaths {
            home: Some(root.join("home")),
            claude_bin_override: Some(a_claude(root)),
            // No operator statement: an explicit engine is exempt from the pin by design
            // (`find_engine_demanded`), so naming one here would test the wrong door.
            engine_override: None,
            config_dir: Some(root.join("home/.claude")),
            exe: None,
            path_var: None,
        }
    }

    /// Whatever a delivered runtime would say — the release/identity decision is what is under
    /// test, and hashing 322 MB is not something a unit test can or should do.
    fn always_usable(_: &Path) -> Result<(), String> {
        Ok(())
    }

    /// The engine is what these tests are about, so the machine has its video tools: the speech
    /// model answers present, and the yt-dlp half is set present after detection (a fixture
    /// home has no tools folder; `richos-core/tests/setup.rs` installs one for real).
    fn a_speech_model() -> Result<String, String> {
        Ok("large-v3-turbo-q5_0".to_string())
    }

    fn detect_with_video_tools(root: &Path, pin: &setup::EnginePin) -> setup::SetupStatus {
        let mut status = setup::detect_with_pin(&paths(root), &[], &always_usable, Some(pin), &a_speech_model);
        status.media_tools.present = true;
        status
    }

    #[test]
    fn a_stale_engine_is_answered_with_the_offer_and_not_with_a_lost_connection() {
        let root = scratch("stale");
        let home = root.0.join("home");
        let installed = setup::engine_install_dir(&home);
        engine_at(&installed, "1.2.0", STALE_SOURCE);

        let pin = setup::pin_from_parts(
            "1.2.0",
            "https://example.invalid/richos-engine-1.2.0.tar.gz",
            PINNED_SOURCE,
        )
        .unwrap();
        let status = detect_with_video_tools(&root.0, &pin);

        // The directory is THERE and is refused, which is the state Ray was in.
        assert!(installed.is_dir(), "the fixture did not write an engine");
        assert_eq!(status.needs(), vec![Component::Engine], "only the engine is missing");
        assert!(status.engine_installable, "a pinned build can fix this in one press");
        assert!(!status.blocked(), "a pinned build must offer the button, not an explanation");

        // AND THE SENTENCE IS THE ONE THAT NAMES THE PIECE AND REOPENS THE SHEET. This is the
        // value `send_message` returns; `ui/main.js`'s `send()` catch re-reads `setup_status`
        // behind it and puts the offer back on screen, so the promise in it is true.
        assert_eq!(
            setup_view::incomplete_message(&status),
            Some(setup_view::SETUP_INCOMPLETE_ENGINE),
            "a stale engine must be answered with the offer"
        );
        // NEVER the other no-lease sentence. "Quit RichOS and open it again" is written for a
        // machine that has everything and is signed out; on this one it is advice that cannot
        // work, which is the defect `SETUP_INCOMPLETE_*` was written to end (ray-opus-a2).
        assert!(
            setup_view::SETUP_INCOMPLETE_ENGINE.contains("nothing to quit and nothing to reopen"),
            "the arm must close the door on advice that cannot help"
        );
    }

    /// THE POSITIVE CONTROL, without which the test above passes for a gate that refuses
    /// everything: the same machine with the engine this build actually pins says nothing.
    #[test]
    fn an_engine_installed_from_the_pinned_asset_is_never_offered_a_refresh() {
        let root = scratch("current");
        let home = root.0.join("home");
        engine_at(&setup::engine_install_dir(&home), "1.2.0", PINNED_SOURCE);

        let pin = setup::pin_from_parts(
            "1.2.0",
            "https://example.invalid/richos-engine-1.2.0.tar.gz",
            PINNED_SOURCE,
        )
        .unwrap();
        let status = detect_with_video_tools(&root.0, &pin);

        assert!(status.complete(), "nothing is missing: {:?}", status.needs());
        assert_eq!(
            setup_view::incomplete_message(&status),
            None,
            "a healthy Mac must not be interrupted — the no-lease cause is the other one"
        );
    }
}

#[cfg(test)]
mod entity_choice_tests {
    //! WHICH COMPANY THIS COPY OF RICH WORKS FOR — the four-step order, the durable answer,
    //! and the refusal that is still a refusal.
    //!
    //! WHY THESE TESTS COULD NOT HAVE EXISTED BEFORE. `boot_entity` read `std::env::var` and
    //! `std::env::current_dir()` inline, so the condition that actually matters — a Finder
    //! launch, whose working directory is `/` and whose environment carries nothing a shell
    //! ever put there — was not expressible. Nothing tested it, and the defect shipped: an
    //! installed bundle built from f44f89a, launched with `open`, refused the first sentence
    //! typed into it with "no active thread, and no entity was named", and wrote nothing to
    //! the ledger. `resolve_boot_entity` takes all four inputs as arguments for exactly this
    //! reason.
    //!
    //! A unit test still does not close it — see
    //! `docs/verification/entity-choice-2026-09-01/` for the real double-click either side.

    use super::*;
    use richos_core::config::ConfigStore;
    use std::path::Path;

    /// THE REGISTRY THESE TESTS RUN AGAINST — a local fixture, since 2026-09-04.
    ///
    /// It was `EntityRegistry::ceos_companies()`, a `const` table compiled into the shipping
    /// binary. The registry is the person's own file now, so these tests declare theirs, and
    /// the ids stay as they were because the record below is about a measured 2026-09-01
    /// launch that used them.
    fn reg() -> EntityRegistry {
        EntityRegistry::new(vec![
            Entity::new("femcboost", "FemcBoost", &["/fixture/ab/femcboost"]).unwrap(),
            Entity::new("deeply", "Deeply", &["/fixture/ab/deeply"]).unwrap(),
            Entity::new("prospects", "Prospects", &["/fixture/ab/prospects"]).unwrap(),
            Entity::new("richos", "RichOS", &["/fixture/ab/richos", "/fixture/ab/richos-hq"]).unwrap(),
        ])
        .unwrap()
    }

    fn id(s: &str) -> EntityId {
        EntityId::parse(s).unwrap()
    }

    /// A ledger of its own per test, so no two share a file.
    fn spine_for(tag: &str) -> (Spine, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "richos-company-{tag}-{}-{}.jsonl",
            std::process::id(),
            richos_core::util::now_millis()
        ));
        let _ = std::fs::remove_file(&path);
        let ledger = Ledger::open(&path).expect("open ledger");
        let mut spine = Spine::new(ledger);
        // A SPINE ARRIVES WITH NO COMPANIES since 2026-09-04. `Spine::new` used to carry a
        // compiled-in registry; the shell installs the person's own file at boot, and these
        // tests install [`reg`] — the same call, one line earlier.
        spine.set_entity_registry(reg());
        (spine, path)
    }

    // ---- the order -----------------------------------------------------------------------

    #[test]
    fn the_gui_launch_resolves_nothing_and_says_which_root_it_refused() {
        // THE CONDITION THE CEO LAUNCHES IN. `open` hands the process to launchd: working
        // directory `/`, none of a shell's environment, and on a fresh install nothing saved.
        let out = resolve_boot_entity(&reg(), None, None, Some(Path::new("/")));
        assert_eq!(out.entity, None, "`/` owns no entity and must never resolve to one");
        assert_eq!(out.source, None);
        assert!(
            out.notes.iter().any(|n| n.contains("entity not resolved from /")),
            "the refusal must name the root it refused: {:?}",
            out.notes
        );
    }

    #[test]
    fn a_saved_choice_answers_the_launch_the_working_directory_cannot() {
        // THE FIX, in one line: the same launch, after he has answered once.
        let out = resolve_boot_entity(&reg(), None, Some("richos"), Some(Path::new("/")));
        assert_eq!(out.entity, Some(id("richos")));
        assert_eq!(out.source, Some(EntitySource::SavedChoice));
        assert!(out.notes.is_empty(), "a clean resolution has nothing to report: {:?}", out.notes);
    }

    #[test]
    fn the_choice_is_still_in_force_on_the_next_boot_with_the_same_empty_environment() {
        // PERSISTENCE END TO END, and deliberately not two halves that agree in memory: the
        // store is opened, written, DROPPED, and reopened, and the reopened store is what the
        // resolver is handed — under the GUI condition, which is the only one that matters.
        let path = std::env::temp_dir().join(format!(
            "richos-company-boot-{}-{}.json",
            std::process::id(),
            richos_core::util::now_millis()
        ));
        let _ = std::fs::remove_file(&path);
        {
            let mut store = ConfigStore::open(&path).unwrap();
            let first = resolve_boot_entity(&reg(), None, store.entity_raw(), Some(Path::new("/")));
            assert_eq!(first.entity, None, "before he answers, the launch resolves nothing");
            store.set_entity(&id("deeply")).unwrap();
        }
        let reopened = ConfigStore::open(&path).unwrap();
        let second = resolve_boot_entity(&reg(), None, reopened.entity_raw(), Some(Path::new("/")));
        assert_eq!(second.entity, Some(id("deeply")));
        assert_eq!(second.source, Some(EntitySource::SavedChoice));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn an_explicit_environment_value_outranks_a_saved_choice() {
        let out = resolve_boot_entity(&reg(), Some("deeply"), Some("richos"), Some(Path::new("/")));
        assert_eq!(out.entity, Some(id("deeply")));
        assert_eq!(out.source, Some(EntitySource::Environment));
    }

    #[test]
    fn an_unregistered_environment_value_refuses_and_never_falls_through() {
        // Someone who names an entity MEANT it. Quietly resolving a different one — the saved
        // choice, or the working directory — would be worse than stopping, so this refuses
        // even though both of the steps below it would have answered.
        let out = resolve_boot_entity(
            &reg(),
            Some("acme"),
            Some("richos"),
            Some(Path::new("/fixture/ab/femcboost")),
        );
        assert_eq!(out.entity, None);
        assert_eq!(out.source, None);
        assert!(out.notes.iter().any(|n| n.contains("RICHOS_ENTITY")), "{:?}", out.notes);
    }

    #[test]
    fn a_stale_saved_choice_is_named_and_falls_through_to_containment() {
        // A saved value that no longer resolves is STALE DATA, not a statement — so unlike
        // the environment it does not short-circuit. It falls through to the deterministic
        // step, and it is named rather than dropped: the CEO believes he has answered.
        let out = resolve_boot_entity(
            &reg(),
            None,
            Some("acme"),
            Some(Path::new("/fixture/ab/deeply/src")),
        );
        assert_eq!(out.entity, Some(id("deeply")));
        assert_eq!(out.source, Some(EntitySource::WorkingDirectory));
        assert!(
            out.notes.iter().any(|n| n.contains("\"acme\"")),
            "a discarded saved company must be named: {:?}",
            out.notes
        );
    }

    #[test]
    fn a_launch_from_inside_a_registered_folder_still_resolves_by_containment() {
        // STEP 3 IS UNCHANGED, and this is the guard on that claim. A launch from a terminal
        // inside a registered company's folder resolves by containment, exactly as it always
        // has — including from deep inside it, and including a company with two roots.
        for (dir, want) in [
            ("/fixture/ab/femcboost", "femcboost"),
            ("/fixture/ab/richos/app/crates/richos-core", "richos"),
            ("/fixture/ab/richos-hq/loro/records", "richos"),
            ("/fixture/ab/prospects", "prospects"),
        ] {
            let out = resolve_boot_entity(&reg(), None, None, Some(Path::new(dir)));
            assert_eq!(out.entity, Some(id(want)), "{dir} should resolve to {want}");
            assert_eq!(out.source, Some(EntitySource::WorkingDirectory));
        }
    }

    #[test]
    fn a_launch_with_no_working_directory_at_all_still_fails_closed() {
        let out = resolve_boot_entity(&reg(), None, None, None);
        assert_eq!(out.entity, None);
        assert_eq!(out.source, None);
    }

    // ---- the refusal, which is not weakened -----------------------------------------------

    #[test]
    fn nothing_chosen_and_nothing_resolved_still_refuses_every_send() {
        // THE INVARIANT THIS PASS MUST NOT BREAK. The fix is that he can always REACH a
        // company — never that an unresolved one is allowed through. With nothing resolved
        // the spine has no active context, and both the read path and the write path refuse.
        let (mut spine, path) = spine_for("refuse");
        let boot = resolve_boot_entity(&reg(), None, None, Some(Path::new("/")));
        assert!(boot.entity.is_none());

        assert!(spine.active_thread().is_none(), "boot must not conjure a thread out of nowhere");
        assert!(spine.active_binding().is_none());
        let refused = spine.submit_prompt("file this somewhere", Source::Text);
        assert!(refused.is_err(), "a send with no company must be refused, not filed");
        assert!(
            refused.unwrap_err().to_string().contains("no active thread"),
            "the refusal must be the NoActiveThread one, not something that half-worked"
        );
        std::fs::remove_file(&path).ok();
    }

    // ---- answering it ----------------------------------------------------------------------

    #[test]
    fn choosing_a_company_puts_him_in_a_thread_without_a_relaunch() {
        let (mut spine, path) = spine_for("choose");
        assert!(spine.active_thread().is_none());
        apply_company_choice(&mut spine, &id("richos")).unwrap();
        let binding = spine.active_binding().expect("a company was chosen, so a thread is open");
        assert_eq!(binding.entity_id(), &id("richos"));
        // ...and the send that was refused a moment ago now lands in the ledger.
        let thread = spine.active_thread().unwrap().to_string();
        assert!(spine.messages(&thread).is_ok());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn changing_the_company_never_rehomes_or_leaves_an_open_conversation() {
        // ECS §3.2: a thread's home is immutable. Changing the SETTING governs new work; the
        // conversation he is reading does not move and he is not moved out of it.
        let (mut spine, path) = spine_for("rehome");
        let first = spine.create_thread("Acme deal", &id("femcboost")).unwrap();
        spine.switch_thread(&first).unwrap();
        let before = spine.active_binding().unwrap().clone();

        apply_company_choice(&mut spine, &id("deeply")).unwrap();

        let after = spine.active_binding().unwrap();
        assert_eq!(after.thread_id(), first, "the open conversation was switched out from under him");
        assert_eq!(after.entity_id(), &id("femcboost"), "the thread was re-homed — ECS §3.2 forbids it");
        assert_eq!(after.binding_revision(), before.binding_revision(), "the binding was reissued for nothing");
        // And the durable record still says femcboost, not deeply.
        assert_eq!(
            spine.ledger().thread_binding(&first).unwrap().entity_id(),
            &id("femcboost")
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn an_unregistered_company_is_refused_rather_than_invented() {
        let (mut spine, path) = spine_for("unknown");
        let err = apply_company_choice(&mut spine, &id("acme")).unwrap_err();
        assert!(err.contains("unknown entity"), "{err}");
        assert!(spine.active_thread().is_none(), "a refused choice must not open anything");
        std::fs::remove_file(&path).ok();
    }

    // ---- ADDING A COMPANY (2026-09-04) ---------------------------------------------------
    //
    // The pure halves of `register_entity`. The command itself takes a `State<AppState>` that
    // only a running Tauri app can supply; what is testable here is everything it decides
    // BEFORE it touches the app, which is every decision it makes.

    #[test]
    fn an_id_is_derived_from_the_name_he_typed_and_never_asked_of_him() {
        let empty = EntityRegistry::empty();
        for (typed, want) in [
            ("Northwind Traders", "northwind-traders"),
            ("  Harbor Analytics  ", "harbor-analytics"),
            ("Lumen Labs, Inc.", "lumen-labs-inc"),
            ("ACME", "acme"),
            ("3M", "3m"),
        ] {
            assert_eq!(entity_id_from_name(typed, &empty).unwrap().as_str(), want, "{typed:?}");
        }
        // Bounded, because an id reaches the filesystem (`entity.rs`).
        let long = entity_id_from_name(&"a".repeat(200), &empty).unwrap();
        assert!(long.as_str().len() <= ENTITY_ID_MAX_LEN);
        // A name with nothing file-safe in it is REFUSED rather than turned into "-" or "".
        assert!(entity_id_from_name("!!!", &empty).is_err());
        assert!(entity_id_from_name("   ", &empty).is_err());
    }

    #[test]
    fn a_colliding_name_gets_a_distinct_id_rather_than_a_refusal() {
        // Two companies can legitimately produce one slug, and a person who has just typed a
        // name should not have to guess why it was rejected.
        let mut reg = EntityRegistry::empty();
        reg.register(Entity::try_new("harbor-analytics", "Harbor Analytics", vec![]).unwrap()).unwrap();
        let second = entity_id_from_name("Harbor, Analytics", &reg).unwrap();
        assert_eq!(second.as_str(), "harbor-analytics-2");
        assert!(!reg.contains(&second), "and it is genuinely free");

        // The suffix keeps the WHOLE id inside the bound rather than pushing past it.
        let mut long = EntityRegistry::empty();
        let base = "b".repeat(ENTITY_ID_MAX_LEN);
        long.register(Entity::try_new(&base, "Long", vec![]).unwrap()).unwrap();
        let next = entity_id_from_name(&base, &long).unwrap();
        assert!(next.as_str().len() <= ENTITY_ID_MAX_LEN, "{next}");
        assert!(next.as_str().ends_with("-2"), "{next}");
    }

    #[test]
    fn a_folder_that_is_not_there_is_refused_in_words_he_can_act_on() {
        // A typo that names nothing would otherwise register a company that can never be
        // selected by launching from it, and surface weeks later as "Rich keeps asking me
        // which company this is".
        let missing = std::env::temp_dir().join(format!("richos-no-such-{}", std::process::id()));
        let err = resolve_company_folder(&missing.display().to_string()).unwrap_err();
        assert!(err.contains("nothing at that path"), "{err}");
        assert!(err.contains(&missing.display().to_string()), "it names the path: {err}");
        assert!(err.contains("leave it blank"), "and it names the way through: {err}");

        // Relative is refused too: lexical resolution can never match one.
        assert!(resolve_company_folder("Projects/northwind").unwrap_err().contains("full path"));

        // A file is not a folder, and says so as its own reason.
        let file = std::env::temp_dir().join(format!("richos-not-a-dir-{}.txt", std::process::id()));
        std::fs::write(&file, "x").unwrap();
        assert!(resolve_company_folder(&file.display().to_string()).unwrap_err().contains("not a folder"));
        let _ = std::fs::remove_file(&file);

        // And a real directory is accepted, including through `~`, which is the one shape
        // that looks absolute to a person and is not absolute to `Path`.
        let dir = std::env::temp_dir();
        assert_eq!(resolve_company_folder(&dir.display().to_string()).unwrap(), dir);
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            if home.is_dir() {
                assert_eq!(resolve_company_folder("~/").unwrap(), home);
            }
        }
    }

    // ---- the no-orphan migration ----------------------------------------------------------

    #[test]
    fn the_migration_reads_the_ids_the_ledger_already_uses_and_invents_nothing() {
        // THE PROPERTY THAT KEEPS AN EXISTING INSTALL WORKING. A ledger written under the
        // compiled-in table is full of threads bound to ids that stopped being registered
        // when that table left the binary, and an unregistered id fails every scoped read and
        // write — the threads would be on disk and unreachable.
        let (mut spine, path) = spine_for("migration");
        spine.create_thread("Q4 board pack", &id("deeply")).unwrap();
        spine.create_thread("Pricing", &id("femcboost")).unwrap();
        spine.create_thread("More pricing", &id("femcboost")).unwrap();

        let ids = entity_ids_already_in_the_ledger(&spine);
        assert_eq!(ids.len(), 2, "de-duplicated: {ids:?}");
        assert!(ids.contains(&id("deeply")) && ids.contains(&id("femcboost")));

        let migrated = EntityRegistry::from_existing_ids(&ids);
        for want in &ids {
            assert!(migrated.contains(want), "{want} owns threads here and must stay registered");
        }
        // NOTHING IS INVENTED. No root, so no path resolves to a company nobody named.
        assert!(migrated.entities().iter().all(|e| e.roots.is_empty()));
        assert!(migrated.resolve_root(Path::new("/fixture/ab/femcboost")).is_err());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn an_empty_ledger_migrates_nothing_so_a_fresh_user_is_asked_rather_than_seeded() {
        let (spine, path) = spine_for("migration-empty");
        assert!(entity_ids_already_in_the_ledger(&spine).is_empty());
        assert!(EntityRegistry::from_existing_ids(&[]).is_empty());
        std::fs::remove_file(&path).ok();
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::*;

    /// The FIRST TWO LINES OF THE REAL SHIPPING LEDGER on this machine
    /// (`~/Library/Application Support/com.richos.app/conversation-ledger.jsonl`), copied
    /// byte-for-byte on 2026-08-29. They are the two cases the rail has to get right and
    /// the reason this test is written against real bytes instead of a synthetic fixture:
    ///
    ///   line 1 — a `ThreadCreated` with NO `entity_id` at all. Written before entity
    ///            scoping existed, so it replays as `ThreadEntity::Unbound`. This is the
    ///            record that used to break the shell when opened.
    ///   line 2 — a `ThreadCreated` carrying `entity_id: "richos"`, written after slice 1.
    ///
    /// Any future change to the on-disk event shape that would silently reclassify either
    /// of these fails here.
    const REAL_PRE_ENTITY_LEDGER: &str = concat!(
        r#"{"event":"ThreadCreated","thread_id":"thr_5941509111b84531a299acfce7f99948","title":"General","at":1787577795601}"#,
        "\n",
        r#"{"event":"ThreadCreated","thread_id":"thr_dbaacd4320ee482daf414414f590ccba","title":"Running","at":1787975160731,"entity_id":"richos","person_id":"ceo-default","binding_revision":1}"#,
        "\n",
    );

    const UNBOUND_ID: &str = "thr_5941509111b84531a299acfce7f99948";
    const BOUND_ID: &str = "thr_dbaacd4320ee482daf414414f590ccba";

    fn spine_from_real_ledger(tag: &str) -> (Spine, PathBuf) {
        let mut path = std::env::temp_dir();
        path.push(format!("richos-nav-real-{}-{}.jsonl", tag, std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, REAL_PRE_ENTITY_LEDGER).unwrap();
        let ledger = Ledger::open(&path).expect("replay the real ledger");
        let mut spine = Spine::new(ledger);
        // A SPINE ARRIVES WITH NO COMPANIES since 2026-09-04 — the registry is the person's
        // own file and is installed by the shell at boot. These tests install theirs, which
        // is what the shell does, rather than inheriting somebody's compiled-in list.
        spine.set_entity_registry(nav_test_registry());
        (spine, path)
    }

    /// The companies `REAL_PRE_ENTITY_LEDGER` below actually references.
    ///
    /// It is a REAL ledger, captured off a real machine, and the ids inside it are facts
    /// about that capture — the bound thread is bound to `richos` and nothing here can
    /// rename it without making the fixture describe a ledger that does not exist. So the
    /// registry declares those ids. The roots do not appear in the ledger and are
    /// placeholders.
    fn nav_test_registry() -> EntityRegistry {
        EntityRegistry::new(vec![
            Entity::new("femcboost", "FemcBoost", &["/fixture/ab/femcboost"]).unwrap(),
            Entity::new("deeply", "Deeply", &["/fixture/ab/deeply"]).unwrap(),
            Entity::new("richos", "RichOS", &["/fixture/ab/richos"]).unwrap(),
        ])
        .unwrap()
    }

    #[test]
    fn the_real_pre_entity_thread_lands_in_unbound_and_in_no_entity_group() {
        let (spine, path) = spine_from_real_ledger("group");
        let tree = build_navigation_tree(&spine, &nav::NavState::default());

        // Every registered entity gets its own top-level group (§25 Navigation #1), and
        // the groups ARE the registry — read off it rather than retyped, so this asserts the
        // projection rather than a second copy of the roster.
        let ids: Vec<String> = tree.groups.iter().map(|g| g.entity.id.clone()).collect();
        let expected: Vec<String> =
            nav_test_registry().entities().iter().map(|e| e.id.to_string()).collect();
        assert_eq!(ids, expected);
        assert_eq!(ids.len(), 3, "one group per registered company, in registry order");

        // The pre-entity thread is listed, but NOT inside any entity.
        assert_eq!(tree.unbound.len(), 1);
        assert_eq!(tree.unbound[0].id, UNBOUND_ID);
        assert_eq!(tree.unbound[0].display_title, "General");
        assert!(tree.unbound[0].entity_id.is_none());
        for group in &tree.groups {
            assert!(
                !group.threads.iter().any(|t| t.id == UNBOUND_ID),
                "an unbound thread must never be placed in {} — that placement WOULD BE the guess \
                 slice 1 refused to make",
                group.entity.id
            );
        }

        // The bound thread is in exactly one group, and it is its own home entity.
        let homes: Vec<&str> = tree
            .groups
            .iter()
            .filter(|g| g.threads.iter().any(|t| t.id == BOUND_ID))
            .map(|g| g.entity.id.as_str())
            .collect();
        assert_eq!(homes, vec!["richos"], "§25: exactly one immutable entity home per thread");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_unbound_thread_reports_no_turn_state_because_its_turns_are_not_readable() {
        let (spine, path) = spine_from_real_ledger("facts");
        let tree = build_navigation_tree(&spine, &nav::NavState::default());
        let row = &tree.unbound[0];
        // NOT "idle" and NOT "completed" — unknown, because the scoped accessor refuses.
        assert_eq!(row.last_turn_state, None);
        assert!(!row.has_pending_turn);
        assert_eq!(row.message_count, 0);
        // And the read the UI performs when the row is clicked genuinely refuses.
        let err = spine.messages(UNBOUND_ID).unwrap_err().to_string();
        assert!(err.contains("no entity binding"), "{err}");
        assert!(err.contains("An operator must bind it explicitly"), "{err}");
        // The sentence the UI shows is the one the core raises, not a paraphrase.
        assert!(tree.unbound_explanation.contains("predates entity scoping"));
        assert!(tree.unbound_explanation.contains("An operator must bind it explicitly"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pin_rename_and_archive_move_the_row_without_touching_the_entity_home() {
        let (mut spine, path) = spine_from_real_ledger("navstate");
        let mut nav_state = nav::NavState::default();
        nav_state.pinned_threads.push(BOUND_ID.to_string());
        nav_state.archived_threads.push(BOUND_ID.to_string());
        nav_state.renamed_threads.insert(BOUND_ID.to_string(), "Rich's desk".to_string());

        let reader = spine.reader();
        let tree = build_navigation_tree(&*reader.snapshot(), &nav_state);
        let row = tree
            .groups
            .iter()
            .flat_map(|g| &g.threads)
            .find(|t| t.id == BOUND_ID)
            .expect("still in its entity group");

        assert!(row.pinned && row.archived);
        // §25: these "work without changing context authority".
        assert_eq!(row.entity_id.as_deref(), Some("richos"), "archive must not move a thread's home");
        // A rename is a DISPLAY override; the ledger's title is evidence and is untouched.
        assert_eq!(row.title, "Running");
        assert_eq!(row.display_title, "Rich's desk");
        let _ = std::fs::remove_file(&path);
    }

    /// CEO, 2026-10-19: Haiku's title replaces the provisional one in the sidebar by itself,
    /// and "the user is free to manually change that thread title any time they want".
    #[test]
    fn haikus_title_names_the_row_and_his_own_name_is_never_replaced() {
        let (mut spine, path) = spine_from_real_ledger("haiku-title");
        let nav_path = path.with_extension("nav.json");
        if nav_path.exists() {
            std::fs::remove_file(&nav_path).unwrap();
        }
        let nav = Mutex::new(nav::NavStore::open(&nav_path));
        let reader = spine.reader();
        let shown = |nav: &Mutex<nav::NavStore>, id: &str| {
            let tree = build_navigation_tree(&*reader.snapshot(), nav.lock().unwrap().state());
            tree.groups.iter().flat_map(|g| &g.threads).chain(&tree.unbound)
                .find(|t| t.id == id).expect("listed").display_title.clone()
        };
        assert_eq!(shown(&nav, BOUND_ID), "Running", "until Haiku answers, the provisional title stays");
        assert!(show_haiku_title(&nav, BOUND_ID, "Weekly running plan"));
        assert_eq!(shown(&nav, BOUND_ID), "Weekly running plan");

        // He renames it afterwards: his name is shown, and a later answer changes nothing.
        nav.lock().unwrap().rename_thread(BOUND_ID, "My plan").unwrap();
        assert!(!show_haiku_title(&nav, BOUND_ID, "Something else"));
        assert_eq!(shown(&nav, BOUND_ID), "My plan");

        // He named it before Haiku answered: Haiku's title is never kept for it.
        nav.lock().unwrap().rename_thread(UNBOUND_ID, "Old notes").unwrap();
        assert!(!show_haiku_title(&nav, UNBOUND_ID, "General discussion"));
        assert_eq!(shown(&nav, UNBOUND_ID), "Old notes");

        // Durable: the same names after a relaunch.
        let reopened = nav::NavStore::open(&nav_path);
        assert_eq!(reopened.state().display_title(BOUND_ID, "Running"), "My plan");
        assert_eq!(reopened.state().auto_titles.get(BOUND_ID).map(String::as_str), Some("Weekly running plan"));
        std::fs::remove_file(&nav_path).unwrap();
        std::fs::remove_file(&path).unwrap();
    }

    /// **Haiku's title reaches a phone that is already connected, as a `threads` frame** (review
    /// `rv-20261019T135509Z-1d83fd75-da31`: it showed only on the phone's next list load). The
    /// list is read the way `PhoneBridge::threads_changed` reads it (the reader's snapshot, named
    /// as the sidebar names it) and sent the way `PhoneRuntime::threads_changed` sends it.
    #[test]
    fn a_saved_haiku_title_reaches_a_connected_phone_as_a_threads_frame() {
        let (mut spine, path) = spine_from_real_ledger("haiku-title-phone");
        let nav_path = path.with_extension("nav.json");
        if nav_path.exists() {
            std::fs::remove_file(&nav_path).unwrap();
        }
        let nav = Mutex::new(nav::NavStore::open(&nav_path));
        let reader = spine.reader();
        let hub = phone::stream::PhoneHub::new();
        hub.set_live(true);
        let mut connected = hub.subscribe();
        let phone_hears = |rx: &mut tokio::sync::broadcast::Receiver<phone::stream::Frame>| {
            let frame = rx.try_recv().expect("a threads frame on the live stream");
            assert_eq!(frame.kind, "threads");
            let data: serde_json::Value = serde_json::from_str(&frame.data).unwrap();
            data["threads"].as_array().unwrap().iter()
                .find(|t| t["id"] == BOUND_ID).expect("listed")["title"].as_str().unwrap().to_string()
        };
        let tell_the_phone =
            || phone::send_threads(&hub, &phone::bridge::titled(reader.snapshot().threads(), nav.lock().unwrap().state()));

        assert!(show_haiku_title(&nav, BOUND_ID, "Weekly running plan"));
        tell_the_phone();
        assert_eq!(phone_hears(&mut connected), "Weekly running plan", "Haiku's title, not the provisional one");

        // He renames it: the phone hears his name, the same way.
        nav.lock().unwrap().rename_thread(BOUND_ID, "My plan").unwrap();
        tell_the_phone();
        assert_eq!(phone_hears(&mut connected), "My plan");
        std::fs::remove_file(&nav_path).unwrap();
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn search_finds_titles_and_entities_but_never_an_unbound_threads_contents() {
        let (mut spine, path) = spine_from_real_ledger("search");
        // Give the bound thread real content to find.
        let entity = EntityId::parse("richos").unwrap();
        spine.switch_thread(BOUND_ID).unwrap();
        let binding = spine.active_binding().unwrap().clone();
        assert_eq!(binding.entity_id(), &entity);

        let empty = nav::NavState::default();
        // Entity name.
        let hits = run_search(&spine, &empty, "richos", 40);
        assert!(hits.iter().any(|h| h.kind == "entity" && h.entity_id.as_deref() == Some("richos")));
        // The unbound thread's TITLE is findable (navigation metadata, already on screen).
        let hits = run_search(&spine, &empty, "general", 40);
        let unbound_hits: Vec<&SearchHit> = hits.iter().filter(|h| h.thread_id.as_deref() == Some(UNBOUND_ID)).collect();
        assert_eq!(unbound_hits.len(), 1);
        assert_eq!(unbound_hits[0].kind, "thread");
        assert!(unbound_hits[0].excerpt.is_empty(), "a title hit carries no body excerpt");
        assert_eq!(unbound_hits[0].entity_label, "No entity", "never labelled with a guessed entity");
        // No hit anywhere is ever a `message` from the unbound thread.
        for q in ["e", "a", "n"] {
            for h in run_search(&spine, &empty, q, 200) {
                assert!(
                    !(h.kind == "message" && h.thread_id.as_deref() == Some(UNBOUND_ID)),
                    "search must never read an unbound thread's body"
                );
            }
        }
        let _ = std::fs::remove_file(&path);
    }

    /// §25 Accessibility and performance: *"A 10,000-entity seeded index remains searchable
    /// and bounded."*
    ///
    /// THE SECOND HALF OF THE ROW THE TIMELINE'S SCALE WORK OPENED, AND IT LIVES HERE, NOT
    /// IN THE RENDERER. The claim sits in a UX document beside "a 10,000-item thread history
    /// remains smooth", and the two are different components: the thread history is
    /// `app/ui/timeline.js` (pinned by `app/ui/tests/scale.js`), the entity index is
    /// `run_search` above. Neither had a test at either number until 2026-08-30.
    ///
    /// SEEDED, NOT ASSUMED. The registry is the person's own file and starts EMPTY
    /// (2026-09-04), so 10,000 is a scale no install reaches on its own; this test SEEDS it
    /// through `Spine::set_entity_registry`, which is the same call the shell makes at boot.
    /// That is stated plainly rather than left for a reader to infer from a passing test.
    ///
    /// BOUNDED means the RESULT, and that is what is asserted: `hits.truncate(limit)` and
    /// `limit.clamp(1, 200)` cap what crosses the IPC boundary however many entities match.
    /// The intermediate work is NOT bounded — steps 1 and 2 push a hit per matching entity
    /// and per matching thread before the truncate — and this test measures that cost rather
    /// than claiming it away.
    #[test]
    fn a_ten_thousand_entity_index_stays_searchable_and_the_result_stays_bounded() {
        use richos_core::entity::Entity;

        let (mut spine, path) = spine_from_real_ledger("scale");
        let mut entities = Vec::with_capacity(10_000);
        for i in 0..10_000u32 {
            let root = format!("/Users/example/Projects/seeded/client-{i:05}");
            // Every one of them matches the query below, which is the worst case: the
            // truncate is the ONLY thing standing between 10,000 matches and the renderer.
            entities.push(
                Entity::new(
                    &format!("client-{i:05}"),
                    &format!("Client {i:05}"),
                    &[root.as_str()],
                )
                .unwrap(),
            );
        }
        let seeded = EntityRegistry::new(entities).expect("10,000 distinct ids");
        assert_eq!(seeded.entities().len(), 10_000);
        spine.set_entity_registry(seeded);

        let empty = nav::NavState::default();

        let started = std::time::Instant::now();
        let hits = run_search(&spine, &empty, "client", 40);
        let elapsed = started.elapsed();

        // SEARCHABLE: 10,000 matching entities and it still answers.
        assert!(!hits.is_empty(), "a 10,000-entity index must still be searchable");
        assert!(hits.iter().all(|h| h.kind == "entity"));

        // BOUNDED: the cap holds however many matched.
        assert_eq!(hits.len(), 40, "the default limit is the ceiling, not a suggestion");
        assert_eq!(run_search(&spine, &empty, "client", 1).len(), 1);
        assert_eq!(
            run_search(&spine, &empty, "client", 100_000).len(),
            200,
            "`limit.clamp(1, 200)` caps a caller that asks for everything"
        );

        // A one-entity query is still exact at this scale — the cap is not a filter.
        let one = run_search(&spine, &empty, "client-07231", 40);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].entity_label, "Client 07231");

        // The measured number, on demand: `cargo test -- --nocapture`. Captured by default,
        // so a green run stays silent and the figure is still one flag away.
        eprintln!(
            "10,000-entity index: run_search(\"client\", 40) -> {} hits in {:?} (debug build)",
            hits.len(),
            elapsed
        );

        // The number, not an adjective. This runs in debug (`cargo test` builds unoptimized),
        // so the budget is generous on purpose; it exists to catch an order-of-magnitude
        // regression such as a per-hit registry rescan, not to police milliseconds.
        assert!(
            elapsed.as_millis() < 2_000,
            "10,000-entity search took {elapsed:?} — that is not `searchable`"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn search_folding_is_index_aligned_so_excerpts_are_not_sliced_in_the_wrong_place() {
        // U+0130 LATIN CAPITAL LETTER I WITH DOT ABOVE lowercases to TWO chars
        // ("i" + U+0307 COMBINING DOT ABOVE). `str::to_lowercase` would therefore shift
        // every index after it by one and slice the excerpt in the wrong place;
        // `fold_chars` keeps a strict 1:1 char mapping, so the excerpt maths stays correct.
        const SUBJECT: &str = "\u{130}stanbul office MATCH tail";
        let original: Vec<char> = SUBJECT.chars().collect();
        let folded = fold_chars(SUBJECT);
        assert_eq!(folded.len(), original.len(), "folding must not change the character count");

        // Proof the hazard is real and not theoretical on this exact input.
        assert_eq!(SUBJECT.to_lowercase().chars().count(), original.len() + 1);

        let at = find_folded(&folded, &fold_chars("match")).expect("found");
        assert_eq!(original[at..at + 5].iter().collect::<String>(), "MATCH");
        assert!(excerpt_at(&original, at, 5).contains("MATCH"));

        // The naive index would have landed one char early — on "H" through "tail".
        let naive = SUBJECT.to_lowercase().chars().collect::<Vec<char>>();
        let naive_at = find_folded(&naive, &fold_chars("match")).expect("found");
        assert_eq!(naive_at, at + 1, "the two indices genuinely differ");
    }

    #[test]
    fn create_thread_in_refuses_an_unregistered_entity_rather_than_inventing_one() {
        let (mut spine, path) = spine_from_real_ledger("create");
        let err = spine.create_thread("x", &EntityId::parse("not-an-entity").unwrap()).unwrap_err();
        assert!(err.to_string().contains("not-an-entity"), "{err}");
        // And the valid case binds immutably to the chosen entity.
        let id = spine.create_thread("Q4 board pack", &EntityId::parse("deeply").unwrap()).unwrap();
        spine.switch_thread(&id).unwrap();
        assert_eq!(spine.active_entity().unwrap().as_str(), "deeply");
        let tree = build_navigation_tree(&spine, &nav::NavState::default());
        let deeply = tree.groups.iter().find(|g| g.entity.id == "deeply").unwrap();
        assert!(deeply.threads.iter().any(|t| t.id == id));
        assert_eq!(tree.active.as_ref().unwrap().entity_id, "deeply");
        let _ = std::fs::remove_file(&path);
    }
}

// ---------------------------------------------------------------------------------------
// STEERING AND STOP (UX §9.2, §9.3) — Codex-UX slice 6, 2026-08-29.
//
// Neither of these touches `state.spine`. That is the entire point: while a turn runs, the
// spine mutex is held for its whole length, so anything that needs it is not a mid-turn
// control. Both go through `state.control`, the `Arc` the spine shares.
// ---------------------------------------------------------------------------------------

/// What the webview is told when it asks to stop.
///
/// `reachedLease` is reported rather than smoothed over. `false` means the request is
/// durable and the turn WILL be recorded as stopped, but nothing was there to interrupt —
/// so the work may still run to its natural end. The UI says so instead of implying an
/// interrupt that did not happen (§22: status must never claim more than is known).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StopReport {
    /// `false` when nothing was running. Not an error — the honest answer.
    stopped: bool,
    turn_id: Option<String>,
    /// When the stop request became durable. The UI freezes its timer from this, so the
    /// number the CEO ends up reading is anchored to the moment he pressed the button.
    requested_at: Option<u64>,
    reached_lease: bool,
}

/// §9.3: persist a stop request, then interrupt the active turn. In that order, enforced
/// in `steering.rs` rather than here.
#[richos_command::command]
fn stop_turn(state: State<AppState>, expected_turn_id: Option<String>, expected_thread_id: Option<String>) -> Result<StopReport, String> {
    let view=state.reader.snapshot();
    // The conversation the screen named, even when another one is opening (follow-ups after
    // nightly 65, item 9; review rv-20261018T050016Z-78f97c9d-30c4).
    let thread=view.stop_conversation(expected_thread_id.as_deref());
    let control=thread.map(|thread|state.control.for_thread(thread));
    let team_key=state.operator.as_ref().and_then(|_| {
        let binding=view.ledger().thread_binding(thread?).ok()?;
        Some(richos_core::operator_host::ConversationKey {
            entity_id:binding.entity_id().to_string(),thread_id:binding.thread_id().to_string(),
        })
    });
    let outcome=match (control,expected_turn_id.as_deref()) {
        (Some(control),Some(expected))=>control.request_stop_for(expected),
        (Some(control),None)=>control.request_stop(),
        (None,_)=>Ok(StopOutcome::NothingRunning),
    }.map_err(|e|e.to_string())?;
    // **HIS ESC, on an operator install, is his team's too** (r3 (d) item 6): the lead's turn in
    // that conversation ends and its agents keep running (r4 §1.2). AFTER the front desk's stop
    // and on its own thread, so a lead slow to answer the interrupt never delays the stop he
    // sees; what his team did is said on the conversation by the desk.
    if let (Some(desk), Some(key)) = (state.operator.clone(), team_key) {
        let spawned = std::thread::Builder::new().name("richos-operator-esc".into()).spawn(move || {
            let said = desk.interrupt(&key);
            eprintln!("[richos] his team: Esc in {}: {said}", key.thread_id);
        });
        if let Err(e) = spawned {
            eprintln!("[richos] his team: the Esc could not be sent to your team ({e})");
        }
    }
    match outcome {
        StopOutcome::NothingRunning => {
            Ok(StopReport { stopped: false, turn_id: None, requested_at: None, reached_lease: false })
        }
        StopOutcome::Requested { turn_id, requested_at, reached_lease } => Ok(StopReport {
            stopped: true,
            turn_id: Some(turn_id),
            requested_at: Some(requested_at),
            reached_lease,
        }),
    }
}

/// What the webview gets back when it steers.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SteerReport {
    /// The intake-log id. Not a turn id — there is no turn yet, and inventing one here
    /// would be a claim that the message had been accepted as work.
    intake_id: u64,
    thread_id: String,
    /// The turn that was running when he typed it — the evidence behind the
    /// "Added while Rich was working" cue, rather than a guess made in the renderer.
    steering_turn_id: String,
    at: u64,
}

/// §9.2: the CEO added words while Rich was working.
///
/// Durable on return, delivered at the next turn boundary. It does NOT join the running
/// turn — one turn at a time, and the continuity design's turn-boundary
/// controller is queue-not-interrupt by construction (§3.1) — and the UI says which of
/// those two things happened rather than letting the CEO assume the other.
#[richos_command::command]
fn steer_message(state: State<AppState>, text: String) -> Result<SteerReport, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("nothing to add".into());
    }
    let view=state.reader.snapshot();
    let thread=view.active_thread().ok_or_else(||richos_core::steering::SteeringError::NoActiveTurn.to_string())?;
    match state.control.for_thread(thread).steer(trimmed).map_err(|e| e.to_string())? {
        IntakeRecord::Steer { id, thread_id, steering_turn_id, at, .. } => {
            Ok(SteerReport { intake_id: id, thread_id, steering_turn_id, at })
        }
        other => Err(format!("unexpected intake record: {other:?}")),
    }
}

/// The turn that is running right now, as the CONTROL sees it.
///
/// A mirror of the spine's `turn_in_progress`, written at the same durable points, and
/// readable without the spine lock — so the composer can arm its stop control after a
/// reload without waiting for a turn to finish first. It is not inferred from event
/// silence or from a timer (continuity §5.2).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RunningTurn {
    turn_id: String,
    thread_id: String,
    entity_id: Option<String>,
    started_at: Option<u64>,
}

#[richos_command::command]
fn running_turn(state: State<AppState>) -> Option<RunningTurn> {
    let view=state.reader.snapshot();
    state.control.for_thread(view.active_thread()?).active_turn().map(|a| RunningTurn {
        turn_id: a.turn_id,
        thread_id: a.thread_id,
        entity_id: a.entity_id.map(|e| e.as_str().to_string()),
        started_at: a.started_at,
    })
}

// ---------------------------------------------------------------------------------------
// LORO — reading what the system believes, and correcting it (open-items 3.5)
//
// This is the surface that makes the writer reachable from the APP rather than only from a
// terminal. loro-writer.md named the gap: "Rich does not call the writer yet... the
// writer's reachable surface is the CLI."
//
// EVERY command here goes through `CorrectionDesk`, which cannot write without a prior
// proposal the CEO has confirmed (ceo-decisions.md §7, ask never infer). There is no
// command that writes loro directly, and adding one would be the bug.
//
// They take their own mutex, never the spine's: `send_message` holds the spine lock for a
// whole turn, and a correction panel that froze until Rich finished would not be used.
// ---------------------------------------------------------------------------------------

/// Whether this install can read and correct company memory at all — so the UI can show a
/// real state instead of a dead button. Never a guess: it reflects the configured corpus.
#[richos_command::command]
fn loro_available(state: State<AppState>) -> bool {
    state.correction.lock().unwrap().is_some()
}

/// The desk, or the sentence explaining why there is not one.
///
/// **The refusal NAMES WHAT WAS LOOKED FOR**, which it did not until 2026-09-01, because
/// until then it was almost never reached honestly: the writer was built from environment
/// variables a Finder launch does not have, so on the installed app this branch fired on
/// every call and said only that no corpus was "configured" — a word that is false when the
/// corpus is sitting at a path the app simply never looked at. A confirm that quietly writes
/// nothing is worse than a refusal, and a refusal that cannot say where it looked is barely
/// better than the silence.
///
/// The candidate list comes from `MemoryStatus`, which is the same resolution the boot line
/// printed — not a second guess about it.
/// It returns the `Arc` and not a guard, and the caller locks. That is forced by the field
/// having become swappable: a guard borrowed out of `state.correction` would hold the OUTER
/// lock for the whole of a `confirm`, and a confirm runs `loro-write` as a child process.
/// `loro_available` would then block behind it, and so would `provision_memory`. Cloning the
/// `Arc` and dropping the outer guard on the way out costs one refcount and keeps the two
/// locks strictly nested rather than held as a pair.
fn desk(state: &State<AppState>) -> Result<SharedCorrectionDesk, String> {
    let held = state.correction.lock().unwrap().clone();
    held.ok_or_else(|| {
        let status = state.memory.lock().unwrap().clone();
        let mut msg=String::from(LORO_DESK_ABSENT_MESSAGE);
        if !status.tried.is_empty() {
            msg.push_str("\n\nLooked for it in:");
            for t in &status.tried {
                msg.push_str("\n  • ");
                msg.push_str(t);
            }
        }
        msg
    })
}

/// "What does loro actually believe?" — the answer is a file. Read-only; no proposal, no
/// confirmation, because reading is not correcting.
#[richos_command::command]
fn loro_show_record(state: State<AppState>, record_ref: String) -> Result<WriteOutput, String> {
    desk(&state)?.lock().unwrap().show(&record_ref).map_err(|e| e.to_string())
}

/// The company a correction belongs to: the one the conversation on screen is bound to.
///
/// The spine files a belief correction under the ACTIVE thread's immutable binding
/// (`Spine::stage_belief_correction`), and the app opens conversations from several companies,
/// so the launch selection (`state.entity`) can name a different company than the one the
/// CEO is looking at. Reading the launch selection then hid B's corrections behind A's queue
/// and sent A to `confirm`, which refused B's proposal as `WrongEntity` (hunt part 1, finding 09).
/// The reader snapshot is lock-free, so this never waits for a turn. With no active
/// conversation the launch selection is the scope, as before; with neither, refuse.
fn correction_entity(view: &dyn SpineView, launch: Option<EntityId>) -> Result<EntityId, String> {
    view.active_binding()
        .map(|b| b.entity_id().clone())
        .or(launch)
        .ok_or_else(|| ENTITY_UNRESOLVED_MESSAGE.to_string())
}

fn correction_entity_of(state: &State<AppState>) -> Result<EntityId, String> {
    let launch = state.entity.lock().unwrap().clone();
    correction_entity(&*state.reader.snapshot(), launch)
}

/// Corrections waiting on the CEO, for the company of the conversation now open (its launch
/// selection when none is). Scoped, not global: a proposal about one entity's memory has no
/// business in another's queue.
#[richos_command::command]
fn loro_pending_corrections(state: State<AppState>) -> Result<Vec<Proposal>, String> {
    let entity = correction_entity_of(&state)?;
    Ok(desk(&state)?.lock().unwrap().pending_for(entity.as_str()).into_iter().cloned().collect())
}

/// Stage a change and get back exactly what it WOULD write. **Nothing is written here.**
/// `why` is the CEO's own words for what is wrong, and an empty one is refused before a
/// process is started — a correction with no stated reason is the shape an inferred one
/// takes.
#[richos_command::command]
fn loro_propose_correction(
    state: State<AppState>,
    thread_id: Option<String>,
    write: ProposedWrite,
    why: String,
) -> Result<Proposal, String> {
    let entity = correction_entity_of(&state)?;
    // The thread id is PROVENANCE and comes from the caller. It is deliberately not read
    // off the spine: `send_message` holds that lock for the whole of a turn, so asking the
    // spine which thread is active would freeze a correction panel until Rich finished —
    // the same reason the stop control lives outside the lock (UX §9.3).
    let thread_id = thread_id.unwrap_or_default();
    desk(&state)?.lock().unwrap().propose(entity.as_str(), &thread_id, write, &why).map_err(|e| e.to_string())
}

/// The CEO says yes. The ONLY path in this application to a loro write.
///
/// The write is also recorded in the conversation ledger as a CEO-FACING action, because
/// "Rich changed what the company believes" is exactly the class of fact the action-ledger
/// digest exists to stop a successor denying from absent memory (`reprime.rs`, §2.1 #6).
/// The ledger write is best-effort and never fails the correction: the desk's own log is
/// already durable, and losing a ledger line must not un-write a record that landed.
#[richos_command::command]
fn loro_confirm_correction(state: State<AppState>, id: String) -> Result<Proposal, String> {
    let entity = correction_entity_of(&state)?;
    let done = desk(&state)?.lock().unwrap().confirm(entity.as_str(), &id).map_err(|e| e.to_string())?;
    if let Some(outcome) = &done.outcome {
        let detail = match &outcome.superseded_ref {
            Some(old) => format!("superseded {old} with {}", outcome.r#ref),
            None => format!("wrote {}", outcome.r#ref),
        };
        let _ = state
            .spine
            .lock()
            .unwrap()
            .record_ceo_action("loro_correction", &format!("{detail} — \"{}\"", done.why));
    }
    Ok(done)
}

/// He says no. `permanent` is his explicit "don't ask about this record again" — a plain
/// decline is NOT permanent, because a decline is ambiguous while a repeat is evidence
/// (ceo-decisions.md §7).
#[richos_command::command]
fn loro_decline_correction(state: State<AppState>, id: String, permanent: bool) -> Result<(), String> {
    desk(&state)?.lock().unwrap().decline(&id, permanent).map_err(|e| e.to_string())
}

/// The suppression list, inspectable — §7 requires it, "or a term silently refuses to
/// learn with no way to see why".
#[richos_command::command]
fn loro_suppressed_records(state: State<AppState>) -> Result<Vec<String>, String> {
    Ok(desk(&state)?.lock().unwrap().suppressed().to_vec())
}

/// ...and liftable. A list you can see and cannot clear is only half of inspectable.
#[richos_command::command]
fn loro_unsuppress_record(state: State<AppState>, record_ref: String) -> Result<(), String> {
    desk(&state)?.lock().unwrap().unsuppress(&record_ref).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------
// SPOKEN CORRECTIONS (2026-08-30) — the flywheel's automatic trigger, answered
// ---------------------------------------------------------------------------------------
//
// Nothing here DETECTS anything and nothing here decides anything. The trigger runs inside
// `Spine::submit_prompt` on every utterance the CEO speaks or types, and it only ever
// STAGES — ceo-decisions.md §7: "Nothing is ever learned silently." These commands are the
// other half: the surface through which he gives the answer §7 requires, and the only route
// by which a term reaches the vocabulary.
//
// Every one of them goes through `state.spoken`, which is the same `Arc` the spine holds
// and is NOT behind the spine mutex — so a question raised during a two-hour turn can be
// answered during that turn rather than after it.

/// The desk, or the sentence to show the CEO when there isn't one.
fn spoken_desk<'a>(
    state: &'a State<'a, AppState>,
) -> Result<std::sync::MutexGuard<'a, CandidateDesk>, String> {
    let desk = state.spoken.as_ref().ok_or(
        "I can't record corrections right now: my correction log could not be opened. \
         Nothing you say is being lost from the conversation itself.",
    )?;
    desk.lock().map_err(|_| "the correction desk is busy — try that again".to_string())
}

/// Is the trigger live at all? A UI that cannot tell "nothing to ask" from "the desk is
/// broken" would render a permanently empty HUD and call it calm.
#[richos_command::command]
fn spoken_corrections_available(state: State<AppState>) -> bool {
    state.spoken.is_some()
}

/// Everything awaiting §7's one-keystroke answer. Each carries the CEO's own sentence, the
/// pair, the prompt to show, and — when the wrong form was found on the record — the line
/// it appeared on, so the ask can quote him rather than assert at him.
#[richos_command::command]
fn spoken_pending_corrections(state: State<AppState>) -> Result<Vec<Candidate>, String> {
    Ok(spoken_desk(&state)?.pending().to_vec())
}

/// **He says yes.** The ONLY path from a staged correction to the vocabulary.
///
/// A confirm with no local service configured returns the writer's honest refusal rather
/// than a cheerful nothing — reporting "learned" when nothing wrote is the one outcome that
/// would make him stop correcting the term.
///
/// The ledger write is best-effort and never fails the answer, the same posture
/// `loro_confirm_correction` takes: what the CEO decided is already durable on the desk's
/// own log, and the action-ledger row is a record OF that decision, not the decision.
#[richos_command::command]
fn spoken_confirm_correction(state: State<AppState>, key: String) -> Result<LearnOutcome, String> {
    let (outcome, canonical, mangled) = {
        let mut desk = spoken_desk(&state)?;
        let pair = desk
            .pending()
            .iter()
            .find(|c| c.key == key)
            .map(|c| (c.ask.to.clone(), c.ask.from.clone()))
            .ok_or_else(|| format!("no correction {key} is awaiting an answer"))?;
        let outcome = desk.confirm(&key).map_err(|e| e.to_string())?;
        (outcome, pair.0, pair.1)
    };
    let _ = take_the_spine(&state.spine).record_ceo_action(
        "vocabulary_learn",
        &format!("learned \"{canonical}\" (heard as \"{mangled}\")"),
    );
    Ok(outcome)
}

/// He says no. `permanent` is his explicit "don't ask for this term again". A plain decline
/// is NOT permanent and the pair is asked again on its very next repeat — §7: repetition is
/// the evidence, and waiting dilutes it.
#[richos_command::command]
fn spoken_decline_correction(
    state: State<AppState>,
    key: String,
    permanent: bool,
) -> Result<(), String> {
    spoken_desk(&state)?.decline(&key, permanent).map_err(|e| e.to_string())
}

/// The suppression list, inspectable — §7 requires it, "or a term silently refuses to learn
/// with no way to see why".
#[richos_command::command]
fn spoken_suppressed_terms(state: State<AppState>) -> Result<Vec<String>, String> {
    Ok(spoken_desk(&state)?.suppressed().to_vec())
}

/// ...and liftable. A list you can see and cannot clear is only half of inspectable.
#[richos_command::command]
fn spoken_unsuppress_term(state: State<AppState>, key: String) -> Result<(), String> {
    spoken_desk(&state)?.unsuppress(&key).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------
// THE FEEDBACK CHANNEL — the local half, made reachable (RICH-TODOs row 5)
// ---------------------------------------------------------------------------------------
//
// `feedback.rs` landed complete on 2026-08-30: the CEO's wording in constants, the four
// keys, the versioned taxonomy, the local store, the disclosure, and four tests in
// `tests/feedback_no_outbound_tests.rs` asserting there is no way off this machine. And
// `grep -n feedback src-tauri/src/main.rs` returned nothing, which is row 5b's defect in a
// second costume: a capability the CEO can only reach by writing Rust is not a property of
// the product he uses.
//
// **NOTHING HERE SENDS ANYTHING, AND NOTHING HERE MAY.** These six commands are a window
// onto one local file. There is no transport, no endpoint, no address, no background job
// and no queue — and `feedback_no_outbound_tests.rs` now reads THESE FUNCTIONS' bodies as
// well as the module's, so a later "just wire it up" commit fails a test rather than a
// review.
//
// THIS LAYER AUTHORS NO WORDING. Every sentence the CEO reads on this surface is a
// constant or a term from `feedback.rs` — `PROMPT_QUESTION`, `PROMPT_OPTIONS`,
// `REPORT_OFFER`, `DISCLOSURE_HEADING`, and the `label()`/`sentence()` of every term. The
// module puts them in one place precisely so a UI cannot paraphrase them, so this file
// projects them and adds none of its own. The one exception is the sentence below, which
// is about THIS INSTALL and has nowhere else to live.
//
// They take their own mutex, never the spine's — a question about a session is asked while
// the session is still running.

/// What the CEO is told when the one file cannot be opened.
///
/// The register the other two desks use: it says what it will not do, it names who owns the
/// fix, and it invents no control, because there genuinely is none in the app. Asking him
/// what he thinks and then dropping the answer would be worse than not asking.
const FEEDBACK_STORE_UNAVAILABLE: &str =
    "I can't keep an answer right now: the file I record them in wouldn't open and I'm \
     not going to ask you what you think and then lose it. That one is for whoever set \
     RichOS up to look at; it isn't yours to fix.";

/// What he is told if a key that is not one of the four ever reaches the store.
///
/// `PromptOutcome::from_key` returns `None` for anything else, and this is that `None` said
/// out loud: an unrecognized key is not a dismissal, it is not an answer at all, and
/// recording it as one would put invented data in the store.
const FEEDBACK_KEY_NOT_ONE_OF_FOUR: &str =
    "That isn't one of the four answers, so I haven't written anything down.";

/// What he is told if the text he approved is not the text this build would report.
///
/// The CEO's own rule — *he sees exactly what his RichOS would say, before any of it could
/// ever travel* — is a STRUCTURAL property in `feedback.rs` (`ApprovedReport` has no public
/// constructor; the only way to one is `Disclosure::approve`, and a `Disclosure` cannot
/// exist without having rendered its text). That property is enforced inside one process.
/// This command sits on the other side of an IPC boundary, so it re-renders and compares:
/// an approval whose text does not match what this build would say is refused rather than
/// recorded, and the mismatch is the one case where consent could be recorded for something
/// he was never shown.
const FEEDBACK_PREVIEW_MISMATCH: &str =
    "I won't record that. What you were shown isn't what I would say now, so approving it \
     would be approving something you haven't read. Ask me to show it again.";

/// The store, or the sentence to show the CEO when there isn't one.
fn feedback_store<'a>(
    state: &'a State<AppState>,
) -> Result<std::sync::MutexGuard<'a, FeedbackStore>, String> {
    let store = state.feedback.as_ref().ok_or(FEEDBACK_STORE_UNAVAILABLE)?;
    store.lock().map_err(|_| FEEDBACK_STORE_UNAVAILABLE.to_string())
}

/// Can an answer be kept at all? A surface that cannot tell "nothing recorded yet" from
/// "the file would not open" would render an empty history over a broken store and call it
/// calm — the same distinction both correction desks draw.
#[richos_command::command]
fn feedback_available(state: State<AppState>) -> bool {
    state.feedback.is_some()
}

/// THE PROMPT, THE KEYS AND THE OFFER, verbatim from `feedback.rs`.
///
/// Projected rather than re-typed: the module holds the CEO's wording in constants
/// specifically so the UI cannot paraphrase it, and `invitesReport` comes from
/// `Rating::invites_report` so the "only 1 and 2" rule is not re-derived on the other side
/// of the bridge.
#[richos_command::command]
fn feedback_wording() -> serde_json::Value {
    // Every rating there is. Not a list this file keeps: `Rating` has exactly three
    // variants and `0` is deliberately not one of them, so a fourth would be a compile
    // error here rather than a silently missing button.
    let ratings: Vec<serde_json::Value> = [Rating::Bad, Rating::OkButCouldBeBetter, Rating::Good]
        .iter()
        .map(|r| {
            serde_json::json!({
                "key": r.key().to_string(),
                "label": r.label(),
                // The value serde ACTUALLY writes for this rating, so a surface reading an
                // entry back off disk can match it without transforming the label into a
                // wire name and hoping the two agree.
                "wire": serde_json::to_value(r).unwrap_or(serde_json::Value::Null),
                "invitesReport": r.invites_report(),
            })
        })
        .collect();
    serde_json::json!({
        "question": PROMPT_QUESTION,
        "options": PROMPT_OPTIONS,
        "reportOffer": REPORT_OFFER,
        "disclosureHeading": DISCLOSURE_HEADING,
        "taxonomyVersion": TAXONOMY_VERSION.wire(),
        "ratings": ratings,
        // `0` is deliberately not a `Rating` — dismissing is the absence of a rating, not a
        // fourth value of one — so it is carried separately and its label is read out of
        // the options line the module wrote, never typed here.
        "dismiss": { "key": "0", "label": "Dismiss" },
    })
}

/// THE WHOLE VOCABULARY a report can be assembled from, iterated from each term's own
/// `ALL`. Nothing here is a list this file maintains: adding a term in `feedback.rs` puts
/// it on screen, and removing one takes it off.
#[richos_command::command]
fn feedback_taxonomy() -> serde_json::Value {
    let failure_class: Vec<serde_json::Value> = FailureClass::ALL
        .iter()
        .map(|t| serde_json::json!({ "wire": t.wire(), "label": t.label() }))
        .collect();
    let occurrences: Vec<serde_json::Value> = Occurrences::ALL
        .iter()
        .map(|t| serde_json::json!({ "wire": t.wire(), "label": t.label() }))
        .collect();
    let diagnosis: Vec<serde_json::Value> = DiagnosisTerm::ALL
        .iter()
        .map(|t| serde_json::json!({ "wire": t.wire(), "sentence": t.sentence() }))
        .collect();
    let conditions: Vec<serde_json::Value> = ContributingCondition::ALL
        .iter()
        .map(|t| serde_json::json!({ "wire": t.wire(), "sentence": t.sentence() }))
        .collect();
    serde_json::json!({
        "version": TAXONOMY_VERSION.wire(),
        "failureClass": failure_class,
        "occurrences": occurrences,
        "diagnosis": diagnosis,
        "conditions": conditions,
    })
}

/// What the CEO chose, in the payload's OWN field names.
///
/// `deny_unknown_fields` for the reason `FeedbackPayload` carries it: prose arrives beside a
/// payload as often as inside it, and serde's default is to ignore what it does not know.
/// A `notes` field posted from the webview would otherwise be silently dropped here and
/// then, one careless commit later, silently kept.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    failure_class: FailureClass,
    occurrences_this_session: Occurrences,
    generic_diagnosis: Vec<DiagnosisTerm>,
    #[serde(default)]
    contributing_condition: Vec<ContributingCondition>,
}

/// What he answered when offered the chance to report.
///
/// There is no `Pending` and no `Later`, mirroring `ReportDecision`: an answer is an answer,
/// and a half-state here is the seam a future outbox would grow from.
#[derive(serde::Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case", deny_unknown_fields)]
enum ReportChoice {
    /// `3`, or a dismissal — the offer was never made.
    NotOffered,
    /// The offer was made and he said no. The payload is dropped and nothing about it is
    /// recorded, because a declined report is not a report.
    Declined,
    /// He read the rendered report and said yes. `shown` is the exact block he read, and it
    /// is checked against what this build renders before any consent is recorded.
    Approved { selection: Selection, shown: String },
}

/// Assemble a disclosure from a rating and a selection. The ONLY route to a payload in this
/// file, so the preview command and the record command cannot render two different things.
fn disclosure_for(rating: Rating, sel: &Selection) -> Result<Disclosure, String> {
    let payload = FeedbackPayload::assemble(
        rating,
        sel.failure_class,
        sel.occurrences_this_session,
        sel.generic_diagnosis.clone(),
        sel.contributing_condition.clone(),
    )
    .map_err(|e| e.to_string())?;
    Ok(Disclosure::of(payload))
}

fn rating_from_key(key: &str) -> Result<Rating, String> {
    let outcome = outcome_from_key(key)?;
    outcome.rating().ok_or_else(|| FEEDBACK_KEY_NOT_ONE_OF_FOUR.to_string())
}

fn outcome_from_key(key: &str) -> Result<PromptOutcome, String> {
    let mut chars = key.chars();
    let (first, rest) = (chars.next(), chars.next());
    match (first, rest) {
        (Some(c), None) => {
            PromptOutcome::from_key(c).ok_or_else(|| FEEDBACK_KEY_NOT_ONE_OF_FOUR.to_string())
        }
        _ => Err(FEEDBACK_KEY_NOT_ONE_OF_FOUR.to_string()),
    }
}

/// **THE PREVIEW.** Exactly what would be said, rendered from the same structure that gets
/// recorded — never a description of it.
///
/// Nothing is stored by this command and nothing is consented to by calling it. It exists
/// so the CEO reads the report BEFORE he is asked to approve it, which is the half of his
/// design that the type system cannot carry across an IPC boundary on its own.
#[richos_command::command]
fn feedback_preview(key: String, selection: Selection) -> Result<serde_json::Value, String> {
    let rating = rating_from_key(&key)?;
    let disclosure = disclosure_for(rating, &selection)?;
    Ok(serde_json::json!({
        // The heading and the report separately, so a surface can lay them out, AND the two
        // together in one block — which is the string an approval is checked against, so
        // there is no arrangement of the parts the UI could get wrong.
        "heading": DISCLOSURE_HEADING,
        "text": disclosure.text(),
        "full": disclosure.full_text(),
    }))
}

/// **THE ANSWER, KEPT LOCALLY.** One line appended to one file, and that is the entire
/// effect of this command.
///
/// The report half re-renders from the selection and compares against the block the CEO was
/// shown. `Disclosure::approve` is then the only route to the `ApprovedReport` that goes in,
/// and `FeedbackEntry::with_report` refuses a report attached to a rating that never invited
/// one, or one that disagrees with the rating he gave. Every one of those rules lives in
/// `feedback.rs`; none of them is re-implemented here.
#[richos_command::command]
fn feedback_record(
    state: State<AppState>,
    key: String,
    report: ReportChoice,
) -> Result<FeedbackEntry, String> {
    let outcome = outcome_from_key(&key)?;
    let decision = match &report {
        ReportChoice::NotOffered => ReportDecision::NotOffered,
        ReportChoice::Declined => ReportDecision::Declined,
        ReportChoice::Approved { selection, shown } => {
            let rating = outcome.rating().ok_or_else(|| FEEDBACK_KEY_NOT_ONE_OF_FOUR.to_string())?;
            let disclosure = disclosure_for(rating, selection)?;
            if disclosure.full_text() != *shown {
                return Err(FEEDBACK_PREVIEW_MISMATCH.to_string());
            }
            disclosure.approve()
        }
    };
    let entry = FeedbackEntry::new(outcome).with_report(decision).map_err(|e| e.to_string())?;
    let store = feedback_store(&state)?;
    store.record(&entry).map_err(|e| e.to_string())?;
    Ok(entry)
}

/// What is on this machine, oldest first.
///
/// Each approved entry carries `shown` — the text he read when he approved it, re-rendered
/// from the stored payload. `render_disclosure` is deterministic and total, so no second
/// free-text copy of what he saw has to be kept in the record; keeping one would have put an
/// unvalidated `String` into the durable file, which is the exact channel this design closes.
#[richos_command::command]
fn feedback_history(state: State<AppState>) -> Result<Vec<serde_json::Value>, String> {
    let entries = feedback_store(&state)?.entries().map_err(|e| e.to_string())?;
    Ok(entries
        .into_iter()
        .map(|e| {
            let shown = match &e.report {
                ReportDecision::Approved(a) => Some(a.as_shown()),
                _ => None,
            };
            serde_json::json!({ "entry": e, "shown": shown })
        })
        .collect())
}

// ---------------------------------------------------------------------------------------
// TECHY MODE, Phase 2 (techy-mode design §3.1/§3.4) — the renderer's five commands.
//
// Appended at the END for the reason the block above says: a parallel branch appending its
// own glue merges without touching a line this one also touched.
//
// PHASE 1 ROUTED AND RETAINED AND HAD NO CALLER. `Spine::machinery_journal()` and
// `rich://machinery` shipped 2026-08-28 complete and tested, and `grep -rn machinery
// app/ui/` returned three comments. These five commands are the door.
//
// WHAT IS DELIBERATELY NOT HERE: any control. No interrupt, no approve/deny, no re-run.
// Techy mode is a window, not a cockpit (§5, §9) — R2 business-action governance is
// deferred to V2 by CEO decision for v1 and all 1.x, and a button here is how that gets
// un-deferred by accident.
// ---------------------------------------------------------------------------------------

/// One thread's machinery, for the technical view (§3.4).
///
/// See `machinery_view.rs` for why this is a second command rather than a `mode` argument
/// on `get_timeline` — the calm command stays byte-for-byte what it was, and its gate stays
/// structural rather than becoming a branch.
///
/// **The toggle is NOT read here.** §3.2's rule is that routing and retention run always
/// and the toggle controls RENDERING only; making the read conditional on the flag would
/// put policy in two places, and the second one would eventually disagree. The renderer
/// decides whether to ask.
#[richos_command::command]
fn get_machinery(state: State<AppState>, thread_id: String) -> Result<serde_json::Value, String> {
    if let Ok(mut spine) = state.spine.try_lock() {
        spine.pump_between_turn();
    }
    machinery_payload(&*state.reader.snapshot(), &thread_id)
}

/// §2.4's raw pane, one record at a time.
///
/// Three answers, and the middle one is the point: `retained` with the payload,
/// `not_retained` when the Tier-B window has passed over it (**the record still renders —
/// structure, title, status, paths, summary — and the pane says so; an honest degrade,
/// never a silent blank**), and `unreadable` when the store refused.
///
/// `truncated: true` means §2.4's 32 KB per-record cap fired and the payload is a
/// char-boundary-safe prefix as a JSON *string* rather than the object — a visibly
/// different shape, so nothing can mistake it for the whole thing.
///
/// **§7.2 is not answered here.** How long raw payloads survive is the CEO's open question;
/// nothing in this path consults the window. Whatever he chooses — 14 days, 2 GB, or
/// forever — this returns what is on disk and the two states stay the same two states.
#[richos_command::command]
fn get_machinery_raw(
    state: State<AppState>,
    thread_id: String,
    machinery_id: String,
) -> Result<serde_json::Value, String> {
    let spine = state.reader.snapshot();
    let Some(journal) = spine.machinery_journal() else {
        return Ok(not_retained());
    };
    match journal.raw_payload(&thread_id, &machinery_id) {
        Ok(Some((payload, truncated))) => Ok(serde_json::json!({
            "state": "retained",
            "payload": payload,
            "truncated": truncated,
            "note": truncated.then_some(RAW_TRUNCATED),
        })),
        Ok(None) => Ok(not_retained()),
        Err(why) => Ok(serde_json::json!({
            "state": "unreadable", "payload": null, "truncated": false,
            "note": RAW_UNREADABLE, "reason": why,
        })),
    }
}

/// The sentence for §2.4's evicted-raw state. **The record still renders** — structure,
/// title, status, paths, summary — and this says why the bytes are gone rather than showing
/// a blank. An honest degrade.
///
/// It states the fact and NOT a duration, because the duration is §7.2 and §7.2 is the
/// CEO's open question. A sentence naming "14 days" would answer it in copy.
const RAW_NOT_RETAINED: &str =
    "The full output isn't kept this long. What's above is the whole record that was.";

/// §2.4's 32 KB per-record cap fired. Named so nobody reads a prefix as the whole thing.
const RAW_TRUNCATED: &str = "This output was longer than RichOS keeps; you're seeing the start of it.";

const RAW_UNREADABLE: &str =
    "I can't read the stored output for this one. It's on this machine and I haven't lost it. \
     Whoever set RichOS up needs to look.";

fn not_retained() -> serde_json::Value {
    serde_json::json!({
        "state": "not_retained", "payload": null, "truncated": false, "note": RAW_NOT_RETAINED,
    })
}

/// WHICH COMPANY A THREAD LIVES IN, for the techy-mode resolver's middle tier — or `None`
/// when it lives in none.
///
/// **`None` HERE IS NOT AN ERROR AND MUST NOT BECOME ONE.** A thread written before entity
/// scoping existed is unbound (`Ledger::thread_binding` -> `LedgerError::UnboundThread`),
/// and so is a thread id that names nothing at all — which the renderer can legitimately
/// ask about, because `refreshTechy` runs on every thread switch including the one that
/// races a deletion. Techy mode is a RENDERING preference; refusing to answer "should this
/// conversation show the technical view?" because its binding is unreadable would take the
/// calm surface away on a failure that has nothing to do with the question. So an
/// unresolvable company falls through to the global default, exactly as
/// `ConfigStore::techy_mode` documents for `entity_id: None`.
///
/// This deliberately does NOT go through `assignment_scope`: that function is a SECURITY
/// boundary for reading another company's work and fails closed by design. This one is a
/// preference lookup and fails open to the calm answer. Same question, two different jobs,
/// and collapsing them would drag one of the two to the wrong side.
fn techy_company_of(state: &State<AppState>, thread_id: &str) -> Option<String> {
    let spine = state.reader.snapshot();
    let binding = spine.ledger().thread_binding(thread_id).ok()?;
    Some(binding.entity_id().as_str().to_string())
}

/// This thread's resolved techy-mode state, with its provenance (§3.1, and §7.1's three
/// tiers since 2026-09-18).
///
/// `source` is `"thread"` when the CEO pinned this conversation, `"entity"` when he pinned
/// the company it lives in, and `"default"` when it follows the global switch — so the
/// surface can say *"follows your default"* instead of implying a choice he did not make,
/// and so clearing a pin is a visible, reversible act.
#[richos_command::command]
fn techy_mode(state: State<AppState>, thread_id: String) -> TechyMode {
    let company = techy_company_of(&state, &thread_id);
    state.config.lock().unwrap().techy_mode(&thread_id, company.as_deref())
}

/// **THE CEO'S THREE RADIO BUTTONS, AS ONE COMMAND** (§7.1, his answer of 2026-09-18).
///
/// `scope` is one of `all-companies` / `company` / `thread`, in his order. One call applies
/// the whole choice — see `ConfigStore::apply_techy_scope` for why applying a tier also
/// clears the tiers below it on the path to THIS thread and on no other path — and returns
/// the resolved answer, so the surface renders what the store took rather than what it
/// asked for.
///
/// An unknown scope is REFUSED rather than rounded to a tier, and a `company` scope on a
/// thread with no company is refused by the store. Both refusals come back as a message
/// and leave the store untouched.
#[richos_command::command]
fn set_techy_scope(
    state: State<AppState>,
    thread_id: String,
    scope: String,
    enabled: bool,
) -> Result<TechyMode, String> {
    let scope = TechyScope::parse(&scope)
        .ok_or_else(|| format!("unknown techy scope: {scope}"))?;
    let company = techy_company_of(&state, &thread_id);
    let mut config = state.config.lock().unwrap();
    config
        .apply_techy_scope(scope, &thread_id, company.as_deref(), enabled)
        .map_err(|e| e.to_string())
}

/// Pin or unpin ONE thread (§3.1). `enabled: null` clears the override and hands the
/// thread back to the global default.
///
/// **The `null` arm is what keeps §7.1 open** (global default vs per-thread only — the
/// CEO's, unanswered). Without it a pin is one-way, "all of their conversations" stops
/// reaching any thread he ever touched, and the product has answered his question for him.
#[richos_command::command]
fn set_techy_mode(
    state: State<AppState>,
    thread_id: String,
    enabled: Option<bool>,
) -> Result<TechyMode, String> {
    let company = techy_company_of(&state, &thread_id);
    let mut config = state.config.lock().unwrap();
    match enabled {
        Some(on) => config.set_techy_thread(&thread_id, on).map_err(|e| e.to_string())?,
        None => config.clear_techy_thread(&thread_id).map_err(|e| e.to_string())?,
    }
    // Resolved through all three tiers on the way back: with the pin cleared, this thread
    // now follows its COMPANY if that company is pinned, and only then the global default.
    Ok(config.techy_mode(&thread_id, company.as_deref()))
}

/// The one switch for "all of their conversations" (§3.1, the CEO's own words).
///
/// Threads he pinned individually are untouched — that is what makes a pin mean something,
/// and it is the half of §7.1 a global-only build would lose.
#[richos_command::command]
fn set_techy_default(state: State<AppState>, enabled: bool) -> Result<bool, String> {
    let mut config = state.config.lock().unwrap();
    config.set_techy_default(enabled).map_err(|e| e.to_string())?;
    Ok(config.techy_default())
}

// ---------------------------------------------------------------------------------------
// THE RAW-RETENTION WINDOW AS A SETTING (design §7.2 — open-items 1.4).
//
// §7.2 IS THE CEO'S QUESTION AND IS NOT ANSWERED HERE. These two commands answer a
// different one: what does each of HIS possible answers cost? Until they existed, "14 days"
// was two `const`s and "forever" was a developer, and a question whose status quo is the
// only free answer is not really open. Now all three cost a click.
//
// EVICTION IS AN `unlink` AND NOTHING ANNOUNCES IT, which is the whole reason `set_` below
// applies the new window IMMEDIATELY and returns what it removed. Boot-only eviction would
// make a tightened window look like it did nothing, and then delete at some later launch he
// would never connect to the click. Immediate + counted turns a silent delayed delete into a
// visible present one. Tier A is untouched at every setting, so what is being counted is
// stored OUTPUT, never a record.
// ---------------------------------------------------------------------------------------

/// The window, as the surface needs to show it: the menu entry it is (or `custom`), both
/// axes verbatim, and what the store currently costs on disk.
///
/// `retained_bytes` is not decoration. "Keep everything" is only a real choice if the person
/// making it can see what it costs, and `raw_bytes()` answers that off directory metadata
/// without parsing a byte of anyone's terminal output.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RetentionView {
    /// `two-weeks` | `three-months` | `forever` | `custom`.
    choice: &'static str,
    /// Days, or the string `"forever"`.
    age_days: richos_core::journal::RetentionLimit,
    /// Bytes, or the string `"forever"`.
    total_bytes: richos_core::journal::RetentionLimit,
    /// Raw payload bytes currently on disk, install-wide.
    retained_bytes: u64,
    /// Raw day-shards removed by the call that produced this view. Always 0 for a read.
    evicted: usize,
}

impl RetentionView {
    fn of(retention: RawRetention, choice: RetentionChoice, retained_bytes: u64, evicted: usize) -> Self {
        RetentionView {
            choice: choice.as_str(),
            age_days: retention.age_days,
            total_bytes: retention.total_bytes,
            retained_bytes,
            evicted,
        }
    }
}

/// What the window is set to right now, and what it currently costs.
#[richos_command::command]
fn raw_retention(state: State<AppState>) -> RetentionView {
    let config = state.config.lock().unwrap();
    let bytes = MachineryJournal::new(&state.machinery_root).raw_bytes();
    RetentionView::of(config.raw_retention(), config.retention_choice(), bytes, 0)
}

/// Set the window from one of the three named choices, and apply it now.
///
/// An unrecognized choice — including `custom`, which is a description of the file and never
/// an instruction — is REFUSED rather than rounded to something plausible. Nothing is
/// written and nothing is evicted: the failure mode of a bad argument to a command that
/// deletes has to be "did nothing".
#[richos_command::command]
fn set_raw_retention(state: State<AppState>, choice: String) -> Result<RetentionView, String> {
    let Some(choice) = RetentionChoice::parse(&choice) else {
        return Err(format!("unknown retention choice: {choice}"));
    };
    let mut config = state.config.lock().unwrap();
    config.set_retention_choice(choice).map_err(|e| e.to_string())?;
    let retention = config.raw_retention();
    // Applied at the moment of the click, not at the next boot — see the block comment.
    let journal = MachineryJournal::new(&state.machinery_root);
    let evicted = journal.evict_raw_within(richos_core::util::now_millis(), retention);
    Ok(RetentionView::of(retention, config.retention_choice(), journal.raw_bytes(), evicted))
}

// ---------------------------------------------------------------------------------------
// The opening screen's off switch (2026-08-30).
//
// `docs/design/richos-splash-micro-game-2026-08-30.md` §2 (richos-hq) verified that neither
// the splash NOR a way to turn it off existed at richos `1807319`. This branch adds both, in
// the same commit, deliberately: the surface's failure mode is silent — nobody writes in to
// say a splash screen was beneath them, they switch it off — so the switch is the only
// honest instrument we will ever have for knowing whether it is wanted (§7). A splash
// without one cannot be measured.
//
// THE SURFACE ITSELF IS ENTIRELY IN THE WEBVIEW (`app/ui/splash.js`, `splash-library.js`,
// `splash.css`), and nothing here is on the launch path. `setup()` is untouched by this
// slice: the store these three commands read was already opened there for the company name
// and the assertiveness dial, so the shell does not do one byte of extra work at boot on
// account of the splash. The commands below are called AFTER the app is up.
// ---------------------------------------------------------------------------------------

/// Whether the opening screen shows at launch. Default on (`config.rs`).
#[richos_command::command]
fn splash_enabled(state: State<AppState>) -> bool {
    state.config.lock().unwrap().splash_enabled()
}

/// The switch. `config.rs` stamps `splash_disabled_at` on the way off and clears it on the
/// way back on, and ignores a write of the value already stored — so the UI syncing this
/// preference on every launch can never walk the timestamp forward.
#[richos_command::command]
fn set_splash_enabled(state: State<AppState>, enabled: bool) -> Result<(), String> {
    state
        .config
        .lock()
        .unwrap()
        .set_splash_enabled(enabled, richos_core::util::now_millis())
        .map_err(|e| e.to_string())
}

/// The surface reporting that it has been shown. Idempotent and cheap: only the FIRST call
/// in this store's life touches the disk, so the UI calls it unconditionally rather than
/// having to know whether it is the first launch.
///
/// It is the zero point of §7's time-to-disable, and it is MEASUREMENT, never display —
/// §5 bans every counter and score from the CEO's screen and nothing reads this back to him.
#[richos_command::command]
fn splash_note_shown(state: State<AppState>) -> Result<(), String> {
    state
        .config
        .lock()
        .unwrap()
        .note_splash_shown(richos_core::util::now_millis())
        .map(|_wrote| ())
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------
// APPEARANCE, AND THE PERSON AT THE FOOT OF THE RAIL (CEO ruling §15, 2026-08-30)
// ---------------------------------------------------------------------------------------
//
// Five commands, and NONE of them is on the launch path. The webview applies the CEO's
// theme and type size from a synchronous mirror in `theme-boot.js` before its first paint —
// an async round trip through this shell cannot decide frame one, and booting on the default
// then correcting is a full-screen flash of the wrong palette on every launch. These
// commands are what makes the preference DURABLE and what reconciles the mirror against the
// truth once the app is up. The store they read was already opened in `setup()` for the
// company name and the assertiveness dial, so this costs the boot nothing.
//
// WHO WINS A DISAGREEMENT: this side. `RichTheme.sync` in the webview takes what
// `get_appearance` returns and corrects the mirror, never the other way round. The mirror is
// a cache of a decision, not a second place the decision is made.
//
// ON ⌘/CTRL +/−/0 BEING THE APP'S: `zoomHotkeysEnabled` is now set to `false` EXPLICITLY in
// tauri.conf.json rather than left to its default. That flag is precisely the seam — with it
// on, Tauri injects a polyfill on macOS/Linux that zooms the webview 20% a step, and sets
// WebView2's `IsZoomControlEnabled` on Windows. Webview zoom would scale the whole document
// including fixed chrome, would not persist, and would be invisible to the Text size row:
// two controls, two states, one of them a lie. The app's own handler (`settings-button.js`)
// takes the keystroke with `preventDefault` and moves the single persisted number below.

/// The two appearance preferences, in one call — the UI wants both at init and neither is
/// useful without the other.
#[derive(serde::Serialize)]
struct Appearance {
    /// "dark" | "light" | "system". The CHOICE, not the resolved palette: resolving
    /// "system" needs the OS preference, which is observable in the webview and not here.
    theme: String,
    /// A percentage of the 16px root, always one of `FONT_SCALE_STEPS`.
    font_scale: u16,
    /// Whether he keeps the Oxford comma in what Claude writes (CEO, 2026-10-19). `false`, his
    /// default, deletes it where Rich's and Stu's words and the question cards are drawn.
    oxford_comma: bool,
}

#[richos_command::command]
fn get_appearance(state: State<AppState>) -> Appearance {
    let config = state.config.lock().unwrap();
    Appearance {
        theme: config.theme().as_str().to_string(),
        font_scale: config.font_scale(),
        oxford_comma: config.oxford_comma(),
    }
}

/// Persist the Oxford comma choice (CEO, 2026-10-19: "make the Oxford comma linter switchable in
/// settings"), and tell a watching phone at once so its screen changes with this one's.
#[richos_command::command]
fn set_oxford_comma(app: AppHandle, state: State<AppState>, keep: bool) -> Result<(), String> {
    state.config.lock().unwrap().set_oxford_comma(keep).map_err(|e| e.to_string())?;
    if let Some(phone) = app.try_state::<Arc<phone::PhoneRuntime>>() {
        phone.announce_oxford_comma(keep);
    }
    Ok(())
}

/// Persist the CEO's lighting. An unparseable string is REFUSED rather than coerced to the
/// default: silently writing "dark" because the UI sent something unexpected would look
/// exactly like the CEO changing his mind, on his own machine, for no reason he can see.
#[richos_command::command]
fn set_theme(state: State<AppState>, theme: String) -> Result<(), String> {
    let parsed = richos_core::config::Theme::parse(&theme)
        .ok_or_else(|| format!("unknown theme {theme:?}: expected \"dark\", \"light\" or \"system\""))?;
    state.config.lock().unwrap().set_theme(parsed).map_err(|e| e.to_string())
}

/// Persist the type size. Off-ladder values are snapped by `config.rs` rather than refused,
/// for the reason stated there: rejecting would put a hand-edited file silently back to 100%.
#[richos_command::command]
fn set_font_scale(state: State<AppState>, scale: u16) -> Result<(), String> {
    state.config.lock().unwrap().set_font_scale(scale).map_err(|e| e.to_string())
}

/// The person, and his initials, or honest nulls.
///
/// BOTH FIELDS ARE NULLABLE AND THAT IS THE POINT. The CEO's correction to round 10.1 is
/// that the foot of HIS rail carries HIS identity, not Rich's — and there was no user-name
/// preference in this product until today, so "unset" is the overwhelmingly common state and
/// has to be a real answer rather than an edge case. It is NOT `get_company_name`'s shape:
/// that one has a fallback because a company with no name can honestly be called
/// "My Company", and a person with no name cannot be called anything without inventing them.
#[derive(serde::Serialize)]
struct UserIdentity {
    name: Option<String>,
    initials: Option<String>,
}

#[richos_command::command]
fn get_user_identity(state: State<AppState>) -> UserIdentity {
    let config = state.config.lock().unwrap();
    UserIdentity {
        name: config.user_name().map(|s| s.to_string()),
        initials: config.user_initials(),
    }
}

/// Set (or, with a blank string, clear) the CEO's name. Clearing returns the rail footer to
/// its honest unset state rather than storing an empty string that would render as a circle
/// with nothing in it.
#[richos_command::command]
fn set_user_name(state: State<AppState>, name: String) -> Result<(), String> {
    state.config.lock().unwrap().set_user_name(&name).map_err(|e| e.to_string())
}


// ---------------------------------------------------------------------------------------
// THE LAUNCH RECORD — the shell's half
//
// CEO ruling 2026-08-31 (`richos-hq/wiki/gamification.md` § "Splash tracking"). The model
// itself, and every word of why, is in `richos_core::launch`; this layer does three things
// and nothing else: it tells the webview what kind of start this is BEFORE the opening
// screen decides whether to draw, it hands the record's contents to a caller that asks, and
// it takes the id of a splash that was actually shown.
//
// NOTHING HERE IS ON THE LAUNCH PATH. The classification and the window are built in
// `setup()` above (see the block comment there); the two commands below are called after
// the app is usable, from the same "dead last on purpose" tail of `main.js` that already
// carries the splash's other bookkeeping.
// ---------------------------------------------------------------------------------------

/// The one line injected into every window before any of its own scripts run.
///
/// A literal object, not a command the page has to call: `splash.js` runs synchronously at
/// the top of `<body>` and cannot await anything, and a splash that appeared on a
/// crash-restart because the answer arrived a tick late would be exactly the wrong ceremony
/// at exactly the wrong moment.
///
/// `Object.freeze` because this is a statement of fact about how the app started, and a
/// later script that could edit it could make a crash-restart claim to be a fresh launch.
///
/// `ordinal` is WHICH START THIS IS, 1-based, and the CEO's v1 splash rule is a table over
/// it: first start shows #1, second shows #2, third and after show #1. It rides here for
/// exactly the same reason `kind` does — `splash.js` chooses on its first synchronous line
/// and cannot await a command — and it is `null` rather than a guess whenever
/// `LaunchStore::start_ordinal` could not honestly name one.
/// THE ONLY QUESTION THIS SHELL ASKS THE MACHINE — what displays are attached, and how much
/// of each one a window may actually occupy.
///
/// `Monitor::work_area()` is the whole reason this is three lines rather than a research
/// project: on macOS it is `NSScreen.visibleFrame`
/// (`tauri-runtime-wry-2.11.4/src/monitor/macos.rs:6-34`), so the menu bar and the Dock are
/// already subtracted by AppKit and nothing here has to model either. When AppKit cannot
/// hand back an `NSScreen` that same code degrades to the display's full frame, which is why
/// `window_geometry` keeps its own edge margin on top: the work area is not guaranteed to
/// have anything taken off it.
///
/// A FAILURE HERE IS NOT FATAL AND IS NOT SILENT. An empty list is a real answer —
/// `decide` reads it as "nothing could be read" and falls back to the declared preference
/// with the platform left to center it, which is worse than a derived window and far better
/// than refusing to start.
fn read_displays(app: &tauri::AppHandle) -> Vec<window_geometry::Display> {
    let monitors = match app.available_monitors() {
        Ok(monitors) if !monitors.is_empty() => monitors,
        Ok(_) => {
            eprintln!("[richos] window: the runtime reported no attached display");
            return Vec::new();
        }
        Err(error) => {
            eprintln!("[richos] window: displays could not be read: {error}");
            return Vec::new();
        }
    };
    // `Monitor` carries no "is primary" flag, so the primary is identified by geometry
    // against the one the runtime names. If it cannot be identified, `decide` falls back to
    // the first monitor in the list rather than to a constant.
    let primary = app.primary_monitor().ok().flatten();
    monitors
        .iter()
        .map(|monitor| {
            let area = monitor.work_area();
            let is_primary = primary
                .as_ref()
                .is_some_and(|p| p.position() == monitor.position() && p.size() == monitor.size());
            window_geometry::Display::from_work_area(
                monitor.name().cloned(),
                area.position.x,
                area.position.y,
                area.size.width,
                area.size.height,
                monitor.scale_factor(),
                is_primary,
            )
        })
        .collect()
}

/// KEEP WHERE HE PUT IT. Every move and every resize is recorded, so the next launch can
/// offer it back — and `window_geometry::decide` is what refuses to honor it when the
/// display it was recorded on is no longer there.
///
/// THE WRITE HAPPENS ON A BACKGROUND THREAD, WHICH IS NOT A PERFORMANCE CHOICE. The event
/// callback runs on the main thread inside the event loop, and `inner_position()` /
/// `inner_size()` are round trips into that same runtime: on the main thread
/// `send_user_message` handles the message INLINE rather than posting it
/// (`tauri-runtime-wry-2.11.4/src/lib.rs:235-255`), which is re-entry into the loop from
/// inside its own dispatch. So the callback does one thing — nudge a channel, which cannot
/// block — and every query happens off the main thread where the message takes the ordinary
/// proxy path.
///
/// It doubles as the debounce: a drag emits a `Moved` per frame, and this waits for the
/// movement to STOP and then writes once. `GeometryStore::save` drops an unchanged value on
/// top of that, so an idle window costs no writes at all.
///
/// WHAT THIS DOES NOT COVER, stated rather than discovered later: a move in the last ~400 ms
/// before the process dies may not reach the disk, and a crash between the last write and
/// the move is the same. The cost of that is one launch opening where the window was a
/// moment earlier, which is why it is not worth a synchronous write on the main thread.
fn remember_window_geometry(window: &tauri::WebviewWindow, path: std::path::PathBuf) {
    let (nudge, nudged) = std::sync::mpsc::channel::<()>();
    let recorder = window.clone();
    std::thread::spawn(move || {
        let mut store = window_geometry::GeometryStore::new(path);
        while nudged.recv().is_ok() {
            // Coalesce everything that arrives while the window is still moving.
            while nudged.recv_timeout(std::time::Duration::from_millis(400)).is_ok() {}
            let (Ok(position), Ok(size)) = (recorder.inner_position(), recorder.inner_size())
            else {
                continue;
            };
            // The window's own scale factor, not any display's: it is the one that was used
            // to produce the physical values just read.
            let scale = match recorder.scale_factor() {
                Ok(scale) if scale.is_finite() && scale > 0.0 => scale,
                _ => 1.0,
            };
            let monitor = recorder.current_monitor().ok().flatten();
            let (display, display_width, display_height) = match &monitor {
                Some(monitor) => {
                    let area = monitor.work_area();
                    let monitor_scale = if monitor.scale_factor().is_finite()
                        && monitor.scale_factor() > 0.0
                    {
                        monitor.scale_factor()
                    } else {
                        1.0
                    };
                    (
                        monitor.name().cloned(),
                        Some(area.size.width as f64 / monitor_scale),
                        Some(area.size.height as f64 / monitor_scale),
                    )
                }
                None => (None, None, None),
            };
            // CONTENT rect, in logical points — the exact pair the builder takes back, so a
            // save and a restore are the same numbers and not two conversions that can
            // disagree. `inner_position` is the content top-left in the same global
            // top-left-origin space the builder's `position` speaks
            // (`tao-0.35.3/src/platform_impl/macos/window.rs:716-726` against `:202-212`).
            let geometry = window_geometry::SavedGeometry {
                x: position.x as f64 / scale,
                y: position.y as f64 / scale,
                width: size.width as f64 / scale,
                height: size.height as f64 / scale,
                display,
                display_width,
                display_height,
            };
            if let Err(error) = store.save(geometry) {
                eprintln!("[richos] window: geometry not recorded: {error}");
            }
        }
    });
    window.on_window_event(move |event| {
        if matches!(
            event,
            tauri::WindowEvent::Moved(_)
                | tauri::WindowEvent::Resized(_)
                | tauri::WindowEvent::ScaleFactorChanged { .. }
                | tauri::WindowEvent::CloseRequested { .. }
        ) {
            // Cannot block, cannot fail in a way that matters: if the recorder thread is
            // gone the geometry simply stops being remembered.
            let _ = nudge.send(());
        }
    });
}

/// `splash_enabled` IS IN HERE BECAUSE THE CURTAIN DECIDES BEFORE ANY COMMAND CAN ANSWER —
/// audit-8 row 5.
///
/// The CEO switched the opening screen off, `config.json` recorded it
/// (`"splash_enabled": false`, `splash_disabled_at` stamped), and it came back on the next two
/// launches. Every step of the write path is sound; the read path is not. `splash.js` runs in
/// the `<head>` and decides from a `localStorage` MIRROR alone, and `main.js` reconciles that
/// mirror from the backend at the very end of `init()`, under a comment that says so outright:
/// *"neither call affects anything the CEO can see this launch"*.
///
/// **AND THE MIRROR IS SHARED BETWEEN INSTALLS THAT DISAGREE.** WebKit keys its website data by
/// BUNDLE ID and ignores `$HOME`
/// (`~/Library/WebKit/com.richos.app/WebsiteData/…/LocalStorage/localstorage.sqlite3`), so a
/// candidate walked under a QA scratch `HOME` shares one `localStorage` with the CEO's own
/// installed app while reading a DIFFERENT `config.json`. Whichever instance booted last wrote
/// its own durable answer into the shared mirror, so a scratch instance that had been switched
/// off read back the installed app's `true`. Ray's earlier runs used
/// `com.richos.app.RAY-101-RUN-A/B/C` ids for exactly this reason; the walks that found this
/// did not.
///
/// So the durable answer travels in the script that is ALREADY in front of every page script,
/// beside the launch kind, and costs nothing: it is one boolean from a file this process is
/// about to open anyway. `null` means "this process could not read it", and `splash.js` falls
/// back to the mirror exactly as it does today — a degraded path that is no worse than the
/// current behavior, never a curtain drawn on a guess.
fn launch_init_script(kind: LaunchKind, ordinal: Option<u64>, splash_enabled: Option<bool>) -> String {
    // One line per window, so a walk can read "a fresh start with the splash" or "a second
    // window, no splash" off the log instead of off a frame (dictation plan slice 5's proof,
    // the CEO's "still show the splash screen etc as usual"). The splash itself is the
    // webview's (`splash.js` declines every kind but `fresh`, and the switch).
    eprintln!(
        "[richos] window: launch kind {}, splash {}",
        kind.as_str(),
        match (kind, splash_enabled) {
            (LaunchKind::Fresh, Some(false)) => "off by the switch",
            (LaunchKind::Fresh, _) => "on",
            _ => "not for this kind",
        }
    );
    format!(
        "window.__RICHOS_LAUNCH__ = Object.freeze({{ kind: {:?}, ordinal: {}, splashEnabled: {} }});",
        kind.as_str(),
        ordinal.map(|n| n.to_string()).unwrap_or_else(|| "null".to_string()),
        splash_enabled
            .map(|b| b.to_string())
            .unwrap_or_else(|| "null".to_string())
    )
}

/// The durable opening-screen answer, read straight off disk before the window exists.
///
/// `ConfigStore::open` is opened again fifty lines later for the app's own use and never fails
/// on a missing or corrupt file — it degrades to defaults internally — so this is a second cheap
/// read of one file rather than a new dependency or a new failure mode. It returns `None` only
/// when the store genuinely could not be opened, which is the one case where guessing would be
/// worse than falling back to the mirror.
fn durable_splash_enabled(data_dir: &std::path::Path) -> Option<bool> {
    ConfigStore::open(&data_dir.join("config.json"))
        .ok()
        .map(|store| store.splash_enabled())
}

/// The whole record, for a caller that supplies its own UTC offset.
///
/// **The offset is a PARAMETER and not read here**, and that is the design and not a
/// shortcut: the CEO ruled that timestamps are stored in UTC and bucketed against his LOCAL
/// calendar, and the only place in this process that knows what local means is the webview
/// (`-new Date().getTimezoneOffset()`). Reading a timezone in Rust would need a fifth
/// dependency in a crate whose "nothing can reach off this machine" guarantee rests on
/// having four.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchStateView {
    /// `"fresh"`, `"crash-restart"` or `"second-window"` — the run's kind, not this
    /// window's. Windows are classified at creation, in `setup()`.
    kind: Option<String>,
    /// `None` when the record on disk could not be read. A zero is a claim about his
    /// history and this refuses to make one it cannot support.
    counts: Option<LaunchCounts>,
    /// First-run marker, epoch millis. Every milestone measured in days is measured from it.
    installed_at: Option<u64>,
    /// The last few splash ids shown, most recent first.
    recent_splashes: Vec<String>,
    /// False when the file on disk exists and this build does not understand it. Nothing is
    /// written in that state.
    readable: bool,
    schema_version: u32,
}

/// Read the launch record. `utc_offset_minutes` is positive east — the NEGATION of
/// JavaScript's `getTimezoneOffset()`, so US Pacific daylight time is `-420`.
#[richos_command::command]
fn launch_state(state: State<AppState>, utc_offset_minutes: i32) -> LaunchStateView {
    // The same offset lets Rich's quota lines say a clock time (round 16: "until 2:17 AM").
    state.quota.set_utc_offset(utc_offset_minutes);
    let launch = state.launch.lock().unwrap();
    LaunchStateView {
        kind: launch.run_kind().map(|k| k.as_str().to_string()),
        counts: launch.counts(richos_core::util::now_millis(), utc_offset_minutes),
        installed_at: launch.installed_at(),
        recent_splashes: launch.recent_splashes().to_vec(),
        readable: launch.readable(),
        schema_version: launch.schema_version(),
    }
}

/// Record that a splash was actually shown, onto the front of the recency ring.
///
/// Called by the surface that drew it, so the ring holds what was on screen rather than
/// what was chosen — the two differ whenever a draw is made and the render then declines
/// (`splash.js` has three such paths, and all three leave `state.shown` false).
#[richos_command::command]
fn launch_note_splash_shown(state: State<AppState>, id: String) -> Result<(), String> {
    state.launch.lock().unwrap().note_splash_shown(&id).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------
// THE HOME SCREEN'S ENTITY ROW (CEO, 2026-09-01)
//
// His words, in the order he gave them:
//
//   "At the very top there should be a slim row with buttons named after the user's
//    entities/companies. For RichOS v1 those buttons don't need to do anything. But
//    eventually the user should be able to 'switch' their loro visual from all companies
//    (which is default) to display the loro for just one of their companies."
//
//   "the company buttons should only appear if the user has more than one company. Not if
//    there's only one. And the user should also be able to customize in the settings the
//    labels on those company buttons and well as which to display on their home screen.
//    So, if the user wanted to anonymize their home screen (for sharing on social media),
//    they could change the label buttons to something like '1', '2', '3' etc."
//
// THREE PROPERTIES THIS LAYER HAS AND MUST KEEP:
//
//   1. **The names come from the REGISTRY.** `EntityRegistry::ceos_companies()` through the
//      spine, in registry order, never filtered and never re-sorted — the same source the
//      company picker reads. A list typed into the UI would be wrong the day he adds a
//      company, and `richos` is ONE entity with two roots, which only the registry knows.
//   2. **A label is a MASK, never a rename.** `id` on the wire is always the registry's id;
//      `label` is what the button says. Nothing downstream keys on `label`, and
//      `config.rs`'s own test `a_label_is_a_mask_and_never_a_rename` is what holds that.
//   3. **An absent override is the registry's display name.** Never blank, never the raw id.
//      That is decided HERE, in `resolve`, so no surface can get it wrong on its own.
//
// WHAT IS DELIBERATELY NOT HERE: any effect on the picture. He was explicit that the buttons
// do nothing in v1, and filtering the field by entity is later work. What IS real is the
// label and the visibility, because those are what he would use before the filtering exists.
// ---------------------------------------------------------------------------------------

/// One button in the home screen's row, resolved.
#[derive(serde::Serialize)]
struct HomeEntityView {
    /// The registry's id. This is the identity and it is never affected by a label.
    id: String,
    /// The registry's own name for the company, so a surface can offer "back to the real
    /// name" without a second round trip.
    display_name: String,
    /// What the button SAYS: his override if he set one, otherwise `display_name`.
    label: String,
    /// His override on its own, or `None` when he has not set one. The settings surface
    /// needs the difference — a field pre-filled with the registry name looks like an
    /// override he made, and clearing it would then be indistinguishable from leaving it.
    custom_label: Option<String>,
    /// Whether this company appears in the row. Absent opinion means yes.
    visible: bool,
}

/// The row, resolved and in registry order.
///
/// **The "only if more than one" rule is NOT applied here**, and that is deliberate: this
/// command reports what IS, and the settings surface has to list every company — including
/// the ones he has hidden — or he could never unhide one. The count that decides whether the
/// home screen draws a row at all is `visible`, and the home screen applies it.
#[richos_command::command]
fn home_entity_row(state: State<AppState>) -> Vec<HomeEntityView> {
    // Lock order, everywhere in this file: config, then entity, then spine.
    let config = state.config.lock().unwrap();
    let spine = state.reader.snapshot();
    spine
        .entity_registry()
        .entities()
        .iter()
        .map(|e| {
            let id = e.id.to_string();
            let custom = config.home_entity_label(&id).map(|s| s.to_string());
            HomeEntityView {
                label: custom.clone().unwrap_or_else(|| e.display_name.clone()),
                visible: config.home_entity_visible(&id),
                display_name: e.display_name.clone(),
                custom_label: custom,
                id,
            }
        })
        .collect()
}

/// Set (or clear) his own label for one company.
///
/// `None` or an all-whitespace string CLEARS the override and the button goes back to the
/// registry's name. That is not a convenience: it is how he un-anonymizes the screen, and a
/// one-way override would have made an anonymized home screen permanent.
///
/// It REFUSES an unregistered id rather than storing a key nothing will ever read — the same
/// fail-closed posture `choose_entity` takes, and for the same reason.
#[richos_command::command]
fn set_home_entity_label(
    state: State<AppState>,
    entity_id: String,
    label: Option<String>,
) -> Result<Vec<HomeEntityView>, String> {
    let id = registered_entity(&state, &entity_id)?;
    state
        .config
        .lock()
        .unwrap()
        .set_home_entity_label(id.as_str(), label.as_deref())
        .map_err(|e| format!("I couldn't write that down, so I haven't changed the button: {e}"))?;
    Ok(home_entity_row(state))
}

/// Show or hide one company in the home screen's row. Display only — the company, its
/// threads and its records are untouched, and it is still everywhere else in the app.
#[richos_command::command]
fn set_home_entity_visible(
    state: State<AppState>,
    entity_id: String,
    visible: bool,
) -> Result<Vec<HomeEntityView>, String> {
    let id = registered_entity(&state, &entity_id)?;
    state
        .config
        .lock()
        .unwrap()
        .set_home_entity_visible(id.as_str(), visible)
        .map_err(|e| format!("I couldn't write that down, so I haven't changed the row: {e}"))?;
    Ok(home_entity_row(state))
}

/// Parse and check an id against the registry, or refuse with the sentence the CEO reads.
/// Shared by both setters so the two cannot drift apart on what they accept.
fn registered_entity(state: &State<AppState>, entity_id: &str) -> Result<EntityId, String> {
    let id = EntityId::parse(entity_id.trim()).map_err(|_| unknown_company_message(entity_id.trim()))?;
    if !state.reader.snapshot().entity_registry().contains(&id) {
        return Err(unknown_company_message(id.as_str()));
    }
    Ok(id)
}

// ---------------------------------------------------------------------------------------
// THE HOME SCREEN'S PICTURE, OUT OF HIS OWN CORPUS (`richos_core::home_field`).
//
// `app/ui/home.js`'s banner block named this command before it existed, and named what it
// must not do just as precisely. Both halves are kept here because both are load-bearing.
// ---------------------------------------------------------------------------------------

/// **COMPILE THE CUSTOMER'S OWN MEMORY INTO THE FIELD'S STRUCTURE, OR SAY THAT THERE IS NONE.**
///
/// # What it answers
///
/// `{ "available": true, "field": { meta, nodes, links, sources, ... } }` when a corpus
/// resolved and the compiler produced something, and
/// `{ "available": false, "reason": "<one sentence>" }` in every other case. It is not
/// `Result`, and that is deliberate: **no corpus is not an error.** It is the ordinary state
/// of a fresh install, the home screen keeps drawing the demonstration, and a rejected
/// promise would put a failure in the console for a launch in which nothing failed.
///
/// # WHAT IT DELIBERATELY DOES NOT DECIDE
///
/// Whether the picture it returns should replace the demonstration. It reports
/// `meta.counts.records` beside `meta.counts.objects` and stops. The CEO's instruction for
/// this work is *"The demo is definitely needed, initially, for the user"*, and the design of
/// a handover is `richos-hq/design/mockups/rounds/round-11.4`, which is unruled. The
/// preference lives in exactly one place — `home.js`'s `HOME_FIELD_MIN_OBJECTS` — and its
/// default keeps the demonstration.
///
/// # AND IT IS NOT GATED ON `memory_status`
///
/// `home.js` says why, and it is the whole reason this command exists rather than a boolean:
/// *"A provisioned corpus is not the same fact as a drawn picture, and a banner that vanished
/// the moment a customer created a folder would be claiming his data was on screen while
/// 7,500 synthetic objects were still on it."* So this compiles, and answers with what it
/// compiled. A corpus that resolves and then produces nothing is `available: false`.
///
/// # It does not take the spine's lock
///
/// The company list comes off the spine's published snapshot (`AppState::reader`), and the
/// corpus is reached through `AppState::loro_install` — the SAME `LoroInstall`
/// `wire_company_memory` built the reader and the writer from, carried rather than resolved a
/// second time — so the home screen draws while Rich is mid-turn, exactly as the correction
/// desk and the stop control do.
///
/// `(async)` because it spawns one `node` per company: 6 compiles, 2.795 s wall clock,
/// measured against the CEO's own corpus on 2026-09-06. That must never run on the IPC
/// thread, and it is never on the boot path either — `home.js` calls it from an idle
/// callback with the demonstration already on the screen.
#[richos_command::command]
fn home_field_data(state: State<AppState>) -> serde_json::Value {
    fn no(reason: impl Into<String>) -> serde_json::Value {
        serde_json::json!({ "available": false, "reason": reason.into() })
    }

    // The clone releases the lock immediately, the same shape `desk()` uses: a compile that
    // takes three seconds must not hold a lock `provision_memory` needs.
    let Some(install) = state.loro_install.lock().unwrap().clone() else {
        let status = state.memory.lock().unwrap().clone();
        return no(match status.state.as_str() {
            "none" => "no corpus is configured on this machine".to_string(),
            "no-compiler" => "a corpus is configured and the memory compiler is not installed".to_string(),
            "" => "company memory was never wired on this launch".to_string(),
            other => format!("company memory is {other}"),
        });
    };
    let registry = state.reader.snapshot().entity_registry().clone();

    let census = match home_field::probe_census(install.tools(), install.root()) {
        Ok(c) => c,
        // POSITIVE SIGNAL ONLY, and this is where it bites. A census that could not be read
        // is NOT an empty corpus — the two are different facts, and drawing the second from
        // the first would put a near-empty picture of nothing in front of him and call it his
        // memory. `CorpusLanes::probe` makes the same refusal one seam over.
        Err(e) => {
            eprintln!("[richos] home field: could not read the corpus census ({e}) — the home screen keeps the demonstration");
            return no(format!("the corpus could not be read: {e}"));
        }
    };

    let topics = home_field::field_topics(&census, &registry);
    let items = match home_field::collect_items(install.tools(), install.root(), &topics) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[richos] home field: no topic compiled ({e}) — the home screen keeps the demonstration");
            return no(format!("the corpus could not be compiled: {e}"));
        }
    };

    let field = home_field::build_field(&census, &items, &registry);
    eprintln!(
        "[richos] home field: {} object(s) of {} record(s) at {}, {} link(s), {} source(s), \
         {} question(s) asked. Whether that replaces the demonstration is home.js's \
         HOME_FIELD_MIN_OBJECTS, not this line.",
        field["meta"]["counts"]["objects"],
        field["meta"]["counts"]["records"],
        install.root().path().display(),
        field["meta"]["counts"]["links"],
        field["meta"]["counts"]["sources"],
        topics.len(),
    );
    serde_json::json!({ "available": true, "field": field })
}

/// **The teammates the user added, for the home screen** (product hunt 2026-10-15, finding 7:
/// after a relaunch the home screen showed a made-up company of 18 specialists to a user who
/// had just hired two). `home.js` shows these, by name and role, and nothing made up, once
/// there is at least one; with none it keeps the first-run demonstration. A folder that
/// cannot be read is an error, never an empty team.
#[richos_command::command]
fn home_team(state: State<'_, AppState>) -> Result<Vec<richos_core::engine_profile::Teammate>, String> {
    richos_core::engine_profile::user_teammates(&state.data_dir).map_err(|e| e.0)
}


// ---------------------------------------------------------------------------------------
// ROW 5 AND ROW 9 — the window closes and the work keeps going; a crash invents nothing
// (the background-work spec §2.4/§2.4a/§2.5/§2.5a and §6; `lifecycle.rs` holds the decision)
// ---------------------------------------------------------------------------------------

/// The id of the one menu item this app owns. Matched in `on_menu_event`.
const MENU_QUIT: &str = "richos-quit";

/// **The question, on his screen.** Pushed rather than polled, like every other out-of-turn
/// line (`rich://work-notice`'s own reasoning): there is no turn open when he presses Quit.
pub const EVENT_QUIT_QUESTION: &str = "rich://quit-question";

/// Pending questions keep phone answers possible while the Mac window is closed.
fn questions_pending(state: &AppState) -> bool {
    // A read failure must not silently disconnect a phone. Explicit Quit still works.
    question_host::store(state).pending_for_residency().unwrap_or(true)
}

/// What the assignment register says, using the same derivation as the update gate.
fn registered_work(state: &AppState) -> lifecycle::Registered {
    let work = state.work.background_work();
    lifecycle::Registered {
        running: work.running + usize::from(state.control.has_active_turns())
            + usize::from(state.work.peer_work_pending())
            + usize::from(!state.control.pending_intake().is_empty()),
        awaiting_you: work.awaiting_you,
        readable: work.readable,
        // **His team, on an operator install** (r3 (m)), read from the leads' own streams:
        // this runs inside the exit callback, which must answer at once, so no script is run
        // here (`OperatorHost::team_from_stream`). It names the agents the quit would stop.
        team: state.operator.as_ref()
            .and_then(|desk| richos_core::work_gate::operator_quit_sentence(&desk.team_from_stream())),
        // Commands Rich started, on every live lease (reap gap C5): a few small files, no lock.
        commands: richos_core::lease_commands::LiveLeases::process().reading(),
    }
}

/// **STEP ONE of the two-step quit (§2.5a).** Called from the Quit menu item.
///
/// It never exits by itself when work is registered. With nothing registered it raises the
/// app's own exit, which enters the `ExitRequested` arm with `code: Some(0)` — the
/// preventable path — and is allowed straight through.
fn request_quit(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        app.exit(0);
        return;
    };
    if !registered_work(&state).anything() {
        app.exit(0);
        return;
    }
    ask_before_quitting(app.clone());
}

/// **STEP TWO: ask him, outside the callback.**
///
/// It runs on its own thread for one reason: `ExitRequested`'s answer is read with
/// `try_recv` the instant the callback returns (§2.5a), so anything that waits must happen
/// after it. Nothing here waits for a reply either — the reply arrives as a command
/// ([`confirm_quit_and_stop`] or [`cancel_quit`]).
///
/// **If there is no window, the window comes back first.** A quit while the app is resident
/// and windowless is exactly the state §2.4 creates, and a question nobody can see is not a
/// question.
fn ask_before_quitting(app: AppHandle) {
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        let question = lifecycle::quit_question(&registered_work(&state));
        reopen_window(&app);
        if let Err(error) = app.emit(
            EVENT_QUIT_QUESTION,
            serde_json::json!({
                "say": question,
                "quit": lifecycle::QUIT_AND_STOP,
                "stay": lifecycle::KEEP_WORKING,
            }),
        ) {
            // **A question he cannot be shown must not become a silent refusal to quit.**
            // The honest fallback is the behavior this app had before today: quit, and stop
            // the work, rather than leaving him pressing Quit against a process that will
            // not go away and says nothing about why.
            eprintln!("[richos] the quit question could not be shown ({error}); quitting as this app used to");
            state.quit_confirmed.store(true, std::sync::atomic::Ordering::SeqCst);
            app.exit(0);
        }
    });
}

/// **The way back into a window** — the Dock icon (§2.4), and the quit question's own
/// precondition.
///
/// A window that is merely hidden is shown; otherwise one is built from the same config the
/// boot path builds from, with the geometry he left it at. It is a `SecondWindow` launch:
/// nothing begins, no opening screen, no count (`launch.rs:149-151`) — this is a window
/// coming back on a run that never stopped, which is exactly what that kind is for.
fn reopen_window(app: &AppHandle) {
    if let Some(window) = app.webview_windows().values().next() {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    let Some(config) = app.config().app.windows.first().cloned() else {
        eprintln!("[richos] no window could be reopened: this build declares none");
        return;
    };
    let data_dir = app
        .try_state::<AppState>()
        .map(|state| state.data_dir.clone())
        .unwrap_or_else(std::env::temp_dir);
    let displays = read_displays(app);
    let geometry_path = data_dir.join("window.json");
    let saved = window_geometry::GeometryStore::new(&geometry_path).load();
    let preference = window_geometry::Preference {
        width: config.width,
        height: config.height,
        min_width: config.min_width.unwrap_or(window_geometry::PREFERRED_MIN_WIDTH),
        min_height: config.min_height.unwrap_or(window_geometry::PREFERRED_MIN_HEIGHT),
    };
    let placement = window_geometry::decide_with(&displays, saved.as_ref(), preference);
    let mut builder = match tauri::WebviewWindowBuilder::from_config(app, &config) {
        Ok(builder) => builder,
        Err(error) => {
            eprintln!("[richos] the window could not be rebuilt: {error}");
            return;
        }
    };
    builder = builder
        .inner_size(placement.width, placement.height)
        .min_inner_size(placement.min_width, placement.min_height)
        .initialization_script(launch_init_script(
            richos_core::launch::LaunchKind::SecondWindow,
            None,
            durable_splash_enabled(&data_dir),
        ))
        .initialization_script_for_all_frames(webview_menu::EVERY_FRAME)
        // He asked for it by clicking the Dock icon, so it is visible and focused — the
        // unattended-boot reasoning at the setup site is about a launch nobody asked for.
        .visible(true)
        .focused(true);
    if let Some((x, y)) = placement.position {
        builder = builder.position(x, y);
    }
    match builder.build() {
        Ok(window) => {
            remember_window_geometry(&window, geometry_path);
            // A window rebuilt from the Dock takes drops too (CEO §86).
            mac_attachments::listen_for_drops(&window);
            let _ = window.set_focus();
        }
        Err(error) => eprintln!("[richos] the window could not be rebuilt: {error}"),
    }
}

/// **He chose to quit** (§7.4a). The work is stopped and said to be stopped, and only then
/// does the process end.
///
/// The flag is what the `ExitRequested` arm reads on the second pass; without it the same
/// arm would prevent the exit again and the app could never be quit at all.
#[richos_command::command]
fn confirm_quit_and_stop(app: AppHandle, state: State<'_, AppState>) {
    state.quit_confirmed.store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
}

/// **He chose to keep working.** Nothing happens to the work, which is the point: the
/// prevent came first and the question second, so it was never touched.
#[richos_command::command]
fn cancel_quit(state: State<'_, AppState>) -> bool {
    state.quit_confirmed.store(false, std::sync::atomic::Ordering::SeqCst);
    true
}

/// **"Let Codex review your team's work": what the Settings row shows** (round 20.2). The
/// user's choice and what the Mac reports about Codex now, read each time Settings opens: Codex
/// found where the review finds it and signed in by its own `login status` (a few milliseconds;
/// `richos_core::codex_reviews`). Asked in the environment the reviews run in.
#[richos_command::command]
fn codex_reviews_status(state: State<'_, AppState>) -> richos_core::codex_reviews::Status {
    let probe = richos_core::codex_reviews::Probe::system(&richos_core::codex_reviews::search_path());
    richos_core::codex_reviews::status(&state.data_dir, &probe)
}

/// **The switch pressed.** On is refused while Codex is not installed and signed in (the row
/// shows why, and nothing is written); off always goes through. The next review the app's
/// review-watch starts reads it. The state after.
#[richos_command::command]
fn codex_reviews_set(state: State<'_, AppState>, on: bool) -> Result<richos_core::codex_reviews::Status, String> {
    let probe = richos_core::codex_reviews::Probe::system(&richos_core::codex_reviews::search_path());
    richos_core::codex_reviews::set(&state.data_dir, &probe, on)
        .map_err(|e| format!("the Codex review setting could not be saved: {e}"))
}

#[cfg(test)]
mod ipc_responsiveness_tests {
    #[test]
    fn synchronous_commands_never_run_on_the_native_event_loop() {
        // A read may wait behind a model turn. It must not monopolize Tauri IPC:
        // the Stop command and streamed feedback need the event loop to run.
        for (name, source) in [
            ("main", include_str!("main.rs")),
            ("updates", include_str!("updates.rs")),
        ] {
            let blocking = concat!("#[tauri::", "command]");
            for line in source.lines() {
                assert_ne!(line.trim(), blocking, "{name}: command needs async dispatch");
            }
        }
    }
}

#[cfg(test)]
mod window_read_tests;

#[cfg(test)]
mod command_rule_tests;

#[cfg(test)]
mod onboarding_decline_tests;

#[cfg(test)]
mod turn_boundary_tests;
