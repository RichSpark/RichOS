# Subagent Bash commands: foreground in the app, six-second grace in the terminal

## In the app (every entrypoint other than `cli`)

The app's sessions run as `sdk-cli`. There an ordinary subagent Bash command runs
exactly as the host runs it, the same as in a plain terminal:
`shell-evidence.sh` adds the failure prefix and the ownership record and
nothing else. It does not add a background flag, does not change the timeout
and does not tell the worker to wait. A short command's result comes back
directly. When the host moves a long command to the background at its Bash
timeout, the result arrives through the host's own completion notice and the
worker may keep working or end its turn to await it. Explicit background
requests stay background.

A pause still holds an app worker. Its commands are recorded, so a hold freezes
its whole process tree, foreground shell included and refuses its new
commands. Measured on Claude Code 2.1.314 (print mode, async subagent): a
lead's message reaches a worker in a foreground command only when that command
returns and the host does not move the command just to deliver it. So at a
hold the app moves each of the worker's foreground commands to the background
itself (`control_request{background_tasks, tool_use_id}`, `native.rs`). The
command's tool call returns at once, still frozen and the WAIT reaches the
worker within about a second. Without the app, for example in a session the app
did not start, the WAIT arrives at that command's Bash timeout (two minutes by
default, at most ten). The command stays frozen the whole time and no model
call is made. Past its timeout, a frozen command is moved to the background. It
is neither continued nor killed and after RESUME the same process finishes and
its notice delivers the result.

A worker that ends its turn while one of its commands runs or just after one
ended but before its notice arrived, is not recorded as finished
(`workspaces.py record_end`). In the app's transcripts that second case is read
from the receipt's own text, since those transcripts carry no task metadata.
Stall-watch's automatic idle end never ends such a command
(`stall_watch.py`, HOST CALLS). It still reports the deferred end.

## In the terminal (`cli`)

Ordinary CLI subagent Bash commands get a six-second foreground grace by
default through `shell-evidence.sh`. Installed acceptance passed on Claude Code
2.1.289. Use a directly delivered result immediately. If the host moves the
command to the background, it continues running; before dependent work, run
the installed engine's `agent_hold.py` with its `wait` argument, with the Bash
tool's timeout set to `600000`. The collector returns the bound task's output
and exit status once. Direct results are marked consumed at the next tool
boundary without another collection call. Six seconds bounds foreground
waiting, while the host's background execution limits still apply.

## Rollback

Set `RICHOS_AGENT_BASH_FOREGROUND=0` to restore forced background execution for
ordinary subagent commands, in the app and in the terminal.
