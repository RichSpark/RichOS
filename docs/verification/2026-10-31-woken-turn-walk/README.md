# Rich carries on after his own background command ends, 2026-10-31

The real-app check for finding 2 of the 2026-10-31 app-terminal audit in the private record: the
app's own gate refused every tool
Rich called in the turn Claude Code starts when his background command ends.

## What ran

- **App:** `richos/app/scripts/testvm/branch-bundle.sh` at `0feb47130`, pinned to nightly 82's
  engine (1.2.0, sha256 `5da46847…64508`). The running app reported
  `built_from: 0feb471300d84ddb9a6021803aa3cbb924cf0111` (identity step).
- **Engine in the guest:** this branch's tracked `richos/engine` (`git archive`) with nightly 82's
  `engine/runtime`, because `make-engine-asset.sh` refused nightly 82's runtime for this branch's
  newer runtime recipe and no runtime built from that recipe is on this Mac. The turn gate
  (`app-engine-hook.py`'s refusal) is identical in both.
- **Scenario:** `richos/app/scripts/testvm/woken-turn-walk.py` under `run-walk.py`, one guest run of
  338 s ending 18:23Z, exit 0: identity, first-run, connect, task and woken all PASS.

## Result

In Rich's own session, the guest's hook callbacks in order (walk output `woken.json`):

1. `PostToolUse Bash`, `run_in_background: true`: `sleep 40; echo woken-1e7aebc0-done`
   (backgroundTaskId returned).
2. `Stop`: his turn ended. On screen: "I've started the test command in your Acme folder and
   I'll run the follow-up once it finishes."
3. `UserPromptSubmit` carrying Claude Code's `<task-notification>` for that task.
4. `PreToolUse` and `PostToolUse Bash`: `echo woken-1e7aebc0-carried-on`, stdout
   `woken-1e7aebc0-carried-on`. Not refused.

No tool result in any Claude session of the guest carries "This app turn is stopped or is
supplying context" (`gate_refusals: 0`).

**Seen, not part of this change:** what Rich said in that second turn is not in the conversation.
The ledger holds only the first turn (the walk's `rich_turns_since_send`) and `woken.png`, taken after the
carried-on run, shows only the first turn's line. A turn Claude Code starts itself goes to the
between-turn lane (`native.rs`), whose words reach no caller.

**Test instance:** `run-walk.py` quit the guest app, stopped the VM and deleted the clone ("clean:
app quit, VM stopped, clone deleted, state removed"). Nothing ran on the host's screen.
