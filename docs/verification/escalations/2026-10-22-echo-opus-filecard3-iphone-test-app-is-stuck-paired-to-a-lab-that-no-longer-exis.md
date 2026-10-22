# Escalation: iPhone TEST app is stuck paired to a lab that no longer exists, with one waiting message; the file walk cannot pair the new lab without clearing the TEST app's data

- id: `esc-20261022T100411Z-04ee51ae`
- raised: 2026-10-22T10:04:11Z
- from: echo-opus-filecard3
- worktree: `/Users/alex/ab/richos-wt/echo-opus-filecard3` (branch `cc/echo-opus-filecard3`)
- head: `3808c9395cf83ce87205083ef817eb42c6f46724`
- state: **proceeding**
- for: lead

## The question

Which way out for the iPhone TEST app: (A) I build the missing exit in the app (a waiting message to a Mac that cannot be reached can be discarded or Forget offers Discard and forget), which is a RichConnect UI change; or (B) the CEO allows resetting the TEST app's saved data on the test iPhone (rios perf-seed's copy into the installed app), which section 100 otherwise forbids; or (C) leave the iPhone file walk and the relaunch case for after this land.

## What was already tried

Read the crash that stopped yesterday's walk: RichOSNative crashed 6 s after Send (repeated diffable id in TranscriptItem.build); fixed and committed d6f5b25e2 (test red then green, 45/45 app unit tests). Installed the Release TEST app from 3808c9395 on the iPhone and launched it: it opens on yesterday's conversation, paired to lab generation 4 (disabled yesterday by the predecessor), Reconnecting, with 'send me the final counter and the comps' Waiting to send (the crash ended the app before that send was accepted). Every exit is closed: Discard exists only for 'needs attention' rows (Rows.swift:339), Settings Forget is refused while a message waits and its dialog offers only 'Show the waiting messages' or 'Keep pairing' (Overlays.swift:113-117), a paired phone has no pairing-link control (only the start, scanner and camera-denied screens have one) and the app handles no pairing URL. A new lab (generation 5) cannot be reached by a phone that dials generation 4. Lab started, then stopped by its own PID and disabled again. Phone network checked first with rios device net: the internet is reached over the USB cable (Internet Sharing, en2).

## Proceeding meanwhile

VM walk (identity, first-run, rich-file) on a bundle and engine built from HEAD, then the proof run. The Android-style re-pair check is done in the core (PairingRemovedTests, green on unchanged code, committed 3808c9395); its on-phone half is part of this blocker.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261022T100411Z-04ee51ae`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261022T100411Z-04ee51ae --disposition "<what you decided or did>"
