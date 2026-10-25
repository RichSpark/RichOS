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

# A force-quit during a dictation (branch `cc/echo-opus-appclean2`, 2026-10-25)

The handover of `cc/echo-opus-appclean1` named the one gap left: one dictation's scratch folder,
`dictation-scratch/<24 hex>/`, carried no owner record, so a kill of the dictation tool while it
decoded left the folder, holding his recorded voice, for good. The fix uses the owner record and
reaper path already in place for engine profiles. The tool writes the record into each folder it
makes (`dictation/scratch.rs`, `676fa555f`). The engine's app sweep decides each folder by that
record (`scratch-reaper.py` `scan_app_dictation`, `ee8a04d51`). A record that is there but cannot
be read is undecidable, never treated as missing (`3f94131b3`) and so is one holding JSON
`null` (`ccb8520e9`, after this walk's bundle). Both are error paths, covered by the unit tests
in `scratch-reaper-app.test.py` and not walked: the walk covers the readable-record path, which
neither commit changes.

## Build identity

| What | Identity |
|---|---|
| Commit the bundle was built from | `ed3d94d0cae9ed3af712a46f94e47f12e8e6ec7d` (the running app reported it: `dictation-report.json` step `identity`) |
| Bundle | `testvm/branch-bundle.sh` output, `RichOS.zip`, sha256 `463a33b6d1e14f3eee55351e24fd96925e173a92a164771453d591a300e3e3cc`, identity `com.richos.app 1.2.3` |
| Engine | `make-engine-asset.sh --runtime-dir <nightly state>/runtime-eb026316eb08` at `ed3d94d0c`, `richos-engine-1.2.0.tar.gz`, sha256 `6a4fdcd8bb08f56a6b899177198750253210587ff7958ff0b4b75512285eee82`, pinned into the bundle and handed to the guest with `--engine` |
| Walk scripts | `garbage-walk.py` at `ed3d94d0c` |
| Speech model | `ggml-large-v3-turbo-q5_0.bin`, copied into the guest (`stage-voice`), never fetched |
| Guest | `walk-49559da06d63`, 4 CPUs, 5120 MB (`dictation-run-report.json`) |

## Command

Only the dictation case and the steps it needs. The earlier cases above are reused, not walked
again (see "What was reused").

```
run-walk.py --bundle RichOS.zip --home <empty dir> --engine richos-engine-1.2.0.tar.gz \
  --report run-report.json --wait 1200 -- \
  richos/app/scripts/testvm/garbage-walk.py --out <dir> \
  --expect-sha ed3d94d0cae9ed3af712a46f94e47f12e8e6ec7d \
  --wav <spoken sample, say -o then afconvert to 16 kHz mono: 36564 frames / 16000 = 2.285 s> \
  --model <ggml-large-v3-turbo-q5_0.bin> \
  --steps identity,first-run,stage-voice,restart-dictation,kill-dictation,plant-live-dictation,quit-dictation,dictation-swept
```

`dictation-run-report.json`: boot 44 s, scenario 165 s, `scenario_exit` 0, `cleanup_complete`
true (app quit, VM stopped, clone deleted), 213 s in all. Nothing was played. The sample is a file
standing in for the microphone (`RICHOS_VOICE_INPUT_WAV`) and the guest's output was muted
(`kill-dictation` evidence, `guest_output_muted`).

## Every step (`dictation-report.json`)

| Step | Outcome | Seconds | What it shows |
|---|---|---|---|
| identity | PASS | 0.2 | the app says it was built from `ed3d94d0c` |
| first-run | PASS | 19.8 | |
| stage-voice | PASS | 3.6 | microphone grant, sample and model in |
| restart-dictation | PASS | 40.9 | Accessibility granted, relaunched with dictation on; tool pid 2245, the app's child, holds F1 and the app connected |
| kill-dictation | PASS | 6.1 | F1, the sample delivered, F1; SIGKILL to the TOOL (pid 2245) when `whisper-cli ... -f .../dictation-scratch/e805a47c51cafdd62045f2c4/utt-2245.wav` was running. The folder was still there with `.richos-owner.json` (pid 2245, the record the tool wrote), `decoder.stderr`, `decoder.stdout` and `utt-2245.wav` |
| plant-live-dictation | PASS | 0.5 | the app started the tool again (pid 2608); a folder `0d61379d867e494683e077c3` with that running tool's owner record and a one-second recording (32044 bytes) |
| quit-dictation | PASS | 3.0 | the app's own Quit; one complete quit sweep, `deleted 19, undecidable 0, failures 0`, `kept=1`; the killed tool's folder DELETED, the running tool's folder still there and not in the deletion log |
| dictation-swept | PASS | 90.8 | relaunched; the start sweep `deleted 1, undecidable 0, failures 0` took the planted folder, whose tool had ended with its app; `dictation-scratch` is empty |

The engine's deletion log (`dictation-reaper-log.txt`) for the two folders:

```
2026-10-25T13:33:26Z DELETED .../dictation-scratch/e805a47c51cafdd62045f2c4 bytes=102592 class=app-dictation why=one dictation's scratch folder: the dictation tool that made it (pid 2245) has ended
2026-10-25T13:33:46Z DELETED .../dictation-scratch/0d61379d867e494683e077c3 bytes=36992 class=app-dictation why=one dictation's scratch folder: the dictation tool that made it (pid 2608) has ended
```

The live folder was planted rather than caught mid-decode. A real one exists only from the
second F1 until the decoder returns and no sweep is timed inside that window. The planted one
carries the same name pattern and the same kind of owner record, naming a tool that was really
running. (The watcher's `wav_bytes` and `wav_mtime` are the folder's own, because its
"recording" argument here is the `dictation-scratch/` folder.)

## What was reused

The cases in the walk above (`8cde5bd71`) were not walked again: the start, settled and quit
moments, profiles, the split pair, a spoken turn's recording, MCP logs and a failed sweep
reported as FAILED. This branch's own changes do not touch the code that decides them.
`git diff --stat 9ad5a334d..ed3d94d0c` (`9ad5a334d` is `cc/echo-opus-appclean1`'s tip, where this
branch starts) lists six files: `dictation/scratch.rs`, `scratch-reaper.py` and its app test, the
pins and `garbage-walk.py` and its test. The `scratch-reaper.py` change is 85 lines added and
none removed (the new `scan_app_dictation`, its one call in `scan_app`, its regular expression
and docstring lines), so `scan_app_profiles`, `scan_app_voice` and `scan_app_mcp_logs` are as
they were. Every sweep in this walk still ran all of those scans (the quit sweep deleted 19
paths) and none failed.

The range `8cde5bd71..9ad5a334d` also carries what main had landed by the time
`cc/echo-opus-appclean1` merged it (for example `main.rs`, `scratch-reaper.sh`). That is main's
own work and is not judged by either walk.

## How many runs and why

Two runs, each a complete run under `run-walk.py --wait 1200`. Only the second is recorded here.
The first is kept in the engineer's scratch and deleted with it.

1. `walk-c96d72f9b94e`, bundle `0dac9ba80`: `kill-dictation` FAIL, "no decoder ran in
   dictation-scratch within 300 s". In fact the watcher had killed the tool: the app started a new
   one a second after the second F1 (`dictation.log` 13:23:26, "dictation tool started"). The
   watcher could not then write what it saw, because `payload/garbage-walk/` is made by `mark`,
   which this subset does not run. A harness fault, fixed in `ed3d94d0c`.
2. `walk-49559da06d63`, bundle `ed3d94d0c`: every step passed. The bundle and the engine were
   built again at `ed3d94d0c` because `3f94131b3` (an unreadable owner record is undecidable)
   changed the engine between the two runs. That change is an error path covered by its unit
   test. This walk exercises the readable-record path.

## Files

`dictation-report.json.txt` (every step and its evidence), `dictation-run-report.json.txt`
(timing; the host lock path written as `<TESTVM_ROOT>`), `dictation-facts.json.txt` and
`dictation-reaper-log.txt` (the engine's deletion log at the end). Each is the walk's own output,
unchanged, with `.txt` added. All paths are the guest's. No screenshots.
