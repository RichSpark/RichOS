# Dedicated Codex chat connections

Each Codex task gets a fresh connection to one permanent company/conversation ID.
Titles, the visible screen and later changes to the default never route messages.
One engine module, `richos/engine/scripts/lib/codex_channels.py`, owns discovery,
framing, reads and writes. The app uses its existing watcher and Stu intake.

Rich designates the future default through the host-scoped `codex_channel` tool,
only on the user's explicit request. Rich-first `connect` returns a handoff with
an absolute packaged-runtime command and locator for Codex. Codex-first `connect`
uses that default or copies the destination of an explicit existing locator.
Connect always creates a new pair. Attach to a handed-off pair with send/read.
The original task and source are saved as non-actionable `task_context` evidence.
No Codex native chat ID or per-chat process is required.

The CLI is `richos/engine/scripts/codex-channel.sh`. In an app handoff, use its
returned command, which names the shipped interpreter explicitly. The CLI also
honors `RICHOS_RUNTIME_PYTHON` in the normal engine environment.

```
connect --text 'Original user request' --provenance 'Transcript/source reference'
send --locator CONNECTION --id RECORDED_UNIQUE_ID --text 'Message'
read --locator CONNECTION
read --locator CONNECTION --direction to-rich
```

The scoped native tool exposes designate/connect/send/read. Endpoint IDs and
state roots come from the host. Send requires a recorded message ID. After an
uncertain response, read sent history and retry the same ID, text and reply-to.
Different contents for that ID refuse. A new ID deliberately creates another
message even if its text is identical. Replies use the incoming bridge locator
and message ID. Saved replies wait for Codex's normal read; they do not wake it.

Under the existing coordination root:

```
coordinator.json                         # future default only
coordinator.lock                         # orders designation/legacy cutover
receiver.lock                            # one managed app receiver
connections/COMPANY/CHAT/CONNECTION/
  connection.json                        # immutable endpoint
  to-rich.jsonl                          # incoming source/outbox
  to-codex.jsonl                         # outgoing replies
```

Writers flock one journal, repair only its incomplete unacknowledged tail,
append a full newline-terminated JSON frame and fsync before returning Saved.
New files/directories are synced. Text is at most 128 KiB and the encoded native
inbox frame remains at most 256 KiB. Readers never promote partial frames.
Short writes finish fully or fail without Saved. Missing journals are held rather
than recreated; symlinks and nonregular journal paths refuse. Readers hold a
nonblocking shared lock only while decoding each frame, then release it before
calling a consumer. Invalid complete frames hold that channel with a path diagnostic while healthy
channels progress at the next normal scan. No raw-append fallback is supported.

Native delivery re-resolves the ledger binding, then saves the normal observation
and bridge reference under the existing Stu inbox lock. It searches acknowledged
history too. Delivery reads also fence the writer and sync a bridge row before
Stu can act on a filesystem notification. Busy bridge writers hold only their own
chat. The inbox is synced before an advisory accepted position is saved
under `engine-state/codex-watch/accepted/`. Lost, stale or malformed positions
replay against the same receipt. Archive is navigation only; missing endpoints
hold rather than creating or redirecting a conversation. No reference-bearing
inbox history or source journal is pruned by this feature.

The existing automatic review scheduler calls the same framed decoder with its
own advisory position. Its existing READY handler uses task context and explicit
amendments as labelled original-source evidence. Only the app enumerates managed
app connections, so terminal and app review state cannot start competing reviews.
The terminal retains unmigrated Markdown handling and shares the handover parser.
The originating locator/READY ID is carried in existing pending/lock/attempt
records. Review results use the shared writer on that pair and an incoming
`review_result` observation labelled `review-watch`, never Codex or user authority.
A plain handover starts its first line with uppercase `READY TO LAND: codex/...`.
A `## ` heading also works. Both use the existing READY parser and gates;
ordinary status and acknowledgement prose does not start a review.
A failed result write reconciles by the same ID on the next look. Review policy,
quota controls and landing gates remain in the existing scheduler.

Before explicit designation, an existing unassociated legacy input file or
nonempty saved spool enables the fixed setup-only notice once per conversation.
Users without legacy Codex input receive no notice or setup model turn.
The app retains old Markdown observations in its existing spool.
Designation freezes each old endpoint and writes migration instructions to the
old reply file. The last legacy pass and designation share one lock; the existing
pipe drains in order before framed imports. The first frozen legacy endpoint owns the old unaddressed spool (one flag
on that endpoint, not a second registry). Later default/folder changes cannot
move it. The saved spool is retained and imported by deterministic source/offset IDs, checking exact text in every Stu
inbox including archived conversations. Imports are labelled uncertain legacy
observations. Unframed tails stay preserved, with one fixed-reference notice.
Every running Codex chat creates its own fresh pair against the frozen locator
and resends every unanswered request. Replies travel on that sender's new pair.
Changing the future default never retargets old work.

No designation or live migration is part of building or testing this feature.
The existing Stop/work/review controls determine execution and completion;
channel context and receipts supply evidence rather than authorization.
