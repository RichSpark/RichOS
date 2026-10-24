#!/usr/bin/env bash
#
# workspaces.mutation.sh — PROVES the workspace spec's suite CAN FAIL, one point
# at a time. Invoked by workspaces.test.sh; the loop is mutation-harness.sh.
# Every mutant removes ONE rule of docs/plans/worktree-spec-2026-09-11.md from
# mega-lander/workspaces.py in a throwaway copy of the engine and names the
# test, named after its point, that must go red.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail
[ -n "${RICHOS_MUTATION_INNER:-}" ] && exit 0
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=mutation-harness.sh
. "$SCRIPT_DIR/../../scripts/lib/mutation-harness.sh"
mutation_begin "workspaces.py (the CEO's workspace spec)" "mega-lander/tests/workspaces.test.sh"
# Each want names a test method of workspaces.test.py, which runs only the tests
# named on its command line; so a mutant runs its own test, on the unmutated copy
# (must pass) and then mutated (must fail), rather than all 103. Measured
# 2026-09-23: 103 tests take 60 s, one takes about a second. See mutation_focus.
mutation_focus want-as-argument

W="mega-lander/workspaces.py"

mutant p01-non-cc-accepted "test_point_01_every_non_native_workspace_is_named_cc" "$W" \
    '    if not (branch or "").startswith(CC_PREFIX):{NL}        raise SpecError("branch %r is not named cc/' \
    '    if False:{NL}        raise SpecError("branch %r is not named cc/' \
    "a non-native workspace not named cc/ would be registered (points 1, 3)."

mutant p02-codex-branch-deleted "test_point_02_codex_is_never_touched" "$W" \
    '    if b.startswith(CODEX_PREFIX):{NL}        return None, "branch %s is codex/' \
    '    if False:{NL}        return None, "branch %s is codex/' \
    "a codex/ branch named for deletion would be deleted (point 2)."

mutant p03-registration-without-identity "test_point_03_failed_registration_means_no_spawn" "$W" \
    '    if not ident:{NL}        raise SpecError("the session'"'"'s process identity could not be read from the operating system, "{NL}                        "so its end' \
    '    if False:{NL}        raise SpecError("the session'"'"'s process identity could not be read from the operating system, "{NL}                        "so its end' \
    "a registration that could never tell its session ended would be accepted, so a failed registration would still spawn (point 3)."

mutant p03-unregistered-ignored "test_point_03_unregistered_workspace_and_branch_are_finished_work" "$W" \
    '    made = []{NL}    for repo, path, branch, kind in found:' \
    '    made = []{NL}    for repo, path, branch, kind in []:' \
    "a cc/ or native workspace with no registration would sit forever, never counted as finished work (point 3, hole 6)."

mutant p03-worktree-session-allowed "test_point_03_claude_worktree_sessions_are_not_allowed" "$W" \
    '        if s and s.get("forbidden"):' \
    '        if False:' \
    "a claude --worktree session would be allowed to work (point 3)."

# 2026-09-22: a SUBAGENT's compaction fired SessionStart with the lead's
# session_id and the subagent's worktree, and locked the lead out for 33 minutes.
# The lock-out itself is refused by three independent rules (the platform's
# launch record, the first record, and no refusal from a compact's cwd), so no
# single removal turns "never locks out the lead" red; each rule is proven by
# the case it alone decides.
mutant p03-subagent-compaction-is-the-lead "test_point_03_a_subagent_compaction_gets_none_of_the_leads_context" "$W" \
    '    foreign = bool(payload_cwd and lead_cwd and payload_cwd != lead_cwd and source not in LAUNCH_SOURCES)' \
    '    foreign = False' \
    "a subagent's compaction would be taken for the lead's own start (point 3, 2026-09-22)."

mutant p03-first-record-overwritten "test_point_03_without_the_platform_record_the_first_record_decides" "$W" \
    '    elif prev.get("cwd"):' \
    '    elif False:' \
    "with no platform record, a later SessionStart would move the lead's recorded directory (point 3)."

mutant p03-platform-record-not-asked "test_point_03_a_session_launched_in_a_workspace_is_still_forbidden" "$W" \
    '    if plat and plat.get("cwd"):{NL}        lead_cwd, basis = realpath(str(plat["cwd"])), "platform"' \
    '    if False:{NL}        lead_cwd, basis = realpath(str(plat["cwd"])), "platform"' \
    "the directory a session was LAUNCHED in would be taken from a hook payload instead of the platform's own record (point 3)."

mutant p03-stored-flag-trusted "test_point_03_a_wrong_stored_flag_is_re_derived_at_the_verdict" "$W" \
    '    plat = platform_session(session_id, rec.get("pid"))' \
    '    plat = None' \
    "a wrong FORBIDDEN on the lead's record would stand for the rest of the session, as it did on 2026-09-22 (point 3)."

mutant p03-subagent-gets-lead-context "test_point_03_a_subagent_compaction_gets_none_of_the_leads_context" "$W" \
    '        if foreign:{NL}            # A subagent'"'"'s compaction' \
    '        if False:{NL}            # A subagent'"'"'s compaction' \
    "a subagent's compaction would be handed the lead's land-or-discard gate, which it can do nothing about."

mutant p03-refused-lead-cannot-stop "test_point_03_a_forbidden_lead_can_always_stop_its_agents" "$W" \
    '                stop = lead_recovery_call(payload)' \
    '                stop = ""' \
    "a refused lead could not stop its agents: ten agents at 100% CPU for 33 minutes on 2026-09-22."

mutant p03-anything-rides-along-with-a-stop "test_point_03_a_forbidden_lead_can_always_stop_its_agents" "$W" \
    '    if not cmd or not _one_plain_command(cmd):' \
    '    if not cmd:' \
    "any command chained after stop.sh would run in a session point 3 refuses."

mutant p05-refused-session-held "test_point_05_a_forbidden_session_is_told_once_and_never_held" "$W" \
    '            return True, _refused_session_notice(sid, why), ""' \
    '            pass' \
    "a session that can do nothing but stop agents would be held at every turn end, which is the spin of 2026-09-22 (42 refusals in 12 minutes)."

mutant p05-refused-notice-every-turn "test_point_05_a_forbidden_session_is_told_once_and_never_held" "$W" \
    '        if rec.get("refused_notice") == why:' \
    '        if False:' \
    "the refused session would be told the same thing at every turn end instead of once."

mutant p04-no-automatic-land "test_point_04_landed_means_workspace_and_branch_deleted_automatically" "$W" \
    '        if auto and not _past(deadline):{NL}            try:{NL}                res = land(' \
    '        if False:{NL}            try:{NL}                res = land(' \
    "merged work would stay undecided until somebody ran a command (point 4)."

mutant p05-gate-has-no-budget "test_point_05_the_gate_answers_inside_its_budget" "$W" \
    '        if auto and not _past(deadline):{NL}            try:{NL}                res = land(rec["key"], me, auto=True, deadline=deadline)' \
    '        if auto:{NL}            try:{NL}                res = land(rec["key"], me, auto=True, deadline=None)' \
    "the gate's answer would depend on finishing an unbounded scan inside somebody else's hook timeout; the platform cancels an overrun hook and discards its output, so it would decide nothing and say nothing (point 5)."

mutant p05-pending-work-blocks-spawn "test_point_05_new_work_starts_while_finished_work_is_pending" "$W" \
    '    continuation_keys = []' \
    '    if pending(sid, entity):{NL}        raise SpecError("pending work blocks new agents"){NL}    continuation_keys = []' \
    "unrelated development would freeze behind pending integration (point 5, amended 2026-09-30)."

mutant p05-unrelated-spawn-scans-pending "test_point_05_unrelated_spawn_does_not_scan_pending_work" "$W" \
    '             if continues else [])' \
    '             if True else [])' \
    "unrelated spawns would depend on scanning and auto-landing the integration backlog."

mutant r2-finished-work-not-told "test_r2_a_turn_ends_with_finished_work_pending_and_the_lead_is_told_once" "$W" \
    '    return True, "\n".join(person), fallback_notice(sid, items)' \
    '    return True, "\n".join(person), ""' \
    "with no watcher running, a turn could end with finished work pending and the lead would never hear of it (orchestration reliability R2/R5; point 5's original purpose)."
mutant r6-fallback-said-every-turn "test_r2_a_turn_ends_with_finished_work_pending_and_the_lead_is_told_once" "$W" \
    '        if rec.get("fallback_notice") == key:' \
    '        if False:' \
    "the same due work would be repeated to the lead at every turn end, the wall R6 removes."



mutant p06-native-not-registered "test_point_06_native_workspaces_registered_at_spawn_and_deleted_on_land" "$W" \
    '            _add_workspace(rec, "native", main, npath, NATIVE_BRANCH_PREFIX + agent_id, "PostToolUse[Agent]"){AND}            _add_workspace(rec, "native", main_checkout(native), native, branch, "SubagentStart")' \
    '            pass{AND}            pass' \
    "the workspace Claude Code creates would never be registered, so it would pile up (point 6)."

mutant p07-ceo-order-discarded "test_point_07_ceo_ordered_work_needs_his_word" "$W" \
    '    if ordered and not (ceo_word or "").strip():' \
    '    if False:' \
    "work the CEO ordered would be discarded without his word (point 7)."

mutant p07-continuation-keeps-old "test_point_07_unfinished_work_is_continued_by_a_new_agent" "$W" \
    '        _delete(old, [w for w in live_workspaces(old) if w.get("path")], branches=False,' \
    '        (lambda *a, **k: None)(old, [w for w in live_workspaces(old) if w.get("path")], branches=False,' \
    "the old workspaces of continued work would never be deleted when the new agent starts (point 7)."

mutant p08-uncommitted-landed "test_point_08_nothing_uncommitted_is_ever_landed" "$W" \
    '    if problems:{NL}        raise SpecError("cannot %s' \
    '    if False:{NL}        raise SpecError("cannot %s' \
    "a workspace with uncommitted work would be landed and deleted, losing it (point 8)."

mutant p09-finished-not-locked "test_point_09_a_finished_agent_is_refused_every_tool" "$W" \
    '    if fin:{NL}        return "FINISHED"' \
    '    if False:{NL}        return "FINISHED"' \
    "a restarted finished agent could write again (point 9)."

mutant p09-readonly-not-registered "test_point_09_a_restarted_read_only_agent_is_refused_every_tool" "$W" \
    '        save_agent(rec){NL}    event("registered-readonly"' \
    '        pass{NL}    event("registered-readonly"' \
    "a read-only agent would carry no registration, so the lock-out could never find it finished and a restarted Explore — which carries Bash — could write (point 9)."

mutant p09-processes-not-stopped "test_point_09_every_process_it_started_is_stopped_before_deletion" "$W" \
    '    pids = processes_in(paths, deadline)' \
    '    pids = []' \
    "a process the agent started would keep running while its workspace is deleted under it (point 9)."

mutant p09-named-path-stopped "test_finding_12_a_process_that_only_names_the_workspace_keeps_running" "$W" \
    '    hits -= keep{NL}' \
    '    hits -= keep{NL}    hits.update(p for p, a in _process_args().items() if _names_a_path(a, paths) and p not in keep){NL}' \
    "a process whose command line merely names the workspace (a reviewer, a log reader) would be stopped by a land it has nothing to do with (hunt part 4, finding 12)."

mutant p09-descendant-spared "test_finding_12_a_child_of_a_process_in_the_workspace_is_still_stopped" "$W" \
    '    grew = bool(hits)' \
    '    grew = False' \
    "a command the workspace's own process started, working on it from elsewhere, would outlive the land (point 9; the reason the argument match existed)."

mutant p09-stopped-before-the-ancestry-check "test_finding_13_a_pending_scan_leaves_unmerged_work_running" "$W" \
    '    if early:{NL}        raise SpecError(_not_landed_message(rec, early))' \
    '    if False:{NL}        raise SpecError(_not_landed_message(rec, early))' \
    "an automatic land attempt would stop a finished worker's tests and servers before finding its commits unmerged, and then leave it pending anyway (hunt part 4, finding 13)."

mutant p05-status-lands "test_finding_29_status_reports_and_changes_nothing" "$W" \
    '    items = pending(me, entity, scan=True, dry=True)' \
    '    items = pending(me, entity, scan=True)' \
    "asking for the status would land and delete eligible work and stop its processes on the way to the answer (hunt part 4, finding 29)."

mutant p05-dry-check-lands "test_finding_29_a_dry_spawn_check_changes_nothing" "$W" \
    '    items = (pending(sid, entity, deadline=_gate_deadline(GATE_SPAWN_BUDGET), dry=dry)' \
    '    items = (pending(sid, entity, deadline=_gate_deadline(GATE_SPAWN_BUDGET))' \
    "a dry spawn check, which promises to write nothing, would land and delete other eligible work before answering (hunt part 4, finding 29)."

mutant p08-setup-product-held "test_finding_14_the_setups_own_link_needs_no_waiver" "$W" \
    '            ignored = _minus_generated(w, ignored, deadline)' \
    '            pass' \
    "the workspace creator's own setup product (a per-workspace cache link) would hold every automatic land until somebody typed --ignored-not-needed (hunt part 4, finding 14)."

mutant p08-changed-setup-product-waived "test_finding_14_a_setup_product_the_agent_changed_is_its_own" "$W" \
    '        if rel in made and _fingerprint(os.path.join(w["path"], rel.rstrip("/")), deadline) == made[rel]:' \
    '        if rel in made:' \
    "an entry the engine made at creation and the agent then changed would be waived by name, and the agent's change deleted with the workspace (point 8)."

mutant p05-retry-proof-has-no-deadline "test_finding_15_a_retry_stops_at_its_budget_inside_the_attempt" "$W" \
    '                _require_landed(owner, chain, disposition.get("ignored_not_needed") or "", deadline)' \
    '                _require_landed(owner, chain, disposition.get("ignored_not_needed") or "")' \
    "a deletion retry would repeat the whole landing proof with no deadline, so one attempt could run any length of time past its stated budget (hunt part 4, finding 15)."

mutant p05-gate-retries-on-their-own-clock "test_finding_15_the_stop_gate_puts_its_retries_inside_its_budget" "$W" \
    '    retry_due(deadline=deadline)' \
    '    retry_due()' \
    "the Stop gate's retries would run on their own clock before its budget applies, so the hook could be canceled before it answers (hunt part 4, finding 15)."

mutant p03-no-snapshot-no-pair "test_point_10_a_side_branch_switched_away_from_blocks_the_land" "$W" \
    '        row = {"key": rec["key"], "call": call or "", "at": now(), "repos": snap, "tips": tips}{NL}        write_json(_slot_path(rec["key"], call), row)' \
    '        row = {"key": rec["key"], "call": call or "", "at": now(), "repos": snap, "tips": tips}' \
    "the FIRST half of the pair would record nothing, so no ref could ever be shown to be new: creation would be unobservable and every branch an agent created would be left behind (points 3, 10)."

mutant p03-created-ref-not-attributed "test_point_10_branches_created_in_a_workspace_go_with_it" "$W" \
    '                    mine.append((repo, b))' \
    '                    pass' \
    "a branch the agent created in its own workspace would never be attributed to it, so it would be left behind when its work is landed or discarded (points 3, 10)."

mutant p03-attribution-without-own-work "test_point_03_another_live_agents_branch_is_never_attributed" "$W" \
    '    return set(t for t in tips if t and not is_ancestor(repo, t, target))' \
    '    return set(t for t in tips if t)' \
    "an agent that has produced nothing of its own would still count as having work, so everything that appeared during its tool call -- a second agent's whole spawn, in the normal case of two agents in one repository -- would be on its line of history and become its own (points 3, 5, 8). Reproduced exactly that way on 2026-09-11."

mutant p03-unrelated-ref-attributed "test_point_03_a_branch_rich_cuts_in_the_main_checkout_is_never_the_agents" "$W" \
    '                if not any(t == tip or is_ancestor(repo, t, tip) or is_ancestor(repo, tip, t){NL}                           for t in own):' \
    '                if False and any(t == tip or is_ancestor(repo, t, tip) or is_ancestor(repo, tip, t){NL}                           for t in own):' \
    "co-occurrence in time would be attribution again: a ref carrying somebody else's unlanded commit, cut while the agent happened to be in a tool call, would be deleted with the agent (point 8)."

mutant p03-landed-ref-attributed "test_point_03_a_branch_rich_cuts_in_the_main_checkout_is_never_the_agents" "$W" \
    '                if target and is_ancestor(repo, tip, target):{NL}                    continue                          # entirely landed: nothing at stake' \
    '                if False:{NL}                    continue                          # entirely landed: nothing at stake' \
    "a ref that is entirely in the branch this work integrates on -- the shape of every bookmark Rich cuts in the main checkout -- would be claimed and deleted by an agent that never made it (points 1, 8)."

mutant p03-borrowed-branch-attributed "test_point_08_a_branch_that_existed_before_the_call_is_never_created_in_it" "$W" \
    '        b = set(latest["repos"][repo] or []){NL}        before = b if before is None else (before | b){NL}    return before' \
    '        b = set(latest["repos"][repo] or []){NL}        before = b if before is None else (before | b){NL}    return set()' \
    "every ref would look new, so a PRE-EXISTING branch the agent merely checked out would become the agent's and a discard would delete it with git branch -D (point 8, in the destructive direction)."

mutant p03-recorded-elsewhere-ignored "test_point_03_a_continuing_agents_branch_is_never_its_predecessors" "$W" \
    '            elsewhere = _refs_recorded_elsewhere(rec["key"])' \
    '            elsewhere = set()' \
    "a ref registered to ANOTHER agent could be attributed to this one whenever it is on the same line of history -- which is exactly what a continuation is, since the new agent's workspace is cut from the old agent's branch (points 3, 7, 8)."

mutant p10-one-workspace-left "test_point_10_a_cross_repository_agent_loses_both_workspaces_as_one" "$W" \
    '        for w in workspaces:{NL}            # Point 3' \
    '        for w in workspaces[:1]:{NL}            # Point 3' \
    "a cross-repository agent's second workspace would be left behind (point 10)."

mutant p10-observed-after-the-run-ended "test_point_10_a_branch_rich_cut_from_its_branch_is_not_the_agents" "$W" \
    '    if fin:{NL}        return "FINISHED", "agent %s (%s) is finished: %s" % (aid, rec.get("name"), why){AND}    if finished_state(rec)[0]:{NL}        _drop_snapshots(rec["key"]){NL}        return []' \
    '    if fin:{NL}        snapshot_refs(rec, str(payload.get("tool_use_id") or "")){NL}        return "FINISHED", "agent %s (%s) is finished: %s" % (aid, rec.get("name"), why){AND}    if False:{NL}        _drop_snapshots(rec["key"]){NL}        return []' \
    "a finished agent's restarted call would open a window of its own, so a branch Rich cuts in the workspace afterwards to rescue the work would be attributed to the agent and deleted with it, or would hold its land hostage (points 7, 8, 9)."

mutant p11-pause-ignored "test_point_11_a_recorded_pause_is_not_finished_and_resumes" "$W" \
    '        if pause and pause.get("at", 0) <= end.get("at", 0):' \
    '        if False:' \
    "a paused agent would be taken for finished and locked out (point 11)."

mutant p12-gone-process-not-ended "test_point_12_process_gone_finishes_its_agents" "$W" \
    '        if st == "gone":' \
    '        if False:' \
    "an agent whose session crashed would never be finished (point 12)."

mutant p12-reused-pid-trusted "test_point_12_a_reused_process_number_is_not_the_session" "$W" \
    '        elif st == "ok" and text != ident["pid_start"]:' \
    '        elif False:' \
    "a reused process number would keep an ended session running forever (point 12)."

mutant p12-other-session-steals "test_point_12_two_sessions_each_handle_their_own" "$W" \
    '        st, _ = session_state(owner, rec.get("session_identity"), cache){NL}        if st != "ended":{NL}            return False' \
    '        st, _ = session_state(owner, rec.get("session_identity"), cache){NL}        if False:{NL}            return False' \
    "a running session would handle another running session's agents (point 12)."

mutant p13-no-retry "test_point_13_a_failed_deletion_is_retried_until_it_succeeds" "$W" \
    '        if not d or d.get("next_at", 0) > now():{NL}            continue' \
    '        if True:{NL}            continue' \
    "a deletion that failed once would never be tried again (point 13)."

mutant p14-land-reads-a-moving-head "test_point_14_work_merged_onto_its_dev_branch_counts_as_landed" "$W" \
    '    tip = branch_tip(main, branch)' \
    '    tip = git(main, "rev-parse", "HEAD")[1].strip()' \
    "landed would go back to meaning \"in whatever the main checkout has checked out just now\", so work merged onto its dev branch would be reported not landed and its workspaces and branches would be left behind (points 4, 14)."

mutant p14-target-inferred-not-recorded "test_point_14_the_integration_branch_is_recorded_never_inferred" "$W" \
    '    branch = (work or {}).get("branch") or ""{NL}    if not branch:' \
    '    branch = ((worktree_list(repo) or [{}])[0].get("branch") or ""){NL}    if not branch:' \
    "the branch a land is tested against would be INFERRED from the main checkout at land time instead of read from the record, which is the one thing point 14 forbids: \"nothing infers it and nothing guesses it\"."

mutant p14-spawn-not-refused-without-a-record "test_point_14_the_integration_branch_is_recorded_never_inferred" "$W" \
    '    missing = _unrecorded_repos(repos){NL}    if missing:' \
    '    missing = _unrecorded_repos(repos){NL}    if False:' \
    "a spawn (and a cc/ workspace) would be registered with NO body of work recorded, bound to nothing — and a nothing-bound agent is judged at land time against whichever body of work is current, so a later unrelated recording moves its verdict: the guess both round-6 reviewers reproduced (point 14, \"before its first agent is spawned\")."

# The witness token here used to name test_point_14_the_integration_branch_is_
# recorded_never_inferred, which stays green under this mutant (round 7 §5:
# 44/45); the red lands at the orphan-binding test, so that is the witness.
mutant p14-nothing-bound-falls-back-to-current "test_point_14_a_record_bound_to_nothing_is_refused_never_guessed" "$W" \
    '    if work is None and chain:{NL}        return "", "", (' \
    '    if False:{NL}        return "", "", (' \
    "a chain bound to no body of work would be proved against the repository's CURRENT record at land time — the \"heals later\" fallback round 7 removed, which is a guess about which work the agent belongs to (point 14)."

mutant p14-floor-restored "test_point_14_the_integration_branch_is_recorded_never_inferred" "$W" \
    '    if repo not in cur["repos"]:{NL}        cur["repos"].append(repo){NL}        write_json(path, cur)' \
    '    if repo not in cur["repos"]:{NL}        cur["repos"].append(repo){NL}        write_json(path, cur){NL}    _m = _norm_repo(repo){NL}    if _m and not all_integration_records().get(_m):{NL}        _wl = worktree_list(_m){NL}        _b = (_wl[0].get("branch") or "") if _wl else ""{NL}        if _b:{NL}            _write_integration(_m, _b, "first-registration", "the floor")' \
    "a registration would DERIVE the integration branch from whatever the main checkout happens to be on and write it down as though it were recorded, which point 14 forbids outright: \"nothing infers it and nothing guesses it\". The derived record is also the one that cannot be corrected -- with no record the land refuses and Rich recording the branch HEALS it; with the floor's record the land refuses forever and the workspace is left behind (points 5, 14)."

mutant p14-frozen-copy-restored "test_point_14_the_integration_branch_is_recorded_never_inferred" "$W" \
    '    _bind_body_of_work(rec, repo){NL}    return w{AND}    branch = (work or {}).get("branch") or ""{NL}    if not branch:' \
    '    _ir = integration_record(repo){NL}    if _ir and _ir.get("branch"):{NL}        rec.setdefault("integration", {})[realpath(repo)] = _ir["branch"]{NL}    return w{AND}    branch = ""{NL}    for _r in chain:{NL}        branch = (_r.get("integration") or {}).get(realpath(repo)) or ""{NL}        if branch:{NL}            break{NL}    if not branch:{NL}        branch = (work or {}).get("branch") or ""{NL}    if not branch:' \
    "the branch would be FROZEN onto each agent at its registration and preferred over the live record, so a correction could never reach an agent already in flight: its land would be proved against a branch that is no longer the one this work integrates on, it would refuse forever, its workspace would be left behind and point 5 would be blocked (point 14)."

mutant p03-one-window-per-agent "test_point_14_two_of_one_agents_own_calls_open_at_once_keep_both_windows" "$W" \
    '    if call:{NL}        return os.path.join(_refs_dir(key), "c." + _key_segment(call) + ".json"){NL}    return os.path.join(_refs_dir(key), "u.%020d.%s.json" % (time.time_ns(), os.urandom(4).hex()))' \
    '    return os.path.join(_refs_dir(key), "one-slot.json")' \
    "the creation window would go back to ONE SLOT PER AGENT, so two of the agent's own calls open at once would clobber each other's window: the second Post would find nothing to compare against, and a ref created in that call -- a side branch carrying real commits included -- would be attributed to nobody while land() still reported LANDED (points 3, 5, 8, 10)."

# REPLACED, AND THE REASON IS THE POINT OF THIS ROUND. This mutant used to turn
# `all_open=True` into `all_open=False` and claimed the difference was
# load-bearing. It is not any more: since the before-sets are UNIONED rather
# than intersected, and the last snapshot always joins that union, WHICH open
# windows get consumed no longer changes what is attributed -- only whether
# they are left on disk for a finished agent, which nothing reads. Keeping it
# would have been a dead assertion wearing the shape of a rule, which is the
# class of defect this round exists to end. What IS still load-bearing is that
# the end-of-run signal observes AT ALL, so that is what is mutated.
mutant p03-no-end-of-run-observation "test_point_14_two_of_one_agents_own_calls_open_at_once_keep_both_windows" "$W" \
    '    observe_created_refs(rec, all_open=True){NL}    event("end", key=rec["key"], signal=signal_name, detail=detail)' \
    '    event("end", key=rec["key"], signal=signal_name, detail=detail)' \
    "the end-of-run signal would stop being an agent's LAST observation, so a ref created in a call whose PostToolUse never arrived -- because the run ended inside it -- would be attributed to nobody and left behind (points 3, 10)."

mutant p03-end-of-run-ignores-the-last-snapshot "test_point_03_a_ref_created_after_the_last_post_is_still_the_agents" "$W" \
    '    if all_open and latest and isinstance(latest.get("repos"), dict):{NL}        priors.append(latest)' \
    '    if False:{NL}        priors.append(latest)' \
    "the end-of-run signal would observe only windows still OPEN, so a ref created by the agent's own backgrounded process after its last PostToolUse consumed the window would be compared against nothing, attributed to nobody, and left behind by a land that reports success (points 3, 9, 10)."

mutant p03-backgrounded-window-consumed-at-its-post "test_point_03_a_backgrounded_calls_ref_after_its_post_is_still_the_agents" "$W" \
    '    background = str(payload.get("tool_name") or "") == "Bash" and bool(ti.get("run_in_background"))' \
    '    background = False' \
    "the platform's run_in_background stamp would be ignored, so a backgrounded call's window would be consumed at its Post and the ref its process creates afterwards -- which the next call's snapshot already holds -- would be attributed to nobody and left behind (points 3, 9, 10)."

mutant p14-unmerged-counts-as-landed "test_point_14_work_merged_nowhere_is_not_landed" "$W" \
    '                elif not is_ancestor(repo, out.strip(), tip):{AND}        elif t and not is_ancestor(repo, t, tip):' \
    '                elif False:{AND}        elif False:' \
    "a branch that reached neither main nor its dev branch would be counted as landed, and deleted (points 4, 8, 14)."

mutant p14-failed-read-counts-as-landed "test_point_14_a_failed_read_of_a_workers_commit_or_branch_never_proves_it_landed" "$W" \
    '                elif rc != 0:{NL}                    missing.append("HEAD of %s could not be read{AND}        if unread:{NL}            missing.append("branch %s: %s" % (b, unread))' \
    '                elif False:{NL}                    missing.append("HEAD of %s could not be read{AND}        if False:{NL}            missing.append("branch %s: %s" % (b, unread))' \
    "a worker HEAD and branch tip that git could not READ would be passed over, so a commit in no integration branch would be counted as landed and its workspace deleted (points 4, 8, 14; hunt part 4, finding 4)."

mutant p08-unreadable-ignored-directory-passed-over "test_point_08_an_unreadable_ignored_directory_is_never_certified_unchanged" "$W" \
    '    for root, dirs, files in os.walk(mine, followlinks=False, onerror=unreadable):' \
    '    for root, dirs, files in os.walk(mine, followlinks=False):' \
    "an ignored directory that cannot be listed would be skipped by the walk and reported as having no differences, although it may hold the only copy of a needed file (point 8; hunt part 4, finding 5)."

mutant p08-directory-git-could-not-open-passed-over "test_point_08_an_unreadable_ignored_directory_is_never_certified_unchanged" "$W" \
    '        (ignored if ic == 0 else dirty).append(entry)' \
    '        pass' \
    "a directory git status could not open -- it warns on stderr, exits 0 and lists nothing -- would count as nothing to compare, so an unreadable ignored directory would be certified unchanged (point 8; hunt part 4, finding 5)."

mutant p13-unreadable-branch-recorded-deleted "test_point_13_a_branch_whose_tip_cannot_be_read_is_never_recorded_deleted" "$W" \
    '    if unread:{NL}        return False, "branch %s: %s" % (b, unread)' \
    '    if False:{NL}        return False, "branch %s: %s" % (b, unread)' \
    "a branch whose tip git could not READ would be reported already gone, and the record would call it deleted while it still exists (points 10, 13; hunt part 4, finding 4)."

mutant p03-background-window-consumed-at-the-next-observation "test_point_03_a_background_window_outlives_the_next_observation" "$W" \
    '        ended = all_open{NL}        if not ended:' \
    '        ended = True{NL}        if not ended:' \
    "a backgrounded call's window would be consumed at the agent's next observation while its process may still run, so a branch the process makes after a second, whole call would be attributed to nobody and left outside every integration branch by a land that reports success (points 3, 9, 10; hunt part 4, finding 6)."

mutant p08-background-window-never-closes "test_point_08_a_background_window_closes_when_its_shell_is_confirmed_ended_and_not_before" "$W" \
    '            ended = state == "ended" or (state == "absent" and bool(prior.get("hold_seen")))' \
    '            ended = False' \
    "a background window would stay open after agent_hold's record shows its shell ended, so a ref Rich cuts at the agent's tip afterwards would be attributed to the agent and deleted by its discard (point 8; the reason the window used to close, kept where it holds)."

mutant p08-ended-background-window-settled-late "test_point_08_a_background_window_closes_when_its_shell_is_confirmed_ended_and_not_before" "$W" \
    '        _settle_ended_background(rec){NL}    except (OSError, ValueError, SpecError):' \
    '        pass{NL}    except (OSError, ValueError, SpecError):' \
    "a background window whose shell has ended would stay open until the next PostToolUse, so a ref Rich cuts at the agent's tip after the next call has started would be judged against it and attributed to the agent (point 8)."

mutant p03-known-stray-narrows-the-scan "test_point_03_a_known_stray_in_one_repository_never_hides_one_in_another" "$W" \
    '        its_repos = sorted(set(w.get("repo") for w in live_workspaces(r) if w.get("repo")))' \
    '        repos = its_repos = sorted(set(w.get("repo") for w in live_workspaces(r) if w.get("repo")))' \
    "one stray already known in one repository would narrow the scan to that record's repositories, so a new stray in another requested repository would stay unregistered and outside automatic handling (point 3, hole 6; hunt part 4, finding 7)."

mutant p08-refused-call-widens-the-window "test_point_03_a_refused_call_never_widens_the_window_to_the_whole_run" "$W" \
    '            before = b if before is None else (before | b){NL}    if latest and repo in (latest.get("repos") or {}):' \
    '            before = b if before is None else (before & b){NL}    if False:' \
    "the open windows would be INTERSECTED again and the last snapshot would stop narrowing them, so one window leaked by a REFUSED PreToolUse would make the end-of-run comparison ask \"what appeared while this agent existed\" instead of \"what changed during this call\" -- and a branch Rich cut between two of the agent's calls would be attributed to the agent and destroyed by its discard (point 8)."

mutant p03-unpaired-post-attributes-nothing "test_point_03_a_post_that_carries_no_call_id_still_ends_a_call" "$W" \
    '        own = [p] if os.path.exists(p) else [q for q in _open_slots(key) if not _is_background(q)][:1]' \
    '        own = [p] if os.path.exists(p) else []' \
    "a PostToolUse whose own window is missing -- the platform does not always carry tool_use_id on both halves -- would find nothing at either end, so a ref created inside that call would be attributed to nobody until the end of the run and, once the run had ended, to nobody at all (point 3)."

mutant p14-attribution-waits-for-the-record "test_point_14_attribution_never_waits_for_the_record" "$W" \
    '            _branch, target, _why_not = integration_target([rec], repo){NL}            wl = worktree_list(repo) or []' \
    '            _branch, target, _why_not = integration_target([rec], repo){NL}            if _why_not:{NL}                event("attribution-skipped", key=rec["key"], repo=repo,{NL}                      refs=fresh_refs, why=_why_not){NL}                continue{NL}            wl = worktree_list(repo) or []' \
    "attribution would WAIT for the integration branch to be recorded. It happens once, at the end of a tool call, so \"skipped\" means attributed to NOBODY PERMANENTLY: recording the branch afterwards unblocks the land and never goes back, and land() then reports success over a commit that reached no integration branch (points 5, 8, 10 through 14). The trace it wrote had one producer and no consumer."

mutant p03-borrowed-tip-counts-as-own-work "test_point_14_a_tip_the_agent_only_borrowed_is_not_its_work" "$W" \
    '                if _made_here(subject):{NL}                    tips.add(sha)' \
    '                if True:{NL}                    tips.add(sha)' \
    "every commit the workspace's HEAD ever MOVED TO would count as the agent's own work, checkouts included, so a ref Rich cut at a tip the agent merely BORROWED would be on the agent's line of work and would be deleted with it (points 3, 8)."

mutant p14-moved-recorded-branch-not-reported "test_point_14_a_recorded_branch_moved_in_an_agents_call_is_reported_and_left_alone" "$W" \
    '    _restore_protected_refs(rec, priors + [p for p in bg_priors if not p.get("judged")], latest)' \
    '    pass' \
    "a recorded branch (or a codex/ ref) moved or deleted during an agent's call by an unnamed verb, a verb the guard missed or a non-git write would go unseen: no report, and a deleted one never re-created — the doorway class no verb list can close (points 2, 14)."

mutant p14-a-move-is-put-back-again "test_point_14_the_engine_never_moves_the_recorded_branch_when_two_agents_run" "$W" \
    '                if cur is not None:{AND}    return git(repo, "update-ref", "-m", msg, "--no-deref", "refs/heads/" + branch, tip, "")' \
    '                if False:{AND}    return git(repo, "update-ref", "--no-deref", "refs/heads/" + branch, tip)' \
    "THE PRE-2026-09-14 BEHAVIOR, restored exactly: a MOVE would be written back instead of reported, by an unconditional, unattributed update-ref. That line moved refs/heads/main in richos three times in one night -- twice within twelve seconds in opposite directions, while Rich was landing -- because the check infers the writer from the SHAPE of the result and his ordinary land has the same shape as the doorway it hunts, and because each write manufactured the condition the next agent's check fired on (docs/verification/ref-write-forensics-2026-09-14.md; reproduced from nothing at docs/verification/protected-ref-oscillation-2026-09-14-logs/repro.py, where --mode destruction loses a merge he had just made)."

mutant p14-the-restore-write-is-anonymous-and-unconditional "test_point_02_the_restore_of_a_deleted_ref_is_create_only_and_never_clobbers" "$W" \
    '    return git(repo, "update-ref", "-m", msg, "--no-deref", "refs/heads/" + branch, tip, "")' \
    '    return git(repo, "update-ref", "--no-deref", "refs/heads/" + branch, tip)' \
    "the one write this check still makes would go back to the exact line that moved refs/heads/main unattributed: no old value, so it clobbers a ref somebody re-created between the deletion and the check instead of refusing; and no -m, so it lands in the reflog with an EMPTY message, which is what cost a day of forensics on 2026-09-13 (points 2, 14)."

mutant p14-protected-set-keyed-by-name-across-repositories "test_point_14_a_recorded_branch_is_protected_in_its_own_repository_only" "$W" \
    '    recorded = _protected_names(repo)' \
    '    recorded = set(w["branch"] for w in all_bodies_of_work().values() if (w or {}).get("branch"))' \
    "the protected set would go back to matching on branch NAME across every body of work, so 'main' recorded for one repository would protect -- and make writable -- refs/heads/main in every other. Three bodies of work on this machine, all three integrating on main (forensics §4)."

mutant p14-the-leads-move-reported-too "test_point_14_a_recorded_branch_moved_in_an_agents_call_is_reported_and_left_alone" "$W" \
    '                    else:{NL}                        landed = land_by_another_conversation(repo, b, old, cur){NL}                        if not landed:{NL}                            continue                    # a descendant carrying none of the agent'"'"'s work: the lead'"'"'s land{NL}                        action, why = "LANDED", landed  # except when the land record names another conversation' \
    '                    else:{NL}                        why = "moved"' \
    "the lead's own land onto the recorded branch during an agent's call would be reported as an agent's move (point 14: landing is his). It no longer UNDOES his land -- nothing does -- but a report that fires on every ordinary land is alarm fatigue, and this check's whole remaining value is that it only speaks when something is wrong."

mutant p14-end-of-run-reports-the-leads-land "test_point_14_the_leads_land_after_the_agents_last_call_is_not_undone" "$W" \
    '                elif b not in windowed:' \
    '                elif False:' \
    "the own-work rule would apply with no call open, so the lead's fast-forward of the agent's OWN branch onto the recorded one -- made after its last call, before its end signal -- would be reported as the agent's doing at the end signal (measured as a LAND REFUSAL on certification-sage-runner-round case R8, 2026-09-13, back when this branch of the rule also wrote)."

# --- THE OTHER CONVERSATION'S LAND (2026-09-17) ---------------------------
# The land lock made two threads' lands take turns. These five prove the second
# half: the thread that waited moved the branch under the other one's agents,
# and they are TOLD -- from the land record, never from the shape of the move.

mutant p14-another-conversations-land-not-reported "test_point_14_another_conversations_land_is_reported_and_this_conversations_is_not" "$W" \
    '                        landed = land_by_another_conversation(repo, b, old, cur)' \
    '                        landed = None' \
    "the before-state restored: EVERY fast-forward of the recorded branch during an agent's call would be silent, including the one made by the other conversation's land. Its agents would carry on against a base that had moved, with their snapshots, their records and their own land's before-tip stale and nothing saying so."

mutant p14-end-of-run-land-not-reported "test_point_14_another_conversations_land_is_reported_and_this_conversations_is_not" "$W" \
    '                elif b not in windowed:{NL}                    landed = land_by_another_conversation(repo, b, old, cur)' \
    '                elif b not in windowed:{NL}                    landed = None' \
    "another conversation's land made after the agent's LAST call -- the commonest moment for it, since that is when the agent is being landed and cleaned up -- would go unrecorded on the one record that outlives the run."

mutant p14-this-conversations-own-land-reported "test_point_14_another_conversations_land_is_reported_and_this_conversations_is_not" "$W" \
    '            if thread == mine:{NL}                return None             # this conversation'"'"'s own land' \
    '            if False:{NL}                return None             # this conversation'"'"'s own land' \
    "a conversation would be told about its OWN land -- the one it made, on the work of the very agents being told. Every land in a one-thread day would fire it, which is how a report becomes wallpaper and the cross-thread case it exists for stops being read."

mutant p14-terminal-land-reported-as-unattributed "test_point_14_another_conversations_land_is_reported_and_this_conversations_is_not" "$W" \
    '        return None                     # no record names this move: a hand merge at a terminal' \
    '        return "moved forward by a land the land records do not name"' \
    "a move that no land record names -- in a repository that keeps NO land records (case A) or one whose records name only other moves (case D) -- would be reported as a land by nobody. (The old early return for an empty record file is gone: the final return covers it, so a mutant on it could never fail.) A repository that keeps NO land records -- the terminal path, where Rich lands with a hand-run git merge that writes nothing -- would have every one of his lands reported as a move nothing names. Those teammates are already told at the push by guard-inflight-notify.sh; this would put an alarm on the most ordinary write a recorded branch ever receives."

mutant p14-land-attributed-by-branch-alone "test_point_14_another_conversations_land_is_reported_and_this_conversations_is_not" "$W" \
    '            if record.get("branch") != branch or record.get("commit") != found:' \
    '            if record.get("branch") != branch:' \
    "attribution would answer with the LATEST land on the branch instead of the one that made this move, so with two lands in a row an agent would be told the wrong conversation moved its base -- which is worse than being told nothing, and is exactly what the append-only record was written to prevent."

mutant p02-agent-write-inside-codex-passes-the-lock-out "test_point_02_a_codex_ref_deleted_in_an_agents_call_is_restored_and_a_move_is_reported" "$W" \
    '        cx = _codex_workspace_of(fp, str(payload.get("cwd") or "")){NL}        if cx:' \
    '        cx = _codex_workspace_of(fp, str(payload.get("cwd") or "")){NL}        if False:' \
    "a registered agent's Edit or Write aimed inside a codex/ workspace would pass the only hook that sees it (point 2: an agent never works inside a codex/ workspace)."

mutant p14-second-body-of-work-moves-the-first "test_point_14_a_second_body_of_work_never_moves_the_first_ones_agents" "$W" \
    '    for r in chain or []:{NL}        wid = (r.get("integration_work") or {}).get(main_key)' \
    '    for r in []:{NL}        wid = (r.get("integration_work") or {}).get(main_key)' \
    "the integration branch would go back to one slot per REPOSITORY, read live. Point 5 permits a second body of work to start in a repository while the first one's agents are still running, so recording the second body's branch -- which point 14 requires -- would move the first body's running agents onto it retroactively: their work is merged onto the branch they were spawned for, their land is measured against a branch they never heard of, it refuses forever, and their workspaces are stranded (points 5, 14)."

# POINT 3, WHEN THE HOOK THAT REGISTERS A SPAWN IS KILLED (2026-09-17). The
# registration is a write performed by a PreToolUse hook, and the platform runs
# the tool anyway when it kills one — so the registry can be left holding a
# named record with no agent id beside a provisional record carrying the
# platform's own start and end for the same run, and the finished work cannot
# be retired at all. These four prove the repair is load-bearing, and that it
# adopts recorded facts rather than supplying missing ones.
mutant p03-killed-registration-not-bound-at-its-post "test_point_03_a_spawn_whose_registration_was_killed_is_bound_at_its_post" "$W" \
    '        if not rec and name and NAME_RE.match(name):' \
    '        if False:' \
    "a spawn whose PreToolUse registration was killed would stay unbound at its Post, although the platform delivers the name and the agent id together there — the first moment the gap exists (point 3)."

mutant p03-missed-spawn-never-reconciled "test_point_03_a_spawn_the_registry_never_saw_is_reconciled_from_the_platforms_own_record" "$W" \
    '    if not rec or rec.get("agent_id") or rec.get("provisional") or rec.get("orphan"):{NL}        return None' \
    '    if True:{NL}        return None' \
    "a registration the registry never bound to an agent would never adopt the platform's own record of which agent it became, so its finished work could be retired only by hand — which is what point 11 forbids (\"automatically and never by Rich noticing\")."

mutant p03-reconciliation-picks-a-candidate "test_point_03_an_ambiguous_platform_record_is_never_guessed_at" "$W" \
    '    if len(hits) != 1:{NL}        return None' \
    '    if not hits:{NL}        return None' \
    "two of the platform's records answering to one name would be resolved by taking the first, which makes a reconciliation a guess between candidates instead of the adoption of a fact (point 3)."

mutant p03-reconciliation-invents-an-ending "test_point_03_a_reconciled_binding_never_invents_an_ending" "$W" \
    '        prov = _absorb_provisional(fresh, prov_key)' \
    '        prov = _absorb_provisional(fresh, prov_key){NL}        fresh["end"] = fresh.get("end") or {"at": now(), "signal": "SubagentStop", "detail": ""}' \
    "adopting the binding would also declare the run over, so an agent still working would be landed out from under itself. The platform's per-agent record proves WHICH agent the call became and says nothing about whether it has ended (points 3, 11)."

# THE SECOND REVIEW (CEO §113, 2026-10-09, slice 3 of the automatic second
# review): work lands only with a passing review of exactly its tip. The rule
# lives in scripts/lib/operator_fences.py (the fence and the land command both
# ask it); the land command asks it first, and the delivery hook tells a
# running teammate once.
F="scripts/lib/operator_fences.py"
mutant sr-no-refusal-anywhere "test_second_review_merge_is_refused_with_no_verdict" "$F" \
    '            why += ["  P%s %s (%s)" % f for f in review_findings(row)]{NL}        gaps[tip] = why' \
    '            why += ["  P%s %s (%s)" % f for f in review_findings(row)]{NL}        pass' \
    "work with no passing second review would land by every path: the land command and the fence would both let it in (CEO §113)."

mutant sr-land-command-does-not-ask "test_second_review_merge_is_refused_with_no_verdict" "$W" \
    '        ledgers, broken = _review_ledgers(F, main, listed, recorded)' \
    '        ledgers, broken = [], ""' \
    "workspaces.sh merge would run git's merge gate for minutes before the fence refused the move, and with the fence off it would land unreviewed work (CEO §113, Sage's catch 8)."

mutant sr-fence-does-not-ask "test_second_review_a_plain_merge_is_refused_by_the_fence_the_same_way" "$F" \
    '            if gaps:{NL}                refused.append(((old, new, ref), REVIEW_WHY' \
    '            if False:{NL}                refused.append(((old, new, ref), REVIEW_WHY' \
    "a plain git merge, a fast-forward or a codex/ branch's land would bring unreviewed work into main (Sage's catches 1 and 8)."

mutant sr-any-commit-verdict-counts "test_second_review_merge_is_refused_with_a_verdict_on_an_older_commit" "$F" \
    '    land = [r for r in mine if r.get("tip") == tip and r.get("trigger") in REVIEW_LAND_KINDS]' \
    '    land = [r for r in mine if r.get("trigger") in REVIEW_LAND_KINDS]' \
    "a verdict on an older commit would let newer, unreviewed commits land (plan §4 row 3)."

mutant sr-fast-forward-not-seen "test_second_review_a_fast_forward_and_a_direct_commit_are_refused_an_empty_commit_is_not" "$F" \
    '    if len(line[-1]) < 2 or line[-1][1] != old:{NL}        return [new]' \
    '    if len(line[-1]) < 2 or line[-1][1] != old:{NL}        return []' \
    "a fast-forward to unreviewed work made elsewhere would land it (plan §4 row 3)."

mutant sr-replacement-taken-for-a-rollback "test_second_review_a_divergent_replacement_of_main_needs_a_review_a_rollback_does_not" "$F" \
    '        return [] if rc == 0 else [new]' \
    '        return []' \
    "a reset of main to an unreviewed divergent commit would land its work, taken for a rollback (review rv-20261009T023055Z-b51ebb97-bc65, finding 1)."

mutant sr-merge-own-changes-unseen "test_second_review_a_merge_with_changes_of_its_own_needs_a_review_of_the_merge_itself" "$F" \
    '        if not clean_merge(cwd, c, parents, keep_git_env):{NL}            return [new]' \
    '        if False:{NL}            return [new]' \
    "a merge carrying a file of its own would land on its branch's review alone (review rv-20261009T023055Z-b51ebb97-bc65, finding 2)."

mutant sr-unspecified-old-not-read "test_second_review_an_update_ref_with_no_old_value_is_judged_from_the_real_main" "$F" \
    '            moved_from = current if is_zero(old) and current is not None else old' \
    '            moved_from = old' \
    "a move of main by \`git update-ref\` with no expected old value would be judged without the real main, so a rollback is refused (review rv-20261009T025426Z-15dae5ca-3cc1, finding 1)."

mutant sr-creation-lands-nothing "test_second_review_an_update_ref_with_no_old_value_is_judged_from_the_real_main" "$F" \
    '    if is_zero(old):{NL}        return [new]' \
    '    if is_zero(old):{NL}        return []' \
    "main deleted and created again on an unreviewed commit would land it (review rv-20261009T025426Z-15dae5ca-3cc1, finding 1)."

mutant sr-merges-the-branch-name "test_second_review_merge_lands_the_commit_that_passed_not_a_newer_branch_tip" "$W" \
    '        args = ["merge", "--no-ff", "--no-edit"] + msg + [t]' \
    '        args = ["merge", "--no-ff", "--no-edit"] + msg + [b]' \
    "a commit added to the branch after its tip passed review would land unreviewed with it (review rv-20261009T025426Z-15dae5ca-3cc1, finding 2)."

mutant sr-old-verdict-dropped "test_second_review_a_verdict_waits_for_the_teammates_next_call_however_late" "scripts/lib/review_delivery.py" \
    '                if done is not None:{NL}                    r["_done"] = done' \
    '                if done is not None and time.time() - done <= 24 * 3600:{NL}                    r["_done"] = done' \
    "a verdict a teammate's next call reaches a day later would never be delivered (review rv-20261009T023055Z-b51ebb97-bc65, finding 3)."

mutant sr-delivered-every-call "test_second_review_a_running_teammates_next_tool_call_carries_a_new_verdict_once" "scripts/lib/review_delivery.py" \
    '        os.close(os.open(os.path.join(d, name), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600))' \
    '        os.close(os.open(os.path.join(d, name), os.O_WRONLY | os.O_CREAT, 0o600))' \
    "a mid-job verdict would be repeated at every tool call of the teammate instead of once (plan §2.5)."

mutant sr-launcher-decides-what-is-reviewed "test_second_review_a_listed_repository_without_its_launcher_is_still_refused" "$W" \
    '    declared = recorded or any(F.review_listed(v, main) for v in listed)' \
    '    declared = False' \
    "a repository the declaration lists would merge unreviewed work whenever its fence launcher is missing (review rv-20261009T031207Z-ba444a8a-607a, finding 1)."

mutant sr-unreadable-launcher-ignored "test_second_review_a_listed_repository_whose_launcher_cannot_be_read_is_refused" "$W" \
    '    except FileNotFoundError:{NL}        text = ""                               # no launcher: the declarations alone decide' \
    '    except OSError:{NL}        text = ""' \
    "a listed repository whose review setup cannot be established would merge as if it were established (review rv-20261009T031207Z-ba444a8a-607a, finding 1)."

mutant sr-unreadable-declaration-skipped "test_second_review_a_governing_declaration_that_cannot_be_read_is_refused" "$W" \
    '        if exc.errno not in (errno.ENOENT, errno.ENOTDIR):{NL}            return None, "the governing declaration %s cannot be read' \
    '        if False:{NL}            return None, "the governing declaration %s cannot be read' \
    "a governing orchestration.config made unreadable would be read as listing nothing, and with the launcher gone its repository would merge unreviewed work (review rv-20261009T033652Z-f3bfe22f-9372, finding 1)."

mutant sr-fence-entity-not-consulted "test_second_review_the_entity_the_fence_was_installed_from_governs_without_the_launcher" "$W" \
    '        if mapping[key]:{NL}            governing[os.path.realpath(os.path.expanduser(mapping[key]))] = True' \
    '        if mapping[key]:{NL}            pass' \
    "a repository listed only by the entity its fence was installed from would merge unreviewed work once the launcher is removed (review rv-20261009T033652Z-f3bfe22f-9372, finding 2)."

mutant sr-legacy-entity-inferred "test_second_review_a_legacy_entry_installed_in_the_same_second_as_another_is_unknown" "scripts/lib/operator_fences.py" \
    '        out[main] = v if isinstance(v, str) and v.strip() else ""' \
    '        out[main] = v if isinstance(v, str) and v.strip() else (reg.get("entity") or "")' \
    "a registry entry from before entries carried an entity would be credited the registry's top-level entity, which the earlier installer overwrote at every install, an upgrade install would record that permanently, and with the launcher removed that repository would merge unreviewed work (review rv-20261009T040812Z-9efed042-d03e, finding 1)."

mutant sr-unknown-governing-entity-skipped "test_second_review_a_repository_whose_governing_entity_is_unknown_is_refused" "$W" \
    '        else:{NL}            unknown.append("the fence registry records a fence for %s' \
    '        elif False:{NL}            unknown.append("the fence registry records a fence for %s' \
    "a repository whose governing entity cannot be established would be landed as if nothing governed it (review rv-20261009T035450Z-882c7e75-0eab, finding 1)."

mutant sr-every-registry-entity-read "test_second_review_only_the_declarations_governing_the_landed_repository_are_read" "$W" \
    '    if key in mapping:{NL}        if mapping[key]:' \
    '    for e in mapping.values():{NL}        if e:{NL}            governing.setdefault(os.path.realpath(os.path.expanduser(e)), True){NL}    if key in mapping:{NL}        if mapping[key]:' \
    "an unreadable declaration governing only another repository would block every land (review rv-20261009T035450Z-882c7e75-0eab, finding 2)."

mutant sr-continuation-judges-by-its-own-registration "test_second_review_a_continuation_gets_a_verdict_on_the_work_it_continues" "scripts/lib/review_delivery.py" \
    '    texts = [render(r) for r in rows if r.get("work") in works and r["_done"] >= works[r["work"]] - 1' \
    '    texts = [render(r) for r in rows if r.get("work") in works and r["_done"] >= works["teammate:%s" % rec["key"]] - 1' \
    "a verdict on a predecessor's work that finished before its continuation was registered would never reach the continuation (review rv-20261009T031207Z-ba444a8a-607a, finding 2)."

# "NOT REVIEWED" NEEDS POSITIVE EVIDENCE FOR EVERY GOVERNING ENTITY (review
# rv-20261009T042243Z-42ecc969-5f85, the fourth review in a row to find the
# answer falling back to "not reviewed" with the launcher gone). Each mutant
# brings back one of the fallbacks that review's fix closed.
mutant sr-recorded-entity-without-declaration-lists-nothing "test_second_review_a_recorded_entity_whose_declaration_is_gone_is_refused" "$W" \
    '        if recorded:{NL}            return None, ("the fence registry records that' \
    '        if False:{NL}            return None, ("the fence registry records that' \
    "deleting the orchestration.config of the entity a fence was installed from, or retiring its worktree, would read as listing nothing, and with the launcher gone an unreviewed tip would land (review rv-20261009T042243Z-42ecc969-5f85, finding 1)."

mutant sr-gone-spawning-entity-lists-nothing "test_second_review_a_spawning_entity_that_no_longer_exists_is_refused" "$W" \
    '        if not here:{NL}            return None, ("the entity %s that governs this work is gone' \
    '        if False:{NL}            return None, ("the entity %s that governs this work is gone' \
    "a spawning entity whose directory is gone would be taken for one that never had a declaration, and its work would land unreviewed (review rv-20261009T042243Z-42ecc969-5f85)."

mutant sr-dangling-declaration-lists-nothing "test_second_review_a_declaration_that_is_a_link_to_nothing_is_refused" "$W" \
    '        if os.path.lexists(config):{NL}            return None, "the governing declaration %s is a link to nothing"' \
    '        if False:{NL}            return None, "the governing declaration %s is a link to nothing"' \
    "an orchestration.config that is a link to nothing would be taken for no declaration at all (review rv-20261009T042243Z-42ecc969-5f85)."

mutant sr-ungoverned-work-unreviewed "test_second_review_work_that_no_declaration_governs_is_refused" "$W" \
    '    if not governing and not unknown:{NL}        unknown.append(' \
    '    if False:{NL}        unknown.append(' \
    "work that no declaration is known to govern would land as unreviewed with nothing read at all (review rv-20261009T042243Z-42ecc969-5f85)."

# THE INSTALL IS THE ONE DURABLE RECORD (review rv-20261009T044823Z-fc8c7569-5919,
# the eighth to find a declaration read at land time disappearing): install
# records whether a repository requires review, and removing or breaking a
# declaration afterwards never switches it off. Each mutant brings back one of
# that review's three fallbacks.
mutant sr-install-record-not-written "test_second_review_an_installed_repository_stays_reviewed_until_the_next_install" "scripts/lib/operator_fences_admin.py" \
    '"entity": entity, "reviewed": bool(declared_reviews(decl, r["main"]))}' \
    '"entity": entity}' \
    "install would keep no record that a repository requires review, so deleting or editing its declarations afterwards would land unreviewed work with the launcher gone (review rv-20261009T044823Z-fc8c7569-5919, finding 1)."

mutant sr-install-record-not-read "test_second_review_an_installed_repository_stays_reviewed_until_the_next_install" "$W" \
    '    return values, unknown, key in reviewed' \
    '    return values, unknown, False' \
    "the land would ignore the install's record, and a declaration removed or edited after the install would switch the review off (review rv-20261009T044823Z-fc8c7569-5919, finding 1)."

mutant sr-dangling-registry-absent "test_second_review_a_registry_or_launcher_that_is_a_link_to_nothing_is_refused" "$W" \
    '        if os.path.lexists(path):{NL}            return {}, ["the fence registry %s is a link to nothing"' \
    '        if False:{NL}            return {}, ["the fence registry %s is a link to nothing"' \
    "a fence registry whose target is gone would read as nothing installed, hiding the entity whose declaration lists the repository (review rv-20261009T044823Z-fc8c7569-5919, finding 2)."

mutant sr-dangling-launcher-absent "test_second_review_a_registry_or_launcher_that_is_a_link_to_nothing_is_refused" "$W" \
    '        if os.path.lexists(path):{NL}            return [], ("%s'"'"'s fence launcher %s is a link to nothing' \
    '        if False:{NL}            return [], ("%s'"'"'s fence launcher %s is a link to nothing' \
    "a fence launcher whose target is gone would read as no launcher, hiding the review ledger it carries (review rv-20261009T044823Z-fc8c7569-5919, finding 2)."

# ONE READER, AND IT ASKS THE SHELL (reviews rv-20261009T050654Z-e21238ff-7685 and
# rv-20261009T052655Z-daf09e19-0bc6): F.review_repos is the only reader of
# SECOND_REVIEW_REPOS, for install, the land and review-watch; it sources the
# declaration in bash and checks the value bash sets.
mutant sr-reader-ignores-source-failure "test_second_review_the_reader_returns_the_value_bash_sets" "scripts/lib/operator_fences.py" \
    '_REVIEW_READ = ('"'"'. "$1" >/dev/null </dev/null || exit\n'"'"'' \
    '_REVIEW_READ = ('"'"'. "$1" >/dev/null </dev/null\n'"'"'' \
    "a declaration bash cannot source cleanly (an unterminated quote, a failing last command) would be read for whatever it had set by then, or as unset (review rv-20261009T052655Z-daf09e19-0bc6)."

mutant sr-reader-takes-any-value "test_second_review_the_reader_returns_the_value_bash_sets" "scripts/lib/operator_fences.py" \
    '    if not _REVIEW_NAMES.fullmatch(m.group(2)):' \
    '    if False:' \
    "a value bash sets that is not names (a path from \$HOME, ..) would be taken as the list and match no repository (review rv-20261009T042243Z-42ecc969-5f85)."

mutant sr-reader-takes-trailing-output "test_second_review_the_reader_returns_the_value_bash_sets" "scripts/lib/operator_fences.py" \
    '    m = _REVIEW_OUTPUT.fullmatch(out.decode' \
    '    m = _REVIEW_OUTPUT.match(out.decode' \
    "output a sourced declaration leaves behind (an EXIT trap) would be ignored instead of refused (review rv-20261009T052655Z-daf09e19-0bc6)."

mutant sr-reader-unset-read-as-empty "test_second_review_the_reader_returns_the_value_bash_sets" "scripts/lib/operator_fences.py" \
    '"${SECOND_REVIEW_REPOS+set}"' \
    '"set"' \
    "a declaration that never sets SECOND_REVIEW_REPOS would read as set and empty, so review-watch would never announce that no repository is listed."

mutant sr-install-skips-unreadable-form "test_second_review_install_and_the_land_refuse_what_cannot_be_read" "scripts/lib/operator_fences_admin.py" \
    '    if unreadable:{NL}        # Before any launcher or registry entry is written' \
    '    if False:{NL}        # Before any launcher or registry entry is written' \
    "install would go on past a SECOND_REVIEW_REPOS line it cannot read, moving an existing hook aside before refusing (review rv-20261009T050654Z-e21238ff-7685, finding 1)."

mutant sr-registry-without-repositories-names-nothing "test_second_review_a_fence_registry_without_its_repositories_is_unknown" "$W" \
    '    if not isinstance(reg, dict) or not isinstance(reg.get("repositories"), dict):' \
    '    if not isinstance(reg, dict) or not isinstance(reg.get("repositories", {}), dict):' \
    "a fence registry with no repositories table would be read as naming no entity (review rv-20261009T042243Z-42ecc969-5f85)."

mutant sr-unlisted-unreadable-launcher-ignored "test_second_review_an_unreadable_launcher_is_refused_even_where_no_declaration_lists_it" "$W" \
    '        return [], ("%s'"'"'s fence launcher %s cannot be read' \
    '        return [], ""{NL}        return [], ("%s'"'"'s fence launcher %s cannot be read' \
    "an existing launcher that cannot be read would be taken as carrying no review ledger (review rv-20261009T042243Z-42ecc969-5f85)."

mutant sr-unreadable-repository-has-no-registry-entry "test_second_review_a_repository_that_cannot_be_read_is_refused_even_where_no_declaration_lists_it" "$W" \
    '    if not paths:{NL}        return os.path.realpath(main), ("the repository' \
    '    if False:{NL}        return os.path.realpath(main), ("the repository' \
    "a repository Git cannot read would be looked up in the fence registry under a guessed key (review rv-20261009T042243Z-42ecc969-5f85)."

mutant sr-unlisted-unreadable-repository-unreviewed "test_second_review_a_repository_that_cannot_be_read_is_refused_even_where_no_declaration_lists_it" "$W" \
    '        return [], ("the repository %s cannot be read, so whether its fence launcher' \
    '        return [], ""{NL}        return [], ("the repository %s cannot be read, so whether its fence launcher' \
    "an unlisted repository Git cannot read would be taken as carrying no review ledger (review rv-20261009T042243Z-42ecc969-5f85)."

mutation_end
