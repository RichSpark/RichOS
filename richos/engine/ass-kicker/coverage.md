# Failure catalog and coverage

Assessment date: 2026-09-16. This snapshot contains 61 historical failure
types. The catalog can grow without changing the subsystem's interfaces.

The IDs follow the maintained historical incident register. This is a current
coverage assessment, not a copy of the incident log. Source snapshot SHA-256:
`2141eb0734a82bca47bd04114932874049df07f902db4fcf573cdfd85782f2af`. No runtime or required test depends on that record.

## How to read this page

**Verified for stated scope** means the linked local tests exercise a specific
intervention. It does not mean the whole failure class is solved, every platform
is supported or a delivered nightly build has been accepted. See the
[verification record](../../../docs/verification/ass-kicker-consolidation-2026-09-16.md).

Other assessment states are **implementation present, verification pending**,
**documented gap** and **unassessed**. Unassessed means this consolidation has not
audited the relevant engine mechanisms; it does not mean no mechanism exists.
Controls outside this directory keep their current owners.

## Assessed controls

### Brief provenance

Owner: ASS Kicker. Entry points: shared `spawn.py` and the existing
`notice-claim-capability.sh` record-write adapter. **Intervention: annotates or
reports, never blocks.** The record-write adapter uses the capability predicate,
not every provenance check.

The tested scope includes selected unsupported claims, quotations that disagree
with a named source, certain claims a cited command cannot establish and some
negative-existence claims based on narrow searches. The code does not prove
truth, detect every misreading or compel anyone to act on an annotation.
The heuristics have known false positives. A source can be genuine and wrong.

Evidence: [provenance cases](tests/brief-provenance.test.sh),
[record delivery cases](../scripts/hooks/notice-claim-capability.test.sh) and
[delivery mutations](../scripts/hooks/claim-capability-delivery.mutation.sh).

### Brief scope

Owner: ASS Kicker, using Mega Lander's integration records. Entry points:
shared spawn annotation and `guard-brief-scope.sh` on Agent dispatch.
**Intervention: blocks defined scope violations and requires applicable design
declarations.** Work without a recorded governing specification is outside this
control's scope.

The checks cover declared anchors, verdict freshness, specification identity,
recorded regression, non-convergence and retry design dispositions. They do not
establish that an anchored task is useful or a design is correct. Case S32 shows
that a declaration of open design plus a prescription in the body can still pass.

Evidence: [behavior and historical acceptance cases](tests/brief-scope.test.sh)
and [mutations](tests/brief-scope.mutation.sh). The actual round-9 fixture's bytes
are pinned by a hash assertion. A missing optional private source comparison does
not prevent required acceptance cases from running.

### Stated actions

Owner: ASS Kicker, using shared turn-manifest and idle-land readers. Entry point:
`ass-kicker.sh` at Stop. **Intervention: blocks selected report/action
mismatches and requires a declaration for certain stops.** Existing configuration
can make the gate report-only.

The tested scope is a set of report patterns within a scoped turn, not arbitrary
natural-language truth. Dispatching a tool does not establish that its intended
result exists. Requiring a stop declaration does not verify the declaration.
Missing dependencies retain their existing visible, non-blocking behavior.

Evidence: [hook-level cases](tests/ass-kicker.test.sh),
[mutations with an intact control](tests/stated-actions.mutation.sh) and the
[dated corpus report](docs/stated-actions.corpus.md).

## Catalog

Rows with an assessed control inherit its triggers, limitations and evidence
above. The assessment describes only that limited intervention. Other rows remain
unassessed in this delivery; future assessments can name controls owned by Mega
Lander, shared orchestration or other engine components.

| Type | Failure | Assessment | Intervention and remaining gap |
|---|---|---|---|
| <a id="type-1"></a>1 | The absence of a result is read as a passing result | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-2"></a>2 | The absence of a reading is read as a failure, so a normal gap raises an alarm | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-3"></a>3 | An inference is acted on when the measurement that would settle it is one command away | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-4"></a>4 | A number is chosen because it sounds safe, rather than derived from anything measured | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-5"></a>5 | A control reports a violation where it needed to refuse one, leaving attention to do enforcement's job | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-6"></a>6 | A control de-duplicates its own warnings, so a failure that persists is announced less and less | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-7"></a>7 | A number is lifted from an existing record and quoted as evidence, when that record is only a claim with a date on it | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-8"></a>8 | An action is reported as done because the command was issued, rather than because the artifact was checked | Verified for stated scope | [Stated actions](#stated-actions). Blocks selected claims without matching tool activity. Artifact success after a tool call remains outside this check. |
| <a id="type-9"></a>9 | A mutation is chained in one call with a gate that can refuse it, so a refusal silently discards the earlier steps | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-10"></a>10 | A claim is written into a durable record with no condition that voids it, so it outlives its own truth | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-11"></a>11 | Machinery is built to infer a fact that the system already holds and could simply be asked for | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-12"></a>12 | A fix is verified by calling its code by hand, which proves the code and not the behavior | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-13"></a>13 | A record whose content is a refutation is deleted as though it were finished work and the thing it refuted comes back | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-14"></a>14 | Running work is destroyed on an inference about its state and everything already done is lost with it | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-15"></a>15 | The owner of the system is the one who finds the defect, which means its failure surface does not work | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-16"></a>16 | A premise nobody ever measured is inherited by every round that follows it | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-17"></a>17 | The conclusion is right and the reason given for it is false and the reason is what later work builds on | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-18"></a>18 | A corpus is widened by hand, so the number measured over it is laundered at its source | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-19"></a>19 | An agent that waits on a background job cannot use the result, so its verification is lost even when its work survives | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-20"></a>20 | A report is written in the vocabulary of its source instead of its audience, so the reader cannot use it | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-21"></a>21 | The specification never reaches the work, so what gets built is measured against something else | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-22"></a>22 | Requirements nobody authorized are added to the work and then carried as though they had been | Verified for stated scope | [Brief scope](#brief-scope). Blocks missing or invalid declared anchors in spec-governed work. An extra requirement hidden within an anchored item can still pass. |
| <a id="type-23"></a>23 | A claim is made before it is checked and corrected only after someone pushes back | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-24"></a>24 | Work, rulings and known gaps go unrecorded until the person who ordered them asks | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-25"></a>25 | Tests write into real files and real records instead of an isolated copy | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-26"></a>26 | A gate is overridden to get past it instead of fixing the thing it refused | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-27"></a>27 | Work is started without authorization, on an earlier approval that no longer applies | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-28"></a>28 | A question is answered from what is already in context instead of from evidence, because answering is instant and checking costs a command | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-29"></a>29 | An action is taken without authorization and then reported as though it had been agreed | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-30"></a>30 | Shared state is changed while agents are running and the running agents are killed mid-task | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-31"></a>31 | Finished work leaves residue where the owner will see it, rebuilt by the person who had just cleared it | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-32"></a>32 | A nearby question is answered instead of the one asked, because the nearby one has an answer ready | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-33"></a>33 | The machinery is narrated to someone who asked only for the decision | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-34"></a>34 | A required step is simply omitted, so no claim exists anywhere for a check to catch | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-35"></a>35 | A defect is invented in the owner's own written page and he is asked to resolve a contradiction that is not there | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-36"></a>36 | A deliverable is called corrected once its patches are applied, without anyone reading the whole thing | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-37"></a>37 | The owner is made to read the process instead of being handed the outcome or the decision | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-38"></a>38 | A threat model nobody asked for is invented and rounds are spent defending against an adversary that does not exist | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-39"></a>39 | A round ships regressions against the previous build, so even the part of it that works cannot land | Verified for stated scope | [Brief scope](#brief-scope). Blocks further dispatch after recorded regression. It depends on the governing harness measuring the regression. |
| <a id="type-40"></a>40 | The orchestrator writes the file himself and a file nobody reviewed ships | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-41"></a>41 | A correct check speaks only at the last possible moment, so every mistake costs a full round trip | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-42"></a>42 | One kind of rule is filed on two surfaces and the half nobody is thinking about is the half that refuses them | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-43"></a>43 | A question about a visible symptom is treated as permission to explain the mechanism instead of the outcome | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-44"></a>44 | An agent is spawned to type up analysis the brief already contains, so it has nothing to find out | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-45"></a>45 | A mechanism enforces half of its own stated rule and its existence is read as covering all of it | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-46"></a>46 | Work finishes and leaves things behind and nothing asks whether the place was left as it was found | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-47"></a>47 | An answer is withheld while it is being made certain and the cost of that certainty is charged to the person waiting | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-48"></a>48 | A session's final handoff is written from recollection, so the next session has to re-derive all of it | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-49"></a>49 | A capability ships and the standing instruction still directs everyone to the path it replaced | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-50"></a>50 | A brief describes the system from memory and prescribes a fix from that description, aiming the work before it starts | Verified for stated scope | [Brief provenance](#brief-provenance). Annotates selected unsupported accounts. A plausible prescription or a misread cited fact can still pass. |
| <a id="type-51"></a>51 | Satisfying one requirement means editing an unknown number of other files and nothing enumerates them | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-52"></a>52 | A measurement is correct and the word used for it is not, so a false claim rides in on evidence that is real | Verified for stated scope | [Brief provenance](#brief-provenance). Reports selected claim/command capability mismatches. Its command vocabulary is bounded and heuristic. |
| <a id="type-53"></a>53 | A check passes because execution never reached the thing it was trusted to test | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-54"></a>54 | A mutation of shared state leaves a log entry with no author in it, so the record looks answered when it is not | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-55"></a>55 | A detector catches every instance and can act on none, so the cost is always already paid by the time it speaks | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-56"></a>56 | A question arrives first and the account that follows is on the same subject, so the account is mined for the answer instead of read on its own terms | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-57"></a>57 | A check's own output is quoted as evidence about the world, when the check is the thing that is wrong | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-58"></a>58 | The reviewer writes the prescription and quoting him correctly is what puts it beyond challenge | Verified for stated scope | [Brief scope](#brief-scope). Requires retry design dispositions. It cannot determine whether the reviewer is correct or the disposition truthful. |
| <a id="type-59"></a>59 | A turn ends on a stop declaration that is false and the machine idles until someone restarts it | Verified for stated scope | [Stated actions](#stated-actions). Requires a declaration in selected stop scenarios. The truth of the declaration is still unverified. |
| <a id="type-60"></a>60 | Someone returns after a gap and is told what the lead is currently holding instead of the thing he left on | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |
| <a id="type-61"></a>61 | A deterministic lookup exists and takes seconds and the question is answered from memory instead | Unassessed | Relevant controls, intervention and evidence have not been audited for this entry. |

## Identity and updates

Type IDs are stable and are never renumbered to match a marketing list. A README
may present selected examples in a different order or omit most of this catalog.
The current marketing selection is 55 items; that is a presentation choice, not
a limit on recognized failures or implemented controls.

When an incident introduces a new type, allocate its ID through the maintained
register and add a matching entry here. When implementation changes, update the
assessment, the tested scope and the dated evidence. Preserve the distinction
between an old incident, an old coverage assessment and a current test result.
Do not infer a percentage of failures solved from this table.
