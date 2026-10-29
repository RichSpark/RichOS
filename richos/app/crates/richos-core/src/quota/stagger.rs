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
//! scaled up by the one more being released). No speed before the hold and none since is no
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
//! the hold, `pre_hold x k / team`, and their share of what the readings measure now, `speed x k
//! / n` (`n` released). The `k`th permit carries on only while that projection stays under 100;
//! the latest permits pause first, and pick up again on a later reading that fits, on the same
//! permit. Before, a permit returned at once: a helper released at 95% kept going on a fresh
//! 99% reading with ten minutes left at 1 point a minute (110.4% projected), and at 100% too.
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
    /// Points per ms with `n` calls released, `team` being the helpers whose pace `pre_hold`
    /// measured. Each release adds one helper's share, `pre_hold / team`; a new agent's dispatch
    /// never ran before the hold, so it is not in `team`, and it adds a share all the same
    /// (second review of 2026-10-29, finding 1: counting queued dispatches in `team` made one
    /// measured worker's share a tenth of its real pace).
    pub fn rate(&self, n: usize, team: usize) -> f64 {
        let whole = team.max(1) as f64;
        let share = self.pre_hold * n as f64 / whole;
        // What the readings measure now: with nobody released it is only what was never held
        // (Stu's own turns), and the first release adds one share to it; with `n - 1` released
        // it is their pace, and one more is `n / (n - 1)` of it.
        let measured = match n {
            0 => self.speed,
            1 => self.speed + self.pre_hold / whole,
            n => self.speed * n as f64 / (n - 1) as f64,
        };
        share.max(measured)
    }
    /// The used percent the window reaches at its reset with `n` of `team` released.
    pub fn projected(&self, n: usize, team: usize) -> f64 {
        self.used + self.rate(n, team) * self.resets_at.saturating_sub(self.checked_at) as f64
    }
    /// May the `n`th call be released? Only while the account stays under 100% at the reset,
    /// and only on a measurement: with no speed before the hold and none since (the climb was
    /// not seen, as after a restart in the middle of a hold), a rate of 0 is no measurement, and
    /// everyone waits for the reset (second review of 2026-10-29, finding 2).
    pub fn allows(&self, n: usize, team: usize) -> bool {
        self.measured() && self.projected(n, team) < 100.
    }
    /// May the `k`th of `n` calls already released carry on, on this reading? The readings
    /// since the releases measure the `n` running, so the first `k` are judged at `k / n` of
    /// what they measure, and never below their share of the pace before the hold: the
    /// projection is `used + max(pre_hold x k / team, speed x k / n) x (resets_at - checked_at)`,
    /// under 100 and on a measurement, as for a new release (third review of 2026-10-29, P1).
    pub fn keeps(&self, k: usize, n: usize, team: usize) -> bool {
        let share = self.pre_hold * k as f64 / team.max(1) as f64;
        let measured = self.speed * k as f64 / n.max(k).max(1) as f64;
        let left = self.resets_at.saturating_sub(self.checked_at) as f64;
        self.measured() && self.used + share.max(measured) * left < 100.
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
    if let Some(place) = ledger.released.iter().position(|k| k == key) {
        let n = ledger.released.len();
        return Ok(release.keeps(place + 1, n, team(&ledger.released)));
    }
    if ledger.last_at.is_some_and(|t| now < t.saturating_add(STAGGER_MS)) {
        return Ok(false);
    }
    let n = ledger.released.len() + 1;
    if !release.allows(n, team(&ledger.released)) {
        return Ok(false);
    }
    ledger.released.push(key.into());
    ledger.last_at = Some(now);
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
        assert!((held.projected(1, 10) - 97.66).abs() < 1e-9, "{}", held.projected(1, 10));
        assert!(held.allows(1, 10));
        assert!((held.projected(2, 10) - 100.32).abs() < 1e-9, "{}", held.projected(2, 10));
        assert!(!held.allows(2, 10), "the second would project past 100% at this reading");
        // A rate of 0 while everyone is held is no measurement: the old rule's `speed == 0`
        // would have let all ten go, and the pace before the hold says they reach 121.6%.
        assert!(held.projected(10, 10) > 100.);

        let later = Release { used: 97., checked_at: now + 14 * MINUTE, resets_at: now + 19 * MINUTE,
            speed: per_minute(0.14), pre_hold: per_minute(1.4) };
        let fits: Vec<usize> = (1..=10).filter(|n| later.allows(*n, 10)).collect();
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
        assert!((held.projected(1, 1) - 121.6).abs() < 1e-9, "{}", held.projected(1, 1));
        assert!(!held.allows(1, 1), "one measured worker at its whole pace passes 100%");
        assert!((held.projected(11, 10) - 124.26).abs() < 1e-9, "{}", held.projected(11, 10));
        let unseen = Release { pre_hold: 0., ..held };
        assert!(!unseen.allows(1, 10), "a rate of 0 while everyone is held is no measurement");
        assert!(Release { speed: per_minute(0.1), ..unseen }.allows(1, 10), "a speed measured since is one: 96.9");
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
            assert!(fresh.allows(went, 10), "never past the projected 100%: {went} released at minute {minute}");
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
}
