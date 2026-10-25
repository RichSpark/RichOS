# The RichOS Engine

This is the machinery behind Rich.

RichOS is an **AI Operating System for CEOs**: the CEO talks to one AI executive
— **Rich Hand**, AI Chief of Staff & Business Operations Lead — who owns the
work, delegates it to a team of AI workers, supervises them and comes back with
results and decisions. The engine in this directory is what makes that
survivable in a real repository: worktree-per-worker isolation, a single writer
to `main`, durable "the commit is the handoff" semantics, model tiering and a
mandatory QA pipeline. Drop it into a repo, point it at the project and have
the built-in HR worker staff the domain team.

The engine has five principal components:

- [Loro](loro/README.md) compiles and maintains durable knowledge in an external private corpus.
- [ECS](ecs/README.md) retains scoped commitments, decisions and operational continuity.
- [Mega Lander](mega-lander/README.md) owns isolated workspaces, integration eligibility and cleanup.
- [ASS Kicker](ass-kicker/README.md) checks briefs, reported actions and execution evidence.
- [Voice](voice/README.md) owns shared speech-model definitions and provisioning for Talk to Rich and the speech tools.

The desktop profile registers named teammates: Dean, Clark, Reed, Frank and Pierce
(`agents/`) and the user's own. Each runs as the worker or the reviewer of an
assignment or is consulted on a job that changes no repository (no workspace, no file edits,
its final message handed back), with that duty's app instructions (`mega-lander/duties/`) at
the top of its brief. The shelf (`team/shelf/`) holds stock teammates no install registers;
the back end's `richos_work.team` tool lists and shows them and saves the definition Dean fits
as the user's own (`<app data>/team/<name>.md`), which an engine update never replaces.
Engine code, app coordination and target repositories have separate roots. A fresh
corpus starts empty; installing the engine does not import an operator's knowledge,
private roster or terminal configuration. Installed acceptance is recorded against
an exact app and engine candidate separately from source-level test results.

`PreToolUse[Agent]` automatically runs Pierce before a new worker starts, in
terminal plugin sessions and desktop sessions. It inspects the final assignment,
original user messages and available inherited instructions with Opus and
read-only file tools. It returns findings to the planner, without proposing a
fix. The planner revises the assignment or dismisses the exact report with
`scripts/lib/pierce.py --dismiss <report-id> --reason '<reason>'`, then retries.
A dismissal expires after 15 minutes and does not transfer to changed user
context, assignments or instructions. Unavailable inspections are reported as
unavailable, never PASS. Dry-run spawn checks, resumes and Pierce's own launch
do not start another inspection. The inspector has a five-minute deadline and
uses Claude Code's subscription authentication with safe mode and restricted
Read, Glob and Grep tools, without inherited plugins, hooks or shell access.
Reports live under the Claude config directory's `state/pierce` or the desktop
engine state's `pierce` directory. This uses the shared plugin when the terminal
session lives in `femcboost`; it does not require the session to move to RichOS.
When Zach's `strip-ack-lines` hook is registered, Pierce uses that hook's shared
function to inspect the assignment the teammate receives. The guards still read
the original acknowledgment lines. Generated compaction summaries and task
notifications are excluded from the original user messages.

A revised refused brief continues its preceding review: Pierce checks the changed
lines and unresolved findings, with the original human requests retained. It
reads unchanged standing instructions from their source files only when needed.
The continuation expires 15 minutes after its first inspection and applies only
to the same named worker, model, human context, instructions and roots. A passed
or dismissed review, a changed context or an unrelated assignment requires a
fresh inspection. Required guard acknowledgments and the generated merge-main
handover contract are protocol, not editorial findings. Real new scope errors
and instructions that defeat that contract still need review.

`ceo-wiki/` is retired as the new-install scaffold. Its public historical templates
are retained under `reference/legacy-ceo-wiki/`. Existing adopter-owned stores and
legacy references are preserved during upgrade; personal migration is separate.

**A note on vocabulary.** RichOS's product voice says *Rich* and *AI workers*.
This directory is the engine's own technical documentation, so it also uses the
Claude Code mechanics terms — *orchestrator*, *agent*, *subagent*, *teammate*,
*spawn* — wherever precision demands them. They name the same things: the
orchestrator is Rich; the agents are his AI workers.

## Voice

[Voice](voice/README.md) owns shared speech-model definitions and provisioning for
Talk to Rich, dictation and call transcription. The live audio pipeline remains in
[`richos-voice`](../app/crates/richos-voice/); the component README maps its app and tool integrations.

## Cross-repo workspace automation

[Mega Lander](mega-lander/README.md) manages agent workspaces across repositories,
from registration through landing or discard and automatic cleanup.

## Agent reliability

[ASS Kicker](ass-kicker/README.md) groups checks on agent briefs and reported
actions with their tests and an expanding [failure catalog](ass-kicker/coverage.md).
The catalog distinguishes tested interventions from unresolved failure classes.

## 60-second proof — watch it work before you set anything up

Don't take the rest of this README on faith. Run one command:

```bash
scripts/demo.sh
```

It builds a throwaway sample repo in a temp dir, wires in this engine's actual,
unmodified enforcement hooks via the real `install.sh` and drives seven
narrated beats end-to-end — no Claude API key, no live agents, no network,
nothing left behind:

1. A file-writing spawn **without** worktree isolation gets **blocked** —
   real hook, real refusal message.
2. A write straight into a protected source path from the shared checkout
   gets **blocked** — same real hook, different trigger.
3. A **correctly** isolated spawn sails through the same gate untouched.
4. An engineer commits on a real, isolated git worktree branch — the
   commit-is-the-handoff, for real.
5. A planted defect gets **rejected** by a QA check.
6. The fix lands and the QA check **passes** — the FIX-FIRST loop closes.
7. The fix merges to `main` and `contract-integrity-probe.sh` runs green.

It ends with a plain verdict: **"Your team's enforcement machinery works.
7/7 beats passed."** Any beat failing turns the summary red and exits
non-zero — the demo doubles as an integrity check of your own install. Beats
1, 2, 3, 4 and 7 are **real enforcement** (the actual hook binaries and git
mechanics, unmodified); beats 5 and 6 are **narrated simulation** — there's no
live QA worker in a one-command demo script, so the git/exit-code mechanics are
real but the "QA verdict" is a scripted stand-in, labeled as such every time.
Runs in a couple of seconds; your own repo, your real config and this engine's
own checkout are all completely untouched by it.

## Then read the walkthrough

[`WALKTHROUGH.md`](WALKTHROUGH.md) traces one realistic feature through the
**full** lifecycle the 60-second proof only samples: a wiki consultation, the
real spawn shape, the commit-is-the-handoff, the single-writer land, the
complete four-step QA pipeline **including a genuine FIX-FIRST bounce** and
a design-gatekeeper signoff with a documented gap — only after which the CEO
sees the work. It's an illustrative narrative (clearly labeled as such), not
a captured transcript — `scripts/demo.sh` above is the runnable counterpart
of its enforcement beats. Read it once, end to end, before you set anything
up; it's the fastest way to understand what this engine actually does.

**Helping someone else set up?** If you're the technical operator running a
live setup session for a non-technical CEO, use
[`ONBOARDING-RUNBOOK.md`](ONBOARDING-RUNBOOK.md) instead of improvising —
it's the exact timed script: preflight checklist, the session agenda with
expected-green output for every step, the common stalls with their fixes and
a machine-checkable definition of done.

## License

**GNU AGPL v3, unmodified.** The canonical text is the repository's root
`LICENSE`; the standalone engine release asset carries a byte-identical copy at
`engine/LICENSE`, placed there by packaging. See
[`LICENSING.md`](LICENSING.md) for what the AGPL grants, what its section 13
requires of a modified version offered over a network, the brand material that
is excluded from the grant and the seven bundled third-party items that keep
their own upstream terms.

## Prerequisites

The mechanical layer (`scripts/hooks/`) is plain POSIX shell plus a handful of
external tools it shells out to. All of the following must be resolvable on
`$PATH` wherever Claude Code runs the hooks:

- **bash** — every hook's interpreter (`#!/usr/bin/env bash`).
- **git** — worktree resolution, worktree listing and the write-guard's repo
  root.
- **python3** — every hook parses its JSON tool-input payload via `python3
  -c '...'`. **This is a hard requirement, not an optional nicety**: every
  enforcement hook (`guard-worktree-isolation.sh`, `guard-definition-drift.sh`,
  `guard-main-checkout-writes.sh`,
  `guard-bash-main-writes.sh`, `verify-agent-prompt.sh`, `detect-nonnative-worktree.sh`,
  `guard-resume-isolation.sh`),
  `snapshot-agent-definitions.sh`,
  `scripts/hooks/install.sh` and `scripts/hooks/contract-integrity-probe.sh` all
  check `command -v python3` up front and refuse (non-zero exit, loud stderr) if
  it is missing — they do NOT silently pass every spawn/write through. If your
  environment might not have `python3` on `$PATH` by default (some minimal
  containers, some CI images), install it before relying on the engine's
  enforcement.
- **Standard coreutils** — `grep`, `sed`, `awk`, `cut`, `tr`, `date`, `mkdir`,
  `mktemp`, `basename`, `dirname`. Present by default on macOS and virtually
  every Linux distribution.
- **shasum** or **sha256sum** — for the hook `.sha256` integrity sidecars
  (`install.sh` and `contract-integrity-probe.sh` fall back to a `python3`
  one-liner if neither is present, so python3 doubles as the last-resort hash
  backend too).
- **rsync** — only if you use `scripts/collect-worktree-artifacts.sh` to mirror
  gitignored test/QA evidence out of a worktree before removal.

If you adopt `reference/advanced-tier/` (optional, not wired in by default),
its two "identity-or-refuse" verifiers (`freshness-check.sh`,
`client-data-check.sh`) carry the same `python3` requirement and the same
fail-closed preflight check.

## Critical configuration — never remove

`.claude/settings.local.json` (committed) carries two non-hook keys that are
just as load-bearing as any hook and their absence has caused a REAL
incident:

```json
{
  "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" },
  "worktree": { "baseRef": "head" }
}
```

- **`env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS: "1"`** — this key was once
  accidentally deleted from `settings.local.json` during an edit. **The
  symptom did not show up until the NEXT session start**: Rich could not see
  or spawn ANY teammates — a silent total failure, with no error shown
  anywhere. If your workers suddenly seem to have vanished (no roster, spawns
  behave as if the team feature doesn't exist), check this key FIRST.
- **`worktree.baseRef: "head"`** — the worktree-isolation doctrine
  (`CLAUDE.md.template`, `skills/using-git-worktrees/`, `skills/rich-lander/`)
  assumes every native isolation worktree branches from local HEAD. Without
  this key, that assumption silently breaks and the land sequence's
  merge-base verification goes wrong in ways that are hard to diagnose after
  the fact.

The engine also ships `env.CLAUDE_CODE_MAX_SUBAGENTS_PER_SESSION: "2000"` in the
same `env` block. It is not probe-gated: a long session (a full QA pipeline, a
multi-batch pass, a fresh spawn per follow-up because resuming a landed teammate
is banned) burns spawns fast and the default ceiling stops the session partway
through — but it fails LOUDLY (spawns simply stop) rather than silently, so it
stays a plain config key rather than a hard gate. Lower it if you want a tighter
budget; don't leave it at the default if you run long sessions.

**This is enforced, not just documented.** `scripts/hooks/contract-integrity-probe.sh`
hard-fails (Layers I and J, exit 2, loud stderr) if either key is missing or
wrong the next time you run it and `scripts/hooks/install.sh` refuses to
proceed at all (no migration, no sidecar refresh) if `settings.local.json` is
missing either key. If you ever see teammates go missing or the probe fail on
Layer I/J, restore both keys in `.claude/settings.local.json` and re-run
`scripts/hooks/install.sh`.

### The global-gitignore trap — commit `settings.local.json`, force-add if needed

There is a second, sneakier way to lose this file and it fails **silently**:
vanilla Claude Code treats `.claude/settings.local.json` as machine-local, so a
common global-gitignore entry — `**/.claude/settings.local.json` in
`~/.config/git/ignore` (git's default `core.excludesFile`) — makes `git add -A`
**quietly skip it**. The file stays on disk, so `install.sh`, the probe and the
demo all pass locally; the failure only surfaces at the **next clone/session**,
where the file was never committed and Rich wakes with no team and no error
(the same "ghost team" symptom as a deleted `AGENT_TEAMS` key, arriving by a
different door). This engine **force-adds** the file past that rule; you must do
the same in your repo. After adoption, verify and fix:

```bash
git ls-files .claude/settings.local.json          # must print the path
git add -f .claude/settings.local.json && git commit   # if it printed nothing
```

**This is enforced too.** The probe's **Layer N** hard-fails when
`.claude/settings.local.json` is present on disk but untracked AND matched by a
gitignore rule (the trap), naming the `git add -f` fix; it warns (does not fail)
when the file is simply not committed yet and skips cleanly outside a git repo.

## The two-layer model

The engine is the **generic framework** (Layer A), extracted from a real
production setup. The project-specific instantiation (Layer B) — one product's
stack, surfaces, roster, deploy machinery and canonical data — is deliberately
left out. What ships here is only what transfers to any project:

- **Layer A (this engine):** the mechanical orchestration layer (hooks +
  config), the meta-roles (HR, researcher, reader, devil's advocate), the
  reusable role templates, the worktree/handoff/land doctrine and model
  tiering.
- **Layer B (you supply):** your product's surfaces, protected source trees,
  team roster, deploy targets and any domain-specific hard rules — filled into
  the clearly-marked `<!-- TODO -->` blocks and `orchestration.config`.

## What ships

| Piece | Path | What it is |
|---|---|---|
| ASS Kicker | `ass-kicker/` | Brief provenance annotation, specification-based dispatch checks and stated-action analysis, with tests and explicit coverage limits. Existing hooks remain platform adapters |
| Mechanical hooks | `scripts/hooks/` | Worktree-isolation guard (incl. structural no-name-reuse), resume-isolation guard, definition-drift guard pair, workspace lifecycle (the workspace spec: registration at spawn, land and discard as the only deleters — `mega-lander/workspaces.sh`), spawn-content verifier, reader/creator hints, secrets scanner, durable idle/task handoff loggers, contract-integrity probe + self-test suites, `install.sh` generator |
| PreToolUse runner | `scripts/hooks/dispatch-pretooluse.sh` + `scripts/hooks/dispatch-pretooluse.manifest` | ONE registered script where `PreToolUse[Bash]` had twelve guards and `PreToolUse[Write\|Edit\|MultiEdit\|NotebookEdit]` had five. The rules are those same seventeen files, unmoved and unedited, named in a manifest and still independently runnable as their own processes. **The win is not that bash is cheap to start.** Measured 2026-09-15 with counting shims on `PATH`: one shell call cost **14 bash + 37 python3 + 36 git** processes and bash startup was 4% of it — two of every three python3 forks and all of the git forks were twelve guards asking the same two questions (*which repository governs this call?* and *does this payload parse?*) about one identical payload. One process answers them once and hands the answers to every rule; the same call now costs **3 bash + 19 python3 + 8 git** — the dispatcher plus the two entries that are deliberately not rules. Serial work for one shell call, median of 7: **1203 ms → 397 ms**. `unverified:` whether the host runs its registered entries concurrently; measured side by side, the parallel medians are 310 ms → 254 ms, so the win is 3x if it runs them serially and 18% if it runs them all at once. Each rule runs in its own forked subshell with bash's default options restored, so an `exit` ends the RULE and never the chain and nothing a rule sets can reach a sibling. A rule that is missing or exits anything but 0 or 2, is **announced by name as not having evaluated the call** while every sibling still runs — a rule that did not run is never reported as a rule that found nothing. `shell-evidence.sh` (an `updatedInput` transformer, not a rule) and `guard-sealed-worktree.sh` (registered on the EMPTY matcher, so it governs every tool) stay registered on their own. Adding or removing a rule is one line in the manifest, which is the asymmetry this step exists to remove. Suite: `dispatch-pretooluse.test.sh` (27 cases) + `dispatch-pretooluse.mutation.sh` (16 mutants) |
| Resume-isolation guard | `scripts/hooks/guard-resume-isolation.sh` | Blocks a `SendMessage` that would RESUME a completed/removed teammate (the spawn-side guard never fires on a resume) — a file-bearing follow-up must spawn fresh in a new isolated worktree; a `resume-ack:` line is the audited escape hatch for a safe pure-question resume. Pairs with the spawn guard's structural no-name-reuse to kill live-vs-ghost ambiguity |
| Named-person deny-list | `scripts/hooks/guard-named-persons-writes.sh`, `scripts/hooks/guard-named-persons-commands.sh`, `scripts/named-persons.sh` | Refuses a private individual's NAME entering a repository that publishes — in file content, in a destination path, in a commit message, in a branch name, in a PR/issue/release title and at release. A name is not a secret and never trips the scanner above. The list lives OUTSIDE every repository (`~/.richos-privacy/named-persons`) and a missing list is announced loudly rather than read as "nothing to check" |
| Secrets scanner | `scripts/hooks/scan-secrets.sh` | Blocks a Write/Edit/MultiEdit/NotebookEdit whose content looks like a live secret (AWS/GitHub/Anthropic/OpenAI/Stripe key patterns, PEM private keys, high-entropy assignment literals for `password`, `api_key`, `secret` and `token`) — closes the "will these AI workers leak my keys" objection structurally, not by promise. Config-driven allowlist so placeholders (e.g. `re_xxxxxxxxx`) never false-positive |
| Dialect guard | `scripts/hooks/dialect-guard.sh` + `scripts/lib/dialect/dialect-fix.py` + `scripts/lib/dialect/dialect-en-US.generated.dict` | Fixes a Write/Edit/MultiEdit/NotebookEdit that introduces a word outside the repository's declared dialect where the fix is mechanical (prose; comments and sentences in code), through `updatedInput` and tells the model each change; refuses the rest (code identifiers, one-word string literals, markup and config, quotations, names, `CEO queue`). Built because a rule with no write-time chokepoint decays on a measurable schedule: a 654-site cleanup pass on 2026-08-30 was partly undone **within hours**, into the very page that carries the ruling, because the pass cleaned what existed and constrained nothing that came after. The vocabulary is DATA, in one file, hashed by `install.sh` and verified by probe **Layer T** — a hash-matched guard over an emptied word list is a green tick over an enforcement outage. **The exemptions are the product:** fenced code, inline code spans, blockquotes (quoted external material is never "corrected"), URLs and paths, code identifiers (`\b` boundaries make camelCase/snake_case free; kebab, `--custom-props` and dotted paths are handled explicitly), captured evidence by path segment (`raw/`, `cold-open/`, `transcripts/`, `fixtures/`, `corpora/`, `snapshots/`, `logs/`), vendor `LICENSE`/`NOTICE`, lockfiles, per-word file-type exemptions (`grey` is a legal CSS color), a config allowlist and a per-line `dialect-exempt: <reason>` — declared where a reviewer sees it and a bare marker exempts nothing. **`queue` gets its own rule and it is not a spelling:** `CEO queue` is BLOCKED (it is a name and the CEO's list is CEO-TODOs), the hyphenated `.ceo-queue` legacy file name is untouched, rename narration is exempt and the genuinely ambiguous `the queue`/`his queue` is REPORTED and never blocked — precision it cannot prove, it does not claim. Blank `DIALECT_TARGET` = silent no-op; a dialect with no shipped dictionary = announced no-op, never pretend enforcement |
| Definition-drift guard pair | `scripts/hooks/snapshot-agent-definitions.sh` (SessionStart) + `scripts/hooks/guard-definition-drift.sh` (PreToolUse[Agent]) | Teammate definitions in `.claude/agents/*.md` load ONCE, at session start — exactly like hooks — so a definition installed or updated mid-session never reaches a newly spawned worker's BOOTED prompt, silently. The snapshotter records every definition's sha256 at session start; the guard BLOCKS a spawn whose definition changed since, naming both hashes and the two sanctioned paths (restart into a fresh session or a live `definition-drift-ack: <current sha256>` line plus an explicit order to read the on-disk definition). Blocks only on PROVEN drift — no snapshot means no evidence, so it warns rather than halting the team |
| Workspace spec | `mega-lander/workspaces.py` + `mega-lander/workspaces.sh` + `scripts/hooks/workspace-lifecycle.sh` (SessionStart, SessionEnd, SubagentStart, SubagentStop, TaskCompleted, PostToolUse[Agent\|TaskStop\|SendMessage]) + `scripts/hooks/guard-workspace-gate.sh` (Stop) + clause 7 of `guard-worktree-isolation.sh` + `scripts/hooks/guard-sealed-worktree.sh` (PreToolUse, matcherless, first) + `scripts/hooks/observe-created-refs.sh` (PostToolUse, matcherless) | The CEO's workspace spec (`docs/plans/worktree-spec-2026-09-11.md` in this repository) and nothing else. **Two events:** a workspace is REGISTERED when its agent is spawned — a `cc/` workspace by `create-teammate-worktree.sh` before it exists, the native one at SubagentStart and PostToolUse[Agent] — and if registration fails the spawn does not happen; and when the agent's work is LANDED or DISCARDED, every workspace and branch it has is deleted, as one, after every process it started is stopped. **Finished** is a recorded fact, never a guess about liveness: the platform's own end-of-run signal (SubagentStop, a successful TaskStop) with no pause recorded before it or its session has ended — recorded by SessionEnd or read from the operating system (the recorded process number AND start time no longer match). A finished agent is refused every tool. **The guarantee:** while finished work is neither landed nor discarded, the lead is told at every turn end and at session start, through its model input, with each item's next command; nothing refuses the turn end or unrelated work (orchestration reliability R2, 2026-10-24); work already merged is landed on its own. `codex/` is never touched. A failed deletion retries on its own. Land, discard and that retry are the ONLY code in the engine that deletes a workspace or an agent's branch. Suites: `mega-lander/tests/workspaces.test.sh` (one test per point), `mega-lander/tests/workspaces-e2e.test.sh` (end to end, through the real hooks). Record: `docs/verification/workspace-spec-implementation-2026-09-11.md` |
| Starting one teammate | `scripts/spawn.sh` + `scripts/lib/spawn.py` | ONE command where there were four plus a hand-written JSON file. It resolves the repository by absolute path or by bare name; resolves the branch this work integrates on, RECORDING it when absent and CORRECTING it in place when it names a branch that no longer exists — and, where the answer is genuinely a human's to give, naming WHICH decision in one line rather than guessing (point 14); creates the isolated workspace through `create-teammate-worktree.sh`; assembles the complete Agent payload including the acknowledgement contract (by calling `prepare-agent-spawn.py`, never by copying it); and **evaluates every `PreToolUse[Agent]` guard against that payload BEFORE anything is created, from BOTH surfaces — the engine's `hooks/hooks.json` AND the governed repository's `.claude/settings*.json` — reporting ALL failures together.** Matcherless PreToolUse groups are collected too; they match every tool. The dry evaluation is honest rather than special-cased: `workspaces.py check-spawn` is `register-spawn` with the write removed, reached only when the platform minted no `tool_use_id`, so it can never turn a live spawn into an unregistered one. On ANY failure nothing is left behind — and a rollback may only undo what that run itself made (`workspaces.sh withdraw-cc`), never a workspace that already owned the name. Suite: `scripts/spawn.test.sh` (32 cases on a governed workstation, 30 on a CI runner — the two that read a governed repository's own `.claude/settings.local.json` and its brief-scope guard cannot exist on a runner, so they print `SKIP` with the reason and are counted separately, never as passes). |
| Which command is current | `ENTRYPOINTS` + `INSTRUCTION_SURFACES` in `orchestration.config` + `scripts/lib/entrypoints.sh` + `scripts/entrypoint-currency-lint.sh` + `scripts/hooks/contract-integrity-layer-ep.sh` (probe Layer EP) | **A capability can ship, work, be tested and go green while the announcement every session boots with keeps routing traffic to the thing it replaced.** `spawn.sh` landed 2026-09-13 at 23:24; the next session did not use it, paid 130 s and two sequential guard refusals for work that takes 1.3 s and reported success — because `scripts/hooks/engine-status.sh` still named the four-step path and had never heard of `spawn.sh`. Nothing was broken, both paths worked, so nothing went red and no test failed; the founder found it by paying the old cost and recognizing the number. So supersession is DATA: `ENTRYPOINTS` names, per task, **the command today and what it replaced**; the lint refuses a file in `INSTRUCTION_SURFACES` that names a superseded entrypoint while never naming its replacement and Layer EP runs it plus a two-sided sandbox canary. **It governs what we TELL people, never what is allowed** — every superseded path keeps working and is in both shipped rows CALLED by the canonical one. Scope is narrow on purpose: a reference table (this one) and a failure record name old commands legitimately and constantly and a check that swept them would be waived on the day it landed. Escape hatch: `entrypoint-exempt: <reason>` on the line; a bare marker exempts nothing. **BOTH ROOTS are scanned** — the governed repository and the engine — because in the normal by-reference deployment they are different directories and the announcement lives in the engine; a single-root scan passed by having nothing to look at. Measured on the day it landed: 2 real findings, 0 false positives, across 94 instruction surfaces in two repositories. Suite: `scripts/entrypoint-currency-lint.test.sh` (25 cases). Record: `docs/verification/entrypoint-currency-2026-09-14.md` |
| Worktree ownership ledger | `scripts/lib/worktree-ledger.py` | An advisory, append-only record several reporters read (`land-completeness`, the in-flight sweep, the liveness CLI). It decides nothing about a workspace: registration is the workspace registry's, deletion is land and discard. Contract: `docs/worktree-ownership-ledger.md` |
| Per-worktree setup | `.worktree-setup` (this engine's own is at `.richos/worktree-setup`) + step 4b of `mega-lander/create-teammate-worktree.sh` | **What every worktree of a repository needs and none of them should build for itself.** Its presence is the declaration: the creator runs it inside each new workspace, with that workspace as the working directory, right after seeding `.worktreeinclude`. RichOS points its cargo `target/` at one shared build cache there — measured 2026-09-20, a fresh checkout's first `cargo build` of the desktop shell went 58s to 19s. **Never fatal and always bounded:** a setup that fails and one that never returns, both leave a usable workspace and are reported by name, because a repository's convenience must never cost a teammate its place to stand. Resolved in `.richos/` first and at the repository root second, the order every declaration here uses. Cases C40-C43 of `mega-lander/tests/create-teammate-worktree.test.sh`, with `setup-never-run` and `setup-unbounded` in its mutation harness |
| Raw-Bash main-write guard | `scripts/hooks/guard-bash-main-writes.sh` | Blocks a raw `Bash` command that writes into a protected source tree in the shared MAIN checkout (a compound `cd <root> && mkdir/cp/rm <tree>/…` or an absolute-path write) — the cwd-default drift vector the Write/Edit guard never sees, which otherwise surfaces as an interactive permission prompt to the human operator. Auto-denies with worktree guidance so the worker self-corrects. Protected trees from `PROTECTED_PATHS`; main-root resolved via `resolve-main-checkout.sh` (no hardcoded paths) |
| In-flight sweep | `scripts/hooks/notice-inflight-sends.sh` + `scripts/hooks/guard-inflight-notify.sh` + `scripts/hooks/notice-inflight-acks.sh` (+ `scripts/inflight-notify.sh`, `scripts/inflight-ack.sh`) | Landing moves `main` and every teammate still working was cut from an older base — silently one revision behind, with nothing to tell it. The send half is ENFORCED: a PostToolUse[SendMessage] hook witnesses every message the LEAD sends (recipient + the SHAs in it; never the body), inside the lead's own tool call, so the record survives a mailbox measured at ~50% loss — and `git push origin main` is REFUSED while any live worktree is behind with no witnessed notice naming this tip, by name. The ack half is SURFACED: the teammate answers with a file in its own worktree that the lead can `stat`, because a reply the lead may never receive proves nothing; a Stop-hook notice reports a missing ack after a measured 30 minutes and names `TaskStop` + respawn as the operator's move. Nothing is killed on a timer and the one thing no machine checks — whether the teammate's restatement is CORRECT — is printed under HUMAN JUDGMENT REQUIRED rather than scored. Escape hatch is a recorded waiver, never a token |
| Escalation channel | `scripts/escalate.sh` + `scripts/lib/escalations.py` + `scripts/hooks/notice-escalations.sh` (Stop) + `scripts/hooks/session-start-escalations.sh` (SessionStart) (+ `reference/escalation-protocol-seam.md`, `scripts/install-escalation-protocol.sh`) | The in-flight sweep above is lead → teammate; this is the direction back and it had the same defect one layer worse. A teammate that finds a premise in its brief contradicted by the evidence used to write `BLOCKED.md` on its own branch — durable in exactly the wrong direction: a file on a branch is read only by whoever merges that branch, the teammate cannot see whether its branch was merged and two correct escalations sat unread for two days until a worktree cleanup found them. So `escalate.sh raise` is ONE call that writes an append-only ledger row OUTSIDE every repository, worktree and session (`~/.claude/state/escalations.jsonl`), with the worktree, branch, HEAD, teammate and repository DERIVED from the workspace — the record file under `docs/verification/` is written second and nothing depends on it. Delivery is two hooks: SessionStart puts the teammate's own question, verbatim, into the lead's context (never a count) and Stop names the condition in one line. An unacknowledged escalation is re-announced by every NEW session and gets LOUDER at 1h/24h/72h, so state-change de-duplication cannot decay into silence; it closes only on an `ack` carrying a disposition. `--state` (`work-complete` / `proceeding` / `stopped`) is required and quoted in every notice, because a mechanism that read every escalation as a failure would teach teammates not to raise them — both originals were work-complete and said so |
| CEO-ask gate | `scripts/hooks/notice-ceo-asks.sh` + `scripts/hooks/notice-ceo-unasked.sh` + `scripts/hooks/session-start-ceo-ask.sh` (+ `scripts/ceo-asks-status.sh`) | Every other mechanism here verifies the RECORD; this one verifies the CONVERSATION. A session can prepare decisions for the CEO, pass every lint and end without anyone having asked him anything — that happened, with all guards green and he had to ask three times before his own question reached him. So: a PostToolUse[AskUserQuestion] witness records WHICH prepared item a question was about, computed from the words he actually saw (question + option labels + descriptions), never from what the orchestrator asserts — and **a question matching nothing records `UNMATCHED` and discharges nothing**, which is what stops one junk question per session counting as the ask. Since 2026-10-24 nothing is refused for it (orchestration reliability R3: an unrelated prepared decision never vetoes a repair or an independent dispatch): a Stop notice tells the LEAD, through its model input, which current item has not been put to him, once per set, to ask in its next reply with the default stated and a reply that asks it counts as the ask. The dispatch gate that refused before that is removed. SessionStart opens with a specific answerable question rather than a count. Items come from the repositories named in `CEO_TODOS_REPOS`, parsed by the same code that renders the CEO's own page. No declaration = stand down silently; declared and unreadable = FAIL OPEN, loudly |
| Ingress capture | `scripts/hooks/commit-ceo-inputs.sh` (UserPromptSubmit) + `scripts/hooks/commit-ceo-inputs.py` + `scripts/hooks/notice-ceo-inputs-unheld.sh` (Stop) | **Every other mechanism here watches what the ORCHESTRATOR does. This one watches what the CEO HANDS OVER.** A specification he wrote drove changes to a public repository while the file itself sat untracked on one hard drive; he noticed and nothing else could have. So a path his message names — absolute, `~`-rooted or inside a code span, **never a bare word** — that exists on disk and that git is not holding is **COMMITTED, unmodified, on the spot**, through plumbing that leaves the working tree and everyone else's staged index untouched. **It commits rather than reminding and that is the whole design:** the first version only noticed and a mechanism whose last link is the orchestrator remembering is the same failure with a reminder attached. The judgment is made MECHANICAL out of gates this engine already ships — `scan-secrets.sh` and `guard-publication-writes.sh` are INVOKED, never reimplemented, so a detector added to either is inherited with no edit here. A credential, private material, a path at the repository ROOT, a repository that never adopted the engine, an encoding the gates cannot read, a merge in progress or a detached HEAD all mean NOT COMMITTED, said out loud, with where the file should live instead. It **never blocks**: measured against the shipped binary, exit 2 on this event ERASES the message he typed, so the only exit code is 0. **His file is never modified and no byte of it is ever printed.** The Stop partner re-reads the ledger every turn and keeps naming a refused file until git holds it, an ignore covers it or it is gone — clearing itself from the FACT, never from an acknowledgement, so a refusal cannot be announced once and forgotten. `CHECK_CEO_INPUTS=0` / `CHECK_CEO_INPUTS_UNHELD=0` stand each down, loudly; 42 cases, both directions |
| Model cost ceiling | `scripts/hooks/guard-model-ceiling.sh` + `scripts/lib/resolve-model.sh` (+ `MODEL_CEILING` in `orchestration.config`) | **Capability and cost are two different orders and declaring only the first is how a quota gets spent before anyone decides to spend it.** `MODEL_TIERS` says which model is stronger; the top alias is also roughly twice the price and burns a subscription allowance fastest. So the ceiling is declared as data — `MODEL_CEILING="opus"` — and a PreToolUse[Agent] guard REFUSES any spawn whose RESOLVED model (explicit `model:` override, else the agent definition's own frontmatter default, through the same resolver the spawn guard uses) sits on a higher TIER than that line. It compares ranks, never alias names, so a model added above the current top needs a re-derived declaration and no code edit. **The refusal CARRIES the rule instead of pointing at it** — the ceiling in one line, the is-this-a-one-off-whose-output-everything-downstream-inherits test, the worked examples, the model to drop to AND the rename that drop forces and the exact ack line — because a refusal that says "see the ruling" is one an orchestrator reads past. Escape hatch: one live `model-ceiling-ack: <reason>` prompt line, logged to `.claude/state/model-ceiling-acks.log`; a bare marker exempts nothing and neither does a bare assertion of merit ("most capable", "better", "important"), though a written sentence merely CONTAINING such a word is allowed — judging a real reason's merit is not a machine's job. Undeclared ceiling = allowed and ANNOUNCED per spawn, never silently unenforced. Probe **Layer MC** runs a three-sided canary; 58 cases + 17 mutants |
| Staging staleness | `scripts/hooks/guard-stale-staging.sh` + `scripts/staging-record.sh` (+ `STAGING_TREES`, `STAGING_DEPLOY_RECORD`, `STAGING_DEPLOY_COMMAND`, `STAGING_RECORD_REQUIRED` in `orchestration.config`) | **The rule that staging must be current before the next product work or QA run was a paragraph and a paragraph is a promise.** The first QA run after a gap meets a weeks-stale environment and returns a verdict about older code; the only thing preventing it is somebody remembering a sentence. A PreToolUse[Agent] guard REFUSES a dispatch when three facts all hold: a deploy record exists and is behind the landing branch, the commits between them TOUCH a declared product tree and THIS dispatch is product work or a run against staging. **The middle fact is the one that keeps the guard alive** — a founder ruling of 2026-09-02 says a land touching only docs, scripts or the record ships nothing and risks nothing, so a staging thirty-five commits behind on documentation is met with SILENCE, not a warning. That is a pathspec-restricted `rev-list` and its mutant is the harness's most important property: without it the guard fires on landings it was never written for, gets waived and dies. What staging has is read from a LOCAL record minted by `staging-record.sh` after the deploy's own freshness gate returns — never a network call, because the platform login is deliberately absent until product work resumes and a record written before the verdict would claim a deploy nobody finished. Adoption costs nothing: with no record yet, an in-scope dispatch is ALLOWED and announced and `STAGING_RECORD_REQUIRED=1` is the second declared line that gives it teeth. Escape hatch: one live `stale-staging-ack: <reason>` prompt line, logged; a bare marker exempts nothing. 40 cases + 12 mutants |
| Destroying live work | `scripts/hooks/guard-stop-live-work.sh` + `scripts/lib/stop-live-work.py` + `scripts/stop-work-ack.sh` | **A `TaskStop` against a teammate that is still running requires a written, checkable reason on disk — and this guard reads nobody's sentences to decide it.** On 2026-09-10 the CEO wrote that the running agents *"**might** need to be paused **if** we get close to hitting the quota"*; within a minute two mid-flight teammates were stopped and it was reported back as compliance. `echo-opus-hw2` had no commits and a clean worktree, so its entire session is gone. **The first fix carried a second clause that read his messages for a stop imperative and on 2026-09-20 it was removed** — measured that morning, it answered AUTHORIZES to *"how is the stop command coming along"*, *"tell me about stop.sh"* and *"when will the stop command be ready"*: talk ABOUT stopping, read as the order to destroy a live agent (*"When the fuck did I say 'blindly automate everything even remotely related to a stop'??????"*, ceo-decisions §67). A clause built because a kill was made on an inference from his words had re-encoded that inference as a regex. **His stop still takes seconds:** `scripts/stop.sh <names> --ceo-word "<his sentence>"` writes the ack per named target with his words in it and prints the exact calls — Rich's explicit act, quoting him, rather than a machine reading him. **What always kept the guard alive is the liveness gate, never the language.** It refuses only on a positive ALIVE verdict from `agent-liveness.py`, whose authoritative source is the worktree lock — so retiring a finished teammate, which is the overwhelming bulk of real `TaskStop` traffic, never meets the guard at all and a habit of waiving never forms. It therefore fails **open**, unlike its PreToolUse siblings: a fail-closed gate here would demand paperwork for every cleanup, which is exactly how `g11`/`g12`/`g13` died. INDETERMINATE allows **and says so** — "I could not tell" is never collapsed into "it is fine". The ack is a COMMAND rather than a marker line because `TaskStop`'s entire `tool_input` is `{"task_id": …}` (measured: 100 of 100 real calls) and the turn's prose is written to the transcript *after* the hook returns (measured with a live probe) — there is nowhere to put a marker. One target, 15 minutes, spent on first use; a bare marker exempts nothing. Measured over every real `TaskStop` on this machine — 2,499 transcripts, 100 calls, 17 sessions — **0 false positives** and the corpus now also records why a 6/100 hit rate was evidence for the wrong proposition: it showed the deleted regex agreeing with traffic that had already happened, not understanding a sentence nobody had thought to write down. Corpus: `scripts/hooks/stop-live-work.corpus.md`. 29 cases |
| Standing ownership | `owned-systems.declaration` + `scripts/lib/owned-systems.py` + `scripts/owned-state.sh` + `scripts/hooks/guard-owned-state.sh` (+ `OWNED_SYSTEMS_DECLARATION`, `OWNED_STATE_GATE` in `orchestration.config`) | **Every other mechanism here is triggered by an ACT — a spawn, a land, a commit, a turn ending. This one is triggered by nothing at all.** On 2026-09-10 the founder found, himself, a workflow red on main for thirteen days, 30 GB of worktrees nothing would reclaim and the engine's own hook text on his screen. NONE OF IT WAS ASSIGNED, so no assignment-driven machine could have found it — and the worst of it was already printed in that session's own start-up notice and was read past, so surfacing had been tried and had failed that morning. The inventory of what the orchestrator owns the health of is DECLARED AS DATA, one row per system, each with the command that decides healthy (exit 0) and the incident that put it there; a row whose `jurisdiction:` line does not hold in a repository is silent there and a check that is absent from disk is UNKNOWN and stands — never a pass. A PreToolUse[Agent] hook then tells the LEAD, once per state, the single OLDEST standing system and what clears it; since 2026-10-24 it refuses no dispatch (orchestration reliability R3: an old health incident never vetoes a repair or independent work). **The requirement is ONE, not all of them** — one disposition clears the whole session. **A disposition is the demanded system getting SMALLER, never a prompt mentioning it** (2026-09-25): until then a keyword match counted as addressing a system, the escalations row was "addressed" once in fifteen days by an unrelated dispatch and its backlog reached 143, the oldest 19 days, 13 for the CEO. Now it counts only when the check passes, its declared `measure:` falls below the baseline taken fresh at the session's first refusal, its `backlog:` past the tolerated age reaches zero or a dispatch names its own record (`triage:`, for escalations the ledger file). An escalation's age is its oldest item's own date and the session-start notice opens with the outstanding count and the count for the CEO. That bound is the whole design: this project has killed three guards by making them broad enough that waiving became the daily habit and a gate asking about six systems at every dispatch would have been the fourth. Demanding the OLDEST is what stops the cheapest-first rotation that let thirteen days happen. The `owned-state-ack:` prompt line went with the refusal. Suite: `scripts/hooks/owned-state.test.sh`; harness: `owned-state.mutation.sh` |
| 60-second proof | `scripts/demo.sh` (+ `scripts/demo.test.sh`) | One-command, unattended demo that drives the real hooks + real git mechanics through block → build → reject → fix → land, ending in a pass/fail verdict — see "60-second proof" above |
| Test runner | `scripts/run-all-tests.sh` | Runs EVERY `*.test.sh` suite in the engine, DISCOVERED from disk rather than globbed at one directory and reports a derived fraction. Exists because `scripts/hooks/*.test.sh` — the loop quoted as "18/18 suites green" at every land — omitted five suites, two of which were red on `main` for a day. Refuses to report green over an empty inventory. `--list` / `--verbose` |
| CI self-verification | `.github/workflows/engine-self-verify.yml` + `scripts/ci-verify.sh` | Ready-to-commit GitHub Actions workflow — runs `run-all-tests.sh` + `install.sh` + the probe + the demo on every push/PR, turning engine integrity into a standing guarantee. The YAML is a thin caller; **`scripts/ci-verify.sh` is the single place the steps are written down**, so the adopter template and this engine's own root workflow cannot drift (and so "what does CI do?" is one command you can run locally). **The workflow must sit at YOUR repository root**: Actions only discovers workflows in a root-level `.github/workflows/`, so a copy left under `richos/engine/` never fires — the engine's own copy sat there, unreachable and had never executed once until 2026-08-29. See "CI" below + `docs/ci-portability-notes.md` |
| CI surface, six axes | `scripts/ci-status.sh` + `scripts/lib/ci-surface.py` + `scripts/hooks/guard-ci-red-lands.sh` + `scripts/ci-surface-watch.sh` | Answers "what state is CI in?" for EVERY workflow in every governed repository, on every axis that can produce a question: **slow** (against a `# ci-budget:` the workflow declares in its own source), **red** (with the age of the streak), **skipped** (an observed skip must match a `# ci-skip: <job> — <reason>`; a bare marker declares nothing), **never-run**, **stale** (derived from the workflow's OWN triggers and resolved against the remote, so a path-filtered workflow whose paths have not changed is CURRENT rather than nagged about) and **hollow** (green that proves nothing). Repositories and workflows are enumerated FROM DISK, so a workflow added tomorrow is covered tomorrow with no edit — as UNDECLARED, which is the correct opening state. A missing declaration is a FINDING, never a pass. `guard-ci-red-lands.sh` refuses a `git merge`/`git push` into the watched branch while that repository is red, with a narrow logged `ci-red-ack:` hatch that prints its own repetition count; it is silent on every other command and structurally cannot fire when CI is clean. `ci-surface-watch.sh --install` schedules the unattended pass, which runs a classifier self-test and an end-to-end negative control BEFORE trusting its own output and ends with one `verdict:` line — because a scheduled job that RAN is not a scheduled job that WORKED |
| CI red holds the TURN | `scripts/hooks/guard-ci-turn-gate.sh` + `scripts/hooks/guard-ci-turn-gate.py` (Stop) | The third leg of the CI surface and the only one that costs something at the moment somebody is about to walk away. The land gate makes red cost something at a land and the session-start pass reports it; between them a workflow stayed red for **sixteen days**, named in every session banner and nobody moved — a report nobody is forced to read is not a mechanism. This **refuses to end the orchestrator's turn** while a commit THIS SESSION PUSHED is failing CI. The repositories are DERIVED from the transcript's own `git push` calls rather than typed; the verdict is GitHub's, about that exact commit, read from `refs/remotes/origin/<branch>` so the reader can re-run the lookup by hand. **A run still in progress does NOT hold the turn** — it holds the obligation, written to disk per (repository, commit) and re-read at every later turn-end, because blocking for the length of a twelve-shard suite is how an ack becomes reflex. **The budget is a number, not an adjective:** `BUDGET_SECONDS = 2.0`, enforced as a wall-clock deadline around every subprocess; on expiry the turn is ALLOWED with the reason printed. Unreachable GitHub never holds a turn — "could not look" is not evidence of red. A green finished commit is cached forever, so the steady state costs no API call at all. The hatch is `ci-red-ack: <repository> — <reason>` in the final message: it must name something actually red, a bare marker and the pure assertions ("fine", "known", "flaky", "unrelated") are refused, it covers ONE repository and every accepted ack is logged and counted back at the next refusal |
| Scratch that piles up | `scripts/scratch-reaper.sh` + `scripts/lib/scratch-reaper.py` + `scripts/hooks/session-start-scratch.sh` (+ the `SCRATCH_*` block in `orchestration.config`) | **Dead scratch is deleted on a schedule and every deletion is one line in `~/.claude/state/scratch-reaper.log` naming the path, the bytes and why it qualified.** On 2026-09-17 the operating system reported a full disk — 1.8 GB free of 460 GB — and 19 GB came back only because a person read the warning and deleted it by hand; the failure was never the garbage, it was that reclaiming it took a warning and a person. A thing is deleted because something PROVED nobody owns it, never because it looks old: a session is RUNNING when a session file names its process and it is still that process, ENDED by EXHAUSTION when every running session process is named by a session file and none names this one and INDETERMINATE when a running session process is named by none — which is KEPT, counted and carried in the `verdict:` line, because "I could not tell" and "it is dead" are the two things a confident wrong answer confuses. The liveness primitive is imported from `mega-lander/workspaces.py` rather than reimplemented and a failed import FAILS LOUD: a reaper that silently downgrades its own liveness check is the one shape of this program nobody could notice was broken. Four walls sit between a verdict and an unlink — containment in a declared root, no `.git` anywhere in the tree, no registered workspace inside or containing it, a never-touch list — and a wall that trips makes the entry undecidable rather than a silent skip. `--apply` is required to delete anything; the plan it prints is byte-identical to the dry run's plus one line. Every threshold carries the measurement that chose it, including the process names that ARE sessions, because the whole exhaustion argument rests on that vendor fact. `--install` schedules it (launchd, every six hours, `RunAtLoad` false — a deleter that runs the instant it is installed deletes before anybody has read its plan); the SessionStart notice is SILENT unless more than the declared threshold is reclaimable or undecidable or the scheduled job is missing or has stopped completing passes. 31 cases + 10 mutants |
| Full walkthrough | `WALKTHROUGH.md` | One feature traced through the complete lifecycle — wiki consult, real spawn, commit-is-the-handoff, single-writer land, the full 4-step QA pipeline with a genuine FIX-FIRST bounce, gatekeeper signoff — illustrative narrative, not a captured transcript; `scripts/demo.sh` is its runnable counterpart |
| White-glove onboarding | `ONBOARDING-RUNBOOK.md` | The operator's timed script for a live setup session with a non-technical CEO — preflight checklist/email template, exact commands + expected-green output per step, realistic ~60-90 min timings, the common-stall playbook and a machine-checkable definition of done |
| Guided setup | `skills/bootstrap-interview/SKILL.md` | First-session orchestrator skill: interviews the CEO (~20 min, resumable), then fills `CLAUDE.md`/`orchestration.config`, staffs the initial roster via Dean and seeds confirmed private Loro knowledge from the interview itself — the recommended path through "Adopter flow" below |
| Config | `orchestration.config` | The ONE file you edit to point the hooks at your repo (protected paths, allowlists, meta-role names, artifact dirs, optional QA gate) |
| Cost governance | `docs/cost-governance.md` | What a sprint costs in practice (order-of-magnitude reasoning, not invented pricing), why judgment roles get the Opus tier and mechanical roles don't, the per-spawn override and the levers that keep spend proportional to stakes — grounded in the engine's own role-template mix and QA-pipeline doctrine |
| Failures playbook | `docs/failures-playbook.md` | 12 generic, product-independent operational failure modes distilled from this engine's own doctrine — symptom → why it happens → the rule → how to recover. Seeds orchestrator memory (below); a memory lesson that generalizes graduates back into this playbook |
| Orchestrator memory | `docs/orchestrator-memory.md` | Rich's own persistent operational-memory convention (one file per lesson + an index) — and, front and center, the boundary table distinguishing it from Loro (durable product/business judgment) so the two substrates never get conflated |
| Versioning & upgrade path | `VERSION`, `VERSIONING.md`, `CHANGELOG.md`, `UPGRADING.md` | The packaging that lets the engine be versioned and safely upgraded: the semver scheme for a doctrine + hooks product (what counts as MAJOR/MINOR/PATCH), the engine-owned `VERSION` file the probe prints on every run, the CHANGELOG and the upgrade mechanic — which files are yours after adoption vs. engine-owned and safe to overwrite, with the golden rule "re-run install.sh + probe + demo; green = safe" |
| Land/worktree helpers | `scripts/lib/`, `scripts/collect-worktree-artifacts.sh` | Main-checkout resolver + gitignored-evidence collector |
| Working meta-workers | `.claude/agents/{dean,clark,reed,frank}.md` | Spawnable out of the box — Dean (HR), Clark (research), Reed (source reading), Frank (devil's advocate) |
| Role templates | `.claude/agents/templates/` | 16 non-live skeletons (architect, CTO, engineers, QA, designer/gatekeeper, copywriter, marketing, advisors, domain expert) Dean turns into real teammates |
| HR records | `team/` | Plain-markdown profiles for the meta-roles + a profile skeleton + starter `ROSTER.md` + the `NAMING.md` teammate-naming convention |
| Doctrine | `CLAUDE.md.template` | Rich's operating manual — generic rules intact, product sections stubbed |
| Durable knowledge | `loro/` | Compiler, writer and contracts. Actual records live in an external private corpus; read scope and confirmation rules are explicit. |
| Executive continuity | `ecs/` | Scoped operational obligations, provider observations and receipts. |
| Visibility without meetings | `ceo-briefings/` | Committed sprint/milestone briefs — shipped, in-flight, blocked, escalation-ladder decisions (with wiki citations), wiki updates, open CEO decisions. Rich-written only, under the Loro writer contract. Ships with a worked model, `ceo-briefings/EXAMPLE_BRIEFING.md` |
| Skill library | `skills/` (index: `skills/README.md`) | 25 skills total: 3 meta-role doctrine skills (worktree workflow, land sequence, bootstrap interview) + 22 domain skills (14 ship-as-is, 7 scrubbed, 1 template-only) covering QA/testing, native mobile, marketing, copywriting, Svelte and vendor integrations |
| Advanced tier (reference) | `reference/advanced-tier/` | The optional "identity-or-refuse" freshness / data-render pattern — reference only, not wired in |
| Ingestion tooling | `tools/gpt-exporter/` | GPT Exporter — a Chrome extension that exports ChatGPT threads as clean, Obsidian-compatible `.md` files (with frontmatter), for dropping into `ceo-inbox/for-wiki/` as private source material for Loro — see `tools/gpt-exporter/README.md` |

## Repository structure

Beyond the mechanical layer and doctrine files above, the engine ships a set of
pre-scaffolded directories (each tracked via `.gitkeep` where empty) so an
adopter's first spawn already has somewhere correct to write. Every one of these
is referenced by name from the doctrine or a role definition — there are no
orphan folders:

```
.
├── .claude/
│   ├── agents/                 — spawnable meta-role workers (dean/clark/reed/frank)
│   │   └── templates/          — 16 non-live role-template skeletons
│   └── worktrees/               — native per-worker worktree landing zone
│                                  (tracked via .gitkeep; CONTENTS gitignored —
│                                  each is a live worktree created/removed at
│                                  spawn/land time). The one scaffold dir that is
│                                  NOT project-specific — part of the mechanical
│                                  layer, not something to rename per domain.
├── .github/
│   └── workflows/
│       └── engine-self-verify.yml — ready-to-commit CI: suites + probe + demo
│                                  on every push/PR. MUST end up at your
│                                  REPOSITORY ROOT — Actions discovers
│                                  workflows nowhere else (see "CI" below)
├── ass-kicker/                  — agent reliability checks, coverage and tests
├── assets/                      — vendored static assets (fonts, logos, sounds,
│                                  images) — local-only by convention
├── ceo-briefings/                — committed sprint/milestone briefs (shipped,
│                                  in-flight, blocked, escalation-ladder
│                                  decisions, wiki updates, open CEO decisions);
│                                  Rich-written only, with explicit private scope;
│                                  ships with EXAMPLE_BRIEFING.md, a worked
│                                  model (delete after your first real one)
├── ceo-inbox/                    — CEO's private channel to Rich
│                                  ONLY (teammates never read it); transient —
│                                  a processed inbox is an EMPTY inbox
│   ├── for-wiki/                 — legacy intake name for private Loro sources;
│   │                              preserve citations and confirm derived knowledge
│   └── general/                  — everything else for Rich (work
│                                  requests, task context, one-off directives);
│                                  processed case-by-case
├── loro/                        — compiler, writer, contracts and empty templates
├── ecs/                         — scoped operational store and app protocol
├── reference/legacy-ceo-wiki/    — archived templates for older adopters only
├── docs/
│   ├── audits/                  — dated audit registers and findings docs
│   ├── briefs/                  — Reed's durable, committed source-reading briefs
│   ├── ci-portability-notes.md  — macOS-vs-Linux portability reasoning behind
│   │                              the shipped CI workflow (what was checked,
│   │                              what's clean, what to watch)
│   ├── cost-governance.md       — what a sprint costs, the model-tiering
│   │                              economics, and the levers that control spend
│   ├── design-system/           — the living design-system reference (components,
│   │                              tokens, canonical-state screenshots)
│   ├── failures-playbook.md     — generic, product-independent operational
│   │                              failure modes (symptom -> why -> rule ->
│   │                              recovery); seeds orchestrator-memory.md
│   ├── orchestrator-memory.md   — Rich's own persistent operational
│   │                              memory convention, and its boundary with Loro and ECS
│   └── plans/                   — architecture/migration/rollout/sprint plans
├── hr-inbox/
│   └── team-skills/             — skill packages Dean distributes to teammates
├── qa-audits/                   — committed QA audit deliverables (automation,
│                                  functional, adversarial-visual, device QA);
│                                  ships with EXAMPLE_AUDIT.md, a worked model
│                                  (delete after your first real audit)
├── reference/advanced-tier/      — optional "identity-or-refuse" freshness /
│                                  data-render reference tier (not wired in)
├── scripts/                     — mechanical hooks, land helpers, install.sh
├── skills/                      — 25 skills: 3 meta-role doctrine skills
│                                  (using-git-worktrees, rich-lander,
│                                  bootstrap-interview) + 22 domain skills
│                                  (see skills/README.md for the index)
├── team/                        — HR-record profiles + role-profile skeleton +
│                                  ROSTER.md + NAMING.md (teammate-naming convention:
│                                  the mnemonic roster — floor rules + optional hooks)
├── team-inbox/                  — staging area for sub-team-specific material
│                                  (code examples, running TODOs)
├── tools/
│   └── gpt-exporter/             — GPT Exporter Chrome extension: exports
│                                    ChatGPT threads as clean, Obsidian-
│                                    compatible .md files for
│                                    ceo-inbox/for-wiki/ ingestion (see its
│                                    own README.md)
├── ui-ux-signoffs/               — committed design-gatekeeper signoff files;
│                                  ships with EXAMPLE_SIGNOFF.md, a worked model
│                                  (delete after your first real signoff)
├── CHANGELOG.md                  — per-version contents (see VERSIONING.md)
├── CLAUDE.md.template
├── ONBOARDING-RUNBOOK.md         — the operator's timed white-glove setup-
│                                   session script (preflight, agenda,
│                                   common stalls, definition of done)
├── orchestration.config
├── README.md
├── UPGRADING.md                  — how an adopter pulls future engine updates
│                                   without clobbering their filled config,
│                                   staffed workers, and grown wiki
├── VERSION                       — the engine's semantic version (engine-owned,
│                                   one line; the probe prints it on every run)
├── VERSIONING.md                 — the semver scheme for a doctrine + hooks
│                                   engine + the release ritual
└── WALKTHROUGH.md               — one feature traced through the full
                                   lifecycle (illustrative narrative, not a
                                   captured transcript; scripts/demo.sh is
                                   its runnable counterpart)
```

Add any further repo-root directories your product needs (e.g. one per
deployable app/service) — those are yours to define, not part of the generic
scaffold.

## Adopter flow

### Recommended: clone → hooks → the bootstrap interview

1. **Clone** this engine or copy **its entire contents except its own
   `.git/`** into your existing repo's root — everything, not a hand-picked
   subset: `scripts/`, `.claude/` (**including the committed
   `.claude/settings.local.json`** — see the load-bearing warning just below),
   `.github/workflows/`, `team/`, `skills/`, `orchestration.config`,
   `CLAUDE.md.template`, `reference/`, `tools/`, `ceo-inbox/`, `loro/`, `ecs/`,
   `ceo-briefings/`, plus `.gitignore` and the `VERSION` / `CHANGELOG.md` /
   `UPGRADING.md` packaging files.
   - **`.claude/settings.local.json` is committed BY DESIGN and easy to lose.**
     It is the SOLE hook-registration source plus the two load-bearing config
     keys below — a copy or `git add -A` that skips it ships a repo that works
     locally but strands the next clone/session with **zero teammates and no
     error**. Worse, a common global-gitignore convention silently drops it
     (next paragraph) — so after copying, confirm it is actually tracked:
     `git ls-files .claude/settings.local.json` must print the path; if it
     doesn't, `git add -f .claude/settings.local.json`. The integrity probe's
     Layer N checks exactly this.
2. **Mint the sidecars:** the hooks are already wired — they register in exactly
   ONE committed file, `.claude/settings.local.json`, which Claude Code reads
   directly (no generated `.claude/settings.json`; a second copy would make
   every hook fire twice — the double-fire the probe's Layer M guards). Run
   `scripts/hooks/install.sh` once to regenerate the gitignored hook `.sha256`
   integrity sidecars from the current hook bytes (and to migrate away any stale
   hook-duplicating `.claude/settings.json` left by an older install). Some
   sidecars land outside `scripts/hooks/` — among them `mega-lander/workspaces.sh.sha256`
   and `mega-lander/workspaces.py.sha256`, for the only code that deletes a
   workspace; all are gitignored.
3. **Provision `CLAUDE.md` so a bare boot IS Rich:** copy
   `identity.config.example` to `identity.config`, fill in at least
   `COMPANY_NAME` and `CEO_NAME`, then run
   `scripts/provision-claude-md.sh`. The engine ships `CLAUDE.md.template`,
   but Claude Code only auto-loads `CLAUDE.md` — until this runs, a bare boot
   is **generic Claude**, not Rich. The script fills the template with your
   actuals, turns every `<!-- TODO (adopter) -->` block into either your value
   or an explicit "not configured — ask, never invent" note and stamps the
   result with the engine version + template hash. It is safe to re-run: an
   unedited file refreshes, a file **you** edited is never overwritten
   (`--upgrade` writes `CLAUDE.md.new` beside it to diff). `--check` exits
   non-zero if the file is missing or stale, so an installer can gate on it.
4. **Tell Rich to run the bootstrap interview:**
   `skills/bootstrap-interview/SKILL.md`. It interviews you conversationally
   (~20 minutes, structured stages, resumable if you need to stop partway),
   then does the rest of this section's old manual work FOR you: fills
   `CLAUDE.md` from `CLAUDE.md.template`, fills `orchestration.config`, spawns
   Dean to staff your initial roster from the answers, seeds confirmed knowledge in an explicitly selected private Loro corpus from the interview (with discuss-before-write
   honored, same as any other wiki ingest) and finishes with its own
   verification pass (`install.sh` + `contract-integrity-probe.sh` +
   `scripts/demo.sh`) so you see a green summary before doing anything else.
   Never fabricates an answer you didn't give — anything unanswered stays an
   explicit TODO. **Prefer voice?** macOS Dictation or Windows `Win+H` work
   today, no setup needed — or run the whole interview in any chat/voice
   assistant first via
   `skills/bootstrap-interview/references/portable-interview-prompt.md` and
   drop the resulting transcript in `ceo-inbox/for-wiki/`; the skill detects
   it automatically ("Transcript mode") and skips straight to generation.
5. **Optionally adopt the advanced tier:** if your project has a comparable
   deploy/device pipeline, adapt `reference/advanced-tier/` and flip
   `ENABLE_QA_INSTALL_FRESH_GATE=1` in `orchestration.config`. Skip it otherwise —
   the mechanical layer stands alone.
6. **Commit `.github/workflows/engine-self-verify.yml` AT YOUR REPOSITORY
   ROOT** (already in the repo, ready as-is) so the same checks keep running on
   every future push/PR — then **check `gh run list` and confirm it actually
   ran.** GitHub Actions discovers workflows only in a root-level
   `.github/workflows/`; a copy anywhere else is inert and silent, which is
   exactly how this engine's own self-verification went months without
   executing once. See "CI — the engine keeps guarding itself" below.
7. **Stay current.** The engine is versioned (see `VERSION` / `VERSIONING.md`);
   when a new release ships a hook hardening, a new skill or a fix, pull it
   with [`UPGRADING.md`](UPGRADING.md) — it spells out which files are yours
   after adoption (your filled `CLAUDE.md`, `orchestration.config`, staffed
   workers and grown `ceo-wiki/`) versus engine-owned and safe to overwrite,
   and the golden rule: after any update, re-run `install.sh` + the probe + the
   demo — all green means the upgrade is safe.

### Fallback: manual setup (no interview)

Every step the bootstrap interview automates can still be done by hand — useful
if you'd rather fill things in yourself or want to understand exactly what
the interview is doing on your behalf:

1. **Fill `orchestration.config`:** set `PROTECTED_PATHS` to your real source
   trees, adjust `READONLY_ALLOWLIST`, `ARTIFACT_MERGE_DIRS` /
   `ARTIFACT_REPLACE_DIRS` and the meta-role names if you renamed them. A
   half-configured hook fails loud, not dangerous — so fill this before relying
   on enforcement.
2. **Fill `CLAUDE.md.template`:** the mechanical way is
   `scripts/provision-claude-md.sh` (fill `identity.config` first) — it does the
   copy, the actuals and every `<!-- TODO -->` block for you and re-running it
   never overwrites your edits. By hand: copy the template to `CLAUDE.md` and
   replace every `<!-- TODO -->` block (surfaces map, team directory + routing,
   deploy table, product hard rules) **and delete the adopter header comment and
   the sample "No pagination" bullet** — anything left in place reads as your
   doctrine. Keep all the generic doctrine intact, including "The Orchestrator
   as COO" section.
3. **Choose a private Loro corpus:** use the app's memory setup or an explicit
   external corpus root for the Loro CLI. See `loro/docs/authoring.md`. Keep the
   corpus outside engine code and the public repository. Write only confirmed
   knowledge with source references; installing templates does not import context.
4. **Staff your domain team via Dean:** for each role you need, have Dean read
   the matching template in `.claude/agents/templates/` (asking Clark to research
   the role for your domain when depth helps), copy it to
   `.claude/agents/<slug>.md` with a real `name`, fill in the persona/stack/
   guardrails, write the matching `team/<name>.md` profile and register the hire
   in `CLAUDE.md` + `team/ROSTER.md`. Never spawn a template directly — instantiate
   a real named copy first.

## Verify the engine works before you rely on it

The fastest check is the [60-second proof](#60-second-proof--watch-it-work-before-you-set-anything-up)
above (`scripts/demo.sh`) — it exercises the real hooks end-to-end against a
throwaway sample and prints a pass/fail verdict. For the lower-level suites:

```bash
# EVERY test suite, discovered from disk (not globbed at one directory):
scripts/run-all-tests.sh          # add --list to see the inventory, --verbose for full output

# The wiring/integrity probe (confirms hooks are installed + chained + unmodified):
scripts/hooks/contract-integrity-probe.sh

# If you run the engine BY REFERENCE (as a plugin), name the repository it
# governs — the probe then runs the plugin-route layers instead of the
# settings-wiring ones:
RICHOS_ENTITY_ROOT=/path/to/your/repo /path/to/engine/scripts/hooks/contract-integrity-probe.sh
```

Use `scripts/run-all-tests.sh` rather than a `scripts/hooks/*.test.sh` loop. That
loop is what this section used to recommend and it silently omitted five suites
— including `scripts/demo.test.sh` and `scripts/locate-engine.test.sh`. On
2026-08-29 both of those were red on `main` for a day (the buyer-facing
`scripts/demo.sh` exited 2 during setup) while "18/18 suites green" was quoted
at every land and was, for the glob it described, entirely true. The runner
discovers suites instead of enumerating directories and refuses to report a
fraction over an empty inventory.

Green suites + a passing probe mean the framework is correctly wired standalone.
A useful smoke test: spawn a file-writing teammate WITHOUT `isolation: "worktree"`
and confirm the worktree-isolation guard blocks it. Another: try to write a file
containing an obvious API-key-shaped string and confirm `scan-secrets.sh`
blocks it (`scripts/hooks/scan-secrets.test.sh` covers this exhaustively —
every vendor pattern, every placeholder-must-not-false-positive case).

## Running the engine BY REFERENCE (one engine, many repositories)

Everything above describes the engine **seated**: copied into your repository,
registered through your `.claude/settings.local.json`. That is the right shape
for one repository. For several, copying the mechanical layer into each of them
means N copies drifting apart — and a drifted guard still reports that it is on,
which is worse than not having it.

The alternative is to load the engine **by reference**, as a Claude Code plugin.
One copy on disk; each repository decides at run time whether it is governed.

**1. Register the marketplace.** The engine ships a marketplace manifest at the
repository root (`.claude-plugin/marketplace.json`), so:

```bash
claude plugin marketplace add /path/to/this/repo
```

Use the CLI rather than hand-editing `known_marketplaces.json`: the command
reconciles the registry immediately, so the plugin is live in the **next**
session. Hand-writing the settings key alone leaves the registry unreconciled
and the plugin inert for a session, with nothing said about it.

**2. Enable it at USER scope.** In `~/.claude/settings.json`:

```json
{ "enabledPlugins": { "richos-engine@richos-local": true } }
```

**User scope is not a preference — it is the only scope that works.** The same
two keys at project or local scope do nothing (measured on 2.1.250), while the
same file's `env` block IS honored, so it is a per-key restriction rather than
a general one. The consequence matters: **the registration is operator-local and
lives in no repository**, so a fresh clone gets NO enforcement until somebody
enables the plugin. Nothing in the repository can tell you that has happened.

**3. Adopt, per repository.** A repository is governed **iff `orchestration.config`
sits at its main-checkout root**. One marker, checked one way. The plugin is
enabled once and loads in every project on the machine; which repository it
GOVERNS is decided per session from `CLAUDE_PROJECT_DIR`. Repositories without
the marker get an explicit stand-down, not silence.

**4. Confirm it, twice.** At every session start `engine-status.sh` states, to
the operator and to the model, whether enforcement is ACTIVE, STOOD DOWN or
BROKEN for this repository, which engine it came from and how it resolved the
root. For the wiring itself:

```bash
RICHOS_ENTITY_ROOT=/path/to/your/repo /path/to/engine/scripts/hooks/contract-integrity-probe.sh
```

That runs the by-reference layer set (BR1–BR9): the plugin manifest, the plugin
hook table (every guard registered exactly once, on the right event, in the
right order), path confinement to `${CLAUDE_PLUGIN_ROOT}`, sidecar hashes, the
declared meta-roles, **whether this operator's registration actually resolves to
this engine**, whether the marketplace manifest is committed, the announcement
on both channels and a live paired canary proving a guard still blocks what it
should and allows what it should. `scripts/hooks/by-reference.test.sh` is that
layer set's negative controls — every layer shown failing for its own reason.

> **Do not read `claude plugin details` as an inventory of the roles.** It
> reports `Agents (0)` for a manifest that declares agent FILE paths, because
> its inventory helper `readdir()`s each declared path and drops the resulting
> ENOTDIR. The roles load correctly — the session loader handles files
> explicitly and the host's own debug log says `Total plugin agents loaded: 4`.
> Ask the probe (BR5) or the session's agent list, not `details`.

> **Why file paths and not an `agents/` directory.** The host's agent loader
> RECURSES into subdirectories and namespaces what it finds as
> `<plugin>:<subdir>:<name>`. `.claude/agents/` here also carries
> `templates/` — sixteen non-live role skeletons — so declaring the directory
> would ship seventeen roles nobody meant to ship. BR5 fails a manifest that
> declares a directory containing subdirectories, for exactly that reason.

## CI — the engine keeps guarding itself

`.github/workflows/engine-self-verify.yml` ships ready to commit as-is — no
edits needed, nothing repo-specific to fill in. It runs on every push/PR
(`ubuntu-latest`) and repeats exactly what "Verify the engine works" above does
by hand, by calling one script:

```
scripts/ci-verify.sh
```

That script — not the YAML — is where the seven steps are written down: tool +
git-identity preconditions, `bash -n` on every shipped script, **every** test
suite via `run-all-tests.sh` (discovered from disk, never globbed at one
directory), `install.sh` to mint the gitignored `.sha256` sidecars a fresh clone
has none of, `contract-integrity-probe.sh` and `scripts/demo.sh` asserting 7/7
beats. Run it yourself before you push; CI runs the identical thing.

This converts the engine's integrity from a one-time manual check into a
**standing guarantee inside your own repo** — a deleted settings key, a modified
hook or a broken probe surfaces immediately in CI on the very next push,
instead of as a silent total failure at some future session start (exactly the
`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` incident this README warns about above).

**Put it at your repository root, then confirm it ran.** Actions discovers
workflows ONLY in a root-level `.github/workflows/`. This engine's own copy
lived under `richos/engine/` — correct as a template for an adopter who copies the
engine's contents to their root and completely inert in the engine's own
repository, where it had never executed a single time. `gh run list` returned
nothing. A workflow file is not CI; an executed run is.

The engine is developed on macOS and this runs on Linux. The first execution
found three real defects the pre-CI portability audit had missed — including one
in shipped enforcement code, not a test. `docs/ci-portability-notes.md` records
all three, what was fixed and the one layer set CI honestly cannot cover
(BR1-BR10, which need an operator's `~/.claude` registration and are covered
instead by `scripts/hooks/by-reference.test.sh`).

## Keeping the working record honest — the row-currency contract

A working list has rows; rows describe things; things change; nothing connects
the two. On one real day, four rows of one record described work as unbuilt
hours after it had landed and every one was found by a person reading the page,
because a person reading the page was the only detector there was.

Create a **`.row-currency`** file in the repository that owns your record —
either at its root or as `.richos/row-currency`, which is the same declaration
in the grouped form — and each governed row carries a **warrant**: a status
word and every
path that row describes pinned to the object id it had when the row was written:

```
| 4.2 | prose about the work | **State:** `OPEN` — `<repo>/src/parser.rs`@`0a1b2c3d4e5f` |
```

`guard-row-currency-commits.sh` recomputes those ids out of the tree each
landing is about to create. If one has moved and the row has not, the landing is
refused by item id and the refusal prints the warrant to paste. There is
deliberately **no re-stamp command**: a tool that refreshed the pin would let the
obligation be discharged with nobody reading the sentence beside it.

It fires only at a landing — main checkout, attached HEAD, `git commit` or
`git merge` — so branch work in a linked worktree is never blocked. A second,
narrower check refuses a message that NAMES an item whose row did not change.
Where the work lives in a different repository from the record, that repository
carries a one-key peer form of the same file; if the record repository is not on
the machine, the guard stands down loudly and blocks nothing.

Run it by hand with `scripts/row-currency-lint.sh <repo>` and
`--explain --message "..."` to see the claim check's reasoning candidate by
candidate. Both declaration forms and the honest list of what the rule cannot
see, are in [`reference/row-currency/`](reference/row-currency/README.md).

## Work written down instead of started — the unstarted-row sweep

Writing a row satisfies the same urge as doing the work. So a lead finds
something, records it and stops — and on one real day that happened three times
before lunch, once about a hook nobody had been asked to build. The prose fix
failed first: the amended paragraph was still on the page, in its author's own
words, when it happened twice more.

`notice-unstarted-rows.sh` runs at **every turn end**, not at a land, because
work is created when something is NOTICED and noticing happens while writing a
report. It reads the ordered backlog and the working record as one corpus — a
half corpus is loud, never a clean sweep of the surviving half — and names any
row that has nothing blocking it and no live worktree working on it.

A row goes quiet by NAMING what it waits on (the backlog's `Blocked by` cell or
`**Blocked:** <who>` in a record row) or by a live worktree claiming it (a
branch or directory containing `row-<id>` or `<worktree>/.claude/row-claims.txt`).
It
never blocks a turn and it speaks once per change rather than once per turn.

Switched on by the backlog file existing; an optional **`.unstarted-rows`**
moves that path or narrows which warrant states count. Run it by hand with
`scripts/unstarted-rows-lint.sh <repo>` — exit 1 means something is unstarted,
exit 2 means nothing was read and those are never the same answer. Full
documentation in [`reference/unstarted-rows/`](reference/unstarted-rows/README.md).

## A defect the tree can show you becomes a row — the mechanical-findings sweep

An audit found eight real defects in twenty minutes that six weeks of attention had
missed and the one that had been known for six weeks sat under a CI comment reading
"Tracked separately" — tracked nowhere. The audit ran because somebody asked; the
findings became rows because somebody typed them. Both links were a person.

`notice-mechanical-findings.sh` runs at **every turn end** and reads the record's own
declared artifact roots at HEAD for three MECHANICAL facts: a test suite skipped in a
workflow by name, a mutation harness that no script or workflow names on a code line,
and a registered hook that no test names. Each finding it does not already see in the
record becomes a row — in the record's own format, with a warrant minted by the
landing guard's own code, carrying its identity as `` `finding:<class>:<path>` `` so the
same defect on the next run is one row and not two. It never edits an existing row; it
NAMES a row whose finding has gone and a row closed over a finding still in the tree.
Whether a finding matters is never decided here and `finding-exempt: <reason>` in the
source is honored and reported.

Run it by hand with `scripts/mechanical-findings-lint.sh <repo>` (`--write` to do what
the hook does). Full documentation, including the named blind spots, in
[`reference/mechanical-findings/`](reference/mechanical-findings/README.md).

## If your repository gets published — the two publication contracts

Some repositories go public. If yours does, create a **`.publication-boundary`**
file at its root. That one committed file is the declaration and it switches on
both halves of the split — nothing is inferred, exactly as `orchestration.config`
declares engine adoption. Its own header documents every key it takes; copy this
engine's own copy, at `.richos/publication-boundary` and edit `PRIVATE_RECORD`
and `PRIVATE_SOURCES` to name where your private material actually lives.

**Where these declarations live.** Every one of them is read through
`scripts/lib/declaration-path.sh`, which looks in `.richos/` first and at the
repository root second, so both forms work and neither has to be migrated. This
repository groups its own four in `.richos/` because a public repository's root
listing is the first thing a stranger reads; see
[`.richos/README.md`](../../.richos/README.md). Declaring the same thing in both
places at once is BROKEN rather than a choice between them and a DECLARATION
in `.richos/` that no contract resolves is BROKEN too — otherwise moving one in
there would switch its contract off in silence. `.richos/` is a shared RichOS
directory, so anything in it that is not a declaration is left alone.

**Half one — may this leave?** `guard-publication-writes.sh` (at the write) and
`guard-publication-commits.sh` (at `git commit`, against the staged index, which
is where it catches files that tools generated and no Write ever touched) refuse
private material entering a published tree. They decide on CONTENT, not on file
extension: the incident that produced them was 137 files of recorded speech that
passed a "no media committed" check three times because the payload was text.

**Half one and a half — whose NAME is in this?** A name is not a secret. It has
no vendor prefix and the entropy of a person's name is the entropy of ordinary
prose, so it never trips `scan-secrets.sh`; it is nobody's recorded speech, so it
never trips the two guards above. On 2026-09-04 a third party found his own name
in this repository hours after it went public, in the source filename of a
recording named inside a test fixture's script — and he found it himself, because
nothing here was looking.

**`guard-named-persons-writes.sh`** (content AND destination path — the leak was a
filename) and **`guard-named-persons-commands.sh`** (commit message, branch name,
PR/issue/release title — the three surfaces the original scrub missed, one of
which it made *worse* by putting the same name in the commit message that removed
it from the file) refuse a listed name entering a published tree.
`scripts/named-persons.sh --tree` is the release-time backstop and
`richos/app/scripts/make-release.sh` runs it before it builds anything.

**The list itself is NOT in any repository.** A roster of your clients, friends
and family is worse to publish than any one name on it, so it lives at
`~/.richos-privacy/named-persons`, operator scope and the loader REFUSES a list
that resolves inside a git work tree. **A missing list is announced, never
assumed away** — the write-time guards say so loudly, by name, once per repository
per session and let the write through, because a stranger who clones your
repository has no such list and must not be blocked by its absence; the release
check refuses. Entries can be plaintext or `sha256:<n>:<hex>`, so a digest-only
list discloses a token count and nothing else. A `name:` entry must be two or more
tokens and matches only where they are adjacent — a bare given name cannot be
entered at all, because a deny-list that blocks ordinary prose is a deny-list
somebody switches off. Start with `scripts/named-persons.sh --doctor` and
`--mint "Firstname Lastname"`.

**Half two — is everything that must be there, there?** A leak guard is blind to
the opposite failure: the public tree claiming a capability it does not deliver.
Four of those shipped in a single day — enforcement with no declaration file to
switch it on, a mechanism left behind in the private repo, a workflow at a path
Actions never looks at and a README citing a CI file that exists nowhere. Run:

```
scripts/publication-completeness.sh
```

It derives what must exist from what your tree already declares — no list of
capabilities to maintain — and names anything missing, with the fix. `--explain`
shows what it derived and from where. `ci-verify.sh` runs it on every push, so
this is not a step anybody has to remember.

When a finding is genuinely not a defect — a reference tree whose paths describe
*your* repository rather than the engine's — say so in an optional
**`.publication-completeness`** file beside the boundary declaration, with the
reason in a comment. That escape hatch is safe to have because an entry which
suppresses nothing FAILS and names itself for deletion, so an exemption cannot
outlive its reason. The engine's own copy is at
`.richos/publication-completeness`; read it for the four keys and for what each
exemption is actually buying.

## If you redistribute other people's work — the vendoring registry

Vendoring is a decision somebody makes in thirty seconds and nobody writes down.
On 2026-09-04 an audit of this engine found that **15 of its 27 skills came from
outside the project and exactly one of those vendorings had ever been recorded —
in a commit message.** Reconstructing the other fourteen took a full agent doing
byte comparisons against upstream repositories and two verdicts came back short
of certain because by then the evidence had decayed. The same missing fact caused
a second, unrelated failure the same day: `dialect-guard.sh` Americanized four
British spellings inside a vendored MIT-licensed file, because nothing in the
repository could tell it whose bytes those were.

So create a **`.vendored-material`** file (this engine's own is at
`.richos/vendored-material`). Its presence is the declaration. It names the paths
that hold redistributable material — `REDISTRIBUTABLE_PATHS` — and carries one
tab-separated line per thing: path, origin, license, copyright holder, upstream,
revision, arrival date, confidence, what was modified and where the evidence
lives. Read the file's own header for the format; it is data, not prose, so a
guard can act on it.

**`guard-vendoring-commits.sh`** then refuses, at `git commit`, any commit that
adds files under a governed path with no entry covering them, naming the path and
the line to add. It never guesses provenance — a guard that infers origin is
confidently wrong some of the time and a wrong provenance claim is worse than an
absent one. It asks one question: was the fact written down? The escape hatch is a
`vendoring-ack: <reason>` line in the commit message, logged to
`.claude/state/vendoring-acks.log`; a bare marker exempts nothing.

**`dialect-guard.sh` reads the same registry** and leaves `third-party` entries
alone, which is the concrete payoff of the fact being machine-readable rather than
prose in a document. Material recorded as `richos` is still checked — recording
your own authorship is what lets the guard tell "we wrote this" from "nobody wrote
anything down".

## The meta-roles (ship working)

- **Dean** (`dean`, HR) — creates teammates and re-authors the role templates for
  your domain.
- **Clark** (`clark`, research) — researches the skills/knowledge a real expert in
  a role would have, for Dean to build from.
- **Reed** (`reed`, source reading) — reads long sources in full and always leaves
  a durable, committed, cited brief.
- **Frank** (`frank`, devil's advocate, opus) — brutally honest stress-testing of
  plans and decisions.

The orchestrator is **Rich** — Rich Hand, the AI executive the CEO actually
talks to. In RichOS that name is the product; if you adopt this engine into
another context you can rename him, but the "Rich only delegates, never does the
work" role is what matters, not the name.
