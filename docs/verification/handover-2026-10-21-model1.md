# Handover: per-job model choice (zach-opus-model1), 2026-10-21

Branch `cc/zach-opus-model1`, worktree `/Users/alex/ab/richos-wt/zach-opus-model1`. Build plan item 7
(richos-hq `docs/plans/2026-10-21-everything-for-every-user.md`; switching plan section 5 row M).
Handed over unfinished by CEO order (23:05Z): the final proof run was stopped to free the Mac.

## Done

- `1a0262ee5`: `richos_work.prepare` takes an optional `model`. It must be one of the user's
  `ALLOWED_MODELS`; the ceiling stays `guard-model-ceiling.sh`'s at the spawn; a move below the
  teammate's default needs a `model-downgrade-ack:` line, as the isolation guard asks. The tool
  schema says how to pass both reasons. `stu.md:18` is unchanged: it already says to choose a model
  within the ceiling and the field's description is the way to pass it (no second copy, §121).
- `7625183e4`: the generated coordination config takes `ALLOWED_MODELS`, `MODEL_TIERS` and
  `MODEL_CEILING` from the shipped engine's `orchestration.config` where the user's file sets none,
  so a new user gets the engine's full list and the Opus ceiling.
- `b92e31372`: `lease-setup-walk.py` step `model` (fixture teammate `walkhaiku`, default haiku).
- `1530f7939`: mid-job review rv-20261021T164124Z-0fe64c41-a94a finding 1 fixed (a named model is
  checked by itself, never through the teammate's default). Later reviews of `1530f7939`,
  `af2e6d01a` and `d3b8e8835` PASSED.
- Tests red on main, green on the branch: `app.test.py`
  `test_prepare_runs_one_job_on_a_named_model_within_the_users_ceiling`; Rust `engine_profile`
  `the_users_orchestration_config_reaches_the_coordination_folder_beside_the_apps_own_keys`.
- Merges of main: `0fe64c416`, `af2e6d01a`, `d3b8e8835` (main `cb0c7ae43`). Since `1530f7939` they
  changed only `richos/app/scripts/testvm/` and `docs/`.

## Walks (test VM, `lease-setup-walk.py --steps identity,first-run,connect,model`)

PASS three times, each guest "clean: app quit, VM stopped, clone deleted, state removed":

| Tip | Worker started on a named model | Above the ceiling |
|---|---|---|
| `b92e31372` | `walkhaiku-sonnet-*`, start record `agent-a903a0da9e5f70c95.meta.json` model `sonnet` | `walkhaiku-fable-*` refused: "This work asked for a more expensive model than this app is allowed to spend on" |
| `0fe64c416` | `agent-a9aa487897f5ee8e2.meta.json` model `sonnet` | refused, same message |
| `1530f7939` | `agent-a3a8b5ce27c8bc9af.meta.json` model `sonnet` | refused, same message |

App and engine code at `d3b8e8835` are identical to `1530f7939`.

Known and owned elsewhere: at `af2e6d01a` and `d3b8e8835` the `connect` step fails in the shared
walk tooling ("the chooser's Open button was not enabled within 15 s") and `model` then cannot type
into the composer. zach-opus-landgate3 owns it (esc-20261021T223625Z-7fad6200). Evidence:
`/Volumes/E1TB/tmp/claude/zach-opus-model1/walk5/report.json` (scratch, kept until this lands).

## Left

The final proof run over the branch's own range. No unit failed in any run so far. Logs:

- `/Volumes/E1TB/state/richos/proof-runs/61a52f4f9efe/20261021T175155Z-p6ogx200` (35 PASS, stopped)
- `/Volumes/E1TB/state/richos/proof-runs/61a52f4f9efe/20261021T194240Z-gltyfiar` (24 PASS, stopped)
- `/Volumes/E1TB/state/richos/proof-runs/61a52f4f9efe/20261021T222925Z-bujpr6o8` (stopped at 23:43Z)

To resume the last one exactly as planned (it was planned at `d3b8e8835`):

```
cd /Users/alex/ab/richos-wt/zach-opus-model1
python3 richos/app/scripts/proof-run.py --resume /Volumes/E1TB/state/richos/proof-runs/61a52f4f9efe/20261021T222925Z-bujpr6o8
```

This handover's own commit moves HEAD by docs only. If the resume refuses for that reason, run the
range again and reuse all three:

```
python3 richos/app/scripts/proof-run.py \
  --reuse /Volumes/E1TB/state/richos/proof-runs/61a52f4f9efe/20261021T175155Z-p6ogx200 \
  --reuse /Volumes/E1TB/state/richos/proof-runs/61a52f4f9efe/20261021T194240Z-gltyfiar \
  --reuse /Volumes/E1TB/state/richos/proof-runs/61a52f4f9efe/20261021T222925Z-bujpr6o8 \
  "$(git merge-base main HEAD)..HEAD"
```
