//! Native provider permission requests surfaced in the desktop. One exact action, one
//! visible turn — and **background work never waits on him for a step at all.**
//!
//! # Two audiences, two answers
//!
//! | Audience | What reaches this desk | What happens |
//! |---|---|---|
//! | the conversation (`audience != "worker"`) | a step the provider would not approve on its own, in a turn he is watching | it waits on the permission sheet ([`PermissionDesk::current`]) while the grant file still equals the request's binding |
//! | background work (`audience == "worker"`) | the same kind of step, in work he handed over and is not watching | refused at once with [`BACKGROUND_STEP_REFUSED`]; nothing is put in front of him |
//!
//! # Why background work is never asked about (the CEO, 2026-10-13)
//!
//! *"Is the CEO user supposed to keep babysitting Rich like this all the time OR WHAT???"* —
//! over nightly 54, where a job of reading his own website stopped at "Approve this" for
//! "running a command on your Mac". Work he hands over runs to a finished result with no
//! approval from him. The provider runs in auto mode and approves routine work itself
//! (reading websites, searching, reading and writing in its own workspace and the commands
//! that work needs), so what still reaches this desk from a background lease is a step it
//! would not approve on its own: that step is refused, and the model is told to find another
//! way or to report plainly what it could not do. A real business decision of his travels as
//! a question (`question_tools.rs`), never through this desk.
//!
//! This replaces the background-work spec's §5.2 (*"a request raised with no turn open is
//! queued for his next look"*) and with it the late-approval path of §5.7.
//!
//! # What a background lease is granted outright
//!
//! The six desktop work tools, the land among them (CEO ruling §52, 2026-09-18: *"always land
//! on its own"*), and the app-owned tools listed in [`ScopedPermissions::decide`]'s allow
//! list, each of which carries its own host scope and write contract. The engine still refuses
//! a land that has not earned it (`richos/engine/mega-lander/app.py`).
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
    matches!(binding.audience.as_str(),WORK_AUDIENCE|"rich")
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
}

#[derive(Debug, Default)]
struct Desk {
    /// Ordered, oldest first. §5.5: *"its single `Option<Pending>` becomes an ordered queue
    /// whose head is the one shown"*. Since 2026-10-13 only the conversation's requests are
    /// ever in it: background work's are refused before they reach it.
    queue: Vec<Queued>,
}

#[derive(Debug, Default)]
pub struct PermissionDesk { inner: Mutex<Desk>, changed: Condvar }
#[derive(Clone, Debug)]
pub struct ScopedPermissions { pub desk: Arc<PermissionDesk>, pub scope: PathBuf }

/// What [`PermissionDesk::resolve`] did with his answer.
///
/// **`ToAssignment` is no longer produced** (2026-10-13): it carried an answer he gave after
/// a BACKGROUND call had ended, and background work no longer waits on him at all
/// ([`BACKGROUND_STEP_REFUSED`]). The variant is kept only so the shell and the work host,
/// which still match on it, change in their own follow-up rather than in this fix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answered {
    /// The call that raised it is still waiting and has taken the decision itself.
    Delivered,
    /// Never produced since 2026-10-13; see the type's doc.
    ToAssignment { binding: Binding, allow: bool },
}

/// **What a background step is told when it reaches this desk** (the CEO, 2026-10-13: work he
/// hands over never stops to wait for his approval). Written for the model: nothing ran,
/// nothing was put in front of him, and the two moves left are another way or an honest
/// report.
pub const BACKGROUND_STEP_REFUSED: &str =
    "The action was not run. This background connection cannot obtain manual step approval.";

/// **How long one permission call waits for him: 300 s.**
///
/// `RICHOS_PERMISSION_DEADLINE_MS` can SHORTEN it, to between 1 s and 300 s, and nothing can
/// lengthen it. It was added for a test VM walk of a late approval (`76e977dc`), which
/// otherwise had to wait out the whole 300 s. The same shape as `RICHOS_CANCEL_GRACE_MS` (`native.rs`). The app's launcher
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
    /// Background work never appears here: since 2026-10-13 its steps are refused before they
    /// reach the queue ([`BACKGROUND_STEP_REFUSED`]).
    pub fn current(&self) -> Option<PermissionRequest> {
        self.inner.lock().unwrap().queue.iter()
            .find(|q| !is_background(&q.request.binding)
                && grant(&q.scope).as_ref() == Some(&q.request.binding)
                && q.decision.is_none())
            .map(|q| q.request.clone())
    }

    /// **What background work is waiting on him for: nothing, since 2026-10-13.** Its steps
    /// are refused before they reach the queue ([`BACKGROUND_STEP_REFUSED`]), so this is
    /// always empty. It stays because the work host and the shell still read it to draw an
    /// Approve row; removing that surface is its own change.
    pub fn background_queue(&self) -> Vec<PermissionRequest> {
        self.inner.lock().unwrap().queue.iter()
            .filter(|q| is_background(&q.request.binding) && q.decision.is_none())
            .map(|q| q.request.clone())
            .collect()
    }

    /// [`Self::background_queue`] for one conversation; empty for the same reason.
    pub fn background_queue_on(&self, thread: &str) -> Vec<PermissionRequest> {
        self.background_queue().into_iter().filter(|request| request.binding.thread_id == thread).collect()
    }

    /// His answer to one exact request: the request must still match a live grant, or the
    /// turn it belonged to has gone and the old request cannot be approved.
    pub fn resolve(&self, id: &str, allow: bool) -> Result<Answered, String> {
        let mut desk = self.inner.lock().unwrap();
        let index = desk.queue.iter().position(|q| q.request.id == id)
            .ok_or_else(|| "That action is no longer waiting for permission.".to_string())?;
        if desk.queue[index].decision.is_some() {
            return Err("That action's turn has stopped or changed. The old request cannot be approved.".into());
        }
        if grant(&desk.queue[index].scope).as_ref() != Some(&desk.queue[index].request.binding) {
            return Err("That action's turn has stopped or changed. The old request cannot be approved.".into());
        }
        desk.queue[index].decision = Some(allow);
        self.changed.notify_all();
        Ok(Answered::Delivered)
    }

    /// **An assignment is over: any request of its goes with it** (§5.4). Called when the
    /// assignment settles, when it is stopped, and at quit. Returns how many were dropped,
    /// which since 2026-10-13 is always 0: background requests never enter the queue.
    pub fn forget(&self, entity: &str, thread: &str, obligation: &str) -> usize {
        let key = (entity.to_string(), thread.to_string(), obligation.to_string());
        let mut desk = self.inner.lock().unwrap();
        let before = desk.queue.len();
        desk.queue.retain(|q| !is_background(&q.request.binding) || assignment_key(&q.request.binding) != key);
        self.changed.notify_all();
        before - desk.queue.len()
    }

}
impl ScopedPermissions {
    pub fn decide(&self, request: &Value) -> PermissionDecision { self.wait(request,permission_deadline()) }
    /// The same decision with a caller-supplied deadline. **Test scaffolding**, and it is
    /// here rather than in a test module because `work_host`'s tests raise a real request
    /// against the real desk — a second implementation of this path would be a test asserting
    /// about itself. The shipping caller is [`Self::decide`], at 300 s ([`permission_deadline`]).
    #[doc(hidden)]
    pub fn decide_within(&self, request: &Value, limit: Duration) -> PermissionDecision { self.wait(request,limit) }
    fn wait(&self, request: &Value, limit: Duration) -> PermissionDecision {
        let deny=|message:&str|PermissionDecision::Deny{message:message.into()};
        let allow=||PermissionDecision::Allow{updated_input:request.get("input").cloned().unwrap_or(json!({}))};
        let tool=request["tool_name"].as_str().unwrap_or("");
        let Some(binding)=grant(&self.scope) else {return deny("This app turn is stopped or is supplying context. New actions are unavailable.")};
        let work_read = matches!(tool, "mcp__richos_work__inspect" | "mcp__richos_work__repositories")
            || (tool == "mcp__richos_work__team" && matches!(request["input"]["action"].as_str(), Some("list" | "show")));
        if work_read { return allow(); }
        // These tools implement their own explicit host scope and write contracts.
        if matches!(tool,"mcp__richos_work__integrate"|"mcp__richos_work__repositories"|"mcp__richos_work__prepare"|"mcp__richos_work__inspect"|"mcp__richos_work__complete"|
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
            "mcp__richos_assignments__record"|"mcp__richos_status__background_work"|"mcp__richos_status__sessions"|
            "mcp__richos_status__transcript"|"mcp__richos_status__send"|
            // HIS TEAM'S TOOLS (`operator_desk_tools.rs`), present only on an operator install's
            // front desk. A named stop only removes (§67: *"unconditional for the agents his
            // words name"*), the interrupt ends a turn and keeps its agents, and the read
            // changes nothing: a sheet asking him to allow his own stop would be the delay the
            // stop exists to remove.
            "mcp__richos_operator__stop"|"mcp__richos_operator__interrupt"|"mcp__richos_operator__read") {
            return allow();
        }
        if tool.is_empty() || request.to_string().len()>65536 {return deny("The requested action could not be safely displayed.");}
        // ===================================================================================
        // BACKGROUND WORK NEVER WAITS ON HIM FOR A STEP — the CEO, 2026-10-13
        // ===================================================================================
        //
        // *"Is the CEO user supposed to keep babysitting Rich like this all the time OR
        // WHAT???"* — said over a job of reading his own website that stopped at "Approve
        // this" for "running a command on your Mac" (nightly 54). Work he hands over runs to a
        // finished result with no approval from him; only a real business decision of his
        // ever reaches him, and that travels as a question (`question_tools.rs`), never
        // through this desk. The provider already approves routine work itself (auto mode);
        // what still arrives here from a background lease is a step it would not approve on
        // its own, so the step is refused at once, nothing is put in front of him, and the
        // model is told to find another way or report what it could not do.
        //
        // This replaces the background-work spec's §5.2 ("queued for his next look"). The
        // queue below now only ever holds the CONVERSATION's requests, which are a turn he is
        // watching.
        if is_background(&binding) {
            return deny(BACKGROUND_STEP_REFUSED);
        }
        let input=request.get("input").cloned().unwrap_or(json!({}));
        let id=uuid::Uuid::new_v4().to_string();
        let mut desk=self.desk.inner.lock().unwrap();
        desk.queue.push(Queued{request:PermissionRequest{id:id.clone(),binding:binding.clone(),tool:tool.into(),
            input:input.clone(),description:request["description"].as_str().unwrap_or("").into(),reason:permission_reason(request),
            raised_at_ms:now_ms()},
            scope:self.scope.clone(),decision:None});
        self.desk.changed.notify_all();
        let deadline=Instant::now()+limit;
        loop {
            let Some(index)=desk.queue.iter().position(|q|q.request.id==id) else {
                break deny("This request was withdrawn, so the action was not approved.");
            };
            // **One exact action, one visible turn**: the request lives only while the grant
            // it was raised under is still the open one.
            if grant(&self.scope).as_ref()!=Some(&binding) {
                desk.queue.remove(index);
                break deny("The app turn was stopped or changed before approval.");
            }
            if let Some(allowed)=desk.queue[index].decision {
                desk.queue.remove(index);
                break if allowed {allow()}
                    else {deny("The user declined this action. It was not run.")};
            }
            if Instant::now()>=deadline {
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

    // ---- the CEO, 2026-10-13: background work never waits on him ------------------------

    /// **His sentence, at the desk.** *"Is the CEO user supposed to keep babysitting Rich like
    /// this all the time OR WHAT???"* — nightly 54 stopped a job of reading his own website at
    /// "Approve this" for a command. A step of background work that reaches this desk is
    /// refused AT ONCE, with the model told to find another way or report what it could not
    /// do, and nothing is put in front of him: not on the sheet, not in the background queue.
    ///
    /// The deadline here is the hang guard: a desk that queued the request would hold this
    /// call for a whole minute and then fail the "nothing is waiting" assertions; the refusal
    /// that does not wait returns the exact sentence below.
    ///
    /// Positive control in the same test: the same request on a CONVERSATION binding still
    /// waits on the sheet for him, so the refusal is a fact about background work and not
    /// about a desk that has stopped asking anything.
    #[test] fn a_background_step_is_refused_at_once_and_never_put_in_front_of_him(){
        let desk=Arc::new(PermissionDesk::default());
        let (p,work_lease)=joined(&desk,WORK_AUDIENCE,"obligation-7");
        for tool in ["Bash","Write","Edit","WebFetch","a_tool_with_no_grant"] {
            let call=work_lease.wait(&json!({"tool_name":tool,"input":{"command":"python3 /engine/pierce.py --dismiss"}}),HANG_GUARD);
            match &call {
                PermissionDecision::Deny{message}=>assert_eq!(message,BACKGROUND_STEP_REFUSED,"{tool}"),
                _=>panic!("{tool} from background work was approved on his behalf"),
            }
            assert!(desk.background_queue().is_empty(),"{tool} from background work was put in front of him");
            assert!(desk.current().is_none(),"{tool} from background work opened the permission sheet");
        }
        // The refusal reports what happened; the plain role file owns what to do next.
        assert!(BACKGROUND_STEP_REFUSED.contains("not run"));
        assert!(BACKGROUND_STEP_REFUSED.contains("cannot obtain manual step approval"));
        // POSITIVE CONTROL: the conversation's request still waits on his sheet.
        let (q,conversation)=joined(&desk,"ceo","turn-9");
        let call=ask(&conversation,"Bash",HANG_GUARD);
        assert!(wait_for(||desk.current().is_some()),"the conversation's request no longer reaches him");
        desk.resolve(&desk.current().unwrap().id,false).unwrap();
        assert_eq!(call.join().unwrap().behavior(),"deny");
        std::fs::remove_file(p).unwrap();std::fs::remove_file(q).unwrap();
    }

    // ---- the deadline ----------------------------------------------------------------------

    /// **The deadline is 300 s unless the test VM asks for less, and never more.**
    /// `RICHOS_PERMISSION_DEADLINE_MS` lets a walk in the guest avoid waiting out 300 s
    /// (`76e977dc`); the same shape as
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
    /// arm is a step with no grant from the SAME lease on the SAME desk, and it is refused. And
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
        // POSITIVE CONTROL, same lease, same desk: a step with no grant is still refused, so
        // the six allows above are a fact about those six tools and not about a desk that has
        // stopped refusing anything. It is refused without being put in front of him.
        assert_eq!(work_lease.wait(&json!({"tool_name":"Bash","input":{}}),Duration::from_millis(20)).behavior(),"deny");
        assert!(desk.background_queue().is_empty(),"a step of background work was put in front of him");
        std::fs::remove_file(p).unwrap();
    }

    /// Both top-level agents have the same scoped work capabilities.
    #[test] fn both_top_level_agents_reach_the_same_scoped_work_tools(){
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
            assert_eq!(conversation.wait(&json!({"tool_name":tool,"input":{}}),Duration::from_millis(20)).behavior(),"allow","Rich lost {tool}");
            assert_eq!(work_lease.wait(&json!({"tool_name":tool,"input":{}}),Duration::from_millis(20)).behavior(),"allow","back end: {tool}");
        }
        assert!(desk.background_queue().is_empty(),"a work tool reached his queue; the job does its own work");
        // **A step of the back end with no grant is refused, and never reaches him**, so the
        // allows above are a fact about work tools and not about a desk that allows anything.
        assert_eq!(work_lease.wait(&json!({"tool_name":"Bash","input":{}}),Duration::from_millis(20)).behavior(),"deny");
        assert!(desk.background_queue().is_empty(),"a step of background work was put in front of him");
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
