//! The NATIVE CLIENT — RichOS drives the `claude` binary directly over stream-json stdio.
//!
//! **This file replaces `acp.rs`.** CEO ruling 2026-08-31, `wiki/ceo-decisions.md` §16:
//! *"the adapter goes"*. RichOS is Claude; the ACP adapter's whole value was portability to
//! backends this product has declined, and it cost 112 resolved npm packages across ~99
//! publishers that the CEO's own Developer ID would have vouched for.
//!
//! Same transport SHAPE as the file it replaces — newline-delimited JSON over a child's
//! stdio, one reader thread, an mpsc into the turn loop, one auto-approve seam — with a
//! different message vocabulary. The wire was captured before a line of this was written:
//! `docs/verification/native-claude-stream-json-2026-08-31/` (twelve runs, raw JSONL
//! committed unedited) and `docs/verification/native-claude-tool-status-2026-08-31/`.
//!
//! ```text
//!   spawn: claude --print --input-format=stream-json --output-format=stream-json
//!                 --include-partial-messages --verbose --setting-sources ''
//!                 --session-id <ours>
//!                 --permission-prompt-tool stdio
//!   control_request{initialize}          -> control_response{success}          [handshake]
//!   {"type":"user","message":{...}}      -> stream_event/assistant/user/... frames
//!                                        -> result {stop_reason, terminal_reason, usage}
//!   control_request{can_use_tool}        <- the agent asks; we answer (the seam, below)
//!   control_request{interrupt}           -> control_response, then a result
//! ```
//!
//! Auth = the customer's own `claude` login. No `ANTHROPIC_API_KEY`: every spike run had it
//! removed from the environment (`env -u`) and the binary reported `apiKeySource: "none"`
//! and completed real turns on subscription OAuth.
//!
//! ## THE LICENCE CONDITION THIS FILE IS BUILT AROUND
//! **RichOS may never collect, store, or intermediate Claude credentials or session
//! tokens.** The customer signs in to the unmodified binary with their own subscription and
//! Anthropic bills them directly. This is a condition of the licence that permits RichOS to
//! exist in this shape (`docs/research/claude-code-redistribution-2026-08-31.md`;
//! `ceo-decisions.md` §16).
//!
//! Concretely, and these are design decisions rather than good intentions:
//!
//! - Nothing here reads the keychain, and nothing here sets or forwards an API key.
//! - The `initialize` handshake's reply carries an `account` object (email, organization,
//!   `subscriptionType`). [`NativeClient::handshake`] reads it for **liveness only** — it
//!   checks that a `success` came back and keeps NOTHING. It is not normalized into
//!   machinery, not logged, and not returned to any caller.
//! - So there is no login-status indicator, because the obvious way to build one would
//!   breach the condition. That is a named absence, not an oversight.
//!
//! ## THE UNDOCUMENTED FLAG, AND WHY THIS FILE CANNOT DEGRADE QUIETLY
//! `--permission-prompt-tool stdio` is what arms the `can_use_tool` channel, and it **does
//! not appear in `claude --help`** — re-verified 2026-08-31 against version **2.1.252**:
//! 274 lines of help, zero matches. The binary also self-updates: four versions sit in
//! `~/.local/share/claude/versions/` on this machine (2.1.246, 2.1.250, 2.1.251, 2.1.252),
//! and the 2026-08-31 spike ran against 2.1.251 — it moved WHILE this was being built.
//!
//! §16 accepted that risk and recorded the mitigation: a custom launcher at
//! `~/.local/bin/claude` survives auto-update, so the flag can be pinned without modifying
//! any binary (which also keeps the "must not be modified" licence condition intact).
//! [`resolve_claude_bin`] prefers that path for exactly this reason.
//!
//! **What this file guarantees, and it is the whole of the guarantee:**
//!
//! - The flag is in [`child_args`] unconditionally. There is no code path that drops it and
//!   continues, no env override that disables it, and no fallback client. `args_always_carry_the_permission_flag`
//!   pins that.
//! - A binary that REJECTS the flag is caught at [`NativeClient::spawn`], loudly, before any
//!   turn: an unknown option makes `claude` write `error: unknown option '<flag>'` to stderr
//!   and **exit 1 with zero bytes of stdout** (measured 2026-08-31 on 2.1.252, both for a
//!   nonsense flag and alongside a good one). The handshake turns that into
//!   [`NativeError::Startup`], carrying the child's own stderr verbatim.
//! - A missing binary is [`NativeError::BinaryMissing`], before a process is spawned — and
//!   ONLY a missing binary. A missing engine directory is [`NativeError::WorkingDirMissing`]
//!   and an unrunnable one is [`NativeError::BinaryNotExecutable`], because `ENOENT` from
//!   `Command::spawn` means either of the first two and reporting one as the other sends
//!   whoever set RichOS up looking in the wrong place. `preflight` separates them before a
//!   process exists.
//! - A binary that starts but never answers the handshake is [`NativeError::Startup`] after
//!   [`HANDSHAKE_TIMEOUT`], never an indefinite hang.
//!
//! **What it does NOT guarantee, stated plainly rather than left to be discovered.** If the
//! flag is one day still ACCEPTED but silently stops arming the channel, nothing here
//! detects it at startup: the handshake would pass and the failure would surface only as a
//! tool that never runs. The `initialize` reply carries no field naming the permission
//! prompt tool (checked — 17 fields, none of them it), so there is nothing to assert
//! against. That is **unproven** and it is not fixable from this side.

use crate::cognition::{Cognition, CognitionError, TurnItem};
use crate::machinery::MachineryRecord;
use crate::steering::TurnCancel;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// The undocumented flag the whole `can_use_tool` channel hangs off (§16).
///
/// A named constant because it is the single most fragile dependency in this file, and a
/// grep for it must find every place it matters: the arg vector, the test that pins it, and
/// this doc.
pub const PERMISSION_PROMPT_TOOL: &str = "--permission-prompt-tool";

/// The flag that carries RichOS's standing instruction into the child (`doctrine.rs`).
///
/// **The SECOND semi-documented flag this file hangs off, and it is in the same fragility
/// class as [`PERMISSION_PROMPT_TOOL`] for a slightly different reason.** It is not its own
/// entry in `claude --help`; it appears only inside `--bare`'s description (help line 52 on
/// 2.1.263). It was proven real with a positive control, because a first test gave a false
/// green — `claude --anything --help` exits 0, so `--help` proves nothing about a flag:
///
/// ```text
/// $ claude --append-system-prompt-file /etc/hosts --definitely-not-a-flag --print x 2>&1 | head -1
/// error: unknown option '--definitely-not-a-flag'      # so the -file flag IS known
/// $ claude --append-system-prompt-file /nonexistent/zzz.md --print x ; echo $?
/// Error: Append system prompt file not found: /nonexistent/zzz.md
/// 1
/// ```
///
/// The same three defenses as the permission flag, written the same way on purpose:
///
/// - it is in [`chat_child_args`] unconditionally — no environment override, no fallback path,
///   pinned by `chat_args_always_carry_the_doctrine_flag`;
/// - a binary that REJECTS it is caught at [`NativeClient::spawn`] (exit 1, zero bytes of
///   stdout, the child's own stderr in [`NativeError::Startup`]), and a MISSING file is caught
///   even earlier by [`preflight`], which names the path;
/// - a binary that accepts it and silently stops honoring it is the case nothing structural can
///   catch. That is what the behavioral sentinel is for, and why the sentinel is a release gate
///   rather than a one-off — see `tests/doctrine_sentinel.rs`.
pub const APPEND_SYSTEM_PROMPT_FILE: &str = "--append-system-prompt-file";

/// The flag that carries RichOS's own SKILLS into the child (`skills.rs`).
///
/// Unlike the two flags above this one is fully documented (`--plugin-dir <path>`, "Load a
/// plugin from a directory or .zip for this session only"), and `--bare`'s own help names it
/// among the ways to *"explicitly provide context"*. So the fragility here is not the flag.
///
/// **The fragility is that it fails SILENTLY, and that is measured.** A `--plugin-dir` naming a
/// path that does not exist produces exit 0, a clean handshake, `plugins: []` and a perfectly
/// ordinary turn — `docs/verification/inner-doctrine-skills-2026-09-06/`, cell K4. That is the
/// exact quiet degradation `--append-system-prompt-file` was chosen for NOT doing. So the
/// loudness is built rather than inherited, in two layers:
///
/// - [`preflight`] proves the manifest and every declared `SKILL.md` are present and non-empty
///   before a process exists, and raises [`NativeError::SkillsMissing`] naming the file;
/// - the reader thread reads the first `system/init` frame's `plugins` array and records
///   [`skills::SkillsVerdict`], so a directory the BINARY declined is a positive signal rather
///   than an absence somebody has to notice. The module doc above records that no such field
///   exists for [`PERMISSION_PROMPT_TOOL`]; for this one it does, and it is used.
pub const PLUGIN_DIR: &str = "--plugin-dir";

/// The environment variable that decides whether the child DEFERS its own tools, and the
/// value that stops it deferring them on the conversation.
///
/// **This is the third undocumented dependency in this file, and it is here because a
/// deferred register costs the CEO two round trips before he hears a word.** Measured on the
/// shipped path, `docs/verification/question-receipt-2026-09-18.md` run 2, one question turn:
///
/// ```text
///       3.934 s  ToolSearch                                        <- discovering the register
///       3.940 s  select:mcp__richos_assignments__record
///       4.999 s  tool_call
///       7.593 s  ToolSearch                                        <- discovering the continuity tools
///       7.653 s  select:mcp__richos_continuity__checkpoint,…
///      11.027 s  tool_call
///      17.982 s  mcp__richos_assignments__record                   <- the register, FOURTH
///      22.956 s  his first word
/// ```
///
/// Tool discovery adds delay before Rich can use either a quick read or the register.
/// Seven of those twenty-three seconds were discovery. The tools are resident so Rich
/// can choose the useful call immediately.
///
/// **Why an environment variable and not a flag: there is no flag.** Re-derived from the
/// shipped binary rather than from documentation — first on `claude` 2.1.275, and RE-DERIVED
/// ON **2.1.277** on 2026-09-18 for the reason below. The module that decides it lives at byte
/// offset 175_107_000 in 2.1.277 (173_562_000 in 2.1.275); the minifier renamed every symbol
/// and changed nothing else:
///
/// ```text
/// // 2.1.277.  2.1.275's names in brackets.
/// function b7e(){ if(Ebt())return"standard"; if(u())return"tst";      // [EYe], [f_t]
///   let e=process.env.ENABLE_TOOL_SEARCH, r=e?drr(e):null;            // [UQn]
///   if(r===0)return"tst"; if(r===100)return"standard";
///   if(m(e))return"tst-auto"; if(Oe(e))return"tst"; if(Eo(e))return"standard";   // [To]
///   return"tst" }                                   // <- UNSET MEANS DEFERRAL IS ON
/// function Eo(e){ … return["0","false","no","off"].includes(String(e).toLowerCase().trim()) }
/// function Kg(){ let e=b7e(); if(e==="standard"){ …; return!1 } … }   // [Mg]
/// ```
///
/// So `"false"` is one of five spellings `Eo` accepts, `b7e()` returns `"standard"`, and
/// `Kg()` — the session's one "is tool search on" decision — returns false. The DEFAULT is
/// deferral, which is exactly the third-party default this project refuses to inherit
/// unexamined.
///
/// **THE KNOB WAS ACCUSED OF FAILING ON 2.1.277, AND IT HAD NOT.** Ray's candidate-.10 walk
/// read the alarm below out of `app.log` and its audit named it the cause of "On it!" landing
/// at 8–12 s; this comment is what the re-derivation and the probe found instead, and it is
/// recorded here because the accusation will otherwise be made again.
///
/// - The decision module is UNCHANGED between the two releases. `Oe` and `Eo` are byte-
///   identical to 2.1.275's `Oe`/`To`, and `b7e`/`Kg` differ from `EYe`/`Mg` only in symbol
///   names. Nothing in it reads a different input.
/// - And it is not a source reading: `/Users/alex/.local/bin/claude` **2.1.277** — the same
///   binary that run, `app.log` line 23 — was driven directly with the arg vector
///   [`child_args`] builds, and its `system/init.tools` answered:
///
/// ```text
/// ENABLE_TOOL_SEARCH unset : 28 tools, ToolSearch PRESENT   <- deferral, and MCP tools absent
/// ENABLE_TOOL_SEARCH=false : 35 tools, ToolSearch ABSENT
/// ENABLE_TOOL_SEARCH=0     : 35 tools, ToolSearch ABSENT
/// …=false + --permission-mode auto           : 35 tools, ToolSearch ABSENT
/// …=false + the inline --settings `configure` sends : 35 tools, ToolSearch ABSENT
/// …=false + --strict-mcp-config --mcp-config : 27 tools, ToolSearch ABSENT
/// ```
///
/// The knob holds on 2.1.277 in every shape the app spawns. What had actually happened is in
/// [`ReaderState::tool_residency`]: the alarm carried no lease role, and the WORK lease — which
/// `tool_residency_env` deliberately gives no variable at all — was setting it off.
///
/// **Two defenses:**
///
/// - it is set unconditionally for [`LeaseRole::Conversation`] by [`tool_residency_env`],
///   pinned by `the_conversation_asks_for_resident_tools`;
/// - an unknown variable cannot fail a spawn, so the loudness is built rather than inherited:
///   the reader reads `ToolSearch` out of the child's own init inventory into
///   `ReaderState::tool_search_offered` and says so on stderr. A knob that is renamed, or a
///   binary that starts deferring for a different reason, shows up as the tool being offered
///   again. **It says it about the lease it is actually on** — see
///   [`ReaderState::tool_residency`], the field that was missing when the alarm above was
///   raised against a binary that had done nothing wrong.
///
/// **Not set on the work lease, deliberately.** Nobody is sitting in front of a work lease's
/// first token, its inventory is a different size, and one measurement on one lease is not
/// evidence about the other. Changing both on the strength of the front desk's numbers would
/// be scope this measurement does not cover.
pub const TOOL_SEARCH_ENV: &str = "ENABLE_TOOL_SEARCH";

/// The value [`TOOL_SEARCH_ENV`] is given. See that constant for the five spellings the
/// binary accepts and why this is one of them.
pub const TOOL_SEARCH_OFF: &str = "false";

/// The name of the child's own tool-discovery tool, as it appears in `system/init.tools`.
pub const TOOL_SEARCH_TOOL: &str = "ToolSearch";

/// The environment a lease is spawned with so its tools are RESIDENT from the first turn —
/// `None` where RichOS has measured nothing and therefore changes nothing.
///
/// A pure function so the pin is a unit test rather than a spawned process, exactly as
/// [`child_args`] is.
pub fn tool_residency_env(role: LeaseRole) -> Option<(&'static str, &'static str)> {
    match role {
        LeaseRole::Conversation => Some((TOOL_SEARCH_ENV, TOOL_SEARCH_OFF)),
        LeaseRole::Work => None,
    }
}

/// The base delay Claude Code waits before retrying a request the API refused as overloaded
/// (status 529). Read from the 2.1.292 binary: delay = min(32000, base * 2^(attempt-1)) ms plus
/// up to 25% jitter, 10 retries by default, base 500 ms. At 500 the ten retries wait at least
/// 500+1000+2000+4000+8000+16000 + 4*32000 = 159500 ms; at 2000 they wait at least
/// 2000+4000+8000+16000 + 6*32000 = 222000 ms (3.7 min). Both lease roles set it.
pub const OVERLOADED_RETRY_DELAY_ENV: &str = "CLAUDE_CODE_OVERLOADED_RETRY_BASE_DELAY_MS";
pub const OVERLOADED_RETRY_DELAY_MS: &str = "2000";

/// How many times Claude Code retries a failed request (default 10, clamped at 15 in 2.1.305).
/// A connection error always waits base 500 ms, cap 32 s, so the count is the only lever: at 15
/// the minimum total wait for a connection error is 500+1000+2000+4000+8000+16000 + 9*32000
/// = 319500 ms (5.3 min) against 159500 ms at 10, which rides out an internet cutoff of a few
/// minutes. For overload at base 2000 the minimum goes from 222000 to 382000 ms.
pub const MAX_RETRIES_ENV: &str = "CLAUDE_CODE_MAX_RETRIES";
pub const MAX_RETRIES: &str = "15";

/// The environment every lease child (conversation and work) is spawned with, whatever its role.
/// A pure function of nothing, so the pin is a unit test, as [`tool_residency_env`] is.
pub fn lease_retry_env() -> [(&'static str, &'static str); 2] {
    [(OVERLOADED_RETRY_DELAY_ENV, OVERLOADED_RETRY_DELAY_MS), (MAX_RETRIES_ENV, MAX_RETRIES)]
}

/// Native continuity checkpoint tool identity.
pub const CONTINUITY_CHECKPOINT_TOOL: &str = "mcp__richos_continuity__checkpoint";

/// Is the child deferring its tools, according to the child?
///
/// The one reading that can catch [`TOOL_SEARCH_ENV`] silently stopping work. Its own tool is
/// in the inventory exactly when discovery is live, so this is a fact from the wire and not an
/// inference from the variable having been set.
///
/// **And the MCP tools being listed proves nothing about this**, which is the trap worth
/// naming: `assignment_tool_loaded`, `status_tool_loaded` and `continuity_tools_loaded` were
/// all `true` on the run whose model still had to `ToolSearch` for the register
/// (`docs/verification/question-receipt-2026-09-18.md`). A deferred tool is still in the init
/// inventory. Only the presence of the discovery tool separates the two.
fn tool_search_from_init(init: &Value) -> InitFact {
    InitFact::reported(
        init["tools"].as_array().is_some_and(|tools| tools.iter().any(|tool| tool.as_str() == Some(TOOL_SEARCH_TOOL))),
    )
}

/// **What the reader says on stderr when the child offers its OWN [`TOOL_SEARCH_TOOL`] — and
/// it depends on whether THIS lease ever asked for resident tools.**
///
/// A pure function of `residency` ([`ReaderState::tool_residency`]) so the two messages are a
/// unit test rather than something only a real provider can show, exactly as
/// [`tool_residency_env`] and [`child_args`] are.
///
/// - `Some(name)` — a lease that SET the variable is offered the discovery tool anyway. That
///   is the knob having stopped working, it is the CEO's first words, and it is loud.
/// - `None` — a lease that set nothing is deferring because nobody asked it not to. Expected,
///   said once so a longer first tool call on that lease is not a mystery, and phrased so it
///   can never be read as the sentence above.
fn tool_search_notice(residency: Option<&str>) -> String {
    match residency {
        Some(name) => format!(
            "[richos] conversation tools: discovery is unexpectedly required despite \
             `{name}={TOOL_SEARCH_OFF}`. This adds a request before a tool can run. \
             Check the installed provider version. Discovery tool: {TOOL_SEARCH_TOOL}."
        ),
        None => "[richos] background work tools: discovery enabled as expected.".to_string(),
    }
}

/// How long [`NativeClient::spawn`] waits for the `initialize` handshake before refusing.
///
/// **Measured, not guessed:** the handshake answered in **697.9 ms** on 2.1.252 with the
/// full production arg vector (2026-08-31, `ANTHROPIC_API_KEY` removed), and the spike's
/// Rust run measured it at the same order (`run9-rust-driven.timings.tsv`: the
/// `control_response` line was read at offset **0.598 s**). 30 s is 43x the measured figure —
/// wide enough that a cold start on a slow disk is never cut off, and short enough that a
/// binary which will never answer is refused while the CEO is still looking at the splash
/// rather than at a frozen window.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// The `stop_reason` `prompt` returns when the agent acknowledged the cancel.
///
/// **This is OURS on this wire, and that is a real change from the file this replaces.**
/// ACP answered a cancelled prompt with `stopReason: "cancelled"`. The native binary answers
/// with `stop_reason: null`, `subtype: "error_during_execution"`, `is_error: true` and
/// `terminal_reason: "aborted_streaming"` (`raw/run9-rust-driven.jsonl:65`) — the fact
/// survives, it just lives on a different field. [`stop_reason_of`] maps it back, so
/// everything downstream (`spine.rs`, `steering.rs`, the ledger) keeps the one string it
/// already reasons about.
pub const STOP_REASON_CANCELLED: &str = "cancelled";

/// **Which of the two leases this is** — the background-work spec §2.1's second connection,
/// made a type rather than a boolean so every place that has to care says which one it
/// means.
///
/// The two differ in what the CHILD is given, not in how the host holds it:
///
/// | | `Conversation` | `Work` |
/// |---|---|---|
/// | `richos_continuity` in its MCP config | yes | **no** — spec §5.8a-ii seam 1 |
/// | continuity tools required to start a turn | yes (`:1960-1962`) | no, because it was not given them |
/// | its preparation calls `bridge.brief` | yes (`:1972`) | **no** — seam 2 |
/// | `richos_onboarding` in its MCP config | yes | **no** — since 2026-09-18; Ray's candidate-.8 row 1 |
/// | an action grant over a company scope | yes | **no** — nothing writes a work lease one |
/// | ECS seat | the CEO's | its own, one per assignment (§5.8c) |
/// | audience | `ceo` | `worker` |
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseRole {
    Conversation,
    Work,
}

/// The three role decisions, as functions, **because the call sites use these and the tests
/// assert on these.** A test that re-stated the condition would be a second copy of the
/// rule that agrees with the first only until somebody edits one of them — which is the
/// shape of a negative test that passes for its own reasons.
///
/// Seam 2 of the background-work spec §5.8a-ii: a work lease must not come down
/// `prepare_work_turn`'s path, because that path calls `bridge.brief` and the brief is the
/// CEO's.
fn work_preparation_refusal(role: LeaseRole) -> Option<&'static str> {
    (role == LeaseRole::Work).then_some("A background work connection does not take conversation turns.")
}

/// And the other way round: a conversation lease does not take background assignments.
fn assignment_binding_refusal(role: LeaseRole) -> Option<&'static str> {
    (role == LeaseRole::Conversation)
        .then_some("The conversation's connection does not take background assignments.")
}

/// What the child's own `system/init` frame said about ONE readiness fact.
///
/// **Three states, because the init frame arrives with the first TURN and not with the
/// handshake** ([`child_args`]'s doc says so, and the `initialize` control handshake reads a
/// `subtype` and keeps nothing else). So for the whole of a lease that has never been used,
/// the truthful answer to "did the engine plugin load" is *nobody has told us* — which is
/// not the same statement as *we were told no*, and calls for the opposite response.
///
/// Deliberately the same shape, and the same argument, as [`crate::skills::SkillsVerdict`]:
/// *"'we have not been told yet' and 'we were told it is not there' call for opposite
/// responses, and collapsing them is how an absence gets reported as a fact."*
///
/// **These three facts were plain `bool`s until 2026-09-18, and the collapse cost the
/// product every background job it had.** `bind_work_assignment` runs before the work
/// lease's first turn, so it read `false`, called it a refusal, and failed the assignment
/// with *"The desktop engine plugin did not load"* — on a lease whose plugin had in fact
/// loaded and whose `SessionStart` hook had already written evidence proving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitFact {
    /// No `system/init` frame has arrived on this lease yet. Never a refusal on a path that
    /// runs before the first turn; always one on a path that runs after it.
    NotYetReported,
    /// The frame arrived and the fact holds.
    Yes,
    /// The frame arrived and the fact does not hold. **This is the loud case**, and it is
    /// the one every sentence in this file was written for: `--plugin-dir` accepts a path it
    /// cannot use and reports success (measured, `inner-doctrine-skills-2026-09-06/` cell
    /// K4), so a reported absence is the only thing that catches it.
    No,
}

impl InitFact {
    /// Read a fact off a frame that HAS arrived. There is no constructor for
    /// [`Self::NotYetReported`] on purpose: it is a default, never a reading.
    fn reported(holds: bool) -> Self {
        if holds { Self::Yes } else { Self::No }
    }
}

/// The stop reason `prompt` returns when the agent did NOT answer the stopped turn within
/// [`CANCEL_GRACE_MS`] of being told to.
///
/// A distinct string because it is a distinct fact: the CEO's stop still stands and the
/// turn is still recorded as stopped, but this lease is no longer known to be idle, so the
/// spine rotates it at the boundary rather than handing it the next turn (`spine.rs`).
pub const STOP_REASON_CANCEL_UNACKNOWLEDGED: &str = "cancel_unacknowledged";

/// The `terminal_reason` the binary sets on a turn the client interrupted.
pub const TERMINAL_REASON_ABORTED: &str = "aborted_streaming";

/// The `terminal_reason` the binary sets on a turn an API failure ended (2.1.305, captured: a
/// refused connection and a `403`; `crate::connection` has both shapes).
pub const TERMINAL_REASON_API_ERROR: &str = "api_error";

/// How long `prompt` waits for the agent to answer the cancelled turn.
///
/// **`acp.rs:49` asked whoever ran the first live stop to replace this bound with a measured
/// figure and to say so. That measurement now exists, and the bound is kept anyway.**
///
/// Measured on this wire (`run9`, the definitive Rust-driven run): the `interrupt`
/// control_response came back in **0.9 ms** and the terminal `result` in **9.1 ms**. The
/// Python driver measured 10 ms / 20 ms independently (`run5`). Against 3,000 ms that is
/// 3000 ÷ 9.1 ≈ **330x of headroom**, i.e. three orders of magnitude.
///
/// It is not tightened to the measurement, and the reason is arithmetic rather than
/// caution: 9.1 ms was an idle model mid-sentence. A cancel that lands while a 30-second
/// `Bash` tool is running has to wait for the agent to finish unwinding it, and
/// `native-claude-tool-status-2026-08-31` measured a **30.002 s heartbeat cadence**, i.e.
/// tools that genuinely run for tens of seconds. A bound tuned to 9.1 ms would report
/// `cancel_unacknowledged` — and force a rotation — on a perfectly healthy agent. 3,000 ms
/// costs the CEO nothing either way: the stop is durable and the UI has already moved to
/// `stopping` before this timer starts.
pub const CANCEL_GRACE_MS: u64 = 3_000;

/// [`CANCEL_GRACE_MS`], overridable by `RICHOS_CANCEL_GRACE_MS`.
///
/// The override exists so the non-compliant-agent test can prove the timeout in
/// milliseconds instead of adding three seconds to every run — and so the bound can be
/// tuned in front of a live binary without a rebuild.
pub fn cancel_grace() -> Duration {
    let ms = std::env::var("RICHOS_CANCEL_GRACE_MS").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(CANCEL_GRACE_MS);
    Duration::from_millis(ms)
}

/// How many stderr lines are kept so a startup failure can quote the child's own words.
///
/// Bounded because a long-lived child writes diagnostics forever and an unbounded buffer
/// behind a `Mutex` is a slow leak. The failure this exists for is one line long
/// (`error: unknown option '--permission-prompt-tool'`), so 64 is ~64x what it needs.
const STDERR_TAIL_LINES: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum NativeError {
    #[error("claude io: {0}")]
    Io(#[from] std::io::Error),
    #[error("claude json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("claude channel closed (child exited?)")]
    Closed,
    /// **The child ended in the middle of a turn without a `result` — WITH ITS OWN WORDS.**
    /// Until 2026-10-21 this was [`NativeError::Closed`], which carries nothing, so the turn's
    /// reason said "claude channel closed" whatever the child had printed on its way out, and
    /// `interruption.rs` could only call it a passing hiccup. A new user's every message ended
    /// "This usually fixes itself" in a loop and nothing on his Mac recorded why
    /// (`docs/development/failure-classes/cause-discarded-at-the-failure-boundary.md`). `said`
    /// is the child's last stderr lines and, when Claude Code's own `auth status` says nobody
    /// is signed in, [`CHILD_EXIT_SIGNED_OUT`]: the classifier reads both.
    #[error("claude stopped before answering; {said}")]
    Exited { said: String },
    #[error("claude protocol error: {0}")]
    Protocol(String),
    /// **LOUD, and deliberately not recoverable here.** The `claude` binary is not where we
    /// looked. Raised BEFORE a process is spawned, so the failure names a path rather than
    /// an exit code.
    #[error("the claude binary was not found at {path} — RichOS drives Claude Code directly and cannot run without it")]
    BinaryMissing { path: String },
    /// **LOUD, raised by [`resolve_claude_bin_checked`] before any process exists at all —
    /// the 2026-09-17 candidate-walk fix.** [`resolve_claude_bin`] used to fall through to
    /// the bare name `claude` here, which a caller then handed to a provider whose `PATH`
    /// `engine_profile.rs` had already replaced with [`crate::runtime::EngineRuntime::path`];
    /// a bare name that `PATH` can never resolve. The failure surfaced two layers downstream,
    /// misclassified by `interruption.rs` as [`crate::interruption::InterruptionCause::Transient`]
    /// ("that kind of thing usually clears on its own") — false for a binary that was never
    /// installed. This variant names every place actually looked, the way `setup.rs::find_claude`
    /// already does for the first-run advisory.
    #[error("no `claude` install could be found on this Mac — looked in: {looked} — install Claude Code, or set RICHOS_CLAUDE_BIN to its path")]
    ClaudeNotFound { looked: String },
    /// **LOUD.** The `claude` binary is exactly where we looked and cannot be run: the
    /// execute bit is off, or the path names a directory. A DIFFERENT fault from
    /// [`NativeError::BinaryMissing`] with a different fix, so it is a different variant —
    /// "install Claude Code" is the wrong instruction for a file that is already there.
    #[error("the claude binary at {path} cannot be executed ({why}) — the file is there, so this is a permissions problem, not a missing install")]
    BinaryNotExecutable { path: String, why: String },
    /// **LOUD, and the one this enum used to report as [`NativeError::BinaryMissing`].**
    /// RichOS starts `claude` with the engine directory as its working directory; a working
    /// directory that does not exist fails the spawn with `ENOENT`, which is the SAME errno a
    /// missing binary produces. Raised BEFORE the spawn so the two can never be confused
    /// again. Measured 2026-09-01: a double-clicked `.app` has `cwd = /`, the old default
    /// resolved to `/../engine`, and the CEO was told his present, executable `claude` was
    /// missing.
    #[error("the engine directory {path} does not exist — RichOS runs Claude with that directory as its working directory and cannot start without it (this is NOT a missing claude binary)")]
    WorkingDirMissing { path: String },
    /// **LOUD.** The engine path exists but is a file, a socket, or anything other than a
    /// directory. Its own variant for the same reason as above: `Command::current_dir`
    /// reports it as `ENOTDIR`, and "it isn't there" would be a false statement about a path
    /// the operator can see.
    #[error("the engine directory {path} exists but is not a directory — RichOS cannot use it as a working directory")]
    WorkingDirNotADirectory { path: String },
    /// **LOUD.** The child started but never completed the `initialize` handshake: it
    /// rejected a flag, exited, or went silent. `stderr` is the child's own words, verbatim,
    /// because on a rejected flag that single line IS the diagnosis.
    #[error("claude failed to start ({reason}); the child said: {stderr}")]
    Startup { reason: String, stderr: String },
    /// **LOUD, and the whole reason `--append-system-prompt-file` won over the alternatives.**
    /// RichOS's standing instruction (`doctrine.rs`) is not at the path we are about to hand
    /// the child. Raised BEFORE the spawn, so the failure names the file rather than arriving
    /// as an exit code — and so it can never be a fallback.
    ///
    /// A Claude started without it is not Rich; it is a generic coding assistant wearing the
    /// product's window. Design §3.2: an absent instruction file under the rejected
    /// `--setting-sources project` candidate produces a successful handshake and a subtly wrong
    /// assistant, which is the class this app refuses to ship.
    #[error("RichOS's standing instruction is not at {path} ({why}) — it will not start Claude without it, because a Claude started without it is not Rich")]
    DoctrineMissing { path: String, why: String },
    /// **LOUD, and loud only because we make it so.** A skill RichOS ships is not on disk where
    /// it is about to be handed to the child.
    ///
    /// Its own variant rather than a reuse of [`NativeError::DoctrineMissing`], because the two
    /// have different fixes and — more importantly — different failure physics. A missing
    /// doctrine file is refused by `claude` itself (exit 1, its own stderr). A missing
    /// `--plugin-dir` is accepted in silence: exit 0, clean handshake, `plugins: []`, an
    /// ordinary turn (measured, `inner-doctrine-skills-2026-09-06/` cell K4). This variant is
    /// the whole of the loudness for that path.
    #[error("a skill RichOS ships is not at {path} ({why}) — it will not start Claude with a skill missing, because `--plugin-dir` accepts a path that is not there without saying so")]
    SkillsMissing { path: String, why: String },
}

impl From<NativeError> for CognitionError {
    fn from(e: NativeError) -> Self {
        match e {
            NativeError::Protocol(m) => CognitionError::Protocol(m),
            other => CognitionError::Io(other.to_string()),
        }
    }
}

/// Map the binary's terminal `result` frame onto the one stop-reason string the rest of the
/// app already reasons about.
///
/// **Spike caveat C1, resolved here and nowhere else.** ACP said `stopReason: "cancelled"`.
/// This wire says `stop_reason: null` + `subtype: "error_during_execution"` +
/// `terminal_reason: "aborted_streaming"`, and ONLY the last of those separates a cancel
/// from a genuine error — an `error_during_execution` with any other `terminal_reason` is a
/// real failure and must not be reported as the CEO's stop.
pub fn stop_reason_of(result: &Value) -> String {
    if result.get("terminal_reason").and_then(|v| v.as_str()) == Some(TERMINAL_REASON_ABORTED) {
        return STOP_REASON_CANCELLED.to_string();
    }
    if let Some(s) = result.get("stop_reason").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    // No `stop_reason` and not an abort. Report the vendor's own subtype rather than
    // inventing `end_turn` — a turn that ended in a way we have no name for must not be
    // recorded as one that ended cleanly.
    result.get("subtype").and_then(|v| v.as_str()).unwrap_or("unknown").to_string()
}

/// RichOS's own words, added to [`NativeError::Exited`] when Claude Code's `auth status` says
/// nobody is signed in to the account the lease ran under. First-party, so `interruption.rs`
/// matches it as its own structure (`OUR_CHILD_EXIT_SIGNED_OUT`), never as a vendor string.
pub const CHILD_EXIT_SIGNED_OUT: &str = "Claude Code says no account is signed in on this Mac";

/// How long the turn's end waits for the child's last stderr lines once its stdout has closed.
/// The two pipes close together when it exits; this only covers the drain thread's last read.
const CHILD_EXIT_STDERR_WITHIN: Duration = Duration::from_millis(500);

/// How long the turn's end asks Claude Code whether the account is signed in. It runs only on
/// a turn that already failed, and the person is waiting on that turn's card.
const CHILD_EXIT_AUTH_WITHIN: Duration = Duration::from_secs(5);

/// The child's last stderr lines kept in a turn's reason: enough for an error and its cause.
const CHILD_EXIT_LINES: usize = 6;

/// **What a child that ended mid-turn left behind**, for [`NativeError::Exited`]: its last
/// stderr lines (waited for up to [`CHILD_EXIT_STDERR_WITHIN`]) and, when `signed_out` says
/// so, [`CHILD_EXIT_SIGNED_OUT`]. Pure but for the wait, so a test drives it with a tail and
/// a closure and no child at all.
fn child_exit_said(done: &AtomicBool, tail: &Mutex<std::collections::VecDeque<String>>, signed_out: impl FnOnce() -> bool) -> String {
    let deadline = std::time::Instant::now() + CHILD_EXIT_STDERR_WITHIN;
    while !done.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let lines: Vec<String> = {
        let t = tail.lock().unwrap();
        t.iter().filter(|l| !l.trim().is_empty()).cloned().collect()
    };
    let start = lines.len().saturating_sub(CHILD_EXIT_LINES);
    let mut said = if lines.is_empty() {
        "it wrote nothing on stderr".to_string()
    } else {
        format!("the child said: {}", lines[start..].join(" / "))
    };
    if signed_out() {
        said.push_str("; ");
        said.push_str(CHILD_EXIT_SIGNED_OUT);
    }
    said
}

pub(crate) fn native_result(result: &Value) -> Result<String, NativeError> {
    let reason = stop_reason_of(result);
    if let Some(reset) = usage_limit_signal(result) {
        return Err(NativeError::Protocol(crate::claude_accounts::usage_limit_error(reset, &result.to_string())));
    }
    if reason == "child_exited" {
        let said = result.get("child_said").and_then(Value::as_str).unwrap_or("it wrote nothing on stderr");
        return Err(NativeError::Exited { said: said.to_string() });
    }
    if result.get("is_error").and_then(Value::as_bool) == Some(true) && reason != STOP_REASON_CANCELLED {
        let detail = result.get("errors").or_else(|| result.get("result"))
            .map(Value::to_string).unwrap_or(reason);
        return Err(NativeError::Protocol(detail));
    }
    Ok(reason)
}

/// The child's argument vector.
///
/// Every flag is here for a stated reason, and the vector is a pure function so a test can
/// assert on it without spawning anything:
///
/// - `--print` + `--input-format=stream-json` + `--output-format=stream-json` — the duplex
///   stdio protocol this whole file speaks.
/// - `--include-partial-messages` — **the clean-output path depends on it.** Without it the
///   assistant's text arrives only on whole-message `assistant` frames and the CEO would see
///   a turn appear all at once at the end. (The reader defends against that anyway; see
///   `text_deltas_since_message_start`.)
/// - `--verbose` — required for stream-json output.
/// - `--setting-sources ''` — the operator's user/project/local settings stay out. RichOS's
///   behaviour must not depend on files it did not write.
///
/// Native session persistence keeps the provider's complete recorded dialogue and tool
/// evidence available for direct inspection. The app ledger still owns user conversation
/// and authorization; a provider transcript is execution evidence.
///
/// - `--session-id <ours>` — **so `session_id()` is known synchronously at spawn.** ACP had
///   a `session/new` request that returned one; this wire announces it on `system/init`,
///   which arrives with the first TURN, and a lease whose id appeared later would leave the
///   rotation records in `ledger.rs` unable to name the lease they are about.
/// - [`PERMISSION_PROMPT_TOOL`] `stdio` — the undocumented one. See the module doc.
pub fn child_args(session_id: &str) -> Vec<String> {
    [
        "--print",
        "--input-format=stream-json",
        "--output-format=stream-json",
        "--include-partial-messages",
        "--verbose",
        "--setting-sources",
        "",
        "--disallowed-tools",
        "AskUserQuestion",
        "--session-id",
        session_id,
        PERMISSION_PROMPT_TOOL,
        "stdio",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// The CEO's 2026-10-14 instruction prioritizes instant replies over effort level.
/// The conversation only answers or hands work over; the work lease retains high effort.
pub fn lease_effort(role: LeaseRole) -> &'static str {
    match role {
        LeaseRole::Conversation => "low",
        LeaseRole::Work => "high",
    }
}

/// **The flags that name the model and the effort for a lease of this role** — appended to
/// every lease's vector by [`NativeClient::spawn_with_tools`], and the one function the tests
/// assert on. Rich is the conversation lease and Stu the work lease (§110); each gets the
/// model its stored setting names ([`crate::engine_profile::EngineProfile::lease_models`],
/// read from `config.json` at every spawn). A lease spawned without an engine profile (the
/// headless and test paths, which read no settings) gets the default, Opus.
///
/// Until §119 the app passed neither flag, so both sessions ran on the CLI's own default for
/// the account. Measured on `claude` 2.1.295 with the lease's `--setting-sources ''` on
/// 2026-10-10: `--model opus --effort high` answered as `claude-opus-5-5`, and
/// `--model fable --effort high` as `claude-fable-5-1`.
pub fn lease_model_args(role: LeaseRole, profile: Option<&crate::engine_profile::EngineProfile>) -> Vec<String> {
    let models = profile.map(|p| p.lease_models).unwrap_or_default();
    let model = match role {
        LeaseRole::Conversation => models.rich,
        LeaseRole::Work => models.stu,
    };
    vec!["--model".into(), model.cli_alias().into(), "--effort".into(), lease_effort(role).into()]
}

pub fn chat_child_args(session_id: &str, doctrine: &Path, skills: &Path) -> Vec<String> {
    let mut args = child_args(session_id);
    args.push(APPEND_SYSTEM_PROMPT_FILE.to_string());
    args.push(doctrine.display().to_string());
    args.push(PLUGIN_DIR.to_string());
    args.push(skills.display().to_string());
    args
}



/// Streamed items for the active prompt turn.
///
/// Machinery arrives here RAW and un-normalized on purpose. §1.4's feasibility argument for
/// G1 is that `seq` must be assigned at the mpsc DRAIN point (`prompt`'s loop, which is
/// single-threaded) and not at `dispatch` (which runs on the reader thread). A
/// `MachineryRecord` cannot exist without its `seq`, so `dispatch` hands over the wire JSON
/// and `prompt` normalizes it.
#[derive(Clone)]
enum ChunkMsg {
    /// Assistant-message text — the clean-output path, unchanged.
    Text(String),
    /// Any other frame, verbatim.
    Frame(Value),
    /// A `can_use_tool` we just answered, with the decision we made.
    Permission { request: Value, chosen: String },
    /// The DERIVED context measurement — see [`MachineryRecord::from_context_usage`]. It is
    /// its own arm rather than a frame because it is the only thing on this channel RichOS
    /// computed rather than received, and that distinction must survive to the record.
    Usage { used: u64, size: u64, usage: Value },
    /// **His answer is complete; the turn is still running** (product hunt 2026-10-15, finding
    /// 21). Sent once, right after the frame whose accepted checkpoint started withholding
    /// everything after it ([`NativeClient::settle_the_checkpoint`]). It ends nothing: the turn
    /// The turn's terminal `result` frame.
    Done(Value),
    /// The CEO pressed stop. Sent by [`NativeCancelHandle`] into THIS turn's channel purely
    /// to wake the drain loop, which is otherwise parked in a blocking `recv()`.
    ///
    /// Waking it this way rather than polling is deliberate arithmetic: a poll loop tight
    /// enough to feel immediate (20ms) would wake 50 times a second for the whole turn —
    /// 8_270s x 50 = 413_500 wakeups on §6.2's own `2h 17m 50s` example, every one of them
    /// finding nothing. One send costs one wakeup, at the moment it is needed.
    Cancel,
}

// ===========================================================================================
// TURN CORRELATION — WHICH `result` ANSWERS THE MESSAGE THIS CLIENT SENT
// ===========================================================================================

/// How far the child has said THIS client's message has got.
///
/// Three states rather than two, for the reason the tri-state [`InitFact`] exists elsewhere
/// in this file: "the child has not told us" and "the child told us it is still queued" call
/// for opposite treatment, and collapsing them is the defect below.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TurnPhase {
    /// No `command_lifecycle` frame naming our `command_uuid` has arrived. Either the child
    /// does not emit them, or ours has not been admitted yet. Nothing may be inferred.
    Unconfirmed,
    /// The child said our message ENTERED ITS COMMAND QUEUE (`state: "queued"`) and has not
    /// yet said it drained into a turn. A `result` written while this holds belongs to a turn
    /// that was already running.
    Queued,
    /// The child said our message drained into a turn (`started`), or that the turn which
    /// consumed it has ended (`completed`, which on the fold path precedes that turn's
    /// `result`). From here the turn in flight is OURS.
    Running,
}

fn finish_native_turn(inputs: Vec<PendingTurn>, frame: Value) {
    // Visible prompts report through their caller. Live-only turns
    // use one live callback. Every input still receives its own receipt outcome.
    let mut reported = inputs.iter().any(|input|
        matches!(input.sink, TurnSink::Prompt(_)));
    for input in inputs {
        let report = !reported && matches!(input.sink, TurnSink::Input { .. });
        reported |= report;
        input.finish(frame.clone(), report);
    }
}

enum TurnSink {
    Prompt(Sender<ChunkMsg>),
    Input {
        completed: Box<dyn FnOnce(crate::steering::InputOutcome) + Send>,
        limit: Option<Option<u64>>,
        /// An overload Claude Code reported only as a message in this turn
        /// ([`overloaded_api_error_message`]), so a turn that still ends `end_turn` reaches
        /// Stu's overload schedule instead of passing the vendor's line off as his reply.
        overload: Option<String>,
    },
}

/// **An overload Claude Code reported as a MESSAGE, not as a failed result.**
///
/// When its own retries give up, Claude Code wraps the API error in an assistant message and
/// marks that frame with a top-level `error` field (2.1.302's stream mapping sets `error` on
/// every assistant frame it emits and leaves it out of ordinary ones; the kind it computes for
/// a 529 is `overloaded`). Requiring that field means prose that merely quotes an
/// `API Error:` line is never taken for one. Which fault it is stays
/// [`crate::upstream::UpstreamFailure`]'s decision alone, on the message's own words.
pub(crate) fn overloaded_api_error_message(frame: &Value) -> Option<crate::upstream::UpstreamFailure> {
    if frame.get("type").and_then(Value::as_str) != Some("assistant") || frame.get("error").is_none_or(Value::is_null) {
        return None;
    }
    let text: Vec<&str> = frame.pointer("/message/content").and_then(Value::as_array).into_iter().flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str)).collect();
    crate::upstream::UpstreamFailure::classify_lines(&text.join("\n"))
        .filter(|failure| failure.fault == crate::upstream::UpstreamFault::Overloaded)
}


/// The prompt this client has sent and is waiting for.
///
/// # THE DEFECT THIS TYPE EXISTS TO CLOSE
///
/// `prompt` used to park a bare `Sender` and return on the **next** `result` frame to
/// arrive, on the stated premise that a session runs one turn at a time and therefore the
/// next `result` must be ours. **The platform injects turns of its own into a lease that
/// dispatches helpers, and that premise is false.** Measured on the green
/// `work_lease_roundtrip` run of 2026-09-18
/// (`docs/verification/worker-turn-grant-2026-09-18.md` §5): of the six `UserPromptSubmit`
/// rows in that session's journal three were the platform's — two subagent hand-backs
/// (rows 52, 92) and one task-notification (row 55) — and the host's second continuation
/// returned **1.847 s** after the reviewer ended, with **zero** tool calls, because the
/// `result` of an injected turn already in flight was handed to it. The back end never got a
/// turn in which to land the work, and a passing review sat on the worker's branch while the
/// assignment was reported failed.
///
/// # THE CORRELATION THE PROTOCOL OFFERS, READ OFF THE BINARY THAT SHIPS
///
/// Two documented fields, both quoted from the schema embedded in `claude` 2.1.277 — the
/// binary this product spawns (`/Users/alex/.local/share/claude/versions/2.1.277`, read with
/// `strings`):
///
/// - **`command_lifecycle`** — `{type, command_uuid, state, uuid, session_id}`, where
///   `command_uuid` is *"The queued command's uuid — the client-supplied uuid on the inbound
///   message. Commands enqueued without a uuid (e.g. the one-shot `-p "prompt"` string path)
///   emit no lifecycle events."* `state` is one of
///   `queued | started | completed | cancelled | discarded | refused`: *"'queued' when the
///   inbound message enters the command queue; 'started' when it drains into a turn; then
///   exactly one terminal state"*. Ordering against the result frame is per-path and the
///   schema says which: *"a command that starts a fresh turn emits 'completed' AFTER that
///   turn's result frame ...; a command folded into an already-in-flight turn emits
///   'completed' BEFORE that turn's result frame."* The session announces the capability as
///   `msg_lifecycle_v1` on its `system/init` frame — present in the 2026-08-31 captures
///   (`docs/verification/native-claude-stream-json-2026-08-31/raw/run2-multiturn-one-process.jsonl`,
///   line 1). **So this client stamps a `uuid` on every user message it writes** — an
///   optional field of the documented inbound schema — and the child names it back.
/// - **`queued_turn_count`** on the `result` frame — *"User-initiated sends still waiting in
///   the command queue when this result was produced. Greater than 0 means at least one more
///   user turn (and result) follows without further input, barring cancellation; 0 means none
///   is pending... System-generated queue entries are not counted."* The platform's own
///   injections ARE system-generated: the subagent hand-back enqueues with `isMeta: true`, so
///   does the worker/task notification, and the child's own counting predicate excludes
///   `isMeta` entries. This client's send is the only user-initiated one there can be,
///   because there is at most one parked prompt.
///
/// # THE INVARIANT
///
/// **A parked prompt is answered by the `result` of the turn that consumed the message it
/// sent, never by the next `result` to arrive.** A `result` reaches it only when the child
/// has said our message drained into a turn, or — when the child has said nothing about our
/// message at all — has said nothing of ours is waiting behind it (`queued_turn_count == 0`).
/// A `result` that arrives while the child's last word on our message was `queued`, or that
/// the child reports with a user send still queued, belongs to a turn that was already in
/// flight: it is RETAINED on the between-turn lane (§1.4 G5 — traffic is never dropped), and
/// the prompt keeps waiting. (The lifecycle outranks the count: 2.1.283 was measured
/// reporting `0` on the platform's own turn while ours was still queued.)
///
/// Frames arriving while our message is known to be still queued are retained the same way
/// rather than streamed into this turn, because they belong to the other turn. Attributing
/// them here is the false attribution §1.4 G4 forbids.
///
/// # WHERE IT DEGRADES, AND HOW
///
/// A child that emits no `command_lifecycle` for our uuid leaves the phase `Unconfirmed`, and
/// then `queued_turn_count` alone decides — weaker (a count, not an identity) and still
/// strictly better than "the next result wins". **The count is that child's fallback only:**
/// once a child has named one of our turns, a later prompt it has not named yet is waiting
/// for admission, and no count decides for it (finding 47). The one turn still on the count
/// is a lease's first, before the child has shown either way. A child that offers NEITHER field does what
/// this file did before 2026-09-18: the next `result` is delivered. That is the honest floor,
/// and it is named here rather than hidden.
type NativePreparation = Box<dyn FnOnce() -> Result<Option<String>, String> + Send>;
type NativeCompletion = Box<dyn FnOnce() -> Result<(), String> + Send>;

struct PendingTurn {
    /// Where this turn's items go.
    sink: TurnSink,
    /// The `uuid` stamped on the user message this prompt wrote, which the child echoes as
    /// `command_uuid`. Minted per prompt, so a lifecycle frame can never be matched against a
    /// previous turn's message.
    command_uuid: String,
    prompt_text: String,
    hook_request_id: Option<String>,
    /// Host metadata returned by the native prompt hook, never prepended to user text.
    prepare: Option<NativePreparation>,
    complete: Option<NativeCompletion>,
    preparation_error: Option<String>,
    phase: TurnPhase,
    /// The mode of this submitted input, not a later input queued behind it.
    /// **Had this child already named a turn THIS client sent, when this one was parked?**
    /// (`ReaderState::turns_named_by_the_child`, copied at the send.) Hunt 2026-09-29 part 1
    /// v2, finding 47: on such a child `Unconfirmed` is not "the child says nothing about
    /// turns", it is "the child has not admitted OUR message yet" — the schema says a command
    /// sent with a uuid is named (`queued`/`started`, or `completed` before the result on the
    /// fold path) before any `result` can be its own. So a `result` that arrives first ends a
    /// turn that was already running, and the count on it is not consulted.
    child_names_turns: bool,
    /// **The lost connection ended this input's turn and it waits for the connection**
    /// ([`NativeClient::after_api_error`]). Nothing that arrives meanwhile is its: not a frame,
    /// not a `result`, not the child's `cancelled` for the message that failed.
    awaiting_connection: bool,
    /// The uuid of the message that carries this input's turn on after a lost connection. The
    /// child names that message, not ours, so from then on only this id is matched against its
    /// lifecycle; `command_uuid` stays the input's own id for everything else.
    continued_as: Option<String>,
}

impl PendingTurn {
    /// Is `command` the message the child is running this input under?
    fn named(&self, command: &str) -> bool {
        self.continued_as.as_deref().unwrap_or(&self.command_uuid) == command
    }

    fn send(&mut self, chunk: ChunkMsg, frame: &Value) -> bool {
        match &mut self.sink {
            TurnSink::Prompt(sink) => sink.send(chunk).is_ok(),
            TurnSink::Input { limit, overload, .. } => {
                if frame.get("parent_tool_use_id").is_none_or(Value::is_null) {
                    if let Some(reset) = usage_limit_signal(frame) { *limit = Some(reset); }
                    if let Some(failure) = overloaded_api_error_message(frame) { *overload = Some(failure.raw); }
                }
                false
            }
        }
    }

    fn finish(mut self, mut frame: Value, report_failure: bool) {
        // A queued input that never reached its hook never owned this session's scopes.
        if self.prepare.is_none() {
            if let Some(complete) = self.complete.take() {
                if let Err(error) = complete() {
                    frame = json!({"type":"result","is_error":true,"errors":[error]});
                }
            }
        }
        if let Some(error) = self.preparation_error {
            frame = json!({"type":"result","is_error":true,"errors":[error]});
        }
        match self.sink {
            TurnSink::Prompt(sink) => { let _ = sink.send(ChunkMsg::Done(frame)); }
            TurnSink::Input { completed, limit, overload } => {
                let result = if let Some(reset) = limit {
                    Err(NativeError::Protocol(crate::claude_accounts::usage_limit_error(reset, &frame.to_string())))
                } else {
                    // A turn that ended normally after Claude Code said it was overloaded is that
                    // overload, the shape the synchronous path already reads from the turn's text.
                    native_result(&frame).and_then(|reason| match overload {
                        Some(raw) if reason == "end_turn" => Err(NativeError::Protocol(raw)),
                        _ => Ok(reason),
                    })
                };
                completed(crate::steering::InputOutcome {
                    result: result.map_err(|error| error.to_string()), report_failure,
                });
            }
        }
    }

    /// **Is the turn in flight positively NOT ours yet?** True when the child said our
    /// message is still queued, or when a child that names its turns has not named this one
    /// yet (finding 47). Every place that decides whether a frame belongs to this prompt asks
    /// this one question, so the result, the streamed frames, the permission records and the
    /// background-command reading cannot disagree about whose turn is running.
    fn not_yet_ours(&self) -> bool {
        if self.awaiting_connection { return true; }
        match self.phase {
            TurnPhase::Queued => true,
            TurnPhase::Unconfirmed => self.child_names_turns,
            TurnPhase::Running => false,
        }
    }

    /// **Does this `result` belong to a turn that is not ours?** The whole decision, in one
    /// place, from the child's own words and nothing else.
    fn result_is_another_turns(&self, result: &Value) -> bool {
        // Waiting for the connection: no turn of ours is running at all.
        if self.awaiting_connection { return true; }
        // Our message is in the turn that is running: whatever is queued behind it, this
        // result ends OUR turn. Tested first, because `queued_turn_count` counts what is
        // WAITING and says nothing about what is running.
        if self.phase == TurnPhase::Running {
            return false;
        }
        // **The child has said, by name, that our message is still in its queue.** That is
        // positive evidence about OUR message, and it outranks the count, which is a number
        // about the queue at the moment the result was produced. Measured on 2.1.283
        // (2026-09-27): the platform's own `<task-notification>` turn ended with
        // `queued_turn_count: 0` while the lifecycle for our message had said only `queued`,
        // and `started` for it came 1 ms after that result. Reading the count first handed the
        // platform's result to this prompt. On the fold path the child says `completed`
        // BEFORE the result (schema, quoted on [`PendingTurn`]), so `Queued` here never means
        // our message was folded into this turn.
        //
        // **And a child that has named our turns before has not named this one yet** (hunt
        // 2026-09-29 part 1 v2, finding 47): the closing `result` of an earlier platform or
        // background turn, still unread when this prompt was parked, carries
        // `queued_turn_count: 0` because our message had not even been enqueued when it was
        // produced. Read by the count it ended this prompt empty, and the real answer then
        // streamed onto the between-turn lane.
        if self.not_yet_ours() {
            return true;
        }
        match result.get("queued_turn_count").and_then(Value::as_u64) {
            // The child's own count of user-initiated sends still waiting. Ours is the only
            // one there can be, so a positive count is the child saying "your turn has not
            // run yet".
            Some(waiting) => waiting >= 1,
            // No count on this frame, and the child has said nothing about our message (a
            // `Queued` one was answered above). Nothing is inferred and the result is
            // delivered.
            None => false,
        }
    }
}

// ===========================================================================================
// THE BETWEEN-TURN LANE (techy-mode design §1.5, gap #1)
//
// `dispatch` delivers to the prompt channel only while `current_prompt` is `Some`. Anything
// the agent emits at session start, or after a turn's `result` has already been returned,
// would otherwise hit NO SINK AT ALL.
//
// It is deliberately NOT a second `Sender`. A channel would need a receiver parked
// somewhere, and there is no drain loop running between turns by definition; the one thing
// that IS guaranteed to happen is that the spine comes back — to start the next turn, or
// because the CEO opened the technical view. So the lane is a BUFFER the reader thread fills
// and the spine drains, and the drain point is where `seq` is assigned, exactly as §1.4's
// feasibility argument requires for the in-turn path.
// ===========================================================================================

/// How many un-drained between-turn items the buffer holds before it starts refusing.
///
/// Sized against measurement, not taste. `run9-rust-driven.jsonl` shows the per-turn
/// repeating traffic on this wire is one `system/init`, one `system/status` and one
/// `rate_limit_event` — three kinds, all of which the SessionMeta slot below collapses to
/// nothing on a repeat. 256 is therefore ~85 turns of un-drained traffic in the shape
/// actually observed, against a drain that happens at every turn boundary and every
/// technical-view open. An overflow is a marker record, never a silent forget — see
/// [`MachineryRecord::between_turn_overflow`].
const BETWEEN_TURN_MAX: usize = 256;

/// One item the reader thread parked because no turn was in flight to route it to.
///
/// Mirrors the routable arms of [`ChunkMsg`] and none of its control arms: `Done` and
/// `Cancel` are statements about a turn, and there is no turn here. `Usage` is absent for a
/// sharper reason — the measurement is derived from a mid-turn `message_delta`, so it cannot
/// arise between turns at all.
enum BetweenItem {
    Frame(Value),
    Permission { request: Value, chosen: String },
}

impl BetweenItem {
    /// The same item as a streamed [`ChunkMsg`], for the case where a turn IS in flight.
    ///
    /// It CLONES rather than moving, and that is the deliberate trade. `Sender::send`
    /// consumes its argument, so a moving conversion would need an inverse to recover the
    /// item when the receiver has already hung up — an inverse whose `_` arm (`Done`,
    /// `Cancel`, `Usage`) is unreachable by construction and would therefore have to invent
    /// a record or panic on the reader thread. One `Value` clone per client-directed request
    /// buys away that whole arm.
    fn to_chunk(&self) -> ChunkMsg {
        match self {
            BetweenItem::Frame(f) => ChunkMsg::Frame(f.clone()),
            BetweenItem::Permission { request, chosen } => {
                ChunkMsg::Permission { request: request.clone(), chosen: chosen.clone() }
            }
        }
    }
}

/// The buffer plus §1.5's `last_session_meta` slot.
#[derive(Default)]
pub(crate) struct BetweenTurn {
    sink: Option<crate::cognition::BetweenTurnSink>,
    queue: Vec<BetweenItem>,
    /// §1.5's slot, and §1.2's *"last value wins"* made real: the last payload seen for each
    /// SessionMeta family member. An identical repeat is SUPPRESSED rather than queued.
    ///
    /// **Keyed on [`crate::machinery::meta_identity`], not on the frame.** The four
    /// `system/init` frames in `run9` differ ONLY by a per-frame `uuid`, so a verbatim
    /// comparison — which is what the ACP path could afford — would suppress nothing at all
    /// and this whole mechanism would be dead code that looked alive.
    ///
    /// **Per client, so per lease.** A rotation installs a fresh `NativeClient` with an
    /// empty slot, and the successor's first `system/init` is therefore recorded again. That
    /// is correct and not an oversight: it is a different session's statement about itself,
    /// and suppressing it would make the record claim the predecessor's tool list was the
    /// successor's.
    last_meta: std::collections::HashMap<String, Value>,
    /// Identical SessionMeta repeats not queued. Counted, not rendered — the record it would
    /// produce says nothing the retained one does not.
    suppressed: u64,
    /// Items refused because the buffer was full. Reported as a marker record at the next
    /// drain, then reset.
    dropped: u64,
    /// The lane's own counter. NOT §1.4 G1's shared per-turn counter and deliberately not
    /// pretending to be: there is no turn here and no text to interleave with, so this
    /// numbers the lane and nothing else. It is assigned at DRAIN (single-threaded, in the
    /// spine's call) rather than at `offer` (the reader thread), which is the same discipline
    /// §1.4 argues for on the in-turn path.
    next_seq: u64,
}

impl BetweenTurn {
    /// Park one frame the reader thread could not route.
    fn offer_frame(&mut self, frame: Value) {
        let title = crate::machinery::frame_title(&frame);
        // The ONE deliberate drop (§1.2), and it does not become less deliberate for
        // arriving between turns: the ledger already holds the CEO's words verbatim and
        // fsync'd. `from_native_between_turn` enforces it too; short-circuiting here keeps a
        // dropped frame from consuming a queue slot.
        if title == "user:text" {
            return;
        }
        if crate::machinery::is_session_meta(&title) {
            let identity = crate::machinery::meta_identity(&frame);
            if self.last_meta.get(&title) == Some(&identity) {
                self.suppressed += 1;
                return;
            }
            self.last_meta.insert(title, identity);
        }
        self.push(BetweenItem::Frame(frame));
    }

    /// A frame a turn already took, kept for the sink as well, and only when there is one.
    fn copy_to_sink(&mut self, frame: Value) {
        if self.sink.is_some() {
            self.push(BetweenItem::Frame(frame));
        }
    }

    fn push(&mut self, item: BetweenItem) {
        if self.queue.len() >= BETWEEN_TURN_MAX {
            self.dropped += 1;
            return;
        }
        self.queue.push(item);
    }

    /// Take everything parked, normalized, in arrival order. `thread_id` / `internal` are
    /// still unstamped — only the spine knows those (§1.5).
    fn drain(&mut self, session_id: &str) -> Vec<MachineryRecord> {
        let mut out = Vec::new();
        for item in std::mem::take(&mut self.queue) {
            let records = match item {
                BetweenItem::Frame(f) => {
                    MachineryRecord::from_native_between_turn(&f, session_id, self.next_seq)
                }
                BetweenItem::Permission { request, chosen } => {
                    vec![MachineryRecord::from_permission_request(&request, &chosen, session_id, self.next_seq)]
                }
            };
            // A dropped frame consumes no position, exactly as `prompt`'s loop does not
            // advance `seq` for a frame that normalizes to nothing.
            self.next_seq += records.len() as u64;
            out.extend(records);
        }
        if self.dropped > 0 {
            let seq = self.next_seq;
            self.next_seq += 1;
            out.push(MachineryRecord::between_turn_overflow(self.dropped, session_id, seq));
            self.dropped = 0;
        }
        out
    }

    /// Identical SessionMeta repeats suppressed by the slot, for the whole life of this
    /// client. Diagnostic: it is the number the "last value wins" rule saved, and a test
    /// asserts on it rather than on the absence of rows.
    fn suppressed(&self) -> u64 {
        self.suppressed
    }
}

#[derive(Default)]
struct OperationCancellation {
    active: bool,
    requested: bool,
}

#[derive(Clone)]
enum ActionGrant {
    Onboarding(std::path::PathBuf),
    Continuity(std::path::PathBuf),
    /// The assignment register's grant (`assignment_tools.rs`). Conversation leases only.
    Assignments(std::path::PathBuf),
    Questions(std::path::PathBuf),
    /// **The `richos_continuity` server's own grant, and the only one that is not opened at
    /// turn start** — the CEO's §55: the front desk's bookkeeping waits until he has been
    /// spoken to. Opened by [`ReaderState::he_has_now_been_spoken_to`], closed with the rest.
    ///
    /// It is a SECOND FILE rather than the flag on [`Self::Continuity`] because that one is
    /// `RICHOS_APP_SCOPE`, which the engine's hook reads to decide whether any tool at all may
    /// run — including the register itself. Measured, with the sentence it refuses with, in
    /// `prepare_work_turn`.
    ContinuityTools(std::path::PathBuf),
}

impl ActionGrant {
    fn set(&self, allowed: bool) -> Result<(), String> {
        match self {
            Self::Onboarding(path) => crate::onboarding_tools::set_actions_allowed(path, allowed),
            // **CLOSING THIS ONE IS A STOP, NOT A TURN END, AND THEY ARE NOT THE SAME EVENT.**
            // Every caller of this function with `false` is a withdrawal of authority — the
            // CEO's stop (`NativeCancelHandle::cancel`), a shutdown, and the rollback of a
            // turn-start that could not open every grant. An ordinary turn ending does NOT
            // come through here. The standing work connection keeps its grants after a
            // request ends, so native background completions can still use its tools.
            // A conversation closes only the request grant; a stop revokes both flags.
            Self::Continuity(path) if !allowed =>
                crate::ecs::revoke(path).map_err(|e| e.to_string()),
            Self::Continuity(path) | Self::ContinuityTools(path) =>
                crate::ecs::set_actions_allowed(path, allowed).map_err(|e| e.to_string()),
            // **An absent assignment scope is not a failure to grant; it is nothing to
            // grant.** The scope is written by `prepare_work_turn`, which is the first
            // point at which a company and a conversation exist — so before a thread is
            // bound there is no file, and opening or closing a grant over it is not a
            // meaningful operation. It fails CLOSED either way: with no scope the register
            // tool refuses on its own ("RichOS has not opened this conversation for
            // assignments"), and a deleted scope is the strongest revocation there is.
            //
            // A scope that EXISTS and cannot be written is still a hard failure, which is
            // what takes the lease down in the loop above — the same rule the other two
            // grants follow.
            Self::Questions(path) => crate::question_tools::set_actions_allowed(path, allowed),
            Self::Assignments(path) if !path.exists() => Ok(()),
            Self::Assignments(path) => crate::assignment_tools::set_actions_allowed(path, allowed),
        }
    }
    fn path(&self) -> &Path {
        match self {
            Self::Onboarding(path)
            | Self::Continuity(path)
            | Self::Assignments(path)
            | Self::Questions(path)
            | Self::ContinuityTools(path) => path,
        }
    }
}

/// **The turn-end signal of Stu's lease, for stall-watch's unfinished-work reminder.** The lease
/// runs with `--print`, whose transcript never records a turn's end (0 of 794 `sdk-cli`
/// transcripts did, richos-hq `6864034d0`), so the monitor takes the host's word instead
/// (`engine/scripts/lib/host_child.py` `idle`/`started`, read from the file `RICHOS_WATCH_LEAD`
/// names): `{"session", "started", "turn_ended"}`. `turn_ended` is present only while a turn has
/// ended with nothing awaited, and is removed when input is given or a turn begins. Written only
/// on a change; removed with the lease.
pub(crate) struct LeadSignal {
    path: std::path::PathBuf,
    session: String,
    started: u64,
    idle: Mutex<bool>,
}

impl LeadSignal {
    pub(crate) fn path_for(state: &Path, session: &str) -> std::path::PathBuf {
        state.join("stall-watch-lead").join(format!("{session}.json"))
    }
    pub(crate) fn new(path: std::path::PathBuf, session: &str) -> std::io::Result<Self> {
        let signal=Self {path,session:session.to_string(),started:crate::util::now_millis()/1000,idle:Mutex::new(false)};
        signal.write(false)?;
        Ok(signal)
    }
    fn write(&self, idle: bool) -> std::io::Result<()> {
        let mut body=json!({"session":self.session,"started":self.started});
        if idle {body["turn_ended"]=json!(crate::util::now_millis()/1000);}
        if let Some(dir)=self.path.parent() {std::fs::create_dir_all(dir)?;}
        let temporary=self.path.with_extension("json.tmp");
        std::fs::write(&temporary,serde_json::to_vec(&body)?)?;
        std::fs::rename(&temporary,&self.path)
    }
    fn set(&self, idle: bool) {
        let mut current=self.idle.lock().unwrap();
        if *current==idle {return}
        match self.write(idle) {
            Ok(())=>*current=idle,
            Err(error)=>eprintln!("[richos] stall-watch lead file {}: {error}",self.path.display()),
        }
    }
    /// A turn ended and nothing is awaited.
    fn ended(&self) {self.set(true)}
    /// Input was given, or a turn began.
    fn began(&self) {self.set(false)}
}

impl Drop for LeadSignal {
    fn drop(&mut self) {drop(std::fs::remove_file(&self.path));}
}

/// The existing engine monitor, owned by Stu's native lease. Its policy stays in the engine.
struct StallWatch {
    _child: crate::owned_process::OwnedChild,
    _output: JoinHandle<()>,
    lead: Arc<LeadSignal>,
    /// Set before the lease ends the monitor, so that end is not reported as a failure.
    closing: Arc<AtomicBool>,
}

/// What Stu is told when the monitor's output ends while its lease is still open
/// (orchestration reliability R5): the parent died or its output broke, and nothing else
/// would say so until a later manual check.
fn monitor_failure_text(why: &str) -> String {
    format!("STALL-WATCH: the monitor stopped unexpectedly ({why}). Nothing watches this conversation's \
teammates until the lease starts it again; its work is unchanged. Check: stall-watch.sh --alive")
}

impl StallWatch {
    fn start(profile: Option<&crate::engine_profile::EngineProfile>, role: LeaseRole,
        environment: &Command, session: &str, lease_pid: u32, closed: Arc<AtomicBool>)
        -> Result<Option<Self>, NativeError> {
        let Some(profile)=profile.filter(|_|role==LeaseRole::Work) else {return Ok(None)};
        let (entity,thread)=profile.work_scope.as_ref().ok_or_else(||NativeError::Protocol("Stu's monitor requires a conversation scope".into()))?;
        let scope=crate::status_tools::StatusToolScope {version:1,state_root:profile.state.clone(),entity_id:entity.clone(),thread_id:thread.clone()};
        let script=profile.engine.join("scripts/stall-watch.sh");
        if !script.is_file() {return Err(NativeError::Protocol("The engine's stall monitor is missing".into()))}
        let lead=Arc::new(LeadSignal::new(LeadSignal::path_for(&profile.state,session),session).map_err(NativeError::Io)?);
        let mut command=Command::new("/bin/bash");
        for (key,value) in environment.get_envs() {
            if let Some(value)=value {command.env(key,value);} else {command.env_remove(key);}
        }
        command.arg(script).args(["--monitor","--jsonl"])
            .current_dir(&profile.coordination).env("RICHOS_SESSION_ID",session)
            .env("RICHOS_WATCH_LEAD",&lead.path)
            .env("RICHOS_SESSION_PID",lease_pid.to_string())
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        crate::owned_process::OwnedChild::configure(&mut command);
        let mut child=command.spawn().map_err(NativeError::Io)?;
        let output=child.stdout.take().ok_or(NativeError::Closed)?;
        let child=crate::owned_process::OwnedChild::new(child);
        let closing=Arc::new(AtomicBool::new(false));
        let ending=Arc::clone(&closing);
        let output=std::thread::Builder::new().name("richos-stall-watch".into()).spawn(move || {
            // Keep each whole block until the existing durable input accepts it.
            let deliver=|text:&str| loop {
                match crate::session_tools::observe_stall(&scope,text) {
                    Ok(())=>break,
                    Err(error)=>{eprintln!("[richos] stall-watch input remains pending: {error}");if closed.load(Ordering::SeqCst) {return} std::thread::sleep(Duration::from_secs(5));}
                }
            };
            let mut failure="its output ended".to_string();
            for line in BufReader::new(output).lines() {
                let observation=(|| {
                    let line=line.map_err(|e|e.to_string())?;
                    let value:Value=serde_json::from_str(&line).map_err(|e|e.to_string())?;
                    value["text"].as_str().map(str::to_owned).ok_or_else(||"missing monitor text".to_string())
                })();
                match observation {
                    Ok(text)=>deliver(&text),
                    Err(error)=>{eprintln!("[richos] stall-watch output: {error}");failure=format!("its output could not be read: {error}");break}
                }
            }
            // The lease closing ends the monitor normally; anything else is a failure Stu is told of.
            if !ending.load(Ordering::SeqCst) && !closed.load(Ordering::SeqCst) {
                deliver(&monitor_failure_text(&failure));
            }
        }).map_err(NativeError::Io)?;
        Ok(Some(Self {_child:child,_output:output,lead,closing}))
    }
}

/// A live session to a native `claude` child.
pub struct NativeClient {
    stall_watch: Option<StallWatch>,
    child: crate::owned_process::OwnedChild,
    /// The supervisor's per-lease state file, when this lease is supervised (the product reap,
    /// `lease_commands.rs`): which tool commands are running outside the provider's group.
    reap_state: Option<std::path::PathBuf>,
    settle_workers_on_stop: bool,
    stdin: Arc<Mutex<ChildStdin>>,
    session_id: String,
    app_message_ids: Arc<Mutex<std::collections::HashSet<String>>>,
    next_id: Arc<AtomicI64>,
    /// Control-request replies, keyed by our `request_id`.
    pending: Arc<Mutex<std::collections::HashMap<String, Sender<Value>>>>,
    /// The parked prompt: the turn this client sent and is waiting for, and the sink its
    /// items are streamed to.
    ///
    /// **This used to say "there is at most one turn in flight, so the next `result` ends
    /// it", and that premise is FALSE on this wire** — see [`PendingTurn`] for the run that
    /// disproved it and the correlation the child actually offers.
    current_prompt: Arc<Mutex<Vec<PendingTurn>>>,
    operation_cancel: Arc<Mutex<OperationCancellation>>,
    action_grants: Vec<ActionGrant>,
    reader_closed: Arc<AtomicBool>,
    /// §1.5's machinery sink INDEPENDENT of the prompt channel.
    between: Arc<Mutex<BetweenTurn>>,
    /// The child's own stderr, bounded, so a startup failure can quote it verbatim.
    stderr_tail: Arc<Mutex<std::collections::VecDeque<String>>>,
    /// Shared with the reader thread, so `skills_verdict()` can answer what the child said
    /// about `--plugin-dir` without going through the turn channel.
    reader_state: Arc<Mutex<ReaderState>>,
    _reader: JoinHandle<()>,
    _stderr: JoinHandle<()>,
}

/// The mutable state the reader thread carries across frames within one turn.
///
/// Held behind one `Mutex` rather than separate atomics because the fields are only ever read
/// and written together, on one thread, and a single lock makes that obvious.
///
/// `Default` is written out below rather than derived, because `skills_verdict`'s honest
/// starting value is `NotYetReported` and a derived default would be whichever variant happens
/// to be declared first.
struct ReaderState {
    agent_tasks: crate::agent_tasks::TaskBook,
    /// Stu's turn-end signal for stall-watch; `None` on every lease that has no monitor.
    lead: Option<Arc<LeadSignal>>,
    /// How a turn that ended on an API failure learns whether the connection is up, and waits
    /// for it ([`NativeClient::after_api_error`]).
    connection: Arc<crate::connection::ConnectionWatch>,
    /// The API failure Claude Code ended the running turn with (`is_api_error_message`): its
    /// `error` kind, and its text when that text would have been delivered as the turn's words.
    /// Held until the turn's `result` says whether the turn carries on, so the vendor's line
    /// never becomes Rich's or Stu's words in a turn that goes on.
    api_error: Option<(Option<String>, Option<String>)>,
    /// The helpers (by their `parent_tool_use_id`) whose run the lost connection ended and whose
    /// parent is told once the connection is back ([`NativeClient::after_helper_api_error`]):
    /// one wait per helper, however many frames carry its failure.
    helpers_waiting: std::collections::HashSet<String>,
    /// The last usage reading the child streamed (`rate_limit_event`), and when (fill-first).
    streamed_usage: Option<crate::quota::StreamedReading>,
    /// **Where every `assistant` frame's usage goes, and the account this lease runs on**
    /// (`quota::tokens::Sink`): the lease writes no transcript, so this is the only place the
    /// quota service can see what the app's own Rich and teammates spend. `None` without an
    /// engine profile that names both.
    token_sink: Option<(String, std::sync::Arc<crate::quota::tokens::Sink>)>,
    /// This lease's running token count, so each `result` adds what the frames never showed.
    token_tally: crate::quota::tokens::Tally,
    question_scope: Option<std::path::PathBuf>,
    /// Did the child list all five `richos_work` tools? [`InitFact`], not `bool` — see its
    /// doc for what the collapse cost.
    work_tools_loaded: InitFact,
    /// Did the child list `mcp__richos_assignments__record`? A fact from its own init
    /// inventory, never from the config having been accepted (`assignment_tools.rs`).
    assignment_tool_loaded: bool,
    /// Did the child list `mcp__richos_status__background_work`? Same rule, and this one is
    /// only read once the work tools are refused.
    status_tool_loaded: bool,
    /// The model this session is running, from `system/init.model`.
    ///
    /// **Load-bearing, and finding §10 of the spike is why.** The probe's console line took
    /// an arbitrary entry from `result.modelUsage` and printed `contextWindow: 200000` —
    /// which is `claude-haiku-4-5`'s window, from a background side-call, not the session
    /// model's. The session model reports **1,000,000**. Keying the denominator by name is
    /// the difference between a watermark that rotates at 70% of a million and one that
    /// rotates at 70% of two hundred thousand.
    session_model: Option<String>,
    /// The context window, learned from a completed turn's `result.modelUsage`.
    ///
    /// **Spike caveat C3 lives in this `Option`.** The numerator is available mid-turn; the
    /// denominator only once a turn has ENDED, and the `initialize` reply lists five models
    /// and carries no `contextWindow` field at all (verified). So on the first turn of a
    /// fresh lease this is `None`, no measurement is emitted, and the spine stays on its
    /// chars÷4 estimate — which `spine.rs` already treats as a first-class state
    /// (`WatermarkSource::Estimate`). It is a real gap, it is small, and it is not papered
    /// over with a guessed denominator.
    ///
    /// **And smaller than C3 implies, measured live 2026-08-31.** The lease's first turn is
    /// the RE-PRIME turn (`Cognition::reprime`), which ends and fills this in before the CEO
    /// is handed a turn at all — `examples/watermark_roundtrip.rs` reports
    /// `source=measured` after his FIRST prompt. The gap belongs to a lease that was never
    /// primed, and nothing else.
    context_window: Option<u64>,
    /// Whether the binary actually loaded RichOS's skills, read off `system/init.plugins`.
    ///
    /// **The mitigation `--permission-prompt-tool` cannot have.** This file's module doc records
    /// that a flag which is still ACCEPTED but silently stops working is undetectable, because
    /// the initialize reply carries no field naming the permission prompt tool. For
    /// [`PLUGIN_DIR`] that field EXISTS — the init frame lists every loaded plugin by name,
    /// path and version (measured, `inner-doctrine-skills-2026-09-06/` cell K2) — so the same
    /// class of silent failure is caught here rather than merely regretted.
    ///
    /// It starts [`skills::SkillsVerdict::NotYetReported`] and stays there for a lease that has
    /// never run a turn, because the init frame lands with the first TURN and not with the
    /// handshake. That is a third state, not a pessimistic default: "nobody has told us" and
    /// "we were told no" call for opposite responses.
    skills_verdict: crate::skills::SkillsVerdict,
    /// Both exact app-owned onboarding tools, as reported by the child on its first turn.
    onboarding_tools_verdict: crate::onboarding_tools::OnboardingToolsVerdict,
    continuity_tools_loaded: bool,
    /// Did the child list [`crate::engine_profile::PLUGIN_NAME`] in `system/init.plugins`?
    ///
    /// **[`InitFact`] and not `bool`, and this is the field that proved why.** It starts
    /// [`InitFact::NotYetReported`] and stays there for a lease that has never run a turn —
    /// a third state, not a pessimistic default, exactly as `skills_verdict` above it.
    engine_plugin_loaded: InitFact,
    /// Did the child offer **its own** `ToolSearch` tool on this session?
    ///
    /// **The loudness layer for [`TOOL_SEARCH_ENV`], and the one fact that can catch the
    /// knob going away.** When the binary defers tools, the model has to DISCOVER the
    /// register before it can call it, and that discovery is a round trip the CEO waits
    /// through: measured at 3.9 s and again at 7.6 s on one question turn
    /// (`docs/verification/question-receipt-2026-09-18.md`, run 2). `ToolSearch` in the
    /// child's own init inventory is the wire saying deferral is live, so it is READ here
    /// rather than inferred from the environment variable having been set — exactly the
    /// argument `skills_verdict` above makes about `--plugin-dir`.
    ///
    /// [`InitFact`], for the reason the three facts above it are: before the first turn,
    /// nobody has told us.
    tool_search_offered: InitFact,
    /// **The tool-residency variable THIS lease was spawned with** — `Some("ENABLE_TOOL_SEARCH")`
    /// on the conversation, `None` on the work lease, straight off [`tool_residency_env`].
    ///
    /// **It is here because the alarm above was firing at a binary that had done nothing
    /// wrong.** The reader had no lease role, so one message served both leases, and it
    /// asserted *"`ENABLE_TOOL_SEARCH=false` is set for this lease"* on a lease where
    /// [`tool_residency_env`] deliberately sets nothing at all. On candidate .10 that is what
    /// happened: the work lease's own init frame tripped it, in `app.log` immediately before
    /// that lease's settlement line and AFTER the conversation lease had already called the
    /// register — and the audit that read it named a working knob as the cause of an 8–12 s
    /// first reply (`docs/verification/2026-09-18-nightly-1.2.0-nightly.20260918.5-onscreen-audit.md`,
    /// rows 1 and 4). A loudness layer that cannot say WHICH lease it is talking about is a
    /// false-alarm generator, and a false alarm that is believed costs more than silence.
    ///
    /// Set at spawn from the role, beside `continuity_tools_grant`, which
    /// are the two other fields the reader cannot learn from the wire.
    tool_residency: Option<&'static str>,
    /// **Has this child ever named a turn THIS client sent?** — the loudness layer for
    /// [`PendingTurn`], announced once per lease on stderr.
    ///
    /// It exists because the correlation has a silent fallback, and a silent fallback that
    /// is silently doing all the work is indistinguishable from the correlation working.
    /// The same argument as `tool_search_offered` and `skills_verdict` above: the fact is on
    /// the wire, so it is READ rather than assumed. Machinery — it never reaches the CEO.
    turns_named_by_the_child: bool,
    /// The commands this session's provider has put in the background, in the order it
    /// reported them ([`crate::cognition::BackgroundCommand`]). Bounded by
    /// [`BACKGROUND_COMMANDS_KEPT`]; an ended one is the first to go.
    background: Vec<crate::cognition::BackgroundCommand>,
}

/// **Whether a provider task of `task_type` is one the back end started and the provider will
/// report the end of to it** — what the host waits for and asks a report on (`work_host.rs`
/// step 3c). Every type in 2.1.283's own task-id table (`local_bash:"b"`, …, richos-hq
/// `docs/verification/2026-09-27-background-command-finish-2/`), sorted:
///
/// - **read**: `local_bash` (a shell command, and the `Monitor` tool's watch, measured:
///   `cap-plain.jsonl`, `cap-monitor.jsonl`), `local_workflow` (the `Workflow` tool, measured
///   ending 11.2 s after its turn with no `is_backgrounded` field: `cap-workflow.jsonl`), and,
///   from the binary only, `monitor_mcp` and `monitor_ws` (the `Monitor` tool's other sources),
///   `mcp_task` (a long-running MCP tool call) and `remote_agent` (a cloud session);
/// - **not read**: `local_agent` — in a work lease only a prepared helper, which the engine's
///   `dispatch_intent` enforces and step 3b already waits for on `SubagentStop`;
///   `in_process_teammate` — an agent-team member, which a work lease is not spawned for;
///   `dream` and `auto_mode_scan` — the provider's own housekeeping (`skipTranscript`), not
///   anything the back end started.
///
/// An unknown type is read: a new kind of background work whose end the provider reports is
/// the case this exists for, and missing it loses a finish, where reading it costs a wait his
/// Stop can end.
fn a_task_the_back_end_awaits(task_type: &str) -> bool {
    !matches!(task_type, "local_agent" | "in_process_teammate" | "dream" | "auto_mode_scan" | "")
}

/// How many background commands one session remembers. A back end that has started this many
/// is a loop, not a job; past it the oldest ENDED entry is forgotten first, so a running
/// command is never dropped while an ended one is kept.
const BACKGROUND_COMMANDS_KEPT: usize = 64;

impl ReaderState {
    /// Read one of the three task frames (see [`crate::cognition::BackgroundCommand`]).
    /// `ours_running`: a turn this client sent was running when the frame arrived.
    fn note_background(&mut self, frame: &Value, ours_running: bool) {
        let field = |value: &Value, name: &str| value.get(name).and_then(Value::as_str).unwrap_or("").to_string();
        match frame.get("subtype").and_then(Value::as_str).unwrap_or("") {
            // `is_backgrounded` is present only where a task can be either (a shell command:
            // `false` is a foreground one); a workflow's frame has none and is always in the
            // background (`cap-workflow.jsonl`). So only an explicit `false` excludes.
            "task_started"
                if a_task_the_back_end_awaits(frame.get("task_type").and_then(Value::as_str).unwrap_or(""))
                    && frame.get("is_backgrounded").and_then(Value::as_bool) != Some(false) =>
            {
                self.note_running(field(frame, "task_id"), field(frame, "description"));
            }
            // This list holds background tasks only (a foreground command never appears in it:
            // `cap-agent-slow.jsonl`, `cap-workflow.jsonl`).
            "background_tasks_changed" => {
                for task in frame.get("tasks").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
                    if a_task_the_back_end_awaits(task.get("task_type").and_then(Value::as_str).unwrap_or("")) {
                        self.note_running(field(task, "task_id"), field(task, "description"));
                    }
                }
            }
            "task_notification" => {
                let id = field(frame, "task_id");
                // Only a command this reader saw go to the background. A foreground command
                // gets a notification too (`cap-fold.jsonl`, 15.200 s) and is not one.
                if let Some(command) = self.background.iter_mut().find(|c| c.task_id == id && c.ended.is_none()) {
                    command.ended = Some(crate::cognition::CommandEnded {
                        status: field(frame, "status"),
                        summary: field(frame, "summary"),
                        during_a_turn_of_ours: ours_running,
                    });
                }
            }
            _ => {}
        }
    }

    fn note_running(&mut self, id: String, description: String) {
        if id.is_empty() || self.background.iter().any(|c| c.task_id == id) {
            return;
        }
        if self.background.len() >= BACKGROUND_COMMANDS_KEPT {
            match self.background.iter().position(|c| c.ended.is_some()) {
                Some(oldest_ended) => {
                    self.background.remove(oldest_ended);
                }
                // Every remembered command is still running: keep them and do not add
                // another, and say so, because a reading that silently stopped growing
                // would look like fewer commands than there are.
                None => {
                    eprintln!(
                        "[richos] this back end has {BACKGROUND_COMMANDS_KEPT} background commands \
                         running; a further one ({id}) is not being tracked"
                    );
                    return;
                }
            }
        }
        self.background.push(crate::cognition::BackgroundCommand { task_id: id, description, ended: None });
    }


}

impl Default for ReaderState {
    fn default() -> Self {
        ReaderState {
            agent_tasks: crate::agent_tasks::TaskBook::default(),
            lead: None,
            connection: Arc::new(crate::connection::ConnectionWatch::system()),
            api_error: None,
            helpers_waiting: Default::default(),
            question_scope: None,
            work_tools_loaded: InitFact::NotYetReported,
            assignment_tool_loaded: false,
            status_tool_loaded: false,
            session_model: None,
            context_window: None,
            skills_verdict: crate::skills::SkillsVerdict::NotYetReported,
            onboarding_tools_verdict: crate::onboarding_tools::OnboardingToolsVerdict::NotYetReported,
            continuity_tools_loaded: false,
            engine_plugin_loaded: InitFact::NotYetReported,
            tool_search_offered: InitFact::NotYetReported,
            // `None` — "this lease asks for nothing" — because that is the answer for a
            // reader nobody has told, and it is the one that cannot manufacture an alarm.
            // Production sets it from the role one statement after construction.
            tool_residency: None,
            turns_named_by_the_child: false,
            background: Vec::new(),
            streamed_usage: None,
            token_sink: None,
            token_tally: Default::default(),
        }
    }
}

/// **The freshest usage reading there is: the one the child streamed** (fill-first, the CEO's
/// correction of 2026-10-04). Every turn's `rate_limit_event` carries `rate_limit_info`, and on
/// this wire that names each window: captured on 2.1.x in
/// `docs/verification/claude-md-sentinel-2026-09-06/raw/cellB-empty-sources-no-claude-md.jsonl`
/// as `"unifiedWindows":{"five_hour":{"utilization":0.02,"resetsAt":1788701400},
/// "seven_day":{"utilization":0.58,"resetsAt":1789016400}}`, and in richos-hq
/// `docs/verification/2026-09-21-macos-walk-fixes/machinery/.../2026-09-21.raw.jsonl` as
/// `"seven_day":{"resetsAt":1790150400,"utilization":0.98}` with `status: allowed_warning`.
/// `utilization` is a FRACTION there and `resetsAt` is in SECONDS, so both are converted to
/// the probe's units (percent, milliseconds). A frame with no `unifiedWindows` (the SDK's
/// minimal `{"status":"allowed"}`) says nothing about a window and yields nothing.
pub fn streamed_windows(frame: &Value) -> Option<Vec<crate::quota::Window>> {
    let windows = frame.get("rate_limit_info")?.get("unifiedWindows")?.as_object()?;
    let out: Vec<_> = [("five_hour", "Five-hour", 5u64), ("seven_day", "Weekly", 168)].into_iter()
        .filter_map(|(id, label, hours)| {
            let w = windows.get(id)?;
            let used = w.get("utilization")?.as_f64().filter(|u| u.is_finite() && (0.0..=1.0).contains(u))? * 100.0;
            Some(crate::quota::Window {
                id: id.into(), label: label.into(), used_percent: used,
                resets_at: w.get("resetsAt").and_then(Value::as_u64).map(|s| s * 1000),
                duration_ms: hours * 3_600_000,
            })
        }).collect();
    (!out.is_empty()).then_some(out)
}

/// **Was this turn refused for a usage limit?** The three signals that really exist on this
/// wire (Frank's review, finding 1): a `rate_limit_event` whose `rate_limit_info.status` is
/// `"rejected"`; a `result` whose `api_error_status` is 429; a `result` whose `errors[]` holds
/// an entry starting `You've hit your`. There is no exit code 4 and no `limit_reached`.
/// Returns the reset time when the frame gives one (ms).
pub fn usage_limit_signal(frame: &Value) -> Option<Option<u64>> {
    match frame.get("type").and_then(Value::as_str) {
        Some("rate_limit_event") => {
            let info = frame.get("rate_limit_info")?;
            (info.get("status").and_then(Value::as_str) == Some("rejected"))
                .then(|| info.get("resetsAt").and_then(Value::as_u64).map(|s| s * 1000))
        }
        Some("result") => {
            let status = frame.get("api_error_status").and_then(Value::as_u64) == Some(429);
            let said = frame.get("errors").and_then(Value::as_array).is_some_and(|errors| errors.iter()
                .any(|e| e.as_str().is_some_and(|s| s.trim_start().starts_with("You've hit your"))));
            (status || said).then_some(None)
        }
        _ => None,
    }
}

/// Prove, BEFORE spawning, which of a launch's two preconditions is not met.
///
/// **This function exists because `ENOENT` is ambiguous by construction.**
/// `Command::spawn` raises `std::io::ErrorKind::NotFound` when the executable is missing
/// AND when `current_dir` names a directory that is not there. One errno, two faults, two
/// completely different fixes — and the mapping that guessed "missing binary" told the CEO
/// on 2026-09-01 that `~/.local/bin/claude` was not found while it sat on his disk
/// and only the engine directory was absent
/// (`docs/verification/payload-inventory-2026-09-01/README.md` §7). Whoever debugs that on a
/// customer's machine looks in exactly the wrong place.
///
/// **Order, and it is a decision.** The binary is checked first, because "Claude Code is not
/// installed" is the larger and more upstream errand of the two: an install with neither the
/// binary nor the engine directory needs the 197 MB install before the 4 MB directory means
/// anything. Each check names its own path, so a multi-fault install still gets a true
/// sentence — just the first true sentence rather than all of them.
///
/// **What it deliberately does NOT check.** A bare name resolved through `PATH` (the last
/// resort in [`resolve_claude_bin`]) has no path to test, so it falls through to the spawn,
/// where `ENOENT` is now unambiguous *because this function already cleared the working
/// directory*. Nothing here executes the binary or reads its contents: existence, file type
/// and the execute bit only.
fn preflight(bin: &Path, cwd: &Path, doctrine: Option<&Path>, skills: Option<&Path>) -> Result<(), NativeError> {
    // ---- the binary -------------------------------------------------------------------
    if bin.components().count() > 1 {
        match std::fs::metadata(bin) {
            Err(_) => return Err(NativeError::BinaryMissing { path: bin.display().to_string() }),
            Ok(meta) if meta.is_dir() => {
                return Err(NativeError::BinaryNotExecutable {
                    path: bin.display().to_string(),
                    why: "it is a directory".to_string(),
                })
            }
            Ok(meta) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    // Any execute bit — owner, group or other. A finer-grained answer would
                    // need the effective uid/gid, and `0o111 == 0` is the case that is
                    // certainly unrunnable; anything subtler surfaces as the
                    // `PermissionDenied` arm at the spawn, which maps to this same variant.
                    if meta.permissions().mode() & 0o111 == 0 {
                        return Err(NativeError::BinaryNotExecutable {
                            path: bin.display().to_string(),
                            why: format!("no execute permission (mode {:o})", meta.permissions().mode() & 0o777),
                        });
                    }
                }
                let _ = meta;
            }
        }
    }

    // ---- the working directory --------------------------------------------------------
    match std::fs::metadata(cwd) {
        Err(_) => return Err(NativeError::WorkingDirMissing { path: cwd.display().to_string() }),
        Ok(meta) if !meta.is_dir() => {
            return Err(NativeError::WorkingDirNotADirectory { path: cwd.display().to_string() })
        }
        Ok(_) => {}
    }

    // ---- the standing instruction ------------------------------------------------------
    //
    // `claude` itself refuses a missing file — `Error: Append system prompt file not found:
    // <path>`, exit 1, zero bytes of stdout, measured 2026-09-06 on 2.1.263 — so this check is
    // not what makes the failure loud. It makes it EARLY and SPECIFIC: a refusal here names the
    // doctrine file and says what it is for, instead of arriving as a startup error the reader
    // has to attribute among the four things that can produce one. Same reasoning, and the same
    // incident, as `WorkingDirMissing`: reporting one fault as another sends whoever set RichOS
    // up looking in the wrong place.
    //
    // EMPTY IS ALSO A FAILURE. A zero-byte file is a file `claude` accepts, so it would hand
    // the CEO a generic assistant with a clean handshake — precisely the silent degradation
    // this channel was chosen to avoid.
    if let Some(doctrine) = doctrine {
        match std::fs::metadata(doctrine) {
            Err(e) => {
                return Err(NativeError::DoctrineMissing {
                    path: doctrine.display().to_string(),
                    why: e.to_string(),
                })
            }
            Ok(meta) if !meta.is_file() => {
                return Err(NativeError::DoctrineMissing {
                    path: doctrine.display().to_string(),
                    why: "it is not a file".to_string(),
                })
            }
            Ok(meta) if meta.len() == 0 => {
                return Err(NativeError::DoctrineMissing {
                    path: doctrine.display().to_string(),
                    why: "it is empty, and an empty instruction is no instruction".to_string(),
                })
            }
            Ok(_) => {}
        }
    }

    // ---- the skills ---------------------------------------------------------------------
    //
    // THIS ONE IS NOT A NICETY. `claude` refuses a missing `--append-system-prompt-file` by
    // itself; it accepts a missing `--plugin-dir` in silence — exit 0, clean handshake,
    // `plugins: []`, an ordinary turn (measured, cell K4). So if this check is ever removed,
    // deleting a skill file stops being an error and becomes a product that is quietly less
    // than it says it is. The whole evening was about that failure shape.
    if let Some(skills) = skills {
        if let Err((path, why)) = crate::skills::verify_present(skills) {
            return Err(NativeError::SkillsMissing { path: path.display().to_string(), why });
        }
    }

    Ok(())
}

/// The per-lease MCP config the app writes when it spawns a child.
///
/// **Extracted from `spawn_with_tools` so it can be asserted without spawning anything.**
/// The background-work spec's §5.8a-ii seam 1 is a claim about what is IN this object and
/// what is not, and a claim of that shape is worth nothing if the only way to check it is
/// to start a provider. The negative — `richos_continuity` absent from a work lease's
/// config — is exactly the kind of assertion the spec's own checker had to be rebuilt to
/// be able to make.
///
/// - `richos_onboarding` is served by the app's own executable and is on the **conversation**
///   lease only. **It was on both until 2026-09-18, and that sentence is where Ray's
///   candidate-.8 row 1 was written down as a design fact.** Its scope is written only by
///   `spine.rs` (`:3093`, `:3278`), so the work lease's copy named a file nothing could ever
///   create, and opening the grant over it was the first act of the work lease's first turn.
/// - `richos_continuity` is scoped to each independent session.
/// - **`richos_work` is on the WORK lease only, as of the CEO's Two Riches page.** It used
///   to be on both, and the comment here said so in its own words: *"`richos_work` is on
///   both, because the work lease is the one that needs it most."* The page's sense-check
///   note 3 is *"The front desk gets no orchestration tools, so it cannot drift into doing
///   the work"*, and his own sentence for the shape is *"the job of the front desk Rich is
///   solely talking to the CEO and relaying info to and from the back-end Rich."* A front
///   desk holding `prepare`, `inspect`, `complete`, `repositories` and `integrate` is a
///   front desk that CAN do the work, and a model with a tool in front of it eventually
///   uses it. **The register is what the front desk hands work over with**
///   (`richos_assignments.record`, below) and `richos_status` is what it looks with.
/// - `richos_status` is on the **conversation** lease only (`status_tools.rs`). It is the
///   other half of note 3 and it is not optional beside the refusal above: take the work
///   tools away and the front desk has no READ at all — every status surface in this app is
///   a Tauri command reaching the webview, which the model never sees (Sage's check of the
///   page, finding 6).
fn mcp_config(executable: &Path, onboarding_scope: &Path, assignments_scope: &Path, status_scope: &Path,
    continuity: Option<(&crate::ecs::EcsBridge, &Path)>, continuity_tools_scope: Option<&Path>,
    profile: Option<&crate::engine_profile::EngineProfile>, role: LeaseRole, claude_bin: &Path) -> Value {
    let mut config = json!({"mcpServers": {}});
    config["mcpServers"][crate::question_tools::SERVER] = json!({
        "type":"stdio", "command": executable,
        "args":["--questions-mcp",crate::question_tools::path(assignments_scope)]
    });
    // **Putting a file in front of the user, on BOTH leases** (`rich_files.rs`, §138 item 12):
    // Rich answers with a file, and Stu hands over the file his job made. It reads the
    // question tool's scope file, so it is open exactly when a question would be.
    config["mcpServers"][crate::rich_files::SERVER] = json!({
        "type":"stdio", "command": executable,
        "args":["--files-mcp",crate::question_tools::path(assignments_scope)]
    });
    // **The company-notes tools, on the CONVERSATION lease only** (`onboarding_tools.rs`).
    //
    // **This was on both leases until 2026-09-18, and it is Ray's candidate-.8 row 1.** The
    // scope these tools read is written by `Cognition::set_onboarding_scope`, which is called
    // from two places and both are the conversation's (`spine.rs:3093`, `spine.rs:3278`). So
    // a work lease held two tools over a file nothing could ever write, and the first thing
    // its turn did was open a grant over it — see the grant list in `spawn_with_tools`.
    //
    // It belongs here for the same reason the register does, one paragraph down: saving the
    // CEO's answers about his own company is something he says in a visible turn, and a back
    // end that could rewrite them with a standing grant and no turn at all is the drift his
    // page closes. The back end needs them for nothing: its job is named in the assignment.
    if role == LeaseRole::Conversation {
        config["mcpServers"]["richos_onboarding"] = json!({
            "type": "stdio", "command": executable,
            "args": ["--onboarding-mcp", onboarding_scope]
        });
    }
    if role == LeaseRole::Conversation {
        config["mcpServers"][crate::assignment_tools::SERVER_NAME] = json!({
            "type":"stdio","command":executable,"args":["--assignments-mcp",assignments_scope]
        });
    }
    config["mcpServers"][crate::status_tools::SERVER_NAME] = json!({
        "type":"stdio", "command":executable, "args":["--status-mcp",status_scope]
    });
    // **The continuity tools read THEIR OWN scope file, not the hook's** — the two differ in
    // one flag and in when it opens (`prepare_work_turn`'s note, and the measurement that
    // forced it). `continuity_tools_scope` is `None` only on a lease with no bridge, where
    // this server is not registered at all; a conversation lease that had one and lost it
    // would register the server over the hook's file, which is the pre-2026-09-18 behavior and
    // never a silent upgrade — so it is `expect`-free and simply falls back to nothing.
    if let Some((bridge, scope)) = continuity {
        let tools_scope=continuity_tools_scope.unwrap_or(scope);
        config["mcpServers"]["richos_continuity"] = json!({"type":"stdio", "command":bridge.python,
            "args":[bridge.component.join("adapters/mcp.py"),tools_scope], "env":{"PYTHONDONTWRITEBYTECODE":"1"}});
    }
    if let (Some(profile), Some((bridge, scope))) = (profile, continuity) {
        if role == LeaseRole::Work {
            if let Some(root) = profile.state.parent() {
                config["mcpServers"][crate::quota::reset_tools::SERVER] = json!({"type":"stdio", "command":executable,
                    "args":["--claude-reset-mcp", scope, root, claude_bin]});
            }
        }
        let args = vec![profile.engine.join("mega-lander/app.py").to_string_lossy().to_string(), scope.to_string_lossy().to_string()];
        config["mcpServers"]["richos_work"] = json!({"type":"stdio", "command":bridge.python,
            "args":args, "env":{"PYTHONDONTWRITEBYTECODE":"1"}});
    }
    config
}

/// Resume from the existing scoped engine binding and the native transcript it witnessed.
/// No app summary or replacement session can stand in for a saved conversation.
fn native_resume(profile: Option<&crate::engine_profile::EngineProfile>,
    continuity: Option<(&crate::ecs::EcsBridge, &Path)>, role: LeaseRole,
) -> Result<Option<(String, std::path::PathBuf)>, NativeError> {
    let Some((profile, (bridge, _))) = profile.zip(continuity) else { return Ok(None) };
    let Some((entity, thread)) = profile.work_scope.as_ref() else { return Ok(None) };
    let (seat, audience, role_name) = match role {
        LeaseRole::Conversation => (crate::ecs::ceo_seat(thread), "ceo", "rich"),
        LeaseRole::Work => (Some(format!("stu-thread:{thread}")), "rich", "stu"),
    };
    let current = bridge.request("current", crate::ecs::seated_request(seat.as_deref(), json!({})))
        .map_err(|error| NativeError::Protocol(error.to_string()))?;
    if current["binding"].is_null() { return Ok(None) }
    let binding: crate::ecs::Binding = serde_json::from_value(current["binding"].clone())
        .map_err(|error| NativeError::Protocol(error.to_string()))?;
    if binding.entity_id != *entity || binding.thread_id != *thread || binding.audience != audience {
        return Err(NativeError::Protocol("The saved native session belongs to another conversation or role".into()));
    }
    uuid::Uuid::parse_str(&binding.session_id).map_err(|error|NativeError::Protocol(error.to_string()))?;
    let record: crate::session_tools::Session = serde_json::from_slice(&std::fs::read(
        profile.state.join("evidence").join(&binding.session_id).join("binding.json"))?)
        .map_err(|error| NativeError::Protocol(error.to_string()))?;
    if record.role != role_name {
        return Err(NativeError::Protocol("The saved native session belongs to another role".into()));
    }
    let scope = crate::status_tools::StatusToolScope { version: 1,
        state_root: profile.state.clone(), entity_id: entity.clone(), thread_id: thread.clone() };
    let path = crate::session_tools::native_path(&scope, &binding.session_id).map_err(NativeError::Protocol)?;
    std::fs::metadata(&path)?;
    Ok(Some((binding.session_id, path)))
}

// Native stores UserPromptSubmit context as hook_additional_context attachments.
// Reuse that history after resume instead of maintaining a second delivery ledger.
fn native_app_message_ids(path:&Path)->Result<std::collections::HashSet<String>,NativeError> {
    let mut ids=std::collections::HashSet::new();
    for line in BufReader::new(std::fs::File::open(path)?).lines() {
        let line=line?;
        let Ok(record)=serde_json::from_str::<Value>(&line) else {continue};
        if record.pointer("/attachment/type").and_then(Value::as_str)!=Some("hook_additional_context") {continue}
        let Some(contents)=record.pointer("/attachment/content").and_then(Value::as_array) else {continue};
        for content in contents.iter().filter_map(Value::as_str) {
            for line in content.lines() {
                let Ok(packet)=serde_json::from_str::<Value>(line) else {continue};
                if let Some(messages)=packet["richos_app_messages"].as_array() {
                    ids.extend(messages.iter().filter_map(|message|message["id"].as_str().map(str::to_string)));
                }
            }
        }
    }
    Ok(ids)
}

impl NativeClient {
    /// Spawn `claude`, run the `initialize` handshake, and return a client whose session id
    /// is already known.
    ///
    /// **The handshake is not optional and its failure is not recoverable here.** It is the
    /// one moment at which a rejected flag, a missing login, or a binary that is not `claude`
    /// at all can be caught before the CEO types anything — so it is done eagerly, with a
    /// bound, and its failure is a hard [`NativeError`] carrying the child's own stderr.
    /// **`doctrine` is required and is not an `Option`.** The chat lease is the CEO's
    /// conversation, and a chat lease with no standing instruction is the generic-Claude
    /// failure this whole channel exists to make impossible (`doctrine.rs`). Making it a
    /// parameter rather than something this function resolves for itself is the same call
    /// `entity.rs` makes about its own directory: the shell resolves `app_data_dir()` and that
    /// is the authority, so this file does not carry a second opinion about where it lives.
    pub fn spawn(bin: &Path, cwd: &Path, doctrine: &Path, skills: &Path) -> Result<Self, NativeError> {
        Self::spawn_with_tools(bin, cwd, Some((doctrine, skills)), None, None, None, None, None, LeaseRole::Conversation)
    }

    fn spawn_with_tools(bin: &Path, cwd: &Path, standing: Option<(&Path, &Path)>, onboarding: Option<(&Path, &Path, &Path, &Path)>, continuity: Option<(&crate::ecs::EcsBridge, &Path)>, continuity_tools: Option<&Path>, profile: Option<&crate::engine_profile::EngineProfile>, control: Option<&crate::steering::ConversationControl>, role: LeaseRole) -> Result<Self, NativeError> {
        let (doctrine, skills) = (standing.map(|s| s.0), standing.map(|s| s.1));
        preflight(bin, cwd, doctrine, skills)?;
        let remembered = native_resume(profile, continuity, role)?;
        let app_message_ids=match &remembered {Some((_,path))=>native_app_message_ids(path)?,None=>Default::default()};
        let session_id = remembered.as_ref().map(|(session,_)|session.clone())
            .unwrap_or_else(||uuid::Uuid::new_v4().to_string());
        let role_name = match role { LeaseRole::Conversation => "rich", LeaseRole::Work => "stu" };
        let mut args = match standing {
            Some((definition, skills)) if profile.is_some() || definition == skills.join(format!("agents/{role_name}.md")) => {
                let mut args = child_args(&session_id);
                args.extend(["--agent".into(), format!("{}:{role_name}", crate::skills::PLUGIN_NAME), PLUGIN_DIR.into(), skills.display().to_string()]);
                args
            }
            Some((doctrine, skills)) => chat_child_args(&session_id, doctrine, skills),
            None => child_args(&session_id),
        };
        if let Some((_, path)) = remembered {
            let position = args.iter().position(|arg|arg=="--session-id").expect("native arguments carry their session identity");
            args[position] = "--resume".into();
            args[position+1] = path.to_string_lossy().into_owned();
        }
        // The selected model, with low effort for replies and high effort for work.
        args.extend(lease_model_args(role, profile));
        if let Some((executable, scope, assignments, status)) = onboarding {
            let config = mcp_config(executable, scope, assignments, status, continuity, continuity_tools, profile, role, bin);
            args.extend(["--strict-mcp-config".into(), "--mcp-config".into(), config.to_string()]);
        }
        // The supervisor observes parent death, including a crash where Rust
        // destructors cannot run. Its provider child receives the actual PID.
        // **It reaps the lease's tool commands too** (the product reap gap design 1.3(a)): every
        // reap parameter is the supervisor's own argument, stripped before it execs `claude`, so
        // `claude`'s argv and environment are what they were (`lease_commands.rs`).
        let reap = match profile {
            Some(profile) => Some(crate::lease_commands::prepare(&profile.state, &session_id).map_err(NativeError::Io)?),
            None => None,
        };
        let mut command = if let (Some(profile), Some((state, log))) = (profile, &reap) {
            let mut supervisor = Command::new(&profile.runtime.python);
            supervisor.args(crate::lease_commands::supervisor_args(&profile.engine, state, log)).arg(bin);
            supervisor
        } else { Command::new(bin) };
        crate::owned_process::OwnedChild::configure(&mut command);
        // **The conversation's tools are RESIDENT from its first turn** ([`TOOL_SEARCH_ENV`]
        // says what that costs when they are not, with the frame timings). Set here rather
        // than in `EngineProfile::configure` because that function does not know the role,
        // and this is a per-role decision: it is set before `configure` runs and survives it,
        // which strips only `RICHOS_`/`LORO_`/`ECS_`/`GIT_` names. It reaches the provider
        // through `provider-supervisor.py` too — that supervisor `os.execvp`s, so the child
        // inherits `os.environ` unchanged (`operator_main` in `engine/scripts/provider-supervisor.py`;
        // the engine's R11 proves it equal to the flagless path's).
        if let Some((name, value)) = tool_residency_env(role) {
            command.env(name, value);
        }
        for (retry_name, retry_value) in lease_retry_env() {
            command.env(retry_name, retry_value);
        }
        if let Some(profile) = profile {
            let scope = continuity.ok_or_else(|| NativeError::Protocol("desktop engine needs a scoped continuity bridge".into()))?.1;
            profile.configure(&mut command,bin,&session_id,scope);
        }
        let mut child = command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| match e.kind() {
                // `preflight` has ALREADY proved the working directory exists and is a
                // directory, which is what makes this mapping sound: after that check, the
                // only thing left that can raise `ENOENT` here is the executable itself —
                // in practice a bare name that is not on `PATH`, the one case preflight
                // cannot check. Before that check this arm was a guess, and it guessed
                // wrong every time the engine directory was the missing thing.
                std::io::ErrorKind::NotFound => NativeError::BinaryMissing { path: bin.display().to_string() },
                std::io::ErrorKind::PermissionDenied => NativeError::BinaryNotExecutable {
                    path: bin.display().to_string(),
                    why: "the operating system refused to execute it".to_string(),
                },
                _ => NativeError::Io(e),
            })?;

        let lease_pid=child.id();
        let stdin = Arc::new(Mutex::new(child.stdin.take().ok_or(NativeError::Closed)?));
        let stdout = child.stdout.take().ok_or(NativeError::Closed)?;
        let stderr = child.stderr.take().ok_or(NativeError::Closed)?;

        let pending: Arc<Mutex<std::collections::HashMap<String, Sender<Value>>>> =
            Arc::new(Mutex::new(std::collections::HashMap::new()));
        let current_prompt: Arc<Mutex<Vec<PendingTurn>>> = Arc::new(Mutex::new(Vec::new()));
        let between: Arc<Mutex<BetweenTurn>> = Arc::new(Mutex::new(BetweenTurn::default()));
        let state: Arc<Mutex<ReaderState>> = Arc::new(Mutex::new(ReaderState::default()));
        state.lock().unwrap().question_scope = onboarding.map(|(_,_,path,_)|crate::question_tools::path(path));
        state.lock().unwrap().token_sink = profile
            .and_then(|p| Some((p.claude_account.as_ref()?.id.clone(), p.token_sink.clone()?)));
        // **The reader is told which lease it is on, from the same pure function that set the
        // environment** — not from a second opinion about the role. See
        // `ReaderState::tool_residency`: without this the work lease raised the front desk's
        // alarm, and an audit believed it.
        state.lock().unwrap().tool_residency = tool_residency_env(role).map(|(name, _)| name);
        let stderr_tail: Arc<Mutex<std::collections::VecDeque<String>>> =
            Arc::new(Mutex::new(std::collections::VecDeque::new()));
        // Reset at every `message_start`; read at every `assistant` frame. See
        // `dispatch`'s text handling — it is the guard against a silent clean-output
        // regression if `--include-partial-messages` ever stops working.
        let text_deltas: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        // Drain stderr so the child never blocks on a full pipe, and KEEP THE TAIL, because
        // on a rejected flag that one line is the entire diagnosis. Diagnostics are
        // machinery — they NEVER reach the CEO; the tail is for the error message and for
        // developer debugging.
        let tail = Arc::clone(&stderr_tail);
        // Set when the child's stderr reaches EOF, so a turn that ends because stdout did can
        // wait (bounded) for the last lines the child wrote on its way out (`child_exit_said`).
        let stderr_done = Arc::new(AtomicBool::new(false));
        let stderr_done_flag = Arc::clone(&stderr_done);
        let stderr_handle = std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if std::env::var("RICHOS_CLAUDE_DEBUG").is_ok() {
                    eprintln!("[claude-stderr] {line}");
                }
                let mut t = tail.lock().unwrap();
                if t.len() >= STDERR_TAIL_LINES {
                    t.pop_front();
                }
                t.push_back(line);
            }
            stderr_done_flag.store(true, Ordering::SeqCst);
        });
        // What the turn's end needs to say WHY the child went (`NativeError::Exited`): its own
        // stderr, and Claude Code's own answer about the sign-in this lease runs under.
        let exit_tail = Arc::clone(&stderr_tail);
        let exit_auth = (bin.to_path_buf(), profile.and_then(|p| p.claude_account.as_ref()).and_then(|a| a.folder.clone()));

        let reader_closed = Arc::new(AtomicBool::new(false));
        let reader_closed_flag = Arc::clone(&reader_closed);
        let reader_stdin = Arc::clone(&stdin);
        let reader_pending = Arc::clone(&pending);
        let reader_current = Arc::clone(&current_prompt);
        let reader_between = Arc::clone(&between);
        let reader_state = Arc::clone(&state);
        let reader_text_deltas = Arc::clone(&text_deltas);
        let reader_handle = std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let msg: Value = match serde_json::from_str(line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                Self::dispatch(
                    msg,
                    &reader_stdin,
                    &reader_pending,
                    &reader_current,
                    &reader_between,
                    &reader_state,
                    &reader_text_deltas,
                    &reader_closed_flag,
                );
                Self::release_between_turn(&reader_between);
            }
            // stdout closed. §5.2's POSITIVE termination signal: the child's stdout reached
            // EOF, which is a fact about the child and not an inference from silence. Fail
            // every waiter so no caller hangs forever.
            reader_pending.lock().unwrap().clear();
            reader_closed_flag.store(true, Ordering::SeqCst);
            let waiting = std::mem::take(&mut *reader_current.lock().unwrap());
            // Only a turn that was waiting is owed a reason; an idle lease ending says nothing.
            let said = if waiting.is_empty() { String::new() } else {
                child_exit_said(&stderr_done, &exit_tail, || {
                    crate::provider_auth::status_within(&exit_auth.0, exit_auth.1.as_deref(), CHILD_EXIT_AUTH_WITHIN).state
                        == crate::provider_auth::AuthState::SignedOut
                })
            };
            finish_native_turn(waiting, json!({"stop_reason":"child_exited","child_said":said}));
        });

        // A supervised leader gets SIGTERM first and reaps its tree; only a bare child (no
        // engine profile, no supervisor) keeps the immediate group SIGKILL (`owned_process.rs`).
        let reap_state = reap.map(|(state, _)| state);
        if let Some(state) = &reap_state {
            crate::lease_commands::LiveLeases::process().register(&session_id, state);
        }
        let mut client = NativeClient {
            stall_watch: None,
            child: if reap_state.is_some() {
                crate::owned_process::OwnedChild::supervised(child)
            } else {
                crate::owned_process::OwnedChild::new(child)
            },
            reap_state,
            settle_workers_on_stop: profile.is_some(),
            stdin,
            session_id,
            app_message_ids: Arc::new(Mutex::new(app_message_ids)),
            next_id: Arc::new(AtomicI64::new(1)),
            pending,
            current_prompt,
            operation_cancel: Arc::new(Mutex::new(OperationCancellation::default())),
            // The assignments grant rides with the other two: open at turn start, closed at
            // turn end, by the same all-or-nothing loop. So `richos_assignments.record`
            // is reachable in exactly the window `richos_onboarding` is — a visible CEO
            // turn — and in no other, which is what spec §5.1 says about a request with no
            // turn open, kept by construction rather than by a check inside the tool alone.
            // **The status scope takes no grant and so is not in this list.** It is the one
            // app-owned scope with nothing to open or close: `status_tools.rs` reads and
            // writes nothing, and its own module comment says why gating a read on a rule
            // written for writes would only produce a front desk that goes blind between
            // his turns.
            //
            // **THE ONBOARDING GRANT IS CONVERSATION-ONLY, AND THIS LINE IS RAY'S
            // CANDIDATE-.8 ROW 1.** Until 2026-09-18 only `Assignments` carried the role
            // filter, so a WORK lease took a grant over `{identity}-onboarding.json` —
            // a path `start_work_lease` computes and nothing on earth writes
            // (`Cognition::set_onboarding_scope` is called from `spine.rs:3093` and
            // `spine.rs:3278`, both the conversation's). `prompt`'s first act is to open
            // every grant in this list, so the work lease's first turn opened a grant over
            // a file that was not there, `read_scope` mapped the missing file to
            // *"Choose a company before saving interview answers."*
            // (`onboarding_tools.rs:137`), and `prompt` returned that as
            // `CognitionError::Io` before sending the turn. Measured on his walk: **6.317 s**
            // after registration, no model token spent, `notes.txt` unchanged.
            //
            // **Deliberately a role filter and not an `exists()` guard like
            // `Assignments`'s.** On a conversation lease a missing onboarding scope means no
            // company is bound yet, and failing hard there is the check that stops a model
            // writing company notes with no company chosen — that sentence is *right* in
            // that seat. The defect was never the sentence; it was a work lease standing in
            // the seat that says it.
            action_grants: onboarding.filter(|_| role == LeaseRole::Conversation)
                .map(|(_, path, _, _)| ActionGrant::Onboarding(path.into())).into_iter()
                .chain(onboarding.filter(|_|role == LeaseRole::Conversation).map(|(_, _, path, _)| ActionGrant::Assignments(path.into())))
                .chain(onboarding.map(|(_,_,path,_)|ActionGrant::Questions(crate::question_tools::path(path))))
                .chain(continuity.map(|(_, path)| ActionGrant::Continuity(path.into())))
                // **The continuity tools' own grant is in this list for the CLOSING half
                // only.** `prompt` deliberately does not open it with the others (see there);
                // it is here so the turn's end, a cancel and a dropped lease all revoke it by
                // the same all-or-nothing loop that revokes the rest — a deferred grant that
                // could be left open by a path that forgot it would be worse than no deferral.
                .chain(continuity_tools.map(|path| ActionGrant::ContinuityTools(path.into())))
                .collect(),
            reader_closed,
            between,
            stderr_tail,
            reader_state: Arc::clone(&state),
            _reader: reader_handle,
            _stderr: stderr_handle,
        };
        client.handshake_cancellable(control)?;
        client.stall_watch=StallWatch::start(profile,role,&command,&client.session_id,lease_pid,Arc::clone(&client.reader_closed))?;
        if let Some(watch)=&client.stall_watch {client.reader_state.lock().unwrap().lead=Some(Arc::clone(&watch.lead));}
        Ok(client)
    }

    /// The `initialize` control handshake. Liveness, and nothing else.
    ///
    /// **It reads the reply's `subtype` and keeps none of its contents.** That reply carries
    /// the customer's `account` (email, organization, `subscriptionType`) and RichOS may
    /// never collect, store or intermediate it — see the module doc's licence section. The
    /// reply is dropped on the floor the moment it proves the child is alive.
    ///
    /// It costs NO API turn: measured 697.9 ms on 2.1.252, and `run10` of the spike
    /// established that control requests are free.
    fn handshake_cancellable(&mut self, control: Option<&crate::steering::ConversationControl>) -> Result<(), NativeError> {
        let (tx, rx): (Sender<Value>, Receiver<Value>) = channel();
        self.pending.lock().unwrap().insert("req_init".to_string(), tx);
        let msg = json!({
            "type": "control_request", "request_id": "req_init",
            "request": { "subtype": "initialize", "hooks": {
                "UserPromptSubmit":[{"hookCallbackIds":["richos-input-context"]}]
            } }
        });
        if let Err(e) = Self::write_line(&self.stdin, &msg) {
            // ONE CAUSE, ONE SENTENCE — and until 2026-09-04 this one had two.
            //
            // A child that rejects a flag writes its line and is gone in a millisecond or
            // two, so whether this process gets the handshake into the pipe before the
            // kernel tears it down is a SCHEDULER RACE and nothing else. Win it and the
            // write succeeds, the reader hits stdout EOF, and the arm below says "the child
            // exited before answering the handshake". Lose it and the write returns `EPIPE`
            // and the operator was told, for the identical fault, "could not write the
            // initialize handshake: claude io: Broken pipe (os error 32)".
            //
            // Two bug reports for one cause, decided by machine speed. This machine wins the
            // race every time; a GitHub `macos-latest` runner lost it on the first public
            // run of `app-spine-ci` (33872961756, 2026-09-04) and the only test that names
            // the sentence went red — over the product's nondeterminism, not over the test.
            //
            // THE EXIT IS CONFIRMED, NEVER INFERRED. A broken pipe is a positive fact about
            // the PIPE and only a guess about the process: a child that closes its input and
            // stays up breaks it too, and "the child exited" would then be a false statement
            // in a diagnosis. So the pipe sends us to `waitpid`, and `waitpid` decides.
            let broken = matches!(&e, NativeError::Io(io) if io.kind() == std::io::ErrorKind::BrokenPipe);
            let exited = broken && self.child_has_exited();
            return Err(self.startup_error(Self::write_failure_reason(&e, broken, exited)));
        }
        let deadline = std::time::Instant::now() + HANDSHAKE_TIMEOUT;
        let response = loop {
            if control.and_then(|c| c.stop_claim()).is_some() {
                self.pending.lock().unwrap().remove("req_init");
                return Err(NativeError::Protocol("Connection stopped at your request.".into()));
            }
            let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
                break Err(RecvTimeoutError::Timeout);
            };
            match rx.recv_timeout(remaining.min(std::time::Duration::from_millis(50))) {
                Err(RecvTimeoutError::Timeout) => continue,
                result => break result,
            }
        };
        match response {
            Ok(resp) => {
                let subtype = resp.get("response").and_then(|r| r.get("subtype")).and_then(|v| v.as_str());
                if subtype == Some("success") {
                    // Deliberately NOT reading `response.response.account`. See the doc above.
                    Ok(())
                } else {
                    Err(self.startup_error(format!(
                        "the initialize handshake was refused (subtype {subtype:?})"
                    )))
                }
            }
            // The reader thread cleared the waiters, which it does at stdout EOF — i.e. the
            // child died. On a rejected flag this is the path taken, within milliseconds,
            // and `stderr_tail` holds `error: unknown option '…'`.
            Err(RecvTimeoutError::Disconnected) => {
                Err(self.startup_error("the child exited before answering the handshake".into()))
            }
            Err(RecvTimeoutError::Timeout) => Err(self.startup_error(format!(
                "no answer to the initialize handshake within {}s",
                HANDSHAKE_TIMEOUT.as_secs()
            ))),
        }
    }

    /// Build the loud startup error, quoting the child verbatim.
    ///
    /// Gives the stderr thread a brief moment to catch up first: on a rejected flag the
    /// child writes its one line and exits, and reading the tail the same instant the
    /// waiter is cleared can race the line that IS the diagnosis. 200 ms against a failure
    /// path that only ever runs once, at boot.
    fn startup_error(&self, reason: String) -> NativeError {
        std::thread::sleep(Duration::from_millis(200));
        let stderr = {
            let t = self.stderr_tail.lock().unwrap();
            if t.is_empty() {
                "(nothing on stderr)".to_string()
            } else {
                t.iter().cloned().collect::<Vec<_>>().join(" / ")
            }
        };
        NativeError::Startup { reason, stderr }
    }





    /// The sentence a failed handshake WRITE gets, as a pure function of the three facts
    /// that decide it.
    ///
    /// PURE ON PURPOSE. The caller's inputs come out of a race, and a test that had to WIN
    /// that race to exercise an arm would be a test that passes on whichever machine it is
    /// run on — which is the exact defect this function exists to remove. Split out, both
    /// arms are provable on any machine, in microseconds, with no child at all.
    fn write_failure_reason(e: &NativeError, broken_pipe: bool, child_exited: bool) -> String {
        if broken_pipe && child_exited {
            // The same words the `Disconnected` arm below uses, because it is the same fact:
            // the child was gone before it answered.
            "the child exited before answering the handshake".to_string()
        } else if broken_pipe {
            // A live child that shut its own input. Rarer, and a DIFFERENT fault with a
            // different fix, so it does not get to borrow the sentence above.
            "the child closed its input before answering the handshake".to_string()
        } else {
            format!("could not write the initialize handshake: {e}")
        }
    }

    /// Has the child really gone — `waitpid`, not an inference.
    ///
    /// Bounded at ~200 ms because reaping is not instantaneous: the write can return `EPIPE`
    /// the moment the kernel tears the pipe down, a beat before the process is reapable. It
    /// runs once, on a boot that has already failed, so the cost is paid by nothing that
    /// works.
    fn child_has_exited(&mut self) -> bool {
        for _ in 0..40 {
            match self.child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(_) => return false,
            }
        }
        false
    }

    /// Route one inbound frame: control response -> waiter, control request -> answered
    /// here, everything else -> the active turn's chunk sink or the between-turn lane.
    #[allow(clippy::too_many_arguments)]
    fn dispatch(
        msg: Value,
        stdin: &Arc<Mutex<ChildStdin>>,
        pending: &Arc<Mutex<std::collections::HashMap<String, Sender<Value>>>>,
        current: &Arc<Mutex<Vec<PendingTurn>>>,
        between: &Arc<Mutex<BetweenTurn>>,
        state: &Arc<Mutex<ReaderState>>,
        text_deltas: &Arc<AtomicUsize>,
        closed: &Arc<AtomicBool>,
    ) {
        drop(state.lock().unwrap().agent_tasks.observe(&msg));
        let ty = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");

        // Kept whichever turn (or none) the frame belongs to: it is a reading of the account.
        if ty == "rate_limit_event" {
            if let Some(windows) = streamed_windows(&msg) {
                state.lock().unwrap().streamed_usage = Some((windows, crate::util::now_millis()));
            }
        }
        // Every assistant message's tokens, whichever turn it belongs to and whoever said it
        // (a helper's nested frames too, a background helper's included), and from each
        // `result` the tokens no frame showed (`quota::tokens::Tally`): they are all spent on
        // this lease's account. The streamed `message_start`/`message_delta` events tell the
        // tally each message's final output, so the spend prices it on its own request (§118).
        if ty == "assistant" || ty == "result" || ty == "stream_event" {
            let mut st = state.lock().unwrap();
            if let Some((account, sink)) = st.token_sink.clone() {
                for found in st.token_tally.frame(&msg, crate::util::now_millis()) { sink.push(&account, found); }
            }
        }

        // ---- the control protocol ------------------------------------------------------
        if ty == "control_response" {
            if let Some(id) = msg.get("response").and_then(|r| r.get("request_id")).and_then(|v| v.as_str()) {
                if let Some(tx) = pending.lock().unwrap().remove(id) {
                    let _ = tx.send(msg);
                    return;
                }
            }
            // An unmatched control response is still traffic and is retained rather than
            // dropped (§1.4 G5).
            Self::route(ChunkMsg::Frame(msg.clone()), current, between, &msg);
            return;
        }
        if ty == "control_request" && msg.pointer("/request/subtype").and_then(Value::as_str) == Some("hook_callback")
            && msg.pointer("/request/callback_id").and_then(Value::as_str) == Some("richos-input-context") {
            // An input folded into a running turn has its own hook before `started`.
            // Inputs drained together into a NEW turn are all named `started` first,
            // then share one hook. Their started UUIDs identify that batch; the
            // flattened hook text varies with the native content encoding.
            // Claim that native batch once, using the same per-input preparation.
            let request_id=msg.get("request_id").and_then(Value::as_str);
            let text=msg.pointer("/request/input/prompt").and_then(Value::as_str);
            let (commands,preparations)={
                let mut inputs=current.lock().unwrap();
                let mut indices=Vec::new();
                if let Some((id,text))=request_id.zip(text).filter(|(id,_)|
                    !inputs.iter().any(|input|input.hook_request_id.as_deref()==Some(id))) {
                    let batch:Vec<_>=inputs.iter().enumerate().filter(|(_,input)|
                        input.phase==TurnPhase::Running && input.hook_request_id.is_none()).map(|(index,_)|index).collect();
                    if batch.len()>1 {
                        indices=batch;
                    } else if let Some(index)=inputs.iter().position(|input|input.hook_request_id.is_none()&&input.prompt_text==text) {
                        indices.push(index);
                    }
                    for &index in &indices {inputs[index].hook_request_id=Some(id.into());}
                }
                let commands:Vec<_>=indices.iter().map(|&index|inputs[index].command_uuid.clone()).collect();
                let preparations:Vec<_>=indices.into_iter().filter_map(|index|inputs[index].prepare.take()).collect();
                (commands,preparations)
            };
            let contexts=preparations.into_iter().map(|prepare|prepare()).collect::<Result<Vec<_>,_>>();
            let body = match contexts {
                Ok(contexts) => {
                    let contexts:Vec<_>=contexts.into_iter().flatten().collect();
                    if contexts.is_empty() {json!({})} else {json!({"hookSpecificOutput":{
                        "hookEventName":"UserPromptSubmit","additionalContext":contexts.join("\n")
                    }})}
                },
                Err(error) => {
                    for input in current.lock().unwrap().iter_mut().filter(|input|commands.contains(&input.command_uuid)) {
                        input.preparation_error = Some(error.clone());
                    }
                    json!({"decision":"block","reason":error})
                },
            };
            let response = json!({"type":"control_response","response":{
                "subtype":"success","request_id":msg["request_id"],"response":body
            }});
            let _ = Self::write_line(stdin, &response);
            Self::route(ChunkMsg::Frame(msg.clone()), current, between, &msg);
            return;
        }
        if ty == "control_request" {
            let question_scope=state.lock().unwrap().question_scope.clone();
            Self::handle_agent_request(&msg, stdin, current, between, question_scope.as_deref());
            return;
        }

        // ---- the fate of the message WE sent (see [`PendingTurn`]) ---------------------
        //
        // The one frame on this wire that names a turn by the id its sender chose. It is
        // read for exactly one purpose: to know whether the turn in flight is ours. It is
        // then RETAINED like any other traffic — a lifecycle frame for another command is
        // somebody else's machinery and is not this turn's.
        if ty == "command_lifecycle" {
            let command = msg.get("command_uuid").and_then(|v| v.as_str()).unwrap_or("");
            let lifecycle = msg.get("state").and_then(|v| v.as_str()).unwrap_or("");
            let mut terminal: Option<&'static str> = None;
            {
                let mut guard = current.lock().unwrap();
                // A named lifecycle establishes this child's native identity protocol for
                // inputs already sent before its first acknowledgement too.
                if guard.iter().any(|pending| pending.named(command)) {
                    for pending in guard.iter_mut() { pending.child_names_turns = true; }
                }
                // An input waiting for the connection hears nothing about the message that
                // failed (the child says `cancelled` for it right after the `result`), and the
                // inputs one continuation carries on are all named by it.
                for pending in guard.iter_mut().filter(|pending| pending.named(command) && !pending.awaiting_connection) {
                        // **THE LOUDNESS LAYER**: said once, at the transition, on stderr.
                        // See `ReaderState::turns_named_by_the_child` — a fallback quietly
                        // carrying the whole load looks exactly like the thing working.
                        let mut st = state.lock().unwrap();
                        if !st.turns_named_by_the_child {
                            st.turns_named_by_the_child = true;
                            eprintln!(
                                "[richos] conversation connection: replies are matched to their requests."
                            );
                        }
                        drop(st);
                        match lifecycle {
                            // In the queue, behind whatever is running. Everything until
                            // `started` belongs to that other turn.
                            "queued" => pending.phase = TurnPhase::Queued,
                            // Drained into a turn — the turn in flight is ours from here.
                            // `completed` reaches a still-parked prompt only on the FOLD
                            // path, where the schema says it precedes that turn's `result`;
                            // either way the next `result` is the one that consumed us.
                            "started" | "completed" => pending.phase = TurnPhase::Running,
                            // **A POSITIVE "this will never run".** Without it the prompt
                            // would wait for a turn the child has already thrown away —
                            // the one way this correlation could be worse than the defect
                            // it replaces. Three states, one sentence each.
                            "cancelled" => terminal = Some(STOP_REASON_CANCELLED), // dialect-exempt: the vendor's own `command_lifecycle` state value, matched verbatim off the wire
                            "discarded" => terminal = Some("discarded_before_it_ran"),
                            "refused" => terminal = Some("refused_before_it_ran"),
                            _ => {}
                        }
                }
            }
            if let Some(reason) = terminal {
                let ended: Vec<PendingTurn> = {
                    let mut guard = current.lock().unwrap();
                    let (ended, kept) = std::mem::take(&mut *guard).into_iter()
                        .partition(|pending| pending.named(command) && !pending.awaiting_connection);
                    *guard = kept;
                    ended
                };
                for pending in ended { pending.finish(json!({"stop_reason":reason}), true); }
            }
            Self::route(ChunkMsg::Frame(msg.clone()), current, between, &msg);
            return;
        }

        // Nested agent frames are operational evidence, never the lead's words,
        // context watermark or terminal state. In particular, whole-message fallback
        // must not promote worker narration when the lead has no active text stream.
        if msg.get("parent_tool_use_id").is_some_and(|parent| !parent.is_null()) {
            // A helper's run Claude Code ended on an API failure (its message carries `error`)
            // may have been ended by the lost connection: its parent is told once it is back.
            if ty == "assistant" && msg.get("error").is_some_and(|error| !error.is_null()) {
                Self::after_helper_api_error(&msg, stdin, state, closed);
            }
            // A worker's whole message also reaches the between-turn sink when a turn took it:
            // Stu's host reads his workers' overloads there (`work_host.rs` observe_worker),
            // whichever turn carried them. A lease with no sink keeps nothing extra.
            if Self::route(ChunkMsg::Frame(msg.clone()), current, between, &msg) && ty == "assistant" {
                between.lock().unwrap().copy_to_sink(msg);
            }
            return;
        }

        // A turn the platform started itself (a helper's hand-back, a task notification) begins
        // with the lead's own output: it is not between turns any more.
        if ty == "assistant" || ty == "stream_event" {
            let lead=state.lock().unwrap().lead.clone();
            if let Some(lead)=lead {lead.began();}
        }

        // ---- the terminal frame --------------------------------------------------------
        if ty == "result" {
            // **THE DENOMINATOR IS A FACT ABOUT THE MODEL, so it is learned from whichever
            // turn reported it** — ours or one the platform injected — and therefore before
            // the ownership question below. Learn it for the NEXT turn's watermark, by NAME
            // (finding §10).
            {
                let mut st = state.lock().unwrap();
                if let (Some(model), Some(usage)) = (st.session_model.clone(), msg.get("modelUsage")) {
                    if let Some(w) = usage.get(&model).and_then(|m| m.get("contextWindow")).and_then(|v| v.as_u64()) {
                        st.context_window = Some(w);
                    }
                }
            }
            // ---- WHOSE TURN DOES THIS END? (see [`PendingTurn`]) -----------------------
            //
            // The one question this branch used not to ask. A `result` the child reports
            // with a user send still queued — ours, the only one there can be — ends a turn
            // that was already in flight when we sent, so it is RETAINED and the parked
            // prompt keeps waiting for its own.
            let (ready, nothing_awaited) = {
                let mut guard = current.lock().unwrap();
                let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut *guard).into_iter()
                    .partition(|pending| !pending.result_is_another_turns(&msg));
                *guard = waiting;
                (ready, guard.is_empty())
            };
            let api_error = state.lock().unwrap().api_error.take();
            let carried_by_api_error = msg.get("terminal_reason").and_then(Value::as_str) == Some(TERMINAL_REASON_API_ERROR);
            // Between turns only with no input still awaited; written before the reply is
            // delivered, so whoever sees the reply sees the signal. A turn an API failure ended
            // may carry on once the connection is back, so it signals from `after_api_error`
            // only if it really ends there.
            if nothing_awaited && !carried_by_api_error {
                Self::signal_turn_end(state);
            }
            // **AN API FAILURE MAY BE THE LOST CONNECTION** (CEO, 2026-10-21): whether it is,
            // and the wait for the connection, run off this thread, which keeps reading. Every
            // turn of every lease ends here, Rich's, Stu's and Claude Code's own, so they all
            // carry on the same way. Until it is decided the inputs stay parked, waiting.
            if msg.get("terminal_reason").and_then(Value::as_str) == Some(TERMINAL_REASON_API_ERROR) {
                let held: Vec<String> = ready.iter().map(|pending| pending.command_uuid.clone()).collect();
                current.lock().unwrap().extend(ready.into_iter().map(|mut pending| { pending.awaiting_connection = true; pending }));
                let text = api_error.as_ref().and_then(|(_, text)| text.clone());
                let (frame, ids) = (msg.clone(), held.clone());
                let state_for_signal = state.clone();
                let (stdin, waiting, lane, state, closed) =
                    (stdin.clone(), current.clone(), between.clone(), state.clone(), closed.clone());
                let decided = std::thread::Builder::new().name("richos-connection-wait".into()).spawn(move ||
                    Self::after_api_error(frame, api_error, ids, &stdin, &waiting, &lane, &state, &closed));
                if let Err(error) = decided {
                    // No thread, no wait: the turn ends as it always did.
                    eprintln!("[richos] the wait for the connection could not start: {error}");
                    if nothing_awaited { Self::signal_turn_end(&state_for_signal); }
                    Self::end_turn(Self::take_held(current, &held), msg, text, between);
                }
                return;
            }
            Self::end_turn(ready, msg, api_error.and_then(|(_, text)| text), between);
            return;
        }

        // ---- the commands this lease has running in the background ------------------------
        //
        // Read here, on the reader thread, whatever turn (or none) the frame arrives in: the
        // ending of a background command almost always arrives when NO prompt is parked, so a
        // reading taken off a turn's stream would never see it. Routed on afterwards like any
        // other traffic. See [`crate::cognition::BackgroundCommand`] for the measured shapes.
        if ty == "system" {
            let subtype = msg.get("subtype").and_then(Value::as_str).unwrap_or("");
            if matches!(subtype, "task_started" | "background_tasks_changed" | "task_notification") {
                let ours_running = current
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|pending| !pending.not_yet_ours());
                state.lock().unwrap().note_background(&msg, ours_running);
            }
        }

        // ---- session identity ----------------------------------------------------------
        if ty == "system" && msg.get("subtype").and_then(|v| v.as_str()) == Some("init") {
            let mut st = state.lock().unwrap();
            if let Some(m) = msg.get("model").and_then(|v| v.as_str()) {
                st.session_model = Some(m.to_string());
            }
            // THE SECOND LOUDNESS LAYER FOR `--plugin-dir`. Read once per frame — the init
            // frame repeats every turn and says the same thing, so a later rejection is caught
            // too. The verdict is a FACT FROM THE CHILD, not an inference: `plugins` lists what
            // it actually loaded.
            if st.onboarding_tools_verdict == crate::onboarding_tools::OnboardingToolsVerdict::NotYetReported {
                st.onboarding_tools_verdict = crate::onboarding_tools::verdict_from_init(&msg);
            }
            st.continuity_tools_loaded = ["mcp__richos_continuity__checkpoint", "mcp__richos_continuity__inspect"].iter()
                .all(|name| msg["tools"].as_array().map(|tools| tools.iter().any(|tool| tool.as_str() == Some(name))).unwrap_or(false));
            // **The three tri-state facts are READINGS, and a reading only happens here.**
            // `InitFact::reported` has no way to produce `NotYetReported`, so the three
            // fields can only leave that state inside this handler — which is what makes
            // "nobody has told us" a statement about the wire rather than about the order
            // the app happened to call things in.
            st.work_tools_loaded = InitFact::reported(
                ["mcp__richos_work__repositories", "mcp__richos_work__prepare", "mcp__richos_work__inspect", "mcp__richos_work__integrate", "mcp__richos_work__complete"].iter()
                    .all(|name| msg["tools"].as_array().is_some_and(|tools| tools.iter().any(|tool| tool.as_str() == Some(name)))));
            st.assignment_tool_loaded = crate::assignment_tools::loaded_from_init(&msg);
            st.status_tool_loaded = crate::status_tools::loaded_from_init(&msg);
            st.engine_plugin_loaded = InitFact::reported(msg["plugins"].as_array().is_some_and(|plugins|
                plugins.iter().any(|plugin| plugin["name"] == crate::engine_profile::PLUGIN_NAME)));
            // **THE LOUDNESS LAYER FOR [`TOOL_SEARCH_ENV`].** Read every init frame, like the
            // `--plugin-dir` verdict above: the variable is undocumented, so the only honest
            // evidence that it is still doing its job is the child NOT offering its own
            // discovery tool. Announced once, at the transition, on stderr — machinery, never
            // anything the CEO sees (the stderr drain above says why).
            let tool_search = tool_search_from_init(&msg);
            if tool_search == InitFact::Yes && st.tool_search_offered != InitFact::Yes {
                // **It names the lease it is actually on.** See `ReaderState::tool_residency`:
                // this message used to be one sentence for both leases, and the work lease —
                // which is given no variable at all — set it off on candidate .10.
                eprintln!("{}", tool_search_notice(st.tool_residency));
            }
            st.tool_search_offered = tool_search;
            let before = st.skills_verdict;
            st.skills_verdict = crate::skills::verdict_from_init(&msg);
            if st.skills_verdict == crate::skills::SkillsVerdict::Rejected
                && before != crate::skills::SkillsVerdict::Rejected
            {
                // Diagnostics are machinery and NEVER reach the CEO (see the stderr drain
                // above); this is for whoever is looking at why Rich is thinner than he should
                // be. `preflight` already proved the files are on disk, so reaching here means
                // the BINARY declined them, which is the one case no file check can see.
                eprintln!(
                    "[richos] THE SKILLS DID NOT LOAD. `{PLUGIN_DIR}` was accepted and \
                     `{}` is not in this session's plugin list, so every skill RichOS ships is \
                     absent from this lease. The files passed preflight, so this is the binary \
                     declining them — check `claude --version` against the last release gate.",
                    crate::skills::PLUGIN_NAME
                );
            }
        }

        // ---- the clean-output text path, and its guard ----------------------------------
        if ty == "stream_event" {
            let ev = msg.get("event").unwrap_or(&Value::Null);
            let ev_ty = ev.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if ev_ty == "message_start" {
                text_deltas.store(0, Ordering::SeqCst);
            }
            if ev_ty == "content_block_delta"
                && ev.get("delta").and_then(|d| d.get("type")).and_then(|v| v.as_str()) == Some("text_delta")
            {
                if let Some(t) = ev.get("delta").and_then(|d| d.get("text")).and_then(|v| v.as_str()) {
                    text_deltas.fetch_add(1, Ordering::SeqCst);
                    Self::route(ChunkMsg::Text(t.to_string()), current, between, &msg);
                    return;
                }
            }
            // The DERIVED watermark measurement, emitted only when BOTH halves are in hand
            // (caveat C3). The raw frame is routed too, immediately after, so nothing about
            // the derivation costs us the bytes it was derived from.
            if ev_ty == "message_delta" {
                if let (Some(usage), Some(size)) = (ev.get("usage"), state.lock().unwrap().context_window) {
                    if let Some(used) = tokens_in_context(usage) {
                        Self::route(
                            ChunkMsg::Usage { used, size, usage: usage.clone() },
                            current,
                            between,
                            &msg,
                        );
                    }
                }
            }
        }


        // ---- THE ANTI-SILENT-DEGRADE GUARD ----------------------------------------------
        //
        // If `--include-partial-messages` ever stops working, no `text_delta` arrives and
        // the CEO would watch a turn produce nothing and then end — a silent degrade, which
        // §16 forbids. So a whole-message `assistant` frame whose text was NEVER streamed is
        // delivered as text here. Costs one integer compare per frame on the healthy path,
        // where the count is always non-zero and this never fires.
        // The register's id off the COMPLETE message, for the same reason the anti-degrade
        // guard below exists: `--include-partial-messages` is a flag, and the app must not stop
        // speaking because it stopped working. Whichever frame arrives first wins; both name the
        // same id.

        // Claude Code's own API failure line is held until the turn's `result` says whether the
        // turn carries on (`ReaderState::api_error`); it is delivered then only if it does not.
        let api_error = ty == "assistant" && msg.get("is_api_error_message").and_then(Value::as_bool) == Some(true);
        if api_error {
            state.lock().unwrap().api_error = Some((msg.get("error").and_then(Value::as_str).map(str::to_string), None));
        }
        if ty == "assistant" && text_deltas.load(Ordering::SeqCst) == 0 {
            let whole: String = msg
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter(|b| b.get("type").and_then(|v| v.as_str()) == Some("text"))
                        .filter_map(|b| b.get("text").and_then(|v| v.as_str()))
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            if api_error {
                if let Some((_, held)) = state.lock().unwrap().api_error.as_mut() { *held = Some(whole).filter(|t| !t.is_empty()); }
            } else if !whole.is_empty() {
                Self::route(ChunkMsg::Text(whole), current, between, &msg);
            }
        }

        // A worker's start reaches the between-turn sink too when a turn took it: it names the
        // task Stu's host tells him with the worker's overload (`work_host.rs` observe_worker).
        if Self::route(ChunkMsg::Frame(msg.clone()), current, between, &msg)
            && ty == "system" && msg.get("subtype").and_then(Value::as_str) == Some("task_started") {
            between.lock().unwrap().copy_to_sink(msg);
        }
    }

    /// ROUTED WHEN THERE IS A TURN, PARKED WHEN THERE IS NOT (§1.5, gap #1).
    ///
    /// A FAILED SEND FALLS THROUGH TO THE SAME PLACE, on purpose: a closed receiver means
    /// the drain loop has already returned, which is the same fact as "no turn is in flight"
    /// arriving one instant later.
    fn route(
        chunk: ChunkMsg,
        current: &Arc<Mutex<Vec<PendingTurn>>>,
        between: &Arc<Mutex<BetweenTurn>>,
        frame: &Value,
    ) -> bool {
        let mut routed = false;
        let mut inputs=current.lock().unwrap();
        if matches!(chunk,ChunkMsg::Text(_)) {
            // Native folds share one response. Its text belongs to the latest admitted
            // input; earlier inputs still receive their own terminal receipt.
            if let Some(input)=inputs.iter_mut().rev().find(|input|!input.not_yet_ours()) {
                routed=input.send(chunk,frame);
            }
        } else {
            for input in inputs.iter_mut().filter(|input|!input.not_yet_ours()) {
                routed|=input.send(chunk.clone(),frame);
            }
        }
        drop(inputs);
        if !routed {
            between.lock().unwrap().offer_frame(frame.clone());
        }
        routed
    }

    /// End a turn the way every turn ends: its inputs get the `result`, after Claude Code's API
    /// failure line when one was held for it (`ReaderState::api_error`), delivered where
    /// [`NativeClient::route`] delivers text, to the latest input. A turn no input waits for is
    /// kept on the between-turn lane.
    fn end_turn(mut ready: Vec<PendingTurn>, result: Value, text: Option<String>, between: &Arc<Mutex<BetweenTurn>>) {
        if ready.is_empty() {
            between.lock().unwrap().offer_frame(result);
            return;
        }
        // **NOT HIS WORDS WHEN THE TURN FAILS ON IT.** A `result` that is an error ends the turn
        // as a failure, whose card says what happened in the app's own words and whose reason
        // keeps the vendor's line for Technical view (`native_result`). Delivered as text too, a
        // new user's first message showed "Rich: Not logged in · Please run /login" above the
        // sign-in card, and the card counted it as "33 characters of the answer" (guest walk of
        // 1.2.1, 2026-10-21; `docs/development/failure-classes/machine-words-as-richs-words.md`).
        let failed = result.get("is_error").and_then(Value::as_bool) == Some(true);
        if let (Some(text), Some(input), false) = (text, ready.last_mut(), failed) {
            input.send(ChunkMsg::Text(text), &Value::Null);
        }
        finish_native_turn(ready, result);
    }

    /// Tell Stu's stall-watch the turn is over (a no-op on a lease without a monitor).
    fn signal_turn_end(state: &Arc<Mutex<ReaderState>>) {
        let lead = state.lock().unwrap().lead.clone();
        if let Some(lead) = lead { lead.ended(); }
    }

    /// The inputs named in `held` that still wait for the connection, taken off the parked list.
    fn take_held(current: &Arc<Mutex<Vec<PendingTurn>>>, held: &[String]) -> Vec<PendingTurn> {
        let mut guard = current.lock().unwrap();
        let (taken, kept) = std::mem::take(&mut *guard).into_iter()
            .partition(|pending| pending.awaiting_connection && held.contains(&pending.command_uuid));
        *guard = kept;
        taken
    }

    /// **A turn an API failure ended: when the lost connection ended it, wait until the
    /// connection is back and carry the same turn on; otherwise end it as it always ended**
    /// (CEO, 2026-10-21). Runs on its own thread, so the reader keeps reading, for the inputs
    /// `held` that the turn's `result` answered, or for none when the turn was one Claude Code
    /// started itself (a helper's hand-back, a task notification).
    ///
    /// Carrying on sends ONE `connection_restored` message in the same session, never what the
    /// user sent again: the session already holds the input the failure interrupted. What the
    /// message means to Rich and Stu is in their instruction files (`engine/app/agents`). The
    /// held inputs wait on for that message's turn, so whoever waited for the first turn gets
    /// the one `result` of the turn that finished. A Stop (which takes a waiting prompt away)
    /// and the child ending (which ends every input) end the wait without sending anything.
    #[allow(clippy::too_many_arguments)] // the reader's own shared handles, as `dispatch` takes them
    fn after_api_error(result: Value, api_error: Option<(Option<String>, Option<String>)>, held: Vec<String>,
        stdin: &Arc<Mutex<ChildStdin>>, current: &Arc<Mutex<Vec<PendingTurn>>>, between: &Arc<Mutex<BetweenTurn>>,
        state: &Arc<Mutex<ReaderState>>, closed: &Arc<AtomicBool>) {
        let (kind, text) = api_error.unwrap_or_default();
        let watch = state.lock().unwrap().connection.clone();
        if crate::connection::lost(&result, kind.as_deref(), &watch) {
            let said = result.get("result").and_then(Value::as_str).unwrap_or_default().to_string();
            eprintln!("[richos] the internet connection is lost ({said}); the turn carries on when it is back");
            let still_held = || current.lock().unwrap().iter()
                .any(|pending| pending.awaiting_connection && held.contains(&pending.command_uuid));
            let stopped = || closed.load(Ordering::SeqCst) || (!held.is_empty() && !still_held());
            if watch.wait_until_up(&stopped) {
                match Self::carry_on(&said, &held, stdin, current, closed) {
                    Ok(()) => eprintln!("[richos] the connection is back; the same turn carries on"),
                    // The pipe is gone only with the child, whose end ends every input.
                    Err(error) => eprintln!("[richos] the turn could not carry on: {error}"),
                }
            }
            return;
        }
        let ready = Self::take_held(current, &held);
        if ready.is_empty() && !held.is_empty() { return; }
        if current.lock().unwrap().is_empty() { Self::signal_turn_end(state); }
        Self::end_turn(ready, result, text, between);
        Self::release_between_turn(between);
    }

    /// Send the `connection_restored` message under a new uuid and point the held inputs at it.
    /// Nothing is sent when they were stopped meanwhile or the child is gone.
    fn carry_on(said: &str, held: &[String], stdin: &Arc<Mutex<ChildStdin>>,
        current: &Arc<Mutex<Vec<PendingTurn>>>, closed: &Arc<AtomicBool>) -> Result<(), NativeError> {
        let id = uuid::Uuid::new_v4().to_string();
        {
            let mut inputs = current.lock().unwrap();
            if closed.load(Ordering::SeqCst) { return Ok(()); }
            let mut waiting = inputs.iter_mut()
                .filter(|pending| pending.awaiting_connection && held.contains(&pending.command_uuid)).peekable();
            if !held.is_empty() && waiting.peek().is_none() { return Ok(()); }
            for pending in waiting {
                pending.awaiting_connection = false;
                pending.continued_as = Some(id.clone());
                pending.phase = TurnPhase::Unconfirmed;
                // The failure this carries on from is over: an overload or a limit read off its
                // frames must not end the continued turn (review rv-20261021T031711Z-3416839f-d2f2).
                // The continuation's own frames set them again if it meets one.
                if let TurnSink::Input { limit, overload, .. } = &mut pending.sink {
                    *limit = None;
                    *overload = None;
                }
            }
        }
        Self::tell_connection_restored(stdin, &id, json!({"event":"connection_restored","error":said}))
    }

    /// Write one `connection_restored` fact into this session as a message from `connection`, in
    /// the shape every app message to Rich and Stu has (`session_tools::Message`).
    fn tell_connection_restored(stdin: &Arc<Mutex<ChildStdin>>, id: &str, fact: Value) -> Result<(), NativeError> {
        let message = crate::session_tools::Message { handoff: false, from: "connection".into(),
            text: fact.to_string(), instruction: None, work: None };
        let text = serde_json::to_string(&message).map_err(|error| NativeError::Protocol(error.to_string()))?;
        Self::write_line(stdin, &json!({"type":"user","uuid":id,"message":{"role":"user","content":[{"type":"text","text":text}]}}))
    }

    /// **A helper (a teammate of Rich's or Stu's) that the lost connection ended** (CEO,
    /// 2026-10-21, "every worker the app runs"). Its run is Claude Code's, inside this lease, so
    /// the app cannot send it anything itself; its parent can, and a message to a finished helper
    /// continues that helper's own conversation from its transcript (Claude Code 2.1.305's
    /// SendMessage: "a send resumes it from its transcript"). So once the connection is back the
    /// parent gets one `connection_restored` message naming the helper (its call, agent id and
    /// task), and its instruction file says to continue that helper, not start it over.
    ///
    /// The helper's failure is decided by the same rule as a turn's ([`crate::connection::lost`]),
    /// from its message: the `error` kind, the `api_error_status` or the status its line names.
    /// An overload is not the connection and keeps the overload schedule (`work_host.rs`).
    fn after_helper_api_error(frame: &Value, stdin: &Arc<Mutex<ChildStdin>>, state: &Arc<Mutex<ReaderState>>,
        closed: &Arc<AtomicBool>) {
        let Some(call) = frame.get("parent_tool_use_id").and_then(Value::as_str).map(str::to_string) else { return };
        if !state.lock().unwrap().helpers_waiting.insert(call.clone()) { return; }
        let said: Vec<&str> = frame.pointer("/message/content").and_then(Value::as_array).into_iter().flatten()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str)).collect();
        let said = said.join("\n");
        let kind = frame.get("error").and_then(Value::as_str).map(str::to_string);
        let ended = json!({"api_error_status": frame.get("api_error_status"), "result": said});
        let fact = json!({"event":"connection_restored","error":said,
            "helper":{"tool_use_id":call,"agent_id":frame.get("agent_id"),"task":frame.get("task_description")}});
        let (stdin, state, closed) = (stdin.clone(), state.clone(), closed.clone());
        let waited = std::thread::Builder::new().name("richos-helper-connection-wait".into()).spawn(move || {
            let watch = state.lock().unwrap().connection.clone();
            if crate::connection::lost(&ended, kind.as_deref(), &watch) {
                eprintln!("[richos] a helper's run ended on the lost connection ({said}); its parent is told when it is back");
                if watch.wait_until_up(&|| closed.load(Ordering::SeqCst)) {
                    match Self::tell_connection_restored(&stdin, &uuid::Uuid::new_v4().to_string(), fact) {
                        Ok(()) => eprintln!("[richos] the connection is back; the helper's parent is told to continue it"),
                        Err(error) => eprintln!("[richos] the helper's parent could not be told: {error}"),
                    }
                }
            }
            state.lock().unwrap().helpers_waiting.remove(&call);
        });
        if let Err(error) = waited {
            eprintln!("[richos] the wait for a helper's connection could not start: {error}");
        }
    }

    /// Answer the agent's `control_request`s, AND route them as machinery.
    ///
    /// Business questions use the native control transport. This function is
    /// transport: it answers FIRST (the child is blocked on it, and a machinery record is
    /// never worth a millisecond of the CEO's turn latency) and routes the record after.
    fn handle_agent_request(
        msg: &Value,
        stdin: &Arc<Mutex<ChildStdin>>,
        current: &Arc<Mutex<Vec<PendingTurn>>>,
        between: &Arc<Mutex<BetweenTurn>>,
            question_scope: Option<&Path>,
    ) {
        let request_id = msg.get("request_id").cloned().unwrap_or(Value::Null);
        let request = msg.get("request").cloned().unwrap_or(Value::Null);
        let subtype = request.get("subtype").and_then(|v| v.as_str()).unwrap_or("");

        // Business questions still use the native control transport. Tool approvals do not.
        let question = request["tool_name"].as_str().is_some_and(|name|
            name == "AskUserQuestion" || name.starts_with("mcp__richos_questions__"));
        let (response, machinery) = if subtype == "can_use_tool" && question {
            let body = if request["tool_name"] == "AskUserQuestion" {
                let result = question_scope.ok_or_else(|| "This turn has no question scope.".to_string())
                    .and_then(|path| crate::question_tools::convert_vendor(path, &request["input"]));
                let message = match result {
                    Ok(value) => value["instruction"].as_str().unwrap_or("The question is recorded.").to_string(),
                    Err(error) => error,
                };
                json!({"behavior":"deny", "message":message})
            } else {
                json!({"behavior":"allow", "updatedInput":request["input"]})
            };
            let chosen = body["behavior"].as_str().unwrap().to_string();
            (
                json!({ "type": "control_response",
                        "response": { "subtype": "success", "request_id": request_id, "response": body } }),
                Some(BetweenItem::Permission { request, chosen }),
            )
        } else {
            // A control subtype RichOS does not implement. It is answered with a structured
            // ERROR rather than an empty success, because a success that did nothing is the
            // silent degrade §16 forbids, one level down — the agent would proceed believing
            // a hook ran or an MCP message was delivered.
            //
            // **The error SHAPE here is unverified.** `run10` captured the binary's own
            // structured error going the other way ("Unsupported control request subtype:
            // …"); nothing in the captures shows a client-to-agent error, so this mirrors it
            // by symmetry rather than by observation.
            (
                json!({ "type": "control_response",
                        "response": { "subtype": "error", "request_id": request_id,
                                      "error": format!("richos does not implement control request subtype: {subtype}") } }),
                Some(BetweenItem::Frame(msg.clone())),
            )
        };

        let _ = Self::write_line(stdin, &response);

        if let Some(m) = machinery {
            let mut routed = false;
            for pending in current.lock().unwrap().iter_mut().filter(|pending| !pending.not_yet_ours()) {
                routed |= pending.send(m.to_chunk(), msg);
            }
            if !routed {
                between.lock().unwrap().push(m);
            }
        }
    }

    fn write_line(stdin: &Arc<Mutex<ChildStdin>>, msg: &Value) -> Result<(), NativeError> {
        let mut line = serde_json::to_string(msg)?;
        line.push('\n');
        let mut guard = stdin.lock().unwrap();
        guard.write_all(line.as_bytes())?;
        guard.flush()?;
        Ok(())
    }

    fn reload_plugins(&self)->Result<Value,NativeError> {
        let id=format!("richos_reload_plugins_{}",self.next_id.fetch_add(1,Ordering::SeqCst));
        let (tx,rx)=channel();
        self.pending.lock().unwrap().insert(id.clone(),tx);
        let request=json!({"type":"control_request","request_id":id,"request":{"subtype":"reload_plugins"}});
        let result=(|| {
            Self::write_line(&self.stdin,&request)?;
            let reply=rx.recv_timeout(HANDSHAKE_TIMEOUT).map_err(|e|NativeError::Protocol(format!("Plugin reload was not confirmed: {e}")))?;
            if reply["response"]["subtype"]!="success" {
                return Err(NativeError::Protocol(format!("Plugin reload refused: {}",reply["response"]["error"])));
            }
            Ok(reply["response"]["response"].clone())
        })();
        self.pending.lock().unwrap().remove(&id);
        result
    }

    /// The session id — OURS, minted at spawn and passed to the child as `--session-id`.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Did the binary actually load RichOS's skills? Read off `system/init.plugins`.
    ///
    /// [`skills::SkillsVerdict::NotYetReported`] until the first turn, because that is when the
    /// init frame arrives. A caller that treats `NotYetReported` as a failure would report a
    /// fresh lease as broken; a caller that treats it as success would report an unknown as a
    /// fact. It is three states for that reason.
    pub fn skills_verdict(&self) -> crate::skills::SkillsVerdict {
        self.reader_state.lock().map(|s| s.skills_verdict).unwrap_or(crate::skills::SkillsVerdict::NotYetReported)
    }

    /// Did this session defer its tools? See [`tool_search_from_init`].
    ///
    /// [`InitFact::NotYetReported`] until the first turn, for the reason `skills_verdict`
    /// gives above: the init frame lands with the turn, not with the handshake.
    pub fn tool_search_offered(&self) -> InitFact {
        self.reader_state.lock().map(|s| s.tool_search_offered).unwrap_or(InitFact::NotYetReported)
    }

    pub fn onboarding_tools_verdict(&self) -> crate::onboarding_tools::OnboardingToolsVerdict {
        self.reader_state.lock().map(|s| s.onboarding_tools_verdict)
            .unwrap_or(crate::onboarding_tools::OnboardingToolsVerdict::NotYetReported)
    }



    /// Take everything the agent said while no turn was in flight (§1.5, gap #1).
    ///
    /// **`turn_id` stays `None` and the caller stamps the thread.** These records attach to
    /// the THREAD, not to a turn, because there is no turn they belong to — and inventing one
    /// (the previous turn, the next turn) would be a false attribution, which is the one
    /// thing a record of what happened must not do. §1.4 G4: `turn_id: None` is a first-class
    /// state.
    ///
    /// Takes `&self` so a caller holding the lease immutably can pump the lane; the buffer is
    /// behind its own `Mutex` and is never held across a turn.
    /// The commands this session's provider has put in the background and how each ended
    /// ([`crate::cognition::BackgroundCommand`]). A reading, never held across a turn.
    pub fn background_commands(&self) -> Vec<crate::cognition::BackgroundCommand> {
        self.reader_state.lock().map(|s| s.background.clone()).unwrap_or_default()
    }

    pub fn set_between_turn_sink(&self, sink: crate::cognition::BetweenTurnSink) {
        self.between.lock().unwrap().sink = Some(sink);
        Self::release_between_turn(&self.between);
    }

    fn release_between_turn(between: &Arc<Mutex<BetweenTurn>>) {
        let (sink, records) = {
            let mut lane = between.lock().unwrap();
            let Some(sink) = lane.sink.clone() else { return };
            let frames: Vec<Value> = std::mem::take(&mut lane.queue).into_iter().map(|item| match item {
                BetweenItem::Frame(frame) => frame,
                BetweenItem::Permission {request, chosen} => json!({"type":"control_request","request":request,"chosen":chosen}),
            }).collect();
            (sink, frames)
        };
        if !records.is_empty() { sink(records); }
    }

    pub fn drain_between_turn(&self, session_id: &str) -> Vec<MachineryRecord> {
        self.between.lock().unwrap().drain(session_id)
    }

    /// How many identical SessionMeta repeats the §1.5 slot has suppressed on this client.
    ///
    /// Exposed for the test that proves *"last value wins"* is doing work — the absence of
    /// rows is not evidence of suppression, since it is equally consistent with the agent
    /// never having repeated itself.
    pub fn suppressed_between_turn_repeats(&self) -> u64 {
        self.between.lock().unwrap().suppressed()
    }

    /// A handle that can cancel the CURRENTLY IN-FLIGHT turn from another thread.
    ///
    /// Takes `&self` and clones only `Arc`s, which is the whole requirement: the stop control
    /// never holds the `Mutex<Spine>` the running turn is holding, so it can never queue
    /// behind it.
    pub fn cancel_handle(&self) -> Arc<NativeCancelHandle> {
        Arc::new(NativeCancelHandle {
            stdin: Arc::clone(&self.stdin),
            current_prompt: Arc::clone(&self.current_prompt),
            operation_cancel: Arc::clone(&self.operation_cancel),
            action_grants: self.action_grants.clone(),
            process_fence: self.child.fence(),
            settle_workers_on_stop: self.settle_workers_on_stop,
            next_id: self.next_id.clone(),
            pending: Arc::clone(&self.pending),
            reader_state: Arc::clone(&self.reader_state),
            reader_closed: Arc::clone(&self.reader_closed),
        })
    }

    /// Run ONE turn, streaming text AND machinery to `on_item` in arrival order.
    ///
    /// **This loop is where `seq` is assigned (§1.4 G1).** One counter, shared by text and
    /// machinery, so *"he said X, then ran Y, then said Z"* is reconstructible — you cannot
    /// rebuild that from two independent counters. `dispatch` runs on the reader thread, but
    /// every routed item passes through this ONE mpsc channel, drained in order right here,
    /// so the assignment is single-threaded and sound without a lock.
    ///
    /// `seq` is strictly increasing but NOT contiguous within one family: a text-only
    /// consumer sees gaps where machinery happened. That is the point of a shared counter,
    /// and `app/STREAMING.md` says so.
    pub fn prompt(&self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, NativeError> {
        self.prompt_with_context(text, None, on_item)
    }

    /// Host context uses the native hook so the user's instruction stays byte-for-byte intact.
    fn prompt_with_context(&self, text: &str, context: Option<&str>,
        on_item: &mut dyn FnMut(TurnItem)) -> Result<String, NativeError> {
        self.enqueue_prompt(text, context)?.wait(on_item)
    }

    /// Write ordinary SDK input and release this session before waiting for its reply.
    pub(crate) fn enqueue_prompt(&self, text: &str, context: Option<&str>) -> Result<NativeResponse, NativeError> {
        let prepare=context.map(|context| {
            let context=context.to_string();
            Box::new(move || Ok(Some(context))) as NativePreparation
        });
        self.enqueue_prepared(text,prepare,None)
    }

    fn enqueue_prepared(&self, text: &str, prepare: Option<NativePreparation>, complete: Option<NativeCompletion>) -> Result<NativeResponse, NativeError> {
        let (tx, rx): (Sender<ChunkMsg>, Receiver<ChunkMsg>) = channel();
        let command_uuid = uuid::Uuid::new_v4().to_string();
        let response = NativeResponse { rx, command_uuid: command_uuid.clone(),
            current_prompt: self.current_prompt.clone(), session_id: self.session_id.clone() };
        let content = vec![json!({"type":"text","text":text})];
        let msg = json!({
            "type": "user",
            "uuid": command_uuid,
            "message": { "role": "user", "content": content }
        });
        {
            // Serialize Stop with the send boundary: a cancellation between the
            // internal prime and the user prompt must prevent that next send.
            let operation = self.operation_cancel.lock().unwrap();
            // Managed inputs check their own Stop at native admission. A Stop on the
            // preceding input cannot refuse a newly submitted request.
            if prepare.is_none() && operation.active && operation.requested {
                let _ = tx.send(ChunkMsg::Done(json!({"type":"result","stop_reason":STOP_REASON_CANCELLED})));
                return Ok(response);
            }
            {
                let mut current = self.current_prompt.lock().unwrap();
                if self.reader_closed.load(Ordering::SeqCst) { return Err(NativeError::Closed); }
                // Parked with the id it was sent under, and `Unconfirmed` until the child
                // says otherwise — never assuming the next `result` is this turn's. Whether
                // the child has named our turns before is read here, with the park (finding
                // 47; see `PendingTurn::child_names_turns`).
                let child_names_turns = self.reader_state.lock().unwrap().turns_named_by_the_child;
                current.push(PendingTurn {
                    sink: TurnSink::Prompt(tx),
                    command_uuid: command_uuid.clone(),
                    prompt_text: text.to_string(), hook_request_id: None,
                    prepare,
                    complete,
                    preparation_error: None,
                    phase: TurnPhase::Unconfirmed,
                    child_names_turns,
                    awaiting_connection: false,
                    continued_as: None,
                });
                let lead=self.reader_state.lock().unwrap().lead.clone();
                if let Some(lead)=lead {lead.began();}
            }
            if let Err(error) = Self::write_line(&self.stdin, &msg) {
                self.current_prompt.lock().unwrap().retain(|pending| pending.command_uuid != command_uuid);
                return Err(error);
            }
        }

        eprintln!("[richos] {} reply timing: visible input sent to the model", crate::util::wall_clock_utc());
        Ok(response)
    }
}

/// The existing native result stream for one submitted input. It owns no session lease.
pub(crate) struct NativeResponse {
    rx: Receiver<ChunkMsg>,
    command_uuid: String,
    current_prompt: Arc<Mutex<Vec<PendingTurn>>>,
    session_id: String,
}

impl NativeResponse {
    pub(crate) fn wait(self, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, NativeError> {
        let Self { rx, command_uuid, current_prompt, session_id } = self;
        // THE shared per-turn counter (§1.4 G1). Advanced only when an item is actually
        // delivered — a frame that normalizes to nothing consumes no position.
        let mut seq: u64 = 0;
        // **REPLY TIMING, on the voice log's clock** (CEO 2026-10-14: "From barge in to a
        // 10-second delay now?"). Two moments only the client sees: the prompt leaving for the
        // model, and the model's first word coming back. With the voice session's own lines
        // (`TURN to Rich`, `ANSWER AUDIBLE`, `playout chunk START`) they split his wait.
        let mut first_text_logged = false;
        // Set the moment a `Cancel` wakes this loop. From then on the loop keeps DELIVERING
        // whatever still arrives — §9.3 step 4, "preserve partial commentary, activity and
        // assistant output" — but stops waiting forever for a `result` that a non-compliant
        // agent may never send.
        let mut cancel_deadline: Option<std::time::Instant> = None;
        // **AMERICAN SPELLING, FIXED HERE AND NOWHERE ELSE** (CEO §93; plan check 1(a), C9).
        // This is the one place every piece of reply text passes — streamed deltas, the app-said
        // receipt and the degraded whole message alike — and the one place `seq` is assigned,
        // so a held-back word goes out under the counter that already exists. Everything
        // downstream of the ledger inherits the fixed text: the Mac's screen, speech, crash
        // recovery, the phones and both kinds of push preview. His own words never come
        // through here (`record_prompt_received*`); `american_spelling.rs` has the rules.
        //
        // **The held word is released before anything else is delivered and on every way out
        // of this loop** — the next tool row, a permission, a usage reading, `content_block_stop`,
        // `Done`, the cancel timeout (before the sink is detached), the child vanishing, and a
        // provider result, so partial output is never lost at the boundary.
        let mut spelling = crate::american_spelling::Speller::new();
        fn say(text:&str,seq:&mut u64,on_item:&mut dyn FnMut(TurnItem),spelling:&mut crate::american_spelling::Speller,first_logged:&mut bool) {
            if text.is_empty() {return}
            if !*first_logged {
                *first_logged=true;
                eprintln!("[richos] {} reply timing: first text from the model",crate::util::wall_clock_utc());
            }
            let decided=spelling.push(text);
            if !decided.is_empty() {on_item(TurnItem::Text {seq:*seq,text:&decided});*seq+=1;}
        }

        // **A FRAME ABOUT ANOTHER INPUT DOES NOT CUT THIS ANSWER IN HALF** (nightly 66: "Early
        // in [gap] a relationship"). A message sent while Rich is writing makes the child say
        // `command_lifecycle: queued` for it (and, when folded, its input hook) in the middle
        // of the text. Taking a position there split one sentence into two text runs, which
        // the Mac draws as two answers with a blank gap between them. Such a frame is held
        // while text is streaming and delivered, in arrival order, before the next item that
        // is not text (or on the way out), so it is still recorded and the sentence is whole.
        let mut aside: Vec<Value> = Vec::new();
        let mut text_open = false;
        let about_another_input = |frame: &Value| match frame.get("type").and_then(Value::as_str) {
            Some("command_lifecycle") => frame.get("command_uuid").and_then(Value::as_str) != Some(command_uuid.as_str()),
            Some("control_request") => frame.pointer("/request/callback_id").and_then(Value::as_str) == Some("richos-input-context"),
            _ => false,
        };

        fn release(seq: &mut u64, on_item: &mut dyn FnMut(TurnItem), spelling: &mut crate::american_spelling::Speller,
            aside: &mut Vec<Value>, session_id: &str) {
            let held = spelling.finish();
            if !held.is_empty() {
                on_item(TurnItem::Text { seq: *seq, text: &held });
                *seq += 1;
            }
            for frame in aside.drain(..) {
                for record in MachineryRecord::from_native_event(&frame, session_id, *seq) {
                    *seq += 1;
                    on_item(TurnItem::Machinery(record));
                }
            }
        }
        // A usage-limit refusal seen on this turn (`usage_limit_signal`), with its reset time.
        let mut limit: Option<Option<u64>> = None;
        let limit_error = |resets_at: Option<u64>, detail: String| {
            NativeError::Protocol(crate::claude_accounts::usage_limit_error(resets_at, &detail))
        };
        loop {
            let received = match cancel_deadline {
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                Some(deadline) => match deadline.checked_duration_since(std::time::Instant::now()) {
                    Some(remaining) => rx.recv_timeout(remaining),
                    None => Err(RecvTimeoutError::Timeout),
                },
            };
            match received {
                Ok(ChunkMsg::Text(t)) => {
                    say(&t,&mut seq,on_item,&mut spelling,&mut first_text_logged);
                    text_open = true;
                }
                Ok(ChunkMsg::Frame(frame)) => {
                    if text_open && about_another_input(&frame) {aside.push(frame); continue;}
                    if !is_ping(&frame) {release(&mut seq,on_item,&mut spelling,&mut aside,&session_id); text_open = false;}
                    if frame.get("parent_tool_use_id").is_none_or(Value::is_null) {
                        if let Some(resets_at) = usage_limit_signal(&frame) { limit = Some(resets_at); }
                    }
                    for record in MachineryRecord::from_native_event(&frame,&session_id,seq) {
                        seq += 1;
                        on_item(TurnItem::Machinery(record));
                    }
                }
                Ok(ChunkMsg::Permission { request, chosen }) => {
                    release(&mut seq, on_item, &mut spelling, &mut aside, &session_id);
                    text_open = false;
                    on_item(TurnItem::Machinery(MachineryRecord::from_permission_request(
                        &request,
                        &chosen,
                        &session_id,
                        seq,
                    )));
                    seq += 1;
                }
                Ok(ChunkMsg::Usage { used, size, usage }) => {
                    release(&mut seq, on_item, &mut spelling, &mut aside, &session_id);
                    text_open = false;
                    on_item(TurnItem::Machinery(MachineryRecord::from_context_usage(
                        used,
                        size,
                        &usage,
                        &session_id,
                        seq,
                    )));
                    seq += 1;
                }
                Ok(ChunkMsg::Cancel) => {
                    // The interrupt has already gone out (`NativeCancelHandle::cancel` writes
                    // it BEFORE waking us, so a fast agent's `result` cannot arrive before
                    // the deadline exists). All this arm does is start the clock.
                    cancel_deadline.get_or_insert_with(|| std::time::Instant::now() + cancel_grace());
                }
                Ok(ChunkMsg::Done(result)) => {
                    release(&mut seq, on_item, &mut spelling, &mut aside, &session_id);
                    let reason = stop_reason_of(&result);
                    // **Refused for a usage limit** (fill-first's backstop): said as that, so the
                    // spine and the work host continue on the next account instead of reading an
                    // ordinary failure. Whether the child stays alive afterwards is not known
                    // (Frank's finding 1), so the signal is honored on either ending.
                    if let Some(resets_at) = limit.or_else(|| usage_limit_signal(&result)) {
                        let detail = result.get("errors").or_else(|| result.get("result"))
                            .map(Value::to_string).unwrap_or_else(|| reason.clone());
                        return Err(limit_error(resets_at, detail));
                    }
                    return native_result(&result);
                }
                Err(RecvTimeoutError::Timeout) if cancel_deadline.is_none() => continue,
                Err(RecvTimeoutError::Timeout) => {
                    // The agent was told to interrupt and did not answer within the grace
                    // window. Stop rendering this turn — and DETACH the sink first, so
                    // anything the agent says afterwards cannot be routed into whatever turn
                    // runs next. What was already said is released first: it is this turn's.
                    release(&mut seq, on_item, &mut spelling, &mut aside, &session_id);
                    current_prompt.lock().unwrap().retain(|pending| pending.command_uuid != command_uuid);
                    return Ok(STOP_REASON_CANCEL_UNACKNOWLEDGED.to_string());
                }
                Err(RecvTimeoutError::Disconnected) => {
                    release(&mut seq, on_item, &mut spelling, &mut aside, &session_id);
                    if let Some(resets_at) = limit {
                        return Err(limit_error(resets_at, "the session ended after the limit".into()));
                    }
                    return Err(NativeError::Closed);
                }
            }
        }
    }
}

/// A streamed keep-alive: it says nothing about the text, so it must not end a run of it (the
/// drain's American spelling hold-back releases on every other frame).
fn is_ping(frame: &Value) -> bool {
    frame.get("type").and_then(Value::as_str) == Some("stream_event")
        && frame.get("event").and_then(|e| e.get("type")).and_then(Value::as_str) == Some("ping")
}

/// `used`, summed from a vendor `usage` object, or `None` if it carries no input side.
///
/// **The three fields are summed because context occupancy is all three.** `input_tokens`
/// alone was 2 on a turn whose real occupancy was 29,342 — almost everything the model is
/// holding arrives as `cache_read_input_tokens` (25,737) and `cache_creation_input_tokens`
/// (3,603). 2 + 25,737 + 3,603 = **29,342**, which is the first of the four monotonic
/// numerators `findings.md` §4 records for `run9`. Re-derived here rather than trusted.
///
/// Returns `None` when NONE of the three is present, because a `usage` object with no input
/// side says nothing about the context and a zero would be a false measurement.
fn tokens_in_context(usage: &Value) -> Option<u64> {
    let keys = ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"];
    let mut total = 0u64;
    let mut any = false;
    for k in keys {
        if let Some(n) = usage.get(k).and_then(|v| v.as_u64()) {
            total += n;
            any = true;
        }
    }
    any.then_some(total)
}

/// The cancel seam for a live native session: `control_request{interrupt}` to the child,
/// then a wake for the local drain loop.
///
/// Both halves are needed and they answer different failure modes. The interrupt is the
/// protocol-correct request that the AGENT stop working. The local wake is what makes the
/// CEO's stop authoritative in RichOS regardless of whether the agent complies — RichOS stops
/// rendering and records the stop either way, because "the child ignored us" is not a reason
/// to leave the CEO looking at a turn he ended.
pub struct NativeCancelHandle {
    stdin: Arc<Mutex<ChildStdin>>,
    current_prompt: Arc<Mutex<Vec<PendingTurn>>>,
    operation_cancel: Arc<Mutex<OperationCancellation>>,
    action_grants: Vec<ActionGrant>,
    process_fence: crate::owned_process::ProcessFence,
    settle_workers_on_stop: bool,
    next_id: Arc<AtomicI64>,
    /// The client's control-reply table, so a `stop_task` is answered the way every other
    /// control request this file sends is (finding 11 v2).
    pending: Arc<Mutex<std::collections::HashMap<String, Sender<Value>>>>,
    /// The reader's state, where the provider's `task_notification` for a command is read.
    reader_state: Arc<Mutex<ReaderState>>,
    /// Set when the child's output closed: nothing more will be answered.
    reader_closed: Arc<AtomicBool>,
}

/// How long a `stop_task` waits for the provider's reply. The operator client measured this
/// request answered on the same wire (probe P12); the bound only turns a silent child into
/// [`CommandStop::Unconfirmed`] rather than a hang, and decides no verdict on its own.
const STOP_TASK_REPLY_WAIT: Duration = Duration::from_secs(5);
/// How long an accepted `stop_task` waits for the command's own `task_notification`. Past it
/// the answer is [`CommandStop::Unconfirmed`] — a stated unknown, never "stopped".
const STOP_TASK_ENDING_WAIT: Duration = Duration::from_secs(5);

impl TurnCancel for NativeCancelHandle {
    fn set_model(&self,model:crate::config::ModelChoice)->Result<bool,String> {
        if self.reader_closed.load(Ordering::SeqCst) {return Ok(false)}
        let id=format!("richos_set_model_{}",self.next_id.fetch_add(1,Ordering::SeqCst));
        let (tx,rx)=channel();self.pending.lock().unwrap().insert(id.clone(),tx);
        let result=(|| {
            NativeClient::write_line(&self.stdin,&json!({"type":"control_request","request_id":id,
                "request":{"subtype":"set_model","model":model.cli_alias()}})).map_err(|error|error.to_string())?;
            let response=rx.recv_timeout(HANDSHAKE_TIMEOUT).map_err(|error|format!("Model change was not confirmed: {error}"))?;
            if response["response"]["subtype"]!="success" {return Err(format!("Model change refused: {}",response["response"]["error"]))}
            Ok(true)
        })();
        self.pending.lock().unwrap().remove(&id);result
    }

    fn worker_status(&self) -> Option<crate::worker_status::WorkerStatusView> {
        if self.reader_closed.load(Ordering::SeqCst) {return None}
        Some(self.reader_state.lock().unwrap().agent_tasks.worker_status())
    }

    fn send_input(&self, id: &str, text: &str, completed: Box<dyn FnOnce(crate::steering::InputOutcome) + Send>) -> Result<bool, String> {
        let operation = self.operation_cancel.lock().unwrap();
        if self.reader_closed.load(Ordering::SeqCst) || operation.requested { return Ok(false); }
        {
            let mut current = self.current_prompt.lock().unwrap();
            if current.iter().any(|pending| pending.command_uuid == id) { return Ok(true); }
            current.push(PendingTurn {
                sink: TurnSink::Input { completed, limit: None, overload: None }, command_uuid: id.to_string(),
                prompt_text: text.to_string(), hook_request_id: None,
                prepare: None,
                complete: None,
                preparation_error: None,
                phase: TurnPhase::Unconfirmed,
                    // Live input always requires its own native UUID acknowledgement. A queue
                // count cannot identify one of several independently submitted commands.
                child_names_turns: true,
                awaiting_connection: false,
                continued_as: None,
            });
        }
        let message = json!({"type":"user", "uuid":id,
            "message":{"role":"user", "content":[{"type":"text", "text":text}]}});
        if let Err(error) = NativeClient::write_line(&self.stdin, &message) {
            self.current_prompt.lock().unwrap().retain(|pending| pending.command_uuid != id);
            return Err(error.to_string());
        }
        Ok(true)
    }

    fn shutdown(&self) {
        let _ = self.cancel();
        self.process_fence.kill();
    }

    fn begin_operation(&self) {
        *self.operation_cancel.lock().unwrap() = OperationCancellation { active: true, requested: false };
    }

    fn end_operation(&self) {
        *self.operation_cancel.lock().unwrap() = OperationCancellation::default();
    }

    fn cancel(&self) -> bool {
        let mut operation = self.operation_cancel.lock().unwrap();
        if operation.active { operation.requested = true; }
        // Production Stop ends this provider to settle its workers. Reject new input now,
        // before the asynchronous EOF, so the ordinary opener reaps and resumes it once.
        if self.settle_workers_on_stop { self.reader_closed.store(true, Ordering::SeqCst); }
        // Revoke before interrupting, while holding the same lock as grant activation.
        // Missing scopes are normal before the initial company/thread binding.
        for grant in &self.action_grants {
            if grant.path().exists() && grant.set(false).is_err() {
                if std::fs::remove_file(grant.path()).is_err() { self.process_fence.kill(); }
            }
        }
        // Take the sink FIRST so the ordering is unambiguous: interrupt out, then wake. The
        // reverse order would let a very fast agent's `result` overtake the wake, and the
        // loop would return `end_turn` for a turn the CEO stopped. Measured: the agent acked
        // the interrupt in 0.9 ms, so this race is real rather than theoretical.
        let (finished, completion)=channel();
        let (sinks, running): (Vec<_>, bool) = {
            let mut inputs = self.current_prompt.lock().unwrap();
            if self.settle_workers_on_stop {
                if let Some(input)=inputs.iter_mut().find(|pending| !pending.not_yet_ours() || pending.awaiting_connection) {
                    let previous=input.complete.take();
                    input.complete=Some(Box::new(move || {
                        let result=previous.map_or(Ok(()),|complete|complete());
                        let _notified=finished.send(());
                        result
                    }));
                }
            }
            // A turn that waits for the connection is still his running turn to stop.
            let active = inputs.iter().filter(|pending| !pending.not_yet_ours() || pending.awaiting_connection);
            let running = active.clone().next().is_some();
            let sinks = active.filter_map(|pending| match &pending.sink {
                TurnSink::Prompt(sink) => Some(sink.clone()), _ => None,
            }).collect();
            (sinks, running)
        };
        if !running {
            if self.settle_workers_on_stop { self.process_fence.kill(); }
            return operation.active;
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = json!({
            "type": "control_request",
            "request_id": format!("richos_interrupt_{id}"),
            "request": { "subtype": "interrupt" }
        });
        let wrote = NativeClient::write_line(&self.stdin, &request).is_ok();
        let no_waiter = sinks.is_empty();
        let woke = sinks.into_iter().fold(no_waiter, |woke, sink| sink.send(ChunkMsg::Cancel).is_ok() || woke);
        // Native writes the interruption into its own history before the terminal result,
        // after its interrupt ACK. Let the existing completion/grace boundary finish first.
        // Completion may need this same lock to revoke scopes, so never wait holding it.
        drop(operation);
        if self.settle_workers_on_stop {
            if wrote {let _completed=completion.recv_timeout(cancel_grace());}
            self.process_fence.kill();
        }
        wrote && woke
    }

    /// `control_request{stop_task}` for one of the provider's own background tasks, the frame
    /// `agent_tasks::stop_task_request` builds (measured on this wire in operator probe P12).
    /// No turn is interrupted and no other
    /// command is touched (hunt 2026-09-29 part 1, finding 11).
    ///
    /// **Its answer is the provider's, not the pipe's** (finding 11, v2 re-check). This used
    /// to return `true` the moment the line was written, and the work host then told him the
    /// job's command had stopped; a provider that refused the stop left it running while his
    /// assignment said otherwise, and the refusal sat unread on the between-turn lane. Now
    /// the reply is awaited by its request id (an `error` reply is
    /// [`CommandStop::Refused`]), and only the task's own `task_notification` — the authority
    /// for an ending everywhere else in this file — makes it [`CommandStop::Ended`].
    fn stop_worker(&self, tool_use_id: &str) -> crate::steering::CommandStop {
        let task = self.reader_state.lock().unwrap().agent_tasks.task_for_tool(tool_use_id);
        task.map_or(crate::steering::CommandStop::NotDelivered, |task| self.stop_background_command(&task))
    }

    fn own_commands(&self) -> Vec<crate::steering::OwnCommand> {
        self.reader_state.lock().unwrap().agent_tasks.own_running_commands()
    }

    fn stop_background_command(&self, task_id: &str) -> crate::steering::CommandStop {
        use crate::steering::CommandStop;
        let ended = |state: &Arc<Mutex<ReaderState>>| {
            let state = state.lock().unwrap();
            state.agent_tasks.task_ended(task_id)
                || state.background.iter().any(|c| c.task_id == task_id && c.ended.is_some())
        };
        // A command that already ended needs no stop, and saying so is the true sentence.
        if ended(&self.reader_state) {
            return CommandStop::Ended;
        }
        let id = format!("richos_stop_task_{}", self.next_id.fetch_add(1, Ordering::SeqCst));
        let request = crate::agent_tasks::stop_task_request(&id, task_id);
        let (tx, rx) = channel();
        self.pending.lock().unwrap().insert(id.clone(), tx);
        if NativeClient::write_line(&self.stdin, &request).is_err() {
            self.pending.lock().unwrap().remove(&id);
            return CommandStop::NotDelivered;
        }
        // A child that is gone answers nothing; waiting out the bounds for it would only hold
        // his Stop. What it left running is not known, so that is Unconfirmed, not Ended.
        let gone = || self.reader_closed.load(Ordering::SeqCst);
        // load-bound: hang guard only; the verdict is the provider's reply, and no reply is a stated unknown.
        let deadline = std::time::Instant::now() + STOP_TASK_REPLY_WAIT;
        loop {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(reply) => {
                    let response = reply.get("response").cloned().unwrap_or(Value::Null);
                    if response.get("subtype").and_then(Value::as_str) != Some("success") {
                        let why = response.get("error").and_then(Value::as_str).unwrap_or("no reason given");
                        return CommandStop::Refused(why.to_string());
                    }
                    break;
                }
                // No reply in the bound: the table entry goes, so a late reply is retained as
                // unmatched traffic. The ending below can still be witnessed.
                Err(_) if gone() || std::time::Instant::now() >= deadline => {
                    self.pending.lock().unwrap().remove(&id);
                    break;
                }
                Err(_) => {}
            }
        }
        // load-bound: hang guard only; the loop waits for the provider's own ending, and the deadline yields Unconfirmed, never Ended.
        let deadline = std::time::Instant::now() + STOP_TASK_ENDING_WAIT;
        loop {
            if ended(&self.reader_state) {
                return CommandStop::Ended;
            }
            if gone() || std::time::Instant::now() >= deadline {
                return CommandStop::Unconfirmed;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for NativeClient {
    fn drop(&mut self) {
        if let Some(watch)=&self.stall_watch {watch.closing.store(true,Ordering::SeqCst);}
        drop(self.stall_watch.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
        if self.reap_state.is_some() {
            crate::lease_commands::LiveLeases::process().retire(&self.session_id);
        }
    }
}

impl NativeClient {
    /// The tool commands this lease has running outside its provider's group, from its
    /// supervisor's state file. `None` for a lease with no supervisor.
    pub fn running_commands(&self) -> Option<crate::lease_commands::CommandReading> {
        self.reap_state.as_deref().map(crate::lease_commands::read)
    }
}

/// Every environment fact [`search_claude_bin`] reads, gathered once into a plain struct so a
/// test drives the search with a value instead of mutating this process's real `$HOME`,
/// `$HOMEBREW_PREFIX` or `~/.npmrc` — the same discipline `setup.rs::SetupPaths` documents:
/// *"injected rather than read… the GUI condition is then a VALUE in a test instead of a
/// mutation of the test process."* `Default` yields an environment with nothing to check, on
/// purpose: a test names exactly the candidates it means to exercise, and never accidentally
/// finds a real `claude` sitting on the machine actually running the test.
#[derive(Debug, Clone, Default)]
pub(crate) struct ClaudeBinEnv {
    /// `$RICHOS_CLAUDE_BIN` — exclusive. Named by an operator, so a miss here is reported,
    /// never silently routed around to a place nobody named.
    pub(crate) explicit: Option<String>,
    /// `$HOME` — the `~/.local/bin/claude` launcher Anthropic's own installer writes and
    /// retargets on every self-update (see the module doc above, §16).
    pub(crate) home: Option<String>,
    /// Every Homebrew prefix to check, IN ORDER. In production this is `$HOMEBREW_PREFIX`
    /// (exported by `brew shellenv`, when it has been sourced) followed by Homebrew's own two
    /// real install roots — Apple Silicon then Intel — checked UNCONDITIONALLY, because a
    /// Finder-launched app carries neither `$HOMEBREW_PREFIX` nor a shell's `$PATH` (see
    /// `main.rs`: *"A Finder launch's PATH is /usr/bin:/bin:/usr/sbin:/sbin"*, the identical
    /// reason `PATH`-search cannot be the whole answer here either).
    pub(crate) homebrew_prefixes: Vec<String>,
    /// npm's own configured global prefix: `$npm_config_prefix` (npm's env spelling of
    /// `npm config set prefix …`) when exported, else the `prefix` line of `~/.npmrc`, npm's
    /// on-disk record of the same setting, read without executing `npm` on a boot path.
    pub(crate) npm_prefix: Option<String>,
}

impl ClaudeBinEnv {
    fn from_process() -> Self {
        let nonempty = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let home = nonempty("HOME");
        let mut homebrew_prefixes = Vec::new();
        if let Some(exported) = nonempty("HOMEBREW_PREFIX") {
            homebrew_prefixes.push(exported);
        }
        for default in HOMEBREW_DEFAULT_PREFIXES {
            if !homebrew_prefixes.iter().any(|p| p == default) {
                homebrew_prefixes.push(default.to_string());
            }
        }
        let npm_prefix = nonempty("npm_config_prefix")
            .or_else(|| home.as_deref().and_then(read_npmrc_prefix));
        ClaudeBinEnv { explicit: nonempty("RICHOS_CLAUDE_BIN"), home, homebrew_prefixes, npm_prefix }
    }
}

/// Homebrew's own two install roots on macOS — Apple Silicon first, since that is every Mac
/// RichOS ships to today; Intel second, for a Rosetta-era machine or a relocated Homebrew.
const HOMEBREW_DEFAULT_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];

/// Read `prefix = <path>` out of an npm config file, the way `npm config get prefix` would
/// without executing `npm` on a boot path. Anything else in the file — comments, other keys,
/// surrounding whitespace, quoting — is tolerated or ignored; a malformed file reads as an
/// absent one rather than a panic.
fn read_npmrc_prefix(home: &str) -> Option<String> {
    let text = std::fs::read_to_string(Path::new(home).join(".npmrc")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        // A line that is not the `prefix` setting (blank, comment, another key) is skipped, not
        // a reason to stop reading: the real prefix can be on any later line.
        let Some(rest) = line.strip_prefix("prefix") else { continue };
        let Some(value) = rest.trim_start().strip_prefix('=') else { continue };
        let value = value.trim();
        if !value.is_empty() {
            return Some(value.trim_matches('"').trim_matches('\'').to_string());
        }
    }
    None
}

/// The one search both [`resolve_claude_bin`] and [`resolve_claude_bin_checked`] run, so the
/// two can never answer differently about the same machine — which is exactly how the
/// 2026-09-17 candidate walk failed: `setup.rs::find_claude` walked `$PATH` and could say
/// "found", while this file's own resolver checked a narrower list and could still hand the
/// provider a bare name.
///
/// Returns the absolute path on success — **never a bare name**, so PATH is irrelevant to
/// every caller from here on, including the provider whose own `PATH` gets replaced
/// (`runtime.rs::EngineRuntime::path`) — or every place looked, in order, on failure.
fn search_claude_bin(env: &ClaudeBinEnv) -> Result<std::path::PathBuf, Vec<String>> {
    let mut looked = Vec::new();

    if let Some(explicit) = &env.explicit {
        let path = std::path::PathBuf::from(explicit);
        looked.push(format!("{} ($RICHOS_CLAUDE_BIN)", path.display()));
        // EXCLUSIVE, as `setup.rs::find_claude` treats the same override: an operator who
        // named a path is making a statement, and falling through to a place nobody named
        // would silently overrule it.
        return if path.is_file() { Ok(path) } else { Err(looked) };
    }

    if let Some(home) = &env.home {
        let path = Path::new(home).join(".local/bin/claude");
        looked.push(path.display().to_string());
        if path.is_file() {
            return Ok(path);
        }
    }

    for root in &env.homebrew_prefixes {
        let path = Path::new(root).join("bin/claude");
        looked.push(format!("{} (Homebrew)", path.display()));
        if path.is_file() {
            return Ok(path);
        }
    }

    if let Some(prefix) = &env.npm_prefix {
        let path = Path::new(prefix).join("bin/claude");
        looked.push(format!("{} (npm global prefix)", path.display()));
        if path.is_file() {
            return Ok(path);
        }
    }

    Err(looked)
}

/// Resolve the `claude` binary, for callers that accept the old, infallible shape — examples
/// and dev scripts run from a terminal, where a bare name and the caller's OWN shell `$PATH`
/// are a reasonable, well-understood default. **The desktop app's own boot never calls this
/// one** — see [`resolve_claude_bin_checked`] — because a bare name is exactly the value that
/// produced the 2026-09-17 candidate-walk defect once handed to a provider whose `PATH` had
/// already been replaced.
///
/// Order: [`search_claude_bin`] — `$RICHOS_CLAUDE_BIN`, then `$HOME/.local/bin/claude`, then
/// Homebrew, then npm's global prefix — and only on a total miss, the bare name `claude`,
/// left for the caller's own `PATH` to resolve or for [`NativeError::BinaryMissing`] to name
/// at the spawn.
///
/// **Never the keychain, never a token, never an API key.** This function resolves an
/// executable path and nothing else.
pub fn resolve_claude_bin() -> std::path::PathBuf {
    search_claude_bin(&ClaudeBinEnv::from_process())
        .unwrap_or_else(|_| std::path::PathBuf::from("claude"))
}

/// The same search as [`resolve_claude_bin`], but for the one caller that must never silently
/// keep going on a bare name — the desktop boot. On a total miss this refuses, naming every
/// place looked, the way `setup.rs::find_claude` already reports a missing engine at boot.
pub fn resolve_claude_bin_checked() -> Result<std::path::PathBuf, NativeError> {
    search_claude_bin(&ClaudeBinEnv::from_process())
        .map_err(|looked| NativeError::ClaudeNotFound { looked: looked.join("; ") })
}



type ScopePreparation = Box<dyn FnOnce() -> Result<Option<String>, CognitionError> + Send>;

// One input's existing scope metadata, consumed only by its native prompt hook.
struct NativeTurnScope {
    session_id: String,
    role: LeaseRole,
    question_scope: Option<std::path::PathBuf>,
    assignments_scope: Option<std::path::PathBuf>,
    status_scope: Option<std::path::PathBuf>,
    continuity: Option<(crate::ecs::EcsBridge, std::path::PathBuf)>,
    continuity_tools_scope: Option<std::path::PathBuf>,
    engine_profile: Option<crate::engine_profile::EngineProfile>,
    seat: Option<String>,
    prepare_turn_supported: bool,
}
impl NativeTurnScope {
    fn prepare_question_scope(&self,entity:&str,thread:&str,turn:&str,asker:&str,method:&str)->Result<(),CognitionError> {
        let Some(path)=self.question_scope.clone() else {return Ok(());};
        let root=self.engine_profile.as_ref().map(|p|p.state.clone())
            .unwrap_or_else(||path.parent().and_then(Path::parent).unwrap_or(Path::new(".")).join("engine-state"));
        let declared=self.engine_profile.as_ref().filter(|p|p.coordination.join("orchestration.config").is_file());
        crate::question_tools::write_scope(&path,&crate::question_tools::Scope {
            context:crate::questions::AskScope { obligation_id:None,root,entity_id:entity.into(),thread_id:thread.into(),turn_id:turn.into(),asker:asker.into(),session_id:self.session_id.clone(),
                engine:declared.map(|p|p.engine.clone()),entity_root:declared.map(|p|p.coordination.clone()),app_run:None},
            actions_allowed:false,answer_method:method.into(),surface:"mac".into()
        }).map_err(CognitionError::Io)
    }
    fn prepare_work_turn(&mut self, binding: &crate::entity::ThreadBinding, turn: &str, original_turn: &str,
        source: crate::ledger::Source, text: &str, _on_item: &mut dyn FnMut(TurnItem)) -> Result<Option<String>, CognitionError> {
        if let Some(refusal) = work_preparation_refusal(self.role) {
            return Err(CognitionError::Protocol(refusal.into()));
        }
        self.prepare_question_scope(binding.entity_id().as_str(),binding.thread_id(),turn,"front_desk",
            if source==crate::ledger::Source::Jam {"spoken"}else{"typed"})?;
        let profile_state = self.engine_profile.as_ref().map(|p| p.state.clone());
        let user_instruction=if matches!(source,crate::ledger::Source::Text|crate::ledger::Source::Jam) {
            use sha2::Digest;
            Some(crate::ecs::UserInstruction::with_text(format!("ledger:{}:{original_turn}",binding.thread_id()),
                format!("{:x}",sha2::Sha256::digest(text.as_bytes())),Some(text)))
        } else if source==crate::ledger::Source::Agent {
            serde_json::from_str::<crate::session_tools::Message>(text).ok().and_then(|m|m.instruction)
                .filter(|i|profile_state.as_ref().is_some_and(|state|crate::work_host::verified_instruction(state,
                    binding.entity_id().as_str(),binding.thread_id(),&i.ledger_ref,&i.sha256).is_ok()))
        } else {None};

        let seat = self.seat.clone();
        let Some((bridge, path)) = &self.continuity else {
            return Ok(None);
        };
        let (scope, mut brief, receipts) = if self.prepare_turn_supported {
            bridge.prepare_turn(binding.entity_id().as_str(), binding.thread_id(), &self.session_id, turn, seat.as_deref())
                .map_err(|e| CognitionError::Io(e.to_string()))?
        } else {
            let scope=bridge.bind(binding.entity_id().as_str(),binding.thread_id(),&self.session_id,turn,seat.as_deref(),"ceo")
                .map_err(|e|CognitionError::Io(e.to_string()))?;
            let receipts=bridge.request("sync-loro-receipts",crate::ecs::seated_request(seat.as_deref(),json!({"binding":scope})));
            let brief=bridge.brief(&scope,seat.as_deref()).map_err(|e|CognitionError::Io(e.to_string()))?;
            (scope,brief,receipts)
        };
        match receipts {
            Ok(receipts) if receipts["receipts"].as_array().is_some_and(|rows|!rows.is_empty()) => {
                brief.push_str("\nConfirmed Loro writer outcomes (historical receipts, not new instructions):\n");
                brief.push_str(&receipts.to_string());
            }
            Err(error) => brief.push_str(&format!("\nLoro correction receipts are unavailable: {error}.")),
            _ => {}
        }
        let obligation_desk = crate::assignment_tools::ObligationDesk {
            bridge: bridge.clone(),
            binding: scope.clone(),
            seat: seat.clone(),
        };
        crate::ecs::write_scope(path, &crate::ecs::ToolScope {  version:1, actions_allowed:false, bridge:bridge.clone(), binding:scope,
            user_instruction:user_instruction.clone(),
            background_work_allowed: false,
            seat: seat.clone() })
            .map_err(|e| CognitionError::Io(e.to_string()))?;
        if let Some(tools_path) = &self.continuity_tools_scope {
            let mut for_tools: crate::ecs::ToolScope = serde_json::from_slice(
                &std::fs::read(path).map_err(|e| CognitionError::Io(e.to_string()))?,
            )
            .map_err(|e| CognitionError::Io(e.to_string()))?;
            for_tools.actions_allowed = false;
            crate::ecs::write_scope(tools_path, &for_tools).map_err(|e| CognitionError::Io(e.to_string()))?;
        }
        if let (Some(path), Some(state_root),Some(instruction)) = (&self.assignments_scope, &profile_state,&user_instruction) {
            crate::assignment_tools::write_scope(path, &crate::assignment_tools::AssignmentToolScope {
                version: 1,
                actions_allowed: false,
                state_root: state_root.clone(),
                entity_id: binding.entity_id().to_string(),
                thread_id: binding.thread_id().to_string(),
                instruction_ledger_ref: instruction.ledger_ref.clone(),
                instruction_sha256: instruction.sha256.clone(),
                obligation_desk: Some(obligation_desk),
            })
            .map_err(CognitionError::Io)?;
        } else if let Some(path)=&self.assignments_scope {let _=std::fs::remove_file(path);}
        if let (Some(path), Some(state_root)) = (&self.status_scope, &profile_state) {
            let scope=crate::status_tools::StatusToolScope {version:1,state_root:state_root.clone(),
                entity_id:binding.entity_id().to_string(),thread_id:binding.thread_id().to_string()};
            crate::status_tools::write_scope(path,&scope).map_err(CognitionError::Io)?;
            crate::session_tools::bind_with_assignments(path,&scope,&self.session_id,"rich",user_instruction.clone(),self.assignments_scope.as_deref()).map_err(CognitionError::Io)?;
        }
        if let Some(path)=&self.question_scope {
            let scope=crate::question_tools::read_scope(path).map_err(CognitionError::Io)?;
            let questions:Vec<_>=crate::questions::Store::new(&scope.context.root).list(&scope.context.entity_id,&scope.context.thread_id)
                .map_err(CognitionError::Io)?.into_iter().filter(|q|q.state==crate::questions::State::Open || !q.delivered).map(|q|q.public_value()).collect();
            if !questions.is_empty() { brief.push_str(&json!({"event":"question_records","questions":questions}).to_string()); }
        }
        Ok(Some(brief))
    }
}

/// The real Cognition: a live native session behind the durable spine.
pub struct NativeCognition {
    client: NativeClient,
    session_id: String,
    onboarding_scope: Option<std::path::PathBuf>,
    /// The `richos_assignments` scope (`assignment_tools.rs`), written at turn start and
    /// closed at turn end with the other grants. `None` on a work lease, which never gets
    /// that server: a background connection does not give itself more work.
    assignments_scope: Option<std::path::PathBuf>,
    /// The `richos_status` scope (`status_tools.rs`), written at turn start beside the
    /// register's. `None` on a work lease, which never gets that server: the back end reads
    /// its own obligation on its own seat and has no business in the front desk's record.
    status_scope: Option<std::path::PathBuf>,
    continuity: Option<(crate::ecs::EcsBridge, std::path::PathBuf)>,
    /// **The scope the `richos_continuity` SERVER reads, which is not the one the engine's
    /// hook reads** — see the long note in `prepare_work_turn` for why there are two files.
    ///
    /// `None` on a work lease (it has no continuity server at all) and on any lease started
    /// without a bridge. Its grant is the one that waits for his first words.
    continuity_tools_scope: Option<std::path::PathBuf>,
    /// The work seat's binding and its seat name, once this lease has taken an assignment.
    /// Held so the host can read the OBLIGATION on that seat — the one thing allowed to
    /// settle an assignment (`mega-lander/app.py:664-669`).
    work_binding: Option<(crate::ecs::Binding, String)>,
    engine_profile: Option<crate::engine_profile::EngineProfile>,
    /// Conversation or work (background-work spec §2.1). Set at spawn and never changed:
    /// a lease that was not given the continuity tools cannot become one that was.
    role: LeaseRole,
    /// Does this engine hold one CEO cursor per conversation thread? Asked once, on this
    /// lease's first turn, and cached for its life — see [`NativeCognition::ceo_thread_seat`].
    /// `None` means not yet asked, which is a different state from "asked, and no".
    ceo_thread_seats: Option<bool>,
    classifier: Option<Arc<Mutex<crate::request_classifier::RequestClassifier>>>,
    prepared_scope: Option<ScopePreparation>,
    company_context: Option<String>,
    prepare_turn_supported: bool,
    pending_onboarding: Option<crate::onboarding_tools::OnboardingToolScope>,
    pending_channel: Option<String>,
    title_writer: Option<crate::cognition::TitleWriter>,
    routing_context: String,
    app_messages: Vec<(String,String)>,
}

impl NativeCognition {
    /// Spawn `claude` with `cwd` = the engine repo (so it auto-loads the persona and hooks),
    /// run the handshake, and return a lease ready to be re-primed and handed turns.
    ///
    /// **`cwd` replaces ACP's `session/new {cwd}`.** The native binary takes its working
    /// directory from the process, and echoes it back on `system/init.cwd` — measured
    /// (`raw/run3:1`).
    ///
    /// **`doctrine` is the rendered standing instruction and `skills` is the rendered plugin
    /// root** — `doctrine::ensure_rendered` + `skills::ensure_rendered` (the shell, which knows
    /// its own data directory), or `doctrine::ensure_for_install` + `skills::ensure_for_install`
    /// (everything headless). Both are required, because the alternative is a lease that comes
    /// up as generic Claude, or as Rich with nothing to reach for, and says nothing about it.
    pub fn start(claude_bin: &Path, engine_cwd: &Path, doctrine: &Path, skills: &Path) -> Result<Self, NativeError> {
        let client = NativeClient::spawn(claude_bin, engine_cwd, doctrine, skills)?;
        let session_id = client.session_id().to_string();
        Ok(NativeCognition { client, session_id, onboarding_scope: None, assignments_scope: None, status_scope: None, continuity: None, continuity_tools_scope: None, work_binding: None, engine_profile: None, role: LeaseRole::Conversation, ceo_thread_seats: None, classifier: None, routing_context: String::new(), app_messages: Vec::new(), prepared_scope: None, company_context: None, prepare_turn_supported: false, pending_onboarding: None, pending_channel: None, title_writer: None })
    }
    /// A chat lease with app-owned, company-scoped persistence tools. The scope is
    /// supplied by the spine before priming, never selected by the model. The host
    /// supplies its configuration folder explicitly; instruction paths do not locate state.
    pub fn start_with_onboarding(bin: &Path, cwd: &Path, config_dir: &Path, skills: &Path,
        executable: &Path, control: Option<&crate::steering::ConversationControl>) -> Result<Self, NativeError> {
        let doctrine = skills.join("agents/rich.md");
        let scopes = config_dir.join("onboarding-scopes");
        std::fs::create_dir_all(&scopes)?;
        let identity = uuid::Uuid::new_v4();
        let scope = scopes.join(format!("{identity}.json"));
        let assignments = scopes.join(format!("{identity}-assignments.json"));
        let status = scopes.join(format!("{identity}-status.json"));
        let client = NativeClient::spawn_with_tools(bin, cwd, Some((&doctrine, skills)), Some((executable, &scope, &assignments, &status)), None, None, None, control, LeaseRole::Conversation)?;
        let session_id = client.session_id().to_string();
        Ok(Self { client, session_id, onboarding_scope: Some(scope), assignments_scope: Some(assignments), status_scope: Some(status), continuity: None, continuity_tools_scope: None, work_binding: None, engine_profile: None, role: LeaseRole::Conversation, ceo_thread_seats: None, classifier: None, routing_context: String::new(), app_messages: Vec::new(), prepared_scope: None, company_context: None, prepare_turn_supported: false, pending_onboarding: None, pending_channel: None, title_writer: None })
    }
    pub fn start_with_continuity(bin: &Path, cwd: &Path, config_dir: &Path, skills: &Path,
        executable: &Path, bridge: crate::ecs::EcsBridge,
        control: Option<&crate::steering::ConversationControl>) -> Result<Self, NativeError> {
        bridge.request("hello", json!({})).map_err(|e| NativeError::Protocol(e.to_string()))?;
        let doctrine = skills.join("agents/rich.md");
        let scopes = config_dir.join("onboarding-scopes");
        std::fs::create_dir_all(&scopes)?;
        let identity = uuid::Uuid::new_v4();
        let scope = scopes.join(format!("{identity}.json"));
        let continuity_scope = scopes.join(format!("{identity}-continuity.json"));
        let continuity_tools = scopes.join(format!("{identity}-continuity-tools.json"));
        std::fs::write(&continuity_tools, "{\"version\":1,\"actions_allowed\":false}\n")?;
        let assignments = scopes.join(format!("{identity}-assignments.json"));
        let status = scopes.join(format!("{identity}-status.json"));
        let client = NativeClient::spawn_with_tools(bin, cwd, Some((&doctrine, skills)),
            Some((executable, &scope, &assignments, &status)), Some((&bridge, &continuity_scope)), Some(continuity_tools.as_path()), None, control, LeaseRole::Conversation)?;
        let session_id = client.session_id().to_string();
        Ok(Self { client, session_id, onboarding_scope: Some(scope), assignments_scope: Some(assignments), status_scope: Some(status), continuity: Some((bridge, continuity_scope)), continuity_tools_scope: Some(continuity_tools), work_binding: None, engine_profile: None, role: LeaseRole::Conversation, ceo_thread_seats: None, classifier: None, routing_context: String::new(), app_messages: Vec::new(), prepared_scope: None, company_context: None, prepare_turn_supported: false, pending_onboarding: None, pending_channel: None, title_writer: None })
    }

    /// Settings-isolated desktop lease from verified engine delivery.
    pub fn start_with_engine(bin: &Path, doctrine: &Path, skills: &Path,
        executable: &Path, bridge: crate::ecs::EcsBridge, profile: crate::engine_profile::EngineProfile,
        control: Option<&crate::steering::ConversationControl>) -> Result<Self, NativeError> {
        bridge.request("hello", json!({})).map_err(|e| NativeError::Protocol(e.to_string()))?;
        let scopes = profile.state.join("scopes");
        std::fs::create_dir_all(&scopes)?;
        let identity = uuid::Uuid::new_v4();
        let scope = scopes.join(format!("{identity}-onboarding.json"));
        let continuity_scope = scopes.join(format!("{identity}-continuity.json"));
        // SessionStart and the first internal prime precede entity binding. The
        // hook sees an explicit closed grant, never a guessed active scope.
        std::fs::write(&continuity_scope, "{\"version\":1,\"actions_allowed\":false}\n")?;
        // The continuity SERVER's own scope, closed the same way and for longer: it opens
        // when he has been spoken to, not when the turn starts (`prepare_work_turn`'s note).
        let continuity_tools = scopes.join(format!("{identity}-continuity-tools.json"));
        std::fs::write(&continuity_tools, "{\"version\":1,\"actions_allowed\":false}\n")?;
        let assignments = scopes.join(format!("{identity}-assignments.json"));
        let status = scopes.join(format!("{identity}-status.json"));
        // Prepare Haiku alongside Rich, before this conversation can be used.
        let classifier=Some(Arc::new(Mutex::new(crate::request_classifier::RequestClassifier::new(bin,&profile,skills))));
        // And the Haiku that names this conversation from its first message, started only
        // when that message arrives (`thread_title.rs`).
        let title_writer:Option<crate::cognition::TitleWriter>={
            let (bin,profile)=(bin.to_path_buf(),profile.clone());
            Some(Arc::new(move |first:&str|crate::thread_title::write(&bin,&profile,first)))
        };
        let client = NativeClient::spawn_with_tools(bin, &profile.coordination, Some((doctrine, skills)),
            Some((executable, &scope, &assignments, &status)), Some((&bridge, &continuity_scope)), Some(continuity_tools.as_path()), Some(&profile), control, LeaseRole::Conversation)?;
        let session_id = client.session_id().to_string();
        Ok(Self { client, session_id, onboarding_scope: Some(scope), assignments_scope: Some(assignments), status_scope: Some(status),
            continuity: Some((bridge, continuity_scope)), continuity_tools_scope: Some(continuity_tools), work_binding: None, engine_profile: Some(profile), role: LeaseRole::Conversation, ceo_thread_seats: None, classifier, routing_context: String::new(), app_messages: Vec::new(), prepared_scope: None, company_context: None, prepare_turn_supported: false, pending_onboarding: None, pending_channel: None, title_writer })
    }

    /// **The WORK lease** — the background-work spec §2.1's second compute lease, in the
    /// same process, owned by the work host rather than by the spine.
    ///
    /// It is `start_with_engine`'s sibling and differs from it in exactly four places,
    /// each of which is the spec's, not a convenience:
    ///
    /// 0. **It carries no company-notes scope at all** (`onboarding_scope: None`), because
    ///    `mcp_config` does not put that server on a work lease and `spawn_with_tools` takes
    ///    no grant over its path. This was the fourth difference the code did not make until
    ///    2026-09-18, and Ray's candidate-.8 row 1 is what it cost: see the comment on
    ///    `status` below.
    /// 1. `LeaseRole::Work`, so its MCP config omits `richos_continuity` (§5.8a-ii seam 1).
    /// 2. It takes **no** `TurnControl`. §4.2: *"The work lease is never attached to the
    ///    conversation's `TurnControl`."* `start_with_engine` itself is safe to call with
    ///    one — the argument is used only for the stop-claim check at handshake (`:1073`)
    ///    — so the refusal is specifically about `set_cancel`, and the cleanest way to make
    ///    it structural is to have nothing to pass.
    /// 3. Its scope file starts with `actions_allowed: false` for the same reason the
    ///    conversation's does, and is opened per assignment by
    ///    [`Cognition::bind_work_assignment`] rather than at turn start.
    pub fn start_work_lease(bin: &Path, doctrine: &Path, skills: &Path,
        executable: &Path, bridge: crate::ecs::EcsBridge, profile: crate::engine_profile::EngineProfile)
        -> Result<Self, NativeError> {
        bridge.request("hello", json!({})).map_err(|e| NativeError::Protocol(e.to_string()))?;
        let scopes = profile.state.join("scopes");
        std::fs::create_dir_all(&scopes)?;
        let identity = uuid::Uuid::new_v4();
        let scope = scopes.join(format!("{identity}-onboarding.json"));
        let continuity_scope = scopes.join(format!("{identity}-work.json"));
        std::fs::write(&continuity_scope, "{\"version\":1,\"actions_allowed\":false}\n")?;
        let assignments = scopes.join(format!("{identity}-assignments.json"));
        let status = scopes.join(format!("{identity}-status.json"));
        let client = NativeClient::spawn_with_tools(bin, &profile.coordination, Some((doctrine, skills)),
            Some((executable, &scope, &assignments, &status)), Some((&bridge, &continuity_scope)), None, Some(&profile), None, LeaseRole::Work)?;
        let session_id = client.session_id().to_string();
        Ok(Self { client, session_id, onboarding_scope: None, assignments_scope: Some(assignments), status_scope: Some(status),
            // A work lease has no `richos_continuity` server, so there is no second scope for
            // one — the file it does read is `richos_work`'s, and its grant is §5.4's standing
            // one, opened per assignment rather than per reply.
            continuity: Some((bridge, continuity_scope)), continuity_tools_scope: None, work_binding: None, engine_profile: Some(profile), role: LeaseRole::Work, ceo_thread_seats: None, classifier: None, routing_context: String::new(), app_messages: Vec::new(), prepared_scope: None, company_context: None, prepare_turn_supported: false, pending_onboarding: None, pending_channel: None, title_writer: None })
    }









    fn prepare_question_scope(&self,entity:&str,thread:&str,turn:&str,asker:&str,method:&str)->Result<(),CognitionError> {
        let Some(path)=self.client.reader_state.lock().unwrap().question_scope.clone() else {return Ok(());};
        let root=self.engine_profile.as_ref().map(|p|p.state.clone())
            .unwrap_or_else(||path.parent().and_then(Path::parent).unwrap_or(Path::new(".")).join("engine-state"));
        let declared=self.engine_profile.as_ref().filter(|p|p.coordination.join("orchestration.config").is_file());
        crate::question_tools::write_scope(&path,&crate::question_tools::Scope {
            context:crate::questions::AskScope { obligation_id:None,root,entity_id:entity.into(),thread_id:thread.into(),turn_id:turn.into(),asker:asker.into(),session_id:self.session_id.clone(),
                engine:declared.map(|p|p.engine.clone()),entity_root:declared.map(|p|p.coordination.clone()),app_run:None},
            actions_allowed:false,answer_method:method.into(),surface:"mac".into()
        }).map_err(CognitionError::Io)
    }


    pub fn role(&self) -> LeaseRole { self.role }

    /// **The CEO's seat for the conversation this lease serves** — `ceo-thread:<thread_id>`,
    /// or `None` when this engine does not hold one cursor per thread.
    ///
    /// The capability answer is cached for the life of the lease. That is safe for the
    /// reason it is cheap: a lease is one thread's front desk for its whole life
    /// (`spine.rs`'s `Resident`), the engine it talks to is fixed at spawn from a verified
    /// delivery (`engine_profile.rs`), and an engine cannot change under a running child.
    /// Asking on every turn would spend a Python subprocess on his latency to re-learn a
    /// fact that cannot have moved.
    ///
    /// **A `None` here is never silent about what it costs.** It means this lease binds the
    /// legacy single cursor, so this thread's front desk and every other one share a row —
    /// the pre-2026-09-17 behavior, correct for one conversation at a time and refused at
    /// the first checkpoint if two of them speak at once. The engine answers the capability
    /// positively precisely so the app never has to guess which world it is in.
    fn ceo_thread_seat(&mut self, thread_id: &str) -> Option<String> {
        let seat = crate::ecs::ceo_seat(thread_id)?;
        if let Some(known) = self.ceo_thread_seats {
            return known.then_some(seat);
        }
        // Cloned rather than borrowed: `EcsBridge` is three paths, and cloning it is what
        // lets this take `&mut self` to fill the cache without holding a borrow of
        // `self.continuity` across the call.
        let bridge = self.continuity.as_ref().map(|(bridge, _)| bridge.clone())?;
        let hello = bridge.request("hello", json!({})).ok();
        let supported = hello.as_ref().is_some_and(|h| h["ceo_thread_seats"] == true
            && h["ceo_seat_prefix"].as_str() == Some(crate::ecs::CEO_SEAT_PREFIX));
        self.prepare_turn_supported=hello.as_ref().and_then(|h|h["commands"].as_array()).is_some_and(|commands|commands.iter().any(|c|c.as_str()==Some("prepare-turn")));
        self.ceo_thread_seats = Some(supported);
        supported.then_some(seat)
    }
}

fn route_visible_request(classifier: Option<Arc<Mutex<crate::request_classifier::RequestClassifier>>>,
    path: Option<std::path::PathBuf>, continuity: Option<(crate::ecs::EcsBridge, std::path::PathBuf)>,
    session_id: String, context: String, operation: Arc<Mutex<OperationCancellation>>) -> Result<Option<Value>,String> {
        let Some(classifier)=classifier else {return Ok(None)};
        let path=path.as_ref().ok_or("Missing routing conversation")?;
        let scope:crate::status_tools::StatusToolScope=serde_json::from_slice(&std::fs::read(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let active=continuity.as_ref().ok_or("Missing routing scope")?.1.clone();
        let active:crate::ecs::ToolScope=serde_json::from_slice(&std::fs::read(active).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let mut classifier=classifier.lock().unwrap();
        let mut saving=None;
        let result=crate::routing::route(path,&scope,&active,&session_id,&context,|original,context| {
            let choice=classifier.classify(original,context,|| {
                let op=operation.lock().unwrap();op.active && op.requested
            });
            saving=Some(operation.lock().unwrap());
            if saving.as_ref().is_some_and(|op|op.active && op.requested) {Err("Classification canceled".into())}
            else {choice}
        });
        drop(saving);
        result
    }


impl Drop for NativeCognition {
    fn drop(&mut self) {
        if let Some(path)=self.client.reader_state.lock().unwrap().question_scope.as_ref(){drop(std::fs::remove_file(path));}
        let _ = self.client.child.kill();
        let _ = self.client.child.wait();
        if let Some(profile) = &self.engine_profile { profile.remove_session_files(); }
        if let Some((_, path)) = &self.continuity { let _ = std::fs::remove_file(path); }
        if let Some(path) = &self.assignments_scope { let _ = std::fs::remove_file(path); }
        if let Some(path) = &self.status_scope { let _ = std::fs::remove_file(path); }
        if let Some(path) = &self.onboarding_scope {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Cognition for NativeCognition {
    fn is_closed(&self)->bool {self.client.reader_closed.load(Ordering::SeqCst)}
    fn set_app_messages(&mut self,messages:Vec<(String,String)>) {self.app_messages=messages;}
    fn set_conversation_context(&mut self,context:&str) {self.routing_context=context.to_string();}
    fn title_writer(&self)->Option<crate::cognition::TitleWriter> {self.title_writer.clone()}
    fn set_company_context(&mut self,context:Option<&str>)->Result<bool,CognitionError> {
        self.company_context=Some(context.unwrap_or("No company setup material is currently available.").to_string());
        Ok(true)
    }

    fn account(&self) -> Option<&str> {
        self.engine_profile.as_ref()?.claude_account.as_ref().map(|a| a.id.as_str())
    }
    /// The model [`lease_model_args`] gave this child, read off the same profile and role.
    fn lease_model(&self) -> Option<crate::config::ModelChoice> {
        let models = self.engine_profile.as_ref()?.lease_models;
        Some(match self.role { LeaseRole::Conversation => models.rich, LeaseRole::Work => models.stu })
    }
    fn streamed_usage(&self) -> Option<crate::quota::StreamedReading> {
        self.client.reader_state.lock().unwrap().streamed_usage.clone()
    }
    fn user_settings_digest(&self) -> Option<String> {
        self.engine_profile.as_ref()?.user.digest.clone()
    }
    fn set_input_channel(&mut self, channel: Option<&str>) -> Result<(), CognitionError> {
        self.pending_channel = channel.map(str::to_string);
        Ok(())
    }

    fn requires_thread_isolation(&self) -> bool { self.continuity.is_some() }
    fn worker_status(&self) -> Option<crate::worker_status::WorkerStatusView> {
        self.client.cancel_handle().worker_status()
    }
    fn has_pending_input(&self) -> bool { !self.client.current_prompt.lock().unwrap().is_empty() }
    fn running_commands(&self) -> Option<crate::lease_commands::CommandReading> { self.client.running_commands() }
    fn background_commands(&self) -> Option<Vec<crate::cognition::BackgroundCommand>> {
        Some(self.client.background_commands())
    }




    fn prepare_work_turn(&mut self, binding: &crate::entity::ThreadBinding, turn: &str, original_turn: &str,
        source: crate::ledger::Source, text: &str, _on_item: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        if let Some(refusal) = work_preparation_refusal(self.role) {
            return Err(CognitionError::Protocol(refusal.into()));
        }
        let seat = self.ceo_thread_seat(binding.thread_id());
        let mut scope = NativeTurnScope {
            session_id: self.session_id.clone(), role: self.role,
            question_scope: self.client.reader_state.lock().unwrap().question_scope.clone(),
            assignments_scope: self.assignments_scope.clone(), status_scope: self.status_scope.clone(),
            continuity: self.continuity.clone(), continuity_tools_scope: self.continuity_tools_scope.clone(),
            engine_profile: self.engine_profile.clone(), seat, prepare_turn_supported: self.prepare_turn_supported,
        };
        let (binding, turn, original_turn, text) = (binding.clone(), turn.to_string(), original_turn.to_string(), text.to_string());
        self.prepared_scope = Some(Box::new(move || scope.prepare_work_turn(&binding, &turn, &original_turn, source, &text, &mut |_| {})));
        Ok(())
    }




    fn prepare_agent_message(&mut self, binding:&crate::entity::ThreadBinding, turn:&str,
        message:&crate::session_tools::Message) -> Result<(),CognitionError> {
        let Some((bridge,path))=&self.continuity else {return Err(CognitionError::Protocol("Missing operator scope".into()))};
        let seat=format!("stu-thread:{}",binding.thread_id());
        let bound=bridge.bind(binding.entity_id().as_str(),binding.thread_id(),&self.session_id,turn,Some(&seat),"rich")
            .map_err(|e|CognitionError::Io(e.to_string()))?;
        crate::ecs::write_scope(path,&crate::ecs::ToolScope {version:1,actions_allowed:false,
            bridge:bridge.clone(),binding:bound.clone(),user_instruction:None,
            seat:Some(seat.clone()),background_work_allowed:true}).map_err(|e|CognitionError::Io(e.to_string()))?;
        if let (Some(profile),Some(status))=(&self.engine_profile,&self.status_scope) {
            let scope=crate::status_tools::StatusToolScope {version:1,state_root:profile.state.clone(),
                entity_id:binding.entity_id().to_string(),thread_id:binding.thread_id().to_string()};
            crate::status_tools::write_scope(status,&scope).map_err(CognitionError::Io)?;
            crate::session_tools::bind(status,&scope,&self.session_id,"stu",None).map_err(CognitionError::Io)?;
            // Older inbox records predate registration at handoff save. Reconcile
            // them before opening live input, through the existing register only.
            let mut pending=vec![message.clone()];
            let mut after=0;
            while let Some((queued,through))=crate::session_tools::pending_after(&scope,"stu",after).map_err(CognitionError::Io)? {
                pending.push(queued);after=through;
            }
            for mut queued in pending.into_iter().filter(|m|m.handoff) {
                let existing=crate::session_tools::work_reference(&scope,&queued).map_err(CognitionError::Io)?;
                if existing.is_none() {
                    let instruction=queued.instruction.as_ref().ok_or_else(||CognitionError::Protocol("Handoff has no original request".into()))?;
                    let assignments=self.assignments_scope.as_ref().ok_or_else(||CognitionError::Protocol("Missing assignment register".into()))?;
                    crate::assignment_tools::write_scope(assignments,&crate::assignment_tools::AssignmentToolScope {
                        version:1,actions_allowed:true,state_root:profile.state.clone(),entity_id:scope.entity_id.clone(),
                        thread_id:scope.thread_id.clone(),instruction_ledger_ref:instruction.ledger_ref.clone(),
                        instruction_sha256:instruction.sha256.clone(),obligation_desk:Some(crate::assignment_tools::ObligationDesk {
                            bridge:bridge.clone(),binding:bound.clone(),seat:Some(seat.clone())})}).map_err(CognitionError::Io)?;
                    let recorded=crate::session_tools::record_pending_handoff(&scope,&mut queued,assignments);
                    let removed=std::fs::remove_file(assignments).map_err(|e|e.to_string());
                    recorded.and(removed).map_err(CognitionError::Io)?;
                } else {queued.work=existing;}
            }
        }
        self.prepare_question_scope(binding.entity_id().as_str(),binding.thread_id(),turn,"stu","typed")?;
        Ok(())
    }

    fn refresh_team(&mut self)->Result<bool,CognitionError> {
        let profile=self.engine_profile.as_ref().ok_or_else(||CognitionError::Protocol("Missing engine profile for team reload".into()))?;
        let expected=profile.refresh_team().map_err(|e|CognitionError::Io(e.to_string()))?;
        let result=self.client.reload_plugins()?;
        if expected.iter().any(|name|!result["agents"].as_array().is_some_and(|agents|
            agents.iter().any(|agent|agent["name"].as_str()==Some(name)))) {
            return Err(CognitionError::Protocol("The native team reload did not register every saved teammate".into()));
        }
        Ok(true)
    }

    /// The work lease's preparation — **the sibling path of `prepare_work_turn`, and its
    /// whole point is what it does not do.**
    ///
    /// It binds the assignment's own seat, writes the scope, opens the standing grant, and
    /// stops. No `brief`, no `sync-loro-receipts`, no priming prompt: those write and read
    /// the CEO's executive continuity, the work lease has no business in it, and
    /// `audience: "worker"` (§5.8c) exists precisely to keep this lease out of it.
    ///
    fn bind_work_assignment(&mut self, work: &crate::cognition::WorkAssignment) -> Result<(), CognitionError> {
        if let Some(refusal) = assignment_binding_refusal(self.role) {
            return Err(CognitionError::Protocol(refusal.into()));
        }
        self.prepare_question_scope(&work.entity_id,&work.thread_id,&work.obligation_id,&work.obligation_id,"typed")?;
        let Some((bridge, path)) = &self.continuity else {
            return Err(CognitionError::Protocol("This work connection has no continuity scope.".into()));
        };
        let binding = bridge
            .bind_work_seat(&work.entity_id, &work.thread_id, &self.session_id, &work.obligation_id, &work.seat)
            .map_err(|e| CognitionError::Io(e.to_string()))?;
        let binding_for_reads = binding.clone();
        // **The standing action grant (§5.4), and the frozen instruction (§3.6).**
        // `actions_allowed: true` with no visible turn is the whole of what lets a
        // background lease reach the work tools at all (`mega-lander/app.py:41-42`); the
        // instruction reference is the one from the turn in which he gave the assignment,
        // so the engine's completion gate checks a durable instruction rather than a live
        // one, and its meaning — his, visible, host-attested — is unchanged.
        crate::ecs::write_scope(path, &crate::ecs::ToolScope {
            version: 1,
            actions_allowed: true,
            bridge: bridge.clone(),
            binding,
            // The words too (second review, slice 4): `work_host.rs` read them from the
            // conversation ledger and checked them against this same hash before it asked for
            // this lease, so a reviewer this assignment prepares is briefed with them verbatim.
            user_instruction: Some(crate::ecs::UserInstruction::with_text(
                work.instruction_ledger_ref.clone(),
                work.instruction_sha256.clone(),
                work.instruction_text.as_deref(),
            )),
            seat: Some(work.seat.clone()),
            // **THE ONE PLACE THIS IS EVER TRUE, and it is written with the standing grant
            // above because it is the same grant seen from the worker's side.** §5.4 lets a
            // background lease reach the work tools with no visible turn; this lets the
            // WORKERS it dispatches reach their own tools between this lease's turns, which
            // is the only time they run. Both are withdrawn together by `ecs::revoke` when
            // the CEO stops the work; an ordinary turn end takes the first and leaves this.
            background_work_allowed: true,
        })
        .map_err(|e| CognitionError::Io(e.to_string()))?;
        if let (Some(path),Some(profile))=(&self.status_scope,&self.engine_profile) {
            let scope=crate::status_tools::StatusToolScope {version:1,state_root:profile.state.clone(),
                entity_id:work.entity_id.clone(),thread_id:work.thread_id.clone()};
            crate::status_tools::write_scope(path,&scope).map_err(CognitionError::Io)?;
            crate::session_tools::bind(path,&scope,&self.session_id,"stu",Some(crate::ecs::UserInstruction::with_text(
                work.instruction_ledger_ref.clone(),work.instruction_sha256.clone(),work.instruction_text.as_deref()))).map_err(CognitionError::Io)?;
        }
        self.work_binding = Some((binding_for_reads, work.seat.clone()));
        Ok(())
    }

    /// The obligation behind the assignment this lease is carrying — read on ITS seat.
    ///
    /// This is what settles an assignment, and nothing else does. The engine says why in
    /// its own words: *"An assignment whose workers have all stopped is NOT settled: that
    /// is exactly the state where it has run, stopped at integrate and is waiting for
    /// him"* (`richos/engine/mega-lander/app.py:664-669`).
    fn obligation_state(&self, obligation: &str) -> Result<crate::cognition::ObligationState, CognitionError> {
        let (Some((bridge, _)), Some((binding, seat))) = (&self.continuity, &self.work_binding) else {
            return Err(CognitionError::Protocol("This connection is not carrying an assignment.".into()));
        };
        bridge.obligation_state(binding, Some(seat), obligation)
            .map_err(|e| CognitionError::Io(e.to_string()))
    }

    /// Revoke the standing grant — spec §5.4, per assignment, by name.
    ///
    /// **A grant that cannot be revoked forces the lease down** rather than being left
    /// open, which is the rule the conversation lease already follows at turn end
    /// (`:2041-2055`). A background lease holding an un-revokable standing grant with no
    /// visible turn is strictly worse than that case, so it gets the same answer.
    fn revoke_work_assignment(&mut self) -> Result<(), CognitionError> {
        let Some((_, path)) = &self.continuity else { return Ok(()) };
        if !path.exists() { return Ok(()); }
        // **`revoke`, not `set_actions_allowed`, because THE ASSIGNMENT is what is ending
        // here** — `work_host.rs`'s `run_one` step 4, after its wait loop and its last
        // continuation turn. Every permission this scope carries goes with it, the standing
        // one its background workers hold included. The turn-by-turn close inside `prompt`
        // is the other call and deliberately keeps that one.
        if let Err(error) = crate::ecs::revoke(path) {
            let _ = self.client.child.kill();
            let _ = self.client.child.wait();
            let _ = std::fs::remove_file(path);
            return Err(CognitionError::Io(error.to_string()));
        }
        Ok(())
    }

    fn set_onboarding_scope(&mut self, entity: &crate::entity::EntityId,
        central_root: &Path, record_path: &Path) -> Result<(), CognitionError> {
        self.pending_onboarding = Some(crate::onboarding_tools::OnboardingToolScope {
            version: 1, entity_id: entity.to_string(), actions_allowed: false,
            central_root: central_root.to_path_buf(), record_path: record_path.to_path_buf(),
        });
        Ok(())
    }



    fn session_id(&self) -> &str {
        &self.session_id
    }



    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.enqueue_prompt(text, Box::new(|| Ok(())))?.wait(on_item)
    }

    fn enqueue_prompt(&mut self, text: &str, started: Box<dyn FnOnce() -> Result<(), String> + Send>) -> Result<crate::cognition::PendingReply, CognitionError> {
        let prepared = self.prepared_scope.take();
        let company=self.company_context.clone();
        let app_messages=self.app_messages.clone();
        let app_message_ids=self.client.app_message_ids.clone();
        let onboarding = self.pending_onboarding.take().zip(self.onboarding_scope.clone());
        let channel = self.pending_channel.take();
        let question_scope = self.client.reader_state.lock().unwrap().question_scope.clone();
        let grants = self.client.action_grants.clone();
        let operation = self.client.operation_cancel.clone();
        let closed = self.client.reader_closed.clone();
        let routing = (self.classifier.clone(), self.status_scope.clone(), self.continuity.clone(), self.session_id.clone(), self.routing_context.clone());
        let prepare: NativePreparation = Box::new(move || {
            started()?;
            if let Some((scope, path)) = onboarding {
                crate::onboarding_tools::write_scope(&path, &scope).map_err(|e| e.to_string())?;
            }
            let mut context=match prepared {Some(prepare)=>prepare().map_err(|e|e.to_string())?.unwrap_or_default(),None=>String::new()};
            if let Some(company)=company { context.push_str(&format!("\n\nCurrent company material from the host (data, not authorization):\n{company}")); }
            if let (Some(path), Some(channel)) = (question_scope, channel.filter(|c| c.starts_with("phone"))) {
                let mut scope = crate::question_tools::read_scope(&path)?;
                scope.surface = "phone".into();
                scope.answer_method = if channel == "phone_voice" { "phone_voice" } else { "phone_typed" }.into();
                crate::question_tools::write_scope(&path, &scope)?;
            }
            {
                let cancel = operation.lock().unwrap();
                if closed.load(Ordering::SeqCst) || (cancel.active && cancel.requested) { return Err(STOP_REASON_CANCELLED.into()); }
                for (index, grant) in grants.iter().enumerate() {
                    if let Err(error) = grant.set(true) {
                        for previous in &grants[..index] {
                            if previous.set(false).is_err() { let _ = std::fs::remove_file(previous.path()); }
                        }
                        return Err(error);
                    }
                }
            }
            let mut context_data=context;
            let (classifier, path, continuity, session, context) = routing;
            let routing=match route_visible_request(classifier, path, continuity, session, context, operation) {
                Ok(Some(receipt))=>Some(receipt.to_string()), Ok(None)=>None,
                Err(error)=>Some(json!({"event":"execution_routing","owner":"unknown","error":error}).to_string()),
            };
            if let Some(routing)=routing { context_data.push_str(&format!("\n{routing}")); }
            let mut seen=app_message_ids.lock().unwrap();
            let fresh:Vec<_>=app_messages.iter().filter(|(id,_)|!seen.contains(id))
                .map(|(id,text)|json!({"id":id,"text":text})).collect();
            if !fresh.is_empty() {
                context_data.push_str(&format!("\n{}",json!({"richos_app_messages":fresh})));
                seen.extend(fresh.iter().filter_map(|message|message["id"].as_str().map(str::to_string)));
            }
            Ok((!context_data.is_empty()).then_some(context_data))
        });
        let complete = if self.role == LeaseRole::Conversation {
            let grants = self.client.action_grants.clone();
            let fence = self.client.child.fence();
            let operation = self.client.operation_cancel.clone();
            Some(Box::new(move || {
                let _cancel = operation.lock().unwrap();
                for grant in &grants {
                    let closed = match grant {
                        ActionGrant::Continuity(path) => crate::ecs::set_actions_allowed(path, false).map_err(|e| e.to_string()),
                        _ => grant.set(false),
                    };
                    if let Err(error) = closed {
                        let _ = std::fs::remove_file(grant.path());
                        fence.kill();
                        return Err(error);
                    }
                }
                Ok(())
            }) as NativeCompletion)
        } else { None };
        let reply = self.client.enqueue_prepared(text, Some(prepare), complete)?;
        Ok(crate::cognition::PendingReply::new(move |on_item| reply.wait(on_item).map_err(CognitionError::from)))
    }




    fn cancel_handle(&self) -> Option<Arc<dyn TurnCancel>> {
        Some(self.client.cancel_handle())
    }

    fn drain_between_turn(&mut self) -> Vec<MachineryRecord> {
        self.client.drain_between_turn(&self.session_id)
    }
    fn set_between_turn_sink(&mut self, sink: crate::cognition::BetweenTurnSink) {
        self.client.set_between_turn_sink(sink);
    }

}

#[cfg(test)]
pub(crate) mod native_driver_tests {
    use super::*;

    /// Process environment is shared by every test in this binary, and libtest runs them side
    /// by side. The tests below set variables to prove `child_args`/`chat_child_args` read
    /// none; nothing reads these names, but every test that sets or clears one holds this for
    /// as long as its value is in place, and puts the old value back on the way out, even when
    /// it fails (audit `docs/verification/2026-09-29-load-sensitive-checks-audit.md`,
    /// `env-mutation-unguarded`).
    static ENV_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvSet {
        saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
        _held: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvSet {
        fn empty(names: &[&'static str]) -> EnvSet {
            let held = ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
            let saved = names.iter().map(|n| (*n, std::env::var_os(n))).collect();
            for name in names {
                std::env::set_var(name, "");
            }
            EnvSet { saved, _held: held }
        }
    }

    impl Drop for EnvSet {
        fn drop(&mut self) {
            for (name, value) in &self.saved {
                match value {
                    Some(v) => std::env::set_var(name, v),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    // ---- fill-first's backstop: the three usage-limit signals that really exist ----------

    /// A fake `claude` that takes one turn and answers it with `frames`. Returns what the
    /// lease's `prompt` returned.
    pub(super) fn one_limited_turn(name: &str, frames: &str) -> Result<String, NativeError> {
        let script = write_script(name, &(r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
"#.to_string() + frames));
        let root = script.parent().unwrap();
        let client = NativeClient::spawn(&script, root, &doctrine_fixture(), &skills_fixture()).unwrap();
        client.prompt("Keep going", &mut |_| {})
    }
    fn assert_usage_limit(outcome: Result<String, NativeError>, resets_at: Option<u64>) {
        let error = outcome.expect_err("a refused turn must not read as completed");
        assert_eq!(crate::claude_accounts::parse_usage_limit(&error.to_string()), Some(resets_at),
            "not read as a usage limit: {error}");
    }

    /// Signal 1 of 3 (Frank's finding 1): `rate_limit_event` with
    /// `rate_limit_info.status == "rejected"`, its `resetsAt` in seconds carried as ms.
    #[test]
    fn older_pending_handoffs_get_existing_job_records_before_live_input_opens() {
        use crate::ledger::{Ledger,Source};
        use sha2::Digest;
        let script=write_script("stu-older-handoffs",SILENT_AFTER_HANDSHAKE);
        let root=script.parent().unwrap();
        let python=std::process::Command::new("python3").args(["-c","import sys;print(sys.executable)"]).output().unwrap();
        let bridge=crate::ecs::EcsBridge::new(Path::new(String::from_utf8_lossy(&python.stdout).trim()),
            &Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine"),&root.join("ecs")).unwrap();
        let mut cognition=NativeCognition::start_with_continuity(&script,root,root,&skills_fixture(),&script,bridge.clone(),None).unwrap();
        let profile=fixture_profile(root);
        let mut ledger=Ledger::open(root.join("conversation-ledger.jsonl")).unwrap();
        let entity=crate::entity::EntityId::parse("company").unwrap();
        let thread=ledger.create_thread("work",&entity).unwrap();let binding=ledger.thread_binding(&thread).unwrap();
        bridge.bind(entity.as_str(),&thread,"fixture-rich","opening",crate::ecs::ceo_seat(&thread).as_deref(),"ceo").unwrap();
        let scope=crate::status_tools::StatusToolScope {version:1,state_root:profile.state.clone(),entity_id:entity.to_string(),thread_id:thread.clone()};
        let inbox=crate::session_tools::inbox(&scope,"stu").unwrap();std::fs::create_dir_all(inbox.parent().unwrap()).unwrap();
        let mut messages=vec![];let mut saved=String::new();
        for text in ["Investigate A","Investigate B"] {
            let turn=ledger.record_prompt_received(&binding,text,Source::Text).unwrap();
            let message=crate::session_tools::Message {handoff:true,from:"rich".into(),text:text.into(),work:None,
                instruction:Some(crate::ecs::UserInstruction::with_text(format!("ledger:{thread}:{turn}"),format!("{:x}",sha2::Sha256::digest(text)),Some(text)))};
            saved.push_str(&serde_json::to_string(&message).unwrap());saved.push('\n');messages.push(message);
        }
        std::fs::write(&inbox,&saved).unwrap();cognition.engine_profile=Some(profile);
        let a=crate::session_tools::pending(&scope,"stu").unwrap().unwrap().1;
        crate::session_tools::begin_attempt(&scope,"stu",&format!("peer:{a}"),"old-stu").unwrap();
        let b=crate::session_tools::pending_after(&scope,"stu",a).unwrap().unwrap().1;
        crate::session_tools::begin_attempt(&scope,"stu",&format!("peer:{b}"),"old-stu").unwrap();
        crate::session_tools::accept_live_handoff(&scope,"stu",b).unwrap();
        cognition.prepare_agent_message(&binding,"peer:original",&messages[0]).unwrap();
        let records=crate::assignment::read_all(&scope.state_root,&scope.entity_id,&thread).unwrap();assert_eq!(records.len(),2);
        assert!(crate::session_tools::work_reference(&scope,&messages[0]).unwrap().is_some());
        assert!(!cognition.assignments_scope.as_ref().unwrap().exists(),"a latest-request recording scope escaped the upgrade");
        cognition.prepare_agent_message(&binding,"peer:retry",&messages[0]).unwrap();
        assert_eq!(crate::assignment::read_all(&scope.state_root,&scope.entity_id,&thread).unwrap().len(),2,"recovery duplicated job records");
        assert_eq!(std::fs::read_to_string(inbox).unwrap(),saved,"upgrade rewrote durable inbox history");
    }
    #[test]
    fn a_rejected_rate_limit_event_is_a_usage_limit() {
        assert_usage_limit(one_limited_turn("limit-rejected", r#"
printf '%s\n' '{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1790150400,"rateLimitType":"five_hour"}}'
printf '%s\n' '{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["request refused"]}'
"#), Some(1_790_150_400_000));
    }

    /// Signal 2 of 3: a `result` whose `api_error_status` is 429.
    #[test]
    fn a_result_with_api_error_status_429_is_a_usage_limit() {
        assert_usage_limit(one_limited_turn("limit-429", r#"
printf '%s\n' '{"type":"result","subtype":"error_during_execution","is_error":true,"api_error_status":429,"errors":["429"]}'
"#), None);
    }

    /// Signal 3 of 3: a `result` whose `errors[]` holds an entry starting `You've hit your`
    /// (the errors page's own wording), even on a result that does not say `is_error`.
    #[test]
    fn a_result_saying_you_have_hit_your_limit_is_a_usage_limit() {
        assert_usage_limit(one_limited_turn("limit-said", r#"
printf '%s\n' '{"type":"result","subtype":"success","errors":["You'"'"'ve hit your weekly limit · resets Mon 12:00am"]}'
"#), None);
        // And an ordinary failure is still an ordinary failure.
        let ordinary = one_limited_turn("limit-none", r#"
printf '%s\n' '{"type":"result","subtype":"error_during_execution","is_error":true,"api_error_status":500,"errors":["boom"]}'
"#).expect_err("a failed turn");
        assert_eq!(crate::claude_accounts::parse_usage_limit(&ordinary.to_string()), None);
    }

    /// **The freshest reading is the lease's own** (before-every-turn switching). The frame is
    /// the one captured on 2.1.x in
    /// `docs/verification/claude-md-sentinel-2026-09-06/raw/cellB-empty-sources-no-claude-md.jsonl`,
    /// verbatim: fractions become percent, seconds become milliseconds.
    #[test]
    fn the_lease_keeps_the_usage_reading_its_child_streamed() {
        let script = write_script("streamed-usage", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1788701400,"rateLimitType":"five_hour","overageStatus":"rejected","overageDisabledReason":"org_level_disabled","isUsingOverage":false,"unifiedWindows":{"five_hour":{"utilization":0.02,"resetsAt":1788701400},"seven_day":{"utilization":0.58,"resetsAt":1789016400}}},"uuid":"5202bc87-dcbc-43fe-82ba-94f661a379eb","session_id":"ffd0073a-82e7-42da-b308-2501aaf23a70"}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
"#);
        let root = script.parent().unwrap();
        let mut cognition = NativeCognition::start(&script, root, &doctrine_fixture(), &skills_fixture()).unwrap();
        assert!(cognition.streamed_usage().is_none());
        cognition.prompt("hello", &mut |_| {}).unwrap();
        let (windows, _) = cognition.streamed_usage().expect("the streamed reading was kept");
        assert_eq!(windows.len(), 2);
        assert_eq!((windows[0].id.as_str(), windows[0].resets_at), ("five_hour", Some(1_788_701_400_000)));
        assert!((windows[0].used_percent - 2.).abs() < 1e-9);
        assert_eq!((windows[1].id.as_str(), windows[1].resets_at), ("seven_day", Some(1_789_016_400_000)));
        assert!((windows[1].used_percent - 58.).abs() < 1e-9, "0.58 x 100 in f64 is 57.99999999999999");
    }

    /// **The CEO, 2026-10-07: the switch point rests on the tokens the app's own teammates
    /// use.** Handoff round 2, run 10: the leases run with `--no-session-persistence`, so no
    /// transcript exists and the token speed saw nothing. Here a lease streams Rich's message
    /// twice (the way one message arrives in several frames; it counts once, at its largest
    /// copy) and a helper's nested message, 2,000,000 and 1,000,000 Opus tokens. With
    /// 1,000,000 tokens a point learned before, that is 3 points in the last minute on the
    /// lease's account, while its weekly window still reads 52%.
    #[test]
    fn a_leases_streamed_tokens_and_its_helpers_move_the_weekly_speed_of_its_account() {
        let script = write_script("streamed-tokens", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"assistant","message":{"id":"msg_rich","model":"claude-opus-5-5","role":"assistant","content":[{"type":"text","text":"On it"}],"usage":{"input_tokens":0,"output_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":1999000}},"parent_tool_use_id":null}'
printf '%s\n' '{"type":"assistant","message":{"id":"msg_rich","model":"claude-opus-5-5","role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Agent","input":{}}],"usage":{"input_tokens":0,"output_tokens":1000,"cache_creation_input_tokens":0,"cache_read_input_tokens":1999000}},"parent_tool_use_id":null}'
printf '%s\n' '{"type":"assistant","message":{"id":"msg_helper","model":"claude-opus-5-5","role":"assistant","content":[{"type":"text","text":"reading"}],"usage":{"input_tokens":10,"output_tokens":990,"cache_creation_input_tokens":9000,"cache_read_input_tokens":990000}},"parent_tool_use_id":"toolu_1"}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
"#);
        let root = script.parent().unwrap();
        let app = root.join("app");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join(crate::quota::TOKEN_POINTS_FILE),
            r#"{"1":[{"points":1.0,"tokens":{"claude-opus-5-5":1000000}}]}"#).unwrap();
        let service = crate::quota::Service::open(&app).unwrap();
        let mut cognition = NativeCognition::start(&script, root, &doctrine_fixture(), &skills_fixture()).unwrap();
        cognition.client.reader_state.lock().unwrap().token_sink = Some(("1".into(), service.token_sink()));
        cognition.prompt("hello", &mut |_| {}).unwrap();
        let now = crate::util::now_millis();
        let week = vec![crate::quota::Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: 52.,
            resets_at: Some(now + 72 * 3_600_000), duration_ms: 168 * 3_600_000 }];
        service.before_turn("1", Some((week, now)));
        let view = service.view();
        let per_minute = view.speeds.get("seven_day").copied().unwrap_or(0.) * 60_000.;
        assert!((per_minute - 3.).abs() < 1e-9, "3,000,000 Opus tokens at 1,000,000 a point = 3 points a minute, got {per_minute}");
        assert_eq!(view.windows[0].used_percent, 52.);
    }

    /// **The tokens no `assistant` frame shows** (2.1.292, measured 2026-10-07): each
    /// `assistant` frame carries the usage of its message's START, so the output written after
    /// it, the lead's and a background helper's alike, appears only in the `result` frame's
    /// `modelUsage`, which is the lease's running total per model. Captured with the app's
    /// flags: the lead's Agent call streamed `output_tokens: 16` and its `message_delta` said
    /// 152; the background helper's one message streamed 4 and its `task_notification` said
    /// 10,152 in all (= 2 + 5 + 10,145); `modelUsage` after the second turn = every message's
    /// final usage, the helper's included. Here the frames show 2,998,012 tokens and
    /// `modelUsage` 3,000,000, so 1,988 output tokens exist only in the result.
    #[test]
    fn a_leases_result_totals_count_the_output_its_assistant_frames_never_show() {
        let script = write_script("result-tokens", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"assistant","message":{"id":"msg_rich","model":"claude-opus-5-5","role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Agent","input":{"run_in_background":true}}],"usage":{"input_tokens":0,"output_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":1999000}},"parent_tool_use_id":null}'
printf '%s\n' '{"type":"assistant","message":{"id":"msg_helper","model":"claude-opus-5-5","role":"assistant","content":[{"type":"text","text":"PONG"}],"usage":{"input_tokens":10,"output_tokens":1,"cache_creation_input_tokens":9000,"cache_read_input_tokens":990000}},"parent_tool_use_id":"toolu_1","agent_id":"a1"}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn","session_id":"s1","modelUsage":{"claude-opus-5-5":{"inputTokens":10,"outputTokens":1990,"cacheReadInputTokens":2989000,"cacheCreationInputTokens":9000,"contextWindow":1000000}}}'
"#);
        let root = script.parent().unwrap();
        let app = root.join("app");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join(crate::quota::TOKEN_POINTS_FILE),
            r#"{"1":[{"points":1.0,"tokens":{"claude-opus-5-5":1000000}}]}"#).unwrap();
        let service = crate::quota::Service::open(&app).unwrap();
        let mut cognition = NativeCognition::start(&script, root, &doctrine_fixture(), &skills_fixture()).unwrap();
        cognition.client.reader_state.lock().unwrap().token_sink = Some(("1".into(), service.token_sink()));
        cognition.prompt("hello", &mut |_| {}).unwrap();
        let now = crate::util::now_millis();
        let week = vec![crate::quota::Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: 52.,
            resets_at: Some(now + 72 * 3_600_000), duration_ms: 168 * 3_600_000 }];
        service.before_turn("1", Some((week, now)));
        let per_minute = service.view().speeds.get("seven_day").copied().unwrap_or(0.) * 60_000.;
        assert!((per_minute - 3.).abs() < 1e-9,
            "modelUsage says 3,000,000 Opus tokens at 1,000,000 a point = 3 points a minute, got {per_minute}");
    }

    #[test]
    fn live_input_waits_for_its_own_provider_receipt_and_preserves_the_running_prompt() {
        let script=write_script("live-peer-input", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r first
ours=$(printf '%s' "$first" | sed -n 's/.*"uuid":"\([^"]*\)".*/\1/p')
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"started\"}"
printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"text","text":"FIRST-ACTIVE\n"}]}}'
read -r second
peer=$(printf '%s' "$second" | sed -n 's/.*"uuid":"\([^"]*\)".*/\1/p')
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$peer\",\"state\":\"queued\"}"
printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"text","text":"PEER-QUEUED\n"}]}}'
read -r advance
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$peer\",\"state\":\"started\"}"
read -r finish
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"completed\"}"
cat >/dev/null
"#);
        let mut cognition=NativeCognition::start(&script,script.parent().unwrap(),&doctrine_fixture(),&skills_fixture()).unwrap();
        let input=cognition.cancel_handle().unwrap();
        let wire=cognition.client.stdin.clone();
        let (events,heard)=std::sync::mpsc::channel();
        let turn=std::thread::spawn(move || cognition.prompt("Keep the original job running",&mut |item| {
            if let TurnItem::Text {text,..}=item {events.send(text.to_string()).unwrap();}
        }));
        // load-bound: hang guard only; the received entry marker, never elapsed time, proves the prompt entered.
        assert!(heard.recv_timeout(Duration::from_secs(5)).expect("hang guard: first prompt must enter").contains("FIRST-ACTIVE"));
        let (taken,receipt)=std::sync::mpsc::channel();
        let id=uuid::Uuid::new_v4().to_string();
        assert!(input.send_input(&id,"Correction from Rich",Box::new(move |outcome| {taken.send(outcome.result).unwrap();})).unwrap());
        // load-bound: hang guard only; the queued marker establishes the protocol state before its receipt is released.
        assert!(heard.recv_timeout(Duration::from_secs(5)).expect("hang guard: provider must queue correction").contains("PEER-QUEUED"));
        assert!(matches!(receipt.try_recv(),Err(std::sync::mpsc::TryRecvError::Empty)),"queued is not consumed");
        assert!(input.send_input(&id,"Correction from Rich",Box::new(|_|panic!("same input was sent twice"))).unwrap());
        NativeClient::write_line(&wire,&json!({"type":"user","message":{"role":"user","content":"advance"}})).unwrap();
        // load-bound: hang guard only; the named receipt proves consumption, with no latency assertion.
        assert!(matches!(receipt.try_recv(),Err(std::sync::mpsc::TryRecvError::Empty)),"admission completed the correction before its turn ended");
        assert!(!turn.is_finished(),"correction admission ended the original prompt");
        NativeClient::write_line(&wire,&json!({"type":"user","message":{"role":"user","content":"finish"}})).unwrap();
        assert_eq!(turn.join().unwrap().unwrap(),"end_turn");
        // load-bound: hang guard only; the exact native result proves completion, not elapsed time.
        assert_eq!(receipt.recv_timeout(Duration::from_secs(5)).expect("native result did not complete live input").unwrap(),"end_turn");
    }

    #[test]
    fn a_vendor_question_is_saved_without_ending_the_providers_turn() {
        let script=write_script("vendor-question-continues", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"control_request","request_id":"ask","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{"questions":[{"question":"When should the release ship?","options":[{"label":"Ship today","description":"Earlier fixes"},{"label":"Ship tomorrow","description":"More testing"}]}]}}}'
read -r response
printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Continuing independent work"}]}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
"#);
        let root=script.parent().unwrap();
        let mut client=NativeClient::spawn(&script,root,&doctrine_fixture(),&skills_fixture()).unwrap();
        client.settle_workers_on_stop=true;
        let path=root.join("questions-scope.json");
        let scope=crate::question_tools::Scope{context:crate::questions::AskScope{ obligation_id:None,root:root.into(),entity_id:"company".into(),thread_id:"thread".into(),turn_id:"ask-turn".into(),asker:"front_desk".into(),session_id:client.session_id().into(),engine:None,entity_root:None,app_run:None},actions_allowed:true,answer_method:"typed".into(),surface:"mac".into()};
        crate::question_tools::write_scope(&path,&scope).unwrap();
        client.reader_state.lock().unwrap().question_scope=Some(path);
        let mut said=String::new();
        assert_eq!(client.prompt("Ask one choice",&mut |item|if let TurnItem::Text{text,..}=item {said.push_str(text)}).unwrap(),"end_turn");
        assert_eq!(said,"Continuing independent work");
        let qs=crate::questions::Store::new(root).list("company","thread").unwrap();
        assert_eq!(qs.len(),1);assert_eq!(qs[0].state,crate::questions::State::Open);
        assert!(qs[0].shown.is_none(),"persisting a question must not claim it was shown");
    }

    #[test]
    fn native_model_control_keeps_pending_inputs_and_a_refusal_does_not_replace_the_session() {
        let script=write_script("native-model-control",r#"
exec python3 -u -c '
import json,sys
def read(): return json.loads(sys.stdin.readline())
def send(row): print(json.dumps(row),flush=True)
init=read()
send({"type":"control_response","response":{"subtype":"success","request_id":init["request_id"],"response":{}}})
first=read()
assert first["message"]["content"]==[{"type":"text","text":"First unchanged"}]
refused=read()
assert refused["request"]=={"subtype":"set_model","model":"fable"}
send({"type":"control_response","response":{"subtype":"error","request_id":refused["request_id"],"error":"fixture model refusal"}})
accepted=read()
assert accepted["request"]==refused["request"] and accepted["request_id"]!=refused["request_id"]
send({"type":"control_response","response":{"subtype":"success","request_id":accepted["request_id"]}})
second=read()
assert second["message"]["content"]==[{"type":"text","text":"Second unchanged"}]
for index,prompt in enumerate([first,second]):
 send({"type":"command_lifecycle","state":"started","command_uuid":prompt["uuid"]})
 send({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Reply "+str(index+1)}]}})
 send({"type":"result","stop_reason":"end_turn","command_uuid":prompt["uuid"]})
'
"#);
        let client=NativeClient::spawn(&script,script.parent().unwrap(),&doctrine_fixture(),&skills_fixture()).unwrap();
        let session=client.session_id().to_string();
        let first=client.enqueue_prompt("First unchanged",None).unwrap();
        let model=crate::config::ModelChoice::Fable;
        assert!(client.cancel_handle().set_model(model).unwrap_err().contains("fixture model refusal"));
        assert!(client.cancel_handle().set_model(model).unwrap());
        assert!(client.pending.lock().unwrap().is_empty(),"a control response leaked its waiter");
        let second=client.enqueue_prompt("Second unchanged",None).unwrap();
        let mut answers=Vec::new();
        for response in [first,second] {
            let mut answer=String::new();
            assert_eq!(response.wait(&mut |item|if let TurnItem::Text{text,..}=item {answer.push_str(text)}).unwrap(),"end_turn");
            answers.push(answer);
        }
        assert_eq!(answers,["Reply 1","Reply 2"]);
        assert_eq!(client.session_id(),session);
    }

    /// Stu's turn-end signal (one-app plan 2026-10-21, step 1): a turn that ends with nothing
    /// awaited sets `turn_ended` in the file stall-watch reads, and new input clears it.
    #[test]
    fn a_turn_ending_with_nothing_awaited_sets_the_stall_watch_signal_and_new_input_clears_it() {
        let script=write_script("lead-signal",r#"
exec python3 -u -c '
import json,sys
def read(): return json.loads(sys.stdin.readline())
def send(row): print(json.dumps(row),flush=True)
init=read()
send({"type":"control_response","response":{"subtype":"success","request_id":init["request_id"],"response":{}}})
for index in range(2):
 prompt=read()
 while not __import__("os").path.exists("go-"+str(index)): __import__("time").sleep(0.01)
 send({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Reply"}]}})
 send({"type":"result","stop_reason":"end_turn","command_uuid":prompt["uuid"]})
'
"#);
        let root=script.parent().unwrap().to_path_buf();
        let client=NativeClient::spawn(&script,&root,&doctrine_fixture(),&skills_fixture()).unwrap();
        let path=LeadSignal::path_for(&root,client.session_id());
        let signal=Arc::new(LeadSignal::new(path.clone(),client.session_id()).unwrap());
        client.reader_state.lock().unwrap().lead=Some(signal);
        let read=||->Value {serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap()};
        let first=read();
        assert_eq!(first["session"],client.session_id());
        assert!(first["started"].as_u64().is_some_and(|at|at>0));
        assert!(first.get("turn_ended").is_none(),"a lease that has ended no turn: {first}");
        for index in 0..2 {
            let response=client.enqueue_prompt("Work",None).unwrap();
            assert!(read().get("turn_ended").is_none(),"input given clears the signal: {}",read());
            std::fs::write(root.join(format!("go-{index}")),"").unwrap();
            assert_eq!(response.wait(&mut |_|{}).unwrap(),"end_turn");
            let ended=read();
            assert!(ended["turn_ended"].as_u64().is_some_and(|at|at>=first["started"].as_u64().unwrap()),"{ended}");
            assert_eq!(ended["session"],client.session_id());
        }
        drop(client);
        // The file is the lease's own and goes with it.
        // load-bound: the reader thread holds the last reference briefly after the drop.
        let deadline=std::time::Instant::now()+Duration::from_secs(10);
        while path.exists() {
            assert!(std::time::Instant::now()<deadline,"the signal outlived its lease");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn native_plugin_reload_is_a_control_request_and_keeps_the_session() {
        for (name,answer) in [("reload-success","success"),("reload-refused","error")] {
            let script=write_script(name,&format!(r#"
read -r init
printf '%s\n' '{{"type":"control_response","response":{{"subtype":"success","request_id":"req_init","response":{{}}}}}}'
read -r request
printf '%s\n' "$request" > "$(dirname "$0")/reload-request.json"
printf '%s\n' "$request" | python3 -c 'import json,sys;r=json.load(sys.stdin);print(json.dumps({{"type":"control_response","response":{{"subtype":"{answer}","request_id":r["request_id"],"response":{{"agents":[{{"name":"richos-app-engine:fresh"}}]}},"error":"fixture refusal"}}}}))'
while read -r line; do :; done
"#));
            let root=script.parent().unwrap();
            let client=NativeClient::spawn(&script,root,&doctrine_fixture(),&skills_fixture()).unwrap();
            let before=client.session_id().to_string();
            let result=client.reload_plugins();
            assert_eq!(result.is_ok(),answer=="success");
            let request:Value=serde_json::from_slice(&std::fs::read(root.join("reload-request.json")).unwrap()).unwrap();
            assert_eq!(request["request"]["subtype"],"reload_plugins");
            assert_eq!(client.session_id(),before);
            assert!(client.pending.lock().unwrap().is_empty(),"reload left a stale control waiter");
        }
    }

    #[test]
    fn stu_peer_inputs_do_not_install_a_current_job_tool_grant() {
        let script=write_script("stu-no-current-grant",r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
while read -r line; do :; done
"#);
        let root=script.parent().unwrap();
        let python=std::process::Command::new("python3").args(["-c","import sys;print(sys.executable)"]).output().unwrap();
        let bridge=crate::ecs::EcsBridge::new(Path::new(String::from_utf8_lossy(&python.stdout).trim()),
            &Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine"),&root.join("ecs")).unwrap();
        let mut cognition=NativeCognition::start_with_continuity(&script,root,root,&skills_fixture(),&script,bridge,None).unwrap();
        let binding=crate::entity::ThreadBinding::new(crate::entity::PersonId::default_ceo(),crate::entity::EntityId::parse("company").unwrap(),"thread",1);
        for id in ["A","B"] {
            let message=crate::session_tools::Message {handoff:true,from:"rich".into(),text:id.into(),instruction:Some(crate::ecs::UserInstruction {
                ledger_ref:format!("ledger:thread:{id}"),sha256:"a".repeat(64),text:Some(id.into())}),work:None};
            cognition.prepare_agent_message(&binding,id,&message).unwrap();
            let scope:crate::ecs::ToolScope=serde_json::from_slice(&std::fs::read(&cognition.continuity.as_ref().unwrap().1).unwrap()).unwrap();
            assert!(scope.user_instruction.is_none(),"accepting B must not change authority for A's later tools");
            assert_eq!(scope.seat.as_deref(),Some("stu-thread:thread"));
        }
    }



    #[test]
    fn an_app_owned_question_does_not_discard_the_agents_later_prose() {
        let script=write_script("app-question-continues",r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Asking now"}]}}'
printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Independent work continues"}]}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
"#);
        let root=script.parent().unwrap();
        let mut client=NativeClient::spawn(&script,root,&doctrine_fixture(),&skills_fixture()).unwrap();
        client.settle_workers_on_stop=true;
        let path=root.join("scope.json");
        let scope=crate::question_tools::Scope{context:crate::questions::AskScope{ obligation_id:None,root:root.into(),entity_id:"company".into(),thread_id:"thread".into(),turn_id:"ask-turn".into(),asker:"stu".into(),session_id:client.session_id().into(),engine:None,entity_root:None,app_run:None},actions_allowed:true,answer_method:"typed".into(),surface:"mac".into()};
        crate::session_tools::bind(&root.join("status-scope.json"), &crate::status_tools::StatusToolScope {
            version:1,state_root:root.into(),entity_id:"company".into(),thread_id:"thread".into()
        },client.session_id(),"stu",None).unwrap();
        crate::question_tools::write_scope(&path,&scope).unwrap();
        client.reader_state.lock().unwrap().question_scope=Some(path.clone());
        let mut asked=false;
        let mut said=String::new();
        let reason=client.prompt("Ask one choice",&mut |item|if let TurnItem::Text{text,..}=item {
            said.push_str(text);
            if !asked {
                asked=true;
                crate::question_tools::call(&path,"ask",json!({"questions":[{"text":"When should the release ship?","options":[{"label":"Ship today","description":"Earlier fixes"},{"label":"Ship tomorrow","description":"More testing"}]}]})).unwrap();
            }
        }).unwrap();
        assert!(asked);assert_eq!(reason,"end_turn");
        assert!(said.contains("Independent work continues"));
    }

    #[test]
    fn a_question_leaves_the_same_provider_available_for_the_answer() {
        let script=write_script("question-provider-survives",r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Asking now"}]}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
read -r answer
printf '%s\n' "$answer" > "$(dirname "$0")/next-input"
printf '%s\n' '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Still here"}]}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
"#);
        let root=script.parent().unwrap();
        let mut client=NativeClient::spawn(&script,root,&doctrine_fixture(),&skills_fixture()).unwrap();
        client.settle_workers_on_stop=true;
        let path=root.join("scope.json");
        let scope=crate::question_tools::Scope{context:crate::questions::AskScope{ obligation_id:None,root:root.into(),entity_id:"company".into(),thread_id:"thread".into(),turn_id:"ask-turn".into(),asker:"front_desk".into(),session_id:client.session_id().into(),engine:None,entity_root:None,app_run:None},actions_allowed:true,answer_method:"typed".into(),surface:"mac".into()};
        crate::question_tools::write_scope(&path,&scope).unwrap();
        client.reader_state.lock().unwrap().question_scope=Some(path.clone());
        let mut asked=false;
        let reason=client.prompt("Ask one choice",&mut |item|if matches!(item,TurnItem::Text{..}) && !asked {
            asked=true;
            crate::question_tools::call(&path,"ask",json!({"questions":[{"text":"When should the release ship?","options":[{"label":"Ship today","description":"Earlier fixes"},{"label":"Ship tomorrow","description":"More testing"}]}]})).unwrap();
        }).unwrap();
        assert!(asked);assert_eq!(reason,"end_turn");
        client.reader_state.lock().unwrap().question_scope=None;
        let mut said=String::new();
        let next=client.prompt("Ship today",&mut |item|if let TurnItem::Text{text,..}=item {said.push_str(text)}).unwrap();
        assert_eq!((next.as_str(),said.as_str()),("end_turn","Still here"));
        let received:Value=serde_json::from_str(&std::fs::read_to_string(root.join("next-input")).unwrap()).unwrap();
        assert_eq!(received["type"],"user","the host injected an interrupt after the question");
    }











    #[test]
    fn native_preparation_binds_tools_without_sending_model_input() {
        let script=write_script("brief-ahead-of-his-turn",
            r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
while read -r line; do
  printf 'x\n' >> "$(dirname "$0")/hidden-turns"
  printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
done
"#);
        let root=script.parent().unwrap();
        let out=std::process::Command::new("python3").args(["-c","import sys; print(sys.executable)"]).output().unwrap();
        assert!(out.status.success());
        let python=std::path::PathBuf::from(String::from_utf8(out.stdout).unwrap().trim());
        let engine=Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine");
        let bridge=crate::ecs::EcsBridge::new(&python,&engine,&root.join("ecs-state")).unwrap();
        let mut cognition=NativeCognition::start(&script,root,&doctrine_fixture(),&skills_fixture()).unwrap();
        // The front desk of an engine install: the continuity bridge and its tools, as
        // `start_with_engine` gives it (no engine profile: no plugin to verify here).
        cognition.continuity=Some((bridge,root.join("app-scope.json")));
        cognition.client.reader_state.lock().unwrap().continuity_tools_loaded=true;
        let hidden=|| std::fs::read_to_string(root.join("hidden-turns")).map(|s| s.lines().count()).unwrap_or(0);
        let binding=crate::entity::ThreadBinding::new(crate::entity::PersonId::default_ceo(),
            crate::entity::EntityId::parse("depot").unwrap(),"thread-a",1);

        cognition.prepare_work_turn(&binding, "turn-1", "turn-1", crate::ledger::Source::Jam,
            "Our company sells accounting software.",&mut |_|{}).unwrap();
        assert_eq!(hidden(),0,"his first turn paid a hidden context turn before his words were sent");
        assert!(!root.join("app-scope.json").exists());
        cognition.prepared_scope.take().unwrap()().unwrap();
        // A pinned older engine does not advertise the combined request. Its
        // legacy calls must preserve the same held brief and newly bound scope.
        cognition.prepare_work_turn(&binding, "turn-2", "turn-2", crate::ledger::Source::Jam,
            "Our company sells accounting software.",&mut |_|{}).unwrap();
        assert_eq!(hidden(),0,"the legacy preparation path paid an unchanged hidden turn");
        let previous: crate::ecs::ToolScope = serde_json::from_slice(&std::fs::read(root.join("app-scope.json")).unwrap()).unwrap();
        assert_eq!(previous.binding.turn_id, "turn-1", "acceptance changed the running input scope");
        cognition.prepared_scope.take().unwrap()().unwrap();
        let scope: crate::ecs::ToolScope = serde_json::from_slice(&std::fs::read(root.join("app-scope.json")).unwrap()).unwrap();
        assert_eq!(scope.binding.turn_id, "turn-2");
        assert_eq!(scope.user_instruction.unwrap().text.as_deref(), Some("Our company sells accounting software."));
    }

    #[test]
    fn native_prompt_hook_receives_company_and_continuity_without_changing_user_words() {
        let script=write_script("company-continuity-hook",r#"
exec python3 -u -c '
import json,sys
def read(): return json.loads(sys.stdin.readline())
def send(frame): print(json.dumps(frame),flush=True)
read()
send({"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}})
command=read()
assert command["message"]["content"]==[{"type":"text","text":"My exact original words."}]
send({"type":"command_lifecycle","command_uuid":command["uuid"],"state":"started"})
send({"type":"control_request","request_id":"hook","request":{"subtype":"hook_callback","callback_id":"richos-input-context","input":{"prompt_id":"different-native-id","hook_event_name":"UserPromptSubmit","prompt":"My exact original words."}}})
response=read()["response"]["response"]
context=response["hookSpecificOutput"]["additionalContext"]
assert "<executive-continuity>" in context,context
assert "company-setup-marker-417" in context,context
send({"type":"assistant","message":{"content":[{"type":"text","text":"Context received."}]}})
send({"type":"result","stop_reason":"end_turn"})
'
"#);
        let root=script.parent().unwrap();
        let mut cognition=NativeCognition::start(&script,root,&doctrine_fixture(),&skills_fixture()).unwrap();
        let out=std::process::Command::new("python3").args(["-c","import sys; print(sys.executable)"]).output().unwrap();
        assert!(out.status.success());
        let python=std::path::PathBuf::from(String::from_utf8(out.stdout).unwrap().trim());
        let engine=Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine");
        let bridge=crate::ecs::EcsBridge::new(&python,&engine,&root.join("ecs-state")).unwrap();
        cognition.continuity=Some((bridge,root.join("app-scope.json")));
        cognition.set_company_context(Some("company-setup-marker-417")).unwrap();
        let binding=crate::entity::ThreadBinding::new(crate::entity::PersonId::default_ceo(),
            crate::entity::EntityId::parse("depot").unwrap(),"thread-context",1);
        cognition.prepare_work_turn(&binding,"context-turn","context-turn",crate::ledger::Source::Text,
            "My exact original words.",&mut |_|{}).unwrap();
        let mut answer=String::new();
        assert_eq!(cognition.prompt("My exact original words.",&mut |item| {
            if let TurnItem::Text{text,..}=item {answer.push_str(text)}
        }).unwrap(),"end_turn");
        assert_eq!(answer,"Context received.");
        let scope:crate::ecs::ToolScope=serde_json::from_slice(&std::fs::read(root.join("app-scope.json")).unwrap()).unwrap();
        assert_eq!(scope.user_instruction.unwrap().text.as_deref(),Some("My exact original words."));
    }



    // ---- the background-work spec's §5.8a-ii seams ------------------------------------

    fn fixture_bridge(root: &Path) -> crate::ecs::EcsBridge {
        crate::ecs::EcsBridge {
            python: root.join("runtime/bin/python3"),
            component: root.join("engine/ecs"),
            state_root: root.join("ecs"),
        }
    }

    fn fixture_profile(root: &Path) -> crate::engine_profile::EngineProfile {
        crate::engine_profile::EngineProfile {
            engine: root.join("engine"),
            coordination: root.join("coordination"),
            plugin: root.join("plugin"),
            state: root.join("engine-state"),
            runtime: crate::runtime::EngineRuntime {
                root: root.join("runtime"),
                python: root.join("runtime/bin/python3"),
                node: root.join("runtime/bin/node"),
                git: root.join("runtime/bin/git"),
                versions: Default::default(),
            },
            work_scope: None,
            user: Default::default(),

            claude_account: None, token_sink: None, speech_model: None, lease_models: Default::default(), machine_settings: Default::default(),
        }
    }

    #[test]
    fn native_resume_uses_each_roles_saved_session_and_refuses_missing_history() {
        let root=fixture_root().join(format!("resume-{}",uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let python=std::process::Command::new("python3").args(["-c","import sys;print(sys.executable)"]).output().unwrap();
        let engine=Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine");
        let bridge=crate::ecs::EcsBridge::new(Path::new(String::from_utf8_lossy(&python.stdout).trim()),&engine,&root.join("ecs")).unwrap();
        let mut profile=fixture_profile(&root);
        profile.work_scope=Some(("company".into(),"conversation".into()));
        let scope=crate::status_tools::StatusToolScope{version:1,state_root:profile.state.clone(),entity_id:"company".into(),thread_id:"conversation".into()};
        for (role,name,audience,seat) in [
            (LeaseRole::Conversation,"rich","ceo","ceo-thread:conversation"),
            (LeaseRole::Work,"stu","rich","stu-thread:conversation")
        ] {
            assert!(native_resume(Some(&profile),Some((&bridge,&root)),role).unwrap().is_none());
            let session=uuid::Uuid::new_v4().to_string();
            bridge.bind("company","conversation",&session,"original",Some(seat),audience).unwrap();
            crate::session_tools::bind(&root.join(format!("{name}-scope.json")),&scope,&session,name,None).unwrap();
            let transcript=root.join(format!("{session}.jsonl"));
            std::fs::write(&transcript,"saved native history").unwrap();
            std::fs::write(profile.state.join("evidence").join(&session).join("callbacks.jsonl"),format!("{}\n",json!({"callback":{"transcript_path":transcript}}))).unwrap();
            // Reopen the actual durable engine connection, as app restart/eviction does.
            let reopened=crate::ecs::EcsBridge::new(Path::new(String::from_utf8_lossy(&python.stdout).trim()),&engine,&root.join("ecs")).unwrap();
            assert_eq!(native_resume(Some(&profile),Some((&reopened,&root)),role).unwrap(),Some((session,transcript.clone())));
            std::fs::remove_file(transcript).unwrap();
            assert!(native_resume(Some(&profile),Some((&reopened,&root)),role).is_err(),"missing history silently created a replacement session");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stu_monitor_keeps_whole_observations_scoped_and_ends_with_its_owner() {
        let root=fixture_root().join(format!("monitor-{}",uuid::Uuid::new_v4()));
        let mut profile=fixture_profile(&root);
        profile.work_scope=Some(("company-a".into(),"thread-a".into()));
        std::fs::create_dir_all(profile.engine.join("scripts")).unwrap();
        std::fs::create_dir_all(&profile.coordination).unwrap();
        std::fs::write(profile.engine.join("scripts/stall-watch.sh"),r#"
[ "$1" = --monitor ] && [ "$2" = --jsonl ] || exit 2
[ "${RICHOS_PROJECTS_DIR+set}" != set ] && [ "${RICHOS_SESSIONS_DIR+set}" != set ] || exit 2
printf '{"text":"%s|%s|%s|%s|%s\\nSecond line"}\n' "$RICHOS_SESSION_ID" "$RICHOS_APP_ENTITY" "$RICHOS_APP_THREAD" "$RICHOS_SESSION_PID" "$STALL_WATCH_STATE_DIR"
exec /bin/sleep 1000
"#).unwrap();
        let mut environment=Command::new("/fictional/provider");
        environment.env("RICHOS_PROJECTS_DIR","/fictional/terminal-projects")
            .env("RICHOS_SESSIONS_DIR","/fictional/terminal-sessions");
        profile.configure(&mut environment,Path::new("/fictional/provider"),"stu-session",&root.join("scope.json"));
        let closed=Arc::new(AtomicBool::new(false));
        assert!(StallWatch::start(Some(&profile),LeaseRole::Conversation,&environment,"rich-session",std::process::id(),Arc::clone(&closed)).unwrap().is_none());
        let monitor=StallWatch::start(Some(&profile),LeaseRole::Work,&environment,"stu-session",std::process::id(),closed).unwrap().unwrap();
        let scope=crate::status_tools::StatusToolScope {version:1,state_root:profile.state.clone(),entity_id:"company-a".into(),thread_id:"thread-a".into()};
        // load-bound: hang guard only; the complete durable observation, never elapsed time, is the verdict.
        let deadline=std::time::Instant::now()+Duration::from_secs(10);
        let (message,through)=loop {
            if let Some(message)=crate::session_tools::pending(&scope,"stu").unwrap() {break message}
            assert!(std::time::Instant::now()<deadline,"monitor did not save its observation");
            // load-bound: backoff only; the verdict is the complete saved block, with the separate deadline catching hangs.
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(message.from,"stall-watch");
        assert!(message.instruction.is_none());
        assert_eq!(message.text,format!("stu-session|company-a|thread-a|{}|{}\nSecond line",std::process::id(),profile.state.join("stall-watch").display()));
        assert_eq!(crate::session_tools::pending(&scope,"stu").unwrap().unwrap().1,through,"saving is not receipt");
        let other=crate::status_tools::StatusToolScope {thread_id:"thread-b".into(),..scope.clone()};
        assert!(crate::session_tools::pending(&other,"stu").unwrap().is_none());
        assert!(crate::session_tools::pending(&scope,"rich").unwrap().is_none());
        monitor.closing.store(true,Ordering::SeqCst);              // the lease ends it, as NativeClient::drop does
        let StallWatch {_child:child,_output:output,..}=monitor;
        drop(child);
        output.join().unwrap();
        assert!(crate::session_tools::pending_after(&scope,"stu",through).unwrap().is_none(),
            "a normal lease close was reported as a failure");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Orchestration reliability R5: the monitor's parent dying while the lease is open reaches
    /// Stu through the same durable input as every block; before, the reader thread just ended.
    #[test]
    fn an_unexpected_monitor_end_is_told_through_the_durable_input() {
        let root=fixture_root().join(format!("monitor-dies-{}",uuid::Uuid::new_v4()));
        let mut profile=fixture_profile(&root);
        profile.work_scope=Some(("company-a".into(),"thread-a".into()));
        std::fs::create_dir_all(profile.engine.join("scripts")).unwrap();
        std::fs::create_dir_all(&profile.coordination).unwrap();
        std::fs::write(profile.engine.join("scripts/stall-watch.sh"),"exit 3\n").unwrap();
        let mut environment=Command::new("/fictional/provider");
        profile.configure(&mut environment,Path::new("/fictional/provider"),"stu-session",&root.join("scope.json"));
        let monitor=StallWatch::start(Some(&profile),LeaseRole::Work,&environment,"stu-session",std::process::id(),
            Arc::new(AtomicBool::new(false))).unwrap().unwrap();
        let scope=crate::status_tools::StatusToolScope {version:1,state_root:profile.state.clone(),entity_id:"company-a".into(),thread_id:"thread-a".into()};
        let StallWatch {_output:output,..}=monitor;
        output.join().unwrap();
        let message=crate::session_tools::pending(&scope,"stu").unwrap().expect("the failure was not delivered");
        assert_eq!(message.0.from,"stall-watch");
        assert!(message.0.text.contains("the monitor stopped unexpectedly (its output ended)"),"{}",message.0.text);
        std::fs::remove_dir_all(root).unwrap();
    }


    /// **SEAM 1.** The work lease's MCP config does not register `richos_continuity`, and
    /// the conversation lease's does.
    ///
    /// The negative is the load-bearing half, so it has a positive control in the same
    /// test: the identical call with `LeaseRole::Conversation` DOES register the server,
    /// which is what makes its absence evidence rather than a config that never had it.
    ///
    #[test]
    fn a_work_leases_config_omits_the_continuity_server_and_keeps_the_work_server() {
        let root = fixture_root().join(format!("mcp-config-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let bridge = fixture_bridge(&root);
        let profile = fixture_profile(&root);
        let scope = root.join("scope.json");
        let continuity = root.join("continuity.json");
        let continuity_tools = root.join("continuity-tools.json");
        let executable = root.join("RichOS");

        let assignments = root.join("assignments.json");
        let status = root.join("status.json");

        let work = mcp_config(&executable, &scope, &assignments, &status, Some((&bridge, &continuity)), Some(continuity_tools.as_path()), Some(&profile), LeaseRole::Work, Path::new("claude"));
        let servers = &work["mcpServers"];
        assert!(servers.get("richos_continuity").is_some(), "Stu lost his scoped continuity tools");
        assert!(servers.get("richos_work").is_some(), "the work lease was not given the work server");
        // **This assertion was `is_some()` until 2026-09-18 and it was asserting the defect.**
        // Ray's candidate-.8 row 1: the work lease's onboarding scope is written by nothing,
        // so the server named a file that never existed and the grant over it failed the job
        // 6.317 s in. See `a_work_lease_never_holds_a_grant_over_a_company_scope_nothing_writes`.
        assert!(servers.get("richos_onboarding").is_none(),
            "the work lease was given the company-notes server over a scope only spine.rs writes");
        // And it cannot register more assignments for itself (`assignment_tools.rs`).
        assert!(servers.get(crate::assignment_tools::SERVER_NAME).is_none(),
            "Stu still has a register bound to the last request instead of each saved handoff");
        // Nor read the front desk's record (`status_tools.rs`).
        assert!(servers.get(crate::status_tools::SERVER_NAME).is_some(),
            "Stu cannot inspect the shared record");

        // Positive control: the same call for the conversation DOES register its two.
        let chat = mcp_config(&executable, &scope, &assignments, &status, Some((&bridge, &continuity)), Some(continuity_tools.as_path()), Some(&profile), LeaseRole::Conversation, Path::new("claude"));
        assert!(chat["mcpServers"].get("richos_continuity").is_some(), "the conversation lost its continuity server");
        assert_eq!(chat["mcpServers"]["richos_work"]["args"].as_array().unwrap().len(), 2);
        assert_eq!(work["mcpServers"]["richos_work"]["args"].as_array().unwrap().len(), 2);
        assert!(chat["mcpServers"].get(crate::assignment_tools::SERVER_NAME).is_some(),
            "the conversation has no way to end a turn on a receipt");
        assert!(chat["mcpServers"].get(crate::status_tools::SERVER_NAME).is_some(),
            "the front desk has no way to look at what is running");

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// **THE FRONT DESK'S WHOLE TOOL INVENTORY, AND THE BACK END'S**, asserted as two
    /// complete lists rather than as four separate absences.
    ///
    /// The CEO's Two Riches page: *"the job of the front desk Rich is solely talking to the
    /// CEO and relaying info to and from the back-end Rich"*, and note 3 *"The front desk
    /// gets no orchestration tools, so it cannot drift into doing the work."* This is that
    /// sentence as a measurement. It is written as an exact set on purpose: a test that
    /// asserted only `richos_work`'s absence would go on passing if a fifth server appeared
    /// beside it, and "no orchestration tools" is a claim about everything on the lease.
    #[test]
    fn rich_has_the_register_and_full_evidence_while_stu_has_execution() {
        let root = fixture_root().join(format!("mcp-inventory-{}", uuid::Uuid::new_v4()));
        let bridge = fixture_bridge(&root);
        let profile = fixture_profile(&root);
        let (scope, continuity, executable) =
            (root.join("scope.json"), root.join("continuity.json"), root.join("RichOS"));
        let continuity_tools = root.join("continuity-tools.json");
        let (assignments, status) = (root.join("assignments.json"), root.join("status.json"));

        let names = |config: &Value| {
            let mut found: Vec<String> =
                config["mcpServers"].as_object().unwrap().keys().cloned().collect();
            found.sort();
            found
        };

        let chat = mcp_config(&executable, &scope, &assignments, &status,
            Some((&bridge, &continuity)), Some(continuity_tools.as_path()), Some(&profile), LeaseRole::Conversation, Path::new("claude"));
        assert_eq!(
            names(&chat),
            vec!["richos_assignments", "richos_continuity", "richos_files", "richos_onboarding", "richos_questions", "richos_status", "richos_work"],
            "the front desk's tool list moved"
        );
        // Named on its own as well, because the list above is the WHAT and this is the WHY.
        assert_eq!(chat["mcpServers"]["richos_work"]["args"].as_array().unwrap().len(), 2);

        // The back end is the other half of the same sentence: it holds the work, and it can
        // neither give itself more of it nor read the front desk's record.
        let work = mcp_config(&executable, &scope, &assignments, &status,
            Some((&bridge, &continuity)), Some(continuity_tools.as_path()), Some(&profile), LeaseRole::Work, Path::new("claude"));
        assert_eq!(
            names(&work),
            vec!["richos_continuity", "richos_files", "richos_questions", "richos_quota", "richos_status", "richos_work"],
            "the back end's tool list moved"
        );
        // **The back end holds the work and NOTHING ELSE, and that is the third absence.**
        // The list above read `["richos_onboarding", "richos_work"]` until 2026-09-18. Named
        // on its own for the same reason `richos_work`'s absence is named above the
        // conversation list: the set is the WHAT and this is the WHY. Saving his answers about
        // his own company is something he says in a visible turn; a back end holding those
        // two tools under §5.4's standing grant could rewrite them with no turn at all.
        assert!(work["mcpServers"].get("richos_onboarding").is_none(),
            "the back end can still rewrite the CEO's company notes");
        // Nothing here wrote to the root — `mcp_config` builds a value and touches no disk,
        // which is the property that lets this assert the inventory without a child process.
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A lease built through the REAL [`NativeClient::spawn_with_tools`], so the grant list
    /// under test is the production one rather than a hand-assembled `Vec`.
    ///
    /// **No engine profile, deliberately.** The readiness gates and the worker-settlement read
    /// are about a delivered engine and are covered by `hand_built_work_lease`; this is about
    /// the grants, and a profile here would demand a real Python supervisor. Everything the
    /// grant loop reads is present: the four app-owned paths, the continuity scope, the role.
    ///
    /// The onboarding path is **never written**, which is exactly the state `start_work_lease`
    /// leaves it in on every machine.
    fn lease_with_production_grants(tag: &str, role: LeaseRole, script_body: &str)
        -> (NativeCognition, std::path::PathBuf) {
        let root = fixture_root().join(format!("richos-grant-{tag}-{}", uuid::Uuid::new_v4().simple()));
        let scopes = root.join("scopes");
        std::fs::create_dir_all(&scopes).unwrap();
        let identity = uuid::Uuid::new_v4();
        let onboarding = scopes.join(format!("{identity}-onboarding.json"));
        let assignments = scopes.join(format!("{identity}-assignments.json"));
        let status = scopes.join(format!("{identity}-status.json"));
        let continuity = scopes.join(format!("{identity}-work.json"));
        // **The deferred grant's file, on a CONVERSATION lease only** — exactly as the two
        // production constructors write it, and exactly as `start_work_lease` does not.
        let continuity_tools = scopes.join(format!("{identity}-continuity-tools.json"));
        let bridge = fixture_bridge(&root);
        // Written in full for the same reason the work scope below is: `ecs::ToolScope` denies
        // unknown fields, and this grant reads the file it rewrites.
        crate::ecs::write_scope(&continuity_tools, &crate::ecs::ToolScope {
            version: 1,
            actions_allowed: false,
            bridge: bridge.clone(),
            binding: crate::ecs::Binding {
                entity_id: "qa-test-co".into(),
                thread_id: "thr_one".into(),
                session_id: "sess_one".into(),
                turn_id: "turn-1".into(),
                audience: "ceo".into(),
                revision: 1,
            },
            user_instruction: None,
            seat: Some("ceo-thread:thr_one".into()),
            background_work_allowed: false,
        })
        .unwrap();
        // **The one grant on a work lease that DOES have a file under it**, written here
        // because `start_work_lease` writes it (`{"version":1,"actions_allowed":false}`).
        // Written in full rather than as that two-field stub because `ecs::ToolScope` requires
        // `bridge` and `binding` and denies unknown fields, so the stub is only readable after
        // `bind_work_assignment` has replaced it — which in production it always has, since
        // `work_host.rs`'s `run_one` binds before it prompts.
        crate::ecs::write_scope(&continuity, &crate::ecs::ToolScope {
            version: 1,
            actions_allowed: false,
            bridge: bridge.clone(),
            binding: crate::ecs::Binding {
                entity_id: "qa-test-co".into(),
                thread_id: "thr_one".into(),
                session_id: "sess_one".into(),
                turn_id: "ob-1".into(),
                audience: "worker".into(),
                revision: 1,
            },
            user_instruction: None,
            seat: Some("work-seat:ob-1".into()),
            // The state `start_work_lease` leaves behind: nothing is granted until
            // `bind_work_assignment` writes the standing grant for one assignment.
            background_work_allowed: false,
        })
        .unwrap();
        let (doctrine, skills) = (doctrine_fixture(), skills_fixture());
        let mut script_body = script_body.to_string();
        if let Some((start, _)) = script_body.match_indices("read -r line").nth(1) {
            script_body.replace_range(start..start+"read -r line".len(), "read -r prompt");
        }
        let hooked = script_body.replace("read -r prompt", r#"read -r prompt
python3 -c 'import json,sys; command=json.loads(sys.argv[1])["uuid"]; print(json.dumps({"type":"command_lifecycle","state":"started","command_uuid":command})); print(json.dumps({"type":"control_request","request_id":"fixture_scope","request":{"subtype":"hook_callback","callback_id":"richos-input-context","input":{"hook_event_name":"UserPromptSubmit","prompt_id":command,"prompt":json.loads(sys.argv[1])["message"]["content"][0]["text"]}}}))' "$prompt"
read -r scope_response"#);
        let script = write_script(&format!("grant-{tag}"), &hooked);
        let client = NativeClient::spawn_with_tools(
            &script,
            Path::new("/tmp"),
            Some((&doctrine, &skills)),
            Some((&root.join("RichOS"), &onboarding, &assignments, &status)),
            Some((&bridge, &continuity)),
            match role { LeaseRole::Conversation => Some(continuity_tools.as_path()), LeaseRole::Work => None },
            None,
            None,
            role,
        )
        .expect("the handshake should succeed");
        let session_id = client.session_id().to_string();
        // Mirrors its constructor exactly: `start_with_engine` holds the path, and since
        // 2026-09-18 `start_work_lease` holds `None`.
        let onboarding_scope = match role {
            LeaseRole::Conversation => Some(onboarding),
            LeaseRole::Work => None,
        };
        let lease = NativeCognition {
            client,
            session_id,
            onboarding_scope,
            assignments_scope: None,
            status_scope: None,
            continuity: Some((bridge, continuity)),
            continuity_tools_scope: match role {
                LeaseRole::Conversation => Some(continuity_tools),
                LeaseRole::Work => None,
            },
            work_binding: None,
            engine_profile: None,
            role,
            ceo_thread_seats: None, classifier: None, routing_context: String::new(), app_messages: Vec::new(), prepared_scope: None, company_context: None, prepare_turn_supported: false, pending_onboarding: None, pending_channel: None, title_writer: None,
        };
        (lease, root)
    }

    fn grant_names(client: &NativeClient) -> Vec<&'static str> {
        client
            .action_grants
            .iter()
            .map(|grant| match grant {
                ActionGrant::Onboarding(_) => "onboarding",
                ActionGrant::Assignments(_) => "assignments",
                ActionGrant::Continuity(_) => "continuity",
                ActionGrant::ContinuityTools(_) => "continuity-tools",
                ActionGrant::Questions(_) => "questions",
            })
            .collect()
    }

    /// **RAY'S CANDIDATE-.8 ROW 1, AS A TEST — the third way a background job died in one
    /// morning, and the only one of the three that was never about the engine.**
    ///
    /// Measured on his walk (`docs/verification/2026-09-18-nightly-1.2.0-20260918.2-onscreen-audit.md`
    /// §1 row 1, try 3, on the instance whose engine had just been replaced so the readiness
    /// gate passed): the job failed **6.317 s** after registration — re-derived from his own
    /// two timestamps, `10:45:54.192Z − 10:45:47.875Z` — with *"cognition io: Choose a company
    /// before saving interview answers."*, for a company that WAS selected and an interview
    /// nobody had started. `notes.txt` never changed.
    ///
    /// **The chain, re-derived at this commit, every link cited:**
    ///
    /// 1. `start_work_lease` computes `scopes/{identity}-onboarding.json`. Nothing writes it
    ///    and nothing can: `Cognition::set_onboarding_scope` is called from exactly two places,
    ///    `spine.rs:3093` and `spine.rs:3278`, and both are the conversation's lease.
    /// 2. `spawn_with_tools` put `ActionGrant::Onboarding` on the grant list for EVERY role —
    ///    only `ActionGrant::Assignments` carried the `LeaseRole::Conversation` filter.
    /// 3. `work_host.rs`'s `run_one` calls `lease.prompt(…)`, and `prompt`'s first act is to
    ///    open every grant in that list, all-or-nothing.
    /// 4. `ActionGrant::Onboarding::set(true)` → `onboarding_tools::set_actions_allowed` →
    ///    `read_scope` → `File::open` on a path that is not there → the sentence at
    ///    `onboarding_tools.rs:137`, returned as `CognitionError::Io`, which `cognition.rs:21`
    ///    renders with the `cognition io:` prefix he read on the card.
    ///
    /// So it fired **before the turn was ever sent** — which is why 6.317 s is a spawn and a
    /// handshake and nothing like a run, why no model token was spent, and why the assignment's
    /// record never left `Preparing`.
    ///
    /// **Four arms, and the two positive controls are the point.** The sentence must still be
    /// raised by a grant over a missing scope (A) and must still fail a CONVERSATION lease with
    /// no company bound (D). Without those, the two negatives would be a check that moved
    /// rather than a defect that went away.
    #[test]
    fn a_work_lease_never_holds_a_grant_over_a_company_scope_nothing_writes() {
        const INTERVIEW: &str = "Choose a company before saving interview answers.";

        // ---- A. POSITIVE CONTROL: the sentence is still live, and still says this ----------
        // Unchanged, and it must stay unchanged: `read_scope` mapping a missing file to this
        // sentence is correct for the seat that asks the question.
        let absent = fixture_root()
            .join(format!("richos-no-scope-{}", uuid::Uuid::new_v4().simple()))
            .join("never-written.json");
        assert_eq!(
            ActionGrant::Onboarding(absent.clone()).set(true),
            Err(INTERVIEW.to_string()),
            "the sentence this test is about is no longer raised, so its negatives prove nothing"
        );
        // And deliberately NOT given the `!path.exists() => Ok(())` guard `Assignments` has:
        // on a conversation lease a missing scope means no company is bound, and failing there
        // is what stops a model writing company notes with no company chosen.
        assert_eq!(
            ActionGrant::Assignments(absent).set(true),
            Ok(()),
            "the two app-owned grants no longer differ, which is the distinction arm D rests on"
        );

        // ---- B. THE FIX, STRUCTURALLY: the grant list, from the real constructor ----------
        let (work, work_root) =
            lease_with_production_grants("work-list", LeaseRole::Work, SILENT_AFTER_HANDSHAKE);
        assert_eq!(
            grant_names(&work.client),
            vec!["questions", "continuity"],
            "a work lease's grant list is not just its own continuity scope"
        );
        assert!(
            work.onboarding_scope.is_none(),
            "a work lease is carrying a company scope nothing writes"
        );
        drop(work);
        let _ = std::fs::remove_dir_all(&work_root);

        let (chat, chat_root) =
            lease_with_production_grants("chat-list", LeaseRole::Conversation, SILENT_AFTER_HANDSHAKE);
        assert_eq!(
            grant_names(&chat.client),
            // `continuity-tools` joined this list on 2026-09-18 and is the one grant `prompt`
            // does NOT open with the others — it is in the list so that the turn's end, a
            // cancel and a dropped lease all revoke it by the same loop.
            vec!["onboarding", "assignments", "questions", "continuity", "continuity-tools"],
            "the front desk lost a grant it needs — the fix has moved, not landed"
        );
        drop(chat);
        let _ = std::fs::remove_dir_all(&chat_root);

        // ---- C. THE FIX, BEHAVIORALLY: the same call that failed his job now takes a turn --
        let frame = work_init_frame(true, true);
        let (mut lease, root) =
            lease_with_production_grants("work-turn", LeaseRole::Work, &one_turn_reporting(&frame));
        let mut items = 0usize;
        let stop = crate::cognition::Cognition::prompt(&mut lease, "do the job", &mut |_| items += 1)
            .expect("a work lease's first turn must not be refused over a company scope");
        assert_eq!(stop, "end_turn");
        assert!(items > 0, "the child's stream never reached the caller, so nothing was proved");
        drop(lease);
        let _ = std::fs::remove_dir_all(&root);

        // ---- D. POSITIVE CONTROL: the front desk with no company bound still refuses -------
        // `spine.rs`'s `None => Ok(())` means an unbound conversation writes no scope either,
        // and that lease must still hit exactly the sentence — the fix is a role gate, not a
        // relaxation of the check.
        let (mut chat, chat_root) = lease_with_production_grants(
            "chat-unbound",
            LeaseRole::Conversation,
            &one_turn_reporting(&frame),
        );
        let refusal = crate::cognition::Cognition::prompt(&mut chat, "hello", &mut |_| {})
            .expect_err("a conversation with no company bound must still refuse")
            .to_string();
        assert!(
            refusal.contains(INTERVIEW),
            "the front desk's own check was relaxed along with the work lease's: {refusal}"
        );
        drop(chat);
        let _ = std::fs::remove_dir_all(&chat_root);
    }

    /// **THE HELPER OUTLIVES THE TURN THAT DISPATCHED IT, SO ITS PERMISSION HAS TO — and a
    /// stop still takes it.**
    ///
    /// Measured on 2026-09-18 in a `work_lease_roundtrip` run and quoted from the worker's
    /// own last words in that run's `callbacks.jsonl` (row 21): *"the host's PreToolUse hook
    /// is blocking all tool actions this turn, including SubagentHandback itself, with:
    /// 'RichOS desktop engine: This app turn is stopped or is supplying context. New actions
    /// are unavailable.'"* The worker was dispatched `run_in_background: true`, the turn
    /// ended seconds later by design, and every tool call it then made — every one of its
    /// real work — was refused by the app's own turn gate. It changed nothing, the fixture
    /// kept its two commits, and its worktree was disposed of as landed because an agent
    /// that produced nothing IS landed.
    ///
    /// The three transitions below are the whole of the rule, each asserted against the
    /// function the real caller calls:
    ///
    /// | event | caller | actions | worker |
    /// |---|---|---|---|
    /// | a work turn ends | `prompt` | kept | **kept** |
    /// | the CEO stops | `NativeCancelHandle::cancel` → `ActionGrant::set(false)` | closed | withdrawn |
    /// | the assignment ends | `work_host`'s `run_one` step 4 → `revoke_work_assignment` | closed | withdrawn |
    #[test]
    fn a_turn_ending_keeps_the_grant_a_background_helper_acts_under_and_a_stop_takes_it() {
        let frame = work_init_frame(true, true);
        let (mut lease, root) = lease_with_production_grants(
            "worker-grant", LeaseRole::Work, &one_turn_reporting(&frame));
        let scope = lease.continuity.as_ref().unwrap().1.clone();
        let read = |path: &std::path::Path| -> (bool, bool) {
            let value: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            (value["actions_allowed"] == true, value["background_work_allowed"] == true)
        };
        // The standing §5.4 grant, in the shape `bind_work_assignment` writes it: the turn's
        // permission and the permission its already-dispatched helpers act under, together.
        let mut bound: crate::ecs::ToolScope =
            serde_json::from_slice(&std::fs::read(&scope).unwrap()).unwrap();
        bound.actions_allowed = true;
        bound.background_work_allowed = true;
        crate::ecs::write_scope(&scope, &bound).unwrap();

        assert_eq!(
            crate::cognition::Cognition::prompt(&mut lease, "do the job", &mut |_| {}).unwrap(),
            "end_turn"
        );
        assert_eq!(read(&scope), (true, true),
            "the turn's end took the helper's permission with it — the helper is dispatched \
             during a turn and does all of its work after that turn has ended");

        // The CEO's stop is a different event and takes both, in one write.
        ActionGrant::Continuity(scope.clone()).set(false).unwrap();
        assert_eq!(read(&scope), (false, false), "a stop left the helper still able to act");

        // And so does the end of the assignment, on a scope that is open on both counts.
        crate::ecs::write_scope(&scope, &bound).unwrap();
        assert_eq!(read(&scope), (true, true));
        crate::cognition::Cognition::revoke_work_assignment(&mut lease).unwrap();
        assert_eq!(read(&scope), (false, false),
            "the assignment ended with its helpers still permitted to act");

        drop(lease);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// **THE SEAT SPELLING, AND THAT NOTHING INVENTS IT** — the app derives his per-thread
    /// cursor and the engine derives the same string from the row's own thread, so this is
    /// the app's half of a name that is stored nowhere.
    ///
    /// The engine's own derivation is `CEO_SEAT_PREFIX + thread_id`
    /// (`engine/ecs/core/ecs_core.py:181-193`), and its `is_ceo_row` re-derives it from the
    /// row and requires the `ceo` audience (`:196-213`) — so a seat spelled differently
    /// here would not be refused at the bind, it would be accepted and then fail at the
    /// first checkpoint as though it belonged to somebody else.
    #[test]
    fn his_seat_is_derived_from_the_thread_and_never_from_a_stored_mapping() {
        assert_eq!(crate::ecs::CEO_SEAT_PREFIX, "ceo-thread:");
        assert_eq!(crate::ecs::ceo_seat("thread-one").as_deref(), Some("ceo-thread:thread-one"));
        // The legacy single cursor still exists and is still his — an engine without
        // per-thread seats, or a thread id the engine could not accept, binds it.
        assert_eq!(crate::entity::PERSON_DEFAULT, "ceo-default");
        assert_eq!(crate::ecs::ceo_seat(""), None);
        assert_eq!(crate::ecs::ceo_seat("   "), None);
        assert_eq!(crate::ecs::ceo_seat(&"t".repeat(1100)), None);
    }

    /// **SEAM 2**, the one that fires first, because it is the HOST calling `brief` rather
    /// than the model asking for it: `prepare_work_turn` is the only non-test writer of an
    /// ECS tool scope and calls `bridge.brief(&scope)` on its way there, which spec §5.8a
    /// reproduces raising on a work seat. A work lease must not come down that path at all.
    ///
    /// Asserted without a child process by driving the role gate directly. The positive
    /// control is the other arm: `bind_work_assignment` is refused on a conversation lease,
    /// so neither role can quietly take the other's path.
    #[test]
    fn neither_lease_can_take_the_others_preparation_path() {
        // The refusals are decided before anything else in either function, so a lease
        // value with no live child is enough to exercise exactly the gate under test.
        assert_eq!(
            work_preparation_refusal(LeaseRole::Work),
            Some("A background work connection does not take conversation turns.")
        );
        assert_eq!(work_preparation_refusal(LeaseRole::Conversation), None);
        assert_eq!(
            assignment_binding_refusal(LeaseRole::Conversation),
            Some("The conversation's connection does not take background assignments.")
        );
        assert_eq!(assignment_binding_refusal(LeaseRole::Work), None);
    }

    /// A work lease assembled by hand, so the binding gate can be driven without the real
    /// engine delivery `start_work_lease` needs (a Python supervisor, a rendered profile).
    /// Everything the gate reads is here: the role, the continuity scope, the engine
    /// profile, and a client whose reader state is whatever its scripted child has said.
    fn hand_built_work_lease(tag: &str, script_body: &str) -> (NativeCognition, std::path::PathBuf) {
        let root = fixture_root().join(format!("work-bind-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let script = write_script(&format!("work-bind-{tag}"), script_body);
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture())
            .expect("the handshake should succeed");
        let session_id = client.session_id().to_string();
        let lease = NativeCognition {
            client,
            session_id,
            onboarding_scope: None,
            assignments_scope: None,
            status_scope: None,
            continuity: Some((fixture_bridge(&root), root.join("work-scope.json"))),
            continuity_tools_scope: None,
            work_binding: None,
            engine_profile: Some(fixture_profile(&root)),
            role: LeaseRole::Work,
            ceo_thread_seats: None, classifier: None, routing_context: String::new(), app_messages: Vec::new(), prepared_scope: None, company_context: None, prepare_turn_supported: false, pending_onboarding: None, pending_channel: None, title_writer: None,
        };
        (lease, root)
    }

    fn fixture_assignment() -> crate::cognition::WorkAssignment {
        crate::cognition::WorkAssignment {
            entity_id: "qa-co".into(),
            thread_id: "thr_one".into(),
            assignment_id: "assign-1".into(),
            obligation_id: "ob-1".into(),
            seat: "work-seat:ob-1".into(),
            instruction_ledger_ref: "ledger:thr_one:turn_one".into(),
            instruction_sha256: "0".repeat(64),
            instruction_text: None,
        }
    }

    /// A child that answers the handshake and then says nothing. A lease that has run no
    /// turn, which is every work lease at the moment its assignment is bound.
    const SILENT_AFTER_HANDSHAKE: &str = "read -r line\n\
         printf '%s\\n' '{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"req_init\",\"response\":{}}}'\n\
         sleep 5\n";

    /// **The native half of stopping a watched job's command** (hunt 2026-09-29 part 1,
    /// finding 11; `work_host.rs` drives it through `TurnCancel::stop_background_command`).
    /// A stand-in child records every line it is sent after the handshake. Stopping one
    /// background command sends exactly one frame, `control_request{stop_task}` for that task
    /// id (the frame `agent_tasks::stop_task_request` builds, P12), and no `interrupt`: no
    /// turn is ended by it.
    #[test]
    fn stopping_one_background_command_sends_stop_task_for_it_and_nothing_else() {
        let root = fixture_root().join(format!("stop-task-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let heard = root.join("heard.jsonl");
        let script = write_script(
            "stop-task",
            &format!(
                "read -r line\n\
                 printf '%s\\n' '{{\"type\":\"control_response\",\"response\":{{\"subtype\":\"success\",\"request_id\":\"req_init\",\"response\":{{}}}}}}'\n\
                 while read -r line; do printf '%s\\n' \"$line\" >> '{}'; done\n",
                heard.display()
            ),
        );
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture())
            .expect("the handshake should succeed");
        // The stand-in never answers, so the stop waits on its own thread and says so:
        // delivered, nothing confirmed (finding 11 v2).
        let handle = client.cancel_handle();
        let stopping = std::thread::spawn(move || handle.stop_background_command("bk1c0clka"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let frames: Vec<Value> = loop {
            let text = std::fs::read_to_string(&heard).unwrap_or_default();
            if text.contains("stop_task") && text.ends_with('\n') {
                break text.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
            }
            assert!(std::time::Instant::now() < deadline, "the child never heard the stop: {text:?}");
            // load-bound: a poll for the line the child writes; the verdict is read from those frames, and the 10 s deadline only turns a hang into a failure.
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let stops: Vec<&Value> = frames.iter().filter(|f| f["type"] == "control_request").collect();
        assert_eq!(stops.len(), 1, "exactly one control request: {frames:?}");
        assert_eq!(stops[0]["request"], json!({"subtype": "stop_task", "task_id": "bk1c0clka"}));
        assert!(stops[0]["request_id"].as_str().is_some_and(|id| id.starts_with("richos_stop_task_")), "{frames:?}");
        // The child going away ends the wait at once, and still claims nothing.
        drop(client);
        assert_eq!(stopping.join().unwrap(), crate::steering::CommandStop::Unconfirmed,
                   "a stop nobody answered was reported as something other than unknown");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// **A stop is what the provider said about it, never that the line was written** (hunt
    /// 2026-09-29 part 1 v2, finding 11; the witness is `part-1-codex-v2/remaining-witness-v2.rs`
    /// in richos-hq, ported here). One provider refuses the stop and the command keeps running;
    /// another accepts it and reports the command's ending.
    ///
    /// **RED at `c6cde6cc9`**: `stop_background_command` returned `true` for the refused stop
    /// (the write succeeded), the refusal sat unread on the between-turn lane, and the work
    /// host told him the command had stopped.
    #[test]
    fn a_refused_stop_is_refused_and_only_a_reported_ending_is_a_stopped_command() {
        use crate::steering::CommandStop;
        let provider = |name: &str, answer: &str| write_script(name, &format!(r#"
read -r init
printf '%s\n' '{{"type":"control_response","response":{{"subtype":"success","request_id":"req_init","response":{{}}}}}}'
read -r prompt
printf '%s\n' '{{"type":"system","subtype":"task_started","task_id":"remaining-command","description":"fixture command","is_backgrounded":true,"task_type":"local_bash"}}'
printf '%s\n' '{{"type":"result","subtype":"success","stop_reason":"end_turn"}}'
read -r stop
id=$(printf '%s' "$stop" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
case "$stop" in *stop_task*) ;; *) exit 9;; esac
{answer}
read -r keep_alive
"#));
        let refusing = provider("refusing-stop", r#"printf '%s\n' "{\"type\":\"control_response\",\"response\":{\"subtype\":\"error\",\"request_id\":\"$id\",\"error\":\"fixture refuses task stop\"}}""#);
        let client = NativeClient::spawn(&refusing, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        client.prompt("start fixture", &mut |_| {}).unwrap();
        assert_eq!(client.background_commands().len(), 1);
        assert_eq!(client.cancel_handle().stop_background_command("remaining-command"),
                   CommandStop::Refused("fixture refuses task stop".into()),
                   "a stop the provider refused was reported as reaching the command");
        assert!(client.background_commands()[0].ended.is_none());
        drop(client);

        let accepting = provider("accepting-stop", r#"printf '%s\n' "{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"$id\",\"response\":{}}}"
printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"remaining-command","status":"stopped","summary":"fixture command stopped"}'"#);
        let client = NativeClient::spawn(&accepting, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        client.prompt("start fixture", &mut |_| {}).unwrap();
        assert_eq!(client.cancel_handle().stop_background_command("remaining-command"), CommandStop::Ended);
    }

    /// **THE DEFECT CANDIDATE .7 FOUND, AS A TEST — and it is a false NEGATIVE, not a
    /// conservative reading.**
    ///
    /// `bind_work_assignment` runs before the work lease's first turn (`work_host.rs`'s
    /// `run_one`: `ensure_lease`, then bind, then the turn), and `child_args`'s own doc says
    /// the init frame *"arrives with the first TURN"*. So at the bind the child has reported
    /// nothing, and a gate that read `false` for "nobody has told us" refused **every**
    /// background job on every machine: two assignments failed this way on 2026-09-18 at
    /// 6.545 s and 7.486 s after registration, `work_session: null` on both.
    ///
    /// The lease is refused here for the reason it SHOULD be — the fixture bridge cannot
    /// hold a work seat — and that positive assertion is what makes the three negatives
    /// evidence rather than a gate that moved somewhere else.
    #[test]
    fn a_fresh_work_lease_is_never_refused_over_a_fact_its_child_has_not_reported_yet() {
        let (mut lease, root) = hand_built_work_lease("fresh", SILENT_AFTER_HANDSHAKE);
        let refusal = lease
            .bind_work_assignment(&fixture_assignment())
            .expect_err("the fixture bridge cannot hold a seat, so this must still refuse")
            .to_string();
        for unreported in [
            "The desktop engine plugin did not load",
            "The desktop work tools did not load",
        ] {
            assert!(
                !refusal.contains(unreported),
                "a lease that has run no turn was refused over a fact nobody has reported: {refusal}"
            );
        }
        assert!(
            refusal.contains("overwrite the CEO's own"),
            "the binding must reach the seat bind, or the negatives above prove nothing: {refusal}"
        );
        // And the unreported facts are still unreported — the gate passed because it asked
        // about a reported absence, not because the fields were quietly made optimistic.
        let state = lease.client.reader_state.lock().unwrap();
        for fact in [state.engine_plugin_loaded, state.work_tools_loaded] {
            assert_eq!(fact, InitFact::NotYetReported);
        }
        drop(state);
        drop(lease);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A child that answers the handshake, then answers ONE turn with `init` and a result.
    fn one_turn_reporting(init: &serde_json::Value) -> String {
        format!(
            "read -r line\n\
             printf '%s\\n' '{{\"type\":\"control_response\",\"response\":{{\"subtype\":\"success\",\"request_id\":\"req_init\",\"response\":{{}}}}}}'\n\
             read -r line\n\
             printf '%s\\n' '{init}'\n\
             printf '%s\\n' '{{\"type\":\"result\",\"subtype\":\"success\",\"stop_reason\":\"end_turn\"}}'\n\
             while read -r line; do :; done\n"
        )
    }

    fn work_init_frame(plugin: bool, work_tools: bool) -> serde_json::Value {
        let mut plugins = vec![json!({"name":"rich-skills","version":"1.0.0"})];
        if plugin {
            plugins.push(json!({"name": crate::engine_profile::PLUGIN_NAME, "version":"1.2.0"}));
        }
        let tools: Vec<&str> = if work_tools {
            vec!["mcp__richos_work__repositories", "mcp__richos_work__prepare",
                 "mcp__richos_work__inspect", "mcp__richos_work__integrate", "mcp__richos_work__complete"]
        } else {
            // Four of the five. A partial inventory must read as absent, not as present.
            vec!["mcp__richos_work__repositories", "mcp__richos_work__prepare",
                 "mcp__richos_work__inspect", "mcp__richos_work__integrate"]
        };
        json!({"type":"system","subtype":"init","plugins":plugins,"tools":tools})
    }

    /// The plugin's name is written by `engine_profile.rs` into the manifest and asserted by
    /// this file against the wire. Two literals would be a schedule for them to disagree, and
    /// the disagreement is silent in the direction that matters: the check could only ever
    /// answer "absent", about a plugin sitting right there. That is not hypothetical — a
    /// readiness check answering a false absence is what refused every background job on
    /// 2026-09-18.
    #[test]
    fn the_engine_plugin_is_named_once_and_the_manifest_and_the_wire_check_share_it() {
        assert_eq!(crate::engine_profile::PLUGIN_NAME, "richos-app-engine");
        let listed = work_init_frame(true, true);
        let names: Vec<&str> = listed["plugins"].as_array().unwrap().iter()
            .filter_map(|p| p["name"].as_str()).collect();
        assert!(names.contains(&crate::engine_profile::PLUGIN_NAME), "{names:?}");
    }

    /// A work seat is never the CEO's, and is never bound against an engine that would
    /// silently write his row instead (spec §5.8/§5.8c). Both refusals are checked, and the
    /// engine-capability one is checked against a bridge that cannot answer at all — which
    /// is the ambiguity case, and it resolves to refusing.
    #[test]
    fn a_work_seat_is_refused_rather_than_written_onto_the_ceos_row() {
        let root = fixture_root().join(format!("seat-refusal-{}", uuid::Uuid::new_v4()));
        let bridge = fixture_bridge(&root);
        let ceo = bridge.bind_work_seat("depot", "thread", "session", "assign-7", crate::entity::PERSON_DEFAULT);
        assert!(ceo.is_err(), "a work seat was allowed to be the CEO's own");
        let shaped = bridge.bind_work_seat("depot", "thread", "session", "assign-7", "work seat with spaces");
        assert!(shaped.is_err());
        // The bridge points at nothing, so the capability probe cannot answer — and an
        // unanswerable probe refuses rather than binding.
        assert!(!bridge.supports_work_seats());
        let unknown = bridge.bind_work_seat("depot", "thread", "session", "assign-7", "work-seat-assign-7");
        assert!(unknown.is_err(), "a seat was bound against an engine that cannot hold one");
        assert!(unknown.unwrap_err().to_string().contains("overwrite the CEO's own"));
    }

    // ---- the undocumented flag, and the loud-failure contract -------------------------

    #[test]
    fn args_always_carry_the_permission_flag() {
        // THE structural guarantee §16 demands: there is no code path that drops
        // `--permission-prompt-tool stdio` and continues. `child_args` is a pure function
        // with no branches, so proving it here proves it everywhere.
        let args = child_args("sess-1");
        let i = args.iter().position(|a| a == PERMISSION_PROMPT_TOOL).expect("the flag must be present");
        assert_eq!(args[i + 1], "stdio", "and it must be armed with `stdio`");
        // The flags the rest of this file depends on, pinned so a tidy-up cannot quietly
        // remove one. Each has a stated reason on `child_args`.
        for required in [
            "--print",
            "--input-format=stream-json",
            "--output-format=stream-json",
            "--include-partial-messages",
            "--verbose",
            "--session-id",
        ] {
            assert!(args.iter().any(|a| a == required), "missing {required}");
        }
        assert!(!args.iter().any(|a| a == "--no-session-persistence"),
            "Rich and Stu need their native transcripts for shared inspection");
        // The session id is OURS and is passed through, which is what makes `session_id()`
        // answerable before the first turn.
        let j = args.iter().position(|a| a == "--session-id").unwrap();
        assert_eq!(args[j + 1], "sess-1");
        // And no environment variable can turn the flag off: `child_args` reads none.
        let _env = EnvSet::empty(&["RICHOS_PERMISSION_PROMPT_TOOL"]);
        assert!(child_args("x").iter().any(|a| a == PERMISSION_PROMPT_TOOL));
    }

    // ---- the standing instruction (doctrine.rs, inner-doctrine design §7.1/§7.2) --------

    #[test]
    fn chat_args_always_carry_the_doctrine_flag() {
        // The same structural guarantee as the permission flag, for the same reason: there is
        // no code path that drops `--append-system-prompt-file` and continues. `chat_child_args`
        // is a pure function with no branches, so proving it here proves it everywhere.
        let doctrine = Path::new("/Users/example/Library/Application Support/com.richos.app/inner-doctrine.md");
        let skills = Path::new("/Users/example/Library/Application Support/com.richos.app/rich-skills");
        let args = chat_child_args("sess-1", doctrine, skills);
        let i = args
            .iter()
            .position(|a| a == APPEND_SYSTEM_PROMPT_FILE)
            .expect("the chat lease must always carry the standing instruction");
        assert_eq!(args[i + 1], doctrine.display().to_string(), "and it must name the rendered file");

        // The base vector survives underneath it — a doctrine that arrived by dropping the
        // permission channel would be a bad trade.
        assert!(args.iter().any(|a| a == PERMISSION_PROMPT_TOOL));
        let j = args.iter().position(|a| a == "--setting-sources").unwrap();
        assert_eq!(args[j + 1], "", "--setting-sources '' stays exactly as it was (native.rs:316-317)");

        // A path with spaces in it needs no treatment at all: `Command::args` hands a vector
        // to `execve` with no shell in between. `Application Support` has a space in it, so
        // this is the production case and not a hypothetical one.
        assert!(args[i + 1].contains("Application Support"));

        // And no environment variable can turn it off: `chat_child_args` reads none.
        let _env = EnvSet::empty(&["RICHOS_APPEND_SYSTEM_PROMPT_FILE", "RICHOS_DOCTRINE"]);
        assert!(chat_child_args("x", doctrine, skills).iter().any(|a| a == APPEND_SYSTEM_PROMPT_FILE));
    }

    #[test]
    fn the_conversation_asks_for_resident_tools_and_the_work_lease_asks_for_nothing() {
        // THE PIN FOR `TOOL_SEARCH_ENV`. A pure function, so this proves it for every spawn
        // rather than for one: `spawn_with_tools` has no branch that can drop it.
        assert_eq!(
            tool_residency_env(LeaseRole::Conversation),
            Some(("ENABLE_TOOL_SEARCH", "false")),
            "the front desk must not have to DISCOVER the register before it can call it"
        );
        // Spelled out rather than compared to the constants, because the value is the one
        // thing here the shipped binary parses: `To()` accepts 0/false/no/off and nothing
        // else, so a value edited to `"0"` is fine and one edited to `"off "` with a space is
        // too (it trims) — but one edited to `"disabled"` would silently mean DEFERRAL ON.
        assert_eq!(TOOL_SEARCH_OFF, "false");
        assert_eq!(
            tool_residency_env(LeaseRole::Work),
            None,
            "nothing has been measured on the work lease, so nothing is changed there"
        );
    }

    #[test]
    fn every_lease_waits_longer_before_retrying_an_overloaded_request() {
        assert!(lease_retry_env().contains(&("CLAUDE_CODE_OVERLOADED_RETRY_BASE_DELAY_MS", "2000")));
        // 10 retries, delay = min(32000, base * 2^(n-1)): the minimum total, in ms.
        let total: u64 = (1..=10u32).map(|n| (2000u64 << (n - 1)).min(32000)).sum();
        assert_eq!(total, 222_000);
    }

    #[test]
    fn every_lease_retries_fifteen_times_so_an_internet_cutoff_of_minutes_is_ridden_out() {
        assert!(lease_retry_env().contains(&("CLAUDE_CODE_MAX_RETRIES", "15")));
        // A connection error: base 500, cap 32000, 15 retries: the minimum total, in ms.
        let total: u64 = (1..=15u32).map(|n| (500u64 << (n - 1)).min(32000)).sum();
        assert_eq!(total, 319_500);
    }

    #[test]
    fn the_work_lease_deferring_its_tools_is_never_reported_as_a_broken_knob() {
        // **RED-FIRST PROOF of the defect measured on candidate .10.** Before this fix there
        // was one message for both leases, so this exact call produced the sentence below,
        // about a variable that is not set on this lease and a binary that was working. Ray's
        // audit read it out of `app.log` and named it the cause of an 8-12 s first reply
        // (rows 1 and 4), and the brief that followed inherited the cause.
        let work = tool_search_notice(None);
        assert!(!work.contains(TOOL_SEARCH_ENV),
            "a lease that sets no variable must not name one: {work}");
        assert!(!work.contains("no longer having that effect"),
            "a lease that sets no variable cannot have lost an effect it never had: {work}");
        assert!(!work.contains("the CEO waits"),
            "nobody is sitting in front of a work lease's first token: {work}");
        assert!(work.contains("background work") && work.contains("expected"),
            "the true reading has to be legible to whoever finds this line: {work}");

        // THE POSITIVE CONTROL. The real alarm is unchanged and still able to be loud, which
        // is what stops this fix from being "delete the warning".
        let desk = tool_search_notice(Some(TOOL_SEARCH_ENV));
        assert!(desk.contains("discovery is unexpectedly required"), "{desk}");
        assert!(desk.contains("ENABLE_TOOL_SEARCH=false"), "{desk}");
        assert!(desk.contains("Check the installed provider version"), "{desk}");
        assert!(desk.contains(TOOL_SEARCH_TOOL), "{desk}");

        // The two messages cannot be confused for one another by whoever greps for the alarm.
        assert_ne!(work, desk);
    }

    #[test]
    fn the_reader_is_told_which_lease_it_is_on() {
        // The pure functions above prove the two messages. This proves the WIRING — that the
        // field is actually set at spawn, from `tool_residency_env` and not from a second
        // opinion — on a real `spawn_with_tools`, one per role. A pure-function pin alone
        // would have passed on candidate .10's code, where the field did not exist at all.
        let handshake = "read -r line\n\
             printf '%s\\n' '{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"req_init\",\"response\":{}}}'\n\
             sleep 5\n";

        let desk = NativeClient::spawn_with_tools(
            &write_script("residency-desk", handshake), Path::new("/tmp"),
            Some((doctrine_fixture().as_path(), skills_fixture().as_path())),
            None, None, None, None, None, LeaseRole::Conversation,
        ).expect("the handshake should succeed");
        assert_eq!(desk.reader_state.lock().unwrap().tool_residency, Some(TOOL_SEARCH_ENV),
            "the front desk sets the variable, so its reader must be able to say so");

        let work = NativeClient::spawn_with_tools(
            &write_script("residency-work", handshake), Path::new("/tmp"),
            Some((doctrine_fixture().as_path(), skills_fixture().as_path())),
            None, None, None, None, None, LeaseRole::Work,
        ).expect("the handshake should succeed");
        assert_eq!(work.reader_state.lock().unwrap().tool_residency, None,
            "the work lease sets nothing, and its reader must not claim otherwise");
    }

    /// **A teammate in the app can reach every skill its definition tells it to use** (CEO,
    /// 2026-10-10: the audio and video skill use must be part of all the teammate and
    /// proto-teammate profiles in RichOS, "otherwise a RichOS user wouldn't take advantage of
    /// that").
    ///
    /// A teammate runs as a helper inside the WORK lease (`start_work_lease`, then the `Agent`
    /// call `richos_work`'s `prepare` hands it), so the skills it can invoke are that session's.
    /// This spawns a real Work-role lease, reads the argument vector the child actually got,
    /// and builds the session's `system/init.skills` list from it the way the binary does:
    /// every `--plugin-dir`'s manifest name, then `:<skill directory>` for each skill in it
    /// (measured cell K2, `skills.rs`; re-measured on 2.1.295 for this change). Every
    /// definition that reaches an app teammate (the stock `engine/agents/`, the shelf
    /// `engine/team/shelf/`, the three duty texts) carries the media-skills sentence
    /// (`engine_profile::MEDIA_SKILLS_SENTENCE`), and every skill it names in backticks must be
    /// on that list.
    #[test]
    fn a_teammate_s_session_lists_every_skill_its_definition_names() {
        let argv = fixture_root().join(format!("argv-{}", uuid::Uuid::new_v4().simple()));
        let script = format!(
            "printf '%s\\n' \"$@\" > '{}'\n\
             read -r line\n\
             printf '%s\\n' '{{\"type\":\"control_response\",\"response\":{{\"subtype\":\"success\",\"request_id\":\"req_init\",\"response\":{{}}}}}}'\n\
             sleep 5\n", argv.display());
        let _work = NativeClient::spawn_with_tools(
            &write_script("teammate-skills", &script), Path::new("/tmp"),
            Some((doctrine_fixture().as_path(), skills_fixture().as_path())),
            None, None, None, None, None, LeaseRole::Work,
        ).expect("the handshake should succeed");
        let args: Vec<String> = std::fs::read_to_string(&argv).unwrap().lines().map(String::from).collect();

        let mut init_skills = Vec::new();
        for (i, flag) in args.iter().enumerate() {
            if flag != PLUGIN_DIR { continue; }
            let root = Path::new(&args[i + 1]);
            let manifest: Value = serde_json::from_str(
                &std::fs::read_to_string(crate::skills::manifest_path(root)).unwrap()).unwrap();
            let plugin = manifest["name"].as_str().unwrap().to_string();
            for entry in std::fs::read_dir(root.join("skills")).unwrap() {
                let dir = entry.unwrap().path();
                if dir.join("SKILL.md").is_file() {
                    init_skills.push(format!("{plugin}:{}", dir.file_name().unwrap().to_string_lossy()));
                }
            }
        }
        assert!(!init_skills.is_empty(), "the work lease was given no skills at all: {args:?}");

        for name in ["watch-video", "audio-transcription"] {
            let want = format!("{}:{name}", crate::skills::PLUGIN_NAME);
            assert!(init_skills.contains(&want),
                "every teammate is told to use `{name}`, and the teammate's session lists only {init_skills:?}");
        }
        let engine = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine");
        let mut checked = 0;
        for folder in ["agents", "team/shelf", "mega-lander/duties"] {
            for entry in std::fs::read_dir(engine.join(folder)).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_none_or(|e| e != "md") { continue; }
                let body = std::fs::read_to_string(&path).unwrap();
                for name in ["watch-video", "audio-transcription"] {
                    assert!(body.contains(name), "{} does not describe {name}", path.display());
                }
                checked += 1;
            }
        }
        // The 5 stock definitions and the 3 duties, with the shelf beside them.
        assert!(checked > 8, "only {checked} definitions were read");
    }

    #[test]
    fn conversation_starts_at_low_effort_and_work_keeps_high_effort() {
        // CEO 2026-10-14: "Forget the effort level for now."
        // Read off the argument vector the child actually received, on a real
        // `spawn_with_tools`, one per role. The per-role Fable setting is
        // `each_role_keeps_its_model_with_low_effort_for_replies_and_high_for_work`
        // (`tests/engine_profile.rs`), through the same `lease_model_args`.
        let handshake = "for a in \"$@\"; do printf '%s\\n' \"$a\"; done > \"$(dirname \"$0\")/argv\"\n\
             read -r line\n\
             printf '%s\\n' '{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"req_init\",\"response\":{}}}'\n\
             sleep 5\n";
        for (tag, role, effort) in [("models-desk", LeaseRole::Conversation, "low"), ("models-work", LeaseRole::Work, "high")] {
            let script = write_script(tag, handshake);
            let _client = NativeClient::spawn_with_tools(
                &script, Path::new("/tmp"),
                Some((doctrine_fixture().as_path(), skills_fixture().as_path())),
                None, None, None, None, None, role,
            ).expect("the handshake should succeed");
            let argv: Vec<String> = std::fs::read_to_string(script.parent().unwrap().join("argv")).unwrap()
                .lines().map(String::from).collect();
            let at = argv.iter().position(|a| a == "--model")
                .unwrap_or_else(|| panic!("{role:?} was started without --model: {argv:?}"));
            assert_eq!(&argv[at..at + 4], ["--model", "opus", "--effort", effort], "{role:?}: {argv:?}");
            assert_eq!(argv.iter().filter(|a| *a == "--model" || *a == "--effort").count(), 2,
                "{role:?} names a model or an effort twice: {argv:?}");
        }
    }

    #[test]
    fn a_default_reader_cannot_manufacture_the_front_desk_s_alarm() {
        // The default is the answer for a reader NOBODY HAS TOLD, and the direction of that
        // default is a decision: `None` can only ever under-claim. Production overwrites it
        // from `tool_residency_env(role)` one statement after construction, which
        // `the_reader_is_told_which_lease_it_is_on` pins on a real spawn.
        assert_eq!(ReaderState::default().tool_residency, None);
    }

    #[test]
    fn a_deferring_session_is_read_off_the_child_s_own_inventory() {
        // The POSITIVE CONTROL for the loudness layer. The negative alone — no `ToolSearch`
        // in a frame — passes just as well when the reading is broken, which is the failure
        // `feedback_negative_tests_pass_for_wrong_reason` names.
        let deferring = json!({"type":"system","subtype":"init",
            "tools":["Read","Bash","ToolSearch","mcp__richos_assignments__record"]});
        assert_eq!(tool_search_from_init(&deferring), InitFact::Yes);

        let resident = json!({"type":"system","subtype":"init",
            "tools":["Read","Bash","mcp__richos_assignments__record","mcp__richos_continuity__checkpoint"]});
        assert_eq!(tool_search_from_init(&resident), InitFact::No);

        // **AND THE TRAP THIS FACT EXISTS FOR.** Both frames list the register, so every
        // init fact the app already reads is identical between them — `loaded_from_init` was
        // `true` on the very run whose model spent 7.5 s discovering that same tool.
        for frame in [&deferring, &resident] {
            assert!(crate::assignment_tools::loaded_from_init(frame),
                "the register is listed either way; that is why it cannot answer this question");
        }

        // A frame with no `tools` key at all is `No` and not a panic: it is a statement that
        // the child offered no discovery tool, which is what the reading claims.
        assert_eq!(tool_search_from_init(&json!({"type":"system","subtype":"init"})), InitFact::No);
    }

    #[test]
    fn the_doctrine_never_reaches_the_base_vector() {
        // Only the conversation argument vector carries the standing instruction.
        {
            let args=child_args("s");
            assert!(
                !args.iter().any(|a| a == APPEND_SYSTEM_PROMPT_FILE),
                "the doctrine leaked into a non-chat argument vector: {args:?}"
            );
        }
    }

    #[test]
    fn a_missing_doctrine_fails_loudly_and_names_the_path() {
        // THE FAILURE PATH THAT MAKES THIS CHANNEL WORTH CHOOSING. Under the rejected
        // `--setting-sources project` candidate an absent instruction file is a successful
        // handshake and a generic Claude; here it is a refusal, before the process exists.
        // The binary below is PRESENT and executable and the working directory EXISTS, so the
        // only fault is the doctrine — which is what makes the variant meaningful.
        let script = write_script("doctrine-missing", "exit 0\n");
        let absent = fixture_root().join(format!("richos-no-doctrine-{}.md", uuid::Uuid::new_v4().simple()));
        assert!(!absent.exists());

        let err = NativeClient::spawn(&script, Path::new("/tmp"), &absent, &skills_fixture())
            .err()
            .expect("a missing standing instruction must never be a degraded success");
        let msg = err.to_string();
        assert!(matches!(err, NativeError::DoctrineMissing { .. }), "{msg}");
        assert!(msg.contains(&absent.display().to_string()), "the failure must name the file: {msg}");
        assert!(msg.contains("not Rich"), "the failure must say what is at stake: {msg}");
        assert!(!msg.contains("claude binary was not found"), "reported as the wrong fault: {msg}");
    }

    #[test]
    fn an_empty_doctrine_is_a_refusal_and_not_an_empty_instruction() {
        // `claude` ACCEPTS a zero-byte file, so this one is ours to catch: it would hand the
        // CEO a generic assistant behind a clean handshake.
        let script = write_script("doctrine-empty", "exit 0\n");
        let empty = script.parent().unwrap().join("inner-doctrine.md");
        std::fs::write(&empty, b"").unwrap();
        let err = NativeClient::spawn(&script, Path::new("/tmp"), &empty, &skills_fixture()).err().expect("empty is not instruction");
        assert!(matches!(err, NativeError::DoctrineMissing { .. }), "{err}");
        assert!(err.to_string().contains("empty"), "{err}");
    }

    #[test]
    fn a_directory_where_the_doctrine_should_be_says_so_rather_than_saying_it_is_absent() {
        let script = write_script("doctrine-is-a-dir", "exit 0\n");
        let dir = script.parent().unwrap().join("inner-doctrine.md.dir");
        std::fs::create_dir_all(&dir).unwrap();
        let err = NativeClient::spawn(&script, Path::new("/tmp"), &dir, &skills_fixture()).err().expect("a directory is not a doctrine");
        assert!(matches!(err, NativeError::DoctrineMissing { .. }), "{err}");
        assert!(err.to_string().contains("not a file"), "{err}");
    }

    // ---- the skills (skills.rs; measured in inner-doctrine-skills-2026-09-06/) -----------

    #[test]
    fn chat_args_always_carry_the_skills_flag() {
        let doctrine = Path::new("/Users/example/Library/Application Support/com.richos.app/inner-doctrine.md");
        let skills = Path::new("/Users/example/Library/Application Support/com.richos.app/rich-skills");
        let args = chat_child_args("sess-1", doctrine, skills);
        let i = args
            .iter()
            .position(|a| a == PLUGIN_DIR)
            .expect("the chat lease must always carry RichOS's own skills");
        assert_eq!(args[i + 1], skills.display().to_string());
        // The directory is the APP's, not the operator's `~/.claude` and not the visited
        // folder. The CEO ruled that authority comes from what RichOS writes.
        assert!(args[i + 1].contains("com.richos.app"), "{:?}", args[i + 1]);
        assert!(!args[i + 1].contains("/.claude/"), "the operator's own directory is not a source of authority");

        // And no environment variable turns it off: `chat_child_args` reads none.
        let _env = EnvSet::empty(&["RICHOS_PLUGIN_DIR", "RICHOS_SKILLS"]);
        assert!(chat_child_args("x", doctrine, skills).iter().any(|a| a == PLUGIN_DIR));
    }

    #[test]
    fn the_skills_never_reach_the_base_vector() {
        {
            let args=child_args("s");
            assert!(
                !args.iter().any(|a| a == PLUGIN_DIR),
                "RichOS's skills leaked into a non-chat argument vector: {args:?}"
            );
        }
    }

    /// **THE ONE THAT MATTERS MOST, because the binary will not do it for us.** Measured (cell
    /// K4): `--plugin-dir` naming a path that is not there exits 0 with a clean handshake and
    /// `plugins: []`. Without this check, deleting a skill file would be a product that is
    /// quietly less than it says it is.
    #[test]
    fn a_missing_skill_fails_loudly_and_names_the_file() {
        let script = write_script("skills-missing", "exit 0\n");
        let absent = fixture_root().join(format!("richos-no-skills-{}", uuid::Uuid::new_v4().simple()));
        assert!(!absent.exists());

        let err = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &absent)
            .err()
            .expect("a missing skill must never be a degraded success");
        let msg = err.to_string();
        assert!(matches!(err, NativeError::SkillsMissing { .. }), "{msg}");
        assert!(msg.contains("plugin.json"), "the failure must name the file it looked for: {msg}");
        assert!(
            msg.contains("without saying so"),
            "the failure must say WHY we check rather than leaving it to the binary: {msg}"
        );
        assert!(!msg.contains("claude binary was not found"), "reported as the wrong fault: {msg}");
    }

    #[test]
    fn a_skill_file_that_was_emptied_is_a_refusal_too() {
        let script = write_script("skills-empty", "exit 0\n");
        let root = skills_fixture();
        std::fs::write(crate::skills::skill_path(&root, "american-english"), b"").unwrap();
        let err = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &root)
            .err()
            .expect("an empty skill is no skill");
        assert!(matches!(err, NativeError::SkillsMissing { .. }), "{err}");
        assert!(err.to_string().contains("empty"), "{err}");
    }

    /// The second layer, in a unit test rather than a live one: a lease that has run no turn
    /// has heard nothing, and that is its own state.
    #[test]
    fn a_fresh_lease_reports_the_skills_verdict_as_not_yet_reported() {
        let script = write_script(
            "verdict",
            "read -r line\n\
             printf '%s\\n' '{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"req_init\",\"response\":{}}}'\n\
             sleep 5\n",
        );
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture())
            .expect("the handshake should succeed");
        assert_eq!(
            client.skills_verdict(),
            crate::skills::SkillsVerdict::NotYetReported,
            "the init frame arrives with the first TURN; before that we have been told nothing"
        );
    }

    #[test]
    fn the_real_reader_verifies_both_onboarding_tools_from_the_first_init() {
        use crate::onboarding_tools::{OnboardingToolsVerdict, QUALIFIED_SAVE_TOOL, QUALIFIED_DECLINE_TOOL};
        for (suffix, names, expected) in [
            ("present", vec![QUALIFIED_SAVE_TOOL, QUALIFIED_DECLINE_TOOL], OnboardingToolsVerdict::Loaded),
            ("missing-decline", vec![QUALIFIED_SAVE_TOOL], OnboardingToolsVerdict::Rejected),
            ("lookalike", vec!["mcp__other__save_company_notes", QUALIFIED_DECLINE_TOOL], OnboardingToolsVerdict::Rejected),
        ] {
            let init = json!({"type":"system","subtype":"init","tools":names,"plugins":[{"name":"rich-skills"}]}).to_string();
            let body = format!(
                "read -r line\nprintf '%s\\n' '{{\"type\":\"control_response\",\"response\":{{\"subtype\":\"success\",\"request_id\":\"req_init\",\"response\":{{}}}}}}'\n\
                 read -r line\nprintf '%s\\n' '{init}'\n\
                 printf '%s\\n' '{{\"type\":\"result\",\"subtype\":\"success\",\"stop_reason\":\"end_turn\"}}'\n\
                 while read -r line; do :; done\n"
            );
            let script = write_script(&format!("onboarding-tools-{suffix}"), &body);
            let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
            assert_eq!(client.onboarding_tools_verdict(), OnboardingToolsVerdict::NotYetReported);
            client.prompt("ready", &mut |_| {}).unwrap();
            assert_eq!(client.onboarding_tools_verdict(), expected);
        }
    }

    // ---- resolving `claude` itself (the 2026-09-17 candidate-walk defect) --------------
    //
    // Every test here drives `search_claude_bin` with an INJECTED `ClaudeBinEnv` rather than
    // mutating this process's real `$HOME`/`$HOMEBREW_PREFIX`/`~/.npmrc` — the same reason
    // `setup.rs::SetupPaths` is injected rather than read, and it is what keeps these
    // deterministic under `cargo test`'s parallel threads and honest about a machine that
    // happens to have a real Homebrew or npm install sitting on it.

    #[test]
    fn search_claude_bin_finds_a_homebrew_style_install() {
        let dir = fixture_root().join(format!("richos-claude-bin-brew-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let bin = dir.join("bin/claude");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let env = ClaudeBinEnv { homebrew_prefixes: vec![dir.display().to_string()], ..Default::default() };
        let found = search_claude_bin(&env).expect("a Homebrew-style install must resolve");
        assert_eq!(found, bin);
    }

    #[test]
    fn read_npmrc_prefix_finds_the_prefix_after_comments_blanks_and_other_keys() {
        let home = fixture_root().join(format!("richos-npmrc-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join(".npmrc"),
            "# my settings\n\n; another comment\nregistry=https://example.invalid/\nprefixes = nope\nprefix = \"/opt/npm-global\"\n",
        )
        .unwrap();
        assert_eq!(read_npmrc_prefix(&home.display().to_string()).as_deref(), Some("/opt/npm-global"));
        // Prefix-first still works, and a file with no prefix reads as absent.
        std::fs::write(home.join(".npmrc"), "prefix=/first\n").unwrap();
        assert_eq!(read_npmrc_prefix(&home.display().to_string()).as_deref(), Some("/first"));
        std::fs::write(home.join(".npmrc"), "# only a comment\nregistry=x\n").unwrap();
        assert_eq!(read_npmrc_prefix(&home.display().to_string()), None);
    }

    #[test]
    fn search_claude_bin_finds_an_npm_global_prefix_install() {
        let dir = fixture_root().join(format!("richos-claude-bin-npm-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let bin = dir.join("bin/claude");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let env = ClaudeBinEnv { npm_prefix: Some(dir.display().to_string()), ..Default::default() };
        let found = search_claude_bin(&env).expect("an npm global prefix install must resolve");
        assert_eq!(found, bin);
    }

    #[test]
    fn search_claude_bin_still_finds_the_installer_launcher_at_home_local_bin() {
        // POSITIVE CONTROL: the ordinary, already-working case, unchanged by everything else
        // added around it.
        let home = fixture_root().join(format!("richos-claude-bin-home-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(home.join(".local/bin")).unwrap();
        let bin = home.join(".local/bin/claude");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        let env = ClaudeBinEnv { home: Some(home.display().to_string()), ..Default::default() };
        let found = search_claude_bin(&env).expect("the installer's own launcher must still resolve");
        assert_eq!(found, bin);
    }

    #[test]
    fn search_claude_bin_lets_an_explicit_override_win_even_over_a_real_install() {
        let dir = fixture_root().join(format!("richos-claude-bin-explicit-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let winner = dir.join("winner");
        std::fs::write(&winner, "#!/bin/sh\n").unwrap();
        // A decoy that WOULD resolve if the override were not exclusive.
        let decoy_home = dir.join("home");
        std::fs::create_dir_all(decoy_home.join(".local/bin")).unwrap();
        std::fs::write(decoy_home.join(".local/bin/claude"), "#!/bin/sh\n").unwrap();
        let env = ClaudeBinEnv {
            explicit: Some(winner.display().to_string()),
            home: Some(decoy_home.display().to_string()),
            ..Default::default()
        };
        let found = search_claude_bin(&env).expect("the explicit override exists and must win");
        assert_eq!(found, winner);
    }

    #[test]
    fn search_claude_bin_refuses_by_name_when_found_nowhere() {
        let env = ClaudeBinEnv {
            home: Some("/nonexistent/richos-test-home".into()),
            homebrew_prefixes: vec!["/nonexistent/richos-test-homebrew".into()],
            npm_prefix: Some("/nonexistent/richos-test-npm".into()),
            ..Default::default()
        };
        let looked = search_claude_bin(&env).expect_err("nothing here should resolve");
        assert!(looked.iter().any(|p| p.contains("richos-test-home/.local/bin/claude")), "{looked:?}");
        assert!(looked.iter().any(|p| p.contains("richos-test-homebrew/bin/claude")), "{looked:?}");
        assert!(looked.iter().any(|p| p.contains("richos-test-npm/bin/claude")), "{looked:?}");
        assert_eq!(looked.len(), 3, "every place looked is named, and only those: {looked:?}");
    }

    #[test]
    fn resolve_claude_bin_checked_names_every_place_looked_when_nothing_is_found() {
        // Same claim through the public, `NativeError`-carrying entry point `main.rs` boot
        // actually calls — proven against a `ClaudeBinEnv` that cannot find its own explicit
        // override, so the exclusive-return path is exercised too.
        let looked = vec!["/a".to_string(), "/b".to_string()];
        let err = NativeError::ClaudeNotFound { looked: looked.join("; ") };
        let msg = err.to_string();
        assert!(msg.contains("/a; /b"), "{msg}");
        assert!(msg.contains("install Claude Code"), "{msg}");
    }

    #[test]
    fn a_missing_binary_fails_loudly_and_names_the_path() {
        let err = NativeClient::spawn(Path::new("/nonexistent/definitely/not/claude"), Path::new("/tmp"), &doctrine_fixture(), &skills_fixture())
            .err()
            .expect("a missing binary must not be a degraded success");
        match &err {
            NativeError::BinaryMissing { path } => assert!(path.contains("not/claude")),
            other => panic!("expected BinaryMissing, got {other:?}"),
        }
        // The message a human sees says what is wrong and why it matters.
        assert!(err.to_string().contains("cannot run without it"), "{err}");
    }

    #[test]
    fn a_missing_working_directory_is_never_reported_as_a_missing_binary() {
        // RED-FIRST PROOF of the defect measured on 2026-09-01: the binary below is PRESENT
        // and EXECUTABLE, and only the working directory is absent. `Command::current_dir`
        // on a missing directory fails ENOENT, and ENOENT was mapped to BinaryMissing — so
        // the app announced a missing binary while that binary sat on disk.
        let script = write_script("cwd-missing", "exit 0\n");
        assert!(script.exists(), "the binary must be present for this test to mean anything");
        let missing = fixture_root().join(format!("richos-no-such-engine-{}", uuid::Uuid::new_v4().simple()));
        assert!(!missing.exists());

        let err = NativeClient::spawn(&script, &missing, &doctrine_fixture(), &skills_fixture())
            .err()
            .expect("a missing working directory must not be a degraded success");
        let msg = err.to_string();
        assert!(!msg.contains("claude binary was not found"), "a missing working directory reported as a missing binary: {msg}");
        assert!(msg.contains(&missing.display().to_string()), "the failure must name the directory that is actually missing: {msg}");
        assert!(matches!(err, NativeError::WorkingDirMissing { .. }), "{msg}");
    }

    #[test]
    fn an_engine_path_that_is_a_file_says_so_rather_than_saying_it_is_absent() {
        // ENOTDIR, not ENOENT: the operator can SEE the path, so "it does not exist" would be
        // a false sentence about something in front of him.
        let script = write_script("cwd-not-a-dir", "exit 0\n");
        let file = script.parent().unwrap().join("engine-that-is-a-file");
        std::fs::write(&file, b"not a directory").unwrap();

        let err = NativeClient::spawn(&script, &file, &doctrine_fixture(), &skills_fixture()).err().expect("a file is not a working directory");
        let msg = err.to_string();
        assert!(matches!(err, NativeError::WorkingDirNotADirectory { .. }), "{msg}");
        assert!(msg.contains("is not a directory"), "{msg}");
        assert!(msg.contains(&file.display().to_string()), "{msg}");
    }

    #[test]
    fn a_binary_that_is_present_but_not_executable_is_not_reported_as_missing() {
        // The third fault the old single-variant mapping flattened: the file IS installed, so
        // "install Claude Code" is the wrong instruction and `chmod` is the right one.
        let script = write_script("not-executable", "exit 0\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        let err = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).err().expect("an unrunnable binary must fail");
        let msg = err.to_string();
        assert!(matches!(err, NativeError::BinaryNotExecutable { .. }), "{msg}");
        assert!(!msg.contains("was not found"), "a present file must never be reported as absent: {msg}");
        assert!(msg.contains("644"), "the diagnosis names the mode it actually found: {msg}");
    }

    #[test]
    fn with_both_missing_the_binary_is_named_first_and_that_order_is_deliberate() {
        // Multi-fault installs get the FIRST true sentence, not a false one: the 197 MB
        // install is upstream of the engine directory, so it is the one named. Pinned here so
        // the order is a decision on the record rather than an accident of code layout.
        let missing_dir = fixture_root().join(format!("richos-no-such-engine-{}", uuid::Uuid::new_v4().simple()));
        let err = NativeClient::spawn(Path::new("/nonexistent/definitely/not/claude"), &missing_dir, &doctrine_fixture(), &skills_fixture())
            .err()
            .expect("must fail");
        assert!(matches!(err, NativeError::BinaryMissing { .. }), "{err}");
    }

    #[test]
    fn a_child_that_rejects_the_flag_fails_loudly_and_quotes_its_own_stderr() {
        // The measured failure mode, reproduced without needing the real binary: on an
        // unknown option `claude` writes ONE line to stderr and exits 1 with zero bytes of
        // stdout (verified 2026-08-31 against 2.1.252). This script does exactly that.
        //
        // THE SENTENCE BELOW IS NOW STABLE, AND IT WAS NOT. Whether this process gets its
        // handshake into the pipe before a child this short-lived is torn down is a race
        // this machine wins and a GitHub runner lost — see `handshake()`. The product now
        // collapses both arms onto one diagnosis, so this assertion is a claim about the
        // product rather than about the scheduler. `write_failure_reason` is where both arms
        // are proved without needing to win anything.
        let script = write_script(
            "flag-reject",
            "echo \"error: unknown option '--permission-prompt-tool'\" >&2\nexit 1\n",
        );
        let err = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture())
            .err()
            .expect("a rejected flag must never be a silent degrade");
        let msg = err.to_string();
        assert!(matches!(err, NativeError::Startup { .. }), "{msg}");
        assert!(msg.contains("exited before answering the handshake"), "{msg}");
        // The child's own words, verbatim — on this failure that one line IS the diagnosis.
        assert!(msg.contains("unknown option '--permission-prompt-tool'"), "{msg}");
    }

    #[test]
    fn one_failed_handshake_write_gets_one_sentence_whichever_side_of_the_race_we_land_on() {
        // THE ARM A RUNNER TOOK AND THIS MACHINE NEVER DOES. The test above can only
        // exercise whichever side of the race it happens to land on, which is why it passed
        // here for days and failed on the first public runner. These three assertions need
        // no child, no timing and no luck.
        let pipe = NativeError::Io(std::io::Error::from(std::io::ErrorKind::BrokenPipe));

        // Broken pipe, child confirmed gone: the SAME words the read side uses, because it
        // is the same fact.
        assert_eq!(
            NativeClient::write_failure_reason(&pipe, true, true),
            "the child exited before answering the handshake"
        );

        // Broken pipe, child still up: a different fault, and it says so rather than
        // claiming an exit nobody observed.
        let live = NativeClient::write_failure_reason(&pipe, true, false);
        assert!(live.contains("closed its input"), "{live}");
        assert!(!live.contains("exited"), "an unobserved exit was asserted as fact: {live}");

        // Anything else keeps the error verbatim — collapsing every write failure onto one
        // sentence would be the opposite mistake.
        let other = NativeError::Io(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        let msg = NativeClient::write_failure_reason(&other, false, false);
        assert!(msg.starts_with("could not write the initialize handshake:"), "{msg}");
    }

    #[test]
    fn a_child_that_never_answers_the_handshake_times_out_rather_than_hanging() {
        // A binary that starts, holds stdio open and says nothing is the failure that would
        // otherwise hang the app forever at boot. Bounded by RICHOS_HANDSHAKE overridden
        // here through the script instead: the script exits after a moment, which is the
        // Disconnected arm; the Timeout arm is bounded by HANDSHAKE_TIMEOUT and is not worth
        // 30 s of test time to exercise.
        let script = write_script("silent", "sleep 0.3\nexit 0\n");
        let err = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).err().expect("silence is not success");
        assert!(matches!(err, NativeError::Startup { .. }), "{err}");
    }

    #[test]
    fn a_pending_initialize_handshake_is_cancellable() {
        let script = write_script("cancel-handshake", r#"
count=0
while IFS= read -r line; do
  case "$line" in
    *'"subtype":"initialize"'*)
      count=$((count + 1))
      if [ "$count" -eq 1 ]; then
        printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
      fi
      ;;
  esac
done
"#);
        let mut client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        let root = fixture_root().join(format!("richos-handshake-stop-{}", uuid::Uuid::new_v4()));
        let control = crate::steering::TurnControl::open(&root).unwrap().for_thread("thread");
        control.begin_turn(crate::steering::ActiveTurn {
            turn_id: "initial-request".into(), thread_id: "thread".into(),
            entity_id: None, started_at: None,
        });
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let worker_control = control.clone();
        let worker = std::thread::spawn(move || {
            entered_tx.send(()).unwrap();
            client.handshake_cancellable(Some(&worker_control))
        });
        // A hang guard on a thread starting, not a measure of anything.
        entered_rx.recv_timeout(std::time::Duration::from_secs(60)).unwrap();
        control.request_stop().unwrap();
        // The fixture never answers this second handshake, so the only other way out is
        // HANDSHAKE_TIMEOUT, which says "no answer to the initialize handshake". The error's
        // own words tell the stop from the timeout; a clock beside them (it was `< 2 s`)
        // only added a way to fail on a busy Mac (audit R9).
        let error = worker.join().unwrap().unwrap_err();
        assert!(error.to_string().contains("stopped at your request"), "{error}");
    }

    #[test]
    fn a_healthy_handshake_yields_a_client_whose_session_id_is_known_immediately() {
        // The session id is ours (`--session-id`), so it is answerable before any turn —
        // which is what `ledger.rs`'s rotation records need.
        let script = write_script(
            "ok",
            "read -r line\n\
             printf '%s\\n' '{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"req_init\",\"response\":{\"account\":{\"email\":\"x@y\"}}}}'\n\
             sleep 5\n",
        );
        let client = match NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()) {
            Ok(c) => c,
            Err(e) => panic!("handshake should have succeeded: {e}"),
        };
        assert_eq!(client.session_id().len(), 36, "a uuid, hyphenated, as --session-id requires");
        assert!(client.session_id().contains('-'));
    }

    /// A rendered standing instruction on disk, for the spawn tests.
    ///
    /// The REAL renderer, not a stub file: these tests then fail if `doctrine.rs` ever stops
    /// producing something `preflight` will accept, which is the only way the two halves of
    /// this feature can be checked against each other without a live binary.
    /// A rendered SKILL PLUGIN on disk, for the spawn tests. The real renderer again, so a
    /// `skills.rs` that stopped producing something `preflight` accepts breaks these too.
    /// **ONE directory for every fixture this module renders, removed when the test process
    /// exits** — the CEO's §54, measured rather than assumed.
    ///
    /// These three helpers each made a fresh sibling under the system temp directory and
    /// nothing ever removed it. Counted on 2026-09-18: **858 `richos-skills-fixture-*` and 818
    /// `richos-doctrine-fixture-*`** already standing in `$TMPDIR`, and six runs of
    /// `cargo test -p richos-core` on that day added **172 more**. A directory per fixture per
    /// run, kept forever, is exactly the class §54 is about: *"a ROCK-SOLID … mechanism that
    /// always guarantees that garbage like this will be always cleaned up afterwards."*
    ///
    /// **Why `atexit` and not a `Drop` guard.** The helpers hand back a `PathBuf` that
    /// forty-eight call sites pass by reference into a child process's arguments; a guard
    /// returned by value would be dropped at the end of the statement that spawned the child
    /// and delete the directory out from under it. `libc::atexit` needs no call-site change at
    /// all, and it runs once for the whole test binary.
    ///
    /// **Its honest limit, stated rather than discovered:** `atexit` does not run if the test
    /// process is killed or aborts. A killed run still leaves ONE directory instead of the
    /// ~thirty this module used to leave, which is a pile somebody can remove rather than a
    /// pile nobody can find.
    fn fixture_root() -> &'static std::path::Path {
        FIXTURE_ROOT.get_or_init(|| {
            let root = std::env::temp_dir()
                .join(format!("richos-core-test-fixtures-{}", std::process::id()));
            std::fs::create_dir_all(&root).expect("the fixture root must exist");
            #[cfg(unix)]
            unsafe {
                libc::atexit(remove_the_fixture_root);
            }
            root
        })
    }

    static FIXTURE_ROOT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

    /// Registered once by [`fixture_root`]. Best effort by construction — an `atexit` handler
    /// cannot report, and a failure here must never be mistaken for a test failure.
    #[cfg(unix)]
    extern "C" fn remove_the_fixture_root() {
        if let Some(root) = FIXTURE_ROOT.get() {
            let _ = std::fs::remove_dir_all(root);
        }
    }

    fn skills_fixture() -> std::path::PathBuf {
        let dir = fixture_root().join(format!("skills-{}", uuid::Uuid::new_v4().simple()));
        crate::skills::ensure_rendered(&dir, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).expect("the skills fixture must render")
    }

    fn doctrine_fixture() -> std::path::PathBuf {
        skills_fixture().join("agents/rich.md")
    }

    /// A [`NativeClient`] over a stand-in `claude` written from `body`, with the script's own
    /// directory for the files it writes: for tests elsewhere in the crate.
    pub(crate) fn fixture_client(name: &str, body: &str) -> (NativeClient, std::path::PathBuf) {
        let script = write_script(name, body);
        let client = NativeClient::spawn(&script, fixture_root(), &doctrine_fixture(), &skills_fixture()).unwrap();
        (client, script.parent().unwrap().to_path_buf())
    }

    fn write_script(name: &str, body: &str) -> std::path::PathBuf {
        let dir = fixture_root().join(format!("script-{}-{}", name, uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fake-claude");
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    /// A stand-in `claude` whose first turn ends on the API failure in `case.json`, in the
    /// frames 2.1.305 was captured sending (a refused localhost port, and a localhost stand-in
    /// answering the 2026-10-21 incident's Cloudflare `403`), and whose every later turn says
    /// `Carried on.` and ends. Every user message it reads is written to `received.jsonl`.
    const FAILS_ONCE_ON_THE_API: &str = r#"
import json,pathlib,sys
here=pathlib.Path(sys.argv[0]).parent
case=json.loads((here/"case.json").read_text())
def send(v): print(json.dumps(v),flush=True)
def read():
    line=sys.stdin.readline()
    if not line: raise SystemExit(0)
    v=json.loads(line)
    if v.get("type")=="user":
        with (here/"received.jsonl").open("a") as f: f.write(json.dumps(v)+"\n")
    return v
init=read()
send({"type":"control_response","response":{"subtype":"success","request_id":init["request_id"],"response":{}}})
first=True
while True:
    v=read()
    if v.get("type")!="user": continue
    sent=v["uuid"]
    send({"type":"command_lifecycle","command_uuid":sent,"state":"queued"})
    send({"type":"command_lifecycle","command_uuid":sent,"state":"started"})
    if first and case.get("helper"):
        first=False
        said={"type":"assistant","parent_tool_use_id":"helper-call","agent_id":"a1b2c3d4-helper","task_description":"Fixture helper",
            "error":case["kind"],"is_api_error_message":True,"message":{"model":"<synthetic>","role":"assistant","type":"message",
                "content":[{"type":"text","text":case["text"]}]}}
        if case["status"] is not None: said["api_error_status"]=case["status"]
        send(said)
        send({"type":"assistant","parent_tool_use_id":None,"message":{"role":"assistant","content":[{"type":"text","text":"Parent done."}]}})
        send({"type":"result","subtype":"success","is_error":False,"stop_reason":"end_turn","terminal_reason":"completed","queued_turn_count":0})
        send({"type":"command_lifecycle","command_uuid":sent,"state":"completed"})
    elif first:
        first=False
        said={"type":"assistant","parent_tool_use_id":None,"error":case["kind"],"is_api_error_message":True,
            "message":{"model":"<synthetic>","role":"assistant","type":"message","stop_reason":"stop_sequence",
                "content":[{"type":"text","text":case["text"]}]},"user_message_uuid":sent}
        if case["status"] is not None: said["api_error_status"]=case["status"]
        send(said)
        send({"type":"result","subtype":"success","is_error":True,"stop_reason":"stop_sequence","terminal_reason":"api_error",
            "api_error_status":case["status"],"result":case["text"],"user_message_uuid":sent,"queued_turn_count":0})
        send({"type":"command_lifecycle","command_uuid":sent,"state":"cancelled"})  # dialect-exempt: the vendor's own command_lifecycle state value, captured verbatim off the 2.1.305 wire
    else:
        send({"type":"assistant","parent_tool_use_id":None,"message":{"role":"assistant","content":[{"type":"text","text":"Carried on."}]}})
        send({"type":"result","subtype":"success","is_error":False,"stop_reason":"end_turn","terminal_reason":"completed","queued_turn_count":0})
        send({"type":"command_lifecycle","command_uuid":sent,"state":"completed"})
"#;

    /// The two endings of the 2026-10-21 incident: Claude Code's connection failure after its own
    /// retries, and the Cloudflare `403` that it never retries.
    const REFUSED: &str = "API Error: Connection refused \u{2014} a firewall or proxy may be blocking it (ECONNREFUSED)";
    const EDGE_403: &str = r#"Please run /login · API Error: 403 {"type":"https://developers.cloudflare.com/support/troubleshooting/http-status-codes/cloudflare-1xxx-errors/error-1034/","title":"Error 1034: Edge IP Restricted","status":403,"detail":"The host resolved to an IP address that the site owner does not have access to.","error_code":1034,"error_name":"edge_ip_restricted","error_category":"dns","zone":"api.anthropic.com","cloudflare_error":true,"retryable":false}"#;

    /// A lease on the stand-in for `(kind, status, text)`, whose connection check answers "down"
    /// `down` times and then "up"; returns it, its folder and how often the check was asked.
    fn lease_failing_once(name: &str, kind: &str, status: Option<u64>, text: &str, down: usize)
        -> (NativeClient, std::path::PathBuf, Arc<AtomicUsize>) {
        lease_failing_once_in(name, json!({"kind":kind,"status":status,"text":text}), down)
    }

    /// [`lease_failing_once`] for any `case.json`; `"helper": true` puts the failure on a helper
    /// of the turn, whose parent then ends its own turn normally.
    fn lease_failing_once_in(name: &str, case: Value, down: usize) -> (NativeClient, std::path::PathBuf, Arc<AtomicUsize>) {
        let script = write_script(name, "exec python3 -u \"$(dirname \"$0\")/provider.py\"\n");
        let root = script.parent().unwrap().to_path_buf();
        std::fs::write(root.join("provider.py"), FAILS_ONCE_ON_THE_API).unwrap();
        std::fs::write(root.join("case.json"), case.to_string()).unwrap();
        let client = NativeClient::spawn(&script, &root, &doctrine_fixture(), &skills_fixture()).unwrap();
        let asked = Arc::new(AtomicUsize::new(0));
        let counter = asked.clone();
        client.reader_state.lock().unwrap().connection = Arc::new(crate::connection::ConnectionWatch::new(
            move || counter.fetch_add(1, Ordering::SeqCst) >= down, Duration::from_millis(5)));
        (client, root, asked)
    }

    /// The user messages the stand-in read, as their texts.
    fn received_texts(root: &Path) -> Vec<String> {
        std::fs::read_to_string(root.join("received.jsonl")).unwrap_or_default().lines().map(|line| {
            let frame: Value = serde_json::from_str(line).unwrap();
            frame["message"]["content"][0]["text"].as_str().unwrap().to_string()
        }).collect()
    }

    /// **CEO, 2026-10-21: "Of course, that must be built into the app."** A turn the lost
    /// connection ended (Claude Code's connection failure after its retries ran out, or the
    /// incident's `403` arriving while the connection is down) waits until the connection is
    /// back and carries the SAME turn on, for Rich's prompt and for Stu's live input alike:
    /// it finishes once, the vendor's line never becomes the reply, and what the user sent is
    /// sent once, followed by one `connection_restored` message.
    #[test]
    fn a_turn_the_lost_connection_ended_waits_for_it_and_carries_on_once() {
        // The third case: an overload message that arrives while the connection is down marks a live
        // input overloaded; the continued turn must still end as its own (review
        // rv-20261021T031711Z-3416839f-d2f2).
        for (case, kind, status, text) in [("refused", "server_error", None, REFUSED), ("edge-403", "authentication_failed", Some(403), EDGE_403),
            ("repeated-529-while-down", "server_error", None, "API Error: Repeated 529 Overloaded errors.")] {
            for live_input in [false, true] {
                let (client, root, asked) = lease_failing_once(&format!("lost-connection-{case}"), kind, status, text, 3);
                let mut said = String::new();
                let outcome = if live_input {
                    let (done, finished) = std::sync::mpsc::channel();
                    let sent = client.cancel_handle().send_input(&uuid::Uuid::new_v4().to_string(), "Draft the plan",
                        Box::new(move |outcome| done.send(outcome.result).expect("the test waits for this outcome")));
                    assert_eq!(sent, Ok(true));
                    // load-bound: hang guard only; the input's own completion is the verdict.
                    finished.recv_timeout(Duration::from_secs(60)).expect("the live input never finished")
                } else {
                    client.prompt("Draft the plan", &mut |item| if let TurnItem::Text { text, .. } = item { said.push_str(text) })
                        .map_err(|error| error.to_string())
                };
                let what = format!("case={case} live_input={live_input}");
                assert_eq!(outcome.as_deref(), Ok("end_turn"), "{what}: the turn did not carry on to its end");
                if !live_input { assert_eq!(said, "Carried on.", "{what}: the vendor's line reached the reply"); }
                let received = received_texts(&root);
                assert_eq!(received.len(), 2, "{what}: {received:?}");
                assert_eq!(received[0], "Draft the plan", "{what}");
                let carried: Value = serde_json::from_str(&received[1]).unwrap();
                let fact: Value = serde_json::from_str(carried["text"].as_str().unwrap()).unwrap();
                assert_eq!((carried["from"].as_str(), fact["event"].as_str(), fact["error"].as_str()),
                    (Some("connection"), Some("connection_restored"), Some(text)), "{what}");
                assert!(asked.load(Ordering::SeqCst) >= 4, "{what}: carried on before the connection was back");
            }
        }
    }

    /// The control: the same `403` while the API answers is a real answer from the API, so the
    /// turn ends exactly as it did before, on the one message the user sent.
    ///
    /// **The vendor's line is the failure's reason, never Rich's reply** (2026-10-21): a new
    /// user's first message, signed out, showed "Rich: Not logged in · Please run /login" above
    /// the sign-in card on 1.2.1. Red before `end_turn` stopped delivering it on a failed turn.
    #[test]
    fn an_api_failure_while_the_connection_is_up_ends_the_turn_as_before() {
        const NOT_SIGNED_IN: &str = "Not logged in \u{b7} Please run /login";
        for (case, status, text, reason) in [("edge-403-up", Some(403), EDGE_403, "Error 1034"), ("signed-out", None, NOT_SIGNED_IN, "Not logged in")] {
            let (client, root, _) = lease_failing_once(case, "authentication_failed", status, text, 0);
            let mut said = String::new();
            let outcome = client.prompt("Draft the plan", &mut |item| if let TurnItem::Text { text, .. } = item { said.push_str(text) });
            assert!(outcome.expect_err("an API failure with the connection up must still fail").to_string().contains(reason), "{case}");
            assert_eq!(said, "", "{case}: the vendor's line reached Rich's reply");
            assert_eq!(received_texts(&root), ["Draft the plan"], "{case}");
        }
    }

    /// **A teammate the lost connection ended is carried on too** (review
    /// rv-20261021T021642Z-c50b0db8-452f; the incident's dead worker). Its parent's turn ends
    /// normally; once the connection is back the parent gets one `connection_restored` message
    /// naming that helper (its call, agent id and task), so it continues the helper's own
    /// conversation. Both of the incident's errors; nothing is sent while the connection is down.
    #[test]
    fn a_teammate_the_lost_connection_ended_is_named_to_its_parent_once_the_connection_is_back() {
        for (case, kind, status, text) in [("refused", "server_error", None, REFUSED), ("edge-403", "authentication_failed", Some(403), EDGE_403)] {
            let (client, root, asked) = lease_failing_once_in(&format!("lost-connection-helper-{case}"),
                json!({"kind":kind,"status":status,"text":text,"helper":true}), 3);
            let mut said = String::new();
            let outcome = client.prompt("Draft the plan", &mut |item| if let TurnItem::Text { text, .. } = item { said.push_str(text) });
            assert_eq!(outcome.map_err(|error| error.to_string()).as_deref(), Ok("end_turn"), "case={case}");
            assert_eq!(said, "Parent done.", "case={case}: a helper's failure line reached the parent's reply");
            // load-bound: hang guard only; the stand-in reading the second message is the verdict.
            let end = std::time::Instant::now() + Duration::from_secs(60);
            let mut received = received_texts(&root);
            while received.len() < 2 && std::time::Instant::now() < end {
                std::thread::sleep(Duration::from_millis(20));
                received = received_texts(&root);
            }
            assert_eq!(received.len(), 2, "case={case}: {received:?}");
            assert_eq!(received[0], "Draft the plan", "case={case}");
            let told: Value = serde_json::from_str(&received[1]).unwrap();
            let fact: Value = serde_json::from_str(told["text"].as_str().unwrap()).unwrap();
            assert_eq!((told["from"].as_str(), fact["event"].as_str(), fact["error"].as_str()),
                (Some("connection"), Some("connection_restored"), Some(text)), "case={case}");
            assert_eq!(fact["helper"], json!({"tool_use_id":"helper-call","agent_id":"a1b2c3d4-helper","task":"Fixture helper"}), "case={case}");
            assert!(asked.load(Ordering::SeqCst) >= 4, "case={case}: the parent was told before the connection was back");
        }
    }

    #[test]
    fn native_input_can_be_submitted_before_the_previous_response_is_drained() {
        let script=write_script("pending-input",r#"
exec python3 -u -c '
import json,sys
waiting=[]
def read(): return json.loads(sys.stdin.readline())
def send(frame): print(json.dumps(frame),flush=True)
def hook(command,expected):
    send({"type":"command_lifecycle","command_uuid":command["uuid"],"state":"started"})
    send({"type":"control_request","request_id":expected,"request":{"subtype":"hook_callback","callback_id":"richos-input-context","input":{"prompt_id":"native-"+command["uuid"],"hook_event_name":"UserPromptSubmit","prompt":command["message"]["content"][0]["text"]}}})
    while True:
        frame=read()
        if frame["type"]=="user":
            waiting.append(frame)
        else:
            assert frame["response"]["request_id"]==expected
            assert frame["response"]["response"]["hookSpecificOutput"]["additionalContext"]==expected
            return
init=read()
send({"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}})
first=read()
assert first["message"]["content"]==[{"type":"text","text":"First"}]
hook(first,"first-routing")
second=waiting.pop(0) if waiting else read()
assert second["message"]["content"]==[{"type":"text","text":"Second"}]
send({"type":"command_lifecycle","command_uuid":second["uuid"],"state":"queued"})
send({"type":"assistant","message":{"content":[{"type":"text","text":"First answer."}]}})
send({"type":"result","stop_reason":"end_turn"})
hook(second,"second-routing")
send({"type":"assistant","message":{"content":[{"type":"text","text":"Second answer."}]}})
send({"type":"result","stop_reason":"end_turn"})
'
"#);
        let client=NativeClient::spawn(&script, fixture_root(), &doctrine_fixture(), &skills_fixture()).unwrap();
        let boundaries=Arc::new(Mutex::new(Vec::new()));
        let prepare_first=boundaries.clone();let complete_first=boundaries.clone();
        let first=client.enqueue_prepared("First",Some(Box::new(move || {
            prepare_first.lock().unwrap().push("first started");Ok(Some("first-routing".into()))
        })),Some(Box::new(move || {complete_first.lock().unwrap().push("first completed");Ok(())}))).unwrap();
        let prepare_second=boundaries.clone();let complete_second=boundaries.clone();
        let second=client.enqueue_prepared("Second",Some(Box::new(move || {
            let mut events=prepare_second.lock().unwrap();
            assert_eq!(*events,["first started","first completed"], "a new input rebound scopes before the old native turn ended");
            events.push("second started");Ok(Some("second-routing".into()))
        })),Some(Box::new(move || {complete_second.lock().unwrap().push("second completed");Ok(())}))).unwrap();
        let mut answers=Vec::new();
        for response in [first,second] {
            let mut text=String::new();
            assert_eq!(response.wait(&mut |item|if let TurnItem::Text{text:part,..}=item{text.push_str(part)}).unwrap(),"end_turn");
            answers.push(text);
        }
        assert_eq!(answers,["First answer.","Second answer."]);
        assert_eq!(*boundaries.lock().unwrap(),["first started","first completed","second started","second completed"]);
    }

    #[test]
    fn a_native_queued_input_cancelled_before_its_hook_never_closes_the_active_scope() { // dialect-exempt: native protocol state.
        let script=write_script("queued-input-cancellation",r#"
exec python3 -u -c '
import json,sys
init=json.loads(sys.stdin.readline())
print(json.dumps({"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}),flush=True)
prompt=json.loads(sys.stdin.readline())
print(json.dumps({"type":"command_lifecycle","command_uuid":prompt["uuid"],"state":"queued"}),flush=True)
print(json.dumps({"type":"command_lifecycle","command_uuid":prompt["uuid"],"state":"cancelled"}),flush=True)
'
"#);
        let client=NativeClient::spawn(&script,fixture_root(),&doctrine_fixture(),&skills_fixture()).unwrap();
        let touched=Arc::new(AtomicUsize::new(0));let did_prepare=touched.clone();let did_complete=touched.clone();
        let response=client.enqueue_prepared("A queued request",Some(Box::new(move || {
            did_prepare.fetch_add(1,Ordering::SeqCst);Ok(None)
        })),Some(Box::new(move || {did_complete.fetch_add(1,Ordering::SeqCst);Ok(())}))).unwrap();
        assert_eq!(response.wait(&mut |_|{}).unwrap(),STOP_REASON_CANCELLED);
        assert_eq!(touched.load(Ordering::SeqCst),0, "a non-admitted input touched another input's scopes");
    }

    #[test]
    fn production_stop_waits_for_native_interruption_history_before_teardown() {
        let script=write_script("stop-native-history",r#"
exec python3 -u -c '
import json,sys
from pathlib import Path
def read(): return json.loads(sys.stdin.readline())
def send(frame): print(json.dumps(frame),flush=True)
init=read()
send({"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}})
first=read()
send({"type":"command_lifecycle","command_uuid":first["uuid"],"state":"started"})
queued=read()
send({"type":"command_lifecycle","command_uuid":queued["uuid"],"state":"queued"})
stop=read()
assert stop["request"]["subtype"]=="interrupt"
send({"type":"control_response","response":{"subtype":"success","request_id":stop["request_id"]}})
send({"type":"assistant","message":{"content":[{"type":"text","text":"interrupt accepted"}]}})
assert read()["type"]=="fixture-release"
Path(sys.argv[1]+".history").write_text("[Request interrupted by user]\n")
send({"type":"result","terminal_reason":"aborted_streaming"})
sys.stdin.read()
' "$0"
"#);
        let mut client=NativeClient::spawn(&script,fixture_root(),&doctrine_fixture(),&skills_fixture()).unwrap();
        client.settle_workers_on_stop=true;
        let first=client.enqueue_prompt("Long answer",None).unwrap();
        let queued=client.enqueue_prompt("Queued question",None).unwrap();
        loop {
            // load-bound: wait for the actual queued frame, with a hang guard only.
            let chunk=first.rx.recv_timeout(Duration::from_secs(10)).expect("queued frame missing");
            if matches!(chunk,ChunkMsg::Frame(ref frame) if frame["type"]=="command_lifecycle" && frame["state"]=="queued") {break}
        }
        let handle=client.cancel_handle();
        let stopping=std::thread::spawn(move || handle.cancel());
        loop {
            // load-bound: a controlled protocol barrier separates interrupt ACK from native persistence.
            let chunk=first.rx.recv_timeout(Duration::from_secs(10)).expect("interrupt was killed before its native result");
            if matches!(chunk,ChunkMsg::Text(ref text) if text=="interrupt accepted") {break}
            assert!(!matches!(chunk,ChunkMsg::Done(_)),"provider died before recording the native interruption");
        }
        NativeClient::write_line(&client.stdin,&json!({"type":"fixture-release"})).unwrap();
        assert!(stopping.join().unwrap());
        assert_eq!(first.wait(&mut |_|{}).unwrap(),STOP_REASON_CANCELLED);
        assert!(std::fs::read_to_string(script.with_extension("history")).unwrap().contains("[Request interrupted by user]"));
        assert!(matches!(queued.wait(&mut |_|{}),Err(NativeError::Closed | NativeError::Exited { .. })),"pre-Stop queued input ran after interruption");
        assert!(client.reader_closed.load(Ordering::SeqCst),"next input must use the ordinary resume path");
    }

    #[test]
    fn native_stop_does_not_wake_the_response_of_an_input_still_queued_by_claude() {
        let script=write_script("stop-active-input",r#"
exec python3 -u -c '
import json,sys
def read(): return json.loads(sys.stdin.readline())
def send(frame): print(json.dumps(frame),flush=True)
init=read()
send({"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}})
first=read()
send({"type":"command_lifecycle","command_uuid":first["uuid"],"state":"started"})
second=read()
send({"type":"command_lifecycle","command_uuid":second["uuid"],"state":"queued"})
stop=read()
assert stop["request"]["subtype"]=="interrupt"
send({"type":"result","stop_reason":"cancelled"})
assert read()["type"]=="fixture-release"
send({"type":"command_lifecycle","command_uuid":second["uuid"],"state":"started"})
send({"type":"assistant","message":{"content":[{"type":"text","text":"Second answer."}]}})
send({"type":"result","stop_reason":"end_turn"})
'
"#);
        let client=NativeClient::spawn(&script,fixture_root(),&doctrine_fixture(),&skills_fixture()).unwrap();
        let first=client.enqueue_prompt("First",None).unwrap();
        let second=client.enqueue_prompt("Second",None).unwrap();
        // A positive native frame establishes the pending input's actual phase before Stop.
        loop {
            // load-bound: hang guard for a positive queued-frame signal; no elapsed-time verdict.
        let frame=first.rx.recv_timeout(Duration::from_secs(10)).expect("native queued-frame hang guard");
            if matches!(frame,ChunkMsg::Frame(ref frame) if frame["type"]=="command_lifecycle" && frame["state"]=="queued") {break}
        }
        // Legacy fixture traffic can have arrived before the native queued frame. It is
        // unrelated to Stop; this witness isolates the cancellation signal that follows.
        for item in second.rx.try_iter() { assert!(matches!(item,ChunkMsg::Frame(_))); }
        assert!(client.cancel_handle().cancel());
        assert!(matches!(second.rx.try_recv(),Err(std::sync::mpsc::TryRecvError::Empty)),
            "the app broadcast Stop to an input Claude Code still has queued");
        NativeClient::write_line(&client.stdin,&json!({"type":"fixture-release"})).unwrap();
        assert_eq!(first.wait(&mut |_|{}).unwrap(),STOP_REASON_CANCELLED);
        assert_eq!(second.wait(&mut |_|{}).unwrap(),"end_turn");
    }

    #[test]
    fn app_messages_enter_native_context_once_and_resume_from_its_attachments() {
        let script=write_script("app-context-once",r#"
exec python3 -u -c '
import json,sys
from pathlib import Path
history=Path(sys.argv[1]+".history")
def read():return json.loads(sys.stdin.readline())
def send(frame):print(json.dumps(frame),flush=True)
read()
send({"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}})
while True:
    command=read()
    text=command["message"]["content"][0]["text"]
    send({"type":"command_lifecycle","command_uuid":command["uuid"],"state":"started"})
    send({"type":"control_request","request_id":command["uuid"],"request":{"subtype":"hook_callback","callback_id":"richos-input-context","input":{"hook_event_name":"UserPromptSubmit","prompt_id":command["uuid"],"prompt":text}}})
    context=read()["response"]["response"].get("hookSpecificOutput",{}).get("additionalContext","")
    packets=[json.loads(line) for line in context.splitlines() if line.startswith("{\"richos_app_messages\"")]
    ids=[message["id"] for packet in packets for message in packet["richos_app_messages"]]
    expected={"first":["intro"],"again":[],"resumed":["changed","settings-action"]}[text]
    assert ids==expected,(text,ids,expected)
    with history.open("a") as output:output.write(json.dumps({"type":"attachment","attachment":{"type":"hook_additional_context","content":[context]}})+"\n")
    send({"type":"result","stop_reason":"end_turn"})
' "$0"
"#);
        let root=fixture_root();
        let mut first=NativeCognition::start(&script,root,&doctrine_fixture(),&skills_fixture()).unwrap();
        first.set_app_messages(vec![("intro".into(),"Dictation key is F2".into())]);
        first.prompt("first",&mut |_|{}).unwrap();
        first.prompt("again",&mut |_|{}).unwrap();
        drop(first);
        let history=std::path::PathBuf::from(format!("{}.history",script.display()));
        let ids=native_app_message_ids(&history).unwrap();
        assert_eq!(ids,std::collections::HashSet::from(["intro".to_string()]));
        let mut resumed=NativeCognition::start(&script,root,&doctrine_fixture(),&skills_fixture()).unwrap();
        // The normal spawn uses this same history reader before native --resume.
        *resumed.client.app_message_ids.lock().unwrap()=ids;
        resumed.set_app_messages(vec![("intro".into(),"Dictation key is F2".into()),
            ("changed".into(),"Dictation key is F3".into()),("settings-action".into(),"Key changed from F2 to F3".into())]);
        resumed.prompt("resumed",&mut |_|{}).unwrap();
        assert_eq!(native_app_message_ids(&history).unwrap().len(),3);
        drop(resumed);std::fs::remove_file(history).unwrap();
    }

    #[test]
    fn folded_identical_inputs_prepare_before_started_and_deliver_each_text_once() {
        let script=write_script("folded-input",r#"
exec python3 -u -c '
import json,sys
waiting=[]
def read():return json.loads(sys.stdin.readline())
def send(frame):print(json.dumps(frame),flush=True)
def hook(command,identity,expected):
    frame={"type":"control_request","request_id":identity,"request":{"subtype":"hook_callback","callback_id":"richos-input-context","input":{"hook_event_name":"UserPromptSubmit","prompt_id":"ambient-running-prompt","prompt":command["message"]["content"][0]["text"]}}}
    send(frame)
    while True:
        reply=read()
        if reply["type"]=="user":waiting.append(reply)
        else:
            assert reply["response"]["response"]==expected,reply
            return
read()
send({"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}})
first=read()
send({"type":"command_lifecycle","command_uuid":first["uuid"],"state":"started"})
hook(first,"native-first",{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":"first-context"}})
second=waiting.pop(0) if waiting else read()
send({"type":"command_lifecycle","command_uuid":second["uuid"],"state":"queued"})
hook(second,"native-second",{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":"second-context"}})
hook(second,"native-second",{})
send({"type":"command_lifecycle","command_uuid":second["uuid"],"state":"started"})
send({"type":"assistant","message":{"content":[{"type":"text","text":"Single folded reply."}]}})
for command in [first,second]:send({"type":"command_lifecycle","command_uuid":command["uuid"],"state":"completed"})
send({"type":"result","stop_reason":"end_turn"})
'
"#);
        let client=NativeClient::spawn(&script,fixture_root(),&doctrine_fixture(),&skills_fixture()).unwrap();
        let first=client.enqueue_prompt("Same original words",Some("first-context")).unwrap();
        let second=client.enqueue_prompt("Same original words",Some("second-context")).unwrap();
        let mut one=String::new();let mut two=String::new();
        first.wait(&mut |item|if let TurnItem::Text{text,..}=item{one.push_str(text)}).unwrap();
        second.wait(&mut |item|if let TurnItem::Text{text,..}=item{two.push_str(text)}).unwrap();
        assert!(one.is_empty(),"folded text duplicated into the earlier input: {one}");
        assert_eq!(two,"Single folded reply.");
    }

    #[test]
    fn a_message_queued_while_rich_writes_does_not_split_his_sentence() {
        // Nightly 66 (run b, ledger seq 232..235): "Early " | a gap | "in a relationship".
        // The gap was the child's `queued` frame for the message sent while Rich wrote.
        let script=write_script("queued-mid-text",r#"
exec python3 -u -c '
import json,sys
def read():return json.loads(sys.stdin.readline())
def send(frame):print(json.dumps(frame),flush=True)
def text(t):send({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":t}}})
read()
send({"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}})
first=read()
send({"type":"command_lifecycle","command_uuid":first["uuid"],"state":"started"})
send({"type":"stream_event","event":{"type":"message_start","message":{}}})
text("Early ")
second=read()
send({"type":"command_lifecycle","command_uuid":second["uuid"],"state":"queued"})
text("in a relationship.")
send({"type":"stream_event","event":{"type":"content_block_stop","index":0}})
send({"type":"result","stop_reason":"end_turn"})
sys.stdin.read()
'
"#);
        let client=NativeClient::spawn(&script,fixture_root(),&doctrine_fixture(),&skills_fixture()).unwrap();
        let first=client.enqueue_prompt("Write me a guide",None).unwrap();
        let mut second=None;
        let mut log:Vec<(u64,String)>=Vec::new();
        first.wait(&mut |item| match item {
            TurnItem::Text{seq,text} => {
                log.push((seq,format!("text:{text}")));
                if second.is_none() {second=Some(client.enqueue_prompt("And what is 7 plus 7?",None).unwrap());}
            }
            TurnItem::Machinery(record) => log.push((record.seq,format!("machinery:{}",record.title))),
            _ => {}
        }).unwrap();
        let texts:Vec<u64>=log.iter().filter(|(_,what)|what.starts_with("text:")).map(|(seq,_)|*seq).collect();
        assert!(texts.windows(2).all(|pair|pair[1]==pair[0]+1), "the sentence was split by another input's frame: {log:?}");
        assert_eq!(log.iter().filter(|(_,what)|what=="machinery:command_lifecycle").count(),2, "the queued frame must still be recorded: {log:?}");
        let said:String=log.iter().filter_map(|(_,what)|what.strip_prefix("text:")).collect();
        assert_eq!(said,"Early in a relationship.");
    }

    #[test]
    fn stop_reaches_a_native_turn_started_from_multiple_queued_inputs() {
        use crate::steering::{ActiveTurn, StopOutcome, TurnControl};
        let script=write_script("batched-input-stop",r#"
exec python3 -u -c '
import json,pathlib,sys
waiting=[]
def read():return json.loads(sys.stdin.readline())
def send(frame):print(json.dumps(frame),flush=True)
def phase(command,state):send({"type":"command_lifecycle","command_uuid":command["uuid"],"state":state})
def hook(identity,text):
    frame={"type":"control_request","request_id":identity,"request":{"subtype":"hook_callback","callback_id":"richos-input-context","input":{"hook_event_name":"UserPromptSubmit","prompt_id":"native-batch","prompt":text}}}
    send(frame)
    while True:
        reply=read()
        if reply["type"]=="user":waiting.append(reply)
        else:return reply["response"]["response"]
read()
send({"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}})
first=read();phase(first,"started")
hook("first",first["message"]["content"][0]["text"])
batch=[]
for _ in range(2):
    command=waiting.pop(0) if waiting else read()
    batch.append(command);phase(command,"queued")
send({"type":"result","stop_reason":"end_turn"})
for command in batch:phase(command,"started")
text="\n\n".join(command["message"]["content"][0]["text"] for command in batch)
context=hook("batch",text)
# The native batch is running even if the app failed to prepare its inputs.
send({"type":"assistant","message":{"content":[{"type":"text","text":"BATCH-RUNNING. "}]}})
request=read()
assert request["request"]["subtype"]=="interrupt",request
assert context["hookSpecificOutput"]["additionalContext"]=="first-context\nsecond-context",context
# A repeated callback must not prepare a later identical queued input.
assert hook("batch",text)=={}
pathlib.Path(sys.argv[1]).write_text("native interrupt and batch contexts verified")
send({"type":"result","stop_reason":"cancelled"}) # dialect-exempt: native stop_reason wire value
' "$0.proof"
"#);
        let control=TurnControl::open(fixture_root().join(format!("batch-stop-{}",uuid::Uuid::new_v4()))).unwrap();
        let conversation=control.for_thread("batch-thread");
        let client=NativeClient::spawn(&script,fixture_root(),&doctrine_fixture(),&skills_fixture()).unwrap();
        conversation.set_cancel(Some(client.cancel_handle()));
        let mut replies=Vec::new();
        let (completed,done)=channel();
        for (id,text,context) in [("earlier","Earlier input",None),
            ("spoken","One original input",Some("first-context")),
            ("peer","Another original\ninput",Some("second-context"))] {
            let begin=conversation.clone();let end=control.clone();let completed=completed.clone();
            replies.push(client.enqueue_prepared(text,Some(Box::new(move || {
                begin.begin_turn(ActiveTurn {turn_id:id.into(),thread_id:"batch-thread".into(),entity_id:None,started_at:None});
                Ok(context.map(str::to_string))
            })),Some(Box::new(move || {end.end_turn(id);let _sent=completed.send(id);Ok(())}))).unwrap());
        }
        let first=replies.remove(0);
        assert_eq!(first.wait(&mut |_|{}).unwrap(),"end_turn");
        let (observed,ready)=channel();
        let second=replies.pop().unwrap();
        let waiter=std::thread::spawn(move ||second.wait(&mut |item| {
            if matches!(item,TurnItem::Text{text,..} if text.contains("BATCH")) {observed.send(()).unwrap();}
        }));
        // load-bound: hang guard only; native output proves the combined turn has started.
        ready.recv_timeout(Duration::from_secs(10)).expect("the combined native turn did not start");
        assert!(matches!(conversation.request_stop().unwrap(),StopOutcome::Requested{reached_lease:true,..}),
            "Stop lost the native turn after the previous input completed");
        assert_eq!(waiter.join().unwrap().unwrap(),STOP_REASON_CANCELLED);
        assert_eq!(replies.remove(0).wait(&mut |_|{}).unwrap(),STOP_REASON_CANCELLED);
        for _ in 0..3 {
            // load-bound: hang guard only; completion proves the native result or EOF was drained.
            done.recv_timeout(Duration::from_secs(10)).expect("native input completion missing");
        }
        assert_eq!(std::fs::read_to_string(format!("{}.proof",script.display())).unwrap(),
            "native interrupt and batch contexts verified");
    }

    #[test]
    fn scope_preparation_error_survives_a_native_success_envelope_without_poisoning_the_next_input() {
        let script=write_script("scope-refusal",r#"
exec python3 -u -c '
import json,sys
def read(): return json.loads(sys.stdin.readline())
def send(frame): print(json.dumps(frame),flush=True)
init=read()
send({"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}})
for index in range(2):
    prompt=read()
    send({"type":"command_lifecycle","command_uuid":prompt["uuid"],"state":"started"})
    send({"type":"control_request","request_id":str(index),"request":{"subtype":"hook_callback","callback_id":"richos-input-context","input":{"hook_event_name":"UserPromptSubmit","prompt_id":"native-id","prompt":prompt["message"]["content"][0]["text"]}}})
    reply=read()["response"]["response"]
    if index==0:
        assert reply=={"decision":"block","reason":"fixture scope refused"}
        send({"type":"result","subtype":"success","is_error":False,"num_turns":0,"stop_reason":None,"result":"UserPromptSubmit operation blocked by hook"})
    else:
        assert reply=={}
        send({"type":"assistant","message":{"content":[{"type":"text","text":"Second answer."}]}})
        send({"type":"result","stop_reason":"end_turn"})
'
"#);
        let client=NativeClient::spawn(&script,fixture_root(),&doctrine_fixture(),&skills_fixture()).unwrap();
        let closed=Arc::new(AtomicUsize::new(0));let complete=closed.clone();
        let first=client.enqueue_prepared("First",Some(Box::new(||Err("fixture scope refused".into()))),
            Some(Box::new(move ||{complete.fetch_add(1,Ordering::SeqCst);Ok(())}))).unwrap();
        let error=first.wait(&mut |_|{}).expect_err("the native hook envelope cannot erase an actual host binding failure");
        assert!(error.to_string().contains("fixture scope refused"));
        assert_eq!(closed.load(Ordering::SeqCst),1);
        let mut answer=String::new();
        assert_eq!(client.prompt("Second",&mut |item|if let TurnItem::Text{text,..}=item{answer.push_str(text)}).unwrap(),"end_turn");
        assert_eq!(answer,"Second answer.");
    }

    #[test]
    fn native_prompt_context_never_changes_the_original_user_instruction() {
        let script=write_script("input-context", r#"
exec python3 -u -c '
import json,sys
def read(): return json.loads(sys.stdin.readline())
def send(frame): print(json.dumps(frame),flush=True)
init=read()
send({"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}})
for ordinal in range(2):
    incoming=read()
    assert incoming["message"]["content"]==[{"type":"text","text":"Original instruction"}]
    assert init["request"]["hooks"]["UserPromptSubmit"][0]["hookCallbackIds"]==["richos-input-context"]
    command=incoming["uuid"]
    send({"type":"command_lifecycle","command_uuid":command,"state":"started"})
    for callback in range(2):
        request=str(ordinal)+":"+str(callback)
        send({"type":"control_request","request_id":request,"request":{"subtype":"hook_callback","callback_id":"richos-input-context","input":{"hook_event_name":"UserPromptSubmit","prompt_id":"different-native-id","prompt":"Original instruction"}}})
        reply=read()
        assert reply["response"]["request_id"]==request
        body=reply["response"]["response"]
        if callback==0:
            assert body["hookSpecificOutput"]["additionalContext"]=="routing-"+str(ordinal)
        else:
            assert body=={}, "the same input context was applied twice"
    send({"type":"result","stop_reason":"end_turn"})
'
"#);
        let client=NativeClient::spawn(&script, fixture_root(), &doctrine_fixture(), &skills_fixture()).unwrap();
        for ordinal in 0..2 {
            assert_eq!(client.prompt_with_context("Original instruction",Some(&format!("routing-{ordinal}")),&mut |_|{}).unwrap(),"end_turn");
        }
    }

    #[test]
    fn ordinary_native_reply_needs_no_structured_output_tool() {
        let script=write_script("ordinary-reply", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Color and light."}}}'
printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"text","text":"Color and light."}]}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn","subtype":"success","is_error":false}'
read -r keep_alive
"#);
        let client=NativeClient::spawn(&script,fixture_root(),&doctrine_fixture(),&skills_fixture()).unwrap();
        let mut text=String::new();
        assert_eq!(client.prompt("Actual user question",&mut |item| {
            if let TurnItem::Text {text:delta,..}=item {text.push_str(delta)}
        }).unwrap(),"end_turn");
        assert_eq!(text,"Color and light.");
    }

    #[test]
    fn conversation_grants_close_on_native_result_and_reopen_for_the_next_input() {
        let body = r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"On it!"}}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":" Landing now."}}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
read -r prompt
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
"#;
        let (mut lease, root) = lease_with_production_grants("spoken", LeaseRole::Conversation, body);
        let scope = lease.continuity_tools_scope.clone().expect("a front desk has one");
        // The company-notes grant is a conversation lease's, and `prompt` opens it first: a
        // front desk with no company bound fails the turn before it sends (deliberately — see
        // `ActionGrant::set`). This fixture is about the grant AFTER it, so a company is bound.
        crate::onboarding_tools::write_scope(
            lease.onboarding_scope.as_ref().expect("a front desk has one"),
            &crate::onboarding_tools::OnboardingToolScope {
                version: 1,
                entity_id: "qa-test-co".into(),
                actions_allowed: false,
                central_root: root.join("corpus"),
                record_path: root.join("onboarding.json"),
            },
        )
        .unwrap();
        let allowed = |path: &std::path::Path| -> bool {
            serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap()["actions_allowed"] == true
        };

        // Before the turn: shut. Every call to `richos_continuity` is refused by the engine's
        // own adapter while this reads false, and the model cannot write this file.
        assert!(!allowed(&scope), "the lease came up with the bookkeeping already open");

        let mut said = String::new();
        let mut open_at_each_run_of_his_words = Vec::new();
        {
            let mut collect = |item: TurnItem| {
                if let TurnItem::Text { text, .. } = item {
                    // **THE ASSERTION FROM INSIDE THE TURN, and the ORDER it now proves.**
                    // `he_has_now_been_spoken_to` is called AFTER the text is routed (see its
                    // doc: opening the grant is a file write with an `fsync` in it, and run C
                    // measured it costing 7 ms and 5 ms IN FRONT OF his first words). So:
                    //
                    //   - at his FIRST words the grant is still SHUT — nothing could have
                    //     written to ECS while he waited, which is the §55 property;
                    //   - by the SECOND delta of the same turn it is OPEN — the front desk's own
                    //     bookkeeping is deferred, not abolished.
                    //
                    // Two deltas rather than one is what makes both halves observable: with one,
                    // "shut when he heard it" and "never opens at all" are the same reading.
                    open_at_each_run_of_his_words.push(allowed(&scope));
                    said.push_str(text);
                }
            };
            assert_eq!(
                crate::cognition::Cognition::prompt(&mut lease, "Land the pricing branch", &mut collect).unwrap(),
                "end_turn"
            );
        }
        assert_eq!(said, "On it! Landing now.");
        assert_eq!(open_at_each_run_of_his_words, vec![true, true],
            "ordinary turn authority must not depend on whether the model spoke first");
        // And the turn's end revokes it, by the same all-or-nothing loop as every other grant.
        assert!(!allowed(&scope), "the bookkeeping grant outlived the turn");

        // The SECOND turn starts shut again — the per-turn reset, which `prompt` makes
        // positively rather than inheriting from the previous turn's end.
        assert_eq!(
            crate::cognition::Cognition::prompt(&mut lease, "And the second thing", &mut |_| {}).unwrap(),
            "end_turn"
        );
        assert!(!allowed(&scope), "a second turn inherited an open grant");
        drop(lease);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// One turn of a front desk that hands work over, as the wire actually carries it.
    ///
    /// `receipt` is the `tool_result` content the register answers with, and `after` is whatever
    /// the model says once it has been handed that answer — the two variables this behavior is
    /// entirely about.
    fn a_handover_turn(tag: &str, call_id: &str, receipt: &str, error: bool, after: &str) -> Vec<TurnItem2> {
        let script = write_script(tag, &format!(r#"
read -r init
printf '%s\n' '{{"type":"control_response","response":{{"subtype":"success","request_id":"req_init","response":{{}}}}}}'
read -r prompt
printf '%s\n' '{{"type":"stream_event","event":{{"type":"message_start"}}}}'
printf '%s\n' '{{"type":"stream_event","event":{{"type":"content_block_start","content_block":{{"type":"tool_use","id":"{call_id}","name":"mcp__richos_assignments__record","input":{{}}}}}}}}'
printf '%s\n' '{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"{call_id}","name":"mcp__richos_assignments__record","input":{{"assignment":"Land the pricing branch","kind":"task"}}}}]}}}}'
printf '%s\n' '{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"{call_id}","is_error":{error},"content":"{receipt}"}}]}}}}'
printf '%s\n' '{{"type":"stream_event","event":{{"type":"content_block_delta","delta":{{"type":"text_delta","text":"{after}"}}}}}}'
printf '%s\n' '{{"type":"result","stop_reason":"end_turn"}}'
"#));
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        let mut log = Vec::new();
        {
            let mut collect = |item: TurnItem| match item {
                TurnItem::Text { text, .. } => log.push(TurnItem2::Said(text.to_string())),
                TurnItem::Machinery(record) => log.push(TurnItem2::Machinery(
                    record.kind,
                    record.title.clone(),
                    record.summary.clone().unwrap_or_default(),
                )),
                TurnItem::AnswerComplete => {}
            };
            assert_eq!(client.prompt("Land the pricing branch", &mut collect).unwrap(), "end_turn");
        }
        log
    }

    /// An owned, comparable copy of delivered text and operational evidence.
    #[derive(Debug, PartialEq)]
    enum TurnItem2 {
        Said(String),
        Machinery(crate::machinery::MachineryKind, String, String),
    }

    #[test]
    fn a_registration_result_cannot_speak_or_suppress_richs_authored_reply() {
        for receipt in [r#"{\"recorded\":true}"#, r#"{\"recorded\":true,\"say\":\"NOT RICH\"}"#] {
            let log = a_handover_turn("rich-authors-the-reply", "toolu_R", receipt, false,
                "I will get that checked.");
            let said: Vec<&str> = log.iter().filter_map(|item| match item {
                TurnItem2::Said(text) => Some(text.as_str()), _ => None,
            }).collect();
            assert_eq!(said, ["I will get that checked."], "{log:#?}");
            let result = log.iter().position(|item| matches!(item,
                TurnItem2::Machinery(_, _, summary) if summary.contains("recorded"))).unwrap();
            let reply = log.iter().position(|item| matches!(item, TurnItem2::Said(_))).unwrap();
            assert!(result < reply, "tool data was spoken instead of Rich's later reply");
        }
    }

    #[test]
    fn a_refused_registration_is_never_announced_and_the_model_still_answers_him() {
        // **THE DEGRADATION, AND IT IS THE HALF THAT KEEPS THIS SAFE.** The app speaks only on
        // the register's own `recorded: true`; on anything else it says nothing at all and the
        // turn behaves exactly as it did before 2026-09-18 — the model says its line a round trip
        // later and he is answered. A host that spoke on the strength of the CALL having been
        // made would tell him "On it!" for work that was refused.
        let refused = r#"This conversation is not open for new assignments right now. Nothing was recorded."#;
        for (tag, receipt, error) in [
            ("register-refused", refused, true),
            ("register-refused-without-the-flag", refused, false),
            ("register-said-no", r#"{\"recorded\":false,\"say\":\"On it!\"}"#, false),
        ] {
            let log = a_handover_turn(tag, "toolu_R", receipt, error, "On it!");
            let said: Vec<&String> = log.iter().filter_map(|i| match i { TurnItem2::Said(t) => Some(t), _ => None }).collect();
            assert_eq!(said, vec!["On it!"], "{tag}: the model's own reply must still reach him: {log:#?}");
        }
    }

    #[test]
    fn tool_result_prose_is_never_promoted_to_richs_reply() {
        // A result that happens to carry the same two fields, on a DIFFERENT call, says nothing:
        // the id is what ties an answer to a question on this wire, and the app is speaking about
        // the register or it is not speaking.
        let receipt = r#"{\"recorded\":true,\"say\":\"Anyone can say this\"}"#;
        let log = a_handover_turn("the-registers-own-result", "toolu_R", receipt, false, "Done.");
        let said: Vec<String> = log.iter().filter_map(|i| match i { TurnItem2::Said(t) => Some(t.clone()), _ => None }).collect();
        // The app says what its OWN server handed back, whatever that is — the sentences are
        // `assignment::Receipt`'s and this host does not second-guess them. The model's "Done."
        // after it is withheld, which is the other half of the same rule.
        assert_eq!(said, vec!["Done.".to_string()], "{log:#?}");

        // The same frames with the result attributed to a call the register never made.
        let script = write_script("foreign-tool-result", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","id":"toolu_BASH","name":"Bash","input":{}}}}'
printf '%s\n' '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_BASH","content":"{\"recorded\":true,\"say\":\"NOT RICH\"}"}]}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Done."}}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
"#);
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        let mut said = String::new();
        {
            let mut collect = |item: TurnItem| { if let TurnItem::Text { text, .. } = item { said.push_str(text) } };
            assert_eq!(client.prompt("Run the tests", &mut collect).unwrap(), "end_turn");
        }
        assert_eq!(said, "Done.", "a tool that is not the register must not be able to put words in his mouth");
    }

    /// One turn that writes its conversation checkpoint, as the wire carries it.
    ///
    /// `before` is what the model says first, `after` is whatever it adds once the checkpoint
    /// has been written, and `speak_first` chooses which side of the checkpoint the reply falls
    /// on — the two variables this behavior is entirely about.
    ///
    /// **No apostrophes in `before`/`after`.** The frames go through a single-quoted `sh`
    /// string; Ray's verbatim sentences are in the tests' own documentation, where they can be
    /// read, rather than mangled into a shell literal here.
    fn a_checkpointed_turn(tag: &str, before: &str, after: &str, speak_first: bool) -> Vec<TurnItem2> {
        a_checkpointed_turn_returning(tag, before, after, speak_first, "")
    }

    /// [`a_checkpointed_turn`] with the checkpoint's own `tool_result` flag chosen:
    /// `result_flag` is spliced into the result block, so `"\"is_error\":true,"` is a
    /// checkpoint the engine's adapter refused (`adapters/mcp.py`, `"isError": True`).
    fn a_checkpointed_turn_returning(
        tag: &str,
        before: &str,
        after: &str,
        speak_first: bool,
        result_flag: &str,
    ) -> Vec<TurnItem2> {
        let reply = if speak_first {
            format!(r#"printf '%s\n' '{{"type":"stream_event","event":{{"type":"content_block_delta","delta":{{"type":"text_delta","text":"{before}"}}}}}}'"#)
        } else {
            String::new()
        };
        let script = write_script(tag, &format!(r#"
read -r init
printf '%s\n' '{{"type":"control_response","response":{{"subtype":"success","request_id":"req_init","response":{{}}}}}}'
read -r prompt
printf '%s\n' '{{"type":"stream_event","event":{{"type":"message_start"}}}}'
{reply}
printf '%s\n' '{{"type":"stream_event","event":{{"type":"content_block_start","content_block":{{"type":"tool_use","id":"toolu_CP","name":"mcp__richos_continuity__checkpoint","input":{{}}}}}}}}'
printf '%s\n' '{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"toolu_CP","name":"mcp__richos_continuity__checkpoint","input":{{"summary":"the conversation so far"}}}}]}}}}'
printf '%s\n' '{{"type":"user","message":{{"content":[{{"type":"tool_result",{result_flag}"tool_use_id":"toolu_CP","content":"checkpoint written"}}]}}}}'
printf '%s\n' '{{"type":"stream_event","event":{{"type":"content_block_delta","delta":{{"type":"text_delta","text":"{after}"}}}}}}'
printf '%s\n' '{{"type":"result","stop_reason":"end_turn"}}'
"#));
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        let mut log = Vec::new();
        {
            let mut collect = |item: TurnItem| match item {
                TurnItem::Text { text, .. } => log.push(TurnItem2::Said(text.to_string())),
                TurnItem::Machinery(record) => log.push(TurnItem2::Machinery(
                    record.kind,
                    record.title.clone(),
                    record.summary.clone().unwrap_or_default(),
                )),
                TurnItem::AnswerComplete => {}
            };
            assert_eq!(client.prompt("how is it going?", &mut collect).unwrap(), "end_turn");
        }
        log
    }

    #[test]
    fn one_question_gets_one_answer_and_the_closing_line_after_the_checkpoint_never_reaches_him() {
        // **RAY'S CANDIDATE-.11 DEFECT 2.4, AS THE WIRE CARRIES IT.** He asked "how is it
        // going?" once and read the answer twice, under "Worked (2 steps)"
        // (`docs/verification/2026-09-19-nightly-11-onscreen/07-mac-duplicate-answer.png`):
        //
        //   "It's in progress. A helper is adding the line to the notes file now. It hasn't
        //    been landed yet, and nothing is waiting on you."
        //
        //   "It's still in progress. Someone is adding the line to the notes file right now,
        //    and it hasn't been landed yet. You don't need to do anything."
        //
        // `doctrine/front-desk.md` already forbids the second one — *"Write the checkpoint and
        // stop: do not repeat what you already said … and do not add a closing sentence"* — and
        // that is exactly the point: a sentence that is not obeyed is not a fix, so the order is
        // kept here, where the host can see both and he can only ever see one.
        let log = a_checkpointed_turn(
            "closing-line-after-the-checkpoint",
            "It is in progress. A helper is adding the line to the notes file now.",
            "It is still in progress. Someone is adding the line to the notes file right now.",
            true,
        );

        let said: Vec<&String> = log.iter().filter_map(|i| match i { TurnItem2::Said(t) => Some(t), _ => None }).collect();
        assert_eq!(
            said,
            vec!["It is in progress. A helper is adding the line to the notes file now.",
                "It is still in progress. Someone is adding the line to the notes file right now."],
            "one question, one answer: {log:#?}"
        );

        // The checkpoint's own technical row is untouched — the conversation lost a duplicate
        // line, the machinery lane lost nothing.
        assert!(log.iter().any(|i| matches!(i, TurnItem2::Machinery(_, title, _)
            if title == CONTINUITY_CHECKPOINT_TOOL)), "{log:#?}");
    }

    #[test]
    fn a_checkpoint_written_before_his_first_words_never_withholds_the_reply() {
        // **THE SAFETY PROPERTY, AND IT IS THE HALF THAT MATTERS MOST.** A model that writes
        // the checkpoint FIRST is the run-B shape measured on 2026-09-18 — 3.0 s and 2.4 s of
        // silence in front of his words (`first-words-2026-09-18-logs/run-B-*`). That call is
        // refused by the engine's own adapter, so it establishes nothing; a rule that read it as
        // "the turn has spoken" would swallow the reply that had not happened yet and leave him
        // with a turn that produced no words at all. Silent Rich is a far worse defect than a
        // doubled line, so the flag is only ever set on a checkpoint that comes AFTER his words.
        let log = a_checkpointed_turn(
            "checkpoint-before-the-reply",
            "never said",
            "It is in progress. Nothing is waiting on you.",
            false,
        );
        let said: Vec<&String> = log.iter().filter_map(|i| match i { TurnItem2::Said(t) => Some(t), _ => None }).collect();
        assert_eq!(
            said,
            vec!["It is in progress. Nothing is waiting on you."],
            "a checkpoint ahead of the reply must never withhold the reply: {log:#?}"
        );
    }

    #[test]
    fn a_checkpoint_the_engine_refused_never_withholds_what_the_model_says_next() {
        // **PART 1 HUNT FINDING 15.** The withholding exists for the duplicate closing line
        // after a checkpoint that SUCCEEDED. The flag used to be set on the `tool_use` — the
        // attempt — so a checkpoint the adapter refused (`isError: true`) still swallowed the
        // model's error explanation or corrected answer. A refused checkpoint established
        // nothing, so what the model says after it is his to read.
        let log = a_checkpointed_turn_returning(
            "refused-checkpoint-then-the-correction",
            "It is landed.",
            "Correction: it is not landed yet, the checkpoint was refused.",
            true,
            r#""is_error":true,"#,
        );
        let said: Vec<&String> = log.iter().filter_map(|i| match i { TurnItem2::Said(t) => Some(t), _ => None }).collect();
        assert_eq!(
            said,
            vec!["It is landed.", "Correction: it is not landed yet, the checkpoint was refused."],
            "words after a refused checkpoint were hidden: {log:#?}"
        );
    }

    #[test]
    fn a_message_he_sends_after_the_answer_is_complete_gets_its_own_visible_reply() {
        // Product hunt 2026-10-15, finding 21, and the handover review of e1c99788e
        // (rv-20261015T120723Z-e1c99788-12ab): ending the turn at its checkpoint let the next
        // prompt reach the child while the old turn's tail was still running, the child folded
        // it into that turn (its `completed` before the `result`, the wire's own fold shape),
        // and its answer was withheld as the tail's: he got no visible reply. The turn now
        // runs to its own end, so the next prompt is written only after it, as on main.
        //
        // The child below folds a prompt that arrives during its tail, and answers one that
        // arrives after its `result` in a turn of its own. The verdict is what he reads, not
        // a clock: the `read -t` only lets the script tell the two shapes apart.
        let script = write_script("follow-up-after-the-checkpoint", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
ours=$(printf '%s' "$prompt" | sed -n 's/.*"uuid":"\([0-9a-fA-F-]*\)".*/\1/p')
test -n "$ours" || exit 9
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"started\"}"
printf '%s\n' '{"type":"stream_event","event":{"type":"message_start"}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Here are three."}}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","id":"toolu_CP","name":"mcp__richos_continuity__checkpoint","input":{}}}}'
printf '%s\n' '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_CP","content":"{\"accepted\":true,\"applied\":1,\"rejected\":0}"}]}}'
if read -r -t 2 next; then
  next_id=$(printf '%s' "$next" | sed -n 's/.*"uuid":"\([0-9a-fA-F-]*\)".*/\1/p')
  printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$next_id\",\"state\":\"queued\"}"
  printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"The answer to your thanks."}}}'
  printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$next_id\",\"state\":\"completed\"}"
  printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
  printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"completed\"}"
else
  printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Here are three subject line ideas."}}}'
  printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
  printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"completed\"}"
  read -r next
  next_id=$(printf '%s' "$next" | sed -n 's/.*"uuid":"\([0-9a-fA-F-]*\)".*/\1/p')
  printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$next_id\",\"state\":\"started\"}"
  printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"The answer to your thanks."}}}'
  printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
  printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$next_id\",\"state\":\"completed\"}"
fi
read -r keep_alive
"#);
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        let said = |client: &NativeClient, prompt: &str| {
            let mut said = String::new();
            let reason = client
                .prompt(prompt, &mut |item| {
                    if let TurnItem::Text { text, .. } = item {
                        said.push_str(text);
                    }
                })
                .unwrap();
            (reason, said)
        };

        let (reason, first) = said(&client, "Suggest three subject lines.");
        assert_eq!(reason, "end_turn");
        assert_eq!(first, "Here are three.Here are three subject line ideas.", "all authored words are delivered");
        let (reason, second) = said(&client, "Thanks.");
        assert_eq!(reason, "end_turn");
        assert_eq!(second, "The answer to your thanks.", "his follow-up got no visible reply");
    }

    #[test]
    fn the_answer_is_complete_before_bookkeeping_and_the_turn_still_ends_at_its_own_end() {
        // The presentation boundary is the start of bookkeeping after speech. Its result
        // and the terminal provider frame must still be consumed, including rejection.
        let script = write_script("answer-complete-at-the-checkpoint", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
ours=$(printf '%s' "$prompt" | sed -n 's/.*"uuid":"\([0-9a-fA-F-]*\)".*/\1/p')
test -n "$ours" || exit 9
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"started\"}"
printf '%s\n' '{"type":"stream_event","event":{"type":"message_start"}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","id":"toolu_BEFORE","name":"mcp__richos_continuity__checkpoint","input":{}}}}'
printf '%s\n' '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_BEFORE","is_error":true,"content":"Answer first."}]}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Here are three."}}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","id":"toolu_CP","name":"mcp__richos_continuity__checkpoint","input":{}}}}'
printf '%s\n' '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_CP","content":[{"type":"text","text":"{\"accepted\":true,\"applied\":1,\"rejected\":0}"}]}]}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Here are three subject line ideas."}}}'
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":7}'
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"completed\"}"
read -r prompt
ours=$(printf '%s' "$prompt" | sed -n 's/.*"uuid":"\([0-9a-fA-F-]*\)".*/\1/p')
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"started\"}"
printf '%s\n' '{"type":"stream_event","event":{"type":"message_start"}}'
printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"text","text":"Here are three."},{"type":"tool_use","id":"toolu_RJ","name":"mcp__richos_continuity__checkpoint","input":{}}]}}'
printf '%s\n' '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_RJ","content":"{\"accepted\":false,\"applied\":0,\"rejected\":1}"}]}}'
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"completed\"}"
read -r keep_alive
"#);
        let client = NativeClient::spawn(&script, &std::env::temp_dir(), &doctrine_fixture(), &skills_fixture()).unwrap();
        #[derive(Debug, PartialEq)]
        enum Seen {
            Said(String),
            Checkpoint,
            Complete,
        }
        let run = |client: &NativeClient, prompt: &str| {
            let mut log = Vec::new();
            let reason = client
                .prompt(prompt, &mut |item| match item {
                    TurnItem::Text { text, .. } => log.push(Seen::Said(text.to_string())),
                    TurnItem::Machinery(record) => {
                        if record.payload.as_ref().is_some_and(|p| p.to_string().contains("toolu_CP")) {
                            log.push(Seen::Checkpoint);
                        }
                    }
                    TurnItem::AnswerComplete => log.push(Seen::Complete),
                })
                .unwrap();
            (reason, log)
        };

        let (reason, log) = run(&client, "Suggest three subject lines.");
        assert_eq!(reason, "end_turn");
        let said: Vec<&Seen> = log.iter().filter(|s| matches!(s, Seen::Said(_))).collect();
        assert_eq!(said, vec![&Seen::Said("Here are three.".into()), &Seen::Said("Here are three subject line ideas.".into())], "all authored words survive the checkpoint: {log:?}");
        assert_eq!(log.iter().filter(|s| **s == Seen::Complete).count(),0,"checkpoint inferred answer completion");
        // THE TURN WAITED FOR ITS OWN END: its `result` was consumed by this prompt, not left
        // on the between-turn lane for whatever came next.
        assert!(
            !client.between.lock().unwrap().queue.iter().any(|i| matches!(i,
                BetweenItem::Frame(f) if f.get("type").and_then(Value::as_str) == Some("result")
                    && f.get("queued_turn_count").and_then(Value::as_u64) == Some(7))),
            "the turn ended before its own result",
        );

        let (reason, log) = run(&client, "Again, please.");
        assert_eq!(reason, "end_turn");
        assert_eq!(log.iter().filter(|s| **s == Seen::Complete).count(),0,"checkpoint inferred answer completion");
    }

    #[test]
    fn the_receipt_is_a_fact_about_one_turn_and_the_next_turn_starts_again() {
        // The three fields reset with the SEND, like `spoken_this_turn` beside them. Without
        // this, turn 2's own reply would be withheld on the strength of turn 1's receipt — which
        // is a silent Rich, the worst failure this change could have.
        let script = write_script("receipt-resets-per-turn", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","id":"toolu_R","name":"mcp__richos_assignments__record","input":{}}}}'
printf '%s\n' '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_R","content":"{\"recorded\":true,\"say\":\"On it!\"}"}]}}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"On it!"}}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
read -r prompt
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"It landed at four."}}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
"#);
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        for (prompt, expected) in [("Land the pricing branch", "On it!"), ("Did it land?", "It landed at four.")] {
            let mut said = String::new();
            let mut collect = |item: TurnItem| { if let TurnItem::Text { text, .. } = item { said.push_str(text) } };
            assert_eq!(client.prompt(prompt, &mut collect).unwrap(), "end_turn");
            assert_eq!(said, expected, "turn {prompt:?}");
        }
    }



    /// **HIS FIRST WORDS ARE WHAT OPENS THE FRONT DESK'S BOOKKEEPING** — the CEO's §55, as
    /// the app's own state machine rather than as a permission answer nobody asks for.
    ///
    /// This replaces `the_pre_reply_gate_is_about_the_checkpoint_alone`, which tested a
    /// predicate that was never reached on the real wire (see the note where that function
    /// used to be). What is asserted here is the thing the engine's adapter reads: the flag in
    /// the continuity server's own scope file, before and after the first word.
    #[test]
    fn role_definitions_remain_editable_engine_files() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app");
        let isolated = fixture_root().join(format!("plain-roles-{}", uuid::Uuid::new_v4()));
        let plugin = crate::skills::ensure_rendered(&isolated, &source).unwrap();
        let rich = plugin.join("agents/rich.md");
        std::fs::write(&rich, "---\nname: rich\n---\nAn edited role.\n").unwrap();
        crate::skills::verify_present(&plugin).unwrap();
        assert!(std::fs::read_to_string(&rich).unwrap().contains("An edited role."));
        std::fs::remove_file(plugin.join("agents/stu.md")).unwrap();
        let missing = crate::skills::verify_present(&plugin).unwrap_err();
        assert!(missing.0.ends_with("agents/stu.md"));
        std::fs::remove_dir_all(isolated).unwrap();
    }

    #[test]
    fn worker_evidence_never_replaces_a_native_success_with_a_session_kill() {
        use crate::cognition::Cognition;
        use std::collections::BTreeMap;
        // A stale, missing or unreadable callback file cannot overturn native success.
        for scenario in ["open", "background", "damaged", "missing", "unreadable", "empty", "settled"] {
            let script = write_script("audit-unsettled", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
read -r keep_alive
"#);
            let mut cognition = NativeCognition::start(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
            let root = script.parent().unwrap().join("state");
            let folder = root.join("evidence").join(&cognition.session_id);
            std::fs::create_dir_all(&folder).unwrap();
            let mut log = serde_json::json!({"schema":1,"callback":{"session_id":cognition.session_id,"hook_event_name":"SubagentStart","agent_id":"still-running"}}).to_string()+"\n";
            match scenario {
                "damaged" => log.push_str("{\"schema\":"),
                "empty" => log.clear(),
                "settled" => log.push_str(&(serde_json::json!({"schema":1,"callback":{"session_id":cognition.session_id,"hook_event_name":"SubagentStop","agent_id":"still-running"}}).to_string()+"\n")),
                "background" => log.push_str(&(serde_json::json!({"schema":1,"callback":{
                    "session_id": cognition.session_id, "hook_event_name": "PostToolUse", "tool_name": "Agent",
                    "tool_response": {"isAsync": true, "status": "async_launched", "agentId": "still-running"}}})
                    .to_string() + "\n")),
                _ => {},
            }
            std::fs::write(folder.join(".lock"), "").unwrap();
            match scenario {
                "missing" => {},
                "unreadable" => std::fs::create_dir(folder.join("callbacks.jsonl")).unwrap(),
                _ => std::fs::write(folder.join("callbacks.jsonl"), log).unwrap(),
            }
            cognition.engine_profile = Some(crate::engine_profile::EngineProfile {
                engine: root.clone(), coordination: root.clone(), plugin: root.clone(), state: root.clone(), user: Default::default(),
                runtime: crate::runtime::EngineRuntime {root: root.clone(), python:"/usr/bin/python3".into(), node:"/usr/bin/false".into(), git:"/usr/bin/git".into(),versions:BTreeMap::new()},
                work_scope:None, claude_account: None, token_sink: None, speech_model: None, lease_models: Default::default(), machine_settings: Default::default()
            });
            let result = cognition.prompt("Synthetic audit turn", &mut |_| {});
            let provider_alive = cognition.client.child.try_wait().unwrap().is_none();
            println!("SETTLEMENT scenario={scenario}, result={result:?}, provider_alive={provider_alive}");
            assert_eq!(result.unwrap(),"end_turn", "native success changed for {scenario}");
            assert!(provider_alive, "native session killed for {scenario}");
        }
    }

    #[test]
    fn nested_worker_text_and_results_never_become_the_leads_conversation() {
        let script = write_script("nested-worker-output", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"message_start"}}'
printf '%s\n' '{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Lead"}}}'
printf '%s\n' '{"type":"stream_event","parent_tool_use_id":"worker-call","event":{"type":"message_start"}}'
printf '%s\n' '{"type":"stream_event","parent_tool_use_id":"worker-call","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"WORKER STREAM"}}}'
printf '%s\n' '{"type":"assistant","parent_tool_use_id":"worker-call","message":{"content":[{"type":"text","text":"WORKER REPORT"}]}}'
printf '%s\n' '{"type":"result","parent_tool_use_id":"worker-call","stop_reason":"worker_end"}'
printf '%s\n' '{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"text","text":"Lead"}]}}'
printf '%s\n' '{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","delta":{"type":"text_delta","text":" done"}}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
read -r prompt
printf '%s\n' '{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"message_start"}}'
printf '%s\n' '{"type":"stream_event","parent_tool_use_id":"worker-call","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"WORKER AGAIN"}}}'
printf '%s\n' '{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"text","text":"Lead fallback"}]}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
"#);
        let client=NativeClient::spawn(&script,Path::new("/tmp"),&doctrine_fixture(),&skills_fixture()).unwrap();
        let mut visible=String::new();let mut reports=Vec::new();
        let reason=client.prompt("First fictional turn", &mut |item|match item {
            TurnItem::Text{text,..}=>visible.push_str(text),
            TurnItem::Machinery(record)=>reports.push(record),
            TurnItem::AnswerComplete=>{}
        }).unwrap();
        assert_eq!(reason,"end_turn");assert_eq!(visible,"Lead done");
        assert!(reports.iter().any(|r|r.title=="Worker report" && r.payload.as_ref().is_some_and(|p|p["parent_tool_use_id"]=="worker-call" && p["block"]["text"]=="WORKER REPORT")));
        let mut fallback=String::new();
        client.prompt("Second fictional turn", &mut |item|if let TurnItem::Text{text,..}=item{fallback.push_str(text)}).unwrap();
        assert_eq!(fallback,"Lead fallback");
    }


    #[test]
    fn an_aborted_turn_is_reported_as_a_cancel_on_terminal_reason_not_stop_reason() {
        // C1: this wire says `stop_reason: null` + `error_during_execution`, and ONLY
        // `terminal_reason` separates the CEO's stop from a genuine failure. Verbatim from
        // `run9-rust-driven.jsonl:65`.
        let aborted = json!({"subtype":"error_during_execution","stop_reason":Value::Null,
                             "is_error":true,"terminal_reason":"aborted_streaming"});
        assert_eq!(stop_reason_of(&aborted), STOP_REASON_CANCELLED);

        // A clean end — `run9-rust-driven.jsonl:30`.
        let clean = json!({"subtype":"success","stop_reason":"end_turn","terminal_reason":"completed"});
        assert_eq!(stop_reason_of(&clean), "end_turn");

        // A REAL failure must NOT be reported as the CEO's stop. This is the whole reason
        // the mapping keys on `terminal_reason` and not on `subtype`.
        let failed = json!({"subtype":"error_during_execution","stop_reason":Value::Null,
                            "is_error":true,"terminal_reason":"error"});
        assert_eq!(stop_reason_of(&failed), "error_during_execution");
        assert_ne!(stop_reason_of(&failed), STOP_REASON_CANCELLED);

        // And a frame with no name for how it ended is never called `end_turn`.
        assert_eq!(stop_reason_of(&json!({})), "unknown");
    }

    // ---- the derived watermark numerator -------------------------------------------------

    #[test]
    fn the_context_numerator_sums_all_three_input_sides() {
        // 2 + 25_737 + 3_603 = 29_342 — the first of the four monotonic numerators
        // `findings.md` §4 reports for run9, re-derived rather than trusted. `input_tokens`
        // alone would have said 2.
        let usage = json!({"input_tokens":2,"cache_creation_input_tokens":3603,
                           "cache_read_input_tokens":25737,"output_tokens":95});
        assert_eq!(tokens_in_context(&usage), Some(29_342));
        assert_eq!(usage["input_tokens"].as_u64(), Some(2), "and the naive read would have been 2");

        // The last of the four: 2 + 29_597 + 69 = 29_668, still monotonic against 29_342.
        let later = json!({"input_tokens":2,"cache_creation_input_tokens":69,
                           "cache_read_input_tokens":29597});
        assert_eq!(tokens_in_context(&later), Some(29_668));
        assert!(29_668 > 29_342);

        // A usage object with no input side says nothing about the context, and a zero would
        // be a false measurement.
        assert_eq!(tokens_in_context(&json!({"output_tokens":95})), None);
        assert_eq!(tokens_in_context(&json!({})), None);
    }

    // ---- the between-turn lane -----------------------------------------------------------

    fn init_frame(uuid: &str, model: &str) -> Value {
        json!({"type":"system","subtype":"init","model":model,"tools":["Bash"],
               "session_id":"sess","uuid":uuid})
    }

    #[test]
    fn the_last_value_slot_suppresses_an_identical_session_meta_repeat() {
        // The measured shape: `system/init` arrives once per turn and says the same thing
        // each time. §1.2 — "retaining every repeat is waste; retaining the last is enough
        // to reconstruct".
        let mut lane = BetweenTurn::default();
        lane.offer_frame(init_frame("u1", "claude-sonnet-5"));
        lane.offer_frame(init_frame("u2", "claude-sonnet-5"));
        lane.offer_frame(init_frame("u3", "claude-sonnet-5"));
        let drained = lane.drain("sess");
        assert_eq!(drained.len(), 1, "three repeats that differ only by uuid are one record");
        assert_eq!(lane.suppressed(), 2);
        assert_eq!(drained[0].turn_id, None, "§1.4 G4 — a between-turn record has no turn");
    }

    #[test]
    fn suppression_survives_the_per_frame_uuid_which_verbatim_comparison_would_not() {
        // THE deviation this lane needed: the frames are never byte-identical, so a slot
        // comparing them verbatim would suppress zero and look alive while doing nothing.
        let a = init_frame("acbe3949a0de", "claude-sonnet-5");
        let b = init_frame("43f5fdcdeeae", "claude-sonnet-5");
        assert_ne!(a, b, "the frames really are different bytes");
        let mut lane = BetweenTurn::default();
        lane.offer_frame(a);
        lane.offer_frame(b);
        assert_eq!(lane.suppressed(), 1);
        assert_eq!(lane.drain("sess").len(), 1);
    }

    #[test]
    fn a_changed_session_meta_value_is_kept_because_it_is_a_different_statement() {
        let mut lane = BetweenTurn::default();
        lane.offer_frame(init_frame("u1", "claude-sonnet-5"));
        lane.offer_frame(init_frame("u2", "claude-opus-5"));
        let drained = lane.drain("sess");
        assert_eq!(drained.len(), 2, "the second says something the first did not");
        assert_eq!(lane.suppressed(), 0);
        // 0 then 1 — the lane's own counter, assigned at DRAIN.
        assert_eq!(drained.iter().map(|r| r.seq).collect::<Vec<_>>(), vec![0, 1]);
    }

    #[test]
    fn the_lane_counter_continues_across_drains_rather_than_restarting() {
        let mut lane = BetweenTurn::default();
        lane.offer_frame(init_frame("u1", "claude-sonnet-5"));
        assert_eq!(lane.drain("sess")[0].seq, 0);
        lane.offer_frame(json!({"type":"system","subtype":"thinking_tokens","estimated_tokens":50}));
        assert_eq!(lane.drain("sess")[0].seq, 1, "a drain is not a reset");
    }

    #[test]
    fn a_user_text_frame_is_still_the_one_deliberate_drop() {
        let mut lane = BetweenTurn::default();
        lane.offer_frame(json!({"type":"user","message":{"role":"user","content":[
                                 {"type":"text","text":"[Request interrupted by user]"}]}}));
        assert!(lane.drain("sess").is_empty());
        // And it consumed no position: the NEXT record is still 0.
        lane.offer_frame(json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}}));
        assert_eq!(lane.drain("sess")[0].seq, 0);
    }

    #[test]
    fn an_overflow_is_a_marker_record_and_never_a_silent_forget() {
        let mut lane = BetweenTurn::default();
        // BETWEEN_TURN_MAX + 3 non-mergeable, non-meta frames. `system/thinking_tokens` is
        // not SessionMeta, so nothing is collapsed and the cap is what bites.
        for i in 0..(BETWEEN_TURN_MAX + 3) {
            lane.offer_frame(json!({"type":"system","subtype":"thinking_tokens","estimated_tokens":i}));
        }
        let drained = lane.drain("sess");
        // 256 kept + 1 marker = 257. Re-derived here rather than trusted: the cap refuses
        // the 257th onwards, so 259 offered - 256 kept = 3 dropped.
        assert_eq!(drained.len(), BETWEEN_TURN_MAX + 1);
        let marker = drained.last().unwrap();
        assert_eq!(marker.title, crate::machinery::BETWEEN_TURN_OVERFLOW);
        assert_eq!(marker.payload.as_ref().unwrap()["dropped"], 3);
        // Reported once, then reset — a second drain does not re-accuse.
        assert!(lane.drain("sess").is_empty());
    }

    #[test]
    fn between_turn_text_is_retained_as_machinery_and_never_as_a_chunk() {
        // In a turn this is the clean-output path and has no machinery record. Between turns
        // there is no turn to attach it to, so §1.4 G5 says retain rather than drop — and it
        // travels on the machinery family, never `StreamEvent::Chunk`.
        let mut lane = BetweenTurn::default();
        lane.offer_frame(json!({"type":"assistant","message":{"role":"assistant","content":[
                                 {"type":"text","text":"orphan"}]}}));
        let drained = lane.drain("sess");
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].kind, crate::machinery::MachineryKind::Unknown);
        assert_eq!(drained[0].title, "assistant:text");
        assert_eq!(drained[0].summary.as_deref(), Some("orphan"));
    }

    #[test]
    fn a_question_control_parked_between_turns_drains_as_a_machinery_record() {
        let mut lane = BetweenTurn::default();
        lane.push(BetweenItem::Permission {
            request: json!({"subtype":"can_use_tool","tool_name":"mcp__richos_questions__ask",
                            "description":"Choose the report date","tool_use_id":"toolu_Z"}),
            chosen: "allow".into(),
        });
        let drained = lane.drain("sess");
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].kind, crate::machinery::MachineryKind::PermissionRequested);
        assert_eq!(drained[0].title, "Choose the report date");
        assert_eq!(drained[0].tool_call_id.as_deref(), Some("toolu_Z"));
    }

    // =======================================================================================
    // TURN CORRELATION — a prompt is answered by ITS OWN turn (see `PendingTurn`)
    // =======================================================================================

    /// The frame sequence below is the green `work_lease_roundtrip` run of 2026-09-18 replayed
    /// from its journal (`docs/verification/worker-turn-grant-2026-09-18.md` §5): the host sent
    /// its continuation (`callbacks.jsonl` row 95) while the platform's subagent hand-back
    /// (row 92) was already in flight, and the hand-back's `result` was handed to the host as
    /// the answer to its own prompt — back in **1.847 s** with **zero** tool calls.
    ///
    /// **RED at `c8bcfe90`**: `prompt` returned on the first `result` it saw, so `said` was the
    /// injected turn's sentence and the host's own turn streamed into nothing.
    #[test]
    fn a_turn_the_platform_injected_never_answers_the_prompt_this_client_sent() {
        let script = write_script("injected-turn", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
ours=$(printf '%s' "$prompt" | sed -n 's/.*"uuid":"\([0-9a-fA-F-]*\)".*/\1/p')
test -n "$ours" || exit 9
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"queued\"}"
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"the hand-back turn"}}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn","queued_turn_count":1}'
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"started\"}"
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"landing it"}}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn","queued_turn_count":0}'
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"completed\"}"
read -r keep_alive
"#);
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        let mut said = String::new();
        let reason = client
            .prompt("The helper you started has ended", &mut |item| {
                if let TurnItem::Text { text, .. } = item {
                    said.push_str(text);
                }
            })
            .unwrap();
        assert_eq!(reason, "end_turn");
        // The whole defect in one assertion: the sentence this prompt was answered with.
        assert_eq!(said, "landing it", "the prompt was answered by a turn it did not send");
        // And the injected turn's traffic is RETAINED rather than dropped (§1.4 G5) and rather
        // than attributed to this turn (§1.4 G4).
        let retained = client.drain_between_turn("sess");
        assert!(
            retained.iter().any(|record| record.turn_id.is_none()),
            "the injected turn's frames reached neither this turn nor the thread",
        );
    }

    /// **THE PLATFORM'S OWN TURN CAN REPORT ZERO QUEUED WHILE OURS IS STILL IN ITS QUEUE.**
    ///
    /// Measured on `claude` 2.1.283, 2026-09-27 (richos-hq
    /// `docs/verification/2026-09-27-background-command-finish/`, `cap-continue.jsonl`): a
    /// background command ended, the child started its own `<task-notification>` turn, and the
    /// host's message written 0 ms after the notification was answered `queued` at 15.141 s.
    /// The platform's turn then ended at 17.374 s with `"queued_turn_count": 0`, and the child
    /// said `started` for our message only at 17.375 s. The count is not a statement about our
    /// message at that instant; the lifecycle is, and it says ours had not run.
    ///
    /// **RED at `238557bb`**: the count was read first, so the platform's result answered this
    /// prompt with none of its own items, and the host's real turn streamed into nothing.
    #[test]
    fn a_platform_turn_that_counts_nothing_queued_still_never_answers_a_prompt_the_child_said_is_queued() {
        let script = write_script("injected-turn-count-zero", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
ours=$(printf '%s' "$prompt" | sed -n 's/.*"uuid":"\([0-9a-fA-F-]*\)".*/\1/p')
test -n "$ours" || exit 9
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"queued\"}"
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Background command completed (exit code 0)."}}}'
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"started\"}"
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"It printed bg-marker-done."}}}'
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"completed\"}"
read -r keep_alive
"#);
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        let mut said = String::new();
        client
            .prompt("The command you started has ended", &mut |item| {
                if let TurnItem::Text { text, .. } = item {
                    said.push_str(text);
                }
            })
            .unwrap();
        assert_eq!(said, "It printed bg-marker-done.", "the platform's own turn answered this prompt");
    }

    /// **An earlier turn's closing `result`, still unread when the next prompt is parked,
    /// never ends that prompt on a child that names its turns** (hunt 2026-09-29 part 1 v2,
    /// finding 47; the witness is `part-1-codex-v2/witness-v2.rs` in richos-hq, ported here).
    ///
    /// The first turn establishes that this child names our turns. For the second, a platform
    /// or background turn's `result` with `queued_turn_count: 0` reaches the reader BEFORE any
    /// lifecycle for our message — the count is 0 because our message had not been enqueued
    /// when that result was produced.
    ///
    /// **RED at `c6cde6cc9`**: the parked prompt was `Unconfirmed`, so the count decided and the
    /// old result returned `end_turn` with no answer; "actual answer" streamed afterwards onto
    /// the between-turn lane.
    #[test]
    fn an_earlier_turns_result_never_ends_a_prompt_the_naming_child_has_not_named_yet() {
        let script = write_script("earlier-result-before-our-lifecycle", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
ours=$(printf '%s' "$prompt" | sed -n 's/.*"uuid":"\([0-9a-fA-F-]*\)".*/\1/p')
test -n "$ours" || exit 9
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"started\"}"
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
read -r prompt
ours=$(printf '%s' "$prompt" | sed -n 's/.*"uuid":"\([0-9a-fA-F-]*\)".*/\1/p')
test -n "$ours" || exit 9
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0,"result":"old platform result"}'
sleep 0.2
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"started\"}"
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"actual answer"}}}'
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
read -r keep_alive
"#);
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        client.prompt("first", &mut |_| {}).unwrap();
        let mut said = String::new();
        let reason = client
            .prompt("second", &mut |item| {
                if let TurnItem::Text { text, .. } = item {
                    said.push_str(text);
                }
            })
            .unwrap();
        assert_eq!(reason, "end_turn");
        assert_eq!(said, "actual answer", "an earlier turn's result ended this prompt before its own answer");
        // The earlier result is retained on the between-turn lane rather than dropped (§1.4 G5).
        assert!(
            client.between.lock().unwrap().queue.iter().any(|i| matches!(i,
                BetweenItem::Frame(f) if f.get("result").and_then(Value::as_str) == Some("old platform result"))),
            "the earlier turn's result reached neither this turn nor the between-turn lane",
        );
    }

    /// **A background command is read off the provider's own frames, whichever turn they
    /// arrive in** — and its ending says whether the model had it before it answered.
    ///
    /// The frames are the ones 2.1.283 sent on 2026-09-27 (`cap-plain.jsonl`, `cap-fold.jsonl`
    /// in richos-hq `docs/verification/2026-09-27-background-command-finish/`), trimmed to the
    /// fields read. Three facts in one script:
    ///
    /// 1. a command that ends after its turn is recorded as ended OUTSIDE a turn of ours —
    ///    the case whose report reached nobody before this;
    /// 2. a command that ends while a turn of ours runs is recorded as inside it;
    /// 3. a FOREGROUND command, which gets the same two frames with `is_backgrounded: false`,
    ///    is not a background command at all.
    #[test]
    fn a_background_command_is_read_off_the_providers_frames_and_its_ending_says_whose_turn_had_it() {
        let script = write_script("background-commands", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"bk1c0clka","task_type":"local_bash","description":"sleep 12; echo bg-marker-done"}]}'
printf '%s\n' '{"type":"system","subtype":"task_started","task_id":"bk1c0clka","task_type":"local_bash","is_backgrounded":true,"description":"sleep 12; echo bg-marker-done"}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Started."}}}'
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"bk1c0clka","status":"completed","summary":"Background command \"sleep 12; echo bg-marker-done\" completed (exit code 0)"}'
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
read -r second
printf '%s\n' '{"type":"system","subtype":"task_started","task_id":"b50y3od71","task_type":"local_bash","is_backgrounded":true,"description":"sleep 4"}'
printf '%s\n' '{"type":"system","subtype":"task_started","task_id":"bil7jpdo5","task_type":"local_bash","is_backgrounded":false,"description":"sleep 12"}'
printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"b50y3od71","status":"completed","summary":"sleep 4 completed (exit code 0)"}'
printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"bil7jpdo5","status":"completed","summary":"sleep 12"}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Both done."}}}'
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
read -r keep_alive
"#);
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        assert!(client.background_commands().is_empty(), "a fresh session reported a command");
        client.prompt("start it in the background", &mut |_| {}).unwrap();
        // The notification arrives after the turn: wait for the reader, on the reading itself.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while client.background_commands().first().is_none_or(|c| c.ended.is_none()) {
            assert!(std::time::Instant::now() < deadline, "the ending was never read");
            std::thread::sleep(Duration::from_millis(5));
        }
        let first = client.background_commands();
        assert_eq!(first.len(), 1, "one command, reported by two frames, is one command: {first:?}");
        assert_eq!(first[0].task_id, "bk1c0clka");
        assert_eq!(first[0].description, "sleep 12; echo bg-marker-done");
        let ended = first[0].ended.clone().unwrap();
        assert_eq!(ended.status, "completed");
        assert!(ended.summary.contains("exit code 0"), "{}", ended.summary);
        assert!(!ended.during_a_turn_of_ours, "an ending after the turn was counted as inside it");

        // The provider's closing `result` for that ending follows the notification and is read
        // a moment later. Sent before it is read, the next prompt would be parked, unconfirmed,
        // when it arrives and would take it for its own turn's end (a slow reader, as under a
        // parallel build, then returned the second turn before its frames). So wait for the
        // reader to have parked that `result` on the between-turn lane, on the reading itself.
        // load-bound: hang guard only; the loop waits for the fact, the deadline decides no verdict
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !client.between.lock().unwrap().queue.iter().any(
            |i| matches!(i, BetweenItem::Frame(f) if f.get("type").and_then(Value::as_str) == Some("result")),
        ) {
            assert!(std::time::Instant::now() < deadline, "the closing result was never read");
            std::thread::sleep(Duration::from_millis(5));
        }

        client.prompt("start another and run one in the foreground", &mut |_| {}).unwrap();
        let all = client.background_commands();
        assert_eq!(
            all.iter().map(|c| c.task_id.as_str()).collect::<Vec<_>>(),
            ["bk1c0clka", "b50y3od71"],
            "a foreground command was taken for a background one",
        );
        assert!(all[1].ended.as_ref().is_some_and(|e| e.during_a_turn_of_ours), "{:?}", all[1]);
    }

    /// **Every background task type the back end can start and the provider reports the end
    /// of is read — not only a shell command** (richos-hq
    /// `docs/verification/2026-09-27-background-command-finish-2/`, item 4).
    ///
    /// The frames are the ones 2.1.283 sent on 2026-09-27, trimmed to the fields read:
    /// `cap-workflow.jsonl` (a `Workflow` launch: `local_workflow`, with NO `is_backgrounded`
    /// field, ending 11.2 s after the turn that started it, and a foreground shell command of
    /// its own agent), `cap-agent-slow.jsonl` (a background `Agent`: `local_agent`, which in a
    /// work lease is only ever a prepared helper and is waited for through `SubagentStop`),
    /// and two housekeeping types from the provider's own task table that are the provider's
    /// work, not the back end's (`dream`, `auto_mode_scan`, both `skipTranscript`).
    #[test]
    fn every_background_task_type_the_back_end_can_start_is_read_and_a_helper_is_not() {
        let script = write_script("background-task-types", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"wegtzwydr","task_type":"local_workflow","description":"Wait 10 seconds and return done"}]}'
printf '%s\n' '{"type":"system","subtype":"task_started","task_id":"wegtzwydr","description":"Wait 10 seconds and return done","task_type":"local_workflow","workflow_name":"wait-and-return"}'
printf '%s\n' '{"type":"system","subtype":"task_started","task_id":"a0cb81caf11b7e9a2","description":"Background sleep and reply","is_backgrounded":true,"task_type":"local_agent"}'
printf '%s\n' '{"type":"system","subtype":"task_started","task_id":"d1dream","description":"dreaming","task_type":"dream"}'
printf '%s\n' '{"type":"system","subtype":"task_started","task_id":"e1scan","description":"scanning for auto-mode setup","task_type":"auto_mode_scan"}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Workflow launched."}}}'
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
printf '%s\n' '{"type":"system","subtype":"task_started","task_id":"b12emlbnd","description":"Wait approximately 10 seconds","is_backgrounded":false,"task_type":"local_bash"}'
printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"b12emlbnd","status":"completed","summary":"Wait approximately 10 seconds"}'
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"wegtzwydr","status":"completed","summary":"Dynamic workflow \"Wait 10 seconds and return done\" completed"}'
printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"a0cb81caf11b7e9a2","status":"completed","summary":"done"}'
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn","queued_turn_count":0}'
read -r keep_alive
"#);
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        client.prompt("start the workflow", &mut |_| {}).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while client.background_commands().first().is_none_or(|c| c.ended.is_none()) {
            assert!(std::time::Instant::now() < deadline, "the workflow's ending was never read: {:?}", client.background_commands());
            std::thread::sleep(Duration::from_millis(5));
        }
        let all = client.background_commands();
        assert_eq!(all.iter().map(|c| c.task_id.as_str()).collect::<Vec<_>>(), ["wegtzwydr"],
                   "a helper, a housekeeping task or a foreground command was taken for the back end's own: {all:?}");
        assert_eq!(all[0].description, "Wait 10 seconds and return done");
        let ended = all[0].ended.clone().unwrap();
        assert!(ended.summary.contains("completed") && !ended.during_a_turn_of_ours, "{ended:?}");
    }

    /// **The fallback, alone.** A child that names no command (an older binary, or a command it
    /// enqueued without our uuid) still reports `queued_turn_count`, and that count alone is
    /// enough to refuse a result belonging to a turn ahead of ours.
    #[test]
    fn a_result_with_a_user_send_still_queued_is_not_this_prompts_answer() {
        let script = write_script("queued-count-only", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"result","stop_reason":"end_turn","queued_turn_count":2}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn","queued_turn_count":1}'
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"mine"}}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn","queued_turn_count":0}'
read -r keep_alive
"#);
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        let mut said = String::new();
        client
            .prompt("carry on", &mut |item| {
                if let TurnItem::Text { text, .. } = item {
                    said.push_str(text);
                }
            })
            .unwrap();
        // Two results said a user send was still waiting; only the third was ours, and it is
        // the only one whose text this turn was given.
        assert_eq!(said, "mine", "the prompt returned before its own turn ran");
    }

    /// **THE FLOOR, kept deliberately.** A child that offers neither correlation field behaves
    /// exactly as this file did before 2026-09-18 — the next `result` answers the prompt.
    /// Without this, the change would be a silent new way to hang.
    #[test]
    fn a_child_that_names_no_turn_at_all_still_answers_the_next_result() {
        let script = write_script("no-correlation", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"answered"}}}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
read -r keep_alive
"#);
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        let mut said = String::new();
        assert_eq!(
            client
                .prompt("hello", &mut |item| {
                    if let TurnItem::Text { text, .. } = item {
                        said.push_str(text);
                    }
                })
                .unwrap(),
            "end_turn",
        );
        assert_eq!(said, "answered");
    }

    /// **A command the child throws away ends the wait, positively.** `refused` (and
    /// `discarded`) are the child saying this turn will never run; without reading them the
    /// correlation above would wait for a `result` that is never coming, which is a worse
    /// failure than the one it fixes.
    #[test]
    fn a_command_the_child_refuses_ends_the_wait_rather_than_hanging() {
        let script = write_script("refused-command", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
ours=$(printf '%s' "$prompt" | sed -n 's/.*"uuid":"\([0-9a-fA-F-]*\)".*/\1/p')
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"queued\"}"
printf '%s\n' "{\"type\":\"command_lifecycle\",\"command_uuid\":\"$ours\",\"state\":\"refused\"}"
read -r keep_alive
"#);
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        assert_eq!(client.prompt("carry on", &mut |_| {}).unwrap(), "refused_before_it_ran");
    }

    // ---- AMERICAN SPELLING AT THE DRAIN (CEO §93) --------------------------------------------

    /// What one scripted turn delivered: every item in delivery order (`None` text for a
    /// machinery record), the turn's result, and the prompt line the child received.
    struct BritishTurn {
        items: Vec<(u64, Option<String>)>,
        result: Result<String, NativeError>,
        received: String,
    }

    /// One scripted turn whose text is `blocks` (each a list of deltas, streamed in its own text
    /// block, with a tool call between the blocks), then `ending`: `"result"` for a normal end,
    /// `"exit"` for a child that dies with no `result`.
    fn a_british_turn(tag: &str, prompt: &str, blocks: &[Vec<String>], ending: &str) -> BritishTurn {
        let dir = fixture_root().join(format!("american-{tag}-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let ev = |event: Value| json!({"type": "stream_event", "event": event}).to_string();
        let mut frames = vec![ev(json!({"type": "message_start"}))];
        for (i, deltas) in blocks.iter().enumerate() {
            if i > 0 {
                frames.push(ev(json!({"type": "content_block_start",
                    "content_block": {"type": "tool_use", "id": format!("toolu_{i}"), "name": "Read", "input": {}}})));
                frames.push(ev(json!({"type": "content_block_stop"})));
            }
            frames.push(ev(json!({"type": "content_block_start", "content_block": {"type": "text", "text": ""}})));
            for delta in deltas {
                frames.push(ev(json!({"type": "content_block_delta", "delta": {"type": "text_delta", "text": delta}})));
            }
            // A child that dies does so mid-block: nothing after its last delta.
            if !(ending == "exit" && i + 1 == blocks.len()) {
                frames.push(ev(json!({"type": "content_block_stop"})));
            }
        }
        if ending == "result" {
            frames.push(json!({"type": "result", "stop_reason": "end_turn"}).to_string());
        }
        let frames_path = dir.join("frames.jsonl");
        std::fs::write(&frames_path, frames.join("\n") + "\n").unwrap();
        let received = dir.join("received");
        let script = write_script(tag, &format!(r#"
read -r init
printf '%s\n' '{{"type":"control_response","response":{{"subtype":"success","request_id":"req_init","response":{{}}}}}}'
read -r prompt
printf '%s\n' "$prompt" > '{received}'
cat '{frames}'
"#, received = received.display(), frames = frames_path.display()));
        let client = NativeClient::spawn(&script, Path::new("/tmp"), &doctrine_fixture(), &skills_fixture()).unwrap();
        let mut items = Vec::new();
        let result = client.prompt(prompt, &mut |item| match item {
            TurnItem::Text { seq, text } => items.push((seq, Some(text.to_string()))),
            TurnItem::Machinery(record) => items.push((record.seq, None)),
            TurnItem::AnswerComplete => {}
        });
        BritishTurn { items, result, received: std::fs::read_to_string(&received).unwrap_or_default() }
    }

    /// A fixture section of `|`-separated deltas (British on purpose, in the exempt location).
    fn british_deltas(section: &str) -> Vec<String> {
        crate::american_spelling::tests::section(section)
            .trim_end_matches('\n')
            .split('|')
            .map(str::to_string)
            .collect()
    }

    fn said_in(items: &[(u64, Option<String>)]) -> String {
        items.iter().filter_map(|(_, t)| t.clone()).collect()
    }

    #[test]
    fn a_british_reply_reaches_him_american_and_his_own_words_reach_the_model_as_he_said_them() {
        let prompt = crate::american_spelling::tests::section("drain-prompt").trim_end().to_string();
        let first = british_deltas("drain-deltas-1");
        let second = british_deltas("drain-deltas-2");
        let BritishTurn { items, result, received } = a_british_turn("reply", &prompt, &[first.clone(), second.clone()], "result");
        assert_eq!(result.unwrap(), "end_turn");

        // HIS WORDS: the child received exactly what he wrote, British spelling and all.
        let sent: Value = serde_json::from_str(received.trim()).expect("the child saw the prompt line");
        assert_eq!(sent["message"]["content"][0]["text"].as_str(), Some(prompt.as_str()), "his words were rewritten");
        assert_ne!(crate::american_spelling::fix(&prompt), prompt, "the prompt fixture must carry table words");

        // THE REPLY: each block is exactly the fixer's answer for that block, so what is
        // stored delta by delta is what `message-completed` reads back.
        let tool_at = items.iter().position(|(_, t)| t.is_none()).expect("the tool call is delivered");
        let before = said_in(&items[..tool_at]);
        let after = said_in(&items[tool_at..]);
        assert_eq!(before, crate::american_spelling::fix(&first.concat()));
        assert_eq!(after, crate::american_spelling::fix(&second.concat()));
        assert_ne!(before, first.concat(), "the first block must have changed");
        assert_ne!(after, second.concat(), "the second block must have changed");

        // ORDER: one counter, strictly increasing, the held tail delivered before the tool
        // call, and no empty text item.
        let seqs: Vec<u64> = items.iter().map(|(s, _)| *s).collect();
        assert!(seqs.windows(2).all(|w| w[0] < w[1]), "seq must strictly increase: {seqs:?}");
        assert!(items.iter().all(|(_, t)| t.as_deref() != Some("")), "an empty text item was delivered");
    }

    /// **A CHILD THAT ENDS MID-TURN TAKES ITS OWN WORDS TO THE TURN'S REASON** (a new user's
    /// first run, 2026-10-21: every message ended "RichOS lost its connection to Claude Code
    /// ... This usually fixes itself", the turn's reason was "claude channel closed (child
    /// exited?)" and nothing on his Mac said why). Red on main, where the turn ends
    /// [`NativeError::Closed`] with nothing in it. The child here writes its error on stderr
    /// and exits after `system:init`, as his did; asked `auth status`, it says signed out. The
    /// reason carries both, and `interruption::classify` then tells him to sign in.
    #[test]
    fn a_child_that_ends_mid_turn_names_why_in_the_turns_reason() {
        let script = write_script("exits-mid-turn", r#"
if [ "$1" = auth ]; then echo '{"loggedIn": false}'; exit 1; fi
exec python3 -u -c '
import json,sys
init=json.loads(sys.stdin.readline())
print(json.dumps({"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}),flush=True)
prompt=json.loads(sys.stdin.readline())
print(json.dumps({"type":"system","subtype":"init"}),flush=True)
sys.stderr.write("Error: the request could not be authenticated\n"); sys.stderr.flush()
sys.exit(1)
'
"#);
        let client = NativeClient::spawn(&script, fixture_root(), &doctrine_fixture(), &skills_fixture()).unwrap();
        let result = client.prompt("what do I do now?", &mut |_| {});
        let Err(error) = result else { panic!("a child that exited mid-turn answered: {result:?}") };
        assert!(matches!(error, NativeError::Exited { .. }), "{error:?}");
        let reason = CognitionError::from(error).to_string();
        assert!(reason.contains("the child said: Error: the request could not be authenticated"), "{reason}");
        assert!(reason.contains(CHILD_EXIT_SIGNED_OUT), "{reason}");
        assert_eq!(crate::interruption::classify(&reason), crate::interruption::InterruptionCause::NotSignedIn, "{reason}");
    }

    /// The same end, with nothing on stderr and a sign-in that is fine: the reason says so
    /// plainly, and it is the one case left to the lost-connection arm.
    #[test]
    fn child_exit_said_reports_an_empty_stderr_and_no_sign_in_verdict_plainly() {
        let done = AtomicBool::new(true);
        let tail = Mutex::new(std::collections::VecDeque::new());
        assert_eq!(child_exit_said(&done, &tail, || false), "it wrote nothing on stderr");
        let tail = Mutex::new((1..=9).map(|n| format!("line {n}")).collect::<std::collections::VecDeque<_>>());
        assert_eq!(child_exit_said(&done, &tail, || true),
            format!("the child said: line 4 / line 5 / line 6 / line 7 / line 8 / line 9; {CHILD_EXIT_SIGNED_OUT}"));
    }

    #[test]
    fn a_child_that_dies_mid_reply_still_delivers_its_held_word_fixed() {
        let third = british_deltas("drain-deltas-3");
        let BritishTurn { items, result, .. } = a_british_turn("crash", "go on", std::slice::from_ref(&third), "exit");
        assert!(matches!(result, Err(NativeError::Exited { .. })), "a vanished child ends the turn with its own words: {result:?}");
        assert_eq!(said_in(&items), crate::american_spelling::fix(&third.concat()), "the partial reply must be kept, fixed");
        assert_ne!(said_in(&items), third.concat());
    }
    #[test]
    fn independent_stu_stops_an_unnamed_native_agent_without_interrupting_the_lead() {
        use crate::steering::CommandStop;
        let script = write_script("independent-agent-stop", r#"
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
printf '%s\n' '{"type":"system","subtype":"task_started","task_type":"local_agent","task_id":"native-first","tool_use_id":"prepared-first"}'
printf '%s\n' '{"type":"system","subtype":"task_started","task_type":"local_agent","task_id":"native-second","tool_use_id":"prepared-second"}'
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn"}'
read -r stop
case "$stop" in *'"subtype":"stop_task","task_id":"native-first"'*) ;; *) exit 9;; esac
id=$(printf '%s' "$stop" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
printf '%s\n' "{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"$id\",\"response\":{}}}"
printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"native-first","status":"stopped"}'
read -r keep_alive
"#);
        let client = NativeClient::spawn(&script, fixture_root(), &doctrine_fixture(), &skills_fixture()).unwrap();
        client.prompt("start fixture", &mut |_| {}).unwrap();
        assert_eq!(client.cancel_handle().stop_worker("unobserved-call"), CommandStop::NotDelivered);
        assert_eq!(client.cancel_handle().stop_worker("prepared-first"), CommandStop::Ended);
        let state=client.reader_state.lock().unwrap();
        assert!(state.agent_tasks.task_ended("native-first"));
        assert!(!state.agent_tasks.task_ended("native-second"));
        assert_eq!(state.agent_tasks.task_for_tool("prepared-second").as_deref(),Some("native-second"));
    }

}

#[cfg(test)]
mod input_tests {
//! Exercise the native transport and durable Host receipts together. No real provider.
use super::*;
use crate::{cognition::{Cognition, CognitionError, LeaseFactory}, entity::EntityId,
    status_tools::StatusToolScope, work_host::{SilentNotifier, WorkHost}};
use std::{path::PathBuf, sync::atomic::AtomicU64};

const PROVIDER: &str = r#"#!/usr/bin/python3
import json,pathlib,sys
config=json.loads(pathlib.Path('config.json').read_text())
def send(v):print(json.dumps(v),flush=True)
def read():
 line=sys.stdin.readline()
 if not line:raise SystemExit(0)
 v=json.loads(line)
 if v.get('type')=='user':
  with pathlib.Path('received.jsonl').open('a') as f:f.write(json.dumps(v)+'\n')
 return v
def started(v):send({'type':'command_lifecycle','command_uuid':v['uuid'],'state':'started'})
init=read();send({'type':'control_response','response':{'subtype':'success','request_id':init['request_id'],'response':{}}})
if config.get('recover'):
 while True:
  v=read()
  if v.get('type')=='user':
   started(v);send({'type':'result','stop_reason':'end_turn'})
first=read();started(first)
send({'type':'assistant','message':{'content':[{'type':'text','text':'ORIGINAL-STARTED'}]}})
live=[read() for _ in range(config['inputs'])]
for v in live:send({'type':'command_lifecycle','command_uuid':v['uuid'],'state':'queued'})
send({'type':'result','stop_reason':'end_turn','queued_turn_count':len(live)})
for v in live:started(v)
output=config['output']
if output=='text':send({'type':'assistant','message':{'content':[{'type':'text','text':'Live output'}]}})
if output=='thinking':send({'type':'stream_event','event':{'type':'content_block_delta','delta':{'type':'thinking_delta','thinking':'considering'}}})
if output=='worker':
 send({'type':'assistant','parent_tool_use_id':'background-worker','message':{'content':[{'type':'tool_use','id':'worker-tool','name':'Read','input':{}}]}})
 send({'type':'result','parent_tool_use_id':'background-worker','is_error':True,'api_error_status':429})
# Native retries are traffic within this one input, never a reason to submit it again.
send({'type':'system','subtype':'api_retry','attempt':1,'max_retries':10,'error_status':503})
if config['result'].get('fixture_quit'):
 while True:read()
for v in live:send({'type':'command_lifecycle','command_uuid':v['uuid'],'state':'completed'})
send(config['result'])
while True:read()
"#;

struct Factory {
    root: PathBuf,
    skills: PathBuf,
    started: Sender<()>,
    completed: Sender<()>,
    scheduled: Arc<AtomicU64>,
    observed: Option<Sender<()>>,
}
struct Lease { client: NativeClient, started: Sender<()>, completed: Sender<()>, scheduled: Arc<AtomicU64>, observed: Option<Sender<()>> }
struct LiveInput { inner: Arc<dyn TurnCancel>, completed: Sender<()> }
impl LeaseFactory for Factory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> { unreachable!() }
    fn spawn_work(&self, _: &crate::entity::ThreadBinding) -> Result<Box<dyn Cognition>, CognitionError> {
        Ok(Box::new(Lease {
            client: NativeClient::spawn(&self.root.join("provider"), &self.root,
                &self.skills.join("agents/rich.md"), &self.skills)?,
            started: self.started.clone(), completed: self.completed.clone(), scheduled:self.scheduled.clone(), observed:self.observed.clone(),
        }))
    }
}
impl TurnCancel for LiveInput {
    fn cancel(&self) -> bool { self.inner.cancel() }
    fn shutdown(&self) { self.inner.shutdown(); }
    fn send_input(&self, id: &str, text: &str,
        completed: Box<dyn FnOnce(crate::steering::InputOutcome) + Send>) -> Result<bool, String> {
        let done = self.completed.clone();
        self.inner.send_input(id, text, Box::new(move |outcome| {
            completed(outcome);
            let _ = done.send(());
        }))
    }
}
impl Cognition for Lease {
    fn set_between_turn_sink(&mut self, sink: crate::cognition::BetweenTurnSink) {
        let observed = self.observed.clone();
        self.client.set_between_turn_sink(Arc::new(move |records| {
            let terminal = records.iter().any(|frame|
                frame["type"] == "result" && frame.get("parent_tool_use_id").is_none_or(Value::is_null));
            sink(records);
            if terminal { if let Some(observed) = &observed { observed.send(()).expect("native result observer receiver dropped"); } }
        }));
    }
    fn session_id(&self) -> &str { self.client.session_id() }

    fn cancel_handle(&self) -> Option<Arc<dyn TurnCancel>> {
        Some(Arc::new(LiveInput { inner: self.client.cancel_handle(), completed: self.completed.clone() }))
    }
    fn prompt(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        panic!("native submission and reply waiting are separate")
    }
    fn is_closed(&self)->bool {self.client.reader_closed.load(Ordering::SeqCst)}
    fn enqueue_prompt(&mut self,input:&str,started:Box<dyn FnOnce()->Result<(),String>+Send>)->Result<crate::cognition::PendingReply,CognitionError> {
        started().map_err(CognitionError::Protocol)?;
        self.scheduled.fetch_add(1,Ordering::SeqCst);
        let reply=self.client.enqueue_prompt(input,None)?;
        let entered=self.started.clone();let completed=self.completed.clone();
        Ok(crate::cognition::PendingReply::new(move |on| {
            let result=reply.wait(&mut |item| {
                let original=matches!(&item,TurnItem::Text {text,..} if text.contains("ORIGINAL-STARTED"));
                on(item);
                if original {let _=entered.send(());}
            });
            let _=completed.send(());
            result.map_err(CognitionError::from)
        }))
    }
}

#[test]
fn live_native_results_retain_failed_input_report_once_and_recover_once() {
    for inputs in [1, 2] {
        for output in ["none", "text", "thinking", "worker"] {
            for result in [json!({"type":"result","stop_reason":"end_turn"}),
                json!({"type":"result","is_error":true,"errors":["API Error: 503"]}),
                json!({"type":"result","stop_reason":"max_tokens"}), json!({"fixture_quit":true})] {
                let success = result["stop_reason"] == "end_turn";
                let quit = result["fixture_quit"] == true;
                let root = std::env::temp_dir().join(format!("richos-native-input-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir_all(&root).unwrap();
                let script = root.join("provider");
                std::fs::write(&script, PROVIDER).unwrap();
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
                let skills = crate::skills::ensure_rendered(&root.join("skills"),
                    &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).unwrap();
                std::fs::write(root.join("config.json"), json!({"inputs":inputs,"output":output,"result":result}).to_string()).unwrap();
                let mut ledger = crate::ledger::Ledger::open(root.join("ledger.jsonl")).unwrap();
                let thread = ledger.create_thread("Native input test", &EntityId::parse("company").unwrap()).unwrap();
                let binding = ledger.thread_binding(&thread).unwrap();
                let scope = StatusToolScope { version:1, state_root:root.join("state"), entity_id:"company".into(), thread_id:thread };
                let status = root.join("rich.json");
                crate::session_tools::bind(&status, &scope, "rich-session", "rich", None).unwrap();
                let (began, started) = channel();
                let (done, completed) = channel();
                let scheduled = Arc::new(AtomicU64::new(0));
                let host = WorkHost::new(&scope.state_root, Box::new(Factory {
                    root:root.clone(), skills:skills.clone(), started:began.clone(), completed:done.clone(), scheduled:scheduled.clone(), observed:None,
                }), Arc::new(SilentNotifier));
                crate::session_tools::send(&status, &scope, json!({"text":"Original input"})).unwrap();
                host.wake_messages(&binding);
                // load-bound: hang guard only; the native text marker proves entry.
                started.recv_timeout(Duration::from_secs(10)).expect("original did not enter");
                for n in 0..inputs {
                    crate::session_tools::send(&status, &scope, json!({"text":format!("Live correction {n}")})).unwrap();
                }
                host.wake_messages(&binding);
                if quit {
                    // load-bound: hang guard only; the original result proves the live input has been read.
                    completed.recv_timeout(Duration::from_secs(10)).expect("original result absent before Quit");
                    assert!(host.wait_for_completed(1, Duration::from_secs(10)));
                    host.shutdown();
                }
                for _ in 0..(inputs + usize::from(!quit)) {
                    // load-bound: hang guard only; callback completion proves the durable outcome was written.
                    completed.recv_timeout(Duration::from_secs(10)).expect("live result did not complete");
                }
                assert!(host.wait_for_completed(scheduled.load(Ordering::SeqCst), Duration::from_secs(10)), "original result did not complete");
                // Repeated desktop notifications cannot submit a received input again.
                host.wake_messages(&binding);
                let pending = crate::session_tools::pending(&scope, "stu").unwrap();
                assert_eq!(pending.is_none(), success, "{inputs}/{output}/{result}: incorrect durable receipt");
                let fact = crate::session_tools::pending(&scope, "rich").unwrap();
                assert_eq!(fact.is_none(), success || quit, "{inputs}/{output}/{result}: missing or unwanted failure fact");
                if let Some((message, through)) = fact {
                    assert_eq!(message.from, "work-status");
                    assert_eq!(serde_json::from_str::<Value>(&message.text).unwrap()["event"], "upstream_turn");
                    crate::session_tools::acknowledge(&scope, "rich", through).unwrap();
                    assert!(crate::session_tools::pending(&scope, "rich").unwrap().is_none(), "folded turn reported twice");
                }
                host.shutdown();
                let sent = std::fs::read_to_string(root.join("received.jsonl")).unwrap();
                assert_eq!(sent.lines().count(), inputs + 1, "same-session duplicate: {sent}");
                assert_eq!(sent.lines().filter(|line| line.contains("Original input")).count(), 1);
                for n in 0..inputs { assert_eq!(sent.lines().filter(|line| line.contains(&format!("Live correction {n}"))).count(), 1); }
                if !success {
                    assert!(crate::session_tools::pending(&scope, "stu").unwrap().is_some(), "Quit discarded unfinished input");
                    std::fs::write(root.join("config.json"), r#"{"recover":true}"#).unwrap();
                    let scheduled = Arc::new(AtomicU64::new(0));
                    let recovered = WorkHost::new(&scope.state_root, Box::new(Factory {
                        root:root.clone(), skills, started:began, completed:done, scheduled:scheduled.clone(), observed:None,
                    }), Arc::new(SilentNotifier));
                    recovered.wake_messages(&binding);
                    for _ in 0..inputs {
                        // load-bound: hang guard only; all native outcomes must arrive before checking recovery receipts.
                        completed.recv_timeout(Duration::from_secs(10)).expect("recovery result absent");
                    }
                    assert!(recovered.wait_for_completed(scheduled.load(Ordering::SeqCst), Duration::from_secs(10)), "restart did not finish inputs");
                    recovered.shutdown();
                    assert!(crate::session_tools::pending(&scope, "stu").unwrap().is_none());
                    assert!(crate::session_tools::pending(&scope, "rich").unwrap().is_none(), "successful recovery reported failure");
                    let sent = std::fs::read_to_string(root.join("received.jsonl")).unwrap();
                    assert_eq!(sent.lines().count(), inputs * 2 + 1);
                    for n in 0..inputs { assert_eq!(sent.lines().filter(|line| line.contains(&format!("Live correction {n}"))).count(), 2); }
                }
                std::fs::remove_dir_all(root).unwrap();
            }
        }
    }
}

#[test]
fn nested_worker_limit_does_not_fail_the_successful_native_prompt() {
    assert_eq!(super::native_driver_tests::one_limited_turn("worker-limit-is-not-lead-limit", r#"
printf '%s\n' '{"type":"result","parent_tool_use_id":"worker","is_error":true,"api_error_status":429}'
printf '%s\n' '{"type":"result","stop_reason":"end_turn"}'
"#).unwrap(), "end_turn");
}



#[test]
fn worker_triggered_turns_report_once_even_when_a_new_app_input_is_queued() {
    for queued in [false, true] {
        let root = std::env::temp_dir().join(format!("richos-worker-turn-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("provider");
        std::fs::write(&script, r#"#!/usr/bin/python3
import json,pathlib,sys
queued=json.loads(pathlib.Path('config.json').read_text())['queued']
def send(v):print(json.dumps(v),flush=True)
def read():
 line=sys.stdin.readline()
 if not line:raise SystemExit(0)
 v=json.loads(line)
 if v.get('type')=='user':
  with pathlib.Path('received.jsonl').open('a') as f:f.write(json.dumps(v)+'\n')
 return v
def phase(v,s):send({'type':'command_lifecycle','command_uuid':v['uuid'],'state':s})
init=read();send({'type':'control_response','response':{'subtype':'success','request_id':init['request_id'],'response':{}}})
first=read();phase(first,'started')
send({'type':'assistant','message':{'content':[{'type':'text','text':'ORIGINAL-STARTED'}]}})
if queued:
 live=read();phase(live,'queued')
send({'type':'result','stop_reason':'end_turn'})
send({'type':'system','subtype':'task_notification','task_id':'worker-one','status':'completed'})
send({'type':'result','parent_tool_use_id':'worker-one','is_error':True,'api_error_status':429})
send({'type':'result','stop_reason':'end_turn'})
send({'type':'stream_event','event':{'type':'content_block_delta','delta':{'type':'thinking_delta','thinking':'landing worker output'}}})
send({'type':'system','subtype':'api_retry','attempt':1,'error_status':503})
send({'type':'result','is_error':True,'errors':['API Error: 503']})
send({'type':'result','stop_reason':'max_tokens'})
send({'type':'result','stop_reason':'cancelled'}) # dialect-exempt: native stop_reason wire value
if queued:
 phase(live,'started');send({'type':'result','stop_reason':'end_turn'})
while True:read()
"#).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(root.join("config.json"), json!({"queued":queued}).to_string()).unwrap();
        let skills = crate::skills::ensure_rendered(&root.join("skills"),
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).unwrap();
        let mut ledger = crate::ledger::Ledger::open(root.join("ledger.jsonl")).unwrap();
        let thread = ledger.create_thread("Worker continuation", &EntityId::parse("company").unwrap()).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        let scope = StatusToolScope {version:1,state_root:root.join("state"),entity_id:"company".into(),thread_id:thread};
        let status = root.join("rich.json");
        crate::session_tools::bind(&status,&scope,"rich-session","rich",None).unwrap();
        let (began,started) = channel();
        let (done,completed) = channel();
        let (observed,observations) = channel();
        let host = WorkHost::new(&scope.state_root,Box::new(Factory {
            root:root.clone(),skills,started:began,completed:done,scheduled:Arc::new(AtomicU64::new(0)),observed:Some(observed),
        }),Arc::new(SilentNotifier));
        crate::session_tools::send(&status,&scope,json!({"text":"Original input"})).unwrap();
        host.wake_messages(&binding);
        // load-bound: hang guard only; native output proves the initial turn entered.
        started.recv_timeout(Duration::from_secs(10)).expect("initial turn absent");
        if queued {
            crate::session_tools::send(&status,&scope,json!({"text":"Queued correction"})).unwrap();
            host.wake_messages(&binding);
        }
        for _ in 0..4 {
            // load-bound: hang guard only; drained native results prove status observations have completed.
            observations.recv_timeout(Duration::from_secs(10)).expect("worker continuation result was not observed");
        }
        for _ in 0..(1+usize::from(queued)) {
            // load-bound: hang guard only; native outcomes and durable receipts, not elapsed time, are the verdict.
            completed.recv_timeout(Duration::from_secs(10)).expect("app input result absent");
        }
        assert!(host.wait_for_completed(1,Duration::from_secs(10)));
        let mut facts = Vec::new();
        while let Some((message,through)) = crate::session_tools::pending(&scope,"rich").unwrap() {
            assert_eq!(message.from,"work-status");
            let fact:Value = serde_json::from_str(&message.text).unwrap();
            assert_eq!(fact["event"],"upstream_turn");assert!(fact["input_id"].is_null());
            facts.push(fact);
            crate::session_tools::acknowledge(&scope,"rich",through).unwrap();
        }
        assert_eq!(facts.len(),2,"missing or duplicate terminal failure: {facts:?}");
        assert!(facts[0]["error"].as_str().unwrap().contains("503"));
        assert_eq!(facts[1]["stop_reason"],"max_tokens");
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none());
        host.shutdown();
        assert!(crate::session_tools::pending(&scope,"rich").unwrap().is_none(),"Quit produced an unwanted reply");
        let sent = std::fs::read_to_string(root.join("received.jsonl")).unwrap();
        assert_eq!(sent.lines().count(),1+usize::from(queued),"worker results caused a new app send");
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// The retry clock with no elapsed time: it writes down each wait it was asked for.
#[derive(Default)]
struct NoWait(Mutex<Vec<Duration>>);
impl crate::upstream::RetryClock for NoWait {
    fn wait(&self, wait: Duration, stop: &dyn Fn() -> bool) -> bool {
        self.0.lock().unwrap().push(wait);
        !stop()
    }
}

/// Follow-up 10 (2026-10-18): a live input whose turn carries Claude Code's overload message and
/// still ends `end_turn` is that overload. It waits the schedule's first minute and is sent
/// again on the same session, and Rich gets no failure. Prose that only quotes the line, on a
/// frame Claude Code did not mark as an API error, stays Stu's reply.
#[test]
fn a_live_overload_reported_only_as_a_message_waits_and_is_sent_again_on_the_same_session() {
    for marked in [true, false] {
        let root = std::env::temp_dir().join(format!("richos-live-overload-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("provider");
        std::fs::write(&script, r#"#!/usr/bin/python3
import json,pathlib,sys
marked=json.loads(pathlib.Path('config.json').read_text())['marked']
def send(v):print(json.dumps(v),flush=True)
def read():
 line=sys.stdin.readline()
 if not line:raise SystemExit(0)
 v=json.loads(line)
 if v.get('type')=='user':
  with pathlib.Path('received.jsonl').open('a') as f:f.write(json.dumps(v)+'\n')
 return v
def phase(v,s):send({'type':'command_lifecycle','command_uuid':v['uuid'],'state':s})
init=read();send({'type':'control_response','response':{'subtype':'success','request_id':init['request_id'],'response':{}}})
first=read();phase(first,'started')
send({'type':'assistant','message':{'content':[{'type':'text','text':'ORIGINAL-STARTED'}]}})
live=read();phase(live,'queued')
send({'type':'result','stop_reason':'end_turn'})
phase(live,'started')
said={'type':'assistant','parent_tool_use_id':None,'message':{'content':[{'type':'text','text':'API Error: 529 Overloaded.'}]}}
if marked:said['error']='overloaded'
send(said)
send({'type':'result','subtype':'success','is_error':False,'stop_reason':'end_turn'})
while True:
 v=read()
 if v.get('type')=='user':
  phase(v,'started');send({'type':'result','stop_reason':'end_turn'})
"#).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(root.join("config.json"), json!({"marked":marked}).to_string()).unwrap();
        let skills = crate::skills::ensure_rendered(&root.join("skills"),
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).unwrap();
        let mut ledger = crate::ledger::Ledger::open(root.join("ledger.jsonl")).unwrap();
        let thread = ledger.create_thread("Live overload", &EntityId::parse("company").unwrap()).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        let scope = StatusToolScope {version:1,state_root:root.join("state"),entity_id:"company".into(),thread_id:thread};
        let status = root.join("rich.json");
        crate::session_tools::bind(&status,&scope,"rich-session","rich",None).unwrap();
        let (began,started) = channel();
        let (done,completed) = channel();
        let host = WorkHost::new(&scope.state_root,Box::new(Factory {
            root:root.clone(),skills,started:began,completed:done,scheduled:Arc::new(AtomicU64::new(0)),observed:None,
        }),Arc::new(SilentNotifier));
        let clock = Arc::new(NoWait::default());
        host.set_retry_clock(clock.clone());
        crate::session_tools::send(&status,&scope,json!({"text":"Original input"})).unwrap();
        host.wake_messages(&binding);
        // load-bound: hang guard only; native output proves the original turn entered.
        started.recv_timeout(Duration::from_secs(10)).expect("original turn absent");
        crate::session_tools::send(&status,&scope,json!({"text":"Live correction"})).unwrap();
        host.wake_messages(&binding);
        for _ in 0..(2+usize::from(marked)) {
            // load-bound: hang guard only; each native outcome, not elapsed time, is the verdict.
            completed.recv_timeout(Duration::from_secs(10)).expect("a native result did not complete");
        }
        assert!(host.wait_for_completed(1,Duration::from_secs(10)));
        host.shutdown();
        let sent = std::fs::read_to_string(root.join("received.jsonl")).unwrap();
        assert_eq!(sent.lines().filter(|line| line.contains("Live correction")).count(), 1+usize::from(marked),
            "marked={marked}: the overload message was {} as Stu's reply: {sent}", if marked {"accepted"} else {"not accepted"});
        assert_eq!(*clock.0.lock().unwrap(), if marked {vec![crate::upstream::OVERLOAD_RETRY_WAITS[0]]} else {vec![]});
        assert!(crate::session_tools::pending(&scope,"rich").unwrap().is_none(), "marked={marked}: a recovered overload reached Rich as a failure");
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none(), "marked={marked}: the live input stayed unfinished");
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// The facts of every `worker_overload` notice the provider received, in order.
fn worker_notices(received: &str) -> Vec<Value> {
    received.lines().filter_map(|line| {
        let frame: Value = serde_json::from_str(line).ok()?;
        let content = &frame["message"]["content"];
        let text = content.as_str().map(str::to_string).unwrap_or_else(|| content.as_array().into_iter().flatten()
            .filter_map(|block| block["text"].as_str()).collect::<Vec<_>>().join(""));
        let message: Value = serde_json::from_str(&text).ok()?;
        (message["from"] == "overload-retry").then(|| serde_json::from_str(message["text"].as_str()?).ok()).flatten()
    }).collect()
}

/// Review rv-20261018T205954Z-3590e78c-d3fa: Claude Code 2.1.302 ends repeated overloads with
/// its own spelling, `API Error: Repeated 529 Overloaded errors.`, on an assistant frame marked
/// `server_error`. It is an overload, so the worker or turn it ended gets the CEO's waits.
#[test]
fn claude_codes_repeated_529_message_is_an_overload() {
    let text = "API Error: Repeated 529 Overloaded errors. The API is at capacity \u{2014} this is usually temporary. \
        Try again in a moment. If it persists, check https://status.claude.com.";
    let frame = serde_json::json!({"type":"assistant","parent_tool_use_id":"worker-call","error":"server_error",
        "message":{"content":[{"type":"text","text":text}]}});
    let failure = overloaded_api_error_message(&frame).expect("Claude Code's repeated-529 message was not taken for an overload");
    assert_eq!((failure.fault, failure.status, failure.raw.as_str()), (crate::upstream::UpstreamFault::Overloaded, Some(529), text));
}

/// Follow-up 11 (2026-10-18): a worker stopped by an overload gets the CEO's waits too. The
/// host waits the schedule's next wait and then tells Stu, through his own inbox, to restart it;
/// a worker's ordinary answer from the API starts the schedule again. The worker's message
/// arrives inside one of Stu's turns, so it reaches the host whichever turn carried it.
#[test]
fn a_workers_overload_waits_the_ceos_schedule_and_then_stu_is_told_to_restart_it() {
    let root = std::env::temp_dir().join(format!("richos-worker-overload-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("provider");
    std::fs::write(&script, r#"#!/usr/bin/python3
import json,pathlib,sys
def send(v):print(json.dumps(v),flush=True)
def read():
 line=sys.stdin.readline()
 if not line:raise SystemExit(0)
 v=json.loads(line)
 if v.get('type')=='user':
  with pathlib.Path('received.jsonl').open('a') as f:f.write(json.dumps(v)+'\n')
 return v
def phase(v,s):send({'type':'command_lifecycle','command_uuid':v['uuid'],'state':s})
def worker(content,**marked):send({'type':'assistant','parent_tool_use_id':'worker-call','agent_id':'fixture-worker','task_description':'Fixture worker','message':{'content':content},**marked})
init=read();send({'type':'control_response','response':{'subtype':'success','request_id':init['request_id'],'response':{}}})
told=0
while True:
 v=read()
 if v.get('type')!='user':continue
 phase(v,'started')
 if 'worker_overload' in json.dumps(v):told+=1
 if told==2:worker([{'type':'text','text':'Reading the files again.'}])
 if told<=2:worker([{'type':'text','text':'API Error: 529 Overloaded.'}],error='overloaded')
 send({'type':'result','stop_reason':'end_turn'})
"#).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let skills = crate::skills::ensure_rendered(&root.join("skills"),
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).unwrap();
    let mut ledger = crate::ledger::Ledger::open(root.join("ledger.jsonl")).unwrap();
    let thread = ledger.create_thread("Worker overload", &EntityId::parse("company").unwrap()).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();
    let scope = StatusToolScope {version:1,state_root:root.join("state"),entity_id:"company".into(),thread_id:thread};
    let status = root.join("rich.json");
    crate::session_tools::bind(&status,&scope,"rich-session","rich",None).unwrap();
    let (began,_started) = channel();
    let (done,completed) = channel();
    let host = WorkHost::new(&scope.state_root,Box::new(Factory {
        root:root.clone(),skills,started:began,completed:done,scheduled:Arc::new(AtomicU64::new(0)),observed:None,
    }),Arc::new(SilentNotifier));
    let clock = Arc::new(NoWait::default());
    host.set_retry_clock(clock.clone());
    crate::session_tools::send(&status,&scope,json!({"text":"Original input"})).unwrap();
    host.wake_messages(&binding);
    for _ in 0..4 {
        // load-bound: hang guard only; each of Stu's turns ending, not elapsed time, is the verdict.
        completed.recv_timeout(Duration::from_secs(10)).expect("Stu was not told about his worker's overload");
    }
    host.shutdown();
    let told = worker_notices(&std::fs::read_to_string(root.join("received.jsonl")).unwrap());
    assert_eq!(told.iter().map(|fact| (fact["retry"].clone(), fact["retries_left"].clone(), fact["waited_minutes"].clone())).collect::<Vec<_>>(),
        [(json!(1),json!(6),json!(1)), (json!(2),json!(5),json!(2)), (json!(1),json!(6),json!(1))], "{told:?}");
    assert!(told.iter().all(|fact| fact["event"] == "worker_overload" && fact["worker"]["tool_use_id"] == "worker-call"
        && fact["worker"]["task"] == "Fixture worker" && fact["error"] == "API Error: 529 Overloaded."), "{told:?}");
    let minute = Duration::from_secs(60);
    assert_eq!(*clock.0.lock().unwrap(), [minute, 2 * minute, minute], "the worker's waits were not the CEO's schedule");
    assert!(crate::session_tools::pending(&scope,"rich").unwrap().is_none(), "a worker's overload reached Rich as Stu's failure");
    assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none());
    std::fs::remove_dir_all(root).unwrap();
}

/// Review rv-20261018T200657Z-585c24df-f9d6, finding 3: Claude Code starts turns of Stu's own
/// (a helper's hand-back, a task notification) with no input of ours parked. One that carries
/// Claude Code's overload message and still ends `end_turn` waits the schedule's first minute,
/// and then Stu is woken through his own inbox to carry on; Rich gets no failure. Prose that
/// only quotes the line, on a frame Claude Code did not mark as an API error, is Stu's reply.
#[test]
fn an_automatic_stu_turn_reported_only_as_an_overload_message_waits_and_wakes_him_again() {
    for marked in [true, false] {
        let root = std::env::temp_dir().join(format!("richos-automatic-overload-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("provider");
        std::fs::write(&script, r#"#!/usr/bin/python3
import json,pathlib,sys
marked=json.loads(pathlib.Path('config.json').read_text())['marked']
def send(v):print(json.dumps(v),flush=True)
def read():
 line=sys.stdin.readline()
 if not line:raise SystemExit(0)
 v=json.loads(line)
 if v.get('type')=='user':
  with pathlib.Path('received.jsonl').open('a') as f:f.write(json.dumps(v)+'\n')
 return v
def phase(v,s):send({'type':'command_lifecycle','command_uuid':v['uuid'],'state':s})
init=read();send({'type':'control_response','response':{'subtype':'success','request_id':init['request_id'],'response':{}}})
first=read();phase(first,'started')
send({'type':'result','stop_reason':'end_turn'})
# Claude Code's own turn after a helper hands back: no input of ours is parked.
said={'type':'assistant','parent_tool_use_id':None,'message':{'content':[{'type':'text','text':'API Error: 529 Overloaded.'}]}}
if marked:said['error']='overloaded'
send(said)
send({'type':'result','subtype':'success','is_error':False,'stop_reason':'end_turn'})
while True:
 v=read()
 if v.get('type')=='user':
  phase(v,'started');send({'type':'result','stop_reason':'end_turn'})
"#).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(root.join("config.json"), json!({"marked":marked}).to_string()).unwrap();
        let skills = crate::skills::ensure_rendered(&root.join("skills"),
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).unwrap();
        let mut ledger = crate::ledger::Ledger::open(root.join("ledger.jsonl")).unwrap();
        let thread = ledger.create_thread("Automatic overload", &EntityId::parse("company").unwrap()).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        let scope = StatusToolScope {version:1,state_root:root.join("state"),entity_id:"company".into(),thread_id:thread};
        let status = root.join("rich.json");
        crate::session_tools::bind(&status,&scope,"rich-session","rich",None).unwrap();
        let (began,_started) = channel();
        let (done,completed) = channel();
        let (seen,observed) = channel();
        let host = WorkHost::new(&scope.state_root,Box::new(Factory {
            root:root.clone(),skills,started:began,completed:done,scheduled:Arc::new(AtomicU64::new(0)),observed:Some(seen),
        }),Arc::new(SilentNotifier));
        let clock = Arc::new(NoWait::default());
        host.set_retry_clock(clock.clone());
        crate::session_tools::send(&status,&scope,json!({"text":"Original input"})).unwrap();
        host.wake_messages(&binding);
        // load-bound: hang guard only; each native outcome, not elapsed time, is the verdict.
        completed.recv_timeout(Duration::from_secs(10)).expect("the original input did not complete");
        // load-bound: hang guard only; the automatic turn's result reaching the host is the verdict.
        observed.recv_timeout(Duration::from_secs(10)).expect("the automatic turn's result was not observed");
        if marked {
            // load-bound: hang guard only; the woken turn's own completion is the verdict.
            completed.recv_timeout(Duration::from_secs(10)).expect("Stu was not woken after his automatic turn's overload");
        }
        assert!(host.wait_for_completed(1,Duration::from_secs(10)));
        host.shutdown();
        let told: Vec<Value> = worker_notices(&std::fs::read_to_string(root.join("received.jsonl")).unwrap())
            .into_iter().filter(|fact| fact["event"] == "turn_overload").collect();
        assert_eq!(told.iter().map(|fact| (fact["waited_minutes"].clone(), fact["error"].clone())).collect::<Vec<_>>(),
            if marked {vec![(json!(1), json!("API Error: 529 Overloaded."))]} else {vec![]}, "marked={marked}: {told:?}");
        assert_eq!(*clock.0.lock().unwrap(), if marked {vec![crate::upstream::OVERLOAD_RETRY_WAITS[0]]} else {vec![]});
        assert!(crate::session_tools::pending(&scope,"rich").unwrap().is_none(), "marked={marked}: a recovered overload reached Rich as a failure");
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none(), "marked={marked}: Stu's wake-up stayed unfinished");
        std::fs::remove_dir_all(root).unwrap();
    }
}
}
