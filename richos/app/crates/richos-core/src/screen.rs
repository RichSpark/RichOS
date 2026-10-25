//! **WAITING FOR THE SCREEN** — the CEO's ruling §56, and the one thing in this file that is
//! a refusal rather than a feature.
//!
//! `richos-hq/wiki/ceo-decisions.md` §56 (2026-09-18), verbatim: *"I like that "Watch for the
//! Mac's screen to unlock" feature that you had running here. Should be added to the RichOS
//! app for automatic use when Rich needs it."*
//!
//! What he saw: candidate .8's walk could not start because the screen locked minutes after
//! launch, and a watcher outside the app — `ioreg -n Root -d1 -a | grep
//! CGSSessionScreenIsLocked`, polled every 10 s, exit on unlock — noticed the unlock and the
//! walk was relaunched. **The acceptance is his sentence: when Rich needs the screen and it is
//! locked, the app waits for the unlock on its own and carries on.**
//!
//! ## THE DECISION IS HERE AND THE READING IS IN THE SHELL
//!
//! Exactly the split [`crate::work_gate`] uses, for exactly its reason: `app/src-tauri`
//! carries the whole webview dependency tree and is deliberately detached from this
//! workspace, so a decision that lived there would be a decision this crate's test suite
//! could not reach. This module holds no syscall, links no framework and opens no file. It
//! takes readings through [`ScreenSource`] and decides what to do about them, and every rule
//! below is a unit test against a [`FakeScreen`].
//!
//! The real reader is `app/src-tauri/src/screen.rs`.
//!
//! ## THE POLARITY RULE, AND IT IS THE OPPOSITE OF THE UPDATE GATE'S
//!
//! [`crate::work_gate`] fails TOWARD waiting: there, a delayed update is a nuisance and
//! destroyed work is the defect, so `Unknown` blocks exactly as `Busy` does.
//!
//! **Here it is inverted, and the inversion is deliberate rather than an oversight.** A wait
//! for the screen has no deadline of its own — §56 is *"waits for the unlock"*, with no
//! timeout — so a reading this build cannot establish must NEVER block, or a Windows box, a
//! headless CI host or a future macOS that renames the key would strand every screen-bound
//! assignment forever with a sentence saying it is about to carry on. **Waiting is the defect
//! here, and the honest failure is to proceed.** So:
//!
//! | reading | blocks? | why |
//! |---|---|---|
//! | [`Screen::Locked`] | **yes** | positively read, and it is the whole case §56 is about |
//! | [`Screen::Unlocked`] | no | positively read |
//! | [`Screen::Unknown`] | **no** | nothing was established, and an unbounded wait on nothing is worse than trying |
//!
//! `display_asleep` is reported and **does not block** — see [`ScreenReading::display_asleep`].
//!
//! ## NOTHING HERE HAS A TIMER, AND THAT IS STRUCTURAL
//!
//! This module spawns no thread and starts no
//! background poll. There is no `std::thread::spawn` in this file. A sample is taken only
//! inside [`ScreenWatch::wait_for_screen`], by the thread that is doing the waiting — so an
//! app with nothing screen-bound outstanding does not read the screen at all, ever, and
//! [`ScreenWatch::samples`] stays at zero. `nothing_polls_the_screen_until_something_waits`
//! pins it.
//!
//! ## THE POLL INTERVAL IS A MEASUREMENT, NOT A PREFERENCE
//!
//! Measured on this Mac on 2026-09-18 against the real reader. **The first figure taken was an
//! outlier and it is left in this table on purpose**, because it is the one the arithmetic
//! below is deliberately built on:
//!
//! ```text
//!   six samples, idle machine, optimized, 2000 readings each
//!       45.9  47.0  47.8  48.9  49.3  58.1 µs        min 45.9, max 58.1
//!   the shipped reader, DEBUG build, two keys + CGDisplayIsAsleep
//!       75.1 µs                                       (src-tauri screen::tests)
//!   one sample taken while cargo was building this crate
//!       298.4 µs                                      WORST OBSERVED
//! ```
//!
//! A reading is an IPC round trip to the window server, so its cost tracks how busy that
//! server is — a 6× spread between an idle machine and a loaded one is the finding, and a
//! single number would have hidden it. **The arithmetic therefore uses the worst observed
//! figure and not the typical one**, so the conclusion is an upper bound rather than a
//! best case:
//!
//! ```text
//!   worst observed reading                        298.4 µs      =  0.0002984 s
//!   at SCREEN_POLL = 2 s   0.0002984 / 2       =  0.00014920   =  0.014920 % of one core
//!   at the typical 75.1 µs  0.0000751 / 2      =  0.00003755   =  0.003755 % of one core
//!   at the 10 s of the watcher he liked         =  0.00002984   =  0.002984 % of one core
//!   worst-case added latency                    =  one interval =  2 s
//! ```
//!
//! So 2 s costs **at most** 0.0149 % of one core, and about 0.0038 % in practice, **while a
//! wait is outstanding and nothing at all otherwise** — at a fifth of the latency of the
//! 10-second watcher he described as the thing he liked. That is the whole argument for the
//! number, and it survives the reading being six times more expensive than it usually is.
//!
//! ## WHY THERE IS NO DISTRIBUTED-NOTIFICATION OBSERVER, STATED RATHER THAN LEFT AS A GAP
//!
//! The obvious alternative is `com.apple.screenIsLocked` / `com.apple.screenIsUnlocked` on
//! `NSDistributedNotificationCenter`. It is **deliberately not built**, and the arithmetic
//! above is the reason:
//!
//! 1. **It can buy at most 2 s of latency** against a cost of 0.0149 % of one core, and only
//!    while something is already waiting.
//! 2. **It cannot be the mechanism, only an optimization.** Those notifications are posted by
//!    `loginwindow` and are not delivered for every path into a locked session, which is why
//!    the brief that asked for them asked for a poll backstop in the same sentence. A
//!    mechanism that needs a backstop to be correct is the backstop.
//! 3. **It would cost real unsafety for that 2 s**: a block-based observer, retained across
//!    threads, with an `NSOperationQueue` and a run loop, inside a `Send + Sync` source.
//!
//! A poll that is already correct does not get an unreliable fast path bolted onto it. If the
//! 2 s is ever shown to matter, the interval is one constant.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Whether the Mac's screen is locked, **or whether that could not be established at all.**
///
/// Three values and not a `bool`, for the reason the whole of this project keeps saying in
/// other modules: absence of an answer is not an answer. A `bool` here would force every
/// caller to pick a lie for the platform it has not got a reader for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Positively read: there is a GUI session and it is not locked.
    Unlocked,
    /// Positively read: the session is locked. **The one reading that waits.**
    Locked,
    /// **Nothing was established.** No reader on this platform, no GUI session for this
    /// process, or a session dictionary that did not answer. It never blocks — see this
    /// module's polarity table.
    Unknown,
}

impl Screen {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unlocked => "unlocked",
            Self::Locked => "locked",
            Self::Unknown => "unknown",
        }
    }
}

/// One reading of the screen, with the two axes kept apart because they are two facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenReading {
    pub screen: Screen,
    /// Whether the main display is asleep, or `None` when that was not established.
    ///
    /// **It is reported and it does NOT block, and that is a narrow, deliberate call.** A
    /// sleeping display on an unlocked session is still a usable GUI session — the window
    /// server is up and a window can be shown — and this Mac's own configuration is the
    /// reason the distinction matters rather than being academic: `sysadminctl -screenLock
    /// status` reports a **300-second** delay, so for five minutes after the display sleeps
    /// the session is asleep and **not locked**. Blocking on sleep would therefore make the
    /// app wait through five minutes in which it could have worked.
    ///
    /// Nothing here has measured a screen-bound operation failing against a sleeping display,
    /// and a block on an unmeasured suspicion is how an unbounded wait gets built. If one is
    /// ever measured, it belongs in [`ScreenReading::blocks`] with the measurement beside it.
    pub display_asleep: Option<bool>,
}

impl ScreenReading {
    /// A positively-read unlocked session.
    pub fn unlocked() -> Self {
        ScreenReading { screen: Screen::Unlocked, display_asleep: Some(false) }
    }
    /// A positively-read locked session.
    pub fn locked() -> Self {
        ScreenReading { screen: Screen::Locked, display_asleep: None }
    }
    /// Nothing established. The honest reading for a platform with no reader.
    pub fn unknown() -> Self {
        ScreenReading { screen: Screen::Unknown, display_asleep: None }
    }

    /// **Does this reading make something screen-bound wait?** Only [`Screen::Locked`] does.
    pub fn blocks(&self) -> bool {
        matches!(self.screen, Screen::Locked)
    }

    /// The inverse, for callers that read better this way round.
    pub fn usable(&self) -> bool {
        !self.blocks()
    }
}

/// How the screen is read. Implemented by the shell (`src-tauri/src/screen.rs`) so this crate
/// keeps no framework link and no opinion about an operating system.
///
/// `Send + Sync` because the work host reads one from its runner threads; every
/// implementation is stateless or internally synchronized, and a reading is a value.
pub trait ScreenSource: Send + Sync {
    fn read(&self) -> ScreenReading;
}

/// **The default, and it is honest rather than optimistic.** A build with no reader installed
/// says it does not know, and because `Unknown` never blocks, nothing screen-bound is ever
/// stranded by the absence of a reader.
///
/// This is the same shape as [`crate::recovery::UnreadableRepositories`]: the injected
/// fallback states that it could not look, instead of implying it did.
pub struct UnknownScreen;

impl ScreenSource for UnknownScreen {
    fn read(&self) -> ScreenReading {
        ScreenReading::unknown()
    }
}

/// A screen this suite can drive. Counts its readings, because half the properties worth
/// asserting about a poll are about HOW MANY TIMES it looked.
pub struct FakeScreen {
    reading: Mutex<ScreenReading>,
    reads: AtomicU64,
    /// Optional: flip to this reading once `reads` reaches `after`. How a transition is
    /// staged without a second thread and without a sleep.
    then: Mutex<Option<(u64, ScreenReading)>>,
}

impl FakeScreen {
    pub fn new(reading: ScreenReading) -> Arc<Self> {
        Arc::new(FakeScreen {
            reading: Mutex::new(reading),
            reads: AtomicU64::new(0),
            then: Mutex::new(None),
        })
    }
    pub fn locked() -> Arc<Self> {
        Self::new(ScreenReading::locked())
    }
    pub fn unlocked() -> Arc<Self> {
        Self::new(ScreenReading::unlocked())
    }
    pub fn unknown() -> Arc<Self> {
        Self::new(ScreenReading::unknown())
    }
    /// Become `reading` on the `after`-th reading (1-based), so a lock/unlock transition is a
    /// deterministic fact rather than a race with a sleeping test.
    pub fn flips_to(self: &Arc<Self>, after: u64, reading: ScreenReading) -> Arc<Self> {
        *self.then.lock().unwrap() = Some((after, reading));
        Arc::clone(self)
    }
    pub fn set(&self, reading: ScreenReading) {
        *self.reading.lock().unwrap() = reading;
    }
    pub fn reads(&self) -> u64 {
        self.reads.load(Ordering::SeqCst)
    }
}

impl ScreenSource for FakeScreen {
    fn read(&self) -> ScreenReading {
        let n = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
        let mut current = self.reading.lock().unwrap();
        let flip = *self.then.lock().unwrap();
        if let Some((after, reading)) = flip {
            if n >= after {
                *current = reading;
            }
        }
        *current
    }
}

/// **The shipped poll interval.** Its whole justification is the measured arithmetic in this
/// module's own documentation: at the WORST observed reading cost — 298.4 µs, six times the
/// typical 45.9–58.1 µs and taken while the machine was building — a wait costs 0.0149 % of
/// one core, and nothing at all when no wait is outstanding.
pub const SCREEN_POLL: Duration = Duration::from_secs(2);

/// How a wait ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitOutcome {
    /// The screen is usable. `reading` is the one that ended the wait.
    Available { reading: ScreenReading, samples: u64 },
    /// **The app is stopping, or this assignment was stopped.** The screen was still locked.
    /// A caller must never read this as availability — that is what the two variants are for.
    Stopped { samples: u64 },
}

impl WaitOutcome {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available { .. })
    }
    pub fn samples(&self) -> u64 {
        match self {
            Self::Available { samples, .. } | Self::Stopped { samples } => *samples,
        }
    }
}

/// **The wait.** One source, one interval, and a stop that can always reach a waiter.
///
/// It is not a watcher in the background sense: see this module's *nothing here has a timer*.
pub struct ScreenWatch {
    source: Arc<dyn ScreenSource>,
    poll: Duration,
    /// Set once, by [`Self::stop`]. Every waiter wakes and returns [`WaitOutcome::Stopped`].
    ///
    /// **A quit that could not reach a waiting thread would be the defect this field exists
    /// for.** §56 gives the wait no timeout, so without a positive stop signal a locked
    /// screen would make the app unquittable — the exact shape of the `confirm_quit_and_stop`
    /// hazard `src-tauri/src/main.rs` already guards against on the other lease.
    stopped: Mutex<bool>,
    wake: Condvar,
    samples: AtomicU64,
}

impl ScreenWatch {
    pub fn new(source: Arc<dyn ScreenSource>) -> Arc<Self> {
        Self::with_poll(source, SCREEN_POLL)
    }

    /// The same thing with a stated interval. Used by this suite so a transition costs
    /// milliseconds instead of seconds; a test that slept 2 s per sample is a test nobody
    /// runs, which is the reasoning `ui/tests/waiting-state.js` already records for its clock.
    pub fn with_poll(source: Arc<dyn ScreenSource>, poll: Duration) -> Arc<Self> {
        Arc::new(ScreenWatch {
            source,
            poll: poll.max(Duration::from_millis(1)),
            stopped: Mutex::new(false),
            wake: Condvar::new(),
            samples: AtomicU64::new(0),
        })
    }

    /// A build with no reader. Reads [`Screen::Unknown`], so it never blocks anything.
    pub fn unknown() -> Arc<Self> {
        Self::new(Arc::new(UnknownScreen))
    }

    /// One reading, now. Counts as a sample.
    pub fn read(&self) -> ScreenReading {
        self.samples.fetch_add(1, Ordering::SeqCst);
        self.source.read()
    }

    /// How many times the screen has been read through this watch. The number the
    /// no-timer property is asserted on.
    pub fn samples(&self) -> u64 {
        self.samples.load(Ordering::SeqCst)
    }

    pub fn is_stopped(&self) -> bool {
        *self.stopped.lock().unwrap()
    }

    /// **Stop every wait, now.** Idempotent, and it wakes waiters rather than relying on the
    /// next poll tick — a quit must not have to wait out an interval.
    pub fn stop(&self) {
        *self.stopped.lock().unwrap() = true;
        self.wake.notify_all();
    }

    /// Let a stopped watch be used again. The work host's stop is per assignment, and the
    /// next assignment must not inherit the last one's stop.
    pub fn resume(&self) {
        *self.stopped.lock().unwrap() = false;
    }

    /// **Block until the screen is usable, or until [`Self::stop`].**
    ///
    /// Returns immediately — with exactly one sample — when the first reading does not
    /// block, which is the ordinary case and the reason an unlocked Mac pays nothing for this
    /// feature existing.
    ///
    /// `on_wait` is called ONCE, before the first sleep, and only when the wait is really
    /// going to happen. That is what writes the durable state and puts the sentence on his
    /// timeline, and it is a callback rather than a return value because the state has to be
    /// visible DURING the wait, not after it.
    pub fn wait_for_screen(&self, on_wait: impl FnOnce(ScreenReading)) -> WaitOutcome {
        let first = self.read();
        if !first.blocks() {
            return WaitOutcome::Available { reading: first, samples: self.samples() };
        }
        if self.is_stopped() {
            return WaitOutcome::Stopped { samples: self.samples() };
        }
        on_wait(first);
        loop {
            // The sleep is a timed condvar wait so `stop` interrupts it immediately.
            let stopped = self.stopped.lock().unwrap();
            if *stopped {
                return WaitOutcome::Stopped { samples: self.samples() };
            }
            let (stopped, _timeout) = self.wake.wait_timeout(stopped, self.poll).unwrap();
            if *stopped {
                return WaitOutcome::Stopped { samples: self.samples() };
            }
            drop(stopped);
            let reading = self.read();
            if !reading.blocks() {
                return WaitOutcome::Available { reading, samples: self.samples() };
            }
        }
    }

    /// The same wait with nothing to announce. For a caller that has already said its piece.
    pub fn wait_quietly(&self) -> WaitOutcome {
        self.wait_for_screen(|_| {})
    }

    /// How long a wait has been outstanding, for a caller that wants to say so. Never used to
    /// END a wait: §56 gives it no timeout, and a clock that could end it would be one.
    pub fn elapsed_since(start: Instant) -> Duration {
        start.elapsed()
    }
}

/// The sentences. **American English, spoken-safe, no identifiers, no counts he cannot act
/// on** — the same contract [`crate::assignment::says`] keeps, and they live together for the
/// same reason.
pub mod says {
    /// **What he sees while a job is waiting for the screen** — the sentence the CEO's own
    /// §56 brief gives, kept verbatim.
    ///
    /// It promises exactly one thing and it is a thing the app controls: that it carries on by
    /// itself. It does not say how long, it does not ask him to do anything, and it is not a
    /// notice — nothing raises it through [`crate::assignment::raise_notice`], because §56 is
    /// a wait that looks after itself and a push about it would be the nag he did not ask for.
    pub fn waiting_for_the_screen() -> &'static str {
        "Waiting for the screen to unlock. I'll carry on the moment it's back."
    }

    /// The durable `detail` on the assignment record. A whole sentence, so it keeps its
    /// period ([`crate::assignment`]'s `sanitize_line` rule).
    pub fn detail() -> &'static str {
        "The screen is locked, so this is waiting for it to unlock. It will carry on by itself."
    }

    /// **What the wait turned out to be, once it is over.** Named separately because a
    /// resumed job must not keep claiming it is waiting.
    pub fn carrying_on() -> &'static str {
        "The screen is back, so this is carrying on."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **THE PROPERTY THE WHOLE MODULE EXISTS FOR.** A locked screen that becomes unlocked
    /// ends the wait by itself, with nobody touching anything.
    #[test]
    fn a_locked_screen_that_unlocks_ends_the_wait_by_itself() {
        // Locked, and the 3rd reading onward is unlocked.
        let screen = FakeScreen::locked().flips_to(3, ScreenReading::unlocked());
        let watch = ScreenWatch::with_poll(screen.clone(), Duration::from_millis(2));
        let mut announced = None;
        let outcome = watch.wait_for_screen(|reading| announced = Some(reading));
        assert!(outcome.is_available(), "the wait must end when the screen unlocks: {outcome:?}");
        // It announced the wait, once, with the reading that caused it.
        assert_eq!(announced.map(|r| r.screen), Some(Screen::Locked));
        // Sample 1 was locked, 2 was locked, 3 was unlocked — so it looked exactly 3 times.
        assert_eq!(outcome.samples(), 3, "one sample per poll and not one more");
        assert_eq!(screen.reads(), 3);
    }

    /// An unlocked screen costs ONE reading and announces nothing. The ordinary case pays
    /// nothing for this feature existing.
    #[test]
    fn an_unlocked_screen_never_waits_and_never_says_anything() {
        let screen = FakeScreen::unlocked();
        let watch = ScreenWatch::with_poll(screen.clone(), Duration::from_millis(2));
        let mut announced = false;
        let outcome = watch.wait_for_screen(|_| announced = true);
        assert!(outcome.is_available());
        assert_eq!(outcome.samples(), 1, "an unlocked screen is one reading");
        assert!(!announced, "nothing waited, so nothing may be said about waiting");
    }

    /// **THE POLARITY RULE, and it is the inverse of the update gate's on purpose.** A
    /// reading that established nothing must not block, or a platform with no reader strands
    /// every screen-bound assignment forever.
    #[test]
    fn an_unknown_reading_never_blocks_because_an_unbounded_wait_is_the_worse_failure() {
        let watch = ScreenWatch::with_poll(FakeScreen::unknown(), Duration::from_millis(2));
        let mut announced = false;
        let outcome = watch.wait_for_screen(|_| announced = true);
        assert!(outcome.is_available(), "Unknown must proceed, never wait: {outcome:?}");
        assert_eq!(outcome.samples(), 1);
        assert!(!announced);
        // And the default source is exactly this, so a build with no reader is safe by
        // construction rather than by a caller remembering.
        assert!(ScreenWatch::unknown().wait_quietly().is_available());
    }

    /// A sleeping display on an unlocked session is a usable session. This Mac's 300-second
    /// screen-lock delay is why the distinction is load-bearing rather than academic.
    #[test]
    fn a_sleeping_display_on_an_unlocked_session_does_not_block() {
        let reading = ScreenReading { screen: Screen::Unlocked, display_asleep: Some(true) };
        assert!(!reading.blocks());
        assert!(reading.usable());
        let watch = ScreenWatch::with_poll(FakeScreen::new(reading), Duration::from_millis(2));
        assert!(watch.wait_quietly().is_available());
    }

    /// **A locked screen must not make the app unquittable.** §56 gives the wait no timeout,
    /// so the stop signal is the only way out, and it has to reach a thread that is asleep in
    /// a condvar rather than wait out the interval.
    #[test]
    fn a_stop_reaches_a_waiting_thread_and_is_never_reported_as_available() {
        // Locked forever. Only `stop` can end this.
        let watch = ScreenWatch::with_poll(FakeScreen::locked(), Duration::from_secs(3600));
        let stopper = Arc::clone(&watch);
        let handle = std::thread::spawn(move || stopper.wait_quietly());
        // Wait until the waiter is really inside the wait, then stop it.
        while watch.samples() == 0 {
            std::thread::yield_now();
        }
        watch.stop();
        let outcome = handle.join().unwrap();
        assert!(!outcome.is_available(), "a stop must never be reported as availability");
        assert!(matches!(outcome, WaitOutcome::Stopped { .. }), "{outcome:?}");
        assert!(watch.is_stopped());
    }

    /// A stop already in force is honored before the wait begins, and `resume` gives the
    /// next assignment a clean watch rather than the last one's stop.
    #[test]
    fn a_stop_in_force_is_honored_up_front_and_resume_clears_it() {
        let watch = ScreenWatch::with_poll(FakeScreen::locked(), Duration::from_millis(2));
        watch.stop();
        assert!(matches!(watch.wait_quietly(), WaitOutcome::Stopped { .. }));
        watch.resume();
        assert!(!watch.is_stopped());
        // Still locked, so it waits again rather than inheriting the stop as availability.
        let screen = FakeScreen::locked().flips_to(2, ScreenReading::unlocked());
        let second = ScreenWatch::with_poll(screen, Duration::from_millis(2));
        assert!(second.wait_quietly().is_available());
    }

    /// **NOTHING POLLS THE SCREEN UNTIL SOMETHING WAITS.** The no-timer property, asserted
    /// rather than described: constructing a watch, and letting real time pass, reads nothing.
    #[test]
    fn nothing_polls_the_screen_until_something_waits() {
        let screen = FakeScreen::locked();
        let watch = ScreenWatch::with_poll(screen.clone(), Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(screen.reads(), 0, "a watch nobody is waiting on must not read the screen");
        assert_eq!(watch.samples(), 0);
        // And there is no thread to have done it: this module contains no spawn. The
        // negative control for that claim is the source itself, asserted in
        // `the_module_spawns_nothing` below.
    }

    /// The negative control for the claim above, because "we do not spawn" is the kind of
    /// claim that quietly stops being true. It reads THIS file.
    ///
    /// **It checks CODE and not prose, and it had to be taught the difference the hard way.**
    /// The first version read the whole production half and failed on this module's own
    /// documentation, which says *"There is no `std::thread::spawn` in this file"* — a true
    /// sentence, flagged as the thing it was denying. So comment lines are stripped, and the
    /// failure is kept in this doc because it is the proof this assertion can go red at all.
    #[test]
    fn the_module_spawns_nothing_outside_its_own_tests() {
        let source = include_str!("screen.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let code: String = production
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // The negative control on the negative control: the phrase really is in the prose, so
        // a stripper that stripped nothing would be caught here rather than pass silently.
        assert!(production.contains("thread::spawn"), "this module's prose names the phrase");
        assert!(
            !code.contains("thread::spawn"),
            "the production half of screen.rs must start no thread: the poll belongs to the \
             thread that is waiting, so an app with nothing outstanding reads nothing"
        );
    }

    /// The reading vocabulary says what it means, and `Unknown` is not spelled like either
    /// of the answers it is not.
    #[test]
    fn the_three_readings_are_named_apart() {
        assert_eq!(Screen::Unlocked.as_str(), "unlocked");
        assert_eq!(Screen::Locked.as_str(), "locked");
        assert_eq!(Screen::Unknown.as_str(), "unknown");
        assert!(ScreenReading::locked().blocks());
        assert!(!ScreenReading::unlocked().blocks());
        assert!(!ScreenReading::unknown().blocks());
    }

    /// The sentences are his, spoken-safe, and carry no identifier or number.
    #[test]
    fn the_sentences_survive_being_spoken() {
        for sentence in [says::waiting_for_the_screen(), says::detail(), says::carrying_on()] {
            assert!(!sentence.is_empty());
            assert!(
                !sentence.chars().any(|c| c.is_ascii_digit()),
                "no counts he cannot act on: {sentence}"
            );
            assert!(!sentence.contains('_'), "no identifiers: {sentence}");
            assert!(
                sentence.ends_with('.') || sentence.ends_with('!'),
                "a whole sentence keeps its own end mark: {sentence}"
            );
        }
        // The CEO's own sentence, verbatim from the §56 brief.
        assert_eq!(
            says::waiting_for_the_screen(),
            "Waiting for the screen to unlock. I'll carry on the moment it's back."
        );
        // `ui/timeline.js`'s SCREEN_BACK_DETAIL reads this exact record detail to put "carrying
        // on" on the timeline: a change here must change that line with it.
        assert_eq!(says::carrying_on(), "The screen is back, so this is carrying on.");
    }

    /// The measured arithmetic this module's poll interval rests on, recomputed here so a
    /// change to the constant has to face the number that justified it.
    ///
    /// **It is computed from the WORST reading observed, not the typical one**, so the
    /// conclusion is an upper bound. A reading is an IPC round trip to the window server and
    /// its cost tracks that server's load: 45.9–58.1 µs idle, 75.1 µs through the shipped
    /// reader in a debug build, and 298.4 µs once while cargo was building this crate.
    #[test]
    fn the_poll_interval_is_the_measurement_it_claims_to_be() {
        // The worst of eight real samples taken on 2026-09-18, each over 2000 readings.
        let worst_observed = Duration::from_nanos(298_400);
        let share = worst_observed.as_secs_f64() / SCREEN_POLL.as_secs_f64();
        // 0.0002984 / 2 = 0.0001492 = 0.01492 % of one core.
        assert!(
            (share - 0.000_149_2).abs() < 1e-9,
            "the poll's cost share moved: {share} of one core per waiting thread"
        );
        assert!(share < 0.001, "a wait must cost well under a tenth of a percent of a core");
        // The typical case, for the same interval, is about a quarter of that.
        let typical = Duration::from_nanos(75_100).as_secs_f64() / SCREEN_POLL.as_secs_f64();
        assert!(typical < share, "the typical cost must be below the bound the constant uses");
        assert!((typical - 0.000_037_55).abs() < 1e-9, "typical share moved: {typical}");
        // And it is a fifth of the latency of the 10-second watcher the CEO said he liked.
        assert_eq!(SCREEN_POLL, Duration::from_secs(2));
    }
}
