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

    let work = read("crates/richos-core/src/work_host.rs");
    assert!(body(&work, "may_send").contains("crate::turn_gate::allows(&self.turn_gate, binding, origin)"));
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
    let gate = watch[start..].find("if not self.world.may_start(item):").map(|at| start + at).expect("Watcher.start asks may_start");
    assert!(gate < spawn && watch[start..spawn].matches("\n    def ").count() == 0, "the start is not inside Watcher.start, after the rule");
    assert!(watch.contains(&format!("\"{}\"", richos_core::turn_gate::RECORD)), "the watcher reads another record than the gate writes");
    assert!(watch.contains("if not self.chat_open(endpoint):"), "a shut chat's Codex journal is read");
}
