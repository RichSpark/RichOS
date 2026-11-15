READY: nightly v1.3.3-nightly.20261115.100 (run 20261115T061911Z-109b9887, source ab438dcb6, build b5154ee32). Evidence: `docs/verification/2026-11-15-nightly-100-candidate-walk/after-update-no-setup-sheet.png`, the update from nightly 99 landing on the conversation with no setup sheet. `engine-update-walk.py`: `setup: PASS`, `update: PASS`, `after: PASS`, `quit: PASS`. The other three walks passed every step: `openings-walk.py` 10 of 10, `loro-stages-walk.py` 4 of 4 and `home-video-walk.py` 5 of 5.

# Nightly 100 candidate walk, 2026-11-15 (Ray)

**How it was tested.** I used the candidate's own zip, `RichOS-1.3.3-nightly.20261115.100-macos-aarch64.zip`. `shasum -a 256 -c SHA256SUMS` reported every file OK. The app ran only in the test VM, through `run-walk.py --no-audio`:
- Every guest was booted with no sound device, so nothing played on this Mac's speakers.
- Each run had one guest, a 1680x1050 guest screen and one window. No test had a second user.

Every claim below was **checked on the live app in the VM**, from its screen, its accessibility tree and its own log.

**Identity.** The app says it was built from `b5154ee32806ba10e44b45520d00eb96ef530494` (`engine-update-walk.py` `after`: `built_from`). That build commit's parent is the source `ab438dcb6` (`git log -1 --format='%H %P' b5154ee32`). The engine it pins is `72523d0dae48…` (`engine-pin.env`). That matches `shasum -a 256 richos-engine-1.2.0.tar.gz` in the candidate folder.

**Tools used.** All are committed tools on this branch, which is richos `ab438dcb6` plus the commits below:
- `testvm/run-walk.py --no-audio`.
- `testvm/openings-walk.py`, `testvm/loro-stages-walk.py`, `testvm/home-video-walk.py` and `testvm/engine-update-walk.py`.
- `qa/contrast.py`, `qa/wait-for.sh` and `qa/ocr-gate.sh`. The gate's positive control passed and 0 of the 9 committed frames were flagged.

**Written from scratch: nothing. Two committed walks were changed:**
- `loro-stages-walk.py` gained a `replay` step. The walk only recorded that "Watch it grow" was on the screen and never pressed it.
- `engine-update-walk.py` now decides from the app's own log, in order, whether a message sent during the install was held. A message counts as held only if it reached the model after the install ended. Before this change, the walk failed whenever the install was faster than a reply. Tests are in `testvm/test/engine-update-walk.test.py`: 10 pass, including the case that rejects a held message.

## The four items

1. **Your openings (round 74): PASS.** `openings-walk.py`, all 10 steps.
   - **The gallery** lists 6 cards: The Ruled Standard, Midnight Suede, One Year with Rich, Bridle Leather, Honed Slate and Blued Steel. It also shows 5 sealed tiles (`openings-gallery.png`).
   - **Play** on Bridle Leather and on the year's edition covers the gallery, ends by itself and brings the gallery back.
   - **Make it my opening** on Bridle Leather: the next start played Bridle Leather.
   - **"One of mine, a different one each day"** can be chosen (`openings-different-one-each-day.png`). The next start played today's opening. With the guest's clock moved one day ahead, the start after that played the next one.
   - **The sound switch:** "Play its sound" under Splash screen starts on. Pressed, it is off and the settings file says `"opening_sound": false`. It is still off in Settings and in the gallery after a restart (`settings-play-its-sound-off.png`). The walk read the switch and the file. It did not listen for sound, because the guest has no sound device.
   - **Save image** wrote a 1600x900 picture to Downloads. The gallery opens over Token spend and gives way to the sheet that Connected folders opens.
2. **Home picture stages (round 75): PASS.** `loro-stages-walk.py`, all 4 steps.
   - At each size, the picture shows its stage and how far away the next one is: 42 memories, "58 to first color"; 114, "186 to the river"; 314, "686 to full color"; 1,010, "1,490 to Self-Made".
   - Each stage's sentence showed as it was crossed: "The first territory is in its own color", "The river is running", "Every territory is in color" (`stages-full-color-1010-memories.png`).
   - **Watch it grow plays:** pressed, the button reads Stop. The line counted up from "Day 1 · 108 memories" to "Day 60 · 550 memories", then reached "Day 105 · 1,010 memories · Full Color" (`watch-it-grow-playing.png`, taken at "Day 79 · 636 memories" during The River). It ended by itself after 25 s and the button read "Watch it grow" again.
   - **Contrast:** "Stop" measured 7.70:1, "Watch it grow" 12.85:1 and the replay line 14.53:1 (`qa/contrast.py`).
3. **Make a video (round 77): PASS on the retry, run alone.** `home-video-walk.py`, all 5 steps.
   - "Make a video" sits at the top right, beside Settings and opens its three choices.
   - **The voice recording** was saved through the Mac's save window: "Saved to Documents as RichOS-day-105-322-memories.mp4". The file is 11.73 s of AAC sound against the sheet's 0:11 (`make-a-video-sheet.png`).
   - **The film** played with the numbers and buttons hidden and ended by itself after 20 s (`make-a-video-film-recording.png`).
   - **Space** on the player paused it and a second Space played it again. The sheet and the home screen stayed up.
   - **No picture in the guest's file.** The guest has no hardware H.264 encoder (the walk's own documented limit), so its file holds sound only. A user's Mac has that encoder. The picture in the file is proved on the host by `ui/tests/home-video.js`.
4. **No setup sheet after an update: PASS.** `engine-update-walk.py`, nightly 99 (`v1.3.2-nightly.20261114.99`) updated to this candidate.
   - App log: "this build's engine is installed and in use, in 14513 ms, with no setup sheet".
   - The installed engine's stamp now carries `72523d0dae48…`.
   - The walk read the screen 7 times through the install and the 15 s after it, by the accessibility tree and by OCR. The setup sheet's title never showed.
   - Voice was offered again twice after the swap, with no relaunch. A message sent right after was answered: "Received." (`after-update-no-setup-sheet.png`).

## What failed first and why it does not block

- **Make a video, run 1: the saved file was shorter than the sheet said.** The sheet said 0:19 and its own player showed 0:13. The file was 13.76 s (`make-a-video-sheet-under-load-0-19-vs-0-13.png`).
  - That run had a second guest running beside it and the Mac was at 88% CPU.
  - Run alone, the same walk on the same candidate gave 11.73 s against 0:11. The engineer's guest run gave 10.39 s against 0:10.
  - **What a user would see if it happens:** the last seconds of a recording missing, on a Mac whose recorder is starved of time. This was seen once, in a VM recording sound only, under load from a second VM. It did not repeat alone.
- **Engine update, runs 1 and 2: the walk failed on its timing, not on a sheet.**
  - Run 1: the install was already over 15 s after going in, before the walk's planned "send a message during the install" moment.
  - Run 2: the message went to the model at `08:14:07.454Z`, before the log line "this build's engine is installed and in use, in 30836 ms". The first text came back at `08:14:12.888Z`. The message was not held. The install simply ended before the 12 s reply finished.
  - Neither run showed the setup sheet. The walk is fixed and its third run passed every step.

## Surfaces applied

Applied:
- Settings, Splash screen: the "Play its sound" switch and "Your openings".
- The gallery's Play, Make it my opening, the daily choice and Save image.
- The home picture at four sizes, Watch it grow and the Make a video button, its choices, the recording, the sheet, the save window and Space.
- The update path from nightly 99 and the conversation after it.
- Returning-user paths: an older user with the old splash-off setting, a relaunch and the next day.

Not applicable:
- Dark mode for the openings: they draw their own dark palette.
- The phones: no `richos/mobile/**` change since nightly 99, so the battery check does not apply.
- Voice talk-over: no barge-in claim was tested.
- The launch speed and the VM boot proof: the build ran both and the brief said not to repeat them.

## Test data and app instances

The test data was invented fixtures only: the "Alder Works" repository written in each disposable guest. Nothing ran on this Mac's screen or in its own app data.

Every run ended with `run-walk.py`'s stop reporting `clean: app quit, VM stopped, clone deleted, state removed.` The guest app pids quit were 5119, 5397, 1634, 1612, 2015 and 1988. In the last engine-update run the walk's own `quit` step quit pid 2104 and checked it was gone. `pgrep -fl richos-tauri` on this Mac found nothing. Both guest slots are free.
