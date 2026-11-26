# Opening the app and changing Rich's model uses no model (test VM, 2026-11-26)

The CEO's words: "Everything was either stopped, finished, ended or paused in the RichOS app. So,
unless I'm missing something major, there should have been absolutely zero usage after me just
opening the app."

## What ran

- **App:** `richos/app/scripts/testvm/branch-bundle.sh` at `5c8759ead` (the one rule for a turn the
  app starts by itself, `richos-core` `turn_gate.rs`, with the review fixes through that commit).
  The running app reported `built_from: 5c8759ead6a28df0ea94b91f2e758a6da41ddf8f`.
- **Engine:** this branch's own, `richos/app/scripts/make-engine-asset.sh` from the same tree (its
  review watcher reads the rule's record), given to the guest with `run-walk.py --engine`.
- **Scenario:** `richos/app/scripts/testvm/open-idle-walk.py` under `run-walk.py`, one guest, exit 0,
  on fixture data (company "Acme"). Nothing was shown on the host's screen and no sound played.
- **Result:** all ten steps passed; the figures below are from the run's own `report.json` and
  `paused.json` (written by the walk to its `--out` folder).

## What it shows

| Step | What happened | The app's own record |
|---|---|---|
| paused | The app quit. While it was closed, in the chat: a job of his paused for the usage limit, as the app keeps it (its assignment record, reported `waiting-for-quota` by the engine's own `report_assignment` and its handoff, given to Stu's session and kept open while the job is), the Stop the app saves for the team, a report in Rich's inbox of the paused job and a READY TO LAND handover whose review the last run had taken up and Quit stopped (seeded with the engine's own review watcher). The app opened, Settings opened, the conversation's row set to Opus, the team's row to Opus and back to Fable, Settings closed with Escape, then 90 s left alone. | 0 transcripts, 0 model answers before and after; the paused job still `waiting-for-quota` and its handoff's receipt unchanged (not given again); the Stop still in the team's inbox; the paused report not filed as a turn (0 agent turns in the ledger, so the chat shows nothing running); no review lock, review log, review ledger row or `second-review` process; `turn-gate.json` lists no chat; `config.json` holds `rich_model: opus`, `stu_model: fable` |
| closed | The app quit and a Codex report written on the chat's connection while it was closed. | 0 transcripts, 0 answers |
| open | Opened, Settings opened, the conversation's row set to Opus, Escape, 90 s alone. | still 0 transcripts and 0 answers; the report is not in the team's inbox; the app's log says it waits |
| window | Settings opened again, the team's row set to Opus and the window closed by its red button with Settings still open. | `config.json` holds `stu_model: opus` |
| ask | Opened again, the conversation opened from the sidebar and a question typed. | the held report reached the inbox once; 14 answers, all `claude-opus-5-5` |
| live | A report written while the app was open, nothing typed. | it reached the inbox by itself; all answers `claude-opus-5-5` |
| behind | Closed, a report written, opened and left alone (it waits), then one more written while open. | both reached the inbox, the one from before first; all answers `claude-opus-5-5` |

No answer from a Fable model appears anywhere in the run and nothing answered at all until the
user typed.

Earlier runs of the same walk: at `c49dbf61d` the held report in Rich's inbox, filed as a turn,
showed the chat as "Working" with nothing running (fixed in `c45967b1f`); at `c45967b1f` all ten
steps passed without the persisted paused job, which this run adds.

**Test instance:** `run-walk.py` quit the guest app (pid 12619), stopped the VM and deleted the clone
("clean: app quit, VM stopped, clone deleted, state removed").
