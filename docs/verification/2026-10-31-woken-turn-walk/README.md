# Rich carries on after his own background command ends, in a turn he is seen in and can be stopped in, 2026-10-31

The real-app check for finding 2 of the 2026-10-31 app-terminal audit in the private record and
for the lead's ruling on its escalation: in the terminal, the turn that follows a background
command's completion is an ordinary turn. The CEO sees the lead's words and can stop it. So in the
app it must be a visible app turn, with Rich's words in the conversation and a working Stop.

## What ran

- **App:** `richos/app/scripts/testvm/branch-bundle.sh` at `2b19585de` (the app as handed over:
  the continued turn carries his request's authority), pinned to nightly 82's engine (1.2.0,
  sha256 `5da46847…64508`). The running app reported
  `built_from: 2b19585de9734be978ffbc301a59254658dd294e` (identity step).
- **Engine in the guest:** this branch's tracked `richos/engine` (`git archive`) with nightly 82's
  `engine/runtime`, because `make-engine-asset.sh` refused nightly 82's runtime for this branch's
  newer runtime recipe and no runtime built from that recipe is on this Mac.
- **Scenario:** `richos/app/scripts/testvm/woken-turn-walk.py` under `run-walk.py`, one guest run of
  270 s ending 20:25Z, exit 0: identity, first-run, connect, task, woken and stop all PASS. The run
  before it, on the same build, failed only its stop step: Rich's follow-up was `sleep 600`, which
  Claude Code itself refuses to run in the foreground ("Blocked: sleep 600 followed by ..."), so
  the walk's follow-up is now a `ping` of the same length.

## Result

**woken.** Rich was asked to start `sleep 40; echo woken-253f12d0-done` in the background
himself, end his turn and run `echo woken-253f12d0-carried-on` when it finished. In his own
session, in this order: the background launch, the end of his turn, then a Bash run of his that
printed `woken-253f12d0-carried-on`, not refused. No tool result in the guest carries "This app
turn is stopped or is supplying context". His completed turns in the ledger since the send:

1. "The test command is now running in the background in your Acme folder."
2. "The test command finished and the follow-up command printed: `woken-253f12d0-carried-on`"

The second is the turn Claude Code started for the finished command, adopted as an app turn: it
is in the conversation (`woken.png` and the top of `stopped.png`).

**stop.** The same, with a long foreground follow-up (`ping -c 600 127.0.0.1 > /dev/null; echo
woken-253f12d0-stop-late`). Once his session had started it, the screen showed Rich working and
the Stop button (`stop-shown.png`). The walk pressed Stop. The ledger then recorded a turn stopped
after the press, the screen says "You stopped after 9s" with the composer back to "Talk to
Rich..." (`stopped.png`) and `woken-253f12d0-stop-late` was never printed.

The "Connected project folders" panel over the conversation in the screenshots is left open by
the connect step; it is not part of this check.

**Test instance:** `run-walk.py` quit the guest app, stopped the VM and deleted the clone ("clean:
app quit, VM stopped, clone deleted, state removed"). Nothing ran on the host's screen.
