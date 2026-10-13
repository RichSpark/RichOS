# Walk of nightly candidate 54 (`v1.2.0-nightly.20261013.54`), 2026-10-13

Candidate 54 was built from richos main `17fe48f48` (run `20261013T051859Z-7a5c0272`). Candidate
53 was walked READY (`docs/verification/2026-10-13-nightly-53-candidate-walk.md`). Nothing from
that walk was walked again. This build already ran front-door `--release` and gui-boot in its own
guest, so neither was run again here. This walk covers only what a user sees that changed since 53.

| Item | Result |
|---|---|
| 1. A queued idea card and a queued bug card at 150% text size, light and dark | **PASS**. In both cards and both themes, the status pill wraps to a second line ("…saved on this" / "Mac"). The break falls between two words, and the pill's outline ends 19 px inside the card's edge. At 100% both cards measure exactly as in nightly 53 |
| 2. The starter settings file's new first lines | **PASS**. The new line is correct: the app writes the file at the path it names. Both ways it names to open the file work, tried live in the guest. Two wording notes, not blocking (below) |
| Readable text of everything walked, WCAG AA, light and dark | **PASS**. 18.07:1 light, 12.06:1 dark, computed (table below) |
| Seen in passing, not from this nightly | **D34** (not blocking). At 150% the Settings menu clips and overlaps its own rows, and the left list and the panel's message box show scrollbars |

## What changed since candidate 53, re-derived

`git log --first-parent 06087e1ae..17fe48f48` lists three merges:

- **`588ea3c54`**, D30 at 150% (item 1). It changes one CSS rule in `ui/style.css`, `.bug-pill`:
  - `white-space: nowrap` becomes `min-width: 0; max-width: 100%; overflow-wrap: anywhere`;
  - it also changes the `bug-report.js` check.
- **`072ab9735`**: gui-boot's watcher rule and the D30 check. No user-facing change.
- **`17fe48f48`**: portable-settings follow-ups. What a user sees here is D31, one new line in
  `richos/engine/.env.example`, the file the app copies as the starter (item 2). The rest is:
  - the nightly running front-door and gui-boot in its guest;
  - D32, wording in the engine's docs;
  - a new legacy settings key;
  - the mobile proof driver.

  None of these puts anything new on screen.

`git diff --stat 06087e1ae 17fe48f48 -- richos/app/ui` touches only `style.css` (6 lines) and
`tests/bug-report.js`.

## Identity, checked before any claim: PASS

- **The zip.** `shasum -a 256` of `RichOS-1.2.0-nightly.20261013.54-macos-aarch64.zip` gives
  `d67d659ff57ef8e5038fdc998d8630f904366bab63695968940785ae38fd47c2`. That matches both of its
  `SHA256SUMS` lines (the versioned name and `RichOS-macos-aarch64.zip`).
- **The engine.** `richos-engine-1.2.0.tar.gz` gives
  `2e7ab21d3882107c7c800f48a44d117326c4a0178938b8b5630a9668824d8116`. That is its `SHA256SUMS`
  line and the digest in `engine-pin.env`.
- **`candidate.json` (`.info`):**
  - `source_commit` `17fe48f48ae059a4ff805e0f28ca09ef67607aa4`;
  - `run_id` `20261013T051859Z-7a5c0272`;
  - `version` `1.2.0-nightly.20261013.54`;
  - `build_commit` `10bbdf9f7f51cce58a681a8eecf6e443778421b7`;
  - `checks_skipped` `[]`.
- **The build commit.** Its parent is `17fe48f48`. `git diff --stat 17fe48f48 10bbdf9f7` touches
  only `nightly-build.json`, `Cargo.lock`, `Cargo.toml` and `tauri.conf.json`.
- **The running app.**
  - In every run, the guest's `app.log` reads `this app: built from
    10bbdf9f7f51cce58a681a8eecf6e443778421b7`.
  - The Settings update line reads "RichOS 1.2.0-nightly.20261013.54 is up to date." (run D2,
    `d-01-settings-version`).
- **The suites the build ran in its guest.** I read their results and did not rerun them, as the
  brief says. Under `~/.richos-nightly/gui-proofs/20261013T051859Z-7a5c0272.proof.suites/`:
  - `front-door.rc` 0, "6 passed, 0 failed";
  - `gui-boot.test.sh.rc` 0, "=== gui-boot.test.sh: all 35 passed ===".

  D33 from nightly 53 (B2) is closed by `072ab9735`.

## 1. The queued card at 150% text size: PASS

**Setup, the same for both cards.** This is as in nightly 53:
- There is no reporting token in the guest (keychain lookup exit 44).
- `api.github.com` and `github.com` point at the guest itself. `curl` was refused before the walk,
  right before the press, and at the end.
- Pressing **Post idea** or **Send report** left the card waiting. The outbox file says
  `"reason":"not-set-up"`.
- **Nothing was posted or sent anywhere.**

**How a user gets to 150%.**
- Idea card (run A2): **Settings › Text size › +** four times. The row then reads "150%", with
  the + dimmed at the top step.
- Bug card (run C): **⌘=** four times. That is the app's own shortcut, sent only after checking
  that RichOS was the frontmost app.
- Each card was queued at 100% and then made bigger, so the walk also saw it reflow.

| Card | Size | Theme | Title | Pill text (accessibility tree) | Pill outline (frame) | Card edge | Gap |
|---|---|---|---|---|---|---|---|
| Feature idea, "Waiting to post · saved on this Mac" | 100% | light | one line, 96 x 20 | 262 x 20, one line | x 1182..1468, 28 px tall | x 1487 | 19 px |
| | 100% | dark | one line | same | x 1182..1468, 28 px tall | x 1487 | 19 px |
| | **150%** | light | one line, 139 x 29 | **324 x 62, two lines** | **x 1108..1468, 72 px tall** | x 1487 | **19 px** |
| | **150%** | dark | one line | 324 x 62 | x 1108..1468, 72 px tall | x 1487 | 19 px |
| Bug report, "Waiting to send · saved on this Mac" | 100% | light | one line, 82 x 20 | 266 x 20, one line | x 1178..1468, 28 px tall | x 1487 | 19 px |
| | 100% | dark | one line | same | x 1178..1468, 28 px tall | x 1487 | 19 px |
| | **150%** | light | one line, 119 x 29 | **330 x 62, two lines** | **x 1108..1468, 72 px tall** | x 1487 | **19 px** |
| | **150%** | dark | one line | 330 x 62 | x 1108..1468, 72 px tall | x 1487 | 19 px |

**What the measurements show:**
- **The pill wraps inside the card.** Outlines come from `frame.py extent` over the card's head;
  the card edge from `frame.py edges --row`.
- **No word is broken.** Both 150% pills read "Waiting to post · saved on this" / "Mac" (and
  "…send…"). The line breaks at the space before "Mac", seen on the frames.
  - `overflow-wrap: anywhere` would split a word only if one word could not fit on a line, and
    none of these words comes close.
- **At the default size it looks as it did in nightly 53.** Measured the way 53 measured it, with
  `frame.py edges --row` through the pill's text:
  - the last ink is at x 1449 and the card's edge at x 1487/1488: the "38 px inside" nightly 53
    recorded, for both cards and both themes;
  - the title sizes (96 x 20 and 82 x 20) and pill text sizes (262 x 20 and 266 x 20) match 53's
    numbers exactly.

**Frames:**
- `a-01-idea-100-light`, `a-02-idea-100-dark`;
- `a-04-idea-150-light`, `a-05-idea-150-dark`;
- `b-01-bug-100-light`, `b-02-bug-100-dark` (run B1);
- `c-03-bug-150-light`, `c-04-bug-150-dark` (run C).

**One thing a 150% user will notice, not a defect of this change.**
- At 150% the panel's action bar ("Try now / Change it / Cancel report") takes two rows and stays
  fixed at the bottom of the panel.
- Right after the reflow in run B2, the pill's second line sat under that bar until the panel was
  scrolled a little.
- The card's content scrolls under a fixed bar at 100% too (nightly 53 frames). The pill is fully
  visible once scrolled (run C).

## 2. The starter settings file's first lines: PASS

**What a fresh account gets** (runs A1, A2, B1, B2, each with an empty home):
- `~/Library/Application Support/RichOS/.env`, mode `-rw-------`, 1260 bytes, comments only
  (0 non-comment lines).
- **sha256** `e65fba1032fdd9bbd3772d851593ae7420d284421e8576d8c33d30df577ac3c0` in every run. That
  is byte-identical to `richos/engine/.env.example` at `17fe48f48`, which the app compiles in
  (`machine_settings.rs:23`, `include_str!`).
- The same hash and mode were read again at the end of run A2, after the cards, the theme changes
  and the text-size change.
- Its first two lines:

  ```
  # Optional RichOS storage settings. Everything can remain commented out.
  # Open ~/Library/Application Support/RichOS/.env with your editor or Finder Go to Folder.
  ```

**Is the line correct? Yes, tried both ways it names, live in the guest** (run B2,
`walk-40d49bdb60d4`).
- The app's HOME in a walk is a fixture folder, so the file the app wrote was copied, byte for
  byte, to the guest user's own `~/Library/Application Support/RichOS/.env`. That is where a real
  user's app writes it. The guest had no file there before.
- **Finder › Go to Folder.**
  - Finder opened on the home folder, and ⇧⌘G brought up the box. Typing the line's path exactly,
    `~/Library/Application Support/RichOS/.env`, showed the match "Users › admin › Library ›
    Application Support › RichOS › .env" (`b-10-goto-typed`).
  - Return opened the **RichOS** folder with **`.env` shown and selected** (`b-11-goto-result`).
    The file shows there although Finder's hidden-file setting is off (`AppleShowAllFiles` not
    set).
  - ⌘O opened it in **TextEdit**, with no "no application" dialog (`b-12-open-file`). The window is
    named `.env` and shows the file's text.
- **"Your editor"** (TextEdit's own Open panel).
  - ⌘O, then ⇧⌘G, then the same path, then Return. The panel landed in the RichOS folder with
    `.env` selected and previewed as "Document - 1 KB" (`b-14-editor-goto-result`).
  - Return opened it. TextEdit's window list then read `.env`.

**Is it clear? Clear enough for a Mac user. Two wording notes, neither blocking:**
- **"Finder Go to Folder"** names the command but not where it is. A person who has never used it
  will not know it is in Finder's **Go** menu, or that it is **⇧⌘G**. Suggested:
  "In Finder, choose Go > Go to Folder (Shift-Command-G) and paste this path:
  ~/Library/Application Support/RichOS/.env".
- **The line lives inside the file it explains.** Only someone who already has the file open
  reads it. The app still names the file nowhere.
  - That is the other half of nightly 53's D31: no screen, menu or Preferences row mentions it,
    and Rich does not know where it is.
  - It was not walked again, per the brief. Nothing in `06087e1ae..17fe48f48` changes it
    (source: the diff above). The PRD's requirement was the header line, and the line is now
    there.

## Contrast (`qa/contrast.py`), the elements this nightly changed

| What | Size | Light | Dark |
|---|---|---|---|
| Idea card: "Feature idea" | 100% / 150% | 18.07:1 / 18.07:1 | 12.06:1 / 12.06:1 |
| Idea card pill, line 1 and line 2 at 150% | 100% / 150% | 18.07:1 / 18.07:1, 18.07:1 | 12.06:1 / 12.06:1, 12.06:1 |
| Bug card: "Bug report" | 100% / 150% | 18.07:1 / 18.07:1 | 12.06:1 / 12.06:1 |
| Bug card pill, line 1 and line 2 at 150% | 100% / 150% | 18.07:1 / 18.07:1, 18.07:1 | 12.06:1 / 12.06:1, 12.06:1 |

- **Not measured, declared:**
  - **The pill's dashed outline.** It is not the indicator; the words inside it state the state.
    The same declaration as in nightly 53.
  - **The starter file's text.** It is plain text shown by the user's own editor in that
    editor's colors, not RichOS's screen.
  - **D34's menu.** It is unchanged by this nightly and was not measured.

## Findings

### D34. At 150% text size the Settings menu clips and overlaps its rows (not blocking, not from this nightly)

**What a user sees at 150%** (`b-06-settings-menu-150`, run B2):
- In the **Settings menu:**
  - the "Home screen" choice reads **"Company butto"**, cut off at the menu's right edge;
  - the second line under **"Connected folders"** is drawn over by the **"Bust a bug!"** row, so
    neither reads cleanly;
  - the last row, **"Use Rich from your phone"**, is cut off at the bottom, and a horizontal
    scrollbar shows under it.
- **The left list** shows a horizontal scrollbar above "Set your name", which means it is a few
  pixels too wide at 150%.
- **The panel's one-line message box** ("Tell Rich what to change, …") shows a vertical scrollbar
  beside its cut-off placeholder.

**Why not blocking:**
- Nothing in this nightly touched these: the only UI change is the `.bug-pill` rule (diff above).
  So candidate 53 almost certainly showed the same thing.
- That is by source, not walked on 53. No earlier walk record mentions it.
- Every control still works and the menu scrolls.

**Who:** a user who sets 150% text size, the setting meant for people who need bigger text. The
menu they just used to set it is the first thing that breaks.

**Suggested fix:** give the menu's rows the same treatment D30 gave the pill:
- let the select and each row's label wrap or shrink within the menu's width;
- keep each row's secondary line in the row's own flow, so the next row cannot paint over it.

### Notes, no finding

- **D33 is closed.** gui-boot passed 35 of 35 in the build's own guest (above).
- **D31's first half is fixed** (item 2). Its second half, that the app never names the file,
  stands as recorded in nightly 53.
- **Raised as `esc-20261013T073742Z-e6e0526e`, not about the candidate.** During this walk
  (08:35 local) Homebrew moved this Mac's `python3` to 3.15.0. Every lint baseline records
  Python 3.14, so the pre-commit lint refuses every commit under `richos/app` with "lint versions
  do not match the committed baseline".
  - Python 3.14.8 is still installed as `python3.14`.
  - This record's one `richos/app` change (the step-list rows) passes that same lint under
    3.14: `python3.14 richos/app/scripts/lint/driver.py --changed --strict` prints "Lint passed".
  - The commit's own hook ran with a PATH whose `python3` is `python3.14`. Nothing was skipped,
    and `--no-verify` was not used.

## Verification mode

- **Environment:** the test VM, one fresh guest per run through `run-walk.py`, never this Mac's
  screen.
  - One run at a time; one app window per guest. No two-user test.
  - Guest screen 1680x1050 at 1x; app window 1400x864.
  - Light and dark through the guest's macOS appearance. Text size through the app's own control
    (A2) and its own shortcut (C).
- **Artifacts:** the candidate's zip and the same folder's engine tarball, as `nightly-local.py
  candidate --run 20261013T051859Z-7a5c0272` prints them.
- **Account:** the real Claude login that `run.sh` copies into each guest. No account file on this
  Mac was written.
- **GitHub:** no reporting token, and `api.github.com` and `github.com` unreachable in every run
  that queued a card. No post or issue was made.
- **Audio:** none.
- **Verification basis:**
  - Every PASS above was seen on the running candidate in a guest: its accessibility tree, its
    frames, the guest's files and `app.log`, and the walks' step records.
  - Lines naming commits, diffs or source lines are read from source.

### Runs

| Run | VM | Walk | Result | Slot held |
|---|---|---|---|---|
| A1 | `walk-99be40a177de` | `steps/walkA.json` (first form) | starter hash read. The walk stopped when a full accessibility-tree read hit its 20 s limit (exit 124), a harness limit. In the second form the tree reads became targeted `find`s | 190 s |
| A2 | `walk-e1db4ddc1d7f` | `steps/walkA.json` (second form) | idea card at 100% and 150%, light and dark. Its Finder step, which sent Apple events to Finder, waited 120 s and was ended (exit 124). Run B2 used `open -a Finder` instead | 442 s |
| B1 | `walk-3faaddcc2f40` | `steps/walkB.json` (first form) | bug card at 100%, light and dark. Then the click closing the Settings menu hit the accessibility deadline (exit 124). The second form uses ⌘= and no menu | 360 s |
| B2 | `walk-40d49bdb60d4` | `steps/walkB.json` | bug card at 150%, the Settings menu at 150%, the starter line through Finder and TextEdit. Exit 0 | 327 s |
| C | `walk-4a854a97415b` | `steps/walkC.json` | bug card at 150%, scrolled a few steps so the whole pill shows, light then dark. Exit 0 | 297 s |
| D1 | `walk-a1086bbeceee` | `steps/walkD.json` (first form) | my error: opened Settings while a first-run question was up. The tool refused the click behind the question | 44 s |
| D2 | `walk-7142d2b76408` | `steps/walkD.json` | the Settings version line. Exit 0 | 103 s |

**Cleanup.**
- Every run ended with run-walk.py's "clean: app quit, VM stopped, clone deleted, state removed."
  and released its slot.
- Afterwards, `slots.py status` read `guest.lock: free` and `guest-2.lock: free`. The test VM's own
  `tart list` (`~/.richos-testvm/bin/tart.app/Contents/MacOS/tart`, with its `TART_HOME`) lists no
  `walk-` guest, only `richos-base` and its two source images, all stopped.
- No app instance was started on this Mac. `pgrep -fl richos-tauri` matched only my own shell,
  whose arguments contain that string.

## Method and toolkit

The `qa/` and `testvm/` tools are those of `17fe48f48` (this branch's base, which contains the
toolkit commit `b43e608d3`). None was changed.

**`testvm/`:**
- `run-walk.py --wait 600`;
- `steps-walk.py --steps … --out …`, and `--check` on every list;
- through it, `ax.sh` (`find`, `click`, `type`, `--key`), `guest.sh`, `shot.sh` and
  `pointer-drag.sh`;
- `slots.py status`.

**`qa/`:**
- `contrast.py` on frame regions;
- `frame.py extent`, `edges --row` and `crop`;
- `ocr-gate.sh`, with its positive control, on the 14 committed frames: "0 of 13" and "0 of 1"
  carry something that must not ship;
- `wait-for.sh --log` for each run's end.

**Elsewhere:**
- `nightly-local.py candidate`, `shasum`, `jq`;
- `git log`, `git diff`, `git show`;
- `sips` for the JPEG copies.

**Scripts written from scratch: none.**
- The step lists are data for `steps-walk.py`, kept in `steps/`. They are added to the `inputs` and
  `covers` rows of `richos/app/scripts/steps-walk-data.test.sh`, which passes: "ok covers row names
  every committed step list", "ok every committed step list is valid", "ok malformed step lists
  fail the check".
- `steps.json` outputs were read with inline `python3 -c` one-liners.
- One inline edit of `walkA.json`, which would have marked its tree reads as allowed to fail, was
  refused by the harness before it ran. The list was rewritten with the editor instead.

Verdict: READY
