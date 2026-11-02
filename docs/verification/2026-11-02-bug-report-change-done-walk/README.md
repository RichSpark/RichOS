# Bust a bug: Change it then Done with nothing changed keeps a waiting report (2026-11-02)

The real-app check for the one finding still open at the handover review of `cc/echo-sonnet-bab2`
(`rv-20261102T003409Z-35b12ee9-35e9`, CEO ruling §156): for a report waiting to go out, Change it
removed the waiting copy and Done with nothing changed saved nothing, so a quit lost the report.
It ran once, in the test VM, on fixture data only (an empty fixture home), against
`report-relay-stand-in.py` on the guest's own `127.0.0.1:8765`, whose test repository is
`richos-test/report-relay-local`. Nothing reached the real relay or the real repository and
nothing ran on the host's screen.

## Build identity

| What | Identity |
|---|---|
| Commit | `112969c0d37e7d2eec37700cd755f9e48253e160` (this branch: the core, shell and window commits of the fix) |
| Bundle | `testvm/branch-bundle.sh` output, `RichOS.zip`, sha256 `fd8d392f7093f0d6fea9d25fb8932cc9286dcfe862e6c2a57fce18cc9a4837ab`, identity `com.richos.app 1.2.6`, ad-hoc signed, guest only |
| Engine | the v1.2.5 release's `richos-engine-1.2.0.tar.gz`, sha256 `80d40c80da401788f35148a731c3f678c1adea99a57face418918fda384f9e30`, pinned into the bundle and handed to the guest with `--engine` |
| Guest | `walk-0c6dd27de019`, 4 CPUs, 5120 MB |
| App environment | relaunched with `RICHOS_CLAUDE_BIN` = `fake-claude-bug-report.py`, `RICHDOCTOR_REPORT_RELAY=http://127.0.0.1:8765`, `RICHDOCTOR_REPORT_REPOSITORY=richos-test/report-relay-local` |

## Command

```
run-walk.py --wait 1800 --bundle RichOS.zip --home <empty dir> \
  --engine richos-engine-1.2.0.tar.gz --report run-report.json -- \
  richos/app/scripts/testvm/steps-walk.py \
  --steps docs/verification/2026-11-02-bug-report-change-done-walk/steps/walk.json.txt --out <out>
```

Run once. `run-report.json.txt`: boot 64 s, scenario 304 s, `scenario_exit` 0,
`cleanup_complete` true (app pid 1953 quit by its recorded pid, VM stopped, clone deleted, CEO
§54), 372 s in all. All 44 steps exited 0 (`steps.json.txt`); every disk and relay check ends in a
`test` that fails its step.

## What it showed

| Step | What the app did |
|---|---|
| nothing listens on `127.0.0.1:8765` (the stand-in is down); answer typed; Send report | "Waiting to send · saved on this Mac" (`1-waiting-relay-down.jpg`); `bug-reports/waiting/` holds ONE file, `unchecked/` none, nothing at the relay |
| Change it | "Changing it"; `waiting/` still holds that ONE file (before this fix it held none from here on) |
| Done, nothing changed | "Waiting to send · saved on this Mac" again, Try now / Change it / Cancel report (`3-done-unchanged-back-waiting.jpg`); `waiting/` ONE file, `unchecked/` none |
| the app quit and started again (pid 1368 to 1953) | `waiting/` still holds the same ONE file, `dd49e5fb-7a48-446a-ab72-29cf0b76d337.json`; nothing at the relay (`4-after-relaunch.jpg`) |
| the stand-in started | the report went out by itself: `sent.jsonl` holds ONE line with id `dd49e5fb-…`, number 412; `waiting/` empty; ONE report and ONE completion at the relay (`stand-in.log.txt`), still one 45 s later |

## What this does not cover

- After the relaunch the report is on this Mac and goes out by itself, but the panel shows no card
  for it (`4-after-relaunch.jpg`): a waiting report has had no card after a relaunch since before
  this branch (`ui/bug-report.js`, `onDelivered`: "After a relaunch there is no card to update, so
  the notice is the whole of it"). This walk shows the report kept on disk and sent, not a card.
- The notice when it went out was not caught in a frame: the last frame was taken 45 s after the
  stand-in confirmed the report and shows the guest's own "Allow Python to find devices on local
  networks?" prompt for the stand-in, which nothing here answered.
- Changed words (Done after typing) and a change told to Rich that he cannot make are covered by the
  browser suite (`ui/tests/bug-report.js`), not by this walk.
