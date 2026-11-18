NOT READY: nightly v1.3.4-nightly.20261118.105 (run 20261118T022442Z-81fb34fd, source c16a18501, build commit 5c41da810).

What a user sees:

1. **The update (round 91), with Settings open.** The download bar fills. Then the Updates row scrolls out of view the moment the download ends. For the whole checking stretch, 17 to 26 seconds, the open Settings panel shows other settings and nothing in it moves. Then the restart card appears. This is the "nothing seems to be happening" stretch round 91 was meant to remove and the row now disappears as well. It happened in both runs with Settings open.
2. **Settings, RichGlance.** The sheet's body sits against the panel's left edge. The "Works while RichOS is running" line, "Your shortcut", the key caps and the card's left border start 2 px from the edge, while the header and the right side have 33 px. The approved round 92 design insets them like the header.
3. **First capture on macOS 15.** macOS asks whether RichGlance may "bypass the system private window picker". After **Allow** nothing happens: no capture appears, even 10 seconds later and the user has to choose Capture Region… again. This is a press that seems to do nothing. It happened in both runs where Allow was pressed.

Everything else in the brief works: the capture itself, the copy and the save, Escape, the shortcut setting and the shortcut over another app. The table below has the details.

# Nightly 105 candidate walk, 2026-11-18 (Ray)

**What was walked.** Only what changed since nightly 104, whose source was `f5320650b`. `git log --first-parent --oneline f5320650b..c16a18501` lists four merges:
- the RichGlance capture gate,
- the round 91 update progress,
- the nightly 104 walk record,
- a Newt changelog.

The build's own VM boot proof, new-user walk and launch-speed check were not repeated.

**Identity.**
- `shasum -a 256 -c SHA256SUMS` in the release folder: every file OK.
- The candidate's `RichOS.app` carries `RichOSSourceCommit 5c41da810d35` and version `1.3.4-nightly.20261118.105`. The bundled RichGlance carries `RichOSSourceRevision 5c41da810d35` and it was read again inside the guest.
- `git log --oneline -2 5c41da810` gives `Build v1.3.4-nightly.20261118.105` on top of `c16a18501`. `git diff --stat c16a18501 5c41da810 -- richos/tools/richfiles richos/app/ui richos/app/src-tauri/src` is empty.

**Item 3 ran on a development build, not the candidate.** As the brief says, the screens during an update are drawn by the app that is updating. So `richos/app/scripts/package-app.sh` was run on this worktree at `c16a18501` (clean tree) through `testvm/reserve.py --wait 600`. It made `1.3.4-dev.c16a18501`, ad-hoc signed. The zip's sha256 is `25210555b8378a2ee5e6e27da8feb90e2310348707405540921cc633b2a64973`. That build updated to published nightly 104's own `latest.json`, so a real signed release was downloaded, verified and staged. The development build differs from the candidate only in its version files.

**How it was tested.**
- Every app ran in the test VM only, through `testvm/run-walk.py`, with no host sound device (the default; `--no-audio` passed). Nothing ran on this Mac's screen.
- The guest screen was 1680x1050, non-Retina, in light appearance. RichOS opened at its default window size, 1400x864.
- There was one RichOS window and at most one RichGlance window and one Calculator at a time. There was no second user.
- There were eight guest runs:
  - P: the permission probe.
  - B: capture, save and Escape (item 1), then the Settings sheet (item 2).
  - D: arrow keys, the plain click and the copy, then the shortcut over Calculator with RichOS minimized.
  - U1 and U1b: the update with Settings open.
  - U2: the update with Settings closed.
  - Two discarded runs:
    - U0 stopped at the walk's stale first-run step.
    - C1 ended when a step of mine pressed Escape on the viewer window. That closes the window and quits RichGlance, as `Viewer.swift:55` makes it do. This is not a change in this candidate.
- Claims are **seen on the live app in the guest** (screen and accessibility tree) unless they say otherwise.

**Tools used.** All are committed, at richos `c16a18501` plus this branch's change below:
- `testvm/run-walk.py` with `testvm/steps-walk.py`, with three step lists, each passed through `--check`. The lists stayed in the scratch record because they name scratch paths. Their steps are summarized in the tables below.
- `testvm/update-relaunch-walk.py`, `testvm/ax.sh`, `testvm/guest.sh`, `testvm/shot.sh`, `testvm/pointer-drag.sh`, `testvm/slots.py` and `testvm/reserve.py`.
- `qa/frame.py` (`motion`, `crop`, `edges`), `qa/contrast.py`, `qa/ocr-find.sh` and `qa/ocr-gate.sh`.

**Toolkit change on this branch.** `testvm/update-relaunch-walk.py` now does three new things:
- It writes each download frame's moment to `download-frames/times.tsv`.
- It keeps the updating app's own phase timings (`update-timing.log`) before any later check can fail.
- It takes `--close-settings`, which shuts the menu right after Download update so the frames show the pill.

Its existing test, `testvm/test/ax-wait.test.py`, passes.

**Written from scratch: no helper.** Only one-off commands and data:
- the stripe fixture picture, drawn once with Pillow;
- the step lists;
- `run-u.sh`, a one-line wrapper that put the update command line in a file. It is not a helper.

## Item 1: RichGlance Capture Region

| Check | Result | Evidence |
|---|---|---|
| First use: the macOS prompt | macOS's "Screen Recording" prompt says: **"RichOS" would like to record this computer's screen and audio. Grant access to this application in Privacy & Security settings, located in System Settings.** Its buttons are Open System Settings and Deny. It names **RichOS**, the app that bundles RichGlance and TCC recorded `com.richos.app`. RichGlance's own explanation opens **on top of** that prompt and covers its title. The explanation says "RichGlance needs your permission to capture the screen … Turn on the app named in the macOS prompt …", with Open System Settings, Try Again and Not Now | `01-…`, `02-…` |
| Turning it on | Stand-in for the user's switch in System Settings: a `kTCCServiceScreenCapture` row for `com.richos.app` written into the guest's TCC database. A guest-only step, declared here | a `guest` step in runs B and D (`sqlite3` INSERT, read back as `com.richos.app|2`) |
| First capture after turning it on | macOS 15 shows "**"RichGlance" is requesting to bypass the system private window picker and directly access your screen and audio.**" with Allow and Open System Settings. **After Allow, no capture appears:** 3 s and 10 s later the screen is unchanged and RichGlance has one window, no overlay. The user must choose Capture Region… again and the next capture works. Seen in runs B and D | `03-…`, `04-…` |
| Freeze, guides, magnifier, size | The screen freezes under the pointer. Thin dashed yellow guides run to the display edges. A round magnifier sits below and left of the point. The readout says "1680 × 1050 px" (accessibility value "Display 1680 × 1050 px") | `05-…` |
| A plain click does not lock a selection | After a click without a drag the value stays "Display 1680 × 1050 px" | `06-…` |
| A drag makes a selection | A drag from 700,400 to 801,457 gives "Crop 101 × 57 px" | `07-…` |
| The arrow keys move the active point by one pixel | Right after the drag gives "Crop 102 × 57 px" | `08-…` |
| Return copies the selection as a PNG | The clipboard holds a 102×57 PNG. It matches the screen at x=700 exactly (`frame.py motion`, step 1: 0 of 5,814 pixels differ, max channel delta 1). x=699 and x=701 differ in every pixel. The fixture's vertical stripes cannot show the y origin. No overlay pixels are in the PNG | `31-copied-png-102x57.png` |
| Save PNG… writes only the selection | `~/Documents/region-save.png` is 100×100 and matches the screen at 600,500 (max channel delta 1; x±1 differ in every pixel). The clipboard still held its marker | `10-…`, `32-saved-png-100x100.png` |
| Escape in the Save panel goes back to the selection | "Crop 101 × 57 px" again, with the action bar shown | `11-…` |
| Start Over | The box is gone, the crosshairs and magnifier are back, "1680 × 1050 px" | `12-…` |
| Escape backs up one step per press and the clipboard is left alone | From the Save panel: press 1 shows the selection ("Crop 100 × 50 px"), press 2 the crosshairs ("Display …"), press 3 exits. The clipboard still reads its marker `MARK-FOUR` and focus is back on RichGlance, the app it started from | `13-…`, `14-…` |
| Text contrast (light) | The readout is 15.31:1. The action bar is 6.83:1 (Copy Image, white on gold) and 18.07:1 (Save PNG…) | `qa/contrast.py` on `09-…` |

## Item 2: the capture shortcut in RichOS Settings

| Check | Result | Evidence |
|---|---|---|
| Off by default | The row reads "RichGlance Off". The sheet says "Off. Capture Region… is always in RichGlance's File menu." and the switch reads 0 | `15-…` |
| Turning it on offers Control-Shift-C | "On. Press ⌃⇧C in any app to capture a region." and "Saved." | `16-…` |
| It can be changed while off | Change, then ⌃⌥G, while off: the key caps show ⌃⌥G and "Saved." Turned on, it says "On. Press ⌃⌥G …". Changed back to ⌃⇧C while on, it saves | `17-…`, `18-…` |
| The shortcut works over another app with RichOS minimized | RichOS's window was minimized (`AXMinimized` true) and Calculator was in front with RichGlance not running. ⌃⇧C cold-launched RichGlance into the capture overlay at the pointer ("Display 1680 × 1050 px"). Escape returned focus to Calculator. A second press worked again | `21-…`, `22-…` |
| Tab after the last control (on record) | **Seen:** Tab from Change moves focus to the window behind (`ax.sh --focused`: AXWebArea "RichOS") | `19-…` |
| **Sheet layout** | **Defect:** the body starts at x=269, 2 px inside the panel edge at x=266. The header text starts at x=299 and the right side has 33 px (`frame.py edges`, rows 294 and 428). Likely cause, from the code: the sheet puts `.dict-col` straight in the panel without the `.dict-body` wrapper that gives every other such sheet `padding: 16px 32px` (`ui/glance-capture-settings.js` `build()`, `ui/style.css` `.dict-body`/`.dict-col`) | `20-sheet-body-flush-left-3x.png`, `16-…`; round 92 `captures/light-12-shortcut-on.jpg` for the approved inset |
| Text contrast (light) | Summary 6.33:1, note 6.20:1, hint 6.15:1, "Saved." 6.15:1 | `qa/contrast.py` on `16-…` |

**Harness accommodation, declared.** In the guest, RichOS runs with its fixture HOME, while RichGlance, opened by Launch Services, runs with the guest user's home. So the shortcut's request ticket lands where RichGlance does not look. In run B the first press over Calculator left the ticket unread and no capture started (run B, step 159: the ticket was still in the fixture home's `Capture Requests` and RichGlance was not running). For run D one symlink was made: the guest user's `~/Library/Application Support/app.richos.richglance` points to the fixture home's folder. A real user has one home and needs nothing like this.

## Item 3: the update, from the press to the restart (round 91)

The app's own phase timings (`update-timing.log`):

| Run | Download | Checking | Countdown |
|---|---|---|---|
| U2 (Settings closed, alone on the host) | 35,757 ms | **21,482 ms** | 3,015 ms |
| U1b (Settings open, beside run D) | 35,849 ms | **26,083 ms** | 3,041 ms |

**The stretch from the end of the download to the restart took 24.5 s and 29.1 s** (checking plus countdown).

| Check | Result | Evidence |
|---|---|---|
| **Settings open: the checking steps after the download** | **Fail.** At 33.0 s the row reads "159.2 MB of 160.3 MB" with focus on the row's line. In the next frame, at 34.4 s, the panel has scrolled to its end. The right column, where Updates was, is empty and the middle column shows RichScribe … Terminal setup. Frames 022 to 031 (34.4 to 52.4 s) differ by only 0.3 to 0.5% each and that is the home screen's animation above the panel. **The panel stood still for about 17 s.** U1b shows the same: frames 020 to 027, about 20 s still. Likely cause, from the code: on entering checking with focus on the row's line, `revealUpdateRow()` sets `#set-menu` `scrollTop` to `scrollHeight` (`ui/updates.js` lines 884 to 893, called at 1202 to 1206). It assumes Updates is the last row. In the three-column layout at the default window size the middle column is taller, so the end of the scroll passes the Updates row. Pressing the pill reveals the row the same way (`openTheRow`, line 874). Not walked; likely the same | `23-…`, `24-…`, `25-…`, `26-…`; `frame.py motion` |
| Settings closed: the pill keeps showing progress | **Pass.** "Downloading 78% / 80% / 90%", then "Updating RichOS · 0:02 · 0:06 · 0:11 · 0:15 · 0:17 · 0:19 · 0:20", with a turning glyph. The pill text is 7.68:1 | `29-…`, `30-…`; `runU2/frame-times.tsv` |
| A countdown precedes the restart | **Pass.** A card over the dimmed window shows Downloaded 160.3 MB ✓, Checked signed by us ✓, Restarting now, a ring with "2", "Restarting into RichOS 1.3.4-nightly.20261117.104" and "Nothing for you to do. …". The text is 12.06:1 and 5.78:1. The window is gone by 55.9 s | `27-…`, `28-…` |
| Did anything stand still for more than about a second? | **With Settings open, yes: the whole visible panel, about 17 to 20 s.** With Settings closed, the pill's number changed in every frame. Frames were 1.4 to 2.6 s apart (`shot.sh` speed), so movement shorter than that cannot be seen in them | as above |
| The update installs | The installed bundle afterward is `1.3.4-nightly.20261117.104` in both U1b and U2. The walk's own "update activated" log check failed: that line is not in the first process's log. This is a harness gap, recorded in the walk's report | `runU*/report.json` |
| "Updated to RichOS" note | **Not walked.** It is drawn by the version updated to. That is nightly 104, which does not have it | n/a |

## Surfaces applied

Applied:
- the File menu path;
- the RichOS shortcut with RichOS minimized over Calculator, cold and warm;
- the mouse (drag, plain click, buttons);
- the keyboard (arrows, Return, Escape, Tab);
- Save and copy output;
- first use with macOS's two prompts;
- the update with Settings open and closed.

Not applicable or not done:
- **Dark theme:** not walked for the sheet or the update card. Light passed every text ratio measured.
- **Retina display and more than one display:** the guest has one 1x display.
- **Tab between corners and M:** not in the brief.
- **Refused permission and retry:** walked only as far as the explanation sheet.
- **The phones:** no `richos/mobile/**` change in `f5320650b..c16a18501`, so the battery check does not apply.
- **Voice:** nothing in this candidate changes it.

## Privacy gate

`qa/ocr-gate.sh` was run over the 28 source frames of the JPEGs, with its positive control passing. Its one hit is `runU1/download-frames/032.png`, "shape match, 11 characters, starting '20…'". That is the release version "1.3.4-nightly.20261117.104" on the restart card, not a person's detail, so it ships unredacted (declared). The PNGs and text files in the folder: 0 hits.

## Test data and app instances

- **Test data:** a drawn stripe picture and clipboard marker strings, in disposable guests only. Nothing touched this Mac's own app data.
- **App instances:** every guest run ended with `clean: app quit, VM stopped, clone deleted, state removed.`. RichGlance and Calculator in run D were quit by their own quit commands ("RichGlance-gone-B", "Calculator-gone").
- **Afterward on this Mac:** `pgrep -fl richos-tauri` and `pgrep -fl RichGlance` list nothing and `slots.py status` shows both guest slots free.
