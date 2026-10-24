# Which hooks read the CEO's words and what they decide from them

**Audited at `dd6fb305` (richos main), 2026-09-20. 172 files: every non-test
script under `scripts/hooks/` and `scripts/lib/`, plus the three `ass-kicker/`
analyzers that registered hooks execute.**

**The ruling this audit answers (CEO, 2026-09-20, verbatim):** *"since when can
a fucking hook be **automatically** inferred from my words??? since when is such
a thing become reliably possible????"* — *"WHAT THE FUCK DO I NEED **YOU** FOR IF
YOU FUCKING BUILD GUARDS THAT ARE SUPPOSED TO AUTOMATICALLY INFER **ANYTHING**
FROM MY WORDS???"*

In one line: **a hook may check what Rich DID; it never decides what the CEO
MEANT.**

## The premise this audit was given and what it turned out to be

The brief recorded, marked unverified, that the only hook reading his words was
`scripts/hooks/guard-stop-live-work.sh`, on the strength of one grep:

```
grep -lE '"role": ?"user"' hooks/*.sh lib/*.py     ->  0 hits
```

**That grep was wrong and its zero was meaningless.** The harness writes a
transcript record as `{"type": "user", ...}`, never `"role": "user"`, so the
pattern could not match the very file it was run to confirm. The audit therefore
re-derived the question from the shapes that actually occur:

```
rec.get("type")   "role": ?"user"   get("prompt"   ["prompt"]
last_user_message   promptSource   origin.kind   get("origin")   transcript_path
```

run over all 172 files, with comment-only lines discarded, then every hit read
in context. Re-runnable: the script is in the commit message of this file's
commit and each row below names the line that classified it.

## What was found

| class | count | meaning |
|---|---|---|
| **(a)** reads tool input, Rich's brief or reply or files on disk | 164 | never sees a sentence the CEO typed |
| **(a), was (b)-decides** — the removals in this pass | 2 | `scripts/hooks/guard-idle-land.py` + `ass-kicker/ass-kicker.py` |
| **(b)-quote** — reads his words, quotes them, decides nothing from them | 2 | `scripts/hooks/left-off-report.sh` + `scripts/lib/left-off.py` |
| **(b)-literal** — reads his message for a FILE PATH he typed and acts | 2 | `scripts/hooks/commit-ceo-inputs.sh` + its analyzer — **left in place, escalated** |
| n/a — writes or fixtures a transcript, never reads one | 2 | `scripts/lib/app-evidence.py`, `scripts/hooks/contract-integrity-probe.sh` |

**Exactly one live branch in the engine decided anything from his sentences and
it is gone.** `scripts/hooks/guard-idle-land.py` carried `HOLD_RE`, `OFF_DUTY_RE` and
`hold_signal()` — "hold", "stand down", "don't dispatch", "going to bed",
"calling it a night" — and TERM 3b, which stood that BLOCKING gate down on a
match; `read_turn()` collected his prompts as `said` to feed it and
`ass-kicker/ass-kicker.py` imported the same predicate for its ARM 2
`ceo-owed` verdict. Deleted, not narrowed: the regexes, the function, the term,
the collection, the import site. The return dict now has **no `said` key**, so a
caller reaching for his words gets a `KeyError` at the line that reaches rather
than a quiet empty string the behavior could grow back inside.

**What the deletion cost, measured rather than argued.** The gate's own replay
of 1,082 real orchestrator turns is in its wrapper header: `0 were held by the
operator` — **the suppressor never fired once.** What it protected is still
protected, by the orchestrator's own act: `stop-declared: ceo-owns-it — <reason>`,
written in its own final message and shown to him every time.

**The defense the deleted code made for itself and why it does not survive the
ruling.** It argued the error was one-directional: the predicate could only ever
stand a gate DOWN, never accuse anybody, so a false positive cost one un-fired
gate. That is true. It is also beside the point — **standing a blocking gate
down IS a decision** and it was being taken from a sentence he typed for a
person to read. A direction of error is a property of a mistake; the ruling is
about who is entitled to make the judgment at all.

## The four (b) rows that remain, each stated plainly

**`scripts/hooks/left-off-report.sh` + `scripts/lib/left-off.py` — (b)-quote, KEPT.** They read his
past turns and reproduce the last one **verbatim** under the heading `HIS LAST
MESSAGE BEFORE THE GAP`. Nothing is inferred from the text: the report fires on
`(now - anchor_when)` in seconds and the only content-derived branch is
`drop_current_prompt()`, a byte-identity comparison that avoids quoting his
message back to him in the same turn he sent it. Quoting him is what the brief
explicitly allows; this is the whole of what they do.

**`scripts/hooks/commit-ceo-inputs.sh` + its analyzer — (b)-literal, KEPT AND ESCALATED.** This one
is a genuine collision between two of his rulings and it is not mine to settle.
It reads his `UserPromptSubmit` message, extracts **path literals** (absolute,
`~`-rooted, backticked, quoted — `candidates()` line 235, whose own header says
`BARE WORDS ARE NEVER GUESSED AT`; there is no intent predicate anywhere in the
file) and **commits** any file he handed over that the record is not holding.
That is a decision taken from his message. It is also the mechanism he ordered
by name on 2026-09-05 — *"'it does not commit for you': Then who commits for me?
Santa Claus?"* — after a specification driving live changes to a public
repository sat untracked on his disk.

The distinction I would draw and it is a claim, not a finding: recognizing a
**path he typed** is recognizing a reference, not deciding a meaning, in the way
that `HOLD_RE` decided he had ordered a stop. But it is still his message
steering a hook, so it is named here rather than quietly kept and the call is
his. Escalation raised the same day; ledger id in the final handoff.

## Two things this audit checked because the brief named them and cleared

* **`scripts/hooks/guard-ceo-ask-first.sh`, `scripts/lib/ceo-asks.py`,
  `scripts/lib/ceo-todos.py`, `scripts/hooks/notice-ceo-asks.sh`** — a TODO is discharged by **Rich asking**, witnessed at
  `PostToolUse[AskUserQuestion]` from the call's own `tool_input` (the question
  Rich wrote). **His answer is never read** and no reply of his is parsed to
  mark anything answered.
* **`scripts/lib/ceo-ruled.sh` and its analyzer,
  `scripts/hooks/guard-ceo-ruled-ask.sh`, `scripts/hooks/notice-ceo-ruled-prose.sh`** — these match Rich's pending question against
  the **headings** of rulings in the CEO decisions page and the open-items page of the PRIVATE record
  repository and in this repository's own `CLAUDE.md`. Those are Rich-authored records and the anchor is a title, not
  his speech. the ceo-ruled analyzer's own header is explicit that it does not attempt
  to decide whether a ruling ANSWERS a question, "no text predicate can".
* **`scripts/hooks/notice-unasked-deferral.sh` /
  `scripts/hooks/guard-unasked-deferral.py`, `scripts/hooks/ass-kicker.sh`,
  `scripts/hooks/guard-brief-scope.sh`** — all classify the
  **orchestrator's** `last_assistant_message` or its Agent brief. In
  `scripts/hooks/guard-unasked-deferral.py` a `type == "user"` record is read as a **turn
  boundary** (line 367 `break`s the backward walk) and its content is never
  touched.
* **The `UserPromptSubmit` registrations** in `hooks/hooks.json` are exactly two,
  `scripts/hooks/commit-ceo-inputs.sh` and `scripts/hooks/left-off-report.sh`, both adjudicated above.

## How to re-run this

The twelve files that read the transcript and are classed (a) were each checked
twice: once for the pattern set and once for a content read of a user record —

```
grep -nE '"user"|"assistant"|type.*==.*user|role.*user' <file>
```

which returns nothing in every one of them (the single hit in
`scripts/hooks/notice-inflight-sends.sh` is the string `"user"` in a list of lead-channel
names, not a transcript read). They join on `tool_use` / `tool_result` blocks and
agent ids — Rich's acts.

## Enforcement

Hooks snapshot at session start. **The removals take effect from Rich's next
session**, not this one.

## The table — one row per file

| file | class | what text it reads | decides from it? | evidence |
|---|---|---|---|---|
| `scripts/hooks/commit-ceo-inputs.py` | b-literal | HIS PROMPT (UserPromptSubmit payload `prompt`) | YES — commits a file he named | 751 `text = payload.get("prompt")`; extraction is candidates() at 235 — path literals only (absolute/tilde/backtick/quoted), no intent predicate, `BARE WORDS ARE NEVER GUESSED AT`. LEFT IN PLACE, ESCALATED: he ordered this mechanism by name on 2026-09-05. |
| `scripts/hooks/commit-ceo-inputs.sh` | b-literal | wrapper; hands the payload to the .py | no verdict of its own | 271 `python3 "$ANALYZER"` |
| `scripts/hooks/contract-integrity-layer-ep.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/contract-integrity-probe.sh` | n/a | builds a fixture transcript | no | 2960 `{"type": "user", "promptId": pid, ...}` inside a heredoc fixture |
| `scripts/hooks/detect-nonnative-worktree.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 265, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/dispatch-pretooluse.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/engine-status.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-agent-state-claims.py` | a | the transcript, joined on TOOL CALLS / agent ids | no | 1 non-comment hit(s), first: 251:    transcript = payload.get("transcript_path") |
| `scripts/hooks/guard-agent-state-claims.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-bash-main-writes.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-pierce.sh` | a | tool input / files on disk / its own state | no | added 2026-10-06, after this audit; fixed Python helper invocation only |
| `scripts/lib/pierce.py` | a | TOOL INPUT — the orchestrator's own Agent brief | no | added 2026-10-06, after this audit; reads `tool_input.prompt`, the brief Rich wrote, never a message the CEO typed |
| `scripts/hooks/guard-brief-scope.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at -, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/guard-ceo-ask-first.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 225, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/guard-ceo-ruled-ask.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-ceo-todos-commits.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-ci-red-lands.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-ci-turn-gate.py` | a | the transcript, joined on TOOL CALLS / agent ids | no | 8 non-comment hit(s), first: 368:def observe_pushes(transcript_path, state, budget): |
| `scripts/hooks/guard-ci-turn-gate.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-completeness-commits.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-definition-drift.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 238, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/dialect-guard.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-hook-registration-commits.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-host-display-power.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 507, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/guard-idle-land.py` | a (was b-decides) | tool calls + host notices | no — his words are read and discarded at the collection site | 233 keeps only MACHINE_PROMPT_RE matches; TERM 3b and hold_signal() DELETED this commit; the return dict has no `said` key. |
| `scripts/hooks/guard-idle-land.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-inflight-notify.sh` | a | the transcript, joined on TOOL CALLS / agent ids | no | 1 non-comment hit(s), first: 295:tpath = str(d.get("transcript_path", "") or "") |
| `scripts/hooks/guard-interactive-prompt.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-main-checkout-writes.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-model-ceiling.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 274, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/guard-named-persons-commands.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-named-persons-writes.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-no-home-network-phone.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 422, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/guard-owned-state.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 296, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/guard-public-record-repo.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at -, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/guard-publication-commits.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-publication-writes.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-reference-ledger.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at -, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/guard-resume-isolation.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 209, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/guard-row-currency-commits.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-sealed-worktree.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at -, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/guard-stale-staging.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 323, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/ass-kicker.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/ass-kicker.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-stop-live-work.sh` | a | tool input + the ack ledger | no | same landed change; not touched by this audit |
| `scripts/hooks/guard-unasked-deferral.py` | a | the orchestrator's last_assistant_message | no — his turn is a BOUNDARY, never content | 367 `if rec.get("type") == "user": ... break` (stops the backward walk); 416 `text = payload.get("last_assistant_message")` is the classified text |
| `scripts/hooks/guard-unresolved-claims.py` | a | the transcript, joined on TOOL CALLS / agent ids | no | 1 non-comment hit(s), first: 1151:    blob, turn_tools = read_transcript(payload.get("transcript_path"), |
| `scripts/hooks/guard-unresolved-claims.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-vendoring-commits.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-workflow-ban.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-workspace-gate.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/guard-worktree-isolation.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 439, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/guard-worktree-removal.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/handoff-facts-annotate.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/install.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/left-off-report.sh` | b-quote | HIS PROMPT + his past turns | no — timestamps decide, text is quoted | 339 `d.get("prompt", "")`, passed to `scripts/lib/left-off.py` as --prompt |
| `scripts/hooks/notice-ceo-asks.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/notice-ceo-inputs-unheld.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/notice-ceo-ruled-prose.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/notice-ceo-unasked.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/notice-claim-capability.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/release-land-leases.sh` | a | files on disk / its own state (lease files, the repository's Git state) | no | added 2026-09-24 after this audit, registered on Stop; reads no prompt and no transcript |
| `scripts/hooks/operator-claim.sh` | a | its own session record / the operator claim file / the entity's launcher | no | added 2026-09-25 after this audit, registered on SessionStart; reads the payload's cwd and source only, no prompt and no transcript |
| `scripts/hooks/guard-operator-claim.sh` | a | tool input (tool name, target path, command) / the operator claim file / its own session record | no | added 2026-09-25 after this audit, PreToolUse on Agent, SendMessage and the Bash and Write chains; reads no prompt text and no transcript |
| `scripts/hooks/guard-shared-writes.sh` | a | tool input (target path) / lease files / the target repository's Git state | no | added 2026-09-25 after this audit, a module of the Write chain; reads no prompt and no transcript |
| `scripts/hooks/release-shared-writes.sh` | a | tool input (target path, tool_use_id) / lease files | no | added 2026-09-25 after this audit, PostToolUse and PostToolUseFailure; reads no prompt and no transcript |
| `scripts/hooks/guard-live-names.sh` | a | tool input (the spawn's name) / the workspace registry | no | added 2026-09-25 after this audit, PreToolUse[Agent]; reads the name only, never the prompt's words |
| `scripts/hooks/notice-unanswered-question.sh` | a | the transcript: the lead's own `QUESTION FOR YOU:` final message; turn starts as boundaries | no — his prompt is a BOUNDARY (turnOrigin human), never content | added 2026-10-01 after this audit, registered on Stop; scripts/lib/blocking_ask.py `unanswered` reads assistant text and each row's turnOrigin, never the text of his prompt |
| `scripts/hooks/notice-disk-alert.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/notice-escalations.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/notice-hook-staleness.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/notice-inflight-acks.sh` | a | the transcript, joined on TOOL CALLS / agent ids | no | 1 non-comment hit(s), first: 187:    d=json.load(sys.stdin); print(str(d.get("transcript_path","") or "") if isinstance |
| `scripts/hooks/notice-inflight-sends.sh` | a | the transcript, joined on TOOL CALLS / agent ids | no | 1 non-comment hit(s), first: 200:            team_dir, str(payload.get("transcript_path", "") or ""), session_id) |
| `scripts/hooks/notice-mechanical-findings.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/notice-protected-ref-moves.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/notice-unasked-deferral.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/notice-unlanded-branches.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/notice-unstarted-rows.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/notice-waiver-repetition.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/notice-waiver-repetition.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/observe-created-refs.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/ref-transaction-forensics.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/scan-secrets.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/session-start-ceo-ask.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/session-start-ci-surface.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/session-start-escalations.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/session-start-scratch.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/session-start-quota.sh` | a | files on disk (the status-line payload, one config line) | no | added 2026-09-25, after this audit; 0 hits for the pattern set in it, `scripts/quota-watch.sh` and `scripts/lib/quota_watch.py`. It PRINTS ruling §87 as a fixed quotation and reads none of his words |
| `scripts/hooks/session-start-stall.sh` | a | files on disk (whether an orchestration.config exists) | no | added 2026-09-28, after this audit; 0 hits for the pattern set in it, `scripts/stall-watch.sh` and `scripts/lib/stall_watch.py`. It PRINTS a fixed notice and reads none of his words |
| `scripts/hooks/shell-evidence.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/shell-evidence.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/strip-ack-lines.py` | a | tool input (the spawn prompt) | no | added 2026-10-06, after this audit; it removes fixed acknowledgment-prefix lines from the prompt and reads none of his words |
| `scripts/hooks/strip-ack-lines.sh` | a | tool input / files on disk / its own state | no | added 2026-10-06, after this audit; fixed Python helper invocation only |
| `scripts/hooks/snapshot-agent-definitions.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/snapshot-enforcing-hooks.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/task-completed-handoff.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/teammate-idle-handoff.sh` | a | the transcript, joined on TOOL CALLS / agent ids | no | 1 non-comment hit(s), first: 139:    "transcript_path": payload.get("transcript_path") or "", |
| `scripts/hooks/turn-manifest.py` | a | the transcript, joined on TOOL CALLS / agent ids | no | 2 non-comment hit(s), first: 204:        return [], {}, 0, "the Stop payload carried no transcript_path" |
| `scripts/hooks/turn-manifest.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/verify-agent-prompt.sh` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 184, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/hooks/worker-created-handoff.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/worker-ended-handoff.sh` | a | the transcript, joined on TOOL CALLS / agent ids | no | 2 non-comment hit(s), first: 191:if payload.get("agent_transcript_path"): |
| `scripts/hooks/worker-started-handoff.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/worker-updated-handoff.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/hooks/workspace-lifecycle.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/agent-liveness.py` | a | the transcript, joined on TOOL CALLS / agent ids | no | 8 non-comment hit(s), first: 294:def agent_spawns(transcript_path, limit_bytes=64 * 1024 * 1024): |
| `scripts/lib/agent-liveness.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/app-evidence.py` | n/a | WRITES synthetic transcript records for tests | no | 33/57 `record.update(type="user", ...)` — a generator, not a reader |
| `scripts/lib/appinstances.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/assert-own-worktree-registered.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/ceo-asks.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/ceo-asks.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/ceo-ruled.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/ceo-ruled.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/ceo-todos.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/ceo-todos.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/ci-receipts.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/ci-red.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/ci-run-records.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/ci-surface.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/ci_pause.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/completion-proof.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/containers.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/declaration-path.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/disk-watchdog.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/durable-filesystem-identity.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/entrypoints.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/escalations.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/escalations.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/git-jurisdiction.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/global-state-witness.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/hook-dependencies.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/hook-dependencies.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/inflight.py` | a | the transcript, joined on TOOL CALLS / agent ids | no | 4 non-comment hit(s), first: 343:def identity_index(teams_dir, transcript_path="", session_id=""): |
| `scripts/lib/inflight.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/interactive-prompt.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/land-completeness.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/land-residue-gate.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/leak-canary.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/left-off.py` | b-quote | HIS TURNS in the transcript | no — the gap is seconds; the text is reproduced VERBATIM for him to read | 269 is_human(); 350 find_gap() branches on (now - anchor_when) only; 952 `HIS LAST MESSAGE BEFORE THE GAP, VERBATIM`. The one content-derived branch is drop_current_prompt() at 324, a byte-identity comparison. |
| `scripts/lib/mechanical-findings.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/mechanical-findings.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/model-tiers.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/mutation-harness.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/mutation-pool.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/named-persons.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/named-persons.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/owned-dispatch.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/owned-session.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/owned-systems.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/premise-ask.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/premise-ask.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/protected-ref-moves.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/publication-boundary.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/publication-boundary.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/record-canary.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/registered-hooks.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/resolve-main-checkout.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/resolve-model.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/resolve-roots.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/row-currency.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/row-currency.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/sandbox-completeness.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/scratch-reaper.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/scratch.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/seat-jurisdiction.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/spawn-guard-audience.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/spawn.py` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 829, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `scripts/lib/stop-hook-notice.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/stop-live-work.py` | a | tool input + the stop-work-ack ledger | no — authority B was removed at dd6fb305 by zach-opus-stopguard1 | no authorizes_stop, no last_user_message, no --transcript/--user-text |
| `scripts/lib/stop.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/stopwatch.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/teammate-identity.py` | a | the transcript, joined on TOOL CALLS / agent ids | no | 4 non-comment hit(s), first: 155:def resolve_transcript(transcript_path="", session_id="", teams_dir=""): |
| `scripts/lib/teammate-name.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/tree-witness.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/unevaluated-notice.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/unlanded-branches.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/unstarted-rows.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/unstarted-rows.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/vendored-material.sh` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/workspaces.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `scripts/lib/worktree-ledger.py` | a | tool input / files on disk / its own state | no | 0 non-comment hits for the pattern set |
| `ass-kicker/ass-kicker.py` | a (was b-decides) | tool calls + the orchestrator's own final message | no | 614: the idle.hold_signal(turn["said"]) call DELETED this commit |
| `ass-kicker/brief-scope.py` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 876, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |
| `ass-kicker/brief-provenance.py` | a | TOOL INPUT — the orchestrator's own Agent brief | no | `ti.get("prompt")` at 1448, where `ti` is tool_input: the brief Rich wrote, not a message the CEO typed |

## Addendum, 2026-09-26: two hooks added after this audit, classified the same way

**Not audited at `dd6fb305`.** These rows were added when the hooks were written, by their author,
using this page's classes. They are a declaration for the next audit to check, not a finding of
this one.

The operator asked, on 2026-09-26, what must always happen whenever he mentions the words "type"
and "failure" close to each other. That rule is stated in terms of his WORDS, so reading his
message for those two words is the rule itself, not an inference about what he meant. The class is
the one this page already uses for `scripts/hooks/commit-ceo-inputs.sh`: a literal reference
recognized in his message, acted on. Nothing in either file classifies intent, tone or topic and
neither decides which failure type applies; that judgment stays with the lead.

| file | class | reads | decides from his words? | evidence |
|---|---|---|---|---|
| `scripts/hooks/failure-type-lookup.sh` | b-literal | HIS PROMPT, for two words | yes, literally: whether an inflection of "type" and one of "failure" are within six words; if so, the register's list is injected and an obligation recorded | `scripts/lib/failure-type.py` 410 `prompt = payload.get("prompt")`, 413 `trigger(prompt)` |
| `scripts/hooks/guard-failure-type-answer.sh` | b-literal | his turns in the transcript (origin human, the `left-off.py` predicate), for the same two words; everything else it checks is the lead's tool calls, reply and git | yes, literally, the same predicate; the refusal itself is decided by what the lead did (read, named, committed) | `scripts/lib/failure-type.py` 522 `lo.is_human(rec)`, 665 `trigger(text)` |
| `scripts/lib/failure-type.py` | b-literal | both of the above | as above | the predicate is `trigger()`; its window and word lists are constants at the top of the file |

## Addendum, 2026-09-27: one hook added after this audit, classified the same way

**Not audited at `dd6fb305`.** This row was added when the hook was written, by its author, using
this page's classes. It is a declaration for the next audit to check, not a finding of this one.

The gate never reads the operator's messages or the transcript. It reads wait records, the process
table, the test VM's state files and the escalation ledger; the only prose it reads is escalations
TEAMMATES raised, for a literal claim that the teammate itself is waiting (`WAIT_CLAIM`, calibrated
on the real ledger). His rule is quoted in its header and refusal; it is not a predicate.

| file | class | reads | decides from his words? | evidence |
|---|---|---|---|---|
| `scripts/hooks/guard-resource-waits.sh` | a | payload `agent_id` / `stop_hook_active`, orchestration.config; hands the rest to the .py | no | the wrapper passes `RESOURCE_WAIT_MINUTES` and reads one `RW` verdict line |
| `scripts/lib/resource_waits.py` | a | wait records, `ps`, `<TESTVM_ROOT>` state and teammates' escalation titles, questions and meanwhile lines | no: a teammate's literal wait claim, never his message | 568 `WAIT_CLAIM.finditer(whole)`; 925 `payload.get("agent_id")` is the only payload field it decides on |

## Addendum, 2026-10-09: one hook added after this audit, classified the same way

**Not audited at `dd6fb305`.** This row was added when the hook was written, by its author, using
this page's classes. It is a declaration for the next audit to check, not a finding of this one.

The hook hands a second-review verdict to the running teammate whose work it is. It never reads the
operator's messages or any transcript; it reads the payload's `agent_id`, second-review's verdict
ledger and the workspace registry and it never blocks.

| file | class | reads | decides from his words? | evidence |
|---|---|---|---|---|
| `scripts/hooks/deliver-review-verdict.sh` | a | payload text for `"agent_id"`, the review ledger's presence; hands the rest to the .py | no | the wrapper exits 0 on every path |
| `scripts/lib/review_delivery.py` | a | payload `agent_id`, `<state>/reviews.jsonl`, the verdict records, the workspace registry | no | `payload.get("agent_id")` is the only payload field it decides on |
