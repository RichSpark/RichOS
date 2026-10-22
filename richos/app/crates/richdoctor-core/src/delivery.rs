//! **DELIVERY: ONE APPROVED REPORT, ONE ID, AN HONEST STATE** (PRD §7, §8; round 39 §5).
//!
//! The wire contract is the reporting relay's (`report.richos.ceo`), agreed on 2026-10-22 with
//! its owner (`zach-opus-reportrelay1`; the relay's committed copy is `richos/mobile/service/RICHDOCTOR-RELAY.md` on
//! `cc/zach-opus-reportrelay1`): `POST /v1/reports` creates, `POST /v1/reports/{id}/status`
//! asks and never creates, every reply carries a `state`.
//!
//! The rules this file enforces, and the PRD lines they come from:
//!
//!   - The app generates a random report ID and an unguessable receipt secret, and persists
//!     both with the approved bytes BEFORE the first request ([`approve`]; the caller saves the
//!     result before calling [`attempt`]).
//!   - The exact approved bytes never change under that ID. An edit after approval is a new,
//!     unapproved revision; it can be approved under a NEW ID only after a definitive
//!     non-creation of the old one ([`State::permits_revision`]).
//!   - Only `delivered` with an issue link on the configured repository is "Report submitted"
//!     ([`issue_link_ok`]). A queued job, an HTTP 200 or a timer running out is not.
//!   - A lost or unclear answer is never a reason to create again: it is checked, by status,
//!     for at most two minutes from the attempt, then "Delivery could not be confirmed" with
//!     Check status and Email support ([`attempt`]).
//!   - Try again re-sends the same ID, secret and bytes, and only after a definite "not sent"
//!     ([`State::permits_try_again`]). Nothing is retransmitted without the person.
//!   - The receipt secret is never shown, logged or put in the footer: the person's receipt is
//!     the report ID.
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

/// One request's local bound (PRD §8).
pub const REQUEST_BOUND: Duration = Duration::from_secs(30);
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

/// **THE IMMUTABLE APPROVED PAYLOAD.** Made once, at Send report, from the exact text the person
/// approved, and saved before any request is made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approved {
    pub report_id: String,
    /// Private. Never shown, logged or put in the report.
    pub receipt_secret: String,
    pub title: String,
    pub body: String,
    /// hex sha256 of `title + "\n\n" + body`.
    pub content_sha256: String,
    pub approved_at: u64,
    pub installation_id: String,
    /// The approved screenshots that belong to this report (their local IDs and hashes). Frozen
    /// with the text; delivered by the screenshot contract once it lands, never dropped here.
    #[serde(default)]
    pub images: Vec<ImageRef>,
}

/// A frozen reference to one approved screenshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRef {
    pub id: String,
    pub sha256: String,
}

pub fn content_sha256(title: &str, body: &str) -> String {
    crate::util::sha256_hex(format!("{title}\n\n{body}").as_bytes())
}

/// Freeze the approved bytes under a new ID and secret. Refused when the last mechanical check
/// finds anything ([`crate::scrub::check_outgoing`]): nothing that fails it is ever approved.
pub fn approve(report_id: &str, title: &str, body: &str, now: u64, installation_id: &str) -> Result<Approved, Vec<crate::scrub::Problem>> {
    let receipt_secret = new_receipt_secret().map_err(|e| vec![crate::scrub::Problem { item: "body".into(), detail: format!("RichDoctor could not make a receipt: {e}") }])?;
    let problems = crate::scrub::check_outgoing(title, body, Some(&receipt_secret));
    if !problems.is_empty() {
        return Err(problems);
    }
    if !body.ends_with(&crate::report::footer(report_id)) {
        return Err(vec![crate::scrub::Problem { item: "body".into(), detail: "The report ID footer is missing.".into() }]);
    }
    Ok(Approved { report_id: report_id.into(), receipt_secret, title: title.into(), body: body.into(), content_sha256: content_sha256(title, body), approved_at: now, installation_id: installation_id.into(), images: Vec::new() })
}

impl Approved {
    /// The create request's body (the contract's field names).
    pub fn create_body(&self) -> Value {
        json!({
            "schema": "richdoctor-report/1",
            "report_id": self.report_id,
            "receipt_secret": self.receipt_secret,
            "content_sha256": self.content_sha256,
            "installation_id": self.installation_id,
            "app_version": crate::VERSION,
            "approved_at": self.approved_at,
            "title": self.title,
            "body": self.body,
        })
    }
    pub fn status_body(&self) -> Value {
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
    /// The service has no record of the report: the request never reached it.
    NotReceived,
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
        }
    }
    /// Whether the fix is in the content: round 39 offers Review report instead of Try again.
    pub fn needs_review(&self) -> bool {
        matches!(self, NotSent::Rejected { reason, .. } if reason == "content" || reason == "size" || reason == "schema")
    }
}

/// **THE DISTINCT STATES** (PRD §8): saved, sending, checking, unconfirmed, submitted, not sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum State {
    /// Approved and saved; the request is being made.
    Sending,
    /// The answer was unclear; status is being asked, within the two-minute bound.
    Checking { since: u64 },
    /// The bound passed without a definite answer. The issue may exist.
    Unconfirmed { checked_at: u64 },
    /// A matching issue link was confirmed.
    Submitted { issue_url: String, issue_number: u64, at: u64 },
    /// Definitely not created.
    NotSent { why: NotSent, at: u64 },
}

impl State {
    /// Try again with the same ID, secret and bytes: only after a definite "not sent" whose fix
    /// is not in the content.
    pub fn permits_try_again(&self) -> bool {
        matches!(self, State::NotSent { why, .. } if !why.needs_review())
    }
    /// A changed revision may be approved under a new ID only when the old one is known not to
    /// exist: rejected, closed, or not received (the relay's rule 5). "Busy" is a request that
    /// was not processed, which says nothing about earlier attempts.
    pub fn permits_revision(&self) -> bool {
        matches!(self, State::NotSent { why: NotSent::Rejected { .. } | NotSent::Closed { .. } | NotSent::NotReceived | NotSent::Refused { .. }, .. })
    }
    /// Whether delivery is unresolved: edits may be saved, nothing may be sent.
    pub fn unresolved(&self) -> bool {
        matches!(self, State::Sending | State::Checking { .. } | State::Unconfirmed { .. })
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
}

/// The HTTPS seam: the app crate makes the requests, tests make them up.
pub trait Relay {
    fn post(&self, url: &str, body: &Value) -> Transport;
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
    /// Pending: ask again after this many milliseconds.
    Pending(u64),
    /// Not a definite answer: keep checking.
    Unclear,
}

/// Read one relay answer. `now` stamps the state; `create` says whether it answered a create.
pub fn read(t: &Transport, repository: &str, now: u64) -> Reading {
    let not_sent = |why: NotSent| Reading::Done(State::NotSent { why, at: now });
    let until = |s: Option<u64>| s.map(|s| now + s * 1000);
    match t {
        Transport::NotConnected { offline: true } => not_sent(NotSent::Offline),
        Transport::NotConnected { offline: false } => not_sent(NotSent::Unreachable),
        Transport::Unclear => Reading::Unclear,
        Transport::Answer { body: None, .. } => Reading::Unclear,
        Transport::Answer { body: Some(b), retry_after, .. } => {
            let secs = b["retry_after_seconds"].as_u64().or(*retry_after);
            match b["state"].as_str() {
                Some("delivered") => match b["issue_url"].as_str().and_then(|u| issue_link_ok(u, b["issue_number"].as_u64(), repository).map(|n| (u, n))) {
                    Some((u, n)) => Reading::Done(State::Submitted { issue_url: u.to_string(), issue_number: n, at: now }),
                    // A "delivered" without a link on the right repository is not a success.
                    None => Reading::Unclear,
                },
                Some("pending") => Reading::Pending(b["check_after_seconds"].as_u64().map(|s| s * 1000).unwrap_or(CHECK_EVERY_MS)),
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
                // conflict, forbidden, anything new: never a reason to create again.
                _ => Reading::Unclear,
            }
        }
    }
}

/// **ONE ATTEMPT**: create, then (on anything but a definite answer) ask status until a definite
/// answer or [`RECONCILE_BOUND_MS`] after the attempt began, then "could not be confirmed". It
/// never creates twice. `on_state` hears each state as it is reached, so the window and the
/// saved copy follow it; `clock` and `sleep` are injected so the bound is testable.
pub fn attempt(approved: &Approved, dest: &Destination, relay: &dyn Relay, clock: &dyn Fn() -> u64, sleep: &dyn Fn(u64), on_state: &mut dyn FnMut(&State)) -> State {
    let begun = clock();
    on_state(&State::Sending);
    let created = relay.post(&format!("{}/v1/reports", dest.relay), &approved.create_body());
    let mut wait = match read(&created, &dest.repository, clock()) {
        Reading::Done(s) => {
            on_state(&s);
            return s;
        }
        Reading::Pending(ms) => ms,
        Reading::Unclear => CHECK_EVERY_MS,
    };
    on_state(&State::Checking { since: clock() });
    loop {
        if clock() + wait > begun + RECONCILE_BOUND_MS {
            let s = State::Unconfirmed { checked_at: clock() };
            on_state(&s);
            return s;
        }
        sleep(wait);
        let answer = check(approved, dest, relay, clock());
        match answer {
            Reading::Done(State::NotSent { why: NotSent::Offline | NotSent::Unreachable | NotSent::Busy { .. }, .. }) | Reading::Unclear => wait = CHECK_EVERY_MS,
            Reading::Pending(ms) => wait = ms,
            Reading::Done(s) => {
                on_state(&s);
                return s;
            }
        }
    }
}

/// **CHECK STATUS** once, for the same report and ID. Never creates.
pub fn check(approved: &Approved, dest: &Destination, relay: &dyn Relay, now: u64) -> Reading {
    let t = relay.post(&format!("{}/v1/reports/{}/status", dest.relay, approved.report_id), &approved.status_body());
    read(&t, &dest.repository, now)
}

/// What a status check during "could not be confirmed" leaves: a definite answer replaces it;
/// anything else keeps it, with the time checked (round 39: "Still not confirmed, checked at
/// 9:47 AM. The report ID has not changed.").
pub fn after_check(reading: Reading, now: u64) -> State {
    match reading {
        Reading::Done(s @ (State::Submitted { .. } | State::NotSent { why: NotSent::Rejected { .. } | NotSent::Refused { .. } | NotSent::Closed { .. } | NotSent::NotReceived, .. })) => s,
        _ => State::Unconfirmed { checked_at: now },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn dest() -> Destination {
        Destination { relay: "http://127.0.0.1:9".into(), repository: "WebDevBooster/richos-test".into() }
    }

    fn approved() -> Approved {
        let id = "RD-7K3M-Q9VX";
        let body = format!("## What happened\n\nIt closes.\n\n{}", crate::report::footer(id));
        approve(id, "RichOS closes at start", &body, 1_000, "inst-1").unwrap()
    }

    /// A scripted relay: answers in order, and counts creates.
    struct Script {
        answers: RefCell<Vec<Transport>>,
        creates: Cell<u32>,
        bodies: RefCell<Vec<Value>>,
    }
    impl Relay for Script {
        fn post(&self, url: &str, body: &Value) -> Transport {
            if url.ends_with("/v1/reports") {
                self.creates.set(self.creates.get() + 1);
            }
            self.bodies.borrow_mut().push(body.clone());
            let mut a = self.answers.borrow_mut();
            if a.is_empty() { Transport::Unclear } else { a.remove(0) }
        }
    }
    fn script(answers: Vec<Transport>) -> Script {
        Script { answers: RefCell::new(answers), creates: Cell::new(0), bodies: RefCell::new(vec![]) }
    }
    fn json_answer(v: Value) -> Transport {
        Transport::Answer { status: 200, body: Some(v), retry_after: None }
    }

    fn run(relay: &Script) -> (State, Vec<State>) {
        let now = Cell::new(10_000u64);
        let mut seen = Vec::new();
        let s = attempt(&approved(), &dest(), relay, &|| now.get(), &|ms| now.set(now.get() + ms), &mut |s| seen.push(s.clone()));
        (s, seen)
    }

    #[test]
    fn ids_and_secrets_have_their_shapes() {
        for _ in 0..50 {
            let id = new_report_id().unwrap();
            assert!(is_report_id(&id), "{id}");
        }
        assert!(!is_report_id("RD-0000-1111"));
        assert_eq!(new_receipt_secret().unwrap().len(), 43);
        let a = approved();
        assert_eq!(a.content_sha256, crate::util::sha256_hex(format!("{}\n\n{}", a.title, a.body).as_bytes()));
        let body = a.create_body();
        assert_eq!(body["schema"], "richdoctor-report/1");
        assert!(!a.body.contains(&a.receipt_secret), "the secret is never in the report");
    }

    #[test]
    fn approval_refuses_what_the_last_check_finds_and_a_missing_footer() {
        assert!(approve("RD-7K3M-Q9VX", "t", "mail jane@example.com\n\nSent with RichDoctor 1.0 · Report ID RD-7K3M-Q9VX", 1, "i").is_err());
        assert!(approve("RD-7K3M-Q9VX", "t", "no footer", 1, "i").is_err());
    }

    #[test]
    fn only_a_delivered_link_on_the_configured_repository_is_submitted() {
        let r = script(vec![json_answer(json!({"state":"delivered","issue_url":"https://github.com/WebDevBooster/richos-test/issues/42","issue_number":42}))]);
        let (s, _) = run(&r);
        assert!(matches!(s, State::Submitted { issue_number: 42, .. }));
        assert_eq!(issue_link_ok("https://github.com/WebDevBooster/richos/issues/42", None, "WebDevBooster/richos-test"), None);
        assert_eq!(issue_link_ok("https://github.com/WebDevBooster/richos-test/issues/42?x=1", None, "WebDevBooster/richos-test"), None);
        assert_eq!(issue_link_ok("https://github.com/WebDevBooster/richos-test/issues/42", Some(43), "WebDevBooster/richos-test"), None);
    }

    #[test]
    fn a_lost_answer_is_checked_never_created_again_and_confirmed() {
        let r = script(vec![Transport::Unclear, Transport::Unclear, json_answer(json!({"state":"pending","check_after_seconds":5})), json_answer(json!({"state":"delivered","issue_url":"https://github.com/WebDevBooster/richos-test/issues/7"}))]);
        let (s, seen) = run(&r);
        assert!(matches!(s, State::Submitted { issue_number: 7, .. }));
        assert_eq!(r.creates.get(), 1, "one create, however unclear the answers");
        assert!(seen.iter().any(|s| matches!(s, State::Checking { .. })));
        assert!(r.bodies.borrow()[1].get("title").is_none(), "status carries only the secret");
    }

    #[test]
    fn two_minutes_without_a_definite_answer_is_unconfirmed() {
        let r = script(vec![]);
        let (s, _) = run(&r);
        assert!(matches!(s, State::Unconfirmed { .. }));
        assert_eq!(r.creates.get(), 1);
        // 120 s / 5 s = 24 checks at most, measured, not assumed.
        assert!(r.bodies.borrow().len() <= 1 + 24, "{}", r.bodies.borrow().len());
        assert!(!s.permits_try_again() && !s.permits_revision() && s.unresolved());
    }

    #[test]
    fn definite_refusals_say_why_and_allow_the_right_next_step() {
        let cases = [
            (Transport::NotConnected { offline: true }, "Your Mac is not connected to the internet. Try again when you are online.", true, false),
            (Transport::NotConnected { offline: false }, "RichDoctor could not reach the reporting service. The connection failed; the cause is not known.", true, false),
            (Transport::Answer { status: 429, body: Some(json!({"state":"limited"})), retry_after: Some(60) }, "The reporting service is busy", true, false),
            (json_answer(json!({"state":"rejected","reason":"content","detail":"in the startup log excerpt, one line looks like it contains a password.","item":"line 12"})), "did not accept the report: in the startup log", false, true),
            (json_answer(json!({"state":"closed","reason":"daily_cap","retry_after_seconds":3600})), "is not taking reports right now", true, true),
            (json_answer(json!({"state":"rejected","reason":"delivery"})), "could not file the report this time", true, true),
        ];
        for (answer, words, try_again, revision) in cases {
            let r = script(vec![answer]);
            let (s, _) = run(&r);
            let State::NotSent { why, .. } = &s else { panic!("{s:?}") };
            assert!(why.reason().starts_with(words) || why.reason().contains(words), "{} / {words}", why.reason());
            assert_eq!(s.permits_try_again(), try_again, "{words}");
            assert_eq!(s.permits_revision(), revision, "{words}");
        }
    }

    #[test]
    fn a_conflict_or_an_unknown_state_is_never_success_nor_a_new_create() {
        for answer in [json_answer(json!({"state":"conflict"})), json_answer(json!({"state":"delivered"})), json_answer(json!({"state":"teleported"})), Transport::Answer { status: 502, body: None, retry_after: None }] {
            let r = script(vec![answer]);
            let (s, _) = run(&r);
            assert!(matches!(s, State::Unconfirmed { .. }), "{s:?}");
            assert_eq!(r.creates.get(), 1);
        }
    }

    #[test]
    fn a_status_check_keeps_unconfirmed_unless_the_answer_is_definite() {
        assert!(matches!(after_check(Reading::Unclear, 5), State::Unconfirmed { checked_at: 5 }));
        assert!(matches!(after_check(read(&json_answer(json!({"state":"unknown_id"})), "a/b", 5), 5), State::NotSent { why: NotSent::NotReceived, .. }));
        assert!(matches!(after_check(read(&Transport::NotConnected { offline: true }, "a/b", 5), 5), State::Unconfirmed { .. }), "offline while unconfirmed says nothing about the first attempt");
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
    }
}
