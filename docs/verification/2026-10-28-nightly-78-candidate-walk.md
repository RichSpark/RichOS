# Walk of nightly candidate 78 (`v1.2.5-nightly.20261028.78`), 2026-10-28

Candidate 78 was built from richos main `19454cbb94591c48d8aff83d7c21ef6ae7a3a3a8` (run
`20261028T111233Z-41017cf2`). The boot proof the build ran is not mine and was not rerun:
`result=fail:screen-suites`, `windows=1`, front-door 6/6, gui-boot 35/36. The one failing gui-boot
check is B2, a missing boot-line rule; Echo retakes that proof separately.

| Item | Result |
|---|---|
| Verdict | **READY** (item 2 not walked live, a non-blocking gap: see item 2 and the verdict section) |
| Identity | **PASS** (below) |
| 1. Codex watcher (`ef7a26057`) | **PASS**, carried from my earlier run (deleted before it could commit) |
| 2. Write checks in every app session (`20ce71c36`) | **NOT WALKED LIVE**: two full walks, both stopped at `checks`, the guest's account at 99% of its weekly limit |
| 3. Updater, changelog link, 1.2.5 bump (`8a2689030`, `9e64a48fc`) | **PASS** light and dark |
| Contrast | **PASS** light and dark, every row at or above 5.80:1 (table below) |

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

## 3. Updater, changelog link and the 1.2.5 bump (`8a2689030`, `9e64a48fc`): PASS, light and dark

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
  (`ax: notfound`, step 38). A corrected plan, `walkF.json` (click Dark theme directly), ran next in
  its own guest `walk-66a7733b8f6a` (`f-run.log`, `f-out/steps.json`, exit clean, guest deleted).
- **Dark half: PASS.** `walkF` pressed Dark theme (the app turned dark, frame `f-03-home-dark.png`)
  and reopened Settings (`f-04-settings-dark-again.png`, read live on screen). The Updates row reads
  "RichOS 1.2.5-nightly.20261028.78 is up to date.", "Checked 1 minute ago.", the "Check for updates"
  button and the "See what's new" link, all legible in dark. The link's target is theme-independent
  and was walked in light.

## Contrast (`qa/contrast.py`, toolkit commit `5b4a0e013`; light `e-02-after-check.png`, dark `f-04-settings-dark-again.png`, 100% text size)

| What | Size (source, `style.css`) | Light | Dark |
|---|---|---|---|
| "Updates" heading | 16px (not read from CSS, the row's title) | 18.07:1 | 12.06:1 (#DFE4EE on #182440) |
| "RichOS 1.2.5-nightly… is up to date." | 1rem = 16px (`.update-line`) | 18.07:1 | 12.06:1 (#DFE4EE on #182440) |
| "Checked just now." | 1rem = 16px (`.update-sub`) | 6.35:1 (#595E67 on #FDFCF8) | 5.80:1 (#989FAF on #182440) |
| "Check for updates" button label | 1rem = 16px (`.update-btn`) | 15.58:1 | 14.78:1 (#DFE4EE on #0A1120) |
| "See what's new" link | 1rem = 16px (`.update-notes`) | 6.65:1 (#715715 on #FDFCF8) | 6.36:1 (#C2A35C on #182440) |

All values, light and dark, clear 4.5:1 at 16px. Dark values were measured on the guest's own live
frame (`walk-66a7733b8f6a`, 1680x1050, regions read from the live accessibility positions); the
"Checked" line reads "Checked 1 minute ago." in the dark frame and "Checked just now." in the light one.

## Item 2: write checks in every app session (`20ce71c36`): not walked live, blocked by the account's quota

Two complete walks of `write-checks-walk.py` (the whole walk each time, never `checks` alone, each in
a fresh guest with `--seed-home` on an empty home, the candidate zip and engine tarball,
`--expect-sha f1de705bab2ea4d366c0dff996f635bba9599280`):

| Run | Guest | Output | identity | first-run | connect | config | checks |
|---|---|---|---|---|---|---|---|
| 1 | `walk-27568a1d4c61` | `wc2-out/report.json` | PASS | PASS | PASS | PASS | FAIL after 616 s |
| 2 | `walk-50b6fc33522a` | `wc3-out/report.json` | PASS | PASS | PASS | PASS | FAIL after the 600 s limit |

(A third start, `walk-633b8ca099ac`, was stopped by my own 600 s background timeout during `checks`;
I removed its clone with `stop.sh` and it is not counted.)

- **What the app did.** In both runs the company, the connected folder and `DIALECT_TARGET="en-US"`
  were created and the job was typed. Rich's turn started (`TurnStarted`, conversation ledger) and
  the app then posted its own notice: "Your Claude account has used **99%** of this week's limit. It
  is fresh again tomorrow at 5:00 AM. If it runs out before then, the team pauses until it is." That
  notice arrived in each run about 14 s to 64 s after the prompt. After it, no reply from Rich, no
  `PreToolUse` or `PostToolUse` record and no teammate call reached the end of the 10-minute window
  (`observed-checks.json`: `teammate_calls` 0, `secret_write_asked` 0, `dialect` "neither").
- **Why it is not a finding against the candidate.** The guest runs on a copy of this Mac's Claude
  login and that account reads 99% used. Item 1 got Rich's model reply at 12:30Z the same day, so
  the model path worked when the account had room. The silence after the quota notice is what the
  product says it does at that limit. This is consistent with a quota stop; I could not prove the
  cause beyond that, because the guest's model call leaves no record of its own.
- **What this does not show.** Nothing in either run shows a write wrongly allowed or wrongly refused,
  and nothing shows a teammate with an explicit `tools:` list failing to start. They are unobserved,
  not clear. The secret-key refusal, the British-to-American spelling guard and the `tools:` start
  are NOT verified on the live app.
- **Re-walk when the limit resets** (the notice says tomorrow 5:00 AM): the same two commands, `--seed-home` then
  `run-walk.py ... -- write-checks-walk.py --out DIR --expect-sha f1de705...`, with a `--wait` of 900 and a
  caller timeout above 1,400 s (the walk holds a guest for about 11 to 13 minutes).

## Verdict: READY, with one named gap

Nothing I found is a hole a user could hit in normal use from these changes: identity matches the
build commit in every guest; the Codex watcher delivers each entry once with no replay after a
relaunch; the Settings version line, Check for updates and the changelog link work in light and
dark; every checked text row clears 4.5:1 in both themes (lowest 5.80:1).

**The gap, stated so the publisher can weigh it:** item 2 (`20ce71c36`, the write checks) was not
seen working on the live app. I rate it non-blocking under the brief's rule (only a found hole blocks)
because the stop came from the account's quota, not from anything the candidate did, but it is an
unverified change to what a teammate may write. If the CEO wants it seen before publishing, the
answer is a re-walk after the limit resets, not a different build.

Other non-blocking notes: item 1 has no light/dark frames and its `CODEX_CHANNELS` entrance was not
walked; the boot proof's one failing gui-boot check (B2, a missing boot-line rule) is Echo's to retake
and was not rerun here.

## Cleanup, checked

Guests: `run-walk.py` quit the app by its recorded pid, stopped the VM and deleted the clone in every
run (`clean: app quit, VM stopped, clone deleted, state removed`); the one I interrupted I cleaned
with `stop.sh` on its own name. `slots.py status` shows `guest.lock: free` and `guest-2.lock: free`,
no `run-walk.py` or `write-checks-walk.py` of mine is running and only `richos-base` remains in the
VM store. Scratch `/Volumes/E1TB/tmp/claude/ray` deleted after this record was written.
