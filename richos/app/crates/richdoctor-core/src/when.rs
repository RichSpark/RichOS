//! **"ABOUT WHEN DID IT LAST HAPPEN?"** (round 39 §2): the first question, asked by RichDoctor
//! itself, because the answer sets the incident window every collector reads (PRD §5).
//!
//! The choices are rough on purpose (round 39's unresolved point 3, built as the mockup shows:
//! no time picker). A typed time is accepted as typed: a clock time it can read sets the window
//! around that time today; anything else keeps the person's words for the report and widens the
//! window to the last day.
use crate::collect::Window;

pub const QUESTION: &str = "Thanks. About when did it last happen?";
pub const CHOICES: [&str; 4] = ["Just now", "Earlier today", "Yesterday", "Enter a time…"];
pub const ENTER: &str = "Enter a time…";
pub const ROUGH_QUESTION: &str = "About what time? A rough time is fine.";
pub const ROUGH: [&str; 5] = ["Early this morning", "Late this morning", "Around lunch", "This afternoon", "This evening"];

/// The question when two copies of RichOS are installed (round 39 §1).
pub const WHICH_QUESTION: &str = "Which RichOS were you opening?";

const HOUR: u64 = 3_600_000;

/// Local midnight of the day `now` is in, as epoch millis.
fn midnight(now: u64) -> u64 {
    let off = crate::util::local_offset_seconds(now) * 1000;
    let local = now as i64 + off;
    (local - local.rem_euclid(86_400_000) - off).max(0) as u64
}

/// "8:40", "8:40 AM", "8 pm", "14:10" as minutes after midnight.
pub fn parse_clock(s: &str) -> Option<u32> {
    let t = s.trim().to_ascii_lowercase().replace('.', "");
    let (num, meridiem) = if let Some(n) = t.strip_suffix("am") {
        (n.trim().to_string(), Some(false))
    } else if let Some(n) = t.strip_suffix("pm") {
        (n.trim().to_string(), Some(true))
    } else {
        (t.clone(), None)
    };
    let num = num.trim_start_matches("about ").trim_start_matches("around ").trim();
    let (h, m) = match num.split_once(':') {
        Some((h, m)) => (h.trim().parse::<u32>().ok()?, m.trim().parse::<u32>().ok()?),
        None => (num.parse::<u32>().ok()?, 0),
    };
    if m > 59 {
        return None;
    }
    let h = match meridiem {
        Some(pm) if (1..=12).contains(&h) => (h % 12) + if pm { 12 } else { 0 },
        Some(_) => return None,
        None if h <= 23 => h,
        None => return None,
    };
    Some(h * 60 + m)
}

/// The window and the words the report uses for an answer to [`QUESTION`] or
/// [`ROUGH_QUESTION`], or a typed time.
pub fn window_for(answer: &str, now: u64) -> (Window, String) {
    let day = midnight(now);
    let zone = crate::util::utc_offset_label(now);
    let date = crate::util::local_date(now);
    // A part of the day that has not begun yet today means the same part of yesterday.
    let part = |a: u64, b: u64, words: &str| {
        let d = if day + a * HOUR > now { day.saturating_sub(24 * HOUR) } else { day };
        let label = if d == day { words.to_string() } else { format!("{words} (taken as yesterday: that part of today has not come yet)") };
        (Window { start: d + a * HOUR, end: (d + b * HOUR).min(now + 60_000), known: true }, format!("{label}, {} ({zone}).", crate::util::local_date(d + a * HOUR)))
    };
    match answer.trim() {
        "Just now" => (Window::around(now), format!("Just now: about {}, {date} ({zone}).", crate::util::local_clock(now))),
        "Earlier today" => (Window::today(now), format!("Earlier today, {date} ({zone}).")),
        "Yesterday" => {
            let y = day.saturating_sub(24 * HOUR);
            (Window { start: y, end: day, known: true }, format!("Yesterday, {} ({zone}).", crate::util::local_date(y)))
        }
        "Early this morning" => part(0, 9, "Early this morning"),
        "Late this morning" => part(9, 12, "Late this morning"),
        "Around lunch" => part(11, 14, "Around lunch"),
        "This afternoon" => part(12, 18, "This afternoon"),
        "This evening" => part(17, 24, "This evening"),
        typed => match parse_clock(typed) {
            Some(minutes) => {
                let mut at = day + minutes as u64 * 60_000;
                if at > now + 5 * 60_000 {
                    at = at.saturating_sub(24 * HOUR);
                }
                (Window::around(at), format!("About {}, {} ({zone}), as the person said: \"{typed}\".", crate::util::local_clock(at), crate::util::local_date(at)))
            }
            None => (Window::unknown(now), format!("As the person said: \"{typed}\". RichDoctor looked at the last day. ({zone})")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_times_read_as_people_write_them() {
        assert_eq!(parse_clock("8:40"), Some(8 * 60 + 40));
        assert_eq!(parse_clock("8:40 AM"), Some(8 * 60 + 40));
        assert_eq!(parse_clock("8 pm"), Some(20 * 60));
        assert_eq!(parse_clock("12 am"), Some(0));
        assert_eq!(parse_clock("12:30 PM"), Some(12 * 60 + 30));
        assert_eq!(parse_clock("14:10"), Some(14 * 60 + 10));
        assert_eq!(parse_clock("around 9"), Some(9 * 60));
        for bad in ["25:00", "13 pm", "8:75", "after lunch", ""] {
            assert_eq!(parse_clock(bad), None, "{bad}");
        }
    }

    #[test]
    fn every_choice_sets_a_window_that_ends_by_now() {
        let now = 1_792_658_590_000;
        for a in CHOICES.iter().chain(ROUGH.iter()).filter(|a| **a != ENTER) {
            let (w, words) = window_for(a, now);
            assert!(w.start < w.end, "{a}: {w:?}");
            assert!(w.end <= now + Window::HALF, "{a}");
            assert!(!words.is_empty());
        }
        let (w, words) = window_for("sometime after breakfast", now);
        assert!(!w.known);
        assert!(words.contains("sometime after breakfast"));
        let (w, _) = window_for("8:40 AM", now);
        assert!(w.known && w.end - w.start == 2 * Window::HALF);
    }
}
