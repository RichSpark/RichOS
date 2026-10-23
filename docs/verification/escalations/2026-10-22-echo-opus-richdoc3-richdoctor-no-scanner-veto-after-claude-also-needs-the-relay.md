# Escalation: RichDoctor 'no scanner veto after Claude' also needs the relay's content veto relaxed (Zach's code)

- id: `esc-20261022T215504Z-44bbcd75`
- raised: 2026-10-22T21:55:05Z
- from: echo-opus-richdoc3
- worktree: `/Users/alex/ab/richos-wt/echo-opus-richdoc3` (branch `cc/echo-opus-richdoc3`)
- head: `0881b5f8f0ee4eadb0eac50e8be928d5bb32a47b`
- state: **proceeding**
- for: lead

## The question

Should Zach's relay stop rejecting reports for privacy-shaped text (richos/mobile/service/report/checks.mjs privateKind: secret, email, home_path -> rejected/content)? Without that, removing RichDoctor's local veto only moves the same refusal to after the network round trip.

## What was already tried

Read richos/mobile/service/report/checks.mjs (contentProblem, privateKind lines 123-130) and RICHDOCTOR-RELAY.md step 4 ('The content rules run. The relay accepts the exact words or rejects them'). RichDoctor's check_outgoing (scrub.rs) mirrors those rules by agreement with the relay owner and approve() (delivery.rs:142) refuses on them.

## Proceeding meanwhile

On cc/echo-opus-richdoc3 I am removing RichDoctor's privacy veto after Claude (approve() and the review list keep only the format, size and receipt-secret rules the relay needs to publish at all), keeping the mechanical sweep first and making the no-AI review say it was reviewed by hand. The relay change is not mine to make; until it lands, a report with an email address or a home path that the person chose to keep will come back 'did not accept the report' from the relay.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261022T215504Z-44bbcd75`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261022T215504Z-44bbcd75 --disposition "<what you decided or did>"
