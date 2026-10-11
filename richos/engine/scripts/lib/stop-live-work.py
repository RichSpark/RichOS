#!/usr/bin/env python3
"""stop-live-work.py — the predicate behind the PreToolUse[TaskStop] gate.

===========================================================================
THE FAILURE THIS GUARD EXISTS FOR
===========================================================================
2026-09-10. The CEO wrote:

    "the currently running agents MIGHT need to be paused IF we get close to
     hitting the quota before it resets."

Within a minute the orchestrator called TaskStop on two mid-flight teammates
and reported it back as compliance. No burn rate had been measured. No
condition had been established. Nothing had been asked.

The cost, verified afterwards: `echo-opus-hw2` had NO COMMITS and a clean
worktree — its entire session is gone. `zach-opus-dor1` survived only because
it had committed as it went.

His answer was "Did I say anything about stopping???".

===========================================================================
THE SECOND FAILURE, AND WHY THIS FILE NO LONGER READS HIS SENTENCES
===========================================================================
2026-09-20. The original fix carried a second clause — "authority B" — which
read the last genuine user message, looked for an unconditional stop
imperative, and ALLOWED the kill when it found one. It was written to honor
his orders without paperwork, and it was a regex over his prose.

Measured that morning (escalation esc-20260920T050611Z-fb0a4718, richos-hq
docs/verification/escalations/), every one of these returned AUTHORIZES —
each of them would have let a provably live teammate be destroyed with no
ack at all:

    "how is the stop command coming along"
    "tell me about stop.sh"
    "when will the stop command be ready"
    "the stop rule is in CLAUDE.md now"

His reply: "When the fuck did I say 'blindly automate everything even remotely
related to a stop'??????" He never did. Ruling: richos-hq
wiki/ceo-decisions.md §67.

The mechanism is worth naming, because the clause was built in response to a
kill made on an INFERENCE from his words, and it re-encoded that same
inference as a pattern. A predicate over prose cannot tell an order from a
conversation about orders, and on this tool being wrong destroys work that
cannot be recovered.

So: THIS FILE NEVER READS THE CEO'S MESSAGE TEXT. There is no stop-word
regex, no hedge list, no sentence splitter, and no transcript read of any
kind here. His stop still executes in seconds — `scripts/stop.sh <names>
--ceo-word "<his sentence>"` writes the ack per named target and prints the
exact TaskStop calls — but it executes through Rich's explicit act, quoting
him, rather than through a machine's reading of him.

===========================================================================
THE ONE THING THIS DECIDES
===========================================================================
A TaskStop against a LIVE teammate requires a written, checkable reason.
That is all. Everything else about TaskStop is somebody else's problem, and
deliberately so — see THE NARROWNESS below.

===========================================================================
WHAT A PreToolUse HOOK CAN ACTUALLY SEE — MEASURED, NOT ASSUMED
===========================================================================
Measured 2026-09-10 with a live one-shot session and a probe hook
(scripts/hooks/fixtures/stop-live-work/pretooluse-payload-probe.json holds the
captured payload). Two findings decided where the ack lives:

  1. THE ASSISTANT'S OWN TEXT FOR THIS TURN IS **NOT** IN THE TRANSCRIPT YET.
     The assistant records carrying that turn's preface text (line 25) and the
     tool_use itself (line 26) were written AFTER the hook returned. A hook
     cannot read one word of the orchestrator's reasoning for the call it is
     about to make.

  2. TaskStop's ENTIRE tool_input is {"task_id": "<name-or-agent-id>"}.
     Verified across all 100 real TaskStop calls on this machine: 100/100 have
     exactly that one key. There is no reason field, no note field, nothing.

Together they kill the transport every other ack in this engine uses.
`model-ceiling-ack:` rides in the Agent prompt and `resume-ack:` rides in the
SendMessage body because those tools HAVE a text field. TaskStop has none, and
the turn's prose is invisible. So the ack here cannot be a marker line — it has
to be WRITTEN DOWN BEFORE THE CALL, by `scripts/stop-work-ack.sh`, into
.claude/state/stop-work-acks.jsonl. That is a harder ack to give than a marker
in a sentence, and on this particular tool that is the correct direction: the
thing being destroyed cannot be recovered.

(A third measurement — the user's message is readable at that moment — was the
foundation of the deleted authority B. It is still true and it is no longer
used by anything here. Being able to read his words was never the same as
being able to understand them.)

===========================================================================
THE TWO AUTHORITIES, IN ORDER
===========================================================================
  A. THE TARGET IS NOT PROVABLY ALIVE  -> ALLOW, SILENTLY.
     Retiring a finished teammate is the overwhelming majority of real
     TaskStop traffic (measured in scripts/hooks/stop-live-work.corpus.md) and
     it destroys nothing. The verdict comes from scripts/lib/agent-liveness.py
     — the same resolver the removal helper, guard-agent-state-claims.sh and
     scripts/stop.sh use, whose authoritative source is the worktree lock.
     ALIVE means the lock is held and the locking pid is running. NOT-ALIVE
     and INDETERMINATE both allow (see FAIL-OPEN).

  B. A LIVE, TARGETED, UNCONSUMED stop-work-ack  -> ALLOW + record.
     Written by scripts/stop-work-ack.sh — directly, or by scripts/stop.sh
     when the CEO has ordered a stop, which puts his sentence verbatim in the
     ack's `--why`. It names the exact task_id, says what is being destroyed
     and why it cannot wait. A BARE MARKER EXEMPTS NOTHING: both fields are
     required and both have a floor. One ack covers ONE target and is consumed
     on use — because the two kills of 2026-09-10 were 2.7 seconds apart, and
     a single ack that waved through a sweep would have let exactly that
     incident happen with one sentence of paperwork.

  Otherwise -> REFUSE, naming the target, what is known about it, and the two
  honest routes (his order via stop.sh, or your own reason via
  stop-work-ack.sh — both of which end in an ack on disk).

===========================================================================
THE NARROWNESS, AND WHY IT IS THE POINT
===========================================================================
This project killed three guards (g11/g12/g13) in one day by shipping gates
broad enough that waiving became routine — and a habitually waived guard is a
dead guard that still looks alive. So this one refuses in ONE situation:

  the orchestrator destroying work that is PROVABLY still running,
  with nothing written down.

It never fires on:
  * a teammate stopping ITSELF (the caller is the target)
  * a protocol shutdown (no TaskStop involved at all)
  * cleanup of an agent that has already finished  -> authority A
  * a target it cannot positively establish as alive -> authority A

===========================================================================
FAIL-OPEN ON DOUBT, AND WHY THAT IS NOT A HOLE
===========================================================================
The guard blocks ONLY on a POSITIVE ALIVE verdict. INDETERMINATE allows.

That looks backwards for a destructive operation, and it is not, for a reason
this repository has already paid for once: the dominant legitimate use of
TaskStop is retiring a teammate whose worktree is long gone, and for those the
resolver frequently cannot resolve anything at all. Fail closed and every one
of those cleanups needs paperwork; paperwork every time is how g11, g12 and
g13 died. Fail open and the guard still refuses the exact case that cost the
CEO a session's work, because a LIVE agent is precisely the one whose lock IS
held and IS resolvable.

An INDETERMINATE stop is not silent, though — it is allowed and NOTED, so the
one class the guard cannot decide is at least visible.
"""

import json
import os
import sys
import time

# ---------------------------------------------------------------------------
# 1. THE WRITTEN ACK — THE ONLY INPUT THIS FILE HAS BESIDES LIVENESS
# ---------------------------------------------------------------------------
# Because TaskStop has no text field and the turn's prose is invisible (see the
# module header), the ack is a FILE. scripts/stop-work-ack.sh writes it; this
# reads it. Three properties, each of which exists because of a specific way an
# ack can be made meaningless:
#
#   TARGETED    it names one task_id.        (the two kills were 2.7s apart)
#   LIVE        it expires.                  (a stale ack is not a decision)
#   CONSUMED    it is spent on first use.    (one ack, one destruction)

ACK_TTL_SECONDS = 900          # 15 minutes: long enough to write, too short to bank
MIN_DESTROYING = 20
MIN_WHY = 20


def ack_path(entity_root):
    return os.path.join(entity_root, ".claude", "state", "stop-work-acks.jsonl")


def read_acks(entity_root):
    p = ack_path(entity_root)
    if not os.path.isfile(p):
        return []
    out = []
    try:
        with open(p, encoding="utf-8", errors="replace") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                try:
                    out.append(json.loads(line))
                except Exception:
                    continue
    except OSError:
        return []
    return out


def find_live_ack(entity_root, task_id, now=None, session_id=""):
    """The newest unconsumed, unexpired, well-formed ack for this exact target."""
    now = time.time() if now is None else now
    target = (task_id or "").strip()
    if not target:
        return None, "no task_id to match an ack against"
    best = None
    stale = expired = malformed = 0
    for rec in read_acks(entity_root):
        if (rec.get("task_id") or "").strip() != target:
            continue
        if rec.get("consumed"):
            stale += 1
            continue
        if session_id and rec.get("session_id") and rec["session_id"] != session_id:
            stale += 1
            continue
        try:
            age = now - float(rec.get("epoch") or 0)
        except (TypeError, ValueError):
            malformed += 1
            continue
        if age > ACK_TTL_SECONDS or age < -60:
            expired += 1
            continue
        # A BARE MARKER EXEMPTS NOTHING.
        d = (rec.get("destroying") or "").strip()
        w = (rec.get("why") or "").strip()
        if len(d) < MIN_DESTROYING or len(w) < MIN_WHY:
            malformed += 1
            continue
        if best is None or float(rec.get("epoch") or 0) > float(best.get("epoch") or 0):
            best = rec
    if best:
        return best, ""
    bits = []
    if stale:
        bits.append("%d already spent" % stale)
    if expired:
        bits.append("%d older than %d minutes" % (expired, ACK_TTL_SECONDS // 60))
    if malformed:
        bits.append("%d without a real reason" % malformed)
    return None, ("no live stop-work-ack for %s%s"
                  % (target, (" (" + "; ".join(bits) + ")") if bits else ""))


def consume_ack(entity_root, ack):
    """Spend the ack. One ack, one destruction — rewritten in place.

    Best-effort: a failure to mark it spent must never turn an ALLOW into a
    refusal, so the caller ignores the result. The worst case is one ack that
    could cover a second kill inside the same 15 minutes, which is strictly
    better than a guard that blocks because it could not write a file.
    """
    p = ack_path(entity_root)
    if not os.path.isfile(p):
        return False
    try:
        with open(p, encoding="utf-8", errors="replace") as fh:
            lines = fh.read().splitlines()
        out = []
        done = False
        for line in lines:
            try:
                rec = json.loads(line)
            except Exception:
                out.append(line)
                continue
            if (not done and not rec.get("consumed")
                    and rec.get("id") and rec.get("id") == ack.get("id")):
                rec["consumed"] = True
                rec["consumed_at"] = time.strftime("%Y-%m-%dT%H:%M:%S%z")
                done = True
            out.append(json.dumps(rec, ensure_ascii=False))
        if done:
            tmp = p + ".tmp"
            with open(tmp, "w", encoding="utf-8") as fh:
                fh.write("\n".join(out) + "\n")
            os.replace(tmp, p)
        return done
    except OSError:
        return False


# ---------------------------------------------------------------------------
# 2. THE VERDICT
# ---------------------------------------------------------------------------

ALLOW_NOT_LIVE = "allow-not-live"
ALLOW_SELF = "allow-self"
ALLOW_ACK = "allow-acked"
ALLOW_RECOVERY = "allow-recovery"
ALLOW_UNDECIDABLE = "allow-undecidable"
REFUSE = "refuse"


def decide(task_id, liveness, entity_root, self_agent_id="",
           resolved_agent_id="", now=None, session_id="", recovery_authority=False):
    """The whole contract, as one pure function so the tests can drive it.

    `liveness` is "ALIVE" | "NOT-ALIVE" | "INDETERMINATE" — supplied by the
    caller from scripts/lib/agent-liveness.py, never re-derived here.

    THERE IS NO user_text PARAMETER, AND THAT IS THE DESIGN. Between
    2026-09-10 and 2026-09-20 there was one, and what it did with the CEO's
    sentences is written at the top of this file. Nothing about what he wrote,
    or what anyone wrote, reaches this decision: the only questions are
    whether the target is running and whether a reason is on disk.
    """
    tid = (task_id or "").strip()
    if self_agent_id and tid and tid in (self_agent_id, "agent-" + self_agent_id):
        return ALLOW_SELF, "a teammate stopping itself destroys nobody else's work", None
    if (self_agent_id and resolved_agent_id
            and resolved_agent_id == self_agent_id):
        return ALLOW_SELF, "a teammate stopping itself destroys nobody else's work", None

    if liveness == "NOT-ALIVE":
        return (ALLOW_NOT_LIVE,
                "the target is not running — retiring a finished teammate "
                "destroys nothing", None)
    if liveness != "ALIVE":
        return (ALLOW_UNDECIDABLE,
                "liveness is INDETERMINATE for %s — allowed, and said out loud "
                "rather than guessed" % (tid or "the target"), None)

    if recovery_authority:
        return ALLOW_RECOVERY, "exact-worker cancellation requested by recorded recovery control", None
    ack, ack_why = find_live_ack(entity_root, tid, now=now, session_id=session_id)
    if ack:
        return ALLOW_ACK, ack_why, ack

    return REFUSE, ack_why, None


# ---------------------------------------------------------------------------
# 3. CLI — the hook's single entry point
# ---------------------------------------------------------------------------
# NOTE THE ARGUMENTS THAT ARE NOT HERE: --transcript and --user-text. They fed
# the deleted authority B, and a CLI that still accepted them would be a door
# left in the wall for the next person who thinks reading his sentence is
# cheaper than writing the ack. An unknown argument is an error, loudly.

def _cli():
    import argparse
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--task-id", required=True)
    ap.add_argument("--liveness", required=True,
                    choices=["ALIVE", "NOT-ALIVE", "INDETERMINATE"])
    ap.add_argument("--entity-root", required=True)
    ap.add_argument("--self-agent-id", default="")
    ap.add_argument("--resolved-agent-id", default="")
    ap.add_argument("--session-id", default="")
    ap.add_argument("--consume", action="store_true")
    a = ap.parse_args()

    recovery_authority = False
    try:
        from pathlib import Path
        import recovery_controls
        import second_review
        if recovery_controls.R.mode(a.entity_root)["mode"] != "off":
            ws = second_review.load_workspaces(str(Path(__file__).resolve().parents[2]))
            recovery_authority = recovery_controls.cancellation_authority(ws, a.task_id, a.session_id)
    except (OSError, ValueError, ImportError):
        pass  # No typed authority: the existing live-stop predicate still applies.
    verdict, why, ack = decide(
        a.task_id, a.liveness, a.entity_root,
        self_agent_id=a.self_agent_id, resolved_agent_id=a.resolved_agent_id,
        session_id=a.session_id, recovery_authority=recovery_authority)
    if verdict == ALLOW_ACK and ack and a.consume:
        consume_ack(a.entity_root, ack)
    print(json.dumps({"verdict": verdict, "reason": why,
                      "ack": ack, "task_id": a.task_id,
                      "liveness": a.liveness}, ensure_ascii=False))
    return 2 if verdict == REFUSE else 0


if __name__ == "__main__":
    sys.exit(_cli())
