// answer-permission-prompt.ts: the engine plugin's mod (hooks/hooks.json `modules`).
// NO CLAUDE CODE PERMISSION PROMPT REACHES A USER WHO CHOSE BYPASS PERMISSIONS.
//
// The CEO, 2026-11-13, running with bypass permissions, was shown a Yes/No prompt a teammate's
// command caused ("Dangerous rm operation not ruled out", from an inline python heredoc with no rm
// in it): "Why am I seeing this". A rule refusing that command's shape was rejected the same
// morning: "how many of the possible variations will that patch stop? One?"
//
// So this acts on the prompt, never on the command text. Claude Code raises
// `classic.PermissionRequest` whenever it is about to ask the user, whatever the tool and
// whatever its reason. In a session whose user chose bypassPermissions, the user has already
// said "do not ask me", so every prompt that still comes is answered No here: nothing runs,
// nothing is shown, and the agent is told why and to do the same thing a simpler way.
//
// Why a mod and not a settings hook on the same event: a command hook on PermissionRequest is
// started after Claude Code has drawn the dialog, which then stays on screen for the hook's run
// (measured 120 to 140 ms, even for a 20 ms bash hook). The mod's in-process answer comes before
// anything is drawn, and `tool.check` hands it Claude Code's own reason, which a command hook
// never receives. Measured on Claude Code 2.1.323:
// docs/verification/permission-prompt-mod-2026-11-13.md.
//
// Left alone on purpose:
//   - sessions in any other permission mode: that user chose to be asked;
//   - AskUserQuestion and ExitPlanMode: they are how an agent asks the user a question, not a
//     permission check, and Claude Code routes them through the same event.
// If this hook fails, its `.catch` answers No as well. Suite: answer-permission-prompt.test.ts,
// run by scripts/hooks/answer-permission-prompt.test.sh.
import type { Register } from 'claude-code'

const QUESTION_TOOLS = new Set(['AskUserQuestion', 'ExitPlanMode'])

// Claude Code's own reason for each call it is about to ask about, from `tool.check`, read once by
// the PermissionRequest answer that follows it. A few entries at most; the oldest go first.
const reasons = new Map<string, string>()
const keyOf = (tool: string, input: unknown) => `${tool}\u0000${JSON.stringify(input)}`

export const message = (tool: string, reason?: string) =>
  `Claude Code stopped this ${tool} call to ask the user for permission, which it does for a few ` +
  `safety checks even in bypass-permissions mode. The user is never asked here: the answer is No, ` +
  `and nothing ran. Claude Code's reason: ${reason?.trim() || 'it did not give one.'} ` +
  `Do the same thing a simpler way that Claude Code can check without asking: for a shell command, ` +
  `one plain command per call, an inline script saved to a file with the Write tool and run from ` +
  `there, and literal paths (or every variable guarded as "\${VAR:?}") for rm. Do not repeat the ` +
  `same call; it would be stopped again.`

export const register: Register = on => {
  on('tool.check', async ($, e, next) => {
    const decided = await next(e)
    if (decided.decision === 'ask' && decided.reason) {
      reasons.set(keyOf(e.tool, e.input), decided.reason)
      while (reasons.size > 32) reasons.delete(reasons.keys().next().value as string)
    }
    return decided
  })

  on('classic.PermissionRequest', async ($, e, next) => {
    if (e.permission_mode !== 'bypassPermissions' || QUESTION_TOOLS.has(e.tool_name)) return next(e)
    const key = keyOf(e.tool_name, e.tool_input)
    const reason = reasons.get(key)
    reasons.delete(key)
    return { decision: { behavior: 'deny', message: message(e.tool_name, reason) } }
  }).catch(($, e, next) =>
    e.permission_mode === 'bypassPermissions' && !QUESTION_TOOLS.has(e.tool_name)
      ? { decision: { behavior: 'deny', message: message(e.tool_name) } }
      : next(e),
  )
}
