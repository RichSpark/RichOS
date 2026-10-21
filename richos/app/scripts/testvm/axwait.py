"""axwait.py — the ONE rule for a walk that waits on the guest's screen: which failed `ax.sh`
call means "not there yet, ask again" and which one ends the step.

Every walk's waiting find ("is it on screen yet?") decides with `not_yet()`, and every walk that
asks a slow read again decides with `ask_again()`. Neither adds a deadline or a loop: the wait
that calls it keeps its own limit, and that limit still decides when the step fails.

THE THREE ANSWERS A FAILED CALL CAN GIVE
  absent     the read finished and nothing matched (ax.py: `notfound`, "nothing matched").
             For any mode: a click that matched nothing pressed nothing.
  no answer  the read did not finish, so nothing is known either way: the app did not answer
             one attribute (ax.py respond(): `unresponsive`, kAXErrorCannotComplete) or the
             call hit an accessibility deadline (ax-deadline.py timeout_record():
             `<phase>_deadline`, ax.sh exit 124), or the deadline helper itself failed on
             an overdue read (a traceback naming ax-deadline.py, cf8c93712).
  anything else is a real failure and is raised.

WHY THE MODE DECIDES, NEVER THE RECORD'S `status`
A find or a tree read changes nothing in the guest, so asking it again asks the same question.
A click, press, focus or type may already have acted, so it is never asked again here.
ax-deadline.py's own `guest_deadline` record says `status: effect_unknown` for EVERY mode
(ax-deadline.py timeout_record), so the status cannot tell a find from a click; the mode can.

esc-20261021T164604Z-4c30b43e: after ax.py began answering `unresponsive` (ced86ec1d), the
reap walk ended at first-run on one such read of "Add this company", 13 s after the app was
ready, host CPU 100%: adopt-walk.py's present() counted only `notfound` as "not yet".
"""

READS = ('find', 'tree')
ABSENT = ('notfound', 'nothing matched')
NO_ANSWER = ('unresponsive', 'guest_deadline', 'transport_deadline', 'preflight_deadline',
             'render_deadline', 'command failed (124)', 'ax-deadline.py')


def absent(exc):
    """The call finished and nothing matched."""
    text = str(exc)
    return any(token in text for token in ABSENT)


def no_answer(exc):
    """The call did not finish: nothing is known about the screen, and for a click, nothing is
    known about whether it acted."""
    text = str(exc)
    return any(token in text for token in NO_ANSWER)


def ask_again(exc, mode):
    """A read (find or tree) that did not finish: asking it again is the same question."""
    return mode in READS and no_answer(exc)


def not_yet(exc, mode='find'):
    """A waiting find's answer: True when the failed `ax.sh <mode>` call means "not on screen
    yet" (absent, or a read with no answer), so the caller's own wait asks again; False when it
    is a real failure the caller must raise."""
    return absent(exc) or ask_again(exc, mode)
