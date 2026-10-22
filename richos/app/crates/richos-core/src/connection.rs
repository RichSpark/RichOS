//! **A worker's turn that a lost internet connection ended is carried on once the connection is
//! back** (CEO, 2026-10-21: "Of course, that must be built into the app. What else is the app
//! good for if it can't handle that?").
//!
//! ## What Claude Code does, read and captured rather than assumed (2.1.305)
//!
//! A request that cannot connect is retried by Claude Code itself, base 500 ms, cap 32 s, for
//! `CLAUDE_CODE_MAX_RETRIES` attempts, and then the turn ENDS. On the stream-json wire that end
//! is an assistant frame marked `is_api_error_message: true`, `error: "server_error"`, whose
//! text is the vendor's line (`API Error: Connection refused — a firewall or proxy may be
//! blocking it (ECONNREFUSED)`), then a `result` with `terminal_reason: "api_error"`,
//! `is_error: true` and `api_error_status: null`. A `403` such as the incident's Cloudflare
//! "Error 1034: Edge IP Restricted" page is never retried at all: the same two frames, with
//! `error: "authentication_failed"` and `api_error_status: 403`. Both shapes were captured
//! from the 2.1.305 binary against a localhost port (one refusing, one answering the
//! incident's `403` body); nothing left the machine.
//!
//! ## What "the connection is back" means
//!
//! Claude Code's own connectivity preflight, adopted as it is: `GET <base>/api/hello` answering
//! `200` within 10 s, where `<base>` is `ANTHROPIC_BASE_URL` or `https://api.anthropic.com`
//! (2.1.305: `` `${u.BASE_API_URL}/api/hello` `` with `p.status!==200` a failure and a 1e4 ms
//! timeout). A Cloudflare edge answering `403` for the API host is therefore "not back", which
//! is exactly the incident. `curl` honors the same proxy variables Claude Code does.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Checks the connection to the model API and waits for it to come back.
pub struct ConnectionWatch {
    probe: Box<dyn Fn() -> bool + Send + Sync>,
    every: Duration,
}

impl ConnectionWatch {
    /// How often a wait asks again. The probe itself is bounded at 10 s, so the turn carries on
    /// at most about 20 s after the connection returns.
    pub const EVERY: Duration = Duration::from_secs(10);

    /// The real check: Claude Code's own `/api/hello` preflight, through `curl`.
    pub fn system() -> ConnectionWatch {
        ConnectionWatch::new(api_answers, ConnectionWatch::EVERY)
    }

    /// A watch on any probe, asked every `every` while waiting. Tests use it in place of the network.
    pub fn new(probe: impl Fn() -> bool + Send + Sync + 'static, every: Duration) -> ConnectionWatch {
        ConnectionWatch { probe: Box::new(probe), every }
    }

    /// Does the model API answer now?
    pub fn is_up(&self) -> bool {
        (self.probe)()
    }

    /// Wait until the API answers (`true`) or `stopped` says the wait is over (`false`). Asks
    /// `stopped` every 200 ms between probes, so a Stop or a quit is honored promptly.
    pub fn wait_until_up(&self, stopped: &dyn Fn() -> bool) -> bool {
        loop {
            if stopped() {
                return false;
            }
            if self.is_up() {
                return true;
            }
            let end = Instant::now() + self.every;
            loop {
                if stopped() {
                    return false;
                }
                let now = Instant::now();
                if now >= end {
                    break;
                }
                std::thread::sleep((end - now).min(Duration::from_millis(200)));
            }
        }
    }
}

/// **Did the lost connection end this turn?** Read off the two frames Claude Code ends an API
/// failure with: `kind` is the assistant frame's `error`; the HTTP status is the `result`'s
/// `api_error_status`, or the one its line names (Claude Code ends repeated overloads with
/// `API Error: Repeated 529 Overloaded errors.`, a `server_error` with no status field).
///
/// - No HTTP answer at all and Claude Code calls it a `server_error`: its connection failure
///   (refused, reset, unreachable, timed out), after its own retries ran out. Always the
///   connection, whatever the API answers now; [`ConnectionWatch::wait_until_up`] then decides
///   how long to wait.
/// - An HTTP answer (the incident's `403`, an overload): the connection only when the API does
///   not answer now. With the connection up it is a real answer from the API and is left to the
///   handling it always had, the overload schedule included.
pub fn lost(result: &serde_json::Value, kind: Option<&str>, watch: &ConnectionWatch) -> bool {
    let said = result.get("result").and_then(serde_json::Value::as_str).unwrap_or_default();
    let status = result.get("api_error_status").and_then(serde_json::Value::as_u64)
        .or_else(|| crate::upstream::UpstreamFailure::classify_lines(said).and_then(|failure| failure.status).map(u64::from));
    match status {
        None => kind == Some("server_error"),
        Some(_) => !watch.is_up(),
    }
}

fn api_answers() -> bool {
    let base = std::env::var("ANTHROPIC_BASE_URL")
        .ok()
        .filter(|base| !base.trim().is_empty())
        .unwrap_or_else(|| "https://api.anthropic.com".to_string());
    let url = format!("{}/api/hello", base.trim().trim_end_matches('/'));
    Command::new("curl")
        .args(["-sS", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "10", &url])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|out| out.stdout == b"200")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A connection failure is the connection whatever the API says now; an HTTP answer is the
    /// connection only while the API does not answer, Claude Code's own repeated-overload line
    /// included; a `max_output_tokens` end is neither.
    #[test]
    fn which_api_failures_are_the_lost_connection() {
        let up = ConnectionWatch::new(|| true, Duration::ZERO);
        let down = ConnectionWatch::new(|| false, Duration::ZERO);
        let ended = |status: Option<u64>, said: &str| serde_json::json!({"terminal_reason":"api_error","api_error_status":status,"result":said});
        let refused = ended(None, "API Error: Connection refused (ECONNREFUSED)");
        let edge = ended(Some(403), "API Error: 403 Error 1034: Edge IP Restricted");
        let overloads = ended(None, "API Error: Repeated 529 Overloaded errors.");
        assert!(lost(&refused, Some("server_error"), &up));
        assert!(lost(&edge, Some("authentication_failed"), &down));
        assert!(!lost(&edge, Some("authentication_failed"), &up));
        assert!(!lost(&overloads, Some("server_error"), &up), "an overload keeps its own schedule");
        assert!(!lost(&ended(None, "API Error: output token maximum"), Some("max_output_tokens"), &up));
    }

    /// The wait asks again until the API answers, and a stop ends it without another probe.
    #[test]
    fn a_wait_ends_when_the_api_answers_or_when_it_is_stopped() {
        let asked = Arc::new(AtomicUsize::new(0));
        let counter = asked.clone();
        let watch = ConnectionWatch::new(move || counter.fetch_add(1, Ordering::SeqCst) >= 2, Duration::from_millis(5));
        assert!(watch.wait_until_up(&|| false));
        assert_eq!(asked.load(Ordering::SeqCst), 3, "down twice, then up");
        assert!(!watch.wait_until_up(&|| true));
        assert_eq!(asked.load(Ordering::SeqCst), 3, "a stopped wait asks nothing");
    }
}
