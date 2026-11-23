//! One transaction publishes a minute and its hourly rollup. No per-PID rows survive a sample.
use super::{mac, Current, Growth, Health, Snapshot, Trend, KINDS};
use rusqlite::{
    params, Connection, Error, ErrorCode, OpenFlags, OptionalExtension, TransactionBehavior,
};
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

const MINUTE_MS: i64 = 60_000;
const MINUTES_PER_HOUR: i64 = 60;
const RAW_MINUTES: i64 = 48 * MINUTES_PER_HOUR;
const LONG_HOURS: i64 = 90 * 24;
const STALE_MS: i64 = 2 * MINUTE_MS + MINUTE_MS / 2;
const LEAK_HOURS: usize = 4;
const LEAK_MINUTES: i64 = LEAK_HOURS as i64 * MINUTES_PER_HOUR;
const MIB: i64 = 1024 * 1024;

// SQLite reuses freed pages. At most 2 MiB of main-file pages and one rollback journal
// of the original pages (plus sector headers). No WAL or unbounded sidecar.
const PAGE_BYTES: i64 = 4096;
const MAX_PAGES: i64 = 512;
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS latest (
        id INTEGER PRIMARY KEY CHECK(id = 1),
        at_ms INTEGER NOT NULL, ok INTEGER NOT NULL, scanned INTEGER NOT NULL,
        matched INTEGER NOT NULL, unreadable INTEGER NOT NULL, capture_us INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS minutes (
        minute INTEGER NOT NULL, kind INTEGER NOT NULL, bytes INTEGER,
        members INTEGER NOT NULL, identity INTEGER NOT NULL,
        PRIMARY KEY (minute, kind)
    ) WITHOUT ROWID;
    CREATE TABLE IF NOT EXISTS hours (
        hour INTEGER NOT NULL, kind INTEGER NOT NULL, samples INTEGER NOT NULL,
        minimum INTEGER NOT NULL, total INTEGER NOT NULL, maximum INTEGER NOT NULL,
        PRIMARY KEY (hour, kind)
    ) WITHOUT ROWID;
";

pub struct Recorder(Connection);

impl Recorder {
    pub fn open(state_root: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(state_root).map_err(|error| error.to_string())?;
        let path = super::path(state_root);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
        Self::open_file(&path, MAX_PAGES).map_err(|error| error.to_string())
    }

    fn open_file(path: &Path, pages: i64) -> rusqlite::Result<Self> {
        let db = Connection::open_with_flags(
            path,
            OpenFlags::default() | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.busy_timeout(Duration::from_millis(250))?;
        db.pragma_update(None, "page_size", PAGE_BYTES)?;
        db.pragma_update(None, "journal_mode", "DELETE")?;
        db.pragma_update(None, "temp_store", "MEMORY")?;
        db.pragma_update(None, "max_page_count", pages)?;
        db.execute_batch(SCHEMA)?;
        Ok(Self(db))
    }

    /// Do one capture with no subprocesses. A failed inventory still publishes its outcome,
    /// without inventing fresh readings for the processes that could not be inspected.
    pub fn tick(&mut self) -> Result<(), String> {
        let at_ms = crate::util::now_millis() as i64;
        let start = Instant::now();
        let capture = mac::capture();
        let capture_us = start.elapsed().as_micros() as i64;
        self.record(at_ms, capture.as_ref().ok(), capture_us)
            .map_err(|error| error.to_string())
    }

    fn record(
        &mut self,
        at_ms: i64,
        capture: Option<&mac::Capture>,
        capture_us: i64,
    ) -> rusqlite::Result<()> {
        loop {
            match self.write_minute(at_ms, capture, capture_us) {
                Err(Error::SqliteFailure(code, _)) if code.code == ErrorCode::DiskFull => {
                    if !self.trim_oldest()? {
                        return Err(Error::SqliteFailure(code, None));
                    }
                }
                result => return result,
            }
        }
    }

    fn write_minute(
        &mut self,
        at_ms: i64,
        capture: Option<&mac::Capture>,
        capture_us: i64,
    ) -> rusqlite::Result<()> {
        let minute = at_ms / MINUTE_MS;
        let hour = minute / MINUTES_PER_HOUR;
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if latest(&tx)?.is_some_and(|row| row.at_ms / MINUTE_MS >= minute) {
            // A second Doctor or a wall-clock adjustment must not double-count an hour.
            return Ok(());
        }
        tx.execute(
            "DELETE FROM minutes WHERE minute <= ?1",
            [minute - RAW_MINUTES],
        )?;
        tx.execute("DELETE FROM hours WHERE hour <= ?1", [hour - LONG_HOURS])?;
        let (mut scanned, mut matched, mut unreadable) = (0, 0, 0);
        if let Some(capture) = capture {
            scanned = capture.process_count;
            unreadable = capture.unreadable;
            let mut raw = tx.prepare_cached("INSERT INTO minutes VALUES (?1, ?2, ?3, ?4, ?5)")?;
            let mut rollup = tx.prepare_cached(
                "INSERT INTO hours VALUES (?1, ?2, 1, ?3, ?3, ?3)
                 ON CONFLICT(hour, kind) DO UPDATE SET
                    samples = samples + 1, minimum = MIN(minimum, excluded.minimum),
                    total = total + excluded.total, maximum = MAX(maximum, excluded.maximum)",
            )?;
            for (kind, reading) in capture.readings.iter().enumerate() {
                if reading.count == 0 {
                    continue;
                }
                matched += reading.count;
                raw.execute(params![
                    minute,
                    kind,
                    reading.bytes,
                    reading.count,
                    reading.identity
                ])?;
                if let Some(bytes) = reading.bytes {
                    rollup.execute(params![hour, kind, bytes])?;
                }
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO latest VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                at_ms,
                capture.is_some(),
                scanned,
                matched,
                unreadable,
                capture_us
            ],
        )?;
        tx.commit()
    }

    fn trim_oldest(&mut self) -> rusqlite::Result<bool> {
        // A rolled-up hour older than all raw samples is the first expendable data.
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let removed = tx.execute(
            "DELETE FROM hours WHERE hour < (
                SELECT MIN(hour) + 24 FROM hours
             ) AND hour < COALESCE((SELECT MIN(minute) / 60 FROM minutes), 9223372036854775807)",
            [],
        )?;
        let removed = if removed > 0 {
            removed
        } else {
            tx.execute(
                "DELETE FROM minutes WHERE minute < (SELECT MIN(minute) + 60 FROM minutes)",
                [],
            )?
        };
        tx.commit()?;
        Ok(removed > 0)
    }
}

struct Latest {
    at_ms: i64,
    ok: bool,
    scanned: i64,
    matched: i64,
    unreadable: i64,
    capture_us: i64,
}

impl Latest {
    fn status(&self, now_ms: i64) -> &'static str {
        if !(0..=STALE_MS).contains(&(now_ms - self.at_ms)) {
            "stale"
        } else if !self.ok {
            "inventory_failed"
        } else if self.unreadable > 0 {
            "partial"
        } else {
            "available"
        }
    }
}

fn latest(db: &Connection) -> rusqlite::Result<Option<Latest>> {
    db.query_row(
        "SELECT at_ms, ok, scanned, matched, unreadable, capture_us FROM latest WHERE id = 1",
        [],
        |r| {
            Ok(Latest {
                at_ms: r.get(0)?,
                ok: r.get(1)?,
                scanned: r.get(2)?,
                matched: r.get(3)?,
                unreadable: r.get(4)?,
                capture_us: r.get(5)?,
            })
        },
    )
    .optional()
}

fn reader(state_root: &Path) -> rusqlite::Result<Connection> {
    let db = Connection::open_with_flags(
        super::path(state_root),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    db.busy_timeout(Duration::from_millis(250))?;
    db.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(db)
}

pub(super) fn read(state_root: &Path, now_ms: i64) -> Snapshot {
    let mut db = match reader(state_root) {
        Ok(db) => db,
        Err(_) => return Snapshot::new("unavailable"),
    };
    snapshot(&mut db, now_ms).unwrap_or_else(|_| Snapshot::new("unreadable"))
}

pub(super) fn health(state_root: &Path) -> Health {
    let Ok(db) = reader(state_root) else {
        return Health::unavailable("unavailable");
    };
    match latest(&db) {
        Ok(Some(row)) => Health {
            status: row.status(crate::util::now_millis() as i64),
            sampled_at_ms: Some(row.at_ms),
            same_user_processes: row.scanned,
            matched_processes: row.matched,
            unreadable_processes: row.unreadable,
            capture_us: row.capture_us,
        },
        Ok(None) => Health::unavailable("unavailable"),
        Err(_) => Health::unavailable("unreadable"),
    }
}

fn snapshot(db: &mut Connection, now_ms: i64) -> rusqlite::Result<Snapshot> {
    let tx = db.transaction()?;
    let Some(latest) = latest(&tx)? else {
        return Ok(Snapshot::new("unavailable"));
    };
    let age_ms = now_ms - latest.at_ms;
    let status = latest.status(now_ms);
    let mut result = Snapshot::new(status);
    result.sampled_at_ms = Some(latest.at_ms);
    result.age_ms = (age_ms >= 0).then_some(age_ms);
    let minute = now_ms / MINUTE_MS;
    let hour = minute / MINUTES_PER_HOUR;
    let first_48 = trends(
        &tx,
        "SELECT kind, minute / 60, COUNT(bytes), MIN(bytes), SUM(bytes), MAX(bytes)
           FROM minutes WHERE minute > ?1 AND minute <= ?2 AND bytes IS NOT NULL
          GROUP BY kind, minute / 60 ORDER BY kind, minute / 60",
        minute - RAW_MINUTES,
        minute,
    )?;
    let first_90 = trends(
        &tx,
        "SELECT kind, hour, samples, minimum, total, maximum
           FROM hours WHERE hour > ?1 AND hour <= ?2 ORDER BY hour",
        hour - LONG_HOURS,
        hour,
    )?;
    for ((app, short), long) in result.apps.iter_mut().zip(first_48).zip(first_90) {
        app.last_48_hours = short;
        app.last_90_days = long;
    }

    if matches!(status, "available" | "partial") {
        let sample_minute = latest.at_ms / MINUTE_MS;
        let current = current_readings(&tx, sample_minute)?;
        let growth = growth_flags(&tx, sample_minute, &current)?;
        for ((app, reading), growth) in result.apps.iter_mut().zip(current).zip(growth) {
            app.current = Some(Current {
                status: match (reading.count, reading.bytes) {
                    (0, _) => "not_running",
                    (_, None) => "unreadable",
                    _ => "measured",
                },
                bytes: reading.bytes,
                process_count: reading.count,
            });
            app.sustained_growth = growth;
        }
    }
    Ok(result)
}

fn trends(
    db: &Connection,
    sql: &str,
    after: i64,
    through: i64,
) -> rusqlite::Result<[Option<Trend>; KINDS.len()]> {
    let mut result: [Option<Trend>; KINDS.len()] = std::array::from_fn(|_| None);
    let mut totals = [0_i64; KINDS.len()];
    let mut query = db.prepare(sql)?;
    let mut rows = query.query([after, through])?;
    while let Some(row) = rows.next()? {
        let kind = row.get::<_, usize>(0)?;
        let (hour, samples, min, total, max): (i64, i64, i64, i64, i64) = (
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
        );
        let Some(entry) = result.get_mut(kind) else {
            continue;
        };
        let trend = entry.get_or_insert(Trend {
            first_hour_start_ms: hour * MINUTES_PER_HOUR * MINUTE_MS,
            last_hour_start_ms: 0,
            observed_hours: 0,
            samples: 0,
            min_bytes: min,
            mean_bytes: 0,
            max_bytes: max,
            first_hour_mean_bytes: total / samples,
            last_hour_mean_bytes: 0,
        });
        trend.last_hour_start_ms = hour * MINUTES_PER_HOUR * MINUTE_MS;
        trend.last_hour_mean_bytes = total / samples;
        trend.observed_hours += 1;
        trend.samples += samples;
        trend.min_bytes = trend.min_bytes.min(min);
        trend.max_bytes = trend.max_bytes.max(max);
        totals[kind] += total;
    }
    for (trend, total) in result.iter_mut().zip(totals) {
        if let Some(trend) = trend {
            trend.mean_bytes = total / trend.samples;
        }
    }
    Ok(result)
}

fn current_readings(db: &Connection, minute: i64) -> rusqlite::Result<[mac::Reading; KINDS.len()]> {
    let mut readings = [mac::Reading::default(); KINDS.len()];
    let mut query =
        db.prepare("SELECT kind, bytes, members, identity FROM minutes WHERE minute = ?1")?;
    let mut rows = query.query([minute])?;
    while let Some(row) = rows.next()? {
        if let Some(reading) = readings.get_mut(row.get::<_, usize>(0)?) {
            *reading = mac::Reading {
                bytes: row.get(1)?,
                count: row.get(2)?,
                identity: row.get(3)?,
            };
        }
    }
    Ok(readings)
}

#[derive(Default)]
struct GrowthWindow {
    totals: [i64; LEAK_HOURS],
    counts: [i64; LEAK_HOURS],
    last_minute: Option<i64>,
    broken: bool,
}

fn growth_flags(
    db: &Connection,
    through: i64,
    current: &[mac::Reading; KINDS.len()],
) -> rusqlite::Result<[Option<Growth>; KINDS.len()]> {
    let first = through + 1 - LEAK_MINUTES;
    let mut windows: [GrowthWindow; KINDS.len()] = std::array::from_fn(|_| GrowthWindow::default());
    let mut query = db.prepare(
        "SELECT minute, kind, bytes, identity FROM minutes WHERE minute >= ?1 AND minute <= ?2 ORDER BY minute",
    )?;
    let mut rows = query.query([first, through])?;
    while let Some(row) = rows.next()? {
        let minute = row.get::<_, i64>(0)?;
        let kind = row.get::<_, usize>(1)?;
        let Some(window) = windows.get_mut(kind) else {
            continue;
        };
        let bytes = row.get::<_, Option<i64>>(2)?;
        let identity = row.get::<_, i64>(3)?;
        if identity != current[kind].identity {
            // Seeing another generation anywhere in the window breaks lifetime continuity.
            window.broken = true;
        }
        if let Some(bytes) = bytes {
            if minute - window.last_minute.unwrap_or(first - 1) > 3 {
                window.broken = true;
            }
            window.last_minute = Some(minute);
            let bin = ((minute - first) / MINUTES_PER_HOUR) as usize;
            window.totals[bin] += bytes;
            window.counts[bin] += 1;
        }
    }
    Ok(windows.map(|window| {
        if window.broken
            || window.last_minute != Some(through)
            || window.counts.iter().any(|count| *count < 55)
        {
            return None;
        }
        let means: [i64; LEAK_HOURS] = std::array::from_fn(|i| window.totals[i] / window.counts[i]);
        let increase = means[LEAK_HOURS - 1] - means[0];
        (means.windows(2).all(|pair| pair[1] - pair[0] >= MIB)
            && increase >= (32 * MIB).max(means[0] / 10))
        .then_some(Growth {
            window_minutes: LEAK_MINUTES,
            first_hour_mean_bytes: means[0],
            last_hour_mean_bytes: means[LEAK_HOURS - 1],
            increase_bytes: increase,
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suite_memory::{self, Kind};
    use std::path::PathBuf;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("suite-memory-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn database(&self) -> PathBuf {
            suite_memory::path(&self.0)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    fn sample(entries: &[(Kind, Option<i64>, i64)]) -> mac::Capture {
        let mut readings = [mac::Reading::default(); KINDS.len()];
        for &(kind, bytes, identity) in entries {
            readings[kind as usize] = mac::Reading {
                bytes,
                count: 1,
                identity,
            };
        }
        mac::Capture {
            process_count: 150,
            unreadable: entries
                .iter()
                .filter(|(_, bytes, _)| bytes.is_none())
                .count(),
            readings,
        }
    }

    #[test]
    fn current_sample_age_unreadable_processes_and_support_health() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        let now = crate::util::now_millis() as i64;
        assert_eq!(read(&fixture.0, now).status, "unavailable");
        assert!(!fixture.database().exists());
        let mut recorder = Recorder::open(&fixture.0).unwrap();
        assert_eq!(
            fixture.database().metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
        let capture = sample(&[(Kind::RichOs, Some(120 * MIB), 55), (Kind::Codex, None, 65)]);
        recorder.record(now, Some(&capture), 1750).unwrap();
        let view = read(&fixture.0, now);
        assert_eq!(view.status, "partial");
        assert_eq!(
            view.apps[Kind::RichOs as usize]
                .current
                .as_ref()
                .unwrap()
                .bytes,
            Some(120 * MIB)
        );
        assert_eq!(
            view.apps[Kind::Codex as usize]
                .current
                .as_ref()
                .unwrap()
                .status,
            "unreadable"
        );
        assert_eq!(
            view.apps[Kind::RichGlance as usize]
                .current
                .as_ref()
                .unwrap()
                .status,
            "not_running"
        );
        let health = health(&fixture.0);
        assert_eq!(
            (
                health.matched_processes,
                health.unreadable_processes,
                health.capture_us
            ),
            (2, 1, 1750)
        );
        assert_eq!(health.status, "partial");

        let stale = read(&fixture.0, now + STALE_MS + 1);
        assert_eq!(stale.status, "stale");
        assert!(stale.apps[0].current.is_none());
        assert_eq!(stale.apps[0].last_48_hours.as_ref().unwrap().samples, 1);
        let failed_at = now + 3 * MINUTE_MS;
        recorder.record(failed_at, None, 111).unwrap();
        assert_eq!(read(&fixture.0, failed_at).status, "inventory_failed");
        assert_eq!(
            read(&fixture.0, failed_at).apps[0]
                .last_90_days
                .as_ref()
                .unwrap()
                .samples,
            1
        );
    }

    #[test]
    fn rollups_are_weighted_and_a_minute_is_committed_once() {
        let fixture = Fixture::new();
        let mut recorder = Recorder::open(&fixture.0).unwrap();
        let start = 30_000 * MINUTES_PER_HOUR * MINUTE_MS;
        for (minute, bytes) in [(0, 30), (1, 60), (1, 900), (60, 120)] {
            recorder
                .record(
                    start + minute * MINUTE_MS,
                    Some(&sample(&[(Kind::RichOs, Some(bytes), 77)])),
                    0,
                )
                .unwrap();
        }
        let view = read(&fixture.0, start + 60 * MINUTE_MS);
        for trend in [
            view.apps[0].last_48_hours.as_ref(),
            view.apps[0].last_90_days.as_ref(),
        ] {
            let trend = trend.unwrap();
            assert_eq!(
                (
                    trend.samples,
                    trend.min_bytes,
                    trend.mean_bytes,
                    trend.max_bytes
                ),
                (3, 30, 70, 120)
            );
            assert_eq!(
                (trend.first_hour_mean_bytes, trend.last_hour_mean_bytes),
                (45, 120)
            );
        }
        let later = start + (RAW_MINUTES + 60) * MINUTE_MS;
        recorder.record(later, None, 0).unwrap();
        let view = read(&fixture.0, later);
        assert!(view.apps[0].last_48_hours.is_none());
        assert_eq!(view.apps[0].last_90_days.as_ref().unwrap().samples, 3);
    }

    #[test]
    fn ninety_days_of_all_nine_apps_fit_on_disk() {
        let fixture = Fixture::new();
        let mut recorder = Recorder::open(&fixture.0).unwrap();
        // Preserve the real schema, page cap and per-sample transactions. Skip fsync
        // for this accelerated simulation of nearly 131,000 separate minutes.
        recorder
            .0
            .pragma_update(None, "synchronous", "OFF")
            .unwrap();
        let start = 100_000 * MINUTES_PER_HOUR;
        let span = (LONG_HOURS + 1) * MINUTES_PER_HOUR;
        for offset in 0..span {
            let minute = start + offset;
            let entries = KINDS.map(|kind| {
                let i = kind as i64;
                (
                    kind,
                    Some((96 + i * 32) * MIB + (offset % 103) * 16384),
                    i64::MAX - i,
                )
            });
            recorder
                .record(minute * MINUTE_MS, Some(&sample(&entries)), 350)
                .unwrap();
        }
        let now = (start + span - 1) * MINUTE_MS;
        let view = read(&fixture.0, now);
        for app in view.apps {
            assert_eq!(app.last_48_hours.unwrap().samples, RAW_MINUTES);
            assert_eq!(
                app.last_90_days.unwrap().samples,
                LONG_HOURS * MINUTES_PER_HOUR
            );
        }
        let bytes = fixture.database().metadata().unwrap().len();
        assert!(bytes <= (MAX_PAGES * PAGE_BYTES) as u64);
        assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 1);
        println!(
            "90 days of all nine processes, 48 hours at minute resolution: {bytes} bytes on disk"
        );
    }

    #[test]
    fn a_tight_page_cap_evicts_oldest_rollups_and_keeps_new_samples() {
        let fixture = Fixture::new();
        let cap = 16;
        let mut recorder = Recorder::open_file(&fixture.database(), cap).unwrap();
        recorder
            .0
            .pragma_update(None, "synchronous", "OFF")
            .unwrap();
        let start = 100_000;
        let entries = KINDS.map(|kind| (kind, Some(150 * MIB), i64::MAX));
        let capture = sample(&entries);
        for hour in start..start + LONG_HOURS {
            recorder
                .record(hour * MINUTES_PER_HOUR * MINUTE_MS, Some(&capture), 0)
                .unwrap();
        }
        let last = start + LONG_HOURS - 1;
        let view = read(&fixture.0, last * MINUTES_PER_HOUR * MINUTE_MS);
        let trend = view.apps[0].last_90_days.as_ref().unwrap();
        assert!(
            trend.samples < LONG_HOURS,
            "the tight test cap was never reached"
        );
        assert_eq!(
            trend.last_hour_start_ms,
            last * MINUTES_PER_HOUR * MINUTE_MS
        );
        assert_eq!(
            trend.last_hour_start_ms - trend.first_hour_start_ms,
            (trend.samples - 1) * MINUTES_PER_HOUR * MINUTE_MS
        );
        assert_eq!(
            view.apps[0].current.as_ref().unwrap().bytes,
            Some(150 * MIB)
        );
        assert!(fixture.database().metadata().unwrap().len() <= (cap * PAGE_BYTES) as u64);
    }

    #[test]
    fn sustained_growth_requires_hours_from_one_generation_with_coverage() {
        let fixture = Fixture::new();
        let mut recorder = Recorder::open(&fixture.0).unwrap();
        recorder
            .0
            .pragma_update(None, "synchronous", "OFF")
            .unwrap();
        let start = 80_000 * MINUTES_PER_HOUR * MINUTE_MS;
        for minute in 0..LEAK_MINUTES {
            let rising = (100 + minute / 2) * MIB;
            let temporary_step = (100 + 70 * i64::from(minute >= 120)) * MIB;
            let mut entries = vec![
                (Kind::RichOs, Some(rising), 2),
                (
                    Kind::RichFiles,
                    Some(rising),
                    if minute < 120 { 5 } else { 6 },
                ),
                (Kind::RichDoctor, Some(temporary_step), 7),
                (Kind::RichGlance, Some(100 * MIB), 8),
            ];
            if !(120..135).contains(&minute) {
                entries.push((Kind::Capture, Some(rising), 9));
            }
            recorder
                .record(start + minute * MINUTE_MS, Some(&sample(&entries)), 0)
                .unwrap();
        }
        let view = read(&fixture.0, start + (LEAK_MINUTES - 1) * MINUTE_MS);
        let flag = view.apps[0].sustained_growth.as_ref().unwrap();
        assert_eq!(flag.increase_bytes, 90 * MIB);
        assert_eq!(flag.window_minutes, 240);
        assert!(view
            .apps
            .iter()
            .skip(1)
            .all(|app| app.sustained_growth.is_none()));
    }
}
