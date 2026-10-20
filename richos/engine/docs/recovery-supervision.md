# Axel and Wes recovery supervision

The host schedules both Axel and Wes at the witnessed start of the fourth actual round of one unresolved outcome. The initial attempt is round 1 and each corrective pass is the next round, including another correction inside the same worker; a refused dispatch, a status message or a retry adds none. Fix 3 is the last ordinary chance. A name suffix such as `agent-model-task4` is not a round. Both advisors are also scheduled when inexpensive triage judges a recorded work event to warrant scrutiny (see Event triage below). No failed review or blocking finding is required. Three distinct failed implementation handovers remain an additional Axel trigger. Wes receives explicitly disputed blocking findings, findings refused at landing and findings selected by the declared routing policy. Both run through the existing asynchronous review infrastructure. Evaluation runs outside the dispatch hook and overlaps the running round. Hard, probing questions come first. When summoned, ask Rich one or two short, context-specific questions that expose the assumption keeping the job going. Deliver your questions as soon as they are ready, independently of the other advisor. Do not first require a diagnosis, verdict or proposed fix and do not disguise an unsupported verdict as a question. Give Rich room to recognize the problem and correct course himself. That is a successful intervention without imposing your own recovery plan. If an answer is unsatisfying, ask a harder, specific follow-up. Only repeated unsatisfying answers after that follow-up justify stronger intervention. Silence, unrelated messages and missing evidence are not unsatisfying answers. Existing acceptance and review obligations still apply. Neither the round count nor a triage alarm ever authorizes a stop. The first model output contains questions only; each is delivered through the existing lead-notice path before answer assessment. Only new witnessed lead replies after acknowledged delivery can support a response assessment. A first unsatisfying answer gets a follow-up; a second distinct unsatisfying answer after that follow-up can authorize the existing exact-worker control path. A satisfactory reply allows Rich to correct course without an imposed advisor plan. A later actual round or a new material event triggers questions again. An alarm is never control authority over a worker or a fabricated recovery failure.

This feature is inactive without an entity-local the adopting entity's own `recovery.json` (in its `.claude` folder). Landing review acceptance remains independently active. Agent profiles alone do not activate recovery.

## Observation first

After installing the engine, an entity can declare observation for explicit repositories:

```json
{
  "schema": 1,
  "mode": "observe",
  "repositories": ["/absolute/path/to/governed/repository"],
  "wes_policy": "carried"
}
```

`carried` routes a blocking finding carried still-open through an independent correction review. `all` is the comparison policy for every new blocker. Explicit disputes and refused-landing routing remain available under either policy. Observation produces durable judgments and cost records but applies no recovery controls or recovery dispatch refusals.

Enforcement requires `mode: enforce` and an absolute `qualification` reference. The qualification record must have schema 1, status `qualified`, named evidence and the declared repositories. Its `results` must contain passed evidence for `historical_replay`, `axel_semantics`, `wes_routing`, `host_races`, `native_capabilities`, `observation_pilot` and `independent_review`. `source_pins` binds SHA-256 fingerprints for both canonical profiles, recovery state/runner/controls, Pierce, workspaces and the native desktop hook. Source drift refuses enforcement until evidence is reconciled. These records represent a reviewed rollout decision; a filled-in JSON file is not independent proof of its assertions.

The implementation's fixtures and restricted-provider read probe do not qualify complete native cancellation. A historical counter replay proves scheduling, not successful recovery. Keep enforcement inactive until the complete PRD qualification record is supported. The private delivery report records the actual gaps.

## questions-only

`mode: questions-only` runs event triage, both advisors and owner question delivery, with no prior qualification. Everything else follows `observe`: a recovery classification or schema failure is recorded as a gap and the dispatch is judged by Pierce's ordinary inspection alone, dispatch admission reserves nothing, no budget, hold or worker control applies, finding resolutions are not consumed and any proposed control is recorded only. Questions follow `enforce`: they reach the owner through the existing notice path and a later advisor judgment needs acknowledged delivery first. Ordinary brief, authorization and safety refusals stay effective. `off` stays inert and `observe` stays silent recording.

## Event triage

The terminal review watcher starts `scripts/lib/recovery_events.py` in its own process when a record it reads has changed. It reads only existing records: each governed worker's dispatch (its Agent call and result in the lead transcript), each successful lead SendMessage to that worker (joined to the send hook's `inflight-notices.jsonl` ledger by session, recipient, body hash and timestamp order) and each review result already written for its job. A send the hook did not record is still witnessed from the transcript and reported as a coverage gap. A refused Agent call adds no round and costs no triage call; a `spawn.sh` preflight refusal makes no Agent call at all.

Each new event gets one call of `scripts/recovery.sh triage --input <file>`, the single tool-free Haiku triage command shared with the app adapter (contract in `scripts/lib/recovery_triage.py`). Its decision is routine, inspect or uncertain; unavailable (provider failure, malformed answer, missing provider, quota hold, oversized input) routes to the same inspection as inspect. A dispatch reuses Pierce's or the app issuer's existing relationship; a send also gets its relation to the work round. A send classified as a correction counts as a round only once the worker's own transcript shows the message arriving and new work beginning. Results and round relations are stored beside the existing assignment as `recovery_events` on the worker's registry record; alarms go to the existing recovery job record and coalesce until both advisors have asked. Codex file handovers, lead-to-user prose and unregistered execution are not watched by this extension.

## Identity and controls

The desktop host binds implementation to its accepted obligation. Recovery-enabled terminal registration reuses the existing Pierce inspection to attest job kind and correction relationships against indexed original human words and native briefs. `lands-pending` is a merge dependency, not automatic evidence of one job. Legacy relationships require source-qualified reconciliation. Recovery history does not enlarge lifecycle ownership or cleanup.

A control binds the exact job, acceptance, evidence frontier and worker source, including uncommitted changes. A stale result requests a bounded update. The supervisor refuses subsequent tools for the affected worker, ends only birth-checked owned commands and observes in-flight completion. WIP preservation follows quiescence. A preserved patch and untracked-file archive remain in the resolved WIP directory: `RICHOS_RECOVERY_WIP_ROOT` when set, `state/richos/recovery-wip/` under a configured bulk root or `recovery/wip/` beside the review ledger by default; the original worktree remains available. Verify their manifest hashes before retaining or restoring work in an isolated replacement. No control kills the lead or a sibling merely because it shares a session.

Side effects disabled, owned commands ended and native harness cancellation acknowledged are separate recorded facts. The lead receives the exact native cancellation request. A registry `end` or a fabricated CEO acknowledgment cannot stand in for actual native completion. A missing acknowledgment remains an open control gap.

An accepted replacement plan includes a falsifiable diagnosis, disproof evidence, original acceptance matrix, proof for every exact matrix condition and retained valid evidence. A separate reviewer assesses it. Replacement metadata names `recovery-disposition: <plan id>` and, for an infrastructure replacement, `recovery-replaces: <worker key>`. Following ordinary workspace/continuation metadata, `recovery-implementation:` introduces the exact accepted implementation brief. Existing isolation gates apply; unsafe predecessor work cannot be silently reused.

The three failures that trigger investigation are separate from the three recovery attempts. After two failed recoveries, the third must use a materially different approach with independent assessment. A third failure persistently blocks a fourth correction and brings a durable escalation to the owner for Alex. Delivery to the owner is not proof Alex read it.

## Inspect and dispute

Use `scripts/recovery.sh status --job <job id>` to inspect a record. Use `scripts/recovery.sh dispute --repo <repository> --work <work key> --tip <commit> --finding <exact finding id>` to request applicability review. Repeat `--finding` for multiple exact blockers. Requests deduplicate identical input; they neither withdraw a finding nor authorize implementation.

State and append-only events live beside the existing review ledger under `recovery/`. Consultation inputs, schemas, outputs, process generations, bounded attempts and actual token/cost facts remain available there. Full original evidence is indexed in the resolved evidence directory: `RICHOS_RECOVERY_EVIDENCE_ROOT` when set, `state/richos/recovery-evidence/` under a configured bulk root or `recovery/evidence/` beside the review ledger by default. Missing lineage or oversized inputs are explicit gaps, not evidence that a qualification or exception never existed.

Turning operator lease fencing off requires a verified operator and `--reason`. It appends actor, authority, time, repository and setting to `operator-fence-settings.jsonl`. Review acceptance, recovery acceptance and current-operation checks remain active for reviewed repositories. Restoring an unreadable installed reader requires repairing the installation; switching lease fencing off does not repair acceptance.

Codex's current file-based handover participates in review and landing reconciliation. It has no equivalent native recovery dispatch issuer or interruption consumer in this implementation. Do not claim that boundary is enforced.

The runtime profiles are staged under `recovery-profiles/`, outside the live agent roster. The restricted runner reads them directly. Native agent installation and visibility require the R10 qualification step first.

The host can set absolute `RICHOS_RECOVERY_EVIDENCE_ROOT` and `RICHOS_RECOVERY_WIP_ROOT` paths. On this Mac the default remains the external SSD; other platforms default to private recovery state. A missing configured storage destination is an observation gap or an enforcement refusal. Observation preserves the baseline Pierce verdict when recovery classification, source freezing or identity reconciliation fails. It records the gap and applies no new tool restriction.

Disabling operator lease fencing deliberately follows the existing operator ownership policy. The verified desktop owner must perform it while the app owns the team; a terminal cannot use it to override that owner. An installation repair remains available through the ordinary installation workflow. The setting never disables acceptance or recovery gates.

`recovery.sh report [--job <id>]` derives firing and scheduling counts, consultation time, known cost, native control observations and missing measurements from retained records. Historical events without firing fields are reported as incomplete. A missing cost is never zero. This report is bounded to 4096 jobs and 16 MiB of job records; select a job for larger histories.

`recovery.sh contract --entity <entity> --installed-engine <engine>` checks the declared mode against the installed reader and compares runtime/profile fingerprints. The existing contract probe calls this for an entity that declares recovery. This verifies the mode/source contract; native capabilities still require their own terminal/app witnesses.

The installed contract probe needs an independent frozen reference for an adopted
observation declaration. Before deployment, run `scripts/recovery.sh
contract-snapshot --entity <absolute entity> --output <absolute new reference>`
from the reviewed source engine. Keep that reference outside the mutable engine
and set `runtime_contract` to its absolute path in the adopting entity's own `recovery.json` (in its `.claude` folder).
The snapshot does not qualify enforcement or native capabilities. The probe
compares installed bytes and declared mode against the frozen reference and
refuses an adopted engine comparing itself without one. Enforcement may use its
separate reviewed qualification source pins instead.

A concurrent review joins the shared source claim without taking a model worker
permit. Its wait uses the existing producer's finite admission and fallback
budget. After release, exact inputs reuse the recorded verdict. Changed inputs
still require an explicit recheck. Each completed/reused invocation emits a host
receipt naming its actual ledger row. The watcher validates that receipt against
repository, source, work, purpose and input identity before accepting an older
reused verdict. Missing or mismatched evidence remains a lost invocation.

Contradictory handover verdicts at one repository, work and source create one
durable outcome reconciliation task. The watcher requests a focused independent
recheck with the original review IDs and blocker IDs as evidence. This applies
through the existing review service even when advisor recovery is off. A recheck
can resolve the task while upholding its blockers; it does not grant acceptance.
The task does not loop after a finalized recheck. Missing verdicts retain the
watcher's existing two-loss bound. Explicit disputes remain available afterward.
