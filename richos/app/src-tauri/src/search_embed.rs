//! App-owned embedding model. The child accepts requests through one private
//! Unix socket and runs only while a configured folder needs meaning search.
use richos_core::owned_process::OwnedChild;
use richos_search::embed::Embedder;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MODEL: &str = "bge-small-en-v1.5-q8_0.gguf";
const MODEL_SHA: &str = "ec38e8da142596baa913124ae50550de284b6916bf59577ef2f0cb9660c2f514";
const DIMENSION: usize = 384;
const CONTEXT: usize = 512;
const MAX_REPLY: u64 = 256 * 1024;
const REQUEST_BOUND: Duration = Duration::from_millis(500);
const INDEX_REQUEST_BOUND: Duration = Duration::from_secs(8);
// A fresh test guest can exceed the old 15-second startup bound. The search
// worker starts the helper while Markdown search remains available.
const START_BOUND: Duration = Duration::from_secs(45);

struct SocketDirectory(PathBuf);

impl SocketDirectory {
    fn create() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!("rf-{:x}-{now:x}-{sequence:x}", std::process::id()));
        if directory.join("m.sock").as_os_str().as_encoded_bytes().len() >= 104 {
            return Err("The search model socket path is too long.".into());
        }
        std::fs::DirBuilder::new().mode(0o700).create(&directory)
            .map_err(|_| "The search model could not create its private socket directory.")?;
        Ok(Self(directory))
    }

    fn socket(&self) -> PathBuf { self.0.join("m.sock") }
}

impl Drop for SocketDirectory {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("[richos] document search model: socket cleanup failed");
            }
        }
    }
}

pub struct ResidentEmbedder(Arc<ModelProcess>);

struct ModelProcess {
    resources: PathBuf,
    running: Mutex<Option<Running>>,
    repairing: AtomicBool,
    closed: AtomicBool,
}

struct Running {
    // Drop the process before removing its socket directory.
    _child: OwnedChild,
    directory: SocketDirectory,
}

enum LookupError {
    Retry(String),
    Input(String),
}

impl From<String> for LookupError {
    fn from(message: String) -> Self { Self::Retry(message) }
}

pub(crate) fn resources() -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("RICHOS_SEARCH_RESOURCES") {
        return Ok(PathBuf::from(path));
    }
    let executable = std::env::current_exe().map_err(|_| "The search model resources are unavailable.")?;
    Ok(executable.parent().and_then(Path::parent)
        .ok_or("The search model resources are unavailable.")?.join("Resources/RichFinder"))
}

impl ResidentEmbedder {
    pub fn bundled() -> Result<Self, String> {
        let resources = resources()?;
        if !resources.join("llama-server").is_file() {
            return Err("The local search model could not start.".into());
        }
        let model = resources.join(MODEL);
        let bytes = std::fs::read(&model).map_err(|_| "The bundled search model is unavailable.")?;
        if richos_search::corpus::content_hash(&bytes) != MODEL_SHA {
            return Err("The bundled search model failed its integrity check.".into());
        }
        drop(bytes);
        Ok(Self(Arc::new(ModelProcess {
            resources,
            running: Mutex::new(None),
            repairing: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        })))
    }

    fn index_one(&self, text: &str, stop: &dyn Fn() -> bool) -> Result<Vec<f32>, String> {
        let text = bounded_text(text);
        loop {
            self.0.prepare(stop)?;
            let mut guard = self.0.running.lock().map_err(|_| "The local search model is unavailable.")?;
            if stop() {
                return Err("The local search model is unavailable.".into());
            }
            let Some(runtime) = guard.as_mut() else { continue };
            // A cold CPU-only embedding can take much longer than a warm query.
            // Give the background request one chance to complete before restarting.
            match runtime.one(&text, INDEX_REQUEST_BOUND) {
                Ok(vector) => return Ok(vector),
                Err(LookupError::Input(reason)) => return Err(reason),
                Err(LookupError::Retry(_)) => {}
            }
            eprintln!("[richos] document search model: restarting after a failed background request");
            drop(guard.take());
            drop(guard);
            retry_pause(stop);
        }
    }

    fn query_one(&self, text: &str) -> Result<Vec<f32>, String> {
        let text = bounded_text(text);
        let mut guard = self.0.running.try_lock().map_err(|_| "The local search model is busy.")?;
        if let Some(runtime) = guard.as_ref() {
            match runtime.one(&text, REQUEST_BOUND) {
                Ok(vector) => return Ok(vector),
                Err(LookupError::Input(reason)) => return Err(reason),
                Err(LookupError::Retry(reason)) => {
                    let retired = guard.take();
                    drop(guard);
                    self.0.repair(retired);
                    return Err(reason);
                }
            }
        }
        drop(guard);
        self.0.repair(None);
        Err("The local search model is busy.".into())
    }
}

impl Drop for ResidentEmbedder {
    fn drop(&mut self) {
        self.0.closed.store(true, Ordering::Release);
    }
}

impl ModelProcess {
    fn prepare(&self, stop: &dyn Fn() -> bool) -> Result<(), String> {
        let stop = || self.closed.load(Ordering::Acquire) || stop();
        loop {
            if stop() {
                return Err("The local search model is unavailable.".into());
            }
            let mut running = self.running.lock().map_err(|_| "The local search model is unavailable.")?;
            if running.is_some() {
                return Ok(());
            }
            match Running::start(&self.resources, &stop) {
                Ok(runtime) => {
                    *running = Some(runtime);
                    return Ok(());
                }
                Err(_) if stop() => return Err("The local search model is unavailable.".into()),
                Err(reason) => eprintln!("[richos] document search model: retrying startup ({reason})"),
            }
            drop(running);
            retry_pause(&stop);
        }
    }

    fn repair(self: &Arc<Self>, retired: Option<Running>) {
        if self.repairing.swap(true, Ordering::AcqRel) {
            return;
        }
        let model = self.clone();
        if std::thread::Builder::new().name("richos-search-model-repair".into()).spawn(move || {
            drop(retired);
            let _result = model.prepare(&|| false);
            model.repairing.store(false, Ordering::Release);
        }).is_err() {
            self.repairing.store(false, Ordering::Release);
        }
    }
}

fn retry_pause(stop: &dyn Fn() -> bool) {
    for _ in 0..10 {
        if stop() { break; }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn bounded_text(text: &str) -> String {
    text.chars().take(8192).map(|c| if matches!(c, '\r' | '\n') { ' ' } else { c }).collect()
}

fn running_in_virtual_machine() -> bool {
    let mut present: libc::c_int = 0;
    let mut bytes = std::mem::size_of_val(&present);
    let status = unsafe {
        libc::sysctlbyname(
            c"kern.hv_vmm_present".as_ptr(),
            (&mut present as *mut libc::c_int).cast(),
            &mut bytes,
            std::ptr::null_mut(),
            0,
        )
    };
    status == 0 && bytes == std::mem::size_of_val(&present) && present != 0
}

impl Running {
    fn start(resources: &Path, stop: &dyn Fn() -> bool) -> Result<Self, String> {
        if stop() {
            return Err("The local search model could not start.".into());
        }
        let model = resources.join(MODEL);
        let server = resources.join("llama-server");
        let directory = SocketDirectory::create()?;
        let socket = directory.socket();
        let mut command = Command::new(server);
        command.env_clear().env("RICHFINDER_OWNER_SOCKET", &socket).arg("--model").arg(model).arg("--host").arg(&socket).args([
            "--embedding", "--parallel", "1", "-c", "512", "-b", "512", "-ub", "512",
            "--threads", "2", "--threads-batch", "2", "--no-webui", "--no-slots", "--log-disable",
        ]);
        if running_in_virtual_machine() {
            // On Apple's paravirtual GPU, the small embedding model is faster on CPU.
            command.args(["--device", "none", "--gpu-layers", "0"]);
        }
        command.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
        OwnedChild::configure(&mut command);
        let start = Instant::now();
        let mut child = OwnedChild::new(command.spawn().map_err(|_| "The local search model could not start.")?);
        while !stop() && start.elapsed() < START_BOUND {
            if child.try_wait().map_err(|_| "The local search model could not start.")?.is_some() {
                return Err("The local search model exited during startup.".into());
            }
            if request(&socket, "GET", "/health", None, REQUEST_BOUND).is_ok_and(|(status, _)| status == 200) {
                eprintln!("[richos] document search model: ready in {} ms", start.elapsed().as_millis());
                return Ok(Self { _child: child, directory });
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        Err("The local search model timed out during startup.".into())
    }

    fn one(&self, text: &str, bound: Duration) -> Result<Vec<f32>, LookupError> {
        let socket = self.directory.socket();
        let (status, first) = request(&socket, "POST", "/v1/embeddings", Some(json!({"input":text})), bound)?;
        let data = if status == 200 {
            first
        } else if matches!(status, 400 | 500) {
            let rejection = status;
            let (status, tokenized) = request(&socket, "POST", "/tokenize",
                Some(json!({"content":text, "add_special":true, "parse_special":false})), bound)?;
            let mut tokens: Vec<u32> = serde_json::from_value(tokenized["tokens"].clone())
                .map_err(|_| "The local search tokenizer returned invalid tokens.".to_string())?;
            if status != 200 {
                return Err(LookupError::Retry("The local search model could not answer.".into()));
            }
            if tokens.len() <= CONTEXT {
                return Err(rejected(rejection, "The local search model rejected the input."));
            }
            let last = tokens.pop().ok_or_else(|| "The local search tokenizer returned no tokens.".to_string())?;
            tokens.truncate(CONTEXT - 1);
            tokens.push(last);
            let (status, data) = request(&socket, "POST", "/v1/embeddings", Some(json!({"input":tokens})), bound)?;
            if status != 200 {
                return Err(rejected(status, "The local search model rejected the bounded input."));
            }
            data
        } else {
            return Err(LookupError::Retry("The local search model could not answer.".into()));
        };
        let mut vector: Vec<f32> = serde_json::from_value(data["data"][0]["embedding"].clone())
            .map_err(|_| "The local search model returned an invalid vector.".to_string())?;
        if vector.len() != DIMENSION || vector.iter().any(|n| !n.is_finite()) || vector.iter().all(|n| *n == 0.0) {
            return Err(LookupError::Retry("The local search model returned an invalid vector.".into()));
        }
        richos_search::embed::normalize(&mut vector);
        Ok(vector)
    }
}

fn rejected(status: u16, input: &str) -> LookupError {
    if status == 400 {
        LookupError::Input(input.into())
    } else {
        LookupError::Retry("The local search model could not answer.".into())
    }
}

impl Embedder for ResidentEmbedder {
    fn identity(&self) -> String { format!("bge-small-en-v1.5:q8_0:{}:ctx512-specials-v1:{DIMENSION}", &MODEL_SHA[..12]) }
    fn dimension(&self) -> usize { DIMENSION }
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        self.embed_for_index(texts, &|| false)
    }

    fn prepare(&self, stop: &dyn Fn() -> bool) -> Result<(), String> {
        self.0.prepare(stop)
    }

    fn embed_for_index(&self, texts: &[&str], stop: &dyn Fn() -> bool) -> Result<Vec<Vec<f32>>, String> {
        texts.iter().map(|text| self.index_one(text, stop)).collect()
    }

    fn embed_query(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        texts.iter().map(|text| self.query_one(text)).collect()
    }
}

fn request(socket: &Path, method: &str, endpoint: &str, payload: Option<Value>, bound: Duration) -> Result<(u16, Value), String> {
    let body = payload.map_or_else(Vec::new, |value| value.to_string().into_bytes());
    let unavailable = |_| "The local search model connection is unavailable.".to_string();
    let mut stream = UnixStream::connect(socket).map_err(unavailable)?;
    stream.set_read_timeout(Some(bound)).map_err(unavailable)?;
    stream.set_write_timeout(Some(bound)).map_err(unavailable)?;
    write!(stream, "{method} {endpoint} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", body.len())
        .map_err(unavailable)?;
    stream.write_all(&body).map_err(unavailable)?;
    let mut reply = Vec::new();
    stream.take(MAX_REPLY + 1).read_to_end(&mut reply).map_err(unavailable)?;
    if reply.len() as u64 > MAX_REPLY {
        return Err("The local search model response exceeded its limit.".into());
    }
    let boundary = reply.windows(4).position(|v| v == b"\r\n\r\n").ok_or("The local search model response was incomplete.")?;
    let header = std::str::from_utf8(&reply[..boundary]).map_err(|_| "The local search model response was invalid.")?;
    let status = header.lines().next().and_then(|line| line.strip_prefix("HTTP/1.1 "))
        .and_then(|line| line.split_once(' ')).and_then(|(code, _)| code.parse().ok())
        .ok_or("The local search model response status was invalid.")?;
    let body = serde_json::from_slice(&reply[boundary + 4..]).map_err(|_| "The local search model response was invalid.")?;
    Ok((status, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_errors_retry_and_foreground_queries_do_not_wait_on_the_model_lock() {
        assert!(matches!(rejected(500, "input error"), LookupError::Retry(_)));
        assert!(matches!(rejected(400, "input error"), LookupError::Input(_)));
        let model = ResidentEmbedder(Arc::new(ModelProcess {
            resources: PathBuf::new(),
            running: Mutex::new(None),
            repairing: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        }));
        let guard = model.0.running.lock().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                send.send(model.embed_query(&["refund"])).unwrap();
            });
            // load-bound: hang guard for a query that must not wait for this lock.
            let result = receive.recv_timeout(Duration::from_secs(10));
            drop(guard);
            assert!(result.unwrap().is_err());
        });
    }

    #[test]
    #[ignore = "requires the pinned RichFinder package resources"]
    fn bundled_search_model_keeps_a_private_socket_and_bounds_long_inputs() {
        assert!(std::env::var_os("RICHOS_SEARCH_RESOURCES").is_some());
        let model = ResidentEmbedder::bundled().unwrap();
        model.prepare(&|| false).unwrap();
        let socket = model.0.running.lock().unwrap().as_ref().unwrap().directory.socket();
        let vectors = model.embed(&[
            "how do I get my money back",
            "Refunds are approved within 30 days",
            "The hedgehog lives under a tree",
        ]).unwrap();
        let score = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x*y).sum::<f32>();
        assert!(score(&vectors[0], &vectors[1]) > score(&vectors[0], &vectors[2]));
        let long = "searchable metadata ".repeat(420);
        let bounded = model.index_one(&long, &|| false).unwrap();
        assert_eq!(bounded.len(), DIMENSION);
        assert!((score(&bounded, &bounded) - 1.0).abs() < 1e-4);
        assert!(socket.exists());

        model.0.running.lock().unwrap().as_mut().unwrap()._child.kill().unwrap();
        let recovered = model.index_one("how do I get my money back", &|| false).unwrap();
        assert!(score(&vectors[0], &recovered) > 0.99);
        let replacement = model.0.running.lock().unwrap().as_ref().unwrap().directory.socket();
        assert_ne!(socket, replacement);
        assert!(!socket.exists());
        drop(model);
        assert!(!replacement.exists());
    }
}
