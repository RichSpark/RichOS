# Recovered work: `echo-opus-dr1`, orphaned 2026-09-08

Two committed commits from a teammate whose session died on 2026-09-08. They were
never merged and were not noticed for a day. They are preserved here in three
independent places so that no single cleanup can destroy them.

## Where the work lives

| Form | Location | Survives |
|---|---|---|
| Branch | `echo-opus-dr1` on `origin` | worktree reaping, local branch deletion |
| Tag | `orphaned/echo-opus-dr1-2026-09-08` on `origin` | branch deletion |
| Patches | the two `.patch` files beside this README, on `main` | everything, and readable without checkout |

## What it is

| Commit | Subject |
|---|---|
| `6c1b8a3c` | Tell the inner Rich a stored entry can be out of date, not just missing |
| `7ced3d00` | Hold the two new doctrine clauses with a test that bites |

Branched from `28f07ab53a3d`. Touches four files:

- `richos/app/crates/richos-core/src/doctrine.rs` — merges cleanly
- `richos/app/README.md` — **conflicts**
- Historical inner-doctrine file ([preserved changes](0001-Tell-the-inner-Rich-a-stored-entry-can-be-out-of-dat.patch)): **conflicts**
- `richos/app/crates/richos-core/tests/action_ledger_tests.rs` — **conflicts**

## Why it does not just merge

A merge was attempted on 2026-09-09 against `main` at `8af9365f` and aborted with
conflicts in the three files above. Main moved 47 commits past this branch's base
while the work sat unnoticed, and three of the files it edits were edited again on
main in that window. **The conflict is a consequence of the delay, not of the work.**
Had it been landed on the day it was written, it would have merged.

Resolving it needs an engineer with context on the doctrine text, not a mechanical
conflict resolution: two of the three conflicting files are load-bearing doctrine and
its test.

## To review it

```
git log 28f07ab53a3d..echo-opus-dr1
git diff 28f07ab53a3d..echo-opus-dr1
```

or read the two patch files here directly.

## How it was found

Not by any check. Its worktree kept blocking pushes from the in-flight guard, and the
block was waived three times before anyone opened the worktree to see what was inside.
The failure is recorded in `richos-hq/wiki/enforcement-and-failures.md`.
