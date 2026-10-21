//! Read-only offer polling and a separately authorized, one-use weekly reset action.
//! Neither polling nor approval redeems anything. The prepared desktop action or
//! orchestrator tool can consume approval after fresh account, grant and usage checks.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const WEEKLY_THRESHOLD: f64 = 99.;
const FILE: &str = "claude-reset-offers.json";
const MAX_ATTEMPTS: usize = 128;

pub struct Account {
    pub key: String,
}
pub trait Transport {
    fn account(&mut self) -> Result<Account, String>;
    fn usage(&mut self) -> Result<Value, String>;
    fn redeem(&mut self, grant: &str, request: &str) -> Result<Value, String>;
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub id: String,
    pub label: String,
    pub remaining: u32,
    pub starts_at: u64,
    pub expires_at: u64,
    pub clears: Vec<String>,
    pub usable_now: bool,
    pub requires_limit: bool,
}
impl Offer {
    fn weekly(&self, now: u64) -> bool {
        self.remaining > 0
            && self.starts_at <= now
            && self.expires_at > now
            && self.clears.iter().any(|s| s == "seven_day")
    }
    fn same_grant(&self, other: &Self) -> bool {
        self.id == other.id
            && self.starts_at == other.starts_at
            && self.expires_at == other.expires_at
            && self.clears == other.clears
            && self.remaining == other.remaining
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: String,
    pub offer: Offer,
    pub approved_at: u64,
    pub weekly_threshold: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attempt {
    pub grant_id: String,
    pub request_id: String,
    pub at: u64,
    /// `uncertain` is durably written BEFORE the POST. A crash is never a fresh try.
    pub outcome: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    account: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub state: String,
    pub checked_at: Option<u64>,
    pub retry_at: Option<u64>,
    pub message: Option<String>,
    pub offers: Vec<Offer>,
    pub approval: Option<Approval>,
    pub last_attempt: Option<Attempt>,
    pub weekly_threshold: u8,
    #[serde(default)]
    pub weekly_used: Option<f64>,
    #[serde(default)]
    pub weekly_resets_at: Option<u64>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            state: "unknown".into(),
            checked_at: None,
            retry_at: None,
            message: Some("Reset availability has not been checked.".into()),
            offers: vec![],
            approval: None,
            last_attempt: None,
            weekly_threshold: WEEKLY_THRESHOLD as u8,
            weekly_used: None,
            weekly_resets_at: None,
        }
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    account: String,
    #[serde(default)]
    view: View,
    #[serde(default)]
    attempts: Vec<Attempt>,
    // A legacy nightly is fenced before its approval enters the shared ledger.
    // Older readers deny unknown fields, so they cannot redeem that approval too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    migration: Option<(String, PathBuf)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    imports: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    poll_after: Option<u64>,
}
struct Reading {
    offers: Vec<Offer>,
    eligible: bool,
    reason: Option<String>,
    next: Option<String>,
    weekly_used: Option<f64>,
    weekly_reset: Option<u64>,
    cooldown: Option<u64>,
}
pub(super) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
}
fn parse(body: &Value) -> Result<Reading, String> {
    let b = body
        .get("cedar_ember")
        .filter(|v| v.is_object())
        .ok_or("Reset availability is unknown; Anthropic did not evaluate the offer.")?;
    let eligible = b
        .get("eligible")
        .and_then(Value::as_bool)
        .ok_or("Reset eligibility is unreadable.")?;
    let reason = b.get("ineligible_reason").and_then(Value::as_str).map(|s| match s {
        "surface" | "cli_version" => "This offer is not available through this Claude Code connection. Check Claude’s Usage page.",
        "no_grant" => "No reset offer is available for this account.",
        _ => "Anthropic is not offering a reset through this connection.",
    }.to_string());
    let grants = b
        .get("grants")
        .and_then(Value::as_array)
        .ok_or("Reset offers are unreadable.")?;
    if grants.len() > 64 {
        return Err("Too many reset offers to read safely.".into());
    }
    let mut offers = Vec::new();
    for g in grants {
        let id = g
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| valid_id(s))
            .ok_or("Reset offer identity is unreadable.")?;
        let starts = g
            .get("starts_at")
            .and_then(super::reset)
            .ok_or("Reset start time is unreadable.")?;
        let expires = g
            .get("ends_at")
            .and_then(super::reset)
            .filter(|t| *t > starts)
            .ok_or("Reset expiry is unreadable.")?;
        let clears = g
            .get("clears")
            .and_then(Value::as_array)
            .ok_or("Reset limits are unreadable.")?;
        let mut limits: Vec<String> = clears
            .iter()
            .map(|v| {
                v.as_str()
                    .filter(|s| valid_id(s))
                    .map(String::from)
                    .ok_or_else(|| "Reset limits are unreadable.".to_string())
            })
            .collect::<Result<_, _>>()?;
        if limits.is_empty() || limits.len() > 32 {
            return Err("Reset limits are unreadable.".into());
        }
        limits.sort();
        limits.dedup();
        let remaining = g
            .get("resets_left")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or("Reset count is unreadable.")?;
        let paused = g
            .get("paused")
            .and_then(Value::as_bool)
            .ok_or("Reset availability is unreadable.")?;
        let usable = g
            .get("usable_now")
            .and_then(Value::as_bool)
            .ok_or("Reset availability is unreadable.")?;
        let requires = g
            .get("use_requires_limit")
            .and_then(Value::as_bool)
            .ok_or("Reset conditions are unreadable.")?;
        let blocking = g
            .get("blocking")
            .and_then(Value::as_array)
            .ok_or("Reset conditions are unreadable.")?;
        let label: String = g
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or("Claude usage-limit reset")
            .chars()
            .filter(|c| !c.is_control())
            .take(160)
            .collect();
        if offers.iter().any(|o: &Offer| o.id == id) {
            return Err("Duplicate reset offer identity.".into());
        }
        offers.push(Offer {
            id: id.into(),
            label,
            remaining,
            starts_at: starts,
            expires_at: expires,
            clears: limits,
            usable_now: usable && !paused && blocking.is_empty(),
            requires_limit: requires,
        });
    }
    let weekly_used = body
        .pointer("/seven_day/utilization")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && (0. ..=100.).contains(n));
    Ok(Reading {
        offers,
        eligible,
        reason,
        next: b
            .get("next_grant_id")
            .and_then(Value::as_str)
            .map(String::from),
        weekly_used,
        weekly_reset: body.pointer("/seven_day/resets_at").and_then(super::reset),
        cooldown: b.get("cooldown_until").and_then(super::reset),
    })
}

struct RecordLock(fs::File);
impl Drop for RecordLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // Closing alone can leave flock held by a concurrently spawned child's
            // inherited descriptor until exec. End our critical section explicitly.
            let _best_effort = unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
        }
    }
}

pub struct Service {
    root: PathBuf,
}
impl Service {
    pub fn new(root: &Path) -> Self {
        Self { root: root.into() }
    }
    pub fn marker(&self) -> Option<String> {
        fs::read_to_string(self.root.join("claude-reset-refresh.json")).ok()
    }
    /// Transfer an old isolated approval without ever leaving two redeemable copies.
    /// The import receipt makes recovery idempotent after a crash or a later revoke.
    pub fn import_legacy(&self, root: &Path) -> Result<(), String> {
        if root == self.root || !root.join(FILE).exists() { return Ok(()); }
        let legacy = Self::new(root);
        let _old_lock = legacy.lock()?;
        let _shared_lock = self.lock()?;
        let mut old = legacy.read_record()?;
        let mut shared = self.read()?;
        if old.migration.as_ref().is_some_and(|(_, target)| target != &self.root) {
            return Err("The old reset record names a different shared store.".into());
        }
        let (id, _) = old.migration.get_or_insert_with(||
            (uuid::Uuid::new_v4().to_string(), self.root.clone())).clone();
        legacy.save(&old)?; // Old executables now fail closed, before any copy is armed.
        if !shared.imports.contains(&id) {
            for attempt in &old.attempts {
                if !shared.attempts.iter().any(|a| a.request_id == attempt.request_id) {
                    shared.attempts.push(attempt.clone());
                }
            }
            if shared.account.is_empty() || shared.account == old.account {
                if shared.view.approval.is_none() {
                    shared.account = old.account.clone();
                    shared.view = old.view.clone();
                    shared.view.checked_at = None;
                    shared.view.state = "unknown".into();
                    shared.view.retry_at = None;
                    shared.view.message = Some("Shared reset approval needs a fresh eligibility check.".into());
                }
                if shared.view.approval.as_ref().is_some_and(|a|
                    Self::attempt_blocks(&shared, &a.offer.id)) {
                    shared.view.approval = None;
                }
            } else if old.view.approval.is_some() {
                return Err("The isolated reset approval belongs to a different Claude account. Reconnect before using it.".into());
            }
            shared.imports.push(id);
            self.save(&shared)?;
        }
        old.view.approval = None;
        legacy.save(&old)
    }
    fn read(&self) -> Result<Record, String> {
        let record = self.read_record()?;
        if record.migration.is_some() {
            return Err("Reset approval moved to the shared account store. Update this client before using it.".into());
        }
        Ok(record)
    }
    fn read_record(&self) -> Result<Record, String> {
        match super::gate::read_json(&self.root.join(FILE)) {
            Ok(v) => Ok(v),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Record::default()),
            Err(_) => Err("The reset approval record is unreadable. No reset will be used.".into()),
        }
    }
    fn save(&self, record: &Record) -> Result<(), String> {
        super::atomic_write(&self.root.join(FILE), record)
            .and_then(|_| fs::File::open(&self.root)?.sync_all())
            .map_err(|_| "Could not save the reset record. No further action was taken.".into())
    }
    fn lock(&self) -> Result<RecordLock, String> {
        fs::create_dir_all(&self.root).map_err(|_| "Reset storage unavailable.")?;
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
            let file = options
                .open(self.root.join("claude-reset-offers.lock"))
                .map_err(|_| "Reset lock unavailable.")?;
            // lock-busy: busy is held across a network poll, so it is answered at once in words: a reset check is in progress, try again shortly.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err("A reset check or action is in progress. Try again shortly.".into());
            }
            Ok(RecordLock(file))
        }
        #[cfg(not(unix))]
        {
            Err("Reset approval is not supported on this platform yet.".into())
        }
    }
    /// One five-minute reservation shared by every desktop and terminal poller.
    /// An approved poll reads eligibility once and uses that same fresh preflight.
    pub fn tick(&self, bin: &Path, authorized: impl Fn() -> bool) -> View {
        self.tick_using(crate::util::now_millis(), |approved| {
            let mut transport = super::reset_transport::System::connect(bin)?;
            self.poll_transport(approved, &mut transport, authorized)
        })
    }
    /// Injection seam for the real-reader/fake-redemption proof. Production tick
    /// constructs System only after winning the shared poll reservation.
    pub fn tick_with_transport(&self, transport: &mut dyn Transport, authorized: impl Fn() -> bool) -> View {
        self.tick_using(crate::util::now_millis(), |approved|
            self.poll_transport(approved, transport, authorized))
    }
    fn poll_transport(&self, approved: bool, transport: &mut dyn Transport, authorized: impl Fn() -> bool) -> Result<View, String> {
        if approved {
            match self.use_approved(transport, authorized) {
                // A successful read can find usage below 99% or an unusable offer.
                Err(error) if error.starts_with("Reset not used:") => Ok(self.view()),
                other => other,
            }
        } else { self.refresh_transport(transport) }
    }
    fn tick_using(&self, now: u64, read: impl FnOnce(bool) -> Result<View, String>) -> View {
        let approved = {
            let Ok(_lock) = self.lock() else { return self.view(); };
            let Ok(mut record) = self.read() else { return self.view(); };
            let v = &record.view;
            if record.poll_after.is_some_and(|t| t > now)
                || v.retry_at.is_some_and(|t| t > now)
                || (v.state == "fresh" && !self.prepared_action_due(now) && v.checked_at.is_some_and(|t|
                    t <= now && now - t < super::REFRESH_INTERVAL_MS)) {
                return Self::visible(record.view, now);
            }
            record.poll_after = Some(now + super::REFRESH_INTERVAL_MS);
            if self.save(&record).is_err() { return self.view(); }
            record.view.approval.is_some()
        };
        match read(approved) {
            Ok(view) => view,
            Err(error) => {
                self.record_failure(error);
                self.view()
            }
        }
    }
    fn record_failure(&self, error: String) {
        if let Ok(_lock) = self.lock() {
            if let Ok(mut record) = self.read() {
                record.view.state = "unknown".into();
                record.view.message = Some(error);
                record.view.retry_at = Some(crate::util::now_millis() + super::REFRESH_INTERVAL_MS);
                let _saved = self.save(&record);
            }
        }
    }
    pub fn prepared_action_due(&self, now: u64) -> bool {
        let Ok(record) = self.read() else {
            return false;
        };
        let v = Self::visible(record.view, now);
        v.state == "fresh"
            && v.approval.is_some()
            && v.weekly_used.is_some_and(|used| used >= WEEKLY_THRESHOLD)
            && v.weekly_resets_at.is_some_and(|until| until > now)
    }
    pub fn view(&self) -> View {
        let now = crate::util::now_millis();
        match self.read() {
            Ok(record) => Self::visible(record.view, now),
            Err(e) => View {
                message: Some(e),
                ..View::default()
            },
        }
    }
    fn visible(mut v: View, now: u64) -> View {
        if v.state == "fresh"
            && v.checked_at
                .is_none_or(|t| t > now || now - t >= super::REFRESH_INTERVAL_MS)
        {
            v.state = "stale".into();
            v.message = Some("Reset offers need a fresh check.".into());
        }
        v.offers.retain(|o| o.remaining > 0 && o.expires_at > now);
        if v.approval
            .as_ref()
            .is_some_and(|a| a.offer.expires_at <= now)
        {
            v.approval = None;
        }
        if let Some(a) = &mut v.last_attempt {
            a.account.clear();
        }
        v
    }
    fn update(record: &mut Record, account: Account, reading: Reading, now: u64) {
        if record.account != account.key {
            record.view.approval = None;
            record.view.last_attempt = None;
        }
        record.account = account.key;
        if record.view.approval.as_ref().is_some_and(|a| {
            !reading.eligible
                || !reading
                    .offers
                    .iter()
                    .any(|o| o.same_grant(&a.offer) && o.weekly(now))
        }) {
            record.view.approval = None;
        }
        record.view.weekly_used = reading.weekly_used;
        record.view.weekly_resets_at = reading.weekly_reset;
        record.view.offers = reading.offers;
        record.view.checked_at = Some(now);
        record.view.retry_at = None;
        record.view.state = if reading.eligible {
            "fresh"
        } else {
            "ineligible"
        }
        .into();
        record.view.message = reading.reason;
    }
    pub fn refresh(&self, bin: &Path, force: bool) -> View {
        let now = crate::util::now_millis();
        let Ok(_lock) = self.lock() else {
            return self.view();
        };
        let Ok(mut record) = self.read() else {
            return self.view();
        };
        let v = &record.view;
        let waiting = if force {
            // **A manual check is not an automatic one.** The automatic reservation (`poll_after`)
            // and the failure wait (`retry_at`) exist so the monitor does not hit the provider
            // every tick; a person who just fixed the problem and asked again must not be handed
            // the old answer for five minutes. What still holds for him is the short cooldown
            // that stops a double-click from hitting the provider twice, measured from the last
            // attempt of any kind: a reading, a failure (whose wait was set five minutes past
            // it) or the monitor's reservation (set five minutes ahead of its attempt).
            let interval = super::REFRESH_INTERVAL_MS;
            let last_attempt = [
                v.checked_at,
                v.retry_at.map(|t| t.saturating_sub(interval)),
                record.poll_after.map(|t| t.saturating_sub(interval)),
            ]
            .into_iter()
            .flatten()
            .filter(|t| *t <= now)
            .max();
            last_attempt.is_some_and(|t| now - t < 5000)
        } else {
            record.poll_after.is_some_and(|t| t > now)
                || v.retry_at.is_some_and(|t| t > now)
                || v.checked_at.is_some_and(|t| t <= now && now - t < super::REFRESH_INTERVAL_MS)
        };
        if waiting {
            return Self::visible(record.view, now);
        }
        // Tests supply a Transport explicitly. Unit tests must never touch real credentials.
        #[cfg(not(test))]
        let result = super::reset_transport::System::connect(bin)
            .and_then(|mut t| Self::read_transport(&mut t));
        #[cfg(test)]
        let result: Result<(Account, Reading), String> = {
            let _best_effort = bin;
            Err("Fixture has no reset connection.".into())
        };
        match result {
            Ok((account, reading)) => {
                Self::update(&mut record, account, reading, crate::util::now_millis())
            }
            Err(e) => {
                record.view.state = "unknown".into();
                record.view.message = Some(e);
                record.view.retry_at = Some(now + super::REFRESH_INTERVAL_MS);
            }
        }
        let _best_effort = self.save(&record);
        Self::visible(record.view, crate::util::now_millis())
    }
    pub fn refresh_transport(&self, t: &mut dyn Transport) -> Result<View, String> {
        let (account, reading) = Self::read_transport(t)?;
        let _lock = self.lock()?;
        let mut record = self.read()?;
        Self::update(&mut record, account, reading, crate::util::now_millis());
        self.save(&record)?;
        Ok(self.view())
    }

    fn read_transport(t: &mut dyn Transport) -> Result<(Account, Reading), String> {
        let account = t.account()?;
        if account.key.is_empty() {
            return Err("Claude account identity unavailable.".into());
        }
        Ok((account, parse(&t.usage()?)?))
    }
    pub fn clear_connection(&self) {
        if let Ok(_lock) = self.lock() {
            if let Ok(mut record) = self.read() {
                record.view = View::default();
                record.account.clear();
                let _best_effort = self.save(&record);
            }
        }
    }
    /// Called only by user approval surfaces. There is no MCP approval tool.
    pub fn approve(&self, offer: &Offer) -> Result<View, String> {
        let _lock = self.lock()?;
        let mut record = self.read()?;
        let now = crate::util::now_millis();
        let v = Self::visible(record.view.clone(), now);
        if v.state != "fresh"
            || record.account.is_empty()
            || !offer.weekly(now)
            || !v.offers.iter().any(|o| o.same_grant(offer))
        {
            return Err("Refresh reset offers before approving this offer.".into());
        }
        if Self::attempt_blocks(&record, &offer.id) || record.attempts.len() >= MAX_ATTEMPTS {
            return Err(
                "This reset already has a recorded attempt. Check its outcome in Claude.".into(),
            );
        }
        record.view.approval = Some(Approval {
            id: uuid::Uuid::new_v4().to_string(),
            offer: offer.clone(),
            approved_at: now,
            weekly_threshold: WEEKLY_THRESHOLD as u8,
        });
        self.save(&record)?;
        Ok(self.view())
    }
    pub fn revoke(&self) -> Result<View, String> {
        let _lock = self.lock()?;
        let mut record = self.read()?;
        record.view.approval = None;
        self.save(&record)?;
        Ok(self.view())
    }
    /// The one-use fence: a recorded attempt on this grant, or ANY uncertain attempt (the network
    /// outcome is unknown, so a duplicate redemption cannot be ruled out), blocks a new one.
    /// The exception is a definite `notUsed` answer (`not_limited`, `cooldown`, `ineligible`):
    /// the service said nothing was consumed, so the same grant can be offered and used again
    /// once it becomes eligible.
    fn attempt_blocks(r: &Record, grant: &str) -> bool {
        r.attempts.iter().any(|a| {
            a.account == r.account
                && ((a.grant_id == grant && a.outcome != "notUsed") || a.outcome == "uncertain")
        })
    }
    pub fn use_approved(
        &self,
        t: &mut dyn Transport,
        still_authorized: impl Fn() -> bool,
    ) -> Result<View, String> {
        let lock = self.lock()?;
        let record = self.read()?;
        let approved = record
            .view
            .approval
            .clone()
            .ok_or("The user has not approved a weekly reset in Technical Settings or the terminal.")?;
        if record.view.retry_at.is_some_and(|t| t > crate::util::now_millis()) {
            return Err("The eligibility check is waiting for its next five-minute retry.".into());
        }
        let approval_account = record.account.clone();
        drop(lock); // A user can revoke approval while read-only preflight is in flight.
        if !still_authorized() {
            return Err("This work session is no longer authorized.".into());
        }
        let (account, reading) = Self::read_transport(t).inspect_err(|error| {
            self.record_failure(error.clone());
        })?;
        let _lock = self.lock()?;
        let mut record = self.read()?;
        if record.account != approval_account
            || record
                .view
                .approval
                .as_ref()
                .is_none_or(|a| a.id != approved.id)
        {
            return Err(
                "Reset approval was revoked or replaced during the check. Nothing was used.".into(),
            );
        }
        let now = crate::util::now_millis();
        if account.key != record.account {
            Self::update(&mut record, account, reading, now);
            self.save(&record)?;
            return Err("The Claude account changed. Fresh user approval is required.".into());
        }
        let offer = reading
            .offers
            .iter()
            .find(|o| o.same_grant(&approved.offer) && o.weekly(now));
        let allowed = reading.eligible
            && offer.is_some_and(|o| o.usable_now)
            && reading.next.as_deref() == Some(approved.offer.id.as_str())
            && reading.cooldown.is_none_or(|until| until <= now)
            && reading
                .weekly_used
                .is_some_and(|used| used >= WEEKLY_THRESHOLD)
            && reading.weekly_reset.is_some_and(|reset| reset > now)
            && approved.weekly_threshold == WEEKLY_THRESHOLD as u8
            && !Self::attempt_blocks(&record, &approved.offer.id)
            && record.attempts.len() < MAX_ATTEMPTS;
        Self::update(&mut record, account, reading, now);
        self.save(&record)?;
        if !allowed {
            return Err("Reset not used: weekly usage must reach 99% and the approved weekly offer must still be usable.".into());
        }
        if !still_authorized() {
            return Err("This work session stopped before the reset. Nothing was used.".into());
        }
        let mut attempt = Attempt {
            grant_id: approved.offer.id.clone(),
            request_id: uuid::Uuid::new_v4().to_string(),
            at: now,
            outcome: "uncertain".into(),
            account: record.account.clone(),
        };
        record.view.approval = None;
        record.view.last_attempt = Some(attempt.clone());
        record.attempts.push(attempt.clone());
        self.save(&record)?; // durable one-use fence BEFORE any network write
        let result = t.redeem(&attempt.grant_id, &attempt.request_id);
        attempt.outcome = match result
            .as_ref()
            .ok()
            .and_then(|v| v.get("result"))
            .and_then(Value::as_str)
        {
            Some("reset") => "used",
            Some("already_used") => "alreadyUsed",
            Some("not_limited" | "cooldown" | "ineligible") => "notUsed",
            _ => "uncertain",
        }
        .into();
        record.view.last_attempt = Some(attempt.clone());
        *record.attempts.last_mut().unwrap() = attempt;
        record.view.state = "unknown".into();
        record.view.checked_at = None;
        record.view.retry_at = None;
        record.view.message =
            Some("A reset was attempted. Refresh quota to see the current allowance.".into());
        self.save(&record)?;
        // The desktop monitor will invalidate its cached quota reading at this marker.
        let _best_effort = super::atomic_write(
            &self.root.join("claude-reset-refresh.json"),
            &record.view.last_attempt,
        );
        Ok(self.view())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::Scratch;
    use super::*;
    use serde_json::json;
    struct Fake {
        account: String,
        usage: Value,
        reply: Result<Value, String>,
        posts: usize,
    }
    impl Fake {
        fn new(weekly: f64) -> Self {
            Self {
                account: "account-a".into(),
                usage: json!({
                "five_hour":{"utilization":100},
                "seven_day":{"utilization":weekly,"resets_at":"2099-01-01T00:00:00Z"},
                "cedar_ember":{"eligible":true,"ineligible_reason":null,"next_grant_id":"launch","cooldown_until":null,
                    "grants":[{"id":"launch","label":"One weekly reset","resets_left":1,
                        "starts_at":"2020-01-01T00:00:00Z","ends_at":"2099-02-01T00:00:00Z",
                        "clears":["five_hour","seven_day"],"paused":false,"usable_now":true,"use_requires_limit":false,"blocking":[]}]}
                }),
                reply: Ok(json!({"result":"reset"})),
                posts: 0,
            }
        }
    }
    impl Transport for Fake {
        fn account(&mut self) -> Result<Account, String> {
            Ok(Account {
                key: self.account.clone(),
            })
        }
        fn usage(&mut self) -> Result<Value, String> {
            Ok(self.usage.clone())
        }
        fn redeem(&mut self, _: &str, request: &str) -> Result<Value, String> {
            assert!(uuid::Uuid::parse_str(request).is_ok());
            self.posts += 1;
            self.reply.clone()
        }
    }
    fn seeded(root: &Path, t: &mut Fake) -> Service {
        let s = Service::new(root);
        let mut record = Record::default();
        let (account, reading) = Service::read_transport(t).unwrap();
        Service::update(&mut record, account, reading, crate::util::now_millis());
        s.save(&record).unwrap();
        s
    }
    fn approve(s: &Service) {
        s.approve(&s.view().offers[0]).unwrap();
    }
    #[test]
    fn nightly_approval_moves_once_and_a_restart_cannot_undo_terminal_revocation() {
        let nightly = Scratch::new();
        let common = Scratch::new();
        let mut t = Fake::new(99.);
        let old = seeded(nightly.path(), &mut t);
        approve(&old);
        let approval = old.view().approval.unwrap().id;
        let app = Service::new(common.path());
        app.import_legacy(nightly.path()).unwrap();
        let terminal = Service::new(common.path());
        assert_eq!(terminal.view().approval.unwrap().id, approval);
        assert!(old.use_approved(&mut t, || true).is_err());
        assert_eq!(t.posts, 0);
        // Simulate a crash after the import committed but before source cleanup.
        let mut source = old.read_record().unwrap();
        source.view.approval = app.view().approval;
        old.save(&source).unwrap();
        terminal.revoke().unwrap();
        app.import_legacy(nightly.path()).unwrap();
        assert!(app.view().approval.is_none());
        assert!(app.use_approved(&mut t, || true).is_err());
        assert_eq!(t.posts, 0);
    }

    #[test]
    fn migration_preserves_uncertain_attempt_fence_across_clients() {
        let nightly = Scratch::new();
        let common = Scratch::new();
        let mut t = Fake::new(99.);
        let old = seeded(nightly.path(), &mut t);
        approve(&old);
        t.reply = Err("lost reply".into());
        old.use_approved(&mut t, || true).unwrap();
        let shared = seeded(common.path(), &mut t);
        approve(&shared);
        shared.import_legacy(nightly.path()).unwrap();
        assert!(shared.view().approval.is_none());
        assert!(shared.use_approved(&mut t, || true).is_err());
        shared.refresh_transport(&mut t).unwrap();
        assert!(shared.approve(&shared.view().offers[0]).is_err());
        assert_eq!(t.posts, 1);
    }

    #[test]
    fn shared_poller_recovers_at_five_minutes_without_duplicate_preflight_or_redemption() {
        let root = Scratch::new();
        let mut t = Fake::new(99.);
        let app = seeded(root.path(), &mut t);
        approve(&app);
        let terminal = Service::new(root.path());
        let now = crate::util::now_millis();
        let failed = app.tick_using(now, |approved| {
            assert!(approved);
            Err("Claude reset eligibility check: HTTP 429".into())
        });
        assert_eq!(failed.state, "unknown");
        assert!(failed.message.unwrap().contains("429"));
        assert!(failed.approval.is_some());
        terminal.tick_using(now + 1000, |_| panic!("peer must honor shared retry"));
        app.tick_using(now + super::super::REFRESH_INTERVAL_MS - 1,
            |_| panic!("must not flood Anthropic"));
        assert!(failed.retry_at.unwrap() <= now + super::super::REFRESH_INTERVAL_MS + 1000);
        // Advance the persisted deadlines instead of sleeping for five minutes.
        let due = crate::util::now_millis();
        let mut record = app.read().unwrap();
        record.poll_after = Some(due - 1);
        record.view.retry_at = Some(due - 1);
        app.save(&record).unwrap();
        let completed = terminal.tick_using(due, |approved| {
            assert!(approved);
            terminal.use_approved(&mut t, || true)
        });
        assert_eq!(completed.last_attempt.unwrap().outcome, "used");
        assert_eq!(t.posts, 1);
        app.tick_using(due + 1, |_| panic!("app must not repeat terminal's check"));
        assert!(app.view().approval.is_none());
        assert!(app.use_approved(&mut t, || true).is_err());
        assert_eq!(t.posts, 1);
    }

    #[test]
    fn failed_action_preflight_is_visible_and_never_records_a_redemption() {
        struct Unavailable;
        impl Transport for Unavailable {
            fn account(&mut self) -> Result<Account, String> { Err("HTTP 429".into()) }
            fn usage(&mut self) -> Result<Value, String> { panic!("account failed") }
            fn redeem(&mut self, _: &str, _: &str) -> Result<Value, String> { panic!("no preflight") }
        }
        let root = Scratch::new();
        let mut t = Fake::new(99.);
        let service = seeded(root.path(), &mut t);
        approve(&service);
        assert!(service.use_approved(&mut Unavailable, || true).is_err());
        let v = service.view();
        assert_eq!(v.state, "unknown");
        assert!(v.message.unwrap().contains("429"));
        assert!(v.approval.is_some());
        assert!(v.last_attempt.is_none());
        assert_eq!(service.read().unwrap().attempts.len(), 0);
    }

    #[test]
    fn failed_offer_read_rechecks_within_five_minutes_and_preserves_approval() {
        let root = Scratch::new();
        let mut transport = Fake::new(99.);
        let service = seeded(root.path(), &mut transport);
        approve(&service);
        let mut record = service.read().unwrap();
        record.view.checked_at = None;
        service.save(&record).unwrap();
        let before = crate::util::now_millis();
        let failed = service.refresh(Path::new("/nonexistent/claude"), false);
        assert_eq!(failed.state, "unknown");
        assert!(failed.approval.is_some());
        assert!(failed.retry_at.unwrap() <= before + super::super::REFRESH_INTERVAL_MS + 1000);
        assert!(!service.prepared_action_due(before));
        assert_eq!(transport.posts, 0);
        service.refresh_transport(&mut transport).unwrap();
        assert!(service.prepared_action_due(crate::util::now_millis()));
        service.use_approved(&mut transport, || true).unwrap();
        assert_eq!(transport.posts, 1);
        assert!(service.use_approved(&mut transport, || true).is_err());
        assert_eq!(transport.posts, 1);
    }

    /// Hunt part 1 finding 23: a manual reset-offer check is not held by the monitor's
    /// five-minute reservation or by the failure wait, but the 5 s double-click cooldown holds.
    /// (In tests the connection is a fixture that always fails, which is what makes a real
    /// attempt visible: it sets `retry_at`.)
    #[test]
    fn a_manual_offer_check_is_not_held_by_the_automatic_reservation_or_failure_wait() {
        let root = Scratch::new();
        let mut transport = Fake::new(20.);
        let service = seeded(root.path(), &mut transport);
        let interval = super::super::REFRESH_INTERVAL_MS;
        let now = crate::util::now_millis();
        // The monitor reserved its poll a minute ago; nothing has been read since.
        let mut record = service.read().unwrap();
        record.view.checked_at = None;
        record.view.retry_at = None;
        record.poll_after = Some(now + interval - 60_000);
        service.save(&record).unwrap();
        assert!(service.refresh(Path::new("/nonexistent/claude"), false).retry_at.is_none(),
            "the automatic check still waits for the reservation");
        let asked = service.refresh(Path::new("/nonexistent/claude"), true);
        assert!(asked.retry_at.is_some(), "his manual check went to the provider");
        let again = service.refresh(Path::new("/nonexistent/claude"), true);
        assert_eq!(again.retry_at, asked.retry_at, "a double-click straight after is absorbed");
        // A failure a minute ago is five minutes of automatic wait; he still gets a real check.
        let mut record = service.read().unwrap();
        record.poll_after = None;
        let stale = now + interval - 60_000;
        record.view.retry_at = Some(stale);
        service.save(&record).unwrap();
        let asked = service.refresh(Path::new("/nonexistent/claude"), true);
        assert!(asked.retry_at.unwrap() > stale, "the failure wait did not hold his manual check");
    }

    #[test]
    fn approval_can_be_given_early_and_armed_action_survives_restart() {
        let root = Scratch::new();
        let mut t = Fake::new(20.);
        let s = seeded(root.path(), &mut t);
        approve(&s);
        assert!(!s.prepared_action_due(crate::util::now_millis()));
        assert_eq!(t.posts, 0);
        assert_eq!(s.view().approval.unwrap().weekly_threshold, 99);
        let restarted = Service::new(root.path());
        assert!(restarted.use_approved(&mut t, || true).is_err());
        assert_eq!(t.posts, 0);
        t.usage["seven_day"]["utilization"] = json!(99.);
        let mut record = restarted.read().unwrap();
        let (account, reading) = Service::read_transport(&mut t).unwrap();
        Service::update(&mut record, account, reading, crate::util::now_millis());
        restarted.save(&record).unwrap();
        assert!(restarted.prepared_action_due(crate::util::now_millis()));
        let v = restarted.use_approved(&mut t, || true).unwrap();
        assert_eq!(v.last_attempt.unwrap().outcome, "used");
        assert!(v.approval.is_none());
        assert_eq!(t.posts, 1);
        assert!(restarted.use_approved(&mut t, || true).is_err());
        assert_eq!(t.posts, 1);
    }
    #[test]
    fn only_weekly_99_qualifies_never_the_five_hour_93_rule() {
        for weekly in [0., 93., 98.99, 99., 100.] {
            let root = Scratch::new();
            let mut t = Fake::new(weekly);
            let s = seeded(root.path(), &mut t);
            approve(&s);
            let result = s.use_approved(&mut t, || true);
            assert_eq!(result.is_ok(), weekly >= 99.);
            assert_eq!(t.posts, usize::from(weekly >= 99.));
        }
    }
    #[test]
    fn null_unknown_and_malformed_are_not_an_empty_inventory() {
        for value in [
            json!({}),
            json!({"cedar_ember":null}),
            json!({"cedar_ember":{"eligible":true}}),
        ] {
            assert!(parse(&value).is_err());
        }
        let mut t = Fake::new(99.);
        t.usage["cedar_ember"]["grants"][0]["ends_at"] = json!("bad");
        assert!(parse(&t.usage).is_err());
        t = Fake::new(99.);
        t.usage["cedar_ember"]["eligible"] = json!(false);
        t.usage["cedar_ember"]["ineligible_reason"] = json!("surface");
        assert!(parse(&t.usage)
            .unwrap()
            .reason
            .unwrap()
            .contains("Usage page"));
    }
    #[test]
    fn revocation_account_change_and_stopped_scope_never_post() {
        for case in ["revoke", "account", "stop"] {
            let root = Scratch::new();
            let mut t = Fake::new(99.);
            let s = seeded(root.path(), &mut t);
            approve(&s);
            match case {
                "revoke" => {
                    s.revoke().unwrap();
                }
                "account" => t.account = "account-b".into(),
                _ => {}
            }
            assert!(s.use_approved(&mut t, || case != "stop").is_err());
            assert_eq!(t.posts, 0);
            if case != "stop" {
                assert!(s.view().approval.is_none());
            }
        }
    }
    #[test]
    fn changed_grant_or_conditions_and_missing_weekly_reading_never_post() {
        for case in [
            "expiry", "count", "clears", "next", "paused", "usable", "cooldown", "weekly", "reset",
        ] {
            let root = Scratch::new();
            let mut t = Fake::new(99.);
            let s = seeded(root.path(), &mut t);
            approve(&s);
            match case {
                "expiry" => {
                    t.usage["cedar_ember"]["grants"][0]["ends_at"] = json!("2021-01-01T00:00:00Z")
                }
                "count" => t.usage["cedar_ember"]["grants"][0]["resets_left"] = json!(0),
                "clears" => t.usage["cedar_ember"]["grants"][0]["clears"] = json!(["five_hour"]),
                "next" => t.usage["cedar_ember"]["next_grant_id"] = json!("different"),
                "paused" => t.usage["cedar_ember"]["grants"][0]["paused"] = json!(true),
                "usable" => t.usage["cedar_ember"]["grants"][0]["usable_now"] = json!(false),
                "cooldown" => {
                    t.usage["cedar_ember"]["cooldown_until"] = json!("2099-01-01T00:00:00Z")
                }
                "weekly" => t.usage["seven_day"]["utilization"] = Value::Null,
                _ => t.usage["seven_day"]["resets_at"] = json!("2020-01-01T00:00:00Z"),
            }
            assert!(s.use_approved(&mut t, || true).is_err(), "{case}");
            assert_eq!(t.posts, 0, "{case}");
        }
    }
    #[test]
    fn no_approval_and_five_hour_only_offers_cannot_be_used() {
        let root = Scratch::new();
        let mut t = Fake::new(100.);
        let s = seeded(root.path(), &mut t);
        assert!(s.use_approved(&mut t, || true).is_err());
        assert_eq!(t.posts, 0);
        t.usage["cedar_ember"]["grants"][0]["clears"] = json!(["five_hour"]);
        let s = seeded(root.path(), &mut t);
        assert!(s.approve(&s.view().offers[0]).is_err());
    }
    #[test]
    fn timeout_and_crash_fence_survive_restart_and_prevent_duplicate_use() {
        for reply in [
            Err("timeout".into()),
            Ok(json!({"result":"unexpected"})),
            Ok(json!({"result":"already_used"})),
        ] {
            let root = Scratch::new();
            let mut t = Fake::new(99.);
            let s = seeded(root.path(), &mut t);
            approve(&s);
            t.reply = reply;
            let v = s.use_approved(&mut t, || true).unwrap();
            assert_ne!(v.last_attempt.unwrap().outcome, "used");
            let restarted = Service::new(root.path());
            assert!(restarted.use_approved(&mut t, || true).is_err());
            // Even refreshing a server response that still advertises the same grant
            // cannot remove the durable fence or let a stale Settings click reapprove it.
            let mut record = restarted.read().unwrap();
            let (account, reading) = Service::read_transport(&mut t).unwrap();
            Service::update(&mut record, account, reading, crate::util::now_millis());
            restarted.save(&record).unwrap();
            assert!(restarted.approve(&restarted.view().offers[0]).is_err());
            assert_eq!(t.posts, 1);
        }
    }
    #[test]
    fn uncertain_attempt_blocks_even_a_different_offer() {
        let root = Scratch::new();
        let mut t = Fake::new(99.);
        let desktop = seeded(root.path(), &mut t);
        approve(&desktop);
        t.reply = Err("response lost".into());
        desktop.use_approved(&mut t, || true).unwrap();
        t.usage["cedar_ember"]["grants"][0]["id"] = json!("another");
        t.usage["cedar_ember"]["next_grant_id"] = json!("another");
        let terminal = Service::new(root.path());
        terminal.refresh_transport(&mut t).unwrap();
        assert!(terminal.approve(&terminal.view().offers[0]).is_err());
        assert_eq!(t.posts, 1);
    }

    #[test]
    fn a_definite_not_used_answer_does_not_fence_the_same_grant_out_forever() {
        let root = Scratch::new();
        let mut t = Fake::new(99.);
        let service = seeded(root.path(), &mut t);
        approve(&service);
        t.reply = Ok(json!({"result":"cooldown"}));
        let v = service.use_approved(&mut t, || true).unwrap();
        assert_eq!(v.last_attempt.unwrap().outcome, "notUsed");
        assert_eq!(t.posts, 1);
        // A later offer for the SAME grant id says it is available again.
        service.refresh_transport(&mut t).unwrap();
        service.approve(&service.view().offers[0]).expect("a notUsed grant must be approvable again");
        t.reply = Ok(json!({"result":"reset"}));
        let v = service.use_approved(&mut t, || true).unwrap();
        assert_eq!(v.last_attempt.unwrap().outcome, "used");
        assert_eq!(t.posts, 2);
        // A grant that really was used stays fenced.
        service.refresh_transport(&mut t).unwrap();
        assert!(service.approve(&service.view().offers[0]).is_err());
        assert_eq!(t.posts, 2);
    }

    #[test]
    #[ignore = "invoked by the cross-process proof"]
    fn reset_process_peer() {
        let root = std::env::var_os("RICHOS_RESET_PEER_ROOT").unwrap();
        let mut t = Fake::new(99.);
        let service = Service::new(Path::new(&root));
        let _outcome = service.use_approved(&mut t, || true);
        if t.posts != 0 {
            use std::io::Write;
            let mut log = fs::OpenOptions::new().create(true).append(true)
                .open(Path::new(&root).join("posts")).unwrap();
            writeln!(log, "redeemed").unwrap();
        }
    }

    #[test]
    fn desktop_and_terminal_processes_consume_one_shared_approval() {
        let root = Scratch::new();
        let mut t = Fake::new(20.);
        let desktop = seeded(root.path(), &mut t);
        approve(&desktop);
        let peer = || std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "quota::resets::tests::reset_process_peer"])
            .env("RICHOS_RESET_PEER_ROOT", root.path())
            .stdout(std::process::Stdio::null()).spawn().unwrap();
        let mut a = peer();
        let mut b = peer();
        assert!(a.wait().unwrap().success());
        assert!(b.wait().unwrap().success());
        assert_eq!(fs::read_to_string(root.path().join("posts")).unwrap(), "redeemed\n");
        assert!(desktop.view().approval.is_none());
        assert_eq!(desktop.view().last_attempt.unwrap().outcome, "used");
    }

    #[test]
    fn ending_critical_section_unlocks_even_with_an_inherited_descriptor() {
        let root = Scratch::new();
        let s = Service::new(root.path());
        let lock = s.lock().unwrap();
        let inherited = lock.0.try_clone().unwrap();
        assert!(s.lock().is_err());
        drop(lock);
        let next = s.lock().expect("descriptor inheritance must not retain the lock");
        drop(inherited);
        assert!(s.lock().is_err(), "old descriptor must not release the new lock");
        drop(next);
        assert!(s.lock().is_ok());
    }
    #[test]
    fn disk_corruption_and_competing_process_lock_fail_closed() {
        let root = Scratch::new();
        let mut t = Fake::new(99.);
        let s = seeded(root.path(), &mut t);
        approve(&s);
        let lock = s.lock().unwrap();
        assert!(Service::new(root.path())
            .use_approved(&mut t, || true)
            .is_err());
        drop(lock);
        fs::write(root.path().join(FILE), "not-json").unwrap();
        assert!(s.use_approved(&mut t, || true).is_err());
        assert_eq!(t.posts, 0);
    }
    #[test]
    fn stopped_during_preflight_is_checked_again_before_post() {
        let root = Scratch::new();
        let mut t = Fake::new(99.);
        let s = seeded(root.path(), &mut t);
        approve(&s);
        let calls = std::cell::Cell::new(0);
        assert!(s
            .use_approved(&mut t, || {
                calls.set(calls.get() + 1);
                calls.get() == 1
            })
            .is_err());
        assert_eq!(t.posts, 0);
    }
    #[test]
    fn crash_after_durable_intent_never_reissues_a_post() {
        struct Crash(Fake);
        impl Transport for Crash {
            fn account(&mut self) -> Result<Account, String> {
                self.0.account()
            }
            fn usage(&mut self) -> Result<Value, String> {
                self.0.usage()
            }
            fn redeem(&mut self, _: &str, _: &str) -> Result<Value, String> {
                panic!("simulated process loss")
            }
        }
        let root = Scratch::new();
        let mut t = Fake::new(99.);
        let s = seeded(root.path(), &mut t);
        approve(&s);
        let _best_effort = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            s.use_approved(&mut Crash(Fake::new(99.)), || true)
        }));
        let restarted = Service::new(root.path());
        assert_eq!(restarted.view().last_attempt.unwrap().outcome, "uncertain");
        assert!(restarted.use_approved(&mut t, || true).is_err());
        assert_eq!(t.posts, 0);
    }
    #[test]
    fn revocation_during_network_preflight_prevents_the_post() {
        struct Revoking<'a> {
            service: &'a Service,
            fake: Fake,
        }
        impl Transport for Revoking<'_> {
            fn account(&mut self) -> Result<Account, String> {
                self.fake.account()
            }
            fn usage(&mut self) -> Result<Value, String> {
                self.service.revoke().unwrap();
                self.fake.usage()
            }
            fn redeem(&mut self, grant: &str, request: &str) -> Result<Value, String> {
                self.fake.redeem(grant, request)
            }
        }
        let root = Scratch::new();
        let mut fake = Fake::new(99.);
        let service = seeded(root.path(), &mut fake);
        approve(&service);
        let mut transport = Revoking {
            service: &service,
            fake,
        };
        assert!(service
            .use_approved(&mut transport, || true)
            .unwrap_err()
            .contains("revoked"));
        assert_eq!(transport.fake.posts, 0);
        assert!(service.view().approval.is_none());
    }
}
