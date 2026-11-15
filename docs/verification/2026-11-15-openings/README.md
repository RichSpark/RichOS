# Round 74's openings on the real app, in the test VM, 2026-11-15

The CEO's order of 2026-11-14 on round 74's five questions: "1: yes. 2: approve. 3: on by default with
the option to turn off in settings; use the 'Two notes' sound. 4: yes. 5: adjust later." And the same
day: "We now also need to remove the settings toggle for disabling the splash screen. Because the
splash screen is now needed for the illusion of a super-fast app launch", "that same switch can be
replaced with the switch for switching off the sound in the splash screen."

Everything here ran in a test VM guest booted with no sound device passed through (`run-walk.py
--no-audio`), macOS 15.7.7, 4 processors, 1680x1050 at 1x, on a synthetic empty home. Nothing ran on
this Mac's screen and nothing was played on it.

## What ran

- **The build:** this branch at `583c4e00f`, `testvm/branch-bundle.sh --release`, version 1.3.3,
  ad-hoc signed, engine 1.2.0.
- **The walk:** `testvm/openings-walk.py` (committed with this work), eight steps, each a real press
  on the app's own window or a read of its own files. Result: `walk-report.json` inside `walk.tgz`. **All eight passed**,
  258 s.
- **The launch speed:** `testvm/launch-speed-walk.py`, unchanged, three runs (below).

## The walk, step by step

| Step | What was done | What the app did |
|---|---|---|
| first | A new user's first start | The splash played (`splash shown, round-11/v1`). The launch record gave him the two openings and nothing else. The sound is on: the settings file says `"opening_sound": true` and the Settings switch reads on (`first-settings-row.jpg`). |
| history | The app quit from its own menu. Its launch record made 430 days old; its settings file given `"splash_enabled": false` and a `splash_disabled_at`, as a user of an older build who had switched the splash off. Started again. | `window: launch kind fresh, splash on` and the splash played from the pictures drawn at the quit (`round-11/v2 (picked at the last quit), its opening used`). Time with Rich earned exactly four more: the year's edition and the leather, slate and steel. |
| gallery | Settings, "Your openings" | Six cards (The Ruled Standard, Midnight Suede, One Year with Rich, Bridle Leather, Honed Slate, Blued Steel), "6 so far", one sealed tile on each ladder that has more and none of the names it must not show (`gallery.jpg`). |
| play | Play on Bridle Leather and on One Year with Rich | Each covered the window with its own screen (`play-material-leather.jpg`, `play-edition-year.jpg`: "1 year", "with Rich, to the day", the bar running) and left by itself; the gallery was there again. |
| pick | "Make it my opening" on Bridle Leather, then the app's own Quit | The settings file says `material-leather`; the page said "Bridle Leather is your opening now." (`picked-leather.jpg`). The quit: `the next start shows opening/material-leather (start 3); its opening was drawn: 11 of 30 pictures over 933 ms ... 2798 ms`. The next start: `splash shown, opening/material-leather (picked at the last quit), its opening used (11 later pictures), the page joined it at 933 ms` (`start-leather.jpg`). |
| daily | "One of mine, a different one each day", quit, start; then the guest's clock a day on, start, quit, start | Day 20,772: the quit picked `opening/ruled-standard` (20,772 mod 6 = 0, the first of his six) and the start played it from its pictures. Day 20,773: the start showed `opening/midnight-suede (picked by the rules at this start), its opening not used: the opening saved at the last quit was another day's` (`daily-next-day.jpg`); the start after that played the same one from the quit's pictures. |
| sound | The Settings switch "Play its sound" pressed off; quit; start | Before: on. After the press the settings file says `"opening_sound": false` and the old `splash_enabled` and `splash_disabled_at` keys are gone from it. After the restart the switch is off in Settings and in the gallery (`sound-on.jpg`, `sound-off.jpg`). |
| save | Save image on Bridle Leather | "Saved to your Downloads folder as RichOS Bridle Leather.jpg." The file: a JPEG, 1600 x 900, 133,214 bytes, with "No. 24 · Bridle Leather · Earned December 11, 2025" at its foot (`saved-bridle-leather.jpg`, the file itself). |

Two things the walk found and this branch fixed before this run: a home with no Downloads folder had
nowhere to put the saved image (the folder is made now) and the image came out 1600 x 872 because
macOS keeps a titled window inside the screen (the hidden window has no title bar now).

## Launch speed: the committed gate, three runs

`launch-speed-walk.py` judges the p95 of every warm launch, from the launch request to the splash
playing, against 380 ms scaled by Dictionary's slowness in the same guest (the CEO: "if the VM is
slower, then the measurements need to be adjusted accordingly"). 20 ordinary launches and 40
rule-switch launches a run, plus 20 cold ones judged on their own.

| Run | Build | p95, 60 warm launches | Limit in that guest | ordinary p95 / median | rule switch p95 / median | cold p95 against its own limit | Verdict |
|---|---|---|---|---|---|---|---|
| 1 | an earlier state of this branch | 374 ms | 709 ms (x 1.87) | 403 / 256 ms | 361 / 241 ms | 2,889 against 3,119 ms | passes |
| 2 | `b7d692a55` | 640 ms | 740 ms (x 1.95) | 442 / 301 ms | 706 / 384 ms | 5,937 against 6,113 ms | passes |
| 3 | `583c4e00f`, the build walked above | 454 ms | 1,089 ms (x 2.87) | 770 / 288 ms | 440 / 277 ms | 2,569 against 5,353 ms | passes |

- The same gate on `4145eb904`, which this branch is built on (its own record,
  `docs/verification/2026-11-14-splash-opening/`): ordinary 320 / 237 ms, rule switch 359 / 254 ms.
  The medians here (256 to 301 and 241 to 384 ms) sit beside those; the p95s move with the host.
- The host carried other agents' work during all three runs. Run 2 also carried this branch's own
  release build, a second guest and its browser suites, which is why it is the slowest; run 3 had
  nothing else of this branch's running and its guest was the slowest of the three by Dictionary's
  own measure (911 ms from the click against 593 and 619).
- Measured against 380 ms unscaled: run 1's p95 is within it (374); runs 2 and 3 are not (640, 454).
- Runs 1 and 2 are builds from before the last two commits, which touch nothing on the launch path
  (the gallery's page script, the saved image's hidden window).
- One start in the walk does not play from saved pictures and it is the first start of a new day
  for a user of "a different one each day": the quit could only draw the day it was on. That start
  showed the splash's ground at 218 ms and the splash itself once the page had drawn it (the page
  took over at 1,655 ms). Every other start in the walk played its pictures (first frame 194 to
  658 ms).

## What this does not cover

- **The sound was not heard.** The guest has no sound device, by order. What is held is that the
  right file is asked for at the right moment (`ui/tests/openings.js` check 3: `two-note.wav` at
  2,070 ms of a 3 s screen, `two-note-grand.wav` on a milestone, nothing when it is off) and that
  the app's webview plays without a click (wry 0.55.1 sets no media type as needing one). Whether it
  sounds right on a real Mac needs a person's ears.
- **The jobs and the hours ladders.** Nothing in RichOS counts jobs completed or hours reclaimed, so
  those ten openings cannot be earned yet; they are drawn and tested and stay sealed.
- **The money ladder on the real app.** The guest's home has no spend, so no opening of AI power was
  earned in the walk; the rule is tested in Rust and the gallery's handling in the browser suite.
- A Retina display, Windows and his own Mac.

## Instances

Every instance the walk started was quit from the app's own menu between its starts; `run-walk.py`
quit the last one by its recorded pid (4492) and ended "clean: app quit, VM stopped, clone deleted,
state removed." Each launch-speed run quit every instance it started by the pid captured at spawn
(123 instances a run, none left running) and ended the same way.

The frames passed the privacy gate (`scripts/qa/ocr-gate.sh`, 0 of 12 frames, 0 of 1 text file).
