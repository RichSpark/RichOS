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
//! whole held team's pace, so one released call is judged at `pre_hold / team`. With `n`
//! released, the rate is the larger of their share, `pre_hold x n / team`, and what the
//! readings since have actually measured (`speed`, scaled up by the one more being released).
//! The projection at the reset is `used + rate x (resets_at - checked_at)`: counted from the
//! reading, so what the released calls spent since it is counted too. One more is released only
//! while that stays under 100, and never sooner than [`STAGGER_MS`] after the previous one.
//! Whoever is not released waits for the reset, when the window's figures stop holding.
//!
//! **Several gate processes decide together**: each held tool call is its own process, so the
//! released set lives in one small file under `engine-state`, read and written under an
//! exclusive lock. A call already released stays released for the rest of that window: its
//! next tool call is the same worker carrying on, not a new one.
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
    /// Points per ms with `n` calls released out of a held team of `team`.
    pub fn rate(&self, n: usize, team: usize) -> f64 {
        let whole = team.max(n).max(1) as f64;
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
    /// May the `n`th call be released? Only while the account stays under 100% at the reset.
    pub fn allows(&self, n: usize, team: usize) -> bool {
        self.projected(n, team) < 100.
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ledger {
    account: String,
    resets_at: u64,
    released: Vec<String>,
    last_at: Option<u64>,
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

/// **May the held call `key` on `account` go on now?** `true` when it was released earlier in
/// this window, or when it is released now: at least [`STAGGER_MS`] after the previous release
/// and only while [`Release::allows`] the one more. `held` counts the calls still waiting on
/// `account` that are not in the released set it is given; the team is those plus the released.
pub fn admit(
    state: &Path,
    account: &str,
    key: &str,
    release: &Release,
    held: &dyn Fn(&[String]) -> usize,
    now: u64,
) -> io::Result<bool> {
    let _locked = lock(state)?;
    let path = state.join(LEDGER_FILE);
    let mut ledger = super::gate::read_json::<Ledger>(&path).ok()
        .filter(|l| l.account == account && l.resets_at == release.resets_at)
        .unwrap_or_else(|| Ledger { account: account.into(), resets_at: release.resets_at, ..Default::default() });
    if ledger.released.iter().any(|k| k == key) {
        return Ok(true);
    }
    if ledger.last_at.is_some_and(|t| now < t.saturating_add(STAGGER_MS)) {
        return Ok(false);
    }
    let n = ledger.released.len() + 1;
    let team = ledger.released.len() + held(&ledger.released).max(1);
    if !release.allows(n, team) {
        return Ok(false);
    }
    ledger.released.push(key.into());
    ledger.last_at = Some(now);
    super::atomic_write(&path, &ledger)?;
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
        let held = |released: &[String]| ten.iter().filter(|k| !released.contains(k)).count();
        let go = |key: &str, reading: &Release, at: u64| admit(&state, "2", key, reading, &held, at).unwrap();

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
        // Another account, or the next window, starts its own count.
        assert!(go(&ten[9], &Release { resets_at: now + 20 * MINUTE, ..reading }, now + 19 * MINUTE));
    }
}
