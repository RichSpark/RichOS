#!/usr/bin/env python3
"""
turn-manifest.py — the rendering half of the Stop-time TURN MANIFEST.

Called by turn-manifest.sh, which has already resolved the two roots and
decided that this repository adopted the engine. This file reads the turn's
tool traffic and renders it. It never decides whether to run, and it never
decides anything about the turn.

===========================================================================
WHAT THIS IS FOR
===========================================================================
The orchestrator's recurring failure is not lying. It is reporting THE
STRONGEST READING OF A NARROW RESULT — a fact that is true, standing in for
the broader claim it resembles. Five instances from the record:

    what the tool actually returned        what was reported
    -------------------------------------  --------------------------------
    "Message queued for delivery at its    "I've told him"
     next tool round"
    (no tool call at all)                  "dispatching it rather than
                                            queuing it"
    a typed list of 13 passed              "13/13 guards"
    one glob matched 18 files              "18/18 suites"
    no file EXTENSIONS matched             "no media committed"

The CEO caught all five. That makes him the detection mechanism for the
orchestrator's own claims, which is the last unmechanized failure class he has
actually been hit by.

===========================================================================
THIS IS DELIBERATELY NOT A DETECTOR — AND THAT IS THE WHOLE DESIGN
===========================================================================
There is nothing here that reads the orchestrator's prose, nothing that scores
it, nothing that decides whether a sentence overstates. So there is no
threshold to tune and the false-positive rate is ZERO BY CONSTRUCTION.

It does not CATCH an overstatement. It makes one UNSUSTAINABLE, by printing
the true status directly beneath the claim, in the same scroll, where the
reader is already looking. "I've told him" survives only until the next line
says `SendMessage  Message queued for delivery to norm-opus-c1 at its next
tool round.`

That is the same move as Timeline refusing to implement Serialize in
richos-core, and the closed-vocabulary feedback payload: UNREPRESENTABLE
rather than DETECTABLE. Judging the prose is a different job and it already
has an owner — guard-unresolved-claims.sh, which measured its own precision
before it was allowed to block anything.

===========================================================================
IT NEVER BLOCKS, AND THAT IS NOT A LIMITATION
===========================================================================
A manifest that can refuse a turn is a different, worse mechanism: it would
need an opinion about which turns deserve refusing, which is the prose
judgment this file exists to avoid. This one only renders. Every exit is 0.

===========================================================================
WHERE THE TEXT GOES — VERIFIED LIVE, NOT ASSUMED
===========================================================================
A Stop hook reaches the OPERATOR (not just the model) by writing
`{"systemMessage": "..."}` to stdout and exiting 0. Verified against the
shipping binary 2.1.251 in two ways:

  * STATIC. The command-hook runner parses the hook's stdout as JSON and
    carries `systemMessage` out of it (`Pt = Ke && ip(Ke) ? Ke.systemMessage
    : void 0`), independently of exit status; the consumer then yields a
    message of type `hook_system_message` tagged with the hook's event. The
    binary's own bundled documentation names the pattern explicitly under the
    heading "Stop hook that displays message to user".

  * LIVE. A sandbox project with one Stop hook emitting
    {"suppressOutput":true,"systemMessage":"..."} was run headless. The
    stream carried, after the assistant's final text and before the result:
      {"type":"system","subtype":"informational",
       "content":"Stop says: <the message>","level":"notice"}
    A multi-line message survives intact; the host prefixes EVERY line with
    "Stop says: ", which is why the rows below are kept narrow.

engine-status.sh already reaches the operator this same way for its session
banner, so this is the engine's existing idiom rather than a new one.

`suppressOutput: true` accompanies it so the raw JSON does not ALSO land in
the transcript as hook stdout — the manifest should appear once.

===========================================================================
SCOPING TO THE TURN — THE BUG THIS FILE MUST NOT REPEAT
===========================================================================
The sibling Stop guard shipped with tool names collected SESSION-WIDE, so its
"did THIS turn call Agent?" question was permanently answered yes after the
first spawn of the session, and its suite passed for weeks because the
fixtures carried no promptId at all. A manifest with that bug is not
degraded, it is INVERTED: it would print the session's calls under the
heading "this turn".

The turn is scoped by promptId, using the binary's own semantic: a UUID
correlating a user prompt with all subsequent events until the next prompt.
Assistant records carry NO promptId (verified against a real 4,718-record
transcript: `tool_use` blocks live in assistant records with promptId null,
`tool_result` blocks live in user records that DO carry it). So the turn is
the file-order span from the first record bearing this prompt_id to the end
of the file — at Stop time there is no next prompt yet.

AND WHERE THE SIBLING FAILS OPEN WIDE, THIS ONE REFUSES. Absent a prompt_id
the sibling counts the whole session, because for its one consumer the wide
answer is the QUIET one. For a manifest the wide answer is a FALSE one: it
would attribute the session's calls to this turn. So an unscopable turn
renders an explicit UNAVAILABLE notice and lists nothing. Same instinct in
both files — say nothing rather than say wrong — expressed opposite ways
because the consumers differ.

===========================================================================
WHERE THE STATUSES COME FROM — STRUCTURE, NEVER PROSE
===========================================================================
Each row's status is derived from the RESULT RECORD'S SHAPE, in this order:

  no matching tool_result in the turn  -> NO RESULT
  the result carries is_error: true    -> ERROR + the result's own first line
  the result is a JSON object with a
    top-level string field "message"   -> that string, VERBATIM
  other Bash output                    -> RETURNED + size; inner commands unverified
  other tool output                    -> ok + a measured size

The third rule is the one that closes the motivating failure. When a tool
declares its own outcome in a `message` field, THAT SENTENCE IS THE STATUS
and it is reproduced word for word. SendMessage returns
{"success":true,"message":"Message queued for delivery to ... at its next
tool round."} — a manifest that rendered `success: true` would be committing
the very error it exists to prevent, so it renders the sentence and not the
flag.

None of these rules interprets anything. is_error is a field. "message" is a
field. The rest is arithmetic on bytes.

===========================================================================
TRUNCATION IS ANNOUNCED, ALWAYS
===========================================================================
A manifest that silently elides is the defect it exists to prevent, rebuilt.
So:
  * the header tallies EVERY call by tool name, before any row is dropped —
    a SendMessage can never be truncated into invisibility;
  * dropping rows prints how many were dropped and their tally by name;
  * a shortened detail says how many characters were removed.

Exit code: always 0. This hook cannot fail a turn.
"""

import json
import os
import sys

# --------------------------------------------------------------------------
# limits — each one is announced when it bites; see "TRUNCATION IS ANNOUNCED"
# --------------------------------------------------------------------------
MAX_ROWS = 25            # rows printed in full before the omission tally
MAX_DETAIL = 150         # characters of a single row's detail

HOOK_TAG = "(hook: scripts/hooks/turn-manifest.sh)"


# --------------------------------------------------------------------------
# transcript reading
# --------------------------------------------------------------------------

def flatten(content):
    """Reduce a message content field to plain text.

    Handles both shapes a tool_result takes in the transcript: a bare string,
    and a list of typed blocks. Non-text blocks (images) contribute nothing
    rather than a placeholder — a manifest row is about status, and inventing
    "[image]" would be this file writing text it did not receive.
    """
    if isinstance(content, str):
        return content
    parts = []
    if isinstance(content, list):
        for b in content:
            if not isinstance(b, dict):
                continue
            if b.get("type") == "text":
                parts.append(b.get("text", ""))
            elif b.get("type") == "tool_result":
                parts.append(flatten(b.get("content")))
    return "\n".join(parts)


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
    """Return (calls, results, records_examined, error).

    calls   [(tool_use_id, tool_name)] in file order, THIS TURN ONLY
    results {tool_use_id: result_block} for results seen in the same window
    records_examined  how many JSONL records were actually parsed. The suite
            asserts this is non-zero, so a manifest cannot pass its own tests
            by rendering an empty transcript convincingly. This session found
            a scanner reporting CLEAN over an empty corpus and a reporting
            layer dead for weeks; this counter is why there will not be a
            third.
    error   a human sentence if the transcript could not be read at all, else
            None. NEVER conflated with "the turn had no tool calls" — those
            are different facts and a manifest that merged them would be
            hiding exactly the case it was built for.
    """
    if not path:
        return [], {}, 0, "the Stop payload carried no transcript_path"
    if not os.path.isfile(path):
        return [], {}, 0, "the transcript is not a readable file at %s" % path
    try:
        os.path.getsize(path)
    except OSError as exc:
        return [], {}, 0, "the transcript could not be sized (%s)" % exc
    # No size cap: the turn is located from the end of the file.
    offset = turn_start_offset(path, prompt_id)

    calls = []
    results = {}
    examined = 0
    # The window opens on the FIRST record carrying this prompt_id — which is
    # the user's prompt itself — and runs to end of file.
    in_turn = False
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
                examined += 1
                if rec.get("promptId") == prompt_id:
                    in_turn = True
                if not in_turn:
                    continue
                # Subagent traffic must never be reported as the
                # orchestrator's own. Today the orchestrator's transcript
                # carries none (verified: 0 isSidechain records across 4,718),
                # so this is a guard against a future shape change rather than
                # a live filter.
                if rec.get("isSidechain"):
                    continue
                content = (rec.get("message") or {}).get("content")
                if not isinstance(content, list):
                    continue
                for b in content:
                    if not isinstance(b, dict):
                        continue
                    if b.get("type") == "tool_use":
                        calls.append((b.get("id"), b.get("name") or "?"))
                    elif b.get("type") == "tool_result":
                        tid = b.get("tool_use_id")
                        if tid is not None:
                            results[tid] = b
    except OSError as exc:
        return [], {}, examined, "the transcript could not be read (%s)" % exc
    if not in_turn:
        # The requested boundary was never found: that is missing evidence,
        # not an empty turn (hunt part 3, finding 26).
        return [], {}, examined, (
            "this turn's prompt (%s) was not found in the transcript" % prompt_id)
    return calls, results, examined, None


# --------------------------------------------------------------------------
# status derivation — structural only
# --------------------------------------------------------------------------

def shorten(text, limit=MAX_DETAIL):
    """Collapse to one line and cap it, SAYING how much was removed."""
    flat = " ".join(str(text).split())
    if len(flat) <= limit:
        return flat
    return "%s… (+%d chars)" % (flat[:limit], len(flat) - limit)


def status_of(result, tool_name=None):
    """(status_word, detail) for one call. See the module docstring's table.

    status_word is one of a closed set: "", "ok", "RETURNED", "ERROR", "NO RESULT";
    and nothing here reads the meaning of any text.
    """
    if result is None:
        return ("NO RESULT", "no tool_result for this call in this turn")

    text = flatten(result.get("content"))

    if result.get("is_error"):
        first = next((ln for ln in text.splitlines() if ln.strip()), "")
        return ("ERROR", shorten(first) if first else "(no message)")

    # The tool's OWN sentence about what it did, when it declares one. This is
    # the rule that renders SendMessage's "queued" instead of its success flag.
    stripped = text.strip()
    if stripped.startswith("{"):
        try:
            obj = json.loads(stripped)
        except Exception:
            obj = None
        if isinstance(obj, dict) and isinstance(obj.get("message"), str):
            return ("", shorten(obj["message"]))

    lines = text.count("\n") + 1 if text else 0
    word = "RETURNED" if tool_name == "Bash" else "ok"
    detail = "%d line(s), %d bytes" % (lines, len(text))
    if tool_name == "Bash":
        detail += "; shell returned, individual commands/checks not verified"
    return (word, detail)


def tally(names):
    """'Bash×30, Read×6, SendMessage×2' — insertion-ordered, never sorted by
    count, so the reader can match it against the rows below."""
    counts = {}
    for n in names:
        counts[n] = counts.get(n, 0) + 1
    return ", ".join("%s×%d" % (n, c) for n, c in counts.items())


# --------------------------------------------------------------------------
# rendering
# --------------------------------------------------------------------------

def render(calls, results):
    names = [n for _, n in calls]
    total = len(calls)

    if total == 0:
        # EXPLICITLY empty, never blank. The "dispatching it rather than
        # queuing it" failure had zero tool calls behind it; a manifest that
        # rendered nothing at all would have hidden precisely that turn.
        return (
            "TURN MANIFEST — 0 tool calls this turn. Nothing was executed.\n"
            "Anything stated above rests on no tool result from this turn."
        )

    out = [
        "TURN MANIFEST — %d tool call(s) this turn: %s" % (total, tally(names)),
        "(each status is read from that call's own result, not written by the assistant)",
    ]

    shown = calls[:MAX_ROWS]
    width = max(len(n) for n in names[:MAX_ROWS])
    width = min(width, 16)
    for i, (tid, name) in enumerate(shown, 1):
        word, detail = status_of(results.get(tid), name)
        label = "%s — %s" % (word, detail) if word else detail
        out.append("%3d %-*s %s" % (i, width, name[:width], label))

    dropped = calls[MAX_ROWS:]
    if dropped:
        out.append(
            "    … %d further call(s) not listed above: %s "
            "(all %d are counted in the header)"
            % (len(dropped), tally([n for _, n in dropped]), total)
        )
    return "\n".join(out)


def unavailable(reason):
    """A manifest that cannot be built says so, in the same place it would
    have printed. Silence here is indistinguishable from a turn with no tool
    calls, and those two facts must never look alike."""
    return (
        "TURN MANIFEST — UNAVAILABLE: %s.\n"
        "No calls are listed. This is a GAP IN THE RECORD, not a turn that ran nothing. %s"
        % (reason, HOOK_TAG)
    )


# NOT SHOWN AT A TURN END (orchestration reliability R6, richos-hq docs/prds/2026-10-23-
# orchestration-reliability.md: "Normal successful turns emit no routine tool manifest";
# the CEO's 2026-10-23 screenshot was a screenful of it under every reply). The turn's
# calls and statuses go to the log below; `turn-manifest.py --show` renders this manifest
# for a Stop payload on request.


# --------------------------------------------------------------------------
# main
# --------------------------------------------------------------------------

def main():
    show = "--show" in sys.argv[1:]
    try:
        payload = json.load(sys.stdin)
    except Exception:
        # An unparseable payload never wedges turn-end, and never renders a
        # manifest it cannot stand behind.
        return 0

    if payload.get("stop_hook_active") and not show:
        # A re-fire after some OTHER Stop hook blocked. The manifest for this
        # turn has already been printed once; printing it again would suggest
        # a second turn happened.
        return 0

    prompt_id = payload.get("prompt_id")
    transcript = payload.get("transcript_path")

    if not prompt_id:
        message = unavailable(
            "the Stop payload carried no prompt_id, so this turn's calls "
            "cannot be told apart from the rest of the session's"
        )
        calls, results, examined = [], {}, 0
        rows = None
    else:
        calls, results, examined, err = read_turn(transcript, prompt_id)
        if err:
            message = unavailable(err)
            rows = None
        else:
            message = render(calls, results) if show else ""   # rendered only when asked (--show)
            rows = len(calls)

    if show:
        sys.stdout.write(message + "\n")
        return 0

    # --- observation record ------------------------------------------------
    # Written every turn, so the manifest is a record rather than a rumour,
    # and so records_examined is checkable after the fact.
    #
    # STATUS WORDS ONLY — never the detail text. The detail is reproduced from
    # tool output, which can carry anything; showing it in the operator's own
    # terminal is theirs to see, appending it to a file on disk is a leak
    # surface this hook has no reason to open.
    record = {
        "session": payload.get("session_id"),
        "prompt_id": prompt_id,
        "records_examined": examined,
        "calls": rows,
        "tools": [n for _, n in calls],
        "statuses": [status_of(results.get(t), n)[0] or "message" for t, n in calls],
        "rendered": "manifest" if rows is not None else "unavailable",
    }
    if rows is None:
        record["unavailable"] = message.split("\n", 1)[0]       # a gap in the record, never "ran nothing"
    try:
        root = os.environ.get("RICHOS_MANIFEST_ENTITY_ROOT") or payload.get("cwd") or os.getcwd()
        state = os.path.join(root, ".claude", "state")
        os.makedirs(state, exist_ok=True)
        with open(os.path.join(state, "turn-manifests.jsonl"), "a", encoding="utf-8") as f:
            f.write(json.dumps(record) + "\n")
    except Exception:
        pass  # the log is a convenience; losing it never changes what rendered

    return 0


if __name__ == "__main__":
    sys.exit(main())
