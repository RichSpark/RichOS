//! Measure how long the user waits for a reply and catch avoidable discovery/bookkeeping.
//! Rich decides whether a quick read or a handoff serves the user best. Tool order alone
//! cannot establish that his decision was wrong. The elapsed wait remains measurable.

use std::time::Duration;

/// Existing measured warm-turn regression ceiling. The goal remains a few seconds;
/// passing this ceiling does not establish that goal or prescribe Rich's words.
pub const FIRST_WORDS_BUDGET: Duration = Duration::from_secs(9);

/// The budget for the **first VISIBLE turn of a lease**, which is no longer the cold one.
///
/// **RE-DERIVED 2026-09-18, 20 s → 11 s, and its meaning changed with the number.** The priming
/// turn is now spent before he types (`Spine::prime_front_desk`), so a lease's first visible turn
/// no longer contains it. Measured on the same run as the warm figure above:
///
/// | | measured | what it was |
/// |---|---|---|
/// | **run A, task, the lease's first visible turn** | **7.998 s** | on a desk primed 2.768 s earlier, before he typed |
/// | run B, task, the lease's first visible turn | 15.714 s | **excluded**: the pre-reply checkpoint again, refused by nothing (see [`FIRST_WORDS_BUDGET`]) |
/// | the three first turns before this slice | 13.464 / 13.628 / 15.513 s | the priming turn inside his first message, plus the model's final round trip |
///
/// 11 s is 7.998 s plus ~38%. It stays a SEPARATE constant because the first visible turn really
/// is slower than a warm one on a primed desk — 7.998 s against 6.358 s, measured 90 seconds
/// apart — and one budget covering both would be 11 s everywhere, which a warm regression would
/// pass.
///
/// **AND THIS IS WHERE THE OLD SHAPE GOES RED ON THE TIMING.** Every unprimed first turn ever
/// measured — 13.464 s, 13.628 s, 15.513 s, and run B's own 15.714 s — is over this budget, so
/// `examples/first_reply_timing_e2e.rs` run with `RICHOS_PROBE_UNPRIMED=1` fails by design: that
/// mode reproduces the shape where his first message pays for the priming turn, and it is a
/// measured failure rather than an argument.
pub const FIRST_TURN_BUDGET: Duration = Duration::from_secs(11);

/// Is this frame the model looking for its own tools? Both shapes count: the `ToolSearch` call
/// and the `select:mcp__…` frame the binary reports its choice on.
fn is_discovery(name: &str) -> bool {
    name == crate::native::TOOL_SEARCH_TOOL || name.starts_with("select:")
}

/// Everything wrong with the opening of one turn, in the order it happened.
///
/// `before_his_first_words` is the tool-call frames that arrived BEFORE the first
/// `MessageStarted`/`MessageDelta`, each with its offset in seconds from the send — exactly what
/// `examples/first_reply_timing_e2e.rs` collects off the machinery stream. `to_first_words` is
/// `None` when no live text event was seen at all, which is its own fault: nothing was measured.
///
/// Returns an empty vector when the turn opened the way §55 asks for. Every string names the
/// number that decided it, because a fault without its measurement is the thing this project
/// keeps getting burned by.
pub fn first_reply_faults(
    before_his_first_words: &[(String, f64)],
    to_first_words: Option<f64>,
    budget: Duration,
) -> Vec<String> {
    let mut faults = Vec::new();

    for (name, at) in before_his_first_words {
        if is_discovery(name) {
            faults.push(format!(
                "he waited on tool DISCOVERY at {at:.3} s ({name}) — the front desk's tools must be \
                 resident from its first turn (native::TOOL_SEARCH_ENV)"
            ));
        }
        // **NO ECS WRITE AHEAD OF HIS FIRST WORDS — scored against the SERVER, not against one
        // tool of it.** Every way the front desk can reach the continuity store is a tool of
        // `richos_continuity` (`native::CONTINUITY_TOOL_PREFIX`), and since 2026-09-18 that
        // server's scope file is closed until `ReaderState::he_has_now_been_spoken_to` opens it
        // — so a frame matching this prefix before his first word means the closure did not
        // hold on the real wire, whichever of that server's tools it was.
        //
        // **The register's own ECS write is deliberately not in scope here, and it is not an
        // omission.** Opening the obligation is now the app's, inside
        // `assignment_tools::call`, measured at 143.4 ms through the bridge and invisible on
        // this wire because it is not a tool call at all. What this rule is about is the ~3 s
        // MODEL round trip run B spent writing a checkpoint before saying anything.
        if name.starts_with(crate::native::CONTINUITY_TOOL_PREFIX) {
            faults.push(if name == crate::native::CONTINUITY_CHECKPOINT_TOOL {
                format!(
                    "the continuity checkpoint was written at {at:.3} s, before he had heard anything — \
                     it is bookkeeping and front-desk.md puts it after the reply"
                )
            } else {
                format!(
                    "an ECS call reached the store at {at:.3} s, before he had heard anything \
                     ({name}) — the front desk's continuity grant is not open until his first words"
                )
            });
        }
    }

    match to_first_words {
        None => faults.push("no live text event was observed, so his first words were never measured".into()),
        Some(seconds) if seconds > budget.as_secs_f64() => faults.push(format!(
            "send -> his first words was {seconds:.3} s, over the {:.0} s budget (§55: a few seconds; 35 s is a defect)",
            budget.as_secs_f64()
        )),
        Some(_) => {}
    }

    faults
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(pairs: &[(&str, f64)]) -> Vec<(String, f64)> {
        pairs.iter().map(|(name, at)| ((*name).to_string(), *at)).collect()
    }

    #[test]
    fn the_shape_55_asks_for_has_no_faults() {
        let opening = calls(&[(crate::assignment_tools::QUALIFIED_RECORD_TOOL, 4.212)]);
        assert!(first_reply_faults(&opening, Some(6.716), FIRST_WORDS_BUDGET).is_empty());
        // And a turn where he was simply answered, with no tool call at all, is the best case.
        assert!(first_reply_faults(&[], Some(2.4), FIRST_WORDS_BUDGET).is_empty());
    }

    /// **Ray's five answer-me turns on `.20260920.1`, scored by the rule that was supposed to
    /// be watching them — and the rule says the BUDGET, not a missing "On it!".**
    ///
    /// The walk is
    /// `docs/verification/2026-09-20-nightly-1.2.0-nightly.20260920.1-mac-to-phone-in-the-vm-audit.md`.
    /// Its own §55 step reported *"the first rendered change after send arrives in 43–504 ms —
    /// fast"* and *"`"On it!"` appeared zero times"* across 210 OCR'd states, and left the
    /// verdict open: *"Whether §55's sentence is meant to cover that path is a design call, not
    /// mine"* (audit line 288-290). This test settles the half that is not a design call.
    ///
    /// # The absence of "On it!" on these five turns is the doctrine, not a defect
    ///
    /// The six sends were `reply with exactly: ray nine one`..`five` and `ray nine phone six`
    /// (audit lines 551, 301) — every one of them a message the front desk can answer itself.
    /// `doctrine/front-desk.md`, QUESTION case 1: *"You know the answer. Answer him. Nothing is
    /// written down, nothing is handed over, and no timer runs. Most of his questions are this,
    /// and this is the fast, good case."* `assignment_tools`'s own suite asserts the same thing
    /// from the other side — *"a question got the task reply"* is a FAULT there. So a desk that
    /// had said "On it!" to `reply with exactly: ray nine one` would have been the defect, and
    /// `first_reply_faults` is right to name nothing about it: an answered turn carries no
    /// register, so there is no receipt to be missing.
    ///
    /// # What the rule DOES say about that walk, and it was never scored
    ///
    /// | turn | first content event | vs the 9 s budget |
    /// |---|---|---|
    /// | m1 | 10.820 s | over by 1.820 s |
    /// | m2 | 10.497 s | over by 1.497 s |
    /// | m3 | 12.735 s | over by 3.735 s |
    /// | m4 | 14.525 s | over by 5.525 s |
    /// | m5 | 12.960 s | over by 3.960 s |
    ///
    /// **Those instants are a LOWER bound on his first words, and that is why they can be used
    /// here.** The audit's own table gives them as the frame in which the reply was OCR'd on the
    /// Mac, which on its own is an UPPER bound and would settle nothing. The second half of the
    /// audit closes it: defect 2 records the band reading `Nothing has come back yet` and
    /// HOLDING, with the counter ticking, for **10.8 / 10.5 / 12.7 / 14.5 / 13.0 s** — the same
    /// five numbers. That sentence is `ui/main.js`'s `t.signals === 0` arm, and `signals` is
    /// incremented by `rich://message-started` (`main.js`'s listener, which hands
    /// `noteTurnSignal` the words "Writing the reply"). So the band cannot have been saying it
    /// unless NO message-started had arrived — his first words had not started. The two
    /// measurements are independent, they agree to the tenth of a second, and together they
    /// pin first-words at those instants rather than below them.
    ///
    /// **The cause is NOT established and this test does not claim one.** The host carried load
    /// average 12–27 throughout (audit, step 6) and the app was in a VM. What is established is
    /// that §55's own budget was red on five of five turns and nothing in the walk scored it —
    /// which is what this test is for.
    #[test]
    fn rays_five_answer_me_turns_are_red_on_the_budget_and_clean_on_everything_else() {
        // Seconds from the press, `.20260920.1`, audit §"Rich's reply, Mac window -> phone page".
        let measured = [("m1", 10.8203), ("m2", 10.4972), ("m3", 12.7351), ("m4", 14.5254), ("m5", 12.9599)];

        for (turn, first_words) in measured {
            // NO TOOL CALL AT ALL. These turns handed nothing over, which is the whole point:
            // the front desk answered him itself, exactly as case 1 of the doctrine asks.
            let faults = first_reply_faults(&[], Some(first_words), FIRST_WORDS_BUDGET);

            // EXACTLY ONE FAULT, and it is the budget. A second fault here would mean the rule
            // had invented something about a turn whose only measured problem is that it was
            // slow — and inventing a missing-"On it!" fault for an answered turn is the precise
            // misreading this test exists to hold shut.
            assert_eq!(faults.len(), 1, "{turn}: {faults:#?}");
            assert!(
                faults[0].contains(&format!("{first_words:.3} s, over the 9 s budget")),
                "{turn}: {faults:#?}"
            );
            assert!(
                !faults.iter().any(|f| f.contains("register") || f.contains("On it")),
                "{turn}: an answered turn has no register, so the rule must say nothing about one: {faults:#?}"
            );
        }

        // THE NEGATIVE PROBE, so the block above is not passing because everything is red.
        // The same shape inside the budget is CLEAN — which is also the statement that this
        // test is about the seconds and not about the words. Ray's own first-repaint figures
        // (43.1-504.2 ms) are what the app was already doing well.
        for (turn, first_words) in [("fast-m5", 0.0431), ("fast-m1", 0.5042), ("warm-run-A", 6.358)] {
            assert!(
                first_reply_faults(&[], Some(first_words), FIRST_WORDS_BUDGET).is_empty(),
                "{turn} at {first_words} s is inside the budget and must be clean"
            );
        }

        // AND UNDER THE COLD BUDGET, THREE OF THE FIVE ARE STILL RED: m3 at 12.735 s, m4 at
        // 14.525 s and m5 at 12.960 s against FIRST_TURN_BUDGET's 11 s. m1 (10.820 s) and m2
        // (10.497 s) sit under it, so a lease's FIRST visible turn would forgive those two.
        //
        // **This assertion was WRONG when it was first written and the test caught it**, which
        // is the reason it is spelled out rather than summarized: it said "only m4", on nothing
        // better than m4 being the largest number in the table. Three of these five exceed 11 s
        // and 14.525 is merely the worst of them. A claim about which measurements cross a
        // threshold is arithmetic, and arithmetic gets run.
        let cold: Vec<&str> = measured
            .iter()
            .filter(|(_, s)| !first_reply_faults(&[], Some(*s), FIRST_TURN_BUDGET).is_empty())
            .map(|(t, _)| *t)
            .collect();
        assert_eq!(cold, vec!["m3", "m4", "m5"], "three of the five exceed the 11 s first-visible-turn budget");
    }

    #[test]
    fn the_measured_defect_of_2026_09_18_is_red() {
        // **THE POSITIVE CONTROL, AND IT IS NOT SYNTHETIC.** These are the twelve frames of the
        // nightly-build question, verbatim from `question-receipt-2026-09-18-logs/
        // run-2-instrumented.log`, whose reply reached him at 22.956 s. If this rule cannot
        // fail on the defect it was written for, it cannot be trusted when it passes.
        let opening = calls(&[
            ("ToolSearch", 3.934),
            ("select:mcp__richos_assignments__record", 3.940),
            ("ToolSearch", 7.593),
            ("select:mcp__richos_continuity__checkpoint,mcp__richos_continuity__inspect", 7.653),
            ("mcp__richos_continuity__checkpoint", 12.980),
            ("mcp__richos_assignments__record", 17.982),
        ]);
        let faults = first_reply_faults(&opening, Some(22.956), FIRST_WORDS_BUDGET);
        assert_eq!(faults.len(), 6, "{faults:#?}");
        assert_eq!(faults.iter().filter(|f| f.contains("tool DISCOVERY")).count(), 4);
        assert_eq!(faults.iter().filter(|f| f.contains("continuity checkpoint was written")).count(), 1);
        assert!(faults.iter().any(|f| f.contains("22.956 s, over the 9 s budget")), "{faults:#?}");
        // **And it stays red under the COLD budget too**, which is the arm that matters: a
        // 20 s allowance for a lease's first turn must not quietly forgive the defect this
        // whole slice is about.
        assert!(!first_reply_faults(&opening, Some(22.956), FIRST_TURN_BUDGET).is_empty());
    }

    #[test]
    fn a_quick_read_may_lead_rich_to_hand_the_remaining_work_to_stu() {
        // The doctrine's case 2, which is NOT a defect: *"a question about how work is going is
        // answered from the read, at once"*. Run 2's first question took exactly this shape
        // (`background_work` at 8.748 s, then a direct answer) and it was right to.
        let answered_from_the_read = calls(&[("mcp__richos_status__background_work", 3.1)]);
        assert!(first_reply_faults(&answered_from_the_read, Some(5.9), FIRST_WORDS_BUDGET).is_empty());

        // The same read ahead of a hand-over is the §55 defect: he waited for a look that the
        // other connection does better, and after his turn. **The figures are under the budget on
        // purpose** — the fault asserted here is the ORDER, so a turn that also happened to be
        // slow would prove it with two faults and settle nothing about the one being tested.
        let looked_then_handed_over =
            calls(&[("mcp__richos_status__background_work", 2.1), (crate::assignment_tools::QUALIFIED_RECORD_TOOL, 4.4)]);
        let faults = first_reply_faults(&looked_then_handed_over, Some(6.9), FIRST_WORDS_BUDGET);
        assert!(faults.is_empty(), "a quick read followed by Rich's handoff is valid: {faults:#?}");
        assert!(!first_reply_faults(&looked_then_handed_over, Some(35.0), FIRST_WORDS_BUDGET).is_empty());
    }

    #[test]
    fn every_turn_the_fix_was_measured_on_passes_its_own_budget() {
        // **THE MEASUREMENTS THIS SLICE'S BUDGETS WERE TAKEN FROM, PINNED** — run A of
        // `docs/verification/first-words-2026-09-18.md`. If a later edit loosens a budget, this
        // test says which real turn it was supposed to be about.
        let warm_opening = calls(&[(crate::assignment_tools::QUALIFIED_RECORD_TOOL, 3.968)]);
        assert!(first_reply_faults(&warm_opening, Some(6.358), FIRST_WORDS_BUDGET).is_empty());
        let first_visible = calls(&[(crate::assignment_tools::QUALIFIED_RECORD_TOOL, 5.836)]);
        assert!(first_reply_faults(&first_visible, Some(7.998), FIRST_TURN_BUDGET).is_empty());
        // **WHY THEY ARE TWO CONSTANTS, AND THE HONEST FORM OF IT.** Both of run A's turns pass
        // both budgets — 7.998 s and 6.358 s are under 9 s as well as under 11 s. What the split
        // buys is the 2 s BETWEEN them: a warm turn that regresses to 10 s fails, while a lease's
        // first visible turn at 10 s does not, which is the 1.640 s of real difference the same
        // run measured (7.998 s against 6.358 s, 90 seconds apart) plus its headroom.
        let ten = calls(&[(crate::assignment_tools::QUALIFIED_RECORD_TOOL, 4.0)]);
        assert!(!first_reply_faults(&ten, Some(10.0), FIRST_WORDS_BUDGET).is_empty(), "a warm turn at 10 s is a regression");
        assert!(first_reply_faults(&ten, Some(10.0), FIRST_TURN_BUDGET).is_empty(), "a lease's first visible turn at 10 s is not");
    }

    #[test]
    fn the_shape_where_his_first_message_pays_for_the_priming_turn_is_red() {
        // **THE OLD SHAPE, RED ON THE TIMING, AGAINST ITS OWN MEASUREMENTS.** Before
        // `Spine::prime_front_desk`, the priming turn ran inside his first `submit_prompt`: three
        // runs of the previous record measured that first visible turn at 13.464 s, 13.628 s and
        // 15.513 s, and run B of this slice measured the same shape at 15.714 s when a pre-reply
        // checkpoint put it back. Every one of them is over the re-derived 11 s.
        for unprimed in [13.464, 13.628, 15.513, 15.714] {
            let opening = calls(&[(crate::assignment_tools::QUALIFIED_RECORD_TOOL, 11.714)]);
            let faults = first_reply_faults(&opening, Some(unprimed), FIRST_TURN_BUDGET);
            assert!(!faults.is_empty(), "{unprimed} s must fail the re-derived first-turn budget");
            assert!(faults.iter().any(|f| f.contains("over the 11 s budget")), "{faults:#?}");
            // And on the warm budget too — a slow first turn is never excused by either.
            assert!(!first_reply_faults(&opening, Some(unprimed), FIRST_WORDS_BUDGET).is_empty());
        }
        // The three warm turns of the previous shipped state pass the timing budget and are
        // caught by the ORDERING rule instead: 1.094 s between the register's answer and his
        // first words is the model repeating the app's own sentence. Stated as a pair, here,
        // because a reader who found only the budget would think nothing had changed for them.
        for warm in [7.128, 8.505, 8.920] {
            let opening = calls(&[(crate::assignment_tools::QUALIFIED_RECORD_TOOL, 3.122)]);
            assert!(first_reply_faults(&opening, Some(warm), FIRST_WORDS_BUDGET).is_empty(), "{warm}");
        }
    }

    #[test]
    fn a_turn_that_said_nothing_is_a_fault_rather_than_a_pass() {
        // An unmeasured turn is not a fast turn. `None` is the state the probe reports when no
        // live text event arrived at all, and it must never be silently fine.
        let faults = first_reply_faults(&[], None, FIRST_WORDS_BUDGET);
        assert_eq!(faults.len(), 1);
        assert!(faults[0].contains("never measured"));
    }

    /// **ANY ECS WRITE AHEAD OF HIS REPLY IS RED, and the checkpoint is only the one that was
    /// measured doing it.** Run B wrote `mcp__richos_continuity__checkpoint` at 8.932 s and
    /// 5.429 s on a turn whose first words came at 15.714 s and 12.176 s; the rule has caught
    /// that by name since it was written. What this test adds is the general case: the front
    /// desk's grant on `richos_continuity` is closed until
    /// `ReaderState::he_has_now_been_spoken_to` opens it, so ANY tool of that server appearing
    /// ahead of his first word means the closure did not hold — including one nobody has added
    /// yet.
    #[test]
    fn no_ecs_write_reaches_the_store_before_his_first_words() {
        // Run B's own frame, verbatim, on the warm turn. One fault, named.
        let run_b = calls(&[
            (crate::native::CONTINUITY_CHECKPOINT_TOOL, 5.429),
            (crate::assignment_tools::QUALIFIED_RECORD_TOOL, 9.8),
        ]);
        let faults = first_reply_faults(&run_b, Some(12.176), FIRST_WORDS_BUDGET);
        assert!(faults.iter().any(|f| f.contains("continuity checkpoint was written at 5.429 s")), "{faults:#?}");

        // `inspect` is the OTHER tool that server already has, and reading the store ahead of
        // the reply costs him the same round trip a write does.
        let read_first = calls(&[("mcp__richos_continuity__inspect", 4.2)]);
        let faults = first_reply_faults(&read_first, Some(6.1), FIRST_WORDS_BUDGET);
        assert_eq!(faults.len(), 1, "{faults:#?}");
        assert!(faults[0].contains("an ECS call reached the store at 4.200 s"), "{faults:#?}");

        // And a tool of that server that does not exist yet, which is the point of the prefix.
        let future = calls(&[("mcp__richos_continuity__amend", 3.0)]);
        assert_eq!(first_reply_faults(&future, Some(6.1), FIRST_WORDS_BUDGET).len(), 1);

        // **THE NEGATIVE CONTROL, and it is the shape this slice SHIPS.** The register is the
        // first and only call ahead of the reply, and the obligation it opens is an ECS write
        // that is NOT a tool call — it happens inside `assignment_tools::call`, 143.4 ms
        // measured, on the app's own bridge. A rule that scored it would fail the fix.
        let shipped = calls(&[(crate::assignment_tools::QUALIFIED_RECORD_TOOL, 3.968)]);
        assert!(first_reply_faults(&shipped, Some(6.358), FIRST_WORDS_BUDGET).is_empty());
    }

    #[test]
    fn everything_after_his_first_word_is_off_his_wait() {
        // The list this rule is given is, by construction, only what happened BEFORE his first
        // word — so a checkpoint written after the reply is invisible here, which is the whole
        // point of the change it enforces.
        assert!(first_reply_faults(&[], Some(6.7), FIRST_WORDS_BUDGET).is_empty());
    }
}
