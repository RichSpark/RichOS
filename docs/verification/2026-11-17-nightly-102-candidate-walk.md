READY on the six walked items: nightly v1.3.4-nightly.20261117.102 (run 20261117T055354Z-892d1779, source b0eb5aa93, build d5f488f87). **Not covered by this verdict: the build's own launch-speed check refused this candidate twice and is being measured again by someone else; publishing waits on that result.** Evidence: `docs/verification/2026-11-17-nightly-102-candidate-walk/effort-reopened.jpg`, Settings opened again with Medium still chosen for the conversation.

# Nightly 102 candidate walk, 2026-11-17 (Ray)

**How it was tested.** I used the candidate's own zip, `RichOS-1.3.4-nightly.20261117.102-macos-aarch64.zip` and its `richos-engine-1.2.0.tar.gz`. `shasum -a 256 -c SHA256SUMS` reported every file OK. The app ran only in the test VM, through `run-walk.py`:
- Every guest was booted with no sound device (`--no-audio`), so nothing played on this Mac's speakers and nothing took its keyboard.
- Each run had one guest, a 1680x1050 guest screen and one app window. No test had a second user.

Every claim below was **checked on the live app in the VM**, from its screen, its accessibility tree, its own files and its own log, unless it says otherwise.

**Identity.** The candidate's `build-info.json` names source `b0eb5aa937cb` and build `d5f488f87c91`. The walks with an identity step (`lease-model-walk.py`, `richfiles-open-walk.py`) read `built from d5f488f87c9176f23eeb3666ce5ee1dea13e1404` from the running app's log. The other walks ran the same checksummed zip.

**The launch-speed check.** The build was still running its own VM checks when this walk started. Its launch-speed check then refused the candidate twice: p95 597 ms over 64 launches (07:27Z) and p95 675 ms over 80 launches (07:45Z), against a limit of 385 ms. Both measurements shared this Mac with a walk guest of mine in the other slot, so they do not show whether the launch is slow or the Mac was busy. I raised it (`esc-20261117T074825Z-46f1aad9`); the check is being taken again with no walk guest beside it, not by me. I did not repeat it.

**Tools used.** All are committed tools on this branch (richos `7a93a8c72` plus the two commits below):
- `testvm/run-walk.py --no-audio`.
- `testvm/lease-model-walk.py`, `testvm/openings-walk.py`, `testvm/keep-alive-walk.py`, `testvm/richfiles-open-walk.py`, `testvm/home-video-walk.py`, `testvm/video-anywhere-walk.py`.
- `testvm/steps-walk.py` with the two step lists printed at the end of this record (`launch-steps.json`, `restart-after-kill-steps.json`), each passed through `steps-walk.py --check`. They are not committed as files: a step list needs the step-list suite's covers row to claim it and a commit guard refuses any change to that row, so neither could be committed.
- `qa/wait-for.sh`, `qa/contrast.py`, `qa/ocr-find.sh`, `qa/timeline.py at` and `qa/ocr-gate.sh`.
  - The gate's positive control passed. 13 of the 14 chosen frames were clean; the one it flagged (a saved file's name, a date it reads as phone-shaped) was left out of the record. The text gate passed on the two step lists.

**Written from scratch: nothing. Two committed walks were changed:**
- `lease-model-walk.py` gained the step `reopen` (commit `3f6063bb2`). The walk pressed a level, closed Settings and read the saved file and the processes; it never opened Settings again, which item 2 asks for. The new step opens Settings again and reads each level button's own checked state. Its reader has a test (`testvm/test/lease-model-walk.test.py`, 6 cases, registered in `testvm/test/run-tests.sh`).
- `home-video-walk.py`: `long` now saves under its own name (second commit). Run after `voice` in one guest, the save window suggested the same name, the Mac asked whether to replace the file and the walk could not press Save.

## The six items

1. **New defaults for Rich and Stu: PASS.** `lease-model-walk.py`, all six steps (identity, first-run, relaunch, job, settings, leases), on a profile that never chose a model.
   - The settings file held no model and no effort key.
   - The conversation's Claude Code process was started with `--model fable --effort low` and both of its answers came from `claude-fable-5-2 effort=low`.
   - The work's process was started with `--model fable --effort medium` and all 30 of its answers came from `claude-fable-5-2 effort=medium`.
2. **The effort setting (round 86.1): PASS.** `lease-model-walk.py --rich fable:medium --stu fable:high`, steps identity, first-run, max-warning, effort, relaunch, job, settings, leases; then a second guest with identity, first-run, effort, reopen.
   - Settings shows "Rich's model and effort" with two rows. Each has an Opus/Fable switch and five levels under it. The default level is marked "Recommended" and the heading reads "Low · recommended" (`effort-default-recommended.jpg`).
   - Pressing Max opened the warning: "WARNING!", "The Max effort level is NOT recommended for anything." with "Use Low, the recommended level" and "Use Max anyway" (`effort-max-warning.jpg`). The first button closed the warning, left Settings open and left Low chosen.
   - Medium was pressed for the conversation and High for the team (`effort-chosen.jpg`). Closing Settings saved `rich_effort_fable: medium` and `stu_effort_fable: high`.
   - Opened again, Settings showed all five levels in each row with exactly one marked: Medium and High (`effort-reopened.jpg`).
   - After a restart the conversation's process ran at `--effort medium` (3 answers at medium) and the work's at `--effort high` (28 answers at high).
   - **Contrast** (`qa/contrast.py`, light): "Recommended" 18.07:1, "Faster replies" 6.35:1, "Generally not recommended." 6.58:1, the warning's "WARNING!" 6.58:1, its subtitle 6.33:1, its body 18.07:1, "Use Low, the recommended level" on its gold button 4.72:1, "Use Max anyway" 18.07:1. Dark mode was not photographed, so it was not measured.
3. **Make a video: PASS on what a guest can show.** `home-video-walk.py --steps seed,voice,long` and `video-anywhere-walk.py --steps seed,button,record,still`, every step PASS.
   - On the home screen and from Settings in a chat: recording started, the count ran, the red Stop showed, Stop ended it, the sheet opened and Save wrote a file through the Mac's save window ("Saved to Documents as …", `video-home-saved.jpg`, `video-anywhere-recording.jpg`).
   - **Lengths the walks measured.** A guest has no hardware video encoder, so each file holds sound only and ends early, as the brief says:

     | Recording | The sheet said | The file |
     |---|---|---|
     | Home screen, short | 0:11 | 11.37 s |
     | Home screen, half a minute | 0:34 | 10.03 s |
     | Across screens, from Settings | 0:35 | 19.84 s |
     | In a chat, nothing moving | 0:32 | 15.60 s |

   - Every file decoded without error and its sound was not silence (mean −27.5 dB). That a file is as long as its recording cannot be judged in a guest.
   - The home screen suggests the same file name for every video that day. A second save asks "already exists. Do you want to replace it?". That is the Mac's own question and nothing is lost unless the user presses Replace; recorded for Urban, not a blocker.
4. **The launch (rounds 84 and 85): PASS.** `openings-walk.py`, all ten steps (first, history, gallery, play, pick, daily, sound, save, spend, sheet); then `steps-walk.py --steps launch-steps.json`: the app quit from its own menu and started again twice, the screen photographed ten times across each start.
   - Both starts logged `splash shown` with the native pack. The first photograph, about a second in, already shows the splash coming up (`launch-1-splash-starting.jpg`); then the full splash (`launch-2-splash.jpg`); then the home screen with its picture starting to bloom (`launch-3-home-arriving.jpg`) and the full picture (`launch-4-home.jpg`).
   - No photograph of either start shows an empty window. The splash shows no spotlight following the pointer.
   - A start after the app was killed, not quit, is a different case (`restart-after-kill-steps.json`): the app shows no splash by its own rule and the window stood plain dark blue for about two seconds before the home screen came up. A user meets this only after a crash or a forced quit. Recorded, not a blocker.
5. **The context meter and Keep chat alive (round 83): PASS.** `keep-alive-walk.py`, all eight steps (setup, talk, meter, keep, idle, keepalive, unseen, after).
   - With the Settings switch on, the chat's top bar reads "Rich 6% 63k/1000k" and "Team … not read yet". The figure equals the last request in Rich's own transcript (63,074).
   - "Keep chat alive" is offered in the chat's top bar and stays checked after a restart (`context-meter-keep-chat-alive.jpg`).
   - After 55 idle minutes (seeded) the session answered one keep-alive with one word, no tool call and nothing about it appeared in the chat. The next message was answered with 97.7% of its request read from the cache.
   - **Contrast** (light): the meter's labels and figures 5.83:1, its percentage 14.90:1, "Keep chat alive" 14.90:1. Dark mode was not photographed, so it was not measured.
   - With the meter on, the chat's title and "Keep chat alive" stack on two lines at this window width. Readable; recorded for Urban.
6. **RichFiles on the first click: PASS, with one thing the lead should see.** `richfiles-open-walk.py`, steps identity, first-run, first-open, again, after-restart (all PASS) and early-open (see below).
   - **The first click ever:** "Opening RichFiles…" was on screen in the frame taken 0.46 to 0.50 s after the click (`richfiles-first-click-loading.jpg`) and RichFiles itself 1.17 to 1.65 s after it (`richfiles-first-click-open.jpg`). The click no longer looks dead.
   - **It is not faster than nightly 101 in a guest.** The fix's own record measured nightly 101's first click at 1.02 to 1.24 s. This candidate loads RichFiles' library ahead of the click (the app's line: loaded 7.1 s after the start) but builds its view on the click and shows the loading line while it does.
   - The second click in the same process: 0 to 0.25 s. The first click after a restart: loading line by 0.14 s, RichFiles 0.52 to 0.76 s.
   - **A click seconds after the start (`early-open`):** in the second run the loading line was on screen 0.06 to 0.12 s after the click and RichFiles 0.94 to 1.53 s after it.
   - **The walk reported FAIL for `early-open` both times and neither is the app's.** Run 1: its click came 0.4 s after Talk to Rich was pressed, the home screen was still up in the frame before it and the rail icon was not there yet to be clicked. Run 2: it said "no frame shows the loading line" and `qa/ocr-find.sh 'Opening RichFiles'` reads the line on four of its frames. The walk's own reader misses the line; it also missed it in `first-open` and `after-restart`. That reader needs fixing before the step can be trusted.

## Surfaces applied

Applied:
- A profile that never chose a model and one that chose levels: Settings, the saved file, the running processes and the answers.
- The effort control's five levels in both rows, the Recommended mark, the Max warning and its first button, closing and reopening Settings, a restart.
- Make a video from the home screen and from Settings in a chat, across the RichPass page, Settings and the home screen; Stop, the sheet and the save window.
- The start after an ordinary quit, twice and after a kill; the openings gallery, Play, a picked opening, a different one each day, the sound switch.
- The context meter's switch, the meter in a chat, Keep chat alive before and after a restart.
- RichFiles from the rail: first click ever, second click, first click after a restart and a click seconds after the start.
- Light theme throughout.

Not applicable or not done:
- Dark theme for the new Settings rows, the warning and the meter: not photographed, so no dark contrast figure is claimed.
- The phones: no `richos/mobile/**` change since nightly 101, so the battery check does not apply.
- Voice talk-over: nothing in this nightly changes it.
- The VM boot proof, the new-user walk and the launch-speed check: the build's own, not repeated. The launch-speed result is open, as above.
- RichGlance (merged after this candidate's source) is not in this build.

## Test data and app instances

The test data was invented fixtures only, written in each disposable guest. Nothing ran on this Mac's screen or in its own app data.

Thirteen guests were booted. Every run ended with `run-walk.py`'s stop reporting `clean: app quit, VM stopped, clone deleted, state removed.` The guest app pids it quit were 1476, 1951, 1046, 1049 (defaults and effort), 4735 (openings), 3469 (keep-alive), 1719 and 1776 (the two launch runs), 3423 and 2357 (RichFiles), 1652 and 1619 (home video) and 1613 (video anywhere).

After the last run `slots.py status` showed both guest slots free and `pgrep -fl richos-tauri` on this Mac listed nothing.

## The two step lists

`launch-steps.json`, run twice over in one guest (the quit, the start and ten photographs, then the app's own launch lines):

```json
[
 {"op": "wait", "seconds": 25},
 {"op": "guest", "command": "pid=$(pgrep -f 'RichOS.app/Contents/MacOS' | head -1); osascript -e \"tell application \\\"System Events\\\" to tell (first process whose unix id is $pid) to click menu item \\\"Quit RichOS\\\" of menu 1 of menu bar item 2 of menu bar 1\"; sleep 4; pgrep -f 'RichOS.app/Contents/MacOS' || echo quit"},
 {"op": "relaunch"},
 {"op": "shot", "name": "launch-a-01"},
 {"op": "shot", "name": "launch-a-02"},
 {"op": "shot", "name": "launch-a-03"},
 {"op": "shot", "name": "launch-a-04"},
 {"op": "shot", "name": "launch-a-05"},
 {"op": "shot", "name": "launch-a-06"},
 {"op": "shot", "name": "launch-a-07"},
 {"op": "shot", "name": "launch-a-08"},
 {"op": "shot", "name": "launch-a-09"},
 {"op": "shot", "name": "launch-a-10"},
 {"op": "guest", "command": "grep -h 'launch timing\\|window: launch kind' /Users/admin/testvm/walk-*/relaunch-*.log | tail -2"}
]
```

The list as run repeats everything after the wait once more, with the photographs named `launch-b-01` to `launch-b-10`.

`restart-after-kill-steps.json` is the same list without the two quit steps: `relaunch` alone stops the app by its pid, which the app reads as a crash.
