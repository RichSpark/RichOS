//! **BUST A BUG** — the shell's half (CEO §115, round 21). Everything that decides what reaches
//! GitHub is `richos_core::bug_report` and is tested there; this file is the three things that
//! need the Mac: running Rich (`claude`) to write the report, with a picture of the window
//! (`window_picture.rs`) and the screen's words to check against, and the HTTPS requests to
//! RichOS's reporting relay. Plus the ten commands the window calls, the loop that sends a
//! waiting report by itself and the one that asks Rich again about a report he could not check
//! yet.
//!
//! # Which account, today
//!
//! RichOS has no GitHub sign-in yet (§20 is not built), so every report and idea goes out as the
//! RichOS reporting account: RichOS's own relay (`richos/mobile/service/RICHDOCTOR-RELAY.md`,
//! the one RichDoctor files through), whose GitHub App files it ([`ReportingRelay`]). No token is
//! on this Mac, in the source or in the bundle, so it works on any Mac. The user's own account
//! slots in as `Account::User` when a sign-in exists; the window's From line and Rich's sentences
//! already read the account off the context and the delivery rather than assume it.
//!
//! # Request a feature (round 24, CEO 2026-10-11)
//!
//! *"can things not be copy-pasted from the "Bust a bug" feature?"*: a feature idea goes through
//! every command and loop here exactly as a bug report does, named by the window's `post`
//! (`"idea"`; a call without it is a bug report). Its outbox is its own folder
//! (`feature-requests/`), so a kept idea can only ever be posted as an idea, and the one
//! difference is where it is posted: a Discussion under Ideas (the relay's `richos-idea/1`),
//! never an issue.

use richdoctor_core::delivery::{self, Destination};
use richos_core::bug_report as bug;
use richos_core::read_view::SpineView;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

/// The event a report sent by itself arrives on: `{ delivery }`.
pub const EVENT: &str = "rich://bug-report";

/// How long Rich gets to write a report before it is kept on this Mac to be checked later.
const WRITE_DEADLINE: Duration = Duration::from_secs(90);
/// How often the loop looks for a waiting report whose rest is over.
const RETRY_EVERY: Duration = Duration::from_secs(15);
/// One relay request's bound: register and complete answer in seconds, and the relay's contract
/// gives a client 30 (`RICHDOCTOR-RELAY.md`, "Requests").
const RELAY_DEADLINE: Duration = Duration::from_secs(30);

/// The app's one Bust a bug state.
pub struct BugReports {
    outbox: bug::Outbox,
    /// Feature ideas (Request a feature), kept and retried exactly as bug reports are.
    ideas: bug::Outbox,
    /// One attempt at a time, so the loop and *Try now* can never file one report twice.
    sending: Mutex<()>,
    /// While a report waits on the user's words, what they say goes to it (`holding_voice`).
    hold_voice: AtomicBool,
    /// Where `claude` writes from: an empty folder, so no project's CLAUDE.md is read.
    quiet_dir: PathBuf,
    version: String,
    /// The picture of the window taken when Bust a bug was last pressed (`bug_report_look`), for
    /// Rich's write-up. In memory only; replaced at the next press, never written to disk.
    look: Mutex<Option<bug::Picture>>,
    /// **WHAT RICH LAST CHECKED OF EACH REPORT ON A CARD**, by the report's id: the words Send
    /// compares the card's against (`bug::at_send`). Kept here, never by the window, so nothing the
    /// window sends can stand in for his check. In memory only: a card does not outlive the app.
    checks: Mutex<std::collections::HashMap<String, bug::Check>>,
    /// This install's random id for the relay: its per-install rate limit key, never
    /// authentication (`RICHDOCTOR-RELAY.md`, "Register body"). Made once, in `bug-reports/`.
    installation: String,
}

impl BugReports {
    pub fn for_app(data_dir: &Path, app_version: &str) -> Arc<Self> {
        let dir = data_dir.join("bug-reports");
        let quiet_dir = dir.join("writing");
        best_effort("the writing folder", std::fs::create_dir_all(&quiet_dir));
        let installation = installation_id(&dir);
        Arc::new(BugReports {
            outbox: bug::Outbox::open(&dir),
            ideas: bug::Outbox::open_for(&data_dir.join("feature-requests"), bug::Post::Idea),
            sending: Mutex::new(()),
            hold_voice: AtomicBool::new(false),
            quiet_dir,
            version: bug::version_line(app_version, macos_version().as_deref(), std::env::consts::ARCH),
            look: Mutex::new(None),
            checks: Mutex::new(std::collections::HashMap::new()),
            installation,
        })
    }

    /// Where its reports and ideas go: the RichOS reporting relay, or the test pair named by
    /// `RICHDOCTOR_REPORT_RELAY` and `RICHDOCTOR_REPORT_REPOSITORY`, read at each send.
    fn relay(&self) -> ReportingRelay {
        ReportingRelay { destination: Destination::from_env(|k| std::env::var(k).ok()), installation: self.installation.clone() }
    }

    /// The outbox of bug reports, or of feature ideas.
    fn outbox(&self, post: bug::Post) -> &bug::Outbox {
        match post {
            bug::Post::Bug => &self.outbox,
            bug::Post::Idea => &self.ideas,
        }
    }

    /// What Rich last checked, by report. A thread that panicked while holding it left a whole
    /// map (every write is one insert or remove), so it is taken back rather than poisoned.
    fn checks(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, bug::Check>> {
        self.checks.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Whether a spoken utterance belongs to a bug report rather than to Rich's conversation.
pub fn holding_voice(app: &AppHandle) -> bool {
    app.try_state::<Arc<BugReports>>().is_some_and(|b| b.hold_voice.load(Ordering::SeqCst))
}

/// A step whose failure changes nothing the user sees, written down rather than dropped.
fn best_effort<T, E: std::fmt::Display>(what: &str, result: Result<T, E>) {
    if let Err(e) = result {
        eprintln!("[richos] bug report: {what}: {e}");
    }
}

fn macos_version() -> Option<String> {
    let out = std::process::Command::new("/usr/bin/sw_vers").arg("-productVersion").output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

// ---------------------------------------------------------------------------------------
// WHO IT GOES OUT AS, AND THE REQUEST
// ---------------------------------------------------------------------------------------

/// This install's relay id, kept in `dir/installation-id`: the one there, or a new random one
/// written there. One that cannot be written is still used, for this run.
fn installation_id(dir: &Path) -> String {
    let path = dir.join("installation-id");
    if let Ok(kept) = std::fs::read_to_string(&path) {
        if uuid_shaped(kept.trim()) {
            return kept.trim().to_string();
        }
    }
    let id = bug::report_id();
    best_effort("the installation id", std::fs::write(&path, &id));
    id
}

fn uuid_shaped(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    groups.iter().map(|g| g.len()).eq([8, 4, 4, 4, 12]) && groups.iter().all(|g| g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// **THE RICHOS REPORTING ACCOUNT**: RichOS's reporting relay, the one RichDoctor files through.
/// Its GitHub App files the issue (or, for an idea, posts the Discussion under Ideas); this Mac
/// holds no token, so the credential it answers carries none. Each send goes through RichDoctor's
/// own delivery client (`richdoctor_core::delivery::send`): register, then complete, text only.
pub struct ReportingRelay {
    destination: Result<Destination, String>,
    installation: String,
}

impl bug::Credentials for ReportingRelay {
    fn credential(&self) -> Option<bug::Credential> {
        Some(bug::Credential { account: bug::Account::Reporting, token: String::new() })
    }
}

impl bug::Transport for ReportingRelay {
    fn create_issue(&self, _: &bug::Credential, _: &str, request: &serde_json::Value) -> bug::Outcome {
        self.file(false, request["title"].as_str().unwrap_or(""), request["body"].as_str().unwrap_or(""))
    }

    fn post_idea(&self, _: &bug::Credential, title: &str, body: &str) -> bug::Outcome {
        self.file(true, title, body)
    }

    /// Never asked: an idea is posted in one step through the relay (`post_idea`).
    fn graphql(&self, _: &bug::Credential, _: &serde_json::Value) -> bug::Reply {
        bug::Reply::Answered { status: 404, body: serde_json::Value::Null }
    }
}

impl ReportingRelay {
    /// One send through the relay, under a new report ID, with the relay's footer line (its
    /// report ID, which the relay requires as the body's last line) after the approved words.
    /// What came back, as GitHub's answer would have been, so the outbox keeps and retries it
    /// exactly as before.
    fn file(&self, idea: bool, title: &str, body: &str) -> bug::Outcome {
        let destination = match &self.destination {
            Ok(d) => d.clone(),
            Err(why) => {
                eprintln!("[richos] bug report: the reporting relay is not set right ({why}); it waits");
                return bug::Outcome::Status { code: 400 };
            }
        };
        let Ok(id) = delivery::new_report_id() else { return bug::Outcome::Status { code: 503 } };
        let body = format!("{}\n\n{}", body.trim_end(), richdoctor_core::report::ending(&destination.relay, &id, 0));
        let mut approved = match delivery::approve(&id, title, &body, vec![], &destination.relay, now_ms(), &self.installation) {
            Ok(a) => a,
            Err(problems) => {
                eprintln!("[richos] bug report: not a report the relay takes ({problems:?}); it waits");
                return bug::Outcome::Status { code: 400 };
            }
        };
        approved.idea = idea;
        // ITS OWN THREAD AND ITS OWN RUNTIME: a `#[tauri::command(async)]` runs on an async
        // runtime's worker thread, and blocking on a runtime there panics ("Cannot start a runtime
        // from within a runtime", walk-aba5c01c19ce). A plain thread has none around it.
        let sent = std::thread::spawn(move || match Https::new() {
            Some(https) => delivery::send(&approved, &|_| None, &destination, &https, now_ms()),
            None => delivery::State::NotSent { why: delivery::NotSent::Unreachable, at: now_ms() },
        });
        outcome_of(sent.join().unwrap_or(delivery::State::NotSent { why: delivery::NotSent::NoAnswer, at: now_ms() }))
    }
}

/// The relay's answer as the outbox reads GitHub's: the post, or why it waits.
fn outcome_of(state: delivery::State) -> bug::Outcome {
    use delivery::NotSent;
    let why = match state {
        delivery::State::Submitted { issue_url, issue_number, .. } => return bug::Outcome::Created { number: issue_number, url: issue_url },
        delivery::State::Sending => return bug::Outcome::TimedOut,
        delivery::State::NotSent { why, .. } => why,
    };
    eprintln!("[richos] bug report: not sent: {}", why.reason());
    match why {
        NotSent::Offline | NotSent::Unreachable => bug::Outcome::Unreachable,
        NotSent::NoAnswer => bug::Outcome::TimedOut,
        // Closed for now, or busy: it is tried again in a few minutes.
        NotSent::Refused { until: Some(_), .. } => bug::Outcome::Status { code: 429 },
        NotSent::Refused { until: None, .. } | NotSent::Screenshot { .. } => bug::Outcome::Status { code: 422 },
    }
}

/// The relay's HTTPS requests (`richdoctor_core::delivery::Relay`), on the thread that owns this
/// runtime. A connection never made is "not connected"; a timeout or a connection lost after the
/// request went is unclear (it may have been filed).
struct Https {
    client: reqwest::Client,
    runtime: tokio::runtime::Runtime,
}

impl Https {
    fn new() -> Option<Https> {
        crate::voice_provision::ensure_crypto_provider();
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .user_agent("RichOS")
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .ok()?;
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().ok()?;
        Some(Https { client, runtime })
    }

    fn answer(&self, request: reqwest::RequestBuilder) -> delivery::Transport {
        self.runtime.block_on(async move {
            let response = match request.timeout(RELAY_DEADLINE).send().await {
                Ok(r) => r,
                Err(e) if e.is_connect() => return delivery::Transport::NotConnected { offline: false },
                Err(_) => return delivery::Transport::Unclear,
            };
            let status = response.status().as_u16();
            let retry_after = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|s| s.trim().parse::<u64>().ok());
            match response.bytes().await {
                Ok(bytes) => delivery::Transport::Answer { status, body: serde_json::from_slice(&bytes).ok(), retry_after },
                Err(_) => delivery::Transport::Unclear,
            }
        })
    }
}

impl delivery::Relay for Https {
    fn post(&self, url: &str, body: &serde_json::Value) -> delivery::Transport {
        self.answer(self.client.post(url).header(reqwest::header::CONTENT_TYPE, "application/json").body(body.to_string()))
    }

    /// Never asked: a report or an idea from here carries no screenshot.
    fn put(&self, _: &str, _: &[(String, String)], _: Vec<u8>) -> delivery::Transport {
        delivery::Transport::NotConnected { offline: false }
    }
}

// ---------------------------------------------------------------------------------------
// RICH WRITES IT
// ---------------------------------------------------------------------------------------

/// One printed answer from `claude`, or why not. `input` is the one stream-json line
/// (`bug::writer_input`): the prompt, and the window's picture when there is one. Bounded by
/// [`WRITE_DEADLINE`]; the child is killed when it outlasts it (CEO §54: whatever this app
/// starts, it ends).
fn ask_rich(app: &AppHandle, bin: &Path, folder: Option<&Path>, cwd: &Path, input: &str) -> Result<String, String> {
    let plugin = app.state::<crate::AppState>().engine_dir.lock().unwrap().join("app");
    richos_core::skills::verify_present(&plugin)
        .map_err(|(path, why)| format!("Rich's instructions are unavailable at {}: {why}", path.display()))?;
    let mut command = std::process::Command::new(bin);
    command
        .args(bug::writer_args())
        .arg("--plugin-dir").arg(&plugin)
        .args(["--agent", "rich-skills:rich"])
        .env_remove("CLAUDECODE")
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(folder) = folder {
        command.env("CLAUDE_CONFIG_DIR", folder);
    }
    let mut child = command.spawn().map_err(|e| format!("claude did not start: {e}"))?;
    // Written from its own thread: a picture is several hundred kilobytes, more than a pipe
    // holds, and `claude` reads it while this thread watches the deadline. Dropping the handle
    // when it is written closes standard input, which ends the one message.
    let writer = child.stdin.take().map(|mut stdin| {
        let input = input.to_string();
        std::thread::spawn(move || best_effort("the prompt", stdin.write_all(input.as_bytes())))
    });
    let mut stdout = child.stdout.take().ok_or("no output")?;
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        best_effort("its answer", stdout.read_to_string(&mut out));
        out
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < WRITE_DEADLINE => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                best_effort("stopping it", child.kill());
                best_effort("reaping it", child.wait());
                return Err("claude took too long".into());
            }
        }
    }
    if let Some(writer) = writer {
        writer.join().map_err(|_| "the writer stopped")?;
    }
    let out = reader.join().map_err(|_| "the reader stopped")?;
    bug::result_text(&out)
}

fn claude_for(state: &crate::AppState) -> (PathBuf, Option<PathBuf>) {
    let bin = state.claude_bin.lock().map(|b| b.clone()).unwrap_or_else(|_| richos_core::native::resolve_claude_bin());
    (bin, state.quota.lease_account().folder)
}

/// Everything RichOS holds that is private: the conversations' names (as he renamed them too),
/// the companies, the folders connected to them, and his own name.
fn private_terms(state: &crate::AppState) -> Vec<bug::PrivateTerm> {
    use bug::{Kind, PrivateTerm};
    let mut terms = Vec::new();
    let nav = state.nav.lock().map(|n| n.state().clone()).unwrap_or_default();
    for thread in state.reader.snapshot().threads() {
        terms.push(PrivateTerm::new(&thread.title, Kind::ConversationName));
        // His rename and Haiku's title are both made of his words.
        for name in [nav.renamed_threads.get(&thread.id), nav.auto_titles.get(&thread.id)].into_iter().flatten() {
            terms.push(PrivateTerm::new(name, Kind::ConversationName));
        }
    }
    for entity in state.reader.snapshot().entity_registry().entities() {
        terms.push(PrivateTerm::new(&entity.display_name, Kind::CompanyName));
        for repo in &entity.connected_repositories {
            if let Some(name) = repo.file_name().and_then(|n| n.to_str()) {
                terms.push(PrivateTerm::new(name, Kind::FolderName));
            }
        }
    }
    if let Some(name) = state.config.lock().ok().and_then(|c| c.user_name().map(str::to_string)) {
        terms.push(PrivateTerm::new(&name, Kind::PersonName));
        for part in name.split_whitespace().filter(|p| p.chars().count() >= 3) {
            terms.push(PrivateTerm::new(part, Kind::PersonName));
        }
    }
    if let Some(user) = std::env::var_os("HOME").and_then(|h| PathBuf::from(h).file_name().map(|n| n.to_string_lossy().to_string())) {
        if user.chars().count() >= 3 {
            terms.push(PrivateTerm::new(&user, Kind::PersonName));
        }
    }
    terms.retain(|t| !t.text.is_empty() && t.text != "New thread");
    terms
}

// ---------------------------------------------------------------------------------------
// THE COMMANDS
// ---------------------------------------------------------------------------------------

/// Who the report goes out as. What RichOS holds that is private is not handed to the window:
/// the card's heads-up asks `bug_report_private_in` instead (fourth review finding 2).
#[richos_command::command]
pub fn bug_report_context() -> serde_json::Value {
    serde_json::json!({
        "account": bug::Account::Reporting,
        "repository": bug::REPOSITORY,
    })
}

/// **THE CARD'S HEADS-UP**: which words of the report, as the user has changed them, are
/// private. Answered by the same scrubber, with the same private words, that cleans the report
/// (`bug::private_in_edit`), so the window keeps no second copy of the rules (fourth review
/// finding 2). `private` is the report's own private words so far, as `bug_report_change` takes
/// them. Decides nothing: the user may send anyway, having been told.
#[richos_command::command]
pub fn bug_report_private_in(state: State<crate::AppState>, text: String, private: Option<Vec<bug::PrivateTerm>>) -> Vec<String> {
    bug::private_in_edit(&text, &private_terms(&state), &private.unwrap_or_default())
}

/// **A PICTURE OF THE WINDOW AS IT IS NOW**, taken when Bust a bug is pressed and before the
/// exchange covers anything (second review finding 6). Kept in memory for Rich's write-up
/// only; the last one is dropped first, so a picture never outlives the press it was taken for.
/// `{ taken, bytes }`; a picture that could not be had is not an error, Rich gets the words.
#[richos_command::command]
pub async fn bug_report_look(app: AppHandle, window: tauri::WebviewWindow) -> Result<serde_json::Value, String> {
    let bugs = app.state::<Arc<BugReports>>().inner().clone();
    off_the_ipc_threads(move || {
        *bugs.look.lock().unwrap_or_else(|p| p.into_inner()) = None;
        let Some(jpeg) = crate::window_picture::take(&window) else {
            return Ok(serde_json::json!({ "taken": false, "bytes": 0 }));
        };
        use base64::Engine;
        let bytes = jpeg.len();
        let picture = bug::Picture { media_type: "image/jpeg".into(), base64: base64::engine::general_purpose::STANDARD.encode(&jpeg) };
        *bugs.look.lock().unwrap_or_else(|p| p.into_inner()) = Some(picture);
        Ok(serde_json::json!({ "taken": true, "bytes": bytes }))
    })
    .await
}

/// **Rich checks, then writes it up.** He is given the user's words, where they were, what was
/// on that screen (its words, `screen.content`) and the picture of the window taken when they
/// pressed the button, so he can check what they say against what they saw. All of it goes to
/// the user's own `claude` only; the issue is what he writes, scrubbed. Sends nothing.
///
/// Answers `bug::WriteUp` and `workedMs`: `{state: "checked", report, draft, digest}`, or, when
/// Claude fails, times out or answers with no report, `{state: "unchecked", id}`. **Then there is
/// no draft to offer**: the report is kept on this Mac and the check loop ([`spawn_retry`]) asks
/// Rich again until he answers (review rv-20261009T162841Z-69294215-70e6 finding 2: the plain
/// write-up that stood in for him put names no rule can know in a public issue). `report` is the
/// id his check is kept under for Send ([`bug_report_send`]).
#[richos_command::command]
pub async fn bug_report_write(app: AppHandle, answer: String, screen: bug::Screen, post: Option<bug::Post>) -> Result<serde_json::Value, String> {
    let started = Instant::now();
    let post = post.unwrap_or_default();
    let (scrubber, (bin, folder), bugs) = read_state(&app);
    // Rich's write-up waits on `claude` for up to WRITE_DEADLINE, so it waits on the blocking
    // pool and never on one of the threads the window's other commands are answered on.
    off_the_ipc_threads(move || {
        let picture = bugs.look.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let looked = picture.is_some() || !bug::screen_words(&screen.content).is_empty();
        let prompt = bug::writer_prompt_for(post, &answer, &screen, &bugs.version, scrubber.terms(), picture.is_some());
        let input = bug::writer_input(&prompt, picture.as_ref());
        // Rich writes it, the scanner goes over it, and Rich gets the last word (CEO §115).
        let last = |prompt: &str| ask_rich(&app, &bin, folder.as_deref(), &bugs.quiet_dir, &bug::writer_input(prompt, None));
        let check = bug::checked_for(post, ask_rich(&app, &bin, folder.as_deref(), &bugs.quiet_dir, &input), &screen, looked, &bugs.version, &scrubber, last);
        let write_up = bugs.outbox(post).keep_unless_checked(&answer, &screen, check, now_ms()).map_err(|e| {
            eprintln!("[richos] bug report: the report could not be kept on this Mac ({e})");
        "RichOS couldn't save the report on this Mac, so nothing was sent. Try sending the report again.".to_string()
        })?;
        let mut answer = serde_json::to_value(&write_up).map_err(|e| e.to_string())?;
        if let bug::WriteUp::Checked(checked) = &write_up {
            let report = bug::report_id();
            bugs.checks().insert(report.clone(), bug::Check::of(&checked.draft));
            answer["report"] = serde_json::json!(report);
        }
        answer["workedMs"] = serde_json::json!(started.elapsed().as_millis() as u64);
        Ok(answer)
    })
    .await
}

/// **A report Rich could not check is taken off this Mac**: the window has shown his check on a
/// card (the card is then the report, as any draft is), or the user canceled it. `false` when it
/// was already gone.
#[richos_command::command]
pub async fn bug_report_take_unchecked(app: AppHandle, id: String, post: Option<bug::Post>) -> Result<bool, String> {
    let bugs = app.state::<Arc<BugReports>>().inner().clone();
    off_the_ipc_threads(move || {
        bugs.outbox(post.unwrap_or_default()).take_unchecked(&id).map_err(|e| {
            eprintln!("[richos] bug report {id}: could not be taken off this Mac ({e})");
            "RichOS couldn't take the report off this Mac, so it's still saved here. Press Cancel report to try again.".to_string()
        })
    })
    .await
}

/// **A change said to Rich**: the sentence to add and where it goes. Sends nothing. `private` is
/// the report's own private words so far (the draft's, and every change's since); they go on
/// applying, with any Rich names in this change, and come back for the card to keep. When Rich
/// cannot be had, nothing is added: the user's words are not put in the report unchecked (review
/// rv-20261009T162841Z-69294215-70e6 finding 2, the same rule as `bug_report_write`), and the card
/// says to tell him again or change the words in place, where the heads-up reads them.
///
/// The scanner goes over what he adds; the card with it added is a changed card, so at Send it
/// goes through the scanner and then Rich, last (`bug::at_send`), like words changed by hand.
#[richos_command::command]
pub async fn bug_report_change(
    app: AppHandle,
    said: String,
    sheet: bug::Sheet,
    private: Option<Vec<bug::PrivateTerm>>,
    post: Option<bug::Post>,
) -> Result<serde_json::Value, String> {
    let started = Instant::now();
    let (scrubber, (bin, folder), bugs) = read_state(&app);
    let report_private = private.unwrap_or_default();
    off_the_ipc_threads(move || {
        let headings: Vec<String> = sheet.sections.iter().map(|s| s.heading.clone()).collect();
        let input = bug::writer_input(&bug::change_prompt_for(post.unwrap_or_default(), &sheet, &said), None);
        let change = ask_rich(&app, &bin, folder.as_deref(), &bugs.quiet_dir, &input)
            .and_then(|raw| bug::parse_change(&raw))
            .and_then(|c| if headings.contains(&c.section) { Ok(c) } else { Err(format!("no section {:?}", c.section)) })
            .map_err(|why| {
                eprintln!("[richos] bug report: Rich could not check the change ({why}); nothing was added");
                "RichOS couldn't check that change just now, so nothing was added. Submit the change again.".to_string()
            })?;
        // The scanner goes over his words now; Rich gets the last word on the changed card at Send
        // (`bug::at_send`), so nothing after him changes a word that is sent (CEO §115).
        let (add, private) = bug::scrub_change(&change, scrubber.terms(), &report_private);
        Ok(serde_json::json!({
            "section": change.section,
            "add": add,
            "private": private,
            "workedMs": started.elapsed().as_millis() as u64,
        }))
    })
    .await
}

/// What the two writing commands read before they wait: the private words, which `claude` and
/// which account, and the reports' state. Read from the app handle, so an async command holds no
/// borrowed state across its wait.
fn read_state(app: &AppHandle) -> (bug::Scrubber, (PathBuf, Option<PathBuf>), Arc<BugReports>) {
    let state = app.state::<crate::AppState>();
    (bug::Scrubber::new(private_terms(&state)), claude_for(&state), app.state::<Arc<BugReports>>().inner().clone())
}

/// Run `work` on the blocking pool and hand back its answer. A command that waits on a child
/// process, the keychain or the network must not hold one of the async runtime's worker threads,
/// where every other command of the window is answered.
async fn off_the_ipc_threads<T: Send + 'static>(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| {
        eprintln!("[richos] bug report: the work stopped ({e})");
        "Something went wrong on this Mac, and nothing was sent.".to_string()
    })?
}

/// The one-attempt-at-a-time lock. A thread that panicked while holding it leaves nothing
/// half-written (every write in the outbox is atomic), so the lock is taken back rather than
/// letting one panic stop every report on this Mac from ever being sent again.
fn one_at_a_time(bugs: &BugReports) -> std::sync::MutexGuard<'_, ()> {
    bugs.sending.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn deliver(bugs: &BugReports, post: bug::Post, attempt: impl FnOnce(&bug::Outbox) -> std::io::Result<Option<bug::Delivery>>) -> Result<Option<bug::Delivery>, String> {
    let _one_at_a_time = one_at_a_time(bugs);
    attempt(bugs.outbox(post)).map_err(|e| {
        eprintln!("[richos] bug report: the report could not be kept on this Mac ({e})");
        "RichOS couldn't save the report on this Mac, so nothing was sent. Try sending the report again.".to_string()
    })
}

/// **SEND: THE ONE PLACE A REPORT LEAVES**. `sheet` is the card's words at Send, `report` the id
/// Rich's last words of it are kept under. What goes is decided by `bug::at_send` alone and is
/// sent exactly as it decided; the scanner runs first and Rich last (CEO §115):
///
///   - the words are what Rich gave last: they are sent as they are; answers the
///     `bug::Delivery` (`{state: "sent" | "waiting", …}`);
///   - some word differs (the user changed it, by hand or by telling Rich): Send is held, the
///     scanner goes over the card and then Rich cleans it up, one `claude` turn. His version is
///     sent when it is the card's words exactly, and otherwise shown on the card first
///     (`{state: "checked", report, draft, digest}`), to send or not; when Claude cannot answer,
///     the words are kept on this Mac, unsent, and the check loop asks him again
///     (`{state: "unchecked", id}`), as with any report he could not check.
///
/// `screen` is where the report was started, kept with words waiting for his check.
#[richos_command::command]
pub async fn bug_report_send(app: AppHandle, report: Option<String>, sheet: bug::Sheet, screen: bug::Screen, post: Option<bug::Post>) -> Result<serde_json::Value, String> {
    let post = post.unwrap_or_default();
    let (scrubber, (bin, folder), bugs) = read_state(&app);
    off_the_ipc_threads(move || {
        let report = report.filter(|r| !r.is_empty()).unwrap_or_else(bug::report_id);
        let app_terms = scrubber.terms().to_vec();
        let check = bugs.checks().get(&report).cloned();
        let last = |prompt: &str| ask_rich(&app, &bin, folder.as_deref(), &bugs.quiet_dir, &bug::writer_input(prompt, None));
        match bug::at_send_for(post, &sheet, check.as_ref(), &app_terms, last) {
            bug::AtSend::Send(public) => {
                let relay = bugs.relay();
                let delivery = deliver(&bugs, post, |o| o.send(&public, &relay, &relay, now_ms()).map(Some))?.ok_or_else(|| "nothing was sent".to_string())?;
                if matches!(delivery, bug::Delivery::Sent(_)) {
                    bugs.checks().remove(&report);
                }
                serde_json::to_value(delivery).map_err(|e| e.to_string())
            }
            bug::AtSend::Show(his) => {
                bugs.checks().insert(report.clone(), bug::Check::of(&his.draft));
                Ok(serde_json::json!({ "state": "checked", "report": report, "draft": his.draft, "digest": his.digest }))
            }
            bug::AtSend::Unchecked(why) => {
                let private = check.map(|c| c.private().to_vec()).unwrap_or_default();
                let kept = bugs.outbox(post).keep_edited(bug::Edited { sheet, private }, &screen, &why, now_ms()).map_err(|e| {
                    eprintln!("[richos] bug report: the changed report could not be kept on this Mac ({e})");
                    "RichOS couldn't save the report on this Mac, so nothing was sent. Try sending the report again.".to_string()
                })?;
                serde_json::to_value(kept).map_err(|e| e.to_string())
            }
        }
    })
    .await
}

/// *Try now*. `None` when it is no longer waiting.
#[richos_command::command]
pub async fn bug_report_try_now(app: AppHandle, id: String, post: Option<bug::Post>) -> Result<Option<bug::Delivery>, String> {
    let bugs = app.state::<Arc<BugReports>>().inner().clone();
    off_the_ipc_threads(move || {
        let relay = bugs.relay();
        deliver(&bugs, post.unwrap_or_default(), |o| o.try_now(&id, &relay, &relay, now_ms()))
    })
    .await
}

/// *Cancel report*, and a waiting report that is being changed (a changed report is approved
/// again). Answers what became of it, CONFIRMED: `{state: "canceled"}` once its copy is off this
/// Mac, or `{state: "sent", number, …}` when it had already gone out. It waits behind a send
/// already in flight (the one-at-a-time lock, held across the request), so it waits off the IPC
/// threads, and what that send did is what it answers.
#[richos_command::command]
pub async fn bug_report_cancel(app: AppHandle, id: String, post: Option<bug::Post>) -> Result<bug::Withdrawn, String> {
    let bugs = app.state::<Arc<BugReports>>().inner().clone();
    off_the_ipc_threads(move || {
        let _one_at_a_time = one_at_a_time(&bugs);
        bugs.outbox(post.unwrap_or_default()).withdraw(&id).map_err(|e| {
            eprintln!("[richos] bug report {id}: could not be taken off this Mac ({e}); it still waits");
            "RichOS couldn't cancel the report, so it's still waiting to send. Press Cancel report to try again.".to_string()
        })
    })
    .await
}

/// While a report waits on the user's words and voice is on, hold spoken words for it.
#[richos_command::command]
pub fn bug_report_voice(bugs: State<Arc<BugReports>>, on: bool) -> bool {
    bugs.hold_voice.store(on, Ordering::SeqCst);
    on
}

/// Open issue `number` of RichOS's repository in the browser, or, for a posted idea
/// (`post: "idea"`), discussion `number`. A number, never an address.
#[richos_command::command]
pub fn bug_report_open_issue(number: u64, post: Option<bug::Post>) -> Result<(), String> {
    let (page, what) = match post.unwrap_or_default() {
        bug::Post::Bug => (bug::issue_page(number), "the issue"),
        bug::Post::Idea => (bug::discussion_page(number), "your idea"),
    };
    let status = std::process::Command::new("/usr/bin/open").arg(&page).status().map_err(|e| format!("this Mac would not open it: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("This Mac wouldn't open {what}. It's at {}.", page.trim_start_matches("https://")))
    }
}

/// **SENT BY ITSELF.** Every [`RETRY_EVERY`], each waiting report whose rest is over is tried
/// once, and one that goes out is said on [`EVENT`] so the card, Rich and a notice say so.
/// Runs from launch, so a report kept before a quit goes out after the next launch.
pub fn spawn_retry(app: AppHandle) {
    let app_for_check = app.clone();
    std::thread::Builder::new()
        .name("bug-report-retry".into())
        .spawn(move || loop {
            std::thread::sleep(RETRY_EVERY);
            let Some(bugs) = app.try_state::<Arc<BugReports>>() else { continue };
            let bugs = bugs.inner().clone();
            let relay = bugs.relay();
            // Bug reports and feature ideas alike: a waiting idea is posted by itself as a waiting
            // report is sent (round 24: "it posts by itself when back online").
            for post in [bug::Post::Bug, bug::Post::Idea] {
                let guard = one_at_a_time(&bugs);
                let due = bugs.outbox(post).send_due(&relay, &relay, now_ms());
                drop(guard);
                match due {
                    Ok(deliveries) => {
                        for delivery in deliveries.into_iter().filter(|d| matches!(d, bug::Delivery::Sent(_))) {
                            if let Err(e) = app.emit(EVENT, serde_json::json!({ "delivery": delivery, "post": post })) {
                                eprintln!("[richos] bug report: the window could not be told it went out: {e}");
                            }
                        }
                    }
                    Err(e) => eprintln!("[richos] bug report: the waiting {post:?} posts could not be read ({e})"),
                }
            }
        })
        .map(|_| ())
        .unwrap_or_else(|e| eprintln!("[richos] bug report: the retry loop did not start ({e}); a waiting report goes out on Try now"));
    spawn_check(app_for_check);
}

/// **RICH IS ASKED AGAIN, BY HIMSELF.** Every [`RETRY_EVERY`], each report he could not check
/// (`bug_report_write`'s `unchecked`) whose rest is over is given to him once more, with the
/// private words RichOS holds now; one he checks is said on [`EVENT`] as `{ checked: { id, draft,
/// digest, here } }`, and again on every pass until the window takes it (`bug_report_take_unchecked`),
/// so a window closed or reloaded meanwhile still gets it. Nothing here sends: the card does,
/// once the user approves it. Its own thread, because one `claude` turn can take
/// [`WRITE_DEADLINE`] and a waiting report must not wait behind it. Runs from launch, so a report
/// kept before a quit is checked after the next launch. The picture of the window is not given
/// again: it is never written to disk, so Rich has the screen's words.
fn spawn_check(app: AppHandle) {
    std::thread::Builder::new()
        .name("bug-report-check".into())
        .spawn(move || loop {
            std::thread::sleep(RETRY_EVERY);
            let Some(bugs) = app.try_state::<Arc<BugReports>>() else { continue };
            let bugs = bugs.inner().clone();
            // Bug reports and feature ideas alike, each asked about as what it is.
            for post in [bug::Post::Bug, bug::Post::Idea] {
                check_once(&app, &bugs, post);
            }
        })
        .map(|_| ())
        .unwrap_or_else(|e| eprintln!("[richos] bug report: the check loop did not start ({e}); a report Rich could not check waits for the next launch"));
}

/// One pass of the check loop over the bug reports, or the feature ideas, Rich could not check.
fn check_once(app: &AppHandle, bugs: &Arc<BugReports>, post: bug::Post) {
    let outbox = bugs.outbox(post);
    // Nothing to read or ask while none waits.
    let waiting = match outbox.unchecked() {
        Ok(waiting) if waiting.is_empty() => return,
        Ok(waiting) => waiting,
        Err(e) => {
            eprintln!("[richos] bug report: the {post:?} posts Rich has not checked could not be read ({e})");
            return;
        }
    };
    let (scrubber, (bin, folder), _) = read_state(app);
    // The scanner first, Rich's last word on what it made (CEO §115).
    let last = |prompt: &str| ask_rich(app, &bin, folder.as_deref(), &bugs.quiet_dir, &bug::writer_input(prompt, None));
    let checked = outbox.check_due(now_ms(), |u| match &u.edited {
        // Words the user changed that he could not check at Send: those, scanned, are his.
        Some(edited) => bug::checked_edit_for(post, edited, scrubber.terms(), last),
        None => {
            let prompt = bug::writer_prompt_for(post, &u.answer, &u.screen, &bugs.version, scrubber.terms(), false);
            let rich = ask_rich(app, &bin, folder.as_deref(), &bugs.quiet_dir, &bug::writer_input(&prompt, None));
            bug::checked_for(post, rich, &u.screen, !bug::screen_words(&u.screen.content).is_empty(), &bugs.version, &scrubber, last)
        }
    });
    // Where the user was, in their own words, for the panel's header when no card waits for it.
    let here = |id: &str| waiting.iter().find(|u| u.id == id).map(|u| u.screen.here.clone()).unwrap_or_default();
    match checked {
        Ok(ready) => {
            for (id, done) in ready {
                // His check is kept under the report's own id for Send, once: a pass that
                // says it again must not undo a change he wrote into it since.
                bugs.checks().entry(id.clone()).or_insert_with(|| bug::Check::of(&done.draft));
                let payload = serde_json::json!({ "checked": { "id": id, "report": id, "draft": done.draft, "digest": done.digest, "here": here(&id), "post": post } });
                if let Err(e) = app.emit(EVENT, payload) {
                    eprintln!("[richos] bug report: the window could not be told Rich checked it: {e}");
                }
            }
        }
        Err(e) => eprintln!("[richos] bug report: the {post:?} posts Rich has not checked could not be kept ({e})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    /// One request as the stand-in read it: its request line, its headers and its body.
    type Seen = (String, Vec<String>, String);

    /// A stand-in for RichOS's reporting relay on this machine that answers `replies.len()`
    /// requests in turn, each with its status and body, and hands back every request it was sent,
    /// so a test reads exactly what would have gone to the relay.
    fn stand_in_answering(replies: Vec<(&'static str, &'static str)>) -> (String, std::thread::JoinHandle<Vec<Seen>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, body) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut headers = Vec::new();
                let mut length = 0usize;
                loop {
                    let mut h = String::new();
                    reader.read_line(&mut h).unwrap();
                    let h = h.trim_end().to_string();
                    if h.is_empty() {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                    headers.push(h);
                }
                let mut buf = vec![0u8; length];
                reader.read_exact(&mut buf).unwrap();
                let reply = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                stream.write_all(reply.as_bytes()).unwrap();
                seen.push((line.trim_end().to_string(), headers, String::from_utf8(buf).unwrap()));
            }
            seen
        });
        (base, handle)
    }

    const TEST_REPOSITORY: &str = "richos-test/report-relay-local";

    fn relay_at(base: &str) -> ReportingRelay {
        ReportingRelay {
            destination: Ok(Destination { relay: base.to_string(), repository: TEST_REPOSITORY.into() }),
            installation: "0b6f3c2e-5d41-4a8e-9f27-3c1d2e4f5a6b".into(),
        }
    }

    /// An outbox of its own, in a folder of its own, removed when the test ends.
    struct Folder(PathBuf);
    impl Drop for Folder {
        fn drop(&mut self) {
            best_effort("the test folder", std::fs::remove_dir_all(&self.0));
        }
    }
    fn folder() -> Folder {
        let dir = std::env::temp_dir().join(format!("richos-bug-relay-{}", bug::report_id()));
        std::fs::create_dir_all(&dir).unwrap();
        Folder(dir)
    }

    /// Words the user approved on a card, as `at_send` lets them leave.
    fn approved(title: &str, text: &str) -> bug::Public {
        let plain = |t: &str| vec![bug::Segment { text: t.into(), was: None, kind: None }];
        let draft = bug::Draft {
            title: plain(title),
            sections: vec![bug::DraftSection { heading: "What happened".into(), paragraphs: vec![plain(text)], steps: vec![] }],
            private: vec![],
        };
        let bug::AtSend::Send(public) = bug::at_send(&bug::sheet_of(&draft), Some(&bug::Check::of(&draft)), &[], |_| panic!("Rich was asked about words he gave")) else {
            panic!("not sendable")
        };
        public
    }

    fn footer_id(body: &str) -> String {
        let last = body.lines().last().unwrap();
        let id = last.rsplit(' ').next().unwrap().to_string();
        assert_eq!(last, richdoctor_core::report::footer(&id), "the body ends with the relay's footer line");
        assert!(delivery::is_report_id(&id), "{id}");
        id
    }

    /// **BUST A BUG WORKS ON ANY MAC** (CEO 2026-10-25, "Is Bust a bug working or is it not
    /// working???"): an approved report goes through RichOS's reporting relay, with no token on
    /// this Mac and none in the request, and comes back as its issue's number and link. Red on the
    /// build that sent only with a keychain token, which no Mac but one set up by hand had.
    #[test]
    fn an_approved_report_goes_through_the_relay_and_comes_back_as_its_issue_with_no_token_on_this_mac() {
        let link = "https://github.com/richos-test/report-relay-local/issues/7";
        let (base, server) = stand_in_answering(vec![
            ("200 OK", r#"{"state": "ready", "stage": "ready", "stored": [], "missing": []}"#),
            ("201 Created", r#"{"state": "delivered", "stage": "delivered", "issue_url": "https://github.com/richos-test/report-relay-local/issues/7", "issue_number": 7, "images_expected": 0, "images_confirmed": 0}"#),
        ]);
        let dir = folder();
        let outbox = bug::Outbox::open(&dir.0);
        let public = approved("Names are cut off", "It broke.");
        let relay = relay_at(&base);
        let delivery = outbox.send(&public, &relay, &relay, 1).unwrap();
        let bug::Delivery::Sent(sent) = &delivery else { panic!("not sent: {delivery:?}") };
        assert_eq!((sent.number, sent.url.as_str(), &sent.account), (7, link, &bug::Account::Reporting));
        assert!(outbox.pending().unwrap().is_empty(), "nothing left waiting");
        let seen = server.join().unwrap();
        let register: serde_json::Value = serde_json::from_str(&seen[0].2).unwrap();
        let id = footer_id(register["body"].as_str().unwrap());
        assert_eq!(seen[0].0, "POST /v1/reports HTTP/1.1");
        assert_eq!(seen[1].0, format!("POST /v1/reports/{id}/complete HTTP/1.1"));
        for (_, headers, _) in &seen {
            assert!(!headers.iter().any(|h| h.to_ascii_lowercase().starts_with("authorization:")), "no token goes: {headers:?}");
        }
        assert_eq!(register["schema"], "richdoctor-report/2");
        assert_eq!(register["title"], public.title());
        assert_eq!(register["body"].as_str().unwrap(), format!("{}\n\n{}", public.body().trim_end(), richdoctor_core::report::footer(&id)), "the approved words, then the footer");
        assert_eq!(register["manifest"], serde_json::json!([]), "no screenshot");
    }

    /// **REQUEST A FEATURE WORKS ON ANY MAC TOO** (CEO 2026-10-25, "So, if Bust a Bug was broken,
    /// then the feature suggestion is also broken"): an idea goes through the same relay as
    /// `richos-idea/1`, never GitHub's GraphQL from this Mac, and comes back as its discussion.
    #[test]
    fn an_approved_idea_goes_through_the_relay_and_comes_back_as_its_discussion() {
        let (base, server) = stand_in_answering(vec![
            ("200 OK", r#"{"state": "ready", "stored": [], "missing": []}"#),
            ("201 Created", r#"{"state": "delivered", "issue_url": "https://github.com/richos-test/report-relay-local/discussions/8", "issue_number": 8}"#),
        ]);
        let dir = folder();
        let outbox = bug::Outbox::open_for(&dir.0, bug::Post::Idea);
        let relay = relay_at(&base);
        let delivery = outbox.send(&approved("Pin a conversation", "Keep one on top."), &relay, &relay, 1).unwrap();
        let bug::Delivery::Sent(sent) = &delivery else { panic!("not posted: {delivery:?}") };
        assert_eq!((sent.number, sent.url.as_str(), sent.post), (8, "https://github.com/richos-test/report-relay-local/discussions/8", bug::Post::Idea));
        let seen = server.join().unwrap();
        assert_eq!(seen.len(), 2);
        let register: serde_json::Value = serde_json::from_str(&seen[0].2).unwrap();
        assert_eq!(register["schema"], delivery::IDEA_SCHEMA);
        footer_id(register["body"].as_str().unwrap());
        assert!(seen.iter().all(|(line, _, _)| line.starts_with("POST /v1/reports")), "{seen:?}");
    }

    /// What does not reach the relay waits, saved, with the reason the card already says: a relay
    /// nothing answers is this Mac offline; one that refuses the words is a refusal; one closed
    /// for now is busy, tried again in a few minutes.
    #[test]
    fn a_report_the_relay_does_not_file_waits_with_its_reason() {
        let dir = folder();
        let outbox = bug::Outbox::open(&dir.0);
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let down = relay_at(&format!("http://127.0.0.1:{port}"));
        let waiting = outbox.send(&approved("t", "b"), &down, &down, 1).unwrap();
        assert!(matches!(waiting, bug::Delivery::Waiting { reason: bug::Reason::Offline, .. }), "{waiting:?}");
        for (reply, reason) in [
            (("422 Unprocessable Entity", r#"{"state": "rejected", "reason": "content", "detail": "It contains an @ mention."}"#), bug::Reason::Refused),
            (("503 Service Unavailable", r#"{"state": "closed", "reason": "daily_cap", "retry_after_seconds": 3600}"#), bug::Reason::GithubDown),
        ] {
            let (base, server) = stand_in_answering(vec![reply]);
            let relay = relay_at(&base);
            let d = outbox.send(&approved("t", "b"), &relay, &relay, 1).unwrap();
            assert!(matches!(d, bug::Delivery::Waiting { reason: r, .. } if r == reason), "{d:?}");
            server.join().unwrap();
        }
        assert_eq!(outbox.pending().unwrap().len(), 3, "every one is kept on this Mac");
        let unset = ReportingRelay { destination: Destination::from_env(|k| (k == delivery::RELAY_ENV).then(|| "http://127.0.0.1:9".to_string())), installation: String::new() };
        assert!(matches!(outbox.send(&approved("t", "b"), &unset, &unset, 1).unwrap(), bug::Delivery::Waiting { reason: bug::Reason::Refused, .. }), "half a test override sends nothing");
    }

    #[test]
    fn the_installation_id_is_made_once_and_kept() {
        let dir = folder();
        let first = installation_id(&dir.0);
        assert!(uuid_shaped(&first), "{first}");
        assert_eq!(installation_id(&dir.0), first);
    }

    #[test]
    fn no_token_is_in_the_source() {
        // No token is on this Mac at all: the relay's GitHub App files the report.
        let src = include_str!("bug_report.rs");
        for prefix in [concat!("gh", "p_"), concat!("github", "_pat_"), concat!("gh", "o_")] {
            assert!(!src.contains(prefix), "a GitHub token prefix is in the source: {prefix}");
        }
        assert!(!src.contains(concat!("find-generic", "-password")), "nothing reads a token from the keychain");
    }
}
