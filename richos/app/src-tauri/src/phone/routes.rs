//! **THE FOUR ROUTES, AND THE FLAT 404 EVERYTHING ELSE GETS.** Plan §2.5 item 5, and the wire
//! contract `docs/architecture/phone-channel.md`.
//!
//! **RECONCILED 2026-09-18 with the phone client.** Every route name, header, field name and
//! status code below comes from the reference client module `richos/web/web-app/lib/api.js`,
//! which shipped first (`dfa7ed27`) and which the conformance corpus both native apps pass is
//! generated from (`richos/mobile/conformance`). This Rust was not yet reachable from anywhere
//! then, so this is the side that moved.
//!
//! # Why this file has no socket in it
//!
//! [`dispatch`] is a pure function from a described request to a described response. The TLS
//! acceptor, the accept loop and the stream write loop are all in [`super::listen`], which calls
//! this. That split is what makes the route table testable: the whole path space — including every
//! way a caller can be refused — is exercised in this module's own tests with no listener, no
//! certificate and no port.
//!
//! # The surface, in full
//!
//! | | Route | Credential |
//! |---|---|---|
//! | 1 | `POST /api/messages` | `Authorization: RichOS-Device …` |
//! | 2 | `GET /api/events` — the stream, or the backfill when `before=` is present | the same, in the query as `auth=` |
//! | 3 | `GET /api/audio/<message_id>` | `Authorization: RichOS-Device …` |
//! | 4 | `POST /api/pair` — pairing with a code, or the device record when authenticated | the code, or the signature |
//! | — | `GET /` and `GET /sw.js` | none — see below |
//!
//! **THE PHONE WEB PAGE IS RETIRED** (CEO, 2026-10-30: *"yes, the phone web page needs to be
//! retired now"*). This port used to serve a web app to the phone's browser. The native RichConnect
//! apps replaced it, and they use only the four routes above. What is left without a credential is
//! two fixed answers for a browser that still opens this address — the pairing link in the QR code
//! is `<origin>/#pair=<code>`, which a phone camera opens in a browser — see [`RETIRED_PAGE`] and
//! [`RETIRE_WORKER`]. Neither carries anything from the conversation.

use super::api_base::ApiBaseDesk;
use super::device::{parse_authorization, platform_of_name, DeviceDesk, Platform, Presented, Refusal};
use super::push::Subscription;
use super::rows::rows_from_payload;
use super::stream::{Frame, PhoneHub, Replay};
use super::MAX_BODY_BYTES;
use serde_json::{json, Value};
#[cfg(test)]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// One request, as the listener read it off the wire.
pub struct Incoming {
    pub method: String,
    /// The path, percent-decoded, with no query.
    pub path: String,
    /// The raw query string, without the `?`.
    pub query: String,
    pub authorization: Option<String>,
    /// `Last-Event-ID`, which the browser sets by itself on an `EventSource` reconnection. Used
    /// only when the URL carries no `since=`.
    pub last_event_id: Option<String>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// What the listener should write back.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Json { status: u16, body: String },
    Bytes { status: u16, content_type: String, body: Vec<u8>, download_as: Option<String> },
    /// Open an event stream. The listener writes these frames, then follows the hub.
    ///
    /// `mark` is the hub's position the opening frames were prepared at; the stream subscribes
    /// from there, so nothing published between preparing the opening and subscribing is lost
    /// ([`PhoneHub::subscribe_from`]).
    ///
    /// `device` is the verified device id the stream is for: its newer stream replaces its older
    /// ones ([`super::device::DeviceDesk::claim_stream`]).
    Stream { opening: Vec<String>, since: Option<u64>, mark: u64, device: String },
    /// **The answer to nearly everything.** One answer to every question a caller that is not the
    /// paired phone can ask, so the surface cannot be mapped by the shape of the refusals.
    NotFound,
    /// **The one deliberate exception**, and it is about him rather than about security: a phone
    /// this Mac has forgotten is told so, once, and stops. A 404 there would be a phone that
    /// retries forever and a CEO who is never told why it went quiet.
    Revoked,
    /// **Authenticated, and waiting for a person to press "They match" on this Mac** (Sage F1).
    /// A 409 with a body, never the flat 404: every client treats a 404 after one re-sign as
    /// final, and this state ends by itself the moment he presses. The body carries no
    /// `retry: false`, so every classifier that exists today reads it as a retryable fault.
    AwaitingMac,
    PayloadTooLarge,
    RateLimited,
}

/// The awaiting answer's body — one constant, so the listener and the route tests read the same
/// bytes. `reason` is what a client shows when it has nothing better of its own to say.
pub const AWAITING_MAC_BODY: &str =
    "{\"awaiting_mac_confirmation\":true,\"reason\":\"Press They match on your Mac.\"}";

impl Outcome {
    pub fn status(&self) -> u16 {
        match self {
            Outcome::Json { status, .. } | Outcome::Bytes { status, .. } => *status,
            Outcome::Stream { .. } => 200,
            Outcome::NotFound => 404,
            Outcome::Revoked => 403,
            Outcome::AwaitingMac => 409,
            Outcome::PayloadTooLarge => 413,
            Outcome::RateLimited => 429,
        }
    }
}

/// What the phone was accepted as having said.
pub struct Accepted {
    pub message_id: String,
    pub thread_id: String,
    pub at: u64,
}

/// **The only things the routes can reach in the rest of the app.**
///
/// A trait rather than a struct holding `AppState`, for two reasons and the second is the
/// important one. It makes the route tests run with no Tauri and no spine — and it means this
/// module's reach is an enumerable list. There is no path from here to the ledger, to the raw
/// event stream or to a `Timeline`, because those are not on this trait (plan §4.2 iii).
pub trait Bridge: Send + Sync {
    fn questions_available(&self)->bool {false}
    fn answer_question(&self,_thread:&str,_answer:richos_core::questions::AnswerRequest)->Result<Value,String> {Err("Question answers are unavailable.".into())}
    fn question_shown(&self,_thread:&str,_question:&str)->Result<(),String> {Err("Question receipts are unavailable.".into())}
    fn native_notifications_available(&self)->bool {false}
    fn register_native_notifications(&self,_device:&str,_registration:Option<super::notifications::Registration>)->Result<Value,String> {Err("Native notifications are unavailable".into())}
    fn voice_available(&self) -> bool { false }
    /// Whether he keeps the Oxford comma in what Claude writes (CEO, 2026-10-19). Sent in every
    /// `hello`; `false`, his default, where the shell has not said.
    fn oxford_comma(&self) -> bool { false }
    fn transcribe(&self, _bytes: &[u8]) -> Result<String, String> { Err("Speech recognition is unavailable".into()) }
    fn reply_audio(&self, _thread: Option<&str>, _id: &str) -> Result<Vec<u8>, String> { Err("Audio playback is unavailable".into()) }
    /// Whether this Mac sends the files Rich attached (`files` in the capabilities).
    fn files_available(&self) -> bool { false }
    /// The bytes of a file Rich attached in this conversation, by its attachment id
    /// (`richos_core::rich_files::read_for_phone`): the file's name and its bytes.
    fn rich_file(&self, _thread: &str, _id: &str) -> Result<(String, Vec<u8>), richos_core::rich_files::Fetch> {
        Err(richos_core::rich_files::Fetch::NotAttached)
    }

    /// **The turn an accepted message became, by the id `submit_text` answered with — an id and
    /// nothing else.** The one narrow reach past the gated payload, and deliberately so: it
    /// carries no text, no event and no row, only the name of a turn whose CEO row the phone
    /// already receives through the gate. Used to join a voice note's length to its row
    /// ([`super::voice_notes`]). `None` when the turn does not exist yet or cannot be read now.
    fn turn_for_intake(&self, _message_id: &str) -> Option<String> { None }

    /// Write the CEO's words to the durable intake log, `fsync`, and hand them to the spine.
    /// Returns as soon as the bytes are on disk — never after the turn.
    ///
    /// **`client_id` is the phone's own id for the message**, the one it sent with it, and it
    /// is a parameter rather than something a route may leave out: it goes onto the turn, so
    /// the CEO row the Mac sends back names the message the phone is already showing, even
    /// when that row beats the receipt (adoption ledger §2.8 row C5; `rows::client_id_of`).
    fn submit_text(&self, thread_id: Option<&str>, text: &str, client_id: &str) -> Result<Accepted, String>;
    fn submit_voice(&self, thread_id: Option<&str>, text: &str, client_id: &str) -> Result<Accepted, String> {self.submit_text(thread_id,text,client_id)}

    /// The CEO-gated timeline payload for one thread. **Obtained through `view(ViewMode::Ceo)`
    /// and no other way** — `Timeline` does not implement `Serialize`, so this signature cannot
    /// carry an ungated one.
    fn snapshot(&self, thread_id: Option<&str>) -> Result<Value, String>;

    /// The thread the phone should be looking at, and what it is called.
    fn current_thread(&self) -> Option<(String, String)>;

    /// Every thread, for the phone's picker. `(id, title)`.
    fn threads(&self) -> Vec<(String, String)>;
}

/// **THE HANDLE A REQUEST HANDLER DOES NOT OTHERWISE HAVE — the one that ends the channel it is
/// running on.**
///
/// Ray's nightly `.8` walk in the test VM, defect 1 (HIGH),
/// `docs/verification/2026-09-20-nightly-1.2.0-nightly.20260919.8-phone-path-in-the-vm-audit.md`:
/// pressing `They do not match` on the phone dropped the device record — the security-critical
/// half, and it worked — while **the Mac kept answering on 8443 at t+10, 20, 30, 40, 50 and 60 s
/// and two minutes later**, with the pairing card still on screen as though nothing had been
/// said. The Mac-side `Forget this phone` ([`super::PhoneRuntime::forget`]) stops the listener,
/// takes the hub off live and deletes the authority; the rejection path did only the device
/// record, because a route has `channel.devices` and no handle on the listener at all. That
/// missing handle is this type.
///
/// # It CANNOT do the teardown itself, and that is arithmetic rather than taste
///
/// [`super::listen::Listener::stop`] ends with `thread.join()` on the `richos-phone-channel`
/// thread, and **every route in this file runs on that thread** — inside the
/// `runtime.block_on(serve_all(..))` that [`super::listen::Listener::start`] spawns. A handler
/// that called `stop()` would join its own thread and hang there forever, holding the response
/// it was about to write. So this is a doorbell and nothing else: it hands one `()` to whoever
/// owns the listener and returns immediately. The owner does the stopping, from its own thread,
/// with its own single copy of the sequence — the same rule the reconnect work already follows
/// (`t3code-mobile-vs-richos-phone-2026-09-18` §1 item 6: the connection owner owns its own
/// teardown).
///
/// # It fires once
///
/// The sender is taken out on the first pull, so a phone that posts the rejection twice — a
/// retry, a double tap — rings the bell once. The second pull returns `false` and does nothing.
///
/// # It says why
///
/// Since Sage's pairing review it carries WHO stopped the pairing ([`super::device::StoppedBy`]):
/// the phone's `They do not match`, or the spent-code alarm, which fires on this thread too. The
/// owner's teardown is one sequence either way; only the sentence the sheet reads differs.
pub struct StopSwitch(Mutex<Option<std::sync::mpsc::Sender<&'static str>>>);

impl StopSwitch {
    /// Wired to an owner. [`super::PhoneRuntime::start`] holds the other end.
    pub fn to(owner: std::sync::mpsc::Sender<&'static str>) -> Self {
        StopSwitch(Mutex::new(Some(owner)))
    }

    /// **Nothing owns this channel**, which is true of every route-table fixture in this file:
    /// there is no listener to stop and no runtime to tell. A pull is a no-op and says so.
    ///
    /// `cfg(test)` because it has no production caller and should not acquire one: a shipped
    /// channel with no owner is the defect this type exists to close, and a constructor for it
    /// sitting in the release binary is an invitation to build one by accident.
    #[cfg(test)]
    pub fn unwired() -> Self {
        StopSwitch(Mutex::new(None))
    }

    /// Ring it. `true` if an owner was there to hear it.
    ///
    /// **Never blocks and never waits for the teardown to finish.** The response to the phone
    /// goes out on this thread; the socket it goes out on is about to be closed by the owner.
    /// That race is harmless and it is checked rather than assumed: the phone ignores the
    /// outcome of this request entirely — `richos/web/web-app/app.js:287`,
    /// `try { await api.confirmFingerprint(false); } catch { /* said locally either way */ }` —
    /// because it has already thrown its own key away on the same press.
    pub fn pull(&self, why: &'static str) -> bool {
        match self.0.lock().unwrap().take() {
            Some(owner) => owner.send(why).is_ok(),
            None => false,
        }
    }
}

/// Everything a dispatch needs, assembled once when the listener starts.
pub struct Channel {
    pub devices: Arc<DeviceDesk>,
    /// **The way out of this channel, for the one request that has to end it** — see
    /// [`StopSwitch`]. `StopSwitch::unwired()` in the route tests, which have no listener.
    pub rejected: StopSwitch,
    pub api_base: Arc<ApiBaseDesk>,
    pub hub: Arc<PhoneHub>,
    pub bridge: Arc<dyn Bridge>,
    pub vapid_public: String,
    /// The certificate authority's SHA-256 as colon-separated hex. **The Mac sends the hash and
    /// the phone renders the six words itself** (`web/web-app/lib/fingerprint.js`): *"if the Mac
    /// sent pretty words, a Mac that wanted to could send words that do not belong to the
    /// certificate it is actually serving."*
    pub fingerprint_hex: String,
    /// **WHICH PATH THE PAIRING CODE IS GOING OUT UNDER RIGHT NOW** —
    /// [`super::device::PairedVia::TAILNET`] or `HOME`. Written into the device record at
    /// pairing (see [`super::device::Device::paired_via`]) and never consulted again.
    ///
    /// **It is the DECISION, not the origin string.** `serving_plan` in `phone/mod.rs` already
    /// answers "did the tailnet branch happen" with a `Some`/`None`, so carrying that answer
    /// costs nothing and needs no `.ts.net` suffix test to read it back.
    ///
    /// **A `Mutex` for a value that no longer moves, and that is worth saying plainly.**
    /// `PhoneRuntime::start` used to construct the channel, try to bind, and fall back to the
    /// home addresses when the tailnet ones would not — a real move from `tailnet` to `home`
    /// after construction. CEO §61 removed the fallback along with the path it fell back to, so
    /// today this is written once at construction and never again. It is left as a `Mutex`
    /// rather than plumbed as a plain field because `Channel` is shared behind an `Arc` and the
    /// interior mutability is what every reader already expects; making it immutable is a
    /// refactor of every call site for no behavior.
    pub pairing_path: Mutex<&'static str>,
}

/// The most bytes the listener will read for this request, decided from the request line and
/// headers BEFORE the body is read. `kind=attachment` in the query is one file upload
/// ([`super::attachments`]); `audio/wav` is a voice note; everything else is JSON.
pub fn body_limit(method: &str, path: &str, query: &str, content_type: Option<&str>) -> usize {
    if method != "POST" || path != "/api/messages" { return MAX_BODY_BYTES; }
    if query_value(query, "kind") == Some("attachment") { return super::attachments::MAX_FILE_BYTES; }
    if content_type == Some("audio/wav") { super::voice::MAX_UPLOAD } else { MAX_BODY_BYTES }
}

/// How long the listener waits for that body. JSON gets 15 s; a voice note 120 s (60 MB at
/// 4 Mbit/s); a file 300 s — the arithmetic is at [`super::attachments::UPLOAD_SECONDS`].
pub fn upload_seconds(method: &str, path: &str, query: &str, content_type: Option<&str>) -> u64 {
    if method == "POST" && path == "/api/messages" && query_value(query, "kind") == Some("attachment") {
        return super::attachments::UPLOAD_SECONDS;
    }
    if body_limit(method, path, query, content_type) > MAX_BODY_BYTES { 120 } else { 15 }
}

/// **The whole route table.** One match, one fall-through, and the fall-through is a 404.
pub fn dispatch(channel: &Channel, request: &Incoming) -> Outcome {
    if request.body.len() > body_limit(&request.method, &request.path, &request.query, request.content_type.as_deref()) {
        return Outcome::PayloadTooLarge;
    }
    match (request.method.as_str(), request.path.as_str()) {
        ("POST", "/api/pair") => pair_or_device_record(channel, request),
        ("POST", "/api/messages") => messages(channel, request),
        // A file Rich attached, on the message route as voice moved onto it: no new route.
        ("GET", "/api/messages") => rich_file(channel, request),
        ("GET", "/api/events") => events(channel, request),
        ("GET", path) if path.starts_with("/api/audio/") => audio(channel, request, &path[11..]),
        ("GET", path) => retired_page(path),
        _ => Outcome::NotFound,
    }
}

// THE TRUST ENDPOINT IS GONE — CEO §61, 2026-09-19. Plan §2.1 item 4 gave this Mac a
// plain-HTTP route on the neighboring port serving exactly one file, an Apple
// `.mobileconfig`, so that a phone on the same network could be made to trust a certificate
// this Mac had signed for itself. §61 rules that a phone app inside the home network is
// *"utterly useless"* and that the Tailscale path is the product; on that path the
// certificate is publicly trusted and nothing is installed on the phone at all. The route,
// its port and the profile it served went together — see `Listener::start` and `ca.rs`.

// -------------------------------------------------------------------------------------
// The credential
// -------------------------------------------------------------------------------------

/// Verify the request, or say why not. `path_with_query` is what the signature covers — for the
/// stream that is the URL **without** the `auth` parameter, because the parameter is the signature.
fn verified(channel: &Channel, request: &Incoming, header: &str, path_with_query: &str) -> Result<(), Outcome> {
    verified_device(channel, request, header, path_with_query, false).map(|_| ())
}

/// [`verified`], returning the device. `answering_the_words` is true for exactly one request —
/// the phone's own answer to the six words — which a device nobody has confirmed at this Mac may
/// still make ([`DeviceDesk::verify_for_confirmation`], Sage §3.1 step 5).
fn verified_device(
    channel: &Channel,
    request: &Incoming,
    header: &str,
    path_with_query: &str,
    answering_the_words: bool,
) -> Result<super::device::Device, Outcome> {
    let Some((device_id, challenge, signature)) = parse_authorization(header) else {
        return Err(Outcome::NotFound);
    };
    let presented = Presented {
        method: &request.method,
        path_with_query,
        device_id: &device_id,
        challenge: &challenge,
        signature,
        body: &request.body,
    };
    let verdict = if answering_the_words {
        channel.devices.verify_for_confirmation(&presented)
    } else {
        channel.devices.verify(&presented)
    };
    match verdict {
        Ok(device) => Ok(device),
        Err(Refusal::RateLimited) => Err(Outcome::RateLimited),
        Err(Refusal::Revoked) => Err(Outcome::Revoked),
        Err(Refusal::AwaitingMacConfirmation) => Err(Outcome::AwaitingMac),
        Err(refusal) => {
            log_refusal(&refusal);
            Err(Outcome::NotFound)
        }
    }
}

/// **Is this body the phone's answer to the six words, and nothing more?** A `fingerprint_confirmed`
/// boolean, optionally the `device_id` it names and the `push_transport` a native app sends with
/// it, and NO other key. Anything that asks for more — a push subscription, a registration, a
/// cursor — is not an answer and is gated like every other request (Sage F1): an unconfirmed key
/// must not be able to carry an action in beside the one request it is allowed.
fn only_answers_the_words(body: &Value) -> bool {
    let Some(map) = body.as_object() else { return false };
    map.get("fingerprint_confirmed").is_some_and(Value::is_boolean)
        && map.keys().all(|k| ["fingerprint_confirmed", "device_id", "push_transport"].contains(&k.as_str()))
}

// -------------------------------------------------------------------------------------
// 4. POST /api/pair — pairing with a code, or the device record when authenticated
// -------------------------------------------------------------------------------------

fn pair_or_device_record(channel: &Channel, request: &Incoming) -> Outcome {
    let Ok(body) = serde_json::from_slice::<Value>(&request.body) else { return Outcome::NotFound };

    // TWO REQUESTS ON ONE ROUTE, told apart by the presence of a code and never by a flag. The
    // phone's own `registerPush` posts here with no `code` and an `Authorization` header; pairing
    // posts a `code` and no header. A fifth route would have broken §2.5's ceiling of four.
    if let Some(code) = body.get("code").and_then(|v| v.as_str()) {
        return complete_pairing(channel, &body, code);
    }

    let Some(header) = request.authorization.as_deref() else { return Outcome::NotFound };
    let answering = only_answers_the_words(&body);
    let device = match verified_device(channel, request, header, &signed_path(&request.path, &request.query), answering) {
        Ok(device) => device,
        Err(refusal) => return refusal,
    };

    if let Some(value)=body.get("native_push") {
        if !channel.bridge.native_notifications_available() {return Outcome::Json {status:422,body:json!({"reason":"unsupported","retryable":false}).to_string()}}
        let Ok(registration)=serde_json::from_value::<Option<super::notifications::Registration>>(value.clone()) else {return Outcome::NotFound};
        if registration.as_ref().is_some_and(|r|!r.validate()) {return Outcome::NotFound}
        let Some((device_id,_,_))=parse_authorization(header) else {return Outcome::NotFound};
        return match channel.bridge.register_native_notifications(&device_id,registration) {
            Ok(answer)=>Outcome::Json {status:200,body:answer.to_string()},
            Err(_)=>Outcome::Json {status:503,body:json!({"reason":"unreachable","retryable":true,"message":super::notifications::unavailable()}).to_string()},
        };
    }

    if let Some(push) = body.get("push") {
        let Ok(subscription) = serde_json::from_value::<Option<Subscription>>(push.clone()) else {
            return Outcome::NotFound;
        };
        if subscription.as_ref().is_some_and(|s| !s.is_supported()) {
            // Plan §2.6 at the edge: what cannot be stored can never be dialed.
            eprintln!("[richos] an unsupported web push subscription was refused at the edge");
            return Outcome::NotFound;
        }
        if let Err(e) = channel.devices.set_push(subscription) {
            eprintln!("[richos] could not record the push subscription: {e}");
            return Outcome::NotFound;
        }
    }
    if body.get("reply_receipts").and_then(Value::as_bool) == Some(true) || body.get("seen_reply").is_some() {
        let seen = match body.get("seen_reply") {
            None => None,
            Some(value) => {
                let Some(thread) = value.get("thread").and_then(Value::as_str).filter(|s| !s.is_empty() && s.len() <= 256) else { return Outcome::NotFound };
                let Some(id) = value.get("id").and_then(Value::as_str).filter(|s| !s.is_empty() && s.len() <= 256) else { return Outcome::NotFound };
                Some((thread, id))
            }
        };
        if let Err(error) = channel.devices.record_reply_receipt(seen) {
            eprintln!("[richos] could not record the phone reply receipt: {error}");
            return Outcome::NotFound;
        }
    }
    // **A cursor is only meaningful inside the conversation that numbered it**, so it is recorded
    // per conversation (`thread_id` beside it, hunt 2026-09-29 part 1 finding 14). A bare cursor
    // says nothing about which conversation was read and is not recorded.
    if let (Some(cursor), Some(thread)) = (
        body.get("delivered_cursor").and_then(|v| v.as_u64()),
        body.get("thread_id").and_then(Value::as_str).filter(|s| !s.is_empty() && s.len() <= 256),
    ) {
        let _ = channel.devices.set_delivered_cursor(thread, cursor);
    }

    // **THE SIX WORDS, ANSWERED** — Ray's nightly `.7` walk, defect 2, and the half of it that
    // was missing from the protocol rather than from the copy.
    //
    // The Mac's sheet said `It is paired` while the phone was still asking the person whether
    // the words matched, and it had no choice: the phone told it nothing either way. `true` is
    // the person pressing `They match — pair this phone`; `false` is `They do not match`, and
    // that one FORGETS the phone rather than recording a flag. A person who has just said the
    // words are wrong has said that something other than his Mac may be on the other end of
    // this credential, and leaving it paired "in case he was mistaken" is the one outcome worse
    // than stopping. The phone throws its own key away on the same press, so both sides end in
    // the same state.
    //
    // **Strictly a boolean, like every other reading on this channel.** A missing key is not a
    // confirmation, and neither is the string "true".
    if let Some(confirmed) = body.get("fingerprint_confirmed").and_then(|v| v.as_bool()) {
        if confirmed {
            if let Err(e) = channel.devices.confirm_fingerprint() {
                eprintln!("[richos] could not record the phone's fingerprint confirmation: {e}");
                return Outcome::NotFound;
            }
            // A native app says which native service reaches it: `apns` (the preserved iPhone app,
            // unchanged) or `fcm` (the native Android app). Any other value is ignored, as before.
            if let Some(transport)=body.get("push_transport").and_then(|v|v.as_str()).filter(|t|["apns","fcm"].contains(t)) {
                let Some((id,_,_))=parse_authorization(header) else {return Outcome::NotFound};
                if channel.devices.use_native_transport(&id,transport).is_err() {return Outcome::NotFound}
            }
            // AND IF NOBODY AT THIS MAC HAS ANSWERED YET, THE PHONE IS TOLD SO (Sage §3.1 step 5):
            // its screen moves to "Now press They match on your Mac". Additive: a client that
            // reads only `ok` — the preserved iPhone app — is unaffected. This is also the one
            // answer the listener may HOLD for a phone that asked it to (`pair-wait`).
            if !device.mac_confirmed {
                return awaiting_answer();
            }
        } else {
            eprintln!("[richos] the phone reported that the six words did NOT match; forgetting it");
            // **THE CREDENTIAL GOES HERE, SYNCHRONOUSLY, AND IT STAYS HERE.** The owner's
            // teardown calls this again a moment later and `DeviceDesk::forget` is idempotent
            // (`device.rs`: `take()`, then a `remove_file` whose failure is ignored), so this
            // line is not the second copy of anything. It is the one part of the sequence that
            // must not depend on a thread being alive to hear a doorbell: if the watcher failed
            // to spawn, this Mac still has no device record.
            if let Err(e) = channel.devices.forget() {
                eprintln!("[richos] could not forget the phone after a rejected fingerprint: {e}");
                return Outcome::NotFound;
            }
            // **AND THE REST OF THE SEQUENCE IS THE OWNER'S** — Ray's nightly `.8` defect 1. The
            // socket, the hub and the certificate authority all belong to `PhoneRuntime`, which
            // is the only thing that may put them down and the only place that sequence is
            // written. See [`StopSwitch`] for why a route cannot do it on this thread.
            if !channel.rejected.pull(super::device::StoppedBy::PHONE) {
                eprintln!(
                    "[richos] nothing owns this channel, so it keeps serving after a rejected \
                     fingerprint - the device record is gone either way"
                );
            }
        }
    }
    Outcome::Json { status: 200, body: json!({ "ok": true }).to_string() }
}

/// **"Your answer is in, and this Mac is still waiting for its own press"** — the 200 the phone's
/// `They match` gets before the press on this Mac. One function, so the route and the listener
/// that may hold it ([`holdable`]) compare the same bytes.
pub fn awaiting_answer() -> Outcome {
    Outcome::Json { status: 200, body: json!({ "ok": true, "awaiting_mac_confirmation": true }).to_string() }
}

/// **MAY THE LISTENER HOLD THIS ANSWER?** — Sage's pair-v2 hypotheses review §1, "The fix" point 2.
///
/// Exactly one shape: the phone's own signed answer to the six words (`POST /api/pair`), answered
/// [`awaiting_answer`]. That answer is only ever produced after
/// [`DeviceDesk::verify_for_confirmation`] accepted the signature, so nothing without the key can
/// reach a hold, and every other request — including every refusal — is answered at once.
pub fn holdable(request: &Incoming, outcome: &Outcome) -> bool {
    request.method == "POST" && request.path == "/api/pair" && *outcome == awaiting_answer()
}

fn complete_pairing(channel: &Channel, body: &Value, code: &str) -> Outcome {
    use super::device::PublicKeyForm;
    let form = if let Some(jwk) = body.get("public_key_jwk") {
        PublicKeyForm::Jwk(jwk.clone())
    } else if let Some(bytes) = body.get("public_key").and_then(|v| v.as_str()) {
        PublicKeyForm::Bytes(bytes.to_string())
    } else {
        return Outcome::NotFound;
    };
    let name = body.get("device_name").and_then(|v| v.as_str()).unwrap_or("Phone");
    // **THE PLATFORM, PREFERRING WHAT THE PHONE SAYS OUTRIGHT OVER WHAT IT CALLS ITSELF.**
    // `web/web-app/` sends no `platform` today, so in practice this reads the name — and the
    // moment the phone page starts sending one, this Mac already honors it without a change
    // here. An unrecognized value is not silently accepted: it falls through to the name.
    let platform = match body.get("platform").and_then(|v| v.as_str()) {
        Some(Platform::IOS) => Platform::IOS,
        Some(Platform::ANDROID) => Platform::ANDROID,
        _ => platform_of_name(name),
    };
    // Which path the code being redeemed went out under. Read once, here, because this is the
    // last moment at which it is a fact rather than a recomputation (Ray's defect 3.2).
    let via = *channel.pairing_path.lock().unwrap();
    // WHICH SIX WORDS THE PHONE WILL SHOW (Sage §3, `pair-v2`). A phone that announces 2 derives
    // them over the origin it dialed and its own key; anything else is a v1 phone, kept for one
    // release. The Mac shows the matching line, so stripping this field only makes two lines differ.
    let pairing_version = if body.get("pairing_version").and_then(Value::as_u64) == Some(2) { 2 } else { 1 };
    let device = match channel.devices.complete_pairing_announcing(code, &form, name, via, platform, pairing_version) {
        Ok(d) => d,
        // THE SPENT-CODE ALARM (Sage F1 item 4). The desk has already forgotten the unconfirmed
        // phone that used the code first; the rest of the teardown — the socket, the authority
        // and the sentence on the Mac — is the owner's, exactly as for `They do not match`. The
        // caller learns nothing: the same flat 404 as any other refused code.
        Err(super::device::Refusal::CodeUsedTwice) => {
            eprintln!("[richos] the pairing code was used by a second device; forgetting the unconfirmed phone");
            if !channel.rejected.pull(super::device::StoppedBy::CODE_USED_TWICE) {
                eprintln!("[richos] nothing owns this channel, so it keeps serving after the spent-code alarm - the device record is gone either way");
            }
            return Outcome::NotFound;
        }
        Err(refusal) => {
            log_refusal(&refusal);
            return Outcome::NotFound;
        }
    };
    let Ok(challenge) = channel.devices.issue_challenge() else { return Outcome::NotFound };
    let offer = channel.api_base.current();
    channel.api_base.mark_told(offer.clone());
    let (thread_id, thread_title) = channel.bridge.current_thread().unwrap_or_default();
    let threads: Vec<Value> = channel
        .bridge
        .threads()
        .into_iter()
        .map(|(id, title)| json!({ "id": id, "title": title }))
        .collect();
    Outcome::Json {
        status: 200,
        body: json!({
            "device_id": device.id,
            "ca_fingerprint_sha256": channel.fingerprint_hex,
            "vapid_public_key": channel.vapid_public,
            "challenge": challenge,
            "api_base": offer.as_ref().map(|o| o.api_base.clone()),
            "thread_id": thread_id,
            "thread_title": thread_title,
            "threads": threads,
            // Additive: a freshly paired native app learns what this Mac supports before its
            // first stream. The `hello` repeats all three and is the one clients replace from.
            "protocol_version": PROTOCOL_VERSION,
            "capabilities": channel_capabilities(channel),
            // Sage §3.1 step 3: "The answer is unchanged, plus pairing_version: 2" — what this Mac
            // derives. `pair-v2` in `capabilities` is the name a v2 phone requires (§3.5).
            "pairing_version": 2,
            // How long the person has to press "They match" on this Mac before the unconfirmed
            // key is forgotten (§3.1 step 6) — the bound on the phone's wait for that press, so
            // the phone never waits on a pairing this Mac has already dropped.
            "confirm_within_seconds": device.confirm_by.saturating_sub(super::now_millis()) / 1000,
            "attachment_limits": attachment_limits(),
            "build": BUILD,
        })
        .to_string(),
    }
}

// -------------------------------------------------------------------------------------
// 1. POST /api/messages
// -------------------------------------------------------------------------------------

/// **WHAT THIS MAC CAN ACTUALLY BE ASKED FOR**, sent in every `hello` (contract §5.3, plan §2 A).
///
/// The phone shipped a "Hold to record" button as one of its two biggest controls while this
/// route answered every voice note `503`. A control that cannot work is worse than an absent one,
/// so the phone now renders a control only where this list names the capability behind it, and
/// believes nothing it was not told.
///
/// **This list and the route below it are one fact written twice, so a test holds them together.**
/// `voice_is_offered_exactly_when_the_route_would_take_one` fails the day someone builds slice B
/// and forgets to advertise it — a Mac that can take a voice note behind a phone that will not
/// show the button — and the day someone advertises it before it works, which is today's defect
/// with a longer fuse.
pub const CAPABILITIES: &[&str] = &["text"];

/// **THE WIRE VERSION, AND WHY IT IS 1 AND STAYS 1 FOR EVERYTHING ADDITIVE.**
///
/// The preserved iOS app's core already reads this field and treats any value other than `1`
/// as "this Mac needs a different app" (`mobile/core/client.js:147`,
/// `frame.protocol_version !== undefined && frame.protocol_version !== 1`). So the number the
/// Mac has always implicitly spoken is `1`, sending it changes nothing for that app, and any
/// other value would switch it off. **New features are named in [`capabilities`], never by
/// raising this;** it moves only for a change an old client cannot survive.
pub const PROTOCOL_VERSION: u64 = 1;

/// Everything this Mac can be asked for, as sent in `hello` and in the pairing answer. The
/// long-standing entries keep their order; each addition is appended, and a client that does not
/// know a name ignores it (`web/web-app/lib/api.js` `offers` is an `indexOf`).
///
/// - `attachments` — photos and files ([`super::attachments`]). Always: the route needs nothing
///   but this Mac's own disk.
/// - `native-push-fcm` — the push registration takes `{"platform":"fcm", …}` beside APNs
///   ([`super::notifications::Registration`]), whenever native push is offered at all.
fn channel_capabilities(channel:&Channel)->Vec<&'static str> {
    let mut caps=capabilities(channel.bridge.voice_available(),channel.bridge.native_notifications_available());
    if channel.bridge.questions_available(){caps.push("questions");}
    // `files`: a file Rich attached comes over on a signed `GET /api/messages?kind=file`.
    if channel.bridge.files_available(){caps.push("files");}
    caps
}

pub fn capabilities(voice: bool, native_push: bool) -> Vec<&'static str> {
    let mut caps = CAPABILITIES.to_vec();
    if voice {
        caps.extend(["voice", "audio"]);
    }
    if native_push {
        caps.push("native-push");
    }
    caps.push("attachments");
    if native_push {
        caps.push("native-push-fcm");
    }
    // Always: every build from this one derives the v2 six words and requires the press on the Mac
    // (Sage §3.5, ledger row S3). A v2 phone refuses a Mac that does not name it.
    caps.push(super::words::PAIR_V2_CAPABILITY);
    // Always: this Mac holds a phone's `They match` for up to `PAIR_WAIT_MAX_SECONDS` when the
    // phone sends `Prefer: wait=…`, and answers it the moment the press lands (Sage's pair-v2
    // hypotheses review §1; ledger row S3's capability pattern). A phone that does not see it
    // sends no `Prefer` and keeps its backed-off schedule.
    caps.push(super::device::PAIR_WAIT_CAPABILITY);
    caps
}

/// The attachment limits a client needs to refuse a file before it uploads it, sent beside the
/// capabilities so the two apps never hard-code a number this Mac owns.
fn attachment_limits() -> Value {
    use super::attachments as a;
    json!({
        "max_file_bytes": a::MAX_FILE_BYTES,
        "max_files_per_message": a::MAX_FILES_PER_MESSAGE,
        "max_message_bytes": a::MAX_MESSAGE_BYTES,
        "upload_seconds": a::UPLOAD_SECONDS,
        "media_types": a::ACCEPTED.iter().map(|k| k.media_type).collect::<Vec<_>>(),
    })
}

/// Which RichOS is answering. `CARGO_PKG_VERSION` and nothing else: `app/src-tauri/Cargo.toml` is
/// the single source the shipped version comes from, `tauri.conf.json` has never carried a
/// `version` key and `make-release.sh` refuses one outright — the chain is written out at
/// `updates.rs:141`. Taken at compile time rather than plumbed through [`Channel`], because a
/// value assembled at a call site is a value a call site can get wrong.
pub const BUILD: &str = env!("CARGO_PKG_VERSION");

fn messages(channel: &Channel, request: &Incoming) -> Outcome {
    let Some(header) = request.authorization.as_deref() else { return Outcome::NotFound };
    let path = signed_path(&request.path, &request.query);
    if let Err(refusal) = verified(channel, request, header, &path) {
        return refusal;
    }

    // A FILE, before the voice check: an attachment upload names itself in the query, and its
    // content type is the file's own, which must never be mistaken for a voice note.
    if query_value(&request.query, "kind") == Some("attachment") {
        return attachment_upload(channel, request);
    }
    if query_value(&request.query, "kind") == Some("voice") || request.content_type.as_deref().is_some_and(|v| v.starts_with("audio/")) {
        return voice_message(channel, request);
    }

    let Ok(body) = serde_json::from_slice::<Value>(&request.body) else { return Outcome::NotFound };
    let Some(client_id) = body.get("client_id").and_then(|v| v.as_str()) else {
        return Outcome::NotFound;
    };
    if client_id.is_empty() || client_id.len() > 128 { return Outcome::NotFound; }
    if matches!(body["kind"].as_str(),Some("answer"|"question_seen")) {
        let Some(thread)=body["thread_id"].as_str() else {return Outcome::NotFound;};
        if !channel.bridge.questions_available(){return Outcome::NotFound;}
        if body["kind"]=="question_seen" {
            let Some(question)=body["question_id"].as_str() else{return Outcome::NotFound;};
            return match channel.bridge.question_shown(thread,question) {
                Ok(())=>Outcome::Json{status:200,body:json!({"acknowledged":true,"message_id":client_id,"duplicate":false,"cursor":channel.hub.cursor_now()}).to_string()},
                Err(reason)=>Outcome::Json{status:422,body:json!({"retry":false,"reason":reason}).to_string()}
            };
        }
        if body.get("text").is_some_and(|v|!v.is_string()) || body.get("expected_revision").is_some_and(|v|!v.is_null() && v.as_u64().is_none()) {return Outcome::Json{status:422,body:json!({"retry":false,"reason":"Invalid answer fields"}).to_string()};}
        let Some(device)=channel.devices.paired() else{return Outcome::NotFound;};
        let request=richos_core::questions::AnswerRequest {
            question_id:body["question_id"].as_str().unwrap_or("").into(),
            client_id:super::hex(&super::sha256(format!("{}:{client_id}",device.id).as_bytes())),
            option_ids:match serde_json::from_value(body.get("option_ids").cloned().unwrap_or_else(||json!([]))){Ok(v)=>v,Err(_)=>return Outcome::NotFound},
            text:body["text"].as_str().unwrap_or("").into(),expected_revision:body["expected_revision"].as_u64()
        };
        return match channel.bridge.answer_question(thread,request) {
            Ok(mut answer)=>{answer["message_id"]=json!(client_id);answer["thread_id"]=json!(thread);answer["cursor"]=json!(channel.hub.cursor_now());answer["accepted_at"]=json!(super::rows::iso8601(super::now_millis()));Outcome::Json{status:200,body:answer.to_string()}},
            Err(reason)=>Outcome::Json{status:422,body:json!({"retry":false,"reason":reason}).to_string()}
        };
    }
    if body.get("kind").and_then(|v| v.as_str()) == Some("attachments") {
        return attachments_message(channel, request, &body, client_id);
    }
    if body.get("kind").and_then(|v| v.as_str()).unwrap_or("text") != "text" {
        return Outcome::NotFound;
    }
    let text = body.get("text").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if text.is_empty() {
        return Outcome::NotFound;
    }
    let thread_id = body.get("thread_id").and_then(|v| v.as_str());

    let Some(device) = channel.devices.paired() else { return Outcome::NotFound; };
    let delivery = channel.devices.deliveries.execute(&device.id, client_id, &request.body, || {
        let accepted = channel.bridge.submit_text(thread_id, &text, client_id)?;
        Ok(json!({ "message_id": accepted.message_id, "cursor": channel.hub.next_cursor(),
            "thread_id": accepted.thread_id, "accepted_at": super::rows::iso8601(accepted.at), "duplicate": false }).to_string())
    });
    use super::delivery::Delivery;
    match delivery {
        Ok(Delivery::Accepted(body)) => Outcome::Json { status: 200, body },
        Ok(Delivery::Duplicate(answer)) => {
            let mut value: Value = serde_json::from_str(&answer).unwrap_or(Value::Null);
            if let Some(map) = value.as_object_mut() { map.insert("duplicate".into(), json!(true)); }
            Outcome::Json { status: 200, body: value.to_string() }
        }
        Ok(Delivery::Conflict) => Outcome::Json { status: 409, body: json!({"retry":false,"reason":"That message ID was already used for different text. Your draft is still on your phone."}).to_string() },
        Ok(Delivery::Uncertain) => Outcome::Json { status: 503, body: json!({"retry":false,"reason":"Your Mac may already have this message. Check the conversation before sending it again; your text is still on your phone."}).to_string() },
        Ok(Delivery::Rejected(reason)) => Outcome::Json { status:422,body:json!({"retry":false,"reason":reason}).to_string() },
        Ok(Delivery::Full) => Outcome::Json { status: 503, body: json!({"retry":false,"reason":"Your Mac's message recovery history is full. Your text is still on your phone."}).to_string() },
        Err(_) => Outcome::Json { status: 503, body: json!({"accepted":false,"reason":"Your Mac could not save the delivery receipt. Your text is still on your phone."}).to_string() },
    }
}

fn voice_message(channel: &Channel, request: &Incoming) -> Outcome {
    let refuse = |status, message: &str| Outcome::Json { status, body: json!({"accepted":false,"retry":false,"reason":message}).to_string() };
    if !channel.bridge.voice_available() { return refuse(503,"Voice notes are not switched on yet. Your recording is still on your phone."); }
    if request.content_type.as_deref() != Some("audio/wav") || query_value(&request.query,"kind") != Some("voice") {
        return Outcome::NotFound;
    }
    let Some(client) = query_value(&request.query,"client_id").and_then(|v| percent_decode_component(v).ok()) else { return Outcome::NotFound };
    let Some(thread) = query_value(&request.query,"thread_id").and_then(|v| percent_decode_component(v).ok()) else { return Outcome::NotFound };
    if client.is_empty() || client.len()>128 || !channel.bridge.threads().iter().any(|(id,_)| *id == thread) { return Outcome::NotFound; }
    let duration_ms = match super::voice::validate(&request.body) {
        Ok(samples) => super::voice_notes::duration_ms(samples.len()),
        Err(message) => return refuse(422,&message),
    };
    let Some(device) = channel.devices.paired() else { return Outcome::NotFound };
    // Bind the receipt to both the destination and the audio. Changing threads must not
    // turn an old message ID into authority to submit a second turn.
    let mut receipt_body = thread.as_bytes().to_vec(); receipt_body.push(0); receipt_body.extend_from_slice(&request.body);
    let delivery = channel.devices.deliveries.execute_prepared(&device.id,&client,&receipt_body,|| channel.bridge.transcribe(&request.body),|text| {
        // Revocation while recognition was running must prevent a new command.
        if channel.devices.paired().is_none_or(|d| d.id != device.id) { return Err("revoked".into()); }
        let accepted = channel.bridge.submit_voice(Some(&thread),&text,&client)?;
        // The length is decoration on an accepted message: failing to remember it is logged,
        // never a reason to answer a message the Mac has already taken as anything but taken.
        if let Err(error) = channel.devices.voice_notes.record(&accepted.message_id,&accepted.thread_id,duration_ms) {
            eprintln!("[richos] a voice note's length could not be saved: {error}");
        }
        Ok(json!({"message_id":accepted.message_id,"cursor":channel.hub.next_cursor(),"thread_id":accepted.thread_id,
            "accepted_at":super::rows::iso8601(accepted.at),"text_sha256":super::hex(&super::sha256(text.as_bytes())),
            "duration_ms":duration_ms,"duplicate":false}).to_string())
    });
    use super::delivery::Delivery;
    match delivery {
        Ok(Delivery::Accepted(body)) => Outcome::Json {status:200,body},
        Ok(Delivery::Duplicate(body)) => { let mut value:Value=serde_json::from_str(&body).unwrap_or(Value::Null); value["duplicate"]=json!(true); Outcome::Json {status:200,body:value.to_string()} },
        Ok(Delivery::Rejected(reason)) => refuse(422,&reason),
        Ok(Delivery::Conflict) => refuse(409,"That recording ID already belongs to a different message. The recording remains on your phone."),
        Ok(Delivery::Uncertain) => refuse(503,"The recording could not be confirmed. Check the conversation before sending again; the recording remains on your phone."),
        _ => refuse(503,"Your Mac could not save this recording's receipt. The recording remains on your phone."),
    }
}

/// One refusal body for the attachment routes, in the shape the text route already uses:
/// `accepted:false`, whether a retry of THIS request can help, and a sentence for the person.
fn attachment_refusal(status: u16, retry: bool, reason: &str) -> Outcome {
    Outcome::Json { status, body: json!({"accepted": false, "retry": retry, "reason": reason}).to_string() }
}

/// `POST /api/messages?kind=attachment&client_id=…&attachment_id=…[&name=…]` — one file.
/// Signed like every request (the signature covers the query and the file's SHA-256), and
/// idempotent: the same bytes under the same id answer `duplicate: true`.
fn attachment_upload(channel: &Channel, request: &Incoming) -> Outcome {
    use super::attachments::{Upload, MAX_RAW_NAME_BYTES};
    let decoded = |name: &str| query_value(&request.query, name).and_then(|v| percent_decode_component(v).ok());
    let Some(client) = decoded("client_id").filter(|c| !c.is_empty() && c.len() <= 128) else { return Outcome::NotFound };
    let Some(id) = decoded("attachment_id").filter(|id| super::attachments::valid_id(id)) else { return Outcome::NotFound };
    let name = decoded("name");
    if name.as_ref().is_some_and(|n| n.len() > MAX_RAW_NAME_BYTES) { return Outcome::NotFound; }
    let Some(content_type) = request.content_type.as_deref() else { return Outcome::NotFound };
    let Some(device) = channel.devices.paired() else { return Outcome::NotFound };
    let answer = |staged: super::attachments::Staged, duplicate: bool| Outcome::Json {
        status: 200,
        body: json!({"attachment_id": staged.id, "name": staged.name, "media_type": staged.media_type,
                     "size": staged.size, "sha256": staged.sha256, "duplicate": duplicate}).to_string(),
    };
    match channel.devices.attachments.stage(&device.id, &client, &id, name.as_deref(), content_type, &request.body) {
        Ok(Upload::Stored(staged)) => answer(staged, false),
        Ok(Upload::Duplicate(staged)) => answer(staged, true),
        Ok(Upload::Conflict) => attachment_refusal(409, false, "That file ID already belongs to a different file. The file is still on your phone."),
        Ok(Upload::Refused(reason)) | Ok(Upload::Limit(reason)) => attachment_refusal(422, false, &reason),
        Err(error) => {
            eprintln!("[richos] a file from the phone could not be saved: {error}");
            attachment_refusal(503, true, "Your Mac could not save this file just now. It is still on your phone and will be sent again.")
        }
    }
}

/// `POST /api/messages` with `kind:"attachments"` — the message that commits uploaded files.
///
/// ```json
/// {"client_id":"…","thread_id":"thr_…","kind":"attachments","text":"optional words",
///  "attachments":[{"id":"…","sha256":"<64 lowercase hex>"}],"sent_at":"…"}
/// ```
///
/// Replay-safe by the same receipt as text. A file the Mac does not hold is answered `422` with
/// `missing: [ids]` and NO reservation, so the phone uploads those and sends the same bytes again.
fn attachments_message(channel: &Channel, request: &Incoming, body: &Value, client_id: &str) -> Outcome {
    use super::attachments::{describe, valid_id, MAX_FILES_PER_MESSAGE};
    let Some(thread) = body.get("thread_id").and_then(Value::as_str) else { return Outcome::NotFound };
    if !channel.bridge.threads().iter().any(|(id, _)| id == thread) { return Outcome::NotFound; }
    let text = match body.get("text") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(t)) => t.trim().to_string(),
        Some(_) => return Outcome::NotFound,
    };
    let Some(list) = body.get("attachments").and_then(Value::as_array) else { return Outcome::NotFound };
    if list.is_empty() || list.len() > MAX_FILES_PER_MESSAGE { return Outcome::NotFound; }
    let mut wanted: Vec<(String, String)> = Vec::new();
    for item in list {
        let id = item.get("id").and_then(Value::as_str).filter(|id| valid_id(id));
        let sha = item.get("sha256").and_then(Value::as_str)
            .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        let (Some(id), Some(sha)) = (id, sha) else { return Outcome::NotFound };
        if wanted.iter().any(|(seen, _)| seen == id) { return Outcome::NotFound; }
        wanted.push((id.to_string(), sha.to_string()));
    }
    let Some(device) = channel.devices.paired() else { return Outcome::NotFound };
    let desk = &channel.devices.attachments;
    let delivery = channel.devices.deliveries.execute_prepared(
        &device.id,
        client_id,
        &request.body,
        || desk.staged(&device.id, client_id, &wanted).map_err(|_| "missing".to_string()),
        |files| {
            // Revocation while the files were being checked must prevent a new command.
            if channel.devices.paired().is_none_or(|d| d.id != device.id) { return Err("revoked".into()); }
            let stored = desk.commit(&device.id, client_id, thread, &files).map_err(|e| e.to_string())?;
            let accepted = channel.bridge.submit_text(Some(thread), &describe(&text, &stored), client_id)?;
            let files: Vec<Value> = stored.iter()
                .map(|f| json!({"id": f.id, "name": f.name, "media_type": f.media_type, "size": f.size}))
                .collect();
            Ok(json!({"message_id": accepted.message_id, "cursor": channel.hub.next_cursor(), "thread_id": accepted.thread_id,
                "accepted_at": super::rows::iso8601(accepted.at), "attachments": files, "duplicate": false}).to_string())
        },
    );
    use super::delivery::Delivery;
    match delivery {
        Ok(Delivery::Accepted(body)) => Outcome::Json { status: 200, body },
        Ok(Delivery::Duplicate(answer)) => {
            let mut value: Value = serde_json::from_str(&answer).unwrap_or(Value::Null);
            if let Some(map) = value.as_object_mut() { map.insert("duplicate".into(), json!(true)); }
            Outcome::Json { status: 200, body: value.to_string() }
        }
        Ok(Delivery::Rejected(_)) => {
            let missing = match desk.staged(&device.id, client_id, &wanted) { Err(m) => m.0, Ok(_) => Vec::new() };
            Outcome::Json { status: 422, body: json!({"accepted": false, "retry": true, "missing": missing,
                "reason": "Some files have not reached your Mac yet. They are still on your phone and will be sent again."}).to_string() }
        }
        Ok(Delivery::Conflict) => attachment_refusal(409, false, "That message ID was already used for a different message. Your files are still on your phone."),
        Ok(Delivery::Uncertain) => attachment_refusal(503, false, "Your Mac may already have this message. Check the conversation before sending it again; your files are still on your phone."),
        Ok(Delivery::Full) => attachment_refusal(503, false, "Your Mac's message recovery history is full. Your files are still on your phone."),
        Err(_) => attachment_refusal(503, false, "Your Mac could not save the delivery receipt. Your files are still on your phone."),
    }
}

// -------------------------------------------------------------------------------------
// 2. GET /api/events — the stream, or the backfill
// -------------------------------------------------------------------------------------

fn events(channel: &Channel, request: &Incoming) -> Outcome {
    // The credential is in the query for this route and only this route: `EventSource` cannot set
    // a header. The signature covers the path WITHOUT the `auth` parameter.
    let Some(auth) = query_value(&request.query, "auth") else { return Outcome::NotFound };
    let Ok(decoded) = percent_decode_component(auth) else { return Outcome::NotFound };
    let path = signed_path(&request.path, &request.query);
    let device = match verified_device(channel, request, &decoded, &path, false) {
        Ok(device) => device,
        Err(refusal) => return refusal,
    };

    let thread_id = query_value(&request.query, "thread_id")
        .map(|s| s.to_string())
        .or_else(|| channel.bridge.current_thread().map(|(id, _)| id))
        .unwrap_or_default();

    // `before=` present means the backfill behind infinite scroll — ordinary JSON, no stream.
    // Chunked loading behind a scroll is explicitly fine; page numbers are never built.
    if let Some(before) = query_value(&request.query, "before").and_then(|v| v.parse::<u64>().ok()) {
        let limit = query_value(&request.query, "limit")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(40)
            .clamp(1, 200);
        let Ok(payload) = channel.bridge.snapshot(Some(&thread_id)) else { return Outcome::NotFound };
        let all = rows_from_payload(&payload);
        // **RE-SEEDED HERE TOO, AND NOT ONLY IN `hello`.** The hub takes one cursor per streamed
        // reply; the projection counts one per MESSAGE. A turn the CEO starts at the Mac moves the
        // projection by two rows and the hub by one, because no live event in this build carries a
        // CEO turn (`rows::event_from_live`) — so the live sequence falls one behind the
        // projection per desk turn, and a phone that is reading both at once sees two orderings.
        // `seed_cursor` only ever raises, so this is the same correction `hello_frame` makes, at
        // the other place the projection is counted, and it lands after every reply rather than
        // only on a reconnection.
        channel.hub.seed_cursor(all.len() as u64);
        let mut earlier: Vec<Value> =
            all.iter().filter(|r| r["cursor"].as_u64().unwrap_or(0) < before).cloned().collect();
        let start = earlier.len().saturating_sub(limit);
        let more = start > 0;
        annotate_voice_notes(channel, &mut earlier[start..]);
        return Outcome::Json {
            status: 200,
            body: json!({ "messages": earlier[start..], "more": more }).to_string(),
        };
    }

    // Otherwise: the live stream. `since=` from the URL, or `Last-Event-ID` when the browser
    // reconnected by itself and the phone did not rebuild the URL.
    let since = query_value(&request.query, "since")
        .and_then(|v| v.parse::<u64>().ok())
        .or_else(|| request.last_event_id.as_deref().and_then(|v| v.parse::<u64>().ok()));

    let mut opening: Vec<String> = Vec::new();
    // The hub's position is taken WITH the answer and before any `hello` is built from the ledger.
    let (replay, mark) = channel.hub.replay_after_marked(since);
    match replay {
        Replay::Tail(frames) => {
            for frame in frames {
                opening.push(frame.to_wire());
            }
        }
        Replay::Hello => {
            let Ok(hello) = hello_frame(channel, &thread_id) else { return Outcome::NotFound };
            opening.push(hello.to_wire());
        }
    }
    Outcome::Stream { opening, since, mark, device: device.id }
}

/// `duration_ms` on the CEO rows that were phone voice notes, joined exactly through
/// [`Bridge::turn_for_intake`] — see [`super::voice_notes`] for why the live frame does not carry it.
fn annotate_voice_notes(channel: &Channel, rows: &mut [Value]) {
    channel.devices.voice_notes.annotate(rows, &|intake| channel.bridge.turn_for_intake(intake));
}

/// The `hello` the phone opens on: the challenge, the API base, the thread list and everything
/// that is already true. Built from the gated projection and nothing else.
fn hello_frame(channel: &Channel, thread_id: &str) -> Result<Frame, ()> {
    let payload = channel.bridge.snapshot(Some(thread_id)).map_err(|e| {
        eprintln!("[richos] the phone channel could not read the thread: {e}");
    })?;
    let mut rows = rows_from_payload(&payload);
    annotate_voice_notes(channel, &mut rows);
    // Seed the live cursor from the projection, so a live row continues the conversation's own
    // numbering rather than starting a second sequence.
    channel.hub.seed_cursor(rows.len() as u64);
    let challenge = channel.devices.issue_challenge().map_err(|e| {
        eprintln!("[richos] the phone channel could not issue a challenge: {e}");
    })?;
    let offer = channel.api_base.current();
    channel.api_base.mark_told(offer.clone());
    let threads: Vec<Value> = channel
        .bridge
        .threads()
        .into_iter()
        .map(|(id, title)| json!({ "id": id, "title": title }))
        .collect();
    let data = json!({
        "challenge": challenge,
        "api_base": offer.as_ref().map(|o| o.api_base.clone()),
        "thread_id": thread_id,
        "latest_cursor": rows.len() as u64,
        "threads": threads,
        "vapid_public_key": channel.vapid_public,
        // What this Mac can be asked for, and which RichOS is answering. Sent on EVERY `hello`
        // rather than once at pairing, because the phone outlives the build it paired with: he
        // updates the Mac and the app on his phone is the same app, holding whatever it was last
        // told. The phone replaces its answer from each frame and never merges (`api.js`).
        "capabilities": channel_capabilities(channel),
        "protocol_version": PROTOCOL_VERSION,
        "attachment_limits": attachment_limits(),
        "build": BUILD,
        // Whether he keeps the Oxford comma in what Claude writes (CEO, 2026-10-19): the phone
        // draws Rich's replies and question cards through the same filter the desktop does.
        "oxford_comma": channel.bridge.oxford_comma(),
        // The rows themselves ride along, so the first paint needs no second request. The phone
        // merges them by cursor exactly as it merges a live `message`.
        "messages": rows,
    });
    Ok(Frame { cursor: channel.hub.cursor_now(), kind: "hello", data: data.to_string() })
}

// -------------------------------------------------------------------------------------
// 3. GET /api/audio/<message_id>
// -------------------------------------------------------------------------------------

fn audio(channel: &Channel, request: &Incoming, message_id: &str) -> Outcome {
    let Some(header) = request.authorization.as_deref() else { return Outcome::NotFound };
    let path = signed_path(&request.path, &request.query);
    if let Err(refusal) = verified(channel, request, header, &path) {
        return refusal;
    }
    let Ok(decoded)=percent_decode_component(message_id) else {return Outcome::NotFound};
    if decoded.is_empty() || decoded.len()>256 || !decoded.bytes().all(|b|b.is_ascii_alphanumeric() || b"_:-".contains(&b)) {return Outcome::NotFound}
    let message_id=decoded.as_str();
    // The id indexes a table the Mac wrote. There is no path here, no file name and nothing
    // derived from the request, so there is nothing to traverse.
    let Some(file) = channel.devices.audio_file(message_id) else {
        let thread = query_value(&request.query,"thread_id").and_then(|v| percent_decode_component(v).ok());
        if !channel.bridge.voice_available() { return Outcome::NotFound; }
        return match channel.bridge.reply_audio(thread.as_deref(),message_id) {
            Ok(body) if body.len() <= super::voice::MAX_REPLY => Outcome::Bytes {status:200,content_type:"audio/wav".into(),body,download_as:None},
            _ => Outcome::Json {status:503,body:json!({"retry":false,"reason":"Audio playback is unavailable. The reply remains in the conversation."}).to_string()},
        };
    };
    match std::fs::read(&file) {
        Ok(bytes) => Outcome::Bytes {
            status: 200,
            content_type: "audio/wav".to_string(),
            body: bytes,
            download_as: None,
        },
        Err(e) => {
            eprintln!("[richos] a minted audio id pointed at nothing readable: {e}");
            Outcome::NotFound
        }
    }
}

// -------------------------------------------------------------------------------------
// GET /api/messages?kind=file&file_id=<id>&thread_id=<thread> — a file Rich attached
// -------------------------------------------------------------------------------------

/// The phone's tap on a file Rich put in front of the user (round 36 screen 3): its bytes, from
/// this Mac, only when asked. Signed like the audio route. The id is looked up in the named
/// conversation's own record (`rich_files::read_for_phone`); there is no path in the request.
/// A file that has gone, or one too large to send, is a `410`/`413` with a `reason` the phone
/// shows; everything else that is not a file this conversation holds is the flat 404.
fn rich_file(channel: &Channel, request: &Incoming) -> Outcome {
    use richos_core::rich_files::Fetch;
    let Some(header) = request.authorization.as_deref() else { return Outcome::NotFound };
    let path = signed_path(&request.path, &request.query);
    if let Err(refusal) = verified(channel, request, header, &path) {
        return refusal;
    }
    if query_value(&request.query, "kind") != Some("file") || !channel.bridge.files_available() {
        return Outcome::NotFound;
    }
    let decoded = |key: &str| query_value(&request.query, key).and_then(|v| percent_decode_component(v).ok());
    let (Some(id), Some(thread)) = (decoded("file_id"), decoded("thread_id")) else { return Outcome::NotFound };
    let plain = |s: &str| !s.is_empty() && s.len() <= 256 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_:-".contains(&b));
    if !plain(&id) || !plain(&thread) {
        return Outcome::NotFound;
    }
    match channel.bridge.rich_file(&thread, &id) {
        Ok((name, body)) => Outcome::Bytes {
            status: 200,
            content_type: crate::output_files::mime_of(std::path::Path::new(&name)).to_string(),
            body,
            download_as: None,
        },
        Err(Fetch::Gone) => Outcome::Json {
            status: 410,
            body: json!({"retry": false, "reason": "No longer on this Mac. It was moved or deleted."}).to_string(),
        },
        Err(Fetch::TooLarge(_)) => Outcome::Json {
            status: 413,
            body: json!({"retry": false, "reason": "This file is too large to send to your phone. Open it on your Mac."}).to_string(),
        },
        Err(Fetch::Unreadable(why)) => {
            eprintln!("[richos] an attached file could not be read for the phone: {why}");
            Outcome::Json {
                status: 503,
                body: json!({"retry": false, "reason": "Your Mac could not read this file."}).to_string(),
            }
        }
        Err(Fetch::NotAttached) | Err(Fetch::Refused) => Outcome::NotFound,
    }
}

// -------------------------------------------------------------------------------------
// The retired phone web page
// -------------------------------------------------------------------------------------

/// **What a phone's browser gets at `/` now that the phone web page is retired** (CEO,
/// 2026-10-30). A plain page, no script, nothing from the conversation, that says where to go
/// instead. A phone camera that scans the pairing QR code opens `<origin>/#pair=<code>` in a
/// browser; the fragment never reaches this Mac, so this is what that person sees.
///
/// Colors, computed rather than eyeballed (WCAG 2 relative luminance): `#1a1a1a` on `#ffffff`
/// is 17.40:1 in light mode and `#f2f2f2` on `#141414` is 16.46:1 in dark mode, both far over
/// the 4.5:1 floor for normal text.
pub const RETIRED_PAGE: &str = "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<meta name=\"color-scheme\" content=\"light dark\"><title>Use the RichConnect app</title>\
<style>body{font:18px/1.5 -apple-system,system-ui,sans-serif;margin:0;padding:48px 24px;\
background:#ffffff;color:#1a1a1a}main{max-width:32em;margin:0 auto}h1{font-size:24px}\
@media (prefers-color-scheme:dark){body{background:#141414;color:#f2f2f2}}</style></head>\
<body><main><h1>Use the RichConnect app</h1>\
<p>RichOS no longer has a phone web page. To talk to Rich from your phone, use the RichConnect \
app: open it and scan the code your Mac shows.</p></main></body></html>\n";

/// **What a phone that once installed the web page gets when its browser asks for `/sw.js`.**
///
/// That page installed a service worker which kept its whole shell in the browser's cache, so
/// a phone that had it would keep opening the old page from its own cache and never ask this
/// Mac for `/` at all. A browser checks a registered worker's script for a newer version when
/// the page opens; this is the newer version. It deletes the old caches, unregisters itself
/// (which also ends the old page's push subscription) and reloads the open page, which then
/// gets [`RETIRED_PAGE`] from this Mac. A browser that never had the page never asks for it.
pub const RETIRE_WORKER: &str = "// RichOS retired its phone web page. This removes it from this browser.\n\
self.addEventListener('install', () => self.skipWaiting());\n\
self.addEventListener('activate', (event) => event.waitUntil((async () => {\n\
  for (const key of await caches.keys()) await caches.delete(key);\n\
  await self.registration.unregister();\n\
  for (const client of await self.clients.matchAll({ type: 'window' })) client.navigate(client.url);\n\
})()));\n";

/// Every credential-free `GET`: the two fixed answers, and the flat 404 for everything else,
/// including every file the retired page used to be made of.
fn retired_page(path: &str) -> Outcome {
    let (content_type, body) = match path {
        "/" | "/index.html" => ("text/html; charset=utf-8", RETIRED_PAGE),
        "/sw.js" => ("text/javascript; charset=utf-8", RETIRE_WORKER),
        _ => return Outcome::NotFound,
    };
    Outcome::Bytes {
        status: 200,
        content_type: content_type.to_string(),
        body: body.as_bytes().to_vec(),
        download_as: None,
    }
}

// -------------------------------------------------------------------------------------
// Query helpers
// -------------------------------------------------------------------------------------

/// The only query parameter that is part of the credential, and therefore the only one excluded
/// from the signed path. The phone's own comment: *"The signature covers the path WITHOUT this
/// parameter, because the parameter is the signature."*
const CREDENTIAL_PARAM: &str = "auth";

/// What the signature covers: the path, plus every query parameter except `auth`, in the order
/// the caller sent them.
///
/// **Derived by a rule rather than agreed by convention.** Both ends compute it from the same
/// definition, so there is nothing for them to drift about.
pub fn signed_path(path: &str, query: &str) -> String {
    if query.is_empty() {
        return path.to_string();
    }
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| pair.split('=').next().unwrap_or("") != CREDENTIAL_PARAM)
        .collect();
    if kept.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{}", kept.join("&"))
    }
}

fn query_value<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        if key == name {
            Some(value)
        } else {
            None
        }
    })
}

/// Percent-decode one query-parameter value, where `+` also means a space.
fn percent_decode_component(value: &str) -> Result<String, ()> {
    let spaced = value.replace('+', " ");
    Ok(super::listen::percent_decode(&spaced))
}

/// Public reach must not let unauthenticated traffic fill the Mac's log. Sample a
/// bounded reason at most once per ten seconds, without any caller-controlled path.
pub(super) fn refusal_log_due(last: &mut Option<std::time::Instant>, now: std::time::Instant) -> bool {
    if last.is_some_and(|at| now.saturating_duration_since(at).as_secs() < 10) { return false; }
    *last = Some(now); true
}
fn log_refusal(refusal: &Refusal) {
    static LAST: std::sync::OnceLock<Mutex<Option<std::time::Instant>>> = std::sync::OnceLock::new();
    if refusal_log_due(&mut LAST.get_or_init(|| Mutex::new(None)).lock().unwrap(), std::time::Instant::now()) {
        eprintln!("[richos] the phone channel refused a request: {refusal:?} (sampled)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phone::device::signing_string;
    use crate::phone::device::tests::Phone;
    use std::sync::Mutex;

    #[test]
    fn unauthenticated_refusal_flood_has_a_fixed_log_bound() {
        let start = std::time::Instant::now();
        let mut last = None;
        assert!(refusal_log_due(&mut last, start));
        for millis in 0..10000 { assert!(!refusal_log_due(&mut last, start + std::time::Duration::from_millis(millis))); }
        assert!(refusal_log_due(&mut last, start + std::time::Duration::from_secs(10)));
        assert!(!refusal_log_due(&mut last, start));
    }

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "richos-phone-routes-{tag}-{}-{}",
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

    /// A bridge that records what it was asked and never touches a spine.
    struct FakeBridge {
        submitted: Mutex<Vec<(Option<String>, String)>>,
        /// The phone's own id for each submitted message, in order.
        client_ids: Mutex<Vec<String>>,
        refuse: bool,
        voice: bool,
        rows: usize,
        questions: Option<richos_core::questions::Store>,
        /// What the shell's Oxford comma setting answers (CEO, 2026-10-19).
        oxford: bool,
    }
    impl Bridge for FakeBridge {
        fn questions_available(&self)->bool {self.questions.is_some()}
        fn oxford_comma(&self)->bool {self.oxford}
        fn answer_question(&self,thread:&str,answer:richos_core::questions::AnswerRequest)->Result<Value,String> {
            self.questions.as_ref().unwrap().answer("femcboost",thread,answer,"phone_tap","phone").map(|r|r.public_value())
        }
        fn question_shown(&self,thread:&str,question:&str)->Result<(),String> {self.questions.as_ref().unwrap().acknowledge("femcboost",thread,question,"phone")}

        fn voice_available(&self)->bool {self.voice}
        fn transcribe(&self,bytes:&[u8])->Result<String,String> { super::super::voice::validate(bytes)?;Ok("A spoken request".into()) }
        fn submit_text(&self, thread_id: Option<&str>, text: &str, client_id: &str) -> Result<Accepted, String> {
            if self.refuse {
                return Err("the intake log is not writable".into());
            }
            self.submitted.lock().unwrap().push((thread_id.map(String::from), text.to_string()));
            self.client_ids.lock().unwrap().push(client_id.to_string());
            Ok(Accepted {
                message_id: "msg_new".into(),
                thread_id: thread_id.unwrap_or("thr_5c1e").to_string(),
                at: 1_758_200_000_123,
            })
        }
        fn snapshot(&self, _thread_id: Option<&str>) -> Result<Value, String> {
            // `rows` CEO messages, alternating roles, in the gated projection's own shape.
            let items: Vec<Value> = (0..self.rows)
                .map(|i| {
                    json!({
                        "kind": if i % 2 == 0 { "user_message" } else { "rich_message" },
                        "id": format!("i{i}"),
                        "threadId": "thr_5c1e",
                        "turnId": "t1",
                        "createdAt": 1_758_200_000_000u64 + i as u64,
                        "text": format!("row {i}"),
                        "source": "text",
                        "phase": "unknown",
                        "slot": "stream",
                        "visibility": "ceo",
                        "sequence": i as u64,
                        "entityId": "femcboost",
                        "bindingRevision": 1
                    })
                })
                .collect();
            Ok(json!({ "entityId": "femcboost", "threadId": "thr_5c1e", "mode": "ceo",
                       "betweenTurns": [], "items": items }))
        }
        fn current_thread(&self) -> Option<(String, String)> {
            Some(("thr_5c1e".into(), "the proposal".into()))
        }
        fn threads(&self) -> Vec<(String, String)> {
            vec![("thr_5c1e".into(), "the proposal".into())]
        }
    }

    struct Fixture {
        dir: TempDir,
        channel: Channel,
        phone: Phone,
        device_id: String,
        challenge: String,
        bridge: Arc<FakeBridge>,
    }

    fn fixture(tag: &str) -> Fixture {
        fixture_with(tag, false, true, 4)
    }

    fn fixture_with(tag: &str, refuse: bool, pair_it: bool, rows: usize) -> Fixture {
        fixture_full(tag, refuse, pair_it, true, rows)
    }

    /// A phone that redeemed the code and that nobody at this Mac has confirmed yet.
    fn fixture_awaiting(tag: &str) -> Fixture {
        fixture_full(tag, false, true, false, 4)
    }

    fn fixture_full(tag: &str, refuse: bool, pair_it: bool, confirm_on_mac: bool, rows: usize) -> Fixture {
        let dir = TempDir::new(tag);
        let devices = Arc::new(DeviceDesk::open(&dir.0).unwrap());
        let phone = Phone::new();
        let mut device_id = String::new();
        if pair_it {
            let window = devices.open_pairing().unwrap();
            device_id = devices
                .complete_pairing(
                    &window.code,
                    &crate::phone::device::PublicKeyForm::Jwk(phone.jwk()),
                    "iPhone",
                    crate::phone::device::PairedVia::HOME,
                    crate::phone::device::Platform::IOS,
                )
                .unwrap()
                .id;
            // Paired AND confirmed by a person at this Mac: the state every route test below is
            // about. `fixture_awaiting` is the one that stops short of the press (Sage F1).
            if confirm_on_mac {
                devices.confirm_on_mac().unwrap();
            }
        }
        let challenge = devices.issue_challenge().unwrap();
        let hub = PhoneHub::new();
        hub.set_live(true);
        let bridge = Arc::new(FakeBridge { submitted: Mutex::new(Vec::new()), client_ids: Mutex::new(Vec::new()), refuse, voice:false, rows, questions:None, oxford:false });
        let channel = Channel {
            // No listener behind these tests, so there is nothing to ring. The route's own
            // behavior on a rejection — the device record going — is asserted here; that the
            // bell reaches an owner and the port closes is asserted over real TLS in `listen.rs`.
            rejected: StopSwitch::unwired(),
            devices,
            api_base: Arc::new(ApiBaseDesk::only("https://mm1.tail9a3b2.ts.net:8443")),
            hub,
            bridge: Arc::clone(&bridge) as Arc<dyn Bridge>,
            vapid_public: "BExampleVapidKey".into(),
            fingerprint_hex: "3D:9C:A1".into(),
            pairing_path: Mutex::new(crate::phone::device::PairedVia::HOME),
        };
        Fixture { dir, channel, phone, device_id, challenge, bridge }
    }

    /// Build a signed request exactly as `web/web-app/lib/api.js` does.
    fn signed(f: &Fixture, method: &str, path: &str, query: &str, body: impl AsRef<[u8]>) -> Incoming {
        let path_with_query =
            if query.is_empty() { path.to_string() } else { format!("{path}?{query}") };
        let message =
            signing_string(&f.challenge, method, &path_with_query, body.as_ref());
        let sig = super::super::b64url(&f.phone.sign(&message));
        Incoming {
            method: method.into(),
            path: path.into(),
            query: query.into(),
            authorization: Some(format!("RichOS-Device {}.{}.{sig}", f.device_id, f.challenge)),
            last_event_id: None,
            content_type: Some("application/json".into()),
            body: body.as_ref().to_vec(),
        }
    }

    /// The stream's request: the credential in the query, the signature over the path without it.
    fn signed_stream(f: &Fixture, query: &str) -> Incoming {
        let path_with_query =
            if query.is_empty() { "/api/events".to_string() } else { format!("/api/events?{query}") };
        let message = signing_string(&f.challenge, "GET", &path_with_query, b"");
        let sig = super::super::b64url(&f.phone.sign(&message));
        let auth = format!("RichOS-Device {}.{}.{sig}", f.device_id, f.challenge);
        // `encodeURIComponent` — the phone's own call. `.` and `_` and `-` are not escaped; `+`
        // and `/` in base64url do not occur.
        let encoded = auth.replace(' ', "%20");
        let full = if query.is_empty() { format!("auth={encoded}") } else { format!("{query}&auth={encoded}") };
        Incoming {
            method: "GET".into(),
            path: "/api/events".into(),
            query: full,
            authorization: None,
            last_event_id: None,
            content_type: None,
            body: Vec::new(),
        }
    }

    fn plain(method: &str, path: &str) -> Incoming {
        Incoming {
            method: method.into(),
            path: path.into(),
            query: String::new(),
            authorization: None,
            last_event_id: None,
            content_type: None,
            body: Vec::new(),
        }
    }


    #[test]
    fn reply_receipts_are_authenticated_scoped_and_survive_restart() {
        let f=fixture("reply-receipts");
        let body=r#"{"seen_reply":{"thread":"thread-a","id":"reply-a"}}"#;
        let mut request=signed(&f,"POST","/api/pair","",body);
        request.authorization=None;
        assert!(matches!(dispatch(&f.channel,&request),Outcome::NotFound));
        assert!(!f.channel.devices.paired().unwrap().reply_receipts);
        assert!(matches!(dispatch(&f.channel,&signed(&f,"POST","/api/pair","",body)),Outcome::Json{status:200,..}));
        let reopened=DeviceDesk::open(&f.dir.0).unwrap().paired().unwrap();
        assert!(reopened.reply_receipts);
        assert_eq!(reopened.seen_replies,vec![("thread-a".into(),"reply-a".into())]);
        let bad=r#"{"seen_reply":{"thread":"","id":"reply-a"}}"#;
        assert!(matches!(dispatch(&f.channel,&signed(&f,"POST","/api/pair","",bad)),Outcome::NotFound));
    }

    #[test]
    fn web_push_disable_clears_the_stored_subscription_and_receipts_bound_presence() {
        let f=fixture("push-disable");
        let body=r#"{"push":{"endpoint":"https://fcm.googleapis.com/test","keys":{"p256dh":"public","auth":"secret"}},"reply_receipts":true}"#;
        assert!(matches!(dispatch(&f.channel,&signed(&f,"POST","/api/pair","",body)),Outcome::Json{status:200,..}));
        let slot=f.channel.devices.claim_stream(&f.device_id).unwrap();
        // A stale connection cannot suppress a reply this browser has not acknowledged.
        assert!(super::super::push::should_notify(&f.channel.devices,&f.device_id,"a","r"));
        f.channel.devices.record_reply_receipt(Some(("a","r"))).unwrap();
        assert!(!super::super::push::should_notify(&f.channel.devices,&f.device_id,"a","r"));
        assert!(super::super::push::should_notify(&f.channel.devices,&f.device_id,"b","r"));
        for n in 0..70 {f.channel.devices.record_reply_receipt(Some(("a",&format!("r{n}")))).unwrap();}
        assert_eq!(f.channel.devices.paired().unwrap().seen_replies.len(),64);
        assert!(matches!(dispatch(&f.channel,&signed(&f,"POST","/api/pair","",r#"{"push":null}"#)),Outcome::Json{status:200,..}));
        assert!(f.channel.devices.paired().unwrap().push.is_none());
        assert!(!super::super::push::should_notify(&f.channel.devices,&f.device_id,"a","new"));
        drop(slot);
    }

    // --- the positive control, first ------------------------------------------------------

    #[test]
    fn a_paired_phone_can_post_a_message_and_it_reaches_the_bridge() {
        let f = fixture("post");
        let body = r#"{"client_id":"01J8","thread_id":"thr_5c1e","kind":"text","text":"where are we on the proposal?","sent_at":"2026-09-18T13:00:00.000Z"}"#;
        match dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", body)) {
            Outcome::Json { status, body } => {
                assert_eq!(status, 200);
                let v: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(v["message_id"], "msg_new");
                assert_eq!(v["thread_id"], "thr_5c1e");
                assert_eq!(v["duplicate"], false);
                assert!(v["cursor"].as_u64().unwrap() > 0);
                // Read off `date -u -r 1758200000`, not typed: the first version of this line
                // said 13:33:20 from memory and the tool said 12:53:20.
                assert_eq!(v["accepted_at"], "2025-09-18T12:53:20.123Z");
            }
            other => panic!("{other:?}"),
        }
        let submitted = f.bridge.submitted.lock().unwrap();
        assert_eq!(submitted.len(), 1);
        assert_eq!(submitted[0].1, "where are we on the proposal?");
        // The phone's own id for the message reaches the turn with the words.
        assert_eq!(*f.bridge.client_ids.lock().unwrap(), vec!["01J8".to_string()]);
    }

    // --- everything else is a 404 ----------------------------------------------------------

    #[test]
    fn an_unpaired_caller_gets_the_same_answer_to_every_question_including_the_real_routes() {
        // A caller that is not the paired phone cannot map the surface by the SHAPE of the
        // refusals. Every one of these is 404 with an empty body — including the four real ones.
        let f = fixture_with("unpaired", false, false, 4);
        for (method, path) in [
            ("POST", "/api/messages"),
            ("GET", "/api/events"),
            ("GET", "/api/audio/msg_1"),
            ("POST", "/api/pair"),
            ("GET", "/api/threads"),
            ("POST", "/api/audio/msg_1"),
            ("DELETE", "/api/messages"),
            ("GET", "/admin"),
            ("GET", "/.env"),
            ("GET", "/api/"),
            ("PUT", "/"),
        ] {
            assert_eq!(dispatch(&f.channel, &plain(method, path)), Outcome::NotFound, "{method} {path}");
        }
    }

    #[test]
    fn a_paired_phone_without_a_signature_gets_nothing() {
        // Being on his Wi-Fi is not authentication (plan §2.5 item 7).
        let f = fixture("no-sig");
        assert_eq!(dispatch(&f.channel, &plain("POST", "/api/messages")), Outcome::NotFound);
        assert_eq!(dispatch(&f.channel, &plain("GET", "/api/events")), Outcome::NotFound);
        assert_eq!(dispatch(&f.channel, &plain("GET", "/api/audio/msg_1")), Outcome::NotFound);
    }

    #[test]
    fn a_signature_from_another_key_gets_nothing() {
        let f = fixture("impostor");
        let mut request = signed(&f, "POST", "/api/messages", "", r#"{"client_id":"c","kind":"text","text":"x"}"#);
        let impostor = Phone::new();
        let message = signing_string(&f.challenge, "POST", "/api/messages", &request.body);
        let sig = super::super::b64url(&impostor.sign(&message));
        request.authorization = Some(format!("RichOS-Device {}.{}.{sig}", f.device_id, f.challenge));
        assert_eq!(dispatch(&f.channel, &request), Outcome::NotFound);
    }

    #[test]
    fn a_forgotten_phone_is_told_so_once_rather_than_refused_forever() {
        // The one deliberate non-404: `web/web-app/lib/api.js` treats 403 + `{"revoked":true}` as
        // final, clears its credential and says so. A 404 would be a phone that hammers his Mac.
        let f = fixture("revoked");
        f.channel.devices.forget().unwrap();
        let out = dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", r#"{"client_id":"c","kind":"text","text":"x"}"#));
        assert_eq!(out, Outcome::Revoked);
        assert_eq!(out.status(), 403);
    }

    #[test]
    fn a_body_over_the_ceiling_is_refused_before_it_is_looked_at() {
        let f = fixture("too-big");
        let mut request = signed(&f, "POST", "/api/messages", "", "{}");
        request.body = vec![b'x'; MAX_BODY_BYTES + 1];
        assert_eq!(dispatch(&f.channel, &request), Outcome::PayloadTooLarge);
        let mut at_ceiling = signed(&f, "POST", "/api/messages", "", "{}");
        at_ceiling.body = vec![b'x'; MAX_BODY_BYTES];
        assert_ne!(dispatch(&f.channel, &at_ceiling), Outcome::PayloadTooLarge);
    }

    #[test]
    fn an_empty_message_is_not_a_turn() {
        let f = fixture("empty");
        let out = dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", r#"{"client_id":"c","kind":"text","text":"   "}"#));
        assert_eq!(out, Outcome::NotFound);
        assert!(f.bridge.submitted.lock().unwrap().is_empty());
    }

    #[test]
    fn a_retried_post_gets_the_original_answer_marked_duplicate_rather_than_acting_twice() {
        // The phone's queue retries after a failure it could not classify. The CEO's one message
        // must stay one message, and the phone must be able to tell that is what happened.
        let f = fixture("retry");
        let body = r#"{"client_id":"01JSAME","kind":"text","text":"one message"}"#;
        let first = dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", body));
        let second = dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", body));
        match (first, second) {
            (Outcome::Json { body: a, .. }, Outcome::Json { body: b, .. }) => {
                let a: Value = serde_json::from_str(&a).unwrap();
                let b: Value = serde_json::from_str(&b).unwrap();
                assert_eq!(a["duplicate"], false);
                assert_eq!(b["duplicate"], true, "a retry was not marked as a duplicate");
                assert_eq!(a["message_id"], b["message_id"], "the retry got a different message");
                assert_eq!(a["cursor"], b["cursor"]);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(f.bridge.submitted.lock().unwrap().len(), 1, "the message was submitted twice");
    }

    #[test]
    fn an_uncertain_bridge_result_never_becomes_an_automatic_second_submission() {
        let f = fixture_with("unwritable", true, true, 4);
        let request = signed(&f, "POST", "/api/messages", "", r#"{"client_id":"c","kind":"text","text":"hello"}"#);
        for _ in 0..2 {
            let out = dispatch(&f.channel, &request);
            assert_eq!(out.status(), 503);
            if let Outcome::Json { body, .. } = out {
                let v: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(v["retry"], false);
                assert!(v["reason"].as_str().unwrap().contains("still on your phone"));
            } else { panic!("missing recovery sentence"); }
        }
    }

    #[test]
    fn bounded_signed_voice_is_durable_and_bound_to_its_thread() {
        let mut f = fixture("voice-enabled");
        // Install a bridge that actually accepts speech, leaving old-Mac coverage intact.
        let bridge = Arc::new(FakeBridge {submitted:Mutex::new(Vec::new()),client_ids:Mutex::new(Vec::new()),refuse:false,voice:true,rows:3,questions:None,oxford:false});
        f.channel.bridge = bridge.clone();
        let bytes=richos_voice::wav::encode_pcm16_mono(&vec![0.1;16000],16000);
        let query="client_id=spoken-1&thread_id=thr_5c1e&kind=voice&codec=wav16k&sample_rate=16000&seconds=1";
        let mut request=signed(&f,"POST","/api/messages",query,&bytes);
        request.content_type=Some("audio/wav".into());
        assert_eq!(dispatch(&f.channel,&request).status(),200);
        assert_eq!(dispatch(&f.channel,&request).status(),200);
        assert_eq!(bridge.submitted.lock().unwrap().len(),1);
        assert_eq!(bridge.submitted.lock().unwrap()[0],(Some("thr_5c1e".into()),"A spoken request".into()));
        // The recording's own id reaches the turn, so the Mac's row for it can name it.
        assert_eq!(*bridge.client_ids.lock().unwrap(),vec!["spoken-1".to_string()]);
        let mut changed=bytes.clone();changed[45]^=1;
        let mut request=signed(&f,"POST","/api/messages",query,&changed);request.content_type=Some("audio/wav".into());
        assert_eq!(dispatch(&f.channel,&request).status(),409);
        assert_eq!(body_limit("POST","/api/pair","",Some("audio/wav")),MAX_BODY_BYTES);
        let mut oversized=signed(&f,"POST","/api/messages",query,vec![0;super::super::voice::MAX_UPLOAD+1]);oversized.content_type=Some("audio/wav".into());
        assert_eq!(dispatch(&f.channel,&oversized),Outcome::PayloadTooLarge);
    }

    #[test]
    fn a_voice_note_is_refused_with_a_sentence_rather_than_accepted_and_dropped() {
        // Slice B is not built. The route and its parameters are honored so the phone's queue
        // gets a real answer, and the answer says what is true.
        let f = fixture("voice");
        let query = "client_id=c1&thread_id=thr_5c1e&kind=voice&codec=wav16k&sample_rate=16000&seconds=3";
        let path_with_query = format!("/api/messages?{query}");
        let wav = b"RIFF....WAVE".to_vec();
        let message = signing_string(&f.challenge, "POST", &path_with_query, &wav);
        let sig = super::super::b64url(&f.phone.sign(&message));
        let request = Incoming {
            method: "POST".into(),
            path: "/api/messages".into(),
            query: query.into(),
            authorization: Some(format!("RichOS-Device {}.{}.{sig}", f.device_id, f.challenge)),
            last_event_id: None,
            content_type: Some("audio/wav".into()),
            body: wav,
        };
        let out = dispatch(&f.channel, &request);
        assert_eq!(out.status(), 503);
        match out {
            Outcome::Json { body, .. } => {
                let v: Value = serde_json::from_str(&body).unwrap();
                assert!(v["reason"].as_str().unwrap().contains("still on your phone"), "{body}");
                // AND IT SAYS SO ONCE. This build's answer to a voice note will be identical every
                // time it is asked, so the phone is told to stop asking. Without it the phone reads
                // a 503 as a fault, a fault is retryable, and the recording holds the head of his
                // queue against every text he types afterwards (plan §2 A).
                assert_eq!(
                    v["retry"], false,
                    "a refusal that can never change was not marked final: {body}"
                );
            }
            other => panic!("{other:?}"),
        }
        assert!(f.bridge.submitted.lock().unwrap().is_empty(), "a voice note was filed as text");
    }

    #[test]
    fn voice_is_offered_exactly_when_the_route_would_take_one() {
        // CAPABILITIES and the route are one fact written twice, and this is what holds them
        // together. It fails the day slice B lands and nobody advertises it — a Mac that can take
        // a voice note behind a phone that will not show the button — and the day voice is
        // advertised before it works, which is today's defect with a longer fuse.
        //
        // The route is ASKED rather than read: a real signed voice note goes through `dispatch`,
        // and whether it was taken is read off the answer.
        let f = fixture("capability-agreement");
        let query = "client_id=c-cap&thread_id=thr_5c1e&kind=voice&codec=wav16k&sample_rate=16000&seconds=1";
        let path_with_query = format!("/api/messages?{query}");
        let wav = b"RIFF....WAVE".to_vec();
        let message = signing_string(&f.challenge, "POST", &path_with_query, &wav);
        let sig = super::super::b64url(&f.phone.sign(&message));
        let request = Incoming {
            method: "POST".into(),
            path: "/api/messages".into(),
            query: query.into(),
            authorization: Some(format!("RichOS-Device {}.{}.{sig}", f.device_id, f.challenge)),
            last_event_id: None,
            content_type: Some("audio/wav".into()),
            body: wav,
        };
        let taken = dispatch(&f.channel, &request).status() == 200;
        let advertised = CAPABILITIES.contains(&"voice");
        assert_eq!(
            advertised, taken,
            "CAPABILITIES says voice is {}, and the route {} a voice note",
            if advertised { "offered" } else { "not offered" },
            if taken { "took" } else { "refused" }
        );
        // Text is the one thing this build is FOR, so it is never absent from the list.
        assert!(CAPABILITIES.contains(&"text"), "the Mac stopped advertising the only thing it does");
    }

    #[test]
    fn the_build_the_phone_is_told_is_the_one_this_binary_was_compiled_as() {
        // Not a second place the version can be written. `updates.rs:141` writes the chain out:
        // `Cargo.toml` -> `CARGO_PKG_VERSION` -> here, one hop and no branch. A `build` assembled
        // at a call site would be a `build` a call site could get wrong.
        assert_eq!(BUILD, env!("CARGO_PKG_VERSION"));
        assert!(!BUILD.is_empty());
        assert!(BUILD.chars().next().unwrap().is_ascii_digit(), "the build is not a version: {BUILD}");
    }

    // --- pairing ----------------------------------------------------------------------------

    #[test]
    fn pairing_answers_with_everything_the_phone_needs_and_nothing_it_does_not() {
        let f = fixture_with("pair", false, false, 4);
        let window = f.channel.devices.open_pairing().unwrap();
        let phone = Phone::new();
        let mut request = plain("POST", "/api/pair");
        request.body = json!({
            "code": window.code,
            "public_key_jwk": phone.jwk(),
            "device_name": "iPhone",
        })
        .to_string()
        .into_bytes();
        match dispatch(&f.channel, &request) {
            Outcome::Json { status, body } => {
                assert_eq!(status, 200);
                let v: Value = serde_json::from_str(&body).unwrap();
                assert!(v["device_id"].as_str().unwrap().starts_with("dev_"));
                assert_eq!(v["api_base"], "https://mm1.tail9a3b2.ts.net:8443");
                assert_eq!(v["vapid_public_key"], "BExampleVapidKey");
                // THE HASH, NOT THE WORDS. `web/web-app/lib/fingerprint.js` renders the six words
                // itself, deliberately: words the Mac chose could belong to a different
                // certificate than the one it is serving.
                assert_eq!(v["ca_fingerprint_sha256"], "3D:9C:A1");
                assert!(v.get("ca_fingerprint_words").is_none(), "the Mac sent words");
                assert!(!v["challenge"].as_str().unwrap().is_empty());
                assert_eq!(v["threads"][0]["id"], "thr_5c1e");
                // Nothing about the conversation travels in a pair response.
                assert!(v.get("messages").is_none());
                assert!(v.get("timeline").is_none());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn pairing_with_a_wrong_code_or_no_window_or_a_bad_key_is_a_404() {
        let f = fixture_with("pair-refuse", false, false, 4);
        let phone = Phone::new();

        let mut no_window = plain("POST", "/api/pair");
        no_window.body =
            json!({ "code": "WRONGCOD", "public_key_jwk": phone.jwk() }).to_string().into_bytes();
        assert_eq!(dispatch(&f.channel, &no_window), Outcome::NotFound, "no window was open");

        let window = f.channel.devices.open_pairing().unwrap();
        let mut wrong_code = plain("POST", "/api/pair");
        wrong_code.body =
            json!({ "code": "WRONGCOD", "public_key_jwk": phone.jwk() }).to_string().into_bytes();
        assert_eq!(dispatch(&f.channel, &wrong_code), Outcome::NotFound);
        assert!(!f.channel.devices.is_paired());

        let window2 = f.channel.devices.open_pairing().unwrap();
        assert_ne!(window.code, window2.code);
        let mut bad_key = plain("POST", "/api/pair");
        bad_key.body = json!({
            "code": window2.code,
            "public_key_jwk": { "kty": "RSA", "n": "AA", "e": "AQAB" },
        })
        .to_string()
        .into_bytes();
        assert_eq!(dispatch(&f.channel, &bad_key), Outcome::NotFound);
        assert!(!f.channel.devices.is_paired());
    }

    #[test]
    fn the_device_record_is_updated_on_the_same_route_and_only_with_a_signature() {
        // The phone's `registerPush` posts here with no code. A fifth route would have broken
        // §2.5's ceiling of four, so it rides on this one — behind the signature.
        let f = fixture("device-record");
        let body = json!({
            "device_id": f.device_id,
            "push_transport": "web-push",
            "push": {
                "endpoint": "https://api.push.apple.com/3/device/abc",
                "keys": { "p256dh": "BA", "auth": "AA" }
            }
        })
        .to_string();
        let out = dispatch(&f.channel, &signed(&f, "POST", "/api/pair", "", &body));
        assert_eq!(out.status(), 200);
        assert_eq!(
            f.channel.devices.paired().unwrap().push.unwrap().endpoint,
            "https://api.push.apple.com/3/device/abc"
        );

        // Unsigned, it is nothing.
        let mut unsigned = plain("POST", "/api/pair");
        unsigned.body = body.clone().into_bytes();
        assert_eq!(dispatch(&f.channel, &unsigned), Outcome::NotFound);
    }

    /// **THE MAC LEARNS THE ANSWER TO THE SIX WORDS, WHICH IT COULD NOT BEFORE** — Ray's
    /// nightly `.7` walk, defect 2.
    ///
    /// Its sheet read `It is paired. Open Rich on it and keep talking.` while the phone, on
    /// screen beside it, was still asking `They match — pair this phone` / `They do not match`.
    /// That was not a wording slip: nothing in the protocol carried the person's answer, so the
    /// Mac had nothing to wait for. This is the message.
    #[test]
    fn a_phone_is_not_confirmed_until_the_person_says_the_six_words_matched() {
        let f = fixture("confirm");
        assert!(
            !f.channel.devices.fingerprint_confirmed(),
            "a phone that has only redeemed a code counts as confirmed"
        );

        let body = json!({ "device_id": f.device_id, "fingerprint_confirmed": true }).to_string();
        let out = dispatch(&f.channel, &signed(&f, "POST", "/api/pair", "", &body));
        assert_eq!(out.status(), 200);
        assert!(f.channel.devices.fingerprint_confirmed());
        assert!(f.channel.devices.is_paired(), "confirming forgot the phone");

        let native=json!({"fingerprint_confirmed":true,"push_transport":"apns"}).to_string();
        assert_eq!(dispatch(&f.channel,&signed(&f,"POST","/api/pair","",&native)).status(),200);
        assert_eq!(f.channel.devices.paired().unwrap().push_transport,"apns");

        // Idempotent: a phone that says it twice, or says it again after a relaunch, is fine.
        let again = dispatch(&f.channel, &signed(&f, "POST", "/api/pair", "", &body));
        assert_eq!(again.status(), 200);
        assert!(f.channel.devices.fingerprint_confirmed());
    }

    #[test]
    fn they_do_not_match_forgets_the_phone_rather_than_recording_a_flag() {
        // The phone throws its own key away on that press. A Mac that kept the device record
        // "in case he was mistaken" would leave a live credential for something the person has
        // just said may not be his Mac's phone at all.
        let f = fixture("mismatch");
        let body = json!({ "device_id": f.device_id, "fingerprint_confirmed": false }).to_string();
        let out = dispatch(&f.channel, &signed(&f, "POST", "/api/pair", "", &body));
        assert_eq!(out.status(), 200);
        assert!(!f.channel.devices.is_paired(), "a rejected phone is still paired");
        assert!(!f.channel.devices.fingerprint_confirmed());
    }

    #[test]
    fn the_confirmation_needs_a_signature_and_a_boolean_and_accepts_nothing_else() {
        let f = fixture("confirm-strict");

        // Unsigned it is nothing at all — being on his Wi-Fi is not authentication.
        let mut unsigned = plain("POST", "/api/pair");
        unsigned.body = json!({ "device_id": f.device_id, "fingerprint_confirmed": true })
            .to_string()
            .into_bytes();
        assert_eq!(dispatch(&f.channel, &unsigned), Outcome::NotFound);
        assert!(!f.channel.devices.fingerprint_confirmed());

        // STRICTLY a boolean. The string "true" and the number 1 are not a person's answer, and
        // reading them as one would be this Mac guessing at the one thing it must not guess at.
        for shape in [json!("true"), json!(1), json!(null)] {
            let body = json!({ "device_id": f.device_id, "fingerprint_confirmed": shape }).to_string();
            let out = dispatch(&f.channel, &signed(&f, "POST", "/api/pair", "", &body));
            assert_eq!(out.status(), 200, "the request itself is well formed; the field is not");
            assert!(
                !f.channel.devices.fingerprint_confirmed(),
                "{shape} was read as a confirmation"
            );
            assert!(f.channel.devices.is_paired(), "{shape} was read as a rejection");
        }
    }

    // --- Sage F1: nothing but the answer to the six words until the Mac is pressed -------------

    /// **THE HIGH FINDING, THROUGH THE ROUTE TABLE** — Sage's review F1, its own test, as he wrote
    /// it: pair with a code, then a message, the stream and an attachment upload signed by the new
    /// device are each refused with the awaiting answer; the phone's own `fingerprint_confirmed:
    /// true` does not change that; after `confirm_on_mac()` the same three succeed. On `main`
    /// before this change all three were answered 200 the moment the code was redeemed.
    #[test]
    fn a_device_nobody_confirmed_at_the_mac_is_answered_awaiting_on_every_route() {
        let f = fixture_awaiting("f1-routes");
        let message = r#"{"client_id":"01JF1","thread_id":"thr_5c1e","kind":"text","text":"read me everything"}"#;
        let three = |f: &Fixture| {
            [
                ("message", dispatch(&f.channel, &signed(f, "POST", "/api/messages", "", message))),
                ("stream", dispatch(&f.channel, &signed_stream(f, "thread_id=thr_5c1e"))),
                ("upload", dispatch(&f.channel, &upload(f, "c1", "a1", "a.jpg", "image/jpeg", JPEG))),
            ]
        };
        for (what, out) in three(&f) {
            assert_eq!(out, Outcome::AwaitingMac, "{what} reached a device nobody confirmed at the Mac");
            assert_eq!(out.status(), 409, "{what}");
        }
        assert!(f.bridge.submitted.lock().unwrap().is_empty(), "an unconfirmed device's words reached Rich");

        // The phone confirming ITSELF is answered, and it activates nothing.
        let answer = dispatch(&f.channel, &signed(&f, "POST", "/api/pair", "", json!({"fingerprint_confirmed": true}).to_string()));
        match answer {
            Outcome::Json { status: 200, body } => {
                let v: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(v["awaiting_mac_confirmation"], true, "the phone was not told to press on the Mac: {body}");
            }
            other => panic!("the phone's own answer was refused: {other:?}"),
        }
        assert!(f.channel.devices.fingerprint_confirmed());
        for (what, out) in three(&f) {
            assert_eq!(out, Outcome::AwaitingMac, "{what}: the phone's own confirmation let it in");
        }

        // The press on the Mac.
        f.channel.devices.confirm_on_mac().unwrap();
        for (what, out) in three(&f) {
            assert!(matches!(out.status(), 200), "{what} was still refused after the Mac confirmed: {out:?}");
        }
        assert_eq!(f.bridge.submitted.lock().unwrap().len(), 1);
    }

    /// The one allowed request carries nothing that acts. A body that asks for a push
    /// subscription BESIDE the answer is not an answer, and is gated like everything else.
    #[test]
    fn an_unconfirmed_device_cannot_carry_an_action_in_beside_its_answer() {
        let f = fixture_awaiting("f1-smuggle");
        let body = json!({
            "fingerprint_confirmed": true,
            "push": { "endpoint": "https://web.push.apple.com/x", "keys": { "p256dh": "BA", "auth": "AA" } }
        })
        .to_string();
        assert_eq!(dispatch(&f.channel, &signed(&f, "POST", "/api/pair", "", &body)), Outcome::AwaitingMac);
        assert!(f.channel.devices.paired().unwrap().push.is_none());
        assert!(!f.channel.devices.fingerprint_confirmed(), "half of a refused request was acted on");
        for extra in [json!({"native_push": null}), json!({"delivered_cursor": 9}), json!({"seen_reply": {"thread": "a", "id": "b"}})] {
            let mut body = extra.clone();
            body["fingerprint_confirmed"] = json!(true);
            assert_eq!(dispatch(&f.channel, &signed(&f, "POST", "/api/pair", "", body.to_string())), Outcome::AwaitingMac, "{extra}");
        }
    }

    /// **THE SPENT-CODE ALARM THROUGH THE ROUTE** (Sage F1 item 4): a second device redeeming the
    /// code while the first is unconfirmed gets the same flat 404 as any refused code, the first
    /// device is forgotten, and the owner of the listener is told to stop and why. The same phone
    /// asking twice is answered again, not alarmed.
    #[test]
    fn a_second_device_with_the_code_rings_the_owner_and_learns_nothing() {
        let mut f = fixture_with("alarm-route", false, false, 4);
        let (doorbell, heard) = std::sync::mpsc::channel::<&'static str>();
        f.channel.rejected = StopSwitch::to(doorbell);
        let window = f.channel.devices.open_pairing().unwrap();
        let pair = |f: &Fixture, phone: &Phone| {
            let mut request = plain("POST", "/api/pair");
            request.body = json!({"code": window.code, "public_key_jwk": phone.jwk(), "pairing_version": 2}).to_string().into_bytes();
            dispatch(&f.channel, &request)
        };
        let first = Phone::new();
        let (status, answer) = json_of(pair(&f, &first));
        assert_eq!(status, 200);
        let within = answer["confirm_within_seconds"].as_u64().expect("the answer carries no bound for the phone's wait");
        assert!((290..=300).contains(&within), "the bound is not the window's remainder: {within}");
        assert_eq!(json_of(pair(&f, &first)).0, 200, "the same phone asking twice was refused");
        assert!(heard.try_recv().is_err(), "the same phone asking twice rang the alarm");

        assert_eq!(pair(&f, &Phone::new()), Outcome::NotFound, "the second device learned something");
        assert!(!f.channel.devices.is_paired(), "the unconfirmed first device stayed paired");
        assert_eq!(heard.try_recv(), Ok(super::super::device::StoppedBy::CODE_USED_TWICE));
    }

    /// "They do not match" is the other thing an unconfirmed phone may say, and it still forgets.
    #[test]
    fn an_unconfirmed_phone_can_still_say_they_do_not_match() {
        let f = fixture_awaiting("f1-reject");
        let out = dispatch(&f.channel, &signed(&f, "POST", "/api/pair", "", json!({"fingerprint_confirmed": false}).to_string()));
        assert_eq!(out.status(), 200);
        assert!(!f.channel.devices.is_paired(), "a phone that rejected the words stayed paired");
    }

    #[test]
    fn a_push_subscription_outside_the_allowlist_is_never_stored() {
        // Plan §2.6 at the edge: what cannot be stored can never be dialed.
        let f = fixture("push-host");
        let bad = json!({
            "device_id": f.device_id,
            "push": { "endpoint": "https://fcm.googleapis.com.evil.example/x", "keys": { "p256dh": "BA", "auth": "AA" } }
        })
        .to_string();
        assert_eq!(dispatch(&f.channel, &signed(&f, "POST", "/api/pair", "", &bad)), Outcome::NotFound);
        assert!(f.channel.devices.paired().unwrap().push.is_none());
    }

    // --- the event stream --------------------------------------------------------------------

    #[test]
    fn the_stream_opens_with_a_hello_that_carries_everything_the_phone_needs_to_start() {
        let f = fixture("stream-open");
        match dispatch(&f.channel, &signed_stream(&f, "thread_id=thr_5c1e")) {
            Outcome::Stream { opening, since, .. } => {
                assert_eq!(since, None);
                assert_eq!(opening.len(), 1);
                let wire = &opening[0];
                assert!(wire.contains("event: hello"), "{wire}");
                assert!(wire.ends_with("\n\n"));
                let data: Value =
                    serde_json::from_str(wire.split("data: ").nth(1).unwrap().trim_end()).unwrap();
                assert!(!data["challenge"].as_str().unwrap().is_empty());
                assert_eq!(data["api_base"], "https://mm1.tail9a3b2.ts.net:8443");
                assert_eq!(data["thread_id"], "thr_5c1e");
                assert_eq!(data["latest_cursor"], 4);
                assert_eq!(data["vapid_public_key"], "BExampleVapidKey");
                // What this Mac can be asked for, and which RichOS is answering (contract §5.3).
                // The phone renders a control only where the capability behind it is named here,
                // so an absent or renamed key is a button that disappears from his screen.
                // `attachments` is appended, never inserted: the long-standing entries keep their
                // place for any client that ever read them positionally.
                assert_eq!(data["capabilities"], json!(["text", "attachments", "pair-v2", "pair-wait"]));
                // The preserved iPhone core accepts exactly 1 (`mobile/core/client.js:147`).
                assert_eq!(data["protocol_version"], 1);
                assert_eq!(data["attachment_limits"]["max_file_bytes"], 26_214_400);
                assert_eq!(data["build"], BUILD);
                // The Oxford comma setting, his default off (CEO, 2026-10-19).
                assert_eq!(data["oxford_comma"], false);
                assert_eq!(data["threads"][0]["title"], "the proposal");
                assert_eq!(data["messages"].as_array().unwrap().len(), 4);
                assert_eq!(data["messages"][0]["cursor"], 1);
                assert_eq!(data["messages"][3]["cursor"], 4);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_hello_carries_the_oxford_comma_setting_the_shell_holds() {
        // CEO, 2026-10-19: what Claude writes is drawn with or without the comma before and/or/nor
        // as he chose, on the phone as on the Mac. The phone reads his choice from the hello.
        let mut f = fixture("oxford");
        let bridge = Arc::new(FakeBridge { submitted: Mutex::new(Vec::new()), client_ids: Mutex::new(Vec::new()), refuse: false, voice: false, rows: 1, questions: None, oxford: true });
        f.channel.bridge = bridge;
        match dispatch(&f.channel, &signed_stream(&f, "thread_id=thr_5c1e")) {
            Outcome::Stream { opening, .. } => {
                let data: Value = serde_json::from_str(opening[0].split("data: ").nth(1).unwrap().trim_end()).unwrap();
                assert_eq!(data["oxford_comma"], true, "{data}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_hello_seeds_the_live_cursor_from_the_projection() {
        // So a live row continues the conversation's own numbering rather than starting a second
        // sequence, which is what makes a live cursor and a reloaded one mean the same thing.
        let f = fixture_with("seed", false, true, 9);
        assert_eq!(f.channel.hub.cursor_now(), 0);
        dispatch(&f.channel, &signed_stream(&f, "thread_id=thr_5c1e"));
        assert_eq!(f.channel.hub.cursor_now(), 9);
    }

    #[test]
    fn a_stream_with_no_credential_or_a_forged_one_gets_nothing() {
        let f = fixture("stream-refuse");
        let mut no_auth = plain("GET", "/api/events");
        no_auth.query = "thread_id=thr_5c1e".into();
        assert_eq!(dispatch(&f.channel, &no_auth), Outcome::NotFound);

        let mut forged = signed_stream(&f, "thread_id=thr_5c1e");
        forged.query = forged.query.replace("auth=", "auth=RichOS-Device%20dev_x.CH.AAAA&ignored=");
        assert_eq!(dispatch(&f.channel, &forged), Outcome::NotFound);
    }

    #[test]
    fn the_signed_path_excludes_only_the_credential_parameter() {
        // The rule both ends derive the signed path from. If it changed, every request would fail
        // at once — so it is pinned here.
        assert_eq!(signed_path("/api/events", ""), "/api/events");
        assert_eq!(signed_path("/api/events", "auth=abc"), "/api/events");
        assert_eq!(
            signed_path("/api/events", "thread_id=t&since=4&auth=abc"),
            "/api/events?thread_id=t&since=4"
        );
        assert_eq!(signed_path("/api/messages", "a=1&b=2"), "/api/messages?a=1&b=2");
    }

    #[test]
    fn a_reconnection_with_a_cursor_gets_its_tail_and_not_a_whole_hello() {
        let f = fixture("reconnect");
        f.channel.hub.seed_cursor(4);
        f.channel.hub.publish("message", 5, "{\"n\":5}".into()).unwrap();
        match dispatch(&f.channel, &signed_stream(&f, "thread_id=thr_5c1e&since=4")) {
            Outcome::Stream { opening, since, .. } => {
                assert_eq!(since, Some(4));
                assert_eq!(opening.len(), 1);
                assert!(opening[0].contains("event: message"), "{}", opening[0]);
                assert!(!opening[0].contains("event: hello"), "a tail was sent as a hello");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_browser_reconnection_uses_last_event_id_when_the_url_has_no_since() {
        // `EventSource` reconnects to the identical URL and sets this header itself. Without
        // honoring it, every automatic reconnection would redraw his whole thread.
        let f = fixture("last-event-id");
        f.channel.hub.seed_cursor(4);
        f.channel.hub.publish("message", 5, "{\"n\":5}".into()).unwrap();
        let mut request = signed_stream(&f, "thread_id=thr_5c1e");
        request.last_event_id = Some("4".into());
        match dispatch(&f.channel, &request) {
            Outcome::Stream { opening, since, .. } => {
                assert_eq!(since, Some(4));
                assert!(opening[0].contains("event: message"), "{}", opening[0]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_backfill_is_the_same_route_with_before_and_it_is_json_rather_than_a_stream() {
        // Chunked loading behind infinite scroll. Page numbers and pagination controls are never
        // built (standing rule) — this is a window into an append-only projection.
        let f = fixture_with("backfill", false, true, 10);
        let out = dispatch(&f.channel, &signed_stream(&f, "thread_id=thr_5c1e&before=8&limit=3"));
        match out {
            Outcome::Json { status, body } => {
                assert_eq!(status, 200);
                let v: Value = serde_json::from_str(&body).unwrap();
                let messages = v["messages"].as_array().unwrap();
                assert_eq!(messages.len(), 3);
                // The three immediately before cursor 8, oldest first.
                assert_eq!(messages[0]["cursor"], 5);
                assert_eq!(messages[2]["cursor"], 7);
                assert_eq!(v["more"], true, "there are four earlier rows and `more` said otherwise");
            }
            other => panic!("{other:?}"),
        }

        // And at the start of the thread, `more` is false rather than "probably".
        let out = dispatch(&f.channel, &signed_stream(&f, "thread_id=thr_5c1e&before=3&limit=40"));
        match out {
            Outcome::Json { body, .. } => {
                let v: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(v["messages"].as_array().unwrap().len(), 2);
                assert_eq!(v["more"], false);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_backfill_re_seeds_the_live_cursor_from_the_projection_so_the_two_orderings_agree() {
        // **THE LIVE SEQUENCE FALLS BEHIND THE PROJECTION, ONE PER DESK TURN.** The hub takes a
        // cursor per streamed REPLY (`opens_a_row` is `rich://message-started` and nothing else);
        // the projection counts one per MESSAGE. A turn the CEO starts at his Mac therefore adds
        // two rows to the projection and one to the hub, because no live event in this build
        // carries a CEO turn — so a phone reading the stream and the backfill at once is reading
        // two different orderings of one conversation.
        //
        // `hello_frame` already re-seeds. This is the OTHER place the projection is counted, and
        // it is the one a phone reaches after every reply rather than only on a reconnection.
        let f = fixture_with("reseed", false, true, 10);
        assert_eq!(f.channel.hub.cursor_now(), 0, "the fixture's hub starts at zero");

        let out = dispatch(&f.channel, &signed_stream(&f, "thread_id=thr_5c1e&before=8&limit=3"));
        assert!(matches!(out, Outcome::Json { status: 200, .. }), "{out:?}");
        assert_eq!(
            f.channel.hub.cursor_now(),
            10,
            "the backfill read ten rows out of the projection and left the live sequence behind them"
        );

        // `seed_cursor` only ever raises, so a backfill can never drag a live sequence backwards
        // over frames it has already issued.
        f.channel.hub.publish("message", 40, "{}".to_string());
        let out = dispatch(&f.channel, &signed_stream(&f, "thread_id=thr_5c1e&before=8&limit=3"));
        assert!(matches!(out, Outcome::Json { status: 200, .. }), "{out:?}");
        assert_eq!(f.channel.hub.cursor_now(), 40, "a backfill rewound the live cursor");
    }

    #[test]
    fn a_backfill_limit_is_clamped_so_one_request_cannot_ask_for_the_whole_conversation() {
        let f = fixture_with("clamp", false, true, 500);
        match dispatch(&f.channel, &signed_stream(&f, "thread_id=thr_5c1e&before=500&limit=100000")) {
            Outcome::Json { body, .. } => {
                let v: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(v["messages"].as_array().unwrap().len(), 200);
            }
            other => panic!("{other:?}"),
        }
    }

    // --- audio ---------------------------------------------------------------------------------

    #[test]
    fn encoded_projection_ids_resolve_without_becoming_file_paths() {
        let f=fixture("encoded-audio");let wav=f.dir.0.join("reply.wav");std::fs::write(&wav,b"RIFF....WAVE").unwrap();
        f.channel.devices.mint_audio("turn_9:text:0",wav);
        assert_eq!(dispatch(&f.channel,&signed(&f,"GET","/api/audio/turn_9%3Atext%3A0","","")).status(),200);
        for id in ["..%2Freply.wav","turn_9%253Atext%253A0","turn_9%2Ftext"] {
            assert_eq!(dispatch(&f.channel,&signed(&f,"GET",&format!("/api/audio/{id}"),"","")).status(),404);
        }
    }

    #[test]
    fn only_an_audio_id_the_mac_minted_returns_bytes() {
        let f = fixture("audio");
        let wav = f.dir.0.join("reply.wav");
        std::fs::write(&wav, b"RIFF....WAVE").unwrap();
        f.channel.devices.mint_audio("msg_1", wav);

        match dispatch(&f.channel, &signed(&f, "GET", "/api/audio/msg_1", "", "")) {
            Outcome::Bytes { status, content_type, body, .. } => {
                assert_eq!(status, 200);
                assert_eq!(content_type, "audio/wav");
                assert_eq!(body, b"RIFF....WAVE");
            }
            other => panic!("{other:?}"),
        }

        for guess in ["msg_2", "../../etc/passwd", "", "%2e%2e%2f"] {
            let path = format!("/api/audio/{guess}");
            assert_eq!(dispatch(&f.channel, &signed(&f, "GET", &path, "", "")), Outcome::NotFound, "{guess}");
        }
    }

    // --- the retired phone web page --------------------------------------------------------------

    /// **THE PAGE IS RETIRED, AND A BROWSER THAT OPENS THE ADDRESS IS TOLD WHERE TO GO** (CEO,
    /// 2026-10-30). `/` and `/index.html` give the plain page, with no credential; on `main` before
    /// this change they gave the web app's own `index.html`.
    #[test]
    fn a_browser_at_the_old_address_gets_the_plain_answer_and_no_web_app() {
        let f = fixture_with("retired", false, false, 4);
        for path in ["/", "/index.html"] {
            match dispatch(&f.channel, &plain("GET", path)) {
                Outcome::Bytes { status, content_type, body, .. } => {
                    assert_eq!(status, 200, "{path}");
                    assert_eq!(content_type, "text/html; charset=utf-8", "{path}");
                    let page = String::from_utf8(body).unwrap();
                    assert!(page.contains("Use the RichConnect app"), "{path}: {page}");
                    assert!(!page.contains("<script"), "{path} carries a script: {page}");
                }
                other => panic!("{path}: {other:?}"),
            }
        }
    }

    #[test]
    fn every_file_the_web_app_was_made_of_is_a_flat_404() {
        let f = fixture("retired-files");
        for path in [
            "/app.js",
            "/styles.css",
            "/manifest.webmanifest",
            "/lib/api.js",
            "/lib/oxford-comma.js",
            "/icons/icon-192.png",
            "/icons/apple-touch-icon.png",
            "/not-a-file.js",
            "/assets/",
        ] {
            assert_eq!(dispatch(&f.channel, &plain("GET", path)), Outcome::NotFound, "{path} was served");
        }
    }

    /// A phone that installed the page has a service worker serving it from the browser's cache;
    /// its browser checks `/sw.js` for a newer version, and the newer version removes it.
    #[test]
    fn the_old_service_worker_is_replaced_by_one_that_removes_the_page() {
        let f = fixture("retire-worker");
        match dispatch(&f.channel, &plain("GET", "/sw.js")) {
            Outcome::Bytes { status, content_type, body, .. } => {
                assert_eq!(status, 200);
                assert_eq!(content_type, "text/javascript; charset=utf-8");
                let worker = String::from_utf8(body).unwrap();
                for step in ["skipWaiting()", "caches.delete(key)", "registration.unregister()", "client.navigate(client.url)"] {
                    assert!(worker.contains(step), "the worker does not {step}: {worker}");
                }
                assert!(!worker.contains("importScripts"), "it must not load anything from the retired page");
                assert!(!worker.contains("cache.put"), "it must not cache anything");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn nothing_outside_the_two_fixed_answers_can_be_reached() {
        // The probes the embedded-table route was held to, kept: every one is still a 404.
        let f = fixture("traversal");
        let outside = f.dir.0.join("private.txt");
        std::fs::write(&outside, "the CEO's conversation").unwrap();

        for probe in [
            "/../private.txt",
            "/../../etc/passwd",
            "/./index.html",
            "//index.html",
            "//sw.js",
            "/assets/../../private.txt",
            "/lib//api.js",
            &format!("/{}", outside.display()),
        ] {
            assert_eq!(dispatch(&f.channel, &plain("GET", probe)), Outcome::NotFound, "{probe} was served");
        }
    }

    // --- photos and files ------------------------------------------------------------------------

    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F', 0];
    const PDF: &[u8] = b"%PDF-1.7\n1 0 obj\n";

    fn upload(f: &Fixture, client: &str, id: &str, name: &str, media: &str, bytes: &[u8]) -> Incoming {
        let query = format!("kind=attachment&client_id={client}&attachment_id={id}&name={name}");
        let mut request = signed(f, "POST", "/api/messages", &query, bytes);
        request.content_type = Some(media.into());
        request
    }

    fn sha(bytes: &[u8]) -> String {
        super::super::hex(&super::super::sha256(bytes))
    }

    fn json_of(outcome: Outcome) -> (u16, Value) {
        match outcome {
            Outcome::Json { status, body } => (status, serde_json::from_str(&body).unwrap()),
            other => panic!("{other:?}"),
        }
    }

    fn commit_body(client: &str, text: &str, files: &[(&str, &[u8])]) -> String {
        let list: Vec<Value> = files.iter().map(|(id, bytes)| json!({"id": id, "sha256": sha(bytes)})).collect();
        json!({"client_id": client, "thread_id": "thr_5c1e", "kind": "attachments", "text": text,
               "attachments": list, "sent_at": "2026-09-22T20:00:00.000Z"}).to_string()
    }

    /// The Mac restarting: a fresh device desk over the same directory, and a challenge from it.
    fn restart(f: &mut Fixture) {
        f.channel.devices = Arc::new(DeviceDesk::open(&f.dir.0).unwrap());
        f.challenge = f.channel.devices.issue_challenge().unwrap();
    }

    #[test]
    fn a_photo_and_a_pdf_reach_rich_as_one_message_naming_where_each_file_is() {
        let f = fixture("attach-happy");
        let (status, photo) = json_of(dispatch(&f.channel, &upload(&f, "msg-1", "p1", "IMG%200001.HEIC.jpg", "image/jpeg", JPEG)));
        assert_eq!(status, 200, "{photo}");
        assert_eq!(photo["name"], "IMG 0001.HEIC.jpg");
        assert_eq!((photo["size"].as_u64(), photo["duplicate"].as_bool()), (Some(JPEG.len() as u64), Some(false)));
        assert_eq!(photo["sha256"], sha(JPEG));
        assert_eq!(json_of(dispatch(&f.channel, &upload(&f, "msg-1", "d1", "contract.pdf", "application/pdf", PDF))).0, 200);
        // Uploading is not sending: nothing has reached Rich yet.
        assert!(f.bridge.submitted.lock().unwrap().is_empty());

        let body = commit_body("msg-1", "  Here is the contract.  ", &[("p1", JPEG), ("d1", PDF)]);
        let (status, answer) = json_of(dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", &body)));
        assert_eq!(status, 200, "{answer}");
        assert_eq!(answer["message_id"], "msg_new");
        assert_eq!(answer["duplicate"], false);
        assert_eq!(answer["attachments"], json!([
            {"id": "p1", "name": "IMG 0001.HEIC.jpg", "media_type": "image/jpeg", "size": JPEG.len()},
            {"id": "d1", "name": "contract.pdf", "media_type": "application/pdf", "size": PDF.len()},
        ]));
        let submitted = f.bridge.submitted.lock().unwrap();
        assert_eq!(submitted.len(), 1);
        assert_eq!(submitted[0].0.as_deref(), Some("thr_5c1e"));
        let folder = f.dir.0.join("attachments").join("thr_5c1e").join("msg-1");
        assert_eq!(
            submitted[0].1,
            format!(
                "Here is the contract.\n\nAttached from the phone (2 files, saved on this Mac):\n- {} (image/jpeg, {} bytes)\n- {} (application/pdf, {} bytes)",
                folder.join("IMG 0001.HEIC.jpg").display(), JPEG.len(), folder.join("contract.pdf").display(), PDF.len()
            )
        );
        // The file Rich is pointed at holds exactly what the phone sent.
        assert_eq!(std::fs::read(folder.join("contract.pdf")).unwrap(), PDF);
    }

    #[test]
    fn a_retried_commit_after_a_mac_restart_is_one_message_and_a_changed_one_is_refused() {
        let mut f = fixture("attach-replay");
        dispatch(&f.channel, &upload(&f, "msg-2", "p1", "a.jpg", "image/jpeg", JPEG));
        let body = commit_body("msg-2", "", &[("p1", JPEG)]);
        let (_, first) = json_of(dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", &body)));
        restart(&mut f);
        // The answer was lost; the phone sends the identical bytes again after the Mac restarted.
        let (status, again) = json_of(dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", &body)));
        assert_eq!(status, 200);
        assert_eq!(again["duplicate"], true);
        assert_eq!(again["message_id"], first["message_id"]);
        assert_eq!(again["attachments"], first["attachments"]);
        // And a late retry of the UPLOAD is harmless too: it stages a copy that nothing commits.
        assert_eq!(json_of(dispatch(&f.channel, &upload(&f, "msg-2", "p1", "a.jpg", "image/jpeg", JPEG))).0, 200);
        assert_eq!(json_of(dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", &body))).1["duplicate"], true);
        assert_eq!(f.bridge.submitted.lock().unwrap().len(), 1, "one message reached Rich twice");
        let changed = commit_body("msg-2", "different words", &[("p1", JPEG)]);
        assert_eq!(dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", &changed)).status(), 409);
        assert_eq!(f.bridge.submitted.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_retried_upload_after_a_restart_is_recognized_and_a_different_file_under_its_id_is_refused() {
        let mut f = fixture("attach-upload-replay");
        assert_eq!(json_of(dispatch(&f.channel, &upload(&f, "msg-3", "p1", "a.jpg", "image/jpeg", JPEG))).1["duplicate"], false);
        restart(&mut f);
        assert_eq!(json_of(dispatch(&f.channel, &upload(&f, "msg-3", "p1", "a.jpg", "image/jpeg", JPEG))).1["duplicate"], true);
        let mut other = JPEG.to_vec();
        other.push(9);
        let (status, refusal) = json_of(dispatch(&f.channel, &upload(&f, "msg-3", "p1", "a.jpg", "image/jpeg", &other)));
        assert_eq!((status, refusal["retry"].as_bool()), (409, Some(false)));
    }

    #[test]
    fn a_commit_naming_a_file_the_mac_does_not_hold_lists_it_and_reserves_nothing() {
        let f = fixture("attach-missing");
        dispatch(&f.channel, &upload(&f, "msg-4", "p1", "a.jpg", "image/jpeg", JPEG));
        let body = commit_body("msg-4", "two files", &[("p1", JPEG), ("d1", PDF)]);
        let (status, answer) = json_of(dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", &body)));
        assert_eq!(status, 422);
        assert_eq!(answer["missing"], json!(["d1"]));
        assert_eq!(answer["retry"], true);
        assert!(f.bridge.submitted.lock().unwrap().is_empty());
        // The phone uploads the missing one and sends the SAME message bytes: it goes through,
        // because the refusal reserved nothing.
        dispatch(&f.channel, &upload(&f, "msg-4", "d1", "b.pdf", "application/pdf", PDF));
        let (status, answer) = json_of(dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", &body)));
        assert_eq!((status, answer["duplicate"].as_bool()), (200, Some(false)));
        assert_eq!(f.bridge.submitted.lock().unwrap().len(), 1);
    }

    #[test]
    fn files_the_mac_does_not_take_are_refused_with_a_sentence_and_oversize_is_refused_unread() {
        let f = fixture("attach-refuse");
        for (media, bytes) in [("application/zip", b"PK\x03\x04".as_slice()), ("image/jpeg", b"#!/bin/sh".as_slice()), ("application/pdf", b"".as_slice())] {
            let (status, refusal) = json_of(dispatch(&f.channel, &upload(&f, "msg-5", "x1", "f", media, bytes)));
            assert_eq!(status, 422, "{media}");
            assert_eq!(refusal["retry"], false);
            assert!(refusal["reason"].as_str().unwrap().ends_with('.'), "{refusal}");
        }
        assert_eq!(body_limit("POST", "/api/messages", "kind=attachment&client_id=c", Some("image/jpeg")), 26_214_400);
        assert_eq!(upload_seconds("POST", "/api/messages", "kind=attachment", Some("image/jpeg")), 300);
        // The ceilings every other request had are untouched.
        assert_eq!(body_limit("POST", "/api/messages", "", Some("application/json")), MAX_BODY_BYTES);
        assert_eq!(body_limit("POST", "/api/messages", "kind=voice", Some("audio/wav")), super::super::voice::MAX_UPLOAD);
        assert_eq!(body_limit("POST", "/api/pair", "kind=attachment", Some("image/jpeg")), MAX_BODY_BYTES);
        assert_eq!(upload_seconds("POST", "/api/messages", "", Some("audio/wav")), 120);
        assert_eq!(upload_seconds("POST", "/api/messages", "", Some("application/json")), 15);
        let mut oversized = upload(&f, "msg-5", "big", "f.jpg", "image/jpeg", JPEG);
        oversized.body = vec![0xFF; super::super::attachments::MAX_FILE_BYTES + 1];
        assert_eq!(dispatch(&f.channel, &oversized), Outcome::PayloadTooLarge);
    }

    #[test]
    fn malformed_attachment_requests_get_the_same_flat_404_as_everything_else() {
        let f = fixture("attach-malformed");
        // Unsigned, and signed-but-malformed, both look like nothing.
        let mut unsigned = upload(&f, "msg-6", "p1", "a.jpg", "image/jpeg", JPEG);
        unsigned.authorization = None;
        assert_eq!(dispatch(&f.channel, &unsigned), Outcome::NotFound);
        for query in ["kind=attachment&attachment_id=p1", "kind=attachment&client_id=c", "kind=attachment&client_id=c&attachment_id=../p1", "kind=attachment&client_id=c&attachment_id=a.b"] {
            let mut request = signed(&f, "POST", "/api/messages", query, JPEG);
            request.content_type = Some("image/jpeg".into());
            assert_eq!(dispatch(&f.channel, &request), Outcome::NotFound, "{query}");
        }
        let mut no_type = upload(&f, "msg-6", "p1", "a.jpg", "image/jpeg", JPEG);
        no_type.content_type = None;
        assert_eq!(dispatch(&f.channel, &no_type), Outcome::NotFound);
        let long_name = upload(&f, "msg-6", "p1", &"n".repeat(1025), "image/jpeg", JPEG);
        assert_eq!(dispatch(&f.channel, &long_name), Outcome::NotFound);
        let sha = sha(JPEG);
        for body in [
            json!({"client_id":"m","thread_id":"thr_5c1e","kind":"attachments","attachments":[]}),
            json!({"client_id":"m","thread_id":"thr_5c1e","kind":"attachments"}),
            json!({"client_id":"m","thread_id":"thr_other","kind":"attachments","attachments":[{"id":"p1","sha256":sha}]}),
            json!({"client_id":"m","kind":"attachments","attachments":[{"id":"p1","sha256":sha}]}),
            json!({"client_id":"m","thread_id":"thr_5c1e","kind":"attachments","attachments":[{"id":"p1","sha256":sha.to_uppercase()}]}),
            json!({"client_id":"m","thread_id":"thr_5c1e","kind":"attachments","attachments":[{"id":"p1","sha256":sha},{"id":"p1","sha256":sha}]}),
            json!({"client_id":"m","thread_id":"thr_5c1e","kind":"attachments","text":7,"attachments":[{"id":"p1","sha256":sha}]}),
            json!({"client_id":"m","thread_id":"thr_5c1e","kind":"attachments","attachments":(0..11).map(|n| json!({"id":format!("f{n}"),"sha256":sha})).collect::<Vec<_>>()}),
        ] {
            assert_eq!(dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", body.to_string())), Outcome::NotFound, "{body}");
        }
        assert!(f.bridge.submitted.lock().unwrap().is_empty());
    }

    #[test]
    fn the_text_route_is_unchanged_by_attachments() {
        // A text message that happens to carry an `attachments` key is still a text message, as
        // it was before this route existed: the key selects nothing unless `kind` says so.
        let f = fixture("attach-text");
        let body = r#"{"client_id":"t1","thread_id":"thr_5c1e","kind":"text","text":"plain words","attachments":[{"id":"p1"}]}"#;
        assert_eq!(dispatch(&f.channel, &signed(&f, "POST", "/api/messages", "", body)).status(), 200);
        assert_eq!(f.bridge.submitted.lock().unwrap()[0].1, "plain words");
    }

    #[test]
    fn attachments_are_offered_exactly_when_the_route_would_take_one() {
        // The same agreement `voice_is_offered_exactly_when_the_route_would_take_one` holds for
        // voice: ASK the route, read whether it took the file, and compare with what is advertised.
        let f = fixture("attach-capability");
        let taken = dispatch(&f.channel, &upload(&f, "msg-7", "p1", "a.jpg", "image/jpeg", JPEG)).status() == 200;
        assert_eq!(capabilities(false, false).contains(&"attachments"), taken);
        // And the old Mac's list is still a prefix of the new one in every combination.
        // `pair-v2` (Sage §3.5) is appended, never inserted, like every addition before it, and
        // so is `pair-wait` (Sage's pair-v2 hypotheses review §1).
        assert_eq!(capabilities(false, false), ["text", "attachments", "pair-v2", "pair-wait"]);
        assert_eq!(capabilities(true, true), ["text", "voice", "audio", "native-push", "attachments", "native-push-fcm", "pair-v2", "pair-wait"]);
    }

    /// **SAGE §3.5: `pair-v2` IS ANNOUNCED IN BOTH PLACES A PHONE READS IT**, and the pairing answer
    /// says which derivation this Mac uses. A v2 phone refuses a Mac without it rather than falling
    /// back, so a Mac that stopped announcing it would stop pairing every v2 phone — this holds it.
    /// On `main` before this change neither place named it.
    #[test]
    fn pair_v2_is_announced_in_the_pairing_answer_and_in_every_hello_and_recorded_from_the_phone() {
        let f = fixture_with("pair-v2", false, false, 4);
        let window = f.channel.devices.open_pairing().unwrap();
        let mut request = plain("POST", "/api/pair");
        request.body = json!({"code": window.code, "public_key_jwk": Phone::new().jwk(), "device_name": "Pixel", "platform": "android", "pairing_version": 2}).to_string().into_bytes();
        let (status, v) = json_of(dispatch(&f.channel, &request));
        assert_eq!(status, 200);
        assert!(v["capabilities"].as_array().unwrap().contains(&json!("pair-v2")), "{v}");
        // And the hold beside it, in the answer a phone reads before its first "They match".
        assert!(v["capabilities"].as_array().unwrap().contains(&json!("pair-wait")), "{v}");
        assert_eq!(v["pairing_version"], 2);
        assert_eq!(f.channel.devices.paired().unwrap().pairing_version, 2, "the phone's announcement was not recorded");

        // A phone that says nothing, or anything but the number 2, is a v1 phone.
        for said in [json!(null), json!("2"), json!(3), json!(1)] {
            let f = fixture_with("pair-v1", false, false, 4);
            let window = f.channel.devices.open_pairing().unwrap();
            let mut request = plain("POST", "/api/pair");
            request.body = json!({"code": window.code, "public_key_jwk": Phone::new().jwk(), "pairing_version": said}).to_string().into_bytes();
            assert_eq!(dispatch(&f.channel, &request).status(), 200);
            assert_eq!(f.channel.devices.paired().unwrap().pairing_version, 1, "{said}");
        }
    }

    #[test]
    fn the_pairing_answer_tells_a_new_app_what_this_mac_supports() {
        let f = fixture_with("attach-pair", false, false, 4);
        let window = f.channel.devices.open_pairing().unwrap();
        let mut request = plain("POST", "/api/pair");
        request.body = json!({"code": window.code, "public_key_jwk": Phone::new().jwk(), "device_name": "Pixel", "platform": "android"}).to_string().into_bytes();
        let (status, v) = json_of(dispatch(&f.channel, &request));
        assert_eq!(status, 200);
        assert_eq!(v["protocol_version"], 1);
        assert_eq!(v["capabilities"], json!(capabilities(false, false)));
        assert_eq!(v["attachment_limits"]["max_files_per_message"], 10);
        assert_eq!(v["build"], BUILD);
    }

    #[test]
    fn an_android_confirmation_records_fcm_and_the_iphone_one_still_records_apns() {
        let f = fixture("attach-transport");
        for (sent, recorded) in [("fcm", "fcm"), ("apns", "apns"), ("carrier-pigeon", "apns")] {
            let body = json!({"fingerprint_confirmed": true, "push_transport": sent}).to_string();
            assert_eq!(dispatch(&f.channel, &signed(&f, "POST", "/api/pair", "", &body)).status(), 200);
            assert_eq!(f.channel.devices.paired().unwrap().push_transport, recorded, "{sent}");
        }
    }

    // --- how long a voice note was ---------------------------------------------------------------

    /// A bridge whose projection holds the turn a voice note became (`t9:user`), a typed turn
    /// (`t8:user`) and a desktop voice-mode turn (`t7:user`, source `jam`), and whose ledger links
    /// `intake_42` to `t9` only once `drained` is set — the order the real drain produces.
    struct VoiceHistory { drained: Mutex<bool> }
    impl Bridge for VoiceHistory {
        fn voice_available(&self) -> bool { true }
        fn transcribe(&self, bytes: &[u8]) -> Result<String, String> { super::super::voice::validate(bytes)?; Ok("A spoken request".into()) }
        fn submit_text(&self, thread_id: Option<&str>, _text: &str, _client_id: &str) -> Result<Accepted, String> {
            Ok(Accepted { message_id: "intake_42".into(), thread_id: thread_id.unwrap_or("thr_5c1e").into(), at: 1_758_200_000_000 })
        }
        fn turn_for_intake(&self, message_id: &str) -> Option<String> {
            (*self.drained.lock().unwrap() && message_id == "intake_42").then(|| "t9".to_string())
        }
        fn snapshot(&self, _thread_id: Option<&str>) -> Result<Value, String> {
            let item = |id: &str, source: &str, text: &str, at: u64| json!({"kind":"user_message","id":id,"threadId":"thr_5c1e","turnId":"t",
                "createdAt":at,"text":text,"source":source,"slot":"opening","visibility":"ceo","entityId":"femcboost","bindingRevision":1});
            Ok(json!({"items":[item("t7:user","jam","said at the desk",1),item("t8:user","text","typed",2),item("t9:user","text","A spoken request",3)]}))
        }
        fn current_thread(&self) -> Option<(String, String)> { Some(("thr_5c1e".into(), "the proposal".into())) }
        fn threads(&self) -> Vec<(String, String)> { vec![("thr_5c1e".into(), "the proposal".into())] }
    }

    /// A bridge over a real `rich_files` record: one turn in which Rich attached a PDF before he
    /// wrote his reply, as the tool is called while he works.
    struct FileBridge { root: std::path::PathBuf }
    impl Bridge for FileBridge {
        fn files_available(&self) -> bool { true }
        fn rich_file(&self, thread: &str, id: &str) -> Result<(String, Vec<u8>), richos_core::rich_files::Fetch> {
            richos_core::rich_files::read_for_phone(&self.root, thread, id).map(|(row, body)| (row.name, body))
        }
        fn submit_text(&self, _t: Option<&str>, _text: &str, _c: &str) -> Result<Accepted, String> { Err("not here".into()) }
        fn snapshot(&self, _thread_id: Option<&str>) -> Result<Value, String> {
            let mut items = vec![
                json!({"kind":"user_message","id":"t1:user","threadId":"thr_5c1e","turnId":"t1","createdAt":1,"text":"send me the counter","source":"text","visibility":"ceo"}),
            ];
            items.extend(richos_core::rich_files::items(&self.root, "femcboost", "thr_5c1e"));
            items.push(json!({"kind":"rich_message","id":"t1:text:0","threadId":"thr_5c1e","turnId":"t1","createdAt":u64::MAX,"text":"Here it is.","visibility":"ceo"}));
            Ok(json!({"items": items}))
        }
        fn current_thread(&self) -> Option<(String, String)> { Some(("thr_5c1e".into(), "the proposal".into())) }
        fn threads(&self) -> Vec<(String, String)> { vec![("thr_5c1e".into(), "the proposal".into())] }
    }

    /// **A FILE RICH ATTACHED REACHES THE PHONE** (round 36 screen 3): a `file` row under his
    /// reply in `hello`, `files` among the capabilities, and the bytes on a signed tap — from this
    /// conversation's record only, with the gone state said rather than a flat 404.
    #[test]
    fn a_file_rich_attached_is_a_row_under_his_reply_and_comes_over_on_a_signed_tap() {
        let mut f = fixture("rich-file");
        let root = f.dir.0.join("engine-state");
        let file = f.dir.0.join("Acme-counteroffer.pdf");
        std::fs::write(&file, b"%PDF-1.4 counter").unwrap();
        let row = richos_core::rich_files::attach(&root, "femcboost", "thr_5c1e", "t1", "front_desk", &file.to_string_lossy()).unwrap();
        f.channel.bridge = Arc::new(FileBridge { root: root.clone() });

        let data = match dispatch(&f.channel, &signed_stream(&f, "thread_id=thr_5c1e")) {
            Outcome::Stream { opening, .. } => serde_json::from_str::<Value>(opening[0].split("data: ").nth(1).unwrap().trim_end()).unwrap(),
            other => panic!("{other:?}"),
        };
        assert!(data["capabilities"].as_array().unwrap().contains(&json!("files")), "{}", data["capabilities"]);
        let rows = data["messages"].as_array().unwrap();
        let kinds: Vec<&str> = rows.iter().map(|r| r["kind"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["text", "text", "file"], "the file goes under his words: {rows:?}");
        assert_eq!(rows[2]["role"], "rich");
        assert_eq!(rows[2]["file"]["id"], row.id.as_str());
        assert_eq!(rows[2]["file"]["name"], "Acme-counteroffer.pdf");
        assert!(rows[2]["file"].get("path").is_none(), "the phone is handed an id, never a path");

        let id = row.id.clone();
        let query = format!("kind=file&file_id={id}&thread_id=thr_5c1e");
        match dispatch(&f.channel, &signed(&f, "GET", "/api/messages", &query, b"")) {
            Outcome::Bytes { status, content_type, body, .. } => {
                assert_eq!((status, content_type.as_str(), body.as_slice()), (200, "application/pdf", &b"%PDF-1.4 counter"[..]));
            }
            other => panic!("{other:?}"),
        }
        // Unsigned, another conversation, an id nobody attached, or no `kind`: the flat 404.
        assert_eq!(dispatch(&f.channel, &plain("GET", "/api/messages")), Outcome::NotFound);
        for q in [format!("kind=file&file_id={id}&thread_id=thr_other"), "kind=file&file_id=rf_00&thread_id=thr_5c1e".into(), format!("file_id={id}&thread_id=thr_5c1e")] {
            assert_eq!(dispatch(&f.channel, &signed(&f, "GET", "/api/messages", &q, b"")), Outcome::NotFound, "{q}");
        }
        // Gone from the Mac: said, so the phone can say it.
        std::fs::remove_file(&file).unwrap();
        let (status, answer) = json_of(dispatch(&f.channel, &signed(&f, "GET", "/api/messages", &query, b"")));
        assert_eq!((status, answer["reason"].as_str()), (410, Some("No longer on this Mac. It was moved or deleted.")));
    }

    #[test]
    fn a_voice_notes_length_is_in_its_answer_and_on_its_own_row_in_hello_and_backfill() {
        let mut f = fixture("voice-duration");
        let bridge = Arc::new(VoiceHistory { drained: Mutex::new(false) });
        f.channel.bridge = bridge.clone();
        // 8.25 s of 16 kHz mono: 132,000 samples -> 132,000 x 1000 / 16,000 = 8,250 ms.
        let wav = richos_voice::wav::encode_pcm16_mono(&vec![0.1; 132_000], 16000);
        let query = "client_id=spoken-9&thread_id=thr_5c1e&kind=voice&codec=wav16k&sample_rate=16000&seconds=8";
        let mut request = signed(&f, "POST", "/api/messages", query, &wav);
        request.content_type = Some("audio/wav".into());
        let (status, answer) = json_of(dispatch(&f.channel, &request));
        assert_eq!((status, answer["duration_ms"].as_u64()), (200, Some(8_250)), "{answer}");
        // A retry answers from the receipt, length included.
        assert_eq!(json_of(dispatch(&f.channel, &request)).1["duration_ms"], 8_250);

        let hello_rows = |f: &Fixture| match dispatch(&f.channel, &signed_stream(f, "thread_id=thr_5c1e")) {
            Outcome::Stream { opening, .. } => {
                let data: Value = serde_json::from_str(opening[0].split("data: ").nth(1).unwrap().trim_end()).unwrap();
                data["messages"].as_array().unwrap().clone()
            }
            other => panic!("{other:?}"),
        };
        // Before the drain there is no turn to join to, and nothing is guessed.
        assert!(hello_rows(&f).iter().all(|r| r.get("duration_ms").is_none()));
        *bridge.drained.lock().unwrap() = true;
        let rows = hello_rows(&f);
        assert_eq!(rows[2]["id"], "t9:user");
        assert_eq!(rows[2]["duration_ms"], 8_250);
        assert_eq!(rows[2]["kind"], "text", "a voice note's row keeps the kind the preserved clients know");
        assert!(rows[0].get("duration_ms").is_none(), "a desktop voice-mode turn has no recording length");
        assert!(rows[1].get("duration_ms").is_none(), "a typed message got a length");

        // The backfill says the same, and so does a Mac that restarted in between.
        restart(&mut f);
        let path = "/api/events?thread_id=thr_5c1e&before=100&limit=10";
        let message = signing_string(&f.challenge, "GET", path, b"");
        let auth = format!("RichOS-Device {}.{}.{}", f.device_id, f.challenge, super::super::b64url(&f.phone.sign(&message))).replace(' ', "%20");
        let (status, page) = json_of(dispatch(&f.channel, &Incoming { method: "GET".into(), path: "/api/events".into(),
            query: format!("thread_id=thr_5c1e&before=100&limit=10&auth={auth}"), authorization: None, last_event_id: None, content_type: None, body: Vec::new() }));
        assert_eq!(status, 200);
        let durations: Vec<Option<u64>> = page["messages"].as_array().unwrap().iter().map(|r| r["duration_ms"].as_u64()).collect();
        assert_eq!(durations, [None, None, Some(8_250)]);
    }

    // --- the trust endpoint: REMOVED, CEO §61 -------------------------------------------------
    //
    // `the_trust_endpoint_serves_exactly_one_file_and_404s_everything_else` proved plan §2.1
    // item 4: a plain-HTTP route on the neighboring port answering `GET /ca` with an Apple
    // `.mobileconfig` and 404ing six other shapes. The route, the port and the profile all went
    // when §61 removed the path that had a phone install anything — see the note where
    // `dispatch_trust` used to be, and `listen.rs`'s end-to-end walk, which now asserts that the
    // channel binds one socket rather than that the second one behaves.
    #[test]
    fn question_answers_use_signed_messages_and_authoritative_revisions() {
        use richos_core::questions::{Store,AskScope,QuestionInput,OptionInput};
        let mut f=fixture("question-answers");let store=Store::new(&f.dir.0);
        let scope=AskScope{obligation_id:None,root:f.dir.0.clone(),entity_id:"femcboost".into(),thread_id:"thr_5c1e".into(),turn_id:"ask".into(),asker:"front_desk".into(),session_id:"session".into(),engine:None,entity_root:None,app_run:None};
        let q=store.ask(&scope,vec![QuestionInput{text:"When should the release ship?".into(),options:vec![OptionInput{label:"Today".into(),description:"Earlier fixes".into()},OptionInput{label:"Tomorrow".into(),description:"More tests".into()}],multiple:false,free_answer:true,recommended:None}]).unwrap().remove(0);
        f.channel.bridge=Arc::new(FakeBridge{submitted:Mutex::new(vec![]),client_ids:Mutex::new(vec![]),refuse:false,voice:false,rows:0,questions:Some(store.clone()),oxford:false});
        let body=json!({"kind":"answer","client_id":"phone-answer","thread_id":"thr_5c1e","question_id":q.id,"option_ids":[q.options[0].id]}).to_string();
        let signed_request=signed(&f,"POST","/api/messages","",&body);
        let mut unsigned=signed(&f,"POST","/api/messages","",&body);unsigned.authorization=None;
        assert_ne!(dispatch(&f.channel,&unsigned).status(),200);
        let (status,result)=json_of(dispatch(&f.channel,&signed_request));assert_eq!(status,200);assert_eq!(result["outcome"],"accepted");assert_eq!(result["question"]["revision"],1);
        assert!(result["question"].get("session_id").is_none());
        assert_eq!(json_of(dispatch(&f.channel,&signed_request)).1["question"]["revision"],1);
        let edit=json!({"kind":"answer","client_id":"edit","thread_id":"thr_5c1e","question_id":q.id,"option_ids":[q.options[1].id],"expected_revision":1}).to_string();
        assert_eq!(json_of(dispatch(&f.channel,&signed(&f,"POST","/api/messages","",edit))).1["question"]["revision"],2);
        let wrong=json!({"kind":"answer","client_id":"wrong","thread_id":"another-thread","question_id":q.id,"option_ids":[q.options[0].id]}).to_string();
        assert_eq!(dispatch(&f.channel,&signed(&f,"POST","/api/messages","",wrong)).status(),422);
        let shown=json!({"kind":"question_seen","client_id":"seen","thread_id":"thr_5c1e","question_id":q.id}).to_string();
        assert_eq!(dispatch(&f.channel,&signed(&f,"POST","/api/messages","",shown)).status(),200);
        assert_eq!(store.all().unwrap()[0].shown.as_ref().unwrap().surface,"phone");
        store.close("femcboost","thr_5c1e",None,"Deleted",true).unwrap();
        assert_eq!(json_of(dispatch(&f.channel,&signed_request)).1["outcome"],"conversation_deleted");
        assert!(store.pending_threads().unwrap().is_empty());
    }

}
