//! **NOTHING GOES OUTBOUND.** The CEO's constraint on the feedback channel's v1 half,
//! asserted mechanically instead of promised in a comment.
//!
//! # Why this suite is not in `feedback.rs`
//!
//! Most of what follows reads the feedback module's own source and fails on tokens that
//! would indicate a way off this machine. If the check lived inside the file it reads,
//! its own list of banned tokens would be in the text being scanned, and the test would
//! either fail against itself or have to be written in an obfuscated way that nobody can
//! review. It lives one directory over, where the list can be plain.
//!
//! # What "no outbound path" is taken to mean, stated so it can be argued with
//!
//! Three separate claims, each with its own test, because they fail independently:
//!
//! 1. **The feature contains no transport.** No socket, no client, no address, no
//!    subprocess — checked against the module's code with its comments removed, so the
//!    words in the prose above cannot make the check pass or fail.
//! 2. **The crate could not reach the network if it wanted to.** `richos-core` depends on
//!    four crates, none of which can open a connection. A dependency added later fails
//!    this test by name, which is the point: the person adding it has to say what it is.
//! 3. **Nothing else in the crate consumes the feature.** The module is reachable only
//!    from the crate root. It is not wired into the native client, the spine, or anything
//!    else that already talks to a subprocess, so there is no existing pipe for a report
//!    to be handed to.
//!
//! And one behavioral claim: recording an approval touches exactly one file and leaves
//! no second artefact — no spool, no marker, no "unsent" shard that a later version could
//! find and flush.

use richos_core::feedback::*;
use std::collections::BTreeSet;

/// The feedback module's source, embedded at compile time so this test cannot be fooled
/// by a working directory.
const FEEDBACK_SOURCE: &str = include_str!("../src/feedback.rs");

/// The crate manifest, same reasoning.
const MANIFEST: &str = include_str!("../Cargo.toml");

/// Everything a report could be handed to, or handed through.
///
/// Matched against lower-cased code with comments removed. The list is deliberately
/// broader than "things that open a socket": a subprocess is a transport, an outbox is a
/// transport with a delay on it, and both are the shapes a "just wire it up" commit
/// reaches for first.
///
/// **String literals are in scope on purpose** — an address is a literal, so exempting
/// them would exempt the thing most worth finding. The price is real and was paid on the
/// first run: this list rejected the module's own user-facing heading, which said RichOS
/// had no way to "transmit" the report. The copy was reworded, not the check. Product
/// copy in this module may not use transport vocabulary verbatim, and that is the cheaper
/// side of the trade.
const BANNED_IN_CODE: &[&str] = &[
    // A connection, by any name.
    "tcpstream",
    "tcplistener",
    "udpsocket",
    "socket",
    "std::net",
    "toserveraddrs",
    "reqwest",
    "hyper",
    "ureq",
    "isahc",
    "attohttpc",
    "curl",
    "http",
    "url",
    "websocket",
    "grpc",
    "dns",
    // A subprocess is a transport too — `native.rs` in this same crate talks to another
    // program this way.
    "command",
    "process::",
    "spawn",
    // A queue is a transport with a delay on it. This is the class the CEO named: "no
    // queue that something else could later flush."
    "outbox",
    "spool",
    "enqueue",
    "dequeue",
    "unsent",
    "transmit",
    "upload",
    "telemetry",
    "analytics",
    "beacon",
    "endpoint",
    "webhook",
    "bearer",
    "api_key",
];

/// Strip comments and the in-file test module, leaving only shipping code.
///
/// Comments must go or the module's own doc prose — which discusses transports at
/// length, deliberately — would trip every needle. The test module must go because its
/// fixtures build JSON by hand and use `std::process::id()` for temp paths; neither is a
/// path off this machine, and neither ships.
fn shipping_code(src: &str) -> String {
    let cut = src.find("#[cfg(test)]").unwrap_or(src.len());
    src[..cut]
        .lines()
        .map(|line| match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase()
}

#[test]
fn the_feedback_module_contains_no_transport_of_any_kind() {
    let code = shipping_code(FEEDBACK_SOURCE);
    // Guard the guard: if the strip ever eats the whole file, every needle passes
    // vacuously and this suite reports green over nothing.
    assert!(
        code.contains("pub fn render_disclosure"),
        "comment/test stripping removed the code it was supposed to check"
    );
    for needle in BANNED_IN_CODE {
        assert!(
            !code.contains(needle),
            "the feedback module's shipping code contains {needle:?} — \
             v1 has no outbound path and must not acquire one by accident"
        );
    }
}

#[test]
fn the_crate_depends_on_nothing_that_could_open_a_connection() {
    // Not "the feedback module does not call the network" — the stronger claim that
    // there is nothing here to call. serde/serde_json are data formats, uuid generates
    // identifiers, thiserror derives error types. None can reach a host.
    //
    // **`sha2` ARRIVED 2026-09-01 AND IS ANSWERED, NOT WAVED THROUGH.** It is here for
    // `setup.rs`, which pins the fetched engine release by SHA-256. Why it cannot reach a
    // host, stated rather than assumed: it is a pure-Rust hash implementation over
    // `&[u8]`, it pulls `digest`/`block-buffer`/`crypto-common`/`generic-array`/`typenum`
    // and `cpufeatures`, and not one of those links a socket, a TLS stack, or `std::net`.
    // `sha2` is `#![no_std]`-capable, which is a stronger statement than "we checked" — a
    // crate that compiles without `std` cannot be opening a connection.
    //
    // `time` supplies RFC3339 parsing for Claude quota reset timestamps. Its enabled
    // parsing feature operates on input bytes and date/time values, with no socket,
    // HTTP client or TLS dependency. It is not called by the feedback path.
    //
    // `unicode-normalization` does Unicode-normalized name matching. It is a pure-Rust text
    // library over `&str`/`char` data tables, with no socket, HTTP client or TLS dependency.
    //
    // `zstd` decodes local Codex archives. Its runtime dependency chain is `zstd-safe`
    // and `zstd-sys`, the bindings to the bundled compression library. The decoder takes
    // bytes from a caller-supplied reader and brings no networking or TLS dependency.
    // The feedback path does not call it.
    //
    // AND THE THING THAT MAKES THIS SAFE IS WHERE THE NETWORK ACTUALLY IS. `setup.rs`
    // downloads, and it does so through `/usr/bin/curl` — a subprocess, spawned only by
    // `CurlFetcher`, which the feedback channel neither constructs nor can reach. The
    // guarantee this test protects is unchanged: nothing in this crate opens a connection
    // from inside this crate's own process.
    let mut found: BTreeSet<&str> = BTreeSet::new();
    let mut in_deps = false;
    for line in MANIFEST.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_deps = t == "[dependencies]";
            continue;
        }
        if !in_deps || t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((name, _)) = t.split_once('=') {
            found.insert(name.trim());
        }
    }
    let expected: BTreeSet<&str> =
        ["serde", "serde_json", "uuid", "thiserror", "sha2", "time", "unicode-normalization", "zstd"].into_iter().collect();
    assert_eq!(
        found, expected,
        "richos-core's dependency set changed. That is not automatically wrong — but the \
         feedback channel's 'nothing goes outbound' guarantee rests on this crate having \
         no network-capable dependency, so state what the new one is and why it cannot \
         reach a host, then update this list."
    );
    assert!(
        !MANIFEST.contains("[dev-dependencies]"),
        "a dev-dependency arrived; check it the same way and update this assertion"
    );
}

#[test]
fn no_other_module_in_the_crate_consumes_the_feedback_feature() {
    // The module is reachable from the crate root and nowhere else. So a report cannot be
    // slipped into a pipe that already exists — notably native.rs, which does talk to
    // another program.
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut consumers: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for entry in std::fs::read_dir(&src_dir).expect("src/ is readable") {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        if name == "lib.rs" || name == "feedback.rs" {
            continue;
        }
        checked += 1;
        let text = std::fs::read_to_string(&path).unwrap();
        if text.contains("feedback::") || text.contains("crate::feedback") {
            consumers.push(name);
        }
    }
    assert!(checked > 10, "only {checked} modules scanned — the walk found the wrong directory");
    assert!(
        consumers.is_empty(),
        "the feedback module is consumed by {consumers:?}. That may be legitimate, but it \
         means a report can now reach code this suite has not checked for a transport."
    );
}

#[test]
fn an_approved_report_lands_in_one_local_file_and_leaves_nothing_else_behind() {
    // The behavioral half. Whatever the source says, this watches the filesystem: one
    // file, containing the payload, with no sibling for anything to pick up later.
    let dir = std::env::temp_dir().join(format!(
        "richos-feedback-outbound-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = dir.join("feedback.jsonl");
    let store = FeedbackStore::open(&path).unwrap();

    let payload = FeedbackPayload::assemble(
        Rating::Bad,
        FailureClass::UnpreparedTaskHandedToUser,
        Occurrences::ThreeTimes,
        vec![DiagnosisTerm::NoInputArtifactNamed, DiagnosisTerm::NoMethodGiven],
        vec![ContributingCondition::NoUserFacingItemCarriedAnAcceptanceCriterion],
    )
    .unwrap();
    let entry = FeedbackEntry::new(PromptOutcome::Rated(Rating::Bad))
        .with_report(Disclosure::of(payload).approve())
        .unwrap();
    store.record(&entry).unwrap();

    let siblings: BTreeSet<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        siblings,
        ["feedback.jsonl".to_string()].into_iter().collect::<BTreeSet<_>>(),
        "an approval produced something besides the one local file"
    );

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("unprepared-task-handed-to-user"));
    // Nothing in the stored line is addressed to anywhere, and nothing marks it as owed.
    for shape in ["http", "://", "@", "host", "pending", "queued", "sent", "attempt"] {
        assert!(
            !written.contains(shape),
            "the stored record contains {shape:?}: {written}"
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

// =======================================================================================
// THE SURFACE, ADDED 2026-08-30 — the same claim, over the code that now reaches the module
// =======================================================================================
//
// Everything above this line was written when `feedback.rs` had no caller: `grep -n feedback
// app/src-tauri/src/main.rs` returned nothing and `grep -rn feedback app/ui/main.js` returned
// nothing. "There is no transport in the module" was then the whole of the claim, because
// the module was the whole of the feature.
//
// It is not any more. Six Tauri commands and a web surface now reach it, and an outbound
// path added THERE would satisfy every assertion above while sending exactly the thing the
// CEO said must not be sent. So the claim is extended rather than restated: the checks below
// read the shell's own source and the shipped web layer, and they JOIN this suite rather
// than sitting beside it, so "nothing goes outbound" keeps meaning the whole feature.
//
// TEXT, NOT A DEPENDENCY. These are `include_str!`s. `richos-core` gains no build-time
// relationship to the Tauri shell — the crate stays native-dep-free and fast to test, which
// is the reason `src-tauri/` is detached from the workspace in the first place. The cost is
// that the paths are relative to this file, so the suite is coupled to the tree's shape; that
// is deliberate and is the cheaper side of the trade.

/// The Tauri shell, embedded at compile time.
const SHELL_SOURCE: &str = include_str!("../../../src-tauri/src/main.rs");

/// The whole shipped web layer. Not "the feedback part of it": a transport anywhere in a
/// file the feedback surface lives inside is reachable from that surface, and drawing a
/// boundary inside `main.js` would be a boundary guessed once instead of derived.
const UI_MAIN_JS: &str = include_str!("../../../ui/main.js");
const UI_TIMELINE_JS: &str = include_str!("../../../ui/timeline.js");
const UI_INDEX_HTML: &str = include_str!("../../../ui/index.html");
/// The two files the theme/settings work added (2026-08-31). Included so the claim this
/// suite makes — "there is nothing in the renderer to send with" — keeps covering the whole
/// renderer rather than the three files it happened to name on the day it was written.
const UI_THEME_BOOT_JS: &str = include_str!("../../../ui/theme-boot.js");
const UI_SETTINGS_BUTTON_JS: &str = include_str!("../../../ui/settings-button.js");
/// Bust a bug's renderer (round 21). It files GitHub issues, and it does so ONLY through the
/// shell's `bug_report_*` commands: the request is made in Rust, so this file has nothing to
/// send with, and this scan holds it to that.
const UI_BUG_REPORT_JS: &str = include_str!("../../../ui/bug-report.js");

/// Everything a report could be handed to, or handed through, IN THE SHELL LAYER.
///
/// A DIFFERENT LIST FROM `BANNED_IN_CODE`, and the difference is worth stating rather than
/// leaving as an inconsistency. That list bans the bare words `command`, `spawn` and
/// `process::`, which is right for a module that has no business containing any of them.
/// This layer is a Tauri command layer: every one of its functions is annotated
/// `#[tauri::command]`, and banning the word would ban the feature. So the needles here are
/// the things that actually START a process or open a connection, not the vocabulary around
/// them.
const BANNED_IN_SHELL: &[&str] = &[
    // A connection.
    "tcpstream",
    "tcplistener",
    "udpsocket",
    "std::net",
    "reqwest",
    "hyper",
    "ureq",
    "curl",
    "http://",
    "https://",
    "websocket",
    // A subprocess, by the things that actually make one.
    "command::new",
    "std::process::command",
    "tokio::process",
    "stdio::",
    // A queue is a transport with a delay on it.
    "outbox",
    "spool",
    "enqueue",
    "unsent",
    "transmit",
    "upload",
    "telemetry",
    "analytics",
    "beacon",
    "endpoint",
    "webhook",
    "bearer",
    "api_key",
];

/// Every network primitive the web layer could reach for. Identifiers, deliberately — a
/// product sentence may say "send" (the composer's own button does), and banning the word
/// would ban the copy rather than the capability.
const BANNED_IN_WEB: &[&str] = &[
    "fetch(",
    "xmlhttprequest",
    "websocket",
    "eventsource",
    "sendbeacon",
    "rtcpeerconnection",
    "importscripts",
    "new worker",
    "serviceworker",
    "http://",
    "https://",
    // A form is a transport with a submit button on it — but only if it has somewhere to
    // go. `<form>` itself is NOT banned, and that is a DELIBERATE NARROWING made when this
    // check first ran red on `index.html`: the composer is a `<form id="composer">`, which
    // is the right element for "type a sentence and press Enter" and which posts nowhere.
    // What makes a form a transport is a destination, so the destination is what is banned,
    // and the test below pins the composer as the only form there is.
    "action=",
    "formaction",
    "formdata",
];

/// The brace-balanced body of `fn name`, from its signature to its closing brace.
///
/// String and char literals are skipped so a `"}"` inside a sentence cannot close a function
/// early, and comments are skipped so a `{` in prose cannot open one — the same rules
/// `ui/tests/lib/state-strings.js` applies for the same reason.
fn function_body(src: &str, name: &str) -> Option<String> {
    let needle = format!("fn {name}");
    let start = src.find(&needle)?;
    let bytes: Vec<char> = src[start..].chars().collect();
    let mut depth = 0usize;
    let mut opened = false;
    let mut out = String::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '/' && bytes.get(i + 1) == Some(&'/') {
            while i < bytes.len() && bytes[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '"' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == '\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == '"' {
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        out.push(c);
        if c == '{' {
            opened = true;
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if opened && depth == 0 {
                return Some(out);
            }
        }
        i += 1;
    }
    None
}

/// Every function in the shell that a `feedback_*` command can reach, WITHIN THAT FILE.
///
/// Derived rather than typed, and transitively: start from every `#[tauri::command] fn
/// feedback_*` the file declares, pull in every identifier they call that is also a `fn`
/// declared in the same file, and repeat until nothing new arrives. A helper added tomorrow
/// is scanned tomorrow, with nobody remembering to add it here — which is the difference
/// between a check and a list.
fn feedback_reachable_source() -> (Vec<String>, Vec<String>, String) {
    let declared: std::collections::BTreeSet<String> = SHELL_SOURCE
        .match_indices("fn ")
        .filter_map(|(i, _)| {
            let rest = &SHELL_SOURCE[i + 3..];
            let name: String =
                rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        })
        .collect();

    // THE SEED IS THE REGISTRATION LIST, not "every fn whose name starts with feedback_".
    // That is the authority over what the webview can actually call, and the difference is
    // not academic: the first version of this seeded on the name prefix and swept in
    // `feedback_store`, a private helper, which made the command count wrong by one and
    // would have gone on being wrong every time a helper was named tidily.
    let handler = {
        let from = SHELL_SOURCE.find("generate_handler![").expect("no generate_handler! in the shell");
        let to = SHELL_SOURCE[from..].find(']').expect("generate_handler! is not closed") + from;
        &SHELL_SOURCE[from..to]
    };
    let mut frontier: Vec<String> = declared
        .iter()
        .filter(|n| n.starts_with("feedback_"))
        .filter(|n| handler.contains(&format!("{n},")) || handler.contains(&format!("{n}\n")))
        .cloned()
        .collect();
    let commands: Vec<String> = {
        let mut c = frontier.clone();
        c.sort();
        c
    };
    let mut seen: std::collections::BTreeSet<String> = frontier.iter().cloned().collect();
    let mut text = String::new();
    while let Some(name) = frontier.pop() {
        let Some(body) = function_body(SHELL_SOURCE, &name) else { continue };
        for callee in &declared {
            // A call site: the name followed by `(`. Bounded on the left so `feedback_store`
            // does not match inside `my_feedback_store`.
            let call = format!("{callee}(");
            if body.contains(&call) && !seen.contains(callee) {
                seen.insert(callee.clone());
                frontier.push(callee.clone());
            }
        }
        text.push_str(&body);
        text.push('\n');
    }
    (commands, seen.into_iter().collect(), text.to_lowercase())
}

#[test]
fn the_feedback_commands_and_everything_they_call_contain_no_transport() {
    let (commands, reached, code) = feedback_reachable_source();
    // Guard the guard, twice. An extraction that found nothing would pass every needle
    // vacuously, and a closure that found ONLY the seed would mean the call-graph walk is
    // not walking. `commands` is the REGISTERED set; `reached` is that plus everything they
    // call, which is why the two are counted separately — `feedback_store` is a private
    // helper and belongs in the second, never the first.
    assert_eq!(
        commands.len(),
        6,
        "expected 6 feedback commands in the shell, found {commands:?} — if a command was \
         added or removed, this number is the place to say so"
    );
    assert!(
        reached.len() > commands.len(),
        "the call-graph walk reached no helpers at all ({reached:?}), so it is not walking"
    );
    assert!(
        code.contains("feedbackstore::open") || code.contains("state.feedback"),
        "the extracted code does not contain the store access it must — the walk found the \
         wrong text"
    );

    for needle in BANNED_IN_SHELL {
        assert!(
            !code.contains(needle),
            "the feedback command layer (and everything it calls: {reached:?}) contains \
             {needle:?} — v1 has no outbound path and must not acquire one by accident"
        );
    }
}

#[test]
fn the_command_that_records_an_approval_still_checks_what_the_user_was_shown() {
    // THE POSITIVE HALF, in the same suite as the absences, because it protects the same
    // constraint from the other side. In one process "he saw exactly this" is structural:
    // `ApprovedReport` has no public constructor and the only route to one is
    // `Disclosure::approve`, which cannot exist without having rendered its text. That does
    // NOT survive an IPC boundary — a webview can post any `shown` it likes — so the command
    // re-renders the selection and compares. Deleting that comparison would leave every
    // other test in this file green while making it possible to record consent for text
    // nobody ever read.
    let body = function_body(SHELL_SOURCE, "feedback_record")
        .expect("feedback_record is not in the shell source");
    let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("if disclosure.full_text() != *shown"),
        "feedback_record no longer compares the rendered report against what the webview says \
         it showed him. Restore it, or state here what replaced it: {flat}"
    );
    assert!(
        flat.contains("return Err(FEEDBACK_PREVIEW_MISMATCH"),
        "the mismatch is detected and not refused"
    );
    // ...and the only route to an approval is still the one that had to render the text.
    assert!(
        flat.contains("disclosure.approve()"),
        "an approval is being constructed by something other than Disclosure::approve"
    );
}

/// Remove the XML NAMESPACE IDENTIFIERS before scanning for network primitives.
///
/// A DELIBERATE NARROWING, in the spirit of the `action=` one above and made for the same
/// reason: this check ran red on 2026-08-31 when the RichOS wordmark was inlined into
/// `index.html` as SVG, and what it caught was `xmlns="http://www.w3.org/2000/svg"`.
///
/// That string is not a network primitive and cannot become one. An XML namespace is
/// IDENTIFIED by a URI and is never dereferenced — no user agent fetches it — and the SVG
/// specification requires this exact literal. `document.createElementNS` takes the same
/// literal for the same reason, which is why `settings-button.js` holds it as a constant.
/// Rewriting it to dodge a checker would be obfuscation, and obfuscation is the one response
/// to a guard that is worse than deleting it.
///
/// THE NARROWING IS TWO EXACT STRINGS, not a pattern and not a syntax. Only the literal SVG
/// and xlink namespace URIs are removed, in any position; every other URL in these files
/// still fails, including an `xmlns` pointing anywhere else.
/// `a_planted_namespace_shaped_url_is_still_caught` is the proof, and it is the reason this
/// exemption can be trusted not to widen quietly.
const XML_NAMESPACE_LITERALS: &[&str] =
    &["http://www.w3.org/2000/svg", "http://www.w3.org/1999/xlink"];

/// The literal is only a namespace when it ENDS there. `http://www.w3.org/2000/svg/spec`
/// is a link to a document, not the namespace, and a plain substring replace would swallow
/// the scheme and leave `/spec` behind — the URL would vanish from the scan while remaining
/// in the file. `a_planted_namespace_shaped_url_is_still_caught` found exactly that in the
/// first draft of this function, which is the entire argument for writing the positive
/// control before trusting the exemption.
fn strip_xml_namespaces(lower: &str) -> String {
    let mut out = String::with_capacity(lower.len());
    let mut rest = lower;
    'outer: while !rest.is_empty() {
        for ns in XML_NAMESPACE_LITERALS {
            if let Some(after) = rest.strip_prefix(ns) {
                // A URI-path character here means this is a longer URL that merely starts
                // with the namespace, so it is NOT exempt and is copied through untouched.
                let continues = after
                    .chars()
                    .next()
                    .is_some_and(|c| c == '/' || c == '?' || c == '#' || c.is_alphanumeric());
                if !continues {
                    out.push_str("xml-namespace-elided");
                    rest = after;
                    continue 'outer;
                }
            }
        }
        let mut chars = rest.chars();
        let c = chars.next().expect("rest is non-empty");
        out.push(c);
        rest = chars.as_str();
    }
    out
}

#[test]
fn a_planted_namespace_shaped_url_is_still_caught() {
    // The narrowing above is only safe if it is narrow, so this is the positive control:
    // shapes that LOOK like the exempted one and are not. Without this, "strip the namespace"
    // is a sentence someone could later widen into "strip the URLs".
    assert!(
        !strip_xml_namespaces(r#"<svg xmlns="http://www.w3.org/2000/svg">"#).contains("http://"),
        "the genuine SVG namespace must be elided, or the wordmark cannot ship"
    );
    for planted in [
        // An xmlns pointing somewhere that is not the SVG namespace.
        r#"<svg xmlns="http://telemetry.example.com/ns">"#,
        // The real namespace with a real URL beside it.
        r#"<svg xmlns="http://www.w3.org/2000/svg" data-x="https://evil.example.com">"#,
        // A w3.org URL that is a LINK rather than a namespace — one character of path
        // different from the exempted literal.
        r#"<a href="http://www.w3.org/2000/svg/spec">the spec</a>"#,
        // https where the namespace is http.
        r#"<svg xmlns="https://www.w3.org/2000/svg">"#,
    ] {
        let stripped = strip_xml_namespaces(planted);
        assert!(
            stripped.contains("http://") || stripped.contains("https://"),
            "a URL that is not one of the two exempted namespace literals survived the \
             narrowing: {planted}"
        );
    }
}

#[test]
fn the_shipped_web_layer_contains_no_network_primitive_at_all() {
    // The STRONGER claim, and the reason it is available: the whole of `app/ui/` has never
    // had one. So this is not "the feedback panel does not send" with a boundary drawn
    // around it — it is "there is nothing in the renderer to send with", which no future
    // edit to the feedback surface can satisfy by moving code one function over.
    for (name, src) in [
        ("main.js", UI_MAIN_JS),
        ("timeline.js", UI_TIMELINE_JS),
        ("index.html", UI_INDEX_HTML),
        ("theme-boot.js", UI_THEME_BOOT_JS),
        ("settings-button.js", UI_SETTINGS_BUTTON_JS),
        ("bug-report.js", UI_BUG_REPORT_JS),
    ] {
        let lower = strip_xml_namespaces(&src.to_lowercase());
        assert!(lower.len() > 2000, "{name} did not load — the include is pointing at nothing");
        for needle in BANNED_IN_WEB {
            assert!(
                !lower.contains(needle),
                "{name} contains {needle:?}. The feedback surface lives in this layer, and \
                 v1 has no outbound path — if this arrived for something unrelated, say so \
                 here and narrow the check deliberately rather than deleting it."
            );
        }
    }
    // THE ONE FORM, PINNED. `<form` is allowed above because the composer is one; this is
    // what stops that allowance from being a hole. One form, no destination, and it is the
    // composer — a second one, or an `action` on this one, fails here rather than passing
    // through the narrowed needle list.
    assert_eq!(
        UI_INDEX_HTML.matches("<form").count(),
        1,
        "a second form appeared in the shipped markup; the needle list allows `<form` only \
         because the composer is the only one"
    );
    assert!(
        UI_INDEX_HTML.contains(r#"<form id="composer" autocomplete="off">"#),
        "the one form is not the composer, or it has grown attributes worth reading"
    );

    // Guard the guard: the feedback surface really is in the file being scanned.
    assert!(
        UI_MAIN_JS.contains("feedback_record") && UI_INDEX_HTML.contains("feedback-overlay"),
        "the surface is not in the sources this test scans, so it proves nothing about it"
    );
}
