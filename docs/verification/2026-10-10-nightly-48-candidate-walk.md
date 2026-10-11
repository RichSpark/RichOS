# Walk of nightly candidate 48 (`v1.2.0-nightly.20261010.48`), 2026-10-10

**Verdict: NOT READY.** Two defects in Bust a bug block it. Everything else new since candidate 47
passed on the candidate's own signed bundle in the test VM.

| Item | Result |
|---|---|
| 1. Bust a bug: from a conversation, the home screen and the held opening screen | **PASS** |
| 1. Bust a bug: with Corrections open | **FAIL**, D24: the Settings button does not open its menu, so the report cannot be started (3 of 3) |
| 1. The card's words: Rich asks, Rich writes, private details replaced, his text on the card | **PASS** |
| 1. The card's privacy line | **FAIL**, D23: it says "Nothing private was in it, so nothing was left out" under a report full of stand-ins (2 of 2 reports with private details) |
| 1. Editing (plain text only, a pasted list stays plain), the edit goes back through his check | **PASS** |
| 1. Send with no token and Send offline: the report waits and says so; nothing sent before Send | **PASS** |
| 2. The Codex review switch: off on first run, all five states, each state's words | **PASS**. All five reached; four needed a stand-in Codex command (see item 2) |
| 3. D20, D21, D22 | **PASS** in light and dark |
| 4. A held queue | **PASS** for the sign-in hold; the "work connection would not open" hold could not be made to happen in the guest |

Candidate 47 was READY (`docs/verification/2026-10-08-nightly-47-candidate-walk.md`). What passed there
was not walked again.

## What changed since candidate 47, re-derived

- **Source.** 47 was built from `7b24dbad8`, 48 from `d0d06501c`. `git log --first-parent
  7b24dbad8..d0d06501c` lists 23 merges.
- **The ones a user can see**, all four walked:
  - `f6b15d6fa`: Bust a bug (round 21);
  - `6fcafe502`: the Codex review switch in Settings (round 20.2);
  - `3f71aff98`: D20, D21, D22 (the inline talk icon and the setup sheet's Tab);
  - `3fca55926` and `5b0b0c3d9`: the held queue.
- **The rest** are the second-review machinery (`d93a57151`, `ad45b09c4`, `2ec9241b1`, `9eb83287a`,
  `61df60963`), commit and merge checks, crash-dialog and browser fixes for the test suites, the
  replay harness, the watch-video skill, a license inventory and the 47 walk record.
  - `9eb83287a` runs second reviews inside the app. Its only on-screen part a user meets is the
    Codex switch (item 2). Its reviews run when a team member hands over work, which no guest run
    here did. Not walked.
- **The brief's line reference holds.** `richos/app/ui/codex-reviews.js` lines 13 to 17 describe
  five states: off, on, missing, signed out and lapsed.

## Identity, checked before any claim: PASS

- **The zip.** `shasum -a 256` of `RichOS-1.2.0-nightly.20261010.48-macos-aarch64.zip` gives
  `d1ec6adee4d17e9ec107a7b08968f64c5c45abc5d7971e70888412fe33efa612`. Both of its `SHA256SUMS`
  lines carry it: the versioned name and `RichOS-macos-aarch64.zip`.
- **The engine.** `richos-engine-1.2.0.tar.gz` gives
  `f57379193bd9d7a0397b96511996d2b3981be4d55740221a288e92256248e82e`, its `SHA256SUMS` line.
- **`candidate.json` `info`:**
  - `source_commit` `d0d06501c014c6e198a02513eb9eef20c965fe18`;
  - `run_id` `20261010T032015Z-af885386`;
  - `version` `1.2.0-nightly.20261010.48`;
  - `build_commit` `4a19b92e54395c6ec5249c52fe274814167a39d0`.
- **The build commit.** Its parent is `d0d06501c`. `git diff --stat d0d06501c 4a19b92e5` touches
  only `nightly-build.json`, `Cargo.lock`, `Cargo.toml` and `tauri.conf.json`.
- **The Settings version line** reads "RichOS 1.2.0-nightly.20261010.48 is up to date." (run C3
  step 13, and run B step 13, accessibility find).

## 1. Bust a bug

Every Bust a bug run used **the real Claude login** that `run.sh` copies into each guest, so the
report was written by the user's own Rich, not by the walk's fake.

**Nothing could reach GitHub.**
- The guest has no reporting token: the keychain lookup in the app's HOME exited 44, "no such item"
  (runs B, B2, B3).
- Before any Send, each run pointed `api.github.com` at the guest itself in `/etc/hosts` (IPv4 and
  IPv6).
- Then a `curl` to it had to fail, or the run stopped. It failed every time: "Failed to connect to
  api.github.com port 443".
- The same check ran again at the end of run B2: still unreachable.
- No issue was created anywhere.

### Where it opens: three entrances pass, Corrections fails

| Where Bust a bug was pressed | On top? | Answerable at once? | Run |
|---|---|---|---|
| A conversation (Settings, "Bust a bug!") | Rich asks in the conversation under a "Bust a bug" divider: "What went wrong? Tell me in your own words, typed or out loud, and where it happened if it wasn't here. I've noted that you were on the Running conversation." | Yes. Focus is in the composer (`AXTextArea` "Message to Rich", placeholder "Tell Rich what went wrong…"). The answer was typed with no click first and landed there | B, B3, R |
| The home screen | The Rich panel at the right, "Bust a bug · the home screen", over the home picture | Yes. Focus is in the panel's box; typed words landed | B2 |
| The held opening screen (Space held it; Settings showed while held) | The Rich panel over the held screen, "Bust a bug · the opening screen". The opening screen stays held behind it, and spaces typed into the answer stay in the answer | Yes. Focus is in the panel's box | P, P2 |
| With Corrections open | **Not reached. D24** | — | B3, R, K |

Frames: `b-ask-conversation-light`, `b2-ask-home`, `p2-typed-held-opening-screen`,
`p2-card-held-opening-screen`.

### D24 (blocks). With Corrections open, the Settings button does nothing, so a bug cannot be reported from there

**What a user does.** They open Corrections, then click the Settings button at the top right to
find "Bust a bug!".

**What happens.**
- The button shows its hover look and no menu opens (frames `k-settings-over-corrections`,
  `r-settings-over-corrections`).
- A second click does nothing either (run R).
- Focus stays on Corrections' "Close corrections" button.
- A click where "Bust a bug!" would be lands on the conversation behind, and that closes
  Corrections.

**How often:** 3 of 3 times, in runs B3, R and K, each a fresh guest.
- B3 and R had a canceled report in the conversation first; K had none.
- In each, Corrections had moved focus into itself, onto "Close corrections".

**The one time it worked:** run Q.
- There the menu opened over Corrections (frame `q-settings-over-corrections-opened`).
- In that run, focus had stayed on the rail's Corrections button after it opened.
- So the failing state is the normal one, focus inside Corrections. That is a reading of four
  runs, not a proven cause.

**The same pointer click works elsewhere on this build:** it opened the Settings menu with no window
open (run Q, `q-01`), and it pressed the Codex switch (run C3).

**A screen-reader user cannot reach it either.** While Corrections is open, the Settings button
is not in the accessibility tree at all: `ax.sh` reports "blocked: modal=Corrections; nothing
matched" (run B, step 85).

**Not seen, because of this:** the Rich panel beside an open Corrections window, which the brief
asks for.

### The report Rich writes: PASS, except its privacy line (D23)

**What the user typed** (run B): "In the Acme conversation, the names in the sidebar get cut off
when I make the text bigger. Dana Whitfield at Northwind Traders sees the same thing, and my notes
in ~/Library/Application Support/Client Plans/budget.xlsx now look empty."

**What the card says** (frames `b-card-light`, `b-card-dark`):
- From: "the RichOS reporting account". To: "RichOS on GitHub, as an issue anyone can read".
- Pill: "Not sent yet".
- Title: "Sidebar names are cut off when the text size is made bigger".
- What happened: "In [a conversation], the user made the text bigger and the names in the sidebar
  on the left got cut off. They say [a person] at [a company] sees the same thing. They also say
  their notes in [a file on this Mac] now look empty."
- Then Where, Steps to see it (three), What the user expected, and Version ("RichOS 1.2.0,
  nightly 48 · macOS 15.7.7 · Apple silicon").
- Above the card, Rich: "Here's the report as I'd file it. Nothing goes out until you press Send.
  Anyone can read GitHub issues, so I left out names, company details and file paths."
- Above that: "Looked at the screen you were on · Checked the version".

**No private word reached the card or the saved report.**
- None of Acme, Dana, Whitfield, Northwind, "Client Plans" or budget.xlsx is on the card.
- Run B's saved report (the JSON in `bug-reports/waiting`) carries the title and body word for
  word, and none of them.
- The user's own message above the card still shows what they typed. It is never part of the
  report.

**D23 (blocks). The card says nothing private was in it, right under a report full of stand-ins.**

- **Under the card:** "Nothing private was in it, so nothing was left out. Anyone can read GitHub
  issues."
- **Rich, two lines above:** "…so I left out names, company details and file paths."
- **The report itself:** four stand-ins.

The stand-ins are plain words, not marked.
- The accessibility tree has the whole paragraph as one text node (`b-03-card.tree`).
- So none of them is underlined, and none can be pointed at to see what it replaced.
- The design's "The underlined words stand in for them; point at one to see what it replaced" line
  never appears.

**Reproduced.** Run R typed "My colleague Dana Whitfield at Northwind Traders says the Send button
flickers when she presses it, and the file ~/Documents/Client Plans/budget.xlsx did not attach."
- The card said "a colleague, [a person] at [a company] … [a file on this Mac]" and the same
  "Nothing private was in it" line (frame `r-card-private-light`).
- A find for "Left out, because" matched nothing.
- That is 2 of 2 reports with private details.

The two reports with nothing private in them (runs B3 and P2) say the same line, and there it is
true.

**After a change by hand** the line is still wrong:
- Rich's check replaced the typed "Acme" in the title with "[a company]". That one stand-in is
  underlined.
- The line then read "Left out, because anyone can read GitHub issues: one company name. The
  underlined words stand in for it…"
- But four more stand-ins sat unmarked in the paragraph below (frame `b-after-check-light`).

**Why it matters.** This is the screen where the user decides to send something to a public page.
It contradicts itself about exactly that. The words that would go are clean; the explanation of
them is wrong.

**Hypothesis from code, not seen:** `richos_core::bug_report::finished` marks a stand-in only when
the scanner's or Rich's own `private` list has an entry of its kind. On these reports neither list
gave one, so the stand-ins Rich wrote stayed plain words.

### Changing the card: PASS

Run B: Change it, then type, then paste a list, then Done, then Send.
- **Change it** made the card editable ("Changing it"). Focus went to the title, caret at the end.
  Text typed there went in, and "“Acme” looks private. Anyone can read this report on GitHub."
  appeared at once (frame `b-pasted-list-light`).
- **Plain text only.** The guest's clipboard held an HTML list (`<ul>` with a bold and an italic
  item) and its plain text. It was pasted (Command-V) into the next field.
  - It went in as two plain lines, "Open Settings" and "Make the text bigger": no bullets, no bold,
    no italic.
  - The saved report carries them as plain words ("Open Settings Make the text biggerIn…"; the
    missing space is the walk's paste, which ended without one).
- **Done** turned the pill to "Not sent yet · changed".
- **Send went through Rich's check first.** About 8 s after Send, the card came back:
  - Rich said "I checked it again before sending, and some of its words would go out differently
    from how the card showed them. Here it is exactly as it would go. Nothing goes out until you
    press Send.";
  - the title now read "…for [a company] at larger text" (frame `b-after-check-light`);
  - nothing went until Send was pressed again.
  - The pill "Rich is checking your changes…" was not caught: a find one second after Send matched
    nothing. Only the outcome was seen.
- **Cancel** on a fresh report (run B3) gave "Canceled · nothing sent", a struck-through title and
  Rich's "Canceled. Nothing was sent." (frame `b3-canceled-light`).

### Send: PASS. The report waits and says why; nothing is sent before Send

**No token** (run B; frames `b-waiting-not-set-up-light`, `b-waiting-not-set-up-dark`):
- Pill: "Waiting to send · saved on this Mac". Buttons: Try now, Change it, Cancel report.
- Rich: "The RichOS reporting account isn't set up on this Mac yet, so the report didn't go out.
  Nothing is lost: it's saved on this Mac exactly as you approved it. I'll send it by myself once
  the account is set up, and tell you here when it's filed."
- On disk: one file in `bug-reports/waiting`, `"reason":"not-set-up"`, and no `sent.jsonl`.

**Offline** (run B2): a stand-in token, `walk-stand-in-not-a-real-token`, was put in the guest's
keychain, with `api.github.com` still unreachable. Then Send.
- Rich: "This Mac is offline, so the report didn't go out. Nothing is lost: it's saved on this Mac
  exactly as you approved it. I'll send it by myself as soon as you're back online, and tell you
  here when it's gone." (frame `b2-offline`).
- On disk: `"reason":"offline"`, and `sent.jsonl` absent ("No such file or directory").

**Nothing before Send.**
- Before the first Send, `bug-reports` held only an empty `writing/` folder (run B step 41).
- No `sent.jsonl` existed in any run.
- The basis is the app's own store plus a blocked network. No traffic capture was taken.

### Seen in Bust a bug, not blocking

- **D25 (polish).** The Rich panel shows a horizontal scroll bar under its messages, its thumb
  almost the panel's width (frame `b2-panel-scrollbar`, runs B2 and P2): the content is a few
  pixels wider than the panel.
  - The guest has no trackpad, so macOS shows scroll bars all the time.
  - On a trackpad Mac it shows only while scrolling. The overflow is there either way.
- **Bust a bug pressed again while a report is open in the panel continues that report.**
  - The new words were taken as a change: "Added that to What happened, above. It still waits for
    you to send it." (run B2, frame `b2-card-home`).
  - The placeholder says so ("Tell Rich what to change, or press Send…"). That matches the file's
    stated design. Noted only because a user pressing Bust a bug may expect a new report.
- **The panel's ✕ is hidden while a report waits on a decision** (draft, editing, sending). By the
  code that is deliberate: Send or Cancel is the way out.

### Contrast, Bust a bug (`qa/contrast.py`)

| What | Light | Dark |
|---|---|---|
| Rich's question (conversation) | 14.90:1 | 14.55:1 |
| "Rich" label | 4.99:1 | 6.11:1 |
| "Bust a bug" divider | 5.83:1 | 6.48:1 |
| Never mind | 5.83:1 | 6.48:1 |
| composer placeholder | 5.37:1 | 5.51:1 |
| card: To line | 6.33:1 | 5.78:1 |
| card: section heading ("What happened") | 5.98:1 | 6.51:1 |
| card: body and Version | 15.58:1 | 14.78:1 |
| card: privacy line | 6.33:1 | 5.78:1 |
| Send report (dark on gold) | 4.72:1 | 7.68:1 |
| Change it, Cancel | 6.33:1 | 5.78:1 |
| editing: "“Acme” looks private." | 6.65:1 | 6.36:1 |
| editing: "Anyone can read this report on GitHub." | 18.07:1 | 12.06:1 |
| editing: "Change any of the words above…" | 6.33:1 | 5.78:1 |
| Done | 4.72:1 | 7.68:1 |
| waiting: Rich's line | 14.90:1 | 14.55:1 |
| Try now | 4.72:1 | 7.68:1 |
| Cancel report | 6.33:1 | 5.78:1 |
| canceled pill / struck title | 6.33:1 / 13.08:1 | not photographed |
| "Worked for 14s" / "Looked at the screen…" | 4.99:1 / 4.99:1 | not photographed |
| user's message bubble | 5.50:1 | not photographed |

**The Rich panel over the home and opening screens is always dark.** §15 forces those screens dark,
so light and dark frames were identical:
- header 13.02:1, "· the home screen" 6.09:1;
- "Rich" 5.80:1, question 13.02:1, Never mind 6.09:1;
- input placeholder 12.06:1;
- privacy line 5.78:1, user bubble 5.26:1;
- Try now 7.68:1, Version 14.78:1.

**Not measured:** the stand-in's underline (dotted, one pixel), a supporting mark beside the
bracketed words, which carry the meaning themselves.

## 2. The Codex review switch: PASS

Run C3 (`walkC.json`). Settings opens on the row, right under Technical view.

**Off on first run.** At the first opening, the switch read `AXSwitch value='0'`, "Reviewing now:
Claude".

**Five states, all reached.**
- The guest has no Codex. It has no ChatGPT.app in either Applications folder and no `codex` in
  `/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin` or the app HOME's `.local/bin` (step 0).
  So **missing** is what a guest without Codex reaches by itself.
- The other four need Codex. A stand-in `codex` was written to the app HOME's `.local/bin`, one of
  the folders the app searches (`codex_reviews.rs` `search_list`).
- It answers only `login status`: exit 1 "Not logged in", or exit 0 "Logged in using ChatGPT".
- The row read it again each time Settings opened.

| State | How | Switch | "Reviewing now" | ⓘ words (seen in the tree, verbatim to `codex-reviews.js`) |
|---|---|---|---|---|
| missing | no codex | dashed track, hollow knob; a press shakes it, tints the row and opens the tooltip; value stays 0 | Claude | "…Codex could do it instead… **The Codex app isn't on this Mac.** Install it and sign in to ChatGPT, and this switch can be turned on." |
| signed out | stand-in, exit 1 | as missing; a press nudges | Claude | "**Codex isn't signed in.** Open the Codex app and sign in to ChatGPT, and this switch can be turned on." |
| off | stand-in, exit 0 | normal, off | Claude | "…Turn this on and Codex does it instead…" plus the allowance and "won't show up in your ChatGPT or Codex app" lines |
| on | pressed | gold, `value='1'` | **Codex** | "Codex reviews your team's finished work… Turn this off and Claude reviews it again." plus the same "won't show up" line |
| lapsed (missing) | on, then the stand-in removed | gold, still on | Claude | "You turned this on… **The Codex app isn't on this Mac right now,** so Claude is reviewing in the meantime…" |
| lapsed (signed out) | stand-in back, exit 1 | gold, still on | Claude | "**Codex isn't signed in right now,** so Claude is reviewing in the meantime…" |

**It can always be turned off.** Pressed in the lapsed state it went to off (`value='0'`), and the
row showed the signed-out words again.

**Esc puts away only the tooltip.** After one Esc the menu was still open (C3 step 29).

Frames: `c-missing-tip-light`, `c-missing-nudge-light`, `c-signedout-tip-dark`,
`c-off-ready-tip-light`, `c-on-tip-dark`, `c-lapsed-missing-tip-light`,
`c-lapsed-signedout-tip-dark`.

| What | Light | Dark |
|---|---|---|
| row name | 18.07:1 | 12.06:1 |
| "Reviewing now:" | 6.33:1 | 5.78:1 |
| tooltip body (every state) | 6.33:1 | 5.78:1 |
| tooltip bold reason and aware block | 18.07:1 | 12.06:1 |
| lapsed: the gold reason ("…isn't on this Mac right now," / "…isn't signed in right now,") | 6.65:1 | 6.36:1 |
| switch on: gold track on the menu (non-text) | 3.83:1 | 6.36:1 |
| switch on: dark knob on gold (non-text) | 4.72:1 | 7.68:1 |
| switch off: knob on the menu (non-text) | 4.51:1 | 4.99:1 |

The unavailable switch (dashed track) is an inactive control. WCAG 1.4.11 exempts it, and its
reason is in words beside it. **Declared exempt here.**

## 3. D20, D21, D22: PASS

Run D (`walkD.json`), as 47's run T. The speech models and the first decodes were waited out. The
talk button ran on 30 s of silence (`RICHOS_VOICE_INPUT_WAV`); nothing was played.

**D20. The inline icon follows the button.** While listening, the voice row's "tap [icon] to end
voice" draws the button's listening face: a dark disc and gold bars, in light and dark (frames
`d-voice-row-listening-light`, `-dark`). At rest, the greeting's icon is the resting face, as on 47.
- The greeting is not on screen while listening: the "faster hearing" notice takes its place. So
  the greeting icon's listening face was not seen.

**D21. It has a spoken name.**
- At rest the greeting's icon is `AXImage desc='Talk to Rich'` between "You can type, or tap " and
  " to talk to me." (`d-07-greeting-light.tree`).
- While listening, the voice row's icon is `AXImage desc='Stop talking'`, and the button is
  `AXToggle title='Stop talking' value='1'`.
- The icon's name after stopping could not be read: the greeting does not come back after a voice
  session.

**D22. Tab stays on the sheet's visible stops.**
- **The memory sheet** ("Where should I keep what you tell me?"): Tab went Not now, Set it up, Not
  now, Set it up. Shift+Tab went back the same way.
  - Never `AXWebArea`. The ring is visible in both themes (frames `d-memory-sheet-tab-light`,
    `-dark`).
- **The setup sheet** with the engine away and Claude signed out (run S, `walkS.json`, as 47's
  S): focus opened on Set it up.
  - Tab went Not now, then the link "sign up there first and pick the Max/Max 20x tier", then Set
    it up, then Not now.
  - Shift+Tab went Set it up, then the link, and Tab went back to Set it up.
  - Never `AXWebArea`. 47's empty stop is gone.
  - `AppleKeyboardUIMode` is 3.

| What | Light | Dark |
|---|---|---|
| voice row icon, listening: ring on the page | 3.77:1 (`#717477` on `#EAE6DD`) | 4.75:1 (`#7C818D` on `#0C1322`) |
| voice row icon, listening: gold bars on the dark disc | 3.94:1 (antialiased core `#8B7032`; token 4.72:1) | 6.26:1 (core `#AD9355`; token 7.68:1) |
| voice row icon, listening: dark disc on the page | 14.90:1 | 1:1, the disc is the page color; the ring outlines it, as on the button |
| memory sheet: gold focus ring on the sheet | 3.83:1 | 6.36:1 |
| setup sheet: the link's gold focus box | measured on 47 (3.83:1) | 6.36:1 (`#C2A35C` on `#182440`) |

**Seen, not new, does not block: the company sheet's Tab leaves the sheet.** On "Which company is
this copy of Rich for?", Tab went from the page to the rail behind the sheet:
- "RichOS: go to the home screen", Search, + New thread, Corrections, Feedback;
- none of them visible or usable while the sheet is up (frame `d-company-sheet-tab-5`).

D22 covers only the two first-run questions (`QUESTION_SELECTOR` in `main.js` says so and names the
entity picker as outside it). The company sheet opening with nothing focused was noted on 47.

## 4. A held queue: PASS for the sign-in hold

Run H: the committed `testvm/held-queue-walk.sh`, unchanged, on the candidate. It stages a hold
with a fake Claude: the back end refuses every turn because nobody is signed in. Every check passed.
- The front desk wrote two jobs down. Exactly one back-end turn was refused, for the first job.
- **What the user sees**, once (frame `h-held`): "I haven't started "Prepare the Northwind invoice
  summary" or the one request waiting behind it yet, because nobody is signed in to Claude on this
  Mac. Sign in from Settings, under Claude accounts, and they will start by themselves."
- **It resumes.** After the sign-in came back:
  - the first job started 48 s later, then the second, in order;
  - no front-desk turn was spent;
  - no work record says failed (registered, registered, then settled, settled). Frame `h-resumed`.

**Not reached:**
- **The other cause, "the work connection would not open"** (`work_host.rs`
  `HoldCause::BackEnd`). Nothing in the guest makes the work connection fail to open without
  building a way, and the brief says not to build one.
- **`5b0b0c3d9`** (a back-end hold no longer blocks an update). It needs an update to arrive while
  held. Not reached.

**D26 (small, does not block).** While held, the composer's status line reads "··· 2 assignments
starting", right under the notice that says nothing has started (frame `h-held`).

**Contrast:**
- The notice in light: 18.07:1. The status line: 4.99:1.
- The fake-Claude run took no dark frame. The same notice card style (`tl-notice`), the "faster
  hearing" notice of run D, measured 12.06:1 in dark and 18.07:1 in light.

## Verification mode

- **Environment:** the test VM, one fresh guest per run through `run-walk.py`, never this Mac's
  screen.
  - One run at a time after the lead's correction at 04:47Z; before that, two of mine queued
    behind each other once (see Runs).
  - One app window per guest, no two-user test.
  - Guest screen 1680x1050 at 1x; the app window 1400x864.
  - Light and dark through the guest's macOS appearance; the app's theme row was on "system".
- **Artifact:** the candidate's own signed zip, with the same folder's `richos-engine-1.2.0.tar.gz`
  as `--engine`.
- **Account:** the real Claude login copied into each guest by `run.sh`.
  - Run S removed that copy inside its own guest only.
  - The held-queue run used its committed fake Claude.
  - No token was revoked, and no account file on this Mac was written.
- **GitHub:** no reporting token, plus `api.github.com` unreachable inside every guest that pressed
  Send, so nothing could be posted.
- **Audio:** nothing was played; the talk button read a file of silence.
- **Verification basis:**
  - Every PASS and FAIL above was seen on the running candidate in the guest: its accessibility
    tree, its frames, the app's own files in the guest, and the walks' reports.
  - Lines marked "hypothesis" or "by the code" are read from source and were not seen.

### Runs

| Run | VM | Walk | Result | Seconds |
|---|---|---|---|---|
| C | `walk-bba14447a1b3` | `walkC.json`, first version | harness, mine: zsh refused an unmatched glob in step 0; nothing of the app tested | 32 |
| C2 | `walk-2c0c3eedf0b4` | `walkC.json`, second version | harness, mine: `ax.sh` will not press a disabled switch by title; first steps (missing state) good; stopped at step 30 | 205 |
| D | `walk-a5f97e67862e` | `walkD.json` | every required step passed, exit 0 | 383 |
| B | `walk-47b13f370cc4` | `walkB.json` | conversation entrance, card, edit, paste, check, waiting: done. Stopped at step 85: Settings unreachable by the tree with Corrections open (D24) | 617 |
| B2 | `walk-ec1baff89e0d` | `walkB2.json` | home screen entrance twice and offline: done, exit 0. The "held" frames show the home screen: the Space landed before the opening screen could hear it | 659 |
| C3 | `walk-fa222f70f260` | `walkC.json`, as committed | all five states, exit 0 | 560 |
| B3 | `walk-626f1cf7bf67` | `walkB3.json` | ask and editing in dark, Cancel; Corrections: menu did not open (D24); exit 0 | 487 |
| P | `walk-144d5987fbd6` | `walkP.json` | opening screen held, report asked and answered; the card's tree timed out (`ax.sh` 124) before its frame | 483 |
| Q | `walk-1ee211cbca6f` | `walkQ.json` | the pointer opens Settings with no window, and once over Corrections; exit 0 | 254 |
| P2 | `walk-55dd98b514d3` | `walkP2.json` | opening screen held, card on top of it, exit 0 | 357 |
| R | `walk-f44fcc4dbfd6` | `walkR.json` | D23 reproduced; D24 reproduced twice; exit 0 | 484 |
| H | `walk-502bd65c9383` | `held-queue-walk.sh` | every check ok, exit 0 | 370 |
| K | `walk-5e1b87c7ef4c` | `walkK.json` | D24 with no earlier report; exit 0 | 256 |
| S | `walk-59339f983abe` | `walkS.json` | Tab order on the setup sheet, exit 0 | 191 |

- **Every run ended "clean: app quit, VM stopped, clone deleted, state removed."** and released its
  slot.
- **One more B was started and stopped before it booted.** It waited in `run-walk.py` for memory
  behind run D, and the lead asked that no walk wait on the lock. Its pid 34049 was named by the
  lead and checked to be my `run-walk.py`; it was stopped with `kill -TERM` and wrote no report.
- **Holding the opening screen.** It takes a Space inside its first three seconds, and the screen
  paints a few seconds after the window appears.
  - Run P's guest step therefore polled one 201x1 strip of the screen (`screencapture -R`) with the
    committed `qa/frame.py px`, pushed into the guest.
  - It sent Space only when the logo's white and the card's navy were both there: 28 and 30
    samples.

## Method and toolkit

The walks and tools are `richos/app/scripts/` on this branch: the candidate's source `d0d06501c`
plus this record. The `qa/` tools are unchanged from `4e142bc0e`.

**`testvm/`:**
- `run-walk.py --wait` with `TESTVM_AX_TIMEOUT=60`, `slots.py status`;
- `steps-walk.py --steps … --out …` and `--check` on every list;
- `held-queue-walk.sh`, as committed;
- `ax.sh` (find, click, type, `--focused`, `--key`, `--at`), `guest.sh`, `shot.sh`, `relaunch.py`
  through the `relaunch` step.

**`qa/`:**
- `contrast.py` on frame regions, and on color pairs read with `frame.py edges`;
- `frame.py` (`edges`, `crop`, `px`, and `px` inside the guest for the opening-screen trap);
- `ocr-gate.sh` with its positive control;
- `redact.py --homes` and `--box`;
- `wait-for.sh --file` and `--log`.

**Elsewhere:**
- `nightly-local.py candidate --run 20261010T032015Z-af885386`;
- `shasum`, `jq`, `git log`, `git show`, `git diff`;
- `sips` for the JPEG copies.

**Scripts written from scratch: none.**
- Eleven step lists were added under `steps/`. They are declared in `steps-walk-data.test.sh`'s
  inputs and covers rows, and that suite passes (3 ok lines).
- One inline Python snippet built `walkB2.json` out of `walkB.json`'s steps. It edits data and
  measures nothing.
- The opening-screen trap is a guest step inside `walkP.json` and `walkP2.json` that calls the
  committed `frame.py`.

### Frames

- **Format:** JPEG q75 copies of the 1680x1050 PNG originals. The `d-voice-row-*`, `d-memory-*`,
  `s-link-*`, `h-resumed` and `b2-panel-scrollbar` frames are crops.
- **Names:** the run letter, then what the frame shows.
- **The privacy gate.** `ocr-gate.sh` ran over the 35 PNGs the JPEGs were made from, and its
  positive control found the known address first.
  - It flagged 4 frames carrying the guest's own home path (`/Users/admin/testvm/…`, in the
    memory sheet and in Corrections).
  - Those were covered with `redact.py --homes`, and the two Corrections frames also with `--box`
    over the whole path paragraph.
  - The second gate: "0 of 35 frame(s) carry something that must not ship".
- **The names in the frames are invented for the walk:** Acme, Dana Whitfield, Northwind Traders,
  Contoso.

## Surfaces

**Applied and checked:**
- the Mac desktop app from its signed bundle, in light and dark:
  - first run (setup sheet, memory sheet, company sheet);
  - a clean quit and a fresh launch (the opening screen);
  - returning launches;
- every entrance to Bust a bug the brief names: Settings on a conversation, the home screen, the
  held opening screen, and with Corrections open;
- the Codex switch by pointer and by its ⓘ, with Codex absent, signed out and signed in;
- the talk button by its accessibility press;
- Tab and Shift+Tab on both first-run questions.

**Not applicable:**
- phones: nothing touched;
- battery: no `richos/mobile/**` change;
- the stable channel and publishing: nothing was published.

**Not covered:**
- VoiceOver itself (the accessibility tree was read instead);
- a Mac with keyboard navigation off;
- a real Codex install;
- a real GitHub issue (forbidden by the brief);
- Bust a bug from Feedback, Search or the company picker (run Q opened Settings over Feedback and
  Search but went no further);
- the "work connection would not open" hold;
- the update path while held.

**Test data:** each guest was a fresh clone with its own empty home. Nothing touched this Mac's app
data, and no app was started on this Mac.

## Cleanup

- **The runs:** all fourteen ended clean (table above).
- **After the last run:**
  - `slots.py status` read "guest.lock: free" and "guest-2.lock: free";
  - `~/.richos-testvm/tart/vms/` holds only `richos-base`;
  - `~/.richos-testvm/run/` is empty.
- **Scratch:** `/Volumes/E1TB/tmp/claude/ray-opus-walk48/` is deleted after this record is
  committed.

## Defects, in one list

- **D23 (blocks).** The card's privacy line says "Nothing private was in it, so nothing was left
  out" under a report carrying [a person], [a company], [a conversation] and [a file on this Mac].
  - Rich says two lines above that he left names, company details and file paths out.
  - The stand-ins are not marked, so none can be pointed at.
  - Seen 2 of 2 reports with private details, runs B and R.
- **D24 (blocks).** With Corrections open, the Settings button does not open its menu, so a bug
  cannot be reported from there.
  - Seen 3 of 3, runs B3, R and K. It opened once (run Q), when focus had stayed outside
    Corrections.
  - The button is also missing from the accessibility tree while Corrections is open.
- **D25 (polish).** A horizontal scroll bar under the Rich panel's messages.
- **D26 (small).** "2 assignments starting" under the held-queue notice that says nothing has
  started.
- **Seen, not new:** Tab on the company sheet walks into the rail behind it.

Verdict: NOT READY
