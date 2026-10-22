//! Durable conversation input and presentation around independent native sessions.
//!
//! The ledger serializes input receipts and UI events. Each conversation owns its
//! provider connection outside the shared writer lock. Native sessions own context
//! and execution; this layer does not replace sessions or reconstruct their memory.

use crate::cognition::{Cognition, CognitionError, LeaseFactory, TurnItem};
use crate::entity::{EntityId, EntityRegistry, ThreadBinding};
use crate::journal::MachineryJournal;
use crate::ledger::{AttentionTier, ActionStatus, ActionVisibility, Ledger, LedgerError, Message, Source};
use crate::live::{
    proactive_message_events, EventFence, LiveEvent, LiveObserver, LiveTurn, ThreadStatus, TurnStatus,
};
use crate::machinery::{ContextUsage, MachineryObserver, MachineryRecord};
use crate::reprime::{
    EvidenceLookup, LoroContextCompiler,
};
use crate::correction::{ProposalObserver, SharedCorrectionDesk};
use crate::loro::SharedSliceProvenance;
use crate::staging::{CorrectionObserver, SharedCandidateDesk};
use crate::steering::{ActiveTurn, IntakeRecord, StopClaim, TurnControl, ConversationControl};
use crate::stream::{StreamEvent, TurnObserver};
use crate::thread::{summaries, ThreadSummary};
use crate::timeline::Timeline;
use crate::util::now_millis;
use crate::worker_events::{self, WorkerEventRow};
use std::collections::VecDeque;
use std::path::PathBuf;

/// Existing durable Stop and intake protocol values.
pub const ACCOUNT_SWITCH: &str = "account-switch";
pub const ACCOUNT_EXHAUSTED: &str = "account-exhausted";
pub const STOPPED_BY_CEO: &str = "stopped_by_ceo";
pub const DESK_CHANNEL: &str = "desk";
/// Existing correction-evidence window.
pub const ANCHOR_WINDOW_MESSAGES: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum SpineError {
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Cognition(#[from] CognitionError),
    /// No thread is active AND none can be chosen without guessing. UX §21 "Entity
    /// binding failure": *"Block send. State that Rich cannot safely determine which
    /// entity the work belongs to. Require an explicit entity choice. Never default to
    /// the last entity."* That is why this no longer silently creates a thread.
    #[error(
        "no active thread, and no entity was named — Rich will not guess which entity area this \
         belongs to. Choose an entity, or activate an existing thread."
    )]
    NoActiveThread,
    /// An entity id that is not in the registry (ECS §3.3: never default, never guess).
    #[error("unknown entity {0}: not in the entity registry")]
    UnknownEntity(String),
    #[error("no compute lease attached")]
    NoLease,
    #[error("no lease factory attached — cannot rotate or recover")]
    NoLeaseFactory,
    /// The durable intake log (UX §9.2/§9.3) could not be written. Surfaced rather than
    /// swallowed: an undrained record is the CEO's words still waiting to be delivered.
    #[error("steering intake: {0}")]
    Steering(String),
    /// A job's result could not be taken off its assignment to be written into the
    /// conversation (the ordinary intake queue); it stays pending there.
    #[error("assignments: {0}")]
    Assignments(String),
}

struct SpareFrontDesk {
    lease: Box<dyn Cognition>,
    /// The entity this spare was primed FOR. A spare primed for one entity is never adopted by
    /// another — the same entity-boundary rule `IntakeRecord::Steer::thread_id` states — and the
    /// re-prime payload is entity-scoped (`reprime.rs`'s action digest, `company_block`), so a
    /// cross-entity adoption would hand one company's Rich to another company's first sentence.
    entity: EntityId,
    /// The id this desk is scoped to: reserved and unspent. **No ledger record carries it.**
    reserved_thread: String,
}

/// **What [`Spine::begin_a_spare`] decided, carried across the two steps that run with the
/// spine's mutex DOWN.** It owns everything those steps need and borrows nothing from the
/// `Spine`, which is the property that makes the unlocked road possible at all.
struct SparePlan {
    entity: EntityId,
    binding: ThreadBinding,
    reserved: String,
    /// The same shared factory, safe for independent conversation startup.
    factory: std::sync::Arc<dyn crate::cognition::LeaseFactory>,
    control: crate::steering::ConversationControl,
    started: std::time::Instant,
}

pub fn ready_a_spare_front_desk_without_the_spine(spine:&std::sync::Mutex<Spine>,entity:&EntityId)->SpareReady {
    let plan=match spine.lock().unwrap().begin_a_spare(entity) {Ok(plan)=>plan,Err(verdict)=>return verdict};
    match plan.factory.spawn_scoped(&plan.binding,&plan.control) {
        Ok(lease)=>spine.lock().unwrap().install_the_spare(plan,lease),
        Err(error)=>spine.lock().unwrap().abandon_a_spare(plan,error.to_string()),
    }
}

/// **What happened when the app asked for a spare front desk** —
/// [`Spine::ready_a_spare_front_desk`]'s answer, and a deliberate mirror of [`FrontDeskReady`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpareReady {
    /// A spare was spawned and primed by this call. `millis` is what the CEO's first message
    /// into the next brand-new thread would otherwise have waited through.
    Ready { millis: u64 },
    /// A primed spare for this entity was already standing there: nothing was spent, nothing
    /// was sent.
    AlreadyReady,
    /// A turn is in flight. Priming the spare is a model turn on its OWN child, so it breaks no
    /// continuity-§3.1 invariant — but the caller holds the spine's mutex for the whole of it,
    /// and taking it here would put a second multi-second hold in front of everything the
    /// running turn needs. Refused for the reason [`FrontDeskReady::TurnInProgress`] is.
    TurnInProgress,
    /// It could not be done: no lease factory, an unregistered entity, or the priming turn
    /// itself failed. The next brand-new thread then primes the way it does today, which is the
    /// only cost, and nothing is surfaced to the CEO.
    NotReady(String),
}

/// **ONE THREAD'S FRONT DESK, PARKED WHILE ANOTHER THREAD IS SPEAKING** — the CEO's Two
/// Riches page: *"each conversation thread always holds one front desk Rich and one
/// back-end Rich"*, and *"each conversation thread in the app can go forever."*
///
/// Before this, a thread switch called `clear_lease` and the outgoing child was KILLED:
/// coming back meant a fresh provider session, a fresh `claude` process and a full re-prime
/// off the ledger tail. His sentence was true as reconstitution and false as residence, and
/// the difference is visible — the re-primed Rich has only what the payload carried.
///
/// A parked lease is a live child with its own provider session, its own priming and its
/// own context accounting. Nothing here is a copy of the spine's state: the fields are
/// MOVED out of the chair and moved back into it, so there is never a second opinion about
/// how much context a session has used.
#[derive(Default)]
struct Resident {
    lease: std::sync::Arc<std::sync::Mutex<Option<Box<dyn Cognition>>>>,
    upstream_budget: crate::upstream::RetryBudget,
    context_usage: Option<ContextUsage>,
    parked_at_ms: u64,
}

/// How many front desks stay resident at once, the one in the chair included.
///
/// **His page says "any number of conversation threads" and this is not a limit on that.**
/// A thread is a ledger partition and there can be thousands; this bounds how many live
/// `claude` children the app holds open at one time, which is a memory and process
/// question rather than a product one. Past the cap the LEAST RECENTLY SPOKEN parked desk
/// is retired, which puts exactly that thread back to the behavior every thread had before
/// this change: its next turn spawns a fresh lease and re-primes from the ledger. So the
/// degradation is the old path rather than a new failure, and it never touches the desk in
/// the chair or the one he is typing into.
///
/// Eight, because that is more conversations than the CEO has ever had open at once and
/// eight idle children is a bounded, honest footprint. It is a number to revisit with a
/// measurement, not a constant with an argument behind it.
pub const MAX_RESIDENT_FRONT_DESKS: usize = 8;

/// **What happened when the app asked for the front desk to be ready BEFORE he types** —
/// [`Spine::prime_front_desk`]'s answer.
///
/// Every variant is a fact about a LEASE, never a promise about a turn: nothing here changes what
/// his next message does, only when the waiting for it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontDeskReady {
    /// The lease was spawned and/or primed by this call. `millis` is what he would otherwise have
    /// waited through on his first message.
    Ready { millis: u64, spawned: bool },
    /// It was already primed for this thread: nothing was spent and nothing was sent.
    AlreadyReady,
    /// A turn is in flight, so nothing was done. Priming is NEVER run underneath a turn — the
    /// lease is the thing running it, and a second prompt into it is what the continuity design
    /// forbids structurally (§3.1).
    TurnInProgress,
    /// It could not be done: the thread does not exist, has no entity home, or the priming turn
    /// itself failed. His first message then primes the way it always did, which is the only cost.
    NotReady(String),
}

struct Delivery {
    connection:Option<ConnectionPlan>,
    retry_wait:Option<(std::time::Duration,std::sync::Arc<dyn crate::upstream::RetryClock>)>,
    turn_id:String,
    binding:ThreadBinding,
    text:String,
    allow_recovery:bool,
    internal_turn:bool,
    live_turn:LiveTurn,
    persist_err:Option<LedgerError>,
    stopped_before_prompt:bool,
    preparation_failed:bool,
    preparation_stop:Option<String>,
    preparation:Result<(),CognitionError>,
    lease:std::sync::Arc<std::sync::Mutex<Option<Box<dyn Cognition>>>>,
    usage:Option<ContextUsage>,
    response:Option<crate::cognition::PendingReply>,
    session_id:Option<String>,
    account:Option<String>,
}

fn enqueue_delivery(delivery: &mut Delivery) -> Result<(), CognitionError> {
    if delivery.stopped_before_prompt { return Ok(()); }
    std::mem::replace(&mut delivery.preparation, Ok(()))?;

    Ok(())
}

fn run_delivery(delivery: &mut Delivery, on_item: &mut dyn FnMut(&mut Delivery, TurnItem)) -> Result<String, CognitionError> {
    if delivery.stopped_before_prompt {
        return Ok(delivery.preparation_stop.take().unwrap_or_else(|| crate::native::STOP_REASON_CANCELLED.into()));
    }
    let response = delivery.response.take().ok_or_else(|| CognitionError::Protocol("No submitted conversation input".into()))?;
    response.wait(&mut |item| on_item(delivery, item))
}

/// Submit every available input before waiting. Claude Code owns its scheduling.
pub fn drive_shared_turns(spine: &std::sync::Mutex<Spine>) -> Result<(), SpineError> {
    let mut failure = None;
    loop {
        let ran = std::thread::scope(|workers| {
            let mut submitted = Vec::new();
            loop {
                let Some(mut delivery) = spine.lock().unwrap().pending_delivery.pop_front() else { break };
                // Preserve stdin submission order. Each reply then waits independently.
                prepare_delivery_io(&mut &*spine, &mut delivery);
                let queued = enqueue_delivery(&mut delivery);
                submitted.push(workers.spawn(move || {
                    let binding = delivery.binding.clone();
                    let result = queued.and_then(|_| run_delivery(&mut delivery, &mut |delivery, item| {
                        spine.lock().unwrap().capture_delivery_item(delivery, item);
                    }));
                    let mut state = spine.lock().unwrap();
                    let delivered = state.finish_delivery(delivery, result);
                    let boundary = state.after_turn_boundary(&binding);
                    delivered.and(boundary)
                }));
            }
            let ran = !submitted.is_empty();
            for worker in submitted {
                if let Err(error) = worker.join().expect("native reply owner panicked") {
                    if failure.is_none() { failure = Some(error); }
                }
            }
            ran
        });
        if !ran { break; }
    }
    match failure { Some(error) => Err(error), None => Ok(()) }
}

// Shared state is needed for durable commits, never for provider I/O.
trait PreparationState {
    fn state<R>(&mut self, run: impl FnOnce(&mut Spine) -> R) -> R;
}
impl PreparationState for &mut Spine {
    fn state<R>(&mut self, run: impl FnOnce(&mut Spine) -> R) -> R {
        run(self)
    }
}
impl PreparationState for &std::sync::Mutex<Spine> {
    fn state<R>(&mut self, run: impl FnOnce(&mut Spine) -> R) -> R {
        run(&mut self.lock().unwrap())
    }
}
struct ConnectionPlan {
    binding: ThreadBinding,
    control: crate::steering::ConversationControl,
    factory: Option<std::sync::Arc<dyn LeaseFactory>>,
}
fn retain_preparation_item(
    state: &mut impl PreparationState,
    binding: &ThreadBinding,
    turn: Option<&str>,
    item: TurnItem,
) {
    if let TurnItem::Machinery(record) = item {
        state.state(|s| {
            Spine::retain_and_emit_machinery(
                s.machinery_journal.as_ref(),
                s.machinery_observer.as_deref(),
                record.stamp(binding.thread_id(), turn, true),
            )
        });
    }
}


/// `turn` is the reply this connection is prepared for: it runs on the lease's account until
/// [`crate::claude_accounts::Accounts::turn_ended`].
fn prepare_connection(state:&mut impl PreparationState,plan:&ConnectionPlan,slot:&std::sync::Arc<std::sync::Mutex<Option<Box<dyn Cognition>>>>,turn:&str)->Result<bool,SpineError> {
    let mut slot=slot.lock().unwrap();
    if slot.as_ref().is_some_and(|lease|lease.is_closed()) {drop(slot.take());}
    if let Some(lease)=slot.as_ref() {
        let account_changed=plan.factory.as_ref().and_then(|factory|factory.quota())
            .zip(lease.account()).is_some_and(|(quota,account)|quota.before_turn(account,lease.streamed_usage())!=account);
        // The user's own settings files changed since this lease read them (item 0).
        let settings_changed=crate::cognition::user_settings_changed(plan.factory.as_deref(),lease.as_ref());
        if (account_changed || settings_changed) && !crate::cognition::account_resume_blocked(lease.as_ref()) {
            // The normal opener below resumes the same transcript under the chosen account
            // and the user's current settings.
            drop(slot.take());
        }
    }
    let spawned=slot.is_none();
    if spawned {
        *slot=Some(plan.factory.as_ref().ok_or(SpineError::NoLease)?.spawn_scoped(&plan.binding,&plan.control)?);

    }
    if let Some(lease)=slot.as_mut() {
        if spawned {
            plan.control.set_cancel(lease.cancel_handle());
            plan.control.set_lease_session(Some(lease.session_id().to_string()));
        }
        state.state(|s| {
            if let Some(active)=plan.control.active_turn() {s.ledger.mark_turn_started(&active.turn_id,lease.session_id())?;}
            if let (Some(quota),Some(account))=(s.quota_service(),lease.account()) {quota.accounts.ran_on(turn,account);}
            s.raise_quota_notices(Some(plan.binding.thread_id()));
            Ok::<_,SpineError>(())
        })?;
    }
    Ok(spawned)
}
/// **Haiku names the conversation his first message opens, beside the reply and never in front
/// of it** (CEO, 2026-10-19). The titler runs on a thread of its own: the reply does not wait
/// for it, and a titler that fails or never answers leaves the provisional title in place.
fn start_title(observer: Option<TitleObserver>, writer: Option<crate::cognition::TitleWriter>, thread: &str, first: String) {
    let (Some(observer), Some(write)) = (observer, writer) else { return };
    let thread = thread.to_string();
    let started = std::thread::Builder::new().name("richos-title".into())
        .spawn(move || title_conversation(&*write, &*observer, &thread, &first));
    if let Err(error) = started {
        eprintln!("[richos] the conversation keeps its first title: {error}");
    }
}
/// The titler thread's whole body: Haiku's answer goes to the shell, and a failure sends
/// nothing, so the provisional title stays. Returns whether a title was sent.
fn title_conversation(write: &dyn Fn(&str) -> Result<String, String>, observer: &dyn Fn(&str, &str), thread: &str, first: &str) -> bool {
    match write(first) {
        Ok(title) => { observer(thread, &title); true }
        Err(error) => { eprintln!("[richos] the conversation keeps its first title: {error}"); false }
    }
}
#[cfg(test)]
mod title_tests {
    /// Item 3: Haiku cannot be reached, so nothing reaches the shell and the provisional
    /// title stays. Synchronous: the body the titler thread runs, called directly.
    #[test]
    fn an_unreachable_haiku_sends_no_title() {
        let sent = std::cell::RefCell::new(Vec::new());
        let observer = |thread: &str, title: &str| sent.borrow_mut().push((thread.to_string(), title.to_string()));
        assert!(!super::title_conversation(&|_| Err("Classifier disconnected".into()), &observer, "thr_a", "Draft the board update"));
        assert!(sent.borrow().is_empty(), "a failure produced a title");
        assert!(super::title_conversation(&|_| Ok("Board update".into()), &observer, "thr_a", "Draft the board update"));
        assert_eq!(*sent.borrow(), vec![("thr_a".to_string(), "Board update".to_string())], "positive control");
    }
}
fn prepare_delivery_io(state: &mut impl PreparationState, delivery: &mut Delivery) {
    let Some(plan) = delivery.connection.as_ref() else {
        return;
    };
    let intake_id = state.state(|s| s.ledger.turn(&delivery.turn_id).and_then(|turn| turn.intake_id));
    let stopped = || plan.control.stop_claim().is_some_and(|claim| claim.covers(&delivery.turn_id, intake_id));
    if stopped() { delivery.stopped_before_prompt = true; return; }
    if let Some((wait, clock)) = delivery.retry_wait.take() {
        clock.wait(wait, &|| {
            plan.control.stop_claim_for(&delivery.turn_id).is_some()
        });
    }
    let prepared=prepare_connection(state,plan,&delivery.lease,&delivery.turn_id).and_then(|_| {
        if stopped() {return Ok(())}
        let (original,source,conversation,channel,app_messages,first_words)=state.state(|s| {
            let mut successor=delivery.turn_id.as_str();let mut seen=std::collections::HashSet::new();
            while let Some(previous)=s.ledger.turns().iter().find(|t|t.superseded_by.as_deref()==Some(successor)&&t.thread_id==plan.binding.thread_id()&&seen.insert(t.id.clone())) {
                successor=&previous.id;
            }
            let original=successor.to_string();let turn=s.ledger.turn(&delivery.turn_id).ok_or(SpineError::NoLease)?;
            let channel=s.input_channels.get(&delivery.turn_id).cloned().or_else(||turn.channel.clone());
            let mut conversation=String::new();
            for message in s.ledger.messages_scoped(&plan.binding)?.into_iter().filter(|m|m.turn_id!=delivery.turn_id){conversation.push_str(&format!("{}: {}\n",message.role,message.text));}
            let mut from=conversation.len().saturating_sub(8192);while !conversation.is_char_boundary(from){from+=1;}
            let scoped=s.ledger.thread_turns(plan.binding.thread_id())?;
            // His first message in this conversation: the words are his (typed or spoken, at
            // the desk or on the phone), never a pressed button's or another agent's, and no
            // message of his came EARLIER in the ledger. Earlier, not "nothing else": a
            // follow-up he sent while the conversation was still starting is already
            // recorded when this one is delivered, and it must not hide the first.
            let his=|t:&crate::ledger::Turn|t.superseded_by.is_none() && matches!(t.source,Source::Text|Source::Jam) && !t.pressed;
            let earlier=scoped.iter().take_while(|t|t.id!=delivery.turn_id).any(|t|his(t));
            let first_words=(his(turn) && !earlier).then(||turn.user_text.clone());
            let current=|id:&str|scoped.iter().any(|turn|turn.id==id&&turn.superseded_by.is_none());
            let mut app_messages:Vec<_>=scoped.iter().filter(|turn|turn.source==Source::Proactive&&turn.superseded_by.is_none())
                .map(|turn|(turn.id.clone(),turn.assistant_text.clone())).collect();
            app_messages.extend(s.ledger.actions().iter().filter(|action|action.kind=="settings_change"&&action.turn_id.as_deref().is_some_and(current))
                .map(|action|(action.id.clone(),action.detail.clone())));
            Ok::<_,SpineError>((original,turn.source,conversation[from..].to_string(),channel,app_messages,first_words))
        })?;
        let mut slot=delivery.lease.lock().unwrap();
        let lease=slot.as_mut().ok_or(SpineError::NoLease)?;
        if let Some(first)=first_words {
            start_title(state.state(|s|s.title_observer.clone()),lease.title_writer(),plan.binding.thread_id(),first);
        }
        if let Some(scope)=state.state(|s|s.onboarding_tool_scope(&plan.binding)) {
            lease.set_onboarding_scope(plan.binding.entity_id(),&scope.central_root,&scope.record_path)?;
        }
        let company=state.state(|s|s.company_block(&plan.binding));
        lease.set_company_context(company.as_deref())?;
        lease.prepare_work_turn(&plan.binding,&delivery.turn_id,&original,source,&delivery.text,&mut |item|retain_preparation_item(state,&plan.binding,Some(&delivery.turn_id),item))?;
        lease.set_conversation_context(&conversation);
        lease.set_app_messages(app_messages);
        lease.set_input_channel(channel.as_deref())?;
    let control = plan.control.clone();
    let turn = ActiveTurn { turn_id: delivery.turn_id.clone(), thread_id: delivery.binding.thread_id().into(),
        entity_id: Some(delivery.binding.entity_id().clone()), started_at: None };
    delivery.account=lease.account().map(str::to_string);
    delivery.response = Some(lease.enqueue_prompt(&delivery.text, Box::new(move || {
        if control.stop_claim().is_some_and(|claim| claim.covers(&turn.turn_id, intake_id)) { return Err(crate::native::STOP_REASON_CANCELLED.into()); }
        control.begin_turn(turn);
        Ok(())
    }))?);
        Ok(())
    });
    delivery.stopped_before_prompt = delivery.response.is_none() && stopped();
    delivery.preparation_stop = match &prepared {
        Err(SpineError::Cognition(CognitionError::PrimingStopped(reason))) => Some(reason.clone()),
        _ => None,
    };
    delivery.preparation_failed = prepared.is_err();
    delivery.preparation = prepared.map_err(|e| match e {
        SpineError::Cognition(e) => e,
        other => CognitionError::Protocol(other.to_string()),
    });
    delivery.session_id = plan.control.lease_session();
}
fn prime_front_desk_with(state:&mut impl PreparationState,thread:&str)->FrontDeskReady {
    let begun=state.state(|s| {
        if s.thread_is_running(thread) {return Err(FrontDeskReady::TurnInProgress)}
        if s.resident.get(thread).is_some_and(|desk|desk.lease.try_lock().is_ok_and(|slot|slot.is_some())) {return Err(FrontDeskReady::AlreadyReady)}
        let binding=s.ledger.thread_binding(thread).map_err(|e|FrontDeskReady::NotReady(e.to_string()))?;
        let plan=s.select_connection(&binding).map_err(|e|FrontDeskReady::NotReady(e.to_string()))?;
        plan.control.begin_front_desk_prime();
        let desk=s.resident.entry(thread.into()).or_default().lease.clone();
        Ok((plan,desk))
    });
    let (plan,desk)=match begun {Ok(value)=>value,Err(verdict)=>return verdict};
    let started=std::time::Instant::now();
    // The priming is a reply of its own on the lease's account, over when it returns.
    let priming=format!("prime:{thread}");
    let result=prepare_connection(state,&plan,&desk,&priming);
    state.state(|s| {
        if let Some(quota)=s.quota_service() {quota.accounts.turn_ended(&priming);}
        if let Some(resident)=s.resident.get_mut(thread) {resident.parked_at_ms=now_millis();}
        plan.control.end_front_desk_prime();
        if let Err(error)=s.poll_intake() {eprintln!("[richos] queued input after preparation: {error}");}
    });
    match result {Ok(spawned)=>FrontDeskReady::Ready{millis:started.elapsed().as_millis() as u64,spawned},Err(error)=>FrontDeskReady::NotReady(error.to_string())}
}
/// Prepare this conversation without holding shared state across provider operations.
pub fn prime_front_desk_without_the_spine(
    spine: &std::sync::Mutex<Spine>,
    thread: &str,
) -> FrontDeskReady {
    prime_front_desk_with(&mut &*spine, thread)
}

pub struct Spine {
    ledger: Ledger,
    reader: Option<crate::read_view::SpineReader>,
    /// **The other front desks** — every thread that has spoken, keyed by thread id, minus
    /// whichever one currently holds [`Spine::lease`]. See [`Resident`].
    resident: std::collections::HashMap<String, Resident>,
    /// **The desk for the thread he has not started yet.** See [`SpareFrontDesk`].
    ///
    /// **ONE, and the bound comes from the product rather than from the price.** A spare covers
    /// "the next brand-new thread", and there is exactly one of those at any instant: the app
    /// has one composer and one new-thread screen, so a second spare could only ever be adopted
    /// by a second brand-new thread opened before the first had been spoken in, which no
    /// sequence of clicks can produce. `MAX_RESIDENT_FRONT_DESKS` bounds desks he HAS used and
    /// answers a different question.
    spare: Option<SpareFrontDesk>,
    /// **A spare's spawn and priming turn are in flight, and for part of that time this mutex
    /// is DOWN** — see [`ready_a_spare_front_desk_without_the_spine`]. The slot is reserved by
    /// step 1 and released by step 5, so a second ask arriving in the gap is answered
    /// `AlreadyReady` instead of spawning a second child for one slot.
    spare_in_flight: bool,
    /// THE ACTIVE CONTEXT (ECS §3.3): person + entity + thread + binding revision. Not a
    /// bare thread id — holding the full binding is what lets every downstream call be
    /// scoped without re-deriving (or re-guessing) the entity.
    active: Option<ThreadBinding>,
    /// The entity areas this spine will accept. **EMPTY by default** — the registry
    /// belongs to the person running the app and is loaded from their own configuration
    /// file (`entity.rs` rule 4), so a spine that nobody has told about any company accepts
    /// none. Membership is checked on thread creation, so an unregistered entity can never
    /// become a thread's immutable home; with an empty registry that means every
    /// `create_thread` is refused, which is the correct and honest state of an install
    /// whose owner has not said which company they work for yet.
    ///
    /// The shell installs the real one with [`Spine::set_entity_registry`] as soon as it
    /// has read the file.
    registry: EntityRegistry,
    /// The turn-boundary controller state. Keyed on turn-in-progress (NOT workers-live):
    /// a turn can END while engine subagents keep running, so delivery/rotation proceeds
    /// the moment the turn ends. Rotation NEVER happens inside a turn.
    turn_in_progress: bool,
    detached_delivery: bool,
    pending_delivery: VecDeque<Delivery>,
    rotation_count: u64,
    last_rotation_reason: Option<String>,
    /// **The conversations whose turns have ENDED since the shell last asked** — one binding
    /// per thread, in the order their first turn ended. See [`Spine::take_ended_turns`].
    ended_turns: Vec<ThreadBinding>,
    input_channels: std::collections::HashMap<String,String>,
    /// The live UI sink (streaming deltas + turn state). Optional so the spine runs
    /// headless (tests, the round-trip examples) with zero UI attached.
    observer: Option<Box<dyn TurnObserver>>,
    /// Spawns a fresh, un-primed lease at rotation/recovery time. `None` means the spine
    /// can prime/run ONE lease (whatever was `attach_lease`d) but can never rotate or
    /// recover from a crash — an honest degrade for tests/contexts with no respawn story.
    lease_factory: Option<std::sync::Arc<dyn LeaseFactory>>,
    /// The optional Tier-C seam (continuity §2.3/§4) — a different engineer's parallel
    /// work on `loro/`. Absent by default; the re-prime payload degrades gracefully.
    loro_compiler: Option<Box<dyn LoroContextCompiler>>,
    /// The optional Tier-E seam — the evidence lookup over the CEO's own files (the
    /// 2026-09-17 evidence-retrieval ruling §1/§4). Absent by default, exactly like the
    /// compiler above, and the payload degrades to `EvidenceTier::NotWired`.
    ///
    /// **It is gated on the compiler, not merely placed beside it.** The lookup runs only
    /// when the compiled slice's own `coverage` says memory did not answer, so an install
    /// with a lookup and no compiler never looks at anything — there is no coverage label,
    /// and absence of the signal is not the signal.
    evidence_lookup: Option<Box<dyn EvidenceLookup>>,
    /// **Keep the mouth his words came through on each prompt record** (operator back-end
    /// spec r3 (s)). Off unless the shell turns it on, which it does only on an install whose
    /// `operator.json` passed the gate at launch, so every product install writes exactly the
    /// ledger lines it wrote before. See `ledger::Event::PromptReceived::channel`.
    keep_intake_channel: bool,
    /// The machinery journal (techy-mode design §2.1). `None` means machinery is routed
    /// and emitted but NOT retained — an honest degrade for tests and headless runs, not
    /// a supported product state: §3.2's rule is that retention runs ALWAYS, because the
    /// CEO's requirement is to flip a thread he ALREADY HAD.
    machinery_journal: Option<MachineryJournal>,
    /// The live machinery sink. A SEPARATE observer from `TurnObserver` on purpose — two
    /// families means the default UI's subscription list is the proof that the calm view
    /// carries no machinery (§3.3).
    machinery_observer: Option<Box<dyn MachineryObserver>>,
    /// THE OUTPUT RECORD (Output side panel PRD §3-§4) and the evidence folder its witnesses
    /// (b) and (c) read, `<app-data>/engine-state/evidence`. `None` — every test and headless
    /// run — records nothing and behaves exactly as before; the shell sets both
    /// ([`Spine::set_output_store`]).
    output: Option<(crate::output::OutputStore, Option<PathBuf>)>,
    /// The THIRD live sink: the additive §13 event family (`live.rs`). Separate from
    /// `observer` on purpose — the four `stream.rs` events are unchanged and a UI that
    /// listens only to them is unaffected by anything on this one.
    live: Option<Box<dyn LiveObserver>>,
    /// THE FLYWHEEL'S AUTOMATIC TRIGGER (`spoken.rs` + `staging.rs`). Optional: a build
    /// with no desk attached behaves exactly as this file did before it existed. Held as a
    /// `SharedCandidateDesk` (an `Arc<Mutex<_>>`) rather than owned, so answering §7's
    /// question never waits on the turn lock — see `staging::SharedCandidateDesk`.
    candidates: Option<SharedCandidateDesk>,
    /// The dictation journal the heard-vs-sent trigger reads. `None` on every install
    /// without a journalling dictation app, and on every install where the CEO has not
    /// switched the trigger on — see [`Spine::set_heard_source`].
    heard: Option<Box<dyn crate::heard::HeardSource>>,
    /// THE LORO DESK, and the memory it resolves against (`belief.rs` + `loro.rs`).
    /// Optional as a PAIR: a build with a desk and no provenance can resolve nothing, and a
    /// build with provenance and no desk has nowhere to file. Both absent is the ordinary
    /// state of an install with no corpus.
    correction_desk: Option<SharedCorrectionDesk>,
    loro_provenance: Option<SharedSliceProvenance>,
    proposal_observer: Option<Box<dyn ProposalObserver>>,
    /// Where a staged correction is announced. A FOURTH family, separate from the other
    /// three for the reason `machinery_observer` is separate from `observer`.
    correction_observer: Option<Box<dyn CorrectionObserver>>,
    /// THE CEO'S TWO MID-TURN CONTROLS (UX §9.2/§9.3), and the only thing in this struct
    /// that is reachable while a turn is running.
    ///
    /// Everything else here is behind `&mut self`, which `deliver()` holds for the whole
    /// turn. `TurnControl` is an `Arc` the shell keeps its own handle on, so a stop can be
    /// recorded and delivered to the lease without ever waiting for this spine's lock —
    /// which is the difference between a stop control and a button that fires after the
    /// work it meant to interrupt has finished.
    ///
    /// Defaults to `TurnControl::detached()`: turn bookkeeping works, stop and steer are
    /// REFUSED, because there is nowhere durable to record the request. Every pre-existing
    /// test and headless run therefore behaves exactly as it did.
    control: ConversationControl,
    /// THE CENTRAL FOLDER, or `None` when nobody has told this spine where it is.
    ///
    /// `None` is the shipped default and it is a real state rather than a placeholder: every
    /// headless example and every test in this crate runs without one and gets exactly the
    /// priming turn it got before this field existed. The Tauri shell sets it at boot
    /// ([`Spine::set_central_root`]).
    ///
    /// What it buys is `company.rs`: `<central>/companies/<entity>/company.md`, read at prime
    /// time and appended to the scoped assertion — `richos-central-folder-2026-09-06.md` §6,
    /// right-hand column, the half that "waits on the app".
    central_root: Option<std::path::PathBuf>,
    /// Where `onboarding.rs`'s declination record lives, or `None` when nobody has said.
    /// `None` behaves as "no declination on file", which leans toward asking — see
    /// `OnboardingRecord::load` on why the failure leans that way and only that way.
    onboarding_record: Option<std::path::PathBuf>,
    /// Where [`Spine::timeline`] reads the engine's worker-lifecycle stream from, so a
    /// `Task` tool call can be joined to the worker it spawned (UX §7).
    ///
    /// Defaults to [`WorkerEventsSource::Disabled`], which is what shipped before this
    /// slice: `Timeline::project` supplied an EMPTY stream, so `project_with_workers`
    /// existed and was reachable only from tests. Nothing in the app ever set it, which
    /// meant `TimelineItem::WorkerActivity` could not occur on the wire at all.
    worker_events: WorkerEventsSource,
    /// How an overload retry waits out the CEO's schedule (`upstream::OVERLOAD_RETRY_WAITS`).
    /// The real clock in the app; a test drives the 158 minutes with a fake one.
    retry_clock: std::sync::Arc<dyn crate::upstream::RetryClock>,
    /// Where Haiku's title for a new conversation goes (`thread_title.rs`). Called off the
    /// spine, on the titler's own thread, with `(thread_id, title)`.
    title_observer: Option<TitleObserver>,
}

/// Receives `(thread_id, title)` when Haiku has named a new conversation. The shell shows it
/// unless the user has named the conversation himself.
pub type TitleObserver = std::sync::Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Where a timeline read finds the engine's worker-lifecycle rows.
///
/// A source rather than a `Vec` because the stream is APPEND-ONLY and long-lived: a read
/// must see the rows written since the last one, and the team directory a session writes
/// to can change without the app relaunching. Both non-disabled variants re-read on every
/// call for that reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerEventsSource {
    /// No stream. Every `Task` call stays an ordinary activity row — the honest degrade
    /// when the engine's lifecycle hooks are not registered (they snapshot at session
    /// start, so a freshly installed emitter writes its first row only in the NEXT
    /// session).
    Disabled,
    /// A fixed file. Tests, and an explicit operator override.
    File(PathBuf),
    /// Resolve THIS SESSION's team directory on every read
    /// (`worker_status::resolve_team_dir`, honoring `RICHOS_TEAM_DIR`) and read its
    /// `worker-events.jsonl`.
    ///
    /// Re-resolved per read rather than bound at boot because the lease is swappable: a
    /// rotation mints a new session id, and the directory follows it. It is derived from
    /// that id and NEVER from a directory mtime — see `worker_status.rs`, "WHOSE workers
    /// these are". When no directory can be attributed, the stream is EMPTY, which costs
    /// exactly one thing (a `Task` call stays an ordinary activity row) and claims nothing.
    ///
    /// The home fallback (`~/.claude/worker-events.jsonl`) is deliberately never resolved —
    /// it accumulates across every session that ever missed a team dir and cannot be
    /// session-scoped (`worker_events.rs`, "Session scope").
    CurrentTeamDir,
}

impl WorkerEventsSource {
    /// Read the rows this source currently holds, for the session `session_id` names.
    ///
    /// A missing/unreadable file is an EMPTY stream, never an error: "no worker events" is
    /// a true and common state. So is `session_id: None` — no lease, therefore no
    /// directory this app is entitled to, therefore no rows.
    pub fn read(&self, session_id: Option<&str>) -> Vec<WorkerEventRow> {
        match self {
            WorkerEventsSource::Disabled => Vec::new(),
            WorkerEventsSource::File(p) => worker_events::read_stream(p),
            WorkerEventsSource::CurrentTeamDir => {
                match crate::worker_status::resolve_team_dir(session_id) {
                    Ok(dir) => worker_events::read_stream(&worker_events::worker_events_path(&dir)),
                    Err(_) => Vec::new(),
                }
            }
        }
    }
}

impl Spine {
    /// Share immutable window snapshots without sharing the turn's lock or writer.
    pub fn reader(&mut self) -> crate::read_view::SpineReader {
        if let Some(reader) = &self.reader { return reader.clone(); }
        let reader = crate::read_view::SpineReader::new(self);
        let updates = reader.clone();
        self.ledger.observe_reads(std::sync::Arc::new(move |event| updates.apply(event.clone())));
        self.reader = Some(reader.clone());
        reader
    }

    fn publish_read_metadata(&self) {
        if let Some(reader) = &self.reader { reader.refresh(self); }
    }

    pub(crate) fn read_metadata(&self) -> crate::read_view::ReadMetadata {
        crate::read_view::ReadMetadata {
            active: self.active.clone(), registry: self.registry.clone(),
            machinery_root: self.machinery_journal.as_ref().map(|j| j.root().to_path_buf()),
            workers: self.worker_events.clone(), control: self.control.clone(),
            central_root: self.central_root.clone(), onboarding_record: self.onboarding_record.clone(),
        }
    }

    pub fn new(ledger: Ledger) -> Self {
        Spine {
            ledger,
            reader: None,
            resident: std::collections::HashMap::new(),
            spare: None,
            spare_in_flight: false,
            active: None,
            registry: EntityRegistry::empty(),
            turn_in_progress: false,
            detached_delivery: false,
            pending_delivery: VecDeque::new(),
            rotation_count: 0,
            last_rotation_reason: None,
            control: TurnControl::detached().for_thread(""),
            central_root: None,
            onboarding_record: None,
            ended_turns: Vec::new(),
            input_channels: std::collections::HashMap::new(),
            observer: None,
            lease_factory: None,
            loro_compiler: None,
            evidence_lookup: None,
            keep_intake_channel: false,
            machinery_journal: None,
            machinery_observer: None,
            output: None,
            live: None,
            candidates: None,
            heard: None,
            correction_observer: None,
            correction_desk: None,
            loro_provenance: None,
            proposal_observer: None,
            worker_events: WorkerEventsSource::Disabled,
            retry_clock: std::sync::Arc::new(crate::upstream::SystemRetryClock),
            title_observer: None,
        }
    }

    /// Attach the sink for Haiku's conversation titles. Headless callers never set one, and
    /// then no title is asked for.
    pub fn set_title_observer(&mut self, observer: TitleObserver) {
        self.title_observer = Some(observer);
    }

    /// Point the timeline read path at the engine's worker-lifecycle stream (UX §7).
    ///
    /// Without this a `Task` call projects as an ordinary activity row reading *"Worked"*
    /// — no name, no state, no chip — because there is no identity to join by. With it,
    /// a `Task` call that carries an extractable `agentId` AND has at least one row in the
    /// stream **for the same session** becomes a `TimelineItem::WorkerActivity`.
    ///
    /// ## The id spaces match — settled 2026-08-29, no longer a caveat
    /// The join's session clause compares the MACHINERY record's `session_id` — the lease's
    /// session id the adapter minted (`Cognition::session_id`) — against the worker row's
    /// `session_id`, which the engine hook read from the Claude Code harness. This used to
    /// say the two might be different id spaces and could not be measured in this checkout.
    /// They are the SAME: the probe's ACP session id
    /// `55c79b81-ace3-4b07-a5f3-406853ac1a36`
    /// (`docs/verification/acp-emission-probe-2026-08-28/run1.raw.jsonl`) has a Claude Code
    /// transcript at `~/.claude/projects/-Users-alex-ab-richos-engine/55c79b81-….jsonl`. So
    /// the join can fire in production, and `WorkerEventsSource::CurrentTeamDir` derives its
    /// directory from that same id.
    ///
    /// A refusal is still possible and still reported — it now means one thing,
    /// "genuinely another session's worker", surfaced as
    /// `RejectionReason::WorkerSessionMismatch` so it can be told apart from "the engine
    /// emitted nothing".
    ///
    /// Loosening the session clause to make the join fire is not an option: `agent_id` is
    /// not globally unique (the engine's own residue reuses one id across twelve rows) and
    /// the clause is what stops another session's worker name and authored summary
    /// rendering inside this entity's thread (timeline.rs `project_with_workers`).
    pub fn set_worker_events(&mut self, source: WorkerEventsSource) {
        self.worker_events = source;
        self.publish_read_metadata();
    }

    pub fn worker_events_source(&self) -> &WorkerEventsSource {
        &self.worker_events
    }

    /// Attach the live UI sink. The spine emits turn-start, per-delta chunk, and
    /// terminal (completed/error) events to it — keyed to thread + turn — while the
    /// ledger remains the source of truth. Headless callers simply never set one.
    pub fn set_observer(&mut self, observer: Box<dyn TurnObserver>) {
        self.observer = Some(observer);
    }

    /// Emit one live turn event to the UI sink, if attached. Infallible: a missing or
    /// non-listening UI never affects the turn (the ledger already holds the truth).
    fn emit(&self, event: StreamEvent) {
        if let Some(obs) = self.observer.as_deref() {
            obs.on_event(&event);
        }
    }

    /// Attach the ADDITIVE live-work sink (UX brief §13). Optional and independent: the
    /// four `stream.rs` events fire identically whether or not this is set, so the
    /// shipping UI is unaffected by its presence or absence.
    pub fn set_live_observer(&mut self, observer: Box<dyn LiveObserver>) {
        self.live = Some(observer);
    }

    /// THE ONE CHOKEPOINT for the additive family — every `LiveEvent` in this file goes
    /// through here or through [`Spine::forward_live`], which is this function's body.
    fn emit_live(&self, events: Vec<LiveEvent>) {
        Self::forward_live(self.live.as_deref(), events);
    }

    /// The gate, as an associated function so the streaming closure in `deliver` (which
    /// holds a `&mut Ledger` and therefore cannot call `&self` methods) enforces the SAME
    /// rule rather than a second copy of it.
    ///
    /// **Only `Visibility::Ceo` leaves this process on the additive family.** An internal
    /// item (re-prime, rotation, model reasoning, a Tier-3 silent proactive message) and a
    /// technical-only item are both refused here; technical detail already has its own
    /// family, `rich://machinery`, which the calm view does not subscribe to. The item was
    /// CONSTRUCTED before being refused, deliberately: a guard that is never asked cannot
    /// be tested, and `tests/live_event_tests.rs` removes this line to watch it leak.
    fn forward_live(observer: Option<&dyn LiveObserver>, events: Vec<LiveEvent>) {
        let Some(obs) = observer else { return };
        for e in events {
            if e.may_reach_webview() {
                obs.on_live_event(&e);
            }
        }
    }

    /// §13 `rich://thread-summary-updated` for one thread, computed exactly the way
    /// `thread::summaries` computes the sidebar today — same title, same scoped message
    /// count, same recency — so a live row and a re-listed row can never disagree.
    fn thread_summary_event(&self, binding: &ThreadBinding, turn_id: &str, status: ThreadStatus) -> Vec<LiveEvent> {
        let thread_id = binding.thread_id();
        let Some(thread) = self.ledger.threads().iter().find(|t| t.id == thread_id) else {
            return Vec::new();
        };
        let message_count = self.ledger.messages(thread_id).map(|m| m.len()).unwrap_or(0);
        let last_activity = self
            .ledger
            .turns()
            .iter()
            .filter(|tn| tn.thread_id == thread_id)
            .map(|tn| tn.created_at)
            .max()
            .unwrap_or(thread.created_at);
        vec![LiveEvent::ThreadSummaryUpdated {
            fence: EventFence::for_turn(binding, turn_id),
            title: thread.title.clone(),
            message_count,
            last_activity,
            status,
            at: now_millis(),
        }]
    }

    /// `rich://ceo-message` — his own sentence, read back OUT of the ledger for the reason
    /// [`Spine::turn_status_event`] is: the wire must carry what survived the write.
    ///
    /// **THE ID AND THE INSTANT ARE THE PROJECTION'S**, `{turn_id}:user` and `turn.created_at`,
    /// which is what makes a consumer that merges on `id` see one row rather than two — see
    /// [`LiveEvent::CeoMessage`]. They are derived from the same turn record `timeline.rs`
    /// derives them from, so a change in `timeline.rs` that moved either would be caught by
    /// `tests/live_event_tests.rs`'s wire-and-reload agreement rather than by a phone.
    ///
    /// Returns nothing at all for a turn with no CEO text — a proactive turn has none by
    /// construction, and the projection contributes no user item for one either.
    fn ceo_message_event(&self, binding: &ThreadBinding, turn_id: &str) -> Vec<LiveEvent> {
        let Some(turn) = self.ledger.turn(turn_id) else { return Vec::new() };
        // A pressed button's words are not his, so they are never sent as his row (finding 14).
        if turn.user_text.is_empty() || matches!(turn.source, Source::Proactive | Source::Agent) || turn.pressed {
            return Vec::new();
        }
        vec![LiveEvent::CeoMessage {
            fence: EventFence::for_turn(binding, turn_id),
            message_id: format!("{turn_id}:user"),
            text: turn.user_text.clone(),
            source: turn.source,
            created_at: turn.created_at,
            client_id: turn.client_id.clone(),
            voice_note: turn.voice_note,
            at: now_millis(),
        }]
    }

    /// §13 `rich://turn-status`, read back OUT of the ledger rather than assembled from
    /// the values in hand: `started_at` and the MEASURED `active_ms` are whatever survived
    /// the write, so the wire cannot report a span the ledger does not hold.
    fn turn_status_event(
        &self,
        binding: &ThreadBinding,
        turn_id: &str,
        status: TurnStatus,
        supersedes_turn_id: Option<String>,
    ) -> Vec<LiveEvent> {
        let turn = self.ledger.turn(turn_id);
        vec![LiveEvent::TurnStatus {
            fence: EventFence::for_turn(binding, turn_id),
            status,
            started_at: turn.and_then(|t| t.started_at),
            active_duration_ms: turn.and_then(|t| t.active_ms()),
            supersedes_turn_id,
            answered: false,
            at: now_millis(),
        }]
    }

    /// Attach a fresh compute lease. This is the swappable-lease seam: a later
    /// turn-boundary rotation re-attaches here and the spine re-primes it before the
    /// next turn. Marks the lease un-primed so continuity re-injection happens.
    /// Attach the machinery journal (§2.1). Retention is unconditional once attached —
    /// there is no flag gating it, deliberately (§3.2).
    pub fn set_machinery_journal(&mut self, journal: MachineryJournal) {
        self.machinery_journal = Some(journal);
        self.publish_read_metadata();
    }

    pub fn has_machinery_journal(&self) -> bool {
        self.machinery_journal.is_some()
    }

    /// Read-only access for a UI/command layer that wants to project a thread's machinery
    /// (`journal.project_thread`). Read-only because nothing outside the spine may write
    /// to this store.
    pub fn machinery_journal(&self) -> Option<&MachineryJournal> {
        self.machinery_journal.as_ref()
    }

    /// Attach the live `rich://machinery` sink. Optional: the spine runs headless with
    /// nothing listening, and per §2.2 a UI that isn't listening never stalls a turn.
    pub fn set_machinery_observer(&mut self, observer: Box<dyn MachineryObserver>) {
        self.machinery_observer = Some(observer);
    }

    /// Attach the output record (Output side panel PRD §3) and the evidence folder,
    /// `<app-data>/engine-state/evidence`, its hook witnesses write to.
    ///
    /// With it, witness (a) records each of Rich's own completed write-tool calls at the live
    /// merge, and at the end of every turn this lease's `writes.jsonl` is projected (witnesses
    /// (b) and (c): a front-desk worker's writes and the files Rich's commands made).
    pub fn set_output_store(&mut self, store: crate::output::OutputStore, evidence_root: Option<PathBuf>) {
        self.output = Some((store, evidence_root));
    }

    /// Witnesses (b) and (c) for the lease that just ran a turn of `thread_id`: its session's
    /// `writes.jsonl`, joined through the ledger's `TurnStarted` rows (one lease serves one
    /// thread). A failure is logged and never fails the turn (§4.7).
    fn project_output_evidence(&self, thread_id: &str) {
        let Some((store, Some(root))) = self.output.as_ref() else { return };
        let Some(session)=self.control.for_thread(thread_id).lease_session() else {return};
        let join = crate::output::SessionJoin::new(self.ledger.turns(), &[]);
        if let Err(e) = store.project_session(thread_id, root, &session, &join) {
            eprintln!("[richos] output: this turn's hook witnesses could not be recorded: {e}");
        }
    }

    /// Attach the correction-staging desk. **This is what makes the flywheel's trigger
    /// automatic**: with a desk attached, every CEO utterance is examined as it is
    /// submitted, and a correction is written down without a command being typed. With no
    /// desk attached this file behaves exactly as it did before the desk existed.
    /// Attach the loro correction desk the belief trigger files into. The SAME `Arc` the
    /// Tauri commands hold, so a proposal filed inside a turn is answerable during it.
    pub fn set_correction_desk(&mut self, desk: SharedCorrectionDesk) {
        self.correction_desk = Some(desk);
    }

    /// Attach the provenance of what memory Rich was actually given. The SAME `Arc` the
    /// context compiler writes to (`CliContextCompiler::set_provenance_sink`) — without it
    /// the belief trigger can resolve nothing and stays silent, which is the honest
    /// degradation rather than a guessed reference.
    pub fn set_loro_provenance(&mut self, prov: SharedSliceProvenance) {
        self.loro_provenance = Some(prov);
    }

    pub fn set_proposal_observer(&mut self, observer: Box<dyn ProposalObserver>) {
        self.proposal_observer = Some(observer);
    }

    pub fn set_candidate_desk(&mut self, desk: SharedCandidateDesk) {
        self.candidates = Some(desk);
    }

    /// Attach the dictation journal — the "heard" side of the DIFF trigger.
    ///
    /// **Optional, and OFF unless something calls this.** Two independent reasons, and the
    /// second is the one that matters:
    ///
    /// 1. Most installs have no journalling dictation app at all, so there is nothing to
    ///    read. `heard.rs`'s trigger cannot fire without a journal, which is an honest
    ///    degradation rather than a failure.
    /// 2. **It is the only one of the three triggers that did not measure precision
    ///    1.000.** It measured 0.972 over 156 invented pairs, and the single false positive
    ///    is a pair that would corrupt an ordinary English word in every future decode
    ///    (`heard.rs`, "the one false positive"). It also fires on an edit the CEO never
    ///    volunteered, so a wrong question here is more expensive than in either trigger
    ///    before it. Until that defect is repaired in the SHARED expansion rule where it
    ///    lives, this stays behind a deliberate switch rather than on by default, and the
    ///    shell honours `RICHOS_HEARD_TRIGGER` rather than deciding for him.
    pub fn set_heard_source(&mut self, source: Box<dyn crate::heard::HeardSource>) {
        self.heard = Some(source);
    }

    /// Attach the sink that renders §7's ask. Optional and independent of the desk: the
    /// question is durable on disk BEFORE this is called, so a surface that is not
    /// listening loses the prompt, never the record.
    pub fn set_correction_observer(&mut self, observer: Box<dyn CorrectionObserver>) {
        self.correction_observer = Some(observer);
    }

    /// The shared handle, for the surface that answers §7's three outcomes. It is a handle
    /// rather than a borrow precisely so the answer path does not go through this struct.
    pub fn candidate_desk(&self) -> Option<&SharedCandidateDesk> {
        self.candidates.as_ref()
    }

    pub fn attach_lease(&mut self,lease:Box<dyn Cognition>) {
        let thread=self.active_thread().expect("attach_lease requires a conversation").to_string();
        self.restore_front_desk(&thread,Resident{lease:std::sync::Arc::new(std::sync::Mutex::new(Some(lease))),parked_at_ms:now_millis(),..Default::default()});
    }



    pub fn lease_session_id(&self)->Option<String> {
        self.control.for_thread(self.active_thread()?).lease_session()
    }







    fn restore_front_desk(&mut self,thread:&str,mut resident:Resident) {
        let control=self.control.for_thread(thread);
        if let Ok(slot)=resident.lease.try_lock() {
            control.set_cancel(slot.as_ref().and_then(|lease|lease.cancel_handle()));
            control.set_lease_session(slot.as_ref().map(|lease|lease.session_id().to_string()));
        }
        resident.parked_at_ms=now_millis();
        self.resident.insert(thread.to_string(),resident);
        self.evict_beyond_the_cap();
    }



    /// Retire the least recently spoken PARKED desk while there are more than the cap.
    ///
    /// Never the one in the chair — it is not in this map — and never one mid-turn, which
    /// cannot happen: a turn runs on the chair's lease, and parking only ever happens at a
    /// turn boundary (`select_connection`, reached with `turn_in_progress == false`).
    fn evict_beyond_the_cap(&mut self) {
        while self.resident.len() > MAX_RESIDENT_FRONT_DESKS {
            let Some(oldest) = self.resident.iter()
                .filter(|(thread, resident)| self.control.for_thread(thread).active_turn().is_none()
                    && std::sync::Arc::strong_count(&resident.lease)==1)
                .min_by_key(|(_, resident)| resident.parked_at_ms)
                .map(|(thread, _)| thread.clone()) else { break };
            // Dropping the `Resident` drops its `Box<dyn Cognition>`, whose `Drop` kills the
            // child. That is deliberate and it is the old behavior for this one thread.
            self.resident.remove(&oldest);
            let control=self.control.for_thread(&oldest);
            control.set_cancel(None);
            control.set_lease_session(None);
        }
    }

    pub fn ready_a_spare_front_desk(&mut self,entity:&EntityId)->SpareReady {
        let plan=match self.begin_a_spare(entity) {Ok(plan)=>plan,Err(verdict)=>return verdict};
        match self.spawn_a_spare_here(&plan) {Ok(lease)=>self.install_the_spare(plan,lease),Err(error)=>self.abandon_a_spare(plan,error.to_string())}
    }

    // -----------------------------------------------------------------------------------
    // THE FIVE STEPS, SPLIT SO THE TWO SLOW ONES CAN RUN WITH THE MUTEX DOWN
    // -----------------------------------------------------------------------------------
    //
    // Steps 1, 3 and 5 are ledger and configuration work and take microseconds. Steps 2 and 4
    // — the child's spawn and its priming turn — are SECONDS, and they touch nothing but the
    // detached lease. Until 2026-09-19 all five ran under one lock, and that is where the
    // CEO's whole wait was: `create_thread_in` starts this work microseconds before the window
    // issues eight more bridge calls, and every one of them queued behind the priming turn.
    // Measured on the real window that day — `a window command waited 18314 ms for the spine`
    // on a debug build, the turn's own header starting its clock 18.6 s after the keystroke;
    // on the shipped candidate the same shape is Ray's 5378 ms spare and ~6 s of pre-turn
    // overhead.

    /// **Step 1 (locked): decide, and reserve the slot.** Everything that reads or writes the
    /// spine happens here, so that steps 2 and 4 can run against nothing but the plan.
    fn begin_a_spare(&mut self, entity: &EntityId) -> Result<SparePlan, SpareReady> {
        if self.is_turn_in_progress() {
            return Err(SpareReady::TurnInProgress);
        }
        if !self.registry.contains(entity) {
            return Err(SpareReady::NotReady(SpineError::UnknownEntity(entity.to_string()).to_string()));
        }
        if self.spare.as_ref().is_some_and(|spare| &spare.entity == entity) {
            return Err(SpareReady::AlreadyReady);
        }
        // **ONE ATTEMPT AT A TIME, and this is the flag that makes the unlocked road safe.**
        // With the mutex down between the steps, a second ask can arrive and find no spare
        // standing — because the first one has not installed it yet. Two spawns would then
        // race for one slot and the loser's child would be killed after it had been paid for.
        if self.spare_in_flight {
            return Err(SpareReady::AlreadyReady);
        }
        // Anything else standing here is for the wrong entity, or was un-primed by a change
        // the whole app made (`unprime_every_front_desk`). Either way it is retired BEFORE the
        // replacement is spawned, so two spare children never exist at once.
        self.discard_spare();
        if self.control.stop_claim().is_some() {
            return Err(SpareReady::NotReady("a stop is claimed".into()));
        }
        if self.lease_factory.is_none() {
            return Err(SpareReady::NotReady(SpineError::NoLeaseFactory.to_string()));
        }
        // **THE ID IS MINTED HERE AND WRITTEN NOWHERE** (UX §3.3). The binding built around it
        // is for SPAWN SCOPING only — `EngineProfile::scope_to` and nothing else — and it is
        // deliberately unable to pass `Ledger::verify_binding`: the thread does not exist, so
        // every scoped read or write through it fails closed, which is the property that makes
        // it safe to hold a binding for a conversation that has not started.
        let reserved = crate::ledger::Ledger::reserve_thread_id();
        let binding = crate::entity::ThreadBinding::new(
            crate::entity::PersonId::default_ceo(),
            entity.clone(),
            &reserved,
            self.ledger.take_revision(),
        );
        self.spare_in_flight = true;
        Ok(SparePlan {
            entity: entity.clone(),
            binding,
            control: self.control.for_thread(&reserved),
            reserved,
            factory: self.lease_factory.as_ref().unwrap().clone(),
            started: std::time::Instant::now(),
        })
    }

    /// Synchronous headless spelling of the same factory operation.
    fn spawn_a_spare_here(&self, plan: &SparePlan) -> Result<Box<dyn Cognition>, crate::cognition::CognitionError> {
        let factory = self.lease_factory.as_ref().ok_or(crate::cognition::CognitionError::Protocol(
            SpineError::NoLeaseFactory.to_string(),
        ))?;
        factory.spawn_scoped(&plan.binding, &plan.control)
    }



    fn install_the_spare(&mut self,plan:SparePlan,lease:Box<dyn Cognition>)->SpareReady {
        self.spare_in_flight=false;
        // A waiting spare is already live and receives the same Settings control.
        plan.control.set_cancel(lease.cancel_handle());
        plan.control.set_lease_session(Some(lease.session_id().to_string()));
        self.spare=Some(SpareFrontDesk{lease,entity:plan.entity,reserved_thread:plan.reserved});
        SpareReady::Ready{millis:plan.started.elapsed().as_millis() as u64}
    }

    fn abandon_a_spare(&mut self,_plan:SparePlan,why:String)->SpareReady {
        self.spare_in_flight=false;
        SpareReady::NotReady(why)
    }

    /// Retire the spare: its `Box<dyn Cognition>` is dropped, whose `Drop` kills the child.
    ///
    /// **This is the answer to "what happens to the spare on an entity switch", and it is
    /// discard-and-re-prime rather than one-spare-per-entity.** The cost decision follows the
    /// measurement in `docs/verification/first-words-2026-09-18-primed.md` §2: an idle primed
    /// child is not free, and a pool sized by the number of companies the CEO has registered
    /// would hold that many children open for a surface that can only ever consume one of them
    /// next. The thing thrown away is one priming turn, and it is thrown away only when he has
    /// moved to a different company — at which point the spare was primed with the wrong
    /// company's material and could not have been adopted anyway.
    fn discard_spare(&mut self) {
        if let Some(spare)=self.spare.take() {
            let control=self.control.for_thread(&spare.reserved_thread);
            control.set_cancel(None);
            control.set_lease_session(None);
        }
    }

    /// Is a PRIMED spare standing ready for this entity? **Test scaffolding and the shell's
    /// own log line** — nothing in the product branches on it.
    pub fn spare_front_desk_is_ready_for(&self, entity: &EntityId) -> bool {
        self.spare.as_ref().is_some_and(|spare| &spare.entity == entity)
    }

    /// The thread id the spare is scoped to, reserved and unspent. **Test scaffolding** — it is
    /// how a test proves the id the child was spawned under is the id the thread ends up with.
    #[doc(hidden)]
    pub fn spare_front_desk_reserved_thread(&self) -> Option<&str> {
        self.spare.as_ref().map(|spare| spare.reserved_thread.as_str())
    }

    pub fn resident_front_desks(&self)->usize {self.resident.len()}

    pub fn front_desk_is_resident(&self,thread:&str)->bool {
        self.control.for_thread(thread).active_turn().is_some() || self.resident.get(thread).is_some_and(|desk|desk.lease.try_lock().is_ok_and(|slot|slot.is_some()))
    }

    pub fn set_turn_control(&mut self,control:impl Into<TurnControl>) {
        self.control=control.into().for_thread("");
        if let Some(last) = self.ledger.turns().iter().filter_map(|turn| turn.intake_id).max() {
            self.control.advance_input_order(last);
        }
        for (thread,resident) in &self.resident {
            let control=self.control.for_thread(thread);
            if let Ok(slot)=resident.lease.try_lock() {
                control.set_cancel(slot.as_ref().and_then(|lease|lease.cancel_handle()));
                control.set_lease_session(slot.as_ref().map(|lease|lease.session_id().to_string()));
            }
        }
        self.publish_read_metadata();
    }

    pub fn turn_control(&self)->ConversationControl {self.control.for_thread(self.active_thread().unwrap_or(""))}

    /// Replace the clock an overload retry waits on. Tests only; the app keeps the real one.
    pub fn set_retry_clock(&mut self, clock: std::sync::Arc<dyn crate::upstream::RetryClock>) {
        self.retry_clock = clock;
    }

    pub fn has_lease(&self)->bool {self.resident.values().any(|desk|desk.lease.try_lock().is_ok_and(|slot|slot.is_some())) || self.is_turn_in_progress()}

    /// Called with the shell's idle lock held and worker observations checked.
    pub fn retire_idle_lease(&mut self) -> Result<(), String> {
        if self.control.has_active_turns() { return Err("A turn is still running.".into()); }
        self.clear_lease(); Ok(())
    }

    pub fn clear_memory_wiring(&mut self) {
        self.loro_compiler = None;
        // THE LOOKUP GOES WITH THE COMPILER. It was resolved from the same corpus, and a
        // lookup left attached to a corpus the app has stopped reading would be pointed at
        // an evidence zone nothing else in the build still believes in.
        self.evidence_lookup = None;
        self.correction_desk = None;
    }

    fn clear_lease(&mut self) {
        if let Some(thread)=self.active_thread().map(str::to_string) {
            self.resident.remove(&thread);
            let control=self.control.for_thread(&thread);
            control.set_cancel(None);control.set_lease_session(None);
        }
    }

    /// Tell this spine where the central folder is, so the company layer reaches the model.
    ///
    /// It clears `lease_primed`. The priming turn is what carries the company layer; a lease already
    /// primed without it would keep serving the conversation with no company material until
    /// the next thread switch, and nothing would say so. Un-priming re-issues it at the next
    /// turn, which is the cheap and honest answer.
    ///
    /// This never creates the folder and never checks it here. `company.rs` reads it at prime
    /// time and reports what it found; a path stored at boot and validated at boot would be a
    /// path that is right once, and the four dangling `corpus.*` symlinks on this machine are
    /// what that costs (`richos-central-folder-2026-09-06.md` §1.3).
    pub fn set_central_root(&mut self, root: std::path::PathBuf) {
        self.central_root = Some(root);
        self.publish_read_metadata();
    }

    /// What the company layer looks like for one thread, right now.
    ///
    /// `None` means no central folder has been set — which is NOT the same as a central folder
    /// with nothing in it ([`crate::company::CompanyLayer::NoHome`]), and the two must stay
    /// apart: one is "nobody told the app where to look", the other is "the app looked and the
    /// company is not there". Collapsing them would report an unconfigured install as an
    /// un-interviewed one.
    pub fn company_layer(&self, binding: &ThreadBinding) -> Option<crate::company::CompanyLayer> {
        let root = self.central_root.as_ref()?;
        Some(crate::company::CompanyLayer::read(root, binding.entity_id()))
    }

    /// Where the declination record lives — `onboarding.rs`'s single durable fact.
    ///
    /// Separate from [`Spine::set_central_root`] because the two answer different questions and
    /// live in different places: the company layer is the CEO's material in HIS folder, the
    /// declination is the app's own record in the app's own configuration directory. Folding
    /// them into one setter would put a RichOS bookkeeping file inside a folder the CEO owns.
    pub fn set_onboarding_record(&mut self, path: std::path::PathBuf) {
        self.onboarding_record = Some(path);
        self.publish_read_metadata();
    }

    /// The declination record as it stands on disk right now.
    ///
    /// Re-read at every prime rather than cached, so a declination recorded during a
    /// conversation takes effect at the next prime instead of at the next launch.
    fn onboarding_record(&self, binding: &ThreadBinding) -> crate::onboarding::OnboardingRecord {
        match self.onboarding_record.as_ref() {
            Some(p) => crate::onboarding::OnboardingRecord::load(p).for_entity(binding.entity_id()),
            None => crate::onboarding::OnboardingRecord::default(),
        }
    }



    fn company_block(&self, binding: &ThreadBinding) -> Option<String> {
        let layer = self.company_layer(binding);
        let onboarding =
            crate::onboarding::priming_block(binding.entity_id(), layer.as_ref(), &self.onboarding_record(binding));
        match self.registry.get(binding.entity_id()) {
            Some(entity)=>Some(serde_json::json!({"entity_id":binding.entity_id(),"company_name":entity.display_name,
                "setup":onboarding.and_then(|text|serde_json::from_str::<serde_json::Value>(&text).ok())}).to_string()),
            None=>onboarding,
        }
    }

    /// One line per company for the boot log — what this launch will actually do about
    /// onboarding, said out loud because the failure being fixed was silence.
    pub fn describe_onboarding(&self, binding: &ThreadBinding) -> String {
        let layer = self.company_layer(binding);
        crate::onboarding::describe(binding.entity_id(), layer.as_ref(), &self.onboarding_record(binding))
    }

    /// WHERE THIS INSTALL STANDS ON ONBOARDING, for the WINDOW rather than for the log.
    ///
    /// [`Spine::describe_onboarding`] answers the same question and answers it as a sentence
    /// for a terminal. This returns the state itself, because the surface that shows the CEO
    /// the offer has to branch on it, and a surface that parsed the log line would be a second
    /// opinion about a fact that already has one.
    ///
    /// It is DERIVED at every call, from the same two facts on disk the priming block is
    /// derived from ([`crate::onboarding::state`]), so the notice on screen and the block in
    /// the priming turn cannot disagree. Nothing is cached, and there is deliberately no
    /// "the notice was shown" anywhere in this crate — `onboarding.rs`'s module doc gives the
    /// reason and [`OnboardingRecord`] has a test whose only job is to keep it that way.
    ///
    /// [`OnboardingRecord`]: crate::onboarding::OnboardingRecord
    pub fn onboarding_state(&self, binding: &ThreadBinding) -> crate::onboarding::OnboardingState {
        let layer = self.company_layer(binding);
        crate::onboarding::state(layer.as_ref(), &self.onboarding_record(binding))
    }

    /// HE WAS ASKED AND SAID NOT NOW — the write half of the one fact the company file
    /// cannot hold.
    ///
    /// `record_declination` existed in `onboarding.rs` from the day that module landed and
    /// had NO CALLER anywhere in the product, which made [`OnboardingState::Declined`] and
    /// `DECLINED_BLOCK` unreachable outside their own tests: a CEO who said "not now" in
    /// conversation was re-offered the interview at the next prime, forever, which is exactly
    /// the nag the onboarding document's M5 named and the memory question had already paid
    /// for once. This is that caller.
    ///
    /// **`Err` when nobody has said where the record lives.** Not a silent success and not a
    /// default path: a declination that goes nowhere is worse than one that was refused, since
    /// the refusal can be reported to him and the silent drop cannot. The same "no silent
    /// default" the corpus provisioner enforces at the only door that can create a corpus.
    ///
    /// [`OnboardingState::Declined`]: crate::onboarding::OnboardingState::Declined
    pub fn record_onboarding_declination(&mut self, now_millis: u64) -> Result<(), crate::doctrine::DoctrineError> {
        let path = self.onboarding_record.as_ref().ok_or_else(|| crate::doctrine::DoctrineError::Unwritable {
            path: "<no onboarding record configured>".into(),
            why: "nobody told this spine where the declination record lives, so there is nowhere to write".into(),
        })?;
        let binding = self.active_binding().ok_or_else(|| crate::doctrine::DoctrineError::Unwritable {
            path: path.display().to_string(), why: "no company is selected; no answer was recorded".into(),
        })?;
        crate::onboarding::OnboardingRecord::record_declination_for_entity(path, binding.entity_id(), now_millis)?;
        Ok(())
    }

    /// Run after restoring the active company (or its first explicit selection). Legacy
    /// records cannot identify a company, so this is a one-time, explicit compatibility step.
    pub fn migrate_legacy_onboarding_declination(&mut self) -> Result<bool, crate::doctrine::DoctrineError> {
        let (Some(path), Some(binding)) = (self.onboarding_record.as_ref(), self.active_binding()) else { return Ok(false); };
        let changed = crate::onboarding::OnboardingRecord::migrate_legacy(path, binding.entity_id())?;
        Ok(changed)
    }

    /// Only the app's bound company and configured paths become tool authority.
    pub fn onboarding_tool_scope(&self, binding: &ThreadBinding) -> Option<crate::onboarding_tools::OnboardingToolScope> {
        Some(crate::onboarding_tools::OnboardingToolScope::new(
            binding.entity_id(), self.central_root.as_ref()?, self.onboarding_record.as_ref()?,
        ))
    }

    /// Attach the rotation/recovery seam. Without one, the spine can still run its
    /// (single) attached lease indefinitely, but a context watermark, an explicit
    /// rotation request, or a mid-turn crash all degrade to their pre-P1.4 behavior
    /// (watermark: never fires since rotation can't proceed; crash: surfaces the error
    /// honestly instead of silently retrying against nothing).
    pub fn set_lease_factory(&mut self, factory: Box<dyn LeaseFactory>) {
        self.lease_factory = Some(std::sync::Arc::from(factory));
    }

    pub fn has_lease_factory(&self) -> bool {
        self.lease_factory.is_some()
    }

    /// Attach the optional Tier-C seam (continuity §2.3/§4). See `LoroContextCompiler`'s
    /// doc for the degrade-gracefully contract when this is never called, and
    /// `crate::loro::CliContextCompiler` for the shipped implementation.
    pub fn set_loro_context_compiler(&mut self, compiler: Box<dyn LoroContextCompiler>) {
        self.loro_compiler = Some(compiler);
    }

    pub fn has_loro_context_compiler(&self) -> bool {
        self.loro_compiler.is_some()
    }

    /// Attach the optional Tier-E seam — the evidence lookup over the CEO's own files
    /// (the 2026-09-17 evidence-retrieval ruling §1/§4). See `EvidenceLookup`'s doc for the
    /// degrade-gracefully contract when this is never called, and
    /// `crate::evidence::CliEvidenceLookup` for the shipped implementation.
    pub fn set_evidence_lookup(&mut self, lookup: Box<dyn EvidenceLookup>) {
        self.evidence_lookup = Some(lookup);
    }

    pub fn has_evidence_lookup(&self) -> bool {
        self.evidence_lookup.is_some()
    }

    /// Record ONE completed, CEO-facing action that the app took outside a turn.
    ///
    /// The shell needs this because `ledger()` is deliberately `&Ledger`: the ledger is
    /// append-only evidence and handing out `&mut` would be one more place a caller could
    /// write an event the spine knows nothing about. The concrete need is a confirmed loro
    /// correction — "Rich changed what the company believes" is exactly the class of fact
    /// the action-ledger digest exists to stop a successor denying from absent memory
    /// (continuity §2.1 #6, `reprime.rs`'s identity assertion: *"an entry PRESENT is proof
    /// the action happened"*).
    ///
    /// `turn_id: None` is first-class here, not a gap: a correction the CEO confirms in a
    /// side panel belongs to the THREAD, not to any turn, the same way re-prime machinery
    /// does (§1.4 G4).
    pub fn record_ceo_action(&mut self, kind: &str, detail: &str) -> Result<String, SpineError> {
        Ok(self.ledger.record_action_with(
            None,
            kind,
            detail,
            ActionVisibility::CeoFacing,
            ActionStatus::Completed,
        )?)
    }







    pub fn context_usage(&self)->Option<ContextUsage> {self.resident.get(self.active_thread()?)?.context_usage}





























    // ---- threads -----------------------------------------------------------

    /// Replace the entity registry (tests, or a future CEO-configured registry).
    pub fn set_entity_registry(&mut self, registry: EntityRegistry) {
        self.registry = registry;
        self.publish_read_metadata();
    }

    pub fn entity_registry(&self) -> &EntityRegistry {
        &self.registry
    }

    /// Create a thread with its IMMUTABLE entity home. The entity must be registered —
    /// an unknown one is refused, never invented (ECS §3.3).
    ///
    /// **AND THIS IS WHERE A SPARE FRONT DESK IS ADOPTED** (see [`SpareFrontDesk`]). If a
    /// primed spare is standing ready for this entity, the thread takes ITS reserved id and the
    /// spare is filed as this thread's resident desk, already primed. The CEO's first message
    /// then finds `select_connection` resuming a primed desk instead of spawning and priming one,
    /// which is the whole of the saving.
    ///
    /// **The record is identical either way.** Same event, same immutable entity home, same
    /// fresh binding revision, same auto-activation. Only the id was decided earlier, and only
    /// so that the child already running was scoped to THIS conversation rather than to a name
    /// it would have had to borrow.
    ///
    /// **A spare for another entity is left standing.** It is not this thread's and adopting it
    /// would be the cross-entity leak its own doc names; it is retired at the next
    /// [`Spine::ready_a_spare_front_desk`] for a different entity, not here, because "he
    /// started a thread in company A" is not evidence about company B.
    pub fn create_thread(&mut self, title: &str, entity_id: &EntityId) -> Result<String, SpineError> {
        if !self.registry.contains(entity_id) {
            return Err(SpineError::UnknownEntity(entity_id.to_string()));
        }
        let adopted = match &self.spare {
            Some(spare) if &spare.entity == entity_id => Some(spare.reserved_thread.clone()),
            _ => None,
        };
        let id = match adopted {
            // **THE ID IS SPENT BEFORE THE DESK IS FILED, AND THE ORDER MATTERS.** If the
            // append fails the spare is still standing, unspent and re-usable by the next
            // attempt; if it succeeded, the desk is filed against a thread that now exists.
            // Filing first would leave a resident desk keyed to a thread nobody can read.
            Some(reserved) => {
                let id = self.ledger.create_thread_with_reserved_id(&reserved, title, entity_id)?;
                let spare = self.spare.take().expect("checked directly above");
                self.restore_front_desk(&id, Resident {
                    lease: std::sync::Arc::new(std::sync::Mutex::new(Some(spare.lease))),
                    upstream_budget:crate::upstream::RetryBudget::new(),
                    context_usage: None,
                    parked_at_ms: now_millis(),
                });
                // The common restore path registers live controls and enforces the cap.
                id
            }
            None => self.ledger.create_thread(title, entity_id)?,
        };
        if self.active.is_none() {
            self.activate(&id)?;
        }
        Ok(id)
    }

    /// The ACTIVE-CONTEXT TRANSACTION (ECS §11.3: *"Active-context switching is a
    /// transaction, so entity and thread cannot disagree"*).
    ///
    /// Re-reads the entity from the durable record and issues a NEW binding revision, so
    /// every command captured under the previous context is now stale and will be refused
    /// (ECS §3.4). Entity and thread move together or not at all: an unbound thread cannot
    /// be activated, so there is no state in which a thread is active without an entity.
    fn activate(&mut self, thread_id: &str) -> Result<ThreadBinding, SpineError> {
        let binding = self.ledger.rebind_at_new_revision(thread_id)?;
        self.active = Some(binding.clone());
        self.publish_read_metadata();
        Ok(binding)
    }

    /// The active context's binding, or `NoActiveThread`. NEVER falls back to "the first
    /// thread" — that would be picking an entity for the CEO, which UX §21 forbids
    /// ("Never default to the last entity") and which is the exact failure mode §22 calls
    /// out as unfakeable.
    pub fn ensure_active_thread(&mut self) -> Result<ThreadBinding, SpineError> {
        self.active.clone().ok_or(SpineError::NoActiveThread)
    }

    /// Ensure a thread is active WITHIN A NAMED ENTITY. This is the honest replacement for
    /// the old zero-argument auto-create: the entity is supplied by the caller (in the
    /// dogfood shell, from deterministic repository-root resolution — ECS §3.3), never
    /// inferred here. If a thread is already active in that entity it is returned; if one
    /// exists in that entity it is activated (most recent first); otherwise a new
    /// "Onboarding" thread is created bound to it.
    ///
    /// A thread active in a DIFFERENT entity is not reused — that would be a silent
    /// cross-entity switch.
    pub fn ensure_active_thread_in(&mut self, entity_id: &EntityId) -> Result<ThreadBinding, SpineError> {
        if !self.registry.contains(entity_id) {
            return Err(SpineError::UnknownEntity(entity_id.to_string()));
        }
        if let Some(active) = &self.active {
            if active.entity_id() == entity_id {
                return Ok(active.clone());
            }
        }
        let existing = self
            .ledger
            .threads()
            .iter()
            .filter(|t| t.entity_id() == Some(entity_id))
            .map(|t| (t.id.clone(), t.created_at))
            .max_by_key(|(_, created)| *created)
            .map(|(id, _)| id);
        match existing {
            Some(id) => self.activate(&id),
            None => {
                // "Onboarding" (the CEO, 2026-10-13: the first conversation of the first
                // company "should be named "Onboarding""; it was "Running" per the UX direction
                // doc §2.1): the pinned default thread's real title, not a placeholder the UI
                // has to cosmetically relabel.
                let id = self.create_thread("Onboarding", entity_id)?;
                self.activate(&id)
            }
        }
    }

    /// Switch the active topic view. Continuity holds across the switch because every
    /// thread folds over the SAME shared ledger (and later, shared loro) — WITHIN its
    /// entity. Switching to an unbound legacy thread fails closed.
    pub fn switch_thread(&mut self, thread_id: &str) -> Result<(), SpineError> {
        self.activate(thread_id)?;
        Ok(())
    }

    pub fn active_thread(&self) -> Option<&str> {
        self.active.as_ref().map(|b| b.thread_id())
    }

    /// The active entity area — the scope every read and write is currently under.
    pub fn active_entity(&self) -> Option<&EntityId> {
        self.active.as_ref().map(|b| b.entity_id())
    }

    /// The full active-context binding (person + entity + thread + revision).
    pub fn active_binding(&self) -> Option<&ThreadBinding> {
        self.active.as_ref()
    }

    /// ECS §3.4's FENCING CHECK for an outbound command: *"The command is rejected as
    /// `stale_binding` if the active-context binding revision ... has advanced ... an old
    /// Rich instance is never allowed to send, dispatch or write into a newly switched
    /// entity/thread. This is a fencing token, not a UI hint."*
    ///
    /// A caller that captured a binding at turn start passes it back here before acting on
    /// the outside world. A binding for a different thread or entity is `ScopeMismatch`; a
    /// binding older than the current active context is `StaleBinding`.
    ///
    /// **Honest scope note.** Slice 1's spine is synchronous and single-threaded (the
    /// shell serializes every call behind one `Mutex<Spine>`), so there is no concurrent
    /// in-process holder that can actually go stale today. This is the seam the Tauri
    /// command layer and any future async writer must call, and it is exercised by test
    /// rather than merely declared — but it is defense in depth, not a bug being fixed.
    pub fn verify_active_binding(&self, binding: &ThreadBinding) -> Result<(), SpineError> {
        let active = self.active.as_ref().ok_or(SpineError::NoActiveThread)?;
        if binding.thread_id() != active.thread_id() || binding.entity_id() != active.entity_id() {
            return Err(LedgerError::ScopeMismatch {
                thread_id: active.thread_id().to_string(),
                home: format!("{}/{}", active.entity_id(), active.thread_id()),
                presented: format!("{}/{}", binding.entity_id(), binding.thread_id()),
            }
            .into());
        }
        if binding.binding_revision() < active.binding_revision() {
            return Err(LedgerError::StaleBinding {
                thread_id: active.thread_id().to_string(),
                presented: binding.binding_revision(),
                current: active.binding_revision(),
            }
            .into());
        }
        Ok(())
    }

    pub fn threads(&self) -> Vec<ThreadSummary> {
        summaries(&self.ledger)
    }

    /// Scoped read. An unbound legacy thread returns `UnboundThread` rather than an empty
    /// conversation — see `Ledger::messages`.
    pub fn messages(&self, thread_id: &str) -> Result<Vec<Message>, SpineError> {
        Ok(self.ledger.messages(thread_id)?)
    }

    /// One thread's TYPED TIMELINE (UX brief §12) — the ledger and the machinery journal
    /// folded into one scoped projection.
    ///
    /// The read path, and the only assembly point: without it a consumer would have to
    /// re-derive the binding and re-read the journal itself, and every consumer that did
    /// that would be one more place the scope guard could be forgotten. Fails closed on an
    /// unbound thread exactly like `messages()`.
    ///
    /// A spine with no journal attached still returns a timeline — one with the
    /// conversation and no activity rows. That is the honest degrade: machinery retention
    /// is a separate store that may legitimately be absent (`set_machinery_journal` is
    /// optional), and an empty activity lane is not a claim that no tools ran.
    ///
    /// The same posture applies to the worker stream: with
    /// [`WorkerEventsSource::Disabled`] (the default) a `Task` call projects as an
    /// ordinary activity row, which is what the app did until 2026-08-29. See
    /// [`Spine::set_worker_events`] for what wiring it can and cannot promise.
    pub fn timeline(&self, thread_id: &str) -> Result<Timeline, SpineError> {
        let binding = self.ledger.thread_binding(thread_id)?;
        let machinery =
            self.machinery_journal.as_ref().map(|j| j.read_thread(thread_id)).unwrap_or_default();
        // The worker stream is read HERE rather than by the caller, for the same reason the
        // binding and the journal are: every consumer that assembled these itself would be
        // one more place the scope guard could be forgotten.
        let workers = self.worker_events.read(self.control.for_thread(thread_id).lease_session().as_deref());
        Ok(Timeline::project_with_workers(&self.ledger, &binding, &machinery, &workers)?)
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    // ---- the turn flow -----------------------------------------------------

    /// Accept a CEO prompt. CRASH-SAFETY: the prompt is journaled + fsync'd `received`
    /// BEFORE anything else. QUEUE-NOT-INTERRUPT: if a turn is in flight it is queued,
    /// never delivered as an interrupt. Returns the turn id (persisted regardless).
    /// SCOPE FIRST, then persist-before-send. The binding is captured before the durable
    /// write, not after, because ECS §3.4 says the store rejects an unscoped event —
    /// journaling first and scoping second would make the crash window contain exactly the
    /// record the model forbids. A turn whose entity cannot be resolved is therefore
    /// REFUSED, loudly:
    ///
    ///   - no active context           -> `SpineError::NoActiveThread`
    ///   - active thread is unbound    -> `LedgerError::UnboundThread`
    ///   - binding contradicts the log -> `LedgerError::ScopeMismatch`
    ///
    /// Nothing is lost by refusing. The caller still holds the CEO's text and the send is
    /// blocked with an explanation, which is precisely UX §21's "Entity binding failure"
    /// behavior: *"Block send. State that Rich cannot safely determine which entity the
    /// work belongs to. Require an explicit entity choice."* The alternative — persisting
    /// an unscoped turn and sorting it out later — is how a message ends up rendered in
    /// the wrong entity, which is a privacy incident rather than an inconvenience.
    /// **His words, on the thread he typed them into** — the CEO's *"the CEO could open and
    /// run multiple things in parallel."*
    ///
    /// `send_message` used to refuse outright when the named thread was not the single
    /// active one: *"The conversation changed before your message was sent. Open the
    /// original conversation to try again."* He typed a sentence and the app threw it back.
    /// Now the named thread is made active and the message is his message on that thread —
    /// **answered, never refused.** If a turn is already running, [`Spine::submit_prompt`]
    /// queues it with ITS OWN binding and the boundary delivers it there (the `Queued`
    /// record has carried its binding since before this existed, for exactly this reason).
    ///
    /// **Activating mid-turn is safe and was already possible**: he can switch threads while
    /// Rich is working, `deliver` holds the binding it was handed rather than re-reading the
    /// active one, and the deferred proactive emits carry their own. What is new is only
    /// that the front desk he switches away from stays alive (`park_current_front_desk`).
    ///
    /// Each thread has its own provider and scoped controls. A running reply only
    /// queues later requests addressed to that same thread.
    pub fn submit_prompt_to(&mut self, thread_id: &str, text: &str, source: Source) -> Result<String, SpineError> {
        if self.active_thread() != Some(thread_id) {
            self.activate(thread_id)?;
        }
        self.submit_prompt(text, source)
    }

    /// **Keep the mouth each prompt came through on its ledger record** (operator back-end
    /// spec r3 (s); the operator-client record's §7 item 3). Turned on by the shell only on an
    /// operator install, before the first turn, so the host can tell a phone assignment from
    /// one given at the desk without the walk handing it in. Never turned on anywhere else.
    pub fn keep_intake_channel(&mut self, keep: bool) {
        self.keep_intake_channel = keep;
    }

    pub fn submit_prompt(&mut self, text: &str, source: Source) -> Result<String, SpineError> {
        self.submit_prompt_inner(text, source, None, false)
    }

    /// **Words the app sends Rich because he pressed a button**, into `thread_id` (product hunt
    /// 2026-10-15, finding 14: "Start the questions" posted "Let's do the twenty minutes of
    /// questions about my business." as his own message). Rich receives them as an ordinary
    /// turn; the record marks them `pressed`, so no surface shows them in his name, and they
    /// are not read as a correction he made.
    pub fn submit_pressed_prompt_to(&mut self, thread_id: &str, text: &str) -> Result<String, SpineError> {
        if self.active_thread() != Some(thread_id) {
            self.activate(thread_id)?;
        }
        self.submit_prompt_inner(text, Source::Text, None, true)
    }

    /// A SPOKEN prompt, carrying what the capture path measured about Rich's own audible
    /// window while the audio was being recorded.
    ///
    /// **It exists because `source: Source::Jam` was the whole of a spoken turn's provenance,
    /// and that is not enough to tell an echo-born turn from a genuine one after the fact**
    /// (the voice pipeline's own handoff, richos `c712ccd5`). Candidate .5 left a bubble in the
    /// CEO's thread that was Rich's own counting, and the ledger cannot say so today.
    ///
    /// Separate from [`Self::submit_prompt`] rather than an extra argument on it, so that a
    /// typed turn writes `None` by CONSTRUCTION. See `Event::PromptReceived::rich_audible`:
    /// `None` is "not recorded" and is never to be read as "no".
    pub fn submit_prompt_spoken(
        &mut self,
        text: &str,
        source: Source,
        rich_audible: bool,
    ) -> Result<String, SpineError> {
        self.submit_prompt_inner(text, source, Some(rich_audible), false)
    }

    fn submit_prompt_inner(
        &mut self,
        text: &str,
        source: Source,
        rich_audible: Option<bool>,
        pressed: bool,
    ) -> Result<String, SpineError> {
        let binding = self.ensure_active_thread()?;
        let intake_id = self.control.take_input_order();
        let turn_id = if pressed {
            self.accept_pressed_prompt(&binding, text, intake_id)?
        } else {
            self.accept_prompt(&binding, text, source, rich_audible, intake_id, DESK_CHANNEL)?
        };

        self.deliver(&turn_id, &binding, text, true)?;
        self.after_turn_boundary(&binding)?;
        Ok(turn_id)
    }

    /// **The one durable write of a sentence of his**, with the mouth it came through when
    /// this install keeps it (operator back-end spec r3 (s); [`Self::keep_intake_channel`]).
    /// With keeping off, this is exactly the write each road made before, byte for byte: the
    /// channel is never written on a product install.
    fn record_prompt(
        &mut self,
        binding: &ThreadBinding,
        text: &str,
        source: Source,
        rich_audible: Option<bool>,
        intake_id: Option<u64>,
        channel: &str,
    ) -> Result<String, SpineError> {
        if self.keep_intake_channel {
            return Ok(self.ledger.record_prompt_received_via(binding, text, source, intake_id, rich_audible, channel)?);
        }
        Ok(self.ledger.record_prompt_received_desktop(binding, text, source, intake_id, rich_audible, false)?)
    }

    /// **ACCEPTING one CEO utterance: everything that happens between his sentence arriving
    /// and the decision to queue or deliver it.** Persist, announce, and look at it for the
    /// three correction families — in that order, and never in any other.
    ///
    /// # Why this is a function now, and what it stops from happening
    ///
    /// It was the body of [`Self::submit_prompt_inner`], whose own comment calls that
    /// function *"the ONE function every CEO utterance passes through"*. That was true, and
    /// then it stopped being true in one place: `drain_intake` files an intake-borne message
    /// straight into the ledger and pushes it on the queue, so it emits the `queued` pair and
    /// stages NOTHING. Today that only costs the phone. The moment a typed message can be
    /// deferred through the same log (`IntakeRecord::Desk`, the CEO's §55), a correction he
    /// typed during a front-desk prime would be dropped on the floor with no trace — the
    /// flywheel's trigger silently not firing because of WHICH ROAD his words took.
    ///
    /// So the claim is restored as a function rather than as a sentence: every path that
    /// accepts a CEO utterance calls this, and a path that does not, does not compile into
    /// the same shape.
    ///
    /// `intake_id` is `Some` when the utterance came off the intake log. It rides into the
    /// ledger so the de-duplication key survives a crash between the ledger write and
    /// `mark_drained` — that window is the whole reason `record_prompt_received_from_intake`
    /// exists, and it is at-least-once into a ledger that can spot the replay.
    fn accept_prompt(
        &mut self,
        binding: &ThreadBinding,
        text: &str,
        source: Source,
        rich_audible: Option<bool>,
        intake_id: Option<u64>,
        channel: &str,
    ) -> Result<String, SpineError> {
        // (1) persist-before-send, under a verified scope — the message is durable before
        //     any risk, and it is never durable without an entity.
        let turn_id = self.record_prompt(binding, text, source, rich_audible, intake_id, channel)?;

        // ADDITIVE (§13): the turn is durably `received`, so its first authoritative state
        // transition is emittable — §11's `queued`, the state whose timeline treatment is
        // "CEO bubble plus scaffold". Emitted for BOTH branches below, so a prompt that is
        // delivered immediately still shows queued -> working rather than appearing
        // mid-flight. Nothing on `stream.rs` changes here.
        self.emit_live(self.turn_status_event(binding, &turn_id, TurnStatus::Queued, None));
        // **AND WHAT HE ACTUALLY SAID** — `esc-20260919T003541Z-6885f74b`. Every other event in
        // this family is about Rich; until today the CEO's own sentence reached a surface only
        // by being PROJECTED, which was enough while the one surface was a webview that had
        // already drawn it itself. The phone subscribes to this family and drew nothing, so a
        // message typed on the Mac did not appear there until something asked for the rows
        // again. Emitted HERE, beside the queued status, because this is the one function every
        // CEO utterance passes through and the message is durable one statement above.
        self.emit_live(self.ceo_message_event(binding, &turn_id));
        self.emit_live(self.thread_summary_event(binding, &turn_id, ThreadStatus::Queued));

        // (1b) THE FLYWHEEL'S AUTOMATIC TRIGGER. Here, and not in the shell, because this
        //      is the ONE function every CEO utterance passes through — voice mode calls
        //      it with `Source::Jam` (`src-tauri/src/main.rs`) and the composer calls it
        //      with `Source::Text`, so a correction is caught by SPEAKING it with no
        //      command typed. Placed before the queue/deliver branch so a correction
        //      spoken while Rich is working is recorded the moment he says it rather than
        //      whenever the turn ahead of it finishes.
        self.stage_spoken_correction(binding, &turn_id, text, source);
        // (1c) THE SAME SEAM, for the OTHER correction family. Both live here rather than
        //      one here and one in the shell, because this is the one function every CEO
        //      utterance passes through, and a correction family reached by a different
        //      route would be a correction family with different rules about when it runs.
        self.stage_belief_correction(binding, text, source);
        // (1d) AND THE THIRD, which is the only one that watches rather than listens: what
        //      he DICTATED against what he actually sent. Same seam, same reason.
        self.stage_heard_correction(binding, &turn_id, text, source);
        Ok(turn_id)
    }

    /// [`Self::accept_prompt`] for words the app sends because he pressed a button: the same
    /// durable write and the same queued status, recorded `pressed`, and none of the three
    /// correction seams, which read only what he said.
    fn accept_pressed_prompt(&mut self, binding: &ThreadBinding, text: &str, intake_id: Option<u64>) -> Result<String, SpineError> {
        let turn_id = self.ledger.record_prompt_received_desktop(binding, text, Source::Text, intake_id, None, true)?;
        self.emit_live(self.turn_status_event(binding, &turn_id, TurnStatus::Queued, None));
        self.emit_live(self.thread_summary_event(binding, &turn_id, ThreadStatus::Queued));
        Ok(turn_id)
    }

    /// Examine one CEO utterance for a spoken correction and, if it is one, write it down.
    ///
    /// **Infallible from the turn's point of view, deliberately.** `journal.rs` establishes
    /// the rule this follows: a machinery write failure never fails a turn, because
    /// machinery is not truth. The same holds here for a stronger reason — a staged
    /// correction is a QUESTION, and losing the chance to ask one must never cost the CEO
    /// the sentence he actually said. Every error path below logs and returns.
    ///
    /// Runs on `Text` and `Jam` and on nothing else: `Internal` is a re-prime or a
    /// handoff-summary request that the CEO never wrote, and `Proactive` is Rich speaking.
    /// Neither is a person correcting anything, and detecting a "correction" in RichOS's
    /// own priming payload would be the system teaching itself its own mistakes.
    fn stage_spoken_correction(
        &mut self,
        binding: &ThreadBinding,
        turn_id: &str,
        text: &str,
        source: Source,
    ) {
        if !matches!(source, Source::Text | Source::Jam) {
            return;
        }
        if self.candidates.is_none() {
            return;
        }
        let thread_id = binding.thread_id().to_string();

        // The record the correction is spoken AGAINST — the anchor's evidence. THE
        // UTTERANCE ITSELF IS EXCLUDED: it was journalled a few lines above, and a
        // correction that anchors to its own sentence would report evidence it invented.
        let record: Vec<String> = match self.ledger.messages(&thread_id) {
            Ok(msgs) => {
                let mut kept: Vec<String> = msgs
                    .into_iter()
                    .filter(|m| m.turn_id != turn_id)
                    .map(|m| m.text)
                    .collect();
                if kept.len() > ANCHOR_WINDOW_MESSAGES {
                    kept.drain(..kept.len() - ANCHOR_WINDOW_MESSAGES);
                }
                kept
            }
            Err(e) => {
                eprintln!("[richos] correction trigger could not read the record: {e}");
                Vec::new()
            }
        };

        let detection = crate::spoken::detect(text, &record);
        if detection.asks.is_empty() {
            return;
        }
        let desk = self.candidates.as_ref().expect("checked above");
        // A poisoned desk lock loses the QUESTION, never the turn — same posture as the
        // disk-failure arm below, and stated rather than unwrapped.
        let Ok(mut desk) = desk.lock() else {
            eprintln!("[richos] correction desk lock is poisoned — the question is dropped, the turn is not");
            return;
        };
        match desk.stage(&detection, &thread_id, turn_id, text) {
            Ok(staged) => {
                // Released before the observer runs: a surface that blocks must not hold
                // the desk shut against the CEO's own answer.
                drop(desk);
                if let Some(obs) = self.correction_observer.as_deref() {
                    if !staged.is_empty() {
                        obs.on_correction_staged(&staged);
                    }
                }
            }
            // A disk failure here loses ONE question. It must not lose the turn.
            Err(e) => eprintln!("[richos] correction could not be staged: {e}"),
        }
    }

    /// Examine one SENT message against what was DICTATED, and if he silently fixed a
    /// mis-heard word, write the question down.
    ///
    /// **Infallible from the turn's point of view**, the same posture and for the same
    /// reason as the two triggers above: a staged correction is a QUESTION, and losing the
    /// chance to ask one must never cost the CEO the sentence he actually said.
    ///
    /// **`Source::Text` only** — one narrower than the other two, and deliberately. A
    /// `Jam` turn is live voice: `rich://voice-transcript` goes straight into the thread as
    /// a turn, so there is no composer, no edit, and nothing to diff. Running this on a Jam
    /// turn would diff a fresh utterance against an unrelated open-wispr dictation, which
    /// is precisely the wrong-match failure `heard::MATCH_MIN_SIMILARITY` exists to stop —
    /// and would be asking it to stop something we could simply not do.
    ///
    /// **Nothing is written by this** beyond the desk record. `CandidateDesk::confirm` — a
    /// human answer — remains the only path to a vocabulary write, and there is no argument
    /// this function could pass to skip it.
    fn stage_heard_correction(
        &mut self,
        binding: &ThreadBinding,
        turn_id: &str,
        text: &str,
        source: Source,
    ) {
        if !matches!(source, Source::Text) {
            return;
        }
        let (Some(src), Some(desk)) = (self.heard.as_ref(), self.candidates.as_ref()) else {
            return;
        };
        let now = crate::util::now_millis();
        // Only the window the pairing condition can claim anyway, so the journal read is
        // bounded by the same number the match is — one policy, not two.
        let journal = src.recent(now.saturating_sub(crate::heard::MATCH_WINDOW_MS));
        if journal.is_empty() {
            return;
        }
        let review = crate::heard::review(&journal, text, now);
        if review.detection.asks.is_empty() {
            return;
        }
        let thread_id = binding.thread_id().to_string();
        let Ok(mut desk) = desk.lock() else {
            eprintln!("[richos] correction desk lock is poisoned — the question is dropped, the turn is not");
            return;
        };
        match desk.stage(&review.detection, &thread_id, turn_id, text) {
            Ok(staged) => {
                // Released before the observer runs, for the reason the spoken path does
                // it: a surface that blocks must not hold the desk shut against the CEO's
                // own answer.
                drop(desk);
                if let Some(obs) = self.correction_observer.as_deref() {
                    if !staged.is_empty() {
                        obs.on_correction_staged(&staged);
                    }
                }
            }
            // A disk failure here loses ONE question. It must not lose the turn.
            Err(e) => eprintln!("[richos] a dictation correction could not be staged: {e}"),
        }
    }

    /// Examine one CEO utterance for a correction of a recorded BELIEF and, if it resolves
    /// to exactly one loro record, file a proposal on the correction desk.
    ///
    /// **Infallible from the turn's point of view**, the same posture and for a stronger
    /// version of the reason [`Self::stage_spoken_correction`] is: a proposal is a
    /// QUESTION, and losing the chance to ask one must never cost the CEO the sentence he
    /// actually said. Every error path below logs and returns.
    ///
    /// **Nothing is written by this.** `CorrectionDesk::propose` runs the loro writer with
    /// `--dry-run` and stores what WOULD be written; `confirm` — a human answer — remains
    /// the only path to a loro write, and there is no argument this function could pass to
    /// skip it (`correction.rs`, ceo-decisions.md §7).
    ///
    /// Runs on `Text` and `Jam` only, for exactly the reason the spoken trigger does:
    /// `Internal` is a re-prime or a handoff-summary request the CEO never wrote, and
    /// `Proactive` is Rich speaking. Detecting a "correction" inside RichOS's own priming
    /// payload would be the system correcting its own memory from a copy of it.
    fn stage_belief_correction(&mut self, binding: &ThreadBinding, text: &str, source: Source) {
        if !matches!(source, Source::Text | Source::Jam) {
            return;
        }
        let (Some(desk), Some(prov)) = (self.correction_desk.as_ref(), self.loro_provenance.as_ref())
        else {
            return;
        };
        let thread_id = binding.thread_id().to_string();
        // The records Rich was ACTUALLY given for this thread. Cloned out from under the
        // lock rather than held across detection: the compiler writes this same map on a
        // re-prime, and detection must not be able to stall a rotation.
        let records = match prov.lock() {
            Ok(p) => p.records_for(&thread_id).to_vec(),
            Err(_) => {
                eprintln!("[richos] the loro provenance lock is poisoned — the question is dropped, the turn is not");
                return;
            }
        };
        if records.is_empty() {
            return;
        }
        let detection = crate::belief::detect(text, &records);
        if detection.asks.is_empty() {
            return;
        }
        let entity = binding.entity_id().as_str().to_string();
        for ask in &detection.asks {
            let Ok(mut d) = desk.lock() else {
                eprintln!("[richos] the correction desk lock is poisoned — the proposal is dropped, the turn is not");
                return;
            };
            let filed = d.propose(&entity, &thread_id, ask.proposed_write(), ask.why());
            drop(d);
            match filed {
                Ok(p) => {
                    if let Some(obs) = self.proposal_observer.as_deref() {
                        obs.on_correction_proposed(&p);
                    }
                }
                // A suppressed record, a writer refusal at --dry-run, or a disk failure
                // loses ONE proposal. None of them may lose the turn.
                Err(e) => eprintln!("[richos] a loro correction could not be proposed: {e}"),
            }
        }
    }

    /// Deliver one already-journaled turn to the current lease. Each assistant delta is
    /// persisted to the ledger FIRST (crash-safe partial capture) and then emitted LIVE
    /// to the UI sink, so the CEO sees Rich's reply render token-by-token. Turn-state
    /// events bracket the turn: `turn-started` (the calm "Rich is working" affordance)
    /// and a terminal `turn-completed` / `turn-error`, all keyed to thread + turn.
    ///
    /// `allow_recovery`: when the lease dies mid-turn (a positive signal — `prompt()`
    /// returned `Err`, never inferred from silence), attempt ONE automatic mid-turn-crash
    /// recovery + replay (continuity §5.3) if a lease factory is attached. Pass `false`
    /// from the recovery path itself so a lease that dies immediately on every respawn
    /// surfaces honestly after one attempt rather than looping forever.
    fn deliver(&mut self,turn_id:&str,binding:&ThreadBinding,text:&str,allow_recovery:bool)->Result<(),SpineError> {
        if let Some(claim)=self.control.for_thread(binding.thread_id()).stop_claim().filter(|claim|
            claim.covers(turn_id, self.ledger.turn(turn_id).and_then(|turn|turn.intake_id))) {
            // Preserve the existing durable Stop cut without submitting pre-Stop input.
            self.ledger.stop_turn(turn_id,claim.requested_at)?;
            self.emit_live(self.turn_status_event(binding,turn_id,TurnStatus::Stopped,None));
            return Ok(());
        }
        let delivery=self.prepare_delivery(turn_id,binding,text,allow_recovery)?;
        self.dispatch_delivery(delivery)
    }

    fn dispatch_delivery(&mut self,mut delivery:Delivery)->Result<(),SpineError> {
        if self.detached_delivery {
            self.pending_delivery.push_back(delivery);
            return Ok(());
        }
        prepare_delivery_io(&mut &mut *self,&mut delivery);
        let result=enqueue_delivery(&mut delivery).and_then(|_|run_delivery(&mut delivery,&mut |delivery,item|self.capture_delivery_item(delivery,item)));
        self.finish_delivery(delivery,result)
    }

    /// Shared desktop callers drive prepared visible replies after releasing their lock.
    pub fn detach_conversation_io(&mut self) {self.detached_delivery=true;}

    fn prepare_delivery(&mut self,turn_id:&str,binding:&ThreadBinding,text:&str,allow_recovery:bool)->Result<Delivery,SpineError> {
        // The scope is re-verified at delivery, not merely at acceptance: a queued turn
        // may have waited across an active-context switch, and this is the last point
        // before the text reaches a compute lease.
        self.ledger.verify_binding(binding)?;
        let thread_id = binding.thread_id();
        let connection=self.select_connection(binding)?;
        let control=connection.control.clone();
        // §1.5 gap #1: everything the lease said while no turn was in flight — its
        // session-start traffic on the very first turn, and whatever arrived after the
        // previous turn's response — lands NOW, as honest thread-scoped machinery, BEFORE
        // the priming turn below can put its own residue in the lane. The ordering is the
        // rule: honest first, then internal. See `pump_between_turn_stamped`.
        self.pump_between_turn_stamped(binding, false);
        let lease=self.resident.entry(thread_id.into()).or_default().lease.clone();
        let session_id=control.lease_session();
        self.ledger.mark_turn_started(turn_id,session_id.as_deref().unwrap_or(""))?;
        self.turn_in_progress = true;
        // Publish the running turn to the shared control. A MIRROR of `turn_in_progress`,
        // written at the same instant and from the same durable fact — never inferred from
        // activity or from silence (continuity §5.2).
        if control.active_turn().is_none() {control.begin_turn(ActiveTurn {
            turn_id: turn_id.to_string(),
            thread_id: thread_id.to_string(),
            entity_id: Some(binding.entity_id().clone()),
            started_at: self.ledger.turn(turn_id).and_then(|t| t.started_at),
        });}

        // Turn start: the UI shows the calm "Rich is working" state now.
        self.emit(StreamEvent::TurnStarted {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            at: now_millis(),
        });
        // ADDITIVE (§13), after the SAME durable write and after the existing event, never
        // instead of it.
        self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Working, None));
        self.emit_live(self.thread_summary_event(binding, turn_id, ThreadStatus::Working));

        // A turn the CEO never sees (re-prime injection, the rotation handoff summary)
        // produces machinery he must never see either — §1.5's `internal` rule, the same
        // distinction `ActionVisibility::Internal` already draws (`ledger.rs:119-133`),
        // and the standing order that Rich never reveals or references session rotation.
        let internal_turn = self.ledger.turn(turn_id).map(|t| t.source == Source::Internal).unwrap_or(false);
        // The additive family's per-turn bookkeeping (§13). Holds no counter of its own:
        // message ids come from the ledger's `text_runs` fold, activity identity and
        // position from `machinery.rs`'s merge rules.
        let live_turn = LiveTurn::new(EventFence::for_turn(binding, turn_id), internal_turn);


        let stopped_before_prompt=control.stop_claim_for(turn_id).is_some();
        let preparation_stop=None;let preparation_failed=false;let preparation=Ok(());
        if let Some(session)=session_id.as_deref() {
            if self.ledger.turn(turn_id).is_some_and(|turn|turn.session_id.as_deref()!=Some(session)) {
                self.ledger.mark_turn_started(turn_id,session)?;
            }
        }
        if !internal_turn && !self.ledger.turns().iter().any(|turn|turn.superseded_by.as_deref()==Some(turn_id)) {
            self.resident.get_mut(thread_id).unwrap().upstream_budget.new_message();
        }
        self.turn_in_progress=false;
        Ok(Delivery {turn_id:turn_id.into(),binding:binding.clone(),text:text.into(),allow_recovery,
            connection:Some(connection),retry_wait:None,internal_turn,live_turn,persist_err:None,stopped_before_prompt,preparation_failed,
            preparation_stop,preparation,session_id,lease,usage:None,response:None,account:None})
    }

    fn capture_delivery_item(&mut self,delivery:&mut Delivery,item:TurnItem) {
        let turn_id=delivery.turn_id.as_str();
        let binding=&delivery.binding;
        let thread_id=binding.thread_id();
        let internal_turn=delivery.internal_turn;
        let live_turn=&mut delivery.live_turn;
        let persist_err=&mut delivery.persist_err;
        let ledger=&mut self.ledger;
        let observer=self.observer.as_deref();
        let journal=self.machinery_journal.as_ref();
        let machinery_observer=self.machinery_observer.as_deref();
        let live_observer=self.live.as_deref();
        let output_store=self.output.as_ref().map(|(store,_)|store);
        let worker_source=&self.worker_events;
        let worker_session=&delivery.session_id;
        let usage_slot=&mut delivery.usage;
        match item {
                // `seq` comes from the LEASE now (§1.4 G1): ONE counter per turn, shared
                // by text and machinery. The spine no longer counts text items itself —
                // that would be the second counter G1 exists to forbid.
                TurnItem::Text { seq, text: c } => {
                    // Ledger stays the source of truth: persist the delta BEFORE emitting
                    // — and persist it AT its shared-sequence position, so the
                    // interleaving of text and machinery survives the process (§1.4 G1).
                    let at = match ledger.append_assistant_delta(turn_id, c, seq) {
                        Ok(at) => at,
                        Err(e) => { *persist_err = Some(e); return; }
                    };
                    // Then emit it live (clean output: assistant text only ever reaches here).
                    if let Some(obs) = observer {
                        obs.on_event(&StreamEvent::Chunk {
                            thread_id: thread_id.to_string(),
                            turn_id: turn_id.to_string(),
                            seq,
                            text_delta: c.to_string(),
                            at,
                        });
                    }
                    // ADDITIVE (§13): message-started / message-delta, keyed to the run the
                    // LEDGER just folded this delta into — so the boundary between "he said
                    // X" and "then said Z" is the ledger's, not a second opinion.
                    // Borrowed, never cloned: this runs once per delta, and copying the
                    // whole reply-so-far each time would be quadratic in the reply length.
                    let runs = ledger.turn(turn_id).map(|t| t.text_runs.as_slice()).unwrap_or(&[]);
                    Self::forward_live(live_observer, live_turn.on_text(runs, seq, c, now_millis()));
                }
                TurnItem::Machinery(record) => {
                    let record = record.stamp(thread_id, Some(turn_id), internal_turn);
                    // THE ONE machinery record that is read rather than merely retained.
                    // Everything else in this family is routed for later interpretation;
                    // this pair decides when Rich rotates, so it is consumed here, live.
                    if let Some(usage) = record.context_usage() {
                        *usage_slot = Some(usage);
                        // MID-TURN, AT THE WALL. Rotation is structurally forbidden here
                        // (continuity §3.1) and firing one would be worse than firing
                        // late, so this DETECTS and RECORDS; `after_turn_boundary` acts.
                        // First crossing wins — it is the honest "when", and a later,
                        // higher reading in the same turn changes nothing that can be done
                        // about it.
                    }
                    // A tool call ENDS the open run of prose — the ledger will fold the next
                    // delta into a new run — so the message is closed here rather than at
                    // the turn's end, and §5.2's "commentary, then activity" is live-accurate.
                    let runs = ledger.turn(turn_id).map(|t| t.text_runs.as_slice()).unwrap_or(&[]);
                    Self::forward_live(live_observer, live_turn.close_open_message(runs, now_millis()));
                    Self::forward_live(live_observer, live_turn.on_machinery(&record, &|| worker_source.read(worker_session.as_deref())));
                    // Witness (a): a write-tool call of Rich's that just completed. Recorded
                    // here, live, so the count ticks while the turn is still running; a
                    // failure is logged and never fails the turn (§4.7).
                    let written = live_turn.take_output_writes();
                    if let (Some(store), false) = (output_store, written.is_empty()) {
                        if let Err(e) = store.append(thread_id, &written) {
                            eprintln!("[richos] output: a file Rich wrote could not be recorded: {e}");
                        }
                    }
                    Self::retain_and_emit_machinery(journal, machinery_observer, record);
                }
                // **HIS ANSWER IS COMPLETE, THE TURN RUNS ON** (product hunt 2026-10-15, finding
                // 21). The turn is still working, durably and for every other purpose (a message
                // he sends now steers it, exactly as before); this status only tells the screen
                // the answer is done, so it stops saying "Rich is working". Never for a turn he
                // does not see.
                TurnItem::AnswerComplete => {
                    if !internal_turn {
                        let turn = ledger.turn(turn_id);
                        Self::forward_live(live_observer, vec![LiveEvent::TurnStatus {
                            fence: EventFence::for_turn(binding, turn_id),
                            status: TurnStatus::Working,
                            started_at: turn.and_then(|t| t.started_at),
                            active_duration_ms: None,
                            supersedes_turn_id: None,
                            answered: true,
                            at: now_millis(),
                        }]);
                    }
                }
            }
    }

    fn finish_delivery(&mut self,mut delivery:Delivery,stop:Result<String,CognitionError>)->Result<(),SpineError> {
        let turn_id=delivery.turn_id.as_str();
        let binding=&delivery.binding;
        let thread_id=binding.thread_id();
        let text=delivery.text.as_str();
        let allow_recovery=delivery.allow_recovery;
        let stopped_before_prompt=delivery.stopped_before_prompt;
        let preparation_failed=delivery.preparation_failed;
        let persist_err=delivery.persist_err.take();
        let live_turn=&mut delivery.live_turn;
        if let Some(resident)=self.resident.get_mut(thread_id) {
            resident.parked_at_ms=now_millis();
            if delivery.usage.is_some() {resident.context_usage=delivery.usage;}
        }
        self.evict_beyond_the_cap();
        let control=self.control.for_thread(thread_id);
        self.turn_in_progress = false;
        // Witnesses (b) and (c) of the Output side panel PRD, for the lease that just ran this
        // turn: a front-desk worker's writes (nested inside the lead's turn) and the files the
        // turn's commands made. Whatever the outcome — a stopped turn's files were still
        // written. Before any rotation, so the session is still the one that ran the turn.
        self.project_output_evidence(thread_id);
        // The turn is over, so anything still arriving has no turn to belong to (§1.5).
        // Pumped HONESTLY here, and pumped here rather than only at the next turn's start
        // for one ordering reason: a rotation may follow immediately, and the rotation path
        // drains the lane as INTERNAL. Without this line, post-turn traffic that is the
        // CEO's own turn's aftermath would be stamped internal and disappear.
        //
        // It usually lands nothing. The adapter writes those updates AFTER the response
        // that unblocked the drain loop, so the reader thread has often not read them yet —
        // which is why this is one of several pump points and not the only one.
        self.pump_between_turn_stamped(binding, false);
        // The same durable Stop also covers inputs already submitted to this provider.
        // Its teardown is intentional, including for inputs Claude had not started yet.
        let stop_claim = control.stop_claim().filter(|claim|
            claim.covers(turn_id, self.ledger.turn(turn_id).and_then(|turn| turn.intake_id)));
        // The mirror is cleared whatever happens below, including on the error paths: a
        // stale "still running" would make the next stop request name a dead turn.
        control.end_turn(turn_id);
        // Nor does it run on its account any more: the switch may be complete now.
        if let Some(quota)=self.quota_service() {quota.accounts.turn_ended(turn_id);}
        // Every entrance reports completion here, including direct intake and failures.
        if !self.thread_is_running(thread_id) && !self.ended_turns.iter().any(|known|known.thread_id()==thread_id) {
            self.ended_turns.push(binding.clone());
        }

        // Close whatever run of prose was still open, whatever the outcome: the partial
        // text is already durable, so finalizing it is a statement about the ledger rather
        // than about the turn's success.
        let final_runs = self.ledger.turn(turn_id).map(|t| t.text_runs.as_slice()).unwrap_or(&[]);
        let closing = live_turn.close_open_message(final_runs, now_millis());
        self.emit_live(closing);

        // ONE LAST WORKER RE-JOIN, before the terminal turn status goes out. A worker's
        // state changes through hook writes in another process and produces no agent traffic
        // at all, so the last machinery record is not necessarily the last thing that
        // happened to a delegation. Without this the live row and a snapshot taken a second
        // later could disagree at exactly the moment the transcript settles and collapses.
        // Emitted for every outcome — completed, stopped, interrupted — because a turn that
        // ended badly still delegated.
        // The session id is threaded through here for the same reason it is at the other
        // call site: whose workers these are is derived from the session identity, never
        // guessed from a directory mtime.
        let session_id=self.control.for_thread(thread_id).lease_session();
        let converged = live_turn.on_turn_end(&|| self.worker_events.read(session_id.as_deref()));
        self.emit_live(converged);

        // A ledger write failing mid-stream is terminal for the turn (durability first).
        if let Some(e) = persist_err {
            self.ledger.interrupt_turn(turn_id, &e.to_string())?;
            self.emit(StreamEvent::TurnError {
                thread_id: thread_id.to_string(),
                turn_id: turn_id.to_string(),
                reason: e.to_string(),
                at: now_millis(),
            });
            // ADDITIVE (§13). No recovery is attempted on this path, so `failed` is final.
            self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Failed, None));
            self.emit_live(self.thread_summary_event(binding, turn_id, ThreadStatus::Failed));
            return Err(e.into());
        }

        // THE CEO STOPPED IT (§9.3 steps 4 and 5) — IF the stop is what ended the turn.
        //
        // Two things must not happen when a stop genuinely lands: a `completed` row for
        // work the CEO ended, and a crash-replay of a turn he explicitly told to stop.
        // Everything already streamed is already durable (each delta was persisted before
        // it was emitted), so step 4 — "preserve partial commentary, activity and
        // assistant output" — needs no code: not deleting it is the whole implementation.
        //
        // A THIRD THING MUST NOT HAPPEN, AND USED TO. This branch was taken on the mere
        // EXISTENCE of a stop claim, before looking at what the lease reported — so a turn
        // that ran to completion while the stop was in flight was written `Stopped`, and
        // `app/ui/timeline.js` rendered `You stopped after {d}` (the one row that names the
        // CEO as the cause of anything) above a complete, successful answer. It failed in
        // the direction where he believes he prevented something he did not.
        //
        // The race is real and narrow: `NativeCancelHandle::cancel` clones the sink and
        // appends `ChunkMsg::Cancel` AFTER whatever is already queued, so when the
        // adapter's `Done` is already in the channel `rx.recv()` returns it first and
        // `prompt` returns `"end_turn"` (`native.rs`'s `prompt` loop). That handle's own doc guards
        // `Done` RACING the wake; this is `Done` ALREADY QUEUED before the wake exists.
        //
        // The distinguishing signal was passed into `finish_stopped_turn` and thrown away.
        // `STOP_REASON_CANCELLED` — the adapter's acknowledgement that it HONOURED the
        // cancel — had no production reader anywhere. It has one now, right here.
        if let Some(claim) = stop_claim {
            let lease_reported = stop.as_deref().ok();
            // Did the stop end this turn?
            //
            //   `cancelled`             — yes, the adapter said so.
            //   `cancel_unacknowledged` — yes as far as RichOS is concerned: the CEO's stop
            //                             stands and we stopped rendering, whatever the
            //                             adapter is still doing.
            //   `None` (the lease errored) — the turn did not reach a terminal of its own,
            //                             and a stopped turn must never be crash-replayed.
            //   anything else           — NO. The lease reported its OWN terminal
            //                             (`end_turn`, `max_tokens`, `refusal`, …). The
            //                             turn ended because it finished.
            let stop_ended_the_turn = stopped_before_prompt || preparation_failed || !matches!(
                lease_reported,
                Some(r) if r != crate::native::STOP_REASON_CANCELLED
                    && r != crate::native::STOP_REASON_CANCEL_UNACKNOWLEDGED
            );
            if stop_ended_the_turn {
                self.finish_stopped_turn(turn_id, binding, &claim)?;
            } else {
                self.finish_completed_turn_the_stop_missed(turn_id, binding, &claim, lease_reported.unwrap())?;
            }
            // Native marks a stopped production connection closed. The normal opener
            // reaps and resumes it on the next explicit input without clearing newer handles.
            return Ok(());
        }

        // ==============================================================================
        // THE UPSTREAM MODEL API FAILED (`open-items.md` row 3.30, measured 2026-09-03)
        // ==============================================================================
        //
        // Checked BEFORE the ordinary Ok/Err split, because it can arrive on EITHER side
        // and neither side alone would catch it:
        //
        //   * `Err(CognitionError)` — the client gave up. Its `Display` carries the
        //     child's words.
        //   * `Ok(stop_reason)` — and this is the shape that would otherwise be missed
        //     entirely. `claude` reports an API failure as an ASSISTANT MESSAGE, so
        //     `native.rs` streams `API Error: 529 Overloaded …` as text, the ledger
        //     persists it as Rich's reply, and the turn ends with a perfectly ordinary
        //     terminal. Nothing in this file used to look at it, so a `529` presented as
        //     a completed turn whose answer was a vendor diagnostic in Rich's voice.
        //
        // NEITHER WIRE SHAPE IS CAPTURED, and that is stated rather than glossed: the
        // twelve runs in docs/verification/native-claude-stream-json-2026-08-31/raw/
        // contain no API error at all. So both are covered, which is breadth in place of
        // a capture and not the same thing as one.
        // ==============================================================================
        // FILL-FIRST'S BACKSTOP: THE TURN WAS REFUSED FOR A USAGE LIMIT ANYWAY
        // ==============================================================================
        //
        // The switch normally happens BEFORE a turn (`select_connection`), from the freshest
        // reading and the measured speed, so the account is left before it is full. This is
        // only for one turn that by itself used up what remained. The account is marked gone,
        // the next account with room is put in use, and his prompt is re-served on a fresh
        // lease exactly as the mid-turn-crash path does: one clean exchange, the failed turn
        // superseded. With no account left, nothing changes here and the turn ends as it
        // would have with one account.
        if let Err(error)=&stop {
            if let Some(resets_at)=crate::claude_accounts::parse_usage_limit(&error.to_string()) {
                if let (Some(quota),Some(account))=(self.quota_service(),delivery.account.as_deref()) {
                    if quota.limit_reached(account,resets_at)==crate::claude_accounts::AfterLimit::Continue {
                        self.ledger.interrupt_turn(turn_id,&error.to_string())?;
                        self.raise_quota_notices(Some(thread_id));
                        self.emit(StreamEvent::TurnError{thread_id:thread_id.into(),turn_id:turn_id.into(),
                            reason:"Continuing on the next Claude account.".into(),at:now_millis()});
                        self.emit_live(self.turn_status_event(binding,turn_id,TurnStatus::Recovering,None));
                        return self.retry_saved_input(turn_id,binding,text,std::time::Duration::ZERO);
                    }
                }
            }
        }

        if let Some(failure) = self.detect_upstream_failure(turn_id, &stop) {
            return self.finish_upstream_failure(turn_id, binding, text, failure, allow_recovery);
        }

        match stop {
            Ok(stop_reason) => {
                // A COMPLETED TURN IS THE POSITIVE SIGNAL that the upstream works, and it
                // is the only thing that restores the retry allowance. Never the absence
                // of a failure, which is what silence looks like.
                if let Some(desk)=self.resident.get_mut(thread_id) {desk.upstream_budget.succeeded();}
                self.ledger.complete_turn(turn_id, &stop_reason)?;
                self.emit(StreamEvent::TurnCompleted {
                    thread_id: thread_id.to_string(),
                    turn_id: turn_id.to_string(),
                    stop_reason,
                    at: now_millis(),
                });
                // ADDITIVE (§13). `complete_turn` wrote `ended_at` first, so the
                // `activeDurationMs` this carries is the MEASURED span, not an estimate —
                // and it is still `null` for a turn whose start was never recorded.
                self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Completed, None));
                self.emit_live(self.thread_summary_event(binding, turn_id, ThreadStatus::Idle));
                Ok(())
            }
            Err(e) => {
                // Deltas up to the failure are already persisted + emitted; mark interrupted.
                self.ledger.interrupt_turn(turn_id, &e.to_string())?;
                // WHY it ended, classified here and made durable beside it (D2). Before
                // 2026-09-17 the raw `CognitionError` Display went out on the wire as the
                // CEO-facing `reason` and the surface replaced it with two fixed sentences
                // that were wrong for this case — a permanent condition called a snag, a
                // promise about saved work that did not exist, and a retry that could not
                // succeed. The statement below is authored in `interruption.rs` from the
                // classification and from counts read off the ledger.
                let (_cause, statement) = self.record_interruption(turn_id, &e.to_string());
                self.emit(StreamEvent::TurnError {
                    thread_id: thread_id.to_string(),
                    turn_id: turn_id.to_string(),
                    reason: statement,
                    at: now_millis(),
                });
                self.emit_live(self.turn_status_event(binding,turn_id,TurnStatus::Failed,None));
                self.emit_live(self.thread_summary_event(binding,turn_id,ThreadStatus::Failed));
                Err(e.into())
            }
        }
        }

    /// Retain ONE machinery record, then hand it to the live sink.
    ///
    /// **Retention first, rendering second, and retention is unconditional** (§3.2:
    /// *"Routing and retention run ALWAYS. The toggle controls rendering ONLY"*). Making
    /// retention conditional would destroy the feature, because the CEO's requirement is
    /// to flip a thread he ALREADY HAD.
    ///
    /// Drain the lease's between-turn lane into the journal (techy-mode §1.5, gap #1).
    ///
    /// Returns how many records landed. `0` is an ordinary answer and never an error: it
    /// means the adapter has said nothing since the last drain, which is the normal state
    /// of a quiet session.
    ///
    /// **`turn_id` is `None` and stays `None`.** These records attach to the THREAD. There
    /// is no turn they belong to, and stamping the previous turn (or the next one) would be
    /// a false attribution — the one thing a record of what happened must not do. §1.4 G4
    /// makes `turn_id: None` a first-class state rather than a gap.
    ///
    /// **Scope is verified BEFORE the drain, not after.** A drain is destructive: the lane
    /// hands its records over and forgets them. Verifying first means a thread whose binding
    /// no longer holds leaves the traffic PARKED for whoever can scope it, instead of
    /// draining it into a refusal.
    ///
    /// Retention needs no special case here — [`Self::retain_and_emit_machinery`] is the
    /// same append the in-turn path uses, so the raw window ([`crate::journal::RawRetention`])
    /// covers these records exactly as it covers every other one.
    pub fn pump_between_turn(&mut self) -> usize {
        let Some(binding) = self.active.clone() else { return 0 };
        self.pump_between_turn_stamped(&binding, false)
    }

    /// [`Self::pump_between_turn`], with the `internal` bit supplied by the caller that
    /// knows what the lease was just doing.
    ///
    /// **THIS IS WHERE THE STANDING ORDER IS HELD, and it is a structural rule rather than
    /// an argument about what the adapter is likely to say.** Machinery produced by a turn
    /// the CEO never sees is `internal: true` (§1.5) — but the lane is filled by the reader
    /// thread at moments no turn owns, so "which turn produced it" is not a question the
    /// record can answer for itself. The rule the callers follow instead:
    ///
    ///   * the lane is drained as HONEST traffic *before* an internal turn is started, and
    ///   * drained as INTERNAL immediately *after* one finishes.
    ///
    /// So anything the lease says in the aftermath of a re-prime, a handoff summary or a
    /// rotation is stamped `internal: true` and can never render, without anyone having to
    /// reason about whether a particular vendor kind would give a rotation away. Rotation
    /// stays invisible to the CEO, and that includes its residue.
    fn pump_between_turn_stamped(&mut self, binding: &ThreadBinding, internal: bool) -> usize {
        if self.ledger.verify_binding(binding).is_err() {
            return 0;
        }
        let records = match self.resident.get(binding.thread_id()).and_then(|desk|desk.lease.try_lock().ok()) {
            Some(mut slot) => match slot.as_mut() {Some(lease)=>lease.drain_between_turn(),None=>return 0},
            None => return 0,
        };
        if records.is_empty() {
            return 0;
        }
        let thread_id = binding.thread_id().to_string();
        let journal = self.machinery_journal.as_ref();
        let observer = self.machinery_observer.as_deref();
        let landed = records.len();
        for record in records {
            let record = record.stamp(&thread_id, None, internal);
            Self::retain_and_emit_machinery(journal, observer, record);
        }
        landed
    }

    /// **A machinery write failure NEVER fails a turn** (§2.2's corollary). `deliver`
    /// makes a LEDGER write failure terminal for the turn — correctly, because the ledger
    /// is truth. Machinery is not truth: a failed write is logged to stderr and the turn
    /// continues. Taking a `&` journal (not `&mut`) is what lets this sit inside the
    /// streaming closure alongside the `&mut Ledger` borrow.
    fn retain_and_emit_machinery(
        journal: Option<&MachineryJournal>,
        observer: Option<&dyn MachineryObserver>,
        record: MachineryRecord,
    ) {
        if let Some(j) = journal {
            if let Err(e) = j.append(&record) {
                eprintln!("[richos] machinery journal write failed (turn continues): {e}");
            }
        }
        if let Some(obs) = observer {
            obs.on_machinery(&record);
        }
    }

    /// Terminal handling for a turn the CEO stopped (UX §9.3 steps 4-6).
    ///
    /// Two things must not happen here, and both used to be the only options: a
    /// `completed` row for work the CEO ended, and a crash-replay of a turn he explicitly
    /// told to stop. `Ledger::stop_turn` quotes the REQUEST's timestamp rather than the
    /// clock, so `You stopped after {duration}` is anchored to the moment he pressed the
    /// button and not to the moment the lease got round to letting go.
    ///
    /// **This is now reached only when the stop actually ended the turn** — the lease
    /// reported `cancelled`, reported `cancel_unacknowledged`, or reported nothing because
    /// it errored. A lease that reported its own terminal goes to
    /// [`Spine::finish_completed_turn_the_stop_missed`] instead.
    ///
    /// **On the legacy `stream.rs` family this emits `TurnCompleted`, not `TurnError`.**
    /// That family has four events and none of them means "stopped"; of the two that could
    /// carry a terminal, `TurnError` says something went wrong, and nothing did. The
    /// authoritative statement is `TurnStatus::Stopped` on the additive §13 family, which
    /// is what the shipping UI reads. The legacy `stop_reason` carries `stopped_by_ceo` so
    /// even a consumer that only listens to the old family is not misled.
    fn finish_stopped_turn(
        &mut self,
        turn_id: &str,
        binding: &ThreadBinding,
        claim: &StopClaim,
    ) -> Result<(), SpineError> {
        let thread_id = binding.thread_id().to_string();
        self.ledger.stop_turn(turn_id, claim.requested_at)?;
        self.emit(StreamEvent::TurnCompleted {
            thread_id,
            turn_id: turn_id.to_string(),
            stop_reason: STOPPED_BY_CEO.to_string(),
            at: now_millis(),
        });
        self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Stopped, None));
        self.emit_live(self.thread_summary_event(binding, turn_id, ThreadStatus::Idle));

        Ok(())
    }

    /// Terminal handling for a turn the CEO tried to stop and which finished anyway.
    ///
    /// The order below is the whole fix and it is not interchangeable:
    ///
    /// 1. `complete_turn` — because that is what happened. Written FIRST, while the turn is
    ///    still `InFlight`, so it actually takes.
    /// 2. `stop_turn` — because the CEO really did ask, and a request that did not land is
    ///    still a fact worth having on disk. `Ledger::apply` refuses to move a turn that has
    ///    already ended (*"A stop OVERRIDES nothing that already ended: a turn that completed
    ///    before the stop request reached the lease stays completed, because it did"*), so
    ///    this writes a durable `TurnStopped` event that the projection correctly ignores.
    ///
    /// That guard existed and was UNREACHABLE from the live path — the spine never called
    /// those two in that order, so the only thing proving it was a unit test calling the
    /// ledger directly. This is the call site that makes it real.
    ///
    /// The events emitted are the ordinary completion events, byte-for-byte, because the
    /// ordinary thing is what occurred: `stop_reason` is the lease's own terminal, verbatim.
    /// Emitting `Stopped` here is what put "You stopped after" over a finished answer.
    ///
    /// The stop is not discarded, only outranked: `settle_stop_claim` still runs at the
    /// boundary, so anything the CEO had QUEUED behind this turn is still stopped. That is
    /// the part of his instruction that can still be honoured, and withholding is the safe
    /// direction.
    fn finish_completed_turn_the_stop_missed(
        &mut self,
        turn_id: &str,
        binding: &ThreadBinding,
        claim: &StopClaim,
        lease_stop_reason: &str,
    ) -> Result<(), SpineError> {
        let thread_id = binding.thread_id().to_string();
        self.ledger.complete_turn(turn_id, lease_stop_reason)?;
        self.ledger.stop_turn(turn_id, claim.requested_at)?; // recorded; refused by the guard
        self.emit(StreamEvent::TurnCompleted {
            thread_id,
            turn_id: turn_id.to_string(),
            stop_reason: lease_stop_reason.to_string(),
            at: now_millis(),
        });
        self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Completed, None));
        self.emit_live(self.thread_summary_event(binding, turn_id, ThreadStatus::Idle));
        Ok(())
    }

    /// What a stop means for work the CEO had already lined up behind it.
    ///
    /// A stop that immediately starts the next queued turn is not a stop — the CEO would
    /// press the button and watch Rich carry straight on. So anything ACCEPTED BUT NOT YET
    /// DELIVERED when the stop request landed is stopped too, and his words stay in the
    /// ledger and on screen rather than being deleted.
    ///
    /// The cut is by intake id, not by wall clock: steering written BEFORE the stop is
    /// cancelled with it, steering written AFTER it is a new instruction and survives.
    fn settle_stop_claim(&mut self, binding: &ThreadBinding) -> Result<(), SpineError> {
        let control=self.control.for_thread(binding.thread_id());
        let Some(claim)=control.stop_claim() else {return Ok(())};
        if self.ledger.pending_turns().iter().any(|turn| turn.thread_id == binding.thread_id()
            && claim.covers(&turn.id, turn.intake_id)) { return Ok(()); }
        if control.active_turn().is_none() {
            self.emit_live(self.thread_summary_event(binding, &claim.turn_id, ThreadStatus::Idle));
        }
        control.clear_stop_claim();
        Ok(())
    }

    /// **THE BINDING AN EVENT MAY BE FENCED WITH, AND IT IS NOT ALWAYS THE LEDGER'S.**
    ///
    /// Two revisions exist for one thread and only one of them is on disk:
    ///
    ///   * `Ledger::thread_binding` returns the revision written when the thread was BOUND
    ///     and never rewritten — 1, for the life of a thread nobody re-binds.
    ///   * `Spine::activate` mints a FRESH, higher one from `Ledger::take_revision`
    ///     (`rebind_at_new_revision`) for the active context, and deliberately persists
    ///     nothing. That is the revision `main.js` hands `RichTimeline.bind` — 3 in
    ///     `phone_intake_tests::the_fence_on_his_phone_message_is_the_one_the_open_window_is_holding`.
    ///
    /// `timeline.js accepts()` applies §13's staleness fence — `payload.bindingRevision <
    /// model.bindingRevision` → reject — so an event fenced from the LEDGER for the thread
    /// the CEO currently has open is thrown away by the one surface it was written for.
    ///
    /// **That is Ray's R3 on candidate .16, and the measurement is the diagnosis.** A message
    /// typed on his phone took **7,288 ms** to reach the open thread and arrived in the SAME
    /// PAINT as the finished reply, while the sidebar's dot moved at **+471 ms** — because
    /// `rich://thread-summary-updated` is handled outside the timeline model and is not
    /// fenced at all. The events were on time; the fenced ones were discarded, and nothing
    /// but `turn-completed`'s reload ever drew them.
    /// (`docs/verification/2026-09-19-nightly-1.2.0-nightly.20260919.5-stale-engine-and-phone-audit.md`
    /// §"R3, phone → Mac".)
    ///
    /// A desk message never had the defect: `submit_prompt_inner` fences on
    /// `ensure_active_thread()`. So this is the intake road being given the same binding the
    /// desk road already uses, and nothing more — *"from here on the spine does not know or
    /// care which mouth the CEO used"* is restored to the events as well as to the ledger.
    ///
    /// For any OTHER thread the ledger's binding is the right and only answer: no window is
    /// fenced on it, and a snapshot draws it when he opens it.
    fn fence_binding(&self, thread_id: &str) -> Result<ThreadBinding, SpineError> {
        if let Some(active) = &self.active {
            if active.thread_id() == thread_id {
                return Ok(active.clone());
            }
        }
        Ok(self.ledger.thread_binding(thread_id)?)
    }

    /// Move durable intake records into the ledger, now that the boundary is clear.
    ///
    /// This is where §9.2's "persisted before delivery" pays out: the CEO's steering was
    /// already on disk when he pressed Enter, and it becomes a real, entity-scoped turn
    /// here — through `record_prompt_received`, under a binding read fresh from the
    /// ledger, exactly like any other prompt. The intake log is not a second source of
    /// truth; this is the only thing that ever happens to a record.
    ///
    /// **A record that cannot be turned into a turn is NOT marked drained and NOT
    /// discarded.** It stops the drain and comes back at the next boundary. That blocks
    /// everything behind it, which is the deliberate trade: the CEO's words surviving
    /// matters more than the liveness of the records queued after them, and a binding
    /// failure here is systemic rather than per-message.
    fn drain_intake(&mut self) -> Result<(), SpineError> {
        let mut deferred_stop=false;
        for record in self.control.pending_intake() {
            match record {
                IntakeRecord::Steer { id, thread_id, text, .. } => {
                    // ALREADY DRAINED, and this is not a theoretical branch. The order below
                    // is ledger-first, marker-second, on purpose: a crash between them must
                    // re-present the CEO's words rather than lose them. So the drain is
                    // at-least-once, and without this check a restart in that window files
                    // the same sentence as a second turn — the CEO's one message, asked
                    // twice.
                    if self.ledger.turn_for_intake(id).is_some() {
                        if !deferred_stop {self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;}
                        continue;
                    }
                    let binding = self.fence_binding(&thread_id)?;
                    let turn_id = self.record_prompt(&binding, &text, Source::Text, None, Some(id), DESK_CHANNEL)?;
                    self.emit_live(self.turn_status_event(&binding, &turn_id, TurnStatus::Queued, None));
                    // AND WHAT HE ACTUALLY SAID — the same line `accept_prompt` carries, for the
                    // same reason. See the `Channel` arm below, where its absence was a defect a
                    // person could see.
                    self.emit_live(self.ceo_message_event(&binding, &turn_id));
                    self.emit_live(self.thread_summary_event(&binding, &turn_id, ThreadStatus::Queued));
                    self.deliver(&turn_id, &binding, &text, true)?;
                    if !deferred_stop {self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;}
                }
                IntakeRecord::Channel { id, thread_id, text, channel, client_id, voice_note, source, .. } => {
                    if self.ledger.turn_for_intake(id).is_some() || client_id.as_ref().is_some_and(|key|
                        self.ledger.thread_turns(&thread_id).is_ok_and(|turns| turns.iter().any(|turn| turn.client_id.as_ref() == Some(key)))) {
                        if !deferred_stop {self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;}
                        continue;
                    }
                    let binding = self.fence_binding(&thread_id)?;
                    let source = source.unwrap_or(Source::Text);
                    let kept = (self.keep_intake_channel || source == Source::Agent).then_some(channel.as_str());
                    let turn_id = self.ledger.record_prompt_received_from_client(&binding, &text, source, id, kept, client_id.as_deref(), voice_note)?;
                    self.input_channels.insert(turn_id.clone(), channel);
                    self.emit_live(self.turn_status_event(&binding, &turn_id, TurnStatus::Queued, None));
                    self.emit_live(self.ceo_message_event(&binding, &turn_id));
                    self.emit_live(self.thread_summary_event(&binding, &turn_id, ThreadStatus::Queued));
                    self.deliver(&turn_id, &binding, &text, true)?;
                    if !deferred_stop {self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;}
                }
                IntakeRecord::Desk { id, thread_id, text, .. } => {
                    // **HIS OWN COMPOSER, DEFERRED — so it lands exactly as if it had never
                    // been deferred** (CEO §55). The two arms above file the turn and stop;
                    // this one goes through `accept_prompt`, which is the whole of the
                    // acceptance `submit_prompt_inner` performs. A correction he typed while
                    // the front desk was priming is therefore staged, and the only difference
                    // between this road and the direct one is WHEN the spine could be reached.
                    if self.ledger.turn_for_intake(id).is_some() {
                        if !deferred_stop {self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;}
                        continue;
                    }
                    let binding = self.fence_binding(&thread_id)?;
                    let turn_id = self.accept_prompt(&binding, &text, Source::Text, None, Some(id), DESK_CHANNEL)?;
                    self.deliver(&turn_id, &binding, &text, true)?;
                    if !deferred_stop {self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;}
                }
                IntakeRecord::Stop { id, turn_id, at } => {
                    let thread = self.ledger.turn(&turn_id).map(|turn| turn.thread_id.clone());
                    let claim = StopClaim { turn_id: turn_id.clone(), intake_id: id, requested_at: at };
                    if self.control.has_active_turn_id(&turn_id) || self.ledger.pending_turns().iter()
                        .any(|turn| Some(&turn.thread_id) == thread.as_ref() && claim.covers(&turn.id, turn.intake_id)) {
                        deferred_stop=true;
                        continue;
                    }
                    let open = self.ledger.delivery_chain(&turn_id).iter().any(|id| {
                        self.ledger.turn(id).is_some_and(|turn| matches!(turn.state,
                            crate::ledger::TurnState::Received | crate::ledger::TurnState::InFlight))
                    });
                    if open {
                        self.ledger.stop_turn(&turn_id, at)?;
                    }
                    if !deferred_stop {self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;}
                }
                IntakeRecord::Drained { .. } => {}
            }
        }
        Ok(())
    }

    /// **THE IDLE DRAIN — the entry point the phone is unusable without** (phone-client
    /// plan §4.2 ii and vii).
    ///
    /// # The gap this closes, measured rather than recalled
    ///
    /// Before this function, `drain_intake` was called from exactly two places:
    /// `after_turn_boundary` and `reconcile_intake` at boot. Both are true today — `:2658`
    /// and `:2614` in the version this was written against, and the plan cites the same two
    /// lines. **Neither of them happens while the app sits idle.** So a message written to
    /// the intake log by a channel that is not the desktop window would sit there until the
    /// CEO's next desktop turn or the next launch. On the desktop that was invisible,
    /// because the only writer was the steering composer, which by construction only writes
    /// while a turn is running and therefore always has a boundary coming. A phone does not:
    /// *"He is standing in the kitchen watching his phone"*, and silence there looks exactly
    /// like a broken bridge.
    ///
    /// # What it does, and what it deliberately does not
    ///
    /// Take the lock, drain, and start the turn if none is running — the plan's own three
    /// clauses, in that order. It is `reconcile_intake` minus the crash-replay half, which
    /// is why it is three lines rather than thirty: **no new machinery, one new caller for
    /// machinery that already exists.**
    ///
    /// Intake can arrive while other conversations are replying. It is accepted into
    /// the same ledger immediately; only a turn on its own conversation delays delivery.
    /// Synchronous preparation still excludes intake until it releases the shared state.
    ///
    /// **It is called after a write, never on a timer.** Plan §6 forbids *"a polling loop
    /// that wakes the Mac"*, and a drain that runs on a schedule is exactly that. The
    /// bridge writes the record, `fsync`s it, answers the phone, and then calls this.
    ///
    /// Returns what `drain_queue` returns: the turn runs to completion inside this call,
    /// exactly as it does for a desktop prompt.
    pub fn queue_question_answer(&mut self,delivery:&crate::questions::Delivery)->Result<String,SpineError> {
        let binding=self.fence_binding(&delivery.thread_id)?;
        if binding.entity_id().as_str()!=delivery.entity_id {return Err(SpineError::Steering("Question scope mismatch".into()));}
        // The order taken when the app accepted his click (the question store carries it), or
        // the same acceptance order a typed message takes, so a Stop he presses after it covers it.
        let order=match delivery.order {Some(order)=>{self.control.advance_input_order(order);Some(order)},None=>self.control.take_input_order()};
        let turn_id=self.ledger.record_question_answer(&binding,delivery,order)?;
        if self.ledger.turn(&turn_id).is_some_and(|t|t.state==crate::ledger::TurnState::Received)
            && !self.pending_delivery.iter().any(|pending|pending.turn_id==turn_id) {
            if delivery.set_id.is_none(){self.emit_live(self.ceo_message_event(&binding,&turn_id));}
            // Accepted before his Stop, settled or still pending when this delivery arrives:
            // stopped with the rest (the pending case is the one `deliver` would stop).
            let control=self.control.for_thread(binding.thread_id());
            let intake=self.ledger.turn(&turn_id).and_then(|turn|turn.intake_id);
            let covering=delivery.order.and_then(|order|control.settled_stop_covering(order))
                .or_else(||control.stop_claim().filter(|claim|claim.covers(&turn_id,intake)));
            if let Some(claim)=covering {
                self.ledger.stop_turn(&turn_id,claim.requested_at)?;
                self.emit_live(self.turn_status_event(&binding,&turn_id,TurnStatus::Stopped,None));
                // The receipt says it was stopped, so the card does not say Rich has it.
                return Ok(format!("{}{turn_id}",crate::questions::STOPPED_RECEIPT_PREFIX));
            }
            self.deliver(&turn_id,&binding,&delivery.text,true)?;
        }
        Ok(turn_id)
    }

    pub fn poll_intake(&mut self) -> Result<(), SpineError> {
        // Preparation still owns the shared state. Detached visible replies instead
        // own their per-conversation control and do not hold this flag.
        if self.turn_in_progress {return Ok(());}
        self.drain_intake()
    }

    /// **Every conversation whose turn has ended since this was last asked, and the list
    /// emptied.** The shell's one question at every turn boundary: which conversations may
    /// now start the work Rich wrote down during them (`WorkHost::adopt_registered`,
    /// background-work spec §7.1: *"`prepare` runs on the work lease after the turn has
    /// ended"*).
    ///
    /// **Why the spine answers it and the caller does not guess.** The typed send used to
    /// adopt for the thread that was active afterwards, and the spoken turn and the phone's
    /// drain adopted for nothing (esc-20260926T083231Z-23865924): a task he gave aloud or
    /// from the phone sat `Registered` until he next typed. Worse, one call here can run
    /// turns on several conversations — `poll_intake` drains every record on the log, a
    /// turn's boundary drains what arrived during it, the boot reconciliation and the front
    /// desk's priming drain what was waiting. Every ending is recorded by `finish_delivery`,
    /// including failures and direct intake delivery, and every entrance asks here after
    /// its call and adopts for all of them, with the spine released first (§0 row 3).
    ///
    /// A turn that FAILED is on the list too: a registration written before the failure is
    /// still a receipt he was given, and `adopt_registered` is a directory read that adopts
    /// only `Registered` rows and each one once. Nothing here retries or restarts anything
    /// (spec §6.3).
    pub fn take_ended_turns(&mut self) -> Vec<ThreadBinding> {
        std::mem::take(&mut self.ended_turns)
    }

    /// Startup reconciliation: apply stop requests that outlived the process.
    ///
    /// The crash window this closes is one line wide. The stop request is fsync'd before
    /// the lease is touched (`steering.rs`), and the ledger's `TurnStopped` is written when
    /// the turn ends. Die in between and the turn is `in_flight` forever with a durable
    /// stop request sitting beside it — and §5.3's crash-replay would REPLAY a turn the
    /// CEO explicitly stopped. Running this at boot is what makes that impossible.
    ///
    /// Called by the shell after the ledger and the control are both open. Safe to call
    /// when there is nothing to do.
    pub fn reconcile_intake(&mut self) -> Result<(), SpineError> {
        let interrupted: Vec<String> = self.ledger.pending_turns().iter()
            .filter(|turn| turn.source != Source::Internal)
            .map(|turn| turn.id.clone()).collect();
        // A Stop that settled before the restart still covers a card answer saved before it
        // and not yet delivered (review rv-20261018T204635Z-c68a30ec-f61e#2).
        for record in self.control.stop_records() {
            if let IntakeRecord::Stop { id, turn_id, at } = record {
                if let Some(thread) = self.ledger.turn(&turn_id).map(|turn| turn.thread_id.clone()) {
                    self.control.for_thread(&thread).restore_settled(StopClaim { turn_id, intake_id: id, requested_at: at });
                }
            }
        }
        for record in self.control.pending_intake() {
            if let IntakeRecord::Stop { id, turn_id, at } = record {
                let claim = StopClaim { turn_id: turn_id.clone(), intake_id: id, requested_at: at };
                let thread = self.ledger.turn(&turn_id).map(|turn| turn.thread_id.clone());
                if let Some(thread) = &thread {
                    self.control.for_thread(thread).restore_stop_claim(claim.clone());
                }
                let affected: Vec<_> = self.ledger.pending_turns().iter()
                    .filter(|turn| Some(&turn.thread_id) == thread.as_ref() && claim.covers(&turn.id, turn.intake_id))
                    .map(|turn| turn.id.clone()).collect();
                self.ledger.stop_turn_after_restart(&turn_id, at)?;
                for id in affected { self.ledger.stop_turn_after_restart(&id, at)?; }
            }
        }
        for id in interrupted {
            let Some(turn) = self.ledger.turn(&id) else { continue };
            if !matches!(turn.state, crate::ledger::TurnState::Received | crate::ledger::TurnState::InFlight) {
                continue;
            }
            let binding = self.fence_binding(&turn.thread_id)?;
            if turn.state == crate::ledger::TurnState::Received {
                let text=turn.user_text.clone();
                self.deliver(&id,&binding,&text,true)?;
                continue;
            }
            self.ledger.interrupt_turn_after_restart(&id,&crate::native::NativeError::Closed.to_string())?;

        }
        // Restore the already accepted ledger requests before appending newer intake.
        // Durable Stops above still take precedence over both paths.
        self.drain_intake()
    }

    /// Everything that happens AT a turn boundary, after delivery and before the next
    /// prompt is considered (continuity §3.1: the turn-boundary controller's other job,
    /// alongside queue-not-interrupt). Runs whether the turn came from `submit_prompt`
    /// directly or from draining the queue — both call sites only reach here once
    /// `turn_in_progress` is false, so nothing below ever runs mid-turn.
    fn after_turn_boundary(&mut self, binding: &ThreadBinding) -> Result<(), SpineError> {
        if self.thread_is_running(binding.thread_id()) {return Ok(())}
        // Keep transient phone provenance through an in-process lease recovery, then
        // release it at the boundary. Product installs still write no channel to the ledger.
        self.input_channels.retain(|id, _| self.ledger.turn(id).is_some_and(|turn|
            matches!(turn.state, crate::ledger::TurnState::Received | crate::ledger::TurnState::InFlight)));
        // A switch notice or a high-speed alert that arrived while this turn held the spine
        // is said now, the first moment it can be.
        // The CEO's two mid-turn controls settle HERE, at the boundary, and in this order.
        //
        // DRAIN FIRST, THEN STOP — and the order was chosen by running it the other way.
        // Settling the stop first drops the pre-stop steering records straight out of the
        // intake log, so words the CEO had already watched appear as a bubble never became
        // a ledger turn: they survived until the next reload and then vanished. Draining
        // first means everything he typed becomes a real, durable turn, and the stop then
        // marks the ones it covers as stopped. His words stay on screen, correctly marked,
        // and a reload agrees with what he saw.
        self.drain_intake()?;
        self.settle_stop_claim(binding)?;
        self.raise_quota_notices(Some(binding.thread_id()));
        // Never start hidden model roundtrips after telling the user the request
        // finished. The next request performs this work under Working and Stop.
        Ok(())
    }





    /// Mid-turn-crash recovery + replay (continuity §5.3). The dead lease is dropped;
    /// a fresh one is spawned and re-primed (`prime_connection`, called from the
    /// nested `deliver`, naturally carries the just-interrupted turn forward — it is
    /// still `Interrupted` in the ledger at this point, so `reprime.rs`'s
    /// `pending_decisions` picks it up as "Unfinished: <text>" — plus the full action
    /// ledger, the anti-double-execution guard). The CEO's original prompt is then
    /// RE-SERVED as a brand-new turn; the failed turn is marked superseded (never edited
    /// in place — it stays in the ledger as the durable crash record) so the CEO sees
    /// ONE clean exchange, not a duplicate.
    /// **Was this turn's outcome an upstream model-API failure?** `None` for everything
    /// else, which is the answer this returns almost always.
    ///
    /// Three channels are read, in the order they are trustworthy:
    ///
    /// 1. the lease's own error `Display`, when it errored;
    /// 2. the assistant TEXT the ledger holds for this turn — the shape a `529` actually
    ///    takes on this wire, where the vendor reports it as a message rather than as a
    ///    transport failure;
    /// 3. nothing else. A `stop_reason` is a vendor terminal, not a diagnosis, and
    ///    inventing a fault from `error_during_execution` would classify every interrupted
    ///    turn as an outage.
    ///
    /// **The text channel is read line by line and only the LAST run is considered.** A
    /// turn whose real answer happened to quote an `API Error:` line — the CEO pasting a
    /// log and asking about it — must not be reported as an outage, and the failure always
    /// arrives as the final thing the lease said.
    fn detect_upstream_failure(
        &self,
        turn_id: &str,
        stop: &Result<String, CognitionError>,
    ) -> Option<crate::upstream::UpstreamFailure> {
        if let Err(e) = stop {
            if let Some(f) = crate::upstream::UpstreamFailure::classify_lines(&e.to_string()) {
                return Some(f);
            }
        }
        let turn = self.ledger.turn(turn_id)?;
        let last = turn.text_runs.last()?;
        crate::upstream::UpstreamFailure::classify_lines(&last.text)
    }

    /// **WHAT SURVIVED THIS TURN, read off the ledger rather than narrated.**
    ///
    /// Extracted from `finish_upstream_failure` on 2026-09-17, behavior unchanged, so the
    /// ordinary interruption path (D2) states what is on disk using the SAME counts the
    /// upstream path does. Two implementations of "what survived" is how the sentence and
    /// the screen come to disagree, and the half that drifted would be the half that tells
    /// a customer his work is safe.
    ///
    /// `lease_context_lost` is `true` unconditionally: whatever the session had worked out
    /// and had not yet said is gone with the lease, on every one of these endings.
    fn turn_loss(&self, turn_id: &str) -> crate::upstream::TurnLoss {
        let turn = self.ledger.turn(turn_id);
        crate::upstream::TurnLoss {
            prompt_is_durable: turn.map(|t| !t.user_text.is_empty()).unwrap_or(false),
            // The vendor's diagnostic is NOT part of what survived, so it is cut out of
            // the count by the SAME function the timeline projects with
            // (`upstream::split_at_vendor_diagnostic`). Telling him "169 characters of the
            // answer are saved" when all 169 are the error message would be a true number
            // about the wrong thing — and having two implementations of "which half is the
            // answer" would be worse: the sentence and the screen would eventually
            // disagree, and neither would be wrong on its own terms.
            partial_reply_chars: turn
                .map(|t| {
                    t.text_runs
                        .iter()
                        .map(|r| {
                            crate::upstream::split_at_vendor_diagnostic(&r.text).0.chars().count()
                        })
                        .sum()
                })
                .unwrap_or(0),
            actions_recorded: self
                .ledger
                .actions()
                .iter()
                .filter(|a| a.turn_id.as_deref() == Some(turn_id))
                .count(),
            lease_context_lost: true,
        }
    }

    /// **CLASSIFY AN ORDINARY INTERRUPTION AND MAKE THE ANSWER DURABLE — the nightly's D2.**
    ///
    /// Called immediately after `interrupt_turn`, never before it, per
    /// `Ledger::record_interruption_cause`'s stated contract.
    ///
    /// It is handed the SAME string that was written as the turn's `reason`, so the record
    /// and the reason cannot describe two different events. The returned statement is what
    /// the CEO is shown — assembled here from the authored sentences so the live event and a
    /// reload months later say the same thing.
    ///
    /// **A failure to write the record is NOT a failure of the turn.** The turn is already
    /// terminal and durable at this point; losing the explanation degrades the surface to
    /// its generic card, which is exactly the fallback a pre-2026-09-17 record gets. Raising
    /// here would convert a cosmetic loss into a lost error path.
    fn record_interruption(
        &mut self,
        turn_id: &str,
        reason: &str,
    ) -> (crate::interruption::InterruptionCause, String) {
        let cause = crate::interruption::classify(reason);
        let loss = self.turn_loss(turn_id);
        let record = crate::interruption::InterruptionRecord::new(cause, &loss);
        let statement=match &record.loss_message {Some(loss)=>format!("{} {loss}",record.ceo_message),None=>record.ceo_message.clone()};

        // misclassification is visible in a log without reading the ledger.
        eprintln!("[richos] turn interrupted [{}]: {reason}", cause.tag());
        if let Err(e) = self.ledger.record_interruption_cause(turn_id, &record) {
            eprintln!("[richos] the reason for that interruption could not be written down: {e}");
        }
        (cause, statement)
    }

    /// **Everything that happens when a turn dies to the upstream API, in the order the
    /// durability rules require.**
    ///
    /// 1. `interrupt_turn` — the turn ended, and that fact is written first. Its `reason`
    ///    stays the OPERATOR's summary (`upstream overloaded http=529 request_id=…`)
    ///    rather than the CEO's sentence, because `TimelineItem::SystemError` renders it
    ///    at Technical visibility and §21's rule for that record is unchanged.
    /// 2. `record_upstream_failure` — the classification and the three sentences the CEO
    ///    is about to be shown, made durable so a reload months later shows him what he
    ///    was told rather than what a later build would say.
    /// 3. the live events — the same statement, now.
    /// 4. the retry decision, LAST, because it is the only step that can start more work.
    ///
    /// **The loss statement is built from COUNTS read off the ledger at this instant**, so
    /// it cannot claim something survived that did not. `lease_context_lost` is `true`
    /// unconditionally and that is a fact about this failure class rather than a
    /// convenience: whatever the session had worked out and not yet said is gone with it.
    fn finish_upstream_failure(
        &mut self,
        turn_id: &str,
        binding: &ThreadBinding,
        original_text: &str,
        failure: crate::upstream::UpstreamFailure,
        _allow_recovery: bool,
    ) -> Result<(), SpineError> {
        let thread_id = binding.thread_id().to_string();
        eprintln!("[richos] {}", failure.summary());

        let loss = self.turn_loss(turn_id);

        // CHARGED BEFORE ANYTHING IS EMITTED, so the sentence the CEO reads carries the
        // attempt that just failed rather than the one before it.
        let budget=&mut self.resident.get_mut(binding.thread_id()).ok_or(SpineError::NoLease)?.upstream_budget;
        let may_retry=budget.charge(failure.fault);
        let record=crate::upstream::UpstreamRecord::new(&failure,&loss,budget);
        let wait=budget.wait_before_retry().unwrap_or_default();

        self.ledger.interrupt_turn(turn_id, &failure.summary())?;
        self.ledger.record_upstream_failure(turn_id, &record)?;

        // ONE statement, in the order a person needs it: what happened, what survived,
        // what was spent. `retry_message` already contains the fault's own sentence, so it
        // replaces the first clause rather than repeating it.
        let statement=record.retry_message.clone().unwrap_or_else(|| failure.raw.clone());
        self.emit(StreamEvent::TurnError {
            thread_id: thread_id.clone(),
            turn_id: turn_id.to_string(),
            reason: statement,
            at: now_millis(),
        });

        // AN OVERLOAD WAITS OUT HIS SCHEDULE AND RETRIES ON THE SAME SESSION (CEO, 2026-10-07):
        // 1, 2, 5, 10, 20, 40 and 80 minutes before retries 1 to 7. Bounded by the budget alone,
        // so it does not depend on `allow_recovery`, and it needs no lease factory: the lease
        // that met the `529` is kept and asked again, so its context is not lost.
        if may_retry && failure.fault == crate::upstream::UpstreamFault::Overloaded {
            self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Recovering, None));
            return self.retry_saved_input(turn_id, binding, original_text, wait);
        }
        self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Failed, None));
        self.emit_live(self.thread_summary_event(binding, turn_id, ThreadStatus::Failed));
        Err(SpineError::Cognition(CognitionError::Protocol(failure.summary())))
    }

    /// **The overload retry: wait, then ask the SAME lease again** (CEO, 2026-10-07).
    ///
    /// The replay turn is journaled first and published as the running turn, so his Stop (and
    /// a quit, which stops the running turn) reaches it during the wait: the wait ends at once
    /// and `deliver` then finds the stop before the prompt and ends the turn as stopped. The
    /// failed turn is superseded exactly as `recover_and_replay` supersedes one; unlike it, the
    /// lease is NOT cleared, so the session keeps what it had worked out.
    fn retry_saved_input(
        &mut self,
        failed_turn_id: &str,
        binding: &ThreadBinding,
        original_text: &str,
        wait: std::time::Duration,
    ) -> Result<(), SpineError> {
        let original_channel = self.input_channels.get(failed_turn_id).cloned()
            .or_else(|| self.ledger.turn(failed_turn_id).and_then(|t| t.channel.clone()));
        // One fsynced transition preserves provenance and retires the previous
        // attempt before either the retry wait or another process can recover it.
        let retry_turn_id = self.ledger.replay_turn(binding, failed_turn_id)?;
        if let Some(channel) = original_channel { self.input_channels.insert(retry_turn_id.clone(), channel); }
        self.emit_live(self.turn_status_event(
            binding, &retry_turn_id, TurnStatus::Queued, Some(failed_turn_id.to_string()),
        ));
        self.control.for_thread(binding.thread_id()).begin_turn(ActiveTurn {
            turn_id: retry_turn_id.clone(),
            thread_id: binding.thread_id().to_string(),
            entity_id: Some(binding.entity_id().clone()),
            started_at: None,
        });
        eprintln!("[richos] upstream overloaded: retrying in {} s on the same session", wait.as_secs());
        let mut delivery=self.prepare_delivery(&retry_turn_id,binding,original_text,true)?;
        delivery.retry_wait=Some((wait,self.retry_clock.clone()));
        self.dispatch_delivery(delivery)
    }



    pub fn rotation_count(&self)->u64 {self.rotation_count}
    pub fn last_rotation_reason(&self)->Option<&str> {self.last_rotation_reason.as_deref()}

    /// The shared account service exposed by this factory.
    fn quota_service(&self) -> Option<std::sync::Arc<crate::quota::Service>> {
        self.lease_factory.as_ref().and_then(|factory| factory.quota())
    }







    pub fn raise_proactive(
        &mut self,
        thread_id: Option<&str>,
        tier: AttentionTier,
        text: &str,
    ) -> Result<String, SpineError> {
        self.raise_proactive_identified(thread_id, tier, text, None)
    }

    fn raise_proactive_identified(
        &mut self, thread_id: Option<&str>, tier: AttentionTier, text: &str, delivery_id: Option<String>,
    ) -> Result<String, SpineError> {
        // A proactive message is a scoped WRITE like any other: the binding comes from the
        // ledger (never from the caller), so Rich cannot speak unprompted into an unbound
        // thread or into an entity the target thread does not belong to.
        let binding = match thread_id {
            Some(t) => self.ledger.thread_binding(t)?,
            None => self.ensure_active_thread()?,
        };
        let thread_id = binding.thread_id().to_string();
        // Durable regardless of tier or turn state — once Rich has "said" something
        // (even if Silent never renders it), it must survive a crash immediately after.
        let turn_id = match delivery_id {
            Some(id) => self.ledger.record_proactive_message_as(&binding, tier, text, id)?,
            None => self.ledger.record_proactive_message(&binding, tier, text)?,
        };

        // ACTION LEDGER (continuity §5.4 / §6.1): raising a proactive message is the one
        // genuinely CEO-FACING thing this app does on its own initiative today — Rich
        // reached out unprompted. Recording it is what makes the re-prime's
        // "ground truth for what Rich has done" claim TRUE rather than aspirational:
        //   - a successor can never deny having flagged something (false attribution), and
        //   - a successor can never re-raise the same thing (double execution), which for
        //     Tier 3/Silent is otherwise UNKNOWABLE — a Silent message has no render path
        //     at all (`messages()` skips it), so outside the action ledger there is no
        //     surface on which a successor could ever learn it already happened.
        // Recorded `Completed`, not `Claimed`: the durable, fsync'd ProactiveMessage
        // event above IS the execution — there is no second phase that could fail.
        self.ledger.record_action_with(
            Some(&turn_id),
            "proactive_message",
            &format!("[{}] {}", tier.as_str(), text),
            ActionVisibility::CeoFacing,
            ActionStatus::Completed,
        )?;

        // **SAID AT ONCE, EVEN WHILE A TURN RUNS** (round 16, state `switched-line`: Rich's
        // "Switched to Work: …" line sits above "• 3 agents working on Work", the row that
        // exists only while a turn runs). This used to be deferred to the turn's end so it
        // would not "collide with the working row"; round 16 draws the two together, and the
        // switch notice is raised INSIDE the turn that runs on the new account
        // (`select_connection`, after `ran_on`), so deferring it hid the line for exactly the
        // turn it describes. The message is its own turn, already durable above and already in
        // the published read view, so the window's reload draws it beside the running turn
        // without touching that turn's live state.
        if tier != AttentionTier::Silent {
            self.emit(StreamEvent::ProactiveMessage {
                thread_id,
                turn_id: turn_id.clone(),
                tier,
                at: now_millis(),
            });
            self.emit_proactive_live(&binding, &turn_id, tier, text);
        }
        Ok(turn_id)
    }

    fn emit_proactive_live(&self, binding: &ThreadBinding, turn_id: &str, tier: AttentionTier, text: &str) {
        let fence = EventFence::for_turn(binding, turn_id);
        self.emit_live(proactive_message_events(&fence, tier, text, now_millis()));
        self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Completed, None));
        // Said during a turn of this same thread, the thread is still working: the message
        // is complete, the turn beside it is not.
        let working = self.thread_is_running(binding.thread_id());
        let status = if working { ThreadStatus::Working } else { ThreadStatus::Idle };
        self.emit_live(self.thread_summary_event(binding, turn_id, status));
    }

    pub fn raise_quota_notices(&mut self, thread_id: Option<&str>) {
        let Some(quota) = self.quota_service() else { return };
        let thread = match thread_id {
            Some(thread) => thread.to_string(),
            None => match self.active.as_ref() {
                Some(binding) => binding.thread_id().to_string(),
                None => return,
            },
        };
        // The kind, when the line is about accounts (round 18): its turn is recorded, so the
        // conversation draws the buttons round 18 puts under it (See your accounts, Add a
        // second account, Not now).
        let mut pending = vec![
            (AttentionTier::Digest, Some("switched".to_string()), quota.accounts.take_notice()),
        ];
        pending.extend(quota.take_notes().into_iter().map(|note| (AttentionTier::Digest, None, Some(note))));
        pending.extend(quota.take_account_notes().into_iter().map(|(kind, note)| (AttentionTier::Digest, Some(kind), Some(note))));
        for (tier, kind, text) in pending {
            if let Some(text) = text {
                match self.raise_proactive(Some(&thread), tier, &text) {
                    Ok(turn) => if let Some(kind) = kind {
                        if let Err(error) = quota.accounts.noted(&kind, &turn) {
                            eprintln!("[richos] claude accounts: a line's buttons could not be recorded ({error})");
                        }
                    },
                    Err(error) => eprintln!("[richos] claude accounts: a notice could not be written ({error})"),
                }
            }
        }
    }


    pub fn open_conversation_with_message(
        &mut self,
        entity_id: &EntityId,
        title: &str,
        text: &str,
    ) -> Result<(String, String), SpineError> {
        if !self.registry.contains(entity_id) {
            return Err(SpineError::UnknownEntity(entity_id.to_string()));
        }
        let id = self.ledger.create_thread(title, entity_id)?;
        let turn = self.raise_proactive(Some(&id), AttentionTier::Digest, text)?;
        self.publish_read_metadata();
        Ok((id, turn))
    }

    pub fn replace_app_message(&mut self, thread_id: &str, turn_id: Option<&str>, text: &str, fact: &str) -> Result<String, SpineError> {
        let old_turn = match turn_id {
            Some(turn) => Some(turn.to_string()),
            None => self.ledger.thread_turns(thread_id)?.first().map(|turn| turn.id.clone()),
        };
        let new_turn = self.raise_proactive(Some(thread_id), AttentionTier::Digest, text)?;
        if let Some(old_turn) = old_turn {
            self.ledger.mark_turn_superseded(&old_turn, &new_turn)?;
        }
        let today = time::OffsetDateTime::now_utc().date();
        self.ledger.record_action_with(
            Some(&new_turn),
            "settings_change",
            &format!("{today}: {fact}"),
            ActionVisibility::CeoFacing,
            ActionStatus::Completed,
        )?;
        let entity = self.ledger.thread_binding(thread_id)?.entity_id().clone();
        let ledger = &self.ledger;
        let of_company = |thread: &str| ledger.thread_binding(thread).is_ok_and(|b| b.entity_id() == &entity);
        self.resident.retain(|thread, _| {
            let control=self.control.for_thread(thread);
            if !of_company(thread) || control.active_turn().is_some() {return true;}
            control.set_cancel(None);
            control.set_lease_session(None);
            false
        });
        if self.spare.as_ref().is_some_and(|spare| spare.entity == entity) {
            self.discard_spare();
        }
        self.publish_read_metadata();
        Ok(new_turn)
    }


    fn select_connection(&mut self,binding:&ThreadBinding)->Result<ConnectionPlan,SpineError> {
        Ok(ConnectionPlan{binding:binding.clone(),control:self.control.for_thread(binding.thread_id()),factory:self.lease_factory.clone()})
    }

    /// Synchronous headless entry. The desktop uses prime_front_desk_without_the_spine
    /// so native startup, handoff and priming never hold shared conversation state.
    pub fn prime_front_desk(&mut self,thread_id:&str)->FrontDeskReady {
        prime_front_desk_with(&mut &mut *self,thread_id)
    }

    pub fn queue_depth(&self) -> usize {
        0
    }

    fn thread_is_running(&self,thread:&str)->bool {
        self.turn_in_progress || self.control.for_thread(thread).active_turn().is_some() || self.control.for_thread(thread).front_desk_priming().is_some()
    }

    pub fn is_turn_in_progress(&self) -> bool {
        self.turn_in_progress || self.control.has_active_turns()
    }

    /// TEST-ONLY seam for headless fixtures. Desktop deliveries and preparation run
    /// independently outside shared state. This lets a synchronous fixture force a turn
    /// deterministically to exercise the proactive seam's deferred-emit branch (§ raise_proactive)
    /// without spinning up real threads. NOT part of the spine's real API contract.
    #[doc(hidden)]
    pub fn debug_set_turn_in_progress(&mut self, in_progress: bool) {
        self.turn_in_progress = in_progress;
    }
}
