// answer-permission-prompt.test.ts: the engine mod that answers every Claude Code permission
// prompt No in a bypass-permissions session (answer-permission-prompt.ts). Run by
// scripts/hooks/answer-permission-prompt.test.sh through `claude plugin test`.
//
// The hooks `on` registers here sit beneath the mod and stand for Claude Code itself.
import { expect, test } from 'claude-code/testing'

const RM = { command: 'rm -f $D/*.jsonl', description: 'Remove the logs' }
const HEREDOC = { command: "python3 - <<'EOF'\nprint('it\\'s')\nEOF", description: 'Patch a file' }
const CORE_REASON = 'Dangerous rm operation on possibly-empty variable path: $D/*.jsonl'

const request = (permission_mode: string, tool_name: string, tool_input: unknown) => ({
  session_id: 's', transcript_path: '/t.jsonl', cwd: '/w', permission_mode,
  hook_event_name: 'PermissionRequest' as const, tool_name, tool_input,
})

test('a bypass session: the rm prompt is answered No, with Claude Code\'s own reason', async ($, on) => {
  on('tool.check', () => ({ decision: 'ask', reason: CORE_REASON }))
  on('classic.PermissionRequest', () => ({ decision: { behavior: 'allow' } }))  // never reached

  const checked = await $.tool.check({ tool: 'Bash', input: RM })
  expect(checked.decision).toBe('ask')  // the mod changes nothing at the check itself
  const answer = await $.classic.PermissionRequest(request('bypassPermissions', 'Bash', RM))

  expect(answer.decision?.behavior).toBe('deny')
  const message = answer.decision?.behavior === 'deny' ? answer.decision.message ?? '' : ''
  expect(message).toContain(CORE_REASON)
  expect(message).toContain('nothing ran')
  expect(message).toContain('simpler way')
})

test('a bypass session: a prompt with no reason, from any tool, is still answered No', async ($, on) => {
  on('classic.PermissionRequest', () => ({ decision: { behavior: 'allow' } }))
  for (const [tool, input] of [['Bash', HEREDOC], ['Write', { file_path: '/x', content: '' }],
                               ['mcp__server__tool', {}], ['SendMessage', { to: 'peer' }]] as const) {
    const answer = await $.classic.PermissionRequest(request('bypassPermissions', tool, input))
    expect(answer.decision?.behavior).toBe('deny')
    const message = answer.decision?.behavior === 'deny' ? answer.decision.message ?? '' : ''
    expect(message).toContain(`this ${tool} call`)
    expect(message).toContain('it did not give one')
  }
})

test('any other permission mode: the user chose to be asked, so the prompt is left alone', async ($, on) => {
  on('classic.PermissionRequest', () => ({}))
  for (const mode of ['default', 'acceptEdits', 'plan', 'auto', 'dontAsk']) {
    const answer = await $.classic.PermissionRequest(request(mode, 'Bash', RM))
    expect(answer.decision).toBeUndefined()
  }
})

test('a question to the user is not a permission check and is left alone', async ($, on) => {
  on('classic.PermissionRequest', () => ({}))
  for (const tool of ['AskUserQuestion', 'ExitPlanMode']) {
    const answer = await $.classic.PermissionRequest(request('bypassPermissions', tool, {}))
    expect(answer.decision).toBeUndefined()
  }
})
