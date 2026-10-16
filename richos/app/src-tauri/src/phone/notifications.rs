//! Native alerts contain opaque references only. The hosted service owns APNs keys.
//! Durable registration intent and pending events survive Mac/process/network failures.
use super::{connect::client::{Client, Reply}, device::Device, hex, sha256, unb64url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// **THE iOS APPS WHOSE APNs TOKENS THIS MAC FORWARDS.** The first two are the preserved app's;
/// `dev.richos.native.ios` is the new native app's development ID (Rich's decision, 2026-09-22).
/// `dev.richos.connect` is RichConnect's permanent identifier, also required in the
/// Worker's private `APNS_TOPICS` configuration. `dev.richos.connect.perf` is the team's TEST copy
/// of RichConnect, which registers under its own ID (CEO, 2026-10-03).
pub const APNS_TOPICS: &[&str] = &["dev.richos.mobile.loop", "dev.richos.mobile.integration", "dev.richos.native.ios", "dev.richos.connect", "dev.richos.connect.perf"];
/// **THE ANDROID APPS WHOSE FCM TOKENS THIS MAC FORWARDS.** `dev.richos.native.android` is the
/// native app's development application ID; `dev.richos.connect` is its permanent ID;
/// `dev.richos.connect.perf` is the TEST copy's.
pub const FCM_APPS: &[&str] = &["dev.richos.native.android", "dev.richos.connect", "dev.richos.connect.perf"];

/// A native push registration, as the phone sends it inside `{"native_push": …}`.
///
/// **Two shapes, and the first is byte-for-byte the one the preserved iPhone app sends:**
///
/// ```json
/// {"token":"<lowercase hex>","environment":"sandbox"|"production","topic":"<bundle id>","preview_key":"…","previews":true}
/// {"platform":"fcm","token":"<FCM registration token>","topic":"<Android application id>","preview_key":"…","previews":true}
/// ```
///
/// `platform` is optional and means APNs when absent (`"apns"` is accepted and stored as absent,
/// so an explicit iPhone registration and the preserved app's registration are one record).
/// `deny_unknown_fields` stays: an unknown key is still refused, which is why an older Mac refuses
/// the FCM shape outright rather than filing an FCM token as an APNs one — the phone learns the
/// difference from the `native-push-fcm` capability. For FCM there is no `environment` (FCM has
/// no sandbox), matching the Worker's Android contract (`mobile/service/notifications.md`,
/// "Android (FCM, schema 3)", stream W `7f34a958`); a phone that sends one is refused.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    #[serde(default, skip_serializing_if = "Option::is_none")] pub platform: Option<String>,
    pub token: String,
    // Never empty for APNs, always empty for FCM — so an APNs record serializes exactly as before.
    #[serde(default, skip_serializing_if = "String::is_empty")] pub environment: String,
    pub topic: String,
    #[serde(default)] pub preview_key: Option<String>,
    #[serde(default = "preview_default")] pub previews: bool,
}
fn preview_default() -> bool { true }
/// FCM registration tokens are opaque; Google documents no format. The bound is the Worker's own
/// (`FCM_TOKEN` in `mobile/service/connect/fcm.mjs`: 32 to 4,096 characters of
/// `A-Z a-z 0-9 _ - :`), so this Mac never accepts a token the Worker would refuse, nor refuses
/// one it would take.
fn fcm_token(token: &str) -> bool {
    (32..=4096).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-:".contains(&b))
}
impl Registration {
    /// `"apns"` or `"fcm"` — the value recorded in the device's `push_transport`.
    pub fn transport(&self) -> &'static str { if self.platform.as_deref()==Some("fcm") {"fcm"} else {"apns"} }
    /// The canonical stored form: `platform` absent for APNs, so an explicit `"apns"` and the
    /// preserved app's registration are one record.
    pub fn normalized(mut self) -> Self {
        if self.platform.as_deref()==Some("apns") {self.platform=None;}
        self
    }
    pub fn validate(&self) -> bool {
        let preview_ok = self.preview_key.as_ref().is_none_or(|key| unb64url(key).is_ok_and(|bytes| bytes.len()==32));
        match self.platform.as_deref() {
            None | Some("apns") => preview_ok
                && (32..=512).contains(&self.token.len()) && self.token.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                && ["sandbox","production"].contains(&self.environment.as_str())
                && APNS_TOPICS.contains(&self.topic.as_str()),
            Some("fcm") => preview_ok
                && fcm_token(&self.token)
                && self.environment.is_empty()
                && FCM_APPS.contains(&self.topic.as_str()),
            Some(_) => false,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Target { device: String, hash: String, registration: Registration, route: String }
#[derive(Clone, Serialize, Deserialize)]
struct Event { event: String, thread: String, expires: u64, #[serde(default)] preview: Option<Value> }
#[derive(Default, Serialize, Deserialize)]
struct State {
    revision: u64, target: Option<Target>, dirty: bool, host: Option<String>, generation: u64,
    jobs: Vec<Event>, delivered: Vec<String>,
}
pub trait Control { fn call(&self, method: &str, path: &str, body: &str) -> Result<Reply, String>; }
impl Control for Client {
    fn call(&self, method:&str,path:&str,body:&str)->Result<Reply,String> {Client::call(self,method,path,body).map_err(|_| unavailable())}
}
pub fn unavailable() -> String { "Reply notifications could not reach the service. Your conversation still works. Retry notifications in settings.".into() }
pub struct Desk { path: PathBuf, state: State }
impl Desk {
    // The runtime's action mutex serializes this file with pairing and revocation.
    pub fn open(directory:&Path)->Result<Self,String> {
        let path=directory.join("phone/native-notifications.json");
        let state=match std::fs::read(&path) {
            Ok(bytes) if bytes.len()<=256_000=>serde_json::from_slice::<State>(&bytes).map_err(|_|unavailable())?,
            Err(e) if e.kind()==std::io::ErrorKind::NotFound=>State::default(),
            _=>return Err(unavailable()),
        };
        if state.jobs.len()>100 || state.delivered.len()>1000 {return Err(unavailable())}
        Ok(Self {path,state})
    }
    fn save(&self)->Result<(),String> {
        use std::{io::Write,os::unix::fs::OpenOptionsExt};
        std::fs::create_dir_all(self.path.parent().unwrap()).map_err(|_|unavailable())?;
        let pending=self.path.with_extension("pending");
        let mut file=std::fs::OpenOptions::new().create(true).truncate(true).write(true).mode(0o600).open(&pending).map_err(|_|unavailable())?;
        file.write_all(&serde_json::to_vec(&self.state).map_err(|_|unavailable())?).map_err(|_|unavailable())?;
        file.sync_all().map_err(|_|unavailable())?;
        std::fs::rename(pending,&self.path).map_err(|_|unavailable())?;
        std::fs::File::open(self.path.parent().unwrap()).and_then(|f|f.sync_all()).map_err(|_|unavailable())
    }
    pub fn set(&mut self,device:&Device,registration:Option<Registration>)->Result<(),String> {
        if registration.as_ref().is_some_and(|r|!r.validate()) {return Err("Invalid notification registration".into())}
        let registration=registration.map(Registration::normalized);
        self.state.revision=self.state.revision.saturating_add(1).max(super::now_millis());
        self.state.target=match registration {Some(registration)=>Some(Target {device:device.id.clone(),hash:hex(&sha256(&unb64url(&device.public_key).map_err(|_|unavailable())?)),registration,
            route:if device.paired_via==super::device::PairedVia::CONNECT {"connect"} else {"tailnet"}.into()}),None=>None};
        self.state.dirty=true;self.state.jobs.clear();self.state.delivered.clear();self.save()
    }
    pub fn clear(&mut self)->Result<(),String> {
        if self.state.target.is_none() && self.state.jobs.is_empty() {return Ok(())}
        self.state.revision=self.state.revision.saturating_add(1).max(super::now_millis());
        self.state.target=None;self.state.jobs.clear();self.state.dirty=true;self.save()
    }
    pub fn enabled_for(&self,device:&Device)->bool {self.state.target.as_ref().is_some_and(|t|t.device==device.id)}
    pub fn enqueue(&mut self,device:&Device,thread:&str,event:&str)->Result<(),String> {
        if !self.enabled_for(device) {return Ok(())}
        let event=hex(&sha256(event.as_bytes()));
        if self.state.delivered.contains(&event) || self.state.jobs.iter().any(|j|j.event==event) {return Ok(())}
        self.state.jobs.retain(|j|j.expires>super::now_millis());
        if self.state.jobs.len()>=100 {return Err(unavailable())}
        self.state.jobs.push(Event {event,thread:hex(&sha256(thread.as_bytes())),expires:super::now_millis()+3600000,preview:None});self.save()
    }
    pub fn reconcile(&mut self,control:&dyn Control,current:Option<&Device>)->Result<(),String> {
        if self.state.target.as_ref().is_some_and(|t|!current.is_some_and(|d|d.id==t.device && d.trusted())) {self.clear()?;}
        self.state.jobs.retain(|j|j.expires>super::now_millis());
        if !self.state.dirty && self.state.jobs.is_empty() {return Ok(())}
        let enrollment=control.call("POST","/v1/push/hosts","{}")?;
        if enrollment.status!=200 {return Err(unavailable())}
        let host=enrollment.value["hostId"].as_str().filter(|s|s.len()==32 && s.bytes().all(|b|b.is_ascii_hexdigit())).ok_or_else(unavailable)?;
        let generation=enrollment.value["generation"].as_u64().filter(|g|*g>0).ok_or_else(unavailable)?;
        if self.state.generation!=generation {self.state.dirty=true;self.state.revision=self.state.revision.saturating_add(1).max(super::now_millis());}
        self.state.host=Some(host.into());self.state.generation=generation;self.save()?;
        if self.state.dirty {
            if let Some(target)=&self.state.target {
                if target.route=="connect" {
                    let result=control.call("PUT","/v1/host/device",&json!({"generation":generation,"deviceKeyHash":target.hash}).to_string())?;
                    if result.status!=200 {return Err(unavailable())}
                }
            }
            let body=device_body(self.state.revision,generation,self.state.target.as_ref());
            let result=control.call("PUT","/v1/push/device",&body.to_string())?;
            if result.status!=200 {return Err(unavailable())}
            self.state.dirty=false;self.save()?;
        }
        while let (Some(target),Some(job))=(&self.state.target,self.state.jobs.first()) {
            let result=control.call("POST","/v1/push/events",&json!({"deviceHash":target.hash,"revision":self.state.revision,"eventRef":job.event,"threadRef":job.thread,"preview":job.preview}).to_string())?;
            if result.status==409 {self.state.dirty=true;self.state.revision=self.state.revision.saturating_add(1).max(super::now_millis());self.save()?;return Err(unavailable())}
            if result.status!=202 {return Err(unavailable())}
            self.state.delivered.push(job.event.clone());if self.state.delivered.len()>1000 {self.state.delivered.remove(0);}
            self.state.jobs.remove(0);self.save()?;
        }
        Ok(())
    }
    pub fn needs_reconcile(&self,current:Option<&Device>)->bool {
        self.state.dirty || !self.state.jobs.is_empty() || self.state.target.as_ref().is_some_and(|t|!current.is_some_and(|d|d.id==t.device && d.trusted()))
    }
    pub fn response(&self)->Value {json!({"host_id":self.state.host,"registered":self.state.target.is_some() && !self.state.dirty})}
}

/// **THE `PUT /v1/push/device` BODY THE WORKER RECEIVES.** For APNs it is exactly the key set the
/// Worker has always taken (`mobile/service/connect/notifications.mjs:10`, an `exact(...)` check
/// that refuses any other key) — no `platform` key, so an APNs registration keeps working against
/// the Worker as deployed. An FCM registration is the Worker's Android shape, exactly
/// (`mobile/service/notifications.md`, "Android (FCM, schema 3)", stream W `7f34a958`):
/// `{revision, generation, deviceHash, token, platform:"fcm", topic, route}` with NO
/// `environment`. A Worker without Android push refuses it with a 400; the Mac then answers the
/// phone 503 `unreachable` and the phone's conversation is unaffected.
fn device_body(revision:u64,generation:u64,target:Option<&Target>)->Value {
    match target {
        Some(t) if t.registration.transport()=="fcm"=>json!({"revision":revision,"generation":generation,"deviceHash":t.hash,"token":t.registration.token,"platform":"fcm","topic":t.registration.topic,"route":t.route}),
        Some(t)=>json!({"revision":revision,"generation":generation,"deviceHash":t.hash,"token":t.registration.token,"environment":t.registration.environment,"topic":t.registration.topic,"route":t.route}),
        None=>json!({"revision":revision,"generation":generation,"deviceHash":null,"token":null,"route":"tailnet"}),
    }
}

pub fn queue_reply(desk:&mut Desk,device:&Device,thread:&str,payload:&Value,turn:&str)->Result<(),String> {
    let Some(row)=super::rows::reply_for_turn(payload,turn) else {return Ok(())};
    if device.has_read(thread,row["cursor"].as_u64()) {return Ok(())}
    let Some(event)=row["id"].as_str() else {return Ok(())};
    desk.enqueue(device,thread,event)?;
    let event=hex(&sha256(event.as_bytes()));
    if let (Some(target),Some(job))=(&desk.state.target,desk.state.jobs.iter_mut().find(|job|job.event==event)) {
        if target.registration.previews && job.preview.is_none() {
            if let Some(key)=&target.registration.preview_key {
                job.preview=Some(seal_preview(key,&job.thread,&job.event,row["text"].as_str().unwrap_or(""))?);
                desk.save()?;
            }
        }
    }
    Ok(())
}

fn seal_preview(key:&str,thread:&str,event:&str,text:&str)->Result<Value,String> {
    use ring::aead::{Aad,LessSafeKey,Nonce,UnboundKey,AES_256_GCM};
    let key=LessSafeKey::new(UnboundKey::new(&AES_256_GCM,&unb64url(key).map_err(|_|unavailable())?).map_err(|_|unavailable())?);
    let nonce: [u8;12]=super::random_bytes(12).map_err(|_|unavailable())?.try_into().map_err(|_|unavailable())?;
    let mut body=text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(240).collect::<String>().into_bytes();
    let aad=format!("richos-preview-v1\n{thread}\n{event}");
    key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce),Aad::from(aad.as_bytes()),&mut body).map_err(|_|unavailable())?;
    Ok(json!({"v":1,"nonce":super::b64url(&nonce),"body":super::b64url(&body)}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Scratch(PathBuf);
    // Said, not asserted: a panic inside Drop while a failing test unwinds would abort the run.
    impl Drop for Scratch {fn drop(&mut self){if let Err(e)=std::fs::remove_dir_all(&self.0){eprintln!("test scratch {} was not removed: {e}",self.0.display());}}}
    fn fixture()->(Scratch,Device,Registration) {
        let dir=Scratch(std::env::temp_dir().join(format!("native-push-{}",hex(&super::super::random_bytes(12).unwrap()))));
        std::fs::create_dir_all(&dir.0).unwrap();
        let device:Device=serde_json::from_value(json!({"id":"phone","name":"iPhone","public_key":super::super::b64url(&[4;65]),"paired_at":1,"push":null,"delivered_cursor":null,"fingerprint_confirmed":true,"paired_via":"connect"})).unwrap();
        (dir,device,Registration{platform:None,token:"a".repeat(64),environment:"sandbox".into(),topic:"dev.richos.mobile.integration".into(),preview_key:None,previews:true})
    }
    #[derive(Default)] struct Fake {calls:Mutex<Vec<(String,Value)>>, fail:Mutex<bool>, fail_events:Mutex<bool>}
    impl Control for Fake {
        fn call(&self,_method:&str,path:&str,body:&str)->Result<Reply,String> {
            self.calls.lock().unwrap().push((path.into(),serde_json::from_str(body).unwrap()));
            if *self.fail.lock().unwrap() || (path.ends_with("events") && *self.fail_events.lock().unwrap()) {return Err("offline".into())}
            Ok(Reply {status:if path.ends_with("events"){202}else{200},value:json!({"hostId":"a".repeat(32),"generation":1})})
        }
    }
    #[test] fn registration_intent_and_event_retry_survive_restart_without_content() {
        let (dir,device,registration)=fixture();let fake=Fake::default();*fake.fail.lock().unwrap()=true;
        let mut desk=Desk::open(&dir.0).unwrap();desk.set(&device,Some(registration)).unwrap();
        assert!(desk.reconcile(&fake,Some(&device)).is_err());drop(desk);
        let mut desk=Desk::open(&dir.0).unwrap();*fake.fail.lock().unwrap()=false;desk.reconcile(&fake,Some(&device)).unwrap();
        desk.enqueue(&device,"private title","private reply").unwrap();desk.reconcile(&fake,Some(&device)).unwrap();
        desk.enqueue(&device,"private title","private reply").unwrap();desk.reconcile(&fake,Some(&device)).unwrap();
        let calls=fake.calls.lock().unwrap();assert_eq!(calls.iter().filter(|(path,_)|path.ends_with("events")).count(),1);
        assert!(!serde_json::to_string(&*calls).unwrap().contains("private"));
        assert!(!std::fs::read_to_string(&desk.path).unwrap().contains("private"));
    }
    #[test] fn revocation_clears_queued_events_before_contacting_provider() {
        let (dir,device,registration)=fixture();let fake=Fake::default();
        let mut desk=Desk::open(&dir.0).unwrap();desk.set(&device,Some(registration)).unwrap();desk.enqueue(&device,"thread","reply").unwrap();
        *fake.fail.lock().unwrap()=true;assert!(desk.reconcile(&fake,None).is_err());drop(desk);
        let mut desk=Desk::open(&dir.0).unwrap();assert!(!desk.enabled_for(&device));assert!(desk.state.jobs.is_empty());
        *fake.fail.lock().unwrap()=false;desk.reconcile(&fake,None).unwrap();
        let calls=fake.calls.lock().unwrap();assert!(calls.iter().all(|(path,_)|!path.ends_with("events")));
        assert_eq!(calls.last().unwrap().1["token"],Value::Null);
    }
    #[test] fn failed_reply_delivery_remains_pending_after_restart() {
        let (dir,device,registration)=fixture();let fake=Fake::default();
        let mut desk=Desk::open(&dir.0).unwrap();
        desk.set(&device,Some(registration)).unwrap();desk.reconcile(&fake,Some(&device)).unwrap();
        desk.enqueue(&device,"thread","reply").unwrap();
        *fake.fail_events.lock().unwrap()=true;
        assert!(desk.reconcile(&fake,Some(&device)).is_err());drop(desk);
        let mut desk=Desk::open(&dir.0).unwrap();
        assert!(desk.needs_reconcile(Some(&device)));
        assert_eq!(desk.state.jobs.len(),1);
        *fake.fail_events.lock().unwrap()=false;
        desk.reconcile(&fake,Some(&device)).unwrap();
        assert!(!desk.needs_reconcile(Some(&device)));
        drop(desk);
        let mut desk=Desk::open(&dir.0).unwrap();
        desk.reconcile(&fake,Some(&device)).unwrap();
        let calls=fake.calls.lock().unwrap();
        assert_eq!(calls.iter().filter(|(path,_)|path.ends_with("events")).count(),2);
    }
    #[test] fn previews_are_authenticated_ciphertext_and_registration_keys_stay_on_mac() {
        use ring::aead::{Aad,LessSafeKey,Nonce,UnboundKey,AES_256_GCM};
        let (dir,device,mut registration)=fixture();let fake=Fake::default();
        let raw=[7u8;32];registration.preview_key=Some(super::super::b64url(&raw));
        let mut desk=Desk::open(&dir.0).unwrap();desk.set(&device,Some(registration)).unwrap();desk.reconcile(&fake,Some(&device)).unwrap();
        assert!(!serde_json::to_string(&*fake.calls.lock().unwrap()).unwrap().contains("preview_key"));
        let thread="b".repeat(64);let event="c".repeat(64);
        let sealed=seal_preview(&super::super::b64url(&raw),&thread,&event,"The supplier accepted £42,000.").unwrap();
        let nonce: [u8;12]=unb64url(sealed["nonce"].as_str().unwrap()).unwrap().try_into().unwrap();
        let mut bytes=unb64url(sealed["body"].as_str().unwrap()).unwrap();
        assert!(!sealed.to_string().contains("supplier"));
        let key=LessSafeKey::new(UnboundKey::new(&AES_256_GCM,&raw).unwrap());
        let aad=format!("richos-preview-v1\n{thread}\n{event}");
        assert_eq!(key.open_in_place(Nonce::assume_unique_for_key(nonce),Aad::from(aad.as_bytes()),&mut bytes).unwrap(),"The supplier accepted £42,000.".as_bytes());
        let mut bytes=unb64url(sealed["body"].as_str().unwrap()).unwrap();
        assert!(key.open_in_place(Nonce::assume_unique_for_key(nonce),Aad::from(b"wrong reply"),&mut bytes).is_err());
    }
    /// **A DELIVERED MARK IN ONE CONVERSATION DOES NOT SILENCE ANOTHER** (hunt 2026-09-29, part 1,
    /// finding 14). Row cursors number each conversation from 1, but the mark was one number for
    /// the whole phone: having read row 100 of a long thread suppressed the push for row 3 of a
    /// new one. Built from JSON so the same test runs against the old record shape.
    #[test] fn a_reply_in_a_short_conversation_is_not_suppressed_by_reading_a_long_one() {
        let (dir,_,registration)=fixture();
        let device_with=|extra:Value|{
            let mut record=json!({"id":"phone","name":"iPhone","public_key":super::super::b64url(&[4;65]),"paired_at":1,"push":null,"delivered_cursor":null,"fingerprint_confirmed":true,"paired_via":"connect"});
            for (key,value) in extra.as_object().unwrap() {record[key]=value.clone();}
            serde_json::from_value::<Device>(record).unwrap()
        };
        let reply=|id:&str,count:usize|{
            let items:Vec<Value>=(1..=count).map(|n|json!({"kind":"rich_message","id":format!("{id}-{n}"),"threadId":id,"turnId":id,"text":"Done.","createdAt":n})).collect();
            json!({"items":items})
        };
        // He has read 100 rows of thread A. The old record says so device-wide; the new one says so for A.
        let read_a_long_thread=device_with(json!({"delivered_cursor":100,"delivered_cursors":{"thread-a":100}}));
        let mut desk=Desk::open(&dir.0).unwrap();desk.set(&read_a_long_thread,Some(registration)).unwrap();
        queue_reply(&mut desk,&read_a_long_thread,"thread-b",&reply("thread-b",3),"thread-b").unwrap();
        assert_eq!(desk.state.jobs.len(),1,"row 3 of another conversation was suppressed by row 100 of thread A");
        // POSITIVE CONTROL: the guard still does its job for the conversation it was recorded for.
        queue_reply(&mut desk,&read_a_long_thread,"thread-a",&reply("thread-a",100),"thread-a").unwrap();
        assert_eq!(desk.state.jobs.len(),1,"a reply he had already read in thread A was announced again");
        // And in its own conversation a NEWER row is still announced.
        queue_reply(&mut desk,&read_a_long_thread,"thread-a",&reply("thread-a",101),"thread-a").unwrap();
        assert_eq!(desk.state.jobs.len(),2);
    }
    #[test] fn malformed_native_registrations_are_rejected() {
        let (_,_,mut r)=fixture();assert!(r.validate());r.topic="unrelated.app".into();assert!(!r.validate());
        r.topic="dev.richos.mobile.loop".into();r.environment="test".into();assert!(!r.validate());
        r.environment="production".into();r.token="../somewhere".into();assert!(!r.validate());
    }

    /// An FCM-shaped token, built rather than written out: `<instance>:APA91b<payload>`.
    fn fcm_shaped()->String {format!("{}:APA91b{}",  "d".repeat(11), "x-y_z".repeat(28))}

    /// **THE PRESERVED iPHONE APP'S REGISTRATION, PINNED AS BYTES** — every hop it takes: what the
    /// phone sends, what this Mac stores on disk, and what it sends the Worker. The additions in
    /// this file are all behind a `platform` key the preserved app never sends, and this is the
    /// test that fails if one of them leaks into its path.
    #[test] fn the_preserved_apns_registration_is_unchanged_on_the_wire_on_disk_and_to_the_worker() {
        let key=super::super::b64url(&[7;32]);
        let sent=format!(r#"{{"token":"{}","environment":"sandbox","topic":"dev.richos.mobile.loop","preview_key":"{key}","previews":true}}"#,"a".repeat(64));
        let r:Registration=serde_json::from_str(&sent).unwrap();
        assert!(r.validate());assert_eq!(r.transport(),"apns");
        let stored=r.clone().normalized();
        assert_eq!(stored,r,"normalizing changed an APNs registration");
        // The Target record on disk serializes the registration exactly as the pre-FCM struct did:
        // no `platform` key, same field order.
        assert_eq!(serde_json::to_string(&stored).unwrap(),sent);
        let target=Target{device:"phone".into(),hash:"b".repeat(64),registration:stored,route:"connect".into()};
        let body=device_body(5,1,Some(&target));
        let mut keys:Vec<&str>=body.as_object().unwrap().keys().map(String::as_str).collect();keys.sort_unstable();
        // `mobile/service/connect/notifications.mjs:10` — the deployed Worker's exact key set.
        assert_eq!(keys,["deviceHash","environment","generation","revision","route","token","topic"]);
        assert_eq!(body,json!({"revision":5,"generation":1,"deviceHash":"b".repeat(64),"token":"a".repeat(64),"environment":"sandbox","topic":"dev.richos.mobile.loop","route":"connect"}));
        assert_eq!(device_body(5,1,None),json!({"revision":5,"generation":1,"deviceHash":null,"token":null,"route":"tailnet"}));
        // `"platform":"apns"` spelled out is the same registration, stored the same way.
        let explicit:Registration=serde_json::from_str(&sent.replacen('{',r#"{"platform":"apns","#,1)).unwrap();
        assert!(explicit.validate());assert_eq!(explicit.normalized(),r);
        // And the two preserved topics are still the first two allowed.
        assert_eq!(&APNS_TOPICS[..2],["dev.richos.mobile.loop","dev.richos.mobile.integration"]);
    }

    #[test] fn the_new_native_apps_register_under_their_own_ids_and_only_on_their_own_service() {
        let (_,_,mut r)=fixture();
        r.topic="dev.richos.native.ios".into();assert!(r.validate(),"the native iPhone app's APNs topic");
        r.topic="dev.richos.native.android".into();assert!(!r.validate(),"an Android application id as an APNs topic");
        let fcm:Registration=serde_json::from_value(json!({"platform":"fcm","token":fcm_shaped(),"topic":"dev.richos.native.android"})).unwrap();
        assert!(fcm.validate());assert_eq!(fcm.transport(),"fcm");assert!(fcm.previews,"previews default on, as for APNs");
        let stored=fcm.clone().normalized();assert_eq!(stored,fcm);
        assert!(!serde_json::to_string(&stored).unwrap().contains("environment"),"an FCM record carries no environment");
        let mut ios_as_fcm=fcm.clone();ios_as_fcm.topic="dev.richos.native.ios".into();assert!(!ios_as_fcm.validate());
    }

    /// The team's TEST copy of RichConnect (`dev.richos.connect.perf`) registers under its own ID on
    /// both phones (CEO, 2026-10-03), and the real app's ID and every earlier entry keep their place.
    #[test] fn the_test_copy_registers_under_its_own_id_and_the_real_app_is_unchanged() {
        let (_,_,mut apns)=fixture();
        for topic in ["dev.richos.connect","dev.richos.connect.perf"] {
            apns.topic=topic.into();assert!(apns.validate(),"{topic} as an APNs topic");assert_eq!(apns.transport(),"apns");
            let fcm:Registration=serde_json::from_value(json!({"platform":"fcm","token":fcm_shaped(),"topic":topic})).unwrap();
            assert!(fcm.validate(),"{topic} as an FCM application id");assert_eq!(fcm.transport(),"fcm");
        }
        apns.topic="dev.richos.connect.other".into();assert!(!apns.validate(),"only the one test copy is added");
        assert_eq!(&APNS_TOPICS[..4],["dev.richos.mobile.loop","dev.richos.mobile.integration","dev.richos.native.ios","dev.richos.connect"]);
        assert_eq!(&FCM_APPS[..2],["dev.richos.native.android","dev.richos.connect"]);
    }

    #[test] fn fcm_registrations_reach_the_worker_marked_as_fcm_and_survive_restart() {
        let (dir,device,_)=fixture();let fake=Fake::default();
        let fcm:Registration=serde_json::from_value(json!({"platform":"fcm","token":fcm_shaped(),"topic":"dev.richos.native.android","preview_key":super::super::b64url(&[9;32])})).unwrap();
        let mut desk=Desk::open(&dir.0).unwrap();desk.set(&device,Some(fcm)).unwrap();drop(desk);
        let mut desk=Desk::open(&dir.0).unwrap();desk.reconcile(&fake,Some(&device)).unwrap();
        let calls=fake.calls.lock().unwrap();
        let (_,body)=calls.iter().find(|(path,_)|path=="/v1/push/device").unwrap();
        // Exactly the Worker's Android shape (stream W `7f34a958`): no `environment`, `platform` present.
        let mut keys:Vec<&str>=body.as_object().unwrap().keys().map(String::as_str).collect();keys.sort_unstable();
        assert_eq!(keys,["deviceHash","generation","platform","revision","route","token","topic"]);
        assert_eq!(body["platform"],"fcm");assert_eq!(body["topic"],"dev.richos.native.android");
        assert_eq!(body["token"],json!(fcm_shaped()));
        assert!(!serde_json::to_string(&*calls).unwrap().contains("preview_key"),"the preview key left the Mac");
    }

    #[test] fn malformed_fcm_registrations_are_rejected() {
        let good=json!({"platform":"fcm","token":fcm_shaped(),"topic":"dev.richos.native.android"});
        let parsed=|v:&Value|serde_json::from_value::<Registration>(v.clone());
        assert!(parsed(&good).unwrap().validate());
        let mut longest=good.clone();longest["token"]=json!("x".repeat(4096));
        assert!(parsed(&longest).unwrap().validate(),"the Worker's own ceiling must be accepted here too");
        for (field,value) in [("token",json!(format!("{} {}",fcm_shaped(),"x"))),("token",json!("short")),("token",json!("x".repeat(4097))),("environment",json!("production")),
            ("token",json!(format!("{}\"",fcm_shaped()))),("token",json!(format!("{}/..",fcm_shaped()))),("topic",json!("dev.richos.mobile.loop")),
            ("environment",json!("sandbox")),("platform",json!("hms")),("platform",json!("FCM")),("preview_key",json!("short"))] {
            let mut bad=good.clone();bad[field]=value.clone();
            assert!(parsed(&bad).map(|r|!r.validate()).unwrap_or(true),"{field}={value} was accepted");
        }
        // Unknown fields are still refused outright, for both platforms.
        let mut extra=good.clone();extra["sender_id"]=json!("123");assert!(parsed(&extra).is_err());
        let mut missing=good.clone();missing.as_object_mut().unwrap().remove("token");assert!(parsed(&missing).is_err());
    }
}
