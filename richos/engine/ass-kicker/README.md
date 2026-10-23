# ASS Kicker

**Asinine Slop & Stupidity Kicker: RichOS's agent reliability subsystem.**

AI agents can make confident claims without evidence, dispatch work outside the
requirements or describe an action without taking it. ASS Kicker brings together
existing checks on agent briefs and reported actions, their regression tests and
a growing [failure catalog with explicit coverage](coverage.md).

**This is a subsystem under development. The catalog is broader than the
implemented controls.** A documented failure is not a solved failure. The first
three control families cover specific, testable behaviors and leave substantial
gaps. Local verification and acceptance through a delivered nightly build are
reported separately in the [verification record](../../../docs/verification/ass-kicker-consolidation-2026-09-16.md).

## What is implemented

| Capability | Intervention | Boundary |
|---|---|---|
| Brief provenance | Annotates selected claims with missing or inadequate evidence, distorted quotations and certain misleading uses of measurements | Advisory. A valid citation does not prove a claim true. Heuristics have known false positives |
| Brief scope | Refuses certain dispatches against recorded specifications: missing anchors, satisfied requirements, stale verdicts, regressions and non-convergence. Requires applicable design declarations on retries | Only applies to spec-governed work with recorded state. A declared design can still be wrong |
| Stated actions | Refuses selected mismatches between the final report and tool activity. Requires a stop declaration for certain completion turns | Pattern-based coverage. It cannot establish whether the stop declaration is true or whether an artifact is correct |

The engineering loop is: observe an incident, identify the recurring failure,
implement a bounded intervention and verify that it catches the defect while
allowing legitimate work. Mutation tests deliberately remove parts of a control
and require the corresponding regression case to fail.

## Three concrete examples

- **An unnecessary round is dispatched after the requirements are satisfied.**
  The [scope suite](tests/brief-scope.test.sh) replays a
  [committed historical brief](fixtures/brief-scope/round9-brief-2026-09-13.md).
  It asserts refusal when the anchored point is green. A mutation removes that
  refusal and must make the historical case fail. The same suite records the
  remaining gap: a declaration that design is open can conceal a prescription.
- **A real measurement is described as proving something it cannot establish.**
  The [provenance suite](tests/brief-provenance.test.sh) tests the difference
  between Git reachability and claims about a particular act. Correctly scoped
  claims remain unflagged. A known mention-versus-use false positive is tested
  explicitly rather than hidden in a precision claim.
- **An agent reports that another worker is taking an action without dispatching
  or messaging that worker.** The [stated-action suite](tests/ass-kicker.test.sh)
  tests the narrow report patterns and benign alternatives through the Stop
  hook. The [historical corpus report](docs/stated-actions.corpus.md) records the
  original measurement. Its numbers are dated observations, not a new benchmark.
- **An agent claims something was recorded, saved or made a rule and nothing
  durable was written.** ARM 3 of the same Stop hook is an alert, never a
  refusal: one line in the lead's own context, nothing shown to the CEO. It is
  tested by the same suite (cases R1 to R33). The corpus report's
  ARM 3 section gives the replay counts; `tests/record-claims.replay.py`
  reproduces them from any directory of lead transcripts, printing counts only.

## Architecture

```text
Agent dispatch -> shared spawn orchestration -> provenance / scope
               -> existing PreToolUse hook -> scope decision
Record write   -> existing PostToolUse hook -> provenance capability notice
Turn ending    -> existing Stop hook -> stated-action analysis

Scope reads Mega Lander's existing integration records.
Stated-action analysis uses shared transcript and stop-declaration readers.
```

| Path | Responsibility |
|---|---|
| `brief-provenance.py` | Provenance annotation and the shared capability predicate |
| `brief-scope.py` | Specification, verdict and dispatch checks |
| `ass-kicker.py` | Analysis of reports and tool activity |
| `tests/` | Behavioral, mutation and relocation tests |
| `fixtures/` | Committed historical acceptance inputs |
| `docs/` | Existing corpus evidence |
| `coverage.md` | Stable failure IDs, current assessments and known gaps |

Platform hooks remain in [`../scripts/hooks/`](../scripts/hooks/).
Shared spawn orchestration remains in [`../scripts/lib/spawn.py`](../scripts/lib/spawn.py).
[Mega Lander](../mega-lander/README.md) continues to own workspace lifecycle and
integration state. Other engine controls can contribute to reliability without
belonging to this directory.

Existing command and import paths under `scripts/` forward to these canonical
implementations. Existing state locations, command arguments, environment
settings and hook registrations are preserved. No new service or state store is
required. The current adapters target Claude Code; this move does not establish
support for other agent platforms.

## Reproduce the checks

From the engine directory, with Python 3, Bash and Git available:

```sh
bash ass-kicker/tests/brief-provenance.test.sh
bash ass-kicker/tests/brief-scope.test.sh
bash ass-kicker/tests/ass-kicker.test.sh
bash ass-kicker/tests/stated-actions.mutation.sh
bash ass-kicker/tests/relocation.test.sh
bash scripts/hooks/notice-claim-capability.test.sh
bash scripts/hooks/claim-capability-delivery.mutation.sh
```

The scope suite includes its mutation harness. Set `RICHOS_MUTATION_INNER=1`
only when deliberately running behavioral checks separately from mutations.
`RICHOS_MUTANT_JOBS` bounds mutation concurrency. The stated-action self-test
also remains available as `scripts/hooks/ass-kicker.sh --self-test`.

Tests construct temporary repositories and state. The required acceptance
fixture is committed here. An optional comparison with its original source
prints `NOT RUN` when that source is unavailable; it is never counted as a pass.
The historical transcript corpus is not needed for the regression suites.
These commands run locally and do not enable CI or create nightly builds.

## Extending coverage

Add a new control only with a named failure, a reproducible case, a defined
intervention and tests for both the defect and legitimate behavior. Update the
coverage entry with the tested scope and evidence. Keep advisory behavior
distinct from blocking behavior. Broad semantic failures may remain unresolved
even when a narrow signature can be caught reliably.
