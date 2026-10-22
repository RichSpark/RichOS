//! **DELIVERY: ONE APPROVED REPORT, ITS SCREENSHOTS, ONE ID, AN HONEST STATE** (PRD version 6
//! §6 to §8; round 39 §5).
//!
//! The wire contract is the reporting relay's, agreed on 2026-10-22 with its owner
//! (`zach-opus-reportrelay1`). Its authority is `richos/mobile/service/RICHDOCTOR-RELAY.md` on
//! `cc/zach-opus-reportrelay1`, protocol 2: one Send report is
//!
//!   1. **register** (`POST /v1/reports`, schema `richdoctor-report/2`): the approved text, the
//!      screenshot manifest, the report ID and the receipt secret. It never creates an issue;
//!   2. **upload** each approved image (`PUT /v1/reports/{id}/attachments/{n}`, the secret in the
//!      `X-Receipt-Secret` header, never the URL);
//!   3. **complete** (`POST /v1/reports/{id}/complete`), which creates the issue only when every
//!      approved image is stored, and never creates a text-only issue for a report with images.
//!
//! Every reply carries `state`, `stage`, `stored` and `missing`. The rules this file enforces:
//!
//!   - The app generates the report ID and an unguessable receipt secret and saves them with the
//!     approved bytes BEFORE the first request ([`approve`]; the caller saves before
//!     [`attempt`]). The content hash covers the exact text and the manifest.
//!   - Only `delivered` with an issue link on the configured repository AND every approved image
//!     confirmed is "Report submitted". An issue with fewer confirmed images is "Report created;
//!     checking screenshots" with its real link; nothing creates another issue.
//!   - A lost or unclear answer is checked by status, for at most two minutes from the attempt,
//!     then "Delivery could not be confirmed". It never creates twice.
//!   - Try again re-sends the same ID, secret, text and images, after a definite "not sent" only.
//!   - A changed revision first RETIRES the earlier ID at the relay ([`retire`]); only a
//!     `retired` answer lets the revision be sent.
//!   - The receipt secret is never shown, logged or put in the report.
use crate::image_profile::{canonical, Entry};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

/// The report ID's alphabet: no 0, 1, I, L, O or U, so it reads aloud and copies cleanly.
pub const ID_ALPHABET: &[u8; 30] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The production relay and repository, compiled in.
pub const RELAY: &str = "https://report.richos.ceo";
pub const REPOSITORY: &str = "WebDevBooster/richos";
/// The test override, both set together or neither (agreed with the relay owner).
pub const RELAY_ENV: &str = "RICHDOCTOR_REPORT_RELAY";
pub const REPOSITORY_ENV: &str = "RICHDOCTOR_REPORT_REPOSITORY";

/// A register, status, complete or retire request's local bound (PRD §8).
pub const REQUEST_BOUND: Duration = Duration::from_secs(30);
/// An image upload's no-progress bound and absolute cap (PRD §8).
pub const UPLOAD_STALL: Duration = Duration::from_secs(60);
pub const UPLOAD_CAP: Duration = Duration::from_secs(300);
/// Automatic reconciliation's bound per attempt (PRD §7).
pub const RECONCILE_BOUND_MS: u64 = 120_000;
/// How often to ask while pending, when the relay does not say.
pub const CHECK_EVERY_MS: u64 = 5_000;

/// Where reports go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Destination {
    pub relay: String,
    pub repository: String,
}

impl Destination {
    /// The production destination, or the test pair when both variables are set. One set
    /// without the other is refused: a test relay must never be paired with the production
    /// repository's link check, nor the reverse.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Destination, String> {
        match (get(RELAY_ENV).filter(|s| !s.is_empty()), get(REPOSITORY_ENV).filter(|s| !s.is_empty())) {
            (None, None) => Ok(Destination { relay: RELAY.into(), repository: REPOSITORY.into() }),
            (Some(relay), Some(repository)) => {
                let loopback = relay.starts_with("http://127.0.0.1:") || relay.starts_with("http://localhost:");
                if !(relay.starts_with("https://") || loopback) || repository.split('/').count() != 2 {
                    return Err("the test reporting destination is not a valid relay address and owner/name".into());
                }
                Ok(Destination { relay: relay.trim_end_matches('/').into(), repository })
            }
            _ => Err(format!("{RELAY_ENV} and {REPOSITORY_ENV} must be set together")),
        }
    }
}

/// A new report ID: `RD-XXXX-XXXX`.
pub fn new_report_id() -> std::io::Result<String> {
    let mut chars = Vec::with_capacity(8);
    while chars.len() < 8 {
        for b in crate::util::random_bytes(16)? {
            // Rejection sampling keeps every character equally likely.
            if (b as usize) < 30 * 8 && chars.len() < 8 {
                chars.push(ID_ALPHABET[b as usize % 30] as char);
            }
        }
    }
    let s: String = chars.into_iter().collect();
    Ok(format!("RD-{}-{}", &s[..4], &s[4..]))
}

pub fn is_report_id(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 12 && s.starts_with("RD-") && b[7] == b'-' && b[3..7].iter().chain(&b[8..12]).all(|c| ID_ALPHABET.contains(c))
}

/// The receipt secret: 32 random bytes, base64url, 43 characters.
pub fn new_receipt_secret() -> std::io::Result<String> {
    Ok(crate::util::base64url(&crate::util::random_bytes(32)?))
}

/// The staff-only link the issue carries for screenshot `n` (the relay's read route, behind
/// Cloudflare Access): `<relay>/v1/reports/<id>/attachments/<n>`.
pub fn attachment_link(relay: &str, report_id: &str, n: u32) -> String {
    format!("{}/v1/reports/{report_id}/attachments/{n}", relay.trim_end_matches('/'))
}

/// **THE IMMUTABLE APPROVED PAYLOAD.** Made once, at Send report, from the exact text and
/// screenshot set the person approved, and saved before any request is made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approved {
    pub report_id: String,
    /// Private. Never shown, logged or put in the report.
    pub receipt_secret: String,
    pub title: String,
    pub body: String,
    /// The approved screenshots, in order (their bytes are the draft's `shots/<n>.<ext>`).
    #[serde(default)]
    pub manifest: Vec<Entry>,
    /// hex sha256 of `title + "\n\n" + body + "\n\n" + canonical(manifest)`.
    pub content_sha256: String,
    pub approved_at: u64,
    pub installation_id: String,
}

pub fn content_sha256(title: &str, body: &str, manifest: &[Entry]) -> String {
    crate::util::sha256_hex(format!("{title}\n\n{body}\n\n{}", canonical(manifest)).as_bytes())
}

/// Freeze the approved bytes and images under a new ID and secret. Refused when the last
/// mechanical check finds anything ([`crate::scrub::check_outgoing`]), when the manifest breaks
/// the limits, or when the body does not end with the screenshot lines and the footer.
pub fn approve(report_id: &str, title: &str, body: &str, manifest: Vec<Entry>, relay: &str, now: u64, installation_id: &str) -> Result<Approved, Vec<crate::scrub::Problem>> {
    let problem = |d: String| vec![crate::scrub::Problem { item: "body".into(), detail: d }];
    if !crate::util::is_uuid(installation_id) {
        return Err(problem("RichDoctor could not make its installation ID, so the report was not sent.".into()));
    }
    let receipt_secret = new_receipt_secret().map_err(|e| problem(format!("RichDoctor could not make a receipt: {e}")))?;
    let problems = crate::scrub::check_outgoing(title, body, Some(&receipt_secret));
    if !problems.is_empty() {
        return Err(problems);
    }
    crate::image_profile::check_set(&manifest).map_err(|e| problem(format!("The screenshots are over the limit: {e}.")))?;
    let ending = crate::report::ending(relay, report_id, manifest.len());
    if !body.ends_with(&ending) {
        return Err(problem("The report's screenshot lines or its report ID footer are missing.".into()));
    }
    Ok(Approved {
        report_id: report_id.into(),
        receipt_secret,
        title: title.into(),
        body: body.into(),
        content_sha256: content_sha256(title, body, &manifest),
        manifest,
        approved_at: now,
        installation_id: installation_id.into(),
    })
}

impl Approved {
    /// The register request's body (the contract's field names).
    pub fn register_body(&self) -> Value {
        json!({
            "schema": "richdoctor-report/2",
            "report_id": self.report_id,
            "receipt_secret": self.receipt_secret,
            "content_sha256": self.content_sha256,
            "installation_id": self.installation_id,
            "app_version": crate::VERSION,
            "approved_at": self.approved_at,
            "title": self.title,
            "body": self.body,
            "manifest": self.manifest,
        })
    }
    pub fn secret_body(&self) -> Value {
        json!({ "receipt_secret": self.receipt_secret })
    }
}

/// Why a report was definitely not sent, in the words round 39 shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cause", rename_all = "kebab-case")]
pub enum NotSent {
    /// The Mac is confirmed offline.
    Offline,
    /// The service asked to wait. `until` is when, when it said.
    Busy { until: Option<u64> },
    /// The connection failed and the cause is not known (nothing reached the service).
    Unreachable,
    /// The service refused the content; the person must change it.
    Rejected { reason: String, detail: String, item: Option<String> },
    /// The service could not create it (GitHub refused, or the attempt stopped first).
    Refused { until: Option<u64> },
    /// The service is not taking reports right now (operator switch or daily cap).
    Closed { until: Option<u64> },
    /// The service has no record of the report, or never began creating it.
    NotReceived,
    /// A screenshot upload did not finish (it stalled, hit its cap or was canceled); the
    /// approved images are kept and Try again resumes with the ones still missing.
    UploadStopped { item: String, why: String },
    /// This report ID was retired (a revision replaced it). It cannot be sent.
    Retired,
}

impl NotSent {
    /// The plain reason (round 39's "Not sent yet, by cause").
    pub fn reason(&self) -> String {
        let at = |until: &Option<u64>| until.map(crate::util::local_clock);
        match self {
            NotSent::Offline => "Your Mac is not connected to the internet. Try again when you are online.".into(),
            NotSent::Busy { until } => match at(until) {
                Some(t) => format!("The reporting service is busy and asked RichDoctor to wait. You can try again after {t}."),
                None => "The reporting service is busy and asked RichDoctor to wait. You can try again in a few minutes.".into(),
            },
            NotSent::Unreachable => "RichDoctor could not reach the reporting service. The connection failed; the cause is not known.".into(),
            NotSent::Rejected { detail, .. } => format!("The reporting service did not accept the report: {}", detail.trim()),
            NotSent::Refused { until } => match at(until) {
                Some(t) => format!("The reporting service could not file the report this time. You can try again after {t}."),
                None => "The reporting service could not file the report this time. You can try again.".into(),
            },
            NotSent::Closed { until } => match at(until) {
                Some(t) => format!("The reporting service is not taking reports right now. You can try again after {t}."),
                None => "The reporting service is not taking reports right now. You can try again later.".into(),
            },
            NotSent::NotReceived => "The reporting service did not receive the report. You can send it again.".into(),
            NotSent::UploadStopped { item, why } => format!("{} did not finish uploading: {why} Nothing was filed. Your report and its screenshots are kept; Try again sends the rest.", capitalized(item)),
            NotSent::Retired => "This copy of the report was replaced by a revised one, so it is not sent.".into(),
        }
    }
    /// Whether the fix is in the content: round 39 offers Review report instead of Try again.
    pub fn needs_review(&self) -> bool {
        matches!(self, NotSent::Rejected { reason, .. } if ["content", "size", "schema", "attachment"].contains(&reason.as_str())) || matches!(self, NotSent::Retired)
    }
}

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// **THE DISTINCT STATES** (PRD §8): sending, sending screenshots, checking, unconfirmed,
/// created with screenshots being checked, submitted, not sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum State {
    /// Approved and saved; the first request is being made.
    Sending,
    /// The screenshots are uploading, before any issue exists.
    SendingScreenshots { done: u32, total: u32 },
    /// The completion request (the one that may create the issue) is about to be made. The
    /// caller saves this BEFORE the request: it is the durable proof of whether completion was
    /// ever attempted (the V6 handover's clarification 1).
    Creating,
    /// The answer was unclear; status is being asked, within the two-minute bound.
    Checking { since: u64 },
    /// The bound passed without a definite answer. The issue may exist.
    Unconfirmed { checked_at: u64 },
    /// The issue exists (its link is real) and not every approved image is confirmed yet.
    CreatedCheckingImages { issue_url: String, issue_number: u64, confirmed: u32, expected: u32, at: u64 },
    /// A matching issue link was confirmed with every approved image.
    Submitted { issue_url: String, issue_number: u64, at: u64 },
    /// Definitely not created.
    NotSent { why: NotSent, at: u64 },
}

impl State {
    /// Try again with the same ID, secret, text and images: only after a definite "not sent"
    /// whose fix is not in the content.
    pub fn permits_try_again(&self) -> bool {
        matches!(self, State::NotSent { why, .. } if !why.needs_review())
    }
    /// Whether a changed revision may be offered for sending: the earlier one is definitely not
    /// created. The relay's retirement ([`retire`]) still has to say `retired` before it goes.
    pub fn permits_revision(&self) -> bool {
        matches!(self, State::NotSent { .. })
    }
    /// Whether delivery is unresolved: edits may be saved, nothing may be sent.
    pub fn unresolved(&self) -> bool {
        matches!(self, State::Sending | State::SendingScreenshots { .. } | State::Creating | State::Checking { .. } | State::Unconfirmed { .. } | State::CreatedCheckingImages { .. })
    }
}

/// What a transport saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transport {
    /// An HTTP answer, with its body when it was JSON.
    Answer { status: u16, body: Option<Value>, retry_after: Option<u64> },
    /// The connection was never made: nothing reached the service. `offline` when this Mac
    /// has no network at all (confirmed, not guessed).
    NotConnected { offline: bool },
    /// The request may have reached the service: a timeout or a connection lost mid-way.
    Unclear,
    /// An upload made no progress for [`UPLOAD_STALL`], ran past [`UPLOAD_CAP`] or was canceled.
    Stopped(String),
}

/// The HTTPS seam: the app crate makes the requests, tests make them up.
pub trait Relay {
    fn post(&self, url: &str, body: &Value) -> Transport;
    /// Upload `bytes` with `headers`; `progress` hears the bytes sent so far.
    fn put(&self, url: &str, headers: &[(String, String)], bytes: Vec<u8>, progress: &mut dyn FnMut(u64)) -> Transport;
}

/// An issue link is accepted only on the configured repository, as a plain issue number.
pub fn issue_link_ok(url: &str, number: Option<u64>, repository: &str) -> Option<u64> {
    let rest = url.strip_prefix(&format!("https://github.com/{repository}/issues/"))?;
    let n: u64 = rest.parse().ok()?;
    (n > 0 && number.is_none_or(|k| k == n)).then_some(n)
}

/// What one answer means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    Done(State),
    /// Creation is pending: ask again after this many milliseconds.
    Pending(u64),
    /// Registered and not created: these screenshots are still missing (empty: ready).
    Receiving { missing: Vec<String> },
    /// Not a definite answer: keep checking.
    Unclear,
}

fn ids(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string).or_else(|| x.as_u64().map(|n| n.to_string()))).collect()).unwrap_or_default()
}

/// Read one relay answer. `now` stamps the state.
pub fn read(t: &Transport, repository: &str, now: u64) -> Reading {
    let not_sent = |why: NotSent| Reading::Done(State::NotSent { why, at: now });
    let until = |s: Option<u64>| s.map(|s| now + s * 1000);
    match t {
        Transport::NotConnected { offline: true } => not_sent(NotSent::Offline),
        Transport::NotConnected { offline: false } => not_sent(NotSent::Unreachable),
        Transport::Unclear | Transport::Stopped(_) => Reading::Unclear,
        Transport::Answer { body: None, .. } => Reading::Unclear,
        Transport::Answer { body: Some(b), retry_after, .. } => {
            let secs = b["retry_after_seconds"].as_u64().or(*retry_after);
            let link = || b["issue_url"].as_str().and_then(|u| issue_link_ok(u, b["issue_number"].as_u64(), repository).map(|n| (u.to_string(), n)));
            let count = |k: &str| b[k].as_u64().map(|n| n as u32);
            match b["state"].as_str() {
                Some("delivered") => match (link(), count("images_confirmed"), count("images_expected")) {
                    // Every approved image is confirmed (or there were none).
                    (Some((u, n)), c, e) if c == e => Reading::Done(State::Submitted { issue_url: u, issue_number: n, at: now }),
                    (Some((u, n)), c, e) => Reading::Done(State::CreatedCheckingImages { issue_url: u, issue_number: n, confirmed: c.unwrap_or(0), expected: e.unwrap_or(0), at: now }),
                    // A "delivered" without a link on the right repository is not a success.
                    (None, ..) => Reading::Unclear,
                },
                Some("created_checking_images") => match link() {
                    Some((u, n)) => Reading::Done(State::CreatedCheckingImages { issue_url: u, issue_number: n, confirmed: count("images_confirmed").unwrap_or(0), expected: count("images_expected").unwrap_or(0), at: now }),
                    None => Reading::Unclear,
                },
                Some("receiving") | Some("ready") => Reading::Receiving { missing: ids(&b["missing"]) },
                Some("pending") | Some("creating") => Reading::Pending(b["check_after_seconds"].as_u64().map(|s| s * 1000).unwrap_or(CHECK_EVERY_MS)),
                Some("unconfirmed") => Reading::Done(State::Unconfirmed { checked_at: now }),
                Some("rejected") => {
                    let reason = b["reason"].as_str().unwrap_or("content").to_string();
                    if reason == "delivery" {
                        not_sent(NotSent::Refused { until: until(secs) })
                    } else {
                        let detail = b["detail"].as_str().map(|d| d.chars().take(400).collect()).unwrap_or_else(|| "it found something it could not publish.".into());
                        not_sent(NotSent::Rejected { reason, detail, item: b["item"].as_str().map(str::to_string) })
                    }
                }
                Some("limited") => not_sent(NotSent::Busy { until: until(secs) }),
                Some("closed") => not_sent(NotSent::Closed { until: until(secs) }),
                Some("unknown_id") => not_sent(NotSent::NotReceived),
                Some("retired") => not_sent(NotSent::Retired),
                // conflict, forbidden, anything new: never a reason to create again.
                _ => Reading::Unclear,
            }
        }
    }
}

/// The approved image `n`'s bytes, from the draft (the caller's loader).
pub type Images<'a> = &'a dyn Fn(&Entry) -> Option<Vec<u8>>;

/// Upload every image in `missing`, in manifest order. `Ok(())` when the relay answers that
/// none is missing; `Err(state)` for the state to show.
/// How often one upload is resent after a "limited" answer, and the longest wait slept through.
const UPLOAD_WAITS: u32 = 3;
const UPLOAD_WAIT_MAX_MS: u64 = 30_000;

fn upload(approved: &Approved, missing: Vec<String>, images: Images, dest: &Destination, relay: &dyn Relay, clock: &dyn Fn() -> u64, sleep: &dyn Fn(u64), on_state: &mut dyn FnMut(&State)) -> Result<(), State> {
    let total = approved.manifest.len() as u32;
    let mut missing = missing;
    // Twice at most: an identical retry of an image that was stored answers the same.
    for _round in 0..2 {
        if missing.is_empty() {
            return Ok(());
        }
        let round: Vec<&Entry> = approved.manifest.iter().filter(|e| missing.contains(&e.attachment_id)).collect();
        for entry in round {
            if !missing.contains(&entry.attachment_id) {
                continue;
            }
            let done = total - missing.len() as u32;
            on_state(&State::SendingScreenshots { done, total });
            let item = format!("screenshot {}", entry.order);
            let Some(bytes) = images(entry) else {
                return Err(State::NotSent { why: NotSent::UploadStopped { item, why: "its saved copy on this Mac could not be read.".into() }, at: clock() });
            };
            if crate::util::sha256_hex(&bytes) != entry.sha256 {
                return Err(State::NotSent { why: NotSent::UploadStopped { item, why: "its saved copy on this Mac is not the approved image.".into() }, at: clock() });
            }
            let headers = vec![
                ("X-Receipt-Secret".to_string(), approved.receipt_secret.clone()),
                ("Content-Type".to_string(), entry.media_type.clone()),
                ("X-Content-SHA256".to_string(), entry.sha256.clone()),
            ];
            let url = format!("{}/v1/reports/{}/attachments/{}", dest.relay, approved.report_id, entry.attachment_id);
            let mut waits = 0;
            loop {
                let t = relay.put(&url, &headers, bytes.clone(), &mut |_| {});
                if let Transport::Stopped(why) = &t {
                    return Err(State::NotSent { why: NotSent::UploadStopped { item, why: why.clone() }, at: clock() });
                }
                match read(&t, &dest.repository, clock()) {
                    Reading::Receiving { missing: m } => missing = m,
                    // "limited": the upload met the receipt lock or a running cleanup. The same
                    // upload is sent again after the short wait the relay asked for; a long one
                    // is shown to the person instead of slept through.
                    Reading::Done(State::NotSent { why: NotSent::Busy { until: Some(u) }, .. }) if waits < UPLOAD_WAITS && u.saturating_sub(clock()) <= UPLOAD_WAIT_MAX_MS => {
                        waits += 1;
                        sleep(u.saturating_sub(clock()));
                        continue;
                    }
                    Reading::Done(s) => return Err(s),
                    Reading::Pending(_) => return Err(State::Checking { since: clock() }),
                    Reading::Unclear => {} // asked again in the next round
                }
                break;
            }
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(State::NotSent { why: NotSent::UploadStopped { item: format!("screenshot {}", missing[0]), why: "the reporting service did not confirm it was stored.".into() }, at: clock() })
    }
}

/// **ONE ATTEMPT** (one Send report, or one Try again): register, upload the missing images,
/// complete; then, on anything but a definite answer, ask status until a definite answer or
/// [`RECONCILE_BOUND_MS`] after the attempt began, then "could not be confirmed". It never
/// creates twice. `retry` is Try again: a register that answers the report's earlier definite
/// refusal goes on to complete, which the relay allows from that state.
pub fn attempt(approved: &Approved, images: Images, dest: &Destination, relay: &dyn Relay, clock: &dyn Fn() -> u64, sleep: &dyn Fn(u64), on_state: &mut dyn FnMut(&State), retry: bool) -> State {
    let begun = clock();
    on_state(&State::Sending);
    let registered = relay.post(&format!("{}/v1/reports", dest.relay), &approved.register_body());
    let missing = match read(&registered, &dest.repository, clock()) {
        Reading::Receiving { missing } => missing,
        Reading::Done(State::NotSent { why, .. }) if retry && matches!(why, NotSent::Refused { .. } | NotSent::Closed { .. } | NotSent::Busy { .. }) => match &registered {
            Transport::Answer { body: Some(b), .. } => ids(&b["missing"]),
            _ => Vec::new(),
        },
        Reading::Done(s) => {
            on_state(&s);
            return s;
        }
        Reading::Pending(ms) => return reconcile(approved, dest, relay, clock, sleep, on_state, begun, ms),
        // Unclear: whether it registered is checked by status, which never creates.
        Reading::Unclear => match check(approved, dest, relay, clock()) {
            Reading::Receiving { missing } => missing,
            Reading::Done(s) => {
                on_state(&s);
                return s;
            }
            _ => return reconcile(approved, dest, relay, clock, sleep, on_state, begun, CHECK_EVERY_MS),
        },
    };
    if let Err(s) = upload(approved, missing, images, dest, relay, clock, sleep, on_state) {
        on_state(&s);
        return s;
    }
    on_state(&State::Creating);
    let complete = |relay: &dyn Relay| relay.post(&format!("{}/v1/reports/{}/complete", dest.relay, approved.report_id), &approved.secret_body());
    let mut answer = complete(relay);
    // The relay found an image missing after all (a cleanup, a lost write): upload it once more.
    if let Reading::Receiving { missing } = read(&answer, &dest.repository, clock()) {
        if !missing.is_empty() {
            if let Err(s) = upload(approved, missing, images, dest, relay, clock, sleep, on_state) {
                on_state(&s);
                return s;
            }
        }
        answer = complete(relay);
    }
    match read(&answer, &dest.repository, clock()) {
        Reading::Done(s) => {
            on_state(&s);
            s
        }
        Reading::Receiving { .. } => {
            let s = State::NotSent { why: NotSent::NotReceived, at: clock() };
            on_state(&s);
            s
        }
        Reading::Pending(ms) => reconcile(approved, dest, relay, clock, sleep, on_state, begun, ms),
        Reading::Unclear => reconcile(approved, dest, relay, clock, sleep, on_state, begun, CHECK_EVERY_MS),
    }
}

/// Ask status until a definite answer, within the two-minute bound of the attempt.
#[allow(clippy::too_many_arguments)]
fn reconcile(approved: &Approved, dest: &Destination, relay: &dyn Relay, clock: &dyn Fn() -> u64, sleep: &dyn Fn(u64), on_state: &mut dyn FnMut(&State), begun: u64, first_wait: u64) -> State {
    let mut wait = first_wait;
    on_state(&State::Checking { since: clock() });
    loop {
        if clock() + wait > begun + RECONCILE_BOUND_MS {
            let s = State::Unconfirmed { checked_at: clock() };
            on_state(&s);
            return s;
        }
        sleep(wait);
        match check(approved, dest, relay, clock()) {
            Reading::Done(State::NotSent { why: NotSent::Offline | NotSent::Unreachable | NotSent::Busy { .. }, .. }) | Reading::Unclear => wait = CHECK_EVERY_MS,
            Reading::Pending(ms) => wait = ms,
            // Registered and never created: creation did not begin, a definite non-creation.
            Reading::Receiving { .. } => {
                let s = State::NotSent { why: NotSent::NotReceived, at: clock() };
                on_state(&s);
                return s;
            }
            Reading::Done(s) => {
                on_state(&s);
                return s;
            }
        }
    }
}

/// **CHECK STATUS** once, for the same report and ID. Never creates.
pub fn check(approved: &Approved, dest: &Destination, relay: &dyn Relay, now: u64) -> Reading {
    let t = relay.post(&format!("{}/v1/reports/{}/status", dest.relay, approved.report_id), &approved.secret_body());
    read(&t, &dest.repository, now)
}

/// What a status check during "could not be confirmed" or "checking screenshots" leaves: a
/// definite answer replaces it; anything else keeps it, with the time checked (round 39:
/// "Still not confirmed, checked at 9:47 AM. The report ID has not changed.").
pub fn after_check(reading: Reading, current: &State, now: u64) -> State {
    match reading {
        Reading::Done(s @ (State::Submitted { .. } | State::CreatedCheckingImages { .. } | State::NotSent { why: NotSent::Rejected { .. } | NotSent::Refused { .. } | NotSent::Closed { .. } | NotSent::NotReceived | NotSent::Retired, .. })) => s,
        Reading::Receiving { .. } => State::NotSent { why: NotSent::NotReceived, at: now },
        _ => match current {
            State::CreatedCheckingImages { .. } => current.clone(),
            _ => State::Unconfirmed { checked_at: now },
        },
    }
}

/// **RETIRE THE EARLIER ID** before a changed revision is sent (PRD §7). `Ok(())` only when the
/// relay answered `retired`; otherwise the earlier report's real state, which the person sees
/// instead of the revision being sent.
pub fn retire(approved: &Approved, dest: &Destination, relay: &dyn Relay, now: u64) -> Result<(), State> {
    let t = relay.post(&format!("{}/v1/reports/{}/retire", dest.relay, approved.report_id), &approved.secret_body());
    match read(&t, &dest.repository, now) {
        Reading::Done(State::NotSent { why: NotSent::Retired, .. }) => Ok(()),
        Reading::Done(s) => Err(s),
        // Lost or unclear: the earlier report's state is checked, never assumed.
        _ => Err(State::Checking { since: now }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    const BASE: &str = "http://127.0.0.1:9";

    fn dest() -> Destination {
        Destination { relay: BASE.into(), repository: "WebDevBooster/richos-test".into() }
    }

    fn png(n: u8) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = png_encoder(&mut out);
            e.write_image_data(&[n, 0, 0, 255]).unwrap();
        }
        out
    }
    fn png_encoder(out: &mut Vec<u8>) -> png::Writer<&mut Vec<u8>> {
        let mut e = png::Encoder::new(out, 1, 1);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap()
    }

    const INST: &str = "0b6f3c2e-5d41-4a8e-9f27-3c1d2e4f5a6b";

    fn approved_with(k: u8) -> (Approved, Vec<Vec<u8>>) {
        let id = "RD-7K3M-Q9VX";
        let images: Vec<Vec<u8>> = (1..=k).map(png).collect();
        let manifest: Vec<Entry> = images.iter().enumerate().map(|(i, b)| Entry::of(i as u32 + 1, b).unwrap()).collect();
        let body = format!("## What happened\n\nIt closes.\n\n{}", crate::report::ending(BASE, id, manifest.len()));
        (approve(id, "RichOS closes at start", &body, manifest, BASE, 1_000, INST).unwrap(), images)
    }

    /// A scripted relay: answers in order; records every request.
    struct Script {
        answers: RefCell<Vec<Transport>>,
        log: RefCell<Vec<(String, String)>>,
    }
    impl Relay for Script {
        fn post(&self, url: &str, _body: &Value) -> Transport {
            self.log.borrow_mut().push(("POST".into(), url.trim_start_matches(BASE).into()));
            self.next()
        }
        fn put(&self, url: &str, headers: &[(String, String)], _bytes: Vec<u8>, _p: &mut dyn FnMut(u64)) -> Transport {
            assert!(headers.iter().any(|(k, _)| k == "X-Receipt-Secret"));
            assert!(!url.contains("secret"), "the secret is never in a URL");
            self.log.borrow_mut().push(("PUT".into(), url.trim_start_matches(BASE).into()));
            self.next()
        }
    }
    impl Script {
        fn next(&self) -> Transport {
            let mut a = self.answers.borrow_mut();
            if a.is_empty() { Transport::Unclear } else { a.remove(0) }
        }
        fn count(&self, what: &str) -> usize {
            self.log.borrow().iter().filter(|(_, u)| u.ends_with(what)).count()
        }
    }
    fn script(answers: Vec<Transport>) -> Script {
        Script { answers: RefCell::new(answers), log: RefCell::new(vec![]) }
    }
    fn j(v: Value) -> Transport {
        Transport::Answer { status: 200, body: Some(v), retry_after: None }
    }
    fn run(a: &Approved, images: &[Vec<u8>], relay: &Script, retry: bool) -> (State, Vec<State>) {
        let now = Cell::new(10_000u64);
        let mut seen = Vec::new();
        let loader = |e: &Entry| images.get(e.order as usize - 1).cloned();
        let s = attempt(a, &loader, &dest(), relay, &|| now.get(), &|ms| now.set(now.get() + ms), &mut |s| seen.push(s.clone()), retry);
        (s, seen)
    }
    const LINK: &str = "https://github.com/WebDevBooster/richos-test/issues/42";

    #[test]
    fn ids_secrets_and_the_hash_over_text_and_manifest() {
        for _ in 0..50 {
            assert!(is_report_id(&new_report_id().unwrap()));
        }
        assert!(!is_report_id("RD-0000-1111"));
        assert_eq!(new_receipt_secret().unwrap().len(), 43);
        let (a, _) = approved_with(2);
        assert_eq!(a.content_sha256, crate::util::sha256_hex(format!("{}\n\n{}\n\n{}", a.title, a.body, canonical(&a.manifest)).as_bytes()));
        let (t, _) = approved_with(0);
        assert_eq!(t.content_sha256, crate::util::sha256_hex(format!("{}\n\n{}\n\n[]", t.title, t.body).as_bytes()));
        assert_eq!(a.register_body()["schema"], "richdoctor-report/2");
        assert_eq!(a.register_body()["manifest"][1]["filename"], "screenshot-2.png");
        assert!(!a.body.contains(&a.receipt_secret));
    }

    #[test]
    fn approval_refuses_a_leak_a_missing_ending_and_too_many_images() {
        let id = "RD-7K3M-Q9VX";
        let ending = crate::report::ending(BASE, id, 0);
        assert!(approve(id, "t", &format!("mail jane@example.com\n\n{ending}"), vec![], BASE, 1, INST).is_err());
        assert!(approve(id, "t", "no footer", vec![], BASE, 1, INST).is_err());
        assert!(approve(id, "t", &format!("x\n\n{ending}"), vec![], BASE, 1, "inst-173fb086d09f").is_err(), "the relay refuses a non-UUID installation ID");
        assert!(approve(id, "t", &format!("x\n\n{ending}"), vec![], BASE, 1, INST).is_ok());
        let (a, _) = approved_with(1);
        assert!(approve(id, "t", &format!("x\n\n{ending}"), a.manifest.clone(), BASE, 1, INST).is_err(), "the screenshot lines must match the manifest");
    }

    #[test]
    fn register_upload_complete_and_submitted_only_with_every_image() {
        let (a, images) = approved_with(2);
        let r = script(vec![
            j(json!({"state":"receiving","stage":"receiving","stored":[],"missing":["1","2"]})),
            j(json!({"state":"receiving","stage":"receiving","stored":["1"],"missing":["2"]})),
            j(json!({"state":"ready","stage":"ready","stored":["1","2"],"missing":[]})),
            j(json!({"state":"delivered","stage":"delivered","issue_url":LINK,"issue_number":42,"images_confirmed":2,"images_expected":2})),
        ]);
        let (s, seen) = run(&a, &images, &r, false);
        assert!(matches!(s, State::Submitted { issue_number: 42, .. }), "{s:?}");
        let order: Vec<String> = r.log.borrow().iter().map(|(m, u)| format!("{m} {u}")).collect();
        assert_eq!(order, ["POST /v1/reports", "PUT /v1/reports/RD-7K3M-Q9VX/attachments/1", "PUT /v1/reports/RD-7K3M-Q9VX/attachments/2", "POST /v1/reports/RD-7K3M-Q9VX/complete"]);
        assert!(seen.contains(&State::SendingScreenshots { done: 1, total: 2 }));
    }

    #[test]
    fn an_issue_with_fewer_confirmed_images_is_created_checking_screenshots() {
        let (a, images) = approved_with(1);
        let r = script(vec![
            j(json!({"state":"ready","stored":["1"],"missing":[]})),
            j(json!({"state":"delivered","issue_url":LINK,"issue_number":42,"images_confirmed":0,"images_expected":1})),
        ]);
        let (s, _) = run(&a, &images, &r, false);
        assert!(matches!(s, State::CreatedCheckingImages { confirmed: 0, expected: 1, .. }), "{s:?}");
        assert!(s.unresolved() && !s.permits_try_again());
        assert_eq!(r.count("/attachments/1"), 0, "a stored image is not sent again");
    }

    #[test]
    fn a_stalled_upload_creates_nothing_and_try_again_resumes_the_missing_image() {
        let (a, images) = approved_with(2);
        let r = script(vec![
            j(json!({"state":"receiving","stored":[],"missing":["1","2"]})),
            j(json!({"state":"receiving","stored":["1"],"missing":["2"]})),
            Transport::Stopped("it made no progress for 60 seconds.".into()),
        ]);
        let (s, _) = run(&a, &images, &r, false);
        let State::NotSent { why: NotSent::UploadStopped { ref item, .. }, .. } = s else { panic!("{s:?}") };
        assert_eq!(item, "screenshot 2");
        assert_eq!(r.count("/complete"), 0, "no issue without every approved image");
        assert!(s.permits_try_again());
        let again = script(vec![
            j(json!({"state":"receiving","stored":["1"],"missing":["2"]})),
            j(json!({"state":"ready","stored":["1","2"],"missing":[]})),
            j(json!({"state":"delivered","issue_url":LINK,"issue_number":42,"images_confirmed":2,"images_expected":2})),
        ]);
        let (s, _) = run(&a, &images, &again, true);
        assert!(matches!(s, State::Submitted { .. }));
        assert_eq!(again.count("/attachments/1"), 0);
        assert_eq!(again.count("/attachments/2"), 1);
    }

    #[test]
    fn a_limited_upload_is_resent_after_the_wait_the_relay_asked_for() {
        // The relay answers "limited" when an upload meets its receipt lock or a running
        // cleanup (RICHDOCTOR-RELAY.md): the same upload is sent again after retry_after_seconds.
        let (a, images) = approved_with(1);
        let r = script(vec![
            j(json!({"state":"receiving","stored":[],"missing":["1"]})),
            Transport::Answer { status: 429, body: Some(json!({"state":"limited","retry_after_seconds":2})), retry_after: Some(2) },
            j(json!({"state":"ready","stored":["1"],"missing":[]})),
            j(json!({"state":"delivered","issue_url":LINK,"issue_number":42,"images_confirmed":1,"images_expected":1})),
        ]);
        let (s, _) = run(&a, &images, &r, false);
        assert!(matches!(s, State::Submitted { .. }), "{s:?}");
        assert_eq!(r.count("/attachments/1"), 2);
        // A long wait is not slept through: the person sees when to try again.
        let r = script(vec![
            j(json!({"state":"receiving","stored":[],"missing":["1"]})),
            Transport::Answer { status: 429, body: Some(json!({"state":"limited","retry_after_seconds":900})), retry_after: Some(900) },
        ]);
        let (s, _) = run(&a, &images, &r, false);
        assert!(matches!(s, State::NotSent { why: NotSent::Busy { until: Some(_) }, .. }), "{s:?}");
        assert_eq!(r.count("/complete"), 0);
    }

    #[test]
    fn a_lost_complete_is_checked_never_created_again_and_bounded() {
        let (a, images) = approved_with(0);
        let r = script(vec![j(json!({"state":"ready","stored":[],"missing":[]})), Transport::Unclear]);
        let (s, _) = run(&a, &images, &r, false);
        assert!(matches!(s, State::Unconfirmed { .. }));
        assert_eq!(r.count("/complete"), 1);
        assert!(r.log.borrow().len() <= 2 + 24, "{}", r.log.borrow().len());
        let r = script(vec![j(json!({"state":"ready","missing":[]})), Transport::Unclear, j(json!({"state":"pending"})), j(json!({"state":"delivered","issue_url":LINK}))]);
        let (s, _) = run(&a, &images, &r, false);
        assert!(matches!(s, State::Submitted { .. }), "{s:?}");
    }

    #[test]
    fn definite_refusals_say_why_and_allow_the_right_next_step() {
        let (a, images) = approved_with(0);
        let cases = [
            (Transport::NotConnected { offline: true }, "Your Mac is not connected to the internet.", true, true),
            (Transport::NotConnected { offline: false }, "the cause is not known", true, true),
            (Transport::Answer { status: 429, body: Some(json!({"state":"limited"})), retry_after: Some(60) }, "busy", true, true),
            (j(json!({"state":"rejected","reason":"content","detail":"one line looks like it contains a password."})), "did not accept", false, true),
            (j(json!({"state":"rejected","reason":"attachment","detail":"screenshot 1 has metadata.","item":"screenshot 1"})), "did not accept", false, true),
            (j(json!({"state":"closed","reason":"daily_cap","retry_after_seconds":3600})), "not taking reports", true, true),
            (j(json!({"state":"retired"})), "replaced by a revised one", false, true),
        ];
        for (answer, words, try_again, revision) in cases {
            let r = script(vec![answer]);
            let (s, _) = run(&a, &images, &r, false);
            let State::NotSent { why, .. } = &s else { panic!("{s:?}") };
            assert!(why.reason().contains(words), "{} / {words}", why.reason());
            assert_eq!(s.permits_try_again(), try_again, "{words}");
            assert_eq!(s.permits_revision(), revision, "{words}");
        }
    }

    #[test]
    fn a_conflict_or_an_unknown_state_is_never_success_nor_a_new_create() {
        let (a, images) = approved_with(0);
        for answer in [j(json!({"state":"conflict"})), j(json!({"state":"delivered"})), j(json!({"state":"teleported"})), Transport::Answer { status: 502, body: None, retry_after: None }] {
            let r = script(vec![j(json!({"state":"ready","missing":[]})), answer]);
            let (s, _) = run(&a, &images, &r, false);
            assert!(matches!(s, State::Unconfirmed { .. }), "{s:?}");
            assert_eq!(r.count("/complete"), 1);
        }
    }

    #[test]
    fn retirement_must_be_acknowledged() {
        let (a, _) = approved_with(0);
        assert_eq!(retire(&a, &dest(), &script(vec![j(json!({"state":"retired"}))]), 5), Ok(()));
        assert!(matches!(retire(&a, &dest(), &script(vec![j(json!({"state":"delivered","issue_url":LINK}))]), 5), Err(State::Submitted { .. })));
        assert!(matches!(retire(&a, &dest(), &script(vec![Transport::Unclear]), 5), Err(State::Checking { .. })));
    }

    #[test]
    fn a_status_check_keeps_the_uncertain_state_unless_the_answer_is_definite() {
        assert!(matches!(after_check(Reading::Unclear, &State::Unconfirmed { checked_at: 1 }, 5), State::Unconfirmed { checked_at: 5 }));
        assert!(matches!(after_check(read(&j(json!({"state":"unknown_id"})), "a/b", 5), &State::Unconfirmed { checked_at: 1 }, 5), State::NotSent { why: NotSent::NotReceived, .. }));
        let created = State::CreatedCheckingImages { issue_url: LINK.into(), issue_number: 42, confirmed: 0, expected: 1, at: 1 };
        assert_eq!(after_check(Reading::Unclear, &created, 5), created);
    }

    #[test]
    fn the_test_destination_is_both_variables_or_neither() {
        let none = |_: &str| None;
        assert_eq!(Destination::from_env(none).unwrap(), Destination { relay: RELAY.into(), repository: REPOSITORY.into() });
        let one = |k: &str| (k == RELAY_ENV).then(|| "http://127.0.0.1:8787".to_string());
        assert!(Destination::from_env(one).is_err());
        let both = |k: &str| Some(if k == RELAY_ENV { "http://127.0.0.1:8787/".to_string() } else { "WebDevBooster/richos-test".to_string() });
        assert_eq!(Destination::from_env(both).unwrap().relay, "http://127.0.0.1:8787");
        let remote_plain = |k: &str| Some(if k == RELAY_ENV { "http://example.com".to_string() } else { "a/b".to_string() });
        assert!(Destination::from_env(remote_plain).is_err());
        assert_eq!(attachment_link("https://report.richos.ceo/", "RD-7K3M-Q9VX", 2), "https://report.richos.ceo/v1/reports/RD-7K3M-Q9VX/attachments/2");
    }
}
