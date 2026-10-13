# Walk of the combined build for the next nightly, interim (2026-10-13)

**Build under test:** richos `e44123519` on branch `cc/ray-opus-test55`. It is main at `470845fbe` with four branches merged in this order:
1. `cc/echo-opus-voice55`;
2. `cc/echo-opus-onboard55`;
3. `cc/echo-opus-hire55` (one conflict, a test-count comment in `richos/app/README.md`);
4. `cc/echo-opus-rclick55`.

The conflict resolution: main 2002 tests, plus 3 from onboard55, plus 1 from hire55, gives 2006 tests.

The bundle was built ad hoc by `package-app.sh` with nightly 54's engine pin. Version: `1.2.0-dev.e44123519`. Each guest's `app.log` reads `this app: built from e441235196904d0525126d7aab7d02d42e6ab388`.

The engine was built by `make-engine-asset.sh` from the same tree, using the verified runtime `runtime-eb026316eb08`. Nothing was uploaded.

**Status: interim, and not the final verdict.** The lead has changed the order. The final build from main also carries the Talk to Rich chain, the dictation fixes, the saved-work fix (`b177f3bf4`) and the whisper-on-launch fix. That build gets one walk of all five areas.

## How it was walked

Every run is a fresh guest with an empty home, run through `run-walk.py`. The app quit, the VM stopped and the clone was deleted at the end of every run.

Toolkit at this tree:
- walks: `setup-walk.py`, `hire-walk.py`, `voice-walk.py`, `steps-walk.py`;
- frame and privacy tools: `qa/frame.py` (`crop`, `motion`), `qa/ocr-gate.sh` (0 of 11 frames flagged, positive control found), `qa/wait-for.sh`;
- the `watch-video` skill for the CEO's recording.

The step lists for the voice check and the CPU check are data passed to `steps-walk.py`. No helper scripts were written.

## Results so far

| Area | Result | Evidence (this folder) |
|---|---|---|
| Animations | **PASS** | `1e-setup-sheet-ring.png`; `1f-spinners-t3.000/.350/.600.png` |
| 1. Feedback items | **PASS**, with one defect raised | `1a`-`1d` |
| 2. Rich's team | **Partly verified** | `2a-hiring-saved-work-unavailable.png` |
| 3. No Approve card or babysitting | **PASS for the hiring** | `go-observed.json` (scratch) |
| 4. Right-click on a thread row | **PASS** | `4a`-`4d` |
| 5. Talk to Rich | **Not yet verified** | `5a-listening-stop-talking.png` |

**Animations.** The setup sheet draws round 19's ring. The guest recording `r19-state2.mov` was cut at its own frame times from 3.000 to 3.600 s. Over that stretch the arc turns about 210°, while the counter reads 909, 914, 917 and then 920 MB. It does not snap back at a reading, and both rows turn together. The guest recorder caught about 14 frames a second, so smoothness at 60 frames a second is not judged.

**1. Feedback items.** Run `setup-walk.py identity,r19-grants,r19-sheet,r19-set-up`, company "Brightwave LABS".
- The rail and Rich's own words both say "Brightwave LABS" (OCR and ledger).
- The first conversation is titled "Onboarding".
- Both of the CEO's new texts appear word for word: the offer copy and the "Good news! Dictation is set up and enabled." message.
- F2 was picked, saved (`key: 2`) and named in that message.
- There is no dictation line under the message box, and the dictation text left the Onboarding conversation.
- Rich's onboarding paragraph is the CEO's copy word for word.
- Defect raised (esc-20261013T210016Z-20625b89): the new "Dictation feature" conversation opens with an "I don't know your business yet / Start the questions" card, while the interview is already running in Onboarding. A fix is in progress.

**2. Rich's team.** `hire-walk.py` runs 1, 3 and 5.
- **PASS:** the interview ends with Rich naming Clark, Dean, Frank, Pierce and Reed, then listing Will and a bookkeeper, then "Say go and I'll get them hired now."
- **PASS:** after "Go." both new teammates were saved to `team/` in all 3 runs, with 0 Approve presses.
- **Not verified:** "still there after a relaunch" and "Rich hands one of them a job". All 3 runs failed at the first read after the relaunch. Cause measured (esc-20261013T222951Z-d927933b): after every relaunch the guest runs 2 to 5 whisper-cli at 50-140% CPU while the home screen animates, so guest idle is 0%. Nightly 54 behaves the same.
- `hire-walk.py` now waits that load out (`dcb8b0196`).
- During the hiring, the side panel said "Saved work is unavailable: A saved work record has an unsupported state." (esc-20261013T203037Z-9b49483d). This is fixed on main by `b177f3bf4` and not re-walked here.

**3. No Approve card or babysitting.** The hiring finished on its own, with 0 Approve presses in 3 runs. The onboarding interview in `setup-walk.py` still clicks "Allow action" if it ever appears. Whether it appeared in this run is not separated out here.

**4. Right-click on a thread row.** A real right mouse button opened Rename… / Pin / Archive on the title, on the empty space and on the `⋯`. Pin from that menu moved the row under Pinned.

**5. Talk to Rich.** The sample is spoken, with pauses of 1.4 s and 1.8 s. It is written to a file with `say -o` and fed in as the microphone through `RICHOS_VOICE_INPUT_WAV`. Nothing was played aloud.
- Its three stretches were held as fragments of one turn.
- Rich stayed silent: `Rich audible=false` throughout.
- The end of the turn came 2.000 s after the last word.
- The stored turn was not seen before the run ended: recognition in the guest took 55 s for 2.9 s of audio. The verdict is left to the final build.
- The "I can't hear anything" line that followed comes from the file's digital silence (below -80 dBFS). A microphone does not produce that, so this is a harness artifact.
