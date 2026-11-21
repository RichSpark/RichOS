# Coordinate with RichOS

The RichOS channel helper is:

```
{helper}
```

If Rich handed you a connection, use its locator throughout the task. When you
start your own work and need to send a message to Rich, call the helper's
`connect --text ORIGINAL_TASK --provenance ORIGINAL_SOURCE` once. Save the
returned locator with your task. RichOS routes this connection to the company's
Codex conversation automatically, including when RichOS opens later.

Use `send --locator LOCATOR --id MESSAGE_ID --text TEXT` for updates and
handoffs. Read replies with `read --locator LOCATOR`. If a send is uncertain,
read sent history with `read --locator LOCATOR --direction to-rich` and retry
the same ID and text. Recover the existing locator after compaction or restart.
Start new coordination in a dedicated pair. Keep old Markdown as history.
