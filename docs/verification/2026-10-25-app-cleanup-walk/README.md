# The app's cleanup runs by itself and says when it fails: the second garbage walk (2026-10-25)

The CEO, 2026-10-25: "Make sure Rich fixes this properly and proves the cleanup runs
automatically. I don't want another manual workaround or a reminder telling me something still
needs doing. And make sure this works in the app just as well without any ifs and buts."

This is the real-app check of branch `cc/echo-opus-appclean1`. It ran in the test VM, on fixture
data only (company "Acme", an empty fixture home). Nothing ran on the host's screen and nothing was
played: the spoken turn is a file standing in for the microphone (capture.rs
`RICHOS_VOICE_INPUT_WAV`) and the guest's output was muted (`kill` evidence,
`guest_output_muted`). It adds to `../2026-10-25-app-garbage-walk/` (built at `7f2ca0662`), which
did not have a recording, a split pair or a failed sweep to judge.

## What the packaged app runs and when

No terminal, hook or command is involved. The app calls the reaper of the engine it runs
(`<engine>/scripts/scratch-reaper.sh --apply --app <data folder>`, `app_cleanup.rs`
`Sweeper::command`) at three moments:

| Moment | Where | Shown in this walk |
|---|---|---|
| start | `main.rs` `cleanup(Moment::Start)` at boot | every launch's first `cleanup at start` line (`summary-*.json` `cleanup_lines`) |
| settled | `work_host.rs` `sweep_if_settled`, at the end of every turn of Stu's | `summary-running.json` `kept_while_running.sweeps_after_plant` |
| quit | `main.rs` Exit arm, `Moment::Quit`, after every lease has ended | `summary-quit.json` `cleanup_lines.first`, `cleanup at quit: deleted 27` |

The settled moment did not run before this branch. Run 3 below showed it: the job settled and the
app logged only `cleanup at start`. A job's last steps run as automatic turns (a teammate's
hand-back) and the check sat only after a turn the app sent Stu. Fixed in `8cde5bd71`.

## Build identity

| What | Identity |
|---|---|
| Commit the bundle was built from | `8cde5bd71c611e4d6a41e20e81d9009e71bff89e` (the running app reported it: `report.json` step `identity`, `built_from`) |
| Bundle | `testvm/branch-bundle.sh` output, `RichOS.zip`, sha256 `89a9425c17751295cad41ac73d0e71f2c19df320fe81648bef09b74354e6ee0f`, identity `com.richos.app 1.2.3` |
| Engine | `make-engine-asset.sh --runtime-dir <nightly state>/runtime-eb026316eb08` at `a7c5bae69`, `richos-engine-1.2.0.tar.gz`, sha256 `f5e1851727a7a96b38a0b3f70522cc374c54fd36783aa0a2784bcfdacd4b0707`, handed to the guest with `--engine`. `richos/engine` is unchanged from `a7c5bae69` to `8cde5bd71` (`git diff --stat a7c5bae69..8cde5bd71 -- richos/engine` prints nothing), so this is the engine of the bundle's commit |
| Walk scripts | `garbage-walk.py` at `fc14809a7` (test harness only; the app and the engine are the rows above) |
| Speech model | `ggml-large-v3-turbo-q5_0.bin`, copied into the guest (`stage-voice`), never fetched |
| Guest | `walk-83322aca7700`, 4 CPUs, 5120 MB (`run-report.json`) |

## Command

```
run-walk.py --bundle RichOS.zip --home <empty dir> --engine richos-engine-1.2.0.tar.gz \
  --report run-report.json --wait 1200 -- \
  richos/app/scripts/testvm/garbage-walk.py --out <dir> \
  --expect-sha 8cde5bd71c611e4d6a41e20e81d9009e71bff89e \
  --gone-file richos/app/scripts/testvm/garbage-walk.gone \
  --wav <spoken sample, say -o then afconvert to 16 kHz mono, 2.98 s> \
  --model <ggml-large-v3-turbo-q5_0.bin>
```

`run-report.json`: boot 54 s, scenario 815 s, `scenario_exit` 0, `cleanup_complete` true (app
quit, VM stopped, clone deleted), 873 s in all.

## Every step (`report.json`)

| Step | Outcome | Seconds | What it shows |
|---|---|---|---|
| identity | PASS | 0.1 | the app says it was built from `8cde5bd71` |
| mark | PASS | 0.9 | |
| first-run | PASS | 16.4 | |
| plant-live | PASS | 0.2 | `voice-scratch/utt-1086.wav`, 32044 bytes, named for the RUNNING app (pid 1086) |
| job | PASS | 499.2 | a job for Stu that needs a teammate, settled |
| idle | PASS | 7.0 | |
| list-running | PASS | 3.1 | a complete settled sweep ran after the plant (`kept=29`, `deleted 0`) and the recording and all six profile folders of the running app, each with its own owner record, are still there |
| quit | PASS | 4.9 | the app's own Quit |
| list-quit | PASS | 1.9 | 29 judged paths before, 0 after: 6 profile folders, the recording, 20 MCP logs, 2 session scratch folders |
| stage-voice | PASS | 5.9 | microphone grant, sample and model in |
| restart | PASS | 149.8 | relaunched with the sample as microphone, voice ready |
| list-restart | PASS | 2.9 | |
| kill | PASS | 11.1 | the talk control pressed; SIGKILL to pid 17872 when `whisper-cli ... -f .../voice-scratch/utt-17872.wav` was running; the recording, 119852 bytes, still there after |
| list-kill | PASS | 2.0 | |
| plant | PASS | 0.5 | one pair of the killed app split (its base removed; its `-standing` copy carries its own owner record, pid 17872) and a pair of the killed app whose base holds a mode-000 folder |
| restart-after-kill | PASS | 104.7 | the app's own start sweep, nothing else |
| list-final | PASS | 3.4 | 19 judged paths left by the killed run, 0 after: 4 profile folders (the split copy among them), the recording, 14 MCP logs |
| reported | PASS | 0.6 | below |

The recording's size is the frame math, not a guess: 119852 = 44 header bytes + 59904 samples x 2
bytes and 59904 / 16000 = 3.744 s of 16 kHz mono speech (the 2.98 s sample with the capture's
own lead-in and hangover).

## A failure is reported as a failure (`reported`)

The start sweep after the kill could not delete the base holding the unreadable folder. The app's
log line for it:

```
[richos] cleanup at start: FAILED, deleted 19, undecidable 0, failures 1 (verdict: decided deletable=20 ...)
```

and the engine's deletion log (`reaper-log.txt`) for the planted cases:

```
DELETED .../engine-profiles/1a016d66-...-standing   why=the app that made it (pid 17872) has ended ...
DELETED .../engine-profiles/3b718f82-...-standing   why=the app that made it (pid 17872) has ended ...
DELETED .../voice-scratch/utt-17872.wav             class=app-voice why=a spoken turn's recording: the app that wrote it (pid 17872) has ended
FAILED  .../engine-profiles/3b718f82-...            error=[Errno 13] Permission denied: '.../agents/locked'
```

The split copy went at the first start after the kill; before this branch it had no owner record
of its own and was kept until the 60-minute floor. The undeletable base is still there and is the
only thing the line says FAILED about; everything else the sweep decided was deleted.

## How many runs and why

Five runs of this walk, each a complete run under `run-walk.py --wait 1200`; only the fifth is
recorded here. The first four, kept in the engineer's scratch and deleted with it:

1. `walk-c804627d9108`, bundle `a7c5bae69`: `job` FAIL at 1209 s. Rich's own message on screen:
   "Stu's session stopped because Claude's sign-in token was revoked ('OAuth token revoked. Please
   log in again')". The guest's sign-in, not this branch.
2. `walk-e757c8fdba66`, bundle `a7c5bae69`: `list-running` FAIL, no `cleanup at settled` line.
3. `walk-d04d9bdd7d53`, bundle `a7c5bae69`, with the cleanup lines and the assignment register now
   kept in every summary: the one assignment record said `settled` and the log had `cleanup at
   start` only. That is the settled-moment defect fixed in `8cde5bd71`.
4. `walk-90345680aef3`, bundle `8cde5bd71`: passed through `list-quit` (the settled sweep ran and
   kept the recording; the Quit took 29 of 29), then `restart` FAIL. The home screen was on the
   saved frame and OCR read its door as ". taken») to Rich"; the walk now looks for "to Rich"
   (`fc14809a7`).

## Files

Each file is the walk's own JSON output, unchanged, saved with `.txt` added to its name, as in
the first garbage walk. `report.json` (every step and its evidence), `run-report.json` (timing; the
host lock path written as `<TESTVM_ROOT>`), `facts.json`, `lead-files.json`, `listing-<step>.json`
(every file, link and new directory written since the mark), `summary-<step>.json` (grouped by
place, the processes still running, each run's `cleanup at` lines, the assignment register and the
engine's deletion log tail; for `running` the keep proof, for `quit` and `final` the judgment) and
`reaper-log.txt` (the engine's deletion log at the end). All paths are the guest's. No screenshots.
