# Automatic checks at commit and at land

Git runs these itself. Nobody runs them by hand and nobody has to be told to (CEO ruling §97,
2026-09-29, after nightly `20260929T063824Z-b27b6279` was refused on
`clippy::let_underscore_must_use: 782 > 777`, a growth no one had checked before it reached main).

| When | On | What runs | A failure |
| --- | --- | --- | --- |
| `git commit` | every branch but `main`, in every worktree | `lint.sh --changed --strict`: the lint ratchets for what differs from `HEAD` (static and load rules; Clippy for a Rust set whose inputs changed) and no count may grow, whatever room a ceiling has | refuses the commit, with the lint's reason |
| `git commit` | every branch but `main`, in every worktree | a branch-changed file that a reviewed check pins by SHA-256 in `docs/development/verification-input-qualifications.json` must have its pin renewed in the same change (the merge would refuse it with `UnqualifiedReader`); read only when a changed path is named there | refuses the commit, naming the file, the unit and the fix |
| `git merge` into a branch | every branch but `main` | `lint.sh --changed` (ceilings; what main brings was held to its land) | refuses the merge |
| `git merge` into `main`, a commit on `main` | the main checkout | the suites that own the changed files (`proof-for.sh --gate`), run by `proof-run.py`, plus `lint.sh --changed` when the land changes something under `richos/app` and that selection does not already include `lint.test.sh`, plus `publication-completeness.sh` over the whole tree being landed (see "What every commit refuses, the merge refuses"); each check capped at 600 s, the gate run in rounds of at most 900 s that keep every pass (at most six) | a failing check refuses the merge before main moves, unless it passes when run again alone, once; nothing else does |
| `git push` of `main` | wherever main is pushed from | the same land checks (the lint as `--all`: HEAD is already the land), only when main's tip has no land receipt (a fast-forward, a cherry-pick, a `--no-verify` merge) | refuses the push |
| main moved (post-merge, post-commit) | the operator's main checkout | `richos/mobile/perf/watch.py trigger`: when the land changed `richos/mobile/`, a detached run measures each wired phone's cold start and return against the benchmarks (`richos/mobile/perf/README.md`, "It runs by itself"); it also reports any earlier run that never finished | never refuses (main has moved); a slower, refused or unmeasured run, a missed run and a trigger that could not run are escalations in the lead's ledger |

No formatter runs: the repository does not enforce one.

**A branch commit is checked on the bytes it commits.** Until 2026-09-30 the commit check
linted the working tree and approved the staged copy, so a bad staged edit whose unstaged
replacement passed was committed with no record and a clean staged edit was refused over
unstaged work it did not hold (hunt part 2, finding 10). Now, when anything under
`richos/app` is staged, the unstaged difference is saved as a binary patch and untracked
files are moved into `<worktree git dir>/richos-autocheck-aside/` for the length of the
check, so the working tree is exactly what is being committed (the index git hands the
hook, including the temporary one of `git commit <path>` and `-a`); afterwards both are put
back. This is the design of pre-commit's `staged_files_only`. Ignored files are never moved.
If a check is killed while work is set aside or it cannot be put back, it stays in that
directory, whose `README.txt` says how to restore it and every later commit check refuses
until it is gone.

**Putting work back never overwrites a later save** (recheck N01, 2026-09-30). Editing goes
on while a commit is checked. A tracked file written during the check, by the check or by
an editor, is left exactly as found and named, unless the set-aside edits also touch it.
In that case the newer bytes are first moved to `changed-during-check/` in the same
directory, your edit from before the check is put back and the commit is refused so you
can compare the two and keep what you want. Until 2026-09-30 that collision reset the whole
working tree to the index, which erased saves in files the edits never touched.

**The one escape is `--no-verify`.** Git cannot be stopped from honoring it, so it is recorded:
a commit whose tree the commit check never passed or a main that moved to a tree with no land
receipt, raises an escalation in the lead's ledger (`richos/engine/scripts/escalate.sh`, read
at every session start and turn end) and is appended to
`<git-common-dir>/richos-autocheck/bypass.log`. A rebase, cherry-pick or revert on a branch
replays commits and is not recorded; its land runs the full checks.

## A changed reader the engine's input selector pins

`richos/engine/scripts/lib/verification-dependencies.json` pins every reviewed reader by
SHA-256. A stale pin makes `verification_inputs.py` refuse every unit that reaches it
("changed reader requires input qualification") and `verification-inputs.test.sh` fails. On
2026-09-30 that happened twice in one day on main (95 failures, then 108), each time summed
from branches whose own commits had passed. So every branch commit also runs
`dependency-pins.py` (the committed copy at `HEAD`, else `main`'s): over the branch's changed
files only, it compares each file the declaration pins (a node source, a hook-reader source, an
external reader in this repository and any pin the branch typed by hand) with the bytes being
committed and refuses a mismatch, naming the file, the row and the renewal command:

    python3 richos/app/scripts/autocheck/dependency-pins.py --renew <file> ...

Renew only after reading the file's added lines for a new configuration key, environment
variable or file it now reads and declaring it in that row. `--all [--rev <commit>]` lists
every stale pin in the file. It parses the declaration only when a changed path is named in it,
and takes under a second on a branch that renewed 274 pins. `../dependency-pins.test.sh` proves
it with throwaway repositories and checks its digests against `verification_inputs.py`.

**The commit also runs every selected check that is fast (2026-10-09, section 97).** After the
selection, the commit runs each selected check whose recorded duration is under 10 seconds,
fastest first, until 40 seconds of recorded time are spent (about 30 s of wall clock at most; a
commit that selects none adds nothing) and refuses on a failure; slower ones stay at the land.
The durations are the merge gate's own medians (`weights.tsv` under the proof-runs folder, the
main checkout's) and a check with no record is not assumed fast. It also runs
`mutation-anchors.py` when the change touches the engine. Cause: a merge refused after 25
minutes by two 3-4 second checks.

**A routine pin renewal uses the fast validator (2026-10-05).** A changed pinned reader selects
`scripts/verification-pins.test.sh`, which uses the selector's existing node and hook-reader
validation over all reviewed source and external pins, directory inventories and known
key/execute floors. When a range, staged change or working change alters only existing digests
in `verification-dependencies.json`, that validator replaces `verification-inputs.test.sh`;
owning suites and other map readers remain selected. Changes to keys, edges, inventories,
reviews, sources or registration contracts retain the full suite, as do malformed maps,
path-only requests and inventories without the fast unit. The fast validator always executes
on a proof-run attempt because external reader bytes are not bound by whole-checkout reuse.
Measured through `ci-shard.sh`: full verification-inputs 154.4 s, fast validator 1.7 s. These
are unit execution times, not a measured complete-job saving.

**Two branches that renewed pins merge by themselves (2026-10-05).** Because every branch renews
the pins of the readers it changed, any two branches built in parallel used to conflict in both
maps and somebody took one side and re-ran `renew-verification-pins.py` by hand. A `.gitattributes`
beside each map names the merge driver `richos-verification-pins`
(`richos/engine/scripts/lib/merge-verification-pins.py`), which `install.sh` registers: it merges
the two maps as JSON, keeps every entry from both sides and sets a pin both sides changed to the
sha256 of the merged file (the three versions merged with `git merge-file`), which is what renewing
it by hand after the merge gives. A value both sides changed that is not a pin (a review) is still
a conflict with git's markers. `install.sh --check` reports the driver;
`richos/engine/scripts/lib/merge-verification-pins.test.sh` proves it in a throwaway repository.

## The merge gate: small, capped and blocking only on a failure (2026-09-30)

The CEO, after a day of 25-71 minute merge checks that failed on everything but the fixes:
*"That whole CI shitshow had the exact same end effect as what has been happening today"*.
T3 Code's release gating (adoption ledger §2.4) is the design copied: affected checks only,
every job capped at 10 minutes. A merge into main runs exactly:

1. **The lint on what changed**: `lint.sh --changed` against the main being landed onto,
   which the lint guarantees is never looser than `--all`. The push backstop keeps `--all`.
2. **The owning suites of the changed files**, from `proof-for.sh --gate`: a directory
   input (a suite that reads a whole tree) selects its suite only for a product path, so a
   change to scripts, the engine, the UI test harness or documents runs only the suites that
   cover or name its files. The merge of zach-opus-lander1 (three mega-lander files) ran
   make-engine-asset, gui-boot and proof-for for that reason; it no longer does.
3. **Caps**: `proof-run.py --cap 600` stops every check, engine units included, at ten
   minutes whatever a dated weight predicts and `--run-cap` ends each round at 900 s (the
   first at what is left of it after the selection); admission and the proof-run slot wait at
   most 900 s in every round.
4. **Rounds (since 2026-10-02)**: a round that ends with checks that have no verdict (timed
   out at their cap or ended at the round's cap) is followed by another that resumes it,
   keeps every pass and runs only those checks. See "A large land runs in rounds" below.
5. **A failure is retried alone, once (since 2026-10-08)**: see the next section.

## A land onto a release branch is checked as a land onto main (2026-10-13, landed 2026-10-19)

`workspaces.sh merge` lands onto a branch the main checkout is not on (v1.2.1, v1.3.0) in a
temporary checkout of that branch. The gate chose the land check only for a checked-out main,
so there it ran the commit check and work the land into main refuses went onto the release
branch (review rv-20261013T095956Z-f4ae06df-2813, finding 1). On 2026-10-18 eighteen fixes
landed onto v1.2.1 that way and two app suites one of them broke were found only when v1.2.1
was merged into main. The lander now names the branch it lands onto in `RICHOS_AUTOCHECK_LAND_TARGET` and a merge
or commit carrying it, detached or on that same branch, runs the land check. A name that
disagrees with the checked-out branch is ignored, so it can add the check and never remove
it. `../autocheck.test.py` proves it for v1.2.1 and main, detached and checked out.

## The policy in force is asked first, on every entrance (2026-10-20)

Verification reliability PRD R8 and A9. What a land is held to was decided late or by an older
copy; each of these is now settled by the policy in force before any check runs
(`../autocheck.test.py`, `ActiveGate`):

- **The second review.** A plain `git merge` of v1.2.1 into main ran its whole land check on
  2026-10-19 and was then refused by the operator fence: its exact tip had no review.
  `workspaces.sh merge` already asked first. Every land whose checkout is on the branch it lands
  onto (main or a release branch) now asks the fence the same question before its checks: the
  installed fence copy is brought current by the land command's own refresh
  (`operator-fences.sh refresh`, `bring_current`) run from the fence the engine has now (main's
  committed files when the engine lives in this repository), because a copy as old as the one of
  2026-10-18 cannot bring itself current; then `operator_fences.py review-check` judges the move
  from HEAD to the commit being landed exactly as the fence judges a move of main, follow-up
  rulings and recovery included. A land onto a release branch is asked it too, which the fence
  itself never sees. A commit made in a detached checkout with the land target named moves no
  branch and is reviewed once it exists.
- **What a land onto a release branch is and which gate judges it.** The shim ran HEAD's
  `autocheck.py`, which for a land onto a release branch is that branch's own copy: v1.3.0 was
  cut before the land check learned its target (d2c2c28dc), so every land onto it ran the commit
  check. And only the lander named its target, so a plain `git merge` in a checkout of v1.3.0 was
  no land at all. Now main's copy runs first for every hook and decides: a checked-out main, the
  branch the lander names or a checked-out branch the workspace registry records a body of work
  integrating on (`workspaces.py integration --all`, its own answer) is a land, judged by main's
  copy; anything else is handed to HEAD's copy when that differs, so a branch's commits are still
  checked by its own version. A failed registry answer is said and the commit is checked as a
  branch commit. A fast-forward onto a release branch writes no merge commit, so no hook can
  refuse it where it happens (the fence guards main's ref only): it is recorded as one onto main
  is and its push asks the review of the move it publishes and then needs a land receipt or runs
  the land check, as a push of main does.
- **The installed shim.** `install.sh` copied the shim once and the copy was older than the file
  for ten days. An installed copy that differs from the shim committed on main runs that one.
  This needs the one reinstall that puts this shim in place (`install.sh`) and never another.

## A changed reference picture carries a recorded look (2026-10-20)

Verification reliability PRD R7 and A5. A picture in `ui/tests/shots-*` is the expected answer
of a check and the run that drew a new one is no evidence that it is right. The harness keeps
each regenerated picture's reference, new picture, difference and page state
(`.shots/regenerated/<picture>/`, `../../ui/tests/README.md`) and asks for one line per picture
saying what a look at them found. Every reference picture a commit adds or changes needs that
line in the commit's message (`commit-msg`):

    Look shots-<dir>/<name>.png: <what the look found, three words or more>

and a land checks every picture it brings against its incoming commits' messages before any
check runs, so a commit made with `--no-verify` or before this rule is caught where it lands (an
empty commit on the branch carrying the line answers it). A deleted picture needs no look.
`../autocheck.test.py` `ReferenceLooks`.

## What every commit refuses, the merge refuses (2026-10-11)

The merge of `codex/axel-wes-recovery` (`9c5f0f6dc`) passed this gate and left
`richos/engine/docs/recovery-supervision.md` citing a path the public tree does not hold. The
engine's commit guard `guard-completeness-commits.sh` runs `publication-completeness.sh` over
the whole tree at every `git commit`, so every commit on every branch was refused until
`918319ed0` reworded the citation. A merge makes its commit without `git commit`, so that guard
never saw it and a merge can make the finding from two clean sides (one branch moves a file,
another cites its old path).

So every land (merge into main, commit on main, the push backstop) runs the same checker from
the tree being landed, as the commit guard runs it (`--skip-private-trees`), before any suite
starts and refuses what the guard refuses: findings or a check that did not complete. A tree
with no `.publication-boundary` is NOT APPLICABLE and is said so. The engine's other commit
guards are not in this position: the leak, vendoring and hook-registration guards judge what a
commit changes against its HEAD, so a later commit that does not touch a finding still passes,
and the row-currency guard already judges the merge itself. `../autocheck.test.py` proves it
with a merge whose citation dangles only after the land before it.

## Nothing appears at the root of main without the CEO's authorization (2026-10-15)

The CEO: *"ANY PUBLICLY VISIBLE ITEM IN ROOT on GitHub must always require my explicit
authorization."* The root rule was a manual step at each land and `AGENTS.md` reached the root
of main through one. So every commit and merge onto main, before any suite and every push of
any branch (main, with or without a land receipt and the release branches), each a root
publicly visible on GitHub, refuses a tree whose top level holds an entry not in
`AUTHORIZED_ROOT_ENTRIES` (`autocheck.py`):
`.claude-plugin`, `.github`, `.gitignore`, `.richos`, `LICENSE`, `docs`, `richos`. The refusal names each entry and says that a new root item needs his
explicit authorization. The list is read from main's own copy of `autocheck.py`, so an item he
has authorized is added to the list in a land of its own, before the item.

A tag push is not asked. The `nightly` tag is the update feed and its root has always held
`latest.json` and `build-info.json`; the check refused nightly 58's feed on 2026-10-15 and the
CEO: *"since when do I need to authorize every fart like this?"*

## A check that fails under the gate's own load runs again alone, once (2026-10-08)

Twice on 2026-10-08 the gate refused a merge on one timing check while its own parallel checks
held the Mac at 97-99% CPU and the same check passed when Rich reran it alone on the same tree:
`setup.js` case 24 (`page.waitForFunction: Timeout 30000ms exceeded`) and `splash.js` case 10b
(the shutter's window moved from 236 ms to 358 ms). His step each time was the first step of
`docs/development/verification-retries.md`: retry the failed unit alone, with normal parallelism
and admission. The gate now takes that step itself:

1. After a round has finished, each check that **failed** runs again in a run of its own:
   `proof-run.py --resume <that round> --only-check <the check> --retry-reason ...`, the same caps,
   admission and proof-run slot as any round, every validated pass carried, nothing else run.
2. **Passed alone**: it no longer refuses the merge. The gate prints
   `FAILED UNDER LOAD, PASSED ALONE: <check>` with the log of each run, the final line names it and
   the land receipt lists it under `failed_under_load_passed_alone` with both logs. Any check that
   still has no verdict then gets its rounds as before.
3. **Failed alone or no verdict alone**: the gate refuses as before and the refusal names both
   logs. The first check that fails alone ends the retries; the gate refuses whatever the others
   would do.
4. Once per check per gate run: never a second retry, never a loop. Only `failed` is retried:
   `blocked` is a failure the runner refuses to repeat on unchanged inputs, `invalid` is a check
   that could not show it ran and the no-verdict states have their rounds.

## A large land runs in rounds (2026-10-02)

The combined land of 13 finished fixes, `cc/zach-opus-e2fix1` (about 114 changed paths, 214
checks), was refused four times without a failing check. Measured from those attempts
(`merge-integ1`, `merge-e46`, `merge-e46b`, `merge-e2` and their state directories):

- The checks that reached a verdict took 6608 s between them. The largest:
  `native-android-app` 471 s, contract-integrity section `Q` 261 s, `proof-run` 260 s, section
  `P` 198 s, `verification-inputs` 192 s, section `MT` 181 s, `native-android-ui` 180 s, `lint`
  171 s, section `manifest` 170 s, `quota-watch` 151 s, `autocheck` 148 s. 36 never reached one.
- Every run kept only about three checks going at once (2557-3291 s of check time in each
  ~914 s run): a check starts only when total CPU is under CEO ruling §77's 80% line and the
  gate's own checks keep the Mac above it. Summed, checks waited 64,000-91,000 s for admission
  in each run. So the plan needed about 2200 s even when the Mac was 78% idle at the start; no
  run of 900 s could hold it.
- Every retry re-ran what had passed. 214 of 214 identities differed from the run before only in
  `settings`: the first run waited for admission what was left of the gate (893 s) and its retry
  900 s and both waits were part of each check's identity. And 91 engine units with no reviewed
  input contract were `fresh`, which the runner never reuses.

Now:

1. **A pass survives the round and the attempt.** A check's identity no longer holds how long
   it may wait to start (`proof-run.py` IDENTITY_SETTINGS), every round passes the same waits,
   and an engine unit with no contract is keyed by the checkout's content like every other
   unqualified check (its receipt is copied with its provenance; `engine receipts` re-reads
   the content and proves the union). A pass is reused only on the same content and inputs:
   the merge result tree, the installs, tools and environment; a changed tree runs everything
   whose inputs changed. The next `git merge` (or the `git commit` that concludes a refused one)
   on the same tree resumes the last round, so it too runs only what has no verdict.
2. **Rounds, not one scaled cap.** A cap derived from planned weights trusts dated data that is
   wrong in both directions; a round boundary instead saves every verdict and a round that
   decides nothing ends the gate at once instead of spending a longer cap. Each round is at most
   900 s and names only the checks with no verdict (`--only-check`).
3. **It stops** when every check has a verdict (the land), when a check fails (refused, as
   before), when a check times out at its own 600 s cap a second time ("NO VERDICT AFTER ONE
   RETRY", a hang is still ended and reported), when a round decides nothing the round before it
   had not or after six rounds: **the total bound is 6 x 900 s = 5400 s**, about twice what
   this land needed. The last two refuse with "NO VERDICT AFTER N ROUNDS: <check>". A land that
   fits one round, as most do, behaves exactly as before.
4. The land receipt records `rounds`.

**Left to the nightly, by name**: suites that need a device this Mac has one of: the
iPhone simulator (native-ios-app, native-ios-share, native-ios-ui) or a screen,
the host's or the test VM's guest (read off the suite as `proof-run.py` reads it).

**No mutation pass runs in the merge (2026-09-30).** The merge of cc/zach-opus-q1
(`4e73fd89`) ran `scripts/operator-fences-mutation.test.sh`, a unit whose whole job is the fence
suite's mutation pass, planned at 1408 s: a comment in it names `ci-unit-weights.tsv`, the land
changed that file and the engine's selector maps a file to every unit that mentions its name.
It ran into its cap, the Mac stayed too busy to start anything else and the gate passed with
60 of 63 checks NOT RUN. Now:

1. An engine unit that is a mutation pass (named `<suite>-mutation.test.sh` or declaring
   `# merge-gate: mutation-pass` in its header) is dropped from the selection, whatever chose
   it and named NOT RUN.
2. Every other check runs with `RICHOS_MUTATION_PASSES=0` and `RICHOS_FOURTEEN_MUTANTS=0`.
   Every `*.mutation.sh` and `*.mutation.py` harness reads the switch as its first command,
   prints one NOT RUN line and exits 0, so the suite that invokes it reaches its own verdict on
   its own checks. `mutation-inventory.test.sh` (engine) and `autocheck.test.py` (app) refuse a
   harness without it. Unset, as an engineer runs a suite by hand, the engine harnesses run as
   they always did; the workspace suites' and the fourteen-point pass run only when told 1.
3. The passes run in the nightlies: `nightly-engine.py` runs every engine unit with both
   switches at 1, as a job of its own that never starts beside an app nightly and raises an
   escalation for anything that does not pass (the lead's decision on
   esc-20260930T223507Z-b12f0d6a); the app nightly's `gates/workspace-mutants` and its script
   suites run the app's.

**A check meets the nightly's conditions (2026-10-01).** Nightly attempt 2 failed two suites
that had passed for their engineers, because the build hands its suites a long TMPDIR and
variables (RICHOS_IOS_POOL_WAIT) an engineer's shell does not. Now `proof-run.py` gives every
check of a suite a desktop nightly gate runs (a `run-tests.sh --only` suite in `run-tests.sh
--for desktop --list` and a UI suite run directly) what that gate gives it:
`nightly-local.py gate_conditions()`, built by the same functions the gates call, so nothing is
copied and nothing can drift. That is macOS's per-user temporary folder (`getconf
DARWIN_USER_TEMP_DIR`, which the nightly now uses whoever starts it), the nightly's PATH and
every variable it sets for that gate; a check's own values still win. The plan prints a
`nightly conditions:` line naming them. Deliberate differences: the mutation switches above
stay 0, RUN_TESTS_SKIP_UNCHANGED is never set and RICHOS_RUNTIME_DIR comes from
`supply_runtime()`. `--without-nightly-conditions` runs a diagnosis without them and is refused
here. `merge-check-scope.test.py` (NightlyConditions) proves it against the real gate code.

**A check planned past its cap is never started and the long checks go first (2026-10-05).**
Under `--cap`, `proof-run.py` names a check whose planned weight is over the cap NOT RUN before
anything starts ("planned N s, over its 600 s cap; not started, the nightly runs it"): it
takes no token, lane or admission and an engine unit leaves the receipts proof. The rest
start longest first, so the long poles overlap everything else instead of following it: started
cheapest first (2026-09-30 to 2026-10-05), they were the work a round's 900 s cap ended after
most of it was done, 15,508 s redone in a later round. The planned weight is the median of the
check's last nine measured executions in this checkout (`weights.tsv` in the proof-run store,
engine units included); a check never measured here keeps its dated row
(`lib/ci-unit-weights.tsv` for an engine unit) and the engine lanes are packed with the same
numbers (`ci-units.sh`, `CI_UNIT_WEIGHTS_MEASURED`).

**A refusal names the command that reruns the check as the gate ran it (2026-10-05).** The gate's
checks inherit `LC_ALL=C` from the merge (and the nightly's conditions and what the runner sets),
where a shell has its own locale, so a hand rerun could pass where the gate failed
(`land-completeness.test.sh`, fixed in 3c0383fba). Every check's exact directory, environment
(`env -i`) and command are written to `<run>/rerun/<nn>-<check>.sh` when it starts; the summary
prints `rerun as the gate ran it: sh <file>` with its locale and time zone beside every check that
did not pass, `summary.json` carries it as `rerun` and the refusal banner repeats it.
It also reruns the same TREE: the merge is aborted after a refusal, so the main checkout is back
on main (a front-door.js failure "passed" its rerun on main that way, attempt-ioj6a1_a). The
tree the checkout held when the run started is recorded (`lib/rerun_tree.py`), the summary line
shows it as `tree=` and the file runs the check in the checkout while it still holds that tree,
otherwise in a scratch git worktree of it (the recorded HEAD and MERGE_HEAD, removed afterwards).

**What blocks**: a check that failed (`failed`), an unchanged failure the runner refuses to
run again (`blocked`) and an `invalid` result for any reason except inputs that changed while
the gate ran (an invalid result can hide a failure, for example a UI suite whose ledger
records a failed check). `engine receipts` blocks only when every engine unit passed.
**Nothing else blocks**: a check over its cap, ended at the gate's cap, not admitted, stopped
by the verification controller, NOT RUN for any reason or left to the nightly is named in
the verdict and written to the receipt's `not_run` with its reason. The nightlies run all of
it: `nightly-local.py` for the app and it stays the release gate; `nightly-engine.py` for
every engine unit, which gates no release.

### Measuring the gate

`python3 richos/app/scripts/autocheck/autocheck.py measure`, run by hand in a checkout, runs
the land check of what is staged there against HEAD, exactly as a commit onto main does and
writes no land receipt. To replay a land: detach, stage the land's change on top of a base
without it, run `measure`, then return to the branch.

Measured 2026-09-30 on cc/zach-opus-gate1 (07d289da8), replaying two of that day's lands:

| Land | Old gate that day | New gate |
| --- | --- | --- |
| zach-opus-lander1 (3 mega-lander files) | 11.4 min (21 checks) and 24.3 min (45 checks, iPhone suites, make-engine-asset, gui-boot), both refused | 2.9 min, 8 checks (6 engine units, receipts, the contamination row); refused: `verification-inputs.test.sh` (reader pins changed by an earlier land) and the change's own `app.test.sh` writing its HOME record |
| echo-opus-assign1 (4 richos-core files) | 4.0 min (21 checks) refused; a second try passed 10 min with the iPhone suites failing | 5.6 min cold (sccache bypassed, see the escalation of that day), 16 checks, front-door and gui-boot left to the nightly; refused: the operator walk's W5 check (deleted with the operator path on 2026-10-21), whose snippet the change removed |

Both refusals are failures of checks the change owns; nothing was refused for a simulator, a
screen, a cap or admission.

Measured 2026-10-01 on cc/zach-opus-gate2, in its own workspace, with the Mac at 84-94% CPU
from other work for most of each run:

| Land | Gate of 4e73fd89 | This gate |
| --- | --- | --- |
| cc/zach-opus-q1 (6 files: pins, weights, contract-integrity's shared sandbox list, nightly-local) | 922.9 s; 63 checks, 3 reached a verdict; the fence suite's mutation unit took a lane for 602 s | 905 s; 57 checks (the mutation unit left to the nightly engine run), 30 reached a verdict (18 passed, 12 failed), 8 invalid because something wrote what they read, 19 ended at the 900 s cap |
| one comment line in `ui/quota.js` | not measured | 499.4 s; 16 checks: 14 passed, 2 screen suites left to the nightly |

Of the 12 failures in the first replay, 3 units are red on main by themselves, 8
contract-integrity sections fail their intact-probe case only under proof-run's private
execution profile (they pass through `ci-shard.sh` directly) and 1 was this branch's own (a
test pinning SCR's stale weight, fixed). Escalation esc-20261001T002153Z-46ac290e has the
detail. A qualified engine unit sees only the environment its contract declares, so every
qualified unit that reads a harness declares `RICHOS_MUTATION_PASSES` and
`RICHOS_FOURTEEN_MUTANTS` (`autocheck.test.py` refuses one that does not).

## NOT RUN is not a pass

A check `proof-run.py` could not run is reported as its own state, `not-run`, with a reason:
`no-screen` (a suite that boots the app on a screen, under `--no-host-screen` with no test-VM
guest named), `host-gap` (a declared host gap), `unchanged-inputs` (a suite skipped by
`RUN_TESTS_SKIP_UNCHANGED`) or `suite-skipped` (a UI suite run directly whose evidence ledger
records only a skip, such as `realbytes.js` with no `cargo`). Until 2026-09-29 `front-door` and
`gui-boot` were recorded as `passed` in exactly that case and until 2026-09-30 so was a
skipped UI suite.

**The land's decision (since 2026-09-30): no NOT RUN refuses the land; each is named.** Until
then a `host-gap`, `unchanged-inputs` or `suite-skipped` NOT RUN refused it and only
`no-screen` was accepted, for the reasons below. A check that did not run is not a failure of
the change and the merge gate blocks only on failures (above); the nightly answers for it.

**One exception (since 2026-10-01): an owning check that ends `timed-out` or `cancelled` is run
again and refuses the merge if it never reaches a verdict.** Merge `7af4c981e` landed
although its owning suites `native-android-app` (timed out) and `native-android-ui` (ended at the
gate's cap) had no verdict, with the Mac near 99% CPU; both passed when run alone afterward. So
when such a unit appears, the gate resumes the saved plan (`proof-run.py --resume`, every pass
kept, a fresh round, the verification-retries procedure) and decides on that answer. Since
2026-10-02 that is the next round ("A large land runs in rounds"): a unit that times out twice,
or has no verdict when a round decides nothing new or the six rounds are spent, refuses the merge,
naming it with "re-run it alone". The other no-verdict states (`not-admitted`, stopped by the
controller, runner failed) stay NOT RUN, named as above; a merge that passes on a later round
carries no NOT RUN for that unit.

The original reasoning for the screen case, which still holds:

- A land may never put anything on this Mac's screen (CEO ruling §65; `--no-host-screen` is
  how every land runs), so a screen suite can never run in one here. Refusing would make every
  land that touches the app's screen inputs impossible and the only way through would be
  `--no-verify`, which skips every other check too.
- It is still not a pass and nothing records it as one: the verdict line says
  `ALLOWED WITH N CHECK(S) NOT RUN, WHICH IS NOT A PASS` and names each one, the land receipt
  lists them under `not_run` and `nightly-local.py publish` refuses a build of that commit
  until a gui-boot proof taken against it exists (`--gui-proof`). The screen proof is made
  where a screen may be used, the test VM, before anything is published.
- With `RICHOS_GUI_HOST` naming a guest, the same suites run in the guest and are real passes
  or failures; nothing here changes that.
- The land check still removes `RUN_TESTS_SKIP_UNCHANGED` from its environment: a land runs
  every suite it selected, so an `unchanged-inputs` NOT RUN does not happen there.

## A check's inputs are not another check's outputs

`proof-run.py` fingerprints each qualified check's declared inputs before the run and again
after it; a difference makes a pass `invalid`. In the refused land of 6ef73abf,
`no-foreign-app-data` (inputs include `richos/app/crates`) passed 3 of 3 and was invalidated
because the lint check beside it ran Clippy on `crates/richos-user-update`, whose Cargo target
directory is inside that root. A build cache is output: a directory git ignores, with nothing
tracked in it, carrying the standard `CACHEDIR.TAG` (Cargo writes one into every target
directory) is not part of any check's input identity (`lib/proof_evidence.py`,
`build_cache`). The UI suites' own per-run output directories (`ui/tests/.shots`, `receipts`,
`.vouch`) are output by the same rule, named in `DECLARED_OUTPUT_DIRECTORIES` because their
node writers do not tag them: the push of c7491c34 was refused when `.shots` changed under
`no-foreign-app-data`. Everything else is bound exactly as before. When a pass is still invalidated,
the note names the changed paths and every check that had started by then, instead of only
"execution inputs changed".

## Pieces

- `autocheck.py`: the checks, one file, standard library only. Its header is the reference.
- `shim.sh`: copied into `<git-common-dir>/hooks/` as `pre-commit`, `commit-msg`, `pre-merge-commit`,
  `post-commit`, `post-merge` and `pre-push`. It runs the committed `autocheck.py` from git's
  object store: main's, which decides whether this is a land and hands anything else to HEAD's
  when that differs; else HEAD's, else the incoming branch's (the land that introduces it). With
  none of them, it does nothing. An installed copy that differs from the shim committed on main
  runs that one.
- `install.sh [--check|--uninstall] [<repo>]`: installs the shim and proves git will call it.
  On this Mac `core.hooksPath` points at a global dispatcher that chains to the repository's
  own hooks; `--check` reports a hook that would never run. It never replaces a hook it did
  not write. Every linked worktree shares one hooks directory, so one install covers the main
  checkout, `~/ab/richos-wt/*`, native worktrees, Codex's and the nightly's.
- `../autocheck.test.sh`: throwaway repositories with stand-in tools; every row of the table
  above, refused and accepted.

Receipts: a passed commit check leaves the tree it checked in the worktree's git directory for
`post-commit` to compare; a passed land check leaves
`<git-common-dir>/richos-autocheck/land/<tree>`. Both are written by the checks and read only
to tell a skipped check from a passed one.

## Cost

Measured on this Mac on 2026-09-29, on the commits and the land check that built this:

| Change | What the commit check runs | Measured |
| --- | --- | --- |
| nothing under `richos/app` (docs, the record) | nothing | 0.0 s in the check |
| scripts, tests, declarations | `--changed --strict` static and load rules over the changed files | 0.2 to 2.5 s |
| a Rust test (the refused `let _ = tx.send(())`) | the above plus the Clippy fast set, warm | 5.3 s for the whole `git commit` |
| the lint itself or its baselines | the full static pass and the fast set | 41 s |

Before this, the same checks cost 35 to 49 s for the static pass and 49 s for a warm Clippy
fast set on every run: `richos-voice`'s build script watched `~/ab/richos-wt`, whose mtime
moves whenever any worktree is created and recompiled the crate each time (fixed alongside).

The land check costs what `proof-for.sh` assigns to the change, which is what a land was
supposed to run anyway, plus about 4 s of its own (selection and receipt). Measured: a land of
one changed script (5 checks, including `lint.test.sh` and `make-engine-asset.test.sh`) took
128 s end to end in a fresh clone, 112 s of it the lint suite compiling Rust at a new path.
When the land changes something under `richos/app` and the selection does not include
`lint.test.sh`, the check adds `lint.sh --changed` (until 2026-09-30 `--all`, 58 s warm); a
land that changes nothing under `richos/app` (engine-only or documentation) adds nothing, as
the commit check does. The merge gate's own measured cost is in the land records of the
change that introduced it (cc/zach-opus-gate1).

## Merge retries and phone scope

Merge checks retain each proof attempt on the mounted external SSD under
`/Volumes/E1TB/state/richos/proof-runs/autocheck/`. An unchanged tree and selection
resume the frozen plan. A changed tree gets a fresh selection and offers the previous
attempt to the runner with `--reuse`. The runner validates inputs and evidence before
reusing a pass. Missing or corrupt retry records refuse the check. Resource refusals
remain NOT RUN and cannot satisfy a merge. Diagnosed behavioral retries still require
`RICHOS_AUTOCHECK_RETRY_REASON`; the hook supplies no automatic diagnosis and preserves
the runner's existing retry budget.

The three iPhone suites declare `select-inputs` for product dependencies separately
from their broader `inputs` used to validate saved evidence. Harness changes run the
fixture controls in `merge-check-scope.test.sh`. Actual Core, project, plist and app
inputs still select the phone checks for an engineer's own proof and the nightly; since
2026-09-30 a merge into main leaves the simulator suites to the nightly and names them (the
merge gate, above). Where they run, they queue on one simulator lane with a finite pool wait
bounded by the check's existing deadline.

`lib/ios_ui_scope.py` maps reviewed feature files to their affected XCTest families
and unit-test suites. Shared inputs, new feature files and unreviewed test inventories
use full coverage. When adding cases or changing feature dependencies, review this
map before refreshing its inventory fingerprint. A test-source change selects its
own cases. Simulator retries enumerate the required cases and re-read the retained
result bundles with matching source/tool/device/selection identity and build stamps.
Only failed or missing cases rerun, plus the existing unit-bundle attribution probe
when required. A completed device reuses its evidence without acquiring a simulator.
Failure bundles stay in the external cache and are copied into the proof report;
reports retain the distinction between newly executed and reused cases.

## commit-msg: the Battery-check trailer (CEO ruling §81)

A commit touching `richos/mobile/` is refused at the commit unless its message carries a
`Battery-check: NO — <evidence>` trailer that git itself parses (`battery-check.py --message`,
the verdict the land gives). One line or every continuation line indented; a wrapped answer
with unindented continuation lines is plain text to git and is refused, with the way to write it.
