# Escalation: CORRECTION to esc-20261022T231346Z-c7a9937f: the iPhone largest-text-size hang is already on main, not from 21af5639a; filecard3 proceeds to handover

- id: `esc-20261022T232706Z-335cacd1`
- raised: 2026-10-22T23:27:06Z
- from: echo-opus-filecard3
- worktree: `/Users/alex/ab/richos-wt/echo-opus-filecard3` (branch `cc/echo-opus-filecard3`)
- head: `6d3a6134b6b4e2ebb53368234998af5acaa5cb70`
- state: **proceeding**
- for: lead

## The question

None for this branch. Main needs an owner (the iOS engineer) for one user-reachable defect and two red UI cases, all reproduced on main's own tree 8cecfc544 and listed under tried.

## What was already tried

A second run of AccessibilityLayoutTests testTheHeaderHoldsTheLongestLineWithTheKeyboardAndACard on main's tree 8cecfc544 (rerun_tree.py scratch checkout, iPhone 16 Pro Max) FAILED exactly as on this branch: the fifth launch (conn-tailscale-off, light, the largest accessibility text size) never leaves the launch logo. The first base run passed, so on main it is intermittent (1 pass, 1 fail); on this branch's trees it failed 5 of 5. Reverting both of 21af5639a's view-structure changes on HEAD did not stop it, so I withdraw the attribution to 21af5639a. A stack sample of the hung app (my own test instance, 99% CPU) shows an endless SwiftUI layout loop: ScreenView.body, bottomZone (cardsRoom, EdgeCuedScroll wholeLines, headerBottom/zoneTop geometry) and TranscriptView.updateUIView are re-run on every pass. A user at the largest text size with Tailscale off and a waiting message gets a frozen app. On main too: InteractionTests testATapIsTooShortAndSendsNothing and testAPressWithTheMicrophoneOffShowsTheCardAndNotNowDismissesIt fail on iPhone SE at 8cecfc544 (row m1 leaves the visible set when the hint appears). Evidence: /Volumes/E1TB/caches/richos-native-ios-ui/cfe92503c4/run.L0NBtB (base hang), /Volumes/E1TB/caches/richos-native-ios-ui/a6eefe2745/run.Zq1i4N (base SE failures).

## Proceeding meanwhile

Handing over filecard3 with native-ios-ui's failures named as main's, not this branch's; no change to main's layout code on this branch.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261022T232706Z-335cacd1`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261022T232706Z-335cacd1 --disposition "<what you decided or did>"
