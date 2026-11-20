//! Anthropic's internal reset interface. Secrets stay in memory and curl's stdin,
//! never command arguments, persisted snapshots, logs or model-visible responses.
use super::resets::{Account, Transport};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

const MAX_BYTES: u64 = 262_144;
const READ: &str = "/api/oauth/usage?cedar_ember=1&skip_spend=1";

pub struct System {
    token: String,
    version: String,
    organization: String,
    usage_windows: Option<Vec<super::Window>>,
    refusal: Option<super::shared_usage::Refusal>,
}
impl System {
    pub fn validate_environment() -> Result<(), String> {
        if [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_BASE_URL",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
            "CCR_OAUTH_TOKEN_FILE",
            "CLAUDE_CODE_CUSTOM_OAUTH_URL",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
            "CLAUDE_CODE_USE_FOUNDRY",
        ]
        .iter()
        .any(|key| std::env::var_os(key).is_some_and(|v| !v.is_empty()))
        {
            return Err("Reset offers need the saved Claude account connection, without authentication overrides.".into());
        }
        Ok(())
    }
    pub fn connect(bin: &Path) -> Result<Self, String> {
        Self::validate_environment()?;
        // Use the installed version, never a guessed newer client or a web identity.
        let bytes = bounded(Command::new(bin).arg("--version"), None)
            .map_err(|e| format!("Claude Code version check: {e}"))?;
        let text = String::from_utf8(bytes).map_err(|_| "Claude Code version unavailable.")?;
        let version = text.split_whitespace().next().unwrap_or("");
        if !text.contains("Claude Code")
            || version.len() > 32
            || version.split('.').count() != 3
            || !version.chars().all(|c| c.is_ascii_digit() || c == '.')
        {
            return Err("Reset offers require a supported Claude Code installation.".into());
        }
        let credentials = credentials()?;
        let token = credentials
            .pointer("/claudeAiOauth/accessToken")
            .and_then(Value::as_str)
            .filter(|s| {
                !s.is_empty()
                    && s.len() <= 8192
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
            })
            .ok_or("Connect your Claude account to check reset offers.")?
            .to_owned();
        Ok(Self {
            token,
            version: version.into(),
            organization: String::new(),
            usage_windows: None,
            refusal: None,
        })
    }

    pub(super) fn take_usage_windows(&mut self) -> Option<Vec<super::Window>> {
        self.usage_windows.take()
    }

    pub(super) fn take_refusal(&mut self) -> Option<super::shared_usage::Refusal> {
        self.refusal.take()
    }

    fn request(&mut self, path: &str, body: Option<Value>) -> Result<Value, String> {
        // Paths and the host are internal constants or validated UUIDs. No redirects,
        // retries or user curl configuration. Proxy settings follow the environment.
        let mut config = format!("url = \"https://api.anthropic.com{path}\"\nheader = \"Authorization: Bearer {}\"\nheader = \"anthropic-beta: oauth-2025-04-20\"\nheader = \"User-Agent: claude-cli/{} (external, cli)\"\nheader = \"Content-Type: application/json\"\n", self.token, self.version);
        if let Some(body) = body {
            let escaped = body.to_string().replace('\\', "\\\\").replace('"', "\\\"");
            config.push_str(&format!("request = \"POST\"\ndata = \"{escaped}\"\n"));
        }
        let mut command = Command::new("/usr/bin/curl");
        command.args([
            "-q",
            "--silent",
            "--include",
            "--write-out",
            "\n%{http_code}",
            "--proto",
            "=https",
            "--max-time",
            "15",
            "--connect-timeout",
            "5",
            "--max-filesize",
            "262144",
            "--config",
            "-",
        ]);
        let stage = if body_is_redemption(path) { "redemption" }
            else if path == "/api/oauth/profile" { "account check" } else { "eligibility check" };
        let bytes = bounded(&mut command, Some(config.into_bytes()))
            .map_err(|e| format!("Claude reset {stage}: {e}"))?;
        let (headers, body) = response_headers(&bytes);
        response(body).map_err(|error| {
            let at = crate::util::now_millis();
            let retry_at = retry_after(&headers, at);
            self.refusal = Some(super::shared_usage::Refusal {
                at, error: super::ReadError::Failed, explicit_refusal: false, headers, retry_at,
            });
            format!("Claude reset {stage}: {error}")
        })
    }
}

fn response_headers(mut bytes: &[u8]) -> (BTreeMap<String, String>, &[u8]) {
    let mut headers = BTreeMap::new();
    while bytes.starts_with(b"HTTP/") {
        let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") else { break };
        headers.clear();
        if let Ok(block) = std::str::from_utf8(&bytes[..end]) {
            for (name, value) in block.lines().skip(1).filter_map(|line| line.split_once(':')) {
                let name = name.to_ascii_lowercase();
                let value = value.trim();
                if value.len() <= 256 && value.bytes().all(|b| (b' '..=b'~').contains(&b))
                    && (name == "retry-after" || name.starts_with("anthropic-ratelimit-")
                        || name.starts_with("x-ratelimit-"))
                {
                    headers.insert(name, value.into());
                }
            }
        }
        bytes = &bytes[end + 4..];
    }
    (headers, bytes)
}

fn retry_after(headers: &BTreeMap<String, String>, at: u64) -> Option<u64> {
    let value = headers.get("retry-after")?;
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(at.saturating_add(seconds.saturating_mul(1000)));
    }
    let date = time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc2822).ok()?;
    u64::try_from(date.unix_timestamp()).ok().map(|seconds| seconds.saturating_mul(1000))
}
fn body_is_redemption(path: &str) -> bool { path.ends_with("/reset_rate_limits") }

// Only status and fixed descriptions leave this boundary. Provider error bodies can
// contain account data, so neither bodies nor child stderr appear in errors or logs.
fn response(bytes: &[u8]) -> Result<Value, String> {
    let split = bytes.iter().rposition(|b| *b == b'\n')
        .ok_or("HTTP status unavailable.")?;
    let status = std::str::from_utf8(&bytes[split + 1..]).ok()
        .filter(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|s| s.parse::<u16>().ok()).ok_or("HTTP status unavailable.")?;
    if !(200..300).contains(&status) {
        let reason = match status {
            401 | 403 => "account authorization was rejected",
            429 => "Anthropic rate-limited the check",
            500..=599 => "Anthropic returned a server error",
            _ => "Anthropic rejected the request",
        };
        return Err(format!("HTTP {status}: {reason}."));
    }
    serde_json::from_slice(&bytes[..split]).map_err(|_| "Anthropic returned an unreadable response.".into())
}

impl Transport for System {
    fn account(&mut self) -> Result<Account, String> {
        let profile = self.request("/api/oauth/profile", None)?;
        let account = profile
            .pointer("/account/uuid")
            .and_then(Value::as_str)
            .ok_or("Claude account identity unavailable.")?;
        let org = profile
            .pointer("/organization/uuid")
            .and_then(Value::as_str)
            .ok_or("Claude organization unavailable.")?;
        if uuid::Uuid::parse_str(account).is_err() || uuid::Uuid::parse_str(org).is_err() {
            return Err("Claude account identity is unreadable.".into());
        }
        self.organization = org.into();
        Ok(Account {
            key: format!("{:x}", Sha256::digest(format!("{account}:{org}"))),
        })
    }
    fn usage(&mut self) -> Result<Value, String> {
        let value = self.request(READ, None)?;
        self.usage_windows = super::normalize(&json!({
            "rate_limits_available": true, "rate_limits": value,
        })).ok();
        Ok(value)
    }
    fn redeem(&mut self, grant: &str, request: &str) -> Result<Value, String> {
        if uuid::Uuid::parse_str(&self.organization).is_err()
            || !super::resets::valid_id(grant)
            || uuid::Uuid::parse_str(request).is_err()
        {
            return Err("Reset request identity unavailable.".into());
        }
        self.request(
            &format!("/api/organizations/{}/reset_rate_limits", self.organization),
            Some(json!({"program":"cedar_ember", "grant_id":grant, "request_id":request})),
        )
    }
}

fn credentials() -> Result<Value, String> {
    let config = std::env::var_os("CLAUDE_SECURESTORAGE_CONFIG_DIR")
        .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR"));
    #[cfg(target_os = "macos")]
    {
        // Claude Code normalizes custom configuration paths to NFC. Refuse non-ASCII
        // here rather than selecting a different keychain item for the same spelling.
        let suffix = match config.as_ref().filter(|c| !c.is_empty()) {
            Some(path) => {
                let path = path
                    .to_str()
                    .filter(|s| s.is_ascii())
                    .ok_or("Custom Claude credential path is unsupported.")?;
                format!(
                    "-{}",
                    &format!("{:x}", Sha256::digest(path.as_bytes()))[..8]
                )
            }
            None => String::new(),
        };
        let user = std::env::var("USER").map_err(|_| "Claude credential account unavailable.")?;
        let bytes = bounded(
            Command::new("/usr/bin/security").args([
                "find-generic-password",
                "-a",
                &user,
                "-w",
                "-s",
                &format!("Claude Code-credentials{suffix}"),
            ]),
            None,
        ).map_err(|e| format!("Claude credential read: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|_| "Claude credentials are unreadable.".into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let root = config
            .filter(|c| !c.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| Path::new(&p).join(".claude")))
            .ok_or("Claude credential directory unavailable.")?;
        super::gate::read_json(&root.join(".credentials.json"))
            .map_err(|_| "Connect your Claude account to check reset offers.".into())
    }
}

/// Bound both output and wall time, including keychain prompts. Never include child
/// stderr or credentials in an error. The process group is reaped on every path.
fn bounded(command: &mut Command, input: Option<Vec<u8>>) -> Result<Vec<u8>, String> {
    use crate::owned_process::OwnedChild;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    OwnedChild::configure(command);
    let mut raw = command
        .spawn()
        .map_err(|_| "Could not start the Claude account reader.")?;
    let mut stdin = raw.stdin.take().unwrap();
    let stdout = raw.stdout.take().unwrap();
    let mut child = OwnedChild::new(raw);
    let (tx, rx) = std::sync::mpsc::channel();
    let writer = std::thread::spawn(move || {
        if let Some(input) = input {
            let _best_effort = stdin.write_all(&input);
        }
    });
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _best_effort = tx.send(result);
    });
    let received = rx.recv_timeout(Duration::from_secs(20));
    if received
        .as_ref()
        .ok()
        .and_then(|r| r.as_ref().ok())
        .is_none_or(|b| b.len() as u64 > MAX_BYTES)
    {
        let _best_effort = child.kill();
        let _best_effort = child.wait();
        let _best_effort = writer.join();
        let _best_effort = reader.join();
        return Err("Could not read reset offers. Try refreshing later.".into());
    }
    // EOF need not mean exit. Wait with a deadline rather than blocking on wait().
    let until = std::time::Instant::now() + Duration::from_secs(2);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if std::time::Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => break None,
        }
    };
    let _best_effort = child.kill();
    let _best_effort = child.wait();
    let _best_effort = writer.join();
    let _best_effort = reader.join();
    if !status.is_some_and(|s| s.success()) {
        return Err(match status.and_then(|s| s.code()) {
            Some(code) => format!("reader exited with code {code}."),
            None => "reader ended without an exit status.".into(),
        });
    }
    Ok(received.unwrap().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refusal_keeps_rate_headers_and_understands_both_retry_after_formats() {
        let raw = b"HTTP/1.1 200 Connection established\r\n\r\nHTTP/2 429\r\nRetry-After: 700\r\nAnthropic-RateLimit-Requests-Remaining: 0\r\nSet-Cookie: private\r\n\r\nprivate body\n429";
        let (mut headers, body) = response_headers(raw);
        assert_eq!(headers.len(), 2);
        assert_eq!(headers["anthropic-ratelimit-requests-remaining"], "0");
        assert_eq!(retry_after(&headers, 1_000), Some(701_000));
        assert!(response(body).unwrap_err().contains("HTTP 429"));
        headers.insert("retry-after".into(), "Fri, 20 Nov 2026 13:00:00 GMT".into());
        assert_eq!(retry_after(&headers, 1_000), Some(1_795_179_600_000));
    }
    #[test]
    fn http_status_identifies_auth_rate_limit_and_server_errors_without_leaking_bodies() {
        for status in [401, 403, 429, 503] {
            let error = response(format!("private account data\n{status}").as_bytes()).unwrap_err();
            assert!(error.contains(&status.to_string()));
            assert!(!error.contains("private"));
        }
        assert_eq!(response(b"{\"ok\":true}\n200").unwrap(), json!({"ok":true}));
        assert!(response(b"private malformed response\n200").unwrap_err().contains("unreadable"));
        assert!(response(b"secret\nnot a status").unwrap_err().contains("status unavailable"));
    }
    #[test]
    fn bounded_reader_keeps_stderr_private_and_rejects_failed_or_oversized_output() {
        assert_eq!(
            bounded(Command::new("/bin/sh").args(["-c", "printf ok"]), None).unwrap(),
            b"ok"
        );
        let error = bounded(
            Command::new("/bin/sh").args(["-c", "echo credential >&2; exit 1"]),
            None,
        )
        .unwrap_err();
        assert!(!error.contains("credential"));
        assert!(bounded(
            Command::new("/bin/sh").args(["-c", "dd if=/dev/zero bs=1024 count=257 2>/dev/null"]),
            None
        )
        .is_err());
    }
    #[test]
    fn invalid_installation_never_reads_credentials_or_contacts_anthropic() {
        assert!(System::connect(Path::new("/usr/bin/false")).is_err());
    }
}
