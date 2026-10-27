//! **BUST A BUG** — the CEO's §115 (2026-10-09), built to round 21: *"we should just let the
//! user say what's wrong and where and let their Rich check and articulate everything properly
//! and then submit a GitHub issue on their behalf."*
//!
//! What this file holds the core to, one test per promise the round makes on the card:
//!
//!   1. **Private details are left out.** Names RichOS holds (conversations, companies, people,
//!      folders), file paths and email addresses never reach the issue: each is replaced by a
//!      plain stand-in, and what it replaced is kept beside it for the user's eyes only.
//!   2. **The draft is shown before anything is sent.** Writing a report sends nothing; the
//!      issue is rendered from the sheet the user approved, word for word.
//!   3. **Send.** One POST to `RichSpark/RichOS`'s issues, with the token read at send time.
//!   4. **A failed send is kept and resent.** Offline, GitHub down, or no reporting account set
//!      up: the report stays on the Mac exactly as approved, survives a relaunch, and goes out by
//!      itself once it can.
//!
//! Everything here runs against a fake transport and fake credentials, so no test can file a
//! real issue and none reads the login keychain.

use richos_core::bug_report::*;
use std::cell::RefCell;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("richos-bug-report-{name}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn terms() -> Vec<PrivateTerm> {
    vec![
        PrivateTerm::new("Acme deal", Kind::ConversationName),
        PrivateTerm::new("Design RichOS memory strategy", Kind::ConversationName),
        // A one-word conversation name is left alone, as round 21 does: "Running" is also a word.
        PrivateTerm::new("Running", Kind::ConversationName),
        PrivateTerm::new("Northwind Traders", Kind::CompanyName),
        PrivateTerm::new("Acme", Kind::CompanyName),
        PrivateTerm::new("Deeply", Kind::CompanyName),
        PrivateTerm::new("Dana Whitfield", Kind::PersonName),
        PrivateTerm::new("femcboost", Kind::FolderName),
    ]
}

fn joined(segments: &[Segment]) -> String {
    segments.iter().map(|s| s.text.as_str()).collect()
}

fn screen() -> Screen {
    Screen {
        key: "conversation".into(),
        here: "the Acme deal conversation".into(),
        public: "a conversation, the main RichOS screen".into(),
        conversation: Some("Acme deal".into()),
        text_size: 135,
        theme: "dark".into(),
        technical_view: false,
        content: String::new(),
    }
}

const VERSION: &str = "RichOS 1.2.0, nightly 45 · macOS 15.6 · Apple silicon";

// ---- a fake Rich for his last pass (the CEO's order, §115: the scanner first, Rich last) ----

/// The report Rich is given in his last pass ([`finish_prompt`]), as the scanner left it.
fn report_in(prompt:&str)->serde_json::Value {
    serde_json::from_str::<serde_json::Value>(prompt).unwrap()["draft"].clone()
}

/// A Rich who, last, finds nothing more to clean up: his words are the scanner's.
fn rich_keeps(prompt: &str) -> Result<String, String> {
    let mut report = report_in(prompt);
    report["private"] = serde_json::json!([]);
    Ok(report.to_string())
}

/// A Rich who, last, cleans up `name` (a `kind` he writes as `stand_in`) wherever it still is.
fn rich_replaces(name: &'static str, kind: &'static str, stand_in: &'static str) -> impl Fn(&str) -> Result<String, String> + Copy {
    move |prompt| {
        let words = report_in(prompt).to_string();
        let found = words.contains(name);
        let mut report: serde_json::Value = serde_json::from_str(&words.replace(name, stand_in)).unwrap();
        report["private"] = if found { serde_json::json!([{"text": name, "kind": kind}]) } else { serde_json::json!([]) };
        Ok(report.to_string())
    }
}

/// Claude, unable to answer.
fn rich_down(_: &str) -> Result<String, String> {
    Err("claude took too long".into())
}

// ---------------------------------------------------------------------------------------
// 1. PRIVATE DETAILS ARE LEFT OUT
// ---------------------------------------------------------------------------------------

#[test]
fn names_paths_and_addresses_are_replaced_by_stand_ins_and_what_they_replaced_is_kept() {
    let scrubber = Scrubber::new(terms());
    // A path hides its whole LINE (since review rv-20261009T162841Z-69294215-70e6), so the other
    // private words are on lines of their own.
    let said = "In the acme deal chat, Dana Whitfield's notes went missing.\nMail dana@northwind.example about \
                Northwind Traders and Acme. FEMCBOOST is the folder.\nThey were at \
                /Users/alex/ab/femcboost/notes.txt and ~/Desktop/plan.md.";
    let segments = scrubber.scrub(said);
    let text = joined(&segments);
    for private in ["acme deal", "Dana Whitfield", "/Users/alex", "femcboost", "FEMCBOOST", "~/Desktop", "dana@northwind.example", "Northwind Traders", "Acme."] {
        assert!(!text.contains(private), "{private:?} reached the report: {text}");
    }
    assert!(text.contains("[a conversation]"), "{text}");
    assert!(text.contains("[a person]"), "{text}");
    assert!(text.contains("[a file on this Mac]"), "{text}");
    assert!(text.contains("[an email address]"), "{text}");
    assert!(text.contains("[a company]"), "{text}");
    assert!(text.contains("[a folder]"), "{text}");
    // Each stand-in remembers what it replaced, for the tooltip only the user sees.
    let replaced: Vec<(&str, Kind)> = segments
        .iter()
        .filter_map(|s| s.was.as_deref().map(|w| (w, s.kind.unwrap())))
        .collect();
    assert!(replaced.contains(&("acme deal", Kind::ConversationName)), "{replaced:?}");
    // A path takes its whole line with it, up to the line's final full stop.
    assert!(
        replaced.contains(&("They were at /Users/alex/ab/femcboost/notes.txt and ~/Desktop/plan.md", Kind::FilePath)),
        "{replaced:?}"
    );
    assert!(replaced.contains(&("dana@northwind.example", Kind::EmailAddress)), "{replaced:?}");
    // The line's own full stop stays outside the stand-in, and the line before it is untouched.
    assert!(text.contains("Mail [an email address]"), "{text}");
    assert!(text.ends_with("is the folder.\n[a file on this Mac]."), "{text}");
}

#[test]
fn ordinary_words_that_happen_to_spell_a_name_are_left_alone() {
    let scrubber = Scrubber::new(terms());
    let text = joined(&scrubber.scrub("It keeps running deeply slowly, and and/or 24/7 is fine; Running is a word."));
    // "Running" is a one-word conversation name (skipped), "deeply" is not capitalized, and a
    // slash between words is not a path.
    assert_eq!(text, "It keeps running deeply slowly, and and/or 24/7 is fine; Running is a word.");
}

#[test]
fn rich_can_name_more_private_words_than_richos_holds() {
    let mut extra = terms();
    extra.push(PrivateTerm::new("Marta", Kind::PersonName));
    let text = joined(&Scrubber::new(extra).scrub("Marta saw it too."));
    assert_eq!(text, "[a person] saw it too.");
}

#[test]
fn the_left_out_line_counts_what_was_replaced() {
    let scrubber = Scrubber::new(terms());
    let mut all = scrubber.scrub("Acme deal and Design RichOS memory strategy, with Dana Whitfield.");
    all.extend(scrubber.scrub("Also /Users/alex/x/y.txt"));
    assert_eq!(
        left_out_line(&all),
        "Left out, because anyone can read GitHub issues: two conversation names, one person's name \
         and one file path. The underlined words stand in for them; point at one to see what it replaced."
    );
    assert_eq!(
        left_out_line(&scrubber.scrub("Nothing private here.")),
        "Nothing private was in it, so nothing was left out. Anyone can read GitHub issues."
    );
}

// ---------------------------------------------------------------------------------------
// 2. RICH WRITES IT UP; THE DRAFT IS SHOWN BEFORE ANYTHING IS SENT
// ---------------------------------------------------------------------------------------

#[test]
fn richs_write_up_is_read_out_of_his_answer_even_with_words_around_it() {
    let raw = "Here it is:\n```json\n{\"title\": \"Conversation names in the sidebar are cut off at larger text sizes\",\n\
               \"what_happened\": \"With Text size at 135%, the Acme deal name is cut off.\\n\\nAt 100% it fits.\",\n\
               \"where\": \"The list of conversations on the left, in the Acme deal conversation.\",\n\
               \"steps\": [\"Open any conversation.\", \"Raise Text size to 135%.\"],\n\
               \"expected\": \"Every name can still be read.\",\n\
               \"private\": [{\"text\": \"Marta\", \"kind\": \"person\"}]}\n```";
    let written = parse_written(raw).expect("a fenced answer is read");
    assert_eq!(written.title, "Conversation names in the sidebar are cut off at larger text sizes");
    assert_eq!(written.steps.len(), 2);
    assert_eq!(written.private, vec![PrivateTerm::new("Marta", Kind::PersonName)]);

    let draft = draft_from(&written, VERSION, &Scrubber::new(terms()));
    let headings: Vec<&str> = draft.sections.iter().map(|s| s.heading.as_str()).collect();
    assert_eq!(headings, ["What happened", "Where", "Steps to see it", "What the user expected", "Version"]);
    let what = &draft.sections[0];
    assert_eq!(what.paragraphs.len(), 2, "a blank line starts a new paragraph");
    assert!(joined(&what.paragraphs[0]).contains("[a conversation]"));
    assert!(!joined(&what.paragraphs[0]).contains("Acme"));
    // The version line is not scrubbed: it holds nothing private and must read as it is.
    assert_eq!(joined(&draft.sections[4].paragraphs[0]), VERSION);
}

#[test]
fn an_answer_that_is_not_a_report_is_refused_not_guessed_at() {
    assert!(parse_written("I can't help with that.").is_err());
    assert!(parse_written("{\"what_happened\": \"no title\"}").is_err());
    assert!(parse_written("{\"title\": \"   \", \"what_happened\": \"x\"}").is_err());
}

#[test]
fn the_prompt_carries_the_users_words_the_screen_and_the_version_and_asks_for_json() {
    let prompt = writer_prompt("names get cut off", &screen(), VERSION, &terms(), false);
    assert!(prompt.contains("names get cut off"));
    assert!(prompt.contains("the Acme deal conversation"));
    assert!(prompt.contains("135%"));
    assert!(prompt.contains(VERSION));
    assert!(prompt.contains("\"what_happened\""));
    assert!(prompt.contains("anyone can read"));
}

#[test]
fn rich_runs_with_no_tools_no_settings_and_no_saved_session() {
    let args = writer_args();
    let has = |flag: &str, value: &str| args.windows(2).any(|w| w[0] == flag && w[1] == value);
    assert!(args.iter().any(|a| a == "--print"));
    // One stream-json message in, so a picture can go beside the words (finding 6).
    assert!(has("--input-format", "stream-json"), "{args:?}");
    assert!(has("--output-format", "stream-json"), "{args:?}");
    assert!(args.iter().any(|a| a == "--verbose"), "stream-json output needs --verbose: {args:?}");
    assert!(has("--tools", ""), "{args:?}");
    assert!(has("--setting-sources", ""), "{args:?}");
    assert!(args.iter().any(|a| a == "--no-session-persistence"));
    assert!(args.iter().any(|a| a == "--strict-mcp-config"));
}

#[test]
fn the_reply_envelope_is_read_and_an_error_reply_is_refused() {
    let ok = r#"{"type":"result","is_error":false,"result":"{\"title\":\"T\",\"what_happened\":\"W\"}"}"#;
    assert_eq!(result_text(ok).unwrap(), "{\"title\":\"T\",\"what_happened\":\"W\"}");
    assert!(result_text(r#"{"type":"result","is_error":true,"result":"Not logged in"}"#).is_err());
    assert!(result_text("not json").is_err());
}

#[test]
fn a_change_said_to_rich_is_one_sentence_added_where_he_says() {
    let raw = r#"{"section": "What happened", "add": "It happens in the light theme too.", "private": []}"#;
    let change = parse_change(raw).unwrap();
    assert_eq!(change.section, "What happened");
    assert_eq!(change.add, "It happens in the light theme too.");
    // An answer that is not a change is refused, and nothing is added in its place (since review
    // rv-20261009T162841Z-69294215-70e6 there is no change Rich did not write).
    assert!(parse_change("I can't help with that.").is_err());
}

#[test]
fn the_issue_is_the_sheet_word_for_word() {
    let sheet = Sheet {
        title: "Names are cut off at 135%".into(),
        sections: vec![
            SheetSection { heading: "What happened".into(), paragraphs: vec!["They are cut off with \"…\".".into(), "At 100% they fit.".into()], steps: vec![] },
            SheetSection { heading: "Steps to see it".into(), paragraphs: vec![], steps: vec!["Open any conversation.".into(), "Raise *Text size*.".into()] },
            SheetSection { heading: "Version".into(), paragraphs: vec![VERSION.into()], steps: vec![] },
        ],
    };
    let body = issue_body(&sheet);
    assert_eq!(
        body,
        "### What happened\n\nThey are cut off with \"…\".\n\nAt 100% they fit.\n\n\
         ### Steps to see it\n\n1. Open any conversation.\n2. Raise \\*Text size\\*.\n\n\
         ### Version\n\nRichOS 1.2.0, nightly 45 · macOS 15.6 · Apple silicon\n"
    );
    // The request is the title and that body, and nothing else.
    let (path, json) = issue_request(&public_of(&sheet));
    assert_eq!(path, "/repos/RichSpark/RichOS/issues");
    assert_eq!(json, serde_json::json!({ "title": "Names are cut off at 135%", "body": body }));
    // The card's link is built from the number, never from a URL the page passes.
    assert_eq!(issue_page(412), "https://github.com/RichSpark/RichOS/issues/412");
}

#[test]
fn the_version_line_says_which_build_in_words() {
    assert_eq!(version_line("1.2.0-nightly.20261007.45", Some("15.6"), "aarch64"), VERSION);
    assert_eq!(version_line("1.2.0-dev.a8a232c0e", Some("15.6"), "x86_64"), "RichOS 1.2.0, development build a8a232c0e · macOS 15.6 · Intel");
    assert_eq!(version_line("1.2.0", None, "aarch64"), "RichOS 1.2.0 · Apple silicon");
}

// ---------------------------------------------------------------------------------------
// 3 AND 4. SEND; A FAILED SEND IS KEPT AND RESENT
// ---------------------------------------------------------------------------------------

struct FakeGitHub {
    answers: RefCell<Vec<Outcome>>,
    seen: RefCell<Vec<(String, String, String, String)>>,
}
impl FakeGitHub {
    fn answering(answers: Vec<Outcome>) -> Self {
        FakeGitHub { answers: RefCell::new(answers), seen: RefCell::new(vec![]) }
    }
}
impl Transport for FakeGitHub {
    fn create_issue(&self, credential: &Credential, path: &str, request: &serde_json::Value) -> Outcome {
        self.seen.borrow_mut().push((
            credential.token.clone(),
            path.to_string(),
            request["title"].as_str().unwrap().to_string(),
            request["body"].as_str().unwrap().to_string(),
        ));
        self.answers.borrow_mut().remove(0)
    }
    fn graphql(&self, _: &Credential, request: &serde_json::Value) -> Reply {
        // GraphQL is how a feature idea is posted (`feature_request_tests.rs`); a bug report is an
        // issue, and never goes there.
        panic!("a bug report made a GraphQL request: {request}")
    }
}

struct Token(Option<&'static str>);
impl Credentials for Token {
    fn credential(&self) -> Option<Credential> {
        self.0.map(|t| Credential { account: Account::Reporting, token: t.to_string() })
    }
}

fn sheet(title: &str) -> Sheet {
    Sheet {
        title: title.into(),
        sections: vec![SheetSection { heading: "What happened".into(), paragraphs: vec!["It broke.".into()], steps: vec![] }],
    }
}

fn plain(text: &str) -> Vec<Segment> {
    vec![Segment { text: text.to_string(), was: None, kind: None }]
}

/// `sheet` as a draft of plain words, as the card would show it.
fn draft_of(sheet: &Sheet, private: Vec<PrivateTerm>) -> Draft {
    Draft {
        title: plain(&sheet.title),
        sections: sheet
            .sections
            .iter()
            .map(|s| DraftSection { heading: s.heading.clone(), paragraphs: s.paragraphs.iter().map(|p| plain(p)).collect(), steps: s.steps.iter().map(|p| plain(p)).collect() })
            .collect(),
        private,
    }
}

/// What may leave this Mac of `sheet`, when it is exactly what Rich gave last.
fn public_of(sheet: &Sheet) -> Public {
    match at_send(sheet, Some(&Check::of(&draft_of(sheet, vec![]))), &[], |_| panic!("Rich was asked about words he gave")) {
        AtSend::Send(public) => public,
        other => panic!("not sendable: {other:?}"),
    }
}

#[test]
fn send_files_one_issue_from_the_reporting_account_and_keeps_a_record_of_it() {
    let dir = scratch("send");
    let outbox = Outbox::open(&dir);
    let github = FakeGitHub::answering(vec![Outcome::Created { number: 412, url: "https://github.com/RichSpark/RichOS/issues/412".into() }]);
    let delivery = outbox.send(&public_of(&sheet("Names are cut off")), &Token(Some("tok-1")), &github, 1_000).unwrap();
    match delivery {
        Delivery::Sent(sent) => {
            assert_eq!(sent.number, 412);
            assert_eq!(sent.account, Account::Reporting);
            assert_eq!(sent.url, "https://github.com/RichSpark/RichOS/issues/412");
        }
        other => panic!("not sent: {other:?}"),
    }
    let seen = github.seen.borrow();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, "tok-1", "the token read at send time is the one used");
    assert_eq!(seen[0].1, "/repos/RichSpark/RichOS/issues");
    assert_eq!(seen[0].2, "Names are cut off");
    assert_eq!(seen[0].3, issue_body(&sheet("Names are cut off")));
    assert!(outbox.pending().unwrap().is_empty(), "a sent report is not waiting");
    assert_eq!(outbox.sent().unwrap().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_failed_send_is_kept_on_the_mac_exactly_as_approved_and_resent_when_it_can_go() {
    let dir = scratch("kept");
    let approved = sheet("Not now leaves the suggestion");
    {
        let outbox = Outbox::open(&dir);
        let offline = FakeGitHub::answering(vec![Outcome::Unreachable]);
        match outbox.send(&public_of(&approved), &Token(Some("tok")), &offline, 10_000).unwrap() {
            Delivery::Waiting { reason, .. } => assert_eq!(reason, Reason::Offline),
            other => panic!("{other:?}"),
        }
    }
    // A RELAUNCH: a fresh store over the same folder still holds it, word for word.
    let outbox = Outbox::open(&dir);
    let waiting = outbox.pending().unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].title, approved.title);
    assert_eq!(waiting[0].body, issue_body(&approved));
    assert_eq!(waiting[0].reason, Some(Reason::Offline));

    // Not yet due: nothing is tried, so an offline Mac is not hammered.
    let none = FakeGitHub::answering(vec![]);
    assert!(outbox.send_due(&Token(Some("tok")), &none, 10_000 + 1_000).unwrap().is_empty());
    assert!(none.seen.borrow().is_empty());

    // Back online and due: it goes out by itself, with the body it was approved with.
    let online = FakeGitHub::answering(vec![Outcome::Created { number: 413, url: "u".into() }]);
    let due = waiting[0].next_try_at_ms;
    let sent = outbox.send_due(&Token(Some("tok")), &online, due).unwrap();
    assert_eq!(sent.len(), 1);
    assert!(matches!(&sent[0], Delivery::Sent(s) if s.number == 413));
    assert_eq!(online.seen.borrow()[0].3, issue_body(&approved));
    assert!(outbox.pending().unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn try_now_sends_a_waiting_report_at_once() {
    let dir = scratch("trynow");
    let outbox = Outbox::open(&dir);
    let id = match outbox.send(&public_of(&sheet("x")), &Token(Some("t")), &FakeGitHub::answering(vec![Outcome::Status { code: 503 }]), 0).unwrap() {
        Delivery::Waiting { id, reason } => {
            assert_eq!(reason, Reason::GithubDown);
            id
        }
        other => panic!("{other:?}"),
    };
    let github = FakeGitHub::answering(vec![Outcome::Created { number: 9, url: "u".into() }]);
    assert!(matches!(outbox.try_now(&id, &Token(Some("t")), &github, 1).unwrap(), Some(Delivery::Sent(_))));
    assert_eq!(outbox.try_now(&id, &Token(Some("t")), &github, 2).unwrap(), None, "it is no longer waiting");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn with_no_reporting_account_set_up_nothing_is_tried_and_the_report_waits() {
    let dir = scratch("notoken");
    let outbox = Outbox::open(&dir);
    let github = FakeGitHub::answering(vec![]);
    match outbox.send(&public_of(&sheet("x")), &Token(None), &github, 0).unwrap() {
        Delivery::Waiting { reason, .. } => assert_eq!(reason, Reason::NotSetUp),
        other => panic!("{other:?}"),
    }
    assert!(github.seen.borrow().is_empty(), "no request without a token");
    assert_eq!(outbox.pending().unwrap().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_canceled_report_is_gone_and_is_never_sent() {
    let dir = scratch("cancel");
    let outbox = Outbox::open(&dir);
    let id = match outbox.send(&public_of(&sheet("x")), &Token(Some("t")), &FakeGitHub::answering(vec![Outcome::Unreachable]), 0).unwrap() {
        Delivery::Waiting { id, .. } => id,
        other => panic!("{other:?}"),
    };
    assert!(outbox.cancel(&id).unwrap());
    assert!(outbox.pending().unwrap().is_empty());
    let github = FakeGitHub::answering(vec![]);
    assert!(outbox.send_due(&Token(Some("t")), &github, u64::MAX).unwrap().is_empty());
    assert!(github.seen.borrow().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn what_github_answered_decides_what_the_user_is_told() {
    assert_eq!(classify(&Outcome::Unreachable), Err(Reason::Offline));
    assert_eq!(classify(&Outcome::TimedOut), Err(Reason::GithubDown));
    assert_eq!(classify(&Outcome::Status { code: 502 }), Err(Reason::GithubDown));
    assert_eq!(classify(&Outcome::Status { code: 429 }), Err(Reason::GithubDown));
    assert_eq!(classify(&Outcome::Status { code: 401 }), Err(Reason::Refused));
    assert_eq!(classify(&Outcome::Status { code: 404 }), Err(Reason::Refused));
    assert_eq!(classify(&Outcome::Created { number: 1, url: "u".into() }), Ok((1, "u".to_string())));
    // An offline Mac is asked again sooner than a GitHub that is down, which is asked every few minutes.
    assert!(Reason::Offline.retry_after_ms() < Reason::GithubDown.retry_after_ms());
    assert!(Reason::GithubDown.retry_after_ms() >= 120_000);
}

// ---------------------------------------------------------------------------------------
// THE SECOND REVIEW'S CASES (rv-20261009T102303Z-1c3dda1d-4c78, on 1c3dda1dc): one test per
// fixture case, each a thing a user hits in normal use.
// ---------------------------------------------------------------------------------------

/// The scrubbed text, and what each file-path stand-in replaced.
fn paths_left_out(text: &str) -> (String, Vec<String>) {
    let segments = Scrubber::default().scrub(text);
    let was = segments.iter().filter(|s| s.kind == Some(Kind::FilePath)).filter_map(|s| s.was.clone()).collect();
    (joined(&segments), was)
}

#[test]
fn a_path_in_backticks_is_left_out_whole() {
    // Finding 1, fixture `core-edge-cases.py`: on the tip the backtick kept the path out of the
    // detector and the whole path reached the public report.
    // Since review rv-20261009T154959Z-96dcc581-085b the backticks go with it, and so does the
    // rest of its sentence: no closing mark ends a path. Since review
    // rv-20261009T162841Z-69294215-70e6 the whole line goes.
    let (text, was) = paths_left_out("The file `/Users/alex/Clients/SecretCo/budget.xlsx` disappeared.");
    assert_eq!(text, "[a file on this Mac].");
    assert_eq!(was, ["The file `/Users/alex/Clients/SecretCo/budget.xlsx` disappeared"]);
}

#[test]
fn a_path_with_spaces_is_left_out_whole() {
    // Finding 1: on the tip only "/Users/alex/Client" was replaced and "Plans/SecretCo.xlsx" went public.
    // Since the third review a plain path takes the rest of its clause ("disappeared") with it,
    // and since review rv-20261009T162841Z-69294215-70e6 the whole line.
    let (text, was) = paths_left_out("The file /Users/alex/Client Plans/SecretCo.xlsx disappeared.");
    assert_eq!(text, "[a file on this Mac].");
    assert_eq!(was, ["The file /Users/alex/Client Plans/SecretCo.xlsx disappeared"]);
    // The common Mac shape: a folder with a space in it, written plainly, and one that ENDS on it.
    let (text, _) = paths_left_out("Logs are in ~/Library/Application Support/RichOS/logs/today.log now");
    assert_eq!(text, "[a file on this Mac]");
    let (text, _) = paths_left_out("It wrote to ~/Library/Application Support and stopped.");
    assert_eq!(text, "[a file on this Mac].");
}

#[test]
fn a_quoted_or_escaped_path_with_spaces_is_left_out_whole() {
    // Since review rv-20261009T154959Z-96dcc581-085b the quote or bracket before a path goes with
    // it, to the end of its line (here, of the text), whatever it is written between; since review
    // rv-20261009T162841Z-69294215-70e6 the line's words before it go too.
    for said in [
        "Open \"/Users/alex/My Clients/Secret Co/plan.pdf\" please",
        "Open '~/Documents/Secret Co/plan.pdf' please",
        "Open “~/Documents/Secret Co/plan.pdf” please",
        "Open `~/Library/Application Support/RichOS` please",
        "Open (/Users/alex/Secret Co/plan.pdf) please",
        "Open /Users/alex/Secret\\ Co/plan.pdf please",
    ] {
        let (text, was) = paths_left_out(said);
        assert_eq!(text, "[a file on this Mac]", "{said}");
        assert_eq!(was, [said], "{said}");
        assert!(!text.contains("Secret") && !text.contains("alex"), "{said} => {text}");
    }
    // Words after a plain path go with it to the end of the sentence (the third review replaced
    // the rule that kept them public): nothing in the words says where a name with spaces ends,
    // and since review rv-20261009T151254Z-84d1bdce-20df a comma does not either.
    let (text, _) = paths_left_out("Open /Users/alex/notes.txt and then quit.");
    assert_eq!(text, "[a file on this Mac].");
    let (text, _) = paths_left_out("Open /Users/alex/notes.txt, then quit.");
    assert_eq!(text, "[a file on this Mac].");
}

#[test]
fn an_address_after_an_emoji_is_found_on_character_boundaries() {
    // Finding 5: on the tip this panicked slicing inside the emoji's four bytes.
    let text = joined(&Scrubber::default().scrub("Mail 🐛alice@example.com"));
    assert_eq!(text, "Mail 🐛[an email address]");
    let text = joined(&Scrubber::default().scrub("Écrivez à josé.ñ@example.com — merci"));
    assert!(!text.contains("example.com"), "{text}");
}

#[test]
fn rich_is_given_what_was_on_the_screen_and_a_picture_of_the_window_to_check_against() {
    // Finding 6: on the tip Rich had the user's words, the screen's name, settings and version,
    // and nothing that was ON the screen, so "Looked at the screen you were on" was not true.
    let mut on_screen = screen();
    on_screen.content = "Acme deal\n  what's   the status on Acme?\nNames cut off: Design RichOS memo…".into();
    let prompt = writer_prompt("the names get cut off", &on_screen, VERSION, &terms(), true);
    let data:serde_json::Value=serde_json::from_str(&prompt).unwrap();
    assert!(data["screen_content"].is_string());
    assert!(prompt.contains("what's the status on Acme?"), "the screen's words are not in the prompt: {prompt}");
    assert!(prompt.contains("Names cut off: Design RichOS memo…"));
    assert_eq!(data["picture_attached"],true);
    assert_eq!(data["user_message"],"the names get cut off");
    assert!(prompt.contains("\"checked\""), "{prompt}");
    // Without either, nothing claims there is something to look at.
    let blind = writer_prompt("the names get cut off", &screen(), VERSION, &terms(), false);
    assert_eq!(serde_json::from_str::<serde_json::Value>(&blind).unwrap()["picture_attached"],false);

    // The one line on standard input: the picture as an image block, then the prompt.
    let picture = Picture { media_type: "image/jpeg".into(), base64: "/9j/4AAQ".into() };
    let line = writer_input(&prompt, Some(&picture));
    assert!(line.ends_with('\n') && line.trim_end().lines().count() == 1, "{line}");
    let message: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(message["type"], "user");
    assert_eq!(message["message"]["role"], "user");
    let content = message["message"]["content"].as_array().unwrap();
    assert_eq!(content[0], serde_json::json!({ "type": "image", "source": { "type": "base64", "media_type": "image/jpeg", "data": "/9j/4AAQ" } }));
    assert_eq!(content[1], serde_json::json!({ "type": "text", "text": prompt }));
    assert_eq!(serde_json::from_str::<serde_json::Value>(writer_input("hi", None).trim()).unwrap()["message"]["content"].as_array().unwrap().len(), 1);

    // stream-json's answer: its last `result` line, as claude 2.1.295 printed it on 2026-10-09.
    let streamed = "{\"type\":\"system\",\"subtype\":\"init\"}\n{\"type\":\"assistant\",\"message\":{}}\n\
                    {\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"{\\\"color\\\": \\\"red\\\"}\"}\n";
    assert_eq!(result_text(streamed).unwrap(), "{\"color\": \"red\"}");
    assert!(result_text("{\"type\":\"system\"}\n").is_err(), "a reply with no result line is not a report");

    // The screen's words are bounded, cut on a character boundary.
    let long = "é".repeat(SCREEN_CONTENT_MAX + 50);
    let kept = screen_words(&long);
    assert!(kept.starts_with(&"é".repeat(SCREEN_CONTENT_MAX)) && kept.ends_with("[the rest of the screen is left off]"));

    // What the digest over his answer says: he looked only when he had the screen. (There is no
    // digest without him since review rv-20261009T162841Z-69294215-70e6.)
    let checked = parse_written("{\"title\": \"T\", \"what_happened\": \"W\", \"checked\": \"saw the names cut off at 135%.\", \"private\": []}").unwrap().checked;
    assert_eq!(digest(&on_screen, true, &checked), "Looked at the screen you were on, at 135% text size · Saw the names cut off at 135% · Checked the version");
    assert_eq!(digest(&screen(), false, ""), "Noted the screen you were on, at 135% text size · Checked the version");
    let mut corrections = screen();
    corrections.key = "corrections".into();
    corrections.here = "Corrections".into();
    corrections.text_size = 100;
    assert_eq!(digest(&corrections, true, ""), "Looked at Corrections · Checked the version");
}

#[test]
fn cancel_says_canceled_only_once_the_copy_is_gone_and_names_the_issue_when_it_went_out_first() {
    // Finding 2, fixture `change-races.js`: on the tip Cancel said "Canceled. Nothing was sent."
    // whatever the shell answered, and the shell only answered whether a file was removed.
    let dir = scratch("withdraw");
    let outbox = Outbox::open(&dir);
    let waiting = |answer: Outcome| match outbox.send(&public_of(&sheet("x")), &Token(Some("t")), &FakeGitHub::answering(vec![answer]), 0).unwrap() {
        Delivery::Waiting { id, .. } => id,
        other => panic!("{other:?}"),
    };

    // Taken off this Mac: canceled, and nothing is ever tried.
    let id = waiting(Outcome::Unreachable);
    assert_eq!(outbox.withdraw(&id).unwrap(), Withdrawn::Canceled);
    assert!(outbox.pending().unwrap().is_empty());

    // A retry already sending finishes first (the shell holds one lock across both) and files
    // it: canceling then finds the issue, not "nothing was sent".
    let id = waiting(Outcome::Unreachable);
    let retry = FakeGitHub::answering(vec![Outcome::Created { number: 412, url: "u".into() }]);
    assert!(matches!(outbox.try_now(&id, &Token(Some("t")), &retry, 1).unwrap(), Some(Delivery::Sent(_))));
    match outbox.withdraw(&id).unwrap() {
        Withdrawn::AlreadySent(sent) => assert_eq!(sent.number, 412),
        other => panic!("canceled a report that went out: {other:?}"),
    }
    let said = serde_json::to_value(outbox.withdraw(&id).unwrap()).unwrap();
    assert_eq!((said["state"].as_str(), said["number"].as_u64()), (Some("sent"), Some(412)), "{said}");
    assert_eq!(serde_json::to_value(Withdrawn::Canceled).unwrap(), serde_json::json!({ "state": "canceled" }));

    // The copy cannot be removed (a folder this app may not write): an error, and it still waits.
    let id = waiting(Outcome::Unreachable);
    let folder = dir.join("waiting");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
    let refused = outbox.withdraw(&id);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(refused.is_err(), "a copy that is still on disk was reported canceled: {refused:?}");
    assert_eq!(outbox.pending().unwrap().len(), 1, "the report no longer waits");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_private_name_rich_found_stays_left_out_on_every_later_change() {
    // Finding 4, fixture `core-edge-cases.py`: on the tip "Jane Doe", whom Rich named private in
    // his write-up, was left out of the draft and then went public on the first change.
    let written = Written {
        title: "Names are cut off".into(),
        what_happened: "Jane Doe saw it.".into(),
        private: vec![PrivateTerm::new("Jane Doe", Kind::PersonName)],
        ..Written::default()
    };
    let draft = draft_from(&written, VERSION, &Scrubber::default());
    assert_eq!(joined(&draft.sections[0].paragraphs[0]), "[a person] saw it.");
    assert_eq!(draft.private, vec![PrivateTerm::new("Jane Doe", Kind::PersonName)], "the card is not given the report's private words");

    // A change with none of its own private words: the report's still apply.
    let change = parse_change(r#"{"section": "What happened", "add": "Jane Doe saw it in light mode too.", "private": []}"#).unwrap();
    let (add, kept) = scrub_change(&change, &[], &draft.private);
    assert_eq!(joined(&add), "[a person] saw it in light mode too.");
    assert_eq!(kept, draft.private);

    // Rich names another one while changing it: it is left out, and kept from then on.
    let change = parse_change(r#"{"section": "What happened", "add": "Marta and Jane Doe saw it.", "private": [{"text": "Marta", "kind": "person"}]}"#).unwrap();
    let (add, kept) = scrub_change(&change, &[], &draft.private);
    assert_eq!(joined(&add), "[a person] and [a person] saw it.");
    let again = parse_change(r#"{"section": "What happened", "add": "Marta saw it again.", "private": []}"#).unwrap();
    let (add, _) = scrub_change(&again, &[], &kept);
    assert_eq!(joined(&add), "[a person] saw it again.");
}

// ---------------------------------------------------------------------------------------
// THE THIRD REVIEW'S CASES (rv-20261009T135204Z-9f4d77d4-1892, on 9f4d77d44), fixture
// `path-cases.py`: one test per case. The rule changed here instead of growing another shape:
// from where a path starts, everything up to the end of its clause is left out.
// ---------------------------------------------------------------------------------------

#[test]
fn a_file_name_with_spaces_is_left_out_to_the_end_of_its_clause() {
    // On the tip: "The file [a file on this Mac] Budget.xlsx disappeared." Since review
    // rv-20261009T162841Z-69294215-70e6 the whole line goes.
    let (text, was) = paths_left_out("The file /Users/alex/Documents/Client Budget.xlsx disappeared.");
    assert_eq!(text, "[a file on this Mac].");
    assert_eq!(was, ["The file /Users/alex/Documents/Client Budget.xlsx disappeared"]);
    let (text, _) = paths_left_out("Open ~/Documents/Client Plans please.");
    assert_eq!(text, "[a file on this Mac].");
    let (text, _) = paths_left_out("The file /Users/alex/Client Plans/budget.xlsx disappeared, twice.");
    assert_eq!(text, "[a file on this Mac].");
    // Between delimiters too, since review rv-20261009T154959Z-96dcc581-085b: the opening mark
    // goes with it and no closing mark ends it.
    let (text, was) = paths_left_out("The file `/Users/alex/Documents/Client Budget.xlsx` disappeared.");
    assert_eq!(text, "[a file on this Mac].");
    assert_eq!(was, ["The file `/Users/alex/Documents/Client Budget.xlsx` disappeared"]);
    let (text, _) = paths_left_out("Open \"/Users/alex/Smith, Jones/plan.pdf\" please.");
    assert_eq!(text, "[a file on this Mac].");
}

#[test]
fn a_file_url_is_left_out() {
    // On the tip it reached the report unchanged.
    let (text, was) = paths_left_out("Open file:///Users/alex/Documents/budget.xlsx please.");
    assert_eq!(text, "[a file on this Mac].");
    assert_eq!(was, ["Open file:///Users/alex/Documents/budget.xlsx please"]);
    let (text, _) = paths_left_out("It linked FILE:///Volumes/Work/Secret Co/plan.pdf; nothing opened.");
    assert_eq!(text, "[a file on this Mac].");
}

#[test]
fn every_named_path_start_hides_its_clause_whatever_comes_before_it() {
    // A comma or a semicolon no longer ends what is left out (review
    // rv-20261009T151254Z-84d1bdce-20df): only a line break does, and since review
    // rv-20261009T162841Z-69294215-70e6 the words before it on its line go too.
    for (said, left) in [
        ("Saved to /Volumes/Backup Disk/Clients/x.xlsx, then it froze.", "[a file on this Mac]."),
        ("Temp files in /private/var/folders/ab/Secret Co stay.", "[a file on this Mac]."),
        ("Look at /var/log/Secret Co.log: it is empty.", "[a file on this Mac]."),
        ("It wrote /tmp/Secret Co notes and stopped!", "[a file on this Mac]!"),
        ("The path=/Users/alex/Secret Co/x.txt; that is all.", "[a file on this Mac]."),
        ("Windows saved C:\\Users\\alex\\Client Plans\\budget.xlsx, then closed.", "[a file on this Mac]."),
        // "? " no longer ends it (since cc/echo-opus-bug7): the line's end does.
        ("Windows saved D:/Clients/Secret Co/budget.xlsx? Yes.", "[a file on this Mac]."),
        ("Line one /Users/alex/Secret Co\nline two stays.", "[a file on this Mac]\nline two stays."),
        ("It said “/Users/alex/Rich’s notes.txt” twice.", "[a file on this Mac]."),
        ("It opened /Users/alex/Rich's notes.txt, then froze.", "[a file on this Mac]."),
    ] {
        let (text, _) = paths_left_out(said);
        assert_eq!(text, left, "{said}");
    }
    // A web address, "and/or" and "24/7" are not paths. "Note:/x" is one since review
    // rv-20261009T154959Z-96dcc581-085b: a word with a slash is a path unless it is a short form.
    let said = "See https://example.com/a/b and/or 24/7, or Note:/x.";
    assert_eq!(paths_left_out(said).0, "[a file on this Mac].");
    assert_eq!(paths_left_out("See https://example.com/a/b and/or 24/7.\nOr Note:/x.").0, "See https://example.com/a/b and/or 24/7.\n[a file on this Mac].");
}

#[test]
fn a_short_home_path_does_not_panic() {
    // Finding 4: on the tip both panicked in debug builds (an unsigned subtraction underflowed).
    let (text, _) = paths_left_out("~/a disappeared.");
    assert_eq!(text, "[a file on this Mac].");
    let (text, _) = paths_left_out("~/foo won't open.");
    assert_eq!(text, "[a file on this Mac].");
    let (text, _) = paths_left_out("~/a");
    assert_eq!(text, "[a file on this Mac]");
}

// ---- the fourth review's cases (rv-20261009T142223Z-3d74fe4c-3b6d, on 3d74fe4cb) ----

/// `text` scrubbed with `terms`: the public words, and what each stand-in replaced.
fn scrubbed_with(terms: Vec<PrivateTerm>, text: &str) -> (String, Vec<String>) {
    let segments = Scrubber::new(terms).scrub(text);
    let public = segments.iter().map(|s| s.text.as_str()).collect();
    (public, segments.iter().filter_map(|s| s.was.clone()).collect())
}

#[test]
fn a_private_name_in_other_capitals_is_left_out_in_every_alphabet() {
    // Finding 1, fixture `privacy-probe.py`: on the tip the case folding was ASCII-only, so each
    // of these stayed in the public report word for word.
    let company = |t: &str| vec![PrivateTerm::new(t, Kind::CompanyName)];
    let person = |t: &str| vec![PrivateTerm::new(t, Kind::PersonName)];
    let a_company = Kind::CompanyName.stand_in();
    let a_person = Kind::PersonName.stand_in();
    for (terms, said, public, was) in [
        (company("Café North"), "The CAFÉ NORTH window froze.", format!("The {a_company} window froze."), "CAFÉ NORTH"),
        (person("Zoë Kim"), "ZOË KIM saw it too.", format!("{a_person} saw it too."), "ZOË KIM"),
        (company("CAFÉ NORTH"), "The café north window froze.", format!("The {a_company} window froze."), "café north"),
        // A capital-only spelling that is longer than the name: "ß" is written "SS" in capitals.
        (company("Straße Partners"), "Ask STRASSE PARTNERS, then retry.", format!("Ask {a_company}, then retry."), "STRASSE PARTNERS"),
        (company("Ωmega Σystems"), "The ωmega σystems sync stopped.", format!("The {a_company} sync stopped."), "ωmega σystems"),
        // A one-word Capitalized name is still matched only where it is written with a capital.
        (person("Zoë"), "ZOË saw it; zoë is also a word here.", format!("{a_person} saw it; zoë is also a word here."), "ZOË"),
    ] {
        let (text, left) = scrubbed_with(terms, said);
        assert_eq!(text, public, "{said}");
        assert_eq!(left, vec![was.to_string()], "{said}: the stand-in does not replace the words as written");
    }
    // Accents are letters, not case: "Cafe North" is not "Café North".
    assert_eq!(scrubbed_with(company("Café North"), "The Cafe North menu.").0, "The Cafe North menu.");
    // A name in other capitals is one term, not two.
    let both = vec![PrivateTerm::new("Café North", Kind::CompanyName), PrivateTerm::new("CAFÉ NORTH", Kind::CompanyName)];
    assert_eq!(Scrubber::new(both).terms().len(), 1);
}

#[test]
fn the_heads_up_on_the_users_own_words_is_the_scrubbers_answer() {
    // Finding 2, fixture `email-warning.js`: on the tip the window's own ASCII-only copy of the
    // rules gave no heads-up for an address the scrubber leaves out. The window now asks this.
    let app = terms();
    let report = vec![PrivateTerm::new("Jane Doe", Kind::PersonName)];
    // One line per path: a path hides its whole line (since review
    // rv-20261009T162841Z-69294215-70e6), so each other private word is on a line of its own.
    let said = "Write to alice@büro.de or 用户@例子.中国 about the NORTHWIND TRADERS file.\nfile:///Users/you/Secret.xlsx.\nC:\\Users\\you\\notes.txt.\nFrom JANE DOE.";
    let found = private_in_edit(said, &app, &report);
    for private in ["alice@büro.de", "用户@例子.中国", "NORTHWIND TRADERS", "file:///Users/you/Secret.xlsx", "C:\\Users\\you\\notes.txt", "JANE DOE"] {
        assert!(found.iter().any(|f| f == private), "{private:?} gets no heads-up: {found:?}");
    }
    // Every word the heads-up names is one the report would leave out, and nothing else.
    let scrubbed = Scrubber::new(app.iter().chain(&report).cloned().collect()).scrub(said);
    let left_out: Vec<String> = scrubbed.iter().filter_map(|s| s.was.clone()).collect();
    assert_eq!(found, left_out);
    assert!(private_in_edit("The names on the left get cut off.", &app, &report).is_empty());
}

// ---- the review of 84d1bdced (rv-20261009T151254Z-84d1bdce-20df), fixture `privacy-probe.py` ----

#[test]
fn a_comma_semicolon_or_bracket_in_a_file_name_does_not_end_what_is_left_out() {
    // Finding 1: on the tip a comma or a semicolon ended the path, so the rest of the file's name
    // went public: "Open [a file on this Mac], Jones Budget.xlsx please." Now only a sentence end
    // (outside the file's own extension) or a line break ends it.
    for (said, was) in [
        ("Open /Users/alex/Documents/Smith, Jones Budget.xlsx please.", "Open /Users/alex/Documents/Smith, Jones Budget.xlsx please"),
        ("Open ~/Documents/Client;Secret Budget.xlsx please.", "Open ~/Documents/Client;Secret Budget.xlsx please"),
        ("Open /Users/alex/Documents/Budget (SecretClient).xlsx please.", "Open /Users/alex/Documents/Budget (SecretClient).xlsx please"),
        ("Open /Users/alex/Documents/SecretClient.xlsx please.", "Open /Users/alex/Documents/SecretClient.xlsx please"),
    ] {
        let (text, left) = paths_left_out(said);
        assert_eq!(text, "[a file on this Mac].", "{said}");
        assert_eq!(left, [was], "{said}");
    }
    // A sentence end no longer ends it (since cc/echo-opus-bug7: ". " can be inside a file's
    // name); a line break does.
    let (text, _) = paths_left_out("Open /Users/alex/Smith, Jones.xlsx. Then it froze.");
    assert_eq!(text, "[a file on this Mac].");
    let (text, _) = paths_left_out("Open ~/Smith; Jones.xlsx\nThen it froze.");
    assert_eq!(text, "[a file on this Mac]\nThen it froze.");
}

#[test]
fn private_details_that_overlap_are_left_out_together() {
    // Finding 1: on the tip a span that started inside an earlier one was dropped whole, so its
    // part past the earlier one's end stayed public.
    let person = |t: &str| PrivateTerm::new(t, Kind::PersonName);
    let (text, left) = scrubbed_with(vec![person("Mary Jane"), person("Jane Smith")], "Mary Jane Smith saw it.");
    assert_eq!(text, "[a person] saw it.");
    assert_eq!(left, ["Mary Jane Smith"]);
    // A company name that starts inside a path and runs past where the path ends. Since
    // cc/echo-opus-bug7 a path runs to the end of its line, so the one place left to run past is
    // the line's final full stop, which the path keeps outside. Since review
    // rv-20261009T162841Z-69294215-70e6 the path's whole line is left out.
    let company = vec![PrivateTerm::new("Acme Inc. Partners", Kind::CompanyName)];
    let (text, left) = scrubbed_with(company, "Open ~/Clients/Acme Inc. Partners plan now.");
    assert_eq!(text, "[a file on this Mac].");
    assert_eq!(left, ["Open ~/Clients/Acme Inc. Partners plan now"]);
    let company = vec![PrivateTerm::new("Acme Inc.", Kind::CompanyName)];
    let (text, left) = scrubbed_with(company, "Open ~/Clients/Acme Inc.");
    assert_eq!(text, "[a file on this Mac]");
    assert_eq!(left, ["Open ~/Clients/Acme Inc."]);
    // The fixture's registered company inside a path: all of it is left out, and the heads-up
    // on what is left finds nothing (on the tip it found nothing on a draft that still leaked).
    let known = Scrubber::new(vec![PrivateTerm::new("Smith, Jones", Kind::CompanyName)]);
    let said = "Open /Users/alex/Documents/Smith, Jones Budget.xlsx please.";
    let public = joined(&known.scrub(said));
    assert_eq!(public, "[a file on this Mac].");
    assert_eq!(known.private_in(said), ["Open /Users/alex/Documents/Smith, Jones Budget.xlsx please"]);
    assert!(known.private_in(&public).is_empty(), "{public}");
}

// ---- the fifth review of cc/echo-opus-bug6 (rv-20261009T154959Z-96dcc581-085b, on 96dcc5813),
// fixture `path-delimiters.py`. Rounds kept finding one more shape a path could take, so the rule
// no longer recognizes shapes: any word with a slash or a backslash in it is a path, except a web
// address and a short common form ("and/or", "24/7", a date), and it is left out from the start of
// that word, bracket or quote included, to the end of its sentence or line.

#[test]
fn a_bracketed_file_name_with_brackets_inside_is_left_out_whole() {
    // Finding 1: on the tip the first closing bracket ended the path, so the public report read
    // "Open ([a file on this Mac]) SecretClient.xlsx) please." with no heads-up.
    for said in [
        "Open (/Users/alex/Documents/Budget (draft) SecretClient.xlsx) please.",
        "Open [/Users/alex/Documents/Budget [draft] SecretClient.xlsx] please.",
        "Open (/Users/alex/Documents/Budget [draft (v2)] SecretClient.xlsx) please.",
    ] {
        let (text, was) = paths_left_out(said);
        assert_eq!(text, "[a file on this Mac].", "{said}");
        assert_eq!(was, [&said[..said.len() - 1]], "{said}");
        assert!(Scrubber::default().private_in(&text).is_empty(), "{text}");
    }
}

#[test]
fn a_relative_file_path_is_left_out_and_gets_a_heads_up() {
    // Finding 2: on the tip both reached the public report unchanged, with no heads-up. Since
    // review rv-20261009T162841Z-69294215-70e6 the whole line goes.
    let (text, was) = paths_left_out("Open `./clients/SecretClient/budget.xlsx` please.");
    assert_eq!(text, "[a file on this Mac].");
    assert_eq!(was, ["Open `./clients/SecretClient/budget.xlsx` please"]);
    let (text, was) = paths_left_out("The file clients/SecretClient/budget.xlsx disappeared.");
    assert_eq!(text, "[a file on this Mac].");
    assert_eq!(was, ["The file clients/SecretClient/budget.xlsx disappeared"]);
    // Every other way a path is written: up a folder, the home folder, a Windows drive or folder.
    for said in ["Open ../SecretClient/plan.pdf now.", "Open ~/SecretClient now.", "Open C:\\SecretClient now.", "Open SecretClient\\plan.pdf now."] {
        let (text, _) = paths_left_out(said);
        assert_eq!(text, "[a file on this Mac].", "{said}");
    }
    // The heads-up on the user's own words is the same scrubber's answer, so it names them too.
    let found = private_in_edit("The file clients/SecretClient/budget.xlsx disappeared.", &[], &[]);
    assert_eq!(found, ["The file clients/SecretClient/budget.xlsx disappeared"]);
}

#[test]
fn short_common_slash_forms_and_web_addresses_stay_public() {
    let said = "Use it and/or the menu 24/7 since 10/09 and on 10/09/2026, (and/or) see https://example.com/a/b please.";
    assert_eq!(paths_left_out(said).0, said);
    assert!(Scrubber::default().private_in(said).is_empty(), "{said}");
}

// ---- the two leaks cc/echo-opus-bug7 left open (on 1e7b696cf) ----
// A path is left out to the end of its LINE: ". " inside a file's name ("Dr. SecretClient") does
// not end it. And a word with a slash is a path unless it is on a short fixed list.

#[test]
fn a_full_stop_inside_a_file_name_does_not_end_what_is_left_out() {
    // On 1e7b696cf: "Open [a file on this Mac]. SecretClient Budget.xlsx please."
    // Since review rv-20261009T162841Z-69294215-70e6 the whole line goes.
    let said = "Open /Users/alex/Documents/Dr. SecretClient Budget.xlsx please.";
    let (text, was) = paths_left_out(said);
    assert_eq!(text, "[a file on this Mac].");
    assert_eq!(was, ["Open /Users/alex/Documents/Dr. SecretClient Budget.xlsx please"]);
    assert!(Scrubber::default().private_in(&text).is_empty(), "{text}");
    // The same with "!" or "?" in the name, and the line's own "!" or "?" kept outside.
    let (text, _) = paths_left_out("Open ~/Clients/Wow! SecretCo plan.pdf now!");
    assert_eq!(text, "[a file on this Mac]!");
    let (text, _) = paths_left_out("Where is ~/Clients/Why? SecretCo plan.pdf now?");
    assert_eq!(text, "[a file on this Mac]?");
    // A line break still ends it, so the next line stays public.
    let (text, _) = paths_left_out("Open ~/Dr. SecretClient.xlsx please.\nThen it froze.");
    assert_eq!(text, "[a file on this Mac].\nThen it froze.");
    let (text, _) = paths_left_out("Open ~/Dr. SecretClient.xlsx please.\r\nThen it froze.");
    assert_eq!(text, "[a file on this Mac].\r\nThen it froze.");
}

#[test]
fn two_plain_words_with_a_slash_are_a_path() {
    // On 1e7b696cf: "Clients/SecretCo" was read as a short form like "and/or" and stayed public.
    // Since review rv-20261009T162841Z-69294215-70e6 the whole line goes.
    let (text, was) = paths_left_out("The folder Clients/SecretCo is gone.");
    assert_eq!(text, "[a file on this Mac].");
    assert_eq!(was, ["The folder Clients/SecretCo is gone"]);
    for said in ["Open SecretCo/plan now.", "Open (Clients/SecretCo) now.", "Open AND/OR/SecretCo now.", "Open w/SecretCo now.", "Open 10/SecretCo now."] {
        assert_eq!(paths_left_out(said).0, "[a file on this Mac].", "{said}");
    }
}

#[test]
fn the_fixed_list_of_slash_forms_stays_public() {
    let said = "Use it and/or the menu w/ a key, w/o a mouse, n/a here, 24/7, 1/2 of it, since 10/09 \
                and on 10/09/2026; AND/OR N/A W/O (and/or) see https://example.com/a/b or HTTP://x.org/y please.";
    assert_eq!(paths_left_out(said).0, said);
    assert!(Scrubber::default().private_in(said).is_empty(), "{said}");
}

// ---- the review of 692942153 (rv-20261009T162841Z-69294215-70e6), fixture `privacy-probe.py` ----

#[test]
fn a_backslash_after_a_web_address_is_a_path() {
    // Finding 3: on 692942153 the web-address exception was checked before the backslash, so this
    // reached the public report unchanged, with no heads-up.
    let said = "Open https://example.org\\SecretCo\\budget.xlsx please.";
    let (text, was) = paths_left_out(said);
    assert!(!text.contains("SecretCo") && !text.contains("budget"), "{text}");
    assert_eq!(was.len(), 1, "{was:?}");
    assert!(!Scrubber::default().private_in(said).is_empty(), "no heads-up for {said}");
    // A web address with only forward slashes is still not a path.
    assert_eq!(paths_left_out("See https://example.org/a/b please.").0, "See https://example.org/a/b please.");
}

/// Every word of a draft the card would show: its title and each section's paragraphs and steps.
fn draft_words(draft: &Draft) -> String {
    let mut words = vec![joined(&draft.title)];
    for section in &draft.sections {
        words.extend(section.paragraphs.iter().chain(&section.steps).map(|p| joined(p)));
    }
    words.join("\n")
}

#[test]
fn when_claude_cannot_check_it_the_report_is_not_offered_and_waits_on_this_mac_until_he_can() {
    // Finding 2, fixture `privacy-probe.py`: on 692942153 a Claude error, timeout or malformed
    // answer put the plain write-up on the card, ready to send, and its public body read "Jane Doe
    // at SecretCo saw the window freeze while opening the plan." with nothing found private. (On
    // 692942153 this test does not compile: there is no way to keep a report Rich did not check.)
    let dir = scratch("unchecked");
    let outbox = Outbox::open(&dir);
    let said = "Jane Doe at SecretCo saw the window freeze while opening the plan.";
    let place = Screen { key: "conversation".into(), here: "a conversation".into(), public: "a conversation, the main RichOS screen".into(), conversation: None, text_size: 100, theme: "dark".into(), technical_view: false, content: String::new() };
    // The fixture's app knows only a client it was told about; Jane Doe and SecretCo are not it.
    let known = Scrubber::new(vec![PrivateTerm::new("Northwind Traders", Kind::CompanyName)]);
    let failures = [Err("claude took too long".to_string()), Err("Claude answered with an error: Overloaded".to_string()), Ok("I can't help with that.".to_string())];
    for (n, rich) in failures.into_iter().enumerate() {
        let check = checked(rich, &place, false, VERSION, &known, rich_keeps);
        assert!(check.is_err(), "a draft came out of an answer that is not a report: {check:?}");
        match outbox.keep_unless_checked(said, &place, check, 1_000).unwrap() {
            WriteUp::Unchecked { .. } => {}
            offered => panic!("a report Rich did not check was offered: {offered:?}"),
        }
        assert_eq!(outbox.unchecked().unwrap().len(), n + 1, "it is not kept on this Mac");
    }
    // Nothing of it waits to go to GitHub, and nothing went.
    assert!(outbox.pending().unwrap().is_empty() && outbox.sent().unwrap().is_empty());
    let waiting = outbox.unchecked().unwrap();
    assert!(waiting.iter().all(|u| u.answer == said && u.checked.is_none()), "{waiting:?}");
    // The window is told so: no draft, only the report's id.
    let told = serde_json::to_value(WriteUp::Unchecked { id: waiting[0].id.clone() }).unwrap();
    assert_eq!(told, serde_json::json!({ "state": "unchecked", "id": waiting[0].id }));

    // Not due yet: Rich is not asked.
    let mut asked = 0;
    assert!(outbox.check_due(1_000 + CHECK_RETRY_MS - 1, |_| { asked += 1; Err("not now".into()) }).unwrap().is_empty());
    assert_eq!(asked, 0);
    // Due, and Claude still cannot answer: it waits again, and is still nothing to send.
    let due = waiting[0].next_try_at_ms;
    assert!(outbox.check_due(due, |_| Err("claude took too long".into())).unwrap().is_empty());
    let waiting = outbox.unchecked().unwrap();
    assert!(waiting.iter().all(|u| u.attempts == 2 && u.next_try_at_ms == due + CHECK_RETRY_MS && u.checked.is_none()), "{waiting:?}");

    // Claude answers: Rich checks it, names Jane Doe and SecretCo, and the draft leaves them out.
    let rich = r#"{"title": "The window froze while a plan opened", "what_happened": "Jane Doe at SecretCo saw the window freeze while opening the plan.", "private": [{"text": "Jane Doe", "kind": "person"}, {"text": "SecretCo", "kind": "company"}]}"#;
    let ready = outbox.check_due(due + CHECK_RETRY_MS, |u| checked(Ok(rich.to_string()), &u.screen, false, VERSION, &known, rich_keeps)).unwrap();
    assert_eq!(ready.len(), 3);
    for (_, done) in &ready {
        let words = draft_words(&done.draft);
        assert!(!words.contains("Jane Doe") && !words.contains("SecretCo"), "{words}");
        assert!(words.contains("[a person] at [a company] saw the window freeze"), "{words}");
    }
    // Still nothing sent: the user approves the card first. Until the window takes it, it is
    // answered again, without asking Rich again.
    assert!(outbox.pending().unwrap().is_empty() && outbox.sent().unwrap().is_empty());
    let again = outbox.check_due(u64::MAX, |_| panic!("Rich was asked again about a report he checked")).unwrap();
    assert_eq!(again, ready);
    for (id, _) in &ready {
        assert!(outbox.take_unchecked(id).unwrap());
        assert!(!outbox.take_unchecked(id).unwrap(), "taken twice");
    }
    assert!(outbox.unchecked().unwrap().is_empty());

    // Canceled while Rich was being asked: it stays gone, and his late answer is not kept.
    let id = match outbox.keep_unless_checked(said, &place, Err("claude took too long".into()), 0).unwrap() {
        WriteUp::Unchecked { id } => id,
        other => panic!("{other:?}"),
    };
    let late = outbox.check_due(CHECK_RETRY_MS, |u| {
        assert!(outbox.take_unchecked(&u.id).unwrap());
        checked(Ok(rich.to_string()), &u.screen, false, VERSION, &known, rich_keeps)
    });
    assert!(late.unwrap().is_empty(), "a canceled report came back");
    assert!(outbox.unchecked().unwrap().is_empty(), "{id} came back");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_answer_whose_private_list_is_missing_or_malformed_is_not_offered_and_waits_on_this_mac() {
    // Review rv-20261009T171105Z-c0a24290-58b4 finding 1, fixture `privacy-gate.py`: on c0a24290f
    // an answer with a title and a body but no private list, a private list that is a string, or
    // entries that say "name" instead of "text" was read as "nothing private", and its public body
    // read "Jane Doe at SecretCo saw the window freeze" on a card offered for sending.
    let dir = scratch("malformed-private");
    let outbox = Outbox::open(&dir);
    let said = "Jane Doe at SecretCo saw the window freeze while opening the plan.";
    let place = Screen { key: "conversation".into(), here: "a conversation".into(), public: "a conversation, the main RichOS screen".into(), conversation: None, text_size: 100, theme: "dark".into(), technical_view: false, content: String::new() };
    let known = Scrubber::new(vec![PrivateTerm::new("Northwind Traders", Kind::CompanyName)]);
    let variants = [
        ("list omitted", None),
        ("list is a string", Some(serde_json::json!("not checked yet"))),
        ("entries say name, not text", Some(serde_json::json!([{"name": "Jane Doe", "kind": "person"}, {"name": "SecretCo", "kind": "company"}]))),
    ];
    // Every variant is tried before anything is asserted, so a failure names all that were offered.
    let mut offered = Vec::new();
    for (label, private) in variants {
        let mut reply = serde_json::json!({"title": "The window froze", "what_happened": said});
        if let Some(p) = private {
            reply["private"] = p;
        }
        let check = checked(Ok(reply.to_string()), &place, false, VERSION, &known, rich_keeps);
        if let WriteUp::Checked(c) = outbox.keep_unless_checked(said, &place, check, 1_000).unwrap() {
            offered.push(format!("{label}: offered for sending, public words {:?}", draft_words(&c.draft)));
        }
    }
    assert!(offered.is_empty(), "a report Rich did not check was offered:\n{}", offered.join("\n"));
    assert_eq!(outbox.unchecked().unwrap().len(), 3, "not every one is kept on this Mac");
    // Nothing of it waits to go to GitHub, and nothing went; each waits to be asked again.
    assert!(outbox.pending().unwrap().is_empty() && outbox.sent().unwrap().is_empty());
    assert!(outbox.unchecked().unwrap().iter().all(|u| u.checked.is_none() && u.next_try_at_ms == 1_000 + CHECK_RETRY_MS));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_private_list_reads_only_when_every_entry_has_its_text_and_kind() {
    let with = |private: serde_json::Value| serde_json::json!({"title": "T", "what_happened": "W", "private": private}).to_string();
    // Truly empty: nothing private, and the answer is read.
    assert_eq!(parse_written(&with(serde_json::json!([]))).unwrap().private, vec![]);
    // Each malformed shape refuses the whole answer; no entry is dropped quietly.
    for bad in [
        serde_json::json!(null),
        serde_json::json!({"text": "Jane Doe", "kind": "person"}),
        serde_json::json!(["Jane Doe"]),
        serde_json::json!([{"text": "Jane Doe"}]),
        serde_json::json!([{"text": "Jane Doe", "kind": 3}]),
        serde_json::json!([{"text": 7, "kind": "person"}]),
        serde_json::json!([{"text": "Jane Doe", "kind": "person"}, {"name": "SecretCo", "kind": "company"}]),
    ] {
        assert!(parse_written(&with(bad.clone())).is_err(), "read as a report: {bad}");
    }
    // A change said to Rich is held to the same rule: its words are added to the public report.
    assert!(parse_change(r#"{"section": "What happened", "add": "Jane Doe saw it too."}"#).is_err());
    assert!(parse_change(r#"{"section": "What happened", "add": "Jane Doe saw it too.", "private": [{"name": "Jane Doe", "kind": "person"}]}"#).is_err());
    assert_eq!(parse_change(r#"{"section": "What happened", "add": "It happens in light mode too.", "private": []}"#).unwrap().private, vec![]);
}

#[test]
fn a_line_with_a_path_in_it_is_left_out_whole() {
    // Finding 1: on 692942153 the path was left out from its own word on, so the words of a
    // relative path before the slash stayed public: "Open SecretCo [a file on this Mac]."
    let said = "Open SecretCo Plans/budget.xlsx please.";
    let (text, was) = paths_left_out(said);
    assert_eq!(text, "[a file on this Mac].");
    assert_eq!(was, ["Open SecretCo Plans/budget.xlsx please"]);
    assert!(Scrubber::default().private_in(&text).is_empty(), "{text}");
    // Only that line: the lines around it stay as they are, its indent included.
    let (text, _) = paths_left_out("It froze.\n  Open SecretCo Plans/budget.xlsx please.\nThen it closed.");
    assert_eq!(text, "It froze.\n  [a file on this Mac].\nThen it closed.");
}

// ---- the review of 63b3f5101 (rv-20261009T172237Z-63b3f510-acf4), fixture `privacy-terms.py` ----

/// Rich's answer: `said` as what happened, and `private` (a JSON list) as his private words.
fn answer_naming(said: &str, private: &str) -> String {
    format!(r#"{{"title": "Conversation disappeared", "what_happened": "{said}", "private": {private}}}"#)
}

/// A change adding `said` to What happened, with `private` (a JSON list) as his private words.
fn change_naming(said: &str, private: &str) -> Change {
    parse_change(&format!(r#"{{"section": "What happened", "add": "{said}", "private": {private}}}"#)).unwrap()
}

#[test]
fn a_one_word_name_rich_lists_as_private_is_left_out_of_the_draft_and_every_change() {
    // On 63b3f5101 "Zephyr", which Rich named a private conversation, went public in the draft
    // and in a change, with no heads-up: a one-word conversation name was dropped from the list
    // whoever named it.
    let said = "The Zephyr conversation disappeared.";
    let private = r#"[{"text": "Zephyr", "kind": "conversation"}]"#;
    let checked = checked(Ok(answer_naming(said, private)), &screen(), false, VERSION, &Scrubber::default(), rich_keeps).unwrap();
    assert_eq!(joined(&checked.draft.sections[0].paragraphs[0]), "The [a conversation] conversation disappeared.");
    let (add, kept) = scrub_change(&change_naming(said, private), &[], &[]);
    assert_eq!(joined(&add), "The [a conversation] conversation disappeared.");
    // On every later change, in any capitals, and in the heads-up on the user's own words.
    let later = change_naming("the zephyr one and ZEPHYR too.", "[]");
    assert_eq!(joined(&scrub_change(&later, &[], &kept).0), "the [a conversation] one and [a conversation] too.");
    assert_eq!(private_in_edit(said, &[], &checked.draft.private), ["Zephyr"]);
    // The app's own one-word conversation names keep their rule: "Running" is also a word.
    assert_eq!(joined(&Scrubber::new(terms()).scrub("It keeps Running.")), "It keeps Running.");
}

#[test]
fn a_name_rich_lists_in_lower_case_is_left_out_when_the_app_holds_it_capitalized() {
    // On 63b3f5101, with "SecretCo" held by the app, Rich's "secretco" was dropped as a repeat,
    // and the app's capital-only rule for "SecretCo" then left "secretco" public.
    let known = vec![PrivateTerm::new("SecretCo", Kind::CompanyName)];
    let said = "The secretco conversation disappeared.";
    let private = r#"[{"text": "secretco", "kind": "company"}]"#;
    let checked = checked(Ok(answer_naming(said, private)), &screen(), false, VERSION, &Scrubber::new(known.clone()), rich_keeps).unwrap();
    assert_eq!(joined(&checked.draft.sections[0].paragraphs[0]), "The [a company] conversation disappeared.");
    let (add, kept) = scrub_change(&change_naming(said, private), &known, &[]);
    assert_eq!(joined(&add), "The [a company] conversation disappeared.");
    let later = change_naming("SecretCo, secretco and SECRETCO.", "[]");
    assert_eq!(joined(&scrub_change(&later, &known, &kept).0), "[a company], [a company] and [a company].");
    assert_eq!(private_in_edit(said, &known, &checked.draft.private), ["secretco"]);
    // Without Rich's word the app's rule stands: its capitalized one-word name is matched capitalized.
    assert_eq!(joined(&Scrubber::new(known).scrub(said)), said);
}

// ---- the review of baf60f3c0 (rv-20261009T173448Z-baf60f3c-c54d), fixture `privacy_probe.rs` ----

#[test]
fn a_name_rich_lists_across_a_line_break_is_left_out_where_the_draft_joins_the_line() {
    // On baf60f3c0 the draft turned the line break in "Jane\nDoe" into a space before scrubbing,
    // while Rich's private word kept its line break, so "Jane Doe" went public with no heads-up.
    let said = r"Jane\nDoe saw the window freeze.";
    let private = r#"[{"text": "Jane\nDoe", "kind": "person"}]"#;
    let checked = checked(Ok(answer_naming(said, private)), &screen(), false, VERSION, &Scrubber::default(), rich_keeps).unwrap();
    assert_eq!(joined(&checked.draft.sections[0].paragraphs[0]), "[a person] saw the window freeze.");
    // In a change, and in every later change written on one line.
    let (add, kept) = scrub_change(&change_naming(said, private), &[], &[]);
    assert_eq!(joined(&add), "[a person] saw the window freeze.");
    let later = change_naming("jane doe and JANE  DOE saw it too.", "[]");
    assert_eq!(joined(&scrub_change(&later, &[], &kept).0), "[a person] and [a person] saw it too.");
    // And in the heads-up on the words as the card shows them.
    assert_eq!(private_in_edit("Jane Doe saw the window freeze.", &[], &checked.draft.private), ["Jane Doe"]);
}

#[test]
fn a_name_written_with_two_spaces_matches_the_same_name_written_with_one() {
    // Any run of spaces, tabs or line breaks in a private name matches any run in the text,
    // whichever side has more.
    let said = "Jane Doe saw the window freeze.";
    let private = r#"[{"text": "Jane  Doe", "kind": "person"}]"#;
    let checked = checked(Ok(answer_naming(said, private)), &screen(), false, VERSION, &Scrubber::default(), rich_keeps).unwrap();
    assert_eq!(joined(&checked.draft.sections[0].paragraphs[0]), "[a person] saw the window freeze.");
    let (add, kept) = scrub_change(&change_naming(said, private), &[], &[]);
    assert_eq!(joined(&add), "[a person] saw the window freeze.");
    assert_eq!(private_in_edit(said, &[], &kept), ["Jane Doe"]);
    let one = vec![PrivateTerm::new("Jane Doe", Kind::PersonName)];
    assert_eq!(private_in_edit("Jane \t Doe saw it.", &[], &one), ["Jane \t Doe"]);
    // The names RichOS holds go by the same rule.
    assert_eq!(joined(&Scrubber::new(terms()).scrub("Dana\n  Whitfield called.")), "[a person] called.");
    // A space still has to be there: "JaneDoe" is one word, not the name.
    assert!(private_in_edit("JaneDoe saw it.", &[], &one).is_empty());
}

// ---- the review of 3007e3200 (rv-20261009T174727Z-3007e320-578a), fixture `privacy_probe.rs` ----

#[test]
fn a_name_rich_lists_across_a_blank_line_is_left_out_before_the_draft_splits_it_into_paragraphs() {
    // Finding 2: on 3007e3200 the draft split "Jane\n\nDoe" into two paragraphs before scrubbing
    // either, so neither held the whole name: the card showed "Jane" and "Doe saw the window
    // freeze.", and the issue's body read "Jane\n\nDoe saw the window freeze."
    let said = r"Jane\n\nDoe saw the window freeze.";
    let checked = checked_with(said, r#"[{"text": "Jane\n\nDoe", "kind": "person"}]"#);
    let paragraphs: Vec<String> = checked.draft.sections[0].paragraphs.iter().map(|p| joined(p)).collect();
    assert_eq!(paragraphs, ["[a person] saw the window freeze."]);
    // Paragraphs on either side of a name still split where Rich left a blank line.
    let said = r"It froze.\n\nJane Doe saw it.\n\nThen it closed.";
    let checked = checked_with(said, r#"[{"text": "Jane Doe", "kind": "person"}]"#);
    let paragraphs: Vec<String> = checked.draft.sections[0].paragraphs.iter().map(|p| joined(p)).collect();
    assert_eq!(paragraphs, ["It froze.", "[a person] saw it.", "Then it closed."]);
}

/// Rich's checked draft of `said`, naming `private` (a JSON list), with no names RichOS holds.
fn checked_with(said: &str, private: &str) -> Checked {
    checked(Ok(answer_naming(said, private)), &screen(), false, VERSION, &Scrubber::default(), rich_keeps).unwrap()
}

// ---------------------------------------------------------------------------------------
// THE CEO'S ORDER (§115, 2026-10-09): THE SCANNER FIRST, RICH LAST, AND HIS WORDS AS THEY ARE.
// "If it's so hard to automate this particular part, then just let's the user's Rich check and
// clean-up what needs to be cleaned-up in that particular user's case." Every review since the
// morning found a gap in the scanner; it stays as an extra help, and Rich gets the last word.
// ---------------------------------------------------------------------------------------

/// The title and the body GitHub is sent are the card's words, every one: the title as the card
/// shows it, and the body as the card's sections written out, every mark escaped so that GitHub
/// shows the words themselves ([`issue_body`]).
fn assert_sent_as_shown(card: &Sheet, public: &Public) {
    assert_eq!(public.title(), card.title, "the title sent is not the title the card showed");
    assert_eq!(public.body(), issue_body(card), "the body sent is not the body the card showed");
    let (_, request) = issue_request(public);
    assert_eq!((request["title"].as_str(), request["body"].as_str()), (Some(card.title.as_str()), Some(issue_body(card).as_str())));
}

/// **A LINE THAT STARTS WITH A MARKDOWN BLOCK MARKER GOES TO GITHUB AS THE LINE THE CARD SHOWED**
/// (review rv-20261010T194410Z-796ff877-381f): on 796ff877d the paragraph "- The button does
/// nothing." went out as a bullet and "---" as a divider. The marker that starts a line is
/// escaped, and the words the last scrub reads in the body are the user's words with no
/// backslash left in them (an email address that starts a numbered-looking line is still found
/// whole and left out whole).
#[test]
fn a_line_starting_with_a_block_marker_is_sent_as_plain_text() {
    let body = |paragraphs: &[&str], steps: &[&str]| {
        issue_body(&Sheet {
            title: "t".into(),
            sections: vec![SheetSection {
                heading: "What happened".into(),
                paragraphs: paragraphs.iter().map(|p| p.to_string()).collect(),
                steps: steps.iter().map(|s| s.to_string()).collect(),
            }],
        })
    };
    assert_eq!(body(&["- The button does nothing.", "---"], &[]), "### What happened\n\n\\- The button does nothing.\n\n\\---\n");
    let expected = [
        "### What happened",
        "",
        "\\+ one",
        "",
        "\\* two",
        "",
        "\\*\\*\\*",
        "",
        "\\_\\_\\_",
        "",
        "\\# three",
        "",
        "\\> four",
        "",
        "1\\. five",
        "",
        "2\\) six",
        "",
        "Fine",
        "\\- seven",
        "12\\. eight",
        "\\===",
        "",
        "1. \\- a step",
        "2. 3\\. another",
        "",
    ];
    assert_eq!(
        body(&["+ one", "* two", "***", "___", "# three", "> four", "1. five", "2) six", "Fine\n- seven\n  12. eight\n==="], &["- a step", "3. another"]),
        expected.join("\n")
    );
    // Not at the start of a line, the same characters are words and stay as they are.
    assert_eq!(body(&["It costs 3.5 - 1 + 2 = 4.5)"], &[]), "### What happened\n\nIt costs 3.5 - 1 + 2 = 4.5)\n");
    // The last scrub reads the words GitHub shows: no stray backslash, so a body written out from
    // its card is left as it is, and an address after an escaped number is left out whole.
    let plain = body(&["- The button does nothing.", "---", "1. five"], &["2) six"]);
    assert_eq!(Scrubber::default().scrub_markdown(&plain), plain);
    assert_eq!(Scrubber::default().scrub_markdown(&body(&["1.jane@example.com wrote it."], &[])), "### What happened\n\n\\[an email address\\] wrote it.\n");
    // Review rv-20261010T204141Z-53407e2a-dc94: a line indented four or more spaces (pasted during
    // Change it) was a GitHub code block, where the added backslashes show. Indentation means
    // nothing in a bug report, so no line keeps it: none starts a code block, and the escape is
    // on the line's real first character.
    assert_eq!(body(&[], &["Click Save.\n\n        - No response\n        ---"]), "### What happened\n\n1. Click Save.\n\n\\- No response\n\\---\n");
    assert_eq!(body(&["\t\t1. five\n    plain words"], &[]), "### What happened\n\n1\\. five\nplain words\n");
}

/// **WHAT GOES, AND THE CARD THE USER LAST SAW WHEN IT WENT**: Send pressed on `card` under
/// `check`, with `rich` as Rich's last pass, and pressed once more on the card shown again if it
/// is shown again, as the window does. A card shown again is Rich's version, so the second press
/// sends it without asking him again: a third card would be a card that never ends.
fn sent_from(card: &Sheet, check: Option<&Check>, app: &[PrivateTerm], rich: impl Fn(&str) -> Result<String, String>) -> (Sheet, Public) {
    match at_send(card, check, app, &rich) {
        AtSend::Send(public) => (card.clone(), public),
        AtSend::Show(his) => {
            let again = sheet_of(&his.draft);
            match at_send(&again, Some(&Check::of(&his.draft)), app, |_| panic!("Rich was asked again about his own words")) {
                AtSend::Send(public) => (again, public),
                other => panic!("the card shown again was not sent as shown: {other:?}"),
            }
        }
        AtSend::Unchecked(why) => panic!("unchecked: {why}"),
    }
}

/// What GitHub is sent for `sheet` under `check`, Rich keeping what the scanner made.
fn sent_body(sheet: &Sheet, check: &Check) -> String {
    let (_, public) = sent_from(sheet, Some(check), &[], rich_keeps);
    let (_, request) = issue_request(&public);
    request["body"].as_str().unwrap().to_string()
}

#[test]
fn every_prompt_asks_rich_for_the_report_with_this_users_private_details_already_left_out() {
    let written:serde_json::Value=serde_json::from_str(&writer_prompt("names get cut off",&screen(),VERSION,&terms(),false)).unwrap();
    assert_eq!(written["event"],"user_requested_public_draft");
    assert!(written["private_terms"].as_str().unwrap().contains("Acme deal"));
    assert!(written["privacy_placeholders"].as_str().unwrap().contains("person [a person]"));
    let card=sheet("Names are cut off");
    let last:serde_json::Value=serde_json::from_str(&finish_prompt(&card,&card,&terms())).unwrap();
    assert_eq!(last["event"],"public_draft_privacy_check");
    assert!(last["output_schema"]["private"].is_array());
    assert_eq!(last["draft"],last["before_automatic_check"]);
    assert_eq!(report_in(&last.to_string())["sections"][0]["paragraphs"][0],"It broke.");

}

#[test]
fn rich_gets_the_last_word_on_what_the_scanner_made_and_the_card_is_his_words_exactly() {
    // (a) The scanner goes first (it leaves out "Acme deal", which RichOS holds); Rich is given its
    // output and cleans up a name no rule can know (Marta); the card is his words, every one.
    let answer = r#"{"title": "The Acme deal list froze", "what_happened": "Marta saw the Acme deal list freeze.", "private": []}"#;
    let given = RefCell::new(String::new());
    let rich = |prompt: &str| {
        *given.borrow_mut() = prompt.to_string();
        rich_replaces("Marta", "person", "[a person]")(prompt)
    };
    let done = checked(Ok(answer.into()), &screen(), false, VERSION, &Scrubber::new(terms()), rich).unwrap();
    let report = report_in(&given.borrow());
    assert_eq!(report["sections"][0]["paragraphs"][0], "Marta saw the [a conversation] list freeze.", "Rich was not given the scanner's output");
    let card = sheet_of(&done.draft);
    assert_eq!(card.title, "The [a conversation] list froze");
    assert_eq!(card.sections[0].paragraphs[0], "[a person] saw the [a conversation] list freeze.");
    assert_eq!(card.sections.last().unwrap().paragraphs, [VERSION]);
    // Each stand-in in his words is marked for the tooltip, its words unchanged: the scanner's with
    // what it replaced, his with what he says he replaced.
    let marks: Vec<(String, String)> = done.draft.sections[0].paragraphs[0].iter().filter_map(|s| Some((s.text.clone(), s.was.clone()?))).collect();
    assert_eq!(marks, [("[a person]".to_string(), "Marta".to_string()), ("[a conversation]".to_string(), "Acme deal".to_string())]);
    // What he replaced is the report's from then on, for the scanner's pass over any later change.
    assert!(done.draft.private.iter().any(|t| t.text == "Marta"), "{:?}", done.draft.private);

    // (c) Send posts the card's words exactly, with only the escaping GitHub needs, and asks no one.
    let public = match at_send(&card, Some(&Check::of(&done.draft)), &terms(), |_| panic!("Rich was asked again about his own words")) {
        AtSend::Send(public) => public,
        other => panic!("{other:?}"),
    };
    assert_sent_as_shown(&card, &public);
    let dir = scratch("verbatim");
    let github = FakeGitHub::answering(vec![Outcome::Created { number: 9, url: "u".into() }]);
    assert!(matches!(Outbox::open(&dir).send(&public, &Token(Some("t")), &github, 0).unwrap(), Delivery::Sent(_)));
    let seen = github.seen.borrow();
    assert_eq!((seen[0].2.as_str(), seen[0].3.as_str()), (card.title.as_str(), issue_body(&card).as_str()));
    assert_eq!(seen[0].3, format!("### What happened\n\n\\[a person\\] saw the \\[a conversation\\] list freeze.\n\n### Version\n\n{VERSION}\n"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn words_the_user_changes_go_through_the_scanner_then_rich_and_send_waits_for_his_version() {
    // (b) Finding 1 of review rv-20261009T174727Z-3007e320-578a, in the CEO's order: words the user
    // changed by hand are scanned, then Rich cleans them up, and his version is on the card before
    // anything goes.
    let app = vec![PrivateTerm::new("Pat Morgan", Kind::PersonName)];
    let written = checked(Ok(answer_naming("The window froze while opening the plan.", "[]")), &screen(), false, VERSION, &Scrubber::new(app.clone()), rich_keeps).unwrap();
    let check = Check::of(&written.draft);
    let card = sheet_of(&written.draft);
    assert!(matches!(at_send(&card, Some(&check), &app, |_| panic!("asked about his own words")), AtSend::Send(_)));

    let mut edited = card.clone();
    edited.sections[0].paragraphs[0] = "Pat Morgan and Jane Doe saw the window freeze.".into();
    let asked = RefCell::new(Vec::new());
    let rich = |prompt: &str| {
        asked.borrow_mut().push(report_in(prompt));
        rich_replaces("Jane Doe", "person", "[a person]")(prompt)
    };
    let shown = match at_send(&edited, Some(&check), &app, rich) {
        AtSend::Show(his) => his,
        other => panic!("it went before the user saw Rich's version: {other:?}"),
    };
    // The scanner first (Pat Morgan, whom RichOS holds), then Rich (Jane Doe, whom no rule knows).
    assert_eq!(asked.borrow().len(), 1);
    assert_eq!(asked.borrow()[0]["sections"][0]["paragraphs"][0], "[a person] and Jane Doe saw the window freeze.");
    assert_eq!(shown.digest, EDIT_DIGEST);
    let again = sheet_of(&shown.draft);
    assert_eq!(again.sections[0].paragraphs[0], "[a person] and [a person] saw the window freeze.");
    match at_send(&again, Some(&Check::of(&shown.draft)), &app, |_| panic!("asked again about his own words")) {
        AtSend::Send(public) => assert_sent_as_shown(&again, &public),
        other => panic!("{other:?}"),
    }

    // A change with nothing private in it goes at once, once Rich has given it back unchanged; one
    // changed character is a change.
    let mut one = card.clone();
    one.title.push('!');
    let mut turns = 0;
    match at_send(&one, Some(&check), &app, |p| {
        turns += 1;
        rich_keeps(p)
    }) {
        AtSend::Send(public) => assert_sent_as_shown(&one, &public),
        other => panic!("{other:?}"),
    }
    assert_eq!(turns, 1, "a changed title went without Rich");
    // A card with no check of his at all is his to clean up first too.
    assert!(matches!(at_send(&card, None, &app, rich_down), AtSend::Unchecked(_)));
}

#[test]
fn with_claude_unavailable_nothing_is_sent() {
    // (d) His last pass cannot be had: there is no draft, and the report waits on this Mac.
    let dir = scratch("claude-down");
    let outbox = Outbox::open(&dir);
    let github = FakeGitHub::answering(vec![]);
    let said = "Jane Doe saw the window freeze.";
    let check = checked(Ok(answer_naming(said, "[]")), &screen(), false, VERSION, &Scrubber::default(), rich_down);
    assert!(check.is_err(), "{check:?}");
    assert!(matches!(outbox.keep_unless_checked(said, &screen(), check, 0).unwrap(), WriteUp::Unchecked { .. }));
    // Nor when his last pass is not the report: no title, another number of sections, or a
    // malformed private list.
    for bad in [
        "I can't help with that.".to_string(),
        serde_json::json!({"title": "T", "sections": [], "private": []}).to_string(),
        serde_json::json!({"title": "", "sections": [{"paragraphs": ["x"]}, {"paragraphs": [VERSION]}], "private": []}).to_string(),
        serde_json::json!({"title": "T", "sections": [{"paragraphs": ["x"]}, {"paragraphs": [VERSION]}], "private": "none"}).to_string(),
        serde_json::json!({"title": "T", "sections": [{"paragraphs": ["x"]}, {"paragraphs": [VERSION]}]}).to_string(),
    ] {
        let answer = checked(Ok(answer_naming(said, "[]")), &screen(), false, VERSION, &Scrubber::default(), |_| Ok(bad.clone()));
        assert!(answer.is_err(), "a draft came out of {bad}: {answer:?}");
    }

    std::fs::remove_dir_all(dir).unwrap();

    // At Send, words the user changed: nothing goes, and they wait on this Mac.
    let dir = scratch("claude-down-at-send");
    let outbox = Outbox::open(&dir);
    let written = checked_with("The window froze.", "[]");
    let mut edited = sheet_of(&written.draft);
    edited.sections[0].paragraphs[0] = said.into();
    let why = match at_send(&edited, Some(&Check::of(&written.draft)), &[], rich_down) {
        AtSend::Unchecked(why) => why,
        other => panic!("{other:?}"),
    };
    let id = match outbox.keep_edited(Edited { sheet: edited.clone(), private: vec![] }, &screen(), &why, 1_000).unwrap() {
        WriteUp::Unchecked { id } => id,
        other => panic!("{other:?}"),
    };
    assert!(outbox.pending().unwrap().is_empty() && outbox.sent().unwrap().is_empty() && github.seen.borrow().is_empty());

    // Once Claude answers, the check loop: the scanner, then Rich, as a card. Still nothing sent.
    let ready = outbox.check_due(1_000 + CHECK_RETRY_MS, |u| checked_edit(u.edited.as_ref().expect("the changed words"), &[], rich_replaces("Jane Doe", "person", "[a person]"))).unwrap();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].0, id);
    assert_eq!(sheet_of(&ready[0].1.draft).sections[0].paragraphs[0], "[a person] saw the window freeze.");
    assert!(outbox.pending().unwrap().is_empty() && outbox.sent().unwrap().is_empty() && github.seen.borrow().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_change_told_to_rich_is_scanned_and_then_his_at_send() {
    // His words for a change are scanned when they are added; the card with them is a changed
    // card, so at Send he has the last word on it, and nothing after him changes it.
    let written = checked_with("The window froze.", "[]");
    let check = Check::of(&written.draft);
    let card = sheet_of(&written.draft);
    let change = change_naming("Jane Doe saw it too.", r#"[{"text": "Jane Doe", "kind": "person"}]"#);
    let (add, _) = scrub_change(&change, &[], check.private());
    assert_eq!(joined(&add), "[a person] saw it too.");
    let mut with_add = card.clone();
    with_add.sections[0].paragraphs.push(joined(&add));
    let mut turns = 0;
    match at_send(&with_add, Some(&check), &[], |p| {
        turns += 1;
        rich_keeps(p)
    }) {
        AtSend::Send(public) => assert!(public.body().contains("\\[a person\\] saw it too."), "{}", public.body()),
        other => panic!("{other:?}"),
    }
    assert_eq!(turns, 1, "a changed card went without Rich's last word");
}

#[test]
fn nothing_after_rich_changes_the_words_he_gave() {
    // The words the card shows are his, and they go as they are: the scanner does not run after
    // him (on a1d158b35 it ran last, at Send, and could change what he had given).
    let jane = vec![PrivateTerm::new("Jane Doe", Kind::PersonName)];
    let card = Sheet {
        title: "`Esc` froze it".into(),
        sections: vec![
            SheetSection { heading: "What happened".into(), paragraphs: vec!["It was opened by Jane".into(), "Doe saw it freeze.".into()], steps: vec![] },
            SheetSection { heading: "Version".into(), paragraphs: vec![VERSION.into()], steps: vec![] },
        ],
    };
    let check = Check::of(&draft_of(&card, jane.clone()));
    match at_send(&card, Some(&check), &[], |_| panic!("Rich was asked again about his own words")) {
        AtSend::Send(public) => assert_sent_as_shown(&card, &public),
        other => panic!("{other:?}"),
    }

    // The scanner itself, which runs first, is unchanged: it reads the words as GitHub shows them;
    // a name with a mark in it is found though the body escapes the mark, and its stand-in is
    // escaped like every other word.
    let body = |p: &str| issue_body(&Sheet { title: "t".into(), sections: vec![SheetSection { heading: "What happened".into(), paragraphs: vec![p.into()], steps: vec![] }] });
    let acme = Scrubber::with_rich(vec![], vec![PrivateTerm::new("Acme_Co", Kind::CompanyName)]);
    assert_eq!(acme.scrub_markdown(&body("Acme_Co *broke* it.")), "### What happened\n\n\\[a company\\] \\*broke\\* it.\n");
    assert_eq!(Scrubber::with_rich(vec![], jane).scrub_markdown(&issue_body(&card)), format!("### What happened\n\nIt was opened by \\[a person\\] saw it freeze.\n\n### Version\n\n{VERSION}\n"));
    assert_eq!(Scrubber::default().scrub_markdown(&body("[a person] saw it.")), body("[a person] saw it."));
    assert_eq!(Scrubber::default().scrub_markdown(&body("Open C:\\Users\\x please.")), "### What happened\n\n\\[a file on this Mac\\].\n");
    assert_eq!(body("~~Jane~~ Doe and Jane&#32;Doe"), "### What happened\n\n\\~\\~Jane\\~\\~ Doe and Jane\\&\\#32;Doe\n");
    // Its title substitution (a backtick as an apostrophe) is the scanner's, before Rich; writing a
    // card out changes no word.
    assert_eq!(sheet_of(&as_posted(&draft_of(&card, vec![]), &Scrubber::default())).title, "'Esc' froze it");
    assert_eq!(issue_of(&card).title, "`Esc` froze it");
}

// ---- the review of a2fc94dc4 (rv-20261009T183159Z-a2fc94dc-20e7), fixture `privacy_probe.rs` ----

#[test]
fn a_private_name_between_underscores_stars_or_tildes_is_left_out() {
    // On a2fc94dc4 "_Jane Doe_", with Rich naming Jane Doe private, went out as "\_Jane Doe\_" with
    // no heads-up: an underscore counted as part of a word, so the name was never whole there.
    let jane = r#"[{"text": "Jane Doe", "kind": "person"}]"#;
    for said in ["_Jane Doe_ saw the window freeze.", "Jane Doe_ saw the window freeze.", "*Jane Doe* saw the window freeze.", "~~Jane Doe~~ saw the window freeze.", "__jane doe__ saw the window freeze."] {
        let written = checked_with(said, jane);
        let body = sent_body(&sheet_of(&written.draft), &Check::of(&written.draft));
        assert!(!body.to_lowercase().contains("jane") && !body.to_lowercase().contains("doe"), "{said:?} sent {body:?}");
        assert!(body.contains("\\[a person\\]"), "{body}");
        assert_eq!(private_in_edit(said, &[], &written.draft.private).len(), 1, "no heads-up on {said:?}");
    }
    // RichOS's own names go by the same rule.
    assert_eq!(joined(&Scrubber::new(terms()).scrub("_Dana Whitfield_ and *Acme*")), "_[a person]_ and *[a company]*");
}

#[test]
fn a_private_name_between_underscores_in_words_changed_by_hand_is_left_out_before_rich_has_them() {
    // The same on a2fc94dc4 after a change by hand: "\_Jane Doe\_" went out. Now the scanner goes
    // over the changed words with the report's private words before Rich has the last word.
    let jane = r#"[{"text": "Jane Doe", "kind": "person"}]"#;
    let written = checked_with("Jane Doe saw the window freeze.", jane);
    let mut edited = sheet_of(&written.draft);
    edited.sections[0].paragraphs[0] = "_Jane Doe_ saw the window freeze.".into();
    let given = RefCell::new(String::new());
    let rich = |p: &str| {
        *given.borrow_mut() = p.to_string();
        rich_keeps(p)
    };
    let (shown, public) = sent_from(&edited, Some(&Check::of(&written.draft)), &[], rich);
    assert_eq!(report_in(&given.borrow())["sections"][0]["paragraphs"][0], "_[a person]_ saw the window freeze.", "Rich was not given the scanner's output");
    assert_sent_as_shown(&shown, &public);
    assert!(!public.body().contains("Jane") && public.body().contains("\\_\\[a person\\]\\_ saw the window freeze."), "{}", public.body());
}

#[test]
fn an_at_name_in_the_body_notifies_nobody_on_github() {
    // On a2fc94dc4 "@octocat" went out as written, so GitHub linked it and notified that account.
    let card = Sheet {
        title: "The window froze".into(),
        sections: vec![
            SheetSection { heading: "What happened @team".into(), paragraphs: vec!["@octocat said it froze; (@hubot) too. Mail a@example.com.".into()], steps: vec!["Ask @octo-cat".into()] },
            SheetSection { heading: "Version".into(), paragraphs: vec![VERSION.into()], steps: vec![] },
        ],
    };
    // Typed by hand: the scanner, then Rich, then the card shown again as it goes.
    let (_, public) = sent_from(&card, None, &[], rich_keeps);
    let body = public.body();
    assert!(body.matches('@').count() == 4 && body.matches("@\u{200B}").count() == 4, "{body:?}");
    // The email address is still found and left out, and nothing else changes for the reader.
    assert!(!body.contains("example.com") && body.contains("\\[an email address\\]"), "{body:?}");
    assert_eq!(body.replace('\u{200B}', ""), format!("### What happened @team\n\n@octocat said it froze; (@hubot) too. Mail \\[an email address\\].\n\n1. Ask @octo-cat\n\n### Version\n\n{VERSION}\n"));
    // Rich is given the words as they are, the scanner's stand-ins in them.
    assert!(finish_prompt(&card, &card, &[]).contains("@octocat said it froze"));
    // A zero-width space the user wrote is kept as written.
    let typed = Sheet { title: "t".into(), sections: vec![SheetSection { heading: "What happened".into(), paragraphs: vec!["@\u{200B}x".into()], steps: vec![] }] };
    assert_eq!(sent_body(&typed, &Check::of(&draft_of(&typed, vec![]))), "### What happened\n\n@\u{200B}\u{200B}x\n");
}

// ---- the review of c3c40906d (rv-20261009T184833Z-c3c40906-bf02), fixture `privacy_probe.rs` ----

/// "Café North" twice, the same visible name: "é" as one character (U+00E9), and as "e" followed
/// by the combining acute accent (U+0301). Text copied from different places carries either.
const CAFE_ONE: &str = "Caf\u{E9} North";
const CAFE_TWO: &str = "Cafe\u{301} North";

#[test]
fn a_private_name_with_an_accent_written_the_other_unicode_way_is_left_out() {
    // On c3c40906d, with Rich naming "Café North" private one way and the text holding it the other
    // way, the report went out with "CAFÉ NORTH" in it and no heads-up, both ways round.
    for (listed, said) in [(CAFE_ONE, "CAFE\u{301} NORTH froze."), (CAFE_TWO, "CAF\u{C9} NORTH froze.")] {
        let private = serde_json::json!([{"text": listed, "kind": "company"}]).to_string();
        let written = checked_with(said, &private);
        let body = sent_body(&sheet_of(&written.draft), &Check::of(&written.draft));
        assert!(!body.contains("NORTH") && body.contains("\\[a company\\] froze."), "{listed:?} in {said:?} sent {body:?}");
        let name = said.trim_end_matches(" froze.").to_string();
        assert_eq!(private_in_edit(said, &[], &written.draft.private), vec![name], "no heads-up on {said:?}");
    }
    // RichOS's own names go by the same rule, and the stand-in replaces the words as written. A
    // one-word name written Capitalized is still matched only where it is written with a capital.
    let app = Scrubber::new(vec![PrivateTerm::new(CAFE_TWO, Kind::CompanyName), PrivateTerm::new("Rene\u{301}e", Kind::PersonName)]);
    assert_eq!(joined(&app.scrub("Ask CAF\u{C9} NORTH. Ren\u{E9}e and ren\u{E9}e.")), "Ask [a company]. [a person] and ren\u{E9}e.");
    // Two spellings of one name are one private word, and each is found as it is written.
    let both = Scrubber::with_rich(vec![PrivateTerm::new(CAFE_ONE, Kind::CompanyName)], vec![PrivateTerm::new(CAFE_TWO, Kind::CompanyName)]);
    assert!(both.terms().is_empty(), "{:?}", both.terms());
    assert_eq!(both.private_in(&format!("{CAFE_ONE} and {CAFE_TWO}")), vec![CAFE_ONE.to_string(), CAFE_TWO.to_string()]);
    // A name without the accent is still another name: "Cafe North" is not "Café North".
    assert!(Scrubber::with_rich(vec![], vec![PrivateTerm::new("Cafe North", Kind::CompanyName)]).private_in(CAFE_TWO).is_empty());
}

#[test]
fn a_private_name_with_an_accent_written_the_other_unicode_way_is_left_out_of_a_change_by_hand() {
    // The same on c3c40906d after a change by hand: "CAFÉ NORTH" went out. The report's private
    // words are the scanner's on the changed words, before Rich has them.
    for (listed, said) in [(CAFE_ONE, "CAFE\u{301} NORTH froze."), (CAFE_TWO, "CAF\u{C9} NORTH froze.")] {
        let private = serde_json::json!([{"text": listed, "kind": "company"}]).to_string();
        let written = checked_with(&format!("{listed} froze."), &private);
        let mut edited = sheet_of(&written.draft);
        edited.sections[0].paragraphs[0] = said.into();
        let (shown, public) = sent_from(&edited, Some(&Check::of(&written.draft)), &[], rich_keeps);
        assert_sent_as_shown(&shown, &public);
        assert!(!public.body().contains("NORTH") && public.body().contains("\\[a company\\] froze."), "{listed:?} in {said:?} sent {:?}", public.body());
    }
}

// ---- the review of d9cdd913f (rv-20261009T190633Z-d9cdd913-9f1c), fixture `approval.rs` ----

/// Rich's draft of a report titled `title`, with `happened` under What happened and `private` (a
/// JSON list) as the private words he named, through the scanner, his last pass keeping it.
fn written_as(title: &str, happened: &str, private: serde_json::Value) -> Checked {
    let answer = serde_json::json!({"title": title, "what_happened": happened, "private": private}).to_string();
    checked(Ok(answer), &screen(), false, VERSION, &Scrubber::default(), rich_keeps).unwrap()
}

#[test]
fn the_title_sent_is_the_title_the_card_shows() {
    // Finding 1: on d9cdd913f the card showed "`Enter` does not send", and "'Enter' does not
    // send" was filed: the title's backticks became apostrophes after the user approved it.
    let written = written_as("`Enter` does not send", "The window froze.", serde_json::json!([]));
    let card = sheet_of(&written.draft);
    match at_send(&card, Some(&Check::of(&written.draft)), &[], |_| panic!("asked about his own words")) {
        AtSend::Send(public) => assert_sent_as_shown(&card, &public),
        other => panic!("Rich's draft was not sendable: {other:?}"),
    }
    assert_eq!(card.title, "'Enter' does not send", "the card does not show the title as it goes");

    // A backtick the user types into the title is shown as it would go before anything is sent.
    let mut typed = card.clone();
    typed.title = "`Enter` still does not send".into();
    match at_send(&typed, Some(&Check::of(&written.draft)), &[], rich_keeps) {
        AtSend::Show(again) => assert_eq!(sheet_of(&again.draft).title, "'Enter' still does not send"),
        other => panic!("card title {:?}; no card shown again: {other:?}", typed.title),
    }
    let (shown, public) = sent_from(&typed, Some(&Check::of(&written.draft)), &[], rich_keeps);
    assert_sent_as_shown(&shown, &public);
}

#[test]
fn a_heading_the_scanner_or_rich_changes_is_shown_before_anything_is_sent() {
    // Finding 2: with a company named "Version" private, on d9cdd913f the card showed the
    // "Version" heading and the issue was filed with "[a company]" there.
    let version = serde_json::json!([{"text": "Version", "kind": "company"}]);
    let written = written_as("Window froze", "Version froze.", version);
    let card = sheet_of(&written.draft);
    match at_send(&card, Some(&Check::of(&written.draft)), &[], |_| panic!("asked about his own words")) {
        AtSend::Send(public) => assert_sent_as_shown(&card, &public),
        other => panic!("Rich's draft was not sendable: {other:?}"),
    }
    assert_eq!(card.sections.last().unwrap().heading, "[a company]", "the card does not show the heading as it goes");

    // A card that shows the heading, changed by hand to name the company, which Rich cleans up last:
    // his version, heading included, is shown before anything is sent.
    let plain = written_as("Window froze", "The window froze.", serde_json::json!([]));
    let mut edited = sheet_of(&plain.draft);
    assert_eq!(edited.sections.last().unwrap().heading, "Version");
    edited.sections[0].paragraphs[0] = "Version froze.".into();
    let rich = rich_replaces("Version", "company", "[a company]");
    match at_send(&edited, Some(&Check::of(&plain.draft)), &[], rich) {
        AtSend::Show(again) => assert_eq!(again.draft.sections.last().unwrap().heading, "[a company]"),
        other => panic!("card heading Version; no card shown again: {other:?}"),
    }
    let (shown, public) = sent_from(&edited, Some(&Check::of(&plain.draft)), &[], rich);
    assert_sent_as_shown(&shown, &public);
    assert!(!public.body().contains("Version"), "{}", public.body());
}

/// `text` with every stand-in taken out, escaped or not: what is left is words the user wrote.
fn without_stand_ins(text: &str) -> String {
    let mut out = text.replace("\\[", "[").replace("\\]", "]");
    for kind in Kind::ALL {
        out = out.replace(kind.stand_in(), "");
    }
    out
}

#[test]
fn every_card_is_word_for_word_the_issue_that_is_posted() {
    // The class, not the two cases: whatever the user typed on the card, it goes through the
    // scanner and then Rich, and what is posted is exactly what the card showed when Send was
    // pressed; a card shown again is sent as shown.
    let person = |t: &str| PrivateTerm::new(t, Kind::PersonName);
    let section = |heading: &str, paragraphs: &[&str], steps: &[&str]| SheetSection {
        heading: heading.into(),
        paragraphs: paragraphs.iter().map(|p| p.to_string()).collect(),
        steps: steps.iter().map(|s| s.to_string()).collect(),
    };
    let cases: Vec<(Sheet, Vec<PrivateTerm>)> = vec![
        // A title with backticks, marks and an @name in every field.
        (Sheet { title: "`Esc` *closes* @octocat's [panel]".into(), sections: vec![section("What happened @team", &["`code` and _this_ ~~gone~~ #1 <b>x</b> | a & b"], &["Press `Esc`"])] }, vec![]),
        // A name split across two paragraphs, and a path in a step: the step keeps its number.
        (Sheet { title: "It froze".into(), sections: vec![section("What happened", &["It was opened by Jane", "Doe saw it freeze."], &["Open /Users/jane/notes.txt now"])] }, vec![person("Jane Doe")]),
        // A heading that is a private name, and a name in the title.
        (Sheet { title: "Jane froze it".into(), sections: vec![section("What happened", &["It froze."], &[]), section("Version", &[VERSION], &[])] }, vec![person("Jane"), PrivateTerm::new("Version", Kind::CompanyName)]),
        // A private word that is also a word of a stand-in: the stand-in is not private.
        (Sheet { title: "Mac froze".into(), sections: vec![section("What happened", &["Mac lost it.", "It was in /Users/mac/x.txt then."], &[])] }, vec![person("Mac")]),
        // A path on a line that starts with a stand-in already on the card.
        (Sheet { title: "Notes vanished".into(), sections: vec![section("What happened", &["[a person] keeps notes in /Users/x/notes.txt"], &[])] }, vec![]),
        // Line breaks the user typed inside a paragraph join no two words.
        (Sheet { title: "Send".into(), sections: vec![section("What happened", &["Click\nSend does\nnothing twice."], &[])] }, vec![]),
    ];
    for (card, private) in cases {
        let report = Check::of(&draft_of(&sheet("before"), private.clone()));
        let (shown, public) = sent_from(&card, Some(&report), &[], rich_keeps);
        assert_sent_as_shown(&shown, &public);
        let words = without_stand_ins(&format!("{}\n{}", public.title(), public.body()));
        for p in &private {
            assert!(!words.to_lowercase().contains(&p.text.to_lowercase()), "{:?} was posted: {public:?}", p.text);
        }
        assert!(!words.contains("/Users"), "{public:?}");
        assert!(!words.contains("ClickSend") && !words.contains("doesnothing"), "{public:?}");
    }
}

// ---- the review of 593f4791b (rv-20261009T204435Z-593f4791-c2ff), fixture `core_probe.rs` ----

/// Every `<<< … >>>` block of a prompt that reads as JSON, in the order they are written.
fn json_blocks(prompt:&str)->Vec<serde_json::Value> {
    let data:serde_json::Value=serde_json::from_str(prompt).unwrap();
    vec![data["draft"].clone(),data["before_automatic_check"].clone()]
}

/// A Rich who, last, finds that nothing the scanner hid is private in this user's case: he answers
/// with the report as it was before the scanner (the second report his last pass is given), and,
/// given only the scanner's, with the scanner's.
fn rich_puts_back(prompt: &str) -> Result<String, String> {
    let blocks = json_blocks(prompt);
    let mut report = blocks.get(1).or(blocks.first()).cloned().expect("a report in the prompt");
    report["private"] = serde_json::json!([]);
    Ok(report.to_string())
}

#[test]
fn rich_puts_back_ordinary_words_the_scanner_hid_and_they_go_as_he_gave_them() {
    // Finding 1: on 593f4791b the scanner made "The Send/Cancel buttons overlap." into "[a file on
    // this Mac]." and Rich's last pass was given only that, told to keep every stand-in, so the card
    // read `["[a file on this Mac]."]`. The scanner still runs first; Rich, last, has the final say.
    let said = "The Send/Cancel buttons overlap.";
    let answer = serde_json::json!({"title": "Buttons overlap", "what_happened": said, "private": []}).to_string();
    let given = RefCell::new(String::new());
    let rich = |prompt: &str| {
        *given.borrow_mut() = prompt.to_string();
        rich_puts_back(prompt)
    };
    let done = checked(Ok(answer), &screen(), false, VERSION, &Scrubber::default(), rich).unwrap();
    assert_eq!(report_in(&given.borrow())["sections"][0]["paragraphs"][0], "[a file on this Mac].", "the scanner did not run first");
    let card = sheet_of(&done.draft);
    assert_eq!(card.sections[0].paragraphs, [said], "Rich could not put back what the scanner hid");
    assert!(done.draft.sections[0].paragraphs[0].iter().all(|s| s.was.is_none()), "{:?}", done.draft.sections[0].paragraphs[0]);
    match at_send(&card, Some(&Check::of(&done.draft)), &[], |_| panic!("Rich was asked again about his own words")) {
        AtSend::Send(public) => {
            assert_sent_as_shown(&card, &public);
            assert!(public.body().contains(said), "{:?}", public.body());
        }
        other => panic!("{other:?}"),
    }

    // Words changed by hand go through the scanner and then Rich, who puts them back as typed:
    // they go as the card shows them.
    let mut edited = card.clone();
    edited.sections[0].paragraphs[0] = "The Send/Cancel buttons overlap at 135% text size.".into();
    match at_send(&edited, Some(&Check::of(&done.draft)), &[], rich_puts_back) {
        AtSend::Send(public) => assert_sent_as_shown(&edited, &public),
        other => panic!("the user's own ordinary words were not sendable as typed: {other:?}"),
    }
}

#[test]
fn a_last_pass_with_a_malformed_section_is_not_a_report_and_waits_on_this_mac() {
    // Finding 2: on 593f4791b `finished` read sections `[{"paragraphs": "The window froze."}, {}]`
    // as a report, made the paragraph list empty, filled in the headings itself, and Send was
    // permitted with the body "### What happened\n\n### Version\n".
    let dir = scratch("malformed-sections");
    let outbox = Outbox::open(&dir);
    let said = "The window froze.";
    let answer = serde_json::json!({"title": "Window froze", "what_happened": said, "private": []}).to_string();
    let version = serde_json::json!({"heading": "Version", "paragraphs": [VERSION], "steps": []});
    let malformed = [
        ("the review's fixture", serde_json::json!([{"paragraphs": said}, {}])),
        ("a section with no heading", serde_json::json!([{"paragraphs": [said], "steps": []}, version])),
        ("a heading that is not words", serde_json::json!([{"heading": 1, "paragraphs": [said], "steps": []}, version])),
        ("a paragraph that is not words", serde_json::json!([{"heading": "What happened", "paragraphs": [7], "steps": []}, version])),
        ("steps that are not a list", serde_json::json!([{"heading": "What happened", "paragraphs": [said], "steps": "none"}, version])),
        ("a section that is not an object", serde_json::json!(["What happened", version])),
        ("a section whose words are gone", serde_json::json!([{"heading": "What happened", "paragraphs": [], "steps": []}, version])),
    ];
    let card = Sheet {
        title: "Window froze".into(),
        sections: vec![
            SheetSection { heading: "What happened".into(), paragraphs: vec![said.into()], steps: vec![] },
            SheetSection { heading: "Version".into(), paragraphs: vec![VERSION.into()], steps: vec![] },
        ],
    };
    let mut offered = Vec::new();
    for (label, sections) in &malformed {
        let reply = serde_json::json!({"title": "Window froze", "sections": sections, "private": []}).to_string();
        let check = checked(Ok(answer.clone()), &screen(), false, VERSION, &Scrubber::default(), |_| Ok(reply.clone()));
        if let WriteUp::Checked(c) = outbox.keep_unless_checked(said, &screen(), check, 1_000).unwrap() {
            offered.push(format!("{label}: offered for sending, body {:?}", issue_body(&sheet_of(&c.draft))));
        }
        // Words changed by hand, given to Rich last at Send: nothing goes either.
        match at_send(&card, None, &[], |_| Ok(reply.clone())) {
            AtSend::Unchecked(_) => {}
            other => offered.push(format!("{label}: at Send, {other:?}")),
        }
    }
    assert!(offered.is_empty(), "a malformed last pass was offered:\n{}", offered.join("\n"));
    // Every one is kept on this Mac, unsent, to ask Rich again.
    assert_eq!(outbox.unchecked().unwrap().len(), malformed.len());
    assert!(outbox.pending().unwrap().is_empty() && outbox.sent().unwrap().is_empty());
    assert!(outbox.unchecked().unwrap().iter().all(|u| u.checked.is_none() && u.next_try_at_ms == 1_000 + CHECK_RETRY_MS));

    // A well-formed last pass is still a report; a list a section never had may be left out.
    let good = serde_json::json!({"title": "Window froze", "sections": [{"heading": "What happened", "paragraphs": [said], "steps": []}, {"heading": "Version", "paragraphs": [VERSION]}], "private": []}).to_string();
    let done = checked(Ok(answer), &screen(), false, VERSION, &Scrubber::default(), |_| Ok(good.clone())).unwrap();
    assert_eq!(sheet_of(&done.draft), card);
    std::fs::remove_dir_all(dir).unwrap();
}

// ---- nightly 48's walk, D23 (docs/verification/2026-10-10-nightly-48-candidate-walk.md) ----

#[test]
fn every_stand_in_on_the_card_is_marked_and_counted_whatever_rich_listed_beside_it() {
    // D23: Rich wrote the stand-ins himself and listed nothing private beside them (what his
    // prompt asks for: what is still in his words, and nothing was). On d0d06501c none of the four
    // was marked, and the line under the card said "Nothing private was in it, so nothing was left
    // out." under all four. The line and the marks follow the words, not the list.
    let written = serde_json::json!({
        "title": "Sidebar names are cut off when the text size is made bigger",
        "what_happened": "In [a conversation], the user made the text bigger and the names in the sidebar got cut off. \
                          They say [a person] at [a company] sees the same thing. They also say their notes in \
                          [a file on this Mac] now look empty.",
        "where": "The sidebar on the left.",
        "steps": [],
        "expected": "",
        "checked": "",
        "private": []
    })
    .to_string();
    let draft = checked(Ok(written), &screen(), false, VERSION, &Scrubber::new(terms()), rich_keeps).unwrap().draft;
    let mut all: Vec<Segment> = draft.title.clone();
    for section in &draft.sections {
        all.extend(section.paragraphs.iter().chain(&section.steps).flatten().cloned());
    }
    let marked: Vec<(&str, Option<Kind>)> = all.iter().filter(|s| s.kind.is_some()).map(|s| (s.text.as_str(), s.kind)).collect();
    assert_eq!(
        marked,
        [
            ("[a conversation]", Some(Kind::ConversationName)),
            ("[a person]", Some(Kind::PersonName)),
            ("[a company]", Some(Kind::CompanyName)),
            ("[a file on this Mac]", Some(Kind::FilePath)),
        ],
        "a stand-in on the card is not marked"
    );
    // Nothing accounts for what they replaced, so none claims to know.
    assert!(all.iter().all(|s| s.was.is_none()), "{all:?}");
    assert_eq!(
        left_out_line(&all),
        "Left out, because anyone can read GitHub issues: one conversation name, one person's name, one company name \
         and one file path. The underlined words stand in for them; point at one to see what it replaced."
    );
    // The words are Rich's, to the character.
    assert!(draft_words(&draft).contains("They say [a person] at [a company] sees the same thing."));
}

// ---- the review of f29db7082 (rv-20261010T180313Z-f29db708-733e), finding 1, fixture `change-standins.rs` ----

#[test]
fn stand_ins_in_a_change_are_marked_and_counted_as_in_the_first_draft() {
    // On f29db7082 `scrub_change` left the stand-ins Rich wrote in a change as plain words, so the
    // card said "Nothing private was in it, so nothing was left out." under "[a person] could not
    // open [a file on this Mac] either." A name the scanner leaves out beside them keeps what it
    // replaced; his own say their kind alone, as in the first draft.
    let change = change_naming("[a person] could not open [a file on this Mac] either, and Jane Doe saw it.", "[]");
    let report = vec![PrivateTerm::new("Jane Doe", Kind::PersonName)];
    let (add, _) = scrub_change(&change, &[], &report);
    assert_eq!(joined(&add), "[a person] could not open [a file on this Mac] either, and [a person] saw it.");
    let marked: Vec<(&str, Option<Kind>, Option<&str>)> =
        add.iter().filter(|s| s.is_stand_in()).map(|s| (s.text.as_str(), s.kind, s.was.as_deref())).collect();
    assert_eq!(
        marked,
        [
            ("[a person]", Some(Kind::PersonName), None),
            ("[a file on this Mac]", Some(Kind::FilePath), None),
            ("[a person]", Some(Kind::PersonName), Some("Jane Doe")),
        ],
        "a stand-in in the change is not marked: {add:?}"
    );
    assert_eq!(
        left_out_line(&add),
        "Left out, because anyone can read GitHub issues: two people's names and one file path. The underlined words \
         stand in for them; point at one to see what it replaced."
    );
    // The review's own words, alone: both marked, none plain.
    let (add, _) = scrub_change(&change_naming("[a person] could not open [a file on this Mac] either.", "[]"), &[], &[]);
    assert_eq!(add.iter().filter(|s| s.is_stand_in()).count(), 2, "{add:?}");
    assert_eq!(joined(&add), "[a person] could not open [a file on this Mac] either.");
}

// ---- the review of 9463220ef (rv-20261010T072702Z-9463220e-543f), fixture `mixed-stand-in-receipts.py` ----

/// What each stand-in on `draft`'s card says it replaced, in the order they are met.
fn receipts(draft: &Draft) -> Vec<Option<String>> {
    let mut all: Vec<&Segment> = draft.title.iter().collect();
    for section in &draft.sections {
        all.extend(section.paragraphs.iter().chain(&section.steps).flatten());
    }
    all.into_iter().filter(|s| s.is_stand_in()).map(|s| s.was.clone()).collect()
}

#[test]
fn a_stand_in_names_only_the_words_it_replaced_where_it_stands() {
    // Finding 1: a report already carries a [a person] Rich wrote for Bob, and the user adds "Alex
    // saw it too" with Change it, Alex being a name RichOS holds. On 9463220ef the scanner's receipt
    // for Alex went to the first [a person] of the report, so Bob's stand-in said it replaced Alex
    // and Alex's said nothing. A stand-in is tied to what it replaced by its place, or claims nothing.
    let alex = [PrivateTerm::new("Alex", Kind::PersonName)];
    let edit = |said: &str| Edited {
        sheet: Sheet {
            title: "Window froze".into(),
            sections: vec![SheetSection { heading: "What happened".into(), paragraphs: vec![said.into()], steps: vec![] }],
        },
        private: vec![],
    };

    // The review's fixture: the earlier stand-in claims nothing; Alex's names Alex.
    let done = checked_edit(&edit("[a person] saw it. Alex saw it too."), &alex, rich_keeps).unwrap();
    assert_eq!(draft_words(&done.draft), "Window froze\n[a person] saw it. [a person] saw it too.", "the words changed");
    assert_eq!(receipts(&done.draft), [None, Some("Alex".to_string())], "a stand-in names words it did not replace");

    // Rich cleans up a name written before the scanner's: each names its own.
    let done = checked_edit(&edit("Carol saw it. Alex saw it too."), &alex, rich_replaces("Carol", "person", "[a person]")).unwrap();
    assert_eq!(receipts(&done.draft), [Some("Carol".to_string()), Some("Alex".to_string())]);

    // Rich writes a stand-in he lists nothing for beside the scanner's: which one is Alex's cannot
    // be told, so neither claims to know.
    let rich_hides_bob = |prompt: &str| {
        let mut report: serde_json::Value = serde_json::from_str(&report_in(prompt).to_string().replace("Bob", "[a person]")).unwrap();
        report["private"] = serde_json::json!([]);
        Ok(report.to_string())
    };
    let done = checked_edit(&edit("Bob saw it. Alex saw it too."), &alex, rich_hides_bob).unwrap();
    assert_eq!(receipts(&done.draft), [None, None]);
}

/// THE GITHUB MOVE (CEO, 2026-10-27): a sent card opens the link the relay answered with, so a
/// report sent before the move opens its issue on the old address and one sent after opens it on
/// the new one. The webview passes the link; only a post of the right kind on RichOS's repository,
/// under either address, is opened, and on a test relay only that relay's own repository.
#[test]
fn a_sent_card_opens_the_relay_link_on_either_address_and_nothing_else() {
    for (url, post) in [
        ("https://github.com/WebDevBooster/richos/issues/412", Post::Bug),
        ("https://github.com/RichSpark/RichOS/issues/413", Post::Bug),
        ("https://github.com/WebDevBooster/richos/discussions/9", Post::Idea),
        ("https://github.com/RichSpark/RichOS/discussions/10", Post::Idea),
    ] {
        assert_eq!(page_to_open(url, post, REPOSITORY).as_deref(), Some(url), "{url}");
    }
    for (url, post) in [
        ("https://github.com/RichSpark/RichOS/issues/413", Post::Idea),
        ("https://github.com/RichSpark/RichOS/discussions/10", Post::Bug),
        ("https://github.com/RichSpark/RichOS-HQ/issues/1", Post::Bug),
        ("https://github.com/RichCore/RichOS/issues/1", Post::Bug),
        ("https://github.com/someone/else/issues/1", Post::Bug),
        ("https://evil.example/RichSpark/RichOS/issues/1", Post::Bug),
        ("file:///etc/passwd", Post::Bug),
    ] {
        assert_eq!(page_to_open(url, post, REPOSITORY), None, "{url}");
    }
    let test = "WebDevBooster/richos-report-test";
    assert!(page_to_open("https://github.com/WebDevBooster/richos-report-test/issues/3", Post::Bug, test).is_some());
    assert_eq!(page_to_open("https://github.com/RichSpark/RichOS/issues/3", Post::Bug, test), None);
}
