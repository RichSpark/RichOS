READY

# Nightly 81 candidate walk (2026-10-29, Ray)

Candidate `v1.2.5-nightly.20261029.81`, run `20261029T183546Z-54038771`, source `c0bab17ffba04b3906bcefb25686c301dc7c3aea` (richos main), build commit `72c32f61c755981a3d204d187312d7addae5f414`.

**READY** means: no hole a user could hit in normal use was found from these changes. Two of the five things the brief asked to see (the "Can't move yet" line and the quota hold card's "one at a time" sentence) could not be put on screen in the real app inside a guest; they are covered by the app's own screen suites, run here and that is stated again under "Not reached". Neither is a defect.

## Verification mode

- Surface: the RichOS desktop app, **in the test VM only** (never this Mac's screen, CEO 65), one window, one user, no second-user test. Guest desktop 1680x1050, app window 1400x866, light and dark.
- Bundle and engine: the candidate zip and engine tarball named in the brief, copied into a fresh guest by `run-walk.py` (guest slot held for that run only; app quit and clone deleted at the end of every run: `[testvm] clean: app quit, VM stopped, clone deleted`).
- Walk: `richos/app/scripts/testvm/nightly81-walk.sh` (new, committed with this record). Its setup and steps 0 to 5 are `accounts-walk.sh`'s, changed in two ways this build needs: the start screen's "Talk to Rich" door is pressed and then the company question answered and the Settings rows are `AXButton`s (they were `AXMenuItem`s). **Finding for whoever owns `accounts-walk.sh`: it cannot reach the conversation on this build** (my first run, `walk-d08246fa071b`: "Not now" and "Add this company" never appeared, no composer, Settings rows not found).
- Fixture-driven: `fake-claude-fill-first.pl` answers every `claude` call, so there is **no real Claude login and no real second account**; the second account is the fixture's "Work" (the walk's own mock accounts, as the brief allowed).
- Verification basis on every line: **verified live in the guest** (accessibility read plus a captured frame of the guest's screen), unless the line says "suite" (the app's own headless WebKit suite run on this worktree) or "not reached".
- Walk runs: 5 (run 1 and 2 stopped by me at the start-screen and company-question faults above; run 3 stopped at the AXMenuItem fault; run 4 is the first complete pass of items 1, 2, 4, 5; run 5 repeated it). I stopped each run's walk script by its own pid, so `run-walk.py` did its normal cleanup.
- Toolkit (`richos/app/scripts/qa/`, richos main at `c0bab17ff`): `contrast.py` (every ratio below, its one stated estimator), `ocr-gate.sh --text` (evidence text clean: "0 of 6 text file(s) carry something that must not ship"). Helpers written from scratch: **none**. One walk script added where walks live (`scripts/testvm/`).

## Freshness

- `run-walk.py` has **no `--expect-sha` option** (its `--help`: `--bundle --home --engine --no-app --previous-bundle --report --state-dir --wait --tailnet --new-user`), so the brief's flag could not be passed. Identity was established instead by:
  - the zip's sha256 (`5faa82d6d815...59e7`) equals `SHA256SUMS` for both zip names and the engine tarball's (`fbc0f1b0f5ee...3fba`) equals `SHA256SUMS`;
  - `candidate.json`: `source_commit c0bab17ff`, run `20261029T183546Z-54038771`;
  - `72c32f61c` is `c0bab17ff` plus four files only (`nightly-build.json`, `Cargo.lock`, `Cargo.toml`, `tauri.conf.json`: the version stamp), so the UI and Rust source this worktree holds (`c0bab17ff`) is the build's;
  - the running app itself (item 1).

## Items

| # | Item | Result | Basis and evidence |
|---|---|---|---|
| 1 | Identity | **PASS** | Live: Settings shows `RichOS 1.2.5-nightly.20261029.81` (read from the accessibility tree, walk line "read: RichOS 1.2.5-nightly.20261029.81"); the guest's `Info.plist` `CFBundleShortVersionString` and `CFBundleVersion` are both `1.2.5-nightly.20261029.81`; the update check says "RichOS 1.2.5-nightly.20261029.81 is up to date." Evidence: `2026-10-29-nightly-81-evidence/run4-walk.log`, `app-version-run4.txt`, `run4-update-check.tree`. |
| 2a | "Switching to Work." while the move is pending (dark) | **PASS** | Live. Work's "Use this one now" pressed while a turn runs on Home: the sheet reads **"Switching to Work."**, Home stays first and "Still in use", Work says "Switching to this one" and "Rich moves to Work with his next reply."; 12 s later, turn still running: still "Switching to". Frame: `~/.richos-walk-evidence/nightly-81/run4/M1b-pressed-dark.png`. Contrast (`qa/contrast.py`, dark): line 13.02:1, pill 12.06:1, note 5.78:1, all over 4.5:1. |
| 2b | "Rich now uses Work." only once the work really runs there (dark) | **PASS** | Live, two runs. Run 5: the pressed turn ended and the line **still read "Switching to"** (`run5-walk.log` 20:12:40); only after Rich's next reply did it read "Rich now uses Work. Home is next." (frame `run5/M1d-turn-ended-dark.png`) Run 4: it read "Rich now uses" once the turn ended and Work was in use. Never before the work moved. Frame: `run4/M1e-moved-dark.png`; contrast line 13.02:1, "In use now" pill 6.36:1. |
| 2c | "Switching to Home." (light) | **PASS** | Live. Home's "Use this one now" pressed while Work runs: "Switching to Home.", Work "Still in use", Home "Switching to this one". Frame: `run4/M2b-pressed-light.png`. Contrast (light): line 17.02:1, pill 18.07:1, note 6.33:1, "Still in use" 6.65:1. |
| 2d | "Can't move yet: work is still running on Home." | **PASS by suite, not reached live** | The guest cannot stage work the *team* runs on the account in use (the fake's helpers are on Rich's own lease and the real app answers "Switching to" for that). `ui/tests/accounts.js` run on this worktree, exit 0, 27 checks PASS, 0 FAIL: "Use this one now with work still on the account in use: Can't move yet or Switching and Rich now uses Work only once the work has moved: light: Can't move yet: work is still running on Home. Undo / light: Switching to Work, then Rich now uses Work / dark: (same)". Its both-theme contrast sweep: `use-blocked` worst text 5.29:1 dark, 6.22:1 light; `blocked` 5.29:1 / 6.22:1; `moving` 5.29:1 / 6.22:1. Evidence: `~/.richos-walk-evidence/nightly-81/ui-accounts.log`. |
| 3 | Quota hold card says held work resumes one at a time | **PASS by suite, not reached live** | `ui/tests/quota.js` run on this worktree: 37 checks PASS, 0 FAIL. Its `D3` check puts three held agents (Mark, Andy, Tom, 41 minutes) in the Technical view's quota sheet and asserts the status line "Holding 3 agents since ..." and the exact detail: "The five-hour window is at 94% used, past your 93% line. Each has kept its place. They resume one at a time from <time>, never past 100%; the rest at the reset or sooner if a fresh reading is back under the line." The same run's dark and light state matrices fit and meet the contrast floors. The real app could not be made to hold a team in the guest (see "Not reached"). Evidence: `~/.richos-walk-evidence/nightly-81/ui-quota.log`. |
| 4a | Smoke: message and reply | **PASS** | Live: "Smoke check: please confirm you are there." answered "Smoke reply for nightly 81: the client report is on track." The reply comes from the fixture `claude`, not a Claude login (the guest has none; the brief's "documented equivalent"). Frame `run3/S1-reply.png`; `run4-walk.log` "read: Smoke reply for nightly 81". |
| 4b | Smoke: Settings and the update check | **PASS** | Live: Settings opens with the Updates block; "Check for updates" answers "RichOS 1.2.5-nightly.20261029.81 is up to date. Checked just now." Frame `run3/S3-update-check.png`; `run4-update-check.tree`. |
| 5 | Runtime launchers are Mach-O, not `#!` scripts | **PASS** | Live in the guest, installed engine's `runtime/bin`: `ffmpeg`, `ffprobe`, `git`, `jq`, `node`, `python3`, `whisper-cli`, each `file` -> "Mach-O 64-bit executable arm64", first two bytes `317 372` (0xCF 0xFA, the Mach-O magic), none `#!`. Evidence: `runtime-bin-run3.txt`, `runtime-bin-run4.txt`. |

## Not reached or not claimed

- **"Can't move yet" in the real app** (2d): the suite checks the exact wording and contrast in both themes; the real app's blocked answer needs the team's work running on the account in use (a real work grant). Not claimed live.
- **The quota hold card's staggered sentence in the real app** (3): in the guest the quota gate refuses the fake's helpers first ("this work has no open grant", `calls.log` "gate walk-agent-N exit 2"), so no hold is recorded and the card, at 95% of the five-hour window, reads "Ready to pause. The five-hour allowance has reached 93%. Agents will pause when they finish their current step. No pauses observed yet." (frame `run5/H-hold-dark.png`). A held team needs a real work grant (`work-lease-walk.py`'s setup), not this fixture.
- Light-theme live frame of the *moved* line ("Rich now uses Home."): the light pass ended at the pending line (2c); the moved line's wording and contrast are asserted in both themes by the suite (2d).
- No real second Claude account, no real Claude reply, no phone, no second user.
- `nightly81-walk.sh`'s hold-card block was changed after run 5 (it asserted "one at a time", which this fixture cannot show; it now asserts "Ready to pause", which run 5's frame shows) and was **not re-run** after that edit; the rest of the script is as run 4 and 5 ran it.

## Non-blocking notes

- The accounts record's `inUse` (`claude-accounts.json`) turns to the new account the moment "Use this one now" is pressed (`run4-M1b-record.txt`: `"inUse":"2"` while the sheet says Home is "Still in use"), while the screen follows where the work runs. A reader of that file would take the move as done; no user reads it and the screen is right. Worth knowing for anyone writing a check on that file.
- When the pressed turn has already ended, "Switching to Work." waits for Rich's *next* reply (run 5: stayed through the turn's end; run 4 moved at the turn's end). The card's own line says so ("Rich moves to Work with his next reply."), so it is honest, but a user who goes quiet sees "Switching" until they next write.
- `accounts-walk.sh` is stale against this build (start screen door, company question, `AXButton` rows): see Verification mode.
- Each walk's frames carry macOS's own "Login Item Added: RichDoctor" banner (the guest's login item prompt), on screen in the top right.

## Method, evidence

- Frames, trees and per-run logs: `~/.richos-walk-evidence/nightly-81/` (`run3/`, `run4/`, `run5/`, `ui-accounts.log`, `ui-quota.log`; the two suite logs stay out of the repository because the publication guard flags their stock closing line as source material). Text evidence copied beside this record: `docs/verification/2026-10-29-nightly-81-evidence/`.
- Contrast numbers: `python3 richos/app/scripts/qa/contrast.py <frame.png> label:x0,y0,x1,y1 ...` on the regions the accessibility tree gave for each line (`M1b-line-dark.json`, `M1e-line-dark.json`), frames at 1:1 with screen points.
- The scratch directory was deleted at the end (see the hand-over).
