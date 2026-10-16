//! **BUST A BUG** — the shell's half (CEO §115, round 21). Everything that decides what reaches
//! GitHub is `richos_core::bug_report` and is tested there; this file is the three things that
//! need the Mac: running Rich (`claude`) to write the report, with a picture of the window
//! (`window_picture.rs`) and the screen's words to check against, reading the reporting
//! account's token from the login keychain, and the HTTPS request. Plus the ten commands the
//! window calls, the loop that sends a waiting report by itself and the one that asks Rich again
//! about a report he could not check yet.
//!
//! # Which account, today
//!
//! RichOS has no GitHub sign-in yet (§20 is not built), so [`KeychainCredentials`] answers the
//! RichOS reporting account, with the token at `KEYCHAIN_SERVICE` / `KEYCHAIN_ACCOUNT` in the
//! login keychain, read at send time. The user's own account slots in there, as
//! `Account::User`, when a sign-in exists; the window's From line and Rich's sentences already
//! read the account off the context and the delivery rather than assume it.
//!
//! # Request a feature (round 24, CEO 2026-10-11)
//!
//! *"can things not be copy-pasted from the "Bust a bug" feature?"*: a feature idea goes through
//! every command and loop here exactly as a bug report does, named by the window's `post`
//! (`"idea"`; a call without it is a bug report). Its outbox is its own folder
//! (`feature-requests/`), so a kept idea can only ever be posted as an idea, and the one
//! difference is where it is posted: GraphQL's `createDiscussion` under Ideas ([`GitHub`]'s
//! `graphql`, `richos_core::bug_report::post_discussion`), never the issues endpoint.
//!
//! # What is NOT here: a shipped token
//!
//! No token is in the source or the bundle. On a Mac where none has been put in the keychain the
//! report waits, saved, with Rich saying the reporting account is not set up on this Mac.

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
/// A keychain read on an unlocked keychain answers in milliseconds; this bounds a dialog nobody
/// answers (`phone/secrets.rs` measured that `security` can raise one).
const KEYCHAIN_DEADLINE: Duration = Duration::from_secs(20);

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
}

impl BugReports {
    pub fn for_app(data_dir: &Path, app_version: &str) -> Arc<Self> {
        let dir = data_dir.join("bug-reports");
        let quiet_dir = dir.join("writing");
        best_effort("the writing folder", std::fs::create_dir_all(&quiet_dir));
        Arc::new(BugReports {
            outbox: bug::Outbox::open(&dir),
            ideas: bug::Outbox::open_for(&data_dir.join("feature-requests"), bug::Post::Idea),
            sending: Mutex::new(()),
            hold_voice: AtomicBool::new(false),
            quiet_dir,
            version: bug::version_line(app_version, macos_version().as_deref(), std::env::consts::ARCH),
            look: Mutex::new(None),
            checks: Mutex::new(std::collections::HashMap::new()),
        })
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
// WHO IT GOES OUT AS
// ---------------------------------------------------------------------------------------

/// The RichOS reporting account's token, read from the login keychain at send time.
pub struct KeychainCredentials;

impl bug::Credentials for KeychainCredentials {
    fn credential(&self) -> Option<bug::Credential> {
        let token = read_keychain(bug::KEYCHAIN_SERVICE, bug::KEYCHAIN_ACCOUNT)?;
        Some(bug::Credential { account: bug::Account::Reporting, token })
    }
}

fn read_keychain(service: &str, account: &str) -> Option<String> {
    let mut child = std::process::Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut out = String::new();
                child.stdout.take()?.read_to_string(&mut out).ok()?;
                let token = out.trim().to_string();
                return (!token.is_empty()).then_some(token);
            }
            Ok(None) if started.elapsed() < KEYCHAIN_DEADLINE => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                best_effort("stopping it", child.kill());
                best_effort("reaping it", child.wait());
                eprintln!("[richos] bug report: the keychain did not answer; the report waits");
                return None;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// THE REQUEST
// ---------------------------------------------------------------------------------------

/// `POST /repos/WebDevBooster/richos/issues` for a bug report, and `POST /graphql` for a feature
/// idea, to GitHub or to the stand-in named by `RICHOS_BUG_REPORT_API`.
pub struct GitHub {
    base: String,
}

impl GitHub {
    pub fn from_env() -> Self {
        let base = std::env::var(bug::API_OVERRIDE_ENV).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| bug::API_BASE.to_string());
        GitHub { base: base.trim_end_matches('/').to_string() }
    }
}

impl bug::Transport for GitHub {
    fn create_issue(&self, credential: &bug::Credential, path: &str, request: &serde_json::Value) -> bug::Outcome {
        crate::voice_provision::ensure_crypto_provider();
        let url = format!("{}{}", self.base, path);
        let body = request.to_string();
        let token = credential.token.clone();
        // ITS OWN THREAD AND ITS OWN RUNTIME, because of where it is called from: a
        // `#[tauri::command(async)]` runs on one of the async runtime's worker threads, and
        // blocking on a runtime there panics ("Cannot start a runtime from within a runtime"),
        // which left the card on Sending… for good in walk-aba5c01c19ce. A plain thread has no
        // runtime around it, so the request runs the same from a command, the retry loop or a test.
        let request = std::thread::spawn(move || -> bug::Outcome {
            let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(r) => r,
                Err(_) => return bug::Outcome::Unreachable,
            };
            runtime.block_on(Self::post(url, token, body))
        });
        request.join().unwrap_or(bug::Outcome::Unreachable)
    }

    /// One GraphQL request, `POST {base}/graphql`: a feature idea's lookup and its
    /// `createDiscussion` (`bug::post_discussion`). Its own thread and runtime, as `create_issue`.
    fn graphql(&self, credential: &bug::Credential, request: &serde_json::Value) -> bug::Reply {
        crate::voice_provision::ensure_crypto_provider();
        let url = format!("{}/graphql", self.base);
        let body = request.to_string();
        let token = credential.token.clone();
        let request = std::thread::spawn(move || -> bug::Reply {
            let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(r) => r,
                Err(_) => return bug::Reply::Unreachable,
            };
            runtime.block_on(Self::graphql_post(url, token, body))
        });
        request.join().unwrap_or(bug::Reply::Unreachable)
    }
}

impl GitHub {
    async fn graphql_post(url: String, token: String, body: String) -> bug::Reply {
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(25))
            .connect_timeout(Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::none())
            .build()
        {
            Ok(c) => c,
            Err(_) => return bug::Reply::Unreachable,
        };
        let sent = client
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header(reqwest::header::USER_AGENT, "RichOS")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await;
        let response = match sent {
            Ok(r) => r,
            Err(e) if e.is_timeout() => return bug::Reply::TimedOut,
            Err(_) => return bug::Reply::Unreachable,
        };
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        bug::Reply::Answered { status, body: serde_json::from_str(&text).unwrap_or(serde_json::Value::Null) }
    }

    async fn post(url: String, token: String, body: String) -> bug::Outcome {
        {
            let client = match reqwest::Client::builder()
                .timeout(Duration::from_secs(25))
                .connect_timeout(Duration::from_secs(8))
                .redirect(reqwest::redirect::Policy::none())
                .build()
            {
                Ok(c) => c,
                Err(_) => return bug::Outcome::Unreachable,
            };
            let sent = client
                .post(&url)
                .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(reqwest::header::ACCEPT, "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header(reqwest::header::USER_AGENT, "RichOS")
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body)
                .send()
                .await;
            let response = match sent {
                Ok(r) => r,
                Err(e) if e.is_timeout() => return bug::Outcome::TimedOut,
                Err(_) => return bug::Outcome::Unreachable,
            };
            let code = response.status().as_u16();
            if code != 201 {
                return bug::Outcome::Status { code };
            }
            let text = response.text().await.unwrap_or_default();
            let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            match value["number"].as_u64() {
                Some(number) => bug::Outcome::Created { number, url: value["html_url"].as_str().map(str::to_string).unwrap_or_else(|| bug::issue_page(number)) },
                // A 201 with no issue number is not an issue we can point at: treat it as GitHub
                // not answering properly, so the report waits rather than claiming it was filed.
                None => bug::Outcome::Status { code: 502 },
            }
        }
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
    let renamed = state.nav.lock().map(|n| n.state().renamed_threads.clone()).unwrap_or_default();
    for thread in state.reader.snapshot().threads() {
        terms.push(PrivateTerm::new(&thread.title, Kind::ConversationName));
        if let Some(name) = renamed.get(&thread.id) {
            terms.push(PrivateTerm::new(name, Kind::ConversationName));
        }
    }
    if let Ok(registry) = state.registry.lock() {
        for entity in registry.entities() {
            terms.push(PrivateTerm::new(&entity.display_name, Kind::CompanyName));
            for repo in &entity.connected_repositories {
                if let Some(name) = repo.file_name().and_then(|n| n.to_str()) {
                    terms.push(PrivateTerm::new(name, Kind::FolderName));
                }
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
            "I couldn't check the report or save it on this Mac, so nothing was written up or sent. Tell me again and I'll try once more.".to_string()
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
            "I couldn't take the report off this Mac, so it's still saved here. Press Cancel report to try again.".to_string()
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
                "I couldn't check that change just now, so nothing was added.".to_string()
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
        "I couldn't save the report on this Mac, so nothing was sent.".to_string()
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
                let github = GitHub::from_env();
                let delivery = deliver(&bugs, post, |o| o.send(&public, &KeychainCredentials, &github, now_ms()).map(Some))?.ok_or_else(|| "nothing was sent".to_string())?;
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
                    "I couldn't save the report on this Mac, so nothing was sent. Press Send report to try again.".to_string()
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
        let github = GitHub::from_env();
        deliver(&bugs, post.unwrap_or_default(), |o| o.try_now(&id, &KeychainCredentials, &github, now_ms()))
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
            "I couldn't take the report off this Mac, so it's still waiting to send.".to_string()
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
            let github = GitHub::from_env();
            // Bug reports and feature ideas alike: a waiting idea is posted by itself as a waiting
            // report is sent (round 24: "it posts by itself when back online").
            for post in [bug::Post::Bug, bug::Post::Idea] {
                let guard = one_at_a_time(&bugs);
                let due = bugs.outbox(post).send_due(&KeychainCredentials, &github, now_ms());
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
    use bug::Transport;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    /// A stand-in for GitHub's issues endpoint on this machine: answers one request with
    /// `status` and `body`, and hands back what it was sent (the request line, the headers and
    /// the body) so the test can read exactly what would have gone to GitHub.
    fn stand_in(status: &'static str, body: &'static str) -> (String, std::thread::JoinHandle<(String, Vec<String>, String)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
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
            (line.trim_end().to_string(), headers, String::from_utf8(buf).unwrap())
        });
        (base, handle)
    }

    fn credential() -> bug::Credential {
        bug::Credential { account: bug::Account::Reporting, token: "stand-in-token".into() }
    }

    #[test]
    fn the_request_is_one_post_to_the_issues_endpoint_with_the_token_and_the_sheet() {
        let (base, server) = stand_in("201 Created", r#"{"number": 412, "html_url": "https://github.com/WebDevBooster/richos/issues/412"}"#);
        let plain = |t: &str| vec![bug::Segment { text: t.into(), was: None, kind: None }];
        let draft = bug::Draft {
            title: plain("Names are cut off"),
            sections: vec![bug::DraftSection { heading: "What happened".into(), paragraphs: vec![plain("It broke.")], steps: vec![] }],
            private: vec![],
        };
        let bug::AtSend::Send(public) = bug::at_send(&bug::sheet_of(&draft), Some(&bug::Check::of(&draft)), &[], |_| panic!("Rich was asked about words he gave")) else { panic!("not sendable") };
        let (path, request) = bug::issue_request(&public);
        let outcome = GitHub { base }.create_issue(&credential(), &path, &request);
        assert_eq!(outcome, bug::Outcome::Created { number: 412, url: "https://github.com/WebDevBooster/richos/issues/412".into() });
        let (line, headers, body) = server.join().unwrap();
        assert_eq!(line, "POST /repos/WebDevBooster/richos/issues HTTP/1.1");
        assert!(headers.iter().any(|h| h.eq_ignore_ascii_case("authorization: Bearer stand-in-token")), "{headers:?}");
        assert!(headers.iter().any(|h| h.eq_ignore_ascii_case("accept: application/vnd.github+json")), "{headers:?}");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&body).unwrap(), request, "the body is the sheet's request, nothing added");
    }

    /// WHERE A COMMAND RUNS IT FROM: inside a runtime. A `#[tauri::command(async)]` runs on an
    /// async runtime's worker thread, and the first build blocked on a runtime there, which
    /// panicked and left the card on Sending… (walk-aba5c01c19ce's app log: "Cannot start a
    /// runtime from within a runtime"). Red on that build, green on its own thread.
    #[test]
    fn the_request_completes_when_it_is_made_from_inside_a_runtime_as_a_command_makes_it() {
        let (base, server) = stand_in("201 Created", r#"{"number": 7}"#);
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let outcome = runtime.block_on(async move {
            GitHub { base }.create_issue(&credential(), "/repos/WebDevBooster/richos/issues", &serde_json::json!({"title": "t", "body": "b"}))
        });
        assert_eq!(outcome, bug::Outcome::Created { number: 7, url: bug::issue_page(7) });
        server.join().unwrap();
    }

    #[test]
    fn a_github_that_answers_503_is_a_status_and_a_closed_port_is_unreachable() {
        let (base, server) = stand_in("503 Service Unavailable", "{}");
        let outcome = GitHub { base }.create_issue(&credential(), "/repos/WebDevBooster/richos/issues", &serde_json::json!({"title": "t", "body": "b"}));
        assert_eq!(outcome, bug::Outcome::Status { code: 503 });
        server.join().unwrap();
        // A port nothing listens on: bind one, learn it, close it.
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let outcome = GitHub { base: format!("http://127.0.0.1:{port}") }.create_issue(&credential(), "/x", &serde_json::json!({}));
        assert_eq!(outcome, bug::Outcome::Unreachable);
    }

    /// A stand-in GitHub that answers `replies.len()` requests in turn, each with its status and
    /// body, and hands back every request it was sent (the request line, the headers, the body).
    /// One request as the stand-in read it: its request line, its headers and its body.
    type Seen = (String, Vec<String>, String);

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

    /// **REQUEST A FEATURE GOES TO GRAPHQL, NEVER TO ISSUES** (CEO 2026-10-11): an idea, posted
    /// through the shell's own transport against a stand-in GitHub, is two `POST /graphql`
    /// requests (the lookup, then `createDiscussion` in the Ideas category) with the token.
    #[test]
    fn an_idea_is_two_graphql_requests_and_never_the_issues_endpoint() {
        let (base, server) = stand_in_answering(vec![
            ("200 OK", r#"{"data": {"repository": {"id": "R_1", "discussionCategory": {"id": "DIC_ideas"}}}}"#),
            ("200 OK", r#"{"data": {"createDiscussion": {"discussion": {"number": 413, "url": "https://github.com/WebDevBooster/richos/discussions/413"}}}}"#),
        ]);
        let outcome = bug::post_discussion(&GitHub { base }, &credential(), "An idea", "### The idea\n\nIt.\n");
        assert_eq!(outcome, bug::Outcome::Created { number: 413, url: "https://github.com/WebDevBooster/richos/discussions/413".into() });
        let seen = server.join().unwrap();
        assert_eq!(seen.len(), 2);
        for (line, headers, _) in &seen {
            assert_eq!(line, "POST /graphql HTTP/1.1");
            assert!(headers.iter().any(|h| h.eq_ignore_ascii_case("authorization: Bearer stand-in-token")), "{headers:?}");
        }
        let create: serde_json::Value = serde_json::from_str(&seen[1].2).unwrap();
        assert!(create["query"].as_str().unwrap().contains("createDiscussion"), "{create}");
        assert_eq!(create["variables"]["categoryId"], "DIC_ideas");
        assert_eq!(create["variables"]["repositoryId"], "R_1");
    }

    /// GitHub's GraphQL answer to a token that may not post in Discussions is HTTP 200 with a
    /// FORBIDDEN error: the shell hands it back as it came, and the idea is refused, not posted.
    #[test]
    fn a_forbidden_graphql_answer_is_refused() {
        let (base, server) = stand_in_answering(vec![(
            "200 OK",
            r#"{"data": {"repository": null}, "errors": [{"type": "FORBIDDEN", "message": "Resource not accessible by personal access token"}]}"#,
        )]);
        let outcome = bug::post_discussion(&GitHub { base }, &credential(), "t", "b");
        assert_eq!(bug::classify(&outcome), Err(bug::Reason::Refused), "{outcome:?}");
        server.join().unwrap();
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        assert_eq!(GitHub { base: format!("http://127.0.0.1:{port}") }.graphql(&credential(), &serde_json::json!({})), bug::Reply::Unreachable);
    }

    #[test]
    fn no_token_is_in_the_source() {
        // The reporting account's token is read from the keychain at send time, never shipped.
        let src = include_str!("bug_report.rs");
        for prefix in [concat!("gh", "p_"), concat!("github", "_pat_"), concat!("gh", "o_")] {
            assert!(!src.contains(prefix), "a GitHub token prefix is in the source: {prefix}");
        }
    }
}
