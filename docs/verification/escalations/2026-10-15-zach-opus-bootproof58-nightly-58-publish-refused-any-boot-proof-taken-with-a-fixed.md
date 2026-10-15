# Escalation: Nightly 58: publish refused any boot proof taken with a fixed checker; I amended the gate on my branch

- id: `esc-20261015T192120Z-10afc842`
- raised: 2026-10-15T19:21:20Z
- from: zach-opus-bootproof58
- worktree: `/Users/alex/ab/richos-wt/zach-opus-bootproof58` (branch `cc/zach-opus-bootproof58`)
- head: `6c9ded4afb59ac8756e38b5ac29798ac8e3412a1`
- state: **proceeding**
- for: lead

## The question

None needed to proceed. Before landing, read commit 2 on cc/zach-opus-bootproof58: nightly-local.py require_gui_proof now also accepts a gui-boot_commit that descends from the candidate's source or build commit and changes ONLY richos/app/scripts/gui-boot.test.sh. Without it, the only way to publish 58 is a rebuild, which the brief forbade.

## What was already tried

A2 root cause: 9a6ff39eb reworded the write-half boot line; A2 kept grepping out the old wording, removed nothing, and fed the checker the healthy log. Fixed in 6c9ded4af (A2 red before, green after). But gui-proof-in-vm.sh records gui-boot_commit from the checkout HEAD, and publish demanded it equal a07aae59 or 1cc41ffa7 exactly (nightly-local.py:2670). A proof with the fixed checker is therefore at 6c9ded4af and was refused as the gate stood. Recording a false commit or running a dirty tree would have been a lie in the proof.

## Proceeding meanwhile

VM boot proof for run 20261015T175328Z-66ef7231 running now from 6c9ded4af (candidate bundle reused, not rebuilt), writing /Users/alex/.richos-nightly/gui-proofs/20261015T175328Z-66ef7231.checker-6c9ded4af.proof. Publish must run with the landed nightly-local.py, not the source worktree copy.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261015T192120Z-10afc842`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261015T192120Z-10afc842 --disposition "<what you decided or did>"
