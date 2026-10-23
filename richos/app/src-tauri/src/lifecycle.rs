//! **CLOSING THE WINDOW STOPS MEANING QUIT** — row 5 of the CEO's nine, and the decision
//! behind every exit this app can be asked for.
//!
//! His words (`richos-hq/wiki/ceo-decisions.md` §50): *"OK, window-closed only."* Row 5:
//! *"You close the window. The work keeps going. Quit ends it."*
//!
//! # What this file is, and what it is deliberately not
//!
//! It is the DECISION and the WORDS, with no Tauri in it. The wiring — the `ExitRequested`
//! arm, the Quit menu item, the Dock-icon reopen — is `main.rs`, because that is where the
//! runtime is. The split is the same one `work_gate.rs` argues for itself: a decision made
//! inside the shell is a decision no suite can drive, and this one has five branches and a
//! sentence the CEO hears.
//!
//! # The mechanism, re-derived at this base rather than quoted
//!
//! The background-work spec §2.3 says the app has no exit arm at all, and that was still
//! true at `eef1580b`:
//!
//! ```text
//! grep -rnE "prevent_exit|prevent_close|ExitRequested|RunEvent::Exit|CloseRequested" \
//!   richos/app/src-tauri/src --include=*.rs
//!   main.rs:2421  (a comment naming RunEvent::Exit)
//!   main.rs:2430  if let tauri::RunEvent::Exit = event {
//!   main.rs:6264  | tauri::WindowEvent::CloseRequested { .. }     (a geometry nudge)
//! ```
//!
//! Three hits, none of them a prevent. So the last window destroyed ended the process, and
//! the process ending killed the work within about a tenth of a second (`main.rs`'s Exit arm
//! → `shutdown_lease()` → `process_fence.kill()` → the provider group).
//!
//! **The two events this build now answers, verified in the runtime it ships against**
//! (`tauri-runtime-wry-2.11.4`, the version in `src-tauri/Cargo.lock` under `tauri 2.11.5`):
//!
//! | Event | Raised when | Read at |
//! |---|---|---|
//! | `ExitRequested { code: None }` | the LAST window was destroyed | `lib.rs:4307-4326` |
//! | `ExitRequested { code: Some(_) }` | `app.exit(code)` — `Message::RequestExit` | `lib.rs:4353-4366` |
//! | `Reopen { has_visible_windows }` | the Dock icon was clicked | `lib.rs:4391-4394` |
//!
//! **The answer is read SYNCHRONOUSLY**, with `try_recv` and not `recv` (`lib.rs:4318`,
//! `:4361`), so an empty channel is not `Prevent` — §2.5a's point exactly: a handler that
//! returns before deciding, because it is waiting for a dialog, exits. Everything in this
//! file is therefore a pure function that answers immediately; the asking happens afterwards.
//!
//! # And one correction to §2.5, found by reading the runtime rather than the app
//!
//! §2.5 says *"the app builds no menu … so it inherits AppKit's, whose Quit calls
//! `terminate:` directly."* The app builds no menu — true, and still true at this base — but
//! it does not inherit AppKit's: **Tauri installs `Menu::default` for it**
//! (`tauri-2.11.5/src/app.rs:2244-2249`, guarded by `enable_macos_default_menu`, which
//! defaults to `true` at `:1620`). The conclusion is unchanged and the reason is one level
//! down: that default menu's Quit is `PredefinedMenuItem::quit`
//! (`tauri-2.11.5/src/menu/menu.rs:194`), and muda maps it to `sel!(terminate:)`
//! (`muda-0.19.3/src/platform_impl/macos/mod.rs:994`) — a selector no handler can refuse. So
//! the app must own that one item, which is what `main.rs` now does.

/// What the assignment register says is registered right now — the same three numbers the
/// update gate reads (`richos_core::work_gate::BackgroundWork`), because "is there work"
/// must not have two answers in one process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registered {
    pub running: usize,
    pub awaiting_you: usize,
    /// `false` when the register could not be read at all. **Never collapsed into zero.**
    pub readable: bool,
    /// **Commands Rich started that are still running** (the product reap gap design C5): quit
    /// now ends every lease's tool commands, so a background command he asked for is work.
    /// Read from every live lease's supervisor state file (`lease_commands.rs`); a lease whose
    /// file cannot be read counts, never as a zero.
    pub commands: richos_core::lease_commands::CommandsReading,
}

impl Registered {
    pub fn nothing() -> Self {
        Registered { running: 0, awaiting_you: 0, readable: true, commands: Default::default() }
    }
    /// Is there anything this app would be destroying by going away? An unreadable register
    /// answers `true`, which is the standing rule everywhere else in this system: what
    /// cannot be witnessed counts as work, never as zero (`app_workers.rs:33-47`).
    pub fn anything(&self) -> bool {
        !self.readable || self.running > 0 || self.awaiting_you > 0
            || self.commands.running > 0 || self.commands.unreadable > 0
    }
}

/// One request to end the process, with everything the decision turns on.
#[derive(Clone, Debug)]
pub struct ExitRequest {
    /// Durable questions keep the phone service available after a window close.
    /// They are persisted and never add a confirmation gate to an explicit Quit.
    pub questions_pending: bool,
    /// `true` for `ExitRequested { code: Some(_) }` — this app asked to exit, rather than a
    /// window being closed.
    pub programmatic: bool,
    /// He has already been asked and answered "quit and stop the work".
    pub confirmed: bool,
    pub registered: Registered,
    /// Whether there is a way back into a window: a `Regular` activation has a Dock icon
    /// (`activation.rs:1-18, 304-314`), an `Accessory` one has none.
    pub can_come_back: bool,
}

/// What to do with it. Exactly one of these is returned, and `main.rs` does nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitDecision {
    /// Let the process end — **this is today's behavior, unchanged**, and it is what an idle
    /// app still does when its last window closes (§2.4: *"idle must still quit"*).
    Allow,
    /// Prevent the exit and stay alive with no window. The Dock icon is the way back
    /// (§2.4).
    StayResident,
    /// Prevent the exit and ask him first (§2.5, §2.5a). The asking is `main.rs`'s and
    /// happens AFTER the callback returns, never inside it.
    AskBeforeQuitting,
}

/// **The whole of row 5's decision.**
pub fn decide(request: &ExitRequest) -> ExitDecision {
    // 1. He said quit and stop the work. This is the second pass through the same arm and
    //    it must go through, or the two-step prevent becomes a trap he cannot leave.
    if request.confirmed {
        return ExitDecision::Allow;
    }
    if !request.programmatic && request.can_come_back && request.questions_pending {
        return ExitDecision::StayResident;
    }
    // 2. Nothing is registered: quit exactly as this app has always quit, whether this is a
    //    window closing or a quit. No new behavior on the path every user is on.
    if !request.registered.anything() {
        return ExitDecision::Allow;
    }
    // 3. A programmatic exit with work registered. The only ones this app raises are its own
    //    (already covered by `confirmed`), §2.4a's self-quit — which asks the register
    //    first, so it cannot reach here with work registered — and the updater's relaunch,
    //    which the update gate already refuses while work is live (`work_gate.rs`, §6.4).
    //    So this is a path that should not arise; it ASKS rather than allowing, because the
    //    cost of asking is a question and the cost of allowing is his work.
    if request.programmatic {
        return ExitDecision::AskBeforeQuitting;
    }
    // 4. His last window closed with work registered. This is the CEO's decision, and the
    //    change: the process stays, the work runs, the Dock icon brings the window back.
    if request.can_come_back {
        return ExitDecision::StayResident;
    }
    // 5. **Work registered, and NO way back in.** An `Accessory` launch has no Dock icon and
    //    builds its window invisible (`activation.rs:1-18, 304-314`), so staying resident
    //    would leave a process the person at this Mac can neither see nor reach — which is
    //    the "background process the user did not ask for" §2.4a refuses, in its worst form.
    //    §2.4b already states that such a launch cannot walk 7.4 at all. So it quits, as it
    //    did before this change, and the work stops with it.
    ExitDecision::Allow
}

/// **What he is asked before a quit takes his work with it** (§2.5).
///
/// American English, spoken-safe, comparative rather than absolute, and it names what is
/// actually running rather than a count he cannot act on. It never says the work will be
/// lost: it is stopped, and everything it produced is kept — which is what his answer
/// actually does (`WorkHost::record_quit_and_stop`: his Stop on every open job, files retained).
pub fn quit_question(registered: &Registered) -> String {
    if !registered.readable {
        return "I can't tell whether anything is still running in the background. \
                If you quit now, anything that is running stops. Nothing is lost and \
                nothing is landed."
            .into();
    }
    let running = registered.running;
    let waiting = registered.awaiting_you;
    let mut what = String::new();
    if running > 0 {
        what.push_str(&format!(
            "{running} {} still running in the background",
            if running == 1 { "assignment" } else { "assignments" }
        ));
    }
    if waiting > 0 {
        if !what.is_empty() {
            what.push_str(", and ");
        }
        what.push_str(&format!(
            "{waiting} {} waiting for you to approve {}",
            if waiting == 1 { "assignment" } else { "assignments" },
            if waiting == 1 { "it" } else { "them" }
        ));
    }
    // **Commands Rich started** (reap gap C5): quitting ends them, so he is told first.
    let commands = registered.commands;
    if commands.running > 0 || commands.unreadable > 0 {
        if !what.is_empty() {
            what.push_str(", and ");
        }
        what.push_str(&match commands.running {
            0 => "a command Rich started that may still be running".to_string(),
            1 => "a command Rich started still running".to_string(),
            n => format!("{n} commands Rich started still running"),
        });
    }
    format!(
        "You have {what}. Quitting stops the work. Everything it has done so far is kept \
         and nothing is landed in your repository."
    )
}

/// **WHY A QUIT WAS ALLOWED, so a walk can tell one red button from another** — audit-10
/// row 5.
///
/// Ray closed the window twice on candidate .10 and got two different outcomes from one
/// control. With work registered, `.9` wrote a line and he could read it:
///
/// ```text
/// [richos] window closed with work registered: RichOS stays running with no window.
///          The Dock icon brings it back.
/// ```
///
/// With nothing registered the app quit and `app.log` carried **nothing at all**
/// (`docs/verification/2026-09-18-nightly-1.2.0-nightly.20260918.5-onscreen-audit.md` §5).
/// He is right that the behavior is bimodal and he is explicitly NOT calling the quit wrong,
/// and neither is this: the nothing-registered quit is the spec, by name. §2.4 is *"idle must
/// still quit"*, with the reason *"otherwise every user leaves invisible processes behind"*,
/// and the branch table at `docs/verification/two-riches-window-and-recovery-2026-09-17.md:161`
/// records it as "exactly today's behavior". So the defect is not the decision. It is that
/// **one of the two paths says what it did and the other is silent**, which leaves a walk
/// unable to distinguish "quit as designed" from "died".
///
/// `None` means the exit was not allowed — the caller has a `StayResident` or an
/// `AskBeforeQuitting`, both of which already log or ask for themselves.
///
/// **THE BRANCH ORDER MIRRORS `decide` AND A TEST REFUSES TO LET THE TWO DRIFT.** A reason
/// function that says something `decide` did not do would be worse than no line, because a
/// walk would then be reading a confident sentence about the wrong branch — so
/// `every_allowed_exit_has_exactly_one_reason_and_no_other_decision_has_any` sweeps the whole
/// request space and asserts `allow_reason(r).is_some()` exactly when `decide(r)` is `Allow`.
pub fn allow_reason(request: &ExitRequest) -> Option<&'static str> {
    if decide(request) != ExitDecision::Allow { return None; }
    if request.questions_pending && request.programmatic && !request.registered.anything() {
        return Some("Quit requested: questions and answers are saved for the next launch.");
    }

    if request.confirmed {
        return Some(
            "quit confirmed: he was asked and answered \"quit and stop the work\". RichOS is \
             ending, and everything the work produced is kept.",
        );
    }
    if !request.registered.anything() {
        return Some(
            "last window closed with nothing registered: RichOS quits. Nothing was running \
             and nothing was waiting for him, so there is nothing to stay alive for \
             (background-work spec \u{a7}2.4 \u{2014} idle must still quit).",
        );
    }
    if request.programmatic {
        return None;
    }
    if request.can_come_back {
        return None;
    }
    Some(
        "window closed with work registered and NO way back in \u{2014} this launch has no Dock \
         icon, so staying resident would leave a process he can neither see nor reach. RichOS \
         quits and the work stops with it (\u{a7}2.4b).",
    )
}

/// The two answers, as the surface names them. Here rather than in the page because a
/// control's words and the action behind it drifting apart is the defect the affordance
/// gate exists for.
pub const QUIT_AND_STOP: &str = "Quit and stop the work";
pub const KEEP_WORKING: &str = "Keep working";

#[cfg(test)]
mod tests {
    use super::*;

    fn request(programmatic: bool, registered: Registered) -> ExitRequest {
        ExitRequest { questions_pending: false, programmatic, confirmed: false, registered, can_come_back: true }
    }
    #[test]
    fn pending_questions_keep_a_windowless_service_but_do_not_block_explicit_quit() {
        let mut r = request(false, Registered::nothing());
        r.questions_pending = true;
        assert_eq!(decide(&r), ExitDecision::StayResident);
        assert_eq!(allow_reason(&r), None);
        r.programmatic = true;
        assert_eq!(decide(&r), ExitDecision::Allow);
        r.programmatic = false;
        r.can_come_back = false;
        assert_eq!(decide(&r), ExitDecision::Allow);
        r.can_come_back = true;
        r.confirmed = true;
        assert_eq!(decide(&r), ExitDecision::Allow);
    }

    fn some_work() -> Registered {
        Registered { running: 1, awaiting_you: 0, readable: true, commands: Default::default() }
    }

    /// **A command Rich started is work** (the product reap gap design C5). Quit now ends every
    /// lease's tool commands, so closing the window keeps the app alive for one, a quit asks
    /// first and names it, and a lease whose commands cannot be read counts rather than
    /// reading as none.
    #[test]
    fn a_command_rich_started_keeps_the_app_alive_and_the_quit_question_names_it() {
        use richos_core::lease_commands::CommandsReading;
        let one = Registered { commands: CommandsReading { running: 1, unreadable: 0 }, ..Registered::nothing() };
        assert!(one.anything());
        assert_eq!(decide(&request(false, one.clone())), ExitDecision::StayResident);
        assert_eq!(decide(&request(true, one.clone())), ExitDecision::AskBeforeQuitting);
        let said = quit_question(&one);
        assert!(said.starts_with("You have a command Rich started still running. Quitting stops the work."), "{said}");
        let unknown = Registered { commands: CommandsReading { running: 0, unreadable: 1 }, ..Registered::nothing() };
        assert!(unknown.anything(), "an unreadable lease is not a zero");
        assert!(quit_question(&unknown).contains("a command Rich started that may still be running"), "{}", quit_question(&unknown));
        let both = quit_question(&Registered { running: 1, commands: CommandsReading { running: 2, unreadable: 0 }, ..Registered::nothing() });
        assert!(both.starts_with("You have 1 assignment still running in the background, and 2 commands Rich started still running."), "{both}");
    }

    /// **The whole request space, so `allow_reason` can never describe a branch `decide` did
    /// not take** — audit-10 row 5.
    ///
    /// Two functions with the same five branches in the same order is a drift waiting to
    /// happen, and the cost of that drift is specific: a walk reads a confident sentence
    /// about the wrong branch and trusts it. So this enumerates every shape an `ExitRequest`
    /// can have — 2 x 2 x 2 over the flags, crossed with every interesting register state,
    /// 48 requests — and asserts the two agree on every one.
    #[test]
    fn every_allowed_exit_has_exactly_one_reason_and_no_other_decision_has_any() {
        let registers = [
            Registered::nothing(),
            Registered { running: 1, awaiting_you: 0, readable: true, commands: Default::default() },
            Registered { running: 0, awaiting_you: 1, readable: true, commands: Default::default() },
            Registered { running: 2, awaiting_you: 3, readable: true, commands: Default::default() },
            Registered { running: 0, awaiting_you: 0, readable: false, commands: Default::default() },
            Registered { running: 1, awaiting_you: 1, readable: false, commands: Default::default() },
            // A command Rich started, and a lease whose commands cannot be read (reap gap C5).
            Registered { commands: richos_core::lease_commands::CommandsReading { running: 1, unreadable: 0 }, ..Registered::nothing() },
            Registered { commands: richos_core::lease_commands::CommandsReading { running: 0, unreadable: 1 }, ..Registered::nothing() },
        ];
        let mut allowed = 0;
        let mut refused = 0;
        for registered in registers {
            for programmatic in [false, true] {
                for confirmed in [false, true] {
                    for can_come_back in [false, true] {
                        let r = ExitRequest { questions_pending: false, programmatic, confirmed, registered: registered.clone(), can_come_back };
                        let decision = decide(&r);
                        let reason = allow_reason(&r);
                        if decision == ExitDecision::Allow {
                            allowed += 1;
                            let said = reason.unwrap_or_else(|| {
                                panic!("decide allowed {r:?} and allow_reason had nothing to say")
                            });
                            // A LINE A PERSON CAN ACT ON, not a token. The StayResident line
                            // this one is built to sit beside is a sentence, and a walk
                            // comparing the two has to tell them apart by reading.
                            assert!(said.len() > 40, "the reason for {r:?} is not a sentence: {said:?}");
                            assert!(
                                !said.contains("lease") && !said.contains("obligation"),
                                "the reason for {r:?} leaked machinery: {said:?}"
                            );
                        } else {
                            refused += 1;
                            assert!(
                                reason.is_none(),
                                "decide returned {decision:?} for {r:?} and allow_reason still \
                                 claimed {reason:?} — a walk would read that as a quit"
                            );
                        }
                    }
                }
            }
        }
        // POSITIVE CONTROL ON THE SWEEP ITSELF: a matrix that happened to be all-allow or
        // all-refuse would pass the loop above while proving nothing about the other side.
        // 8 registers (the two command states added 2026-09-27) x
        // programmatic x confirmed x can_come_back = 8 x 2 x 2 x 2 = 64.
        assert_eq!(allowed + refused, 64, "the sweep did not cover the space it claims to");
        assert!(allowed > 0 && refused > 0, "the sweep never saw both outcomes: {allowed} / {refused}");
    }

    /// The two paths a person at the Mac can actually take with the red button, and the
    /// point of the row: **they must not read the same.**
    #[test]
    fn the_two_red_button_outcomes_say_different_things() {
        let idle = ExitRequest {
            questions_pending: false,
            programmatic: false,
            confirmed: false,
            registered: Registered::nothing(),
            can_come_back: true,
        };
        assert_eq!(decide(&idle), ExitDecision::Allow);
        let said = allow_reason(&idle).expect("the quit that ended Ray's walk says nothing");
        assert!(said.contains("nothing registered"), "{said}");
        assert!(said.contains("quits"), "{said}");

        // The other path is the one that already had a line, and it must still not produce
        // one from here — `main.rs` logs `StayResident` itself.
        let working = ExitRequest { registered: some_work(), ..idle };
        assert_eq!(decide(&working), ExitDecision::StayResident);
        assert_eq!(allow_reason(&working), None);
    }

    /// **Row 5, both halves, and the second half is the one that must not regress.**
    #[test]
    fn closing_the_window_keeps_work_running_and_still_quits_when_there_is_none() {
        // Work registered, a window closed: the process stays.
        assert_eq!(decide(&request(false, some_work())), ExitDecision::StayResident);
        // An assignment waiting for HIM also keeps it alive — §7.8 says so outright: he
        // comes back to a running app rather than to a relaunch.
        assert_eq!(
            decide(&request(false, Registered { running: 0, awaiting_you: 1, readable: true, commands: Default::default() })),
            ExitDecision::StayResident
        );
        // NOTHING registered: exactly today's behavior, which §2.4 requires by name —
        // "otherwise every user leaves invisible processes behind".
        assert_eq!(decide(&request(false, Registered::nothing())), ExitDecision::Allow);
    }

    /// **A register that cannot be read is work, never zero** — the same rule the update
    /// gate keeps, and the positive control beside it.
    #[test]
    fn an_unreadable_register_is_treated_as_work_rather_than_as_nothing() {
        let unreadable = Registered { running: 0, awaiting_you: 0, readable: false, commands: Default::default() };
        assert!(unreadable.anything());
        assert_eq!(decide(&request(false, unreadable.clone())), ExitDecision::StayResident);
        // Positive control: the same zeros, readable, quit.
        assert_eq!(decide(&request(false, Registered::nothing())), ExitDecision::Allow);
        // And the question says what it does not know rather than naming a count it has
        // not got.
        let said = quit_question(&unreadable);
        assert!(said.contains("can't tell"), "{said}");
        assert!(!said.contains("0 "), "{said}");
    }

    /// **§2.5: a quit with work registered asks first, and a confirmed one goes through.**
    /// Without the second half the two-step prevent would be a trap.
    #[test]
    fn a_quit_asks_first_and_the_confirmed_second_pass_is_allowed() {
        assert_eq!(decide(&request(true, some_work())), ExitDecision::AskBeforeQuitting);
        let confirmed = ExitRequest { confirmed: true, ..request(true, some_work()) };
        assert_eq!(decide(&confirmed), ExitDecision::Allow);
        // A quit with nothing registered never asks: the question would be about nothing.
        assert_eq!(decide(&request(true, Registered::nothing())), ExitDecision::Allow);
    }

    /// **§2.4b: an accessory launch has no way back, so it must not stay resident.** A
    /// windowless process with no Dock icon is one the person at this Mac can neither see
    /// nor reach.
    #[test]
    fn a_launch_with_no_way_back_quits_rather_than_going_invisible() {
        let no_dock = ExitRequest { can_come_back: false, ..request(false, some_work()) };
        assert_eq!(decide(&no_dock), ExitDecision::Allow);
        // Positive control: the same request with a way back stays.
        assert_eq!(decide(&request(false, some_work())), ExitDecision::StayResident);
    }

    /// The question is his, in his terms: no identifiers, no jargon, singular and plural
    /// both right, and it never promises that nothing will be lost in general — it says
    /// exactly what is kept.
    #[test]
    fn the_quit_question_reads_the_same_spoken_as_written() {
        let one = quit_question(&some_work());
        assert!(one.contains("You have 1 assignment still running in the background."), "{one}");
        assert!(one.contains("Everything it has done so far is kept"), "{one}");
        assert!(one.contains("nothing is landed"), "{one}");
        let many = quit_question(&Registered { running: 2, awaiting_you: 3, readable: true, commands: Default::default() });
        assert!(many.contains("2 assignments still running in the background"), "{many}");
        assert!(many.contains("3 assignments waiting for you to approve them"), "{many}");
        let waiting_one = quit_question(&Registered { running: 0, awaiting_you: 1, readable: true, commands: Default::default() });
        assert!(waiting_one.contains("You have 1 assignment waiting for you to approve it."), "{waiting_one}");
        // Nothing technical reaches him.
        for word in ["lease", "session", "process", "assignment id", "obligation"] {
            assert!(!many.contains(word), "the question said {word:?}: {many}");
        }
    }
}
