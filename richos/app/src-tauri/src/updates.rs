//! Desktop update checks and verified staging.
//!
//! The plugin verifies downloaded bytes with the configured minisign key. Its macOS
//! installer is never called because it may request administrator access. The user-only
//! installer stages under `~/Applications/.richos-updater` and the UI reports when the
//! update is ready for the next normal launch.
//!
//! A live update never replaces its running bundle or discards input. Startup activation
//! uses an exclusive session lease before any runtime work starts; every participating app
//! retains a shared lease for its full lifetime. The separate work verdict still hides the
//! download action while a turn or worker is active.
//!
//! Once an update this app downloaded is ready and the same work verdict is clear, the app
//! relaunches into it after a short notice (CEO feedback 2026-10-06, item 1); while work
//! runs it waits, and relaunches when the work ends. `update_relaunch.rs` holds the how.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};

use richos_core::work_gate::{self, Liveness, WorkSources, WorkVerdict};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

/// The one event the webview subscribes to. Same `rich://` family as every other live
/// channel in this shell (`app/STREAMING.md`).
pub const EVENT_UPDATE: &str = "rich://update";

/// The host in `tauri.conf.json`'s committed `plugins.updater.endpoints`.
///
/// `.invalid` is reserved by RFC 2606 and can never resolve, and that is the point: WHERE
/// RICHOS UPDATES ARE HOSTED IS NOT AN ENGINEERING DECISION and has not been made. A
/// plausible-looking hostname committed here would fail as a DNS error six months from now
/// and read as a bug; this fails as `UpdateState::Unconfigured`, which says the true thing
/// in the UI: no update server has been chosen yet.
pub const PLACEHOLDER_ENDPOINT_HOST: &str = "updates.richos.invalid";

/// Point this build at a different manifest, for a staging channel or for
/// `app/scripts/updater-e2e.sh`.
///
/// It is not a back door. The public key is compiled into the binary from
/// `tauri.conf.json`, and `Update::download` verifies every byte against it before a single
/// byte is installed — so an attacker who can set this variable can make the updater fetch
/// a manifest and can make it REFUSE, and cannot make it install anything we did not sign.
/// The endpoint in force is reported to the UI in every payload (`endpoint`), so a build
/// pointed somewhere unusual says so on screen rather than in a log nobody opens.
pub const ENDPOINT_ENV: &str = "RICHOS_UPDATE_ENDPOINT";

/// Headless end-to-end mode; see `app/scripts/updater-e2e.sh`.
pub const SELFTEST_ENV: &str = "RICHOS_UPDATE_SELFTEST";

// ---------------------------------------------------------------------------------------
// The view the webview renders
// ---------------------------------------------------------------------------------------

/// Why an update attempt stopped, in the CEO's language and in the operator's.
///
/// `kind` is the machine-readable discriminant the UI styles on; `headline` is the sentence
/// a non-technical CEO reads; `detail` is the vendor's own error text, kept verbatim so a
/// support conversation is not a game of telephone. The UI shows the headline always and
/// the detail behind a disclosure — an error that hides its cause is how a five-minute
/// diagnosis becomes a week.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub kind: &'static str,
    pub headline: String,
    pub detail: String,
}

impl Failure {
    /// Map a plugin error onto the five things that can actually go wrong, plus a catch-all.
    ///
    /// `tauri_plugin_updater::Error` is `#[non_exhaustive]`, so the wildcard arm is required
    /// rather than lazy — but it is deliberately LAST and deliberately named "other", so a
    /// new vendor variant surfaces as an unclassified failure with its text intact instead
    /// of being silently folded into a neighbour.
    pub fn classify(err: &tauri_plugin_updater::Error) -> Self {
        use tauri_plugin_updater::Error as E;
        let detail = err.to_string();
        let (kind, headline) = match err {
            // THE ONE THAT MATTERS. minisign said no: the bytes on the wire are not the
            // bytes we signed. There is no "retry" for this and the UI must not offer one.
            E::Minisign(_) | E::Base64(_) | E::SignatureUtf8(_) => (
                "signature",
                "This download was not signed by RichOS, so it was not installed.",
            ),
            E::Reqwest(e) if e.is_connect() || e.is_timeout() => (
                "offline",
                "RichOS could not reach the update server. Check your internet connection.",
            ),
            E::Reqwest(_) | E::Network(_) => (
                "network",
                "The download did not finish. Nothing was installed.",
            ),
            E::ReleaseNotFound
            | E::Serialization(_)
            | E::Semver(_)
            | E::TargetNotFound(_)
            | E::TargetsNotFound(_)
            | E::UnsupportedArch
            | E::UnsupportedOs => (
                "manifest",
                "The update server answered, but not with something RichOS understands.",
            ),
            E::Io(_)
            | E::AuthenticationFailed
            | E::FailedToDetermineExtractPath
            | E::TempDirNotFound
            | E::BinaryNotFoundInArchive
            | E::InvalidUpdaterFormat
            | E::PackageInstallFailed => (
                "install",
                "The update downloaded and verified, but could not be put in place.",
            ),
            E::EmptyEndpoints | E::InsecureTransportProtocol | E::UrlParse(_) => (
                "configuration",
                "This build of RichOS has no usable update server configured.",
            ),
            _ => ("other", "The update did not complete."),
        };
        Failure {
            kind,
            headline: headline.to_string(),
            detail,
        }
    }
}

/// Everything the update surface needs to draw itself, in one payload.
///
/// One flat struct rather than a tagged union because it is also the ANSWER to
/// `update_state`, and a UI that reconnects mid-download has to be able to paint the bar at
/// the right place from a single read.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateView {
    /// `unconfigured` | `idle` | `checking` | `upToDate` | `available` | `downloading`
    /// | `installing` | `ready` | `restarting` | `failed`
    pub state: &'static str,
    /// The version running right now, and the ONLY thing the window may ever call the version.
    ///
    /// ITS SOURCE IS `app/src-tauri/Cargo.toml`, not `tauri.conf.json` — this line used to say
    /// the opposite and `tauri.conf.json` has never carried a `version` key at all;
    /// `make-release.sh` refuses one outright ("it OVERRIDES Cargo.toml, so the version would
    /// be written in two places and the wrong one could win silently"), and tauri-utils then
    /// takes the number from the Cargo manifest. So: Cargo.toml -> `CARGO_PKG_VERSION` ->
    /// `package_info().version` -> here -> `ui/updates.js`, one hop each and no branch.
    ///
    /// WRITTEN ONCE, AT `init`, AND NEVER AGAIN. No arm of check, download or install touches
    /// it, so a manifest cannot rename the build the CEO is running — proven by
    /// `current_version_has_exactly_one_writer` below and by `ui/tests/updates.js` check 18.
    pub current_version: String,
    /// The version the manifest offers, once a check has found one.
    pub available_version: Option<String>,
    /// The manifest's `notes`, shown verbatim. Never invented.
    pub notes: Option<String>,
    /// The manifest's `pub_date`, RFC 3339, as the server sent it.
    pub pub_date: Option<String>,
    pub downloaded_bytes: u64,
    /// `None` when the server sent no `Content-Length` — in which case the UI shows bytes
    /// and NOT a percentage, because a progress bar with an invented denominator is a lie
    /// that looks like a measurement.
    pub total_bytes: Option<u64>,
    pub percent: Option<u8>,
    pub failure: Option<Failure>,
    /// The endpoint actually in force, so a build pointed at a staging manifest says so.
    pub endpoint: String,
    /// Whether that endpoint is the committed placeholder.
    pub endpoint_is_placeholder: bool,
    /// Wall-clock milliseconds of the last completed check, or `None` for "never checked".
    /// The CEO must be able to see THAT it checked, not only what it found.
    pub checked_at: Option<u64>,

    // ---- the work gate (CEO, 2026-09-05) ----------------------------------------------
    /// **RichOS is doing something, so the update must not act and must not be offered.**
    ///
    /// It is on the view rather than behind its own command for one reason: the view is
    /// already *"everything the update surface needs to draw itself, in one payload"*, and a
    /// surface that had to combine two payloads could paint an Install button from a fresh
    /// update state and a stale work state. There is one answer and it arrives with
    /// everything else.
    pub busy: bool,
    /// WHAT is running, in the CEO's words — "Rich is working on your last message.",
    /// "2 workers are still running." `None` when nothing is.
    ///
    /// A blocking verdict ALWAYS carries one (`work_gate`'s
    /// `no_blocking_verdict_is_ever_silent` walks all 27 combinations to prove it), because
    /// a control that vanishes with no explanation is indistinguishable from a broken one.
    pub busy_reason: Option<String>,
    /// What the gate could NOT establish, one clause each — rendered as well as the reason.
    ///
    /// *"Never claim to have waited for something you did not check."* The live case is an
    /// app with no compute lease: it has no session id, so it cannot see workers, so it says
    /// so instead of implying it looked.
    pub unchecked: Vec<String>,
    /// When this update first became something the CEO could act on — the moment the view
    /// entered `available`, carried through `ready`.
    ///
    /// **It exists because the gate can hide the control for a long time.** RichOS is built
    /// to run workers for stretches, so "ready since Tuesday" is honest and a silent
    /// indefinite wait is not. Cleared by `clear_attempt`, so a fresh check restarts it
    /// rather than ageing an update that was replaced.
    pub ready_since: Option<u64>,

    // ---- the way back (CEO, 2026-09-19) -----------------------------------------------
    /// **The version "Go back" would install, or `None` when there is no way back.**
    ///
    /// He runs the nightly as his daily driver, so "this one is bad, how do I get off it"
    /// has to be answerable from the same surface as "is there a new one" — and answerable
    /// with a VERSION, because a control that says "go back" without saying where is a
    /// control nobody presses.
    ///
    /// Read from this installation's own publication history (`rollback_target`), never
    /// from the server: it is the version this Mac was demonstrably running one update
    /// ago. `None` covers both empty answers — no record of an earlier version, and a
    /// record that points forwards because the last thing that happened WAS a rollback.
    pub rollback_version: Option<String>,
    /// Whether the thing waiting for the next launch is a rollback rather than an update.
    ///
    /// `ready` alone cannot say it: the two are the same exchange of two directories and
    /// only the receipt that authorized it tells them apart. The row's promise differs —
    /// "1.2.1 is ready for the next launch" against "RichOS will go back to 1.2.0 when you
    /// next open it" — so the flag travels with the state instead of being inferred by
    /// comparing two numbers on the way out.
    pub ready_is_rollback: bool,

    // ---- the relaunch (CEO feedback, 2026-10-06, item 1) ------------------------------
    /// **This app downloaded the update that is ready, so it relaunches into it as soon as
    /// nothing is running.** His words: *"the app should automatically re-launch immediately
    /// after downloading the new version. Avoiding/preventing a re-launch is only meant for
    /// when there are workers running."*
    ///
    /// Only an update staged BY THIS PROCESS sets it. A `ready` found by `check` was staged
    /// earlier and survived a launch that could not activate it (another RichOS session held
    /// the lease), so relaunching would meet the same lease and loop; that case keeps the
    /// next-launch sentence. A rollback never sets it: going back is a separate button with
    /// its own promise.
    pub restarts_when_idle: bool,
}

impl UpdateView {
    fn new(current_version: String, endpoint: String, placeholder: bool) -> Self {
        UpdateView {
            state: if placeholder { "unconfigured" } else { "idle" },
            current_version,
            available_version: None,
            notes: None,
            pub_date: None,
            downloaded_bytes: 0,
            total_bytes: None,
            percent: None,
            failure: None,
            endpoint,
            endpoint_is_placeholder: placeholder,
            checked_at: None,
            busy: false,
            busy_reason: None,
            unchecked: Vec::new(),
            ready_since: None,
            rollback_version: None,
            ready_is_rollback: false,
            restarts_when_idle: false,
        }
    }

    /// Reset everything a previous attempt left behind, keeping only what is still true.
    fn clear_attempt(&mut self) {
        self.available_version = None;
        self.notes = None;
        self.pub_date = None;
        self.downloaded_bytes = 0;
        self.total_bytes = None;
        self.percent = None;
        self.failure = None;
        // A new attempt is a new update. Carrying the old `ready_since` forward would age a
        // version that is no longer the one on offer, which is the one thing this field
        // exists to state accurately.
        self.ready_since = None;
        // The DIRECTION belongs to the attempt, not to the installation. `rollback_version`
        // deliberately survives: it is a fact about this Mac's history, and a check that
        // found nothing does not change where back is.
        self.ready_is_rollback = false;
        self.restarts_when_idle = false;
    }
}

/// The managed state: one view, plus the `Update` handle a successful check produced.
///
/// The handle is kept so that "install" does not re-check. Re-checking between the CEO
/// reading "0.1.1 is available" and pressing the button would mean the thing he agreed to
/// install is not necessarily the thing that gets installed.
pub struct Updates {
    view: Mutex<UpdateView>,
    pending: Mutex<Option<tauri_plugin_updater::Update>>,
    operation: AtomicBool,
    /// A relaunch notice is on screen; a second one must not start beside it.
    relaunching: AtomicBool,
}

impl Updates {
    pub fn snapshot(&self) -> UpdateView {
        self.view.lock().expect("update view lock").clone()
    }
}

/// Serialize checks and staging within this app instance. Activation belongs to startup
/// and is separately excluded by the session lease and publication file lock.
struct UpdateOperation<'a>(&'a AtomicBool);
impl<'a> UpdateOperation<'a> {
    fn acquire(active: &'a AtomicBool) -> Option<Self> {
        active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(active))
    }
}
impl Drop for UpdateOperation<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
/// The headline when the update record stayed busy for the updater's whole wait.
const BUSY_HEADLINE: &str = "RichOS is busy with another update task. Try again in a minute.";
/// The headline for every other update-record or staging error.
const INSTALL_HEADLINE: &str = "RichOS could not prepare this update.";

/// AN ERROR FROM THE UPDATE RECORD (`~/Applications/.richos-updater`), as the CEO reads it.
///
/// `WouldBlock` is the record being BUSY: another part of RichOS (the launch check, the
/// window's own read, another copy of the app, a staging) held its lock for the whole of
/// `richos_user_update`'s wait. Nothing failed and nothing was being prepared, so it is said
/// as busy, with the same Try again control. Until 2026-10-21 every error here became
/// "RichOS could not prepare this update." with "Resource temporarily unavailable (os error
/// 35)" behind it, which a new user saw on a plain check. Every other error is still the
/// install failure it always was: the `failed` state with kind `install`.
fn journal_failure(app: &AppHandle, error: std::io::Error) -> UpdateView {
    let failure = Failure::of_journal(&error);
    transition(app, |v| {
        v.state = "failed";
        v.failure = Some(failure);
    })
}

impl Failure {
    /// [`journal_failure`]'s decision, pure so it is testable without a running app.
    fn of_journal(error: &std::io::Error) -> Self {
        let (kind, headline) = if error.kind() == std::io::ErrorKind::WouldBlock {
            ("busy", BUSY_HEADLINE)
        } else {
            ("install", INSTALL_HEADLINE)
        };
        Failure { kind, headline: headline.into(), detail: error.to_string() }
    }
}

// ---------------------------------------------------------------------------------------
// Wiring
// ---------------------------------------------------------------------------------------

/// The endpoints this process will try, IN ORDER, and whether they are the committed
/// placeholder.
///
/// THE GITHUB MOVE (CEO, 2026-10-27): the shipping config lists RichOS's new address first and
/// its old one second (`richos_core::github_home`), and [`first_answer`] asks them in that order.
/// Before the move the new address does not exist and the old one answers; after it the new one
/// answers. The plugin is handed one address at a time so this file knows WHICH one answered:
/// that is the endpoint the view reports, the one a rollback derives its tag manifest from, and
/// the one that decides where the download is fetched ([`richos_core::github_home::download_at`]).
///
/// Read once per call rather than cached, so `updater-e2e.sh` can start two processes from
/// the same bundle with different endpoints.
fn resolve_endpoints(app: &AppHandle) -> (Vec<String>, bool) {
    if let Ok(env) = std::env::var(ENDPOINT_ENV) {
        let env = env.trim().to_string();
        if !env.is_empty() {
            let placeholder = env.contains(PLACEHOLDER_ENDPOINT_HOST);
            return (vec![env], placeholder);
        }
    }
    // Fall back to what the config declares. `tauri::Config` does not expose plugin config
    // as a typed value, so this reads the same JSON the plugin reads.
    let configured: Vec<String> = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|v| v.get("endpoints"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).map(str::to_string).collect())
        .unwrap_or_default();
    let placeholder = configured.is_empty() || configured.iter().any(|e| e.contains(PLACEHOLDER_ENDPOINT_HOST));
    (configured, placeholder)
}

/// The first endpoint, for the view before any check has answered.
fn resolve_endpoint(app: &AppHandle) -> (String, bool) {
    let (endpoints, placeholder) = resolve_endpoints(app);
    (endpoints.into_iter().next().unwrap_or_default(), placeholder)
}

/// **ASK EACH ADDRESS IN TURN UNTIL ONE ANSWERS.** `ask` is one plugin check against one
/// address. An answer is any manifest the plugin read, whether it offers an update or not
/// (`Ok`): that address is where RichOS lives now, and the rest are not asked. An error (the
/// address has no manifest, 404, which the plugin reports as `ReleaseNotFound`; no connection;
/// a manifest it could not read) moves on to the next. When none answers, the LAST address's
/// failure is the one reported: before the move the first address always fails for the same
/// reason (it does not exist yet), so its failure says nothing about this Mac.
///
/// Returns the index of the address that answered, with its answer.
async fn first_answer<T, F, Fut>(endpoints: &[String], mut ask: F) -> Result<(usize, T), Failure>
where
    F: FnMut(tauri::Url) -> Fut,
    Fut: std::future::Future<Output = Result<T, tauri_plugin_updater::Error>>,
{
    let mut last = Failure {
        kind: "configuration",
        headline: "This build of RichOS has no usable update server configured.".to_string(),
        detail: "no update endpoint is configured".to_string(),
    };
    for (i, endpoint) in endpoints.iter().enumerate() {
        let url = match tauri::Url::parse(endpoint) {
            Ok(url) => url,
            Err(e) => {
                // Not routed through `Failure::classify`: the endpoint never became a URL, so
                // there is no plugin error to classify and inventing one would misreport where
                // the fault is. This is a configuration fault and says so.
                last = Failure {
                    kind: "configuration",
                    headline: "This build of RichOS has no usable update server configured.".to_string(),
                    detail: format!("{endpoint}: {e}"),
                };
                continue;
            }
        };
        match ask(url).await {
            Ok(answer) => return Ok((i, answer)),
            Err(e) => last = Failure::classify(&e),
        }
    }
    Err(last)
}

/// Where the plugin fetches `update`'s archive: on RichOS's new address when the manifest came
/// from there and still names the old one. The bytes are signature-checked either way.
fn download_from_home(update: &mut tauri_plugin_updater::Update, answered: &str) {
    if let Some(home) = richos_core::github_home::download_at(answered, update.download_url.as_str()) {
        if let Ok(url) = tauri::Url::parse(&home) {
            update.download_url = url;
        }
    }
}

/// Install the update state. Pure — no I/O, no network, nothing that can fail — so it is
/// safe to call at any point in `setup` without moving what is already first there.
pub fn init(app: &AppHandle) {
    let (endpoint, placeholder) = resolve_endpoint(app);
    let version = app.package_info().version.to_string();
    app.manage(Updates {
        view: Mutex::new(UpdateView::new(version, endpoint, placeholder)),
        pending: Mutex::new(None),
        operation: AtomicBool::new(false),
        relaunching: AtomicBool::new(false),
    });
}

/// Write a transition and tell the webview. Every state change in this file goes through
/// here, so there is exactly one place an emit could be forgotten.
fn transition<F: FnOnce(&mut UpdateView)>(app: &AppHandle, f: F) -> UpdateView {
    let state = app.state::<Updates>();
    let view = {
        let mut guard = state.view.lock().expect("update view lock");
        f(&mut guard);
        guard.clone()
    };
    // Best-effort, exactly like every other emitter in this shell: a missing or closed
    // webview must never stall the update path.
    let _ = app.emit(EVENT_UPDATE, view.clone());
    view
}

fn now_millis() -> u64 {
    richos_core::util::now_millis()
}

// ---------------------------------------------------------------------------------------
// The three operations
// ---------------------------------------------------------------------------------------

/// Ask the endpoint what it has. Never installs, never downloads a byte of payload.
pub async fn check(app: &AppHandle) -> UpdateView {
    let state = app.state::<Updates>();
    let Some(_operation) = UpdateOperation::acquire(&state.operation) else {
        return state.snapshot();
    };
    refresh_rollback_target(app);
    #[cfg(target_os = "macos")]
    if let Ok(home) = app.path().home_dir() {
        match richos_user_update::staged(&home) {
            // A STAGED ROLLBACK IS ALSO SOMETHING WAITING FOR THE NEXT LAUNCH, and it is
            // deliberately NOT newer, so the newer-than test alone would walk straight past
            // it and go asking the server about an update he has just decided to leave.
            Ok(Some(s))
                if s.rollback
                    || s.publication
                        .is_newer_than(&app.package_info().version.to_string())
                        .unwrap_or(false) =>
            {
                return transition(app, |v| {
                    v.state = "ready";
                    v.available_version = Some(s.publication.version);
                    v.ready_is_rollback = s.rollback;
                    v.percent = Some(100);
                })
            }
            // A shared startup cannot retire an obsolete prepared receipt. It must
            // still query the server for releases newer than its actual running build.
            Ok(Some(_)) | Ok(None) => {}
            Err(e) => return journal_failure(app, e),
        }
    }
    let (endpoints, placeholder) = resolve_endpoints(app);
    let endpoint = endpoints.first().cloned().unwrap_or_default();

    if placeholder {
        return transition(app, |v| {
            v.clear_attempt();
            v.state = "unconfigured";
            v.endpoint = endpoint.clone();
            v.endpoint_is_placeholder = true;
        });
    }

    transition(app, |v| {
        v.clear_attempt();
        v.state = "checking";
        v.endpoint = endpoint.clone();
        v.endpoint_is_placeholder = false;
    });

    let answered = first_answer(&endpoints, |url| {
        let app = app.clone();
        async move { app.updater_builder().endpoints(vec![url])?.build()?.check().await }
    })
    .await;
    let answered = match answered {
        Ok((i, answer)) => {
            let endpoint = endpoints[i].clone();
            transition(app, |v| v.endpoint = endpoint.clone());
            Ok(answer.map(|mut update| {
                download_from_home(&mut update, &endpoint);
                update
            }))
        }
        Err(failure) => Err(failure),
    };

    match answered {
        Ok(Some(update)) => {
            let version = update.version.clone();
            let notes = update.body.clone();
            // The manifest's own `pub_date` string, not a re-formatting of the parsed
            // `OffsetDateTime`. `Update::date` is already proof it parsed; passing the
            // server's text through means the date on screen is the date in the file, with
            // no chance of a formatter quietly shifting a zone.
            let date = update
                .raw_json
                .get("pub_date")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            *app.state::<Updates>().pending.lock().expect("pending lock") = Some(update);
            transition(app, |v| {
                v.state = "available";
                v.available_version = Some(version);
                v.notes = notes;
                v.pub_date = date;
                v.checked_at = Some(now_millis());
                // FROM NOW, not from the manifest's `pub_date`. The question this answers is
                // "how long has RichOS been unable to offer me this", which starts when this
                // install learned about it — a version published in March that this machine
                // met an hour ago has been waiting an hour.
                v.ready_since = Some(now_millis());
            })
        }
        Ok(None) => {
            *app.state::<Updates>().pending.lock().expect("pending lock") = None;
            transition(app, |v| {
                v.state = "upToDate";
                v.checked_at = Some(now_millis());
            })
        }
        Err(failure) => transition(app, |v| {
            v.state = "failed";
            v.failure = Some(failure);
            v.checked_at = Some(now_millis());
        }),
    }
}

/// Download and verify the offered update into private staging. Ready means activation
/// will happen at a normal startup when no participating RichOS session is running.
/// This command never replaces an application bundle or exits the live process.
pub async fn install(app: &AppHandle) -> UpdateView {
    let state = app.state::<Updates>();
    let Some(_operation) = UpdateOperation::acquire(&state.operation) else {
        return state.snapshot();
    };
    // THE GATE, BEFORE THE PENDING UPDATE IS EVEN LOOKED AT.
    //
    // `updates.js` removes the Install control while `busy`, so on the shipping surface this
    // is normally unreachable — and it is here anyway, for the same reason
    // `start_voice_capture` still refuses after `main.js` has hidden the talk button: hiding
    // an affordance is the affordance half of a fix and never the whole of it. What reaches
    // it is the RACE — he pressed while idle and a turn, a proactive message or a worker
    // started in the milliseconds before the command landed — plus anything that ever calls
    // this function from somewhere other than that button.
    //
    // It REFUSES rather than queueing, and the distinction is the CEO's own: he still
    // presses it, and the control comes back when his work is finished. A queued install
    // that fired by itself later would be mode 1, which §26 rules is a design session of its
    // own and which this file does not invent.
    let verdict = refresh_work_verdict(app);
    if verdict.busy {
        // The state is UNCHANGED — still `available`, still holding the same pending update,
        // with `busy` now true. Nothing was downloaded, nothing was lost, and the surface
        // reads the reason off the same payload it already had.
        return app.state::<Updates>().snapshot();
    }

    let pending = {
        let state = app.state::<Updates>();
        let guard = state.pending.lock().expect("pending lock");
        guard.clone()
    };

    let Some(update) = pending else {
        return transition(app, |v| {
            v.state = "failed";
            v.failure = Some(Failure {
                kind: "configuration",
                headline: "There is no update to install.".to_string(),
                detail: "install was called before a check found an update".to_string(),
            });
        });
    };

    transition(app, |v| {
        v.state = "downloading";
        v.downloaded_bytes = 0;
        v.total_bytes = None;
        v.percent = None;
        v.failure = None;
    });

    // Progress is emitted on a THROTTLE, not per chunk. reqwest hands back chunks in the
    // low tens of kilobytes, so the event count scales with the payload and the payload is
    // about to change by an order of magnitude (CEO decision 16 deletes `app/acp-adapter/`).
    // A time-based throttle is the only kind that is correct at both sizes: no byte count,
    // no chunk count, no threshold tuned to today's bundle.
    let app_for_chunks = app.clone();
    let mut received: u64 = 0;
    let mut last_emit = Instant::now() - Duration::from_millis(200);
    let on_chunk = move |chunk: usize, total: Option<u64>| {
        received += chunk as u64;
        let done = total.map(|t| received >= t).unwrap_or(false);
        if last_emit.elapsed() >= Duration::from_millis(100) || done {
            last_emit = Instant::now();
            let percent = total.and_then(|t| {
                if t == 0 {
                    None
                } else {
                    Some(((received.min(t) * 100) / t) as u8)
                }
            });
            transition(&app_for_chunks, |v| {
                v.state = "downloading";
                v.downloaded_bytes = received;
                v.total_bytes = total;
                v.percent = percent;
            });
        }
    };

    // The gap between the last byte and the first extracted file is where the signature is
    // checked, and it is long enough to look like a stall. So it gets its own state and its
    // own sentence in the UI rather than a bar frozen at 100%. Deliberately not tied to any
    // duration: minisign over the whole payload takes as long as the payload is long.
    let app_for_finish = app.clone();
    let on_finish = move || {
        transition(&app_for_finish, |v| {
            v.state = "installing";
            v.percent = Some(100);
        });
    };

    // Download returns only after the plugin verifies the signed payload. Its macOS
    // install method has an administrator fallback, so it is never called here.
    let bytes = match update.download(on_chunk, on_finish).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return transition(app, |v| {
                v.state = "failed";
                v.failure = Some(Failure::classify(&e));
            })
        }
    };
    // A turn or worker may have started during the download. Keep the offer available
    // and let the user retry after work finishes, without publishing the bundle.
    if refresh_work_verdict(app).busy {
        return transition(app, |v| {
            v.state = "available";
        });
    }
    #[cfg(target_os = "macos")]
    let result = app
        .path()
        .home_dir()
        .map_err(std::io::Error::other)
        .and_then(|home| richos_user_update::stage_verified(&home, &bytes, &update.version).map(|_| ()));
    #[cfg(not(target_os = "macos"))]
    let result: std::io::Result<()> = Err(std::io::Error::other(
        "Password-free application updates are not implemented on this platform.",
    ));
    match result {
        Ok(()) => transition(app, |v| {
            v.state = "ready";
            v.percent = Some(100);
            v.ready_is_rollback = false;
        }),
        Err(error) => journal_failure(app, error),
    }
}

// ---------------------------------------------------------------------------------------
// The way back
// ---------------------------------------------------------------------------------------

/// Re-read where back is, from this installation's own publication history.
///
/// Cheap — three small reads under the updater's own lock — and deliberately NOT cached at
/// `init`: the answer changes the moment an update or a rollback activates, and a stale
/// "Go back to 1.2.0" on a copy that is already on 1.2.0 is worse than no control at all.
/// Called at the top of every check and on every `update_state`, which are the two moments
/// the surface is about to be painted.
fn refresh_rollback_target(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use richos_user_update::RollbackTarget;
        let target = app
            .path()
            .home_dir()
            .ok()
            .and_then(|home| richos_user_update::rollback_target(&home).ok())
            .and_then(|t| match t {
                RollbackTarget::Available(version) => Some(version),
                // Both empty answers become `None`. The UI's question is "is there a way
                // back", and "the record points forwards" is a no with a longer story.
                RollbackTarget::NoRecord | RollbackTarget::NotOlder(_) => None,
            });
        transition(app, |v| v.rollback_version = target);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// THE MANIFEST FOR ONE SPECIFIC EARLIER TAG, built from the endpoint this build already
/// uses.
///
/// `app/scripts/nightly.py::finish` uploads `latest.json` to the per-version release as well
/// as to the rolling channel release (`gh release upload info["tag"] … out/"latest.json"`,
/// the line after the one that uploads the archive), and `NIGHTLY.md` states that existing
/// tags and artifacts are never overwritten. So every published nightly carries its own
/// immutable, signed manifest at a URL of exactly one shape:
///
///   `<host>/<owner>/<repo>/releases/download/v<version>/latest.json`
///
/// ...whether the channel endpoint in force is the rolling nightly one
/// (`…/releases/download/nightly/latest.json`) or the stable one
/// (`…/releases/latest/download/latest.json`). Everything between `/releases/` and the final
/// segment is the part that names WHICH release, so replacing exactly that is one rule that
/// covers both, and it is the same rule for a fixture server that mirrors the layout.
///
/// A build pointed somewhere that is not a release channel gets `None` and a sentence, not a
/// guessed URL: an updater that invents a download location is worse than one that says it
/// cannot find the file.
fn previous_manifest_url(endpoint: &str, version: &str) -> Option<tauri::Url> {
    let url = tauri::Url::parse(endpoint).ok()?;
    let path = url.path();
    let (before, after) = path.split_once("/releases/")?;
    let file = after.rsplit('/').next()?;
    if file.is_empty() {
        return None;
    }
    let mut next = url.clone();
    next.set_path(&format!("{before}/releases/download/v{version}/{file}"));
    Some(next)
}

/// **Go back to the version this Mac was running before the last update.**
///
/// The CEO runs the nightly as his daily driver. Before this, the only documented way off a
/// bad one was `NIGHTLY.md`'s *"install a stable version at least as new as the nightly"* —
/// by hand, from a browser, on the machine he was trying to use.
///
/// It is the SAME PATH AS AN UPDATE, deliberately and at every step: the same
/// signature-verifying `Update::download` against the same compiled public key, the same
/// staging, the same activation at the next ordinary launch. Only two things differ — the
/// manifest fetched is one specific earlier tag's rather than the channel's, and the
/// staging call writes the authorization that lets startup accept a downgrade once.
///
/// What it does NOT do is reuse the copy the updater retired. `collect_retired` reclaims
/// those bytes at a later startup, so the retained copy is not something a recovery path may
/// depend on — and bytes that have been sitting in a user-writable directory since the day
/// they were displaced are not bytes to put back without re-verifying them anyway.
pub async fn rollback(app: &AppHandle) -> UpdateView {
    let state = app.state::<Updates>();
    let Some(_operation) = UpdateOperation::acquire(&state.operation) else {
        return state.snapshot();
    };
    // The same gate, for the same reason, in the same place: hiding the control is the
    // affordance half of a fix and never the whole of it.
    if refresh_work_verdict(app).busy {
        return app.state::<Updates>().snapshot();
    }

    refresh_rollback_target(app);
    let Some(target) = app.state::<Updates>().snapshot().rollback_version else {
        return transition(app, |v| {
            v.state = "failed";
            v.failure = Some(Failure {
                kind: "rollback",
                headline: "RichOS has no earlier version on this Mac to go back to."
                    .to_string(),
                detail: "no publication receipt records a version older than the one \
                         installed now"
                    .to_string(),
            });
        });
    };

    let (endpoints, placeholder) = resolve_endpoints(app);
    let endpoint = endpoints.first().cloned().unwrap_or_default();
    if placeholder {
        return transition(app, |v| {
            v.clear_attempt();
            v.state = "unconfigured";
            v.endpoint = endpoint.clone();
            v.endpoint_is_placeholder = true;
        });
    }
    // The tag's own manifest under each address, in the same order as a check asks them: before
    // the GitHub move it is found under the old address, after it under the new one.
    let channels: Vec<String> = endpoints.iter().filter(|e| previous_manifest_url(e, &target).is_some()).cloned().collect();
    let manifests: Vec<String> = channels.iter().filter_map(|e| previous_manifest_url(e, &target)).map(|u| u.to_string()).collect();
    if manifests.is_empty() {
        let detail = format!("{endpoint} is not a release-channel manifest URL");
        return transition(app, |v| {
            v.state = "failed";
            v.failure = Some(Failure {
                kind: "configuration",
                headline: "This build of RichOS cannot find where earlier versions are kept."
                    .to_string(),
                detail,
            });
        });
    };

    transition(app, |v| {
        v.clear_attempt();
        v.state = "checking";
        v.available_version = Some(target.clone());
        v.ready_is_rollback = true;
        v.endpoint = endpoint.clone();
        v.endpoint_is_placeholder = false;
    });

    // THE COMPARATOR IS THE WHOLE OF THE DIFFERENCE AT THIS LAYER. The plugin's default is
    // `release.version > current_version`, which is the correct default and the reason a
    // rollback cannot happen by accident: without this closure the fetch would succeed, the
    // manifest would parse, and `check()` would hand back `None`. It admits exactly one
    // version — the one this installation's own history named — and nothing else, so a
    // manifest that answers with something other than the tag asked for is refused here
    // rather than downloaded and then argued with.
    let answered = first_answer(&manifests, |url| {
        let app = app.clone();
        let wanted = target.clone();
        async move {
            app.updater_builder()
                .version_comparator(move |_current, remote| remote.version.to_string() == wanted)
                .endpoints(vec![url])?
                .build()?
                .check()
                .await
        }
    })
    .await;

    let update = match answered {
        Ok((i, Some(mut update))) => {
            let channel = channels[i].clone();
            download_from_home(&mut update, &channel);
            transition(app, |v| v.endpoint = channel.clone());
            update
        }
        Ok((_, None)) => {
            let detail = format!("the manifest for v{target} does not offer {target}");
            return transition(app, |v| {
                v.state = "failed";
                v.failure = Some(Failure {
                    kind: "manifest",
                    headline: "The update server does not have that earlier version."
                        .to_string(),
                    detail,
                });
            });
        }
        Err(failure) => {
            return transition(app, |v| {
                v.state = "failed";
                v.failure = Some(failure);
                v.checked_at = Some(now_millis());
            });
        }
    };

    transition(app, |v| {
        v.state = "downloading";
        v.available_version = Some(update.version.clone());
        v.notes = None;
        v.downloaded_bytes = 0;
        v.total_bytes = None;
        v.percent = None;
        v.failure = None;
        v.ready_is_rollback = true;
    });

    let app_for_chunks = app.clone();
    let mut received: u64 = 0;
    let mut last_emit = Instant::now() - Duration::from_millis(200);
    let on_chunk = move |chunk: usize, total: Option<u64>| {
        received += chunk as u64;
        let done = total.map(|t| received >= t).unwrap_or(false);
        if last_emit.elapsed() >= Duration::from_millis(100) || done {
            last_emit = Instant::now();
            let percent = total.and_then(|t| {
                if t == 0 {
                    None
                } else {
                    Some(((received.min(t) * 100) / t) as u8)
                }
            });
            transition(&app_for_chunks, |v| {
                v.state = "downloading";
                v.downloaded_bytes = received;
                v.total_bytes = total;
                v.percent = percent;
            });
        }
    };
    let app_for_finish = app.clone();
    let on_finish = move || {
        transition(&app_for_finish, |v| {
            v.state = "installing";
            v.percent = Some(100);
        });
    };

    // Returns only after the plugin has verified the signed payload. Identical to the
    // update path, and the reason a rollback is not a hole in the signature story.
    let bytes = match update.download(on_chunk, on_finish).await {
        Ok(bytes) => bytes,
        Err(e) => {
            return transition(app, |v| {
                v.state = "failed";
                v.failure = Some(Failure::classify(&e));
            })
        }
    };
    if refresh_work_verdict(app).busy {
        // Nothing staged, nothing lost. Unlike an update there is no standing offer to
        // return to, so the row goes back to what it can still say truthfully.
        return transition(app, |v| {
            v.state = "idle";
            v.percent = None;
        });
    }
    #[cfg(target_os = "macos")]
    let result = app
        .path()
        .home_dir()
        .map_err(std::io::Error::other)
        .and_then(|home| {
            richos_user_update::stage_rollback_verified(&home, &bytes, &update.version).map(|_| ())
        });
    #[cfg(not(target_os = "macos"))]
    let result: std::io::Result<()> = {
        let _ = bytes;
        Err(std::io::Error::other(
            "Going back to an earlier version is not implemented on this platform.",
        ))
    };
    match result {
        Ok(()) => transition(app, |v| {
            v.state = "ready";
            v.percent = Some(100);
            v.ready_is_rollback = true;
            v.ready_since = Some(now_millis());
        }),
        Err(error) => journal_failure(app, error),
    }
}

// ---------------------------------------------------------------------------------------
// The work gate
// ---------------------------------------------------------------------------------------

/// How often the gate is re-read while an update is waiting. See [`spawn_work_watcher`].
const WATCH_INTERVAL: Duration = Duration::from_secs(5);

/// The two states in which the CEO has something to act on, and therefore the only two in
/// which the gate is worth reading at all. Everywhere else the watcher does a single mutex
/// read and goes back to sleep.
fn state_awaits_the_ceo(state: &str) -> bool {
    matches!(state, "available" | "ready")
}

/// Take the three readings, from the shell, without ever locking the spine for an answer.
///
/// The readings and the reasoning about each are in [`richos_core::work_gate`]'s module doc;
/// this function is only the part that has to touch handles. It is deliberately tiny and
/// deliberately dumb — every decision it could make is made there, where the spine suite can
/// test it.
fn work_verdict(app: &AppHandle) -> WorkVerdict {
    let Some(state) = app.try_state::<crate::AppState>() else {
        // Before `setup` has managed the state there is no app to be busy. Reachable only in
        // the window before the window exists, where nothing can have pressed anything.
        return work_gate::decide(&WorkSources::all_clear());
    };

    // 1. THE ACTIVE TURN — the spine's own mirror, read without the spine's mutex. Never
    //    inferred from silence (continuity §5.2).
    let turn = if state.control.has_active_turns() {
        Liveness::Busy
    } else {
        Liveness::Clear
    };

    // 2. WHETHER THE SPINE IS BEING DRIVEN AT ALL. `try_lock`, never `lock`: `send_message`
    //    holds this mutex for the entire length of a turn, so blocking here would answer
    //    after the turn it means to protect had ended. A lock we cannot take is UNKNOWN, and
    //    unknown waits — which is what covers the window at `spine.rs:1609` where the mirror
    //    is already cleared and `drain_queue` has not yet begun the next queued turn.
    let spine = match state.spine.try_lock() {
        Ok(_guard) => Liveness::Clear,
        Err(_) => Liveness::Unknown,
    };

    // Every conversation keeps its own session, including while parked.
    let engine_state = state.data_dir.join("engine-state");
    let views:Vec<_>=state.control.lease_sessions().iter()
        .map(|session|richos_core::app_workers::status(&engine_state,Some(session))).collect();
    let (workers, worker_gap) = work_gate::all_workers(&views);

    // 4. **BACKGROUND WORK — the second lease** (background-work spec §6.4, which calls this
    //    *"not optional and not a follow-up"*). Reading 3 is scoped to the CONVERSATION
    //    lease's session, so a background worker is structurally invisible to it and an
    //    update could install over live work.
    //
    //    **Two readings, not one, and the second is the one the spec's own sentence would
    //    have missed.** The work lease's worker view covers workers it started; the
    //    ASSIGNMENT REGISTER covers an assignment between its registration and its first
    //    worker, which has no workers to see and is still work in flight — and it is the
    //    only thing that can see an assignment stopped at a decision of his (§7.8), which
    //    has no workers at all and must never be installed over.
    //
    //    The counting is `WorkHost::background_work`'s, in `richos-core`, for the reason
    //    this function's own doc gives: a decision made in the shell is a decision the spine
    //    suite cannot reach. This function stays the part that touches handles.
    let background_work = state.work.background_work();
    //    **EVERY back end, not "the" one.** The CEO's Two Riches spec puts one back-end Rich
    //    behind each conversation thread, so there is no single work session to read: an
    //    update that installed over the second thread's work would destroy it exactly as
    //    surely as over the first's.
    let work_lease_views: Vec<_> = state
        .work
        .lease_sessions()
        .iter()
        .map(|session| richos_core::app_workers::status(&engine_state, Some(session)))
        .collect();
    let (background, background_gap) = work_gate::worst(
        work_gate::background(&background_work, &work_lease_views),
        // 5. **COMMANDS RICH STARTED, on every product lease** (reap gap C5): the relaunch ends
        //    them now, so a running one blocks as background work does.
        work_gate::commands(&richos_core::lease_commands::LiveLeases::process().reading()),
    );

    work_gate::decide(&WorkSources { turn, spine, workers, worker_gap, background, background_gap })
}

/// Re-read the gate and write it into the view. Emits only when something CHANGED.
///
/// The "only when changed" is not an optimization, it is the ruling: *"Nothing about the
/// update may interrupt. No modal, no focus steal, no repeated prompting while work runs."*
/// An event every five seconds that says the same thing is a surface that repaints under his
/// hands for no reason.
fn refresh_work_verdict(app: &AppHandle) -> WorkVerdict {
    let verdict = work_verdict(app);
    let changed = {
        let state = app.state::<Updates>();
        let guard = state.view.lock().expect("update view lock");
        guard.busy != verdict.busy
            || guard.busy_reason != verdict.reason
            || guard.unchecked != verdict.unchecked
    };
    if changed {
        let v = verdict.clone();
        transition(app, move |view| {
            view.busy = v.busy;
            view.busy_reason = v.reason;
            view.unchecked = v.unchecked;
        });
    }
    verdict
}

/// Watch the gate for as long as there is something the CEO could act on.
///
/// **WHY A POLL AND NOT AN EVENT.** The turn half could be pushed — the spine already emits
/// `TurnStatus` — but the worker half cannot: a worker's state changes through hook writes
/// in another process and produces no signal in this one, which is the same fact
/// `spine.rs`'s own "one last worker re-join" comment records. So workers must be read, and
/// once one source is read the other may as well be read beside it rather than growing two
/// mechanisms that agree most of the time.
///
/// **WHAT IT COSTS, MEASURED RATHER THAN ASSERTED.**
/// `crates/richos-core/tests/work_gate_cost.rs` times the whole question — the worker read
/// plus the decision — against a deliberately pessimistic fixture: 3,000 worker rows, half
/// of them open runs, so 1,500 liveness syscalls per read. On this machine, release profile,
/// 2026-09-05: **4,825 us per read**. The duty cycle is that over the interval:
///
/// ```text
/// 4_825 us / 5_000_000 us = 0.0965 % of one core
/// ```
///
/// and only while an update is waiting. In every other state the tick is a single mutex read
/// of a `&str` and returns, which is why the loop does that read FIRST. A real session's
/// file on the same machine held 2,918 rows with far fewer open runs, so the shipped cost is
/// below the measured one rather than above it.
///
/// **WHY 5 s AND NOT 1 s.** The only thing a shorter interval buys is that the cue reappears
/// sooner after work ends; it costs five times the reads to buy it, and 0.48 % of a core to
/// make chrome arrive four seconds earlier is the wrong trade for a ruling whose whole
/// content is *"not important enough to get in the way"*. **WHY NOT 30 s:** the cue would be
/// absent for up to half a minute after his work finished, which reads as an app that has
/// not noticed rather than one that is waiting.
pub fn spawn_work_watcher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(WATCH_INTERVAL).await;
            let awaits = {
                let Some(updates) = app.try_state::<Updates>() else {
                    continue;
                };
                let guard = updates.view.lock().expect("update view lock");
                state_awaits_the_ceo(guard.state)
            };
            // Nothing is waiting on him, so there is no control to hide and nothing to say.
            // Deliberately NOT `break`: a check five minutes from now can put us back into
            // `available`, and a watcher that had exited would leave that update's cue
            // painted from whatever the verdict was when the check happened.
            if !awaits {
                continue;
            }
            refresh_work_verdict(&app);
            // The work he was waiting on may just have ended.
            relaunch_if_clear(&app);
        }
    });
}

// ---------------------------------------------------------------------------------------
// The relaunch (CEO feedback, 2026-10-06, item 1)
// ---------------------------------------------------------------------------------------

/// How long the notice is on screen before RichOS quits and comes back. Long enough to read
/// one sentence in the pill or the row; short enough that it is still "immediately".
const RELAUNCH_NOTICE: Duration = Duration::from_secs(3);

/// How long a declined quit may leave the notice up before the row goes back to `ready`.
/// The quit path is bounded at 2 s; this is that, with room.
const RELAUNCH_DECLINED: Duration = Duration::from_secs(10);

/// THE WHOLE DECISION, pure so it is tested without a window or a gate.
///
/// * `ready` and `restarts_when_idle`: an update this app downloaded is prepared.
/// * `!busy`: the work gate is clear. While work runs the answer is no, and the watcher
///   asks again every [`WATCH_INTERVAL`], so the relaunch follows the end of the work.
/// * `has_window`: a window is open. With none, the app is quitting by itself as its work
///   ends (§2.4a), and a relaunch would put back a window he closed.
pub fn should_relaunch(state: &str, restarts_when_idle: bool, busy: bool, has_window: bool) -> bool {
    state == "ready" && restarts_when_idle && !busy && has_window
}

/// Relaunch into the prepared update if nothing is running; otherwise do nothing and let
/// the watcher ask again.
fn relaunch_if_clear(app: &AppHandle) {
    let view = app.state::<Updates>().snapshot();
    if !view.restarts_when_idle || view.state != "ready" {
        return;
    }
    let verdict = refresh_work_verdict(app);
    if !should_relaunch(view.state, view.restarts_when_idle, verdict.busy, !app.webview_windows().is_empty()) {
        return;
    }
    if app
        .state::<Updates>()
        .relaunching
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        transition(&app, |v| v.state = "restarting");
        tokio::time::sleep(RELAUNCH_NOTICE).await;
        // ASKED AGAIN AT THE LAST MOMENT: he may have sent a message during the notice.
        let verdict = refresh_work_verdict(&app);
        let still = should_relaunch("ready", true, verdict.busy, !app.webview_windows().is_empty());
        let back_to_ready = |app: &AppHandle, keep: bool| {
            transition(app, |v| {
                v.state = "ready";
                v.restarts_when_idle = keep;
            });
            app.state::<Updates>().relaunching.store(false, Ordering::Release);
        };
        if !still {
            // Work started: today's behavior, and the watcher relaunches when it ends.
            back_to_ready(&app, true);
            return;
        }
        let regular = app
            .try_state::<crate::AppState>()
            .map(|s| s.can_come_back)
            .unwrap_or(true);
        if let Err(error) = crate::update_relaunch::spawn_helper(regular) {
            // Nothing quits without something to bring it back. The update stays prepared
            // and activates at the next launch, which the row says again.
            eprintln!("[richos] update relaunch: could not start the helper: {error}");
            back_to_ready(&app, false);
            return;
        }
        eprintln!(
            "[richos] update ready and nothing is running: relaunching into {}",
            app.state::<Updates>().snapshot().available_version.unwrap_or_default()
        );
        // The app's own quit path: every lease settles and the clean-exit marker is written.
        app.exit(0);
        // Still here: the quit was declined (registered work appeared in the last instant and
        // he chose to keep it). The helper starts nothing after its own bound, and this does
        // not try again, so there is never a second helper waiting on the same quit.
        tokio::time::sleep(RELAUNCH_DECLINED).await;
        back_to_ready(&app, false);
    });
}

// ---------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------

/// The view, with the work gate read FRESH rather than served from the last poll.
///
/// `updates.js` calls this when the settings menu is opened, which is the one moment the CEO
/// is deliberately looking at this surface. Serving him a verdict up to
/// `WATCH_INTERVAL` old there would be the one place a stale answer is guaranteed to be
/// seen — and it would be seen as an Install button that should not be there.
#[richos_command::command]
pub fn update_state(app: AppHandle) -> UpdateView {
    refresh_work_verdict(&app);
    // The way back is re-read here for the same reason the gate is: this is the moment he
    // is deliberately looking at the surface, and it is the one place a stale answer is
    // guaranteed to be seen.
    refresh_rollback_target(&app);
    app.state::<Updates>().snapshot()
}

#[richos_command::command]
pub async fn update_check(app: AppHandle) -> UpdateView {
    check(&app).await
}

#[richos_command::command]
pub async fn update_install(app: AppHandle) -> UpdateView {
    let view = install(&app).await;
    // THE PRESS THAT DOWNLOADED IT ARMS THE RELAUNCH, and only that press: the selftest
    // calls `install` directly and exits at `ready`, as `updater-e2e.sh` requires.
    if view.state != "ready" || view.ready_is_rollback {
        return view;
    }
    transition(&app, |v| v.restarts_when_idle = true);
    relaunch_if_clear(&app);
    app.state::<Updates>().snapshot()
}

/// The third verb, reached the same way the other two are.
#[richos_command::command]
pub async fn update_rollback(app: AppHandle) -> UpdateView {
    rollback(&app).await
}

/// Compatibility endpoint for an older frontend. Updates never exit a live session;
/// verified staging is activated automatically during the next normal app launch.
#[richos_command::command]
pub fn update_relaunch(app: AppHandle) -> UpdateView {
    app.state::<Updates>().snapshot()
}

// ---------------------------------------------------------------------------------------
// The automatic check
// ---------------------------------------------------------------------------------------

/// Check once, shortly after launch, on a background task.
///
/// SILENT ONLY WHEN THERE IS NOTHING TO SAY. It emits `rich://update` for every transition
/// exactly as the button does, so the settings surface shows the result and the rail's gear
/// carries a mark when something was found — the CEO can always see that RichOS checked and
/// what it found. It does not download and it does not install.
///
/// The delay is not cosmetic: `setup` has just replayed the ledger and built the window, and
/// a TLS handshake competing with first paint is a slower first paint for a result nobody is
/// waiting on.
pub fn spawn_launch_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let _ = check(&app).await;
    });
}

// ---------------------------------------------------------------------------------------
// The headless end-to-end mode
// ---------------------------------------------------------------------------------------

/// `RICHOS_UPDATE_SELFTEST=check|install|rollback`, read once at launch.
pub fn selftest_mode() -> Option<String> {
    match std::env::var(SELFTEST_ENV) {
        Ok(v) if v == "check" || v == "install" || v == "rollback" => Some(v),
        _ => None,
    }
}

/// Drive the real flow from the command line and exit with a meaningful code.
///
/// WHY THIS EXISTS RATHER THAN A SCRIPTED CLICK. Proving an update end to end means a
/// process that is a real `.app` bundle replacing itself — nothing outside a bundle can
/// exercise `install_inner`, which resolves the bundle from `current_exe`. Driving the GUI
/// with AppleScript would need an Accessibility grant that an ad-hoc bundle loses on every
/// rebuild, i.e. exactly the grant this repository has measured as dying. So the harness
/// calls the SAME `check` and `install` functions the two commands call — not a copy of
/// them, and not a reimplementation with the verification left out.
///
/// It prints one machine-readable line per transition, prefixed, so `updater-e2e.sh` asserts
/// on states rather than on log prose.
///
/// Exit codes: 0 the requested operation reached its terminal success state; 10 the check
/// found nothing; 11 it failed; 12 the install failed; 13 the rollback failed. The failure
/// KIND is on stdout, which is how case T asserts that a tampered artifact is refused for
/// the signature and not for something that merely resembles it.
pub fn spawn_selftest(app: AppHandle, mode: String) {
    tauri::async_runtime::spawn(async move {
        let say = |v: &UpdateView| {
            println!(
                "RICHOS-UPDATE-SELFTEST state={} current={} available={} percent={} back={} rollback={} endpoint={} failure={} detail={}",
                v.state,
                v.current_version,
                v.available_version.clone().unwrap_or_else(|| "-".into()),
                v.percent.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
                v.rollback_version.clone().unwrap_or_else(|| "-".into()),
                v.ready_is_rollback,
                // The address that answered (the GitHub move: new first, old second).
                if v.endpoint.is_empty() { "-" } else { v.endpoint.as_str() },
                v.failure.as_ref().map(|f| f.kind).unwrap_or("-"),
                v.failure
                    .as_ref()
                    .map(|f| f.detail.replace('\n', " "))
                    .unwrap_or_else(|| "-".into()),
            );
            use std::io::Write;
            let _ = std::io::stdout().flush();
        };

        // `rollback` NEVER CHECKS THE CHANNEL FIRST. Asking the server what is newest is
        // the one thing the CEO pressing this button is not asking for, and a check here
        // would also leave a staged update in the way of the thing he did ask for.
        if mode == "rollback" {
            let before = update_state(app.clone());
            say(&before);
            let back = rollback(&app).await;
            say(&back);
            let code = if back.state == "ready" && back.ready_is_rollback {
                0
            } else {
                13
            };
            println!("RICHOS-UPDATE-SELFTEST exit={code}");
            use std::io::Write;
            let _ = std::io::stdout().flush();
            std::process::exit(code);
        }

        let checked = check(&app).await;
        say(&checked);

        let code = match checked.state {
            "available" if mode == "install" => {
                let installed = install(&app).await;
                say(&installed);
                if installed.state == "ready" {
                    0
                } else {
                    12
                }
            }
            "available" => 0,
            "upToDate" => 10,
            _ => 11,
        };
        println!("RICHOS-UPDATE-SELFTEST exit={code}");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        std::process::exit(code);
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn update_operation_excludes_another_thread_and_releases_on_failure() {
        use std::sync::Arc;
        let active = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let guard = super::UpdateOperation::acquire(&active).unwrap();
        let other = Arc::clone(&active);
        assert!(
            std::thread::spawn(move || super::UpdateOperation::acquire(&other).is_none())
                .join()
                .unwrap()
        );
        drop(guard);
        assert!(super::UpdateOperation::acquire(&active).is_some());
    }

    use super::*;

    /// The classifier is the only judgement this file makes about a vendor error, and the
    /// signature arm is the one that must never widen. A minisign failure that came back as
    /// "network" would be offered a Retry button, and a retry against a tampered artifact
    /// refuses again — turning a security refusal into something a person clicks through.
    #[test]
    fn a_minisign_failure_is_classified_as_a_signature_failure_and_nothing_else() {
        let err = tauri_plugin_updater::Error::SignatureUtf8("not base64".into());
        let f = Failure::classify(&err);
        assert_eq!(f.kind, "signature");
        assert!(f.headline.contains("not signed by RichOS"));
        // The vendor's own words survive into the payload.
        assert!(f.detail.contains("not base64"));
    }

    /// CEO feedback 2026-10-06, item 1: relaunch at once when nothing runs, wait while
    /// anything does, and never for a `ready` this app did not download.
    #[test]
    fn a_downloaded_update_relaunches_only_when_nothing_is_running() {
        assert!(should_relaunch("ready", true, false, true), "idle: relaunch now");
        assert!(!should_relaunch("ready", true, true, true), "work running: wait for it");
        assert!(
            !should_relaunch("ready", false, false, true),
            "staged by an earlier launch that could not activate it: a relaunch would loop"
        );
        assert!(!should_relaunch("ready", true, false, false), "no window: never put one back");
        for other in ["available", "downloading", "installing", "restarting", "failed"] {
            assert!(!should_relaunch(other, true, false, true), "{other}: nothing to relaunch into");
        }
        let fresh = UpdateView::new("0.1.0".into(), "https://example.com/u".into(), false);
        assert!(!fresh.restarts_when_idle, "nothing is armed before a download");
    }

    /// THE 2026-10-21 FIRST-RUN DEAD END, at the surface: a busy update record is said as
    /// busy, never as "could not prepare this update" over a bare OS error, and every other
    /// record error is still the install failure it was.
    #[test]
    fn a_busy_update_record_is_busy_and_never_a_failure_to_prepare() {
        let busy = std::io::Error::new(std::io::ErrorKind::WouldBlock, richos_user_update::PUBLICATION_BUSY);
        let f = Failure::of_journal(&busy);
        assert_eq!(f.kind, "busy");
        assert_eq!(f.headline, BUSY_HEADLINE);
        assert!(!f.headline.contains("could not prepare"), "{}", f.headline);
        // The OS's own words for the same kind are never the only thing he is shown either.
        let raw = Failure::of_journal(&std::io::Error::from_raw_os_error(libc::EWOULDBLOCK));
        assert_eq!(raw.kind, "busy");
        assert_eq!(raw.headline, BUSY_HEADLINE);

        let broken = std::io::Error::new(std::io::ErrorKind::InvalidData, "pending update no longer matches its verified receipt");
        let f = Failure::of_journal(&broken);
        assert_eq!(f.kind, "install");
        assert_eq!(f.headline, INSTALL_HEADLINE);
        assert!(f.detail.contains("no longer matches"));
    }

    #[test]
    fn an_unreachable_server_is_offline_and_a_bad_manifest_is_not() {
        let manifest = tauri_plugin_updater::Error::ReleaseNotFound;
        assert_eq!(Failure::classify(&manifest).kind, "manifest");

        let target = tauri_plugin_updater::Error::TargetNotFound("darwin-aarch64".into());
        assert_eq!(Failure::classify(&target).kind, "manifest");

        let empty = tauri_plugin_updater::Error::EmptyEndpoints;
        assert_eq!(Failure::classify(&empty).kind, "configuration");

        let insecure = tauri_plugin_updater::Error::InsecureTransportProtocol;
        assert_eq!(Failure::classify(&insecure).kind, "configuration");
    }

    /// A fresh view on a placeholder endpoint must not read as "up to date" or as an error.
    /// "No update server has been chosen yet" is a THIRD thing, and the UI needs it to be.
    #[test]
    fn the_committed_placeholder_endpoint_opens_as_unconfigured() {
        let v = UpdateView::new(
            "0.1.0".into(),
            format!("https://{PLACEHOLDER_ENDPOINT_HOST}/x"),
            true,
        );
        assert_eq!(v.state, "unconfigured");
        assert!(v.endpoint_is_placeholder);
        assert!(v.failure.is_none());
    }

    #[test]
    fn a_real_endpoint_opens_as_idle_with_nothing_claimed_about_it() {
        let v = UpdateView::new("0.1.0".into(), "https://example.com/u".into(), false);
        assert_eq!(v.state, "idle");
        assert!(!v.endpoint_is_placeholder);
        assert!(
            v.checked_at.is_none(),
            "never checked is not the same as checked and clean"
        );
        assert!(v.available_version.is_none());
    }

    // ===================================================================================
    // THE VERSION ON SCREEN IS THE RUNNING BUILD'S (CEO, 2026-09-17, item 4)
    // ===================================================================================
    //
    // His words: *"The RichOS app is currently lying about the current version saying
    // "RichOS 1.0.3 is up to date." even though the current version is 1.0.2. Make sure the
    // app always displays the correct version."*
    //
    // MEASURED, BECAUSE THE FIX DEPENDS ON WHERE 1.0.3 CAME FROM. The bundle he was running
    // IS 1.0.3 — `~/Applications/RichOS.app/Contents/Info.plist` says
    // `CFBundleShortVersionString = 1.0.3`, dated 2026-09-08 12:01. The number entered the
    // tree at 7bd1e727 (2026-09-07, "prepare version 1.0.3"), which set Cargo.toml's version
    // to the NEXT UNRELEASED stable release, as `scripts/nightly.py` requires. No v1.0.3 tag
    // and no 1.0.3 release ever existed, and main went on to 1.2.0 at d9cd1b6c. So this file
    // reported the running build honestly and the running build was never a release.
    //
    // WHAT THESE TWO TESTS HOLD is the half that is this file's to keep: the number comes
    // from the package the bundle was compiled as, and NOTHING renames it afterwards. The
    // other half — that a build which is not a release must not be stamped with a bare
    // release number — is a property of the build scripts, not of this file.

    /// The view carries whatever version it was constructed with, unaltered. It is a straight
    /// carry rather than a formatter for a reason: a pre-release version
    /// (`1.2.0-nightly.20260917.1`) is the one form that must survive to the screen intact,
    /// because it is the form that tells the CEO he is not on a release.
    #[test]
    fn the_view_carries_the_running_version_verbatim() {
        for version in ["1.0.2", "1.0.3", "1.2.0-nightly.20260917.1", "0.1.0"] {
            let v = UpdateView::new(version.into(), "https://example.com/u".into(), false);
            assert_eq!(
                v.current_version, version,
                "the view rewrote the version it was given"
            );
        }
    }

    /// `current_version` IS ASSIGNED ONCE AND NEVER AGAIN, read off this file's own source.
    ///
    /// The lie the CEO reported would become a real one here the day some arm of check,
    /// download or install writes the manifest's version into this field: the window would
    /// then name a build he is not running and have no way to know it. A behavioral test
    /// cannot see the writer that has not been added yet, so this reads the source — the same
    /// technique `ui/tests/setup.js` uses to keep a one-line convenience from coming back.
    #[test]
    fn current_version_has_exactly_one_writer() {
        let src = include_str!("updates.rs");
        // Construction (`current_version,` as a struct-literal shorthand and the `new`
        // parameter) is not an assignment; `x.current_version = ...` is.
        let writers: Vec<&str> = src
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with("//") && !line.starts_with("///"))
            .filter(|line| {
                line.contains("current_version")
                    && line.contains('=')
                    && !line.contains("==")
                    && !line.contains("assert")
            })
            .collect();
        assert!(
            writers.is_empty(),
            "`current_version` is written after construction, so the running build can be \
             renamed by something that is not the bundle it is: {writers:?}"
        );
    }

    /// ONE RULE FOR BOTH ENDPOINT SHAPES, and a refusal for anything else.
    ///
    /// The nightly channel endpoint and the stable one disagree about everything except the
    /// thing that matters: the segment naming WHICH release sits between `/releases/` and
    /// the final path element. If this rule were wrong, a rollback would fetch a 404 and
    /// report it as a manifest failure — a wrong URL that looks like a missing release.
    #[test]
    fn the_previous_tags_manifest_is_derived_from_either_endpoint_shape() {
        let cases = [
            (
                "https://github.com/WebDevBooster/richos/releases/download/nightly/latest.json",
                "https://github.com/WebDevBooster/richos/releases/download/v1.2.0-nightly.20260919.3/latest.json",
            ),
            (
                "https://github.com/WebDevBooster/richos/releases/latest/download/latest.json",
                "https://github.com/WebDevBooster/richos/releases/download/v1.2.0-nightly.20260919.3/latest.json",
            ),
            // The fixture server in `scripts/updater-e2e.sh` case R, which mirrors the
            // published layout precisely so the harness exercises this rule and not a
            // friendlier one.
            (
                "http://127.0.0.1:8973/releases/download/nightly/latest.json",
                "http://127.0.0.1:8973/releases/download/v1.2.0-nightly.20260919.3/latest.json",
            ),
        ];
        for (endpoint, want) in cases {
            let got = previous_manifest_url(endpoint, "1.2.0-nightly.20260919.3")
                .unwrap_or_else(|| panic!("no URL derived from {endpoint}"));
            assert_eq!(got.as_str(), want, "from {endpoint}");
        }
    }

    /// A build pointed somewhere that is not a release channel gets NOTHING, not a guess.
    /// An updater that invents a download location is worse than one that says it cannot
    /// find the file, because the sentence on screen would be about a missing release.
    #[test]
    fn an_endpoint_that_is_not_a_release_channel_derives_no_url_at_all() {
        for endpoint in [
            // The committed placeholder. `rollback` never reaches the derivation with this
            // one — it returns `unconfigured` first — and it must still not resolve.
            "https://updates.richos.invalid/darwin/aarch64/1.0.0",
            "https://example.com/latest.json",
            "https://example.com/releases/",
            "not a url at all",
        ] {
            assert!(
                previous_manifest_url(endpoint, "1.0.0").is_none(),
                "{endpoint} must not resolve to a guessed location"
            );
        }
    }

    const NEW: &str = "https://github.com/RichSpark/RichOS/releases/latest/download/latest.json";
    const OLD: &str = "https://github.com/WebDevBooster/richos/releases/latest/download/latest.json";

    /// THE GITHUB MOVE (CEO, 2026-10-27): the release that ships before the move must find its
    /// updates at the old address until the move and at the new one after it, so the shipping
    /// config names both, the new one first. Only one address here and the updater would read
    /// one place only.
    #[test]
    fn the_shipping_config_asks_the_new_address_first_and_the_old_one_second() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["plugins"]["updater"]["endpoints"], serde_json::json!([NEW, OLD]));
    }

    /// Asked in order through [`first_answer`]; `answers` is what each address says, `None` for
    /// "no manifest here" (the plugin's `ReleaseNotFound`, a 404) and `Some(v)` for a manifest
    /// offering `v`. Returns which answered and the addresses asked.
    fn ask(answers: [Option<&'static str>; 2]) -> (Result<(usize, &'static str), &'static str>, Vec<String>) {
        let asked = std::cell::RefCell::new(Vec::new());
        let endpoints = [NEW.to_string(), OLD.to_string()];
        let got = tauri::async_runtime::block_on(first_answer(&endpoints, |url| {
            asked.borrow_mut().push(url.to_string());
            let answer = answers[asked.borrow().len() - 1];
            async move { answer.ok_or(tauri_plugin_updater::Error::ReleaseNotFound) }
        }));
        (got.map_err(|f| f.kind), asked.into_inner())
    }

    /// Before the move `RichSpark/RichOS` does not exist, so the new address answers 404 and the
    /// old one answers: the update is found there.
    #[test]
    fn before_the_move_the_new_address_has_nothing_and_the_old_one_answers() {
        let (got, asked) = ask([None, Some("1.3.0")]);
        assert_eq!(got, Ok((1, "1.3.0")));
        assert_eq!(asked, [NEW, OLD], "the new address first, then the old one");
    }

    /// After the move the new address answers and the old one is never asked, so nothing
    /// depends on GitHub's redirect from it.
    #[test]
    fn after_the_move_the_new_address_answers_and_the_old_one_is_not_asked() {
        let (got, asked) = ask([Some("1.3.0"), Some("1.2.0")]);
        assert_eq!(got, Ok((0, "1.3.0")));
        assert_eq!(asked, [NEW], "the old address is not asked once the new one answers");
    }

    /// Neither answers: the old address's failure is the one reported (before the move the new
    /// one always fails for its own reason, which says nothing about this Mac).
    #[test]
    fn when_neither_address_answers_the_last_failure_is_reported() {
        let endpoints = [NEW.to_string(), OLD.to_string()];
        let mut n = 0;
        let got = tauri::async_runtime::block_on(first_answer(&endpoints, |_url| {
            n += 1;
            let e = if n == 1 { tauri_plugin_updater::Error::ReleaseNotFound } else { tauri_plugin_updater::Error::TargetNotFound("darwin-aarch64".into()) };
            async move { Err::<(), _>(e) }
        }));
        let failure = got.unwrap_err();
        assert_eq!((failure.kind, n), ("manifest", 2));
        assert!(failure.detail.contains("darwin-aarch64"), "{}", failure.detail);
        // An address that is not a URL is skipped as a configuration fault, not a stop.
        let got = tauri::async_runtime::block_on(first_answer(&["not a url".to_string(), OLD.to_string()], |_url| async { Ok::<_, tauri_plugin_updater::Error>(7) }));
        assert_eq!(got.map_err(|f| f.kind), Ok((1, 7)));
    }
}
