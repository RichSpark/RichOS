//! Requested revisions use the existing tool's capture, panel and native target.
//! All text is operation-owned and cleared on cancellation, expiry or new capture.
use super::{Tool,ToolMessage,MenuMessage,Phase,TapAction,Problem,insert,bar,menubar,draft_now,log};
use crate::dictation::store::WritingConsent;
use richos_core::writing::{self,Mode,Failure};
use serde_json::{json,Value};
use std::{sync::atomic::AtomicBool,time::{Duration,Instant}};

pub(super) struct Assistance {
    pub id:String,
    pub client:u64,
    mode:Mode,
    stage:&'static str,
    original:String,
    destination:String,
    anchor:Option<insert::Anchor>,
    instruction:Option<String>,
    proposal:Option<String>,
    binding:Option<WritingConsent>,
    began:Instant,
    expires:Instant,
    ready:Option<Instant>,
    failure:Option<Failure>,
    status:Option<&'static str>,
}

impl Assistance {
    fn owns(&self, client:u64, id:&str)->bool {self.client==client && self.id==id}
    fn retry(&mut self, now:Instant)->String {
        let earlier=std::mem::replace(&mut self.id,richos_core::cleaner::capture_id());
        self.began=now;self.stage="checking";self.failure=None;
        self.ready=(self.mode==Mode::Cleanup || self.instruction.is_some()).then_some(now);
        earlier
    }
    fn fail(&mut self, failure:Failure, now:Instant)->Value {
        self.stage="error";self.failure=Some(failure);self.proposal=None;
        json!({"id":self.id,"mode":self.mode,"stage":"failed","reason":failure,
            "elapsed_ms":now.saturating_duration_since(self.began).as_millis(),
            "ready_to_failure_ms":self.ready.map(|ready|now.saturating_duration_since(ready).as_millis())})
    }
    fn request_expired(&self, now:Instant)->bool {
        self.ready.is_some_and(|ready|now.saturating_duration_since(ready)>=writing::DEADLINE)
    }
    fn remaining(&self, now:Instant)->Duration {
        let expiry=self.expires.saturating_duration_since(now);
        if matches!(self.stage,"requesting"|"checking") {
            self.ready.map(|ready|(ready+writing::DEADLINE).saturating_duration_since(now).min(expiry)).unwrap_or(expiry)
        } else {expiry}
    }
}

fn timing_record(version:&str, mut detail:Value)->Value {
    if let Some(fields)=detail.as_object_mut() {
        fields.insert("schema".into(),json!(1));
        fields.insert("version".into(),json!(version));
        fields.insert("source".into(),json!(crate::engine::source_commit()));
        fields.insert("profile".into(),json!(if cfg!(debug_assertions) {"debug"} else {"release"}));
    }
    detail
}
fn timing(version:&str, detail:Value) {
    log::line(&format!("writing timing {}",timing_record(version,detail)));
}

impl Tool {
    pub(super) fn writing_laid_out(&self) {
        if let Some(a)=&self.assistance {
            if a.stage=="proposal" {
                timing(&self.version,json!({"id":a.id,"stage":"proposal_layout_received",
                    "ready_to_layout_received_ms":a.ready.map(|t|t.elapsed().as_millis())}));
            }
        }
    }
    pub(super) fn instruction_recording(&self)->bool {
        self.assistance.as_ref().is_some_and(|a|matches!(a.stage,"instructing"|"transcribing"))
    }
    pub(super) fn writing_timeout(&self)->Option<Duration> {
        self.assistance.as_ref().map(|a|a.remaining(Instant::now()))
    }

    pub(super) fn writing_expire(&mut self) {
        if self.assistance.as_ref().is_some_and(|a|Instant::now()>=a.expires) {
            self.cancel_assistance();self.close_menu();
        } else if self.assistance.as_ref().is_some_and(|a|matches!(a.stage,"requesting"|"checking") && a.request_expired(Instant::now())) {
            if let Some(a)=&self.assistance {self.hub.send_to(a.client,&ToolMessage::Writing {id:a.id.clone(),action:"cancel".into(),payload:json!({})});}
            self.writing_failure(Failure::Timeout);
        }
        if self.drafts.cleanup_subject(draft_now()).is_none() {
            self.cleanup_anchor=None;
            if self.assistance.as_ref().is_some_and(|a|a.mode==Mode::Cleanup && !a.original.is_empty() && a.stage!="done") {
                self.cancel_assistance();self.close_menu();
            }
        }
    }
    pub(super) fn cancel_assistance(&mut self) {
        if let Some(a)=self.assistance.take() {
            self.hub.send_to(a.client,&ToolMessage::Writing {id:a.id.clone(),action:"cancel".into(),payload:json!({})});
            timing(&self.version,json!({"id":a.id,"stage":if a.stage=="done" {"closed"} else {"canceled"},"elapsed_ms":a.began.elapsed().as_millis()}));
        }
        if let Some(tap)=&self.tap {tap.writing_escape(false);}
    }
    pub(super) fn writing_view(&self)->Option<Value> {
        let a=self.assistance.as_ref()?;
        Some(json!({"id":a.id,"mode":a.mode,"stage":a.stage,"original":a.original,"proposal":a.proposal,
            "expiresMs":richos_core::util::now_millis().saturating_add(a.expires.saturating_duration_since(Instant::now()).as_millis() as u64),
            "destination":a.destination,"canReplace":a.stage=="proposal" && a.anchor.as_ref().zip(a.proposal.as_ref()).is_some_and(|(anchor,text)|anchor.can_replace(text)),
            "failure":a.failure,"status":a.status}))
    }
    fn show_writing(&mut self) {
        let message=self.menu_message(0);
        if self.menu.is_open() {self.menu.redraw(&self.app,|seq|MenuMessage {seq,..message});}
        else if let Some(tray)=self.tray.clone() {
            if let Some(item)=bar::ask_main(&self.app,move||menubar::item_rect(&tray)).flatten() {
                self.menu.open(&self.app,item,|seq|MenuMessage {seq,..message});
            }
        }
    }
    pub(super) fn begin_assistance(&mut self,mode:Mode) {
        if matches!(self.session.phase(),Phase::Listening {..}|Phase::Writing) || mode==Mode::Rewrite && !self.settings.on {return}
        self.cancel_assistance();
        let (original,destination,anchor)=match mode {
            Mode::Cleanup=>{
                let draft=self.drafts.cleanup_subject(draft_now()).cloned();
                (draft.as_ref().map(|d|d.text.clone()).unwrap_or_default(),draft.as_ref().map(|d|d.destination.clone()).unwrap_or_default(),
                    self.cleanup_anchor.as_ref().and_then(|a|a.requested()))
            }
            Mode::Rewrite=>{
                let anchor=insert::Anchor::selected();
                (anchor.as_ref().map(|a|a.original.clone()).unwrap_or_default(),anchor.as_ref().map(|a|a.destination_name().to_string()).unwrap_or_default(),anchor)
            }
        };
        let client=self.writing_client.unwrap_or(0);
        let lifetime=if mode==Mode::Cleanup {self.drafts.cleanup_lifetime(draft_now()).unwrap_or(Duration::from_secs(1800))}
            else {Duration::from_secs(1800)};
        self.assistance=Some(Assistance {id:richos_core::cleaner::capture_id(),client,mode,stage:"checking",original,destination,anchor,
            instruction:None,proposal:None,binding:None,began:Instant::now(),expires:Instant::now()+lifetime,
            ready:(mode==Mode::Cleanup).then(Instant::now),failure:None,status:None});
        if let Some(tap)=&self.tap {tap.writing_escape(true);}
        let a=self.assistance.as_ref().unwrap();
        let failure=if a.original.trim().is_empty() {Some(Failure::Empty)}
            else if a.original.chars().count()>writing::MAX_CHARACTERS {Some(Failure::TooLong)} else {None};
        if let Some(failure)=failure {self.writing_failure(failure);return}
        if !self.hub.send_to(client,&ToolMessage::Writing {id:a.id.clone(),action:"prepare".into(),payload:json!({"mode":mode})}) {
            self.writing_failure(Failure::Unavailable);return;
        }
        timing(&self.version,json!({"id":a.id,"stage":"invoked","mode":mode}));
        self.show_writing();
    }
    pub(super) fn writing_reply(&mut self,client:u64,id:String,action:String,payload:Value) {
        if action=="revoked" && self.writing_client==Some(client) {
            if self.instruction_recording() {self.cancel_recording();}
            self.cancel_assistance();self.close_menu();return;
        }
        if !self.assistance.as_ref().is_some_and(|a|a.owns(client,&id)) {return}
        match action.as_str() {
            "binding"=>{
                if self.assistance.as_ref().is_some_and(|a|!matches!(a.stage,"checking"|"consent"|"requesting") ||
                    a.stage=="requesting" && payload["allowed"]==true) {return}
                if let Ok(failure)=serde_json::from_value::<Failure>(payload["failure"].clone()) {self.writing_failure(failure);return}
                let Some(account)=payload["account"].as_str() else{self.writing_failure(Failure::Unavailable);return};
                if payload["destination"]!=writing::DESTINATION {self.writing_failure(Failure::Unavailable);return}
                let a=self.assistance.as_mut().unwrap();
                a.binding=Some(WritingConsent {account:account.into(),destination:writing::DESTINATION.into()});
                if payload["allowed"]!=true {a.stage="consent";a.ready=None;self.show_writing();return}
                if a.mode==Mode::Rewrite && a.instruction.is_none() {
                    a.stage="instructing";a.ready=None;
                    self.close_menu();
                    if self.session.tap(super::now_ms())==TapAction::StartListening {self.listen();}
                } else {self.request_revision();}
            }
            "proposal"=>{
                if self.assistance.as_ref().is_none_or(|a|a.stage!="requesting") {return}
                let Some(text)=payload["text"].as_str().filter(|s|!s.trim().is_empty() && s.chars().count()<=writing::MAX_CHARACTERS && !s.contains('\0')) else{self.writing_failure(Failure::Invalid);return};
                let a=self.assistance.as_mut().unwrap();
                if a.request_expired(Instant::now()) {self.writing_failure(Failure::Timeout);return}
                timing(&self.version,json!({"id":id,"stage":"proposal_received","startup_ms":payload["startupMs"].as_u64(),
                    "response_ms":payload["responseMs"].as_u64(),"provider_ready_to_result_ms":payload["readyToResultMs"].as_u64(),"invocation_to_proposal_ms":a.began.elapsed().as_millis(),
                    "ready_to_proposal_received_ms":a.ready.map(|t|t.elapsed().as_millis())}));
                a.proposal=Some(text.to_string());a.stage="proposal";self.show_writing();
            }
            "failure"=>{
                let failure=serde_json::from_value(payload["reason"].clone()).unwrap_or(Failure::Unavailable);
                timing(&self.version,json!({"id":id,"stage":"provider_failed","reason":failure,"ready_to_result_ms":payload["readyToResultMs"].as_u64()}));
                self.writing_failure(failure);
            }
            _=>{}
        }
    }
    fn request_revision(&mut self) {
        let Some(a)=self.assistance.as_mut() else{return};
        let instruction=a.instruction.as_deref().unwrap_or("");
        if let Err(failure)=writing::validate(a.mode,&a.original,instruction) {self.writing_failure(failure);return}
        let Some(binding)=&a.binding else{self.writing_failure(Failure::Unavailable);return};
        let ready=*a.ready.get_or_insert_with(Instant::now);
        let budget=writing::DEADLINE.saturating_sub(ready.elapsed());
        if budget.is_zero() {self.writing_failure(Failure::Timeout);return}
        a.stage="requesting";a.failure=None;a.proposal=None;
        if !self.hub.send_to(a.client,&ToolMessage::Writing {id:a.id.clone(),action:"request".into(),payload:json!({"mode":a.mode,
            "original":a.original,"instruction":instruction,"account":binding.account,"destination":binding.destination,"budgetMs":budget.as_millis() as u64})}) {
            self.writing_failure(Failure::Unavailable);return;
        }
        self.show_writing();
    }
    pub(super) fn instruction_done(&mut self,instruction:Option<String>,outcome:Result<(),Problem>) {
        self.session.reset();self.show_bar(super::BarView::Hidden);
        if let Some(instruction)=instruction.filter(|s|outcome.is_ok() && !s.trim().is_empty()) {
            if let Some(a)=self.assistance.as_mut() {a.instruction=Some(instruction);a.ready=Some(Instant::now());}
            self.request_revision();
        } else {
            timing(&self.version,json!({"stage":"instruction_unavailable","reason":outcome.err().map(|p|p.tag())}));
            self.writing_failure(Failure::Instruction);
        }
    }
    fn writing_failure(&mut self,failure:Failure) {
        if let Some(a)=self.assistance.as_mut() {timing(&self.version,a.fail(failure,Instant::now()));}
        self.show_writing();
    }
    pub(super) fn writing_action(&mut self,act:&str,id:Option<&str>) {
        if !self.assistance.as_ref().is_some_and(|a|Some(a.id.as_str())==id) {return}
        match act {
            "writing-cancel"=>{
                if self.instruction_recording() {self.cancel_recording();}
                self.cancel_assistance();self.close_menu();self.publish();
            }
            "writing-consent"=>{
                let a=self.assistance.as_mut().unwrap();
                if a.stage!="consent" {return}
                let Some(binding)=&a.binding else{return};
                a.stage="checking";
                a.ready=(a.mode==Mode::Cleanup || a.instruction.is_some()).then(Instant::now);
                if !self.hub.send_to(a.client,&ToolMessage::Writing {id:a.id.clone(),action:"consent".into(),payload:json!({"account":binding.account,"destination":binding.destination})}) {
                    self.writing_failure(Failure::Unavailable);return;
                }
                self.show_writing();
            }
            "writing-retry"=>{
                let a=self.assistance.as_mut().unwrap();
                if a.stage!="error" {return}
                let earlier=a.retry(Instant::now());
                self.hub.send_to(a.client,&ToolMessage::Writing {id:earlier,action:"cancel".into(),payload:json!({})});
                if !self.hub.send_to(a.client,&ToolMessage::Writing {id:a.id.clone(),action:"prepare".into(),payload:json!({"mode":a.mode})}) {
                    self.writing_failure(Failure::Unavailable);return;
                }
                self.show_writing();
            }
            "writing-copy"=>{
                let a=self.assistance.as_mut().unwrap();
                if a.stage!="proposal" {return}
                if a.proposal.as_ref().is_some_and(|text|insert::copy_only(text).is_ok()) {
                    timing(&self.version,json!({"id":a.id,"stage":"completed","outcome":"copied"}));
                    a.stage="done";a.status=Some("copied");a.original.clear();a.proposal=None;a.instruction=None;a.anchor=None;
                } else {a.status=Some("copy-failed");}
                self.show_writing();
            }
            "writing-privacy"=>{crate::opener::open("www.anthropic.com/legal/privacy").ok();}
            "writing-replace"=>{
                if !self.assistance.as_ref().is_some_and(|a|a.stage=="proposal" && a.anchor.is_some() && a.proposal.is_some()) {return}
                // A nonactivating panel still owns the keyboard. Return it before
                // the single paste, then revalidate the target and source. The
                // existing main-thread reply is a completion barrier, not a sleep.
                self.close_menu();
                let hidden=bar::ask_main(&self.app,||()).is_some();
                let a=self.assistance.as_ref().unwrap();
                timing(&self.version,json!({"id":a.id,"stage":"replacement_panel_hidden","completed":hidden}));
                let outcome=if hidden {
                    a.anchor.as_ref().unwrap().replace(a.proposal.as_ref().unwrap(),super::v_code_on_main(&self.app),&AtomicBool::new(false))
                } else {Ok(insert::Replacement::CopyOnly)};
                match outcome {
                    Ok(insert::Replacement::Replaced {text,anchor})=>{
                        if self.assistance.as_ref().is_some_and(|a|a.mode==Mode::Cleanup) {
                            self.drafts.revised(text,anchor.as_ref().map(|a|a.source_range()),draft_now());self.cleanup_anchor=anchor;
                        }
                        let a=self.assistance.as_mut().unwrap();a.stage="done";a.status=Some("replaced");a.original.clear();a.proposal=None;a.instruction=None;a.anchor=None;
                    }
                    Ok(insert::Replacement::Check)=>{
                        let a=self.assistance.as_mut().unwrap();a.stage="done";a.status=Some("check");a.original.clear();a.proposal=None;a.instruction=None;a.anchor=None;self.cleanup_anchor=None;
                    }
                    _=>{if let Some(a)=self.assistance.as_mut() {a.anchor=None;a.status=Some("copy-only");}}
                }
                if let Some(a)=&self.assistance {timing(&self.version,json!({"id":a.id,"stage":"replacement_result","outcome":a.status}));}
                self.show_writing();
            }
            _=>{}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn operation(now:Instant)->Assistance {
        Assistance {id:"current".into(),client:8,mode:Mode::Rewrite,stage:"consent",original:"Do not commit £36,000 before Friday.".into(),destination:"TextEdit".into(),anchor:None,
            instruction:None,proposal:None,binding:None,began:now,expires:now+Duration::from_secs(1800),ready:None,failure:None,status:None}
    }
    #[test]
    fn an_explicit_retry_rejects_already_queued_old_replies_without_extending_retention() {
        let now=Instant::now();let mut a=operation(now);
        let expiry=a.expires;
        a.stage="error";a.instruction=Some("Keep the refusal, make it polite.".into());a.failure=Some(Failure::Timeout);
        let old=a.retry(now+Duration::from_secs(20));
        assert_eq!(old,"current");
        assert!(!a.owns(8,&old),"An already queued old result cannot satisfy the new attempt");
        assert!(a.owns(8,&a.id));
        assert_eq!(a.expires,expiry,"Explicit retries do not extend the original's lifetime");
        assert_eq!(a.original,"Do not commit £36,000 before Friday.");
        assert_eq!(a.instruction.as_deref(),Some("Keep the refusal, make it polite."));
        assert_eq!(a.ready,Some(now+Duration::from_secs(20)));
        assert_eq!(a.failure,None);
    }
    #[test]
    fn doctor_receives_the_actual_content_free_writing_timing_record() {
        use richdoctor_core::{collect::{collect,Category,Scope,Window},scrub::Context,evidence::Status};
        let home=std::env::temp_dir().join(format!("writing-diagnostics-{}",std::process::id()));
        let data=home.join("Library/Application Support/com.richos.app");
        std::fs::create_dir_all(&data).unwrap();
        let at=1_798_784_000_000;
        let record=timing_record("1.2.9-dev.fixture",json!({"id":"owned-operation","stage":"proposal_received","ready_to_proposal_received_ms":721}));
        let now=Instant::now();let mut a=operation(now);a.ready=Some(now);
        let failed=timing_record("1.2.9-dev.fixture",a.fail(Failure::Timeout,now+Duration::from_secs(11)));
        assert_eq!(a.original,"Do not commit £36,000 before Friday.","Failure never clears the source");
        assert_eq!(a.stage,"error");
        std::fs::write(data.join("dictation.log"),format!("{stamp} writing timing {record}\n{stamp} writing timing {failed}\n",stamp=richdoctor_core::util::utc_stamp(at))).unwrap();
        let scope=Scope {home:home.clone(),window:Window::around(at),claude_bin:None,diagnostics:true};
        let collected=collect(Category::Richscribe,&scope,&Context::default(),0);
        assert_eq!(collected[0].status,Status::Collected);
        let records:Vec<Value>=collected[0].text.lines().map(|line|serde_json::from_str(line.split_once("writing timing ").unwrap().1).unwrap()).collect();
        let parsed=&records[0];
        assert_eq!(records[1]["reason"],"timeout");
        assert_eq!(records[1]["ready_to_failure_ms"],11_000,"Controlled input time, not a wall-clock latency verdict");
        assert!(!records[1].to_string().contains("£36,000"));
        assert_eq!(parsed["ready_to_proposal_received_ms"],721);
        assert_eq!(parsed["version"],"1.2.9-dev.fixture");
        assert!(parsed.get("original").is_none() && parsed.get("instruction").is_none() && parsed.get("proposal").is_none());
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn a_stale_connection_or_operation_cannot_supply_a_proposal() {
        let a=operation(Instant::now());
        assert!(a.owns(8,"current"));
        assert!(!a.owns(7,"current"));
        assert!(!a.owns(8,"earlier"));
    }
    #[test]
    fn reading_consent_is_not_request_wait_and_the_original_expiry_never_moves() {
        let now=Instant::now();let mut a=operation(now);
        assert_eq!(a.remaining(now+Duration::from_secs(60)),Duration::from_secs(1740));
        a.stage="requesting";a.ready=Some(now+Duration::from_secs(60));
        assert_eq!(a.remaining(now+Duration::from_secs(61)),Duration::from_secs(9));
        assert!(a.request_expired(now+Duration::from_secs(70)));
        a.stage="proposal";
        assert_eq!(a.remaining(now+Duration::from_secs(70)),Duration::from_secs(1730));
        assert_eq!(a.remaining(now+Duration::from_secs(1801)),Duration::ZERO);
    }
}
