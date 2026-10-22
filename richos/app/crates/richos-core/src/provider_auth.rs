//! Provider-owned browser login. Credentials and account details never enter app state.
use crate::owned_process::OwnedChild;
use serde::Serialize;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AuthState { Connected, SignedOut, Connecting, Cancelled, Unavailable, Failed, SameAccount }

#[derive(Clone, Debug, Serialize)]
pub struct AuthView { pub state: AuthState, pub message: &'static str }
impl AuthView {
    fn new(state: AuthState) -> Self {
        let message = match state {
            AuthState::Connected => "Your Anthropic account is connected.",
            AuthState::SignedOut => "Connect your Anthropic account to start working with Rich.",
            AuthState::Connecting => "Complete sign-in in your browser, then return here.",
            AuthState::Cancelled => "Sign-in was canceled. You can try again when you are ready.",
            AuthState::Unavailable => "Install Claude Code first, then connect your account.",
            AuthState::Failed => "Account connection could not be verified. Check your connection and try again.",
            AuthState::SameAccount => "That is the Claude account this Mac already uses. Sign in with your other one.",
        };
        Self { state, message }
    }
    /// The answer for an added account's sign-in that came back as an account already in use.
    pub fn same_account() -> Self { Self::new(AuthState::SameAccount) }
}

/// Read only the boolean needed for readiness. Never retain provider output,
/// account identifiers, subscription details or diagnostic stderr.
pub fn status(bin: &Path) -> AuthView {
    status_in(bin, None)
}

/// [`status`] for one Claude Code configuration folder (fill-first: an added account). `None`
/// is Account 1, the folder the app's own environment names.
pub fn status_in(bin: &Path, folder: Option<&Path>) -> AuthView {
    status_within(bin, folder, Duration::from_secs(15))
}

/// [`status_in`] with the caller's bound: a turn that already failed asks it for a few seconds
/// only (`native.rs`, the child that ended mid-turn).
pub fn status_within(bin: &Path, folder: Option<&Path>, within: Duration) -> AuthView {
    let mut command = Command::new(bin);
    command.args(["auth", "status", "--json"]).env_remove("CLAUDECODE")
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(folder) = folder { command.env("CLAUDE_CONFIG_DIR", folder); }
    OwnedChild::configure(&mut command);
    let Ok(mut raw) = command.spawn() else { return AuthView::new(AuthState::Unavailable); };
    let mut stdout = raw.stdout.take().unwrap();
    let mut child = OwnedChild::new(raw);
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        let result = stdout.by_ref().take(64 * 1024 + 1).read_to_end(&mut output);
        (result, output)
    });
    let deadline = Instant::now() + within;
    let exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => { let _ = child.kill(); let _ = child.wait(); break None; }
        }
    };
    let Ok((Ok(_), output)) = reader.join() else { return AuthView::new(AuthState::Failed); };
    if exit.is_none() || output.len() > 64 * 1024 { return AuthView::new(AuthState::Failed); }
    let answer = serde_json::from_slice::<serde_json::Value>(&output).ok()
        .and_then(|value| value.get("loggedIn").and_then(serde_json::Value::as_bool));
    AuthView::new(match answer { Some(true) if exit.is_some_and(|s| s.success()) => AuthState::Connected, Some(false) => AuthState::SignedOut, _ => AuthState::Failed })
}

/// **Is the account signed in under `a` the same Claude account as the one under `b`?** Round
/// 18's likeliest mistake for someone who is not technical: adding a second account and signing
/// in with the first one again. `claude auth status --json` names the account it is signed in
/// as (`email`, `orgId`; the plan's key list, richos-hq
/// `docs/plans/2026-10-04-multi-subscription-fill-first.md`). Both are compared HERE and
/// dropped: nothing is returned but the answer, and nothing is kept, so RichOS still holds no
/// account identifier (this file's rule, and the CEO's answer 4: rows show the user's label
/// only). `None` when either folder does not say who it is, which is never treated as the same.
pub fn same_account(bin: &Path, a: Option<&Path>, b: Option<&Path>) -> Option<bool> {
    let who = |folder: Option<&Path>| -> Option<(String, String)> {
        let value = status_json(bin, folder)?;
        let email = value.get("email")?.as_str()?.trim().to_lowercase();
        if email.is_empty() { return None; }
        let org = value.get("orgId").and_then(serde_json::Value::as_str).unwrap_or("").to_string();
        Some((email, org))
    };
    Some(who(a)? == who(b)?)
}

/// **Which email an account is signed in with**, for round 31's "Show email" on its card (the
/// CEO 2026-10-18: "an extra click there somehow to temporarily reveal which email is connected
/// to which account name"). Read when he clicks, from Claude Code's own `claude auth status
/// --json` under that account's folder, and handed to the window for those 8 seconds. Never
/// typed in, never kept: nothing here or in the caller stores it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum SignedInAs {
    /// The address Claude Code names for this sign-in.
    Email { email: String },
    /// Claude Code says this folder is not signed in.
    SignedOut,
    /// No answer that names an address (no binary, a timeout, an error, no `email`): the card
    /// says it could not be read, never a guess.
    Unknown,
}

/// [`SignedInAs`] for the folder `folder` (`None`: Account 1, the app's own environment).
pub fn signed_in_as(bin: &Path, folder: Option<&Path>) -> SignedInAs {
    let Some((finished_ok, value)) = status_answer(bin, folder) else { return SignedInAs::Unknown };
    match value.get("loggedIn").and_then(serde_json::Value::as_bool) {
        Some(false) => SignedInAs::SignedOut,
        Some(true) if finished_ok => match value.get("email").and_then(serde_json::Value::as_str).map(str::trim) {
            Some(email) if !email.is_empty() => SignedInAs::Email { email: email.to_string() },
            _ => SignedInAs::Unknown,
        },
        _ => SignedInAs::Unknown,
    }
}

/// `claude auth status --json` under `folder`, bounded like [`status_in`], read and parsed in
/// memory only. `None` for anything but a finished, successful, parseable answer.
fn status_json(bin: &Path, folder: Option<&Path>) -> Option<serde_json::Value> {
    status_answer(bin, folder).and_then(|(ok, value)| ok.then_some(value))
}

/// The parsed answer and whether the binary exited successfully. A signed-out folder may answer
/// `{"loggedIn":false}` with a failing exit, which [`signed_in_as`] still reads as signed out.
/// `None` when the binary did not run, did not finish in time, or said nothing parseable.
fn status_answer(bin: &Path, folder: Option<&Path>) -> Option<(bool, serde_json::Value)> {
    let mut command = Command::new(bin);
    command.args(["auth", "status", "--json"]).env_remove("CLAUDECODE")
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    if let Some(folder) = folder { command.env("CLAUDE_CONFIG_DIR", folder); }
    OwnedChild::configure(&mut command);
    let mut raw = command.spawn().ok()?;
    let mut stdout = raw.stdout.take()?;
    let mut child = OwnedChild::new(raw);
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        let result = stdout.by_ref().take(64 * 1024 + 1).read_to_end(&mut output);
        (result, output)
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => { drop(child.kill()); drop(child.wait()); break None; }
        }
    };
    let Ok((Ok(_), output)) = reader.join() else { return None };
    let exit = exit?;
    if output.len() > 64 * 1024 { return None; }
    serde_json::from_slice(&output).ok().map(|value| (exit.success(), value))
}

/// **Sign an added account's folder out** (fill-first's Remove): the stock
/// `claude auth logout` under that folder, bounded like [`status_in`]. RichOS never touches the
/// credential itself; the binary removes its own. Best effort: the folder is deleted next.
pub fn logout_in(bin: &Path, folder: &Path) {
    let mut command = Command::new(bin);
    command.args(["auth", "logout"]).env_remove("CLAUDECODE").env("CLAUDE_CONFIG_DIR", folder)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    OwnedChild::configure(&mut command);
    let Ok(raw) = command.spawn() else { return };
    let mut child = OwnedChild::new(raw);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => { drop(child.kill()); drop(child.wait()); return; }
        }
    }
}

pub struct ProviderAuth {
    login: Option<(OwnedChild, Instant)>,
    view: AuthView,
    /// `None`: Account 1 (unchanged). `Some`: an added account's folder (fill-first).
    folder: Option<std::path::PathBuf>,
}
impl Default for ProviderAuth {
    fn default() -> Self { Self { login: None, view: AuthView::new(AuthState::SignedOut), folder: None } }
}
impl ProviderAuth {
    /// The same sign-in, for an added account's own Claude Code folder.
    pub fn for_folder(folder: std::path::PathBuf) -> Self {
        Self { folder: Some(folder), ..Self::default() }
    }
    pub fn refresh(&mut self, bin: &Path) -> AuthView {
        if self.login.is_some() { return self.poll(bin); }
        self.view = status_in(bin, self.folder.as_deref());
        self.view.clone()
    }
    pub fn start(&mut self, bin: &Path) -> AuthView {
        if self.login.is_some() { return self.view.clone(); }
        let mut command = Command::new(bin);
        command.args(["auth", "login", "--claudeai"])
            .env_remove("CLAUDECODE").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        if let Some(folder) = &self.folder { command.env("CLAUDE_CONFIG_DIR", folder); }
        OwnedChild::configure(&mut command);
        match command.spawn() {
            Ok(child) => {
                self.login = Some((OwnedChild::new(child), Instant::now() + Duration::from_secs(600)));
                self.view = AuthView::new(AuthState::Connecting);
            }
            Err(_) => self.view = AuthView::new(AuthState::Unavailable),
        }
        self.view.clone()
    }
    pub fn poll(&mut self, bin: &Path) -> AuthView {
        let Some((child, deadline)) = &mut self.login else { return self.view.clone(); };
        let finished = match child.try_wait() {
            Ok(Some(_)) | Err(_) => true,
            Ok(None) => Instant::now() >= *deadline,
        };
        if finished {
            self.login = None; // Retire owned login processes before checking credentials.
            self.view = status_in(bin, self.folder.as_deref());
            if self.view.state == AuthState::SignedOut { self.view = AuthView::new(AuthState::Failed); }
        }
        self.view.clone()
    }
    pub fn cancel(&mut self) -> AuthView {
        self.login = None; // Does not sign out or remove provider-owned credentials.
        self.view = AuthView::new(AuthState::Cancelled);
        self.view.clone()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::quota::tests::Scratch;

    /// **Round 18's "That is the account you already use"**, decided from what Claude Code says
    /// each folder is signed in as. A fixture binary answers `auth status --json` from a file in
    /// each folder (Account 1 has no folder of its own: the fixture's `one`).
    #[test]
    fn the_same_claude_account_signed_in_twice_is_recognized_and_nothing_about_it_is_kept() {
        use std::os::unix::fs::PermissionsExt;
        let root = Scratch::new();
        let bin = root.path().join("claude-fixture");
        std::fs::write(&bin, format!("#!/bin/sh\nf=\"${{CLAUDE_CONFIG_DIR:-{}}}/status.json\"\n[ -f \"$f\" ] && cat \"$f\"\nexit 0\n",
            root.path().join("one").display())).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o700)).unwrap();
        let (one, work) = (root.path().join("one"), root.path().join("work"));
        for dir in [&one, &work] { std::fs::create_dir_all(dir).unwrap(); }
        let status = |dir: &Path, json: &str| std::fs::write(dir.join("status.json"), json).unwrap();
        status(&one, r#"{"loggedIn":true,"email":"Pat@Example.com","orgId":"org-home"}"#);
        status(&work, r#"{"loggedIn":true,"email":"pat@example.com","orgId":"org-home"}"#);
        assert_eq!(same_account(&bin, None, Some(&work)), Some(true), "the same address and organization, case aside");
        status(&work, r#"{"loggedIn":true,"email":"pat@example.com","orgId":"org-work"}"#);
        assert_eq!(same_account(&bin, None, Some(&work)), Some(false), "one person, two subscriptions: two accounts");
        status(&work, r#"{"loggedIn":true,"email":"sam@example.com","orgId":"org-home"}"#);
        assert_eq!(same_account(&bin, None, Some(&work)), Some(false));
        status(&work, r#"{"loggedIn":true}"#);
        assert_eq!(same_account(&bin, None, Some(&work)), None, "a folder that does not say who it is is never called the same");
    }

    /// **Round 31's "Show email": each card's email is the one that account's OWN sign-in
    /// names.** Fixture folders only: the fixture binary answers `auth status --json` from the
    /// `status.json` in the folder it is run under, and Account 1 (no folder of its own, the
    /// app's environment, whatever that is where this test runs) is answered from the fixture's
    /// `one` folder, so no real Claude Code folder is ever read.
    #[test]
    fn each_account_shows_the_email_its_own_sign_in_names_and_says_so_when_it_cannot() {
        use crate::claude_accounts::Accounts;
        use std::os::unix::fs::PermissionsExt;
        let root = Scratch::new();
        let data = root.path().join("data");
        let one = root.path().join("one");
        std::fs::create_dir_all(&one).unwrap();
        let accounts = Accounts::open(&data).unwrap();
        let work = accounts.add("Work").unwrap().folder.unwrap();
        let bin = root.path().join("claude-fixture");
        std::fs::write(&bin, format!(
            "#!/bin/sh\ncase \"${{CLAUDE_CONFIG_DIR:-}}\" in\n  {data}/*) f=\"$CLAUDE_CONFIG_DIR/status.json\" ;;\n  *) f=\"{one}/status.json\" ;;\nesac\n[ -f \"$f\" ] || exit 1\ncat \"$f\"\ngrep -q '\"loggedIn\":false' \"$f\" && exit 1\nexit 0\n",
            data = data.display(), one = one.display())).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o700)).unwrap();
        let status = |dir: &Path, json: &str| std::fs::write(dir.join("status.json"), json).unwrap();
        status(&one, r#"{"loggedIn":true,"email":"jordan.lee@example.com","orgId":"org-home"}"#);
        status(&work, r#"{"loggedIn":true,"email":"jordan@northwind.example","orgId":"org-work"}"#);
        let who = |id: &str| signed_in_as(&bin, accounts.sign_in_folder(id).expect("an account on this Mac").as_deref());
        assert_eq!(who("1"), SignedInAs::Email { email: "jordan.lee@example.com".into() }, "Account 1: the app's own sign-in");
        assert_eq!(who("2"), SignedInAs::Email { email: "jordan@northwind.example".into() }, "Work: its own folder's sign-in, not Account 1's");
        assert_eq!(accounts.sign_in_folder("9"), None, "an account that is not on this Mac is never answered for");
        // What it cannot read, it says, and never guesses.
        status(&work, r#"{"loggedIn":false}"#);
        assert_eq!(who("2"), SignedInAs::SignedOut, "signed out, even though the binary exits 1");
        status(&work, r#"{"loggedIn":true}"#);
        assert_eq!(who("2"), SignedInAs::Unknown, "signed in but no email named");
        std::fs::remove_file(work.join("status.json")).unwrap();
        assert_eq!(who("2"), SignedInAs::Unknown, "no answer at all");
        assert_eq!(signed_in_as(&root.path().join("no-such-claude"), Some(&work)), SignedInAs::Unknown, "no Claude Code");
        // The window reads it as tagged JSON.
        assert_eq!(serde_json::to_value(SignedInAs::Email { email: "a@b.example".into() }).unwrap(), serde_json::json!({ "state": "email", "email": "a@b.example" }));
        assert_eq!(serde_json::to_value(SignedInAs::SignedOut).unwrap(), serde_json::json!({ "state": "signedOut" }));
        assert_eq!(serde_json::to_value(SignedInAs::Unknown).unwrap(), serde_json::json!({ "state": "unknown" }));
    }
}
