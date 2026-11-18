READY: nightly v1.3.4-nightly.20261118.107 (run 20261118T094247Z-6a923421, source 39fca4f45, build commit 91273b18d).

Both faults the nightly 106 walk found are fixed, as a user meets them:

1. **First capture on macOS 15, after turning screen recording on.** After the user turns RichOS on in System Settings, one Capture Region… now reaches macOS's second prompt within 2 seconds and one Allow brings the capture. The picture that was open is still open and the user never quits RichGlance. This works after Quit & Reopen, after Later with the help panel still up, after Later then Not Now and for the very first capture started from the RichOS shortcut with nothing open. Nothing sat there doing nothing after any press and RichGlance was never left closed or reopening itself.
2. **RichFiles Settings, window gestures.** The three instruction lines now measure 7.07:1 in light and 6.29:1 in dark, above the 4.5:1 floor.

Outside the two checks and not part of this verdict: one older line in the same sheet, "Open RichFiles from Applications or keep it in the Dock." under "Dock and Applications", still measures 3.51:1 light and 4.11:1 dark (the 106 walk's own figures for it). It shipped in earlier nightlies. The section "Outside the brief" below has the details.

# Nightly 107 candidate walk, 2026-11-18 (Ray)

**What was walked.** Only what changed since nightly 106 (source `1dba44fe8`). `git log --first-parent --oneline 1dba44fe8..39fca4f45` lists three merges: the instruction contrast fix (`09a19e52e`), the nightly 106 walk record (`e268af515`) and the RichGlance permission fix (`39fca4f45`, carrying `cf46edd82`). What the nightly 106 walk found working was not walked again. The build's own VM boot proof, new-user walk and launch-speed check were not repeated.

**Identity.**
- `shasum -a 256 -c SHA256SUMS` in `~/.richos-nightly/releases/v1.3.4-nightly.20261118.107`: every file OK.
- The candidate's `RichOS.app` carries `CFBundleShortVersionString 1.3.4-nightly.20261118.107` and `RichOSSourceCommit 91273b18dad01c59e8579c17cac25579ae2cfd43`. `git log --oneline -2 91273b18d` gives `Build v1.3.4-nightly.20261118.107` on top of `39fca4f45` and `git diff --stat 39fca4f45 91273b18d -- richos/tools richos/app/ui richos/app/src-tauri/src` is empty.
- The bundled RichGlance and the RichFiles Window Gestures helper both carry `RichOSSourceRevision 91273b18dad01c59e8579c17cac25579ae2cfd43`. Each guest read RichGlance's revision again before the walk (step 3 of every run).

**How it was tested.**
- Every app ran in the test VM only, through `testvm/run-walk.py --no-audio`, so no guest had a host sound device. Nothing ran on this Mac's screen and nothing took this Mac's keyboard focus.
- The guest is macOS 15.7.7 (`sw_vers`), one 1680x1050 display, light appearance unless a line says dark. RichOS opened at its default window size. One RichOS window; RichGlance, Calculator and System Settings were opened only by the steps that name them. There was no second user.
- In every capture run (A3, B, C, D, step 1), the guest's TCC database returned no screen-recording row for any client containing `richos` before the first press: the guest had never been asked about RichOS or RichGlance.
- The grant was always made the user's way: Open System Settings in macOS's prompt, the RichOS switch in Screen & System Audio Recording, the guest's password in macOS's "Privacy & Security is trying to modify your system settings" dialog, then macOS's "RichOS may not be able to record the contents of your screen until it is quit" alert. After it, the guest's TCC row read `com.richos.app|2|4` (auth_value 2, auth_reason 4) in every run.
- Every press was the real pointer (`pointer` steps: File, then Capture Region… in RichGlance's menu; the buttons in macOS's dialogs), except the RichOS shortcut, which was the ⌃⇧C key chord sent into the guest.
- Claims are **seen on the live app in the guest** (screen, accessibility tree, process list with start times) unless they say otherwise.

**Guest runs** (each one booted, walked and deleted by `run-walk.py`):

| Run | What it did |
|---|---|
| A3 | Item 1, Quit & Reopen, a picture open in RichGlance |
| B | Item 1, Later, Capture Region… chosen again with the help panel still up |
| C | Item 1, Later, then Not Now on the help panel, then Capture Region… |
| D | Item 1, the very first capture from the RichOS shortcut, RichGlance not running and no picture open, Calculator in front; Later |
| G3 | Item 2, RichFiles › Settings › Launch and window, both gesture switches on, Accessibility not granted, light and dark |

Discarded runs, none of them about the candidate: A and A2 (my step lists: a missing path on the scenario command, then the RichOS switch and the password button sought in the wrong process), G and G2 (my step lists: RichOS's first-run "Not now" sheet still up, then RichFiles sought as its own process; it is drawn inside RichOS).

**Tools used.** All committed, at richos `39fca4f45`; nothing was added:
- `testvm/run-walk.py` with `testvm/steps-walk.py` (every step list passed `--check`; the lists stay in my scratch folder because they name scratch paths and the tables below give their steps), `testvm/ax.sh` (`find`, `tree`), `testvm/guest.sh`, `testvm/shot.sh`, `testvm/pointer-drag.sh`, `testvm/slots.py`, `testvm/reserve.py`.
- `qa/contrast.py`, `qa/ocr-gate.sh`.

**Written from scratch: no helper.** Only one-off data: the step lists, a test picture (a blue field with a yellow disc, made with one Pillow command) and a fixture home holding RichOS's shortcut setting.

## Item 1: first capture on macOS 15 after turning screen recording on (fault 1 of the 106 walk)

RichGlance was the copy inside the candidate, opened on the test picture with `open -a` (Finder's Open With path), except in run D.

### Quit & Reopen, a picture open (run A3)

| Check | Result | Evidence |
|---|---|---|
| The first press | File › Capture Region… shows macOS's prompt **"RichOS" would like to record this computer's screen and audio.** with Open System Settings and Deny and RichGlance's help ("RichGlance Needs Screen Recording Permission") below it, covering nothing of it | `01-…` |
| Turning it on | The RichOS switch, the password, then macOS's alert with Later and Quit & Reopen | `02-…` |
| Quit & Reopen | RichOS restarts (pid 1027 → 1743, 11:02:22). RichGlance keeps running (pid 1165) with its picture and its help panel, behind the restarted RichOS window | `03-…`; A3 steps 31 and 36 |
| **One Capture Region… reaches the capture after one Allow, without quitting RichGlance** | **Pass.** Capture Region… chosen at 11:02:37. RichGlance reopened itself (pid 1973, started 11:02:40, arguments `--capture-request --retry-screen-recording 0 /Users/admin/walk/picture.png`). 2 s after the press macOS's second prompt is up: **"RichGlance" is requesting to bypass the system private window picker and directly access your screen and audio.** with Allow and Open System Settings, over the picture window. 3 s after Allow the screen is frozen with the guides, the magnifier and "1680 × 1050 px"; `ax.sh find` reads `Region selection` = `Display 1680 × 1050 px` | `04-…`, `05-…`; A3 steps 44 and 54 |
| The picture that was open is still open | **Pass.** The reopened RichGlance has the `picture.png` window during the alert, during the capture and after Escape | `04-…`, `06-…`; A3 trees `at-glance-bypass`, `at-glance-after` |
| Not reopening itself again | **Pass.** RichGlance pid 1973 with the same start time at Escape and 20 s later, one process | A3 steps 60 and 62 |

The help panel's new text, read in every run: "Allow the app named by macOS under Screen & System Audio Recording, then try again here. RichGlance may need to reopen after the grant, even if macOS restarted RichOS. Captures stay on this Mac."

### Later, help panel still up (run B)

| Check | Result | Evidence |
|---|---|---|
| After Later | RichOS on in the list; RichGlance's help still up below System Settings; no process restarted (RichOS 1042, RichGlance 1191) | `10-…`; B step 35 |
| **Capture Region… with the help still up** | **Pass.** Chosen at 11:08:12. RichGlance reopened itself (pid 1858, 11:08:15) with the picture; 2 s after the press the bypass prompt is up; 3 s after Allow, `Region selection` = `Display 1680 × 1050 px` | `11-…`, `12-…`; B steps 43 and 53 |
| Picture still open, no repeated reopening | **Pass.** `picture.png` open during the capture and after Escape; pid 1858 unchanged 20 s later | B trees `bt-glance-capture`, `bt-glance-after`; steps 59 and 61 |

### Later, then Not Now, then Capture Region… (run C)

| Check | Result | Evidence |
|---|---|---|
| Not Now | The help panel closes; RichGlance keeps its picture window (pid 1180) | `20-…`; C tree `ct-glance-after-not-now` |
| **Capture Region… after Not Now** | **Pass.** Chosen at 11:13:07. RichGlance reopened itself (pid 1945, 11:13:09) with the picture. In the 2 s frame the capture guides and magnifier are already drawn; by 5 s macOS's bypass prompt is in front and the guides are gone. 3 s after Allow, `Region selection` = `Display 1680 × 1050 px` | `21-…`, `22-…`, `23-…`; C steps 48 and 58 |
| Picture still open, no repeated reopening | **Pass.** `picture.png` open after Escape; pid 1945 unchanged 20 s later | C tree `ct-glance-after`; steps 64 and 66 |

The guides showing for a moment before macOS's prompt (run C only; in A3 and B the prompt was already up at 2 s) is the order macOS raises its prompt in, after the first frame is asked for; after Allow the capture came back without another press, so nothing waited on the user.

### The very first capture from the RichOS shortcut, nothing open (run D)

The shortcut was on from the start: the fixture home held RichOS's `richglance-shortcut.json` with ⌃⇧C enabled (the setting itself passed in the 105 walk). **Harness accommodation, declared (as in the 105 and 106 walks):** RichOS runs with its fixture HOME and RichGlance with the guest user's home, so the guest user's `~/Library/Application Support/app.richos.richglance` was made a link to the fixture home's folder. A real user has one home.

| Check | Result | Evidence |
|---|---|---|
| Before | Calculator in front, RichOS's first-run sheet dismissed with Not now, no RichGlance process | `30-…`; D step 9 |
| First press of ⌃⇧C | RichGlance starts (pid 1367, `--capture-request`) and macOS's prompt for RichOS appears with RichGlance's help below it | `31-…`; D step 14 |
| Turning it on, Later | RichOS on; RichGlance's help still up; no process restarted | `32-…`; D step 33 |
| **Second press, Calculator in front again** | **Pass.** ⌃⇧C at 11:12:50 with Calculator frontmost. RichGlance reopened itself (pid 1786, 11:12:51, `--retry-screen-recording 1285`, Calculator's pid). 2 s after the press the bypass prompt is up; 3 s after Allow, `Region selection` = `Display 1680 × 1050 px` | `33-…`, `34-…`; D steps 40 and 49 |
| After Escape | Calculator is the front app again; RichGlance, which had no picture to show, quits | `35-…`; D steps 53 and 55 |
| Not reopening itself | **Pass.** No RichGlance process at Escape, 10 s and 20 s later | D steps 55, 57 and 59 |

## Item 2: the window-gesture instructions in RichFiles Settings (fault 2 of the 106 walk)

Run G3. RichOS › RichFiles (the rail icon) › Settings (gear) › Launch and window, then both switches on: "Double-click a title bar to maximize or restore" and "Click the active app's Dock icon to minimize or restore". macOS showed "Background Items Added". Accessibility was not granted (TCC: `app.richos.richfiles.window-gestures|0`), so all three instruction lines show. Dark is the guest's own appearance switched to dark, with RichOS following it.

`qa/contrast.py` on the full-size frames, one region per line:

| Line | Light (`40-…`) | Dark (`41-…`) |
|---|---|---|
| "These gestures work in other apps and continue after you quit RichOS and RichFiles." | `#53575E` on `#FDFCF8`, **7.07:1, PASS** | `#A0A6B1` on `#182440`, **6.29:1, PASS** |
| "In Desktop & Dock, set "Double-click a window's title bar to" to Do Nothing. Choose your previous action there to restore native behavior." | **7.07:1, PASS** | **6.29:1, PASS** |
| "In Accessibility, add and allow RichFiles Window Gestures to move and minimize other apps' windows." | **7.07:1, PASS** | **6.29:1, PASS** |

Commands: `contrast.py h05-both-on-light.png line1-these-gestures:544,521,1185,543 line2-desktop-and-dock:544,553,1282,598 line3-accessibility:544,650,1197,693` and the same regions on `h06-both-on-dark.png` (the full-size PNGs behind `40-…` and `41-…`). In the 106 walk the same lines measured 3.51:1 and 4.11:1.

## Outside the brief

Seen in the same sheet, not walked as a check and not part of the verdict above: the line under "Dock and Applications", "Open RichFiles from Applications or keep it in the Dock.", is still `#84878D` on `#FDFCF8`, **3.51:1** in light and `#7C8497` on `#182440`, **4.11:1** in dark (`contrast.py … older-dock-and-applications-line:544,282,965,302`). It is below the 4.5:1 floor for text meant to be read. The 106 walk recorded the same color on it and judged it not new; the fix in this candidate changed only the gesture lines.

## Surfaces applied

Applied:
- first capture: RichGlance's File › Capture Region… and the RichOS shortcut ⌃⇧C over Calculator; both macOS prompts; System Settings' switch with Quit & Reopen and with Later; RichGlance's help left up and closed with Not Now; a picture open and nothing open;
- window-gesture instructions: RichFiles Settings inside RichOS, light and dark.

Not applicable or not done:
- **Deny, the help's insets and Escape, the shortcut bringing the help forward:** passed in the 106 walk and not changed since.
- **Try Again on the help panel:** its action changed (it now runs the same path as choosing Capture Region… again); the brief did not list it and it was not pressed.
- **The phones:** no `richos/mobile/**` change in `1dba44fe8..39fca4f45`, so the battery check does not apply.
- **Voice:** nothing in this candidate changes it.
- **RichFiles opened as its own app:** not walked.

## Privacy gate

`qa/ocr-gate.sh` over the 21 source PNGs of this record's JPEGs, with its positive control passing: `0 of 21 frame(s) carry something that must not ship`. `qa/ocr-gate.sh --text` on this record, its positive control passing: `0 of 1 text file(s) carry something that must not ship`.

## Test data and app instances

- **Test data:** one generated picture, RichOS's shortcut setting file and the guests' own settings, in disposable guests only. Nothing touched this Mac's own app data.
- **App instances:** every guest run ended with `clean: app quit, VM stopped, clone deleted, state removed.`
- **Afterward on this Mac:** `ps -axo pid=,comm=` lists no `richos-tauri`, `RichGlance` or `RichFilesWindowGestures` process, `tart list` shows only the base images (no walk clone) and `slots.py status` shows both guest slots free.
