//! **THE TAILSCALE PATH — a name the whole internet already trusts, or an honest "not yet".**
//!
//! CEO decision §61, verbatim: *"The RichOS desktop app will guide technical users (with the help
//! of crystal-clear and ultra-simple how-to screens in the app) to their Tailscale setup. That's
//! it. The Tailscale setup is where we start now."*
//!
//! The reach document's §2.3 is what this implements: *"You install Tailscale on the Mac and on
//! the phone and sign in to both with the same free account … the Mac gets a name with a
//! **publicly trusted** certificate — which would delete the sixteen taps as a side effect."*
//!
//! # This module never decides anything. It reports a state.
//!
//! Tailscale is the **user's** software, signed into the **user's** account. Every question this
//! module can ask has an answer that is not ours to change: not installed, not signed in, signed
//! in but switched off, signed in but the tailnet has not been told to issue certificates. So the
//! whole of this file is one function that turns `tailscale status --json` into one of eight
//! named states, each with a sentence HE can read. **Nothing here installs, enables, signs in or
//! retries.** §2.3's own failure mode — *"when Tailscale is off or logged out, the name stops
//! resolving including at home"* — is a state on this list, not an error.
//!
//! # What was measured, and what was NOT
//!
//! **Tailscale is not installed on this Mac.** Checked on 2026-09-18 eight ways: no
//! `/Applications/Tailscale.app`, no `tailscale` on `PATH`, nothing at `/usr/local/bin`,
//! `/opt/homebrew/bin` or inside a bundle's `Contents/MacOS`, no `~/Library/Containers/io.tailscale*`,
//! no `/var/run/tailscale*` socket, nothing in `launchctl list`, not in `brew list`, and no
//! `ts.net` resolver in `scutil --dns`. So **no output in this file was captured from a running
//! daemon**, and saying otherwise would be the exact failure the house rule about numbers is
//! about. Raised as `esc-20260918T204405Z-e25d25de`.
//!
//! What the fixtures ARE pinned to is the upstream **type declarations**, quoted verbatim from
//! `tailscale/tailscale` `main`:
//!
//! * `ipn/backend.go` — `type State int`, with `stateStrings = [...]string{"NoState",
//!   "InUseOtherUser", "NeedsLogin", "NeedsMachineAuth", "Stopped", "Starting", "Running"}`.
//!   **Seven**, and `InUseOtherUser` is the one every third-party wrapper forgets.
//! * `ipn/ipnstate/ipnstate.go` — `BackendState string` (*"is an ipn.State string value"*),
//!   `Self *PeerStatus`, and `CertDomains []string`: *"the set of DNS names for which the control
//!   plane server will assist with provisioning TLS certificates … These names are FQDNs without
//!   trailing periods."*
//! * `ipn/ipnstate/ipnstate.go` — `PeerStatus.DNSName`: *"DNSName is the Peer's FQDN. **It ends
//!   with a dot.** It has the form `host.<MagicDNSSuffix>.`"* That trailing dot is why
//!   [`origin_for`] exists rather than a `format!` at the call site.
//! * `cmd/tailscale/cli/cert.go` — `"tailscale cert [flags] <domain>"`, `--cert-file` and
//!   `--key-file` each documented as *"output cert file or `-` for stdout"*, and the two refusals
//!   this module classifies: *"HTTPS cert support is not enabled/configured for your tailnet."*
//!   and *"Tailscale is not running."*
//!
//! So the PARSER is pinned to the source of truth; the SHAPE OF A REAL RUN on this machine is
//! `unverified:` and settled by one `tailscale status --json` once it is installed. The parser is
//! deliberately written to ignore every field it was not told about, so a real daemon's much
//! larger document cannot break it by being larger.
//!
//! # Raw output is never logged, and that is borrowed rather than invented
//!
//! `richos-hq/docs/research/t3code-mobile-vs-richos-phone-2026-09-18.md` idea 7, reading T3 Code's
//! own wrapper: they classify this binary's stderr into a closed label set *"because stderr can
//! contain auth keys (`tskey-…`) and node names, and these labels are logged."* [`Diagnostic`] is
//! that closed set for us. **The raw text does not leave this module.**

use super::{PhoneError, HTTPS_PORT};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where a `tailscale` command line lives on macOS, best first.
///
/// Three install shapes, and they are not interchangeable
/// (<https://tailscale.com/docs/concepts/macos-variants>):
///
/// 1. **Standalone** (system extension) — installs a launcher at `/usr/local/bin/tailscale`.
/// 2. **Mac App Store** (network extension, sandboxed) — the CLI is *inside the bundle*, run as
///    `/Applications/Tailscale.app/Contents/MacOS/Tailscale`. Funnel and the SSH server are
///    documented as unavailable in this variant; **port-serving and `cert` are not on that list**,
///    which is why the path this module takes works on all three.
/// 3. **Open-source `tailscaled`** — Homebrew, so `/opt/homebrew/bin` on Apple silicon and
///    `/usr/local/bin` on Intel.
///
/// `PATH` is consulted LAST rather than first, deliberately: a `tailscale` earlier on someone's
/// `PATH` is far more likely to be a shell function, a wrapper or a shim than the real client, and
/// the three absolute paths above are the ones the vendor documents.
const CLI_CANDIDATES: [&str; 4] = [
    "/usr/local/bin/tailscale",
    "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    "/opt/homebrew/bin/tailscale",
    "/usr/bin/tailscale",
];

/// **The closed label set.** Nothing else about a failed command is ever recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Diagnostic {
    /// No command line was found at any of the documented places.
    NotInstalled,
    /// The command ran and said the daemon is not up — `"Tailscale is not running."`
    DaemonUnreachable,
    /// `"HTTPS cert support is not enabled/configured for your tailnet."` A switch in the user's
    /// own Tailscale admin console, which nothing on this Mac can throw for him.
    HttpsNotEnabled,
    /// Ran, exited non-zero, and said something this module does not recognize. **The something
    /// is not kept**, because it can carry a `tskey-…` or a node name.
    Refused,
    /// The command could not be executed at all, or its output was not the shape this build reads.
    Unreadable,
}

impl Diagnostic {
    /// The one sentence that is safe to put in the Mac's own log.
    pub fn label(&self) -> &'static str {
        match self {
            Diagnostic::NotInstalled => "tailscale: not installed",
            Diagnostic::DaemonUnreachable => "tailscale: not running",
            Diagnostic::HttpsNotEnabled => "tailscale: certificates not enabled for this tailnet",
            Diagnostic::Refused => "tailscale: refused",
            Diagnostic::Unreadable => "tailscale: unreadable answer",
        }
    }

    /// Classify a command's stderr **without keeping it**, lowercased so a capitalization change
    /// upstream is not a silent reclassification.
    ///
    /// # The third pattern was measured, not read
    ///
    /// The first two come from `cmd/tailscale/cli/cert.go`'s own strings. The third does not exist
    /// in that file at all. **Running `tailscale cert` on this Mac, against a real signed-in
    /// account with certificates not yet enabled, returned:**
    ///
    /// ```text
    /// 500 Internal Server Error: your Tailscale account does not support getting TLS certs
    /// ```
    ///
    /// That is the **control plane's** sentence relayed through the CLI, not the CLI's own, so
    /// reading `cert.go` could never have found it — and it is the single most likely refusal a
    /// real user meets, because it is what a brand-new free tailnet says before the admin-console
    /// switch is thrown. Without this arm it classified as [`Refused`](Diagnostic::Refused), and
    /// the screen would have said "something went wrong" instead of "turn on HTTPS certificates".
    pub fn classify(stderr: &str) -> Diagnostic {
        let text = stderr.to_lowercase();
        if text.contains("not running") {
            Diagnostic::DaemonUnreachable
        } else if text.contains("not enabled")
            || text.contains("not configured")
            // Measured on this Mac, 2026-09-18. See the note above.
            || text.contains("does not support getting tls cert")
        {
            Diagnostic::HttpsNotEnabled
        } else {
            Diagnostic::Refused
        }
    }
}

/// **Where the user is on the Tailscale path.** Eight states, and every one of them is somewhere a
/// real person can actually be sitting.
///
/// The brief named four — *absent / installed not signed in / signed in, name known / serving*.
/// Building it turned up more that a how-to screen has to be able to show, and each one is a
/// different thing for the user to DO:
///
/// * [`Stopped`](TailnetState::Stopped) — signed in, switched off. This is §2.3's named failure
///   mode *"including at home"*, and the fix is one click in his menu bar, not a sign-in.
/// * [`NeedsApproval`](TailnetState::NeedsApproval) — `NeedsMachineAuth`. Signed in correctly and
///   waiting for a tailnet admin to approve the device. Telling him to sign in again would be
///   wrong advice.
/// * [`CertificatesOff`](TailnetState::CertificatesOff) — **the important one.** Signed in, online,
///   name known, and `CertDomains` is empty, which is the control plane saying it will not issue a
///   certificate for this tailnet. HTTPS certificates are a switch in the Tailscale admin console
///   on the web, and **until it is on, the publicly trusted certificate this whole path exists for
///   does not exist.** A screen that says "you're all set" here would be lying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TailnetState {
    /// No Tailscale on this Mac.
    Absent,
    /// Installed, but the daemon did not answer.
    NotRunning,
    /// Installed and running, and nobody is signed in.
    NeedsSignIn,
    /// Signed in, waiting for an administrator to approve this machine.
    NeedsApproval,
    /// Signed in and deliberately switched off. The name does not resolve, including at home.
    Stopped,
    /// Another user account on this Mac is using Tailscale. `ipn.State`'s `InUseOtherUser`.
    InUseByAnotherUser,
    /// Signed in and up, but the tailnet will not be issued certificates.
    CertificatesOff { name: String, account: Option<Account>, phone: Option<PhonePeer> },
    /// Everything this path needs: a name, and a control plane that will certify it.
    Ready {
        name: String,
        addresses: Vec<IpAddr>,
        account: Option<Account>,
        phone: Option<PhonePeer>,
    },
}

/// **A phone on this tailnet, and whether Tailscale is actually switched on on it.**
///
/// Read off this Mac's own daemon on 2026-09-19, after the CEO signed in again: his Android
/// appears in `Peer` **twice** — one stale registration per reinstall — and **both entries are
/// `"Online": false`, because Tailscale is installed on the phone and switched off.** Three facts
/// follow, and each one is a sentence the screen would otherwise get wrong:
///
/// 1. **Presence is not reachability.** A peer with `Online: false` is a node the control plane
///    cannot see; the phone will not answer at this Mac's name. Telling that user "your phone is
///    on this network" and leaving them to work out why nothing happens is the same dead end this
///    whole screen exists to close.
/// 2. **Duplicates are one device.** Reinstalling leaves the old node behind, and the CEO
///    reinstalled three times. Counting registrations would report three phones where there is one.
/// 3. **Off is a different instruction from wrong-identity.** The fix for an offline phone is one
///    switch in the Tailscale app; the fix for a mismatched identity is a sign-out and a sign-in.
///    One sentence for both would send half the users to the wrong one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhonePeer {
    /// What the daemon calls it — `HostName`, which is what the user's phone calls itself.
    pub name: String,
    /// `Online`, documented upstream as *"whether node is connected to the control plane"*.
    /// A device that has ever signed in is in `Peer` forever; this is what says it is there NOW.
    pub online: bool,
}

/// **WHICH account this Mac is signed in to, in the words the user will see on their phone.**
///
/// CEO, from his own live attempt: he signed in with **Apple** on the Mac and with **Google** on
/// the Android, *"and had no way to know they were different networks, or which to reuse."* Two
/// providers make two tailnets, the devices never see each other, and from the phone it looks like
/// "cannot connect" with nothing naming the cause — Sage §2.3's silent failure, arrived at by
/// following the instructions.
///
/// A screen that says *"use the same account"* cannot prevent that, because the user does not know
/// which one they used. A screen that says *"sign in with Apple as alex@icloud.com"* can. So this
/// is read off the Mac and put INTO the instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The login name exactly as the daemon reports it — normally an email address.
    pub login_name: String,
    /// `"Apple"`, `"Google"`, `"GitHub"`, `"Microsoft"` — or `None` when it cannot be known.
    pub provider: Option<&'static str>,
}

impl Account {
    /// **The provider, inferred from the login domain, because the JSON does not name it.**
    ///
    /// `tailscale status --json` reports a `LoginName` and no provider field, so the domain is the
    /// only evidence there is. Each arm below is a domain the provider actually owns.
    ///
    /// **`None` is a real answer and it is the common one.** A Google Workspace account logs in as
    /// `someone@their-own-company.com`, and a self-hosted or SSO login can be any domain at all —
    /// so a guess would be wrong precisely for the users most likely to have two identities. When
    /// it is `None` the screen shows the address alone, which is still the thing the CEO could not
    /// get: *which* account to reuse.
    pub fn infer_provider(login_name: &str) -> Option<&'static str> {
        let domain = login_name.rsplit('@').next()?.to_lowercase();
        // GitHub logins do not always carry an `@` at all, so it is matched on the whole string.
        let whole = login_name.to_lowercase();
        if whole.contains("github") {
            return Some("GitHub");
        }
        match domain.as_str() {
            "icloud.com" | "me.com" | "mac.com" => Some("Apple"),
            "gmail.com" | "googlemail.com" => Some("Google"),
            "outlook.com" | "hotmail.com" | "live.com" | "msn.com" | "microsoft.com" => {
                Some("Microsoft")
            }
            _ => None,
        }
    }

    /// *"Apple as alex@icloud.com"*, or just the address when the provider cannot be known.
    /// One phrase, so the Mac's screen and the phone's screen cannot word it differently.
    pub fn described(&self) -> String {
        match self.provider {
            Some(provider) => format!("{provider} as {}", self.login_name),
            None => self.login_name.clone(),
        }
    }
}

impl TailnetState {
    /// The machine's tailnet name, when there is one — lowercase, no trailing dot.
    pub fn name(&self) -> Option<&str> {
        match self {
            TailnetState::CertificatesOff { name, .. } | TailnetState::Ready { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Which account this Mac is signed in to, when it is signed in to one.
    pub fn account(&self) -> Option<&Account> {
        match self {
            TailnetState::CertificatesOff { account, .. }
            | TailnetState::Ready { account, .. } => account.as_ref(),
            _ => None,
        }
    }

    /// **The name of a phone already on this tailnet, if one is.**
    ///
    /// The CEO's estimate is that the mismatched-account failure hits **9 in 10 users**, so it is
    /// DETECTED rather than only warned about: a phone signed in to the same account appears in
    /// the daemon's own `Peer` map with an `OS` of `iOS` or `android`. Its presence is the one
    /// piece of evidence that the two devices really are on one network — which is exactly the
    /// thing no amount of screen copy can assert on its own.
    pub fn phone(&self) -> Option<&PhonePeer> {
        match self {
            TailnetState::CertificatesOff { phone, .. } | TailnetState::Ready { phone, .. } => {
                phone.as_ref()
            }
            _ => None,
        }
    }

    /// Every address the daemon says this node answers on, for the bind list and the leaf's SANs.
    pub fn addresses(&self) -> &[IpAddr] {
        match self {
            TailnetState::Ready { addresses, .. } => addresses,
            _ => &[],
        }
    }

    /// The origin the phone would pair with, and **only when it would actually work.**
    ///
    /// `CertificatesOff` deliberately answers `None` even though it holds a perfectly good name.
    /// Offering that origin would produce a certificate warning on the phone at the exact moment
    /// this path's whole promise is "no certificate warning" — and per `api_base.rs`,
    /// *"it never guesses an address and never promises a reach it has not tested."*
    pub fn origin(&self) -> Option<String> {
        match self {
            TailnetState::Ready { name, .. } => Some(origin_for(name)),
            _ => None,
        }
    }

    /// A short, stable token for the screen and for the log. Never a sentence, never translated.
    pub fn token(&self) -> &'static str {
        match self {
            TailnetState::Absent => "absent",
            TailnetState::NotRunning => "not-running",
            TailnetState::NeedsSignIn => "needs-sign-in",
            TailnetState::NeedsApproval => "needs-approval",
            TailnetState::Stopped => "stopped",
            TailnetState::InUseByAnotherUser => "other-user",
            TailnetState::CertificatesOff { .. } => "certificates-off",
            TailnetState::Ready { .. } => "ready",
        }
    }

    /// **What HE reads.** One sentence per state, each one naming the next thing to do and who has
    /// to do it, in American English, and each one true when spoken aloud as well as read — the
    /// whole app is voice-first, so a sentence that only works on a screen is half a sentence.
    ///
    /// These are a floor for Urban's how-to screens rather than a replacement for them: a screen
    /// can say more, and must never say less or something different.
    pub fn sentence(&self) -> &'static str {
        match self {
            TailnetState::Absent => {
                "Tailscale is not on this Mac. Installing it here and on your phone and signing \
                 in to the same free account on both, is what lets you reach me when you are away \
                 from your desk."
            }
            TailnetState::NotRunning => {
                "Tailscale is on this Mac but it is not running right now. Open it and I will \
                 pick it up."
            }
            TailnetState::NeedsSignIn => {
                "Tailscale is on this Mac and nobody is signed in to it. Sign in and sign in to \
                 the same free account on your phone."
            }
            TailnetState::NeedsApproval => {
                "Tailscale is signed in and waiting for the owner of the account to approve this \
                 Mac. Nothing on this Mac can approve it for you."
            }
            TailnetState::Stopped => {
                "Tailscale is signed in and switched off, so its name for this Mac does not work \
                 anywhere, not even on a phone sitting beside it. Switch it back on."
            }
            TailnetState::InUseByAnotherUser => {
                "Tailscale is being used by a different account on this Mac, so I cannot read it \
                 from yours."
            }
            TailnetState::CertificatesOff { .. } => {
                "Tailscale has a name for this Mac and your Tailscale account has not been told \
                 to issue certificates for it yet. That is one switch on Tailscale's own website \
                 and until it is on, your phone would still show a warning."
            }
            TailnetState::Ready { .. } => {
                "Tailscale has a name for this Mac that your phone can reach from anywhere and a \
                 certificate your phone already trusts. There is nothing to install on the phone."
            }
        }
    }
}

/// `mm1.tailnet-name.ts.net` becomes `https://mm1.tailnet-name.ts.net:8443`.
///
/// **Lowercased, and the trailing dot removed**, for the reason `names.rs` gives about
/// `mm1.local`: a certificate's DNS name is matched case-insensitively but an ORIGIN is compared
/// byte for byte by the browser, and two spellings would be two origins — two service workers, two
/// push subscriptions, two Home Screen icons.
///
/// **The port stays 8443.** It is the same listener on the same port as at home; only the name and
/// the certificate presented for it are new. `mod.rs` on `HTTPS_PORT`: *"the port is part of the
/// origin … changing it means re-installing the phone app."*
pub fn origin_for(name: &str) -> String {
    format!("https://{}:{}", normalize_name(name), HTTPS_PORT)
}

/// Strip the trailing dot the upstream `DNSName` comment promises, and lowercase.
pub fn normalize_name(name: &str) -> String {
    name.trim().trim_end_matches('.').to_lowercase()
}

// -------------------------------------------------------------------------------------
// reading the machine
// -------------------------------------------------------------------------------------

/// The first `tailscale` command line that exists, or `None`.
pub fn find_cli() -> Option<PathBuf> {
    for candidate in CLI_CANDIDATES {
        let path = Path::new(candidate);
        if path.is_file() {
            return Some(path.to_path_buf());
        }
    }
    // `PATH` last — see the note on [`CLI_CANDIDATES`].
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join("tailscale");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// **Ask this Mac where it is on the Tailscale path.**
///
/// Returns the state and, when something went wrong, the label for it. It never returns an error:
/// every way this can fail is a state a screen has to be able to show, and a `Result` here would
/// push that decision to a caller who has less to say about it.
pub fn detect() -> (TailnetState, Option<Diagnostic>) {
    let Some(cli) = find_cli() else {
        return (TailnetState::Absent, Some(Diagnostic::NotInstalled));
    };
    detect_with(&cli)
}

/// [`detect`], against a named command line. Separated so a test can point it at a script that
/// prints a recorded document, which is the only way any of this is exercised on a Mac with no
/// Tailscale on it.
pub fn detect_with(cli: &Path) -> (TailnetState, Option<Diagnostic>) {
    let output = match Command::new(cli).arg("status").arg("--json").output() {
        Ok(o) => o,
        Err(_) => return (TailnetState::NotRunning, Some(Diagnostic::Unreadable)),
    };
    if !output.status.success() {
        // NOT the stderr text into a log line. Classified, then dropped.
        let diagnostic = Diagnostic::classify(&String::from_utf8_lossy(&output.stderr));
        return (TailnetState::NotRunning, Some(diagnostic));
    }
    match parse_status(&String::from_utf8_lossy(&output.stdout)) {
        Ok(state) => (state, None),
        Err(_) => (TailnetState::NotRunning, Some(Diagnostic::Unreadable)),
    }
}

/// The document `tailscale status --json` prints, **narrowed to the four fields this build reads**.
///
/// `#[serde(default)]` on every field and no `deny_unknown_fields`, on purpose: the real document
/// is far larger than this and gains fields between releases, and a parser that refused what it was
/// not told about would turn a Tailscale update into a RichOS outage.
///
/// # Every list is `Option<Vec<…>>`, and that is not defensive padding
///
/// **A nil Go slice marshals to `null`, not to `[]`**, and `#[serde(default)]` supplies a default
/// for a **missing** field rather than for an explicit `null` — so `Vec<String>` here rejects the
/// real document with *"invalid type: null, expected a sequence"* and the whole parse fails.
///
/// **This was a live defect, found by running the binary rather than by reading its types.** The
/// first draft of this module was pinned to upstream's declarations — `CertDomains []string`,
/// `TailscaleIPs []netip.Addr` — and used `Vec<String>`. Tailscale 1.102.4 was then installed on
/// this Mac, and its very first `status --json` came back with `"CertDomains": null`,
/// `"TailscaleIPs": null` and `"Peer": null`. Against that document the parser returned
/// `Unreadable`, so **a signed-out Mac would have been reported as "not running"** and Urban's
/// sign-in screen would never have been drawn. The type declaration was right and the inference
/// from it was wrong, which is the whole reason the rule is to read the machine.
#[derive(serde::Deserialize, Default)]
struct StatusDocument {
    #[serde(rename = "BackendState", default)]
    backend_state: String,
    #[serde(rename = "Self", default)]
    self_peer: Option<PeerDocument>,
    #[serde(rename = "CertDomains", default)]
    cert_domains: Option<Vec<String>>,
    /// The signed-in identities, keyed by user id as a STRING — JSON object keys always are, even
    /// when the id is a number both in Go and in the `ID` field inside the value.
    #[serde(rename = "User", default)]
    users: Option<std::collections::HashMap<String, UserDocument>>,
    /// Every other node on this tailnet, keyed by node key. This is where a phone shows up.
    #[serde(rename = "Peer", default)]
    peers: Option<std::collections::HashMap<String, PeerDocument>>,
}

#[derive(serde::Deserialize, Default)]
struct UserDocument {
    #[serde(rename = "LoginName", default)]
    login_name: Option<String>,
}

#[derive(serde::Deserialize, Default)]
struct PeerDocument {
    #[serde(rename = "DNSName", default)]
    dns_name: Option<String>,
    #[serde(rename = "TailscaleIPs", default)]
    tailscale_ips: Option<Vec<String>>,
    #[serde(rename = "Online", default)]
    online: bool,
    /// Which entry of the top-level `User` map is this node's owner. `0` when signed out.
    #[serde(rename = "UserID", default)]
    user_id: Option<u64>,
    /// `"macOS"`, `"iOS"`, `"android"`, `"windows"`, `"linux"`. How a phone is told from a laptop.
    #[serde(rename = "OS", default)]
    os: Option<String>,
    /// The node's own name, which is what the screen calls it.
    #[serde(rename = "HostName", default)]
    host_name: Option<String>,
}

/// Is this peer a phone or a tablet?
///
/// Matched case-insensitively against the two values Tailscale's mobile clients report. A
/// desktop peer is not evidence of anything here: the user's other Mac being on the tailnet
/// says nothing about whether their phone is.
fn is_phone(os: &str) -> bool {
    let os = os.trim().to_lowercase();
    os == "ios" || os == "android" || os == "ipados"
}

/// **The whole decision, as one pure function over the document.**
///
/// Pure so it can be tested without a daemon, which on this Mac is the only way it can be tested
/// at all.
pub fn parse_status(json: &str) -> Result<TailnetState, PhoneError> {
    let document: StatusDocument = serde_json::from_str(json).map_err(|e| {
        PhoneError::Malformed(format!("tailscale status was not the expected shape: {e}"))
    })?;

    // The seven `ipn.State` spellings, from `ipn/backend.go`. An unrecognized value is treated as
    // `NotRunning` rather than as ready, because the only safe reading of a state we do not know
    // is "do not offer this path yet".
    match document.backend_state.as_str() {
        "Running" => {}
        "NeedsLogin" | "NoState" => return Ok(TailnetState::NeedsSignIn),
        "NeedsMachineAuth" => return Ok(TailnetState::NeedsApproval),
        "Stopped" => return Ok(TailnetState::Stopped),
        "InUseOtherUser" => return Ok(TailnetState::InUseByAnotherUser),
        "Starting" => return Ok(TailnetState::NotRunning),
        _ => return Ok(TailnetState::NotRunning),
    }

    let Some(peer) = document.self_peer else {
        // Running with no self is not a state the daemon should reach; it is certainly not one to
        // offer an origin from.
        return Ok(TailnetState::NotRunning);
    };
    let name = normalize_name(peer.dns_name.as_deref().unwrap_or_default());
    if name.is_empty() {
        return Ok(TailnetState::NotRunning);
    }
    if !peer.online {
        // `Online` is documented as *"whether node is connected to the control plane"*. A node the
        // control plane cannot see is a node whose name other devices will not be told about.
        return Ok(TailnetState::Stopped);
    }

    // `CertDomains` is the control plane's own answer to "will I be given a certificate for this
    // name?", and it is the ONLY place that question is answered without asking for a certificate.
    // Comparing normalized on both sides: upstream says these are FQDNs *without* trailing
    // periods, but normalizing both is free and survives that changing.
    // WHICH ACCOUNT, in the words the phone screen will repeat back. Looked up by `Self.UserID`
    // rather than by taking the only entry: the `User` map carries every identity the node has
    // seen, so "the only one" is true right up until it is not.
    let account = peer
        .user_id
        .filter(|id| *id != 0)
        .and_then(|id| document.users.as_ref().and_then(|users| users.get(&id.to_string())))
        .and_then(|user| user.login_name.as_deref())
        .map(str::trim)
        .filter(|login| !login.is_empty())
        .map(|login| Account {
            login_name: login.to_string(),
            provider: Account::infer_provider(login),
        });

    // IS A PHONE ALREADY ON THIS TAILNET? The CEO puts the mismatched-account failure at 9 in 10
    // users, and this is the only evidence that answers it: a phone signed in to the SAME account
    // appears here; one signed in to a different provider's account appears in a different tailnet
    // and therefore nowhere. Deterministic order, because a HashMap's is not, and a name that
    // changes between two identical polls is a screen that flickers.
    //
    // DE-DUPLICATED BY NAME, AND ONLINE WINS — measured on this Mac on 2026-09-19 rather than
    // reasoned about. The CEO's own document carries his Android TWICE, once per reinstall, and
    // both entries say `"Online": false` because the app on the phone is switched off. Counting
    // registrations would have reported two phones; ignoring `Online` would have reported a phone
    // that cannot answer. So identical names collapse to one device, and that device is online if
    // ANY of its registrations is.
    let mut phones: Vec<(String, bool)> = Vec::new();
    for peer in document.peers.unwrap_or_default().into_values() {
        if !peer.os.as_deref().map(is_phone).unwrap_or(false) {
            continue;
        }
        let Some(name) = peer.host_name else { continue };
        let name = name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        match phones.iter_mut().find(|(seen, _)| seen.eq_ignore_ascii_case(&name)) {
            Some(existing) => existing.1 |= peer.online,
            None => phones.push((name, peer.online)),
        }
    }
    phones.sort_by(|a, b| a.0.cmp(&b.0));
    // A phone that is ON is the one to name: with a live phone and a stale registration beside it,
    // reporting the stale one would tell a user who has done everything right that they have not.
    let phone = phones
        .iter()
        .find(|(_, online)| *online)
        .or_else(|| phones.first())
        .map(|(name, online)| PhonePeer { name: name.clone(), online: *online });

    let certified = document
        .cert_domains
        .unwrap_or_default()
        .iter()
        .any(|domain| normalize_name(domain) == name);
    if !certified {
        return Ok(TailnetState::CertificatesOff { name, account, phone });
    }

    // Taken from the status document rather than from `ifconfig`, deliberately. `names.rs` reads
    // addresses out of `ifconfig -a`, and a Tailscale address WOULD show up there — it is a
    // `100.64.0.0/10` address on a `utun`, and `ipv4_addresses` filters only loopback, link-local,
    // unspecified and broadcast. But "it happens to appear in ifconfig" is an accident of one
    // variant's implementation, on a machine nobody here has. Asking the daemon which addresses it
    // assigned this node is the same answer, from the thing that knows it.
    let addresses = peer
        .tailscale_ips
        .unwrap_or_default()
        .iter()
        .filter_map(|text| text.trim().parse::<IpAddr>().ok())
        .collect();
    Ok(TailnetState::Ready { name, addresses, account, phone })
}

// -------------------------------------------------------------------------------------
// the certificate
// -------------------------------------------------------------------------------------

/// A private key as the PEM label said it was.
///
/// **Two variants, because I do not know which one `tailscale cert` emits and will not guess.**
/// An ECDSA key travels either as PKCS#8 (PEM label `PRIVATE KEY`) or as SEC1 (PEM label
/// `EC PRIVATE KEY`); rustls wants to be told which, and feeding it the wrong one fails at the
/// handshake rather than at the parse. `listen.rs`'s own TLS test exists because *"a
/// SEC1-versus-PKCS#8 mismatch … is the one thing about this handshake that is easy to get wrong
/// and impossible to see."* Reading the label is free and settles it on the machine rather than
/// in a recollection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyDer {
    Pkcs8(Vec<u8>),
    Sec1(Vec<u8>),
}

/// What the listener needs in order to present this Mac's tailnet name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailnetCert {
    /// Leaf first, then every intermediate the command handed over.
    pub chain_der: Vec<Vec<u8>>,
    pub key: KeyDer,
}

/// Every PEM block in a bundle, as `(label, DER)`.
fn pem_blocks(text: &str) -> Vec<(String, Vec<u8>)> {
    const OPEN: &str = "-----BEGIN ";
    const DASHES: &str = "-----";
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        let after = &rest[start + OPEN.len()..];
        let Some(label_end) = after.find(DASHES) else { break };
        let label = after[..label_end].trim().to_string();
        let body_start = label_end + DASHES.len();
        let end_marker = format!("-----END {label}{DASHES}");
        let Some(body_end) = after[body_start..].find(&end_marker) else { break };
        let body: String = after[body_start..body_start + body_end]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if let Ok(der) = super::unb64std(&body) {
            out.push((label, der));
        }
        rest = &after[body_start + body_end + end_marker.len()..];
    }
    out
}

/// **Ask Tailscale for the publicly trusted certificate for this Mac's tailnet name.**
///
/// `tailscale cert --cert-file - --key-file - <name>` — **both to stdout, and that is the whole
/// trick.** `cmd/tailscale/cli/cert.go` documents each flag as *"output cert file or `-` for
/// stdout"*, and asking for stdout rather than a path does two things a path cannot:
///
/// * it crosses the **Mac App Store sandbox**, where the client is a sandboxed network extension
///   and writing to a directory of our choosing is not a given, through a pipe we already own; and
/// * **the private key never touches the disk.** It goes from the pipe into memory and from there
///   into the Keychain, which is how `ca.rs` already handles every other key in this module.
///
/// Both streams arriving on one file descriptor means the two are concatenated, so the blocks are
/// sorted **by their PEM label rather than by their order** — which also makes this immune to
/// upstream ever writing the key first.
///
/// `unverified:` that this command succeeds on any variant, because there is no Tailscale on this
/// Mac. What IS verified is the parse, against a bundle built from real DER — see
/// `a_bundle_from_stdout_round_trips_to_the_bytes_it_was_built_from`.
pub fn fetch_cert(cli: &Path, name: &str, min_validity: &str) -> Result<TailnetCert, Diagnostic> {
    let name = normalize_name(name);
    let output = Command::new(cli)
        .arg("cert")
        .arg("--cert-file")
        .arg("-")
        .arg("--key-file")
        .arg("-")
        .arg("--min-validity")
        .arg(min_validity)
        .arg(&name)
        .output()
        .map_err(|_| Diagnostic::Unreadable)?;
    if !output.status.success() {
        // The two sentences this command prints — "HTTPS cert support is not enabled/configured
        // for your tailnet." and "Tailscale is not running." — are exactly what `classify` reads,
        // and the raw text goes no further than this line.
        return Err(Diagnostic::classify(&String::from_utf8_lossy(&output.stderr)));
    }
    let mut chain_der = Vec::new();
    let mut key = None;
    for (label, der) in pem_blocks(&String::from_utf8_lossy(&output.stdout)) {
        match label.as_str() {
            "CERTIFICATE" => chain_der.push(der),
            "PRIVATE KEY" => key = Some(KeyDer::Pkcs8(der)),
            "EC PRIVATE KEY" => key = Some(KeyDer::Sec1(der)),
            _ => {}
        }
    }
    match (chain_der.is_empty(), key) {
        (false, Some(key)) => Ok(TailnetCert { chain_der, key }),
        // A success exit with nothing usable in it is not something to paper over with a retry.
        _ => Err(Diagnostic::Unreadable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    /// **NOT CAPTURED FROM A RUNNING DAEMON.** There is no Tailscale on this Mac — see the module
    /// header for the eight checks. Every field name, every spelling and the trailing dot are
    /// transcribed from the upstream type declarations quoted there; the surrounding document is
    /// abbreviated, which is exactly the property the parser must tolerate.
    ///
    /// `unverified:` that a real `tailscale status --json` on this machine agrees. Settled by one
    /// command once the CEO installs it, and `the_parser_ignores_everything_it_was_not_told_about`
    /// below is the standing insurance against the real one being bigger.
    fn ready_document() -> String {
        r#"{
          "Version": "1.no-assertion-is-made-about-this",
          "BackendState": "Running",
          "AuthURL": "",
          "TailscaleIPs": ["100.101.102.103"],
          "Self": {
            "HostName": "MM1",
            "DNSName": "mm1.tail1a2b3c.ts.net.",
            "TailscaleIPs": ["100.101.102.103", "fd7a:115c:a1e0::1"],
            "Online": true
          },
          "CertDomains": ["mm1.tail1a2b3c.ts.net"],
          "MagicDNSSuffix": "tail1a2b3c.ts.net"
        }"#
        .to_string()
    }

    fn with(document: &str, find: &str, replace: &str) -> String {
        assert!(document.contains(find), "the fixture no longer contains {find}");
        document.replace(find, replace)
    }

    /// **CAPTURED FROM THIS MAC, VERBATIM**, on 2026-09-18, by `/usr/local/bin/tailscale status
    /// --json` immediately after the CEO installed Tailscale 1.102.4 and before anybody signed in.
    /// Not abbreviated, not tidied — every key and every `null` is as the daemon printed it.
    ///
    /// **This document is the reason this module's lists are `Option<Vec<…>>`.** Against the
    /// constructed fixture below, which used `[]`, the parser was green. Against this one it
    /// returned `Unreadable`, because a nil Go slice marshals to `null` and `#[serde(default)]`
    /// does not cover an explicit `null`. A signed-out Mac would have been reported as "not
    /// running", and Urban's sign-in screen — the one screen this state exists to draw — would
    /// never have appeared.
    ///
    /// No personal data: `User`, `CurrentTailnet` and `Peer` are all `null` on a signed-out node,
    /// the node key is all zeroes, and `MM1` is this Mac's host name, already throughout this
    /// module.
    const REAL_SIGNED_OUT: &str = r#"{
  "Version": "1.102.4-t3caf7d9e7-g084ee3b64",
  "TUN": true,
  "BackendState": "NeedsLogin",
  "AuthURL": "",
  "TailscaleIPs": null,
  "Self": {
    "ID": "",
    "NodeID": 0,
    "PublicKey": "nodekey:0000000000000000000000000000000000000000000000000000000000000000",
    "HostName": "MM1",
    "DNSName": "",
    "OS": "macOS",
    "UserID": 0,
    "TailscaleIPs": null,
    "Addrs": [],
    "CurAddr": "",
    "Relay": "",
    "PeerRelay": "",
    "RxBytes": 0,
    "TxBytes": 0,
    "Created": "0001-01-01T00:00:00Z",
    "LastWrite": "0001-01-01T00:00:00Z",
    "LastSeen": "0001-01-01T00:00:00Z",
    "LastHandshake": "0001-01-01T00:00:00Z",
    "Online": false,
    "ExitNode": false,
    "ExitNodeOption": false,
    "Active": false,
    "PeerAPIURL": null,
    "TaildropTarget": 0,
    "NoFileSharingReason": "",
    "InNetworkMap": false,
    "InMagicSock": false,
    "InEngine": false
  },
  "Health": [
    "Tailscale is stopped."
  ],
  "MagicDNSSuffix": "",
  "CurrentTailnet": null,
  "CertDomains": null,
  "ExtraRecords": null,
  "Peer": null,
  "User": null,
  "ClientVersion": null
}"#;

    /// **CAPTURED FROM THIS MAC**, 2026-09-18, minutes after `REAL_SIGNED_OUT` above, once the CEO
    /// had signed in to a new free tailnet — and **scrubbed**, which is the one difference from
    /// the fixture above and is named here rather than left to be noticed.
    ///
    /// A signed-in document carries an identity that a signed-out one does not: `User` holds a
    /// `LoginName` and a `DisplayName`, and `CurrentTailnet.Name` is the account's email address.
    /// Those two objects are replaced below, and the real `MagicDNSSuffix` is replaced with the
    /// placeholder used everywhere else in this file. **Everything the parser reads — `BackendState`,
    /// `Self.DNSName`, `Self.Online`, `Self.TailscaleIPs`, `CertDomains` — is structurally as the
    /// daemon printed it**, including `CertDomains: null` and both address families.
    ///
    /// **This Mac was in `certificates-off` at the moment of capture**, which is the state the
    /// brief's four-state list did not have. It is not a hypothetical: it is where a brand-new
    /// free tailnet lands, before anybody visits the admin console.
    const REAL_SIGNED_IN_NO_CERTS: &str = r#"{
  "Version": "1.102.4-t3caf7d9e7-g084ee3b64",
  "TUN": true,
  "BackendState": "Running",
  "AuthURL": "",
  "TailscaleIPs": ["100.75.153.24", "fd7a:115c:a1e0::b3a:991a"],
  "Self": {
    "HostName": "MM1",
    "DNSName": "mm1.tail1a2b3c.ts.net.",
    "OS": "macOS",
    "TailscaleIPs": ["100.75.153.24", "fd7a:115c:a1e0::b3a:991a"],
    "Online": true,
    "Active": false,
    "InNetworkMap": true
  },
  "Health": [],
  "MagicDNSSuffix": "tail1a2b3c.ts.net",
  "CurrentTailnet": {"Name": "scrubbed", "MagicDNSSuffix": "tail1a2b3c.ts.net", "MagicDNSEnabled": true},
  "CertDomains": null,
  "ExtraRecords": null,
  "Peer": null,
  "User": {"1": {"ID": 1, "LoginName": "scrubbed", "DisplayName": "scrubbed"}},
  "ClientVersion": null
}"#;

    /// `REAL_SIGNED_IN_NO_CERTS` with a phone added to `Peer`, in the shape the daemon reports a
    /// mobile node: an `OS` of `iOS` or `android`, and a `HostName`.
    fn with_phone(os: &str) -> String {
        with(
            REAL_SIGNED_IN_NO_CERTS,
            r#""Peer": null"#,
            &format!(
                r#""Peer": {{
      "nodekey:aaa": {{"HostName": "alexs-macbook", "OS": "macOS", "Online": true}},
      "nodekey:bbb": {{"HostName": "alexs-phone", "OS": "{os}", "Online": true}}
    }}"#
            ),
        )
    }

    /// **THE DOCUMENT THIS MAC ACTUALLY PRINTED ON 2026-09-19**, after the CEO signed in again —
    /// and the reason the two rules below exist at all.
    ///
    /// Scrubbed the same way as `REAL_SIGNED_IN_NO_CERTS`: the login name, the MagicDNS suffix and
    /// the device name are replaced. **The SHAPE is verbatim and it is the finding** — his Android
    /// appears TWICE, one stale registration per reinstall (he reinstalled three times looking for
    /// a "connect to Mac" step that does not exist), and **both entries are `"Online": false`,**
    /// because Tailscale is installed on the phone and switched off.
    ///
    /// Read the old way, this document said "you have two phones and they are both here". Neither
    /// half of that was true.
    const REAL_PHONE_REGISTERED_TWICE_AND_OFFLINE: &str = r#"{
  "Version": "1.102.4-t3caf7d9e7-g084ee3b64",
  "BackendState": "Running",
  "TailscaleIPs": ["100.75.153.24"],
  "Self": {
    "HostName": "MM1",
    "DNSName": "mm1.tail1a2b3c.ts.net.",
    "OS": "macOS",
    "UserID": 1,
    "TailscaleIPs": ["100.75.153.24"],
    "Online": true
  },
  "MagicDNSSuffix": "tail1a2b3c.ts.net",
  "CertDomains": null,
  "Peer": {
    "nodekey:bbb": {"HostName": "his-android", "OS": "android", "Online": false},
    "nodekey:ccc": {"HostName": "his-android", "OS": "android", "Online": false}
  },
  "User": {"1": {"ID": 1, "LoginName": "someone@gmail.com", "DisplayName": "scrubbed"}},
  "ClientVersion": null
}"#;

    #[test]
    fn a_phone_on_the_same_tailnet_is_detected_and_one_on_a_different_account_is_not() {
        // THE FAILURE THE CEO PUTS AT 9 IN 10 USERS, detected rather than described. A phone signed
        // in to the SAME account appears in `Peer`; one signed in with a different provider is in a
        // different tailnet and so appears NOWHERE. That asymmetry is the whole mechanism.
        for os in ["iOS", "android", "ipados", "ANDROID"] {
            let state = parse_status(&with_phone(os)).expect("the document was refused");
            let phone = state.phone().expect("a mobile peer was not recognized as a phone");
            assert_eq!(phone.name, "alexs-phone", "wrong phone for an {os} peer");
            assert!(phone.online, "an `Online: true` {os} peer was not reported as online");
        }

        // THE CONTROL, and the reason the above means anything: the SAME document with NO phone
        // peer — which is exactly what a mismatched account looks like — reports none. The macOS
        // peer is present in both, so this is about the OS test and not about `Peer` being read.
        let no_phone = parse_status(REAL_SIGNED_IN_NO_CERTS).unwrap();
        assert_eq!(no_phone.phone(), None);

        // And a tailnet with only a second DESKTOP on it is still "no phone". The user's other Mac
        // being reachable says nothing about whether their phone is.
        let desktops = with(
            REAL_SIGNED_IN_NO_CERTS,
            r#""Peer": null"#,
            r#""Peer": {"nodekey:aaa": {"HostName": "alexs-macbook", "OS": "macOS", "Online": true}}"#,
        );
        assert_eq!(parse_status(&desktops).unwrap().phone(), None);
    }

    #[test]
    fn the_ceos_own_document_reads_as_one_phone_that_is_switched_off() {
        // MEASURED, NOT REASONED ABOUT. Two registrations of one device, both offline, is what his
        // Mac printed — and it is the shape every user who reinstalls the app will have.
        let state = parse_status(REAL_PHONE_REGISTERED_TWICE_AND_OFFLINE).expect("refused");
        let phone = state.phone().expect("his phone was not found at all");
        // ONE device, not two: the duplicate registrations collapse by name.
        assert_eq!(phone.name, "his-android");
        // AND IT CANNOT ANSWER. This is the assertion that stops the screen saying "your phone is
        // on this network" over a phone with Tailscale switched off — a sentence that would send
        // him back to check an identity that was already right.
        assert!(!phone.online, "two offline registrations were read as a reachable phone");

        // AND THE IDENTITY IS THE GOOGLE ONE, end to end through the parser rather than through
        // the provider table alone. Apple is covered by the test above; this is the pair the CEO
        // actually hit — Apple on the Mac, Google on the phone.
        let account = state.account().cloned().expect("no account read");
        assert_eq!(account.login_name, "someone@gmail.com");
        assert_eq!(account.provider, Some("Google"));
        assert_eq!(account.described(), "Google as someone@gmail.com");
    }

    #[test]
    fn one_live_registration_beside_a_stale_one_reports_the_phone_as_online() {
        // THE OTHER HALF, and the one that decides which way the de-duplication leans. He turns
        // Tailscale ON on the phone: the live node appears beside the stale registrations that
        // never went away. Reporting the stale one would tell a user who has just done everything
        // right that they have not — the exact loop this screen exists to end.
        let document = with(
            REAL_PHONE_REGISTERED_TWICE_AND_OFFLINE,
            r#""nodekey:ccc": {"HostName": "his-android", "OS": "android", "Online": false}"#,
            r#""nodekey:ccc": {"HostName": "his-android", "OS": "android", "Online": true}"#,
        );
        let phone = parse_status(&document).unwrap().phone().cloned().expect("no phone");
        assert_eq!(phone.name, "his-android");
        assert!(phone.online, "a live registration was masked by a stale duplicate");

        // And a second, genuinely different phone that is online is the one named, whatever the
        // alphabet says — `a-phone` sorts first here and is the live one.
        let two = with(
            REAL_PHONE_REGISTERED_TWICE_AND_OFFLINE,
            r#""nodekey:ccc": {"HostName": "his-android", "OS": "android", "Online": false}"#,
            r#""nodekey:ccc": {"HostName": "a-phone", "OS": "iOS", "Online": true}"#,
        );
        let phone = parse_status(&two).unwrap().phone().cloned().expect("no phone");
        assert_eq!(phone.name, "a-phone");
        assert!(phone.online);
    }

    #[test]
    fn the_account_is_read_off_the_mac_so_the_phone_screen_can_name_it() {
        // The CEO signed in with Apple on the Mac and Google on the Android and "had no way to
        // know they were different networks, or which to reuse". This is what lets the screen say
        // WHICH.
        let document = with(
            REAL_SIGNED_IN_NO_CERTS,
            r#""User": {"1": {"ID": 1, "LoginName": "scrubbed", "DisplayName": "scrubbed"}}"#,
            r#""User": {"7": {"ID": 7, "LoginName": "someone@icloud.com", "DisplayName": "Someone"}}"#,
        );
        let document = with(&document, r#""HostName": "MM1","#, r#""HostName": "MM1", "UserID": 7,"#);
        let account = parse_status(&document).unwrap().account().cloned().expect("no account read");
        assert_eq!(account.login_name, "someone@icloud.com");
        assert_eq!(account.provider, Some("Apple"));
        assert_eq!(account.described(), "Apple as someone@icloud.com");
    }

    #[test]
    fn the_provider_is_inferred_from_the_domain_and_is_none_when_it_cannot_be_known() {
        let cases = [
            ("someone@icloud.com", Some("Apple")),
            ("someone@me.com", Some("Apple")),
            ("someone@gmail.com", Some("Google")),
            ("someone@outlook.com", Some("Microsoft")),
            ("someone@hotmail.com", Some("Microsoft")),
            ("alex@github", Some("GitHub")),
            // THE HONEST `None`s, and they are the important half. A Google Workspace login is the
            // user's own company domain and a self-hosted or SSO login can be anything at all — so
            // a guess would be wrong for exactly the people most likely to keep two identities.
            ("someone@their-own-company.com", None),
            ("someone@example.org", None),
        ];
        for (login, expected) in cases {
            assert_eq!(Account::infer_provider(login), expected, "wrong provider for {login}");
        }
        // With no provider the screen still gets the thing the CEO could not get: WHICH account.
        let bare = Account { login_name: "someone@their-own-company.com".into(), provider: None };
        assert_eq!(bare.described(), "someone@their-own-company.com");
    }

    #[test]
    fn the_real_signed_in_document_lands_in_certificates_off_with_the_name_known() {
        // THE STATE THIS MAC WAS ACTUALLY IN. Signed in, online, a real tailnet name, and a
        // control plane that will not certify it until a switch is thrown on Tailscale's website.
        let state = parse_status(REAL_SIGNED_IN_NO_CERTS).expect("the real document was refused");
        assert_eq!(state.token(), "certificates-off");
        assert_eq!(state.name(), Some("mm1.tail1a2b3c.ts.net"));
        // AND NO ORIGIN, which is the whole point: offering one here would put a certificate
        // warning on the phone on the one path whose promise is that there is not one.
        assert_eq!(state.origin(), None);
        assert!(state.sentence().contains("has not been told"));
    }

    #[test]
    fn the_refusal_this_mac_actually_returned_is_read_as_certificates_being_off() {
        // MEASURED, NOT READ. `cmd/tailscale/cli/cert.go` does not contain this sentence — it is
        // the control plane's, relayed through the CLI — so no amount of reading that file would
        // have produced it. Without this arm it classified as `Refused`, and the screen would have
        // said "something went wrong" instead of naming the one switch that fixes it.
        let measured = "500 Internal Server Error: your Tailscale account does not support getting TLS certs";
        assert_eq!(Diagnostic::classify(measured), Diagnostic::HttpsNotEnabled);
        // POSITIVE CONTROL: something genuinely unrecognized still lands in `Refused`, so the arm
        // above is a pattern and not a catch-all that has swallowed the distinction.
        assert_eq!(Diagnostic::classify("some unrelated trouble"), Diagnostic::Refused);
    }

    #[test]
    fn the_real_document_this_mac_printed_is_read_as_a_mac_waiting_to_be_signed_in() {
        // THE REGRESSION TEST FOR THE NULL DEFECT, and the only fixture in this file that was
        // taken off a running daemon rather than transcribed from a type declaration.
        let state = parse_status(REAL_SIGNED_OUT).expect("the real document was refused");
        assert_eq!(state.token(), "needs-sign-in");
        assert_eq!(state.origin(), None);
        assert_eq!(state.name(), None);
        // The sentence Urban's Screen 3 is built on has to be the one this state produces.
        assert!(state.sentence().contains("nobody is signed in"));
    }

    #[test]
    fn a_null_list_is_read_as_an_empty_one_wherever_one_can_appear() {
        // Said once per field, because each is a separate `Option` and a future edit could drop
        // one of them back to `Vec` without any other test noticing.
        let running_with_nulls = r#"{
          "BackendState": "Running",
          "CertDomains": null,
          "Self": {"DNSName": "mm1.tail1a2b3c.ts.net.", "TailscaleIPs": null, "Online": true}
        }"#;
        // Certificates unknown -> certificates-off, NOT a parse failure.
        let state = parse_status(running_with_nulls).unwrap();
        assert_eq!(state.token(), "certificates-off");
        assert_eq!(state.name(), Some("mm1.tail1a2b3c.ts.net"));

        // And a null `DNSName`, which Go would not produce for a string but costs nothing to
        // survive.
        let null_name = r#"{"BackendState": "Running", "Self": {"DNSName": null, "Online": true}}"#;
        assert_eq!(parse_status(null_name).unwrap().token(), "not-running");
    }

    #[test]
    fn a_signed_in_mac_with_certificates_enabled_is_the_only_state_that_yields_an_origin() {
        let state = parse_status(&ready_document()).unwrap();
        assert_eq!(state.token(), "ready");
        assert_eq!(state.name(), Some("mm1.tail1a2b3c.ts.net"));
        // THE TRAILING DOT IS GONE and the port is the one the listener already uses. Both halves
        // matter: a browser compares an origin byte for byte, so `mm1.…ts.net.` and `mm1.…ts.net`
        // would be two different installed apps.
        assert_eq!(state.origin(), Some("https://mm1.tail1a2b3c.ts.net:8443".to_string()));
        assert!(!state.origin().unwrap().contains(".ts.net.:"), "the trailing dot survived");
    }

    #[test]
    fn the_addresses_come_from_the_daemon_and_both_families_survive() {
        let TailnetState::Ready { addresses, .. } = parse_status(&ready_document()).unwrap() else {
            panic!("the ready document did not parse as ready");
        };
        assert_eq!(
            addresses,
            vec![
                IpAddr::V4(Ipv4Addr::new(100, 101, 102, 103)),
                IpAddr::V6("fd7a:115c:a1e0::1".parse::<Ipv6Addr>().unwrap()),
            ]
        );
    }

    #[test]
    fn certificates_being_off_is_its_own_state_and_offers_no_origin() {
        // THE STATE THE BRIEF'S FOUR-STATE LIST DOES NOT HAVE. Signed in, online, name known, and
        // the control plane will not certify it — an admin-console switch on Tailscale's website.
        // Offering the origin here would put a certificate warning on the phone at the exact
        // moment this path's promise is that there is not one.
        let document = with(
            &ready_document(),
            r#""CertDomains": ["mm1.tail1a2b3c.ts.net"]"#,
            r#""CertDomains": []"#,
        );
        let state = parse_status(&document).unwrap();
        assert_eq!(state.token(), "certificates-off");
        assert_eq!(state.name(), Some("mm1.tail1a2b3c.ts.net"), "the name is known even here");
        assert_eq!(state.origin(), None, "an origin was offered with no certificate behind it");
        // POSITIVE CONTROL, in the same test: the SAME document with the domain restored does
        // yield an origin, so the `None` above is about `CertDomains` and not about the parser
        // having given up somewhere earlier.
        assert!(parse_status(&ready_document()).unwrap().origin().is_some());
    }

    #[test]
    fn a_certificate_for_some_other_machine_does_not_count_as_one_for_this_one() {
        // `CertDomains` is tailnet-wide in principle. Being non-empty is not the question; holding
        // THIS name is. An `is_empty()` check would have passed this and been wrong.
        let document = with(
            &ready_document(),
            r#""CertDomains": ["mm1.tail1a2b3c.ts.net"]"#,
            r#""CertDomains": ["someone-elses-laptop.tail1a2b3c.ts.net"]"#,
        );
        assert_eq!(parse_status(&document).unwrap().token(), "certificates-off");
    }

    #[test]
    fn every_one_of_the_seven_backend_states_lands_somewhere_named() {
        // From `ipn/backend.go`'s `stateStrings`, all seven, so a state added upstream shows up
        // here as a miss rather than as a silent "ready".
        let cases = [
            ("Running", "ready"),
            ("NeedsLogin", "needs-sign-in"),
            ("NoState", "needs-sign-in"),
            ("NeedsMachineAuth", "needs-approval"),
            ("Stopped", "stopped"),
            ("InUseOtherUser", "other-user"),
            ("Starting", "not-running"),
        ];
        for (backend, expected) in cases {
            let document = with(
                &ready_document(),
                r#""BackendState": "Running""#,
                &format!(r#""BackendState": "{backend}""#),
            );
            assert_eq!(
                parse_status(&document).unwrap().token(),
                expected,
                "BackendState {backend} landed in the wrong state"
            );
        }
    }

    #[test]
    fn a_backend_state_this_build_has_never_heard_of_is_not_ready() {
        // The safe reading of an unknown state is "do not offer this path", never "carry on".
        let document = with(
            &ready_document(),
            r#""BackendState": "Running""#,
            r#""BackendState": "SomethingAddedIn2027""#,
        );
        assert_eq!(parse_status(&document).unwrap().token(), "not-running");
    }

    #[test]
    fn a_node_the_control_plane_cannot_see_is_reported_as_switched_off() {
        // Sage's reach document, 2.3: "when Tailscale is off or logged out, the name stops
        // resolving including at home." Offline-but-Running is that, and it must not offer an
        // origin that will not resolve.
        let document = with(&ready_document(), r#""Online": true"#, r#""Online": false"#);
        let state = parse_status(&document).unwrap();
        assert_eq!(state.token(), "stopped");
        assert_eq!(state.origin(), None);
    }

    #[test]
    fn the_parser_ignores_everything_it_was_not_told_about() {
        // THE STANDING INSURANCE against the fixture being smaller than the real document. A real
        // `status --json` carries Peer, User, TUN, ClientVersion, Health and more, and gains
        // fields between releases. A parser that refused them would make a Tailscale update into a
        // RichOS outage.
        let document = with(
            &ready_document(),
            r#""MagicDNSSuffix": "tail1a2b3c.ts.net""#,
            r#""MagicDNSSuffix": "tail1a2b3c.ts.net",
               "Health": ["something new"],
               "Peer": {"nodekey:abc": {"HostName": "phone", "Online": true}},
               "ClientVersion": {"RunningLatest": true},
               "AFieldInventedAfterThisBuildShipped": 17"#,
        );
        assert_eq!(parse_status(&document).unwrap().token(), "ready");
    }

    #[test]
    fn a_document_that_is_not_json_is_a_refusal_rather_than_a_panic() {
        assert!(parse_status("Tailscale is not running.").is_err());
        assert!(parse_status("").is_err());
        // And an EMPTY object parses, because every field defaults — landing on "not running",
        // which is the honest answer to a daemon that told us nothing.
        assert_eq!(parse_status("{}").unwrap().token(), "not-running");
    }

    #[test]
    fn running_with_no_self_and_running_with_no_name_are_both_refused() {
        let no_self = r#"{"BackendState": "Running", "CertDomains": ["x"]}"#;
        assert_eq!(parse_status(no_self).unwrap().token(), "not-running");
        let no_name = with(
            &ready_document(),
            r#""DNSName": "mm1.tail1a2b3c.ts.net.","#,
            r#""DNSName": "","#,
        );
        assert_eq!(parse_status(&no_name).unwrap().token(), "not-running");
    }

    #[test]
    fn a_name_is_lowercased_because_an_origin_is_compared_byte_for_byte() {
        let document = with(
            &ready_document(),
            r#""DNSName": "mm1.tail1a2b3c.ts.net.""#,
            r#""DNSName": "MM1.Tail1A2B3C.ts.net.""#,
        );
        // And `CertDomains` still matches, because both sides are normalized.
        let state = parse_status(&document).unwrap();
        assert_eq!(state.token(), "ready");
        assert_eq!(state.origin(), Some("https://mm1.tail1a2b3c.ts.net:8443".to_string()));
    }

    #[test]
    fn stderr_is_classified_into_the_closed_set_and_never_kept() {
        // T3 Code's reason, adopted: "stderr can contain auth keys (`tskey-…`) and node names, and
        // these labels are logged."
        assert_eq!(
            Diagnostic::classify("Tailscale is not running."),
            Diagnostic::DaemonUnreachable
        );
        assert_eq!(
            Diagnostic::classify("HTTPS cert support is not enabled/configured for your tailnet."),
            Diagnostic::HttpsNotEnabled
        );
        // A key-SHAPED string, assembled at run time so that no line of this file is one: GitHub's
        // secret scanning flagged the old literal as a Tailscale API key (alert #3, 2026-10-01).
        // It was never a real key (its secret half was the word SECRET); this keeps it so.
        let fake_key = ["tskey", "auth", "kFakeKeyIdCNTRL", "SECRETnotarealkey"].join("-");
        let secret = format!("failed to start: {fake_key} on node alex-laptop");
        // POSITIVE CONTROL: what goes in really is key-shaped, or the assertions below prove nothing.
        let prefix = ["tskey", "auth", ""].join("-");
        assert!(secret.contains(&prefix) && fake_key.starts_with(&prefix), "not key-shaped: {secret}");
        assert!(fake_key.ends_with("CNTRL-SECRETnotarealkey"), "not key-shaped: {secret}");
        let label = Diagnostic::classify(&secret).label();
        assert_eq!(label, "tailscale: refused");
        // The thing this test exists for: the label carries NONE of it.
        assert!(!label.contains("tskey"), "an auth key reached a label");
        assert!(!label.contains("alex"), "a node name reached a label");
        assert!(!label.contains("SECRET"));
    }

    #[test]
    fn every_state_has_a_sentence_and_a_distinct_token() {
        let all = [
            TailnetState::Absent,
            TailnetState::NotRunning,
            TailnetState::NeedsSignIn,
            TailnetState::NeedsApproval,
            TailnetState::Stopped,
            TailnetState::InUseByAnotherUser,
            TailnetState::CertificatesOff { name: "mm1.example.ts.net".into(), account: None, phone: None },
            TailnetState::Ready { name: "mm1.example.ts.net".into(), addresses: vec![], account: None, phone: None },
        ];
        let mut seen: Vec<&str> = Vec::new();
        for state in &all {
            let sentence = state.sentence();
            assert!(sentence.len() > 30, "{} has a sentence too short to help", state.token());
            assert!(sentence.ends_with('.'), "{} does not end in a full stop", state.token());
            // American English and spoken-aloud both mean no bare jargon the ear cannot hold. The
            // concrete check that is worth having: no state's sentence says "error" or "failed",
            // because seven of these eight are not failures at all.
            let lowered = sentence.to_lowercase();
            assert!(!lowered.contains("error"), "{} calls a state an error", state.token());
            assert!(!lowered.contains("failed"), "{} calls a state a failure", state.token());
            assert!(!seen.contains(&state.token()), "two states share the token {}", state.token());
            seen.push(state.token());
        }
        assert_eq!(seen.len(), 8);
    }

    #[test]
    fn only_ready_offers_an_origin() {
        // Said once, over every state, so a state added later cannot quietly start offering one.
        for state in [
            TailnetState::Absent,
            TailnetState::NotRunning,
            TailnetState::NeedsSignIn,
            TailnetState::NeedsApproval,
            TailnetState::Stopped,
            TailnetState::InUseByAnotherUser,
            TailnetState::CertificatesOff { name: "mm1.example.ts.net".into(), account: None, phone: None },
        ] {
            assert_eq!(state.origin(), None, "{} offered an origin", state.token());
            assert!(state.addresses().is_empty(), "{} offered addresses", state.token());
        }
        assert!(TailnetState::Ready { name: "mm1.example.ts.net".into(), addresses: vec![], account: None, phone: None }
            .origin()
            .is_some());
    }

    /// Write an executable script that prints `stdout_text`, exits `code`, and optionally prints
    /// `stderr_text`. The ONLY way any of the command-running code in this module is exercised on
    /// a Mac with no Tailscale on it.
    fn fake_cli(tag: &str, stdout_text: &str, stderr_text: &str, code: i32) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "richos-tailnet-{tag}-{}-{}",
            std::process::id(),
            super::super::now_millis()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("tailscale");
        // Single-quoted heredocs, so nothing in the payload is expanded by the shell.
        let body = format!(
            "#!/bin/sh\ncat <<'RICHOS_OUT'\n{stdout_text}\nRICHOS_OUT\ncat >&2 <<'RICHOS_ERR'\n{stderr_text}\nRICHOS_ERR\nexit {code}\n"
        );
        std::fs::write(&script, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        script
    }

    fn pem(label: &str, der: &[u8]) -> String {
        format!("-----BEGIN {label}-----\n{}\n-----END {label}-----", super::super::b64std(der))
    }

    #[test]
    fn a_bundle_from_stdout_round_trips_to_the_bytes_it_was_built_from() {
        // REAL DER, from this repository's own certificate authority, so the round trip is over
        // certificate bytes rather than over a string somebody typed. Both streams land on one
        // file descriptor, so the bundle is a leaf, an intermediate and a key, concatenated.
        use crate::phone::ca::PhoneCa;
        use crate::phone::names::LocalNames;
        use crate::phone::secrets::MemorySecrets;

        let dir = std::env::temp_dir()
            .join(format!("richos-tailnet-ca-{}-{}", std::process::id(), super::super::now_millis()));
        std::fs::create_dir_all(&dir).unwrap();
        let ca = PhoneCa::open(
            &dir,
            &MemorySecrets::default(),
            LocalNames {
                host: "MM1".into(),
                bonjour: "mm1.tail1a2b3c.ts.net".into(),
                addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            },
        )
        .unwrap();

        let bundle = format!(
            "{}\n{}\n{}",
            pem("CERTIFICATE", &ca.leaf_der),
            pem("CERTIFICATE", &ca.ca_der),
            pem("PRIVATE KEY", &ca.leaf_key_pkcs8)
        );
        let cli = fake_cli("cert", &bundle, "", 0);
        let got = fetch_cert(&cli, "MM1.Tail1A2B3C.ts.net.", "720h").expect("the bundle was refused");

        // LEAF FIRST, INTERMEDIATE AFTER, byte for byte.
        assert_eq!(got.chain_der, vec![ca.leaf_der.clone(), ca.ca_der.clone()]);
        assert_eq!(got.key, KeyDer::Pkcs8(ca.leaf_key_pkcs8.clone()));
        // THE CHAIN IS WHY THIS IS A Vec. `tailscale cert` hands over the leaf AND its issuing
        // intermediate, and a public chain missing the intermediate is the classic "works in
        // curl, fails on a phone" certificate.
        assert_eq!(got.chain_der.len(), 2);

        let _ = std::fs::remove_dir_all(cli.parent().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_key_variant_follows_the_label_rather_than_an_assumption_about_tailscale() {
        // The whole reason `KeyDer` has two variants. Same bytes, two labels, two answers — and
        // rustls fails at the HANDSHAKE rather than at the parse when this is wrong, which is why
        // it is read rather than assumed.
        let der = vec![9u8; 40];
        let sec1 = fake_cli("sec1", &format!("{}\n{}", pem("CERTIFICATE", &[1, 2, 3]), pem("EC PRIVATE KEY", &der)), "", 0);
        assert_eq!(fetch_cert(&sec1, "x.ts.net", "720h").unwrap().key, KeyDer::Sec1(der.clone()));

        let pkcs8 = fake_cli("pkcs8", &format!("{}\n{}", pem("CERTIFICATE", &[1, 2, 3]), pem("PRIVATE KEY", &der)), "", 0);
        assert_eq!(fetch_cert(&pkcs8, "x.ts.net", "720h").unwrap().key, KeyDer::Pkcs8(der));

        for path in [&sec1, &pkcs8] {
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        }
    }

    #[test]
    fn the_order_the_two_streams_arrive_in_does_not_matter() {
        // Both flags say `-`, so the two land on one file descriptor and their order is upstream's
        // to change. Sorting by label rather than by position is what makes that a non-event.
        let der = vec![7u8; 32];
        let cli = fake_cli(
            "reversed",
            &format!("{}\n{}", pem("PRIVATE KEY", &der), pem("CERTIFICATE", &[4, 5, 6])),
            "",
            0,
        );
        let got = fetch_cert(&cli, "x.ts.net", "720h").unwrap();
        assert_eq!(got.chain_der, vec![vec![4, 5, 6]]);
        assert_eq!(got.key, KeyDer::Pkcs8(der));
        let _ = std::fs::remove_dir_all(cli.parent().unwrap());
    }

    #[test]
    fn the_two_refusals_this_command_actually_prints_become_their_own_labels() {
        // From `cmd/tailscale/cli/cert.go`, the two sentences quoted in the module header.
        let off = fake_cli(
            "off",
            "",
            "HTTPS cert support is not enabled/configured for your tailnet.",
            1,
        );
        assert_eq!(fetch_cert(&off, "x.ts.net", "720h"), Err(Diagnostic::HttpsNotEnabled));

        let down = fake_cli("down", "", "Tailscale is not running.", 1);
        assert_eq!(fetch_cert(&down, "x.ts.net", "720h"), Err(Diagnostic::DaemonUnreachable));

        // A ZERO EXIT WITH NOTHING USABLE IN IT IS STILL A REFUSAL. Succeeding and returning a
        // certificate are two different things, and treating the first as the second is how an
        // empty chain reaches rustls.
        let empty = fake_cli("empty", "nothing resembling a certificate", "", 0);
        assert_eq!(fetch_cert(&empty, "x.ts.net", "720h"), Err(Diagnostic::Unreadable));

        // A CERTIFICATE WITH NO KEY IS ALSO A REFUSAL, not half an answer.
        let keyless = fake_cli("keyless", &pem("CERTIFICATE", &[1, 2, 3]), "", 0);
        assert_eq!(fetch_cert(&keyless, "x.ts.net", "720h"), Err(Diagnostic::Unreadable));

        for path in [&off, &down, &empty, &keyless] {
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        }
    }

    #[test]
    fn detection_reads_a_recorded_document_through_a_real_subprocess() {
        // `parse_status` is tested directly everywhere else; this is the one test that proves the
        // COMMAND path — argv, stdout capture, exit code — rather than the parser.
        let cli = fake_cli("status", &ready_document(), "", 0);
        let (state, diagnostic) = detect_with(&cli);
        assert_eq!(state.token(), "ready");
        assert_eq!(diagnostic, None);
        assert_eq!(state.origin(), Some("https://mm1.tail1a2b3c.ts.net:8443".to_string()));

        // And a daemon that is down is the label, with the raw text dropped.
        let down = fake_cli("statusdown", "", "Tailscale is not running.", 1);
        let (state, diagnostic) = detect_with(&down);
        assert_eq!(state.token(), "not-running");
        assert_eq!(diagnostic, Some(Diagnostic::DaemonUnreachable));

        for path in [&cli, &down] {
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        }
    }

    /// **THE LIVE PROOF, against whatever Tailscale is really doing on this machine.**
    ///
    /// `#[ignore]` because its answer depends on the state of somebody's Tailscale account, which
    /// is not a thing a test suite may depend on. Run it deliberately:
    ///
    /// ```text
    /// cargo test -p richos-tauri --bins phone::tailnet::tests::live -- --ignored --nocapture
    /// ```
    ///
    /// It asserts only what is true of EVERY state — that detection answers, that it never
    /// panics, and that an origin is offered if and only if the state is `ready` — and prints the
    /// rest for a human to read. An ignored test that asserted a particular tailnet's condition
    /// would be a test that fails when the CEO signs out.
    #[test]
    #[ignore = "depends on this machine's real Tailscale state; run with --ignored"]
    fn live_detection_against_the_real_binary_on_this_machine() {
        let found = find_cli();
        println!("tailscale command line: {found:?}");
        let Some(cli) = found else {
            println!("not installed on this machine — nothing to prove");
            return;
        };
        let (state, diagnostic) = detect_with(&cli);
        println!("state      = {}", state.token());
        println!("name       = {:?}", state.name());
        println!("origin     = {:?}", state.origin());
        println!("addresses  = {:?}", state.addresses());
        println!("diagnostic = {:?}", diagnostic.map(|d| d.label()));
        println!("sentence   = {}", state.sentence());

        // The one invariant that holds whatever the account is doing.
        assert_eq!(
            state.origin().is_some(),
            state.token() == "ready",
            "an origin was offered by a state that is not ready, or withheld by one that is"
        );
        // And a name exists exactly where the two states that have one say it does.
        assert_eq!(
            state.name().is_some(),
            matches!(state.token(), "ready" | "certificates-off"),
            "the name and the state disagree"
        );
    }

    #[test]
    fn a_command_line_that_is_not_there_is_a_state_rather_than_a_failure() {
        // TRUE ON THIS MACHINE TODAY, which is why it is worth asserting here rather than only in
        // the record: nothing panics, nothing returns `Err`, and no origin is offered.
        let (state, diagnostic) = detect_with(Path::new("/nonexistent/tailscale"));
        assert_eq!(state.token(), "not-running");
        assert_eq!(diagnostic, Some(Diagnostic::Unreadable));
        assert_eq!(state.origin(), None);
    }
}
