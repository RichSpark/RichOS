#!/usr/bin/env bash
#
# app.mutation.sh — PROVES THE LAND LOCK'S TESTS CAN FAIL, one property at a
# time. Invoked by app.test.sh; the loop is scripts/lib/mutation-harness.sh.
#
# WHY THIS HARNESS EXISTS AT ALL. Before 2026-09-17 two conversation threads
# each took their own lock file and both landed into one repository at the same
# moment; `mega-lander/tests/app.test.py` was GREEN throughout, because it ran
# one thread and one thread never contends with itself. A suite that is green
# with the property present and green with it removed measures nothing, and that
# is exactly the state the land lock's tests would be in without this file.
#
# Every mutant below removes ONE property of the lock in a throwaway copy of the
# engine and names the test that must go red for it.
#
# Run directly: mega-lander/tests/app.mutation.sh
# Exit 0 = every property is proven load-bearing.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0"), a mutation pass (RICHOS_MUTATION_PASSES=0, the merge gate; the nightly runs it)"; exit 0; fi

set -uo pipefail
[ -n "${RICHOS_MUTATION_INNER:-}" ] && exit 0
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../../scripts/lib/mutation-harness.sh
. "$SCRIPT_DIR/../../scripts/lib/mutation-harness.sh"

mutation_begin "the per-repository land lock (app.py integrate)" "mega-lander/tests/app.test.sh"
# Part 4 (2026-09-29 hunt), finding 20. app.test.py takes case names (a bare name
# selects the methods it begins), and every case builds its own fixture in setUp, so
# a mutant's verdict, one named case going red, needs only that case. The unmutated
# control still runs first and must print `PASS  <want>`. See mutation_focus in
# scripts/lib/mutation-harness.sh.
mutation_focus want-as-argument

A="mega-lander/app.py"
# THE LOCK'S PATH IS DERIVED IN workspaces.py SINCE 2026-09-17, and so the
# mutants for its keying live against that file. The move was forced by the
# second half of the lock: `_restore_protected_refs` in workspaces.py has to
# name the conversation whose land moved a protected ref, and workspaces.py
# cannot import app.py. The alternative was the keying rule in two files, which
# is the before-state itself — two conversations taking two lock files while
# believing they share one.
WS="mega-lander/workspaces.py"

# --- 1. THE LAND IS UNDER THE LOCK, OR IT IS NOT LOCKED AT ALL -------------
# The before-state itself: `integrate` reaches `git merge --ff-only` holding
# only per-conversation locks, so two threads merge into one repository at once.
# Since part 4, finding 2 the land takes EVERY repository's lock (land_locks);
# one repository is exactly land_lock alone, so removing the call removes it.
mutant land-not-locked "test_two_conversations_landing_in_one_repository_take_one_lock_and_the_merge_is_under_it" "$A" \
    '    with land_locks(scope,repositories_to_land) as lands:' \
    '    with contextlib.nullcontext({r:{} for r in repositories_to_land}) as lands:' \
    "the fast-forward would run with no lock on the repository at all -- the exact defect, restored: two threads' merges race and the loser's finished, reviewed work is re-implemented."

# --- 2. THE LOCK IS KEYED TO THE REPOSITORY -------------------------------
# A lock keyed to nothing is machine-wide, which is "correct" and makes every
# unrelated pair of conversations queue behind each other for no reason.
mutant lock-not-keyed-to-the-repository "test_one_repository_reached_two_ways_is_one_lock_and_two_repositories_are_two" "$WS" \
    '    canonical = canonical_repository(repo){NL}    name = os.path.basename(canonical.rstrip("/"))' \
    '    canonical = "every repository at once"{NL}    name = os.path.basename(canonical.rstrip("/"))' \
    "one lock would cover every repository on the machine, so two conversations landing into two different repositories -- the case the CEO says he will actually run -- would wait on each other."

# --- 3. ONE REPOSITORY REACHED TWO WAYS IS ONE LOCK -----------------------
# Without the canonicalization the key is the string it was reached by: a
# symlinked connection, or a linked worktree, gets its own lock file and the two
# of them move the same branch at the same time.
mutant canonical-path-not-resolved "test_one_repository_reached_two_ways_is_one_lock_and_two_repositories_are_two" "$WS" \
    '    return os.path.realpath(os.path.expanduser(common or str(repo)))' \
    '    return str(repo)' \
    "two paths to one repository -- a symlink, or a linked worktree sharing the ref store -- would take two different locks and race on the same branch."

# --- 4. THE LOCK LIVES OUTSIDE BOTH CONVERSATION PARTITIONS ---------------
# The whole defect wearing a new name: a lock file under a per-thread directory
# is a lock two threads cannot share.
mutant partition-home-accepted "test_the_land_lock_refuses_to_live_inside_either_conversation_partition" "$WS" \
    '        if base == partition or base.startswith(partition.rstrip(os.sep) + os.sep):' \
    '        if False:' \
    "the land lock could be pointed inside \$RICHOS_WORKSPACES_DIR or the app state root, where each thread has its own copy -- two locks again, and nothing would say so."

# --- 5. THE SECOND LANDER WAITS; IT NEVER REFUSES THE WORK ---------------
# The bound is a backstop for a stuck holder. If it is the FIRST answer instead,
# every ordinary overlap refuses reviewed work and costs it an implementation
# and a review to recover.
mutant contention-refuses-instead-of-waiting "test_the_second_lander_waits_for_the_first_and_never_refuses_the_work" "$A" \
    '            waited = time.monotonic() - started{NL}            if waited >= timeout:' \
    '            waited = time.monotonic() - started{NL}            if True:' \
    "a land that merely overlapped another would be refused instead of queued, and a refused land sends finished, reviewed work back for a fresh implementation and a fresh review."

# --- 6. AND THE REFUSAL NAMES THE HOLDER ---------------------------------
# The matched control for mutant 5. A refusal is allowed, at the bound; a
# refusal that cannot say who is holding the repository is an unactionable one,
# and the operator's only next move would be to guess.
mutant refusal-does-not-name-the-holder "test_a_holder_past_the_bound_is_refused_by_name_with_nothing_merged" "$A" \
    '    if not holder:{NL}        return "another conversation, which left no readable record in the lock file"' \
    '    if True:{NL}        return "another conversation, which left no readable record in the lock file"' \
    "the refusal at the bound would say only that somebody was landing, never which conversation -- and the one thing the reader needs is which other thread to go and look at."

# --- 7. A WAIT IS REPORTED, NOT SILENT -----------------------------------
# "The second lander WAITS, in order, AND SAYS SO." A silent wait looks exactly
# like a slow land, and a land nobody can see queueing is a land nobody believes
# is serialized.
mutant wait-not-reported "test_the_second_lander_waits_for_the_first_and_never_refuses_the_work" "$A" \
    '        if holder is not None:{NL}            status["waited_for"] = _land_holder_text(holder)' \
    '        if False:{NL}            status["waited_for"] = _land_holder_text(holder)' \
    "a land that queued behind another conversation for minutes would report nothing about it, so the serialization would be invisible to the only person who could notice it was wrong."

# --- 8. THE LAND IS RECORDED, OR NOBODY CAN BE TOLD WHO LANDED -----------
# The second half of the lock (2026-09-17). Serializing the two lands is not the
# whole answer: the thread that waited then moved the branch UNDER the other
# one's running agents, and workspaces.py can only name the conversation that
# did it if the land wrote a record.
mutant land-not-recorded "test_the_land_record_is_appended_beside_the_lock_and_names_the_conversation" "$A" \
    '                        W.append_land_record(repo,lands[repo]["landed"])' \
    '                        pass' \
    "a land would move the recorded branch and leave nothing saying which conversation moved it, so the other thread's agents would be told only that their base changed -- or, where no record exists at all, nothing. Attribution cannot be recovered afterwards: the reflog names a ref write, never a conversation."

# --- 9. AND IT IS APPENDED, NEVER OVERWRITTEN ---------------------------
# The exact shape the first implementation had: a `landed` field on the lock
# file, rewritten in place by the next holder.
mutant land-record-overwritten-not-appended "test_the_land_record_is_appended_beside_the_lock_and_names_the_conversation" "$WS" \
    '        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND | os.O_NOFOLLOW, 0o600)' \
    '        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600)' \
    "each land would erase the one before it, so of two lands in a row only the second could ever be named -- and an agent whose snapshot predates the first would be told its base moved by nobody. That is the defect this record was moved off the lock file to fix."

# --- 10. IT IS WRITTEN WHERE THE REF MOVES, NOT WHERE THE CALL ENDS -----
# `integrate` is idempotent and is re-run after a partial cleanup. A record
# written at the end of the call rather than at the merge would count those
# repeats as lands that never happened.
mutant land-recorded-without-a-merge "test_the_land_record_is_appended_beside_the_lock_and_names_the_conversation" "$A" \
    '                    git(repo,"merge-base","--is-ancestor",commit,"refs/heads/"+branch)' \
    '                    W.append_land_record(repo,{"schema":1,"branch":branch,"before":tip,"commit":commit,"at":W.iso(),"thread_id":"x","entity_id":"y","session_id":"z","pid":0}){NL}                    git(repo,"merge-base","--is-ancestor",commit,"refs/heads/"+branch)' \
    "a repeated integrate -- the recovery path, which merges nothing -- would record a land it did not perform, and the history would then answer for moves that never happened."

# --- 11. THE WAITER READS WHAT THE HOLDER DID FROM THE HISTORY ----------
# The matched control for 8 and 9 on the reading side: the record exists to be
# read, and the one reader in this file is the next lander.
mutant waited-for-land-not-read "test_the_second_lander_waits_for_the_first_and_never_refuses_the_work" "$A" \
    '            if previous and previous[0].get("thread_id") == holder.get("thread_id"):' \
    '            if False:' \
    "the lander that waited would report that it waited and never what it waited THROUGH, although the holder's own land is sitting in the history beside the lock it just released."

# --- 12. HIS OWN THREAD SEATS ARE RECONCILED AT ALL ---------------------
# The before-state, and it is a one-line filter: `reconcile_seats` enumerated
# every seat and skipped everything whose audience was not `worker`, so one of
# his per-thread seats was walked past on every pass, forever.
mutant his-thread-seat-skipped "test_his_orphan_thread_seat_is_reconciled_and_a_live_threads_never_is" "$A" \
    '        his = ECS.is_ceo_row(row) and row["person_id"] != ECS.PERSON_ID' \
    '        his = False' \
    "his per-thread seats would be enumerated and then skipped again, so a seat for a conversation thread that no longer exists would live forever -- with nothing reporting it, because a skip is silent."

# --- 13. A SEAT IS NEVER RELEASED ON AN ABSENCE -------------------------
# Whether a thread exists is the app's knowledge. "The app's record could not be
# read" and "the app has no such thread" are two different answers, and only one
# of them is a fact about the thread.
mutant unreadable-ledger-treated-as-no-threads "test_his_orphan_thread_seat_is_reconciled_and_a_live_threads_never_is" "$A" \
    '        if not path.is_file():{NL}            return None' \
    '        if not path.is_file():{NL}            return set()' \
    "a missing conversation ledger would read as \"no thread exists\", so the first reconcile on a machine where that file has not been written yet would delete EVERY one of his thread seats -- the live one included. Absence is never evidence."

# --- 14. A LIVE THREAD'S SEAT IS NEVER RELEASED -------------------------
mutant live-thread-seat-released "test_his_orphan_thread_seat_is_reconciled_and_a_live_threads_never_is" "$A" \
    '                if row["thread_id"] in known[0]:' \
    '                if False:' \
    "every one of his thread seats would be released whatever the app's record says, so the front desk of a live conversation would lose its cursor mid-turn -- which is the collision the per-thread seat was introduced to end, arriving from the other side."

# --- 15. AND THE RELEASE CARRIES THE REVISION IT WAS ENUMERATED AT ------
# The store's own independent liveness proof: a seat that has bound a turn since
# the enumeration belongs to a demonstrably live thread.
mutant release-without-the-enumerated-revision "test_his_orphan_thread_seat_is_reconciled_and_a_live_threads_never_is" "$A" \
    '                            "expected_revision":int(row["revision"]),' \
    '                            "expected_revision":None,' \
    "the release would carry no proof of what was enumerated, so the store could not refuse a seat that had bound a turn in the meantime -- and the engine would be racing the front desk it is cleaning up after."

# --- 16. THE SEAT OF THE CONVERSATION THIS IS RUNNING IN -----------------
# The one seat that needs no file to decide, and the one whose loss would be
# felt immediately.
mutant this-conversations-own-seat-not-kept "test_his_orphan_thread_seat_is_reconciled_and_a_live_threads_never_is" "$A" \
    '                if row["thread_id"] == binding["thread_id"]:' \
    '                if False:' \
    "this conversation's OWN thread seat would be decided by a file instead of by the fact that it is the one calling, so an unreadable ledger would report the running thread's own seat as unreconciled -- and a ledger that had lost that one record would release it."

# --- 17. AN APP LAND GETS THE REPOSITORY'S OWN CHECKS (plan item 6, 2026-10-21) ----
mutant land-skips-the-repositorys-hooks "test_an_app_land_runs_the_repositorys_own_merge_check_and_a_refusal_lands_nothing" "$A" \
    '    result = subprocess.run(["git", "-c", "commit.gpgSign=false", "-C", str(repo), "merge", "--no-ff", "--no-edit",' \
    '    result = subprocess.run(["git", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgSign=false", "-C", str(repo), "merge", "--no-ff", "--no-edit",' \
    "an app land would again run with no hook of the repository, so a merge its own check refuses would land anyway -- the before-state itself."

# --- 18. AND TAKES THE LAND LEASE WHERE THE FENCE IS ON -----------------
mutant land-takes-no-lease "test_an_app_land_into_a_fenced_repository_takes_the_land_lease_and_lands" "$A" \
    '        lease = _take_fence_lease(repo, path)' \
    '        lease = None' \
    "the land would meet the repository's fence holding no lease, and the fence would refuse every app land into a fenced repository."

mutation_end
