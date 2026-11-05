//! The app owns consent and one short text-provider worker. The dictation tool
//! owns native targets and proposal UI. No conversation or provider stays warm.
use crate::{AppState, dictation::{ipc::{AppMessage,ToolMessage},store::{self,WritingConsent}}};
use richos_core::{engine_profile::EngineProfile, writing::{self,Failure,Mode}};
use serde_json::{json,Value};
use std::{path::PathBuf,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},thread::JoinHandle,time::Instant};
use tauri::{AppHandle,Manager,Emitter};

struct Worker { stop:Arc<AtomicBool>, join:JoinHandle<()> }
#[derive(Clone)]
struct Operation { id:String, binding:WritingConsent, consented:bool }
impl Operation {
    fn permits(&self,id:&str,current:&WritingConsent,allowed:bool)->bool {
        self.id==id && self.binding==*current && self.consented && allowed
    }
}
#[derive(Default)]
pub struct Host { revoked:AtomicBool, operation:Mutex<Option<Operation>>, worker:Mutex<Option<Worker>> }

impl Host {
    pub fn cancel(&self) {
        self.operation.lock().unwrap_or_else(|p|p.into_inner()).take();
        if let Some(worker)=self.worker.lock().unwrap_or_else(|p|p.into_inner()).as_ref() {
            worker.stop.store(true,Ordering::Release);
        }
    }
    pub fn stop(&self) {
        self.cancel();
        let worker=self.worker.lock().unwrap_or_else(|p|p.into_inner()).take();
        if let Some(worker)=worker {worker.join.join().ok();}
    }
}

fn host(app:&AppHandle)->Option<Arc<Host>> {app.try_state::<Arc<Host>>().map(|h|h.inner().clone())}
fn binding(app:&AppHandle)->WritingConsent {
    WritingConsent {account:app.state::<AppState>().quota.lease_account().id,destination:writing::DESTINATION.into()}
}
fn allowed(app:&AppHandle, expected:&WritingConsent)->bool {
    if host(app).is_some_and(|host|host.revoked.load(Ordering::Acquire)) {return false}
    store::read(&app.state::<AppState>().data_dir).is_ok_and(|s|s.writing_consent.as_ref()==Some(expected))
}
fn tell(app:&AppHandle,id:&str,action:&str,payload:Value) {
    if let Some(link)=app.try_state::<Arc<crate::dictation_app::Link>>() {
        link.tell(&AppMessage::Writing {id:id.into(),action:action.into(),payload});
    }
}
fn availability(app:&AppHandle, expected:&WritingConsent)->Option<Failure> {
    let view=app.state::<AppState>().quota.view();
    if view.leaving.contains(&expected.account) || view.held_until.is_some() ||
        matches!(view.admission,richos_core::quota::Admission::Held {..}) {return Some(Failure::Quota)}
    if view.accounts.iter().any(|a|a.id==expected.account && a.signed_in==Some(false)) ||
        view.in_use.as_ref().is_some_and(|a|a.id==expected.account) && view.signed_in==Some(false) {return Some(Failure::Unavailable)}
    None
}
fn publish_binding(app:&AppHandle,id:&str,current:&WritingConsent) {
    tell(app,id,"binding",json!({"account":current.account,"destination":current.destination,
        "allowed":allowed(app,current),"failure":availability(app,current)}));
}
fn profile(app:&AppHandle, expected:&WritingConsent)->Result<(PathBuf,EngineProfile),Failure> {
    let state=app.state::<AppState>();
    let account=state.quota.lease_account();
    if account.id!=expected.account {return Err(Failure::Canceled)}
    let engine=state.engine_dir.lock().map_err(|_|Failure::Unavailable)?.clone();
    let runtime=richos_core::runtime::verify_engine(&engine).map_err(|_|Failure::Unavailable)?;
    let mut profile=EngineProfile::prepare(&engine,&state.data_dir,runtime).map_err(|_|Failure::Unavailable)?;
    profile.claude_account=Some(account);
    profile.token_sink=Some(state.quota.token_sink());
    let bin=state.claude_bin.lock().map_err(|_|Failure::Unavailable)?.clone();
    Ok((bin,profile))
}

/// Revoking is immediate and affects pending requests as well as the next one.
#[richos_command::command]
pub fn dictation_writing_allowed(app:AppHandle,on:Option<bool>)->Result<Value,String> {
    let current=binding(&app);
    if on==Some(true) && !allowed(&app,&current) {return Err("Choose Clean up or Rewrite to review the privacy notice first.".into())}
    if on==Some(false) {
        // End the operation even if saving the preference fails. This process
        // requires fresh explicit consent before another request is allowed.
        if let Some(host)=host(&app) {host.revoked.store(true,Ordering::Release);host.cancel();}
        tell(&app,"","revoked",json!({}));
    }
    if let Some(on)=on {
        store::update(&app.state::<AppState>().data_dir,|s|s.writing_consent=on.then(||current.clone()))?;
    }
    Ok(json!({"allowed":allowed(&app,&current)}))
}

pub fn on_message(app:&AppHandle,message:Option<&ToolMessage>) {
    let Some(host)=host(app) else{return};
    let Some(message)=message else {host.cancel();return};
    if let ToolMessage::State {data_dir,..}=message {
        if std::path::Path::new(data_dir)!=app.state::<AppState>().data_dir {return}
        let changed=host.operation.lock().unwrap_or_else(|p|p.into_inner()).as_ref()
            .is_some_and(|op|op.binding!=binding(app) || op.consented && !allowed(app,&op.binding));
        if changed {host.cancel();tell(app,"","revoked",json!({}));}
        return;
    }
    let ToolMessage::Writing {id,action,payload}=message else{return};
    let owns=host.operation.lock().unwrap_or_else(|p|p.into_inner()).as_ref().is_some_and(|op|op.id==*id);
    match action.as_str() {
        "prepare"=>{
            host.cancel();
            let current=binding(app);
            *host.operation.lock().unwrap_or_else(|p|p.into_inner())=Some(Operation {id:id.clone(),binding:current.clone(),consented:allowed(app,&current)});
            publish_binding(app,id,&current);
        }
        "cancel" if owns=>host.cancel(),
        "consent"=>{
            let current=binding(app);
            let matches=host.operation.lock().unwrap_or_else(|p|p.into_inner()).as_ref()
                .is_some_and(|op|op.id==*id && op.binding==current) && payload["account"]==current.account && payload["destination"]==current.destination;
            if matches && store::update(&app.state::<AppState>().data_dir,|s|s.writing_consent=Some(current.clone())).is_err() {
                tell(app,id,"failure",json!({"reason":Failure::Unavailable}));return;
            }
            if matches {host.revoked.store(false,Ordering::Release);}
            *host.operation.lock().unwrap_or_else(|p|p.into_inner())=Some(Operation {id:id.clone(),binding:current.clone(),consented:matches});
            publish_binding(app,id,&current);
        }
        "accounts" if owns=>{
            crate::reopen_window(app);
            app.emit("rich://cleaner-settings",json!({"surface":"accounts"})).ok();
        }
        "request"=>start(app,host,id,payload),
        _=>{}
    }
}

fn start(app:&AppHandle,host:Arc<Host>,id:&str,payload:&Value) {
    let Some(operation)=host.operation.lock().unwrap_or_else(|p|p.into_inner()).clone().filter(|op|op.id==id) else{return};
    let current=binding(app);
    if !operation.permits(id,&current,allowed(app,&current)) || payload["account"]!=current.account || payload["destination"]!=current.destination {
        publish_binding(app,id,&current);return;
    }
    let received=Instant::now();
    let prepared=(|| {
        if let Some(failure)=availability(app,&current) {return Err(failure)}
        let mode:Mode=serde_json::from_value(payload["mode"].clone()).map_err(|_|Failure::Invalid)?;
        let original=payload["original"].as_str().ok_or(Failure::Invalid)?.to_string();
        let instruction=payload["instruction"].as_str().ok_or(Failure::Invalid)?.to_string();
        writing::validate(mode,&original,&instruction)?;
        Ok((mode,original,instruction))
    })();
    let (mode,original,instruction)=match prepared {
        Ok(value)=>value,Err(reason)=>{tell(app,id,"failure",json!({"reason":reason}));return}
    };
    let mut worker=host.worker.lock().unwrap_or_else(|p|p.into_inner());
    if worker.as_ref().is_some_and(|w|!w.join.is_finished()) {
        tell(app,id,"failure",json!({"reason":Failure::Unavailable}));return;
    }
    if let Some(previous)=worker.take() {previous.join.join().ok();}
    let stop=Arc::new(AtomicBool::new(false));let flag=stop.clone();
    let worker_app=app.clone();let worker_id=id.to_string();let owner=host.clone();
    let budget=std::time::Duration::from_millis(payload["budgetMs"].as_u64().unwrap_or(0).min(10_000))
        .saturating_sub(received.elapsed());
    let join=std::thread::Builder::new().name("writing-request".into()).spawn(move || {
        let began=Instant::now();
        // Engine/profile setup must not occupy the dictation link reader:
        // WillListen and cancellation stay immediately available to local capture.
        let result=(|| {
            if flag.load(Ordering::Acquire) {return Err(Failure::Canceled)}
            let (bin,profile)=profile(&worker_app,&operation.binding)?;
            writing::revise(&bin,&profile,writing::Request {mode,original:&original,instruction:&instruction},&flag,budget.saturating_sub(began.elapsed()))
        })();
        let valid=!flag.load(Ordering::Acquire) && owner.operation.lock().unwrap_or_else(|p|p.into_inner()).as_ref()
            .is_some_and(|op|op.permits(&worker_id,&binding(&worker_app),allowed(&worker_app,&op.binding)));
        if !valid {return}
        match result {
            Ok(result)=>tell(&worker_app,&worker_id,"proposal",json!({"text":result.text,"spawnMs":result.spawn_ms,
                "initFrameMs":result.init_frame_ms,"initToResultMs":result.init_to_result_ms,"readyToResultMs":result.ready_to_result_ms})),
            Err(reason)=>tell(&worker_app,&worker_id,"failure",json!({"reason":reason,"readyToResultMs":began.elapsed().as_millis() as u64})),
        }
    });
    match join {Ok(join)=>*worker=Some(Worker {stop,join}),Err(_)=>tell(app,id,"failure",json!({"reason":Failure::Unavailable}))}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consent_cannot_follow_a_late_result_to_another_account_or_destination() {
        let binding=WritingConsent {account:"current-account".into(),destination:writing::DESTINATION.into()};
        let operation=Operation {id:"current-operation".into(),binding:binding.clone(),consented:true};
        assert!(operation.permits("current-operation",&binding,true));
        assert!(!operation.permits("canceled-operation",&binding,true));
        assert!(!operation.permits("current-operation",&binding,false));
        let mut changed=binding.clone();changed.account="other-account".into();
        assert!(!operation.permits("current-operation",&changed,true));
        changed=binding;changed.destination="https://other.example".into();
        assert!(!operation.permits("current-operation",&changed,true));
        let unapproved=Operation {consented:false,..operation};
        assert!(!unapproved.permits("current-operation",&unapproved.binding,true));
    }
}
