# Escalation: --new-user folder has no login keychain; making its own empty one on the host is unmeasured and could touch his keychain setup

- id: `esc-20261018T215954Z-e37f7702`
- raised: 2026-10-18T21:59:54Z
- from: zach-opus-newborn66
- worktree: `/Users/alex/ab/richos-wt/zach-opus-newborn66` (branch `cc/zach-opus-newborn66`)
- head: `67c84f4affc80c9d5ac6b3538dc7edba2fac4085`
- state: **proceeding**
- for: lead

## The question

May one test-VM run measure whether making the folder its own empty, unlocked login keychain (security create-keychain plus list-keychains/default-keychain -d user with HOME and CFFIXED_USER_HOME at the folder home, unlock in the GUI session) leaves the account's own search list and default keychain untouched? Only after that proof would the launcher do it on his Mac.

## What was already tried

Read the evidence: docs/testvm.md 'The fixture's login keychain' and richos/app/scripts/testvm/keychain.sh measured (2026-09-20) that a HOME with no keychain makes security add-generic-password raise macOS's 'Keychain Not Found' window and wait (Set my phone up sat 120 s on 'Getting this Mac ready'). docs/verification/first-words-on-the-window-2026-09-19.md shows Claude Code keeps its sign-in in the login keychain, so a brand-new user's Claude sign-in in a folder with no keychain is expected to hit the same window. The VM recipe that fixes it writes the keychain search list and default keychain in the GUI session; on the host that session is the CEO's own, and whether the write lands in the folder's preferences or his is not measured anywhere. The brief forbids touching his Keychain, so I did not try it on this Mac.

## Proceeding meanwhile

Building --new-user exactly as briefed for the four links (none of ~/.claude, ~/.claude.json, ~/Library/Keychains, ~/.local/bin/claude), no keychain made, and the usage text says plainly that whatever stores a secret meets 'Keychain Not Found' and to answer Cancel. Test red before, green after; handover with the command.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261018T215954Z-e37f7702`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261018T215954Z-e37f7702 --disposition "<what you decided or did>"
