//! **The bar, the words' flight and the menu bar item: every decision, with no AppKit.**
//!
//! Slice 3 of `richos-hq/docs/plans/2026-10-08-dictation-anywhere.md` (revision 2): round 19's
//! states 13 to 19 (`design/mockups/rounds/round-19/dictation.html`) and Iris's two bar lines
//! (`dictation-more-lines.html`, `desk-nomodel` and `desk-nowrite`). The tool's windows
//! (`src-tauri/src/dictation/bar.rs`, `menubar.rs`) are thin; what the bar shows for a phase,
//! how long it stays, whether it takes a click, where it sits, how often the meter moves, which
//! range of text the words occupy and the menu bar item's picture are all decided here, so each
//! one is a unit test.
//!
//! Geometry is in POINTS with a TOP-LEFT origin on the primary screen, the space Accessibility
//! reports in. AppKit's frames are bottom-left; [`to_cocoa`] and [`from_cocoa`] are the one
//! conversion.

use crate::dictation::{Phase, Problem, FASTER, MORE_ACCURATE};
use std::time::Duration;

// =============================================================================================
// WHAT THE BAR SHOWS
// =============================================================================================

/// What the bar shows. One per phase of the session, plus the menu's off notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarView {
    Hidden,
    /// The gold microphone, the live level meter, the time, "Tap F1 to finish" (state 14).
    Listening,
    /// "Writing it down…" (state 15).
    Writing,
    /// "Added", then the bar goes (state 16).
    Added,
    /// Honest delivery result. Its readable notice/hover timer belongs to the page.
    Delivery,
    /// One drawn problem line (states 18 and 19, the other drawn moments, Iris's two lines).
    Problem(Problem),
    /// "Dictation is off." after **Turn dictation off** in the menu (round 19 `sb-onoff`).
    Off,
}

impl BarView {
    /// The name the bar page switches on (`ui/dictation-bar.js`).
    pub fn tag(self) -> &'static str {
        match self {
            BarView::Hidden => "hidden",
            BarView::Listening => "listening",
            BarView::Writing => "writing",
            BarView::Added => "added",
            BarView::Delivery => "delivery",
            BarView::Problem(_) => "problem",
            BarView::Off => "off",
        }
    }
}

/// **What the bar shows for `phase`.** Idle hides it; done is "Added"; a problem is its line.
pub fn view_for(phase: Phase) -> BarView {
    match phase {
        Phase::Idle => BarView::Hidden,
        Phase::Listening { .. } => BarView::Listening,
        Phase::Writing => BarView::Writing,
        Phase::Done => BarView::Added,
        Phase::Problem(p) => BarView::Problem(p),
    }
}

/// "Added" stays this long, then the bar goes (round 19 `paste()`, `setTimeout(…, 1500)`).
pub const ADDED_FOR: Duration = Duration::from_millis(1500);
/// "Dictation is off." stays this long (round 19 `toast()`, 2800 ms), then the tool ends.
pub const OFF_FOR: Duration = Duration::from_millis(2800);

/// **How long a problem line stays** (round 19 `problem()`, and `dictation-more-lines.html`
/// for Iris's two): the lines that hand him something to do (paste it himself, allow
/// Accessibility, wait for the download, say it again) stay 5.2 s; the others 4.2 s.
pub fn problem_shown_for(p: Problem) -> Duration {
    match p {
        Problem::NoTextBox | Problem::NoAccessibility | Problem::ModelMissing | Problem::CouldNotWrite => {
            Duration::from_millis(5200)
        }
        Problem::DidNotCatch | Problem::NoSound | Problem::NoMicrophone | Problem::VoiceStillListening => {
            Duration::from_millis(4200)
        }
    }
}

/// **The bar goes by itself after this**, or stays (listening and writing end with the session).
pub fn hide_after(view: BarView) -> Option<Duration> {
    match view {
        BarView::Added => Some(ADDED_FOR),
        BarView::Problem(p) => Some(problem_shown_for(p)),
        BarView::Off => Some(OFF_FOR),
        BarView::Hidden | BarView::Listening | BarView::Writing | BarView::Delivery => None,
    }
}

/// **The problem bars that carry Fix it**: the microphone and Accessibility (round 19
/// `renderPill`, `D.problem === "mic" || D.problem === "ax"`). Fix it opens RichOS on the
/// Dictation sheet.
pub fn has_fix(p: Problem) -> bool {
    matches!(p, Problem::NoMicrophone | Problem::NoAccessibility)
}

/// **Does the bar take the mouse?** Only while a bar with Fix it is up (plan section 6, "It
/// ignores the mouse except while a problem bar with Fix it is up"); every other moment a click
/// on it reaches the app beneath.
pub fn takes_mouse(view: BarView) -> bool {
    view == BarView::Listening || view == BarView::Delivery || matches!(view, BarView::Problem(p) if has_fix(p))
}

// =============================================================================================
// THE METER
// =============================================================================================

/// The least time between two meter updates: 34 ms is at most 29.4 a second, inside the
/// plan's "at most 30 updates a second" (section 2, row 14). 1000 / 33 would be 30.3.
pub const METER_MIN_GAP_MS: u64 = 34;

/// Lets a meter update through at most once every [`METER_MIN_GAP_MS`].
#[derive(Debug, Default, Clone, Copy)]
pub struct MeterGate {
    last_ms: Option<u64>,
}

impl MeterGate {
    /// Is an update due at `now_ms`? Records it when it is.
    pub fn due(&mut self, now_ms: u64) -> bool {
        match self.last_ms {
            Some(last) if now_ms.saturating_sub(last) < METER_MIN_GAP_MS => false,
            _ => {
                self.last_ms = Some(now_ms);
                true
            }
        }
    }
}

/// The bar meter's level for one frame: the same function voice mode's meter reads.
pub fn meter_level(frame: &[f32]) -> f32 {
    crate::vad::display_level(crate::vad::rms(frame))
}

// =============================================================================================
// WHERE THE WINDOWS SIT
// =============================================================================================

/// A rectangle in points, top-left origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn contains(&self, (px, py): (f64, f64)) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// **Which screen the bar goes on** (plan section 6): the one holding the focused window's
/// center, otherwise the one holding the pointer, otherwise the first (the primary).
pub fn bar_screen(screens: &[Rect], focused_window: Option<Rect>, pointer: Option<(f64, f64)>) -> usize {
    let holding = |p: (f64, f64)| screens.iter().position(|s| s.contains(p));
    focused_window.and_then(|w| holding(w.center())).or_else(|| pointer.and_then(holding)).unwrap_or(0)
}

/// The space between the bar's window and the bottom of the screen's visible area (above the
/// Dock). The window carries its own room for the pill's shadow (`ui/dictation-bar.css`).
pub const BAR_GAP: f64 = 12.0;

/// **The bar's window, bottom center** of a screen's visible area (`visible` excludes the menu
/// bar and the Dock), for a window of `size`.
pub fn bar_frame(visible: Rect, (w, h): (f64, f64)) -> Rect {
    Rect { x: (visible.x + (visible.w - w) / 2.0).round(), y: (visible.y + visible.h - h - BAR_GAP).round(), w, h }
}

/// The space between the menu bar item and the menu's window.
pub const MENU_GAP: f64 = 4.0;

/// **The menu's window under its menu bar item**: its right edge on the item's right edge (round
/// 19 `renderSb`, `r.right - 330`), just below the item, and kept inside the screen's visible
/// area with 8 points to spare.
pub fn menu_frame(item: Rect, visible: Rect, (w, h): (f64, f64)) -> Rect {
    let x = (item.x + item.w - w).clamp(visible.x + 8.0, (visible.x + visible.w - w - 8.0).max(visible.x + 8.0));
    let y = (item.y + item.h + MENU_GAP).max(visible.y);
    Rect { x: x.round(), y: y.round(), w, h }
}

/// A top-left rectangle as AppKit's bottom-left `(x, y, w, h)`, given the primary screen's height.
pub fn to_cocoa(r: Rect, primary_height: f64) -> (f64, f64, f64, f64) {
    (r.x, primary_height - r.y - r.h, r.w, r.h)
}

/// AppKit's bottom-left `(x, y, w, h)` as a top-left rectangle.
pub fn from_cocoa((x, y, w, h): (f64, f64, f64, f64), primary_height: f64) -> Rect {
    Rect { x, y: primary_height - y - h, w, h }
}

/// A screen rectangle as a rectangle inside a window whose frame is `window` (both top-left).
pub fn within(r: Rect, window: Rect) -> Rect {
    Rect { x: r.x - window.x, y: r.y - window.y, w: r.w, h: r.h }
}

// =============================================================================================
// WHERE THE WORDS LANDED
// =============================================================================================

/// The words' length as Accessibility counts text: UTF-16 units.
pub fn utf16_len(text: &str) -> isize {
    text.encode_utf16().count() as isize
}

/// **The range the pasted words occupy**: from where the selection began, for their length.
pub fn words_range(selection_start: isize, text: &str) -> (isize, isize) {
    (selection_start, utf16_len(text))
}

/// **Has the paste landed?** The app's cursor is right after the words: where the selection
/// began, plus their length. Before that, the text Accessibility reports is not yet the words.
pub fn paste_landed(selection_start: isize, text: &str, cursor_now: isize) -> bool {
    cursor_now == selection_start + utf16_len(text)
}

/// A rectangle Accessibility gives for the words is worth a flight only when it is a real place
/// on a screen: some width, some height, and not absurdly large (an app that answers with its
/// whole text view would light everything).
pub fn worth_lighting(r: Rect, screen: Rect) -> bool {
    r.w >= 1.0 && r.h >= 1.0 && r.w <= screen.w && r.h <= screen.h / 2.0 && screen.contains(r.center())
}

// =============================================================================================
// THE MENU
// =============================================================================================

/// The two Accuracy rows, as the menu names them (round 19 `sb-mode`, `data-v`).
pub fn model_for_choice(choice: &str) -> Option<&'static str> {
    match choice {
        "accurate" => Some(MORE_ACCURATE),
        "fast" => Some(FASTER),
        _ => None,
    }
}

/// Which row is ticked: derived from the model id, never stored beside it (plan section 1).
pub fn choice_for_model(model: &str) -> &'static str {
    if model == FASTER {
        "fast"
    } else {
        "accurate"
    }
}

// =============================================================================================
// THE MENU BAR ITEM'S PICTURE
// =============================================================================================

/// The menu bar item's size in pixels: 18 points at 2x.
pub const ICON_PX: u32 = 36;

/// **Round 19's microphone (`IC.mic`) as a coverage mask**, `px` by `px`, one byte per pixel.
///
/// The glyph in its own 24-unit box, stroke 2.4, round caps: the capsule `rect x=9 y=3 w=6 h=11
/// rx=3`, the cup `M5 11 a7 7 0 0 0 14 0` (the lower half of a circle of radius 7 about
/// (12, 11)) and the stem from (12, 18) to (12, 21). Drawn by distance to each stroke's middle
/// line, 4x4 samples per pixel, so the edges are smooth with no image library.
pub fn mic_mask(px: u32) -> Vec<u8> {
    let scale = 24.0 / px as f64;
    let half = 1.2;
    let mut out = vec![0u8; (px * px) as usize];
    for j in 0..px {
        for i in 0..px {
            let mut hits = 0u32;
            for sj in 0..4 {
                for si in 0..4 {
                    let x = (i as f64 + (si as f64 + 0.5) / 4.0) * scale;
                    let y = (j as f64 + (sj as f64 + 0.5) / 4.0) * scale;
                    if mic_distance(x, y) <= half {
                        hits += 1;
                    }
                }
            }
            out[(j * px + i) as usize] = ((hits * 255 + 8) / 16) as u8;
        }
    }
    out
}

fn segment_distance((px, py): (f64, f64), (ax, ay): (f64, f64), (bx, by): (f64, f64)) -> f64 {
    let (dx, dy) = (bx - ax, by - ay);
    let t = (((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    ((px - ax - t * dx).powi(2) + (py - ay - t * dy).powi(2)).sqrt()
}

/// Distance from (x, y) to the nearest stroke middle line of the glyph.
fn mic_distance(x: f64, y: f64) -> f64 {
    // The capsule: its outline is radius 3 about the segment (12, 6)-(12, 11).
    let capsule = (segment_distance((x, y), (12.0, 6.0), (12.0, 11.0)) - 3.0).abs();
    // The cup: the lower half circle, with its two ends.
    let r = ((x - 12.0).powi(2) + (y - 11.0).powi(2)).sqrt();
    let cup = if y >= 11.0 {
        (r - 7.0).abs()
    } else {
        let a = ((x - 5.0).powi(2) + (y - 11.0).powi(2)).sqrt();
        let b = ((x - 19.0).powi(2) + (y - 11.0).powi(2)).sqrt();
        a.min(b)
    };
    let stem = segment_distance((x, y), (12.0, 18.0), (12.0, 21.0));
    capsule.min(cup).min(stem)
}

/// **The item while dictation is on and idle**: the microphone in black on clear, which macOS
/// draws as a template image in the menu bar's own ink (light or dark), as every system item is.
pub fn template_icon() -> Vec<u8> {
    let mask = mic_mask(ICON_PX);
    let mut rgba = Vec::with_capacity(mask.len() * 4);
    for a in mask {
        rgba.extend_from_slice(&[0, 0, 0, a]);
    }
    rgba
}

/// The gold the item turns while listening: round 19's `--gold`, dark and light
/// (`#sbBtn.is-rec{background:var(--gold)}`), and the ink drawn on it (`--on-gold`).
pub const GOLD_DARK: [u8; 3] = [0xC2, 0xA3, 0x5C];
pub const GOLD_LIGHT: [u8; 3] = [0x9C, 0x7C, 0x34];
pub const ON_GOLD: [u8; 3] = [0x0C, 0x13, 0x22];

/// **The item while listening** (state 14, "the menu bar item turns gold"): a gold rounded
/// square, the microphone cut into it in `--on-gold`. `dark_menu_bar` picks the gold for the
/// menu bar's appearance, the way round 19 measured it (7.90:1 dark, 3.52:1 light).
pub fn listening_icon(dark_menu_bar: bool) -> Vec<u8> {
    let gold = if dark_menu_bar { GOLD_DARK } else { GOLD_LIGHT };
    // The glyph inset so the square has a margin of gold around it, as round 19's 15px glyph in
    // its 24px-high button.
    let inner = ICON_PX - 10;
    let glyph = mic_mask(inner);
    let radius = 7.0;
    let n = ICON_PX as f64;
    let mut rgba = Vec::with_capacity((ICON_PX * ICON_PX * 4) as usize);
    for j in 0..ICON_PX {
        for i in 0..ICON_PX {
            let (x, y) = (i as f64 + 0.5, j as f64 + 0.5);
            // Rounded-square coverage: distance outside the inner square of the corners.
            let qx = (x - n / 2.0).abs() - (n / 2.0 - radius);
            let qy = (y - n / 2.0).abs() - (n / 2.0 - radius);
            let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() - radius;
            let square = (0.5 - outside).clamp(0.0, 1.0);
            let g = if i >= 5 && j >= 5 && i < 5 + inner && j < 5 + inner {
                glyph[((j - 5) * inner + (i - 5)) as usize] as f64 / 255.0
            } else {
                0.0
            };
            let mix = |c: usize| (gold[c] as f64 * (1.0 - g) + ON_GOLD[c] as f64 * g).round() as u8;
            rgba.extend_from_slice(&[mix(0), mix(1), mix(2), (square * 255.0).round() as u8]);
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT: every phase has its bar, and the bar is round 19's: idle hides it, listening,
    /// writing and done are states 14 to 16, a problem shows its own line.
    #[test]
    fn each_phase_shows_its_bar() {
        assert_eq!(view_for(Phase::Idle), BarView::Hidden);
        assert_eq!(view_for(Phase::Listening { started_ms: 5 }), BarView::Listening);
        assert_eq!(view_for(Phase::Writing), BarView::Writing);
        assert_eq!(view_for(Phase::Done), BarView::Added);
        assert_eq!(view_for(Phase::Problem(Problem::NoSound)), BarView::Problem(Problem::NoSound));
        let tags: Vec<_> = [BarView::Hidden, BarView::Listening, BarView::Writing, BarView::Added, BarView::Problem(Problem::NoSound), BarView::Off]
            .iter()
            .map(|v| v.tag())
            .collect();
        assert_eq!(tags, ["hidden", "listening", "writing", "added", "problem", "off"]);
    }

    /// INVARIANT: round 19's timers, to the millisecond: Added 1.5 s; the lines that hand him
    /// something to do 5.2 s, the rest 4.2 s; listening and writing never time out.
    #[test]
    fn how_long_each_bar_stays() {
        assert_eq!(hide_after(BarView::Added), Some(Duration::from_millis(1500)));
        assert_eq!(hide_after(BarView::Off), Some(Duration::from_millis(2800)));
        assert_eq!(hide_after(BarView::Listening), None);
        assert_eq!(hide_after(BarView::Writing), None);
        assert_eq!(hide_after(BarView::Hidden), None);
        for p in [Problem::NoTextBox, Problem::NoAccessibility, Problem::ModelMissing, Problem::CouldNotWrite] {
            assert_eq!(hide_after(BarView::Problem(p)), Some(Duration::from_millis(5200)), "{p:?}");
        }
        for p in [Problem::DidNotCatch, Problem::NoSound, Problem::NoMicrophone, Problem::VoiceStillListening] {
            assert_eq!(hide_after(BarView::Problem(p)), Some(Duration::from_millis(4200)), "{p:?}");
        }
    }

    /// INVARIANT: Fix it is on the microphone and Accessibility bars only, and the bar takes the
    /// mouse only while one of those is up, so every other click goes to the app beneath.
    #[test]
    fn only_a_fix_it_bar_takes_the_mouse() {
        assert!(has_fix(Problem::NoMicrophone));
        assert!(has_fix(Problem::NoAccessibility));
        for p in [Problem::DidNotCatch, Problem::NoSound, Problem::NoTextBox, Problem::ModelMissing, Problem::CouldNotWrite, Problem::VoiceStillListening] {
            assert!(!has_fix(p), "{p:?}");
            assert!(!takes_mouse(BarView::Problem(p)), "{p:?}");
        }
        assert!(takes_mouse(BarView::Problem(Problem::NoAccessibility)));
        assert!(takes_mouse(BarView::Listening), "Finish and Cancel are available while recording");
        assert!(takes_mouse(BarView::Delivery), "Recover is available on failed-delivery notices");
        for v in [BarView::Hidden, BarView::Writing, BarView::Added, BarView::Off] {
            assert!(!takes_mouse(v), "{v:?}");
        }
    }

    /// INVARIANT (plan row 14): at most 30 meter updates a second. The capture delivers one
    /// frame every 256 x 1000 / 16000 = 16 ms, so the gate passes every third frame, 48 ms
    /// apart: 0, 48, ..., 960 is 21 updates in the first second, 20.8 a second. Never two
    /// closer than 34 ms, whatever the frame rhythm.
    #[test]
    fn the_meter_moves_at_most_thirty_times_a_second() {
        let frame_ms = (crate::vad::VAD_FRAME_SAMPLES as u64 * 1000) / u64::from(crate::vad::SAMPLE_RATE);
        assert_eq!(frame_ms, 16);
        let mut gate = MeterGate::default();
        let passed: Vec<u64> = (0..1000u64).step_by(frame_ms as usize).filter(|t| gate.due(*t)).collect();
        assert_eq!(passed.len(), 21, "{passed:?}");
        assert!(passed.windows(2).all(|w| w[1] - w[0] == 48));
        let mut any = MeterGate::default();
        let ragged: Vec<u64> = [0u64, 5, 33, 34, 40, 67, 68, 101, 102, 500].into_iter().filter(|t| any.due(*t)).collect();
        assert_eq!(ragged, [0, 34, 68, 102, 500]);
        assert!(ragged.windows(2).all(|w| w[1] - w[0] >= METER_MIN_GAP_MS));
        assert!((1000.0 / METER_MIN_GAP_MS as f64) <= 30.0);
    }

    /// INVARIANT: the bar's meter reads a frame exactly as voice mode's panel does.
    #[test]
    fn the_bar_meter_is_voice_modes_meter() {
        let frame = vec![0.1f32; 256];
        let mut vad = crate::vad::Vad::default();
        vad.push_frame(&frame);
        assert_eq!(meter_level(&frame), vad.level());
        assert_eq!(meter_level(&[0.0; 256]), 0.0);
        assert_eq!(meter_level(&[1.0; 256]), 1.0);
        // -20 dBFS is two thirds of the way up.
        assert!((meter_level(&[0.1; 256]) - 2.0 / 3.0).abs() < 1e-4);
    }

    const MAIN: Rect = Rect { x: 0.0, y: 0.0, w: 1440.0, h: 900.0 };
    const RIGHT: Rect = Rect { x: 1440.0, y: -200.0, w: 1920.0, h: 1080.0 };

    /// INVARIANT (plan section 6): the bar goes on the screen holding the focused window, else
    /// the pointer's, else the primary.
    #[test]
    fn the_bar_follows_the_focused_window_then_the_pointer() {
        let screens = [MAIN, RIGHT];
        let mail = Rect { x: 1600.0, y: 100.0, w: 800.0, h: 600.0 };
        assert_eq!(bar_screen(&screens, Some(mail), Some((10.0, 10.0))), 1);
        assert_eq!(bar_screen(&screens, None, Some((2000.0, 500.0))), 1);
        assert_eq!(bar_screen(&screens, None, Some((10.0, 10.0))), 0);
        assert_eq!(bar_screen(&screens, None, None), 0);
        let nowhere = Rect { x: -5000.0, y: 0.0, w: 10.0, h: 10.0 };
        assert_eq!(bar_screen(&screens, Some(nowhere), Some((2000.0, 500.0))), 1);
    }

    /// INVARIANT: bottom center of the visible area, above the Dock, whole points.
    #[test]
    fn the_bar_sits_bottom_center_above_the_dock() {
        let visible = Rect { x: 0.0, y: 25.0, w: 1440.0, h: 805.0 }; // menu bar 25, Dock 70
        let f = bar_frame(visible, (400.0, 100.0));
        assert_eq!(f, Rect { x: 520.0, y: 25.0 + 805.0 - 100.0 - BAR_GAP, w: 400.0, h: 100.0 });
        let g = bar_frame(RIGHT, (401.0, 100.0));
        assert_eq!(g.x, (1440.0f64 + (1920.0 - 401.0) / 2.0).round());
    }

    /// INVARIANT: the menu hangs from its item, right edges together, and never leaves the screen.
    #[test]
    fn the_menu_hangs_under_its_item() {
        let visible = Rect { x: 0.0, y: 25.0, w: 1440.0, h: 805.0 };
        let item = Rect { x: 1100.0, y: 0.0, w: 30.0, h: 24.0 };
        assert_eq!(menu_frame(item, visible, (330.0, 300.0)), Rect { x: 800.0, y: 28.0, w: 330.0, h: 300.0 });
        let near_left = Rect { x: 40.0, y: 0.0, w: 30.0, h: 24.0 };
        assert_eq!(menu_frame(near_left, visible, (330.0, 300.0)).x, 8.0);
        let past_right = Rect { x: 1430.0, y: 0.0, w: 30.0, h: 24.0 };
        assert_eq!(menu_frame(past_right, visible, (330.0, 300.0)).x, 1440.0 - 330.0 - 8.0);
    }

    /// INVARIANT: the two coordinate spaces convert both ways without loss.
    #[test]
    fn top_left_and_bottom_left_agree() {
        let r = Rect { x: 520.0, y: 718.0, w: 400.0, h: 100.0 };
        assert_eq!(to_cocoa(r, 900.0), (520.0, 82.0, 400.0, 100.0));
        assert_eq!(from_cocoa(to_cocoa(r, 900.0), 900.0), r);
        assert_eq!(within(Rect { x: 1500.0, y: 300.0, w: 50.0, h: 20.0 }, RIGHT), Rect { x: 60.0, y: 500.0, w: 50.0, h: 20.0 });
    }

    /// INVARIANT: the words' range is counted in UTF-16 units, as Accessibility counts, so a
    /// thumbs-up is two; the paste has landed when the cursor sits right after them.
    #[test]
    fn where_the_words_landed() {
        assert_eq!(utf16_len("Talk soon."), 10);
        assert_eq!(utf16_len("ok \u{1F44D}"), 5);
        assert_eq!(words_range(12, " Talk soon."), (12, 11));
        assert!(paste_landed(12, " Talk soon.", 23));
        assert!(!paste_landed(12, " Talk soon.", 12), "not yet");
        let screen = MAIN;
        assert!(worth_lighting(Rect { x: 100.0, y: 200.0, w: 180.0, h: 18.0 }, screen));
        assert!(!worth_lighting(Rect { x: 100.0, y: 200.0, w: 0.0, h: 18.0 }, screen));
        assert!(!worth_lighting(Rect { x: 0.0, y: 0.0, w: 1440.0, h: 800.0 }, screen), "a whole text view");
        assert!(!worth_lighting(Rect { x: 3000.0, y: 200.0, w: 50.0, h: 18.0 }, screen), "off this screen");
    }

    /// INVARIANT: the menu's two rows name the two models, and the tick is derived from the model.
    #[test]
    fn the_accuracy_rows() {
        assert_eq!(model_for_choice("accurate"), Some("large-v3-turbo-q5_0"));
        assert_eq!(model_for_choice("fast"), Some("small.en"));
        assert_eq!(model_for_choice("medium"), None);
        assert_eq!(choice_for_model("small.en"), "fast");
        assert_eq!(choice_for_model("large-v3-turbo-q5_0"), "accurate");
        assert_eq!(choice_for_model("something-else"), "accurate");
    }

    /// INVARIANT: the microphone is drawn: ink on its strokes, clear inside the capsule and
    /// around the glyph, and symmetric about its stem.
    #[test]
    fn the_microphone_picture() {
        let m = mic_mask(ICON_PX);
        assert_eq!(m.len(), 36 * 36);
        let at = |x: u32, y: u32| m[(y * 36 + x) as usize];
        // Units to pixels: 1.5 px per unit.
        assert_eq!(at(18, 1), 0, "above the glyph");
        assert!(at(17, 5) > 200 || at(18, 5) > 200, "the capsule's top stroke (y = 3 units)");
        assert_eq!(at(18, 13), 0, "inside the capsule");
        assert!(at(18, 29) > 200, "the stem (y = 19.5 units)");
        assert!(at(4, 2) == 0 && at(33, 34) == 0, "the corners are clear");
        for y in 0..36 {
            for x in 0..18 {
                let (a, b) = (at(x, y) as i32, at(35 - x, y) as i32);
                assert!((a - b).abs() <= 32, "asymmetric at ({x},{y}): {a} vs {b}");
            }
        }
        let t = template_icon();
        assert_eq!(t.len(), 36 * 36 * 4);
        assert!(t.chunks(4).all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0), "a template is black, shaped by alpha");
    }

    /// INVARIANT: listening is a gold square with the microphone in `--on-gold`: gold at the
    /// edge, ink on the stem, and the gold follows the menu bar's appearance.
    #[test]
    fn the_listening_picture_is_gold() {
        for (dark, gold) in [(true, GOLD_DARK), (false, GOLD_LIGHT)] {
            let p = listening_icon(dark);
            let px = |x: u32, y: u32| &p[((y * 36 + x) * 4) as usize..((y * 36 + x) * 4 + 4) as usize];
            assert_eq!(&px(18, 2)[..3], &gold[..], "gold at the top edge");
            assert_eq!(px(18, 2)[3], 255);
            assert_eq!(px(0, 0)[3], 0, "the corner is rounded off");
            // The stem: glyph row 19.5 units of 26 px = 21 px, plus the 5 px inset.
            let stem = px(18, 26);
            assert!(stem[0] < 0x40, "on-gold ink on the stem: {stem:?}");
        }
    }
}
