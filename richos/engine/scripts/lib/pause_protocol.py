#!/usr/bin/env python3
"""The orchestrator's pause message: render once, validate before delivery.

No natural-language rewrite is permitted inside a pause control message. This
validates the whole message, including its summary, rather than banning words
such as 'stop' which also occur in 'do not stop'. The only variable fields are a
closed reason enum and a UTC reset time.

THE HOLD NEVER DEPENDS ON THE MESSAGE. The command line (`--to <agent>`) first
freezes that agent through the workspace registry (workspaces.pause, which runs
agent_hold.py) and confirms its hold is in place, and only then prints the
message; `--resume --to <agent>` first releases the hold (workspaces.resume).
The desktop's richos_work.pause_message and resume_message tools take the same
step through the same function (issue) before they return their payload.
The message's format belongs to Claude Code: on 2026-10-11 it added a field to
SendMessage, the delivery check refused every generated pause, and nothing was
frozen for 47 minutes, because the freeze ran only when the message got through.
A refused or lost message now never prevents or undoes a hold; the message only
tells the agent to wait, or to carry on. Rendering and validation (render,
message_input, validate_payload) control nothing.
"""
import argparse
import json
import os
import re
import shlex
import sys

# WHAT THE AGENT HEARS IS WAIT, AND NOTHING ADDED TO IT. The CEO, 2026-09-28: "aha,
# so the keyword is WAIT, not pause." and, of a rewording that told the agent never
# to end its turn: "why do you always have to include harmful garbage?" "Pause" is
# what the lead and the Mac see (the agent's running work frozen by agent_hold.py);
# the agent is told to wait until it is resumed and then carry on. The wait is a
# command, because an agent waits by running something: a background subagent that
# finishes its reply ends its run (Rich, brief addition 1). No prohibitions: they
# break the agent's normal finishing and a real stop, and naming an act invites it.
WAIT_COMMAND = "python3 " + shlex.quote(os.path.join(os.path.dirname(os.path.realpath(__file__)), "agent_hold.py")) + " wait"
BODY = "\n".join([
    "WAIT until the orchestrator resumes you, then carry on from where you were.",
    "To wait, run this command with the Bash timeout 600000, and run it again each time it prints STILL WAITING: "
    + WAIT_COMMAND,
])
SUMMARY = "WAIT until resumed"
MARKER = "richos-pause-control: "
# The RESUME that ends a pause. It carries its own control line and is accepted
# only unchanged. It never carries `pause-until:`, which is what makes the
# registry record a resume (and agent_hold.py continue the held work).
RESUME_BODY = "\n".join([
    "RESUME: carry on from where you were.",
    "The commands that were running are running again, and their elapsed times include the wait.",
])
RESUME_SUMMARY = "RESUME: carry on from where you were"
RESUME_MARKER = "richos-resume-control: "
RECIPIENT = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}\Z")
CLOCK = re.compile(r"(?:[01][0-9]|2[0-3]):[0-5][0-9]Z\Z")
# A routing check for declared control messages, not a classifier of user intent.
# The quota watcher always renders a declared control. Rich's manual command does
# the same. Legacy pause-until messages and imperative pause requests are refused
# unless they equal the template, so the incident's handwritten message fails.
# Reserve pause wording and imperative holds in outbound worker messages. This
# deliberately also refuses prose about pausing rather than guessing its intent.
INTENT = re.compile(r"\b(?:pause|pausing|paused)\b|\bhold(?:\s+(?:now|your|all|the|this|until|without|on|position|fire)\b|\s*[:.!?]|\s*$)", re.I)


def until_text(reason, reset=None):
    """What ends the pause: the message's `pause-until:` line, and the registry's record."""
    return render(reason, reset).rsplit("\npause-until: ", 1)[1]


def render(reason, reset=None):
    if reason not in ("manual", "quota", "weekly-quota"):
        raise ValueError("reason must be manual, quota or weekly-quota")
    if reason == "quota":
        if not isinstance(reset, str) or not CLOCK.fullmatch(reset):
            raise ValueError("quota pause requires its reset time as HH:MMZ")
        until = "the five-hour quota reset at " + reset
    elif reason == "weekly-quota":
        import datetime
        if reset is None:
            until = "the weekly quota reset confirmed by a fresh reading"
        elif not isinstance(reset, str) or not re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", reset):
            raise ValueError("weekly pause requires its full UTC reset time")
        else:
            datetime.datetime.fromisoformat(reset.replace("Z", "+00:00"))
            until = "the weekly quota reset at " + reset
    else:
        if reset is not None:
            raise ValueError("manual holds have no automatic reset")
        # Rich made this pause, so it is Rich's resume that ends it (no silent holds,
        # 2026-10-22): it is a hold on his work, reported until it runs or he decides.
        until = "Rich's resume"
    metadata = json.dumps({"version": 1, "reason": reason, "reset": reset}, separators=(",", ":"))
    return BODY + "\n" + MARKER + metadata + "\npause-until: " + until


def end_of(reason, reset=None):
    """The pause's checkable end for the registry (workspaces.py HOLDS): a quota
    reset is at:<time>; a manual pause, or a weekly one with no reset time, has none."""
    if reason in ("quota", "weekly-quota") and reset:
        return "at:" + reset
    return ""


def render_resume(reason="manual"):
    if reason not in ("manual", "quota", "weekly-quota"):
        raise ValueError("reason must be manual, quota or weekly-quota")
    metadata = json.dumps({"version": 1, "reason": reason}, separators=(",", ":"))
    return RESUME_BODY + "\n" + RESUME_MARKER + metadata


def message_input(to, reason="manual", reset=None, resume=False):
    if not isinstance(to, str) or not RECIPIENT.fullmatch(to):
        raise ValueError("the exact recipient is required")
    if resume:
        if reset is not None:
            raise ValueError("a resume has no reset time")
        return {"to": to, "summary": RESUME_SUMMARY, "message": render_resume(reason)}
    return {"to": to, "summary": SUMMARY, "message": render(reason, reset)}


def validate_resume_text(text):
    if not isinstance(text, str) or len(text) > 8192:
        raise ValueError("invalid resume control message")
    rows = text.splitlines()
    metadata = [row[len(RESUME_MARKER):] for row in rows if row.startswith(RESUME_MARKER)]
    if len(metadata) != 1 or any(row.startswith(MARKER) for row in rows):
        raise ValueError("resume messages must come from pause_protocol.py --resume, unchanged")
    try:
        value = json.loads(metadata[0])
        if set(value) != {"version", "reason"} or value["version"] != 1:
            raise ValueError("invalid resume control fields")
        expected = render_resume(value["reason"])
    except (TypeError, KeyError, json.JSONDecodeError) as error:
        raise ValueError("invalid resume control fields") from error
    if text not in (expected, expected + "\n"):
        raise ValueError("resume instructions were changed; send the generated message unchanged")
    return value


def is_resume(text):
    return isinstance(text, str) and any(row.startswith(RESUME_MARKER) for row in text.splitlines())


def declared(text):
    return isinstance(text, str) and (bool(INTENT.search(text)) or
        bool(re.search(r"(?im)^\s*(?:pause-until|richos-pause-control|richos-resume-control)\s*:", text)))


def validate_text(text):
    if not isinstance(text, str) or len(text) > 8192:
        raise ValueError("invalid pause control message")
    rows = text.splitlines()
    metadata = [row[len(MARKER):] for row in rows if row.startswith(MARKER)]
    if len(metadata) != 1:
        raise ValueError("pause messages must come from pause_protocol.py, unchanged")
    try:
        value = json.loads(metadata[0])
        if set(value) != {"version", "reason", "reset"} or value["version"] != 1:
            raise ValueError("invalid pause control fields")
        expected = render(value["reason"], value["reset"])
    except (TypeError, KeyError, json.JSONDecodeError) as error:
        raise ValueError("invalid pause control fields") from error
    # Permit the single trailing newline a text file conventionally carries.
    if text not in (expected, expected + "\n"):
        raise ValueError("pause instructions were changed; send the generated message unchanged")
    return value


def is_generated_pause(text):
    """True for the unchanged generated pause: the only message that freezes an agent's work
    and that nothing but the generated RESUME ends (workspaces.py, Sage's catch 3)."""
    try:
        validate_text(text)
        return True
    except (ValueError, TypeError):
        return False


def is_generated_resume(text):
    """True for the unchanged generated RESUME, the only message that ends a generated pause."""
    try:
        validate_resume_text(text)
        return True
    except (ValueError, TypeError):
        return False


# Claude Code backfills `content` into the SendMessage input its hooks see, as a
# PREVIEW of `message`: the whole message when it fits in 50 display columns,
# otherwise a prefix and an ellipsis (captured on 2.1.283, 2026-09-27). Never a
# copy of a long message, so demanding equality refused every generated pause.
# The captured multiline RESUME has 50 display characters plus a newline.
# Newlines consume no preview columns; exact-prefix validation below still
# prevents an alias from adding or changing instructions.
PREVIEW_COLUMNS = 50
ELLIPSIS = "…"


def is_preview(alias, text):
    """True when alias is text itself, or the harness's truncated preview of it.

    A preview adds no word of its own: everything before its final ellipsis is
    the start of the already-validated message, and it is no longer than the
    harness ever makes one. Anything else is a second instruction.
    """
    if alias == text:
        return True
    if not isinstance(alias, str) or not alias.endswith(ELLIPSIS) or len(alias.replace("\n", "")) > PREVIEW_COLUMNS:
        return False
    head = alias[:-len(ELLIPSIS)]
    return 0 < len(head) < len(text) and text.startswith(head)


# Every spelling of a SendMessage's kind: a control message is only ever a plain
# message, never a shutdown request, whichever name the harness uses for the field.
KIND_FIELDS = ("type", "messageType", "message_type")


def validate_payload(payload):
    if payload.get("tool_name") != "SendMessage":
        return
    ti = payload.get("tool_input")
    if not isinstance(ti, dict):
        raise ValueError("invalid SendMessage input")
    if "to" in ti and "recipient" in ti and ti["to"] != ti["recipient"]:
        raise ValueError("conflicting SendMessage recipients")
    # Messages reporting to the orchestrator are not instructions from it.
    target = ti.get("to", ti.get("recipient", ""))
    if target in ("main", "team-lead", "lead", "leader", "user") and all(
            ti.get(key, "message") == "message" for key in KIND_FIELDS):
        return
    text = ti.get("message", ti.get("content"))
    summary = ti.get("summary", "")
    if not any(declared(ti.get(key)) for key in ("message", "content", "summary")):
        return
    if not isinstance(target, str) or not RECIPIENT.fullmatch(target):
        raise ValueError("a pause requires the exact recipient")
    # Only what the agent reads is checked: who gets it (to, recipient, type) and
    # what it says (message, content, summary). Any other field the harness adds
    # (recipient_kind, 2026-10-11) changes neither and never refuses a pause or
    # resume; refusing unknown fields blocked every hold until a code change.
    if is_resume(text):
        validate_resume_text(text)
        if summary not in (None, "", RESUME_SUMMARY):
            raise ValueError("resume summary was changed; use the generated summary unchanged")
    else:
        validate_text(text)
        if summary not in (None, "", SUMMARY):
            raise ValueError("pause summary was changed; use the generated summary unchanged")
    # An alias must not carry a second, contradictory instruction. The harness's
    # own preview of the message is not one.
    if "content" in ti and not is_preview(ti["content"], text):
        raise ValueError("pause content alias must equal the generated message")
    if any(ti.get(key, "message") != "message" for key in KIND_FIELDS):
        raise ValueError("a pause is a message, never a shutdown request")


def _workspaces():
    """mega-lander/workspaces.py, the registry that records a pause and holds the agent's work."""
    mega = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "mega-lander")
    if mega not in sys.path:
        sys.path.insert(0, mega)
    import workspaces
    return workspaces


def take_effect(to, reason="manual", reset=None, resume=False):
    """The hold itself, before any message: (ok, line for the lead).

    A pause freezes the agent (workspaces.pause runs agent_hold.hold: its running
    commands suspended, its new ones refused with the WAIT) and is confirmed by the
    hold record agent_hold.py keeps for it. A resume releases it (workspaces.resume
    runs agent_hold.release) and is confirmed by that record being gone. Never
    raises: a hold that could not be confirmed is said, never assumed."""
    word = "RELEASED" if resume else "HELD"
    try:
        ws = _workspaces()
        me = ws.current_session()
        notes = []
        if resume:
            rec = ws.resume(to, me, notes)
        else:
            rec = ws.pause(to, until_text(reason, reset), me, notes, control=True, end=end_of(reason, reset))
        ah = ws._agent_hold()
        sid, aid = str(rec.get("session_id") or ""), str(rec.get("agent_id") or "")
        mine = [r for r in ah.records() if r.get("session_id") == sid and r.get("agent_id") == aid]
        detail = "; ".join(notes) or ("nothing was held" if resume else "no hold line was reported")
        if resume:
            ok = not mine and not any(": FAILED" in n for n in notes)
        else:
            ok = bool(mine) and not mine[0].get("not_stopped")
        return ok, "%s %s: %s" % (word if ok else "NOT " + word, to, detail)
    except Exception as error:     # the registry's refusal (SpecError) or anything unforeseen
        return False, "NOT %s %s: %s" % (word, to, str(error)[:300] or type(error).__name__)


def issue(to, reason="manual", reset=None, resume=False):
    """THE one way a pause or a resume is made, by the terminal command and the desktop's
    richos_work.pause_message / resume_message alike: (ok, line for the lead, payload).
    Invalid input is refused (ValueError/TypeError) before anything is held; then the
    hold or its release FIRST (take_effect), and only then the message to send. So no
    refusal or loss of the message, on any surface, can prevent or undo the hold."""
    payload = message_input(to, reason, reset, resume=resume)
    ok, line = take_effect(to, reason, reset, resume=resume)
    return ok, line, payload


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="validate a hook payload on stdin")
    parser.add_argument("--to")
    parser.add_argument("--reason", choices=("manual", "quota", "weekly-quota"), default="manual")
    parser.add_argument("--reset")
    parser.add_argument("--resume", action="store_true", help="render the RESUME that ends a pause")
    args = parser.parse_args()
    try:
        if args.check:
            raw = sys.stdin.read(262145)
            if len(raw) > 262144:
                raise ValueError("oversized hook payload")
            value = json.loads(raw)
            if not isinstance(value, dict):
                raise ValueError("invalid hook payload")
            validate_payload(value)
        else:
            # The hold FIRST, so no refusal or loss of the message can prevent or undo it.
            ok, line, payload = issue(args.to, args.reason, args.reset, resume=args.resume)
            print(line, file=sys.stderr)
            if not ok:
                print("The message below is still printed and still tells the agent to %s; check "
                      "agent_hold.py status before reporting it %s." % (
                          "carry on" if args.resume else "wait", "resumed" if args.resume else "paused"),
                      file=sys.stderr)
            print(json.dumps(payload))
            return 0 if ok else 3
    except (ValueError, TypeError) as error:
        print("REFUSED: " + str(error) + ". Generate the pause with scripts/lib/pause_protocol.py --to <agent>, "
              "and the resume with --resume --to <agent>.", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
