READY: nightly v1.3.4-nightly.20261117.104 (run 20261117T230527Z-1876823f, source f5320650b, build commit ab1513ed6). Nightly 103 was refused because RichGlance's info overlay text was unreadable on light pictures. On this candidate that text now reads clearly on both a white and a dark picture: every measurement is 6.17:1 or higher against the 4.5:1 floor. A line half scrolled out of view at the top keeps its black edge too.

# Nightly 104 candidate walk, 2026-11-18 (Ray)

**What was walked.** Only the round 90 metadata overlay in RichGlance, as the brief asked. Nightly 103's walk (`docs/verification/2026-11-17-nightly-103-candidate-walk.md`) passed items 1, 2, 3, 5, 6 and 7 and they were not walked again. The build's own VM boot proof, new-user walk and launch-speed check were not repeated.

**How it was tested.**
- The candidate's own zip, `RichOS-1.3.4-nightly.20261117.104-macos-aarch64.zip` and its `richos-engine-1.2.0.tar.gz`. `shasum -a 256 -c SHA256SUMS` in the release folder reported every file OK.
- The app ran only in the test VM, in one guest run through `run-walk.py --bundle … --no-audio`. The guest had no sound device and nothing ran on this Mac's screen. Boot took 74 s and the walk 191 s.
- The guest screen was 1680x1050 in light appearance. There was one RichOS window and one RichGlance window at a time and no second user.
- RichGlance was the copy bundled inside the candidate (`RichOS.app/Contents/Applications/RichFiles.app/Contents/Applications/RichGlance.app`), opened with `open -a` on each picture. That is the same path as Finder's Open With. Nightly 103 had already passed opening it from RichFiles.

Every claim below was **checked on the live app in the VM**, from its screen and its accessibility tree, unless it says otherwise.

**Identity.**
- `build-info.json` names source `f5320650bf00`.
- The guest's own bundled RichGlance carries `RichOSSourceRevision ab1513ed6594`, which is the "Build v1.3.4-nightly.20261117.104" commit made on top of the source.
- `richos/tools/richfiles` is the same tree at both commits (`e56c5942a`). Both overlay fixes, `e10dc55bc` and `38fd74da7`, are ancestors of it.

**Tools used.** All are committed tools on richos `f5320650b`:
- `testvm/run-walk.py`, with `testvm/steps-walk.py` running one step list (185 steps, passed through `--check`).
- `testvm/glance-saved-note.sh`, pushed into the guest and run there. It gave the saved notes.
- `qa/contrast.py` (`--edge`, regions and pairs of colors) and `qa/frame.py` (`crop`, `motion`, `edges`, `px`).
- `qa/ocr-gate.sh`. Its positive control passed and it found 0 of 17 frames carrying anything that must not ship.
- `qa/wait-for.sh`.

**Written from scratch: no helper.** These were one-off commands, not helpers:
- The six fixture pictures (1840x1296, pure white and `#121620`) and the two note files were drawn once with Pillow on this Mac.
- The step list was produced by an inline one-time command.
- Two montages for viewing were made with `ffmpeg vstack`. Neither is in the record.

## The checks

Frames are window crops (`frame.py crop 380 98 920 300`) in `docs/verification/2026-11-18-nightly-104-candidate-walk/`. The contrast measure is `contrast.py <over-white> --edge <over-dark> label:box`. It takes each white stroke pixel and compares it with the lightest untouched pixel within 1 px of it, in the frame over the white picture. The floor is 4.5:1.

| Check | Result | Evidence |
|---|---|---|
| The info icon in the upper left corner shows the saved note on hover | PASS: the note "Harbor at first light, kept for the spring catalog." and "Saved Nov 18, 2026 at 12:17 AM" appear, with "Show by default" and "Shortcut I" beside the icon | `1-hover-white-saved-note.png` |
| A click keeps it showing after the pointer leaves | PASS: the real mouse clicked the icon at 405,151, then rested at 1620,700, outside the window. The note stays and the icon reads "Hide RichCleaner Info" in the accessibility tree | `2-click-pinned-pointer-away.png` |
| A second click hides it | PASS: the note is gone and the icon reads "Show RichCleaner Info" | `3-click-again-hidden.png` |
| The I key shows and hides it | PASS: I shows the note ("Hide RichCleaner Info"). A second I hides it and that frame is pixel-identical to the frame of the clicked-hidden state (`frame.py motion`: 0.00% changed) | `4-i-key-white-saved-note.png`, `5-i-key-again-hidden.png` |
| An image with nothing saved says so | PASS: "RichCleaner has saved nothing for this image." (accessibility value of "RichCleaner metadata") | `7-i-key-white-nothing-saved.png` |
| "Saved nothing" line over a white picture | **PASS, 11.82:1** lowest (1st pct 13.51:1, 581 stroke pixels; box 5,69,380,112) | `7-…white-nothing-saved.png` with `8-…dark-nothing-saved.png` |
| "Saved nothing" line over a dark picture | **PASS, 15.50:1** lowest (dark frame as both) | `8-i-key-dark-nothing-saved.png` |
| Saved note over a white picture | **PASS, 10.17:1** lowest (1st pct 13.97:1, 1,256 stroke pixels; box 5,69,500,140) | `4-…white-saved-note.png` with `6-…dark-saved-note.png` |
| Saved note over a dark picture | **PASS, 14.67:1** lowest (dark frame as both) | `6-i-key-dark-saved-note.png` |
| Long note over a white picture, a line partly cut off at the top while scrolling, keeps its black edge | **PASS, 6.17:1 or higher on every one of 17 positions.** One wheel line at a time, 16 turns, a frame after each, the same turns over the dark picture for the pair. Box 40,44,546,84 runs from the clear picture above the note down through the cut line, so a missing edge at the cut would show as white beside white. The cut through the middle of a line is 11.82:1 at turn 4. Only the bottom of a line left is 6.96:1 at turn 5. Only 2 px of descenders left is 6.17:1 at turn 8, the lowest. All 17 positions range from 6.17:1 to 14.17:1 | `9-scroll-04-…`, `10-scroll-05-…`, `11-scroll-08-…` (white, dark and 4x zoom of each) |

The long note was an invented 1,870-character paragraph. Each wheel turn moved the note (`frame.py motion` between consecutive frames: 37 to 39% of pixels changed). The scroller stays clear of the text box: it sits at x 547 to 561 in the crop and every box ends at x 546.

**How the cut-line box was chosen.** `contrast.py --edge` takes the dark frame's most frequent color as the ground. My first box, 5,64,548,82, held only the band below the clip. On turns where a whole line filled it, the most frequent color was the white text itself and the tool printed "dark frame's ground 1.0000" with a meaningless 1.00:1. The box also clipped the bottom of the info icon. So the box was moved up to include the clear picture above the note, 40,44,546,84. With that box every pair read the ground as 0.0081 (`#121620`), which is the picture. Every figure in the table comes from boxes whose printed ground is the picture.

## Seen, not a blocker (for Urban)

- **The icon's circle over a white picture.** While the overlay is showing, the icon is a white circle with a black "i". Over a white picture the circle cannot be seen, so the icon reads as a bare "i". The glyph itself is 19.44:1 against white (`#0D0D0D` on `#FFFFFF`, measured with `frame.py edges`, then `contrast.py`), so the control can still be found and its state is clear from the text below it. When hidden, the circle is `#595959`, 7.00:1 against white and its white "i" is 6.54:1 against the circle.

## Surfaces applied

Applied:
- RichGlance's info overlay by hover, by a real-mouse click and by a second click, by the I key and by a second I.
- A saved note, an image with nothing saved and a long note scrolled with the wheel.
- A white and a dark picture for each.

Not applicable or not done:
- **Dark theme:** not walked. The overlay is drawn on the picture, so the theme does not change these figures.
- **Light band beside a portrait picture:** not walked. Nightly 103's 1.44:1 was measured there. The pure white picture is the harder case for the same text.
- **View › Change Info Shortcut, Show Info by Default and stepping between images:** passed in nightly 103 and not changed since, so they were not walked again.
- **The phones:** no `richos/mobile/**` change in `fc696acad..f5320650b`, so the battery check does not apply.
- **Voice:** nothing in this candidate changes it.

## Test data and app instances

- **Test data:** invented fixtures only, in one disposable guest: six drawn pictures and two invented notes, filed by `glance-saved-note.sh` into the guest user's RichCleaner catalog. Nothing touched this Mac's own app data.
- **RichGlance:** six instances in the guest, pids 1282, 1733, 1833, 2010, 2124 and 2913. Each was quit with its own quit command (`tell application id "app.richos.richglance" to quit`). The step after each one reported `RichGlance-gone-A` through `-F`.
- **RichOS:** the recorded instance, pid 1060 in the guest, was quit by `run-walk.py`. The run ended with `clean: app quit, VM stopped, clone deleted, state removed.` and `slot released: guest.lock after 269s`.
- **After the run:** `slots.py status` showed both guest slots free and `pgrep -fl richos-tauri` and `pgrep -fl RichGlance` on this Mac listed nothing.

## The step list

The step list was kept with the scratch record and is summarized here:
- Push `glance-saved-note.sh` and the fixtures.
- File the short note for `w-note.png` and `d-note.png` and the long note for `w-long.png` and `d-long.png`.
- Read the bundled RichGlance's revision.
- **White saved note:** open, rest the pointer outside the window, then hover the icon (found by `--title "Show RichCleaner Info"`), click, move away, click again, I, I. Quit.
- **Dark saved note, white nothing-saved, dark nothing-saved:** open, rest the pointer outside, I, quit.
- **Long note, white and then dark:** open, I, then 16 times one wheel line toward the user at 700,260 with a frame after each. Quit.

Every input step ran with `"allow_fail": true` and recorded its exit status. All of them exited 0.
