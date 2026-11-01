# A chat that ran on a second Claude account keeps its history and answers after that account is removed, 2026-11-01

The real-app check for the CEO's report of 2026-11-01: removing one connected Claude account broke
every chat that had run on it. Every account's chat records now live in Account 1's Claude Code
folder (`projects/`) and an added account's own `projects` is only a link to it
(`claude_accounts.rs`, `Accounts::share_records`).

## What ran

- **App:** `richos/app/scripts/testvm/branch-bundle.sh` at `2433aae77` (this branch), pinned to
  nightly 85's engine (1.2.0, sha256 `80d40c80…4f9e30`). The running app reported that commit
  (identity step).
- **Scenario:** `richos/app/scripts/testvm/account-remove-walk.py` under `run-walk.py`, one guest
  run of 239 s ending 17:59Z, exit 0. Every step passed: identity, first-run, accounts, chat,
  remove, continue (`report.json.txt`, which holds each step's evidence).
- **Sign-ins:** this Mac has no sign-in for a second Claude account, so the guest's Work folder
  was signed in with this Mac's own Claude sign-in (`claude-login.sh push --account`, access token
  only). Work was a separate account folder (`CLAUDE_CONFIG_DIR`) with its own sign-in, on the
  same Claude subscription.

## Result

- **accounts.** Work was added to the app's own list and put in use, then the app was relaunched.
  After the relaunch, Work's `projects` was a link to the guest's `~/.claude/projects`.
- **chat.** The message carrying codeword `kiwi-889d6f` was answered ("noted") on Work: the
  provider's `transcript_path` for that turn is under `claude-accounts/2/projects/`. The record
  itself is in `~/.claude/projects/` (session `7a6a50f3`).
- **remove.** Work's Remove button and its confirmation in the Claude accounts sheet, used as a
  person uses them. The list then held Account 1 only and Work's folder was gone
  (`removed.png`: "Work is removed from RichOS.").
- **continue.** "What was the codeword…" ran in the same session `7a6a50f3`, now on Account 1
  (`transcript_path` under `~/.claude/projects/`) and was answered `kiwi-889d6f`
  (`continue.png`). The chat resumed its own record and did not start over.

The test app was quit, the VM stopped and the clone deleted by `run-walk.py`
("clean: app quit, VM stopped, clone deleted, state removed").
