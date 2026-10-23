# Reliable session evidence

The shell evidence hook enables `errexit` and `pipefail` before each Bash tool
command. A failed test followed by `; echo done` or piped through `tail`, now
fails the tool call. Other tool arguments, including background execution and
timeout, are preserved. The hook does not grant permission or execute commands.

It uses the official [PreToolUse updatedInput contract](https://code.claude.com/docs/en/hooks#pretooluse-decision-control).
It is registered in both the seated engine settings and the plugin hooks and
in the managed-hook inventory used to check plugin installations.
Restart/resume Claude Code after installing to load the new registration.

Expected nonzero outcomes belong in explicit control flow:

```sh
if command > 'command.log' 2>&1; then
    cat 'command.log'
else
    command_status=$?
    cat 'command.log'
    exit "$command_status"
fi
```

Use the original command's status. A trailing filter, `echo` or `cat` must not
stand in for it. The shell's explicit if/else, AND/OR and subprocess semantics
still apply. A script that itself swallows a failure can still exit zero.
Reading a log is evidence retrieval, not a rerun of its tests.

The turn manifest now calls plain Bash output RETURNED and explicitly says
individual commands/checks were not verified. Structured tool errors remain
ERROR. This prevents a successful shell wrapper being labeled as successful
inner work.

Input handovers include authored files. The ingress excludes directories, devices,
OS executables, Claude settings/state and automatic task-notification/system-reminder
envelopes. A shared file predicate is used by ingress and historical rechecking. Its follow-up reader
also rechecks historical system-resource and Claude task-output entries, so they stop
creating notices without editing the original ledger. Real untracked documents,
symlinks and refusals continue through their existing safety checks.

For Cargo output, `richos/app/scripts/rust-test-summary.py path/to/cargo.log` separates
ordinary passes, ignored cases and documentation passes. A log with no ordinary
test result is an error. Failed suites exit 1. The summary explicitly does not
attest the original cargo process exit status or log completeness.

Verification:

- `scripts/hooks/session-evidence.test.sh` reproduces hidden failures in Bash
  and Zsh, tests argument preservation and inspects real ingress ledger output.
- `scripts/hooks/ceo-inputs.test.sh` checks existing handover safety and recovery.
- `scripts/hooks/turn-manifest.test.sh` includes its seven negative controls.
- `ass-kicker/tests/ass-kicker.test.sh` checks the manifest consumer.
- `scripts/hooks/unevaluated-payload.test.sh` checks every registered hook.

The seven focused negative controls in `session-evidence.mutation.sh` restore each
failure in an isolated copy. Each corresponding regression must fail. The source
worktree and live engine are never mutated by that runner.

Read-only replay against the incident ledger removed all 68 false handover entries
without editing that ledger. The positive controls still retain a real untracked
document and pass file symlinks to the existing refusal gates.

The claim-gate fixtures extend real commit IDs when a seven-character prefix is
all digits or all letters. Those prefixes are intentionally outside the gate's
claim vocabulary and previously made its tests intermittent. A forced numeric
branch prefix reproduced seven failures before the fixture fix and passed all
65 cases afterward. Deterministic controls cover both prefix shapes. The full
contract runner now includes the child test output when this section fails.

The plugin inventory also includes the previously omitted unlanded-branches
notice. The regression compares every registered hook/event pair to the managed
inventory, so either direction of drift fails before a plugin is installed.
