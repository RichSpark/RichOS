# Removing a Claude account keeps every chat's history, after the removal and after a relaunch, 2026-11-01

The real-app check of `cc/echo-opus-acct2b`. This branch merges `cc/echo-opus-acct1` with
echo-sonnet-fail3's lost-history handling. It also keeps a removed account's `projects` link for
good: a saved transcript names its long tool output by an absolute path under that account's
folder (handover review rv-20261101T200002Z-f21f248b-b72b, P2).

## What ran

- **App:** `richos/app/scripts/testvm/branch-bundle.sh` at `3bb988e6e`, pinned to nightly 85's
  engine (1.2.0, sha256 `80d40c80…4f9e30`). Every walk's identity step saw that commit. The
  commits after it change only the two walk scripts, which run on the host.
- **Walks:** each ran in its own guest under `run-walk.py` and each ended "clean: app quit, VM
  stopped, clone deleted, state removed".

## Result

- **`account-remove-walk.py`, every step PASS** (`account-remove-report.json.txt`): identity,
  first-run, accounts, chat, switch, remove, relaunch, continue.
  - **chat:** the codeword turn ran on Work.
  - **switch:** the chat moved to Account 1 in the same session.
  - **remove:** Work was removed through its Remove button. Its folder then held only `projects`,
    the link to Account 1's records.
  - **relaunch (new step):** after the app relaunched, Work's folder still held only that link,
    pointing at the same place. The chat's record could still be read through the path under
    Work's folder (4 codeword lines).
  - **continue:** the codeword question was asked after the relaunch. It was answered `kiwi-f428f6`,
    in the same session as the codeword (`dec32d1f`).
- **`history-gone-walk.py`, conversation steps, every step PASS** (`history-gone-report.json.txt`):
  identity, first-run, chat, lose, queue, answer. A chat whose record is gone was told so once,
  and the next message was answered on a new session.
- **`history-gone-walk.py`, Stu steps, every step PASS** (`history-gone-stu-report.json.txt`):
  identity, first-run, stu-job, stu-idle, stu-lose, stu-handoff.
  - Stu's record of session `c0141774` was deleted.
  - Rich was told once that the history is gone.
  - The next handoff settled on a new session, `5887d460`, with a new record.

## Walk-script fixes found on the way (no app change)

- **history-gone-walk.py: opening the lost chat.** The chat's row title is written from the first
  message and varies between runs ("History walk ready"). The row is now found by "History walk".
- **history-gone-walk.py: the home screen after a relaunch.** When the home shows a hired
  specialist, OCR misreads its "Talk to Rich" pill. The home is now also recognized by
  "SPECIALIST". This failed two runs in a row, one of them alone on a quiet host.
- **account-remove-walk.py: the chat after the relaunch.** A relaunch opens a new chat, so the
  relaunch step now opens the codeword's chat from the sidebar. On the first run the question was
  typed into the new chat and never sent.
