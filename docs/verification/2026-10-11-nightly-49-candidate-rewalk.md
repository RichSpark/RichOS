# Rewalk of D28 and D29 on nightly candidate 50 (`v1.2.0-nightly.20261011.50`), 2026-10-11

**Verdict: READY.** D28 and D29, the two defects that kept candidate 49 from READY, are fixed on
the running candidate:
- **D28.** While a report is open in the Rich panel, nothing the conversation behind it finishes
  takes the keyboard to the conversation's message box. The first-run "Not now" finished with the
  panel open in six runs, and the box never took focus. Every typed word stayed in the panel's box
  through the card's arrival. After the panel closed, a click in the conversation's box took
  typing again.
- **D29.** After Change it, an edit and Done, the button row draws Send report, Change it and
  Cancel in full, in light and in dark, and after the theme changes under it.

| Item | Result |
|---|---|
| D28: with a report open, Rich's answers and the first-run "Not now" finishing never move focus to the conversation's box | **PASS**. 0 typed words reached the conversation's box in 8 runs |
| D28: every typed word stays in the panel's box, up to and after the card arrives | **PASS** while the caret is in the panel (runs K, L, P, Q, R, S). See "Not seen" for the one order not reproduced |
| D28: after the panel closes, the conversation's box takes typing again | **PASS**: one click in the box, then typing lands there (runs P, Q, R, S) |
| D29: after Done, all three buttons are drawn in full, in light | **PASS** (Done pressed in light; also after a switch from dark) |
| D29: the same in dark | **PASS** (Done pressed in dark; also after a switch from light) |
| D29: the three buttons' text meets WCAG AA in both themes | **PASS**: 4.72:1 to 7.68:1, computed |

Candidate 49 was NOT READY on D28 and D29 only
(`docs/verification/2026-10-10-nightly-49-candidate-walk.md`, which ends "Verdict: NOT READY"
and lists D23 to D26 fixed, D27 not blocking). Nothing else from that walk was walked again.

## What changed since candidate 49, re-derived

- **Source.** 49 was built from `13aa9bc0f`, 50 from `1388a3fdd`.
  `git log --first-parent 13aa9bc0f..1388a3fdd` lists two merges:
  - `b0446e9c6`: the 49 walk record and the steps-walk push fix. Nothing a user sees.
  - `1388a3fdd` (echo-opus-walk49fix1): `fe43132e3` (D28) and `1ce71a2c2` (D29).
- **The only UI files changed** (`git diff --stat 13aa9bc0f 1388a3fdd -- richos/app/ui
  richos/app/src-tauri/src`): `main.js` +15, `bug-report.js` +15 -2, and `mock.js` and
  `tests/bug-report.js`, which do not ship as behavior.
  - `main.js`: `inputEl.focus` does nothing while `body.bug-dock-open` is set.
  - `bug-report.js` `paintCard`: the button row is a new box at every paint, not the old one
    emptied.
- **By the code, not seen:** `hideDock` focuses nothing when the panel closes. So after ✕ the
  page has focus, and the user clicks the box to type. That is how 49 behaved too.

## Identity, checked before any claim: PASS

- **The zip.** `shasum -a 256` of `RichOS-1.2.0-nightly.20261011.50-macos-aarch64.zip` gives
  `f567faa8dcfda1ff45650a99822a4f4b9ce0adfd45479103382be8e8f4014b27`. Both of its `SHA256SUMS`
  lines carry it: the versioned name and `RichOS-macos-aarch64.zip`.
- **The engine.** `richos-engine-1.2.0.tar.gz` gives
  `17d61f5b57333583fb8c937bbecb50b8dd12d476e27883158e08f3d02c858546`, its `SHA256SUMS` line.
- **`candidate.json`:**
  - `source_commit` `1388a3fdd41f9e79c090e4af2d88c31a72b4bb9d`;
  - `run_id` `20261011T010111Z-abdbc9e8`;
  - `version` `1.2.0-nightly.20261011.50`;
  - `build_commit` `407a905b973fb035d9502898054d210c55078b2e`.
- **The build commit.** Its parent is `1388a3fdd`. `git diff --stat 1388a3fdd 407a905b9` touches
  only `nightly-build.json`, `Cargo.lock`, `Cargo.toml` and `tauri.conf.json`.
- **The Settings version line** reads "RichOS 1.2.0-nightly.20261011.50 is up to date." (run V,
  step 11). Every card in the walk said "RichOS 1.2.0, nightly 50".

## Nothing could reach GitHub

- No Send was pressed in any run. Every report was canceled.
- The guest had no reporting token: the keychain lookup exited 44, "no such item", in runs K, L,
  M, N, P, Q, R and S.
- Before the report opened, every one of those runs pointed `api.github.com` at the guest itself
  in `/etc/hosts`, and a `curl` to it had to fail or the run stopped. It failed every time:
  "Failed to connect to api.github.com port 443".
- The same check at the end of runs K, L, P, Q, R and S: still unreachable. M and N ended early
  (below), before it.

## D28: PASS. The conversation behind the panel no longer takes the keyboard

### How it was walked

- **The words.** Short made-up words ("markerone" to "markersix") were typed with the keyboard at
  each moment. After each, the accessibility tree was searched for the word. A hit names the text
  box it is in: the panel's box is at x 1152, the conversation's at x 735. `ax.sh --focused` names
  the element that has focus.
- **What the conversation finishes while the panel is open:**
  - Rich's answer to a message sent just before Bust a bug;
  - Rich's "reached out" account notice;
  - the first-run "Not now" (its receipt "Left with you. Ask me any time and we'll go through
    it.").
- **The 49 walk's order** (its run I step list, unchanged) was runs K and L: "Not now" pressed,
  a message sent, then Bust a bug.

### What happened

| Run | Theme | First-run "Not now" | Words in the panel's box (x 1152) | Words in the conversation's box (x 735) while the panel was open |
|---|---|---|---|---|
| K | light | finished before the panel opened (receipt found at the first look) | markerone, markertwo (15 s after the receipt and Rich's answer), markerthree (at the card), markerfour (30 s later) | none |
| L | dark | the same | markerone, markertwo, markerthree (at the card) | none |
| M | light | pressed by accessibility press after the panel opened. The press itself moved focus to the page | none: the page had focus. The run then ended (below) | none: focus stayed on the page after the receipt |
| N | dark | the same as M | the same as M | none, the same |
| P | light | accessibility press after the panel opened, then a mouse click back in the panel's box | markerone to markerfour (after the receipt and the account notice), markerfive (at the card) | none |
| Q | dark | the same as P | the same as P | none |
| R | light | clicked with the mouse while the caret was in the panel. The click moved focus to the page's group around it | markerone; then markerfour and markerfive, after a click back in the box | none: focus stayed on that group after the receipt |
| S | dark | the same as R | the same as R | none, the same |

**After each card arrived,** `ax.sh --focused` read the panel's "Message to Rich" box with an
empty value (K, L, P, Q, R, S). Then the word typed next was in it. On 49 the caret and gold
border were on the conversation's box at this moment in runs F, G and J.

**The fix, seen working.** In M, N, R and S the "Not now" finished with the panel open while the
caret was on the page, not in any box.
- On 49 the code at that moment focused the conversation's box unconditionally.
  `declineFirstRunInterview` calls `inputEl.focus()` after the bridge answers, and that is how
  49's run I lost " markertwo".
- Here focus stayed where it was, 4 of 4 times. The words typed next went into no box, and none
  into the conversation's.

Frames:
- `k-card-light`: the card arrived, the caret is in the panel's box.
- `p-panel-after-not-now-light`: the receipt is in, "markerone" is in the panel.
- `p-typed-at-card-light` and `q-typed-at-card-dark`: "markerfive" in the panel at the card.
- `m-after-not-now-light`, `r-after-mouse-not-now-light` and `s-after-mouse-not-now-dark`: after
  the receipt, no caret in the conversation's box and nothing typed in it.

**Not seen: the caret in the panel's box at the instant the first-run "Not now" finished.**
- Every way the walk can press "Not now" moves focus first: the accessibility press (M, N, P, Q)
  and the mouse (R, S).
- 49's run I order did not recur. In K and L the receipt was already there when the panel opened.
- Its cause is ruled out above: the call that moved focus no longer moves it, wherever the caret
  is.

**Why M and N ended early.** With focus on the page, the report sentence typed after it went
nowhere. So no card came, and the step that waits for "Not sent yet" stopped each run at 180 s.
That is the walk's own design: it had no click back into the panel. P and Q are the same walk
with that click.

### After the panel closes: the conversation's box takes typing again

- **Cancel, then ✕.** A search for "marker" found nothing anywhere (K, L, P, Q, R, S). No typed
  word was left in the conversation's box, and none was sent to Rich.
- **Focus was on the page** (`AXWebArea`), as the code says (`hideDock` focuses nothing).
- **One mouse click in the conversation's box:** `--focused` read the box ("Message to Rich", x
  735). "markersix" typed next landed in it (P, Q, R, S).
- Frames `p-conversation-after-close-light` and `q-conversation-after-close-dark`.
- In K and L the click was `ax.sh click --at`. It is an accessibility click at a point, and it
  does not focus a text box, so the words went nowhere. The real mouse is now a steps-walk step
  (see Method).

## D29: PASS. After Done, all three buttons are drawn

Run L, with the 49 run E report ("When I press the Send button twice quickly…"), long enough that
the button row floats over the card.

| Moment | Send report | Change it | Cancel |
|---|---|---|---|
| Done pressed in dark (`l-done-dark`) | drawn x 1108 to 1230 | outline x 1239 to 1343 | outline x 1352 to 1437 |
| Then switched to light (`l-done-then-light`), and 30 s later | the same | the same | the same |
| Change it again, edit, Done pressed in light (`l-done-light`) | the same | the same | the same |
| Then switched to dark (`l-done-then-dark`) | the same | the same | the same |

- **Measured:** `frame.py edges --row 780 1100 1450` on each frame. Each button's color run starts
  and ends where its accessibility box does: Send report 1108 + 123, Change it 1238 + 107, Cancel
  1352 + 87 (`l-05-done-dark.tree` and `l-09-done-light.tree`, lines 131 to 133). The 1 px
  difference at the outlines' ends is antialiasing.
- **On 49:** a gold "Send rep" cut at about 74 px, and neither of the other two (2 of 2).
- **Crops:** `l-done-dark-row-crop`, `l-done-light-row-crop`.
- **Pressing them:** Change it reopened the editor after the first Done, and Cancel ended the
  report, both by accessibility press. Send report was not pressed (no Send in this walk).

## Contrast (`qa/contrast.py`), the elements walked

| What | Light | Dark |
|---|---|---|
| "Send report" on its gold button | 4.72:1 | 7.68:1 |
| "Change it" | 6.33:1 | 5.78:1 |
| "Cancel" | 6.33:1 | 5.78:1 |

- Measured on `l-done-light` and the uncommitted light frame 30 s after the first Done, and on
  `l-done-dark` and `l-done-then-dark`. The two frames in each theme give the same figures.
- **The outline of Change it and Cancel: 2.95:1 in light, 2.97:1 in dark.** That is below 3:1 for
  a non-text indicator.
  - **Declared not an indicator here.** The button's own label names it and clears 4.5:1 in both
    themes. WCAG 1.4.11 does not require a boundary for a control identified by its text.
  - It is unchanged by 50 and is the app's outlined-button style everywhere ("Not now" in the
    conversation is the same).
  - Noted so the claim is visible, not judged as new.
- **Nothing new is drawn for D28.** The fix changes where focus goes, not what is shown.

## Seen, not new or not blocking

- **A strip of the card shows under the floating button row,** between it and the panel's box
  ("text "Talk to Rich…", and only one sent…" in `l-done-dark`). 49 showed the same
  ("Steps to see it" under the row). It is the scroll area's last lines, readable and not cut by
  the buttons. Not new.
- **After ✕ the page has focus, not the conversation's box.** A keyboard user who closes the
  panel and types at once types into nothing until they click the box. It is unchanged from 49 by
  the code, and no word reaches a wrong place. Noted as a possible polish item, not a defect.
- **The guest's own spelling correction** turned "markerone" into "Marker one" once another word
  followed it, and offered "Markerfive". It is macOS text correction in the guest. It changes no
  verdict above, because each search matched the word's own part.

## Verification mode

- **Environment:** the test VM, one fresh guest per run through `run-walk.py`, never this Mac's
  screen.
  - Two runs at a time, one per guest slot.
  - One app window per guest; no two-user test.
  - Guest screen 1680x1050 at 1x; app window 1400x864.
  - Light and dark through the guest's macOS appearance.
- **Artifact:** the candidate's own signed zip, with the same folder's
  `richos-engine-1.2.0.tar.gz` as `--engine`.
- **Account:** the real Claude login that `run.sh` copies into each guest. No account file on
  this Mac was written.
- **GitHub:** no reporting token, `api.github.com` unreachable, and no Send pressed.
- **Audio:** none played or recorded.
- **Verification basis:**
  - Every PASS above was seen on the running candidate in the guest: its accessibility tree, its
    focus, its frames and the walks' step records.
  - Lines marked "by the code" are read from source and were not seen.

### Runs

| Run | VM | Walk | Result | Seconds |
|---|---|---|---|---|
| K | `walk-be3317cace66` | `walkK.json` | D28 in 49's run I order, light; card; close. Exit 0 (the `--at` click did not focus) | 375 |
| L | `walk-616e122c4d2d` | `walkL.json` | D28 in 49's order, dark; D29 Done in dark and in light, theme switches. Exit 0 | 449 |
| M | `walk-ac3027455c0b` | `walkM.json` | "Not now" by accessibility press with the panel open, light. Ended at step 47: no card (above) | 445 |
| N | `walk-c2bb29830190` | `walkN.json` | the same, dark. Ended at step 48, the same way | 427 |
| P | `walk-41ccfd31e849` | `walkP.json` | press, then a mouse click back in the panel, light; card; close; mouse click in the conversation's box. Exit 0 | 372 |
| Q | `walk-0c456629b2b7` | `walkQ.json` | the same, dark. Exit 0 | 355 |
| R | `walk-05193c92e364` | `walkR.json` | "Not now" clicked with the mouse, light; the rest as P. Exit 0 | 327 |
| S | `walk-ebec26a3f84f` | `walkS.json` | the same, dark. Exit 0 | 321 |
| V | `walk-c419f38d5ac0` | `walkV.json` (first form) | harness, mine: Settings pressed under the first-run question. Nothing of the app tested | 36 |
| V2 | `walk-ad2b1b54825c` | `walkV.json` | the Settings version line. Exit 0 | 88 |

## Method and toolkit

The `qa/` tools are unchanged from `1388a3fdd`. `testvm/steps-walk.py` is this branch's copy with
one new step (below).

**`testvm/`:**
- `run-walk.py --wait 900` with `TESTVM_AX_TIMEOUT=60`;
- `steps-walk.py --steps … --out …`, and `--check` on every list;
- through it, `ax.sh` (find, click, type, `--focused`, `--key`, `--at`), `guest.sh`, `shot.sh`
  and `pointer-drag.sh`;
- `slots.py status`.

**`qa/`:**
- `contrast.py` on frame regions and on two color pairs;
- `frame.py edges` and `frame.py crop`;
- `ocr-gate.sh` with its positive control;
- `redact.py --phones`;
- `wait-for.sh --log` for each run's end.

**Elsewhere:** `nightly-local.py candidate`, `shasum`, `jq`, `git log`, `git diff`, and `sips` for
the JPEG copies.

**Scripts written from scratch: none.**
- **One step added to a committed tool:** `ef4a41c20`.
  - `steps-walk.py` has a `pointer` step: `pointer-drag.sh` with the same point twice, a real
    mouse click.
  - The walk needed it because `ax.sh click --at` does not focus a text box (run K).
  - `test/steps-walk.test.py` proves the step's command and its refusal of five malformed
    `points`: "0 failed". The README row names it.
- **The step lists.** Nine were added under `steps/` and declared in `steps-walk-data.test.sh`'s
  inputs and covers rows, and that suite passes (3 ok lines).
- **Two scratch shell files copied and converted frames by a list** (`cp`, `sips`). They measure
  nothing. Inline Python read the step records. It measures nothing either.

### Frames

- **Format:** JPEG q75 copies of the 1680x1050 PNG originals. The `*-crop` frames are crops.
- **Names:** the run letter, then what the frame shows.
- **The privacy gate.** `ocr-gate.sh` ran over the 15 PNGs the JPEGs were made from. Its positive
  control found the known address first.
  - It flagged one frame, `s-after-mouse-not-now-dark`, for a phone-shaped run of digits. The
    frame shows none. The match was in the guest's Dock icons.
  - `redact.py --phones` covered the Dock strip, and its re-scan found nothing.
  - The second gate: "0 of 15 frame(s) carry something that must not ship".
- **The text gate.** `ocr-gate.sh --text` over this record and its nine step lists, after its own
  positive control: "0 of 10 text file(s) carry something that must not ship".
- **The names in the frames are invented for the walk:** Acme.

## Surfaces

**Applied and checked:**
- the Mac desktop app from its signed bundle, in light and dark;
- Bust a bug from the Settings menu on a conversation, the entrance where D28 and D29 were found;
- typing into the panel at the question, while the conversation finishes three kinds of work, at
  the card, and 30 s after it;
- the card's Change it, Done and Cancel, and the panel's ✕;
- the way back: after ✕, the conversation's box by a mouse click;
- the theme changing under an open card.

**Not applicable:**
- phones: nothing touched;
- battery: no `richos/mobile/**` change;
- the stable channel and publishing: nothing was published.

**Not covered:**
- the other four entrances (Corrections, the home screen, the held opening screen). 49 passed
  them, and neither fix is specific to an entrance;
- the panel's talk button, which 49 passed;
- Send, real or waiting (forbidden for a real one; not needed for D28 or D29);
- VoiceOver itself (the accessibility tree was read instead);
- a physical Mac.

**Test data:** each guest was a fresh clone with its own empty home. Nothing touched this Mac's
app data, and no app was started on this Mac.

## Cleanup

- **Every run** ended "clean: app quit, VM stopped, clone deleted, state removed." and released
  its slot. All ten are in the table above.
- **After the last run:**
  - `slots.py status` read "guest.lock: free" and "guest-2.lock: free";
  - `~/.richos-testvm/tart/vms/` holds only `richos-base`;
  - `~/.richos-testvm/run/` is empty.
- **Scratch:** `/Volumes/E1TB/tmp/claude/ray-opus-walk49b/` is deleted after this record is
  committed.

## Defects, in one list

- **Fixed from 49:** D28 and D29.
- **Still open from 49, does not block:** D27 (a single Return joined into the next sentence at
  Send). Not walked again.
- **New:** none.
- **Noted, not judged as defects:**
  - the panel's ✕ leaves focus on the page;
  - the outlined buttons' border is 2.95:1 in light and 2.97:1 in dark (declared above).

Verdict: READY
