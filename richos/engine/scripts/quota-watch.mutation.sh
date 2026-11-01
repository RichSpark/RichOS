#!/usr/bin/env bash
#
# quota-watch.mutation.sh — PROVES THE 93% RULE'S SUITE CAN FAIL.
#
# Takes the SHIPPED scripts/lib/quota_watch.py, removes ONE property at a time
# in a throwaway copy of the engine, and asserts that (1) the suite fails,
# (2) the SPECIFIC named case fails, and (3) the mutation actually applied.
# The loop is scripts/lib/mutation-harness.sh; this file is the list of
# properties his rule rests on.
#
# Invoked by quota-watch.test.sh, so the runner that discovers *.test.sh runs
# it too. Run directly: scripts/quota-watch.mutation.sh
# Exit 0 = every property is proven load-bearing.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail
[ -n "${RICHOS_MUTATION_INNER:-}" ] && exit 0
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/mutation-harness.sh
. "$SCRIPT_DIR/lib/mutation-harness.sh"
mutation_begin "quota-watch (the CEO's 93% rule)" "scripts/quota-watch.test.sh"
# Each focused case retains its setup and exits only after its own verdict.
# The harness first proves that exact focus green on an unmutated copy.
mutation_focus want-as-argument

L="scripts/lib/quota_watch.py"

# 1. THE EDGE. "once it crosses the 93% threshold" — 93 itself is across.
mutant threshold-strict "Q02" "$L" \
    'if r["used"] >= threshold:' \
    'if r["used"] > threshold:' \
    "A reading of exactly 93% would read as below the threshold and nobody would be paused."

# 2. THE NUMBER IS HIS AND LIVES IN ONE PLACE. A built-in fallback is a second
#    place holding 93, which is how the number drifts.
mutant builtin-fallback "Q11" "$L" \
    'raw = (raw or "").strip()' \
    'raw = (raw or "93").strip()' \
    "An undeclared threshold would silently become 93 instead of being announced as missing."

mutant hardcoded-threshold "Q13" "$L" \
    'return v, ""' \
    'return 93.0, ""' \
    "The declared QUOTA_PAUSE_PERCENT would be read and ignored."

# 3. ABSENCE IS NEVER FINE (design notes R6).
mutant unknown-exits-zero "Q04" "$L" \
    'return {"below": 0, "at-or-above": 1, "near-reset": 3}.get(verdict, 2)' \
    'return {"below": 0, "at-or-above": 1, "near-reset": 3}.get(verdict, 0)' \
    "A missing or unreadable payload would exit exactly like a healthy reading below the threshold."

# Targets Q08, the direct case. R01 no longer depends on it: since the fallback
# is refreshed before it turns one poll old (W12), R01's lead is woken by
# QUOTA-REFRESH before the reading can go stale.
mutant stale-read-as-fresh "Q08" "$L" \
    'if r["age"] is not None and r["age"] > stale_after:' \
    'if False:' \
    "A reading an hour old would be reported as a fresh 'below the threshold': the 2026-09-25 failure."

mutant ended-window-read-as-current "Q10" "$L" \
    '    if r["ended"]:{NL}        return "unknown"' \
    '    if False:{NL}        return "unknown"' \
    "A reading of a window that no longer exists would be trusted."

mutant stale-bound-three-polls "Q16" "$L" \
    'a.stale = _env_int("QUOTA_WATCH_STALE_SECONDS", a.poll)' \
    'a.stale = _env_int("QUOTA_WATCH_STALE_SECONDS", 3 * a.poll)' \
    "A reading two polls old would pass as current, which is how the 93% crossing went unseen."

mutant stale-wakes-nobody "W05" "$L" \
    'stale = r["state"] == "ok" and not r["ended"]' \
    'stale = False' \
    "A stale reading would wait out a further poll on the watcher's own clock before telling the lead to refresh."

# 4. HIS "every 5 minutes".
mutant poll-not-five-minutes "W09" "$L" \
    'POLL_SECONDS = 300' \
    'POLL_SECONDS = 30' \
    "The watcher would poll ten times as often as he said."

# 5. THE PAUSE IS A REAL PAUSE. Without the pause-until: line the registry
#    records the agent's own end of run as FINISHED, and the wake at the reset
#    is refused: 2026-09-18, exactly.
mutant pause-message-without-pause-until "E02" "$L" \
    'return pause_protocol.render("quota", hhmm(r["resets_at"]))' \
    'return pause_protocol.render("quota", hhmm(r["resets_at"])).split("\npause-until:", 1)[0]' \
    "The pause message would read correctly and record nothing, and every paused agent would be finished at its first SubagentStop."

mutant resume-message-re-pauses "E09" "$L" \
    'return pause_protocol.render_resume(reason="quota")' \
    'return pause_protocol.render_resume(reason="quota") + "\npause-until: later"' \
    "The resume message would re-pause the agent it was meant to wake."

# Sage's catch 3: a generated pause ends only on the generated RESUME, so a
# handwritten wake leaves the agent waiting, which is what E09 would show.
mutant resume-message-handwritten "E09" "$L" \
    'return pause_protocol.render_resume(reason="quota")' \
    'return "RESUME: the five-hour quota window reset. Continue exactly where you stopped."' \
    "A handwritten wake would read like a resume and leave the agent waiting."

# 6. WAKE THE LEAD ONLY WHEN THERE IS SOMETHING TO DO.
mutant fires-with-nothing-working "W03" "$L" \
    '                if w["known"] and not w["working"]:{NL}                    print("  at the threshold with nothing working' \
    '                if False:{NL}                    print("  at the threshold with nothing working' \
    "After the lead paused everybody, restarting the watcher would wake it again at once, forever, until the reset."

mutant blind-wake-with-nothing-working "W06" "$L" \
    '                    if w["known"] and not w["working"]:{NL}                        print("  the reading is unknown but nothing' \
    '                    if False:{NL}                        print("  the reading is unknown but nothing' \
    "A quiet session would be woken for a stale reading when there is no spend it could hide."

# 7. HIS 2026-09-25 UPDATE (ruling §87). At or above the threshold, no pause
#    when the reset is LESS than 20 minutes away; exactly 20 still pauses; a
#    hold in place releases inside that window. And "Keep it consistent at 5
#    minutes": no usage tier.
mutant near-reset-ignored "Q17" "$L" \
    '    if verdict == "at-or-above" and in_last_twenty(r["resets_at"], now):' \
    '    if False:' \
    "Agents would be paused with 19 minutes left, for a hold that ends almost as soon as it starts."

mutant twenty-minutes-inclusive "Q18" "$L" \
    'return resets_at is not None and 0 < resets_at - now < RESET_EXEMPTION_SECONDS' \
    'return resets_at is not None and 0 < resets_at - now <= RESET_EXEMPTION_SECONDS' \
    "Exactly 20 minutes would skip the pause; his words keep it."

mutant exemption-ten-minutes "Q17" "$L" \
    'RESET_EXEMPTION_SECONDS = 20 * 60' \
    'RESET_EXEMPTION_SECONDS = 10 * 60' \
    "The window would shrink to 10 minutes and 19 minutes left would pause."

mutant exemption-thirty-minutes "Q19" "$L" \
    'RESET_EXEMPTION_SECONDS = 20 * 60' \
    'RESET_EXEMPTION_SECONDS = 30 * 60' \
    "The window would grow to 30 minutes and 21 minutes left would not pause."

mutant near-reset-exits-as-pause "Q17" "$L" \
    'return {"below": 0, "at-or-above": 1, "near-reset": 3}.get(verdict, 2)' \
    'return {"below": 0, "at-or-above": 1, "near-reset": 1}.get(verdict, 2)' \
    "--once would tell a caller to pause inside the last 20 minutes."

mutant watch-ignores-exception "W10" "$L" \
    '        verdict, why = rule_verdict(r, a.threshold, a.stale, now){NL}        print("%s  %s"' \
    '        verdict, why = classify(r, a.threshold, a.stale){NL}        print("%s  %s"' \
    "--once would say no pause while the watcher woke the lead to pause anyway."

mutant release-never-fires "E05" "$L" \
    '            if not w["known"] or w["quota_paused"]:{NL}                out.wake("RELEASE"' \
    '            if False:{NL}                out.wake("RELEASE"' \
    "A hold would last to the reset instead of releasing inside the last 20 minutes."

mutant release-with-nobody-held "W03" "$L" \
    '            if not w["known"] or w["quota_paused"]:{NL}                out.wake("RELEASE"' \
    '            if True:{NL}                out.wake("RELEASE"' \
    "The lead would be woken to release a hold nobody is in."

mutant release-a-poll-late "E05" "$L" \
    '            if to_release > 0:' \
    '            if False:' \
    "The release would come up to one 5-minute poll after the 20-minute mark."

# The release message is the generated RESUME (render_resume("quota")); until
# 2026-09-28 this mutant targeted a handwritten text the file no longer has,
# so it matched nothing.
mutant release-message-re-pauses "E06" "$L" \
    '    return pause_protocol.render_resume("quota")' \
    '    return pause_protocol.render_resume("quota") + "\npause-until: later"' \
    "The release message would re-pause the agent it was meant to wake."

mutant stale-bound-usage-tier "Q23" "$L" \
    'if r["age"] is not None and r["age"] > stale_after:' \
    'if r["age"] is not None and r["age"] > (stale_after if r["used"] >= 70 else 6 * stale_after):' \
    "Below 70% a reading 30 minutes old would pass as current: the schedule he dropped."

mutant status-omits-update "Q24" "$L" \
    '    print("  update    : %s" % RULE_UPDATE)' \
    '    pass' \
    "--status would state the rule without his 2026-09-25 update."

# 8. THE READING COMES FROM CLAUDE CODE ITSELF, AT EVERY POLL (2026-09-25).
mutant get-usage-never-used "G01" "$L" \
    '    g = read_get_usage(now){NL}    if g["state"] == "ok":' \
    '    g = {"state": "failed", "why": "mutated"}{NL}    if g["state"] == "ok":' \
    "The watcher would read only the status-line file, which an idle lead leaves ten minutes old."

mutant get-usage-process-left-running "G02" "$L" \
    '                os.killpg(proc.pid, sig)' \
    '                pass' \
    "Every poll would leave a claude process behind."

mutant get-usage-unbounded "G05" "$L" \
    '        until = time.time() + deadline_s' \
    '        until = time.time() + 3600' \
    "A claude that never answers would hang the watcher for an hour instead of falling back."

mutant get-usage-no-fallback "G03" "$L" \
    '    f = read_reading(a.payload, now){NL}    f["source"] = "the status line"' \
    '    f = dict(g){NL}    f["source"] = "the status line"' \
    "A failed get_usage would leave the watcher blind although the status-line file is fresh."

mutant watch-reads-file-only "R04" "$L" \
    '        r = read_source(a, now){NL}        w = workers(a.engine_root)' \
    '        r = read_reading(a.payload, now){NL}        w = workers(a.engine_root)' \
    "Every poll after the first would read the status-line file: the 07:57Z/08:07Z miss again."

# 9. THE FALLBACK IS REFRESHED BEFORE IT TURNS ONE POLL OLD (the pause fired at
#    95%, not 93%, on 2026-09-25: a 300 s poll first saw the file stale ~600 s on).
mutant refresh-never "W12" "$L" \
    '                if r["age"] >= at:{NL}                    w = workers' \
    '                if False:{NL}                    w = workers' \
    "An idle lead would be woken only once the reading was already a poll old: every 10 minutes."

mutant refresh-at-the-bound "W12" "$L" \
    '    return stale_after - min(REFRESH_AHEAD_SECONDS, stale_after // 10)' \
    '    return stale_after' \
    "The lead would be woken as the reading turns 300 s old, so its reply lands after it."

mutant refresh-not-scheduled "W12" "$L" \
    '            sleep_for = max(1, min(sleep_for, int(math.ceil(refresh_in))))' \
    '            pass' \
    "The poll would come a full interval later, past the refresh point, and see the reading stale."

mutant refresh-with-nothing-working "W13" "$L" \
    '                    if w["known"] and not w["working"]:{NL}                        print("  the reading is about to turn' \
    '                    if False:{NL}                        print("  the reading is about to turn' \
    "A quiet session would be woken every 5 minutes to refresh a number that nothing is spending."

# 10. THE POLLING NEVER STOPS (2026-09-28). The watcher exited at the 14:30Z
#     reset and on a get_usage timeout at 15:32Z, and nothing polled until it
#     was started again by hand.
mutant polling-stops-at-reset "N01" "$L" \
    '                reset_done = window_end' \
    '                reset_done = window_end{NL}                return 0' \
    "The watcher would end at the reset, as it did at 14:30Z, and nothing would poll the new window."

mutant polling-stops-at-threshold "N03" "$L" \
    '                        threshold_named[window_end] = set(w["working"]) if w["known"] else set()' \
    '                        threshold_named[window_end] = set(w["working"]) if w["known"] else set(){NL}                        return 0' \
    "The watcher would end at the pause, and nothing would poll for the release or the reset."

mutant stale-wakes-every-poll "N06" "$L" \
    '                    elif not blind_woken:{NL}                        out.wake("STALE" if stale' \
    '                    elif True:{NL}                        out.wake("STALE" if stale' \
    "A watcher that no longer exits would wake the lead at every poll of a stale reading."

mutant outage-wakes-at-once "N02" "$L" \
    '    if reach >= a.threshold:' \
    '    if True:' \
    "A get_usage timeout at 8% would wake the lead at once, as at 15:32Z, though the threshold could not have been crossed."

mutant outage-not-retried-sooner "N02" "$L" \
    '            sleep_for = max(1, min(sleep_for, _retry_seconds(a)))' \
    '            pass' \
    "A failed get_usage would wait a whole poll to be tried again, and a short outage would reach the two-poll mark."

mutant outage-ignores-measured-rise "N04" "$L" \
    '    if reach >= a.threshold:' \
    '    if False:' \
    "A timeout streak right after a steep rise would wait for the two-poll mark while the threshold may already be crossed."

mutant outage-never-wakes "N05" "$L" \
    '    if age >= cap:' \
    '    if False:' \
    "A get_usage outage far from the threshold would never wake anyone, however long it lasted."

mutant outage-reason-unkept "N02" "$L" \
    '        r["why"] += " (%s)" % "; ".join(timing' \
    '        r["why"] += "" and " (%s)" % "; ".join(timing' \
    "The next timeout would again leave no record of which request hung or what claude said."

mutant refresh-not-re-read "R03" "$L" \
    '            sleep_for = max(1, min(sleep_for, REFRESH_AHEAD_SECONDS, a.stale // 10))' \
    '            pass' \
    "After a refresh the watcher would read the status line a whole poll later, and the next refresh would come as it turns one poll old."

# 11. HIGH SPEED, ruling §108 (2026-10-04): the 15 Fable workers of 2026-09-29
#     took the five-hour window from 93% past 100% between two 5-minute checks.
mutant fast-never-polls-sooner "R06" "$L" \
    '        sleep_for = a.fast_poll if fast or expected else a.poll' \
    '        sleep_for = a.fast_poll if expected else a.poll' \
    "At high speed the watcher would still read every 5 minutes, which is what let the window reach 100%."

mutant expected-rise-ignored "R10" "$L" \
    '        sleep_for = a.fast_poll if fast or expected else a.poll' \
    '        sleep_for = a.fast_poll if fast else a.poll' \
    "Ten teammates started at once would be checked every 5 minutes until the burn had already been measured."

mutant jump-averaged-away "R11" "$L" \
    '    if t1 - t0 >= jump:{NL}        rates.append' \
    '    if False:{NL}        rates.append' \
    "A sudden jump would be averaged over 5 minutes, and the pause would come a check too late."

mutant fast-never-ends "R12" "$L" \
    '        fast = speed is not None and has_five(r) and speed >= fast_speed(a){NL}' \
    '        fast = (speed is not None and has_five(r) and speed >= fast_speed(a)) or same_window(fast_alerted, r["resets_at"]){NL}' \
    "After a burst the watcher would stay on 1-minute checks and the early pause point for the rest of the window."

mutant fast-poll-clamped-back "R06" "$L" \
    '            sleep_for = max(1, min(sleep_for, window_end - now))' \
    '            sleep_for = max(1, min(a.poll, window_end - now))' \
    "The window-end clamp would put the 5-minute interval back, and the 1-minute check would never happen."

mutant early-wake-never "R09" "$L" \
    '            if verdict == "below" and early_wake(a, r["used"], speed):' \
    '            if False:' \
    "At high speed the pause would wait for 93%, and at some poll phases the window reaches 100% before the hold lands."

mutant fast-alert-never "R08" "$L" \
    '            out.wake("FAST", _capture(_emit_fast, a, r, speed, now, w), now)' \
    '            pass' \
    "The CEO would not be told that usage is running at high speed."

mutant fast-alert-every-poll "R08" "$L" \
    '        if fast and r["state"] == "ok" and not r["ended"] and not same_window(fast_alerted, r["resets_at"]):' \
    '        if fast and r["state"] == "ok" and not r["ended"]:' \
    "The lead would be woken with the same alert at every 1-minute check."

mutant normal-read-as-fast "R07" "$L" \
    '    return (100.0 - a.threshold) / float(a.poll + a.hold)' \
    '    return (100.0 - a.threshold) / float(3 * a.poll + a.hold)' \
    "Ordinary work at its fastest would count as high speed: an alert for nothing and twice the checks."

# THE DOORBELL EVERY SESSION STARTS WITH: the plugin monitor.
mutant monitor-delivers-nothing "M01" "$L" \
    '                sys.stdout.write(ev["text"])' \
    '                pass' \
    "The monitor would run all session and never wake the lead."

mutant monitor-outlives-session "M01" "$L" \
    '    try:{NL}        while alive():' \
    '    try:{NL}        while True:' \
    "A monitor would keep running after its session ended."

mutant poller-outlives-session "M01" "$L" \
    '    alive = session_alive_check(a.session_pid) if a.session_pid else None' \
    '    alive = None' \
    "A detached poller would poll forever after its session ended."

# A DEAD LOGIN SEEN BY THE WATCHER REACHES THE CEO (2026-09-26).
mutant login-refusal-unreported "G09" "$L" \
    '    if g.get("auth_failed"):{NL}        told = ' \
    '    if False:{NL}        told = ' \
    "get_usage answering 'Login expired' would fall back quietly and nobody would be told to run /login."

mutation_end
