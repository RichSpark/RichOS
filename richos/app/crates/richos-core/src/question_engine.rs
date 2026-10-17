//! The declared engine owns both the predicate and witness algorithm.
use crate::questions::{AskScope, Question, QuestionInput};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn invoke(scope: &AskScope, hook: &str, input: Value, budget: Duration) -> Result<String, String> {
    let (engine, root) = match (&scope.engine, &scope.entity_root) {
        (None, None) => return Ok(String::new()),
        (Some(engine), Some(root)) => (engine, root),
        _ => return Err("The declared question engine has no complete entity scope.".into()),
    };
    let path = engine.join("scripts/hooks").join(hook);
    if !path.is_file() {
        return Err("The declared engine's question guard is unavailable.".into());
    }
    let mut command = Command::new("bash");
    command.arg(path);
    match run_bounded(command, engine, root, input.to_string(), budget)? {
        (Some(s), out, _) if s.success() => Ok(out),
        (Some(_), out, err) => Err(if err.trim().is_empty() { out } else { err }),
        (None, _, _) => Err("The question guard did not finish in time. Nothing was shown.".into()),
    }
}

/// One engine command, in the entity root with the engine's variables, fed `input` and bounded
/// by `budget`: its exit status (`None` when the budget ran out and it was killed), stdout and
/// stderr.
fn run_bounded(
    mut command: Command,
    engine: &std::path::Path,
    root: &std::path::Path,
    input: String,
    budget: Duration,
) -> Result<(Option<std::process::ExitStatus>, String, String), String> {
    command
        .current_dir(root)
        .env("CLAUDE_PLUGIN_ROOT", engine)
        .env("RICHOS_ENTITY_ROOT", root)
        .env("CLAUDE_PROJECT_DIR", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::owned_process::OwnedChild::configure(&mut command);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or("The question guard has no input channel.")?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let read = |pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            drop(pipe.take(65536).read_to_string(&mut text));
            text
        })
    };
    let out = read(Box::new(stdout));
    let err = read(Box::new(stderr));
    let mut child = crate::owned_process::OwnedChild::new(child);
    let deadline = Instant::now() + budget;
    // A broken hook may never read stdin. Bound that case too, including a set
    // larger than the OS pipe buffer, instead of blocking before the timer starts.
    let write = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            drop(child.kill());
            drop(child.wait());
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let out = out.join().unwrap_or_default();
    let err = err.join().unwrap_or_default();
    drop(write.join());
    Ok((status, out, err))
}

/// **What the already-ruled check says about words a lead reports to him** (F3).
#[derive(Debug, PartialEq, Eq)]
pub enum Ruled {
    /// Nothing in the words asks, or nothing they ask is ruled, or no engine is declared.
    Clear,
    /// A paragraph asks him something his record has ruled: the refusal, naming the ruling.
    Ruled(String),
    /// The check could not run; why. The words go through, as every engine gate fails open.
    Unchecked(String),
}

/// **The paragraphs of `text` that ask him something**: those with a question mark, or those
/// presenting `**Options:**` / `**Decision:**`, joined and bounded to 6,000 characters. This is
/// the window the engine's `scripts/hooks/notice-ceo-ruled-prose.sh` measured (asking
/// paragraphs fired on 9 of 193 asking turns and caught the Option D failure; whole messages on
/// 51) and checks at a terminal turn's end; a test holds the two windows to the same pattern.
pub fn asking_paragraphs(text: &str) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.split('\n').chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.join("\n").trim().to_string());
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    let asks = |p: &str| {
        let lower = p.to_lowercase();
        p.contains('?') || lower.contains("**option:") || lower.contains("**options:") || lower.contains("**decision:")
    };
    let joined = paragraphs.into_iter().filter(|p| asks(p)).collect::<Vec<_>>().join("\n");
    joined.chars().take(6000).collect()
}

/// The already-ruled half of `guard-ceo-ruled-ask.sh` (its check 2), through the library's
/// documented usage (`scripts/lib/ceo-ruled.sh`): the same record, the same predicate and the
/// same per-session exemptions. The premise check (its check 1) is for a question card, not a
/// sentence, and is not run here, as the terminal's prose notice does not run it either.
const RULED_CHECK: &str = r#"
lib="$1/scripts/lib/ceo-ruled.sh"
[ -f "$lib" ] || { printf 'UNCHECKED\tthe engine has no scripts/lib/ceo-ruled.sh\n'; exit 0; }
. "$lib"
cr_require || { printf 'UNCHECKED\t%s\n' "$CR_BROKEN"; exit 0; }
rc=0
cr_resolve "$2" || rc=$?
case "$rc" in
    0) ;;
    1) printf 'VERDICT\tCLEAR\n'; exit 0 ;;
    *) printf 'UNCHECKED\t%s\n' "$CR_REASON"; exit 0 ;;
esac
cr_exempts "$2" "$3"
cr_check "$4" || printf 'UNCHECKED\t%s\n' "$CR_BROKEN"
"#;

/// **F3 (operator contract notes §4 item 1): a question in a lead's report goes through the
/// settled-by-his-words check too.** `guard-ceo-ruled-ask.sh` sees only `AskUserQuestion`, and a
/// lead's prepared questions reach it through [`check`]; a question written into the words of
/// an update, an answer, an outcome or a failure reached him unchecked. Here the words' asking
/// paragraphs are checked against his record before anything is recorded, so a ruled question
/// is refused with the ruling, as the guard refuses a card.
pub fn check_ruled(scope: &AskScope, text: &str, budget: Duration) -> Ruled {
    let (engine, root) = match (&scope.engine, &scope.entity_root) {
        (None, None) => return Ruled::Clear,
        (Some(engine), Some(root)) => (engine, root),
        _ => return Ruled::Unchecked("the declared question engine has no complete entity scope".into()),
    };
    let asking = asking_paragraphs(text);
    if asking.is_empty() {
        return Ruled::Clear;
    }
    let qfile = std::env::temp_dir().join(format!("richos-ruled-{}.txt", uuid::Uuid::new_v4().simple()));
    if let Err(e) = std::fs::write(&qfile, format!("{asking}\n")) {
        return Ruled::Unchecked(format!("the words could not be handed to the check ({e})"));
    }
    let mut command = Command::new("bash");
    command.arg("-c").arg(RULED_CHECK).arg("ruled-check").arg(engine).arg(root).arg(&scope.session_id).arg(&qfile);
    let ran = run_bounded(command, engine, root, String::new(), budget);
    if let Err(e) = std::fs::remove_file(&qfile) {
        eprintln!("ruled check: {} could not be removed ({e})", qfile.display());
    }
    let out = match ran {
        Ok((Some(_), out, _)) => out,
        Ok((None, _, _)) => return Ruled::Unchecked("the check did not finish in time".into()),
        Err(e) => return Ruled::Unchecked(e),
    };
    let rows: Vec<Vec<&str>> = out.lines().map(|l| l.split('\t').collect()).collect();
    if let Some(why) = rows.iter().find(|r| r[0] == "UNCHECKED").map(|r| r.get(1).copied().unwrap_or("").to_string()) {
        return Ruled::Unchecked(why);
    }
    if !rows.iter().any(|r| r[0] == "VERDICT" && r.get(1) == Some(&"RULED")) {
        return Ruled::Clear;
    }
    let mut refusal = String::from(
        "Nothing was recorded: these words ask him something his record has already ruled.\n");
    let mut shown = 0;
    for row in &rows {
        match row[0] {
            "RULED" if row.len() >= 9 => {
                shown += 1;
                if shown <= 4 {
                    refusal.push_str(&format!("  {} — {} ({}, line {})\n", row[2], row[3], row[1], row[8]));
                }
            }
            "QUOTE" if row.len() >= 3 && shown <= 4 => refusal.push_str(&format!("    > {}\n", row[2])),
            _ => {}
        }
    }
    refusal.push_str(&format!(
        "Answer from that ruling instead of asking him, and report again without the question. If a ruling \
genuinely does not cover it, say which one and why, on the record, and then report again:\n  {}/scripts/ceo-ruled-exempt.sh {} \"<cite>\" \"<why it does not cover this>\"",
        engine.display(), scope.session_id));
    Ruled::Ruled(refusal)
}
pub fn check(
    scope: &AskScope,
    questions: &[QuestionInput],
    budget: Duration,
) -> Result<(), String> {
    let questions: Vec<_> = questions
        .iter()
        .map(|q| json!({"question":q.text,"options":q.options,"multiSelect":q.multiple}))
        .collect();
    let output = invoke(
        scope,
        "guard-ceo-ruled-ask.sh",
        // The app card is nonblocking. Preserve its identity so only the terminal
        // box is judged by the blocking-ask check; the other question checks still run.
        json!({"hook_event_name":"PreToolUse","tool_name":"mcp__richos_questions__ask","session_id":scope.session_id,"cwd":scope.entity_root,"tool_input":{"questions":questions}}),
        budget,
    )?;
    // A broken declared guard may report a systemMessage with exit 0. Do not mistake
    // that degraded result for a verified pass on the app-owned question path.
    if output.lines().any(|line| {
        serde_json::from_str::<Value>(line)
            .ok()
            .is_some_and(|v| v.get("systemMessage").is_some())
    }) {
        return Err(output);
    }
    Ok(())
}
pub fn witness(scope: &AskScope, q: &Question) -> Result<(), String> {
    if scope.engine.is_none() && scope.entity_root.is_none() {
        return Ok(());
    }
    let mut payload = json!({"hook_event_name":"PostToolUse","tool_name":"AskUserQuestion","session_id":scope.session_id,"cwd":scope.entity_root,"app_question_id":q.id,"tool_input":{"questions":[{"question":q.text,"options":q.options,"multiSelect":q.multiple}]}});
    // F10: the run this ask was made in travels with it, so the engine can credit it to every
    // lead of the run, and to nothing outside it.
    if let Some(run) = scope.app_run.as_deref().filter(|r| !r.trim().is_empty()) {
        payload["app_run"] = json!(run);
    }
    invoke(scope,"notice-ceo-asks.sh",payload,Duration::from_secs(5)).and_then(|output| {
        if output.lines().any(|line|serde_json::from_str::<Value>(line).ok().is_some_and(|v|v["app_question_id"]==q.id && (v["witness"]=="written" || v["witness"]=="not_applicable"))) {Ok(())} else {Err("The engine did not confirm this question’s display witness.".into())}
    })
}
