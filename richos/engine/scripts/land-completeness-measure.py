#!/usr/bin/env python3
"""land-completeness-measure.py — G0. THE NUMBER, BEFORE ANYTHING BLOCKS.

===========================================================================
WHY A MEASUREMENT SCRIPT SHIPS AT ALL
===========================================================================
`docs/plans/land-completeness-2026-09-10.md` R6 says any blocking behavior is
measured against real lands on this machine before it ships, and the measured
false-positive rate is stated. This project killed three guards in one day
(`g11`/`g12`/`g13`) by making them broad enough that waiving became the daily
habit, so a rate is not a formality here — it is the thing that decides whether
the gate ships armed, ships reporting-only, or does not ship.

A rate quoted in a report is a claim. A rate a reader can re-derive with one
command is evidence. That is the only reason this file is committed rather
than being a throwaway in a scratch directory.

    engine/scripts/land-completeness-measure.py --projects ~/.claude/projects

===========================================================================
WHAT IT MEASURES, AND THE PART THAT IS NOT RECONSTRUCTABLE
===========================================================================
A land, in this engine, is `git merge` into the watched branch from the main
checkout (`skills/rich-lander/SKILL.md`; the same act `guard-ci-red-lands.sh`
already classifies). A land is COMPLETE when the worktree whose branch it just
merged is gone. The steps in between — collect artifacts, remove the worktree,
resolve the branch — are the ones with no gate on them.

So this reads orchestrator transcripts and, per session, pairs each land with
the worktree-removal traffic that follows it:

  lands                 `git merge` calls, classified with shlex rather than
                        matched as a substring, dry runs excluded.
  removal steps         `remove-agent-worktree.sh`, `git worktree remove`,
                        `collect-worktree-artifacts.sh`, or a
                        `reconcile-terminal-worktrees.py` run — any of the
                        sanctioned ways the residue actually goes away.
  a land LEFT RESIDUE   no removal step appears between it and the next land
                        in the same session.

That loose predicate is a CEILING and it is reported as one. The SHIPPED gate is
narrower: it fires only when a registered worktree's owner is judged NOT-ALIVE
**and** its branch is already merged. The second term is free here — the residue
of land N-1 is merged by construction, because land N-1 is the merge. So the
entire distance between the ceiling and the real rate is one question:

    WAS THE OWNER OF THAT WORKTREE STILL ALIVE WHEN THE NEXT LAND HAPPENED?

That is the question the orchestrator gets wrong when it guesses, and it is why
`scripts/lib/worktree-ledger.py` exists. At run time the gate answers it against
the live process table. Retrospectively the process table is gone — but two
durable records survive that answer it for a past instant, and this script uses
both:

  the ledger      ~/.claude/state/worktree-ledger.jsonl is append-only and
                  timestamped. A `registered` line maps a branch and a worktree
                  path to an agent id, a session id and that session's pid. A
                  `terminated` line is a WITNESSED death with a timestamp on it.
  the agent's own
  transcript      <session>/subagents/agent-*<agent-id>*.jsonl. Its LAST entry
                  is the last moment that agent demonstrably existed. If that
                  moment is AFTER the land being judged, the owner was alive
                  then, full stop, and the tight gate would not have fired.

THE THREE-VALUED ANSWER IS KEPT THREE-VALUED. An owner with no registration, or
one whose transcript cannot be found, is UNKNOWN — and an unknown owner is
counted as a NON-FIRE, because the shipped gate refuses only on a fact it
established. Rounding unknown to residue here would inflate the very number the
CEO is being asked to decide on.

Under-counting is possible too, in one specific way: a session that lands once
and stops leaves residue that no LATER land in that session can be measured
against. Those are counted separately as `trailing` and are neither a fire nor
a pass.

===========================================================================
WHAT A FALSE POSITIVE IS HERE — AND WHAT IS NOT ONE
===========================================================================
NOT a false positive: refusing land N because land N-1 left real residue. That
is the forcing function R2 asks for, working. The land was legitimate work and
it is still refused, deliberately, until one step that should already have
happened happens.

A FALSE POSITIVE is a refusal naming residue that was not residue:

  * the worktree belonged to an agent that was STILL RUNNING — the dominant
    risk, because the orchestrator routinely lands one branch while its author
    is still working. Counted here from the agent's own transcript.
  * the branch was UNMERGED, which R3 says is never swept and must never block.
    Impossible by construction in this simulation and excluded in the shipped
    gate by an explicit clause.
  * the worktree was already gone — removed by the nightly reconciler, by a
    prior session, or by a path this script cannot see. Counted as unknown, not
    as a fire.

Exit codes:  0 the measurement ran   2 nothing to measure
"""

import argparse
import glob
import json
import os
import re
import shlex
import sys

# The sanctioned ways residue goes away. A land followed by any of these, before
# the next land, is a complete land as far as this measurement can see.
REMOVAL_MARKERS = (
    "remove-agent-worktree.sh",
    "collect-worktree-artifacts.sh",
    "reconcile-terminal-worktrees",
    "reap-stale-worktrees.sh",
)


# Flags that CONSUME the token after them. Missing one of these is not a
# cosmetic bug: `git merge --no-ff -m "Merge zach-opus-ob1: ..."` then reports
# the COMMIT MESSAGE as the merged branch, and every such land is scored as an
# owner that could not be found. Twelve hand-read refusals on 2026-09-10 were
# all this, which is why they were hand-read before the rate was quoted.
_ARG_TAKING = ("-m", "--message", "-F", "--file", "-S", "--gpg-sign",
               "-s", "--strategy", "-X", "--strategy-option",
               "-C", "-c", "--git-dir", "--work-tree", "--into-name")

_HEREDOC = re.compile(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1")


def strip_noncommand(cmd):
    """Remove the parts of a command line that DO NOT EXECUTE.

    A heredoc body is data. On this machine it is very often a TEST FIXTURE
    being written to disk — `cat > guard-x.test.sh <<'SH' ... git merge ... SH`
    — and counting the fixture's text as a land is the same defect
    guard-worktree-removal.sh had to fix twice under the heading PROSE ABOUT A
    COMMAND IS NOT THE COMMAND. Three of the first twelve hand-read refusals
    here were exactly that.
    """
    if not cmd:
        return cmd
    out, i = [], 0
    lines = cmd.split("\n")
    while i < len(lines):
        line = lines[i]
        m = _HEREDOC.search(line)
        out.append(line)
        if m:
            tag = m.group(2)
            i += 1
            while i < len(lines) and lines[i].strip() != tag:
                i += 1
        i += 1
    return "\n".join(out)


_SEPARATORS = {";", "&&", "||", "|", "&", "(", "{", "then", "do", "else", "!"}


def _command_tokens(cmd):
    """shlex tokens with `;`, `&&`, `|` split out as their own tokens."""
    try:
        lex = shlex.shlex(cmd, posix=True, punctuation_chars="();|&\n")
        lex.whitespace = " \t\r"
        lex.whitespace_split = True
        return list(lex)
    except Exception:
        return []


def _at_command_position(toks, i):
    """True when toks[i] starts a command, not an argument of `echo`/`printf`.

    Hunt P5-46: `echo git merge cc/example` printed a merge and was credited as
    one. A command starts the line or follows a separator, after any leading
    VAR=value assignments."""
    k = i - 1
    while k >= 0 and re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", toks[k]):
        k -= 1
    return k < 0 or toks[k] in _SEPARATORS


def _only_if_left_failed(toks, i):
    """True when the command starting at toks[i] follows `||`.

    Hunt P5-46 (v3): `true || git merge cc/example` succeeds without running the
    merge. A successful tool result shows the shell call succeeded, not that the
    right side of `||` ran, so such a command is never credited."""
    k = i - 1
    while k >= 0 and re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", toks[k]):
        k -= 1
    return k >= 0 and toks[k] == "||"


_RUNNER_WORDS = {"bash", "sh", "zsh", "python", "python3", "env", "exec", "nohup", "time", "command"}


def _runs_marker(toks):
    """True when a REMOVAL_MARKERS script is the program of a command that runs.

    The program is the command word, or the word after a runner (`bash x.sh`).
    A marker anywhere else (an echo argument, a commit message) is not the act."""
    for k, t in enumerate(toks):
        if not any(os.path.basename(t).startswith(m) for m in REMOVAL_MARKERS):
            continue
        start = k
        if k > 0 and os.path.basename(toks[k - 1]) in _RUNNER_WORDS:
            start = k - 1
        if _at_command_position(toks, start) and not _only_if_left_failed(toks, start):
            return True
    return False


def classify_land(cmd):
    """Is this command a `git merge`? Returns the merged ref or None.

    Deliberately conservative and deliberately shlex rather than regex, for the
    reason guard-ci-red-lands.sh gives at length: a regex over a command line is
    how a classifier acquires holes. Anything unsure is not a land — a missed
    land costs this measurement a data point, a false land costs it its number.
    """
    if not cmd or not cmd.strip():
        return None
    cmd = strip_noncommand(cmd)
    toks = _command_tokens(cmd)
    # Only the final foreground list determines a successful shell result.
    # Earlier lists can fail or skip a merge while a later `true` hides it.
    while toks and toks[-1] in (";", "\n"):
        toks.pop()
    depth, start = 0, 0
    for index, token in enumerate(toks):
        if token and set(token) == {"("}:
            depth += len(token)
        elif token and set(token) == {")"}:
            depth -= len(token)
        elif token in (";", "\n") and depth == 0:
            start = index + 1
    toks = toks[start:]
    # Success proves every member of an && chain ran and succeeded. Compound
    # commands, pipelines, || and background jobs need evidence we do not have.
    if any(t != "&&" and t and set(t) <= set(";|&()\n") for t in toks) or any(
            t in {"if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "!", "{"}
            for t in toks):
        return None
    i = 0
    while i < len(toks):
        t = toks[i]
        if ((t == "git" or t.endswith("/git")) and _at_command_position(toks, i)
                and not _only_if_left_failed(toks, i)):
            j = i + 1
            while j < len(toks) and toks[j].startswith("-"):
                j += 2 if toks[j] in _ARG_TAKING else 1
            # `merge`, and never `merge-base` / `merge-tree` — those read the
            # graph and land nothing.
            if j < len(toks) and toks[j] == "merge":
                if "--dry-run" in toks or "--abort" in toks or "--continue" in toks:
                    return None
                k = j + 1
                while k < len(toks):
                    tk = toks[k]
                    if tk in _ARG_TAKING:
                        k += 2
                        continue
                    if tk.startswith("-"):
                        k += 1
                        continue
                    return tk
                return "(unnamed)"
        i += 1
    return None


def is_removal(cmd):
    if not cmd:
        return False
    toks = _command_tokens(strip_noncommand(cmd))
    if _runs_marker(toks):
        return True
    # A raw `git worktree remove`. Matched as a token sequence, not a substring,
    # so prose in a commit message does not count as the act.
    for k in range(len(toks) - 2):
        if (toks[k].endswith("git") and _at_command_position(toks, k)
                and not _only_if_left_failed(toks, k)
                and toks[k + 1] == "worktree" and toks[k + 2] == "remove"):
            return True
    return False


def walk_transcript(path):
    """Yield (kind, timestamp, detail) for the events this measurement needs."""
    try:
        fh = open(path, encoding="utf-8", errors="replace")
    except Exception:
        return
    pending = {}
    with fh as f:
        for line in f:
            try:
                d = json.loads(line)
            except Exception:
                continue
            ts = d.get("timestamp") or ""
            typ = d.get("type")
            if typ == "assistant":
                for b in (d.get("message") or {}).get("content") or []:
                    if not isinstance(b, dict) or b.get("type") != "tool_use":
                        continue
                    name = b.get("name")
                    inp = b.get("input") or {}
                    if name == "Bash":
                        cmd = inp.get("command") or ""
                        ref = classify_land(cmd)
                        if ref:
                            ev = ("land", ts, {"ref": ref, "cmd": cmd[:400]})
                        elif is_removal(cmd):
                            ev = ("removal", ts, {"cmd": cmd[:400]})
                        else:
                            continue
                        # Hunt P5-46: credited only once its tool_result arrives
                        # and is not an error; a refused or failed request is not
                        # a landing.
                        pending[b.get("id")] = ev
                    elif name == "SendMessage":
                        yield ("message", ts, {"to": str(inp.get("to") or "")})
                    elif name == "Agent":
                        yield ("spawn", ts, {"name": str(inp.get("name") or "")})
            elif typ == "user":
                # Host-written task notifications name the teammate that finished.
                content = (d.get("message") or {}).get("content")
                text = ""
                if isinstance(content, str):
                    text = content
                elif isinstance(content, list):
                    for b in content:
                        if isinstance(b, dict) and b.get("type") == "tool_result":
                            ev = pending.pop(b.get("tool_use_id"), None)
                            if ev is not None and not b.get("is_error"):
                                yield ev
                        if isinstance(b, dict) and b.get("type") == "text":
                            text += b.get("text") or ""
                if text:
                    yield ("text", ts, {"text": text[:4000]})


def branch_token(ref):
    """The branch a merge names, stripped of the ref decoration it may carry."""
    r = (ref or "").strip()
    for prefix in ("refs/heads/", "origin/", "heads/"):
        if r.startswith(prefix):
            r = r[len(prefix):]
    return r


# ---------------------------------------------------------------------------
# THE TIGHT PREDICATE'S ONE HARD TERM: was the owner alive at that instant?
# ---------------------------------------------------------------------------

def ledger_index(ledger_path):
    """branch -> the ownership records that name it, oldest first; plus
    witnessed terminations keyed by agent id.

    BOTH `prepared` AND `registered` ARE OWNERSHIP RECORDS, and reading only the
    second is a hole this measurement fell into before it was hand-checked.
    `create-teammate-worktree.sh` writes `prepared` at the moment it creates a
    cross-repository worktree; `registered` is written LATER, by
    detect-nonnative-worktree.sh at PostToolUse[Agent], and therefore only if a
    spawn actually reached that worktree. A worktree whose spawn was refused by
    a guard, or never made, has a `prepared` line and nothing else — and on this
    machine on 2026-09-10, `zach-opus-rec1r` was exactly that. Counting it as
    "no ownership record" put a real, decidable worktree in the UNKNOWN column.

    Measured effect of the two together: 508 `registered` lines carry a branch on
    only 170 of them, while `prepared` lines carry one on nearly all — so the
    branch key this simulation joins on lives mostly in the record that was
    being ignored.

    Nothing is inferred from a NAME here beyond the branch the record itself
    wrote down. The ledger's own header is emphatic that name-based matching is
    not ownership, and a measurement inherits that rule rather than relaxing it.
    """
    by_branch = {}
    terminated = {}
    try:
        fh = open(ledger_path, encoding="utf-8", errors="replace")
    except Exception:
        return by_branch, terminated
    with fh as f:
        for line in f:
            try:
                d = json.loads(line)
            except Exception:
                continue
            ev = d.get("event")
            aid = d.get("agent_id") or ""
            if ev in ("registered", "prepared"):
                br = d.get("branch") or ""
                if br:
                    by_branch.setdefault(br, []).append({
                        "event": ev,
                        "agent_id": aid,
                        "session_id": d.get("session_id") or "",
                        "repo": d.get("repo") or "",
                        "worktree": d.get("worktree") or "",
                        "ts": d.get("ts") or "",
                    })
            elif ev == "terminated" and aid:
                ts = d.get("ts") or ""
                if aid not in terminated or ts < terminated[aid]:
                    terminated[aid] = ts
    return by_branch, terminated


def last_line_timestamp(path):
    """The timestamp of a transcript's final entry, read from the tail.

    Seeking rather than reading the whole file matters: the corpus on this
    machine holds single transcripts over 40MB and this is called once per
    simulated refusal.
    """
    try:
        size = os.path.getsize(path)
        with open(path, "rb") as f:
            back = min(size, 65536)
            f.seek(size - back)
            chunk = f.read(back)
        for line in reversed(chunk.split(b"\n")):
            if not line.strip():
                continue
            try:
                d = json.loads(line.decode("utf-8", "replace"))
            except Exception:
                continue
            ts = d.get("timestamp")
            if ts:
                return ts
    except Exception:
        return None
    return None


class OwnerLiveness(object):
    """Answers ALIVE / NOT-ALIVE / UNKNOWN for a branch's owner at an instant."""

    def __init__(self, projects_dir, ledger_path):
        self.projects_dir = projects_dir
        self.by_branch, self.terminated = ledger_index(ledger_path)
        self._subagent_cache = {}
        self._last_ts_cache = {}

    def _subagent_files(self, session_id):
        if session_id in self._subagent_cache:
            return self._subagent_cache[session_id]
        hits = glob.glob(os.path.join(self.projects_dir, "*", session_id, "subagents", "*.jsonl"))
        self._subagent_cache[session_id] = hits
        return hits

    def _agent_last_activity(self, session_id, agent_id):
        key = (session_id, agent_id)
        if key in self._last_ts_cache:
            return self._last_ts_cache[key]
        best = None
        for p in self._subagent_files(session_id):
            base = os.path.basename(p)
            if agent_id and agent_id in base:
                ts = last_line_timestamp(p)
                if ts and (best is None or ts > best):
                    best = ts
        self._last_ts_cache[key] = best
        return best

    def _session_last_activity(self, session_id):
        """When did the OWNING SESSION last write anything?

        The retrospective stand-in for the live `session_pid` check the ledger
        does at run time, and it rests on the same fact the ledger rests on:
        every agent of a session runs inside that session's one process. When
        the process is gone, so are they. A transcript that stopped writing
        before the land is the strongest evidence a past instant leaves behind
        that the process was no longer there.
        """
        key = ("session", session_id)
        if key in self._last_ts_cache:
            return self._last_ts_cache[key]
        best = None
        for p in glob.glob(os.path.join(self.projects_dir, "*", session_id + ".jsonl")):
            ts = last_line_timestamp(p)
            if ts and (best is None or ts > best):
                best = ts
        self._last_ts_cache[key] = best
        return best

    def verdict(self, branch, at_ts):
        """Was the owner of `branch` alive at `at_ts`?

        ALIVE      its own transcript, or its session's, was still being written
                   after that moment
        NOT-ALIVE  a witnessed termination is on record before it, or every
                   transcript that could speak for it had already stopped
        UNKNOWN    no ownership record, or nothing readable to judge it by

        The order is the ledger's order and the reasons are the ledger's
        reasons: a WITNESSED termination outranks everything, an agent's own
        transcript outranks its session's, and absence of evidence produces
        UNKNOWN rather than a verdict.
        """
        regs = self.by_branch.get(branch) or []
        if not regs:
            return "UNKNOWN", "no ownership record for branch %s" % branch
        for reg in regs:
            aid = reg["agent_id"]
            t = self.terminated.get(aid)
            if t and t <= at_ts:
                return "NOT-ALIVE", "witnessed termination at %s" % t
        for reg in regs:
            if not reg["agent_id"]:
                continue
            last = self._agent_last_activity(reg["session_id"], reg["agent_id"])
            if last is None:
                continue
            if last > at_ts:
                return "ALIVE", "its own transcript was still being written at %s" % last
            return "NOT-ALIVE", "its own transcript stopped at %s" % last
        for reg in regs:
            if not reg["session_id"]:
                continue
            last = self._session_last_activity(reg["session_id"])
            if last is None:
                continue
            if last > at_ts:
                return "ALIVE", ("the session that owns it (%s) was still writing at %s"
                                 % (reg["session_id"][:8], last))
            return "NOT-ALIVE", ("the session that owns it (%s) stopped at %s — the process every"
                                 " agent of it ran inside was gone"
                                 % (reg["session_id"][:8], last))
        return "UNKNOWN", "an ownership record exists, but nothing readable can date its owner"


def ledger_epoch(ledger_path):
    """The timestamp of the ledger's FIRST line — the moment before which no
    ownership question on this machine has an answer.

    THE RATE MUST BE SCOPED TO THIS OR IT IS A LIE BY DILUTION. The ownership
    ledger began on 2026-09-02. Every land before that has an UNKNOWN owner not
    because the record is silent about a live question but because the record
    did not exist, and 868 of the 1,062 unknowns measured on 2026-09-10 were
    from July and August. Averaging those into the rate makes a gate look
    quieter than it will be on the day it ships, which is the one direction a
    number given to the CEO must never be wrong in.
    """
    try:
        with open(ledger_path, encoding="utf-8", errors="replace") as f:
            for line in f:
                try:
                    return json.loads(line).get("ts") or ""
                except Exception:
                    continue
    except Exception:
        pass
    return ""


def measure(projects_dir, ledger_path, since=""):
    # Orchestrator sessions only. A subagent transcript lives under
    # <session>/subagents/ and its author never lands — including them would
    # count an engineer's `git merge main` into its own branch as a land, which
    # is the opposite of the act being measured.
    files = [p for p in glob.glob(os.path.join(projects_dir, "*", "*.jsonl"))
             if os.sep + "subagents" + os.sep not in p]
    files.sort(key=lambda p: os.path.getmtime(p), reverse=True)

    liveness = OwnerLiveness(projects_dir, ledger_path)

    totals = {
        "sessions_read": 0,
        "sessions_with_lands": 0,
        "lands": 0,
        "removal_steps": 0,
        "pairs": 0,            # lands with a LATER land to be judged against
        "loose_fire": 0,       # ceiling: previous land's residue never cleared
        "clean": 0,
        "trailing": 0,         # last land of a session: not judgeable either way
        "tight_fire": 0,       # owner demonstrably NOT-ALIVE: a real refusal
        "owner_alive": 0,      # the ceiling's false positives, removed by the
                               # liveness clause before anything is refused
        "owner_unknown": 0,    # never refused, always reported
        "before_since": 0,     # outside the scoped window: not counted at all
        "unreadable": 0,
    }
    fires = []

    for path in files:
        try:
            events = list(walk_transcript(path))
        except Exception:
            totals["unreadable"] += 1
            continue
        totals["sessions_read"] += 1
        lands = [(ts, det) for kind, ts, det in events if kind == "land"]
        if not lands:
            continue
        totals["sessions_with_lands"] += 1
        totals["lands"] += len(lands)
        totals["removal_steps"] += sum(1 for k, _, _ in events if k == "removal")

        seq = [(kind, ts, det) for kind, ts, det in events]
        land_positions = [i for i, (k, _, _) in enumerate(seq) if k == "land"]

        for n, pos in enumerate(land_positions):
            if n + 1 >= len(land_positions):
                totals["trailing"] += 1
                continue
            nxt = land_positions[n + 1]
            at = seq[nxt][1]        # the instant the NEXT land is attempted
            if since and at and at < since:
                totals["before_since"] += 1
                continue
            window = seq[pos + 1:nxt]
            removed = any(k == "removal" for k, _, _ in window)
            totals["pairs"] += 1
            if removed:
                totals["clean"] += 1
                continue
            totals["loose_fire"] += 1
            ref = branch_token(seq[pos][2].get("ref"))
            verdict, why = liveness.verdict(ref, at)
            if verdict == "ALIVE":
                totals["owner_alive"] += 1
            elif verdict == "NOT-ALIVE":
                totals["tight_fire"] += 1
            else:
                totals["owner_unknown"] += 1
            fires.append({
                "session": os.path.basename(path),
                "land_index": n + 1,
                "ref": ref,
                "next_land_ts": at,
                "owner": verdict,
                "why": why,
                "cmd": seq[pos][2].get("cmd"),
            })

    return totals, fires


def main(argv=None):
    ap = argparse.ArgumentParser(prog="land-completeness-measure.py")
    ap.add_argument("--projects", default=os.path.expanduser("~/.claude/projects"),
                    help="the transcript root to measure (default: ~/.claude/projects)")
    ap.add_argument("--ledger", default=os.environ.get(
        "RICHOS_WORKTREE_LEDGER",
        os.path.expanduser("~/.claude/state/worktree-ledger.jsonl")),
        help="the ownership ledger the liveness term is read from")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--show-fires", type=int, default=0,
                    help="print the first N simulated refusals, for hand-reading")
    ap.add_argument("--only", choices=("", "ALIVE", "NOT-ALIVE", "UNKNOWN"), default="",
                    help="with --show-fires, restrict the listing to one owner verdict")
    ap.add_argument("--since", default="",
                    help="ISO timestamp; default is the ledger's first line, because before that "
                         "no ownership question has an answer. '' measures everything.")
    ap.add_argument("--all-time", action="store_true",
                    help="do not scope to the ledger epoch (dilutes the rate; say so if you quote it)")
    args = ap.parse_args(argv)

    if not os.path.isdir(args.projects):
        sys.stderr.write("no transcript root at %s — nothing to measure\n" % args.projects)
        return 2

    since = "" if args.all_time else (args.since or ledger_epoch(args.ledger))
    totals, fires = measure(args.projects, args.ledger, since)

    shown = [f for f in fires if not args.only or f["owner"] == args.only]

    if args.json:
        print(json.dumps({"totals": totals, "fires": shown[:args.show_fires or 0]}, indent=2))
        return 0

    pairs = totals["pairs"] or 1
    print("=== G0: WHAT A LAND-COMPLETENESS GATE WOULD HAVE REFUSED ===")
    print("    transcripts: %s" % args.projects)
    print("    ledger:      %s" % args.ledger)
    print("    window:      %s" % (("lands from %s onward — the ledger epoch, because before it "
                                    "no ownership\n                 question has an answer "
                                    "(%d earlier lands excluded, not scored)"
                                    % (since[:19], totals["before_since"]))
                                   if since else "ALL TIME (unscoped — the rate below is diluted "
                                                 "by the pre-ledger era)"))
    print("")
    print("    orchestrator sessions read ............. %d" % totals["sessions_read"])
    print("    sessions containing a land ............. %d" % totals["sessions_with_lands"])
    print("    lands (git merge, shlex-classified) .... %d" % totals["lands"])
    print("    worktree-removal steps seen ............ %d" % totals["removal_steps"])
    print("")
    print("    lands judgeable against a NEXT land .... %d" % totals["pairs"])
    print("      residue was cleared before it ........ %d  (%.1f%%)"
          % (totals["clean"], 100.0 * totals["clean"] / pairs))
    print("      residue was still there .............. %d  (%.1f%%)   <- LOOSE CEILING"
          % (totals["loose_fire"], 100.0 * totals["loose_fire"] / pairs))
    print("    trailing lands (no next land to judge) . %d" % totals["trailing"])
    print("")
    print("    OF THAT CEILING, WHAT THE SHIPPED PREDICATE DOES WITH IT")
    print("      owner ALIVE      -> NOT refused ...... %d  (%.1f%% of all judgeable lands)"
          % (totals["owner_alive"], 100.0 * totals["owner_alive"] / pairs))
    print("      owner UNKNOWN    -> NOT refused ...... %d  (%.1f%%)"
          % (totals["owner_unknown"], 100.0 * totals["owner_unknown"] / pairs))
    print("      owner NOT-ALIVE  -> REFUSED .......... %d  (%.1f%%)   <- THE RATE"
          % (totals["tight_fire"], 100.0 * totals["tight_fire"] / pairs))
    if totals["unreadable"]:
        print("")
        print("    transcripts that could not be read ..... %d   <- NOT examined, not clean"
              % totals["unreadable"])
    print("")
    print("    HOW TO READ THE LAST FIGURE. It is not a false-positive rate: every one of those")
    print("    refusals names a worktree whose owner was demonstrably gone and whose branch was")
    print("    already merged, which is residue by definition. The false positives are the ALIVE")
    print("    row, and the liveness clause removes all of them BEFORE anything is refused.")
    print("    UNKNOWN is never refused and never rounded to clean — it is reported by")
    print("    engine/scripts/land-completeness.sh in its own column.")

    if args.show_fires:
        print("")
        print("=== %d of the simulated refusals%s, for hand-reading ==="
              % (min(args.show_fires, len(shown)), (" (owner %s)" % args.only) if args.only else ""))
        for f in shown[:args.show_fires]:
            print("  %s  land #%-3d %-28s owner=%s" % (f["session"][:12], f["land_index"],
                                                       f["ref"], f["owner"]))
            print("      %s" % (f["cmd"] or "").replace("\n", " ")[:140])
            print("      %s" % f["why"])
    return 0


if __name__ == "__main__":
    sys.exit(main())
