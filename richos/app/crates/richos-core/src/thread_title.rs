//! **Haiku names a new conversation from its first message** (CEO, 2026-10-19: "As soon as
//! the user submits their first message in a new chat, Haiku needs to come up with the most
//! fitting title for that conversation and automatically change "New chat" to that title.").
//!
//! The same tool-free Haiku launch as the request classifier (`request_classifier.rs`), on the
//! conversation's own Claude login, given `app/title.md` from the engine as its whole instruction
//! (CEO §124: what a good title is lives in that file, never here). One exchange, then the
//! process is gone: a title is asked for once per conversation, so nothing is kept warm for it.
//!
//! **Nothing waits on it.** The spine starts it on a thread of its own beside the first reply
//! (`spine.rs`, `prepare_delivery_io`) and hands the answer to the shell, which shows it unless
//! the user has named the conversation himself. A failure leaves the provisional title in place.
use crate::{engine_profile::EngineProfile, request_classifier::Provider};
use serde_json::{json, Value};
use std::{path::Path, time::{Duration, Instant}};

/// Cold start of the provider plus one short answer. Nothing is held up while it runs; the
/// bound only stops a stalled provider being kept for ever.
const WAIT: Duration = Duration::from_secs(60);
/// Enough of a pasted document for Haiku to see what it is; the frame stays well inside the
/// provider reader's 64 KiB line.
const MESSAGE_CHARS: usize = 8_000;
/// A row title, not a sentence. Longer answers are cut at a word.
pub const TITLE_CHARS: usize = 80;

/// Ask Haiku for the title of the conversation that `first_message` opens.
pub fn write(bin: &Path, profile: &EngineProfile, first_message: &str) -> Result<String, String> {
    let message: String = first_message.chars().take(MESSAGE_CHARS).collect();
    if message.trim().is_empty() { return Err("No first message to title".into()) }
    let mut live = Provider::new(bin, profile, "app/title.md", "titles").start()?;
    let body = json!({"type":"user","message":{"role":"user",
        "content":json!({"first_message":message}).to_string()}}).to_string();
    live.exchange(body, Instant::now() + WAIT, || false, title_of)
}

/// The one line Haiku answered, without the dressing a model sometimes adds anyway.
fn title_of(frame: &Value) -> Result<String, String> {
    let text = frame["result"].as_str().unwrap_or_default();
    let line = text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default();
    let line = line.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '*' | '\u{201c}' | '\u{201d}')).trim();
    if line.is_empty() { return Err("Empty conversation title".into()) }
    if line.chars().count() <= TITLE_CHARS { return Ok(line.to_string()) }
    let cut: String = line.chars().take(TITLE_CHARS).collect();
    Ok(cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head).trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_classifier::tests::fixture;

    #[test]
    fn the_answer_is_the_first_line_without_quotes_and_never_empty() {
        let title = |text: &str| title_of(&json!({"result": text}));
        assert_eq!(title("Q4 board pack").unwrap(), "Q4 board pack");
        assert_eq!(title("\n  \"Pricing for Acme\"  \nextra").unwrap(), "Pricing for Acme");
        assert!(title("   ").is_err());
        assert!(title_of(&json!({"result": null})).is_err());
        let long = "word ".repeat(40);
        let cut = title(&long).unwrap();
        assert!(cut.chars().count() <= TITLE_CHARS && !cut.ends_with(' '), "{cut:?}");
    }

    /// Item 1 and item 3 of the brief, through the provider launch the app uses: a stub
    /// `claude` records what it was started with and what it was asked, and answers.
    #[test]
    fn haiku_titles_the_first_message_from_the_engine_instruction_file() {
        let (root, bin, profile, _) = fixture(r#"
open('argv.json','w').write(json.dumps(sys.argv))
print(json.dumps({'type':'system','subtype':'init','tools':[]}),flush=True)
for line in sys.stdin:
    data=json.loads(json.loads(line)['message']['content'])
    open('asked.json','w').write(json.dumps(data))
    result='Acme renewal pricing'
    print(json.dumps({'type':'assistant','message':{'model':'claude-haiku-5-5','content':[{'type':'text','text':result}]}}),flush=True)
    print(json.dumps({'type':'result','is_error':False,'result':result}),flush=True)
"#);
        let title = write(&bin, &profile, "Can you work out what we should charge Acme at renewal?\nThey asked twice.");
        assert_eq!(title.unwrap(), "Acme renewal pricing");
        let folder = profile.state.join("titles");
        let argv: Vec<String> = serde_json::from_str(&std::fs::read_to_string(folder.join("argv.json")).unwrap()).unwrap();
        let after = |flag: &str| argv[argv.iter().position(|a| a == flag).unwrap() + 1].clone();
        assert_eq!(after("--model"), "claude-haiku-5-5");
        assert_eq!(after("--tools"), "", "the titler is tool-free");
        let instructions = std::fs::read_to_string(profile.engine.join("app/title.md")).unwrap();
        assert!(!instructions.trim().is_empty());
        assert_eq!(after("--system-prompt"), instructions, "what a good title is comes from the instruction file");
        let asked: Value = serde_json::from_str(&std::fs::read_to_string(folder.join("asked.json")).unwrap()).unwrap();
        assert_eq!(asked["first_message"], "Can you work out what we should charge Acme at renewal?\nThey asked twice.");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Item 3: a provider that fails gives no title, and says so, rather than a made-up one.
    #[test]
    fn a_provider_that_fails_gives_no_title() {
        let (root, bin, profile, _) = fixture(r#"
print(json.dumps({'type':'system','subtype':'init','tools':[]}),flush=True)
for line in sys.stdin:
    print(json.dumps({'type':'result','is_error':True,'result':'Not signed in'}),flush=True)
"#);
        assert!(write(&bin, &profile, "Plan the offsite").is_err());
        assert!(write(&bin, &profile, "   ").is_err(), "nothing to title is not sent at all");
        std::fs::remove_dir_all(root).unwrap();
    }
}
