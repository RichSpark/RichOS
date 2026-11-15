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

- **The build:** this branch at `ba53bfa6b`, `testvm/branch-bundle.sh --release`, version 1.3.3,
  ad-hoc signed, engine 1.2.0.
- **The walk:** `testvm/openings-walk.py` (committed with this work), ten steps, each a real press
  on the app's own window or a read of its own files. Result: `walk-report.json` inside `walk.tgz`.
  **All ten passed**, 212 s.
- **The launch speed:** `testvm/launch-speed-walk.py` with its new day-change route, on `408e628f9`.
  The two commits after it change the gallery's page script and Token spend's (which sheet gives
  way to which) and nothing on the launch path. Result: `launch-speed-report.json` inside `walk.tgz`. **It passes.**

## The walk, step by step

| Step | What was done | What the app did |
|---|---|---|
| first | A new user's first start | The splash played (`splash shown, round-11/v1`). The launch record gave him the two openings and nothing else. The sound is on: the settings file says `"opening_sound": true` and the Settings switch reads on (`first-settings-row.jpg`). |
| history | The app quit from its own menu. Its launch record made 430 days old; its settings file given `"splash_enabled": false` and a `splash_disabled_at`, as a user of an older build who had switched the splash off. Started again. | `window: launch kind fresh, splash on` and the splash played from the pictures drawn at the quit. Time with Rich earned exactly four more: the year's edition and the leather, slate and steel. |
| gallery | Settings, "Your openings" | Six cards (The Ruled Standard, Midnight Suede, One Year with Rich, Bridle Leather, Honed Slate, Blued Steel), "6 so far", one sealed tile on each ladder that has more and none of the names it must not show (`gallery.jpg`). |
| play | Play on Bridle Leather and on One Year with Rich | Each covered the window with its own screen (`play-material-leather.jpg`, `play-edition-year.jpg`) and left by itself; the gallery was there again. |
| pick | "Make it my opening" on Bridle Leather, then the app's own Quit | The settings file says `material-leather` (`picked-leather.jpg`). The quit picked `opening/material-leather` and drew its opening; the next start played it from those pictures (`start-leather.jpg`). |
| daily | "One of mine, a different one each day", quit, start; then the guest's clock a day on, start, quit, start | Below. |
| sound | The Settings switch "Play its sound" pressed off; quit; start | Before: on. After the press the settings file says `"opening_sound": false` and the old `splash_enabled` and `splash_disabled_at` keys are gone from it. After the restart the switch is off in Settings and in the gallery (`sound-on.jpg`, `sound-off.jpg`). |
| save | Save image on Bridle Leather | "Saved to your Downloads folder as RichOS Bridle Leather.jpg." The file: a JPEG, 1600 x 900 (`saved-bridle-leather.jpg`, the file itself). |
| spend | Token spend opened from Settings; then Settings, "Your openings" over it | The gallery is on the screen and Token spend is gone from under it (`spend-open.jpg`, `gallery-over-spend.jpg`). The same step on the build before this fix (`408e628f9`) fails: "Token spend is still up with the gallery open: the gallery is under it". |
| sheet | With the gallery up: Settings, "Connected folders (repositories)" | The gallery gives way and the sheet is on the screen (`sheet-over-gallery.jpg`). The same step on the build before this fix (`73b752872`) fails: "the gallery is still up after Connected folders was pressed: its sheet is under the gallery". |

### A different one each day, across a change of day

The mid-job review found that the first start of a new day showed bare ground until the page drew the
splash: 1,726 ms in this guest, against 265 ms the same day. The quit now draws the next day's
opening beside today's and a later day opens on the one after the last (so a Monday follows a
Friday's).

| Moment | The app's own line |
|---|---|
| The quit on day 20,772 | `the next start shows opening/ruled-standard (start 4); its opening was drawn: 30 of 30 pictures ... 1180 ms` and `a start on a later day shows opening/midnight-suede; its opening was drawn: 30 of 30 pictures ... 1740 ms` |
| The start the same day | `first frame 151 ms ... splash shown, opening/ruled-standard (picked at the last quit), its opening used (30 later pictures)` |
| The first start of day 20,773 | `first frame 173 ms ... splash shown, opening/midnight-suede (picked at the last quit), its opening used (30 later pictures)` (`daily-next-day.jpg`) |
| The quit on day 20,773 | `the next start shows opening/midnight-suede ... 2174 ms` and `a start on a later day shows opening/edition-year ... 1986 ms` |

What it costs: the first quit of a day for such a user draws two openings, 2.9 s and 4.2 s in all
here, with no window on screen; later quits that day find both saved and draw nothing.

## Launch speed: the committed gate

`launch-speed-walk.py` judges the p95 of every warm launch, from the launch request to the splash
playing, against 380 ms scaled by Dictionary's slowness in the same guest (the CEO: "if the VM is
slower, then the measurements need to be adjusted accordingly").

**The run on `408e628f9`, with the day-change route.** The host was quiet: Dictionary took 321 ms
from the click in the guest against 318 ms on his Mac, so the limit was 383 ms (x 1.01) and every
number below is also within 380 ms unscaled.

| Route | Launches | p95 | median | slowest |
|---|---|---|---|---|
| ordinary | 20 | 217 ms | 191 ms | 232 ms |
| rule switch (#1 to #2 to #1) | 40 | 225 ms | 217 ms | 239 ms |
| **day change** (the first start of a new day, his two openings in turn) | 20 | 223 ms | 214 ms | 233 ms |
| all warm launches together | 80 | 225 ms | | |
| cold (after `sudo purge`), judged on its own against 2,998 ms | 20 | 1,653 ms | 1,482 ms | 2,967 ms |

The day-change route: one home set to "a different one each day"; before each judged launch its
record of the last such day is made a day older, which is what a night does; the launch must show the
other opening than the one before it. A start that drew its splash in the page would refuse the
route on its time (the gate's own test holds that with the 1,726 ms the review measured).

Three earlier runs of the gate on earlier states of this branch, without the day-change route, all
passed on a busy host: p95 374 ms against 709, 640 against 740 and 454 against 1,089.

The same gate on `4145eb904`, which this branch is built on (its own record,
`docs/verification/2026-11-14-splash-opening/`): ordinary 320 / 237 ms, rule switch 359 / 254 ms.

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
- **Two starts of "a different one each day" still draw in the page:** one after he earned or lost an
  opening in a way that changes whose turn it is and one after a quit whose drawing failed. The
  launch line says which.
- A Retina display, Windows and his own Mac.

## Instances

Every instance the walk started was quit from the app's own menu between its starts; `run-walk.py`
quit the last one by its recorded pid (4968) and ended "clean: app quit, VM stopped, clone deleted,
state removed." The launch-speed run quit every instance it started by the pid captured at spawn
(145 instances, none left running) and ended the same way.

The frames and reports passed the privacy gate (`scripts/qa/ocr-gate.sh`, 0 of 15 frames, 0 of 2
text files).
