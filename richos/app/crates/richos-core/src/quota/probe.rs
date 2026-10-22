//! Reusable control-only Claude process. It never receives a user message.
use super::{normalize, ReadError, Source, Window};
use crate::owned_process::OwnedChild;
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::JoinHandle,
    time::{Duration, Instant},
};

const DEADLINE: Duration = Duration::from_secs(20);
const MAX_FRAME: u64 = 256 * 1024;
const STDERR_BYTES: usize = 4096;

/// The one stop every account's reader shares. Each reader's latest process has its own
/// fence here, keyed by its folder (`None` is Account 1), so a stop reaches all of them: one
/// shared slot was overwritten by the next account's child, and quit then waited out
/// `DEADLINE` on Account 1's pending read (hunt part 1 v3, finding 51).
#[derive(Default)]
pub(super) struct Control {
    stopped: std::sync::atomic::AtomicBool,
    fences: std::sync::Mutex<std::collections::BTreeMap<Option<PathBuf>, crate::owned_process::ProcessFence>>,
}
impl Control {
    pub(super) fn stopped(&self) -> bool {
        self.stopped.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub(super) fn stop(&self) {
        self.stopped
            .store(true, std::sync::atomic::Ordering::SeqCst);
        for fence in self.fences.lock().unwrap().values() {
            fence.kill();
        }
    }
}

struct Connection {
    child: OwnedChild,
    stdin: ChildStdin,
    replies: Receiver<Value>,
    reader: Option<JoinHandle<()>>,
    stderr_reader: Option<JoinHandle<()>>,
    stderr: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<u8>>>,
    next: u64,
    problem: Option<String>,
}
impl Connection {
    fn start(bin: &Path, cwd: &Path, control: &Control, folder: Option<&Path>) -> Result<Self, (ReadError, String)> {
        if control.stopped() {
            return Err((ReadError::Failed, "usage check was stopped".into()));
        }
        let mut command = Command::new(bin);
        command
            .args([
                "--print",
                "--input-format=stream-json",
                "--output-format=stream-json",
                "--verbose",
                "--setting-sources",
                "",
                "--no-session-persistence",
                "--tools",
                "",
                "--strict-mcp-config",
                "--mcp-config",
                "{\"mcpServers\":{}}",
            ])
            .env_remove("CLAUDECODE");
        // An added account reads under its own Claude Code folder (fill-first).
        if let Some(folder) = folder {
            command.env("CLAUDE_CONFIG_DIR", folder);
        }
        command
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        OwnedChild::configure(&mut command);
        let mut raw = command.spawn().map_err(|e| (ReadError::Failed, format!("usage reader could not start: {e}")))?;
        let stdin = raw.stdin.take().unwrap();
        let stdout = raw.stdout.take().unwrap();
        let mut stderr_pipe = raw.stderr.take().unwrap();
        let child = OwnedChild::new(raw);
        control.fences.lock().unwrap().insert(folder.map(Path::to_path_buf), child.fence());
        if control.stopped() {
            return Err((ReadError::Failed, "usage check was stopped".into()));
        }
        // Drain all stderr while retaining only its last bounded bytes. A long line cannot
        // allocate unbounded memory and a full pipe cannot stop the provider from answering.
        let stderr = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
        let tail = stderr.clone();
        let stderr_reader = std::thread::spawn(move || {
            let mut bytes = [0; 1024];
            while let Ok(n) = stderr_pipe.read(&mut bytes) {
                if n == 0 { break; }
                let mut tail = tail.lock().unwrap();
                tail.extend(&bytes[..n]);
                while tail.len() > STDERR_BYTES { tail.pop_front(); }
            }
        });
        let (tx, replies) = mpsc::sync_channel(8);
        let reader = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut bytes = Vec::new();
                match reader
                    .by_ref()
                    .take(MAX_FRAME + 1)
                    .read_until(b'\n', &mut bytes)
                {
                    Ok(0) | Err(_) => break,
                    Ok(_) if bytes.len() as u64 > MAX_FRAME => break,
                    Ok(_) => {}
                }
                let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
                    break;
                };
                if value["type"] == "control_response" && tx.try_send(value).is_err() {
                    break;
                }
            }
        });
        let mut connection = Self {
            child,
            stdin,
            replies,
            reader: Some(reader),
            stderr_reader: Some(stderr_reader),
            stderr,
            next: 0,
            problem: None,
        };
        if let Err(error) = connection.request("initialize") {
            let detail = connection.failure("initialize");
            return Err((error, detail));
        }
        Ok(connection)
    }
    fn failure(&mut self, stage: &str) -> String {
        let exit = match self.child.try_wait() {
            Ok(Some(status)) => status.to_string(),
            Ok(None) => "still running when the request failed".into(),
            Err(error) => format!("exit status unavailable: {error}"),
        };
        self.reap();
        let bytes: Vec<u8> = self.stderr.lock().unwrap().iter().copied().collect();
        let said = String::from_utf8_lossy(&bytes);
        format!("{stage}: {exit}; {}; {}", self.problem.as_deref().unwrap_or("the provider did not complete the request"), if said.trim().is_empty() {
            "the usage reader wrote nothing on stderr".to_string()
        } else { format!("the usage reader said: {}", said.trim()) })
    }
    fn reap(&mut self) {
        let _best_effort = self.child.kill();
        let _best_effort = self.child.wait();
        if let Some(reader) = self.reader.take() { let _best_effort = reader.join(); }
        if let Some(reader) = self.stderr_reader.take() { let _best_effort = reader.join(); }
    }
    fn request(&mut self, subtype: &str) -> Result<Value, ReadError> {
        self.next += 1;
        let id = format!("quota-{}", self.next);
        let request = json!({"type":"control_request", "request_id":id, "request":{"subtype":subtype,"hooks":{}}});
        writeln!(self.stdin, "{request}")
            .and_then(|_| self.stdin.flush())
            .map_err(|error| { self.problem = Some(format!("could not write the request: {error}")); ReadError::Failed })?;
        let deadline = Instant::now() + DEADLINE;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let reply = self
                .replies
                .recv_timeout(left)
                .map_err(|error| { self.problem = Some(format!("could not read the reply: {error}")); ReadError::Failed })?;
            let response = &reply["response"];
            if response["request_id"] != id {
                continue;
            }
            if response["subtype"] != "success" {
                self.problem = Some(format!("provider refused the request: {}", response.to_string().chars().take(1024).collect::<String>()));
                return Err(ReadError::Failed);
            }
            return Ok(response["response"].clone());
        }
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.reap();
    }
}

#[derive(Default)]
pub struct ClaudeSource {
    connection: Option<Connection>,
    binary: PathBuf,
    control: std::sync::Arc<Control>,
    /// `None`: Account 1, the folder the app's own environment names (unchanged).
    folder: Option<PathBuf>,
    last_diagnostic: Option<String>,
}
impl ClaudeSource {
    pub(super) fn controlled(control: std::sync::Arc<Control>) -> Self {
        Self {
            control,
            ..Self::default()
        }
    }
    /// An added account's reader: the same control-only child, under that account's folder.
    pub(super) fn for_folder(control: std::sync::Arc<Control>, folder: PathBuf) -> Self {
        Self {
            control,
            folder: Some(folder),
            ..Self::default()
        }
    }
}
impl Source for ClaudeSource {
    fn read(&mut self, bin: &Path, cwd: &Path) -> Result<Vec<Window>, ReadError> {
        self.last_diagnostic = None;
        if self.binary != bin {
            self.connection = None;
            self.binary = bin.to_path_buf();
        }
        if self.connection.is_none() {
            self.connection = Some(match Connection::start(bin, cwd, &self.control, self.folder.as_deref()) {
                Ok(connection) => connection,
                Err((error, detail)) => { self.last_diagnostic = Some(detail); return Err(error); }
            });
        }
        let result = self
            .connection
            .as_mut()
            .unwrap()
            .request("get_usage")
            .and_then(|v| normalize(&v));
        if result.is_err() {
            if matches!(result, Err(ReadError::Failed)) {
                self.last_diagnostic = self.connection.as_mut().map(|c| c.failure("get_usage"));
            }
            self.connection = None;
        }
        // **NO LIMITS REPORTED IS NOT A VERDICT ON ITS OWN.** A signed-out Claude Code answers
        // `get_usage` exactly as an account without subscription limits does
        // (`rate_limits_available: false`; measured on 2.1.306, 2026-10-21), so the card said
        // "No reading yet" to a person who had never signed in and could not tell that from a
        // slow check (`docs/development/failure-classes/unknown-state-shown-as-pending.md`).
        // Claude Code's own `auth status` decides which one this is, and BOTH answers are kept:
        // a signed-in verdict stands on its own and is never lost because no usage reading
        // came with it (mid-job review rv-20261021T213648Z-b3f0990a-85f9: a signed-in account
        // without limits was offered "Sign in").
        if result == Err(ReadError::Unsupported) {
            use crate::provider_auth::AuthState;
            match crate::provider_auth::status_in(bin, self.folder.as_deref()).state {
                AuthState::SignedOut => return Err(ReadError::SignedOut),
                AuthState::Connected => return Err(ReadError::SignedInNoLimits),
                _ => {}
            }
        }
        result
    }
    fn diagnostic(&self) -> Option<&str> { self.last_diagnostic.as_deref() }
    fn disconnect(&mut self) {
        self.connection = None;
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use crate::quota::tests::Scratch;
    use std::os::unix::fs::PermissionsExt;
    fn script(root: &Path, body: &str) -> PathBuf {
        let path = root.join("claude-fixture");
        std::fs::write(&path, format!("#!/usr/bin/env python3\n{body}")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    #[test]
    fn usage_failures_keep_the_stage_and_bounded_stderr_then_clear_on_recovery() {
        for stage in ["initialize", "get_usage"] {
            let root = Scratch::new();
            let bin = script(root.path(), &format!(r#"import json, sys
from pathlib import Path
for line in sys.stdin:
    v = json.loads(line)
    kind = v['request']['subtype']
    if kind == '{stage}' and not Path('recovered').exists():
        sys.stderr.write('x' * 20000 + '\nfixture usage failure\n')
        sys.stderr.flush()
        sys.exit(19)
    payload = {{}} if kind == 'initialize' else {{'rate_limits_available': True, 'rate_limits': {{'five_hour': {{'utilization': 12}}}}}}
    print(json.dumps({{'type': 'control_response', 'response': {{'subtype': 'success', 'request_id': v['request_id'], 'response': payload}}}}), flush=True)
"#));
            let mut source = ClaudeSource::default();
            assert_eq!(source.read(&bin, root.path()), Err(ReadError::Failed));
            let detail = source.diagnostic().expect("the failed process must leave technical evidence");
            assert!(detail.contains(stage) && detail.contains("fixture usage failure"), "{detail}");
            assert!(detail.len() <= STDERR_BYTES + 512, "stderr retention must be bounded");
            std::fs::write(root.path().join("recovered"), "").unwrap();
            assert_eq!(source.read(&bin, root.path()).unwrap()[0].used_percent, 12.);
            assert_eq!(source.diagnostic(), None, "a successful read must clear the old diagnostic");
        }
    }

    #[test]
    fn a_missing_usage_reader_keeps_the_spawn_error_and_a_refusal_keeps_its_reason() {
        let root = Scratch::new();
        let mut source = ClaudeSource::default();
        assert_eq!(source.read(&root.path().join("missing"), root.path()), Err(ReadError::Failed));
        assert!(source.diagnostic().unwrap().contains("could not start"));
        let bin = script(root.path(), r#"import json, sys
for line in sys.stdin:
    v = json.loads(line)
    print(json.dumps({'type': 'control_response', 'response': {'subtype': 'error', 'request_id': v['request_id'], 'error': 'fixture permission refused'}}), flush=True)
"#);
        assert_eq!(source.read(&bin, root.path()), Err(ReadError::Failed));
        assert!(source.diagnostic().unwrap().contains("fixture permission refused"));
    }
    #[test]
    fn control_only_connection_is_reused_and_disconnect_reaps_the_process() {
        let root = Scratch::new();
        let bin = script(
            root.path(),
            r#"import sys, json, os
from pathlib import Path
Path('pid').write_text(str(os.getpid()))
assert '--no-session-persistence' in sys.argv
assert sys.argv[sys.argv.index('--tools')+1] == ''
assert '--strict-mcp-config' in sys.argv
for line in sys.stdin:
    v=json.loads(line)
    assert v['type'] == 'control_request'
    kind=v['request']['subtype']
    assert kind in ('initialize','get_usage')
    with open('requests','a') as log: log.write(kind+'\n')
    payload={} if kind=='initialize' else {'rate_limits_available':True,'rate_limits':{'five_hour':{'utilization':72,'resets_at':'2026-09-25T23:00:00Z'}}}
    print(json.dumps({'type':'control_response','response':{'subtype':'success','request_id':v['request_id'],'response':payload}}), flush=True)
"#,
        );
        let mut source = ClaudeSource::default();
        assert_eq!(source.read(&bin, root.path()).unwrap()[0].used_percent, 72.);
        source.read(&bin, root.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.path().join("requests")).unwrap(),
            "initialize\nget_usage\nget_usage\n"
        );
        let pid: i32 = std::fs::read_to_string(root.path().join("pid"))
            .unwrap()
            .parse()
            .unwrap();
        source.disconnect();
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    }
    /// **A signed-out account is told apart from one with no limits** (a new user's first run,
    /// 2026-10-21: the card said "No reading yet" to somebody who had never signed in). Both
    /// answer `get_usage` with `rate_limits_available: false`, exactly as Claude Code 2.1.306
    /// does signed out; only `claude auth status --json` separates them. And a signed-in verdict
    /// stands on its own: it is never lost because no usage reading came with it (mid-job
    /// review rv-20261021T213648Z-b3f0990a-85f9: the card offered "Sign in" to it).
    #[test]
    fn no_limits_from_a_signed_out_account_is_reported_as_signed_out() {
        for (logged_in, want) in [("False", ReadError::SignedOut), ("True", ReadError::SignedInNoLimits)] {
            let root = Scratch::new();
            let bin = script(
                root.path(),
                &format!(
                    r#"import sys, json
if sys.argv[1:4] == ['auth','status','--json']:
    print(json.dumps({{'loggedIn': {logged_in}}}))
    sys.exit(0 if {logged_in} else 1)
for line in sys.stdin:
    v=json.loads(line)
    kind=v['request']['subtype']
    payload={{}} if kind=='initialize' else {{'subscription_type':None,'rate_limits_available':False,'rate_limits':None}}
    print(json.dumps({{'type':'control_response','response':{{'subtype':'success','request_id':v['request_id'],'response':payload}}}}), flush=True)
"#
                ),
            );
            assert_eq!(ClaudeSource::default().read(&bin, root.path()), Err(want), "loggedIn {logged_in}");
        }
    }
    #[test]
    fn eof_and_oversized_frames_fail_without_a_hung_reader() {
        for body in ["pass", "print('x'*262145,flush=True)"] {
            let root = Scratch::new();
            let bin = script(root.path(), body);
            assert_eq!(
                ClaudeSource::default().read(&bin, root.path()),
                Err(ReadError::Failed)
            );
        }
    }
    #[test]
    fn shutdown_interrupts_a_control_request() {
        let root = Scratch::new();
        let bin = script(
            root.path(),
            "import time\nfrom pathlib import Path\nPath('started').touch()\ntime.sleep(60)",
        );
        let control = std::sync::Arc::new(Control::default());
        let mut source = ClaudeSource::controlled(control.clone());
        let cwd = root.path().to_path_buf();
        let reader = std::thread::spawn(move || source.read(&bin, &cwd));
        // A hang guard on a Python child starting, which under memory pressure alone has
        // taken seconds; it was 5 s (audit R9).
        let deadline = Instant::now() + Duration::from_secs(60);
        while !root.path().join("started").exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(root.path().join("started").exists());
        let stopped = Instant::now();
        control.stop();
        assert_eq!(reader.join().unwrap(), Err(ReadError::Failed));
        // load-bound: the only other way out of this read is the request's own DEADLINE
        // (20 s, above), which also ends in `Failed`, so the elapsed time is the one thing
        // that tells the stop from it. Half of DEADLINE is thousands of times what the stop
        // takes, so only a stall of ten seconds between two lines can fail it. The `< 2 s`
        // that stood here failed on a busy Mac; the audit's suggested 20 s would have equaled
        // DEADLINE and told nothing apart.
        assert!(stopped.elapsed() < DEADLINE / 2, "the stop did not interrupt the request; it ran to its deadline");
    }
}
