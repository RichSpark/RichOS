//! **Inside the twenty-minute window a held team is released one at a time** (Rich's decision,
//! 2026-10-29, after Work's 09:50Z limit). His words: *"release inside the window as written,
//! staggered, with the rate judged from the speed measured before the hold (a rate of 0 while
//! everyone is held is no measurement). The stagger stops before the account would reach 100%
//! ahead of the reset, and anything not yet released waits for the reset itself."*
//!
//! What it replaces: inside [`super::RESET_EXEMPTION_MS`] of the five-hour reset every held
//! call was admitted at once whenever the measured speed was 0, and with the whole team held
//! the speed always read 0. At 09:49:59Z that released ten workers and Stu's dispatch together
//! and they spent the rest of Work's window in about 8 seconds.
//!
//! **How a release is judged.** The account's reading says `used`% at `checked_at`, and the
//! window resets at `resets_at`. The speed before the hold (`pre_hold`, points per ms) is the
//! pace of the helpers that were working, the `team` (released since or still held; a new
//! agent's dispatch never ran, so it is not one of them), so one released call is judged at
//! `pre_hold / team`. With `n` released, dispatches included, the rate is the larger of their
//! share, `pre_hold x n / team`, and what the readings since have actually measured (`speed`,
//! per helper it measured, times `n`; see below). No speed before the hold and none since is no
//! measurement: nobody is released early.
//! The projection at the reset is `used + rate x (resets_at - checked_at)`: counted from the
//! reading, so what the released calls spent since it is counted too. One more is released only
//! while that stays under 100, and never sooner than [`STAGGER_MS`] after the previous one.
//! Whoever is not released waits for the reset, when the window's figures stop holding.
//!
//! **Several gate processes decide together**: each held tool call is its own process, so the
//! released sets live in one small file under `engine-state`, one per account, read and
//! written under an exclusive lock. A helper already released keeps its permit for the rest of
//! that window: its next tool call is the same worker carrying on, not a new one, and it never
//! queues for a fresh release slot. An Agent call starts a new agent and needs a release of its
//! own (`gate::release_key`).
//!
//! **A permit is not a pass for the rest of the window** (third review of 2026-10-29, P1):
//! every call on a permit is judged again on the reading it meets ([`Release::keeps`]). With
//! the first `k` released carrying on, the rate is the larger of their share of the pace before
//! the hold, `pre_hold x k / team`, and `k` times the pace each helper the reading measured is
//! going at, `speed / running`. The `k`th permit carries on only while that projection stays
//! under 100; the latest permits pause first, and pick up again on a later reading that fits, on
//! the same permit. Before, a permit returned at once: a helper released at 95% kept going on a
//! fresh 99% reading with ten minutes left at 1 point a minute (110.4% projected), and at 100%
//! too.
//!
//! **Only the helpers a reading measured divide its speed** (fourth review of 2026-10-29, P1):
//! `running` counts the permits whose run the reading saw (`Ledger::measured`), not every permit
//! released this window. One waiting or finished spends nothing, and one picked up again is
//! counted at the measured pace, never below it.
//!
//! **A speed no helper seen running accounts for resumes nobody** (fifth review of 2026-10-29,
//! rv-20261029T172038Z-eb39280f-2916, P1). A helper whose last call was more than a minute
//! before a reading, one long tool or model step, is not measured though its work is in the
//! speed. With nobody measured, that speed was read as someone else's and each helper picking
//! up again added only its share before the hold, so which ones went on hung on the order they
//! retried in: "order=[2, 1, 0]; permits=[true, true, true]; actual projection=102.90%" against
//! 98.97% for [0, 1, 2]. Now, on a reading that measures a speed and nobody running, no permit
//! that waited picks up again and nobody new is released: they wait for a reading that measures
//! a helper, or for the reset, as Rich's rule has it. A helper still going (its run not
//! stopped) is judged as before.
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

/// **The gap between two releases: one fast check.** While usage is fast the quota is read
/// every minute (`FAST_REFRESH_INTERVAL_MS`), so a release waits for at least one reading taken
/// after the previous one began to spend. At 60 s a twenty-minute window allows at most 20
/// releases (20 x 60 000 = 1 200 000 ms = `RESET_EXEMPTION_MS`).
pub const STAGGER_MS: u64 = super::FAST_REFRESH_INTERVAL_MS;

/// The released set, beside the published reading.
pub const LEDGER_FILE: &str = "quota-release.json";
const LOCK_FILE: &str = "quota-release.lock";

/// **What a held call inside the window is judged on** (`View::release`): set only while the
/// five-hour window is the hold and its reset is less than [`super::RESET_EXEMPTION_MS`] away.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub used: f64,
    pub checked_at: u64,
    pub resets_at: u64,
    /// The speed the readings measure now, points per ms (0 while everyone is held).
    pub speed: f64,
    /// The speed measured over the rise into the hold, points per ms (`Snapshot::pre_hold`).
    pub pre_hold: f64,
}

impl Release {
    /// Points per ms with the first `n` permits going, `running` of the released being the
    /// helpers whose work this reading measured (`Ledger::measured`), and `team` the helpers
    /// whose pace `pre_hold` measured. Each permit is one helper's share, `pre_hold / team`; a new
    /// agent's dispatch never ran before the hold, so it is not in `team`, and it adds a share all
    /// the same (second review of 2026-10-29, finding 1: counting queued dispatches in `team`
    /// made one measured worker's share a tenth of its real pace).
    ///
    /// **Only the helpers the reading measured divide what it measures** (fourth review of
    /// 2026-10-29, rv-20261029T165127Z-b54baa2c-6070, P1). With none of them measured, what the
    /// reading says is kept whole and each of the `n` adds its share; `admit` uses that only
    /// for a helper still going, or on a speed of 0 (fifth review: a measured speed nobody
    /// running accounts for resumes and releases nobody). With `running` measured, each of the
    /// `n` is judged at the larger of its share and `speed / running`, the pace each measured helper is going at: a helper picking up again
    /// after a wait, or released anew, is counted at that pace too, never below it. Before, the
    /// whole released set divided it: three released at 1.2 a minute, two waiting, the one
    /// still going measured at 0.4 a minute was read as 0.13 each and all three went on, 101.2%
    /// projected; two finished and the third at 1.2 a minute, it was judged at 0.4 (98%) and
    /// went on to 102%.
    pub fn rate(&self, n: usize, running: usize, team: usize) -> f64 {
        let share = self.pre_hold / team.max(1) as f64;
        match running {
            0 => self.speed + share * n as f64,
            running => share.max(self.speed / running as f64) * n as f64,
        }
    }
    /// The used percent the window reaches at its reset with the first `n` permits going.
    pub fn projected(&self, n: usize, running: usize, team: usize) -> f64 {
        self.used + self.rate(n, running, team) * self.resets_at.saturating_sub(self.checked_at) as f64
    }
    /// May the `n`th call be released? Only while the account stays under 100% at the reset,
    /// and only on a measurement: with no speed before the hold and none since (the climb was
    /// not seen, as after a restart in the middle of a hold), a rate of 0 is no measurement, and
    /// everyone waits for the reset (second review of 2026-10-29, finding 2).
    pub fn allows(&self, n: usize, running: usize, team: usize) -> bool {
        self.measured() && self.projected(n, running, team) < 100.
    }
    /// May the `k`th permit already released carry on, on this reading? Judged as a release:
    /// the first `k` going, each at the larger of its share of the pace before the hold and the
    /// pace each of the `running` the reading measured is going at, under 100 at the reset and
    /// on a measurement (third review of 2026-10-29, P1; fourth, P1: `running`, not every permit
    /// ever released).
    pub fn keeps(&self, k: usize, running: usize, team: usize) -> bool {
        self.allows(k, running, team)
    }
    /// A speed before the hold or one measured since: a rate of 0 while everyone is held is no
    /// measurement (second review of 2026-10-29, finding 2).
    fn measured(&self) -> bool {
        self.pre_hold > 0. || self.speed > 0.
    }
}

/// One account's released set in its current five-hour window.
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ledger {
    resets_at: u64,
    released: Vec<String>,
    last_at: Option<u64>,
    /// Each permit's latest run: released, or picked up again after a wait, until a call on it
    /// waits. A ledger written before runs were kept has none, and nobody counts as measured.
    #[serde(default)]
    runs: std::collections::BTreeMap<String, Run>,
}

/// **One permit's run** (fourth review of 2026-10-29, P1): when it began, when a call on it was
/// last let through, and when a call on it first waited (it stopped there).
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Run {
    since: u64,
    last: u64,
    #[serde(default)]
    until: Option<u64>,
}

impl Ledger {
    /// **The helpers whose work a reading taken at `checked_at` measured**: the runs that began
    /// at least one fast check ([`STAGGER_MS`]) before it, so the minute the speed was measured
    /// over saw them going (one picked up again since, or just before, is not in it); had a call
    /// let through within one fast check before it, or since (a finished helper falls silent
    /// and drops out); and had not stopped to wait before it. Counting too few judges each at a
    /// higher pace, so the projection errs above: a helper silent for more than a minute in one
    /// long step is not counted, and its next call is judged as one picking up again. What it
    /// can still count too many of: a helper that finished in the minute before the reading, or
    /// a reading whose speed spans more than a minute and saw a run that began inside it, for
    /// only part of that span.
    fn measured(&self, checked_at: u64) -> usize {
        self.runs.values()
            .filter(|run| run.since.saturating_add(STAGGER_MS) <= checked_at)
            .filter(|run| run.last.saturating_add(STAGGER_MS) >= checked_at)
            .filter(|run| run.until.is_none_or(|until| until >= checked_at))
            .count()
    }
    /// Is `key`'s run still going: let through and not stopped to wait since (fifth review of
    /// 2026-10-29, P1)? A permit with no run kept (a ledger written before runs were) is not.
    fn going(&self, key: &str) -> bool {
        self.runs.get(key).is_some_and(|run| run.until.is_none())
    }
    /// A call on `key` was let through at `now`: its run goes on, or a new one begins.
    fn went(&mut self, key: &str, now: u64) {
        match self.runs.get_mut(key) {
            Some(run) if run.until.is_none() => run.last = run.last.max(now),
            _ => {
                self.runs.insert(key.into(), Run { since: now, last: now, until: None });
            }
        }
    }
    /// A call on `key` waits at `now`: its run stops there. Reports whether that changed.
    fn waited(&mut self, key: &str, now: u64) -> bool {
        match self.runs.get_mut(key) {
            Some(run) if run.until.is_none() => {
                run.until = Some(now);
                true
            }
            _ => false,
        }
    }
}

/// **Each account keeps its own count** (review of 2026-10-29, finding 3): with one shared
/// ledger, calls alternating between two accounts during a move each wiped the other's released
/// set and gap, and each account let a second worker go at once.
#[derive(Default, Serialize, Deserialize)]
struct Ledgers {
    #[serde(default)]
    accounts: std::collections::BTreeMap<String, Ledger>,
}

/// An exclusive lock on the ledger for as long as it lives (the descriptor's close releases it).
struct Locked(#[allow(dead_code)] fs::File);
fn lock(state: &Path) -> io::Result<Locked> {
    fs::create_dir_all(state)?;
    let file = fs::OpenOptions::new().create(true).truncate(false).write(true).open(state.join(LOCK_FILE))?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // A blocking wait: the ledger is read and written in microseconds.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(Locked(file))
}

/// **May the held call `key` on `account` go on now?** When it was released earlier in this
/// window, while [`Release::keeps`] its place among the released on this reading (the permit
/// stays either way); otherwise when it is released now: at least [`STAGGER_MS`] after the
/// previous release and only while [`Release::allows`] the one more. `team` gives, for the
/// released set, the helpers whose pace the speed before the hold measured (released or still
/// waiting).
pub fn admit(
    state: &Path,
    account: &str,
    key: &str,
    release: &Release,
    team: &dyn Fn(&[String]) -> usize,
    now: u64,
) -> io::Result<bool> {
    let _locked = lock(state)?;
    let path = state.join(LEDGER_FILE);
    let mut all = super::gate::read_json::<Ledgers>(&path).unwrap_or_default();
    let ledger = all.accounts.entry(account.into()).or_default();
    // A new window starts a new count; the same window read by the probe and by the stream,
    // a fraction of a second apart, keeps it (`super::same_reset`, finding 2).
    if !super::same_reset(ledger.resets_at, release.resets_at) {
        *ledger = Ledger { resets_at: release.resets_at, ..Default::default() };
    }
    // Only the helpers this reading measured divide its speed: not one waiting, finished, or
    // picked up again since (fourth review of 2026-10-29, P1; `Ledger::measured`).
    let running = ledger.measured(release.checked_at);
    // A speed with no helper seen running behind it may be one in a long step: nobody picks up
    // again or is released on it (fifth review of 2026-10-29, P1; module docs).
    let unattributed = running == 0 && release.speed > 0.;
    if let Some(place) = ledger.released.iter().position(|k| k == key) {
        let resumes = !ledger.going(key);
        let goes = !(unattributed && resumes) && release.keeps(place + 1, running, team(&ledger.released));
        if goes {
            ledger.went(key, now);
        } else if !ledger.waited(key, now) {
            return Ok(false);
        }
        super::atomic_write(&path, &all)?;
        return Ok(goes);
    }
    if ledger.last_at.is_some_and(|t| now < t.saturating_add(STAGGER_MS)) {
        return Ok(false);
    }
    let n = ledger.released.len() + 1;
    if unattributed || !release.allows(n, running, team(&ledger.released)) {
        return Ok(false);
    }
    ledger.released.push(key.into());
    ledger.last_at = Some(now);
    ledger.went(key, now);
    super::atomic_write(&path, &all)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::tests::Scratch;

    const MINUTE: u64 = 60_000;
    /// Points per minute as points per ms.
    fn per_minute(points: f64) -> f64 { points / MINUTE as f64 }

    /// **The frame math, shown.** 95% read now, the reset 19 minutes away, the team measured at
    /// 1.4 points a minute before the hold (80% -> 95% in 10 minutes, its first point unseen:
    /// 14 / 10), ten held: one is 0.14 a minute.
    /// - 1 released: 95 + 0.14 x 19 = 97.66, under 100: released.
    /// - 2 released: 95 + 0.28 x 19 = 100.32: not, at this reading.
    /// - 5 minutes before the reset, a reading of 97% with one running (0.14 a minute
    ///   measured): 2 is 97 + 0.28 x 5 = 98.4, 3 is 98.82... each judged at its own reading.
    ///
    /// So the stagger releases what the remaining window holds and stops before 100; the rest
    /// wait for the reset.
    #[test]
    fn the_projection_releases_only_what_stays_under_100_at_the_reset() {
        let now = 1_000_000_000;
        let held = Release { used: 95., checked_at: now, resets_at: now + 19 * MINUTE, speed: 0., pre_hold: per_minute(1.4) };
        assert!((held.projected(1, 0, 10) - 97.66).abs() < 1e-9, "{}", held.projected(1, 0, 10));
        assert!(held.allows(1, 0, 10));
        assert!((held.projected(2, 0, 10) - 100.32).abs() < 1e-9, "{}", held.projected(2, 0, 10));
        assert!(!held.allows(2, 0, 10), "the second would project past 100% at this reading");
        // A rate of 0 while everyone is held is no measurement: the old rule's `speed == 0`
        // would have let all ten go, and the pace before the hold says they reach 121.6%.
        assert!(held.projected(10, 0, 10) > 100.);

        let later = Release { used: 97., checked_at: now + 14 * MINUTE, resets_at: now + 19 * MINUTE,
            speed: per_minute(0.14), pre_hold: per_minute(1.4) };
        let fits: Vec<usize> = (1..=10).filter(|n| later.allows(*n, 1, 10)).collect();
        // n = 4: 97 + 0.56 x 5 = 99.8; n = 5: 97 + 0.7 x 5 = 100.5.
        assert_eq!(fits, vec![1, 2, 3, 4]);
    }

    /// **Second review of 2026-10-29.** Finding 1: one worker measured at 1.4 points a minute,
    /// with nine Agent dispatches queued behind it. The team whose pace was measured is that one
    /// worker, so it is judged at its whole pace: 95 + 1.4 x 19 = 121.6, it waits. Counting the
    /// nine dispatches in the team made it 95 + 0.14 x 19 = 97.66 and let it go. A dispatch
    /// released adds one share like anyone: ten measured, the eleventh release is
    /// 95 + 1.54 x 19 = 124.26. Finding 2: no speed before the hold and none since is no
    /// measurement, so nobody goes early; a speed measured since is one.
    #[test]
    fn only_the_measured_team_divides_the_pace_and_no_measurement_releases_nobody() {
        let now = 1_000_000_000;
        let held = Release { used: 95., checked_at: now, resets_at: now + 19 * MINUTE, speed: 0., pre_hold: per_minute(1.4) };
        assert!((held.projected(1, 0, 1) - 121.6).abs() < 1e-9, "{}", held.projected(1, 0, 1));
        assert!(!held.allows(1, 0, 1), "one measured worker at its whole pace passes 100%");
        assert!((held.projected(11, 0, 10) - 124.26).abs() < 1e-9, "{}", held.projected(11, 0, 10));
        let unseen = Release { pre_hold: 0., ..held };
        assert!(!unseen.allows(1, 0, 10), "a rate of 0 while everyone is held is no measurement");
        assert!(Release { speed: per_minute(0.1), ..unseen }.allows(1, 0, 10), "a speed measured since is one: 96.9");
    }

    /// **The ledger: one at a time, no faster than one a minute, never past the projection, and
    /// a released call stays released.** Ten held on account 2. At the first moment one goes;
    /// the next nine are refused by the gap; a minute later the second is refused by the
    /// projection; a fresh reading 5 minutes before the reset lets three more go, one a minute,
    /// and then the projection stops it. Whoever was released is still released on its next call.
    #[test]
    fn the_ledger_releases_one_a_minute_until_the_projection_stops_it() {
        let root = Scratch::new();
        let state = root.path().join("engine-state");
        let now = 1_000_000_000;
        let reading = Release { used: 95., checked_at: now, resets_at: now + 19 * MINUTE, speed: 0., pre_hold: per_minute(1.4) };
        let ten: Vec<String> = (0..10).map(|i| format!("worker-{i}")).collect();
        // The ten whose pace was measured, released or still waiting.
        let team = |_: &[String]| ten.len();
        let go = |key: &str, reading: &Release, at: u64| admit(&state, "2", key, reading, &team, at).unwrap();

        let first: Vec<&String> = ten.iter().filter(|k| go(k, &reading, now)).collect();
        assert_eq!(first, vec![&ten[0]], "one at a time, not all at once");
        assert!(go(&ten[0], &reading, now + 1), "a released worker's next call goes on");
        assert!(!go(&ten[1], &reading, now + MINUTE), "the second would pass 100% at this reading");

        let fresh = Release { used: 97., checked_at: now + 14 * MINUTE, resets_at: now + 19 * MINUTE,
            speed: per_minute(0.14), pre_hold: per_minute(1.4) };
        let mut released = 1;
        for minute in 0..5 {
            let at = now + (14 + minute) * MINUTE;
            let went = ten.iter().filter(|k| go(k, &fresh, at)).count();
            released = went;
            assert!(fresh.allows(went, 1, 10), "never past the projected 100%: {went} released at minute {minute}");
        }
        assert_eq!(released, 4, "the stagger stops before 100%; the other six wait for the reset");
        // The next window (five hours on, read at 1% once it began) starts its own count:
        // 1 + 0.14 x 300 = 43, under 100.
        let next = Release { used: 1., checked_at: now + 19 * MINUTE, resets_at: now + 319 * MINUTE, ..reading };
        assert!(go(&ten[9], &next, now + 19 * MINUTE));
    }

    /// **Review of 2026-10-29, finding 2: the probe and the stream read one window.** The probe
    /// reports the reset as an ISO time and the stream in whole seconds, so the same window comes
    /// with reset times 500 ms apart. Before, the ledger matched them exactly, started the count
    /// over and let a second worker go in the same instant, past the projected 100% (95 + 0.28
    /// x 19 = 100.32). The same window keeps its count; a new one, five hours on, starts its own.
    #[test]
    fn the_same_window_read_by_the_probe_and_the_stream_keeps_its_count() {
        let root = Scratch::new();
        let state = root.path().join("engine-state");
        let now = 1_000_000_000;
        let probe = Release { used: 95., checked_at: now, resets_at: now + 19 * MINUTE, speed: 0., pre_hold: per_minute(1.4) };
        let stream = Release { resets_at: probe.resets_at + 500, ..probe };
        let team = |_: &[String]| 10;
        let go = |key: &str, reading: &Release, at: u64| admit(&state, "2", key, reading, &team, at).unwrap();
        assert!(go("worker-0", &probe, now));
        assert!(!go("worker-1", &stream, now), "the same instant: the gap holds");
        assert!(!go("worker-1", &stream, now + MINUTE), "a minute on: 100.32% projected holds");
        assert!(go("worker-0", &stream, now + MINUTE), "the released worker carries on");
    }

    /// **Review of 2026-10-29, finding 3: each account keeps its own count.** During a move,
    /// calls alternate between Home (1) and Work (2), both held inside their windows. Before,
    /// one shared ledger was wiped by every change of account and all four calls in the same
    /// instant went (two on each). Now each account releases its first, and the second on each
    /// waits for the gap.
    #[test]
    fn alternating_accounts_each_keep_their_own_released_set_and_gap() {
        let root = Scratch::new();
        let state = root.path().join("engine-state");
        let now = 1_000_000_000;
        let reading = Release { used: 95., checked_at: now, resets_at: now + 19 * MINUTE, speed: 0., pre_hold: per_minute(1.4) };
        let team = |_: &[String]| 10;
        let go = |account: &str, key: &str| admit(&state, account, key, &reading, &team, now).unwrap();
        let went = [("1", "home-0"), ("2", "work-0"), ("1", "home-1"), ("2", "work-1")]
            .iter().filter(|&&(account, key)| go(account, key)).count();
        assert_eq!(went, 2, "one on each account, not two on each");
        assert!(go("1", "home-0") && go("2", "work-0"), "each released worker carries on");
    }

    /// **Third review of 2026-10-29, P1: a permit is judged again on every reading.** One of
    /// ten released at 95% (95 + 0.14 x 19 = 97.66). Nine minutes on, a fresh reading of 99%
    /// with ten minutes left, measured at 1 point a minute with that one running: carrying on
    /// projects 99 + max(0.14, 1) x 10 = 109, so the released helper waits (before: it went
    /// on, "released helper admitted=true"), and so does a second (99 + 2 x 10 = 119). At 100%
    /// both wait. The permit stays: nine minutes later, a reading of 99.5% with one minute left
    /// and nobody running (the speed before the hold, 0.14 a minute for one) projects
    /// 99.5 + 0.14 x 1 = 99.64, and the same helper carries on, on its first permit, with no
    /// new release written.
    #[test]
    fn a_released_helper_waits_when_a_fresh_reading_would_pass_100_and_keeps_its_permit() {
        let root = Scratch::new();
        let state = root.path().join("engine-state");
        let now = 1_000_000_000;
        let reset = now + 19 * MINUTE;
        let start = Release { used: 95., checked_at: now, resets_at: reset, speed: 0., pre_hold: per_minute(1.4) };
        let team = |_: &[String]| 10;
        let go = |key: &str, reading: &Release, at: u64| admit(&state, "2", key, reading, &team, at).unwrap();
        assert!(go("worker-0", &start, now), "the first is released at 97.66% projected");

        let at = now + 9 * MINUTE;
        let fast = Release { used: 99., checked_at: at, resets_at: reset, speed: per_minute(1.), pre_hold: per_minute(1.4) };
        assert!((fast.used + per_minute(1.) * (reset - at) as f64 - 109.).abs() < 1e-9);
        assert!(!go("worker-0", &fast, at), "109% projected: the released helper waits");
        assert!(!go("worker-1", &fast, at), "119% projected: nobody new goes");
        let full = Release { used: 100., ..fast };
        assert!(!go("worker-0", &full, at + 1), "at 100% the released helper waits");

        let late = now + 18 * MINUTE;
        let calm = Release { used: 99.5, checked_at: late, resets_at: reset, speed: 0., pre_hold: per_minute(1.4) };
        assert!(go("worker-0", &calm, late), "99.64% projected: it carries on, on the same permit");
        let ledger: Ledgers = crate::quota::gate::read_json(&state.join(LEDGER_FILE)).unwrap();
        assert_eq!(ledger.accounts["2"].released, vec!["worker-0".to_string()], "no new release was written");
        assert_eq!(ledger.accounts["2"].last_at, Some(now), "the gap still counts from the one release");
    }

    /// **The latest permits pause first.** Three of ten released, measured together at 0.42 a
    /// minute (0.14 each), 98% read with five minutes left: the first carries on at
    /// 98 + 0.14 x 5 = 98.7, the second at 98 + 0.28 x 5 = 99.4, the third would make it
    /// 98 + 0.42 x 5 = 100.1 and waits.
    #[test]
    fn the_latest_permits_wait_first_when_a_reading_fits_fewer() {
        let now = 1_000_000_000;
        let reading = Release { used: 98., checked_at: now, resets_at: now + 5 * MINUTE, speed: per_minute(0.42), pre_hold: per_minute(1.4) };
        let carry: Vec<bool> = (1..=3).map(|k| reading.keeps(k, 3, 10)).collect();
        assert_eq!(carry, vec![true, true, false]);
        assert!(!Release { pre_hold: 0., speed: 0., ..reading }.keeps(1, 3, 10), "no measurement keeps nobody going");
    }

    /// Three of ten released on account 2, one a minute from 95% with 19 minutes left (0.06 a
    /// minute each before the hold: 95 + 0.18 x 19 = 98.42 for all three), all at work at 13.5.
    fn three_released(state: &Path, reading: &Release, now: u64) -> [&'static str; 3] {
        let three = ["worker-0", "worker-1", "worker-2"];
        let team = |_: &[String]| 10;
        for (i, key) in three.iter().enumerate() {
            assert!(admit(state, "2", key, reading, &team, now + i as u64 * MINUTE).unwrap(), "{key} is released");
        }
        three
    }

    /// **Fourth review of 2026-10-29 (rv-20261029T165127Z-b54baa2c-6070), P1: a permit that
    /// waits is not measured.** Three released, a fresh reading of 96% five minutes before the
    /// reset measures the three at 1.2 a minute, 0.4 each: the first goes on at 96 + 0.4 x 5 =
    /// 98, the second would make it 96 + 0.8 x 5 = 100 and the third 102, so those two wait. A
    /// minute on, 96.4% measures the one still going at 0.4 a minute: with four minutes left the
    /// first is 96.4 + 0.4 x 4 = 98, the second picks up again at 96.4 + 0.8 x 4 = 99.6, and the
    /// third would make it 96.4 + 1.2 x 4 = 101.2 and waits. Before, the whole released set
    /// divided that 0.4, and all three went on: "permits=[true, true, true]; three-worker
    /// projection=101.20%".
    #[test]
    fn a_waiting_permit_does_not_divide_the_speed_the_running_one_is_measured_at() {
        let root = Scratch::new();
        let state = root.path().join("engine-state");
        let now = 1_000_000_000;
        let reset = now + 19 * MINUTE;
        let start = Release { used: 95., checked_at: now, resets_at: reset, speed: 0., pre_hold: per_minute(0.6) };
        let team = |_: &[String]| 10;
        let go = |key: &str, reading: &Release, at: u64| admit(&state, "2", key, reading, &team, at).unwrap();
        let three = three_released(&state, &start, now);
        for key in three { assert!(go(key, &start, now + 13 * MINUTE + 30_000), "{key} at work"); }

        let fast = Release { used: 96., checked_at: now + 14 * MINUTE, resets_at: reset, speed: per_minute(1.2), pre_hold: per_minute(0.6) };
        let before: Vec<bool> = three.iter().map(|key| go(key, &fast, now + 14 * MINUTE + 1_000)).collect();
        assert_eq!(before, vec![true, false, false], "98, 100, 102: the latest two wait");
        assert!(go("worker-0", &fast, now + 14 * MINUTE + 30_000), "the first carries on");

        let later = Release { used: 96.4, checked_at: now + 15 * MINUTE, speed: per_minute(0.4), ..fast };
        let after: Vec<bool> = three.iter().map(|key| go(key, &later, now + 15 * MINUTE + 1_000)).collect();
        assert_eq!(after, vec![true, true, false], "98, 99.6, 101.2: the third still waits");
        let going = after.iter().filter(|g| **g).count() as f64;
        let projected = later.used + per_minute(0.4) * going * (reset - later.checked_at) as f64;
        assert!((projected - 99.6).abs() < 1e-9 && projected < 100., "{projected}");
    }

    /// **Fourth review of 2026-10-29, P1: a finished helper is not measured.** Three released,
    /// two finish (their last calls at minute 5), the third carries on alone. A fresh reading of
    /// 96% five minutes before the reset measures it at 1.2 a minute: going on alone makes it
    /// 96 + 1.2 x 5 = 102, so it waits. Before, the three permits divided the 1.2 and it went on
    /// at 98% projected: "remaining first permit admitted=true; actual one-worker
    /// projection=102.00%". A minute later, 96.2% measures 0.2 a minute with nobody seen
    /// running: whose that is cannot be told (a helper in a long step looks the same), so it
    /// does not pick up again on it and waits for a reading that measures a helper, or for the
    /// reset (fifth review of 2026-10-29, P1). Before that review it went on here, judged at
    /// 96.2 + (0.2 + 0.06) x 4 = 97.24.
    #[test]
    fn finished_helpers_do_not_divide_the_speed_of_the_one_still_running() {
        let root = Scratch::new();
        let state = root.path().join("engine-state");
        let now = 1_000_000_000;
        let reset = now + 19 * MINUTE;
        let start = Release { used: 95., checked_at: now, resets_at: reset, speed: 0., pre_hold: per_minute(0.6) };
        let team = |_: &[String]| 10;
        let go = |key: &str, reading: &Release, at: u64| admit(&state, "2", key, reading, &team, at).unwrap();
        let three = three_released(&state, &start, now);
        for key in three { assert!(go(key, &start, now + 5 * MINUTE), "{key} at work"); }
        assert!(go("worker-0", &start, now + 13 * MINUTE + 30_000), "the first carries on alone");

        let fast = Release { used: 96., checked_at: now + 14 * MINUTE, resets_at: reset, speed: per_minute(1.2), pre_hold: per_minute(0.6) };
        assert!((fast.used + fast.speed * (reset - fast.checked_at) as f64 - 102.).abs() < 1e-9);
        assert!(!go("worker-0", &fast, now + 14 * MINUTE + 1_000), "102% projected alone: it waits");

        let calm = Release { used: 96.2, checked_at: now + 15 * MINUTE, speed: per_minute(0.2), ..fast };
        assert!(!go("worker-0", &calm, now + 15 * MINUTE + 1_000), "a speed nobody running accounts for: it keeps waiting");
    }

    /// **Fifth review of 2026-10-29 (rv-20261029T172038Z-eb39280f-2916), P1: a speed no helper
    /// seen running accounts for resumes nobody.** Three released from 95% with 19 minutes left
    /// (0.06 a minute each before the hold), all at work at 15.5. At 16, 96% measures the three
    /// at 3 a minute, 1 each: the first goes on at 96 + 1 x 3 = 99, the other two wait (102,
    /// 105). The first then spends 61 s in one tool step, so the next reading, 97% at 17:02
    /// with 1.9667 minutes left, measures it at 1 a minute but sees nobody running (its last
    /// call was 61 s before). Before, that speed was read as nobody's and each picking up again
    /// added only its 0.06 share: retried first-to-last only the first went on, but retried
    /// last-to-first all three did, "order=[2, 1, 0]; permits=[true, true, true]; actual
    /// projection=102.90%" (97 + 3 x 1.9667). Now those waiting keep waiting on it; the one
    /// still going is judged as before, 97 + (1 + 0.06) x 1.9667 = 99.08, and goes on. In
    /// either order: 97 + 1 x 1.9667 = 98.97.
    #[test]
    fn a_speed_no_running_helper_accounts_for_resumes_nobody_in_any_order() {
        let now = 1_000_000_000;
        let reset = now + 19 * MINUTE;
        let start = Release { used: 95., checked_at: now, resets_at: reset, speed: 0., pre_hold: per_minute(0.6) };
        for order in [[0, 1, 2], [2, 1, 0], [1, 2, 0], [2, 0, 1]] {
            let root = Scratch::new();
            let state = root.path().join("engine-state");
            let team = |_: &[String]| 10;
            let go = |key: &str, reading: &Release, at: u64| admit(&state, "2", key, reading, &team, at).unwrap();
            let three = three_released(&state, &start, now);
            for key in three { assert!(go(key, &start, now + 15 * MINUTE + 30_000), "{key} at work"); }

            let fast = Release { used: 96., checked_at: now + 16 * MINUTE, speed: per_minute(3.), ..start };
            let before: Vec<bool> = three.iter().map(|key| go(key, &fast, now + 16 * MINUTE + 1_000)).collect();
            assert_eq!(before, vec![true, false, false], "99, 102, 105: the latest two wait");

            let later = Release { used: 97., checked_at: now + 17 * MINUTE + 2_000, speed: per_minute(1.), ..start };
            let mut after = [false; 3];
            for i in order { after[i] = go(three[i], &later, now + 17 * MINUTE + 3_000); }
            assert_eq!(after, [true, false, false], "order {order:?}: only the one still going goes on");
            let going = after.iter().filter(|g| **g).count() as f64;
            let projected = later.used + later.speed * going * (reset - later.checked_at) as f64;
            assert!(projected < 100., "order {order:?}: {projected}");
        }
    }
}
