# Escalation: iosloop2: native-ios-ui cannot finish inside proof-run's 2700 s cap; the freeze fix held, the suite's two devices run one after the other

- id: `esc-20261023T054014Z-e3d8437e`
- raised: 2026-10-23T05:40:14Z
- from: isaac-opus-iosloop2
- worktree: `/Users/alex/ab/richos-wt/isaac-opus-iosloop2` (branch `cc/isaac-opus-iosloop2`)
- head: `ca6914f8a4180e041ded5263a6f49299b250e587`
- state: **proceeding**
- for: lead

## The question

The briefed proof will very likely time out native-ios-ui again for the same reason. Accept device 1 of the lead's run 20261023T044649Z-3za63spa (124 cases, 0 failures, the new largest-text-size launch case passed in 49.7 s) as the native-ios-ui proof or raise that suite's time cap / run its two devices at once?

## What was already tried

Read the lead run's own work dir /Volumes/E1TB/caches/richos-native-ios-ui/37b5a4b315/run.MSJ0Jv. Device 1 (test-0.log): TEST EXECUTE SUCCEEDED, rc 0, 2234 s, 81 UI tests executed, 9 skipped, 0 failures; testTheConversationSettlesAtLaunchAtTheLargestSize passed (49.703 s). Device 2 (test-1.log) only started at 05:25Z, when device 1 finished, passed 6 cases at normal pace and was cut off at 05:31Z by the cap (BUILD INTERRUPTED mid-case). The app did not hang: no case failed or stalled. The line 'device N: 0 retained cases, 124 unresolved/control cases' is the pre-run reuse count, not a result. Cause: in native-ios-ui.test.sh lines 825-887 the per-device block is not backgrounded, so devices run in sequence and each runs the whole selection (shard-1.args = both bundles); 2 x ~2230 s is more than the 2700 s cap. The suite's own comment says 787-884 s per device on 2026-09-24. My own run cannot reuse device 1: the suite's cache key is a hash of the checkout path (mine 8af251269f, the lead's 37b5a4b315), so it starts with no previous work. Merge of main is committed (ca6914f8a); lint passes with main's load baseline.

## Proceeding meanwhile

Running the briefed proof-run once, unchanged and handing over its logs directory whatever it says.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261023T054014Z-e3d8437e`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261023T054014Z-e3d8437e --disposition "<what you decided or did>"
