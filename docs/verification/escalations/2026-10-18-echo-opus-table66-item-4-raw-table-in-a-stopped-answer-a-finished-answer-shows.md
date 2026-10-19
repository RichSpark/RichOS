# Escalation: Item 4 (raw table in a stopped answer): a finished answer shows the same raw pipes on main; the fix is all of Rich's answers

- id: `esc-20261018T191403Z-dd54748f`
- raised: 2026-10-18T19:14:03Z
- from: echo-opus-table66
- worktree: `/Users/alex/ab/richos-wt/echo-opus-table66` (branch `cc/echo-opus-table66`)
- head: `6d075e8929701e997e852beb378f2cb960b4d3c8`
- state: **proceeding**
- for: lead

## The question

None blocking. Record: item 4 says a stopped answer should render tables 'the same as a finished one', but on richos main 6d075e892 a finished answer does not render tables either. ui/timeline.js renderMarkdownInto draws tables, quotations, rules and fenced code only under opts.document (the Output panel); renderRichMessage (timeline.js:2211) and updateProse (timeline.js:3351) never pass it, and ui/tests/markdown.js check 12 pins that answers keep raw '|' and '>' lines. Ray's nightly 64 verdict.json:35 says 'completed-answer tables not checked'. The stopped answer in s01-stopped-03s.jpg also shows a raw '>' quotation. So I am turning the existing block extension (tables, quotations, rules, fenced code; same DOM-only renderer) on for every Rich answer, stopped or finished, with the Output panel's approved table/quote styling copied to .tl-prose, and rewriting check 12 accordingly. Links and backslash escapes stay document-only. The output-panel PRD 2026-10-05 row M3 said answers render 'only the subset Rich emits, by design'; the walk shows Rich emits tables, so that premise no longer holds.

## What was already tried

Read timeline.js renderMarkdownInto/mdDocumentBlock, markdown.js checks 11-14, the nightly 64 verdict and both s5 screenshots.

## Proceeding meanwhile

Building the fix, its red-on-main test, and the VM check.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261018T191403Z-dd54748f`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261018T191403Z-dd54748f --disposition "<what you decided or did>"
