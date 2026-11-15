READY: nightly v1.3.4-nightly.20261115.101 (run 20261115T183436Z-87b31e2a, source 89e1fe26e, build 368fc13f4). Evidence: `docs/verification/2026-11-15-nightly-101-candidate-walk/output-side-by-side-light.jpg`, the Output panel's Git tab showing line 2 of a changed file before and after, side by side. `output-walk.py --steps identity,first-run,connect,panel,repo-history,filter,richfiles-history`: all seven steps PASS. The other walks passed every step: `video-anywhere-walk.py` 3 of 3, `rail-walk.py` 3 of 3, `browser-reader-walk.py` 8 of 8 and `meeting-walk.py` 6 of 6.

# Nightly 101 candidate walk, 2026-11-15 (Ray)

**How it was tested.** I used the candidate's own zip, `RichOS-1.3.4-nightly.20261115.101-macos-aarch64.zip`. `shasum -a 256 -c SHA256SUMS` reported every file OK. The app ran only in the test VM, through `run-walk.py`:
- Every guest was booted with no sound device (`--no-audio`), so nothing played on this Mac's speakers.
- Each run had one guest, a 1680x1050 guest screen and one app window. No test had a second user.

Every claim below was **checked on the live app in the VM**, from its screen, its accessibility tree and its own log.

**Identity.** Every walk with an identity step read `built_from 368fc13f45749ba421bf3002520a2d9527cb574c` from the running app's log: `rail-walk.py`, `output-walk.py` and `meeting-walk.py`. That build commit's parent is the source `89e1fe26e` (`git log -1 --format='%H %P' 368fc13f4`). `video-anywhere-walk.py` has no identity step. It ran the same checksummed zip. The browser reader walk ran that zip's own `RichOS.app` and its executable carries the same commit (`strings`).

**Tools used.** All are committed tools on this branch, which is richos `89e1fe26e` plus the commits below:
- `testvm/run-walk.py --no-audio`.
- `testvm/video-anywhere-walk.py`, `testvm/output-walk.py`, `testvm/rail-walk.py`, `testvm/browser-reader-walk.py` and `testvm/meeting-walk.py`.
- `qa/contrast.py`, `qa/wait-for.sh`, `qa/redact.py` and `qa/ocr-gate.sh`.
  - The gate's positive control passed and 0 of the 13 frames were flagged after redaction.
  - Three frames were redacted with `--phones --homes`: dates and commit hashes the gate reads as phone-shaped, the guest's own home path and the Dock.
  - The text gate passed on `browser-reader-report.json.txt`.

**Written from scratch: nothing. Two committed walks were changed:**
- `output-walk.py` gained three steps:
  - `repo-history`: the Output panel's Git tab.
  - `filter`: the filename filter.
  - `richfiles-history`: RichFiles' Git button.
  - No committed walk pressed either Git entrance or typed into the filter. Tests are in `scripts/output-walk.test.py`; 16 pass, including the cases that refuse stacked versions and a comparison without the changed line.
- `browser-reader-walk.py` now pushes an executable that sits inside an app bundle together with its whole bundle.
  - The first run pushed the candidate's `richos-tauri` on its own. Outside its bundle the signed executable no longer matches its signature: `codesign -v` says "invalid Info.plist". It ended before it answered `initialize`.
  - With the bundle pushed, `codesign -v` reads valid in the guest.
  - Tests are in `testvm/test/browser-reader-walk.test.py` (6 pass). The file is now registered in `testvm/test/run-tests.sh`; before this, no runner ran it.

## The six items

1. **Make a video anywhere (round 79): PASS.** `video-anywhere-walk.py`, all 3 steps (seed, button, record).
   - "Make a video" sits at the right end of the Settings heading. Pressed, it opens two choices, Microphone and Mouse pointer and no choice for when the recording stops. Escape closes the choices and Settings stays.
   - Start recording: Settings closes, the count runs and the red Stop sits beside the settings button (`make-a-video-recording-stop.jpg`).
   - The recording carried on through the RichPass page, a real mouse drag that highlighted a line, Settings (whose heading holds the same red Stop, `make-a-video-stop-in-settings.jpg`), Escape and the home screen. On the home screen, Stop took the place of the home screen's own button.
   - Stop opened the sheet: "0:35 · with your voice · with the mouse pointer · 1,400 × 836 · 0.2 MB" (`make-a-video-sheet.jpg`).
   - Save video went through the Mac's save window: "Saved to Documents as RichOS-2026-11-15-at-19.49.15.mp4". Done closed the sheet, Stop was gone and the home screen's own button was back.
   - **What the guest's file holds:** 13.7 s of AAC sound and no picture. This is the walk's documented guest limit: the guest has no hardware H.264 encoder and there its recorder writes part of the sound only. The round 79 record measured the same thing in three guests. The picture and the length are proved on the host by `ui/tests/video-anywhere.js`.
2. **Git view: PASS.** `output-walk.py`, steps `repo-history` and `richfiles-history`.
   - **The Output panel's Git tab.** The Acme repository was connected and given two commits of its own, "Add the tea prices" and then "Raise the black tea price" (line 2 changed).
     - The panel shows "Files 1 | Git". Git opens the view with "This Chat" and "All on main".
     - "All on main" lists all three commits (`output-commits.jpg`).
     - The newer commit lists "tea-prices.md, Modified". Pressed, it shows "2 − Black tea: $5" on the left and "2 + Black tea: $6" on the right, level at y 516: earlier x 701, later x 1121 (`output-side-by-side-light.jpg`, `output-side-by-side-dark.jpg`).
     - Close Git brings the file list back.
   - **RichFiles' Git button.** panel-check.md's More actions, then Show in RichFiles, opened RichFiles in the Acme folder. Its Git button opened the same view for that folder and listed the same commits. The newer commit and its file showed the same two lines side by side, level at y 719: earlier x 894, later x 1218 (`richfiles-side-by-side.jpg`).
   - **Contrast** (`qa/contrast.py`, light then dark):
     - The editor hint "Opens tea-prices.md in the preferred editor, on main.": 5.83:1 and 7.58:1.
     - "Change 1": 7.07:1 and 7.71:1.
     - "Modified": 5.83:1 and 7.58:1.
     - Line numbers: 5.69:1 and 6.46:1.
     - The removed line: 14.53:1 and 12.39:1. The added line: 15.01:1 and 12.19:1.
3. **Filter by filename: PASS.** `output-walk.py`, step `filter`.
   - Rich wrote filter-green.txt, filter-black.txt and filter-white.txt. With panel-check.md, the panel lists 4 files.
   - "black" leaves only filter-black.txt (`output-filter-black.jpg`).
   - "zzz" leaves no row and says "No matching filenames." (`output-filter-no-match.jpg`).
   - Cleared, all four rows are back.
   - **Contrast** (light): the placeholder "Filter by filename" measured 5.83:1 and a row's folder line 4.99:1. The filter in dark mode was not photographed, so it was not measured.
4. **RichCleaner's window: PASS.** `rail-walk.py`, step `cleaner-close`.
   - Opened from the rail, the RichCleaner window closed with a real click on its red button: before, the windows were "RichCleaner, RichOS"; after, only "RichOS".
   - Opened again, it closed with a real Escape (`richcleaner-open.jpg`, `richcleaner-closed-by-escape.jpg`).
5. **Website reader: PASS.** `browser-reader-walk.py`, steps site, chrome, tool, read-safari, read-chrome, states-safari, states-chrome and cleanup.
   - The tool lists exactly its five tools.
   - **Reading:** 12 of 12 pages were read in Safari and 12 of 12 in Chrome. They include a long article, two accordions, a tab list, a table and its "More", a page that loads more as it scrolls, a GitHub repository, a sign-in redirect, a page at an address outside ASCII, the signed-in page, a code sample whose spacing is part of the program and a link that opens in a new tab.
   - **Closes only its own tabs:** at the end the tool said "The tab RichOS opened in Safari is closed." and the same for Google Chrome.
     - Safari's tab bar afterwards showed 2 tabs, "Start Page" and "Signed in". Both were there before the reader started. Every page the reader opened, including the new tab, was gone.
     - Chrome was left with its "Signed in" window, the user's own.
   - **The desktop states:** in both browsers a page was read with every window minimized, with the app hidden, with no window open and with the browser not running.
     - Chrome not running was read on the second open the tool's own refusal asks for. The other seven cases were read on the first open.
     - Windows the browser restored or opened itself when it started (a GitHub window in Safari, "What's New" in Chrome) were left open. This is commit 46b4ff0ff's rule: a window known only as a window is never closed.
   - Report: `browser-reader-report.json.txt`.
6. **Issue #19: PASS.** `meeting-walk.py`, steps identity, companion, audio, first-run, own-mic and prompt.
   - The guest has no sound device, so the walk installs BlackHole 2ch as the guest's microphone. This item could be walked.
   - On the home screen, Make a video with Microphone On: the capture companion's trace shows WebKit's GPU process (`com.apple.WebKit.GPU`, pid 2979) running input, with macOS holding RichOS (pid 2968) responsible for it.
   - Over 20 s the app raised no prompt: ShowPrompt lines 0 before and 0 after, "microphone in use" lines 0 and 0 and "Call detected" was not on screen (`make-a-video-microphone-on-no-call.jpg`).
   - Off and Escape stopped the input and closed the choices.
   - **The control:** another process then took the microphone and the app said "Call detected in pid:3451." with Take Notes. So the detector was working while it stayed quiet for RichOS (`control-another-app-call-detected.jpg`).

## What failed first and why it does not block

- **Git view, runs 1 to 3: the new steps, not the app.**
  - Run 1 looked for the Git tab as a button. It is a toggle (`aria-pressed`, an AXCheckBox in the guest's tree) and it was on screen the whole time.
  - Run 2 found that `connect_folder`'s "Close" (it presses the first control whose name *contains* "Close") had left the Connected folders sheet open. The sheet's own Close, pressed by its exact name, closed it at once. The new steps now do that first and record it.
  - Run 3's filter count picked up a file link outside the panel. The RichFiles step pressed tea-prices.md in the folder list instead of in the Git view's own list.
  - The app did the right thing in every run. Run 4 passed every step.
- **Browser reader, run 1:** the lone executable described above.
- **RichFiles' Git view opens at its least height.** It sits under the folder at its 190-point minimum, which shows about one and a half commits at this window size; its ideal height is 340 (`GitPanel.swift`). It is a split the user can drag and the walk dragged it up with the real mouse. I record it for Urban's judgment, not as a blocker.

## Surfaces applied

Applied:
- Make a video from Settings' heading on the conversation, the RichPass page, Settings and the home screen, its choices, the red Stop, the sheet and the save window.
- The Output panel's Git tab, its two scopes, a commit, a changed file and the comparison in light and dark and Close Git.
- RichFiles' Git button from Show in RichFiles.
- The filename filter: a match, no match and cleared.
- RichCleaner's red button and Escape.
- The browser reader in Safari and Chrome, on the pages and in the four desktop states.
- RichOS's own microphone against another app's microphone.

Not applicable:
- Firefox, Brave and Edge: the reader's record of 2026-11-13 walked all five browsers. A user has one default browser and Safari and Chrome cover the two engines this nightly changed.
- The phones: no `richos/mobile/**` change since nightly 100, so the battery check does not apply.
- Voice talk-over: no barge-in claim was tested.
- The launch speed, the VM boot proof and the new-user walk: the build ran all three and the brief said not to repeat them.

## Test data and app instances

The test data was invented fixtures only: the "Acme" and "Alder Works" repositories written in each disposable guest. Nothing ran on this Mac's screen or in its own app data.

Every run ended with `run-walk.py`'s stop reporting `clean: app quit, VM stopped, clone deleted, state removed.` The guest app pids it quit were:
- 1621 (Make a video).
- 1028 (RichCleaner).
- 1027, 1041 and 1048 (Git view runs 1, 3 and 4). Run 2's log was replaced by its rerun before its pid was copied out; its stop line read `clean: app quit, VM stopped, clone deleted, state removed.` like every other.
- 2968 (issue #19).

The two browser reader runs launched no app. Their tool servers ended with the walk and each guest was deleted.

`pgrep -fl richos-tauri` on this Mac lists two processes of another agent's splash prototype, run from its own folder. Neither is the candidate and this walk started nothing on this Mac. `slots.py status`: both guest slots are free.
