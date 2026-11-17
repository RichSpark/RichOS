NOT READY: nightly v1.3.4-nightly.20261117.103 (run 20261117T194648Z-21e2cf65, source fc696acad, build c4b3a5113). **What a user sees:** in RichGlance's info overlay (round 90), the text drawn over a light part of the picture is close to unreadable. "RichCleaner has saved nothing for this image." on the light band beside a portrait photo measures 1.44:1. It needs 4.5:1. Round 90's design gives this text a heavy black shadow so it stays readable on white. The built viewer's shadow is faint: its darkest pixel is 3.27:1 against the white text and above each letter the ratio is 1.52:1. A saved note shown over a bright photo or over the band beside a portrait one, has the same problem. Evidence: `docs/verification/2026-11-17-nightly-103-candidate-walk/item4-nothing-saved-zoom-3x.jpg`. Items 1, 2, 3, 6 and 7 passed. For item 5, Stu chose the right model and level, as the brief asked and the worker's files reached the Output panel.

# Nightly 103 candidate walk, 2026-11-17 (Ray)

**How it was tested.** I used the candidate's own zip, `RichOS-1.3.4-nightly.20261117.103-macos-aarch64.zip` and its `richos-engine-1.2.0.tar.gz`. `shasum -a 256 -c SHA256SUMS` reported every file in the release folder OK. The app ran only in the test VM, through `run-walk.py`:
- Every guest had no sound device. `--no-audio` was passed and without `--audio` the guest has only tart's null speaker. Nothing ran on this Mac's screen.
- Each run had one guest, a 1680x1050 guest screen, one RichOS window and, where named, one RichGlance or Preview window. No test had a second user.
- The guest ran macOS 15.7.7 in light appearance.

Every claim below was **checked on the live app in the VM**, from its screen, its accessibility tree, the Mac's own default-app answers (`NSWorkspace URLForApplicationToOpenURL`, asked in the guest) and the walks' records, unless it says otherwise.

**Identity.** `build-info.json` names source `fc696acadb6b` and build `c4b3a51130cd`. `output-walk.py`'s identity step read `built from c4b3a51130cd6daf18b615ef95774c51cd7eb67d` from the running app (step `identity: PASS`). The other runs used the same checksummed zip.

**Not repeated:** the build's own VM boot proof, new-user walk and launch-speed check.

**Tools used.** All are committed tools on this branch: richos `fc696acad` plus `e66566c8e`.
- `testvm/run-walk.py`, `testvm/steps-walk.py` (three step lists, each passed through `--check`), `testvm/output-walk.py`, `testvm/launch-log-walk.py`, `testvm/video-anywhere-walk.py` and `testvm/command-walk.py --seed-team`.
- `qa/contrast.py`, `qa/frame.py` (`crop`, `motion`, `px`), `qa/redact.py` and `qa/ocr-gate.sh`. The gate's positive control passed. Three frames carried a date or a guest path in a digit or path shape; `redact.py --phones --homes` covered those and the gate then passed on all 18 frames.

**Written from scratch: no helper.**
- **One committed tool was extended** (`e66566c8e`). `testvm/pointer-drag.sh` gained `--app NAME`, `--scroll LINES --ctrl` and `--double` and `steps-walk.py`'s `pointer` step passes them through. RichGlance is its own process and the pointer tool only reached the pid that `run.sh` recorded. It could not turn the wheel or double-click either, so Control-scroll zoom, the drag that pans and the double-click in RichFiles could not be walked. Tests are in `testvm/test/pointer-drag.test.js` and `testvm/test/steps-walk.test.py`.
- **One-off commands, not helpers:**
  - The fixture images were drawn once with Pillow, `img2webp` and `sips` on this Mac.
  - A RichCleaner catalog row for `01-harbor.png` was written into the guest's `library.sqlite` by a `guest` step. The row has the shape RichCleaner's catalog uses and the viewer reads it the same way. RichCleaner itself was not run.
  - Montages of several frames were made with `ffmpeg xstack`.

## The seven items

1. **RichGlance, the image viewer: PASS.** Run with `steps-walk.py`, step list `glance-1` in the guest.
   - RichFiles (the rail icon, then the folder) › right-click on `01-harbor.png` › **Open With…** opened a sheet listing Preview (Default) and RichGlance (`item1-open-with-sheet.jpg`). RichGlance › Open showed the PNG fitted to a 920x648 window titled `01-harbor.png` (`item1-png-fitted.jpg`).
   - The menu item is "Open With…" and opens a sheet, not an "Open With ›" submenu.
   - Right stepped by file name through the JPEG, GIF, WebP and `.hif` files and skipped `notes.txt`. Right on the last image wrapped to the PNG and Left on the PNG wrapped to the `.hif` (`item1-stepping-jpeg-gif-webp-hif-wrap.jpg`).
   - **The GIF and the WebP animate.** Two frames 2 s apart, with nothing pressed, differ (`frame.py motion`: 26.45% and 26.83% of pixels changed). The labels move from frame 2 to frame 1. The `.hif` file opens.
   - **Zoom and pan:** Control-scroll (8 lines) zoomed the image and a drag panned it.
   - **Escape:** one press while zoomed closed the viewer. RichGlance's process was gone (`richglance-gone`) and RichOS was frontmost, with RichFiles on the same folder.
   - **Selection:** RichFiles kept the selection it had (`02-meadow.jpg`, from the step before) (`item1-zoom-pan-escape-preview.jpg`). A right-click does not select the row clicked. Open With acted on the row clicked, `01-harbor.png`, while the highlight stayed on `02-meadow.jpg`. Recorded for Urban, not a blocker.
   - **File › Open in Preview:** Preview came forward with a window `01-harbor.png`.
2. **RichGlance is optional: PASS.** Before anything was changed, the Mac named Preview for all five images. Double-clicking `01-harbor.png` in RichFiles opened Preview with `01-harbor.png` (`item2-double-click-opens-preview.jpg`). Selecting `02-meadow.jpg` and pressing Enter opened Preview with `02-meadow.jpg` (`item2-enter-opens-preview.jpg`).
3. **The default-viewer switch (round 89): PASS.** Run with step list `glance-2`.
   - RichFiles Settings › **Default image viewer** › "Use RichGlance as my default image viewer" on. Every row then read RichGlance with "Set by RichFiles. Turning it off brings back Preview."
   - The Mac then named the bundled `RichGlance.app` for all five images; `notes.txt` stayed with TextEdit.
   - **No macOS consent prompt appeared.** The photo 2 s after the switch shows none and the guest's visible processes were Finder and RichOS only (`item3-switch-on-no-consent-prompt.jpg`).
   - A double-click on `02-meadow.jpg` in RichFiles then opened RichGlance (`item3-double-click-opens-richglance.jpg`).
   - Switch off: every row read Preview with "Back to Preview." The Mac named Preview for all five again (`item3-switch-off-back-to-preview.jpg`).
   - The standalone RichFiles (RichFiles › Settings… › Default image viewer) shows the same page, reading "Your Mac's current choice." (`item3-standalone-richfiles-page.jpg`).
   - **Contrast** (light): row hints 7.07:1, heading 6.65:1, the app names 18.07:1.
4. **The metadata overlay (round 90): FAIL on contrast. Everything else works.** Run with step lists `glance-1` and `glance-3`.
   - **What works:**
     - Hovering the "i" in the upper left corner showed the saved note, keywords, folder and date, with "Show by default" and "Shortcut I" beside the icon (`item4-hover-overlay.jpg`).
     - A click kept the overlay showing after the pointer left.
     - On `02-meadow.jpg`, with nothing saved, the I key showed "RichCleaner has saved nothing for this image." and a second I hid it.
     - View › Change Info Shortcut… then K made the chip read "Shortcut K" (`item4-shortcut-set-to-k.jpg`). After that, I did nothing and K showed and hid the overlay. Both are read from the accessibility tree.
     - View › Show Info by Default: the next image, with nothing saved, opened clean, as the design asks. Back on the saved image, the overlay showed. Closed and reopened from RichFiles, it showed at once (`item4-reopened-shows-by-default.jpg`).
   - **On record and seen:** a pinned overlay is cleared when stepping to the next image (`item4-hover-pinned-next.jpg`, third panel).
   - **The failure:**
     - The overlay text is white on the picture with a faint shadow (`NSShadow`, black, blur 3, offset 1.5 px down). On the light band beside a portrait image: core text against the band 1.44:1 (`qa/contrast.py`). The darkest shadow pixel under a letter is `#918E86`, 3.27:1 and above the letter the band is 1.52:1 (`qa/frame.py px`, then `qa/contrast.py`) (`item4-nothing-saved-on-light-letterbox.jpg`, `item4-nothing-saved-zoom-3x.jpg`).
     - Round 90's README measured its design's black shadow at 9.03:1 or more at the first pixel over pure white.
     - Over the dark blue test image the same text is 6.94:1, so the overlay reads on dark pictures and fails on light ones.
   - **Other contrast:** the chip label "Show by default" is 8.78:1 and "Shortcut I" is 15.13:1.
   - Dark appearance was not walked. The overlay sits on the image, so its result does not change with the theme. Only the band beside the image turns dark.
5. **Stu names a teammate's model and effort: PASS on the name, the model and the level. The walk's own record check is broken.** Run with `output-walk.py --steps identity,first-run,connect,backend-worker,named --within 1500`, with Mark seeded (`command-walk.py --seed-team`).
   - **The job:** add `notes.md`, make `notes.zip` from it with `git archive`, commit both and land them. That is row 11, "Everyday building", which starts on **Opus, high**. Stu picked Opus, high.
   - **The teammate's name** is `mark-opus-high-4a72167ef26c`: four parts, with the effort word.
   - **The job's run record:**
     - The receipt `inspect` returned and the Agent call as it ran both carry `"model": "opus", "effort": "high"`.
     - Agent answered `resolvedModel: claude-opus-5-5`.
     - `named-observed.json` lists the start as `{name: mark-opus-high-4a72167ef26c, subagent_type: richos-app-engine:mark, model: opus, effort: high}`.
     - The worker's own transcript was not kept by this walk, so the level the model actually answered at is not independently read.
   - **The job ran to the end.** Mark wrote and committed both files, the independent review passed and the job was integrated (merge `62889b0`) and closed.
   - **Do a worker's commands reach the record?** Yes, as far as a user can see. The Output panel of the job's thread read "2 files from this thread": `notes.zip` and `notes.md`, each "~/Acme/ by Mark" (`item5-output-panel-by-mark.jpg`).
   - **The walk reported `backend-worker: FAIL — no worker row reached the record within 1500 s` and `named: FAIL`. That is the walk's fault, not the app's:**
     - The walk reads `<data>/output/<thread>.jsonl` for the thread it noted at first run, `thr_713d…` ("Onboarding") and that file had 0 rows.
     - The message was answered in a new thread, "Add notes.md and notes.zip to Acme repo". That thread's Output panel shows Mark's files.
     - The whole run took 430 s, so the 1500 s wait was never reached: the job settled and the walk read the wrong file.
     - This is the same reading as the engineer's runs today. Recorded for whoever owns `output-walk.py`: it must follow the thread the message went to.
6. **Every launch is kept in the launch log: PASS.** Run with `launch-log-walk.py --launches 2`, `--no-app`, starting the app with `open -a` as the Dock does.
   - `~/Library/Logs/RichOS/launch.log` in the guest held one line per launch, each with "splash started playing": 1384 ms (pid 1079) and 438 ms (pid 1150).
   - Both instances quit from their own menu and were gone. "richos-tauri processes left in the guest: none".
   - The lines also say "engine check refused the engine". That is expected: this walk installs no engine (`--engine none`).
7. **Make a video from a screen other than home: PASS on what a guest can show.** Run with `video-anywhere-walk.py --steps seed,button,record`, every step PASS.
   - Recording started from Settings in a chat and carried across RichPass, Settings and the home screen. Stop ended it, the sheet opened ("0:41 · with your voice · with the mouse pointer · 1,400 × 836") and Save wrote `RichOS-2026-11-17-at-21.06.04.mp4` ("Saved to Documents", `item7-video-saved.jpg`).
   - As on record (`esc-20261117T003635Z-1b28dbd4`), the guest has no hardware video encoder. The file holds AAC sound only: 10.03 s, mean −27.6 dB, decodes without error and no picture. This does not make the candidate NOT READY.

## Surfaces applied

Applied:
- RichFiles docked in RichOS (rail) and standalone (its own app): Open With, double-click, Enter, Settings › Default image viewer, switch on and off, the Mac's answer before, during and after.
- RichGlance: PNG, JPEG, GIF, WebP and HEIF (`.hif`); stepping both ways with wrap; Control-scroll zoom; drag pan; Escape, including while zoomed; File › Open in Preview; the info icon by hover, click, the I key, a changed shortcut and the View menu; Show by default on the same, next and reopened image; an image with nothing saved.
- Stu's team job with a seeded teammate, the Output panel, a Dock-style launch twice, Make a video across screens.
- Light theme throughout.

Not applicable or not done:
- **Dark theme:** not walked, so no dark contrast figure is claimed. The overlay failure is on the image and does not depend on the theme.
- **Formats:** AVIF, TIFF, BMP, ICO and ICNS opening; HEIC with the `.heic` name.
- **Other ways in:** Finder's own Open With and RichGlance opened while RichOS is closed.
- **The phones:** no `richos/mobile/**` change since nightly 102, so the battery check does not apply.
- **Voice talk-over:** nothing in this nightly changes it.

## Test data and app instances

- **Test data:** invented fixtures only, in disposable guests. Five drawn images (`01-harbor.png` 2400x1600, `02-meadow.jpg` 1200x1800, `03-spinner.gif` four frames, `04-wave.webp` four frames, `05-dusk.hif` 2000x1300) and `notes.txt`. One catalog row with a made-up note. A repository "Acme" made by the walk. Nothing touched this Mac's own app data.
- **Guests:** ten were booted (three probes and seven walks). Every run ended with `run-walk.py`'s stop reporting `clean: app quit, VM stopped, clone deleted, state removed.`
- **App instances:**
  - The RichOS pids quit in the guests were 1037, 1057, 1058, 1030, 1043, 1048, 1055 and 1655. `launch-log-walk.py` quit 1079 and 1150 itself.
  - RichGlance and Preview instances were closed by the walks (Escape or Preview's own quit).
  - The standalone RichFiles in run `glance-2` answered its quit with "User canceled". Its guest was then stopped and its clone deleted.
- **After the last run:** `slots.py status` showed both guest slots free and `pgrep -fl richos-tauri` on this Mac listed nothing.

## The step lists

The three step lists were kept with the walk's scratch record and are summarized here:
- `glance-1`: copy the images, seed the catalog, then the Mac's default-app answer for each file. RichFiles from the rail, Edit path to the folder. Double-click and Enter (item 2). Open With › RichGlance, Right ×5, Left, Control-scroll, drag, Escape (item 1). Open With again and File › Open in Preview. Hover, click, Right, I, I (item 4).
- `glance-2`: Settings › Default image viewer, switch on, the default-app answers again, a double-click, switch off, the answers again, then the standalone RichFiles' Settings page.
- `glance-3`: Open With › RichGlance, View › Change Info Shortcut… with K, then I, K, K, View › Show Info by Default, Right, Left, Escape and reopen.

Every step ran with `"allow_fail": true` and recorded its exit status. The pointer steps used the new `double`, `scroll` with `ctrl` and `app` keys.
