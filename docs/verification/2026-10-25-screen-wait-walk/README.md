# The screen wait (§56) on the real app: Rich did not mark the job as needing the screen

The CEO's order of 2026-10-24 was to implement #11, the screen-unlock wait. A job marked
`needs_screen` waits while the Mac is locked. The timeline says "Waiting for the screen to
unlock" and the job carries on by itself at the unlock. The wait was built and reviewed on
`cc/echo-opus-unlock1` (tip `a924a2a6e`). This record is its one real-app check.

**Outcome: the wait was never entered, because Rich did not mark the job `needs_screen`.**
He wrote the job down with the record tool, with `needs_screen: false` and handed it to Stu.
Its record said Stu was running it 22.8 s after it was registered (its last update). That is the app as it behaved before §56 and
it is what the record tool's own description says an unmarked job does. Whether to mark a job is
Rich's judgment from the tool's description (`assignment_tools.rs`, `needs_screen`). The walk
does not force it and no fake stood in for him. The locked, unlock and timeline steps
therefore did not run, so nothing about the wait itself was measured on the real app.

`richos/app/scripts/testvm/screen-wait-walk.py`, run through `run-walk.py --wait 1200` with
the guest's own signed-in Claude (host and guest `claude` 2.1.310) and fixture data only. The
job typed into the composer was: "Walk the RichOS app on this Mac's screen for me: open its
Settings, take a screenshot of each pane and put them in a folder called Settings walk on my
Desktop." The guest's screen was locked straight after Send (Control-Command-Q, sent 5.1 s
after the send timestamp).

## The build

| | |
|---|---|
| built from | `d7700d51fbc854c61a7408112e539765a213376e` (`cc/echo-opus-unlock2`), read back from the running app's log by the identity step |
| how | `richos/app/scripts/package-app.sh` once, at that tip, admitted by `reserve.py` (CPU 18% busy) |
| version | `1.2.3-dev.d7700d51f`, an ad-hoc signed development build, cdhash `e9d4ee2e4c1a8c23316363a4e814617510fd58bb` |
| zip handed to the guest | sha256 `d0d8f5ecdfe1373b98ab0ddd296ca5debf2c289801a785d50b01926407ee96b4` |
| engine | `richos-engine-1.2.0.tar.gz` from nightly 74, sha256 `b972c13406b9db459321a46e1f1389c2b0a8009703983851503bc4be3b48db79` |

The second run used the same zip. Between the two runs only the walk script and its offline
test changed (`a1401dc83`). Neither is part of the bundle.

## The runs

| Run | Guest | Step | Result |
|---|---|---|---|
| 1 | `walk-128caef12c81` | identity | PASS, built from `d7700d51f…` |
| 1 | | first-run | PASS, company Acme added, conversation "Onboarding" |
| 1 | | ask | FAIL, a defect in the walk, not the app: its first read of the job register after Send ran a shell glob in the guest's login shell, zsh, where an unmatched glob is an error ("no matches found"). The register does not exist until Rich writes the first job down. Fixed in `a1401dc83` (the register is now listed with `find`), with two offline cases run under `/bin/zsh` itself. |
| 2 | `walk-3bfa38239804` | identity | PASS, built from `d7700d51f…` |
| 2 | | first-run | PASS, company Acme added, conversation "Onboarding" |
| 2 | | ask | FAIL at "Rich marked the job needs_screen": the record says `needs_screen: false`, state `running` |
| 2 | | locked, unlock | not run: the walk ends at the first failing step |

Each run ended with `clean: app quit, VM stopped, clone deleted, state removed.` and its slot
released (run 1 after 85 s, run 2 after 102 s). Afterward `pgrep -fl richos-tauri` on the
host found nothing.

## The job record from run 2 (`records-asked.txt`)

The times are the guest's clock, in ms after the send timestamp (`sent_at_ms` 1792914386195.6):
locked at +5087, registered at +8476, last updated (Stu running) at +31300.

```json
{"id": "cec9c017-c7a0-4892-a8ae-5aa93b7d0db2", "kind": "task", "state": "running", "needs_screen": false,
 "detail": "Starting the Settings walk: open RichOS Settings, screenshot each pane, save to ~/Desktop/Settings walk.",
 "obligation_id": "work-ba948cf9824b44849b92d3ac1dbdfa5f", "seat": "work-seat:work-ba948cf9824b44849b92d3ac1dbdfa5f",
 "notices": [], "repositories": [], "registered_at_ms": 1792914394672, "updated_at_ms": 1792914417496,
 "work_session": "3e218ae5-835e-41f8-8a47-981b888a01b5"}
```

The `work-<uuid>` obligation id is the one `assignment_tools.rs` opens for Rich's record tool,
so this is his own call and not some other registration path. Because the walk stopped at the
ask step, no screenshot of the guest's lock screen was taken. That the lock took effect in run
2 is therefore not shown by a picture.

## The step reports

```json
{"vm": "walk-128caef12c81", "expect_sha": "d7700d51f", "steps": [
  {"step": "identity", "evidence": {"built_from": "d7700d51fbc854c61a7408112e539765a213376e"}, "outcome": "PASS", "seconds": 0.7},
  {"step": "first-run", "evidence": {"thread": "thr_33b6b2c28997443e99ca1078f0814dae", "title": "Onboarding"}, "outcome": "PASS", "seconds": 24.2},
  {"step": "ask", "outcome": "FAIL", "detail": "zsh:1: no matches found: /Users/admin/testvm/walk-128caef12c81/home/Library/Application Support/com.richos.app/engine-state/assignments/*/*.json", "seconds": 7.3}]}
{"vm": "walk-3bfa38239804", "expect_sha": "d7700d51f", "steps": [
  {"step": "identity", "evidence": {"built_from": "d7700d51fbc854c61a7408112e539765a213376e"}, "outcome": "PASS", "seconds": 0.3},
  {"step": "first-run", "evidence": {"thread": "thr_3fae96a8b4a04f90adc3033d00f3f789", "title": "Onboarding"}, "outcome": "PASS", "seconds": 24.1},
  {"step": "ask", "outcome": "FAIL", "detail": "Rich marked the job needs_screen (his judgment, from the record tool)", "seconds": 32.7}]}
```

## What the walk now proves when Rich does mark a job

The handover review (`rv-20261025T031926Z-a924a2a6-8dbf#1`) found that the locked step
recorded whether the window showed the waiting line without asserting it. Its search
(`--title 'waiting for the screen'` with a case-sensitive `in`) could never match the label
"Waiting for the screen to unlock" in any case. Since `d7700d51f` the locked step fails unless
a static text on the timeline reads exactly "Waiting for the screen to unlock". The unlock step
fails unless the line then reads "The screen is back, so this is carrying on" and the waiting
label is gone. Both are proved offline by `richos/app/scripts/testvm/test/screen-wait-walk.test.py`,
and neither has yet been seen on the real app.
