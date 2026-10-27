//! **REQUEST A FEATURE** — the CEO, 2026-10-11 (richos-hq `docs/ceo-input/2026-10-11_01/request.md`),
//! the acceptance criterion: *"Under the "Bust a bug" button, I want to add a "Request a feature"
//! button. But I don't want feature requests to be submitted or branded as "issues" on GitHub."*
//! And on how it behaves: *"can things not be copy-pasted from the "Bust a bug" feature?"*
//!
//! So a feature idea goes through Bust a bug's own machinery (`richos_core::bug_report`, tested in
//! `bug_report_tests.rs`), and these tests hold it to the one difference and to what it shares:
//!
//!   1. **Where it goes.** A post in RichOS's GitHub Discussions under Ideas, through GraphQL's
//!      `createDiscussion` with the repository's and the Ideas category's ids; never the issues
//!      endpoint.
//!   2. **Whose account.** The user's own GitHub sign-in when there is one (a stand-in credential
//!      in `Account::User`, the slot sign-in will fill), and the RichOS reporting account otherwise.
//!   3. **An account that may not post there** is said honestly: the idea waits, refused.
//!   4. **Offline it waits and posts by itself** when the Mac is back online, as a bug report does.
//!   5. **Private names are left out**, by the same scanner and Rich last, and nothing in the idea
//!      Rich writes up calls it an issue or a bug.
//!
//! Everything runs against a stand-in GitHub (`FakeGitHub`) and stand-in credentials: no test can
//! post anything, and none reads the login keychain.

use richos_core::bug_report::*;
use std::cell::RefCell;
use std::path::PathBuf;

const VERSION: &str = "RichOS 1.2.0, nightly 45 · macOS 15.6 · Apple silicon";
const REPO_ID: &str = "R_kgDOStandIn";
const IDEAS_ID: &str = "DIC_kwDOStandIn";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("richos-feature-request-{name}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A stand-in for GitHub: answers each GraphQL request with the next of `replies`, and records
/// every request it was sent (the token and the request) and every issue it was asked to file.
struct FakeGitHub {
    replies: RefCell<Vec<Reply>>,
    graphql: RefCell<Vec<(String, serde_json::Value)>>,
    issues: RefCell<Vec<String>>,
}

impl FakeGitHub {
    fn answering(replies: Vec<Reply>) -> Self {
        FakeGitHub { replies: RefCell::new(replies), graphql: RefCell::new(vec![]), issues: RefCell::new(vec![]) }
    }
}

impl Transport for FakeGitHub {
    fn create_issue(&self, _: &Credential, path: &str, _: &serde_json::Value) -> Outcome {
        self.issues.borrow_mut().push(path.to_string());
        Outcome::Status { code: 500 }
    }
    fn graphql(&self, credential: &Credential, request: &serde_json::Value) -> Reply {
        self.graphql.borrow_mut().push((credential.token.clone(), request.clone()));
        self.replies.borrow_mut().remove(0)
    }
}

fn ok(data: serde_json::Value) -> Reply {
    Reply::Answered { status: 200, body: serde_json::json!({ "data": data }) }
}

/// GitHub's answer to the lookup: the repository and its Ideas category.
fn found() -> Reply {
    ok(serde_json::json!({ "repository": { "id": REPO_ID, "discussionCategory": { "id": IDEAS_ID } } }))
}

/// GitHub's answer to `createDiscussion`: discussion `number`.
fn created(number: u64) -> Reply {
    ok(serde_json::json!({ "createDiscussion": { "discussion": { "number": number, "url": format!("https://github.com/RichCore/RichOS/discussions/{number}") } } }))
}

/// Who it goes out as: the user's own GitHub sign-in, the reporting account, or nobody.
enum Who {
    User,
    Reporting,
}

impl Credentials for Who {
    fn credential(&self) -> Option<Credential> {
        Some(match self {
            Who::User => Credential { account: Account::User { login: "WebDevBooster".into() }, token: "user-stand-in".into() },
            Who::Reporting => Credential { account: Account::Reporting, token: "reporting-stand-in".into() },
        })
    }
}

/// An idea's card, as Rich wrote it and the user approved it.
fn idea() -> Sheet {
    Sheet {
        title: "A short weekly summary of each conversation, from Rich".into(),
        sections: vec![
            SheetSection { heading: "The idea".into(), paragraphs: vec!["Once a week, Rich sends a short summary of each conversation.".into()], steps: vec![] },
            SheetSection { heading: "Why it would help".into(), paragraphs: vec!["Catching up today means reading back through each one.".into()], steps: vec![] },
            SheetSection { heading: BULLETED.into(), paragraphs: vec![], steps: vec!["A message every Friday afternoon.".into(), "One short part per conversation.".into()] },
            SheetSection { heading: "Version".into(), paragraphs: vec![VERSION.into()], steps: vec![] },
        ],
    }
}

fn plain(text: &str) -> Vec<Segment> {
    vec![Segment { text: text.to_string(), was: None, kind: None }]
}

/// What may leave this Mac of `sheet`, when it is exactly what Rich gave last.
fn public_of(sheet: &Sheet) -> Public {
    let draft = Draft {
        title: plain(&sheet.title),
        sections: sheet
            .sections
            .iter()
            .map(|s| DraftSection { heading: s.heading.clone(), paragraphs: s.paragraphs.iter().map(|p| plain(p)).collect(), steps: s.steps.iter().map(|p| plain(p)).collect() })
            .collect(),
        private: vec![],
    };
    match at_send_for(Post::Idea, sheet, Some(&Check::of(&draft)), &[], |_| panic!("Rich was asked about words he gave")) {
        AtSend::Send(public) => public,
        other => panic!("not sendable: {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------
// 1. WHERE IT GOES: A DISCUSSION UNDER IDEAS, NEVER AN ISSUE
// ---------------------------------------------------------------------------------------

#[test]
fn an_idea_is_posted_with_create_discussion_in_the_ideas_category_and_never_to_issues() {
    let dir = scratch("posted");
    let outbox = Outbox::open_for(&dir, Post::Idea);
    let github = FakeGitHub::answering(vec![found(), created(413)]);
    let delivery = outbox.send(&public_of(&idea()), &Who::Reporting, &github, 1_000).unwrap();

    assert!(github.issues.borrow().is_empty(), "an idea went to the issues endpoint: {:?}", github.issues.borrow());
    let seen = github.graphql.borrow();
    assert_eq!(seen.len(), 2, "one lookup and one createDiscussion: {seen:?}");
    // The lookup: the repository and its Ideas category, by the category's slug.
    let lookup = &seen[0].1;
    assert!(lookup["query"].as_str().unwrap().contains("discussionCategory(slug: $slug)"), "{lookup}");
    assert_eq!(lookup["variables"], serde_json::json!({ "owner": "RichCore", "name": "RichOS", "slug": "ideas" }));
    // The post: createDiscussion with those ids, and the card's words exactly.
    let create = &seen[1].1;
    assert!(create["query"].as_str().unwrap().contains("createDiscussion(input:"), "{create}");
    assert!(!create["query"].as_str().unwrap().to_lowercase().contains("issue"), "{create}");
    assert_eq!(create["variables"]["repositoryId"], REPO_ID);
    assert_eq!(create["variables"]["categoryId"], IDEAS_ID);
    assert_eq!(create["variables"]["title"], "A short weekly summary of each conversation, from Rich");
    assert_eq!(create["variables"]["body"], issue_body(&idea()), "the body is the card's words, nothing added");

    match delivery {
        Delivery::Sent(sent) => {
            assert_eq!(sent.number, 413);
            assert_eq!(sent.url, "https://github.com/RichCore/RichOS/discussions/413");
            assert_eq!(sent.post, Post::Idea);
        }
        other => panic!("not posted: {other:?}"),
    }
    assert!(outbox.pending().unwrap().is_empty());
    assert_eq!(outbox.sent().unwrap().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn what_the_user_pictured_is_written_as_bullets_and_a_bug_reports_steps_stay_numbered() {
    let body = issue_body(&idea());
    assert!(body.contains("### What the user pictured\n\n- A message every Friday afternoon.\n- One short part per conversation.\n"), "{body}");
    let bug = Sheet { title: "t".into(), sections: vec![SheetSection { heading: "Steps to see it".into(), paragraphs: vec![], steps: vec!["Open it".into(), "Look".into()] }] };
    assert!(issue_body(&bug).contains("1. Open it\n2. Look\n"), "{}", issue_body(&bug));
    assert_eq!(discussion_page(413), "https://github.com/RichCore/RichOS/discussions/413");
}

// ---------------------------------------------------------------------------------------
// 2. WHOSE ACCOUNT
// ---------------------------------------------------------------------------------------

#[test]
fn the_users_own_sign_in_is_used_when_there_is_one_and_the_reporting_account_otherwise() {
    for (who, token, account) in [
        (Who::User, "user-stand-in", Account::User { login: "WebDevBooster".into() }),
        (Who::Reporting, "reporting-stand-in", Account::Reporting),
    ] {
        let dir = scratch("account");
        let github = FakeGitHub::answering(vec![found(), created(7)]);
        let delivery = Outbox::open_for(&dir, Post::Idea).send(&public_of(&idea()), &who, &github, 0).unwrap();
        assert!(github.graphql.borrow().iter().all(|(t, _)| t == token), "another token was used: {:?}", github.graphql.borrow());
        assert!(matches!(&delivery, Delivery::Sent(s) if s.account == account), "{delivery:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

// ---------------------------------------------------------------------------------------
// 3. AN ACCOUNT THAT MAY NOT POST THERE
// ---------------------------------------------------------------------------------------

#[test]
fn a_credential_without_permission_to_post_discussions_is_refused_and_the_idea_waits() {
    let forbidden = Reply::Answered {
        status: 200,
        body: serde_json::json!({ "data": { "createDiscussion": null }, "errors": [{ "type": "FORBIDDEN", "message": "Resource not accessible by personal access token" }] }),
    };
    let scopes = Reply::Answered { status: 200, body: serde_json::json!({ "errors": [{ "type": "INSUFFICIENT_SCOPES", "message": "needs a scope" }] }) };
    let bad_token = Reply::Answered { status: 401, body: serde_json::json!({ "message": "Bad credentials" }) };
    let no_category = ok(serde_json::json!({ "repository": { "id": REPO_ID, "discussionCategory": null } }));
    for replies in [vec![found(), forbidden], vec![scopes], vec![bad_token], vec![no_category]] {
        let dir = scratch("refused");
        let outbox = Outbox::open_for(&dir, Post::Idea);
        let github = FakeGitHub::answering(replies.clone());
        match outbox.send(&public_of(&idea()), &Who::Reporting, &github, 0).unwrap() {
            Delivery::Waiting { reason, .. } => assert_eq!(reason, Reason::Refused, "{replies:?}"),
            other => panic!("{replies:?}: {other:?}"),
        }
        assert_eq!(outbox.pending().unwrap().len(), 1, "the approved idea is not kept");
        assert!(github.issues.borrow().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn no_account_on_this_mac_waits_as_not_set_up() {
    struct Nobody;
    impl Credentials for Nobody {
        fn credential(&self) -> Option<Credential> {
            None
        }
    }
    let dir = scratch("nobody");
    let github = FakeGitHub::answering(vec![]);
    match Outbox::open_for(&dir, Post::Idea).send(&public_of(&idea()), &Nobody, &github, 0).unwrap() {
        Delivery::Waiting { reason, .. } => assert_eq!(reason, Reason::NotSetUp),
        other => panic!("{other:?}"),
    }
    assert!(github.graphql.borrow().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

// ---------------------------------------------------------------------------------------
// 4. OFFLINE IT WAITS, AND POSTS BY ITSELF WHEN BACK ONLINE
// ---------------------------------------------------------------------------------------

#[test]
fn offline_the_idea_waits_on_this_mac_and_is_posted_by_itself_when_back_online() {
    let dir = scratch("offline");
    let approved = idea();
    {
        let outbox = Outbox::open_for(&dir, Post::Idea);
        match outbox.send(&public_of(&approved), &Who::Reporting, &FakeGitHub::answering(vec![Reply::Unreachable]), 10_000).unwrap() {
            Delivery::Waiting { reason, .. } => assert_eq!(reason, Reason::Offline),
            other => panic!("{other:?}"),
        }
    }
    // A relaunch: a fresh outbox over the same folder still holds it, word for word.
    let outbox = Outbox::open_for(&dir, Post::Idea);
    let waiting = outbox.pending().unwrap();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].body, issue_body(&approved));
    // GitHub not answering (a timeout, or a 503) is "GitHub down", and it still waits.
    let due = waiting[0].next_try_at_ms;
    match &outbox.send_due(&Who::Reporting, &FakeGitHub::answering(vec![Reply::TimedOut]), due).unwrap()[..] {
        [Delivery::Waiting { reason, .. }] => assert_eq!(*reason, Reason::GithubDown),
        other => panic!("{other:?}"),
    }
    // Back online and due: it is posted by itself, as a discussion, with the words approved.
    let due = outbox.pending().unwrap()[0].next_try_at_ms;
    let online = FakeGitHub::answering(vec![found(), created(414)]);
    let posted = outbox.send_due(&Who::Reporting, &online, due).unwrap();
    assert!(matches!(&posted[..], [Delivery::Sent(s)] if s.number == 414 && s.post == Post::Idea), "{posted:?}");
    assert_eq!(online.graphql.borrow()[1].1["variables"]["body"], issue_body(&approved));
    assert!(online.issues.borrow().is_empty());
    assert!(outbox.pending().unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

// ---------------------------------------------------------------------------------------
// 5. PRIVATE NAMES ARE LEFT OUT, AND IT IS NEVER CALLED AN ISSUE OR A BUG
// ---------------------------------------------------------------------------------------

fn screen() -> Screen {
    Screen {
        key: "conversation".into(),
        here: "the Acme deal conversation".into(),
        public: "a conversation, the main RichOS screen".into(),
        conversation: Some("Acme deal".into()),
        text_size: 100,
        theme: "dark".into(),
        technical_view: false,
        content: String::new(),
    }
}

/// Rich, last, finding nothing more to clean up: his words are the scanner's.
fn rich_keeps(prompt: &str) -> Result<String, String> {
    let mut report=serde_json::from_str::<serde_json::Value>(prompt).unwrap()["draft"].clone();
    report["private"] = serde_json::json!([]);
    Ok(report.to_string())
}

#[test]
fn private_names_are_left_out_of_an_idea_as_in_a_bug_report_and_its_sections_are_round_24s() {
    let terms = vec![PrivateTerm::new("Acme deal", Kind::ConversationName), PrivateTerm::new("Dana Whitfield", Kind::PersonName)];
    let rich = serde_json::json!({
        "title": "A weekly summary of each conversation",
        "idea": "Once a week, Rich sends a short summary of each conversation, for example the Acme deal.",
        "why": "Dana Whitfield reads back through everything today.",
        "pictured": ["A message every Friday afternoon.", "One short part per conversation."],
        "where": "",
        "checked": "",
        "private": [],
    })
    .to_string();
    let mut prompts = Vec::new();
    let done = checked_for(Post::Idea, Ok(rich), &screen(), false, VERSION, &Scrubber::new(terms), |p| {
        prompts.push(p.to_string());
        rich_keeps(p)
    })
    .expect("Rich's checked idea");
    let sheet = sheet_of(&done.draft);
    let headings: Vec<&str> = sheet.sections.iter().map(|s| s.heading.as_str()).collect();
    assert_eq!(headings, ["The idea", "Why it would help", "What the user pictured", "Version"]);
    let words = serde_json::to_string(&sheet).unwrap();
    for private in ["Acme", "Dana", "Whitfield"] {
        assert!(!words.contains(private), "\"{private}\" is in the post: {words}");
    }
    assert!(words.contains("[a conversation]") && words.contains("[a person]"), "{words}");
    // What each stand-in replaced is kept for the user's eyes on the card, never in the post.
    let was: Vec<String> = done.draft.sections.iter().flat_map(|s| s.paragraphs.iter().flatten()).filter_map(|s| s.was.clone()).collect();
    assert!(was.contains(&"Acme deal".to_string()) && was.contains(&"Dana Whitfield".to_string()), "{was:?}");
    // Nothing in the post calls it an issue or a bug, and Rich's last pass is told it is posted
    // in Discussions, not filed as an issue.
    let body = issue_body(&sheet).to_lowercase() + &sheet.title.to_lowercase();
    assert!(!body.contains("issue") && !body.contains("bug"), "{body}");
    assert!(prompts[0].contains("feature idea") && prompts[0].contains("GitHub Discussions, under Ideas"), "{}", prompts[0]);
    assert!(!prompts[0].contains("GitHub issue") && !prompts[0].contains("bug report"), "{}", prompts[0]);
}

#[test]
fn the_ideas_prompts_ask_for_an_idea_and_a_change_is_about_the_post() {
    let prompt = writer_prompt_for(Post::Idea, "a weekly summary", &screen(), VERSION, &[], false);
    assert_eq!(serde_json::from_str::<serde_json::Value>(&prompt).unwrap()["kind"],"feature idea");

    assert!(prompt.contains("\"idea\"") && prompt.contains("\"why\"") && prompt.contains("\"pictured\""), "{prompt}");
    let change = change_prompt_for(Post::Idea, &idea(), "also say I'd like to pick the day myself");
    assert!(!change.contains("bug report"), "{change}");
    // An answer with no idea is refused, as a report with nothing that happened is.
    assert!(parse_written_for(Post::Idea, r#"{"title": "T", "idea": "", "private": []}"#).is_err());
    let read = parse_written_for(Post::Idea, r#"{"title": "T", "idea": "I", "why": "W", "pictured": ["P"], "where": "", "private": []}"#).unwrap();
    assert_eq!((read.what_happened.as_str(), read.expected.as_str(), read.steps.as_slice(), read.post), ("I", "W", &["P".to_string()][..], Post::Idea));
}

#[test]
fn changed_words_of_an_idea_go_through_the_scanner_and_rich_at_send_and_are_shown_first_when_his_differ() {
    let mut edited = idea();
    edited.sections[0].paragraphs[0].push_str(" The Acme deal is one.");
    let app = vec![PrivateTerm::new("Acme deal", Kind::ConversationName)];
    let mut asked = Vec::new();
    match at_send_for(Post::Idea, &edited, None, &app, |p| {
        asked.push(p.to_string());
        rich_keeps(p)
    }) {
        AtSend::Show(his) => assert!(serde_json::to_string(&sheet_of(&his.draft)).unwrap().contains("The [a conversation] is one."), "{his:?}"),
        other => panic!("{other:?}"),
    }
    assert!(asked[0].contains("feature idea"), "{}", asked[0]);
}
