# Two messages sent before the launch opens his conversation reach Rich in his order (esc-20261020T225934Z-0d59dda7)

Nightly 69 (`6e04fc12`): Ray's walk had both messages issued in order but received reversed,
the correction as intake 1, twice (`out1b`, `out1d`). Cause: each Tauri command runs as its own
task on the multi-threaded runtime, so two `send_message` calls issued in the same tick race to
the intake log. Fix `571f74ff3`: held sends go out in line, each after Rich has accepted the one
ahead (its `turn-status: queued` or its call ending).

`richos/app/scripts/testvm/first-send-walk.py`, three fresh test guests through `run-walk.py`,
fixture data only, bundle built from `571f74ff3` (ad-hoc signed development build, version
`1.2.2-dev.571f74ff3`), engine `richos-engine-1.2.0.tar.gz` from the nightly 69 release. Input is
a person's: real keystrokes (System Events) and the Return key, straight after leaving the home
screen. "Open when sent" is read by OCR off the frame taken right after the second Return.

| Guest | Step | Open when sent | Result |
|---|---|---|---|
| 1 | two (first relaunch) | no | in his order: intake 1 the proposal, intake 2 the correction, 62 ms apart |
| 1 | two (relaunches 2 to 5) | yes, all four | did not test the window (the walk refuses them); recorded only as not counted |
| 2 | two | no | in his order: intake 1 then 2, 68 ms apart |
| 3 | two | no | in his order: intake 1 then 2, 128 ms apart (`two-sent-at-once.jpg`, `two-sent.jpg`) |

Every run was `TESTVM_AX_TIMEOUT=35 run-walk.py --wait 3000 ...`; each guest ended with
`clean: app quit, VM stopped, clone deleted, state removed.` In guest 3's screenshot the proposal
leads, the correction carries "Added while Rich was working" and the conversation is titled
"Send proposal today" (nightly 69 titled it "Hold message until approval").

## The step records (the `two` evidence from each in-window run's `report.json`)

```json
{"guest": 1, "conversation_open_when_sent": false, "shown_after_send": [true, true], "received": [{"at": 1792539080916, "intake_id": 1, "text": "Please send the proposal today. ..."}, {"at": 1792539080978, "intake_id": 2, "text": "Correction: do not send it until I approve. ..."}], "composer_after": "", "home_left_after_s": 13.6}
{"guest": 2, "conversation_open_when_sent": false, "shown_after_send": [true, true], "received": [{"at": 1792539386444, "intake_id": 1, "text": "Please send the proposal today. ..."}, {"at": 1792539386512, "intake_id": 2, "text": "Correction: do not send it until I approve. ..."}], "composer_after": "", "home_left_after_s": 11.7}
{"guest": 3, "conversation_open_when_sent": false, "shown_after_send": [true, true], "received": [{"at": 1792539584665, "intake_id": 1, "text": "Please send the proposal today. ..."}, {"at": 1792539584793, "intake_id": 2, "text": "Correction: do not send it until I approve. ..."}], "composer_after": "", "home_left_after_s": 16.5}
```
