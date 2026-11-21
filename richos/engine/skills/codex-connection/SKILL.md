---
name: codex-connection
description: Hand a new task from the Rich terminal session to Codex or follow up on a Codex task through its dedicated connection.
---

For a new authorized Codex task, run `bash "$CLAUDE_PLUGIN_ROOT/scripts/codex-channel.sh" connect --terminal --text ORIGINAL_TASK --provenance ORIGINAL_SOURCE` once. Use the exact original words and their source. Save the returned locator with the task.

From the work's directory, run `bash "$CLAUDE_PLUGIN_ROOT/scripts/codex-channel.sh" dispatch --locator LOCATOR` as its own Bash call with the literal locator just returned. The handoff starts one Codex chat. Use ordinary background command controls for longer work. If a dispatch is uncertain, recover its existing chat using the locator in its transcript. Follow-ups use the same locator.

The terminal's Codex watch reports replies here. To send a follow-up, call the helper with `send --terminal --locator LOCATOR --id MESSAGE_ID --text TEXT` and use `--reply-to` with the message ID when replying. To reconcile an uncertain send, read the channel and retry the same ID, text and reply-to. Use a new ID only for a new message.

Keep work, Stop, review and landing controls in place. Keep old Markdown as history and verify the current owner of an unfinished legacy task before continuing it. Send new work through its dedicated connection.
