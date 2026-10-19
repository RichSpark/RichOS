//! Getting the speech model onto this machine — the judgment half.
//!
//! **WHY THIS EXISTS.** `.github/README.md` told every stranger who downloaded RichOS *"Voice
//! does not work yet. Typing does. Speech needs a model this build does not download for you."*
//! Everything under that sentence was already built: `stt.rs` resolves a model and refuses
//! calmly when there is none, `hardware.rs` decides which model this machine should carry,
//! `toolchain.rs` refuses weights that are not the pinned weights, and
//! `engine/voice/models/model-pins.json` pins the exact bytes of every model RichOS may fetch.
//! The one missing piece was the act of fetching — so a customer's Mac had a product that knew
//! precisely which file it needed and no way to get it.
//!
//! ## The split, and it is deliberately the same split the JavaScript already uses
//!
//! `engine/voice/provisioning/model-integrity.js` is pure and decides everything;
//! `model-fetch.js` does the network and decides nothing. This module is the Rust half of that
//! same shape, and the reason is the same: **the failure paths have to be testable without a
//! network.** Everything here is pure or touches only the local filesystem. The transport —
//! sockets, TLS, redirects, `Range` headers — lives in the Tauri shell (`src-tauri/src/voice_provision.rs`),
//! which makes no judgments of its own and asks this module what every byte it received means.
//!
//! That is also why this crate gains no HTTP dependency. `engine/voice/README.md` states the
//! boundary in as many words — *"The crate stays in its original Cargo workspace and does not
//! call Node"* — so the JavaScript could not simply be invoked, and a `reqwest` in here would
//! drag an async runtime into a crate whose whole job is to stay cheap to build and test.
//!
//! ## Ported, not reinvented — and the port is checked against its source
//!
//! Every rule below has a named counterpart in `model-integrity.js`, including the two that look
//! like details and are not:
//!
//! - **Order of diagnosis: absent → empty → CONTENT → size → hash.** Content beats size on
//!   purpose. A captive portal's login page that happens to be the wrong length must be reported
//!   as a login page: *"you are behind a wifi portal"* is an instruction, where *"expected
//!   487,614,201 bytes, got 3,104"* is a puzzle.
//! - **A failed hash is deleted, never quarantined, and never retried automatically.** Truncation
//!   IS retried, because truncation is what a flaky connection does and resuming is cheap.
//!
//! ## Two vocabularies, on purpose
//!
//! [`Finding::describe`] is the engineer's sentence and carries the numbers, the hashes and the
//! file name — it goes to stderr and into the record, which is where that detail is worth
//! something. [`Finding::ceo_message`] is what the CEO reads, and it follows the rule
//! `SttError::ceo_message` already set for this product: no paths, no exit codes, no model
//! filenames. Sizes survive into it, because a size is a fact about his disk rather than a
//! detail of our implementation.

use crate::toolchain::MODEL_PINS_JSON;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// How many leading bytes are enough to tell a model from a web page. Mirrors
/// `model-integrity.js::SNIFF_BYTES`.
pub const SNIFF_BYTES: usize = 1024;

/// How many attempts a download gets before it stops and says so.
///
/// THREE, and the number is the JavaScript's (`model-fetch.js::MAX_ATTEMPTS`) rather than a fresh
/// choice — one fetch path that stops after two and another that stops after five would be two
/// products. What matters more than the number is that there IS one: a retry loop with no bound
/// is how a transient CDN fault becomes an overnight download that nobody asked for.
pub const MAX_ATTEMPTS: u32 = 3;

/// The headroom factor on the disk check: the model plus 10%.
///
/// The extra 10% covers the `.part` file coexisting with filesystem overhead, NOT a second copy —
/// the final rename is in-place. Quoted from `model-catalog.js::requiredFreeBytes`, and the reason
/// given there is timing rather than arithmetic: finding out at 95% of 487.6 MB is the worst
/// possible moment to learn the disk is full.
const HEADROOM_NUMERATOR: u64 = 11;
const HEADROOM_DENOMINATOR: u64 = 10;

// ---------------------------------------------------------------------------------------------
// The pin table.
// ---------------------------------------------------------------------------------------------

/// One pinned model: the exact bytes RichOS means by an id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub id: String,
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
    /// Which independent witnesses vouch for the hash. A single-witness pin is honest and weaker,
    /// and [`Pin::single_witness`] is how a caller can say so rather than present them as equal.
    pub provenance: Vec<String>,
}

impl Pin {
    /// True when only one party vouches for this hash.
    ///
    /// Worth asking, because a pin taken solely from the host that serves the file is a check
    /// against corruption and a stale CDN object and NOT against that host. `model-pins.json`
    /// carries exactly one such entry and its own `witness` field says so.
    pub fn single_witness(&self) -> bool {
        self.provenance.len() < 2
    }

    /// Free bytes a fetch of this model should require before it starts.
    pub fn required_free_bytes(&self) -> u64 {
        // Integer ceiling of bytes * 1.1, so the answer does not depend on float rounding.
        self.required_free_bytes_resuming(0)
    }

    /// Free bytes a fetch should require when `resume_from` bytes are already on disk.
    ///
    /// The partial file is appended to in place, so only the MISSING bytes need new room; the same
    /// 10% headroom is kept on that remainder. `resume_from == 0` is the whole-model figure.
    pub fn required_free_bytes_resuming(&self, resume_from: u64) -> u64 {
        let missing = self.bytes.saturating_sub(resume_from);
        (missing * HEADROOM_NUMERATOR).div_ceil(HEADROOM_DENOMINATOR)
    }
}

/// The parsed pin table, from the ONE place it lives.
///
/// `toolchain.rs` owns the `include_str!` and this module borrows it. A second `include_str!` of
/// the same file would compile perfectly and would be exactly the defect `model-pins.json`'s own
/// header warns about — *"two registries of truth is how the second unpinned consumer happened in
/// the first place"*.
fn table() -> Value {
    serde_json::from_str(MODEL_PINS_JSON).expect("model-pins.json is malformed")
}

/// Every pinned model id, in table order.
pub fn pinned_ids() -> Vec<String> {
    table()["models"]
        .as_array()
        .map(|a| a.iter().filter_map(|m| m["id"].as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// The pin for a model id, or `None` if RichOS does not pin that model.
///
/// NONE IS NOT "FINE". An unpinned model is a model we cannot verify, and every fetch path must
/// refuse rather than fall back to size-and-magic — that fallback is the hole the pin table was
/// written to close.
pub fn pin_for(model_id: &str) -> Option<Pin> {
    let t = table();
    for m in t["models"].as_array()? {
        if m["id"].as_str()? == model_id {
            return Some(Pin {
                id: m["id"].as_str()?.to_string(),
                file: m["file"].as_str()?.to_string(),
                bytes: m["bytes"].as_u64()?,
                sha256: m["sha256"].as_str()?.to_ascii_lowercase(),
                provenance: m["provenance"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
                    .unwrap_or_default(),
            });
        }
    }
    None
}

/// Where pinned models are fetched from. One host, HTTPS, no mirrors.
pub fn base_url() -> String {
    table()["baseUrl"].as_str().unwrap_or_default().to_string()
}

/// The download URL for a pinned model.
pub fn model_url(pin: &Pin) -> String {
    format!("{}/{}", base_url(), pin.file)
}

/// whisper.cpp's GGML magic as it appears on disk — the uint32 `0x67676d6c` little-endian, so the
/// first four bytes are `6c 6d 67 67`. Read from the table rather than spelled out here.
pub fn ggml_magic_hex() -> String {
    table()["ggmlMagicHex"].as_str().unwrap_or("6c6d6767").to_ascii_lowercase()
}

// ---------------------------------------------------------------------------------------------
// Pure judgment.
// ---------------------------------------------------------------------------------------------

/// What a run of bytes looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Ggml,
    Html,
    Gzip,
    Zip,
    Text,
    Binary,
    Empty,
}

/// What do these first bytes look like? Mirrors `model-integrity.js::sniffBody`.
pub fn sniff_body(head: &[u8]) -> Shape {
    if head.is_empty() {
        return Shape::Empty;
    }
    if head.len() >= 4 && hex4(&head[..4]) == ggml_magic_hex() {
        return Shape::Ggml;
    }
    if head.len() >= 2 && head[0] == 0x1f && head[1] == 0x8b {
        return Shape::Gzip;
    }
    if head.len() >= 4 && head[0] == b'P' && head[1] == b'K' {
        return Shape::Zip;
    }

    let sample = &head[..head.len().min(SNIFF_BYTES)];
    // Markup sniffing over the first KB. Captive portals are not tidy: some return a full
    // `<!DOCTYPE html>`, some a bare `<html>`, and some a naked `<meta http-equiv="refresh">`
    // redirect stub with no doctype at all. All three are the same event to the person.
    let text: String = sample.iter().map(|&b| b as char).collect::<String>().to_ascii_lowercase();
    const MARKUP: &[&str] = &["<!doctype html", "<html", "<head", "<body", "<meta ", "<title", "<script"];
    for needle in MARKUP {
        if let Some(at) = text.find(needle) {
            // `<html`, `<head`, `<body`, `<title` and `<script` must be followed by whitespace or
            // `>` to count, so `<headline` in a binary blob is not a web page. `<!doctype html`
            // and `<meta ` already carry their own terminator.
            let needs_terminator = matches!(*needle, "<html" | "<head" | "<body" | "<title" | "<script");
            if !needs_terminator {
                return Shape::Html;
            }
            match text[at + needle.len()..].chars().next() {
                None => return Shape::Html,
                Some(c) if c == '>' || c.is_whitespace() => return Shape::Html,
                Some(_) => {}
            }
        }
    }
    // Printable-ASCII-dominant with no binary noise => somebody sent us a message, not a model.
    let printable = sample
        .iter()
        .filter(|&&b| b == 0x09 || b == 0x0a || b == 0x0d || (0x20..=0x7e).contains(&b))
        .count();
    if printable as f64 / sample.len() as f64 > 0.95 {
        return Shape::Text;
    }
    Shape::Binary
}

/// Every way a candidate model file can be wrong. A KIND, not a message, so callers branch on the
/// situation rather than matching on prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Absent,
    Empty,
    HtmlBody,
    TextBody,
    CompressedBody,
    NotGgml,
    Short,
    Oversize,
    HashMismatch,
    NoSpace,
    Unpinned,
    /// The transport could not deliver. Not a property of any bytes, which is why it is the one
    /// kind this module never produces on its own — the shell reports it.
    Network,
    /// The server answered, and not with the file.
    Http,
}

/// What is wrong, with the numbers needed to say it precisely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub kind: Failure,
    /// What we got — a byte count, or a hex hash, depending on the kind.
    pub have: Option<String>,
    /// What was expected, same encoding as `have`.
    pub want: Option<String>,
    /// The first non-empty line of a rejected body, bounded. Only for HTML/text bodies.
    pub detail: Option<String>,
    /// Was a partial file kept for a resume? Only meaningful for [`Failure::Short`].
    pub resumable: bool,
}

impl Finding {
    fn of(kind: Failure) -> Finding {
        Finding { kind, have: None, want: None, detail: None, resumable: false }
    }
    fn counts(kind: Failure, have: u64, want: u64) -> Finding {
        Finding { have: Some(have.to_string()), want: Some(want.to_string()), ..Finding::of(kind) }
    }
    fn hashes(kind: Failure, have: &str, want: &str) -> Finding {
        Finding { have: Some(have.to_string()), want: Some(want.to_string()), ..Finding::of(kind) }
    }

    fn have_u64(&self) -> u64 {
        self.have.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0)
    }
    fn want_u64(&self) -> u64 {
        self.want.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0)
    }

    /// Could a second attempt plausibly fix this?
    ///
    /// A HASH MISMATCH IS NOT RETRYABLE, and that is the load-bearing entry. Retrying a corrupted
    /// download in a loop is how a transient CDN fault becomes a support ticket; a truncation is
    /// what a flaky connection does, and resuming it is cheap.
    pub fn retryable(&self) -> bool {
        matches!(self.kind, Failure::Empty | Failure::Short | Failure::Network)
    }

    /// **WOULD THE CEO PRESSING "TRY AGAIN" POSSIBLY HELP?** A DIFFERENT QUESTION FROM
    /// [`Finding::retryable`], and the difference is a defect this project's own affordance rule
    /// caught before it shipped.
    ///
    /// `retryable` answers whether the AUTOMATIC attempt loop should go round again, by itself,
    /// within seconds, with nothing about the machine changed. For a captive portal the answer is
    /// plainly no — the next attempt fetches the same login page.
    ///
    /// This answers whether asking again LATER, after the person has done the thing the sentence
    /// names, could succeed. For that same captive portal the answer is plainly yes: `ceo_message`
    /// tells him in as many words to *"Sign in to the network, then ask me again."* Driving the
    /// retry button off `retryable` put that instruction on screen with no button under it — a
    /// state he could change, rendered apart from the thing that changes it, which is exactly what
    /// `ui/tests/affordances.js` exists to refuse. Every one of these sentences names something he
    /// can do and then invites him back.
    ///
    /// `Unpinned` is the one that says no, and it is not about him: a model with no hash in
    /// `model-pins.json` is a fact about this repository's source, and no amount of asking changes
    /// what the shipped table contains.
    pub fn worth_asking_again(&self) -> bool {
        !matches!(self.kind, Failure::Unpinned)
    }

    /// The engineer's sentence: what happened, with every number. stderr and the record.
    ///
    /// Ported from `model-integrity.js::describe`. `on_disk` picks the right next step for a
    /// short file: a stub sitting under a model's name is deleted, where a kept prefix is
    /// resumed, and saying "resume" about an installed file would be advice that goes nowhere.
    pub fn describe(&self, file: &str, on_disk: bool) -> String {
        match self.kind {
            Failure::Unpinned => format!(
                "{file} is not in RichOS's pin table, so there is no hash to check it against — \
                 refusing to install a model that cannot be verified."
            ),
            Failure::Absent => format!("{file} is not on disk — RichOS has no copy of this model to check."),
            Failure::Empty => format!(
                "{file} is empty — the download produced no bytes at all. That usually means the \
                 connection dropped before anything arrived."
            ),
            Failure::HtmlBody => format!(
                "{file} came back as a web page, not a model — the response starts with HTML{}. \
                 That is what a hotel, airport or conference wifi looks like when it is intercepting \
                 the download to show you a login page. Sign in to the network, then try again. \
                 Nothing was installed.",
                self.detail.as_deref().map(|d| format!(" (\"{}\")", truncate(d, 80))).unwrap_or_default()
            ),
            Failure::TextBody => format!(
                "{file} came back as plain text, not a model{}. That is an error message saved under \
                 a model's name. Nothing was installed.",
                self.detail
                    .as_deref()
                    .map(|d| format!(" — the server said: \"{}\"", truncate(d, 120)))
                    .unwrap_or_default()
            ),
            Failure::CompressedBody => format!(
                "{file} came back as a {} archive, not a whisper model. RichOS expects the raw .bin. \
                 Nothing was installed.",
                self.detail.as_deref().unwrap_or("compressed")
            ),
            Failure::NotGgml => format!(
                "{file} is not a whisper model — its first four bytes are {}, and every GGML model \
                 starts with {}. Nothing was installed.",
                self.have.as_deref().unwrap_or("?"),
                self.want.as_deref().unwrap_or("?")
            ),
            Failure::Short if on_disk => format!(
                "{file} is only {} of the {} bytes a real model has ({}) — it is a download that never \
                 finished, not a usable model. Delete it and fetch the model again.",
                commas(self.have_u64()),
                commas(self.want_u64()),
                pct(self.have_u64(), self.want_u64())
            ),
            Failure::Short => format!(
                "{file} is incomplete: {} of {} bytes arrived ({}). {}",
                commas(self.have_u64()),
                commas(self.want_u64()),
                pct(self.have_u64(), self.want_u64()),
                if self.resumable {
                    "The partial download was kept, so the next attempt resumes from where it stopped \
                     rather than starting over."
                } else {
                    "The partial file was discarded; the next attempt starts from the beginning."
                }
            ),
            Failure::Oversize => format!(
                "{file} is larger than the model RichOS pinned: {} bytes where {} were expected. \
                 These are not the bytes we meant. Nothing was installed.",
                commas(self.have_u64()),
                commas(self.want_u64())
            ),
            Failure::HashMismatch => format!(
                "{file} is exactly the right size and starts like a real model, but its contents are \
                 not the bytes RichOS pinned. Expected sha256 {}, got {}. Something between the \
                 download host and this machine changed the file. It has been deleted and nothing \
                 was installed.",
                short_hash(self.want.as_deref().unwrap_or("")),
                short_hash(self.have.as_deref().unwrap_or(""))
            ),
            Failure::NoSpace => format!(
                "Not enough free disk to download {file}: it needs {} free (the model plus 10%) and \
                 this disk has {}. Free up {} and try again. Nothing was started.",
                human(self.want_u64()),
                human(self.have_u64()),
                human(self.want_u64().saturating_sub(self.have_u64()))
            ),
            Failure::Network => format!(
                "{file} could not be downloaded{}.",
                self.detail.as_deref().map(|d| format!(": {d}")).unwrap_or_default()
            ),
            Failure::Http => format!(
                "{file} could not be downloaded: the server answered {}. Nothing was installed.",
                self.detail.as_deref().unwrap_or("an error")
            ),
        }
    }

    /// The CEO's sentence. No paths, no exit codes, no model filenames — the rule
    /// `SttError::ceo_message` already set for this product.
    ///
    /// SIZES SURVIVE, hashes and filenames do not. "There isn't room for a 536 MB download" is a
    /// fact about his disk that he can act on; "expected sha256 c6138d6d58ec…" is a fact about our
    /// implementation that he cannot. The full detail is one `describe` away, on stderr.
    ///
    /// **NAMED `ceo_message` RATHER THAN `ceo_message`, AND THE NAME IS WHAT PUTS IT UNDER THE
    /// AFFORDANCE RULE.** `ui/tests/lib/state-strings.js` scrapes every sentence the CEO can read
    /// into an inventory that `ui/tests/lib/state-registry.js` must classify, and outside
    /// `src-tauri/src` it recognizes exactly one function name: `ceo_message`. Called
    /// `ceo_message`, the eight sentences below — the whole of what he reads when a download
    /// refuses — were invisible to that rule while being the most CEO-facing strings in the
    /// module. It is also simply the right name: `SttError::ceo_message` came first and one
    /// product should not have two words for one idea.
    pub fn ceo_message(&self) -> String {
        match self.kind {
            Failure::NoSpace => format!(
                "There isn't enough room on this disk for my speech model: it needs about {} free \
                 and there's {}. Free up some space and ask me again. Nothing was downloaded.",
                human(self.want_u64()),
                human(self.have_u64())
            ),
            Failure::HtmlBody => "The network sent me a sign-in page instead of my speech model. That's \
                 what hotel, airport and conference wifi does. Sign in to the network, then ask me \
                 again. Nothing was installed."
                .into(),
            Failure::TextBody | Failure::Http => "The server sent back an error instead of my speech \
                 model. Ask me again in a moment. Nothing was installed."
                .into(),
            Failure::CompressedBody | Failure::NotGgml | Failure::Oversize => {
                "What came back isn't my speech model, so I didn't install it. Ask me again when \
                 you're on a network you trust."
                    .into()
            }
            // ITS OWN SENTENCE, AND THE TEST BELOW IS WHY IT HAS ONE. It used to share the line
            // above, which ends "Ask me again when you're on a network you trust" — and no network
            // has anything to do with it. An unpinned model is a fact about what
            // `model-pins.json` contains in the build he is running, so asking again produces the
            // identical refusal forever. `no_failure_sentence_asks_him_to_come_back_without_
            // setting_the_control_flag` caught the mismatch: the shared sentence invited him back
            // while `worth_asking_again` correctly said no, which is an instruction rendered with
            // no control under it. NAMES THE PARTY instead, the way every other unfixable-by-him
            // state in this product does.
            Failure::Unpinned => "I don't have a way to check that this speech model is genuine, so I won't install \
                 it. Whoever set RichOS up can put that right. I can still read what you type."
                .into(),
            Failure::HashMismatch => "What arrived isn't the speech model I was expecting, so I threw it \
                 away rather than listen to you through it. Ask me again when you're on a network you \
                 trust."
                .into(),
            Failure::Empty | Failure::Short | Failure::Network => {
                if self.resumable {
                    "The download stopped partway. I kept what arrived, so asking me again picks up \
                     where it left off rather than starting over."
                        .into()
                } else {
                    "The download stopped before anything useful arrived. Ask me again when the \
                     connection is steadier."
                        .into()
                }
            }
            Failure::Absent => "I don't have a speech model on this machine yet.".into(),
        }
    }
}

/// The verdict on a candidate file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing disqualifying was found. `hashed` says WHICH kind of answer this is — a cheap
    /// check can only ever mean "nothing disqualifying was VISIBLE", and no caller may install on
    /// the strength of a `hashed: false` pass.
    Ok { hashed: bool },
    Bad(Finding),
}

impl Verdict {
    pub fn is_ok(&self) -> bool {
        matches!(self, Verdict::Ok { .. })
    }
    pub fn hashed(&self) -> bool {
        matches!(self, Verdict::Ok { hashed: true })
    }
    pub fn finding(&self) -> Option<&Finding> {
        match self {
            Verdict::Bad(f) => Some(f),
            Verdict::Ok { .. } => None,
        }
    }
}

/// Decide whether a candidate file is the model we pinned.
///
/// `sha256` is optional so the cheap check (stat + 4 bytes) and the full check share ONE rule set.
/// THE ORDER IS THE POINT — absent, empty, content, size, hash — and it is not cheapest-first.
/// See the module docs.
pub fn classify(exists: bool, bytes: u64, head: Option<&[u8]>, sha256: Option<&str>, pin: Option<&Pin>) -> Verdict {
    let Some(pin) = pin else { return Verdict::Bad(Finding::of(Failure::Unpinned)) };
    if !exists {
        return Verdict::Bad(Finding::of(Failure::Absent));
    }
    if bytes == 0 {
        return Verdict::Bad(Finding::of(Failure::Empty));
    }

    if let Some(head) = head.filter(|h| !h.is_empty()) {
        match sniff_body(head) {
            Shape::Html => {
                return Verdict::Bad(Finding { detail: Some(first_line(head)), ..Finding::of(Failure::HtmlBody) })
            }
            Shape::Text => {
                return Verdict::Bad(Finding { detail: Some(first_line(head)), ..Finding::of(Failure::TextBody) })
            }
            Shape::Gzip => {
                return Verdict::Bad(Finding {
                    detail: Some("gzip".into()),
                    ..Finding::of(Failure::CompressedBody)
                })
            }
            Shape::Zip => {
                return Verdict::Bad(Finding { detail: Some("zip".into()), ..Finding::of(Failure::CompressedBody) })
            }
            Shape::Ggml => {}
            Shape::Binary | Shape::Empty => {
                return Verdict::Bad(Finding::hashes(
                    Failure::NotGgml,
                    &hex4(&head[..head.len().min(4)]),
                    &ggml_magic_hex(),
                ))
            }
        }
    }

    if bytes < pin.bytes {
        return Verdict::Bad(Finding::counts(Failure::Short, bytes, pin.bytes));
    }
    if bytes > pin.bytes {
        return Verdict::Bad(Finding::counts(Failure::Oversize, bytes, pin.bytes));
    }

    let Some(sha256) = sha256 else { return Verdict::Ok { hashed: false } };
    if sha256.to_ascii_lowercase() != pin.sha256 {
        return Verdict::Bad(Finding::hashes(Failure::HashMismatch, &sha256.to_ascii_lowercase(), &pin.sha256));
    }
    Verdict::Ok { hashed: true }
}

/// Is there room to do this at all? Asked BEFORE a byte is requested.
///
/// `free_bytes` of `None` means the platform declined to say, and UNKNOWN FREE SPACE IS NOT A
/// REFUSAL — refusing on an answer nobody gave would take voice away from a machine that had room
/// all along. Mirrors `model-integrity.js::diskPreflight`.
pub fn disk_preflight(free_bytes: Option<u64>, need_bytes: u64) -> Option<Finding> {
    let free = free_bytes?;
    if free >= need_bytes {
        return None;
    }
    Some(Finding::counts(Failure::NoSpace, free, need_bytes))
}

/// What the next attempt should do about an existing `.part` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeAction {
    Start,
    Resume,
    Restart,
}

/// The plan, with the reason in words so the record says why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumePlan {
    pub action: ResumeAction,
    pub from: u64,
    pub reason: String,
}

/// Resume, restart, or start. Mirrors `model-integrity.js::resumePlan`.
///
/// TWO CASES REFUSE TO RESUME, and both because resuming would be WRONG rather than merely slow:
/// a partial already at or past the pinned size is not a prefix of the model, it is garbage with
/// the model's name on it; and a partial that does not begin with the GGML magic means the last
/// attempt saved something that was not the beginning of a model — resuming would append real
/// bytes onto a captive portal's login page and hand us a file that is the right length and
/// hashes to nothing.
pub fn resume_plan(part_bytes: u64, total_bytes: u64, part_head: Option<&[u8]>) -> ResumePlan {
    if part_bytes == 0 {
        return ResumePlan { action: ResumeAction::Start, from: 0, reason: "no partial file".into() };
    }
    if part_bytes >= total_bytes {
        return ResumePlan {
            action: ResumeAction::Restart,
            from: 0,
            reason: "the partial file is already at or past the pinned size".into(),
        };
    }
    if let Some(head) = part_head {
        if head.len() >= 4 && sniff_body(head) != Shape::Ggml {
            return ResumePlan {
                action: ResumeAction::Restart,
                from: 0,
                reason: "the partial file does not start like a model, so it is not a prefix worth resuming"
                    .into(),
            };
        }
    }
    ResumePlan {
        action: ResumeAction::Resume,
        from: part_bytes,
        reason: format!("resuming from byte {}", commas(part_bytes)),
    }
}

/// How many bytes of `<name>.part` in `dir` the next fetch would keep and append to (0 when it
/// would start or restart). The offer view uses this so it quotes the same room `plan_fetch` asks.
pub fn resumable_bytes(pin: &Pin, dir: &Path) -> u64 {
    let part = dir.join(format!("{}.part", pin.file));
    let part_bytes = file_bytes(&part);
    let head = if part_bytes > 0 { read_head(&part, SNIFF_BYTES) } else { None };
    let plan = resume_plan(part_bytes, pin.bytes, head.as_deref());
    if plan.action == ResumeAction::Resume { plan.from } else { 0 }
}

/// What the server SAYS it is about to send, as an ABSOLUTE file length.
///
/// On a 206 the `Content-Length` is the remaining bytes, so the resume offset has to be added back
/// before it can be compared with anything. `None` when the server declines to say, which is legal
/// and is not an error — it only means this early check cannot run. Mirrors
/// `model-fetch.js::declaredTotal`, and it is pure so the arithmetic is tested without a socket.
pub fn declared_total(status: u16, content_length: Option<u64>, content_range: Option<&str>, from: u64) -> Option<u64> {
    if let Some(range) = content_range {
        // `bytes 1000-487614200/487614201` — the total is after the slash.
        if let Some((_, total)) = range.rsplit_once('/') {
            if let Ok(n) = total.trim().parse::<u64>() {
                return Some(n);
            }
        }
    }
    let len = content_length?;
    Some(if status == 206 { len + from } else { len })
}

// ---------------------------------------------------------------------------------------------
// Local I/O. Thin, and every decision above is pure and tested without a filesystem.
// ---------------------------------------------------------------------------------------------

/// The first `n` bytes of a file, or `None` if it cannot be read.
pub fn read_head(path: &Path, n: usize) -> Option<Vec<u8>> {
    let mut f = fs::File::open(path).ok()?;
    let mut buf = vec![0u8; n];
    let mut filled = 0;
    while filled < n {
        match f.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(k) => filled += k,
            Err(_) => return None,
        }
    }
    buf.truncate(filled);
    Some(buf)
}

/// Bytes on disk, or 0 when the file is not there.
pub fn file_bytes(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// sha256 of a whole file, streamed so a 1.6 GB model never lands in memory.
///
/// Taken over the COMPLETE file at the end rather than accumulated during the transfer, and that
/// is a RESUMABILITY REQUIREMENT rather than laziness: a resumed download never sees the first
/// half of its own bytes, so an incremental hash would be a hash of the wrong thing.
pub fn hash_file(path: &Path) -> Option<String> {
    let mut f = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

/// Free bytes on the filesystem holding `dir`, or `None` when the platform will not say.
///
/// `statfs` rather than a subprocess, for the reason `toolchain.rs` already gives about hashing
/// in-process: a machine fact parsed out of another program's stdout fails as a plausible number
/// rather than as an error. `libc` is already a direct dependency of this crate (`hardware.rs`
/// uses it for `sysctlbyname`), so this adds no supply-chain surface at all.
#[cfg(unix)]
pub fn free_bytes_for(dir: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    // statfs answers about the filesystem holding a path that EXISTS. A models directory that has
    // never been created yet is the ordinary first-run case, so walk up to the nearest ancestor
    // that is really there rather than reporting "unknown" and skipping the check.
    let mut probe = dir;
    loop {
        if probe.exists() {
            break;
        }
        probe = probe.parent()?;
    }
    let c = CString::new(probe.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    Some((s.f_bavail as u64).saturating_mul(s.f_bsize as u64))
}

#[cfg(not(unix))]
pub fn free_bytes_for(_dir: &Path) -> Option<u64> {
    None
}

/// Where RichOS installs the models IT fetched.
///
/// `~/.config/richos/models`, and the directory matters. The three directories `resolve_model`
/// already searched belong to other software — `~/.config/open-wispr/models` is the dictation
/// app's, `~/Models/Whisper` and `~/.cache/whisper.cpp` are the CEO's own and whisper.cpp's. A
/// product that writes half a gigabyte into another application's directory has made that
/// application's cleanup our outage. `~/.config/richos/` is already RichOS's own (the per-machine
/// whisper toolchain lock lives there), so this is the one place we may write without asking.
///
/// `RICHOS_MODEL_DIR` still wins, because an engineer who names a directory is not asking to be
/// second-guessed — and it is the same variable `resolve_model` consults first, so the two can
/// never disagree about where a model is.
pub fn install_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("RICHOS_MODEL_DIR") {
        if !d.trim().is_empty() {
            return Some(PathBuf::from(expand_tilde(&d)));
        }
    }
    let home = std::env::var("HOME").ok().filter(|h| !h.is_empty())?;
    Some(Path::new(&home).join(crate::stt::RICHOS_MODELS_SUBDIR))
}

/// The full verdict on a file that claims to be a pinned model.
///
/// `deep` defaults true at every call site that decides an install, because the whole point of
/// this work is that the cheap check is NOT an integrity check.
pub fn inspect_file(path: &Path, pin: &Pin, deep: bool) -> Verdict {
    if !path.exists() {
        return Verdict::Bad(Finding::of(Failure::Absent));
    }
    let bytes = file_bytes(path);
    let head = read_head(path, SNIFF_BYTES);
    let cheap = classify(true, bytes, head.as_deref(), None, Some(pin));
    if !cheap.is_ok() || !deep {
        return cheap;
    }
    let Some(sha) = hash_file(path) else {
        return Verdict::Bad(Finding::of(Failure::Absent));
    };
    classify(true, bytes, head.as_deref(), Some(&sha), Some(pin))
}

// ---------------------------------------------------------------------------------------------
// The download, driven by a transport that decides nothing.
// ---------------------------------------------------------------------------------------------

/// What should happen next, decided before a byte is requested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchPlan {
    /// The verified model is already there. Re-downloading 487.6 MB somebody already has is rude.
    AlreadyPresent { path: PathBuf },
    /// Do not start. The disk is too small, or the model is not pinned.
    Refused { finding: Finding },
    /// Go. `from` is 0 for a fresh start and the resume offset otherwise.
    Fetch { url: String, dest: PathBuf, part: PathBuf, from: u64, total: u64, resume_reason: String },
}

/// Decide what a fetch of `pin` into `dir` should do. No network, one stat and at most one hash.
///
/// A PRESENT-BUT-WRONG FILE IS REMOVED HERE. Leaving it would let `resolve_model` pick a file we
/// have just proved is not the model — and `toolchain.rs` would then refuse it at voice-mode
/// start, which is a correct refusal arriving far too late to be useful.
pub fn plan_fetch(pin: &Pin, dir: &Path, free_bytes: Option<u64>) -> FetchPlan {
    let dest = dir.join(&pin.file);
    let part = dir.join(format!("{}.part", pin.file));

    if dest.exists() {
        if inspect_file(&dest, pin, true).is_ok() {
            return FetchPlan::AlreadyPresent { path: dest };
        }
        let _ = fs::remove_file(&dest);
    }

    // A partial of EXACTLY the pinned size may be a complete download that was interrupted before
    // its verification and rename (crash, power loss, quit during "Verifying"). `resume_plan` still
    // calls it unresumable, which is right: appending to it would be wrong. But discarding it
    // unread costs the whole download when one hash could finish the install. Hash it; give it the
    // model's name only when size, magic AND sha256 all agree, otherwise fall through to the
    // restart below.
    if file_bytes(&part) == pin.bytes
        && inspect_file(&part, pin, true).is_ok()
        && fs::rename(&part, &dest).is_ok()
    {
        return FetchPlan::AlreadyPresent { path: dest };
    }

    // The room needed depends on what the partial will do: a resume appends in place and needs only
    // the missing bytes; a start or a restart writes the whole model. Decided BEFORE anything is
    // removed, so a refusal leaves the partial exactly where it was.
    let part_bytes = file_bytes(&part);
    let head = if part_bytes > 0 { read_head(&part, SNIFF_BYTES) } else { None };
    let plan = resume_plan(part_bytes, pin.bytes, head.as_deref());
    let resume_from = if plan.action == ResumeAction::Resume { plan.from } else { 0 };
    if let Some(finding) = disk_preflight(free_bytes, pin.required_free_bytes_resuming(resume_from)) {
        return FetchPlan::Refused { finding };
    }
    if plan.action == ResumeAction::Restart {
        let _ = fs::remove_file(&part);
    }

    FetchPlan::Fetch {
        url: model_url(pin),
        dest,
        part,
        from: resume_from,
        total: pin.bytes,
        resume_reason: plan.reason,
    }
}

/// Once the server has answered, is this response worth reading a body from?
///
/// Asked BEFORE the transfer rather than after it, which is the one check that can save a whole
/// download on a metered or slow connection. `peek` is a BOUNDED prefix of the body — never the
/// whole thing — and it exists so the better sentence wins: a captive portal declares a
/// `Content-Length` too, and *"you are behind a sign-in page"* beats *"the server offered 3,104
/// bytes where RichOS pinned 487,614,201"* every time.
pub fn check_declared(pin: &Pin, declared: Option<u64>, peek: &[u8]) -> Option<Finding> {
    let declared = declared?;
    if declared == pin.bytes {
        return None;
    }
    // Look at what it actually is first. Only a size complaint survives if the bytes look like a
    // model; anything else has a better name.
    if let Verdict::Bad(shape) = classify(true, declared, Some(peek), None, Some(pin)) {
        if !matches!(shape.kind, Failure::Short | Failure::Oversize) {
            return Some(shape);
        }
    }
    Some(Finding::counts(
        if declared < pin.bytes { Failure::Short } else { Failure::Oversize },
        declared,
        pin.bytes,
    ))
}

/// How a download ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Installed { path: PathBuf, bytes: u64, sha256: String, resumed_from: u64 },
    Failed { finding: Finding },
}

/// A download in flight, writing to `<name>.part` and NEVER to the real name.
///
/// STEP 4 OF THE SEQUENCE, and the reason it is a separate type: a resolver searching the model
/// directory can never observe a half-written model, because a half-written model never has a
/// model's name. The rename in [`PartFile::finish`] is the only moment the file becomes visible
/// to `resolve_model`, and it happens only after size, magic AND sha256 have all agreed.
pub struct PartFile {
    pin: Pin,
    part: PathBuf,
    dest: PathBuf,
    file: fs::File,
    received: u64,
    resumed_from: u64,
}

impl PartFile {
    /// Open the part file for a planned fetch. Appends when resuming, truncates when starting.
    pub fn open(pin: &Pin, part: &Path, dest: &Path, from: u64) -> std::io::Result<PartFile> {
        if let Some(parent) = part.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(from > 0)
            .truncate(from == 0)
            .open(part)?;
        Ok(PartFile {
            pin: pin.clone(),
            part: part.to_path_buf(),
            dest: dest.to_path_buf(),
            file,
            received: from,
            resumed_from: from,
        })
    }

    /// Append one chunk. The caller reports progress from [`PartFile::received`].
    ///
    /// NEVER WRITES PAST ONE BYTE OVER THE PINNED SIZE. A response that declared no honest length
    /// can keep sending forever, and every byte beyond the pin is bytes `finish` will reject, so
    /// the writer keeps exactly enough to make `finish` classify the file as oversize (pin + 1)
    /// and drops the rest. The caller stops the transfer when [`PartFile::is_overfull`] is true.
    pub fn write(&mut self, chunk: &[u8]) -> std::io::Result<()> {
        let room = (self.pin.bytes + 1).saturating_sub(self.received);
        let keep = chunk.len().min(room as usize);
        self.file.write_all(&chunk[..keep])?;
        self.received += keep as u64;
        Ok(())
    }

    /// More bytes have arrived than the pinned model has. Nothing more can make this file valid.
    pub fn is_overfull(&self) -> bool {
        self.received > self.pin.bytes
    }

    /// Bytes on disk for this download, resume offset included.
    pub fn received(&self) -> u64 {
        self.received
    }

    pub fn total(&self) -> u64 {
        self.pin.bytes
    }

    /// The transfer stopped early. The prefix is KEPT so the next attempt resumes.
    ///
    /// **RETURNS A `Finding`, NOT AN `Outcome`**, because an interruption has exactly one shape
    /// and a signature that admits a success the caller must then rule out is a signature that
    /// invites the caller to forget.
    ///
    /// Deliberately NOT a verification path: an interrupted transfer has nothing to verify, and
    /// running the whole-file hash over a known-partial file would burn a second per 500 MB to
    /// learn what the byte count already said.
    pub fn interrupted(self, detail: &str) -> Finding {
        let got = self.received;
        drop(self.file);
        // A prefix that does not start like a model is not worth keeping: the next attempt would
        // refuse to resume onto it anyway (`resume_plan`), and leaving it costs disk for nothing.
        let head = read_head(&self.part, SNIFF_BYTES);
        let keepable = got > 0 && head.as_deref().map(|h| h.len() < 4 || sniff_body(h) == Shape::Ggml).unwrap_or(false);
        if !keepable {
            let _ = fs::remove_file(&self.part);
        }
        Finding {
            detail: Some(detail.to_string()),
            resumable: keepable,
            ..Finding::counts(Failure::Short, got, self.pin.bytes)
        }
    }

    /// Verify the WHOLE file — size, GGML magic, then sha256 over every byte — and only then give
    /// it a model's name.
    ///
    /// A FAILED HASH IS DELETED, NOT QUARANTINED. There is no `.bad` file, no "keep it in case",
    /// no second-chance path the resolver could ever read. A SHORT file is the one exception and
    /// it is kept, because a kept prefix is what makes the next attempt cheap.
    pub fn finish(self) -> Outcome {
        let pin = self.pin.clone();
        let part = self.part.clone();
        let dest = self.dest.clone();
        let resumed_from = self.resumed_from;
        // Flush and close before anything stats or reads it.
        let mut file = self.file;
        let _ = file.flush();
        let _ = file.sync_all();
        drop(file);

        let bytes = file_bytes(&part);
        let head = read_head(&part, SNIFF_BYTES);
        if let Verdict::Bad(mut finding) = classify(true, bytes, head.as_deref(), None, Some(&pin)) {
            let keep = finding.kind == Failure::Short;
            if !keep {
                let _ = fs::remove_file(&part);
            }
            finding.resumable = keep;
            return Outcome::Failed { finding };
        }

        let Some(sha) = hash_file(&part) else {
            let _ = fs::remove_file(&part);
            return Outcome::Failed { finding: Finding::of(Failure::Absent) };
        };
        if let Verdict::Bad(finding) = classify(true, bytes, head.as_deref(), Some(&sha), Some(&pin)) {
            let _ = fs::remove_file(&part); // discarded, never quarantined-and-used
            return Outcome::Failed { finding };
        }

        if let Err(e) = fs::rename(&part, &dest) {
            let _ = fs::remove_file(&part);
            return Outcome::Failed {
                finding: Finding { detail: Some(e.to_string()), ..Finding::of(Failure::Network) },
            };
        }
        Outcome::Installed { path: dest, bytes, sha256: sha, resumed_from }
    }
}

// ---------------------------------------------------------------------------------------------
// The event the webview renders. The CONTRACT lives here, with the rules it reports on.
// ---------------------------------------------------------------------------------------------

/// Progress and outcome of getting the speech model. `rich://` name and camelCase payload, the
/// convention `event.rs` established and `richos-core`'s `stream.rs` established before it.
pub const EVENT_VOICE_MODEL: &str = "rich://voice-model";

/// Where a model install has got to.
///
/// `Verifying` IS ITS OWN PHASE and is not cosmetic. Hashing 487,614,201 bytes takes 0.93–0.95 s
/// on the reference M4 (`toolchain.rs` measured it), and a progress bar that sits at 100% for a
/// second with no explanation is the moment a person decides the app has hung. It is also the
/// truth: the transfer IS finished and the check is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelPhase {
    Started,
    Progress,
    Verifying,
    Installed,
    Failed,
}

impl ModelPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelPhase::Started => "started",
            ModelPhase::Progress => "progress",
            ModelPhase::Verifying => "verifying",
            ModelPhase::Installed => "installed",
            ModelPhase::Failed => "failed",
        }
    }
}

/// The payload the webview reads. Numbers stay numbers — a UI that has to parse a string to draw
/// a progress bar is a UI that draws the wrong bar the day the format changes.
pub fn model_event_payload(
    phase: ModelPhase,
    model_id: &str,
    received: u64,
    total: u64,
    message: Option<&str>,
    ask_again: bool,
    at: u64,
) -> Value {
    serde_json::json!({
        "phase": phase.as_str(),
        "modelId": model_id,
        "received": received,
        "total": total,
        "totalLabel": human(total),
        "message": message,
        // NAMED FOR WHAT IT DECIDES: whether the "try again" control appears. It is
        // `Finding::worth_asking_again`, NOT `Finding::retryable` — see that function for the
        // instruction-with-no-control this separation removes. Calling the field `retryable`
        // would invite the next reader to wire it to the other rule.
        "askAgain": ask_again,
        "at": at,
    })
}

// ---------------------------------------------------------------------------------------------
// Formatting. Small, and shared with the JavaScript's wording so one product speaks once.
// ---------------------------------------------------------------------------------------------

/// A size a person can read, picking the unit rather than forcing one.
///
/// NOT COSMETIC: "0.0 MB" is what a fixed-MB formatter says about 4 KB, and a message that reports
/// the wrong number is worse than one that reports none. Mirrors `model-catalog.js::human`.
pub fn human(bytes: u64) -> String {
    let n = bytes as f64;
    if n >= 1_000_000_000.0 {
        format!("{:.2} GB", n / 1_000_000_000.0)
    } else if n >= 1_000_000.0 {
        format!("{:.1} MB", n / 1_000_000.0)
    } else if n >= 1_000.0 {
        format!("{:.1} kB", n / 1_000.0)
    } else {
        format!("{n} bytes")
    }
}

fn commas(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn pct(have: u64, want: u64) -> String {
    if want == 0 {
        return "0%".into();
    }
    let p = have as f64 / want as f64 * 100.0;
    if p < 1.0 {
        format!("{p:.2}%")
    } else {
        format!("{p:.1}%")
    }
}

fn hex4(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn short_hash(h: &str) -> String {
    format!("{}…", h.chars().take(12).collect::<String>())
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n.saturating_sub(1)).collect::<String>())
    } else {
        s.to_string()
    }
}

fn first_line(head: &[u8]) -> String {
    head.iter()
        .take(200)
        .map(|&b| b as char)
        .collect::<String>()
        .split(['\r', '\n'])
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

fn expand_tilde(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    p.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT: the payload keys are camelCase and are the ones `app/ui/main.js` reads.
    #[test]
    fn the_model_event_payload_keys_are_the_ones_the_webview_reads() {
        let p = model_event_payload(ModelPhase::Progress, "small.en", 1_000, 487_614_201, None, false, 7);
        assert_eq!(p["phase"], "progress");
        assert_eq!(p["modelId"], "small.en");
        assert_eq!(p["received"].as_u64(), Some(1_000));
        assert_eq!(p["total"].as_u64(), Some(487_614_201));
        assert_eq!(p["totalLabel"], "487.6 MB");
        assert!(p["message"].is_null());
        assert_eq!(p["at"].as_u64(), Some(7));
    }

    /// INVARIANT: the event name is stable — it is a published contract.
    #[test]
    fn the_model_event_name_is_stable() {
        assert_eq!(EVENT_VOICE_MODEL, "rich://voice-model");
    }

    /// INVARIANT: a failure carries BOTH the sentence and whether asking again could help, so the
    /// UI never has to guess which failures deserve a control.
    #[test]
    fn a_failed_phase_says_whether_asking_again_could_possibly_help() {
        let dropped = Finding { resumable: true, ..Finding::counts(Failure::Short, 10, 100) };
        let p = model_event_payload(
            ModelPhase::Failed,
            "small.en",
            10,
            100,
            Some(&dropped.ceo_message()),
            dropped.worth_asking_again(),
            1,
        );
        assert_eq!(p["phase"], "failed");
        assert_eq!(p["askAgain"], true);

        // THE SEPARATION, ASSERTED. A tampered download must never be retried automatically and
        // MUST still offer him the control, because its own sentence tells him to ask again once
        // he is on a network he trusts. One flag could not have said both.
        let tampered = Finding::hashes(Failure::HashMismatch, "aa", "bb");
        assert!(!tampered.retryable(), "no automatic loop over corruption");
        assert!(tampered.worth_asking_again(), "and yet the sentence invites him back");
        let p = model_event_payload(
            ModelPhase::Failed,
            "small.en",
            100,
            100,
            Some(&tampered.ceo_message()),
            tampered.worth_asking_again(),
            1,
        );
        assert_eq!(p["askAgain"], true);
        assert!(
            p["message"].as_str().unwrap().contains("Ask me again"),
            "the control and the sentence must agree"
        );

        // And the one that genuinely cannot be helped by asking.
        let unpinned = Finding::of(Failure::Unpinned);
        assert!(!unpinned.worth_asking_again(), "a missing pin is a fact about our source");
    }

    /// **EVERY SENTENCE THAT INVITES HIM BACK MUST COME WITH THE CONTROL THAT TAKES HIM THERE.**
    ///
    /// The static half of `affordances.js`'s rule, asserted at the source of the sentences rather
    /// than after they have crossed into JavaScript: if `ceo_message` says "ask me again", the
    /// payload built from the same finding must set `askAgain`.
    #[test]
    fn no_failure_sentence_asks_him_to_come_back_without_setting_the_control_flag() {
        let kinds = [
            Failure::Absent,
            Failure::Empty,
            Failure::HtmlBody,
            Failure::TextBody,
            Failure::CompressedBody,
            Failure::NotGgml,
            Failure::Short,
            Failure::Oversize,
            Failure::HashMismatch,
            Failure::NoSpace,
            Failure::Unpinned,
            Failure::Network,
            Failure::Http,
        ];
        let mut invited = 0;
        for kind in kinds {
            let f = Finding::of(kind);
            let says_come_back = f.ceo_message().to_lowercase().contains("ask me again");
            if says_come_back {
                invited += 1;
                assert!(
                    f.worth_asking_again(),
                    "{kind:?} tells him to ask again and would render with no control"
                );
            }
        }
        assert!(invited >= 6, "only {invited} sentences invite him back - the scan stopped working");
    }

    /// The five phases are distinct strings. Two phases sharing a name is a UI that cannot tell
    /// "still downloading" from "finished and checking".
    #[test]
    fn every_phase_has_its_own_name() {
        let all = [
            ModelPhase::Started,
            ModelPhase::Progress,
            ModelPhase::Verifying,
            ModelPhase::Installed,
            ModelPhase::Failed,
        ];
        let mut names: Vec<&str> = all.iter().map(|p| p.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all.len());
    }
}
