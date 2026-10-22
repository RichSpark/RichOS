# Failure class: no check ever ran the app the way a new customer's first run does

**What it is.** Every check that puts the app on a screen launches it with the harness's
advantages handed in, so the path a new customer takes is the one path no check takes and its
dead ends reach customers first.

**How it showed (2026-10-21).** A new user's first run of stable 1.2.1 hit dead ends no gate
had seen: every message to Rich ended in the same "usually fixes itself" card, the Claude
account said "No reading yet", the update check failed with a raw OS error and machinery rows
reached the conversation.

**Why it could happen.** `scripts/testvm/run.sh` launched every guest walk and the build's boot
proof, with `RICHOS_CLAUDE_BIN`, `CLAUDE_CONFIG_DIR`, `RICHOS_ENGINE_DIR`, an engine payload and
the host's Claude login pushed into the fixture home. So no walk installed Claude Code or the
engine through the app, none was ever signed out and none ran on a home with nothing in it.

## The rule

The build walks every candidate as a new user before it can be published:

- `scripts/testvm/run.sh --new-user` (and `run-walk.py --new-user`) launches the app with `HOME`
  and the window's activation only: no harness Claude Code, login, engine or settings.
- `scripts/testvm/new-user-walk.py` walks it: identity; first setup as a person does it (the app
  installs Claude Code and its engine itself); two messages signed out, each of which must end
  as "not signed in" with the way out on screen; an update check that must reach a verdict; a
  sign-in and Rich's answer. Every frame is read for the known dead ends (`DEAD_ENDS`) and any
  hit fails the step that showed it.
- `nightly-local.py build` runs it after the VM boot proof, in its own guest
  (phase `new-user-walk`) and `publish` refuses a candidate whose walk did not pass on its own
  build commit (`require_new_user_walk`): every step of the whole walk (`NEW_USER_WALK_STEPS`,
  the walk's own `STEPS`) must have run and passed and the walk must have written its own end
  (`complete`), because the report is rewritten after each step and a walk that stopped early
  leaves only passes behind. A candidate built before this requirement keeps the contract it
  was built under.
- The walk reads the screen as a person does: whole-frame OCR text (never a capped search), the
  update row read off the frame, Settings opened and closed with its own button and checked and
  the Claude accounts card visited (step `accounts`).

## The checks that guard the class

| Check | What it refuses |
|---|---|
| `nightly-local.py` phase `new-user-walk`, `require_new_user_walk` | publishing a build a new user's first run has not passed on |
| `scripts/nightly-local.test.py` `test_a_build_walks_its_candidate_as_a_new_user_after_the_boot_proof_and_publish_requires_it`, `test_a_new_user_walk_that_fails_fails_the_build_and_publish_refuses_the_candidate` | the gate being skipped, run out of order or a failed or missing walk being published |
| `scripts/nightly-local.test.py` `test_a_walk_that_stopped_before_its_end_is_refused_however_its_steps_went`, `test_the_gates_step_list_is_the_walks` | a walk that stopped early or a step list that drifted from the walk's |
| `scripts/testvm/test/new-user-walk.test.py` (in `testvm/test/run-tests.sh`) | a dead end the walk would not fail on (anywhere on a frame); an update step passing on an in-between state; the account card's verdict; the update row and the Settings panel read off the frame |

## When a new dead end is found

Add its words to `DEAD_ENDS` in `new-user-walk.py` with a test in `new-user-walk.test.py`, so
every later build is walked against it; then add its class file here.
