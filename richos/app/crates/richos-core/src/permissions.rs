//! Native provider permission requests surfaced in the desktop. One exact action,
//! one visible turn — and, since the background-work spec's §5.2, one exact action that
//! can WAIT for him when there is no visible turn at all.
//!
//! # What changed, and why it is a change of IDENTITY rather than of a refusal
//!
//! The background-work spec (richos-hq `docs/plans/background-work-spec-2026-09-17.md`
//! revision 5) §5.1 states the position this file used to be in: *"a worker's permission
//! request with no open turn is denied, not queued"*. §5.2 decides the replacement —
//! *"a request raised with no turn open is queued for his next look, and never
//! auto-approved"* — and §5.3 names the part that is easy to miss: the grant is not a flag,
//! it is a whole [`Binding`], and **a request cannot outlive its `turn_id`**. So the work
//! lease's binding carries a stable identity that is not a turn id — the ASSIGNMENT — and
//! this desk compares against that.
//!
//! **The two audiences are therefore held to two different liveness tests, and the
//! difference is the whole of §5.3:**
//!
//! | Audience | Alive while… | Shown on |
//! |---|---|---|
//! | the conversation (`audience != "worker"`) | the grant file still equals the request's binding — a turn | the permission sheet ([`PermissionDesk::current`]) |
//! | background work (`audience == "worker"`) | the ASSIGNMENT is still open, which [`PermissionDesk::forget`] is told; a request is admitted while the turn grant OR the standing worker grant is open | the assignment surface ([`PermissionDesk::background_queue`]) |
//!
//! A background request deliberately survives its own grant file being closed, because the
//! work lease's turn ends while he is away and `revoke_work_assignment` closes the grant on
//! the way out (`native.rs`). Gating his queue on that file would have deleted the request
//! seconds after it was raised — the failure §5.7 is written against, wearing a different
//! hat.
//!
//! # The 300-second deadline stops the CALL, not the REQUEST (§5.7)
//!
//! *"a request raised while he is at lunch is denied long before he looks, and 7.6 fails."*
//! The provider call still ends at its deadline and still ends in **not approved** — nothing
//! holds a provider tool call open for hours. What is new is that the entry stays in the
//! queue with [`Queued::call_returned`] set, so when he answers, the answer applies to the
//! ASSIGNMENT: [`Answered::ToAssignment`] hands it to the work host, which resumes the
//! assignment, and [`Standing`] lets that resumed run take the one exact action he approved.
//!
//! **A standing decision is not an auto-approval and the difference is mechanical.** It
//! matches ONE assignment, ONE tool and ONE exact input, it is consumed on first use, and it
//! dies with the assignment ([`PermissionDesk::forget`]). Nothing here ever decides on his
//! behalf: with no answer from him there is no standing decision, and the refusal is a
//! refusal.
//!
//! # WHAT THE QUEUE IS FOR, AFTER 2026-09-18 (CEO ruling §52)
//!
//! *"There's nothing that ever not lands on its own here in the terminal … So, yes, always
//! land on its own."* Until that ruling, the one thing this queue reliably held was a
//! background lease's `mcp__richos_work__integrate` call, and the whole "ready for you to
//! approve" state was built around it. It is now granted to a background lease outright,
//! beside the four work tools that were already allowed, so **no land ever reaches this
//! desk**.
//!
//! The queue is unchanged and it is not vestigial: it exists for a request the desk would
//! have put in front of him in a visible turn and cannot, because there is no turn — a
//! `Bash` command, a write outside the workspace, any tool with no standing grant. That is
//! the whole of its remaining purpose, and every test below that used `integrate` as its
//! convenient example of a waiting request now uses one of those instead, because an
//! example that no longer waits would have quietly turned each of those tests green for the
//! wrong reason.
use crate::{ecs::Binding, native::PermissionDecision};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::{Path, PathBuf}, sync::{Arc, Condvar, Mutex}, time::{Duration, Instant}};

#[derive(Clone, Debug, Serialize)]
pub struct PermissionRequest {
    pub id: String,
    pub binding: Binding,
    pub tool: String,
    pub input: Value,
    pub description: String,
    pub reason: String,
    /// When it was raised. The surface uses it to keep his queue in the order the work
    /// asked, which is the order §5.5 shows it in.
    pub raised_at_ms: u64,
}

/// **The audience a work lease binds under** (`ecs.rs`'s `bind_work_seat`, spec §5.8c). The
/// one field that tells this desk whether a request belongs to a turn he is watching or to
/// an assignment he may be away from.
pub const WORK_AUDIENCE: &str = "worker";

/// Is this request background work's rather than the conversation's? The identity test
/// §5.3 turns on, in one place so no caller re-spells it.
pub fn is_background(binding: &Binding) -> bool {
    binding.audience == WORK_AUDIENCE
}

/// The assignment a background binding belongs to: `(entity, thread, obligation)`.
///
/// `turn_id` is the OBLIGATION on a work seat and never a turn — `ecs.rs`'s `bind_work_seat`
/// says why in its own comment, and the engine's seat reconciler reads exactly that field
/// (`richos/engine/mega-lander/app.py:704`). This is the identity §5.3 requires and the key
/// [`PermissionDesk::forget`] is given.
pub fn assignment_key(binding: &Binding) -> (String, String, String) {
    (binding.entity_id.clone(), binding.thread_id.clone(), binding.turn_id.clone())
}

#[derive(Debug)]
struct Queued {
    request: PermissionRequest,
    scope: PathBuf,
    decision: Option<bool>,
    /// The provider call that raised this request has already ended at its 300 s deadline
    /// (§5.7). The REQUEST is still his to answer; there is simply nobody left holding the
    /// line for it, so his answer goes to the assignment instead of to a call.
    call_returned: bool,
}

/// A decision he gave after the call had already returned — §5.7's *"when he answers, the
/// answer applies to the assignment, not to a call that has long since returned"*.
///
/// Matched on the assignment, the tool AND the exact input, used once, and dropped with the
/// assignment. It is the mechanism by which an approval he gave at 9pm reaches the step that
/// asked for it at 2pm, and it is deliberately the narrowest thing that can do that.
#[derive(Debug)]
struct Standing {
    key: (String, String, String),
    tool: String,
    input: Value,
    allow: bool,
}

#[derive(Debug, Default)]
struct Desk {
    /// Ordered, oldest first. §5.5: *"its single `Option<Pending>` becomes an ordered queue
    /// whose head is the one shown"*. Both audiences share one queue and one desk — §2.7's
    /// one-desk argument — and each surface shows the head of the requests it owns.
    queue: Vec<Queued>,
    standing: Vec<Standing>,
}

#[derive(Debug, Default)]
pub struct PermissionDesk { inner: Mutex<Desk>, changed: Condvar }
#[derive(Clone, Debug)]
pub struct ScopedPermissions { pub desk: Arc<PermissionDesk>, pub scope: PathBuf }

/// What [`PermissionDesk::resolve`] did with his answer — and the second arm is the one
/// §5.7 exists for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answered {
    /// The call that raised it is still waiting and has taken the decision itself.
    Delivered,
    /// The call had already ended at its deadline. His answer now belongs to the assignment
    /// named by this binding: the work host resumes it (approved) or records that he
    /// declined the step (declined).
    ToAssignment { binding: Binding, allow: bool },
}

/// What a background call is told at its deadline. **It is written for the model, not for
/// him**, and it says the two things a model must not get wrong: this was NOT approved, and
/// do not try again — it is his and he still has it.
const NOT_APPROVED_YET: &str =
    "This is waiting for the CEO to approve it and has not been approved. Do not retry it \
     and do not work around it: the request is in his queue and he will answer it. End your \
     turn now and say what is waiting on him; the app gives you another turn when he answers.";

/// What a background call is told when an EARLIER step of the same assignment is already
/// waiting on him with no call left to take his answer. Written for the model, like the one
/// above: nothing ran, nothing was put in front of him, and the one useful move is to end
/// the turn so his answer can be carried out.
const STEP_ALREADY_WAITING: &str =
    "Not run. An earlier step of this assignment is waiting for the CEO to approve it, and \
     nothing else of this assignment runs until he answers. End your turn now and say what \
     is waiting on him; the app gives you another turn when he answers, and you can ask for \
     this then if it is still needed.";

/// **How long one permission call waits for him: 300 s.** A background call then ends in
/// *not approved* and the request stays on his screen (§5.7).
///
/// `RICHOS_PERMISSION_DEADLINE_MS` can SHORTEN it, to between 1 s and 300 s, and nothing can
/// lengthen it. It exists for one caller: the test VM's walk of the late-approval path
/// (`76e977dc`), which otherwise has to wait out the whole 300 s before his Approve can
/// arrive late. The same shape as `RICHOS_CANCEL_GRACE_MS` (`native.rs`). The app's launcher
/// sets no such variable, so an install behaves exactly as before; an override in effect is
/// said on stderr at every call, so a log shows it.
pub const PERMISSION_DEADLINE: Duration = Duration::from_secs(300);

pub fn permission_deadline() -> Duration {
    let value = std::env::var("RICHOS_PERMISSION_DEADLINE_MS").ok();
    let deadline = permission_deadline_from(value.as_deref());
    if deadline != PERMISSION_DEADLINE {
        eprintln!("[richos] permission calls wait {} ms (RICHOS_PERMISSION_DEADLINE_MS)", deadline.as_millis());
    }
    deadline
}

fn permission_deadline_from(value: Option<&str>) -> Duration {
    match value.and_then(|v| v.trim().parse::<u64>().ok()) {
        Some(ms) if (1_000..=300_000).contains(&ms) => Duration::from_millis(ms),
        _ => PERMISSION_DEADLINE,
    }
}

/// What a background call is told when its assignment stopped underneath it.
const ASSIGNMENT_GONE: &str =
    "That assignment was stopped, so this action was not approved.";

#[derive(Deserialize)]
struct Grant {
    version: u32,
    actions_allowed: bool,
    /// `ecs::ToolScope::background_work_allowed`: the work lease's standing grant for the
    /// WORKERS it has dispatched, which an ordinary turn end leaves open and only a stop
    /// (`ecs::revoke`) closes. Absent means false.
    #[serde(default)]
    background_work_allowed: bool,
    binding: Binding,
}
/// **The binding a request on this scope is raised under, or `None` if the scope is closed.**
///
/// A turn's grant (`actions_allowed`) opens it for either audience. A background binding is
/// ALSO open while its standing worker grant is: a worker the back end dispatched runs
/// between the lease's turns by design (`prepare` launches it in the background and the
/// lead's turn ends at once), so `actions_allowed` is closed for every call it makes. The
/// engine hook already lets such a call through on the same flag
/// (`engine/scripts/app-engine-hook.py`); reading only the turn flag here denied the
/// worker's `Bash` at once with *"This app turn is stopped"*, so it never reached the desk,
/// its row never had `awaitingYou`, and no Approve button could appear
/// (esc-20261005T150541Z-ee581ad7). The CEO's stop revokes both flags in one write, so a
/// stopped assignment still gets nothing.
fn grant(path: &Path) -> Option<Binding> {
    if std::fs::metadata(path).ok()?.len() > crate::ecs::SCOPE_LIMIT {return None;}
    let grant: Grant=serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let open = grant.actions_allowed || (grant.background_work_allowed && is_background(&grant.binding));
    (grant.version == 1 && open).then_some(grant.binding)
}
fn permission_reason(request: &Value) -> String {
    if let Some(reason)=request["decision_reason"].as_str().filter(|s|!s.is_empty()) {return reason.into();}
    match request["decision_reason_type"].as_str() {
        Some("safetyCheck") => "The provider requires manual approval for this command form.".into(),
        Some(kind) => format!("The provider requires approval ({kind})."),
        None => String::new(),
    }
}
fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
impl PermissionDesk {
    /// **The head of the CONVERSATION's queue** — what the permission sheet shows.
    ///
    /// Background requests are deliberately NOT here. A modal that opened over his screen
    /// because something he is not watching reached a step would be an interruption, and
    /// the standing rule for background results is that he is told when he is next
    /// listening rather than mid-sentence (spec §0 row 6, §3.5). Their surface is the
    /// assignment they belong to — [`Self::background_queue`].
    pub fn current(&self) -> Option<PermissionRequest> {
        self.inner.lock().unwrap().queue.iter()
            .find(|q| !is_background(&q.request.binding)
                && grant(&q.scope).as_ref() == Some(&q.request.binding)
                && q.decision.is_none())
            .map(|q| q.request.clone())
    }

    /// **Everything background work is waiting on him for, oldest first** (§5.5).
    ///
    /// One entry per request, not per assignment: the surface picks the head for each
    /// assignment it renders, which is the same rule the sheet applies to the conversation.
    /// Nothing here is filtered by a grant file — see the module doc's table.
    pub fn background_queue(&self) -> Vec<PermissionRequest> {
        self.inner.lock().unwrap().queue.iter()
            .filter(|q| is_background(&q.request.binding) && q.decision.is_none())
            .map(|q| q.request.clone())
            .collect()
    }

    /// The same queue, **scoped to one conversation** — the Two Riches spec puts a whole
    /// back-end Rich behind each thread, and Sage's check of it (finding 12) names the desk's
    /// single un-scoped slot as the thing that has to change with it.
    ///
    /// Every entry already carries its own thread in its binding, so this is a reader rather
    /// than a second store: one desk, one ordered queue, and each conversation sees the part
    /// of it that is about its own work.
    pub fn background_queue_on(&self, thread: &str) -> Vec<PermissionRequest> {
        self.background_queue().into_iter().filter(|request| request.binding.thread_id == thread).collect()
    }

    /// His answer to one exact request.
    ///
    /// The conversation's arm is unchanged: the request must still match a live grant, or
    /// the turn it belonged to has gone and the old request cannot be approved. The
    /// background arm is §5.7's: if the call is still waiting it takes the decision; if the
    /// call already ended at its deadline the decision goes to the assignment.
    pub fn resolve(&self, id: &str, allow: bool) -> Result<Answered, String> {
        let mut desk = self.inner.lock().unwrap();
        let index = desk.queue.iter().position(|q| q.request.id == id)
            .ok_or_else(|| "That action is no longer waiting for permission.".to_string())?;
        if desk.queue[index].decision.is_some() {
            return Err("That action's turn has stopped or changed. The old request cannot be approved.".into());
        }
        let background = is_background(&desk.queue[index].request.binding);
        if !background && grant(&desk.queue[index].scope).as_ref() != Some(&desk.queue[index].request.binding) {
            return Err("That action's turn has stopped or changed. The old request cannot be approved.".into());
        }
        if background && desk.queue[index].call_returned {
            let entry = desk.queue.remove(index);
            let binding = entry.request.binding.clone();
            desk.standing.push(Standing {
                key: assignment_key(&binding),
                tool: entry.request.tool.clone(),
                input: entry.request.input.clone(),
                allow,
            });
            self.changed.notify_all();
            return Ok(Answered::ToAssignment { binding, allow });
        }
        desk.queue[index].decision = Some(allow);
        self.changed.notify_all();
        Ok(Answered::Delivered)
    }

    /// **An assignment is over: its queue entries and its standing decision go with it**
    /// (§5.4's *"a grant and its seat are created together and revoked together, by name,
    /// per assignment"*, extended to the two things this desk holds on that assignment's
    /// behalf).
    ///
    /// Called when the assignment settles, when it is stopped, and at quit. Returns how many
    /// waiting requests were dropped, so a caller can say that a stop took a question off
    /// his screen rather than leaving one there pointing at nothing.
    pub fn forget(&self, entity: &str, thread: &str, obligation: &str) -> usize {
        let key = (entity.to_string(), thread.to_string(), obligation.to_string());
        let mut desk = self.inner.lock().unwrap();
        let before = desk.queue.len();
        desk.queue.retain(|q| !is_background(&q.request.binding) || assignment_key(&q.request.binding) != key);
        desk.standing.retain(|s| s.key != key);
        self.changed.notify_all();
        before - desk.queue.len()
    }

    /// Take the standing decision matching this exact action, if he left one. Consumed.
    fn take_standing(desk: &mut Desk, binding: &Binding, tool: &str, input: &Value) -> Option<bool> {
        let key = assignment_key(binding);
        let at = desk.standing.iter().position(|s| s.key == key && s.tool == tool && &s.input == input)?;
        Some(desk.standing.remove(at).allow)
    }
}
impl ScopedPermissions {
    pub fn decide(&self, request: &Value) -> PermissionDecision { self.wait(request,permission_deadline()) }
    /// The same decision with a caller-supplied deadline. **Test scaffolding**, and it is
    /// here rather than in a test module because `work_host`'s tests need to raise a real
    /// request against the real desk — a second implementation of this path would be a test
    /// asserting about itself. The shipping caller is [`Self::decide`], at 300 s
    /// ([`permission_deadline`]).
    #[doc(hidden)]
    pub fn decide_within(&self, request: &Value, limit: Duration) -> PermissionDecision { self.wait(request,limit) }
    fn wait(&self, request: &Value, limit: Duration) -> PermissionDecision {
        let deny=|message:&str|PermissionDecision::Deny{message:message.into()};
        let allow=||PermissionDecision::Allow{updated_input:request.get("input").cloned().unwrap_or(json!({}))};
        let Some(binding)=grant(&self.scope) else {return deny("This app turn is stopped or is supplying context. New actions are unavailable.")};
        let tool=request["tool_name"].as_str().unwrap_or("");
        // ===================================================================================
        // THE FRONT DESK DOES NOT DO THE WORK — the CEO's Two Riches note 3, landed
        // ===================================================================================
        //
        // *"The front desk gets no orchestration tools, so it cannot drift into doing the
        // work."* These four lines are the refusal that implements it, and they were written
        // in the approval slice and deliberately HELD, with both preconditions named. Both
        // are now met, in the same commit as this:
        //
        //   1. **The doctrine moved.** The standing instruction used to route work through
        //      these tools from the conversation; it now tells the front desk to hand work to
        //      the back end with `richos_assignments.record` and to look with
        //      `richos_status.background_work` (`doctrine.rs`).
        //   2. **The read exists.** Sage's check of that page, finding 6: taking `richos_work`
        //      off the front desk left it no read tool at all, because every status surface is
        //      a Tauri command the model cannot call. `status_tools.rs` is that read.
        //
        // **This is the SECOND of two enforcements, and it is the one that catches a lease
        // that should not have the server at all.** The first is the omission in
        // `native.rs`'s `mcp_config` — a conversation lease is no longer given `richos_work`
        // — which is the enforcement that matters, exactly as seam 1 argues for
        // `richos_continuity`. This one refuses the call if a work tool ever reaches the desk
        // on a binding that is not background work: a config edit, a lease that outlived a
        // role change, or a future path nobody has thought of yet. A deny here is never a
        // thing he sees, and it names the register so the model's next move is the right one.
        if tool.starts_with("mcp__richos_work__") && !is_background(&binding) {
            return deny("The conversation does not run work directly. Write the assignment down with \
                         the assignment register and let the background connection run it.");
        }
        // ===================================================================================
        // A BACKGROUND JOB LANDS ON ITS OWN — the CEO's ruling of 2026-09-18, §52
        // ===================================================================================
        //
        // *"There's nothing that ever not lands on its own here in the terminal. Anything
        // including things like design mockups always land before they are presented to me
        // for review. So, yes, always land on its own."*
        //
        // This line is that ruling. `mcp__richos_work__integrate` used to be the one work
        // tool deliberately OFF the list below — background-work spec §0 row 7, §5.4 and
        // §7.8 all turned on its absence, and the whole `blocked`/"ready for you to approve"
        // state existed to hold the assignment at it. §52 closes that open item the other
        // way: a land is not a thing he is asked about, it is the thing the job is FOR, and
        // what he hears is the outcome.
        //
        // **It is guarded on the audience rather than added to the list below, and that is
        // not decoration.** The list below is reached by BOTH audiences; the front desk is
        // kept off every work tool by the four lines above it, so putting `integrate` on the
        // list would have made the ruling depend on that refusal continuing to exist. Here
        // the grant names its own precondition, so a future edit that weakened the front
        // desk's refusal could not silently hand the conversation a land.
        //
        // **What this does NOT do is empty the queue.** A background lease still reaches the
        // desk for anything the desk would have asked about in a visible turn — `Bash`, a
        // `Write` outside its workspace, any tool this app has no standing grant for — and
        // `a_real_question_from_a_work_lease_still_waits_for_him` is the positive control
        // that says so on the same desk, in the same test, as the land that does not ask.
        //
        // **And the engine still refuses a land that has not earned it.** Nothing here is a
        // grant to merge: `integrate` requires an observed run end on both the worker and an
        // independent reviewer, a `passed` verdict on the exact reviewed commit, clean
        // checkouts, an unmoved recorded tip, and a fast-forward — and it never pushes
        // (`richos/engine/mega-lander/app.py:755-836`). What was removed is the CEO's
        // keystroke, not a check.
        if tool == "mcp__richos_work__integrate" && is_background(&binding) {
            return allow();
        }
        // These tools implement their own explicit host scope and write contracts.
        if matches!(tool,"mcp__richos_work__repositories"|"mcp__richos_work__prepare"|"mcp__richos_work__inspect"|"mcp__richos_work__complete"|
            // The team tool (proto-teammate shelf plan slice 4, richos-hq
            // docs/plans/2026-10-06-proto-teammate-shelf.md section 10 point 2: the back end
            // activates a shelf teammate when a job needs it, without asking). It lists and
            // shows the shelf and writes one file, `<app data>/team/<name>.md`, under its own
            // contract: a definition that could not be started by that name is refused, and
            // the user's existing copy is kept unless `replace` says the user asked for it.
            "mcp__richos_work__team"|
            "mcp__richos_continuity__checkpoint"|"mcp__richos_continuity__inspect"|
            "mcp__richos_onboarding__save_company_notes"|"mcp__richos_onboarding__decline_onboarding"|
            // The assignment register (`assignment_tools.rs`). App-owned, with its own host
            // scope and write contract, exactly like the two onboarding tools beside it: the
            // model supplies what the assignment IS, and the company, conversation, state
            // root and attested instruction all come from a scope the app wrote. It records
            // an intent and changes nothing else — what it cannot do is the thing worth
            // saying out loud, which is finish the work.
            //
            // `mcp__richos_work__integrate` is still NOT on this list, and the reason changed
            // on 2026-09-18 rather than going away: it is granted above, on the background
            // audience alone (CEO ruling §52). This list is reached by BOTH audiences, and a
            // land is a thing only the back end does.
            "mcp__richos_assignments__record"|
            // The front desk's READ (`status_tools.rs`). On the list on the same grounds as
            // the three app-owned tools above it — its own host scope, written by the app —
            // and on one stronger ground of its own: it changes nothing at all. There is no
            // input for him to approve, and a permission sheet asking him to allow Rich to
            // LOOK at his own record would be a question with one answer.
            "mcp__richos_status__background_work"|
            // RICH'S DOCUMENT SEARCH (`search_tools.rs`): on the conversation lease only, it
            // reads the CEO's own folder through the app's socket and changes nothing. A sheet
            // asking him to let Rich look something up in his own documents would be a
            // question with one answer.
            "mcp__richos_search__search_documents"|
            // HIS TEAM'S TOOLS (`operator_desk_tools.rs`), present only on an operator install's
            // front desk. A named stop only removes (§67: *"unconditional for the agents his
            // words name"*), the interrupt ends a turn and keeps its agents, and the read
            // changes nothing: a sheet asking him to allow his own stop would be the delay the
            // stop exists to remove.
            "mcp__richos_operator__stop"|"mcp__richos_operator__interrupt"|"mcp__richos_operator__read") {
            return allow();
        }
        if tool.is_empty() || request.to_string().len()>65536 {return deny("The requested action could not be safely displayed.");}
        let input=request.get("input").cloned().unwrap_or(json!({}));
        let id=uuid::Uuid::new_v4().to_string();
        let mut desk=self.desk.inner.lock().unwrap();
        // **His answer from before this call existed** (§5.7). Only ever present for a
        // background binding, only ever for this exact tool and this exact input, and gone
        // the moment it is read.
        if let Some(allowed)=PermissionDesk::take_standing(&mut desk,&binding,tool,&input) {
            return if allowed {allow()} else {deny("The CEO declined this step. Do not retry it and do not work around it.")};
        }
        // **ONE STEP OF AN ASSIGNMENT WAITS ON HIM AT A TIME, AND WHILE IT WAITS THE TURN
        // ENDS** (esc-20260927T093052Z-85f3303f). His answer to a step whose call has already
        // returned goes to the ASSIGNMENT, and the work host can only put the assignment back
        // on the lease once this turn is over. A second request here would hold the turn for
        // a whole deadline of its own — measured in the test VM: he approved the first
        // command and nothing ran for 133.5-137.5 s, because the back end had gone on to ask for a
        // different one. So it is refused at once and never put in front of him; the resumed
        // run asks again if it still needs it. Another assignment's requests are untouched.
        let key=assignment_key(&binding);
        if is_background(&binding) && desk.queue.iter().any(|q| q.call_returned && q.decision.is_none()
            && is_background(&q.request.binding) && assignment_key(&q.request.binding)==key) {
            return deny(STEP_ALREADY_WAITING);
        }
        desk.queue.push(Queued{request:PermissionRequest{id:id.clone(),binding:binding.clone(),tool:tool.into(),
            input:input.clone(),description:request["description"].as_str().unwrap_or("").into(),reason:permission_reason(request),
            raised_at_ms:now_ms()},
            scope:self.scope.clone(),decision:None,call_returned:false});
        self.desk.changed.notify_all();
        let background=is_background(&binding);
        let deadline=Instant::now()+limit;
        loop {
            let Some(index)=desk.queue.iter().position(|q|q.request.id==id) else {
                // Only [`PermissionDesk::forget`] removes an entry somebody is waiting on,
                // and it does that when the assignment stops.
                break deny(ASSIGNMENT_GONE);
            };
            // **The conversation's liveness test, and it stays exactly as it was.** A
            // background binding is deliberately not held to it: its grant closes when the
            // work lease's turn ends, which is the ordinary case rather than a stop.
            if !background && grant(&self.scope).as_ref()!=Some(&binding) {
                desk.queue.remove(index);
                break deny("The app turn was stopped or changed before approval.");
            }
            if background && !self.scope.exists() {
                // The work lease itself is gone — its scope file is removed when the lease
                // drops (`native.rs`'s `Drop`). Nothing can carry out what he approves, so
                // the call ends rather than waiting for an answer that could not be used.
                desk.queue.remove(index);
                break deny(ASSIGNMENT_GONE);
            }
            if let Some(allowed)=desk.queue[index].decision {
                desk.queue.remove(index);
                break if allowed {allow()}
                    else {deny("The user declined this action. Do not retry it through another tool.")};
            }
            if Instant::now()>=deadline {
                if background {
                    // §5.7. The call ends; the request stays exactly where he will find it.
                    desk.queue[index].call_returned=true;
                    break deny(NOT_APPROVED_YET);
                }
                desk.queue.remove(index);
                break deny("No permission decision was received. The action was not approved.");
            }
            desk=self.desk.changed.wait_timeout(desk,Duration::from_millis(100)).unwrap().0;
        }
    }
}
#[cfg(test)] mod tests {
    use super::*;

    fn binding(audience: &str, turn: &str) -> Value {
        json!({"entity_id":"alpha","thread_id":"thread","session_id":"session","turn_id":turn,"audience":audience,"revision":1})
    }
    fn scope_with(audience: &str, turn: &str)->(PathBuf,ScopedPermissions) {
        let p=std::env::temp_dir().join(format!("permission-{}.json",uuid::Uuid::new_v4()));
        std::fs::write(&p,json!({"version":1,"actions_allowed":true,"binding":binding(audience,turn)}).to_string()).unwrap();
        let policy=ScopedPermissions{desk:Arc::new(PermissionDesk::default()),scope:p.clone()};(p,policy)
    }
    fn scope()->(PathBuf,ScopedPermissions) { scope_with("ceo","turn") }
    /// A second lease sharing ONE desk — the shape the app actually runs (§2.7: the desk is
    /// one `Arc<PermissionDesk>` on the engine profile, serving both leases).
    fn joined(desk: &Arc<PermissionDesk>, audience: &str, turn: &str)->(PathBuf,ScopedPermissions) {
        let p=std::env::temp_dir().join(format!("permission-{}.json",uuid::Uuid::new_v4()));
        std::fs::write(&p,json!({"version":1,"actions_allowed":true,"binding":binding(audience,turn)}).to_string()).unwrap();
        (p.clone(),ScopedPermissions{desk:Arc::clone(desk),scope:p})
    }
    /// The deadline of a call THIS TEST answers, stops or forgets, and the longest `wait_for`
    /// waits: a hang guard, never a verdict (audit
    /// `docs/verification/2026-09-29-load-sensitive-checks-audit.md` R9). These calls used to
    /// carry 2-5 s deadlines and `wait_for` gave up after 200 polls of 5 ms, so a test thread
    /// descheduled for a few seconds saw its own call time out into "deny" before it could
    /// answer, and failed while the desk was right. A test that means a deadline to EXPIRE
    /// passes a few milliseconds and says so; that direction cannot be broken by load.
    const HANG_GUARD: Duration = Duration::from_secs(60);

    fn ask(policy: &ScopedPermissions, tool: &str, limit: Duration) -> std::thread::JoinHandle<PermissionDecision> {
        let child=policy.clone();let tool=tool.to_string();
        std::thread::spawn(move||child.wait(&json!({"tool_name":tool,"input":{"command":"fictional action"}}),limit))
    }
    fn wait_for(f: impl Fn()->bool) -> bool {
        let deadline=Instant::now()+HANG_GUARD;
        while Instant::now()<deadline { if f() {return true;} std::thread::sleep(Duration::from_millis(5)); }
        f()
    }

    #[test] fn provider_reason_survives_missing_human_explanation() {
        assert_eq!(permission_reason(&json!({"decision_reason_type":"safetyCheck","classifier_approvable":false})),"The provider requires manual approval for this command form.");
        assert_eq!(permission_reason(&json!({"decision_reason":"Exact provider reason","decision_reason_type":"safetyCheck"})),"Exact provider reason");
        assert_eq!(permission_reason(&json!({})),"");
    }
    #[test] fn allow_and_deny_apply_to_one_exact_request_and_never_persist_a_rule(){
        for allow in [true,false] {let (p,policy)=scope();
            let work=ask(&policy,"Bash",HANG_GUARD);
            assert!(wait_for(||policy.desk.current().is_some()));
            let request=policy.desk.current().unwrap();
            assert!(policy.desk.resolve("obsolete",true).is_err());
            assert_eq!(policy.desk.resolve(&request.id,allow).unwrap(),Answered::Delivered);
            assert_eq!(work.join().unwrap().behavior(),if allow {"allow"}else{"deny"});
            assert!(policy.desk.current().is_none());assert!(policy.desk.resolve(&request.id,true).is_err());std::fs::remove_file(p).unwrap();
        }
    }
    #[test] fn stop_and_expiry_never_approve_an_action(){
        let (p,policy)=scope();assert_eq!(policy.wait(&json!({"tool_name":"Bash"}),Duration::from_millis(1)).behavior(),"deny");
        let work=ask(&policy,"Bash",HANG_GUARD);
        assert!(wait_for(||policy.desk.current().is_some()));
        std::fs::remove_file(&p).unwrap();assert_eq!(work.join().unwrap().behavior(),"deny");assert!(policy.desk.current().is_none());
    }

    // ---- §5.2/§5.5: the queue ------------------------------------------------------------

    /// **§5.2's whole sentence.** A background request raised with no visible turn is NOT
    /// denied on the spot: it waits, it is visible where he will find it, and the answer he
    /// gives is the one that decides it.
    ///
    /// The positive control is the first assertion: the same code path with the SAME desk
    /// and a conversation binding whose grant has gone still denies at once
    /// (`stop_and_expiry_never_approve_an_action` above), so "it waited" is a fact about the
    /// background arm rather than about a desk that never refuses anything.
    ///
    /// **The example is `Bash` and not `integrate`, and that is a correction rather than a
    /// preference** (CEO ruling §52, 2026-09-18). `integrate` is granted outright to a
    /// background lease now, so a test that kept it as its waiting request would have gone
    /// green by never queuing anything and asserting nothing.
    #[test] fn a_background_request_with_no_visible_turn_waits_instead_of_being_denied(){
        let desk=Arc::new(PermissionDesk::default());
        let (p,work_lease)=joined(&desk,WORK_AUDIENCE,"obligation-7");
        let call=ask(&work_lease,"Bash",HANG_GUARD);
        assert!(wait_for(||!desk.background_queue().is_empty()),"the request was refused rather than queued");
        // It is NOT on the conversation's sheet: a modal for background work would be an
        // interruption (§0 row 6).
        assert!(desk.current().is_none(),"a background request opened the permission sheet");
        let request=desk.background_queue().remove(0);
        assert_eq!(request.tool,"Bash");
        assert_eq!(desk.resolve(&request.id,true).unwrap(),Answered::Delivered);
        assert_eq!(call.join().unwrap().behavior(),"allow");
        assert!(desk.background_queue().is_empty());
        std::fs::remove_file(p).unwrap();
    }

    /// **§5.5 and §5.6 in one shape.** Two workers ask at once. Neither refuses the other —
    /// the old desk answered the second with *"Another action is already waiting"* — the
    /// queue keeps his order, and answering the SECOND one releases only that worker while
    /// the first is still waiting.
    #[test] fn a_second_request_queues_behind_the_first_and_blocks_only_its_own_worker(){
        let desk=Arc::new(PermissionDesk::default());
        let (p1,one)=joined(&desk,WORK_AUDIENCE,"obligation-1");
        let (p2,two)=joined(&desk,WORK_AUDIENCE,"obligation-2");
        let first=ask(&one,"Write",HANG_GUARD);
        assert!(wait_for(||desk.background_queue().len()==1));
        let second=ask(&two,"Bash",HANG_GUARD);
        assert!(wait_for(||desk.background_queue().len()==2),"the second request was refused rather than queued");
        let queue=desk.background_queue();
        assert_eq!(queue[0].binding.turn_id,"obligation-1","the queue is not in the order they asked");
        assert_eq!(queue[1].binding.turn_id,"obligation-2");
        desk.resolve(&queue[1].id,true).unwrap();
        assert_eq!(second.join().unwrap().behavior(),"allow");
        // The first worker is STILL waiting: answering one assignment released one worker.
        assert_eq!(desk.background_queue().len(),1);
        assert!(!first.is_finished(),"answering the second request released the first worker too");
        desk.resolve(&desk.background_queue()[0].id,false).unwrap();
        assert_eq!(first.join().unwrap().behavior(),"deny");
        std::fs::remove_file(p1).unwrap();std::fs::remove_file(p2).unwrap();
    }

    // ---- §5.7: the deadline stops the CALL, not the REQUEST -------------------------------

    /// **The deadline is 300 s unless the test VM asks for less, and never more.**
    /// `RICHOS_PERMISSION_DEADLINE_MS` exists so a walk can drive the late-approval path
    /// (`76e977dc`) in the guest without waiting out 300 s; the same shape as
    /// `RICHOS_CANCEL_GRACE_MS` (`native.rs`). It can shorten the wait and cannot lengthen it:
    /// anything outside 1 s to 300 s, or not a number, is the shipping 300 s.
    #[test] fn the_deadline_is_three_hundred_seconds_unless_a_walk_asks_for_less(){
        assert_eq!(permission_deadline_from(None),Duration::from_secs(300));
        assert_eq!(permission_deadline_from(Some("20000")),Duration::from_secs(20));
        assert_eq!(permission_deadline_from(Some("1000")),Duration::from_secs(1));
        for refused in ["999","0","-5","300001","3600000","twenty",""] {
            assert_eq!(permission_deadline_from(Some(refused)),Duration::from_secs(300),"{refused:?} changed the deadline");
        }
    }

    /// **§5.7, and it is the test the acceptance step 7.6 rests on.** The call ends at its
    /// deadline in *not approved* — never in approval — and the request is still there
    /// afterwards. His later answer goes to the ASSIGNMENT, and the standing decision it
    /// leaves lets the resumed run take that one exact action.
    #[test] fn the_deadline_ends_the_call_and_never_the_request(){
        let desk=Arc::new(PermissionDesk::default());
        let (p,work_lease)=joined(&desk,WORK_AUDIENCE,"obligation-7");
        let call=work_lease.wait(&json!({"tool_name":"Bash","input":{"command":"say what happened"}}),Duration::from_millis(30));
        assert_eq!(call.behavior(),"deny","the deadline approved something on his behalf");
        match &call { PermissionDecision::Deny{message}=>assert!(message.contains("has not been approved"),"{message}"), _=>unreachable!() }
        let waiting=desk.background_queue();
        assert_eq!(waiting.len(),1,"the request died with its call, so he could never answer it");
        // He answers an hour later. There is no call left to take it, so it goes to the
        // assignment — which is what the work host resumes.
        let answered=desk.resolve(&waiting[0].id,true).unwrap();
        match answered {
            Answered::ToAssignment{binding,allow}=>{
                assert!(allow);
                assert_eq!(assignment_key(&binding),("alpha".into(),"thread".into(),"obligation-7".into()));
            }
            other=>panic!("his late answer went nowhere: {other:?}"),
        }
        assert!(desk.background_queue().is_empty());
        // The resumed run asks for the SAME action and is allowed exactly once.
        let again=||work_lease.wait(&json!({"tool_name":"Bash","input":{"command":"say what happened"}}),Duration::from_millis(30));
        assert_eq!(again().behavior(),"allow","his approval never reached the step he approved");
        assert_eq!(again().behavior(),"deny","a standing decision was reused — that is an auto-approval");
        std::fs::remove_file(p).unwrap();
    }

    /// A standing decision is the narrowest thing that can carry his answer forward: same
    /// assignment, same tool, **same exact input**. A different action on the same
    /// assignment still has to ask.
    #[test] fn a_standing_decision_never_covers_a_different_action(){
        let desk=Arc::new(PermissionDesk::default());
        let (p,work_lease)=joined(&desk,WORK_AUDIENCE,"obligation-7");
        work_lease.wait(&json!({"tool_name":"Bash","input":{"command":"one exact command"}}),Duration::from_millis(20));
        let waiting=desk.background_queue();
        desk.resolve(&waiting[0].id,true).unwrap();
        // Same tool, different input.
        let other=work_lease.wait(&json!({"tool_name":"Bash","input":{"command":"a different command"}}),Duration::from_millis(20));
        assert_eq!(other.behavior(),"deny","an approval for one command approved another");
        // And it is still HIS: the different action is waiting for him, not thrown away.
        assert_eq!(desk.background_queue().len(),1);
        std::fs::remove_file(p).unwrap();
    }

    /// **A step that is already waiting on him ends the turn; it does not start a second
    /// wait** (esc-20260927T093052Z-85f3303f, measured in the test VM on 2026-09-27).
    ///
    /// The back end asked for one command, its call ended at the deadline, and instead of
    /// ending its turn it asked for a DIFFERENT command. That second request held the turn for
    /// another full deadline, and the work host cannot put the assignment back on the lease
    /// until the turn ends: he approved the first command and nothing ran for 133.5-137.5 s. So once
    /// a step of this assignment is waiting on him with no call left to take his answer, a
    /// further request on the SAME assignment is refused at once, is not put in front of him,
    /// and tells the back end to end its turn.
    ///
    /// Controls in the same test: the waiting request is untouched and his later answer still
    /// reaches the assignment; another assignment's request still waits for him as before.
    #[test] fn a_step_already_waiting_on_him_ends_the_turn_instead_of_opening_a_second_wait(){
        let desk=Arc::new(PermissionDesk::default());
        let (p,work_lease)=joined(&desk,WORK_AUDIENCE,"obligation-7");
        let first=work_lease.wait(&json!({"tool_name":"Bash","input":{"command":"sleep 95"}}),Duration::from_millis(30));
        assert_eq!(first.behavior(),"deny");
        assert_eq!(desk.background_queue().len(),1);
        // A different command, on the same assignment. A desk that queued it and waited would
        // come back with the deadline's own refusal (which also says "End your turn now") and
        // a second entry in his queue; the refusal that did not wait is STEP_ALREADY_WAITING,
        // word for word, with his queue unchanged. Those two facts are the verdict. The
        // `< 1 s` clock that stood here decided it by the speed of the Mac instead (audit R9).
        let second=work_lease.wait(&json!({"tool_name":"Bash","input":{"command":"cat /tmp/out.txt"}}),Duration::from_secs(3));
        match &second {
            PermissionDecision::Deny{message}=>assert_eq!(message,STEP_ALREADY_WAITING,
                "the second request was not refused as a step behind one already waiting on him"),
            _=>panic!("a second step ran while the first was waiting on him"),
        }
        assert_eq!(desk.background_queue().len(),1,"the second request was put in front of him as well");
        assert_eq!(desk.background_queue()[0].input["command"],"sleep 95");
        // His answer to the first still reaches the assignment, and the resumed run takes it.
        let answered=desk.resolve(&desk.background_queue()[0].id,true).unwrap();
        assert!(matches!(answered,Answered::ToAssignment{allow:true,..}));
        assert_eq!(work_lease.wait(&json!({"tool_name":"Bash","input":{"command":"sleep 95"}}),Duration::from_millis(30)).behavior(),"allow");
        // CONTROL: another assignment is not held up by this one's waiting step. This one is
        // put back into exactly that state first: a step waiting on him with no call left.
        let (p2,other)=joined(&desk,WORK_AUDIENCE,"obligation-8");
        assert_eq!(work_lease.wait(&json!({"tool_name":"Bash","input":{"command":"third"}}),Duration::from_millis(30)).behavior(),"deny");
        assert_eq!(desk.background_queue().len(),1);
        let call=ask(&other,"Bash",HANG_GUARD);
        assert!(wait_for(||desk.background_queue().iter().any(|r|r.binding.turn_id=="obligation-8")),
            "another assignment's request was refused because of this one");
        let theirs=desk.background_queue().into_iter().find(|r|r.binding.turn_id=="obligation-8").unwrap();
        desk.resolve(&theirs.id,true).unwrap();
        assert_eq!(call.join().unwrap().behavior(),"allow");
        std::fs::remove_file(p).unwrap();std::fs::remove_file(p2).unwrap();
    }

    /// His DECLINE, given after the call ended, is carried to the assignment the same way —
    /// and the step he declined is refused if it is ever asked for again.
    #[test] fn a_decline_after_the_deadline_reaches_the_assignment_and_refuses_the_step(){
        let desk=Arc::new(PermissionDesk::default());
        let (p,work_lease)=joined(&desk,WORK_AUDIENCE,"obligation-7");
        work_lease.wait(&json!({"tool_name":"Bash","input":{}}),Duration::from_millis(20));
        let waiting=desk.background_queue();
        match desk.resolve(&waiting[0].id,false).unwrap() {
            Answered::ToAssignment{allow,..}=>assert!(!allow),
            other=>panic!("{other:?}"),
        }
        assert_eq!(work_lease.wait(&json!({"tool_name":"Bash","input":{}}),Duration::from_millis(20)).behavior(),"deny");
        std::fs::remove_file(p).unwrap();
    }

    /// **§5.4's unit of revocation, at this desk.** Stopping one assignment takes ITS
    /// question off his screen and leaves every other assignment's alone — and the worker
    /// waiting on the stopped one is released with a refusal rather than left hanging.
    #[test] fn forgetting_one_assignment_leaves_every_other_assignments_request_alone(){
        let desk=Arc::new(PermissionDesk::default());
        let (p1,one)=joined(&desk,WORK_AUDIENCE,"obligation-1");
        let (p2,two)=joined(&desk,WORK_AUDIENCE,"obligation-2");
        let first=ask(&one,"Bash",HANG_GUARD);
        let second=ask(&two,"Bash",HANG_GUARD);
        assert!(wait_for(||desk.background_queue().len()==2));
        assert_eq!(desk.forget("alpha","thread","obligation-1"),1);
        assert_eq!(first.join().unwrap().behavior(),"deny");
        assert_eq!(desk.background_queue().len(),1,"stopping one assignment took another's question away");
        assert_eq!(desk.background_queue()[0].binding.turn_id,"obligation-2");
        desk.resolve(&desk.background_queue()[0].id,false).unwrap();
        assert_eq!(second.join().unwrap().behavior(),"deny");
        std::fs::remove_file(p1).unwrap();std::fs::remove_file(p2).unwrap();
    }

    /// And a standing decision does not outlive its assignment either: forgetting the
    /// assignment drops it, so a later request cannot inherit an answer he gave about work
    /// that has since been stopped.
    #[test] fn forgetting_an_assignment_drops_the_standing_decision_too(){
        let desk=Arc::new(PermissionDesk::default());
        let (p,work_lease)=joined(&desk,WORK_AUDIENCE,"obligation-7");
        work_lease.wait(&json!({"tool_name":"Bash","input":{}}),Duration::from_millis(20));
        let waiting=desk.background_queue();
        desk.resolve(&waiting[0].id,true).unwrap();
        desk.forget("alpha","thread","obligation-7");
        assert_eq!(work_lease.wait(&json!({"tool_name":"Bash","input":{}}),Duration::from_millis(20)).behavior(),"deny");
        std::fs::remove_file(p).unwrap();
    }

    /// **THE INVERSE OF THE TEST THAT USED TO BE HERE, and it is the CEO's ruling §52 in one
    /// measurement** (2026-09-18): *"There's nothing that ever not lands on its own here in
    /// the terminal … So, yes, always land on its own."*
    ///
    /// Its predecessor was named `the_step_that_changes_his_repository_is_the_one_that_has_to_ask`
    /// and asserted the exact opposite — `integrate` reaching his queue and waiting — so the
    /// day the ruling landed it landed against a measurement rather than a memory. **All
    /// SIX** work tools now go through on a background lease and none of them touches the
    /// queue (the sixth, `team`, since proto-teammate shelf slice 4).
    ///
    /// **Both arms are here because either alone passes for the wrong reason.** A desk that
    /// allowed everything would satisfy the first arm and be no desk at all, so the second
    /// arm is a real question from the SAME lease on the SAME desk, and it still waits. And
    /// the loop is over the whole set rather than four names, so a sixth work tool added
    /// tomorrow is a decision somebody has to make here rather than a silent inheritance.
    #[test] fn a_background_lease_lands_without_asking_him(){
        let desk=Arc::new(PermissionDesk::default());
        let (p,work_lease)=joined(&desk,WORK_AUDIENCE,"obligation-7");
        for tool in ["mcp__richos_work__repositories","mcp__richos_work__prepare","mcp__richos_work__inspect",
                     "mcp__richos_work__complete","mcp__richos_work__integrate","mcp__richos_work__team"] {
            assert_eq!(work_lease.wait(&json!({"tool_name":tool,"input":{}}),Duration::from_millis(20)).behavior(),
                "allow","{tool} asked him for permission to do the job he gave it");
        }
        assert!(desk.background_queue().is_empty(),"a work tool reached his queue; a land is not a question");
        // POSITIVE CONTROL, same lease, same desk, same call: the queue still works, so the
        // six allows above are a fact about those six tools and not about a desk that has
        // stopped refusing anything.
        assert_eq!(work_lease.wait(&json!({"tool_name":"Bash","input":{}}),Duration::from_millis(20)).behavior(),"deny");
        assert_eq!(desk.background_queue().len(),1,"a genuine permission request no longer reaches him");
        assert_eq!(desk.background_queue()[0].tool,"Bash");
        std::fs::remove_file(p).unwrap();
    }

    /// **TWO CONVERSATIONS, TWO QUEUES OUT OF ONE DESK** (the Two Riches spec; Sage's
    /// finding 12). Each thread sees the part of his queue that is about its own work, and
    /// answering one leaves the other exactly where it was.
    #[test] fn one_desk_gives_each_conversation_its_own_queue(){
        let desk=Arc::new(PermissionDesk::default());
        let p1=std::env::temp_dir().join(format!("permission-{}.json",uuid::Uuid::new_v4()));
        std::fs::write(&p1,json!({"version":1,"actions_allowed":true,"binding":{"entity_id":"alpha","thread_id":"thread-one",
            "session_id":"s1","turn_id":"obligation-1","audience":WORK_AUDIENCE,"revision":1}}).to_string()).unwrap();
        let p2=std::env::temp_dir().join(format!("permission-{}.json",uuid::Uuid::new_v4()));
        std::fs::write(&p2,json!({"version":1,"actions_allowed":true,"binding":{"entity_id":"alpha","thread_id":"thread-two",
            "session_id":"s2","turn_id":"obligation-2","audience":WORK_AUDIENCE,"revision":1}}).to_string()).unwrap();
        let one=ScopedPermissions{desk:Arc::clone(&desk),scope:p1.clone()};
        let two=ScopedPermissions{desk:Arc::clone(&desk),scope:p2.clone()};
        one.wait(&json!({"tool_name":"Write","input":{}}),Duration::from_millis(20));
        two.wait(&json!({"tool_name":"Bash","input":{}}),Duration::from_millis(20));
        assert_eq!(desk.background_queue().len(),2);
        assert_eq!(desk.background_queue_on("thread-one").len(),1);
        assert_eq!(desk.background_queue_on("thread-two").len(),1);
        assert_eq!(desk.background_queue_on("thread-one")[0].tool,"Write");
        assert_eq!(desk.background_queue_on("thread-two")[0].tool,"Bash");
        // Answering one conversation's question leaves the other's alone.
        desk.resolve(&desk.background_queue_on("thread-one")[0].id,true).unwrap();
        assert!(desk.background_queue_on("thread-one").is_empty());
        assert_eq!(desk.background_queue_on("thread-two").len(),1,"answering one conversation emptied another's queue");
        std::fs::remove_file(p1).unwrap();std::fs::remove_file(p2).unwrap();
    }

    /// **WHAT EACH AUDIENCE CAN REACH NOW THAT NOTE 3 HAS LANDED**, and the two halves of
    /// the CEO's sentence in one measurement.
    ///
    /// *"The front desk gets no orchestration tools, so it cannot drift into doing the
    /// work."* The predecessor of this test pinned the state before the refusal — both
    /// audiences reaching all four work tools — so that the day it landed it would land
    /// against a measurement rather than a memory. This is that same test, after.
    ///
    /// **Every arm has its opposite in the same test**, because each one on its own can
    /// pass for the wrong reason: a front desk that reached nothing at all would satisfy the
    /// refusal and be useless, and a back end that reached nothing would make the app unable
    /// to work while this test stayed green.
    #[test] fn the_front_desk_reaches_no_work_tool_and_the_back_end_reaches_all_six(){
        let desk=Arc::new(PermissionDesk::default());
        let (front,conversation)=joined(&desk,"ceo","turn-9");
        let (back,work_lease)=joined(&desk,WORK_AUDIENCE,"obligation-7");
        // **All FIVE, in one loop, including the land** — CEO ruling §52, 2026-09-18. This
        // loop carried four names and asserted the fifth separately, because the fifth was
        // the one that waited for him; it no longer does, so it belongs with its siblings and
        // there is no longer a name in this file that a sixth work tool could hide behind.
        // The sixth came (`team`, proto-teammate shelf slice 4) and was added here.
        for tool in ["mcp__richos_work__repositories","mcp__richos_work__prepare","mcp__richos_work__inspect",
                     "mcp__richos_work__complete","mcp__richos_work__integrate","mcp__richos_work__team"] {
            let refused=conversation.wait(&json!({"tool_name":tool,"input":{}}),Duration::from_millis(20));
            assert_eq!(refused.behavior(),"deny","the front desk still reaches {tool}");
            // The refusal names the way OUT, not just the door: a model told "no" with no
            // alternative retries; a model told where the work goes hands it over.
            assert!(format!("{refused:?}").contains("assignment register"),"the refusal of {tool} names no alternative");
            // Positive control, same tool, same desk, other audience.
            assert_eq!(work_lease.wait(&json!({"tool_name":tool,"input":{}}),Duration::from_millis(20)).behavior(),"allow","back end: {tool}");
        }
        assert!(desk.background_queue().is_empty(),"a work tool reached his queue; the job does its own work");
        // **What still reaches him from the back end**, so the empty queue above is a fact
        // about work tools and not about a desk that has stopped asking him anything.
        assert_eq!(work_lease.wait(&json!({"tool_name":"Bash","input":{}}),Duration::from_millis(20)).behavior(),"deny");
        assert_eq!(desk.background_queue().len(),1,"a genuine permission request no longer reaches him");
        assert_eq!(desk.background_queue()[0].binding.audience,WORK_AUDIENCE);
        // **And what note 3 LEAVES on the front desk**: hand the work over, and look at it.
        // Without these two arms the test above would be satisfied by a front desk that can
        // do nothing whatsoever, which is not what his page asks for.
        for tool in ["mcp__richos_assignments__record","mcp__richos_status__background_work"] {
            assert_eq!(conversation.wait(&json!({"tool_name":tool,"input":{}}),Duration::from_millis(20)).behavior(),"allow","front desk lost {tool}");
        }
        std::fs::remove_file(front).unwrap();std::fs::remove_file(back).unwrap();
    }

    /// The conversation's own requests still die with their turn. §5.7's survival is for
    /// background work alone — a request tied to a turn he was watching, kept alive after
    /// that turn ended, is the *"one exact action, one visible turn"* rule broken.
    #[test] fn a_conversation_request_still_dies_with_its_turn(){
        let (p,policy)=scope();
        let call=policy.wait(&json!({"tool_name":"Bash","input":{}}),Duration::from_millis(20));
        assert_eq!(call.behavior(),"deny");
        assert!(policy.desk.current().is_none(),"the conversation's expired request is still on the sheet");
        assert!(policy.desk.background_queue().is_empty());
        std::fs::remove_file(p).unwrap();
    }
}
