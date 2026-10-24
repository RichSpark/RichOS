# The mechanical-findings sweep — a defect the tree can show you becomes a row

**Status:** BUILT 2026-09-02. Inert in the session that lands it: hooks snapshot at
session start.

## The failure this closes

On 2026-09-02 an audit found eight real defects in twenty minutes that six weeks of
attention had missed. One of them — the only automated suite over the "no wrong numbers"
data contract, red and skipped in CI since 2026-07-19 — sat under a comment reading
*"Tracked separately"*, tracked in none of the three queue files.

The audit ran because the CEO asked. Its findings became rows because the lead typed
them. The finding that had been known for six weeks was a person's memory, seen from the
outside. Three links were missing and each is plumbing:

1. **Nothing triggered the audit.** Automating the search while the CEO stays the
   trigger moves the babysitting; it does not remove it.
2. **A finding did not become a row.** The lead wrote each row by hand, so a finding
   survived exactly as long as he remembered it.
3. **A row did not start itself.** That link already had an instrument —
   `notice-unstarted-rows.sh` — and it was standing down in the seat that matters,
   with a receipt saying so that nobody read. See "Two defects found in the third
   link" below.

## What it is

Three files and one hook, all under the engine:

| file | job |
|---|---|
| `scripts/lib/mechanical-findings.py` | the sweep and the writer: reads each declared artifact root at HEAD, produces findings with a stable identity, appends rows |
| `scripts/lib/mechanical-findings.sh` | resolution (which record, which roots) and the receipt |
| `scripts/hooks/notice-mechanical-findings.sh` | the `Stop` hook: sweep, write, say one sentence |
| `scripts/mechanical-findings-lint.sh` | the same code by hand; `--write` does what the hook does |

It reads the record's own declared artifact roots (`ARTIFACT_ROOTS` in the record
repository's `.ceo-todos`) at HEAD of their main checkouts. Never a typed list, never the
working copy.

## The three classes — mechanical only

Every class is a statement a machine can make about bytes with no opinion attached.
Whether the defect matters is never decided here.

| class | the fact | the 2026-09-02 instance |
|---|---|---|
| `ci-excluded-suite` | a test file's basename appears on a non-comment line of a CI workflow within eight lines of the word "skip" | `client-data-check.test.sh` under `if [ "$name" = … ]; echo "SKIP"` (Tom's F2) |
| `unrun-harness` | a `*.mutation.sh` is named on no non-comment line of any other script or workflow in its repository | seven of thirteen engine harnesses (Tom's F5), reproduced exactly |
| `untested-hook` | a hook registered in `hooks/hooks.json` or `.claude/settings*.json` that no test file names on a non-comment line | none today; the class exists for the next hook wired with no test |

A declared exemption is honored and reported, never silent: `finding-exempt: <reason>`
on the workflow line (or within the window) or anywhere in the harness or hook itself.
A bare marker exempts nothing — the reason is the declaration, the same discipline
`dialect-guard.sh` uses.

## Identity — the same defect three times is one row

A finding's key is `<class>:<prefix>/<path>` and it is written **into the row** as
`` `finding:<key>` ``, where a reader can see what the machine thinks the identity is. The
next sweep looks for it before writing anything. A row carrying the key, in any state, is
never written again. Three things follow, each reported and none acted on:

- **KNOWN** — the row exists and is open. The unstarted-row sweep names it every turn.
- **GONE** — the row is open and the sweep no longer produces its finding. The row is
  named so a person closes it; it is not touched.
- **CLOSED-BUT-PRESENT** — the row says CLOSED and the finding is still in the tree.

## The row it writes

In the record's own format, with a warrant minted by `row-currency.py`'s own
`identity()` — the code the landing guard reads — so the row is born current and goes
stale by the rules every hand-written row lives under:

```
| 3.21 | **A mutation harness is run by nothing: `richos/engine/scripts/hooks/ceo-asks.mutation.sh` is named on no non-comment line of any script or workflow in its repository.** The properties it proves load-bearing are proven only when somebody runs it by hand; a runner that discovers suites by one glob never sees it. Written by the mechanical sweep (`notice-mechanical-findings.sh`) on 2026-09-02, not by a person: it is a fact about the tree at HEAD, not a judgment about importance. `finding:unrun-harness:richos/engine/scripts/hooks/ceo-asks.mutation.sh` | **State:** `OPEN` — `richos/engine/scripts/hooks/ceo-asks.mutation.sh`@`6f0c1a2b3d4e` |
```

Rows are appended at the end of the governed section's table, ids allocated above every
numeric id already there, under a lock in `~/.claude/state/mechanical-findings/` (outside
every repository, because two seats can reach one record). No existing row is ever edited,
re-stamped, closed or deleted.

## Why the turn end and what it cannot see

Every defect it finds is introduced by a land and a land is a Bash call inside a turn.
`Stop` runs at the end of the turn that landed it. A `PreToolUse` guard at the land would
have to block to matter and a guard that blocks on a coverage fact is the guard that gets
waived — the two waiver ledgers held 251 entries on the day this was written.
`SessionStart` would find the defect tomorrow.

Named blind spots, not discovered later:

- **A defect not yet landed.** The sweep reads main.
- **A repository the record does not declare.**
- **Anything needing a judgment or a run**: a pass condition that is also the
  failure-to-run condition (Tom's F1, F3, F7, F8), a suite that is red, a hook whose
  only test is a copy list in an omnibus suite (Tom's F4 — "named by a test" is not
  "tested by" and no mechanical rule separates them without a naming convention).
- **A waived guard.** `notice-waiver-repetition.sh` produces the fact, but its ledgers
  live outside every tree, so a row about one could carry no warrant. The row would
  have to pin the guard that owns the hatch, which that analyzer does not yet name.
- **Whether a finding matters.**
- **A turn in a session that did not load this hook.**

## Two defects found in the third link

Both found while connecting to `notice-unstarted-rows.sh`, both fixed in the same
landing:

1. **The seat.** The operator's sessions are seated in `femcboost`, which had no
   `.row-currency`. Its receipt read `verdict: STOOD-DOWN` at every turn end since the
   hook was built. Fixed by the peer-form `.row-currency` in `femcboost`, which also
   switches on `guard-row-currency-commits.sh` for its landings.
2. **`**Blocked:** nothing`.** Rows 3.19 and 3.20 of the real record each read
   `**Blocked:** nothing — buildable now, nobody blocked.` and were classified as
   DECLARED — silent — because the construct's presence was taken as a declaration
   without reading what it declared. `unstarted-rows.py` now judges a `**Blocked:**` by
   its first word, as the queue's `Blocked by` cell already was. Six rows of the real
   record surfaced the moment it was fixed.

## Running it

```
scripts/mechanical-findings-lint.sh <repo>            sweep and report; write nothing
scripts/mechanical-findings-lint.sh <repo> --write    do what the hook does
scripts/hooks/mechanical-findings.test.sh             41 cases, two-sided per class
scripts/hooks/mechanical-findings.mutation.sh         every property shown load-bearing
```

Exit 1 from the lint means something is new, gone or contradicted; exit 2 means nothing
was read. They are never the same code.
