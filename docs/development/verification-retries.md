# Verification: unresolved failures first, preserve valid results

This procedure is mandatory for implementation agents, reviewers and
orchestrators. Its purpose is to obtain the required evidence without repeatedly
spending the user's time on unchanged passing work. It supplements the existing
proof selector, runner, resource controls and receipt verifier.

## 1. Before launching any verification

Inspect the existing run directory, progress, summary, logs and receipts. Record:

- The exact selected plan and why each relevant suite was selected.
- The tested commit, tracked diff and untracked-source fingerprint.
- Passed, failed, timed-out, refused, cancelled and unrun units separately.
- The relevant toolchain, dependencies, fixtures, platform and execution settings.
- Which results remain applicable and the exact unresolved evidence gap.

Start from the smallest unresolved unit. A new agent, context compaction,
handoff, user status question or interrupted parent process does not by itself
invalidate completed passing work. Do not start a fresh full run just because
you have lost track of the old one; recover its evidence first.

Keep the existing optimized defaults. Inspect inherited environment variables as
well as command-line flags. In particular, do not silently set
`RICHOS_MUTANT_JOBS` or `--engine-shards`. A CPU-admission refusal is not evidence
that the test code failed or that the machine is inherently slow. Inspect the
recorded admission reason, CPU samples, worker leases and owned processes before
choosing a remedy. Preserve CPU limits, shared worker budgets and build locks.

The proof runner also takes the host-wide proof-run slot, which defaults to one
run at a time. `--slot-wait` bounds that admission wait. The separate
`--engine-slot-wait` bounds the large engine-plan slot wait; leaving it unset
preserves that gate's default. Neither option changes execution deadlines.
The runner and receipt verifier suppress Python bytecode writes so verification
does not alter its own declared source inputs. Existing bytecode remains part
of the input identity and changed inputs still invalidate evidence.

Commit quick/fast checks and nightly commands share
`richos/app/scripts/lib/owned_command.py`. Their execution hang guards start
when the machine worker admits the command. Admission refusal is reported as
no execution, never as a failed assertion. Installed browser identities include
recursive file metadata, including ctime, so edits and re-signing in place
invalidate saved evidence. This detects ordinary input drift; it does not
authenticate a browser download. Keep installed browsers as downloaded.

The app load-site baseline remains a ratchet. Engine test sites are compared
with committed integration source under the same rules, at commit and merge;
the push backstop compares with the pushed range's base. Existing engine sites
are debt, not findings that every one is defective. New sites, duplicates and
multiline timing assertions refuse. Use controlled time for a timing verdict
or wait for the fact. A declared hang guard must explain what proves correctness.

## 2. Before any retry

Run failed, timed-out, refused or previously unrun units before repeating
unchanged passing units. Inspect the failing unit's own log, including nested
suite output; a wrapper's failure label is not a diagnosis.

Classify the failure accurately:

| Outcome | Next action |
| --- | --- |
| Assertion failure | Diagnose the assertion, fix the cause and rerun its owning unit first. |
| Deadline reached | Inspect actual progress and concurrency overrides; retry the unit alone with normal parallelism before widening scope. |
| Admission refused | Resolve or wait for the recorded resource condition; retry the refused unit first. |
| Interrupted run | Preserve finished receipts and identify exactly what never reached a verdict. |
| No screen or missing device | State the missing coverage. Do not turn a successful wrapper exit into a test pass. |

Do not make several speculative execution changes between attempts. A necessary
non-default setting needs a recorded observation, the setting before and after,
the expected effect and the result of a bounded experiment on the affected unit.
Timeouts must remain finite. Do not remove resource guards or reduce assertions
to obtain green output. Longer admission waits and unit deadlines are different
controls and must not be confused.

Before a broad rerun, write a short decision record containing all of:

1. The unresolved unit and the result of its isolated retry.
2. The source, dependency, fixture, toolchain or relevant environment change that
   invalidates earlier results, or the specific gate requiring a fresh run.
3. Why applicable receipts cannot cover the required plan.
4. The exact command, inherited overrides and measured runtime basis.

This is the agent's decision record, not a new user-approval step. Do not make
the user manage test ordering, diagnose the runner or approve routine checks.

If the record has no concrete invalidation or fresh-run requirement, do not
restart the broad selection. User impatience is not a reason to skip proof;
it is also not a reason to discard valid proof and start again.

Do not leave a known failing prerequisite at the end of a long queue. If a run
must be stopped or reordered, use its recorded process ownership and cleanup
mechanism. Preserve evidence first. Do not select processes to kill by name or
path, touch another task's processes or call cancellation a pause.

### Mutation witnesses and full-suite results

The CEO-todos, prompt-verification, unstarted-row and row-currency mutation
harnesses run the full outer suite once. Each mutant then runs its original
named assertion or required fixture group, first against unmodified source and
then against the mutation. Keep those controls and the full outer coverage.

Their internal `--mutation-case` mode requires `RICHOS_MUTATION_INNER=1` and
returns 3 when its selected witness passes. That result is not a full-suite pass
and cannot satisfy an engine-unit receipt. Unknown scopes are errors. Use the
normal no-argument unit invocation for complete coverage.

## 3. Before accepting reused results

Reuse is evidence validation, not optimistic caching. Check source identity and
the relevant inputs and execution conditions. A matching commit alone does not
prove a dirty checkout, changed dependency, toolchain or external fixture was
unchanged. If identity cannot be established, rerun the affected units.

The engine already supplies `scripts/lib/ci-receipts.py verify`. For an unchanged
tested commit, keep one actual passing receipt per planned unit, replacing only
the unresolved unit with its successful retry. Retain the superseded failure and
record the provenance of every selected receipt. Never manufacture a verdict,
edit its SHA or overwrite the failed run's summary.

Verify the selected receipt set against the original plan. The existing verifier
rejects missing, duplicate, unplanned and non-green receipts. Mixed-commit receipts
require target provenance; the legacy invocation still rejects them. For non-engine checks,
preserve their full-run summary and logs and verify their actual scope and status.
For a run with `plan.json` and `outcomes.json`, use
`python3 richos/app/scripts/proof-run.py --resume <saved-run-directory>` to
restore its exact plan. It creates a separate attempt, prioritizes unresolved
checks and copies validated logs and receipts without rewriting their SHAs.
Exact resume requires the original checkout and frozen plan. For a newly selected
target, including another checkout or commit, use
`python3 richos/app/scripts/proof-run.py --reuse <author-run-directory> <target-range>`.
`--reuse` may be repeated for more than one prior run. Each reused obligation must
have identical qualified inputs and command. An unrelated commit does not itself
invalidate its result. Missing qualification, changed inputs or damaged evidence
requires execution with the reason recorded. Live known-red declarations are
evaluated again and are never reused.

The nightly's workspace mutation batches use this same runner and its per-unit
receipts. An interrupted or failed batch retains completed, validated units;
the next request executes only obligations without applicable evidence. The
nightly gate still requires verified coverage of its complete mutation plan.
Mutation passes remain off in the merge gate.

The runner supplies target provenance to `ci-receipts.py verify --proof-run <target-run>`.
The verifier independently recomputes target inputs and checks saved outcomes,
log/receipt hashes and original receipt commits before accepting mixed-SHA coverage.
Changing receipt SHAs is never valid. Older runs without saved input qualifications
still need execution or the explicit same-commit reconciliation described above.
The mechanism does not qualify a suite automatically: production execution recipes
and their per-unit declarations remain necessary before reuse is available.

Qualified inputs are also discovered automatically across this repository's
worktrees. One kernel-held lock owns each identical execution identity. A second
request joins that owner without occupying a worker permit; independent checks
remain eligible. After cleanup the owner publishes its existing evidence, which
the joiner validates through the same reuse and coverage paths. A damaged or
rotated artifact is a cache miss, never a pass. Fresh checks still execute.

The input owner record retains behavioral failures across new run directories.
After diagnosing an unchanged failure, pass `--retry-reason "<diagnosis>"` for
its one permitted retry. Two failed attempts require a relevant input change;
renaming the run or supplying another explanation does not reset the count.
Admission refusal, deadline and infrastructure failures do not spend this
behavioral budget. Their distinct recovery policies still apply. A corrupt retry
record refuses execution rather than silently clearing its history.

If a required gate explicitly demands a fresh single invocation, preserve that
requirement and run it after resolving the blocker. A request for full coverage
alone does not require discarding applicable passing evidence. If only a
reconciled result exists, say that plainly; never claim the original command
exited 0 when it did not.

## Communication and completion

Separate implementation completion from verification completion. State the actual
unresolved blocker and why the next command is needed. Base estimates on observed
durations and disclose uncertainty. Do not blame hardware without measurements
or hide execution overrides behind a generic statement that tests are slow.

When a unit exceeds its measured duration or stops showing progress, inspect its
own log and execution settings before simply raising a timeout. Repetitive
progress counts do not replace diagnosis.

Stop running tests once the required evidence is complete. Do not broaden or
repeat testing without a new change, failure or unresolved coverage concern.
For documentation-only follow-ups, check the changed instructions, links and
formatting, plus a directly affected rendering/provisioning check if necessary.
Do not replay an unrelated application or mutation suite.

The final report must identify the tested code, plan, reused evidence, retry
results, skipped coverage and the distinction between a single clean run and
reconciled verification. Preserve useful evidence before deleting owned scratch.

## Enforcement boundary

These are mandatory agent instructions. The existing coverage verifier and
resource guards provide mechanical checks for their specific invariants. This
runner adds durable exact-plan retry, unresolved-first scheduling and automatic
reuse/joining for qualified input recipes. It does not add a universal command
interceptor. Do not claim that instructions alone make
recurrence technically impossible. A future mechanical change must preserve
source/input identity checks, truthful failures and required coverage.

## Cost review

Managed execution records actual CPU and elapsed cost beside its supervision
result. Qualified input recipes use a comparison identity that retains the
command, toolchain, platform and execution settings across source changes. The
original successful observation stays the baseline; slower later passes cannot
silently raise it. The history retains that baseline and twenty recent costs.

`COST GROWTH` means a comparable metric increased by more than 20% and at least
10 seconds. `COST REVIEW` also exposes elapsed growth when host load is missing
or different. That uncertain comparison is not proof of a code regression.
The next attempt reports unresolved growth again before launch. Inspect the
linked history, original result, actual inputs and load before repeating work.
Fix the cost, reuse applicable evidence or qualify the changed execution inputs.
A functional pass alone does not resolve a performance regression.

Unqualified recipes and interrupted attempts cannot establish a baseline.
Queue time, completed execution and repeated attempts remain separate in the
runner report. The cost warning does not relabel a passing assertion as failed
or turn an unresolved performance target into completion.
