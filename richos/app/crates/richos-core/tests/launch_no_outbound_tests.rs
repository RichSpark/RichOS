//! **THE LAUNCH RECORD NEVER LEAVES THIS MACHINE.** Asserted mechanically, not promised.
//!
//! The CEO ruled on 2026-08-31 that the splash's tracking data is *"local only, never
//! outbound — the same guarantee the feedback channel carries"*, and named the reason it has
//! to be held to the identical standard: it is a record of his own working life. When he
//! opens the app, how often, how long he has had it, what he has been shown. A product that
//! sent that anywhere would be doing the one thing this product exists not to do.
//!
//! # Why this suite is not in `launch.rs`
//!
//! Same reason `feedback_no_outbound_tests.rs` is not in `feedback.rs`: most of what follows
//! reads the module's own source and fails on tokens that would indicate a way off this
//! machine. A check that lived inside the file it reads would either fail against itself or
//! have to be written so obliquely that nobody could review it.
//!
//! # Why it is a SECOND suite rather than four more tests in the feedback one
//!
//! Because they are two different claims about two different call graphs, and folding them
//! together would make each one's failure ambiguous. They are deliberately the same SHAPE —
//! this file reuses that one's structure and its needle lists almost verbatim — so a reader
//! who has understood one has understood both.
//!
//! # The four claims, each failing independently
//!
//! 1. **The module contains no transport.** No socket, no client, no address, no
//!    subprocess — checked against `launch.rs` with its comments removed, so the prose above
//!    cannot make the check pass or fail.
//! 2. **The crate could not reach the network if it wanted to.** Four dependencies, none
//!    network-capable. A fifth fails this by name.
//! 3. **Nothing else in the crate consumes it,** so the record cannot be slipped into a pipe
//!    that already exists — `native.rs` in this same crate does talk to another program.
//! 4. **The shell's two launch commands, and everything they call, contain no transport,**
//!    and neither does the web layer that reaches them. The module having no way out would
//!    mean nothing if the surface on top of it had one.
//!
//! And one behavioral claim: a full run — begin, show a splash, fire a reward, quit —
//! leaves exactly one file and no second artefact for a later version to find and flush.

use richos_core::launch::*;
use std::collections::BTreeSet;

/// The launch module's source, embedded at compile time so this test cannot be fooled by a
/// working directory.
const LAUNCH_SOURCE: &str = include_str!("../src/launch.rs");

/// The crate manifest, same reasoning.
const MANIFEST: &str = include_str!("../Cargo.toml");

/// Everything the record could be handed to, or handed through.
///
/// The same list `feedback_no_outbound_tests.rs` uses, and deliberately so: the two features
/// make the same promise, and two lists that drifted apart would mean one of them was
/// quietly weaker. String literals are in scope on purpose — an address is a literal, so
/// exempting them would exempt the thing most worth finding.
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
    // A subprocess is a transport too.
    "command",
    "process::",
    "spawn",
    // A queue is a transport with a delay on it.
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
/// The comments must go: `launch.rs` discusses processes, kills and markers at length and
/// would trip half the list on its own prose. The test module must go because its fixtures
/// build temp paths from `std::process::id()`, which is not a path off this machine and
/// does not ship.
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
fn the_launch_module_contains_no_transport_of_any_kind() {
    let code = shipping_code(LAUNCH_SOURCE);
    // Guard the guard: if the strip ever eats the whole file, every needle passes vacuously
    // and this suite reports green over nothing.
    assert!(
        code.contains("pub fn begin_run"),
        "comment/test stripping removed the code it was supposed to check"
    );
    assert!(
        code.contains("pub fn note_splash_shown"),
        "comment/test stripping removed the code it was supposed to check"
    );
    for needle in BANNED_IN_CODE {
        assert!(
            !code.contains(needle),
            "the launch module's shipping code contains {needle:?} — this record is local \
             only and must not acquire a way off the machine by accident"
        );
    }
}

#[test]
fn the_crate_depends_on_nothing_that_could_open_a_connection() {
    // Not "the launch module does not call the network" — the stronger claim that there is
    // nothing here to call. Duplicated from the feedback suite ON PURPOSE: if that file is
    // ever deleted, this guarantee must not go with it.
    //
    // **`sha2` ARRIVED 2026-09-01.** The reasoning is written out once, in
    // `feedback_no_outbound_tests.rs`, and it is short enough to restate: pure-Rust SHA-256
    // for `setup.rs`'s engine pin, `no_std`-capable, and no dependency of it links a socket
    // or a TLS stack. Where the network actually is, is `/usr/bin/curl` — a subprocess,
    // spawned only by `CurlFetcher`, which nothing in this module can reach.
    //
    // `time` now parses provider RFC3339 quota reset timestamps, including their
    // offsets. Its parsing feature works on bytes and date/time values without a
    // socket, HTTP client or TLS dependency. The launch record still uses its own
    // local-calendar arithmetic and cannot reach the quota transport.
    //
    // `unicode-normalization` does Unicode-normalized name matching: a pure-Rust text
    // library over `&str`/`char` data tables, with no socket, HTTP client or TLS dependency.
    // `zstd` reads local archive bytes through `zstd-safe` and the bundled decoder in
    // `zstd-sys`. See the dependency audit in `feedback_no_outbound_tests.rs`.
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
        "richos-core's dependency set changed. The launch record's 'never outbound' \
         guarantee rests on this crate having no network-capable dependency. \
         State what the new one is and why \
         it cannot reach a host, then update this list."
    );
}

#[test]
fn no_other_module_in_the_crate_consumes_the_launch_record() {
    // Reachable from the crate root and nowhere else, so the record cannot be handed to a
    // pipe that already exists — notably native.rs, which does talk to another program.
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut consumers: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for entry in std::fs::read_dir(&src_dir).expect("src/ is readable") {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        if name == "lib.rs" || name == "launch.rs" {
            continue;
        }
        checked += 1;
        let text = std::fs::read_to_string(&path).unwrap();
        if text.contains("launch::") || text.contains("crate::launch") {
            consumers.push(name);
        }
    }
    assert!(checked > 10, "only {checked} modules scanned — the walk found the wrong directory");
    assert!(
        consumers.is_empty(),
        "the launch record is consumed by {consumers:?}. That may be legitimate, but it means \
         his usage history can now reach code this suite has not checked for a transport."
    );
}

#[test]
fn a_full_run_lands_in_one_local_file_and_leaves_nothing_else_behind() {
    // The behavioral half. Whatever the source says, this watches the filesystem across a
    // WHOLE session — begin, draw, reward, quit — because the write path uses a temporary
    // file and a rename, and a temporary that survived would be a partial history sitting on
    // disk waiting for something to find it.
    let dir = std::env::temp_dir().join(format!(
        "richos-launch-outbound-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("launches.json");

    let now = 1_788_166_800_000u64; // 2026-08-31T09:00:00Z
    let mut store = LaunchStore::open(&path, now).unwrap();
    assert_eq!(store.begin_run(now, "1234", PriorRun::Unknown).unwrap(), LaunchKind::Fresh);
    store.note_splash_shown("round-10-1/v3").unwrap();
    store.note_reward_fired("thirty-days", now).unwrap();
    store.note_clean_exit().unwrap();

    let siblings: BTreeSet<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        siblings,
        ["launches.json".to_string()].into_iter().collect::<BTreeSet<_>>(),
        "a run produced something besides the one local file"
    );

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("round-10-1/v3"), "the positive probe: the ring really was written");
    // Nothing in the stored record is addressed anywhere, and nothing marks it as owed.
    for shape in ["http", "://", "@", "host", "pending", "queued", "sent", "attempt"] {
        assert!(!written.contains(shape), "the stored record contains {shape:?}: {written}");
    }

    std::fs::remove_dir_all(&dir).ok();
}

// =======================================================================================
// THE SURFACE — the same claim, over the code that reaches the module
// =======================================================================================
//
// Two Tauri commands and a web surface reach the record. An outbound path added THERE would
// satisfy every assertion above while sending exactly the thing the CEO said must not be
// sent. So the claim is extended rather than restated.
//
// TEXT, NOT A DEPENDENCY. These are `include_str!`s, so `richos-core` gains no build-time
// relationship to the Tauri shell and stays native-dep-free and fast to test. The cost is
// coupling to the tree's shape, which is the cheaper side of the trade.

/// The Tauri shell, embedded at compile time.
const SHELL_SOURCE: &str = include_str!("../../../src-tauri/src/main.rs");

/// The web files that read or write the launch record. `splash.js` is here and is NOT
/// covered by the feedback suite's web check, which names five files and not this one — so
/// without this line the file that actually decides what to draw at launch would be scanned
/// by nothing.
const UI_SPLASH_JS: &str = include_str!("../../../ui/splash.js");
const UI_SPLASH_LIBRARY_JS: &str = include_str!("../../../ui/splash-library.js");
const UI_MAIN_JS: &str = include_str!("../../../ui/main.js");

/// Everything the record could be handed through IN THE SHELL LAYER.
///
/// A different list from `BANNED_IN_CODE`, for the reason the feedback suite states: this is
/// a Tauri command layer, every function in it is annotated `#[tauri::command]`, and banning
/// the bare word `command` would ban the feature. The needles here are the things that
/// actually start a process or open a connection.
const BANNED_IN_SHELL: &[&str] = &[
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
    "command::new",
    "std::process::command",
    "tokio::process",
    "stdio::",
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

/// Every network primitive the web layer could reach for. Identifiers, deliberately.
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
    "formdata",
];

/// The brace-balanced body of `fn name`, from its signature to its closing brace.
///
/// String and char literals are skipped so a `"}"` inside a sentence cannot close a function
/// early, and comments are skipped so a `{` in prose cannot open one.
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

/// The RAW source of `fn name`, string literals and all, from its signature to the first
/// closing brace in column zero.
///
/// A blunter reader than [`function_body`] and it exists for one function: the window
/// injection is a string literal, so the helper that removes string literals cannot be used
/// to check it. Column-zero termination is safe here because the shell's functions are all
/// top-level.
fn raw_function_text(src: &str, name: &str) -> Option<String> {
    let start = src.find(&format!("fn {name}"))?;
    let rest = &src[start..];
    let end = rest.find("\n}\n").map(|i| i + 2).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// Every function in the shell that a `launch_*` command can reach, WITHIN THAT FILE.
///
/// Derived rather than typed, and transitively: seed from the REGISTRATION LIST (which is
/// the authority over what the webview can actually call, not a name prefix), pull in every
/// identifier they call that is also a `fn` declared in the same file, repeat until nothing
/// new arrives. A helper added tomorrow is scanned tomorrow.
fn launch_reachable_source() -> (Vec<String>, Vec<String>, String) {
    let declared: BTreeSet<String> = SHELL_SOURCE
        .match_indices("fn ")
        .filter_map(|(i, _)| {
            let rest = &SHELL_SOURCE[i + 3..];
            let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        })
        .collect();

    let handler = {
        let from = SHELL_SOURCE.find("generate_handler![").expect("no generate_handler! in the shell");
        let to = SHELL_SOURCE[from..].find(']').expect("generate_handler! is not closed") + from;
        &SHELL_SOURCE[from..to]
    };
    let mut frontier: Vec<String> = declared
        .iter()
        .filter(|n| n.starts_with("launch_"))
        .filter(|n| handler.contains(&format!("{n},")) || handler.contains(&format!("{n}\n")))
        .cloned()
        .collect();
    let commands: Vec<String> = {
        let mut c = frontier.clone();
        c.sort();
        c
    };
    let mut seen: BTreeSet<String> = frontier.iter().cloned().collect();
    let mut text = String::new();
    while let Some(name) = frontier.pop() {
        let Some(body) = function_body(SHELL_SOURCE, &name) else { continue };
        for callee in &declared {
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
fn the_launch_commands_and_everything_they_call_contain_no_transport() {
    let (commands, reached, code) = launch_reachable_source();
    // Guard the guard. An extraction that found nothing would pass every needle vacuously.
    assert_eq!(
        commands,
        vec!["launch_note_splash_shown".to_string(), "launch_state".to_string()],
        "expected exactly the two registered launch commands, found {commands:?} — if one \
         was added or removed, this is the place to say so"
    );
    assert!(
        code.contains("state.launch"),
        "the extracted code does not reach the store it must — the walk found the wrong text"
    );
    for needle in BANNED_IN_SHELL {
        assert!(
            !code.contains(needle),
            "the launch command layer (and everything it calls: {reached:?}) contains \
             {needle:?} — this record is local only and must not acquire a way out"
        );
    }
}

#[test]
fn the_window_injection_carries_a_verdict_and_never_a_channel() {
    // `launch_init_script` is the ONE thing this feature puts into the page before any of
    // the page's own code runs, which makes it the single most sensitive line in the branch:
    // whatever it writes is trusted by `splash.js` on sight. So it is pinned to a literal —
    // a frozen object holding one string — rather than merely scanned for absences.
    // RAW text, deliberately not `function_body`: that helper strips string literals so a
    // `"}"` in a sentence cannot close a function early, and the entire subject of this test
    // IS a string literal. Reading the stripped body here would assert against an empty
    // `format!()` and pass whatever the injection actually says.
    let raw = raw_function_text(SHELL_SOURCE, "launch_init_script")
        .expect("launch_init_script is not in the shell source");
    let flat: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains("window.__RICHOS_LAUNCH__ = Object.freeze("),
        "the injected verdict is no longer frozen — something in the page could then make a \
         crash-restart claim to be a fresh launch: {flat}"
    );
    assert!(
        flat.contains("kind: {:?}") && flat.contains("kind.as_str()"),
        "the injection no longer carries just the kind, as the wire string `launch.rs` \
         defines it: {flat}"
    );
    for needle in ["addeventlistener", "settimeout", "invoke", "eval", "import"] {
        assert!(
            !flat.to_lowercase().contains(needle),
            "the injected script does more than state a fact ({needle:?}): {flat}"
        );
    }
}

#[test]
fn the_offset_is_the_callers_and_the_shell_never_reads_a_timezone() {
    // The local-bucketing ruling in structural form. `launch_state` takes the offset as a
    // parameter because the webview is the only layer that knows what local means; a shell
    // that reached for a timezone itself would be a second source of truth, and the two
    // would disagree the moment one of them was wrong.
    // THE SIGNATURE, not the body. The first version of this matched `utc_offset_minutes`
    // anywhere in the extracted text and a mutation that DELETED the parameter and declared
    // `let utc_offset_minutes = 0;` inside the function turned nothing red — the check was
    // reading the shadow of the thing it meant to guard. A parameter is a signature fact.
    let signature = SHELL_SOURCE
        .find("fn launch_state(")
        .map(|i| &SHELL_SOURCE[i..i + SHELL_SOURCE[i..].find(')').expect("unclosed signature")])
        .expect("launch_state is not in the shell");
    assert!(
        signature.contains("utc_offset_minutes: i32"),
        "launch_state no longer TAKES the caller's offset as a parameter: {signature}"
    );
    let body = function_body(SHELL_SOURCE, "launch_state").expect("launch_state is not in the shell");
    let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        !flat.contains("let utc_offset_minutes"),
        "the offset is being shadowed inside the function rather than taken from the \
         caller: {flat}"
    );
    let (_, _, code) = launch_reachable_source();
    for needle in ["localtime", "timezone", "tz_offset", "chrono", "local::now"] {
        assert!(
            !code.contains(needle),
            "the launch command layer reads a timezone itself ({needle:?}) — the offset is \
             the caller's, and two sources of truth for 'local' is one too many"
        );
    }
    // The web side really does negate it. `getTimezoneOffset()` is minutes to ADD to local
    // to reach UTC (+420 in California), and `launch.rs` takes offset-from-UTC-positive-east
    // (-420). A missing minus sign would put every bucket boundary fourteen hours out and
    // nothing would look broken.
    let flat_ui: String = UI_MAIN_JS.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat_ui.contains("const utcOffsetMinutes = -new Date().getTimezoneOffset();"),
        "main.js no longer negates getTimezoneOffset() before handing it to the record"
    );
}

/// Remove the XML NAMESPACE IDENTIFIERS before scanning for network primitives.
///
/// THE SAME NARROWING `feedback_no_outbound_tests.rs` makes, ported rather than referenced
/// because a test crate cannot import another test crate's items, and for the same reason it
/// was made there: `splash.js` builds the RichOS mark with `document.createElementNS`, which
/// takes `http://www.w3.org/2000/svg` as a required literal. That string is not a network
/// primitive and cannot become one — an XML namespace is IDENTIFIED by a URI and is never
/// dereferenced. Rewriting it to dodge a checker would be obfuscation, which is the one
/// response to a guard that is worse than deleting it.
///
/// TWO EXACT STRINGS, not a pattern. `a_namespace_shaped_url_is_still_caught_here_too` is the
/// proof, and it is the reason this exemption can be trusted not to widen quietly.
const XML_NAMESPACE_LITERALS: &[&str] =
    &["http://www.w3.org/2000/svg", "http://www.w3.org/1999/xlink"];

/// The literal is only a namespace when it ENDS there — `.../2000/svg/spec` is a link to a
/// document, and a plain substring replace would swallow the scheme and leave `/spec`, which
/// removes the URL from the scan while leaving it in the file.
fn strip_xml_namespaces(lower: &str) -> String {
    let mut out = String::with_capacity(lower.len());
    let mut rest = lower;
    'outer: while !rest.is_empty() {
        for ns in XML_NAMESPACE_LITERALS {
            if let Some(after) = rest.strip_prefix(ns) {
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
fn a_namespace_shaped_url_is_still_caught_here_too() {
    // The positive control for the narrowing. Without it, "strip the namespace" is a
    // sentence someone could later widen into "strip the URLs".
    assert!(
        !strip_xml_namespaces(r#"<svg xmlns="http://www.w3.org/2000/svg">"#).contains("http://"),
        "the genuine SVG namespace must be elided, or the mark cannot be drawn"
    );
    for planted in [
        r#"<svg xmlns="http://telemetry.example.com/ns">"#,
        r#"<svg xmlns="http://www.w3.org/2000/svg" data-x="https://evil.example.com">"#,
        r#"<a href="http://www.w3.org/2000/svg/spec">the spec</a>"#,
        r#"<svg xmlns="https://www.w3.org/2000/svg">"#,
    ] {
        let stripped = strip_xml_namespaces(planted);
        assert!(
            stripped.contains("http://") || stripped.contains("https://"),
            "a URL that is not one of the two exempted namespace literals survived: {planted}"
        );
    }
}

#[test]
fn the_web_layer_that_reads_the_record_contains_no_network_primitive() {
    // The renderer's half. `splash.js` decides what to draw at launch and `main.js` feeds
    // the ring; neither has any way to send what it knows.
    for (name, src) in [
        ("splash.js", UI_SPLASH_JS),
        ("splash-library.js", UI_SPLASH_LIBRARY_JS),
        ("main.js", UI_MAIN_JS),
    ] {
        let lower = strip_xml_namespaces(&src.to_lowercase());
        // Guard the guard: an empty include would pass every needle.
        assert!(lower.len() > 1_000, "{name} came back empty — the include path is wrong");
        for needle in BANNED_IN_WEB {
            assert!(
                !lower.contains(needle),
                "{name} contains {needle:?} — the launch record must have nothing in the \
                 renderer to be sent with"
            );
        }
    }
}

#[test]
fn a_planted_primitive_in_the_web_layer_would_be_caught() {
    // THE POSITIVE CONTROL for the test above, which would pass perfectly over three empty
    // strings. This proves the needles actually bite.
    let planted = format!("{UI_SPLASH_JS}\nfetch(\"https://example.invalid\");");
    let lower = strip_xml_namespaces(&planted.to_lowercase());
    let caught: Vec<&&str> = BANNED_IN_WEB.iter().filter(|n| lower.contains(**n)).collect();
    assert!(
        caught.len() >= 2,
        "a planted fetch() to an https address was caught by {caught:?} — expected at least \
         the two needles that describe it"
    );
}

#[test]
fn the_record_the_shell_hands_out_is_the_record_and_not_a_wider_one() {
    // WHAT LEAVES THE PROCESS, pinned. `LaunchStateView` is the only shape the webview ever
    // sees, and every field on it is one the CEO's ruling names. The danger is not a
    // transport appearing; it is this struct quietly growing a field — the raw `starts` log,
    // a machine id, a path — that a later surface then renders or hands on.
    //
    // Read off the STRUCT rather than the command body, because the struct is what serde
    // serializes and a field could be populated somewhere this walk never visits.
    let decl = SHELL_SOURCE
        .find("struct LaunchStateView {")
        .expect("LaunchStateView is not in the shell source");
    let open = SHELL_SOURCE[decl..].find('{').unwrap() + decl;
    let close = SHELL_SOURCE[open..].find("\n}").expect("LaunchStateView is not closed") + open;
    let fields: Vec<String> = SHELL_SOURCE[open + 1..close]
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("//") && !l.starts_with('#'))
        .filter_map(|l| l.split(':').next().map(|n| n.trim().to_string()))
        .filter(|n| !n.is_empty())
        .collect();
    assert_eq!(
        fields,
        vec!["kind", "counts", "installed_at", "recent_splashes", "readable", "schema_version"],
        "the shape handed to the webview changed. Adding a field is not automatically wrong \
         — say what it is and why the CEO's own usage history is safe carrying it."
    );
}
