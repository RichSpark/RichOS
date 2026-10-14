# Escalation: shots55: 53 of 56 changed pictures also show the rail header NEEDS AN ENTITY become 'Needs an entity' (same CSS line as the company names)

- id: `esc-20261013T232139Z-512b23b4`
- raised: 2026-10-13T23:21:39Z
- from: echo-opus-shots55
- worktree: `/Users/alex/ab/richos-wt/echo-opus-shots55` (branch `cc/echo-opus-shots55`)
- head: `fd17de1179b1c61dc8a96bec6e0e89544cbd6532`
- state: **proceeding**
- for: lead

## The question

May I regenerate the 53 pictures whose only other change is the rail's 'Needs an entity' group header losing its all-caps? Yes = I regenerate them now; No = the header must stay uppercase, which is a one-line code change in style.css/main.js on cc/echo-opus-dictre55, not a picture change.

## What was already tried

Merged main into cc/echo-opus-shots55 (clean). Ran all 7 suites. 56 pictures differ. Pixel-band analysis: every difference sits in 7 rows of the left rail, x 31-206, y 244-674. Six bands are the six company group labels (Northwind Traders, Lumen Labs, Meridian Group, Harbor Analytics, Tidewater Films, Kestrel Supply) now as typed instead of uppercase, which is intended. The seventh band (y 661-674) is the 'Needs an entity' group header, which shares the class .nav-group-label whose text-transform: uppercase the branch removed (style.css:2178), so it now reads 'Needs an entity' instead of 'NEEDS AN ENTITY'. Nothing else differs anywhere in any picture. 3 pictures (3-1-05-where-should-this-apply, 10-1-preferences-dark/light) have only the first company label band.

## Proceeding meanwhile

Regenerating and committing the 3 pictures that show only the company-name change. The 53 others are NOT regenerated per the brief; the brief's literal rule says report, not decide.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261013T232139Z-512b23b4`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261013T232139Z-512b23b4 --disposition "<what you decided or did>"
