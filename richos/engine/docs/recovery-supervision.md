# Axel and Wes recovery supervision

The host schedules Axel after three distinct failed implementation handovers for a tracked job. Wes receives explicitly disputed blocking findings, findings refused at landing and findings selected by the declared routing policy. Both run through the existing asynchronous review infrastructure. Ordinary work continues while evaluation is pending.

This feature is inactive without an entity-local `.claude/recovery.json`. Landing review acceptance remains independently active. Agent profiles alone do not activate recovery.

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

## Identity and controls

The desktop host binds implementation to its accepted obligation. Recovery-enabled terminal registration reuses the existing Pierce inspection to attest job kind and correction relationships against indexed original human words and native briefs. `lands-pending` is a merge dependency, not automatic evidence of one job. Legacy relationships require source-qualified reconciliation. Recovery history does not enlarge lifecycle ownership or cleanup.

A control binds the exact job, acceptance, evidence frontier and worker source, including uncommitted changes. A stale result requests a bounded update. The supervisor refuses subsequent tools for the affected worker, ends only birth-checked owned commands and observes in-flight completion. WIP preservation follows quiescence. A preserved patch and untracked-file archive remain under `/Volumes/E1TB/state/richos/recovery-wip/`; the original worktree remains available. Verify their manifest hashes before retaining or restoring work in an isolated replacement. No control kills the lead or a sibling merely because it shares a session.

Side effects disabled, owned commands ended and native harness cancellation acknowledged are separate recorded facts. The lead receives the exact native cancellation request. A registry `end` or a fabricated CEO acknowledgment cannot stand in for actual native completion. A missing acknowledgment remains an open control gap.

An accepted replacement plan includes a falsifiable diagnosis, disproof evidence, original acceptance matrix, proof for every exact matrix condition and retained valid evidence. A separate reviewer assesses it. Replacement metadata names `recovery-disposition: <plan id>` and, for an infrastructure replacement, `recovery-replaces: <worker key>`. Following ordinary workspace/continuation metadata, `recovery-implementation:` introduces the exact accepted implementation brief. Existing isolation gates apply; unsafe predecessor work cannot be silently reused.

The three failures that trigger investigation are separate from the three recovery attempts. After two failed recoveries, the third must use a materially different approach with independent assessment. A third failure persistently blocks a fourth correction and brings a durable escalation to the owner for Alex. Delivery to the owner is not proof Alex read it.

## Inspect and dispute

Use `scripts/recovery.sh status --job <job id>` to inspect a record. Use `scripts/recovery.sh dispute --repo <repository> --work <work key> --tip <commit> --finding <exact finding id>` to request applicability review. Repeat `--finding` for multiple exact blockers. Requests deduplicate identical input; they neither withdraw a finding nor authorize implementation.

State and append-only events live beside the existing review ledger under `recovery/`. Consultation inputs, schemas, outputs, process generations, bounded attempts and actual token/cost facts remain available there. Full original evidence is indexed under `/Volumes/E1TB/state/richos/recovery-evidence/`. Missing lineage or oversized inputs are explicit gaps, not evidence that a qualification or exception never existed.

Turning operator lease fencing off requires a verified operator and `--reason`. It appends actor, authority, time, repository and setting to `operator-fence-settings.jsonl`. Review acceptance, recovery acceptance and current-operation checks remain active for reviewed repositories. Restoring an unreadable installed reader requires repairing the installation; switching lease fencing off does not repair acceptance.

Codex's current file-based handover participates in review and landing reconciliation. It has no equivalent native recovery dispatch issuer or interruption consumer in this implementation. Do not claim that boundary is enforced.

The runtime profiles are staged under `recovery-profiles/`, outside the live agent roster. The restricted runner reads them directly. Native agent installation and visibility require the R10 qualification step first.

The host can set absolute `RICHOS_RECOVERY_EVIDENCE_ROOT` and `RICHOS_RECOVERY_WIP_ROOT` paths. On this Mac the default remains the external SSD; other platforms default to private recovery state. A missing configured storage destination is an observation gap or an enforcement refusal. Observation preserves the baseline Pierce verdict when recovery classification, source freezing or identity reconciliation fails. It records the gap and applies no new tool restriction.

Disabling operator lease fencing deliberately follows the existing operator ownership policy. The verified desktop owner must perform it while the app owns the team; a terminal cannot use it to override that owner. An installation repair remains available through the ordinary installation workflow. The setting never disables acceptance or recovery gates.

`recovery.sh report [--job <id>]` derives firing and scheduling counts, consultation time, known cost, native control observations and missing measurements from retained records. Historical events without firing fields are reported as incomplete. A missing cost is never zero. This report is bounded to 4096 jobs and 16 MiB of job records; select a job for larger histories.

`recovery.sh contract --entity <entity> --installed-engine <engine>` checks the declared mode against the installed reader and compares runtime/profile fingerprints. The existing contract probe calls this for an entity that declares recovery. This verifies the mode/source contract; native capabilities still require their own terminal/app witnesses.
