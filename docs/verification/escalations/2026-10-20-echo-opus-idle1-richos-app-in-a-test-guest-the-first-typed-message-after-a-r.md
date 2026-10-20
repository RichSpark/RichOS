# Escalation: RichOS app in a test guest: the first typed message after a relaunch is swallowed (composer emptied, nothing on screen, no ledger turn); team-watch-walk.py's task step never noticed

- id: `esc-20261020T171021Z-8fd0fca7`
- raised: 2026-10-20T17:10:22Z
- from: echo-opus-idle1
- worktree: `/Users/alex/ab/richos-wt/echo-opus-idle1` (branch `cc/echo-opus-idle1`)
- head: `f8275f58e85d41df4287d9cc8a32b24dfd6e9905`
- state: **proceeding**
- for: lead

## The question

Who owns the swallowed first send? Seen 2026-10-20 on a bundle built from richos f8275f58e (no UI change since e323589f4, run 12's bundle, which did not show it): after the walk's relaunch (home screen, Enter, composer back), the task was typed into the composer (value read back by AX: the full text) and the one Send button was pressed (AXPress, clicked: true). The composer emptied, no user message appeared and conversation-ledger.jsonl recorded no PromptReceived row within 30 s. Typed and sent again in the same guest, it became a turn at once. Six runs: the first send was lost in all six (runs 1-5 had no second send, so no lead ever started). This may be a harness artifact of AX paste plus AXPress rather than something a person at a keyboard hits; it has not been reproduced by hand. Evidence: /Volumes/E1TB/tmp/claude/echo-opus-idle1/walk/task.json and task-attempt-1.png (scratch, deleted at my handoff; the walk change committed on cc/echo-opus-idle1 makes every future run record the same evidence).

## What was already tried

Ruled out this branch's change: main.js send() draws the pending user bubble before any bridge call (main.js:2787 before :2804) and none appeared and operator.log has no relay, so no Rust code ran for the lost send. Ruled out the AX targets: exactly one 'Message to Rich' textarea and one 'Send' button were exposed and both were the composer's. Ruled out home-screen voice: Enter only hides the home screen (home.js:900).

## Proceeding meanwhile

team-watch-walk.py now confirms the send by its PromptReceived row and re-sends once only after a verified non-receipt, recording both attempts, so the real-app check of the unfinished-work reminder can finish.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261020T171021Z-8fd0fca7`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261020T171021Z-8fd0fca7 --disposition "<what you decided or did>"
