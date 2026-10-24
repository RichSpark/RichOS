use richos_core::questions::*;
use std::path::PathBuf;
struct Fixture {
    root: PathBuf,
    scope: AskScope,
    store: Store,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("richos-questions-test-{}", uuid::Uuid::new_v4()));
        let scope = AskScope { obligation_id:None,
            root: root.clone(),
            entity_id: "entity".into(),
            thread_id: "thread".into(),
            turn_id: "turn".into(),
            asker: "front_desk".into(),
            session_id: "original-session".into(),
            engine: None,
            entity_root: None,
            app_run: None,
        };
        Self {
            store: Store::new(&root),
            root,
            scope,
        }
    }
    fn ask(&self, n: usize) -> Vec<Question> {
        self.store
            .ask(
                &self.scope,
                (0..n)
                    .map(|_| QuestionInput {
                        text: "When should the release ship?".into(),
                        options: vec![
                            OptionInput {
                                label: "Ship today".into(),
                                description: "Users get the fixes sooner.".into(),
                            },
                            OptionInput {
                                label: "Ship tomorrow".into(),
                                description: "Allow another day for testing.".into(),
                            },
                        ],
                        multiple: false,
                        free_answer: true,
                        recommended: Some(1),
                    })
                    .collect(),
            )
            .unwrap()
    }
    fn answer(
        &self,
        q: &Question,
        key: &str,
        choice: usize,
        revision: Option<u64>,
    ) -> AnswerResult {
        self.store
            .answer(
                "entity",
                "thread",
                AnswerRequest {
                    question_id: q.id.clone(),
                    client_id: key.into(),
                    option_ids: vec![q.options[choice].id.clone()],
                    text: String::new(),
                    expected_revision: revision,
                },
                "click",
                "mac",
            )
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.root));
    }
}

#[test]
fn plain_and_spoken_rendering_names_the_same_asker_as_the_card() {
    let mut f = Fixture::new();
    let front = f.ask(1).remove(0);
    assert!(front.render().contains("Rich recommends Ship tomorrow"));
    f.scope.asker = "assignment-1".into();
    let team = f.ask(1).remove(0);
    assert_eq!(team.public_value()["asker"], "Your team");
    assert!(team.render().starts_with("Your team, through Rich\n"));
    assert!(team.render().contains("Your team recommends Ship tomorrow"));
}
#[test]
fn partial_sets_wait_then_deliver_once_with_every_answer() {
    let f = Fixture::new();
    let qs = f.ask(2);
    f.answer(&qs[0], "first", 0, None);
    assert!(f.store.pending_threads().unwrap().is_empty());
    let second = AnswerRequest {
        question_id: qs[1].id.clone(),
        client_id: "phone".into(),
        option_ids: vec![],
        text: "Next Tuesday".into(),
        expected_revision: None,
    };
    f.store
        .answer("entity", "thread", second, "phone_voice", "phone")
        .unwrap();
    let mut texts = vec![];
    assert_eq!(
        f.store
            .deliver("entity", "thread", "front_desk", |d| {
                texts.push(d.text.clone());
                Ok("durable-turn".into())
            })
            .unwrap(),
        1
    );
    assert!(texts[0].contains("Ship today"));
    assert!(texts[0].contains("Next Tuesday"));
    assert!(texts[0].contains("phone_voice on phone"));
    assert_eq!(
        f.store
            .deliver("entity", "thread", "front_desk", |_| panic!("duplicate"))
            .unwrap(),
        0
    );
    assert!(f
        .store
        .list("entity", "thread")
        .unwrap()
        .iter()
        .all(|q| q.delivered));
}
#[test]
fn withdrawal_resolves_partial_set_but_all_withdrawn_starts_nothing() {
    let f = Fixture::new();
    let qs = f.ask(2);
    f.answer(&qs[0], "a", 0, None);
    f.store
        .withdraw(
            "entity",
            "thread",
            "front_desk",
            &qs[1].id,
            "The deadline changed",
        )
        .unwrap();
    f.store
        .deliver("entity", "thread", "front_desk", |d| {
            assert!(d.text.contains("The deadline changed"));
            Ok("turn".into())
        })
        .unwrap();
    let qs = f.ask(2);
    for q in qs {
        f.store
            .withdraw("entity", "thread", "front_desk", &q.id, "No longer needed")
            .unwrap();
    }
    assert!(f.store.pending_threads().unwrap().is_empty());
}
#[test]
fn first_answer_wins_conflicts_become_words_and_retries_do_not_duplicate() {
    let f = Fixture::new();
    let q = f.ask(1).remove(0);
    let accepted = f.answer(&q, "mac", 0, None);
    assert_eq!(accepted.outcome, "accepted");
    assert_eq!(f.answer(&q, "mac", 0, None).outcome, "accepted");
    assert_eq!(
        f.answer(&q, "other-phone", 0, None).outcome,
        "already_answered"
    );
    assert_eq!(f.answer(&q, "conflict", 1, None).outcome, "conflict");
    f.answer(&q, "conflict", 1, None);
    let mut n = 0;
    f.store
        .deliver("entity", "thread", "front_desk", |_| {
            n += 1;
            Ok("saved".into())
        })
        .unwrap();
    assert_eq!(n, 2);
    assert_eq!(
        f.store.list("entity", "thread").unwrap()[0]
            .answer
            .as_ref()
            .unwrap()
            .option_ids,
        vec![q.options[0].id.clone()]
    );
}
#[test]
fn explicit_revision_edit_only_before_handoff() {
    let f = Fixture::new();
    let q = f.ask(1).remove(0);
    f.answer(&q, "initial", 0, None);
    assert_eq!(f.answer(&q, "edit", 1, Some(1)).outcome, "accepted");
    let mut id = String::new();
    assert!(f
        .store
        .deliver("entity", "thread", "front_desk", |d| {
            id = d.id.clone();
            Err("receiver unavailable".into())
        })
        .is_err());
    assert_eq!(f.answer(&q, "late-edit", 0, Some(2)).outcome, "conflict");
    let reopened = Store::new(&f.root);
    let mut seen = vec![];
    reopened
        .deliver("entity", "thread", "front_desk", |d| {
            seen.push(d.id.clone());
            if d.id == id {
                assert!(d.text.contains("Ship tomorrow"));
            }
            Ok("durable".into())
        })
        .unwrap();
    assert!(seen.contains(&id));
    assert_eq!(seen.len(), 2);
}
#[test]
fn simultaneous_submissions_keep_exactly_one_answer() {
    let f = Fixture::new();
    let q = f.ask(1).remove(0);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|i| {
            let store = f.store.clone();
            let q = q.clone();
            let b = barrier.clone();
            std::thread::spawn(move || {
                b.wait();
                store
                    .answer(
                        "entity",
                        "thread",
                        AnswerRequest {
                            question_id: q.id,
                            client_id: format!("phone-{i}"),
                            option_ids: vec![q.options[i].id.clone()],
                            text: "".into(),
                            expected_revision: None,
                        },
                        "phone_tap",
                        "phone",
                    )
                    .unwrap()
                    .outcome
            })
        })
        .collect();
    let mut outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    outcomes.sort();
    assert_eq!(outcomes, vec!["accepted", "conflict"]);
}
#[test]
fn deleted_conversation_cancels_delivery_and_keeps_late_words_on_sender() {
    let f = Fixture::new();
    let q = f.ask(1).remove(0);
    f.answer(&q, "first", 0, None);
    f.store
        .close("entity", "thread", None, "Conversation deleted", true)
        .unwrap();
    assert_eq!(
        f.answer(&q, "late", 1, None).outcome,
        "conversation_deleted"
    );
    assert!(f.store.pending_threads().unwrap().is_empty());
    assert_eq!(
        f.store
            .deliver("entity", "thread", "front_desk", |_| panic!(
                "deleted target restarted"
            ))
            .unwrap(),
        0
    );
}
#[test]
fn scope_and_voice_safe_validation_fail_before_persisting() {
    let f = Fixture::new();
    let q = f.ask(1).remove(0);
    let request = AnswerRequest {
        question_id: q.id.clone(),
        client_id: "x".into(),
        option_ids: vec![q.options[0].id.clone()],
        text: "".into(),
        expected_revision: None,
    };
    assert!(f
        .store
        .answer("other", "thread", request, "typed", "mac")
        .is_err());
    assert!(f
        .store
        .withdraw("entity", "thread", "other-asker", &q.id, "Reason")
        .is_err());
    let bad = QuestionInput {
        text: "When?".into(),
        options: vec![
            OptionInput {
                label: "I'll do it".into(),
                description: "Actor ambiguous".into(),
            },
            OptionInput {
                label: "Tomorrow".into(),
                description: "Wait".into(),
            },
        ],
        multiple: false,
        free_answer: true,
        recommended: None,
    };
    assert!(f.store.ask(&f.scope, vec![bad]).is_err());
    assert_eq!(f.store.all().unwrap().len(), 1);
    assert!(q.public_value().get("session_id").is_none());
}
#[test]
fn mixed_words_tool_result_is_durable_and_replayed_without_a_second_turn() {
    use richos_core::question_tools::{self, Scope};
    use serde_json::json;
    let f = Fixture::new();
    let qs = f.ask(2);
    f.answer(&qs[0], "tap", 0, None);
    let path = f.root.join("scope.json");
    std::fs::create_dir_all(&f.root).unwrap();
    question_tools::write_scope(
        &path,
        &Scope {
            context: f.scope.clone(),
            actions_allowed: true,
            answer_method: "phone_voice".into(),
            surface: "phone".into(),
        },
    )
    .unwrap();
    let args = json!({"question_id":qs[1].id,"client_id":"words","text":"Next Tuesday"});
    let answer = question_tools::call(&path, "answer", args.clone()).unwrap();
    let result = answer["complete_sets"][0].as_str().unwrap();
    assert!(result.contains("Ship today"));
    assert!(result.contains("Next Tuesday"));
    assert!(result.contains("phone_voice on phone"));
    assert_eq!(
        question_tools::call(&path, "answer", args).unwrap()["complete_sets"],
        answer["complete_sets"]
    );
    assert_eq!(
        Store::new(&f.root)
            .deliver("entity", "thread", "front_desk", |_| panic!("second turn"))
            .unwrap(),
        0
    );
}
#[test]
fn work_receiving_inbox_deduplicates_an_uncertain_handoff() {
    let f = Fixture::new();
    let q = f.ask(1).remove(0);
    f.answer(&q, "tap", 0, None);
    let deliver = |d: &Delivery| {
        richos_core::question_work::enqueue(&f.root, d).unwrap();
        Err("crash before sender receipt".into())
    };
    assert!(f
        .store
        .deliver("entity", "thread", "front_desk", deliver)
        .is_err());
    Store::new(&f.root)
        .deliver("entity", "thread", "front_desk", |d| {
            richos_core::question_work::enqueue(&f.root, d)?;
            Ok(d.id.clone())
        })
        .unwrap();
    assert_eq!(
        richos_core::question_work::pending(&f.root).unwrap().len(),
        1
    );
    // Reading what a run carries lets nothing go: a crash before the back end takes it
    // leaves it pending (the work-path design, D1).
    let carried =
        richos_core::question_work::peek(&f.root, "entity", "thread", "front_desk", "session-1").unwrap();
    assert_eq!(carried.new.len(), 1);
    assert_eq!(richos_core::question_work::pending(&f.root).unwrap().len(), 1);
    richos_core::question_work::taken(&f.root, &carried.ids(), "session-1").unwrap();
    assert!(richos_core::question_work::pending(&f.root).unwrap().is_empty());
    // Taken in this session: never carried to it again, and a retried handoff is not new.
    assert!(richos_core::question_work::peek(&f.root, "entity", "thread", "front_desk", "session-1")
        .unwrap()
        .is_empty());
    assert!(!richos_core::question_work::enqueue(&f.root, &carried.new[0]).unwrap());
    // A fresh session has never seen it, so it is told again, as an earlier answer (D5).
    let fresh =
        richos_core::question_work::peek(&f.root, "entity", "thread", "front_desk", "session-2").unwrap();
    assert!(fresh.new.is_empty());
    assert_eq!(fresh.earlier.len(), 1);
    // Let go with its ended assignment: never told to anyone again.
    richos_core::question_work::discard(&f.root, "entity", "thread", "front_desk").unwrap();
    assert!(richos_core::question_work::peek(&f.root, "entity", "thread", "front_desk", "session-3")
        .unwrap()
        .is_empty());
}
/// **"Rich has your answer" only once the taker has it** (the work-path design D7). A front-desk
/// set is taken by its own sink and reads delivered at once. A set whose sink keeps it in a
/// receiving inbox reads NOT delivered until a back end takes it, with no surface change: the
/// card already says "On its way to Rich" for that.
#[test]
fn the_card_says_delivered_only_once_an_inbox_answer_is_taken() {
    let mut f = Fixture::new();
    let front = f.ask(1).remove(0);
    f.answer(&front, "front", 0, None);
    f.store.deliver("entity", "thread", "front_desk", |_| Ok("turn-receipt".into())).unwrap();
    let card = |f: &Fixture, id: &str| f.store.list("entity", "thread").unwrap().into_iter().find(|q| q.id == id).unwrap();
    assert_eq!(card(&f, &front.id).public_value()["delivered"], true, "a front-desk set is unchanged");

    f.scope.asker = "work-1".into();
    let team = f.ask(1).remove(0);
    f.answer(&team, "team", 1, None);
    f.store
        .deliver("entity", "thread", "work-1", |d| {
            richos_core::question_work::enqueue(&f.root, d)?;
            Ok(format!("work-input:{}", d.id))
        })
        .unwrap();
    let saved = card(&f, &team.id);
    assert!(saved.delivered && saved.awaiting_taker, "the store's own delivery fact is unchanged");
    assert_eq!(saved.public_value()["delivered"], false, "the card said Rich has it before any back end did");
    let carried = richos_core::question_work::peek(&f.root, "entity", "thread", "work-1", "session-1").unwrap();
    richos_core::question_work::taken(&f.root, &carried.ids(), "session-1").unwrap();
    let taken = card(&f, &team.id);
    assert!(!taken.awaiting_taker);
    assert_eq!(taken.public_value()["delivered"], true);
}
/// **A job that ended before any back end took his answer says so** (Rich's ruling on
/// esc-20260927T220629Z-cbc90040). His Stop is the path taken here, in the order
/// `WorkHost::stop_assignment` takes it: close the job's questions, then let its inputs go. An
/// answer the back end had already taken is never marked, and neither card is ever "Rich has
/// your answer" and "stopped" at once.
#[test]
fn a_job_that_ends_before_the_taker_has_the_answer_marks_the_card() {
    let mut f = Fixture::new();
    let card = |f: &Fixture, id: &str| f.store.list("entity", "thread").unwrap().into_iter().find(|q| q.id == id).unwrap();
    let inbox = |f: &Fixture, asker: &str| {
        f.store
            .deliver("entity", "thread", asker, |d| {
                richos_core::question_work::enqueue(&f.root, d)?;
                Ok(format!("work-input:{}", d.id))
            })
            .unwrap();
    };
    let end = |f: &Fixture, asker: &str| {
        f.store.close("entity", "thread", Some(asker), "The assignment was stopped", false).unwrap();
        richos_core::question_work::discard(&f.root, "entity", "thread", asker).unwrap();
    };

    // Independent Stu owns two jobs; a Stop must select ownership, not the recipient.
    f.scope.asker="stu".into();
    f.scope.obligation_id=Some("independent-A".into());
    let open_a=f.ask(1).remove(0);
    let queued_a=f.ask(1).remove(0);
    f.answer(&queued_a,"independent-answer-A",0,None);
    f.scope.obligation_id=Some("independent-B".into());
    let open_b=f.ask(1).remove(0);
    let queued_b=f.ask(1).remove(0);
    f.answer(&queued_b,"independent-answer-B",1,None);
    inbox(&f,"stu");
    end(&f,"independent-A");
    assert_eq!(card(&f,&open_a.id).state,State::Withdrawn);
    assert_eq!(card(&f,&open_b.id).state,State::Open);
    let pending=richos_core::question_work::pending(&f.root).unwrap();
    assert!(!pending.iter().any(|d|d.set_id.as_deref()==Some(&queued_a.set_id)));
    assert!(pending.iter().any(|d|d.set_id.as_deref()==Some(&queued_b.set_id)));
    assert!(card(&f,&queued_a.id).ended_before_taken);
    assert!(!card(&f,&queued_b.id).ended_before_taken);
    f.scope.obligation_id=None;

    // Stopped while his answer sat in the inbox, untaken.
    f.scope.asker = "work-1".into();
    let stopped = f.ask(1).remove(0);
    f.answer(&stopped, "stopped", 0, None);
    inbox(&f, "work-1");
    assert_eq!(card(&f, &stopped.id).public_value()["ended_before_taken"], false, "marked before the job ended");
    end(&f, "work-1");
    let ended = card(&f, &stopped.id);
    assert!(ended.ended_before_taken && ended.awaiting_taker);
    assert_eq!(ended.state, State::Answered, "his answer is kept, not withdrawn");
    assert_eq!(ended.public_value()["ended_before_taken"], true, "the card still says it is on its way");
    assert_eq!(ended.public_value()["delivered"], false);
    // A second ending (the cleanup after the Stop) changes nothing.
    end(&f, "work-1");
    assert_eq!(card(&f, &stopped.id).public_value()["ended_before_taken"], true);

    // Taken by a back end, then the job ended: Rich has it, and the card says only that.
    f.scope.asker = "work-2".into();
    let taken = f.ask(1).remove(0);
    f.answer(&taken, "taken", 1, None);
    inbox(&f, "work-2");
    let carried = richos_core::question_work::peek(&f.root, "entity", "thread", "work-2", "session-1").unwrap();
    richos_core::question_work::taken(&f.root, &carried.ids(), "session-1").unwrap();
    end(&f, "work-2");
    let finished = card(&f, &taken.id);
    assert!(!finished.ended_before_taken, "a taken answer was marked as never reaching Rich");
    assert_eq!(finished.public_value()["ended_before_taken"], false);
    assert_eq!(finished.public_value()["delivered"], true);
}
#[test]
fn notifications_are_once_per_set_even_after_answer_changes() {
    let f = Fixture::new();
    let qs = f.ask(4);
    let mut sent = vec![];
    f.store
        .notify_sets(|q| {
            sent.push(q.id.clone());
            Ok(true)
        })
        .unwrap();
    assert_eq!(sent.len(), 1);
    f.answer(&qs[0], "a", 0, None);
    Store::new(&f.root)
        .notify_sets(|_| panic!("answer triggered another notification"))
        .unwrap();
}
/// The shipped engine (`richos/engine`), whose hooks the app runs.
fn real_engine() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../engine")
        .canonicalize()
        .unwrap()
}

/// A seat that declares his TODOs (item 1.1 is the release's shipping day, prepared) and a
/// rulings record (the product's typefaces, approved), as the engine reads them.
fn engine_seat(root: &std::path::Path) -> PathBuf {
    let seat = root.join("seat");
    std::fs::create_dir_all(seat.join("wiki")).unwrap();
    std::fs::write(seat.join(".ceo-todos"),"TODO_RECORD=\"wiki/open-items.md\"\nTODO_VIEW=\"CEO-TODOs.md\"\nROOT_README=\"README.md\"\nCEO_SECTIONS=\"1 2\"\nPREPARER_SECTION=\"3\"\nARTIFACT_ROOTS=\"q=.\"\n").unwrap();
    std::fs::write(seat.join("orchestration.config"), "CEO_RULINGS_PATHS=\"wiki/ceo-decisions.md\"\n").unwrap();
    std::fs::write(seat.join("wiki/ceo-decisions.md"),"# Decisions\n\n## 1. Typeface (CEO, 2026-09-01)\n\n**His words:** Newsreader and Inter are approved for the product.\n").unwrap();
    std::fs::write(seat.join("README.md"), "# Test seat\n").unwrap();
    std::fs::write(seat.join("CEO-TODOs.md"), "# View\n").unwrap();
    std::fs::write(seat.join("wiki/open-items.md"),"# Open items\n\n## 1. Waiting on the CEO\n\n### 1.1 READY-FOR-CEO — Release shipping schedule\n\n- **Open:** `q/wiki/release.md`\n- **Time:** 5 minutes\n- **Done:** shipping day chosen\n- **Unblocks:** release shipping\n\n## 2. Waiting on the CEO\n\n## 3. Buildable\n").unwrap();
    seat
}

/// Whether `session` (or, with `run`, the lead's `RICHOS_OPERATOR_LEAD` app run) has put a
/// prepared item to him: the engine's own verdict, `ceo-asks-status.sh` (0 SATISFIED, 1 OPEN).
fn asked(seat: &std::path::Path, session: &str, run: Option<&str>) -> i32 {
    let mut command = std::process::Command::new("bash");
    command
        .arg(real_engine().join("scripts/ceo-asks-status.sh"))
        .arg(seat)
        .arg("--session")
        .arg(session)
        .current_dir(seat)
        .env("RICHOS_ENTITY_ROOT", seat)
        .env_remove("RICHOS_OPERATOR_LEAD")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(run) = run {
        command.env("RICHOS_OPERATOR_LEAD", run);
    }
    command.status().unwrap().code().unwrap_or(-1)
}

/// **F10 (operator contract notes §4 item 2):** in the app each conversation's lead is its own
/// session, and the ask was keyed per session, so every new conversation's lead was told to
/// put the same top item to him again. An ask one lead put to him, witnessed when he was shown
/// it, now counts for every lead of the same app run, and for no session outside that run.
#[test]
fn an_ask_witnessed_in_one_conversation_counts_for_every_lead_of_the_run() {
    let mut f = Fixture::new();
    let seat = engine_seat(&f.root);
    f.scope.engine = Some(real_engine());
    f.scope.entity_root = Some(seat.clone());
    f.scope.asker = "operator:conversation".into();
    f.scope.session_id = "lead-of-conversation-a".into();
    f.scope.app_run = Some("app-run-1".into());
    assert_eq!(asked(&seat, "lead-of-conversation-b", Some("app-run-1")), 1,
               "before he is asked, the run has not asked him");
    let input:QuestionInput=serde_json::from_value(serde_json::json!({"text":"When should the release ship?\npremise-unverified: This test fixture has no production deadline.","options":[{"label":"Today","description":"Earlier fixes"},{"label":"Tomorrow","description":"More tests"}]})).unwrap();
    let q = f.store.ask(&f.scope, vec![input]).unwrap().remove(0);
    f.store.acknowledge("entity", "thread", &q.id, "mac").unwrap();
    f.store.flush_witnesses().unwrap();
    let ledger = std::fs::read_to_string(seat.join(".claude/state/ceo-asks.jsonl")).unwrap();
    let record: serde_json::Value = serde_json::from_str(ledger.trim()).unwrap();
    assert_eq!(record["app_run"], "app-run-1", "the display witness writes the run: {record}");
    assert_eq!(record["discharges"], true, "the question is about item 1.1: {record}");
    assert_eq!(asked(&seat, "lead-of-conversation-b", Some("app-run-1")), 0,
               "another conversation's lead in the same run counts as having asked");
    assert_eq!(asked(&seat, "lead-of-conversation-b", Some("app-run-2")), 1,
               "a later launch of the app is a new run and was not asked");
    assert_eq!(asked(&seat, "terminal-session", None), 1,
               "a session outside the app is still keyed to its own session");
}

#[test]
fn actual_engine_witness_requires_display_and_preserves_original_session() {
    let mut f = Fixture::new();
    let seat = f.root.join("seat");
    std::fs::create_dir_all(seat.join("wiki")).unwrap();
    std::fs::write(seat.join(".ceo-todos"),"TODO_RECORD=\"wiki/open-items.md\"\nTODO_VIEW=\"CEO-TODOs.md\"\nROOT_README=\"README.md\"\nCEO_SECTIONS=\"1 2\"\nPREPARER_SECTION=\"3\"\nARTIFACT_ROOTS=\"q=.\"\n").unwrap();
    std::fs::write(
        seat.join("orchestration.config"),
        "CEO_RULINGS_PATHS=\"wiki/ceo-decisions.md\"\n",
    )
    .unwrap();
    std::fs::write(seat.join("wiki/ceo-decisions.md"),"# Decisions\n\n## 1. Typeface (CEO, 2026-09-01)\n\n**His words:** Newsreader and Inter are approved for the product.\n").unwrap();
    std::fs::write(seat.join("README.md"), "# Test seat\n").unwrap();
    std::fs::write(seat.join("CEO-TODOs.md"), "# View\n").unwrap();
    std::fs::write(seat.join("wiki/open-items.md"),"# Open items\n\n## 1. Waiting on the CEO\n\n### 1.1 READY-FOR-CEO — Release shipping schedule\n\n- **Open:** `q/wiki/release.md`\n- **Time:** 5 minutes\n- **Done:** shipping day chosen\n- **Unblocks:** release shipping\n\n## 2. Waiting on the CEO\n\n## 3. Buildable\n").unwrap();
    let engine = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../engine")
        .canonicalize()
        .unwrap();
    f.scope.engine = Some(engine);
    f.scope.entity_root = Some(seat.clone());
    let input:QuestionInput=serde_json::from_value(serde_json::json!({"text":"When should the release ship?\npremise-unverified: This test fixture has no production deadline.","options":[{"label":"Today","description":"Earlier fixes"},{"label":"Tomorrow","description":"More tests"}]})).unwrap();
    let q = f.store.ask(&f.scope, vec![input]).unwrap().remove(0);
    let ledger = seat.join(".claude/state/ceo-asks.jsonl");
    assert!(!ledger.exists(), "persistence is not display");
    f.store
        .acknowledge("entity", "thread", &q.id, "phone")
        .unwrap();
    f.store.flush_witnesses().unwrap();
    let text = std::fs::read_to_string(&ledger).unwrap();
    let record: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(record["session_id"], "original-session");
    assert_eq!(record["app_question_id"], q.id);
    f.store
        .acknowledge("entity", "thread", &q.id, "mac")
        .unwrap();
    assert_eq!(std::fs::read_to_string(&ledger).unwrap().lines().count(), 1);
    // Simulate a crash after the engine append but before the app saved its receipt.
    let path = f.root.join("questions/store.json");
    let mut data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    data["questions"][0]["shown"]["witness_written"] = serde_json::json!(false);
    std::fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
    Store::new(&f.root).flush_witnesses().unwrap();
    assert_eq!(std::fs::read_to_string(&ledger).unwrap().lines().count(), 1);
}
#[test]
fn witness_failure_keeps_display_ack_for_recovery() {
    let mut f = Fixture::new();
    std::fs::create_dir_all(f.root.join("engine/scripts/hooks")).unwrap();
    std::fs::write(
        f.root.join("engine/scripts/hooks/guard-ceo-ruled-ask.sh"),
        "exit 0\n",
    )
    .unwrap();
    std::fs::write(
        f.root.join("engine/scripts/hooks/notice-ceo-asks.sh"),
        "exit 0\n",
    )
    .unwrap();
    f.scope.engine = Some(f.root.join("engine"));
    f.scope.entity_root = Some(f.root.clone());
    let q = f.ask(1).remove(0);
    f.store
        .acknowledge("entity", "thread", &q.id, "mac")
        .unwrap();
    assert!(
        f.store.flush_witnesses().is_err(),
        "exit zero is not a witness"
    );
    assert!(
        !Store::new(&f.root).all().unwrap()[0]
            .shown
            .as_ref()
            .unwrap()
            .witness_written
    );
}

#[test]
fn idle_delivery_and_duplicate_receipts_do_not_rewrite_history() {
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new();
    let q = f.ask(1).remove(0);
    f.store
        .acknowledge("entity", "thread", &q.id, "mac")
        .unwrap();
    let path = f.root.join("questions/store.json");
    let inode = std::fs::metadata(&path).unwrap().ino();
    f.store
        .acknowledge("entity", "thread", &q.id, "phone")
        .unwrap();
    f.store
        .deliver("entity", "thread", "front_desk", |_| {
            panic!("open question delivered")
        })
        .unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().ino(),
        inode,
        "no-op triggered a file watcher event"
    );
    f.answer(&q, "a", 0, None);
    assert!(f
        .store
        .deliver("entity", "thread", "front_desk", |_| Err("offline".into()))
        .is_err());
    let inode = std::fs::metadata(&path).unwrap().ino();
    assert!(f
        .store
        .deliver("entity", "thread", "front_desk", |_| Err("offline".into()))
        .is_err());
    assert_eq!(
        std::fs::metadata(&path).unwrap().ino(),
        inode,
        "failed retry triggered another retry"
    );
}
#[test]
fn waiting_at_reply_boundary_is_distinct_from_delivered() {
    let f = Fixture::new();
    let q = f.ask(1).remove(0);
    f.answer(&q, "a", 0, None);
    let queued = f
        .store
        .waiting_for_turn("entity", "thread", &q.id, true)
        .unwrap()
        .unwrap();
    assert!(queued.waiting_for_turn);
    assert!(!queued.delivered);
    assert!(!queued.handoff_started);
    f.store
        .deliver("entity", "thread", "front_desk", |_| Ok("received".into()))
        .unwrap();
    let delivered = f
        .store
        .waiting_for_turn("entity", "thread", &q.id, true)
        .unwrap()
        .unwrap();
    assert!(!delivered.waiting_for_turn);
    assert!(delivered.delivered);
}

#[test]
fn a_guard_that_never_reads_large_input_cannot_hold_the_vendor_fallback() {
    let mut f = Fixture::new();
    std::fs::create_dir_all(f.root.join("engine/scripts/hooks")).unwrap();
    // The guard never reads its input and sleeps for a hang guard's length; the 100 ms budget
    // kills its process group long before. A budget that only started once the input was
    // written (the defect this pins) would block on the full pipe until the guard exited,
    // and the guard exits 0, so the question would be recorded. The budget's own refusal,
    // word for word, is therefore the verdict. The `< 1 s` clock that stood here, against a
    // `sleep 3` guard, decided it by the speed of the Mac instead (audit
    // `docs/verification/2026-09-29-load-sensitive-checks-audit.md` R9).
    std::fs::write(
        f.root.join("engine/scripts/hooks/guard-ceo-ruled-ask.sh"),
        "sleep 60\n",
    )
    .unwrap();
    f.scope.engine = Some(f.root.join("engine"));
    f.scope.entity_root = Some(f.root.clone());
    let input:QuestionInput=serde_json::from_value(serde_json::json!({"text":"x".repeat(4096),"options":[{"label":"Today","description":"x".repeat(2048)},{"label":"Tomorrow","description":"y".repeat(2048)},{"label":"Tuesday","description":"z".repeat(2048)},{"label":"Wednesday","description":"z".repeat(2048)}]})).unwrap();
    let refused = f
        .store
        .ask_bounded(
            &f.scope,
            vec![input; 4],
            std::time::Duration::from_millis(100)
        )
        .expect_err("a guard that never reads its input held the ask until it exited");
    assert!(refused.to_string().contains("did not finish in time"), "{refused}");
    assert!(f.store.all().unwrap().is_empty());
}

#[test]
fn residency_snapshot_keeps_open_and_pending_answers_without_waiting_on_store_lock() {
    let f = Fixture::new();
    assert!(!f.store.pending_for_residency().unwrap());
    let q = f.ask(1).remove(0);
    let lock = std::fs::OpenOptions::new().read(true).write(true)
        .open(f.root.join("questions/lock")).unwrap();
    lock.lock().unwrap();
    assert!(f.store.pending_for_residency().unwrap());
    lock.unlock().unwrap();
    f.answer(&q, "phone-offline-answer", 0, None);
    assert!(f.store.pending_for_residency().unwrap());
    f.store.deliver("entity", "thread", "front_desk", |_| Ok("receipt".into())).unwrap();
    assert!(!f.store.pending_for_residency().unwrap());
    let withdrawn = f.ask(1).remove(0);
    f.store.withdraw("entity", "thread", "front_desk", &withdrawn.id, "No longer needed").unwrap();
    assert!(!f.store.pending_for_residency().unwrap());
    std::fs::write(f.root.join("questions/store.json"), b"broken").unwrap();
    assert!(f.store.pending_for_residency().is_err());
}

#[test]
fn matching_initial_revision_is_an_answer_not_conflicting_words() {
    let f = Fixture::new();
    let q = f.ask(1).remove(0);
    let result = f.answer(&q, "typed-initial-revision", 0, Some(0));
    assert_eq!(result.outcome, "accepted");
    let mut deliveries = Vec::new();
    f.store.deliver("entity", "thread", "front_desk", |d| {
        deliveries.push(d.clone()); Ok("receipt".into())
    }).unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].set_id.as_deref(), Some(q.set_id.as_str()));
    assert!(!deliveries[0].text.contains("You also answered"));
}

#[test]
fn read_only_question_snapshots_do_not_wait_for_the_writer() {
    let f = Fixture::new();
    let asked = f.ask(1);
    let path = f.root.join("questions/store.json");
    let before = std::fs::read(&path).unwrap();
    let writer = std::fs::OpenOptions::new().read(true).write(true)
        .open(f.root.join("questions/lock")).unwrap();
    writer.lock().unwrap();
    let probe = std::fs::OpenOptions::new().read(true).write(true)
        .open(f.root.join("questions/lock")).unwrap();
    assert!(matches!(probe.try_lock(), Err(std::fs::TryLockError::WouldBlock)));
    let store = f.store.clone();
    let (send, receive) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        send.send((store.list("entity", "thread"), store.all(), store.pending_threads())).unwrap();
    });
    // A read result while the kernel writer lock remains held is the verdict.
    // This finite guard only catches a deadlock; it is not a speed threshold.
    let observed = receive.recv_timeout(std::time::Duration::from_secs(10));
    let still_held = matches!(probe.try_lock(), Err(std::fs::TryLockError::WouldBlock));
    drop(writer); // Always release before joining, including the old deadlocking implementation.
    reader.join().unwrap();
    assert!(still_held);
    let (listed, all, pending) = observed.expect("read-only question inspection waited for its writer");
    assert_eq!(listed.unwrap()[0].id, asked[0].id);
    assert_eq!(all.unwrap()[0].id, asked[0].id);
    assert!(pending.unwrap().is_empty());
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn app_questions_identify_the_nonblocking_tool_to_the_engine_guard() {
    let mut f = Fixture::new();
    let hooks = f.root.join("engine/scripts/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("guard-ceo-ruled-ask.sh"), r#"
python3 -c 'import json,sys
payload=json.load(sys.stdin)
assert payload["tool_name"] == "mcp__richos_questions__ask", payload
assert payload["tool_input"]["questions"][0]["question"] == "When should the release ship?"
'
"#).unwrap();
    f.scope.engine = Some(f.root.join("engine"));
    f.scope.entity_root = Some(f.root.clone());
    f.scope.asker = "stu".into();
    let asked = f.ask(1);
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].state, State::Open);
    assert_eq!(asked[0].asker, "stu");
}
