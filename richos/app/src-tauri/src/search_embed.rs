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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MODEL: &str = "bge-small-en-v1.5-q8_0.gguf";
const MODEL_SHA: &str = "ec38e8da142596baa913124ae50550de284b6916bf59577ef2f0cb9660c2f514";
const DIMENSION: usize = 384;
const CONTEXT: usize = 512;
const MAX_REPLY: u64 = 256 * 1024;
const REQUEST_BOUND: Duration = Duration::from_millis(500);
// The first launch of the newly built Metal helper took about eight seconds on
// the reference Mac. Markdown remains available while the helper starts.
const START_BOUND: Duration = Duration::from_secs(15);

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

pub struct ResidentEmbedder {
    // Fields drop in declaration order, so the child is reaped before its socket is removed.
    _child: Mutex<OwnedChild>,
    directory: SocketDirectory,
}

impl ResidentEmbedder {
    pub fn bundled() -> Result<Self, String> {
        #[cfg(debug_assertions)]
        if let Some(path) = std::env::var_os("RICHOS_SEARCH_RESOURCES") {
            return Self::start(Path::new(&path));
        }
        let executable = std::env::current_exe().map_err(|_| "The search model resources are unavailable.")?;
        let resources = executable.parent().and_then(Path::parent)
            .ok_or("The search model resources are unavailable.")?.join("Resources/RichFinder");
        Self::start(&resources)
    }

    fn start(resources: &Path) -> Result<Self, String> {
        let model = resources.join(MODEL);
        let server = resources.join("llama-server");
        let bytes = std::fs::read(&model).map_err(|_| "The bundled search model is unavailable.")?;
        if richos_search::corpus::content_hash(&bytes) != MODEL_SHA {
            return Err("The bundled search model failed its integrity check.".into());
        }
        drop(bytes);
        let directory = SocketDirectory::create()?;
        let socket = directory.socket();
        let mut command = Command::new(server);
        command.env_clear().arg("--model").arg(model).arg("--host").arg(&socket).args([
            "--embedding", "--parallel", "1", "-c", "512", "-b", "512", "-ub", "512",
            "--threads", "2", "--threads-batch", "2", "--no-webui", "--no-slots", "--log-disable",
        ]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        OwnedChild::configure(&mut command);
        let start = Instant::now();
        let mut child = OwnedChild::new(command.spawn().map_err(|_| "The local search model could not start.")?);
        while start.elapsed() < START_BOUND {
            if child.try_wait().map_err(|_| "The local search model could not start.")?.is_some() {
                return Err("The local search model exited during startup.".into());
            }
            if request(&socket, "GET", "/health", None).is_ok_and(|(status, _)| status == 200) {
                let runtime = Self { _child: Mutex::new(child), directory };
                runtime.one("search model readiness")?;
                eprintln!("[richos] document search model: ready in {} ms", start.elapsed().as_millis());
                return Ok(runtime);
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        Err("The local search model timed out during startup.".into())
    }

    fn one(&self, text: &str) -> Result<Vec<f32>, String> {
        // Bound even a pasted query before handing it to the child.
        let text: String = text.chars().take(8192).map(|c| if matches!(c, '\r' | '\n') { ' ' } else { c }).collect();
        let socket = self.directory.socket();
        let (status, first) = request(&socket, "POST", "/v1/embeddings", Some(json!({"input":text})))?;
        let data = if status == 200 {
            first
        } else if matches!(status, 400 | 500) {
            let (status, tokenized) = request(&socket, "POST", "/tokenize",
                Some(json!({"content":text, "add_special":true, "parse_special":false})))?;
            let mut tokens: Vec<u32> = serde_json::from_value(tokenized["tokens"].clone())
                .map_err(|_| "The local search tokenizer returned invalid tokens.")?;
            if status != 200 || tokens.len() <= CONTEXT {
                return Err("The local search model rejected the input.".into());
            }
            let last = tokens.pop().ok_or("The local search tokenizer returned no tokens.")?;
            tokens.truncate(CONTEXT - 1);
            tokens.push(last);
            let (status, data) = request(&socket, "POST", "/v1/embeddings", Some(json!({"input":tokens})))?;
            if status != 200 {
                return Err("The local search model rejected the bounded input.".into());
            }
            data
        } else {
            return Err("The local search model could not answer.".into());
        };
        let mut vector: Vec<f32> = serde_json::from_value(data["data"][0]["embedding"].clone())
            .map_err(|_| "The local search model returned an invalid vector.")?;
        if vector.len() != DIMENSION || vector.iter().any(|n| !n.is_finite()) || vector.iter().all(|n| *n == 0.0) {
            return Err("The local search model returned an invalid vector.".into());
        }
        richos_search::embed::normalize(&mut vector);
        Ok(vector)
    }
}

impl Embedder for ResidentEmbedder {
    fn identity(&self) -> String { format!("bge-small-en-v1.5:q8_0:{}:ctx512-specials-v1:{DIMENSION}", &MODEL_SHA[..12]) }
    fn dimension(&self) -> usize { DIMENSION }
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        texts.iter().map(|text| self.one(text)).collect()
    }
}

fn request(socket: &Path, method: &str, endpoint: &str, payload: Option<Value>) -> Result<(u16, Value), String> {
    let body = payload.map_or_else(Vec::new, |value| value.to_string().into_bytes());
    let unavailable = |_| "The local search model connection is unavailable.".to_string();
    let mut stream = UnixStream::connect(socket).map_err(unavailable)?;
    stream.set_read_timeout(Some(REQUEST_BOUND)).map_err(unavailable)?;
    stream.set_write_timeout(Some(REQUEST_BOUND)).map_err(unavailable)?;
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
    #[ignore = "requires the pinned RichFinder package resources"]
    fn bundled_search_model_keeps_a_private_socket_and_bounds_long_inputs() {
        assert!(std::env::var_os("RICHOS_SEARCH_RESOURCES").is_some());
        let model = ResidentEmbedder::bundled().unwrap();
        let socket = model.directory.socket();
        let vectors = model.embed(&[
            "how do I get my money back",
            "Refunds are approved within 30 days",
            "The hedgehog lives under a tree",
        ]).unwrap();
        let score = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x*y).sum::<f32>();
        assert!(score(&vectors[0], &vectors[1]) > score(&vectors[0], &vectors[2]));
        let long = "searchable metadata ".repeat(420);
        let bounded = model.one(&long).unwrap();
        assert_eq!(bounded.len(), DIMENSION);
        assert!((score(&bounded, &bounded) - 1.0).abs() < 1e-4);
        assert!(socket.exists());
        drop(model);
        assert!(!socket.exists());
    }
}
