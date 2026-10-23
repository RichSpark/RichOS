# Escalation: Selector-standing reuse: third review in a row finds a read the dependency map omits; no unit can be shown covered, so I am making it require a reviewed reads-complete declaration (none exist today)

- id: `esc-20261022T093846Z-20cec921`
- raised: 2026-10-22T09:38:46Z
- from: zach-opus-reuse7
- worktree: `/Users/alex/ab/richos-wt/zach-opus-reuse7` (branch `cc/zach-opus-reuse7`)
- head: `db394e717851559009ea09695a1a801266240ecb`
- state: **proceeding**
- for: lead

## The question

Default I am implementing unless you say otherwise: a pass saved at commit A stands at B through the selector only when every node of the suite's mapped closure carries a reviewed declaration that its reads are complete (a new map field). No node has one today, so this reuse applies to no unit until someone reviews one; exact reuse and the cross-checkout PATH fix (9f4fa6bba) are unaffected. Is that what you want or should the selector-standing override be removed outright?

## What was already tried

Review rv-20261022T082555Z (mutation-inventory finds *.mutation.sh with find) was closed by refusing any closure that enumerates files: 165 of 173 covered units. Review rv-20261022T092656Z-db394e71-62b1 then showed provision-claude-md.sh reading VERSION by name: one of the 8 units that enumerate nothing, the map omits VERSION, the selector does not select for it. Same class as the finding behind esc-20261022T054303Z-e2b6f41e: the map lists executed relationships, never every file read, so no static rule over it proves read coverage.

## Proceeding meanwhile

Letting my author run at db394e717 finish (I cannot edit the worktree while its checks run), then implementing the default with a red/green test, then the final author run and the cross-checkout gate replay at the final tip with no edits in between.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261022T093846Z-20cec921`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261022T093846Z-20cec921 --disposition "<what you decided or did>"
