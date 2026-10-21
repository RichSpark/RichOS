# Handover: per-job model choice (zach-opus-model1), 2026-10-21

Branch `cc/zach-opus-model1`. Build plan item 7 (the private build plan in richos-hq, "everything for
every user"; switching plan section 5 row M). Handed over unfinished by CEO order (23:05Z): the final
proof run was stopped to free the Mac.

## Done

- `1a0262ee5`: `richos_work.prepare` takes an optional `model`. It must be one of the user's
  `ALLOWED_MODELS`; the ceiling stays `guard-model-ceiling.sh`'s at the spawn; a move below the
  teammate's default needs a `model-downgrade-ack:` line, as the isolation guard asks. The tool
  schema says how to pass both reasons. `richos/engine/app/agents/stu.md` line 18 is unchanged: it
  already says to choose a model within the ceiling and the field's description is the way to pass
  it.
- `7625183e4`: the generated coordination config takes `ALLOWED_MODELS`, `MODEL_TIERS` and
  `MODEL_CEILING` from the shipped engine's `orchestration.config` where the user's file sets none,
  so a new user gets the engine's full list and the Opus ceiling.
- `b92e31372`: `lease-setup-walk.py` step `model` (fixture teammate `walkhaiku`, default haiku).
- `1530f7939`: a named model is checked by itself, never through the teammate's default (mid-job
  review finding). Later reviews of `1530f7939`, `af2e6d01a` and `d3b8e8835` passed.
- Tests red on main, green on the branch: `app.test.py`
  `test_prepare_runs_one_job_on_a_named_model_within_the_users_ceiling`; Rust `engine_profile`
  `the_users_orchestration_config_reaches_the_coordination_folder_beside_the_apps_own_keys`.
- Merges of main: `0fe64c416`, `af2e6d01a`, `d3b8e8835`. Since `1530f7939` they changed only
  `richos/app/scripts/testvm/` and `docs/`.

## Walks (test VM, `lease-setup-walk.py --steps identity,first-run,connect,model`)

The model step passed three times and each guest was cleaned (app quit, VM stopped, clone deleted):

| Tip | Worker started on a named model | Above the ceiling |
|---|---|---|
| `b92e31372` | receipt `walkhaiku-sonnet-*`, the worker's own start record says `sonnet` | `walkhaiku-fable-*` refused with the app's "more expensive model" sentence, no worker started |
| `0fe64c416` | same | same |
| `1530f7939` | same | same |

App and engine code at `d3b8e8835` are identical to `1530f7939`.

Known and owned elsewhere: at `af2e6d01a` and `d3b8e8835` the `connect` step fails in the shared
walk tooling ("the chooser's Open button was not enabled within 15 s") and `model` then cannot
type into the composer. zach-opus-landgate3 owns it (escalation esc-20261021T223625Z-7fad6200).

## Left

The final proof run over the branch's own range. No unit failed in any run so far. Runs, in the
proof-run state directory for this repository:

- `20261021T175155Z-p6ogx200`: 35 passed, then stopped.
- `20261021T194240Z-gltyfiar`: 24 passed, then stopped.
- `20261021T222925Z-bujpr6o8`: stopped at 23:43Z. Most of its units are canceled or `invalid`
  ("execution inputs changed during the check"), because the escalation record was written into the
  worktree while it ran.

Resume the last run with `proof-run.py --resume <that run's directory>`. The tree it was planned
against has since moved (docs only), so expect a refusal. Then run the branch's range again with
`--reuse` of all three run directories, over `$(git merge-base main HEAD)..HEAD`, which keeps every
pass whose inputs still match.
