#!/usr/bin/env bash
#
# workspace-spec-fourteen.mutation.sh — EVERY ONE OF THE FOURTEEN CHECKS, WATCHED
# GOING RED FOR ITS OWN REASON. Invoked by workspace-spec-fourteen.test.sh; the
# loop is scripts/lib/mutation-harness.sh.
#
# THE BAR, AND WHERE IT CAME FROM. Round-6 brief §6, after the CEO's ruling of
# 2026-09-12 (lifecycle-failure-record-2026-09-12.md §2d): "why is there any
# need to 'imagine' anything if there is a shitload of transcripts from previous
# sessions and this one?" So every mutant here carries one of two labels, in its
# NAME and in its reason:
#
#   R-  RECORDED      derived from a named incident: the record file, its
#                     section, the date, and the commit where the record has
#                     one. Where the record holds ANY mechanical incident for a
#                     point, at least one R- mutant is required.
#   S-  SPEC-DERIVED  the negation of the CEO's sentence, and labeled as
#                     constructed rather than found.
#
# A point with no R- mutant SAYS SO in the report rather than passing quietly:
# points 1 and 2 have no mechanical incident anywhere in the three records
# (lifecycle-failure-record-2026-09-{10,11,12}.md) — the four pre-spec
# `zach-opus-dor*` trees are excluded by the CEO's own second sentence of point
# 1 and are not a failure of this code, and no codex/ workspace has ever been
# touched — so both carry S- mutants only. The count per point is printed by
# the measurement report from the PASS lines below.
#
# EVERY WANT NAMES A SUB-ASSERTION (`C4.3 `), NEVER A CHECK (`C4 `). Check C14
# was red on the base round 6 was written against (its C14.1: the first spawn
# with no integration branch recorded was not refused), so any mutant wanting
# `FAIL  C14 ` would have been "proven" by a suite that was red before the
# mutation was applied. The trailing space in each want keeps `C1.1 ` from
# matching `C1.10`.
#
# ROUND 7 (2026-09-12) ADDED A MUTANT FOR EVERY SUB-ASSERTION IT ADDED: C14.1
# (green since the spawn refusal), the clauses both round-6 reviewers found the
# fourteen did not ask, and what the census of every sentence then found. Each
# carries its R-/S- label like the rest; the report counts them per point.
#
# Run directly: mega-lander/tests/workspace-spec-fourteen.mutation.sh
# Exit 0 = every property is proven load-bearing.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail
[ -n "${RICHOS_MUTATION_INNER:-}" ] && exit 0
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/mutation-harness.sh
. "$SCRIPT_DIR/../../scripts/lib/mutation-harness.sh"
mutation_begin "the fourteen checks of the workspace spec" "mega-lander/tests/workspace-spec-fourteen.test.sh"
# The checks share one sandbox in sequence, so a check cannot run alone; a mutant
# stops at its named sub-assertion instead. The claim this rests on is true of
# workspace-spec-fourteen.test.sh by construction: `sub` counts every FAIL line
# into its check, `verdict` counts a red check, and the last line exits non-zero
# on any red check. Measured 2026-09-23: a full run is 84 s; the named lines of
# these mutants arrive at 34 s on average. See mutation_focus.
mutation_focus stop-at-want

W="mega-lander/workspaces.py"
G="scripts/hooks/guard-worktree-removal.sh"
C="mega-lander/create-teammate-worktree.sh"
U="scripts/lib/unlanded-branches.py"
Q="scripts/hooks/guard-unresolved-claims.py"

# --- point 1: no recorded mechanical incident; SPEC-DERIVED only --------------
mutant S-p01-non-cc-registered "C1.1 " "$W" \
    '    if not (branch or "").startswith(CC_PREFIX):{NL}        raise SpecError("branch %r is not named cc/ (point 1)" % branch)' \
    '    if False:{NL}        raise SpecError("branch %r is not named cc/ (point 1)" % branch)' \
    "SPEC-DERIVED (point 1 negated; no recorded incident): a non-native workspace on a branch not named cc/ would be registered and created."
mutant S-p01-raw-worktree-add-allowed "C1.2 " "$G" \
    '        elif sub2 == "add":' \
    '        elif sub2 == "add" and False:' \
    "SPEC-DERIVED (points 1, 3 negated; no recorded incident): a raw 'git worktree add' in Bash would create a workspace registered by nothing, named anything."

# --- point 2: no recorded mechanical incident; SPEC-DERIVED only --------------
mutant S-p02-bash-guard-lets-codex-branch-go "C2.2 " "$G" \
    '                if deletes:{NL}                    _refuse_delete(kind, "git branch -d/-D", t)' \
    '                if False:{NL}                    _refuse_delete(kind, "git branch -d/-D", t)' \
    "SPEC-DERIVED (point 2 negated; no recorded incident): 'git branch -D codex/x' in Bash would delete a codex/ branch with no refusal."
mutant S-p02-cc-copy-not-required "C2.4 " "$C" \
    '    codex/*|refs/heads/codex/*)' \
    '    never-a-real-base/*)' \
    "SPEC-DERIVED (point 2 negated; no recorded incident): a cc/ workspace could be cut FROM a codex/ branch, so an agent would work on codex/ work without the copy the page requires."
mutant S-p02-library-codex-guard-removed "C2.8 " "$W" \
    '    if b.startswith(CODEX_PREFIX):{NL}        return None, "branch %s is codex/; never touched (point 2)" % b' \
    '    if False:{NL}        return None, "branch %s is codex/; never touched (point 2)" % b' \
    "SPEC-DERIVED (point 2 negated, 'Landing never deletes anything codex/'; constructed by frank-fable-c7 as F20, certification-frank-round6-2026-09-12.md §2.3, which showed round 6's byte-identity was proved by never aiming a deleter at codex/): the library's own branch deleter, aimed at a codex/ ref on an agent's record, would delete it."

# --- point 2, round 8 (item 3): the record now holds a mechanical incident for this point.
mutant R-p02-codex-deleter-passes-the-guard "C2.13" "$G" \
    '    if kind == "codex":{NL}        spec.append("%s deleting the codex/ ref %s' \
    '    if False:{NL}        spec.append("%s deleting the codex/ ref %s' \
    "RECORDED [richos-hq lifecycle-failure-record-2026-09-13.md §5, 2026-09-13: 'Point 2 of the CEO's page is violable today — an agent can delete a codex/ branch'; brief-audit-frank-round8-2026-09-13.md §3, executed in a fixture on the base: git update-ref -d refs/heads/codex/bare, push --delete . codex/bare, push . :codex/bare, push . :refs/heads/codex/bare and branch -M codex/bare not-codex all passed the guard at rc=0 and the branch was GONE; the CEO's ruling ceo-decisions.md §31 and RICH-TODOs.md:175 (5fb3e5b9, 2026-09-12): 'codex/ is closed as a topic'. No real codex/ ref has been lost on this machine (lifecycle-failure-record-2026-09-12.md: eight codex/ workspaces measured clean) — the incident is the measured violability]: the five deleters would pass the guard again."
mutant S-p02-codex-ref-not-seen-at-all "C2.9 " "$W" \
    '    _restore_protected_refs(rec, priors + [p for p in bg_priors if not p.get("judged")], latest)' \
    '    pass' \
    "SPEC-DERIVED (point 2 negated, 'a codex/ workspace or branch is never deleted'; constructed after both round-8 reviewers showed a verb list cannot close the unnamed doorway): a codex/ ref moved or deleted during an agent's call by a verb the guard missed, by the doorway or by a non-git write would go unseen — no report for a move, and a deleted one never re-created. (C2.10, C2.11 and C14.13 go red under it too.)"
mutant S-p02-a-move-is-written-back-again "C2.9 " "$W" \
    '                if cur is not None:{AND}    return git(repo, "update-ref", "-m", msg, "--no-deref", "refs/heads/" + branch, tip, "")' \
    '                if False:{AND}    return git(repo, "update-ref", "--no-deref", "refs/heads/" + branch, tip)' \
    "RECORDED [docs/verification/ref-write-forensics-2026-09-14.md, 2026-09-13/14: this line moved refs/heads/main in /Users/alex/ab/richos three times, twice within twelve seconds in opposite directions while Rich was landing, each write with an EMPTY reflog message; reproduced from nothing at docs/verification/protected-ref-oscillation-2026-09-14-logs/repro.py, whose --mode destruction loses a merge he had just made]: a MOVED protected ref would be written back instead of reported, by an unconditional, unattributed update-ref aimed at one agent's minutes-stale snapshot."
mutant S-p02-codex-tips-not-snapshotted "C2.11" "$W" \
    '            tips[repo] = _protected_tips(repo, refs)' \
    '            tips[repo] = {}' \
    "SPEC-DERIVED (point 2 negated): the snapshot would record which refs exist and not where the protected ones point, so a move could never be seen — only a deletion."
mutant S-p02-agent-writes-inside-codex-pass-the-lock-out "C2.12" "$W" \
    '        cx = _codex_workspace_of(fp, str(payload.get("cwd") or "")){NL}        if cx:' \
    '        cx = _codex_workspace_of(fp, str(payload.get("cwd") or "")){NL}        if False:' \
    "SPEC-DERIVED (point 2 negated, 'An agent never works inside a codex/ workspace'; measured passing on the base by sage-fable-b3, brief-audit-sage-round8 §4 with a registered agent): a registered agent's Edit or Write aimed inside a codex/ workspace would pass the only hook that sees it."
mutant S-p02-agent-commands-inside-codex-pass-the-guard "C2.14" "$G" \
    'if AGENT:{NL}    for how, p in _codex_workspace_paths(scan, str(d.get("cwd") or "")):' \
    'if False:{NL}    for how, p in _codex_workspace_paths(scan, str(d.get("cwd") or "")):' \
    "SPEC-DERIVED (point 2 negated, 'An agent never works inside a codex/ workspace'): a commit or a shell redirect run with the call's cwd inside a codex/ workspace, or aimed there by git -C / cd, would pass the Bash guard."

# --- point 3 ---------------------------------------------------------------
mutant R-p03-registration-without-identity "C3.1 " "$W" \
    '    ident = identity_for(sid){NL}    if not ident:{NL}        raise SpecError("the session'"'"'s process identity could not be read from the operating system; "' \
    '    ident = identity_for(sid){NL}    if False:{NL}        raise SpecError("the session'"'"'s process identity could not be read from the operating system; "' \
    "RECORDED [lifecycle-failure-record-2026-09-10.md §3.4, 2026-09-10: create-teammate-worktree.sh wrote an ownership row with no agent id; brief-audit-sage-round6 §4.2: the live spec store's one record carries agent_id \"\"]: a spawn whose registration could never tell its session ended would proceed."
mutant R-p03-unregistered-never-listed "C3.4 " "$W" \
    '    made = []{NL}    for repo, path, branch, kind in found:' \
    '    made = []{NL}    for repo, path, branch, kind in []:' \
    "RECORDED [lifecycle-failure-record-2026-09-10.md §3.1 and §2.11, 2026-09-10: 'Nothing made removing a finished one happen'; five finished branches never landed, found by the CEO in his IDE]: a cc/ or native workspace with no registration would sit forever and never be pending work."
mutant S-p03-worktree-session-allowed "C3.3 " "$W" \
    '        if s and s.get("forbidden"):' \
    '        if False:' \
    "SPEC-DERIVED (point 3 negated): a session started inside its own workspace would be allowed every tool."
mutant S-p03-claude-w-allowed "C3.2 " "$G" \
    'if CLAUDE_WT.search(scan):' \
    'if False:' \
    "SPEC-DERIVED (point 3 negated): 'claude --worktree' / 'claude -w' in Bash would not be refused."
mutant R-p03-unregistered-branch-never-listed "C3.8 " "$W" \
    '        for b in local_branches(repo, ["cc/", NATIVE_BRANCH_PREFIX + "*"]) or []:' \
    '        for b in []:' \
    "RECORDED [lifecycle-failure-record-2026-09-10.md §1 and §2.11, 2026-09-10: eight leftover branches across three repositories found by the CEO in his own IDE, five of them unlanded work; lifecycle-failure-record-2026-09-12.md addendum §A2: 22 cc/ branches gone with no record]: a cc/ or native BRANCH with no workspace and no registration would never be listed as finished work of an ended session."

# --- point 4 ---------------------------------------------------------------
mutant R-p04-branch-left-after-land "C4.3 " "$W" \
    '    if branches and not held:{NL}        for repo, b in _branch_targets([rec]):' \
    '    if False:{NL}        for repo, b in _branch_targets([rec]):' \
    "RECORDED [lifecycle-failure-record-2026-09-12.md §2c, 2026-09-12: 'git branch --contains 6fd5aef8' returned cc/frank-opus-c6, cc/sage-opus-c6, cc/zach-opus-g4 after their workspaces were cut and left; and the same record's addendum §A2: 22 cc/ branches deleted by hand today, none by the system]: a land would delete the workspace and leave the branch."
mutant R-p04-quarantine-instead-of-delete "C4.4 " "$W" \
    '            rc, _o, err = git(main, "worktree", "remove", "--force", "--force", path, timeout=_bounded(deadline, 300))' \
    '            rc, _o, err = git(main, "worktree", "move", path, os.path.join(os.path.dirname(path), ".richos-retired-" + os.path.basename(path)), timeout=_bounded(deadline, 300))' \
    "RECORDED [lifecycle-failure-record-2026-09-12.md addendum §A4, 2026-09-12 16:12Z and 16:13Z: the running engine's removal path renamed cc/sage-opus-c6 and cc/frank-opus-c6 into richos-wt/.richos-retired/ and both are still registered git worktrees; brief-audit-frank-round6 P7; brief-audit-sage-round6 §4.4]: a land would quarantine the workspace instead of deleting it."
mutant S-p04-no-automatic-land "C4.1 " "$W" \
    '        if auto and not _past(deadline):{NL}            try:{NL}                res = land(rec["key"], me, auto=True, deadline=deadline)' \
    '        if False:{NL}            try:{NL}                res = land(rec["key"], me, auto=True, deadline=deadline)' \
    "SPEC-DERIVED (point 4 negated, 'automatically, with nothing left undecided'): merged work would stay undecided until somebody ran a command."
mutant S-p04-quarantine-under-another-name "C4.4 " "$W" \
    '            rc, _o, err = git(main, "worktree", "remove", "--force", "--force", path, timeout=_bounded(deadline, 300))' \
    '            rc, _o, err = git(main, "worktree", "move", path, os.path.join(os.path.dirname(path), ".parked-" + os.path.basename(path)), timeout=_bounded(deadline, 300)); (entry and entry.get("branch") and git(main, "branch", "-m", entry["branch"], "parked/" + os.path.basename(path)))' \
    "SPEC-DERIVED (point 4 negated, 'the workspace AND the branch are deleted'; constructed by frank-fable-c7 as F4, certification-frank-round6-2026-09-12.md §2.2, against which every round-6 sub-assertion of check 4 stayed green): a land would park the workspace under a name the old directory grep could not see, on a parked/ branch, and report it deleted."

# --- point 5 ---------------------------------------------------------------
mutant R-p05-turn-end-not-blocked "C5.1b" "$W" \
    '    if not blocking:{NL}        msg = "\n".join(notes)' \
    '    if True:{NL}        msg = "\n".join(notes)' \
    "RECORDED [lifecycle-failure-record-2026-09-10.md §3.1 and §2.11, 2026-09-10: notice-unlanded-branches.sh reported, de-duplicated, and went quiet while six then five finished branches sat outside main; the turn ended every time]: Rich could end his turn with finished work neither landed nor discarded."
mutant S-p05-pending-work-blocks-spawn "C5.1 " "$W" \
    '    continuation_keys = []' \
    '    if pending(sid, entity):{NL}        raise SpecError("pending work blocks new agents"){NL}    continuation_keys = []' \
    "Point 5, amended 2026-09-30: unrelated development would freeze behind finished work pending integration."

mutant S-p05-answer-allowance-unlimited "C5.3 " "$W" \
    '            if prev.get("state") == state[i["key"]]:{NL}                spent.append((i, prev))' \
    '            if False:{NL}                spent.append((i, prev))' \
    "SPEC-DERIVED (point 5's parenthesis negated, 'the reply names the pending work, which is handled right after'): naming the work again and again would end every turn."
mutant S-p05-notification-counts-as-the-ceo "C5.4 " "$W" \
    '    if d.get("queueSkipAttachments") or d.get("promptSource") == "system":{NL}        return False{AND}    if isinstance(origin, dict):{NL}        return kind == "human"' \
    '    if False:{NL}        return False{AND}    if isinstance(origin, dict):{NL}        return True' \
    "SPEC-DERIVED (point 5's first allowance negated, 'answering the CEO'): a real notification row carries three stamps (origin task-notification, promptSource system, queueSkipAttachments) and any one of them refuses it, so all three are removed here — a turn that began with a platform notification would count as answering him."
mutant S-p05-origin-kind-not-read "C5.15b" "$W" \
    '    if isinstance(origin, dict):{NL}        return kind == "human"' \
    '    if isinstance(origin, dict):{NL}        return True' \
    "SPEC-DERIVED (point 5's first allowance negated): a stamped origin that is not a person's — the constructed non-meta [SYSTEM NOTIFICATION …] row, whose only stamp is origin.kind — would count as his."
mutant S-p05-his-word-does-not-end-the-turn "C5.10" "$W" \
    '    return d.get("promptSource") in ("typed", "queued", "sdk", "suggestion_accepted")' \
    '    return d.get("promptSource") in ("typed", "queued", "suggestion_accepted")' \
    "SPEC-DERIVED (point 5's first allowance negated, 'answering the CEO or obeying his stop order'; RECORDED shape: brief-audit-frank-round8 §4.3 measured 128 of 512 of his turns carrying no origin key — the RichOS app's sdk-cli entrypoint): his stop order given through the app could not end the turn, and an origin-only rule would reject every one of those turns. (C5.15 goes red under it too.)"
mutant S-p05-unstamped-rows-count-as-the-ceo "C5.15b" "$W" \
    '    return d.get("promptSource") in ("typed", "queued", "sdk", "suggestion_accepted")' \
    '    return True' \
    "SPEC-DERIVED (point 5's first allowance negated); RECORDED shape [this machine's transcripts, census in the round-8 logs, 2026-09-13: 68 non-meta peer-message rows with no origin and no promptSource, v2.1.229–2.1.267, which the base's deny-list read as a person]: a row the platform stamped with nothing would count as his, so a peer's message or a constructed <cross-session-message> would spend his allowance."
mutant S-p05-peer-meta-row-lends-his-allowance "C5.15b" "$W" \
    '        if kind and kind != "human":{NL}            return False{NL}        return None' \
    '        return None' \
    "SPEC-DERIVED (point 5's first allowance negated): a peer's stamped meta row would be skipped like hook feedback, so the turn it started would be judged by the CEO's row before it and spend his allowance."
mutant S-p05-textual-allow-list-rejects-his-image-turn "C5.15 " "$W" \
    '    if isinstance(origin, dict):{NL}        return kind == "human"' \
    '    if isinstance(origin, dict):{NL}        return kind == "human" and text[:1].isalpha()' \
    "SPEC-DERIVED (point 5's first allowance negated, 'answering the CEO'); RECORDED shape [brief-audit-frank-round8 §4.3 and this round's census: 11 of his turns begin '[Image #N]', origin human]: a textual allow-list ('a person's text starts with a letter') would reject the CEO's own pasted-screenshot turns, and he could not end his turn by answering."
mutant S-p05-outside-reach-needs-no-todo "C5.11" "$W" \
    '    if kind in ("outside", "ceo-discard") and not (todo or "").strip():' \
    '    if False:' \
    "SPEC-DERIVED (point 5 negated, 'the latter goes on the CEO's TODO list'): an item waiting on something outside Rich's reach would be recorded with no CEO-TODO reference, and nothing would ever put it in front of him."
mutant S-p05-outside-reach-blocks-the-turn "C5.12" "$W" \
    '            "blocks_turn_end": kind not in ("ceo-discard", "started", "outside", "held"),' \
    '            "blocks_turn_end": kind not in ("ceo-discard", "started", "held"),' \
    "SPEC-DERIVED (point 5 negated, 'Rich may end his turn when every pending item is ... waiting on something outside his reach'): a recorded outside-reach wait would still block the turn."
mutant S-p05-his-word-blocks-the-turn "C5.13" "$W" \
    '            "blocks_turn_end": kind not in ("ceo-discard", "started", "outside", "held"),' \
    '            "blocks_turn_end": kind not in ("started", "outside", "held"),' \
    "SPEC-DERIVED (point 5 negated, 'that one item then waits on him, is on his TODO list'): a discard waiting on the CEO's word, asked and recorded, would still block the turn."
mutant S-p05-ceo-wait-blocks-new-work "C5.13" "$W" \
    '    continuation_keys = []' \
    '    if any(i["waiting"] == "ceo-discard" for i in pending(sid, entity)):{NL}        raise SpecError("waiting on the CEO blocks new agents"){NL}    continuation_keys = []' \
    "Point 5, amended 2026-09-30: an item waiting on the CEO would block unrelated development."

mutant S-p05-his-word-blocks-the-other-items "C5.14" "$W" \
    '        if auto and not _past(deadline):{NL}            try:{NL}                res = land(rec["key"], me, auto=True, deadline=deadline)' \
    '        if auto and not _past(deadline) and not any((r.get("waiting") or {}).get("kind") == "ceo-discard" for r in all_agents()):{NL}            try:{NL}                res = land(rec["key"], me, auto=True, deadline=deadline)' \
    "SPEC-DERIVED (point 5 negated, 'that one item then waits on him ... and blocks nothing else'): an item waiting on the CEO's word would stop every OTHER finished item from landing on its own until he answered."

# --- point 6 ---------------------------------------------------------------
mutant R-p06-native-workspace-not-registered "C6.1 " "$W" \
    '        if rec.get("isolation") == "worktree" and entity:{AND}        if native:{NL}            branch = ""' \
    '        if False and entity:{AND}        if False:{NL}            branch = ""' \
    "RECORDED [lifecycle-failure-record-2026-09-10.md §3.4, 2026-09-10: 'The sealed manifest is taken at spawn, so a workspace created later joins nothing and no terminal event ever names it. Two workspaces were unreclaimable for the life of a session']: the native workspace the platform creates after the spawn would join no record, so nothing would ever delete it."
mutant S-p06-native-branch-kept "C6.3 " "$W" \
    '            if w.get("branch_deleted_at") or not w.get("branch"):{NL}                continue{NL}            if (w.get("repo"), w["branch"]) not in out:' \
    '            if w.get("branch_deleted_at") or not w.get("branch") or w.get("kind") == "native":{NL}                continue{NL}            if (w.get("repo"), w["branch"]) not in out:' \
    "SPEC-DERIVED (point 6 negated, 'workspace and branch deleted automatically'): a landed native agent's worktree-agent-<id> branch would survive its land."

# --- point 7 ---------------------------------------------------------------
mutant R-p07-discard-records-no-reason "C7.2 " "$W" \
    '            fresh["disposition"] = {"kind": "discarded", "at": now(), "reason": reason.strip(),' \
    '            fresh["disposition"] = {"kind": "discarded", "at": now(), "reason": "",' \
    "RECORDED [lifecycle-failure-record-2026-09-12.md addendum §A2, 2026-09-12: 22 cc/ branches gone today with no deletion event and no reason anywhere in the running engine's ledger; lifecycle-failure-record-2026-09-10.md §2.2: two agents' work destroyed on an inference with nothing recorded]: a discard would record no reason."
mutant S-p07-ending-reads-neither "C7.1 " "$W" \
    '            fresh["disposition"] = {"kind": "landed", "at": now(), "auto": bool(auto), "by_session": me,' \
    '            fresh["disposition"] = {"kind": "closed", "at": now(), "auto": bool(auto), "by_session": me,' \
    "SPEC-DERIVED (point 7 negated, 'ends in exactly one of two ways — landed or discarded'): an ending would read a third word in the store."
mutant S-p07-ceo-order-discardable "C7.4 " "$W" \
    '    if ordered and not (ceo_word or "").strip():' \
    '    if False:' \
    "SPEC-DERIVED (point 7 negated, 'Work the CEO ordered is never discarded without his word'): CEO-ordered work would be discarded on a --not-ceo-ordered claim."
mutant R-p07-discard-without-any-reason "C7.6 " "$W" \
    '    if len((reason or "").strip()) < 10:{NL}        raise SpecError("a discard records its reason (point 7): give one")' \
    '    if False:{NL}        raise SpecError("a discard records its reason (point 7): give one")' \
    "RECORDED [lifecycle-failure-record-2026-09-12.md addendum §A2, 2026-09-12: 22 cc/ branches deleted with no reason recorded anywhere; certification-frank-round6-2026-09-12.md §3 F1: round 6's C7.2 asserted the reason the harness gave was stored, never that one is required]: a discard giving NO reason would be accepted."
mutant S-p07-attestation-not-required "C7.7 " "$W" \
    '    if not ordered and not (ceo_word or "").strip() and len((not_ceo_ordered or "").strip()) < 10:' \
    '    if False:' \
    "SPEC-DERIVED (point 7 negated, 'never discarded without his word' — its mechanism for work the prompt did not mark; constructed by frank-fable-c7 as F2): a discard saying neither --ceo-word nor --not-ceo-ordered would be accepted."
mutant S-p07-continuing-start-deletes-nothing "C7.8 " "$W" \
    '    for old_key in rec.get("continues") or []:{NL}        old = load_agent(old_key)' \
    '    for old_key in []:{NL}        old = load_agent(old_key)' \
    "SPEC-DERIVED (point 7 negated, 'the old workspaces are deleted when the new agent starts'; constructed by frank-fable-b2 as F30): a continuing agent's start would delete nothing of the agent it continues."
mutant S-p07-continued-work-not-landed-with-the-new "C7.9 " "$W" \
    '        for k in r.get("continues") or []:{NL}            o = load_agent(k)' \
    '        for k in []:{NL}            o = load_agent(k)' \
    "SPEC-DERIVED (point 7 negated, 'its work counts as landed when the new agent's does'): the new agent's land would not walk its chain, so the old agent's branch would be neither proved landed nor deleted, and its ending would never read landed."

# --- point 8 ---------------------------------------------------------------
mutant R-p08-ignored-needed-files-landed "C8.3 " "$W" \
    '            if ignored and not ignored_ok:' \
    '            if False:' \
    "RECORDED [lifecycle-failure-record-2026-09-10.md §3b.2, 2026-09-10: an ignored nested repository under a 'disposable' path deleted with no copy taken; scripts/inflight-ack.sh header, 2026-09-05: echo-opus-529's three gitignored acks deleted with its unchanged worktree]: a workspace with ignored files it needs would be landed and the files lost."
mutant S-p08-same-name-is-same-file "C8.8 " "$W" \
    'def _same_file(a, b, deadline=None):{NL}    """Byte-identical?' \
    'def _same_file(a, b, deadline=None):{NL}    return True{NL}    """Byte-identical?' \
    "SPEC-DERIVED (point 8 negated, 'Deletion therefore never loses anything that was meant to land'; constructed by frank-fable-b3 as F-A, brief-audit-frank-round8 §5, which survived the round-7 fourteen): an ignored file the main checkout has under the same NAME with different bytes would be deleted by a land that reports success."
mutant S-p08-same-size-is-same-file "C8.8 " "$W" \
    '        return _file_digest(a, deadline) == _file_digest(b, deadline)' \
    '        return True' \
    "SPEC-DERIVED (point 8 negated; constructed by frank-fable-b3 as F-H, brief-audit-frank-round8 §5, and the reason C8.8's two files are the same SIZE): same size would mean identical, so a rotated key of equal length would be deleted by a land that reports success."
mutant S-p08-uncommitted-landed "C8.1 " "$W" \
    '            if dirty:{NL}                problems.append("%s has %d uncommitted entr%s (%s)" % (' \
    '            if False:{NL}                problems.append("%s has %d uncommitted entr%s (%s)" % (' \
    "SPEC-DERIVED (point 8 negated, 'Nothing uncommitted is ever landed'): an uncommitted file would not hold the land."
mutant R-p08-ignored-directory-skipped-by-name "C8.6 " "$W" \
    '                for sub in _ignored_dir_diff(mine, other, rel.rstrip("/"), deadline):{NL}                    ignored.append(sub){NL}                continue' \
    '                continue' \
    "RECORDED [lifecycle-failure-record-2026-09-10.md §3b.2, 2026-09-10: an ignored nested repository under a folder the policy treated as disposable deleted with no copy taken; certification-frank-round6-2026-09-12.md §2.1, 2026-09-12: workspaces.py:1577–1578 skipped an ignored directory the main checkout also had by NAME, so .claude/notes/needed.txt and a nested repository with unlanded commits were neither refused nor preserved]: the round-6 code, restored — the land proceeds and deletes them."

# --- point 9 ---------------------------------------------------------------
mutant R-p09-finished-agent-not-locked-out "C9.1 " "$W" \
    '    if fin:{NL}        return "FINISHED", "agent %s (%s) is finished: %s" % (aid, rec.get("name"), why)' \
    '    if False:{NL}        return "FINISHED", "agent %s (%s) is finished: %s" % (aid, rec.get("name"), why)' \
    "RECORDED [lifecycle-failure-record-2026-09-10.md §3b.1 and §3b.5, 2026-09-10: thirteen agents restarted after their terminal record, 0.3 s to 8 h later, three deliverables lost to it; lifecycle-failure-record-2026-09-11.md §2 S4: the fourteenth]: a restarted finished agent would be given its tools back."
mutant S-p09-sigkill-escalation-removed "C9.6 " "$W" \
    '    for p in alive:{NL}        try:{NL}            os.kill(p, signal.SIGKILL)' \
    '    for p in []:{NL}        try:{NL}            os.kill(p, signal.SIGKILL)' \
    "SPEC-DERIVED (point 9 negated, 'every process it started is stopped before its workspaces are deleted'; constructed by frank-fable-b3 as F-G, brief-audit-frank-round8 §5, which survived the round-7 fourteen because C9's holder dies on TERM): a process that ignores SIGTERM would outlive the deletion of its workspace."
mutant R-p09-processes-not-stopped "C9.3 " "$W" \
    'def stop_processes(paths, deadline=None):' \
    'def stop_processes(paths, deadline=None):{NL}    return {"stopped": [], "survivors": []}' \
    "RECORDED [femcboost CLAUDE.md, Git Worktree Isolation, 'Corollary (zombie residue, 2026-07-18)': a background child outlived both its agent and its worktree and re-created the path; brief-audit-frank-round6 P11, 2026-09-12: VM pid 1483 holding 324 files inside a finished agent's trees five days later]: a process the agent started would outlive the deletion of its workspace."

# --- point 10 --------------------------------------------------------------
mutant R-p10-cc-workspace-left-behind "C10.2 " "$W" \
    '        for w in workspaces:{NL}            # Point 3: the branches the agent created are its branches too, and' \
    '        for w in [w for w in workspaces if w.get("kind") != "cc"]:{NL}            # Point 3: the branches the agent created are its branches too, and' \
    "RECORDED [lifecycle-failure-record-2026-09-12.md addendum §A4, 2026-09-12: for sage-opus-c6 and frank-opus-c6 the native workspaces and branches are gone and the cc/ halves remain as quarantines; lifecycle-failure-record-2026-09-10.md §3.4: zero of 15,882 finish rows ever named a cross-repository workspace]: a land would delete the native workspace and leave the cc/ one."
mutant S-p10-cc-branch-kept "C10.3 " "$W" \
    '            if w.get("branch_deleted_at") or not w.get("branch"):{NL}                continue{NL}            if (w.get("repo"), w["branch"]) not in out:' \
    '            if w.get("branch_deleted_at") or not w.get("branch") or w.get("kind") == "cc":{NL}                continue{NL}            if (w.get("repo"), w["branch"]) not in out:' \
    "SPEC-DERIVED (point 10 negated, 'every workspace and branch it has is deleted, as one'): the cc/ branch of a two-workspace agent would survive the land."
mutant R-p10-only-the-first-workspace-deleted "C10.10" "$W" \
    '        if not _delete(r, [w for w in live_workspaces(r) if w.get("path")], branches=True, why=why,' \
    '        if not _delete(r, [w for w in live_workspaces(r) if w.get("path")][:1], branches=True, why=why,' \
    "RECORDED [the workspace registry's own events.jsonl on this machine, 2026-09-17: four teammates (norm-opus-wire1/wireguide1, norm-opus-secret1/secret1hq, norm-opus-multiacct1, norm-opus-ms1) needed a workspace in a SECOND repository and were given a second NAME, because spawn.sh took one --repo; norm-opus-wireguide1's workspace and its branch cc/norm-opus-wireguide1 were left behind by a land that named only norm-opus-wire1, and the CEO found the stray himself]: a land would delete the FIRST of an agent's workspaces and leave the rest — the exact shape point 10 forbids, now that one name can hold workspaces in several repositories."

mutant R-p03-ref-after-the-last-post-left-behind "C10.7 " "$W" \
    '    if all_open and latest and isinstance(latest.get("repos"), dict):{NL}        priors.append(latest)' \
    '    if False:{NL}        priors.append(latest)' \
    "RECORDED [certification-frank-round7-2026-09-12.md §4 and certification-frank-recorded-attribution-2026-09-12-probe.py cases outside-stray / outside-side, 2026-09-12: a Bash call returned 3 s before its process finished, and a ref that process created after the call's PostToolUse was attributed to nobody, the land reported success and the ref was left behind; round7-fixes-2026-09-12.md §6 and round8's base run: RED on the real manifest; escalation esc-20260912T225456Z-d34bf4e6]: the end-of-run signal would observe only windows still open, so a ref created after the last PostToolUse would be compared against nothing and left behind (points 3, 9, 10)."
mutant R-p03-backgrounded-call-window-consumed-at-its-post "C10.7b" "$W" \
    '    background = str(payload.get("tool_name") or "") == "Bash" and bool(ti.get("run_in_background"))' \
    '    background = False' \
    "RECORDED [certification-frank-recorded-attribution-2026-09-12-probe.py cases outside-stray / outside-side, 2026-09-12 — a Bash call issued with run_in_background returned 3 s before its process finished; RED on the real manifest through round 7 and round 8's base (round7-fixes §6; this round's runner-base log); escalation esc-20260912T225456Z-d34bf4e6]: the platform's run_in_background stamp would be ignored, the backgrounded call's window consumed at its Post, and the ref its process then creates would be judged by nobody — left behind by a land that reports success (points 3, 9, 10)."
mutant S-p03-background-window-unioned-with-the-next-snapshot "C10.7b" "$W" \
    '        added += _attribute_new_refs(rec, bg_priors, None)' \
    '        added += _attribute_new_refs(rec, bg_priors, latest)' \
    "SPEC-DERIVED (point 3 negated, 'any branch an agent created'): a background window would be unioned with the next call's snapshot, which was taken while the backgrounded process was still running and already holds what it created — so the ref would read as old and be attributed to nobody."
mutant S-p03-every-call-treated-as-backgrounded "C10.7d" "$W" \
    '    background = str(payload.get("tool_name") or "") == "Bash" and bool(ti.get("run_in_background"))' \
    '    background = str(payload.get("tool_name") or "") == "Bash"' \
    "SPEC-DERIVED (point 8 negated, 'Deletion therefore never loses anything that was meant to land'; the shape of the engine's own test_point_08_a_branch_that_existed_before_the_call_is_never_created_in_it and of certification-frank c4-control case leaked-window-widens): every foreground call's window would stay open too, so a ref Rich cuts at the agent's tip between two ordinary calls would be attributed to the agent and deleted by its discard."
mutant R-p10-created-branch-left-behind "C10.6 " "$W" \
    '        for pair in r.get("created_branches") or []:{NL}            t = (pair[0], pair[1])' \
    '        for pair in []:{NL}            t = (pair[0], pair[1])' \
    "RECORDED [certification-frank-attribution-2026-09-12.md §3 B1, 2026-09-12: 'git branch spare-work' inside the workspace, never checked out, then a land — left behind, and the pending list was []; certification-frank-round6-2026-09-12.md §3 F19: no agent in the round-6 fourteen created a side branch]: a branch the agent is recorded as having created would be neither proved landed nor deleted with its work."

# --- point 11 --------------------------------------------------------------
mutant R-p11-sub-run-end-finishes-the-teammate "C11.1 " "$W" \
    '        rec = _record_for_agent(session_id, agent_id){NL}        if not rec:{NL}            return None{NL}        if rec.get("disposition"):{NL}            rec.setdefault("history", []).append({"at": iso(), "fact": "end after disposition", "signal": signal_name})' \
    '        rec = _record_for_agent(session_id, agent_id) or next((r for r in all_agents() if r.get("session_id") == session_id and r.get("agent_id") and not r.get("end") and not r.get("disposition")), None){NL}        if not rec:{NL}            return None{NL}        if rec.get("disposition"):{NL}            rec.setdefault("history", []).append({"at": iso(), "fact": "end after disposition", "signal": signal_name})' \
    "RECORDED [lifecycle-failure-record-2026-09-10.md §3.5, 2026-09-10: the finish event's agent_id is a per-run identifier, not the owning agent, and the code derived the owner from the folder instead; lifecycle-failure-record-2026-09-12.md addendum §A3: 974 finished rows for 25 teammates today, 973 distinct ids, 0 carrying a branch]: a sub-run's end would be recorded as the teammate's own and finish it mid-work."
mutant S-p11-pause-ignored "C11.3 " "$W" \
    '        pause = rec.get("pause"){NL}        if pause and pause.get("at", 0) <= end.get("at", 0):' \
    '        pause = rec.get("pause"){NL}        if False:' \
    "SPEC-DERIVED (point 11 negated, 'An agent Rich pauses ... is not finished'): a paused agent whose run ended would be finished and locked out."
mutant S-p11-nameless-pause-not-pending "C11.5 " "$W" \
    '            if paused_ and rec.get("session_id") == me and not (rec.get("pause") or {}).get("until"):' \
    '            if False:' \
    "SPEC-DERIVED (point 11 negated, 'a pause with nothing named counts as pending work under point 5'): a pause naming nothing would block nothing and stay paused forever."
mutant S-p11-handed-in-finishes-before-the-run-ends "C11.8 " "$W" \
    '    end = rec.get("end"){NL}    if end:{NL}        if end.get("signal") == "stopped":' \
    '    if rec.get("handed_in"):{NL}        return True, False, "handed in"{NL}    end = rec.get("end"){NL}    if end:{NL}        if end.get("signal") == "stopped":' \
    "SPEC-DERIVED (point 11 negated, \"'Finished' means the agent's run has ended\"; constructed by frank-fable-b3 as F-F, brief-audit-frank-round8 §5, which survived the round-7 fourteen because C11.7 handed in and ended back to back): an agent that handed in its work would be locked out and landable while its run is still going."
mutant S-p11-handed-in-then-ended-not-finished "C11.7 " "$W" \
    '        if rec.get("handed_in"):{NL}            return True, False, "it ended after handing in its work (point 11)"' \
    '        if False:{NL}            return True, False, "it ended after handing in its work (point 11)"' \
    "SPEC-DERIVED (point 11 negated, 'An agent that ends after handing in its work is finished even if a pause was sent'; constructed by frank-fable-c7 as F26): an agent that handed in, was paused, and then ended would read paused, keep its tools and never be pending."

# --- point 12 --------------------------------------------------------------
mutant R-p12-ended-session-agents-not-finished "C12.4 " "$W" \
    '        if st == "ended":{NL}            return True, False, "its session has ended: %s (point 12)" % why' \
    '        if False:{NL}            return True, False, "its session has ended: %s (point 12)" % why' \
    "RECORDED [brief-audit-sage-round6-2026-09-12.md §4.3, 2026-09-12: 26 of the running engine's 49 holds belong to sessions b7869424 and d0eef867, both ended, and are still held; lifecycle-failure-record-2026-09-10.md §2.5: pgrep cannot see subagents at all]: an ended session's agents would never count as finished."
mutant S-p12-a-pid-alone-is-the-session "C12.2 " "$W" \
    '        elif st == "ok" and text != ident["pid_start"]:' \
    '        elif False:' \
    "SPEC-DERIVED (point 12 negated, 'The record carries more than a process number, since those get reused'): a reused process number would be taken for the session."
# Its want was "C12.5 " until 2026-10-01. The amendment of 2026-09-30 (b46a1113, "Allow
# independent spawns while finished work awaits integration") turned C12.5 from "new work is
# refused until the previous session's agent is handled" into "independent work starts while the
# previous session's workspace is preserved", which this mutant cannot turn red: with no takeover
# the spawn still succeeds and the workspace is still there. Nightly attempt 6 (20261001T022754Z)
# measured the mutant red exactly where the property lives: C12.4 (the next session is told of
# the ended session's agent by name) and C12.10 (the running session takes over a session that
# recorded its end). C12.4 is the first of them, so it is the want.
mutant S-p12-next-session-does-not-take-over "C12.4 " "$W" \
    '        st, _ = session_state(owner, rec.get("session_identity"), cache){NL}        if st != "ended":{NL}            return False' \
    '        st, _ = session_state(owner, rec.get("session_identity"), cache){NL}        if True:{NL}            return False' \
    "SPEC-DERIVED (point 12 negated, 'the next session reconciles their pending work first while independent development may continue'): the next session would never claim an ended session's agents, so it would not be told of them and would never land or discard them. (C12.10 goes red under it too.)"
mutant S-p12-live-sessions-agents-claimed "C12.7 " "$W" \
    '    if owner and not rec.get("orphan"):' \
    '    if False:' \
    "SPEC-DERIVED (point 12 negated, 'While two sessions run at once, each handles only the agents it started'; constructed by frank-fable-c7 as F10): a running session would claim, list and try to land another RUNNING session's finished agents."
mutant S-p12-recorded-end-ignored "C12.9 " "$W" \
    '    if rec and rec.get("ended_at"):' \
    '    if False:' \
    "SPEC-DERIVED (point 12 negated, 'A session has ended when it recorded its end'; constructed by frank-fable-b2): a session that recorded its end would count as running for as long as its process number stayed alive, and its agents would never be finished."

# --- point 13 --------------------------------------------------------------
mutant R-p13-failed-deletion-not-retried "C13.3 " "$W" \
    '        if not d or d.get("next_at", 0) > now():{NL}            continue' \
    '        if True:{NL}            continue' \
    "RECORDED [lifecycle-failure-record-2026-09-10.md §2.17, 2026-09-10: 'It was a virtual machine holding files open, and the prefix was irrelevant'; brief-audit-frank-round6 P5, 2026-09-12: four trees held by VM pid 1483, called RETRY by the running engine and never retried to completion]: a failed deletion would never be retried."
mutant S-p13-ceo-told-at-the-first-failure "C13.2 " "$W" \
    'RETRY_TELL_CEO_AFTER = int(os.environ.get("RICHOS_WORKSPACES_RETRY_TELL_CEO", "5"))' \
    'RETRY_TELL_CEO_AFTER = int(os.environ.get("RICHOS_WORKSPACES_RETRY_TELL_CEO", "1"))' \
    "SPEC-DERIVED (point 13 negated, 'The CEO hears about it only if it keeps failing'): the CEO would be told at the first failed attempt."

# --- point 14 --------------------------------------------------------------
mutant R-p14-spawn-not-refused-without-a-record "C14.1 " "$W" \
    '    missing = _unrecorded_repos(repos){NL}    if missing:' \
    '    missing = _unrecorded_repos(repos){NL}    if False:' \
    "RECORDED [certification-sage-round6-2026-09-12.md §2 S1–S4 and certification-frank-round6-2026-09-12.md §5, 2026-09-12: an agent spawned with no record was bound to nothing and proved at land time against whichever body of work was current — a second, unrelated recording moved its land verdict while a properly bound control agent was unmoved]: the spawn would proceed with nothing recorded, and 'before its first agent is spawned' would be a habit."
mutant S-p14-cc-branch-as-integration-target "C14.8 " "$W" \
    '    if branch.startswith(CC_PREFIX) or branch.startswith(NATIVE_BRANCH_PREFIX):' \
    '    if False:' \
    "SPEC-DERIVED (point 14 negated, 'Finished work never waits on the agent's own branch'): an agent's own cc/ or worktree-agent- branch could be recorded as the branch its work integrates on, and its land would be proved against itself."
mutant S-p14-agent-may-record-the-branch "C14.9 " "$G" \
    'if AGENT and INTEGRATION_CALL.search(scan):' \
    'if False:' \
    "SPEC-DERIVED (point 14 negated, 'RECORDED when that work starts, before its first agent is spawned' — by Rich, who starts it; both round-6 reviewers: certification-sage-round6 §5 A3, certification-frank-round6 §4 RN2b): an agent's own Bash call could record or correct the integration branch, retargeting every in-flight agent's land and the runner's witness."
# The three below were re-aimed in round 8: items 2 and 3 folded the point-14 verb
# rules into one protected-ref rule, and the lines they named no longer exist.
mutant S-p14-agent-may-move-the-recorded-branch "C14.11" "$G" \
    '                elif renames or forces or (kind == "codex" and len(positional) >= 1):{NL}                    _refuse_write(kind, "git branch", t)' \
    '                elif False:{NL}                    _refuse_write(kind, "git branch", t)' \
    "SPEC-DERIVED (point 14 negated; certification-frank-round6 §4 RN2c, 'git branch -f dev/workspace-spec HEAD' → 0): an agent's call could force-move the recorded integration branch with git branch -f."
mutant S-p14-agent-may-update-ref-the-recorded-branch "C14.11" "$G" \
    '            if deletes:{NL}                _refuse_delete(kind, "git update-ref -d", t){NL}            else:{NL}                _refuse_write(kind, "git update-ref", t)' \
    '            if deletes:{NL}                _refuse_delete(kind, "git update-ref -d", t){NL}            else:{NL}                pass' \
    "SPEC-DERIVED (point 14 negated; certification-frank-round6 §4: 'git update-ref refs/heads/dev/workspace-spec HEAD' → 0): an agent's call could rewrite the recorded integration branch through update-ref."
mutant S-p14-agent-may-push-into-the-recorded-branch "C14.11" "$G" \
    '            if pdelete or src == "":{NL}                _refuse_delete(kind, "git %s" % sub, dst){NL}            else:{NL}                _refuse_write(kind, "git %s into" % sub, dst)' \
    '            if pdelete or src == "":{NL}                _refuse_delete(kind, "git %s" % sub, dst){NL}            else:{NL}                pass' \
    "SPEC-DERIVED (point 14 negated; certification-frank-round6 §4: 'git push . HEAD:dev/workspace-spec' → 0): an agent's call could push its own tip into the recorded integration branch."
mutant S-p14-recorded-branch-not-reported "C14.13" "$W" \
    '    _restore_protected_refs(rec, priors + [p for p in bg_priors if not p.get("judged")], latest)' \
    '    pass' \
    "SPEC-DERIVED (point 14 negated, 'The branch a body of work integrates on is RECORDED ... Nothing infers it and nothing guesses it' — and both round-8 reviewers measured that no verb list closes the doorway: checkout <it>, then commit/reset/merge/rebase move it naming nothing, from either checkout): a recorded branch moved by an unnamed verb in an agent's call would go unseen — nobody would be told, and every in-flight agent's land would be measured against a tip an agent chose with no record that it happened."
mutant S-p14-a-move-is-written-back-again "C14.13" "$W" \
    '                if cur is not None:{AND}    return git(repo, "update-ref", "-m", msg, "--no-deref", "refs/heads/" + branch, tip, "")' \
    '                if False:{AND}    return git(repo, "update-ref", "--no-deref", "refs/heads/" + branch, tip)' \
    "RECORDED [docs/verification/ref-write-forensics-2026-09-14.md: on 2026-09-13/14 this line moved refs/heads/main in /Users/alex/ab/richos three times, twice within twelve seconds in opposite directions while Rich was landing, each with an EMPTY reflog message because it passed no -m; reproduced from nothing at docs/verification/protected-ref-oscillation-2026-09-14-logs/repro.py]: a MOVED protected ref would be written back instead of reported, unconditionally and unattributed, to whichever tip one agent's oldest open window happened to hold."
mutant S-p14-the-leads-move-reported-too "C14.15" "$W" \
    '                    else:{NL}                        landed = land_by_another_conversation(repo, b, old, cur){NL}                        if not landed:{NL}                            continue                    # a descendant carrying none of the agent'"'"'s work: the lead'"'"'s land{NL}                        action, why = "LANDED", landed  # except when the land record names another conversation' \
    '                    else:{NL}                        why = "moved"' \
    "SPEC-DERIVED (point 14 negated, 'Rich merges each finished agent's work onto it' — landing is his): the lead's own land onto the recorded branch during an agent's call would be reported as an agent's move. It is no longer UNDONE — nothing here writes over a move — but a check that fires on every ordinary land is alarm fatigue, and being rare is the whole of its remaining value."
mutant S-p14-end-of-run-reports-the-leads-land "C14.15b" "$W" \
    '                elif b not in windowed:' \
    '                elif False:' \
    "SPEC-DERIVED (point 14 negated, 'Rich merges each finished agent's work onto it'; RECORDED shape: certification-sage-runner-round case R8 went RED under this round's first restore rule on 2026-09-13 — the lead's fast-forward of the agent's own branch, made after its last call and before its end signal, was undone at the end signal and the land refused): the own-work rule would apply with no call open, so his land would be reported as the agent's doing at its end signal."
mutant S-p14-checkout-doorway-open "C14.16" "$G" \
    '                for t in positional[:1]:        # the branch being checked out' \
    '                for t in []:' \
    "SPEC-DERIVED (point 14 negated; brief-audit-frank-round8 §2 and brief-audit-sage-round8 §4, both measured on the base: an agent's plain checkout/switch of the recorded branch passed, after which commit, reset --hard, merge, rebase and --amend moved it unnamed, from the agent's worktree or the main checkout): the doorway would be open again."
mutant S-p14-named-movers-pass-the-guard "C14.16" "$G" \
    '    elif kind == "recorded":{NL}        spec.append("%s writing %s, the RECORDED integration branch' \
    '    elif False:{NL}        spec.append("%s writing %s, the RECORDED integration branch' \
    "SPEC-DERIVED (point 14 negated; brief-audit-frank-round8 §2, executed: branch -C, push HEAD:heads/<it>, fetch, pull, checkout -B, switch -C, symbolic-ref and send-pack each moved the recorded branch at rc=0 on the base): every verb that names the recorded branch would pass the guard from an agent's call again."
mutant R-p14-land-assumes-main "C14.3 " "$W" \
    '    branch = (work or {}).get("branch") or ""{NL}    if not branch:' \
    '    branch = "main"{NL}    if not branch:' \
    "RECORDED [lifecycle-failure-record-2026-09-12.md §2c and brief-audit-sage-round6 §4.6, 2026-09-12: a true land onto dev/workspace-spec was refused by a check that asked main; 19 of the running engine's holds read 'NOT on refs/heads/main' for work that is on dev/workspace-spec]: a land would be proved against main whatever branch was recorded."
mutant S-p14-consumer-keeps-its-own-answer "C14.6 " "$Q" \
    '    if why_not or not branch:{NL}        return (){NL}    remote = "origin/" + branch' \
    '    if why_not or not branch:{NL}        return INTEGRATION_REFS{NL}    remote = "origin/" + branch' \
    "SPEC-DERIVED (point 14 negated, 'None of them is allowed its own answer, and none of them assumes main'): guard-unresolved-claims.py would fall back to main where nothing is recorded."
mutant S-p14-unlanded-sweep-assumes-main "C14.5 " "$U" \
    '    if why_not or not branch:{NL}        return None{NL}    return branch' \
    '    if why_not or not branch:{NL}        return "main"{NL}    return branch' \
    "SPEC-DERIVED (point 14 negated): unlanded-branches.py would measure against main where nothing is recorded instead of abstaining."

mutation_end
