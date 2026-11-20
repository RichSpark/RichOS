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

pub(crate) fn week_has_room(reading: &Reading, now: u64) -> bool {
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Opus,
    OtherAccountFirst,
}

pub(crate) struct Week {
    pub(crate) used: f64,
    pub(crate) resets_at: u64,
    pub(crate) has_room: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Observation {
    at: u64,
    at_point: bool,
}

pub(super) fn observation(reading: &Reading, observed_at: &BTreeMap<String, u64>) -> Option<Observation> {
    let window = fable_window(reading)?;
    Some(Observation {
        at: *observed_at.get(&window.id)?,
        at_point: window.used_percent >= switch_point(reading, window),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Period {
    pub since: u64,
    pub resets_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    estimate_based_on: Option<u64>,
    /// A lease can use Opus on the account it is leaving until its running work is safe to move.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    departing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeReason {
    EstimatedUsage,
    FableRoomConfirmed,
    OtherAccountWithFable,
    OtherAccountWithOpus,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<ChangeReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_label: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub enabled: bool,
    pub mode: Mode,
    pub can_use_other_account: bool,
    pub account: Option<String>,
    pub account_label: Option<String>,
    pub point: Option<f64>,
    pub active: BTreeMap<String, Period>,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct State {
    pub enabled: bool,
    pub mode: Mode,
    active: BTreeMap<String, Period>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    refused_until: BTreeMap<String, u64>,
    changes: Vec<Change>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_account_move: Option<u64>,
}

impl Default for State {
    fn default() -> Self {
        Self { enabled: true, mode: Mode::Opus, active: BTreeMap::new(),
            refused_until: BTreeMap::new(), changes: Vec::new(), last_account_move: None }
    }
}

impl State {
    pub fn view(&self, account: Option<String>, reading: Option<&Reading>, now: u64) -> View {
        View {
            enabled: self.enabled,
            mode: self.mode,
            can_use_other_account: false,
            account,
            account_label: None,
            point: reading.and_then(|r| fable_window(r).map(|w| switch_point(r, w))),
            active: self.active.iter().filter(|(_, period)| self.enabled && period.resets_at > now)
                .map(|(account, period)| (account.clone(), *period)).collect(),
            changes: self.changes.clone(),
        }
    }

    pub fn set_enabled(&mut self, enabled: bool, now: u64) {
        self.enabled = enabled;
        if !enabled {
            self.refused_until.clear();
            for (account, period) in std::mem::take(&mut self.active) {
                if !period.departing { self.record(account, now, false, period.resets_at, None); }
            }
        }
    }

    pub fn settle(&mut self, account: &str, reading: Option<&Reading>, now: u64) -> bool {
        self.refused_until.retain(|_, deadline| *deadline > now);
        let ended: Vec<_> = self.active.iter().filter(|(_, p)| p.resets_at <= now)
            .map(|(id, p)| (id.clone(), *p)).collect();
        for (id, period) in ended {
            self.active.remove(&id);
            if !period.departing { self.record(id, period.resets_at, false, period.resets_at, None); }
        }
        let Some(reading) = reading else { return false };
        let Some(window) = fable_window(reading) else { return false };
        let Some(until) = window.resets_at.filter(|until| *until > now) else { return false };
        let actual = reading.fable_observation;
        if !at_point(reading, window) && self.fable_refusal_until(account, now)
            .is_some_and(|deadline| until > deadline && !super::same_reset(until, deadline)) {
            self.refused_until.remove(account);
        }
        if let Some(period) = self.active.get_mut(account) {
            // Only a later Fable observation can confirm or overturn the estimate.
            if let Some(observation) = actual.filter(|o| period.estimate_based_on.is_some_and(|at| o.at > at)) {
                if observation.at_point {
                    period.estimate_based_on = None;
                } else {
                    let visible = !period.departing;
                    self.active.remove(account);
                    if visible { self.record(account.into(), now, false, until, Some(ChangeReason::FableRoomConfirmed)); }
                    return true;
                }
            }
        }
        // An early reset has a fresh window and a new deadline. Re-reading a low
        // number from the same window cannot undo a refusal that named Fable.
        if !at_point(reading, window) && self.period(account, now).is_some_and(|period| {
            until > period.resets_at && !super::same_reset(until, period.resets_at)
        }) && self.active.remove(account).is_some_and(|period| !period.departing) {
            self.record(account.into(), now, false, now, None);
        }
        false
    }

    pub fn observe(&mut self, account: &str, reading: Option<&Reading>, now: u64) {
        self.observe_for_lease(account, reading, now, false);
    }

    pub fn observe_departure(&mut self, account: &str, reading: Option<&Reading>, now: u64) {
        self.observe_for_lease(account, reading, now, true);
    }

    fn observe_for_lease(&mut self, account: &str, reading: Option<&Reading>, now: u64, departing: bool) {
        if self.settle(account, reading, now) { return; }
        let Some(reading) = reading.filter(|reading| week_has_room(reading, now)) else { return };
        if let Some(until) = self.fable_refusal_until(account, now) {
            self.activate(account, now, until, None, departing);
            return;
        }
        let Some(window) = fable_window(reading) else { return };
        let Some(until) = window.resets_at.filter(|until| *until > now) else { return };
        if at_point(reading, window) || self.period(account, now).is_some() {
            let estimate_based_on = reading.fable_observation.filter(|o| !o.at_point).map(|o| o.at);
            self.activate(account, now, until, estimate_based_on, departing);
        }
    }

    pub fn week(&self, account: &str, reading: &Reading, now: u64) -> Option<Week> {
        let window = fable_window(reading);
        if let Some(resets_at) = self.fable_refusal_until(account, now) {
            return Some(Week { used: 100., resets_at, has_room: false });
        }
        if let Some(Period { resets_at, .. }) = self.period(account, now) {
            return Some(Week { used: window.map_or(100., |w| w.used_percent),
                resets_at, has_room: false });
        }
        let window = window?;
        Some(Week {
            used: window.used_percent,
            resets_at: window.resets_at.filter(|until| *until > now)?,
            has_room: !at_point(reading, window),
        })
    }

    pub fn fable_refusal_until(&self, account: &str, now: u64) -> Option<u64> {
        self.refused_until.get(account).copied().filter(|deadline| *deadline > now)
    }

    pub fn refuse_before_account_choice(&mut self, account: &str, reading: &Reading, limit: UsageLimit, now: u64) -> bool {
        let Some((Some(until), None)) = self.refusal_period(reading, limit, now) else { return false };
        self.refused_until.insert(account.into(), until);
        if let Some(period) = self.active.get_mut(account) {
            period.estimate_based_on = None;
        }
        true
    }

    fn refusal_period(&self, reading: &Reading, limit: UsageLimit, now: u64) -> Option<(Option<u64>, Option<u64>)> {
        if !self.enabled || limit.window == LimitWindow::Other || !week_has_room(reading, now) {
            return None;
        }
        let reset = limit.resets_at;
        let matches_reset = |window: &Window| reset.zip(window.resets_at).is_some_and(|(a, b)| super::same_reset(a, b));
        if reading.windows.iter().any(|w| w.id == "five_hour" && w.resets_at.is_none_or(|t| t > now)
            && (w.used_percent >= 100. || (limit.window == LimitWindow::Unknown && matches_reset(w))))
        {
            return None;
        }
        let fable = fable_window(reading);
        let fable_reset = fable.and_then(|window| window.resets_at);
        let (until, estimate_based_on) = match limit.window {
            LimitWindow::Fable => (reset.or(fable_reset), None),
            LimitWindow::Unknown => {
                let window = fable?;
                // A refusal with no named window needs either measured pressure on
                // Fable or a reset that uniquely identifies Fable's own window.
                let distinct_reset = matches_reset(window) && !reading.windows.iter()
                    .any(|other| !is_window(&other.id) && matches_reset(other));
                if !distinct_reset && (window.used_percent < switch_point(reading, window)
                    || (reset.is_some() && !matches_reset(window))) {
                    return None;
                }
                let estimate_based_on = reading.fable_observation.filter(|o| !o.at_point && !distinct_reset).map(|o| o.at);
                (fable_reset, estimate_based_on)
            }
            LimitWindow::Other => return None,
        };
        Some((until.filter(|t| *t > now), estimate_based_on))
    }

    pub fn refused(&mut self, account: &str, reading: &Reading, limit: UsageLimit, now: u64) -> bool {
        let Some((until, estimate_based_on)) = self.refusal_period(reading, limit, now) else { return false };
        if let Some(until) = until {
            if estimate_based_on.is_none() {
                if let Some(period) = self.active.get_mut(account) {
                    period.estimate_based_on = None;
                }
            }
            let departing = self.period(account, now).is_some_and(|period| period.departing);
            self.activate(account, now, until, estimate_based_on, departing);
        }
        self.on_opus(account, now)
    }

    pub fn period(&self, account: &str, now: u64) -> Option<Period> {
        self.active.get(account).copied().filter(|p| self.enabled && p.resets_at > now)
    }

    pub fn on_opus(&self, account: &str, now: u64) -> bool {
        self.period(account, now).is_some()
    }

    fn activate(&mut self, account: &str, now: u64, resets_at: u64, estimate_based_on: Option<u64>, departing: bool) {
        if !self.enabled { return; }
        let period = self.active.entry(account.into()).or_insert(Period {
            since: now, resets_at, estimate_based_on, departing: true,
        });
        if !departing && period.departing {
            period.departing = false;
            period.since = now;
            let reason = period.estimate_based_on.map(|_| ChangeReason::EstimatedUsage);
            let resets_at = period.resets_at;
            self.record(account.into(), now, true, resets_at, reason);
        }
    }

    pub fn mark_conversation(&mut self, thread: &str) {
        for change in &mut self.changes {
            if change.thread_id.is_none() {
                change.thread_id = Some(thread.into());
            }
        }
    }

    pub fn mark_account_move(&mut self, moved: &crate::claude_accounts::LastSwitch, label: &str, now: u64) {
        if self.last_account_move.is_some_and(|last| last >= moved.at) { return; }
        self.last_account_move = Some(moved.at);
        if let Some(period) = self.period(&moved.to, now).filter(|_| moved.why == "fableOpus") {
            if let Some(change) = self.changes.iter_mut().rev().find(|c| c.account == moved.to
                && c.at == period.since && c.at >= moved.at && c.on_opus && c.thread_id.is_none()) {
                change.reason = Some(ChangeReason::OtherAccountWithOpus);
                change.account_label = Some(label.into());
                return;
            }
            self.record(moved.to.clone(), now, true, period.resets_at, Some(ChangeReason::OtherAccountWithOpus));
        } else {
            self.record(moved.to.clone(), now, false, now, Some(ChangeReason::OtherAccountWithFable));
        }
        if let Some(change) = self.changes.last_mut() {
            change.account_label = Some(label.into());
        }
    }

    fn record(&mut self, account: String, at: u64, on_opus: bool, resets_at: u64, reason: Option<ChangeReason>) {
        self.changes.push(Change { account, thread_id: None, at, on_opus, resets_at, reason, account_label: None });
        if self.changes.len() > 12 {
            self.changes.remove(0);
        }
    }
}

fn at_point(reading: &Reading, window: &Window) -> bool {
    reading.fable_observation.is_some_and(|observation| observation.at_point)
        || window.used_percent >= switch_point(reading, window)
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
    fn an_estimated_switch_stays_after_the_provider_confirms_fable_has_run_out() {
        let mut estimate = reading(40., 99.95);
        estimate.fable_observation = Some(Observation { at: NOW - 1, at_point: false });
        let mut full = reading(40., 100.);
        full.fable_observation = Some(Observation { at: NOW + 2, at_point: true });
        let mut low = reading(40., 95.);
        low.fable_observation = Some(Observation { at: NOW + 3, at_point: false });

        for named_refusal in [false, true] {
            let mut state = State::default();
            state.observe("1", Some(&estimate), NOW);
            assert_eq!(state.period("1", NOW).unwrap().estimate_based_on, Some(NOW - 1));
            let persisted = serde_json::to_string(&state).unwrap();
            let mut state: State = serde_json::from_str(&persisted).unwrap();
            let mut reused = low.clone();
            reused.fable_observation = estimate.fable_observation;
            state.observe("1", Some(&reused), NOW + 1);
            assert!(state.on_opus("1", NOW + 1), "the prior observation has not changed");
            if named_refusal {
                assert!(state.refused("1", &estimate, UsageLimit {
                    window: LimitWindow::Fable, resets_at: Some(END),
                }, NOW + 2));
            } else {
                state.observe("1", Some(&full), NOW + 2);
            }
            state.observe("1", Some(&low), NOW + 3);
            assert!(state.on_opus("1", NOW + 3), "confirmed switches keep the original period");
            assert_eq!(state.period("1", NOW + 3).unwrap().estimate_based_on, None);
        }
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
