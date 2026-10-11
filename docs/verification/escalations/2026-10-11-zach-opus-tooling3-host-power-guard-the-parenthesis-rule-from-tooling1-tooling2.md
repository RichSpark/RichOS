# Escalation: Host-power guard: the parenthesis rule from tooling1/tooling2 lets zsh-runnable host commands through; do not land tooling2 alone

- id: `esc-20261011T133428Z-1c09f165`
- raised: 2026-10-11T13:34:28Z
- from: zach-opus-tooling3
- worktree: `/Users/alex/ab/richos-wt/zach-opus-tooling3` (branch `cc/zach-opus-tooling3`)
- head: `b0f108e97676002fc30dac1d321d8c9c56a02545`
- state: **proceeding**
- for: lead

## The question

None needed to proceed. Know before landing: tooling2 (b0f108e97) and tooling1 (af4b68db1) pass five lines that main refuses, because zsh treats a trailing (...) on a word as glob qualifiers and RUNS the command. Measured harmlessly with /bin/echo: zsh runs '/bin/echo(N) x', '/bin/echo(x) x' and '/bin/echo(:t) x'. The guard text cases '/sbin/<halt-command>(N) -h now' on the Bash surface (this Mac's shell is zsh), 'sudo /sbin/<restart-command>(x)', '<halt-command>(N)', and '/sbin/<restart-command>(:t)' in a .sh are FIND on main and CLEAN on af4b68db1 and b0f108e97. If tooling2 is landed by itself, main gets that hole. My branch (tooling3, which contains tooling2) narrows exception 1 to the two forms no shell runs: empty parens (a definition in bash, sh and zsh; measured) and a paren followed by an ampersand (a parse error in zsh; measured). Consequence versus the brief: 'shutdown(&quota)' and 'shutdown()' still pass, but a bare call with a plain argument such as 'shutdown(x)' is refused, exactly as on main.

## What was already tried

Fixture matrix over main, af4b68db1 and b0f108e97 (text only, classifier extracted, nothing executed). zsh probe used only /bin/echo in a scratch directory.

## Proceeding meanwhile

Implementing the brief on tooling3: lexer removed, exception 1 narrowed as above, exception 2 (Rust /// and //! lines with the trigger only inside backticks) implemented with the extra condition that each such backtick span is a bare name (no spaces, no slash, no arguments), so it cannot carry a command with arguments.

## How this reaches the lead

This file is the RECORD, not the delivery. The escalation was written to the
engine escalation ledger at `/Users/alex/.claude/state/escalations.jsonl` as `esc-20261011T133428Z-1c09f165`, which the
lead's session reads at session start and at every turn end WITHOUT this branch
being merged. If this file is never landed, the escalation still arrives.

Close it with:

    escalate.sh ack esc-20261011T133428Z-1c09f165 --disposition "<what you decided or did>"
