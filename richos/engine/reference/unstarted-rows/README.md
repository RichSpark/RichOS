# `.unstarted-rows` — writing the work down is not starting it

A Stop hook that names the open rows with no running work recorded, once per
change of that set, so none is silently forgotten. It states only that: a
missing branch, row directory or claim is missing evidence, not proof that
nothing is running or that the row may start now.

## The defect

A lead finds work, writes it down as a row and stops — because writing the row
satisfies the same urge as doing the work. On 2026-08-31 that happened three
times in one day in this project: an emptied table announced as an empty
backlog while a section titled *"Buildable now — nobody blocked"* still held
open rows; four defects found, four rows opened, one dispatched; and a row
described to the CEO as "being built now" that nobody had been asked to build.

The prose fix failed first. After the first failure the lead amended the
queue's own header to say the queue is two files and an empty table is not an
empty queue. That text was still there, in its own words, when the second and
third happened the same afternoon.

> A RULE ENFORCED BY ATTENTION LASTS EXACTLY AS LONG AS THE ATTENTION.

## Why the turn end and not the land

There was already a rule on the land: start the top unblocked item before
reporting. It caught none of the three and the reason is structural. **Work is
created when something is NOTICED and noticing happens while writing a
report.** By the time a land ends, the noticing turn is over — and the third
failure involved no land at all.

## What it reads

Two files as ONE corpus, because that is the record's own corrected rule:

| | |
|---|---|
| the queue | `RICH-TODOs.md` — a table whose last column is `Blocked by` |
| the working record | the `.ceo-todos` `TODO_RECORD`, section `PREPARER_SECTION` |

A **half corpus is BROKEN**, never a clean sweep of whichever half survived.
That single branch is the mechanized form of the sentence the lead wrote and
then did not follow.

## How a row declares that it is waiting on somebody

Two shapes, one meaning:

```
| c2 | **femcboost cannot deploy** ... | **CEO — his credentials** |

| 3.3d | **... OPEN FOR THE CEO.** **Blocked:** the CEO — a ruling, not an
         implementation. | **State:** `OPEN` — `docs/x.md`@`41a9578ed2cf` |
```

The queue's `Blocked by` cell already existed and already carried the fact — on
the day this shipped, all four genuinely-CEO rows were silent under this rule
with no edit to anything. The section's `**Blocked:**` is the same construct
wearing markdown; it goes in the row's prose, before the `**State:**` warrant,
and the row-currency contract never sees it.

**No new warrant token, deliberately.** `.row-currency` declares the vocabulary
and refuses an unknown one by design, so a new token would mean editing an
enforced contract to add a word meaning "not enforced here" — and the state of
the WORK and who it WAITS ON are two facts that must not share one slot.

## How a row is claimed

A live worktree, across every repository the record's own `ARTIFACT_ROOTS`
names — derived, never typed:

* a branch or directory containing `row-<id>` (`row-11`, `row-3.1`, `row-3-1`),
  bounded on both sides so `row-1` never claims row `11` and `row-11` never
  claims row `1`;
* or the id on its own line in `<worktree>/.claude/row-claims.txt`.

An unclaimed row is reported. That direction is deliberate: a fuzzy match would
fail toward silence and silence is the failure this exists to prevent.

## Loud, never green over an empty set

Either record absent, empty or unparseable; the `Blocked by` column renamed or
duplicated; zero rows on either side; a warrant token outside the declared
vocabulary; one id in both files; the predicate itself missing; or a queue that
was swept before and has since vanished. Each of those announces; none of them
reports a clean queue.

And the sweep writes a **receipt** on every path
(`.claude/state/unstarted-rows/last-sweep.txt`), so a silence can always be
told apart from a check that never ran.

## Running it by hand

```
~/.claude/richos-engine/scripts/unstarted-rows-lint.sh <repo>
```

`0` everything is accounted for · `1` something is unstarted, each named ·
`2` nothing was read — never the same code as clean.

## Files

* `unstarted-rows.example` — the optional declaration, fully commented
* `scripts/hooks/notice-unstarted-rows.sh` — the Stop hook
* `scripts/lib/unstarted-rows.sh` — the argument and the wiring
* `scripts/lib/unstarted-rows.py` — the predicate
* `scripts/hooks/unstarted-rows.test.sh` — 33 checks
* `scripts/hooks/unstarted-rows.mutation.sh` — 33 mutants, one per check
