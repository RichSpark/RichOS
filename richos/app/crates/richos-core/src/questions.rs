//! Durable, app-owned questions. No operation holds the conversation's turn lock.
//! Readers use atomic snapshots; the OS lock serializes writers.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, String>;
fn yes() -> bool {
    true
}
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn now() -> u64 {
    crate::util::now_millis()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionInput {
    pub label: String,
    pub description: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionInput {
    #[serde(alias = "question")]
    pub text: String,
    pub options: Vec<OptionInput>,
    #[serde(default, alias = "multiSelect")]
    pub multiple: bool,
    #[serde(default = "yes")]
    pub free_answer: bool,
    /// Zero-based index. The app mints option ids after validating the input.
    #[serde(default)]
    pub recommended: Option<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnswerOption {
    pub id: String,
    pub label: String,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Open,
    Answered,
    Withdrawn,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Answer {
    pub option_ids: Vec<String>,
    pub text: String,
    pub method: String,
    pub surface: String,
    pub at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub set_id: String,
    pub entity_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub asker: String,
    /// Selected work owner; the recipient may be the standing Stu session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obligation_id: Option<String>,
    pub session_id: String,
    pub text: String,
    pub options: Vec<AnswerOption>,
    pub multiple: bool,
    pub free_answer: bool,
    pub recommended: Option<String>,
    pub state: State,
    pub answer: Option<Answer>,
    pub revision: u64,
    pub delivered: bool,
    /// **Delivered into a back end's inbox, and not yet taken by it** (the work-path design D7,
    /// richos-hq `docs/plans/2026-09-27-work-path-answer-delivery-design.md`). Set with the
    /// receipt when the sink kept the answer in the inbox (his team's, or a work job's), cleared
    /// when the back end or his team takes it. `delivered` keeps its meaning for this store; the
    /// card is told `delivered` only once this is clear, so "Rich has your answer" is true when
    /// it is said.
    #[serde(default)]
    pub awaiting_taker: bool,
    /// **Its job ended while the answer was still untaken** (his Stop, or the second failed
    /// delivery try; Rich's ruling on escalation esc-20260927T220629Z-cbc90040). Set by
    /// [`crate::question_work::discard`] for an input no back end had taken, only while
    /// `awaiting_taker` holds, and cleared with it if a taker did have it after all. The card
    /// then says "This job stopped before Rich got your answer." instead of "On its way to Rich",
    /// which would never again be true.
    #[serde(default)]
    pub ended_before_taken: bool,
    #[serde(default)]
    pub handoff_started: bool,
    #[serde(default)]
    pub waiting_for_turn: bool,
    #[serde(default)]
    pub remaining: usize,
    #[serde(default)]
    pub set_index: usize,
    #[serde(default)]
    pub set_count: usize,
    pub withdrawal_reason: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub shown: Option<Shown>,
    /// The answer's place in the conversation's input order, taken when the app accepted it
    /// (`TurnControl::take_input_order`), so a Stop pressed after the click covers it even
    /// though the answer reaches Rich later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shown {
    pub surface: String,
    pub at: u64,
    pub witness_written: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerRequest {
    pub question_id: String,
    pub client_id: String,
    #[serde(default)]
    pub option_ids: Vec<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnswerResult {
    pub outcome: String,
    pub question: Option<Question>,
}
/// **The receipt of an answer his Stop covered** (review rv-20261018T204635Z-c68a30ec-f61e#1):
/// the recipient accepted it and stopped it before Rich got it, so the card must not say "Rich
/// has your answer". A sink prefixes the turn id with this; the card then reads as a job that
/// stopped before Rich got the answer.
pub const STOPPED_RECEIPT_PREFIX: &str = "stopped-before-rich:";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Delivery {
    pub id: String,
    pub entity_id: String,
    pub thread_id: String,
    pub asker: String,
    /// Selected work owner; the recipient may be the standing Stu session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obligation_id: Option<String>,
    pub set_id: Option<String>,
    pub text: String,
    /// Set when the recipient durably accepts this identity. Retries reuse it.
    pub receipt: Option<String>,
    /// The input order of the acceptance that made this delivery due (the latest answer of
    /// its set), carried to the recipient so his Stop is judged against the click, not the
    /// later delivery. `None` when the answer was accepted without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Operation {
    request: String,
    result: AnswerResult,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AskScope {
    pub root: PathBuf,
    pub entity_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub asker: String,
    /// Selected work owner; the recipient may be the standing Stu session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obligation_id: Option<String>,
    pub session_id: String,
    #[serde(default)]
    pub engine: Option<PathBuf>,
    #[serde(default)]
    pub entity_root: Option<PathBuf>,
    /// **The app run** his team's lead was started in: the claim id the app gives every lead
    /// of one launch (`RICHOS_OPERATOR_LEAD`, set by the retired operator client). The display witness writes
    /// it on the ledger line, so an ask one conversation's lead put to him satisfies the
    /// engine's ask-first gate for every lead of the same run (operator contract notes §4
    /// item 2, Frank's F10; engine `scripts/lib/ceo-asks.sh`, "THE ONE WIDENING"). `None`
    /// everywhere else, which keeps an ask keyed to its session alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_run: Option<String>,
}
#[derive(Default, Serialize, Deserialize)]
struct Data {
    questions: Vec<Question>,
    #[serde(default)]
    deliveries: Vec<Delivery>,
    #[serde(default)]
    operations: BTreeMap<String, Operation>,
    #[serde(default)]
    deleted: Vec<String>,
    #[serde(default)]
    scopes: BTreeMap<String, AskScope>,
    #[serde(default)]
    notified_sets: Vec<String>,
}
#[derive(Clone)]
pub struct Store {
    directory: PathBuf,
}
impl AnswerResult {
    pub fn public_value(&self) -> Value {
        json!({"outcome":self.outcome,"question":self.question.as_ref().map(Question::public_value)})
    }
}
impl Store {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            directory: root.as_ref().join("questions"),
        }
    }
    fn transaction<T>(&self, write: bool, f: impl FnOnce(&mut Data) -> Result<T>) -> Result<T> {
        std::fs::create_dir_all(&self.directory).map_err(|e| e.to_string())?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.directory.join("lock"))
            .map_err(|e| e.to_string())?;
        // Readers use the same atomic snapshot as pending_for_residency.
        if write {
            lock.lock().map_err(|e| e.to_string())?;
        }
        let path = self.directory.join("store.json");
        let mut data = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Data>(&bytes)
                .map_err(|e| format!("Question history could not be read: {e}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(e) => return Err(e.to_string()),
        };
        // Observers react to replacement of store.json. A no-op must not create
        // another event and restart delivery workers indefinitely while idle.
        let before = if write {
            Some(serde_json::to_vec(&data).map_err(|e| e.to_string())?)
        } else {
            None
        };
        let result = f(&mut data)?;
        if write
            && before.as_deref()
                != Some(
                    serde_json::to_vec(&data)
                        .map_err(|e| e.to_string())?
                        .as_slice(),
                )
        {
            let temp = self.directory.join(format!("{}.new", id()));
            let saved = (|| -> Result<()> {
                let mut file = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&temp)
                    .map_err(|e| e.to_string())?;
                serde_json::to_writer(&mut file, &data).map_err(|e| e.to_string())?;
                file.write_all(b"\n").map_err(|e| e.to_string())?;
                file.sync_all().map_err(|e| e.to_string())?;
                std::fs::rename(&temp, &path).map_err(|e| e.to_string())?;
                #[cfg(unix)]
                File::open(&self.directory)
                    .and_then(|f| f.sync_all())
                    .map_err(|e| e.to_string())?;
                Ok(())
            })();
            if saved.is_err() {
                drop(std::fs::remove_file(&temp));
            }
            saved?;
        }
        Ok(result)
    }
    pub fn list(&self, entity: &str, thread: &str) -> Result<Vec<Question>> {
        self.transaction(false, |d| {
            Ok(d.questions
                .iter()
                .filter(|q| q.entity_id == entity && q.thread_id == thread)
                .cloned()
                .collect())
        })
    }
    pub fn ask(&self, scope: &AskScope, inputs: Vec<QuestionInput>) -> Result<Vec<Question>> {
        self.ask_bounded(scope, inputs, std::time::Duration::from_secs(5))
    }
    pub fn ask_bounded(
        &self,
        scope: &AskScope,
        inputs: Vec<QuestionInput>,
        budget: std::time::Duration,
    ) -> Result<Vec<Question>> {
        if inputs.is_empty() || inputs.len() > 4 {
            return Err("Ask one to four questions.".into());
        }
        if [
            &scope.entity_id,
            &scope.thread_id,
            &scope.asker,
            &scope.session_id,
            &scope.turn_id,
        ]
        .iter()
        .any(|s| s.is_empty())
        {
            return Err("The app has not scoped this question.".into());
        }
        for input in &inputs {
            validate(input)?;
        }
        crate::question_engine::check(scope, &inputs, budget)?;
        let remaining = inputs.len();
        self.transaction(true, |d| {
            if d.deleted.contains(&scope.thread_id) {
                return Err("This conversation was deleted.".into());
            }
            if let Some(owner)=scope.obligation_id.as_deref() {
                let records=crate::assignment::read_all(&scope.root,&scope.entity_id,&scope.thread_id).map_err(|e|e.to_string())?;
                if records.iter().any(|r|r.obligation_id==owner && r.user_stop_requested_at_ms.is_some()) {
                    return Err("The user stopped this assignment.".into());
                }
            }
            let set_id = id();
            let questions: Vec<_> = inputs
                .into_iter()
                .enumerate()
                .map(|(index, input)| {
                    let options: Vec<_> = input
                        .options
                        .into_iter()
                        .map(|o| AnswerOption {
                            id: id(),
                            label: o.label,
                            description: o.description,
                        })
                        .collect();
                    Question {
                        id: id(),
                        set_id: set_id.clone(),
                        entity_id: scope.entity_id.clone(),
                        thread_id: scope.thread_id.clone(),
                        turn_id: scope.turn_id.clone(),
                        asker: scope.asker.clone(),
                        obligation_id: scope.obligation_id.clone(),
                        session_id: scope.session_id.clone(),
                        text: input.text,
                        recommended: input.recommended.map(|i| options[i].id.clone()),
                        options,
                        multiple: input.multiple,
                        free_answer: input.free_answer,
                        state: State::Open,
                        answer: None,
                        revision: 0,
                        delivered: false,
                        awaiting_taker: false,
                        ended_before_taken: false,
                        handoff_started: false,
                        waiting_for_turn: false,
                        remaining,
                        set_index: index + 1,
                        set_count: remaining,
                        withdrawal_reason: None,
                        order: None,
                        created_at: now(),
                        updated_at: now(),
                        shown: None,
                    }
                })
                .collect();
            d.scopes.insert(set_id, scope.clone());
            d.questions.extend(questions.clone());
            Ok(questions)
        })
    }
    /// The highest input order saved with an answer. A restarted app advances past it, so no
    /// new input is given the order of an answer that has not reached Rich yet.
    pub fn highest_order(&self) -> Result<Option<u64>> {
        self.transaction(false, |d| {
            Ok(d.questions
                .iter()
                .filter_map(|q| q.order)
                .chain(d.deliveries.iter().filter_map(|v| v.order))
                .max())
        })
    }
    pub fn answer(
        &self,
        entity: &str,
        thread: &str,
        request: AnswerRequest,
        method: &str,
        surface: &str,
    ) -> Result<AnswerResult> {
        self.answer_in_order(entity, thread, request, method, surface, None)
    }
    /// [`Self::answer`], saving `order` (taken from the conversation's input order when he
    /// clicked) with what this answer makes due, so a Stop pressed after it covers it.
    pub fn answer_in_order(
        &self,
        entity: &str,
        thread: &str,
        request: AnswerRequest,
        method: &str,
        surface: &str,
        order: Option<u64>,
    ) -> Result<AnswerResult> {
        if request.client_id.is_empty() || request.client_id.len() > 128 {
            return Err("An answer needs a stable submission id.".into());
        }
        if ![
            "click",
            "keyboard",
            "typed",
            "spoken",
            "phone_tap",
            "phone_typed",
            "phone_voice",
        ]
        .contains(&method)
            || !["mac", "phone"].contains(&surface)
        {
            return Err("Unknown answer source.".into());
        }
        let fingerprint = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        let key = format!("{entity}:{thread}:{}", request.client_id);
        self.transaction(true, |d| {
            if d.deleted.contains(&thread.to_string()) {
                return Ok(AnswerResult {
                    outcome: "conversation_deleted".into(),
                    question: None,
                });
            }
            if let Some(op) = d.operations.get(&key) {
                if op.request != fingerprint {
                    return Err("That submission id was already used for another answer.".into());
                }
                let mut result = op.result.clone();
                result.question = d
                    .questions
                    .iter()
                    .find(|q| {
                        q.id == request.question_id
                            && q.entity_id == entity
                            && q.thread_id == thread
                    })
                    .cloned();
                return Ok(result);
            }
            let q = d
                .questions
                .iter_mut()
                .find(|q| {
                    q.id == request.question_id && q.entity_id == entity && q.thread_id == thread
                })
                .ok_or("This question does not belong to this conversation.")?;
            let mut choices = request.option_ids.clone();
            choices.sort();
            choices.dedup();
            if choices.len() != request.option_ids.len()
                || choices
                    .iter()
                    .any(|id| !q.options.iter().any(|o| &o.id == id))
                || (!q.multiple && choices.len() > 1)
            {
                return Err("Choose only this question's available options.".into());
            }
            let text = request.text.trim().to_string();
            if (choices.is_empty() && text.is_empty())
                || text.len() > 16384
                || (!q.free_answer && !text.is_empty())
            {
                return Err("Choose an option or enter an allowed answer.".into());
            }
            let incoming = Answer {
                option_ids: choices,
                text,
                method: method.into(),
                surface: surface.into(),
                at: now(),
            };
            let same = q
                .answer
                .as_ref()
                .is_some_and(|a| a.option_ids == incoming.option_ids && a.text == incoming.text);
            let editable = q.state == State::Answered
                && !q.handoff_started
                && !q.delivered
                && request.expected_revision == Some(q.revision);
            let outcome;
            if same {
                outcome = "already_answered";
            } else if (q.state == State::Open
                && request.expected_revision.is_none_or(|revision| revision == q.revision))
                || editable
            {
                q.state = State::Answered;
                q.answer = Some(incoming);
                q.order = order;
                q.revision += 1;
                q.updated_at = now();
                outcome = "accepted";
            } else {
                let prefix = if q.state == State::Withdrawn {
                    "You answered a question Rich had already withdrawn"
                } else {
                    "You also answered"
                };
                let text = format!(
                    "{prefix} on {surface}: {}\n{}",
                    q.text,
                    q.render_answer(&incoming)
                );
                d.deliveries.push(Delivery {
                    id: id(),
                    entity_id: entity.into(),
                    thread_id: thread.into(),
                    asker: "front_desk".into(),
                    obligation_id: None,
                    set_id: None,
                    text,
                    receipt: None,
                    order,
                });
                outcome = if q.state == State::Withdrawn {
                    "withdrawn"
                } else {
                    "conflict"
                };
            }
            let set = q.set_id.clone();
            let remaining = d
                .questions
                .iter()
                .filter(|q| q.set_id == set && q.state == State::Open)
                .count();
            for question in d.questions.iter_mut().filter(|q| q.set_id == set) {
                question.remaining = remaining;
            }
            let result = AnswerResult {
                outcome: outcome.into(),
                question: d
                    .questions
                    .iter()
                    .find(|q| q.id == request.question_id)
                    .cloned(),
            };
            d.operations.insert(
                key,
                Operation {
                    request: fingerprint,
                    result: result.clone(),
                },
            );
            Ok(result)
        })
    }
    /// Project a saved front-desk answer waiting for the current reply boundary.
    pub fn waiting_for_turn(
        &self,
        entity: &str,
        thread: &str,
        question: &str,
        waiting: bool,
    ) -> Result<Option<Question>> {
        self.transaction(true, |d| {
            let set = d
                .questions
                .iter()
                .find(|q| q.id == question && q.entity_id == entity && q.thread_id == thread)
                .map(|q| q.set_id.clone());
            if let Some(set) = set {
                for q in d.questions.iter_mut().filter(|q| {
                    q.set_id == set
                        && q.asker == "front_desk"
                        && q.state == State::Answered
                        && !q.handoff_started
                        && !q.delivered
                }) {
                    q.waiting_for_turn = waiting;
                }
            }
            Ok(d.questions
                .iter()
                .find(|q| q.id == question && q.entity_id == entity && q.thread_id == thread)
                .cloned())
        })
    }
    pub fn withdraw(
        &self,
        entity: &str,
        thread: &str,
        asker: &str,
        question: &str,
        reason: &str,
    ) -> Result<()> {
        if reason.trim().is_empty() || reason.len() > 4096 {
            return Err("Explain why this question is no longer needed.".into());
        }
        self.transaction(true, |d| {
            let q = d
                .questions
                .iter_mut()
                .find(|q| {
                    q.id == question
                        && q.entity_id == entity
                        && q.thread_id == thread
                        && q.asker == asker
                })
                .ok_or("Only the asker can withdraw this question.")?;
            if q.state == State::Open {
                q.state = State::Withdrawn;
                q.withdrawal_reason = Some(reason.into());
                q.updated_at = now();
            }
            let set = q.set_id.clone();
            let remaining = d
                .questions
                .iter()
                .filter(|q| q.set_id == set && q.state == State::Open)
                .count();
            for question in d.questions.iter_mut().filter(|q| q.set_id == set) {
                question.remaining = remaining;
            }
            Ok(())
        })
    }
    pub fn close(
        &self,
        entity: &str,
        thread: &str,
        asker: Option<&str>,
        reason: &str,
        deleted: bool,
    ) -> Result<()> {
        self.transaction(true, |d| {
            if deleted && !d.deleted.contains(&thread.to_string()) {
                d.deleted.push(thread.into());
            }
            for q in &mut d.questions {
                if q.entity_id == entity
                    && q.thread_id == thread
                    && asker.is_none_or(|a| q.asker == a || q.obligation_id.as_deref() == Some(a))
                    && !q.delivered
                {
                    q.state = State::Withdrawn;
                    q.withdrawal_reason = Some(reason.into());
                    q.remaining = 0;
                    q.waiting_for_turn = false;
                    q.updated_at = now();
                }
            }
            for delivery in &mut d.deliveries {
                if delivery.entity_id == entity
                    && delivery.thread_id == thread
                    && asker.is_none_or(|a| delivery.asker == a || delivery.obligation_id.as_deref() == Some(a))
                    && delivery.receipt.is_none()
                {
                    delivery.receipt = Some("target_closed".into());
                }
            }
            Ok(())
        })
    }
    /// The sink must durably deduplicate `Delivery.id`. The store lock orders handoff
    /// against edits/deletion. On a crash before our receipt, the same identity is retried.
    pub fn deliver(
        &self,
        entity: &str,
        thread: &str,
        asker: &str,
        mut sink: impl FnMut(&Delivery) -> Result<String>,
    ) -> Result<usize> {
        self.deliver_matching(entity, thread, asker, None, &mut sink)
    }
    pub fn deliver_set(
        &self,
        entity: &str,
        thread: &str,
        asker: &str,
        set: &str,
        mut sink: impl FnMut(&Delivery) -> Result<String>,
    ) -> Result<usize> {
        self.deliver_matching(entity, thread, asker, Some(set), &mut sink)
    }
    fn deliver_matching(
        &self,
        entity: &str,
        thread: &str,
        asker: &str,
        only_set: Option<&str>,
        sink: &mut dyn FnMut(&Delivery) -> Result<String>,
    ) -> Result<usize> {
        self.transaction(true, |d| {
            if d.deleted.contains(&thread.to_string()) {
                return Ok(Vec::new());
            }
            let sets: std::collections::BTreeSet<_> = d
                .questions
                .iter()
                .filter(|q| {
                    q.entity_id == entity
                        && q.thread_id == thread
                        && q.asker == asker
                        && !q.handoff_started
                        && !q.delivered
                })
                .map(|q| q.set_id.clone())
                .collect();
            for set in sets {
                let qs: Vec<_> = d.questions.iter().filter(|q| q.set_id == set).collect();
                if qs.iter().any(|q| q.state == State::Open)
                    || !qs.iter().any(|q| q.state == State::Answered)
                {
                    continue;
                }
                let text = qs
                    .iter()
                    .map(|q| q.resolved_text())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                // The set became due with its latest answer; an answer saved without an
                // order leaves the recipient to take one, as before.
                let order = qs
                    .iter()
                    .filter(|q| q.state == State::Answered)
                    .map(|q| q.order)
                    .collect::<Option<Vec<_>>>()
                    .and_then(|orders| orders.into_iter().max());
                let delivery_id = format!("question-set:{set}");
                if let Some(delivery) = d.deliveries.iter_mut().find(|v| v.id == delivery_id) {
                    if delivery.receipt.is_none() {
                        delivery.text = text;
                        delivery.order = order;
                    }
                } else {
                    d.deliveries.push(Delivery {
                        id: delivery_id,
                        entity_id: entity.into(),
                        thread_id: thread.into(),
                        asker: asker.into(),
                        obligation_id: qs.first().and_then(|q| q.obligation_id.clone()),
                        set_id: Some(set),
                        text,
                        receipt: None,
                        order,
                    });
                }
            }
            let pending: Vec<_> = d
                .deliveries
                .iter()
                .filter(|delivery| {
                    delivery.entity_id == entity
                        && delivery.thread_id == thread
                        && delivery.asker == asker
                        && delivery.receipt.is_none()
                        && only_set.is_none_or(|set| delivery.set_id.as_deref() == Some(set))
                })
                .cloned()
                .collect();
            // Freeze the durable receiving input before handing it across stores. A crash
            // can retry its identity, but an edit cannot rewrite an uncertain handoff.
            for delivery in &pending {
                if let Some(set) = &delivery.set_id {
                    for q in d.questions.iter_mut().filter(|q| &q.set_id == set) {
                        if !q.handoff_started {
                            q.handoff_started = true;
                            q.waiting_for_turn = false;
                            q.updated_at = now();
                        }
                    }
                }
            }
            Ok(pending)
        })
        .and_then(|pending| {
            let mut count = 0;
            for delivery in pending {
                count += self.transaction(true, |d| {
                    // This lock orders deletion against the receiving inbox write. No
                    // model work runs here; sinks only durably enqueue this identity.
                    if d.deleted.contains(&delivery.thread_id)
                        || !d
                            .deliveries
                            .iter()
                            .any(|v| v.id == delivery.id && v.receipt.is_none())
                    {
                        return Ok(0);
                    }
                    let receipt = sink(&delivery)?;
                    let stopped = receipt.starts_with(STOPPED_RECEIPT_PREFIX);
                    if let Some(saved) = d.deliveries.iter_mut().find(|v| v.id == delivery.id) {
                        saved.receipt = Some(receipt);
                    }
                    // Still in a receiving inbox, not yet taken (D7). Read here, under this lock:
                    // a taker clears it only through this store's lock, so it cannot be cleared
                    // before it is set.
                    let awaiting = self.directory.parent()
                        .is_some_and(|root| crate::question_work::waiting(root, &delivery.id));
                    if let Some(set) = &delivery.set_id {
                        for q in d.questions.iter_mut().filter(|q| &q.set_id == set) {
                            q.delivered = true;
                            // Covered by his Stop: no one will take it, and the card says so.
                            q.awaiting_taker = awaiting || stopped;
                            q.ended_before_taken |= stopped;
                            q.updated_at = now();
                        }
                    }
                    Ok(1)
                })?;
            }
            Ok(count)
        })
    }
    /// **The back end (or his team) took these sets' answers** (design D7): the card may now say
    /// so. Never called while holding a work back end's `inner` lock: `queue_question_answer`
    /// takes that lock under this store's lock.
    pub fn taker_has(&self, sets: &[String]) -> Result<()> {
        if sets.is_empty() {
            return Ok(());
        }
        self.transaction(true, |d| {
            for q in d.questions.iter_mut().filter(|q| q.awaiting_taker && sets.contains(&q.set_id)) {
                q.awaiting_taker = false;
                q.ended_before_taken = false;
                q.updated_at = now();
            }
            Ok(())
        })
    }
    /// **These sets' job ended before any back end took their answers** (Rich's ruling on
    /// esc-20260927T220629Z-cbc90040): the card must stop saying "On its way to Rich". Only a
    /// question still awaiting its taker is marked. Same lock rule as [`Self::taker_has`]: never
    /// called under this store's lock or a work back end's `inner`.
    pub fn ended_before_taken(&self, sets: &[String]) -> Result<()> {
        if sets.is_empty() {
            return Ok(());
        }
        self.transaction(true, |d| {
            for q in d.questions.iter_mut().filter(|q| {
                q.awaiting_taker && !q.ended_before_taken && sets.contains(&q.set_id)
            }) {
                q.ended_before_taken = true;
                q.updated_at = now();
            }
            Ok(())
        })
    }
    /// Enqueue one awareness notification per new set. The sink persists its own
    /// stable event id before returning; a crash before this receipt safely repeats it.
    pub fn notify_sets(&self, mut sink: impl FnMut(&Question) -> Result<bool>) -> Result<()> {
        let pending = self.transaction(false, |d| {
            let mut sets = std::collections::BTreeSet::new();
            Ok(d.questions
                .iter()
                .filter(|q| {
                    q.state == State::Open
                        && !d.notified_sets.contains(&q.set_id)
                        && sets.insert(q.set_id.clone())
                })
                .cloned()
                .collect::<Vec<_>>())
        })?;
        for q in pending {
            if sink(&q)? {
                self.transaction(true, |d| {
                    if !d.notified_sets.contains(&q.set_id) {
                        d.notified_sets.push(q.set_id);
                    }
                    Ok(())
                })?;
            }
        }
        Ok(())
    }
    /// Read the atomically replaced snapshot without waiting on a writer's OS lock.
    /// Window-exit callbacks must answer immediately, including during answer handoff.
    /// Missing history is idle; unreadable history remains an error, never an idle claim.
    pub fn pending_for_residency(&self) -> Result<bool> {
        let bytes = match std::fs::read(self.directory.join("store.json")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.to_string()),
        };
        let data: Data = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        Ok(data.questions.iter().any(|q| q.state == State::Open
            || q.state == State::Answered && !q.delivered)
            || data.deliveries.iter().any(|delivery| delivery.receipt.is_none()))
    }
    pub fn all(&self) -> Result<Vec<Question>> {
        self.transaction(false, |d| Ok(d.questions.clone()))
    }
    pub fn pending_threads(&self) -> Result<Vec<(String, String, String)>> {
        self.transaction(false, |d| {
            let mut out = std::collections::BTreeSet::new();
            for q in &d.questions {
                if !q.delivered
                    && q.state == State::Answered
                    && !d
                        .questions
                        .iter()
                        .any(|other| other.set_id == q.set_id && other.state == State::Open)
                {
                    out.insert((q.entity_id.clone(), q.thread_id.clone(), q.asker.clone()));
                }
            }
            for v in &d.deliveries {
                if v.receipt.is_none() {
                    out.insert((v.entity_id.clone(), v.thread_id.clone(), v.asker.clone()));
                }
            }
            Ok(out.into_iter().collect())
        })
    }
    pub fn acknowledge(
        &self,
        entity: &str,
        thread: &str,
        question: &str,
        surface: &str,
    ) -> Result<()> {
        if !["mac", "phone"].contains(&surface) {
            return Err("Unknown display surface.".into());
        }
        self.transaction(true, |d| {
            let q = d
                .questions
                .iter_mut()
                .find(|q| q.id == question && q.entity_id == entity && q.thread_id == thread)
                .ok_or("Unknown question.")?;
            if q.shown.is_none() {
                q.shown = Some(Shown {
                    surface: surface.into(),
                    at: now(),
                    witness_written: false,
                });
            }
            Ok(())
        })
        // The observer runs the engine writer separately. A phone's receipt must
        // not hold its answer queue behind engine execution or a retryable failure.
    }
    pub fn flush_witnesses(&self) -> Result<()> {
        let pending = self.transaction(false, |d| {
            d.questions
                .iter()
                .filter(|q| q.shown.as_ref().is_some_and(|s| !s.witness_written))
                .map(|q| {
                    let scope = d
                        .scopes
                        .get(&q.set_id)
                        .ok_or("Question attribution is missing.")?;
                    Ok((q.clone(), scope.clone()))
                })
                .collect::<Result<Vec<_>>>()
        })?;
        for (q, scope) in pending {
            // Engine execution never holds the answer store lock. Its writer deduplicates
            // by question and original session when two surfaces acknowledge together.
            crate::question_engine::witness(&scope, &q)?;
            self.transaction(true, |d| {
                if let Some(q) = d.questions.iter_mut().find(|v| v.id == q.id) {
                    if let Some(shown) = q.shown.as_mut() {
                        shown.witness_written = true;
                    }
                }
                Ok(())
            })?;
        }
        Ok(())
    }
}
impl Question {
    pub fn render(&self) -> String {
        let actor = if self.asker == "front_desk" {
            "Rich"
        } else {
            "Your team"
        };
        let mut lines = if self.asker == "front_desk" {
            vec![self.text.clone()]
        } else {
            vec!["Your team, through Rich".into(), self.text.clone()]
        };
        lines.extend(
            self.options
                .iter()
                .map(|o| format!("{}: {}", o.label, o.description)),
        );
        if let Some(o) = self
            .options
            .iter()
            .find(|o| Some(&o.id) == self.recommended.as_ref())
        {
            lines.push(format!("{actor} recommends {}", o.label));
        }
        lines.join("\n")
    }
    pub fn render_answer(&self, answer: &Answer) -> String {
        let mut parts: Vec<_> = self
            .options
            .iter()
            .filter(|o| answer.option_ids.contains(&o.id))
            .map(|o| o.label.clone())
            .collect();
        if !answer.text.is_empty() {
            parts.push(answer.text.clone());
        }
        parts.join("; ")
    }
    pub fn resolved_text(&self) -> String {
        match &self.answer {
            Some(answer) if self.state == State::Answered => format!(
                "{}\nYou answered: {} ({} on {})",
                self.text,
                self.render_answer(answer),
                answer.method,
                answer.surface
            ),
            _ => format!(
                "{}\nRich withdrew this question: {}",
                self.text,
                self.withdrawal_reason
                    .as_deref()
                    .unwrap_or("No longer needed")
            ),
        }
    }
    pub fn public_value(&self) -> Value {
        json!({"id":self.id,"set_id":self.set_id,"thread_id":self.thread_id,"text":self.text,"options":self.options,"multiple":self.multiple,"free_answer":self.free_answer,"recommended":self.recommended,"state":self.state,"answer":self.answer,"revision":self.revision,"delivered":self.delivered && !self.awaiting_taker,"ended_before_taken":self.ended_before_taken && self.awaiting_taker,"handoff_started":self.handoff_started,"waiting_for_turn":self.waiting_for_turn,"remaining":self.remaining,"set_index":self.set_index,"set_count":self.set_count,"asker":if self.asker=="front_desk"{"Rich"}else{"Your team"},"withdrawal_reason":self.withdrawal_reason})
    }
    pub fn item(&self, revision: u64) -> Value {
        json!({"kind":"question","id":self.id,"entityId":self.entity_id,"threadId":self.thread_id,"turnId":self.turn_id,"bindingRevision":revision,
            "visibility":"ceo","createdAt":self.created_at,"sequence":self.created_at,"text":self.render(),"question":self.public_value()})
    }
}
fn validate(q: &QuestionInput) -> Result<()> {
    if q.text.trim().is_empty() || q.text.len() > 4096 {
        return Err("Write a question of at most 4096 bytes.".into());
    }
    if !(2..=4).contains(&q.options.len()) {
        return Err("Provide two to four options.".into());
    }
    if q.recommended.is_some_and(|i| i >= q.options.len()) {
        return Err("The recommended option must exist.".into());
    }
    let mut labels = std::collections::BTreeSet::new();
    for o in &q.options {
        let label = o.label.trim().to_lowercase().replace('’', "'");
        if label.is_empty()
            || o.label.len() > 256
            || o.description.trim().is_empty()
            || o.description.len() > 2048
        {
            return Err(
                "Each option needs a short label and a description of its tradeoff.".into(),
            );
        }
        if !labels.insert(label.clone()) {
            return Err("Use distinct labels, including when spoken without case.".into());
        }
        let words: Vec<_> = label
            .split(|c: char| !c.is_alphanumeric() && c != '\'')
            .filter(|s| !s.is_empty())
            .collect();
        if words.iter().any(|w| {
            [
                "i", "i'll", "me", "my", "we", "our", "us", "above", "below", "here", "left",
                "right",
            ]
            .contains(w)
        }) || [
            "this one",
            "that one",
            "the first",
            "the second",
            "the last",
        ]
        .iter()
        .any(|p| label.contains(p))
            || words
                .windows(2)
                .any(|w| w[0] == "option" && w[1].chars().all(|c| c.is_ascii_digit()))
        {
            return Err("Name the actor or outcome in each label. Remove first-person pronouns and positional references so the choice works when spoken.".into());
        }
    }
    Ok(())
}
