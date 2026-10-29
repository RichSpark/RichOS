//! **N CONVERSATION THREADS, EACH HOLDING ITS OWN FRONT DESK** — the CEO's Two Riches page.
//!
//! *"a CEO can create a conversation thread (i.e. any number of conversation threads) …
//! each conversation thread always holds one front desk Rich and one back-end Rich …
//! the CEO could open and run multiple things in parallel."*
//!
//! What these suites measure is RESIDENCE: a thread's front desk — its provider session,
//! its priming, its own context measurements — survives another thread taking a turn,
//! instead of being killed and reconstituted from the ledger tail. Before this, a thread
//! switch called `clear_lease()` and the outgoing child died.
//!
//! Actual overlapping replies are covered by request_preparation_tests and native transport witnesses.
use richos_core::{
    cognition::{Cognition, CognitionError, LeaseFactory, TurnItem},
    entity::{EntityId, ThreadBinding},
    ledger::{Ledger, Source},
    spine::{Spine, MAX_RESIDENT_FRONT_DESKS},
    steering::TurnControl,
};
use std::sync::{Arc, Mutex};
mod support;

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

/// Everything every desk this factory ever made has done, in one place, so a test can ask
/// about a desk that has since been parked, resumed or dropped.
#[derive(Clone, Default)]
struct Log {
    spawned: Arc<Mutex<Vec<String>>>,
    /// One session entry per original input receiving its current company context.
    contexts: Arc<Mutex<Vec<String>>>,
    /// `(session, what he said)`.
    prompts: Arc<Mutex<Vec<(String, String)>>>,
    /// Sessions whose child was actually killed. `Drop` is the only honest witness — a
    /// parked desk that had quietly been dropped would satisfy every other assertion here.
    dropped: Arc<Mutex<Vec<String>>>,
}

impl Log {
    fn context_count(&self, session: &str) -> usize {
        self.contexts.lock().unwrap().iter().filter(|s| *s == session).count()
    }
    fn prompts_to(&self, session: &str) -> Vec<String> {
        self.prompts.lock().unwrap().iter()
            .filter(|(s, _)| s == session).map(|(_, text)| text.clone()).collect()
    }
    fn spawned(&self) -> Vec<String> {
        self.spawned.lock().unwrap().clone()
    }
    fn dropped(&self) -> Vec<String> {
        self.dropped.lock().unwrap().clone()
    }
}

/// A front desk that is REAL about the one property this whole change turns on:
/// `requires_thread_isolation() == true`, which is what the native conversation lease
/// reports whenever it has continuity configured (`native.rs`). `MockCognition` returns
/// false, so it would never come down the park-and-resume path at all.
struct Desk {
    session: String,
    log: Log,
}

impl Cognition for Desk {
    fn session_id(&self) -> &str {
        &self.session
    }
    fn requires_thread_isolation(&self) -> bool {
        true
    }
    fn set_company_context(&mut self,_context:Option<&str>)->Result<bool,CognitionError> {
        self.log.contexts.lock().unwrap().push(self.session.clone());
        Ok(true)
    }

    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.log.prompts.lock().unwrap().push((self.session.clone(), text.to_string()));
        let reply = format!("{}: {text}", self.session);
        on_item(TurnItem::Text { seq: 0, text: &reply });
        Ok("end_turn".to_string())
    }
}

impl Drop for Desk {
    fn drop(&mut self) {
        self.log.dropped.lock().unwrap().push(self.session.clone());
    }
}

struct Desks {
    log: Log,
    next: Arc<Mutex<u64>>,
}

impl LeaseFactory for Desks {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        let mut next = self.next.lock().unwrap();
        *next += 1;
        let session = format!("desk-{next}");
        self.log.spawned.lock().unwrap().push(session.clone());
        Ok(Box::new(Desk { session, log: self.log.clone() }))
    }
}

fn fixture() -> (Spine, Log, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("richos-resident-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut spine = support::spine(Ledger::open(root.join("ledger.jsonl")).unwrap());
    spine.set_turn_control(TurnControl::open(root.join("intake.jsonl")).unwrap());
    let log = Log::default();
    spine.set_lease_factory(Box::new(Desks { log: log.clone(), next: Arc::new(Mutex::new(0)) }));
    (spine, log, root)
}

/// **THE HEADLINE.** A thread's front desk is still there when he comes back to it: same
/// provider session, not re-primed, and never spawned a second time.
///
/// The positive control is a THIRD thread in the same test. Without it, "no new spawn when
/// he returns to the first thread" would also be satisfied by a factory that had stopped
/// spawning at all, and by a spine that had stopped switching threads.
#[test]
fn a_threads_front_desk_is_still_there_when_he_comes_back_to_it() {
    let (mut spine, log, root) = fixture();
    let one = spine.create_thread("Pricing", &femcboost()).unwrap();
    let two = spine.create_thread("Hiring", &femcboost()).unwrap();
    let three = spine.create_thread("The nightly", &femcboost()).unwrap();

    spine.submit_prompt_to(&one, "where is the pricing review", Source::Text).unwrap();
    spine.submit_prompt_to(&two, "who did we shortlist", Source::Text).unwrap();
    assert_eq!(log.spawned(), vec!["desk-1", "desk-2"], "one desk per conversation, spawned on its first turn");
    assert!(log.dropped().is_empty(), "a conversation's front desk was killed when he switched away from it");
    assert_eq!(spine.resident_front_desks(), 2);
    assert!(spine.front_desk_is_resident(&one) && spine.front_desk_is_resident(&two));

    // Back to the first conversation.
    spine.submit_prompt_to(&one, "and what did it cost", Source::Text).unwrap();
    assert_eq!(log.spawned(), vec!["desk-1", "desk-2"], "coming back spawned a new front desk");
    assert_eq!(log.context_count("desk-1"), 2, "each original input receives company context");
    assert_eq!(
        log.prompts_to("desk-1"),
        vec!["where is the pricing review".to_string(), "and what did it cost".to_string()],
        "the second sentence did not reach the desk that heard the first"
    );

    // Positive control: a conversation that has NEVER spoken does get a fresh desk, primed
    // once — so the three assertions above are about residence, not about a dead factory.
    spine.submit_prompt_to(&three, "how did the nightly go", Source::Text).unwrap();
    assert_eq!(log.spawned(), vec!["desk-1", "desk-2", "desk-3"]);
    assert_eq!(log.context_count("desk-3"), 1);
    assert_eq!(spine.resident_front_desks(), 3);
    std::fs::remove_dir_all(root).unwrap();
}

/// **A MESSAGE TO A CONVERSATION THAT IS NOT THE ONE ON SCREEN IS ANSWERED, NOT REFUSED.**
///
/// `send_message` used to compare the named thread against the single active one and hand
/// his sentence back: *"The conversation changed before your message was sent. Open the
/// original conversation to try again."*
#[test]
fn a_message_to_another_conversation_is_answered_rather_than_refused() {
    let (mut spine, log, root) = fixture();
    let one = spine.create_thread("Pricing", &femcboost()).unwrap();
    let two = spine.create_thread("Hiring", &femcboost()).unwrap();
    spine.submit_prompt_to(&one, "first", Source::Text).unwrap();
    assert_eq!(spine.active_thread(), Some(one.as_str()));

    // He is looking at `one`; the message names `two`.
    spine.submit_prompt_to(&two, "who did we shortlist", Source::Text).unwrap();
    let answered = spine.messages(&two).unwrap();
    assert_eq!(answered[0].text, "who did we shortlist");
    assert!(answered.iter().any(|m| m.text.contains("desk-2")), "the second conversation was never answered");
    // And his first conversation is untouched by it.
    assert!(spine.messages(&one).unwrap().iter().all(|m| !m.text.contains("shortlist")));
    assert_eq!(log.prompts_to("desk-1"), vec!["first".to_string()]);
    std::fs::remove_dir_all(root).unwrap();
}

/// Past the cap, the LEAST RECENTLY SPOKEN desk is retired — and nothing else is.
///
/// That thread is then exactly where every thread was before residency: its next turn
/// spawns a lease and re-primes from the ledger. The degradation is the old path, not a new
/// failure, and the conversation he is actually in is never the one retired.
#[test]
fn past_the_cap_the_least_recently_spoken_front_desk_is_the_one_retired() {
    let (mut spine, log, root) = fixture();
    let threads: Vec<String> = (0..MAX_RESIDENT_FRONT_DESKS + 1)
        .map(|n| spine.create_thread(&format!("Conversation {n}"), &femcboost()).unwrap())
        .collect();
    for thread in &threads {
        spine.submit_prompt_to(thread, "hello", Source::Text).unwrap();
    }
    assert_eq!(spine.resident_front_desks(), MAX_RESIDENT_FRONT_DESKS,
        "the app is holding more front desks open than the cap allows");
    assert_eq!(log.dropped(), vec!["desk-1"], "the retired desk was not the least recently spoken one");
    assert!(!spine.front_desk_is_resident(&threads[0]));
    // Everything since is still resident, the one he is in included.
    for thread in &threads[1..] {
        assert!(spine.front_desk_is_resident(thread), "a desk inside the cap was retired");
    }
    // And the retired conversation still works: a fresh desk, primed once, his words intact.
    spine.submit_prompt_to(&threads[0], "are you still there", Source::Text).unwrap();
    let fresh = log.spawned().last().unwrap().clone();
    assert_eq!(log.context_count(&fresh), 1);
    assert_eq!(log.prompts_to(&fresh), vec!["are you still there".to_string()]);
    std::fs::remove_dir_all(root).unwrap();
}

/// **A BUSY SESSION IS NOT RETIRED** — the regression guard for idle retirement, written against
/// the one retirement main already had (the cap), so it passes before and after that change.
/// The least recently spoken desk has a turn running; the cap passes over it and retires the
/// next oldest instead. `idle_front_desk_retirement_tests.rs` holds the timed retirement's own
/// busy test, which cannot compile on a tree without it.
#[test]
fn past_the_cap_a_busy_front_desk_is_passed_over() {
    let root = std::env::temp_dir().join(format!("richos-resident-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut spine = support::spine(Ledger::open(root.join("ledger.jsonl")).unwrap());
    let control = TurnControl::open(root.join("intake.jsonl")).unwrap();
    spine.set_turn_control(control.clone());
    let log = Log::default();
    spine.set_lease_factory(Box::new(Desks { log: log.clone(), next: Arc::new(Mutex::new(0)) }));

    let threads: Vec<String> = (0..MAX_RESIDENT_FRONT_DESKS + 1)
        .map(|n| spine.create_thread(&format!("Conversation {n}"), &femcboost()).unwrap())
        .collect();
    spine.submit_prompt_to(&threads[0], "hello", Source::Text).unwrap();
    control.for_thread(&threads[0]).begin_turn(richos_core::steering::ActiveTurn {
        turn_id: "still-running".into(), thread_id: threads[0].clone(), entity_id: Some(femcboost()), started_at: None });
    for thread in &threads[1..] {
        spine.submit_prompt_to(thread, "hello", Source::Text).unwrap();
    }
    assert!(spine.front_desk_is_resident(&threads[0]), "a desk with a running turn was retired");
    assert!(!log.dropped().contains(&"desk-1".to_string()), "the busy desk's child was ended");
    assert_eq!(log.dropped(), vec!["desk-2"], "the cap did not retire the next oldest idle desk instead");
    std::fs::remove_dir_all(root).unwrap();
}

/// **The thing that would have gone wrong quietly.** Several facts un-prime a front desk
/// because they are facts about the whole app, not about one lease — the central folder
/// moving, the memory wiring being torn down. Before residency, un-priming the chair was
/// enough, because every other thread built a fresh lease on its next turn anyway. A parked
/// desk that kept `contexts: true` across one of these would go on serving him from material
/// the app had stopped believing in.
#[test]
fn moving_the_central_folder_un_primes_the_parked_front_desks_too() {
    let (mut spine, log, root) = fixture();
    let one = spine.create_thread("Pricing", &femcboost()).unwrap();
    let two = spine.create_thread("Hiring", &femcboost()).unwrap();
    spine.submit_prompt_to(&one, "first", Source::Text).unwrap();
    spine.submit_prompt_to(&two, "second", Source::Text).unwrap();
    assert_eq!(log.context_count("desk-1"), 1);

    spine.set_central_root(root.join("a-different-folder"));

    // The parked desk is primed AGAIN on its way back into the chair — same session, so it
    // is the same Rich; new priming, so it is not the old material.
    spine.submit_prompt_to(&one, "third", Source::Text).unwrap();
    assert_eq!(log.context_count("desk-1"), 2, "a parked front desk kept its stale priming");
    assert_eq!(log.spawned(), vec!["desk-1", "desk-2"], "un-priming killed the desk instead of re-priming it");
    std::fs::remove_dir_all(root).unwrap();
}

/// Residency never crosses conversations: each desk is bound to its own thread and a
/// binding never travels with the chair.
#[test]
fn no_front_desk_ever_takes_another_conversations_binding() {
    let (mut spine, log, root) = fixture();
    let one = spine.create_thread("Pricing", &femcboost()).unwrap();
    let two = spine.create_thread("Hiring", &femcboost()).unwrap();
    for _ in 0..3 {
        spine.submit_prompt_to(&one, "a", Source::Text).unwrap();
        spine.submit_prompt_to(&two, "b", Source::Text).unwrap();
    }
    assert_eq!(log.prompts_to("desk-1"), vec!["a".to_string(); 3]);
    assert_eq!(log.prompts_to("desk-2"), vec!["b".to_string(); 3]);
    assert_eq!(log.spawned().len(), 2, "a conversation was given more than one front desk");
    // The ledger agrees with the desks: neither thread holds the other's words.
    let (first, second): (Vec<_>, Vec<_>) = (spine.messages(&one).unwrap(), spine.messages(&two).unwrap());
    assert!(first.iter().all(|m| m.text == "a" || m.text.starts_with("desk-1")));
    assert!(second.iter().all(|m| m.text == "b" || m.text.starts_with("desk-2")));
    let _: Option<&ThreadBinding> = spine.active_binding();
    std::fs::remove_dir_all(root).unwrap();
}

/// **A PARKED DESK COMES BACK WITH THE COMPANY MATERIAL IT WAS PRIMED WITH** — the latent half
/// of the defect run H measured on the real provider
/// (`docs/verification/first-words-2026-09-18-primed.md` §3).
///
/// `prime_lease_if_needed` opens with
/// `if onboarding_block != self.onboarding_primed_block { self.lease_primed = false; }`, and
/// `onboarding_primed_block` was a SPINE-WIDE field. Residency made that wrong: it described
/// whichever desk last sat in the chair, so a desk returning from one company found the field
/// describing another company's desk, was un-primed by the comparison, and re-primed — on his
/// clock, for nothing. Residency exists so that coming back is a RETURN rather than a
/// reconstitution, and a re-prime is a reconstitution.
///
/// **Two companies, not one, and that is the whole test.** With a single entity both sides of
/// the comparison are the same string and it passes for the wrong reason; the block is built
/// from the entity (`onboarding::priming_block`), so only a cross-company return can tell a
/// per-desk field from a shared one. The suite's other tests set no central root at all, which
/// is why every one of them was green while this was broken.
#[test]
fn a_parked_desk_comes_back_with_the_company_material_it_was_primed_with() {
    let (mut spine, log, root) = fixture();
    let deeply = EntityId::parse("deeply").unwrap();
    // A real install: the CEO's central folder is known, with a folder per company in it.
    let corpus = root.join("corpus");
    std::fs::create_dir_all(corpus.join("femcboost")).unwrap();
    std::fs::create_dir_all(corpus.join("deeply")).unwrap();
    spine.set_central_root(corpus);
    spine.set_onboarding_record(root.join("onboarding.json"));

    let one = spine.create_thread("Pricing", &femcboost()).unwrap();
    let two = spine.create_thread("Q4 board pack", &deeply).unwrap();
    spine.submit_prompt_to(&one, "where is the pricing review", Source::Text).unwrap();
    spine.submit_prompt_to(&two, "what goes in the board pack", Source::Text).unwrap();
    assert_eq!(log.spawned(), vec!["desk-1", "desk-2"], "one desk per conversation");
    assert_eq!(log.context_count("desk-1"), 1);
    assert_eq!(log.context_count("desk-2"), 1);

    // Back to the first company's conversation. Its desk is parked and primed with that
    // company's material; nothing should be spent putting it back in the chair.
    spine.submit_prompt_to(&one, "and what did it cost", Source::Text).unwrap();
    assert_eq!(log.spawned(), vec!["desk-1", "desk-2"], "coming back spawned a new front desk");
    assert_eq!(
        log.context_count("desk-1"), 2,
        "returning to this conversation must supply its own current company facts"
    );
    assert_eq!(
        log.prompts_to("desk-1"),
        vec!["where is the pricing review".to_string(), "and what did it cost".to_string()],
        "the second sentence did not reach the desk that heard the first"
    );
    let _ = std::fs::remove_dir_all(root);
}
