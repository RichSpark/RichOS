# Walk of nightly candidate 53 (`v1.2.0-nightly.20261013.53`), 2026-10-13

Candidate 53 was built from richos main `aad62a214` (run `20261012T230201Z-65d84f2c`). Candidate
51 was walked READY (`docs/verification/2026-10-12-nightly-51-candidate-walk.md`). Nothing from
that walk was walked again. This walk covers what a user sees that changed since 51: the portable
machine settings work (RichOS no longer assumes this Mac's external disk and keeps optional
storage settings in a private `.env` in its support folder) and D30.

| Item | Result |
|---|---|
| 1. A Mac that is not this one: first launch, the engine, one real task that needs scratch space, nothing naming the external disk | **PASS**. A clean guest under another account, with no external disk and no settings file, installed the engine from the published channel, and the team transcribed a voice memo correctly. Nothing the user saw, and nothing the app or the engine wrote, named `/Volumes/E1TB`. The shipped engine's docs still do in 4 lines (D32, not blocking) |
| 2. The starter settings file | **PASS** for the file: comments only, mode 0600, byte-identical to the committed example, and an existing file was left exactly as it was. **The app shows it to a user nowhere** (D31, not blocking) |
| 3. The two suites the build could not run without a screen, in the guest | **front-door: PASS**, 6 passed, 0 failed, against the candidate's own `RichOS.app.tar.gz`. **gui-boot: FAIL on B2 only** (1 failed, 34 passed, twice). Two routine boot lines have no rule. Carried since 2026-10-09, not from this nightly, nothing a user sees (D33) |
| 4. D30: a queued idea or bug card keeps its title on one line and its pill inside the card, light and dark | **PASS**. Both cards, both themes: the title is one line and the pill sits 38 px inside the card's edge |
| Readable text of everything walked, WCAG AA, light and dark | **PASS**. 12.06:1 to 18.07:1, computed (table below) |

## What changed since candidate 51, re-derived

`git log --first-parent 43abf72eb..aad62a214` lists 12 merges. These are the ones a user sees:

- `6e60a3b34`, `dc6d350c6`, `e2a7c50c2`, `09c2e8292`, `ac5b5df28`, `aad62a214`: portable machine
  settings, slices 1 to 3 and their nightly fixes (items 1 and 2).
  - Slice 2 (`dc6d350c6`) writes the starter after the first window
    (`src-tauri/src/main.rs`, `machine_settings::create_starter`).
  - It also forwards the settings to the engine's children (`engine_profile.rs`).
- `c2168ef6a`, D30 (item 4).
- **Not walked, outside this brief:** `9fa77273a` (a QA walk script), `112b90b18` and `db7181d38`
  (agent definition text), `ff1f48493` (a gate deadline), `1b6bcccd9` (the phone speed watch).

## Identity, checked before any claim: PASS

- **The zip.** `shasum -a 256` of `RichOS-1.2.0-nightly.20261013.53-macos-aarch64.zip` gives
  `83fd783bba4453d5bc9e379ca07a5047fca112cd68d127dcd3d4ebc91594603d`. That matches both of its
  `SHA256SUMS` lines (the versioned name and `RichOS-macos-aarch64.zip`).
- **The engine.** `richos-engine-1.2.0.tar.gz` gives
  `8814db6eb0fafb9366e6a2e471a71569498a4e3895b546b11b7e61647a488d00`, its `SHA256SUMS` line. It
  is also the digest in `engine-pin.env`, the engine the app downloads by itself (run A).
- **The release archive** front-door ran from. `RichOS.app.tar.gz` read in the guest gives
  `7e26fcc3d436ed16ce7ed1e59d25bb7eef11f97b186baf46c45d79cd5dc49d2f`, its `SHA256SUMS` line.
- **`candidate.json` (`.info`):**
  - `source_commit` `aad62a2142b7c41b037dbc59efac100aac0c7eaf`;
  - `run_id` `20261012T230201Z-65d84f2c`;
  - `version` `1.2.0-nightly.20261013.53`;
  - `build_commit` `07bb23fb9092e7d7183e06f24388db59b9c03de7`;
  - `checks_skipped` `[]`.
- **The build commit.** Its parent is `aad62a214`. `git diff --stat aad62a214 07bb23fb9` touches
  only `nightly-build.json`, `Cargo.lock`, `Cargo.toml` and `tauri.conf.json`.
- **The running app.**
  - The guest's `app.log` reads `this app: built from 07bb23fb9092e7d7183e06f24388db59b9c03de7`
    (run C).
  - The Settings update line reads "You are still running RichOS 1.2.0-nightly.20261013.53."
    (run B, `b-01-settings-menu`).
  - front-door's own line: "bundle 1.2.0-nightly.20261013.53, built from 07bb23fb9…" (run F).

## 1. A Mac that is not this one: PASS

**The Mac (run A, guest `walk-a4ee58631241`):**
- **The account** is the guest's `admin` (uid 501). The app's HOME is a fresh folder under it.
  The operator's account, his disk and his checkout do not exist there.
- **No external disk.** `/Volumes` holds only `Macintosh HD`, and `/Volumes/E1TB` does not exist.
- **No settings file and no selector.**
  - The fixture home was empty, so there was no `RichOS` support folder before launch.
  - The app's process environment carries none of `RICHOS_ENV_FILE`, `RICHOS_ENGINE_DIR` or
    `RICHOS_BULK_ROOT` (`ps eww` count 0).
  - Unlike every earlier walk, no engine was handed in (`--engine ""`). The app had to get its
    own, as a customer's does.
- **The launch** went through LaunchServices (`open -n -a`, `run.sh`), as a Finder launch does.

**What a new user sees, in order** (light theme):
1. **"There's a bit of setting up to do first."** It lists the RichOS engine and the voice and
   video tools, with **Set it up** and **Not now** (`a-01-setup-sheet`).
2. **Set it up** shows "Setting things up". Rich says about 1 GB of voice AI is downloading,
   and there is a progress bar ("416 MB of 1.06 GB" in an earlier run) (`a-02-setup-running`).
   - `app.log`: "first-run setup: this build installs engine 1.2.0".
3. **"You're all set."** Both rows say **Installed**, "Your Anthropic account is connected.",
   then **Start** (`a-02b-setup-done`).
4. **Start** opens the memory question; **Not now** opens the company question. Acme was
   added with `/Users/admin/Acme` as its folder, and the first-run interview was declined.

**One real task that needs scratch space.**
- **The memo.** The guest wrote a 6-second voice memo into `~/Acme/memo.aiff` with
  `say -o`. That writes a file and plays nothing (no audio test, §53 does not apply).
- **The request**, typed to Rich: "Please transcribe the voice memo at …/Acme/memo.aiff and tell
  me what it says."
- **The engine took the job.**
  - Rich answered "On it!".
  - The work line read "1 waiting for you · 1 assignment running".
  - Under the hood: "Waiting on you: running a command on your Mac." with **Approve this**
    (`a-05b-work-summary`).
- **Approve** was pressed twice: one command each time.
- **The engine's answer** came back in the conversation (`a-06-answer`, `a-07-answer-dark`):
  > "Note for Friday: order two bags of coffee beans, and call the printer about the brochure
  > proof before noon."
  - Those are the memo's words exactly.
  - "I transcribed it here on your Mac, and the recording didn't go to any outside service."
  - The work summary says "Answered: it's in your conversation."
- **So the engine converted the audio and transcribed it in a working folder, on a Mac where the
  external disk does not exist.** Which folder it used is not recorded by this walk: the guest
  was deleted before the job's own transcript was read back.

**Where the engine puts things on this Mac.** The installed engine's own reader, run in the
guest with the app's HOME and TMPDIR (`machine_settings.py show`), gives:
- `settings {}`;
- agents in `<TMPDIR>/richos-agents`;
- cache in `~/Library/Caches/RichOS`;
- state in `~/.richos-nightly`.

`machine_settings.py scratch walk53-probe` created
`<TMPDIR>/richos-agents/walk53-probe`. All four are portable defaults, none on an external disk.

**Nothing names `/Volumes/E1TB`, where a user or the running app can see it:**
- **The screens.** `ocr-find.sh` on every frame of runs A (two of them), B and C: 0 of 25 frames
  show "E1TB", and 0 show "Volumes".
- **What the app and the engine wrote.** `grep -rIl E1TB` over the app's whole HOME found nothing
  in any of these:
  - `com.richos.app` (conversation, ledger, engine state, assignment evidence);
  - `.claude`;
  - the starter file;
  - `app.log`;
  - the company folder.
- **What the shipped engine carries:** see D32. The engine's files do name it. 9 of those
  are comments, test inputs or measurements, each marked exempt on its line. 4 lines of `docs/`
  describe current behavior with the old disk path.

## 2. The starter settings file: PASS for the file; D31 for how a user finds it

**Created on first launch, as specified** (runs A, A3, A4, each on a clean home):
- `~/Library/Application Support/RichOS/.env`, created about one second after the window
  appeared (born 01:42:26Z, launch 01:42:25Z in run A).
- **Mode 0600** (`-rw-------`), owner `admin`, 1170 bytes.
- **Comments only:** every line starts with `#` (`grep -vc '^#'` gives 0).
- **sha256** `9d8e30fcd5b7f38e269966488b8189e0453777468c787946e67709cb73831b66`. That is
  byte-identical to the committed `richos/engine/.env.example` at `aad62a214`.
- **Unchanged by use.** The same hash and mode were read again at the end of run A, after setup,
  the engine install and the team's task.

**An existing file is never overwritten** (run B, guest `walk-58af67896b8e`). This file was put
in the fixture home before the app was ever opened:

```
# My own RichOS settings, written before RichOS was first opened on this Mac.
# RichOS must leave this file exactly as it is.
CARGO_TARGET_DIR=/Users/admin/my builds/cargo
```

| When | Mode | Modified | sha256 |
|---|---|---|---|
| On this Mac, before the walk | `-rw-r--r--` | 2026-10-01 09:00 local | `2a2925ae…3664564` |
| In the guest, right after launch | `-rw-r--r--` | 2026-10-01 08:00 guest clock (the same instant) | `2a2925ae…3664564` |
| At the end of the walk (first run, a company, a model turn, a feature idea) | `-rw-r--r--` | unchanged | `2a2925ae…3664564` |

- The app did not replace the file, add to it, or change its mode or time.
- The support folder held only `.env` and `tools`.
- The file has a real setting, with a space in the path. The app and its engine worked with it:
  the first run, a company, a model turn and an engine assignment.

**The way the app shows it to a user: there is none (D31, below).** Looked for live, in run B:
- **The Settings menu** (`b-01-settings-menu`) shows:
  - Token spend, Claude accounts, Dictation;
  - Connected folders (repositories), Memory folder, Use Rich from your phone;
  - Bust a bug!, Request a feature;
  - Theme, Text size, Updates and Technical view.

  None of them is about storage or this file.
- **Preferences** (`b-01b-preferences`) shows:
  - Rich's model;
  - how much Rich interrupts;
  - Technical view;
  - Keep the stored output;
  - Splash screen;
  - Your name.

  Nothing about storage.
- **Rich, asked in the conversation:** "Where can I tell RichOS which folder or drive to use for
  its temporary files and caches?" (`b-02-rich-answer`).
  - He answered "I'll check." and started an investigation.
  - It came back as: "I don't have an answer for you yet. To find out whether Rich can choose a
    folder or drive for its temporary files and caches … through RichOS's own files on your
    Mac. That request is waiting for your approval. … can't say whether that setting exists or
    where it would be."
  - Rich does not know the file exists.

## 3. The suites the build could not run without a screen

### front-door, against the candidate's `RichOS.app.tar.gz`, in a guest: PASS

**Why not through `run-tests.sh`.**
- Under `RICHOS_GUI_HOST`, `run-tests.sh` records front-door NOT RUN. It declares
  `# run-tests: host-only`.
- `testvm/run-suite.sh` refuses it for that reason.

**How it was run instead** (run F, guest `walk-3bf65f55e4c3`, `run-walk.py --no-app`):
- The scripts went into the guest, with `cliclick` 5.1 (system frameworks only) and the
  candidate's archive.
- `front-door.test.sh --release <that folder>` ran in the guest's GUI session
  (`launchctl asuser`, `env -i`, HOME the guest user's).

**Result: exit 0, "6 passed, 0 failed".**
- C0: one window (pid 1149).
- C1: a native keystroke reaches the web content.
- C2: Cmd-K reaches main.js's document listener.
- C3: Escape closes the search overlay.
- C4: Escape closes the settings menu ("Technical view 114x20 -> absent").
- Z: pid 1149 is gone.

**Two cases did not measure, said by the suite itself, not counted as passes:**
- **C5:** "this build's tree does not carry the quit dialog at rest; the closed-sheet box could
  not be calibrated".
- **C5b:** "the door was not on screen before the key, so Return was never put to it".
- **Why the door was never on screen:** the first-run questions of a fresh home stayed up.
  The suite's log says "a first-run question did not change under Escape; trying its own
  button", many times. They are a queue the suite reports and does not fail on.

### gui-boot, through `run-tests.sh` with `RICHOS_GUI_HOST`: FAIL on B2 (D33)

**How it was run.**
- `RICHOS_GUI_HOST=richos-test-1 run-tests.sh --only gui-boot.test.sh`, from this worktree at
  `aad62a214`, the candidate's source.
- It used the nightly's own runtime and speech models, the way the nightly runs it when it has a
  guest.
- The executables were built on this Mac at that commit. The suite ran in guests
  `walk-90d7d61e619f` and `walk-b27c0f1e2186`.

**The result.**
- `run-tests.sh` retried it once, alone (verification-retries).
- Both runs: "=== gui-boot.test.sh: 1 FAILED, 34 passed ===", and in both B2 failed.
- **B2's own account** of the healthy boot:

  ```
  UNACCOUNTED  [richos] window: launch kind fresh, splash on
  UNACCOUNTED  [richos] second review: the app's review-watch runs as pid 16425
  ```

- Every other line of the healthy boot is accounted for.
- The six negative halves (B3 to B9a) all passed, so each detector is alive.

## 4. D30, the queued card's head: PASS

The checks, the same for both cards:
- **The card.** It is queued with no reporting token in the guest (keychain lookup exit 44).
- **GitHub blocked.** `api.github.com` and `github.com` point at the guest itself. A `curl` was
  refused before the walk, right before the press, and at the end.
- **The press.** It left the card waiting. The outbox file says `"reason":"not-set-up"`.
- **Nothing was posted or sent anywhere.**

| Card | Theme | Title | Pill text | Pill outline's right end / card edge |
|---|---|---|---|---|
| Feature idea, "Waiting to post · saved on this Mac" (run B) | dark | **one line**, 96 x 20 | 262 x 20, on its own row under the title | x 1449 / x 1488: **38 px inside** |
| | light | **one line**, 96 x 20 | 262 x 20 | x 1449 / x 1488: **38 px inside** |
| Bug report, "Waiting to send · saved on this Mac" (run C) | light | **one line**, 82 x 20 | 266 x 20, on its own row | x 1449 / x 1488: **38 px inside** |
| | dark | **one line**, 82 x 20 | 266 x 20 | x 1449 / x 1488: **38 px inside** |

- Sizes are the accessibility tree's. Edges are `frame.py edges --row` across the pill.
- **On 51** the idea title was 59 x 40 (two lines), and the pill ran 2 to 4 px past the card's
  edge.
- Frames: `b-03-idea-card-dark`, `b-04-idea-card-light`, `c2-02-bug-head-light-b` and
  `c2-03-bug-head-dark`.

## Contrast (`qa/contrast.py`), the elements this nightly changed

| What | Light | Dark |
|---|---|---|
| Idea card: "Feature idea" | 18.07:1 | 12.06:1 |
| Idea card: "Waiting to post · saved on this Mac" | 18.07:1 | 12.06:1 |
| Bug card: "Bug report" | 18.07:1 | 12.06:1 |
| Bug card: "Waiting to send · saved on this Mac" | 18.07:1 | 12.06:1 |

- Items 1 to 3 add no new on-screen text. The setup sheet and the first-run questions are
  unchanged since earlier nightlies.
- **Not measured, declared:** the pill's dashed outline. It is not the indicator; the words inside
  it say the state.

## Findings

### D31. The settings file has no way in for a user (not blocking)

**What a user sees:**
- No screen, menu or Preferences row mentions storage settings or the file.
- Asked where to set it, Rich says he does not know and starts an investigation that needs an
  approval.
- The file is a dot-file inside `~/Library`, both hidden in Finder.

**Against the PRD** (`richos-hq/docs/prds/2026-10-12-portable-machine-settings.md`, "One file
and two readers"): "Explain how to open the hidden file in the example header."
- The first header (`1ed91df43`) had the line "Open ~/Library/Application Support/RichOS/.env
  with your editor or Finder Go to Folder."
- `b43e608d3` replaced the header and dropped it. The shipped starter says what to put in the
  file, but not how to reach it.

**Why not blocking:**
- The PRD says customers need no setup screen and nothing in the app needs the file.
- Portable defaults worked without it (item 1).
- The person who uses it today knows where it is.

**Suggested fix:** put the one "how to open it" line back in `.env.example`. Teach Rich where
the file is, so the question above gets an answer.

### D32. The shipped engine's docs still name the external disk (not blocking)

The engine the app installs carries 13 files that contain "E1TB" (`grep -rIl`, run A).
- **9 are comments, test inputs or measurements**, marked on their lines
  (`machine-path-exempt: …`). For example:
  - `orchestration.config` 1197, 1597, 1598, 1706 and 1728;
  - `scripts/lib/foreign_app_data.py:252`;
  - `scripts/disk-watchdog.sh:262`;
  - `scripts/lib/scratch-reaper.py:2816`;
  - `scripts/lib/build_caches.py:3`;
  - the `*.test.sh` files.
- **4 lines of `engine/docs/` describe current behavior with the old path.** The code they
  describe now resolves its folder through `machine_settings` (`cpu_guard.py:42`, `:87`):
  - `docs/host-cpu-enforcement.md:7`: "copies its runtime to
    `/Volumes/E1TB/state/richos/cpu-guard/runtime`";
  - `docs/host-cpu-enforcement.md:72`: "Its manifest is under
    `/Volumes/E1TB/caches/richos-ios-prepared`";
  - `docs/recovery-supervision.md:30`: "remain under `/Volumes/E1TB/state/richos/recovery-wip/`";
  - `docs/recovery-supervision.md:42`: "indexed under
    `/Volumes/E1TB/state/richos/recovery-evidence/`".

**Why not blocking:**
- No screen shows them, and nothing the app or engine ran reads them.
- A teammate reading those docs would be told the wrong folder. The PRD covers "current
  guidance".

**Suggested fix:** name the setting, for example "the `state` folder `machine_settings.py`
resolves", instead of the path.

### D33. gui-boot's B2 has no rule for two routine boot lines (a suite defect, not a product one)

**What a user sees:** nothing. Both lines are the operator log's routine facts.

**Where it comes from:**
- `[richos] window: launch kind …` came in `ef9fbbe81` (2026-10-08).
- `[richos] second review: the app's review-watch runs as pid N` came in `d74d92732`
  (2026-10-09).
- `gui-boot.test.sh` was last changed in `941988288` (2026-10-08) and has no rule for either.

**Why nobody saw it:** every nightly since has recorded gui-boot NOT RUN ("no screen this run
may use"). So it has been red, unseen, since 2026-10-09. The cause is not in this nightly.

**Raised as `esc-20261013T003340Z-e4910ff2`** (record in `docs/verification/escalations/`). During
this walk the guest was used by `echo-sonnet-b2lines1`, whose name says it is the fix.

**Why it does not block READY:**
- The only failure is those two lines.
- Every other line of the healthy boot is accounted for, and all six negative cases pass.
- READY here does **not** claim gui-boot passed.

### Other notes

- **Approving blind.** "Approve this" for the team's command does not say which command it will
  run ("Waiting on you: running a command on your Mac."). The memo task needed two approvals.
  Not new in this nightly.
- **A slow non-answer.** Rich's "I'll check." for the storage question stayed an investigation
  for over 5 minutes, waiting on an approval (D31's second half).
- **First-run questions and Escape.** front-door notes that the first-run questions do not
  close under Escape. It treats them as a queue.

## Verification mode

- **Environment:** the test VM, one fresh guest per run through `run-walk.py`, never this Mac's
  screen.
  - One run at a time: memory allowed one guest while other agents ran.
  - One app window per guest. No two-user test.
  - Guest screen 1680x1050 at 1x; app window 1400x864.
  - Light and dark through the guest's macOS appearance.
- **Artifacts:**
  - the candidate's zip;
  - for runs B and C, the same folder's engine tarball;
  - for run A, no engine (the app downloaded it);
  - for run F, the candidate's `RichOS.app.tar.gz`.
- **Account:** the real Claude login that `run.sh` copies into each guest. No account file on this
  Mac was written.
- **GitHub:** no reporting token, and `api.github.com` and `github.com` unreachable in runs B and
  C. No post or issue was made.
- **Audio:** none played. The memo was written to a file with `say -o` in the guest.
- **Verification basis:**
  - Every PASS above was seen on the running candidate in a guest: its accessibility tree, its
    frames, the guest's files and `app.log`, and the walks' step records.
  - Lines marked "by the code" or naming source lines are read from source.

### Runs

| Run | VM | Walk | Result | Slot held |
|---|---|---|---|---|
| A0 | `walk-a6b688b72162` | first screen only, exploratory | the setup sheet on a clean home. Exit 0 | 80 s |
| A1 | `walk-684fec6f68db` | `steps/walkA.json` (first form) | my error: waited for text "installed" the sheet never shows. The setup itself finished. The walk stopped at the company step | 1381 s |
| A2 | `walk-92fe71265fec` | `steps/walkA.json` (second form) | my error: pressed Start while it was still hidden | 82 s |
| A3 | `walk-01648c1052cc` | `steps/walkA.json` (third form) | first run to the end. The team's task stopped at "waiting on a decision" because the list had no Approve step. Exit 0 | 1000 s |
| A | `walk-a4ee58631241` | `steps/walkA.json` | item 1 and the starter, start to finish, light then dark. Exit 0 | 844 s |
| B | `walk-58af67896b8e` | `steps/walkB.json` | the existing file, Settings, Preferences, the question to Rich, the idea card dark then light. Exit 0 | 675 s |
| C1 | `walk-34fda2229c5b` | first form of `steps/walkC.json` | the bug card waits. My scroll put the head under the panel's top bar. Exit 0 | 247 s |
| C | `walk-a0e0cd9ce391` | `steps/walkC.json` | the bug card's head, light then dark. Exit 0 | 282 s |
| F | `walk-3bf65f55e4c3` | `steps/walkF.json` | front-door in the guest. Exit 0 | 278 s |
| gui-boot | `walk-90d7d61e619f`, `walk-b27c0f1e2186` | `run-tests.sh --only gui-boot.test.sh` | 1 FAILED, 34 passed, twice | 302 s, about 300 s |

**Cleanup.**
- Every run ended with run-walk.py's "clean: app quit, VM stopped, clone deleted, state removed."
  and released its slot.
- front-door's own Z case: "pid 1149 is gone"; after it, `pgrep` in the guest found no
  `richos-tauri`.
- A1's first attempt waited for a slot and was ended by me (pid 48045, its own run-walk process)
  before any guest booted, at the coordinator's request. It had booted nothing.
- `slots.py status` afterwards: `guest.lock: free`, `guest-2.lock: free`.
- No app instance was started on this Mac.

## Method and toolkit

The `qa/` and `testvm/` tools are those of `aad62a214` (this branch's base, which contains the
toolkit commit `b43e608d3`). None was changed.

**`testvm/`:**
- `run-walk.py --wait 0`, with `--no-app` for front-door;
- `steps-walk.py --steps … --out …`, and `--check` on every list;
- through it, `ax.sh`, `guest.sh` (including `--push`), `shot.sh` and `pointer-drag.sh`;
- `slots.py status`;
- `run-suite.sh`, `suite-walk.sh` and `suite-in-guest.sh`, through `run-tests.sh`, for gui-boot;
- `reserve.py --wait 600` around it.

**`qa/`:**
- `contrast.py` on frame regions;
- `frame.py edges`;
- `ocr-find.sh` for "E1TB" and "Volumes" across 25 frames;
- `ocr-gate.sh`, with its positive control, on the 13 committed frames and the step lists. It
  flagged the guest's own `/Users/admin/…` path in 3 frames; `redact.py --homes` covered them, and
  the gate then read "0 of 13 frame(s) carry something that must not ship";
- `wait-for.sh --log` and `--time` for each run's end.

**Elsewhere:**
- `nightly-local.py candidate`, `shasum`, `jq`;
- `git log`, `git diff`, `git show`;
- `sips` for the JPEG copies;
- `escalate.sh raise`.

**Scripts written from scratch: none.**
- The step lists are data for `steps-walk.py`, kept in `steps/`. They are added to the `inputs` and
  `covers` rows of `richos/app/scripts/steps-walk-data.test.sh`, which passes: "ok covers row names
  every committed step list", "ok every committed step list is valid", "ok malformed step lists
  fail the check".
- `walkF.json` names the candidate folder as `CANDIDATE_DIR`. The run used the folder
  `nightly-local.py candidate` prints.
- `steps.json` outputs were read with inline `python3 -c` one-liners.
- One inline edit added `allow_fail` to two steps of a list.

Verdict: READY
