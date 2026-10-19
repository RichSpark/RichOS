# Escalation: Account switch: premise partly false - Rich DID move to Home at 22:49:03Z; Stu's whole website job stayed on Work

- id: `esc-20261019T001343Z-6131937a`
- raised: 2026-10-19T00:13:43Z
- from: echo-opus-acctswitch
- worktree: `/Users/alex/ab/richos-wt/echo-opus-acctswitch` (branch `cc/echo-opus-acctswitch`)
- head: `3f3c22b006500e0e76b4b47da43387d5de278445`
- state: **proceeding**
- for: lead

## The question

None needed to proceed. Record only: the brief says no turn ran on Home; the evidence shows Rich's 23 post-switch messages ran as the Home account (Claude Code's injected identity in Rich's transcript changes from alex@leadersadapt.com to abbooster@gmail.com at 2026-10-18T22:49:03Z; pid 70356 env CLAUDE_CONFIG_DIR=claude-accounts/2, RICHOS_CLAUDE_ACCOUNT=2; cold prompt cache on its first request). Home shows 0% because that chat was small (about 1.17M tokens, 94 percent cache reads, 4K output). The work that spent quota was Stu's job (pid 69013, session eff3d819, 57 messages, 4.6M tokens, 33K output): its lease was spawned on Work at 22:48:38, one second before the switch decision at 22:48:39.688, and every later turn of that job was an automatic turn (background-task notification) inside the same process, which the work host never renews (work_host.rs:558 renews only before an app-sent message; running commands also block it, cognition.rs:232). pid 69013 is still on Work now. The 96 percent trigger is a speed artifact: Quota speed is measured between the last reading of 95 and the first of 96 (quota.rs:551-569 moves the speed base on every equal reading), so a one-point integer step over a minute or two reads as abnormal speed; the real rate was about 1 point per 74 minutes (96 to 97 by 00:02Z). The app then said 'I switched the team ... Nothing stopped' as soon as Rich's first turn ran on Home (claude_accounts.rs:620 ran_on, only spine.rs:312 calls it), and Rich repeated it.

## What was already tried

Read the app's claude-accounts.json, engine-state/claude-quota.json, both Claude Code session registries, the running leases' environments (names only, secrets redacted), all three transcripts and the claude 2.1.303 keychain lookup code.

## Proceeding meanwhile

Fixing (a) the speed artifact so the switch is at 99 percent at normal speed, (b) Stu's lease renewing at the end of automatic turns when nothing of his is running, and (c) the switch notice being held until Stu's lease is also on the new account. Stu's job with running commands still finishes on the old account by design (never kill a running command); I will say so plainly in the handoff.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261019T001343Z-6131937a`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261019T001343Z-6131937a --disposition "<what you decided or did>"
