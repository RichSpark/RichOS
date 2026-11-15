//! THE OPENINGS — which opening screens are the user's, and since when (round 74,
//! `richos-hq/design/mockups/rounds/round-74`; the CEO's order of 2026-11-14).
//!
//! The catalog is data (`ui/openings-catalog.js`), read by the page and by the shell alike:
//! every opening there is, one row each, and what earns it. This module reads that file
//! ([`parse`]), says which openings a user's facts have earned and on what day ([`earned_at`]),
//! and names the key each is recorded under in the launch record (`launch.rs` `rewards_fired`,
//! "the log the reward logic will read"). The rule that picks the opening for a start lives with
//! the other splash rules, `pick_splash` in the launch module.
//!
//! THE NUMBERS ARE IN THE DATA, not here (CEO, 2026-11-14, on the thresholds: "adjust later"):
//! a threshold is changed in `openings-catalog.js` and nothing in this file moves.
//!
//! TWO LADDERS HAVE NOTHING TO READ YET. Nothing in RichOS counts jobs completed or hours
//! reclaimed, so [`Facts::jobs`] and [`Facts::hours_in_a_month`] are `None` from the shell and
//! those ten openings stay sealed. They are drawn, numbered and tested like the rest; they open
//! the day a counter exists and is passed in here.
use serde::Deserialize;
use std::collections::BTreeMap;

/// The launch record's key for an earned opening: `opening/<id>`. Also the id of its entry in
/// the splash library (`ui/openings.js` `entryId`), so one string names it everywhere.
pub const KEY_PREFIX: &str = "opening/";
/// The launch record's key for a version this install has run: `version/<x.y.z>`.
pub const VERSION_PREFIX: &str = "version/";
/// The stored pick that means "one of mine, a different one each day".
pub const DAILY: &str = "daily";

pub fn key(id: &str) -> String {
    format!("{KEY_PREFIX}{id}")
}

/// What makes an opening the user's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Earn {
    /// His from the first start.
    Start,
    /// Dollars of AI power harnessed, all time.
    Spend(f64),
    /// Jobs completed, all time.
    Jobs(u64),
    /// Hours reclaimed in one calendar month.
    Hours(f64),
    /// Calendar months since the first start.
    Months(u32),
    /// Starts of RichOS.
    Starts(u64),
    /// The first start after an update.
    Update,
}

/// One opening, as far as the shell needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Opening {
    pub id: String,
    /// What it is called: "Struck".
    pub name: String,
    pub family: String,
    /// The library entry it is built on; `None` means its family's.
    pub base: Option<String>,
    pub earn: Earn,
    /// Whether it carries a number (a milestone or an edition).
    pub figure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Family {
    pub base: Option<String>,
    /// How long its openings stay up, when the family says; else the base entry's own.
    pub seconds: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Catalog {
    pub openings: Vec<Opening>,
    pub families: BTreeMap<String, Family>,
}

impl Catalog {
    pub fn get(&self, id: &str) -> Option<&Opening> {
        self.openings.iter().find(|o| o.id == id)
    }

    /// The library entry an opening is built on.
    pub fn base_of<'a>(&'a self, opening: &'a Opening) -> Option<&'a str> {
        opening.base.as_deref().or_else(|| self.families.get(&opening.family)?.base.as_deref())
    }

    /// Its family's seconds, when the family sets them.
    pub fn seconds_of(&self, opening: &Opening) -> Option<f64> {
        self.families.get(&opening.family)?.seconds
    }
}

#[derive(Deserialize)]
struct RawCatalog {
    families: BTreeMap<String, RawFamily>,
    openings: Vec<RawOpening>,
}

#[derive(Deserialize)]
struct RawFamily {
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    seconds: Option<String>,
}

#[derive(Deserialize)]
struct RawOpening {
    id: String,
    name: String,
    family: String,
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    figure: Option<String>,
    earn: BTreeMap<String, serde_json::Value>,
}

fn earn_of(raw: &BTreeMap<String, serde_json::Value>) -> Option<Earn> {
    if raw.len() != 1 {
        return None;
    }
    let (kind, value) = raw.iter().next()?;
    let number = value.as_f64().filter(|n| n.is_finite() && *n > 0.0);
    let whole = value.as_u64().filter(|n| *n > 0);
    match kind.as_str() {
        "start" => (value.as_bool() == Some(true)).then_some(Earn::Start),
        "update" => (value.as_bool() == Some(true)).then_some(Earn::Update),
        "spend" => number.map(Earn::Spend),
        "hours" => number.map(Earn::Hours),
        "jobs" => whole.map(Earn::Jobs),
        "starts" => whole.map(Earn::Starts),
        "months" => whole.and_then(|n| u32::try_from(n).ok()).map(Earn::Months),
        _ => None,
    }
}

/// READ THE CATALOG out of `openings-catalog.js`: everything after the assignment is JSON. A
/// row this build cannot read (an `earn` it has no name for) is left out rather than guessed
/// at; a file that does not parse is `None`, and the caller has no openings beyond the library.
pub fn parse(source: &str) -> Option<Catalog> {
    let at = source.find("window.RichOpeningsCatalog")?;
    let rest = &source[at..];
    let json = rest[rest.find('=')? + 1..].trim().trim_end_matches(';').trim();
    let raw: RawCatalog = serde_json::from_str(json).ok()?;
    let families = raw
        .families
        .into_iter()
        .map(|(name, f)| {
            let seconds = f.seconds.and_then(|s| s.parse::<f64>().ok()).filter(|n| n.is_finite() && *n > 0.0);
            (name, Family { base: f.base, seconds })
        })
        .collect::<BTreeMap<_, _>>();
    let openings = raw
        .openings
        .into_iter()
        .filter_map(|o| {
            let earn = earn_of(&o.earn)?;
            families.contains_key(&o.family).then_some(Opening { id: o.id, name: o.name, family: o.family, base: o.base, earn, figure: o.figure })
        })
        .collect();
    Some(Catalog { openings, families })
}

/// Everything an earning rule may read, as the shell knows it now.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Facts {
    pub now_ms: u64,
    /// The first start (`LaunchStore::installed_at`); `None` when the launch record is unreadable.
    pub installed_at: Option<u64>,
    /// Starts so far.
    pub starts: u64,
    /// Dollars of AI power harnessed, all time; `None` until the whole history has been read.
    pub spend: Option<f64>,
    /// Jobs completed, all time. Nothing counts them yet (see the module comment).
    pub jobs: Option<u64>,
    /// The most hours reclaimed in one calendar month. Nothing counts them yet.
    pub hours_in_a_month: Option<f64>,
    /// Whether this install has run an earlier version than the one running.
    pub updated: bool,
}

/// WHEN AN OPENING BECAME THE USER'S, by its rule and these facts: the instant to record, or
/// `None` while it is not his. A rule about time answers with the day it came true (three
/// months after the first start, not the day somebody looked); every other rule answers now,
/// the first moment the app saw it true.
pub fn earned_at(earn: Earn, facts: &Facts) -> Option<u64> {
    match earn {
        Earn::Start => facts.installed_at,
        Earn::Months(months) => {
            let due = months_after(facts.installed_at?, months);
            (facts.now_ms >= due).then_some(due)
        }
        Earn::Spend(dollars) => (facts.spend? >= dollars).then_some(facts.now_ms),
        Earn::Jobs(jobs) => (facts.jobs? >= jobs).then_some(facts.now_ms),
        Earn::Hours(hours) => (facts.hours_in_a_month? >= hours).then_some(facts.now_ms),
        Earn::Starts(starts) => (facts.starts >= starts).then_some(facts.now_ms),
        Earn::Update => facts.updated.then_some(facts.now_ms),
    }
}

const MS_PER_DAY: u64 = 86_400_000;

// This module's own copy of the two civil-date conversions (Howard Hinnant's), as `journal.rs`
// and `home_repos.rs` carry theirs: the launch record's module is read by nothing else in this
// crate (`tests/launch_no_outbound_tests.rs`), and a date needs none of it.

/// Days since 1970-01-01 for a civil (proleptic Gregorian) date.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 }.div_euclid(400);
    let yoe = y - era * 400;
    let mp = ((m as i64) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date for a count of days since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The same time of day, `months` calendar months later (UTC); a day the later month does not
/// have (the 31st) is that month's last.
fn months_after(at_ms: u64, months: u32) -> u64 {
    let days = (at_ms / MS_PER_DAY) as i64;
    let (y, m, d) = civil_from_days(days);
    let total = (m as i64 - 1) + months as i64;
    let (y2, m2) = (y + total.div_euclid(12), (total.rem_euclid(12) + 1) as u32);
    let next_month = if m2 == 12 { (y2 + 1, 1) } else { (y2, m2 + 1) };
    let last = (days_from_civil(next_month.0, next_month.1, 1) - days_from_civil(y2, m2, 1)) as u32;
    let day = days_from_civil(y2, m2, d.min(last));
    (day.max(0) as u64) * MS_PER_DAY + at_ms % MS_PER_DAY
}

/// The calendar day an instant falls on where the user is, as `YYYY-MM-DD` (the page's
/// `longDate` reads it). `utc_offset_minutes` is positive east.
pub fn date(at_ms: u64, utc_offset_minutes: i32) -> String {
    let (y, m, d) = civil_from_days(local_day(at_ms, utc_offset_minutes));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Which local day an instant falls on, as days since 1970-01-01: what "a different one each
/// day" counts by.
pub fn local_day(at_ms: u64, utc_offset_minutes: i32) -> i64 {
    (at_ms as i64 + i64::from(utc_offset_minutes) * 60_000).div_euclid(MS_PER_DAY as i64)
}

/// "ONE OF MINE, A DIFFERENT ONE EACH DAY": which of his openings `day` opens on. `mine` is
/// his openings in the catalog's order; `last` is the one the last such day opened on, and
/// that day.
///
/// The same day opens on the same one. ANY LATER DAY OPENS ON THE ONE AFTER IT, round again
/// after the last, however many days went by: so the quit, which draws the next start's
/// opening ahead, knows the next day's as well as today's (after a night, a weekend, a
/// month), and a start on a new day plays as fast as any other. Before the first one, or when
/// the last one is no longer his, the calendar picks: the day's number among how many he has.
pub fn daily<'a>(mine: &'a [String], day: i64, last: Option<(i64, &str)>) -> Option<&'a str> {
    if mine.is_empty() {
        return None;
    }
    let at = match last.and_then(|(on, id)| Some((on, mine.iter().position(|m| m == id)?))) {
        Some((on, at)) if on == day => at,
        Some((_, at)) => (at + 1) % mine.len(),
        None => day.rem_euclid(mine.len() as i64) as usize,
    };
    Some(mine[at].as_str())
}

/// The ids of the openings a launch record says are the user's, in the catalog's own order.
pub fn earned_ids(catalog: &Catalog, rewards: &BTreeMap<String, u64>) -> Vec<String> {
    catalog.openings.iter().filter(|o| rewards.contains_key(&key(&o.id))).map(|o| o.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = include_str!("../../../ui/openings-catalog.js");
    const DAY: u64 = MS_PER_DAY;
    /// 2025-09-08 12:00 UTC, round 74's made-up user's first start.
    const FIRST: u64 = 1_757_332_800_000;

    fn catalog() -> Catalog {
        parse(SOURCE).expect("the shipped catalog parses")
    }

    #[test]
    fn the_shipped_catalog_is_round_74s_twenty_eight_openings_in_order() {
        let c = catalog();
        assert_eq!(c.openings.len(), 28);
        let count = |family: &str| c.openings.iter().filter(|o| o.family == family).count();
        assert_eq!(
            (count("house"), count("power"), count("jobs"), count("hours"), count("edition"), count("material")),
            (2, 8, 6, 4, 3, 5)
        );
        assert_eq!(c.openings[0].id, "ruled-standard");
        assert_eq!((c.openings[6].id.as_str(), c.openings[6].name.as_str()), ("power-100k", "Struck"), "No. 7 is Struck");
        assert_eq!(c.openings[27].id, "material-leaf");
        // Every opening resolves to a library entry, and the milestone families run 5 seconds.
        for o in &c.openings {
            assert!(c.base_of(o).is_some(), "{} has no base", o.id);
            let five = matches!(o.family.as_str(), "power" | "jobs" | "hours" | "edition");
            assert_eq!(c.seconds_of(o), five.then_some(5.0), "{}", o.id);
            assert_eq!(o.figure.is_some(), five, "{}: a number on a milestone and nowhere else", o.id);
        }
        assert_eq!(c.get("power-100").unwrap().earn, Earn::Spend(100.0));
        assert_eq!(c.get("jobs-1").unwrap().earn, Earn::Jobs(1));
        assert_eq!(c.get("hours-40").unwrap().earn, Earn::Hours(40.0));
        assert_eq!(c.get("edition-1000").unwrap().earn, Earn::Starts(1000));
        assert_eq!(c.get("edition-update").unwrap().earn, Earn::Update);
        assert_eq!(c.get("material-leather").unwrap().earn, Earn::Months(3));
    }

    #[test]
    fn a_row_this_build_cannot_read_is_left_out_and_a_broken_file_is_no_catalog() {
        let later = SOURCE.replacen(r#""earn": { "spend": 100 }"#, r#""earn": { "referrals": 3 }"#, 1);
        let c = parse(&later).unwrap();
        assert_eq!(c.openings.len(), 27);
        assert!(c.get("power-100").is_none());
        assert!(parse("window.RichOpeningsCatalog = { not json").is_none());
        assert!(parse("").is_none());
    }

    #[test]
    fn a_new_user_has_the_two_openings_and_nothing_else() {
        let c = catalog();
        let facts = Facts { now_ms: FIRST + 60_000, installed_at: Some(FIRST), starts: 1, spend: Some(0.0), ..Facts::default() };
        let mine: Vec<&str> = c.openings.iter().filter(|o| earned_at(o.earn, &facts).is_some()).map(|o| o.id.as_str()).collect();
        assert_eq!(mine, ["ruled-standard", "midnight-suede"]);
        assert_eq!(earned_at(Earn::Start, &facts), Some(FIRST), "his since the first start");
    }

    #[test]
    fn each_rule_opens_at_its_number_and_not_before() {
        let base = Facts { now_ms: FIRST + 400 * DAY, installed_at: Some(FIRST), starts: 999, ..Facts::default() };
        // Spend: unknown until the history is read, then at the threshold.
        assert_eq!(earned_at(Earn::Spend(100.0), &base), None, "an unread history earns nothing");
        assert_eq!(earned_at(Earn::Spend(100.0), &Facts { spend: Some(99.99), ..base.clone() }), None);
        assert_eq!(earned_at(Earn::Spend(100.0), &Facts { spend: Some(100.0), ..base.clone() }), Some(base.now_ms));
        // Starts.
        assert_eq!(earned_at(Earn::Starts(1000), &base), None);
        assert_eq!(earned_at(Earn::Starts(1000), &Facts { starts: 1000, ..base.clone() }), Some(base.now_ms));
        // An update.
        assert_eq!(earned_at(Earn::Update, &base), None);
        assert_eq!(earned_at(Earn::Update, &Facts { updated: true, ..base.clone() }), Some(base.now_ms));
        // Jobs and hours: nothing counts them yet, so nothing is earned; with a count, at the number.
        assert_eq!(earned_at(Earn::Jobs(1), &base), None);
        assert_eq!(earned_at(Earn::Hours(12.0), &base), None);
        assert_eq!(earned_at(Earn::Jobs(10), &Facts { jobs: Some(10), ..base.clone() }), Some(base.now_ms));
        assert_eq!(earned_at(Earn::Hours(12.0), &Facts { hours_in_a_month: Some(11.9), ..base.clone() }), None);
        assert_eq!(earned_at(Earn::Hours(12.0), &Facts { hours_in_a_month: Some(12.0), ..base.clone() }), Some(base.now_ms));
        // An unreadable launch record earns nothing that is measured from the first start.
        let unknown = Facts { installed_at: None, ..base.clone() };
        assert_eq!(earned_at(Earn::Start, &unknown), None);
        assert_eq!(earned_at(Earn::Months(3), &unknown), None);
    }

    #[test]
    fn time_with_rich_is_dated_the_day_it_came_true() {
        // Round 74's made-up user: first start September 8, 2025; leather on December 8, 2025,
        // slate on March 8, 2026, steel and the year on September 8, 2026.
        let at = |months| earned_at(Earn::Months(months), &Facts { now_ms: FIRST + 800 * DAY, installed_at: Some(FIRST), ..Facts::default() });
        assert_eq!(date(at(3).unwrap(), 0), "2025-12-08");
        assert_eq!(date(at(6).unwrap(), 0), "2026-03-08");
        assert_eq!(date(at(12).unwrap(), 0), "2026-09-08");
        assert_eq!(date(at(24).unwrap(), 0), "2027-09-08");
        assert_eq!(at(36), None, "three years have not passed");
        // The day before is not the day.
        let eve = Facts { now_ms: months_after(FIRST, 3) - 1, installed_at: Some(FIRST), ..Facts::default() };
        assert_eq!(earned_at(Earn::Months(3), &eve), None);
        // A first start on the 31st: the month that has no 31st gives its last day.
        let jan31 = (days_from_civil(2026, 1, 31) as u64) * DAY;
        assert_eq!(date(months_after(jan31, 1), 0), "2026-02-28");
        assert_eq!(date(months_after(jan31, 3), 0), "2026-04-30");
        assert_eq!(date(months_after(jan31, 12), 0), "2027-01-31");
    }

    #[test]
    fn the_day_is_the_users_own() {
        // 2026-11-14 23:30 in US Pacific (UTC-8) is already the 15th in UTC.
        let at = (days_from_civil(2026, 11, 15) as u64) * DAY + 7 * 3_600_000 + 1_800_000;
        assert_eq!(date(at, 0), "2026-11-15");
        assert_eq!(date(at, -480), "2026-11-14");
        assert_eq!(local_day(at, 0) - local_day(at, -480), 1);
    }

    #[test]
    fn the_earned_list_is_in_catalog_order() {
        let c = catalog();
        let rewards: BTreeMap<String, u64> = [key("material-leather"), key("power-100"), key("ruled-standard"), "thirty-days".to_string()]
            .into_iter()
            .map(|k| (k, 1))
            .collect();
        assert_eq!(earned_ids(&c, &rewards), ["ruled-standard", "power-100", "material-leather"]);
    }
}
