# The unasked-deferral corpus — how the number was got, so it can be got again

`guard-unasked-deferral.py` states a precision figure in its header. A figure in
a header is a claim; this file is the method behind it. Anyone who doubts the
number can reproduce it in about two minutes and anyone who WIDENS the
construction set is expected to re-run it before shipping.

## The corpus

**2,198 orchestrator turns.** Every non-sidechain turn in the five main-session
transcript directories on this machine on 2026-08-31:

```
~/.claude/projects/<project-1>
~/.claude/projects/<project-2>
~/.claude/projects/<project-5>
~/.claude/projects/<project-3>
~/.claude/projects/<project-4>
```

Those are the orchestrator's OWN sessions. Subagent sidechains are excluded:
this guard is about the lead's language and a teammate's prose is a different
register that would pollute the measurement in both directions.

**The specimen turn is in it** — session `042f3850-8b30-478e-97c3-9c991a83fa86`,
which is the 2026-08-31 turn the CEO had to ask about a third time.

## What a "turn" is and why it is defined this way

A turn = the run of assistant records following a REAL user prompt, up to the
next real user prompt. A `user` record whose content is entirely `tool_result`
blocks is not a prompt.

From each turn, two things — and ONLY the two things a `Stop` hook can actually
see at the moment it fires:

| field        | what it is                                          | what the hook reads it from |
|--------------|-----------------------------------------------------|------------------------------|
| `final_text` | the LAST assistant text block in the turn            | the payload's `last_assistant_message` |
| `tools`      | every tool name used anywhere in the turn            | the transcript, walked backward to the prompt |

That asymmetry is not an accident and it is not this file's invention: at Stop
time the transcript already holds the turn's `tool_use` records but NOT the
final assistant text, which has not been flushed. `guard-unresolved-claims.sh`
verified that against the shipping binary (2.1.251) and this measurement
inherits it. Measuring against anything else would be measuring a hook that
does not exist.

## Reproducing it

Three throwaway scripts, none of which ship — they are described here in full so
they can be rewritten rather than trusted:

1. **extract** — walk each `*.jsonl`, emit one JSON object per turn with
   `final_text`, `tools`, `session`, `prompt`.
2. **probe** — run a candidate regex over `final_text`, print every hit with 160
   characters of leading and 220 of trailing context. THIS IS THE STEP THAT
   MATTERS. Every construction in the shipped set and every one in the REFUSED
   table was judged by reading its hits, not by looking at a count.
3. **audit** — for each turn matching the SHIPPED set, print the construction,
   the discharge that fired (or `None`) and the surrounding text. The result is
   hand-adjudicated: genuine unilateral deferral or not.

Import the analyzer directly rather than re-implementing it — `find_deferrals`
and `discharge` are the two entry points and a measurement of a re-implementation
measures the re-implementation:

```python
import importlib.util
spec = importlib.util.spec_from_file_location(
    "gud", "<engine>/scripts/hooks/guard-unasked-deferral.py")
gud = importlib.util.module_from_spec(spec); spec.loader.exec_module(gud)
gud.find_deferrals(turn["final_text"])
gud.discharge(turn["final_text"], set(turn["tools"]))
```

## The result, 2026-08-31

```
turns                        2198
construction matched           13
  discharged AskUserQuestion    0
  discharged CEO's hand         9
  discharged Agent spawn        1
FIRES                           3
```

All 3 fires hand-read as genuine unilateral deferrals. **Precision 3/3.**

The three:

1. the specimen — *"I'm deliberately not spawning a fifth agent for it right
   now ... It goes in with the dialect guard's land — next spawn after these
   finish."*
2. *"I'm serializing it behind Norm rather than starting it now, because it
   lives in the same transcription pipeline he's working in and two agents in
   there would tangle."*
3. *"I'm queueing the fix rather than dispatching it now: it lives in
   `intake_sweep` inside `build-roster.py`, which the Eric Masi work currently
   owns and two agents in that file is how the merge conflicts start."*

All three share the shape: the grounds are the orchestrator's own (collision,
load, ordering), the reasoning is sound and NONE of them is put to the CEO.
That is the defect — not the deferring.

**Recall, stated because a precision figure alone is a half-truth.** Hand-reading
every construction match found **5** turns I judge genuine unilateral deferrals.
It fires on 3.

- One is lost to the **Agent-spawn discharge**: *"Understood — the PR is
  authorized. I'm sequencing it deliberately rather than firing it off now."*
  That turn spawned an unrelated agent and a Stop hook cannot tell which thing
  a spawn started. Named as a blind spot in the analyzer's header rather than
  engineered around.
- One is lost to the **`postponing-it` family being deleted** — *"I'm holding
  this a beat so it's built against a streaming interface."* See below.

**n = 3 is small and the small n is the point.** This defect is rare in the
record and loud when it happens, so the only useful tuning target was zero false
alarms. A guard that cried wolf on ordinary sequencing prose would be switched
off within a day and a switched-off guard is exactly the failure mode that
produced the defect.

## The family that was written, measured and then deleted

`postponing-it` — `I'm (holding|parking|queueing|deferring|shelving|sequencing|
serializing) <it|that|the X>` — was implemented, run and **cut at 4 genuine out
of 9 hits (44%)**.

Its false alarms are not marginal; they are plainly correct pipeline holds:

- *"I'm holding the merge until all three report, then landing them as one
  sequence with a single deploy rather than three."*
- *"I'm holding the land until his tests confirm green, then landing it
  alongside the timeout fix so the watcher restarts once rather than twice."*
- *"I'm holding the next dispatch until his numbers land: loro test runs would
  contend for the same CPU and Metal."*

And 3 of its 4 genuine hits were already caught by `chose-not-to-start-now`. So
it bought ONE extra catch at the price of firing on ordinary sequencing. Cut.

`unasked-deferral.mutation.sh` puts it back as a mutant and asserts the suite
goes red at case `3h`, so the deletion cannot be quietly undone.

## The other refusals

Full table with per-construction hit counts is in the analyzer's header. The
short version: bare `deliberately not` (4/23 — the record's own "Deliberately
NOT open" heading dominates), `I'll X once Y completes` (0/9), `when it's done`
(0/12), `after those land` (3/14 — roadmap narration), `wait until` (39 hits,
mostly the CEO or a process waiting), `in the same commit as` (0/1), bare
`rather than <gerund>` (3/6 — the rest are design choices), bare `X goes in
with Y` (1/3).

## Re-run, 2026-10-23: sentences that hold nothing

Six hold records opened that day; two came from replies DENYING a hold ("Nothing
of yours is on hold.", "none of your work is on hold.") and one from "I'm not
starting anything alongside that", which names no work. Two narrowings, measured
with the same extract and probe over every lead turn in the femcboost project
directory (11,545 turns):

- **Family D, a denied hold word** (`hold_words.held()`): the word right after a
  subject saying nothing is held ("nothing", "none of your work", "no teammate")
  or a negated verb ("is not", "no longer") holds nothing. 8 hits fewer, all 8
  denials, e.g. *"No teammate was on hold, so there's nothing to resume."*
  *"Nothing new: the latest notice is a repeat about a branch that's on hold."*
  still opens a hold: the colon ends the denial.
- **Family A, an object that names no work** (`_NOTHING_MORE`): "anything /
  anyone" followed by else, new, alongside, in parallel, on the side or on top.
  22 hits fewer, all of them "nothing more runs" statements (quota, CPU, his own
  stop orders). *"I'm not spawning anything for it right now"* still fires.

The specimen and the corpus's genuine deferrals still match. Unchanged and
judged real: *"I'm not starting the reviewers yet"*, *"I'm not running it now"*
(a land put behind the nightly) and *"Then I parked everything behind the
nightly and the stable"*: each says his work is not running.

## When to re-run this

- Before widening any construction. A widening that is not measured is a guess.
- If the guard is reported as noisy. The corpus is the arbiter, not an argument.
- After any change to `_HANDBACK`: it discharges 9 of 13 matches, so it is doing
  more work than any single construction. Without it the guard fires 12 times at
  4/12 = 33% and shouts at the CEO for turns in which he himself gave the order.
