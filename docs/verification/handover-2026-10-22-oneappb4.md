# Handover: one app for everyone, handover B (echo-opus-oneappb4), 2026-10-22

Continues `echo-opus-oneappb3` (its note: `handover-2026-10-21-oneappb3.md`). Branch
`cc/echo-opus-oneappb4`, worktree `/Users/alex/ab/richos-wt/echo-opus-oneappb4`. Plan: the
one-app-for-everyone plan of 2026-10-21 in the private record (ruling 137).

## Done

- `89bd825e8`: main `055633407` merged (stopfg4's Stop fix, richruns1's walk wording). The
  Stop fix's `TaskBook` part was written into `operator_lead.rs`, which this branch deleted
  after moving `TaskBook` unchanged into `agent_tasks.rs`. Main's own hunks are applied
  there and `operator_lead.rs`/`operator_claim.rs` stay deleted. `reap-walk.py` takes
  main's side (the stop step's `stop_evidence()`). `cargo test -p richos-core --lib`: 1010
  passed, 0 failed, 3 ignored.
- `78daced3f`: `adopt-walk.py` `photograph_app()` keeps every step inside its `try`, so a
  failed wait always raises its own `StepFailed`. The proof run below found it
  (`ax-wait.test.py`, `AttributeError: 'Walk' object has no attribute 'vm'`). Red before,
  green after.
- Dev bundle `1.2.2-dev.89bd825e8` (`RichOSSourceCommit` 89bd825e8b07, cdhash
  62eb646a06bc) and engine asset 1.2.0 from repo 89bd825e8b07 (sha256 4207f65edc82). The
  app code at `78daced3f` is the same; that commit changes only a test VM script.
- **The reap walk passed `stop`, with the planted `operator.json` in place.** Guest
  walk-56a6baace66a: identity, review-watch, first-run, connect, seed, watch, unrelated and
  stop all PASS. In the stop step, Stu ran both heartbeats. After the press, `stop-fg` ended
  in 119 ms and `stop-bg` in 118 ms (bound 2000 ms). The record says "The job's recorded
  work stopped. Stu has been notified." The deleted reader refused all background work
  whenever that file existed, so the file is inert through Stop.
- "clean:" line: `19:33:25Z clean: app quit, VM stopped, clone deleted, state removed.`

## Left

1. **The quit step fails before its Quit.** `quit: FAIL — prerequisite unavailable: quit-fg,
   quit-bg never recorded itself within 300 s (approved 0 request(s))`. The failure is in
   `start('quit')` (`reap-walk.py:354-374`), the step's second ask, sent right after the
   Stop. The ask was typed and sent (`type_into` and `press('Send')` raise on a miss and
   neither did). After that, no heartbeat started. That timeout path saves no assignment
   list, screenshot or tree, so this walk cannot say whether the ask became a job. Not
   rerun, per the brief.
2. The proof run over the final range: see the handoff message (run 1's two failures are
   resolved above; the final run reuses run 1).

## Evidence

- Walk: `/Volumes/E1TB/state/richos/walks/echo-opus-oneappb4/walk1` (`walk.log`,
  `out/report.json`, `out/facts.json`, `run-report.json`, `package.log`, `engine-pin.env`).
- Proof run 1 (over `055633407..89bd825e8`):
  `/Volumes/E1TB/state/richos/proof-runs/0fe64a69138e/20261022T193528Z-0zhqp2y4`. 170
  planned, 166 passed (0 reused), 2 NOT RUN (front-door, gui-boot: no screen), 2 failed:
  `testvm` (the defect fixed in `78daced3f`) and `cargo --bin richos-tauri`
  (`dictation::scratch::tests::two_dictations_back_to_back_leave_nothing`, "decoder
  deadline" on a 10 s bound while the host was 88-100% busy). Retried alone on unchanged
  code at 59% idle: 3 of 3 passed in 0.31 s. This branch does not touch
  `src-tauri/src/dictation/`.
