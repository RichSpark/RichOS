# Dedicated Codex chat connections

Each task gets a fresh pair of journals. A RichOS app handoff belongs to the
conversation where Rich or Stu created it. A terminal handoff belongs to the
terminal project's lead, across session restarts. Codex starts one persisted chat for that handoff and
gets the original task, source and connection in its initial prompt. Follow-ups
use that same connection.

Codex can start a connection independently. It can save messages before RichOS
opens. The app automatically finds or creates the Codex conversation in the
configured company when independent work first arrives. It can also use the
sole registered company. When a company is still needed, the messages stay on
disk until the app has a destination. Opening a different conversation does
not change a connection's owner.

The CLI is `richos/engine/scripts/codex-channel.sh`. The app hands Rich and
Stu the packaged runtime command. Terminal Rich receives the
`codex-connection` instructions at SessionStart. RichOS adds a managed
reference to the user's Codex instructions on launch so new Codex chats can
find the installed helper.

```sh
connect --text 'Original user request' --provenance 'Transcript/source reference'
connect --terminal --text 'Original user request' --provenance 'Transcript/source reference'
dispatch --locator CONNECTION
resume --locator CONNECTION
send --locator CONNECTION --id RECORDED_UNIQUE_ID --text 'Message'
send --terminal --locator CONNECTION --id RECORDED_UNIQUE_ID --text 'Reply'
read --locator CONNECTION
read --locator CONNECTION --direction to-rich
```

Dispatch records the Codex session as it starts. For follow-ups, send saves
the message and returns a resume command for that same chat. Resume drains
pending messages and records the position after a successful turn. If the
initial launch outcome is uncertain, recover the existing chat by its
locator in the transcript. Messages require a recorded ID. Read sent history and retry an
uncertain send with the same ID, text and reply-to. A different message needs
a new ID. Saved replies wait for Codex's next read. The terminal watch prints
framed replies once for their owning project, including after a restart.

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

An upgrade preserves the old Markdown files, saved spool and any prior frozen
legacy association. On the first start with the new app cursor, the watcher
starts at the end of each existing incoming Markdown file. Subsequent appends
use a separate live spool and deterministic receipt IDs. Archive contents and
migration notices stay available on disk without waking Stu. The old outgoing
file contains the migration handoff. Reconcile an unfinished task with its
original request and current owner before resending it on a dedicated pair.

RichDoctor includes the watcher's last routing state in its Configuration
collection. It reads the bounded `engine-state/codex-watch/health.json`
snapshot and reports only its timestamp, deferred routing state and held
source count. The local `host-child.log` contains the detailed watcher errors.
