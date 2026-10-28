# Walk of nightly candidate 78 (`v1.2.5-nightly.20261028.78`), 2026-10-28

Candidate 78 was built from richos main `19454cbb94591c48d8aff83d7c21ef6ae7a3a3a8` (run
`20261028T111233Z-41017cf2`). The boot proof the build ran is not mine and was not rerun:
`result=fail:screen-suites`, `windows=1`, front-door 6/6, gui-boot 35/36. The one failing gui-boot
check is B2, a missing boot-line rule; Echo retakes that proof separately.

| Item | Result |
|---|---|
| Verdict | **IN PROGRESS** (this first commit holds identity and item 1 only) |
| Identity | **PASS** (below) |
| 1. Codex watcher (`ef7a26057`) | **PASS**, carried from my earlier run (deleted before it could commit) |
| 2. Write checks in every app session (`20ce71c36`) | pending |
| 3. Updater, changelog link, 1.2.5 bump (`8a2689030`, `9e64a48fc`) | pending |
| Contrast | pending |

## Identity, checked before any claim: PASS

Found by my earlier run on this candidate (worktree `ray-sonnet-3086556084ea`, deleted at 12:35:24Z
by another session's Stop hook before it committed). Not rerun; the full values below were re-read
from the staged files in this run (`~/.richos-nightly/releases/v1.2.5-nightly.20261028.78/`).

- **The zip.** `shasum -a 256` of `RichOS-1.2.5-nightly.20261028.78-macos-aarch64.zip` gives
  `cb5d0046ec328c3f0ccc6a3cda1a1862efd87c8fdd00866adfc7d42538225db9`. It matches both of its
  `SHA256SUMS` lines (the versioned name and `RichOS-macos-aarch64.zip`).
- **The engine.** `richos-engine-1.2.0.tar.gz` gives
  `0b3552a3923ff8ed670e859eaa5140e72d0af098fc0522ce1c0be148b1363c3e`. That is its `SHA256SUMS` line
  and `RICHOS_ENGINE_SHA256` in `engine-pin.env`.
- **`candidate.json` (`.info`):**
  - `source_commit` `19454cbb94591c48d8aff83d7c21ef6ae7a3a3a8`;
  - `run_id` `20261028T111233Z-41017cf2`;
  - `version` `1.2.5-nightly.20261028.78`;
  - `build_commit` `f1de705bab2ea4d366c0dff996f635bba9599280`;
  - `checks_skipped` `[]`.
- **The build commit.** Its parent is `19454cbb9`; its diff touches only `nightly-build.json`,
  `Cargo.lock`, `Cargo.toml` and `tauri.conf.json`.
- **The running app.** Every guest's `app.log` reads `this app: built from
  f1de705bab2ea4d366c0dff996f635bba9599280` (item 1's report `identity` step and the later runs
  below).

## What changed and what has nothing a user can see

Merges with nothing user-visible, per the brief: devtools (`19454cbb9`), tquota2 (`58daee867`),
example1 (`47ed2e439`), the newt notes and test-only edits. User-visible: the Codex watcher (item
1), the write checks (item 2), the updater and changelog link plus the 1.2.5 version bump (item 3).

## 1. The Codex watcher (`ef7a26057`): PASS

Found by my earlier run, via `codex-watch-walk.py` in guest `walk-6f77dcb76a39` (exit 0,
`cleanup_complete` true). Evidence: `2026-10-28-nightly-78-candidate-walk/item1-codex-watcher.md`.

- `report.json`: six steps, all PASS: identity (built from `f1de705…`), first-run, channel,
  relaunch, append, answer.
- **Both entries reached Rich's inbox exactly once.** The first, mark `codexwalk-first-d3ac01dafd`,
  arrived as a `CODEX-WATCH 12:30Z` row naming `rich-codex/to-rich.md` lines 1-4. The second, mark
  `codexwalk-second-9d79b95a74`, arrived after the relaunch (turn
  `turn_041bb12c5acd476e9eb4fce5525dc882`, source `agent`).
- **No replay after relaunch.** The relaunch (recorded pid 1834, one window, composer back after
  24.1 s) did not deliver the first entry a second time.
- **Rich replied on screen**, 194 characters: "Codex left a second note … asks for nothing, so
  there's nothing for you to do".
- **The acknowledgment line was written to `to-codex.md`:** `walk-ack
  codexwalk-second-9d79b95a74` (36 bytes).
- The relaunch logged the Codex watch line with 2 channels
  (`[richos] Codex watch: watching 2 channel(s)`).

**Gaps, non-blocking:** no light and dark frames (the walk reads the ledger, not the screen); the
`CODEX_CHANNELS` setting's entrance was not walked.

## 3. Updater, changelog link and the 1.2.5 bump (`8a2689030`, `9e64a48fc`): light half PASS, dark half not yet walked

Run `walk-78187db50c0a` (guest, steps-walk.py with `walkE.json`, exit at step 37 on a plan error of
mine, see below). Frames and step records: `/Volumes/E1TB/tmp/claude/ray/e2-out/` (`steps.json`,
`e-00-home.png`, `e-01-settings-light.png`, `e-02-after-check.png`, `e-03-browser.png`).

- **Identity in the running app:** `app.log` reads `this app: built from
  f1de705bab2ea4d366c0dff996f635bba9599280` (step 3).
- **Settings version line (1.2.5 bump): PASS, light.** The Updates row reads "RichOS
  1.2.5-nightly.20261028.78 is up to date." (accessibility tree, step 17; `e-02-after-check.png`).
- **Check for updates: PASS, light.** Pressed in the guest (step 21); after 20 s the row reads
  "Checked just now." and the same up-to-date line, no error.
- **See what's new: PASS.** The link ("See what's new (opens in your web browser)") was pressed
  (step 25). Safari, the guest's default browser, then opened
  `https://github.com/RichSpark/RichOS/blob/main/docs/CHANGELOG.md` (step 28, read back with
  osascript) and the page loaded: "Changelog", 1.2.4 section (`e-03-browser.png`). It opened in the
  browser, not inside the app window. The new address is the one in the code (`updates.js`
  `CHANGELOG_KEY`, `opener.rs` allowlist); the old address stays allowlisted (source).
- **My plan error, not the app's:** after Safari the Settings menu was still open and my next step
  pressed the Settings button again, so the dark-theme toggle was not on screen
  (`ax: notfound`, step 38). A corrected plan, `walkF.json` (click Dark theme directly), was queued
  but had not started when this handoff was forced.
- **Not walked yet:** Settings version line and Check for updates in the dark theme.

## Contrast so far (`qa/contrast.py`, `e-02-after-check.png`, light, 100% text size)

| What | Size (source, `style.css`) | Light | Dark |
|---|---|---|---|
| "Updates" heading | 16px (not read from CSS, the row's title) | 18.07:1 | not yet walked |
| "RichOS 1.2.5-nightly… is up to date." | 1rem = 16px (`.update-line`) | 18.07:1 | not yet walked |
| "Checked just now." | 1rem = 16px (`.update-sub`) | 6.35:1 (#595E67 on #FDFCF8) | not yet walked |
| "Check for updates" button label | 1rem = 16px (`.update-btn`) | 15.58:1 | not yet walked |
| "See what's new" link | 1rem = 16px (`.update-notes`) | 6.65:1 (#715715 on #FDFCF8) | not yet walked |

All light values clear 4.5:1 and the 16px floor.

## Item 2: write checks in every app session (`20ce71c36`): result not read

Run `walk-27568a1d4c61` (`write-checks-walk.py`, `--seed-home` on an empty home first) reached
`identity: PASS`, `first-run: PASS`, `connect: PASS`, `config: PASS` in the log
(`/Volumes/E1TB/tmp/claude/ray/wc2-run.log`) before this handoff was forced. Its `checks` step (the
secret-key write and the British-spelling write) and the final report in
`/Volumes/E1TB/tmp/claude/ray/wc2-out/report.json` were not read by me. Not claimed PASS.

## Handoff (forced by the Claude-quota gate)

**Verdict: NOT YET DECIDED.** Nothing found so far is a hole a user could hit. Remaining:

1. Read `wc2-out/report.json` and the end of `wc2-run.log` for item 2's `checks` step.
2. Read `f-run.log` and `f-out/` (`walkF.json` dark walk), compute dark contrast with
   `qa/contrast.py` on `f-01-settings-dark.png` and `f-02-after-check-dark.png`, fill the table.
3. Confirm both guests are gone: `slots.py status`, no `walk-` guest left (run-walk.py quits and
   deletes on its own; I stopped nothing by hand).
4. Fill the first-table verdict line, delete `/Volumes/E1TB/tmp/claude/ray`, run the end-of-task
   `git merge main` and `proof-run.py`.
