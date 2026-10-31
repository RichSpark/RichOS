# RichOS streaming event contract (spine → UI)

The spine streams Rich's reply **live** to the web UI via **Tauri events** while a turn
runs. The `send_message` command still returns the final reconciled message list, but the
UI should render deltas from the events below so Rich's reply appears token-by-token and a
calm "Rich is working" state is shown.

This is the whole contract the UI needs — **no Rust reading required.** The Rust source of
truth is `richos/app/crates/richos-core/src/stream.rs` (event names as constants) +
`richos/app/src-tauri/src/main.rs` (`TauriEmitter`).

> **SIX FAMILIES and this document covers all six.** The five events below are the
> shipping calm view and are **unchanged**. `rich://machinery` (further down) is the
> opt-in technical family. [The additive live-work family](#the-additive-live-work-family-13)
> — seven typed §13 events — was added on 2026-08-29 and is what a Codex-style timeline
> renderer should be written against. [Voice mode](#voice-mode-richvoice-) is the fourth,
> emitted by a different crate while voice mode is running and
> [the staging desk](#the-staging-desk-richcorrection-staged) is the fifth, joined on
> 2026-08-30 by [the loro correction desk](#the-loro-correction-desk-richloro-proposed) —
> the same job, a different payload. **A UI that
> subscribes only to the events below keeps working exactly as it does today**, asserted
> byte-for-byte by `crates/richos-core/tests/live_event_tests.rs`. Clean output is
> guaranteed by the spine:
**only assistant-message text is ever emitted on the events below** — never tool calls,
worker chatter, shell or hook output.

*(Every count in that paragraph has been wrong at some point — "three families" and "six
typed events" until 2026-08-30, each written when it was true and left behind by the commit
that changed it, with the voice family never mentioned at all.
`richos/app/ui/tests/docs-claims.js` now joins this document's event names to the
`pub const … = "rich://…"` the Rust source declares and it earned its keep immediately: it
caught `rich://correction-staged` the same day the staging desk landed, before anybody had
to notice.)*

That guarantee is **structural, not a convention**: machinery is not a `StreamEvent` at
all. It is a second, separate family on its own event name — see
[Machinery: `rich://machinery`](#machinery-richmachinery) at the end of this document. **A
UI that does not subscribe to it cannot accidentally render machinery** and the default
calm view does not subscribe. Do not add it to the default subscription list.

## Events

All events are keyed to `threadId` + `turnId` so the UI can scope live state to the exact
turn (and ignore anything for a thread it isn't showing). Timestamps `at` are epoch millis.

| Event name | When | Payload |
|---|---|---|
| `rich://turn-started`   | Rich accepted the turn and started working. Show the "Rich is working" affordance. | `{ threadId, turnId, at }` |
| `rich://chunk`          | One assistant-text delta arrived. Append `textDelta` to the live bubble for `turnId`. | `{ threadId, turnId, seq, textDelta, at }` |
| `rich://turn-completed` | The turn ended cleanly. Finalize the bubble; clear the working state. | `{ threadId, turnId, stopReason, at }` |
| `rich://turn-error`     | The turn ended by error/interrupt (e.g. the compute lease died). Show a calm, Rich-voiced failure; clear the working state. | `{ threadId, turnId, reason, at }` |
| `rich://proactive-message` | **Rich spoke unprompted.** A proactive turn was written to the ledger atomically — there is no `turn-started` and there are no chunks. Re-read the thread (`get_messages` / the typed timeline); do not try to assemble it from a stream that never happened. | `{ threadId, turnId, tier, at }` |

`tier` is `interrupt_now | digest | silent` (`AttentionTier`, `ledger.rs`). §5.1 of the
front-end UX direction governs what each one may do: `interrupt_now` is rare and may notify;
`digest` is the normal batched message; **`silent` never appears in the conversation and
never notifies** — it exists in the ledger for a CEO who goes looking and there is no
activity view for it yet.

**This event was undocumented here until 2026-08-30**, while this page called itself the
whole contract the UI needs. It has been declared in `stream.rs` beside the other four since
the proactive seam landed and `richos/app/ui/main.js` has always subscribed to it. Nothing was
broken by the omission; a second UI written from this page alone would simply never have
heard Rich speak first.

### Ordering & guarantees

- For one turn the UI receives **exactly one** `rich://turn-started`, then **zero or more**
  `rich://chunk` events, then **exactly one** terminal event (`rich://turn-completed` **or**
  `rich://turn-error`) — never both. **A proactive turn is the one exception and is not a
  stream at all**: it is written atomically and announces itself with a single
  `rich://proactive-message`, with no started, chunk or terminal event of its own.
- `seq` is a **strictly increasing** per-turn counter starting at 0. Concatenating
  `textDelta` in `seq` order reproduces the full reply that the ledger holds for that turn
  — so a UI that missed the stream can fall back to `get_messages` (or `send_message`'s
  return) and get the identical text.
- **`seq` is NOT contiguous.** As of the machinery routing commit it is ONE counter per
  turn, shared with `rich://machinery`, so that the true order of "text, then a tool call,
  then more text" is reconstructible from a single sequence. If Rich runs a tool between
  two sentences you will see `seq` 3 then `seq` 7. **Never treat a gap as a lost chunk and
  never use `seq` as an array index.** Order by it; do not count with it.
- **`seq` is now durable and retroactivity is the same story as machinery's.** Each
  `rich://chunk`'s position is persisted with its delta (`AssistantDelta.seq`), so the
  interleaving of text and tool calls can be rebuilt after a restart instead of only being
  visible live. A delta written before that commit has **no** recorded position: the text
  is intact, its place in the turn is unknown and it is reported as unknown (`null`)
  rather than as position 0. A consumer rebuilding a past turn must expect an unpositioned
  run and must not assume it came first.
- `rich://turn-error` may still be preceded by `rich://chunk` events — a partial reply that
  streamed before the failure. Those partials are already durable in the ledger.
- Events are **best-effort from the spine's view**: the ledger, not the UI, is the source
  of truth. A UI that isn't listening never stalls or fails a turn.

## Consuming (web UI)

```js
const { listen } = window.__TAURI__.event;

// buffers keyed by turnId -> accumulated text
const live = new Map();

listen("rich://turn-started", ({ payload }) => {
  live.set(payload.turnId, "");
  showWorking(payload.threadId, true); // "Rich is working"
});

listen("rich://chunk", ({ payload }) => {
  const acc = (live.get(payload.turnId) || "") + payload.textDelta;
  live.set(payload.turnId, acc);
  renderLiveAssistant(payload.threadId, payload.turnId, acc); // token-by-token
});

listen("rich://turn-completed", ({ payload }) => {
  finalizeAssistant(payload.threadId, payload.turnId, live.get(payload.turnId));
  live.delete(payload.turnId);
  showWorking(payload.threadId, false);
});

listen("rich://turn-error", ({ payload }) => {
  showRichVoicedError(payload.threadId, payload.turnId); // never a stack trace
  live.delete(payload.turnId);
  showWorking(payload.threadId, false);
});
```

`send_message({ text, threadId })` still resolves with the final `Message[]` for the active thread;
treat it as the reconciled snapshot, not the primary render path.

---

## Machinery: `rich://machinery`

**The default conversation view does NOT subscribe to this event and must not.** It is a
separate family carrying every non-text frame the agent emits — tool calls, thinking,
permission requests, session metadata — for the opt-in technical view. Contract:
`richos-hq/docs/plans/richos-techy-mode-2026-08-26.md`; Rust source of truth:
`richos/app/crates/richos-core/src/machinery.rs`.

| Event name | When | Payload |
|---|---|---|
| `rich://machinery` | Rich's session emitted something that is not assistant text. | one `MachineryRecord` (below) |

```jsonc
{
  "machineryId": "mach_1f2e…",   // ours; the vendor gives none for thoughts
  "threadId": "thr_…",
  "turnId": "turn_…",            // null for re-prime AND between-turn traffic — see 3 and 6
  "sessionId": "…",              // which compute lease produced it
  "seq": 7,                      // THE ordering key, shared with rich://chunk
  "at": 1756425600000,           // a LABEL, never the ordering key
  "kind": "tool_call",           // tool_call | thought | permission_requested
                                 //   | client_fs_call | compaction | unknown
  "toolCallId": "toolu_…",       // the merge key: later updates MERGE into this row
  "status": "completed",         // pending | in_progress | completed | failed | (verbatim other)
  "title": "cat richos/engine/VERSION",
  "summary": "1.0.0",            // bounded, ≤84 chars
  "locations": ["/abs/path.rs"],
  "internal": false,             // true ⇒ NEVER render this in a thread view
  "payload": { },                // the raw agent frame; absent once the raw window expires
  "truncated": false
}
```

Five rules for whoever renders this:

1. **Order by `(turnId, seq)`. Never by `at`.** Millisecond timestamps collide inside a
   streaming turn.
2. **Merge by `toolCallId`, last-write-wins per field PRESENT.** A tool call arrives as
   several events: the first has a placeholder title (`"Terminal"`) and no arguments and
   most later ones carry no `status` at all. One `toolCallId` is ONE row, never two — and
   a whole-record overwrite would blank the status you already had.
   **`compaction` merges the same way and its key is NOT a vendor id**
   (`compaction:<sessionId>`): the wire gives a compaction's start, its 30-second heartbeats
   and its ending no correlation id at all, so the key is the lease's and the TURN scopes it.
   Two compactions in two turns are two rows.
3. **`internal: true` is never rendered in a thread.** It is re-prime and rotation
   machinery, retained for debugging. Rich never reveals that a rotation happened.
4. **`payload` may be absent** on an older record whose raw window has expired. The record
   still renders — title, status, paths, summary — and the raw pane should say so
   honestly rather than showing a blank.
5. **`kind: "unknown"` is not an error.** It is a kind that has no typed route yet; the
   vendor's own kind name is in `title`. Render it as one dim line.
6. **`turnId: null` on a NON-internal record is between-turn traffic** (techy-mode §1.5):
   what the session said at start-up or after a turn's answer had already gone back. It
   belongs to the THREAD and to no exchange in it, so it does NOT belong in the
   conversation stream — `get_machinery` puts it in `timeline.betweenTurns`, its own lane,
   and `richos/app/ui/` draws it as its own section below the conversation. Ordering inside that
   lane is JOURNAL ORDER, not `seq`: the lane's counter is per compute lease and restarts
   when one is replaced, so sorting by it would interleave two leases wrongly.

Rules 3 and 6 are one distinction, not two: `turnId: null` says only that the record has no
turn and `internal` is what says whether it may ever be seen.

**Retroactivity, stated honestly:** retention began at the routing commit. A thread that
ran before it has no machinery at all and the honest empty state is *"No machinery was
recorded for this conversation."* — not an error and not a blank pane. It is one of
**four** states and they are deliberately not one sentence: `recorded`,
`nothing_recorded`, `not_retained` (nothing on this install) and `unreadable` (the store
is there and the OS refused it). Serving the second over the fourth is the product lying
about its own record.

### The technical view reads this store; it does not subscribe to this event

**As of 2026-08-30 nothing in `richos/app/ui/` listens to `rich://machinery` and that is
deliberate.** The opt-in technical view (techy mode) is served by three commands, not by
this event:

| Command | Answers |
|---|---|
| `get_machinery({threadId})` | `{threadId, state, rowCount, sentence, reason, timeline}` — `timeline` is the SAME projection `get_timeline` returns, at `ViewMode::Technical`: same items, same ids, same `(turn, slot, sequence)` order, with each row's `detail` kept rather than removed. `state` is one of the four above; `sentence` is `null` when there is machinery. |
| `get_machinery_raw({threadId, machineryId})` | `{state, payload, truncated, note}` — §2.4's raw pane, one record at a time. `state` is `retained` \| `not_retained` \| `unreadable`; `note` is the sentence to show when there are no bytes. |
| `techy_mode` / `set_techy_mode` / `set_techy_default` | The per-thread toggle and the global default. `set_techy_mode(enabled: null)` CLEARS a pin. |

The reason to keep the reload path rather than subscribe: the sentence at the top of this
section — *"the default conversation view does NOT subscribe to this event and must
not"* — is a structural fact today and a renderer that subscribed conditionally would
make it a runtime branch. The cost is named: while a turn is running, its rows arrive
through the calm live family without their technical half and gain it when the turn ends.

---

## The additive live-work family (§13)

Added 2026-08-29 (UX brief §13, slice 3 of §24). **Strictly additive**: nothing above
changed — same names, same payload keys, same ordering, same shared `seq`. Rust source of
truth: `richos/app/crates/richos-core/src/live.rs` (event names as constants, payloads, the
gate); relay: `richos/app/src-tauri/src/events.rs`.

### What is live and what is deferred

§13 lists eleven events. **Seven are emitted. Four are not and are not emitted at all** —
not as an empty or "unknown" version, because for those the arrival of the event would
itself be the claim. §22: *"If the source signal does not exist, build the signal first or
show unknown."*

**An eighth is emitted and is not one of §13's**: `rich://ceo-message`, added 2026-09-19.
Every event §13 names is about RICH; the CEO's own message reached a surface only by being
projected, which was enough while the one surface was a webview that had drawn his sentence
itself the moment he pressed Send. A second surface makes that false — the phone (a web page then, the RichConnect apps
since 2026-10-30) subscribes to this family and learned nothing, live, about a message typed on the Mac
(`esc-20260919T003541Z-6885f74b`). It is listed with the seven rather than in a section of
its own, because it is the same family, the same fence and the same gate.

| Event | Status | Payload / why not |
|---|---|---|
| `rich://turn-status` | **LIVE** | `{…fence, status, startedAt, activeDurationMs, supersedesTurnId?, visibility, at}` |
| `rich://ceo-message` | **LIVE** (2026-09-19) | `{…fence, messageId, text, source, createdAt, visibility, at}`. Not a §13 event — see above. `messageId` is `{turnId}:user` and `createdAt` is the turn's own instant: both are exactly what `timeline.rs` derives for `TimelineItem::UserMessage`, so a consumer that merges on `id` sees the live row and a reloaded row as ONE row. `source` is `text` or `jam`, the same spelling the projection uses. Emitted once per CEO turn, at the instant his message is durable — before the turn starts. A proactive turn has no CEO text and emits none. |
| `rich://message-started` | **LIVE** | `{…fence, messageId, phase, seq, visibility, at}` |
| `rich://message-delta` | **LIVE** | `{…fence, messageId, seq, textDelta, visibility, at}` |
| `rich://message-completed` | **LIVE** | `{…fence, messageId, phase, text, visibility, at}` |
| `rich://activity-upserted` | **LIVE** | one timeline activity record + `at` (below) |
| `rich://thread-summary-updated` | **LIVE** | `{…fence, title, messageCount, lastActivity, status, visibility, at}` |
| `rich://worker-upserted` | **LIVE** (2026-08-29) | one timeline `worker_activity` record + `at` (below) |
| `rich://plan-updated` | **DEFERRED** | `plan` updates are retained as untyped machinery; their entries live **only** in the evictable Tier-B raw payload, so a plan projected from them would silently empty out after the retention window. |
| `rich://approval-resolved` | **DEFERRED** | Same. |
| `rich://artifact-upserted` | **DEFERRED** | Checked, not assumed: nothing in the ledger, the journal or machinery records an output as a deliverable. Artifacts and source provenance are Phase 5. |

### The fence — on every payload and what to do with it

Every event carries `entityId`, `threadId`, `turnId` and `bindingRevision`. §13: *"The
renderer rejects events that do not match the immutable binding."* Concretely:

- **Equality is `entityId` + `threadId`.** Those are immutable for the life of a thread.
- **`bindingRevision` is a STALENESS fence, never an equality key.** It is the revision of
  the *activation* that produced the event, so it advances on every thread activation and
  is legitimately **higher** than the revision a re-projection of the same thread reports.
  Comparing it for equality would make you reject every live event after any thread
  switch. Reject anything **older** than your current activation; accept at or above.
- A payload can never be built from a loose thread id: the fence's only constructor takes
  a binding the ledger issued and it is private to richos-core.

### `visibility` — and why you cannot leak from this family

Every payload carries `visibility` and on this family **it is always `"ceo"`** — because
the spine refuses to emit anything else here. This is the *calm* family:

- **Internal** items (re-prime, rotation, model reasoning, a Tier-3 silent proactive
  message) reach no family at all.
- **Technical** detail is not hidden here, it is **absent**. Exact commands, output
  previews and file paths travel on `rich://machinery`, which the calm view does not
  subscribe to. An `activity-upserted` payload has no `detail` object — the bytes were
  removed before the event was constructed, so a renderer of this family cannot show a raw
  command it was never handed.

Keep carrying the field. Do not default it.

### `phase` — **it may be `"unknown"` and today it always is for streamed replies**

```jsonc
{ "messageId": "turn_37…:text:0", "phase": "unknown", … }
```

`phase` is one of `"commentary" | "final" | "proactive" | "recovery" | "unknown"`. **Today
every streamed message is `"unknown"` and a renderer must not treat that as "final".**

This is measured, not assumed and it was re-measured when the ACP adapter was deleted
(`wiki/ceo-decisions.md` §16) rather than assumed to have survived the move.
`docs/verification/acp-emission-probe-2026-08-28.md` §2 is the complete union of inbound ACP
traffic across five runs: 52 `agent_message_chunk`s and **zero** message-open, message-close
or role updates. The native binary's wire (`.../native-claude-stream-json-2026-08-31/`) DOES
carry `message_start` and `message_stop` — but they bracket a message and say nothing about
what KIND of message it is, so the answer is unchanged: nothing on either wire separates
Rich's thinking-out-loud from his answer. And `rich://message-started` fires the instant the first
delta is persisted — before the turn is over — so even a perfect after-the-fact rule would
be unavailable at emission time.

The tempting fallback is also simply false. *"The last run of a completed turn is the final
answer"* breaks the moment Rich verifies something after writing his conclusion, which
makes the last run a two-word `"Confirmed."` and the deliverable the run before it. **The
CEO reads the final response as the deliverable, so labelling commentary as final is a
product defect, not a cosmetic one.**

**What a renderer should do:** render an `"unknown"` message as Rich's prose in sequence
order — no "final answer" treatment, no distinct final-response styling, no
collapse-the-commentary behavior — until a real phase signal exists. `phase` is always
present in the JSON precisely so nobody writes `phase ?? "final"`.

**One phase is real**: a proactive message (Rich speaking unprompted) is emitted as
`"proactive"`, because the ledger records it as one. `"commentary"`, `"final"` and
`"recovery"` are currently unreachable values.

### Messages: ids, runs and what closes one

A turn's prose is **one message per contiguous run of the shared `seq` counter** — a run
ends wherever a tool call took the positions in between. A turn where Rich talks, works,
then talks again produces two messages, not one.

- `messageId` is `{turnId}:text:{runIndex}` — **the same id a reload projects from the
  ledger.** A cold reopen re-states what you already rendered; it does not duplicate it
  (§13: *"repeated event IDs are idempotent"*).
- `seq` on `message-started` is the run's first position in the shared counter or `null`
  when the position was never recorded. **Never zero by default.**
- A message is closed by `message-completed` when the tool call starts, not at turn end —
  so *"commentary, then activity"* is live-accurate.
- `message-completed` carries the run's **full text**. A consumer that missed every delta
  is still correct.
- A turn that fails mid-stream still closes its open message: the partial text is already
  durable.

### `turn-status`: the states that exist and the four that do not

`status` is one of `queued | working | recovering | completed | failed | stopped`.

§11 names ten states. Four are **never emitted** because nothing can produce them:
`draft` (UI state before a durable turn), `streaming_final` (needs the phase signal that
does not exist), `waiting_for_user` (§9.4 has no signal — see below) and `stopping`. Do
not write branches for them yet.

**`stopped` joined this list on 2026-08-29 (slice 6).** It is emitted only after
`Ledger::stop_turn`, which is itself reachable only from a stop request that was fsync'd to
the intake log *before* the lease was cancelled (`steering.rs`). There is no path from a
crash to this status, which is what makes §6.1's `You stopped after {duration}` — an
attribution to the CEO — safe to render. A crash or a rotation still produces
`work_duration state: "interrupted"` on a reload and **which one it was is still not
recorded**; the only thing that state can now say for certain is that it was not the CEO.

The renderer sends `stop_turn({ expectedThreadId })` for the conversation it displays.
The backend rejects a different active conversation, then its existing control atomically
stops the current input owner. The renderer does not guess that owner from live rows:
Claude Code can fold a follow-up into the running answer. The returned `turnId` identifies
the row marked Stopping. Explicit callers may still supply `expectedTurnId`; that guard
rejects an obsolete input identity. A terminated provider is retired even if its completed
answer beat Stop. The next explicit input uses the normal native history resume; Stop
never opens a replacement or replays a request by itself.

**`stopping` is a UI-LOCAL state and is deliberately not on the wire.** The `stop_turn`
command does not answer until the request is durable, so the renderer setting `stopping`
from that return is a statement of recorded fact rather than an optimistic guess. The
control persists Stop without taking the conversation spine. Native reply waits also run
outside that lock. The actual terminal result determines Stopped versus Completed.

**`waiting_for_user` (§9.4) STILL HAS NO SIGNAL and slice 6 did not invent one.** §22 lists
worker waiting state under "must not be faked", `worker_status.rs` refuses to infer it and
`TeammateIdle` cannot distinguish "paused for input" from "finished for good". It is not
merely unimplemented: implementing it would also break §6.3's timer, which pauses in that
state — `Turn::active_ms` is `ended_at - started_at` and is exact today precisely *because*
there is no pause to exclude. Whoever lands the signal must replace that measure with
accumulated active time, not extend it.

- `activeDurationMs` is the **measured** span (`ended_at - started_at`) and is explicitly
  `null` until the turn ends. It is never `now() - startedAt`, because that turns a
  five-minute task into a twelve-hour one after an overnight restart. Use `startedAt` to
  tick a live timer; use `activeDurationMs` for the frozen number.
- A proactive turn is written atomically, so it emits only `completed`, with
  `startedAt: null` and `activeDurationMs: null` — there was no delivery to time.

**Mid-turn crash — the one place this contract goes beyond §13.** When the compute lease
dies mid-turn and an automatic replay is possible, the crashed turn emits `recovering`
(not `failed`: it is about to be superseded and a reload will not render it at all) and
the replacement turn's `queued` carries **`supersedesTurnId`**. That field is a **merge
instruction, not an announcement** — merge the two turn ids into one exchange. Without it
you would draw the CEO's single prompt twice. Do not surface it as "we reconnected"; §21's
calm recovery wording is a product decision that has not been made. When no replay is
possible, the turn emits `failed` and stays failed.

**A clean, watermark-triggered session rotation emits nothing on any family.** It happens
at a turn boundary and is invisible by construction.

### `activity-upserted`: upsert semantics

The payload **is** the timeline activity record (`kind: "activity"`, fence flattened into
it) plus `at`. Merge by `id`, last write wins — one tool call is ONE row that arrives
several times as it moves `queued → completed`. Order by `(turnId, sequence)`, never `at`.

- `state` is `queued | running | completed | stopped | failed | unknown`. **`unknown` is
  common and is not an error**: 34 of the 58 tool events measured on 2026-08-28 carried no
  `status` field at all. It is not "completed". `stopped` has no source and never appears.
- `summary` is a semantic line built from the activity type alone — *"Ran a command"*,
  *"Read 8 files"*. Never the command, never the output. Rolling up *"Read 8 files"* across
  several rows is the renderer's job.
- **A COMPACTION IS AN ACTIVITY ROW and it is the one `system` frame that reaches this
  family.** The child pauses mid-turn to compact its own context — measured 38.1-62.0s over 16
  boundaries — and says so on `system/status` with `status: "compacting"`, repeating every
  30.000s until it ends with a `compact_result`. Its row reads *"Making room to keep going"*
  while it runs, then *"Made room to keep going"* or *"Kept going without making room"* and
  both endings are `state: "completed"` because what ended is the PAUSE. `activityType` is
  `other`: none of section 12's types fits and the words come from the kind instead. Measured
  and derived in `docs/verification/compaction-notice-2026-09-06/`.
- `sequence` is the OPENING record's position in the shared per-turn counter, so an
  activity row interleaves correctly with `message-delta`s ordered by the same counter.
  **Gaps are normal** — see the `seq` rule above.
- `completedAt` is set only when the row genuinely reached a terminal state.

**Accounting updates are not activity.** `usage_update`, `available_commands_update` and
`session_info_update` are untyped vendor kinds; as of 2026-08-29 they are **technical**, so
they do not appear on this family at all. They previously projected as CEO rows reading
*"Worked"* — one live turn produced six of them against one real command. They are still
routed, retained and rendered in technical mode with their vendor kind.

### `worker-upserted`: the delegation you see while it is happening

Added 2026-08-29. It was deferred while there was no worker lifecycle signal anywhere;
the engine's four emitters now write `worker-events.jsonl`
(`richos/engine/docs/worker-lifecycle-events.md`) and a `Task` tool call is joined to it **by
identity**. Before this the join ran only inside `get_timeline`, so a delegation reached
the screen after a snapshot read and showed as a nameless *"Worked"* row for the rest of
the turn. The §26 fixture measured the gap: **0 chips live, 3 after the snapshot.**

The payload **is** the `worker_activity` timeline item a reload projects (fence flattened
into it, `detail` already removed) plus `at`, so:

- **Merge by `id`, last write wins** — the same upsert as `activity-upserted`. The id is
  the machinery id, stable across re-projection, so a repeated id is idempotent and a cold
  reopen re-states the row instead of drawing a second one.
- **One delegated run is ONE row**, however many lifecycle events it produced. `created →
  started → updated → run_ended` all arrive under the same id.
- `worker.agentId` is the join key and **is not globally unique across sessions**. The
  emitter admits a lifecycle row only when its `session_id` matches the tool call's; a
  renderer never has to think about it, but never key anything on `agentId` alone either.
- **A row may arrive as `activity` first and become `worker_activity` later, under the
  same id.** The `agentId` is extractable only from the tool result and the engine's
  `created` hook writes at about that same instant in another process — so a delegation
  whose lifecycle row lands late is drawn as an ordinary activity row and upgraded at the
  next observation. **Replace the item on a kind change; do not merge over it** or the
  activity row's `summary` / `state` / `activityType` cling to a worker row where they mean
  something else.

`worker.state` is `pending_init | running | unknown` and **nothing else in practice**:

- `run_ended` crosses the wire as **`unknown`**, never `completed`. The reason a run ended
  is not observable anywhere in the hook set and `unknown` is the honest superset of
  completed, interrupted and failed. Render it as *Ended · outcome not recorded*.
- `waiting`, `interrupted`, `failed` and `completed` are in the type because they are §12's
  vocabulary. **Nothing constructs them.** If one ever arrives, it is a new signal, not a
  default.
- There is no `completedAt`, no `resultRef`, no `errorRef` and **no duration**: §22 forbids
  faking elapsed active time and the wall-clock spread between the first and last
  lifecycle rows is not active time.

**THE ONE STALENESS LIMIT, STATED.** This event is emitted when machinery arrives and once
more at the turn's end — there is no poll and no timer. A worker's state changes through
hook writes that produce no agent traffic at all, so between two tool calls a chip can be up
to one tool call behind. It is never wrong about a worker that was never witnessed and the
turn-end emission means the last live row is the row an immediate reload projects.

### `thread-summary-updated`

`status` is one of `idle | queued | working | failed`, computed exactly as
`thread::summaries` computes the sidebar, so a live row and a re-listed row cannot
disagree. §3.2's `waiting for CEO`, `completed while away` and `archived` are **not
emitted**: the first has no state to be in, the second needs a per-thread seen marker that
does not exist and the third needs an archive that does not exist. Emitted at turn
transitions only, so it always carries a real `turnId`.

### Ordering and durability

- **Every event is emitted after the durable write that makes it true** and several read
  their content back out of the ledger rather than from the value in hand. What is on the
  wire is what survived, not what was intended.
- Sequences are monotonic within one item stream; repeated ids are idempotent.
- **Missed events never block the spine.** The ledger is the source of truth; a webview
  that is not listening never stalls or fails a turn. On reconnect, take a snapshot
  (`get_messages` and the typed timeline) and resume — do not replay from memory.

### Seeing it for real

```
cargo run -p richos-core --example live_events_roundtrip -- <engine_dir> "your message"
```

Prints both families side by side against a real turn — `old>` for the calm-view events at
the top of this document, `NEW>` for the seven here, payloads verbatim. Needs Claude Code
installed and signed in; no npm, no adapter.

---

## Voice mode: `rich://voice-*`

The fourth family and the only one emitted by a different crate:
`richos/app/crates/richos-voice/src/event.rs` declares the names and payloads;
`TauriVoiceEmitter` in `richos/app/src-tauri/src/main.rs` relays them to the webview verbatim.

`rich://voice-model` is the one exception to that sentence and it is worth saying where: it is
declared in `crates/richos-voice/src/provision.rs` rather than `event.rs`, because it reports on
getting the speech model onto the machine rather than on the audio pipeline and it is emitted
through `voice_provision.rs`'s `ModelObserver` rather than through `VoiceObserver`. Same `rich://`
convention, same camelCase payload, different subject.

| Event name | When | Payload |
|---|---|---|
| `rich://voice-state` | The mic state changed, a new input level is available or the input went silent / came back. | `{ state, level, bargeInArmed, noAudio, at }` |
| `rich://voice-transcript` | An utterance was recognized and **already submitted to the spine as a turn**. | `{ text, durationMs, latencyMs, at }` |
| `rich://voice-error` | Voice mode could not start or had to stop. | `{ message, at }` |
| `rich://voice-model` | Progress and outcome of getting the speech model this machine needs. `phase` is one of `started`, `progress`, `verifying`, `installed`, `failed`. | `{ phase, modelId, received, total, totalLabel, message, askAgain, at }` |
| `rich://voice-notice` | Voice mode is **running**, but not the way it would on a better machine — today, that it started on a lighter recognizer than the best one, because the more accurate one is too slow here, there is not enough free memory for it or the machine's speed could not be measured (`hardware.rs`, `Resolution::ceo_message`). Emitted **once**, at voice-mode start and only when the product made that choice on the CEO's behalf. | `{ message, at }` |

Speech decoder initialization runs silently ahead of capture. Starting capture joins
that initialization before opening the microphone, so the first transcript does not pay
the packaged decoder's cold startup. A version probe or cached hardware timing is not
initialization. See [reply startup and verification](../../docs/development/spoken-reply-latency.md)
and use `voice-walk.py --expect-warmed-decoder` for the actual application proof.

Seven things a renderer of this family has to know, all of them structural rather than
stylistic:

1. **`state` is the mic's real state, never an optimistic one.** It is one of
   `off | listening | hearing | thinking | speaking` and the panel is driven only by this
   event — pressing the toggle does not turn the dot on, the device reporting open does.
2. **`noAudio` is a FIELD of the state event, not an error.** It is the post-open silent
   input verdict: the stream is open and healthy but has delivered nothing above -80.00 dBFS
   for 3.008 s. It is not fatal and it clears by itself the moment audio returns.
3. **A transcript is not a request to send anything.** By the time it arrives the words are
   already a turn on the same thread, through the same `Spine::submit_prompt` typed text
   uses — so voice and text are one durable conversation and the reply arrives on the calm
   family above like any other. `durationMs` and `latencyMs` are measured, never estimated.
4. **`message` on `voice-error` and on `voice-notice` is always a calm, Rich-voiced line.**
   Device names, exit codes and file paths go to stderr and never to the CEO.
5. **`voice-notice` is not a quieter `voice-error` and the two must not share a handler.**
   They carry the same payload shape and mean opposite things: an error says voice stopped,
   a notice says voice works and has made a choice for him. The `voice-error` handler takes
   the panel out of voice mode, so a notice routed there would end the conversation it is
   reporting on. And unlike the other three, a notice is **not** dropped when the voice panel
   is closed — it is a sentence in the thread about a decision that holds for the whole
   session. It is silent when the machine got the best recognizer on the ladder and when an engineer named the model outright.

6. **`verifying` is its own phase and is not cosmetic.** The transfer is finished and the sha256
   is being taken over 487,614,201 bytes, which costs about a second per 500 MB. A bar sitting at
   100% under the download's own words is where a person decides the app has hung.
7. **`askAgain` decides whether a control appears and it is NOT "will the backend retry".**
   Those are different questions and conflating them shipped an instruction with no control: a
   captive portal must never be retried automatically (the next attempt fetches the same login
   page) and the CEO absolutely must be able to ask again once he has signed in — which
   `message` tells him to do in as many words. `Finding::worth_asking_again` answers this field;
   `Finding::retryable` answers the internal loop and never reaches the webview. Like
   `voice-notice`, this event is **not** dropped when the voice panel is closed: a download he
   started and tabbed away from is still running.

There is **no live partial transcript**: `whisper-cli` has no partial-hypothesis stream, so
nothing arrives between the start of an utterance and its finished text. That is a stated
deviation from the UX direction §4.1 sketch, not an omission here.

---

## The staging desk: `rich://correction-staged`

The fifth family, added 2026-08-30 with the spoken-correction trigger. Source of truth:
`richos/app/crates/richos-core/src/staging.rs` (the constant, the `CorrectionObserver` sink and
the payload); the relay is in `richos/app/src-tauri/src/main.rs`.

| Event name | When | Payload |
|---|---|---|
| `rich://correction-staged` | The CEO said something that looked like a correction and it has been written down as a QUESTION. | one `Staged`: `{ candidates: [...], withheld: [...] }` |

It is deliberately **not** a §13 event. §13 lists eleven, this is none of them and a
subscription list is the proof of what a surface carries — inventing a twelfth would
misrepresent the brief.

Four rules for whoever renders it, all of them the ruling rather than taste:

1. **Nothing has been learned.** `ceo-decisions.md` §7: *"Nothing is ever learned
   silently."* This event announces a question and `confirm` is the only path to a
   vocabulary write. Render it as an ask — never as a notification that something changed.
2. **Ask it in the words you were given.** Each candidate carries a built `prompt`
   (*"Add "X" to your vocabulary?"* or *"… (you corrected this before)"* once
   `declinedBefore > 0`), so every surface asks identically and a second ask does not read
   as amnesia.
3. **A candidate carries its own `threadId` and `turnId`, captured at STAGE time.** Do not
   re-derive them when he answers: the active thread can move while a question waits and a
   candidate re-scoped to wherever he happens to be looking has laundered itself across an
   entity boundary.
4. **`withheld` is not an error and is not a candidate.** It is the pairs the gate refused,
   with `from`, `to` and a `reason`. It exists so the refusal is inspectable; it must not be
   rendered as a question the CEO can answer.

The payload is camelCase like every other family. A candidate is
`{ key, at, threadId, turnId, utterance, ask, declinedBefore, prompt }`, where `ask` carries
the detector's own reading — `from`, `to`, `frame`, the two similarity legs, which `leg` let
it through and an `anchor` that is **evidence and never a gate** (`null` simply means there
is nothing to quote).

**Missing this event costs a prompt, never a record.** The question is already durable on
disk before the observer is called, so a UI that is not listening loses the ask and not the
candidate.

---

## The loro correction desk: `rich://loro-proposed`

The sixth family, added 2026-08-30 with the belief-correction trigger (`belief.rs`). Source
of truth: `richos/app/crates/richos-core/src/correction.rs` (the constant, the `ProposalObserver`
sink and the payload); the relay is in `richos/app/src-tauri/src/main.rs`.

| Event name | When | Payload |
|---|---|---|
| `rich://loro-proposed` | The CEO said a RECORDED BELIEF is wrong, it resolved to exactly one record and a proposal against that record is now awaiting him. | one `Proposal` |

**A separate name from `rich://correction-staged`, on purpose.** The two announce the same
KIND of thing — a question the CEO has not answered — and carry different payloads and a
subscription list is the proof of what a surface renders. A renderer that wants both
subscribes to both; one that renders only the vocabulary ask is unaffected by this existing.

Four rules for whoever renders it, all of them the ruling rather than taste:

1. **Nothing has been written.** `CorrectionDesk::propose` runs the loro writer with
   `--dry-run` and stores what WOULD be written; `confirm` is the only path to a write and
   there is no argument any caller can pass to skip it (`ceo-decisions.md` §7).
2. **Show `preview` verbatim, in a `<pre>`.** It is the WRITER's own `--dry-run` bytes. A
   preview generated by anything else would be a description of a write rather than the
   write and it is what the CEO is approving.
3. **Name the record.** `write.recordRef` is what would be superseded and the whole
   difference between a correction and a corruption is that it is the right one. Render it.
4. **`why` is the CEO's own sentence, verbatim** — the detector composes nothing. It is also
   the body of the superseding record, so a surface that paraphrases it is paraphrasing what
   is about to land in a file.

The payload is a `Proposal`: `{ id, at, entity_id, thread_id, write, why, preview, state,
outcome, failure }`, where `write` is the tagged `ProposedWrite` union
(`{op: "supersede", recordRef, newId, kind, scope, body}` for everything this trigger files).

**Missing this event costs a badge move, never a record.** The proposal is durable on the
desk's own JSONL, fsynced, before the observer is called.

## The update path: `rich://update`

The seventh family, added 2026-08-31 with the updater (RICH-TODOs row 12). Source of truth:
`richos/app/src-tauri/src/updates.rs` — the constant `EVENT_UPDATE`, the `UpdateView` payload and
the single `transition()` chokepoint every state change in that file goes through.

| Event name | When | Payload |
|---|---|---|
| `rich://update` | The update state changed: a check started or finished, a download moved, the signature was verified, an install landed or any of it failed. | one `UpdateView` |

**Emitted from the automatic launch check as well as from the buttons**, which is the point
of it being an event at all: RichOS checks by itself three seconds after launch and the mark
on the settings button has to appear whether or not anyone had a menu open.

`UpdateView` is flat rather than a tagged union, deliberately, because it is ALSO the return
value of the `update_state` command — a surface that attaches late or reconnects mid-download,
paints correctly from one read:

```
{ state, currentVersion, availableVersion, notes, pubDate,
  downloadedBytes, totalBytes, percent, failure,
  endpoint, endpointIsPlaceholder, checkedAt }
```

`state` is one of `unconfigured`, `idle`, `checking`, `upToDate`, `available`, `downloading`,
`installing`, `ready`, `failed`.

Four rules for whoever renders it:

1. **`unconfigured` is not an error and not "up to date".** It means no update server has
   been chosen and rendering that as a network failure would report a decision as a defect.
   Since 2026-09-04 the shipped `plugins.updater` endpoint is a real one — GitHub Releases,
   `richos/app/UPDATES.md` — so a shipping build no longer reaches this state; a build pointed at
   the RFC 2606 `updates.richos.invalid` host through `RICHOS_UPDATE_ENDPOINT` still does,
   and the state stays for it.
2. **`totalBytes: null` means DO NOT DRAW A PERCENTAGE.** The server sent no `Content-Length`.
   A determinate bar over an invented denominator is a lie that looks like a measurement;
   show bytes or an indeterminate bar with no `aria-valuenow`.
3. **`failure.kind === "signature"` MUST NOT BE OFFERED A RETRY.** It means the bytes on the
   wire are not the bytes we signed. Every other kind (`offline`, `network`, `manifest`,
   `install`, `configuration`, `other`) is plausibly transient and may be retried; this one
   refuses identically every time and a button that invites someone to keep pressing until a
   security check passes is the wrong control.
4. **`failure.headline` is for the CEO and `failure.detail` is the vendor's own error text.**
   Show the headline; keep the detail one click away rather than dropping it. An error that
   hides its cause turns a five-minute diagnosis into a week.

**Missing this event costs a repaint, never a state.** The truth lives in the Rust-side
`Updates` state and is readable at any time with `update_state`; the emit is `let _ =`, the
same best-effort posture every other emitter in this shell takes.

**The webview holds no updater permission.** These four commands — `update_state`,
`update_check`, `update_install`, `update_relaunch` — are this app's own and
`capabilities/default.json` grants no `plugin:updater|*` command to the page. A frontend
cannot start a download or an install by itself.

## Background work: `rich://work-notice`

The eighth family, added 2026-09-17 with the work lease (the background-work spec, revision 5,
in the private richos-hq record; §3.4 and §3.8). Source of truth:
`richos/app/src-tauri/src/main.rs` — the constant `EVENT_WORK_NOTICE` and the `WorkNotice`
observer the work host is given.

| Event name | When | Payload |
|---|---|---|
| `rich://work-notice` | A background assignment has something to say: it is ready for him to approve, it finished, it failed or it was stopped. Raised by the WORK lease, outside any conversation turn. | `{ threadId, notice: { assignmentId, title, kind, text, raisedAtMs } }` |

`kind` is one of `ready-to-approve`, `settled`, `failed`, `interrupted`.

**This event is the SECOND-best half of the return path and the document says so because a
renderer that treated it as the whole of it would drop results.** The durable half is the
notice held on the assignment's own record until it has been delivered, read with
`take_work_notices` — which both reads AND marks. That is what makes a result survive him
closing the conversation, switching threads or quitting and coming back; this event only
covers the case where he is already looking.

Three rules for whoever renders it:

1. **NEVER MID-SENTENCE.** A notice arriving while a turn is running or while a voice
   exchange is in progress, is HELD and said at the boundary. There are two ways to be
   mid-sentence, so there are two flushes: the turn's terminal event and voice going quiet.
   Nothing times out and nothing is discarded — `take_work_notices` would hand the same
   notice over again if the held copy were lost, because the durable flag is only set when
   the backend hands it out.
2. **NEVER "done" FOR SOMETHING NOT WITNESSED FINISHING.** The `ready-to-approve` kind keeps
   its wire name and has a narrower meaning since the CEO's ruling §52 (2026-09-18): a job
   lands on its own, so this is no longer an assignment held at its land — it is one waiting on
   a decision of his on some step the desk could not put in front of him (a command, a write
   outside its workspace). It is not a completion and must never be rendered as one. Neither is
   `failed`, which since the same ruling also covers a job that ran and did not land. Only
   `settled` is a completion and its sentence carries what landed.
3. **It is an attributed local line, never Rich's own turn text.** It renders through the same
   path `rich://voice-notice` uses — a local notice with a synthetic turn id and no turn
   record — so it can never grow a duration row claiming work that never happened and the
   next snapshot drops it. The record it came from is the evidence; the line is not.

**Missing this event costs a moment, never a message.** The notice is on disk before the emit
is attempted and the surface reads the durable store when the conversation opens and again at
every turn boundary.

**Stu's reports enter the ordinary conversation.** The host accepts each peer message
through durable intake with agent provenance and a stable delivery key. Rich receives it
on his normal conversation connection and decides what to tell the user. His authored
reply follows the ordinary turn, streaming and voice paths. There is no separate result
writer or work-result speech event. Opening a conversation or restarting the app retains
accepted input and pending peer messages.

## Quitting while work runs: `rich://quit-question`

The ninth family, added 2026-09-17 with the window-closed process model (the background-work
spec, revision 5, §2.5 and §2.5a; acceptance §7.4a). Source of truth:
`richos/app/src-tauri/src/main.rs` — the constant `EVENT_QUIT_QUESTION` — with the decision
and both sentences in `richos/app/src-tauri/src/lifecycle.rs`.

| Event name | When | Payload |
|---|---|---|
| `rich://quit-question` | He chose Quit while an assignment is running or waiting for him. The exit has ALREADY been prevented; this asks him what to do about it. | `{ say, quit, stay }` — the question and the two control labels, all three in his own terms |

**The order is the whole of it and it is not a preference.** The runtime reads the answer to
`ExitRequested` synchronously — `let recv = rx.try_recv();`
(`tauri-runtime-wry-2.11.4/src/lib.rs:4318`, `:4361`) — so an empty channel is not `Prevent`.
A handler that waited for this question to be answered would have quit before it was asked.
So the shell prevents first, returns and emits this afterwards, from its own thread.

Two rules for whoever renders it:

1. **The safe answer is the default.** "Keep working" takes focus and Escape answers it. The
   destructive control is never the one under the return key.
2. **Answer it; never dismiss it.** The exit is already prevented, so a sheet closed without
   an answer would leave him pressing Quit against an app that will not go away and says
   nothing about why. Both answers are commands: `cancel_quit` and `confirm_quit_and_stop`.

**Closing the WINDOW raises nothing here, which is the point of the whole change.** With work
registered, the last window closing keeps the process alive with no window and no question —
the CEO's decision, 2026-09-17: *"OK, window-closed only."* The Dock icon brings the window
back. With nothing registered, closing the last window still quits exactly as it always has.

## Files dropped on the window: `rich://file-drag` and `rich://file-drop`

The tenth family, added 2026-09-24 for screenshots and files on the Mac composer (CEO ruling
§86). Source of truth: `richos/app/src-tauri/src/mac_attachments.rs`, the constants
`EVENT_FILE_DRAG` and `EVENT_FILE_DROP`, emitted by `listen_for_drops` from the window's own
drag-and-drop events.

| Event name | When | Payload |
|---|---|---|
| `rich://file-drag` | A drag carrying files entered the window (`enter`) or left it or landed (`leave`). | `{ phase, count }`: `phase` is `enter` or `leave`; `count` is how many paths the drag carries, 0 on `leave` |
| `rich://file-drop` | Files landed on the window. | `{ drop, files: [{ index, name }] }`: a drop number and each file's display name, **never its path** |

**The page never receives a path and cannot name one.** The window's drag-and-drop handler
takes the drop before the webview sees it, so the paths arrive in Rust. They are kept there,
per drop and the page asks for a file by `(drop, index)` with `attach_dropped_file`; each path
is handed out once. No command takes a path from the page. A paste is the other way in: the
page already holds the bytes and `attach_pasted_file` takes them as a raw body.

**What the page does with them** is `ui/attachments.js`: a chip per file on the composer, a
remove control per chip and on Send `commit_attachments`, which moves the files into the
conversation's folder and returns the words Rich receives, in the same shape the phone's
attachments reach him (`phone/attachments.rs`, `describe_from`).

## A thread's output record: `rich://output`

The eleventh family, added 2026-10-05 for the Output side panel (its PRD of that date, §6.6, is
in the private record, not in this repository). Source of truth:
`richos/app/crates/richos-core/src/output.rs`, the constant `EVENT_OUTPUT`, emitted by the shell's
`TauriOutputEmitter` from the ONE `OutputStore` the spine and the work host share, after rows are
appended to `<app-data>/output/<thread_id>.jsonl`.

| Event name | When | Payload |
|---|---|---|
| `rich://output` | A witness recorded a file this thread produced that the record did not hold yet: Rich's own write, the back end's, a worker's or a file a command made. | `{ threadId, added: [Entry], count }`: the entries the new rows touched, re-stated from the disk and the thread's file count after the append |

**Witnessed, never inferred.** Nothing Rich says reaches this event; a file is listed because a
witness saw the write (§4.1). An append that holds nothing new emits nothing. The record on disk
is the truth and this event is a nudge: a reopened panel reads the record, not past events.
As of 2026-10-05 nothing in `richos/app/ui/` listens to it yet; the panel is slice S4.

## Dictation's tool: `rich://dictation`

The twelfth family, added 2026-10-08 for the Dictation sheet and its Settings row (dictation plan
slice 2; the plan is in the private record, not in this repository). Source of truth:
`richos/app/src-tauri/src/dictation_app.rs`, the constant `EVENT`, emitted by the listener the
app sets on its `Link` (the socket to whichever dictation tool holds the key in this login
session) each time that tool says something and once when its socket closes.

| Event name | When | Payload |
|---|---|---|
| `rich://dictation` | The tool's state moved (on, listening, writing, a problem, its key tap made), it said it is about to listen or its socket closed. | `{ changed: true }` |
| `rich://dictation` | Key capture answered: while "Press a different key" waits, the tool's key tap reports the F-key pressed instead of acting on it. | `{ key }`, 1 to 19 |

**A nudge, not a record.** The window reads the facts again with `dictation_status` when it hears
`changed`; nothing in the payload is meant to be drawn. The Dictation sheet and its Settings row are the listener.

## Bust a bug: `rich://bug-report`

The thirteenth family, added 2026-10-09 for Bust a bug (CEO §115, round 21). Source of truth:
`richos/app/src-tauri/src/bug_report.rs`, the constant `EVENT`, emitted by the loop that tries a
report waiting on this Mac again once its rest is over (`spawn_retry`), for each one that went out.

| Event name | When | Payload |
|---|---|---|
| `rich://bug-report` | A report that could not go when the user pressed Send (offline, GitHub down, no reporting account set up) went out by itself. | `{ delivery, post }`, a `Delivery::Sent`: `{ state: "sent", id, number, url, account, title, sent_at_ms, post }` |
| `rich://bug-report` | Rich checked a report he could not check when the user told him (Claude failed, timed out or answered with no report), from the second loop (`spawn_check`). Said on every pass until the window takes it off this Mac (`bug_report_take_unchecked`). | `{ checked }`: `{ id, draft, digest, here, post }` |

The report card waiting on that `id` turns to Sent and Rich says it went out; with no card on screen
(after a relaunch) a notice says so. A report that is still waiting emits nothing. A checked report
comes as its card, unsent, where the user told Rich about it; one kept before a relaunch comes in
the Rich panel. No report is offered for sending without Rich's check (review
rv-20261009T162841Z-69294215-70e6 finding 2).

Request a feature (CEO 2026-10-11, round 24) uses the same event for an idea: `post` is `"idea"`
(a discussion in RichOS's GitHub Discussions, under Ideas) or `"bug"` (an issue) and the window
draws the card and Rich's words for that kind. A delivery's `number` is then the discussion's.
