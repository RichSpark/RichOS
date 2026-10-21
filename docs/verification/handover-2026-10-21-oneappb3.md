# Handover: one app for everyone, handover B (echo-opus-oneappb3), 2026-10-21

**Unfinished.** Stopped at 23:05Z by the CEO's order to free the Mac. Branch
`cc/echo-opus-oneappb3`, worktree `/Users/alex/ab/richos-wt/echo-opus-oneappb3`.
Plan: the one-app-for-everyone plan of 2026-10-21 in the private record (ruling 137).

## Done

- Main merged five times: `499209656` (main 097da9d99), `318567be0` (lease1x, one
  conflict in `engine_profile.rs`, both sides kept), `f3d472073` (axhang2), `1e99dd8e0`
  (axretry1, `adopt-walk.py` taken from main) and `624f5ca24` (connectfix1, clean). The
  operator code removal itself was done before this session (`f6d47dad8`..`564cf4b85`).
- Walk harness fixes, each with a test that fails on the old code:
  - `f6a9bd9cf`, `1d0f056eb`, `15659f41a` and `0338c32d9`: after a relaunch, `reap-walk.py`
    `enter()` waits for the home screen's "Talk to Rich" door, takes the conversation the
    door opens and reads the crumb by its AXValue.
  - `e38f09404`: the stop step opens the work summary by the label ending every chip state
    has ("Open the work summary.").
  - `4c7870f98`, `41d9b16c3` and `dfff4d901`: a failed `wait_for` either samples the app
    (when the last read had no answer) or saves a screenshot and the accessibility tree
    (when it answered "not found").
- Walk 6 settled the question about the app. After the relaunch it showed the right screen:
  home with its door, then the "Acme / Onboarding" conversation. Walk 5 ran on a calm
  host and every read was answered, so there was no app hang. Walk 4's silence was host
  load.
- The leftover `operator.json` is inert as far as walk 10 reached. With the file in place,
  the app ran first-run, connected a folder, relaunched, registered a job and ran its
  commands. The deleted reader refused all background work whenever that file existed.

## Left

1. **The stop step fails: the job's foreground command keeps running after Stop.** Walk 10
   gave 'stop-fg (pid 3294) was still running after the trigger'. This is main's
   behavior and this branch does not change it. `stop_worker` (`native.rs:3571`) only
   sends `stop_task` for a background task and returns `NotDelivered` for a foreground
   tool. Raised as esc-20261021T224235Z-4a2d4e70, with the question of whether it is a
   product defect or a stale walk expectation. The quit step was never reached.
2. A green reap walk through `stop` and `quit`, once (1) is decided.
3. Merge main again, rebuild the bundle and engine asset from that tip, then run the proof
   run over the final range with `--reuse`. No proof run covers any tip after `f3d472073`.

## Exact next commands (from the worktree root)

```
git merge main
CARGO_TARGET_DIR=/Volumes/E1TB/caches/cargo-target python3 richos/app/scripts/testvm/reserve.py --wait 600 -- bash richos/app/scripts/package-app.sh
#   then ditto <cargo-target>/workspaces/0fe2dc86139f575b50003e25/release/bundle/macos/RichOS.app to SCRATCH/bundle/RichOS.app
bash richos/app/scripts/make-engine-asset.sh --out SCRATCH/engine --runtime-dir ~/.richos-nightly/runtime-eb026316eb08
#   fixture home: SCRATCH/walk/home/Library/Application Support/com.richos.app/operator.json, body in eaadc469a's message
cd richos/app/scripts/testvm && TESTVM_APP_MODEL=sonnet python3 run-walk.py --bundle SCRATCH/bundle/RichOS.app --home SCRATCH/walk/home --engine SCRATCH/engine/richos-engine-1.2.0.tar.gz --report SCRATCH/walk/run-report.json --wait 600 -- <worktree>/richos/app/scripts/testvm/reap-walk.py --out SCRATCH/walk/out --expect-sha <HEAD> --steps identity,review-watch,first-run,connect,seed,watch,unrelated,stop,quit
python3 richos/app/scripts/proof-run.py --admission-wait 7200 --reuse /Volumes/E1TB/state/richos/proof-runs/ab3836b490a0/20261021T192653Z-x22tolsj $(git merge-base main HEAD)..HEAD
```

## Evidence

- Walks 1 to 10 (logs, `report.json`, screenshots and trees):
  `/Volumes/E1TB/state/richos/walks/echo-opus-oneappb3/walk1` .. `walk10`. The last one is
  `walk10/out/report.json` and `walk10/walk.log` (guest walk-9ce771fa2ab1, "clean:" line at
  22:40:36Z).
- Proof runs (all stopped before they finished, so none counts as a verdict):
  `/Volumes/E1TB/state/richos/proof-runs/ab3836b490a0/20261021T182631Z-7mvo3lrc` (106 of 167
  finished on f3d472073's range, no failure) and `.../20261021T192653Z-x22tolsj` (77 of 167
  on 4b1454781..bea4e6fd4, no failure).
- Escalations: the six records in `docs/verification/escalations/2026-10-21-echo-opus-oneappb3-*`.
