#!/usr/bin/env bash
#
# review-watch.test.sh: THE SECOND REVIEW STARTS BY ITSELF (scripts/review-watch.sh
# running scripts/lib/review_watch.py), against fixture state and a fake
# second-review only. Never the operator's registry, ~/.claude/state, a real
# model or a real teammate.
#
# 2026-10-08: the CEO fetched a Codex review of an agent's work by hand four
# times. His words (ruling §113): "A regular RichOS user can never be expected
# anything even remotely close to that. So, this all must be completely
# automated." Every case below fails on a checkout without scripts/review-watch.sh.
#
#   W01  THE REPLAY of the 2026-10-08 dictation record (timestamps only:
#        registry rows and commit times, scripts/fixtures/review-watch-
#        dictation-2026-10-08.json), one look per minute from 02:52Z to 19:05Z:
#        a handover review starts within two looks of every handover of
#        dictation slices 1-4 (dict1, dict1b, dict1c, dict2, dict3, dict3b,
#        dict4, dict4c) and of slice 5 (dict5, dict5b, dict5c); at least one
#        mid-job review of echo-fable-dict5 starts before 15:03Z and none in a
#        teammate's first 60 minutes; the stopped and discarded dict4b gets
#        none; one tip is never reviewed twice at once; each handover tip is
#        reviewed once
#   W02  a lost review (it ends with no verdict) is started once more, and a
#        second loss is told once; nothing more starts for that commit
#   W02a an app handover lost twice retries after the cooldown, including after
#        restart; no overlapping review or repeated pass, and terminal caps remain
#   W03  a handover, end to end through the script: one review starts at the
#        first look, by registry key, on the agent's own commits only (the
#        merge base with main, which has moved on since), with the handover
#        trigger and the work's key; a second look starts nothing more (the
#        lock); the verdict is told once, with what the lead can do; told
#        again 30 minutes later while nobody continues the work, and not once
#        a continuation names it
#   W04  the long-job clock starts at the registry's start, never at git's
#        dates: a base committed a day before the start gets no review at a
#        commit five minutes in, and one 61 minutes in
#   W05  a running teammate gone quiet (no commit, no transcript write for 20
#        minutes) with unreviewed commits gets a review
#   W06  a handover replaces a mid-job review of the same tip: the mid-job
#        review is stopped by its recorded pid and the handover one starts
#   W07  Codex's READY entry starts a review of its codex/ branch, with the
#        to-codex.md entry that named the branch as the original words; a READY
#        naming only a repository with no second review is silent, while one
#        for a reviewed repository whose commit is missing there is still told
#   W08  a finding still open through two rechecks in a row is told once as
#        not converging; never one filed as a follow-up (blocks false); one
#        work in two repositories is compared repository by repository
#   W09  a repository not in SECOND_REVIEW_REPOS is never reviewed; with the
#        key absent it says so once and starts nothing; a value bash sets that
#        is not names (review_repos in scripts/lib/operator_fences.py, the one
#        reader install and the land use too, sources the config in bash) is
#        said once and starts nothing; work that changes only release notes
#        is never reviewed, and notes with any other file are
#   W10  monitors.json starts review-watch.sh --monitor always
#   W11  --host-json (the operator install's host child, slice 4): a look's notices
#        are one JSON line per lead session, for the host to send to that lead
#   W12  a verdict reaches its lead session even when the worker committed while
#        its review ran: by the review's own record (lock, then attempt row), and
#        by the work's stable identity when no record names it
#   W13  the host's quit (SIGTERM, five-second bound): the watcher stops every
#        review it started at once, escalating to SIGKILL itself, and exits on
#        its own inside the bound with none left running
#   W14  the app's watcher picks only receipts with request.role worker for a
#        mid-job review, never a quiet handover reviewer's cc/ workspace
#   W15  the host's quit records a review stopped only once its whole process
#        group, its reviewer included, is gone (a responsive and a deaf launcher)
#   W16  a SIGTERM during spawn, before the review's pid reaches its lock, still
#        stops that review and settles it as stopped
#   W17  a verdict goes to the review's recorded owner first; a registry fallback
#        needs the same work, never another work holding the reviewed commit
#   W18  a reviewer forked at any point of a stop ends with its review: one asked
#        for the moment before the SIGKILL by a second-review that cannot act on
#        the stop is never forked (the review is frozen), one forked before the
#        stop's first signal is ended (the reviewer's fixture
#        fork_after_final_scan.py)
#   W19  the repeated handover notice goes to the recorded owner of the same
#        work, on that work's own clock, never to another work at the tip
#   W20  the plain monitor prints a verdict only in its owner's session, unless
#        the owner has no live monitor to print it
#   W21  only a verdict's owner consumes it: a monitor's first look tells its own
#        session's verdicts another lead's monitor saw or left (the reviewer's
#        fixture monitor_first_look_owner_loss.py)
#   W22  the operator host recovers a verdict another monitor passed, and
#        verdicts past a block's cap are told at the next look, never dropped
#   W23  the host's quit ends the reviewer's own tool commands too, each in a
#        process group of its own, as Codex runs them
#   W24  nothing undelivered is marked told at the upgrade or expires, and a
#        told verdict stays told while it is in the ledger
#   W25  the quit ends a tool's background task its launching shell left behind,
#        reparented to PID 1: it carries the review's mark (the reviewer's
#        fixture background_tool_quit.py)
#   W26  past run_bounded's time limit the launcher exits, and its lock is settled
#        only once the reviewer's tool, which carries its mark, is gone (the
#        reviewer's fixture timeout_then_quit.py)
#   W27  a verdict is delivered only once the host accepts it: a failed pipe, or no
#        acknowledgment from a host that acknowledges, leaves it for the next look
#        (the reviewer's fixture delivery_ack_gap.py)
#   W28  a mid-job verdict is recorded delivered only once the app's worker hook has
#        written and flushed its answer: a closed pipe leaves it for the next tool call
#        (the reviewer's fixture mid_job_failed_output.py)
#   W29  the quit keeps ending the review's own session after its launcher exits:
#        a platform tool that hides the mark is ended, and the lock is settled only
#        then (the reviewer's fixture orphan_platform_same_session.py)
#   W30  a process table that cannot be read is never an empty session: the quit
#        keeps the review's lock until a read sees its tool gone (the reviewer's
#        fixture process_table_failure.py)
#   W31  the host's watcher knows its host before the first look: a host that died
#        while it started means no look and an exit (the reviewer's fixture
#        parent_exit_before_init.py)
#   W32  the NO VERDICT TWICE notice is delivered only once the host accepts it,
#        like a verdict: a failed pipe, or no acknowledgment, leaves it for the
#        next look (the reviewer's fixture unacknowledged_host_notice.py)
#   W33  a notice the monitor's output cap leaves out is neither delivered nor told:
#        a later look tells it (the reviewer's fixture capped_lost_notice.py)
#   W34  the NOT CONVERGING notice is kept until its output is accepted, like its
#        verdict: a failed pipe, or no acknowledgment, tells both again
#   W36  a NO VERDICT TWICE notice is never told for a commit already on main, and one
#        that has not landed still is (2026-10-24: four notices from a session ended
#        2026-10-21, for landed commits, told again by a new session)
#   W37  every look of the monitor is fresh code: after its review_watch.py changes on
#        disk, the next look runs the changed code and records the source it evaluated;
#        a look that fails is told once per failing streak and the monitor keeps watching;
#        --status says watching only after a recent successful look
#        (2026-10-24: a landed fix never reached the session's monitor, PID 90830)
#   W38  a look whose evaluation raises is a failed look: told once per failing streak,
#        --status says failing; a clean look reads healthy again; the same error after a
#        successful look is told again (the second review of 5331505bd, finding 1)
#
# Every case loads scripts/lib/app_review.py too: review_watch.py imports its app_paths.
# The app mode (--app-state) and app_review.py's mid-job notice are driven over the real
# app fixture in mega-lander/tests/app.test.py.
#
# Exit 0 = every case passed; exit 1 = at least one failed.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$SCRIPT_DIR/.." && pwd)"
RW="$SCRIPT_DIR/review-watch.sh"
LIB="$SCRIPT_DIR/lib"
FIXTURE="$SCRIPT_DIR/fixtures/review-watch-dictation-2026-10-08.json"

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }
command -v git >/dev/null 2>&1 || { echo "FATAL: git required" >&2; exit 1; }

# shellcheck source=lib/scratch.sh
. "$SCRIPT_DIR/lib/scratch.sh"
SB="$(scratch_new review-watch-test)" || { echo "FATAL: no scratch" >&2; exit 1; }
cleanup() {
    # A fake review still sleeping is stopped by the pid it recorded at its start.
    for f in "$SB"/calls/*/pid; do
        [ -f "$f" ] || continue
        p="$(cat "$f")"
        kill -0 "$p" 2>/dev/null && kill -KILL "$p" 2>/dev/null
    done
    scratch_release "$SB" >/dev/null 2>&1 || true
}
trap cleanup EXIT

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }
check() { if [ "$2" -eq 0 ]; then ok "$1"; else bad "$1 -- $3"; fi; }
has()   { printf '%s' "$1" | grep -qF -- "$2"; }

echo "review-watch.test.sh"
[ -f "$RW" ] || echo "  (scripts/review-watch.sh is missing: every case below fails)"

# --- isolation -------------------------------------------------------------
unset CLAUDE_PROJECT_DIR CLAUDE_PLUGIN_ROOT RICHOS_ENGINE_ROOT CODEX_HOME REVIEW_WATCH_NOW
mkdir -p "$SB/bin" "$SB/calls" "$SB/claude" "$SB/workspaces/agents" "$SB/workspaces/done" "$SB/projects" "$SB/codex"
export CLAUDE_CONFIG_DIR="$SB/claude"
export RICHOS_WORKSPACES_DIR="$SB/workspaces"
export RICHOS_PROJECTS_DIR="$SB/projects"
export RICHOS_SESSIONS_DIR="$SB/sessions"
export REVIEW_WATCH_STATE_DIR="$SB/rw"
export SECOND_REVIEW_STATE_DIR="$SB/sr"
export REVIEW_WATCH_SECOND_REVIEW="$SB/bin/fake-second-review"
export REVIEW_WATCH_TO_RICH="$SB/codex/to-rich.md"
export REVIEW_WATCH_TO_CODEX="$SB/codex/to-codex.md"
export FAKE_CALLS="$SB/calls"

# =============================================================================
# W01, W02: the replay, driving the real watcher with a fixture world
# =============================================================================
python3 - "$LIB" "$FIXTURE" "$SB/replay" <<'PY' >"$SB/replay.out" 2>&1
import datetime, json, os, sys
lib, fixture, root = sys.argv[1:4]
os.makedirs(root)
os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, "rw")
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
sys.path.insert(0, lib)
results = {}
def say(case, ok, why=""):
    print("%s %s %s" % ("PASS" if ok else "FAIL", case, why))
try:
    import review_watch as rw
except Exception as exc:  # the watcher is missing: every case fails
    for c in ("W01", "W02"):
        say(c, False, "review_watch cannot be imported: %s" % exc)
    sys.exit(0)

def ep(s):
    return datetime.datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()
def hm(t):
    return datetime.datetime.fromtimestamp(t, datetime.timezone.utc).strftime("%H:%M:%SZ")

REPO = "/fixture/richos"
REVIEW_SECONDS = 30 * 60      # the first by-hand review's measured length (Sage's check §1.2)
data = json.load(open(fixture))
team = data["teammates"]
for t in team:
    t["start"] = ep(t["started_at"] or t["registered_at"])
    t["end"] = ep(t["end_at"])
    t["gone"] = ep(t["disposition"]["at"]) if t.get("disposition") else None
    for c in t["commits"]:
        c["t"] = ep(c["at"])
by_name = dict((t["name"], t) for t in team)

def root_of(t):
    while t["continues"]:
        t = by_name[t["continues"][0]]
    return t["name"]

class ReplayWorld(object):
    repos = ["richos"]
    repos_error = ""
    def __init__(self):
        self.now = 0
    def registry_items(self, now, seen):
        items = []
        live = [t for t in team if t["start"] <= now and not (t["gone"] is not None and t["gone"] <= now)]
        continued = set(c for t in live for c in t["continues"])
        for t in live:
            commits = [c for c in t["commits"] if c["t"] <= now]
            if not commits:
                continue
            seen.setdefault("teammate:" + root_of(t), now)
            items.append(rw.Item("teammate:" + root_of(t), t["name"], REPO, commits[-1]["sha"], "base-" + t["name"],
                                 "ended" if t["end"] <= now else "running", started=t["start"], ref=t["name"],
                                 continued=t["name"] in continued))
        return items, []
    def codex_items(self, now, codex_state):
        return [], []

class ReplayWatcher(rw.Watcher):
    """The real watcher; only its processes are simulated."""
    def __init__(self, world, mode="pass"):
        rw.Watcher.__init__(self, "/fixture/engine", "/fixture/orchestration.config", world)
        self.mode = mode
        self.fake = {}            # pid -> review
        self.starts = []
        self.next_pid = 1000
        self.now = 0
    def spawn(self, item, trigger, log_path):
        self.next_pid += 1
        pid = self.next_pid
        self.fake[pid] = {"item": item, "trigger": trigger, "at": self.now, "end": self.now + REVIEW_SECONDS,
                          "stopped": False, "done": False}
        self.starts.append((self.now, item.name, item.tip, trigger, pid))
        return pid, "fake-%d" % pid
    def alive(self, info):
        f = self.fake.get(info.get("pid"))
        if f is None:
            return False
        if self.mode == "die":
            return False
        return not f["stopped"] and self.now < f["end"]
    def stop(self, info):
        f = self.fake.get(info.get("pid"))
        if f:
            f["stopped"] = True
    def finish_due(self, now):
        """A fake review that reached its end writes its ledger row, as second-review does."""
        for pid, f in sorted(self.fake.items()):
            if f["done"] or f["stopped"] or now < f["end"] or self.mode == "die":
                continue
            f["done"] = True
            it = f["item"]
            rid = "rv-fake-%d" % pid
            rec = os.path.join(os.environ["SECOND_REVIEW_STATE_DIR"], "reviews", rid)
            os.makedirs(rec)
            verdict = "changes-requested" if (it.name == "echo-fable-dict5" and f["trigger"] == "handover") else "passed"
            findings = [{"priority": 2, "title": "a fake finding", "files": ["x.rs:1"], "evidence": "e", "fixture": ""}] \
                if verdict != "passed" else []
            json.dump({"answer": {"findings": findings, "earlier_findings": []}, "earlier_findings_in": []},
                      open(os.path.join(rec, "verdict.json"), "w"))
            rw.append_jsonl(rw.review_ledger(), {
                "id": rid, "at": rw.iso(f["at"]), "repo": REPO, "tip": it.tip, "work": it.work, "trigger": f["trigger"],
                "verdict": verdict, "findings": len(findings), "p1": 0, "reviewer": "codex",
                "reviewer_model": "gpt-6.1-sol", "author": it.name, "record": rec})

def run(watcher, t0, t1, step=60):
    out = []
    sd = rw.session_dir("replay-session")
    t = t0
    while t <= t1:
        watcher.now = t
        watcher.finish_due(t)
        # concurrency check: no two live fake reviews of one tip at once
        live = [(f["item"].repo, f["item"].tip) for f in watcher.fake.values()
                if not f["stopped"] and not f["done"] and f["at"] <= t < f["end"] and watcher.mode != "die"]
        if len(live) != len(set(live)):
            out.append(("DOUBLE", t, live))
        import io
        buf = io.StringIO()
        rw.tick(watcher, sd, now=t, out=buf)
        if buf.getvalue():
            out.append(("TOLD", t, buf.getvalue()))
        t += step
    return out

# -- W01 ----------------------------------------------------------------------
w = ReplayWatcher(ReplayWorld())
t0 = min(t["start"] for t in team) - 60
t1 = max(t["end"] for t in team) + 30 * 60
log = run(w, t0, t1)
problems = []
for t in team:
    if not t["commits"]:
        mine = [s for s in w.starts if s[1] == t["name"]]
        if mine:
            problems.append("%s has no commits but got %d review(s)" % (t["name"], len(mine)))
        continue
    final = [c for c in t["commits"] if c["t"] <= t["end"]][-1]["sha"]
    ho = [s for s in w.starts if s[1] == t["name"] and s[2] == final and s[3] == "handover"]
    if len(ho) != 1:
        problems.append("%s: %d handover reviews of its final tip %s" % (t["name"], len(ho), final))
    elif not (t["end"] <= ho[0][0] <= t["end"] + 120):
        problems.append("%s: its handover review started at %s, not within two looks of its end %s" % (
            t["name"], hm(ho[0][0]), hm(t["end"])))
    early = [s for s in w.starts if s[1] == t["name"] and s[3] == "long-job" and s[0] < t["start"] + 3600]
    if early:
        problems.append("%s: a long-job review at %s, inside its first 60 minutes" % (t["name"], hm(early[0][0])))
    print("  replay: %-20s start %s end %s | %s" % (t["name"], hm(t["start"]), hm(t["end"]), ", ".join(
        "%s %s@%s" % (s[3], hm(s[0]), s[2]) for s in w.starts if s[1] == t["name"])))
d5 = by_name["echo-fable-dict5"]
mid = [s for s in w.starts if s[1] == "echo-fable-dict5" and s[3] in ("long-job", "quiet")]
before = [s for s in mid if s[0] < ep("2026-10-08T15:03:00Z")]
if not before:
    problems.append("no mid-job review of echo-fable-dict5 started before 15:03Z (mid-job starts: %s)" % [hm(s[0]) for s in mid])
doubles = [x for x in log if x[0] == "DOUBLE"]
if doubles:
    problems.append("one tip reviewed twice at once at %s: %s" % (hm(doubles[0][1]), doubles[0][2]))
slices = ["echo-opus-dict1", "echo-opus-dict1b", "echo-opus-dict1c", "echo-opus-dict2", "echo-opus-dict3",
          "echo-sonnet-dict3b", "echo-opus-dict4", "echo-opus-dict4c"]
missing = [n for n in slices if n not in by_name]
if missing:
    problems.append("the fixture lacks %s" % missing)
told = "".join(x[2] for x in log if x[0] == "TOLD")
if told.count("[CHANGES-REQUESTED] echo-fable-dict5, handover review") != 1:
    problems.append("dict5's changes-requested handover verdict was told %d times, not once (dict5b continues it)"
                    % told.count("[CHANGES-REQUESTED] echo-fable-dict5, handover review"))
say("W01", not problems, "; ".join(problems) or "%d reviews started; dict5 mid-job at %s" % (
    len(w.starts), ", ".join(hm(s[0]) for s in mid)))

# -- W02 ----------------------------------------------------------------------
os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, "rw-lost")
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr-lost")
one = dict(by_name["echo-opus-dict1c"])
team[:] = [one]
w2 = ReplayWatcher(ReplayWorld(), mode="die")
log2 = run(w2, one["end"], one["end"] + 10 * 60)
told2 = "".join(x[2] for x in log2 if x[0] == "TOLD")
starts = [s for s in w2.starts]
ok2 = len(starts) == 2 and told2.count("[NO VERDICT TWICE] echo-opus-dict1c") == 1
say("W02", ok2, "starts=%s told=%r" % ([(hm(s[0]), s[3]) for s in starts], told2[:600]))

# The app has no manual CLI review path: two transient failures must not strand
# the only verdict integration can consume. Drive the real watcher and durable
# attempts through the cooldown with no real waiting, provider or model calls.
os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, "rw-app-lost")
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr-app-lost")
one["gone"] = None  # unlike the historical fixture, this job cannot land without its review
class AppReplayWorld(ReplayWorld):
    def registry_items(self, now, seen):
        items, problems = super().registry_items(now, seen)
        for item in items:
            item.source = "app"
        return items, problems
w3 = ReplayWatcher(AppReplayWorld(), mode="die")
start = one["end"]
first = run(w3, start, start + 120)
notice = "".join(x[2] for x in first if x[0] == "TOLD")
lost_at = start + 120
cooldown = rw.REPEAT_MINUTES * 60
run(w3, lost_at + 60, lost_at + cooldown - 60)
before = len(w3.starts)
# A new watcher must read the existing attempt ledger, not reset its retry budget.
resumed = ReplayWatcher(AppReplayWorld(), mode="pass")
resumed.next_pid = 2000
retry_at = lost_at + cooldown
run(resumed, retry_at, retry_at + 60)
after = len(resumed.starts)
run(resumed, retry_at + 120, retry_at + REVIEW_SECONDS + cooldown * 2)
rows = rw.read_jsonl(rw.review_ledger())
ok3 = before == 2 and after == 1 and len(resumed.starts) == 1 and any(r.get("verdict") == "passed" for r in rows)
ok3 = ok3 and "automatic handover review will retry" in notice and "Nothing more starts" not in notice
# Persistent failure waits from the latest loss. Terminal and progress reviews
# keep their cap, while an app worker that stopped early gets no handover.
item = AppReplayWorld().registry_items(retry_at, {})[0][0]
losses = [dict(repo=item.repo, tip=item.tip, outcome="lost", trigger="handover", ended_at=retry_at)] * 3
book = rw.Book([], {}, losses)
ok3 = ok3 and not rw.due([item], book, retry_at + cooldown - 1, {}, 3600)
ok3 = ok3 and len(rw.due([item], book, retry_at + cooldown, {}, 3600)) == 1
item.source = "registry"
ok3 = ok3 and not rw.due([item], book, retry_at + cooldown * 10, {}, 3600)
item.source, item.state = "app", "running"
ok3 = ok3 and not rw.due([item], book, retry_at + cooldown * 10, {}, 3600)
item.state, item.stopped_early = "ended", True
ok3 = ok3 and not rw.due([item], book, retry_at + cooldown * 10, {}, 3600)
say("W02a", ok3, "starts before cooldown=%d, after restart=%d, final=%d, verdicts=%s" %
    (before, after, len(resumed.starts), [r.get("verdict") for r in rows]))
PY
REPLAY="$(cat "$SB/replay.out")"
grep '^  replay:' "$SB/replay.out" | sed 's/^/    /'
for c in W01 W02 W02a; do
    line="$(grep "^[A-Z]* $c " "$SB/replay.out" | head -1)"
    case "$line" in
        "PASS $c "*) ok "$c ${line#PASS $c }" ;;
        "FAIL $c "*) bad "$c ${line#FAIL $c }" ;;
        *) bad "$c did not run: $(tail -5 "$SB/replay.out" | tr '\n' ' ')" ;;
    esac
done

# =============================================================================
# W03-W10: the real script, a fixture registry and repository, a fake review
# =============================================================================
cat >"$SB/bin/fake-second-review" <<'SH'
#!/usr/bin/env bash
exec python3 "$(dirname "$0")/fake-sr.py" "$@"
SH
cat >"$SB/bin/fake-sr.py" <<'PY'
#!/usr/bin/env python3
"""Records its arguments; FAKE_SR_MODE: sleep (runs until stopped), else writes
one ledger row with that verdict, as second-review does, and ends."""
import json, os, sys, time
argv = sys.argv[1:]
log = os.environ["FAKE_CALLS"]
d = os.path.join(log, "%02d" % len(os.listdir(log)))
os.makedirs(d)
json.dump(argv, open(os.path.join(d, "argv.json"), "w"))
open(os.path.join(d, "pid"), "w").write(str(os.getpid()))
words = []
for i, a in enumerate(argv):
    if a == "--words-file":
        words.append(open(argv[i + 1]).read())
open(os.path.join(d, "words.txt"), "w").write("".join(words))
mode = os.environ.get("FAKE_SR_MODE", "passed")
if mode == "sleep":
    time.sleep(600)
    sys.exit(0)
val = lambda k: argv[argv.index(k) + 1] if k in argv else ""
repo = os.path.realpath(val("--repo"))
state = os.environ["SECOND_REVIEW_STATE_DIR"]
rid = "rv-fake-%s" % os.path.basename(d)
rec = os.path.join(state, "reviews", rid)
os.makedirs(rec)
findings = [] if mode == "passed" else [{"priority": 2, "title": "Shutdown leaves audio on disk", "files": ["a.txt:1"],
                                         "evidence": "e", "fixture": ""}]
json.dump({"answer": {"findings": findings, "earlier_findings": []}, "earlier_findings_in": []},
          open(os.path.join(rec, "verdict.json"), "w"))
at = float(os.environ.get("REVIEW_WATCH_NOW") or time.time())
row = {"id": rid, "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(at)), "repo": repo, "tip": val("--tip"),
       "work": val("--work"), "trigger": val("--trigger"), "verdict": mode, "findings": len(findings), "p1": 0,
       "reviewer": "codex", "reviewer_model": "gpt-6.1-sol", "author": "x", "record": rec}
with open(os.path.join(state, "reviews.jsonl"), "a") as f:
    f.write(json.dumps(row) + "\n")
PY
chmod +x "$SB/bin/fake-second-review" "$SB/bin/fake-sr.py"
mkdir -p "$SB/sr"

REPO="$SB/richos"
G() { git -C "$REPO" -c core.hooksPath=/dev/null -c user.name=Author -c user.email=author@example.invalid -c commit.gpgsign=false "$@"; }
mkdir -p "$REPO"
G init -q -b main
printf 'base\n' >"$REPO/a.txt"
GIT_COMMITTER_DATE="2026-10-07T09:00:00Z" GIT_AUTHOR_DATE="2026-10-07T09:00:00Z" G add -A
GIT_COMMITTER_DATE="2026-10-07T09:00:00Z" GIT_AUTHOR_DATE="2026-10-07T09:00:00Z" G commit -qm "base, a day old"
BASE="$(G rev-parse HEAD)"
G checkout -qb cc/echo-sonnet-w1
printf 'base\nmine\n' >"$REPO/a.txt"
G commit -qam "the agent's own commit"
TIP="$(G rev-parse HEAD)"
G checkout -q main
printf 'unrelated\n' >"$REPO/b.txt"
G add -A; G commit -qm "an unrelated main commit, after the branch point"
G checkout -q cc/echo-sonnet-w1
printf 'SECOND_REVIEW_REPOS="richos"\n' >"$SB/orchestration.config"
cfg() { printf '%s\n' "$1" >"$SB/orchestration.config"; }

# reg <name> <started-ago-seconds> <ended-ago-seconds or -> [continues-key] : one registry record
reg() {
    python3 - "$SB" "$REPO" "$@" <<'PY'
import json, os, sys, time
sb, repo, name, started_ago, ended_ago = sys.argv[1:6]
cont = sys.argv[6:]
now = time.time()
key = "sess-w--" + name
rec = {"key": key, "name": name, "agent_id": "", "registered_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(now - float(started_ago))),
       "started_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(now - float(started_ago))),
       "end": None if ended_ago == "-" else {"at": now - float(ended_ago), "signal": "SubagentStop"},
       "handed_in": None, "pause": None, "disposition": None, "continues": cont, "orphan": False,
       "workspaces": [{"kind": "cc", "repo": repo, "path": repo, "branch": "cc/" + name, "deleted_at": None},
                      {"kind": "native", "repo": os.path.join(sb, "femcboost"), "path": os.path.join(sb, "femcboost"),
                       "branch": "worktree-agent-x", "deleted_at": None}]}
json.dump(rec, open(os.path.join(sb, "workspaces", "agents", key + ".json"), "w"))
json.dump({"repos": [os.path.realpath(repo)]}, open(os.path.join(sb, "workspaces", "repos.json"), "w"))
PY
}
tickrw() { OUT="$(RICHOS_ENTITY_ROOT="$SB" bash "$RW" --tick 2>&1)"; RC=$?; }
ncalls() { ls "$SB/calls" | wc -l | tr -d ' '; }
lastargv() { cat "$(ls -d "$SB"/calls/* | tail -1)/argv.json" 2>/dev/null; }
resetstate() { rm -rf "$SB/rw" "$SB/sr" "$SB/calls" "$SB/workspaces/agents"; mkdir -p "$SB/sr" "$SB/calls" "$SB/workspaces/agents"; }
waitcalls() { for _ in $(seq 1 100); do [ "$(ncalls)" -ge "$1" ] && break; sleep 0.1; done; }
waitrows() {
    for _ in $(seq 1 100); do
        if [ -f "$SB/sr/reviews.jsonl" ] && [ "$(wc -l <"$SB/sr/reviews.jsonl" | tr -d ' ')" -ge "$1" ]; then break; fi
        sleep 0.1
    done
}

# --- W03 ---------------------------------------------------------------------
resetstate
reg echo-sonnet-w1 600 30
FAKE_SR_MODE=changes-requested tickrw
waitcalls 1; waitrows 1
A="$(lastargv)"
[ "$RC" -eq 0 ] && [ "$(ncalls)" = "1" ] && has "$A" '"--name", "sess-w--echo-sonnet-w1"' && has "$A" "\"--tip\", \"$TIP\"" \
  && has "$A" "\"--base\", \"$BASE\"" && has "$A" '"--trigger", "handover"' && has "$A" '"--work", "teammate:sess-w--echo-sonnet-w1"'
check "W03 a handover starts one review at the first look: by key, the agent's own commits only, the handover trigger" $? "rc=$RC calls=$(ncalls) argv=$A out=$OUT"
FAKE_SR_MODE=changes-requested tickrw
[ "$(ncalls)" = "1" ] && has "$OUT" "[CHANGES-REQUESTED] echo-sonnet-w1, handover review" && has "$OUT" "continues: echo-sonnet-w1" \
  && has "$OUT" "Shutdown leaves audio on disk"
check "W03 the verdict is told once with its findings and what the lead can do; no second review of the tip" $? "calls=$(ncalls) out=$OUT"
has "$OUT" "second-review.sh --follow-up <finding id> --why"
check "W03 the notice also names the one command that files a blocking finding as a follow-up (follow-up 12, D1)" $? "out=$OUT"
tickrw
[ -z "$OUT" ]; check "W03 the next look tells nothing again" $? "out=$OUT"
REVIEW_WATCH_NOW="$(python3 -c 'import time; print(time.time() + 31 * 60)')" tickrw
has "$OUT" "told again, notice 2"; check "W03 unhandled after 30 minutes: told again" $? "out=$OUT"
reg echo-sonnet-w1b 60 - sess-w--echo-sonnet-w1
REVIEW_WATCH_NOW="$(python3 -c 'import time; print(time.time() + 65 * 60)')" tickrw
[ "$RC" -eq 0 ] && ! has "$OUT" "[CHANGES-REQUESTED] echo-sonnet-w1,"; check "W03 once a continuation names the work, it is not told again" $? "out=$OUT"

# --- W11 ---------------------------------------------------------------------
# The operator install's host child (slice 4, Sage's check §1.3): --host-json prints each
# look's notices as one JSON line per lead session, so the host can send each to the lead
# whose session registered the work, as a message of its own.
resetstate
reg echo-sonnet-w1 600 30
python3 - "$SB" <<'PY'
import json, os, sys
p = os.path.join(sys.argv[1], "workspaces", "agents", "sess-w--echo-sonnet-w1.json")
r = json.load(open(p)); r["session_id"] = "lead-session-1"
json.dump(r, open(p, "w"))
PY
OUT="$(RICHOS_ENTITY_ROOT="$SB" FAKE_SR_MODE=changes-requested bash "$RW" --tick --host-json 2>&1)"; RC=$?
waitcalls 1; waitrows 1
OUT="$(RICHOS_ENTITY_ROOT="$SB" bash "$RW" --tick --host-json 2>&1)"; RC=$?
printf '%s\n' "$OUT" | python3 -c '
import json, sys
lines = [l for l in sys.stdin.read().splitlines() if l.strip()]
rows = [json.loads(l) for l in lines]
assert len(rows) == 1, rows
assert rows[0]["session"] == "lead-session-1", rows
assert "[CHANGES-REQUESTED] echo-sonnet-w1, handover review" in rows[0]["text"], rows
assert rows[0]["text"].startswith("REVIEW-WATCH "), rows
'
check "W11 --host-json: the verdict is one JSON line for the lead session that registered the work" $? "rc=$RC out=$OUT"

# --- W12 ---------------------------------------------------------------------
# Second review of cc/echo-opus-review4 at cac403f77, finding 1 (fixtures/operator_notice.py):
# the lead session was found only through an item at the reviewed tip, so a worker that committed
# while its review ran got its verdict sent to session "", which the host refuses, while the
# consumed-row cursor moved on: the verdict was lost.
resetstate
reg echo-sonnet-w1 3660 -
python3 - "$SB" <<'PY'
import json, os, sys
p = os.path.join(sys.argv[1], "workspaces", "agents", "sess-w--echo-sonnet-w1.json")
r = json.load(open(p)); r["session_id"] = "lead-session-1"
json.dump(r, open(p, "w"))
PY
OUT="$(RICHOS_ENTITY_ROOT="$SB" FAKE_SR_MODE=changes-requested bash "$RW" --tick --host-json 2>&1)"; RC=$?
waitcalls 1; waitrows 1
printf 'base\nmine\nmore\n' >"$REPO/a.txt"
G commit -qam "the worker commits again while its review runs"
OUT="$(RICHOS_ENTITY_ROOT="$SB" bash "$RW" --tick --host-json 2>&1)"; RC=$?
G reset -q --hard "$TIP"
printf '%s\n' "$OUT" | python3 -c '
import json, sys
rows = [json.loads(l) for l in sys.stdin.read().splitlines() if l.strip()]
assert len(rows) == 1, rows
assert rows[0]["session"] == "lead-session-1", rows
assert "[CHANGES-REQUESTED] " in rows[0]["text"] and "mid-job (long-job)" in rows[0]["text"], rows
'
check "W12 the worker committed while its review ran: the verdict still goes to its lead session (the review's record)" $? \
    "rc=$RC out=$OUT"
python3 - "$LIB" "$SB/w12" <<'PY'
import json, os, sys
lib, root = sys.argv[1:3]
os.makedirs(root)
os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, "rw")
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
sys.path.insert(0, lib)
import review_watch as rw
rw.HOST_JSON["on"] = True
repo = os.path.realpath(os.path.join(root, "fictional-repo"))
old, new = "a" * 40, "b" * 40
row = {"id": "review-1", "repo": repo, "tip": old, "work": "teammate:worker", "trigger": "long-job",
       "verdict": "changes-requested", "findings": 1, "p1": 1, "author": "worker", "reviewer": "claude",
       "reviewer_model": "opus"}
got = {}
for label, tip in (("unchanged tip", old), ("worker committed during review", new)):
    item = rw.Item("teammate:worker", "worker", repo, tip, "c" * 40, "running")
    item.session = "lead-session-1"
    state = {"rows": 0}
    notice = json.loads(rw.tell(10000, state, [row], rw.Book([row], {}, []), [item], [], [])[0])
    got[label] = (notice["session"], state["rows"])
print(got)
sys.exit(0 if all(v == ("lead-session-1", 1) for v in got.values()) else 1)
PY
check "W12 with no record of the review, the work's stable identity names the lead session (the reviewer's fixture)" $? "see above"

# --- W13 ---------------------------------------------------------------------
# Finding 2 (fixtures/shutdown.py): the host gives the watcher five seconds after SIGTERM
# (review_watch.rs STOP_BOUND), and stop_own stopped reviews one after another with ten seconds
# each before SIGKILL; reviews lead their own sessions, so the host's group SIGKILL left them
# running. Two reviews that ignore SIGTERM, the real run_host_loop and its SIGTERM path.
cat >"$SB/w13.py" <<'PY'
import json, os, signal, subprocess, sys, time
lib, root = sys.argv[1], sys.argv[2]
STAND_IN = ("import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); "
            "print('ready',flush=True); time.sleep(60)")
if len(sys.argv) > 3:
    # The watcher: the host's child, owning two review stand-ins, each in its own session.
    os.environ["REVIEW_WATCH_STATE_DIR"] = root
    os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "reviews")
    os.environ["REVIEW_WATCH_POLL_SECONDS"] = "0.2"
    sys.path.insert(0, lib)
    import review_watch as rw

    class Quiet(rw.Watcher):
        told = False
        def look(self, now, session_state):
            if not self.told:                       # the SIGTERM handlers are in place by now
                self.told = True
                print("ready", flush=True)
            return []
    watcher = Quiet("/fixture/engine", "", world=object())
    owned = []
    for n in range(2):
        p = subprocess.Popen([sys.executable, "-c", STAND_IN], stdout=subprocess.PIPE, text=True,
                             start_new_session=True)
        assert p.stdout.readline().strip() == "ready"
        watcher.children[p.pid] = p
        owned.append(p.pid)
        lock = os.path.join(root, "locks", "%d.lock" % n)
        os.makedirs(os.path.dirname(lock), exist_ok=True)
        json.dump({"pid": p.pid, "repo": "fictional", "tip": str(n), "started_at": time.time()}, open(lock, "w"))
    json.dump(owned, open(os.path.join(root, "pids.json"), "w"))
    sys.exit(rw.run_host_loop(watcher, "app"))

def exists(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
os.makedirs(root)
p = subprocess.Popen([sys.executable, os.path.abspath(__file__), lib, root, "watcher"], stdout=subprocess.PIPE,
                     text=True, start_new_session=True)
pids, ok = [], False
try:
    assert p.stdout.readline().strip() == "ready"
    pids = json.load(open(os.path.join(root, "pids.json")))
    t0 = time.monotonic()
    os.kill(p.pid, signal.SIGTERM)
    try:
        p.wait(timeout=5)                           # the host's STOP_BOUND
        killed = False
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, signal.SIGKILL)            # what the host does at the bound
        p.wait(timeout=3)
        killed = True
    took = time.monotonic() - t0
    time.sleep(0.2)
    alive = [x for x in pids if exists(x)]
    try:
        outcomes = [json.loads(l)["outcome"] for l in open(os.path.join(root, "attempts.jsonl"))]
    except OSError:
        outcomes = []
    print("host bound=5s, watcher exit=%s after %.2fs (killed by the host: %s), reviews still alive=%d of %d, "
          "attempts=%s" % (p.returncode, took, killed, len(alive), len(pids), outcomes))
    ok = not killed and not alive and outcomes == ["stopped", "stopped"]
finally:
    if p.poll() is None:
        os.killpg(p.pid, signal.SIGKILL)
        p.wait()
    for x in pids:
        try:
            os.killpg(x, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass
sys.exit(0 if ok else 1)
PY
W13OUT="$(python3 "$SB/w13.py" "$LIB" "$SB/w13" 2>&1)"; W13RC=$?
printf '    %s\n' "$W13OUT"
check "W13 the host's quit: every review is stopped at once, inside the five-second bound, none left running" $W13RC \
    "$W13OUT"

# --- W14 ---------------------------------------------------------------------
# Finding 3 (fixtures/reviewer_as_worker.py): AppWorld took every cc/ workspace as implementation
# work, and a handover reviewer gets one at the worker's commit, so a quiet or long reviewer was
# itself picked for a mid-job review whose verdict could never reach the worker.
python3 - "$LIB" "$SB/w14" <<'PY'
import json, os, sys
from types import SimpleNamespace
from unittest.mock import patch
lib, root = sys.argv[1:3]
os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, "rw")
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
sys.path.insert(0, lib)
import review_watch as rw
app = os.path.join(root, "app")
part = os.path.join(app, "workspaces", "p" * 64)
receipts = os.path.join(app, "work-receipts", "p" * 64)
os.makedirs(part)
os.makedirs(receipts)
repo = os.path.join(root, "fictional-project")
recs = []
for n, (name, role) in enumerate((("mark-sonnet-w", "worker"), ("frank-opus-review", "reviewer"))):
    json.dump({"id": "r%d" % n, "name": name, "request": {"role": role}},
              open(os.path.join(receipts, "r%d.json" % n), "w"))
    recs.append({"key": "lead--" + name, "name": name, "session_id": "lead",
                 "registered_at": "2026-10-09T00:00:00Z", "started_at": "2026-10-09T00:00:00Z",
                 "workspaces": [{"kind": "cc", "repo": repo, "path": os.path.join(repo, name), "branch": "cc/" + name}]})
world = rw.AppWorld.__new__(rw.AppWorld)
world.app_state, world.claude, world.repos, world._merge_bases = app, "fictional-claude", ["(every)"], {}
world.accounts = ""
world.ws = SimpleNamespace(all_agents=lambda: recs, finished_state=lambda r, c: (False, False, ""),
                         review_chain=lambda r: [r], recovery_work_key=lambda r: "teammate:"+r["key"])
world.src = SimpleNamespace()
now = rw.parse_iso("2026-10-09T01:01:00Z")
with patch.object(world, "integration_tip", return_value=("b" * 40, "")), \
     patch.object(world, "merge_base", return_value="b" * 40), \
     patch.object(rw, "git", return_value="a" * 40), \
     patch.object(rw.stall_watch, "_git_last_commit", return_value=None), \
     patch.object(rw.stall_watch, "_transcript", return_value=""):
    seen = {}
    items, _problems = world.registry_items(now, seen)
    due = rw.due(items, rw.Book([], {}, []), now, seen, 3600)
picked = [(i.name, t) for i, t, _ in due]
print("picked for a mid-job review:", picked)
sys.exit(0 if picked == [("mark-sonnet-w", "long-job")] else 1)
PY
check "W14 the app's watcher reviews worker receipts, never a reviewer" $? "see above"

# --- W15 ---------------------------------------------------------------------
# Second review of cc/echo-opus-review4b at f14155545, finding 1 (fixtures/shutdown_descendant.py):
# second-review's reviewer leads its own session, so the watcher's SIGTERM to the launcher's group
# never reaches it; _on_stop sent it one SIGTERM and exited at once, and stop_all, seeing the
# launcher gone, recorded "stopped" over a reviewer that ignores SIGTERM and runs on. Two
# launchers: the real second_review._on_stop and run_bounded ("responsive"), and one that ignores
# SIGTERM itself ("deaf"), whose reviewer only the watcher can stop. Since the second review of
# b5ff41f02 (finding 1) the reviewer stays in its launcher's process group, as second-review's
# run_bounded keeps it, and the one group signal reaches both.
cat >"$SB/w15.py" <<'PY'
import json, os, signal, subprocess, sys, time
lib, root, mode = sys.argv[1], sys.argv[2], sys.argv[3]
role = sys.argv[4] if len(sys.argv) > 4 else ""
sys.path.insert(0, lib)
REVIEWER = ("import os,signal,sys,time\n"
            "signal.signal(signal.SIGTERM,signal.SIG_IGN)\n"
            "open(os.path.join(sys.argv[1],'reviewer.pid'),'w').write(str(os.getpid()))\n"
            "while True:\n"
            "    open(os.path.join(sys.argv[1],'heartbeat'),'w').write(str(time.monotonic()))\n"
            "    time.sleep(0.05)\n")
if role == "launcher":
    if mode == "responsive":
        import second_review as sr
        for sig in (signal.SIGTERM, signal.SIGHUP):
            signal.signal(sig, sr._on_stop)
        sr.run_bounded([sys.executable, "-B", "-c", REVIEWER, root], root, "", os.devnull, os.devnull, 60)
    else:
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        subprocess.Popen([sys.executable, "-B", "-c", REVIEWER, root]).wait()
    sys.exit(0)
if role == "watcher":
    os.environ["REVIEW_WATCH_STATE_DIR"] = root
    os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "reviews")
    os.environ["REVIEW_WATCH_POLL_SECONDS"] = "0.1"
    import review_watch as rw

    class Quiet(rw.Watcher):
        told = False
        def look(self, now, session_state):
            if not self.told:                       # the SIGTERM handlers are in place by now
                self.told = True
                print("ready", flush=True)
            return []
    w = Quiet("/fixture/engine", "", world=object())
    p = subprocess.Popen([sys.executable, "-B", os.path.abspath(__file__), lib, root, mode, "launcher"],
                         start_new_session=True)
    w.children[p.pid] = p
    open(os.path.join(root, "launcher.pid"), "w").write(str(p.pid))
    os.makedirs(os.path.join(root, "locks"))
    json.dump({"pid": p.pid, "repo": "fictional", "tip": "a" * 40, "started_at": time.time()},
              open(os.path.join(root, "locks", "one.lock"), "w"))
    deadline = time.monotonic() + 5
    while not os.path.exists(os.path.join(root, "reviewer.pid")):
        if time.monotonic() > deadline:
            raise SystemExit("the reviewer did not start")
        time.sleep(0.01)
    sys.exit(rw.run_host_loop(w, "app"))

def exists(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
os.makedirs(root)
watcher = subprocess.Popen([sys.executable, "-B", os.path.abspath(__file__), lib, root, mode, "watcher"],
                           stdout=subprocess.PIPE, text=True, start_new_session=True)
reviewer = launcher = None
ok = False
try:
    assert watcher.stdout.readline().strip() == "ready"
    launcher = int(open(os.path.join(root, "launcher.pid")).read())
    reviewer = int(open(os.path.join(root, "reviewer.pid")).read())
    t0 = time.monotonic()
    os.kill(watcher.pid, signal.SIGTERM)
    try:
        watcher.wait(timeout=5)                     # the host's STOP_BOUND
        killed = False
    except subprocess.TimeoutExpired:
        os.killpg(watcher.pid, signal.SIGKILL)
        watcher.wait(timeout=3)
        killed = True
    took = time.monotonic() - t0
    for _ in range(20):                             # an exited reviewer is reaped by launchd, not by us
        if not exists(reviewer):
            break
        time.sleep(0.05)
    beat = open(os.path.join(root, "heartbeat")).read()
    time.sleep(0.3)
    advanced = beat != open(os.path.join(root, "heartbeat")).read()
    try:
        outcomes = [json.loads(l)["outcome"] for l in open(os.path.join(root, "attempts.jsonl"))]
    except OSError:
        outcomes = []
    print("%s launcher: watcher exit=%s after %.2fs (killed by the host: %s), launcher alive=%s, reviewer alive=%s, "
          "heartbeat advanced=%s, attempts=%s" % (mode, watcher.returncode, took, killed, exists(launcher),
                                                  exists(reviewer), advanced, outcomes))
    ok = not killed and not exists(launcher) and not exists(reviewer) and not advanced and outcomes == ["stopped"]
finally:
    for pid in (reviewer, launcher):
        if pid:
            try:
                os.killpg(pid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
    if watcher.poll() is None:
        os.killpg(watcher.pid, signal.SIGKILL)
        watcher.wait()
sys.exit(0 if ok else 1)
PY
for mode in responsive deaf; do
    W15OUT="$(python3 "$SB/w15.py" "$LIB" "$SB/w15-$mode" "$mode" 2>&1)"; W15RC=$?
    printf '    %s\n' "$W15OUT"
    check "W15 the host's quit ($mode launcher): stopped is recorded only once the review's group, reviewer included, is gone" \
        $W15RC "$W15OUT"
done

# --- W16 ---------------------------------------------------------------------
# Finding 2 (fixtures/shutdown_during_spawn.py): stop_own stopped only reviews whose pid was in
# their lock, and the pid reaches the lock after spawn returns; a SIGTERM in between (here inside
# the process-start lookup, and inside Popen itself after the fork) left a review that leads its
# own session running. The real spawn, start, run_host_loop and stop_own.
cat >"$SB/w16.py" <<'PY'
import json, os, shlex, signal, subprocess, sys, time
lib, root, window = sys.argv[1], sys.argv[2], sys.argv[3]
role = sys.argv[4] if len(sys.argv) > 4 else ""
sys.path.insert(0, lib)
if role == "review":
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    open(os.path.join(root, "review.pid"), "w").write(str(os.getpid()))
    while True:
        open(os.path.join(root, "heartbeat"), "w").write(str(time.monotonic()))
        time.sleep(0.05)

def started():
    deadline = time.monotonic() + 5
    while not os.path.exists(os.path.join(root, "heartbeat")):
        if time.monotonic() > deadline:
            raise RuntimeError("the fixture review did not start")
        time.sleep(0.01)

if role == "watcher":
    os.environ["REVIEW_WATCH_STATE_DIR"] = root
    os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "reviews")
    os.environ["REVIEW_WATCH_SECOND_REVIEW"] = os.path.join(root, "fake-review.sh")
    import review_watch as rw
    if window == "in-popen":
        real = subprocess.Popen

        class Interrupted(real):
            fired = False
            def __init__(self, *a, **k):
                real.__init__(self, *a, **k)
                if k.get("start_new_session") and not Interrupted.fired:
                    Interrupted.fired = True        # the fork is done; Popen has not returned yet
                    started()
                    os.kill(os.getpid(), signal.SIGTERM)
                    time.sleep(0.5)
        rw.subprocess.Popen = Interrupted

    class Interrupting(rw.Watcher):
        def process_start(self, pid):
            if window == "process-start":         # the real spawn's first call after the Popen
                started()
                os.kill(os.getpid(), signal.SIGTERM)
                time.sleep(0.5)
            return rw.Watcher.process_start(pid)
        def look(self, now, state):
            item = rw.Item("teammate:fixture", "fixture", root, "a" * 40, "b" * 40, "running")
            self.start(item, "long-job", now, 1)
            return []
    sys.exit(rw.run_host_loop(Interrupting("/fixture/engine", "", world=object()), "app"))

def exists(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
os.makedirs(root)
runner = "exec " + " ".join(shlex.quote(s) for s in [sys.executable, "-B", os.path.abspath(__file__), lib, root,
                                                      window, "review"])
open(os.path.join(root, "fake-review.sh"), "w").write("#!/bin/bash\n" + runner + "\n")
watcher = subprocess.Popen([sys.executable, "-B", os.path.abspath(__file__), lib, root, window, "watcher"],
                           start_new_session=True)
review, ok = None, False
try:
    try:
        watcher.wait(timeout=5)
        killed = False
    except subprocess.TimeoutExpired:
        os.killpg(watcher.pid, signal.SIGKILL)
        watcher.wait(timeout=3)
        killed = True
    review = int(open(os.path.join(root, "review.pid")).read())
    for _ in range(20):
        if not exists(review):
            break
        time.sleep(0.05)
    beat = open(os.path.join(root, "heartbeat")).read()
    time.sleep(0.3)
    advanced = beat != open(os.path.join(root, "heartbeat")).read()
    locks = [json.load(open(os.path.join(root, "locks", f))) for f in os.listdir(os.path.join(root, "locks"))
             if f.endswith(".lock")]
    try:
        outcomes = [json.loads(l)["outcome"] for l in open(os.path.join(root, "attempts.jsonl"))]
    except OSError:
        outcomes = []
    print("SIGTERM %s: watcher exit=%s (killed: %s), locks left=%s, review alive=%s, heartbeat advanced=%s, "
          "attempts=%s" % (window, watcher.returncode, killed, [l.get("pid") for l in locks], exists(review),
                           advanced, outcomes))
    ok = not killed and not locks and not exists(review) and not advanced and outcomes == ["stopped"]
finally:
    if review:
        try:
            os.killpg(review, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass
    if watcher.poll() is None:
        os.killpg(watcher.pid, signal.SIGKILL)
        watcher.wait()
sys.exit(0 if ok else 1)
PY
for window in process-start in-popen; do
    W16OUT="$(python3 "$SB/w16.py" "$LIB" "$SB/w16-$window" "$window" 2>&1)"; W16RC=$?
    printf '    %s\n' "$W16OUT"
    check "W16 a SIGTERM during spawn ($window): the partly registered review is stopped and settled as stopped" \
        $W16RC "$W16OUT"
done

# --- W17 ---------------------------------------------------------------------
# Finding 3 (fixtures/verdict_owner_collision.py): owner_session took the first item at the
# reviewed repository and tip, whatever its work, before the review's own record; once the worker
# moved on and another work held that commit, the verdict went to that other lead.
python3 - "$LIB" "$SB/w17" <<'PY'
import os, sys
lib, root = sys.argv[1:3]
os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, "rw")
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
sys.path.insert(0, lib)
import review_watch as rw
repo = os.path.realpath(os.path.join(root, "fictional-repository"))
old, new = "a" * 40, "b" * 40
row = {"repo": repo, "tip": old, "work": "teammate:lead-A--worker"}
owner = rw.Item(row["work"], "worker", repo, new, "c" * 40, "running")
owner.session = "lead-A"
other = rw.Item("teammate:lead-B--reviewer", "reviewer", repo, old, "c" * 40, "running")
other.session = "lead-B"
record = {"repo": repo, "tip": old, "work": row["work"], "session": "lead-A"}
got = {}
for label, running, attempts in (("running lock", {(repo, old): record}, []), ("settled attempt", {}, [record])):
    got[label] = rw.owner_session(row, [owner, other], rw.Book([], running, attempts), attempts)
got["no record, same work moved on"] = rw.owner_session(row, [other, owner], rw.Book([], {}, []), [])
got["no record, only another work at the tip"] = rw.owner_session(row, [other], rw.Book([], {}, []), [])
got["unique-tip control"] = rw.owner_session(row, [owner], rw.Book([], {}, [record]), [record])
print(got)
want = {"running lock": "lead-A", "settled attempt": "lead-A", "no record, same work moved on": "lead-A",
        "no record, only another work at the tip": "", "unique-tip control": "lead-A"}
sys.exit(0 if got == want else 1)
PY
check "W17 the recorded owner comes first, and a registry fallback needs the same work, never another work at the tip" \
    $? "see above"

# --- W18 ---------------------------------------------------------------------
# Second review of b5ff41f02, finding 1 (fixtures/fork_after_final_scan.py): the reviewer led a
# session of its own, so the watcher found it only by reading the process table, and one forked
# after its last read, before the SIGKILL, outlived the stop. Now the reviewer stays in its
# launcher's process group (second-review's run_bounded) and the watcher signals that group and
# waits for it whole, and freezes it (SIGSTOP) before it reads the process table, so no moment
# of forking escapes. The real second_review.run_bounded and _on_stop under the real
# Watcher.stop_all; the fork is asked for inside the watcher's own os.killpg calls. Two windows:
# "after-final-scan" (the reviewer's fixture: second-review cannot act on the stop, here SIGTERM
# and SIGHUP blocked, is asked to fork the moment before the SIGKILL and would be wedged 10 s
# before it could record the reviewer; a frozen launcher cannot fork at all) and "brief" (the
# fork just before the stop's first signal, Popen returning 0.3 s later, as a real one does).
cat >"$SB/w18.py" <<'PY'
import os, select, signal, subprocess, sys, time
lib, window = sys.argv[1], sys.argv[2]
role = sys.argv[3] if len(sys.argv) > 3 else ""
sys.path.insert(0, lib)
WEDGE = {"after-final-scan": 10.0, "brief": 0.3}[window]
FORK_AT = {"after-final-scan": (signal.SIGKILL,), "brief": (signal.SIGSTOP, signal.SIGTERM)}[window]

def line(stream, seconds=5, required=True):
    if not select.select([stream], [], [], seconds)[0]:
        if required:
            raise RuntimeError("fixture synchronization timed out")
        return ""
    return stream.readline().strip()

if role == "launcher":
    import second_review as sr
    for sig in (signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, sr._on_stop)
    if window == "after-final-scan":
        signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTERM, signal.SIGHUP})
    original = subprocess.Popen
    ready_read, ready_write = os.pipe()
    heartbeat_write = int(sys.argv[4])
    reviewer_code = ("import os,signal,sys,time\n"
                     "signal.signal(signal.SIGTERM,signal.SIG_IGN)\n"
                     "os.write(int(sys.argv[1]),b'R')\n"
                     "while True:\n"
                     " os.write(int(sys.argv[2]),b'H'); time.sleep(0.05)\n")

    def spawning(*args, **kwargs):
        print("before reviewer fork", flush=True)
        assert sys.stdin.readline().strip() == "fork"
        kwargs["pass_fds"] = (ready_write, heartbeat_write)
        child = original(*args, **kwargs)
        assert os.read(ready_read, 1) == b"R"
        print(child.pid, flush=True)
        time.sleep(WEDGE)                           # the launcher has not recorded its reviewer yet
        return child
    sr.subprocess.Popen = spawning
    sr.run_bounded([sys.executable, "-B", "-c", reviewer_code, str(ready_write), str(heartbeat_write)],
                   os.getcwd(), "", os.devnull, os.devnull, 60)
    sys.exit(0)

import review_watch as rw
heartbeat_read, heartbeat_write = os.pipe()
launcher = subprocess.Popen([sys.executable, "-B", os.path.abspath(__file__), lib, window, "launcher",
                             str(heartbeat_write)],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1,
                            start_new_session=True, pass_fds=(heartbeat_write,))
os.close(heartbeat_write)
reviewer, asked, ok = None, False, False


class ForkInsideTheStop(object):
    """review_watch's os, with the reviewer's fork asked for just before the first FORK_AT signal.
    A launcher that is frozen by then cannot answer: no reviewer is forked."""
    def __getattr__(self, name):
        return getattr(os, name)

    def killpg(self, pgid, sig):
        global reviewer, asked
        if sig in FORK_AT and not asked and pgid == launcher.pid:
            asked = True
            launcher.stdin.write("fork\n")
            launcher.stdin.flush()
            got = line(launcher.stdout, 0.5, required=False)   # a running launcher answers in milliseconds
            reviewer = int(got) if got else None
        return os.killpg(pgid, sig)


def exists(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
try:
    assert line(launcher.stdout) == "before reviewer fork"
    watcher = rw.Watcher("/fixture/engine", "", world=object())
    watcher.children[launcher.pid] = launcher
    rw.os = ForkInsideTheStop()
    remaining = watcher.stop_all([{"pid": launcher.pid}], rw.QUIT_TERM_SECONDS, rw.QUIT_KILL_SECONDS)
    rw.os = os
    launcher.wait(timeout=5)
    os.set_blocking(heartbeat_read, False)
    try:
        os.read(heartbeat_read, 4096)               # beats written before the stop ended
    except BlockingIOError:
        pass
    time.sleep(0.2)
    try:
        advanced = bool(os.read(heartbeat_read, 4096))   # b"" (end of file): every writer is gone
    except BlockingIOError:
        advanced = True                             # a writer is still there
    for _ in range(20):                             # a reviewer orphaned by the stop is reaped by launchd
        if reviewer is None or not exists(reviewer):
            break
        time.sleep(0.05)
    alive = reviewer is not None and exists(reviewer)
    print("reviewer fork asked %s (%s, launcher wedged %.1f s): asked=%s, forked=%s, launcher exit=%s, "
          "stop_all remaining=%s, reviewer heartbeat advanced=%s, reviewer alive=%s" % (
              "just before the SIGKILL" if window == "after-final-scan" else "just before the stop's first signal",
              window, WEDGE, asked, reviewer is not None, launcher.returncode, remaining, advanced, alive))
    # after-final-scan: the launcher, frozen or not, ends with the group SIGKILL (-9), and so does any
    # reviewer it forked; brief: the reviewer is forked, and ends with its review whatever the launcher's
    # exit (143 on its SIGTERM, or -9).
    ok = asked and remaining == [] and not advanced and not alive and (
        launcher.returncode == -9 if window == "after-final-scan" else
        reviewer is not None and launcher.returncode in (143, -9))
finally:
    rw.os = os
    for pid in (reviewer, launcher.pid):
        if pid:
            for kill in (os.killpg, os.kill):
                try:
                    kill(pid, signal.SIGKILL)
                except (ProcessLookupError, PermissionError):
                    pass
    launcher.wait(timeout=5)
    os.close(heartbeat_read)
sys.exit(0 if ok else 1)
PY
for window in after-final-scan brief; do
    W18OUT="$(python3 "$SB/w18.py" "$LIB" "$window" 2>&1)"; W18RC=$?
    printf '    %s\n' "$W18OUT"
    check "W18 a reviewer forked during a stop ($window) ends with its review's group: none left running" \
        $W18RC "$W18OUT"
done

# --- W19 ---------------------------------------------------------------------
# Second review of 783a8dba1, finding 2 (fixtures/repeated_verdict_owner_collision.py): the first
# notice went through owner_session, but the 30-minute reminder looked the verdict up by repository
# and tip and sent it through whichever ended item held that tip, on a clock shared by every work
# there: another work at the tip got the owner's reminder and suppressed the owner's own. The full
# tell(), with its state in memory.
python3 - "$LIB" "$SB/w19" <<'PY'
import json, os, sys
from unittest.mock import patch
lib, root = sys.argv[1:3]
os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, "rw")
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
sys.path.insert(0, lib)
import review_watch as rw
repo = os.path.realpath(os.path.join(root, "fictional-repository"))
tip = "a" * 40

def row(work, author):
    return {"repo": repo, "tip": tip, "work": work, "author": author, "verdict": "changes-requested",
            "trigger": "handover", "record": "", "findings": 1, "at": "2026-10-09T00:00:00Z"}

def item(work, name, session):
    it = rw.Item(work, name, repo, tip, "b" * 40, "ended", ref=name)
    it.session = session
    return it
A, B = "teammate:work-A", "teammate:work-B"
rw.HOST_JSON["on"] = True

def run(rows, items, attempts, fresh=True):
    """[[sessions told at the first look], [sessions told 30 minutes later]]."""
    state = {"rows": 0 if fresh else len(rows), "told": {}}
    book = rw.Book(rows, {}, attempts)
    out = []
    with patch.object(rw.stall_watch, "_read_json", return_value={}), patch.object(rw.stall_watch, "_write_json"):
        for t in (100, 100 + rw.REPEAT_MINUTES * 60):
            out.append([json.loads(s)["session"] for s in rw.tell(t, state, rows, book, items, [], attempts)])
            rw.deliver(rw.PRINTED["keys"], t)      # the host accepted it, as tick records it (W27)
    return out
got, want = {}, {}
owner_attempt = [{"repo": repo, "tip": tip, "work": A, "session": "lead-A", "outcome": "verdict"}]
# The reviewer's fixture: another ended work at the same tip comes first in the registry.
got["another work first at the tip"] = run([row(A, "worker-A")],
                                           [item(B, "reviewer-B", "lead-B"), item(A, "worker-A", "lead-A")],
                                           owner_attempt)
want["another work first at the tip"] = [["lead-A"], ["lead-A"]]
# A session whose first look comes after the verdict was told: the reminder loop tells it first.
got["first told by the reminder loop"] = run([row(A, "worker-A")],
                                             [item(B, "reviewer-B", "lead-B"), item(A, "worker-A", "lead-A")],
                                             owner_attempt, fresh=False)
want["first told by the reminder loop"] = [["lead-A"], ["lead-A"]]
# Only another work holds the tip now: nothing goes to its lead. The owner, lead-A, gets its own
# verdict once, never yet delivered to it (second review of b5ff41f02, finding 2: the operator host
# delivers an owner's undelivered verdict wherever the cursor is), and no reminder: its work has no
# item here.
got["only another work at the tip"] = run([row(A, "worker-A")], [item(B, "reviewer-B", "lead-B")], owner_attempt,
                                          fresh=False)
want["only another work at the tip"] = [["lead-A"], []]
# Two works at one tip, each with its own changes-requested verdict: each reminder reaches its own
# lead, and neither clock suppresses the other.
both = [row(A, "worker-A"), row(B, "reviewer-B")]
got["two works, two verdicts"] = run(both, [item(B, "reviewer-B", "lead-B"), item(A, "worker-A", "lead-A")],
                                     owner_attempt + [dict(owner_attempt[0], work=B, session="lead-B")])
want["two works, two verdicts"] = [["lead-A", "lead-B"], ["lead-B", "lead-A"]]
for k in want:
    print("%s: %s" % (k, got[k]))
sys.exit(0 if got == want else 1)
PY
check "W19 the repeated handover notice goes to the recorded owner of the same work, on that work's own clock" \
    $? "see above"

# --- W20 ---------------------------------------------------------------------
# The same class in the plain monitor (review-watch.sh --monitor inside each lead session): there a
# notice is delivered by printing it in the session whose monitor runs, so another lead's monitor
# printed every lead's verdicts. Now a monitor prints a block it owns, one with no owner, and one
# whose owner has no live monitor (that session ended), never one another live lead's monitor will
# print: since the second review of b5ff41f02 (finding 2) that includes a monitor that has not
# looked yet, whose first look tells it (W21).
python3 - "$LIB" "$SB/w20" <<'PY'
import fcntl, json, os, sys
lib, root = sys.argv[1:3]
os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, "rw")
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
sys.path.insert(0, lib)
import review_watch as rw
repo = os.path.realpath(os.path.join(root, "fictional-repository"))

def row(tip, work):
    return {"repo": repo, "tip": tip, "work": work, "author": work, "verdict": "passed", "trigger": "handover",
            "record": "", "findings": 0, "at": "2026-10-09T00:00:00Z"}
rows = [row("a" * 40, "teammate:of-A"), row("b" * 40, "teammate:of-B"), row("c" * 40, "teammate:of-gone"),
        row("d" * 40, "branch:by-hand")]
attempts = [{"repo": repo, "tip": r["tip"], "work": r["work"], "session": s, "outcome": "verdict"}
            for r, s in zip(rows, ("lead-A", "lead-B", "lead-gone", ""))]

def monitor(sid, looked):
    d = rw.session_dir(sid)
    fd = os.open(os.path.join(d, "monitor.lock"), os.O_RDWR | os.O_CREAT, 0o600)
    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    if looked:
        json.dump({"last_look": 1}, open(os.path.join(d, "told.json"), "w"))
    return fd
held = [monitor("lead-A", True), monitor("lead-B", True), monitor("lead-new", False)]
attempts.append({"repo": repo, "tip": "e" * 40, "work": "teammate:of-new", "session": "lead-new", "outcome": "verdict"})
rows.append(row("e" * 40, "teammate:of-new"))

def told(me):
    vars(rw).setdefault("MONITOR", {})["session"] = me    # what run_loop sets; a watcher without it prints all
    lines = rw.tell(100, {"rows": 0, "told": {}}, rows, rw.Book(rows, {}, attempts), [], [], attempts)
    return sorted(t for t in ("a", "b", "c", "d", "e") if any("@" + t * 12 in ln for ln in lines))
got = {"lead-A's monitor": told("lead-A"), "lead-B's monitor": told("lead-B"), "no session known": told("")}
want = {"lead-A's monitor": ["a", "c", "d"], "lead-B's monitor": ["b", "c", "d"],
        "no session known": ["a", "b", "c", "d", "e"]}
print(got)
sys.exit(0 if got == want else 1)
PY
check "W20 the plain monitor prints a verdict only in its owner's session, unless the owner has no live monitor" \
    $? "see above"

# --- W21 ---------------------------------------------------------------------
# Second review of b5ff41f02, finding 2 (fixtures/monitor_first_look_owner_loss.py): lead-A's
# monitor was live but had not looked yet, so lead-B's monitor printed lead-A's verdict and moved
# last-told.json past it, and lead-A's first look started there: lead-A never got its passed
# verdict, which has no reminder. Now only the owner (its own monitor, or the operator host on its
# behalf) records a verdict delivered, and a monitor's first look tells every verdict its session
# owns that was never delivered to it. The real tell(), owner_session and for_this_monitor over
# real state files and real monitor locks.
python3 - "$LIB" "$SB/w21" <<'PY'
import fcntl, os, sys
lib, root = sys.argv[1:3]
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
sys.path.insert(0, lib)
import review_watch as rw
repo = os.path.realpath(os.path.join(root, "fictional-repository"))
now = rw.parse_iso("2026-10-09T01:00:00Z")
row = {"id": "rv-of-A", "repo": repo, "tip": "a" * 40, "work": "teammate:lead-A--worker", "author": "worker-A",
       "verdict": "passed", "trigger": "handover", "record": "", "findings": 0, "at": "2026-10-09T00:30:00Z"}
attempts = [{"repo": repo, "tip": row["tip"], "work": row["work"], "session": "lead-A", "outcome": "verdict"}]
book = rw.Book([row], {}, attempts)


def monitor(sid):
    fd = os.open(os.path.join(rw.session_dir(sid), "monitor.lock"), os.O_RDWR | os.O_CREAT, 0o600)
    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    return fd


def look(me, state, t):
    rw.MONITOR["session"] = me
    told = any("[PASSED]" in ln for ln in rw.tell(t, state, [row], book, [], [], attempts))
    rw.deliver(rw.PRINTED["keys"], t)              # its output accepted, as tick records it (W27)
    return told


def scenario(name, a_live_first, host_first=False):
    os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, name)
    held = [monitor("lead-B")]
    got = {}
    if host_first:
        rw.HOST_JSON["on"] = True                   # the operator host sends the block to lead-A
        got["host"] = look("", {"rows": 0, "told": {}}, now)
        rw.HOST_JSON["on"] = False
    if a_live_first:
        held.append(monitor("lead-A"))              # lead-A's monitor is up and has not looked yet
    got["lead-B"] = look("lead-B", {"rows": 0, "told": {}}, now)   # lead-B has looked before
    if not a_live_first:
        held.append(monitor("lead-A"))              # lead-A's session comes back after lead-B looked
    a_state = {}
    got["lead-A first look"] = look("lead-A", a_state, now + 60)
    got["lead-A next look"] = look("lead-A", a_state, now + 120)
    for fd in held:
        os.close(fd)
    return got


got = {"owner live, not looked yet (the reviewer's fixture)": scenario("live", True),
       "owner without a live monitor, back later": scenario("gone", False),
       "delivered to the owner by the operator host": scenario("host", False, host_first=True)}
want = {"owner live, not looked yet (the reviewer's fixture)":
            {"lead-B": False, "lead-A first look": True, "lead-A next look": False},
        "owner without a live monitor, back later":
            {"lead-B": True, "lead-A first look": True, "lead-A next look": False},
        "delivered to the owner by the operator host":
            {"host": True, "lead-B": True, "lead-A first look": False, "lead-A next look": False}}
for k in want:
    print("%s: %s" % (k, got[k]))
sys.exit(0 if got == want else 1)
PY
check "W21 only a verdict's owner consumes it: the owner's first look still gets a verdict another lead's monitor saw" \
    $? "see above"

# --- W22 ---------------------------------------------------------------------
# The real second review of this change (rv-20261009T052943Z-4d99d0b7-c126,
# fixtures/owner_delivery_edges.py): (1) the operator host's first look did not recover a verdict
# another monitor's look had moved the shared cursor past, and the operator lead's only delivery is
# the host; (2) an owner's look marked every pending verdict delivered before the BLOCK_CHARS cap
# cut the printed block, so the verdicts past the cap were never told. Through the real tick().
python3 - "$LIB" "$SB/w22" <<'PY'
import io, os, sys
lib, root = sys.argv[1:3]
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
sys.path.insert(0, lib)
import review_watch as rw
now = rw.parse_iso("2026-10-09T01:00:00Z")


class Verdicts(object):
    """A watcher whose look is tell() over `count` passed verdicts, each owned by lead-A."""
    def __init__(self, count):
        repo = os.path.join(root, "fictional-repository")
        self.rows = [dict(id="rv-%d" % i, repo=repo, tip="%040x" % (i + 1), work="teammate:lead-A--worker-%d" % i,
                          author="worker-%d" % i, verdict="passed", trigger="handover", record="", findings=0,
                          at="2026-10-09T00:30:00Z") for i in range(count)]
        self.attempts = [dict(repo=repo, tip=r["tip"], work=r["work"], session="lead-A", outcome="verdict")
                         for r in self.rows]

    def look(self, t, state):
        return rw.tell(t, state, self.rows, rw.Book(self.rows, {}, self.attempts), [], [], self.attempts)


def look(watcher, sid, t, host=False):
    rw.MONITOR["session"] = "" if host else sid
    rw.HOST_JSON["on"] = host
    out = io.StringIO()
    rw.tick(watcher, rw.session_dir(sid), now=t, out=out)
    return out.getvalue()


def fresh(name):
    os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, name)
    rw.stall_watch._write_json(os.path.join(rw.session_dir("lead-B"), "told.json"), {"rows": 0, "told": {}})


got, want = {}, {}
fresh("host-late")                                  # lead-A is a print-mode lead: no monitor, the host delivers
w = Verdicts(1)
b, h1, h2 = look(w, "lead-B", now), look(w, "operator-host", now + 60, True), look(w, "operator-host", now + 120, True)
got["host after another monitor"] = {"lead-B": "[PASSED]" in b, "host first look": "[PASSED]" in h1,
                                     "host next look": "[PASSED]" in h2}
want["host after another monitor"] = {"lead-B": True, "host first look": True, "host next look": False}
fresh("backlog")                                    # sixteen verdicts: more than one capped block holds
w = Verdicts(16)
look(w, "lead-B", now)
told = [look(w, "lead-A", now + 60 * n).count("[PASSED]") for n in (1, 2, 3, 4)]
got["owner backlog past the cap"] = {"first look cut by the cap": 0 < told[0] < 16, "told in all": sum(told),
                                     "last look": told[-1]}
want["owner backlog past the cap"] = {"first look cut by the cap": True, "told in all": 16, "last look": 0}
for k in want:
    print("%s: %s" % (k, got[k]))
print("owner backlog, per look: %s" % told)
sys.exit(0 if got == want else 1)
PY
check "W22 the operator host recovers a verdict another monitor passed, and verdicts past the cap wait for the next look" \
    $? "see above"

# --- W23 ---------------------------------------------------------------------
# esc-20261009T054753Z-400c68b3 (measured on codex-cli 0.162.0-alpha.2): Codex runs each shell
# command in a process group of its own and does not end it when it gets SIGTERM, so a stop of the
# review's group alone left the command running ("codex rc=-15; its sleep still running 2 s
# later: True"). The host's quit through the real run_host_loop, stop_own and stop_all, over the real
# second_review.run_bounded and _on_stop, with a reviewer stand-in: "codex" (its tool command sits
# in a group of its own; SIGTERM ends the reviewer only) and "late-tool" (it ignores SIGTERM and
# starts one more tool command in a new group when it gets it).
cat >"$SB/w23.py" <<'PY'
import json, os, signal, subprocess, sys, time
lib, root, mode = sys.argv[1], sys.argv[2], sys.argv[3]
role = sys.argv[4] if len(sys.argv) > 4 else ""
sys.path.insert(0, lib)
TOOL = ("import os,sys,time\n"
        "open(os.path.join(sys.argv[1],'tool-%d.pid'%os.getpid()),'w').write(str(os.getpid()))\n"
        "while True:\n"
        "    open(os.path.join(sys.argv[1],'beat-%d'%os.getpid()),'w').write(str(time.monotonic()))\n"
        "    time.sleep(0.05)\n")
REVIEWER = ("import os,signal,subprocess,sys,time\n"
            "root, mode, tool = sys.argv[1], sys.argv[2], sys.argv[3]\n"
            "def start():\n"
            "    subprocess.Popen([sys.executable,'-B','-c',tool,root],start_new_session=True)\n"
            "if mode == 'late-tool':\n"
            "    signal.signal(signal.SIGTERM, lambda *a: start())\n"
            "start()\n"
            "open(os.path.join(root,'reviewer.pid'),'w').write(str(os.getpid()))\n"
            "while True:\n"
            "    time.sleep(0.05)\n")
if role == "launcher":
    import second_review as sr
    for sig in (signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, sr._on_stop)
    sr.run_bounded([sys.executable, "-B", "-c", REVIEWER, root, mode, TOOL], root, "", os.devnull, os.devnull, 60)
    sys.exit(0)
if role == "watcher":
    os.environ["REVIEW_WATCH_STATE_DIR"] = root
    os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "reviews")
    os.environ["REVIEW_WATCH_POLL_SECONDS"] = "0.1"
    import review_watch as rw

    class Quiet(rw.Watcher):
        told = False
        def look(self, now, session_state):
            if not self.told:
                self.told = True
                print("ready", flush=True)
            return []
    # Started as every review is (Watcher.start and spawn): a session of its own and its own mark.
    import shlex
    fake = os.path.join(root, "fake-second-review.sh")
    open(fake, "w").write("#!/bin/bash\nexec %s\n" % " ".join(shlex.quote(s) for s in [
        sys.executable, "-B", os.path.abspath(__file__), lib, root, mode, "launcher"]))
    os.environ["REVIEW_WATCH_SECOND_REVIEW"] = fake
    w = Quiet("/fixture/engine", "", world=object())
    w.start(rw.Item("teammate:fixture", "fixture", root, "a" * 40, "b" * 40, "running"), "long-job", time.time(), 1)
    deadline = time.monotonic() + 5
    while not os.path.exists(os.path.join(root, "reviewer.pid")):
        if time.monotonic() > deadline:
            raise SystemExit("the reviewer did not start")
        time.sleep(0.01)
    sys.exit(rw.run_host_loop(w, "app"))


def exists(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def tools():
    return sorted(int(f[5:-4]) for f in os.listdir(root) if f.startswith("tool-") and f.endswith(".pid"))


def beats():
    out = {}
    for f in os.listdir(root):
        if f.startswith("beat-"):
            try:
                out[f] = open(os.path.join(root, f)).read()
            except OSError:
                pass
    return out


os.makedirs(root)
watcher = subprocess.Popen([sys.executable, "-B", os.path.abspath(__file__), lib, root, mode, "watcher"],
                           stdout=subprocess.PIPE, text=True, start_new_session=True)
ok = False
try:
    assert watcher.stdout.readline().strip() == "ready"
    t0 = time.monotonic()
    os.kill(watcher.pid, signal.SIGTERM)
    try:
        watcher.wait(timeout=5)                     # the host's STOP_BOUND
        killed = False
    except subprocess.TimeoutExpired:
        os.killpg(watcher.pid, signal.SIGKILL)
        watcher.wait(timeout=3)
        killed = True
    took = time.monotonic() - t0
    time.sleep(0.3)
    b1 = beats()
    time.sleep(0.3)
    advanced = b1 != beats()
    alive = [t for t in tools() if exists(t)]
    try:
        outcomes = [json.loads(l)["outcome"] for l in open(os.path.join(root, "attempts.jsonl"))]
    except OSError:
        outcomes = []
    print("%s reviewer: watcher exit=%s after %.2fs (killed by the host: %s), tool commands started=%d, still "
          "running=%d, a tool heartbeat advanced=%s, attempts=%s" % (mode, watcher.returncode, took, killed,
                                                                   len(tools()), len(alive), advanced, outcomes))
    ok = not killed and tools() and not alive and not advanced and outcomes == ["stopped"]
finally:
    for t in tools():
        try:
            os.killpg(t, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass
    if watcher.poll() is None:
        os.killpg(watcher.pid, signal.SIGKILL)
        watcher.wait()
sys.exit(0 if ok else 1)
PY
for mode in codex late-tool; do
    W23OUT="$(python3 "$SB/w23.py" "$LIB" "$SB/w23-$mode" "$mode" 2>&1)"; W23RC=$?
    printf '    %s\n' "$W23OUT"
    check "W23 the host's quit ends the reviewer's own tool commands, each in a process group of its own ($mode)" \
        $W23RC "$W23OUT"
done

# --- W24 ---------------------------------------------------------------------
# The real second review of 6e8cc3f6d (rv-20261009T054743Z-6e8cc3f6-510d): (1) the first look under
# the owner-delivery rule marked every verdict the old shared cursor had passed as delivered, but
# that cursor also moved when another lead saw a notice, so an owner lost such a verdict at the
# upgrade; (2) a verdict never delivered expired after seven days, so an owner back after eight
# never got it. Now nothing is inferred from the old cursor, nothing undelivered expires, and a
# told verdict stays told for as long as it is in the ledger. Through the real tell().
python3 - "$LIB" "$SB/w24" <<'PY'
import fcntl, os, sys
lib, root = sys.argv[1:3]
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
sys.path.insert(0, lib)
import review_watch as rw
now = rw.parse_iso("2026-10-20T01:00:00Z")
repo = os.path.realpath(os.path.join(root, "fictional-repository"))


def ledger(at):
    rows = [{"id": "rv-of-A", "repo": repo, "tip": "a" * 40, "work": "teammate:lead-A--worker", "author": "worker-A",
             "verdict": "passed", "trigger": "quiet", "record": "", "findings": 0, "at": at}]
    att = [{"repo": repo, "tip": "a" * 40, "work": rows[0]["work"], "session": "lead-A", "outcome": "verdict"}]
    return rows, att


def look(me, state, t, rows, att, host=False):
    rw.MONITOR["session"] = "" if host else me
    rw.HOST_JSON["on"] = host
    out = rw.tell(t, state, rows, rw.Book(rows, {}, att), [], [], att)
    rw.deliver(rw.PRINTED["keys"], t)              # its output accepted, as tick records it (W27)
    rw.HOST_JSON["on"] = False
    return any("[PASSED]" in ln for ln in out)


def fresh(name):
    os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, name)
    fd = os.open(os.path.join(rw.session_dir("lead-A"), "monitor.lock"), os.O_RDWR | os.O_CREAT, 0o600)
    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    return fd


got, want = {}, {}
# (1) State written before the rule: the old shared cursor is past lead-A's verdict, no `delivered`.
fd = fresh("upgrade")
rows, att = ledger("2026-10-19T23:00:00Z")
rw.stall_watch._write_json(rw._p("last-told.json"), {"rows": 1})
got["upgrade, cursor already past"] = [look("lead-A", st, now + n, rows, att) for st in [{}] for n in (0, 60)]
want["upgrade, cursor already past"] = [True, False]
os.close(fd)
# (2) Another lead's monitor moved the cursor; lead-A comes back eight days after the verdict.
for host in (False, True):
    fd = fresh("eight-days-%s" % ("host" if host else "monitor"))
    rows, att = ledger("2026-10-12T00:00:00Z")
    look("lead-B", {"rows": 0, "told": {}}, now, rows, att)
    st = {}
    k = "back after eight days (%s)" % ("operator host" if host else "its monitor")
    got[k] = [look("lead-A", st, now + 60, rows, att, host), look("lead-A", st, now + 120, rows, att, host)]
    want[k] = [True, False]
    os.close(fd)
# (3) Told once, it stays told: weeks later a fresh session state of the owner gets nothing.
fd = fresh("stays-told")
rows, att = ledger("2026-10-19T23:00:00Z")
first = look("lead-A", {}, now, rows, att)
got["told, then a fresh state three weeks later"] = [first, look("lead-A", {}, now + 21 * 86400, rows, att)]
want["told, then a fresh state three weeks later"] = [True, False]
os.close(fd)
for k in want:
    print("%s: %s" % (k, got[k]))
sys.exit(0 if got == want else 1)
PY
check "W24 nothing undelivered is marked told at the upgrade or expires; a told verdict stays told" $? "see above"

# --- W25, W26 ----------------------------------------------------------------
# The real second review of 0d83e456d (rv-20261009T060535Z-0d83e456-4819), findings 1 and 2: a
# stop found the review's processes by walking ancestry and process groups, so (W25,
# fixtures/background_tool_quit.py) a tool's background child, reparented to PID 1 once its
# launching shell returned, survived the quit, and (W26, fixtures/timeout_then_quit.py) after
# run_bounded's time limit the launcher's exit dropped the lock while the reviewer's tool ran on.
# Through the real Watcher.start, spawn, reconcile and stop_own, the real second_review.run_bounded
# and _on_stop, and a review stand-in that second-review's place runs (REVIEW_WATCH_SECOND_REVIEW).
cat >"$SB/w25.py" <<'PY'
import json, os, shlex, signal, subprocess, sys, time
lib, root, case = sys.argv[1], sys.argv[2], sys.argv[3]
role = sys.argv[4] if len(sys.argv) > 4 else ""
sys.path.insert(0, lib)
HERE = os.path.abspath(__file__)


def me(*more):
    return [sys.executable, "-B", HERE, lib, root, case] + list(more)


def beat(name):
    end = time.monotonic() + 60
    while time.monotonic() < end:
        tmp = os.path.join(root, name + ".tmp")
        json.dump({"pid": os.getpid(), "ppid": os.getppid(), "pgid": os.getpgid(0), "beat": time.monotonic()},
                  open(tmp, "w"))
        os.replace(tmp, os.path.join(root, name + ".json"))
        time.sleep(0.03)
    sys.exit(0)


quiet = dict(stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
if role == "background":
    beat("background")
if role == "tool":                                  # a tool command: starts a background task, returns
    subprocess.Popen(me("background"), **quiet)
    sys.exit(0)
if role == "loop":
    beat("tool")
if role == "reviewer":
    if case == "background":                        # the tool in a session of its own, as Codex runs it
        rc = subprocess.Popen(me("tool"), start_new_session=True).wait()
        json.dump({"exit": rc}, open(os.path.join(root, "tool-returned.json"), "w"))
    else:                                           # the tool in the launcher's own group
        subprocess.Popen(me("loop"), **quiet)
    time.sleep(60)
    sys.exit(0)
if role == "launcher":
    import second_review as sr
    for sig in (signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, sr._on_stop)
    rc, _s = sr.run_bounded(me("reviewer"), root, "", os.devnull, os.devnull, 60 if case == "background" else 1.0)
    json.dump({"timed_out": rc is None}, open(os.path.join(root, "launcher-done.json"), "w"))
    sys.exit(0)


def read(name):
    try:
        return json.load(open(os.path.join(root, name + ".json")))
    except (OSError, ValueError):
        return None


def exists(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def until(what, seconds=10):
    end = time.monotonic() + seconds
    while not what():
        if time.monotonic() > end:
            raise RuntimeError("the fixture did not get there")
        time.sleep(0.02)


os.makedirs(root)
os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, "rw")
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
fake = os.path.join(root, "fake-second-review.sh")
open(fake, "w").write("#!/bin/bash\nexec %s\n" % " ".join(shlex.quote(s) for s in me("launcher")))
os.environ["REVIEW_WATCH_SECOND_REVIEW"] = fake
import review_watch as rw
w = rw.Watcher("/fixture/engine", "", world=object())
info = w.start(rw.Item("teammate:fixture", "fixture", root, "a" * 40, "b" * 40, "running"), "long-job", time.time(), 1)
leader, ok = info["pid"], False
watched = "background" if case == "background" else "tool"
try:
    if case == "background":
        until(lambda: read("tool-returned") and (read("background") or {}).get("ppid") == 1)
        got = {"tool shell exit": read("tool-returned")["exit"], "background parent": read("background")["ppid"],
               "background in the tool's group": read("background")["pgid"] != os.getpgid(leader)}
        t0 = time.monotonic()
        w.stop_own(time.time())
    else:
        until(lambda: read("tool") and read("launcher-done") and w.children[leader].poll() is not None)
        got = {"reviewer timed out": read("launcher-done")["timed_out"], "launcher exit": w.children[leader].returncode,
               "tool in the launcher's group": read("tool")["pgid"] == leader}
        t0 = time.monotonic()
        w.reconcile(time.time(), [])               # the look after the launcher exited
        w.stop_own(time.time())                     # then the quit
    got["stop took under 5 s"] = time.monotonic() - t0 < 5
    pid = read(watched)["pid"]
    for _ in range(20):                             # an orphan is reaped by launchd, not by us
        if not exists(pid):
            break
        time.sleep(0.05)
    b1 = read(watched)
    time.sleep(0.3)
    got["%s still running" % watched] = exists(pid)
    got["%s heartbeat advanced" % watched] = read(watched) != b1
    got["attempts"] = [a["outcome"] for a in rw.read_jsonl(rw._p("attempts.jsonl"))]
    got["locks left"] = len([f for f in os.listdir(rw._p("locks")) if f.endswith(".lock")])
    print("%s: %s" % (case, json.dumps(got, sort_keys=True)))
    ok = (got["%s still running" % watched] is False and got["%s heartbeat advanced" % watched] is False
          and got["locks left"] == 0 and got["stop took under 5 s"]
          and got["attempts"] == (["stopped"] if case == "background" else ["lost"]))
finally:
    for name in ("background", "tool"):
        r = read(name)
        if r:
            try:
                os.kill(r["pid"], signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
    try:
        os.killpg(leader, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass
    child = w.children.get(leader)
    if child is not None and child.poll() is None:
        child.wait(timeout=3)
sys.exit(0 if ok else 1)
PY
W25OUT="$(python3 "$SB/w25.py" "$LIB" "$SB/w25" background 2>&1)"; W25RC=$?
printf '    %s\n' "$W25OUT"
check "W25 the quit ends a tool's background task its launching shell left behind, reparented to PID 1" $W25RC "$W25OUT"
W26OUT="$(python3 "$SB/w25.py" "$LIB" "$SB/w26" timeout 2>&1)"; W26RC=$?
printf '    %s\n' "$W26OUT"
check "W26 past run_bounded's time limit, the launcher's exit settles its lock only once the reviewer's tool is gone" \
    $W26RC "$W26OUT"

# --- W27 ---------------------------------------------------------------------
# The real second review of 0d83e456d, finding 3 (fixtures/delivery_ack_gap.py): tell recorded a
# verdict delivered before tick wrote it, so a host pipe that failed at quit lost a passed verdict
# for good. Now a verdict is delivered only once its output is accepted: written and flushed, or,
# for the operator host that says it acknowledges (RICHOS_REVIEW_WATCH_ACKS=1), acknowledged on the
# watcher's stdin once the host sent it to the lead. Through the real tick and tell.
python3 - "$LIB" "$SB/w27" <<'PY'
import io, json, os, sys
lib, root = sys.argv[1:3]
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
os.environ.pop("RICHOS_REVIEW_WATCH_ACKS", None)
sys.path.insert(0, lib)
import review_watch as rw


class BrokenPipe(object):
    def write(self, text):
        raise BrokenPipeError("the host closed the pipe during quit")

    def flush(self):
        pass


now = rw.parse_iso("2026-10-09T07:00:00Z")
row = {"id": "review-A", "repo": os.path.join(root, "fictional-repository"), "tip": "a" * 40, "work": "teammate:A",
       "author": "worker-A", "trigger": "quiet", "verdict": "passed", "at": rw.iso(now), "record": "", "findings": 0}
attempt = {"repo": row["repo"], "tip": row["tip"], "work": row["work"], "session": "lead-A", "outcome": "verdict"}


class Watcher(object):
    def look(self, t, state):
        return rw.tell(t, state, [row], rw.Book([row], {}, [attempt]), [], [], [attempt])


def look(sd, t, out=None):
    out = out or io.StringIO()
    try:
        rw.tick(Watcher(), sd, now=t, out=out)
    except BrokenPipeError:
        return "broken pipe"
    return out.getvalue()


got, want = {}, {}
for name, host, acks in (("monitor", False, False), ("host", True, False), ("host, acknowledging", True, True)):
    os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, name)
    rw.HOST_JSON["on"], rw.MONITOR["session"] = host, ("" if host else "lead-A")
    if acks:
        os.environ["RICHOS_REVIEW_WATCH_ACKS"] = "1"
        r, w = os.pipe()
        rw.ACKS["fd"] = r
    sd = rw.session_dir("operator-host" if host else "lead-A")
    seq = [look(sd, now, BrokenPipe())]
    seq += [look(sd, now + 60 * n) for n in (1, 2)]
    g = {"the host pipe failed": seq[0] == "broken pipe", "the next look tells it": "[PASSED]" in seq[1],
         "the look after that": "[PASSED]" in seq[2]}
    if acks:
        keys = [k for ln in seq[2].splitlines() for k in json.loads(ln).get("keys", [])]
        os.write(w, (json.dumps({"ack": keys}) + "\n").encode())
        g["acknowledged, the next look"] = "[PASSED]" in look(sd, now + 180)
        os.environ.pop("RICHOS_REVIEW_WATCH_ACKS")
    got[name] = g
    want[name] = {"the host pipe failed": True, "the next look tells it": True, "the look after that": acks}
    if acks:
        want[name]["acknowledged, the next look"] = False
for k in want:
    print("%s: %s" % (k, got[k]))
sys.exit(0 if got == want else 1)
PY
check "W27 a verdict is delivered only once the host accepts it: a failed pipe or no acknowledgment leaves it for the next look" \
    $? "see above"

# --- W28 ---------------------------------------------------------------------
# The real second review of 283b4379d, finding 2 (fixtures/mid_job_failed_output.py): the app's
# worker hook recorded a mid-job verdict delivered before it printed the notice, so a hook whose
# output pipe had closed lost it for good. Now it is recorded only once the hook's answer is written
# and flushed. Through the real scripts/app-engine-hook.py handle; scope, receipts and the unrelated
# pre-tool controls are stand-ins, as in the reviewer's fixture.
python3 - "$ENGINE" "$SB/w28" <<'PY'
import fcntl, importlib.util, io, json, os, sys
from pathlib import Path
from types import SimpleNamespace
engine, td = Path(sys.argv[1]), sys.argv[2]
os.makedirs(td)
spec = importlib.util.spec_from_file_location("w28_hook", engine / "scripts/app-engine-hook.py")
hook = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hook)
review = hook.load("w28_review", engine / "scripts/lib/app_review.py")


class ClosedPipe(io.StringIO):
    def write(self, text):
        raise BrokenPipeError("the host has closed its hook pipe")


paths = review.app_paths(td)
record = Path(td) / "verdict"
record.mkdir()
(record / "verdict.json").write_text(json.dumps({"answer": {"findings": [
    {"priority": 2, "title": "Planted defect", "files": ["result.txt:1"], "evidence": "missing result"}]}}))
Path(paths["ledger"]).parent.mkdir(parents=True)
Path(paths["ledger"]).write_text(json.dumps({"id": "rv-pending", "repo": td, "branch": "cc/worker", "trigger": "long-job",
                                             "verdict": "changes-requested", "tip": "a" * 40, "record": str(record)}) + "\n")
os.environ["RICHOS_APP_STATE"] = os.environ["RICHOS_ENTITY_ROOT"] = td
work = SimpleNamespace(validate_shell_target=lambda p: None, worker_context=lambda a, p: None,
                       worker_spaces=lambda a, p: [(td, "cc/worker")])
real_load = hook.load
hook.load = lambda name, path: (SimpleNamespace(capture=lambda p, *a, **kw: p) if name == "richos_app_evidence"
                                else work if name == "richos_desktop_work" else real_load(name, path))
hook.scope = lambda: {"actions_allowed": True}
hook.run = lambda *a, **kw: None
payload = {"hook_event_name": "PreToolUse", "tool_name": "Bash", "agent_id": "worker"}


def call(out):
    real, sys.stdout = sys.stdout, out
    try:
        hook.handle(payload)
        return out.getvalue()
    except BrokenPipeError:
        return "broken pipe"
    finally:
        sys.stdout = real


delivered = Path(paths["delivered"]) / "rv-pending"
got = {"the hook's output failed": call(ClosedPipe()) == "broken pipe",
       "recorded delivered after the failure": delivered.exists()}
held = os.open(os.path.join(paths["watch"], "delivered.lock"), os.O_WRONLY | os.O_CREAT, 0o600)
fcntl.flock(held, fcntl.LOCK_EX)
got["a call while another tells it says nothing"] = "Planted defect" not in call(io.StringIO())
os.close(held)
got["the next hook tells it"] = "Planted defect" in call(io.StringIO())
got["recorded delivered once told"] = delivered.exists()
got["the hook after that tells it again"] = "Planted defect" in call(io.StringIO())
print("    %s" % json.dumps(got, sort_keys=True))
sys.exit(0 if got == {"the hook's output failed": True, "recorded delivered after the failure": False,
                      "a call while another tells it says nothing": True, "the next hook tells it": True,
                      "recorded delivered once told": True, "the hook after that tells it again": False} else 1)
PY
check "W28 a mid-job verdict is recorded delivered only once the worker hook's answer is written and flushed" $? "see above"

# --- W28b --------------------------------------------------------------------
# The second review of 0b0bbed37 (fixtures/mid_job_partial_delivery.py): two notices were written and
# flushed, then creating the second delivered marker failed (disk full), which exited the hook 2 and
# discarded the output while the first verdict stayed marked. Once the output is written, a
# bookkeeping error must not fail the hook: exit 0, mark what can be marked, leave the rest pending.
python3 - "$ENGINE" "$SB/w28b" <<'PY2'
import contextlib, errno, importlib.util, io, json, os, sys
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
engine, td = Path(sys.argv[1]), sys.argv[2]
os.makedirs(td)
spec = importlib.util.spec_from_file_location("w28b_hook", engine / "scripts/app-engine-hook.py")
hook = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hook)
review = hook.load("w28b_review", engine / "scripts/lib/app_review.py")
paths = review.app_paths(td)
record = Path(td) / "verdict"
record.mkdir()
(record / "verdict.json").write_text(json.dumps({"answer": {"summary": "Fix this defect"}}))
Path(paths["ledger"]).parent.mkdir(parents=True)
Path(paths["ledger"]).write_text("".join(json.dumps({"id": rid, "repo": td, "branch": "cc/worker",
    "trigger": "long-job", "verdict": "changes-requested", "tip": t * 40, "record": str(record)}) + "\n"
    for rid, t in (("rv-first", "a"), ("rv-second", "b"))))
work = SimpleNamespace(validate_shell_target=lambda p: None, worker_context=lambda a, p: None,
                       worker_spaces=lambda a, p: [(td, "cc/worker")])
def load(name, path):
    if name == "richos_app_evidence":
        return SimpleNamespace(capture=lambda p, *a, **kw: p)
    if name == "richos_desktop_work":
        return work
    if name == "richos_app_review":
        return review
    raise AssertionError(name)
payload = {"hook_event_name": "PreToolUse", "tool_name": "Bash", "agent_id": "worker"}
real_open = os.open
def full_on_second(path, *a, **kw):
    if str(path) == str(Path(paths["delivered"]) / "rv-second"):
        raise OSError(errno.ENOSPC, "No space left on device")
    return real_open(path, *a, **kw)
def call():
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        try:
            hook.handle(payload)
            return 0, out.getvalue()
        except Exception:
            return 2, out.getvalue()
with patch.dict(os.environ, {"RICHOS_APP_STATE": td, "RICHOS_ENTITY_ROOT": td}), \
        patch.object(hook, "load", load), patch.object(hook, "scope", lambda: {"actions_allowed": True}), \
        patch.object(hook, "run", lambda *a, **kw: None):
    with patch.object(os, "open", full_on_second):
        code, text = call()
    d = Path(paths["delivered"])
    first_marked, second_marked = (d / "rv-first").exists(), (d / "rv-second").exists()
    retry_code, retry = call()
got = {"first hook exit": code, "both told": "aaaaaaaaaaaa" in text and "bbbbbbbbbbbb" in text,
       "first marked": first_marked, "second marked": second_marked,
       "retry exit": retry_code, "retry tells only the second": "bbbbbbbbbbbb" in retry and "aaaaaaaaaaaa" not in retry}
print("    %s" % json.dumps(got, sort_keys=True))
sys.exit(0 if got == {"first hook exit": 0, "both told": True, "first marked": True, "second marked": False,
                      "retry exit": 0, "retry tells only the second": True} else 1)
PY2
check "W28b a failure marking a verdict delivered after the notices are written never fails the hook; the unmarked one stays pending" $? "see above"

# --- W28c --------------------------------------------------------------------
# The second review of 5a8857234: the marker failure plus a really broken stderr pipe, in a real
# process (the interpreter's shutdown flush is what exited 120). Setup as W28b: two notices were written and
# flushed, then creating the second delivered marker failed (disk full), which exited the hook 2 and
# discarded the output while the first verdict stayed marked. Once the output is written, a
# bookkeeping error must not fail the hook: exit 0, mark what can be marked, leave the rest pending.
mkdir -p "$SB/w28c-bin"
cat > "$SB/w28c-bin/w28c.py" <<'PY2'
import contextlib, subprocess, errno, importlib.util, io, json, os, sys
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
engine, td = Path(sys.argv[1]), sys.argv[2]
child = len(sys.argv) > 3
os.makedirs(td, exist_ok=True)
spec = importlib.util.spec_from_file_location("w28c_hook", engine / "scripts/app-engine-hook.py")
hook = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hook)
review = hook.load("w28c_review", engine / "scripts/lib/app_review.py")
paths = review.app_paths(td)
record = Path(td) / "verdict"
if not child:
  record.mkdir()
  (record / "verdict.json").write_text(json.dumps({"answer": {"summary": "Fix this defect"}}))
  Path(paths["ledger"]).parent.mkdir(parents=True)
  Path(paths["ledger"]).write_text("".join(json.dumps({"id": rid, "repo": td, "branch": "cc/worker",
    "trigger": "long-job", "verdict": "changes-requested", "tip": t * 40, "record": str(record)}) + "\n"
    for rid, t in (("rv-first", "a"), ("rv-second", "b"))))
work = SimpleNamespace(validate_shell_target=lambda p: None, worker_context=lambda a, p: None,
                       worker_spaces=lambda a, p: [(td, "cc/worker")])
def load(name, path):
    if name == "richos_app_evidence":
        return SimpleNamespace(capture=lambda p, *a, **kw: p)
    if name == "richos_desktop_work":
        return work
    if name == "richos_app_review":
        return review
    raise AssertionError(name)
payload = {"hook_event_name": "PreToolUse", "tool_name": "Bash", "agent_id": "worker"}
real_open = os.open
def full_on_second(path, *a, **kw):
    if str(path) == str(Path(paths["delivered"]) / "rv-second"):
        raise OSError(errno.ENOSPC, "No space left on device")
    return real_open(path, *a, **kw)
def call():
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        try:
            hook.handle(payload)
            return 0, out.getvalue()
        except Exception:
            return 2, out.getvalue()
with patch.dict(os.environ, {"RICHOS_APP_STATE": td, "RICHOS_ENTITY_ROOT": td}), \
        patch.object(hook, "load", load), patch.object(hook, "scope", lambda: {"actions_allowed": True}), \
        patch.object(hook, "run", lambda *a, **kw: None):
    if child:
        with patch.object(os, "open", full_on_second):
            hook.handle(payload)
        sys.exit(0)    # a normal interpreter exit: stderr is flushed at shutdown
    r, w = os.pipe()
    os.close(r)    # a really broken stderr: every write gets EPIPE
    proc = subprocess.run([sys.executable, __file__, str(engine), td, "child"], stdout=subprocess.PIPE,
                          stderr=w, text=True, env=dict(os.environ))
    os.close(w)
    code, text = proc.returncode, proc.stdout
    d = Path(paths["delivered"])
    first_marked, second_marked = (d / "rv-first").exists(), (d / "rv-second").exists()
    retry_code, retry = call()
got = {"first hook exit": code, "both told": "aaaaaaaaaaaa" in text and "bbbbbbbbbbbb" in text,
       "first marked": first_marked, "second marked": second_marked,
       "retry exit": retry_code, "retry tells only the second": "bbbbbbbbbbbb" in retry and "aaaaaaaaaaaa" not in retry}
print("    %s" % json.dumps(got, sort_keys=True))
sys.exit(0 if got == {"first hook exit": 0, "both told": True, "first marked": True, "second marked": False,
                      "retry exit": 0, "retry tells only the second": True} else 1)
PY2
python3 "$SB/w28c-bin/w28c.py" "$ENGINE" "$SB/w28c"
check "W28c a marker failure with a closed stderr never fails the hook" $? "see above"

# --- W29 ---------------------------------------------------------------------
# The real second review of 283b4379d, finding 1 (fixtures/orphan_platform_same_session.py): once
# the launcher exited, a member of the review's own session that hides the mark (an Apple platform
# binary such as /bin/sleep: the kernel withholds its environment) was no longer the review's, so a
# quit settled the lock as stopped and the tool ran on. Now the session stays the review's until a
# read finds it empty: at the quit of the watcher that started it, and at a later watcher's look
# (the lock's own_session). The environment is unreadable here for every process, as for a
# platform binary on macOS, so only the session can find the tool.
python3 - "$LIB" "$SB/w29" <<'PY'
import json, os, shlex, signal, subprocess, sys, time
lib, root = sys.argv[1:3]
os.makedirs(root)
os.environ["REVIEW_WATCH_STATE_DIR"] = root
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "reviews")
sys.path.insert(0, lib)
import review_watch as rw
rw.environment = lambda pid: []                 # every mark hidden, as a platform binary's is
launcher = os.path.join(root, "launcher.py")
with open(launcher, "w") as f:
    f.write("import subprocess, sys, time\np = subprocess.Popen(['/bin/sleep', '40'])\n"
            "open(sys.argv[1], 'w').write(str(p.pid))\ntime.sleep(0.3)\n")
tools, got = [], {}


def exists(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def started(case, tip):
    w = rw.Watcher("/fixture/engine", "", world=object())
    fake = os.path.join(root, "second-review-%s.sh" % case)
    with open(fake, "w") as f:
        f.write("exec %s\n" % " ".join(map(shlex.quote, [sys.executable, "-B", launcher, os.path.join(root, case)])))
    os.environ["REVIEW_WATCH_SECOND_REVIEW"] = fake
    info = w.start(rw.Item("teammate:fixture", "fixture", root, tip, "b" * 40, "running"), "long-job", time.time(), 1)
    end = time.monotonic() + 5
    while not os.path.exists(os.path.join(root, case)) or not open(os.path.join(root, case)).read():
        assert time.monotonic() < end
        time.sleep(0.01)
    tool = int(open(os.path.join(root, case)).read())
    tools.append(tool)
    w.children[info["pid"]].wait(timeout=5)     # the launcher has exited
    return w, info, tool


def gone(pid):
    for _ in range(40):                         # an orphan is reaped by launchd, not by us
        if not exists(pid):
            return True
        time.sleep(0.05)
    return False


try:
    w, info, tool = started("quit", "a" * 40)
    got["quit: the tool is in the review's session"] = os.getsid(tool) == info["pid"]
    w.stop_own(time.time())
    got["quit: the tool is ended"] = gone(tool)
    got["quit: lock left"] = os.path.exists(rw.lock_path(root, "a" * 40))
    w, info, tool = started("look", "c" * 40)
    got["look: recorded as leading its own session"] = rw.stall_watch._read_json(rw.lock_path(root, "c" * 40)).get("own_session")
    rw.Watcher("/fixture/engine", "", world=object()).reconcile(time.time(), [])   # another watcher's look
    got["look: the tool is ended"] = gone(tool)
    got["look: lock left"] = os.path.exists(rw.lock_path(root, "c" * 40))
    got["attempts"] = [a["outcome"] for a in rw.read_jsonl(rw._p("attempts.jsonl"))]
finally:
    for t in tools:
        try:
            os.kill(t, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass
print("    %s" % json.dumps(got, sort_keys=True))
sys.exit(0 if got == {"quit: the tool is in the review's session": True, "quit: the tool is ended": True,
                      "quit: lock left": False, "look: recorded as leading its own session": True,
                      "look: the tool is ended": True, "look: lock left": False,
                      "attempts": ["stopped", "lost"]} else 1)
PY
check "W29 a review's own session stays its own after its launcher exits: a tool that hides the mark is ended before the lock is settled" \
    $? "see above"

# --- W30 ---------------------------------------------------------------------
# The real second review of 16c154f5a, finding 1 (fixtures/process_table_failure.py): a ps timeout
# made owned_processes answer an empty table, read as an empty session, so the quit recorded the
# review stopped and settled its lock while its tool ran on. Now an unreadable table is unknown,
# never empty: one failed read and the next read still finds and ends the tool; a table that
# cannot be read at all leaves the lock unsettled, and the next look that can read it ends the tool
# and settles it. The real ps, with its "ps -A" reads failing as injected; every mark hidden.
python3 - "$LIB" "$SB/w30" <<'PY'
import json, os, shlex, signal, subprocess, sys, time
lib, root = sys.argv[1:3]
os.makedirs(root)
os.environ["REVIEW_WATCH_STATE_DIR"] = root
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "reviews")
sys.path.insert(0, lib)
import review_watch as rw
rw.environment = lambda pid: []                 # every mark hidden, as a platform binary's is
launcher = os.path.join(root, "launcher.py")
with open(launcher, "w") as f:
    f.write("import subprocess, sys, time\np = subprocess.Popen(['/bin/sleep', '40'])\n"
            "open(sys.argv[1], 'w').write(str(p.pid))\ntime.sleep(0.3)\n")
real_run, tools, got = subprocess.run, [], {}


def exists(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def gone(pid):
    for _ in range(40):                         # an orphan is reaped by launchd, not by us
        if not exists(pid):
            return True
        time.sleep(0.05)
    return False


def failing(reads):
    """subprocess.run whose first `reads` process-table reads time out (None: every one)."""
    n = [0]
    def run(argv, **kw):
        if argv[:3] == ["ps", "-A", "-o"] and (reads is None or n[0] < reads):
            n[0] += 1
            raise subprocess.TimeoutExpired(argv, 1)
        return real_run(argv, **kw)
    return run


def started(case, tip):
    w = rw.Watcher("/fixture/engine", "", world=object())
    fake = os.path.join(root, "second-review-%s.sh" % case)
    with open(fake, "w") as f:
        f.write("exec %s\n" % " ".join(map(shlex.quote, [sys.executable, "-B", launcher, os.path.join(root, case)])))
    os.environ["REVIEW_WATCH_SECOND_REVIEW"] = fake
    info = w.start(rw.Item("teammate:fixture", "fixture", root, tip, "b" * 40, "running"), "long-job", time.time(), 1)
    end = time.monotonic() + 5
    while not os.path.exists(os.path.join(root, case)) or not open(os.path.join(root, case)).read():
        assert time.monotonic() < end
        time.sleep(0.01)
    tool = int(open(os.path.join(root, case)).read())
    tools.append(tool)
    w.children[info["pid"]].wait(timeout=5)     # the launcher has exited
    return w, tool


def outcomes():
    return [a["outcome"] for a in rw.read_jsonl(rw._p("attempts.jsonl"))]


try:
    w, tool = started("once", "a" * 40)
    rw.subprocess.run = failing(1)
    try:
        w.stop_own(time.time())
    finally:
        rw.subprocess.run = real_run
    got["one failed read: the tool is ended"] = gone(tool)
    got["one failed read: lock left"] = os.path.exists(rw.lock_path(root, "a" * 40))
    got["one failed read: attempts"] = outcomes()
    w, tool = started("never", "c" * 40)
    rw.subprocess.run = failing(None)
    try:
        w.stop_own(time.time())
    finally:
        rw.subprocess.run = real_run
    got["unreadable: lock left"] = os.path.exists(rw.lock_path(root, "c" * 40))
    got["unreadable: attempts"] = outcomes()
    rw.Watcher("/fixture/engine", "", world=object()).reconcile(time.time(), [])   # the next look reads it
    got["next look: the tool is ended"] = gone(tool)
    got["next look: lock left"] = os.path.exists(rw.lock_path(root, "c" * 40))
    got["next look: attempts"] = outcomes()
finally:
    for t in tools:
        try:
            os.kill(t, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass
print("    %s" % json.dumps(got, sort_keys=True))
sys.exit(0 if got == {"one failed read: the tool is ended": True, "one failed read: lock left": False,
                      "one failed read: attempts": ["stopped"],
                      "unreadable: lock left": True, "unreadable: attempts": ["stopped"],
                      "next look: the tool is ended": True, "next look: lock left": False,
                      "next look: attempts": ["stopped", "lost"]} else 1)
PY
check "W30 a process table that cannot be read is never an empty session: the tool is ended before the lock is settled" \
    $? "see above"

# --- W31 ---------------------------------------------------------------------
# The real second review of 16c154f5a, finding 2 (fixtures/parent_exit_before_init.py): the host's
# watcher read its parent only after it started, so a host that died while it started left it
# reparented, it took that new parent for the host, looked, and ran on. Now the host passes its own
# pid at spawn (RICHOS_REVIEW_WATCH_HOST, set by richos-core review_watch.rs) and the watcher checks
# it before the first look. The real watcher (--monitor --app-state), started 200 ms late by a
# stand-in host that has exited by then.
python3 - "$ENGINE" "$SB/w31" <<'PY'
import json, os, signal, subprocess, sys, time
engine, root = sys.argv[1:3]
os.makedirs(root)
argv = [sys.executable, "-B", os.path.join(engine, "scripts/lib/review_watch.py"), "--monitor",
        "--engine-root", engine, "--app-state", root]
late = "import os, sys, time; time.sleep(0.2); os.execv(sys.executable, sys.argv[1:])"
host = ("import os, subprocess, sys\n"
        "p = subprocess.Popen([sys.executable, '-B', '-c', sys.argv[1]] + sys.argv[2:], stdin=subprocess.DEVNULL,\n"
        "                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True,\n"
        "                     env=dict(os.environ, RICHOS_REVIEW_WATCH_HOST=str(os.getpid())))\n"
        "print(p.pid, flush=True)\n")


def exists(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


got, child = {}, None
try:
    out = subprocess.run([sys.executable, "-B", "-c", host, late] + argv, capture_output=True, text=True,
                         timeout=10, check=True, env=dict(os.environ, PYTHONDONTWRITEBYTECODE="1"))
    child = int(out.stdout.strip())
    end = time.monotonic() + 6                  # past _nap's five-second parent check
    while exists(child) and time.monotonic() < end:
        time.sleep(0.05)
    got["the watcher has exited"] = not exists(child)
    got["it looked"] = os.path.exists(os.path.join(root, "review-watch/sessions/app/told.json"))
finally:
    if child and exists(child):
        os.kill(child, signal.SIGKILL)
print("    %s" % json.dumps(got, sort_keys=True))
sys.exit(0 if got == {"the watcher has exited": True, "it looked": False} else 1)
PY
check "W31 the host's watcher knows its host before the first look: a host gone while it started means no look and an exit" \
    $? "see above"

# --- W32 ---------------------------------------------------------------------
# The real second review of 802194f0e, finding 2 (fixtures/unacknowledged_host_notice.py): the
# NO VERDICT TWICE notice was recorded told before it was written and carried no key, so a host
# that refused it, or a pipe that failed, lost the only word that automatic reviews of that commit
# had stopped. Now it carries a delivery identity and is recorded only once accepted, like a
# verdict (W27). Through the real tick and tell, two lost attempts and no verdict row.
python3 - "$LIB" "$SB/w32" <<'PY'
import io, json, os, sys
lib, root = sys.argv[1:3]
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
os.environ.pop("RICHOS_REVIEW_WATCH_ACKS", None)
sys.path.insert(0, lib)
import review_watch as rw


class BrokenPipe(object):
    def write(self, text):
        raise BrokenPipeError("the host closed the pipe during quit")

    def flush(self):
        pass


now = rw.parse_iso("2026-10-09T09:00:00Z")
attempts = [{"repo": os.path.join(root, "fictional-repository"), "tip": "b" * 40, "work": "teammate:B",
             "name": "worker-B", "session": "lead-B", "outcome": "lost", "trigger": "long-job",
             "why": "the reviewer exited without a verdict"} for _ in range(2)]


class Watcher(object):
    def look(self, t, state):
        return rw.tell(t, state, [], rw.Book([], {}, attempts), [], [], attempts)


def look(sd, t, out=None):
    out = out or io.StringIO()
    try:
        rw.tick(Watcher(), sd, now=t, out=out)
    except BrokenPipeError:
        return "broken pipe"
    return out.getvalue()


got, want = {}, {}
for name, host, acks in (("monitor", False, False), ("host", True, False), ("host, acknowledging", True, True)):
    os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, name)
    rw.HOST_JSON["on"], rw.MONITOR["session"] = host, ("" if host else "lead-B")
    if acks:
        os.environ["RICHOS_REVIEW_WATCH_ACKS"] = "1"
        r, w = os.pipe()
        rw.ACKS["fd"] = r
    sd = rw.session_dir("operator-host" if host else "lead-B")
    seq = [look(sd, now, BrokenPipe())]
    seq += [look(sd, now + 60 * n) for n in (1, 2)]
    g = {"the host pipe failed": seq[0] == "broken pipe", "the next look tells it": "NO VERDICT TWICE" in seq[1],
         "the look after that": "NO VERDICT TWICE" in seq[2]}
    if host:
        g["it carries a key"] = bool([k for ln in seq[1].splitlines() for k in json.loads(ln).get("keys", [])])
    if acks:
        keys = [k for ln in seq[2].splitlines() for k in json.loads(ln).get("keys", [])]
        os.write(w, (json.dumps({"ack": keys}) + "\n").encode())
        g["acknowledged, the next look"] = "NO VERDICT TWICE" in look(sd, now + 180)
        os.environ.pop("RICHOS_REVIEW_WATCH_ACKS")
    got[name] = g
    want[name] = {"the host pipe failed": True, "the next look tells it": True, "the look after that": acks}
    if host:
        want[name]["it carries a key"] = True
    if acks:
        want[name]["acknowledged, the next look"] = False
for k in want:
    print("    %s: %s" % (k, got[k]))
sys.exit(0 if got == want else 1)
PY
check "W32 the NO VERDICT TWICE notice is kept until the host accepts it: a failed pipe or no acknowledgment leaves it for the next look" \
    $? "see above"

# --- W33 ---------------------------------------------------------------------
# The real second review of 0be50ade1, finding 1 (fixtures/capped_lost_notice.py): add() recorded a
# notice's delivery key before the monitor's output cap could leave its block out, so a NO VERDICT
# TWICE notice behind a backlog of verdicts was marked delivered and never shown. A [NOT STARTED]
# problem behind the same cap was marked told the same way. Through the real tick and tell: eleven
# passed verdicts for this lead, then two lost attempts and one problem, four looks.
python3 - "$LIB" "$SB/w33" <<'PY'
import io, json, os, sys
lib, root = sys.argv[1:3]
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, "rw")
os.environ.pop("RICHOS_REVIEW_WATCH_ACKS", None)
sys.path.insert(0, lib)
import review_watch as rw

rw.HOST_JSON["on"], rw.MONITOR["session"] = False, "lead-A"
now = rw.parse_iso("2026-10-09T09:00:00Z")
repo = os.path.join(root, "fictional-repository")
rows = [{"id": "rv-%d" % n, "repo": repo, "tip": "%040x" % n, "work": "teammate:work-%d" % n, "at": rw.iso(now),
         "verdict": "passed", "trigger": "long-job", "reviewer": "claude", "reviewer_model": "opus",
         "findings": 0, "record": ""} for n in range(1, 12)]
attempts = [{"repo": r["repo"], "tip": r["tip"], "work": r["work"], "session": "lead-A", "outcome": "verdict"}
            for r in rows]
attempts += [{"repo": repo, "tip": "f" * 40, "work": "teammate:lost", "session": "lead-A", "outcome": "lost",
              "name": "worker-lost", "trigger": "long-job", "why": "no answer"} for _ in range(2)]
book = rw.Book(rows, {}, attempts)


class Watcher(object):
    def look(self, t, state):
        return rw.tell(t, state, rows, book, [], ["the reviewer could not be started (fictional)"], attempts)


sd = rw.session_dir("lead-A")
key = "lost:%s:%s" % (repo, "f" * 40)
looks, early = [], []
for n in range(4):
    out = io.StringIO()
    rw.tick(Watcher(), sd, now=now + 60 * n, out=out)
    looks.append(out.getvalue())
    delivered = json.load(open(os.path.join(root, "rw", "last-told.json"))).get("delivered", {})
    early.append(key in delivered and not any("NO VERDICT TWICE" in b for b in looks))
got = {"the first look is capped": "more in the next look" in looks[0],
       "never delivered before it is shown": not any(early),
       "the lost notice is shown once": sum("NO VERDICT TWICE" in b for b in looks) == 1,
       "the problem is shown once": sum("NOT STARTED" in b for b in looks) == 1,
       "every verdict is shown once": sum(b.count("[PASSED]") for b in looks) == 11}
print("    %s" % got)
sys.exit(0 if all(got.values()) else 1)
PY
check "W33 a notice the output cap leaves out is neither delivered nor told: a later look tells it" $? "see above"

# --- W34 ---------------------------------------------------------------------
# The same review: the NOT CONVERGING notice was marked told before it was written, so a pipe that
# failed, or a host that never acknowledged, told its verdict again but never the notice. Through
# the real tick and tell: three verdicts of one work, finding #1 still open in the last two.
python3 - "$LIB" "$SB/w34" <<'PY'
import io, json, os, sys
lib, root = sys.argv[1:3]
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
os.environ.pop("RICHOS_REVIEW_WATCH_ACKS", None)
sys.path.insert(0, lib)
import review_watch as rw


class BrokenPipe(object):
    def write(self, text):
        raise BrokenPipeError("the host closed the pipe during quit")

    def flush(self):
        pass


now = rw.parse_iso("2026-10-09T09:00:00Z")
repo = os.path.join(root, "fictional-repository")
rows = []
for i in range(3):
    rec = os.path.join(root, "records", "rv-nc-%d" % i)
    os.makedirs(rec, exist_ok=True)
    ans, ein = {"findings": [], "earlier_findings": []}, []
    if i:
        ans["earlier_findings"] = [{"id": "rv-nc-0#1", "status": "still-open", "note": ""}]
        ein = [{"id": "rv-nc-0#1", "title": "Audio left on disk"}]
    with open(os.path.join(rec, "verdict.json"), "w") as f:
        json.dump({"answer": ans, "earlier_findings_in": ein}, f)
    rows.append({"id": "rv-nc-%d" % i, "at": rw.iso(now), "repo": repo, "tip": "%040x" % (i + 1),
                 "work": "teammate:B", "trigger": "long-job", "verdict": "changes-requested", "findings": 1,
                 "p1": 0, "reviewer": "codex", "reviewer_model": "m", "record": rec})
attempts = [{"repo": r["repo"], "tip": r["tip"], "work": r["work"], "session": "lead-B", "outcome": "verdict"}
            for r in rows]


class Watcher(object):
    def look(self, t, state):
        return rw.tell(t, state, rows, rw.Book(rows, {}, attempts), [], [], attempts)


def look(sd, t, out=None):
    out = out or io.StringIO()
    try:
        rw.tick(Watcher(), sd, now=t, out=out)
    except BrokenPipeError:
        return "broken pipe"
    return out.getvalue()


got, want = {}, {}
for name, host, acks in (("monitor", False, False), ("host", True, False), ("host, acknowledging", True, True)):
    os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, name)
    rw.HOST_JSON["on"], rw.MONITOR["session"] = host, ("" if host else "lead-B")
    if acks:
        os.environ["RICHOS_REVIEW_WATCH_ACKS"] = "1"
        r, w = os.pipe()
        rw.ACKS["fd"] = r
    sd = rw.session_dir("operator-host" if host else "lead-B")
    seq = [look(sd, now, BrokenPipe())] + [look(sd, now + 60 * n) for n in (1, 2)]
    g = {"the pipe failed": seq[0] == "broken pipe",
         "the next look tells it": seq[1].count("NOT CONVERGING") == 1,
         "the look after that": seq[2].count("NOT CONVERGING") == 1}
    if acks:
        keys = [k for ln in seq[2].splitlines() for k in json.loads(ln).get("keys", [])]
        os.write(w, (json.dumps({"ack": keys}) + "\n").encode())
        g["acknowledged, the next look"] = "NOT CONVERGING" in look(sd, now + 180)
        os.environ.pop("RICHOS_REVIEW_WATCH_ACKS")
    got[name] = g
    want[name] = {"the pipe failed": True, "the next look tells it": True, "the look after that": acks}
    if acks:
        want[name]["acknowledged, the next look"] = False
for k in want:
    print("    %s: %s" % (k, got[k]))
sys.exit(0 if got == want else 1)
PY
check "W34 the NOT CONVERGING notice is kept until accepted, like its verdict: a failed pipe or no acknowledgment tells it again" \
    $? "see above"

# --- W35 ---------------------------------------------------------------------
# echo-opus-watchers1, 2026-10-20: stopped mid-job for a session restart, the lead recorded
# `workspaces.sh wait --started '... then a continues: echo-opus-watchers1 job ...'`, and the
# next session's watcher spent a full handover review on it anyway. The CEO: "why does the
# handover of the unfinished work from the previous session needs a "review" here in this
# session???" Work stopped before it finished is reviewed when its continuation hands over.
resetstate
reg echo-sonnet-w1 600 30
python3 - "$SB" <<'PY'
import json, os, sys, time
p = os.path.join(sys.argv[1], "workspaces", "agents", "sess-w--echo-sonnet-w1.json")
r = json.load(open(p))
r["waiting"] = {"kind": "started", "at": time.time() - 20, "todo": "",
                "on": "stopped early on the CEO word for the session restart; next session: a continues: "
                      "echo-sonnet-w1 job finishes the VM check, then land"}
json.dump(r, open(p, "w"))
PY
tickrw
# A started review's lock is taken inside the look itself (Watcher.start -> take_lock), before
# --tick returns, so no lock is the fact that none started; no clock is waited on.
[ "$RC" -eq 0 ] && [ "$(ncalls)" = "0" ] && [ -z "$(ls "$SB/rw/locks" 2>/dev/null)" ]
check "W35 ended, with a recorded wait naming its continues: job: no handover review (its continuation's covers these commits)" \
    $? "rc=$RC calls=$(ncalls) locks=$(ls "$SB/rw/locks" 2>/dev/null) out=$OUT"
resetstate
reg echo-sonnet-w1 600 30
FAKE_SR_MODE=passed tickrw
waitcalls 1
A="$(lastargv)"
[ "$RC" -eq 0 ] && [ "$(ncalls)" = "1" ] && has "$A" '"--trigger", "handover"'
check "W35 the same end with no stop or wait recorded (a silent death): still one handover review" $? "calls=$(ncalls) argv=$A out=$OUT"

# --- W36 ---------------------------------------------------------------------
# 2026-10-24 13:32Z: a new session's watcher printed four NO VERDICT TWICE notices that a session
# ended on 2026-10-21 had already been told, for commits already on main: the work had landed.
# Through the real tick and tell, in a real repository: an old session's two losses of a commit on
# main and of one that is not, then a look by a new session.
python3 - "$LIB" "$SB/w36" <<'PY'
import io, json, os, subprocess, sys
lib, root = sys.argv[1:3]
os.environ["SECOND_REVIEW_STATE_DIR"] = os.path.join(root, "sr")
os.environ["REVIEW_WATCH_STATE_DIR"] = os.path.join(root, "rw")
os.environ.pop("RICHOS_REVIEW_WATCH_ACKS", None)
sys.path.insert(0, lib)
import review_watch as rw

repo = os.path.join(root, "fictional-repository")
os.makedirs(repo)


def g(*args):
    return subprocess.run(["git", "-C", repo, "-c", "core.hooksPath=/dev/null", "-c", "user.name=Author",
                           "-c", "user.email=author@example.invalid", "-c", "commit.gpgsign=false"] + list(args),
                          check=True, capture_output=True, text=True).stdout.strip()


g("init", "-q", "-b", "main")
g("commit", "-q", "--allow-empty", "-m", "landed work")
landed = g("rev-parse", "HEAD")
g("checkout", "-q", "-b", "cc/worker-open")
g("commit", "-q", "--allow-empty", "-m", "work that never landed")
open_tip = g("rev-parse", "HEAD")
g("checkout", "-q", "main")

now = rw.parse_iso("2026-10-24T13:32:00Z")
attempts = [{"repo": repo, "tip": tip, "work": "teammate:old--" + name, "name": name, "session": "lead-old",
             "outcome": "lost", "trigger": "long-job", "why": "the reviewer exited without a verdict"}
            for tip, name in ((landed, "worker-landed"), (open_tip, "worker-open")) for _ in range(2)]


class Watcher(object):
    def look(self, t, state):
        return rw.tell(t, state, [], rw.Book([], {}, attempts), [], [], attempts)


rw.HOST_JSON["on"] = False
rw.MONITOR["session"] = "lead-new"
out = io.StringIO()
rw.tick(Watcher(), rw.session_dir("lead-new"), now=now, out=out)
look = out.getvalue()
got = {"the landed commit is not told": "worker-landed" not in look,
       "the unlanded one is still told": "[NO VERDICT TWICE] worker-open" in look}
print("    %s" % got)
sys.exit(0 if all(got.values()) else 1)
PY
check "W36 a NO VERDICT TWICE notice is not told for a commit on main, and still told for one that is not" \
    $? "see above"

# --- W04 ---------------------------------------------------------------------
resetstate
reg echo-sonnet-w1 300 -
tickrw
[ "$RC" -eq 0 ] && [ "$(ncalls)" = "0" ]; check "W04 a commit five minutes in, on a base a day old: no long-job review" $? "calls=$(ncalls) out=$OUT"
reg echo-sonnet-w1 3660 -
FAKE_SR_MODE=passed tickrw
waitcalls 1
A="$(lastargv)"
[ "$(ncalls)" = "1" ] && has "$A" '"--trigger", "long-job"'; check "W04 61 minutes after the registry's start: a long-job review" $? "calls=$(ncalls) argv=$A"

# --- W05 ---------------------------------------------------------------------
resetstate
python3 - "$SB" <<'PY'
import json, os, sys, time
sb = sys.argv[1]
d = os.path.join(sb, "projects", "-fixture", "sess-w", "subagents")
os.makedirs(d, exist_ok=True)
p = os.path.join(d, "agent-abcdef0123456789.jsonl")
open(p, "w").write("{}\n")
old = time.time() - 25 * 60
os.utime(p, (old, old))
PY
reg echo-sonnet-w1 1800 -
python3 - "$SB" <<'PY'
import json, os, sys
p = os.path.join(sys.argv[1], "workspaces", "agents", "sess-w--echo-sonnet-w1.json")
r = json.load(open(p)); r["agent_id"] = "abcdef0123456789"; r["session_id"] = ""
json.dump(r, open(p, "w"))
PY
OLD="@$(python3 -c 'import time; print(int(time.time() - 25 * 60))')"
GIT_COMMITTER_DATE="$OLD" GIT_AUTHOR_DATE="$OLD" G commit -q --amend --no-edit
TIP="$(G rev-parse HEAD)"
FAKE_SR_MODE=passed tickrw
waitcalls 1
A="$(lastargv)"
[ "$(ncalls)" = "1" ] && has "$A" '"--trigger", "quiet"' && has "$A" "\"--tip\", \"$TIP\""
check "W05 gone quiet 25 minutes with an unreviewed commit: a review starts" $? "calls=$(ncalls) argv=$A out=$OUT"

# --- W06 ---------------------------------------------------------------------
resetstate
reg echo-sonnet-w1 3660 -
FAKE_SR_MODE=sleep tickrw
waitcalls 1
MIDPID="$(cat "$SB/calls/00/pid" 2>/dev/null)"
reg echo-sonnet-w1 3660 5
FAKE_SR_MODE=passed tickrw
waitcalls 2
A="$(lastargv)"
GONE=1
for _ in $(seq 1 50); do if [ -n "$MIDPID" ] && ! kill -0 "$MIDPID" 2>/dev/null; then GONE=0; break; fi; sleep 0.1; done
[ "$(ncalls)" = "2" ] && [ "$GONE" -eq 0 ] && has "$A" '"--trigger", "handover"' && has "$(cat "$SB/calls/00/argv.json")" '"long-job"' \
  && grep -q '"outcome": "superseded"' "$SB/rw/attempts.jsonl"
check "W06 a handover stops the mid-job review of the same tip by its recorded pid and starts its own" $? \
    "calls=$(ncalls) mid pid=$MIDPID gone=$GONE argv=$A"

# --- W07 ---------------------------------------------------------------------
resetstate
G checkout -qb codex/fix-w7 "$BASE"
printf 'base\ncodex\n' >"$REPO/a.txt"
G commit -qam "codex: a fix"
CTIP="$(G rev-parse HEAD)"
G checkout -q cc/echo-sonnet-w1
printf '## 2026-10-09T01:00:00Z Rich: please build codex/fix-w7\n\nCODEX-ASK-LINE: make the fix for codex/fix-w7.\n\n## 2026-10-09T01:10:00Z Rich: something else\n\nOTHER-ASK\n' >"$SB/codex/to-codex.md"
printf '## 2026-10-09T00:00:00Z STATUS: old\n\nold entry\n' >"$SB/codex/to-rich.md"
tickrw                                    # the first look starts at the end of the file
printf '\n## 2026-10-09T02:00:00Z READY TO LAND: codex/fix-w7\n\nCommit: `%s`\nRepository: richos only.\nCLAIM-LINE\n' "$CTIP" >>"$SB/codex/to-rich.md"
FAKE_SR_MODE=passed tickrw                # it grew: told after it settles one look
FAKE_SR_MODE=passed tickrw
waitcalls 1
A="$(lastargv)"
W="$(cat "$(ls -d "$SB"/calls/* | tail -1)/words.txt" 2>/dev/null)"
[ "$(ncalls)" = "1" ] && has "$A" '"--branch", "codex/fix-w7"' && has "$A" "\"--tip\", \"$CTIP\"" && has "$A" '"--author", "codex"' \
  && has "$A" "\"--base\", \"$BASE\"" && has "$W" "CODEX-ASK-LINE" && ! has "$W" "OTHER-ASK"
check "W07 Codex's READY entry starts a review of its branch, with the entry that asked for it as the words" $? "calls=$(ncalls) argv=$A words=$W out=$OUT"

# Codex's READY for the private record says "Repository richos-hq only", with no colon. The
# watcher took richos, found no such branch there and told [NOT STARTED] (six times on
# 2026-10-24). A repository that gets no second review is not a review case.
HQ="$SB/richos-hq"
H() { git -C "$HQ" -c core.hooksPath=/dev/null -c user.name=Author -c user.email=author@example.invalid -c commit.gpgsign=false "$@"; }
mkdir -p "$HQ"; H init -q -b main
printf 'record\n' >"$HQ/r.txt"; H add -A; H commit -qm "record base"
H checkout -qb codex/hq-w7; printf 'record\nevidence\n' >>"$HQ/r.txt"; H commit -qam "codex: evidence"
HQTIP="$(H rev-parse HEAD)"
python3 -c 'import json,os,sys; json.dump({"repos": [os.path.realpath(p) for p in sys.argv[2:]]}, open(sys.argv[1], "w"))' \
    "$SB/workspaces/repos.json" "$REPO" "$HQ"
resetstate
tickrw
printf '\n## 2026-10-09T03:00:00Z READY TO LAND: codex/hq-w7\n\nRepository richos-hq only (private). Commit %s.\n' "$HQTIP" >>"$SB/codex/to-rich.md"
tickrw; O1="$OUT"
tickrw
[ "$(ncalls)" = "0" ] && ! has "$O1$OUT" "NOT STARTED" && ! has "$O1$OUT" "codex/hq-w7"
check "W07 a READY naming only a repository with no second review (Repository richos-hq only) is silent" $? "calls=$(ncalls) out=$O1 / $OUT"
resetstate
tickrw
printf '\n## 2026-10-09T04:00:00Z READY TO LAND: codex/fix-w7\n\nRepositories: private richos-hq and public richos. Commit `%s`.\n' "$CTIP" >>"$SB/codex/to-rich.md"
FAKE_SR_MODE=passed tickrw
FAKE_SR_MODE=passed tickrw
waitcalls 1
A="$(lastargv)"
[ "$(ncalls)" = "1" ] && has "$A" '"--branch", "codex/fix-w7"' && has "$A" "\"--tip\", \"$CTIP\""
check "W07 a READY for richos (beside richos-hq) still starts its review" $? "calls=$(ncalls) argv=$A out=$OUT"
resetstate
tickrw
printf '\n## 2026-10-09T05:00:00Z READY TO LAND: codex/missing-w7\n\nRepository: richos only. Commit %s.\n' "$HQTIP" >>"$SB/codex/to-rich.md"
tickrw; O1="$OUT"
tickrw
[ "$(ncalls)" = "0" ] && has "$O1$OUT" "NOT STARTED" && has "$O1$OUT" "codex/missing-w7"
check "W07 a READY for richos whose commit is not in richos still says no review started" $? "calls=$(ncalls) out=$O1 / $OUT"

# --- W08 ---------------------------------------------------------------------
resetstate
reg echo-sonnet-w1 600 30
tickrw                                    # the first look: the ledger's end is where telling starts
python3 - "$SB" "$REPO" "$TIP" <<'PY'
import json, os, sys
sb, repo, tip = sys.argv[1:4]
st = os.path.join(sb, "sr")
rows = []
for i, (status, f) in enumerate([(None, True), ("still-open", True), ("still-open", True)]):
    rid = "rv-nc-%d" % i
    rec = os.path.join(st, "reviews", rid)
    os.makedirs(rec, exist_ok=True)
    ans = {"findings": [], "earlier_findings": []}
    ein = []
    if i == 0:
        ans["findings"] = [{"priority": 2, "title": "Audio left on disk", "files": [], "evidence": "", "fixture": ""}]
    else:
        ans["earlier_findings"] = [{"id": "rv-nc-0#1", "status": "still-open", "note": ""}]
        ein = [{"id": "rv-nc-0#1", "title": "Audio left on disk"}]
    json.dump({"answer": ans, "earlier_findings_in": ein}, open(os.path.join(rec, "verdict.json"), "w"))
    rows.append({"id": rid, "at": "2026-10-09T0%d:00:00Z" % i, "repo": os.path.realpath(repo), "tip": "f" * 39 + str(i),
                 "work": "teammate:sess-w--echo-sonnet-w1", "trigger": "long-job", "verdict": "changes-requested",
                 "findings": 1, "p1": 0, "reviewer": "codex", "reviewer_model": "m", "record": rec})
with open(os.path.join(st, "reviews.jsonl"), "a") as f:
    for r in rows:
        f.write(json.dumps(r) + "\n")
PY
tickrw
N1="$(printf '%s' "$OUT" | grep -c 'NOT CONVERGING')"
tickrw
N2="$(printf '%s' "$OUT" | grep -c 'NOT CONVERGING')"
[ "$N1" = "1" ] && [ "$N2" = "0" ]
check "W08 a finding still open through two rechecks in a row is told once as not converging" $? "first=$N1 again=$N2"

# Review rv-20261009T104053Z-8662354d-47b2: two passed rechecks of a finding
# already filed as a follow-up (blocks false) were told as not converging. And
# one work in two repositories: each repository's rechecks are compared only
# with that repository's, so a finding still open in richos through two richos
# rechecks is told though a richos-hq review came between them.
resetstate
reg echo-sonnet-w1 600 30
tickrw
python3 - "$SB" "$REPO" "$SB/w08-rows.jsonl" <<'PY'
import json, os, sys
sb, repo, out = sys.argv[1:4]
st = os.path.join(sb, "sr")
richos, hq = os.path.realpath(repo), os.path.realpath(repo) + "-hq"
rows = []
def add(rid, work, where, verdict, findings=(), earlier=(), blocks=None):
    rec = os.path.join(st, "reviews", rid)
    os.makedirs(rec, exist_ok=True)
    ans = {"findings": [dict(f, blocks=True) for f in findings], "earlier_findings": []}
    ein = []
    for fid, title in earlier:
        ans["earlier_findings"].append({"id": fid, "status": "still-open", "blocks": blocks, "note": ""})
        ein.append({"id": fid, "title": title})
    json.dump({"answer": ans, "earlier_findings_in": ein}, open(os.path.join(rec, "verdict.json"), "w"))
    rows.append({"id": rid, "at": "2026-10-09T0%d:00:00Z" % len(rows), "repo": where, "tip": "e" * 38 + "%02d" % len(rows),
                 "work": work, "trigger": "long-job", "verdict": verdict, "findings": len(findings), "p1": 0,
                 "reviewer": "codex", "reviewer_model": "m", "record": rec})
fu = [{"priority": 3, "title": "Filed follow-up", "files": [], "evidence": "", "fixture": ""}]
add("rv-fu-0", "teammate:fu", richos, "passed", fu)
add("rv-fu-1", "teammate:fu", richos, "passed", earlier=[("rv-fu-0#1", "Filed follow-up")], blocks=False)
xr = [{"priority": 2, "title": "Blocker in richos", "files": [], "evidence": "", "fixture": ""}]
add("rv-xr-0", "teammate:xr", richos, "changes-requested", xr)
add("rv-xr-1", "teammate:xr", hq, "passed")
add("rv-xr-2", "teammate:xr", richos, "changes-requested", earlier=[("rv-xr-0#1", "Blocker in richos")], blocks=True)
add("rv-xr-3", "teammate:xr", hq, "passed")
# The two rows that decide come last, in a look of their own: one look's block
# is capped at BLOCK_CHARS.
add("rv-fu-2", "teammate:fu", richos, "passed", earlier=[("rv-fu-0#1", "Filed follow-up")], blocks=False)
add("rv-xr-4", "teammate:xr", richos, "changes-requested", earlier=[("rv-xr-0#1", "Blocker in richos")], blocks=True)
with open(out, "w") as f:
    for r in rows:
        f.write(json.dumps(r) + "\n")
PY
head -n 6 "$SB/w08-rows.jsonl" >>"$SB/sr/reviews.jsonl"
tickrw
tail -n 2 "$SB/w08-rows.jsonl" >>"$SB/sr/reviews.jsonl"
tickrw
NC="$(printf '%s' "$OUT" | grep 'NOT CONVERGING')"
has "$OUT" "reviews/rv-xr-4/verdict.json" && ! has "$OUT" "... more in the next look" && ! has "$NC" "rv-fu-0#1"
check "W08 a finding filed as a follow-up (blocks false) is never told as not converging" $? "$NC"
[ "$(printf '%s\n' "$NC" | grep -c 'rv-xr-0#1')" = "1" ]
check "W08 one work in two repositories: a finding still open through two rechecks of its own repository is told" $? "$NC"

# --- W09 ---------------------------------------------------------------------
resetstate
cfg 'SECOND_REVIEW_REPOS="some-other-repo"'
reg echo-sonnet-w1 600 30
tickrw
[ "$RC" -eq 0 ] && [ "$(ncalls)" = "0" ]; check "W09 a repository not in SECOND_REVIEW_REPOS (and the femcboost workspace) is never reviewed" $? "calls=$(ncalls) out=$OUT"
resetstate
cfg '# no SECOND_REVIEW_REPOS here'
reg echo-sonnet-w1 600 30
tickrw
O1="$OUT"
tickrw
[ "$(ncalls)" = "0" ] && has "$O1" "no repository is listed in SECOND_REVIEW_REPOS" && [ -z "$OUT" ]
check "W09 with SECOND_REVIEW_REPOS absent it says so once and starts nothing" $? "calls=$(ncalls) first=$O1 second=$OUT"
resetstate
cfg 'SECOND_REVIEW_REPOS="/abs/richos"'
reg echo-sonnet-w1 600 30
tickrw
O1="$OUT"
tickrw
[ "$(ncalls)" = "0" ] && has "$O1" "orchestration.config cannot be read: bash reads it as '/abs/richos'" && [ -z "$OUT" ]
check "W09 a SECOND_REVIEW_REPOS value that is not names is said once and starts nothing" $? "calls=$(ncalls) first=$O1 second=$OUT"
cfg 'SECOND_REVIEW_REPOS="richos"'
W09_BRANCH="$(G symbolic-ref --short HEAD)"
G checkout -qb cc/newt-opus-wnotes main
mkdir -p "$REPO/docs"
printf 'notes\n' >"$REPO/docs/CHANGELOG-NIGHTLY.md"
printf '{}\n' >"$REPO/docs/release-notes.json"
G add docs; G commit -qm "release notes only"
G checkout -qb cc/newt-opus-wmixed
printf 'more\n' >>"$REPO/a.txt"
G commit -qam "release notes and one other file"
G checkout -q "$W09_BRANCH"
resetstate
reg newt-opus-wnotes 600 30
tickrw
[ "$RC" -eq 0 ] && [ "$(ncalls)" = "0" ]; check "W09 work that changes only release notes is never reviewed" $? "calls=$(ncalls) out=$OUT"
resetstate
reg newt-opus-wmixed 600 30
tickrw
waitcalls 1; waitrows 1
[ "$RC" -eq 0 ] && [ "$(ncalls)" = "1" ]; check "W09 release notes with any other file are reviewed" $? "calls=$(ncalls) out=$OUT"

# --- W10 ---------------------------------------------------------------------
python3 - "$ENGINE/monitors/monitors.json" <<'PY'
import json, sys
mons = json.load(open(sys.argv[1]))
m = [x for x in mons if x.get("name") == "review-watch"]
sys.exit(0 if len(m) == 1 and m[0].get("when") == "always"
         and m[0].get("command", "").endswith("/scripts/review-watch.sh --monitor") else 1)
PY
check "W10 monitors.json starts review-watch.sh --monitor always" $? "monitors.json wiring"

# --- W37 ---------------------------------------------------------------------
# 2026-10-24: the fix for a READY naming only richos-hq landed at 19:49Z and was installed,
# but this session's monitor (PID 90830, started 13:32Z) had loaded review_watch.py once and
# told [NOT STARTED] for that READY again at 20:44Z. The real monitor, run from a copy of the
# engine, looks once; the copy's review_watch.py then changes on disk, and the next look runs
# the changed code. Then the copy breaks: the failed look is told once, never passed silently,
# and the monitor keeps watching.
resetstate
W37="$SB/w37"
mkdir -p "$W37/engine/scripts/lib" "$W37/engine/mega-lander"
cp "$LIB"/*.py "$W37/engine/scripts/lib/"
cp "$ENGINE"/mega-lander/*.py "$W37/engine/mega-lander/"
W37_SD="$SB/rw/sessions/w37-lead"
RICHOS_SESSION_ID=w37-lead RICHOS_SESSION_PID=$$ REVIEW_WATCH_POLL_SECONDS=1 W37_MARK="$W37/fresh" \
    python3 -B "$W37/engine/scripts/lib/review_watch.py" --monitor --config "$SB/orchestration.config" \
    --engine-root "$W37/engine" >"$W37/out" 2>&1 &
W37_PID=$!
for _ in $(seq 1 200); do [ -s "$W37_SD/told.json" ] && break; sleep 0.1; done
for _ in $(seq 1 30); do [ -s "$W37_SD/evaluated.json" ] && break; sleep 0.1; done
# evfield <key> [<subkey>]: one field of the session's evaluated.json, or nothing.
evfield() { python3 -c 'import json, sys
v = json.load(open(sys.argv[1]))
for k in sys.argv[2:]:
    v = v.get(k) if isinstance(v, dict) else None
print("" if v is None else v)' "$W37_SD/evaluated.json" "$@" 2>/dev/null; }
FIRST="$(evfield identity)"
python3 - "$W37/engine/scripts/lib/review_watch.py" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
anchor = "    out = out or sys.stdout\n"
assert s.count(anchor) == 1, "tick's first line moved"
open(p, "w").write(s.replace(anchor, '    open(os.environ["W37_MARK"], "a").write("fresh\\n")\n' + anchor))
PY
# The fact waited for: a look recorded a source other than the first one (bounded, for a hang).
for _ in $(seq 1 200); do
    SECOND="$(evfield identity)"
    [ -s "$W37/fresh" ] && [ -n "$SECOND" ] && [ "$SECOND" != "$FIRST" ] && break
    sleep 0.1
done
w37status() { ST="$(RICHOS_SESSION_ID=w37-lead python3 -B "$LIB/review_watch.py" --status --engine-root "$W37/engine" 2>&1)"; STRC=$?; }
w37status; ST1="$ST"; ST1RC=$STRC
[ -s "$W37/fresh" ] && [ -n "$FIRST" ] && [ -n "$SECOND" ] && [ "$FIRST" != "$SECOND" ]
check "W37 the monitor's look after its source changes on disk runs the changed code" $? \
    "marker=$(cat "$W37/fresh" 2>/dev/null) evaluated before=$FIRST after=$SECOND out=$(head -c 600 "$W37/out")"
[ "$ST1RC" -eq 0 ] && has "$ST1" "watching this session"
check "W37 --status reads a monitor whose looks succeed as watching" $? "rc=$ST1RC status=$ST1"
python3 - "$W37/engine/scripts/lib/review_watch.py" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
anchor = "import stall_watch  #"
assert s.count(anchor) == 1, "the import line moved"
open(p, "w").write(s.replace(anchor, 'raise RuntimeError("w37: a broken engine")\n' + anchor))
PY
# The fact waited for: three failing looks recorded (each one's failed.at), bounded for a hang.
SEEN=" "
for _ in $(seq 1 200); do
    F="$(evfield failed at)"
    [ -n "$F" ] && case "$SEEN" in *" $F "*) ;; *) SEEN="$SEEN$F " ;; esac
    [ "$(printf '%s' "$SEEN" | wc -w | tr -d ' ')" -ge 3 ] && break
    sleep 0.1
done
NLOOKS="$(printf '%s' "$SEEN" | wc -w | tr -d ' ')"
NFAIL="$(grep -c "a look failed" "$W37/out")"
w37status; ST2="$ST"; ST2RC=$STRC
kill -0 "$W37_PID" 2>/dev/null; W37_ALIVE=$?
kill "$W37_PID" 2>/dev/null; wait "$W37_PID" 2>/dev/null
[ "$NLOOKS" -ge 3 ] && [ "$NFAIL" = "1" ] && grep -q "w37: a broken engine" "$W37/out" && [ "$W37_ALIVE" -eq 0 ]
check "W37 a look that fails is told once per failing streak, and the monitor keeps watching" $? \
    "failing looks=$NLOOKS told=$NFAIL alive=$W37_ALIVE out=$(head -c 600 "$W37/out")"
[ "$ST2RC" -eq 1 ] && has "$ST2" "LOOKS FAILING"
check "W37 --status reads a monitor whose looks fail as failing, never as watching" $? "rc=$ST2RC status=$ST2"

# --- W38 ---------------------------------------------------------------------
# 2026-10-24, the second review of 5331505bd (finding 1): a look whose evaluation raised was
# caught inside the child, which still recorded a successful look and exited 0, so --status
# said watching; and once told, the same error was never told again, even after a good look.
# The real monitor, from a copy of the engine whose Watcher.look raises while a marker file
# exists: the failing looks are told once, --status says failing; a clean look reads healthy
# again; the same error coming back is told again.
resetstate
W38="$SB/w38"
mkdir -p "$W38/engine/scripts/lib" "$W38/engine/mega-lander"
cp "$LIB"/*.py "$W38/engine/scripts/lib/"
cp "$ENGINE"/mega-lander/*.py "$W38/engine/mega-lander/"
python3 - "$W38/engine/scripts/lib/review_watch.py" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
anchor = '        """Lines to print. Starts what is due; settles what ended."""\n'
assert s.count(anchor) == 1, "Watcher.look's first line moved"
open(p, "w").write(s.replace(anchor, anchor + '        if os.path.exists(os.environ["W38_RAISE"]):\n'
                             '            raise OSError("w38: registry unavailable")\n'))
PY
W38_SD="$SB/rw/sessions/w38-lead"
: >"$W38/raise"
RICHOS_SESSION_ID=w38-lead RICHOS_SESSION_PID=$$ REVIEW_WATCH_POLL_SECONDS=1 W38_RAISE="$W38/raise" \
    python3 -B "$W38/engine/scripts/lib/review_watch.py" --monitor --config "$SB/orchestration.config" \
    --engine-root "$W38/engine" >"$W38/out" 2>&1 &
W38_PID=$!
# w38field <key> [<subkey>]: one field of the session's evaluated.json, or nothing.
w38field() { python3 -c 'import json, sys
v = json.load(open(sys.argv[1]))
for k in sys.argv[2:]:
    v = v.get(k) if isinstance(v, dict) else None
print("" if v is None else v)' "$W38_SD/evaluated.json" "$@" 2>/dev/null; }
# w38fails <n>: wait (bounded, for a hang) until n more failing looks are recorded (each one's failed.at).
w38fails() { local seen=" " f; for _ in $(seq 1 200); do
    f="$(w38field failed at)"
    [ -n "$f" ] && [ "$f" != "$W38_LASTF" ] && case "$seen" in *" $f "*) ;; *) seen="$seen$f " ;; esac
    [ "$(printf '%s' "$seen" | wc -w | tr -d ' ')" -ge "$1" ] && break
    sleep 0.1
done; W38_LASTF="$f"; W38_N="$(printf '%s' "$seen" | wc -w | tr -d ' ')"; }
w38status() { ST="$(RICHOS_SESSION_ID=w38-lead python3 -B "$LIB/review_watch.py" --status --engine-root "$W38/engine" 2>&1)"; STRC=$?; }
W38_LASTF=""
w38fails 3; N1="$W38_N"
TOLD1="$(grep -c "a look failed" "$W38/out")"
w38status; ST1="$ST"; ST1RC=$STRC
[ "$N1" -ge 3 ] && [ "$TOLD1" = "1" ] && grep -q "w38: registry unavailable" "$W38/out" \
    && [ "$ST1RC" -eq 1 ] && has "$ST1" "LOOKS FAILING" && has "$ST1" "w38: registry unavailable"
check "W38 a look whose evaluation raises fails: told once per failing streak, --status never says watching" $? \
    "failing looks=$N1 told=$TOLD1 status rc=$ST1RC status=$ST1 out=$(head -c 600 "$W38/out")"
rm -f "$W38/raise"
# The fact waited for: a successful look recorded after the last failing one.
for _ in $(seq 1 200); do
    [ -n "$(w38field at)" ] && [ -z "$(w38field failed at)" ] && break
    sleep 0.1
done
w38status; ST2="$ST"; ST2RC=$STRC
[ "$ST2RC" -eq 0 ] && has "$ST2" "watching this session"
check "W38 a clean look after failing ones reads healthy again" $? "rc=$ST2RC status=$ST2"
: >"$W38/raise"
W38_LASTF=""
w38fails 1
TOLD2="$(grep -c "a look failed" "$W38/out")"
kill "$W38_PID" 2>/dev/null; wait "$W38_PID" 2>/dev/null
[ "$W38_N" -ge 1 ] && [ "$TOLD2" = "2" ]
check "W38 the same evaluation error after a successful look is told again" $? \
    "failing looks=$W38_N told=$TOLD2 out=$(head -c 900 "$W38/out")"

echo
echo "review-watch.test.sh: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
