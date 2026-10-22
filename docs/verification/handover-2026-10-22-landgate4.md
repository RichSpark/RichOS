# Handover: zach-opus-landgate4 (continues zach-opus-landgate3, branch G item 6), 2026-10-22

Branch `cc/zach-opus-landgate4`, worktree `/Users/alex/ab/richos-wt/zach-opus-landgate4`.

## Done

- Local main `0f7e719b0` merged once as `4c2205f23` (clean; the merge autocheck passed in 213 s).
  Main had changed `richos/engine/` and app code since `cb0c7ae43`, so the app was rebuilt and
  walked again.
- `4e3ab5bbc`: the landgate3 handover's proof-run counts corrected to what that run's
  `outcomes.json` records (61 passed, 12 `cancelled`, 2 not run, none failed), the second
  review's P3 on `5f82f90d0`.
- Rebuilt from `4c2205f23`: engine asset (`make-engine-asset.sh --runtime-dir
  ~/.richos-nightly/runtime-eb026316eb08`, the runtime `verify-runtime.py` accepted against
  `runtime-sources.json`), digest `9e985f617e8277d04cc9053eab00a5dbece2cba18915a163527c30eda9c0130c`;
  ad-hoc dev bundle 1.2.2-dev.4c2205f23 (cdhash `926c1f0c1258daee8cd600946402e0c75917ec56`),
  zipped, sha256 `abbda9c15cbf82ab5ba14596099c6ed55d49a720f95ce1160ce4c59c732f96aa`; fixture home
  an empty folder seeded with `land-checks-walk.py --seed-team`.
- **The real-app walk PASSED on `4c2205f23`** (run walk-b44c76d19316): identity, team, first-run,
  fixture, connect (27.3 s), job, refused, landed all PASS; verdict lines 1 to 10 PASS.
  run-walk.py: "clean: app quit, VM stopped, clone deleted, state removed."; scenario exit 0,
  cleanup complete.
- The intermittent connect failure (the chooser's Open never enabled) did not occur on this walk,
  so `adopt-walk.py connect_folder` is unchanged.

## Evidence

`/Volumes/E1TB/state/richos/verification/zach-opus-landgate4-walk1`: `walk-report.json`,
`walk.log`, `scenario/` (report, verdict, receipts, lease readings, app log), `bundle.log`,
`engine-asset.log`, `engine-pin.env`, `artifacts.sha256`.
