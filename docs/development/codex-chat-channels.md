# Dedicated Codex chat connections

Each task gets a fresh pair of journals. A RichOS app handoff belongs to the
conversation where Rich or Stu created it. A terminal handoff belongs to the
terminal project's lead, across session restarts. Codex starts one persisted chat for that handoff and
gets the original task, source and connection in its initial prompt. Follow-ups
use that same connection.

Codex can start a connection independently and save messages before RichOS
opens. Run `connect` from the work's project directory. RichOS resolves that
directory against the company registry, including Git worktrees whose common
repository is registered, then finds or creates that company's Codex
conversation. Work without a registered project stays queued. Opening a
different company or conversation leaves the connection's owner unchanged.

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
legacy-target.json                       # owner of the conventional legacy channels
connections/COMPANY/CHAT/CONNECTION/
connections/_terminal/SESSION/CONNECTION/
connections/_automatic/codex/CONNECTION/ # independently initiated pair
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

An upgrade preserves the old Markdown files and saved spool. The conventional
`rich-codex` and `rich-codex-questions` channels belong to the company that
registered the RichOS repository. If the first cutover filed their Codex
conversation under the company that was open, the upgrade moves the original
thread and its pairs to the repository's registered company. Inbox receipts
and cursors keep their identities and previously issued locators still resolve.
On the first start with the new app cursor, the watcher
starts at the end of each existing incoming Markdown file. Subsequent appends
use a separate live spool and deterministic receipt IDs. Archive contents and
migration notices stay available on disk without waking Stu. The old outgoing
file contains the migration handoff. Reconcile an unfinished task with its
original request and current owner before resending it on a dedicated pair.

RichDoctor includes the watcher's last routing state in its Configuration
collection. It reads the bounded `engine-state/codex-watch/health.json`
snapshot and reports its timestamp, deferred routing state, held source count
and any recorded company repair outcome. The local `host-child.log` contains
watcher errors. Startup logs record a successful or held company repair.
