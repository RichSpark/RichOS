//! THE OPENINGS, IN THE SHELL (round 74; the CEO's order of 2026-11-14). The rules are
//! `richos-core`'s (`openings.rs` for what is earned, `launch::pick_splash` for which one starts
//! the app); this file is where the shell's own facts meet them:
//!
//! * [`sync`] records, in the launch record, every opening the user has earned and not yet been
//!   given, dated by its rule. It runs at a start (the rules about time, starts and updates), at
//!   a quit and when the gallery asks (those and the money: the Token spend screen's all-time
//!   figure, once the history has been read);
//! * [`splash_facts`] is what the pick reads: his pick from Settings, what he has, the day;
//! * [`page_record`] is what the page reads, put in front of every page script beside
//!   `window.__RICHOS_LAUNCH__`: which openings are his and since when, and his three settings.
use richos_core::launch::{LaunchStore, SplashFacts};
use richos_core::openings::{self, Catalog, Facts};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The catalog the page draws from, compiled in (`ui/openings-catalog.js`, data).
pub const CATALOG_SOURCE: &str = include_str!("../../ui/openings-catalog.js");

pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| openings::parse(CATALOG_SOURCE).unwrap_or_default())
}

/// This Mac's offset from UTC at `now_ms`, in minutes, positive east; 0 where it cannot be read.
#[cfg(unix)]
pub fn local_offset_minutes(now_ms: u64) -> i32 {
    let seconds = (now_ms / 1000) as libc::time_t;
    // SAFETY: `localtime_r` writes only the `tm` it is handed and reads the one `time_t`.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&seconds, &mut tm) }.is_null() {
        return 0;
    }
    (tm.tm_gmtoff / 60) as i32
}

/// Elsewhere the day is counted in UTC.
#[cfg(not(unix))]
pub fn local_offset_minutes(_now_ms: u64) -> i32 {
    0
}

/// His settings for the opening, read off the settings store by whoever holds it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Settings {
    pub pick: Option<String>,
    pub sound: bool,
    pub company: Option<String>,
    /// "A different one each day": the last such day and the opening it showed.
    pub daily: Option<(i64, String)>,
}

impl Settings {
    pub fn of(config: &richos_core::config::ConfigStore) -> Settings {
        Settings {
            pick: config.opening_pick().map(str::to_string),
            sound: config.opening_sound(),
            company: config.opening_company().map(str::to_string),
            daily: config.opening_daily().map(|(day, id)| (day, id.to_string())),
        }
    }
}

/// RECORD WHAT HE HAS EARNED. `spend` is the all-time figure in dollars, or `None` when it has
/// not been read (a start never waits for it). Returns how many openings became his just now.
pub fn sync(launch: &mut LaunchStore, spend: Option<f64>, version: &str, now_ms: u64) -> usize {
    let this_version = format!("{}{version}", openings::VERSION_PREFIX);
    let rewards = launch.rewards_fired();
    let updated = rewards.keys().any(|k| k.starts_with(openings::VERSION_PREFIX) && *k != this_version);
    let facts = Facts {
        now_ms,
        installed_at: launch.installed_at(),
        starts: launch.starts().len() as u64,
        spend,
        // Nothing in RichOS counts these two yet (`richos_core::openings`, the module comment).
        jobs: None,
        hours_in_a_month: None,
        updated,
    };
    let mut fired: Vec<(String, u64)> = catalog()
        .openings
        .iter()
        .filter(|o| !rewards.contains_key(&openings::key(&o.id)))
        .filter_map(|o| Some((openings::key(&o.id), openings::earned_at(o.earn, &facts)?)))
        .collect();
    let earned = fired.len();
    if !version.is_empty() && !rewards.contains_key(&this_version) {
        fired.push((this_version, now_ms));
    }
    if fired.is_empty() {
        return 0;
    }
    match launch.note_rewards_fired(&fired) {
        Ok(_) => earned,
        Err(e) => {
            eprintln!("[richos] openings: what was earned could not be saved ({e})");
            0
        }
    }
}

/// What the pick for a start reads (`launch::pick_splash`): `start` is which start it is for.
pub fn splash_facts(launch: &LaunchStore, settings: &Settings, start: Option<u64>, now_ms: u64) -> SplashFacts {
    SplashFacts {
        start,
        pick: settings.pick.clone(),
        earned: openings::earned_ids(catalog(), launch.rewards_fired()),
        day: Some(openings::local_day(now_ms, local_offset_minutes(now_ms))),
        daily: settings.daily.clone(),
    }
}

/// "A DIFFERENT ONE EACH DAY", TODAY'S: the day where he is and the opening it opens on, or
/// `None` when that is not how he starts or nothing is his (`openings::daily`).
pub fn today(launch: &LaunchStore, settings: &Settings, now_ms: u64) -> Option<(i64, String)> {
    if settings.pick.as_deref() != Some(openings::DAILY) {
        return None;
    }
    let day = openings::local_day(now_ms, local_offset_minutes(now_ms));
    let mine = openings::earned_ids(catalog(), launch.rewards_fired());
    let last = settings.daily.as_ref().map(|(on, id)| (*on, id.as_str()));
    openings::daily(&mine, day, last).map(|id| (day, id.to_string()))
}

/// The facts for THE NEXT DAY'S pick, from today's: a day on, with today's opening as the last.
pub fn later_day_facts(facts: &SplashFacts, today: &(i64, String)) -> SplashFacts {
    SplashFacts { day: Some(today.0 + 1), daily: Some(today.clone()), ..facts.clone() }
}

/// THE FACTS A PICKED SPLASH SHOWS (`splash_pick::Saved::facts`): what is drawn on it that is
/// not in its own files, so an opening drawn at quit is used only while they still hold. His
/// company's line is on every opening; the version is on the one that shows it.
pub fn shown_facts(entry_id: &str, settings: &Settings, version: &str) -> BTreeMap<String, String> {
    let mut facts = BTreeMap::new();
    if let Some(company) = &settings.company {
        facts.insert("company".to_string(), company.clone());
    }
    let shows_version = entry_id
        .strip_prefix(openings::KEY_PREFIX)
        .and_then(|id| catalog().get(id))
        .and_then(|o| o.figure.as_deref())
        .is_some_and(|figure| figure.contains("{version}"));
    if shows_version {
        facts.insert("version".to_string(), version.to_string());
    }
    facts
}

/// The record as the page reads it: a plain object. `earned` maps an opening's id to the local
/// day it became his.
pub fn record(launch: &LaunchStore, settings: &Settings, version: &str, now_ms: u64) -> serde_json::Value {
    let offset = local_offset_minutes(now_ms);
    let rewards = launch.rewards_fired();
    let earned: serde_json::Map<String, serde_json::Value> = catalog()
        .openings
        .iter()
        .filter_map(|o| Some((o.id.clone(), openings::date(*rewards.get(&openings::key(&o.id))?, offset).into())))
        .collect();
    serde_json::json!({
        "firstStart": launch.installed_at().map(|at| openings::date(at, offset)),
        "version": version,
        "earned": earned,
        "pick": settings.pick,
        "sound": settings.sound,
        "company": settings.company,
        "day": openings::local_day(now_ms, offset),
        // With "a different one each day", the one today opens on (the gallery marks it).
        "today": today(launch, settings, now_ms).map(|(_, id)| id),
    })
}

/// A file name in `folder` that nothing has yet: `stem.ext`, then `stem 2.ext`, `stem 3.ext`.
pub fn free_name(folder: &std::path::Path, stem: &str, ext: &str) -> String {
    let stem: String = stem.chars().map(|c| if matches!(c, '/' | ':' | '\\') { '-' } else { c }).collect();
    let mut name = format!("{stem}.{ext}");
    let mut n = 2;
    while folder.join(&name).exists() {
        name = format!("{stem} {n}.{ext}");
        n += 1;
    }
    name
}

/// The line that puts the record in front of every page script.
pub fn page_record(launch: &LaunchStore, settings: &Settings, version: &str, now_ms: u64) -> String {
    format!("window.__RICHOS_OPENINGS__ = {};", record(launch, settings, version, now_ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use richos_core::launch::PriorRun;

    const DAY: u64 = 86_400_000;
    /// 2025-09-08 12:00 UTC.
    const FIRST: u64 = 1_757_332_800_000;

    fn store(name: &str, now: u64) -> (std::path::PathBuf, LaunchStore) {
        let dir = std::env::temp_dir().join(format!("richos-openings-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = LaunchStore::open(dir.join("launches.json"), now).unwrap();
        store.begin_run(now, "t", PriorRun::Unknown).unwrap();
        (dir, store)
    }

    fn mine(store: &LaunchStore) -> Vec<String> {
        openings::earned_ids(catalog(), store.rewards_fired())
    }

    #[test]
    fn a_saved_image_never_replaces_a_file_that_is_there() {
        let dir = std::env::temp_dir().join(format!("richos-openings-names-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(free_name(&dir, "RichOS Struck", "jpg"), "RichOS Struck.jpg");
        std::fs::write(dir.join("RichOS Struck.jpg"), b"x").unwrap();
        assert_eq!(free_name(&dir, "RichOS Struck", "jpg"), "RichOS Struck 2.jpg");
        std::fs::write(dir.join("RichOS Struck 2.jpg"), b"x").unwrap();
        assert_eq!(free_name(&dir, "RichOS Struck", "jpg"), "RichOS Struck 3.jpg");
        assert_eq!(free_name(&dir, "RichOS Every Job, a Light", "jpg"), "RichOS Every Job, a Light.jpg");
        assert_eq!(free_name(&dir, "a/b:c", "jpg"), "a-b-c.jpg");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_shipped_catalog_reads() {
        assert_eq!(catalog().openings.len(), 28);
    }

    #[test]
    fn a_first_start_gives_him_the_two_openings_and_a_later_one_what_time_and_money_earned() {
        let (dir, mut launch) = store("first", FIRST);
        assert_eq!(sync(&mut launch, None, "1.4.0", FIRST), 2);
        assert_eq!(mine(&launch), ["ruled-standard", "midnight-suede"]);
        assert_eq!(sync(&mut launch, None, "1.4.0", FIRST + 1), 0, "nothing is given twice");
        // Four months on, with $1,200 harnessed: the first two rungs of the money and the leather.
        let later = FIRST + 125 * DAY;
        assert_eq!(sync(&mut launch, Some(1_200.0), "1.4.0", later), 3);
        assert_eq!(mine(&launch), ["ruled-standard", "midnight-suede", "power-100", "power-1k", "material-leather"]);
        // The dates: the leather on the day three months came round, the money the day it was seen.
        let page = record(&launch, &Settings { sound: true, ..Settings::default() }, "1.4.0", later);
        assert_eq!(page["earned"]["ruled-standard"], openings::date(FIRST, local_offset_minutes(later)));
        assert_eq!(page["earned"]["material-leather"], openings::date(1_765_195_200_000, local_offset_minutes(later)));
        assert_eq!(page["earned"]["power-1k"], openings::date(later, local_offset_minutes(later)));
        assert_eq!(page["earned"].as_object().unwrap().len(), 5);
        assert_eq!((page["sound"].as_bool(), page["pick"].is_null(), page["company"].is_null()), (Some(true), true, true));
        // And it is on disk: a reopened record says the same.
        drop(launch);
        let reopened = LaunchStore::open(dir.join("launches.json"), later).unwrap();
        assert_eq!(mine(&reopened).len(), 5);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_update_edition_is_his_on_the_first_start_of_a_later_version_only() {
        let (dir, mut launch) = store("update", FIRST);
        sync(&mut launch, None, "1.4.0", FIRST);
        assert!(!mine(&launch).contains(&"edition-update".to_string()), "a first install is not an update");
        sync(&mut launch, None, "1.4.0", FIRST + DAY);
        assert!(!mine(&launch).contains(&"edition-update".to_string()), "the same version again is not an update");
        assert_eq!(sync(&mut launch, None, "1.5.0", FIRST + 2 * DAY), 1);
        assert!(mine(&launch).contains(&"edition-update".to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_pick_reads_his_setting_what_he_has_and_the_day() {
        let (dir, mut launch) = store("facts", FIRST);
        sync(&mut launch, Some(150.0), "1.4.0", FIRST);
        let settings = Settings { pick: Some("power-100".into()), sound: true, company: Some("Acme".into()), daily: None };
        let facts = splash_facts(&launch, &settings, Some(7), FIRST);
        assert_eq!(facts.start, Some(7));
        assert_eq!(facts.pick.as_deref(), Some("power-100"));
        assert_eq!(facts.earned, ["ruled-standard", "midnight-suede", "power-100"]);
        assert_eq!(facts.day, Some(openings::local_day(FIRST, local_offset_minutes(FIRST))));
        // What is drawn on the splash that its files do not hold.
        assert_eq!(shown_facts("opening/power-100", &settings, "1.4.0").into_iter().collect::<Vec<_>>(), [("company".to_string(), "Acme".to_string())]);
        assert_eq!(shown_facts("opening/edition-update", &settings, "1.4.0").get("version").map(String::as_str), Some("1.4.0"));
        assert!(shown_facts("round-11/v1", &Settings::default(), "1.4.0").is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_different_one_each_day_says_todays_and_the_next_days_for_the_quit_to_draw_both() {
        let (dir, mut launch) = store("daily", FIRST);
        sync(&mut launch, Some(150.0), "1.4.0", FIRST);
        let day = openings::local_day(FIRST, local_offset_minutes(FIRST));
        let his = ["ruled-standard", "midnight-suede", "power-100"];
        let daily = |last: Option<(i64, &str)>| Settings {
            pick: Some(openings::DAILY.into()),
            sound: true,
            company: None,
            daily: last.map(|(on, id)| (on, id.to_string())),
        };
        // Before the first such day the calendar picks; from then on they take turns.
        let first = today(&launch, &daily(None), FIRST).unwrap();
        assert_eq!(first, (day, his[day.rem_euclid(3) as usize].to_string()));
        assert_eq!(today(&launch, &daily(Some((day, "midnight-suede"))), FIRST), Some((day, "midnight-suede".to_string())));
        // Left on Friday, back on Monday: the one after Friday's.
        assert_eq!(today(&launch, &daily(Some((day - 3, "midnight-suede"))), FIRST), Some((day, "power-100".to_string())));
        // What the quit draws beside today's: the pick for any later day.
        let settings = daily(Some((day, "midnight-suede")));
        let facts = splash_facts(&launch, &settings, Some(7), FIRST);
        let ids: Vec<String> = his.iter().map(|id| openings::key(id)).collect();
        assert_eq!(richos_core::launch::pick_splash(&ids, &facts), Some("opening/midnight-suede"));
        let later = later_day_facts(&facts, &(day, "midnight-suede".to_string()));
        assert_eq!(richos_core::launch::pick_splash(&ids, &later), Some("opening/power-100"));
        // The page is told today's, so the gallery marks the one the start showed.
        assert_eq!(record(&launch, &settings, "1.4.0", FIRST)["today"], "midnight-suede");
        // Not his way of starting: nothing to say, nothing to draw ahead.
        assert_eq!(today(&launch, &Settings { pick: Some("power-100".into()), ..settings.clone() }, FIRST), None);
        assert!(record(&launch, &Settings::default(), "1.4.0", FIRST)["today"].is_null());
        std::fs::remove_dir_all(&dir).ok();
    }
}
