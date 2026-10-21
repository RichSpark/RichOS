# Handover: echo-opus-skills1, 2026-10-21 (stopped by CEO order at 23:05Z, unfinished)

Branch `cc/echo-opus-skills1`, worktree `/Users/alex/ab/richos-wt/echo-opus-skills1`, tip
`1a512db3a049fa2e004428418d9a709861be0dcf` (a merge of main `cb0c7ae43`). Plan: rows S, K and W of
the private record's 2026-10-21 plan for switching to the app (richos-hq, not in this repository).

## Done

- **S, the engine's skills in every lease** (`9235386a5`): `engine_profile.rs` links every
  `engine/skills/<name>` holding a `SKILL.md` into each lease's engine plugin, except `rich-lander`,
  `using-git-worktrees` and the three the standing plugin already carries. Test
  `every_lease_loads_the_engine_s_shipped_skills_except_the_terminal_only_two`: red on main
  (`tests/engine_profile.rs:1006`, NotFound), green on the branch. Swift language server not built
  (the `sourcekit-lsp` shim opens the developer-tools install dialog on a Mac without them).
- **W, a standing order sticks** (`f66bb118c`): one sentence in `engine/app/agents/rich.md` naming
  `~/myrichos/instructions.md` and saying a "from now on" order goes there. Test
  `rich_s_own_instructions_name_the_file_a_standing_order_is_saved_in`: red on main
  (`tests/engine_profile.rs:755`), green on the branch. No `--add-dir` change was needed: Rich's own
  lease wrote the file with Bash, not refused.
- **K, transcript retention**: does not bite on claude 2.1.306, nothing built. The probe
  `scripts/qa/lease-retention-probe.py` (`aabca7e8d`, fixed `59c089381` so it refuses an answer unless
  every start reached `system/init`): the lease-shaped start reached `system/init` and deleted
  nothing ("Skipping retention cleanup: userSettings source is disabled (--setting-sources) and no
  enabled source provides cleanupPeriodDays"), while the default control deleted the 60- and
  150-day transcripts. `bash scripts/qa.test.sh`: all 257 passed.
- **Walk** (`skills-walk.py`, `6f5a0c875`; stricter skills verdict `840435ae5`, its test
  `test/skills-walk.test.py` red 4 failures before, 7 OK after): on the tip's bundle
  1.2.2-dev.1a512db3a, guest walk-3aad82a5cdb6: identity, first-run, connect, skills and
  standing-order PASS; `scenario_exit` 0, `cleanup_complete` true.

## Left

The branch's proof run over `cb0c7ae43..HEAD`. It was interrupted by the CEO order after 8 checks
passed and none failed (interruption.js, outage.js, realbytes.js, richscribe.js, make-engine-asset,
testvm, claude-quota, example-setup). Its plan and results are held for this handover. Resume it
from the worktree, then hand over its summary line:

```
cd /Users/alex/ab/richos-wt/echo-opus-skills1/richos/app && python3 scripts/proof-run.py --resume /Volumes/E1TB/state/richos/proof-runs/aa5212d1bb5b/20261021T220557Z-oe0at34p
```

Logs: `/Volumes/E1TB/state/richos/proof-runs/aa5212d1bb5b/20261021T220557Z-oe0at34p`.

## Evidence

`/Volumes/E1TB/state/richos/handover/echo-opus-skills1/`:
`walk-final-1a512db3a/` (run.json, out/report.json, out/observed-*.json, the app's logs),
`walk-first-6f5a0c875/`, `k-probe-final.json`, `red-s.log`, `red-w.log`,
`skills-walk-test-red.log`. Walk report record: commit `93ecee44d`. Escalation
`esc-20261021T194753Z-dd8f1ba9` (the connect read, fixed on main by `060cbab4b`).
