#!/usr/bin/env python3
"""left-off.py -- WHERE HE LEFT OFF, ASSEMBLED FROM THINGS THAT CANNOT LIE.

===========================================================================
THE DEFECT
===========================================================================
2026-09-15. The CEO came back after a nine-hour break and asked "TLDR, plain
English". He was answered about a guard-deletion step, because that is what
the lead was holding. Five further attempts each answered a different thing.
It took SIXTEEN MESSAGES AND THIRTY-NINE MINUTES before he got the one line he
wanted, and he got it only after re-pasting his own messages from the night
before, repeatedly, in capitals:

    "HOW MANY MORE FUCKING MESSAGES FROM LAST NIGHT DO I NEED TO COPY AND
     PASTE HERE????"

Recorded as failure types 60 and 61 in richos-hq's
docs/verification/all-lifecycle-failure-records.md:

  TYPE 60  he returns after a gap and is told what the LEAD is currently
           holding instead of the thing HE left on. The aggravating detail,
           and the reason it is a type rather than an oversight: two turns
           before the answer, the lead PRINTED his 21:39 message on screen
           while computing the gap -- and still answered something else.
           RETRIEVAL WAS NOT THE FAILURE. SUBSTITUTING THE LEAD'S AGENDA FOR
           HIS WAS.

  TYPE 61  a deterministic lookup exists, takes seconds, and the question is
           answered from memory instead. And the trap inside the remedy: when
           a ledger finally WAS read, its `runtime_state` field was taken at
           face value and produced "19 jobs were dispatched and never closed
           out". All 19 had landed. The field records the LAST OBSERVED EVENT,
           never the outcome.

===========================================================================
WHAT THIS FILE IS, AND WHAT IT IS NOT
===========================================================================
IT IS NOT A GUARD. It refuses nothing, blocks nothing and has no exit code
anybody has to clear. The standing rule since 2026-09-14 is that a new guard
costs a deleted one; this is not a guard, it is an ANSWER delivered before the
question is asked.

It assembles three things, and every one of them is an artifact rather than a
claim:

  1. HIS LAST MESSAGE BEFORE THE GAP, verbatim, plus the sitting it ended --
     from the session transcript, which records every message with a
     timestamp.

  2. WHAT RAN ACROSS THE GAP AND WHERE IT ENDED UP -- the jobs from the
     transcript's own task notifications (title + token cost), and the
     outcome FROM GIT: the integration branch's REFLOG, cross-checked by
     ancestry. Never from a status field. See "THE OUTCOME COMES FROM GIT"
     below -- this is the half that type 61 is about.

  3. WHAT LANDED IN THAT WINDOW -- `git log --since ... --until ...` on every
     repository this session touched.

===========================================================================
THE OUTCOME COMES FROM GIT, AND THE OBVIOUS GIT CHECK IS THE WRONG ONE
===========================================================================
The obvious implementation asks `git merge-base --is-ancestor cc/<name> main`.
Measured against the very incident this file exists for, THAT CHECK ANSWERS
"branch does not exist" FOR EVERY LANDED JOB:

    $ git -C /Users/alex/ab/richos for-each-ref refs/heads/ \
        | grep -E 'silent1|vdesign1|derive1|r9fix1'
    (nothing)

The land sequence merges the branch and then deletes it, so ancestry is
unanswerable for exactly the work that succeeded -- and a check that cannot
distinguish "landed and cleaned up" from "never existed" would have reproduced
the 19-jobs error with git's authority behind it.

THE REFLOG IS THE ARTIFACT THAT SURVIVES THE DELETION:

    $ git -C /Users/alex/ab/richos reflog show main --date=iso
    de525f19 main@{2026-09-14 22:46:23 +0100}: merge cc/zach-opus-silent1: Fast-forward
    e6b54777 main@{2026-09-14 23:29:45 +0100}: merge cc/sage-opus-vdesign1: Merge made by the 'ort' strategy
    2545d7f1 main@{2026-09-15 01:09:44 +0100}: merge cc/tom-opus-derive1: Fast-forward

It records the instant the integration branch absorbed the work, it names the
branch, and it outlives the ref. So the verdict is:

  LANDED       the reflog names the branch AND the commit it moved to is still
               an ancestor of the integration tip. TWO git facts, because a
               reflog entry alone can name a commit a later reset discarded.
  NOT LANDED   the branch ref still exists and is not an ancestor of the tip.
               Carries the commits-ahead count.
  NO TRACE     neither. Said in those words. NOT "not landed" -- a reflog is
               local and expirable and a teammate may have worked in a
               repository this sweep never looked in. An honest "I cannot see
               it" is the one verdict that cannot mislead, and the 19-jobs
               error is what happens when a tool guesses instead.

===========================================================================
WHICH MESSAGES ARE HIS -- DERIVED, NOT ASSUMED
===========================================================================
A transcript's `user` rows are mostly NOT the user. Over all 766 transcripts on
this machine, every distinct provenance of a text-bearing `user` row:

    $ python3 ... over ~/.claude/projects/*/*.jsonl        (2026-09-15)
      2047  kind=human   src=typed                meta=False   <- HIM
      1348  kind=task-notification  src=system    meta=False
       683  kind=<null>  src=sdk                  meta=False
       443  kind=<null>  src=None                 meta=True    "Continue from where you left off."
       229  kind=<null>  src=None                 meta=False   <command-name>/exit</command-name>
        74  kind=peer    src=system               meta=True    another session's agent
        73  kind=human   src=sdk                  meta=False   a harness driving a session
        53  kind=human   src=queued               meta=False   <- HIM, typed while busy
        24  kind=human   src=suggestion_accepted  meta=False   <- HIM, one click
        14  kind=<null>  src=system               meta=True    fallback wakeup
         9  kind=task-notification  src=sdk       meta=False

So the predicate is: origin.kind == "human", promptSource is not "sdk", and
isMeta is falsy. 2124 rows of 5000.

`queued` IS HIM AND IS THE ONE A HAND-WRITTEN PREDICATE MISSES. He types while
the assistant is mid-turn constantly -- on the morning this file exists for he
sent the same message twice in forty seconds. A predicate keyed to `typed`
alone drops 53 of his messages on this machine, and it drops them silently.

The fallback, for a host version that stops emitting `origin`: any text-bearing
user row that is not meta, not a compact summary, and does not open with one of
the machine-written envelopes. It is WIDER and it is announced as degraded in
the report itself, because a report that silently changed its definition of
"him" would be the substitution failure again.

===========================================================================
THE CURRENT PROMPT IS EXCLUDED BY IDENTITY, NOT BY TIMING
===========================================================================
At UserPromptSubmit it is not knowable, and not stable across host versions,
whether the arriving message has already been appended to the transcript. So
this does not guess and does not measure: the caller passes the prompt TEXT,
and a trailing human row whose text matches it and which is less than
CURRENT_PROMPT_WINDOW_S old is dropped. If the row is not there yet, nothing
matches and the answer is identical. Both orders are covered in the suite.

===========================================================================
COST
===========================================================================
Measured on the 18 MB, 10031-line transcript of the session this was written
in, with three repositories in scope (femcboost, richos, richos-hq):

    full report, three runs   real 1.45  1.48  1.44
    no gap, three runs        real 0.17  0.19  0.18

The no-gap case is the common one and it stops before any git call at all.
Evidence and the commands: docs/verification/left-off-2026-09-15.md.

Usage:
    left-off.py --transcript P [--now ISO] [--prompt-file F] [--event E]
                [--gap-minutes N] [--entity-root R] [--session S]
                [--ledger L] [--format json|text]

Exit: 0 a report was produced, 1 nothing to report (no gap), 2 could not read.
"""

import argparse
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timedelta, timezone

GIT_TIMEOUT = 10
DEFAULT_GAP_MINUTES = 120
CURRENT_PROMPT_WINDOW_S = 180
SITTING_MESSAGES = 10
MESSAGE_CHARS = 220
LAST_MESSAGE_CHARS = 1200
MAX_JOBS = 25
MAX_COMMITS = 12
# MEASURED, not chosen: commit-ceo-inputs.sh records the host's
# additionalContext cap as 8000 characters and 200 lines, and a payload over it
# is dropped. The first full replay of this report came out at 6948 characters
# -- 87% of the cap, from a nine-hour gap that is not close to the worst case.
# So the report BUDGETS itself and says what it trimmed, rather than being
# silently discarded at the boundary. The margin is for the envelope's own JSON
# escaping, which inflates every quote and newline in his messages.
CONTEXT_BUDGET = 6400
TRANSCRIPT_TAIL_BYTES = 64 * 1024 * 1024

MACHINE_ENVELOPES = (
    "<task-notification>",
    "<command-name>",
    "<command-message>",
    "<local-command-stdout>",
    "<user-memory-input>",
    "Another Claude session sent a message:",
    "Stop hook feedback:",
    "Caveat:",
    "This session is being continued from a previous conversation",
)


# ===========================================================================
# TRANSCRIPT
# ===========================================================================
def read_transcript(path, limit_bytes=TRANSCRIPT_TAIL_BYTES):
    """[record] from a .jsonl transcript; [] if unreadable.

    Tail-bounded for the same reason read_ledger is: a hook that reads an
    unbounded file is a hook that will one day time out. Losing the OLDEST
    rows is the right direction -- every question here is about the recent end.
    """
    try:
        size = os.path.getsize(path)
        with open(path, "rb") as fh:
            if size > limit_bytes:
                fh.seek(size - limit_bytes)
                fh.readline()
            raw = fh.read().decode("utf-8", "replace")
    except Exception:
        return []
    out = []
    for line in raw.splitlines():
        line = line.strip()
        if not line or not line.startswith("{"):
            continue
        try:
            rec = json.loads(line)
        except Exception:
            continue
        if isinstance(rec, dict):
            out.append(rec)
    return out


OFFSET_NO_COLON = re.compile(r"([+-]\d{2})(\d{2})$")


def parse_ts(s):
    """An aware datetime, or None.

    git's `--date=iso` writes `+0100` with no colon, which
    datetime.fromisoformat refused before Python 3.11. Normalized here rather
    than at each call site: the reflog is one of exactly two clocks this file
    compares, and a parse that silently returns None turns every landing
    verdict into NO TRACE on an older interpreter -- a wrong answer wearing
    the honest one's words.
    """
    if not s:
        return None
    text = str(s).strip().replace("Z", "+00:00")
    for candidate in (text, OFFSET_NO_COLON.sub(r"\1:\2", text)):
        try:
            return datetime.fromisoformat(candidate)
        except Exception:
            continue
    return None


def text_of(rec):
    """(text, carries_tool_result) for one transcript record."""
    content = (rec.get("message") or {}).get("content")
    if isinstance(content, str):
        return content, False
    if isinstance(content, list):
        tool_result = any(isinstance(b, dict) and b.get("type") == "tool_result"
                          for b in content)
        text = "".join(b.get("text", "") for b in content
                       if isinstance(b, dict) and b.get("type") == "text")
        return text, tool_result
    return "", False


def is_human(rec):
    """THE PREDICATE. Derived from every provenance on this machine; see the
    module docstring's table.

    Returns (verdict, degraded). `degraded` says the row carried no `origin`
    at all and the wider fallback decided it -- the caller reports that rather
    than hiding it.
    """
    if rec.get("type") != "user":
        return False, False
    if rec.get("isMeta") or rec.get("isSidechain") or rec.get("isCompactSummary"):
        return False, False
    text, tool_result = text_of(rec)
    if tool_result or not text.strip():
        return False, False

    origin = rec.get("origin")
    if isinstance(origin, dict):
        if origin.get("kind") != "human":
            return False, False
        # `sdk` is a harness driving a session, not a person returning to one.
        if rec.get("promptSource") == "sdk":
            return False, False
        return True, False

    # No `origin` field: an older host, or one that stops emitting it. Widen,
    # and say so.
    if rec.get("promptSource") == "sdk":
        return False, True
    stripped = text.lstrip()
    for env in MACHINE_ENVELOPES:
        if stripped.startswith(env):
            return False, True
    return True, True


def human_messages(rows):
    """([(dt, text)] oldest first, degraded_count)."""
    out = []
    degraded = 0
    for rec in rows:
        verdict, deg = is_human(rec)
        if not verdict:
            continue
        when = parse_ts(rec.get("timestamp"))
        if when is None:
            continue
        if deg:
            degraded += 1
        text, _ = text_of(rec)
        out.append((when, text))
    out.sort(key=lambda r: r[0])
    return out, degraded


def drop_current_prompt(humans, prompt, now):
    """Remove a trailing row that IS the message being handled right now.

    By identity and recency, never by assuming the host's write order. See the
    module docstring.
    """
    if not humans or not prompt:
        return humans
    target = prompt.strip()
    if not target:
        return humans
    last_when, last_text = humans[-1]
    if last_text.strip() != target:
        return humans
    if (now - last_when).total_seconds() > CURRENT_PROMPT_WINDOW_S:
        return humans
    return humans[:-1]


# ===========================================================================
# THE GAP
# ===========================================================================
def find_gap(humans, now, gap_minutes):
    """{anchor, sitting, gap_seconds} or None when he has not been away."""
    if not humans:
        return None
    anchor_when, anchor_text = humans[-1]
    gap = (now - anchor_when).total_seconds()
    if gap < gap_minutes * 60:
        return None

    # The sitting: the run of messages ending at the anchor with no gap of its
    # own. Carried because his question is rarely in the last message alone --
    # on 2026-09-14 the figure he wanted an update on ("46% of tokens") was
    # eight messages before the one he stopped on.
    sitting = [(anchor_when, anchor_text)]
    for when, text in reversed(humans[:-1]):
        if (sitting[0][0] - when).total_seconds() >= gap_minutes * 60:
            break
        sitting.insert(0, (when, text))

    # DE-DUPLICATED, and this is not cosmetic. He re-sends a message verbatim
    # when an answer misses -- on the night this was built, 21:31 and 21:35
    # are byte-identical, and the morning after he sent the same message twice
    # in forty seconds. Left in, the repeats crowd out the message that
    # actually carried the number he wanted an update on. The repeat is
    # CARRIED as a note rather than dropped, because "he asked this twice" is
    # itself the signal that the first answer missed.
    collapsed, seen = [], {}
    for when, text in sitting:
        key = " ".join(text.split())
        if key in seen:
            index = seen[key]
            collapsed[index] = (collapsed[index][0], collapsed[index][1],
                                collapsed[index][2] + [when])
            continue
        seen[key] = len(collapsed)
        collapsed.append((when, text, []))
    sitting = collapsed
    truncated = max(0, len(sitting) - SITTING_MESSAGES)
    return {
        "anchor_when": anchor_when,
        "anchor_text": anchor_text,
        "sitting": sitting[-SITTING_MESSAGES:],
        "sitting_truncated": truncated,
        "gap_seconds": gap,
    }


def last_assistant_before(rows, when):
    """The last thing the assistant SAID to him before the gap -- its own
    words, not a summary of them."""
    best = None
    for rec in rows:
        if rec.get("type") != "assistant" or rec.get("isSidechain"):
            continue
        ts = parse_ts(rec.get("timestamp"))
        if ts is None or ts > when + timedelta(seconds=CURRENT_PROMPT_WINDOW_S):
            continue
        text, _ = text_of(rec)
        if not text.strip():
            continue
        if best is None or ts > best[0]:
            best = (ts, text)
    return best


# ===========================================================================
# WHAT RAN
# ===========================================================================
TOOL_USE_RE = re.compile(r"<tool-use-id>([^<]+)</tool-use-id>")
TOKENS_RE = re.compile(r"<subagent_tokens>(\d+)</subagent_tokens>")
STATUS_RE = re.compile(r"<status>([^<]+)</status>")
SUMMARY_RE = re.compile(r'<summary>Agent "(.*?)" finished</summary>')


def jobs_in_window(rows, start, end):
    """[job] -- what the session had running across the gap.

    The token cost is the host's own `<subagent_tokens>`, which is why this
    reads the transcript rather than a job ledger. The OUTCOME is not taken
    from here at all; `<status>` is the same class of thing as the
    `runtime_state` field that produced the 19-jobs error, so it is carried as
    "the host said" and the landing verdict comes from git.
    """
    dispatched = {}
    for rec in rows:
        if rec.get("type") != "assistant" or rec.get("isSidechain"):
            continue
        ts = parse_ts(rec.get("timestamp"))
        for block in (rec.get("message") or {}).get("content") or []:
            if not isinstance(block, dict):
                continue
            if block.get("type") != "tool_use" or block.get("name") != "Agent":
                continue
            inp = block.get("input") or {}
            dispatched[block.get("id")] = {
                "dispatched": ts,
                "name": inp.get("name") or "",
                "title": inp.get("description") or "",
                "type": inp.get("subagent_type") or "",
            }

    jobs = []
    seen_ids = set()
    for rec in rows:
        if rec.get("type") != "user":
            continue
        text, _ = text_of(rec)
        if "<task-notification>" not in text:
            continue
        ts = parse_ts(rec.get("timestamp"))
        if ts is None or not (start <= ts <= end):
            continue
        m = TOOL_USE_RE.search(text)
        tool_id = m.group(1) if m else ""
        seen_ids.add(tool_id)
        tok = TOKENS_RE.search(text)
        status = STATUS_RE.search(text)
        summary = SUMMARY_RE.search(text)
        info = dispatched.get(tool_id, {})
        jobs.append({
            "name": info.get("name") or "",
            "title": info.get("title") or (summary.group(1) if summary else ""),
            "dispatched": info.get("dispatched"),
            "ended": ts,
            "tokens": int(tok.group(1)) if tok else None,
            "host_status": status.group(1) if status else "",
            "no_completion_notice": False,
        })

    # Dispatched inside the window and never reported finished. This is the
    # class the transcript alone cannot answer for, and the class git can.
    for tool_id, info in dispatched.items():
        if tool_id in seen_ids:
            continue
        ts = info.get("dispatched")
        if ts is None or not (start <= ts <= end):
            continue
        jobs.append({
            "name": info.get("name") or "",
            "title": info.get("title") or "",
            "dispatched": ts,
            "ended": None,
            "tokens": None,
            "host_status": "",
            "no_completion_notice": True,
        })

    jobs.sort(key=lambda j: j.get("ended") or j.get("dispatched"))
    return jobs


# ===========================================================================
# GIT -- THE HALF THAT CANNOT LIE
# ===========================================================================
def git(root, args, timeout=GIT_TIMEOUT):
    try:
        res = subprocess.run(["git", "-C", root] + list(args),
                             capture_output=True, text=True, timeout=timeout)
    except Exception:
        return None
    if res.returncode != 0:
        return None
    return res.stdout


def git_ok(root, args, timeout=GIT_TIMEOUT):
    """True/False/None -- None means git could not be asked at all."""
    try:
        res = subprocess.run(["git", "-C", root] + list(args),
                             capture_output=True, text=True, timeout=timeout)
    except Exception:
        return None
    return res.returncode == 0


def load_sibling(name):
    """Load a sibling library by path. A duplicated LOADER, never a duplicated
    ANSWER: repository discovery and the integration-branch question each have
    exactly one implementation in this engine and this is not another."""
    lib = os.path.join(os.path.dirname(os.path.abspath(__file__)), name)
    try:
        import importlib.util
        spec = importlib.util.spec_from_file_location(
            "left_off_" + name.replace("-", "_").replace(".py", ""), lib)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        return mod
    except Exception:
        return None


def session_workspaces(session_id):
    """{teammate: [{repo, branch, disposition}]} from the workspace registry.

    THE REGISTRY IS USED FOR THE MAP, NEVER FOR THE VERDICT. It is the one
    place that records which REPOSITORY and which BRANCH a teammate's work
    lives in -- a fact git cannot supply, because git does not know the name
    `sage-opus-vdesign1`. Whether that branch LANDED is then asked of git and
    of nothing else. Any `disposition` here is carried as a quoted CLAIM
    beside git's answer and never instead of it: a status field records the
    last observed event, which is the whole of failure type 61.
    """
    ws = load_sibling("workspaces.py")
    if ws is None:
        return {}
    try:
        records = ws.all_agents(include_done=True)
    except Exception:
        return {}
    out = {}
    for rec in records:
        if session_id and rec.get("session_id") != session_id:
            continue
        name = rec.get("name") or ""
        if not name:
            continue
        disp = rec.get("disposition") or {}
        for work in rec.get("workspaces") or []:
            repo, branch = work.get("repo") or "", work.get("branch") or ""
            if not repo:
                continue
            out.setdefault(name, []).append({
                "repo": repo,
                "branch": branch,
                "claimed": "%s: %s" % (disp.get("kind", ""),
                                       clip(disp.get("reason", ""), 120))
                           if disp.get("kind") else "",
            })
    return out


def repositories(entity_root, session_id, ledger_path, extra="", workspaces=None):
    """(repos, scope) -- every repository this session actually worked in.

    THREE SOURCES, UNIONED, because each one alone has a measured hole:
      * the seat, from unlanded-branches.py -- this session's seat is
        femcboost and every teammate works in richos, so the seat alone is the
        repository where nothing happens.
      * the worktree ledger -- measured on the session this was written in, it
        holds 1857 `finished` rows for the session and ZERO
        `registered`/`prepared` rows, because spawn.sh registers into the
        workspace registry instead. A ledger-only sweep found only the seat.
      * the workspace registry -- which is what spawn.sh writes, and is the
        only source that named richos at all.
    """
    ub = load_sibling("unlanded-branches.py")
    found, scope = [], "session" if session_id else "entity-only"
    if ub is not None:
        try:
            ledger = ub.read_ledger(ledger_path or os.path.expanduser(
                "~/.claude/state/worktree-ledger.jsonl"))
            found, scope = ub.session_repos(entity_root or "", session_id or "",
                                            ledger, extra)
            found = list(found)
        except Exception:
            found = []
    for works in (workspaces or {}).values():
        for work in works:
            repo = work.get("repo") or ""
            if not repo or not os.path.isdir(repo):
                continue
            if ub is not None:
                repo = ub.main_checkout(repo) or repo
            found.append(repo)
    if not found:
        scope = "unavailable"
    return sorted(set(found)), scope


def integration_branch(repo):
    ub = load_sibling("unlanded-branches.py")
    if ub is None:
        return None
    try:
        return ub.trunk_of(repo)
    except Exception:
        return None


REFLOG_RE = re.compile(r"^(\S+)\s+\S+@\{([^}]+)\}:\s*(.*)$")


def reflog_entries(repo, branch, limit=400):
    """[(sha, when, message)] from the integration branch's reflog."""
    out = git(repo, ["reflog", "show", branch, "--date=iso",
                     "-n", str(limit)])
    if out is None:
        return []
    rows = []
    for line in out.splitlines():
        m = REFLOG_RE.match(line.strip())
        if not m:
            continue
        rows.append((m.group(1), parse_ts(m.group(2).replace(" ", "T", 1)
                                          .replace(" ", "")), m.group(3)))
    return rows


def names_ref(name, text):
    """Match a complete branch token, allowing surrounding prose punctuation.

    Git allows punctuation such as +, @ and $ in refs. Enumerating just word
    characters lets a longer branch borrow its prefix's landing (P5-58).
    Quotes/backticks and sentence-ending periods delimit names in these records.
    """
    if not name:
        return False
    # Git's forbidden ref characters, plus the quoting used by these records.
    tokens = re.findall(r"[^\s\x00-\x20\x7f~^:?*\[\\'\"`<>()]+", text)
    # Job names can omit the branch namespace (mark-opus-f1 vs cc/mark-opus-f1).
    return any(token.rstrip(".") == name or token.rstrip(".").endswith("/" + name)
               for token in tokens)


def landing_verdict(repos_info, name, as_of=None, workspaces=None):
    """Where a teammate's work ended up, from git and nothing else.

    `as_of` exists so a REPLAY is faithful: a reflog entry written after the
    instant being replayed is not evidence about that instant. Without it a
    replay quietly answers with today's knowledge and proves nothing about
    what the report would have said.

    Returns (verdict, detail). Verdicts: LANDED, NOT LANDED, NO TRACE.
    """
    if not name:
        return "NO TRACE", "the dispatch recorded no teammate name"

    # The registry's recorded (repo, branch) is checked FIRST and by name, so
    # the answer names the repository the work was actually in rather than
    # whichever repository happened to be swept first.
    for work in (workspaces or {}).get(name, []):
        repo, branch = work.get("repo"), work.get("branch")
        info = repos_info.get(repo)
        if not info or not branch or not info.get("branch"):
            continue
        claim = (" [registry claims %s]" % work["claimed"]) if work.get("claimed") else ""
        for sha, when, message in info["reflog"]:
            if not names_ref(branch, message):
                continue
            if as_of is not None and when is not None and when > as_of:
                continue
            still = git_ok(repo, ["merge-base", "--is-ancestor", sha, info["branch"]])
            if still is True:
                return "LANDED", "%s absorbed %s at %s as %s, still an ancestor of %s%s" % (
                    os.path.basename(repo), branch,
                    when.isoformat() if when else "?", sha, info["branch"], claim)
            if still is False:
                return "NOT LANDED", (
                    "%s: the reflog shows %s merged at %s, but %s is NO LONGER an "
                    "ancestor of %s -- it was undone%s" % (
                        os.path.basename(repo), branch,
                        when.isoformat() if when else "?", sha, info["branch"], claim))
        if branch in info["branches"]:
            anc = git_ok(repo, ["merge-base", "--is-ancestor", branch, info["branch"]])
            if anc is True:
                return "LANDED", "%s: %s is an ancestor of %s%s" % (
                    os.path.basename(repo), branch, info["branch"], claim)
            ahead = git(repo, ["rev-list", "--count", branch, "--not", info["branch"]])
            return "NOT LANDED", "%s: %s is %s commit(s) ahead of %s%s" % (
                os.path.basename(repo), branch, (ahead or "?").strip() or "?",
                info["branch"], claim)
        return "NO TRACE", (
            "%s: the registry records %s but git has no reflog entry and no such "
            "branch -- this is 'not seen', NOT 'not landed'%s" % (
                os.path.basename(repo), branch, claim))

    for repo, info in repos_info.items():
        for sha, when, message in info["reflog"]:
            if not names_ref(name, message):
                continue
            if as_of is not None and when is not None and when > as_of:
                continue
            tip = info["branch"]
            still = git_ok(repo, ["merge-base", "--is-ancestor", sha, tip])
            if still is True:
                return "LANDED", "%s absorbed %s at %s (%s), still an ancestor of %s" % (
                    os.path.basename(repo), sha, when.isoformat() if when else "?",
                    message.split(":")[0], tip)
            if still is False:
                return "NOT LANDED", (
                    "%s: the reflog shows %s merged at %s but %s is NO LONGER an "
                    "ancestor of %s -- it was undone" % (
                        os.path.basename(repo), name,
                        when.isoformat() if when else "?", sha, tip))
    # No reflog trace. Is the branch still sitting there?
    for repo, info in repos_info.items():
        for branch in info["branches"]:
            if not names_ref(name, branch):
                continue
            tip = info["branch"]
            anc = git_ok(repo, ["merge-base", "--is-ancestor", branch, tip])
            if anc is True:
                return "LANDED", "%s: %s is an ancestor of %s" % (
                    os.path.basename(repo), branch, tip)
            ahead = git(repo, ["rev-list", "--count", branch, "--not", tip])
            return "NOT LANDED", "%s: %s is %s commit(s) ahead of %s" % (
                os.path.basename(repo), branch,
                (ahead or "?").strip() or "?", tip)
    return "NO TRACE", (
        "no reflog entry and no branch names %s in the repositories swept -- "
        "this is 'not seen', NOT 'not landed'" % name)


def repo_state(repos, start, end):
    """{repo: {branch, reflog, branches, commits}} -- one pass per repository."""
    info = {}
    for repo in repos:
        branch = integration_branch(repo)
        if not branch:
            info[repo] = {"branch": None, "reflog": [], "branches": [],
                          "commits": [], "why_not": "no recorded integration branch"}
            continue
        refs = git(repo, ["for-each-ref", "--format=%(refname:short)",
                          "refs/heads/"]) or ""
        log = git(repo, ["log", branch,
                         "--since=" + start.isoformat(),
                         "--until=" + end.isoformat(),
                         "--format=%h\t%cI\t%s"])
        if log is None:
            # git() is None when git FAILED, and "" when it ran and found no
            # commit. Folding the two used to print "nothing landed" over a
            # repository nobody managed to read (hunt P5-08). The report already
            # has a channel for a repository it cannot answer for.
            info[repo] = {"branch": branch, "reflog": [], "branches": [],
                          "commits": [],
                          "why_not": "`git log %s` failed, so what landed there in the "
                                     "window is UNKNOWN" % branch}
            continue
        commits = []
        for line in log.splitlines():
            parts = line.split("\t", 2)
            if len(parts) == 3:
                commits.append(tuple(parts))
        info[repo] = {
            "branch": branch,
            "reflog": reflog_entries(repo, branch),
            "branches": [b for b in refs.split() if b],
            "commits": commits,
            "why_not": "",
        }
    return info


def escalations_in_window(start, end):
    """[(id, state, title)] raised inside the gap -- from the shipped ledger."""
    esc = load_sibling("escalations.py")
    if esc is None:
        return []
    try:
        # (rows, malformed) -- and rows is None when the ledger exists and
        # cannot be read, which is NOT the same as empty.
        rows, _malformed = esc.read_rows()
    except Exception:
        return []
    if rows is None:
        return [("", "", "THE ESCALATION LEDGER COULD NOT BE READ -- this list is "
                         "not empty, it is unknown")]
    out = []
    for row in rows:
        if row.get("event") != "Escalation":
            continue
        when = parse_ts(row.get("raised") or row.get("ts"))
        if when is None or not (start <= when <= end):
            continue
        out.append((row.get("id", ""), row.get("state", ""),
                    (row.get("title", "") or "")[:110]))
    return out


# ===========================================================================
# THE REPORT
# ===========================================================================
def human_gap(seconds):
    hours, rem = divmod(int(seconds), 3600)
    return "%dh %02dm" % (hours, rem // 60)


def clip(text, limit):
    text = " ".join((text or "").split())
    if len(text) <= limit:
        return text
    return text[:limit - 1] + "…"


def build(args, now=None):
    live = now is None
    now = now or datetime.now(timezone.utc)
    rows = read_transcript(args.transcript)
    if not live:
        # AS-OF. An explicit --now is a replay, and a replay that can see rows
        # written after the instant it claims to reproduce proves nothing about
        # that instant. Live runs skip this: there is nothing after now.
        rows = [r for r in rows
                if (parse_ts(r.get("timestamp")) or now) <= now]
    fallback_used = ""
    if not rows and args.transcript:
        return {"status": "unreadable",
                "reason": "the transcript at %s could not be read" % args.transcript}

    humans, degraded = human_messages(rows)

    # A NEW session has no history of its own. The thing he left off on is in
    # the PREVIOUS transcript of the same project, which is the ordinary
    # morning case and the one this report exists for.
    #
    # SESSION START ONLY, and the restriction is load-bearing. At
    # UserPromptSubmit there IS a current session and a current message; an
    # empty result there means the PREDICATE found nothing, and reaching into
    # an adjacent session's transcript would answer with somebody else's
    # conversation while looking exactly like an answer about this one.
    if not humans and args.transcript and args.event == "SessionStart":
        project_dir = os.path.dirname(os.path.abspath(args.transcript))
        try:
            siblings = sorted(
                (os.path.join(project_dir, f) for f in os.listdir(project_dir)
                 if f.endswith(".jsonl")),
                key=lambda p: os.path.getmtime(p), reverse=True)
        except Exception:
            siblings = []
        for sib in siblings[:5]:
            if os.path.abspath(sib) == os.path.abspath(args.transcript):
                continue
            sib_rows = read_transcript(sib)
            sib_humans, sib_degraded = human_messages(sib_rows)
            if sib_humans:
                rows, humans, degraded = sib_rows, sib_humans, sib_degraded
                fallback_used = sib
                break

    prompt = ""
    if args.prompt_file and os.path.isfile(args.prompt_file):
        try:
            with open(args.prompt_file, encoding="utf-8", errors="replace") as fh:
                prompt = fh.read()
        except Exception:
            prompt = ""
    humans = drop_current_prompt(humans, prompt, now)

    if not humans:
        return {"status": "no-messages"}

    gap = find_gap(humans, now, args.gap_minutes)
    if gap is None:
        return {"status": "no-gap",
                "last_message_age_s": (now - humans[-1][0]).total_seconds()}

    start, end = gap["anchor_when"], now
    jobs = jobs_in_window(rows, start, end)
    works = session_workspaces(args.session)
    repos, scope = repositories(args.entity_root, args.session, args.ledger,
                                args.extra_repos, works)
    info = repo_state(repos, start, end)
    for job in jobs:
        verdict, detail = landing_verdict(info, job["name"],
                                          as_of=None if live else now,
                                          workspaces=works)
        job["landed"] = verdict
        job["landed_detail"] = detail

    return {
        "status": "gap",
        "now": now.isoformat(),
        "gap_seconds": gap["gap_seconds"],
        "gap_human": human_gap(gap["gap_seconds"]),
        "anchor_when": gap["anchor_when"].isoformat(),
        "anchor_text": gap["anchor_text"],
        "sitting": [(w.isoformat(), t, [r.isoformat() for r in repeats])
                    for w, t, repeats in gap["sitting"]],
        "sitting_truncated": gap["sitting_truncated"],
        "last_reply": (lambda r: {"when": r[0].isoformat(), "text": r[1]}
                       if r else None)(last_assistant_before(rows, gap["anchor_when"])),
        "jobs": [dict(j, dispatched=(j["dispatched"].isoformat() if j["dispatched"] else None),
                      ended=(j["ended"].isoformat() if j["ended"] else None))
                 for j in jobs],
        "repos": {r: {"branch": i["branch"], "commits": i["commits"],
                      "why_not": i["why_not"]} for r, i in info.items()},
        "repo_scope": scope,
        "escalations": escalations_in_window(start, end),
        "degraded_rows": degraded,
        "transcript": fallback_used or args.transcript,
        "transcript_is_previous_session": bool(fallback_used),
        "event": args.event,
    }


def render(result, budget=CONTEXT_BUDGET):
    """The report, trimmed to fit the channel rather than dropped by it.

    The order of the trims is the order of what matters least: the oldest of
    his sitting messages, then the tail of the landed-commit list, then the
    length of the last reply. HIS LAST MESSAGE AND THE JOB TABLE ARE NEVER
    TRIMMED -- a report that dropped the thing it exists to deliver in order to
    fit would be this whole defect wearing a budget.
    """
    if result.get("status") != "gap":
        return ""
    sitting_n, commits_n, reply_n = SITTING_MESSAGES, MAX_COMMITS, 600
    text = _render(result, sitting_n, commits_n, reply_n)
    while budget and len(text) > budget:
        # HIS WORDS GO LAST. My own last reply first, then the commit tail,
        # and only then the oldest of his messages -- the first budgeted
        # version of this trimmed his sitting first and threw away the 21:03
        # message carrying "46% of tokens", which is the single number the
        # whole report exists to put in front of the answer.
        if reply_n > 250:
            reply_n -= 150
        elif commits_n > 5:
            commits_n -= 2
        elif sitting_n > 1:
            # Down to 1 leaves the anchor and drops the sitting entirely. The
            # count of what is not shown stays on screen, so a dropped thread
            # is visible rather than silently absent.
            sitting_n -= 1
        else:
            marker = "\n  [TRIMMED to fit the channel; re-run the command in the footer]"
            return text[:max(0, budget - len(marker))].rstrip() + marker
        text = _render(result, sitting_n, commits_n, reply_n)
    return text


def _render(result, sitting_n, commits_n, reply_n):
    out = []
    add = out.append

    add("=== WHERE HE LEFT OFF — he is back after %s away ===" % result["gap_human"])
    add("")
    add("ANSWER THE THING HE LEFT ON, IN HIS TERMS, BEFORE ANYTHING ELSE. Not what")
    add("you are currently holding, not the backlog, not what is running. That")
    add("substitution is lifecycle failure type 60 and it cost him 16 messages and")
    add("39 minutes on 2026-09-15. Retrieval was never the failure; answering")
    add("something else after retrieving it was.")
    add("")
    add("HIS LAST MESSAGE BEFORE THE GAP, VERBATIM (%s):" % result["anchor_when"])
    for line in clip(result["anchor_text"], LAST_MESSAGE_CHARS).splitlines() or [""]:
        add("    %s" % line)
    add("")

    sitting = result["sitting"][:-1][-(sitting_n - 1):] if sitting_n > 1 else []
    hidden = result["sitting_truncated"] + max(0, len(result["sitting"]) - 1 - len(sitting))
    if sitting:
        add("THE SITTING IT ENDED — his own words, oldest first%s. His question is"
            % (", %d earlier ones not shown" % hidden if hidden else ""))
        add("often not in the last message alone:")
        for when, text, repeats in sitting:
            again = (" [he sent this again at %s — the first answer missed]"
                     % ", ".join(r[11:16] for r in repeats)) if repeats else ""
            add("  %s  %s%s" % (when[11:16], clip(text, MESSAGE_CHARS), again))
        add("")

    if result.get("last_reply"):
        add("WHAT YOU LAST TOLD HIM ABOUT IT (%s):" % result["last_reply"]["when"])
        add("    %s" % clip(result["last_reply"]["text"], reply_n))
        add("")

    jobs = result["jobs"]
    if jobs:
        add("WHAT RAN ACROSS THE GAP — %d job(s). Token cost is the host's own"
            % len(jobs))
        add("<subagent_tokens>; the OUTCOME column is GIT (the integration branch's")
        add("reflog, cross-checked by ancestry), never a status field. A status field")
        add("records the last observed event, not the outcome — failure type 61.")
        add("")
        add("    %9s  %-22s  %-10s  %s" % ("tokens", "teammate", "outcome", "job"))
        # Hunt P5-59: the total covers every job, not just the displayed rows.
        total = sum(j.get("tokens") or 0 for j in jobs)
        for job in jobs[:MAX_JOBS]:
            tokens = job.get("tokens")
            add("    %9s  %-22s  %-10s  %s" % (
                "{:,}".format(tokens) if tokens else
                ("no notice" if job["no_completion_notice"] else "-"),
                clip(job["name"], 22) or "?",
                job.get("landed", "?"),
                clip(job["title"], 44)))
        if len(jobs) > MAX_JOBS:
            add("    (%d more)" % (len(jobs) - MAX_JOBS))
        add("")
        add("    %s tokens over %d job(s) that reported a cost." % (
            "{:,}".format(total), sum(1 for j in jobs if j.get("tokens"))))
        add("    THE SPLIT IS YOURS AND IT IS USUALLY WHAT HE IS ASKING FOR: classify")
        add("    these by what they were FOR and give him the proportion. Do not report")
        add("    a percentage over a window that reaches past his return — this table")
        add("    ends at his first message back, which is what makes it an overnight")
        add("    figure rather than a figure including this morning.")
        add("")
        for job in jobs[:MAX_JOBS]:
            if job.get("landed") in ("NOT LANDED", "NO TRACE"):
                add("    %-22s %s: %s" % (clip(job["name"], 22), job["landed"],
                                          job.get("landed_detail", "")))
            elif job.get("no_completion_notice") and job.get("landed") == "LANDED":
                # The transcript never said this finished and git says it did.
                # This is the case the transcript alone gets wrong, and it is
                # the reason the outcome column is git rather than the host.
                add("    %-22s NO COMPLETION NOTICE IN THE TRANSCRIPT, AND IT LANDED ANYWAY:"
                    % clip(job["name"], 22))
                add("    %-22s   %s" % ("", job.get("landed_detail", "")))
        add("")
    else:
        add("WHAT RAN ACROSS THE GAP: no job started or finished in the window.")
        add("")

    landed_any = False
    for repo, info in sorted(result["repos"].items()):
        if info.get("why_not"):
            add("WHAT LANDED in %s: CANNOT ANSWER — %s." % (repo, info["why_not"]))
            add("")
            continue
        commits = info["commits"]
        if not commits:
            continue
        landed_any = True
        add("WHAT LANDED ON %s IN %s — %d commit(s):" % (
            info["branch"], repo, len(commits)))
        for sha, when, subject in commits[:commits_n]:
            add("  %s  %s  %s" % (sha, when[11:16], clip(subject, 88)))
        if len(commits) > commits_n:
            add("  (%d more; git -C %s log %s --since=%s --until=%s)" % (
                len(commits) - commits_n, repo, info["branch"],
                result["anchor_when"], result["now"]))
        add("")
    if not landed_any and result["repos"]:
        unanswered = [r for r, i in result["repos"].items() if i.get("why_not")]
        if unanswered:
            # "Nothing landed" is a claim about every repository; it is only as
            # wide as the ones that could be read (hunt P5-08).
            add("WHAT LANDED: nothing on the %d repositor%s that could be read; %d could "
                "NOT be answered for (named above), so this is not 'nothing landed'."
                % (len(result["repos"]) - len(unanswered),
                   "y" if len(result["repos"]) - len(unanswered) == 1 else "ies",
                   len(unanswered)))
        else:
            add("WHAT LANDED: nothing on any integration branch in the window.")
        add("")

    esc = result.get("escalations") or []
    if esc:
        add("RAISED WHILE HE WAS AWAY — %d escalation(s):" % len(esc))
        for eid, state, title in esc[:8]:
            add("  %s  [%s]  %s" % (eid, state, title))
        add("")

    add("HOW TO RE-DERIVE ANY OF THIS, so no number here has to be trusted:")
    add("  his messages   %s  (origin.kind=human, promptSource!=sdk, not isMeta)"
        % result["transcript"])
    add("  job cost       the same transcript's <subagent_tokens>")
    for repo, info in sorted(result["repos"].items()):
        if info.get("branch"):
            add("  landings       git -C %s reflog show %s --date=iso" % (repo, info["branch"]))
    add("  repo scope     %s" % result["repo_scope"])
    if result.get("transcript_is_previous_session"):
        add("  NOTE: this session has no messages of its own yet; the anchor comes")
        add("        from the previous transcript in the same project directory.")
    if result.get("degraded_rows"):
        add("  DEGRADED: %d of his messages were identified WITHOUT an `origin` field"
            % result["degraded_rows"])
        add("            (older host format), by a wider fallback predicate. Treat the")
        add("            message set as approximate and check the transcript directly.")
    return "\n".join(out)


def one_line(result):
    if result.get("status") != "gap":
        return ""
    return "HE IS BACK after %s. Answer THIS first: \"%s\"" % (
        result["gap_human"], clip(result["anchor_text"], 150))


def main(argv=None):
    ap = argparse.ArgumentParser(add_help=True)
    ap.add_argument("--transcript", default="")
    ap.add_argument("--now", default="")
    ap.add_argument("--prompt-file", default="")
    ap.add_argument("--event", default="UserPromptSubmit")
    ap.add_argument("--gap-minutes", type=float, default=DEFAULT_GAP_MINUTES)
    ap.add_argument("--entity-root", default="")
    ap.add_argument("--session", default="")
    ap.add_argument("--ledger", default="")
    ap.add_argument("--extra-repos", default="")
    # The channel budget is a MEASURED constant, not a preference, so it is not
    # configuration. It is an argument only so a suite can force the trimming
    # path and watch his last message survive it -- a property that is
    # otherwise only exercised by a report large enough to be hard to build.
    ap.add_argument("--budget", type=int, default=CONTEXT_BUDGET)
    # `hook` is ONE analysis producing everything the wiring needs: line 1 the
    # gap key, line 2 the operator's one-liner, line 3 onward the report. The
    # first shape of this hook ran the analyzer twice to get the report and the
    # line separately, which doubled a 0.89 s cost to get a string it already
    # had.
    ap.add_argument("--format", choices=("json", "text", "line", "hook"),
                    default="text")
    args = ap.parse_args(argv)

    now = parse_ts(args.now) if args.now else None
    if args.now and now is None:
        sys.stderr.write("unreadable --now: %s\n" % args.now)
        return 2
    result = build(args, now=now)

    if args.format == "json":
        sys.stdout.write(json.dumps(result, indent=1, default=str) + "\n")
    elif args.format == "line":
        line = one_line(result)
        if line:
            sys.stdout.write(line + "\n")
    elif args.format == "hook":
        if result.get("status") == "gap":
            sys.stdout.write("%s\n%s\n%s\n" % (
                result["anchor_when"],
                " ".join(one_line(result).split()),
                render(result, args.budget)))
    else:
        text = render(result, args.budget)
        if text:
            sys.stdout.write(text + "\n")

    if result.get("status") == "gap":
        return 0
    if result.get("status") == "unreadable":
        return 2
    return 1


if __name__ == "__main__":
    sys.exit(main())
