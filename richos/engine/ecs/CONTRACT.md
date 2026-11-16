# ECS app protocol 1

Run `python3 ecs/bin/ecs --state-root /absolute/private/state` and supply one JSON
object on stdin. Requests are limited to 1 MiB. Every request names `protocol: 1`
and `command`. Success returns `{protocol:1, ok:true, result:...}`. Failure returns
`{protocol:1, ok:false, error:{kind,message}}` with exit 2. An unavailable component
or unreadable store is distinct from an empty result. The Rust bridge applies a
20-second deadline and a 4 MiB response bound.

| Command | Purpose |
|---|---|
| `hello` | Read-only delivery/protocol identity without creating state |
| `current` | Host reads the current binding for compare-and-set activation |
| `bind` | Host binds entity, thread, session, turn and audience with expected revision |
| `checkpoint` | Persist typed operational statements with a stable request ID |
| `handover` | Inspect a named local app scope while the app is stopped. Read-only and refuses a schema mismatch |
| `receipt` | Read a previous checkpoint receipt in the same entity and thread |
| `brief` | Bounded continuity text, counts and an inspection route |
| `inspect` | Scoped paginated records with sequence fencing |
| `observe` | Host-issued provider observation, never verified assignment completion |
| `import-preview` | Read-only neutral envelope validation and mapping preview |
| `import-apply` | Fenced application with durable partial receipts |
| `seats` | Host reads every seat there is, so orphans can be reconciled |
| `release-seat` | Host releases one worker seat or one of his orphaned thread seats, fenced on that seat's revision |

After binding, commands other than the read-only preview carry the exact
`binding` object returned by `bind`: entity_id, thread_id, session_id, turn_id,
audience and revision. Each mutation checks its scope transactionally. A scope
switch invalidates the old binding. Read results are fenced again before return.
A previous successful binding receipt cannot reactivate a superseded scope.

## Seats

The active context is a **cursor**, not a store: it answers which entity, thread,
session and turn one principal is in right now, while the records themselves are
keyed by entity and thread. It is one row per seat and the conversation rewrites
its own row at the start of every turn. A caller whose binding was frozen before
that turn is therefore stale against the conversation's cursor and current
against its own.

So a request may name a `seat`. An absent seat is the conversation's own cursor
and every call shape that predates seats is unchanged. A named seat is fenced
against that seat's row and the events a command appends carry the fenced row's
own person, so a background record can never land on the conversation's cursor by
default.

A work seat is **one per assignment**, created with it and released with it. One
seat shared by two assignments is the same collision one level in: the table is
keyed by person, so registering the second upserts the first one's cursor and the
first one's frozen binding is stale from then on.

The CEO's own seat is **one per conversation thread**, for the same reason. He can
run any number of conversation threads at once and each one holds its own front
desk, so N front desks on the one `ceo-default` row is that collision with the CEO
on both sides of it: thread B's bind upserts thread A's cursor while A's turn is
open and A's next checkpoint, brief or inspect is refused with "stale app
binding" — which is not a retryable race and dead-letters the turn.

A thread's seat is **derived**, `ceo-thread:<thread_id>`, so the app and the engine
name it without either one holding a mapping the other can drift from. Absent still
means the single legacy cursor, so a client that never names a seat is unaffected.
`hello` answers `ceo_thread_seats` so the app can tell support from silence.

**Whether a call is his is answered positively**, by re-deriving the seat from the
row's own thread and requiring the `ceo` audience — never by comparing a person
against the `ceo-default` literal. A CEO-shaped seat that does not name the thread
it binds is refused and the `ceo` audience is refused to every other seat, so a
work seat cannot reach his continuity or his `ceo_private` records by naming itself
well.

- Bind carries the seat on `thread.activated` only. `entity.registered`,
  `thread.created` and `session.observed` are statements about things that exist
  and are the same facts for whoever is looking, so they stay on the registry
  person; binding them under a seat is refused as a re-registration.
- `checkpoint`, `receipt` and `brief` are the conversation's and are refused on a
  seat that is not one of his. They read and write that seat's cursor by
  construction and an allow-list on tool names cannot see whose seat is calling.
- `inspect` on a seat reads that seat's cursor and that seat's audience. A seat
  bound `worker` sees only worker records.
- `seats` and `release-seat` are the host's, refused to any seat that is not one
  of his. A seat with no assignment behind it is a defect and is reconciled where
  receipts are; a release is an event, so it survives a projection rebuild and
  only a worker seat or one of his thread seats can be released at all —
  `ceo-default` never and no seat can release itself.
- Releasing one of his thread seats requires the revision the seat was enumerated
  at. Whether a conversation thread still exists is the app's knowledge; what the
  store can prove is movement, so a seat that has bound a turn since the
  enumeration belongs to a live thread and is refused. A work seat's release is
  unchanged and needs no such proof.

The model-facing MCP server exposes only `checkpoint` and `inspect`. It takes
its state location and binding from an app-owned scope file. Tool arguments
cannot override either. The app grants access only during a visible turn and
revokes it when the turn returns. Hidden context preparation has no write grant.
This is an application protocol boundary, not an operating-system sandbox.

Checkpoints contain `statements: [{verb, fields}]` or an explicit
`{no_changes:true, reason:...}`. Fields are strings. Opening verbs are priority,
initiative, open_loop, commitment, decision, deadline and blocker. Updates name
the existing `id`. A model-authored checkpoint cannot mark external work
completed, including through an update. Provider exit is not a Git verification
receipt. Completion and correction routing must use their owning app adapters.

A repeated checkpoint ID with identical content returns its receipt. Changed
content is rejected. Interrupted multi-statement checkpoints reconcile existing
statement events before continuing. ECS does not claim a transaction spanning
the app ledger, Loro, a provider or Git.

Schema upgrades hold a migration lock and back up existing SQLite state before
applying new migrations. Individual migrations are transactional. Missing or
unknown migrations are errors, including code downgrade over newer state. Restore
a backup into a separate state root with compatible code; do not overwrite later
work. The tests verify that restoration preserves the previous active scope.
