//! **EVERY SEND CALL THAT REACHES A MODEL, AND THE ONE RULE ON EACH** (CEO 2026-11-26:
//! "Everything was either stopped, finished, ended or paused in the RichOS app. So, unless I'm
//! missing something major, there should have been absolutely zero usage after me just opening
//! the app.").
//!
//! Five review rounds each found another road from opening the app to a model, because each
//! fix guarded one source of input and none guarded the sends. This test lists the sends: every
//! production call, in richos-core and the app shell, of the calls that hand a session a turn
//! (`enqueue_prompt`, `send_input`, `send_observed_input`, `send_live_input`,
//! `keep_chat_alive`, `prompt`, and the spine's `prepare_delivery`), by file and the function
//! it is in, each with how the one rule (`richos_core::turn_gate`) covers it. A new call fails
//! here until it is listed, which means until someone has said how the rule covers it. The
//! review watcher's one start is pinned the same way in its Python source.
//!
//! It reads source text, as `turn_boundary_tests.rs` in the app does: what it pins is where the
//! calls are, which is a fact about the source.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SENDS: &[&str] = &[
    ".enqueue_prompt(", ".send_input(", ".send_observed_input(", ".send_live_input(",
    ".keep_chat_alive(", ".prompt(", "prepare_delivery(",
];

/// (file, function, call) -> how the rule covers it.
const EXPECTED: &[(&str, &str, &str, &str)] = &[
    // ---- Rich ------------------------------------------------------------------------------
    ("crates/richos-core/src/spine.rs", "deliver", "prepare_delivery(",
        "GATED: every turn of Rich's (his words, an agent's message, a recovered or held turn) is asked here"),
    ("crates/richos-core/src/spine.rs", "prepare_delivery_io", ".enqueue_prompt(",
        "BELOW: sends only a Delivery that prepare_delivery made"),
    ("crates/richos-core/src/spine.rs", "prepare_delivery", "prepare_delivery(",
        "DEFINITION: the function itself"),
    ("crates/richos-core/src/spine.rs", "retry_saved_input", "prepare_delivery(",
        "CARRIES ON: the overload/usage-limit retry of a turn deliver already let through"),
    ("crates/richos-core/src/spine.rs", "keep_front_desk_alive_with", ".keep_chat_alive(",
        "KEEP ALIVE: his own setting, due only for a kept chat with a request in the last hour (chat_cache::heartbeat_due)"),
    // ---- Stu -------------------------------------------------------------------------------
    ("crates/richos-core/src/work_host.rs", "run_assignment_message", ".enqueue_prompt(",
        "FED BY GATED SELECTION: run_message, from the run loop's Message (next_for_stu) and Answer (may_send) branches"),
    ("crates/richos-core/src/work_host.rs", "wake_messages", ".send_live_input(",
        "GATED: answers filtered by may_send, inbox input taken from next_for_stu"),
    ("crates/richos-core/src/work_host.rs", "send_live_input", ".send_input(",
        "BELOW: the helper's own provider call"),
    ("crates/richos-core/src/work_host.rs", "send_live_input", ".send_observed_input(",
        "BELOW: the helper's own provider call"),
    ("crates/richos-core/src/work_host.rs", "retry_live_after_usage_limit", ".send_input(",
        "CARRIES ON: the usage-limit resend of a live input already let through"),
    ("crates/richos-core/src/work_host.rs", "live_completion_with", ".send_input(",
        "CARRIES ON: the overload retry of a live input already let through"),
    ("crates/richos-core/src/work_host.rs", "keep_alive", ".keep_chat_alive(",
        "KEEP ALIVE: his own setting, due only for a kept chat with a request in the last hour (chat_cache::heartbeat_due)"),
    // ---- the provider layer: implementations calling each other, never deciding ------------
    ("crates/richos-core/src/native.rs", "prompt_with_context", ".enqueue_prompt(", "PROVIDER"),
    ("crates/richos-core/src/native.rs", "send_input", ".send_observed_input(", "PROVIDER"),
    ("crates/richos-core/src/native.rs", "prompt", ".enqueue_prompt(", "PROVIDER"),
    ("crates/richos-core/src/cognition.rs", "enqueue_prompt", ".prompt(", "PROVIDER"),
    ("crates/richos-core/src/steering.rs", "send_observed_input", ".send_input(", "PROVIDER"),
    // ---- not a model: a token count's field named `prompt` --------------------------------
    ("crates/richos-core/src/spend.rs", "total", ".prompt(", "NOT A MODEL: token arithmetic"),
    ("crates/richos-core/src/spend.rs", "card", ".prompt(", "NOT A MODEL: token arithmetic"),
    ("crates/richos-core/src/spend/ledger.rs", "count", ".prompt(", "NOT A MODEL: token arithmetic"),
    ("crates/richos-core/src/quota/tokens.rs", "correction", ".prompt(", "NOT A MODEL: token arithmetic"),
];

fn app() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn read(path: &str) -> String {
    std::fs::read_to_string(app().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The file's lines with every `#[cfg(test)] mod … { … }` left out.
fn production(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let next_is_mod = lines.get(i + 1).is_some_and(|l| {
            let l = l.trim_start();
            l.starts_with("mod ") || l.starts_with("pub mod ") || l.starts_with("pub(crate) mod ")
        });
        if lines[i].trim() == "#[cfg(test)]" && next_is_mod {
            let mut depth = 0i64;
            let mut started = false;
            let mut j = i + 1;
            while j < lines.len() {
                if !started && lines[j].trim_end().ends_with(';') {
                    break;
                }
                depth += lines[j].matches('{').count() as i64 - lines[j].matches('}').count() as i64;
                started |= lines[j].contains('{');
                if started && depth <= 0 {
                    break;
                }
                j += 1;
            }
            i = j + 1;
            continue;
        }
        out.push((i + 1, lines[i].to_string()));
        i += 1;
    }
    out
}

fn enclosing_fn(lines: &[(usize, String)], index: usize) -> String {
    for (_, line) in lines[..=index].iter().rev() {
        let code = line.trim_start();
        if code.starts_with("//") {
            continue;
        }
        if let Some(at) = code.find("fn ") {
            if at == 0 || !code.as_bytes()[at - 1].is_ascii_alphanumeric() && code.as_bytes()[at - 1] != b'_' {
                return code[at + 3..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            }
        }
    }
    "?".into()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let name = path.file_name().unwrap().to_string_lossy();
            if !name.ends_with("_tests.rs") && name != "tests.rs" {
                out.push(path);
            }
        }
    }
}

/// The body of `fn name` in `text`, braces matched.
fn body(text: &str, name: &str) -> String {
    let start = text.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("fn {name} not found"));
    let open = start + text[start..].find('{').unwrap();
    let mut depth = 0;
    for (offset, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return text[start..open + offset].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("fn {name} never closes")
}

#[test]
fn every_send_call_that_reaches_a_model_is_listed_with_how_the_one_rule_covers_it() {
    let root = app();
    let mut files = Vec::new();
    rust_files(&root.join("crates/richos-core/src"), &mut files);
    rust_files(&root.join("src-tauri/src"), &mut files);
    let mut found: BTreeMap<(String, String, String), Vec<usize>> = BTreeMap::new();
    for file in files {
        let relative = file.strip_prefix(&root).unwrap().to_string_lossy().into_owned();
        let lines = production(&std::fs::read_to_string(&file).unwrap());
        for (index, (number, line)) in lines.iter().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for send in SENDS {
                if line.contains(send) {
                    found.entry((relative.clone(), enclosing_fn(&lines, index), send.to_string())).or_default().push(*number);
                }
            }
        }
    }
    let expected: BTreeMap<(String, String, String), &str> = EXPECTED.iter()
        .map(|(file, function, send, how)| ((file.to_string(), function.to_string(), send.to_string()), *how)).collect();
    let new: Vec<_> = found.iter().filter(|(key, _)| !expected.contains_key(*key))
        .map(|((file, function, send), lines)| format!("{file}:{lines:?} in fn {function}: {send}")).collect();
    assert!(new.is_empty(),
        "a send call that reaches a model is not listed; say how the one rule (richos_core::turn_gate) covers it:\n{}",
        new.join("\n"));
    let gone: Vec<_> = expected.keys().filter(|key| !found.contains_key(*key))
        .map(|(file, function, send)| format!("{file} fn {function}: {send}")).collect();
    assert!(gone.is_empty(), "a listed send call is gone; take it off the list:\n{}", gone.join("\n"));
}

/// The places the list says are gated ask the rule, and the ones it says are fed by them have
/// no other feeder.
#[test]
fn the_gated_sends_ask_the_one_rule() {
    let spine = read("crates/richos-core/src/spine.rs");
    assert!(body(&spine, "deliver").contains("gate.may_start(binding,origin)"), "Spine::deliver no longer asks the rule");
    assert!(body(&spine, "drain_intake").contains("self.deliver_held()"), "held turns are never given once their chat opens");
    // A held intake keeps the drained mark from passing it, so a later one never drops it (finding 2).
    assert!(body(&spine, "drain_intake").contains("deferred_stop = true;\n                        continue;"), "a held intake can be marked drained by a later one");

    let work = read("crates/richos-core/src/work_host.rs");
    assert!(body(&work, "may_send").contains("crate::turn_gate::allows(&self.turn_gate, binding, self.origin_of(binding, input, through))"));
    // Carrying on is a delivery under way when the app ended, never one whose turn ended: a
    // paused job's handoff keeps its receipt (review rv-20261126T151735Z-c45967b1-2484, 1).
    assert!(body(&work, "origin_of").contains("attempt[\"delivered\"] != true"), "a delivered handoff counts as carrying on");
    // "Quit and stop the work" stops what was under way, so it does not carry on at launch.
    assert!(body(&work, "origin_of").contains("attempt[\"stopped\"] != true"), "a delivery stopped at the quit counts as carrying on");
    assert!(body(&work, "record_quit_and_stop").contains("stop_open_attempts("), "quit and stop leaves Stu's deliveries under way");
    let tools = read("crates/richos-core/src/session_tools.rs");
    assert!(body(&tools, "acknowledge").contains("accept_input(scope,role,through,finished,true)"), "an ended turn does not mark its input delivered");
    // Held input is not work that runs (finding 4).
    assert!(body(&work, "peer_work_pending").contains("undelivered_where("), "held Stu input counts as running work");
    assert!(body(&work, "intake_waits").contains("gate.waits(thread_id)"));
    assert!(body(&work, "next_for_stu").contains("self.may_send(binding, &format!(\"peer:{through}\"), Some(through))"),
        "Stu's inbox is selected without the rule");
    assert!(body(&work, "wake_messages").contains("self.may_send(binding, &format!(\"answer:{}\", answer.id), None)"),
        "a live answer is sent without the rule");
    assert!(body(&work, "run").contains("self.may_send(&binding, &format!(\"answer:{}\", d.id), None)"),
        "the run loop's answer is sent without the rule");
    let production_work: String = production(&work).into_iter().map(|(_, l)| l + "\n").collect();
    assert_eq!(production_work.matches("self.run_assignment_message(").count(), 2,
        "run_assignment_message has a feeder other than run_message and the run loop");
    assert_eq!(production_work.matches("self.next_for_stu(").count(), 2,
        "next_for_stu has a caller other than wake_messages and the run loop");

    // Rich's inbox: a message saved before the app opened is filed as from before, so deliver
    // holds its turn; the only other filing is Claude Code's own continuation, a turn the
    // provider has already started.
    let peer = read("src-tauri/src/peer_messages.rs");
    let drain = body(&peer, "drain_inboxes");
    assert!(drain.contains("arrived_while_open(&inbox,through)") && drain.contains("submit_agent_message_written_at("));
    assert_eq!(peer.matches("submit_agent_message(").count(), 1, "a new filing of agent input to Rich");
    assert!(body(&peer, "wire_continuations").contains("submit_agent_message("));

    // Codex reports written before the app opened stay in their journals by the same rule.
    let bridge = read("crates/richos-core/src/codex_bridge.rs");
    assert!(body(&bridge, "accept_when_asked").contains("crate::turn_gate::Origin::FromBefore"));
    let main = read("src-tauri/src/main.rs");
    assert!(!main.contains("richos_core::codex_bridge::accept("), "the app hands Codex input to Stu around the rule");
    assert!(body(&main, "registered_from").contains("work_host.intake_waits(record)"), "a held intake counts as running work at quit");
    // An intake already filed and finished, kept in the log behind a held one, is not running work.
    assert!(body(&main, "registered_from").contains("!finished(record)"), "a finished intake counts as running work at quit");
    assert!(body(&main, "registered_work").contains("turn_for_intake(record.id())"));

    // The gate is in place before anything can start a turn.
    let at = |needle: &str| main.find(needle).unwrap_or_else(|| panic!("main.rs: {needle} not found"));
    assert!(at("work.set_turn_gate(") < at("work.start();"), "the work host runs before it has the rule");
    assert!(at("spine.set_turn_gate(") < at("spine.recover_after_restart_except("), "the conversation recovers before it has the rule");
    // The watchers start from the boot engine check's call (the last mention; the first is its
    // definition), after the record is written empty.
    assert!(at("let turn_gate = ") < main.rfind("check_boot_engine(").unwrap(), "the review watcher can start before the record is written");
}

/// The review watcher is a process of its own: its one start reads the rule's record.
#[test]
fn the_review_watchers_one_start_reads_the_rules_record() {
    let watch = read("../engine/scripts/lib/review_watch.py");
    assert_eq!(watch.matches("self.spawn(").count(), 1, "a second place starts a review process");
    let start = watch.find("    def start(self, item, trigger, now, attempt").expect("Watcher.start");
    let spawn = watch.find("self.spawn(").unwrap();
    let gate = watch[start..].find("if may_start is not None and not may_start(item):").map(|at| start + at).expect("Watcher.start asks may_start");
    assert!(watch.contains("    def may_start(self, item):\n        \"\"\"Every review the app's watcher starts"), "the app's world has no rule to ask");
    assert!(gate < spawn && watch[start..spawn].matches("\n    def ").count() == 0, "the start is not inside Watcher.start, after the rule");
    assert!(watch.contains(&format!("\"{}\"", richos_core::turn_gate::RECORD)), "the watcher reads another record than the gate writes");
    assert!(watch.contains("if not self.chat_open(endpoint):"), "a shut chat's Codex journal is read");
}

// ---- every road to a send, found from the source, not from a list -----------------------------

/// The model sends; the places that file a turn as the user's own words now (a road through one
/// of those is a turn started in his name, which the rule then reads as his acting); and the
/// places that put what an engine process produced (a Codex or review journal, an ECS waiting
/// order) into Rich's or Stu's inbox, the only doors from the engine's Python into a turn
/// (`engine_input_reaches_a_turn_only_through_the_app`).
const ROAD_ENDS: &[&str] = &[
    "enqueue_prompt", "send_input", "send_observed_input", "send_live_input", "keep_chat_alive",
    "prepare_delivery", "record_question_answer", "accept_prompt", "record_prompt",
    "observe_waiting", "observe_channel",
];
/// A function containing one of these asks the rule: a road through it is covered.
const ASKS: &[&str] = &["may_start(", "may_send(", "turn_gate::allows(", ".waits(", "turn_gate::waiting(", "intake_waits("];
/// Where a road may pass without asking, and why. Matched by function name.
const PASSES: &[(&str, &str)] = &[
    ("retry_saved_input", "CARRIES ON: the overload/usage-limit retry of a turn Spine::deliver let through"),
    ("retry_live_after_usage_limit", "CARRIES ON: the usage-limit resend of a live input already let through"),
    ("live_completion_with", "CARRIES ON: the overload retry of a live input already let through"),
    ("keep_alive", "KEEP ALIVE: his own setting, due only for a kept chat with a request in the last hour"),
    ("keep_front_desk_alive_with", "KEEP ALIVE: his own setting, due only for a kept chat with a request in the last hour"),
    ("prepare_delivery_io", "BELOW: sends only a Delivery that prepare_delivery made"),
    ("send_live_input", "BELOW: the live-input helper; its callers are on this graph"),
    ("prompt", "PROVIDER: the Cognition/NativeClient implementations calling each other"),
    ("prompt_with_context", "PROVIDER: the NativeClient implementation"),
];
/// Every function that reaches a road's end without asking the rule, found by walking the call
/// graph of the production source up from the ends. Each says why that is right. A new caller of
/// any send, a new filing of his words, or a new road around the rule adds a function here and
/// fails the test until it asks the rule or is listed with its reason.
const UNASKED: &[(&str, &str)] = &[
    ("crates/richos-core/src/spine.rs::submit_prompt", "HIS WORDS: what he typed or said, now"),
    ("crates/richos-core/src/spine.rs::submit_prompt_inner", "HIS WORDS: what he typed or said, now"),
    ("crates/richos-core/src/spine.rs::submit_prompt_spoken", "HIS WORDS: what he said, now"),
    ("crates/richos-core/src/spine.rs::submit_prompt_to", "HIS WORDS: what he typed, now"),
    ("crates/richos-core/src/spine.rs::submit_prompt_to_authored", "HIS WORDS: what he typed, now"),
    ("crates/richos-core/src/spine.rs::submit_prompt_to_provenance", "HIS WORDS: what he typed, now"),
    ("crates/richos-core/src/spine.rs::submit_pressed_prompt_to", "HIS PRESS: a control he pressed, now"),
    ("src-tauri/src/main.rs::send_message", "HIS WORDS: the composer's send command"),
    ("src-tauri/src/main.rs::start_voice_capture", "HIS WORDS: the voice command he started"),
    ("src-tauri/src/main.rs::run_the_spoken_turn", "HIS WORDS: what he just said"),
    ("crates/richos-core/src/spine.rs::queue_question_answer",
        "HIS ANSWER: reached only through question_host's delivery worker, which asks the rule first (an answer saved before the app opened waits), and Spine::deliver"),
    ("crates/richos-core/src/work_host.rs::deliver_team_answers",
        "NAME ONLY: its own queue_question_answer (work_host.rs) puts the answer where Stu's answer selection asks the rule; the edge to the spine's is the same name"),
    ("crates/richos-core/src/work_host.rs::run_message", "FED BY THE GATED RUN LOOP: next_for_stu and the answer selection ask may_send"),
    ("crates/richos-core/src/work_host.rs::run_assignment_message", "FED BY THE GATED RUN LOOP: through run_message"),
    ("src-tauri/src/peer_messages.rs::reconcile_waiting",
        "ENGINE INPUT, DATED: an order's notice found by the first look at launch says found_at_launch, and drain_inboxes files it as from before, so Spine::deliver holds it; one that comes true while the app is open runs"),
    ("src-tauri/src/peer_messages.rs::start", "ENGINE INPUT, DATED: the waiting-work thread that runs reconcile_waiting"),
];

/// Code with strings, character literals and comments blanked, so braces and calls in them are
/// not read as code. Lengths are kept.
fn lexed(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let blank = |s: &[u8]| s.iter().map(|c| if *c == b'\n' { '\n' } else { ' ' }).collect::<String>();
    while i < b.len() {
        if b[i..].starts_with(b"//") {
            let end = b[i..].iter().position(|c| *c == b'\n').map_or(b.len(), |p| i + p);
            out.push_str(&blank(&b[i..end]));
            i = end;
        } else if b[i..].starts_with(b"/*") {
            let end = text[i + 2..].find("*/").map_or(b.len(), |p| i + 2 + p + 2);
            out.push_str(&blank(&b[i..end]));
            i = end;
        } else if b[i] == b'r' && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_'))
            && b.get(i + 1).is_some_and(|c| *c == b'"' || *c == b'#') && {
                let hashes = b[i + 1..].iter().take_while(|c| **c == b'#').count();
                b.get(i + 1 + hashes) == Some(&b'"')
            } {
            let hashes = b[i + 1..].iter().take_while(|c| **c == b'#').count();
            let close = format!("\"{}", "#".repeat(hashes));
            let start = i + 2 + hashes;
            let end = text[start..].find(&close).map_or(b.len(), |p| start + p + close.len());
            out.push_str(&blank(&b[i..end]));
            i = end;
        } else if b[i] == b'"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != b'"' {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            let end = (j + 1).min(b.len());
            out.push('"');
            out.push_str(&blank(&b[i + 1..end.saturating_sub(1).max(i + 1)]));
            out.push('"');
            i = end;
        } else if b[i] == b'\'' && (b.get(i + 2) == Some(&b'\'') || (b.get(i + 1) == Some(&b'\\') && b.get(i + 3) == Some(&b'\''))) {
            let len = if b.get(i + 2) == Some(&b'\'') { 3 } else { 4 };
            out.push_str(&" ".repeat(len));
            i += len;
        } else {
            let c = text[i..].chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

/// (file, name, body) of every production function: `#[cfg(test)]` modules and test files left out.
fn production_functions() -> Vec<(String, String, String)> {
    let root = app();
    let mut files = Vec::new();
    rust_files(&root.join("crates/richos-core/src"), &mut files);
    rust_files(&root.join("src-tauri/src"), &mut files);
    let mut out = Vec::new();
    for file in files {
        let relative = file.strip_prefix(&root).unwrap().to_string_lossy().into_owned();
        let code: String = production(&lexed(&std::fs::read_to_string(&file).unwrap()))
            .into_iter().map(|(_, line)| line + "\n").collect();
        let bytes = code.as_bytes();
        let mut at = 0;
        while let Some(found) = code[at..].find("fn ") {
            let start = at + found;
            at = start + 3;
            if start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
                continue;
            }
            let name: String = code[at..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            if name.is_empty() {
                continue;
            }
            // The body opens at the first `{` of the signature, unless a `;` ends it first.
            let (mut depth, mut j, mut open) = (0i64, at + name.len(), None);
            while j < bytes.len() {
                match bytes[j] {
                    b'(' | b'[' | b'<' => depth += 1,
                    b')' | b']' => depth -= 1,
                    b'>' if bytes[j - 1] != b'-' => depth -= 1,
                    b';' if depth <= 0 => break,
                    b'{' if depth <= 0 => { open = Some(j); break }
                    _ => {}
                }
                j += 1;
            }
            let Some(open) = open else { continue };
            let mut depth = 0i64;
            let mut close = bytes.len() - 1;
            for (k, c) in bytes[open..].iter().enumerate() {
                match c {
                    b'{' => depth += 1,
                    b'}' => { depth -= 1; if depth == 0 { close = open + k; break } }
                    _ => {}
                }
            }
            out.push((relative.clone(), name, code[open..=close].to_string()));
        }
    }
    out
}

#[test]
fn every_road_to_a_model_send_asks_the_one_rule_or_says_why_not() {
    use std::collections::{BTreeSet, HashMap, HashSet};
    let functions = production_functions();
    let mut definitions: HashMap<&str, Vec<(String, String)>> = HashMap::new();
    let mut bodies: HashMap<(String, String), String> = HashMap::new();
    for (file, name, body) in &functions {
        definitions.entry(name.as_str()).or_default().push((file.clone(), name.clone()));
        bodies.entry((file.clone(), name.clone())).or_default().push_str(body);
    }
    // Calls by name. A method call is followed only for a compound name defined at most twice
    // (`.insert(` is a map's, not ours); a plain or path call for a name defined at most twice.
    let mut callers: HashMap<(String, String), HashSet<(String, String)>> = HashMap::new();
    let mut direct: HashSet<(String, String)> = HashSet::new();
    for ((file, name), body) in &bodies {
        let b = body.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if !(b[i].is_ascii_alphabetic() || b[i] == b'_') || (i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')) {
                i += 1;
                continue;
            }
            let word: String = body[i..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            let after = i + word.len();
            let rest = body[after..].trim_start();
            let rest = if rest.starts_with("::<") { rest.find('>').map_or(rest, |p| rest[p + 1..].trim_start()) } else { rest };
            let is_call = rest.starts_with('(') && !body[..i].trim_end().ends_with("fn");
            if is_call && word != *name {
                let method = body[..i].trim_end().ends_with('.');
                let count = definitions.get(word.as_str()).map_or(0, Vec::len);
                if ROAD_ENDS.contains(&word.as_str()) {
                    direct.insert((file.clone(), name.clone()));
                }
                if (1..=2).contains(&count) && (!method || word.contains('_')) {
                    for target in &definitions[word.as_str()] {
                        callers.entry(target.clone()).or_default().insert((file.clone(), name.clone()));
                    }
                }
            }
            i = after;
        }
    }
    let passes = |node: &(String, String)| {
        ROAD_ENDS.contains(&node.1.as_str())
            || PASSES.iter().any(|(name, _)| *name == node.1)
            || ASKS.iter().any(|ask| bodies[node].contains(ask))
    };
    let mut unasked: BTreeSet<String> = BTreeSet::new();
    let mut frontier: Vec<(String, String)> = direct.into_iter().filter(|node| !passes(node)).collect();
    let mut seen: HashSet<(String, String)> = frontier.iter().cloned().collect();
    while let Some(node) = frontier.pop() {
        unasked.insert(format!("{}::{}", node.0, node.1));
        for caller in callers.get(&node).into_iter().flatten() {
            if !passes(caller) && seen.insert(caller.clone()) {
                frontier.push(caller.clone());
            }
        }
    }
    let listed: BTreeSet<String> = UNASKED.iter().map(|(node, _)| node.to_string()).collect();
    let new: Vec<_> = unasked.difference(&listed).cloned().collect();
    assert!(new.is_empty(),
        "these functions reach a model send, or file a turn as the user's own words, without asking the one rule \
         (richos_core::turn_gate); make the road ask it, or list the function in UNASKED with why:\n{}", new.join("\n"));
    let gone: Vec<_> = listed.difference(&unasked).cloned().collect();
    assert!(gone.is_empty(), "listed in UNASKED but no longer reaching a send unasked; take them off:\n{}", gone.join("\n"));
}

/// **What an engine process produces reaches a turn only through the app's receivers, and each
/// asks the rule** (review rv-20261126T171959Z-6553219b-85a5: an ECS waiting order found at launch
/// started Rich through the engine's adapter). The engine's Python never writes Rich's or Stu's
/// inbox itself: the one place it names the inbox folder is the waiting-work wake signal. Its
/// outputs (Codex and review journals, ECS waiting rows, review processes) enter the app at the
/// receivers pinned here, and the call graph above carries every road from them.
#[test]
fn engine_input_reaches_a_turn_only_through_the_app() {
    fn python_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                python_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "py") {
                let name = path.file_name().unwrap().to_string_lossy();
                if !name.contains("test") && !name.starts_with("fake") {
                    out.push(path);
                }
            }
        }
    }
    let engine = app().join("../engine").canonicalize().unwrap();
    let mut files = Vec::new();
    python_files(&engine, &mut files);
    let mut writers = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        for (index, line) in text.lines().enumerate() {
            let code = line.split('#').next().unwrap_or("");
            if code.contains("\"messages\"") || code.contains("'messages'") {
                writers.push(format!("{}:{}", file.strip_prefix(&engine).unwrap().display(), index + 1));
            }
        }
    }
    let app_adapter = read("../engine/ecs/adapters/app.py");
    assert!(body_py(&app_adapter, "wake_waiting").contains("\"reconcile-waiting.json\").touch()"));
    assert_eq!(writers.len(), 1, "an engine file names the app's inbox folder besides the wake signal: {writers:?}");
    assert!(writers[0].starts_with("ecs/adapters/app.py:"), "{writers:?}");

    // The receivers: Codex and review journals through the bridge, which asks the rule;
    // waiting orders through reconcile_waiting, whose launch-time notices are filed as from before.
    let main = read("src-tauri/src/main.rs");
    assert!(body(&main, "codex_sink").contains("accept_when_asked("), "Codex input reaches Stu around the rule");
    let peer = read("src-tauri/src/peer_messages.rs");
    assert!(body(&peer, "reconcile_waiting").contains("\"found_at_launch\": at_launch"));
    assert!(body(&peer, "drain_inboxes").contains("facts[\"found_at_launch\"] == true"));
}

/// The body of `def name` in Python source, up to the next top-level line.
fn body_py(text: &str, name: &str) -> String {
    let start = text.find(&format!("def {name}(")).unwrap_or_else(|| panic!("def {name} not found"));
    let rest = &text[start..];
    let end = rest[1..].find("\ndef ").map_or(rest.len(), |p| p + 1);
    rest[..end].to_string()
}
