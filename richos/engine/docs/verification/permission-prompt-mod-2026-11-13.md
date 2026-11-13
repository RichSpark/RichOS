# Permission prompts in bypass mode: the engine mod, measured (2026-11-13)

Claude Code 2.1.323. Subject: `hooks/answer-permission-prompt.ts`, loaded through `modules` in
`hooks/hooks.json` (terminal) and copied into every RichOS app lease's plugin by
`engine_profile.rs`.

Every session below was a separate throwaway `claude` process started from a scratch folder
with `--setting-sources project,local` (so no user-scope plugin or hook), its environment
cleared of the parent session's variables. None was the operator's session and none was drawn
on a screen: the interactive ones ran in a pseudo-terminal whose every output chunk was
rendered with `pyte` and checked for the dialog. A test-only PreToolUse hook replaced a marker
command with the exact command under test, so Claude Code checked those bytes and not the
model's retyping. The driver never answers Yes; when a dialog stays up 4 s it presses Esc.

## The two prompts

- **screenshot**: a multi-line command with an inline `python3 - <<'EOF'` script and no `rm` in
  it. Claude Code: "Dangerous rm operation not ruled out: the shell may read this command
  differently than it was parsed".
- **rmvar**: `rm -f $D/*.jsonl`. Claude Code: "Dangerous rm operation on possibly-empty variable
  path".

## Before: no mod, bypass mode, interactive

| case | dialog on screen | PermissionRequest | ran |
| --- | --- | --- | --- |
| screenshot | at 4.961 s, until Esc at 9.108 s | fired, `permission_mode: bypassPermissions`, `tool_name: Bash` | no (Esc) |
| rmvar | at 4.333 s, until Esc at 8.465 s | fired, same | no (Esc) |

A settings-file command hook on PermissionRequest that answers No (measured the same day, on
the same harness) removes the dialog only after it was drawn: it stayed on screen 120 to 140 ms
for the hook's run, even for a 20 ms hook.

## After: the mod alone, bypass mode, interactive

The mod file's SHA-256 at the time of the run:
`d9a0d9b402ecec36cb3fb51df5931cebec4b2c7ba1f1f0d351834b3ac38348a7`.

| case | dialog frames | PermissionRequest | what the model read | ran |
| --- | --- | --- | --- | --- |
| screenshot | 0 | not fired | denied by the mod, with Claude Code's reason ("This command could not be checked for dangerous rm operations ...") and the instruction to do it a simpler way | no: the file it would have edited still held its placeholder |
| rmvar | 0 | not fired | denied, with Claude Code's reason and its `"${D:?}"/*.jsonl` rewrite | no: `a.jsonl` still there |

The order, logged from a probe mod in a throwaway `claude -p` session: `UserPromptSubmit`
(`permission_mode: bypassPermissions`), `PreToolUse`, then `tool.check`, whose `next(e)`
resolved to `decision: ask` with Claude Code's reason, then `PermissionRequest`. With the mod,
`tool.check` answers `deny` and Claude Code never raises `PermissionRequest`; the permission
callback returns a deny before it calls the dialog coordinator, so no dialog is created.

## Person-only asks (review rv-20261113T105115Z-3ce478db-ff6a #1)

Claude Code marks some asks `personOnly` (its connector file-transfer confirmation among them)
and then skips PermissionRequest hooks altogether, so a mod answering only PermissionRequest
left those dialogs up (the reviewer's fixture: "dialog requested=1; PermissionRequest calls=0").
They are the tool's own permission check, which `tool.check`'s `next(e)` returns; the mod now
answers at `tool.check`. In Claude Code's test host (`claude plugin test`) a person-only ask is
answered No at the check, nothing runs and no PermissionRequest is raised; the same test fails
against the reviewed version `3ce478db3`. No live connector was available to trigger one in a
real session.

## Other modes and questions

A session in any other mode keeps its ask (the test host covers default, acceptEdits, plan,
auto, dontAsk and a switch out of bypass at the next prompt). AskUserQuestion and ExitPlanMode
are left alone. EnterPlanMode in a bypass session was allowed by Claude Code without an ask.

## The RichOS app (headless)

The app starts Claude Code with `--permission-prompt-tool stdio` and `--permission-mode
bypassPermissions`, and answers a non-question `can_use_tool` with a structured error
(`native.rs`). No dialog is ever drawn there. Replayed in a throwaway headless session with the
app's exact answer:

- without the mod: one `can_use_tool` for Bash; the model read "Tool permission request failed:
  Error: richos does not implement control request subtype: can_use_tool";
- with the mod: no `can_use_tool` at all; the model read Claude Code's reason and the rewrite.

So every app lease now loads the engine's mods and the app's copy of the old pattern rule
`guard-unguarded-rm.sh` is gone with the rule itself.
