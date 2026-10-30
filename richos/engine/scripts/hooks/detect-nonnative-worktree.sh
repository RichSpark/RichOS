#!/usr/bin/env bash
#
# detect-nonnative-worktree.sh — PostToolUse (Agent) DETECTOR.
#
# It REGISTERS nothing and DELETES nothing. Registration of the spawn's agent
# id and native workspace is workspace-lifecycle.sh's job on this same event
# (docs/plans/worktree-spec-2026-09-11.md, points 3 and 6); deletion is the
# page's land and discard, and nothing else.
#
# SECOND JOB — the spawned-names ledger APPEND. This hook is also the ONLY
# place a spawned teammate's name is written to the session-scoped
# spawned-names.log that guard-worktree-isolation.sh's name-reuse clause reads.
# The write used to live inline in that PreToolUse guard, which runs first in a
# four-hook chain, so a spawn it approved and a later hook vetoed burned its
# name for a teammate that never existed. PostToolUse fires only for a call
# that actually ran, which makes "recorded" and "created" the same event. Full
# rationale: "BLOCKED-SPAWN NAME BURN" in guard-worktree-isolation.sh. The
# CHECK stays there; only the WRITE lives here. See the block near the end.
#
# The companion to guard-worktree-isolation.sh (the PreToolUse PREVENTER).
# This is the DETECTOR: it answers "how do I know I fucked up if all guards
# fail?" It runs AFTER an Agent spawn and loudly surfaces the tell —
# independent of the preventer, so if the preventer is unloaded/disabled the
# failure still gets caught. Non-blocking (the spawn already ran): it exits 0
# and pushes the warning into the model's context as additionalContext. It used
# to exit 2 for that, which made the host announce a NON-BLOCKING detector's
# report as a "blocking error" over a spawn that had already succeeded — see
# "THE CHANNEL THIS DETECTOR REPORTS ON" below for the measurement.
#
# It flags two things:
#   (a) THIS spawn was a file-capable agent WITHOUT native isolation AND
#       WITHOUT the "main-checkout-run:" marker — read straight from the same
#       tool_input. Deterministic and immediate; catches the exact "forgot
#       isolation:worktree" failure even with no preventer.
#   (b) ANY non-native worktree currently exists under .claude/worktrees/ —
#       i.e. a directory NOT named agent-<hex> (native isolation ALWAYS names
#       them agent-<hex> on a worktree-<hex> branch). A human-readable slug
#       like "design-v16" is the guard-free signature of a hand-rolled
#       worktree. This also nags until a lingering botched worktree is landed +
#       removed. (Deliberate hand-rolls trip it too, by design — acknowledge
#       and move on.)
#
#       TWO DIFFERENT CAUSES, and (b) does NOT tell you which — check before
#       you go hunting for a bug that isn't there:
#         1. The LEAD omitted isolation:"worktree" on the spawn. The classic.
#         2. The SUBAGENT hand-rolled a worktree ITSELF — ran `git worktree
#            add` from inside its own, perfectly correct, native worktree. Fix
#            is the same (remove the stray), but do not waste time auditing
#            spawn flags that were never wrong — check `git worktree list` for a
#            NATIVE worktree belonging to the same agent sitting right
#            alongside the stray. If it's there, cause 2 is what happened.
#
# The "main-checkout-run:" marker (see guard-worktree-isolation.sh) sanctions a
# deliberate un-isolated run. This detector RESPECTS the marker for check (a):
# do NOT warn on a launch that declared it. Check (b) — lingering hand-rolled
# worktrees on disk — is unaffected by the marker; it is about the FILESYSTEM's
# current state, not this specific launch.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail
# Payloads can exceed a pipe buffer. Matching pipelines must consume EOF:
# grep -q or head can give the producer SIGPIPE and reverse the verdict.

# Fail-closed, not fail-open: check (a) below depends on python3 to parse the
# spawn's tool_input. If python3 is missing, the swallowed `|| true` failure
# yields empty SUBAGENT_TYPE/ISOLATION, which reads as "nothing to warn about"
# — i.e. this detector backstop would go silently blind at the exact moment
# the preventer (guard-worktree-isolation.sh) is ALSO blind for the same
# reason. Refuse loudly instead of degrading silently.
command -v python3 >/dev/null 2>&1 || { echo "ERROR: detect-nonnative-worktree.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

# --- THE CHANNEL THIS DETECTOR REPORTS ON ----------------------------------
# It reports through `hookSpecificOutput.additionalContext` at exit 0, NOT
# through stderr at exit 2. What it DETECTS is unchanged; only the envelope is.
#
# MEASURED, NOT ASSUMED — Claude Code 2.1.270, macOS, 2026-09-14. Three
# PostToolUse[Write] hooks in one headless session, each emitting a unique
# marker on a different channel, read back out of the session transcript:
#
#   channel                        reaches the author?   how the host frames it
#   ----------------------------   -------------------   ----------------------
#   stderr, exit 2                 yes                   attachment type
#                                                        `hook_blocking_error`:
#                                                        "PostToolUse:Write hook
#                                                        BLOCKING ERROR from
#                                                        command: ..."
#   additionalContext, exit 0      yes                   attachment type
#                                                        `hook_additional_context`:
#                                                        "PostToolUse:Write hook
#                                                        additional context: ..."
#                                                        — the text, nothing else
#   stderr, exit 0                 NO                    `hook_success` with the
#                                                        text in its stderr field
#                                                        and NO rendering at all
#
# THE REASON THE FIRST ROW IS WRONG FOR THIS HOOK, and it is stronger than a
# preference. On PostToolUse, exit 2 CANNOT REFUSE ANYTHING — the tool has
# already run. In the same measurement the Write completed, the file was on
# disk, the tool result read "File created successfully", and the call was not
# retried. So the host's "blocking error" banner over a PostToolUse hook is
# describing an event that never happens on this event type. This hook's own
# header has said "Non-blocking (the spawn already ran)" since it was written,
# and it then announced itself to the reader as a blocking error — a mechanism
# arguing with its own first line, where the reader believes the banner.
#
# THE THIRD ROW IS WHY THE FIX IS NOT "just exit 0". Dropping to stderr at
# exit 0 would have removed the false banner by silencing the hook completely.
# The same table is reached independently in scripts/lib/stop-hook-notice.sh
# and relied on in notice-protected-ref-moves.sh.
#
# python3 is guaranteed present here by the fail-closed check directly above,
# so the JSON encoding below cannot be the thing that fails.
_NOTICE_BUF="$(mktemp -t detect-nonnative-notice.XXXXXX)"
trap 'rm -f "$_NOTICE_BUF"' EXIT

# emit_notice_and_exit — the SINGLE exit path for everything this hook has to
# say. Always exit 0: nothing here is a refusal.
emit_notice_and_exit() {
  if [ -s "$_NOTICE_BUF" ]; then
    NOTICE_TEXT="$(cat "$_NOTICE_BUF")" python3 -c 'import json, os, sys
sys.stdout.write(json.dumps({"hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "additionalContext": os.environ.get("NOTICE_TEXT", ""),
}}) + "\n")'
  fi
  exit 0
}

# ONE SCREAM BELOW IS DELIBERATELY LEFT ON THE CHANNEL THAT REACHES NOBODY, and
# it is left that way because fixing it HERE would be the wrong repair. The
# bootstrap block that follows is byte-identical in every rooted hook and Layer
# R of contract-integrity-probe.sh asserts exactly that; routing this one copy
# through the notice buffer diverged it and failed the layer, which is the
# layer working. So the "BROKEN INSTALL" banner still goes to stderr at exit 0
# — measured 2026-09-14 to be recorded in the transcript and rendered to no
# one. That is not this hook's defect to fix: it is one shared bootstrap that
# every rooted hook carries, so it is one change across all of them or none,
# and that is a larger piece of work than a channel correction.
# --- ROOT RESOLUTION -------------------------------------------------------
# TWO ROOTS, NEVER ONE. The full contract, and why the old single-root
# resolution was wrong the moment the engine became loadable by reference,
# is in scripts/lib/resolve-roots.sh. This bootstrap block is byte-identical
# in every hook that needs a root; contract-integrity-probe.sh Layer R asserts
# that, so a divergent copy is a probe failure rather than a surprise.
#
# THE BANNER BELOW GOES OUT ON TWO CHANNELS AND ONLY THE SECOND IS HEARD.
# Measured on Claude Code 2.1.270 (macOS, 2026-09-14) by registering one probe
# hook per channel on five events at once and reading the transcript back:
#
#   channel, exit 0     SessionStart  UserPromptSubmit  PreToolUse  PostToolUse  Stop
#   stderr              silent        silent            silent      silent       silent
#   stdout, plain text  model         model             silent      silent       silent
#   {"systemMessage"}   PERSON        PERSON            PERSON      PERSON       PERSON
#   additionalContext   model         model             model       model        model
#
# `silent` is not shorthand: the host records a `hook_success` attachment
# carrying the text in its stderr field and renders it to NO ONE. So a hook
# that could not find its own engine announced a dead enforcement layer
# exactly as loudly as a clean pass. Of the 60 files carrying this block, 35
# exit 2 here — where the host does render stderr, as the refusal reason — and
# 24 exit 0 and were inaudible. The 24 are the notices and observers, which is
# the trap: the hooks that must never block are the hooks nobody could hear.
#
# WHY `systemMessage` AND NOT `additionalContext`. additionalContext must name
# its own event in the envelope, and this block is identical in hooks
# registered on eight different events — it cannot know which one it is on.
# `systemMessage` is event-agnostic and it reaches the operator rather than
# only the model, which is the right audience for "your guards are off".
# Measured too: adding it to an exit-2 hook leaves the refusal untouched —
# same `hook error:` tool result, same blocked write — and only adds a render.
# Nothing here changes what any hook detects, refuses, or exits with.
#
# The escaping is deliberately pure bash (verified on 3.2.57, the macOS system
# shell) and calls nothing external: this is the one code path in the engine
# that runs when the install is already known to be broken, so it must not
# depend on python3, jq, or any file it has just failed to find.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_RR_LIB="$SCRIPT_DIR/../lib/resolve-roots.sh"
if [ ! -f "$_RR_LIB" ]; then
    _RR_MSG="=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ===
  hook: scripts/hooks/detect-nonnative-worktree.sh
  scripts/lib/resolve-roots.sh is missing at: $_RR_LIB
  Without it this guard cannot tell WHICH REPOSITORY it governs.
  It will not guess, and it will not carry on quietly — a defense
  that reports 'on' while protecting nothing is worse than none."
    printf '%s\n' "$_RR_MSG" >&2
    _RR_J="${_RR_MSG//\\/\\\\}"; _RR_J="${_RR_J//\"/\\\"}"; _RR_J="${_RR_J//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_RR_J"
    exit 0
fi
# shellcheck source=../lib/resolve-roots.sh
. "$_RR_LIB"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"

INPUT="$(cat)"

# Resolve the governed repository. A log-only hook must never block a session,
# so a BROKEN root SCREAMS instead of exiting non-zero — and it screams into
# both stderr and the hook's own JSON output, so it cannot pass for a skip.
# Until 2026-09-14 that second half was a promise the code did not keep: there
# was no JSON output, and stderr alone at exit 0 is invisible to every reader.
ROOT_FAILURE=""
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    ENTITY_ROOT=""
else
    ENTITY_ROOT=""
    ROOT_FAILURE="$(root_failure_banner "scripts/hooks/detect-nonnative-worktree.sh")"
    printf '%s\n' "$ROOT_FAILURE" | tee -a "$_NOTICE_BUF" >&2
fi

# Nothing to detect against in a repository that never adopted the engine, and
# nothing this backstop can honestly say when its root is broken — but it has
# already SCREAMED in the broken case above, which is the difference between
# this and the silent skip it replaces.
#
# THE SCREAM USED TO BE INAUDIBLE, which is why it now goes through the buffer
# as well as stderr. The comment above promised it reached "both stderr and the
# hook's own JSON output"; there was no JSON output, and stderr at exit 0 is the
# one channel measured to reach NOBODY — recorded in the transcript as a
# `hook_success` attachment with no rendering. A broken install therefore
# announced itself exactly as loudly as a clean pass. Adopted repositories that
# simply never adopted the engine stay silent, as before: the buffer is empty
# for them, and emit_notice_and_exit says nothing when it has nothing.
[ -n "$ENTITY_ROOT" ] || emit_notice_and_exit

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
: "${READONLY_ALLOWLIST:=Explore Plan claude-code-guide statusline-setup}"
: "${SESSION_TEAMS_DIR:=$HOME/.claude/teams}"

# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# The line below is the textbook case the 2026-09-05 survey named: the
# extraction ends in `|| true`, so an unreadable payload yields an empty
# TOOL_NAME and takes the SAME exit 0 that an honest "not an Agent call" takes.
# For THIS hook that silence is expensive out of proportion to its exit code: it
# is the BINDER, and an unbound file-capable spawn is refused every write later
# by guard-sealed-worktree.sh — so the operator would meet the barrier's refusal
# with no record of the moment the binding was skipped. PostToolUse cannot undo
# the spawn, so the verdict is unchanged and correct; only the silence changes.
# scripts/lib/unevaluated-notice.sh carries the measurement and the argument.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "detect-nonnative-worktree.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether this spawn is isolated, and whether its worktree could be bound to it"
fi

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
[ "$TOOL_NAME" = "Agent" ] || exit 0

PARSED="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
    ti = d.get("tool_input", {}) or {}
    st = str(ti.get("subagent_type","") or "")
    iso = str(ti.get("isolation","") or "")
    nm = str(ti.get("name","") or "")
    sid = str(d.get("session_id","") or "")
    cwd = str(ti.get("cwd","") or "")
    pr = str(ti.get("prompt","") or "")
    pr = pr.replace("\t", " ").replace("\n", "\x01")
    print("%s\t%s\t%s\t%s\t%s\t%s" % (st, iso, nm, sid, cwd, pr))
except Exception:
    print("\t\t\t\t\t")
' 2>/dev/null || printf '\t\t\t\t\t')"
SUBAGENT_TYPE="$(printf '%s' "$PARSED" | cut -f1)"
ISOLATION="$(printf '%s' "$PARSED" | cut -f2)"
NAME="$(printf '%s' "$PARSED" | cut -f3)"
SESSION_ID="$(printf '%s' "$PARSED" | cut -f4)"
SPAWN_CWD="$(printf '%s' "$PARSED" | cut -f5)"
PROMPT="$(printf '%s' "$PARSED" | cut -f6- | tr '\001' '\n')"

# Respect the same "main-checkout-run:" marker guard-worktree-isolation.sh
# honors — a sanctioned un-isolated run must not be re-flagged here.
MAIN_CHECKOUT_MARKER=""
if printf '%s' "$PROMPT" | grep -E >/dev/null '^[[:space:]]*main-checkout-run:[[:space:]]*.+'; then
  MAIN_CHECKOUT_MARKER="yes"
fi

WARN=()

# (a) This spawn: file-capable agent without native isolation, and without the
# main-checkout-run marker.
is_readonly=0
for a in $READONLY_ALLOWLIST; do
  [ "$SUBAGENT_TYPE" = "$a" ] && is_readonly=1
done
# App dispatch already verifies its registered linked target before this event.
APP_TARGET=""
if [ -n "${RICHOS_APP_SCOPE:-}" ] && printf '%s' "$PROMPT" | grep -E >/dev/null '^[[:space:]]*cross-repo-worktree:[[:space:]]*/'; then
  APP_TARGET="yes"
fi
if [ "$is_readonly" -eq 0 ] && [ "$ISOLATION" != "worktree" ] && [ "$ISOLATION" != "remote" ] && [ -z "$MAIN_CHECKOUT_MARKER" ] && [ -z "$APP_TARGET" ]; then
  WARN+=("this spawn — agent '${SUBAGENT_TYPE:-<unset/general-purpose>}' is file-capable but was spawned WITHOUT native isolation (isolation='${ISOLATION:-unset}') and carries no 'main-checkout-run:' marker. The preventive guard (guard-worktree-isolation.sh) should have blocked this; if you are seeing this, it did not. Shut this teammate down and re-spawn with isolation:\"worktree\" — or, if it genuinely must run un-isolated in the main checkout, add a 'main-checkout-run: <reason>' prompt line.")
fi

# --- SECOND JOB — the spawned-names ledger APPEND -------------------------
#
# This is the ONLY place a spawned teammate name is written to the
# session-scoped spawned-names.log that guard-worktree-isolation.sh's
# name-reuse clause CHECKS. The write used to live inline in that guard, but it
# runs FIRST in a four-hook PreToolUse[Agent] chain and any later hook can veto
# the same call — burning the name for a spawn that never executed. See
# "BLOCKED-SPAWN NAME BURN" in guard-worktree-isolation.sh.
#
# PostToolUse[Agent] fires ONLY for a tool call that actually ran, so a name is
# recorded if and only if a teammate was truly created. No re-validation is
# needed or wanted here: any non-read-only NAME reaching this point already
# passed the FULL spawn contract upstream. Read-only types (never required a
# name, never tracked) are skipped, matching the preventer's own early exit.
#
# Same directory-resolution rule as the preventer: EXACT session match only,
# overridable for tests via GUARD_ISOLATION_TEAMS_DIR; inert when no session
# team dir resolves. Best-effort — never fail this detector, and never block an
# already-executed spawn, because an append failed.
if [ "$is_readonly" -eq 0 ] && [ -n "$NAME" ] && [ -n "$SESSION_ID" ]; then
  GI_TEAMS_DIR="${GUARD_ISOLATION_TEAMS_DIR:-$SESSION_TEAMS_DIR}"
  GI_TEAM_DIR="$GI_TEAMS_DIR/session-$(printf '%s' "$SESSION_ID" | cut -c1-8)"
  mkdir -p "$GI_TEAM_DIR" 2>/dev/null || true
  printf '%s\n' "$NAME" >>"$GI_TEAM_DIR/spawned-names.log" 2>/dev/null || true
fi

# --- BINDING IS NOT THIS HOOK'S JOB ANY MORE --------------------------------
# The spawn's agent id and the native workspace the platform created for it
# are registered by workspace-lifecycle.sh on this same PostToolUse[Agent]
# event (mega-lander/workspaces.py bind_agent; docs/plans/
# worktree-spec-2026-09-11.md, points 3 and 6). This hook detects and records
# the spawned name; it registers and deletes nothing.


# (b) Any non-native worktree on disk (name != agent-<hex>).
while IFS= read -r wt; do
  [ -n "$wt" ] || continue
  base="$(basename "$wt")"
  case "$base" in
    agent-*) : ;;                       # native isolation worktree — fine
    *) WARN+=("lingering NON-NATIVE worktree: .claude/worktrees/${base} — native isolation always names worktrees 'agent-<hex>', so a readable slug means someone hand-rolled it, or a session was started with claude --worktree (not allowed: docs/plans/worktree-spec-2026-09-11.md, point 3). Check 'git worktree list': if an agent-<hex> worktree for the same agent sits alongside this stray, the SUBAGENT ran 'git worktree add' itself. It has no registration, so it is finished work: land it or discard it with workspaces.sh (point 3).") ;;
  esac
done < <(git -C "$ENTITY_ROOT" worktree list --porcelain 2>/dev/null | sed -n 's|^worktree ||p' | grep "/.claude/worktrees/" || true)

# --- new tells: ZOMBIE RESIDUE (dirs + processes) -------------------------
#
# A background child (a detached long-running verification — install-fresh, a
# build, a device sync) can outlive BOTH its agent AND its worktree, and
# re-create the directory on its way to a state write. Tell (b) above cannot
# catch a re-created agent-<hex> dir (its NAME is native-shaped); these two
# tells close that gap. Both REPORT; neither deletes nor kills anything. The
# only things that delete a workspace are the page's land and discard.
REGISTERED_WT="$(git -C "$ENTITY_ROOT" worktree list --porcelain 2>/dev/null | sed -n 's|^worktree ||p' || true)"
MAIN_CO="$(printf '%s\n' "$REGISTERED_WT" | sed -n '1p')"

PRESERVED_RESIDUE=()
ZOMBIE_PROCS=()

# (c) UNREGISTERED DIRECTORIES under <main>/.claude/worktrees/ — present on
# disk, ABSENT from the git registry. Reported, never removed.
if [ -n "$MAIN_CO" ] && [ -d "$MAIN_CO/.claude/worktrees" ]; then
  for d in "$MAIN_CO/.claude/worktrees"/*/; do
    [ -d "$d" ] || continue
    dir="$( cd "${d%/}" 2>/dev/null && pwd -P )" || continue
    case "$(basename "$dir")" in
      .*) continue ;;
    esac
    if printf '%s\n' "$REGISTERED_WT" | grep -xF >/dev/null "$dir"; then
      continue
    fi
    PRESERVED_RESIDUE+=("$dir")
  done
fi
# (d) ORPHANED PROCESSES referencing an UNREGISTERED worktree path. A background
# child can keep running after its agent and worktree are gone. REPORT-ONLY —
# never auto-kill (it may be mid-write). Scoped to THIS repo's worktrees so a
# sibling repo's live agent is never falsely flagged.
if command -v pgrep >/dev/null 2>&1 && command -v ps >/dev/null 2>&1 && [ -n "$MAIN_CO" ]; then
  while IFS= read -r pid; do
    [ -n "$pid" ] || continue
    [ "$pid" = "$$" ] && continue
    cmd="$(ps -o command= -p "$pid" 2>/dev/null || true)"
    [ -n "$cmd" ] || continue
    while IFS= read -r wtpath; do
      [ -n "$wtpath" ] || continue
      case "$wtpath" in
        "$MAIN_CO"/.claude/worktrees/agent-*) : ;;
        *) continue ;;
      esac
      if printf '%s\n' "$REGISTERED_WT" | grep -xF >/dev/null "$wtpath"; then
        continue
      fi
      ZOMBIE_PROCS+=("pid ${pid} -> ${wtpath}")
    done < <(printf '%s' "$cmd" | grep -oE '/[^[:space:]]*/\.claude/worktrees/agent-[A-Za-z0-9_]+' | sort -u || true)
  done < <(pgrep -f '/\.claude/worktrees/agent-' 2>/dev/null || true)
fi

if [ "${#WARN[@]}" -gt 0 ]; then
  {
    echo "============================================================"
    echo "  WORKTREE FUCK-UP DETECTED — WRONG WORKTREE DIRECTORY NAME"
    echo "============================================================"
    echo ""
    echo "The tell(s):"
    for w in "${WARN[@]}"; do
      echo "  ! $w"
    done
    echo ""
    echo "MANDATORY RESPONSE — do this NOW, before any other work:"
    echo "  1. STOP. Do not continue orchestrating."
    echo "  2. TERMINATE the mis-spawned teammate (TaskStop); its run has then ended."
    echo "  3. CORRECT THE MISTAKE: land or discard its work with workspaces.sh — the only"
    echo "     thing that deletes a workspace (docs/plans/worktree-spec-2026-09-11.md)."
    echo "     Re-spawn correctly with isolation:\"worktree\" if the work still needs doing"
    echo "     — or add 'main-checkout-run: <reason>' if it belongs in the main checkout."
    echo ""
    echo "(hook: scripts/hooks/detect-nonnative-worktree.sh — detective; this is the"
    echo " guard-free backstop for when the PreToolUse preventer fails.)"
  } >>"$_NOTICE_BUF"
fi

if [ "${#PRESERVED_RESIDUE[@]}" -gt 0 ] || [ "${#ZOMBIE_PROCS[@]}" -gt 0 ]; then
  {
    echo "============================================================"
    echo "  ZOMBIE WORKTREE RESIDUE — orphaned dir/process detected"
    echo "============================================================"
    echo ""
    if [ "${#PRESERVED_RESIDUE[@]}" -gt 0 ]; then
      echo "PRESERVED directories under .claude/worktrees/ that git does not list as"
      echo "worktrees. Nothing was removed; no workspace is deleted except by a land or"
      echo "a discard:"
      for r in "${PRESERVED_RESIDUE[@]}"; do
        echo "  - preserved $r"
      done
      echo ""
    fi
    if [ "${#ZOMBIE_PROCS[@]}" -gt 0 ]; then
      echo "ORPHANED PROCESS(es) still referencing an unregistered worktree path"
      echo "- these outlived their agent AND its worktree. NOT auto-killed; verify"
      echo "and kill manually:"
      for z in "${ZOMBIE_PROCS[@]}"; do
        echo "  ! $z"
      done
      pids="$(printf '%s\n' "${ZOMBIE_PROCS[@]}" | sed -n 's/^pid \([0-9][0-9]*\) .*/\1/p' | sort -u | tr '\n' ' ')"
      echo ""
      echo "  Recommended (after confirming these are stray): kill ${pids}"
      echo ""
    fi
    echo "(hook: scripts/hooks/detect-nonnative-worktree.sh)"
  } >>"$_NOTICE_BUF"
fi

# One exit path, always 0. The findings above travel as additionalContext; see
# "THE CHANNEL THIS DETECTOR REPORTS ON" near the top for the measurement that
# chose it. A finding is still LOUD — it is simply no longer announced to its
# reader as a failure of the call that produced it.
emit_notice_and_exit
