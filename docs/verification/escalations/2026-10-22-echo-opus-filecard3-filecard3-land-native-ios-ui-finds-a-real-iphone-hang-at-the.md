# Escalation: filecard3 land: native-ios-ui finds a real iPhone hang at the largest text size, from the rich-file commit 21af5639a

- id: `esc-20261022T231346Z-c7a9937f`
- raised: 2026-10-22T23:13:47Z
- from: echo-opus-filecard3
- worktree: `/Users/alex/ab/richos-wt/echo-opus-filecard3` (branch `cc/echo-opus-filecard3`)
- head: `6d3a6134b6b4e2ebb53368234998af5acaa5cb70`
- state: **proceeding**
- for: lead

## The question

None needed now; this is notice that the filecard3 handover is delayed by a real defect and native-ios-ui cannot go green on this branch until it is fixed. Say so if you want it handed over as is, with the defect filed as a follow-up.

## What was already tried

Proof run 2 (logs /Volumes/E1TB/state/richos/proof-runs/624b4742c10a/20261022T181125Z-vlhm8vox): 156 of 160 passed; native-ios-share failed on main's missing CACHE line (fixed on main by 7b5d4ff20 after my merge point); native-ios-ui timed out. Isolated retry of native-ios-ui: Pro Max 117 passed, 1 failed; SE lease lost after 72 cases. The failing cases were checked one at a time against the merge base 8cecfc544 via rerun_tree.py. (1) InteractionTests testATapIsTooShortAndSendsNothing and testAPressWithTheMicrophoneOffShowsTheCardAndNotNowDismissesIt fail the same way at 8cecfc544: already on main, not this branch. (2) testBackgroundKeepsLockedVoiceAndReturnAllowsANewRecording passes alone on HEAD: a load flake. (3) AccessibilityLayoutTests testTheHeaderHoldsTheLongestLineWithTheKeyboardAndACard: passes at 8cecfc544 on Pro Max; fails at HEAD 3 out of 3 runs and at 311351116 (my merge, before any of my iOS commits). The app never leaves its launch logo at the largest text size, fixture conn-tailscale-off, light. A stack sample of the hung app shows an endless SwiftUI layout loop: ScreenView.body, bottomZone, EdgeCuedScroll and TranscriptView.updateUIView are re-run on every layout pass at 99% CPU. The only iOS app commit between them is the predecessor's 21af5639a (rich file on iPhone).

## Proceeding meanwhile

Bisecting to the exact change inside 21af5639a and fixing it on this branch with the failing case as the red/green test; then rerun native-ios-ui and the proof run with --reuse.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261022T231346Z-c7a9937f`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261022T231346Z-c7a9937f --disposition "<what you decided or did>"

## Correction (esc-20261022T232706Z-335cacd1)

The attribution above to 21af5639a is withdrawn. The same hang reproduced on main's own tree
8cecfc544 on a second run (iPhone 16 Pro Max, the fifth launch, evidence
/Volumes/E1TB/caches/richos-native-ios-ui/cfe92503c4/run.L0NBtB) and reverting 21af5639a's two
view-structure changes on HEAD did not stop it. The defect is main's; this branch does not change it.
