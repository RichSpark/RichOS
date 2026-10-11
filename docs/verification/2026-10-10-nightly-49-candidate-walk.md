# Walk of nightly candidate 49 (`v1.2.0-nightly.20261010.49`), 2026-10-10

**Verdict: NOT READY.** D23, D24, D25 and D26 from candidate 48 are fixed, and Bust a bug now
opens in the Rich panel on top from all five places. Two new defects in Bust a bug block it:
- **D28.** While the panel is open, the conversation behind it can take the keyboard back. The
  next words go into the conversation's message box, not the report.
- **D29.** After Done, the card's buttons are drawn cut off. "Send rep" shows, and Change it and
  Cancel are not drawn at all.

| Item | Result |
|---|---|
| 1. D23: the privacy line and the count match the stand-ins, with and without private details | **PASS** (3 reports with private details, 4 without) |
| 2. D24: with Corrections open, Settings opens its menu and Bust a bug starts a report | **PASS** (pointer and accessibility press) |
| 3. D25: no horizontal scroll bar under the Rich panel's messages | **PASS** in light, dark, home and opening screen |
| 4. D26: the held-queue status line | **PASS**: "2 assignments waiting to start" |
| 5. Bust a bug opens in the Rich panel, on top, from all five places | **PASS** |
| 5. It can be answered at once by typing, and by the panel's talk button | **PASS** at the first question. **FAIL, D28** once the conversation behind it updates |
| 5. While a report is open, Bust a bug is shown in the menu and cannot be pressed; after Cancel or closing the panel it can be pressed again | **PASS** (see the note on Send) |
| 5. Change the card once, with a line that starts with "-"; the report waits unsent and shows the line as written | **PASS** when the line is its own paragraph. A single Return joins it to the next sentence (D27, does not block). **FAIL, D29**: the buttons after Done |

Candidate 48 was NOT READY (`docs/verification/2026-10-10-nightly-48-candidate-walk.md`). It
failed on D23 and D24, and noted D25 and D26. Everything else in it passed and was not walked
again.

## What changed since candidate 48, re-derived

- **Source.** 48 was built from `d0d06501c`, 49 from `13aa9bc0f`.
  `git log --first-parent d0d06501c..13aa9bc0f` lists four merges.
- **The one a user can see:** `13aa9bc0f` (echo-opus-walk48fix26). It brings D23 to D26 and
  Bust a bug in one place, through 28 commits, `51a84e964` to `987ab6e22`. Among them:
  - the Rich panel as the top layer (`8e454cff1`, `279943ec6`);
  - Bust a bug disabled while a report is open (`72f5fddea`);
  - stand-ins counted from the card's final words (`0a235f122`);
  - a line that starts with "-" sent as written (`53407e2ae`, `987ab6e22`).
- **The other three:** the 48 walk record, Codex's build-reliability repairs and the UI suite's
  solo retry. Nothing a user sees. Not walked.
- **The brief's account of 48 holds.** The 48 record ends "Verdict: NOT READY". It blocks on
  D23 and D24 and lists D25 (polish) and D26 (small).

## Identity, checked before any claim: PASS

- **The zip.** `shasum -a 256` of `RichOS-1.2.0-nightly.20261010.49-macos-aarch64.zip` gives
  `8eeecd606ac29dd736bfb1c5f0e6b80a8110c43145a368856eabd29c6626c11f`. Both of its `SHA256SUMS`
  lines carry it: the versioned name and `RichOS-macos-aarch64.zip`.
- **The engine.** `richos-engine-1.2.0.tar.gz` gives
  `17d61f5b57333583fb8c937bbecb50b8dd12d476e27883158e08f3d02c858546`, its `SHA256SUMS` line.
- **`candidate.json` `info`:**
  - `source_commit` `13aa9bc0f3ac11b5f92f21d35b2a2c6b5fa09130`;
  - `run_id` `20261010T211905Z-62031055`;
  - `version` `1.2.0-nightly.20261010.49`;
  - `build_commit` `be8e145e89d6083822d49c33936c395b06d31bef`.
- **The build commit.** Its parent is `13aa9bc0f`. `git diff --stat 13aa9bc0f be8e145e8` touches
  only `nightly-build.json`, `Cargo.lock`, `Cargo.toml` and `tauri.conf.json`.
- **The Settings version line** reads "RichOS 1.2.0-nightly.20261010.49 is up to date." (run A
  step 13).

## Nothing could reach GitHub

- The guest has no reporting token: the keychain lookup in the app's HOME exited 44, "no such
  item" (runs A and E).
- Before any Send, every run pointed `api.github.com` at the guest itself in `/etc/hosts`.
- Then a `curl` to it had to fail, or the run stopped. It failed every time: "Failed to connect
  to api.github.com port 443".
- The same check ran again at the end of runs A, B3, E, F, G and J: still unreachable.
- No `sent.jsonl` existed in any store that was read (runs A and E). No issue was created anywhere.

## 1. D23: PASS. The line under the card counts the stand-ins that are on it

**With private details.** Run A typed: "In the Acme conversation, the names in the sidebar get
cut off when I make the text bigger. Dana Whitfield at Northwind Traders sees the same thing, and
my notes in ~/Library/Application Support/Client Plans/budget.xlsx now look empty."
- The card carried four stand-ins: [a conversation], [a person], [a company] and [a file on this
  Mac].
- Each is its own node in the accessibility tree, so it is marked, not plain words
  (`a-03-card-private.tree`, lines 165 to 168). On 48 the whole paragraph was one node.
- The line under the card: "Left out, because anyone can read GitHub issues: one conversation
  name, one person's name, one company name and one file path. The underlined words stand in for
  them; point at one to see what it replaced."
- A find for "nothing was left out" matched nothing.
- Frames `a-card-private-light`, `a-card-private-dark`.

**Reproduced twice more:**
- **Run G**, the same words: the same four stand-ins and the same line.
- **Run F** typed the 48 run R sentence (Dana Whitfield, Northwind Traders, budget.xlsx). Rich
  wrote "a colleague" without a stand-in and kept one, [a file on this Mac]. The line read "…:
  one file path. The underlined words stand in for it…" One stand-in, counted once (frame
  `f-card-private-dark`).

**Without private details.** The line read "Nothing private was in it, so nothing was left out."
on four reports: run A's second, B3 Corrections, B3 opening screen and B3 home. No card carried
a stand-in (`grep` for "[a " in each tree: 0).

**No private word reached a saved report.** Run A's waiting report and run E's carry none of
Acme, Dana, Whitfield, Northwind, "Client Plans" or budget.xlsx.

**Not seen: a stand-in's underline and its "what it replaced" tip.**
- On every card they were scrolled above the panel's visible area.
- Two keyboard runs (G, J) meant to Shift+Tab to one. Both lost the keyboard to the conversation
  first (D28).
- The marking is proven by the accessibility tree only.

## 2. D24: PASS. With Corrections open, Settings opens and Bust a bug starts a report

Run B3:
- **Focus was in Corrections.** It sat on "Close corrections": the state that failed 3 of 3 on 48.
- **The Settings button is in the accessibility tree** with Corrections open (`AXPopUpButton
  "Settings"`, enabled). On 48 it was missing.
- **The pointer opened the menu over Corrections:** a click at the button, 1505,131 (frame
  `b-settings-over-corrections`). "Bust a bug!" was enabled.
- **Its press started the report.** The Rich panel opened at the right: "Bust a bug ·
  Corrections".
  - Rich: "…I've noted that you were in Corrections, and it stays open beside us."
  - Corrections stayed open behind it ("Close corrections" still found). Frame
    `b-talk-corrections-light`.
- **A second time, by the accessibility press:** Cancel, then the panel's ✕. Settings opened
  again over the still-open Corrections, and Bust a bug started a new report. Typed words landed
  in the panel's box.

## 3. D25: PASS. No horizontal scroll bar under the panel's messages

The guest has no trackpad, so macOS draws every scroll bar all the time. The panel's vertical bar
is drawn in every frame; a horizontal one is in none:
- the conversation, light and dark (crops `d25-panel-bottom-light`, `d25-panel-bottom-dark`);
- the home screen (`d25-panel-bottom-home`) and the opening screen, where 48 saw it.

## 4. D26: PASS. The held queue says the work is waiting to start

Run H: the committed `testvm/held-queue-walk.sh`, unchanged. Every check passed:
- two jobs written down;
- exactly one back-end turn refused;
- the held notice shown once;
- both started by themselves after the sign-in came back, the first 50 s later, in order;
- no work record says failed.

**Under the notice** (frame `h-held`) the status line now reads "··· 2 assignments waiting to
start". On 48 it read "2 assignments starting".

## 5. Bust a bug as it now works

### Where it opens: all five places PASS

| Pressed from | On top? | Answerable at once? | Run |
|---|---|---|---|
| A conversation (Settings menu) | The Rich panel at the right, "Bust a bug · Running", over the conversation | Yes. Typed words landed in the panel's box (`AXTextArea` at x 1152), with no click first | A, E, F, G, I, J |
| Corrections open | The panel over Corrections, which stays open | Yes, by typing and by talking (below) | B3 |
| The home screen | The panel over the home picture, "Bust a bug · the home screen" | Yes | B3 |
| The held opening screen | The panel over the held screen, "Bust a bug · the opening screen". The screen stays held | Yes | B3 |
| The settings menu itself | Every row above is a press of the menu's "Bust a bug!" | — | all |

Frames: `a-ask-conversation-light`, `b-talk-corrections-light`, `b-ask-opening-screen`,
`b-card-opening-screen`, `b-ask-home`, `b-card-home`.

**The panel's talk button answers too.** Run B3 pressed it with Corrections open.
- The button became `AXToggle value='1'` and the box changed to "listening…".
- The sentence came from a WAV file (`say -o`, then `afconvert`; nothing was played) through
  `RICHOS_VOICE_INPUT_WAV`: "The Corrections window is empty, even though I corrected a word
  yesterday."
- It arrived in the panel as the user's words with the "Said aloud" mark. Rich wrote the card
  from it.
- Run F did the same in dark, from a conversation (frame `f-talk-on-dark`).

### D28 (blocks). The conversation behind the panel takes the keyboard back

**What a user does.** They press Bust a bug in a conversation and start typing their answer, or
start typing a change once the card is there.

**What happens.**
- After a while the text box focus jumps to the conversation's own message box, behind the panel.
- Nothing on the panel changes. Its box just loses the caret.
- The next words go into the conversation's box. There, Return sends them to Rich as a
  conversation message, and the report never gets them.

**Proven by typing, run I** (walk `walkI.json`):
- The panel was asked "What went wrong?". "markerone" was typed. It landed in the panel's box
  (x 1152, frame `i-marker-in-panel`).
- 45 s later, with no click and no key in between, " markertwo" was typed. It landed in the
  conversation's box (`AXTextArea` at x 735). The panel still showed "markerone" (frame
  `i-marker-in-conversation`).
- In those 45 s the conversation behind had finished its first-run "Not now" ("Left with you.
  Ask me any time…") and shown the account notice.

**Seen at the card, runs F, G and J.** When the card arrived, the caret and gold focus border were
on the conversation's box, not on the panel's "Tell Rich what to change, or press Send" box
(crop `g-keys-in-conversation-crop`, frame `g-keys-in-conversation-light`).
- Shift+Tab from there walked the conversation (its talk button, "Copy Rich's message", "Not
  now"…), never the card.

**How often:** seen in 4 runs, F, G, I and J.
- **Where the panel kept the keyboard:** run A at both of its cards. The caret is in the panel's
  box in frame `a-card-private-light` and in the uncommitted frame of the second card. Also run
  B3 at all three of its entrances.
  - In A and B3 the first-run "Not now" had finished before Bust a bug was pressed: its "Left
    with you…" line is already in their first frames.
  - In I it finished while the panel was open.
- **Run E does not say either way.** Its card frame was taken after the Settings menu had been
  used.

**Why it blocks.** The brief's words are "answerable at once by typing". Here the user's answer
can go into a conversation turn instead of the report, with nothing on screen saying so.

**Hypothesis from code, not seen:** `main.js` calls `inputEl.focus()` with no check for the
panel. One is at the end of `declineFirstRunInterview` (line 6521), after the bridge answers.
Others are `exitVoiceMode`, `retryTurn`, the dictation offer and the company-added paths. None
looks at `body.bug-dock-open`. Only the boot-time call checks `openPopups()`, and the panel is
not counted there. So any of them that finishes while the panel is open takes the keys.

### Disabled while a report is open: PASS

- **While the ask or the card was open,** the menu's "Bust a bug!" was `enabled=false` (runs A,
  E, F). The accessibility press was refused: "disabled: matched element is disabled".
- **It is drawn disabled:** gray text and icon (crops `e-bust-a-bug-enabled-crop`,
  `e-bust-a-bug-disabled-light-crop`, `e-bust-a-bug-disabled-dark-crop`).
  - It gives no reason: its `AXHelp` is empty.
  - The panel beside it is the reason, and it is in view.
- **After Cancel** it was enabled again (run A step 58). Its press started a new report: the
  panel's words were cleared, and the new question had focus.
- **After closing the panel with ✕** it was enabled again (run A step 122).

**Send, and the waiting report.** A real Send was not possible (no token, network blocked), so
"after Send" was seen as "after a Send that left the report waiting".
- Then the panel stays up with the waiting card, and Bust a bug stays disabled (run A step 112).
- The ✕ is offered in that state. After ✕, Bust a bug is pressable. Its press opens the panel on
  the same waiting report, with focus in its box (frame `a-pressed-again-light`).
- That matches `bug-report.js` `start()`: "the press opens the panel on it again, which is the
  way back to it".
- A user with a report that cannot go (no reporting account) therefore cannot start a second,
  different report without canceling the first. Noted, not a defect against the stated design.
- **Not walked:** a pointer click on the disabled item. In the default window it sits below the
  menu's edge (see "Seen, not new"). The two pointer clicks aimed at it missed it.

### Changing the card with a "-" line

**As its own paragraph: PASS** (run E).
- Change it, Tab into What happened, type "- It also happens right after a restart", then Return
  twice.
- Done, then Send. There was no "words would go out differently" round: it went straight to
  "Waiting to send · saved on this Mac".
- The waiting card shows "- It also happens right after a restart" as its own line, not a bullet
  and with no backslash (`e-06-waiting.tree` line 142; frame `e-dash-paragraph-done-light`).
- The saved report (`reason` "not-set-up", no `sent.jsonl`) has the line on its own, unindented:

  ```
  ### What happened

  \- It also happens right after a restart

  In a conversation, the user pressed the Send button twice in quick succession. …
  ```

  The backslash is the Markdown escape `53407e2ae` adds, so GitHub shows "- It also…" as written.

**D27 (does not block). A single Return is joined into the next sentence** (run A).
- The same line was typed with one Return. While changing it, and after Done, the card showed two
  lines (frame `a-dash-line-editing-light`).
- After Send, Rich: "I checked it again before sending, and some of its words would go out
  differently from how the card showed them. Here it is exactly as it would go."
- The card then read "- It also happens right after a restart In a conversation, I pressed…": one
  line (frame `a-dash-line-after-check-light`).
- A diff of the two trees shows that this joined line is the only change (the rest is the panel's
  own status lines). No word changed.
- So the message says "words" for a line break, and the line the user wrote is not kept.
- Send again left it waiting, saved as `\- It also happens right after a restart In a
  conversation, …`.
- **By the code:** `paragraphs_of` reads "a single line break inside a paragraph … as a space, as
  before". That is the long-standing rule, not new in 49.
- **Why it does not block:** the user is shown exactly what will go, and nothing goes before the
  second Send.

### D29 (blocks). After Done, the card's buttons are drawn cut off

**What a user does.** They press Change it, edit, then press Done.

**What happens.**
- The sticky button row shows a gold "Send rep", cut off at its right edge.
- No Change it and no Cancel are drawn. The card's next heading, "Steps to see it", shows under
  the row.
- Crops `e-actions-after-done-crop` and `a-actions-after-done-crop`; frames
  `e-dash-paragraph-done-light`, `a-done-light`.

**The accessibility tree has all three at full size:** Send report 123 wide, Change it 107,
Cancel 87 (`e-04-done.tree` lines 117 to 119). So what is drawn is not what is there.

**How often:** 2 of 2 times, in runs A and E.
- Run E's frame was taken 34 s after Done, so it is not a passing animation.
- The pressed Send report still worked, by accessibility press.

**Why it blocks.** It is the screen where the user approves a public report. Cancel and Change it
cannot be seen.

**Hypothesis, not seen:** a repaint missed when Done turns into Send report. Done is 74 wide, and
the row is drawn at about that width. `13aa9bc0f` also removed the card's negative side margins
in the panel, for D25.

**Not tested:** whether a hover or a scroll redraws it, and a physical Mac.

### Seen, not new or not blocking

- **"Bust a bug!" can sit below the Settings menu's edge.** In the guest's 1400x864 window, with
  the Claude accounts row showing "90% of this week used" on a second line, only the top few
  pixels of the item show (crop `a-menu-bottom-crop`, frame `a-menu-while-report-open`).
  - The menu scrolls to it: `style.css` `max-height` with `overflow-y: auto`.
  - Without the usage line it fits (run E, frame `e-menu-disabled-light`).
  - The menu's height rule is unchanged since 48. Noted because CEO §15 makes Bust a bug the
    menu's one item that must always be at hand.
- **A "“home” looks private" heads-up on the home-screen card** (frame `b-card-home`).
  - The guest's home folder is literally named "home", so the app's private-word list holds it.
  - On a real Mac that folder carries the user's name. A guest artifact; not judged.
- **The spoken sentence lost its first word.** It came through as "sidebar flickers every time a
  new message arrives." The file is the speech engine's, not the app's. Not judged.

## Contrast (`qa/contrast.py`), elements new or changed in 49

| What | Light | Dark |
|---|---|---|
| the line under the card, with stand-ins | 6.33:1 | 5.78:1 |
| panel header "Bust a bug" | 17.02:1 | 13.02:1 |
| panel header "· Running" | 6.15:1 | 6.09:1 |
| panel "listening…" | 6.15:1 | 6.09:1 |
| "Bust a bug!" enabled | 18.07:1 | not photographed (unchanged from 48) |
| "Bust a bug!" disabled | 5.37:1 | 5.51:1 |
| "“home” looks private." (gold) / rest of the heads-up | always-dark panel: 6.36:1 / 12.06:1 | same |
| Send report, always-dark panel | 7.68:1 | same |
| focus ring on Change it (non-text) | not photographed | 6.36:1 |
| held-queue status line, `--ink-faint` on the page | 4.99:1 | 6.11:1, the same token measured on the "Worked" label, dark frame `f-talk-on-dark`; the held run takes no dark frame |
| held notice | 18.07:1 | not photographed (48 measured the notice style at 12.06:1) |

- **The disabled "Bust a bug!"** is an inactive control, which WCAG 1.4.3 exempts. **Declared
  exempt here.** It clears 4.5:1 anyway.
- **Not measured:** a stand-in's underline, which was never on screen (see D23).

## Verification mode

- **Environment:** the test VM, one fresh guest per run through `run-walk.py`, never this Mac's
  screen.
  - One run at a time.
  - One app window per guest; no two-user test.
  - Guest screen 1680x1050 at 1x; the app window 1400x864. Run E tried 960 high, and the guest
    allowed 887.
  - Light and dark through the guest's macOS appearance.
- **Artifact:** the candidate's own signed zip, with the same folder's
  `richos-engine-1.2.0.tar.gz` as `--engine`.
- **Account:** the real Claude login that `run.sh` copies into each guest. The held-queue run
  used its committed fake Claude. No account file on this Mac was written.
- **GitHub:** no reporting token, and `api.github.com` unreachable inside every guest that
  pressed Send.
- **Audio:** nothing was played. The talk button read WAV files written in the guest by `say -o`.
- **Verification basis:**
  - Every PASS and FAIL above was seen on the running candidate in the guest: its accessibility
    tree, its frames, the app's own files in the guest, and the walks' reports.
  - Lines marked "hypothesis" or "by the code" are read from source and were not seen.

### Runs

| Run | VM | Walk | Result | Seconds |
|---|---|---|---|---|
| A | `walk-6f75efdd2dbd` | `walkA.json` | conversation entrance; D23 both kinds; disabled while open; Cancel; "-" line with one Return; waiting; ✕; pressed again. Exit 0 | 805 |
| B | `walk-874f4d0b1369` | `walkB.json` | harness: `steps-walk.py` resolved a relative push one folder too high. Fixed and committed (see Method). Nothing of the app tested | 30 |
| B2 | `walk-f0203e4df460` | `walkB.json` | harness, mine: my own commit's check set the uncommitted step lists aside as the run read them. Nothing of the app tested | 32 |
| B3 | `walk-bb262fc60879` | `walkB.json` | Corrections by pointer and by tree; talk button; held opening screen; home screen. Exit 0 | 700 |
| E | `walk-3d5b97aac12a` | `walkE.json` | disabled item drawn; "-" line as its own paragraph, waiting; D29. Exit 0 | 453 |
| F | `walk-e72e546d1bdf` | `walkF.json` | third private report; talk in dark; D28 at the card. Exit 0 | 404 |
| H | `walk-93108d35e801` | `held-queue-walk.sh` | D26; every check ok. Exit 0 | 348 |
| G | `walk-2db9f9f3a1ed` | `walkG.json` | private report again; D28 at the card. Exit 0 | 263 |
| I | `walk-56a50cbdc0b7` | `walkI.json` | D28 proven by typed markers. Then it stopped at step 34: my accessibility press on the panel's box does not move focus, so the report text went elsewhere and no card came. Where it went was not captured | 365 |
| J | `walk-0cfeb5895873` | `walkJ.json` | D28 again at the card; a stand-in's focus was not reached. Exit 0 | 223 |

## Method and toolkit

The `qa/` tools are unchanged from `b6881d349`. `testvm/steps-walk.py` is this branch's fixed
copy (below).

**`testvm/`:**
- `run-walk.py --wait 900` with `TESTVM_AX_TIMEOUT=60`, and `TESTVM_APP_ENV` for the voice file;
- `steps-walk.py --steps … --out …`, and `--check` on every list;
- `held-queue-walk.sh`, as committed;
- `ax.sh` (find, click, type, `--focused`, `--key`, `--at`, `--windows`), `guest.sh`, `shot.sh`,
  `relaunch.py` through the `relaunch` step;
- `slots.py status`.

**`qa/`:**
- `contrast.py` on frame regions;
- `frame.py crop`, and `frame.py px` inside the guest for the opening-screen hold;
- `ocr-gate.sh` with its positive control;
- `redact.py --homes --box`;
- `wait-for.sh --file`.

**Elsewhere:** `nightly-local.py candidate`, `shasum`, `jq`, `git log`, `git diff`, and `sips` for
the JPEG copies.

**Scripts written from scratch: none.**
- **The step lists.** Seven were added under `steps/`, declared in `steps-walk-data.test.sh`'s
  inputs and covers rows, and that suite passes (3 ok lines).
- **Inline Python** copied frames by a list and converted them with `sips`. It measures nothing.
- **One committed tool was fixed:** `52c2d5ea6`.
  - `steps-walk.py` resolved a `push` source against the folder above the checkout
    (`HERE.parents[4]`). So every relative push failed: run B, "no such file on this Mac".
  - It now resolves against the repository root.
  - `steps-walk.test.py` gained two checks: a relative source names an existing file in the
    repository, and an absolute source is used as written. 0 failed.

### Frames

- **Format:** JPEG q75 copies of the 1680x1050 PNG originals. The `*-crop` and `d25-*` frames are
  crops.
- **Names:** the run letter, then what the frame shows.
- **The privacy gate.** `ocr-gate.sh` ran over the 37 PNGs the JPEGs were made from. Its positive
  control found the known address first.
  - It flagged 4 frames carrying the guest's own home path, in Corrections' "What I believe".
  - Those were covered with `redact.py --homes` and a `--box` over that paragraph.
  - The second gate: "0 of 37 frame(s) carry something that must not ship".
- **The text gate.** `ocr-gate.sh --text` over this record and its seven step lists, after its
  own positive control: "0 of 8 text file(s) carry something that must not ship".
- **The names in the frames are invented for the walk:** Acme, Dana Whitfield, Northwind Traders,
  Contoso.

## Surfaces

**Applied and checked:**
- the Mac desktop app from its signed bundle, in light and dark;
- every entrance to Bust a bug the brief names: the Settings menu on a conversation, with
  Corrections open, on the home screen and on the held opening screen;
- the Settings button by pointer and by accessibility press;
- the panel's answer by typing and by its talk button;
- the reverse paths: Cancel, ✕, Never mind, and pressing again after each;
- a clean quit and a fresh launch (the opening screen);
- the held queue with its fake Claude.

**Not applicable:**
- phones: nothing touched;
- battery: no `richos/mobile/**` change;
- the stable channel and publishing: nothing was published.

**Not covered:**
- a real Send to GitHub (forbidden by the brief);
- VoiceOver itself (the accessibility tree was read instead);
- a physical Mac;
- a pointer click on the disabled item;
- a stand-in's tip.

**Test data:** each guest was a fresh clone with its own empty home. Nothing touched this Mac's
app data, and no app was started on this Mac.

## Cleanup

- **Every run** ended "clean: app quit, VM stopped, clone deleted, state removed." and released
  its slot. All ten are in the table above.
- **After the last run:**
  - `slots.py status` read "guest.lock: free" and "guest-2.lock: free";
  - `~/.richos-testvm/tart/vms/` holds only `richos-base`;
  - `~/.richos-testvm/run/` is empty.
- **Scratch:** `/Volumes/E1TB/tmp/claude/ray-opus-walk49/` is deleted after this record is
  committed.

## Defects, in one list

- **D28 (blocks).** While the Rich panel is open, the conversation behind it takes the keyboard
  back when it finishes something (seen: the first-run "Not now" and the account notice).
  - The user's next words go into the conversation's box, and Return there sends them to Rich as
    a conversation message.
  - Seen in runs F, G, I and J; proven by typing in I. Runs A and B3 kept the keyboard in the
    panel.
- **D29 (blocks).** After Done, the card's sticky buttons are drawn as a cut-off "Send rep". Change
  it and Cancel are not drawn, though the tree has all three. Seen 2 of 2, runs A and E.
- **D27 (does not block).** A line typed with a single Return is joined into the next sentence at
  Send, and Rich says "some of its words would go out differently" when only the line break
  changed. As its own paragraph the line goes as written.
- **Fixed from 48:** D23, D24, D25, D26.
- **Seen, not new:** "Bust a bug!" below the Settings menu's edge when the Claude accounts row
  shows its usage line.

Verdict: NOT READY
