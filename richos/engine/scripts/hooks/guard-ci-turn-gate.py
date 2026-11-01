#!/usr/bin/env python3
"""guard-ci-turn-gate.py — THE TURN DOES NOT END WHILE THIS SESSION'S OWN PUSH
                            IS FAILING CI.

===========================================================================
WHAT THIS EXISTS FOR — AND THE QUESTION IT HAS TO MAKE ANSWERABLE
===========================================================================
On 2026-09-13 the founder posted three screenshots of red CI in one evening and
asked, twice:

    "HOW MANY MORE TIMES WILL I NEED TO POST SCREENSHOTS OF THAT CI?"

He was then given a promise. He asked the only question a promise cannot
answer: HOW WOULD HE KNOW the answer was zero? He could not, and that is the
whole defect.

The engine already reported red CI at SessionStart
(`session-start-ci-surface.sh`) and already made red cost something at a land
(`guard-ci-red-lands.sh`). Between them a workflow in richos-hq stayed red for
SIXTEEN DAYS, named in every single session-start banner, and nobody moved. A
report nobody is forced to read is not a mechanism; it is a promise with a
larger font.

This is the forcing function. It is the third leg and the only one that costs
the orchestrator something at the moment he is about to walk away: THE TURN
DOES NOT END.

===========================================================================
THE PREDICATE, AND WHY EVERY CLAUSE OF IT IS NARROW
===========================================================================
The lesson this project paid for with g11/g12/g13 — three blocking guards
built in one day, waived into uselessness by lunchtime — is that a guard dies
of FALSE POSITIVES, not of weakness. So the hold is a conjunction:

  1. THIS SESSION PUSHED. Derived from the transcript's own tool calls, not
     from a list somebody typed: a `git push` this session actually ran. A
     session that pushed nothing is never held, and cannot be.
  2. THE COMMIT IS THE ONE THAT WENT. Not "the repository is red" — the
     commit `git push` sent, read from the local `refs/remotes/origin/<branch>`
     that the push itself updated. Somebody else's red, on somebody else's
     commit, holds nothing here.
  3. GITHUB SAYS THAT COMMIT FAILED. A completed run whose conclusion is one
     of the not-green values. Not slow, not skipped, not never-run — those are
     real axes, they are reported in full by `ci-status.sh`, and gating on them
     is how a guard earns a reflexive waiver.
  4. AND THE LATEST PUSH WINS. Push a fix and the branch's newest commit is
     what gets judged; the broken one is superseded with no ceremony at all.

===========================================================================
THE CRUX: A RUN STILL IN PROGRESS DOES NOT HOLD THE TURN. IT HOLDS THE
OBLIGATION.
===========================================================================
This was the one real design decision and it has two wrong answers.

  BLOCK ON IN-PROGRESS, and the session is trapped for the length of the run.
  A twelve-shard suite is twenty minutes in which the orchestrator cannot hand
  the founder an answer. Within one day the ack becomes reflex, and a guard
  that is always acked protects nothing while costing a keystroke. Worse, it
  would be blocking on a fact it has NOT established — the same error as
  blocking because the network was down.

  IGNORE IN-PROGRESS, and the turn ends on a green-so-far verdict that turns
  red ninety seconds later. That is precisely the founder's screenshot.

So: THE TURN IS ALLOWED, THE OBLIGATION IS WRITTEN DOWN, AND EVERY LATER
TURN-END RE-READS IT. The obligation lives in a file keyed by (repository,
commit), outside the turn, and it does not expire when the turn does. Turn-ends
are the most frequent event in a session, and the founder is present at them,
so a verdict that arrives ninety seconds late costs ONE turn — against sixteen
days, which is what the alternative measured.

And the allowance is ANNOUNCED, never silent. "CI is still running on what this
session pushed, the turn is not held for it, the next turn-end reads the
verdict." The absence of a finding must be distinguishable from the absence of
a check; that rule is this engine's oldest, and an unannounced pass here would
break it.

===========================================================================
THE HARD TIME BUDGET — A NUMBER, NOT AN ADJECTIVE
===========================================================================
"It should be fast" is not something anybody can be held to. So the budget is a
named constant, it is enforced as a wall-clock deadline around every subprocess
this file starts, and ON EXPIRY THE TURN IS ALLOWED with the reason printed.

    BUDGET_SECONDS = 2.0

A check that cannot answer inside its budget must never hold the turn. Two
consequences worth stating plainly:

  * A repository read BEFORE the deadline still counts. If the first repository
    is established red and the second cannot be read in time, the turn is held
    for the first and the second is reported as unread. The gate refuses only
    on facts it established, and it never pretends the others were clean.
  * The steady state costs ZERO network. A commit whose runs are all completed
    and green can never change, so that reading is cached forever. In a session
    where nothing new is pushed, every turn-end after the first spends no API
    call at all. A RED reading is refreshed after RED_RECHECK_SECONDS, because
    `gh run rerun` is a real way out and a gate that could not see a re-run
    would be a gate you have to disable to escape.

  MEASURED, rather than asserted, on 2026-09-13 against this project's own
  27 MB session transcript with the real `gh` and the three repositories it had
  actually pushed to: 1.41s on the first turn-end (three live reads) and 0.46s
  on the next. Re-derive it rather than trust it — point the hook at a real
  transcript and time it. The transcript half is not the cost: a full scan of a
  21.5 MB transcript is 0.10s and the incremental re-scan is 0.0000s, because
  the byte offset reached last turn is kept in the session's state file.

===========================================================================
WHEN IT CANNOT LOOK, IT SAYS SO AND STANDS ASIDE
===========================================================================
GitHub unreachable, `gh` missing, a token expired, an aeroplane: none of that
is evidence of red. Blocking on it would refuse turns for reasons unrelated to
CI several times a week, and the fix on the day would always be to waive —
g11/g12/g13's death arrived at by a different road. The gate announces what it
could not read and the exact command that would answer it, and lets go.

===========================================================================
THE ESCAPE HATCH, AND WHY A BARE MARKER EXEMPTS NOTHING
===========================================================================
    ci-red-ack: <repository or workflow> — <why this cannot be fixed now>

on its own line in the turn's final message. It must NAME something that is
actually red in this finding; the reason must be a reason (a length floor, and
a refusal of the pure assertions — "fine", "known", "unrelated"); and it covers
ONE repository, so a second red repository still holds the turn and is named.
Every accepted ack is appended to a log, and the refusal prints how many times
this same repository has already been acked — which turns a habit into evidence
without blocking anybody's day.

The legitimate cases, written down so the hatch is not a mystery:
  * the failure is outside this session's changes (a dependency, a vendor
    outage, a workflow broken by an earlier landing);
  * the run can never complete (a removed runner, a workflow awaiting an
    approval nobody will give);
  * a deliberate hold — the fix is known, scheduled and the founder has been
    told.

Exit codes:
    0  the turn may end. stdout may carry ONE {"systemMessage": "..."} line.
    2  the turn is REFUSED. The refusal is on stderr.
    3  this file could not evaluate (the caller announces and allows).
"""

import json
import os
import re
import subprocess
import sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "lib"))
from ci_pause import pause_for
import time
from datetime import datetime, timezone

# ---------------------------------------------------------------------------
# THE NUMBERS, ALL OF THEM, IN ONE PLACE
# ---------------------------------------------------------------------------
# THE WALL-CLOCK BUDGET. Every subprocess this file starts is bounded by what
# is LEFT of it, and when it is gone the answer is "allowed, not established".
BUDGET_SECONDS = 2.0
# A green reading of a finished commit never changes, so it is cached forever.
# A RED one is re-read after this, because `gh run rerun` must be a way out.
RED_RECHECK_SECONDS = 300
# A green reading is re-read after this long, so a later rerun of the same
# commit is seen (P3-15).
GREEN_RECHECK_SECONDS = 600
# NO GRACE-PERIOD GUESS FOR "NO RUNS YET" — REMOVED 2026-09-28, AND WHY.
# This used to promote a commit with zero runs to "running" for
# RUNS_APPEAR_GRACE_SECONDS on the theory that GitHub takes a few seconds to
# create a run after a push, so "no runs" during that window was assumed to
# mean "about to run". That is exactly what broke: a push to a repository with
# NO WORKFLOWS AT ALL (richos-hq has zero, verified via
# `gh api repos/{owner}/{repo}/actions/workflows --jq .total_count` == 0) will
# NEVER produce a run, so the guess printed "CI IS STILL RUNNING ... (no runs
# have appeared yet)" on a turn that had no CI to wait for. The founder hit
# this verbatim (2026-09-28): "What the fuck is this ... The notice was
# false: nothing was running." A hook must never say something is happening
# that is not, so a commit with zero observed runs is now SILENT — the same
# as a green commit falls through this chain today. It costs nothing to get
# right: "none" is never cached (see cache_is_usable) so the very next
# turn-end reads GitHub live again and will report a real run the moment one
# actually appears.
# AN IN-FLIGHT READING IS NEVER REUSED, and the zero says so rather than
# leaving it to be inferred. It was 20 seconds and the suite's case 6c caught
# it: a second turn-end inside that window read "still running" from the cache
# and let the turn end, which is the precise failure the obligation exists to
# prevent — the verdict that arrives late is the one the founder screenshots.
# An in-flight run is rare and short, so the cost of never caching it is one
# API call per turn-end while OUR OWN push is actually being tested, bounded by
# the budget like everything else. That is the cheapest thing on this page.
INFLIGHT_TTL_SECONDS = 0
# The floor on an ack's reason. Twenty-four characters is about six words: long
# enough that "fine" and "known" cannot reach it, short enough to type.
MIN_ACK_REASON = 24
# How many (repository, branch) pairs one turn will judge. Deduplication makes
# this generous; the cap exists so a pathological transcript cannot make the
# budget the only thing standing between the session and a stall.
MAX_TARGETS = 6

# GitHub Actions `conclusion` values that mean "this did not pass". Quoted from
# the wire, and deliberately WITHOUT `skipped`: a skipped run is the never-ran
# question, which `ci-surface.py` owns, and treating it as red here would make
# the gate fire on a state it cannot explain.
NOT_GREEN = ("failure", "cancelled", "timed_out", "startup_failure", "action_required")  # dialect-exempt: GitHub Actions API conclusion enum, quoted verbatim from the wire

# A reason that is only an assertion that it is fine. A bare marker exempts
# nothing, and neither does a bare opinion.
ASSERTION_ONLY = re.compile(
    r"^(it'?s?\s+)?(fine|ok|okay|known|expected|wip|nothing|no\s+problem|not\s+a\s+problem|"
    r"unrelated|not\s+related|not\s+mine|not\s+my\s+(problem|fault)|ignore|ignorable|"
    r"pre-?existing|flaky|flake|will\s+fix|later|todo|n/?a)\W*$",
    re.IGNORECASE,
)

STATE_DIR = os.environ.get(
    "CI_TURN_GATE_STATE_DIR",
    os.path.join(os.path.expanduser("~"), ".claude", "state", "ci-turn-gate"),
)
ACK_LOG = os.environ.get(
    "CI_TURN_GATE_ACK_LOG",
    os.path.join(os.path.expanduser("~"), ".claude", "state", "ci-turn-gate-acks.log"),
)
HOOK_TAG = "(hook: scripts/hooks/guard-ci-turn-gate.sh)"


# ---------------------------------------------------------------------------
# 0. THE DEADLINE
# ---------------------------------------------------------------------------
class Budget(object):
    """The wall-clock budget, as an object so nothing can forget to consult it.

    `left()` is what remains. `spent()` is true when it is gone. Every
    subprocess below is handed `left()` as its timeout, so an unreachable
    endpoint costs the budget ONCE and never the turn.
    """

    def __init__(self, seconds=BUDGET_SECONDS, now=None):
        self.seconds = float(seconds)
        self.start = time.time() if now is None else now
        self.expired_on = []

    def left(self):
        return self.seconds - (time.time() - self.start)

    def spent(self):
        return self.left() <= 0.0

    def note(self, what):
        self.expired_on.append(what)


# rc `run` returns when the budget ran out: an answer that may still arrive,
# never a reason to forget an obligation (P3-13).
TIMEOUT_RC = 124
TRANSIENT = "transient: "


def run(cmd, budget, floor=0.05):
    """A subprocess bounded by what is left of the budget.

    Returns (rc, stdout, err_reason). `err_reason` is non-empty when the call
    did not produce an answer, and it always says WHY in words a reader can
    act on — "timed out", not "failed".
    """
    remaining = budget.left()
    if remaining <= floor:
        return TIMEOUT_RC, "", "the %.1fs budget was already spent" % budget.seconds
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=remaining)
    except FileNotFoundError:
        return 1, "", "`%s` is not on PATH" % cmd[0]
    except subprocess.TimeoutExpired:
        budget.note(" ".join(cmd[:3]))
        return TIMEOUT_RC, "", ("it did not answer inside the %.1fs budget this gate is "
                       "allowed per turn" % budget.seconds)
    except Exception as exc:                                    # pragma: no cover
        return 1, "", "it could not be run (%s)" % exc
    if p.returncode != 0:
        return p.returncode, p.stdout or "", (p.stderr or "").strip()[:200] or "exited %d" % p.returncode
    return 0, p.stdout or "", ""


# ---------------------------------------------------------------------------
# 1. WHAT DID THIS SESSION PUSH?
# ---------------------------------------------------------------------------
# DERIVED FROM THE TRANSCRIPT, NEVER FROM A LIST. A hardcoded set of
# repositories is a second inventory maintained by memory, which is the defect
# `scripts/lib/registered-hooks.sh` exists to argue against at length. The
# transcript records every tool call this session made, with the directory it
# ran in, so "which repositories did this session push to" is a QUESTION ABOUT
# THE RECORD rather than a guess.
#
# It is read INCREMENTALLY: the byte offset reached last turn is kept in the
# session's state file, so turn N+1 reads only what turn N did not. A session
# transcript grows to tens of megabytes and re-reading it every turn-end would
# eat the budget by itself.

_SPLIT = re.compile(r"(?:&&|\|\||[;\n|])")
_PUSH_STRIP = re.compile(r"^-")


def _segments(command):
    """Split a shell command into rough statements. Rough is enough: the only
    question asked of a segment is whether it is a `git push` and where it ran,
    and a segment boundary guessed one token wide changes neither answer."""
    return [s.strip() for s in _SPLIT.split(command or "") if s.strip()]


def _tokens(segment):
    try:
        import shlex
        return shlex.split(segment, comments=False, posix=True)
    except Exception:
        return segment.split()


_ASSIGNMENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
_WRAPPERS = ("command", "exec", "nohup", "time", "sudo", "env", "nice", "builtin")
# Wrapper options that take a VALUE as the next token (`env -u NAME`, `sudo -u
# USER`). The value is not the program (P3-34).
_WRAPPER_VALUE_OPTS = {
    "env": ("-u", "-C", "-S", "--unset", "--chdir"),
    "sudo": ("-u", "-g", "-C", "-D", "-h", "-p", "-R", "-T", "-U"),
}


def _command_word_index(toks):
    """Index of the token that names the program a statement runs, or None."""
    k = 0
    first = True
    while k < len(toks):
        t = toks[k]
        if first:
            t = t.lstrip("({")          # a subshell or group opener glued to the word
            first = False
            if not t:
                k += 1
                first = True
                continue
            toks[k] = t
        if _ASSIGNMENT.match(t):
            k += 1
            continue
        if t in _WRAPPERS:
            k += 1
            valued = _WRAPPER_VALUE_OPTS.get(t, ())
            # a wrapper's own options (env -i, nice -n 5, env -u NAME) are not the command
            while k < len(toks) and (toks[k].startswith("-")
                                     or (k > 0 and toks[k - 1] == "-n" and toks[k].isdigit())
                                     or (k > 0 and toks[k - 1] in valued)
                                     or _ASSIGNMENT.match(toks[k])):
                k += 1
            continue
        return k
    return None


_REDIRECT = re.compile(r"^(\d*|&)(>>?|<)(&\d*-?)?")


def _unreadable(text):
    """A word the shell would expand (`$R`, `$(...)`, a backtick): this gate reads text, it
    cannot know what the word becomes, so it must not guess a repository or branch from it."""
    return "$" in text or "`" in text


def _without_redirects(toks):
    """The words of a command with its redirects removed: `2>&1`, `>/dev/null`, `> file`."""
    out, k = [], 0
    while k < len(toks):
        m = _REDIRECT.match(toks[k])
        if not m:
            out.append(toks[k])
        elif m.end() == len(toks[k]) and not m.group(3):
            k += 1          # a bare `>` or `2>`: the next word is the file, not an argument
        k += 1
    return out


def parse_pushes(command, cwd):
    """Every `git push` in one command string, with the directory it ran in.

    Returns a list of {"dir": <path>, "branch": <str or "">}.

    THE `cd` MATTERS AND IS NOT HYPOTHETICAL. The measured shape of this
    project's own lands is `cd /Users/alex/ab/richos-hq && git push -q origin
    main` issued from a session whose recorded `cwd` is a different repository
    entirely, so reading the record's `cwd` alone would attribute every land to
    the wrong repository.
    """
    out = []
    here = cwd or ""
    for seg in _segments(command):
        toks = _without_redirects(_tokens(seg))
        if not toks:
            continue
        if toks[0] == "cd" and len(toks) >= 2 and not toks[1].startswith("-"):
            here = toks[1] if os.path.isabs(toks[1]) else os.path.join(here, toks[1])
            continue
        # `git` must be the COMMAND WORD of the statement: after any leading
        # VAR=value assignments and transparent wrappers (env, sudo, ...). A
        # `git` that is only an ARGUMENT — `printf '%s\n' git push origin main`,
        # `echo git push` — prints text and pushes nothing (hunt 2026-09-29
        # part 3, finding 11).
        gi = _command_word_index(toks)
        if gi is None or os.path.basename(toks[gi]) != "git":
            continue
        i = gi + 1
        where = here
        while i < len(toks):
            t = toks[i]
            if t == "-C" and i + 1 < len(toks):
                where = toks[i + 1] if os.path.isabs(toks[i + 1]) else os.path.join(here, toks[i + 1])
                i += 2
                continue
            if t in ("-c", "--namespace", "--work-tree", "--git-dir", "--exec-path"):
                i += 2
                continue
            if _PUSH_STRIP.match(t):
                i += 1
                continue
            break
        if i >= len(toks) or toks[i] != "push":
            continue
        rest = toks[i + 1:]
        # A deletion or a tag push triggers nothing this gate can judge.
        if "--delete" in rest or "-d" in rest or "--tags" in rest:
            continue
        positional = []
        j = 0
        while j < len(rest):
            t = rest[j]
            if t in ("--repo", "-o", "--push-option", "--receive-pack", "--exec"):
                j += 2
                continue
            if t.startswith("-"):
                j += 1
                continue
            positional.append(t)
            j += 1
        branch = ""
        remote = positional[0] if positional else "origin"
        if remote.startswith(("http", "git@", "ssh://", "/", ".")):
            remote = "origin"       # a URL pushed to directly has no tracking ref
        # <remote> <refspec>...; the DESTINATION side of `src:dst` is the
        # branch, and EVERY refspec is a push (P3-16).
        branches = []
        for ref in positional[1:]:
            branch = ref.split(":")[-1]
            branch = re.sub(r"^refs/heads/", "", branch)
            if branch in ("HEAD", ""):
                branch = ""
            if branch not in branches:
                branches.append(branch)
        if _unreadable(where) or _unreadable(remote) or any(_unreadable(b) for b in branches):
            continue        # not readable literally: say nothing
        for branch in branches or [branch]:
            out.append({"dir": where, "remote": remote, "branch": branch})
    return out


def observe_pushes(transcript_path, state, budget):
    """Walk the new bytes of the transcript and record every push seen.

    Returns (observations, reason_it_could_not_look).
    """
    tr = state.setdefault("transcript", {})
    if not transcript_path or not os.path.isfile(transcript_path):
        return [], ("no transcript to read at %s — this gate derives the repositories "
                    "from the session's own tool calls and there were none to read"
                    % (transcript_path or "<none given>"))
    try:
        size = os.path.getsize(transcript_path)
    except OSError as exc:
        return [], "the transcript could not be measured (%s)" % exc

    offset = int(tr.get("offset") or 0)
    if tr.get("path") != transcript_path or offset > size:
        offset = 0                       # a different or a truncated transcript
    seen = []
    # A Bash call whose result is an error was refused or failed before it ran
    # its push; it creates no CI obligation (hunt part 3 v3, finding 19).
    refused = set()
    try:
        # READLINE, NEVER `for line in fh`. Iterating a text file disables
        # tell() ("telling position disabled by next() call"), and the byte
        # offset is the whole point of reading incrementally — a scan that
        # cannot record where it stopped re-reads a 40 MB transcript at every
        # turn-end and eats the budget by itself.
        with open(transcript_path, "rb") as fh:
            fh.seek(offset)
            while True:
                if budget.spent():
                    break               # stop reading; the offset stays honest
                raw = fh.readline()
                if not raw:
                    break
                if not raw.endswith(b"\n"):
                    break               # half-written record: leave the offset before it (P3-18)
                offset = fh.tell()
                line = raw.decode("utf-8", "replace").strip()
                if not line:
                    continue
                if ('"Bash"' not in line or "push" not in line) and '"is_error"' not in line:
                    continue
                try:
                    rec = json.loads(line)
                except Exception:
                    continue
                msg = rec.get("message") or {}
                content = msg.get("content")
                if not isinstance(content, list):
                    continue
                cwd = rec.get("cwd") or ""
                ts = _epoch(rec.get("timestamp"))
                for block in content:
                    if (isinstance(block, dict) and block.get("type") == "tool_result"
                            and block.get("is_error") is True and block.get("tool_use_id")):
                        refused.add(block["tool_use_id"])
                        continue
                    if not isinstance(block, dict) or block.get("type") != "tool_use":
                        continue
                    if block.get("name") != "Bash":
                        continue
                    cmd = (block.get("input") or {}).get("command") or ""
                    for p in parse_pushes(cmd, cwd):
                        p["at"] = ts
                        p["_call"] = block.get("id")
                        seen.append(p)
    except OSError as exc:
        return [], "the transcript could not be read (%s)" % exc

    seen = [p for p in seen if not (p.get("_call") and p["_call"] in refused)]
    for p in seen:
        p.pop("_call", None)
    tr["path"] = transcript_path
    tr["offset"] = offset
    return seen, ""


def _epoch(stamp):
    if not stamp:
        return time.time()
    try:
        return datetime.fromisoformat(str(stamp).replace("Z", "+00:00")).timestamp()
    except Exception:
        return time.time()


# ---------------------------------------------------------------------------
# 2. FROM A PUSH TO A COMMIT AND A REPOSITORY SLUG
# ---------------------------------------------------------------------------
# THE COMMIT COMES OFF THE LOCAL REMOTE-TRACKING REF, which `git push` itself
# updated on success. That is the strongest local evidence of "what went", it
# costs no network, and it is a thing the reader can re-run by hand — which is
# why the refusal prints it.

def repo_facts(directory, remote, budget, cache):
    """(repo_root, slug, error) for a directory. Cached per turn."""
    directory = os.path.normpath(directory or "")
    key = (directory, remote)
    if key in cache:
        return cache[key]
    if not directory or not os.path.isdir(directory):
        res = ("", "", "the directory the push ran in no longer exists: %s" % (directory or "<none>"))
        cache[key] = res
        return res
    rc, out, err = run(["git", "-C", directory, "rev-parse", "--show-toplevel"], budget)
    if rc == TIMEOUT_RC:
        return ("", "", TRANSIENT + err)
    if rc != 0:
        res = ("", "", "it is not a git repository (%s): %s" % (err, directory))
        cache[key] = res
        return res
    root = out.strip()
    rc, out, err = run(["git", "-C", root, "remote", "get-url", remote], budget)
    if rc == TIMEOUT_RC:
        return (root, "", TRANSIENT + err)
    if rc != 0:
        res = (root, "", "it has no `%s` remote (%s)" % (remote, err))
        cache[key] = res
        return res
    slug = slug_of(out.strip())
    if not slug:
        res = (root, "", "its `%s` is not a GitHub remote: %s" % (remote, out.strip()))
        cache[key] = res
        return res
    res = (root, slug, "")
    cache[key] = res
    return res


def slug_of(url):
    """owner/name from any GitHub remote spelling, or "" if it is not GitHub."""
    u = (url or "").strip()
    u = re.sub(r"\.git$", "", u)
    m = re.search(r"github\.com[:/]+([^/]+)/([^/]+?)/?$", u)
    if not m:
        return ""
    return "%s/%s" % (m.group(1), m.group(2))


def head_of(root, remote, branch, budget):
    """The commit at the remote tip, as this checkout knows it.

    THE LOCAL BRANCH IS NOT A FALLBACK, and the temptation to make it one is
    the whole reason this note exists. `refs/remotes/<remote>/<branch>` is
    written BY THE PUSH; the local branch is what is sitting here, pushed or
    not. Substituting one for the other would let this gate judge — and name in
    a refusal — a commit that never left the machine. If the remote-tracking
    ref is absent, the honest answer is that nothing here records what went.
    """
    ref = "refs/remotes/%s/%s" % (remote, branch)
    rc, out, _err = run(["git", "-C", root, "rev-parse", "--verify", "--quiet", ref], budget)
    if rc == 0 and out.strip():
        return out.strip(), ""
    if rc == TIMEOUT_RC:
        return "", TRANSIENT + _err
    return "", ("nothing here records which commit was pushed: there is no `%s` in "
                "this checkout" % ref)


def current_branch(root, budget):
    """The checked-out branch; "" when there is none (detached); None when the
    lookup ran out of time, which says nothing about the branch (P3-13)."""
    rc, out, _err = run(["git", "-C", root, "symbolic-ref", "--quiet", "--short", "HEAD"], budget)
    if rc == TIMEOUT_RC:
        return None
    return out.strip() if rc == 0 else ""


# ---------------------------------------------------------------------------
# 3. WHAT DOES GITHUB SAY ABOUT THAT COMMIT?
# ---------------------------------------------------------------------------
def runs_cache_path(slug, sha):
    key = re.sub(r"[^A-Za-z0-9]+", "-", "%s-%s" % (slug, sha))
    return os.path.join(STATE_DIR, "runs", "%s.json" % key)


def read_runs_cache(slug, sha):
    p = runs_cache_path(slug, sha)
    try:
        with open(p, encoding="utf-8") as fh:
            doc = json.load(fh)
    except Exception:
        return None
    doc["age_seconds"] = max(0, int(time.time() - float(doc.get("read_at_epoch") or 0)))
    return doc


def write_runs_cache(doc):
    try:
        os.makedirs(os.path.dirname(runs_cache_path(doc["slug"], doc["sha"])), exist_ok=True)
        p = runs_cache_path(doc["slug"], doc["sha"])
        tmp = p + ".tmp"
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(doc, fh, indent=2)
        os.replace(tmp, p)
    except Exception:
        pass    # a cache that cannot be written costs a call, never an answer


def cache_is_usable(doc):
    """The asymmetry, stated once: GREEN LASTS LONGER THAN RED, NEITHER IS FOREVER.

    A commit whose runs are all completed and green cannot become red on its
    own, but a rerun can change it, so green is re-read after
    GREEN_RECHECK_SECONDS (P3-15).
    A RED reading is re-read after RED_RECHECK_SECONDS because `gh run rerun`
    is a legitimate way out and a gate that could not see one would have to be
    disabled to escape. An in-flight reading is the one state expected to
    change by itself, so it goes stale in seconds.
    """
    if doc is None:
        return False
    st = doc.get("state")
    age = doc.get("age_seconds", 1 << 30)
    if st == "green":
        return age < GREEN_RECHECK_SECONDS
    if st == "red":
        return age < RED_RECHECK_SECONDS
    if st == "running":
        return age < INFLIGHT_TTL_SECONDS
    return False    # "none" and "unknown" are always re-read


def probe_runs(slug, sha, budget):
    """One API call about one commit. Returns the document."""
    path = "repos/%s/actions/runs?head_sha=%s&per_page=50" % (slug, sha)
    rc, out, err = run(["gh", "api", "-H", "Accept: application/vnd.github+json", path], budget)
    now = time.time()
    base = {"slug": slug, "sha": sha, "read_at_epoch": now,
            "read_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")}
    if rc != 0:
        base.update({"state": "unknown", "reason": err, "runs": []})
        return base
    try:
        data = json.loads(out)
    except Exception:
        base.update({"state": "unknown", "reason": "GitHub returned something that is not JSON",
                     "runs": []})
        return base
    runs = []
    for r in data.get("workflow_runs", []) or []:
        runs.append({
            "workflow": (r.get("path") or "").split("/")[-1] or (r.get("name") or ""),
            "name": r.get("name"),
            "status": r.get("status"),
            "conclusion": r.get("conclusion"),
            "run_number": r.get("run_number"),
            "url": r.get("html_url"),
            "id": r.get("id"),
        })
    failed = [r for r in runs if r.get("status") == "completed" and r.get("conclusion") in NOT_GREEN]
    running = [r for r in runs if r.get("status") != "completed"]
    # GREEN IS A RUN THAT SUCCEEDED, not the absence of a red one. Runs that
    # all concluded skipped (or neutral, stale) proved nothing about the
    # commit: that is "none", silent and re-read every turn, never a green
    # cached as passed (hunt part 3 v3, finding 14).
    passed = [r for r in runs if r.get("status") == "completed" and r.get("conclusion") == "success"]
    if failed:
        state = "red"
    elif running:
        state = "running"
    elif passed:
        state = "green"
    else:
        state = "none"
    base.update({"state": state, "runs": runs, "failed": failed, "running": running})
    return base


def verdict_for(slug, sha, budget):
    """The cached-or-live answer, and where it came from."""
    doc = read_runs_cache(slug, sha)
    if cache_is_usable(doc):
        _age = doc.get("age_seconds", 0)
        doc["source"] = ("moments ago, from this gate's cache" if _age < 10
                         else "%d seconds ago, from this gate's cache" % _age)
        return doc
    if budget.spent():
        stale = doc
        doc = {"slug": slug, "sha": sha, "state": "unknown", "runs": [],
               "reason": "the %.1fs budget this gate gets per turn was spent before it "
                         "could be read" % budget.seconds}
        if stale is not None:
            doc["stale_state"] = stale.get("state")
            doc["stale_age"] = stale.get("age_seconds")
        return doc
    live = probe_runs(slug, sha, budget)
    live["source"] = "just now, live"
    if live.get("state") != "unknown":
        write_runs_cache(live)
    elif doc is not None:
        live["stale_state"] = doc.get("state")
        live["stale_age"] = doc.get("age_seconds")
    return live


# ---------------------------------------------------------------------------
# 4. THE ESCAPE HATCH
# ---------------------------------------------------------------------------
_ACK = re.compile(r"^[ \t>*\-]*ci-red-ack:[ \t]*(.+)$", re.IGNORECASE | re.MULTILINE)


def parse_acks(message):
    """[(target, reason, raw)] from the turn's final message."""
    acks = []
    for m in _ACK.finditer(message or ""):
        body = m.group(1).strip().rstrip("`'\"")
        parts = re.split(r"\s*(?:—|–|--|:)\s*", body, maxsplit=1)
        target = parts[0].strip().rstrip(",.").strip()
        reason = parts[1].strip() if len(parts) > 1 else ""
        acks.append((target, reason, body))
    return acks


def ack_is_real(target, reason):
    """"" if the ack stands, otherwise the sentence saying why it does not."""
    if not target:
        return ("it names nothing. A bare `ci-red-ack:` exempts nothing — it has to "
                "name the repository or the workflow it is about.")
    if not reason:
        return ("it names `%s` and gives no reason. The marker is not the ack; the "
                "reason is." % target)
    if len(reason) < MIN_ACK_REASON:
        return ("its reason is %d characters and the floor is %d. Say what is actually "
                "wrong and why this turn cannot fix it." % (len(reason), MIN_ACK_REASON))
    if ASSERTION_ONLY.match(reason):
        return ("its reason (%r) asserts that it is fine rather than saying why. "
                "\"fine\", \"known\", \"flaky\" and \"unrelated\" are refused on purpose: "
                "each of them is the sentence somebody wrote on the day a workflow "
                "started its sixteen-day streak." % reason[:60])
    return ""


def ack_targets(finding):
    """Everything an ack may legitimately name for one red finding."""
    names = {finding["slug"].lower(), finding["slug"].split("/")[-1].lower(),
             os.path.basename(finding["root"]).lower()}
    for r in finding["doc"].get("failed", []) or []:
        if r.get("workflow"):
            names.add(str(r["workflow"]).lower())
        if r.get("name"):
            names.add(str(r["name"]).lower())
    names.discard("")
    return names


def ack_history(slug):
    """How often has THIS repository already been acked here? Read before
    anything is written, so the count in a refusal is history, not this turn."""
    n, first = 0, ""
    try:
        with open(ACK_LOG, encoding="utf-8", errors="replace") as fh:
            for line in fh:
                parts = line.rstrip("\n").split("\t")
                if len(parts) >= 3 and parts[2].lower() == slug.lower():
                    n += 1
                    if not first:
                        first = parts[0][:10]
    except Exception:
        pass
    return n, first


def log_ack(session, slug, sha, target, reason):
    try:
        os.makedirs(os.path.dirname(ACK_LOG), exist_ok=True)
        with open(ACK_LOG, "a", encoding="utf-8") as fh:
            fh.write("%s\t%s\t%s\t%s\t%s\t%s\n" % (
                datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
                session or "-", slug, sha[:12], target,
                " ".join((reason or "").split())[:300]))
    except Exception:
        pass


# ---------------------------------------------------------------------------
# 5. THE WORDS THE FOUNDER READS
# ---------------------------------------------------------------------------
# WRITTEN FOR HIM, NOT FOR AN ENGINEER. The `main-only` ruleset on GitHub earns
# his trust because he can try to push to main and watch it be refused, and the
# refusal tells him what it is and what to do. That is the standard here: WHAT
# is red, WHERE, and WHAT CLEARS IT — three things, in that order, with no
# engine vocabulary in front of them.

def render_refusal(findings, acked, budget):
    L = []
    L.append("CI IS RED ON WHAT THIS SESSION PUSHED — THE TURN IS HELD.")
    L.append("")
    for f in findings:
        doc = f["doc"]
        when = time.strftime("%H:%M", time.localtime(f["at"])) if f.get("at") else "earlier"
        L.append("  %s" % f["slug"])
        for r in doc.get("failed", []) or []:
            L.append("      %-28s %-14s run #%s" % (
                r.get("workflow") or r.get("name") or "(unnamed workflow)",
                (r.get("conclusion") or "failed").upper(), r.get("run_number")))
            if r.get("url"):
                L.append("      %s" % r["url"])
        L.append("      on commit %s, which this session pushed to `%s` at %s."
                 % (f["sha"][:12], f["branch"], when))
        L.append("      Read from GitHub %s." % doc.get("source", "just now, live"))
        L.append("")
    L.append("  None of that is inferred. The commit is the one `git push` sent — you can")
    L.append("  read it back with:")
    L.append("      git -C %s rev-parse %s/%s"
             % (findings[0]["root"], findings[0].get("remote") or "origin", findings[0]["branch"]))
    L.append("  and the verdict is GitHub's own, about that exact commit.")
    L.append("")
    L.append("WHAT CLEARS IT — one of these three, and nothing else:")
    L.append("")
    L.append("  1. FIX IT. Push the fix. The next turn-end reads the new commit and lets")
    L.append("     go by itself, with nothing to type.")
    first_run = None
    for f in findings:
        for r in f["doc"].get("failed", []) or []:
            if r.get("id"):
                first_run = (f["slug"], r["id"])
                break
        if first_run:
            break
    if first_run:
        L.append("         gh run view %s --repo %s --log-failed" % (first_run[1], first_run[0]))
    L.append("")
    L.append("  2. RE-RUN IT, if it failed for something that is not in the code:")
    if first_run:
        L.append("         gh run rerun %s --repo %s --failed" % (first_run[1], first_run[0]))
    L.append("     A red verdict is re-read every %d minutes, so a re-run is noticed."
             % (RED_RECHECK_SECONDS // 60))
    L.append("")
    L.append("  3. SAY WHY IT CAN WAIT, in this reply, on its own line:")
    L.append("")
    for f in findings:
        n, since = ack_history(f["slug"])
        note = ""
        if n:
            note = "   (this would be ack #%d for %s%s)" % (
                n + 1, f["slug"], " since %s" % since if since else "")
        L.append("         ci-red-ack: %s — <why this cannot be fixed in this turn>%s"
                 % (f["slug"].split("/")[-1], note))
    L.append("")
    L.append("     It must name the repository or the workflow above, and the reason has")
    L.append("     to be a reason: \"fine\", \"known\", \"flaky\" and \"unrelated\" are refused.")
    L.append("     Legitimate: the failure is outside this session's changes, the run can")
    L.append("     never complete, or the fix is scheduled and the founder has been told.")
    L.append("     Every ack is written to %s." % ACK_LOG)
    L.append("     ONE ack covers ONE repository; the others still hold the turn.")
    if acked:
        L.append("")
        L.append("  ALREADY ACKED THIS TURN, and therefore not holding anything:")
        for a in acked:
            L.append("      %s — %s" % (a["slug"], a["reason"][:100]))
    if budget.expired_on:
        L.append("")
        L.append("  (One or more reads ran out of this gate's %.1fs budget and were NOT"
                 % budget.seconds)
        L.append("   counted either way. Red above was established before the budget went.)")
    L.append("")
    L.append("This gate reads what THIS session pushed and asks GitHub about exactly those")
    L.append("commits. It is silent when they are green, it does not hold the turn for a run")
    L.append("still in progress, and it lets the turn end when GitHub cannot be reached.")
    return "\n".join(L)


def render_notice(lines):
    if not lines:
        return ""
    return json.dumps({"systemMessage": "\n".join(lines)}, ensure_ascii=False)


# ---------------------------------------------------------------------------
# 6. STATE
# ---------------------------------------------------------------------------
def session_state_path(session_id):
    safe = re.sub(r"[^A-Za-z0-9._-]+", "-", session_id or "no-session")
    return os.path.join(STATE_DIR, "sessions", "%s.json" % safe)


def load_state(session_id):
    try:
        with open(session_state_path(session_id), encoding="utf-8") as fh:
            doc = json.load(fh)
        if isinstance(doc, dict):
            return doc
    except Exception:
        pass
    return {}


def save_state(session_id, state):
    try:
        p = session_state_path(session_id)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        tmp = p + ".tmp"
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(state, fh, indent=2)
        os.replace(tmp, p)
    except Exception:
        pass


# ---------------------------------------------------------------------------
# 7. THE GATE
# ---------------------------------------------------------------------------
def evaluate(payload, budget):
    """(exit_code, stdout_text, stderr_text)."""
    session = str(payload.get("session_id") or "")
    message = str(payload.get("last_assistant_message") or "")
    state = load_state(session)

    seen, cannot = observe_pushes(str(payload.get("transcript_path") or ""), state, budget)

    # The LATEST push wins, per (repository, branch). Push a fix and the broken
    # commit is superseded with no ceremony: this is clause 4 of the predicate.
    pushes = state.setdefault("pushes", {})
    for p in seen:
        key = "%s\t%s\t%s" % (os.path.normpath(p["dir"]), p.get("remote") or "origin",
                              p.get("branch") or "")
        prev = pushes.get(key) or {}
        if float(p.get("at") or 0) >= float(prev.get("at") or 0):
            pushes[key] = {"dir": p["dir"], "remote": p.get("remote") or "origin",
                           "branch": p.get("branch") or "", "at": p.get("at")}

    notices = []
    if cannot and not pushes and pause_for(payload.get("cwd") or os.getcwd()):
        return 0, "", ""
    if cannot and not pushes:
        # Nothing to judge AND no way to look. Say so once per session: a turn
        # that ended without this check having run must not look like a turn
        # that ran it and found nothing.
        if state.get("announced_blind") != cannot:
            state["announced_blind"] = cannot
            notices.append("THE CI TURN GATE COULD NOT LOOK: %s. The turn is not held — it has "
                           "established nothing. %s" % (cannot, HOOK_TAG))
        save_state(session, state)
        return 0, render_notice(notices), ""

    findings, running, unreadable = [], [], []
    facts_cache = {}
    # Never-checked targets first (newest first), then the longest-unchecked, so
    # an older obligation is not starved by newer ones (P3-17).
    # A target goes to the back of that rotation only once it was actually
    # JUDGED this turn (a verdict, a pause, a duplicate of one judged). One
    # that was only SELECTED, then ran out of budget, keeps its place: stamping
    # the whole selected batch up front let one slow lookup starve the other
    # five every turn (P3-17, v3 re-check).
    targets = sorted(pushes.items(), key=lambda kv: (float((kv[1] or {}).get("checked_at") or 0),
                                                     -float((kv[1] or {}).get("at") or 0)))[:MAX_TARGETS]
    _now = time.time()
    judged = set()

    # AN ANSWER THAT CAN NEVER ARRIVE IS NOT A FINDING TO REPEAT.
    #
    # Measured over this project's own transcripts (2026-09-13): one session
    # pushed a shared branch from SIX scratchpad worktrees that have since been
    # removed. Those pushes can never be judged again — the directory is gone,
    # so there is no remote to read a slug from and no ref to read a commit
    # from — and a gate that reported all six every turn for the rest of the
    # session would teach its reader to skip its output, which is the failure
    # mode this whole guard exists to correct. So a PERMANENTLY unjudgeable
    # target is announced ONCE and dropped from the ledger. A TRANSIENT one (a
    # network failure, an exhausted budget) is kept and re-read, because there
    # the answer really may arrive next turn.
    said = state.setdefault("announced_permanent", [])

    def permanent(key, sentence):
        pushes.pop(key, None)
        if sentence not in said:
            said.append(sentence)
            del said[:-40]
            unreadable.append(sentence)

    for key, push in targets:
        remote = push.get("remote") or "origin"
        root, slug, err = repo_facts(push["dir"], remote, budget, facts_cache)
        if slug and pause_for(slug):
            # Keep the push history for restoration; do not read CI or demand an ack.
            push["checked_at"] = _now
            continue
        if err.startswith(TRANSIENT):
            unreadable.append("%s: %s" % (os.path.basename(push["dir"] or "?"), err[len(TRANSIENT):]))
            continue
        if err:
            permanent(key, "%s: %s" % (os.path.basename(push["dir"] or "?"), err))
            continue
        branch = push.get("branch") or current_branch(root, budget)
        if branch is None:
            # Out of time, not unknowable: keep the push and read it next turn.
            unreadable.append("%s: the pushed branch could not be read inside the budget" % slug)
            continue
        if not branch:
            permanent(key, "%s: the branch that was pushed could not be determined" % slug)
            continue
        if (slug, branch) in judged:
            push["checked_at"] = _now
            continue
        judged.add((slug, branch))
        sha, err = head_of(root, remote, branch, budget)
        if err.startswith(TRANSIENT):
            judged.discard((slug, branch))
            unreadable.append("%s: %s" % (slug, err[len(TRANSIENT):]))
            continue
        if err:
            permanent(key, "%s: %s" % (slug, err))
            continue
        doc = verdict_for(slug, sha, budget)
        item = {"slug": slug, "root": root, "remote": remote, "branch": branch,
                "sha": sha, "at": push.get("at"), "doc": doc}
        st = doc.get("state")
        if st != "unknown":
            push["checked_at"] = _now
        if st == "red":
            findings.append(item)
        elif st == "running":
            running.append(item)
        elif st == "none":
            # ZERO OBSERVED RUNS IS NEVER "RUNNING" — silent, exactly like a
            # green commit falls through this chain with no branch of its own.
            # This covers a repository with no workflows at all (richos-hq),
            # a workflow GitHub has not yet indexed the run for, and CI paused
            # by any other means: none of those are "in flight", so none of
            # them are announced as if they were. See the note above
            # RED_RECHECK_SECONDS-adjacent constants for why this is safe to
            # do silently: "none" is re-read live on every turn-end.
            pass
        elif st == "unknown":
            unreadable.append("%s at %s: %s" % (slug, sha[:12], doc.get("reason") or "no reason given"))

    # --- the hatch ---------------------------------------------------------
    acked, rejected = [], []
    if findings:
        parsed = parse_acks(message)
        still = []
        for f in findings:
            names = ack_targets(f)
            covered = None
            for target, reason, _raw in parsed:
                if target.lower() not in names:
                    continue
                why_not = ack_is_real(target, reason)
                if why_not:
                    rejected.append("`ci-red-ack: %s` was NOT accepted: %s" % (target, why_not))
                    continue
                covered = (target, reason)
                break
            if covered:
                log_ack(session, f["slug"], f["sha"], covered[0], covered[1])
                acked.append({"slug": f["slug"], "reason": covered[1]})
            else:
                still.append(f)
        # An ack that named nothing red at all is worth saying out loud: it is
        # the shape of an ack typed from memory against a repository that was
        # fixed an hour ago.
        for target, reason, _raw in parsed:
            if not any(target.lower() in ack_targets(f) for f in findings):
                why_not = ack_is_real(target, reason)
                rejected.append("`ci-red-ack: %s` names nothing that is red here%s"
                                % (target, "" if not why_not else " (and %s)" % why_not))
        findings = still

    save_state(session, state)

    if findings:
        out = render_refusal(findings, acked, budget)
        if rejected:
            out = out + "\n\nREJECTED THIS TURN:\n  " + "\n  ".join(rejected)
        return 2, "", out

    # --- allowed, and never silently -------------------------------------
    for item in running:
        n = len(item["doc"].get("running", []) or [])
        notices.append(
            "CI IS STILL RUNNING on what this session pushed — %s `%s` at %s%s. THE TURN IS "
            "NOT HELD for it. The next turn-end reads the verdict and holds the turn if it "
            "failed." % (item["slug"], item["branch"], item["sha"][:12],
                         " (%d run(s) in flight)" % n if n else " (no runs have appeared yet)"))
    for u in unreadable:
        notices.append("CI COULD NOT BE READ for %s. The turn is NOT held — \"could not look\" "
                       "is not evidence of red. %s" % (u, HOOK_TAG))
    for a in acked:
        notices.append("CI RED ACKED: %s — %s. Logged to %s." % (a["slug"], a["reason"][:160], ACK_LOG))
    for r in rejected:
        notices.append(r)
    return 0, render_notice(notices), ""


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    # --entity is accepted and deliberately unused for anything but jurisdiction,
    # which the wrapper has already settled by the time this runs.
    if "--entity" in argv:
        i = argv.index("--entity")
        del argv[i:i + 2]
    raw = sys.stdin.read()
    try:
        payload = json.loads(raw) if raw.strip() else {}
    except Exception:
        payload = {}
    if not isinstance(payload, dict):
        payload = {}
    budget = Budget(float(os.environ.get("CI_TURN_GATE_BUDGET_SECONDS") or BUDGET_SECONDS))
    try:
        rc, out, err = evaluate(payload, budget)
    except Exception as exc:                                    # pragma: no cover
        sys.stderr.write("guard-ci-turn-gate.py could not evaluate: %r\n" % (exc,))
        return 3
    if out:
        sys.stdout.write(out + "\n")
    if err:
        sys.stderr.write(err + "\n")
    return rc


if __name__ == "__main__":
    sys.exit(main())
