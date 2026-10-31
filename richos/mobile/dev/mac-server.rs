//! Real Rust phone HTTP/auth/SSE, durable intake, Spine and gated timeline.
//! Only cognition and the Tauri AppHandle adapter are replaced. No model credentials,
//! production pairing, login keychain, microphone or desktop window are accessed.
//! Started by `mobile.mjs lab mac`; this module exists only in the Rust test binary.
use crate::phone::{
    self,
    api_base::ApiBaseDesk,
    ca::PhoneCa,
    device::DeviceDesk,
    listen::{tls_config, Listener},
    names::LocalNames,
    routes::{Accepted, Bridge, Channel, StopSwitch},
    stream::{PhoneHub, PhoneLiveEmitter},
};
use richos_core::{
    cognition::MockLeaseFactory,
    entity::{Entity, EntityId, EntityRegistry},
    ledger::Ledger,
    spine::Spine,
    steering::TurnControl,
};
use serde_json::{json, Value};
use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

// Test-only storage survives an isolated process restart. Production uses Keychain.
struct LabSecrets { directory: PathBuf }
impl phone::secrets::SecretStore for LabSecrets {
    fn get(&self, account: &str) -> Result<Option<Vec<u8>>, phone::PhoneError> {
        let path = self.directory.join(format!("secret-{}",phone::hex(&phone::sha256(account.as_bytes()))));
        match std::fs::read(path) { Ok(bytes)=>Ok(Some(bytes)), Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(None), Err(e)=>Err(e.into()) }
    }
    fn put(&self, account: &str, bytes: &[u8]) -> Result<(), phone::PhoneError> {
        use std::os::unix::fs::OpenOptionsExt;
        use std::io::Write;
        let path = self.directory.join(format!("secret-{}",phone::hex(&phone::sha256(account.as_bytes()))));
        let mut file = std::fs::OpenOptions::new().create(true).truncate(true).write(true).mode(0o600).open(path)?;
        file.write_all(bytes)?; file.sync_all()?; Ok(())
    }
    fn delete(&self, account: &str) -> Result<(), phone::PhoneError> {
        let path = self.directory.join(format!("secret-{}",phone::hex(&phone::sha256(account.as_bytes()))));
        match std::fs::remove_file(path) { Ok(())=>Ok(()), Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(()), Err(e)=>Err(e.into()) }
    }
}

struct IsolatedBridge {
    spine: Arc<Mutex<Spine>>,
    control: TurnControl,
    thread: String,
    entity: EntityId,
    workers: Mutex<Vec<std::thread::JoinHandle<()>>>,
    voice: Option<phone::voice::VoiceDesk>,
    devices: Arc<DeviceDesk>,
    push: Arc<Mutex<phone::notifications::Desk>>,
    push_client: Option<Arc<phone::connect::client::Client>>,
    reply_delay: Duration,
    vapid: Arc<phone::push::VapidKey>,
    origin: String,
    /// The lab's engine state: the files Rich attached (`richos_core::rich_files`), as on a Mac.
    files_root: PathBuf,
    /// Files the lab attaches under each reply, as Rich's `attach_file` tool does
    /// (`RICHOS_MOBILE_LAB_ATTACH`, paths joined by `:`); none when unset.
    attach: Vec<PathBuf>,
    hub: Arc<PhoneHub>,

}
impl Bridge for IsolatedBridge {
    fn voice_available(&self)->bool {self.voice.as_ref().is_some_and(|v|v.available())}
    fn transcribe(&self,bytes:&[u8])->Result<String,String>{self.voice.as_ref().ok_or("Voice test not enabled")?.transcribe(bytes)}
    fn reply_audio(&self,thread:Option<&str>,id:&str)->Result<Vec<u8>,String> {
        let payload=self.snapshot(thread)?;let rows=phone::rows::rows_from_payload(&payload);
        let row=rows.iter().find(|r|r["id"]==id && r["role"]=="rich" && r["complete"]!=false).ok_or("reply unavailable")?;
        self.voice.as_ref().ok_or("Voice test not enabled")?.synthesize(row["text"].as_str().unwrap_or(""))
    }
    fn native_notifications_available(&self)->bool {self.push_client.is_some()}
    fn register_native_notifications(&self,id:&str,registration:Option<phone::notifications::Registration>)->Result<Value,String> {
        let device=self.devices.paired().filter(|d|d.id==id && d.trusted()).ok_or("not paired")?;
        let mut desk=self.push.lock().unwrap();desk.set(&device,registration)?;
        self.devices.use_native_push(id).map_err(|_|"device unavailable")?;
        desk.reconcile(self.push_client.as_ref().ok_or("push unavailable")?.as_ref(),Some(&device))?;Ok(desk.response())
    }

    fn submit_text(&self, thread_id: Option<&str>, text: &str, client_id: &str) -> Result<Accepted, String> {
        self.submit_words(thread_id, text, client_id, false)
    }
    /// A voice note's transcript, written as one, exactly as the production bridge writes it
    /// (`phone/bridge.rs` `submit_voice`), so this lab's rows are the rows a phone gets.
    fn submit_voice(&self, thread_id: Option<&str>, text: &str, client_id: &str) -> Result<Accepted, String> {
        self.submit_words(thread_id, text, client_id, true)
    }
    fn snapshot(&self, thread_id: Option<&str>) -> Result<Value, String> {
        let thread = thread_id.unwrap_or(&self.thread);
        let mut payload = crate::timeline_view::timeline_payload(&*self.spine.lock().unwrap(), thread)?;
        // The cards of the files Rich attached, merged as the production snapshot merges them
        // (`question_host::payload_in`), so this lab's rows are the rows a phone gets.
        if let Some(items) = payload["items"].as_array_mut() {
            items.extend(crate::rich_files_host::items(&self.files_root, self.entity.as_str(), thread));
        }
        Ok(payload)
    }
    /// "files" in the hello only when this lab attaches files, so every other lab run is as before.
    fn files_available(&self) -> bool {
        !self.attach.is_empty()
    }
    fn rich_file(&self, thread: &str, id: &str) -> Result<(String, Vec<u8>), richos_core::rich_files::Fetch> {
        richos_core::rich_files::read_for_phone(&self.files_root, thread, id).map(|(row, body)| (row.name, body))
    }
    fn current_thread(&self) -> Option<(String, String)> {
        Some((self.thread.clone(), "Isolated mobile check".into()))
    }
    fn threads(&self) -> Vec<(String, String)> {
        vec![self.current_thread().unwrap()]
    }
}

impl IsolatedBridge {
    fn submit_words(&self, thread_id: Option<&str>, text: &str, client_id: &str, voice_note: bool) -> Result<Accepted, String> {
        let thread = thread_id.unwrap_or(&self.thread);
        if thread != self.thread {
            return Err("Unknown isolated conversation".into());
        }
        let record = if voice_note {
            self.control.submit_voice_note_from_channel(thread, Some(self.entity.clone()), text, "phone", Some(client_id))
        } else {
            self.control.submit_from_channel_as(thread, Some(self.entity.clone()), text, "phone", Some(client_id))
        }
        .map_err(|e| e.to_string())?;
        let spine = Arc::clone(&self.spine);
        let devices=Arc::clone(&self.devices);let push=Arc::clone(&self.push);let client=self.push_client.clone();
        let reply_thread=thread.to_string();let delay=self.reply_delay;
        let vapid=Arc::clone(&self.vapid);let origin=self.origin.clone();
        let files_root=self.files_root.clone();let attach=self.attach.clone();let hub=Arc::clone(&self.hub);
        let entity=self.entity.as_str().to_string();

        self.workers
            .lock()
            .unwrap()
            .push(std::thread::spawn(move || {
                std::thread::sleep(delay);
                spine
                    .lock()
                    .unwrap()
                    .poll_intake()
                    .expect("isolated phone intake drain");
                if !attach.is_empty() {
                    let payload=crate::timeline_view::timeline_payload(&*spine.lock().unwrap(),&reply_thread).unwrap();
                    let turn=payload["items"].as_array().and_then(|items|items.iter().rev().find(|item|item["kind"]=="rich_message"))
                        .and_then(|item|item["turnId"].as_str()).map(str::to_string);
                    if let Some(turn)=turn {
                        for file in &attach {
                            match richos_core::rich_files::attach(&files_root,&entity,&reply_thread,&turn,"front_desk",&file.to_string_lossy()) {
                                Ok(row)=>{
                                    let item=crate::rich_files_host::item(&row);
                                    if let Some(mut row)=phone::rows::row_from_item(&item) {
                                        let cursor=hub.next_cursor();row["cursor"]=json!(cursor);
                                        drop(hub.publish("message",cursor,row.to_string()));
                                    }
                                    eprintln!("mac-server: attached {} to turn {turn}",row.name);
                                }
                                Err(error)=>eprintln!("mac-server: could not attach {}: {error}",file.display()),
                            }
                        }
                    }
                }
                if let Some(device)=devices.paired() {
                    if let Some(subscription)=device.push.as_ref() {
                        let payload=crate::timeline_view::timeline_payload(&*spine.lock().unwrap(),&reply_thread).unwrap();
                        let rows=phone::rows::rows_from_payload(&payload);
                        if let Some(last)=rows.iter().rev().find(|row|row["role"]=="rich") {
                            if !device.has_read(&reply_thread,last["cursor"].as_u64()) && phone::push::should_notify(&devices,&device.id,&reply_thread,last["id"].as_str().unwrap_or("")) {
                                // Use the production encryption, VAPID and vendor validation against the real phone subscription.
                                let body=json!({"notification":{"title":"Rich","body":last["text"],
                                    "navigate":format!("/#thread={}&at={}",reply_thread,last["id"].as_str().unwrap())},
                                    "message":last,"api_base":origin,"tier":"interrupt_now","truncated":false}).to_string();
                                assert!(phone::push::fits_in_one_push(body.as_bytes()),"Keep isolated acknowledgement messages short");
                                let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                                runtime.block_on(async {
                                    let http=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();
                                    match phone::push::send(&http,&vapid,subscription,body.as_bytes(),"high").await {
                                        Ok(delivery)=>eprintln!("isolated Web Push delivery status: {}",delivery.status),
                                        Err(error)=>eprintln!("isolated Web Push delivery failed: {error}"),
                                    }
                                });
                            }
                        }
                    }
                }
                if let (Some(client),Some(device))=(client,devices.paired()) {
                    if let Ok(payload)=crate::timeline_view::timeline_payload(&*spine.lock().unwrap(),&reply_thread) {
                        let mut desk=push.lock().unwrap();
                        if let Some(turn)=payload["items"].as_array()
                            .and_then(|items|items.iter().rev().find(|item|item["kind"]=="rich_message"))
                            .and_then(|item|item["turnId"].as_str()) {
                            phone::notifications::queue_reply(&mut desk,&device,&reply_thread,&payload,turn).unwrap();
                        }
                        let result=desk.reconcile(client.as_ref(),Some(&device));
                        eprintln!("native notification reconciliation succeeded: {}",result.is_ok());
                    }
                }

            }));
        Ok(Accepted {
            message_id: format!("intake_{}", record.id()),
            thread_id: thread.into(),
            at: phone::now_millis(),
        })
    }
}

#[test]
#[ignore = "CLI-owned isolated server; requires an explicit scratch directory and stop signal"]
fn serve() {
    let dir = PathBuf::from(
        std::env::var_os("RICHOS_MOBILE_MAC_DIR").expect("CLI must supply isolated scratch"),
    );
    let dir = dir.canonicalize().unwrap();
    richos_core::machine_settings::validate_existing_storage(&dir).unwrap();
    let restarting = std::env::var("RICHOS_MOBILE_MAC_RESTART").as_deref() == Ok("1");
    if restarting {
        assert_eq!(std::fs::read_to_string(dir.join("lab-owner")).unwrap(), "richos-mobile-isolated-v1");
    } else {
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(),0,"never reuse an existing data directory");
        std::fs::write(dir.join("lab-owner"),"richos-mobile-isolated-v1").unwrap();
    }
    let connect_token_file = std::env::var_os("RICHOS_MOBILE_CONNECT_TOKEN_FILE");
    let managed = connect_token_file.is_some();
    let origin = std::env::var("RICHOS_MOBILE_MAC_ORIGIN").expect("explicit public test origin");
    let names = LocalNames {
        host: "Mobile integration".into(),
        bonjour: "localhost".into(),
        addresses: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
    };
    let ca = PhoneCa::open(&dir, &LabSecrets {directory:dir.clone()}, names).unwrap();
    let mut spine = Spine::new(Ledger::open(dir.join("ledger.jsonl")).unwrap());
    let root = dir.to_str().unwrap();
    spine.set_entity_registry(
        EntityRegistry::new(vec![
            Entity::new("mobile-check", "Mobile check", &[root]).unwrap()
        ])
        .unwrap(),
    );
    let entity = EntityId::parse("mobile-check").unwrap();
    let thread = if restarting { std::fs::read_to_string(dir.join("lab-thread")).unwrap() }
        else { let thread=spine.create_thread("Isolated mobile check",&entity).unwrap();
            std::fs::write(dir.join("lab-thread"),&thread).unwrap(); thread };
    spine.switch_thread(&thread).unwrap();
    // Empty scripted replies make the existing test provider emit `ack: <fresh message>`.
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec![])));
    let control = TurnControl::open(dir.join("intake.jsonl")).unwrap();
    spine.set_turn_control(control.clone());
    let hub = PhoneHub::new();
    spine.set_live_observer(Box::new(PhoneLiveEmitter::new(Arc::clone(&hub))));
    let lab_hub = Arc::clone(&hub);
    let devices = Arc::new(DeviceDesk::open(&dir).unwrap());
    let push_client=std::env::var_os("RICHOS_MOBILE_CONNECT_IDENTITY_FILE").map(|file| {
        use phone::secrets::SecretStore;
        let file=PathBuf::from(file).canonicalize().unwrap();richos_core::machine_settings::validate_existing_storage(&file).unwrap();
        let secrets=LabSecrets{directory:dir.clone()};secrets.put("connect-host-identity-v1",&std::fs::read(file).unwrap()).unwrap();
        Arc::new(phone::connect::client::Client::new(phone::connect::client::Identity::open(&secrets).unwrap()))
    });
    use phone::secrets::SecretStore;
    let secrets=LabSecrets{directory:dir.clone()};
    let vapid=Arc::new(match secrets.get(phone::secrets::VAPID_KEY).unwrap() {
        Some(key)=>phone::push::VapidKey::from_pkcs8(&key).unwrap(),
        None=>{let key=phone::push::VapidKey::generate().unwrap();secrets.put(phone::secrets::VAPID_KEY,&key.pkcs8).unwrap();key},
    });
    let bridge = Arc::new(IsolatedBridge {
        spine: Arc::new(Mutex::new(spine)),
        control,
        thread,
        entity,
        workers: Mutex::new(vec![]),
        vapid:Arc::clone(&vapid),origin:origin.clone(),
        files_root:dir.join("engine-state"),hub:lab_hub,
        attach:std::env::var("RICHOS_MOBILE_LAB_ATTACH").ok().map(|list|list.split(':').filter(|p|!p.is_empty()).map(PathBuf::from).collect()).unwrap_or_default(),
        voice:if std::env::var("RICHOS_MOBILE_VOICE_TEST").as_deref()==Ok("1") {Some(phone::voice::VoiceDesk::new(dir.join("speech")))} else {None},
        devices:Arc::clone(&devices),push:Arc::new(Mutex::new(phone::notifications::Desk::open(&dir).unwrap())),push_client,
        reply_delay:Duration::from_millis(std::env::var("RICHOS_MOBILE_REPLY_DELAY_MS").ok().and_then(|s|s.parse::<u64>().ok()).unwrap_or(0).min(10000)),
    });
    let channel = Arc::new(Channel {
        devices: Arc::clone(&devices),
        rejected: StopSwitch::unwired(),
        api_base: Arc::new(ApiBaseDesk::only(&origin)),
        hub,
        bridge: bridge.clone(),
        vapid_public: vapid.application_server_key(),
        fingerprint_hex: ca.fingerprint_hex(),
        pairing_path: Mutex::new(if managed { phone::device::PairedVia::CONNECT } else { phone::device::PairedVia::TAILNET }),
    });
    let mut listener = if managed {
        Listener::start_connect(channel, phone::connect::PORT).unwrap()
    } else {
        Listener::start(channel, tls_config(&ca.leaf_der,&ca.leaf_key_pkcs8).unwrap(), &[IpAddr::V4(Ipv4Addr::LOCALHOST)],0).unwrap()
    };
    let _connector = connect_token_file.map(|file| {
        let path = PathBuf::from(file).canonicalize().unwrap();
        richos_core::machine_settings::validate_existing_storage(&path).unwrap();
        let token = std::fs::read_to_string(path).unwrap();
        let helper = std::env::var_os("RICHOS_CONNECT_HELPER").expect("explicit test helper");
        phone::connect::supervisor::Supervisor::start(&PathBuf::from(helper),token).unwrap()
    });
    if !devices.is_paired() { devices.open_pairing().unwrap(); }
    let ready = json!({"protocol":if managed {"http"} else {"https"},"port":listener.bound[0].port(), "ca":dir.join("phone/ca.crt"),
        "pairLink":devices.pairing_window().map(|window|format!("{origin}/#pair={}",window.code)),
        "words":ca.fingerprint_words().join(" "), "replyMarker":"ack: ", "pid":std::process::id()});
    std::fs::write(dir.join("ready.new"), ready.to_string()).unwrap();
    std::fs::rename(dir.join("ready.new"), dir.join("ready.json")).unwrap();
    let manual = std::env::var("RICHOS_MOBILE_MAC_MANUAL").as_deref() == Ok("1");
    let started = Instant::now();
    while !dir.join("stop").exists() && session_within_lifetime(manual, started.elapsed()) {
        // THE PERSON AT THE MAC (Sage's pairing review F1). The production Mac refuses everything
        // from a phone until someone presses They match on the Mac's own sheet, and this harness
        // has no sheet. So it presses on the person's behalf once the phone has given its own
        // answer — the order a person standing at both screens produces — and says so. The gate
        // itself is proved by the Rust tests and the conformance verifier, not here.
        if devices.paired().is_some_and(|d| d.fingerprint_confirmed && !d.mac_confirmed) {
            match devices.confirm_on_mac() {
                Ok(()) => eprintln!("mac-server: pressed They match on the Mac's behalf (this harness has no Mac sheet)"),
                Err(error) => eprintln!("mac-server: could not press They match: {error}"),
            }
        }
        if let Ok(payload) = bridge.snapshot(None) {
            std::fs::write(dir.join("timeline.new"), payload.to_string()).unwrap();
            std::fs::rename(dir.join("timeline.new"), dir.join("timeline.json")).unwrap();
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    listener.stop();
    // Teardown continues past a failed deregistration so the workers below are still joined; the
    // failure is printed because a registration left behind outlives this harness.
    if let Some(client)=&bridge.push_client {let mut desk=bridge.push.lock().unwrap();desk.clear().unwrap();if let Err(error)=desk.reconcile(client.as_ref(),None) {eprintln!("mac-server: push deregistration at teardown failed: {error}");}}

    for worker in bridge.workers.lock().unwrap().drain(..) {
        worker.join().unwrap();
    }
    std::fs::write(
        dir.join("timeline.json"),
        bridge.snapshot(None).unwrap().to_string(),
    )
    .unwrap();
}

fn session_within_lifetime(manual: bool, elapsed: Duration) -> bool {
    manual || elapsed < Duration::from_secs(3600)
}

#[test]
fn manual_session_has_no_deadline_but_automated_session_is_bounded() {
    assert!(session_within_lifetime(false, Duration::from_secs(3599)));
    assert!(!session_within_lifetime(false, Duration::from_secs(3600)));
    assert!(!session_within_lifetime(false, Duration::from_secs(86400)));
    assert!(session_within_lifetime(true, Duration::from_secs(3600)));
    assert!(session_within_lifetime(true, Duration::from_secs(86400)));
}
