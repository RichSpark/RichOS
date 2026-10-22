# Desktop work and authority contracts

The app's selected engine supplies a generic worker and reviewer, the work MCP
adapter and canonical Mega Lander controls. The provider starts in private app
coordination. Target repositories must be explicitly connected to the active
company. Selecting a folder when adding a company connects it through the same
validated path as Settings, including Git tracking. Inherited folder mappings
alone confer no execution grant.

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

Conversation recovery first applies durable user Stops. A saved in-flight request
continues automatically through one fsynced ledger replay transition, preserving
its original text, channel and pressed-control provenance. Repeating recovery does
not manufacture another visible user message. The previous native sessions, partial
replies and unknown delivery outcomes reach Rich as separate facts in the same visible
request. He reconciles prior effects before continuing. This is a new delivery
attempt of the saved request, not new user authorization or a blind worker retry.

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
An unfinished native delivery remains recoverable even if a live correction has
advanced the accepted inbox cursor. Recovery reads that exact original record
within the existing frame bound and retains the previous native session for
reconciliation. Accepted corrections are not replayed as new work.

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

The selected assignment's original ledger reference and attested text remain its
user authority even after another peer input changes Stu's current scope. Worker
preparation and dispatch review carry that original request. Current conversation
constraints still apply. A mismatched original request refuses preparation.

`richos_work.report` updates the existing operational record without dispatching
work or messaging Rich. It cannot mark unresolved execution failed. Code completion
still requires the complete independently reviewed, integrated and cleaned worker
set. An answer-only completion refuses implementation work or unresolved execution.
Actual worker starts update their assignment's observed state. Inspection never
manufactures a new activity timestamp or replaces Stu's own progress report.

A user Stop is recorded durably before cancellation. For independent Stu, the host
persists revocation before waiting for the existing receipt lock, takes its launch
snapshot under that lock, then sends the existing native per-task stop control only to that work's observed
tool calls on their owning session. Missing delivery or ending evidence stays
unknown. The canonical hook uses the native background-task option for Stu's own
Bash calls, preserving the command and its other arguments; interrupting the
session would also stop unrelated workers. Every shell command Stu's own model
starts, foreground or background, is a native task with its own id (Claude Code
2.1.307 answers `stop_task` for a foreground one and ends only that command; the
turn goes on). The job travels with the command: Stu begins each command's
description with the job's obligation_id (stu.md) and the host also stops those
of his running commands that name the stopped job, never one that names another
job. A command that names no job is stopped only when it started after the job was
registered and no other open job could be running it (one waiting on the user,
the screen or the allowance with no change since cannot); the rest are left to
Stu. When Rich runs a job's commands himself in the turn that gave the job,
the Stop also ends those his own model started after the job was registered,
while that turn still runs: never one naming another job and one naming no job
only when no other job came from the same turn. The Stop fact prevents subsequent preparation, dispatch and worker actions
for that obligation. Stu receives typed user-control facts through the same inbox;
other assignments and Rich's conversation keep their own execution ownership.

## Lifecycle

The first desktop policy settles owned workers on Stop, scope change and quit.
A process supervisor also observes desktop parent death, so teardown does not
rely solely on Rust destructors running. Its live group leader reserves the
process identity until teardown. The supervisor does not restart work.

Workers must settle before the reasoning turn ends. If a provider returns while
workers remain open, the host stops its owned processes and retains workspaces
and receipts for reconciliation. Work registered with the CEO continues while the
window is closed, on the process that started it; nothing survives quit, a force
quit or a logout and no persistent job is created for it.
Recovery never converts interrupted execution into verified completion.

A lease working outside the visible turn names its own ECS seat on every request,
one seat per assignment, so its frozen binding survives whatever is said in the
conversation meanwhile. Seats are reconciled in the same sweep as receipts: a
seat whose assignment is settled or absent is released, a seat whose assignment
is merely waiting for a decision is kept and a seat that could not be released
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

Execution handoff uses `richos_status.delegate`, shared by automatic intake routing
and Rich's manual decision. The original user ledger reference identifies the work
in the existing peer inbox. Its existing append lock covers lookup, append and
fsync, so simultaneous calls or retries return the same saved handoff. A new user
turn with identical words remains distinct. Assignment records cannot dispatch it.
New execution requests wait for Stu's next prepared turn; corrections and reports
still use live input. Neither agent is the other's provider teammate.
A queued execution request does not block later live communication. The existing
inbox cursor retains out-of-order acceptance receipts and advances only across
consecutively accepted frames. A correction can be accepted while an earlier new
job stays queued and restarting cannot lose the job or replay the correction.

Recovered deliveries retain the initial user turn's ledger reference for request
authority and execution handoff. The continuation's own turn ID still identifies
its native delivery, questions and lifecycle. Questions and answer deliveries keep their
selected obligation separately from the Stu recipient. Stop closes only that
obligation's questions and discards its untaken answers. An answer restores the
asking job's original authority, even when another request reached Stu later. Recovery never creates new authority
for an existing user request.

Before a visible user request reaches Rich's inference, native request handling
asks a dedicated Haiku 5.5 classifier for the next action. It uses the same Claude
executable, selected account and delivered runtime as the conversation, with no
work tools, hooks, memory files or extra service account. Its plain purpose is in
`engine/app/routing.md`; the loaded Rich/Stu definitions supply their roles.

The decision is whether this message requires a new Stu execution handoff or
Rich can handle its next step quickly. A correction to ongoing work can be quick
communication by Rich without creating another job. The full original request
and bounded recent visible conversation are supplied from the existing scoped
ledger projection. Hidden preparation, peer-input bodies and technical commentary
are not reconstructed from the native transcript.
The classifier cannot dispatch. Native handling saves a Stu decision through the
same inbox operation as Rich and supplies factual ownership with the unchanged
user message. No extra Rich context-only model request is sent.

One classifier process is retained per native conversation lease and ends with
that lease through the existing provider supervisor. It uses the user's existing
Claude account limits. The first request starts it; subsequent requests reuse it.
A five-second deadline, cancellation, unavailable model, invalid output or
uncertainty leaves Rich's normal judgment available. A failed process is retired
before reuse so a late result cannot classify the next message. A saved Stu
handoff retains ownership across those failures and restart. Stop is checked again
under the existing send lock before saving a new handoff. Peer reports and hidden
preparation never trigger classification. No subsidized routing service, local
model download or additional socket server is involved.

Rich's conversation lease uses Claude Code's native formatted-output contract with
one string field, `reply`. That field streams to the conversation, speech and
phone through the existing text path. Ordinary assistant text stays in the
existing technical machinery records. No wording heuristic decides what is
commentary. Missing or invalid formatted output is an explicit provider failure,
not permission to publish the untyped fallback. Valid native formatted completion
qualifies `end_turn` even when its underlying model stop reason is `tool_use`.

The provider's local `StructuredOutput` formatter performs no work. The canonical
hook records its evidence and permits the main session to format output during
hidden preparation or after an interruption without opening any work grant.
Shell, reads, worker dispatch and other actions still require their existing
authority. Hidden preparation remains internal. Partial JSON, escaped strings,
Unicode, duplicate complete frames and cancellation share the same reply path.
