//! **THE PAIRED DEVICE, AND WHY EVERY REFUSAL IS A 404** — plan §2.5 items 4, 6 and 7, §2.7
//! and §4.1.
//!
//! > *"**Every route requires the paired device's credential, and everything else is a flat
//! > 404** — the tg-bridge's `chat_id` allowlist lesson kept verbatim: an unpaired caller
//! > learns nothing, not even that it guessed a real path."*
//!
//! > *"**What is on his Wi-Fi is trusted less than he thinks** — a guest's laptop, a TV, a
//! > doorbell. Being on the home network is explicitly NOT authentication here; the pairing
//! > credential is."*
//!
//! # The credential, and whose shape it is
//!
//! **RECONCILED 2026-09-18 with the landed phone app.** The phone shipped first (`dfa7ed27`)
//! with its own contract stub, and the Mac side adopts its wire shapes rather than the other
//! way round — the phone is a tested artifact and this Rust was not yet reachable from
//! anywhere, so this is the cheaper side to move. `richos/web/web-app/lib/api.js` is the
//! authority for every byte below.
//!
//! At pairing the phone generates a **non-extractable** WebCrypto ECDSA P-256 key and
//! registers the public half. Every request after that carries
//!
//! ```text
//! Authorization: RichOS-Device <device_id>.<challenge>.<base64url signature>
//! ```
//!
//! where the signature is over [`signing_string`]:
//!
//! ```text
//! <challenge>\n<METHOD>\n<path-with-query>\n<lowercase hex SHA-256 of the body, or "">
//! ```
//!
//! **The challenge is issued by the Mac and never by the phone** — `X-RichOS-Challenge` on
//! every response, in the stream's `hello`, and in the pairing answer. Plan §2.7's cheaper
//! alternative, a long random bearer token, is **not** taken: slice C did not run over, so
//! nothing extractable is stored on the phone.
//!
//! # The one property that is weaker than the word "challenge" suggests, said plainly
//!
//! **A challenge is not single-use.** It cannot be, and the reason is the client rather than a
//! shortcut: the phone replays the most recent challenge it holds, and it cannot know a newer
//! one before the response that carries it arrives — so two concurrent requests share one. And
//! `EventSource` reconnects to *the identical URL*, so the stream's credential is presented
//! again on every reconnection by the browser itself.
//!
//! So a challenge is valid for [`CHALLENGE_LIFETIME_MS`] from the moment the Mac issued it, and
//! only the most recent [`LIVE_CHALLENGES`] are live at once. What that buys: a captured
//! request is replayable for at most ten minutes, by somebody already on his home network, and
//! it can do nothing a replay of it could not — the phone's own words, again, on his own
//! thread. What it does NOT claim is one-shot freshness, and this paragraph exists so nobody
//! reads the word "challenge" and assumes it.

use super::push::Subscription;
use super::{constant_time_eq, hex, sha256, PhoneError, PAIRING_WINDOW_MS};
use std::collections::VecDeque;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

/// **THE CAPABILITY A PHONE ASKS BEFORE IT ASKS THIS MAC TO HOLD ITS ANSWER** — Sage's pair-v2
/// hypotheses review §1 "The fix" point 2, and the capability negotiation pattern of ledger row
/// S3 (COPY THE DESIGN): named beside `pair-v2` in every `capabilities` list, so a phone sends
/// `Prefer: wait=…` only to a Mac that will honor it and older Macs and phones keep working
/// unchanged.
pub const PAIR_WAIT_CAPABILITY: &str = "pair-wait";

/// **THE LONGEST THIS MAC HOLDS A PHONE'S "They match" BEFORE ANSWERING "still waiting".**
///
/// Fourteen seconds, and the number is the event stream's rather than a choice made here: the
/// stream already crosses the phone's routes with at most [`super::KEEPALIVE_MS`] = 15,000 ms
/// between bytes, so a hold that sends nothing for 14,000 ms stays inside a silence those routes
/// already carry. The count, recomputed: the window is [`PAIRING_WINDOW_MS`] = 300,000 ms, and
/// 300,000 ÷ 14,000 = 21.4, so a phone that is held every time asks at most 22 times in the
/// window — the same ceiling today's backed-off schedule reaches.
pub const PAIR_WAIT_MAX_SECONDS: u64 = 14;

/// **HOW LONG A PAIRING THIS MAC ACCEPTED MAY STAY UNFINISHED BY THE PHONE** — Sage's review §2
/// "The fix" point 2, measured from [`Device::confirm_by`].
///
/// One more pairing window, five minutes, and it has to exceed the phone's own deadline: the
/// phone waits until the moment of its own "They match" plus `confirm_within_seconds`, which is
/// at most `confirm_by` plus however long the person spent reading the words. **The cost of
/// this number, stated as Sage stated it:** a person who presses They match on this Mac first
/// and then leaves the phone's word screen unanswered for more than five minutes past the window
/// comes back to a phone this Mac has forgotten, and scans again. `unverified:` it is a judgment,
/// not a measurement.
pub const UNFINISHED_GRACE_MS: u64 = PAIRING_WINDOW_MS;

/// How long a challenge stays presentable. Ten minutes: long enough that an `EventSource` which
/// reconnects for half an hour is only asked for a fresh one occasionally, short enough that a
/// captured request is worthless by the time anyone looks at it.
pub const CHALLENGE_LIFETIME_MS: u64 = 600_000;

/// How many challenges are live at once.
///
/// **THIS WAS 16, AND 16 IS SMALLER THAN ONE PAGE LOAD** — Ray's nightly `.7` walk, defect 1, and
/// the half of it that lives on this side.
///
/// The old comment said "the phone holds one and uses it until a response gives it a newer one, so
/// a handful covers every in-flight request". Both halves are true of requests the phone MAKES,
/// and neither is true of the responses this Mac GIVES. [`super::listen::render`] mints a
/// challenge for every single one of them, and the phone app is served over this same port: a
/// static asset GET carries no credential, tells the phone nothing it can read — `EventSource` and
/// a browser's own subresource loads cannot see a response header — and still pushes one entry out
/// of this set.
///
/// The arithmetic, counted rather than estimated: the phone web page (retired 2026-10-30) had a
/// service worker whose `SHELL` array was **19** entries, fetched in one burst at install, so one
/// install was 19 responses. **19 > 16**, so the challenge the phone was holding was pushed out
/// without the phone making a request at all, and the next signed send was refused
/// `UnknownChallenge` → a flat 404 → `Not sent.` on his screen.
/// `challenges_survive_a_burst_of_credential_free_responses` keeps that burst, twice over, as the
/// floor, so the bound cannot quietly go under it again.
///
/// **256, and the bound is still doing its original job.** It is the same size as
/// the former idempotency cache and costs about 14 KB; it is more than ten times the widest legitimate
/// burst; and it still stops the set growing without limit under a caller that only ever asks for
/// challenges. What it does NOT do is lengthen any challenge's life — that is
/// [`CHALLENGE_LIFETIME_MS`]'s job alone, and `issue_challenge` now drops expired entries on the
/// way past so age is what retires a challenge rather than a queue depth.
const LIVE_CHALLENGES: usize = 256;

/// 60 requests per rolling minute, and at most 4 concurrent event streams. Sized for one CEO
/// and one phone (plan §2.5 item 6) — a hostile device on his Wi-Fi cannot exhaust the Mac by
/// being loud, and a phone that reconnects a few times in a row never trips it.
///
/// **THREE BUCKETS OF 60, NOT ONE** — PRD 2026-09-21 §4 (*"per-device limits, bounded
/// unauthenticated work"*) and §7 (*"another host/device cannot … exhaust its authenticated
/// limits"*). One shared window let anybody who could reach the port — over RichOS Connect,
/// anybody on the internet — refuse the paired phone `RateLimited` by sending 60 requests a
/// minute. What [`DeviceDesk::verify`] does now, in order:
///
/// 1. **A forgotten phone's `Revoked` is answered before any bucket.** It is a lookup over at most
///    8 ids, no more work than checking a bucket, and a final answer must not become "come back
///    later" because somebody else is loud.
/// 2. **The cheap constant-time device-id match decides which bucket a request spends.**
/// 3. **Strangers** — any id that is not the paired phone's — share one bucket. Nothing behind it
///    does crypto; it bounds how often the refusal path runs and keeps answering a flood 429.
/// 4. **The paired phone's own bucket** is spent before the challenge scan and the signature, so
///    every request that can reach crypto is counted first. It belongs to the device record and
///    starts empty when a new phone pairs.
/// 5. **Pairing attempts** have their own bucket in [`DeviceDesk::complete_pairing`].
///
/// **Keyed by device, not by caller address, because there is no caller address.** Over Connect
/// every request reaches this Mac from `cloudflared` on `127.0.0.1`. The device id is the only
/// caller identity that exists before the signature.
///
/// **The residual, stated:** a caller that already holds the phone's device id reaches the phone's
/// bucket with forged signatures and can spend it. That id is 48 bits of the phone's key hash, is
/// never shown to anyone, and travels only inside TLS (and through Cloudflare's TLS termination on
/// the Connect route); bounded crypto is chosen over keeping that caller out of the phone's minute.
///
/// **In memory only, and a restart empties all three.** Writing a window to disk would add a disk
/// write to every refused request, which is a cost a flood could make the Mac pay; and a caller
/// cannot restart the Mac, so a restart gives nobody an extra window they could have asked for.
const RATE_WINDOW_MS: u64 = 60_000;
const RATE_LIMIT: usize = 60;
/// At most this many event streams at once, across every device id together.
///
/// **ONE LIVE STREAM PER DEVICE, AND THE NEWEST WINS** (2026-10-01 iPhone walk, defect D4). A
/// phone process that is killed sends no close — over RichOS Connect, `cloudflared` keeps its
/// connection to this Mac open whatever the phone did — so its stream stayed counted while this
/// Mac wrote keep-alives into it. Four quick relaunches filled all four slots with streams of
/// processes that no longer existed, and the fifth launch was answered 429 for a minute.
/// [`DeviceDesk::claim_stream`] now ends the same device's older streams and returns their
/// slots BEFORE the cap is counted. A device's streams are never ended by another device's, so
/// this cap still bounds how many streams this Mac carries at once.
pub const MAX_STREAMS: usize = 4;

/// The pairing code's alphabet: **no `I`, `L`, `O`, `U`, `0` or `1`.** He is reading this off
/// one screen while another scans it, and the two characters people confuse are the two that
/// cost him the sixty seconds.
const CODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";
const CODE_LENGTH: usize = 8;

/// The one phone this install knows.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Device {
    pub id: String,
    /// What he called it. Shown on the settings screen so "Forget this phone" names a thing.
    pub name: String,
    /// The 65-byte uncompressed P-256 point, base64url. Public by definition.
    pub public_key: String,
    pub paired_at: u64,
    /// Where to push. `None` until the phone has been installed to the Home Screen and
    /// notifications allowed, which happens after pairing — and `None` again the moment the
    /// push service reports the subscription gone (plan risk 2).
    pub push: Option<Subscription>,
    /// **No longer consulted** (hunt 2026-09-29, part 1, finding 14). It was one number for the
    /// whole phone, compared with row cursors that each conversation numbers from 1, so having
    /// read row 100 of a long thread silenced the push for row 3 of a new one. Kept so a record
    /// written before [`Device::delivered_cursors`] still reads; see that field.
    pub delivered_cursor: Option<u64>,
    /// The last cursor the phone said it had durably seen, PER CONVERSATION (thread id to cursor).
    /// Advisory: it stops a push repeating something already read, and it never decides what the
    /// ledger holds. A cursor only means something inside the conversation that numbered it, so a
    /// cursor sent without its conversation is not recorded.
    #[serde(default)]
    pub delivered_cursors: std::collections::BTreeMap<String, u64>,
    /// Modern browsers acknowledge completed replies actually rendered in the foreground.
    /// An open proxied stream alone cannot establish that somebody is reading it.
    #[serde(default)]
    pub reply_receipts: bool,
    #[serde(default)]
    pub seen_replies: Vec<(String, String)>,
    /// **`push_transport` from day one**, per plan §5's list of what native reuses: *"a device
    /// record that has a `push_transport` field from day one so APNs and FCM slot in beside
    /// Web Push rather than replacing it"*. Native registration records `"apns"`.
    #[serde(default = "web_push")]
    pub push_transport: String,

    /// **WHICH PATH THIS PHONE ACTUALLY PAIRED OVER**, recorded once, here, at pairing.
    /// [`PairedVia::TAILNET`], [`PairedVia::HOME`] or [`PairedVia::UNKNOWN`].
    ///
    /// **This field exists because the settings card had no durable answer and guessed.** The
    /// sheet's `route` variable is deliberately forgotten on every open (`ui/phone.js`, Urban
    /// §1: *"a remembered choice with no visible way to change it is a trap"*), and the paired
    /// card's copy was keyed off it. So closing and reopening the card for the SAME paired
    /// phone flipped `route` from `"anywhere"` to `null` and redrew the home path's
    /// certificate-removal steps — iOS steps, on an Android phone, on the one path that never
    /// installs a certificate. Ray's candidate .11 walk, defect 3.2, reproduced twice.
    ///
    /// The route the user picked in the sheet is a *question*; this is the *answer*, and only
    /// the answer belongs on disk. `UNKNOWN` is what a record written before this field existed
    /// deserializes to, and the card has its own honest sentence for it rather than a default
    /// that happens to be one of the two.
    #[serde(default)]
    pub paired_via: String,

    /// **WHAT KIND OF PHONE IT IS**, recorded once, at pairing, from what the phone itself
    /// said. [`Platform::IOS`], [`Platform::ANDROID`], [`Platform::OTHER`] or
    /// [`Platform::UNKNOWN`].
    ///
    /// Only iOS has a `.mobileconfig` to remove and only iOS has "VPN & Device Management";
    /// only Chrome on Android calls the install "Install and create shortcut". A card that does
    /// not know which phone it is talking about cannot say either without guessing.
    #[serde(default)]
    pub platform: String,

    /// **HAS THE PERSON TOLD THIS MAC THAT THE SIX WORDS MATCHED?** — Ray's nightly `.7` walk,
    /// defect 2.
    ///
    /// The Mac's sheet read `Phone — It is paired. Open Rich on it and keep talking.` while the
    /// phone, on screen beside it, was still asking `They match — pair this phone` /
    /// `They do not match` (his frame 13). The six words exist so a person can detect that
    /// something other than his Mac answered; a Mac that settles the question before he has
    /// answered it teaches him the check is ceremonial.
    ///
    /// **There was no way for the Mac to know, and that was the finding rather than the
    /// wording.** `pair-confirm` in `web/web-app/app.js` wrote to the phone's own storage and
    /// started the conversation, and told this Mac nothing; `pair-reject` threw the phone's key
    /// away and ALSO told this Mac nothing, so a phone the person had just declared suspect was
    /// left paired here. Both now come back over `POST /api/pair`.
    ///
    /// **`true` for a record written before this field existed**, and that is not a shortcut: a
    /// phone paired by an older build is a phone that is already working, and demoting it to
    /// "waiting" on the first launch of this one would be the Mac asserting something it does
    /// not know in the other direction. A record written by THIS build always carries the value
    /// outright, `false` at pairing, so the default is only ever reached by a record from before.
    #[serde(default = "confirmed_by_default")]
    pub fingerprint_confirmed: bool,

    /// **HAS THE PERSON PRESSED "They match" ON THIS MAC?** — Sage's pairing review F1 (High),
    /// `richos-hq/docs/research/2026-09-24-richconnect-pairing-protocol-review.md` §1 and §3.1.
    ///
    /// [`Device::fingerprint_confirmed`] above is the PHONE's answer, and it is sent by the phone
    /// itself. A device that paired first with a code it saw on a screen share holds the only key
    /// this Mac trusts, so it can send that answer on its own behalf: the device being judged
    /// would be performing the judgment. This flag can only be set by a person at this Mac
    /// ([`DeviceDesk::confirm_on_mac`]), and until it is set [`DeviceDesk::verify`] answers every
    /// signed request [`Refusal::AwaitingMacConfirmation`] except the phone's own confirmation.
    ///
    /// **`true` for a record written before this field existed**, for the reason
    /// `fingerprint_confirmed` gives: that phone is already working, and demoting it on the first
    /// launch of this build would lock out every paired phone at once. That includes a record some
    /// earlier build wrote for an intruder's phone. This build cannot tell the two apart, and the
    /// residual is stated in the handoff rather than guessed at here. A record written by this build
    /// always carries the value outright, `false` at pairing.
    #[serde(default = "confirmed_by_default")]
    pub mac_confirmed: bool,

    /// **WHICH SIX WORDS THIS PHONE IS SHOWING** — `2` when its pairing request announced
    /// `"pairing_version": 2` (Sage §3, `pair-v2`), `1` otherwise, and `1` for a record written
    /// before the field existed. The Mac shows the person the words the phone will show, so the
    /// two can be compared at all; see [`Device::words_to_compare`].
    ///
    /// **Stripping it is not a downgrade anybody can use.** A relay that removes the phone's field
    /// makes this Mac show v1 words while the v2 phone, which never falls back, shows v2 words —
    /// two different lines in front of the person, which is the check working.
    #[serde(default = "pairing_version_one")]
    pub pairing_version: u8,

    /// **WHEN THE PAIRING WINDOW THIS PHONE CAME THROUGH CLOSES**, in `now_millis()` terms — the
    /// window's own opening plus [`PAIRING_WINDOW_MS`] (Sage §3.1 step 6: *"The window's timer
    /// keeps running. An unconfirmed device at expiry is forgotten, so an abandoned pairing never
    /// leaves a key behind"*). Meaningful only while `mac_confirmed` is false; `0` for a record
    /// written before the field, which is always a confirmed one.
    ///
    /// On disk rather than in memory because the window is not: a Mac that relaunches in the
    /// middle of a pairing still knows when the unconfirmed key has to go.
    #[serde(default)]
    pub confirm_by: u64,

    /// **HAS THE PHONE ACTUALLY USED THE PAIRING?** — Sage's pair-v2 hypotheses review §2, the
    /// Mac that listed a phone which could never connect.
    ///
    /// The press on this Mac ([`Device::mac_confirmed`]) is this Mac's yes; it is not evidence
    /// that the phone HEARD it. A phone that lost this Mac after its own "They match" asks until
    /// its deadline, hears nothing, and throws its key away, while this Mac went on saying "It is
    /// paired" for a key whose private half no longer exists. So "paired" now waits for the one
    /// thing only a phone that heard the yes does: a request through [`DeviceDesk::verify`] —
    /// every ask before that goes through [`DeviceDesk::verify_for_confirmation`] instead.
    ///
    /// **`true` for a record written before this field existed**, the precedent
    /// [`Device::mac_confirmed`] set: those phones are already working, and demoting them on the
    /// first launch of this build would have the Mac forget every paired phone at the next sweep.
    /// A record written by this build always carries the value outright, `false` at pairing.
    #[serde(default = "confirmed_by_default")]
    pub completed: bool,
}

fn pairing_version_one() -> u8 {
    1
}

fn confirmed_by_default() -> bool {
    true
}

impl Device {
    /// **Has the phone already read this row of this conversation?** The one question every push
    /// path asks before announcing a reply, so that the answer is the same on all of them. A
    /// cursor counts only inside the conversation that numbered it (finding 14): nothing recorded
    /// for `thread` means nothing is known to be read there.
    pub fn has_read(&self, thread: &str, cursor: Option<u64>) -> bool {
        match (self.delivered_cursors.get(thread), cursor) {
            (Some(read), Some(row)) => *read >= row,
            _ => false,
        }
    }

    /// **Both people-answers are in**: the person said the words matched on the phone AND on this
    /// Mac. What anything that reaches the phone unprompted — a push, a notification
    /// registration — asks before it acts, so an unconfirmed key can never be the one that is
    /// pushed to.
    pub fn trusted(&self) -> bool {
        self.fingerprint_confirmed && self.mac_confirmed
    }

    /// **Nobody pressed "They match" on this Mac before the window closed.** Such a device is
    /// refused like one that does not exist and forgotten at the next sweep
    /// ([`DeviceDesk::expire_unconfirmed`]).
    pub fn confirmation_lapsed(&self, now: u64) -> bool {
        !self.mac_confirmed && now > self.confirm_by
    }

    /// **This Mac said yes, and the phone never used the pairing within the grace** (Sage's
    /// review §2). Such a device is forgotten at the next sweep with its own sentence
    /// ([`StoppedBy::UNFINISHED`]), so a key whose phone gave up does not hold the one pairing
    /// slot until somebody finds `Forget this phone`.
    pub fn finish_lapsed(&self, now: u64) -> bool {
        self.mac_confirmed && !self.completed && now > self.confirm_by + UNFINISHED_GRACE_MS
    }

    /// **The six words this Mac shows beside "They match"** — the ones this phone will show.
    ///
    /// For a v2 phone: [`super::words::v2`] over this Mac's own serving `origin`, its pairing
    /// value `ca_fingerprint_sha256` and the key it REGISTERED — never the key a phone says it
    /// has. For a phone that did not announce v2 (the preserved iPhone app, for one release):
    /// `v1`, the root's words, which the caller already holds.
    pub fn words_to_compare(&self, origin: &str, ca_fingerprint_sha256: &str, v1: &[&'static str]) -> Vec<&'static str> {
        if self.pairing_version >= 2 {
            super::words::v2(origin, ca_fingerprint_sha256, &self.public_key)
        } else {
            v1.to_vec()
        }
    }
}

fn web_push() -> String {
    "web-push".to_string()
}

/// **The two paths a phone can reach this Mac on**, as the tokens that go on disk and to the
/// settings sheet. Stable strings rather than an enum on the wire, for the same reason
/// [`super::TailnetView::state`] is one: the screen switches on them and they are never
/// translated.
pub struct PairedVia;
impl PairedVia {
    /// `https://<name>.ts.net:8443` — the Tailscale path. No certificate is installed on the
    /// phone on this path, which is the whole of its promise.
    pub const CONNECT: &'static str = "connect";
    pub const TAILNET: &'static str = "tailnet";
    /// `https://<name>.local:8443` — the home network, which DOES install a profile.
    pub const HOME: &'static str = "home";
    /// A record written before this field existed. Never written by this build.
    pub const UNKNOWN: &'static str = "";
}

/// **What kind of phone paired**, from the only platform evidence the pairing request carries.
pub struct Platform;
impl Platform {
    pub const IOS: &'static str = "ios";
    pub const ANDROID: &'static str = "android";
    /// A phone that named itself but named nothing this Mac recognizes.
    pub const OTHER: &'static str = "other";
    /// A record written before this field existed. Never written by this build.
    pub const UNKNOWN: &'static str = "";
}

/// **Read the platform off what the phone called itself.**
///
/// `web/web-app/app.js:236` (`deviceName()`) sends exactly one of four strings — `"iPad"`,
/// `"iPhone"`, `"Android phone"`, `"Phone"` — chosen from `navigator.userAgent` on the device
/// itself. That name is the ONLY platform evidence in the pairing request today: the request
/// carries no `platform` field and `routes::Incoming` does not keep the `User-Agent` header.
///
/// So this reads the name, and [`super::routes`] prefers an explicit `platform` in the body
/// when one is there — which is what makes the phone page able to start sending one without
/// this Mac changing. That belongs to `web/web-app/`, which this worktree does not touch.
///
/// **It is matched case-insensitively and by substring**, because the value is a human-facing
/// label: "iPhone" today, "Sam's iPhone" the moment anybody adds a rename box.
pub fn platform_of_name(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.contains("iphone") || lower.contains("ipad") || lower.contains("ipod") {
        return Platform::IOS;
    }
    if lower.contains("android") {
        return Platform::ANDROID;
    }
    Platform::OTHER
}

/// An open pairing window. Sixty seconds, one shot (plan §4.1).
#[derive(Debug, Clone)]
pub struct PairingWindow {
    pub code: String,
    pub opened_at: u64,
}

impl PairingWindow {
    pub fn open() -> Result<Self, PhoneError> {
        let bytes = super::random_bytes(CODE_LENGTH)?;
        // Rejection-free mapping: 256 mod 30 biases slightly toward the first 16 letters, which
        // costs a fraction of a bit out of about 39. Saying so is cheaper than a rejection loop
        // nobody would read, and this code lives for sixty seconds.
        let code: String =
            bytes.iter().map(|b| CODE_ALPHABET[*b as usize % CODE_ALPHABET.len()] as char).collect();
        Ok(PairingWindow { code, opened_at: super::now_millis() })
    }

    pub fn is_open(&self, now: u64) -> bool {
        now >= self.opened_at && now - self.opened_at <= PAIRING_WINDOW_MS
    }
}

/// Why a request was refused.
///
/// **All but one become a flat 404 with an empty body.** The exception is [`Refusal::Revoked`],
/// which is a deliberate 403 with `{"revoked":true}` — the phone treats that as final, clears
/// its credential and says so, instead of retrying forever against a Mac that has forgotten it
/// (`web/web-app/lib/api.js`). A 404 there would be a phone that hammers his Mac and a CEO who is
/// never told why his phone stopped working.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    NoDevice,
    UnknownDevice,
    /// This device id was paired and has been forgotten. Answered honestly, once.
    Revoked,
    StaleChallenge,
    UnknownChallenge,
    BadSignature,
    /// **Authenticated, and nobody has pressed "They match" on this Mac yet** (Sage F1). Told
    /// apart from the flat 404 on purpose: every client treats a 404 after one re-sign as final
    /// (`web/web-app/lib/api.js`), and this state is the opposite of final. It is only ever
    /// returned AFTER the signature verified, so a caller without the key learns nothing from it.
    AwaitingMacConfirmation,
    /// **The spent-code alarm fired** (Sage F1 fix item 4): a second correct use of a redeemed
    /// code while the first device was still unconfirmed. That device has already been forgotten
    /// by the time this is returned; the route answers the caller a flat 404 and tells the owner
    /// of the listener to stop and say why ([`StoppedBy::CODE_USED_TWICE`]).
    CodeUsedTwice,
    MalformedCredential(String),
    RateLimited,
    TooManyStreams,
    /// The pairing could not be written down. **A pairing the Mac cannot record must not be
    /// reported as successful**, because it would vanish at the next relaunch and he would be
    /// left with a phone that says it is paired and a Mac that says it is not.
    CouldNotRecord(String),
}

impl From<PhoneError> for Refusal {
    fn from(e: PhoneError) -> Self {
        Refusal::CouldNotRecord(e.to_string())
    }
}

/// One signed request, as read off the wire.
pub struct Presented<'a> {
    pub method: &'a str,
    /// The path and query the signature covers. For the event stream that is the URL **without
    /// the `auth` parameter**, because the parameter is the signature.
    pub path_with_query: &'a str,
    pub device_id: &'a str,
    pub challenge: &'a str,
    pub signature: Vec<u8>,
    pub body: &'a [u8],
}

/// **The exact bytes both ends sign** — `web/web-app/lib/api.js`'s `signingInput`, in Rust.
///
/// Note the last field: the body hash is the **empty string** when there is no body, not the
/// SHA-256 of zero bytes. That is what the phone sends (`payload === undefined ? '' : …`), and
/// a difference of one field here is a day of two people each being sure they are right.
pub fn signing_string(challenge: &str, method: &str, path_with_query: &str, body: &[u8]) -> String {
    let body_hash = if body.is_empty() { String::new() } else { hex(&sha256(body)) };
    format!("{challenge}\n{}\n{path_with_query}\n{body_hash}", method.to_uppercase())
}

/// Parse `RichOS-Device <device_id>.<challenge>.<base64url signature>`.
///
/// Split from the RIGHT twice rather than by `split('.')`: a device id or a challenge that ever
/// contained a dot would silently produce the wrong triple, and "silently the wrong triple" is a
/// signature failure with no cause attached.
pub fn parse_authorization(header: &str) -> Option<(String, String, Vec<u8>)> {
    let rest = header.strip_prefix("RichOS-Device ")?;
    let (head, signature) = rest.rsplit_once('.')?;
    let (device_id, challenge) = head.rsplit_once('.')?;
    if device_id.is_empty() || challenge.is_empty() {
        return None;
    }
    Some((device_id.to_string(), challenge.to_string(), super::unb64url(signature).ok()?))
}

/// The desk: the paired device, the pairing window, the live challenges, and the small memories
/// that make replay, double-posting and shouting bounded.
pub struct DeviceDesk {
    path: PathBuf,
    state: Mutex<State>,
    pub deliveries: super::delivery::DeliveryDesk,
    /// Photos and files the phone uploaded, staged until their message commits
    /// ([`super::attachments`]). Opening it does no I/O.
    pub attachments: super::attachments::AttachmentDesk,
    /// How long each accepted phone voice note was ([`super::voice_notes`]). No I/O at open.
    pub voice_notes: super::voice_notes::VoiceNoteDesk,
}

struct State {
    device: Option<Device>,
    pairing: Option<PairingWindow>,
    /// Issued challenges with the moment each was issued, oldest first.
    challenges: VecDeque<(String, u64)>,
    /// Device ids that were paired and have been forgotten. Kept so a phone gets a final
    /// answer rather than an endless 404.
    revoked: VecDeque<String>,
    /// The paired phone's own window. Emptied when a new phone pairs (see [`RATE_LIMIT`]).
    device_requests: VecDeque<u64>,
    /// Every caller whose device id is not the paired phone's, together.
    stranger_requests: VecDeque<u64>,
    pairing_requests: VecDeque<u64>,
    /// The event streams counted against [`MAX_STREAMS`], oldest first.
    streams: Vec<OpenStream>,
    next_stream: u64,
    /// Audio blobs the Mac has minted, by the message id the phone was given. Contract: *"no
    /// id it did not mint"* — the table is the authority and the request is only an index.
    audio: VecDeque<(String, PathBuf)>,
    /// **THE REDEEMED CODE, KEPT UNTIL ITS WINDOW WOULD HAVE CLOSED** — Sage F1 fix item 4, the
    /// spent-code alarm. `(SHA-256 of the code, when its window opened)`. Before this, a second
    /// correct redemption was an indistinguishable 404 because the window was already gone; now
    /// it is evidence that two devices had the code. The hash, not the code: nothing here needs
    /// to be able to show it again.
    redeemed: Option<(Vec<u8>, u64)>,
    /// The alarm fired AFTER the person had already confirmed the phone on this Mac, so the phone
    /// was kept and the sheet warns (Sage: *"If it is already confirmed, keep it and warn"*).
    /// In memory: it is about this pairing, and forgetting the phone clears it.
    code_reused: bool,
    /// **THE ONE HELD ASK, AND WHAT RELEASES IT** — see [`DeviceDesk::hold_answer`].
    hold: Hold,
}

/// The bookkeeping behind [`DeviceDesk::hold_answer`]. In memory only: a held ask lives for at
/// most [`PAIR_WAIT_MAX_SECONDS`] on one connection, and a relaunch ends the connection anyway.
#[derive(Default)]
struct Hold {
    /// Bumped by every event that ends a hold with a new answer to give: the press on this Mac,
    /// any forgetting of the device, and the channel's teardown ([`DeviceDesk::release_holds`]).
    released: u64,
    /// Ticket of the ask being held now, if one is, and the waker of the task holding it.
    current: Option<(u64, Option<Waker>)>,
    next_ticket: u64,
}

impl Hold {
    /// Ring the release and wake the held task. Waking under the desk's lock is safe: a tokio
    /// waker only schedules the task and never polls it inline, and the verifier's waker is a
    /// flag.
    fn release(&mut self) {
        self.released += 1;
        if let Some(waker) = self.current.as_mut().and_then(|(_, waker)| waker.take()) {
            waker.wake();
        }
    }
}

/// How a held ask ended. [`HoldEnd::Released`] means something changed that the phone should hear
/// about, so the listener runs the request again and sends THAT answer; a superseded hold answers
/// "still waiting" at once, because the newer ask now carries the wait.
#[derive(Debug, PartialEq, Eq)]
pub enum HoldEnd {
    Released,
    Superseded,
}

/// The future [`DeviceDesk::hold_answer`] returns. **Plain `std`, no runtime**: this module is
/// compiled on its own by `mobile/conformance/verifier`, which has no tokio, and a hold the
/// verifier can poll by hand is a release signal it can prove.
pub struct HeldAnswer {
    desk: Arc<DeviceDesk>,
    ticket: u64,
    since: u64,
}

impl Future for HeldAnswer {
    type Output = HoldEnd;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<HoldEnd> {
        let mut state = self.desk.state.lock().unwrap();
        if state.hold.released != self.since {
            return Poll::Ready(HoldEnd::Released);
        }
        match state.hold.current.as_mut() {
            Some((ticket, waker)) if *ticket == self.ticket => {
                *waker = Some(cx.waker().clone());
                Poll::Pending
            }
            _ => Poll::Ready(HoldEnd::Superseded),
        }
    }
}

impl Drop for HeldAnswer {
    /// A hold that timed out, was superseded or was canceled with its connection gives up the
    /// slot, so a stale waker is never the one a release reaches.
    fn drop(&mut self) {
        let mut state = self.desk.state.lock().unwrap();
        if state.hold.current.as_ref().is_some_and(|(ticket, _)| *ticket == self.ticket) {
            state.hold.current = None;
        }
    }
}

// =========================================================================================
// "THEY DO NOT MATCH", REMEMBERED ACROSS A RELAUNCH
// =========================================================================================
//
// **Two halves, and only one of them survived quitting the app.**
//
// The half that already survives is the REFUSAL, and it survives by construction rather than
// by a flag: `routes.rs` drops the device record synchronously before it rings the stop
// switch, [`DeviceDesk::forget`] deletes `device.json`, and
// [`DeviceDesk::listener_should_run`] reads what is on disk — so the next launch finds nothing
// paired, `PhoneRuntime::resume_if_paired` starts no socket, and the phone that said the words
// did not match is refused because there is nothing left for it to reach. That is asserted in
// this module rather than trusted: see
// `a_rejected_phone_is_still_refused_after_a_relaunch_and_the_notice_goes_with_it`.
//
// The half that did NOT survive is the SENTENCE. `PhoneStatus::rejected` was a `Mutex<bool>`
// on the runtime, seeded `false` at every `PhoneRuntime::install`, so quitting RichOS after
// refusing a phone brought the settings screen back reading as though nothing had happened —
// which is Ray's `.8` defect 1, *"he has no way to know the Mac heard him"*, one launch later.
// So the refusal is written down here, beside `device.json`, in the one directory that holds
// what this Mac remembers about phones.
//
// **The file's EXISTENCE is the fact; its contents are for a person reading it.** A record
// this build cannot parse is still a refusal — the OPPOSITE default from the device record
// above, deliberately: a damaged `device.json` must never be guessed into a paired phone, and
// a damaged `rejected.json` must never be guessed into a Mac that forgot.

/// Where the refusal is written. Derived in one place, so nothing else knows the name.
fn rejection_path(dir: &Path) -> PathBuf {
    dir.join("phone").join("rejected.json")
}

/// **WHO STOPPED THE PAIRING**, as the token written beside the refusal and handed to the sheet,
/// which says a different sentence for each. Stable strings, never translated, like
/// [`PairedVia`].
pub struct StoppedBy;
impl StoppedBy {
    /// The phone answered `They do not match`.
    pub const PHONE: &'static str = "phone";
    /// The person pressed `They do not match` on this Mac (Sage F1, §3.1 step 6).
    pub const MAC: &'static str = "mac";
    /// **The spent-code alarm** (Sage F1 fix item 4): the pairing code was used correctly a
    /// second time while the phone that used it first was still unconfirmed, so two devices had
    /// the code and this Mac forgot the first one rather than guess which is his.
    pub const CODE_USED_TWICE: &'static str = "code-used-twice";
    /// Nobody pressed "They match" on this Mac before the pairing window closed (§3.1 step 6),
    /// so the unconfirmed key was forgotten rather than left behind.
    pub const EXPIRED: &'static str = "expired";
    /// **This Mac pressed They match, and the phone never used the pairing** within
    /// [`UNFINISHED_GRACE_MS`] of the window closing (Sage's pair-v2 hypotheses review §2), so the
    /// key its phone gave up on was forgotten rather than listed as a phone that is paired.
    pub const UNFINISHED: &'static str = "unfinished";
    /// A record this build cannot read, or one written before the reason was recorded. The sheet
    /// says the sentence that was the only one before; the refusal itself still stands.
    pub const UNKNOWN: &'static str = "";
}

/// Write down that the pairing was refused, and by whom ([`StoppedBy`]). `at` is `now_millis()`.
///
/// It runs on the teardown path, after the credential is already gone, so the caller treats a
/// failure as something to SAY and never as something to stop for: a disk that will not take
/// one small file must not be able to keep a channel up.
pub fn record_rejection(dir: &Path, at: u64, by: &str) -> Result<(), PhoneError> {
    let path = rejection_path(dir);
    if let Some(home) = path.parent() {
        std::fs::create_dir_all(home)?;
    }
    std::fs::write(&path, format!("{}\n", serde_json::json!({ "rejected_at": at, "by": by })))?;
    Ok(())
}

/// Did somebody refuse this Mac's six words, with nobody having asked to pair since?
pub fn rejection_recorded(dir: &Path) -> bool {
    rejection_path(dir).exists()
}

/// Who refused ([`StoppedBy`]), read back for the sheet. [`StoppedBy::UNKNOWN`] for a record
/// that is damaged or older than the field — the refusal is [`rejection_recorded`]'s to decide,
/// and this only chooses a sentence.
pub fn rejection_by(dir: &Path) -> &'static str {
    let by = std::fs::read_to_string(rejection_path(dir))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.get("by").and_then(|v| v.as_str()).map(str::to_owned));
    match by.as_deref() {
        Some(StoppedBy::PHONE) => StoppedBy::PHONE,
        Some(StoppedBy::MAC) => StoppedBy::MAC,
        Some(StoppedBy::CODE_USED_TWICE) => StoppedBy::CODE_USED_TWICE,
        Some(StoppedBy::EXPIRED) => StoppedBy::EXPIRED,
        Some(StoppedBy::UNFINISHED) => StoppedBy::UNFINISHED,
        _ => StoppedBy::UNKNOWN,
    }
}

/// Asking to pair a phone is how the rejection screen is left, and this is the durable half of
/// that one act (`PhoneRuntime::begin_pairing`).
pub fn clear_rejection(dir: &Path) {
    let _ = std::fs::remove_file(rejection_path(dir));
}

impl DeviceDesk {
    /// Open the desk, reading any paired device off disk.
    ///
    /// A record this build cannot parse is **reported and treated as unpaired**, never silently
    /// replaced: if the file is damaged, the honest outcome is that the phone stops working and
    /// the settings screen says to pair again — not that a different phone quietly becomes the
    /// paired one.
    pub fn open(dir: &Path) -> Result<Self, PhoneError> {
        let home = dir.join("phone");
        std::fs::create_dir_all(&home)?;
        let path = home.join("device.json");
        let device = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Device>(&text) {
                Ok(d) => Some(d),
                Err(e) => {
                    eprintln!(
                        "[richos] the paired-phone record at {} could not be read ({e}). \
                         RichOS is treating this install as having no phone paired rather than \
                         guessing. Nothing was deleted; pair the phone again to replace it.",
                        path.display()
                    );
                    None
                }
            },
            Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        let revoked_path = home.join("revoked.json");
        let mut revoked: VecDeque<String> = match std::fs::read_to_string(&revoked_path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| PhoneError::Malformed(format!("could not read forgotten phones: {e}")))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => VecDeque::new(),
            Err(e) => return Err(e.into()),
        };
        while revoked.len() > 8 { revoked.pop_front(); }
        // A durable revocation wins even if the process stopped before deleting device.json.
        let device = device.filter(|d| !revoked.contains(&d.id));
        Ok(DeviceDesk {
            deliveries: super::delivery::DeliveryDesk::open(dir)?,
            attachments: super::attachments::AttachmentDesk::open(dir),
            voice_notes: super::voice_notes::VoiceNoteDesk::open(dir),
            path,
            state: Mutex::new(State {
                device,
                pairing: None,
                challenges: VecDeque::new(),
                revoked,
                device_requests: VecDeque::new(),
                stranger_requests: VecDeque::new(),
                pairing_requests: VecDeque::new(),
                streams: Vec::new(),
                next_stream: 0,
                audio: VecDeque::new(),
                redeemed: None,
                code_reused: false,
                hold: Hold::default(),
            }),
        })
    }

    pub fn paired(&self) -> Option<Device> {
        self.state.lock().unwrap().device.clone()
    }

    /// Read by the tests and by nothing in the app: the product asks `listener_should_run`
    /// instead, so there is ONE expression of "should the socket exist" rather than two that
    /// each cover half of it.
    #[allow(dead_code)]
    pub fn is_paired(&self) -> bool {
        self.state.lock().unwrap().device.is_some()
    }

    // --- challenges ---------------------------------------------------------------------

    /// Mint a challenge and remember it. Called for every response, so the phone always leaves
    /// a request holding a newer one than it arrived with.
    ///
    /// **AGE RETIRES A CHALLENGE; THE COUNT ONLY STOPS THE SET GROWING.** The two lines below are
    /// in that order deliberately. [`verify`](Self::verify) already refuses anything older than
    /// [`CHALLENGE_LIFETIME_MS`], so dropping expired entries here changes no answer — it just
    /// means the count bound is reached by live challenges only, and never by ten-minute-old ones
    /// holding a slot in front of them. Getting that backwards is how a bound meant to cap memory
    /// became the thing that expired the phone's credential (see [`LIVE_CHALLENGES`]).
    pub fn issue_challenge(&self) -> Result<String, PhoneError> {
        let now = super::now_millis();
        let mut state = self.state.lock().unwrap();
        while state
            .challenges
            .front()
            .map(|(_, at)| now.saturating_sub(*at) > CHALLENGE_LIFETIME_MS)
            .unwrap_or(false)
        {
            state.challenges.pop_front();
        }
        // Anonymous traffic cannot evict a phone's live challenge by issuing more.
        // Twenty 30-second buckets fit inside the ten-minute lifetime and the bound.
        if let Some((challenge, issued)) = state.challenges.back() {
            if now.saturating_sub(*issued) < 30_000 { return Ok(challenge.clone()); }
        }
        let challenge = super::b64url(&super::random_bytes(24)?);
        state.challenges.push_back((challenge.clone(), now));
        while state.challenges.len() > LIVE_CHALLENGES {
            state.challenges.pop_front();
        }
        Ok(challenge)
    }

    // --- pairing ------------------------------------------------------------------------

    /// Open a sixty-second pairing window. Refused while a phone is already paired — one device
    /// in v1, and "Forget this phone" is what frees the slot (plan §4.1).
    pub fn open_pairing(&self) -> Result<PairingWindow, PhoneError> {
        let mut state = self.state.lock().unwrap();
        if state.device.is_some() {
            return Err(PhoneError::Malformed(
                "a phone is already paired; forget it first, which also closes the listener".into(),
            ));
        }
        let window = PairingWindow::open()?;
        state.pairing = Some(window.clone());
        Ok(window)
    }

    pub fn pairing_window(&self) -> Option<PairingWindow> {
        let state = self.state.lock().unwrap();
        state.pairing.clone().filter(|w| w.is_open(super::now_millis()))
    }

    /// **Close the pairing window because he asked to leave, not because it ran out.**
    ///
    /// Urban's G2: `Pick a different way` exists on every screen of this flow except the one
    /// that has a live code, so *"once `Set my phone up` is pressed, the route chooser is
    /// unreachable for the life of the app process"* — and `expired` sets `pairing` too, so
    /// waiting does not give it back either. The control he needs has to undo what the last
    /// one did, and what the last one did was open this window and bind a socket.
    ///
    /// **It is a separate verb from [`DeviceDesk::forget`] and must stay one.** `forget` is
    /// about a phone that is PAIRED: it deletes the device record and the keys. Nothing is
    /// paired here — the user is backing out of a choice — so there is nothing to delete, and
    /// a "back" button that reached for `forget` would be a back button that erases a phone.
    ///
    /// **The window is dropped rather than left to time out, and that is what makes the socket
    /// go.** [`DeviceDesk::listener_should_run`] answers "yes" while a window is open, so a
    /// caller that stopped the listener without clearing this would be told, correctly, that the
    /// listener should still be running — and the live code would stay redeemable by anything
    /// that reached the socket during the minutes it had left. A code he backed out of is a code
    /// that is gone.
    pub fn close_pairing(&self) {
        self.state.lock().unwrap().pairing = None;
    }

    /// Is there any reason for the listener to exist right now? A paired device, or an open
    /// window. Plan §2.5 item 1 plus the qualification the contract adds: the phone gets the
    /// app FROM the Mac, so the socket must be up during pairing.
    pub fn listener_should_run(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.device.is_some()
            || state.pairing.as_ref().map(|w| w.is_open(super::now_millis())).unwrap_or(false)
    }

    /// Complete a pairing. **One shot**: the window is closed here whether the rest succeeds or
    /// not, so a wrong key does not leave a live code behind.
    ///
    /// `via` is [`PairedVia::TAILNET`] or [`PairedVia::HOME`] — the path the code that is being
    /// redeemed went out under, which the caller knows and this desk cannot. It is recorded
    /// rather than recomputed at read time because the answer can change underneath a paired
    /// phone: turn Tailscale off on this Mac and the serving plan falls back to the home path,
    /// and a card that recomputed would then tell an Android phone to remove an iOS profile it
    /// never had. Ray's defect 3.2.
    ///
    /// The v1 shape, kept because the tests here and `mobile/conformance/verifier` pair through
    /// it; the route calls [`DeviceDesk::complete_pairing_announcing`].
    #[allow(dead_code)]
    pub fn complete_pairing(
        &self,
        code: &str,
        public_key: &PublicKeyForm,
        name: &str,
        via: &str,
        platform: &str,
    ) -> Result<Device, Refusal> {
        self.complete_pairing_announcing(code, public_key, name, via, platform, 1)
    }

    /// [`DeviceDesk::complete_pairing`], recording which six words the phone said it will show
    /// ([`Device::pairing_version`]): `2` for a phone that announced `pair-v2`, anything else is `1`.
    pub fn complete_pairing_announcing(
        &self,
        code: &str,
        public_key: &PublicKeyForm,
        name: &str,
        via: &str,
        platform: &str,
        pairing_version: u8,
    ) -> Result<Device, Refusal> {
        let point = public_key.to_point().map_err(|e| Refusal::MalformedCredential(e.to_string()))?;
        let mut state = self.state.lock().unwrap();
        let now = super::now_millis();
        prune_requests(&mut state.pairing_requests, now);
        if state.pairing_requests.len() >= RATE_LIMIT { return Err(Refusal::RateLimited); }
        state.pairing_requests.push_back(now);

        // THE SPENT-CODE ALARM — Sage F1 fix item 4. The window is gone because the code was
        // redeemed, and the redeemed code is kept until the window would have closed, so a second
        // CORRECT use is evidence rather than an indistinguishable 404.
        if state.pairing.is_none() {
            if let Some((hash, opened_at)) = state.redeemed.clone() {
                let still_open = now >= opened_at && now - opened_at <= PAIRING_WINDOW_MS;
                if still_open && constant_time_eq(&hash, &sha256(code.as_bytes())) {
                    let registered = super::b64url(&point);
                    match state.device.clone() {
                        // The SAME key again is the same phone asking twice — a reload with the
                        // code still in its address, or an answer lost on the way back. It gets
                        // the same pairing, and nothing is escalated: it already holds that key.
                        Some(device) if constant_time_eq(device.public_key.as_bytes(), registered.as_bytes()) => {
                            return Ok(device);
                        }
                        // Two devices had the code and nobody here has said which is his: forget
                        // the one that came first rather than guess, and say so on the Mac.
                        Some(device) if !device.mac_confirmed => {
                            self.forget_in(&mut state)?;
                            return Err(Refusal::CodeUsedTwice);
                        }
                        // He already confirmed the first one on this Mac. Keep it, and warn.
                        Some(_) => {
                            state.code_reused = true;
                            return Err(Refusal::NoDevice);
                        }
                        None => return Err(Refusal::NoDevice),
                    }
                }
            }
        }

        let window = state.pairing.as_ref().ok_or(Refusal::NoDevice)?;
        if !window.is_open(now) {
            return Err(Refusal::NoDevice);
        }
        if !constant_time_eq(window.code.as_bytes(), code.as_bytes()) {
            return Err(Refusal::BadSignature);
        }
        if state.device.is_some() {
            return Err(Refusal::NoDevice);
        }
        let window_opened_at = window.opened_at;
        let device = Device {
            id: format!("dev_{}", hex(&sha256(&point)[..6])),
            name: sanitize_name(name),
            public_key: super::b64url(&point),
            paired_at: now,
            push: None,
            delivered_cursor: None,
            delivered_cursors: Default::default(),
            reply_receipts: false,
            seen_replies: Vec::new(),
            push_transport: web_push(),
            paired_via: via.to_string(),
            platform: platform.to_string(),
            // The person has not been shown the six words yet — the phone computes them from
            // the fingerprint in the answer to THIS request. So the Mac does not know, and it
            // says so until the phone comes back (Ray's defect 2).
            fingerprint_confirmed: false,
            // AND NOBODY AT THIS MAC HAS ANSWERED EITHER (Sage F1). Until somebody does, this
            // key can confirm itself and nothing else.
            mac_confirmed: false,
            pairing_version: if pairing_version == 2 { 2 } else { 1 },
            // THE WINDOW'S TIMER KEEPS RUNNING (Sage §3.1 step 6): the press on this Mac has to
            // come before the window this code came from would have closed.
            confirm_by: window_opened_at + PAIRING_WINDOW_MS,
            // AND "PAIRED" WAITS FOR THE PHONE TO USE IT (Sage's pair-v2 review §2): the first
            // request through `verify` after the press sets this, and nothing else does.
            completed: false,
        };
        // Pairing is explicit authorization to use this key again. Device IDs are key-derived.
        if state.revoked.contains(&device.id) {
            let mut revoked = state.revoked.clone();
            revoked.retain(|id| id != &device.id);
            self.write_revoked(&revoked)?;
            state.revoked = revoked;
        }
        state.device = Some(device.clone());
        if let Err(error) = self.write(&state) {
            state.device = None;
            return Err(error.into());
        }
        state.pairing = None;
        // Kept, as a hash, until the window would have closed: the spent-code alarm above.
        state.redeemed = Some((sha256(code.as_bytes()), window_opened_at));
        state.code_reused = false;
        // The phone's window belongs to the device record: a new phone does not inherit the minute
        // the last one spent.
        state.device_requests.clear();
        Ok(device)
    }

    /// "Forget this phone" (plan §4.1). Instant and complete by construction on the Mac side:
    /// the record is deleted, the id is remembered as revoked so the phone gets a final answer,
    /// and the caller destroys the Keychain keys and closes the listener.
    pub fn forget(&self) -> Result<(), PhoneError> {
        let mut state = self.state.lock().unwrap();
        self.forget_in(&mut state)
    }

    /// **A pairing that ran out of time is forgotten, and the answer says which clock ran out.**
    ///
    /// - [`StoppedBy::EXPIRED`]: nobody pressed They match on this Mac before the window closed —
    ///   Sage §3.1 step 6, *"An unconfirmed device at expiry is forgotten, so an abandoned pairing
    ///   never leaves a key behind."*
    /// - [`StoppedBy::UNFINISHED`]: this Mac pressed, and the phone never used the pairing within
    ///   [`UNFINISHED_GRACE_MS`] after that (Sage's pair-v2 hypotheses review §2).
    ///
    /// `None` when nothing lapsed. The owner of the listener takes the socket down on `Some` and
    /// the sheet says the sentence for that reason. A completed device is never touched.
    pub fn expire_lapsed(&self) -> Result<Option<&'static str>, PhoneError> {
        let mut state = self.state.lock().unwrap();
        let now = super::now_millis();
        let why = match state.device.as_ref() {
            Some(d) if d.confirmation_lapsed(now) => StoppedBy::EXPIRED,
            Some(d) if d.finish_lapsed(now) => StoppedBy::UNFINISHED,
            _ => return Ok(None),
        };
        self.forget_in(&mut state)?;
        Ok(Some(why))
    }

    /// **When the next pairing deadline falls**, if anything is waiting on one: an open window's
    /// close, an unconfirmed device's `confirm_by`, or a pressed-but-unfinished device's
    /// `confirm_by` plus [`UNFINISHED_GRACE_MS`]. What `PhoneRuntime`'s deadline watcher sleeps
    /// until — one sleep per deadline, never a poll.
    pub fn pairing_deadline(&self) -> Option<u64> {
        let state = self.state.lock().unwrap();
        let window = state.pairing.as_ref().map(|w| w.opened_at + PAIRING_WINDOW_MS);
        let device = state.device.as_ref().and_then(|d| {
            if !d.mac_confirmed {
                Some(d.confirm_by)
            } else if !d.completed {
                Some(d.confirm_by + UNFINISHED_GRACE_MS)
            } else {
                None
            }
        });
        window.into_iter().chain(device).max()
    }

    // --- the held ask (Sage's pair-v2 hypotheses review §1) -------------------------------

    /// Where the release signal stands. The listener reads it BEFORE it runs a request that may
    /// be held, and hands it to [`DeviceDesk::hold_answer`]: a press that lands between the
    /// request's answer being computed and the hold starting has already moved this, so the hold
    /// ends at once instead of making the phone wait fourteen seconds for news it missed.
    pub fn hold_generation(&self) -> u64 {
        self.state.lock().unwrap().hold.released
    }

    /// How many holds have begun since this desk opened: a ticket is issued the moment
    /// [`DeviceDesk::hold_answer`] is called. **Test observation only**, so a test that needs an
    /// ask to BE held before it presses, supersedes or tears down waits for this fact instead of
    /// sleeping and betting that the ask got there (2026-09-29, the load-sensitive checks audit).
    #[cfg(test)]
    pub(crate) fn holds_begun(&self) -> u64 {
        self.state.lock().unwrap().hold.next_ticket
    }

    /// **HOLD THE PHONE'S "They match" UNTIL THERE IS SOMETHING NEW TO SAY** — Sage's review §1,
    /// "The fix" point 2.
    ///
    /// Called by the listener only after the request has already been answered "the Mac is still
    /// waiting for its own press" — that is, after its signature verified on the one request an
    /// unconfirmed key may make ([`DeviceDesk::verify_for_confirmation`]). So a caller without the
    /// key never reaches it. The returned future ends:
    ///
    /// - [`HoldEnd::Released`] on the press on this Mac ([`DeviceDesk::confirm_on_mac`]), on any
    ///   forgetting of the device — `They do not match` on this Mac or the phone, the expiry sweep,
    ///   `Forget this phone`, the spent-code alarm — and on the channel's teardown
    ///   ([`DeviceDesk::release_holds`]), or if any of those already happened after `since`;
    /// - [`HoldEnd::Superseded`] when another held ask begins. **At most one ask is held at a
    ///   time**, so the holder of a key can never occupy more than one of the listener's 32
    ///   connection slots with holds.
    ///
    /// The time limit is the caller's ([`PAIR_WAIT_MAX_SECONDS`] under `tokio::time::timeout`).
    pub fn hold_answer(self: &Arc<Self>, since: u64) -> HeldAnswer {
        let mut state = self.state.lock().unwrap();
        state.hold.next_ticket += 1;
        let ticket = state.hold.next_ticket;
        // The previous holder is answered at once: its future sees a ticket that is not its own.
        if let Some((_, Some(waker))) = state.hold.current.replace((ticket, None)) {
            waker.wake();
        }
        HeldAnswer { desk: Arc::clone(self), ticket, since }
    }

    /// **The channel is going away: every held ask gets its answer now.** Called by
    /// `listen::Listener::stop` before it signals shutdown, so a phone that is being held hears
    /// the current state instead of a connection that simply drops.
    pub fn release_holds(&self) {
        self.state.lock().unwrap().hold.release();
    }

    /// **MAY THIS REQUEST'S BODY BE LARGER THAN 64 KiB?** — Sage's pairing review F3.
    ///
    /// The listener chooses how much to read BEFORE it reads it, from the request line alone, and
    /// on Connect the listener is on the public internet: `Content-Type: audio/wav` was granted
    /// 60,000,000 bytes for 120 s and `?kind=attachment` 25 MiB for 300 s, to anybody, across 32
    /// slots — 32 × 60 MB is 1.92 GB of his Mac's memory for a caller with no key at all.
    ///
    /// So a large body is granted only to a header that parses, names the paired device, that
    /// device confirmed ON THIS MAC and inside its window (F1), and presents a challenge this Mac
    /// issued and has not retired. **The no-crypto half of [`DeviceDesk::verify`]**: one lock and
    /// three comparisons, and no bucket is spent, because the full `verify` still runs after the
    /// body and spends it then. The signature is NOT checked here, so the residual is the one
    /// [`RATE_LIMIT`] already states: a caller that holds the device id (48 bits of the key's
    /// hash, never shown) and a live challenge (which `GET /api/challenge` hands anybody) can
    /// still be read up to the large limit before its signature fails.
    pub fn admits_large_body(&self, authorization: Option<&str>) -> bool {
        let Some((device_id, challenge, _signature)) = authorization.and_then(parse_authorization) else { return false };
        let now = super::now_millis();
        let state = self.state.lock().unwrap();
        let Some(device) = state.device.as_ref() else { return false };
        constant_time_eq(device.id.as_bytes(), device_id.as_bytes())
            && device.mac_confirmed
            && !device.confirmation_lapsed(now)
            && state.challenges.iter().any(|(c, at)| {
                constant_time_eq(c.as_bytes(), challenge.as_bytes()) && now.saturating_sub(*at) <= CHALLENGE_LIFETIME_MS
            })
    }

    /// Did somebody use the code again after the person had confirmed this phone on the Mac?
    pub fn code_reused(&self) -> bool {
        self.state.lock().unwrap().code_reused
    }

    fn forget_in(&self, state: &mut State) -> Result<(), PhoneError> {
        if let Some(device) = &state.device {
            let mut revoked = state.revoked.clone();
            revoked.push_back(device.id.clone());
            while revoked.len() > 8 { revoked.pop_front(); }
            // Persist before forgetting in memory. A failed write leaves the current pairing
            // intact and reports failure; a crash after rename still cannot revive its key.
            self.write_revoked(&revoked)?;
            state.revoked = revoked;
            state.device = None;
        }
        // The redeemed code and its warning belong to the pairing that just ended.
        state.redeemed = None;
        state.code_reused = false;
        state.pairing = None;
        state.challenges.clear();
        state.audio.clear();
        // A phone being held hears that this pairing ended, now, rather than at the end of its
        // hold (Sage's pair-v2 review §1: released by "They do not match" and the expiry sweep).
        state.hold.release();
        match std::fs::remove_file(&self.path) {
            Ok(()) => {},
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    fn write_revoked(&self, revoked: &VecDeque<String>) -> Result<(), PhoneError> {
        let bytes = serde_json::to_vec(revoked)
            .map_err(|e| PhoneError::Malformed(e.to_string()))?;
        let pending = self.path.with_file_name("revoked.pending");
        let mut file = std::fs::File::create(&pending)?;
        std::io::Write::write_all(&mut file, &bytes)?;
        file.sync_all()?;
        std::fs::rename(&pending, self.path.with_file_name("revoked.json"))?;
        std::fs::File::open(self.path.parent().unwrap())?.sync_all()?;
        Ok(())
    }

    // --- verification -------------------------------------------------------------------

    /// **The whole of the credential check, in order.** Any failure is a [`Refusal`], and every
    /// `Refusal` but `Revoked` and `AwaitingMacConfirmation` is a flat 404 to the caller.
    ///
    /// **A DEVICE NOBODY HAS CONFIRMED AT THIS MAC IS REFUSED HERE, AFTER ITS SIGNATURE VERIFIED**
    /// (Sage F1). The check lives in `verify` itself rather than in the routes so that every route
    /// that authenticates — messages, voice, attachments, the stream, backfill, audio, the device
    /// record — is gated by default, and a route added later cannot forget it. The one exception
    /// is [`DeviceDesk::verify_for_confirmation`], which exists for exactly one request.
    pub fn verify(&self, presented: &Presented<'_>) -> Result<Device, Refusal> {
        self.verify_credential(presented, false)
    }

    /// [`DeviceDesk::verify`] for the phone's own answer to the six words, and for nothing else:
    /// the one request an unconfirmed device may make (Sage §3.1 step 5). `routes.rs` calls it only
    /// for a body that carries a `fingerprint_confirmed` boolean and nothing that acts. That answer
    /// never activates anything — it protects the phone, and a `false` forgets this device.
    pub fn verify_for_confirmation(&self, presented: &Presented<'_>) -> Result<Device, Refusal> {
        self.verify_credential(presented, true)
    }

    fn verify_credential(&self, presented: &Presented<'_>, unconfirmed_may_answer: bool) -> Result<Device, Refusal> {
        let mut state = self.state.lock().unwrap();
        let now = super::now_millis();

        // 0. A known forgotten phone gets a final answer, including after a new pairing, and
        //    before any bucket: nobody else's volume can turn it into "come back later".
        if state.revoked.iter().any(|id| id == presented.device_id) {
            return Err(Refusal::Revoked);
        }

        // 1. The cheap check chooses the bucket; the bucket comes before any crypto. A stranger
        //    spends the strangers' window and is refused without reaching step 2, so it can
        //    neither make us do crypto nor spend the paired phone's window (see RATE_LIMIT).
        let device = match state.device.clone() {
            Some(d) if constant_time_eq(d.id.as_bytes(), presented.device_id.as_bytes()) => d,
            paired => {
                admit(&mut state.stranger_requests, now)?;
                return Err(if paired.is_some() { Refusal::UnknownDevice } else { Refusal::NoDevice });
            }
        };
        admit(&mut state.device_requests, now)?;

        // 2. the challenge is one WE issued, and it is still live
        let issued_at = state
            .challenges
            .iter()
            .find(|(c, _)| constant_time_eq(c.as_bytes(), presented.challenge.as_bytes()))
            .map(|(_, at)| *at);
        match issued_at {
            None => return Err(Refusal::UnknownChallenge),
            Some(at) if now.saturating_sub(at) > CHALLENGE_LIFETIME_MS => {
                return Err(Refusal::StaleChallenge)
            }
            Some(_) => {}
        }

        // 3. the signature verifies over the exact bytes
        let message = signing_string(
            presented.challenge,
            presented.method,
            presented.path_with_query,
            presented.body,
        );
        let point = super::unb64url(&device.public_key)
            .map_err(|e| Refusal::MalformedCredential(e.to_string()))?;
        let verifier =
            ring::signature::UnparsedPublicKey::new(&ring::signature::ECDSA_P256_SHA256_FIXED, point);
        verifier
            .verify(message.as_bytes(), &presented.signature)
            .map_err(|_| Refusal::BadSignature)?;

        // 4. somebody at THIS MAC said the words matched (Sage F1). Last, so only the holder of
        //    the key ever learns that a pairing is waiting.
        //    A device whose window closed with nobody pressing is refused like one that does not
        //    exist — even its own answer — and the next sweep forgets it (Sage §3.1 step 6).
        if device.confirmation_lapsed(now) {
            return Err(Refusal::NoDevice);
        }
        if !device.mac_confirmed && !unconfirmed_may_answer {
            return Err(Refusal::AwaitingMacConfirmation);
        }

        // 5. THE PHONE IS USING THE PAIRING (Sage's pair-v2 review §2). Only `verify` counts:
        //    every ask a phone makes while it waits for the press goes through
        //    `verify_for_confirmation`, so the first request through here after the press is the
        //    first one a phone makes because it HEARD the yes. Written once per pairing.
        let mut device = device;
        if !unconfirmed_may_answer && !device.completed {
            device.completed = true;
            if let Some(record) = state.device.as_mut() {
                record.completed = true;
            }
            if let Err(error) = self.write(&state) {
                // Not a refusal: the request is genuine and is answered, and the fact stays true
                // in memory so this run's sweep never forgets a phone that is plainly working.
                // What is lost is the record on disk, and a relaunch reads "unfinished" until the
                // phone's next request writes it again. Said, because that is the residual.
                eprintln!("[richos] the phone used its pairing but that could not be written down: {error}");
            }
        }
        Ok(device)
    }

    // --- the small memories -------------------------------------------------------------

    /// **THE PERSON SAID THE SIX WORDS MATCHED** — the answer to the question the Mac's sheet
    /// used to settle for him (Ray's nightly `.7`, defect 2).
    ///
    /// Idempotent: a phone that sends it twice, or sends it again after a relaunch, gets the
    /// same `Ok`. Refused when nothing is paired, which is the only state in which it is
    /// meaningless.
    pub fn confirm_fingerprint(&self) -> Result<(), PhoneError> {
        let mut state = self.state.lock().unwrap();
        let Some(device) = state.device.as_mut() else { return Err(PhoneError::NotPaired) };
        if device.fingerprint_confirmed {
            return Ok(());
        }
        device.fingerprint_confirmed = true;
        self.write(&state)
    }

    /// Has the person confirmed the six words for the phone that is paired right now?
    /// `false` when nothing is paired at all — there is no phone to have confirmed anything.
    pub fn fingerprint_confirmed(&self) -> bool {
        self.state.lock().unwrap().device.as_ref().map(|d| d.fingerprint_confirmed).unwrap_or(false)
    }

    /// **THE PERSON PRESSED "They match" ON THIS MAC** (Sage F1, §3.1 step 6). The only way a
    /// device becomes active, and it is reached from the Mac's own sheet (`phone_confirm_on_mac`)
    /// and from nothing on the network.
    ///
    /// Idempotent. Refused when nothing is paired: a press on a sheet that is one poll behind a
    /// teardown must not be able to activate whatever pairs next.
    pub fn confirm_on_mac(&self) -> Result<(), PhoneError> {
        let mut state = self.state.lock().unwrap();
        let Some(device) = state.device.as_mut() else { return Err(PhoneError::NotPaired) };
        if device.mac_confirmed {
            return Ok(());
        }
        // Too late is not a press: the window closed and the key is on its way out.
        if device.confirmation_lapsed(super::now_millis()) {
            return Err(PhoneError::NotPaired);
        }
        device.mac_confirmed = true;
        if let Err(error) = self.write(&state) {
            // A press this Mac cannot write down is not a press: after a relaunch the record
            // would say unconfirmed and the phone would stop working with nothing said.
            if let Some(device) = state.device.as_mut() { device.mac_confirmed = false; }
            return Err(error);
        }
        // THE PHONE BEING HELD HEARS IT NOW (Sage's pair-v2 review §1): after the write, so the
        // answer it is about to be given is one this Mac will still give after a relaunch.
        state.hold.release();
        Ok(())
    }

    /// **Forget a web push subscription the push service has said is gone** (404 or 410), and
    /// only that one: the phone may have registered a fresh subscription while the push that
    /// found this one gone was in flight, and a late answer about the old address must not erase
    /// the new one. Returns whether anything was cleared.
    pub fn clear_push_if_still(&self, gone: &Subscription) -> Result<bool, PhoneError> {
        let mut state = self.state.lock().unwrap();
        match state.device.as_mut() {
            Some(device) if device.push.as_ref() == Some(gone) => {
                device.push = None;
                self.write(&state)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn set_push(&self, sub: Option<Subscription>) -> Result<(), PhoneError> {
        let mut state = self.state.lock().unwrap();
        if let Some(device) = state.device.as_mut() {
            device.push = sub;
            device.push_transport = web_push();
            self.write(&state)?;
        }
        Ok(())
    }

    pub fn record_reply_receipt(&self, seen: Option<(&str, &str)>) -> Result<(), PhoneError> {
        let mut state = self.state.lock().unwrap();
        if let Some(device) = state.device.as_mut() {
            device.reply_receipts = true;
            if let Some((thread, id)) = seen {
                let key = (thread.to_owned(), id.to_owned());
                if !device.seen_replies.contains(&key) {
                    device.seen_replies.push(key);
                    if device.seen_replies.len() > 64 { device.seen_replies.remove(0); }
                }
            }
            self.write(&state)?;
        }
        Ok(())
    }

    /// The preserved iPhone app's call, unchanged: native push over APNs.
    pub fn use_native_push(&self, device_id: &str) -> Result<(), PhoneError> {
        self.use_native_transport(device_id, "apns")
    }

    /// Record that this phone is reached by a native push service — `"apns"` or `"fcm"` —
    /// instead of Web Push. Anything else is refused rather than written into the record.
    pub fn use_native_transport(&self, device_id: &str, transport: &str) -> Result<(), PhoneError> {
        if !["apns", "fcm"].contains(&transport) {
            return Err(PhoneError::Malformed(format!("unknown native push transport {transport:?}")));
        }
        let mut state = self.state.lock().unwrap();
        // The PHONE's answer, not `trusted()`: this only writes which push service reaches the
        // phone, and a native app sends it WITH its own confirmation, which may arrive before the
        // press on this Mac. Nothing is pushed on the strength of it — `PhoneRuntime`'s push and
        // registration paths ask `trusted()`.
        let device = state.device.as_mut().filter(|d| d.id == device_id && d.fingerprint_confirmed)
            .ok_or(PhoneError::NotPaired)?;
        device.push = None;
        device.push_transport = transport.into();
        self.write(&state)
    }

    /// Record how far the phone has read in ONE conversation (the latest answer wins, as the
    /// single number did).
    pub fn set_delivered_cursor(&self, thread: &str, cursor: u64) -> Result<(), PhoneError> {
        /// More conversations than he will ever have; the bound keeps the record from growing
        /// without limit if a client invents thread ids.
        const MAX_THREADS: usize = 256;
        let mut state = self.state.lock().unwrap();
        if let Some(device) = state.device.as_mut() {
            if !device.delivered_cursors.contains_key(thread) && device.delivered_cursors.len() >= MAX_THREADS {
                if let Some(oldest) = device.delivered_cursors.keys().next().cloned() {
                    device.delivered_cursors.remove(&oldest);
                }
            }
            device.delivered_cursors.insert(thread.to_string(), cursor);
            self.write(&state)?;
        }
        Ok(())
    }

    /// Claim one of the concurrent-stream slots for `device_id`, the AUTHENTICATED device the
    /// stream is for. The returned guard releases it on `Drop`, so a stream that ends by the
    /// client walking out of range still gives its slot back.
    ///
    /// **The same device's older streams are ended first, and their slots returned before the
    /// cap is counted** (see [`MAX_STREAMS`]): a phone opens one stream per process, so a newer
    /// one means the older belongs to a process that is gone or is about to replace it. Each one
    /// ended is told through its [`StreamSlot::ended`], and its slot is no longer counted from
    /// this moment, whether or not its socket ever notices. Another device's streams are never
    /// touched; with [`MAX_STREAMS`] distinct devices streaming, the next one is still refused,
    /// and a refused open ends nothing.
    pub fn claim_stream(self: &std::sync::Arc<Self>, device_id: &str) -> Result<StreamSlot, Refusal> {
        let mut state = self.state.lock().unwrap();
        let others = state.streams.iter().filter(|open| open.device != device_id).count();
        if others >= MAX_STREAMS {
            return Err(Refusal::TooManyStreams);
        }
        state.streams.retain(|open| {
            let same = open.device == device_id;
            if same {
                // `notify_one` keeps the permit if nobody is waiting yet, so a producer that has
                // not reached its wait still ends at it.
                open.end.ring();
            }
            !same
        });
        let id = state.next_stream;
        state.next_stream += 1;
        let end = std::sync::Arc::new(EndSignal::default());
        state.streams.push(OpenStream { id, device: device_id.to_string(), end: std::sync::Arc::clone(&end) });
        Ok(StreamSlot { desk: std::sync::Arc::clone(self), id, end })
    }

    pub fn open_streams(&self) -> usize {
        self.state.lock().unwrap().streams.len()
    }

    /// Record the audio blob for one message id, so `GET /api/audio/<message_id>` can serve it.
    ///
    /// **Built and not yet called: slice B is what synthesizes a reply.** The route it feeds exists
    /// now because the route is part of the contract the landed phone was built against, and a
    /// route that 404s every id until slice B lands is the honest version of "not yet".
    #[allow(dead_code)]
    pub fn mint_audio(&self, message_id: &str, file: PathBuf) {
        let mut state = self.state.lock().unwrap();
        state.audio.push_back((message_id.to_string(), file));
        while state.audio.len() > 64 {
            state.audio.pop_front();
        }
    }

    pub fn audio_file(&self, message_id: &str) -> Option<PathBuf> {
        let state = self.state.lock().unwrap();
        state.audio.iter().find(|(known, _)| known == message_id).map(|(_, p)| p.clone())
    }

    fn write(&self, state: &State) -> Result<(), PhoneError> {
        match &state.device {
            Some(device) => {
                let json = serde_json::to_string_pretty(device)
                    .map_err(|e| PhoneError::Malformed(e.to_string()))?;
                let pending = self.path.with_extension("pending");
                let mut file = std::fs::File::create(&pending)?;
                std::io::Write::write_all(&mut file, json.as_bytes())?;
                file.sync_all()?;
                std::fs::rename(&pending, &self.path)?;
                std::fs::File::open(self.path.parent().unwrap())?.sync_all()?;
            }
            None => {
                let _ = std::fs::remove_file(&self.path);
            }
        }
        Ok(())
    }
}

/// A one-shot "this stream was replaced" signal. **Plain `std`, no runtime**, for the same reason
/// as [`HeldAnswer`]: this module is compiled on its own by `mobile/conformance/verifier`, which
/// has no tokio. Ringing before anyone waits is kept (the flag), so a producer that has not reached
/// its wait still ends at it.
#[derive(Default)]
struct EndSignal {
    state: Mutex<(bool, Option<Waker>)>,
}

impl EndSignal {
    fn ring(&self) {
        let mut state = self.state.lock().unwrap();
        state.0 = true;
        if let Some(waker) = state.1.take() {
            waker.wake();
        }
    }
}

/// The future [`StreamSlot::ended`] returns.
struct EndWait {
    end: std::sync::Arc<EndSignal>,
}

impl Future for EndWait {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut state = self.end.state.lock().unwrap();
        if state.0 {
            Poll::Ready(())
        } else {
            state.1 = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// One stream counted against [`MAX_STREAMS`]: whose it is, and how to end it.
struct OpenStream {
    id: u64,
    device: String,
    end: std::sync::Arc<EndSignal>,
}

/// Releases a concurrent-stream slot when the stream ends, however it ends.
pub struct StreamSlot {
    desk: std::sync::Arc<DeviceDesk>,
    id: u64,
    end: std::sync::Arc<EndSignal>,
}

impl StreamSlot {
    /// Completes when a newer stream from the same device has replaced this one. The slot is
    /// already returned by then; the stream's producer ends at this and its response with it.
    /// Owns what it waits on, so the producer can hold it while the response holds the slot.
    pub fn ended(&self) -> impl std::future::Future<Output = ()> + Send + 'static {
        EndWait { end: std::sync::Arc::clone(&self.end) }
    }
}

impl Drop for StreamSlot {
    /// A replaced stream was already removed by [`DeviceDesk::claim_stream`], so this finds
    /// nothing and returns nobody else's slot.
    fn drop(&mut self) {
        let mut state = self.desk.state.lock().unwrap();
        state.streams.retain(|open| open.id != self.id);
    }
}

/// Spend one request from `bucket`, or refuse `RateLimited` without spending. A refused request
/// is not recorded, so a caller that keeps shouting does not push its own window further out.
fn admit(bucket: &mut VecDeque<u64>, now: u64) -> Result<(), Refusal> {
    prune_requests(bucket, now);
    if bucket.len() >= RATE_LIMIT {
        return Err(Refusal::RateLimited);
    }
    bucket.push_back(now);
    Ok(())
}

fn prune_requests(requests: &mut VecDeque<u64>, now: u64) {
    while let Some(at) = requests.front() {
        if now.saturating_sub(*at) > RATE_WINDOW_MS {
            requests.pop_front();
        } else {
            break;
        }
    }
}

/// The 26-byte prefix of a P-256 `SubjectPublicKeyInfo`. Constant by construction: the algorithm
/// identifier and the curve OID are fixed for P-256, so the only variable part of a 91-byte SPKI
/// is the 65-byte point at the end.
const P256_SPKI_PREFIX: [u8; 26] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];

/// The shapes a phone's public key can arrive in.
///
/// **The landed phone sends a JWK** (`public_key_jwk`, from
/// `crypto.subtle.exportKey('jwk', publicKey)`), which is the form WebCrypto produces without
/// any conversion on the phone. The other two are accepted because they cost nothing and a
/// native client (plan §5) will send raw bytes rather than a JWK.
pub enum PublicKeyForm {
    /// `{"kty":"EC","crv":"P-256","x":"…","y":"…"}`
    Jwk(serde_json::Value),
    /// base64url of either the 65-byte uncompressed point or its 91-byte SPKI wrapper.
    Bytes(String),
}

impl PublicKeyForm {
    /// Normalize to the 65-byte uncompressed point `ring` wants.
    ///
    /// **Every refusal names what was wrong.** A key that is half-parsed is a pairing that
    /// succeeds and then fails every request afterwards, which is the hardest kind of defect to
    /// read backwards from.
    pub fn to_point(&self) -> Result<Vec<u8>, PhoneError> {
        match self {
            PublicKeyForm::Jwk(value) => {
                let kty = value.get("kty").and_then(|v| v.as_str()).unwrap_or("");
                let crv = value.get("crv").and_then(|v| v.as_str()).unwrap_or("");
                if kty != "EC" || crv != "P-256" {
                    return Err(PhoneError::Malformed(format!(
                        "a device key must be an EC P-256 JWK, got kty={kty:?} crv={crv:?}"
                    )));
                }
                let x = super::unb64url(
                    value
                        .get("x")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| PhoneError::Malformed("the JWK has no x".into()))?,
                )?;
                let y = super::unb64url(
                    value
                        .get("y")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| PhoneError::Malformed("the JWK has no y".into()))?,
                )?;
                if x.len() != 32 || y.len() != 32 {
                    return Err(PhoneError::Malformed(format!(
                        "a P-256 JWK's x and y are 32 bytes each, got {} and {}",
                        x.len(),
                        y.len()
                    )));
                }
                let mut point = Vec::with_capacity(65);
                point.push(0x04);
                point.extend_from_slice(&x);
                point.extend_from_slice(&y);
                Ok(point)
            }
            PublicKeyForm::Bytes(b64) => {
                let bytes = super::unb64url(b64)?;
                if bytes.len() == 65 && bytes[0] == 0x04 {
                    return Ok(bytes);
                }
                if bytes.len() == 91 && bytes[..26] == P256_SPKI_PREFIX && bytes[26] == 0x04 {
                    return Ok(bytes[26..].to_vec());
                }
                Err(PhoneError::Malformed(format!(
                    "a device key must be a 65-byte uncompressed P-256 point or its 91-byte SPKI \
                     wrapper, got {} bytes",
                    bytes.len()
                )))
            }
        }
    }
}

/// A device name is shown on the Mac's settings screen, so it is trimmed to something a label
/// can hold and stripped of anything that is not printable. It is not a credential and it
/// decides nothing; it is only ever read by him.
fn sanitize_name(raw: &str) -> String {
    let cleaned: String = raw.chars().filter(|c| !c.is_control()).take(40).collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "Phone".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_FIXED_SIGNING};
    use std::sync::Arc;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "richos-phone-device-{tag}-{}-{}",
                std::process::id(),
                super::super::now_millis()
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A stand-in for the phone: a real P-256 key pair that signs the real string.
    pub struct Phone {
        pkcs8: Vec<u8>,
        pub point: Vec<u8>,
    }

    impl Phone {
        pub fn new() -> Self {
            let rng = SystemRandom::new();
            let doc = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
            let pair =
                EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, doc.as_ref(), &rng).unwrap();
            Phone { pkcs8: doc.as_ref().to_vec(), point: pair.public_key().as_ref().to_vec() }
        }
        /// The JWK the landed phone actually sends.
        pub fn jwk(&self) -> serde_json::Value {
            serde_json::json!({
                "kty": "EC",
                "crv": "P-256",
                "x": super::super::b64url(&self.point[1..33]),
                "y": super::super::b64url(&self.point[33..65]),
            })
        }
        pub fn sign(&self, message: &str) -> Vec<u8> {
            let rng = SystemRandom::new();
            let pair =
                EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &self.pkcs8, &rng).unwrap();
            pair.sign(&rng, message.as_bytes()).unwrap().as_ref().to_vec()
        }
    }

    /// A phone paired AND confirmed by a person at this Mac AND already using the pairing — the
    /// state every credential test below is about. The unconfirmed state has tests of its own
    /// (`unpaired_but_redeemed`), and so does the pressed-but-unfinished one (`pressed_unfinished`).
    fn paired(tag: &str) -> (TempDir, Arc<DeviceDesk>, Phone, Device, String) {
        let (dir, desk, phone, _pressed, challenge) = pressed_unfinished(tag);
        // Set directly rather than through a request, so no test's rate bucket is spent by its
        // setup; `the_phone_using_the_pairing_is_what_completes_it` proves the request path.
        mark_completed(&desk);
        let device = desk.paired().unwrap();
        (dir, desk, phone, device, challenge)
    }

    fn mark_completed(desk: &DeviceDesk) {
        let mut state = desk.state.lock().unwrap();
        state.device.as_mut().expect("a paired device").completed = true;
        desk.write(&state).unwrap();
    }

    /// Pressed on this Mac, and the phone has not used the pairing yet (Sage's pair-v2 review §2).
    fn pressed_unfinished(tag: &str) -> (TempDir, Arc<DeviceDesk>, Phone, Device, String) {
        let (dir, desk, phone, _redeemed, challenge) = unpaired_but_redeemed(tag);
        desk.confirm_on_mac().unwrap();
        let device = desk.paired().unwrap();
        (dir, desk, phone, device, challenge)
    }

    /// A phone that has redeemed a code and that NOBODY AT THIS MAC has confirmed (Sage F1).
    fn unpaired_but_redeemed(tag: &str) -> (TempDir, Arc<DeviceDesk>, Phone, Device, String) {
        let dir = TempDir::new(tag);
        let desk = Arc::new(DeviceDesk::open(&dir.0).unwrap());
        let phone = Phone::new();
        let window = desk.open_pairing().unwrap();
        let device = desk
            .complete_pairing(&window.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::HOME, Platform::IOS)
            .unwrap();
        let challenge = desk.issue_challenge().unwrap();
        (dir, desk, phone, device, challenge)
    }

    // --- Sage F1: the Mac confirms, and nothing else does -------------------------------------

    /// **THE HIGH FINDING, AT THE DESK.** A device that redeemed the code holds a key this Mac
    /// verifies — and until a person at this Mac presses "They match", that key reaches nothing
    /// but its own answer to the six words. On `main` before this change `verify` returned `Ok`
    /// for it, which is how a code seen on a screen share became a paired intruder.
    #[test]
    fn a_redeemed_code_reaches_nothing_until_a_person_confirms_on_the_mac() {
        let (dir, desk, phone, device, challenge) = unpaired_but_redeemed("f1-desk");
        assert!(!device.mac_confirmed, "a redeemed code was born confirmed");
        for (method, path, body) in [
            ("POST", "/api/messages", &b"{}"[..]),
            ("GET", "/api/events", &b""[..]),
            ("POST", "/api/messages?kind=attachment", &b"file"[..]),
        ] {
            let p = present(&phone, &device.id, &challenge, method, path, body);
            assert_eq!(desk.verify(&p), Err(Refusal::AwaitingMacConfirmation), "{method} {path}");
        }

        // The phone's OWN answer is the one request it may make — and making it activates nothing.
        let confirm = present(&phone, &device.id, &challenge, "POST", "/api/pair", b"{\"fingerprint_confirmed\":true}");
        assert!(desk.verify_for_confirmation(&confirm).is_ok());
        desk.confirm_fingerprint().unwrap();
        let p = present(&phone, &device.id, &challenge, "POST", "/api/messages", b"{}");
        assert_eq!(
            desk.verify(&p),
            Err(Refusal::AwaitingMacConfirmation),
            "the phone confirmed itself and was let in: the device being judged performed the judgment"
        );

        // The press on this Mac, and it survives a relaunch.
        desk.confirm_on_mac().unwrap();
        assert!(desk.verify(&p).is_ok(), "a confirmed device was still refused");
        let again = DeviceDesk::open(&dir.0).unwrap();
        assert!(again.paired().unwrap().mac_confirmed, "the press on the Mac did not survive a relaunch");
    }

    /// **SAGE F2 AT THE DESK: the Mac shows words over the key it REGISTERED.** A v2 phone's words
    /// are over the origin it dialed and its own key; the Mac's are over its own origin and the key
    /// that actually redeemed the code. An intruder who paired first registered a different key,
    /// so the person sees two different lines. A phone that did not announce v2 is shown the v1
    /// words for one release, and the choice survives a relaunch.
    #[test]
    fn the_mac_shows_the_words_for_the_key_it_registered_in_the_version_the_phone_announced() {
        let dir = TempDir::new("f2-words");
        let desk = DeviceDesk::open(&dir.0).unwrap();
        let phone = Phone::new();
        let w = desk.open_pairing().unwrap();
        let device = desk
            .complete_pairing_announcing(&w.code, &PublicKeyForm::Jwk(phone.jwk()), "Android phone", PairedVia::CONNECT, Platform::ANDROID, 2)
            .unwrap();
        assert_eq!(device.pairing_version, 2);
        let (origin, ca) = ("https://c-5de0dbe0862dd461ae305af5cef35202-g2.richos.ceo", "3D:9C:A1:00:FF:12");
        let v1 = ["anchor"; 6];
        let shown = device.words_to_compare(origin, ca, &v1);
        assert_eq!(shown, super::super::words::v2(origin, ca, &super::super::b64url(&phone.point)));
        assert_ne!(shown, v1.to_vec(), "a v2 phone was shown the v1 words");

        // The person's phone, which did NOT redeem the code, would show words over its own key.
        let persons_phone = Phone::new();
        assert_ne!(shown, super::super::words::v2(origin, ca, &super::super::b64url(&persons_phone.point)));

        assert_eq!(DeviceDesk::open(&dir.0).unwrap().paired().unwrap().pairing_version, 2, "the version did not survive a relaunch");

        // A phone that announced nothing (or anything but 2) is a v1 phone.
        desk.forget().unwrap();
        let w = desk.open_pairing().unwrap();
        let old = desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::CONNECT, Platform::IOS).unwrap();
        assert_eq!(old.pairing_version, 1);
        assert_eq!(old.words_to_compare(origin, ca, &v1), v1.to_vec());
        let legacy: Device = serde_json::from_value(serde_json::json!({
            "id": "dev_abc123", "name": "iPhone", "public_key": "BA", "paired_at": 1, "push": null, "delivered_cursor": null
        })).unwrap();
        assert_eq!(legacy.pairing_version, 1);
    }

    // --- the spent-code alarm and the window's timer (Sage F1 item 4, §3.1 step 6) -------------

    /// **TWO DEVICES HAD THE CODE.** A second correct redemption while the first device is still
    /// unconfirmed forgets the first and says so; on `main` it was an indistinguishable 404 and
    /// the first device — possibly the intruder — stayed paired.
    #[test]
    fn a_second_correct_use_of_the_code_forgets_an_unconfirmed_phone() {
        let dir = TempDir::new("alarm");
        let desk = DeviceDesk::open(&dir.0).unwrap();
        let (first, second) = (Phone::new(), Phone::new());
        let w = desk.open_pairing().unwrap();
        let device = desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(first.jwk()), "iPhone", PairedVia::CONNECT, Platform::IOS).unwrap();
        assert_eq!(
            desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(second.jwk()), "Android phone", PairedVia::CONNECT, Platform::ANDROID),
            Err(Refusal::CodeUsedTwice)
        );
        assert!(desk.paired().is_none(), "the phone that used the code first is still paired");
        let challenge = desk.issue_challenge().unwrap();
        assert_eq!(desk.verify(&present(&first, &device.id, &challenge, "GET", "/api/events", b"")), Err(Refusal::Revoked));
        assert!(DeviceDesk::open(&dir.0).unwrap().paired().is_none(), "the forgotten phone came back after a relaunch");
        // And the code is spent for good: a third use is an ordinary 404.
        assert_eq!(
            desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(second.jwk()), "Android phone", PairedVia::CONNECT, Platform::ANDROID),
            Err(Refusal::NoDevice)
        );
    }

    #[test]
    fn a_second_use_after_the_mac_confirmed_keeps_the_phone_and_warns() {
        let dir = TempDir::new("alarm-confirmed");
        let desk = DeviceDesk::open(&dir.0).unwrap();
        let w = desk.open_pairing().unwrap();
        let phone = Phone::new();
        desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::CONNECT, Platform::IOS).unwrap();
        desk.confirm_on_mac().unwrap();
        assert!(!desk.code_reused());
        assert_eq!(
            desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(Phone::new().jwk()), "x", PairedVia::CONNECT, Platform::OTHER),
            Err(Refusal::NoDevice)
        );
        assert!(desk.paired().is_some(), "a phone the person confirmed was forgotten");
        assert!(desk.code_reused(), "the second use was not recorded for the sheet");
        desk.forget().unwrap();
        assert!(!desk.code_reused(), "the warning outlived the pairing it was about");
    }

    /// The SAME key asking again is the same phone — a reload with the code still in its address,
    /// or an answer lost on the way back — and must not be read as the alarm.
    #[test]
    fn the_same_phone_redeeming_its_code_twice_is_not_the_alarm() {
        let dir = TempDir::new("alarm-same");
        let desk = DeviceDesk::open(&dir.0).unwrap();
        let phone = Phone::new();
        let w = desk.open_pairing().unwrap();
        let first = desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::CONNECT, Platform::IOS).unwrap();
        let again = desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::CONNECT, Platform::IOS).unwrap();
        assert_eq!(first, again);
        assert!(desk.paired().is_some());
        assert!(!desk.code_reused());
    }

    /// The redeemed code is evidence only until its window would have closed.
    #[test]
    fn a_redeemed_code_is_no_evidence_after_its_window() {
        let (_dir, desk, _phone, _device, _c) = unpaired_but_redeemed("alarm-late");
        let (hash, opened_at) = desk.state.lock().unwrap().redeemed.clone().unwrap();
        desk.state.lock().unwrap().redeemed = Some((hash, opened_at - PAIRING_WINDOW_MS - 1));
        // Whatever code is presented now, nothing is forgotten on its strength.
        assert_eq!(
            desk.complete_pairing("ANYTHING", &PublicKeyForm::Jwk(Phone::new().jwk()), "x", PairedVia::CONNECT, Platform::OTHER),
            Err(Refusal::NoDevice)
        );
        assert!(desk.paired().is_some());
    }

    /// **AN ABANDONED PAIRING NEVER LEAVES A KEY BEHIND** (§3.1 step 6). Past its window an
    /// unconfirmed device is refused like one that does not exist — its own answer included — a
    /// late press activates nothing, and the sweep forgets it. A confirmed device is never swept.
    #[test]
    fn an_unconfirmed_device_is_forgotten_when_its_window_closes() {
        let (dir, desk, phone, device, challenge) = unpaired_but_redeemed("expiry");
        assert_eq!(device.confirm_by, desk.state.lock().unwrap().redeemed.as_ref().unwrap().1 + PAIRING_WINDOW_MS);
        assert_eq!(desk.pairing_deadline(), Some(device.confirm_by));
        assert_eq!(desk.expire_lapsed().unwrap(), None, "a device inside its window was swept");

        desk.state.lock().unwrap().device.as_mut().unwrap().confirm_by = super::super::now_millis() - 1;
        let answer = present(&phone, &device.id, &challenge, "POST", "/api/pair", b"{\"fingerprint_confirmed\":true}");
        assert_eq!(desk.verify_for_confirmation(&answer), Err(Refusal::NoDevice));
        assert!(matches!(desk.confirm_on_mac(), Err(PhoneError::NotPaired)), "a press after the window activated the key");
        assert_eq!(desk.expire_lapsed().unwrap(), Some(StoppedBy::EXPIRED));
        assert!(desk.paired().is_none());
        assert!(DeviceDesk::open(&dir.0).unwrap().paired().is_none(), "the abandoned key survived a relaunch");
        assert_eq!(desk.pairing_deadline(), None);

        // POSITIVE CONTROL: a confirmed device with a long-past confirm_by is untouched.
        let (_dir2, desk2, _p2, _d2, _c2) = paired("expiry-confirmed");
        desk2.state.lock().unwrap().device.as_mut().unwrap().confirm_by = 1;
        assert_eq!(desk2.expire_lapsed().unwrap(), None);
        assert!(desk2.paired().is_some());
        assert_eq!(desk2.pairing_deadline(), None, "a confirmed phone has no deadline");
    }

    /// The awaiting answer is a statement about a pairing, so only the key's holder may learn it:
    /// a caller with the right device id and the wrong key still gets the flat refusal.
    #[test]
    fn only_the_key_holder_learns_that_a_pairing_is_waiting() {
        let (_dir, desk, _phone, device, challenge) = unpaired_but_redeemed("f1-leak");
        let impostor = Phone::new();
        let p = present(&impostor, &device.id, &challenge, "GET", "/api/events", b"");
        assert_eq!(desk.verify(&p), Err(Refusal::BadSignature));
        assert_eq!(desk.verify_for_confirmation(&p), Err(Refusal::BadSignature));
    }

    #[test]
    fn a_press_on_the_mac_with_nothing_paired_activates_nothing() {
        let dir = TempDir::new("f1-nothing");
        let desk = DeviceDesk::open(&dir.0).unwrap();
        assert!(matches!(desk.confirm_on_mac(), Err(PhoneError::NotPaired)));
        assert!(desk.paired().is_none());
    }

    /// A record written before the field existed is a phone that is already working; this build
    /// must not lock every paired phone out on its first launch.
    #[test]
    fn a_record_from_before_the_mac_press_existed_stays_active() {
        let legacy = serde_json::json!({
            "id": "dev_abc123", "name": "iPhone", "public_key": "BA", "paired_at": 1,
            "push": null, "delivered_cursor": null, "fingerprint_confirmed": true
        });
        let device: Device = serde_json::from_value(legacy).unwrap();
        assert!(device.mac_confirmed);
        assert!(device.trusted());
        // Nor may it be swept as a pairing its phone never finished (Sage's pair-v2 review §2).
        assert!(device.completed, "a record from before `completed` existed was read as unfinished");
        assert!(!device.finish_lapsed(u64::MAX));
    }

    // --- Sage's pair-v2 hypotheses review §1: the held ask, and what releases it ---------------

    /// A waker that only records that it was woken, so a hold can be polled by hand.
    struct Flag(std::sync::atomic::AtomicBool);
    impl std::task::Wake for Flag {
        fn wake(self: Arc<Self>) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
    impl Flag {
        fn new() -> Arc<Self> {
            Arc::new(Flag(std::sync::atomic::AtomicBool::new(false)))
        }
        fn woken(&self) -> bool {
            self.0.load(std::sync::atomic::Ordering::SeqCst)
        }
    }
    fn poll_hold(hold: &mut HeldAnswer, flag: &Arc<Flag>) -> Poll<HoldEnd> {
        let waker = Waker::from(Arc::clone(flag));
        Pin::new(hold).poll(&mut Context::from_waker(&waker))
    }

    /// **THE PRESS ON THE MAC REACHES THE HELD PHONE AT ONCE.** Before this, the phone learned of
    /// the press only at its next scheduled ask — up to 15 s later — so the Mac said "It is paired"
    /// while the phone beside it still said "Now press They match on your Mac".
    #[test]
    fn the_press_on_the_mac_releases_a_held_ask_at_once() {
        let (_dir, desk, _phone, _device, _c) = unpaired_but_redeemed("hold-press");
        let since = desk.hold_generation();
        let mut hold = desk.hold_answer(since);
        let flag = Flag::new();
        assert_eq!(poll_hold(&mut hold, &flag), Poll::Pending, "the ask was not held");
        assert!(!flag.woken());
        desk.confirm_on_mac().unwrap();
        assert!(flag.woken(), "the press on the Mac did not wake the held ask");
        assert_eq!(poll_hold(&mut hold, &flag), Poll::Ready(HoldEnd::Released));
    }

    /// A press that lands after the listener computed "still waiting" and before the hold began is
    /// not missed: the generation read before the request moved, so the hold ends at once.
    #[test]
    fn a_press_between_the_answer_and_the_hold_is_not_missed() {
        let (_dir, desk, _phone, _device, _c) = unpaired_but_redeemed("hold-race");
        let since = desk.hold_generation();
        desk.confirm_on_mac().unwrap();
        let mut hold = desk.hold_answer(since);
        assert_eq!(poll_hold(&mut hold, &Flag::new()), Poll::Ready(HoldEnd::Released));
    }

    /// **AT MOST ONE HELD ASK.** A new ask answers the previous one at once ("still waiting"), so
    /// the holder of a key can never occupy more than one of the listener's connection slots.
    #[test]
    fn a_new_ask_answers_the_held_one_and_only_one_is_held() {
        let (_dir, desk, _phone, _device, _c) = unpaired_but_redeemed("hold-one");
        let since = desk.hold_generation();
        let (first_flag, second_flag) = (Flag::new(), Flag::new());
        let mut first = desk.hold_answer(since);
        assert_eq!(poll_hold(&mut first, &first_flag), Poll::Pending);
        let mut second = desk.hold_answer(since);
        assert!(first_flag.woken(), "the older hold was not told a newer ask replaced it");
        assert_eq!(poll_hold(&mut first, &first_flag), Poll::Ready(HoldEnd::Superseded));
        assert_eq!(poll_hold(&mut second, &second_flag), Poll::Pending, "the newer ask is not the one held");
        // And the older one, finished, cannot clear the newer one's slot on its way out.
        drop(first);
        desk.confirm_on_mac().unwrap();
        assert!(second_flag.woken(), "dropping the superseded hold cleared the live one");
    }

    /// Every other way a pairing ends releases the hold too: `They do not match` on either side
    /// (both forget the device), the expiry sweep, and the channel's teardown.
    #[test]
    fn every_way_a_pairing_ends_releases_the_held_ask() {
        type End = fn(&DeviceDesk);
        let ends: [(&str, End); 3] = [
            ("They do not match (forget)", |desk| desk.forget().unwrap()),
            ("the expiry sweep", |desk| {
                desk.state.lock().unwrap().device.as_mut().unwrap().confirm_by = super::super::now_millis() - 1;
                assert_eq!(desk.expire_lapsed().unwrap(), Some(StoppedBy::EXPIRED));
            }),
            ("the channel's teardown", |desk| desk.release_holds()),
        ];
        for (name, end) in ends {
            let (_dir, desk, _phone, _device, _c) = unpaired_but_redeemed("hold-ends");
            let flag = Flag::new();
            let mut hold = desk.hold_answer(desk.hold_generation());
            assert_eq!(poll_hold(&mut hold, &flag), Poll::Pending, "{name}");
            end(&desk);
            assert!(flag.woken(), "{name} did not wake the held ask");
            assert_eq!(poll_hold(&mut hold, &flag), Poll::Ready(HoldEnd::Released), "{name}");
        }
    }

    /// A hold that timed out or whose connection went away gives up the slot: nothing is left for
    /// a later release to wake.
    #[test]
    fn a_hold_that_ends_by_itself_gives_up_its_slot() {
        let (_dir, desk, _phone, _device, _c) = unpaired_but_redeemed("hold-drop");
        let mut hold = desk.hold_answer(desk.hold_generation());
        assert_eq!(poll_hold(&mut hold, &Flag::new()), Poll::Pending);
        drop(hold);
        assert!(desk.state.lock().unwrap().hold.current.is_none(), "a finished hold kept its slot");
    }

    // --- Sage's pair-v2 hypotheses review §2: "paired" needs the phone to have used it ----------

    /// **THE PRESS ON THE MAC IS NOT THE PHONE HEARING IT.** The phone's own asks while it waits
    /// never complete the pairing; the first ordinary request after the press does, and that
    /// survives a relaunch.
    #[test]
    fn the_phone_using_the_pairing_is_what_completes_it() {
        let (dir, desk, phone, device, challenge) = unpaired_but_redeemed("complete");
        assert!(!device.completed, "a pairing was born completed");
        let ask = present(&phone, &device.id, &challenge, "POST", "/api/pair", b"{\"fingerprint_confirmed\":true}");
        let ordinary = present(&phone, &device.id, &challenge, "GET", "/api/events", b"");

        assert_eq!(desk.verify(&ordinary), Err(Refusal::AwaitingMacConfirmation));
        desk.confirm_on_mac().unwrap();
        assert!(desk.verify_for_confirmation(&ask).is_ok());
        assert!(!desk.paired().unwrap().completed, "the phone's own ask while waiting was taken as using the pairing");
        assert!(!DeviceDesk::open(&dir.0).unwrap().paired().unwrap().completed);

        let used = desk.verify(&ordinary).expect("the first ordinary request after the press");
        assert!(used.completed, "verify returned the device as unfinished");
        assert!(desk.paired().unwrap().completed, "the first ordinary request did not complete the pairing");
        assert!(DeviceDesk::open(&dir.0).unwrap().paired().unwrap().completed, "completion did not survive a relaunch");
        assert_eq!(desk.pairing_deadline(), None, "a completed pairing still has a deadline");
    }

    /// **A PAIRING THIS MAC ACCEPTED AND THE PHONE NEVER USED IS FORGOTTEN AFTER THE GRACE** —
    /// Sage's review §2, variant 2a: the phone lost the Mac after its own press, the person pressed
    /// here, and the phone deleted its key at its deadline. On `main` this Mac kept that key as
    /// "It is paired" forever and refused every new pairing until `Forget this phone`.
    #[test]
    fn a_pairing_the_phone_never_used_is_forgotten_after_the_grace() {
        let (dir, desk, phone, device, challenge) = pressed_unfinished("unfinished");
        assert_eq!(desk.pairing_deadline(), Some(device.confirm_by + UNFINISHED_GRACE_MS));
        assert_eq!(desk.expire_lapsed().unwrap(), None, "a pairing inside its grace was swept");

        // Just inside the grace: kept.
        let now = super::super::now_millis();
        desk.state.lock().unwrap().device.as_mut().unwrap().confirm_by = now - UNFINISHED_GRACE_MS + 60_000;
        assert_eq!(desk.expire_lapsed().unwrap(), None);
        // Past it: forgotten, with its own reason, and the key is refused as forgotten.
        desk.state.lock().unwrap().device.as_mut().unwrap().confirm_by = now - UNFINISHED_GRACE_MS - 1;
        assert_eq!(desk.expire_lapsed().unwrap(), Some(StoppedBy::UNFINISHED));
        assert!(desk.paired().is_none());
        assert!(DeviceDesk::open(&dir.0).unwrap().paired().is_none(), "the unfinished key survived a relaunch");
        let p = present(&phone, &device.id, &challenge, "GET", "/api/events", b"");
        assert_eq!(desk.verify(&p), Err(Refusal::Revoked));
        // And a new code can be shown: the slot is free.
        assert!(desk.open_pairing().is_ok(), "the forgotten pairing still holds the one slot");

        // POSITIVE CONTROL: a completed pairing with the same long-past deadline is untouched.
        let (_dir2, desk2, _p2, _d2, _c2) = paired("unfinished-control");
        desk2.state.lock().unwrap().device.as_mut().unwrap().confirm_by = 1;
        assert_eq!(desk2.expire_lapsed().unwrap(), None);
        assert!(desk2.paired().is_some());
    }

    #[test]
    fn the_unfinished_reason_is_read_back_from_disk() {
        let dir = TempDir::new("unfinished-record");
        record_rejection(&dir.0, 1, StoppedBy::UNFINISHED).unwrap();
        assert_eq!(rejection_by(&dir.0), StoppedBy::UNFINISHED);
    }

    /// Build a signed request the way the phone will.
    fn present<'a>(
        phone: &Phone,
        device_id: &'a str,
        challenge: &'a str,
        method: &'a str,
        path: &'a str,
        body: &'a [u8],
    ) -> Presented<'a> {
        let message = signing_string(challenge, method, path, body);
        Presented {
            method,
            path_with_query: path,
            device_id,
            challenge,
            signature: phone.sign(&message),
            body,
        }
    }

    // --- the signed string, against the phone's own definition -----------------------------

    #[test]
    fn the_signed_string_is_the_one_the_landed_phone_builds() {
        // `web/web-app/lib/api.js`:
        //   `${challenge}\n${method.toUpperCase()}\n${pathWithQuery}\n${bodyHashHex || ''}`
        // Pinned byte for byte, because a difference of one newline is a day of two people
        // each being sure they are right.
        assert_eq!(
            signing_string("CH", "post", "/api/messages", b"{\"a\":1}"),
            format!("CH\nPOST\n/api/messages\n{}", hex(&sha256(b"{\"a\":1}")))
        );
        // AND THE EMPTY-BODY CASE, which is the one worth pinning: the phone sends the EMPTY
        // STRING, not the SHA-256 of zero bytes. A Mac that hashed nothing-as-bytes would
        // refuse every GET the phone ever makes.
        assert_eq!(signing_string("CH", "GET", "/api/events", b""), "CH\nGET\n/api/events\n");
        assert!(!signing_string("CH", "GET", "/api/events", b"").contains("e3b0c442"));
    }

    #[test]
    fn the_authorization_header_is_parsed_the_way_the_phone_writes_it() {
        let sig = super::super::b64url(&[7u8; 64]);
        let header = format!("RichOS-Device dev_abc.CHALLENGE.{sig}");
        let (id, challenge, bytes) = parse_authorization(&header).expect(&header);
        assert_eq!(id, "dev_abc");
        assert_eq!(challenge, "CHALLENGE");
        assert_eq!(bytes, vec![7u8; 64]);

        // The refusals, each for its own reason.
        assert!(parse_authorization("Bearer abc").is_none(), "another scheme was accepted");
        assert!(parse_authorization("RichOS-Device dev_abc.CHALLENGE").is_none(), "two parts");
        assert!(parse_authorization("RichOS-Device .CHALLENGE.AAAA").is_none(), "empty device id");
        assert!(parse_authorization("RichOS-Device dev_abc..AAAA").is_none(), "empty challenge");
        assert!(parse_authorization(&format!("RichOS-Device dev_abc.CH.{}", "not base64!")).is_none());
    }

    // --- pairing ------------------------------------------------------------------------------

    #[test]
    fn a_paired_phone_survives_a_relaunch() {
        let dir = TempDir::new("survive");
        let phone = Phone::new();
        let id = {
            let desk = DeviceDesk::open(&dir.0).unwrap();
            let w = desk.open_pairing().unwrap();
            desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::HOME, Platform::IOS).unwrap().id
        };
        let again = DeviceDesk::open(&dir.0).unwrap();
        let device = again.paired().expect("the paired phone did not survive a relaunch");
        assert_eq!(device.id, id);
        assert_eq!(device.public_key, super::super::b64url(&phone.point));
        assert_eq!(device.push_transport, "web-push");
    }

    /// **RAY'S DEFECT 3.2, ON THE MAC SIDE: the path and the platform are on the RECORD.**
    ///
    /// The card that offers "Forget this phone" was deriving its certificate-removal copy from
    /// the sheet's `route` variable, which `ui/phone.js` deliberately forgets on every open. So
    /// reopening it printed iOS profile-removal steps for an Android phone that had paired over
    /// Tailscale and installed nothing. The fix is that the answer is written down once, here,
    /// and these are the four combinations the card has to be able to tell apart.
    #[test]
    fn what_a_phone_paired_over_and_what_kind_of_phone_it_is_are_both_recorded() {
        for (via, platform, name) in [
            (PairedVia::TAILNET, Platform::ANDROID, "Android phone"),
            (PairedVia::TAILNET, Platform::IOS, "iPhone"),
            (PairedVia::HOME, Platform::ANDROID, "Android phone"),
            (PairedVia::HOME, Platform::IOS, "iPhone"),
        ] {
            let dir = TempDir::new(&format!("recorded-{via}-{platform}"));
            let desk = DeviceDesk::open(&dir.0).unwrap();
            let phone = Phone::new();
            let w = desk.open_pairing().unwrap();
            let device = desk
                .complete_pairing(&w.code, &PublicKeyForm::Jwk(phone.jwk()), name, via, platform)
                .unwrap();
            assert_eq!(device.paired_via, via);
            assert_eq!(device.platform, platform);
            // AND IT SURVIVES A RELAUNCH, which is the whole point of putting it on disk: a
            // fresh desk over the same directory reads the same two answers back.
            let again = DeviceDesk::open(&dir.0).unwrap().paired().unwrap();
            assert_eq!(again.paired_via, via, "the path did not survive a reopen");
            assert_eq!(again.platform, platform, "the platform did not survive a reopen");
        }
    }

    /// **A RECORD WRITTEN BEFORE THOSE TWO FIELDS EXISTED IS `UNKNOWN`, NOT ONE OF THE TWO.**
    ///
    /// The whole defect was a screen picking a default and stating it as fact. A legacy record
    /// deserializing to `"home"` would rebuild that defect inside the type instead of inside the
    /// sheet, so absence stays absence and the card has its own sentence for it.
    #[test]
    fn a_record_from_before_these_fields_reads_as_unknown_rather_than_as_a_default() {
        let legacy = serde_json::json!({
            "id": "dev_abc123",
            "name": "Android phone",
            "public_key": "BA",
            "paired_at": 1,
            "push": null,
            "delivered_cursor": null
        });
        let device: Device = serde_json::from_value(legacy).unwrap();
        assert_eq!(device.paired_via, PairedVia::UNKNOWN);
        assert_eq!(device.platform, Platform::UNKNOWN);
        assert_eq!(device.push_transport, "web-push", "the day-one default still applies");
    }

    /// The only platform evidence the pairing request carries today is the name the phone chose
    /// for itself — `web/web-app/app.js:236` sends exactly these four.
    #[test]
    fn the_platform_is_read_off_the_name_the_phone_sent() {
        assert_eq!(platform_of_name("iPhone"), Platform::IOS);
        assert_eq!(platform_of_name("iPad"), Platform::IOS);
        assert_eq!(platform_of_name("Android phone"), Platform::ANDROID);
        assert_eq!(platform_of_name("Phone"), Platform::OTHER);
        // Case and surrounding words do not decide it: the value is a human-facing label, and
        // the first rename box anybody adds turns "iPhone" into "Sam's iPhone".
        assert_eq!(platform_of_name("alex's IPHONE"), Platform::IOS);
        assert_eq!(platform_of_name("Android phone (android)"), Platform::ANDROID);
    }

    #[test]
    fn wrong_pairing_codes_do_not_consume_the_legitimate_window() {
        let dir = TempDir::new("one-shot");
        let desk = DeviceDesk::open(&dir.0).unwrap();
        let phone = Phone::new();
        let w = desk.open_pairing().unwrap();
        assert!(matches!(desk.complete_pairing("WRONGCOD", &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::HOME, Platform::IOS), Err(Refusal::BadSignature)));
        assert_eq!(desk.pairing_window().unwrap().code, w.code);
        assert!(desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::HOME, Platform::IOS).is_ok());
        assert!(desk.pairing_window().is_none());
        // One shot for anybody ELSE. The same key asking again is the same phone (see
        // `the_same_phone_redeeming_its_code_twice_is_not_the_alarm`); a different key with the
        // same code is the spent-code alarm.
        assert!(desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(Phone::new().jwk()), "iPhone", PairedVia::HOME, Platform::IOS).is_err());
    }

    #[test]
    fn pairing_attempts_are_bounded_without_cancelling_the_window() {
        let dir = TempDir::new("bounded-pairing");
        let desk = DeviceDesk::open(&dir.0).unwrap();
        let phone = Phone::new();
        let w = desk.open_pairing().unwrap();
        for _ in 0..RATE_LIMIT {
            assert!(desk.complete_pairing("WRONGCOD", &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::HOME, Platform::IOS).is_err());
        }
        assert!(matches!(desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::HOME, Platform::IOS), Err(Refusal::RateLimited)));
        assert_eq!(desk.pairing_window().unwrap().code, w.code);
        desk.state.lock().unwrap().pairing_requests.clear();
        assert!(desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::HOME, Platform::IOS).is_ok());
    }

    #[test]
    fn anonymous_challenge_flood_cannot_evict_a_live_phone_challenge() {
        let (_dir, desk, phone, device, challenge) = paired("flood");
        for _ in 0..1000 { desk.issue_challenge().unwrap(); }
        let signature = phone.sign(&signing_string(&challenge, "POST", "/api/messages", b"hello"));
        let request = Presented { device_id: &device.id, challenge: &challenge, signature, method: "POST", path_with_query: "/api/messages", body: b"hello" };
        assert!(desk.verify(&request).is_ok());
        assert!(desk.state.lock().unwrap().challenges.len() < LIVE_CHALLENGES);
    }

    #[test]
    fn an_expired_window_pairs_nothing() {
        let dir = TempDir::new("expired");
        let desk = DeviceDesk::open(&dir.0).unwrap();
        let mut w = desk.open_pairing().unwrap();
        w.opened_at = super::super::now_millis() - PAIRING_WINDOW_MS - 1;
        assert!(!w.is_open(super::super::now_millis()));
        assert!(w.is_open(w.opened_at + PAIRING_WINDOW_MS), "the boundary itself must be open");
    }

    #[test]
    fn only_one_phone_can_be_paired_at_a_time() {
        let (_dir, desk, _phone, _device, _c) = paired("one-device");
        assert!(desk.open_pairing().is_err(), "a second window opened while a phone was paired");
        desk.forget().unwrap();
        assert!(desk.open_pairing().is_ok());
    }

    #[test]
    fn the_pairing_code_avoids_every_character_people_confuse() {
        for _ in 0..50 {
            let w = PairingWindow::open().unwrap();
            assert_eq!(w.code.len(), CODE_LENGTH);
            for c in w.code.chars() {
                assert!(CODE_ALPHABET.contains(&(c as u8)), "{c} is not in the alphabet");
                assert!(!"ILOU01".contains(c), "{c} is a character people confuse");
            }
        }
    }

    #[test]
    fn the_listener_runs_while_a_window_is_open_or_a_phone_is_paired_and_at_no_other_time() {
        let dir = TempDir::new("lifecycle");
        let desk = DeviceDesk::open(&dir.0).unwrap();
        assert!(!desk.listener_should_run(), "the socket exists before he has done anything");
        let w = desk.open_pairing().unwrap();
        assert!(desk.listener_should_run(), "the socket is down during pairing");
        let phone = Phone::new();
        desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::HOME, Platform::IOS).unwrap();
        assert!(desk.listener_should_run());
        desk.forget().unwrap();
        assert!(!desk.listener_should_run(), "the socket outlived the last paired phone");
    }

    /// **URBAN'S G2, at the layer that decides whether the socket lives.** *"`Pick a different
    /// way` must be on the pairing screen too, and it must stop serving."*
    ///
    /// The half that is easy to get wrong is the second assertion, not the first: backing out of
    /// a pairing window on a Mac that ALREADY HAS A PHONE must not take that phone offline. One
    /// desk answers both, which is the point — `listener_should_run` is the single expression of
    /// "should the socket exist", so a back button wired to it cannot disagree with the boot path
    /// or with `forget`.
    #[test]
    fn backing_out_of_a_pairing_window_takes_the_socket_down_unless_a_phone_is_paired() {
        let dir = TempDir::new("close-pairing");
        let desk = DeviceDesk::open(&dir.0).unwrap();

        // He pressed `Set my phone up`, then `Pick a different way`. Nothing is paired, so
        // there is no longer any reason for the socket to exist.
        let w = desk.open_pairing().unwrap();
        assert!(desk.listener_should_run());
        assert!(desk.pairing_window().is_some(), "the window was not open to begin with");
        desk.close_pairing();
        assert!(
            desk.pairing_window().is_none(),
            "the code he backed out of is still redeemable"
        );
        assert!(
            !desk.listener_should_run(),
            "the listener outlived the pairing window the user closed — Urban's G2, \
             `Pick a different way` that does not stop serving"
        );

        // AND THE SAME CALL ON A MAC WITH A PHONE ON IT LEAVES THE PHONE CONNECTED. Reached the
        // way a person reaches it: open a window, pair through it, then close.
        let w2 = desk.open_pairing().unwrap();
        let phone = Phone::new();
        desk.complete_pairing(&w2.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::TAILNET, Platform::IOS)
            .unwrap();
        assert!(desk.listener_should_run());
        desk.close_pairing();
        assert!(
            desk.listener_should_run(),
            "closing a pairing window unplugged a phone that was already paired"
        );
        let _ = w;
    }

    // --- the credential -----------------------------------------------------------------------

    #[test]
    fn a_correctly_signed_request_is_accepted() {
        // The positive control every refusal below is measured against.
        let (_dir, desk, phone, device, challenge) = paired("accept");
        let p = present(&phone, &device.id, &challenge, "POST", "/api/messages", b"{}");
        assert!(desk.verify(&p).is_ok());
    }

    #[test]
    fn an_unpaired_caller_is_refused_before_anything_else_happens() {
        let dir = TempDir::new("unpaired");
        let desk = DeviceDesk::open(&dir.0).unwrap();
        let phone = Phone::new();
        let challenge = desk.issue_challenge().unwrap();
        let p = present(&phone, "dev_invented", &challenge, "POST", "/api/messages", b"{}");
        assert_eq!(desk.verify(&p), Err(Refusal::NoDevice));
    }

    #[test]
    fn a_forgotten_phone_gets_a_final_answer_and_a_stranger_gets_nothing() {
        // The one deliberate non-404 in the surface, and its negative control in the same
        // test: the phone the Mac remembers forgetting is told so, and a device id that was
        // never paired still learns nothing.
        let (dir, desk, phone, device, _c) = paired("revoked");
        let saved_device = std::fs::read(&desk.path).unwrap();
        desk.forget().unwrap();
        // Simulate interruption after the revocation was persisted but before key deletion.
        std::fs::write(&desk.path, saved_device).unwrap();
        drop(desk);
        let desk = DeviceDesk::open(&dir.0).unwrap();
        assert!(desk.paired().is_none());
        let challenge = desk.issue_challenge().unwrap();
        let known = present(&phone, &device.id, &challenge, "POST", "/api/messages", b"{}");
        assert_eq!(desk.verify(&known), Err(Refusal::Revoked));
        let stranger = present(&phone, "dev_never_seen", &challenge, "POST", "/api/messages", b"{}");
        assert_eq!(desk.verify(&stranger), Err(Refusal::NoDevice));
        let window = desk.open_pairing().unwrap();
        let replacement = Phone::new();
        desk.complete_pairing(&window.code, &PublicKeyForm::Jwk(replacement.jwk()), "new phone", PairedVia::TAILNET, Platform::IOS).unwrap();
        drop(desk);
        let desk = DeviceDesk::open(&dir.0).unwrap();
        assert!(desk.paired().is_some());
        assert_eq!(desk.verify(&known), Err(Refusal::Revoked));
        assert_eq!(desk.verify(&stranger), Err(Refusal::UnknownDevice));
    }

    #[test]
    fn forgotten_phone_history_is_bounded_and_malformed_history_refuses_to_open() {
        let dir = TempDir::new("revoked-history");
        let phone = Phone::new();
        let mut ids = Vec::new();
        for _ in 0..9 {
            let phone = Phone::new();
            let desk = DeviceDesk::open(&dir.0).unwrap();
            let window = desk.open_pairing().unwrap();
            let device = desk.complete_pairing(&window.code, &PublicKeyForm::Jwk(phone.jwk()), "phone", PairedVia::TAILNET, Platform::IOS).unwrap();
            ids.push(device.id);
            desk.forget().unwrap();
        }
        let desk = DeviceDesk::open(&dir.0).unwrap();
        let challenge = desk.issue_challenge().unwrap();
        assert_eq!(desk.state.lock().unwrap().revoked.len(), 8);
        assert_eq!(desk.verify(&present(&phone, &ids[0], &challenge, "GET", "/api/events", b"")), Err(Refusal::NoDevice));
        assert_eq!(desk.verify(&present(&phone, &ids[8], &challenge, "GET", "/api/events", b"")), Err(Refusal::Revoked));
        std::fs::write(dir.0.join("phone/revoked.json"), b"damaged").unwrap();
        assert!(DeviceDesk::open(&dir.0).is_err());
    }

    #[test]
    fn explicit_pairing_can_authorize_a_forgotten_key_again() {
        let (dir, desk, phone, _, _) = paired("re-pair");
        desk.forget().unwrap();
        let window = desk.open_pairing().unwrap();
        let device = desk.complete_pairing(&window.code, &PublicKeyForm::Jwk(phone.jwk()), "phone", PairedVia::TAILNET, Platform::IOS).unwrap();
        desk.confirm_on_mac().unwrap();
        drop(desk);
        let desk = DeviceDesk::open(&dir.0).unwrap();
        let challenge = desk.issue_challenge().unwrap();
        assert!(desk.verify(&present(&phone, &device.id, &challenge, "GET", "/api/events", b"")).is_ok());
    }

    #[test]
    fn a_failed_revocation_write_reports_failure_and_preserves_the_pairing() {
        let (dir, desk, phone, device, challenge) = paired("revoke-write-failed");
        std::fs::create_dir(dir.0.join("phone/revoked.pending")).unwrap();
        assert!(desk.forget().is_err());
        let presented = present(&phone, &device.id, &challenge, "GET", "/api/events", b"");
        assert!(desk.verify(&presented).is_ok());
        assert!(DeviceDesk::open(&dir.0).unwrap().paired().is_some());
    }

    #[test]
    fn a_different_key_is_refused_even_with_the_right_device_id_and_challenge() {
        // The device id and the challenge both travel in the clear on every request. What
        // authenticates is the signature.
        let (_dir, desk, _phone, device, challenge) = paired("wrong-key");
        let impostor = Phone::new();
        let p = present(&impostor, &device.id, &challenge, "POST", "/api/messages", b"{}");
        assert_eq!(desk.verify(&p), Err(Refusal::BadSignature));
    }

    #[test]
    fn a_challenge_the_mac_never_issued_is_refused() {
        // "Server-issued and never client-generated" — asserted rather than described. A phone
        // that invented its own challenge could sign anything at any time.
        let (_dir, desk, phone, device, _real) = paired("invented");
        let invented = super::super::b64url(&[9u8; 24]);
        let p = present(&phone, &device.id, &invented, "POST", "/api/messages", b"{}");
        assert_eq!(desk.verify(&p), Err(Refusal::UnknownChallenge));
    }

    #[test]
    fn one_changed_byte_anywhere_in_the_signed_string_is_a_refusal() {
        // Each of the four signed fields is proven covered rather than assumed to be: the
        // signature is made over one value and the request presents another.
        let (_dir, desk, phone, device, challenge) = paired("bind");
        let good = present(&phone, &device.id, &challenge, "POST", "/api/messages", b"{\"a\":1}");
        assert!(desk.verify(&good).is_ok());

        let mut swapped = present(&phone, &device.id, &challenge, "POST", "/api/messages", b"{\"a\":1}");
        swapped.method = "GET";
        assert_eq!(desk.verify(&swapped), Err(Refusal::BadSignature), "the method is not covered");

        let mut moved = present(&phone, &device.id, &challenge, "POST", "/api/messages", b"{\"a\":1}");
        moved.path_with_query = "/api/events";
        assert_eq!(desk.verify(&moved), Err(Refusal::BadSignature), "the path is not covered");

        let mut tampered = present(&phone, &device.id, &challenge, "POST", "/api/messages", b"{\"a\":1}");
        tampered.body = b"{\"a\":2}";
        assert_eq!(desk.verify(&tampered), Err(Refusal::BadSignature), "the body is not covered");

        desk.state.lock().unwrap().challenges.back_mut().unwrap().1 -= 30_000;
        let second = desk.issue_challenge().unwrap();
        let mut rechallenged =
            present(&phone, &device.id, &challenge, "POST", "/api/messages", b"{\"a\":1}");
        rechallenged.challenge = &second;
        assert_eq!(desk.verify(&rechallenged), Err(Refusal::BadSignature), "the challenge is not covered");
    }

    #[test]
    fn a_challenge_may_be_presented_more_than_once_and_this_is_a_stated_property() {
        // NOT AN OVERSIGHT. The phone replays the most recent challenge it holds and cannot
        // know a newer one before the response that carries it, and `EventSource` reconnects
        // to one URL. If this refused, the phone would break on its second request.
        let (_dir, desk, phone, device, challenge) = paired("reuse");
        for attempt in 0..5 {
            let p = present(&phone, &device.id, &challenge, "GET", "/api/events", b"");
            assert!(desk.verify(&p).is_ok(), "presentation {attempt} was refused");
        }
    }

    #[test]
    fn a_challenge_older_than_its_lifetime_is_refused() {
        // The bound the paragraph above trades against: a captured request is worthless after
        // ten minutes. Aged by hand rather than by sleeping for ten minutes.
        let (_dir, desk, phone, device, challenge) = paired("stale");
        {
            let mut state = desk.state.lock().unwrap();
            for entry in state.challenges.iter_mut() {
                entry.1 = super::super::now_millis() - CHALLENGE_LIFETIME_MS - 1;
            }
        }
        let p = present(&phone, &device.id, &challenge, "GET", "/api/events", b"");
        assert_eq!(desk.verify(&p), Err(Refusal::StaleChallenge));
        // POSITIVE CONTROL: a freshly issued challenge on the same desk works.
        let fresh = desk.issue_challenge().unwrap();
        let ok = present(&phone, &device.id, &fresh, "GET", "/api/events", b"");
        assert!(desk.verify(&ok).is_ok());
    }

    #[test]
    fn only_the_most_recent_challenges_stay_live() {
        // The set is bounded, so a caller that only ever asks for challenges cannot make it
        // grow. The oldest one falls out and stops working.
        let (_dir, desk, phone, device, first) = paired("bounded");
        for _ in 0..LIVE_CHALLENGES {
            desk.state.lock().unwrap().challenges.back_mut().unwrap().1 -= 30_000;
            desk.issue_challenge().unwrap();
        }
        let p = present(&phone, &device.id, &first, "GET", "/api/events", b"");
        assert_eq!(desk.verify(&p), Err(Refusal::UnknownChallenge));
    }

    /// **THE DEFECT THIS SIZE EXISTS FOR** — Ray's nightly `.7` walk, defect 1.
    ///
    /// A static asset GET carries no credential and tells the phone nothing it can read: a
    /// browser's own subresource loads and its `EventSource` cannot see a response header. But
    /// [`super::super::listen::render`] mints a challenge for every response this port gives, so
    /// until this commit each one of them pushed an entry out of a set of 16 — and
    /// the phone web page's service worker (retired 2026-10-30) fetched its whole 19-entry shell
    /// at install. One install therefore emptied the set of everything older than itself,
    /// including the challenge the phone was holding, and the next send was refused as a flat 404.
    ///
    /// That burst, twice over, stays the floor: the widest run of credential-free responses this
    /// Mac has ever given between a phone reading a challenge and presenting it.
    #[test]
    fn challenges_survive_a_burst_of_credential_free_responses() {
        let (_dir, desk, phone, device, held) = paired("shell");
        let burst = 19 * 2;
        for _ in 0..burst {
            desk.issue_challenge().unwrap();
        }

        let p = present(&phone, &device.id, &held, "GET", "/api/messages", b"");
        assert_eq!(
            desk.verify(&p),
            Ok(device.clone()),
            "{burst} credential-free responses evicted the credential the phone is holding \
             (LIVE_CHALLENGES = {LIVE_CHALLENGES})"
        );
    }

    #[test]
    fn an_expired_challenge_gives_its_slot_back_rather_than_holding_it_in_front_of_a_live_one() {
        // Age is what retires a challenge; the count only stops the set growing. So a set full of
        // ten-minute-old entries must not be what pushes out the one the phone is using.
        let (_dir, desk, phone, device, _first) = paired("aged");
        for _ in 0..LIVE_CHALLENGES {
            desk.state.lock().unwrap().challenges.back_mut().unwrap().1 -= 30_000;
            desk.issue_challenge().unwrap();
        }
        {
            let mut state = desk.state.lock().unwrap();
            assert_eq!(state.challenges.len(), LIVE_CHALLENGES);
            for entry in state.challenges.iter_mut() {
                entry.1 = super::super::now_millis() - CHALLENGE_LIFETIME_MS - 1;
            }
        }
        // The phone gets a live one, and then the Mac answers a handful of asset requests.
        let held = desk.issue_challenge().unwrap();
        for _ in 0..8 {
            desk.issue_challenge().unwrap();
        }
        assert!(
            desk.state.lock().unwrap().challenges.len() <= 9,
            "the expired entries were carried rather than dropped"
        );
        let p = present(&phone, &device.id, &held, "GET", "/api/messages", b"");
        assert_eq!(desk.verify(&p), Ok(device));
    }

    #[test]
    fn a_shouting_caller_is_rate_limited_before_any_crypto_happens() {
        let (_dir, desk, phone, device, challenge) = paired("rate");
        for i in 0..RATE_LIMIT {
            let p = present(&phone, &device.id, &challenge, "POST", "/api/messages", b"{}");
            assert!(desk.verify(&p).is_ok(), "request {i} was refused early");
        }
        let one_more = present(&phone, &device.id, &challenge, "POST", "/api/messages", b"{}");
        assert_eq!(desk.verify(&one_more), Err(Refusal::RateLimited));
    }

    /// **A STRANGER MUST NOT BE ABLE TO LOCK THE PAIRED PHONE OUT** — PRD 2026-09-21 §7
    /// "Isolation/abuse": *"Another host/device cannot … exhaust its authenticated limits."*
    ///
    /// Found by `ray-opus-install2` from reading `verify`: step 0 pushed EVERY caller into one
    /// window before the device id was looked at, so sixty requests a minute from anybody who
    /// can reach the port — over Connect, anybody on the internet — refused the phone
    /// `RateLimited` for as long as they kept it up.
    #[test]
    fn a_stranger_cannot_exhaust_the_paired_phones_allowance() {
        let (_dir, desk, phone, device, challenge) = paired("stranger-flood");
        let stranger = Phone::new();
        let mut rate_limited = 0;
        for _ in 0..RATE_LIMIT * 3 {
            let p = present(&stranger, "dev_000000000000", &challenge, "POST", "/api/messages", b"{}");
            match desk.verify(&p) {
                Err(Refusal::RateLimited) => rate_limited += 1,
                Err(Refusal::UnknownDevice) => {}
                other => panic!("a stranger was answered {other:?}"),
            }
        }
        // POSITIVE CONTROL for the property that must be KEPT: strangers are still bounded.
        assert_eq!(rate_limited, RATE_LIMIT * 2, "strangers are no longer rate limited at all");

        let p = present(&phone, &device.id, &challenge, "POST", "/api/messages", b"{}");
        assert_eq!(
            desk.verify(&p),
            Ok(device),
            "{} stranger requests locked the paired phone out of its own Mac",
            RATE_LIMIT * 3
        );
    }

    /// **THE REVERSE: THE PAIRED PHONE SPENDING ITS OWN ALLOWANCE STARVES NOTHING ELSE.**
    ///
    /// The design (see [`RATE_LIMIT`]): a forgotten phone's answer is final and costs no bucket;
    /// strangers share one bucket of their own; the paired phone has its own, which belongs to
    /// the device record and starts empty when a new phone pairs; pairing attempts have theirs.
    #[test]
    fn the_paired_phone_flooding_its_own_allowance_starves_nothing_else() {
        let dir = TempDir::new("own-flood");
        let desk = Arc::new(DeviceDesk::open(&dir.0).unwrap());
        let pair = |phone: &Phone| {
            let w = desk.open_pairing().unwrap();
            desk.complete_pairing(&w.code, &PublicKeyForm::Jwk(phone.jwk()), "iPhone", PairedVia::CONNECT, Platform::IOS)
                .unwrap();
            desk.confirm_on_mac().unwrap();
            mark_completed(&desk);
            desk.paired().unwrap()
        };
        let forgotten = Phone::new();
        let forgotten_id = pair(&forgotten).id;
        desk.forget().unwrap();
        let loud = Phone::new();
        let loud_device = pair(&loud);
        let challenge = desk.issue_challenge().unwrap();

        for i in 0..RATE_LIMIT {
            let p = present(&loud, &loud_device.id, &challenge, "GET", "/api/events", b"");
            assert!(desk.verify(&p).is_ok(), "request {i} was refused inside the phone's own allowance");
        }
        let p = present(&loud, &loud_device.id, &challenge, "GET", "/api/events", b"");
        assert_eq!(desk.verify(&p), Err(Refusal::RateLimited), "the phone's own bucket is unbounded");

        // 1. The forgotten phone is still told, finally, rather than told to come back later.
        let p = present(&forgotten, &forgotten_id, &challenge, "GET", "/api/events", b"");
        assert_eq!(desk.verify(&p), Err(Refusal::Revoked), "a revoked phone was answered with the paired phone's limit");

        // 2. A stranger is answered from the stranger's bucket, not the phone's.
        let p = present(&forgotten, "dev_000000000000", &challenge, "GET", "/api/events", b"");
        assert_eq!(desk.verify(&p), Err(Refusal::UnknownDevice));

        // 3. Forgetting the loud phone and pairing another: the new phone is not refused for
        //    the minute the old one spent.
        desk.forget().unwrap();
        let next = Phone::new();
        let next_device = pair(&next);
        let challenge = desk.issue_challenge().unwrap();
        let p = present(&next, &next_device.id, &challenge, "GET", "/api/events", b"");
        assert_eq!(desk.verify(&p), Ok(next_device), "a newly paired phone inherited the last phone's limit");
    }

    /// **THE STATED RESIDUAL, pinned so nobody reads the fix as more than it is.** A caller that
    /// already holds the paired phone's device id — which travels only inside TLS, and is 48 bits
    /// of the phone's public key hash, never guessable — reaches the phone's own bucket with forged
    /// signatures, and is bounded there BEFORE any crypto. Bounded work wins over that caller's
    /// ability to spend the phone's minute; the id is not a secret we can rely on and the key is.
    #[test]
    fn forged_signatures_under_the_real_device_id_are_bounded_before_any_crypto() {
        let (_dir, desk, _phone, device, challenge) = paired("forged-id");
        let impostor = Phone::new();
        for _ in 0..RATE_LIMIT {
            let p = present(&impostor, &device.id, &challenge, "POST", "/api/messages", b"{}");
            assert_eq!(desk.verify(&p), Err(Refusal::BadSignature));
        }
        let p = present(&impostor, &device.id, &challenge, "POST", "/api/messages", b"{}");
        assert_eq!(desk.verify(&p), Err(Refusal::RateLimited));
    }

    #[test]
    fn a_stream_slot_comes_back_when_the_stream_ends_however_it_ends() {
        let (_dir, desk, _phone, _device, _c) = paired("slots");
        // Distinct devices, so no stream replaces another and the cap itself is what is measured.
        let mut held = Vec::new();
        for i in 0..MAX_STREAMS {
            held.push(desk.claim_stream(&format!("dev_{i}")).unwrap_or_else(|e| panic!("slot {i}: {e:?}")));
        }
        assert_eq!(desk.open_streams(), MAX_STREAMS);
        assert_eq!(desk.claim_stream("dev_new").err(), Some(Refusal::TooManyStreams));
        held.clear();
        assert_eq!(desk.open_streams(), 0, "a slot was not returned");
        assert!(desk.claim_stream("dev_new").is_ok());
    }

    /// **A DEVICE'S NEW STREAM REPLACES ITS OWN OLD ONES AND NOBODY ELSE'S** (2026-10-01 iPhone
    /// walk, defect D4). Four streams held for one phone, as four killed processes leave them:
    /// the fifth from that phone is admitted, the four are told to end and stop being counted,
    /// and another device's stream is neither ended nor uncounted. The cap still refuses the
    /// stream past [`MAX_STREAMS`] distinct devices.
    #[test]
    fn a_devices_new_stream_replaces_its_own_old_ones_and_never_another_devices() {
        let (_dir, desk, _phone, _device, _c) = paired("newest-wins");
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let ended_within = |slot: &StreamSlot| {
            // The timer is built inside the runtime: `timeout` registers with the reactor it is made in.
            runtime.block_on(async { tokio::time::timeout(std::time::Duration::from_millis(50), slot.ended()).await }).is_ok()
        };

        let other = desk.claim_stream("dev_other").unwrap();
        let mut old = Vec::new();
        for i in 0..MAX_STREAMS - 1 {
            old.push(desk.claim_stream("dev_phone").unwrap_or_else(|e| panic!("open {i}: {e:?}")));
        }
        assert_eq!(desk.open_streams(), 2, "each open from one phone replaced the one before it");
        let newest = desk.claim_stream("dev_phone").expect("the phone was refused by its own earlier streams");
        assert_eq!(desk.open_streams(), 2, "the phone's newest stream and the other device's");
        for (i, slot) in old.iter().enumerate() {
            assert!(ended_within(slot), "the phone's stream {i} was replaced but never told to end");
        }
        assert!(!ended_within(&newest), "the newest stream was ended");
        assert!(!ended_within(&other), "another device's stream was ended by this phone's");

        // A replaced slot dropping late returns nobody else's slot.
        old.clear();
        assert_eq!(desk.open_streams(), 2, "a replaced stream's late drop uncounted a live one");

        // The flood bound holds across devices: at MAX_STREAMS distinct devices the next is refused.
        let mut more = Vec::new();
        for i in 0..MAX_STREAMS - 2 {
            more.push(desk.claim_stream(&format!("dev_more_{i}")).unwrap());
        }
        assert_eq!(desk.claim_stream("dev_one_too_many").err(), Some(Refusal::TooManyStreams));
        assert!(!ended_within(&other), "a refused open ended somebody's stream");
        drop((newest, other, more));
        assert_eq!(desk.open_streams(), 0);
    }

    // --- the key forms -------------------------------------------------------------------------

    #[test]
    fn the_jwk_the_phone_sends_becomes_the_point_ring_verifies_with() {
        // The end-to-end claim: a key that arrives as a JWK verifies a signature made by the
        // key that produced the JWK. Anything less would be a pairing that succeeds and then
        // refuses every request afterwards.
        let phone = Phone::new();
        assert_eq!(PublicKeyForm::Jwk(phone.jwk()).to_point().unwrap(), phone.point);

        let mut spki = P256_SPKI_PREFIX.to_vec();
        spki.extend_from_slice(&phone.point);
        assert_eq!(
            PublicKeyForm::Bytes(super::super::b64url(&spki)).to_point().unwrap(),
            phone.point
        );
        assert_eq!(
            PublicKeyForm::Bytes(super::super::b64url(&phone.point)).to_point().unwrap(),
            phone.point
        );
    }

    #[test]
    fn a_key_that_is_not_a_p256_public_key_is_refused_by_name() {
        let phone = Phone::new();
        // Wrong curve, wrong type, missing coordinate, short coordinate.
        for bad in [
            serde_json::json!({"kty":"EC","crv":"P-384","x":"AA","y":"AA"}),
            serde_json::json!({"kty":"RSA","n":"AA","e":"AQAB"}),
            serde_json::json!({"kty":"EC","crv":"P-256","y": super::super::b64url(&phone.point[33..65])}),
            serde_json::json!({"kty":"EC","crv":"P-256","x":"AAAA","y":"AAAA"}),
        ] {
            assert!(PublicKeyForm::Jwk(bad.clone()).to_point().is_err(), "{bad} was accepted");
        }
        assert!(PublicKeyForm::Bytes(super::super::b64url(&[0x02; 33])).to_point().is_err());
        assert!(PublicKeyForm::Bytes(super::super::b64url(&phone.point[..64])).to_point().is_err());
    }

    // --- the small memories ---------------------------------------------------------------------

    #[test]
    fn an_audio_id_the_mac_did_not_mint_resolves_to_nothing() {
        let (dir, desk, _phone, _device, _c) = paired("audio");
        desk.mint_audio("msg_1", dir.0.join("reply.wav"));
        assert_eq!(desk.audio_file("msg_1"), Some(dir.0.join("reply.wav")));
        assert_eq!(desk.audio_file("msg_2"), None);
        assert_eq!(desk.audio_file("../../etc/passwd"), None);
        assert_eq!(desk.audio_file(""), None);
    }

    #[test]
    fn a_damaged_device_record_means_unpaired_rather_than_some_other_phone() {
        let dir = TempDir::new("damaged");
        std::fs::create_dir_all(dir.0.join("phone")).unwrap();
        std::fs::write(dir.0.join("phone/device.json"), "{ this is not json").unwrap();
        let desk = DeviceDesk::open(&dir.0).unwrap();
        assert!(!desk.is_paired(), "a damaged record was read as a paired phone");
        assert!(dir.0.join("phone/device.json").exists(), "the bytes were deleted");
    }

    #[test]
    fn forgetting_the_phone_removes_the_record_from_disk() {
        let (dir, desk, _phone, _device, _c) = paired("forget");
        assert!(dir.0.join("phone/device.json").exists());
        desk.forget().unwrap();
        assert!(!dir.0.join("phone/device.json").exists());
        assert!(!DeviceDesk::open(&dir.0).unwrap().is_paired());
    }

    // --- "They do not match", across a relaunch --------------------------------------------

    /// **THE WHOLE OF THE REJECTION, AS A SECOND LAUNCH SEES IT.**
    ///
    /// Both halves in one test, because they are one promise to the person who pressed the
    /// button: the phone he refused is still refused, AND the Mac still says it heard him.
    ///
    /// The teardown is reproduced here rather than called: `stop_because_the_phone_rejected_
    /// the_words` needs a `tauri::AppHandle`, a Keychain and a tailnet. Its two durable acts
    /// are the two lines below, in its own order — the device record goes (the route does this
    /// synchronously before it rings), then the refusal is written down.
    ///
    /// EVERY ASSERTION HAS ITS POSITIVE CONTROL IN THE SAME TEST: before the rejection the same
    /// directory answers `listener_should_run() == true` and `rejection_recorded() == false`, so
    /// a version of this that passed because the path was wrong would fail on the first line.
    #[test]
    fn a_rejected_phone_is_still_refused_after_a_relaunch_and_the_notice_goes_with_it() {
        let (dir, desk, phone, device, _c) = paired("rejected");

        // POSITIVE CONTROL: a paired Mac serves, and remembers no refusal.
        assert!(desk.listener_should_run(), "a paired Mac was not going to serve at all");
        assert!(!rejection_recorded(&dir.0), "a refusal was recorded before anything was refused");

        // The rejection, in the order the product does it.
        desk.forget().unwrap();
        record_rejection(&dir.0, 1_758_000_000_000, StoppedBy::PHONE).unwrap();

        // --- THE RELAUNCH. Nothing of the first process survives but the directory. ---------
        let after = DeviceDesk::open(&dir.0).unwrap();
        assert!(
            !after.listener_should_run(),
            "the next launch would have started a socket for a phone that was refused"
        );
        assert!(!after.is_paired(), "the refused phone came back paired");
        assert!(
            rejection_recorded(&dir.0),
            "the Mac forgot that a person stood at it and said the words did not match"
        );

        // AND THE REFUSED PHONE'S CREDENTIAL IS STILL NO CREDENTIAL. The socket is the real
        // answer — there is none — but if anything ever reaches this desk with the old id, it
        // is refused rather than served.
        let refused = after.verify(&present(
            &phone,
            &device.id,
            "a-challenge-from-the-last-life",
            "GET",
            "/api/events",
            b"",
        ));
        assert!(refused.is_err(), "the refused phone was verified by the next launch");

        // AND ASKING TO PAIR IS WHAT CLEARS IT — the one act, `PhoneRuntime::begin_pairing`.
        clear_rejection(&dir.0);
        assert!(!rejection_recorded(&dir.0), "the notice outlived the press that clears it");
    }

    /// The opposite default from `a_damaged_device_record_means_unpaired_rather_than_some_other
    /// _phone`, and it is deliberate: a record that cannot be read must never be guessed into a
    /// Mac that FORGOT a refusal.
    #[test]
    fn a_damaged_rejection_record_still_counts_as_a_refusal() {
        let dir = TempDir::new("damaged-rejection");
        std::fs::create_dir_all(dir.0.join("phone")).unwrap();
        std::fs::write(dir.0.join("phone/rejected.json"), "{ this is not json").unwrap();
        assert!(rejection_recorded(&dir.0));
        assert_eq!(rejection_by(&dir.0), StoppedBy::UNKNOWN);
    }

    /// The sheet says who stopped it, and the answer survives a relaunch like the refusal does.
    #[test]
    fn who_refused_the_words_is_written_down_with_the_refusal() {
        for by in [StoppedBy::PHONE, StoppedBy::MAC] {
            let dir = TempDir::new(&format!("rejected-by-{by}"));
            record_rejection(&dir.0, 1, by).unwrap();
            assert!(rejection_recorded(&dir.0));
            assert_eq!(rejection_by(&dir.0), by);
        }
        // A record from the build before the reason existed still refuses, and reads as unknown.
        let dir = TempDir::new("rejected-by-old");
        std::fs::create_dir_all(dir.0.join("phone")).unwrap();
        std::fs::write(dir.0.join("phone/rejected.json"), "{\"rejected_at\":1}\n").unwrap();
        assert!(rejection_recorded(&dir.0));
        assert_eq!(rejection_by(&dir.0), StoppedBy::UNKNOWN);
    }

    #[test]
    fn a_device_name_is_something_a_label_can_hold() {
        assert_eq!(sanitize_name("iPhone"), "iPhone");
        assert_eq!(sanitize_name("  iPhone  "), "iPhone");
        assert_eq!(sanitize_name(""), "Phone");
        assert_eq!(sanitize_name("a\nb\u{0}c"), "abc");
        assert_eq!(sanitize_name(&"x".repeat(200)).len(), 40);
    }
}
