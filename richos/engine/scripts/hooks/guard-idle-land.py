#!/usr/bin/env python3
"""
guard-idle-land.py — the analysis half of the Stop-time idle-land gate.

Called by guard-idle-land.sh, which has already resolved the two roots and
decided that this repository adopted the engine. This file does the reading,
the resolving and the verdict; it never decides whether to run.

WHAT THIS IS FOR
  The orchestrator's working record opens by stating its own rule: a land ends
  by STARTING the top unblocked item, then reports -- not the other way round;
  the only permitted stop is an item whose next action needs a decision only
  the CEO can make. The rule was written down. Then four branches landed across
  two repositories, a long report was written, and the turn ended with an empty
  dispatch queue and seven unblocked rows sitting in the file. The operator had
  to type "why has everything stopped again" for the seventh time in two days.

  Every previous answer to that question was a document. This is the same
  defect the whole engine keeps finding in itself -- A RULE ENFORCED BY
  ATTENTION LASTS EXACTLY AS LONG AS THE ATTENTION -- and the answer to it here
  is the answer everywhere else: put it at a chokepoint that fires whether or
  not anybody remembers it. Stop is the chokepoint. The turn is the thing.

WHAT WENT WRONG WITH THE FIRST VERSION, AND WHY THIS ONE IS DIFFERENT
  The paragraph above was written on 2026-08-30 and the gate shipped that night,
  BLOCKING, measured over 1,082 real turns. On 2026-09-01 the operator reported
  the identical failure twice in one day and said it had been happening for
  months. A gate that ships and then watches the thing it forbids happen twice
  is not a gate; it is a receipt. So the first question was not "what else can
  be built" but "why did the built thing not fire", and the answer is in the
  gate's OWN observation record -- 107 landing turns on this machine:

      dispatched (correctly silent)     60
      background-running (STOOD DOWN)   44      <- 41% of every landing turn
      backlog-empty (correctly silent)   2
      block                              1

  Two defects, and both of them made it quiet:

    * TERM 4 WAS A BLANKET DISARM. It stood the whole gate down whenever
      `background_tasks` held anything running at all -- and that field is the
      host's entire task registry: teammates, subagents, shells, monitors,
      workflows, MCP tasks, scans. This orchestrator keeps ten to fifteen
      teammates alive at all times, so the gate was off almost whenever it
      mattered. The long argument, with the binary's own type table, is beside
      still_running() below.

    * TERM 1 READ ONLY HALF OF "COMPLETED". Work completes here in two ways: a
      land, and a teammate handing its work back. The gate saw only the first,
      so the turn that answers a finished teammate by LISTING what is left --
      the second of the two failures reported that day -- was invisible to it.

  Both are fixed below. The gate now triggers on either kind of completion, and
  a thing that was already running suppresses nothing.

THE PREDICATE, AND WHY EVERY TERM IS READ FROM GROUND TRUTH
  The condition, stated as the thing that must never happen: A TURN ENDS IN
  WHICH WORK WAS COMPLETED, NO FURTHER WORK WAS STARTED, AND NOTHING IS OWED TO
  THE CEO. Four terms, all four required, none read from prose the orchestrator
  wrote:

    1. THIS TURN COMPLETED SOMETHING -- either half is enough:

       (a) A LAND. Not "the message says landed" -- a `git merge` or `git push`
           in THIS TURN'S OWN TOOL TRAFFIC, whose EFFECT is then confirmed
           against the repository by identity:
             merge <ref>  ->  `merge-base --is-ancestor <ref> HEAD`
             push         ->  HEAD == the branch's remote-tracking ref
           A merge that conflicted and was aborted fails both. This is the
           freshness contract's own rule -- identity or refuse -- pointed at an
           action instead of at an artifact.

       (b) A TEAMMATE FINISHING. The host's own `<task-notification>` record,
           with `<status>completed</status>` and an `Agent "..." finished`
           summary, inside this turn's window. Structured, host-written, and
           turn-scoped by its own promptId. See agent_finishes().

    2. NO WORK WAS STARTED. No `Agent` tool call and no BACKGROUNDED tool call
       in this same turn, SCOPED TO promptId. The scoping is not an
       optimization: its sibling guard collected tool names session-wide, so
       "did this turn call Agent?" was permanently yes after the first spawn and
       its reporting layer was silently dead for weeks while its suite stayed
       green. That bug is replayed as a test case here (cases p2/p3 in the
       suite) so it cannot come back.

    3. NOTHING IS OWED TO THE CEO. No `AskUserQuestion` in this turn -- read
       from the TOOL CALL, not from prose. Ending a turn on a question he has to
       answer is the one move nobody else can make for him.

       WHAT USED TO BE HERE AND IS GONE (2026-09-20, CEO ruling §68): a second
       half of this term read the CEO's own typed messages for a hold -- "hold",
       "stand down", "don't dispatch" -- or for the end of his day, and stood
       the gate down on a match. It was the engine's other language predicate
       over his words, and it decided something: whether a blocking gate fired.
       His ruling is that a hook may check what the orchestrator DID and never
       decide what HE MEANT, however careful the regex and however one-
       directional the error. So the branch is deleted, not narrowed. A turn he
       told to stop is now ended the same way every other legitimate stop is
       ended -- the orchestrator writes `stop-declared: ceo-owns-it` with its
       reason, which is the orchestrator's own act, in its own words, on the
       record where the CEO can read it.

    4. THERE IS SOMETHING TO START. At least one row of the record's `## Next`
       table that is neither struck through nor blocked -- DERIVED FROM THE
       FILE, never a typed count, never a number in a report. If the file is
       missing, unreadable, has no parsable table, or is ambiguous (two
       candidate records on this machine), THE GATE GOES INERT AND SAYS SO. It
       never blocks on a guess about what is left to do.

       THIS TERM IS ALSO WHAT ANSWERS "a teammate is still running". The
       legitimate version of that stop is "still running AND THE NEXT STEP
       DEPENDS ON ITS RESULT", and dependency is exactly what the `Blocked by`
       column records. A row the record calls unblocked does not depend on the
       running teammate; a row that does is marked blocked and never reaches the
       verdict. The old term 4 was answering a much easier question in its place.

  Everything unrecognized is treated as BLOCKED, not as free. Every ambiguity
  resolves toward silence. A gate that cries wolf is removed within a day, and
  then the operator is worse off than before it existed.

THE ESCAPE IS A DECLARATION, NOT A TOKEN
  This file used to carry a section headed "THE ESCAPE, AND WHY THERE IS NO
  TOKEN FOR IT", arguing that an in-the-moment override would be reached for at
  exactly the moment the gate was working. That argument is RIGHT ABOUT TOKENS,
  and it is why the escape here is not one.

  A flag is free, so it gets typed reflexively. A DECLARATION is not: it names
  WHICH of the three legitimate stops applies and WHY, in a sentence, in the
  reply the CEO reads, and it is shown to him through systemMessage every time.
  To write "nothing unblocked remains because X" you have to have checked -- and
  that is the behavior change. Half the value of this gate is making the stop
  DELIBERATE rather than absent-minded; a stop that has been thought about and
  justified in front of the person it affects is the outcome, not a hole in it.
  Full argument, the three cases and the bare-marker rule at stop_declaration().

  Moving the row into the CEO's record remains the other, better escape, and it
  is still what the refusal recommends first, because it is committed and
  diffable. The gate also stays INERT unless the repository declares
  `.ceo-todos`: the deferral target must exist before a refusal can honestly
  point at it, and a gate that refuses and offers nowhere to go is a gate people
  unwire.

WHAT IT CANNOT SEE
  * work started any way other than an `Agent` call or a backgrounded tool call
    -- a task written into a store, a message to a running teammate. Those are
    not dispatches of the top row and the record's rule is about dispatching.
  * a land that reaches a repository some way other than `git merge`/`git push`
    (a cherry-pick, an `am`, a rebase-and-fast-forward). Stated gap, not an
    oversight: none of them is how work lands here.
  * whether the row that WAS dispatched is the TOP one. This gate checks that
    something started, never that the right thing started. Ordering is
    judgment and this is not a judge.
  * a merge whose branch ref was deleted immediately afterwards: the identity
    confirmation cannot resolve it, so the turn passes. Quiet direction.
  * whether a declaration is TRUE. It cannot, and it does not pretend to: it
    checks the shape and shows the sentence to the one person who can tell.

Exit codes:
  0  completed nothing, started something, owed the CEO an answer, declared the
     stop, nothing to start, not evaluable, or anything went wrong
  2  BLOCKED -- work completed, nothing started, nothing owed, and the record
     has an unblocked row
"""

import json
import os
import re
import shlex
import subprocess
import sys

# --------------------------------------------------------------------------
# this turn's tool traffic
# --------------------------------------------------------------------------

# A prompt the HOST wrote, not the operator: a task notification carrying an
# agent's whole handoff, a system reminder, a queued-command echo.
MACHINE_PROMPT_RE = re.compile(
    r"<(task-notification|system-reminder|command-message|command-name|"
    r"local-command-stdout)>")


UPTODATE_RE = re.compile(r"Already up[ -]to[ -]date", re.I)


def turn_start_offset(path, prompt_id, chunk=4 * 1024 * 1024):
    """Byte offset of the first record carrying prompt_id, found by reading
    backward from the END of the file, so a long session costs no more than its
    last turn does (2026-10-10: no hook may skip itself because a transcript is
    large).

    The first record is accepted only when a record of ANOTHER turn precedes it
    in the window already read (or the window reaches the start of the file);
    otherwise the window doubles. When the prompt cannot be located without
    pulling in more than 64 MB, the answer is 0 and the caller's forward
    streaming read from the start takes over, which is the previous behavior.
    """
    needle = prompt_id.encode("utf-8", "replace")

    def pid_of(raw):
        try:
            rec = json.loads(raw)
        except Exception:
            return None
        return rec.get("promptId") if isinstance(rec, dict) else None

    try:
        size = os.path.getsize(path)
        window = chunk
        with open(path, "rb") as fh:
            while True:
                pos = max(0, size - window)
                fh.seek(pos)
                buf = fh.read(size - pos)
                base = 0
                if pos > 0:
                    nl = buf.find(b"\n")
                    base = len(buf) if nl < 0 else nl + 1
                lines, at = [], pos + base
                for ln in buf[base:].split(b"\n"):
                    lines.append((at, ln))
                    at += len(ln) + 1
                first = None
                for idx, (_o, ln) in enumerate(lines):
                    if needle in ln and pid_of(ln) == prompt_id:
                        first = idx
                        break
                if first is not None:
                    if pos == 0:
                        return lines[first][0]
                    for _o, ln in reversed(lines[:first]):
                        if b'"promptId"' in ln:
                            other = pid_of(ln)
                            if other and other != prompt_id:
                                return lines[first][0]
                if pos == 0 or window >= 64 * 1024 * 1024:
                    return 0
                window *= 2
    except OSError:
        return 0


def read_turn(path, prompt_id):
    """Tool names, Bash command strings and host-written notices, THIS TURN.

    THE CEO'S OWN WORDS ARE NOT COLLECTED HERE ANY MORE (2026-09-20, §68). The
    `said` list this function used to build had exactly one consumer -- the
    deleted hold predicate -- so it is gone with it, and the only prompt text
    that still reaches the caller is a HOST-WRITTEN notice (`notices`), which
    is not a person speaking but the transport a teammate's completion arrives
    on.

    The turn is scoped by promptId, using the binary's own semantic: a UUID
    correlating a user prompt with all subsequent events until the next prompt.
    Assistant records carry no promptId, so the turn is the file-order span from
    the first record bearing this prompt_id to the end of the file -- at Stop
    time there is no next prompt yet.

    ABSENT A prompt_id THE TURN CANNOT BE SCOPED, and here that means the gate
    is not evaluable at all: an unscoped read would attribute the whole
    session's merges to this turn AND the whole session's Agent calls to it,
    and the two errors do not cancel. So it returns None and the caller stands
    down. The sibling gate could afford to widen; this one cannot.

    At Stop time the transcript already holds every tool_use and tool_result of
    the turn; it does NOT yet hold the final assistant text. Nothing here needs
    that text -- this gate reads actions, not claims.
    """
    if not prompt_id:
        return None
    if not path or not os.path.isfile(path):
        return None
    try:
        os.path.getsize(path)
    except OSError:
        return None
    # No size cap: the turn is located from the end of the file.
    offset = turn_start_offset(path, prompt_id)

    tools, bash, notices, cwd = [], [], [], ""
    # Bash commands whose own result says "Already up to date": a merge that
    # changed nothing (hunt part 3, finding 2). Keyed by tool_use id.
    bash_by_id, uptodate = {}, set()
    backgrounded = False
    started = False
    # Every tool call as (id, name, backgrounded), and the ids whose result
    # says the call FAILED or was REFUSED: a refused Agent call started no
    # agent, so neither it nor its background flag counts (hunt part 3, 19).
    calls, errored = [], set()
    # WHEN THE TURN BEGAN, from the host's own record of it: the timestamp on
    # the first record bearing this prompt_id. confirm_landing uses it to ask
    # whether a merge or push CHANGED a ref during this turn, rather than only
    # whether the ref contains the commit (hunt part 3, finding 2). None when
    # the record carries no timestamp.
    started_at = None
    try:
        with open(path, "rb") as fh:
            fh.seek(offset)
            for raw in fh:
                line = raw.decode("utf-8", "replace").strip()
                if not line:
                    continue
                try:
                    rec = json.loads(line)
                except Exception:
                    continue
                if rec.get("promptId") == prompt_id:
                    if not started:
                        started = True
                        started_at = _epoch(rec.get("timestamp"))
                    if rec.get("cwd"):
                        cwd = rec["cwd"]
                if not started:
                    continue
                msg = rec.get("message") or {}
                content = msg.get("content")
                # ONLY THE HOST-WRITTEN PROMPTS ARE KEPT, and that is the whole
                # of what this branch does now. A <task-notification> arrives on
                # the user channel carrying a teammate's whole handoff, and it
                # is the ONLY place a teammate's completion is visible from
                # inside the turn that has to answer for it (see
                # agent_finishes).
                #
                # THE OPERATOR'S OWN PROMPTS ARE READ AND DISCARDED -- kept
                # apart here rather than filtered away upstream so that the one
                # thing this file must never do again is visible at the line
                # where it would happen. Nothing downstream receives his text.
                # Until 2026-09-20 it was collected as `said` and fed to a
                # regex that could stand the gate down; the CEO ruled that no
                # hook infers anything from his words, and the collection went
                # with the predicate.
                if rec.get("type") == "user" and (rec.get("origin") or rec.get("promptSource")):
                    parts = []
                    if isinstance(content, str):
                        parts.append(content)
                    elif isinstance(content, list):
                        for b in content:
                            if isinstance(b, dict) and b.get("type") == "text":
                                parts.append(b.get("text", ""))
                    for t in parts:
                        if MACHINE_PROMPT_RE.search(t):
                            notices.append(t)
                if rec.get("type") == "assistant" and isinstance(content, list):
                    for b in content:
                        if not isinstance(b, dict) or b.get("type") != "tool_use":
                            continue
                        name = b.get("name", "")
                        inp = b.get("input") or {}
                        if not isinstance(inp, dict):
                            inp = {}
                        calls.append((b.get("id"), name,
                                      inp.get("run_in_background") is True))
                        # A tool call sent to the background IS work started:
                        # the turn handed something off and is now waiting on
                        # it, which is the same shape as a dispatch and has to
                        # be read the same way. See started_work().
                        if name == "Bash":
                            cmd_text = str(inp.get("command", "") or "")
                            bash.append(cmd_text)
                            if b.get("id"):
                                bash_by_id[b["id"]] = cmd_text
                if rec.get("type") == "user" and isinstance(content, list):
                    for b in content:
                        if not isinstance(b, dict) or b.get("type") != "tool_result":
                            continue
                        if b.get("is_error") is True and b.get("tool_use_id"):
                            errored.add(b["tool_use_id"])
                        cmd_text = bash_by_id.get(b.get("tool_use_id"))
                        if cmd_text is None:
                            continue
                        res = b.get("content")
                        if isinstance(res, list):
                            res = " ".join(x.get("text", "") for x in res
                                           if isinstance(x, dict))
                        if isinstance(res, str) and UPTODATE_RE.search(res):
                            uptodate.add(cmd_text)
    except OSError:
        return None
    if not started:
        return None
    for cid, cname, bg in calls:
        if cid is not None and cid in errored:
            continue
        tools.append(cname)
        backgrounded = backgrounded or bg
    # NO `said` KEY, deliberately. A caller reaching for the operator's words
    # gets a KeyError at the line that reaches, not a quiet empty string that
    # lets the old behavior grow back looking harmless.
    return {"tools": tools, "bash": bash, "uptodate": uptodate,
            "notices": notices, "backgrounded": backgrounded, "cwd": cwd,
            "started_at": started_at}


def _epoch(stamp):
    """An ISO-8601 transcript timestamp as epoch seconds, or None."""
    if not isinstance(stamp, str) or not stamp:
        return None
    try:
        from datetime import datetime
        return datetime.fromisoformat(stamp.replace("Z", "+00:00")).timestamp()
    except Exception:
        return None


# --------------------------------------------------------------------------
# PREDICATE 1 -- did this turn land anything?
# --------------------------------------------------------------------------

HEREDOC_RE = re.compile(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1")

# Flags of `git merge` that TAKE A VALUE. Without this list the value is read
# as the merged ref: `git merge -m "…" branch` would confirm against a commit
# message. Nothing downstream would notice, because an unresolvable ref simply
# makes the gate quiet — which is how a check goes dead without failing.
MERGE_VALUE_FLAGS = {"-m", "--message", "-F", "--file", "-s", "--strategy",
                     "-X", "--strategy-option", "-S", "--gpg-sign",
                     "--into-name", "--cleanup"}


def strip_heredocs(cmd):
    """Drop heredoc BODIES, keeping the line that opens them.

    A heredoc body is data, not commands. These commands routinely carry whole
    python programs inside `<<'PY' … PY`, and such a body will contain quotes
    that do not balance as shell — which breaks any quote-aware scan of the
    rest of the line, and can also contain the literal text of a git command
    that was never run.
    """
    lines = (cmd or "").split("\n")
    out, i = [], 0
    while i < len(lines):
        line = lines[i]
        out.append(line)
        m = HEREDOC_RE.search(line)
        if m:
            tag = m.group(2)
            j = i + 1
            while j < len(lines) and lines[j].strip() != tag:
                j += 1
            i = j + 1
            continue
        i += 1
    return "\n".join(out)


def segments(cmd):
    """Split a shell command into segments, RESPECTING QUOTES.

    A regex split on `[;\\n|&]` is wrong here and the failure is silent rather
    than loud: every real landing in this repository is written

        git merge --no-ff <branch> -m "<subject>
        <a body with blank lines and newlines in it>"

    and a naive newline split cuts that quoted message in half, leaves shlex
    with an unbalanced quote, and drops the merge on the floor. Replayed
    against the turn this gate was written for, that bug found 3 pushes and
    ZERO of the 4 merges.
    """
    text = strip_heredocs(cmd)
    segs, buf, q, i = [], [], None, 0
    n = len(text)
    while i < n:
        c = text[i]
        if q:
            buf.append(c)
            if c == "\\" and q == '"' and i + 1 < n:
                buf.append(text[i + 1])
                i += 2
                continue
            if c == q:
                q = None
            i += 1
            continue
        if c in ("'", '"'):
            q = c
            buf.append(c)
            i += 1
            continue
        if c == "\\" and i + 1 < n:
            buf.append(c)
            buf.append(text[i + 1])
            i += 2
            continue
        if c in "\n;|&":
            segs.append("".join(buf))
            buf = []
            while i < n and text[i] in "\n;|&":
                i += 1
            continue
        buf.append(c)
        i += 1
    segs.append("".join(buf))
    return [s for s in segs if s.strip()]


def _argv(seg):
    try:
        argv = shlex.split(seg, comments=False)
    except ValueError:
        return None
    while argv and re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", argv[0]):
        argv.pop(0)
    return argv or None


def landing_ops(commands, base_cwd):
    """[(repo_path, 'merge'|'push', ref)] for every merge/push in the traffic.

    Parsed as COMMAND LINES, never matched as substrings: `echo "git push"` is
    not a push, and a gate that thinks it is teaches people it cries wolf. `cd`
    is tracked across segments because that is how these commands are actually
    written -- `cd <repo> && git merge <branch>`.
    """
    ops = []
    for cmd in commands:
        cwd = base_cwd
        for seg in segments(cmd):
            argv = _argv(seg)
            if not argv:
                continue
            head = os.path.basename(argv[0])
            if head == "cd" and len(argv) > 1:
                target = argv[1]
                if not target.startswith("-"):
                    target = os.path.expanduser(target)
                    cwd = target if os.path.isabs(target) else os.path.normpath(
                        os.path.join(cwd or ".", target))
                continue
            if head != "git":
                continue
            repo, k, sub = "", 1, ""
            while k < len(argv):
                a = argv[k]
                if a == "-C" and k + 1 < len(argv):
                    repo = argv[k + 1]
                    k += 2
                    continue
                if a in ("-c", "--git-dir", "--work-tree") and k + 1 < len(argv):
                    k += 2
                    continue
                if a.startswith("-"):
                    k += 1
                    continue
                sub = a
                k += 1
                break
            if sub not in ("merge", "push"):
                continue
            rest = argv[k:]
            # --abort / --continue / --quit / --dry-run are not landings.
            if any(a in ("--abort", "--continue", "--quit", "--dry-run", "-n",
                         "--no-commit") for a in rest):
                continue
            # The merged ref. Value-taking flags are consumed with their value
            # (see MERGE_VALUE_FLAGS), and a redirection token is never a ref.
            # For a push nothing here is used at all -- the confirmation is
            # HEAD against the remote-tracking ref.
            ref = ""
            j = 0
            while j < len(rest):
                a = rest[j]
                if a in MERGE_VALUE_FLAGS:
                    j += 2
                    continue
                if a.startswith("-"):
                    j += 1
                    continue
                if ">" in a or "<" in a:
                    j += 1
                    continue
                ref = a
                break
            path = repo or cwd or ""
            path = os.path.expanduser(path)
            if path and not os.path.isabs(path):
                path = os.path.normpath(os.path.join(cwd or ".", path))
            ops.append((path, sub, ref))
    return ops


def _git(repo, *args, **kw):
    try:
        p = subprocess.run(["git", "-C", repo] + list(args),
                           capture_output=True, text=True, timeout=kw.get("timeout", 10))
    except Exception:
        return None
    if p.returncode != 0:
        return None
    return p.stdout.strip()


def _recorded_integration_branch(repo):
    """The branch RECORDED as the one this body of work integrates on, or "" --
    asked of mega-lander/workspaces.py, which is the only implementation of the
    question (point 14). Loading it by path is a duplicated LOADER, not a
    duplicated ANSWER."""
    lib = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                       "..", "mega-lander", "workspaces.py")
    try:
        import importlib.util
        spec = importlib.util.spec_from_file_location("workspaces_answer", lib)
        ws = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(ws)
        branch, _tip, why_not = ws.integration_for(repo)
    except Exception:
        return ""
    return "" if why_not else (branch or "")


ZERO_SHA = "0" * 40


def _reflog(repo, refname):
    """[(old, new, epoch, message)] for refname, oldest first, or None.

    Read from git's own reflog file, which states BOTH ends of every update
    (the `git reflog` porcelain gives only the new one). None when there is no
    file to read (reflogs off, a reftable store, a ref that was never
    updated): the caller then confirms no land (finding 2, v3).
    """
    rel = _git(repo, "rev-parse", "--git-path", "logs/" + refname)
    if not rel:
        return None
    path = rel if os.path.isabs(rel) else os.path.join(repo, rel)
    out = []
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            for line in fh:
                head, _, msg = line.rstrip("\n").partition("\t")
                parts = head.split(" ")
                if len(parts) < 4:
                    continue
                try:
                    when = int(parts[-2])
                except ValueError:
                    continue
                out.append((parts[0], parts[1], when, msg))
    except OSError:
        return None
    return out or None


def _contains(repo, commit, tip):
    if not tip or tip == ZERO_SHA:
        return False
    try:
        p = subprocess.run(["git", "-C", repo, "merge-base", "--is-ancestor", commit, tip],
                           capture_output=True, text=True, timeout=10)
    except Exception:
        return False
    return p.returncode == 0


def _in_window(when, since):
    # Whole seconds on the reflog, milliseconds on the transcript: the turn's
    # own second counts as inside it.
    return since is None or when >= int(since)


def changed_in_turn(repo, kind, ref, head, since):
    """Did THIS TURN move a ref so that it now carries the landed commit?

    THE QUESTION THE ANCESTRY TEST CANNOT ANSWER (hunt part 3, finding 2). A
    merge of a ref that was already merged changes nothing and still leaves
    the ref an ancestor of HEAD; a push of a branch already at its upstream
    pushes nothing and still leaves HEAD equal to it. The first fix excluded
    only a merge whose own output said "Already up to date", so the same
    no-op with its output sent to a log still counted as a land. The answer
    does not come from output at all: it comes from the reflog git keeps of
    every update to a ref, read from the start of this turn.

    merge  an update of the current branch (HEAD when detached), inside the
           turn, from a tip that did NOT contain <ref> to one that does. The
           creation of a branch is not a merge and does not count.
    push   an `update by push` of the branch's remote-tracking ref, inside
           the turn, to HEAD.

    Without a turn start (a transcript with no timestamps) every entry is
    in the window, which is the most this can say. True/False, or None when
    no reflog could be read. The caller treats None as "not confirmed": the
    identity answer alone is true of every no-op merge, so a repository that
    keeps no reflog rounds to quiet (hunt part 3, finding 2, v3).
    """
    if kind == "merge":
        branch = _git(repo, "symbolic-ref", "-q", "HEAD")
        entries = _reflog(repo, branch or "HEAD")
        if entries is None:
            return None
        commit = _git(repo, "rev-parse", "--verify", "-q", ref + "^{commit}")
        if not commit:
            return False
        for old, new, when, _msg in entries:
            if not _in_window(when, since) or old == ZERO_SHA or old == new:
                continue
            if _contains(repo, commit, new) and not _contains(repo, commit, old):
                return True
        return False
    branch = _git(repo, "symbolic-ref", "--short", "-q", "HEAD")
    if not branch:
        return False
    upstream = _git(repo, "rev-parse", "--symbolic-full-name", branch + "@{upstream}")
    if not upstream:
        return False
    entries = _reflog(repo, upstream)
    if entries is None:
        return None
    return any(_in_window(when, since) and new == head and old != new
               and msg.startswith("update by push")
               for old, new, when, msg in entries)


def confirm_landing(repo, kind, ref, since=None):
    """Did the operation actually change this repository? Identity, not prose.

    merge <ref>  the merged tip is now an ancestor of HEAD. A merge that
                 conflicted, was aborted, or was never committed fails this.
    push         HEAD equals the branch's remote-tracking ref. A push that was
                 rejected, or that never ran, fails this.

    Both are content identities, and neither parses command output that a
    pipe or a redirect may have swallowed. Both now also require that the
    ref MOVED during this turn (changed_in_turn): inclusion alone is true of
    every no-op merge and push of work that landed before the turn began.
    `since` is the turn's start in epoch seconds, or None when unknown.
    """
    if not repo or not os.path.isdir(repo):
        return False
    top = _git(repo, "rev-parse", "--show-toplevel")
    if not top:
        return False
    head = _git(repo, "rev-parse", "HEAD")
    if not head:
        return False
    if kind == "merge":
        if not ref:
            return False
        if _git(repo, "rev-parse", "--verify", "-q", ref + "^{commit}") is None:
            return False
        try:
            p = subprocess.run(["git", "-C", repo, "merge-base", "--is-ancestor", ref, "HEAD"],
                               capture_output=True, text=True, timeout=10)
        except Exception:
            return False
        if p.returncode != 0:
            return False
        # AND THIS TURN MOVED THE BRANCH TO CARRY IT (finding 2). A merge of
        # something already merged passes the ancestry test above and lands
        # nothing.
        # No reflog to read (logging off, a reftable store) is NOT a yes: a
        # no-op merge leaves the same inclusion as a real one, so an answer
        # that cannot see the change rounds to quiet (finding 2, v3).
        if changed_in_turn(repo, "merge", ref, head, since) is not True:
            return False
        # AND IT REACHED THE BRANCH THIS WORK INTEGRATES ON (point 14). HEAD
        # alone is the literal effect of `git merge` -- it merges into the
        # current branch and nothing else -- so it is not a guess and it stays.
        # But "a land happened" is a claim about the branch RECORDED for this
        # body of work, and this gate is not allowed its own answer to that.
        # A merge into some other branch that happens to be checked out is not
        # a land, and treating it as one is how this gate would fire on a turn
        # that landed nothing.
        #
        # Where nothing is recorded, HEAD alone stands: abstaining here means
        # "no land was confirmed", which is this gate's QUIET direction, and a
        # gate that cannot answer must round toward quiet.
        branch = _recorded_integration_branch(repo)
        if not branch:
            return True
        try:
            q = subprocess.run(["git", "-C", repo, "merge-base", "--is-ancestor", ref, branch],
                               capture_output=True, text=True, timeout=10)
        except Exception:
            return False
        return q.returncode == 0
    branch = _git(repo, "symbolic-ref", "--short", "-q", "HEAD")
    if not branch:
        return False
    up = _git(repo, "rev-parse", "--verify", "-q", branch + "@{upstream}")
    if not (bool(up) and up == head):
        return False
    # The same question for a push: did one in this turn move the upstream?
    # A push of a branch already at its upstream ("Everything up-to-date")
    # leaves HEAD equal to it and pushed nothing.
    # As for a merge, an unreadable reflog confirms nothing (finding 2, v3).
    return changed_in_turn(repo, "push", ref, head, since) is True


# --------------------------------------------------------------------------
# PREDICATE 1b -- did a teammate FINISH inside this turn?
# --------------------------------------------------------------------------
#
# THE SECOND COMPLETION SIGNAL, AND WHY THE GATE WAS HALF-BLIND WITHOUT IT.
# The first version of this gate triggered on a LANDING and nothing else. That
# reads only one of the two ways work completes here. The other is a teammate
# handing its work back -- and the turn that answers a finished teammate by
# LISTING what is left instead of STARTING it is the same failure wearing
# different clothes. It was reported on the same night as the landing one.
#
# The signal is STRUCTURED AND HOST-WRITTEN, never prose. When an agent stops,
# the host injects a user record carrying
#
#     <task-notification>
#       <task-id>...</task-id>
#       <status>completed</status>
#       <summary>Agent "<its task title>" finished</summary>
#       <note>A task-notification fires each time this agent stops with no live
#             background children of its own. ...</note>
#       <result>...</result>
#     </task-notification>
#
# and that record carries its OWN promptId, so it opens a turn and read_turn's
# window is already scoped to exactly the right span. Nothing is inferred, and
# nothing is read from the orchestrator's own words.
#
# THREE NARROWINGS, every one of them toward silence:
#   * `<status>completed</status>` is REQUIRED as well as the summary. A
#     notification for a failure, a cancelation or an unknown state is not a
#     delivery.
#   * `Background command "..." completed` is NOT a teammate. The summary shape
#     below matches only the `Agent "..." finished` form, so a finished shell
#     never triggers the gate -- it is ordinary tool traffic.
#   * `Agent "..." was stopped by user` is not a delivery either -- the operator
#     killing an agent is the operator taking control of the turn, and treating
#     it as one would refuse the turn in which he did it. It is excluded BY THE
#     SUMMARY SHAPE rather than by a second rule, and that is worth a sentence
#     because the second rule was written first and then removed. `was stopped by
#     user` and `finished` are DIFFERENT SUMMARIES; a separate exclusion could
#     not be made to go red by any mutation, which is the definition of a check
#     that is not doing anything. Case AA3 asserts the behavior and the mutation
#     `finish-ignores-the-word-finished` proves the summary shape is what carries
#     it.
#
# WHAT IT CANNOT SEE, said here rather than discovered later: the host's own
# note says the same task-id may notify MORE THAN ONCE, because an agent that
# stops can be resumed. So "finished" means "stopped with nothing running", not
# "will never speak again". That is the right reading for this gate anyway --
# at the moment it stops, its work is back with the orchestrator and something
# has to happen next -- but it does mean a resumed teammate produces two
# completion turns. Both are gated. Both should be.

TASK_NOTIFICATION_RE = re.compile(r"<task-notification>", re.I)
COMPLETED_STATUS_RE = re.compile(r"<status>\s*completed\s*</status>", re.I)
AGENT_FINISHED_RE = re.compile(
    "<summary>\\s*Agent\\s+[\"“]?(?P<title>[^\"”<]{1,160}?)[\"”]?"
    "\\s+finished\\s*</summary>", re.I)
MAX_FINISHES = 20


def agent_finishes(notices):
    """Titles of the agents whose completion arrived in THIS turn's window."""
    out, seen = [], set()
    for text in notices or []:
        if not TASK_NOTIFICATION_RE.search(text):
            continue
        if not COMPLETED_STATUS_RE.search(text):
            continue
        m = AGENT_FINISHED_RE.search(text)
        if not m:
            continue
        title = re.sub(r"\s+", " ", m.group("title")).strip()
        if not title or title in seen:
            continue
        seen.add(title)
        out.append(title)
        if len(out) >= MAX_FINISHES:
            break
    return out


# --------------------------------------------------------------------------
# PREDICATE 2 -- was anything STARTED?
# --------------------------------------------------------------------------

def started_work(turn):
    """Did this turn hand work off? Two shapes, both read from tool traffic.

    An `Agent` call is the obvious one. A tool call sent to the BACKGROUND is
    the other, and it belongs HERE rather than in the suppressor below for the
    reason that is the whole point of this file's rewrite: a thing the TURN
    started is work started; a thing that was ALREADY running when the turn
    began is not. Conflating the two is what made the first version of this
    gate fire once in 107 landing turns.
    """
    if "Agent" in turn["tools"]:
        return "Agent"
    if turn.get("backgrounded"):
        return "background task"
    return ""


# --------------------------------------------------------------------------
# WHAT IS STILL RUNNING -- REPORTED, AND NOT A SUPPRESSOR ANY MORE
# --------------------------------------------------------------------------
#
# THIS IS THE CHANGE THAT MADE THE GATE FIRE. It shipped 2026-08-30 standing
# itself down whenever `background_tasks` held anything at all, and its own
# observation record says what that cost, over 107 landing turns on this
# machine:
#
#     dispatched (correctly silent)     60
#     background-running (STOOD DOWN)   44      <- 41% of every landing turn
#     backlog-empty (correctly silent)   2
#     block                              1
#
# In the operator's live session it was 12 of 20 -- 60%. The gate was not
# enforcing "start the next item"; it was enforcing "start the next item unless
# anything at all is running", and on this machine something always is.
#
# `background_tasks` IS NOT A LIST OF TEAMMATES. Read out of the shipping binary
# (2.1.252) it is `taskRegistry.all()` filtered to status running|pending, over
# ten task types:
#
#     local_agent -> subagent        in_process_teammate -> teammate
#     local_bash  -> shell           monitor_mcp/monitor_ws -> monitor
#     local_workflow -> workflow     mcp_task / dream / auto_mode_scan /
#                                    remote_agent -> cloud session
#
# So a monitor, a leftover background shell, an auto-mode scan, or any one of
# the ten-to-fifteen teammates this orchestrator keeps alive at all times
# disarmed it completely.
#
# THE LEGITIMATE CASE IS REAL, AND IT IS NARROWER THAN THE SUPPRESSOR WAS:
# "a teammate is still running AND THE NEXT STEP DEPENDS ON ITS RESULT". The
# dependency half is the part that matters, and it is already written down --
# in the record's own `Blocked by` column, which is what parse_record() reads.
# A row the record calls unblocked is, by construction, not waiting on the
# running teammate; a row that IS waiting on it is marked blocked and never
# reaches the verdict. The record already answers that case correctly, and the
# suppressor was answering a much easier question in its place.
#
# What survives here is REPORTING. The count goes into the refusal and into the
# observation log, so the operator can see it -- and if the record is wrong in
# the moment, that is what the one declared sentence is for. It no longer votes.

# --------------------------------------------------------------------------
# PREDICATE 4 -- is anything still running?
# --------------------------------------------------------------------------

TERMINAL = {"completed", "complete", "done", "failed", "failure", "error",
            "cancelled", "canceled", "terminated", "killed", "stopped", "finished"}


def still_running(payload):
    bt = payload.get("background_tasks")
    if not isinstance(bt, list) or not bt:
        return 0
    n = 0
    for t in bt:
        if isinstance(t, dict):
            st = str(t.get("status") or t.get("state") or "").strip().lower()
            if st in TERMINAL:
                continue
        # A non-dict entry, or one with no status we recognize, counts as
        # RUNNING. Unrecognized means quiet.
        n += 1
    return n


# --------------------------------------------------------------------------
# THE OPERATOR'S OWN HOLD -- DELETED 2026-09-20, AND THE SPACE IS KEPT EMPTY
# --------------------------------------------------------------------------
#
# What stood here: HOLD_RE, OFF_DUTY_RE and hold_signal(). Two regexes over the
# CEO'S OWN TYPED MESSAGES -- "hold", "stand down", "don't dispatch", "going to
# bed", "calling it a night" -- and a function that stood this blocking gate
# down whenever one matched.
#
# THE RULING (CEO, 2026-09-20): "since when can a fucking hook be automatically
# inferred from my words??? since when is such a thing become reliably
# possible????" A hook may check what the orchestrator DID. It never decides
# what HE MEANT.
#
# The defense written here was that the error was one-directional -- it could
# only ever stand a gate DOWN, never accuse anybody -- and that argument is
# true and is not sufficient. Standing a blocking gate down IS a decision, and
# the decision was being taken from a sentence he typed for a person to read.
# A direction of error is a property of a mistake; the ruling is about who is
# entitled to make the judgment at all.
#
# WHAT REPLACES IT: nothing here, and something already present. A turn he told
# to stop ends through stop_declaration() below -- `stop-declared: ceo-owns-it`
# plus a reason, written by the orchestrator, in the orchestrator's own final
# message, where the CEO can read what it claimed he wanted. That is the
# orchestrator's own act and it is checkable. The regexes are not narrowed,
# not moved and not kept behind a flag: a language predicate over his words
# that still exists is one that gets reached for again.
#
# CODE_SPAN_RE survives because stop_declaration() uses it to keep a
# declaration quoted inside a fenced block from exempting anything.

CODE_SPAN_RE = re.compile(r"```.*?```|`[^`\n]*`", re.S)


# --------------------------------------------------------------------------
# THE DECLARATION -- the only live escape, and it is a SENTENCE
# --------------------------------------------------------------------------
#
# THE EARLIER VERSION OF THIS FILE REFUSED TO HAVE ONE, and its reasoning is
# still on the record two paragraphs up in the module docstring: an in-the-
# moment token gets reached for at exactly the moment the gate is working. That
# argument is right about TOKENS and wrong about this, and the difference is
# the whole design.
#
# A flag is free. `--force`, `SKIP=1`, a bare marker -- none of them cost the
# writer anything, so they get typed reflexively and the defense becomes a
# formality with a hook attached. A DECLARATION is not free: it makes the
# writer state WHICH of the three legitimate stops applies and WHY, in a
# sentence, in the reply the CEO reads. To write "nothing unblocked remains
# because the corpus rebuild is the only open row and Mark owns it", you have to
# have looked. THAT IS THE BEHAVIOR CHANGE -- half the value of this gate is
# making the stop deliberate rather than absent-minded, and a stop that has been
# thought about and justified in front of the person it affects is the outcome,
# not a loophole in it.
#
# It is the same discipline `dialect-exempt:` and `main-checkout-run:` already
# use in this engine, and it carries their rule verbatim: A BARE MARKER EXEMPTS
# NOTHING. `stop-declared:` on its own, or followed by four words, is rejected
# and the refusal says which of the two tests it failed.
#
# THREE CASES AND NO OTHERS. The set is closed on purpose: an open one would
# accept "stop-declared: reasons" and be a flag again.
#
# CODE SPANS ARE STRIPPED BEFORE THE SEARCH, and that is not a nicety. The
# refusal text below QUOTES the declaration line so the operator knows what to
# write. Without the strip, pasting the refusal -- or quoting this guard's own
# documentation -- would disarm it, which is a gate that can be switched off by
# reading it out loud.

DECLARED_CASES = {
    "nothing-unblocked":
        "everything unblocked is genuinely done, and what is left needs a CEO "
        "decision or an external party",
    "ceo-owns-it":
        "the CEO stopped this, or asked a question whose answer IS the "
        "deliverable",
    "waiting-on-teammate":
        "a teammate is still running and the next step depends on its result",
}

DECLARATION_RE = re.compile(
    r"^[ \t>*\-\u2022]*stop-declared:[ \t]*(?P<case>[A-Za-z][A-Za-z0-9-]{2,40})"
    r"[ \t]*(?:[-\u2013\u2014:]+[ \t]*)?(?P<why>.*)$", re.M)

# A reason has to be a reason. Six words and thirty characters is not a high
# bar -- it is the bar between a sentence and a shrug, and it was set there
# because "n/a", "see above" and "as discussed" all clear anything lower.
MIN_DECLARATION_WORDS = 6
MIN_DECLARATION_CHARS = 30


def stop_declaration(message):
    """None, or {case, why, ok, problem} for the declaration in this reply.

    Returns a REJECTION rather than nothing when the line is present but
    malformed. A malformed declaration that silently fails is the worst of both
    worlds: the turn is refused and the writer cannot tell why, so the next
    thing he does is unwire the gate.
    """
    if not message:
        return None
    text = CODE_SPAN_RE.sub(" ", message)
    m = DECLARATION_RE.search(text)
    if not m:
        return None
    case = m.group("case").strip().lower()
    why = re.sub(r"\s+", " ", m.group("why") or "").strip().strip("*_`\"'" )
    out = {"case": case, "why": why, "ok": False, "problem": ""}
    if case not in DECLARED_CASES:
        out["problem"] = ("\"%s\" is not one of the three cases: %s"
                          % (case, ", ".join(sorted(DECLARED_CASES))))
        return out
    if len(why) < MIN_DECLARATION_CHARS or len(why.split()) < MIN_DECLARATION_WORDS:
        out["problem"] = ("the reason is %d words / %d characters; a declaration "
                          "needs at least %d words and %d characters, because a "
                          "bare marker exempts nothing"
                          % (len(why.split()), len(why), MIN_DECLARATION_WORDS,
                             MIN_DECLARATION_CHARS))
        return out
    if why.lower().replace("-", " ") == case.replace("-", " "):
        out["problem"] = "the reason only restates the case name"
        return out
    out["ok"] = True
    return out


# --------------------------------------------------------------------------
# PREDICATE 3 -- the record, and what is left in it
# --------------------------------------------------------------------------

DEFAULT_RECORD = "RICH-TODOs.md"
DEFAULT_SECTION = "Next"


def _repo_top(path):
    if not path or not os.path.isdir(path):
        return ""
    return _git(path, "rev-parse", "--show-toplevel") or ""


def backlog_candidates(landed_repos, entity_root, record_name):
    """Every place on this machine that could hold the orchestrator's backlog.

    A repository qualifies only if it declares `.ceo-todos` AND carries the
    record. The declaration requirement is not decoration: it is what
    guarantees the refusal has somewhere legitimate to send the row (see the
    module docstring, "THE ESCAPE"). Search order is the landed repositories,
    then their siblings, then the entity and its siblings -- deterministic, and
    the chosen path is printed in every refusal so it is never a mystery.
    """
    seen, out = set(), []

    def consider(p):
        top = _repo_top(p)
        if not top or top in seen:
            return
        seen.add(top)
        if not os.path.isfile(os.path.join(top, ".ceo-todos")):
            return
        rec = os.path.join(top, record_name)
        if os.path.isfile(rec):
            out.append(rec)

    def with_siblings(p):
        consider(p)
        parent = os.path.dirname(os.path.abspath(p)) if p else ""
        if not parent or not os.path.isdir(parent):
            return
        try:
            for d in sorted(os.listdir(parent))[:60]:
                consider(os.path.join(parent, d))
        except OSError:
            pass

    for r in landed_repos:
        with_siblings(r)
    if entity_root:
        with_siblings(entity_root)
    return out


STRIP_RE = re.compile(r"(\*\*|~~|`|\*|_)")
REF_RANGE_RE = re.compile(r"(\d+)\s*[-‐-―]\s*(\d+)")

# A cell that says the work is finished. The record strikes such rows through
# as well, so this is a second, independent reading of the same fact.
DONE_WORDS = {"done", "landed", "closed", "shipped", "merged", "complete", "completed"}

# A cell that states nothing is in the way. Everything NOT in this set, and not
# matching the "<something> free" shape below, is treated as BLOCKED -- the
# quiet direction, because the cost of a wrong "blocked" is one missed nudge
# and the cost of a wrong "free" is the gate crying wolf.
FREE_WORDS = {"", "-", "--", "–", "—", "n/a", "na", "none", "nothing",
              "nobody", "unblocked", "ready", "now", "free", "no", "nil"}

FREE_SHAPE_RE = re.compile(r"^(?:.{0,40}?\s)?free(?:\s+after\s+(?P<refs>.+))?$")


def _norm(cell):
    s = STRIP_RE.sub("", cell or "")
    return re.sub(r"\s+", " ", s).strip().lower()


def parse_record(text, section):
    """The `## <section>` table, as rows with a derived state.

    Returns (rows, reason). rows is None when the record cannot be read as a
    table at all -- and the caller then goes INERT rather than guessing.

    Each row is {num, item, blocked, state} where state is one of
    'done' / 'free' / 'blocked'. Nothing here is a count; the count is
    len([r for r in rows if r.state == 'free']) and it is computed, never read.
    """
    lines = text.split("\n")
    start = None
    pat = re.compile(r"^#{1,6}\s+" + re.escape(section) + r"\s*$", re.I)
    for i, ln in enumerate(lines):
        if pat.match(ln.strip()):
            start = i + 1
            break
    if start is None:
        return None, "no '%s' section heading" % section

    body = []
    for ln in lines[start:]:
        if re.match(r"^#{1,6}\s+\S", ln.strip()):
            break
        body.append(ln)

    table = [ln.strip() for ln in body if ln.strip().startswith("|")]
    if len(table) < 3:
        return None, "the '%s' section holds no table" % section

    def cells(ln):
        parts = ln.split("|")
        if parts and not parts[0].strip():
            parts = parts[1:]
        if parts and not parts[-1].strip():
            parts = parts[:-1]
        return [p.strip() for p in parts]

    header = cells(table[0])
    if len(header) < 2:
        return None, "the table header has fewer than two columns"
    sep = cells(table[1])
    if not sep or not all(re.match(r"^:?-{2,}:?$", s) for s in sep if s):
        return None, "the table has no header separator"

    # Column roles are DERIVED from the header, never assumed by position.
    blocked_col = None
    for i, h in enumerate(header):
        if re.search(r"block", h, re.I):
            blocked_col = i
    if blocked_col is None:
        blocked_col = len(header) - 1
    item_col = 1 if len(header) > 1 else 0
    if item_col == blocked_col:
        item_col = 0

    rows, by_num = [], {}
    for ln in table[2:]:
        c = cells(ln)
        if len(c) < 2:
            continue
        num_raw = c[0] if len(c) > 0 else ""
        item_raw = c[item_col] if item_col < len(c) else ""
        blk_raw = c[blocked_col] if blocked_col < len(c) else ""
        struck = "~~" in num_raw or "~~" in item_raw
        blk = _norm(blk_raw)
        state = "blocked"
        if struck or blk in DONE_WORDS:
            state = "done"
        else:
            m = FREE_SHAPE_RE.match(blk)
            if blk in FREE_WORDS:
                state = "free"
            elif m:
                state = "free" if m.group("refs") is None else "deferred"
                if state == "deferred":
                    state = ("deferred", m.group("refs"))
        row = {"num": _norm(num_raw), "num_raw": num_raw.strip(),
               "item": item_raw.strip(), "blocked": blk_raw.strip(),
               "state": state}
        rows.append(row)
        if row["num"]:
            by_num[row["num"]] = row

    # "<something> free after 1-2" -- resolve the references. Every referenced
    # row must exist AND be done; anything else stays blocked. This is derived
    # from the same table, so it can never disagree with a count somebody typed.
    for row in rows:
        st = row["state"]
        if not isinstance(st, tuple):
            continue
        refs = st[1]
        nums = set()
        for a, b in REF_RANGE_RE.findall(refs):
            try:
                lo, hi = int(a), int(b)
            except ValueError:
                nums = None
                break
            if hi - lo > 50:
                nums = None
                break
            nums.update(str(n) for n in range(lo, hi + 1))
        if nums is None:
            row["state"] = "blocked"
            continue
        rest = REF_RANGE_RE.sub(" ", refs)
        nums.update(re.findall(r"\d+", rest))
        if not nums:
            row["state"] = "blocked"
            continue
        ok = True
        for n in nums:
            target = by_num.get(n)
            if target is None or target["state"] != "done":
                ok = False
                break
        row["state"] = "free" if ok else "blocked"

    if not rows:
        return None, "the '%s' table has no rows" % section
    return rows, ""


def summarize(item, limit=160):
    """One line of the row, for the refusal. Never the whole cell.

    Kept short on purpose: the refusal names the row so it can be found, it
    does not reproduce the record. The record is the record.
    """
    s = re.sub(r"\s+", " ", item or "").strip()
    return s[:limit] + ("..." if len(s) > limit else "")


# --------------------------------------------------------------------------
# main
# --------------------------------------------------------------------------


def main():
    try:
        payload = json.load(sys.stdin)
    except Exception:
        return 0  # unparseable payload never wedges turn-end

    if payload.get("stop_hook_active"):
        # Already blocked once this turn. A gate that re-blocks its own retry is
        # a gate that can strand a session.
        return 0

    entity_root = (os.environ.get("RICHOS_IDLE_ENTITY_ROOT")
                   or payload.get("cwd") or os.getcwd())
    record_name = os.environ.get("RICHOS_IDLE_RECORD") or DEFAULT_RECORD
    section = os.environ.get("RICHOS_IDLE_SECTION") or DEFAULT_SECTION
    enforce = os.environ.get("RICHOS_IDLE_ENFORCE", "1") != "0"

    turn = read_turn(payload.get("transcript_path"), payload.get("prompt_id"))
    if turn is None:
        # No scoped turn, no gate. Silent: this is the ordinary state of every
        # session that is not the orchestrator's, and a line here would be
        # printed thousands of times to say nothing.
        return 0

    message = payload.get("last_assistant_message") or ""

    base_cwd = turn.get("cwd") or entity_root
    ops = []
    for cmd in turn["bash"]:
        cmd_ops = landing_ops([cmd], base_cwd)
        # A lone merge whose own output says "Already up to date" moved no
        # branch, so it completed nothing and owes no start. Only that narrow
        # case is dropped: a command with several git operations has one output
        # that cannot say which of them it was, and output this gate cannot see
        # (swallowed by a pipe) leaves the op in place, as before.
        if (cmd in turn.get("uptodate", ()) and len(cmd_ops) == 1
                and cmd_ops[0][1] == "merge"):
            continue
        ops.extend(cmd_ops)

    landed, unconfirmed = [], []
    for repo, kind, ref in ops:
        top = _repo_top(repo)
        if confirm_landing(repo, kind, ref, turn.get("started_at")):
            if top and top not in landed:
                landed.append(top)
        else:
            unconfirmed.append((repo, kind, ref))

    # TERM 1b. The other way work completes here. `ops` is no longer an early
    # exit, because a turn that started no git command at all can still be a
    # turn in which a teammate handed its work back.
    finishes = agent_finishes(turn.get("notices"))
    started = started_work(turn)
    running = still_running(payload)

    record = {
        "prompt_id": payload.get("prompt_id"),
        "session": payload.get("session_id") or "",
        "ops": len(ops),
        "landed": landed,
        "unconfirmed": len(unconfirmed),
        "finishes": finishes,
        "dispatched": turn["tools"].count("Agent"),
        "running": running,
        "verdict": "pass",
    }

    def log():
        try:
            state = os.path.join(entity_root, ".claude", "state")
            os.makedirs(state, exist_ok=True)
            with open(os.path.join(state, "idle-land-checks.jsonl"), "a",
                      encoding="utf-8") as f:
                f.write(json.dumps(record) + "\n")
        except Exception:
            pass  # the log is a convenience; losing it never changes a verdict

    # TERM 1. NOTHING COMPLETED -- and a turn that completed nothing is not a
    # turn that owed a start. Silent, and by far the commonest outcome.
    if not landed and not finishes:
        record["verdict"] = "no-confirmed-landing" if ops else "no-completion"
        log()
        return 0

    # TERM 2. SOMETHING WAS STARTED. An `Agent` call, or a tool call sent to the
    # background -- both are the turn handing work off, which is the thing the
    # rule asks for.
    if started:
        record["verdict"] = "dispatched"
        record["started"] = started
        log()
        return 0

    # TERM 3a. THE TURN PUT SOMETHING TO THE CEO. Ending a turn on a question he
    # has to answer is not idling; it is the one move that cannot be taken by
    # anybody else. This suppressor is read from a TOOL CALL, not from prose:
    # AskUserQuestion either happened or it did not.
    if "AskUserQuestion" in turn["tools"]:
        record["verdict"] = "asked-ceo"
        log()
        return 0

    # TERM 3b IS GONE (2026-09-20). It read the CEO's own prompts for a hold or
    # an end-of-day and returned 0 on a match. A turn he stopped now ends the
    # way every other legitimate stop ends: `stop-declared: ceo-owns-it`, read
    # below from the orchestrator's OWN final message. See the tombstone above
    # CODE_SPAN_RE for the ruling.

    # From here the turn HAS landed and started nothing, so every remaining
    # outcome is worth a line on stderr. Above this point silence is correct;
    # below it, silence would be a defense that reports "on" while looking at
    # nothing.
    cands = backlog_candidates(landed, entity_root, record_name)
    tag = "(hook: scripts/hooks/guard-idle-land.sh)"

    if not cands:
        record["verdict"] = "inert-no-record"
        log()
        sys.stderr.write(
            "idle-land gate INERT: this turn completed work and started nothing,\n"
            "  but no %s beside a `.ceo-todos` declaration was found near %s.\n"
            "  Nothing was checked -- said out loud so that 'the gate is on' never\n"
            "  means 'the gate looked'. %s\n"
            % (record_name, ", ".join(landed) or entity_root, tag))
        return 0

    if len({os.path.realpath(c) for c in cands}) > 1:
        record["verdict"] = "inert-ambiguous"
        record["candidates"] = cands
        log()
        sys.stderr.write(
            "idle-land gate INERT: %d candidate records found and a gate that guesses\n"
            "  is not a gate:\n%s\n  Declare one, or remove the others. %s\n"
            % (len(cands), "\n".join("      " + c for c in cands), tag))
        return 0

    path = cands[0]
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            text = f.read()
    except OSError as e:
        record["verdict"] = "inert-unreadable"
        log()
        sys.stderr.write("idle-land gate INERT: %s is unreadable (%s). Nothing was\n"
                         "  checked. %s\n" % (path, e, tag))
        return 0

    rows, reason = parse_record(text, section)
    if rows is None:
        record["verdict"] = "inert-unparsable"
        record["reason"] = reason
        log()
        sys.stderr.write("idle-land gate INERT: %s -- %s. The gate will not guess at\n"
                         "  what is left to do, so nothing was checked. %s\n"
                         % (path, reason, tag))
        return 0

    free = [r for r in rows if r["state"] == "free"]
    record["rows"] = len(rows)
    record["free"] = len(free)
    record["record"] = path
    if not free:
        record["verdict"] = "backlog-empty"
        log()
        return 0

    # THE DECLARATION IS READ LAST, ON PURPOSE. It is only ever spent on a turn
    # the gate would otherwise refuse, so it is logged beside the row it
    # overrode and it costs nothing on the turns where it was not needed.
    decl = stop_declaration(message)
    if decl and decl["ok"]:
        record["verdict"] = "declared"
        record["declared"] = {"case": decl["case"], "why": decl["why"]}
        record["top"] = free[0]["num_raw"] or summarize(free[0]["item"], 60)
        log()
        # THE OPERATOR SEES EVERY DECLARATION. A stop justified where nobody
        # reads it is a flag with a longer spelling, so the sentence goes out on
        # the one channel measured to reach him: the wrapper turns this line
        # into a systemMessage. He is the reviewer this discipline names.
        sys.stdout.write("RICHOS_STOP_DECLARED\t%s\t%s\t%s\n"
                         % (decl["case"], decl["why"].replace("\t", " "),
                            record["top"]))
        sys.stderr.write(
            "idle-land gate: STOP DECLARED (%s) over %d unblocked row(s) in %s.\n"
            "  Reason given: %s\n"
            "  Declared, NOT verified -- the record is %s and the operator has\n"
            "  been shown this sentence. %s\n"
            % (decl["case"], len(free), os.path.basename(path), decl["why"],
               path, tag))
        return 0

    top = free[0]
    record["verdict"] = "block" if enforce else "report"
    record["top"] = top["num_raw"] or summarize(top["item"], 60)
    if decl:
        record["declaration_rejected"] = decl["problem"]
    log()

    head = ("=== WORK COMPLETED, NOTHING STARTED — TURN BLOCKED ==="
            if enforce else
            "=== idle-land gate: WORK COMPLETED, NOTHING STARTED (report only) ===")
    out = [head, ""]

    # 1. WHAT COMPLETED. Named, because a refusal that says only "you stopped
    #    early" makes the reader reconstruct the gate's reasoning, and a reader
    #    who has to reconstruct it argues with it instead of acting on it. That
    #    is the same failure one level up.
    out.append("  WHAT COMPLETED IN THIS TURN")
    if landed:
        out.append("      landed, confirmed by identity: %s" % ", ".join(landed))
    for t in finishes[:6]:
        out.append("      teammate finished: %s" % summarize(t, 90))
    if len(finishes) > 6:
        out.append("      ...and %d more" % (len(finishes) - 6))
    out.append("")

    # 2. WHAT WAS NOT STARTED, and what is sitting there to start.
    out.append("  WHAT WAS STARTED IN THIS TURN")
    out.append("      Agent calls: 0.  Background tasks started: 0.")
    out.append("      Questions put to the CEO: 0.")
    if running:
        out.append("      (%d task(s) were ALREADY running when this turn began."
                   % running)
        out.append("       That is not work this turn started, and a row the")
        out.append("       record calls unblocked does not depend on it. If one")
        out.append("       really does, the record is wrong — fix it, or declare.)")
    out.append("")
    out.append("  UNBLOCKED AND AVAILABLE TO START — %d row(s) in %s (%s)"
               % (len(free), os.path.basename(path), section))
    for r in free[:6]:
        out.append("      %-4s %s" % (r["num_raw"] or "-", summarize(r["item"], 120)))
    if len(free) > 6:
        out.append("      ...and %d more" % (len(free) - 6))
    out.append("")

    # 3. THE WAYS THROUGH, INCLUDING THE EXACT LINE. A refusal that describes an
    #    escape without spelling it is a refusal that gets waived by guesswork.
    out.append("  THREE WAYS THROUGH, AND ONLY THREE:")
    out.append("    1. START THE TOP ONE. Dispatch %s now, in this turn, then report."
               % (top["num_raw"] or "it"))
    out.append("    2. MOVE IT. If its next action needs a decision only the CEO can")
    out.append("       make, move the row into the CEO's record — a committed,")
    out.append("       diffable act.")
    out.append("    3. DECLARE THE STOP. If one of the three legitimate stops really")
    out.append("       applies, say so in your reply, on its own line, as plain text")
    out.append("       (NOT inside a code span — a quoted example must never disarm")
    out.append("       this gate), in exactly this form:")
    out.append("")
    out.append("           stop-declared: <case> — <why, in a full sentence>")
    out.append("")
    for name in sorted(DECLARED_CASES):
        out.append("           %-20s %s" % (name, DECLARED_CASES[name]))
    out.append("")
    out.append("       At least %d words and %d characters of reason. A bare marker"
               % (MIN_DECLARATION_WORDS, MIN_DECLARATION_CHARS))
    out.append("       exempts nothing, and the sentence is shown to the CEO — which")
    out.append("       is the point: to write \"nothing unblocked remains because X\"")
    out.append("       you have to have checked.")
    if decl:
        out.append("")
        out.append("  YOUR DECLARATION WAS REJECTED:")
        out.append("      %s" % decl["problem"])
        out.append("      read: stop-declared: %s — %s"
                   % (decl["case"], decl["why"][:80] or "<nothing>"))
    out.append("")
    out.append("  Record: %s" % path)
    if not enforce:
        out.append("  (report-only: IDLE_LAND_ENFORCE=0 — this turn was NOT blocked)")
    else:
        out.append("  Do not weaken or unwire this hook.")
    out.append(tag)
    sys.stderr.write("\n".join(out) + "\n")
    return 2 if enforce else 0


if __name__ == "__main__":
    sys.exit(main())
