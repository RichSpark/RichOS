NOT READY: nightly v1.3.4-nightly.20261118.106 (run 20261118T063846Z-7660775a, source 1dba44fe8, build commit 017070fe8).

What a user sees:

1. **First capture on macOS 15, after turning screen recording on.** Capture Region… shows macOS's prompt; the user presses Open System Settings and turns RichOS on. macOS then says RichOS may not record until it is quit and offers Later or Quit & Reopen. Whichever the user presses, back in RichGlance **Capture Region… does nothing**: no second prompt, no capture, the same help panel stays. Quit & Reopen restarts RichOS and leaves RichGlance running unchanged. Only after the user quits RichGlance by hand and opens it again does Capture Region… reach macOS's second prompt and then one Allow brings the capture at once. Nothing on screen says it is RichGlance that must be quit. Every new user meets this on their first capture.
2. **RichFiles Settings, window gestures.** The three lines that tell the user what to do ("These gestures work in other apps…", "In Desktop & Dock, set … to Do Nothing…", "In Accessibility, add and allow…") are pale gray: 3.51:1 in light and 4.11:1 in dark, below the 4.5:1 floor.

Everything else in the brief works: the update keeps its Updates row and its moving steps in view with Settings open; the RichGlance sheet is inset like its header and Tab and Shift-Tab stay inside it; macOS's prompts are never covered; after Allow the capture appears without choosing Capture Region… again; after Deny the help shows at once, comes to the front from the menu and from the shortcut over another app and Escape closes it; and the window gestures do what the brief asks in TextEdit, Safari and Terminal, with RichOS and RichFiles quit, after logging out and in and stop when they are turned off. The tables below have the details.

# Nightly 106 candidate walk, 2026-11-18 (Ray)

**What was walked.** Only what changed since nightly 105 (source `c16a18501`), as the brief listed it. `git log --first-parent --oneline c16a18501..1dba44fe8` lists six merges: the nightly 105 walk record, the two nightly 105 fix branches, the merge gate's scratch fix and the two RichFiles window-gesture merges. What the nightly 105 walk found working was not walked again. The build's own VM boot proof, new-user walk and launch-speed check were not repeated.

**Identity.**
- `shasum -a 256 -c SHA256SUMS` in `~/.richos-nightly/releases/v1.3.4-nightly.20261118.106`: every file OK.
- The candidate's `RichOS.app` carries version `1.3.4-nightly.20261118.106` and `RichOSSourceCommit 017070fe8a22`. `git log --oneline -2 017070fe8` gives `Build v1.3.4-nightly.20261118.106` on top of `1dba44fe8` and `git diff --stat 1dba44fe8 017070fe8 -- richos/tools richos/app/ui richos/app/src-tauri/src` is empty.
- The bundled RichGlance and the RichFiles Window Gestures helper both carry `RichOSSourceRevision 017070fe8a22`. In every guest the app's log said `this app: built from 017070fe8a2227b37ef953f8fe63811c320d4e02`.

**Item 1 ran on a development build, not the candidate**, as the brief says (the screens during an update are drawn by the app that is updating). `richos/app/scripts/package-app.sh` ran on this worktree at `1dba44fe8` (clean tree) through `testvm/reserve.py --wait 600`. It made `1.3.4-dev.1dba44fe8` (`RichOSSourceCommit 1dba44fe8968`), ad-hoc signed; the zip's sha256 is `f50303185a72891119acc8d9569f22f1c44a8c27ffc32fadd7b17793d0b1f84c`. That build updated to published nightly 104's own `latest.json`, so a real signed release was downloaded, verified and staged.

**How it was tested.**
- Every app ran in the test VM only, through `testvm/run-walk.py`, with no host sound device (the default; `--no-audio` passed). Nothing ran on this Mac's screen and nothing took this Mac's keyboard focus.
- The guest is macOS 15.7.7 (`sw_vers`), one 1680x1050 non-Retina display, light appearance unless a line says dark. RichOS opened at its default window size, 1400x864. The Dock is at the bottom and always shown (the guest's default).
- One RichOS window; RichGlance, TextEdit, Terminal and Safari were opened only by the steps that name them. There was no second user.
- Claims are **seen on the live app in the guest** (screen, accessibility tree, window frames read through System Events) unless they say otherwise.

**Guest runs** (each one booted, walked and deleted by `run-walk.py`):

| Run | What it did |
|---|---|
| U2 | Item 1: the update with Settings open (`update-relaunch-walk.py --steps install,identity,ready,download`) |
| A | Item 2: the RichGlance sheet, Tab and Shift-Tab, light and dark; item 3's first press |
| A2, A3 | Item 3, Allow path: the macOS prompt, the grant (A2 a stand-in database row, A3 the real System Settings switch and Later) |
| A4 | Item 3, Allow path: the real switch, macOS's Quit & Reopen, then RichGlance quit and opened by hand, then Allow |
| B, B2 | Item 3, Deny path: help at once, insets, a later press, the shortcut over TextEdit, Escape |
| G1c | Item 4: turning the gestures on, the macOS prompts, Do Nothing, the Accessibility grant, RichOS quit, the TextEdit title-bar walks |
| G2 | Item 4: the same set-up, then a guest restart (log out and in), the title bar before RichOS or RichFiles is opened |
| G3 | Item 4: the Dock walks, Window › Fill, Terminal, Safari's text field and tab, the restart again with the title bar and the Dock |
| G4, G5 | Item 4: Safari's title bar with one and two windows; Safari's toolbar buttons |
| G6 | Item 4: turning the gestures off (G3's own attempt after the restart stopped at a keychain dialog the restarted guest put over RichOS, a harness matter) |

Discarded runs, none of them about the candidate: U (the walk's own first-run step no longer matches the first screen; U2 skips it as the nightly 105 walk did), G1 and G1b (my step lists: a toggle clicked while still disabled, a Do Nothing pick that missed its pop-up, AppleScript sent to TextEdit that waited on macOS's Automation prompt) and the first B (the same TextEdit Automation wait, after its Deny evidence was taken).

**Tools used.** All are committed, at richos `1dba44fe8` plus this branch's three tool commits below:
- `testvm/run-walk.py` with `testvm/steps-walk.py` (every step list passed through `--check`; they stay in the scratch record because they name scratch paths and the tables below give their steps), `testvm/update-relaunch-walk.py`, `testvm/ax.sh`, `testvm/guest.sh`, `testvm/shot.sh`, `testvm/pointer-drag.sh`, `testvm/relaunch.py`, `testvm/slots.py`, `testvm/reserve.py`.
- `qa/frame.py` (`edges`, `crop`, `motion`), `qa/contrast.py`, `qa/ocr-find.sh`, `qa/ocr-gate.sh`.

**Toolkit changes on this branch** (each with its own test case, run green):
- `pointer-drag.sh --as-is` (steps-walk `"asis"`): a click where the pointer is, with no app raised first. The Dock minimizes the front app's window and only activates an app in the background, so raising the app before the click changes what the click tests. `test/pointer-drag.test.js`.
- steps-walk: a `pointer` step with `find` and no points clicks the found control (a Dock icon has no fixed place). `test/steps-walk.test.py`.
- `ax.sh --in app`: searches an app that has no window, such as the Dock's icon list. `test/ax-search.test.py`.

**Written from scratch: no helper.** Only one-off data and command lines: the step lists, `run-steps.sh` and `run-u.sh` (one command line each in a file) and two evidence copy lists.

## Item 1: the update with Settings open (fault 1 of the 105 walk)

Run U2, the development build updating to nightly 104, Settings left open after Download update, default window size. The app's own phase timings (`runU2/update-timing.log`): download 77,079 ms, checking 23,081 ms, countdown 3,013 ms.

| Check | Result | Evidence |
|---|---|---|
| Download | The Updates row in the right column reads "Downloading RichOS 1.3.4-nightly.20261117.104…" with "17.5 MB of 160.3 MB" and a filling bar | `01-…` |
| The row stays in view through checking | **Pass.** At 79.2 s after the press the same row, in the same place, reads "Checking and preparing the update…", "RichOS is confirming this update was signed by us.", three steps (Downloaded 160.3 MB ✓, Checking with a turning ring, Restarting by itself) and "Nothing for you to do. RichOS restarts by itself." It is on screen in all 11 checking frames, 031 to 041 (`qa/ocr-find.sh 'Checking and preparing'` hits exactly those 11) | `03-…`, `04-…`, `07-…` |
| Animated steps and elapsed time | **Pass.** The elapsed time under Checking reads 0:01, 0:04, 0:06, 0:08, 0:10, 0:12, 0:14, 0:16, 0:18, 0:19, 0:22 in frames 031 to 041 and the ring's arc is at a different place in each. `qa/frame.py motion` frame to frame: 0.34 to 0.63% of pixels changed every time, so nothing stood still between frames (frames were 1.4 to 2.6 s apart, `runU2/frame-times.tsv`) | `07-update-row-frames-026-to-043.png` |
| Countdown, then the restart | **Pass.** A card over the dimmed window: Downloaded 160.3 MB ✓, Checked signed by us ✓, Restarting now, a ring with "3", "Restarting into RichOS 1.3.4-nightly.20261117.104", "Nothing for you to do. Your window and conversation will be right where you left them." The window is gone in frame 045 (106 s) | `05-…`, `06-…` |
| The update installs | The installed bundle afterward is `1.3.4-nightly.20261117.104`. The walk's own "update activated" log check failed exactly as in the 105 walk: that line is not in the first process's log (a harness gap, already recorded there) | the walk's console line `download: FAIL - the relaunch did not activate the update: no activation line; the installed bundle is 1.3.4-nightly.20261117.104` |

## Item 2: the RichGlance sheet in RichOS Settings (fault 2)

Run A, light and dark.

| Check | Result | Evidence |
|---|---|---|
| Body inset like the header, left and right | **Pass.** The panel's edges are x=266 and x=1465. `qa/frame.py edges` on rows 294 (header text), 378 (the card), 444 (the "Works while RichOS is running" line) and 530 (key caps and Change): the card's left border is at x=299 and its right border at x=1432, a 32 px gap on each side; the header text starts at x=300 and the note at x=301; the key caps start at x=299 and the Change button ends at x=1432 with 32 px to the edge | `10-…`, `12-sheet-left-edge-3x.png`, `13-sheet-right-edge-3x.png` |
| Dark | The same insets; nothing else differs | `11-…` |
| Tab stays inside | **Pass.** `ax.sh --focused` after each of seven Tabs: Close → Capture from any app (switch) → Change → Close → switch → Change → Close → switch | run A `steps.json`, steps 19 to 33 |
| Shift-Tab stays inside | **Pass.** After each of seven Shift-Tabs: Close → Change → switch → Close → Change → switch → Close | run A `steps.json`, steps 35 to 48 |
| Text contrast (light) | Summary 6.33:1, note 16.87:1, hint 6.15:1 (`qa/contrast.py`) | `10-…` |

## Item 3: first capture on macOS 15 (fault 3)

RichGlance opened on a picture with `open -a` on the copy inside the candidate (Finder's Open With path), then File › Capture Region….

| Check | Result | Evidence |
|---|---|---|
| The first macOS prompt, nothing of RichGlance over it | **Pass.** macOS's "Screen Recording" prompt reads **"RichOS" would like to record this computer's screen and audio. Grant access to this application in Privacy & Security settings, located in System Settings.** with Open System Settings and Deny. It is fully visible. RichGlance's help ("RichGlance Needs Screen Recording Permission") sits at the bottom of the screen, below it, overlapping nothing of it | `20-…` (run A; the same in A2, A3, A4, B, B2) |
| Turning it on, the user's way | Open System Settings in the prompt opens Screen & System Audio Recording with RichOS listed and off. Its switch asks for the password ("Privacy & Security is trying to modify your system settings"); after it, macOS says **"RichOS" may not be able to record the contents of your screen until it is quit. You can choose to quit "RichOS" now or do it on your own later.** with Later and Quit & Reopen. TCC then holds `com.richos.app|2|4` (user set) | `28-…`; run A3 step 22 |
| **After Later: Capture Region…** | **Fail.** Back in RichGlance, Capture Region… changes nothing: no second macOS prompt, no capture, the same help stays as it was. Seen 4 s and 20 s after the press (A3); the same after a stand-in database grant (A2) | `29-…`; A3 window list, steps 29 and 36 |
| **After Quit & Reopen: Capture Region…** | **Fail.** Quit & Reopen restarts **RichOS** (pid 1049 → 1456) and leaves **RichGlance running, same pid 1203**. In that RichGlance, Capture Region… again changes nothing, 4 s and 14 s later | `30-…`, `31-…`; A4 steps 15 and 18 |
| After RichGlance itself is quit and opened again | The next Capture Region… shows macOS's second prompt: **"RichGlance" is requesting to bypass the system private window picker and directly access your screen and audio. This will allow RichGlance to record your screen and system audio, including personal or sensitive information that may be visible or audible.** with Allow and Open System Settings | `32-…` |
| **After one Allow, the capture appears without choosing Capture Region… again** | **Pass.** 3 s after Allow the screen is frozen with the guides, the magnifier and "1680 × 1050 px"; `ax.sh find` reads `Region selection` = `Display 1680 × 1050 px`. This is the fault the 105 walk found and it is fixed | `33-…`; A4 step 47 |
| Deny: the recovery help at once | **Pass.** 1 s after Deny the help is on screen: "Allow screen recording for the app named by macOS under Screen & System Audio Recording. If macOS asks you to quit and reopen the app, do that before capturing again. Captures stay on this Mac." with Not Now, Try Again, Open System Settings | `21-…` (run B) |
| Its text and buttons inset from both edges | **Pass.** The panel spans x=620 to 1059. `qa/frame.py edges`: the text runs from x=641 to 1032 (21 px and 27 px from the edges, row 850); the buttons from x=639 (19 px) to 962 (98 px from the right edge, row 916). On its first showing the buttons sit at the left; every later showing draws them at the right (`24-…`, `28-…`). Both inset | `21-…`, `24-…` |
| Help contrast | Light: text 13.13:1, buttons 15.13:1. Dark: text 10.80:1, buttons 5.18:1. The title of the panel while it is not the key window is macOS's inactive-title gray, 1.95:1 (system-drawn, declared, not RichGlance's text); while key it is dark | `21-…`, `22-…` |
| A later Capture Region… brings the help to the front | **Pass.** With a TextEdit window covering it (`23-…`), choosing RichGlance and Capture Region… puts the help in front, key | `24-…` (run B2) |
| The shortcut with another app in front brings it to the front | **Pass.** TextEdit in front again, the help hidden (`25-…`). ⌃⇧C brings the help in front of TextEdit as the key window; TextEdit stays the front app | `26-…` |
| Escape closes it | **Pass.** After Escape the help is gone and RichGlance keeps only its picture window | `27-…`; B2 `b10-richglance.tree` |

**Harness accommodation, declared (as in the 105 walk).** RichOS runs with its fixture HOME and RichGlance with the guest user's home, so for the shortcut one symlink was made: the guest user's `~/Library/Application Support/app.richos.richglance` points to the fixture home's folder. A real user has one home.

## Item 4: RichFiles window gestures (new)

The spec is the RichFiles window-gestures engineering brief of 18 November 2026 in the private record (richos-hq), read in full. Runs G1c, G2, G3, G4, G5 and G6.

**Turning them on and what macOS asks** (G1c):

| Step | What the user sees | Evidence |
|---|---|---|
| RichFiles (rail icon) › Settings › Launch and window | "System-wide window gestures" with two switches, both off: "Double-click a title bar to maximize or restore" and "Click the active app's Dock icon to minimize or restore" | `40-…` |
| Both switched on | RichFiles: "These gestures work in other apps and continue after you quit RichOS and RichFiles."; "In Desktop & Dock, set "Double-click a window's title bar to" to Do Nothing. Choose your previous action there to restore native behavior." with Open Desktop & Dock Settings; "In Accessibility, add and allow RichFiles Window Gestures to move and minimize other apps' windows." with Open Accessibility Settings and Show Helper in Finder | `41-…` |
| macOS prompt 1 | A notification: **Background Items Added. "RichFiles" added items that can run in the background. You can manage these in Login Items & Extensions.** | `42-…` |
| Desktop & Dock | "Double-click a window's title bar to" was **Zoom** (the guest's default); chosen **Do Nothing** there. `defaults read -g AppleActionOnDoubleClick` then reads `None` | `43-…`; G1c step 43 |
| Accessibility | The list already shows **RichFiles Window Gestures**, off; nothing to add by hand | `44-…` |
| macOS prompt 2 | Its switch asks: **Privacy & Security. Privacy & Security is trying to modify your system settings. Enter your password to allow this.** with Modify Settings and Cancel. After the password TCC holds `kTCCServiceAccessibility|app.richos.richfiles.window-gestures|2|4` (user set) | `45-…`; G1c step 65 |
| Input Monitoring | Never asked for: once Accessibility was on, the helper reported `windowGestures.status = running` and RichFiles showed no further step | `46-…`; G1c step 66 |
| RichOS quit from its own menu | No RichOS and no RichFiles process; the helper (`RichFiles Window Gestures.app/…/RichFilesWindowGestures`) still running | `47-…`; G1c step 72 |

**The gestures, with RichOS and RichFiles quit.** Window frames are read through System Events as (x, y, width, height); the usable area is x 0–1680, y 25–965.

| Check | Result | Evidence |
|---|---|---|
| TextEdit: one double-click fills the usable area, not full screen | **Pass.** (200,150,700,500) → (0,25,1680,939), `AXFullScreen` false, menu bar and Dock still shown | `50-…`, `51-…` |
| A second double-click, at another point, restores the exact frame | **Pass.** → (200,150,700,500) | `52-…` |
| Two windows keep their own earlier frames | **Pass.** B (700,300,600,500) filled, A filled, A restored to (200,150,700,500) while B stayed filled, then B restored to (700,300,600,500) | `53-…`, `54-…` |
| Safari: fill and exact restore | **Pass.** (150,120,1000,680) → (0,25,1680,939) from the blank toolbar beside the address field → (150,120,1000,680) | `56-…`, `57-…`, `58-…` (G4) |
| Safari: two windows keep their own frames | **Pass.** Window 2 (400,300,900,600) and window 1 each filled and restored to exactly their own frames | `59-…`; G4 steps 86 to 101 |
| A double-click on a tab, a toolbar button or a text field does nothing extra | **Pass.** The Safari window stayed (150,120,1000,680) after a double-click on the address field (`60-…`), on a tab (`61-…`), on the sidebar button and on Tab Overview (`62-…`) | as listed |
| Ordinary title-bar dragging | **Pass.** A drag from (300,164) to (400,214) moves TextEdit A to (300,200), size unchanged | `55-…` |
| Terminal already filled shrinks, then fills again | **Pass.** (0,25,1677,931) → (210,142,1257,707) → (0,25,1677,931) | `64-…`, `65-…`, `66-…` |
| TextEdit filled with Window › Fill (tiled margins on, the default) shrinks | **Pass.** Fill gives (8,33,1664,924); a double-click gives (210,142,1260,706) | `67-…`, `68-…` |
| Dock: the front window minimizes, alone | **Pass.** A in front of B: one click on TextEdit's Dock icon, A `AXMinimized` true, B false and unmoved | `70-…` |
| Dock: the same window comes back after a switch to another app | **Pass.** Finder brought to the front, then one click on the icon: A `AXMinimized` false, `AXMain` true, at (200,150,700,500); TextEdit in front | `71-…` |
| Dock: an app in the background only activates | **Pass.** Finder in front, no pending restore: one click activates TextEdit, nothing minimized | `72-…` |
| Dock: with B in front, B minimizes and comes back | **Pass.** B minimized alone, then restored at (700,300,600,500) | `73-…` |
| After logging out and in, before RichOS or RichFiles is opened | **Pass.** Walked as a guest restart (`shutdown -r`; the guest logs `admin` back in by itself): boot time changed, launchd shows `app.richos.richfiles.window-gestures` running (pid 431 in G3, 429 in G2), no RichOS or RichFiles process. TextEdit A then filled to (0,25,1680,939) and restored to (200,150,700,500); the Dock icon minimized A and brought it back | `74-…` to `77-…`; G3 steps 157 to 173, G2 steps 90 to 99 |
| Turning the gestures off stops them | **Pass.** RichOS opened again, RichFiles › Settings › Launch and window, both switches off: `windowGestures.selection = 0`, launchd answers `Could not find service "app.richos.richfiles.window-gestures"`, no helper process. RichOS quit again; then a double-click on TextEdit A's title bar left it at (200,150,700,500) and a click on TextEdit's Dock icon minimized nothing | `82-…`, `83-…`, `84-…`; G6 steps 106 to 116 |

**Also seen, not a pass mark of the brief:** with Safari's sidebar open, a double-click on the blank strip above the sidebar left the window as it was (`63-…`). With Do Nothing chosen in Desktop & Dock, that spot now does nothing at all.

**Contrast of RichFiles's new instructions.** `qa/contrast.py` on G1c `g05-both-on.png` (light): the switch labels 18.07:1 and the "System-wide window gestures" heading 6.65:1 pass. The three instruction lines ("These gestures work in other apps…", "In Desktop & Dock, set…", "In Accessibility, add and allow…") are `#84878D` on `#FDFCF8`, **3.51:1, below 4.5:1**. RichFiles's older secondary lines in the same sheet (for example "Open RichFiles from Applications or keep it in the Dock.") measure the same 3.51:1, so the color is not new; the new lines inherit it. Dark (G6 `o01d`): the same lines are `#7C8497` on `#182440`, **4.11:1, also below 4.5:1**; labels 12.06:1 and heading 6.36:1 pass. They are instructions the user has to follow, not fine print. `80-…`, `81-…`

**Not walked, as the author's record also could not:** a second display, any macOS other than 15.7.7 and an auto-hidden Dock (the guest has one display, one macOS and a Dock that is always shown). Also not walked: the two settings for minimized thumbnails, Dock placement left and right, Spaces, rapid clicks, Chrome and Finder.

## Surfaces applied

Applied:
- the update from Settings (the Settings-closed pill passed in the 105 walk and was not re-walked);
- the RichGlance sheet: mouse, Tab, Shift-Tab, Escape, light and dark;
- first capture: File › Capture Region…, the RichOS shortcut over another app, the recovery help's Escape, both macOS prompts, Allow and Deny, System Settings' switch with Later and with Quit & Reopen;
- window gestures: RichFiles Settings inside RichOS (the rail icon), System Settings' Desktop & Dock and Accessibility, real pointer double-clicks, drags and Dock clicks in TextEdit, Safari and Terminal, after quitting RichOS and after a restart and turning them off.

Not applicable or not done:
- **The phones:** no `richos/mobile/**` change in `c16a18501..1dba44fe8`, so the battery check does not apply.
- **Voice:** nothing in this candidate changes it.
- **RichFiles opened as its own app** (not inside RichOS): not walked.
- **Retina display:** the guest has one 1x display.

## Privacy gate

`qa/ocr-gate.sh` was run over the 62 source frames of the JPEGs and the record folder's own PNGs (65 frames), with its positive control passing. It reports three shape hits, each read back with `qa/ocr-read.py` and declared here, none of them a person's detail, so they ship unredacted:
- `runU2/download-frames/042.png` (`05-…`), "11 characters, starting '20…'": the release version "1.3.4-nightly.20261117.104" on the restart card, as in the 105 walk.
- `runG1c/g06-desktop-and-dock.png` (`42-…`) and `g07-do-nothing.png` (`43-…`), "10 characters, starting '0-…'": OCR noise read off the Dock's icon row (`9R08=8O0%o °SSO0- 7920`).

`qa/ocr-gate.sh --text` on this record and `runU2/`: 0 hits, its positive control passing.

## Test data and app instances

- **Test data:** a RichGlance icon picture, three small text and HTML files and the guest's own settings, in disposable guests only. Nothing touched this Mac's own app data.
- **App instances:** every guest run ended with `clean: app quit, VM stopped, clone deleted, state removed.` Inside the guests, TextEdit, Terminal, Safari, RichGlance and System Settings were quit from their own menus.
- **Afterward on this Mac:** `ps -axo pid=,comm=` lists no `richos-tauri`, `RichGlance` or `RichFilesWindowGestures` process, `tart list` has no walk clone and `slots.py status` shows both guest slots free. The development build was copied out of the shared Cargo target into my scratch folder, which is deleted.
