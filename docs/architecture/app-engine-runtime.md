# App-owned engine runtime

Target: app 1.2.0 and engine 1.2.0. Initial installed acceptance is macOS.

## Ownership

The app ledger owns conversation and tool evidence. ECS owns typed obligations.
The provider owns observed execution state. Mega Lander owns workspace identity
and eligibility for cleanup. Git owns integration and publication facts. Loro
owns durable knowledge. Existing ASS Kicker analyzers own their control decisions.
An integration adapter transports facts between these authorities; it does not
create alternative registries or infer success from a worker's final sentence.

Engine code is read-only. Private app state includes a neutral coordination Git
repository, hook-evidence projections and an explicit ECS store. Registered target
repositories and their worktrees are separate. The Loro corpus is separately
selected private storage. No installed component searches a developer checkout
for missing implementation or imports personal context during setup.

## Native delivery

Keep the native stream-json protocol, `--setting-sources ''` and
`--no-session-persistence`. Deliver an explicit app-owned plugin. Do not rely on
terminal plugin registration. A valid plugin manifest alone does not prove that
its hooks or workers execute.

On Claude Code 2.1.273, no-session-persistence callbacks supply prompt and tool
IDs but their native transcript paths are unreadable. `scripts/lib/app-evidence.py`
projects actual callbacks into the shape the existing analyzers read. The
projection is private, scoped by session and prompt and separates worker
sidechains. Attempts have no result until a result callback arrives. Runtime
input is not automatically labeled a direct user instruction. The app ledger
remains the authority for instruction provenance.

The app must keep servicing permission requests after the lead returns a result
while a worker remains active. Native permission callbacks can enforce a denial,
but the provider does not request permission for every operation. App access
policy therefore also needs the PreToolUse boundary; connecting a repository or
reading a quoted approval cannot grant arbitrary execution authority.

The app's ordinary Stop fences new dispatch before interruption. On quit, settle
owned provider process groups within a bounded grace period, then stop only those
groups. Persist uncertain worker outcomes for reconciliation on restart. A dead
process does not establish successful completion or safe worktree deletion.

## Component processes and delivery

Use the reviewed Python ECS event store behind a versioned local JSON command
interface. The app supplies the state root and entity/thread/session binding
explicitly, and every caller that is not the one legacy cursor also supplies its
own seat: the active context is one cursor per seat, each front desk rewrites its
own row every turn, and both a background lease's binding and a parallel
conversation thread's have to outlive the turn beside them. His seat is one per
conversation thread (`ceo-thread:<thread_id>`), a lease's is one per assignment. A seat
outlives the turn that made it, so both kinds are reconciled: a lease's against its
assignment's state, one of his against whether that conversation thread still exists —
which is the app's own knowledge and is read from the app's conversation ledger, never
inferred from the store. An unreadable ledger reports the seat as unreconciled and
releases nothing, and the release of one of his carries the revision it was enumerated at,
so the store refuses a seat whose thread has bound a turn since. There is no
terminal-cwd inference at this boundary. Each invocation
has a bounded request and response. SQLite transactions and idempotency receipts
protect local changes. Operations spanning ECS, Loro and Git use intent and
reconciliation rather than a claimed shared transaction.

Deliver verified Python, Node and the engine's command dependencies with the
macOS candidate. Do not rely on Homebrew or an OS Python. Keep runtime versions
and component schemas in the compatibility manifest independently of app/engine
1.2.0. Actual clean-install runtime delivery remains an installed acceptance gate.

Account connection uses the unmodified provider's `auth login --claudeai` flow
from the app. The provider owns the browser handoff and credentials. Read only
the authentication-success result needed for readiness, discard account details
and never persist tokens. Cancellation and return require app-level tests.

## Feasibility evidence

`richos/app/scripts/probe-engine-runtime.py` is an opt-in live probe requiring an
authenticated provider. It creates disposable fictional repositories, uses the
native argument vector, loads a synthetic agent and invokes canonical engine
mechanisms. It does not activate an installed engine or modify user repositories.

On macOS 15.6 arm64 with Claude Code 2.1.273, the probe demonstrated an enforced
canonical stated-action refusal, a corrected Stop, native permission allow and
deny responses and a real worker writing the expected artifact in a registered
cross-repository worktree. Its native worktree remained in the coordination
repository. The target's main checkout remained untouched. Paths contained spaces.

The probe also exposed a full-line path parsing defect in the registry, spawn
preparer and isolation guard. `mega-lander/tests/path-spaces.test.py` covers the
fix with a registered positive case and a refused shortened path.

This is runtime feasibility evidence. It is not installed-app acceptance, proof
that every hook is delivered or proof of an end-to-end assignment workflow.

## Desktop profile and action decisions

The desktop factory now loads a generated profile using the selected delivered
runtime. The profile separates shipped code, app-owned coordination and connected
repositories. Repository connections are explicit registry facts; ordinary company
folder mappings do not become execution grants when an older registry is loaded.

The desktop requests the provider's `auto` permission mode and verifies that the
native initialization frame reports it before visible work starts. The generated
settings retain the classifier defaults and add only the selected company's
connected repositories and this thread's target-worktree directory. Shell commands
remain subject to classification; file reads outside registered working directories
remain subject to the provider's access checks. These settings are not an OS sandbox.
Publication requires the user's operation and destination, independently of a
request for local implementation. There is no bypass-mode fallback.

Native callbacks that still need a decision use the scoped app desk. They display
the exact input and the provider's reason when supplied. Validated host tools retain
their explicit contracts. Stop revokes the visible-turn grant before acknowledging
cancellation, and the independent PreToolUse fence also checks that grant. An old
request cannot revive a stopped turn.

Claude Code 2.1.273 permits direct Git checks automatically but routes some compound
shell forms to a `safetyCheck` with `classifier_approvable: false`. The shipped roles
use direct commands and literal target paths for routine checks. A PreToolUse
format check rejects shell-variable Git targets before permission evaluation;
it neither executes a replacement nor grants approval. A corrected command still
passes the normal provider checks. A denied permission is not retried through
another command. The opt-in `work_roundtrip --permissions`
probe checks direct commands and an intentionally denied compound command. The
`RICHOS_PROBE_NO_MANUAL=1` natural-assignment probe rejects any manual request.
Provider availability and future behavior still require the installed release gate.
See the provider's [auto-mode configuration](https://code.claude.com/docs/en/auto-mode-config).

See the [intermediate profile verification](../verification/desktop-engine-profile-2026-09-16.md)
for the tested scope and remaining integration work.

## Speech decoder startup

The pinned runtime decoder must actually initialize before the first user utterance.
A fresh decoder invocation measured 9.764 seconds versus under one second when warm
on the physical Mac. Download verification, metadata probes and cached calibration do
not cover this cost. Read [the startup requirement and regression checks](../development/spoken-reply-latency.md)
when changing decoder packaging or its initialization path. Rust setup starts it silently
as soon as the delivered runtime is known, before frontend state reads. Readiness shares
the same warmup, and starting capture joins it before opening the microphone.
