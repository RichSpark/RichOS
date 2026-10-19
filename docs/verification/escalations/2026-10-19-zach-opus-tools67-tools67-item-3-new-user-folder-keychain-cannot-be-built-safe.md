# Escalation: tools67 item 3 (new-user folder keychain) cannot be built safely: it contradicts the 2026-10-18 disposition of esc-20261018T215954Z-e37f7702 and this brief's no-test-VM rule

- id: `esc-20261019T105330Z-ff20b49b`
- raised: 2026-10-19T10:53:30Z
- from: zach-opus-tools67
- worktree: `/Users/alex/ab/richos-wt/zach-opus-tools67` (branch `cc/zach-opus-tools67`)
- head: `63b11f7a191983e6506a7914d33af0d08e7f337f`
- state: **proceeding**
- for: lead

## The question

Item 3 (rv-20261018T221416Z-e862157b-95ed#1, give the --new-user folder its own usable login keychain) needs either keychain creation plus a keychain search-list/default-keychain write in the CEO's own GUI session on his Mac or a test-VM measurement first that such a write lands only in the folder's preferences. The recorded disposition of esc-20261018T215954Z-e37f7702 (2026-10-18 22:00) says: no keychain creation on his Mac and no VM measurement now; a separate macOS user account is the recommendation. This brief also forbids the test VM. Does that disposition still stand (then item 3 stays skipped, as I am doing) or is one test-VM measurement now allowed?

## What was already tried

Read the review fixture (it only probes for a login.keychain file), nightly-launch.sh on main (no keychain step by design), docs/testvm.md 'The fixture's login keychain' (unlock must happen in the GUI session via launchctl asuser, measured in a guest) and the escalation ledger entry and its ack.

## Proceeding meanwhile

Skipping item 3 with no code change; items 1, 2, 4, 5 and 6 proceed.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261019T105330Z-ff20b49b`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261019T105330Z-ff20b49b --disposition "<what you decided or did>"
