"""Why an app scope's grant is closed, in one sentence a refusal can carry.

The desktop app writes a `closed` record on the scope every time it closes a grant
(`richos-core` `ecs.rs`, `GrantClosed`: `{"by": <code>, "at_ms": <unix ms>}`) and clears it when
the grant opens. Every refusal of a closed grant names which work, who closed it and when from
that record (follow-up "app work tools refused dispatches for unexplained reasons",
2026-10-28: "This app work has ended or was stopped" was read as a Stop nobody had made).

Keep CLOSED_BY in step with `ecs.rs`'s `closed_by` codes and `quota/gate.rs`'s
`closed_sentence`.
"""
import time

CLOSED_BY = {
    "stop": "a Stop (the user's, or the app ending this connection)",
    "assignment-end": "the end of the assignment this work carried",
    "crash-recovery": "the app's start-up sweep after it last quit unexpectedly",
    "turn-end": "the end of the turn",
    "turn-start-failed": "a turn that could not start",
    "not-started": "the app, which has written it for an input whose turn has not started yet",
}


def utc(ms):
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(ms / 1000))


def work_of(scope):
    """Which work a scope is: its thread, turn and seat, as far as it names them."""
    binding = scope.get("binding") if isinstance(scope, dict) else None
    binding = binding if isinstance(binding, dict) else {}
    thread, turn = binding.get("thread_id"), binding.get("turn_id")
    work = "the work of thread %s, turn %s" % (thread, turn) if thread and turn else "this work"
    if isinstance(scope, dict) and isinstance(scope.get("seat"), str):
        work += " (seat %s)" % scope["seat"]
    return work


def describe(scope):
    """'Cause: <which work> was closed by <whom> at <when>.' for a scope whose grant is closed."""
    work = work_of(scope)
    closed = scope.get("closed") if isinstance(scope, dict) else None
    if not isinstance(closed, dict) or not isinstance(closed.get("by"), str):
        return ("Cause: %s has no open grant, and no reason for closing it was recorded (a scope an "
                "older app wrote, or one written by hand)." % work)
    by = CLOSED_BY.get(closed["by"], "an unrecognized cause (%s)" % closed["by"])
    at = closed.get("at_ms")
    when = "at " + utc(at) if isinstance(at, int) and not isinstance(at, bool) else "at an unrecorded time"
    return "Cause: %s was closed by %s %s." % (work, by, when)
