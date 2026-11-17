#!/usr/bin/env python3
"""guard-agent-state-claims.py — THE ANALYSIS HALF of the Stop-time check that
a statement about a NAMED AGENT'S STATE agrees with that agent's authoritative
liveness.

===========================================================================
THE DEFECT, WITH THE DATE AND THE SENTENCE
===========================================================================
2026-08-31, ~22:45. The lead's report to the CEO contained this table row:

    | `zach-opus-g1` | **completed** | the ask gate — landed and pushed already |

`zach-opus-g1` was not completed. The CEO's own screen showed it working —
*"Confirming clean status in ceo-asks.test.sh worktrees"* — and its isolation
worktree lock was held the entire time. `git worktree list` said `locked`.
`remove-agent-worktree.sh` REFUSED to remove it, for exactly the right reason.
The lead called the lock "residue" and quoted the `ListAgents` roster, which
said `completed`. The roster was stale. The CEO corrected his own assistant
from a screenshot.

Doctrine already covered the absence of a signal — *never infer an agent is
dead from filesystem inactivity*. It did not cover the INVERSE, which is what
happened: A POSITIVE-LOOKING SIGNAL THAT IS WRONG. A stale `completed` is worse
than no status at all, because absence prompts a check and a false positive
does not.

===========================================================================
WHY THIS REPORTS AND DOES NOT BLOCK — SAID IN THE HEADER, AS INSTRUCTED
===========================================================================
Two reasons, and the second is the honest one.

1. THE HARM HAS ALREADY HAPPENED. Unlike guard-idle-land.sh, which refuses a
   turn that is about to leave work unstarted, the thing this guard sees is a
   sentence already written. The value is that the CEO reads the correction in
   the same turn instead of sending a screenshot; blocking would only delay the
   same correction by one round trip.

2. THE LIVENESS HALF'S FALSE-POSITIVE RATE IS UNPROVEN, in those words. It
   cannot be measured on the archive: worktree lock state is not retained, so
   there is no way to ask of a 2026-08-14 message "was that agent's lock held
   when this was written?". And there was one known false-positive mode, named
   rather than glossed: AN AGENT THAT GENUINELY FINISHED BUT WHOSE WORKTREE HAS
   NOT YET BEEN REAPED read ALIVE from the lock, because the lock is the
   SESSION's and is released by removal, not by the agent's last breath. Since
   round 8 (2026-09-13) the resolver this guard calls asks the workspace
   registry's finished_state first (docs/plans/worktree-spec-2026-09-11.md,
   point 11), so a finished agent the registry knows reads NOT-ALIVE and the
   guard stays silent on a true claim about it; an agent the registry does not
   know still falls back to the lock, and that residue is why this still
   reports. A guard that blocked on an unmeasured signal with a known FP mode
   would be switched off inside a day, and a guard people switch off protects
   nothing. That is the failure mode that produced this defect in the first
   place.

   The sibling guard set the standard: it rejected a refinement measuring 10.3%
   precision and kept it as a report with the number written down. Same rule
   here. If somebody later retains lock history and measures this, the number
   goes in this docstring and the decision can be revisited on evidence.

===========================================================================
THE DETECTOR — MEASURED ON 4,230 REAL FINAL MESSAGES (177 transcripts)
===========================================================================
Corpus: the last assistant text of every turn in every session transcript under
~/.claude/projects, built the same way the sibling guard built its 3,532.

  POSITIVE PROBE FIRST, so a zero cannot pass for the wrong reason: 130 of the
  4,230 messages contain an agent identifier at all, and 16 sentences contain
  an identifier AND a completion word. Those 16 are the ground truth the
  constructions below were fitted to and adjudicated against.

  T1  TABLE ROW           2 hits   2/2 genuine   <- the exact defect, twice
      `| <name> | completed | ... |` — a markdown row whose name cell is an
      agent identifier and a LATER cell opens with a terminal state.
  T2  ADJACENT PREDICATE  1 hit    1/1 genuine
      `<name> is/has/'s <terminal>`, or a bare past-tense terminal verb.
  T2b APPOSITIVE          2 hits   2/2 genuine
      one parenthetical or appositive allowed between name and predicate:
      "`clark-opus-d1`, the licensing research, which finished".

  EXTRACTOR TOTAL: 5 hits over 4,230 messages, 5 of 5 genuine claims that a
  NAMED agent had finished. No ordinary prose entered, because the trigger
  requires a full `<role>-<model>-<id>` identifier — no English sentence
  contains "word-opus-word" by accident.

  RECALL on the 16 probe sentences: every unambiguous completion claim was
  caught. The ones deliberately not caught are listed under REFUSED below.

  Two of the five (the clark-opus-d1 pair) were TRUE, and the guard is silent
  on them: that agent's worktree is unregistered, so liveness returns NOT-ALIVE
  and nothing is said. That is the end-to-end negative case, on real data.

===========================================================================
CONSTRUCTIONS REFUSED, AND WHY — the list matters as much as the list kept
===========================================================================
* `landed`, `reported`, `delivered`, `handed off`, `signed off` as terminal
  words. THEY DESCRIBE THE WORK, NOT THE AGENT. Proven by the incident itself:
  the correct later message read "| `zach-opus-g1` | ALIVE | landed already;
  still ..." — the branch was landed and the agent was running. Including these
  would have fired on the one statement that was RIGHT.
* `ran`, `stopped`. Ambiguous between "has stopped" and "was stopped from".
* BARE ROLES ("Zach is done"). Nothing to resolve: a role does not name an
  agent, and on the day of the defect three Zachs were running at once, so a
  role-prefix match would have had to pick one. guard-unresolved-claims.sh
  already owns the bare-role case with its own measured numbers.
* The `dispatching|spawning|launching` prose family. The sibling guard measured
  it at 17%, measured its "names nobody" refinement at 10.3%, and refused to
  enforce either. Not repeated here.
* A claim whose name cannot be joined to an agent id (see below). The guard
  says nothing rather than guessing which agent was meant.

===========================================================================
NAME -> AGENT ID: AN EXACT JOIN, NOT A HEURISTIC
===========================================================================
`spawned-names.log` holds names with no ids; `worker-events.jsonl` holds ids
with no names. The join exists only in the orchestrator's own transcript, and
it is exact: the `Agent` tool_use carries the teammate `name` and a tool_use
id; the following tool_result carries `toolUseResult.agentId` under the same
id. scripts/lib/agent-liveness.py:names_to_ids does that join.

An unjoinable name produces NO verdict and NO notice. The tempting fallback —
match the role prefix against a live worktree of that agent type — is refused
above: it is ambiguous exactly when it matters.
"""

import json
import os
import re
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(_HERE), "lib"))

try:
    import importlib.util as _ilu
    _spec = _ilu.spec_from_file_location(
        "agent_liveness", os.path.join(os.path.dirname(_HERE), "lib", "agent-liveness.py"))
    _al = _ilu.module_from_spec(_spec)
    _spec.loader.exec_module(_al)
except Exception:
    _al = None


# Both shapes: with the effort word (2026-11-17) and the three parts before it.
NAME = r"[a-z][a-z0-9]{1,15}-(?:fable|opus|sonnet|haiku)-(?:(?:low|medium|high|xhigh|max)-)?[a-z0-9]{1,12}"
AGENT = r"(?<![A-Za-z0-9_-])(" + NAME + r")(?![A-Za-z0-9_-])"

# TIGHT, and the exclusions are the measurement (see REFUSED above).
TERM_ADJ = (r"(?:completed?|done|finished|idle|terminated|dead|gone|"
            r"shut\s+down|wrapped\s+up|no\s+longer\s+running)")
TERM_VERB = (r"(?:completed|finished|terminated|shut\s+down|wrapped\s+up|"
             r"ended|exited)")
ADV = (r"(?:now\s+|already\s+|finally\s+|just\s+|all\s+|fully\s+|"
       r"officially\s+|since\s+)*")

PRED = (r"(?:(?:is|was|'s|are|were)\s+" + ADV + TERM_ADJ + r"|"
        r"(?:has|have|had)\s+" + ADV + TERM_VERB + r"|"
        + ADV + TERM_VERB + r")")

T2 = re.compile(r"[`*_]*" + AGENT + r"[`*_]*(?:'s)?\s+" + PRED + r"\b", re.I)
T2B = re.compile(r"[`*_]*" + AGENT + r"[`*_]*"
                 r"(?:\s*[,(][^.!?|]{0,70}?[),]\s*(?:which\s+|who\s+|and\s+)?)"
                 + PRED + r"\b", re.I)

TABLE_STATE = re.compile(r"^[\s`*_]*" + TERM_ADJ + r"\b", re.I)
NAME_CELL = re.compile(r"^[\s`*_]*(" + NAME + r")[\s`*_]*$", re.I)

# A subordinate clause is a condition, not a claim: "once X is done", "when X
# has finished". Measured against the corpus: this removes the
# "As soon as zach-opus-kit2 and zach-opus-dp2 complete" family.
SUBORD = re.compile(r"\b(once|when|whenever|after|if|unless|until|till|while|"
                    r"before|as\s+soon\s+as|assuming|provided|should|whether)\b"
                    r"[^.!?]{0,140}$", re.I)
NEG = re.compile(r"\b(not|isn't|is\s+not|never|hasn't|has\s+not|wasn't|"
                 r"was\s+not|neither|nor|aren't)\b", re.I)
FUT = re.compile(r"\b(will|would|going\s+to|expect|expects|should|might|may|"
                 r"could)\b[^.!?]{0,40}$", re.I)
# REPORTED SPEECH ABOUT A PAST CLAIM is the correction turn itself, and a guard
# that fires on the apology is a guard that punishes the fix. Measured: this is
# what keeps "`zach-opus-g1` is plainly alive ... and I told you it was
# completed" and "still holding its lock despite reporting complete" silent.
REPORTED = re.compile(r"\b(I\s+(?:told|said|reported|called)|you\s+were\s+told|"
                      r"claimed|reporting|despite|even\s+though)\b", re.I)

MAX_CLAIMS = 20


def table_claims(text):
    out = []
    for line in text.splitlines():
        if line.count("|") < 2:
            continue
        cells = line.split("|")
        named = []
        for i, c in enumerate(cells):
            m = NAME_CELL.match(c)
            if m:
                named.append((i, m.group(1)))
        for i, nm in named:
            for c in cells[i + 1:]:
                if TABLE_STATE.match(c):
                    out.append((nm, line.strip()[:220], "table-row"))
                    break
    return out


def sentence_claims(text):
    out = []
    for s in re.split(r"(?<=[.!?\n])\s+", text):
        for tag, rx in (("adjacent", T2), ("appositive", T2B)):
            for m in rx.finditer(s):
                pre = s[:m.start()]
                if SUBORD.search(pre) or FUT.search(pre) or REPORTED.search(s):
                    continue
                if NEG.search(s[m.start():m.end() + 40]):
                    continue
                out.append((m.group(1), s.strip()[:220], tag))
    return out


def extract(text):
    """[(name, span, construction)] -- one entry per name, table rows first."""
    seen = set()
    out = []
    for nm, span, tag in table_claims(text) + sentence_claims(text):
        if nm in seen:
            continue
        seen.add(nm)
        out.append((nm, span, tag))
    return out[:MAX_CLAIMS]


def main():
    try:
        payload = json.load(sys.stdin)
    except Exception:
        return 0

    if payload.get("stop_hook_active"):
        return 0

    message = payload.get("last_assistant_message") or ""
    if not message.strip():
        return 0

    claims = extract(message)
    if not claims:
        print("CLEAR")
        return 0

    entity_root = (os.environ.get("RICHOS_CLAIMS_ENTITY_ROOT")
                   or payload.get("cwd") or os.getcwd())
    transcript = payload.get("transcript_path")

    if _al is None:
        # The resolver is the only thing that decides anything here. Without it
        # there is no check to report the result of, and inventing one is the
        # defect. Silence, and the shell half announces the broken install.
        return 0

    name_map = _al.names_to_ids(transcript)

    fired = []
    silent = []
    for nm, span, tag in claims:
        aid = name_map.get(nm)
        if not aid:
            silent.append({"name": nm, "why": "unmapped", "construction": tag})
            continue
        rec = _al.resolve(entity_root, aid)
        v = rec.get("verdict")
        if v == _al.ALIVE:
            fired.append({"name": nm, "agent_id": aid, "span": span,
                          "construction": tag, "record": rec})
        else:
            silent.append({"name": nm, "agent_id": aid, "why": v,
                           "construction": tag})

    # Observation record for EVERY evaluated turn, fired or not -- identifiers
    # only, never the operator's words. This is what makes a future measurement
    # of the liveness half possible at all, and it is the reason it is written
    # even when nothing fires.
    record = {
        "session": payload.get("session_id"),
        "prompt_id": payload.get("prompt_id"),
        "claims": [{"name": n, "construction": t} for n, _s, t in claims],
        "fired": [{"name": f["name"], "agent_id": f["agent_id"],
                   "construction": f["construction"]} for f in fired],
        "silent": silent,
        "verdict": "notice" if fired else "pass",
    }
    try:
        state = os.path.join(entity_root, ".claude", "state")
        os.makedirs(state, exist_ok=True)
        with open(os.path.join(state, "agent-state-claims.jsonl"), "a",
                  encoding="utf-8") as f:
            f.write(json.dumps(record) + "\n")
    except Exception:
        pass

    if not fired:
        print("CLEAR")
        return 0

    # One line for the operator channel, plus a state key so a persisting
    # contradiction is announced once rather than under every turn.
    key = "alive:" + ",".join(sorted(f["name"] for f in fired))
    parts = []
    for f in fired[:3]:
        ev = f["record"].get("evidence") or {}
        dis = f["record"].get("disagreements") or []
        bit = ("%s is ALIVE: %s is LOCKED by running pid %s"
               % (f["name"], ev.get("worktree_path", "its worktree"),
                  ev.get("pid", "?")))
        if dis:
            bit += " (" + dis[0].split(" -- ")[0].strip() + ")"
        parts.append(bit)
    more = "" if len(fired) <= 3 else " (+%d more)" % (len(fired) - 3)

    msg = ("AGENT-STATE CLAIM CONTRADICTED — you reported %s finished; the workspace "
           "registry does not record it finished (point 11) and its session's lock is "
           "held by a running pid. %s%s. The roster (ListAgents) is not the source of "
           "truth for this and went stale exactly this way on 2026-08-31. Confirm or "
           "correct before the CEO has to: scripts/agent-liveness.sh %s"
           % (", ".join(f["name"] for f in fired), "; ".join(parts), more,
              fired[0]["agent_id"]))

    print("FIRE\t%s\t%s" % (key, msg))
    return 0


if __name__ == "__main__":
    sys.exit(main())
