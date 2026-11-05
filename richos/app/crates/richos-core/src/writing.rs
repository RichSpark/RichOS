//! One explicit text-only revision, on the current account. No conversation,
//! warm-up, automatic retry, tools or resident provider. The caller owns consent.
use crate::{engine_profile::EngineProfile, request_classifier::Provider};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::Path, sync::atomic::{AtomicBool, Ordering}, time::{Duration, Instant}};

pub const MAX_CHARACTERS: usize = 12_000;
pub const DEADLINE: Duration = Duration::from_secs(10);
pub const DESTINATION: &str = "https://api.anthropic.com";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="kebab-case")]
pub enum Mode { Cleanup, Rewrite }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="kebab-case")]
pub enum Failure { Empty, Instruction, TooLong, Canceled, Timeout, Quota, Offline, Unavailable, Invalid }

/// Numeric endpoints only. Full text remains in the calling operation's memory.
pub struct Revision {
    pub text: String,
    pub spawn_ms: u64,
    pub init_frame_ms: Option<u64>,
    pub init_to_result_ms: Option<u64>,
    pub ready_to_result_ms: u64,
}

pub fn validate(mode:Mode, original:&str, instruction:&str)->Result<(),Failure> {
    if original.trim().is_empty() {return Err(Failure::Empty)}
    if mode==Mode::Rewrite && instruction.trim().is_empty() {return Err(Failure::Instruction)}
    if original.chars().count().saturating_add(instruction.chars().count())>MAX_CHARACTERS {return Err(Failure::TooLong)}
    Ok(())
}

pub struct Request<'a> {pub mode:Mode,pub original:&'a str,pub instruction:&'a str}

pub fn revise(bin:&Path, profile:&EngineProfile, request:Request<'_>, canceled:&AtomicBool,
    budget:Duration)->Result<Revision,Failure> {
    let Request {mode,original,instruction}=request;
    validate(mode,original,instruction)?;
    let began=Instant::now();
    let deadline=began+budget.min(DEADLINE);
    if canceled.load(Ordering::Acquire) {return Err(Failure::Canceled)}
    if Instant::now()>=deadline {return Err(Failure::Timeout)}
    let mut live=Provider::new(bin,profile,"app/writing.md","writing").private_text().start()
        .map_err(|_|Failure::Unavailable)?;
    let spawn_ms=began.elapsed().as_millis() as u64;
    let request=json!({"type":"user","message":{"role":"user","content":
        json!({"action":mode,"original":original,"instruction":instruction}).to_string()}}).to_string();
    let result=live.exchange(request,deadline,||canceled.load(Ordering::Acquire),answer);
    let received=Instant::now();
    let init_frame_ms=live.initialized_at.map(|at|at.saturating_duration_since(began).as_millis() as u64);
    let init_to_result_ms=live.initialized_at.map(|at|received.saturating_duration_since(at).as_millis() as u64);
    // Include the inherited owned-child cleanup in the visible deadline.
    drop(live);
    if canceled.load(Ordering::Acquire) {return Err(Failure::Canceled)}
    if Instant::now()>=deadline {return Err(Failure::Timeout)}
    let text=result.map_err(|error|match error.as_str() {
        "writing-quota"=>Failure::Quota,"writing-offline"=>Failure::Offline,
        "writing-invalid"=>Failure::Invalid,_=>Failure::Unavailable,
    })?;
    let text=revision_text(original,&text)?;
    Ok(Revision {text,spawn_ms,init_frame_ms,init_to_result_ms,ready_to_result_ms:began.elapsed().as_millis() as u64})
}

fn revision_text(original:&str, proposal:&str)->Result<String,Failure> {
    // Original edge whitespace belongs to the surrounding document: both a
    // selected paragraph's separator and spaces added by ordinary dictation.
    // Carry it once into the visible proposal, Copy and exact replacement.
    let start=original.len()-original.trim_start().len();
    let end=original.trim_end().len();
    let mut text=original[..start].to_string();
    text.push_str(proposal.trim());
    text.push_str(&original[end..]);
    if text.chars().count()>MAX_CHARACTERS {return Err(Failure::Invalid)}
    Ok(text)
}

fn answer(frame:&Value)->Result<String,String> {
    if frame["is_error"]==true {
        let category=frame["subtype"].as_str().unwrap_or_default();
        return Err(match category {
            "error_rate_limit"|"rate_limit_error"=>"writing-quota",
            "error_api_connection"|"api_connection_error"=>"writing-offline",
            _=>"writing-invalid",
        }.into())
    }
    let text=frame["result"].as_str().ok_or("writing-invalid")?;
    if text.trim().is_empty() || text.chars().count()>MAX_CHARACTERS || text.contains('\0') {return Err("writing-invalid".into())}
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_classifier::tests::fixture;

    #[test]
    fn text_and_instruction_share_one_limit_without_truncation() {
        assert_eq!(validate(Mode::Rewrite,"draft",""),Err(Failure::Instruction));
        assert_eq!(validate(Mode::Cleanup,&"a".repeat(MAX_CHARACTERS),""),Ok(()));
        assert_eq!(validate(Mode::Rewrite,&"a".repeat(MAX_CHARACTERS),"edit"),Err(Failure::TooLong));
        assert_eq!(answer(&json!({"result":"Proposal\n\n"})).unwrap(),"Proposal\n\n");
        assert!(answer(&json!({"result":""})).is_err());
    }

    #[test]
    fn revisions_keep_original_boundary_whitespace_without_duplication() {
        for (original,proposal,wanted) in [
            ("Review the forecast.\n","Please review it.\n","Please review it.\n"),
            ("Review it.\r\n\r\n","Please review it.","Please review it.\r\n\r\n"),
            ("Review it.\u{2029}","Please review it.\n","Please review it.\u{2029}"),
            ("Review it.","Please review it.\n\n","Please review it."),
            (" So we need the forecast.","So we need the forecast."," So we need the forecast."),
            ("So we need the forecast. ","So we need the forecast.","So we need the forecast. "),
            (" \tReview it. \r\n","  Please review it. \n"," \tPlease review it. \r\n"),
        ] {assert_eq!(revision_text(original,proposal).unwrap(),wanted);}
        assert_eq!(revision_text("Original.\n",&"x".repeat(MAX_CHARACTERS)),Err(Failure::Invalid),
            "Restoring a selected separator cannot exceed the output bound");
    }

    #[test]
    fn explicit_request_uses_only_exact_text_and_instruction_and_exits() {
        let (root,bin,profile,_)=fixture(r#"
assert sys.argv[sys.argv.index('--tools')+1]==''
assert '--no-session-persistence' in sys.argv
assert '--disable-slash-commands' in sys.argv
assert sys.argv[sys.argv.index('--debug-file')+1]=='/dev/null'
assert os.environ['CLAUDE_CODE_MAX_RETRIES']=='0'
assert os.environ['CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC']=='1'
assert os.environ.get('CLAUDE_CODE_ENABLE_TELEMETRY') is None
assert os.environ.get('DISABLE_TELEMETRY')=='1'
open('argv.json','w').write(json.dumps(sys.argv))
print(json.dumps({'type':'system','subtype':'init','tools':[]}),flush=True)
for line in sys.stdin:
    payload=json.loads(json.loads(line)['message']['content'])
    open('asked.json','w').write(json.dumps(payload))
    result='Please review the £36,000 forecast by Monday.\n'
    print(json.dumps({'type':'assistant','message':{'model':'claude-haiku-5-5','content':[{'type':'text','text':result}]}}),flush=True)
    print(json.dumps({'type':'result','is_error':False,'result':result}),flush=True)
"#);
        let result=revise(&bin,&profile,Request {mode:Mode::Rewrite,original:" Review £36,000 by Monday.\n",instruction:"Make this more polite."},&AtomicBool::new(false),DEADLINE).unwrap();
        assert_eq!(result.text," Please review the £36,000 forecast by Monday.\n");
        let asked:Value=serde_json::from_str(&std::fs::read_to_string(profile.state.join("writing/asked.json")).unwrap()).unwrap();
        assert_eq!(asked,json!({"action":"rewrite","original":" Review £36,000 by Monday.\n","instruction":"Make this more polite."}));
        let args:Vec<String>=serde_json::from_str(&std::fs::read_to_string(profile.state.join("writing/argv.json")).unwrap()).unwrap();
        assert!(!args.iter().any(|a|a.contains("£36,000")),"User content never enters process arguments");
        assert_eq!(args[args.iter().position(|a|a=="--system-prompt").unwrap()+1],std::fs::read_to_string(profile.engine.join("app/writing.md")).unwrap());
        assert!(result.spawn_ms<=result.ready_to_result_ms);
        assert!(result.init_frame_ms.is_some_and(|n|n<=result.ready_to_result_ms));
        assert!(result.init_to_result_ms.is_some_and(|n|n<=result.ready_to_result_ms));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_provider_initialization_is_unknown_not_an_invented_startup_time() {
        let (root,bin,profile,_)=fixture(r#"
for line in sys.stdin:
    print(json.dumps({'type':'assistant','message':{'model':'claude-haiku-5-5','content':[{'type':'text','text':'Keep the refusal.'}]}}),flush=True)
    print(json.dumps({'type':'result','is_error':False,'result':'Keep the refusal.'}),flush=True)
"#);
        let result=revise(&bin,&profile,Request {mode:Mode::Cleanup,original:"Keep the refusal.",instruction:""},&AtomicBool::new(false),DEADLINE).unwrap();
        assert_eq!(result.init_frame_ms,None);
        assert_eq!(result.init_to_result_ms,None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_before_submission_starts_no_provider() {
        let (root,bin,profile,_)=fixture("");
        assert!(matches!(revise(&bin,&profile,Request {mode:Mode::Cleanup,original:"draft",instruction:""},&AtomicBool::new(true),DEADLINE),Err(Failure::Canceled)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_stops_an_actual_request_and_reaps_its_provider() {
        let (root,bin,profile,_)=fixture(r#"
for line in sys.stdin:
    open('submitted.pid.tmp','w').write(str(os.getpid()))
    os.replace('submitted.pid.tmp','submitted.pid')
    time.sleep(120) # Hang fixture; cancellation and child reaping decide the verdict.
"#);
        let marker=profile.state.join("writing/submitted.pid");
        let stop=std::sync::Arc::new(AtomicBool::new(false));let canceled=stop.clone();
        let worker=std::thread::spawn(move||revise(&bin,&profile,Request {mode:Mode::Cleanup,original:"Keep £36,000 and Monday.",instruction:""},&canceled,DEADLINE));
        let guard=Instant::now()+Duration::from_secs(30);
        // A finite hang guard, not a latency verdict: wait for actual submitted input.
        while !marker.exists() {
            if Instant::now()>=guard {panic!("The fixture never received its request")}
            std::thread::sleep(Duration::from_millis(10));
        }
        let pid:i32=std::fs::read_to_string(marker).unwrap().parse().unwrap();
        stop.store(true,Ordering::Release);
        assert!(matches!(worker.join().unwrap(),Err(Failure::Canceled)));
        assert_eq!(unsafe {libc::kill(pid,0)},-1,"The owned provider must be gone after cancellation");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_stalled_request_returns_timeout_without_retrying() {
        let (root,bin,profile,_)=fixture(r#"
for line in sys.stdin:
    with open('requests','a') as log: log.write('request\n')
    time.sleep(120) # The production deadline ends this child, not a success-path sleep.
"#);
        let result=revise(&bin,&profile,Request {mode:Mode::Cleanup,original:"No commitment before Friday.",instruction:""},&AtomicBool::new(false),DEADLINE);
        assert!(matches!(result,Err(Failure::Timeout)));
        assert_eq!(std::fs::read_to_string(profile.state.join("writing/requests")).unwrap(),"request\n");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_real_quota_signal_stops_without_switching_or_repeating() {
        let (root,bin,profile,_)=fixture(r#"
for line in sys.stdin:
    print(json.dumps({'type':'rate_limit_event','rate_limit_info':{'status':'rejected'}}),flush=True)
"#);
        assert!(matches!(revise(&bin,&profile,Request {mode:Mode::Cleanup,original:"Keep the original.",instruction:""},&AtomicBool::new(false),DEADLINE),Err(Failure::Quota)));
        std::fs::remove_dir_all(root).unwrap();
    }
}
