//! Calendar meetings for the meeting prompt (Granola PRD V4, "Meeting prompts and automatic
//! completion"): what the existing calendar sync has written, read back as the meetings that
//! should prompt one minute before they start.
//!
//! The sync itself is the service's (`richos-service workspace sync <vendor> --once --source
//! calendar`); this module only READS what it wrote: `<zone>/<vendor>/calendar/<event>/rev-*/
//! item.json`, one directory per revision. Per event the latest revision wins (its
//! `provenance.fetchedAt`), so a cancellation or a move synced since the last pass is what this
//! answers. A meeting prompts when it is timed (not all-day), not canceled or withdrawn, and has
//! someone other than the CEO in it. The same meeting on two calendars prompts once.
//!
//! Pure functions over a directory, so the rules are unit tests; the app runs the sync and the
//! timer (`meetings.rs`).

use crate::meeting::{MeetingApp, Scheduled};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The calendar vendors the service syncs, each with the client file `workspace connect` writes in
/// the zone (`config.js` `workspaceClientConfigPath`).
pub const VENDORS: [(&str, &str); 2] = [("google", "_oauth_client.json"), ("microsoft", "_oauth_client_microsoft.json")];

/// How long before its start a calendar meeting prompts.
pub const PROMPT_LEAD_MS: u64 = 60_000;

/// How often the app pulls the calendar (PRD V4: at launch and every five minutes).
pub const REFRESH_MS: u64 = 5 * 60_000;

/// The Google adapter's stored key for a canceled event, spelled as the service writes it.
const GOOGLE_CANCELED_KEY: &str = "cancelled"; // dialect-exempt: the service's stored JSON key

/// The CEO's workspace zone, where `workspace connect` and `sync` keep a calendar connected with
/// no company active (`config.js` `workspaceZone`, `evidenceRoot`).
pub fn workspace_zone(corpus: &Path) -> PathBuf {
    corpus.join("ceo").join("evidence").join("unfiled").join("workspace")
}

/// The vendors connected in this zone: the ones whose client file exists.
pub fn connected_vendors(zone: &Path) -> Vec<&'static str> {
    VENDORS.iter().filter(|(_, file)| zone.join(file).is_file()).map(|(v, _)| *v).collect()
}

/// One meeting that may prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarMeeting {
    pub event_id: String,
    pub title: String,
    pub starts_at: u64,
    pub ends_at: Option<u64>,
    /// The app its meeting link opens, when the location or invite text has one.
    pub app: Option<MeetingApp>,
}

impl CalendarMeeting {
    /// Its end, or, with none, an hour after its start.
    fn end(&self) -> u64 {
        self.ends_at.unwrap_or(self.starts_at + 60 * 60_000)
    }

    /// Due to prompt at `now`: from a minute before its start until it ends (or, with no end, for
    /// an hour), so an app opened late in a meeting still offers notes.
    pub fn due(&self, now: u64) -> bool {
        now + PROMPT_LEAD_MS >= self.starts_at && now < self.end()
    }

    pub fn scheduled(&self) -> Scheduled {
        Scheduled { event_id: self.event_id.clone(), title: self.title.clone(), starts_at: self.starts_at, ends_at: self.end(), app: self.app }
    }
}

/// Every meeting in the zone that may prompt, earliest first.
pub fn meetings(zone: &Path) -> Vec<CalendarMeeting> {
    let mut latest: BTreeMap<PathBuf, (u64, serde_json::Value, PathBuf)> = BTreeMap::new();
    for (vendor, _) in VENDORS {
        let Ok(events) = std::fs::read_dir(zone.join(vendor).join("calendar")) else { continue };
        for event in events.flatten() {
            let Ok(revisions) = std::fs::read_dir(event.path()) else { continue };
            for revision in revisions.flatten() {
                let Ok(text) = std::fs::read_to_string(revision.path().join("item.json")) else { continue };
                let Ok(item) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
                let fetched = item["provenance"]["fetchedAt"].as_u64().unwrap_or(0);
                let newer = latest.get(&event.path()).is_none_or(|(at, _, _)| fetched >= *at);
                if newer {
                    latest.insert(event.path(), (fetched, item, revision.path()));
                }
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut out: Vec<CalendarMeeting> = latest.into_values().filter_map(|(_, item, dir)| meeting_of(&item, &stored_text(&item, &dir))).collect();
    out.sort_by(|a, b| (a.starts_at, &a.event_id).cmp(&(b.starts_at, &b.event_id)));
    // One meeting on two calendars (or two accounts) is one prompt.
    out.retain(|m| seen.insert((m.starts_at, m.title.clone())));
    out
}

/// A revision's invite text: `content.text`, or, as the service stores it (`evidence.js`
/// `writeEvidence` moves the text out of the JSON), the file `content.textFile` names beside
/// `item.json` (rv-20261014T114658Z-04417671-85c4 #3: a Google invite's Zoom link only in its
/// HTML description was never read). A bare file name only; empty when there is none.
fn stored_text(item: &serde_json::Value, revision: &Path) -> String {
    if let Some(text) = item["content"]["text"].as_str() {
        return text.to_string();
    }
    item["content"]["textFile"]
        .as_str()
        .filter(|name| !name.is_empty() && Path::new(name).file_name() == Some(std::ffi::OsStr::new(name)))
        .and_then(|name| std::fs::read_to_string(revision.join(name)).ok())
        .unwrap_or_default()
}

/// The meeting a revision describes, when it should prompt. `text` is its invite text
/// ([`stored_text`]).
fn meeting_of(item: &serde_json::Value, text: &str) -> Option<CalendarMeeting> {
    let s = &item["content"]["structured"];
    let flag = |key: &str| s[key].as_bool() == Some(true);
    let withdrawn = flag(GOOGLE_CANCELED_KEY) || flag("withdrawn") || flag("removed");
    let timed = match item["vendor"].as_str() {
        Some("google") => s["start"]["dateTime"].is_string(),
        _ => !flag("isAllDay"),
    };
    if withdrawn || !timed {
        return None;
    }
    let starts_at = item["temporal"]["occurredAt"].as_u64()?;
    // "With other attendees": more than one person (by address) between organizer and attendees.
    let mut people = BTreeSet::new();
    let actors = &item["actors"];
    for person in std::iter::once(&actors["author"]).chain(actors["attendees"].as_array().into_iter().flatten()) {
        if let Some(email) = person["email"].as_str().map(str::trim).filter(|e| !e.is_empty()) {
            people.insert(email.to_lowercase());
        }
    }
    if people.len() < 2 {
        return None;
    }
    Some(CalendarMeeting {
        event_id: item["sourceItemId"].as_str()?.to_string(),
        title: item["content"]["title"].as_str().unwrap_or("Meeting").to_string(),
        starts_at,
        ends_at: item["temporal"]["validUntil"].as_u64(),
        // The link is in the location (a Zoom or Teams URL there), the links the adapter kept
        // (Google's conference link, an HTML invite's link targets: `meetingLinks`), or the
        // invite text (rv-20261014T105556Z-71dc7488-f9ab #3).
        app: s["location"]
            .as_str()
            .and_then(MeetingApp::in_text)
            .or_else(|| s["meetingLinks"].as_array().into_iter().flatten().filter_map(|l| l.as_str()).find_map(MeetingApp::in_text))
            .or_else(|| MeetingApp::in_text(text)),
    })
}

/// The prompts to withdraw after a refresh: a meeting known before that is gone now (canceled,
/// or no longer a meeting) or has moved. A moved meeting is withdrawn and then prompts at its new
/// time (the monitor forgets it on withdrawal).
pub fn withdrawn(previous: &BTreeMap<String, u64>, current: &[CalendarMeeting]) -> Vec<String> {
    let now: BTreeMap<&str, u64> = current.iter().map(|m| (m.event_id.as_str(), m.starts_at)).collect();
    previous
        .iter()
        .filter(|(id, start)| now.get(id.as_str()) != Some(start))
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Zone(PathBuf);
    impl Zone {
        fn new(name: &str) -> Zone {
            let dir = std::env::temp_dir().join(format!("richos-meeting-calendar-{name}-{}", std::process::id()));
            if dir.exists() {
                std::fs::remove_dir_all(&dir).unwrap();
            }
            std::fs::create_dir_all(&dir).unwrap();
            Zone(dir)
        }
        fn revision(&self, vendor: &str, event: &str, rev: &str, item: serde_json::Value) {
            let dir = self.0.join(vendor).join("calendar").join(event).join(rev);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("item.json"), item.to_string()).unwrap();
        }
    }
    impl Drop for Zone {
        fn drop(&mut self) {
            if self.0.exists() {
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
    }

    fn google(id: &str, title: &str, start: u64, fetched: u64, canceled: bool, others: &[&str]) -> serde_json::Value {
        let mut attendees = vec![json!({"name": "Me", "email": "me@example.com", "orgRelation": "self"})];
        attendees.extend(others.iter().map(|e| json!({"name": "", "email": e, "orgRelation": "unknown"})));
        let mut structured = json!({"start": {"dateTime": "2026-10-12T14:00:00Z"}});
        structured[GOOGLE_CANCELED_KEY] = json!(canceled);
        json!({
            "sourceItemId": id, "vendor": "google", "source": "calendar",
            "provenance": {"fetchedAt": fetched},
            "actors": {"author": {"email": "me@example.com", "orgRelation": "self"}, "attendees": attendees},
            "temporal": {"occurredAt": start, "validUntil": start + 30 * 60_000},
            "content": {"title": title, "structured": structured}
        })
    }

    #[test]
    fn timed_meetings_with_others_prompt_once_at_their_latest_revision() {
        let z = Zone::new("latest");
        z.revision("google", "a", "rev-1", google("google:calendar:x:a", "Acme pricing review", 1_000_000, 1, false, &["maya@acme.example"]));
        // A later revision moved it by an hour.
        let mut moved = google("google:calendar:x:a", "Acme pricing review", 4_600_000, 2, false, &["maya@acme.example"]);
        moved["content"]["structured"]["location"] = json!("https://us02web.zoom.us/j/123");
        z.revision("google", "a", "rev-2", moved);
        // Canceled since: its latest revision withdraws it.
        z.revision("google", "b", "rev-1", google("google:calendar:x:b", "Standup", 2_000_000, 1, false, &["sam@acme.example"]));
        z.revision("google", "b", "rev-2", google("google:calendar:x:b", "Standup", 2_000_000, 2, true, &["sam@acme.example"]));
        // Nobody else in it: a focus block never prompts.
        z.revision("google", "c", "rev-1", google("google:calendar:x:c", "Focus", 3_000_000, 1, false, &[]));
        // All-day: never prompts.
        let mut all_day = google("google:calendar:x:d", "Offsite", 3_000_000, 1, false, &["sam@acme.example"]);
        all_day["content"]["structured"]["start"] = json!({"date": "2026-10-12"});
        z.revision("google", "d", "rev-1", all_day);
        // The same meeting on a second account's calendar: one prompt.
        z.revision("google", "e", "rev-1", google("google:calendar:y:a", "Acme pricing review", 4_600_000, 1, false, &["maya@acme.example"]));
        // A Microsoft meeting the CEO was invited to (his own entry is not marked self there).
        z.revision("microsoft", "f", "rev-1", json!({
            "sourceItemId": "microsoft:calendar:z:f", "vendor": "microsoft", "source": "calendar",
            "provenance": {"fetchedAt": 1},
            "actors": {"author": {"email": "lee@partner.example"}, "attendees": [{"email": "me@example.com"}]},
            "temporal": {"occurredAt": 5_000_000u64, "validUntil": null},
            "content": {"title": "Partner sync", "text": "Join: https://teams.microsoft.com/l/meetup-join/x",
                        "structured": {"location": "Microsoft Teams Meeting", "isAllDay": false, "withdrawn": false}}
        }));
        let found = meetings(&z.0);
        let titles: Vec<(&str, u64)> = found.iter().map(|m| (m.title.as_str(), m.starts_at)).collect();
        assert_eq!(titles, vec![("Acme pricing review", 4_600_000), ("Partner sync", 5_000_000)]);
        // The meeting link's app, from the location or else the invite text.
        assert_eq!(found.iter().map(|m| m.app).collect::<Vec<_>>(), vec![Some(MeetingApp::Zoom), Some(MeetingApp::Teams)]);
    }

    #[test]
    fn the_meeting_link_the_service_kept_names_the_app() {
        // rv-20261014T105556Z-71dc7488-f9ab #3: what the adapters wrote for a Google Meet event
        // (its link only in the conference data) and a Teams invite (its link only as an HTML link
        // target). The adapters keep those links in `meetingLinks`.
        let z = Zone::new("links");
        let mut meet = google("google:calendar:x:m", "Google Meet A", 1_000_000, 1, false, &["other@example.com"]);
        meet["content"]["text"] = json!("");
        meet["content"]["structured"]["location"] = json!(null);
        meet["content"]["structured"]["meetingLinks"] = json!(["https://meet.google.com/abc-defg-hij"]);
        z.revision("google", "m", "rev-1", meet);
        z.revision("microsoft", "t", "rev-1", json!({
            "sourceItemId": "microsoft:calendar:z:t", "vendor": "microsoft", "source": "calendar",
            "provenance": {"fetchedAt": 1},
            "actors": {"author": {"email": "me@example.com"}, "attendees": [{"email": "other@example.com"}]},
            "temporal": {"occurredAt": 2_000_000u64, "validUntil": 3_800_000u64},
            "content": {"title": "Teams B", "text": "Join the meeting now",
                        "structured": {"location": "Microsoft Teams Meeting", "isAllDay": false, "withdrawn": false,
                                       "meetingLinks": ["https://teams.microsoft.com/l/meetup-join/fixture"]}}
        }));
        let found = meetings(&z.0);
        assert_eq!(found.iter().map(|m| (m.title.as_str(), m.app)).collect::<Vec<_>>(), vec![("Google Meet A", Some(MeetingApp::Meet)), ("Teams B", Some(MeetingApp::Teams))]);
    }

    #[test]
    fn the_invite_text_is_read_from_where_the_service_stores_it() {
        // rv-20261014T114658Z-04417671-85c4 #3 (`fixtures/calendar_storage.mjs`): the service's
        // writeEvidence keeps an item's text in content.txt beside item.json, never in the JSON,
        // so a Google invite whose Zoom link is only in its HTML description named no app.
        let z = Zone::new("stored-text");
        let mut zoom = google("google:calendar:x:z", "Google HTML Zoom invite", 1_000_000, 1, false, &["other@example.com"]);
        zoom["content"]["textFile"] = json!("content.txt");
        zoom["content"]["structured"]["meetingLinks"] = json!([]);
        z.revision("google", "z", "rev-1", zoom);
        let dir = z.0.join("google").join("calendar").join("z").join("rev-1");
        std::fs::write(dir.join("content.txt"), "<a href=\"https://us02web.zoom.us/j/123\">Join Zoom</a>").unwrap();
        let found = meetings(&z.0);
        assert_eq!(found.iter().map(|m| (m.title.as_str(), m.app)).collect::<Vec<_>>(), vec![("Google HTML Zoom invite", Some(MeetingApp::Zoom))]);
    }

    #[test]
    fn a_meeting_is_due_from_one_minute_before_its_start_until_it_ends() {
        let m = CalendarMeeting { event_id: "e".into(), title: "T".into(), starts_at: 10 * 60_000, ends_at: Some(40 * 60_000), app: None };
        assert!(!m.due(10 * 60_000 - PROMPT_LEAD_MS - 1));
        assert!(m.due(10 * 60_000 - PROMPT_LEAD_MS));
        assert!(m.due(39 * 60_000));
        assert!(!m.due(40 * 60_000));
    }

    #[test]
    fn a_canceled_or_moved_meeting_is_withdrawn_and_an_unchanged_one_is_not() {
        let previous: BTreeMap<String, u64> = [("a".to_string(), 100), ("b".to_string(), 200), ("c".to_string(), 300)].into();
        let now = vec![
            CalendarMeeting { event_id: "a".into(), title: "A".into(), starts_at: 100, ends_at: None, app: None },
            CalendarMeeting { event_id: "b".into(), title: "B".into(), starts_at: 250, ends_at: None, app: None },
        ];
        assert_eq!(withdrawn(&previous, &now), vec!["b".to_string(), "c".to_string()]);
    }

    #[test]
    fn the_connected_vendors_are_the_ones_with_a_client_file() {
        let z = Zone::new("vendors");
        assert!(connected_vendors(&z.0).is_empty());
        std::fs::write(z.0.join("_oauth_client_microsoft.json"), "{}").unwrap();
        assert_eq!(connected_vendors(&z.0), vec!["microsoft"]);
        assert_eq!(workspace_zone(Path::new("/c")), PathBuf::from("/c/ceo/evidence/unfiled/workspace"));
    }
}
