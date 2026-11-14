NOT READY: nightly v1.3.2-nightly.20261114.98 (run 20261114T035233Z-81b80da2, source 01a169eb9, build e65b67d0b). Two reasons. (1) **Item 1, Source links in meeting notes, is still broken.** The first link you press, with the Output panel closed, now lands on its passage. The next link you press, with the panel already open, reopens the transcript at its top: the cited line is 540 to 740 px below the bottom of the panel. I saw this in both of my runs, at 4 s and again at 12 s after each press. (2) **The candidate's own VM boot proof failed**, so the publish command will refuse this build whatever this walk says. Items 2 to 5 pass.

# Nightly 98 candidate walk, 2026-11-14 (Ray)

**How it was tested.** The candidate's own zip (`RichOS-1.3.2-nightly.20261114.98-macos-aarch64.zip`; `shasum -a 256 -c SHA256SUMS` reported every file OK). It ran only in the test VM, through `run-walk.py`: headless, one guest per run, with the guest screen at 1680x1050 and a single window. No second user was involved. Five runs; every run's report says `cleanup_complete: true` (app quit, VM stopped, clone deleted). Every claim below was **checked on the live app in the VM**, by reading the screen and the accessibility tree, unless it says otherwise. Frames are in `docs/verification/2026-11-14-nightly-98-candidate-walk/`.

**Identity.** Every run's identity step passed. The app log says `this app: built from e65b67d0b9ae0d72d08473c7395e2f42f5f50a08` and the build's own `build-info.json` maps that build commit to source `01a169eb929cc2f3e316d4e9616a59d83d1f36c5`, version `1.3.2-nightly.20261114.98`.

**Tools used.** Toolkit `richos/app/scripts/qa/` on this branch: `contrast.py` (every ratio below, one estimator), `frame.py crop`/`px`, `redact.py --homes --phones` and `ocr-gate.sh` (0 of 18 frames flagged; the positive control passed), `speech-wav.py` (the meeting fixture, rendered to a file, so **nothing was played aloud**, per CEO §53) and `wait-for.sh`. Test VM tools: `run-walk.py`, `release-lines-walk.py` (unchanged) and `home-standard-walk.py`, to which this branch adds four checks. **Written from scratch: nothing.** The additions to `home-standard-walk.py`:
- `identity` (adopt-walk's rule);
- `history`, a fictional Claude Code input history with a known answer for every SwearMeter number;
- the card's last-30-days figure, all-time total, newest six months, the "Show 6 earlier months" reveal and a dark-mode frame of the card;
- `enter` (Return opens Talk to Rich).

The new offline test `scripts/testvm/test/home-standard-walk.test.py` is registered in the testvm suite (208 of 208 passed).

## One line per item

1. **Source link from a closed Output panel: NOT READY.** Two runs, each the same `release-lines-walk.py --steps identity,companion,audio,first-run,prompt,ask-live,stop-notes` with a 166 s three-voice meeting.
   - **First press: fixed.** It came from a closed panel (`source-links-before-press-panel-closed.png`). Run 1, Source 00:29: the cited rows were at y 545 and 607 inside the panel's 190 to 900 view, at 4 s and at 12 s (`source-0029-first-press-in-view.png`). Run 2, Source 00:19: y 533 (`source-0019-first-press-in-view-run2.png`).
   - **Second press: broken.** It came from the open panel. Run 1, Source 00:44: the panel went back to "Transcript — Desktop call" at its top, with the [00:44] rows at y 1638 and 1700 (`source-0044-second-press-lands-at-top.png`). Run 2, Source 00:31: top again, with the [00:31] row at y 1439 (`source-0031-second-press-lands-at-top-run2.png`). Both readings were the same at 4 s and 12 s. "This source passage could not be located." never showed.
   - **What a user sees:** the first Source link they press works. Every later one opens the transcript at its header and they have to scroll down hunting for the moment the link names. The fix (`b1220480a`, `landPassage`) only waits for the panel's opening animation. The second press comes with the panel already open and the regression test in `ui/tests/output.js` presses only from a closed panel. That second point is a hypothesis from the code, not something I measured.
2. **RichFiles hint text contrast: PASS.** `release-lines-walk.py --steps identity,first-run,richfiles`, all PASS. Measured with `contrast.py` on the guest frames:

   | Text | Light | Dark | Nightly 97 (light / dark) |
   |---|---|---|---|
   | Search box hint "Search admin" | 5.83:1 (#53575E on #EAE6DD) | 7.58:1 (#A0A6B1 on #0C1322) | 1.80:1 / 2.19:1 |
   | Disk Usage footnote | 7.07:1 (#53575E on #FDFCF8) | 6.29:1 (#A0A6B1 on #182440) | 3.51:1 / not measured |

   Frames: `richfiles-search-hint-{light,dark}.png`, `richfiles-disk-usage-footnote-{light,dark}.png`.
3. **Home screen with two companies: PASS.** `home-standard-walk.py`, step `picture`, in both runs of it.
   - The picture kept from the repositories has exactly two territories, Alder Works 1,822 and Brightwater Labs 918, each labeled on its own side of the picture, with 2,740 objects and 1 cross-company link.
   - No "Example, not yours" is on the screen.
   - The left column reads "LORO · 2,740 MEMORIES", "3 AI SPECIALISTS", "2,716 SOURCES UNDERSTOOD" and "26 DECISIONS REMEMBERED" (`home-two-companies.png`).
   - Contrast of the column labels on the dark home: 5.17 to 10.31:1. The home screen is dark in both appearances.
4. **SwearMeter: PASS.** The `history` step wrote 252 fictional entries: 5 in the last 30 days and a different count in each of 14 months.
   - **On.** The home column shows "5 swear words / IN THE LAST 30 DAYS" directly above Talk to Rich (`swearmeter-on-home-signal.png`). Contrast: 13.69:1 for the number, 8.68:1 for "swear words" and 4.86:1 for the small caps.
   - **Rail button.** The SwearMeter button sits on the rail above Bust a bug. Resting the pointer on it opens the card: Last 30 days 5, A day on average Under 1, All time 252, then By month from November ("so far") 5 down to June 15 (`swearmeter-card-light.png`). "Show 6 earlier months" reveals May to December 2025 in place and the button then offers "Show 2 earlier months" (`swearmeter-card-six-more-months.png`). The card closes when the pointer leaves.
   - **Card contrast.** Light: 5.37 to 18.07:1 for text and 3.83:1 for the month bars. Dark: 5.80 to 13.02:1 for text and 6.87:1 for the bars (`swearmeter-card-dark.png`).
   - **Off.** The home signal is gone (`swearmeter-off-home.png`) and so is the rail button (`swearmeter-off-rail.png`).
5. **Enter or Space on the home screen: PASS.** With focus on nothing (a click on the picture first), Space opened Talk to Rich (`space-opens-talk-to-rich.png`) and so did Return (`enter-opens-talk-to-rich.png`).

## Not this walk's items, but blocking

- **The build's VM boot proof failed.** Build log lines 12150 to 12212:
  - `UNACCOUNTED [richos] SwearMeter historical sources: read (0 ms)`
  - `FAIL B2 every line of a healthy Finder-condition boot is accounted for`
  - `FAIL the candidate's guest proof did not complete (fail:screen-suites) ... publish will refuse it`

  The proof record is `~/.richos-nightly/gui-proofs/20261114T035233Z-81b80da2.proof`. The build stopped there, so its new-user walk never ran. Cause: merge `0234a504a` added that boot line (`swearmeter_host.rs:128`) with no matching rule in `scripts/gui-boot.test.sh`. Raised as `esc-20261114T044731Z-28622006`.

## Notes that do not block

- "Ask Rich about the call while it records" gave the same answer as on nightly 97, the VM speed limit: in both runs Rich said the live transcript was unavailable and did not guess. It was not in this walk's scope.
- In my first combined run, the RichFiles step could not open Find Duplicate Files and Disk Usage. The guest's accessibility read was incomplete (node cap reached) while the Output panel held the transcript. This was a driver limit, not the app. The clean RichFiles run opened all three sheets.

## Surfaces applied

Applied:
- The home screen (both appearances read the same dark home).
- The rail and the SwearMeter card (light and dark).
- Settings' Show SwearMeter switch, on and off.
- The conversation, the meeting panel, the Output panel and the RichFiles page and its sheets (light and dark).
- A returning user's start (relaunch with two registered companies).

Not applicable:
- The phones and `richos/mobile/**`: no mobile change, so the battery check does not apply.
- Voice talk-over: no barge-in claim was tested.

## The meeting fixture (input to `qa/speech-wav.py`, voices Daniel, Karen, Moira)

34 lines, 166.6 s. Decisions at 21 s (raise wholesale prices 8% on March 1), 36 s (no grace period), 67 s (second tin proof) and 139 s (flat $12 shipping under $200). The booth line "Seattle Coffee Fest in April. It is in hall B, booth forty two" is at 81 s. Every name and company is invented.

## Test data and app instances

Invented fixtures only, inside each disposable guest. Nothing ran on this Mac's screen or in its own app data (§65). Every guest's app was quit and its clone deleted by `run-walk.py`'s stop (`cleanup_complete: true` in all five reports). No `richos-tauri` was started on this Mac.
