//! IS RICHOS DOING ANYTHING RIGHT NOW — the one question the updater never asked.
//!
//! # What this exists to stop
//!
//! Verified in the shipped source on 2026-09-05, and it was three missing checks rather
//! than one. `updates.rs`'s `install()` took the pending update and began downloading with
//! no test of turn state; `ui/updates.js` wired the Install button straight to the command,
//! so nothing was disabled and nothing warned; and `update_relaunch()` was `app.restart()`
//! and nothing else — two lines, no condition. **Pressed mid-turn, the third one replaces
//! the process while a `claude` child is mid-answer.**
//!
//! The CEO's decision, in his words: *"Yes, offer to wait and install when all work is
//! finished."* **ALL WORK**, not the current turn — RichOS runs workers alongside the
//! conversation, and a worker still running is still work. And the sentence that ranks it
//! against everything else: *"The update is not important enough to get in the way of
//! finishing work."*
//!
//! # Why the decision is HERE and the reading is in the shell
//!
//! This module holds no handles, opens no files and calls no syscalls. It takes three
//! already-measured [`Liveness`] readings and returns [`WorkVerdict`]. That split is not
//! tidiness: `app/src-tauri` carries the whole webview dependency tree and is deliberately
//! detached from the workspace, so a decision that lived there would be a decision the
//! 927-test spine suite could not reach. Every rule below is unit-tested against a table of
//! constructed readings, including the ones a real machine produces about twice a year.
//!
//! # THE THREE SOURCES, AND WHY EACH IS THE ONE IT IS
//!
//! **1. Any active turn.** `TurnControl::has_active_turns()` — a MIRROR of the spine's
//! `turn_in_progress`, written at `spine.rs:1475` beside `mark_turn_started` and cleared at
//! `spine.rs:1609` beside the terminal event. It is readable WITHOUT the spine's mutex,
//! which is the only reason this is possible at all: `send_message` holds that mutex for the
//! entire length of a turn (`main.rs:448`), so a gate that locked the spine to ask whether a
//! turn was running would get its answer after the turn it meant to protect had ended. It is
//! never inferred from silence or from activity (continuity §5.2).
//!
//! **2. Whether the spine is being driven at all.** The mirror above has a REAL GAP and it
//! was found by reading the drain path rather than by guessing: `submit_prompt`
//! (`spine.rs:1181`) runs `deliver` → `after_turn_boundary` → `drain_queue` inside ONE call,
//! and `end_turn` fires at `spine.rs:1609` — before the closing runs, the last worker
//! re-join, the terminal status emit, `settle_stop_claim` and `drain_intake`, and well
//! before the NEXT queued turn's `begin_turn`. Between two queued turns `active_turn()` is
//! genuinely `None` while work is genuinely continuing, and every one of those steps does
//! file I/O, so the window is milliseconds of wall clock rather than nanoseconds. A poller
//! CAN land in it. The shell closes that window without touching the spine at all: it
//! `try_lock`s the spine mutex, and a lock it cannot take means someone is inside a spine
//! call — which is [`Liveness::Unknown`], never [`Liveness::Clear`]. Conservative by
//! construction: a 200µs `threads()` call costs one extra poll of waiting, and waiting is
//! never the defect.
//!
//! **4. BACKGROUND WORK — the second lease, and the hole this module had until 2026-09-17.**
//! The background-work spec §6.4 names it and refuses to call it a follow-up: *"its worker
//! reading is scoped to the conversation lease … a second lease's session is not that one, so
//! background workers would be INVISIBLE to the update gate, and an update could install over
//! live work — precisely the destroyed-work failure that gate exists to prevent. The update
//! gate must read both leases."* [`background`] is that reading, and it is deliberately wider
//! than the spec's sentence: the work lease's own worker view is one half, and the ASSIGNMENT
//! REGISTER is the other, because an assignment between its registration and its first worker
//! has no workers to see and is still work this app must not be replaced in the middle of.
//!
//! **And it distinguishes two things the spec insists are different sentences** (§6.5): work
//! that is RUNNING, and work that is WAITING FOR HIM. *"'An assignment is running' and 'an
//! assignment is waiting for you' are different sentences, and only one of them is about
//! him."* Both block; they say different things, and the second one names something he can
//! act on in this same app.
//!
//! **3. Workers.** [`crate::worker_status::WorkerStatusView`], whose own arithmetic is
//! liveness-reconciled open runs — a `created`/`started` with no later `run_ended` whose
//! recorded `host_pid` is witnessed alive by a real syscall. [`workers`] below maps that
//! view onto a reading and, crucially, decides which of its refusals are an honest zero and
//! which are an honest "I cannot tell".
//!
//! # FAIL TOWARD WAITING — the rule, and the one place it is deliberately not applied
//!
//! **Ambiguity resolves to waiting, never to installing.** Same shape as the voice gate
//! landed the same day: a dropped turn is a nuisance, a fabricated one is the defect. Here,
//! a delayed update is a nuisance and destroyed work is the defect. So [`Liveness::Unknown`]
//! blocks exactly as [`Liveness::Busy`] does — the two differ only in what the CEO is told.
//!
//! **The exception is named rather than hidden, because an unnamed exception is the failure
//! this whole module is about.** [`Unattributed::NoSession`] means no compute lease is
//! attached. Workers are spawned by the Claude session the app's lease is serving, and their
//! directory is `~/.claude/teams/session-<first8 of that id>`, so with no lease there is no
//! id, no directory, and nothing this app dispatched. It is read as CLEAR **and reported in
//! [`WorkVerdict::unchecked`]**, so the surface says workers were not checked rather than
//! implying they were. Reading it as Unknown instead would mean an app whose `claude` child
//! failed to start — which refuses every message and is exactly the state a customer most
//! needs to update out of — could never update again. That is the "a test that passes
//! because everything now defers" failure, and it is a worse trap than the one being fixed.
//!
//! # WHAT THIS MODULE DOES NOT CLAIM
//!
//! It does not see anything outside this process and this session: a worker belonging to
//! another session, a build running in a terminal, a file the CEO is copying. It has never
//! claimed to. Everything it could not establish is named in `unchecked` and rendered in
//! words, because *"never claim to have waited for something you did not check"* is a
//! requirement of the ruling and not a nicety.

use crate::worker_status::{Unattributed, WorkerStatusView};

/// One source's answer about whether work is live.
///
/// Three values rather than a `bool` because *"nothing is running"* and *"I could not tell
/// whether anything is running"* lead to the same action and to two different sentences, and
/// collapsing them is how a mechanism ends up claiming to have checked something it did not.
/// The same distinction `worker_status` already draws between a zero and an
/// [`Unattributed`], and the same one [`crate::skip::SkipKind::Ambiguous`] draws for a record
/// this build cannot classify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// Positively running. A turn is in flight, or a worker's host PID answered a live probe.
    Busy,
    /// Positively not running, from a signal that says so — never from silence.
    Clear,
    /// Could not be established. Blocks exactly like [`Liveness::Busy`]; says something else.
    Unknown,
}

impl Liveness {
    /// Whether this reading permits acting. `Unknown` does not — fail toward waiting.
    pub fn permits_action(self) -> bool {
        matches!(self, Liveness::Clear)
    }
}

/// The three readings, taken by the shell and decided here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkSources {
    /// `TurnControl::active_turn().is_some()` → [`Liveness::Busy`]. Never `Unknown`: the
    /// control is always present in the shell, and a mutex read cannot fail to answer.
    pub turn: Liveness,
    /// The spine mutex: taken → [`Liveness::Clear`], refused → [`Liveness::Unknown`].
    pub spine: Liveness,
    /// From [`workers`].
    pub workers: Liveness,
    /// What the worker reading could not see, in the CEO's language. Empty when it saw
    /// everything. Carried separately from the reading because a CLEAR worker answer can
    /// still have a gap in it — [`Unattributed::NoSession`] is exactly that case.
    pub worker_gap: Option<String>,
    /// **The second lease** (spec §6.4), from [`background`]. `workers` above reads the
    /// CONVERSATION lease's session and is structurally blind to this one.
    pub background: Liveness,
    /// The sentence naming what background work is doing — running, or waiting for him.
    pub background_gap: Option<String>,
}

impl WorkSources {
    /// Every source clear and nothing unchecked. The state a quiet app is actually in, and
    /// the one the "install immediately when idle" proof asserts against.
    pub fn all_clear() -> Self {
        WorkSources {
            turn: Liveness::Clear,
            spine: Liveness::Clear,
            workers: Liveness::Clear,
            worker_gap: None,
            background: Liveness::Clear,
            background_gap: None,
        }
    }
}

/// What the updater is allowed to do, and the sentence that says why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkVerdict {
    /// `true` = do not download, do not install, do not restart, do not offer the control.
    pub busy: bool,
    /// One sentence, in the CEO's language, naming WHAT is running — or `None` when nothing
    /// is. Never a state name, never a token: this is rendered verbatim on the surface he
    /// opens on purpose, and it is the whole of "the wait must be visible".
    pub reason: Option<String>,
    /// Everything the verdict could NOT establish, one clause each. Rendered as well as the
    /// reason, so the mechanism never implies a check it did not perform. Empty is the
    /// ordinary case and means every source answered.
    pub unchecked: Vec<String>,
}

/// Map a worker view onto a reading plus whatever that reading could not see.
///
/// Each arm is a documented statement of `worker_status`'s own, quoted rather than inferred:
///
/// - **attributed, `active > 0`** — open runs with live hosts. [`Liveness::Busy`], and the
///   count goes in the sentence, because "2 workers are still running" is actionable and
///   "work is running" is not.
/// - **attributed, `liveness_unknown > 0`** — an open run whose host could not be probed
///   (no `host_pid` on the row, EPERM, no `/bin/kill`). The module's own instruction is
///   *"if neither can be established, the honest render is `unknown`, not a count"*, so:
///   [`Liveness::Unknown`].
/// - **attributed, both zero** — [`Liveness::Clear`]. A real directory, read, with nothing
///   open in it.
/// - **[`Unattributed::NoTeamDirForSession`]** — [`Liveness::Clear`]. Not a failure: the
///   engine creates the directory on the first spawn, so its absence is the documented,
///   correct state of a session that has dispatched no workers.
/// - **[`Unattributed::NoSession`]** — [`Liveness::Clear`] **with a gap**. See the module
///   doc; this is the one declared exception to fail-toward-waiting and it is reported in
///   words rather than assumed away.
/// - **[`Unattributed::NoHome`] / [`Unattributed::UnusableSessionId`] /
///   [`Unattributed::OverrideNotADirectory`]** — [`Liveness::Unknown`]. A lease exists (or
///   the filesystem cannot be located at all) and its workers cannot be looked at. Nothing
///   here guesses.
/// - **[`Unattributed::WorkerLogUnreadable`]** — [`Liveness::Unknown`]. The directory is
///   known and its worker log is there, and it could not be read.
/// - **[`Unattributed::WorkerLogDamaged`]** — [`Liveness::Unknown`]. The log was read and a
///   line of it was not a record.
///
/// **The match is exhaustive rather than wildcarded, and that earned its keep on the first
/// compile:** `OverrideNotADirectory` was added to `worker_status` after this file was
/// drafted and a `_ =>` arm would have swallowed it into whichever neighbour it was written
/// beside. An operator who points `RICHOS_TEAM_DIR` at a path that is not a directory must
/// get "I cannot tell", never a silent zero. When a sixth reason is added, this stops
/// compiling — which is the intended cost.
pub fn workers(view: &WorkerStatusView) -> (Liveness, Option<String>) {
    match view.unattributed {
        None => {
            if view.active > 0 {
                let n = view.active;
                let noun = if n == 1 { "worker is" } else { "workers are" };
                return (Liveness::Busy, Some(format!("{n} {noun} still running.")));
            }
            if view.liveness_unknown > 0 {
                let n = view.liveness_unknown;
                let noun = if n == 1 { "worker" } else { "workers" };
                return (
                    Liveness::Unknown,
                    Some(format!(
                        "RichOS could not tell whether {n} {noun} had finished."
                    )),
                );
            }
            (Liveness::Clear, None)
        }
        Some(Unattributed::AppEvidenceUnavailable) => (Liveness::Unknown, Some("RichOS could not read its worker observations.".into())),
        Some(Unattributed::NoTeamDirForSession) => (Liveness::Clear, None),
        Some(Unattributed::NoSession) => (
            Liveness::Clear,
            Some("RichOS is not connected to a session, so it cannot see any workers.".into()),
        ),
        Some(Unattributed::NoHome) => (
            Liveness::Unknown,
            Some("RichOS could not find where workers are recorded.".into()),
        ),
        Some(Unattributed::UnusableSessionId) => (
            Liveness::Unknown,
            Some("RichOS could not tell which workers are its own.".into()),
        ),
        Some(Unattributed::OverrideNotADirectory) => (
            Liveness::Unknown,
            Some("RichOS was pointed at a workers folder that is not there.".into()),
        ),
        // Hunt 2026-09-29 part 1, finding 41: the log is there and nobody read it, so
        // whether anyone is running is not known. Never a silent zero.
        Some(Unattributed::WorkerLogUnreadable) => (
            Liveness::Unknown,
            Some("RichOS could not read its record of which workers are running.".into()),
        ),
        // Finding 41, v2 re-check: part of the log could not be read, and that part may have
        // started a worker or ended one.
        Some(Unattributed::WorkerLogDamaged) => (
            Liveness::Unknown,
            Some("RichOS could not read part of its record of which workers are running.".into()),
        ),
    }
}

/// Apply the existing worker reading to every conversation session.
/// An empty list retains the existing, explicit no-session observation.
pub fn all_workers(views:&[WorkerStatusView])->(Liveness,Option<String>) {
    views.iter().map(workers).reduce(worst).unwrap_or_else(||workers(&WorkerStatusView {
        unattributed:Some(Unattributed::NoSession),..Default::default()
    }))
}

/// What the work host's own records say about background work — counted by the shell,
/// decided here, like every other reading in this module.
///
/// **Two numbers and a flag, because the three answers they produce are different.** A
/// register this build cannot read is an "I cannot tell" and blocks (the same rule as every
/// other source); an assignment holding a question for him is a different sentence from one
/// that is merely running (spec §6.5); and zero of both, from a register that WAS read, is an
/// honest clear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundWork {
    /// Open assignments that are not waiting on a decision of his.
    pub running: usize,
    /// Assignments stopped at a step only he can approve. Still open, not running, and the
    /// thing he can act on.
    ///
    /// **What lands here narrowed on 2026-09-18 and the gate did not have to change, which
    /// is the point of counting it this way.** This number is `open_assignments()` filtered
    /// by whether a request of that assignment's is on the permission desk
    /// (`work_host.rs`'s `background_work`), never by a state word. Spec §7.8's case — a job
    /// held at its land — can no longer occur, because the CEO's ruling §52 grants the land
    /// outright; what reaches this count now is a request the desk would have put in front of
    /// him in a visible turn. A job that ran and did NOT land is neither running nor waiting
    /// on him: it is `failed`, it is not open, and it correctly stops blocking an update.
    pub awaiting_you: usize,
    /// Whether the assignment register could be read at all. `false` is never a zero.
    pub readable: bool,
}

impl BackgroundWork {
    /// A register that was read and had nothing open in it.
    pub fn nothing() -> Self {
        BackgroundWork { running: 0, awaiting_you: 0, readable: true }
    }
}

/// Map background work onto a reading plus the sentence that names it.
///
/// `leases` is **every open back end's own worker view — one per conversation thread**, and
/// empty when none has been opened in this process. The CEO's Two Riches spec puts one
/// back-end Rich behind each thread, so there is no single "the work lease" to read: an
/// update that installed over the second thread's work would destroy it exactly as surely as
/// one that installed over the first's. An empty slice is an honest clear with no gap rather
/// than a [`Unattributed::NoSession`] shrug: with no back end there is no session, and the
/// assignment register above is the authority on whether anything is outstanding.
///
/// **Precedence inside this source: waiting for him speaks before running.** If he has a
/// decision to make, that is the sentence he needs — it is the one he can act on, and it is
/// the reason §6.5 refuses to call both of them busy in the same words.
pub fn background(work: &BackgroundWork, leases: &[WorkerStatusView]) -> (Liveness, Option<String>) {
    if !work.readable {
        return (
            Liveness::Unknown,
            Some("RichOS could not tell whether the work you asked for had finished.".into()),
        );
    }
    if work.awaiting_you > 0 {
        let n = work.awaiting_you;
        return (
            Liveness::Busy,
            Some(if n == 1 {
                "An assignment is waiting for you to approve it.".into()
            } else {
                format!("{n} assignments are waiting for you to approve them.")
            }),
        );
    }
    if work.running > 0 {
        let n = work.running;
        let noun = if n == 1 { "assignment is" } else { "assignments are" };
        return (Liveness::Busy, Some(format!("{n} {noun} still running in the background.")));
    }
    // The register says nothing is open. EVERY back end's own workers are read anyway,
    // because a worker outliving the assignment that started it is exactly the kind of
    // ambiguity this module resolves toward waiting rather than toward installing — and one
    // conversation's leftover worker is as real as another's. The first reading that is not
    // clear decides, so a single busy back end among quiet ones still blocks.
    for view in leases {
        match workers(view) {
            (Liveness::Clear, _) => continue,
            (reading, said) => {
                return (
                    reading,
                    Some(said.unwrap_or_else(|| "Background work is still running.".into())),
                )
            }
        }
    }
    (Liveness::Clear, None)
}

/// **The commands Rich started, on every product lease** (the product reap gap design C5,
/// richos-hq `docs/plans/2026-09-27-product-reap-gap-design.md`). Every lease's supervisor now
/// ends its tool commands with the lease, so an update's relaunch would end a background command
/// the gate did not see: a server he asked for, a long install. His words rank it: *"The update
/// is not important enough to get in the way of finishing work."* So a running command blocks
/// exactly as background work does, read from each live lease's supervisor state file
/// (`lease_commands.rs`).
///
/// **Fail toward waiting:** a live lease whose state file cannot be read is
/// [`Liveness::Unknown`], never a zero. A command that never ends (`tail -f`) holds the gate
/// at "wait"; he can still choose to install, as the gate already allows.
pub fn commands(reading: &crate::lease_commands::CommandsReading) -> (Liveness, Option<String>) {
    match reading.running {
        0 => {}
        1 => return (Liveness::Busy, Some("A command Rich started is still running.".into())),
        n => return (Liveness::Busy, Some(format!("{n} commands Rich started are still running."))),
    }
    if reading.unreadable > 0 {
        return (Liveness::Unknown, Some("RichOS could not tell whether a command Rich started had finished.".into()));
    }
    (Liveness::Clear, None)
}

/// **Two readings of one source, as one**: busy before unknown before clear, each with its own
/// sentence. The update gate reads the product's background work and Rich's commands through it, so
/// neither can hide the other.
pub fn worst(a: (Liveness, Option<String>), b: (Liveness, Option<String>)) -> (Liveness, Option<String>) {
    let rank = |l: Liveness| match l { Liveness::Busy => 2, Liveness::Unknown => 1, Liveness::Clear => 0 };
    if rank(b.0) > rank(a.0) { b } else { a }
}

/// The whole decision, and the only place it is made.
///
/// **Precedence is by what the CEO can act on, not by source order.** A running turn is the
/// thing he is looking at, so it speaks first; workers second, because they are the "all
/// work" half he cannot see in the composer; the spine's own lock last, because it is the
/// one reading with no CEO-facing meaning — it says "RichOS is in the middle of something"
/// and nothing finer, and saying that when a turn is already named would be the same fact
/// twice.
pub fn decide(sources: &WorkSources) -> WorkVerdict {
    let mut unchecked: Vec<String> = Vec::new();
    let mut reason: Option<String> = None;

    // A CLEAR worker reading can still carry a gap — NoSession is exactly that — so the gap
    // is collected before anything decides, and independently of the verdict.
    if let Some(gap) = &sources.worker_gap {
        if sources.workers == Liveness::Clear {
            unchecked.push(gap.clone());
        }
    }

    match sources.turn {
        Liveness::Busy => reason = Some("Rich is working on your last message.".into()),
        // THE SHELL CANNOT PRODUCE THIS TODAY and the arm is still here, with its own
        // sentence, because `no_blocking_verdict_is_ever_silent` walks all 27 combinations
        // and found this one blocking in silence on its first run. A state nothing reaches
        // is one nothing maintains; the day something reaches it, it must not be the state
        // where the update quietly stops working and says nothing about it.
        Liveness::Unknown => {
            reason = Some("RichOS could not tell whether Rich had finished.".into())
        }
        Liveness::Clear => {}
    }

    match sources.workers {
        Liveness::Busy => {
            let said = sources
                .worker_gap
                .clone()
                .unwrap_or_else(|| "Workers are still running.".into());
            reason = Some(match reason {
                Some(first) => format!("{first} {said}"),
                None => said,
            });
        }
        Liveness::Unknown => {
            let said = sources
                .worker_gap
                .clone()
                .unwrap_or_else(|| "RichOS could not tell whether workers had finished.".into());
            reason = Some(match reason {
                Some(first) => format!("{first} {said}"),
                None => said,
            });
        }
        Liveness::Clear => {}
    }

    // **The second lease** (spec §6.4). It speaks after the conversation's own workers and
    // before the spine's lock, because it is work he can act on — and, when it is waiting for
    // him, work only he can move.
    match sources.background {
        Liveness::Busy | Liveness::Unknown => {
            let said = sources
                .background_gap
                .clone()
                .unwrap_or_else(|| "Background work is still running.".into());
            reason = Some(match reason {
                Some(first) => format!("{first} {said}"),
                None => said,
            });
        }
        Liveness::Clear => {}
    }

    // Last, and only when nothing above already named something. "RichOS is in the middle of
    // something" beside "Rich is working on your last message" is one fact wearing two hats.
    if sources.spine != Liveness::Clear && reason.is_none() {
        reason = Some("RichOS is in the middle of something.".into());
    }

    let busy = !sources.turn.permits_action()
        || !sources.spine.permits_action()
        || !sources.workers.permits_action()
        || !sources.background.permits_action();

    WorkVerdict {
        busy,
        reason: if busy { reason } else { None },
        unchecked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(active: usize, unknown: usize) -> WorkerStatusView {
        WorkerStatusView {
            active,
            needs_you: 0,
            items: Vec::new(),
            liveness_unknown: unknown,
            unattributed: None,
            completion_history_unavailable: None,
        }
    }

    // ---- the two proofs the ruling names, and they are two rather than one -------------

    /// **The proof that the fix is a fix.** Nothing is running, so the updater acts at once
    /// and adds no delay of its own — no waiting state, no reason, nothing unchecked.
    #[test]
    fn an_idle_app_is_not_busy_and_carries_no_sentence_at_all() {
        let v = decide(&WorkSources::all_clear());
        assert!(!v.busy);
        assert_eq!(v.reason, None, "an idle app must not invent something to say");
        assert!(v.unchecked.is_empty());
    }

    /// **The proof that the fix is not merely a stall.** A test suite where everything
    /// defers passes for the wrong reason; this is the half that catches it, and the one
    /// above is the half that catches over-blocking.
    #[test]
    fn a_running_turn_blocks_and_names_itself() {
        let v = decide(&WorkSources { turn: Liveness::Busy, ..WorkSources::all_clear() });
        assert!(v.busy);
        assert_eq!(v.reason.as_deref(), Some("Rich is working on your last message."));
    }

    // ---- fail toward waiting -----------------------------------------------------------

    /// UNKNOWN BLOCKS. This is the rule the CEO stated in the same shape as the voice gate:
    /// ambiguity never resolves in favor of installing.
    #[test]
    fn an_unknown_reading_blocks_exactly_as_a_busy_one_does() {
        for probe in [
            WorkSources { spine: Liveness::Unknown, ..WorkSources::all_clear() },
            WorkSources { workers: Liveness::Unknown, ..WorkSources::all_clear() },
            WorkSources { turn: Liveness::Unknown, ..WorkSources::all_clear() },
        ] {
            let v = decide(&probe);
            assert!(v.busy, "an unknown source must block: {probe:?}");
            assert!(v.reason.is_some(), "and it must say why: {probe:?}");
        }
    }

    /// The spine's lock is the LAST thing to speak and never doubles a fact already named.
    /// Its whole job is the window between two queued turns (`spine.rs:1609` clears the
    /// mirror before `drain_queue` starts the next one), where it is the ONLY source that
    /// can see anything.
    #[test]
    fn the_spine_lock_speaks_only_when_nothing_else_has() {
        let alone = decide(&WorkSources { spine: Liveness::Unknown, ..WorkSources::all_clear() });
        assert_eq!(alone.reason.as_deref(), Some("RichOS is in the middle of something."));

        let with_turn = decide(&WorkSources {
            turn: Liveness::Busy,
            spine: Liveness::Unknown,
            ..WorkSources::all_clear()
        });
        assert_eq!(
            with_turn.reason.as_deref(),
            Some("Rich is working on your last message."),
            "the lock must not restate a turn that has already been named"
        );
    }

    // ---- workers: ALL WORK, not just the turn ------------------------------------------

    /// The half of "all work" the composer cannot see. A quiet conversation with two live
    /// workers is BUSY, and the count is in the sentence because a count is actionable.
    #[test]
    fn live_workers_block_a_completely_quiet_conversation() {
        let (l, said) = workers(&view(2, 0));
        assert_eq!(l, Liveness::Busy);
        let v = decide(&WorkSources { workers: l, worker_gap: said, ..WorkSources::all_clear() });
        assert!(v.busy);
        assert_eq!(v.reason.as_deref(), Some("2 workers are still running."));
    }

    #[test]
    fn all_conversation_workers_are_checked_even_when_another_session_is_quiet() {
        assert_eq!(all_workers(&[view(0,0),view(2,0)]).0,Liveness::Busy);
        assert_eq!(all_workers(&[view(2,0),view(0,0)]).0,Liveness::Busy);
        let unknown=WorkerStatusView{unattributed:Some(Unattributed::AppEvidenceUnavailable),..Default::default()};
        assert_eq!(all_workers(&[view(0,0),unknown]).0,Liveness::Unknown);
        assert_eq!(all_workers(&[view(0,0),view(0,0)]),(Liveness::Clear,None));
        assert_eq!(all_workers(&[]),workers(&WorkerStatusView{unattributed:Some(Unattributed::NoSession),..Default::default()}));
    }

    #[test]
    fn one_worker_is_singular_because_a_sentence_a_person_reads_has_to_be_one() {
        let (_, said) = workers(&view(1, 0));
        assert_eq!(said.as_deref(), Some("1 worker is still running."));
    }

    /// A turn AND workers: both are named, in that order, in one sentence. The CEO gets the
    /// whole reason rather than the first clause of it.
    #[test]
    fn a_turn_and_workers_are_both_named_turn_first() {
        let (l, said) = workers(&view(3, 0));
        let v = decide(&WorkSources {
            turn: Liveness::Busy,
            workers: l,
            worker_gap: said,
            ..WorkSources::all_clear()
        });
        assert_eq!(
            v.reason.as_deref(),
            Some("Rich is working on your last message. 3 workers are still running.")
        );
    }

    /// `worker_status`'s own instruction, honored: *"if neither can be established, the
    /// honest render is `unknown`, not a count."* An unprobeable host blocks.
    #[test]
    fn a_worker_whose_host_could_not_be_probed_blocks_rather_than_being_counted_either_way() {
        let (l, said) = workers(&view(0, 1));
        assert_eq!(l, Liveness::Unknown);
        let v = decide(&WorkSources { workers: l, worker_gap: said, ..WorkSources::all_clear() });
        assert!(v.busy);
        assert_eq!(
            v.reason.as_deref(),
            Some("RichOS could not tell whether 1 worker had finished.")
        );
    }

    // ---- the four unattributed arms, one test each, because they disagree ---------------

    /// The ordinary state of a session that has dispatched no workers. A CLEAR that is a
    /// real answer, not a shrug — and therefore nothing unchecked to declare.
    #[test]
    fn no_team_dir_is_an_honest_zero_and_not_a_gap() {
        let v = WorkerStatusView::unattributed(Unattributed::NoTeamDirForSession);
        let (l, said) = workers(&v);
        assert_eq!(l, Liveness::Clear);
        assert_eq!(said, None);
        let d = decide(&WorkSources { workers: l, worker_gap: said, ..WorkSources::all_clear() });
        assert!(!d.busy);
        assert!(d.unchecked.is_empty());
    }

    /// THE ONE DECLARED EXCEPTION. No lease means no session id, no team directory and
    /// nothing this app dispatched — so it does not block, and the surface is told in words
    /// that workers were not checked. An app whose `claude` child failed to start is
    /// exactly the app a customer most needs to update, and blocking it for ever would be a
    /// worse trap than the one being fixed.
    #[test]
    fn no_session_does_not_block_and_says_out_loud_that_workers_were_not_checked() {
        let v = WorkerStatusView::unattributed(Unattributed::NoSession);
        let (l, said) = workers(&v);
        assert_eq!(l, Liveness::Clear);
        let d = decide(&WorkSources { workers: l, worker_gap: said, ..WorkSources::all_clear() });
        assert!(!d.busy, "a lease-less app must still be able to update");
        assert_eq!(
            d.unchecked,
            vec!["RichOS is not connected to a session, so it cannot see any workers."],
            "and it must never imply it looked"
        );
    }

    #[test]
    fn a_session_whose_workers_cannot_be_located_blocks_and_says_which_half_failed() {
        for (reason, sentence) in [
            (Unattributed::NoHome, "RichOS could not find where workers are recorded."),
            (
                Unattributed::UnusableSessionId,
                "RichOS could not tell which workers are its own.",
            ),
            // Caught by the exhaustive match on the first compile of this file rather than
            // by review — see `workers`'s doc. An operator's broken override is an "I
            // cannot tell", never a zero.
            (
                Unattributed::OverrideNotADirectory,
                "RichOS was pointed at a workers folder that is not there.",
            ),
        ] {
            let (l, said) = workers(&WorkerStatusView::unattributed(reason));
            assert_eq!(l, Liveness::Unknown, "{reason:?}");
            let d =
                decide(&WorkSources { workers: l, worker_gap: said, ..WorkSources::all_clear() });
            assert!(d.busy, "{reason:?}");
            assert_eq!(d.reason.as_deref(), Some(sentence));
            // It BLOCKED, so the gap is the reason rather than an unchecked footnote —
            // never both, or the surface says the same thing twice.
            assert!(d.unchecked.is_empty(), "{reason:?}");
        }
    }

    /// A gap on a CLEAR reading is an `unchecked` clause; a gap on a blocking reading is the
    /// REASON. Never both, and never neither.
    #[test]
    fn a_gap_is_either_the_reason_or_an_unchecked_clause_and_never_both() {
        let clear = decide(&WorkSources {
            workers: Liveness::Clear,
            worker_gap: Some("nothing was looked at".into()),
            ..WorkSources::all_clear()
        });
        assert!(!clear.busy);
        assert_eq!(clear.unchecked, vec!["nothing was looked at"]);
        assert_eq!(clear.reason, None);

        let blocked = decide(&WorkSources {
            workers: Liveness::Unknown,
            worker_gap: Some("nothing was looked at".into()),
            ..WorkSources::all_clear()
        });
        assert!(blocked.busy);
        assert_eq!(blocked.reason.as_deref(), Some("nothing was looked at"));
        assert!(blocked.unchecked.is_empty());
    }

    /// A blocking verdict ALWAYS carries a sentence. A silent block is the exact failure the
    /// ruling was written against — *"a state where nothing is wrong and nothing is said"* —
    /// so this walks every combination that blocks rather than sampling three.
    #[test]
    fn no_blocking_verdict_is_ever_silent() {
        let all = [Liveness::Busy, Liveness::Clear, Liveness::Unknown];
        let mut blocking = 0;
        for turn in all {
            for spine in all {
                for w in all {
                    for background in all {
                        let s = WorkSources {
                            turn,
                            spine,
                            workers: w,
                            worker_gap: None,
                            background,
                            background_gap: None,
                        };
                        let v = decide(&s);
                        if v.busy {
                            blocking += 1;
                            assert!(v.reason.is_some(), "silent block: {s:?}");
                        } else {
                            assert_eq!(v.reason, None, "an idle verdict must say nothing: {s:?}");
                        }
                    }
                }
            }
        }
        // 81 combinations with the second lease in the walk, and exactly one of them — all
        // four clear — is idle. It was 27 before §6.4; a fourth source that nothing walked
        // would be the same blind spot one level along.
        assert_eq!(blocking, 80, "only all-clear may act");
    }

    // ---- §6.4: the second lease, which this gate could not see ---------------------------

    /// **The hole §6.4 names, closed.** A quiet conversation with a background assignment
    /// running is BUSY — and the sentence says it is background work rather than repeating
    /// the conversation's own.
    #[test]
    fn background_work_blocks_an_update_even_with_the_conversation_idle() {
        let (l, said) = background(&BackgroundWork { running: 1, awaiting_you: 0, readable: true }, &[]);
        assert_eq!(l, Liveness::Busy);
        let v = decide(&WorkSources { background: l, background_gap: said, ..WorkSources::all_clear() });
        assert!(v.busy, "an update could have installed over live background work");
        assert_eq!(v.reason.as_deref(), Some("1 assignment is still running in the background."));
    }

    /// **§6.5's two sentences.** "Running" and "waiting for you" are different states, and
    /// only one of them is about him. Both block; the second names what he can do about it.
    #[test]
    fn an_assignment_waiting_for_him_says_so_rather_than_saying_it_is_running() {
        let (l, said) = background(&BackgroundWork { running: 2, awaiting_you: 1, readable: true }, &[]);
        assert_eq!(l, Liveness::Busy);
        assert_eq!(said.as_deref(), Some("An assignment is waiting for you to approve it."));
        let (_, plural) = background(&BackgroundWork { running: 0, awaiting_you: 3, readable: true }, &[]);
        assert_eq!(plural.as_deref(), Some("3 assignments are waiting for you to approve them."));
    }

    /// An assignment register this build cannot read is an "I cannot tell", never a zero —
    /// the same rule every other source in this module follows.
    #[test]
    fn an_unreadable_assignment_register_waits_rather_than_installing() {
        let (l, said) = background(&BackgroundWork { running: 0, awaiting_you: 0, readable: false }, &[]);
        assert_eq!(l, Liveness::Unknown);
        let v = decide(&WorkSources { background: l, background_gap: said, ..WorkSources::all_clear() });
        assert!(v.busy);
        assert_eq!(
            v.reason.as_deref(),
            Some("RichOS could not tell whether the work you asked for had finished.")
        );
    }

    /// **The half that stops this being a gate that blocks forever** (§6.5's named trap: *"a
    /// test that passes because everything now defers"*). Nothing registered and no work
    /// lease is CLEAR, with nothing unchecked to declare — an app that has never done any
    /// background work must update exactly as it did before §6.4.
    #[test]
    fn no_background_work_is_an_honest_clear_and_not_a_gap() {
        let (l, said) = background(&BackgroundWork::nothing(), &[]);
        assert_eq!(l, Liveness::Clear);
        assert_eq!(said, None);
        let v = decide(&WorkSources { background: l, background_gap: said, ..WorkSources::all_clear() });
        assert!(!v.busy, "an app with no background work could never update");
        assert!(v.unchecked.is_empty());
        assert_eq!(v.reason, None);
    }

    /// With the register saying nothing is open, the WORK LEASE's own workers are still read:
    /// a worker outliving its assignment is ambiguity, and ambiguity waits.
    #[test]
    fn a_worker_outliving_its_assignment_still_blocks() {
        let (l, said) = background(&BackgroundWork::nothing(), &[view(1, 0)]);
        assert_eq!(l, Liveness::Busy);
        assert_eq!(said.as_deref(), Some("1 worker is still running."));
        let unknown = background(&BackgroundWork::nothing(), &[view(0, 2)]);
        assert_eq!(unknown.0, Liveness::Unknown);
    }

    /// **TWO THREADS, TWO BACK ENDS** (the CEO's Two Riches spec). A quiet first back end
    /// must not hide a busy second one — an update that installed over the second
    /// conversation's work would destroy it exactly as surely as over the first's.
    #[test]
    fn a_quiet_back_end_never_hides_a_busy_one_on_another_conversation() {
        let (l, said) = background(&BackgroundWork::nothing(), &[view(0, 0), view(2, 0)]);
        assert_eq!(l, Liveness::Busy, "a second conversation's live workers were invisible");
        assert_eq!(said.as_deref(), Some("2 workers are still running."));
        // POSITIVE CONTROL: both quiet is still clear, so the reading above is about the
        // second view rather than about a loop that always blocks.
        let (quiet, nothing) = background(&BackgroundWork::nothing(), &[view(0, 0), view(0, 0)]);
        assert_eq!(quiet, Liveness::Clear);
        assert_eq!(nothing, None);
    }

    /// A turn, the conversation's workers AND background work: all three are named, in that
    /// order, in one sentence. He gets the whole reason rather than its first clause.
    #[test]
    fn the_conversation_and_the_background_are_both_named_conversation_first() {
        let (w, worker_said) = workers(&view(2, 0));
        let (b, background_said) =
            background(&BackgroundWork { running: 1, awaiting_you: 0, readable: true }, &[]);
        let v = decide(&WorkSources {
            turn: Liveness::Busy,
            workers: w,
            worker_gap: worker_said,
            background: b,
            background_gap: background_said,
            ..WorkSources::all_clear()
        });
        assert_eq!(
            v.reason.as_deref(),
            Some(
                "Rich is working on your last message. 2 workers are still running. 1 assignment is still running in the background."
            )
        );
    }

    #[test]
    fn the_worse_of_two_readings_speaks_and_a_clear_one_never_hides_the_other() {
        let busy = (Liveness::Busy, Some("busy".to_string()));
        let unknown = (Liveness::Unknown, Some("unknown".to_string()));
        let clear = (Liveness::Clear, None);
        assert_eq!(worst(clear.clone(), busy.clone()), busy);
        assert_eq!(worst(busy.clone(), unknown.clone()), busy);
        assert_eq!(worst(unknown.clone(), busy.clone()), busy);
        assert_eq!(worst(clear.clone(), unknown.clone()), unknown);
        assert_eq!(worst(clear.clone(), clear.clone()), clear);
    }

    // ---- the product's running commands (reap gap C5) ------------------------------------

    fn reading(running: usize, unreadable: usize) -> crate::lease_commands::CommandsReading {
        crate::lease_commands::CommandsReading { running, unreadable }
    }

    /// **A command Rich started is work the update must not cut short.** With the reap, a
    /// relaunch ends every lease's tool commands, so the gate has to see them: busy, with a
    /// sentence, when one runs; unknown when a live lease's state cannot be read; clear only
    /// from a reading that says so.
    #[test]
    fn a_running_command_is_busy_an_unreadable_lease_waits_and_only_a_read_zero_is_clear() {
        assert_eq!(commands(&reading(1, 0)), (Liveness::Busy, Some("A command Rich started is still running.".into())));
        assert_eq!(commands(&reading(3, 1)), (Liveness::Busy, Some("3 commands Rich started are still running.".into())),
                   "running speaks before unknown");
        assert_eq!(commands(&reading(0, 1)),
                   (Liveness::Unknown, Some("RichOS could not tell whether a command Rich started had finished.".into())));
        assert_eq!(commands(&reading(0, 0)), (Liveness::Clear, None));
        // Through the one decision, beside background work: neither hides the other.
        let (busy, said) = worst(background(&BackgroundWork::nothing(), &[]), commands(&reading(1, 0)));
        let verdict = decide(&WorkSources { background: busy, background_gap: said, ..WorkSources::all_clear() });
        assert!(verdict.busy);
        assert_eq!(verdict.reason.as_deref(), Some("A command Rich started is still running."));
        let (quiet, said) = worst(background(&BackgroundWork::nothing(), &[]), commands(&reading(0, 0)));
        assert!(!decide(&WorkSources { background: quiet, background_gap: said, ..WorkSources::all_clear() }).busy,
                "an idle app with no command running must still update at once");
    }
}
