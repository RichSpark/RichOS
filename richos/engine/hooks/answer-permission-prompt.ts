// answer-permission-prompt.ts: the engine plugin's mod (hooks/hooks.json `modules`).
// NO CLAUDE CODE PERMISSION PROMPT REACHES A USER WHO CHOSE BYPASS PERMISSIONS.
//
// The CEO, 2026-11-13, running with bypass permissions, was shown a Yes/No prompt a teammate's
// command caused ("Dangerous rm operation not ruled out", from an inline python heredoc with no rm
// in it): "Why am I seeing this". A rule refusing that command's shape was rejected the same
// morning: "how many of the possible variations will that patch stop? One?"
//
// So this acts on the prompt, never on the command text. In a session whose user chose
// bypassPermissions, the user has already said "do not ask me", so every ask Claude Code still
// raises is answered No here, before any dialog exists: nothing runs, nothing is shown, and the
// agent reads Claude Code's own reason and is told to do the same thing a simpler way.
//
// Where it answers:
//   - `tool.check`, the permission decision itself. Every ask a tool call can raise comes out of
//     it, including the person-only safety asks (a connector sending files, for one) that
//     Claude Code never shows to a PermissionRequest hook; a deny there is a tightening, which
//     a hook may always make, and a deny never reaches the dialog.
//   - `classic.PermissionRequest`, for an ask raised after that decision.
// A settings-file command hook on PermissionRequest is not enough: it starts after the dialog is
// drawn, which stays on screen for the hook's run (measured 120 to 140 ms), and it never sees the
// person-only asks. Measured on Claude Code 2.1.323:
// docs/verification/permission-prompt-mod-2026-11-13.md.
//
// The RichOS app runs Claude Code headless in bypass mode and loads this mod too (engine_profile.rs
// copies the engine's `modules` into each lease's plugin): no prompt is ever drawn there, and without
// the mod the agent read only the app's "richos does not implement control request subtype:
// can_use_tool" instead of Claude Code's reason.
//
// tool.check carries no permission mode, so the mode is read from the classic events that do
// (SessionStart, UserPromptSubmit, PostToolUse, PermissionRequest): a switch of mode takes effect
// after the next tool call or prompt. PostToolUse is the one a plan approval reaches: approving
// ExitPlanMode with bypass permissions resumes the same turn, with no new prompt (review
// rv-20261113T131228Z-92a13da1-850a).
//
// Left alone on purpose:
//   - sessions in any other permission mode: that user chose to be asked;
//   - AskUserQuestion and ExitPlanMode: they are how an agent asks the user a question, not a
//     permission check;
//   - a `$.tool.check` query from another plugin: nothing is being run.
// If a hook fails, its `.catch` answers No as well. Suite: answer-permission-prompt.test.ts,
// run by scripts/hooks/answer-permission-prompt.test.sh.
import type { Register } from 'claude-code'

const BYPASS = 'bypassPermissions'
const QUESTION_TOOLS = new Set(['AskUserQuestion', 'ExitPlanMode'])

let mode: string | undefined

export const message = (tool: string, reason?: string) =>
  `Claude Code stopped this ${tool} call to ask the user for permission, which it does for a few ` +
  `safety checks even in bypass-permissions mode. The user is never asked here: the answer is No, ` +
  `and nothing ran. Claude Code's reason: ${reason?.trim() || 'it did not give one.'} ` +
  `Do the same thing a simpler way that Claude Code can check without asking: for a shell command, ` +
  `one plain command per call, an inline script saved to a file with the Write tool and run from ` +
  `there, and literal paths (or every variable guarded as "\${VAR:?}") for rm. Do not repeat the ` +
  `same call; it would be stopped again.`

const answers = (tool: string) => mode === BYPASS && !QUESTION_TOOLS.has(tool)

export const register: Register = on => {
  on('classic.SessionStart', ($, e, next) => { mode = e.permission_mode ?? mode; return next(e) })
  on('classic.UserPromptSubmit', ($, e, next) => { mode = e.permission_mode ?? mode; return next(e) })
  on('classic.PostToolUse', ($, e, next) => { mode = e.permission_mode ?? mode; return next(e) })

  on('tool.check', async ($, e, next) => {
    const decided = await next(e)
    if (decided.decision !== 'ask' || e.tool_use_id === undefined || !answers(e.tool)) return decided
    return { decision: 'deny', reason: message(e.tool, decided.reason) }
  }).catch(async ($, e, next) => {
    const decided = await next(e)
    return decided.decision === 'ask' && e.tool_use_id !== undefined && answers(e.tool)
      ? { decision: 'deny', reason: message(e.tool) }
      : decided
  })

  on('classic.PermissionRequest', async ($, e, next) => {
    mode = e.permission_mode ?? mode
    if (!answers(e.tool_name)) return next(e)
    return { decision: { behavior: 'deny', message: message(e.tool_name) } }
  }).catch(($, e, next) =>
    answers(e.tool_name) ? { decision: { behavior: 'deny', message: message(e.tool_name) } } : next(e),
  )
}
