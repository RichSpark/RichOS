// answer-permission-prompt.test.ts: the engine mod that answers every Claude Code permission
// prompt No in a bypass-permissions session (answer-permission-prompt.ts). Run by
// scripts/hooks/answer-permission-prompt.test.sh through `claude plugin test`.
//
// The hooks `on` registers here sit beneath the mod and stand for Claude Code itself: its
// permission decision (`tool.check`), the person a PermissionRequest would reach, and the tool's
// run (`tool.call`), which records whether anything ran.
import { expect, test } from 'claude-code/testing'

const RM = 'rm -f $D/*.jsonl'
const RM_REASON = 'Dangerous rm operation detected in `rm -f $D/*.jsonl`. The target is a shell variable expansion.'
const SEND = { files: ['/isolated/example.txt'] }
const SEND_REASON = 'send_file can send this session\'s files elsewhere, so Claude asks before using it.'

const request = (permission_mode: string, tool_name: string, tool_input: unknown) => ({
  permission_mode, hook_event_name: 'PermissionRequest' as const, tool_name, tool_input,
})

// The session's own events, which carry its permission mode.
const lifecycle = (on: any) => {
  on('classic.SessionStart', () => ({}))
  on('classic.UserPromptSubmit', () => ({}))
  on('classic.PostToolUse', () => ({}))
}

// Claude Code beneath the mod: its decision is `ask` with `reason`, a person who would answer Yes,
// and a tool run that is recorded.
const claudeCode = (on: any, reason: string) => {
  const seen = { ran: 0, asked: 0 }
  lifecycle(on)
  on('tool.check', () => ({ decision: 'ask', reason }))
  on('classic.PermissionRequest', () => { seen.asked++; return { decision: { behavior: 'allow' } } })
  on('tool.call', () => { seen.ran++; return { result: 'ran' } })
  return seen
}

test('bypass: Claude Code\'s own rm safety ask is answered No at the check, with its reason; nothing runs', async ($, on) => {
  const seen = claudeCode(on, RM_REASON)
  await $.classic.UserPromptSubmit({ prompt: 'go', permission_mode: 'bypassPermissions' })

  const result = await $.tool.call({ tool: 'Bash', command: RM })

  expect(result.isError).toBe(true)
  expect(result.text).toContain(RM_REASON)
  expect(result.text).toContain('nothing ran')
  expect(result.text).toContain('simpler way')
  expect(seen).toEqual({ ran: 0, asked: 0 })  // no prompt was raised, and nothing ran
})

test('bypass: a person-only ask, which never reaches a PermissionRequest hook, is answered No at the check', async ($, on) => {
  // A connector's file transfer: Claude Code asks for a person and skips PermissionRequest
  // altogether (review rv-20261113T105115Z-3ce478db-ff6a #1), so only the check can answer it.
  const seen = claudeCode(on, SEND_REASON)
  await $.classic.SessionStart({ source: 'startup', permission_mode: 'bypassPermissions' })

  const result = await $.tool.call({ tool: 'mcp__connector__send_file', ...SEND })

  expect(result.isError).toBe(true)
  expect(result.text).toContain(SEND_REASON)
  expect(seen).toEqual({ ran: 0, asked: 0 })
})

test('bypass: an ask raised after the check, from any tool, with no reason, is still answered No', async ($, on) => {
  on('classic.PermissionRequest', () => ({ decision: { behavior: 'allow' } }))
  for (const [tool, input] of [['Bash', { command: 'ls' }], ['Write', { file_path: '/x', content: '' }],
                               ['mcp__server__tool', {}], ['SendMessage', { to: 'peer' }]] as const) {
    const answer = await $.classic.PermissionRequest(request('bypassPermissions', tool, input))
    expect(answer.decision?.behavior).toBe('deny')
    const message = answer.decision?.behavior === 'deny' ? answer.decision.message ?? '' : ''
    expect(message).toContain(`this ${tool} call`)
    expect(message).toContain('it did not give one')
  }
})

test('any other permission mode: the user chose to be asked, so the ask is left alone', async ($, on) => {
  const seen = claudeCode(on, RM_REASON)
  const modes = ['default', 'acceptEdits', 'plan', 'auto', 'dontAsk']
  for (const mode of modes) {
    await $.classic.UserPromptSubmit({ prompt: 'go', permission_mode: mode })
    await $.tool.call({ tool: 'Bash', command: RM })
  }
  expect(seen.asked).toBe(modes.length)  // each ask reached the person
})

test('a switch out of bypass at the next prompt: the check\'s ask goes to the user again', async ($, on) => {
  const seen = claudeCode(on, RM_REASON)
  await $.classic.UserPromptSubmit({ prompt: 'go', permission_mode: 'bypassPermissions' })
  expect((await $.tool.call({ tool: 'Bash', command: RM })).isError).toBe(true)
  expect(seen.asked).toBe(0)

  await $.classic.UserPromptSubmit({ prompt: 'again', permission_mode: 'default' })
  await $.tool.call({ tool: 'Bash', command: RM })
  expect(seen.asked).toBe(1)  // the ask reached the person Claude Code would show it to
})

test('a plan approved with bypass permissions: the next ask in the same turn is answered No', async ($, on) => {
  // Approving ExitPlanMode with bypass permissions resumes the same turn, with no new prompt; the
  // mode reaches the mod on that tool's PostToolUse (review rv-20261113T131228Z-92a13da1-850a).
  const seen = claudeCode(on, SEND_REASON)
  await $.classic.UserPromptSubmit({ prompt: 'plan it', permission_mode: 'plan' })
  await $.classic.PostToolUse({ tool_name: 'ExitPlanMode', tool_input: {}, tool_response: {},
                                tool_use_id: 'toolu_plan', permission_mode: 'bypassPermissions' })

  const result = await $.tool.call({ tool: 'mcp__connector__send_file', ...SEND })

  expect(result.isError).toBe(true)
  expect(seen).toEqual({ ran: 0, asked: 0 })
})

test('a question to the user is not a permission check and is left alone', async ($, on) => {
  lifecycle(on)
  on('classic.PermissionRequest', () => ({}))
  await $.classic.UserPromptSubmit({ prompt: 'go', permission_mode: 'bypassPermissions' })
  for (const tool of ['AskUserQuestion', 'ExitPlanMode']) {
    const answer = await $.classic.PermissionRequest(request('bypassPermissions', tool, {}))
    expect(answer.decision).toBeUndefined()
  }
})

test('another plugin\'s $.tool.check query runs nothing and is left alone', async ($, on) => {
  lifecycle(on)
  on('tool.check', () => ({ decision: 'ask', reason: RM_REASON }))
  await $.classic.UserPromptSubmit({ prompt: 'go', permission_mode: 'bypassPermissions' })
  const checked = await $.tool.check({ tool: 'Bash', input: { command: RM } })
  expect(checked.decision).toBe('ask')
})
