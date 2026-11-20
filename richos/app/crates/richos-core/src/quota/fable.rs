//! Model-specific limits can change the next model without exhausting the account.
use super::{Reading, Window};
use crate::claude_accounts::{LimitWindow, UsageLimit};
use crate::config::{EffortChoices, LeaseEfforts, LeaseModels, ModelChoice};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Claude Code's rate-limit event schema calls this window "Fable limit".
pub(crate) const STREAM_ID: &str = "seven_day_overage_included";

pub fn is_window(id: &str) -> bool {
    id.eq_ignore_ascii_case("model:Fable") || matches!(id, STREAM_ID | "seven_day_fable")
}

pub(super) fn week_has_room(reading: &Reading, now: u64) -> bool {
    reading.windows.iter().any(|window| {
        window.id == "seven_day" && window.resets_at.is_none_or(|until| until > now)
            && !reading.reaches_weekly(window)
    })
}

pub(super) fn has_general_window(windows: &[Window]) -> bool {
    windows.iter().any(|window| window.id == "seven_day")
}

fn fable_window(reading: &Reading) -> Option<&Window> {
    reading.windows.iter().find(|window| is_window(&window.id))
}

/// Cover the measured use until the next reading. The refusal retry allows
/// the final target to stay near the end even before there is a measured rate.
fn switch_point(reading: &Reading, window: &Window) -> f64 {
    reading.act_point(window, 99.9)
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Period {
    pub since: u64,
    pub resets_at: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub account: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub at: u64,
    pub on_opus: bool,
    pub resets_at: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub enabled: bool,
    pub account: Option<String>,
    pub point: Option<f64>,
    pub active: BTreeMap<String, Period>,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct State {
    pub enabled: bool,
    active: BTreeMap<String, Period>,
    changes: Vec<Change>,
}

impl Default for State {
    fn default() -> Self {
        Self { enabled: true, active: BTreeMap::new(), changes: Vec::new() }
    }
}

impl State {
    pub fn view(&self, account: Option<String>, reading: Option<&Reading>, now: u64) -> View {
        View {
            enabled: self.enabled,
            account,
            point: reading.and_then(|r| fable_window(r).map(|w| switch_point(r, w))),
            active: self.active.iter().filter(|(_, period)| self.enabled && period.resets_at > now)
                .map(|(account, period)| (account.clone(), *period)).collect(),
            changes: self.changes.clone(),
        }
    }

    pub fn set_enabled(&mut self, enabled: bool, now: u64) {
        self.enabled = enabled;
        if !enabled {
            for (account, period) in std::mem::take(&mut self.active) {
                self.record(account, now, false, period.resets_at);
            }
        }
    }

    pub fn observe(&mut self, account: &str, reading: Option<&Reading>, now: u64) {
        let ended: Vec<_> = self.active.iter().filter(|(_, p)| p.resets_at <= now)
            .map(|(id, p)| (id.clone(), p.resets_at)).collect();
        for (id, at) in ended {
            self.active.remove(&id);
            self.record(id, at, false, at);
        }
        let Some(reading) = reading else { return };
        let Some(window) = fable_window(reading) else { return };
        let Some(until) = window.resets_at.filter(|until| *until > now) else { return };
        let at_point = window.used_percent >= switch_point(reading, window);
        // An early reset has a fresh window and a new deadline. Re-reading a low
        // number from the same window cannot undo a refusal that named Fable.
        if !at_point && self.period(account, now).is_some_and(|period| {
            until > period.resets_at && !super::same_reset(until, period.resets_at)
        }) {
            self.active.remove(account);
            self.record(account.into(), now, false, now);
        }
        if at_point && week_has_room(reading, now) {
            self.activate(account, now, until);
        }
    }

    pub fn refused(&mut self, account: &str, reading: &Reading, limit: UsageLimit, now: u64) -> bool {
        if !self.enabled || limit.window == LimitWindow::Other || !week_has_room(reading, now) {
            return false;
        }
        let reset = limit.resets_at;
        let matches_reset = |window: &Window| reset.zip(window.resets_at).is_some_and(|(a, b)| super::same_reset(a, b));
        if reading.windows.iter().any(|w| w.id == "five_hour" && w.resets_at.is_none_or(|t| t > now)
            && (w.used_percent >= 100. || (limit.window == LimitWindow::Unknown && matches_reset(w))))
        {
            return false;
        }
        let fable = fable_window(reading);
        let fable_reset = fable.and_then(|window| window.resets_at);
        let until = match limit.window {
            LimitWindow::Fable => reset.or(fable_reset),
            LimitWindow::Unknown => {
                let Some(window) = fable else { return false };
                // A refusal with no named window needs either measured pressure on
                // Fable or a reset that uniquely identifies Fable's own window.
                let distinct_reset = matches_reset(window) && !reading.windows.iter()
                    .any(|other| !is_window(&other.id) && matches_reset(other));
                if !distinct_reset && (window.used_percent < switch_point(reading, window)
                    || (reset.is_some() && !matches_reset(window))) {
                    return false;
                }
                fable_reset
            }
            LimitWindow::Other => return false,
        };
        if let Some(until) = until.filter(|t| *t > now) {
            self.activate(account, now, until);
        }
        self.on_opus(account, now)
    }

    pub fn period(&self, account: &str, now: u64) -> Option<Period> {
        self.active.get(account).copied().filter(|p| self.enabled && p.resets_at > now)
    }

    pub fn on_opus(&self, account: &str, now: u64) -> bool {
        self.period(account, now).is_some()
    }

    fn activate(&mut self, account: &str, now: u64, resets_at: u64) {
        if !self.enabled || self.active.contains_key(account) {
            return;
        }
        self.active.insert(account.into(), Period { since: now, resets_at });
        self.record(account.into(), now, true, resets_at);
    }

    pub fn mark_conversation(&mut self, thread: &str) {
        for change in &mut self.changes {
            if change.thread_id.is_none() {
                change.thread_id = Some(thread.into());
            }
        }
    }

    fn record(&mut self, account: String, at: u64, on_opus: bool, resets_at: u64) {
        self.changes.push(Change { account, thread_id: None, at, on_opus, resets_at });
        if self.changes.len() > 12 {
            self.changes.remove(0);
        }
    }
}

pub fn lease_choices(
    using_opus: bool, mut models: LeaseModels, efforts: EffortChoices,
) -> (LeaseModels, LeaseEfforts) {
    if using_opus {
        models.rich = ModelChoice::Opus;
        models.stu = ModelChoice::Opus;
    }
    (models, efforts.on(models))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000_000;
    const END: u64 = NOW + 300_000;

    fn reading(weekly: f64, fable: f64) -> Reading {
        let window = |id: &str, used_percent| Window {
            id: id.into(), label: id.into(), used_percent, resets_at: Some(END), duration_ms: 604_800_000,
        };
        Reading { windows: vec![window("seven_day", weekly), window("model:Fable", fable)], ..Reading::default() }
    }

    #[test]
    fn slow_use_keeps_fable_until_nearly_full_and_recovery_is_automatic() {
        let mut state = State::default();
        state.observe("1", Some(&reading(40., 99.)), NOW);
        assert!(!state.on_opus("1", NOW));
        state.observe("1", Some(&reading(40., 99.9)), NOW);
        state.observe("1", Some(&reading(40., 99.)), NOW + 1);
        assert!(state.on_opus("1", NOW + 1));
        state.observe("1", None, END);
        assert!(!state.on_opus("1", END));
        let changes = state.view(None, None, END).changes;
        assert_eq!(changes.iter().map(|c| (c.at, c.on_opus)).collect::<Vec<_>>(), [(NOW, true), (END, false)]);
    }

    #[test]
    fn measured_use_can_advance_the_point_and_only_with_general_room() {
        let mut usage = reading(30., 97.);
        usage.speeds.insert("model:Fable".into(), 4. / super::super::REFRESH_INTERVAL_MS as f64);
        let mut state = State::default();
        state.observe("1", Some(&usage), NOW);
        assert!(state.on_opus("1", NOW));
        usage.windows[0].used_percent = 99.;
        state.observe("2", Some(&usage), NOW);
        assert!(!state.on_opus("2", NOW));
    }

    #[test]
    fn ambiguous_and_other_window_refusals_do_not_switch_models() {
        let mut usage = reading(40., 30.);
        usage.windows.push(Window {
            id: "five_hour".into(), label: "5-hour".into(), used_percent: 10.,
            resets_at: Some(NOW + 120_000), duration_ms: 18_000_000,
        });
        let mut state = State::default();
        assert!(!state.refused("1", &usage, UsageLimit::default(), NOW));
        for resets_at in [NOW + 120_000, END] {
            assert!(!state.refused("1", &usage, UsageLimit { resets_at: Some(resets_at), window: LimitWindow::Unknown }, NOW));
        }
        assert!(!state.refused("1", &reading(40., 100.), UsageLimit { resets_at: None, window: LimitWindow::Other }, NOW));
        assert!(!state.on_opus("1", NOW));
    }

    #[test]
    fn a_new_fable_window_brings_everyone_back_early() {
        let mut state = State::default();
        state.observe("1", Some(&reading(40., 100.)), NOW);
        let mut reset = reading(40., 0.);
        reset.windows[1].resets_at = Some(END + 120_000);
        state.observe("1", Some(&reset), NOW + 1);
        assert!(!state.on_opus("1", NOW + 1));
        assert_eq!(state.view(None, None, NOW + 1).changes.last().map(|c| (c.at, c.on_opus)),
            Some((NOW + 1, false)));
    }

    #[test]
    fn refused_fable_can_retry_with_the_same_account_and_saved_opus_efforts() {
        let mut state = State::default();
        let refused = UsageLimit { resets_at: Some(END), window: LimitWindow::Fable };
        assert!(state.refused("1", &reading(40., 90.), refused, NOW));
        state.observe("1", Some(&reading(40., 90.)), NOW + 1);
        assert!(state.on_opus("1", NOW + 1));
        assert!(!state.refused("2", &reading(99., 100.), refused, NOW));
        let models = LeaseModels::default();
        let efforts = EffortChoices { rich_opus: crate::config::EffortLevel::Medium, ..EffortChoices::default() };
        assert_eq!(lease_choices(state.on_opus("1", NOW), models, efforts).1.rich, efforts.rich_opus);
        assert_eq!(lease_choices(state.on_opus("1", END), models, efforts).0, models);
    }
}
