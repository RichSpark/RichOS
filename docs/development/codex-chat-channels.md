# Dedicated Codex chat connections

Each task gets a fresh pair of journals. A RichOS app handoff belongs to the
conversation where Rich or Stu created it. A terminal handoff belongs to the
Rich session that ran it. Codex starts one persisted chat for that handoff and
gets the original task, source and connection in its initial prompt. Follow-ups
use that same connection.

Codex can start a connection independently. It can save messages before RichOS
opens. The app automatically finds or creates the Codex conversation in the
configured company when independent work first arrives. It can also use the
sole registered company. When a company is still needed, the messages stay on
disk until the app has a destination. Opening a different conversation does
not change a connection's owner.

The CLI is `richos/engine/scripts/codex-channel.sh`. The app hands Rich and
Stu the packaged runtime command. The terminal plugin loads the
`codex-connection` instructions.

```sh
connect --text 'Original user request' --provenance 'Transcript/source reference'
connect --terminal --text 'Original user request' --provenance 'Transcript/source reference'
dispatch --locator CONNECTION
send --locator CONNECTION --id RECORDED_UNIQUE_ID --text 'Message'
send --terminal --locator CONNECTION --id RECORDED_UNIQUE_ID --text 'Reply'
read --locator CONNECTION
read --locator CONNECTION --direction to-rich
```

On a Rich-started connection, only dispatch launches the chat. If the launch
outcome is uncertain, recover the existing chat by its locator in the
transcript. Messages require a recorded ID. Read sent history and retry an
uncertain send with the same ID, text and reply-to. A different message needs
a new ID. Saved replies wait for Codex's next read. The terminal watch prints
framed replies only for its owning session.

```
automatic.json                          # installed app's automatic destination
connections/COMPANY/CHAT/CONNECTION/
connections/_terminal/SESSION/CONNECTION/
connections/_automatic/codex/CONNECTION/ # pending until first destination
  connection.json
  to-rich.jsonl
  to-codex.jsonl
```

The app binds pending connections to the ledger conversation before delivering
any message. Each pending locator retains its own binding so it still works
through retries and app restarts. Direct app connections keep their existing
ledger IDs. The source journals retain complete synced frames. A busy or
damaged journal holds its own channel. The app saves a Stu inbox receipt before
its advisory cursor. Replaying a cursor is idempotent against inbox history.
Both review watchers use the same framed READY reader and deliver review results
on the originating pair. Existing work, review and landing authorization still
apply.

An upgrade preserves the old Markdown files and any prior frozen legacy
association. The saved incoming spool imports into its owning conversation
with deterministic receipt IDs, checking archived inboxes for existing
deliveries. The old outgoing file remains accessible with a migration handoff
and a reference in the app. It has no per-task delivery receipts. Reconcile an
unfinished task with its original request and current owner before dispatching
it. Unframed tails remain available for recovering complete requests.

RichDoctor includes the watcher's last routing state in its Configuration
collection. It reads the bounded `engine-state/codex-watch/health.json`
snapshot and reports only its timestamp, deferred routing state and held
source count. The local `host-child.log` contains the detailed watcher errors.
