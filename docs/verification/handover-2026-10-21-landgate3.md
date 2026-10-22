# Handover: zach-opus-landgate3 (branch G item 6, land checks and the land lease), 2026-10-21

Stopped at 23:43Z by CEO order (free the Mac for the CPU fix). Branch `cc/zach-opus-landgate3`,
worktree `/Users/alex/ab/richos-wt/zach-opus-landgate3`.

## Done

- Main merged four times: `0ad4c6c62` (main `b214bbf28`, branch L; item 4 `d53d42777` was
  already on main, so it exists once), `14b0d5bf3` (axhang2, `ced86ec1d`), `0bc8f16be` (axretry1,
  `4b1454781`), `f411e9d9e` (connect fix, `cb0c7ae43`). Main has not been merged since `cb0c7ae43`.
- `94506d9e7`: land-checks-walk.py connect reads the typed folder in the guest first and keeps
  the chooser's screenshot and AX tree when the shared step fails.
- **The real-app walk PASSED on `94506d9e7`** (bundle 1.2.2-dev.94506d9e7, run
  walk-c2d099638695): identity, team, first-run, fixture, connect, job, refused, landed all PASS;
  verdict lines 1 to 10 PASS (the check ran twice, the first land refused with main unmoved, the
  refusal reached the user, the second land is one merge commit of the reviewed work, the app
  held the lease while the check ran and it is gone after, the fence refused nothing of the app
  land, the checkout is clean, the assignment settled). run-walk.py: "clean: app quit, VM
  stopped, clone deleted, state removed."
- Proof run over `cb0c7ae43..94506d9e7` was interrupted: of its 75 checks, 61 passed (some reused
  from the earlier runs), 12 were stopped by the interruption (state `cancelled`) and 2 never ran;
  none failed (counted from that run's `outcomes.json` by its `state` field, corrected 2026-10-22
  by zach-opus-landgate4).

## Left

1. `git merge main` (if main moved), rebuild, walk once on the new tip.
2. The proof run over the branch's own range, reusing the three earlier runs (next command below).

## Why the chooser's Open is never enabled (findings so far)

- Three failures at the same point in the shared step `adopt-walk.py connect_folder`, after
  Go to folder, the path and Return: walk-9696d0b3a4f1 on `0bc8f16be` (Open "disabled" at the
  click, 2.5 s after Return), walk-eb9f006fb025 on `f411e9d9e` (Open not enabled within the 15 s
  wait from `d093d39a6`) and zach-opus-model1 on `d3b8e8835`
  (`/Volumes/E1TB/tmp/claude/zach-opus-model1/walk5/report.json`, same message, a different walk).
- **Not a missing path:** walk-c2d099638695 read the typed path in the guest just before the
  chooser: `drwxr-xr-x ... /Users/admin/testvm/walk-c2d099638695/home/Acme`, a Git repository. The
  same unchanged step then PASSED. The path is the same folder first-run registers and the fixture
  commits into.
- **Not timing alone:** 15 s with Open never enabled. So it is intermittent and lies in the shared
  step itself (owner: whoever Rich assigns; last changed by echo-sonnet-walkfix1 `455193c1a` and
  echo-sonnet-connectfix1 `d093d39a6`). Most likely the Go to folder keystrokes do not reach the
  Go to folder sheet on some runs (the sheet is not up yet when the path is typed, so the path
  lands elsewhere and nothing is chosen). Unverified: no failing run has a picture yet; the
  capture added in `94506d9e7` will keep the screenshot and AX tree the next time it fails in this
  walk.

## Next command

From the worktree root, after the merge and rebuild (the predecessor brief's steps 1 to 4 build
the engine asset, bundle and fixture home):

    python3 richos/app/scripts/proof-run.py \
      --reuse /Volumes/E1TB/state/richos/proof-runs/2dba54c05837/20261021T165438Z-3bbcracc \
      --reuse /Volumes/E1TB/state/richos/proof-runs/2dba54c05837/20261021T192021Z-3sbixw1w \
      --reuse /Volumes/E1TB/state/richos/proof-runs/2dba54c05837/20261021T211314Z-q_s5u6vo \
      "$(git merge-base main HEAD)..HEAD"

## Evidence

- Passing walk: `/Volumes/E1TB/state/richos/verification/zach-opus-landgate3-walk-c2d099638695`
  (scenario report, verdict, receipts, lease readings, runner log, build log, artifact hashes).
- Failed walks: `/Volumes/E1TB/state/richos/verification/zach-opus-landgate3-walk-9696d0b3a4f1`,
  `/Volumes/E1TB/state/richos/verification/zach-opus-landgate3-walk-eb9f006fb025`.
- Proof runs: `/Volumes/E1TB/state/richos/proof-runs/2dba54c05837/20261021T165438Z-3bbcracc`,
  `.../20261021T192021Z-3sbixw1w`, `.../20261021T211314Z-q_s5u6vo`.
- Escalations: esc-20261021T161946Z-88258ba2, esc-20261021T191731Z-ac12598b,
  esc-20261021T204817Z-14118fbb (records under `docs/verification/escalations/`).
