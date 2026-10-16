# Desktop work and authority contracts

The app's selected engine supplies a generic worker and reviewer, the work MCP
adapter and canonical Mega Lander controls. The provider starts in private app
coordination. Target repositories must be explicitly connected to the active
company. Company folder mappings alone confer no execution grant.

Workspace registries and target-worktree directories are partitioned by company and thread before provider
startup. Provider sessions share a partition only when they serve the same
thread. User instructions are attested from an exact ledger text digest; hidden
priming and provider-authored prompts never become user instructions.

## Dispatch and recovery

`richos_work.prepare` requires an open ECS obligation and the current visible
user turn. It records an intent before creating a target worktree through the
canonical spawn preparer. The exact returned Agent payload is submitted once.
A hook joins the receipt to the actual provider tool call, then canonical
lifecycle callbacks supply the real agent ID. An uncertain attempt is inspected,
not automatically dispatched again under its old key.

The worker receives a host-verified target. Direct writes outside that target,
including symlink escapes, are refused. Shell actions retain native permission
decisions; a path check is not represented as an OS sandbox. The profile excludes
terminal-wide Git configuration and uses a generic local RichOS commit identity.
It does not import an operator's terminal hooks, staff or private knowledge.

Receipt states distinguish preparing, prepared, dispatching, running, run-ended,
interrupted, unknown and integrated. A run ending does not establish success.
ECS observations use durable outboxes so a crash after the ECS commit but before
the local receipt can reconcile without a duplicate event.

## Continuation

A new worker can name `continue_of` after the original execution settles. The
adapter derives the base from the actual saved commit. Dirty files are retained
and refused before workspace creation; Rich must inspect and reconcile the
intended changes through the normal permission path. The adapter never resets,
discards or silently commits them. Canonical continuation records the original
session's exact workspace key and requires a fresh review of the revised result.

The delivered desktop execution contract joins the app's standing instruction.
Users describe their assignment; Rich owns internal obligation and receipt IDs.
Saved work summaries are available in the conversation, with observed run ends,
review verdicts and local integration kept distinct from whole-task completion.

Provider messages with a non-null `parent_tool_use_id` belong to a worker. Their
reports retain that attribution in the work journal and never become the lead's
conversation text. Nested worker results cannot end the lead turn or reset its
streaming state. Permission control messages still use the normal decision path.

## Review, integration and cleanup

A reviewer names a worker receipt. Its worktree is based on the worker's actual
clean commit. The host captures a typed verdict from that reviewer's observed
SubagentStop callback, including the exact commit and provider identity. On providers
that deliver the report through `SubagentHandback`, the host records only a successful
PostToolUse delivery from that same reviewer. A later observed SubagentStop must
settle the run before that report can authorize integration. Attempts, failed
handbacks, wrong identities and mismatched commits never establish a passing review.
An explicit malformed final verdict cannot fall back to an older passing handback.

`richos_work.integrate` requires that passing verdict, unchanged worker/reviewer
commits and clean checkouts. It persists an integration intent before a local
fast-forward. It does not push, rebase, resolve conflicts or overwrite unrelated
edits. Recovery checks Git before retrying. Mega Lander owns cleanup, with partial
cleanup reported separately from a verified integration. An explicit retry uses
the canonical deletion path and rechecks eligibility; an earlier landing receipt
does not certify that resource deletion completed.

A verified work result is not completion of every condition in a broader
obligation. The adapter does not close the entire obligation automatically.
Completed work and authority receipts remain inspectable with `include_closed`.

## Independent Rich and Stu sessions

Each conversation has an independent Rich session and an independent Stu session.
Rich handles the conversation and quick checks. He decides whether work belongs
with Stu based on the request and current circumstances. Stu executes delegated
work, coordinates specialists and reports in his own words. Neither is the
other's Claude Code teammate or subagent.

The app's durable peer inbox is the only desktop handoff path. A peer message
preserves the sender's complete text and the host-attested original user request.
The receiving host acknowledges it only after durable acceptance. Uncertain
provider delivery retains its previous native-session evidence for reconciliation.
An operational assignment record is bookkeeping, never a second dispatch channel.
Turn boundaries wake the inbox; they do not independently execute its records.
The separate declared operator client retains its explicit assignment intake.

`richos_assignments.record` opens an ECS obligation and stores the complete handoff,
original instruction reference and digest. It returns the actual assignment and
obligation IDs. `richos_status.background_work` exposes those same scoped IDs.
Stu's independent session can coordinate several obligations and is not bound to
one assignment. He supplies the selected obligation ID to `richos_work.prepare`
and `complete`. Legacy assignment-bound clients still derive that ID from their
host scope. Prepared receipts, dispatch identities, review and integration checks
continue to enforce their existing contracts.

Rich can inspect scoped operational records, work receipts and the underlying
native transcripts directly. Reading those facts does not acquire Stu's execution
lock or require a reply from him. A work record being registered does not prove
execution started. A worker ending does not prove completion. A local integration
does not prove publication.

Stu authors reports to Rich through the peer inbox. Operational result notices do
not automatically start additional Rich turns. Rich decides what the user needs
to hear and writes the reply. The application supplies observed facts rather than
writing either agent's conversational response.

## Lifecycle

The first desktop policy settles owned workers on Stop, scope change and quit.
A process supervisor also observes desktop parent death, so teardown does not
rely solely on Rust destructors running. Its live group leader reserves the
process identity until teardown. The supervisor does not restart work.

Workers must settle before the reasoning turn ends. If a provider returns while
workers remain open, the host stops its owned processes and retains workspaces
and receipts for reconciliation. Work registered with the CEO continues while the
window is closed, on the process that started it; nothing survives quit, a force
quit or a logout, and no persistent job is created for it.
Recovery never converts interrupted execution into verified completion.

A lease working outside the visible turn names its own ECS seat on every request,
one seat per assignment, so its frozen binding survives whatever is said in the
conversation meanwhile. Seats are reconciled in the same sweep as receipts: a
seat whose assignment is settled or absent is released, a seat whose assignment
is merely waiting for a decision is kept, and a seat that could not be released
is reported rather than skipped. An assignment whose workers have all stopped is
not settled -- that is the state where it is waiting for approval.

## Knowledge corrections

Operational changes use ECS checkpoint/update contracts. Knowledge changes use
the existing Loro proposal and confirmation desk. ECS never becomes a second
knowledge writer. It projects confirmed desk writer outcomes as verified
historical receipts, with scoped idempotent recovery. An unknown writer outcome
is held for reconciliation and cannot become an unanswered proposal on restart.
A bounded receipt summary accompanies rehydration; omitted receipts are available
through scoped inspection.

The native hook interface supports host context at tool boundaries. See the
[provider hooks reference](https://code.claude.com/docs/en/hooks). Actual behavior
is also exercised by the opt-in live desktop probe.
