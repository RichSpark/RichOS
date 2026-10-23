# The false-positive corpus for `ass-kicker.sh`

A blocking guard that is wrong gets waived and a guard that is waived habitually
is a formality with a hook attached. So before either arm of this one was allowed
to `exit 2`, it was replayed over every real orchestrator turn in the governed
project, through the analyzer that ships and every fire was read. This file is
the record of what was measured, what was refused and the one number per arm.

## What the gate was built for

2026-09-02, one session. The lead wrote a sentence describing an action and
treated having written it as having done it, seven times. Two of the seven share
a machine signature — the text states an action and the turn's tool calls do not
contain it:

| time  | the sentence                                   | the turn's tool calls |
|-------|------------------------------------------------|-----------------------|
| 17:49 | "Zach builds it tomorrow."                     | Bash ×3, no Agent     |
| 18:29 | "Frank breaks it first and I want him …"      | Bash ×7, no Agent     |

The CEO's next message after the second asked, in blunt terms, where Frank was
this time; after the apology, he said this class of failure would never end.

The same day he sent six messages of a second shape — "No Frank this time?",
"Next Sage.", then a blunter repeat of the same — each restarting work that had
paused because a teammate returned and the lead answered with a report and no
dispatch. `guard-idle-land.sh` saw all three of the evening's finishes and stood
down each time on its backlog term (`verdict: backlog-empty, rows: 41, free: 0`,
from its own observation log): the next step after a returned design is not a
backlog row.

## The corpus

**Every turn in every orchestrator transcript of the governed project.** Not a
sample and not turns written for this purpose.

| | |
|---|---|
| Source | `~/.claude/projects/<project>/*.jsonl`, over the project this engine was first adopted in |
| Session files read | **19** |
| Turns replayed | **1,280** |
| Turns carrying a final assistant message | **1,226** |
| Span | 2026-07-27 → 2026-09-02 (the live session included, to its last turn at replay time) |
| Measured | 2026-09-02, through the shipped `guard-stated-actions.py` |

## Regenerating it

Each turn is a `promptId` span — the file-order records from the first one
carrying that id to the first one carrying a different id. The span's records go
into a file of their own, which is exactly what `read_turn` sees at Stop time
(the transcript ends at the turn's end); the payload carries that path, the
`prompt_id`, the `session_id` and the last assistant text of the span as
`last_assistant_message`. The analyzer runs as a subprocess with
`RICHOS_SA_ENTITY_ROOT` pointing at a scratch entity that holds a copy of the
project's `.claude/agents/*.md` (the roster the roles are derived from) and
`RICHOS_SA_TEAMS_DIR` pointing nowhere. Exit code, stderr and the analyzer's own
observation record are collected per turn. Sidechain records are skipped: a
subagent's turn fires `SubagentStop`, not `Stop`.

An INDEPENDENT reader — a second, separately written segmentation that records
each span's tool names and whether the span holds a `<task-notification>` — is
run over the same files and compared against every ARM 2 refusal: a refusal
whose span the second reader says held an `Agent` or `AskUserQuestion` call or
no notification, would be a disagreement. There were **0** over 265.

## ARM 1a — ROLE ACT: **SHIPPED, BLOCKING**

A clause whose subject is a roster role (capitalized, first word of the clause),
whose verb is present-simple third person and whose object is a pronoun or a
determiner phrase, in a turn with no `Agent` call and no `SendMessage` to a
teammate.

### The raw shape, before its exclusions — 15 fires, 13 false

| turn | clause | class |
|---|---|---|
| 0224e460:903 | Zach builds it tomorrow. | **the defect** |
| 0224e460:1203 | Frank breaks it first | **the defect** |
| 0224e460:607 | Sage comes back with either: | report/return verb, running agent |
| 042f3850:380 | **1.** Zach builds it — in flight now. | list item; present-progress |
| 042f3850:500 | **1.** Zach finishes → 2. I verify … | list item; return verb |
| 29f6d180:117 | Frank recommends the hybrid | report verb |
| 29f6d180:293 | **-** Dean fixes Sterling's definition | list item (two agents running) |
| 44276098:168 | Zach fixes 1–3 as one job | bare object; turn ends on a question |
| 6ae8d995:1846 | Echo continues on the rename | continue verb; SendMessage in turn |
| 80de54b4:371 | Echo recommends both fixes | report verb |
| 80de54b4:950 | Sage rates this the biggest risk | report verb |
| 80de54b4:2336 | Clark researches what an art director is | bare object; "Say go and I'll start" |
| 80de54b4:2496 | Art designs Bootstrap components | bare object (a craft, described) |
| db69ba07:302 | **5.** Iris builds each … **7.** Echo implements it | list items (a plan he asked for) |

Every false one belongs to one of four classes and each class is excluded for a
reason that is stated in the analyzer rather than by tuning to the row:

* **REPORT VERBS** — saying, judging, returning, continuing, thinking, being.
  These describe what an agent produced or is; that is the liveness class, owned
  report-only by `guard-agent-state-claims.py` on its own measurement.
* **LIST ITEMS** — a bulleted or numbered line is a plan or a status table. Both
  real failures were plain sentences in prose. (The replay found one hiding
  inside emphasis: `**1. Zach builds it — in flight now.**`; the line filter now
  reads through leading `*`/`_`.)
* **BARE OBJECTS** — "Art designs Bootstrap components" describes a craft. An
  announced act takes a pronoun or a determiner: "builds **it**", "breaks
  **the** design".
* **CEO PROPOSALS** — a clause conditioned on his word ("say the word and …"), a
  reply whose last sentence is a question to him or an `AskUserQuestion` this
  turn. Ending a turn on his answer is legitimate by every rule in this engine.

And two more that the end-to-end replay found after the first pass, both in the
quiet direction:

* **ANY `Agent` call discharges 1a.** Role-matching it fired on "Sage is designing
  that now … Zach builds it." in the turn that spawned Sage — a plan in motion,
  not the defect. (ARM 1b stays role-matched: "I'm dispatching Frank" names who.)
* **A dash does not split a clause.** "Zach builds it — in flight now" is one
  clause and the appositive is what marks it as a state claim.

### After the exclusions

| | |
|---|---|
| fires over 1,226 final messages | **2** |
| genuine (the two named turns) | **2** |
| false | **0** |

**0.0% false-positive rate, n = 1,226. Recall is the price and it is stated:** a
role act with a bare-noun object ("Frank breaks the elimination design first")
is not caught and neither is one inside a list.

## ARM 1b — FIRST-PERSON DISPATCH: **SHIPPED, BLOCKING, 0 FIRES**

"I'm dispatching Frank", "I'll spawn frank-opus-x1 now", "Dispatching Zach" —
first person or sentence-initial gerund, a dispatch verb and a role or agent
name as the object; discharged only by an `Agent` call for that role or name.

| | |
|---|---|
| raw matches of the shape | 10 |
| … every one "Say the word and I'll dispatch it" / "Say go and I'll start …" | 10 |
| fires after the proposal exclusion | **0** |
| genuine | 0 |

This is `guard-unresolved-claims.py`'s 17%-precision `dispatching|spawning|
launching` family narrowed three ways: first person (past incidents and
quotations drop out), must name who (the "names nobody" refinement that measured
10.3% is inverted — a nameless dispatch is not scanned) and not conditioned on
the CEO's word. It ships blocking on the precedent of that guard's
never-dispatched-role arm: 0 fires, 0 false, zero cost and a positive probe in
the suite (case c) proves it can fire.

## ARM 1c — ROLE FUTURE: **REPORTS ONLY**

"Sage will come back with the actual count", "Sage will fold all of it in and
I'll land it". The shape matched 2 sentences in the corpus; both were predictions
about an agent already running and neither was an untaken act. 0 genuine of 2.
It reports, with that number in its refusal text, so nobody promotes it without
re-measuring.

## Refused outright — measured and shipped in no form

| shape | fires | what they were |
|---|---|---|
| "Landed / merged / pushed" with no `git merge`/`git push` this turn | 12 | cross-turn references ("Landed at `13e3f00ac`" about an earlier turn's merge) and nouns ("Landing rule enforced per merge"). The SHA-bearing landing claim is already owned by `guard-unresolved-claims` against the repository, which is the right ground truth. |
| "Running now: X" / "I'll run X" with no tool call | 5 | status about work dispatched in an earlier turn, "running tally" and a conditional future. Liveness class. |

## ARM 2 — THE TURN THAT STOPS: **SHIPPED, BLOCKING**

A host-written `<task-notification>` in the turn's window saying an Agent
FINISHED with `<status>completed</status>`; no `Agent` call; no backgrounded tool
call; no `AskUserQuestion`; no hold or end-of-day in the operator's own words;
and no valid `stop-declared:` line in the reply. Every term is
`guard-idle-land.py`'s, by import, except the one that gate adds — the backlog —
which is exactly the term that let the three named turns through.

| | |
|---|---|
| turns whose window carries an Agent-finished notice | **368** |
| … started something (Agent or backgrounded) — silent | 101 |
| … put something to the CEO or held — silent | 2 |
| … declared — the vocabulary did not exist before 2026-08-30 | 0 |
| … **undeclared stop — refused** | **265** |
| independent reader disagreements over the 265 | **0** |

**The mechanical false-positive rate is 0 over 265, in the sense the engine's
prior Stop guard used ("zero cases where the gate misread ground truth").** Every
refused turn really carries a completed-Agent notice, really made no `Agent` or
backgrounded call, really asked nothing and was held by nothing.

**What that number does not say, said plainly.** Whether each of those 265 stops
was *legitimate* — the other designers still running, the thread genuinely done,
a decision genuinely the CEO's — is the very thing the declaration exists to
state and it cannot be read off a transcript. Sixteen were read by hand, spread
across twelve sessions; they divide into exactly the three declared cases plus
the defect:

* parallel-wave progress reports while the rest of the wave runs ("Wave 1: 6 of 9
  in", "Batch 3 is in: … only batch 2 outstanding") — `waiting-on-teammate`;
* a thread ending on his next word ("Send decisions remain yours", "Nothing sends
  without your word") — `ceo-owns-it` / `nothing-unblocked`;
* the defect: a returned design answered with a report and his next message a
  restart — the three named turns and two more that arrived in the live session
  while this was being measured (0224e460:1610, 0224e460:1708).

**The cost, as a number rather than an adjective:** 265 of 1,280 turns (20.7%)
would have needed one declaration line. A refusal costs one extra turn,
structurally — the gate stands itself down on `stop_hook_active` — and the line
it asks for is shown to the CEO every time it is used, which is what keeps it a
declaration and not a token. The rule this enforces is his, in his own record:
*"A land ends by STARTING the top unblocked item, then reports. Not the other
way round."*

## What is deliberately not caught

* **A claim about state that no tool call established** — "three repos clean and
  pushed", "the scheme is untouched". Five of the day's seven instances. No
  monotonic ground truth for most of it; a different guard, if any.
* **A deferral announced in prose** — "What's parked until the two blockers are
  solved: …" in a turn that DID dispatch an Agent. `notice-unasked-deferral.py`
  owns that class, report-only and measured "parked" at 32 hits mostly nouns.
* **A turn with no final text.** There is no report to reconcile.
* **Anything after the turn ends.** Point-in-time, like its siblings.

## The cost of a fire, stated as the objection

One turn, at most, per refusal. On the re-fire `stop_hook_active` is true and
ARMS 1 and 2 stand down. The refusal names the clause or the finished agent, what
the turn actually called and the exact line that would have let it through — so
the correction is a dispatch or a sentence, not an investigation.

## ARM 3 — RECORDED, NOT WRITTEN: **SHIPPED, BLOCKING** (2026-10-02)

The CEO asked "recorded WHERE?" or the same question in other words, **13
times** between 2026-09-08 and 2026-10-02, about **9 claims on 7 days**. The
excerpts (his messages, the lead's claims, what each turn wrote) are in the
private record, `richos-hq/docs/audits/2026-10-02-recorded-where/`, together
with the per-turn replay output. Only counts and shapes are here.

**Corpus:** all 72 lead transcripts of the governed project (2026-08-07 to
2026-10-02), replayed at **every Stop point** (the last text before a sibling
hook's refusal, before a `stop_hook_summary` or at the end of the span) by
[`tests/record-claims.replay.py`](../tests/record-claims.replay.py), through
the shipped analyzer. The claim that started it sat at an inner Stop point
refused by another hook, so a replay of span ends alone would have missed it.

| | |
|---|---|
| Stop points replayed | 5,337 |
| Turns whose final text claims a record | 382 |
| Backed | 146 |
| Refused | 236 |
| … only a private note (memory or ECS), no "in my private notes" | 80 |
| … nothing written at all | 10 |
| … a durable write and the reply never says where | 69 |
| … a repository record named, no commit | 66 |
| … a citation of an existing record that names no place | 11 |
| **Turns refused by a detector misfire alone (read by hand)** | **10 (4.2% of refusals, 0.19% of Stop points)** |

**Every claim of the record family he challenged is refused** (6 of 6). Three of
his questions answered something this arm cannot see: an omission with no claim
behind it, a correction claim ("corrected at <sha>") and a mechanism claim
with no record verb.

**The misfire classes left**, each a sentence shape the arm reads as a claim and
is not: past confessions ("I recorded it wrong."), version-control "saved"
("saved their work"), descriptions of an existing document ("Documented, never
fixed.") and a citation of an old rule without its place. Each costs one
corrected sentence.

**The commit term, priced:** 66 of the refusals are real repository records
whose reply named the place and no commit. The brief requires the commit; the
fix is one token. Without the term, refusals would be 170.

**Eight classes were removed by measurement**, each listed in the private
record with the case that exposed it, among them: list items had to be scanned;
Bash targets resolve through `cd` and shell variables; `awk 'NR>=593'` is not a
redirect; ECS checkpoints are private; engine record commands count when their
entry is named; a claim pointing at "my notes" is private whatever else the
turn wrote. Two bugs in the arm were found by the replay before it shipped: a
quadratic path pattern on a long command and a directory walk that never ended
on a path starting with `//`.

**The cost of a fire:** unlike ARMS 1 and 2, ARM 3 does not stand down on the
re-fire. The re-fire carries the corrected reply; if it still claims a record
nobody wrote, it is refused again. Removing a false sentence needs no tool, so
this cannot strand a turn and there is no declaration line that excuses it.

### ARM 3 became an alert (CEO, 2026-10-02)

The refusal described above is replaced. His spec: check whether a "recorded"
claim was followed by the appropriate action and issue an alert if it was not;
then, "I don't need to see it. YOU need to see it and do your job." So ARM 3 no
longer refuses, blocks or re-fires, recites no SHA and shows the CEO nothing:
it puts one line in the lead's own context (a Stop `additionalContext`) when the
turn wrote nothing durable behind the claim or only a private note the reply
did not call one. Any durable write silences it, whether or not the reply names
a place.

Replay of the same 72 lead sessions (5,357 Stop points) after the change: 374
claim turns, 87 alerted (a private note only or nothing written), 287 silent
(143 backed, 144 with a durable write behind them or a citation of an existing
record). Read by hand, 6 of the 87 are clear misreads (a citation of an existing
rule, a description of a document, a confession, git "saved and pushed") and 7
more cannot be settled from the sentence alone; the other 74 are true alerts.
The three misread classes named above are now excluded by the detector.
