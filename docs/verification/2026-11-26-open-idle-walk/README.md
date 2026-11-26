# Opening the app and changing Rich's model uses no model (test VM, 2026-11-26)

The CEO's words: "there should have been absolutely zero usage after me just opening the app."

## What ran

- **App:** `richos/app/scripts/testvm/branch-bundle.sh` at `cfa2a307a`, pinned to nightly 118's engine
  (1.2.0). The running app reported `built_from: cfa2a307ae8067e2935aa39ddb21b249a752cae6`.
- **Scenario:** `richos/app/scripts/testvm/open-idle-walk.py` under `run-walk.py`, one guest, 505 s,
  exit 0, on fixture data (company "Acme"). Nothing was shown on the host's screen and no sound played.
- **Result:** all nine steps passed; the figures below are from its `report.json`.

## What it shows

| Step | What happened | The app's own record |
|---|---|---|
| closed | The app was quit and a Codex report written on the chat's connection while it was closed. | 0 transcripts, 0 model answers |
| open | The app was opened, Settings opened, the conversation's row set to Opus, Settings closed with Escape, then 90 s left alone. | `config.json` holds `rich_model: opus`; still 0 transcripts and 0 answers; the report is not in the team's inbox; the app's log says it waits |
| window | Settings opened again, the team's row set to Opus and the window closed by its red button with Settings still open. | `config.json` holds `stu_model: opus` |
| ask | The app was opened again and a question typed in that chat. | The held report reached the inbox once; 2 answers, both from `claude-opus-5-5` |
| live | A second report written while the app was open, nothing typed. | It reached the inbox by itself; 3 answers in all, every one from `claude-opus-5-5` |
| behind | The app closed, a report written, the app opened and left alone (the report waits), then one more written while open. | Both reached the inbox, the one from before first; every answer from `claude-opus-5-5` |

No answer from a Fable model appears anywhere in the run.

**Test instance:** `run-walk.py` quit the guest app (pid 7413), stopped the VM and deleted the clone
("clean: app quit, VM stopped, clone deleted, state removed").
