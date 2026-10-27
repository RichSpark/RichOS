//! **SENDING: ONE APPROVED REPORT, ONE REQUEST, THE ISSUE LINK OR "NOT SENT"** (the CEO's
//! §140, 2026-10-22: "dead-simple as possible, avoid any over-engineering and ensure that the
//! core functionality always works").
//!
//!   - **Send sends what the review shows.** [`approve`] takes the exact title, body and
//!     screenshot manifest at that moment under a report ID; the engine saves it before anything is
//!     sent. The report stays editable: the person can change it and press Send again.
//!   - **Send runs the live relay's calls straight through** ([`send`]; the relay is
//!     `richos/mobile/service/RICHDOCTOR-RELAY.md`, protocol 2): register the approved text and
//!     manifest, upload each approved screenshot, complete. The relay stores the screenshots
//!     privately and files one GitHub issue whose numbered staff-only links are the body's last
//!     lines ([`crate::report::ending`]). The issue link is "Report submitted"; anything else at
//!     any step is "Not sent yet", with the relay's reason when it gives one.
//!   - **Send again** starts over under a fresh report ID, with the report as it is then. A rare duplicate issue is acceptable; the team closes it. Nothing about a
//!     send is kept for later: its one-time receipt lives only for the calls of that send.
use crate::image_profile::Entry;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

/// The report ID's alphabet: no 0, 1, I, L, O or U, so it reads aloud and copies cleanly.
pub const ID_ALPHABET: &[u8; 30] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The production relay and repository, compiled in. The repository is RichOS's address from the
/// GitHub move on; a link the relay answers on its address from before the move is the same
/// repository and is accepted too ([`richos_core::github_home::accepted`]).
pub const RELAY: &str = "https://report.richos.ceo";
pub const REPOSITORY: &str = richos_core::github_home::HOME;
/// The test override, both set together or neither (agreed with the relay owner).
pub const RELAY_ENV: &str = "RICHDOCTOR_REPORT_RELAY";
pub const REPOSITORY_ENV: &str = "RICHDOCTOR_REPORT_REPOSITORY";

/// One call's bound: a screenshot upload is up to 5 MiB (PRD §8's upload cap).
pub const SEND_BOUND: Duration = Duration::from_secs(300);

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

/// The staff-only link the issue carries for screenshot `n` (the relay's read route, behind
/// Cloudflare Access): `<relay>/v1/reports/<id>/attachments/<n>`.
pub fn attachment_link(relay: &str, report_id: &str, n: u32) -> String {
    format!("{}/v1/reports/{report_id}/attachments/{n}", relay.trim_end_matches('/'))
}

/// **THE APPROVED REPORT**, final: the exact title, body and screenshot manifest the person
/// approved, under its report ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approved {
    pub report_id: String,
    pub title: String,
    pub body: String,
    /// The approved screenshots, in order (their bytes are the draft's `shots/<id>`).
    #[serde(default)]
    pub manifest: Vec<Entry>,
    pub approved_at: u64,
    pub installation_id: String,
    /// A RichOS feature idea (Request a feature), posted by the relay as a Discussion under Ideas
    /// rather than an issue: schema [`IDEA_SCHEMA`], no screenshots. Never set by RichDoctor.
    #[serde(default)]
    pub idea: bool,
}

/// The register schema of a RichOS feature idea (`RICHDOCTOR-RELAY.md`, "Feature ideas").
pub const IDEA_SCHEMA: &str = "richos-idea/1";

/// Freeze the approved text and screenshot manifest under `report_id`, exactly as the person
/// reviewed it. Refused only when the manifest breaks the limits or the body does not end with
/// the screenshot lines and the footer.
pub fn approve(report_id: &str, title: &str, body: &str, manifest: Vec<Entry>, relay: &str, now: u64, installation_id: &str) -> Result<Approved, Vec<crate::scrub::Problem>> {
    let problem = |d: String| vec![crate::scrub::Problem { item: "body".into(), detail: d }];
    if !crate::util::is_uuid(installation_id) {
        return Err(problem("RichDoctor could not make its installation ID, so the report was not sent.".into()));
    }
    crate::image_profile::check_set(&manifest).map_err(|e| problem(format!("The screenshots are over the limit: {e}.")))?;
    if !body.ends_with(&crate::report::ending(relay, report_id, manifest.len())) {
        return Err(problem("The report's screenshot lines or its report ID footer are missing.".into()));
    }
    Ok(Approved { report_id: report_id.into(), title: title.into(), body: body.into(), manifest, approved_at: now, installation_id: installation_id.into(), idea: false })
}

impl Approved {
    /// The registration's body (the relay's field names): the approved text and manifest, the
    /// fingerprint over them, and this send's one-time receipt.
    pub fn register_body(&self, receipt_secret: &str) -> Value {
        let content = crate::util::sha256_hex(format!("{}\n\n{}\n\n{}", self.title, self.body, crate::image_profile::canonical(&self.manifest)).as_bytes());
        json!({
            "schema": if self.idea { IDEA_SCHEMA } else { "richdoctor-report/2" },
            "report_id": self.report_id,
            "receipt_secret": receipt_secret,
            "content_sha256": content,
            "installation_id": self.installation_id,
            "app_version": crate::VERSION,
            "approved_at": self.approved_at,
            "title": self.title,
            "body": self.body,
            "manifest": self.manifest,
        })
    }
}

/// Why a report was not sent, in plain words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cause", rename_all = "kebab-case")]
pub enum NotSent {
    /// The Mac is confirmed offline.
    Offline,
    /// The connection failed before anything reached the service.
    Unreachable,
    /// No answer came back (a timeout, a connection lost on the way): it may or may not have
    /// been filed.
    NoAnswer,
    /// The service answered without filing it, with its reason when it gave one.
    Refused { detail: String, until: Option<u64> },
    /// An approved screenshot could not be read on this Mac, so nothing was sent.
    Screenshot { item: String },
}

impl NotSent {
    /// The plain reason ("Not sent yet").
    pub fn reason(&self) -> String {
        match self {
            NotSent::Offline => "Your Mac is not connected to the internet. Send again when you are online.".into(),
            NotSent::Unreachable => "RichDoctor could not reach the reporting service. Send again or email support.".into(),
            NotSent::NoAnswer => "RichDoctor did not hear back from the reporting service, so it may not have been filed. Send again or email support.".into(),
            NotSent::Refused { detail, until } => match until.map(crate::util::local_clock) {
                Some(t) => format!("The reporting service did not file it: {} You can send again after {t}.", detail.trim()),
                None => format!("The reporting service did not file it: {} Send again or email support.", detail.trim()),
            },
            NotSent::Screenshot { item } => format!("{item} could not be read on this Mac, so nothing was sent. Email support, or start a new report."),
        }
    }
}

/// Where an approved report is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum State {
    /// Approved and saved; the one request is out.
    Sending,
    /// The relay answered with the issue's link on the configured repository.
    Submitted { issue_url: String, issue_number: u64, at: u64 },
    /// Not filed, as far as RichDoctor knows: Send again sends the same report.
    NotSent { why: NotSent, at: u64 },
}

impl State {
    /// Whether Send again is offered: anything but submitted (and not while sending).
    pub fn may_send(&self) -> bool {
        matches!(self, State::NotSent { .. })
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
    fn put(&self, url: &str, headers: &[(String, String)], bytes: Vec<u8>) -> Transport;
}

/// An issue link is accepted only on the configured repository, as a plain issue number.
pub fn issue_link_ok(url: &str, number: Option<u64>, repository: &str) -> Option<u64> {
    link_ok(url, number, repository, "issues")
}

/// A link on the configured repository's `kind` page (`issues`, or `discussions` for an idea), as a
/// plain number. RichOS's repository has two addresses across the GitHub move, its old one and its
/// new one, and a link on either is accepted; any other repository's is not.
fn link_ok(url: &str, number: Option<u64>, repository: &str, kind: &str) -> Option<u64> {
    use richos_core::github_home::{accepted, page_number};
    let n = page_number(url, &accepted(repository), kind)?;
    number.is_none_or(|k| k == n).then_some(n)
}

/// The approved image `n`'s bytes, from the draft (the caller's loader).
pub type Images<'a> = &'a dyn Fn(&Entry) -> Option<Vec<u8>>;

/// What one relay answer means: `delivered` with the issue link on the configured repository
/// (the issue and every screenshot confirmed), a step that went through (`next`), or not sent.
/// Anything else, `created_checking_images` (fewer screenshots confirmed than sent) included,
/// is not sent: Send again is offered.
fn read(t: &Transport, next: &[&str], repository: &str, kind: &str, now: u64) -> Result<Option<State>, State> {
    let not_sent = |why: NotSent| State::NotSent { why, at: now };
    match t {
        Transport::NotConnected { offline: true } => Err(not_sent(NotSent::Offline)),
        Transport::NotConnected { offline: false } => Err(not_sent(NotSent::Unreachable)),
        Transport::Unclear => Err(not_sent(NotSent::NoAnswer)),
        Transport::Answer { status, body, retry_after } => {
            let b = body.clone().unwrap_or(Value::Null);
            let state = b["state"].as_str().unwrap_or("");
            if (200..300).contains(status) {
                if next.contains(&state) {
                    return Ok(None);
                }
                if let (true, Some(u)) = (state == "delivered", b["issue_url"].as_str()) {
                    if let Some(n) = link_ok(u, b["issue_number"].as_u64(), repository, kind) {
                        return Ok(Some(State::Submitted { issue_url: u.to_string(), issue_number: n, at: now }));
                    }
                }
            }
            let detail = match (b["detail"].as_str(), state) {
                (Some(d), _) => d.chars().take(400).collect(),
                (None, "limited") => "it is busy right now.".into(),
                (None, "closed") => "it is not taking reports right now.".into(),
                _ => "it could not be filed this time.".into(),
            };
            let until = b["retry_after_seconds"].as_u64().or(*retry_after).map(|s| now + s * 1000);
            Err(not_sent(NotSent::Refused { detail, until }))
        }
    }
}

/// **SEND** the approved report, straight through: every approved screenshot read and checked
/// against its manifest hash, then register, upload each, complete. Returns where it stands.
pub fn send(approved: &Approved, images: Images, dest: &Destination, relay: &dyn Relay, now: u64) -> State {
    let mut bytes = Vec::with_capacity(approved.manifest.len());
    for e in &approved.manifest {
        match images(e).filter(|b| crate::util::sha256_hex(b) == e.sha256) {
            Some(b) => bytes.push(b),
            None => return State::NotSent { why: NotSent::Screenshot { item: format!("Screenshot {}", e.order) }, at: now },
        }
    }
    let Ok(secret) = crate::util::random_bytes(32).map(|b| crate::util::base64url(&b)) else {
        return State::NotSent { why: NotSent::Refused { detail: "RichDoctor could not make the report's one-time key.".into(), until: None }, at: now };
    };
    let base = format!("{}/v1/reports", dest.relay);
    let steps = ["receiving", "ready"];
    // An idea comes back as a discussion of the configured repository, a report as an issue.
    let kind = if approved.idea { "discussions" } else { "issues" };
    if let Err(s) = read(&relay.post(&base, &approved.register_body(&secret)), &steps, &dest.repository, kind, now) {
        return s;
    }
    for (e, b) in approved.manifest.iter().zip(bytes) {
        let headers = [("X-Receipt-Secret".to_string(), secret.clone()), ("Content-Type".to_string(), e.media_type.clone()), ("X-Content-SHA256".to_string(), e.sha256.clone())];
        let url = format!("{base}/{}/uploads/{}", approved.report_id, e.attachment_id);
        if let Err(s) = read(&relay.put(&url, &headers, b), &steps, &dest.repository, kind, now) {
            return s;
        }
    }
    let done = relay.post(&format!("{base}/{}/complete", approved.report_id), &json!({ "receipt_secret": secret }));
    match read(&done, &[], &dest.repository, kind, now) {
        Ok(Some(s)) | Err(s) => s,
        Ok(None) => State::NotSent { why: NotSent::Refused { detail: "it could not be filed this time.".into(), until: None }, at: now },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const BASE: &str = "http://127.0.0.1:9";
    const INST: &str = "0b6f3c2e-5d41-4a8e-9f27-3c1d2e4f5a6b";
    const LINK: &str = "https://github.com/WebDevBooster/richos-test/issues/42";

    fn dest() -> Destination {
        Destination { relay: BASE.into(), repository: "WebDevBooster/richos-test".into() }
    }

    fn png(n: u8) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, 1, 1);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&[n, 0, 0, 255]).unwrap();
        }
        out
    }

    fn approved_with(k: u8) -> (Approved, Vec<Vec<u8>>) {
        let id = "RD-7K3M-Q9VX";
        let images: Vec<Vec<u8>> = (1..=k).map(png).collect();
        let manifest: Vec<Entry> = images.iter().enumerate().map(|(i, b)| Entry::of(i as u32 + 1, b).unwrap()).collect();
        let body = format!("## What happened\n\nIt closes.\n\n{}", crate::report::ending(BASE, id, manifest.len()));
        (approve(id, "RichOS closes at start", &body, manifest, BASE, 1_000, INST).unwrap(), images)
    }

    /// A scripted relay: answers in order; keeps every request (an upload's body as its bytes).
    struct Script {
        answers: RefCell<Vec<Transport>>,
        sent: RefCell<Vec<(String, Value, Vec<u8>)>>,
    }
    impl Relay for Script {
        fn post(&self, url: &str, body: &Value) -> Transport {
            self.sent.borrow_mut().push((format!("POST {}", url.trim_start_matches(BASE)), body.clone(), vec![]));
            self.next()
        }
        fn put(&self, url: &str, headers: &[(String, String)], bytes: Vec<u8>) -> Transport {
            assert!(headers.iter().any(|(k, _)| k == "X-Receipt-Secret"));
            self.sent.borrow_mut().push((format!("PUT {}", url.trim_start_matches(BASE)), Value::Null, bytes));
            self.next()
        }
    }
    impl Script {
        fn next(&self) -> Transport {
            let mut a = self.answers.borrow_mut();
            if a.is_empty() { Transport::Unclear } else { a.remove(0) }
        }
    }
    fn script(answers: Vec<Transport>) -> Script {
        Script { answers: RefCell::new(answers), sent: RefCell::new(vec![]) }
    }
    fn ok(v: Value) -> Transport {
        Transport::Answer { status: 201, body: Some(v), retry_after: None }
    }

    #[test]
    fn send_registers_uploads_and_completes_straight_through_with_the_approved_text_and_screenshots() {
        let (a, images) = approved_with(2);
        let r = script(vec![
            ok(json!({"state": "receiving", "missing": ["1", "2"]})),
            ok(json!({"state": "receiving", "missing": ["2"]})),
            ok(json!({"state": "ready", "missing": []})),
            ok(json!({"state": "delivered", "issue_url": LINK, "issue_number": 42})),
        ]);
        let s = send(&a, &|e: &Entry| images.get(e.order as usize - 1).cloned(), &dest(), &r, 5);
        assert!(matches!(s, State::Submitted { issue_number: 42, .. }), "{s:?}");
        let sent = r.sent.borrow();
        let calls: Vec<&str> = sent.iter().map(|c| c.0.as_str()).collect();
        assert_eq!(calls, ["POST /v1/reports", "PUT /v1/reports/RD-7K3M-Q9VX/uploads/1", "PUT /v1/reports/RD-7K3M-Q9VX/uploads/2", "POST /v1/reports/RD-7K3M-Q9VX/complete"]);
        let register = &sent[0].1;
        assert_eq!((register["title"].as_str(), register["body"].as_str()), (Some(a.title.as_str()), Some(a.body.as_str())), "the approved text, exactly");
        assert_eq!(register["schema"], "richdoctor-report/2");
        assert_eq!(register["manifest"][1]["filename"], "screenshot-2.png");
        assert_eq!((&sent[1].2, &sent[2].2), (&images[0], &images[1]), "the screenshots exactly as approved");
        assert_eq!(sent[3].1["receipt_secret"], register["receipt_secret"], "one receipt for the whole send");
    }

    /// The live relay answers `created_checking_images` when fewer screenshots were confirmed
    /// than sent (report-worker.mjs view()): that is not "Report submitted".
    #[test]
    fn an_issue_with_screenshots_not_all_confirmed_is_not_sent() {
        let (a, images) = approved_with(1);
        let r = script(vec![
            ok(json!({"state": "ready", "missing": []})),
            ok(json!({"state": "ready", "missing": []})),
            ok(json!({"state": "created_checking_images", "issue_url": LINK, "issue_number": 42, "images_confirmed": 0, "images_expected": 1})),
        ]);
        let s = send(&a, &|e: &Entry| images.get(e.order as usize - 1).cloned(), &dest(), &r, 5);
        assert!(matches!(s, State::NotSent { .. }), "only delivered is success: {s:?}");
        assert!(s.may_send(), "Send again is offered");
    }

    #[test]
    fn anything_but_the_issue_link_is_not_sent_and_says_why() {
        let (a, images) = approved_with(1);
        let load = |e: &Entry| images.get(e.order as usize - 1).cloned();
        // Each answer stops the send where it comes: at registration here.
        for (answer, words) in [
            (Transport::NotConnected { offline: true }, "not connected to the internet"),
            (Transport::NotConnected { offline: false }, "could not reach"),
            (Transport::Unclear, "did not hear back"),
            (Transport::Answer { status: 503, body: Some(json!({"error": "closed", "detail": "Reports are paused for today."})), retry_after: None }, "Reports are paused for today."),
            (Transport::Answer { status: 500, body: None, retry_after: None }, "could not be filed this time"),
            // A link on another repository is not this report's issue.
            (ok(json!({"state": "delivered", "issue_url": "https://github.com/someone/else/issues/1", "issue_number": 1})), "could not be filed"),
        ] {
            let s = send(&a, &load, &dest(), &script(vec![answer]), 5);
            let State::NotSent { why, .. } = &s else { panic!("{s:?}") };
            assert!(why.reason().contains(words), "{words}: {}", why.reason());
            assert!(s.may_send(), "Send again is offered");
        }
        // A screenshot that is not the approved one: nothing is sent at all.
        let r = script(vec![]);
        let s = send(&a, &|_: &Entry| Some(png(99)), &dest(), &r, 5);
        assert!(matches!(s, State::NotSent { why: NotSent::Screenshot { .. }, .. }), "{s:?}");
        assert!(r.sent.borrow().is_empty());
    }

    /// A RichOS feature idea: schema richos-idea/1, register then complete, and the discussion's
    /// link on the configured repository is "submitted"; an issue link is not this idea's post.
    #[test]
    fn an_idea_registers_as_an_idea_and_its_discussion_link_is_submitted() {
        let id = "RD-7K3M-Q9VX";
        let body = format!("### The idea\n\nPin a conversation\\.\n\n{}", crate::report::ending(BASE, id, 0));
        let mut a = approve(id, "Pin a conversation", &body, vec![], BASE, 1_000, INST).unwrap();
        a.idea = true;
        let link = "https://github.com/WebDevBooster/richos-test/discussions/9";
        let r = script(vec![ok(json!({"state": "ready", "missing": []})), ok(json!({"state": "delivered", "issue_url": link, "issue_number": 9}))]);
        let s = send(&a, &|_: &Entry| None, &dest(), &r, 5);
        assert_eq!(s, State::Submitted { issue_url: link.into(), issue_number: 9, at: 5 });
        let sent = r.sent.borrow();
        assert_eq!(sent[0].1["schema"], IDEA_SCHEMA);
        assert_eq!(sent[0].1["manifest"], json!([]));
        let as_issue = script(vec![ok(json!({"state": "ready"})), ok(json!({"state": "delivered", "issue_url": LINK, "issue_number": 42}))]);
        assert!(matches!(send(&a, &|_: &Entry| None, &dest(), &as_issue, 5), State::NotSent { .. }), "an issue link is not an idea's discussion");
    }

    /// THE GITHUB MOVE (CEO, 2026-10-27): the production relay files on `WebDevBooster/richos`
    /// until it is switched, and on `RichSpark/RichOS` after. A report or idea sent before, during
    /// or after shows as submitted with whichever repository the relay names, and a link on any
    /// other repository is still not this report's post.
    #[test]
    fn the_production_link_check_accepts_the_old_and_the_new_repository_and_nothing_else() {
        let production = Destination { relay: BASE.into(), repository: REPOSITORY.into() };
        let (a, images) = approved_with(0);
        let mut idea = a.clone();
        idea.idea = true;
        let delivered = |url: &str, n: u64| script(vec![ok(json!({"state": "ready"})), ok(json!({"state": "delivered", "issue_url": url, "issue_number": n}))]);
        let load = |e: &Entry| images.get(e.order as usize - 1).cloned();
        for (report, url, n) in [
            (&a, "https://github.com/WebDevBooster/richos/issues/412", 412),
            (&a, "https://github.com/RichSpark/RichOS/issues/413", 413),
            (&idea, "https://github.com/WebDevBooster/richos/discussions/9", 9),
            (&idea, "https://github.com/RichSpark/RichOS/discussions/10", 10),
        ] {
            assert_eq!(send(report, &load, &production, &delivered(url, n), 5), State::Submitted { issue_url: url.into(), issue_number: n, at: 5 }, "{url}");
            assert_eq!(issue_link_ok(url, Some(n), REPOSITORY).is_some(), url.contains("/issues/"), "{url}");
        }
        for url in [
            "https://github.com/RichSpark/RichOS-HQ/issues/1",
            "https://github.com/WebDevBooster/richos-hq/issues/1",
            "https://github.com/WebDevBooster/richos-report-test/issues/1",
            "https://github.com/RichSpark/richos/issues/1",
            "https://github.com/RichCore/RichOS/issues/1",
            "https://github.com/someone/else/issues/1",
        ] {
            assert!(matches!(send(&a, &load, &production, &delivered(url, 1), 5), State::NotSent { .. }), "{url}");
            assert_eq!(issue_link_ok(url, None, REPOSITORY), None, "{url}");
        }
        // A test relay's repository stays one repository: it never accepts RichOS's.
        let test = dest();
        assert!(matches!(send(&a, &load, &test, &delivered("https://github.com/RichSpark/RichOS/issues/5", 5), 5), State::NotSent { .. }));
        assert!(matches!(send(&a, &load, &test, &delivered(LINK, 42), 5), State::Submitted { issue_number: 42, .. }));
    }

    #[test]
    fn ids_are_readable_and_random() {
        for _ in 0..50 {
            assert!(is_report_id(&new_report_id().unwrap()));
        }
        assert!(!is_report_id("RD-0000-1111"));
    }

    /// PRD V7 C4: no scanner veto after Claude and the person's review.
    #[test]
    fn approval_sends_what_was_reviewed() {
        let id = "RD-7K3M-Q9VX";
        let ending = crate::report::ending(BASE, id, 0);
        let kept = richos_core::bug_report::escaped_lines("mail jane@example.com, the file /Users/jane/notes.txt");
        assert!(approve(id, "t", &format!("{kept}\n\n{ending}"), vec![], BASE, 1, INST).is_ok(), "the reviewed words go");
    }

    #[test]
    fn approval_refuses_a_missing_ending_and_too_many_images() {
        let id = "RD-7K3M-Q9VX";
        let ending = crate::report::ending(BASE, id, 0);
        assert!(approve(id, "t", "no footer", vec![], BASE, 1, INST).is_err());
        assert!(approve(id, "t", &format!("x\n\n{ending}"), vec![], BASE, 1, "inst-173fb086d09f").is_err(), "the relay refuses a non-UUID installation ID");
        assert!(approve(id, "t", &format!("x\n\n{ending}"), vec![], BASE, 1, INST).is_ok());
        let (a, _) = approved_with(1);
        assert!(approve(id, "t", &format!("x\n\n{ending}"), a.manifest.clone(), BASE, 1, INST).is_err(), "the screenshot lines must match the manifest");
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
