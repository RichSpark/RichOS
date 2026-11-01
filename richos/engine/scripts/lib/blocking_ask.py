#!/usr/bin/env python3
"""blocking_ask.py: THE LEAD MUST NOT GO DEAF WHILE IT WAITS FOR THE CEO.

Item B of richos-hq docs/operations/2026-10-01-escalation-wakes-the-lead.md,
built past Sage's review of it (richos-hq 8ba32b71, items 8 and 9).

===========================================================================
THE FAILURE
===========================================================================
2026-10-01, 13:02:02Z: the lead put a question to the CEO with AskUserQuestion.
The tool holds the lead's turn open until he answers, and every task
notification and monitor event waits in the queue behind it: the R1 merge
result (13:02:48Z) and isaac-opus-d3c's final report (13:09:19Z) reached the
lead only at 13:39:01Z, when he came back. For 37 minutes nothing could wake
it: not a teammate finishing, not the stall watcher, not the 93% quota rule.

===========================================================================
TWO HALVES, ONE FILE
===========================================================================
  check       the PreToolUse half (guard-ceo-ruled-ask.sh, the deaf-lead
              check). While a teammate of this session is live, an
              AskUserQuestion is refused and the lead is told to end its turn
              with the question as its FINAL MESSAGE, under the fixed lead-in
              `QUESTION FOR YOU:`. He reads the same words in the terminal, his
              answer arrives as his next prompt, and meanwhile the lead can be
              woken. It is ALLOWED, with nothing to declare:
                - with no live teammate of this session (nothing to be deaf to);
                - when this turn was already refused at Stop. A blocking Stop
                  gate (guard-resource-waits.sh, which "CLEARS ONLY WHEN THE
                  WAIT ENDS") would otherwise keep pushing the turn on, the
                  question would stop being the final message, and the lead
                  could neither block on it nor leave it on screen (Sage 8);
                - when liveness cannot be read (fail open, as the rest of this
                  guard does: a gate that can wedge every question to the CEO
                  is worse than the failure it prevents);
                - after a declaration for this session, with a reason:
                  blocking-ask-exempt.sh (the ceo-ruled-exempt.sh shape: the
                  only text of an AskUserQuestion is what the CEO reads, so a
                  marker line inside it would be put in front of him).
              Teammates the registry does not know (55 of 166 escalations
              since 2026-09-24 had no registry match) are invisible here, so
              the ask is allowed for them: the safe direction, stated so
              nobody reads this as complete coverage.

  unanswered  the Stop half (notice-unanswered-question.sh). The final-message
              route has one cost AskUserQuestion did not: the question
              scrolls away under every later turn the lead is woken for, and
              he comes back to the newest notification, not to the question
              (Sage 9). So while a question put under `QUESTION FOR YOU:` in an
              earlier turn has no later human prompt, it is repeated in the
              Stop systemMessage at every turn end. That channel is shown to
              the PERSON and never to the model (measured, escalations.py), so
              it re-surfaces the question without waking anything.

The lead-in is matched mechanically, never by a prose classifier.
"""

import argparse
import importlib.util
import json
import os
import re
import sys
import time

LEAD_IN = "QUESTION FOR YOU:"
EXEMPT_LOG = "blocking-ask-exempts.log"
MIN_REASON = 20
RESURFACE_CHARS = 800
STOP_FEEDBACK = "Stop hook feedback:"
# No `>` in the prefix: a blockquoted lead-in is a quoted example, not a
# question put to him (hunt part 3, finding 35). Fenced blocks are removed
# before the search for the same reason.
_LEAD_IN_RX = re.compile(r"^[ \t*_#-]*" + re.escape(LEAD_IN), re.M)
# A fence closes on a line of the SAME character at least as long as the
# opener: ``` is closed by ``` or ````, never by ~~~ or ``. An exact-length
# closer only (\1 alone) let a valid longer closer fall through to the
# unclosed arm, which then swallowed the real question after it (hunt part 3
# v3, finding 40). The opener is its WHOLE run ((?!`) / (?!~)), so a
# ```` opener cannot backtrack to ``` and be closed by a shorter line.
_FENCE_RX = re.compile(r"^[ \t]*(`{3,}(?!`)|~{3,}(?!~)).*?^[ \t]*\1(?:(?<=`)`*|(?<=~)~*)[ \t]*$"
                       r"|^[ \t]*(`{3,}|~{3,}).*\Z",
                       re.M | re.S)


def _rows(transcript):
    """Every JSON object row of the transcript, in order. Unreadable rows are
    skipped; an unreadable FILE raises OSError to the caller."""
    with open(transcript, "rb") as fh:
        for raw in fh:
            if not raw.endswith(b"\n"):
                break                         # a row the host is still writing
            try:
                d = json.loads(raw.decode("utf-8", "replace"))
            except ValueError:
                continue
            if isinstance(d, dict):
                yield d


def _is_turn_start(d):
    """A `user` row the host marks as starting a turn: the CEO's prompt
    (turnOrigin human) or a notification that woke the lead
    (task_notification). A Stop hook's feedback row and a tool result carry no
    turnOrigin."""
    return d.get("type") == "user" and bool(d.get("turnOrigin"))


def _is_stop_refusal(d):
    """A Stop hook refused this turn's end: the exit-2 shape (a `user` row whose
    text starts "Stop hook feedback:") or the JSON-decision shape (a
    hook_blocking_error attachment of the Stop event). Both are host rows,
    seen in 212ab083 and 02c7d12c."""
    if d.get("type") == "user":
        msg = d.get("message")
        content = msg.get("content") if isinstance(msg, dict) else None
        return isinstance(content, str) and content.startswith(STOP_FEEDBACK)
    att = d.get("attachment")
    return (d.get("type") == "attachment" and isinstance(att, dict)
            and att.get("type") == "hook_blocking_error" and att.get("hookEvent") == "Stop")


def stop_refused_this_turn(transcript):
    """True when a Stop refusal row follows the last turn-start row. None when
    the transcript cannot be read (the caller fails open)."""
    if not transcript:
        return False
    refused = False
    try:
        for d in _rows(transcript):
            if _is_turn_start(d):
                refused = False
            elif _is_stop_refusal(d):
                refused = True
    except FileNotFoundError:
        return False
    except OSError:
        return None
    return refused


def _assistant_text(d):
    if d.get("type") != "assistant":
        return ""
    msg = d.get("message")
    content = msg.get("content") if isinstance(msg, dict) else None
    if isinstance(content, str):
        return content
    if not isinstance(content, list):
        return ""
    return "\n".join(str(b.get("text") or "") for b in content
                     if isinstance(b, dict) and b.get("type") == "text")


def _user_text(d):
    msg = d.get("message")
    content = msg.get("content") if isinstance(msg, dict) else None
    if isinstance(content, str):
        return [content]
    if not isinstance(content, list):
        return []
    return [b.get("text") for b in content if isinstance(b, dict) and b.get("type") == "text"]


def _his_turn(d, app_words):
    """The CEO's own prompt starts this turn. In the terminal Claude Code marks
    it turnOrigin human. The app sends every input over the SDK (turnOrigin
    sdk): his words, and also the messages his teammates send the lead, so
    there his turn is an sdk input whose text is one of his original words in
    the app's conversation ledger (pierce.py desktop_turns), exactly."""
    if d.get("turnOrigin") == "human":
        return True
    return (app_words is not None and d.get("turnOrigin") == "sdk"
            and any(text in app_words for text in _user_text(d)))


def app_user_words(payload):
    """His original words in this app conversation, or None outside the app
    (no RICHOS_APP_SCOPE). Raises when the app's ledger cannot be read."""
    if not os.environ.get("RICHOS_APP_SCOPE"):
        return None
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "pierce.py")
    spec = importlib.util.spec_from_file_location("blocking_ask_pierce", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return set(mod.desktop_turns(payload)[2].values())


def unanswered_question(transcript, app_words=None):
    """(question text, asked-at timestamp) of a `QUESTION FOR YOU:` the lead
    put in an EARLIER turn that no human prompt has answered yet, or None.

    The question's own turn is skipped: at its Stop the question is the last
    thing on screen already. Any later turn start that is the CEO's own prompt
    (_his_turn) answers it; a notification turn does not."""
    pending = None
    later_turn = False
    try:
        for d in _rows(transcript):
            if _is_turn_start(d):
                if _his_turn(d, app_words):
                    pending = None
                elif pending is not None:
                    later_turn = True
                continue
            text = _assistant_text(d)
            if not text:
                continue
            text = _FENCE_RX.sub("", text)
            m = _LEAD_IN_RX.search(text)
            if m:
                pending = (text[m.start():].strip(), str(d.get("timestamp") or ""))
                later_turn = False
    except OSError:
        return None
    return pending if (pending and later_turn) else None


def _workspaces(engine_root):
    path = os.path.join(engine_root, "mega-lander", "workspaces.py")
    if not os.path.isfile(path):
        return None
    spec = importlib.util.spec_from_file_location("blocking_ask_workspaces", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def live_teammates(engine_root, session_id):
    """Names of this session's teammates whose run has not ended and who are
    not paused, from the workspace registry. None when it cannot be read."""
    if not session_id:
        return []
    try:
        ws = _workspaces(engine_root)
        if ws is None:
            return None
        cache = {}
        names = []
        for rec in ws.all_agents():
            if rec.get("session_id") != session_id or rec.get("disposition"):
                continue
            fin, paused, _why = ws.finished_state(rec, cache)
            if not fin and not paused:
                names.append(str(rec.get("name") or rec.get("key") or "?"))
        return sorted(names)
    except Exception:  # noqa: BLE001: unknown liveness fails open, and says why
        return None


def declared(entity_root, session_id):
    """The reason of this session's declaration, or ""."""
    path = os.path.join(entity_root or "", ".claude", "state", EXEMPT_LOG)
    found = ""
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            for line in fh:
                parts = line.rstrip("\n").split("\t")
                if len(parts) >= 3 and parts[1] == "session=%s" % session_id:
                    found = parts[2]
    except OSError:
        return ""
    return found


def check(payload, entity_root, engine_root):
    """('ALLOW' or 'REFUSE', detail) for one AskUserQuestion payload."""
    if not isinstance(payload, dict):
        return "ALLOW", "the payload could not be read"
    if payload.get("tool_name") not in (None, "", "AskUserQuestion"):
        return "ALLOW", "not an AskUserQuestion call"
    if payload.get("agent_id"):
        return "ALLOW", "a teammate's own question"
    sid = str(payload.get("session_id") or "")
    live = live_teammates(engine_root, sid)
    if live is None:
        return "ALLOW", "teammate liveness could not be read (fail open)"
    if not live:
        return "ALLOW", "no live teammate of this session"
    reason = declared(entity_root, sid)
    if reason:
        return "ALLOW", "declared for this session: %s" % reason
    refused = stop_refused_this_turn(str(payload.get("transcript_path") or ""))
    if refused is None:
        return "ALLOW", "the transcript could not be read (fail open)"
    if refused:
        return "ALLOW", "this turn was already refused at Stop, so the question could not stay its final message"
    return "REFUSE", ", ".join(live)


def main(argv):
    ap = argparse.ArgumentParser(prog="blocking_ask.py")
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("check", help="the PreToolUse half: payload on stdin")
    c.add_argument("--entity", default="")
    c.add_argument("--engine-root", default="")
    u = sub.add_parser("unanswered", help="the Stop half: payload on stdin")
    u.add_argument("--max", type=int, default=RESURFACE_CHARS)
    a = ap.parse_args(argv)
    raw = sys.stdin.read()
    try:
        payload = json.loads(raw) if raw.strip() else {}
    except ValueError:
        payload = None
    if a.cmd == "check":
        verdict, detail = check(payload, a.entity, a.engine_root)
        print("%s\t%s\t%s" % (verdict, detail.replace("\t", " ").replace("\n", " "),
                              str((payload or {}).get("session_id") or "")))
        return 0
    if not isinstance(payload, dict) or payload.get("stop_hook_active") or payload.get("agent_id"):
        return 0
    tpath = str(payload.get("transcript_path") or "")
    if tpath and not os.access(tpath, os.R_OK):
        # An unreadable transcript is NOT "no unanswered question" (hunt part
        # 3, finding 28): say that the watch did not run.
        print(json.dumps({"suppressOutput": True, "systemMessage":
                          "UNANSWERED-QUESTION WATCH DID NOT RUN: the transcript at %s "
                          "could not be read, so an unanswered question is not repeated "
                          "this turn." % tpath}))
        return 0
    found = unanswered_question(tpath)
    if found:
        # In the app, his answer is told from a teammate's message by his words in the
        # app's ledger, read only when a question is still pending without them.
        try:
            words = app_user_words(payload)
        except (OSError, ValueError, KeyError) as error:
            # Without his words his answer would look like no answer: say the watch did
            # not run rather than repeat a question he answered.
            print(json.dumps({"suppressOutput": True, "systemMessage":
                              "UNANSWERED-QUESTION WATCH DID NOT RUN: the app's conversation "
                              "record could not be read (%s), so an unanswered question is not "
                              "repeated this turn." % error}))
            return 0
        if words is not None:
            found = unanswered_question(tpath, words)
    if not found:
        return 0
    text, at = found
    if len(text) > a.max:
        text = text[:a.max - 3] + "..."
    when = at[11:16] + "Z" if len(at) >= 16 else "an earlier turn"
    print(json.dumps({"suppressOutput": True, "systemMessage":
                      "STILL WAITING FOR YOUR ANSWER (asked %s; your next message answers it): %s"
                      % (when, " ".join(text.split()))}))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
