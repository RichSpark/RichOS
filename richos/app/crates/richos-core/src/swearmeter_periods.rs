//! Original submission times are kept with numeric counts, never with excerpts.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const DAYS_30_MS: u64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dates {
    counts: BTreeMap<u64, u64>,
    first: Option<u64>,
    undated: u64,
}

impl Dates {
    pub fn add(&mut self, at: Option<u64>, count: u64) {
        let Some(at) = at.filter(|&at| at > 0) else {
            self.undated += count;
            return;
        };
        self.first = Some(self.first.map_or(at, |first| first.min(at)));
        if count != 0 {
            *self.counts.entry(at).or_default() += count;
        }
    }

    pub fn remove(&mut self, at: Option<u64>, count: u64) {
        if count == 0 {
            return;
        }
        let Some(at) = at.filter(|&at| at > 0) else {
            self.undated -= count;
            return;
        };
        if let std::collections::btree_map::Entry::Occupied(mut entry) = self.counts.entry(at) {
            *entry.get_mut() -= count;
            if *entry.get() == 0 {
                entry.remove();
            }
        }
    }

    pub fn report(sources: &[&Self], now: u64, month_key: impl Fn(u64) -> Option<u32>) -> Periods {
        let cutoff = now.saturating_sub(DAYS_30_MS);
        let first = sources.iter().filter_map(|dates| dates.first).min();
        let mut months = BTreeMap::new();
        if let (Some(first), Some(last)) = (first.and_then(&month_key), month_key(now)) {
            let mut month = first;
            while month <= last {
                months.insert(month, 0);
                month = if month % 100 == 12 {
                    month + 89
                } else {
                    month + 1
                };
            }
        }
        let mut report = Periods::default();
        for dates in sources {
            report.undated += dates.undated;
            report.last_30_days += dates
                .counts
                .range(cutoff..=now)
                .map(|(_, count)| count)
                .sum::<u64>();
            for (&at, &count) in &dates.counts {
                match month_key(at) {
                    Some(month) if at <= now => *months.entry(month).or_default() += count,
                    _ => report.undated += count,
                }
            }
        }
        report.months = months
            .into_iter()
            .rev()
            .map(|(month, count)| Month { month, count })
            .collect();
        report
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Periods {
    pub last_30_days: u64,
    pub months: Vec<Month>,
    pub undated: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Month {
    /// YYYYMM in the user's local calendar.
    pub month: u32,
    pub count: u64,
}

pub fn local_month(at: u64) -> Option<u32> {
    #[cfg(unix)]
    {
        let seconds = libc::time_t::try_from(at / 1000).ok()?;
        let mut out = std::mem::MaybeUninit::<libc::tm>::uninit();
        // localtime_r writes the entire result on success and is safe on worker threads.
        let local = unsafe {
            if libc::localtime_r(&seconds, out.as_mut_ptr()).is_null() {
                return None;
            }
            out.assume_init()
        };
        let year = u32::try_from(local.tm_year.checked_add(1900)?).ok()?;
        let month = u32::try_from(local.tm_mon + 1).ok()?;
        if !(1970..=9999).contains(&year) {
            return None;
        }
        Some(year * 100 + month)
    }
    #[cfg(not(unix))]
    {
        let date =
            time::OffsetDateTime::from_unix_timestamp(i64::try_from(at / 1000).ok()?).ok()?;
        Some(u32::try_from(date.year()).ok()? * 100 + u32::from(u8::from(date.month())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periods_share_counts_and_rolling_cutoff_moves_without_a_submission() {
        let mut a = Dates::default();
        let mut b = Dates::default();
        let now = DAYS_30_MS * 4;
        let cutoff = now - DAYS_30_MS;
        a.add(Some(cutoff - 1), 2);
        a.add(Some(cutoff), 3);
        b.add(Some(now), 5);
        b.add(None, 1);
        let month = |at| {
            Some(if at < cutoff {
                202601
            } else if at < now {
                202602
            } else {
                202604
            })
        };
        let report = Dates::report(&[&a, &b], now, month);
        assert_eq!(report.last_30_days, 8);
        assert_eq!(report.undated, 1);
        assert_eq!(
            report
                .months
                .iter()
                .map(|m| (m.month, m.count))
                .collect::<Vec<_>>(),
            [(202604, 5), (202603, 0), (202602, 3), (202601, 2)]
        );
        assert_eq!(Dates::report(&[&a, &b], now + 1, month).last_30_days, 5);
        a.remove(Some(cutoff), 3);
        assert_eq!(Dates::report(&[&a, &b], now, month).last_30_days, 5);
    }
}
