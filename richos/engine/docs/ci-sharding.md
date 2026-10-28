# The full verification pass runs where nobody is waiting for it

**2026-09-10.** `engine-self-verify` was a single job that took **123 minutes** and had not
fired on a push since 2026-09-01. This document is why it now fires on every push, what had
to exist first and what each measurement here was measured with — every number below carries
the command or the run id that produced it.

> A read of the source tells you what a script says. Only a run tells you what it does.
> Where the two disagree, the run wins. — `ci-portability-notes.md`

## The state that had to be changed, measured rather than recalled

| Fact | How it was obtained |
|---|---|
| The workflow object is **`active`**, id 345224407 | `gh workflow list --all --repo <owner>/richos` |
| Only `push:` and `pull_request:` were missing from the YAML | read of the file at `4f01e5c7` |
| **95 of 98** pushes to `main` in the event feed had **no run at all** | `scripts/ci-run-record-check.sh --repo <owner>/richos --workflow engine-self-verify.yml` |
| The last full pass took **123.5 min** and was **117/121** | run `34396549904`, 2026-09-09, `workflow_dispatch` at `1bd9f688` |
| Four units were red in it | that run's own summary |
| **101 of 120** suites were created in the previous twelve days, 35 in one day | `git log --diff-filter=A` over `*.test.sh` |

The last row is the one that decided the design. **The problem is inflow, not a red list.** At
roughly eight new suites a day, none of them ever run on Linux before landing, fixing today's
four buys exactly one green run.

## What exists now

### 1. A fast gate that matches the inflow — `affected`

`scripts/ci-affected-units.sh` maps a diff to the units it can affect and the `affected` jobs
run those, sharded. A guard-plus-suite diff is two to five minutes, on Linux, on the push that
introduces it — so a new suite gets its first Linux execution from the person who wrote it,
while they still have the context to fix it.

The mapping is four rules, in order: the path IS a suite; a sibling `<stem>.test.sh` exists; a
suite NAMES its basename (`grep -lF`); the sectioned suite's individual SECTIONS name it.

**The third rule carries the mapping and its premise was verified before being relied on:**
all 96 files under `scripts/hooks/` are named by at least one suite. That is not left as a
remembered fact — `ci-affected-units.test.sh` case **A5** re-derives it on every run and fails
the moment it stops being true.

**An unmapped executable is a failure and that is the point.** The dangerous case is not a
diff that maps to too much; it is one that maps to NOTHING, because the gate then certifies
the push against an empty set. A changed `.sh`/`.py` that no suite names is named and, under
`--strict`, fails. The remedy is never "add it to an exclusion list here" — the list would
BE the untested surface. Give it a suite or make a suite name it.

**The worst case was stated and then the first real push produced it.** A change to the shared
fixture at the top of `contract-integrity.test.sh` maps to all 24 of its sections; the commits
in this very branch touch `contract-integrity-probe.sh`, which that suite's preamble names and
they pull in `reconcile-terminal-worktrees.test.sh` at 940 s besides. Run `34422722682`, pushed
2026-09-10, selected **54 units costing ~100 minutes serial** — and the `affected` job was one
job with a 60-minute timeout. It would have been killed.

**So the affected set is sharded too, by the same planner.** `ci-units.sh --units-file` restricts
the inventory to a set of ids and packs exactly those; `ci-shard.sh --units-file --shard i/N`
runs shard *i* of it. Same discovery, same weights, same longest-first packing, same
determinism — the diff-scoped gate and the full pass are one mechanism at two scopes rather than
two that have to be kept in step. The same 54 units now pack into twelve shards whose longest is **the largest
indivisible unit in the set and nothing more**, so the packing is not the limit — which is the
property that matters and does not move when a weight is re-measured.

The matrix carries only shards that HAVE units, so a two-unit diff is two small jobs rather than
twelve, ten of which would have nothing to run — and a shard that verifies nothing must never
exit 0.

**An id in the restriction that is not a unit is FATAL** (`ci-units.test.sh` case U13). A
restriction that silently dropped one would plan over less than it was asked for and still exit
0, which is this document's whole subject wearing a diff filter.

`affected-coverage` then proves the affected shards covered exactly the set the diff selected —
checked against the RESTRICTED plan, never the whole inventory, which would report 94 false
absences and teach everyone to ignore the job. `ci-shard.test.sh` case S17 checks both
directions: the restricted plan certifies and the same receipts against the whole inventory
correctly fail.

**`affected-coverage` is the job to mark required**, not `affected`. It is the one that both
waits for every affected shard and proves the union and it is the only one of the two that
still reports on a docs-only diff, where `affected` is correctly skipped and the absence of a
required check would otherwise leave the commit ungated.

**A NEW CHECK NEEDS NO EDIT HERE and that was tested rather than claimed.** On 2026-09-10 the
lead asked whether the changed-paths job wanted the cleanup-routing contract script,
which had just landed as a standalone script with its own suite (both were removed with the
old workspace machinery on 2026-09-11; the measurement below is kept as it was taken). The answer is no and the
answer is the design:

```
$ richos/engine/scripts/ci-units.sh units | grep -c cleanup-routing
1
$ richos/engine/scripts/ci-affected-units.sh --paths engine/scripts/cleanup-routing-contract.py
scripts/cleanup-routing-contract.test.sh
scripts/lib/worktree-transactions.test.sh
```

The suite became a unit the moment it landed, because the inventory is discovered from disk; and
a change to the script selects it twice over, by the sibling rule and by the basename rule.
Adding a bespoke workflow step would be a typed inventory entry — the one object this engine
refuses to keep — and it would have to be re-added for every future standalone check.

### 2. The full pass, sharded, with its coverage PROVEN

`scripts/ci-units.sh` derives the inventory; `scripts/ci-shard.sh` runs a shard of it.

**A unit is not a suite.** Sharding buys nothing past the largest indivisible unit and one
suite dominated everything:

| unit | Linux, run `34396549904` |
|---|---|
| `scripts/hooks/contract-integrity.test.sh` | 2335.9 s (39 min) |
| the reconciler's suite (removed 2026-09-11) | 939.7 s (16 min) |
| the other 119 suites together | 4133.4 s |

`contract-integrity.test.sh` already takes `--only <section>` and carries 24 sections, so it
contributes **24 units** instead of one. That moves the floor from a 39-minute suite to the
largest unit that cannot be divided further — a quarter of an hour rather than two hours and a
verdict somebody will wait for. Twelve shards sit below that floor already, so **more shards
cannot improve the wall clock**; only making a big unit divisible can.

**A scoped section is green at exit 3 and its exit 0 is a FAILURE.** That suite exits 3 when
scoped-and-green precisely so nothing can read a partial run as a full one. The shard runner
therefore expects 3 — and treats 0 as `SCOPE-LOST`, because 0 from a scoped invocation means
the `--only` selector did not apply and the unit ran something other than what its id says.

**Sharding introduces a failure the serial runner never had: a shard that runs FEWER units
than it was given still exits 0.** A selector that stopped matching, a matrix entry that never
started, a job stopped by a concurrency rule — each is a green tick over an unverified commit,
and twelve green jobs is not the same claim as "121/121". So every unit writes a **receipt**
(unit id, exit code, verdict, wall clock, shard, **and the commit**) and the `coverage` job
refuses unless:

- the union of units run **equals** the planned inventory exactly — a missing one is named, an
  unplanned one is named;
- every receipt carries the **same commit**, so a union across two trees certifies neither;
- no unit is duplicated;
- and every verdict is green.

`ci-shard.test.sh` proves each of those by execution, including S11 (drop one receipt → the
unit is named) and S14 (no receipts at all → refuse rather than certify a plan against
nothing).

**The leak canary is not lost.** `run-all-tests.sh` takes a per-suite baseline so a suite that
writes outside its sandbox is NAMED rather than bisected — the 2026-09-05 `escalations.test.sh`
finding. `ci-shard.sh` takes the same per-unit baseline through the same
`lib/leak-canary.sh`; case S15 proves it fires. Since round 15 (2026-09-11) both runners also
baseline **the operator's record** per unit — the ownership ledger (every row except the
platform's per-turn `finished` rows), the fallback event log and the team directory entries —
through `lib/record-canary.sh`, after three green suites in two days wrote into `$HOME` under
the leak canary (the last a false `terminated` witness for a running agent, written by the
shipped reaper from inside `session-start-stdin.test.sh`). A unit that changes them is
`RECORD-TOUCHED`, red, with the rows printed; case S15b proves it fires.

**Since 2026-09-27 each unit gets a record of its own.** The canary compared the live record
before and after a unit, so any spawn, land or message elsewhere on the machine during the
unit failed it as `RECORD-TOUCHED` and engine proofs could pass only on an idle session. The
runner now runs every unit with `HOME` in a throwaway home (CI's shape: no record, a git
identity), removes every variable that points into the operator's record (`CLAUDE_CONFIG_DIR`
and `RICHOS_WORKSPACES_DIR` included, so their defaults follow `HOME`; they are never set, so a
unit that builds a world of its own keeps it, S15h) and watches the record in that home
(`rc_sandbox` in `lib/record-canary.sh`). A unit that would have written the operator's record writes that one
and is still red (S15b-S15d); a concurrent writer to the live record no longer fails a clean
unit (S15e). What it cannot see is named in the library's header: a path to the operator's
record fixed before the unit started or read from the password database.

**`run-all-tests.sh` was not modified.** The sharded reading is a second, independent
implementation of the same discovery rule and `ci-units.test.sh` case **U1** asserts by
execution that its suite set is byte-identical to `run-all-tests.sh --list`. A shared helper
would have made them agree by construction and then nothing would have noticed when one
started missing a directory.

### 3. A check for runs that never happened — `engine-run-record.yml`

Every other check asks "did it pass?". This one asks "did it RUN?"

GitHub Free allows **20 concurrent jobs account-wide**, shared here with six other active
workflows and per GitHub's documentation an exceeded limit **DROPS** runs rather than queueing
them. A dropped run leaves no cross, no yellow dot, no `gh run list` entry — so every tool that
answers "is main green?" from the latest run answers with the **previous** commit's verdict,
indefinitely. That failure is invisible by construction.

It compares **pushes**, not commits: `on: push` fires once per push, so five commits pushed
together correctly produce one run and a commit-based check would cry wolf on every batch.

It lives in a **separate workflow on a schedule** and that is not tidiness: if
`engine-self-verify`'s run was the one dropped, a job inside it did not run either. A check for
absence cannot be hosted by the thing that might be absent.

`--since 2026-09-10` bounds it to pushes made after the trigger came back. Without that bound
it would be red on its first execution over 95 historical pushes that correctly have no run,
and a check that is red on arrival is a check nobody reads. Deriving the date from the run
history would be circular — if every run had been dropped there would be no history to derive
it from.

### The full pass, green — `34434314424`

```
✓ ci-receipts: 150/150 planned unit(s) ran, all green, all at cfa8e5136f8fb90bb0612ceaaa84c557c0700cd7.
  1 declared KNOWN-RED (lib/ci-known-red.tsv): scripts/lib/worktree-ledger.test.sh
  serial cost 4915 s across 12 shard(s); longest shard 633 s (10.6 min).
```

Twelve shards, 4m54s to 10m49s, every one green. **150 of 150 units named, at one commit**, with
the single declared exception printed rather than silent. That is the claim this design exists to
be able to make: not twelve green ticks, but one statement about a set somebody can read.

The 633 s longest shard is what that run's own packing achieved against the PREVIOUS weights;
re-packed against its own receipts the plan now says 410 s, which is the number the next run will
demonstrate. `timeout-minutes: 60` is still 5.7x that and should be re-promised against a run,
not against this sentence.

## "Required" is a repository setting and it is NOT set — deliberately left for a decision

The `affected-coverage` job is **the one to mark required** and nothing here has marked it,
because doing so changes how this repository accepts work and that is not an engineer's call to
make on the way past.

**Measured 2026-09-10, not assumed:**

```
$ gh api repos/WebDevBooster/richos/branches/main/protection
{"required_signatures":{"enabled":false},"enforce_admins":{"enabled":false},
 "required_linear_history":{"enabled":false},"allow_force_pushes":{"enabled":false},
 "allow_deletions":{"enabled":false},"block_creations":{"enabled":false},
 "required_conversation_resolution":{"enabled":false},"lock_branch":{"enabled":false},
 "allow_fork_syncing":{"enabled":false}}
```

Branch protection exists on `main` and carries **no `required_status_checks` key at all**. So
today every check in this directory is advisory: it runs, it reports and nothing consults it
before a commit lands.

**What turning it on would actually do, stated rather than glossed**, because the consequence is
not "CI matters more":

- **It blocks the lander.** The lander pushes directly to `main`. A required status check applies
  to direct pushes as well as pull requests, so a red or missing check stops a land until it is
  green or the protection is bypassed. That is the point of it and it is also a real change to
  how this team works on a bad day.
- **A DROPPED run leaves the check permanently pending, not green.** That is strictly better than
  today's behavior, where absence reads as the previous commit's success — but "pending forever"
  and "blocked forever" are the same thing to somebody trying to land and the remedy is a manual
  re-dispatch. `engine-run-record.yml` is what turns that from a mystery into a named finding.
- **The check name must be the JOB name and matrix jobs do not have stable names.**
  `affected-coverage` and `coverage` are the two jobs whose names are fixed; the shards are
  `shards (1, 12)` and so on and requiring those individually would break the moment the shard
  count changes. This is the second reason the coverage jobs and not the shard jobs, are the
  ones to require.

**The one command, for whoever decides:**

```
gh api -X PUT repos/RichSpark/RichOS/branches/main/protection/required_status_checks \
  -f strict=false -f 'contexts[]=affected-coverage'
```

Add `-f 'contexts[]=coverage'` as well to require the full pass on `main` — which is the
stronger and slower promise and a separate decision from the fast one.

## The executions, because a YAML that looks right is not a gate that works

Every claim above is from a run somebody can open.

| run | event | SHA | outcome |
|---|---|---|---|
| `34422722682` | push | `328ff8c9` | **the affected gate as ONE job and it was wrong.** 54 units, ~100 min serial, 60-minute timeout. Stopped by hand once the arithmetic was read off the plan rather than waited for. This is the run that produced the sharded affected gate. |
| `34423889101` | push | `e62ab1ae` | **green.** 21 minutes wall clock, 12 affected shards, coverage 32/32. |
| `34425552226` | push | `a84784d3` | **green in 1 m 11 s.** A three-unit diff produced a THREE-shard matrix, not twelve — the matrix carries only shards that have work. This is the required check at the speed it promises. |
| `34425677957` | dispatch | `a84784d3` | **the FULL pass and it is the most useful run of the four.** See below. |
| `34432651037` | dispatch | `05124e66` | red — `worktree-ledger` again, on a different shard. Two failures is not a flake and this is the run that turned it into a diagnosis. |
| `34434314424` | dispatch | `cfa8e513` | **the full pass, GREEN. 150/150 units, all at one commit, longest shard 10m49s.** |

Run `34423889101`, job by job:

```
✓ plan                     13s
✓ non-suite-steps          58s     ci-verify.sh --no-suites: bash -n over 291 scripts,
                                   install.sh, the whole seated probe, demo 7/7,
                                   publication completeness
✓ affected  (1, 12)     17m30s
✓ affected  (2, 12)      6m15s
✓ affected  (3, 12)      3m31s
✓ affected  (4, 12)      4m13s
✓ affected  (5, 12)      3m15s
✓ affected  (6, 12)      3m38s
✓ affected  (7, 12)     17m15s
✓ affected  (8, 12)      5m44s
✓ affected  (9, 12)      4m45s
✓ affected (10, 12)      3m03s
✓ affected (11, 12)      3m15s
✓ affected (12, 12)      3m18s
✓ affected-coverage         8s
- shards                        skipped: a branch push does not pay for the full pass
- coverage                      skipped with it
```

### The full sharded pass, `34425677957` — red and red in exactly the right way

**148 units, all 148 accounted for, one of them red.** The coverage job's verdict:

```
✗ ci-receipts: this run does NOT certify a84784d31d59553eb47c90272406de4e016f3829.

  - 1 unit(s) did not reach a green verdict:
    scripts/lib/worktree-ledger.test.sh                                      FAIL (rc=1)
```

**No missing units. No unplanned units. No duplicates. No mixed commits.** The union proof
passed and the run was refused for the only reason left — which is the whole design working:
twelve shards' exit codes became one claim about a named set and the one thing wrong with that
set was named.

Shard wall clocks ran 5 m 26 s to 17 m 28 s against 140 minutes serial.

**Two findings came out of that run and neither is the suite that went red.**

**1. It asked for 26 jobs at once and GitHub put it in `queued`.** Twelve affected shards, twelve
full shards, plan and non-suite-steps. The account-wide ceiling is 20 across every workflow and
an exceeded limit DROPS runs. Declaring `max-parallel: 6` and then asking for two matrices at
once was arguing with myself. The affected set is a SUBSET of the full inventory by
construction, so `affected` is now skipped whenever `shards` runs and `affected-coverage` says
so out loud rather than going quiet. 26 jobs becomes 14.

**2. `scripts/lib/worktree-ledger.test.sh` is red on git >= 2.55 — which is what every GitHub
runner ships.** This was written up first as an unreproducible one-off. It then failed a SECOND
time, on a different shard and a different commit and one variable isolated it:

| host | git | the `bound-members-fallback` mutant |
|---|---|---|
| `ubuntu:24.04` container | 2.43.0 | **passes** — removing the fallback turns L28 red, as the harness expects |
| the same image + git-core PPA | 2.55.0 | **fails** — L29 goes red returning `[]` and L28 does not |
| the development Mac | 2.52.0 | passes |
| `ubuntu-latest` | **2.55.0** | fails, twice: run `34425677957` shard 11, run `34432651037` shard 4 |

**The BASE suite passes 37/37 on every host.** Only the mutation harness catches it, which is
that harness earning its cost — and what it reports is precise: the mutant forces the
registration fallback unconditionally and under 2.55 that returns `[]` for the L29 fixture
where under 2.43 it returns registrations. So `read_all()` yields no registration for that
fixture under the newer git.

**Four hypotheses were killed by execution before that one survived**, each cheaper to assert
than to test: not a flake (twice, different shards, different commits); not an ordering
interaction introduced by sharding (the whole of shard 11, in the same order, is green on
Linux — the hypothesis worth killing first, because it would have been a defect in this work);
not TMPDIR shape (plain, real, symlinked and the runner's own `/home/runner/work/_temp`, all
pass); and not the seal (a hand-built fixture seals a correctly bound native member under 2.55).

It is DECLARED rather than fixed, because it is worktree-lifecycle internals and the lead ruled
those off-limits to the CI side on 2026-09-10 — `esc-20260910T033915Z-df18e8fe`.

**And it needed a capability the known-red table did not have.** An unconditional row would have
been correct on the runner and a LIE on the development Mac, where rule 2 would then have fired
on every local full pass with "declared red, but it PASSED". So a row may now carry a seventh
column: a shell predicate saying WHERE it applies.

- Empty or absent means everywhere, so every existing row means exactly what it always did
  (case S18e).
- Where the predicate is false the unit is judged **normally, rule 2 included** — a host without
  the defect gets a real verdict rather than a tolerated one (S18b, S18c).
- A predicate that cannot be evaluated is treated as **applying** and says so. Failing open
  would rebuild the skip list one broken shell expression at a time (S18d).

A mechanism that can only say "red everywhere" describes a defect shape that is not the common
one; this tree already carries launchd-only and darwin-only cases.

And the line that is the point of the whole design, from `affected-coverage`:

```
✓ ci-receipts: 32/32 planned unit(s) ran, all green, all at e62ab1ae0b402ac439ccb0f7a2dc5de3c5285b43.
  serial cost 4358 s across 12 shard(s); longest shard 1036 s (17.3 min).
```

**32 of 32, named, at one commit.** Not twelve green ticks and a hope. The set is 32 rather
than the 54 of the earlier run because the diff base moved with the push — the second push's
`before` is the first push's tip, so it correctly selected the units its own commit can affect.

`shards` and `coverage` are skipped on a branch push by design: twelve more jobs on every branch
push would be this workflow starving the other six of the account's 20 concurrent jobs, which is
the same limit `engine-run-record.yml` exists to catch the consequences of.

## The four red units and what was done with each

Each was reproduced on Linux locally first, in an `ubuntu:24.04` container built to the
2026-09-04 record's specification (bash 5.2.21, git 2.43.0, python3 3.12.3, unprivileged uid
1001, `rsync`/`procps`/`lsof` present, the clone alone in its directory so `../richos-hq` does
not resolve) and each was also run on macOS. **That second run is what separated two
completely different problems** and the brief's framing — inflow, not portability — turned out
to be even more right than it claimed.

### FIXED — `contract-integrity.test.sh` case 33 (Linux only)

Case 33 asserts that Layer N **warns** rather than hard-fails when `settings.local.json` is
untracked but not ignored, so it expects the probe to exit 0. On Linux it got 2.

The cause was not Layer N. The case points `GIT_CONFIG_GLOBAL` at a throwaway config to make
the gitignore answer deterministic and points `GIT_CONFIG_SYSTEM` at `/dev/null` — which
between them leave **no `user.name` and no `user.email` anywhere**. The probe's own Layer Q
fixture then cannot `git commit`, Q reports `FUNCTIONAL CANARY DID NOT RUN` and the probe
exits 2 for a reason with nothing to do with the case.

It survived on macOS **by accident of the host**: git falls back to
`user@<canonicalized-hostname>` and refuses only when that has no domain part. A Mac's
hostname canonicalizes to something with a dot; a container's (`dec5b7ba7942`) does not.
Measured on both.

Case 32 was worse than case 33: it EXPECTS exit 2, so it kept passing while testing two
unrelated things at once — a negative test passing for the wrong reason.

Two fixes, each correct on its own:

- **the suite** — the throwaway config now carries an identity, so it replaces only what it
  means to replace and case 33 tests only what it names;
- **the probe** — Layer Q's fixture sets a local identity, as layers AL and IL already did.
  This is the operational half and is not a test artifact: on **any** host where git cannot
  auto-detect an email — every container, every runner without a declared global identity —
  Layer Q silently degraded to `DID NOT RUN` and the worktree reaper went unverified in a probe
  that otherwise looked complete.

Verified: `--only N` exits 3 on Linux and on macOS; `--only Q,Qscope,base,worktree` exits 3
with 46 cases passing on Linux.

### FIXED — `legacy-workspace-orphan.acceptance.test.sh` (Linux only)

`FileNotFoundError: [Errno 2] No such file or directory: '/Library'`, three errors, from
`terminal-branch-shadow.py:51`.

`TRUSTED_GIT` named `/Library/Developer/CommandLineTools/usr/bin/git` and nothing else, so off
macOS the default resolved to a path that does not exist and
`Path(...).resolve(strict=True)` raised before `_authority` could reach any of its own
refusals.

**The platform policy already existed — copied by hand into five test files.**
`legacy-workspace-expiry`, `legacy-workspace-retirement`, `terminal-branch-shadow`,
`terminal-recovery-shadow` and `legacy-workspace-operator-integration` each write
`shadow.TRUSTED_GIT if sys.platform == 'darwin' else '/usr/bin/git'` at their call sites. The
answer was decided; it just was not written where the DEFAULT is taken from and the one
acceptance suite that did not copy the incantation is the one that went red.

`/usr/bin/git` is the right non-darwin value and this is not a loosening: `_authority` already
refuses `/usr/bin/git` **`if sys.platform == 'darwin'`**, because there it is the
developer-selection shim rather than a binary. Off darwin it is the real binary. The refusal
was always darwin-gated; only the default was not.

Second fix, independent of the platform question: a missing trusted binary now raises
`ShadowError` like every other refusal in that function, instead of `FileNotFoundError` with a
traceback pointing at `posixpath`.

Verified: 7/7 on Linux, 7/7 on macOS.

### DECLARED, then FIXED BY THEIR OWNER the same day — `shell-worktree-sparse.test.sh` and `terminalize-agent-worktrees.test.sh`

**Both are red identically on macOS and on Linux, so neither is a portability finding and
neither was ever a CI problem.** Both are contract regressions and the attribution was proven
by execution rather than argued:

| unit | at the suspect's parent | at `4f01e5c7` | broken by |
|---|---|---|---|
| `shell-worktree-sparse.test.sh` | **21/21 pass** at `6472bb60^` | 6 fail | `6472bb60` (2026-09-07) |
| `terminalize-agent-worktrees.test.sh` | **42/42 pass** at `2afb9703^` | 11 fail | `2afb9703` (2026-09-08) |

`6472bb60` made every native member carry `cleanup_owner=claude-code` and taught
`shell-worktree-sparse.eligible()` to refuse such a member, so no shell is sparsified any more.
`2afb9703` introduced `cleanup_policy=integrated-daily` and `terminalize()` now takes the
save_ref+quarantine route only for a member carrying neither marker. **Neither commit updated
the suite it invalidated** and both suites still assert the old contract.

Re-specifying them means deciding what the new contract IS, which belongs to the engineer who
changed it and not to whoever is turning CI on. **The lead ruled on that on 2026-09-10: a
dedicated engineer owns both fixes and neither suite — nor `shell-worktree-sparse.py`,
`terminalize-agent-worktrees.sh` or `worktree-transactions.py` — is to be edited from the CI
side.** Both commits DID update some suites (`worktree-transactions`, `reap-stale-worktrees`,
`reconcile-terminal-worktrees`) and left these two behind, which is why the bisect is
unambiguous. Leaving them red would put a cross on every
commit for a reason unrelated to that commit; deleting their assertions would hide two real
defects. So they are declared in `scripts/lib/ci-known-red.tsv` with the commit, the failing
cases, a sentence saying what would un-skip them and an **expiry**.

### Both rows were deleted the same day, by the rule that exists to delete them

**2026-09-10, about two hours after the table was written.** The lead dispatched the dedicated
engineer its ruling promised; branch `zach-opus-cr2` re-specified both suites against the
contracts those two commits introduced and landed at `b4945c11`. Verified by execution rather
than taken on trust: **21/21** and **42/42** at that tip, against 6 and 11 failures at
`4f01e5c7`.

**Nobody remembered the rows.** `ci-shard.sh` ran the two units after the merge, found them
green and failed the run:

```
FAIL — declared red, but it PASSED
lib/ci-known-red.tsv declares this unit red and it PASSED. The defect is fixed;
DELETE the entry. A known-red table that outlives its defects is how a skip
becomes permanent.
```

**Twelve days of tolerance were granted and fourteen hours were used.** The rows are gone and
the table is empty. That is the only evidence that will ever exist for whether this file is a
skip list or not and the rule which produced it — a declared unit that PASSES fails the build —
is the one that would have been easiest to leave out.

**This is not a skip list and the difference is enforced rather than asserted:**

1. a declared unit that fails is reported `KNOWN-RED`, printed every run with its expiry and
   does not fail the job;
2. **a declared unit that PASSES fails the job** — the defect is fixed and the row is now a
   lie. This is the negative control and without it the table would silently outlive
   everything in it, which is how every skip list everywhere becomes permanent;
3. **an entry past its expiry fails the job** — the tolerance was time-boxed when it was
   granted, so a decision is forced rather than deferred indefinitely.

`ci-shard.test.sh` cases S6, S7 and S8 prove all three.

## What the current weights produce, from one full pass's receipts

```
$ richos/engine/scripts/ci-units.sh plan 12
150 unit(s), 4915 s serial, 12 shard(s), longest shard 410 s (6.8 min)
the serial pass is 12.0x the longest shard
```

**82 minutes of serial verification in 6.8 minutes of wall clock and it began the day as 123
minutes in one job.** Every weight is from run `34434314424` — a single full sharded pass on
`ubuntu-latest`, so the numbers are comparable with each other, which numbers stitched from
several sources are not.

`12.0x the longest shard` is the number to read: with twelve shards that is perfect scaling, so
**the packing is now the only limit and no single unit dominates**. The largest indivisible unit
is the WTI section at 381.6 s, comfortably under the 410 s longest shard.

**The history of that file is the argument for re-measuring it.** It was rebuilt three times in
one day and every version was honestly wrong:

| when | WTI section | why it changed |
|---|---|---|
| a local macOS `--only` sweep | 1500.1 s | taken at load average 34 |
| the quiet-Mac record, 2026-09-04 | 599 s | a quiet host |
| run `34423889101` receipts | 870.7 s | the runner, before mutant concurrency |
| run `34434314424` receipts | **381.6 s** | `mutation-pool.sh` landed and made the harnesses concurrent |

Four numbers for one harness, none of them wrong when taken and only the last is the cost CI
pays today. **A weight is a measurement with a date, never a fact** — which is why every row
carries the run it came from and why `--verify-receipts` prints the serial cost and the longest
shard on every run.

## Re-packing the shards against measured cost

`scripts/lib/ci-unit-weights.tsv` is one measurement per line, each carrying the run it came
from. A unit with no recorded weight is packed at `DEFAULT_WEIGHT` — never at zero or a batch
of new suites all lands in one shard and that shard becomes the wall clock.

To re-measure: read the per-unit durations out of a real run's receipts and update that file.
The plan changes; nothing else does. `ci-units.sh plan` prints what the current weights
produce, longest shard included, which is the number `timeout-minutes` should be re-promised
against.

## What a CI run still cannot cover

The integrity probe's **BY-REFERENCE layers, BR1–BR10**. They verify that an operator's
user-scope `~/.claude` plugin registration resolves to this engine — a property of a
workstation, not of a repository, with no honest way to synthesize it on a runner. They are
exercised instead by `scripts/hooks/by-reference.test.sh`, which builds that two-root topology
from scratch and which the unit inventory discovers automatically. Named here rather than
skipped silently; see `ci-portability-notes.md`.
