//! **PRIVATE DETAILS OUT, BEFORE ANYTHING SEES THE TEXT** (PRD §6), and the last mechanical
//! check on the exact outgoing bytes.
//!
//! Data passes four boundaries (PRD §6): local read-only extraction, then THIS deterministic
//! filtering and scrubbing before any evidence reaches Claude, then the person's review with
//! another mechanical check after every AI or human edit ([`check_outgoing`]), then the
//! approved, immutable payload.
//!
//! [`scrub`] replaces, in this order: PEM private-key blocks; the values of `Authorization`,
//! `Cookie`, `Set-Cookie`, `X-Api-Key` and `Bearer`; secret-shaped tokens (Anthropic, OpenAI,
//! GitHub, Slack, AWS, JSON Web Tokens) and the value of any `token`/`secret`/`password`/
//! `api_key`-style key; secret query parameters in web addresses; email addresses; absolute
//! personal paths (anything under `/Users/<name>/` but `/Users/Shared`, and `~/…`); and the
//! private terms RichDoctor can know without loading the RichOS engine ([`Context`]: the account
//! name, the computer's name, and the person's and company's names from RichOS's own
//! `config.json`). Each becomes a stand-in the person sees: `[secret removed]`, `[email removed]`,
//! `[path removed]`, `[name removed]`. A model may suggest more removals; it cannot undo these.
//!
//! It is a help, not a guarantee (PRD §6): "a scanner cannot reliably know every private name
//! or fact", which is why the person reviews the exact text.
use crate::evidence::Removed;
use std::path::Path;

pub const SECRET: &str = "[secret removed]";
pub const CREDENTIAL: &str = "[credential removed]";
pub const EMAIL: &str = "[email removed]";
pub const PATH: &str = "[path removed]";
pub const NAME: &str = "[name removed]";
pub const KEY_BLOCK: &str = "[private key removed]";

/// The one address a report may name.
pub const SUPPORT_ADDRESS: &str = "support@richos.ceo";

/// The private words known on this Mac without the RichOS engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    /// The account's short name (`/Users/<name>`).
    pub user: String,
    /// The home folder itself, wherever it is: any word inside it is a personal path.
    pub home: String,
    /// Names to remove wherever they appear, longest first.
    pub terms: Vec<String>,
}

impl Context {
    /// Gather the context for the account whose home is `home`. RichOS's `config.json` is read
    /// for its two name fields only, bounded, and a missing or unreadable file is no terms.
    pub fn gather(home: &Path) -> Context {
        let user = home.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let mut terms = Vec::new();
        let config = home.join("Library/Application Support").join(crate::RICHOS_IDENTIFIER).join("config.json");
        if let Ok(read) = crate::util::bounded_tail(&config, 256 * 1024, std::time::Duration::from_secs(2)) {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&read.bytes) {
                for key in ["user_name", "company_name"] {
                    if let Some(s) = v[key].as_str() {
                        terms.push(s.trim().to_string());
                        // A full name's parts, each long enough not to be an ordinary word.
                        terms.extend(s.split_whitespace().filter(|w| w.chars().count() >= 3).map(str::to_string));
                    }
                }
            }
        }
        if let Some(host) = hostname() {
            terms.push(host.trim_end_matches(".local").to_string());
        }
        if user.chars().count() >= 3 {
            terms.push(user.clone());
        }
        let mut ctx = Context::with(user, terms);
        ctx.home = home.to_string_lossy().trim_end_matches('/').to_string();
        ctx
    }

    pub fn with(user: String, terms: Vec<String>) -> Context {
        let mut terms: Vec<String> = terms.into_iter().map(|t| t.trim().to_string()).filter(|t| t.chars().count() >= 2).collect();
        terms.sort_by_key(|t| std::cmp::Reverse(t.len()));
        terms.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        Context { user, home: String::new(), terms }
    }
}

fn hostname() -> Option<String> {
    let mut buf = [0u8; 256];
    // SAFETY: gethostname writes at most buf.len() bytes into buf.
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if rc != 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let s = String::from_utf8_lossy(&buf[..end]).trim().to_string();
    (!s.is_empty()).then_some(s)
}

const SECRET_KEYS: [&str; 16] = [
    "token", "secret", "password", "passwd", "pwd", "api_key", "apikey", "api-key", "access_token", "refresh_token", "client_secret", "private_key",
    "session", "sessionid", "auth", "signature",
];

const HEADER_KEYS: [&str; 6] = ["authorization:", "proxy-authorization:", "cookie:", "set-cookie:", "x-api-key:", "anthropic-api-key:"];

fn is_secret_key(key: &str) -> bool {
    let k = key.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-').to_ascii_lowercase();
    !k.is_empty() && SECRET_KEYS.iter().any(|s| k == *s || k.ends_with(&format!("_{s}")) || k.ends_with(&format!("-{s}")))
}

/// A token that looks like a credential by its shape alone.
pub fn secret_shaped(word: &str) -> bool {
    let w = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '.');
    let rest = |p: &str| w.len() > p.len() + 8 && w.starts_with(p);
    let alnum = |s: &str| s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ["sk-ant-", "sk-proj-", "ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_", "xoxb-", "xoxp-", "xoxa-", "xoxr-", "xoxs-", "glpat-"].iter().any(|p| rest(p)) {
        return true;
    }
    if w.len() >= 20 && w.starts_with("sk-") && alnum(&w[3..]) {
        return true;
    }
    if w.len() == 20 && (w.starts_with("AKIA") || w.starts_with("ASIA")) && w[4..].chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        return true;
    }
    // A JSON Web Token: three base64url parts, the first two starting with `eyJ`.
    let parts: Vec<&str> = w.split('.').collect();
    parts.len() == 3 && parts[0].starts_with("eyJ") && parts[1].starts_with("eyJ") && parts.iter().all(|p| p.len() >= 4 && alnum(p))
}

fn is_email(word: &str) -> Option<(usize, usize)> {
    let at = word.find('@')?;
    let local_ok = |c: char| c.is_alphanumeric() || "._%+-".contains(c);
    let host_ok = |c: char| c.is_alphanumeric() || ".-".contains(c);
    let start = word[..at].char_indices().rev().take_while(|&(_, c)| local_ok(c)).last().map(|(i, _)| i)?;
    let after = &word[at + 1..];
    let mut end = at + 1 + after.find(|c: char| !host_ok(c)).unwrap_or(after.len());
    while end > at + 1 && word[..end].ends_with(['.', '-']) {
        end -= 1;
    }
    let domain = &word[at + 1..end];
    let dotted = domain.rfind('.').is_some_and(|d| d > 0 && d + 1 < domain.len());
    (start < at && dotted).then_some((start, end))
}

fn is_personal_path(word: &str) -> bool {
    if word.starts_with("~/") || word == "~" {
        return true;
    }
    match word.find("/Users/") {
        Some(i) => {
            let rest = &word[i + 7..];
            let name = rest.split('/').next().unwrap_or("");
            !name.is_empty() && name != "Shared" && !name.starts_with('[') && !name.starts_with("\\[")
        }
        None => false,
    }
}

/// A web address with its secret query values replaced.
fn scrub_url(word: &str, removed: &mut Removed) -> String {
    let Some(q) = word.find('?') else { return word.to_string() };
    let (head, query) = word.split_at(q + 1);
    let parts: Vec<String> = query
        .split('&')
        .map(|kv| match kv.split_once('=') {
            Some((k, v)) if is_secret_key(k) || ["code", "sig", "key", "access_token"].contains(&k.to_ascii_lowercase().as_str()) || secret_shaped(v) => {
                removed.secrets += 1;
                format!("{k}={SECRET}")
            }
            _ => kv.to_string(),
        })
        .collect();
    format!("{head}{}", parts.join("&"))
}

/// Remove PEM private-key blocks.
fn scrub_key_blocks(text: &str, removed: &mut Removed) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(b) = rest.find("-----BEGIN") {
        let header_end = rest[b..].find('\n').map_or(rest.len(), |i| b + i);
        if !rest[b..header_end].contains("PRIVATE KEY") {
            out.push_str(&rest[..header_end]);
            rest = &rest[header_end..];
            continue;
        }
        out.push_str(&rest[..b]);
        out.push_str(KEY_BLOCK);
        removed.secrets += 1;
        rest = match rest[b..].find("-----END") {
            Some(e) => {
                let after = &rest[b + e + 8..];
                after.find("-----").map_or("", |i| &after[i + 5..])
            }
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// Remove header values line by line.
fn scrub_headers(text: &str, removed: &mut Removed) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if let Some((i, key)) = HEADER_KEYS.iter().filter_map(|k| lower.find(k).map(|i| (i, *k))).min_by_key(|(i, _)| *i) {
                removed.secrets += 1;
                let nl = if line.ends_with('\n') { "\n" } else { "" };
                return format!("{} {CREDENTIAL}{nl}", &line[..i + key.len()]);
            }
            if let Some(i) = lower.find("bearer ") {
                removed.secrets += 1;
                let nl = if line.ends_with('\n') { "\n" } else { "" };
                return format!("{}Bearer {CREDENTIAL}{nl}", &line[..i]);
            }
            line.to_string()
        })
        .collect()
}

/// Whole-word, any-case matches of `term` in `text`, as byte ranges.
fn find_term(text: &str, term: &str) -> Vec<(usize, usize)> {
    let lower = text.to_lowercase();
    let t = term.to_lowercase();
    if lower.len() != text.len() || t.is_empty() {
        // Case folding changed byte lengths (rare scripts): fall back to an exact match.
        return text.match_indices(term).map(|(i, m)| (i, i + m.len())).filter(|&(s, e)| boundary(text, s, e)).collect();
    }
    lower.match_indices(&t).map(|(i, m)| (i, i + m.len())).filter(|&(s, e)| boundary(text, s, e)).collect()
}

fn boundary(text: &str, s: usize, e: usize) -> bool {
    let before = text[..s].chars().next_back();
    let after = text[e..].chars().next();
    !before.is_some_and(|c| c.is_alphanumeric()) && !after.is_some_and(|c| c.is_alphanumeric())
}

/// **SCRUB ONE TEXT.** Deterministic: the same text and context give the same result.
pub fn scrub(text: &str, ctx: &Context) -> (String, Removed) {
    let mut removed = Removed::default();
    let text = scrub_key_blocks(text, &mut removed);
    let text = scrub_headers(&text, &mut removed);
    // Word by word, keeping every separator as it was.
    let mut out = String::with_capacity(text.len());
    let mut previous_key = false;
    for piece in split_keep(&text) {
        if piece.chars().all(char::is_whitespace) {
            out.push_str(piece);
            continue;
        }
        let lead = piece.len() - piece.trim_start_matches(['"', '\'', '`', '(', '[', '{', '<']).len();
        let trail_trimmed = piece.trim_end_matches(['"', '\'', '`', ')', ']', '}', '>', ',', ';', '.', ':', '!', '?']);
        let (open, core, close) = (&piece[..lead], &piece[lead..trail_trimmed.len().max(lead)], &piece[trail_trimmed.len().max(lead)..]);
        let replaced = scrub_word(core, previous_key, &ctx.home, &mut removed);
        previous_key = is_secret_key(core) && (core.ends_with(':') || close.contains(':') || core.ends_with('='));
        out.push_str(open);
        out.push_str(&replaced);
        out.push_str(close);
    }
    // Private terms last, over the whole text: a name can be several words.
    let mut text = out;
    for term in &ctx.terms {
        let spans = find_term(&text, term);
        if spans.is_empty() {
            continue;
        }
        let mut next = String::with_capacity(text.len());
        let mut at = 0;
        for (s, e) in spans {
            if s < at {
                continue;
            }
            next.push_str(&text[at..s]);
            next.push_str(NAME);
            removed.names += 1;
            at = e;
        }
        next.push_str(&text[at..]);
        text = next;
    }
    (text, removed)
}

fn scrub_word(core: &str, previous_key: bool, home: &str, removed: &mut Removed) -> String {
    if core.is_empty() {
        return String::new();
    }
    if previous_key && !core.starts_with('[') {
        removed.secrets += 1;
        return SECRET.to_string();
    }
    if secret_shaped(core) {
        removed.secrets += 1;
        return SECRET.to_string();
    }
    // key=value and key:value with a secret key.
    for sep in ['=', ':'] {
        if let Some((k, v)) = core.split_once(sep) {
            if is_secret_key(k) && !v.is_empty() && !v.starts_with("//") {
                removed.secrets += 1;
                return format!("{k}{sep}{SECRET}");
            }
        }
    }
    if is_personal_path(core) || (home.len() > 1 && core.contains(home)) {
        removed.paths += 1;
        return PATH.to_string();
    }
    let core = if core.contains("://") { scrub_url(core, removed) } else { core.to_string() };
    if let Some((s, e)) = is_email(&core) {
        if !core[s..e].eq_ignore_ascii_case(SUPPORT_ADDRESS) {
            removed.emails += 1;
            return format!("{}{EMAIL}{}", &core[..s], &core[e..]);
        }
    }
    core
}

/// `text` as alternating runs of non-space and space, every byte kept.
fn split_keep(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_space = None;
    for (i, c) in text.char_indices() {
        let space = c.is_whitespace();
        if in_space.is_some_and(|s| s != space) {
            out.push(&text[start..i]);
            start = i;
        }
        in_space = Some(space);
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

// ---------------------------------------------------------------------------------------
// THE LAST MECHANICAL CHECK, ON THE EXACT OUTGOING BYTES
// ---------------------------------------------------------------------------------------

/// The final title cap (PRD §5).
pub const TITLE_MAX_CHARS: usize = 200;
/// The final body cap, footer and escaping included (PRD §5).
pub const BODY_MAX_BYTES: usize = 60_000;

/// One thing the last check found, with where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// `"title"` or `"line N"` (1-based), as the relay names it.
    pub item: String,
    pub detail: String,
}

/// **THE LAST CHECK** (PRD §6 boundary 3), run after every AI or human edit and again on the
/// frozen payload at Send. It mirrors the reporting relay's own content rules (agreed with the
/// relay owner, `cc/zach-opus-reportrelay1` `richos/mobile/service/RICHDOCTOR-RELAY.md`), so a report this
/// passes is one the relay accepts on content: no secret-shaped text, no email address but
/// support's, no `/Users/<name>/` path, no unescaped `<`, `[` or `]`, every `@` followed by a
/// zero-width space, no control characters, the size limits. It never changes the text; the
/// person sees what it found and removes it.
pub fn check_outgoing(title: &str, body: &str, receipt_secret: Option<&str>) -> Vec<Problem> {
    let mut out = Vec::new();
    if title.chars().count() > TITLE_MAX_CHARS {
        out.push(Problem { item: "title".into(), detail: format!("The title is longer than {TITLE_MAX_CHARS} characters.") });
    }
    if title.trim().is_empty() {
        out.push(Problem { item: "title".into(), detail: "The report needs a title.".into() });
    }
    if title.chars().any(char::is_control) {
        out.push(Problem { item: "title".into(), detail: "The title contains a control character.".into() });
    }
    if body.len() > BODY_MAX_BYTES {
        out.push(Problem { item: "body".into(), detail: "The report is longer than the size limit.".into() });
    }
    for (place, text) in std::iter::once(("title".to_string(), title)).chain(body.split('\n').enumerate().map(|(i, l)| (format!("line {}", i + 1), l))) {
        let unbroken = text.replace('\u{200B}', "");
        let mut found = |detail: &str| out.push(Problem { item: place.clone(), detail: detail.into() });
        if place != "title" && text.chars().any(|c| c.is_control() && c != '\t') {
            found("This line contains a control character.");
        }
        if let Some(secret) = receipt_secret {
            if !secret.is_empty() && text.contains(secret) {
                found("This line contains the report's private receipt.");
            }
        }
        for word in unbroken.split(|c: char| c.is_whitespace() || "()\"'`,;<>".contains(c)) {
            let core = word.trim_matches(|c: char| ".:!?\\[]{}*_~".contains(c));
            if secret_shaped(core) {
                found("This line looks like it contains a password or access key.");
            }
            if let Some((s, e)) = is_email(core) {
                if !core[s..e].eq_ignore_ascii_case(SUPPORT_ADDRESS) {
                    found("This line contains an email address.");
                }
            }
            if is_personal_path(core) && core.contains("/Users/") {
                found("This line contains a file path that names a person's folder.");
            }
        }
        let lower = unbroken.to_ascii_lowercase();
        let header_value = HEADER_KEYS.iter().any(|k| lower.find(k).is_some_and(|i| !lower[i + k.len()..].trim_start().starts_with("\\[credential")));
        let bearer = lower.contains("bearer ") && !lower.contains("bearer \\[credential");
        if header_value || bearer {
            found("This line looks like it contains a login credential.");
        }
        if text.contains("-----BEGIN") && text.contains("PRIVATE KEY") {
            found("This line contains a private key.");
        }
        let chars: Vec<char> = text.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            if matches!(c, '<' | '[' | ']') {
                let slashes = chars[..i].iter().rev().take_while(|&&p| p == '\\').count();
                if slashes % 2 == 0 {
                    found("This line has a mark that GitHub would read as a link or as formatting.");
                    break;
                }
            }
        }
        if chars.iter().enumerate().any(|(i, &c)| c == '@' && chars.get(i + 1) != Some(&'\u{200B}')) {
            found("This line would mention someone on GitHub.");
        }
    }
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        Context::with("jdoe".into(), vec!["Jane Doe".into(), "Jane".into(), "SecretCo".into(), "Janes-MacBook-Pro".into(), "jdoe".into()])
    }

    /// Fixture credentials, assembled at run time so no credential-shaped literal sits in the
    /// source (the repository's secret scanner refuses one, rightly).
    fn fake(kind: &str) -> String {
        let tail = "abcdefghijklmnopqrstuvwxyz0123456789";
        match kind {
            "github" => format!("gh{}_{tail}", "p"),
            "anthropic" => format!("sk-{}-api03-{tail}", "ant"),
            "pem" => format!("-----BEGIN OPENSSH {} KEY-----\nAAAA\n-----END OPENSSH {} KEY-----", "PRIVATE", "PRIVATE"),
            "value" => ["q1w2", "e3r4", "t5y6"].concat(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn secrets_paths_emails_and_names_are_replaced_and_counted() {
        let raw = format!(
            "executable: /Users/jdoe/Applications/RichOS.app/Contents/MacOS/richos-tauri\n\
             failure: auth failed for jane@example.com with {}={} at https://api.example.com/v1?key={}&page=2\n\
             Authorization: Bearer {}\n\
             Jane Doe of SecretCo on Janes-MacBook-Pro wrote {}\n\
             kept: /Applications/RichOS.app and /Users/Shared/x and support@richos.ceo",
            "token",
            fake("value"),
            fake("value"),
            fake("anthropic"),
            fake("github")
        );
        let (out, removed) = scrub(&raw, &ctx());
        assert!(!out.contains("jdoe") && !out.contains("jane@") && !out.contains(&fake("value")), "{out}");
        assert!(!out.contains("sk-ant") && !out.contains("ghp_") && !out.contains("Jane") && !out.contains("SecretCo") && !out.contains("MacBook"), "{out}");
        assert!(out.contains("executable: [path removed]"), "{out}");
        assert!(out.contains("page=2"), "an ordinary query value stays: {out}");
        assert!(out.contains("/Applications/RichOS.app") && out.contains("/Users/Shared/x") && out.contains("support@richos.ceo"), "{out}");
        assert_eq!((removed.paths, removed.emails), (1, 1));
        assert!(removed.secrets >= 3 && removed.names >= 3, "{removed:?}");
    }

    #[test]
    fn a_json_secret_value_and_a_key_block_are_removed() {
        let raw = format!("{{\"api_key\": \"{}\", \"status\": 529}}\n{}\nafter", fake("value"), fake("pem"));
        let (out, _) = scrub(&raw, &Context::default());
        assert!(!out.contains(&fake("value")) && !out.contains("AAAA"), "{out}");
        assert!(out.contains("529") && out.contains("after") && out.contains(KEY_BLOCK), "{out}");
    }

    #[test]
    fn meaningful_error_text_survives() {
        let line = "2026-10-22T08:43:11Z exit signal=SIGABRT status 529 overloaded_error: Overloaded (request took 30.2 s)";
        assert_eq!(scrub(line, &ctx()).0, line);
    }

    #[test]
    fn scrubbing_is_deterministic() {
        let raw = "Jane at /Users/jdoe/x token: abc";
        assert_eq!(scrub(raw, &ctx()), scrub(raw, &ctx()));
        assert!(!scrub(raw, &ctx()).0.contains("abc"));
    }

    #[test]
    fn the_last_check_passes_scrubbed_escaped_text_and_names_every_leak_by_line() {
        let (clean, _) = scrub("failure at /Users/jdoe/x for jane@example.com", &ctx());
        let body = format!("{}\n\nSent with RichDoctor 1.0 · Report ID RD-7K3M-Q9VX", richos_core::bug_report::escaped_lines(&clean));
        assert_eq!(check_outgoing("RichOS closes at start", &body, None), vec![]);
        let leaks = format!("line one\nmail me at jane@example.com\n/Users/jdoe/Desktop\n[a link](https://x)\nhi @octocat\n{}\nAuthorization: Basic Zm9v", fake("github"));
        let found = check_outgoing("ok", &leaks, None);
        let lines: Vec<&str> = found.iter().map(|p| p.item.as_str()).collect();
        for want in ["line 2", "line 3", "line 4", "line 5", "line 6", "line 7"] {
            assert!(lines.contains(&want), "{want} in {found:?}");
        }
        assert!(!lines.contains(&"line 1"));
        assert!(!check_outgoing("ok", "my receipt is SeCrEt-123", Some("SeCrEt-123")).is_empty());
        assert!(!check_outgoing(&"x".repeat(201), "", None).is_empty());
        assert!(!check_outgoing("ok", &"x".repeat(BODY_MAX_BYTES + 1), None).is_empty());
    }
}
