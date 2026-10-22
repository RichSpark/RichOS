# Escalation: RichDoctor: round 40's example issue footer differs from the relay's agreed byte format; building the relay's

- id: `esc-20261022T161834Z-48df35c8`
- raised: 2026-10-22T16:18:34Z
- from: echo-opus-richdoc1
- worktree: `/Users/alex/ab/richos-wt/echo-opus-richdoc1` (branch `cc/echo-opus-richdoc1`)
- head: `37637303c468c1c7904419c36c83ade0df8fa43c`
- state: **proceeding**
- for: lead

## The question

Should the public issue's screenshot lines switch to round 40's wording (which needs zach-opus-reportrelay1 to change the relay's byte-for-byte body check) or stay on the agreed relay protocol 2 wording?

## What was already tried

Round 40 README section 4 shows the outgoing text ending: 'Screenshots: 3, shared privately with RichOS support (staff sign-in required)' then '1. https://report.richos.ceo/images/RD-7K3M-Q9VX/1' and so on. The relay contract agreed with zach (richos/mobile/service/RICHDOCTOR-RELAY.md on cc/zach-opus-reportrelay1, commit 51b4ccd93) requires, byte for byte, 'Screenshots shared privately with RichOS support: 3' then 'Screenshot 1 (RichOS support staff only): <relay>/v1/reports/<id>/attachments/1', then the footer. The relay rejects any other ending. The PRD (final V6, section 6) fixes no byte format, only that the issue shows 'Screenshots shared privately with RichOS support' and numbered support-only links; both wordings satisfy it. Iris's URL path /images/<id>/N is also illustrative; the real staff route is the relay's /v1/reports/<id>/attachments/N.

## Proceeding meanwhile

Building every round 40 screen and its UI copy as drawn, including the review's footer line ('Footer, included as sent: Screenshots: 3, numbered links only the RichOS support team can open ...'). Show as it will be sent displays the exact bytes the relay accepts (the agreed wording). If you choose round 40's wording, it is a one-line change in report::ending on my side plus the matching check in zach's relay.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261022T161834Z-48df35c8`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261022T161834Z-48df35c8 --disposition "<what you decided or did>"
