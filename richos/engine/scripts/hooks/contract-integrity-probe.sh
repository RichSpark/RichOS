#!/usr/bin/env bash
#
# contract-integrity-probe.sh — verify the hook enforcement machinery is
# actually wired AND functional on this machine.
#
# Rationale: the enforcement hooks are registered in EXACTLY ONE committed
# file, `.claude/settings.local.json`. A fresh clone that lost that file, an
# accidental delete, or an edit-out during troubleshooting all leave the gates
# silently disabled with no warning. This probe catches that.
#
# CANONICAL SOURCE (single-registration root-fix): hooks live ONLY in the
# committed `.claude/settings.local.json`. This probe reads the wiring from THAT
# file, and Layer M asserts the wiring is not ALSO duplicated into the generated
# `.claude/settings.json` — which Claude Code reads too and MERGES additively,
# firing every duplicated hook TWICE per matching tool event (the invisible
# defect this layer makes visible). See scripts/hooks/install.sh for the
# migration that removes any stale hook-duplicating settings.json.
#
# Run it standalone whenever you need to confirm enforcement is live:
#
#   scripts/hooks/contract-integrity-probe.sh
#
# Exit codes:
#   0  every layer passes
#   1  unexpected error
#   2  one or more layers failed; diagnostic on stderr
#
# Layers checked (in order):
#   A. .claude/settings.local.json (the canonical committed source) exists.
#   B. settings.local.json wires PreToolUse[Write|Edit|MultiEdit|NotebookEdit]
#      -> guard-main-checkout-writes.sh (path-confined, manifest-matched).
#   C. settings.local.json wires PreToolUse[Agent] -> the structural chain
#      prefix, IN ORDER: guard-worktree-isolation.sh, guard-definition-drift.sh,
#      verify-agent-prompt.sh (each path-confined, manifest-matched). The tail
#      of the chain is deliberately unconstrained — see
#      scripts/lib/registered-hooks.sh, AGENT_CHAIN_STRUCTURAL_PREFIX.
#   D. The wired write-guard hook rejects a known-bad main-checkout source
#      write (functional canary, targeting the first PROTECTED_PATHS tree).
#   E. Every wired Agent hook chain script exists, is executable.
#   F. TeammateIdle log hook (warn-only).
#   G. TaskCompleted log hook (warn-only).
#   H. PostToolUse[Agent] detect-nonnative-worktree.sh detector (warn-only).
#   I. .claude/settings.local.json carries env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS
#      == "1" (HARD gate — see the "ghost team" incident below).
#   J. .claude/settings.local.json carries worktree.baseRef == "head" (HARD gate).
#   L. PreToolUse[SendMessage] guard-resume-isolation.sh resume-guard (warn-only)
#      — wired + path-confined + manifest-matched + a functional canary.
#   K. settings.local.json wires PreToolUse[Write|Edit|MultiEdit|NotebookEdit] ->
#      scan-secrets.sh (path-confined, manifest-matched) AND the wired scanner
#      actually rejects a known-bad high-entropy secret (functional canary).
#   M. REGISTRATION UNIQUENESS: each canonical hook is registered EXACTLY ONCE
#      across settings.local.json + settings.json (no additive double-fire),
#      plus a live single-fire canary on the resume-guard append path.
#   N. GIT-TRACKED canonical source: .claude/settings.local.json is actually
#      tracked in this repo (git ls-files) — HARD-fails when the file is present
#      on disk but untracked AND matched by a gitignore rule (the silent
#      global-gitignore stranding trap below); warns when simply not-yet-
#      committed; skips cleanly outside a git work tree.
#   O. BASH-WRITE GUARD: settings.local.json wires PreToolUse[Bash] ->
#      guard-bash-main-writes.sh (path-confined, manifest-matched), and it denies
#      a raw main-checkout source write (functional canary against the first
#      PROTECTED_PATHS tree) — closing the cwd-default drift vector the Write/Edit
#      guard never sees. Warn-only (not fail) when PROTECTED_PATHS is empty.
#   IP. INTERACTIVE-PROMPT GUARD: settings.local.json wires PreToolUse[Bash] ->
#      guard-interactive-prompt.sh (path-confined, manifest-matched) with its
#      shape table scripts/lib/interactive-prompt.py present and hashed, and the
#      wired guard both REFUSES `security import` with no -P — the command that
#      put a password window on the CEO's screen at 02:01 on 2026-09-01 — and
#      PASSES the same command carrying the -P the refusal names. Two-sided,
#      because this guard exits 2 to refuse and 2 when it cannot start.
#   P. DEFINITION-DRIFT GUARD PAIR: SessionStart wires
#      snapshot-agent-definitions.sh and PreToolUse[Agent] wires
#      guard-definition-drift.sh, EACH EXACTLY ONCE, both present + executable
#      with current .sha256 sidecars, and the guard both BLOCKS a definition
#      modified since the session-start snapshot and ALLOWS an unchanged one
#      (paired functional canaries — a negative test alone can pass for the
#      wrong reason).
#   Q. THE WORKSPACE SPEC (docs/plans/worktree-spec-2026-09-11.md):
#      workspace-lifecycle.sh wired exactly once on each of its six events, the
#      Stop gate exactly once, every retired deleter wired nowhere, the registry
#      and its hooks manifest-matched, and a sandboxed spawn -> commit -> finish
#      that is locked out, holds the turn, and once merged is landed with its
#      workspace and branch gone (real worktrees are never touched).
#   R. THE ROOT-RESOLUTION CONTRACT is present, hashed, sourced by every rooted
#      hook with a byte-identical bootstrap, and engine-status.sh is registered
#      on both registration surfaces. Runs in BOTH modes.
#   AL. THE AGENT-LIVENESS RESOLVER (scripts/lib/agent-liveness.{py,sh} and the
#      operator CLI) is present, hashed, and BEHAVING: a worktree locked by a
#      running pid resolves ALIVE, an absent one NOT-ALIVE, and an unqueryable
#      repository INDETERMINATE — the third verdict asserted because collapsing
#      it is the confusion the 2026-08-31 incident was made of. Three callers
#      take their whole answer from that file, one of which DELETES worktrees.
#      Runs in BOTH modes.
#   EP. THE STANDING INSTRUCTION NAMES THE CURRENT COMMAND: supersession is
#      declared once as data (orchestration.config ENTRYPOINTS), the parser and
#      the currency lint are present, and no file declared in
#      INSTRUCTION_SURFACES names a superseded entrypoint without naming what
#      replaced it — plus a two-sided sandbox canary (refuses a known-bad
#      instruction naming file:line, ALLOWS the corrected one). Exists because a
#      capability can ship, work, be tested and go green while the announcement
#      every session boots with keeps routing traffic to the thing it replaced,
#      so the new path is simply never taken and nothing anywhere goes red.
#      Implemented in scripts/hooks/contract-integrity-layer-ep.sh, which also
#      runs standalone.
#
# BY-REFERENCE MODE runs a different set entirely (BR1-BR10 + R + AL), because the
# guards are then registered in the plugin's hooks/hooks.json and this
# repository's settings file legitimately never mentions them. See the
# "BY-REFERENCE LAYER SET" banner below for what each BR layer asserts.
#
# Layer N exists because of a real onboarding trap: .claude/settings.local.json
# is committed BY DESIGN (it is the SOLE hook-registration source plus the two
# load-bearing config keys), but a common global-gitignore convention
# (~/.config/git/ignore carrying `**/.claude/settings.local.json`, the way
# vanilla Claude Code treats it as machine-local) makes `git add -A` SILENTLY
# skip it. The file still exists on disk, so Layers A/I/J and the whole probe
# pass locally — yet the NEXT clone/session never receives it and the
# orchestrator wakes with ZERO teammates and no error (the Layer I incident,
# arriving by a different door). `git ls-files` is the ground truth: tracked ==
# it reaches the next cloner. Fix is `git add -f .claude/settings.local.json`.
#
# Layers I/J exist because of a real incident: `env.CLAUDE_CODE_EXPERIMENTAL_
# AGENT_TEAMS` was once accidentally deleted from settings.local.json during an
# edit, and at the NEXT session start the orchestrator could not see or spawn
# ANY teammates — total silent failure, no error shown anywhere. `worktree.
# baseRef: "head"` is the twin load-bearing key the worktree-isolation doctrine
# (CLAUDE.md.template, skills/using-git-worktrees, skills/rich-lander) assumes
# is set — without it, native worktrees stop branching from local HEAD and the
# land sequence's merge-base assumptions break. Both are HARD failures (not
# warn-only like F/G/H) precisely because their absence is otherwise invisible
# until the next session/land, at which point the failure mode is silent.
#
# Layer K is a HARD gate (not warn-only) for the same reason as B/C: a missing
# or gutted secrets scanner is a genuine security regression, not a cosmetic
# drift — an adopter relying on "the engine blocks leaked credentials" deserves a
# probe that actually verifies it's still true. It is checked independently of
# Layer B: the Write|Edit matcher now wires TWO hooks (guard-main-checkout-
# writes.sh first, scan-secrets.sh second), and Layer K scans the FULL list of
# wired commands under that matcher (not just the first) so its position
# doesn't matter, unlike Layer B's single-capture logic which specifically
# targets the first entry. Its canary is TWO-SIDED — planted secret refused AND
# clean content passed — because the scanner also refuses to START by exiting 2,
# and for months this layer could not tell the two apart. See the canary itself.
#
# Idempotent. Safe to run from any cwd.

set -eo pipefail
# Piped predicates must drain their input: grep -q can trigger SIGPIPE in
# the producer and turn an existing match into a failure under pipefail.

# Fail-closed, not fail-open: this probe leans on python3 for JSON extraction
# (Layers B/C/F/G/H) AND for realpath resolution (realpath_of(), which is the
# anti-shim symlink-confinement check). Some of those calls are `|| true`-
# guarded and degrade to an empty/unresolved value rather than aborting —
# empty extraction happens to still fail the "not wired" checks downstream,
# but realpath_of()'s silent fallback to the raw (non-symlink-resolved) path
# would quietly WEAKEN the adversarial path-confinement guarantee this probe
# exists to provide. A probe that is supposed to verify enforcement integrity
# must not itself degrade silently, so refuse outright if python3 is absent.
command -v python3 >/dev/null 2>&1 || { echo "ERROR: contract-integrity-probe.sh: python3 is required for settings.json parsing + path-confinement checks — refusing (fail-closed)" >&2; exit 2; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# --- ROOT RESOLUTION -------------------------------------------------------
# TWO ROOTS, NEVER ONE. Contract: scripts/lib/resolve-roots.sh.
#
# This probe reads two different kinds of asset and used to have one variable
# for both. Seated in the engine's own repository they coincide, which is why
# the conflation survived; loaded by reference they are different directories
# and the probe reported the ENTITY's canonical settings file as "missing"
# while looking in a repository that never had one.
#
#   ENGINE_ROOT   VERSION, scripts/hooks/*.sh, scripts/lib/*,
#                 mega-lander/workspaces.sh             — what the engine SHIPS
#   ENTITY_ROOT   .claude/settings*.json, orchestration.config, the
#                 PROTECTED_PATHS canary            — what the engine GOVERNS
_RR_LIB="$SCRIPT_DIR/../lib/resolve-roots.sh"
if [ ! -f "$_RR_LIB" ]; then
    echo "FATAL: missing helper $_RR_LIB — cannot resolve the engine and entity roots" >&2
    exit 1
fi
# shellcheck source=../lib/resolve-roots.sh
. "$_RR_LIB"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"

# THE SHARED HOOK-INVENTORY LIBRARY, SOURCED HERE AND NOT WHERE IT IS FIRST USED.
# Both routes need it and they are far apart in this file: BR2 (plugin route)
# judges the Agent chain's order around line 1250, Layer C (seated route) derives
# the whole chain around line 2050. Sourcing it beside either one leaves the other
# calling an undefined function, which under `set -u` is an unbound-variable exit
# rather than a named failure. The layers that use it each carry their own
# not-available branch, so a missing library is reported, never assumed away.
_RH_LIB="$ENGINE_ROOT/scripts/lib/registered-hooks.sh"
if [ -f "$_RH_LIB" ]; then
    # shellcheck source=../lib/registered-hooks.sh disable=SC1090
    . "$_RH_LIB"
fi

if resolve_entity_root ""; then
    REPO_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
else
    # A probe whose whole job is to verify enforcement integrity must never
    # report on a repository it could not identify. Both remaining statuses are
    # fatal HERE — unlike a live guard, which stands down in an unadopted repo,
    # because being ASKED to verify a repository that has not adopted the engine
    # is itself the finding.
    root_failure_banner "scripts/hooks/contract-integrity-probe.sh" >&2
    echo "  A probe cannot verify enforcement in a repository it cannot name." >&2
    echo "  Declare the subject explicitly:  RICHOS_ENTITY_ROOT=<repo> $0" >&2
    exit 2
fi

# --- WHICH MODE IS THIS? ---------------------------------------------------
# Two ways the engine can be installed, and the difference decides whether most
# of the layers below mean anything at all:
#
#   SEATED         ENGINE_ROOT == REPO_ROOT. The engine IS the repository. The
#                  guards are registered in the entity's own
#                  .claude/settings.local.json, so Layers A-Q audit that file
#                  and every "canonical path" is entity-relative — because that
#                  is what the wiring `$CLAUDE_PROJECT_DIR/scripts/hooks/x.sh`
#                  resolves to.
#
#   BY REFERENCE   ENGINE_ROOT != REPO_ROOT. The engine is a plugin; its guards
#                  are registered in the PLUGIN's hooks/hooks.json, and the
#                  entity's settings file legitimately does not mention them.
#                  Running the settings-wiring layers here would emit a wall of
#                  "NOT wired" failures that are all false — the exact false
#                  alarm this probe exists to prevent, pointed at itself.
#
# The by-reference wiring needs its own layer set, and that is not yet written.
# Rather than pretend, the probe SAYS SO and refuses to issue a verdict it
# cannot support. A tool that reports on a configuration it does not understand
# is worse than one that admits the gap.
# The test is "same REPOSITORY?", not "same path". A path comparison alone
# calls the engine "by reference" when the probe is merely invoked from a
# LINKED WORKTREE of the engine's own repo — ENGINE_ROOT is then the worktree
# while REPO_ROOT normalizes to the shared main checkout. Same repository, both
# of them; still seated. So normalize the engine root to ITS main checkout
# before comparing.
PROBE_MODE="seated"
_ENGINE_MAIN="$(resolve_main_checkout "$ENGINE_ROOT" "$ENGINE_ROOT" 2>/dev/null || printf '%s' "$ENGINE_ROOT")"
if [ "$ENGINE_ROOT" != "$REPO_ROOT" ] && [ "$_ENGINE_MAIN" != "$REPO_ROOT" ]; then
    PROBE_MODE="by-reference"
fi

# Load project config (protected trees drive the Layer-D functional canary).
CONFIG="$REPO_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
: "${PROTECTED_PATHS:=}"

C_RED=$'\033[31m'; C_GREEN=$'\033[32m'; C_YELLOW=$'\033[33m'; C_RESET=$'\033[0m'

FAIL=0
# Canonical hook-registration source (committed). Every Layer B-L extraction
# reads this file; Layer M additionally inspects SETTINGS_JSON for duplicates.
SETTINGS="$REPO_ROOT/.claude/settings.local.json"
SETTINGS_LOCAL="$REPO_ROOT/.claude/settings.local.json"
# The gitignored, generated settings file. Post-root-fix it must carry NO hook
# stanzas (install.sh removes/strips them). It may be entirely absent — that is
# the clean single-source state.
SETTINGS_JSON="$REPO_ROOT/.claude/settings.json"

GREEN_N=0
WARN_N=0
emit_pass() { printf '  %s✓%s %s\n' "$C_GREEN" "$C_RESET" "$1" >&2; GREEN_N=$((GREEN_N+1)); }
emit_fail() { printf '  %s✗%s %s\n' "$C_RED" "$C_RESET" "$1" >&2; FAIL=$((FAIL+1)); }
emit_warn() { printf '  %s⚠%s %s\n' "$C_YELLOW" "$C_RESET" "$1" >&2; WARN_N=$((WARN_N+1)); }

# A PASSING RUN USED TO SAY NOTHING AT ALL: it printed its last green line and
# exited 0. A FAILING run gets a loud block naming what broke, so the two
# outcomes were asymmetric in the wrong direction — exit 0 is the one people
# quote at each other, and it arrived with no statement of WHAT was verified.
#
# That is not hypothetical. On 2026-09-10 the same script, run from a femcboost
# working directory, exited 0 after fifteen checks of the PLUGIN route, while
# from the engine's own checkout it runs twenty-seven and audits this
# repository's wiring — and the two runs were compared to each other as though
# they were the same measurement. The by-reference banner at the TOP does say
# which mode is in force and that layers A-Q do not apply; a reader who scrolls
# to the END for the verdict found the verdict was an exit code.
#
# So a passing run now closes by naming its own subject and its own count, and
# the by-reference form says out loud which layers it did NOT run. An artifact
# that does not identify what it attests to gets read as attesting to
# everything.
emit_verdict() { # <what was audited> <what that does and does not cover>
    printf '\n  %s✓%s contract integrity: PASS — %s\n' "$C_GREEN" "$C_RESET" "$1" >&2
    printf '    %s\n' "$2" >&2
    printf '    %s green, 0 broken, %s could not run.\n' "$GREEN_N" "$WARN_N" >&2
}

# --- Engine version banner (informational — NEVER gates) ---
#
# The engine's semantic version lives in the top-level `VERSION` file
# (engine-owned; see VERSIONING.md). Print it so every integrity check is
# self-identifying: an adopter reading a probe run — or a support/onboarding
# operator — can see at a glance which engine version they are verifying. This
# is a pure banner: a missing/unreadable VERSION never increments FAIL and never
# blocks a layer (an adopter who deleted VERSION has a cosmetic gap, not a
# broken guard).
ENGINE_VERSION="$(awk 'NR==1 {gsub(/[[:space:]]/,""); print; exit}' "$ENGINE_ROOT/VERSION" 2>/dev/null || true)"
if [ -n "$ENGINE_VERSION" ]; then
    printf 'richos-engine v%s — contract integrity probe\n' "$ENGINE_VERSION" >&2
else
    printf 'richos-engine (VERSION file absent) — contract integrity probe\n' >&2
fi

realpath_of() {
    local p="$1"
    if [ -z "$p" ]; then
        echo ""
        return
    fi
    python3 -c "import os,sys; print(os.path.realpath(sys.argv[1]))" "$p" 2>/dev/null || echo "$p"
}

sha256_of() {
    local p="$1"
    if [ ! -f "$p" ]; then
        echo ""
        return
    fi
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$p" | awk '{print $1}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$p" | awk '{print $1}'
    else
        python3 -c "import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest())" "$p" 2>/dev/null
    fi
}

# Read the expected hash from the committed manifest sidecar
# ("<hook>.sh.sha256") rather than from the live file itself. The sidecars are
# written by scripts/hooks/install.sh from canonical source. A manifest file
# missing or unreadable is itself a hard failure (the probe refuses to pass
# without a trusted hash source).
manifest_hash_of() {
    local f="$1"
    local mf="$f.sha256"
    if [ ! -f "$mf" ]; then
        echo ""
        return
    fi
    awk 'NR==1 {print $1; exit}' "$mf" 2>/dev/null
}

# run_layer_R — Layer R, as a function.
#
# It is the ONE layer that means the same thing in both installation modes:
# seated or by reference, every guard's decision about WHICH REPOSITORY it is
# protecting comes from scripts/lib/resolve-roots.sh, and a hook that quietly
# went back to trusting its own on-disk location looks identical to one that
# did not until it protects the wrong repository or protects nothing. So both
# paths call it.
run_layer_R() {
    # --- Layer R: THE ROOT-RESOLUTION CONTRACT is wired, uniform and announced ---
    #
    # The guards no longer derive "the repository I protect" from where their own
    # file happens to sit; they resolve it from the SESSION, through
    # scripts/lib/resolve-roots.sh. That makes the resolver the single most
    # consequential file in the mechanical layer — and gives it a failure mode with
    # no external symptom: a hook that quietly reverted to the old
    # `SCRIPT_DIR/../..` resolution looks exactly like a hook that did not, right
    # up until it protects the wrong repository or protects nothing.
    #
    # R1 library present and hashed. R2 every root-resolving hook sources it.
    # R3 the bootstrap block is byte-identical across them — a divergent copy is the
    # original defect in miniature, one hook disagreeing with its siblings about
    # what the root is. R4 engine-status.sh is registered, because it is the only
    # thing in the system that answers "is this defense actually on?".
    R_LIB="$ENGINE_ROOT/scripts/lib/resolve-roots.sh"
    R_OK=1

    if [ ! -f "$R_LIB" ]; then
        emit_fail "R. root-resolution contract MISSING: $R_LIB. Every guard's bootstrap refuses to start without it, so enforcement is off everywhere."
        R_OK=0
    elif [ ! -f "$R_LIB.sha256" ]; then
        emit_fail "R. root-resolution contract unhashed: $R_LIB.sha256 missing — run scripts/hooks/install.sh to regenerate."
        R_OK=0
    else
        R_LIVE="$(sha256_of "$R_LIB" 2>/dev/null || true)"
        R_WANT="$(tr -d '[:space:]' < "$R_LIB.sha256" 2>/dev/null || true)"
        if [ -n "$R_LIVE" ] && [ -n "$R_WANT" ] && [ "$R_LIVE" != "$R_WANT" ]; then
            emit_fail "R. root-resolution contract MODIFIED since install: $R_LIB (sha256 $R_LIVE != manifest $R_WANT). Every guard's root decision comes from this file — review the change, then re-run scripts/hooks/install.sh."
            R_OK=0
        fi
    fi

    # R2/R3 — the hooks that resolve a root. MEMBERSHIP IS DERIVED FROM THE
    # REGISTRATION. The only thing typed here is the ROOTLESS exemption.
    #
    # ===================================================================
    # WHY THIS ONE IS DERIVED AND BR_EXPECTED IS NOT
    # ===================================================================
    # Layer C already states the rule this file lives by, and it decides both
    # cases without appeal to anyone's opinion:
    #
    #   A derivation is a CROSS-SURFACE CHECK when the side it is derived from
    #   and the side it is checked against are DIFFERENT FILES. It is a
    #   TAUTOLOGY when they are the same file.
    #
    # BR2 compares BR_EXPECTED against hooks/hooks.json. Deriving BR_EXPECTED
    # from hooks/hooks.json would leave it checked against itself, unable ever
    # to report a missing guard — so it stays typed, and so does
    # engine-status.test.sh's ACKNOWLEDGED_SCRIPTS, which case 1b compares
    # against a REGISTERED_LIST read from that same file. Both are ORACLES.
    #
    # Layer R is the other shape. Membership comes from the REGISTRATION
    # (hooks/hooks.json); the property — does this hook carry the shared root
    # bootstrap — is read off each hook's OWN SOURCE on disk. Two surfaces, so
    # the check keeps its teeth. The comment that used to sit here feared that
    # "a derived list would have nothing to say when a hook quietly stops
    # sourcing the library", and that fear is exactly right about deriving
    # membership FROM THE DISK: a hook that drops its `. "$_RR_LIB"` line would
    # drop out of a disk-derived list and take its own check with it. Derived
    # from the REGISTRATION it cannot: a hook cannot deregister itself by
    # editing its own bootstrap, so it is still walked and R2 still names it.
    #
    # ===================================================================
    # WHAT THE TYPED LIST COST, MEASURED RATHER THAN ARGUED
    # ===================================================================
    # It named 52 hooks. 57 registered hooks assign ENGINE_ROOT from
    # resolve_engine_root (measured 2026-09-14 against hooks/hooks.json). The
    # five it did not name were not red — a hook this layer does not name is
    # not checked at all — and one of them, guard-stop-live-work.sh, had been
    # carrying a bootstrap that diverges from its siblings the whole time. The
    # preceding commit named the five and fixed the divergence. This one removes
    # the way that happens: a rooted hook registered tomorrow is walked with no
    # edit here, and the list can no longer be short.
    #
    # THE DIRECTION OF THE REMAINING COST IS THE POINT. A hook that resolves NO
    # root must now be declared below, or R2 names it. That is a real edit and it
    # is the minority case — 13 of 70 today — where the old shape charged an edit
    # for the majority case, 57 of 70, and charged it silently.
    #
    # ROOTLESS, EACH FOR A REASON READ OFF ITS SOURCE RATHER THAN ITS INTENT:
    #   guard-brief-scope, notice-claim-capability, handoff-facts-annotate,
    #   notice-inflight-sends, session-start-ci-surface, session-start-scratch,
    #   session-start-quota, session-start-stall, shell-evidence — they
    #     resolve no entity root at all (session-start-stall.sh leaves that to
    #     stall-watch.sh, as session-start-quota.sh does), so naming them would make this layer
    #     assert something false about them (hook-registration-completeness.sh
    #     names handoff-facts-annotate.sh as a live instance of exactly that).
    #   task-completed-handoff, teammate-idle-handoff, worker-created-handoff,
    #     worker-started-handoff, worker-updated-handoff, worker-ended-handoff —
    #     the lifecycle emitters. They append to a store keyed by session, not by
    #     repository, and none of them asks which repository it is in.
    #   guard-ci-red-lands — THE ONE THAT LOOKS LIKE A MISTAKE AND IS NOT. It
    #     SOURCES resolve-roots.sh, so R2's grep would pass on it, and it then
    #     deliberately never calls resolve_engine_root: its own header argues at
    #     length that this gate resolves no entity root and must not start doing
    #     git work for the sake of a printed remedy line. R3's comparison runs to
    #     the resolve_engine_root assignment, so walking it would compare its
    #     whole remaining file against a bootstrap and report a divergence that
    #     is not one. Sourcing the library is not the same claim as resolving a
    #     root, which is why this exemption is declared and not derived.
    #   guard-land-lease-commands — the operator fence's early Bash check. Its
    #     subject is the repository the COMMAND targets (`-C`, `cd`, cwd), and that
    #     repository's own launcher says whether its fence is on; the session's
    #     entity root would be the wrong repository for a `git -C` elsewhere.
    #   guard-foreign-app-data — the Bash rule against reading another app's data.
    #     Its subject is the COMMAND's text and the payload's cwd; which repository the
    #     session sits in changes nothing about whether `find ~` opens other apps'
    #     containers, so it asks for no root.
    #   guard-shared-scratchpad — a teammate's delete inside the lead session's
    #     shared scratchpad. Its subject is the COMMAND's text, the payload's cwd and
    #     agent_id, and the Claude scratch roots; no repository decides it.
    #   guard-unguarded-rm — the Bash rule that refuses an rm Claude Code would prompt the CEO
    #     about. Its subject is the COMMAND's text and the payload's cwd, not a repository.
    #   deliver-review-verdict - a matcherless PreToolUse delivery of a second-review
    #     verdict to the running teammate whose work it is. Its subject is the
    #     payload's agent_id, the review ledger and the workspace registry, all in
    #     the operator's state directory; no repository decides it, and it never
    #     blocks.
    # guard-pierce delegates to its installation-relative Python module; that
    # module reads runtime project/desktop roots, without this shell bootstrap.
    R_ROOTLESS_HOOKS="guard-brief-scope guard-pierce notice-claim-capability handoff-facts-annotate \
    notice-inflight-sends session-start-ci-surface session-start-scratch session-start-quota session-start-stall shell-evidence strip-ack-lines \
    task-completed-handoff teammate-idle-handoff \
    worker-created-handoff worker-started-handoff worker-updated-handoff worker-ended-handoff \
    guard-ci-red-lands guard-land-lease-commands guard-foreign-app-data guard-shared-scratchpad guard-unguarded-rm \
    deliver-review-verdict \
    guard-no-shell-c"

    # FAIL LOUD, NEVER FALL BACK. A typed list kept here "in case the derivation
    # cannot run" would be the second inventory this change exists to delete, and
    # it would be consulted on exactly the day it is most likely to be stale. If
    # the registration cannot be read, this layer says so and fails.
    R_HOOKS_JSON="$ENGINE_ROOT/hooks/hooks.json"
    R_ROOTED_HOOKS=""
    R_DERIVE_ERROR=""
    if ! command -v registered_hook_scripts >/dev/null 2>&1; then
        R_DERIVE_ERROR="the shared hook-inventory library is missing or unreadable at $_RH_LIB"
    else
        set +e
        _r_reg="$(registered_hook_scripts "$R_HOOKS_JSON" 2>/dev/null)"
        _r_rc=$?
        set -e
        case "$_r_rc" in
            0) ;;
            1) R_DERIVE_ERROR="the registration surface is missing at $R_HOOKS_JSON" ;;
            *) R_DERIVE_ERROR="the registration surface at $R_HOOKS_JSON is present but unparseable, or registers no hook script at all" ;;
        esac
    fi
    if [ -n "$R_DERIVE_ERROR" ]; then
        emit_fail "R. the rooted-hook inventory could NOT be derived: $R_DERIVE_ERROR. This layer walks the hooks the registration names and checks each one's root bootstrap; with no registration to read it would walk nothing and pass, which is a green tick over an unchecked defense. Restore hooks/hooks.json and scripts/lib/registered-hooks.sh, then re-run this probe."
        R_OK=0
    else
        _r_rootless_pad=" $(printf '%s ' $R_ROOTLESS_HOOKS)"
        while IFS= read -r _r_h; do
            [ -n "$_r_h" ] || continue
            _r_stem="${_r_h%.sh}"
            case "$_r_rootless_pad" in
                *" $_r_stem "*) continue ;;
            esac
            R_ROOTED_HOOKS="$R_ROOTED_HOOKS $_r_stem"
        done <<R_REG_EOF
$_r_reg
R_REG_EOF
    fi

    # DERIVED, for the same reason BR2's is: a typed count in a green tick is a
    # stale inventory waiting to happen.
    #
    # `|| true` IS LOAD-BEARING AND IS NOT A SWALLOWED ERROR. When the derivation
    # above failed, this list is legitimately EMPTY, and `grep -c .` on empty
    # input prints 0 and exits 1 — which under this file's `set -eo pipefail`
    # aborted the whole probe with exit 1 where the caller expects 2. The typed
    # list could never be empty, so this path did not exist before the
    # derivation; by-reference.test.sh cases 2a and 2h caught it, both of them
    # deleting the registration surface on purpose. The failure is already
    # reported by emit_fail above; this line must not turn it into a crash.
    R_ROOTED_COUNT="$(printf '%s\n' $R_ROOTED_HOOKS | grep -c . || true)"

    R_MISSING_SOURCE=""
    R_BOOTSTRAP_REF=""
    R_DIVERGENT=""
    for h in $R_ROOTED_HOOKS; do
        f="$ENGINE_ROOT/scripts/hooks/$h.sh"
        [ -f "$f" ] || { R_MISSING_SOURCE="$R_MISSING_SOURCE $h(absent)"; continue; }
        if ! grep -q '\. "\$_RR_LIB"' "$f" 2>/dev/null; then
            R_MISSING_SOURCE="$R_MISSING_SOURCE $h"
            continue
        fi
        # The bootstrap runs from the '# --- ROOT RESOLUTION' banner to the
        # resolve_engine_root assignment. Everything in between is fixed text apart
        # from the hook's own name in the diagnostic and the exit code, so both are
        # normalized out before comparison — the point is that the MECHANISM is
        # identical, not that the messages are.
        blk="$(sed -n '/^# --- ROOT RESOLUTION ---/,/^ENGINE_ROOT="\$(resolve_engine_root/p' "$f" 2>/dev/null \
               | sed -e 's|scripts/hooks/[a-z-]*\.sh|<HOOK>|' -e 's|^    exit [0-9]*$|    exit <RC>|')"
        if [ -z "$blk" ]; then
            R_DIVERGENT="$R_DIVERGENT $h(no-bootstrap)"
            continue
        fi
        if [ -z "$R_BOOTSTRAP_REF" ]; then
            R_BOOTSTRAP_REF="$blk"
        elif [ "$blk" != "$R_BOOTSTRAP_REF" ]; then
            R_DIVERGENT="$R_DIVERGENT $h"
        fi
    done

    if [ -n "$R_MISSING_SOURCE" ]; then
        emit_fail "R. hook(s) do NOT source the root-resolution contract:$R_MISSING_SOURCE. A hook that resolves its root any other way has silently gone back to trusting its own on-disk location."
        R_OK=0
    fi
    if [ -n "$R_DIVERGENT" ]; then
        emit_fail "R. root-resolution bootstrap DIVERGED in:$R_DIVERGENT. Every rooted hook must carry the identical bootstrap; a divergent copy is one hook disagreeing with its siblings about which repository is being protected."
        R_OK=0
    fi

    # R4 — the status announcement must be registered. WHERE depends on the
    # installation mode, and conflating the two is the very mistake this probe
    # was rebuilt to stop making: a SEATED engine registers it in the entity's
    # .claude/settings.local.json, while a BY-REFERENCE engine registers it in
    # the plugin's hooks/hooks.json and the entity's settings file correctly
    # never mentions it. Asserting the seated location in by-reference mode
    # produced a hard failure whose only cause was the probe looking in the
    # wrong file. The plugin-surface half below is checked in BOTH modes,
    # because an engine that ships a hooks.json missing the status hook would
    # announce nothing to every adopter.
    if [ "$PROBE_MODE" = "seated" ] && [ -f "$SETTINGS" ]; then
        if ! grep -q 'engine-status\.sh' "$SETTINGS" 2>/dev/null; then
            emit_fail "R. engine-status.sh is NOT registered in $SETTINGS. It is the only hook that reports whether enforcement is active, stood down, or broken — without it every other status in this probe is a claim nobody re-checks at session start."
            R_OK=0
        fi
    fi
    R_PLUGIN_HOOKS="$ENGINE_ROOT/hooks/hooks.json"
    if [ -f "$R_PLUGIN_HOOKS" ] && ! grep -q 'engine-status\.sh' "$R_PLUGIN_HOOKS" 2>/dev/null; then
        emit_fail "R. engine-status.sh is registered for a seated session but NOT in $R_PLUGIN_HOOKS — so a plugin-loaded engine would announce nothing. The two registration surfaces must agree; one of them silently missing a hook is exactly the drift this probe exists to catch."
        R_OK=0
    fi

    if [ "$R_OK" -eq 1 ]; then
        emit_pass "R. root-resolution contract present + hashed + sourced by all $R_ROOTED_COUNT rooted hooks with a byte-identical bootstrap; engine-status.sh registered on both surfaces"
    fi
}

# run_layer_AL — Layer AL, the LIVENESS RESOLVER, as a function.
#
# Mode-independent, for run_layer_R's reason one step over. Two callers
# delegate the whole question "is this agent alive?" to
# scripts/lib/agent-liveness.{py,sh}: guard-agent-state-claims.sh, which
# contradicts the lead's report on the answer, and scripts/agent-liveness.sh,
# the call an operator makes before saying anything about an agent's state. No
# workspace is deleted on its answer: the workspace spec asks no liveness
# question, and its only deleters are land and discard.
#
# On 2026-08-31 the lead told the CEO a LIVE agent had completed, because he
# read a stale roster instead of this answer. A tampered or reverted resolver
# would make that stale reading the correct one in all three places at once —
# and would look exactly like nothing had happened. install.sh mints sidecars
# for these files; without this layer nothing ever compares them, which is
# minting a key and never checking the lock.
#
# MEASURED, and the reason this layer exists rather than a line in install.sh:
# BR4 covers REGISTERED GUARD SCRIPTS only, and Layers Q and S name their own
# files. Appending a byte to scripts/lib/agent-liveness.py and re-running the
# probe was GREEN before this function existed — the sidecar was being minted
# and never read.
# --- Layer MT: the MODEL CAPABILITY ORDER is data, declared once, read by
# every consumer, and the spawn guard obeys it in BOTH directions (HARD gate) ---
#
# On 2026-09-02 the orchestrator inferred a capability order from alias names,
# read Sonnet -> Fable as a downgrade (it is an upgrade), killed a correctly
# configured teammate on the inference, and commissioned a guard whose fixtures
# would have refused that shape permanently. The doctrine had said "don't
# downgrade" for weeks and never said which way down was. Prose does not hold;
# a guard on wrong prose holds the wrong thing. So the order is DATA —
# orchestration.config MODEL_TIERS — and this layer is what keeps every
# statement of it pointing at that one line:
#
#   1. the parser (scripts/lib/model-tiers.sh) is present and hash-matched —
#      it is the only thing that turns the declaration into a verdict, and a
#      trimmed or inverted copy would leave the guard wired and wrong;
#   2. MODEL_TIERS is declared, well-formed, and names exactly the aliases
#      ALLOWED_MODELS names — an alias one has and the other lacks is a spawn
#      the guard cannot rank (it fails open and says so, but says so to nobody
#      a probe reads);
#   3. the doctrine file (CLAUDE.md) does not DRIFT: every `MODEL_TIERS="..."`
#      it quotes equals the declaration, and a CLAUDE.md that says "downgrade"
#      without quoting the declaration at all is the original defect — a
#      direction in prose with no data behind it;
#   4. the guard obeys it, TWO-SIDED, in a sandbox entity (so the layer proves
#      the mechanism, not this repository's roster): an opus-default teammate
#      overridden to sonnet is refused naming the remedy line, and the same
#      teammate overridden to fable (same tier) is allowed IN SILENCE. A dead
#      guard fails the second half; a guard refusing everything fails it too.
#
# Runs in BOTH probe modes: the declaration and the doctrine are the ENTITY's,
# the parser and the guard are the ENGINE's, and the layer reads each from its
# own root.
run_layer_MT() {
    MT_LIB="$ENGINE_ROOT/scripts/lib/model-tiers.sh"
    MT_GUARD="$ENGINE_ROOT/scripts/hooks/guard-worktree-isolation.sh"
    MT_DOCTRINE="$REPO_ROOT/CLAUDE.md"
    MT_OK=1

    if [ ! -f "$MT_LIB" ]; then
        emit_fail "MT. the model-tier parser is MISSING: $MT_LIB. Clause 6 of the spawn guard fails OPEN without it (announced per spawn, read by nobody here), so a move to a lower capability tier is checked by nothing."
        MT_OK=0
    else
        mt_live="$(sha256_of "$MT_LIB" 2>/dev/null || true)"
        mt_want="$(manifest_hash_of "$MT_LIB" 2>/dev/null || true)"
        if [ -z "$mt_want" ]; then
            emit_fail "MT. model-tier parser unhashed: $MT_LIB.sha256 missing — run scripts/hooks/install.sh to regenerate."
            MT_OK=0
        elif [ -n "$mt_live" ] && [ "$mt_live" != "$mt_want" ]; then
            emit_fail "MT. model-tier parser MODIFIED since install: $MT_LIB (sha256 $mt_live != manifest $mt_want). Every tier verdict the spawn guard issues comes out of this file — review the change, then re-run scripts/hooks/install.sh."
            MT_OK=0
        fi
    fi

    if [ "$MT_OK" -eq 1 ]; then
        # shellcheck disable=SC1090
        . "$MT_LIB"
        mt_problem="$(model_tiers_problem "${MODEL_TIERS:-}" 2>/dev/null || true)"
        if [ -n "$mt_problem" ]; then
            emit_fail "MT. MODEL_TIERS in $REPO_ROOT/orchestration.config: $mt_problem. The capability order must be declared as data, once, there — until it is, the spawn guard's clause 6 fails OPEN and nothing checks a move to a lower tier. Declare it beside ALLOWED_MODELS, e.g. MODEL_TIERS=\"fable > opus > sonnet > haiku\", re-derived for the models this harness actually offers."
            MT_OK=0
        else
            mt_set="$(model_tiers_set_problem "$MODEL_TIERS" "${ALLOWED_MODELS:-fable opus sonnet haiku}" 2>/dev/null || true)"
            if [ -n "$mt_set" ]; then
                emit_fail "MT. MODEL_TIERS and ALLOWED_MODELS disagree in $REPO_ROOT/orchestration.config: $mt_set. Both live in that file; an alias one names and the other does not is a spawn the guard cannot rank. Re-derive MODEL_TIERS so the two sets are equal."
                MT_OK=0
            fi
        fi
    fi

    if [ "$MT_OK" -eq 1 ] && [ -f "$MT_DOCTRINE" ]; then
        mt_expected="MODEL_TIERS=\"$MODEL_TIERS\""
        mt_quotes="$(grep -oE 'MODEL_TIERS="[^"]*"' "$MT_DOCTRINE" 2>/dev/null | sort -u || true)"
        if [ -n "$mt_quotes" ]; then
            while IFS= read -r mt_q; do
                [ -n "$mt_q" ] || continue
                if [ "$mt_q" != "$mt_expected" ]; then
                    emit_fail "MT. $MT_DOCTRINE quotes $mt_q but orchestration.config declares $mt_expected — the prose has DRIFTED from the data. The declaration is canonical: fix the quotation in CLAUDE.md, never the other way around."
                    MT_OK=0
                fi
            done <<MT_Q_EOF
$mt_quotes
MT_Q_EOF
        elif grep -qi 'downgrade' "$MT_DOCTRINE" 2>/dev/null; then
            emit_fail "MT. $MT_DOCTRINE says \"downgrade\" but never quotes the declaration it means (no MODEL_TIERS=\"...\" anywhere in the file). That is the 2026-09-02 defect exactly: a direction stated in prose with no data behind it. Add the quotation \`$mt_expected\` to the Models paragraph so this probe can hold the prose to the declaration."
            MT_OK=0
        fi
    fi

    if [ "$MT_OK" -eq 1 ]; then
        if [ ! -x "$MT_GUARD" ]; then
            emit_fail "MT. spawn guard not found / not executable: $MT_GUARD"
            MT_OK=0
        else
            MT_SB="$(mktemp -d -t mt-canary.XXXXXX)"
            mkdir -p "$MT_SB/entity/.claude/agents" "$MT_SB/home"
            printf 'PROTECTED_PATHS=""\nREADONLY_ALLOWLIST="Explore Plan"\nALLOWED_MODELS="fable opus sonnet haiku"\nMODEL_TIERS="fable > opus > sonnet > haiku"\n' \
                >"$MT_SB/entity/orchestration.config"
            printf -- '---\nname: mtjudge\nmodel: opus\n---\nA sandbox judgment role that exists only for this canary.\n' \
                >"$MT_SB/entity/.claude/agents/mtjudge.md"
            # THE ALLOWED LEG HAS TO REACH THE END OF THE GUARD, AND SINCE ROUND 7
            # (eedfbc7d) the end of the guard is clause 7: a spawn is REGISTERED or
            # it does not happen, and registration refuses a repository with no
            # recorded body of work (point 14). A bare directory therefore fails
            # this canary with exit 2 for a reason that has nothing to do with
            # model tiers — which is exactly how it read on main at a6a9e888. So
            # the sandbox is a real repository on `main` with the recording point
            # 14 requires, and the only thing left that can refuse the fable leg is
            # clause 6, which is what MT is the canary for.
            #
            # AND THE SANDBOX SESSION NEEDS A PROCESS IT CAN POINT AT. Clause 7
            # registers through workspaces.py, which refuses a registration whose
            # session identity it cannot read from the operating system (point
            # 12: a registration that could never tell its session ended). That
            # identity is resolved by walking the process ancestry for a process
            # named `claude` — so on a workstation, where the probe is always a
            # descendant of the live session, it resolves by accident of where
            # the probe was run, and on a RUNNER, where nothing in the ancestry
            # is `claude`, it resolves to nothing and the fable leg is refused by
            # clause 7 for a reason that has nothing to do with model tiers.
            # That is exactly the shape the comment above describes, one clause
            # later, and it made this layer red on every GitHub run while passing
            # on every Mac. `mt-canary-0000` is a synthetic session, so it gets a
            # synthetic process to be: a live `sleep` whose pid is handed to
            # every leg through RICHOS_SESSION_PID — the same stand-in Layer Q
            # mints for the same reason, and the same one every sandboxed suite
            # in this engine uses. Nothing about tier enforcement changes; the
            # canary stops depending on its own ancestry.
            MT_SB_OK=1
            MT_SESS_PID="$(sh -c 'sleep 600 >/dev/null 2>&1 & echo $!')"
            git -C "$MT_SB/entity" init -q -b main >/dev/null 2>&1 || MT_SB_OK=0
            printf '.claude/\n' >"$MT_SB/entity/.gitignore"
            git -C "$MT_SB/entity" add -A >/dev/null 2>&1 || MT_SB_OK=0
            GIT_CONFIG_GLOBAL=/dev/null git -C "$MT_SB/entity" \
                -c user.name=probe -c user.email=probe@example.invalid \
                commit -q -m seed >/dev/null 2>&1 || MT_SB_OK=0
            mt_env() { env HOME="$MT_SB/home" RICHOS_ENTITY_ROOT="$MT_SB/entity" \
                           RICHOS_WORKSPACES_DIR="$MT_SB/ws" RICHOS_SESSION_PID="$MT_SESS_PID" \
                           GIT_CONFIG_GLOBAL=/dev/null "$@"; }
            mt_env python3 "$ENGINE_ROOT/mega-lander/workspaces.py" --entity "$MT_SB/entity" \
                --session mt-canary-0000 integration --repo "$MT_SB/entity" --branch main \
                --why "the model-tier canary's body of work" >/dev/null 2>&1 || MT_SB_OK=0
            mt_spawn() { # <name> <model>
                printf '{"tool_name":"Agent","cwd":"%s","session_id":"mt-canary-0000","tool_use_id":"mt-canary-tu","tool_input":{"subagent_type":"mtjudge","name":"%s","isolation":"worktree","model":"%s","prompt":"canary"}}' \
                    "$MT_SB/entity" "$1" "$2"
            }
            set +e
            MT_DOWN_ERR="$(printf '%s' "$(mt_spawn mtjudge-sonnet-1 sonnet)" | mt_env bash "$MT_GUARD" 2>&1 >/dev/null)"
            MT_DOWN_RC=$?
            MT_UP_OUT="$(printf '%s' "$(mt_spawn mtjudge-fable-1 fable)" | mt_env bash "$MT_GUARD" 2>&1)"
            MT_UP_RC=$?
            set -e
            kill "$MT_SESS_PID" 2>/dev/null || true
            rm -rf "$MT_SB"
            if [ "$MT_SB_OK" -ne 1 ]; then
                # A FAILURE, not a warning, for Layer Q's reason: a canary whose
                # sandbox never got built proves nothing, and a warning here would
                # read as "MT was checked" on a run where it was not.
                emit_fail "MT. CANARY DID NOT RUN — the sandbox repository (init, seed commit, or the point-14 recording) failed to build, so the two-sided tier check never executed. The declaration and the parser hash are verified; BEHAVIOR IS NOT."
            elif [ "$MT_DOWN_RC" -ne 2 ]; then
                emit_fail "MT. the spawn guard did NOT refuse an override to a LOWER tier (opus-default teammate on sonnet: exit=$MT_DOWN_RC, expected 2). Clause 6 is not enforcing the declared order."
            elif ! printf '%s' "$MT_DOWN_ERR" | grep -F >/dev/null 'model-downgrade-ack:'; then
                emit_fail "MT. the spawn guard refused a lower-tier override but did NOT name the remedy line (model-downgrade-ack:) — a refusal that says only \"no\" gets routed around instead of obeyed."
            elif [ "$MT_UP_RC" -ne 0 ]; then
                emit_fail "MT. the spawn guard REFUSED a SAME-TIER override (opus-default teammate on fable: exit=$MT_UP_RC, expected 0). It is not enforcing the order, it is refusing everything or refusing to start — exit 2 is ambiguous, which is why this canary is two-sided."
            elif [ -n "$MT_UP_OUT" ]; then
                emit_fail "MT. a same-tier override was allowed but NOT SILENTLY (output: ${MT_UP_OUT:0:160}). Equal-or-higher must say nothing — a notice on an allowed move is a nag that becomes noise."
            else
                emit_pass "MT. capability order declared as data (MODEL_TIERS=\"$MODEL_TIERS\"; alias set = ALLOWED_MODELS), parser hash-matched, doctrine quotation matches, spawn guard REFUSES a lower tier naming the remedy and is SILENT on a same tier (two-sided canary)"
            fi
        fi
    fi
}

# --- Layer MC: the COST CEILING is declared as data, the guard reads it, and
# it refuses ONE tier while staying silent on the next one down (HARD gate) ---
#
# Layer MT keeps the CAPABILITY order honest. This keeps the SPEND order honest,
# and they are not the same order: the top tier is the most capable alias AND
# roughly twice the price of the one below it. The founder ruled on 2026-09-03
# that the normal ceiling for critical work is one tier down, with the top
# reserved for super-critical work and extreme one-off cases, and ordered a
# guard rather than another paragraph — because the ruling of the day before had
# "no guard reads this today" written into it and this repository's own
# measurement is that a rule left as prose gets broken.
#
# WHAT THIS LAYER ASSERTS, and why each half is here:
#
#   1. the guard exists and is executable. Its CONTENT is deliberately NOT
#      hashed here: Layer C (seated) and BR2 (by-reference) already pin its path
#      and hash under REPO_ROOT, and a second hash under ENGINE_ROOT is a false
#      failure rather than a belt — a probe run FROM A LINKED WORKTREE keeps
#      ENGINE_ROOT at the worktree, which carries no scripts/hooks/*.sha256
#      until install.sh has run inside it;
#   2. scripts/lib/resolve-model.sh is present and hash-matched. It is not a
#      hook and appears in no hook table, and BOTH the spawn guard and this one
#      take their entire answer to "which model does this spawn boot on" out of
#      it. Hashing the guards and leaving the thing that decides unverified is
#      checking the lock and ignoring the key, for the Nth time in this probe;
#   3. MODEL_CEILING is declared and RANKABLE under MODEL_TIERS. Undeclared is a
#      WARN, not a failure: the guard fails open and announces per spawn, and an
#      adopter that has not chosen a ceiling yet is unconfigured rather than
#      broken. Declared-but-unrankable IS a failure — an unrankable ceiling is
#      not a ceiling, and it looks exactly like one;
#   4. THE GUARD OBEYS IT, THREE-SIDED, in a sandbox entity (so the layer proves
#      the mechanism, not this repository's roster): a spawn one tier ABOVE the
#      declared ceiling is refused AND the refusal names the ack line; the same
#      teammate AT the ceiling is allowed IN SILENCE; and with no ceiling
#      declared at all the spawn is allowed WITH AN ANNOUNCEMENT. The third arm
#      is the one a dead hook cannot satisfy: a script that never runs exits 0
#      silently, which passes arm two and fails arm three.
#
# Runs in BOTH probe modes: the declaration is the ENTITY's, the guard and the
# resolver are the ENGINE's, and the layer reads each from its own root.
run_layer_MC() {
    MC_GUARD="$ENGINE_ROOT/scripts/hooks/guard-model-ceiling.sh"
    MC_RESOLVER="$ENGINE_ROOT/scripts/lib/resolve-model.sh"
    MC_TIERS_LIB="$ENGINE_ROOT/scripts/lib/model-tiers.sh"
    MC_OK=1

    # THE GUARD: present and executable, NOT hashed here. Its content hash is
    # already pinned twice — Layer C pins it as a member of the PreToolUse[Agent]
    # chain in seated mode, BR2 in by-reference mode — and both read it under
    # REPO_ROOT, where the sidecars live. Hashing it AGAIN under ENGINE_ROOT is
    # not redundancy, it is a false failure: a probe invoked FROM A LINKED
    # WORKTREE resolves REPO_ROOT back to the main checkout but keeps
    # ENGINE_ROOT at the worktree, and a real linked worktree carries NO
    # scripts/hooks/*.sha256 at all until install.sh has been run inside it.
    # Layer MT makes the same distinction for the same reason and only checks
    # its guard is executable. (The RESOLVER below IS hashed here, because
    # nothing else hashes it: it is not a hook and appears in no hook table.)
    if [ ! -f "$MC_GUARD" ]; then
        emit_fail "MC. the cost-ceiling guard is MISSING: $MC_GUARD. Nothing refuses a spawn above the declared ceiling."
        MC_OK=0
    elif [ ! -x "$MC_GUARD" ]; then
        emit_fail "MC. the cost-ceiling guard is not executable: $MC_GUARD. A hook the shell cannot run is a hook that enforces nothing, and the chain does not notice."
        MC_OK=0
    fi

    if [ ! -f "$MC_RESOLVER" ]; then
        emit_fail "MC. the shared model resolver is MISSING: $MC_RESOLVER. Without it neither the cost-ceiling guard NOR the spawn guard can tell which model a spawn boots on — one refuses to start, the other announces that the ceiling is off."
        MC_OK=0
    else
        mc_live="$(sha256_of "$MC_RESOLVER" 2>/dev/null || true)"
        mc_want="$(manifest_hash_of "$MC_RESOLVER" 2>/dev/null || true)"
        if [ -z "$mc_want" ]; then
            emit_fail "MC. shared model resolver unhashed: $MC_RESOLVER.sha256 missing — run scripts/hooks/install.sh to regenerate."
            MC_OK=0
        elif [ -n "$mc_live" ] && [ "$mc_live" != "$mc_want" ]; then
            emit_fail "MC. shared model resolver MODIFIED since install: $MC_RESOLVER (sha256 $mc_live != manifest $mc_want). Every ceiling verdict AND every model-truthfulness verdict the spawn guard issues starts from this one file. Review the change, then re-run scripts/hooks/install.sh."
            MC_OK=0
        fi
    fi

    # The DECLARATION. Undeclared -> WARN (unconfigured, and announced at every
    # spawn by the guard itself). Declared-but-unrankable -> FAIL.
    MC_DECLARED="$(printf '%s' "${MODEL_CEILING:-}" | tr -d '[:space:]')"
    MC_CEIL_RANK=""
    if [ "$MC_OK" -eq 1 ] && [ -f "$MC_TIERS_LIB" ]; then
        # shellcheck disable=SC1090
        . "$MC_TIERS_LIB"
        if [ -z "$MC_DECLARED" ]; then
            emit_warn "MC. no MODEL_CEILING declared in $REPO_ROOT/orchestration.config — the cost ceiling is UNENFORCED here (the guard allows every spawn and announces that per spawn, which is loud but is not a ceiling). Declare it beside MODEL_TIERS, e.g. MODEL_CEILING=\"opus\": the normal ceiling for critical work, with the tier above reserved for super-critical work and extreme one-off cases."
        else
            MC_CEIL_RANK="$(model_tier_rank "$MC_DECLARED" "${MODEL_TIERS:-}" 2>/dev/null || true)"
            if [ -z "$MC_CEIL_RANK" ]; then
                emit_fail "MC. MODEL_CEILING=\"$MC_DECLARED\" is declared in $REPO_ROOT/orchestration.config but MODEL_TIERS=\"${MODEL_TIERS:-}\" ranks it NOWHERE. An unrankable ceiling is not a ceiling: the guard fails open on every spawn while the declaration sits there looking enforced. Both values live in that one file — re-derive them together."
                MC_OK=0
            fi
        fi
    fi

    # THE PROSE MUST NOT DRIFT FROM THE DATA — the half of Layer MT's doctrine
    # check that costs an adopter nothing. If CLAUDE.md quotes a MODEL_CEILING
    # declaration, it must be THIS one. The converse rule MT carries ("the file
    # talks about downgrades but quotes nothing") is deliberately NOT mirrored
    # here: a repository is entitled to run an enforced ceiling without writing a
    # paragraph about it, and a probe that demanded the paragraph would be
    # failing an entity for prose rather than for a hole.
    MC_DOCTRINE="$REPO_ROOT/CLAUDE.md"
    if [ "$MC_OK" -eq 1 ] && [ -n "$MC_DECLARED" ] && [ -f "$MC_DOCTRINE" ]; then
        mc_expected="MODEL_CEILING=\"$MC_DECLARED\""
        mc_quotes="$(grep -oE 'MODEL_CEILING="[^"]*"' "$MC_DOCTRINE" 2>/dev/null | sort -u || true)"
        while IFS= read -r mc_q; do
            [ -n "$mc_q" ] || continue
            if [ "$mc_q" != "$mc_expected" ]; then
                emit_fail "MC. $MC_DOCTRINE quotes $mc_q but orchestration.config declares $mc_expected — the prose has DRIFTED from the data. The declaration is canonical: fix the quotation, never the other way around."
                MC_OK=0
            fi
        done <<MC_Q_EOF
$mc_quotes
MC_Q_EOF
    fi

    if [ "$MC_OK" -eq 1 ]; then
        MC_SB="$(mktemp -d -t mc-canary.XXXXXX)"
        mkdir -p "$MC_SB/entity/.claude/agents" "$MC_SB/home"
        printf -- '---\nname: mcprobe\nmodel: opus\n---\nA sandbox role that exists only for this canary.\n' \
            >"$MC_SB/entity/.claude/agents/mcprobe.md"
        mc_config() { # <ceiling-line-or-empty>
            {
                printf 'PROTECTED_PATHS=""\nREADONLY_ALLOWLIST="Explore Plan"\n'
                printf 'ALLOWED_MODELS="fable opus sonnet haiku"\n'
                printf 'MODEL_TIERS="fable > opus > sonnet > haiku"\n'
                if [ -n "$1" ]; then printf '%s\n' "$1"; fi
            } >"$MC_SB/entity/orchestration.config"
        }
        mc_spawn() { # <name> <model>
            printf '{"tool_name":"Agent","cwd":"%s","session_id":"mc-canary-0000","tool_use_id":"mc-canary-tu","tool_input":{"subagent_type":"mcprobe","name":"%s","isolation":"worktree","model":"%s","prompt":"canary"}}' \
                "$MC_SB/entity" "$1" "$2"
        }
        mc_run() { # <name> <model> — sets MC_OUT / MC_RC
            set +e
            MC_OUT="$(printf '%s' "$(mc_spawn "$1" "$2")" | env HOME="$MC_SB/home" RICHOS_ENTITY_ROOT="$MC_SB/entity" bash "$MC_GUARD" 2>&1)"
            MC_RC=$?
            set -e
        }

        mc_config 'MODEL_CEILING="opus"'
        mc_run mcprobe-fable-1 fable
        MC_OVER_OUT="$MC_OUT"; MC_OVER_RC="$MC_RC"
        mc_run mcprobe-opus-1 opus
        MC_AT_OUT="$MC_OUT"; MC_AT_RC="$MC_RC"
        mc_config ''
        mc_run mcprobe-fable-2 fable
        MC_NONE_OUT="$MC_OUT"; MC_NONE_RC="$MC_RC"
        rm -rf "$MC_SB"

        if [ "$MC_OVER_RC" -ne 2 ]; then
            emit_fail "MC. the guard did NOT refuse a spawn one tier ABOVE the declared ceiling (opus ceiling, fable spawn: exit=$MC_OVER_RC, expected 2). The cost ceiling is declared and enforcing nothing."
        elif ! printf '%s' "$MC_OVER_OUT" | grep -F >/dev/null 'model-ceiling-ack:'; then
            emit_fail "MC. the guard refused an over-ceiling spawn but did NOT name the remedy line (model-ceiling-ack:) — the founder asked for acknowledge-OR-reconsider, and a refusal that offers neither gets routed around instead of obeyed."
        elif ! printf '%s' "$MC_OVER_OUT" | grep -F >/dev/null 'EVERYTHING DOWNSTREAM INHERITS'; then
            emit_fail "MC. the over-ceiling refusal no longer carries the ONE-OFF test. It is the whole instruction: the note has to REACH the reader, not be pointed at."
        elif [ "$MC_AT_RC" -ne 0 ]; then
            emit_fail "MC. the guard REFUSED a spawn AT the ceiling (opus ceiling, opus spawn: exit=$MC_AT_RC, expected 0). It is not enforcing a ceiling, it is refusing everything or refusing to start — exit 2 is ambiguous, which is why this canary has more than one side."
        elif [ -n "$MC_AT_OUT" ]; then
            emit_fail "MC. a spawn AT the ceiling was allowed but NOT SILENTLY (output: ${MC_AT_OUT:0:160}). A ceiling that comments on the normal case is a nag, and a nag is how a guard becomes something to disable."
        elif [ "$MC_NONE_RC" -ne 0 ]; then
            emit_fail "MC. with NO ceiling declared the guard did not fail OPEN (exit=$MC_NONE_RC, expected 0). An undeclared ceiling must never wedge a dispatch — a guard that blocks over its own configuration is a guard that gets switched off."
        elif ! printf '%s' "$MC_NONE_OUT" | grep -F >/dev/null 'NOT DECLARED'; then
            emit_fail "MC. with NO ceiling declared the guard was SILENT. That is the arm a dead hook passes for free — an absent ceiling and an enforced one must never look the same."
        else
            if [ -n "$MC_DECLARED" ]; then
                MC_DECL_NOTE="cost ceiling declared as data (MODEL_CEILING=\"$MC_DECLARED\")"
            else
                # Never read as "declared". The canary proves the MECHANISM in a
                # sandbox that declares its own ceiling; this repository has not
                # declared one, and the warning above is the finding.
                MC_DECL_NOTE="cost ceiling mechanism proven in a sandbox (this repository declares NO ceiling — see the warning above)"
            fi
            emit_pass "MC. ${MC_DECL_NOTE}, guard present and executable, shared model resolver hash-matched, and the guard REFUSES one tier above naming the remedy, is SILENT at the ceiling, and ANNOUNCES when no ceiling is declared (three-sided canary)"
        fi
    fi
}

run_layer_AL() {
    AL_OK=1
    AL_FILES="scripts/lib/agent-liveness.py scripts/lib/agent-liveness.sh scripts/agent-liveness.sh"
    AL_MISSING=""
    AL_UNHASHED=""
    AL_MISMATCH=""
    for f in $AL_FILES; do
        af="$ENGINE_ROOT/$f"
        if [ ! -f "$af" ]; then
            AL_MISSING="$AL_MISSING $f"
            continue
        fi
        al_live="$(sha256_of "$af" 2>/dev/null || true)"
        al_want="$(manifest_hash_of "$af" 2>/dev/null || true)"
        if [ -z "$al_want" ]; then
            AL_UNHASHED="$AL_UNHASHED $f"
        elif [ -n "$al_live" ] && [ "$al_live" != "$al_want" ]; then
            AL_MISMATCH="$AL_MISMATCH $f"
        fi
    done

    if [ -n "$AL_MISSING" ]; then
        emit_fail "AL. the agent-liveness resolver is MISSING:$AL_MISSING. guard-agent-state-claims.sh and scripts/agent-liveness.sh decide nothing themselves — without it the claim check stops running."
        AL_OK=0
    fi
    if [ -n "$AL_UNHASHED" ]; then
        emit_fail "AL. agent-liveness resolver unhashed:$AL_UNHASHED (.sha256 sidecar missing) — run scripts/hooks/install.sh to regenerate."
        AL_OK=0
    fi
    if [ -n "$AL_MISMATCH" ]; then
        emit_fail "AL. agent-liveness resolver MODIFIED since install:$AL_MISMATCH. Three callers take their answer from this file, including the one that deletes worktrees — review the change, then re-run scripts/hooks/install.sh."
        AL_OK=0
    fi

    # FUNCTIONAL CANARY. A resolver gutted to "always NOT-ALIVE" passes every
    # hash check above and turns the removal helper into a shredder. So the
    # shipped file is asked three questions with known answers, in a throwaway
    # repo whose worktree is locked by a pid this probe knows is running: its
    # own. All three verdicts are asserted, because a resolver that only ever
    # says one of them is broken in a way that no single question reveals.
    if [ "$AL_OK" -eq 1 ] && command -v python3 >/dev/null 2>&1 && command -v git >/dev/null 2>&1; then
        AL_SB="$(mktemp -d 2>/dev/null || true)"
        if [ -n "$AL_SB" ] && [ -d "$AL_SB" ]; then
            AL_SB="$(cd "$AL_SB" && pwd -P)"
            AL_E="$AL_SB/e"
            mkdir -p "$AL_E" "$AL_SB/nohooks"
            git -C "$AL_E" init -q -b main . >/dev/null 2>&1
            git -C "$AL_E" config user.email p@p.p >/dev/null 2>&1
            git -C "$AL_E" config user.name p >/dev/null 2>&1
            git -C "$AL_E" config core.hooksPath "$AL_SB/nohooks" >/dev/null 2>&1
            echo s > "$AL_E/s.txt"
            git -C "$AL_E" add -A >/dev/null 2>&1
            git -C "$AL_E" commit -qm s >/dev/null 2>&1
            AL_WT="$AL_E/.claude/worktrees/agent-aprobe0000000001"
            git -C "$AL_E" worktree add -q -b wt-probe "$AL_WT" >/dev/null 2>&1
            git -C "$AL_E" worktree lock --reason "claude agent agent-aprobe0000000001 (pid $$ start now)" "$AL_WT" >/dev/null 2>&1
            AL_PY="$ENGINE_ROOT/scripts/lib/agent-liveness.py"
            AL_V_LIVE="$(python3 "$AL_PY" --entity "$AL_E" --owner aprobe0000000001 --format triple 2>/dev/null | cut -f1)"
            AL_V_GONE="$(python3 "$AL_PY" --entity "$AL_E" --owner anevereverexisted1 --format triple 2>/dev/null | cut -f1)"
            AL_V_IND="$(python3 "$AL_PY" --entity "$AL_SB/not-a-repo" --owner aprobe0000000001 --format triple 2>/dev/null | cut -f1)"
            rm -rf "$AL_SB"
            if [ "$AL_V_LIVE" != "ALIVE" ]; then
                emit_fail "AL. FUNCTIONAL CANARY: a worktree locked by a RUNNING pid resolved '$AL_V_LIVE', not ALIVE. The resolver is gutted — no claim about an agent's state is being checked against anything."
                AL_OK=0
            elif [ "$AL_V_GONE" != "NOT-ALIVE" ]; then
                emit_fail "AL. FUNCTIONAL CANARY: an unregistered worktree resolved '$AL_V_GONE', not NOT-ALIVE. A resolver that never says NOT-ALIVE refuses every removal, and an operator who cannot remove anything reaches for the ack override."
                AL_OK=0
            elif [ "$AL_V_IND" != "INDETERMINATE" ]; then
                emit_fail "AL. FUNCTIONAL CANARY: an unqueryable repository resolved '$AL_V_IND', not INDETERMINATE. The third outcome has been collapsed, so 'I could not tell' is now indistinguishable from an answer — which is the confusion the 2026-08-31 incident was made of."
                AL_OK=0
            else
                emit_pass "AL. agent-liveness resolver present, hashed and behaving: a live lock -> ALIVE, an absent worktree -> NOT-ALIVE, an unqueryable repo -> INDETERMINATE (never collapsed)"
            fi
        else
            emit_warn "AL. FUNCTIONAL CANARY DID NOT RUN — no sandbox directory (mktemp). Presence and hashes are verified; BEHAVIOR IS NOT, so a gutted resolver would not be caught by this run."
        fi
    elif [ "$AL_OK" -eq 1 ]; then
        emit_warn "AL. FUNCTIONAL CANARY DID NOT RUN — git or python3 unavailable. Presence and hashes are verified; BEHAVIOR IS NOT."
    fi
}

# ===========================================================================
# BY-REFERENCE LAYER SET — BR1..BR9
# ===========================================================================
#
# Layers A-Q audit `.claude/settings.local.json`. Under a plugin-loaded engine
# that file legitimately never mentions a single engine guard, so running them
# here would emit a wall of "NOT wired" failures, every one of them false.
#
# Until 2026-08-28 the probe said exactly that and refused a verdict. Honest,
# and it left an adopter running by reference with NO automated wiring check at
# all — which is how a real regression (an adopting repository deleted its 26
# hand-ported guard copies and did not visibly get them back from the plugin)
# reached an operator instead of a test. The layers below are that missing
# check.
#
#   BR1  the plugin manifest exists, parses, and names the plugin
#   BR2  the plugin's hooks/hooks.json registers every managed guard EXACTLY
#        ONCE, on the right event, with the Agent chain in canonical ORDER
#   BR3  every registered command is confined to ${CLAUDE_PLUGIN_ROOT} — no
#        absolute path, no $CLAUDE_PROJECT_DIR, no `..` escape
#   BR4  every registered script exists, is executable, and matches its sidecar
#        hash when one is present (absent sidecars are NAMED as unverified,
#        never waved through: the engine root is read-only by reference, so
#        minting them is the engine maintainer's job, not the adopter's)
#   BR5  the manifest's declared agent files resolve, are regular .md files
#        inside the plugin root, and carry a frontmatter name
#   BR6  THIS OPERATOR WILL ACTUALLY LOAD THIS ENGINE — the user-scope plugin
#        registration chain (enabledPlugins -> known marketplace -> marketplace
#        manifest -> plugin source) resolves to THIS engine root
#   BR6b the entity-facing engine POINTER (the symlink install.sh mints, which
#        an entity's OWN scripts follow because they get no $CLAUDE_PLUGIN_ROOT)
#        agrees with that registration. Absent -> named warning; present and
#        disagreeing, dangling, or aimed at a non-engine -> failure
#   BR10 the ENTITY's own critical config — env.CLAUDE_CODE_EXPERIMENTAL_AGENT_
#        TEAMS="1" and worktree.baseRef="head". The plugin cannot supply these:
#        they are project-scope keys and they are the entity's property. Every
#        other BR layer audits the ENGINE; without this one an entity gained
#        plugin verification and silently LOST config verification on adoption
#   BR7  the marketplace manifest is git-TRACKED, so the next clone can register
#   BR8  engine-status.sh reports ACTIVE for this entity AND emits an
#        operator-visible systemMessage — paired against an unadopted directory
#        that must report STOOD DOWN, because "always says ACTIVE" satisfies a
#        one-armed check
#   BR9  a guard actually BLOCKS what it should and ALLOWS what it should, run
#        by reference (engine root != entity root), against the entity's own
#        orchestration.config and roster
#
# BR6 is the layer that would have caught the reported regression on its own:
# every byte of the engine can be correct and every guard present while the
# operator's registration points somewhere else, or nowhere. Guards on disk are
# not enforcement; guards the host will load are.
if [ "$PROBE_MODE" = "by-reference" ]; then
    {
        echo ""
        echo "=== ENGINE LOADED BY REFERENCE — auditing the PLUGIN route (BR1-BR10) ==="
        echo "  engine : $ENGINE_ROOT"
        echo "  entity : $REPO_ROOT"
        echo ""
        echo "  Layers A-Q audit .claude/settings.local.json and DO NOT APPLY here: the"
        echo "  guards are registered in the plugin's hooks/hooks.json and this repo's"
        echo "  settings file legitimately never mentions them."
        echo ""
    } >&2

    BR_PLUGIN_MANIFEST="$ENGINE_ROOT/.claude-plugin/plugin.json"
    BR_PLUGIN_HOOKS="$ENGINE_ROOT/hooks/hooks.json"
    # The engine may legitimately be audited from a LINKED WORKTREE of its own
    # repository while the operator's registration points at the shared main
    # checkout. BR6 needs the main-checkout TWIN of the audited directory —
    # note twin, not repository root: resolve_main_checkout() answers "which
    # checkout?", and a nested engine at <repo>/engine normalizes to <repo>,
    # which is not an engine and never resolves against any plugin source.
    # (First live run named the repository root for an engine that lives one
    # directory further down. Same conflation as ENGINE_ROOT vs ENTITY_ROOT,
    # one level in.)
    BR_ENGINE_TWIN=""
    if [ "$_ENGINE_MAIN" != "$ENGINE_ROOT" ]; then
        _br_wt_top="$(git -C "$ENGINE_ROOT" rev-parse --show-toplevel 2>/dev/null || true)"
        if [ -n "$_br_wt_top" ]; then
            case "$ENGINE_ROOT" in
                "$_br_wt_top") BR_ENGINE_TWIN="$_ENGINE_MAIN" ;;
                "$_br_wt_top"/*) BR_ENGINE_TWIN="$_ENGINE_MAIN/${ENGINE_ROOT#"$_br_wt_top"/}" ;;
            esac
        fi
    fi
    BR_ENGINE_MAIN="$ENGINE_ROOT"

    # The managed guard set, and the event each one belongs on. This is the
    # SAME set the seated table wires; the two registration surfaces disagreeing
    # is itself the drift Layer R's R4 check exists to catch, and BR2 catches
    # the plugin half of it.
    #
    # THIS TABLE IS THE SPECIFICATION, NOT AN INVENTORY. It is typed on purpose
    # and it must stay typed: it is the probe's INDEPENDENT oracle, the only
    # record of what SHOULD be wired. Deriving it from hooks.json would make
    # BR2 tautological — the file would be checked against itself and could
    # never report a missing guard again.
    #
    # What must never be typed is a COUNT of it (BR_EXPECTED_COUNT below is
    # derived) or a second copy of it elsewhere. engine-status.sh used to keep
    # exactly such a second copy for its session banner; it drifted twice in two
    # days and now derives from hooks.json instead. The consequence for this
    # table is that BR2 is checked in BOTH directions — spec -> registration
    # (a declared guard that is not wired) and registration -> spec (a wired
    # guard nobody declared). One direction alone is how a real guard ran for
    # two days uncounted by anything.
    #
    # ONE LINE PER REGISTRATION, NOT PER SCRIPT. dispatch-pretooluse.sh appears
    # TWICE on PreToolUse because it is registered twice — once under the `Bash`
    # matcher and once under `Write|Edit|MultiEdit|NotebookEdit`. The rules it
    # runs are named in scripts/hooks/dispatch-pretooluse.manifest and are still
    # listed here individually: BR2's parse expands the dispatcher into them, so
    # a rule deleted from the manifest arrives here as NOT registered, exactly as
    # a rule deleted from hooks.json used to.
    #
    # guard-no-home-network-phone.sh appears TWICE for the same reason and a
    # different one: it is the first RULE registered on BOTH surfaces — once
    # under `Agent` in hooks.json, once in the Write chain's manifest — because
    # the ruling it carries (ceo-decisions §61) names "any brief, code write or
    # spawn". A brief and code are Writes; a dispatch is an Agent call. It is
    # therefore deliberately NOT in the single-registration list below.
    #
    # guard-host-display-power.sh appears FOUR TIMES, which is the most of any
    # rule here, and the count is the ruling: ceo-decisions §65 says "no agent,
    # and no brief, MESSAGE or script". So it is registered under `Agent`, under
    # `SendMessage`, and in BOTH dispatcher chains (Bash and Write). The
    # SendMessage registration is not symmetry — it is the surface the incident
    # actually used: the `pmset displaysleepnow` that blacked out the CEO's
    # monitors reached the teammate through the mailbox, and its spawn prompt
    # does not contain the word.
    #
    # guard-public-record-repo.sh appears TWICE: once under `Agent` in
    # hooks.json and once in the Bash chain's manifest. The two entries are the
    # two ends of one incident — the dispatch that sent a research read into the
    # PUBLIC repository, and the commit that put its ledger in published
    # history. Refusing only the second would leave a teammate discovering at
    # commit time that its whole workspace is in the wrong repository.
    #
    # guard-reference-ledger.sh appears ONCE, on `Agent` only, and the asymmetry
    # is deliberate: it judges what a DISPATCH is about to build, which is a
    # question only a spawn prompt answers. A Write registration would have to
    # re-derive "which surface is this file on" from a path, and that is the
    # ledger's own job.
    BR_EXPECTED="\
engine-status.sh|SessionStart
workspace-lifecycle.sh|SessionStart
workspace-lifecycle.sh|SessionEnd
snapshot-agent-definitions.sh|SessionStart
snapshot-enforcing-hooks.sh|SessionStart
session-start-ceo-ask.sh|SessionStart
session-start-escalations.sh|SessionStart
session-start-ci-surface.sh|SessionStart
notice-disk-alert.sh|SessionStart
session-start-scratch.sh|SessionStart
session-start-quota.sh|SessionStart
session-start-stall.sh|SessionStart
left-off-report.sh|SessionStart
guard-sealed-worktree.sh|PreToolUse
guard-worktree-isolation.sh|PreToolUse
guard-definition-drift.sh|PreToolUse
verify-agent-prompt.sh|PreToolUse
guard-ceo-ask-first.sh|PreToolUse
guard-model-ceiling.sh|PreToolUse
guard-stale-staging.sh|PreToolUse
guard-owned-state.sh|PreToolUse
guard-brief-scope.sh|PreToolUse
guard-pierce.sh|PreToolUse
guard-main-checkout-writes.sh|PreToolUse
scan-secrets.sh|PreToolUse
guard-publication-writes.sh|PreToolUse
guard-named-persons-writes.sh|PreToolUse
guard-dialect.sh|PreToolUse
guard-no-home-network-phone.sh|PreToolUse
guard-no-home-network-phone.sh|PreToolUse
guard-host-display-power.sh|PreToolUse
guard-host-display-power.sh|PreToolUse
guard-host-display-power.sh|PreToolUse
guard-host-display-power.sh|PreToolUse
guard-reference-ledger.sh|PreToolUse
guard-public-record-repo.sh|PreToolUse
guard-public-record-repo.sh|PreToolUse
guard-resume-isolation.sh|PreToolUse
guard-bash-main-writes.sh|PreToolUse
dispatch-pretooluse.sh|PreToolUse
dispatch-pretooluse.sh|PreToolUse
shell-evidence.sh|PreToolUse
strip-ack-lines.sh|PreToolUse
guard-interactive-prompt.sh|PreToolUse
guard-inflight-notify.sh|PreToolUse
guard-worktree-removal.sh|PreToolUse
guard-publication-commits.sh|PreToolUse
guard-named-persons-commands.sh|PreToolUse
guard-ceo-todos-commits.sh|PreToolUse
guard-completeness-commits.sh|PreToolUse
guard-row-currency-commits.sh|PreToolUse
guard-vendoring-commits.sh|PreToolUse
guard-hook-registration-commits.sh|PreToolUse
guard-workflow-ban.sh|PreToolUse
guard-ci-red-lands.sh|PreToolUse
guard-land-lease-commands.sh|PreToolUse
guard-foreign-app-data.sh|PreToolUse
guard-shared-scratchpad.sh|PreToolUse
guard-unguarded-rm.sh|PreToolUse
guard-no-shell-c.sh|PreToolUse
guard-stop-live-work.sh|PreToolUse
deliver-review-verdict.sh|PreToolUse
observe-created-refs.sh|PostToolUse
detect-nonnative-worktree.sh|PostToolUse
worker-created-handoff.sh|PostToolUse
worker-updated-handoff.sh|PostToolUse
notice-inflight-sends.sh|PostToolUse
notice-ceo-asks.sh|PostToolUse
workspace-lifecycle.sh|PostToolUse
handoff-facts-annotate.sh|PostToolUse
notice-claim-capability.sh|PostToolUse
workspace-lifecycle.sh|SubagentStart
worker-started-handoff.sh|SubagentStart
workspace-lifecycle.sh|SubagentStop
worker-ended-handoff.sh|SubagentStop
teammate-idle-handoff.sh|TeammateIdle
task-completed-handoff.sh|TaskCompleted
workspace-lifecycle.sh|TaskCompleted
guard-unresolved-claims.sh|Stop
turn-manifest.sh|Stop
notice-hook-staleness.sh|Stop
notice-disk-alert.sh|Stop
notice-unlanded-branches.sh|Stop
guard-workspace-gate.sh|Stop
guard-ci-turn-gate.sh|Stop
notice-inflight-acks.sh|Stop
notice-mechanical-findings.sh|Stop
guard-ceo-ruled-ask.sh|PreToolUse
notice-ceo-ruled-prose.sh|Stop
notice-unanswered-question.sh|Stop
notice-waiver-repetition.sh|Stop
notice-escalations.sh|Stop
notice-protected-ref-moves.sh|Stop
notice-unstarted-rows.sh|Stop
notice-ceo-unasked.sh|Stop
notice-unasked-deferral.sh|Stop
guard-agent-state-claims.sh|Stop
guard-idle-land.sh|Stop
ass-kicker.sh|Stop
commit-ceo-inputs.sh|UserPromptSubmit
left-off-report.sh|UserPromptSubmit
failure-type-lookup.sh|UserPromptSubmit
guard-failure-type-answer.sh|Stop
guard-resource-waits.sh|Stop
notice-ceo-inputs-unheld.sh|Stop
release-land-leases.sh|Stop
operator-claim.sh|SessionStart
guard-operator-claim.sh|PreToolUse
guard-operator-claim.sh|PreToolUse
guard-operator-claim.sh|PreToolUse
guard-operator-claim.sh|PreToolUse
guard-live-names.sh|PreToolUse
guard-shared-writes.sh|PreToolUse
guard-record-owner-memory.sh|PreToolUse
release-shared-writes.sh|PostToolUse
release-shared-writes.sh|PostToolUseFailure"

    # DERIVED, never hand-maintained. A literal count in the PASS text is a
    # drift surface of exactly the kind this probe exists to remove: add a
    # guard, forget the number, and the probe reports a stale inventory while
    # passing.
    BR_EXPECTED_COUNT="$(printf '%s\n' "$BR_EXPECTED" | grep -c '|')"

    # --- BR1 — plugin manifest present, parseable, named ---
    BR_PLUGIN_NAME=""
    if [ ! -f "$BR_PLUGIN_MANIFEST" ]; then
        emit_fail "BR1. plugin manifest MISSING: $BR_PLUGIN_MANIFEST. Without it the host has no plugin to load, so none of the guards below can ever run no matter how correct they are."
    else
        BR_PLUGIN_NAME="$(python3 -c '
import json, sys
try:
    d = json.load(open(sys.argv[1], encoding="utf-8"))
except Exception:
    sys.exit(0)
n = d.get("name")
if isinstance(n, str):
    print(n.strip())
' "$BR_PLUGIN_MANIFEST" 2>/dev/null || true)"
        if [ -z "$BR_PLUGIN_NAME" ]; then
            emit_fail "BR1. plugin manifest unparseable or has no \"name\": $BR_PLUGIN_MANIFEST. The name IS the namespace every plugin-supplied role and skill is addressed by, so an unnamed plugin supplies nothing addressable."
        else
            emit_pass "BR1. plugin manifest present and names the plugin: $BR_PLUGIN_NAME"
        fi
    fi

    # --- BR2 — every managed guard registered EXACTLY ONCE, right event, right order ---
    #
    # "Exactly once" is not pedantry. Claude Code MERGES hook sources additively,
    # so a guard registered twice fires twice per matching tool event; Layer M
    # exists because that has actually happened on the seated surface. The
    # plugin surface has the same additive property and, until now, no check.
    BR_HOOKS_ROWS=""
    if [ ! -f "$BR_PLUGIN_HOOKS" ]; then
        emit_fail "BR2. plugin hook table MISSING: $BR_PLUGIN_HOOKS. The manifest alone registers nothing — this file is where the guards are wired for a plugin-loaded engine. Every guard would be present on disk and none of them would ever run."
    else
        BR_HOOKS_ROWS="$(python3 -c '
import json, os, re, sys
try:
    d = json.load(open(sys.argv[1], encoding="utf-8"))
except Exception:
    sys.exit(1)
hooks = d.get("hooks", {})
if not isinstance(hooks, dict):
    sys.exit(1)

# THE DISPATCHER EXPANDS INTO ITS RULES, and this parse implements that rule
# INDEPENDENTLY — it reads the manifest itself rather than asking
# scripts/lib/registered-hooks.sh, because BR2 exists to disagree with that
# library when the library is wrong. Each rule gets a row carrying the
# dispatcher own event and matcher, which is where that rule was registered
# before 2026-09-15. The dispatcher own rows are KEPT as well: it is the single
# point of failure for seventeen guards, so its presence, its event and its
# sha256 must still be checked like any other registered script.
manifest = {}
_m = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(sys.argv[1]))),
                  "scripts", "hooks", "dispatch-pretooluse.manifest")
if os.path.isfile(_m):
    for line in open(_m, encoding="utf-8"):
        line = line.strip()
        if line and not line.startswith("#") and "|" in line:
            k, mod = line.split("|", 1)
            manifest.setdefault(k.strip(), []).append(mod.strip())

for event, entries in hooks.items():
    if not isinstance(entries, list):
        continue
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        matcher = entry.get("matcher", "")
        for h in entry.get("hooks", []) or []:
            if not isinstance(h, dict):
                continue
            cmd = str(h.get("command", ""))
            print("\t".join([event, str(matcher), cmd]))
            hit = re.search(r"(\S*scripts/hooks/)dispatch-pretooluse\.sh\s+(\S+)", cmd)
            if hit:
                for mod in manifest.get(hit.group(2), []):
                    print("\t".join([event, str(matcher), hit.group(1) + mod]))
' "$BR_PLUGIN_HOOKS" 2>/dev/null || true)"
        if [ -z "$BR_HOOKS_ROWS" ]; then
            emit_fail "BR2. plugin hook table unparseable or registers nothing: $BR_PLUGIN_HOOKS"
        else
            BR2_OK=1
            BR2_PROBLEMS=""
            while IFS='|' read -r _script _event; do
                [ -n "$_script" ] || continue
                _count="$(printf '%s\n' "$BR_HOOKS_ROWS" | grep -c "scripts/hooks/$_script" || true)"
                _on_event="$(printf '%s\n' "$BR_HOOKS_ROWS" | awk -F'\t' -v s="scripts/hooks/$_script" -v e="$_event" '$1==e && index($3,s)>0' | wc -l | tr -d ' ')"
                # EXACTLY ONCE IS A PER-EVENT INVARIANT, NOT A PER-SCRIPT ONE.
                # The double-fire this guards against is one hook wired twice on
                # the SAME event, which fires it twice per occurrence of that
                # event. A hook legitimately wired on two DIFFERENT events fires
                # once on each, which is not a double-fire and must not be
                # reported as one — terminalize-agent-worktrees.sh is wired on
                # both SubagentStop and WorktreeRemove because the two race for
                # one terminal claim, and the old per-script count would call
                # that a defect. The expected total is DERIVED from how many
                # events the managed set declares for this script, so it can
                # never be a typed number that falls behind.
                _declared_events="$(printf '%s\n' "$BR_EXPECTED" | awk -F'|' -v s="$_script" '$1==s' | grep -c . || true)"
                # ...and how many of those declarations are ON THIS EVENT. The
                # two numbers are different the moment a script is declared on
                # more than one event (workspace-lifecycle.sh: six) or more than
                # once on ONE event (dispatch-pretooluse.sh: twice on PreToolUse,
                # under two different matchers).
                _declared_here="$(printf '%s\n' "$BR_EXPECTED" | awk -F'|' -v s="$_script" -v e="$_event" '$1==s && $2==e' | grep -c . || true)"
                if [ "$_count" -eq 0 ]; then
                    BR2_PROBLEMS="$BR2_PROBLEMS $_script(NOT registered)"
                    BR2_OK=0
                elif [ "$_count" -gt "$_declared_events" ]; then
                    BR2_PROBLEMS="$BR2_PROBLEMS $_script(registered ${_count}x for ${_declared_events} declared event(s) -> double-fire)"
                    BR2_OK=0
                # ONE PER DECLARED REGISTRATION, not literally one. A script is
                # allowed to be registered more than once on ONE event when the
                # managed set declares it that way — dispatch-pretooluse.sh is
                # registered on PreToolUse twice, under the `Bash` matcher and
                # under the `Write|Edit|...` matcher, which is two different
                # tools and therefore not the additive double-fire this is
                # guarding against. Registering it a THIRD time still trips the
                # `_count -gt _declared_events` arm above.
                elif [ "$_on_event" -ne "$_declared_here" ]; then
                    BR2_PROBLEMS="$BR2_PROBLEMS $_script(registered, but not exactly once on $_event)"
                    BR2_OK=0
                fi
            done <<BR_EOF
$BR_EXPECTED
BR_EOF
            if [ "$BR2_OK" -eq 0 ]; then
                emit_fail "BR2. plugin hook table wiring wrong:$BR2_PROBLEMS. A guard that is absent enforces nothing; a guard registered twice fires twice on every matching event (the additive-merge double-fire Layer M exists for, arriving through the plugin door)."
            fi

            # The PreToolUse[Agent] chain ORDER is load-bearing: the isolation
            # guard must refuse a bad spawn before the later hooks reason about
            # a spawn that should never have been considered.
            BR_AGENT_ORDER="$(printf '%s\n' "$BR_HOOKS_ROWS" \
                | awk -F'\t' '$1=="PreToolUse" && $2=="Agent" {print $3}' \
                | sed -e 's|.*/scripts/hooks/||' -e 's|[[:space:]].*||' | tr '\n' ' ')"
            # THIS WAS A TYPED SEQUENCE OF EIGHT NAMES AND IT WENT STALE THE DAY
            # A NINTH HOOK WAS REGISTERED. It failed with `want:` and `got:`
            # lines that were identical for eight entries and differed only by a
            # trailing guard-brief-scope.sh — a correctly registered hook
            # reported as a defect, by a list, on behalf of nobody.
            #
            # What was load-bearing in that sequence was never the sequence. It
            # was that the four STRUCTURAL gates come first, in their order,
            # ahead of every policy gate. The tail's order is explicitly NOT
            # load-bearing — the comment beside guard-model-ceiling.sh says so
            # in as many words — so constraining it bought nothing and cost an
            # edit to every future appender.
            #
            # The rule now lives once, in scripts/lib/registered-hooks.sh, and
            # BOTH routes ask it: Layer C for the seated table, this for the
            # plugin table. A TENTH hook appended tomorrow passes here untouched.
            BR_AGENT_NAMES=()
            for _bn in $BR_AGENT_ORDER; do BR_AGENT_NAMES+=("$_bn"); done
            if ! command -v agent_chain_order_violations >/dev/null 2>&1; then
                emit_fail "BR2. the PreToolUse[Agent] chain ORDER was NOT checked: the shared hook-inventory library is missing or unreadable at $_RH_LIB, and this rule is declared there. An order nobody checked is not an order anybody verified."
                BR2_OK=0
            elif [ "${#BR_AGENT_NAMES[@]}" -eq 0 ]; then
                emit_fail "BR2. the plugin hook table registers NOTHING on PreToolUse[Agent]. Every spawn-contract gate this engine has is on that chain, so a spawn would meet none of them."
                BR2_OK=0
            elif ! BR_ORDER_PROBLEMS="$(agent_chain_order_violations "${BR_AGENT_NAMES[@]}")"; then
                while IFS= read -r _v; do
                    [ -n "$_v" ] || continue
                    emit_fail "BR2. PreToolUse[Agent] chain ORDER: $_v"
                done <<BR_ORDER_EOF
$BR_ORDER_PROBLEMS
BR_ORDER_EOF
                BR2_OK=0
            fi

            # --- BR2 REVERSE — nothing wired that the managed set does not name.
            #
            # The loop above walks BR_EXPECTED and asks "is it registered?". For
            # two days nothing asked the other direction, and that is exactly
            # where the drift lived: guard-worktree-removal.sh was wired at
            # 79d6958/084eed3, every forward check stayed green, and the engine
            # carried a guard that no inventory in the system knew about. A
            # seventeenth guard added tomorrow would repeat it.
            #
            # So: every script the plugin table registers must appear in the
            # managed set. Add a guard and forget to declare it here, and the
            # probe now goes red with the guard's name in the message.
            BR2_REGISTERED="$(printf '%s\n' "$BR_HOOKS_ROWS" | awk -F'\t' '{print $3}' \
                | grep -o 'scripts/hooks/[A-Za-z0-9._+-]*\.sh' | sed 's|.*/||' | LC_ALL=C sort -u)"
            BR2_UNDECLARED=""
            while IFS= read -r _reg; do
                [ -n "$_reg" ] || continue
                printf '%s\n' "$BR_EXPECTED" | cut -d'|' -f1 | grep -xF >/dev/null "$_reg" \
                    || BR2_UNDECLARED="$BR2_UNDECLARED $_reg"
            done <<BR_EOF_REV
$BR2_REGISTERED
BR_EOF_REV
            if [ -n "$BR2_UNDECLARED" ]; then
                emit_fail "BR2. plugin hook table registers script(s) the managed set above does not name:$BR2_UNDECLARED. The host WILL load them and nothing in this probe knows they exist — so their event, their order, their presence on disk and their sha256 all go unchecked, and every count in this run understates the engine. Add them to BR_EXPECTED."
                BR2_OK=0
            fi

            # --- BR2 BANNER — the session banner counts THIS set, or it is wrong.
            #
            # engine-status.sh sizes its "N/M guards" fraction from
            # scripts/lib/registered-hooks.sh, reading the same hooks.json. That
            # fraction is the FIRST thing an operator reads every session and the
            # number most likely to be trusted at a glance, and twice now it has
            # been a full fraction over a stale inventory.
            #
            # This check is the reason the library cannot fail quietly. The probe
            # parses hooks.json its own way, above, into BR2_REGISTERED; the
            # library parses it independently; if the two disagree the banner is
            # about to miscount and the probe says so BEFORE an operator reads a
            # wrong number. A shared parser with a shared bug is the one way the
            # derivation could still drift, and this is what forecloses it.
            BR2_INV_LIB="$ENGINE_ROOT/scripts/lib/registered-hooks.sh"
            if [ ! -f "$BR2_INV_LIB" ]; then
                emit_fail "BR2. guard-inventory library MISSING: $BR2_INV_LIB. engine-status.sh derives its guard fraction from it, so every session would open with an unknown count instead of a verified one."
                BR2_OK=0
            else
                # Sourced in a SUBSHELL: the probe must borrow the banner's
                # parser without inheriting its definitions into its own scope.
                # shellcheck source=../lib/registered-hooks.sh disable=SC1090
                BR2_LIB_VIEW="$( . "$BR2_INV_LIB" >/dev/null 2>&1 && registered_hook_scripts "$BR_PLUGIN_HOOKS" 2>/dev/null | LC_ALL=C sort -u )"
                if [ "$BR2_LIB_VIEW" != "$BR2_REGISTERED" ]; then
                    emit_fail "BR2. the session banner's guard inventory DISAGREES with this probe's reading of $BR_PLUGIN_HOOKS. probe sees: $(printf '%s' "$BR2_REGISTERED" | tr '\n' ' ')| banner sees: $(printf '%s' "$BR2_LIB_VIEW" | tr '\n' ' '). Whichever is right, the fraction an operator reads at session start is not the set that will actually load."
                    BR2_OK=0
                fi
            fi

            if [ "$BR2_OK" -eq 1 ]; then
                emit_pass "BR2. all $BR_EXPECTED_COUNT managed guards registered exactly once on the right event and nothing else registered; PreToolUse[Agent] chain leads with its structural gates; session banner's inventory agrees"
            fi
        fi
    fi

    # --- BR3 — path confinement inside ${CLAUDE_PLUGIN_ROOT} ---
    #
    # The seated layers resolve $CLAUDE_PROJECT_DIR and then realpath-compare,
    # because there an adversarial shim at another path is the threat. Here the
    # threat has a different shape and a sharper tell: the plugin root is
    # supplied by the HOST at run time, so any command that names a path some
    # other way has stopped being confined to the plugin. An absolute path is
    # also a portability and privacy defect — this repository is public-bound
    # and an operator's home directory must never appear in it.
    if [ -n "$BR_HOOKS_ROWS" ]; then
        BR3_BAD=""
        while IFS= read -r _row; do
            [ -n "$_row" ] || continue
            _cmd="$(printf '%s' "$_row" | awk -F'\t' '{print $3}')"
            case "$_cmd" in
                *scripts/hooks/*) : ;;
                *) continue ;;   # inline echo hooks carry no path
            esac
            case "$_cmd" in
                *'${CLAUDE_PLUGIN_ROOT}'*|*'$CLAUDE_PLUGIN_ROOT'*) : ;;
                *) BR3_BAD="$BR3_BAD [no \${CLAUDE_PLUGIN_ROOT}: $_cmd]" ;;
            esac
            case "$_cmd" in
                *'$CLAUDE_PROJECT_DIR'*) BR3_BAD="$BR3_BAD [uses \$CLAUDE_PROJECT_DIR (the ENTITY root) for an ENGINE asset: $_cmd]" ;;
            esac
            case "$_cmd" in
                *'/Users/'*|*'/home/'*|*'/root/'*) BR3_BAD="$BR3_BAD [absolute home path: $_cmd]" ;;
            esac
            case "$_cmd" in
                *..*) BR3_BAD="$BR3_BAD [escapes the plugin root with '..': $_cmd]" ;;
            esac
        done <<BR_EOF2
$BR_HOOKS_ROWS
BR_EOF2
        if [ -n "$BR3_BAD" ]; then
            emit_fail "BR3. plugin hook command(s) NOT confined to \${CLAUDE_PLUGIN_ROOT}:$BR3_BAD"
        else
            emit_pass "BR3. every plugin hook command is confined to \${CLAUDE_PLUGIN_ROOT} (no absolute path, no \$CLAUDE_PROJECT_DIR, no '..')"
        fi
    fi

    # --- BR4 — registered scripts exist, are executable, hash-match ---
    BR4_MISSING=""
    BR4_NOEXEC=""
    BR4_MISMATCH=""
    BR4_UNHASHED=""
    while IFS='|' read -r _script _event; do
        [ -n "$_script" ] || continue
        _f="$ENGINE_ROOT/scripts/hooks/$_script"
        if [ ! -f "$_f" ]; then
            BR4_MISSING="$BR4_MISSING $_script"
            continue
        fi
        [ -x "$_f" ] || BR4_NOEXEC="$BR4_NOEXEC $_script"
        _want="$(manifest_hash_of "$_f")"
        if [ -z "$_want" ]; then
            BR4_UNHASHED="$BR4_UNHASHED $_script"
        else
            _live="$(sha256_of "$_f")"
            [ "$_live" = "$_want" ] || BR4_MISMATCH="$BR4_MISMATCH $_script"
        fi
    done <<BR_EOF3
$BR_EXPECTED
BR_EOF3
    BR4_OK=1
    if [ -n "$BR4_MISSING" ]; then
        emit_fail "BR4. registered guard script(s) NOT ON DISK at the plugin root:$BR4_MISSING. The host would register a hook whose command cannot run."
        BR4_OK=0
    fi
    if [ -n "$BR4_NOEXEC" ]; then
        emit_fail "BR4. registered guard script(s) not executable:$BR4_NOEXEC"
        BR4_OK=0
    fi
    if [ -n "$BR4_MISMATCH" ]; then
        emit_fail "BR4. registered guard script(s) MODIFIED since install (sha256 != sidecar):$BR4_MISMATCH. Review the change, then re-run the engine's scripts/hooks/install.sh."
        BR4_OK=0
    fi
    if [ -n "$BR4_UNHASHED" ]; then
        # NOT a green tick with a parenthesis. The sidecars are gitignored,
        # generated artefacts, and the two-root contract makes the engine root
        # read-only to an adopter — so an adopter CANNOT mint them and must not
        # be failed for it. What they get instead is the truth about what is
        # therefore unverified.
        emit_warn "BR4. TAMPER CHECK DID NOT RUN for:$BR4_UNHASHED — no .sha256 sidecar at the engine root. Presence and executability were verified; whether the script still contains the guard it shipped with was NOT. Sidecars are minted by the ENGINE maintainer (scripts/hooks/install.sh, run in the engine's own checkout), because a by-reference engine root is read-only to the repository it governs."
    fi
    if [ "$BR4_OK" -eq 1 ] && [ -z "$BR4_UNHASHED" ]; then
        emit_pass "BR4. all $BR_EXPECTED_COUNT registered guard scripts present, executable and hash-matched to their sidecars"
    elif [ "$BR4_OK" -eq 1 ]; then
        emit_pass "BR4. all $BR_EXPECTED_COUNT registered guard scripts present and executable"
    fi

    # --- BR5 — the declared meta-roles resolve ---
    #
    # `claude plugin details` reports Agents (0) for a manifest that declares
    # agent FILE paths, because its inventory helper readdir()s every declared
    # path and silently drops the ENOTDIR. The roles load correctly — the host's
    # session loader handles files explicitly — but the operator-facing
    # inventory says otherwise, and on 2026-08-28 that reading was taken as
    # proof the roles did not resolve. This layer answers the question the
    # inventory command gets wrong, from the files themselves.
    if [ -n "$BR_PLUGIN_NAME" ]; then
        BR5_OUT="$(python3 -c '
import json, os, sys
manifest_path, engine_root = sys.argv[1:3]
try:
    d = json.load(open(manifest_path, encoding="utf-8"))
except Exception:
    print("ERR unparseable manifest")
    sys.exit(0)
decl = d.get("agents")
if decl is None:
    print("NONE")
    sys.exit(0)
if isinstance(decl, str):
    decl = [decl]
if not isinstance(decl, list) or not decl:
    print("ERR \"agents\" is present but not a non-empty list of paths")
    sys.exit(0)
root = os.path.realpath(engine_root)
names, problems = [], []
for rel in decl:
    if not isinstance(rel, str):
        problems.append("non-string entry %r" % (rel,))
        continue
    full = os.path.realpath(os.path.join(engine_root, rel))
    if not (full == root or full.startswith(root + os.sep)):
        problems.append("%s ESCAPES the plugin root" % rel)
        continue
    if os.path.isdir(full):
        # The host RECURSES into subdirectories of a declared agent directory
        # and namespaces what it finds as <plugin>:<subdir>:<name>. A directory
        # carrying subdirectories therefore ships phantom roles.
        subs = [e for e in os.listdir(full) if os.path.isdir(os.path.join(full, e))]
        if subs:
            problems.append("%s is a directory containing subdirectories (%s) — the host recurses and would register phantom <plugin>:<subdir>:<role> types" % (rel, ",".join(sorted(subs))))
        for e in sorted(os.listdir(full)):
            if e.endswith(".md"):
                names.append(os.path.splitext(e)[0])
        continue
    if not os.path.isfile(full):
        problems.append("%s NOT FOUND at %s" % (rel, full))
        continue
    if not full.endswith(".md"):
        problems.append("%s is not a .md agent definition" % rel)
        continue
    head = ""
    try:
        with open(full, encoding="utf-8", errors="replace") as f:
            head = f.read(4096)
    except Exception as exc:
        problems.append("%s unreadable (%s)" % (rel, exc))
        continue
    if not head.lstrip().startswith("---"):
        problems.append("%s has no YAML frontmatter — the host cannot name it" % rel)
        continue
    name = None
    for line in head.splitlines()[1:]:
        if line.strip() in ("---", "..."):
            break
        if line.startswith("name:"):
            name = line.split(":", 1)[1].strip()
            break
    if not name:
        problems.append("%s has no frontmatter name:" % rel)
        continue
    names.append(name)
if problems:
    print("ERR " + "; ".join(problems))
else:
    print("OK " + ",".join(names))
' "$BR_PLUGIN_MANIFEST" "$ENGINE_ROOT" 2>/dev/null || echo "ERR probe failed")"
        case "$BR5_OUT" in
            OK\ *)
                BR5_NAMES="${BR5_OUT#OK }"
                BR5_N="$(printf '%s' "$BR5_NAMES" | tr ',' '\n' | grep -c . || true)"
                emit_pass "BR5. $BR5_N declared meta-role(s) resolve to real definitions, namespaced ${BR_PLUGIN_NAME}: — $BR5_NAMES"
                ;;
            NONE)
                emit_warn "BR5. the plugin manifest declares no \"agents\" — the host will auto-load an agents/ directory at the plugin root instead, if one exists. Nothing is wrong with that; it is simply not verifiable from the manifest, so the shipped role set is UNVERIFIED here."
                ;;
            *)
                emit_fail "BR5. declared meta-role(s) do NOT resolve: ${BR5_OUT#ERR }"
                ;;
        esac
    fi

    # --- BR6 — will this operator actually load THIS engine? ---
    #
    # THE LAYER THE 2026-08-28 REGRESSION NEEDED. Everything above can be
    # perfect while the host loads nothing, because the registration is
    # operator-local: it lives in ~/.claude, not in any repository. A fresh
    # clone gets zero enforcement until somebody registers the marketplace and
    # enables the plugin, and NOTHING in the repository can tell you that has
    # happened. So the probe follows the whole chain and refuses to assume any
    # link in it.
    BR6_OUT="$(python3 -c '
import json, os, sys
engine_root = os.path.realpath(sys.argv[1])
twin_arg = sys.argv[2] if len(sys.argv) > 2 else ""
engine_twin = os.path.realpath(twin_arg) if twin_arg else None
home = os.path.expanduser("~")
settings = os.path.join(home, ".claude", "settings.json")
known = os.path.join(home, ".claude", "plugins", "known_marketplaces.json")

def load(p):
    try:
        with open(p, encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return None

s = load(settings)
if s is None:
    print("ERR no readable user-scope settings at " + settings)
    sys.exit(0)
enabled = s.get("enabledPlugins") or {}
extra = s.get("extraKnownMarketplaces") or {}
k = load(known) or {}

matches = []
twins = []
reasons = []
for key, on in enabled.items():
    if on is not True or "@" not in key:
        continue
    plugin_name, market = key.rsplit("@", 1)
    loc = None
    entry = k.get(market)
    if isinstance(entry, dict):
        loc = entry.get("installLocation")
        if not loc:
            src = entry.get("source") or {}
            if isinstance(src, dict) and src.get("source") == "directory":
                loc = src.get("path")
    if not loc:
        src = (extra.get(market) or {}).get("source") or {}
        if isinstance(src, dict) and src.get("source") == "directory":
            loc = src.get("path")
    if not loc:
        continue
    mpath = os.path.join(loc, ".claude-plugin", "marketplace.json")
    m = load(mpath)
    if m is None:
        reasons.append("%s: marketplace manifest unreadable at %s" % (key, mpath))
        continue
    for p in m.get("plugins", []) or []:
        if not isinstance(p, dict) or p.get("name") != plugin_name:
            continue
        src = p.get("source")
        if not isinstance(src, str):
            reasons.append("%s: non-directory plugin source, cannot compare to a local engine" % key)
            continue
        resolved = os.path.realpath(os.path.join(loc, src))
        if resolved == engine_root:
            matches.append(key)
        elif engine_twin and resolved == engine_twin:
            twins.append("%s -> %s" % (key, resolved))
        else:
            reasons.append("%s resolves to %s" % (key, resolved))
if matches:
    print("OK " + ",".join(sorted(matches)))
elif twins:
    print("TWIN " + "; ".join(sorted(twins)))
elif reasons:
    print("MISS " + "; ".join(reasons))
else:
    print("NONE")
' "$BR_ENGINE_MAIN" "$BR_ENGINE_TWIN" 2>/dev/null || echo "ERR probe failed")"
    case "$BR6_OUT" in
        OK\ *)
            emit_pass "BR6. this operator's user-scope registration resolves to THIS engine: ${BR6_OUT#OK } -> $BR_ENGINE_MAIN"
            ;;
        TWIN\ *)
            # Not a pass and not a failure: the registration points at the
            # main-checkout twin of the copy under audit. Everything verified
            # above describes the WORKTREE; what the host loads is the twin.
            # Saying "green" here would be the freshness contract's exact sin —
            # attesting to bytes nobody is running.
            emit_warn "BR6. this operator's registration resolves to the MAIN-CHECKOUT TWIN of the audited engine, not to the audited copy: ${BR6_OUT#TWIN }. Every BR layer above describes $BR_ENGINE_MAIN; the host will load $BR_ENGINE_TWIN. Re-run this probe against the twin after landing to attest to what is actually running."
            ;;
        NONE)
            emit_fail "BR6. NO enabled plugin on this machine resolves to this engine ($BR_ENGINE_MAIN). The engine's bytes are on disk and every guard above may be perfect, but the host will not load any of them, so this repository is UNGUARDED. Register and enable it:  claude plugin marketplace add <marketplace root>  then set \"enabledPlugins\": {\"<plugin>@<marketplace>\": true} in ~/.claude/settings.json (USER scope — project/local scope does not work for these two keys)."
            ;;
        MISS\ *)
            emit_fail "BR6. an enabled plugin was found but it does NOT resolve to this engine: ${BR6_OUT#MISS }. Expected $BR_ENGINE_MAIN. The session is loading a DIFFERENT copy of the engine than the one being audited here, so this probe's verdict does not describe what is running."
            ;;
        *)
            emit_fail "BR6. could not read this operator's plugin registration: ${BR6_OUT#ERR }. A probe that cannot confirm the host will load the engine cannot claim enforcement is on."
            ;;
    esac

    # --- BR6b — the entity-facing engine POINTER agrees with the registration ---
    #
    # scripts/locate-engine.sh gives an entity's OWN scripts (an install-fresh
    # pipeline, a freshness verifier, a CI step) a way to find the engine, since
    # they get no $CLAUDE_PLUGIN_ROOT and, by reference, have no relative path to
    # it. Its last-resort candidate is a symlink minted by install.sh, which is a
    # CACHE of the registration BR6 just walked.
    #
    # A cache nobody checks is how an entity script ends up calling a moved or
    # deleted engine while every other layer stays green. So: present and
    # disagreeing is a FAILURE; absent is a NAMED WARNING and never a green tick,
    # because an adopter cannot mint it (the engine root is read-only to the
    # repository it governs) and must not be failed for the engine maintainer's
    # once-per-checkout step.
    BR6_CFG_DIR="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
    BR6_POINTER="$BR6_CFG_DIR/richos-engine"
    if [ -L "$BR6_POINTER" ] && [ ! -e "$BR6_POINTER" ]; then
        # A DANGLING symlink is not the same as an absent one and must not be
        # reported as if it were: it is the residue of an engine that moved or
        # was deleted, and it is exactly the state a stale cache leaves behind.
        emit_fail "BR6b. the engine pointer $BR6_POINTER is a DANGLING symlink -> '$(readlink "$BR6_POINTER" 2>/dev/null)'. The engine it named is gone. Re-run $BR_ENGINE_MAIN/scripts/hooks/install.sh to re-point it, or delete it."
    elif [ ! -e "$BR6_POINTER" ]; then
        emit_warn "BR6b. the entity-facing engine pointer is ABSENT ($BR6_POINTER). Hooks are unaffected — they get \$CLAUDE_PLUGIN_ROOT — but an ENTITY's own scripts that call an engine asset will fall back to walking the registration, and will FAIL LOUD if that is unavailable too. Mint it once per engine checkout:  $BR_ENGINE_MAIN/scripts/hooks/install.sh"
    else
        BR6_POINTER_REAL="$(realpath_of "$BR6_POINTER")"
        if [ ! -d "$BR6_POINTER_REAL/scripts/hooks" ] || [ ! -f "$BR6_POINTER_REAL/VERSION" ]; then
            emit_fail "BR6b. the engine pointer $BR6_POINTER resolves to '$BR6_POINTER_REAL', which is NOT an engine (no scripts/hooks/ + VERSION). An entity script following it would find nothing where it expects the mechanical layer. Re-run $BR_ENGINE_MAIN/scripts/hooks/install.sh."
        elif [ "$BR6_POINTER_REAL" != "$(realpath_of "$BR_ENGINE_MAIN")" ] \
             && [ "$BR6_POINTER_REAL" != "$(realpath_of "$BR_ENGINE_TWIN")" ]; then
            emit_fail "BR6b. the engine pointer DISAGREES with the audited engine: $BR6_POINTER -> $BR6_POINTER_REAL, but this probe audited $BR_ENGINE_MAIN. An entity script would run a different engine's checks than the one verified here — which is the freshness contract's exact failure, one layer out."
        else
            emit_pass "BR6b. the entity-facing engine pointer agrees with the audited engine: $BR6_POINTER -> $BR6_POINTER_REAL"
        fi
    fi

    # --- BR10 — the ENTITY's own critical config keys ---
    #
    # THE GAP THIS CLOSES. Every layer above audits the ENGINE: its manifest,
    # its hook table, its scripts, its registration. None of them audits the one
    # thing the engine cannot supply and the entity must carry itself.
    #
    # Two keys in the entity's own .claude/settings.local.json are as
    # load-bearing as any guard:
    #
    #   env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS = "1"
    #       without it the orchestrator sees and spawns ZERO teammates at the
    #       next session start, WITH NO ERROR SHOWN. A recorded incident.
    #   worktree.baseRef = "head"
    #       the whole worktree-isolation doctrine assumes a teammate's worktree
    #       branches from local HEAD rather than from the remote's default.
    #
    # Unlike the two plugin keys, these DO work at project scope, so they are
    # entity property and travel with the clone. The SEATED layer set has
    # checked them since the incident (Layers I/J); the by-reference set did
    # not, which meant an entity gained plugin-wiring verification and quietly
    # LOST config verification at the moment it adopted. That is the exact
    # shape of drift this whole migration exists to remove, so it is a HARD
    # failure here rather than a warning.
    BR10_SETTINGS="$REPO_ROOT/.claude/settings.local.json"
    if [ ! -r "$BR10_SETTINGS" ]; then
        emit_fail "BR10. the entity has no readable .claude/settings.local.json ($BR10_SETTINGS). Under a by-reference engine the guards are registered by the plugin, but this file still carries the entity's OWN critical config (env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS, worktree.baseRef) and any project-scope hooks it keeps. Its absence is not 'nothing to check'."
    else
        BR10_OUT="$(python3 - "$BR10_SETTINGS" <<'PY' 2>/dev/null || echo "ERR unreadable/unparseable"
import json, sys

try:
    with open(sys.argv[1], encoding="utf-8") as h:
        d = json.load(h)
except Exception as e:
    print("ERR %s" % e)
    raise SystemExit

def get(dotted):
    cur = d
    for k in dotted.split("."):
        if isinstance(cur, dict) and k in cur:
            cur = cur[k]
        else:
            return None
    return cur

bad = []
teams = get("env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS")
if teams != "1":
    bad.append('env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS is %r, expected "1"' % (teams,))
ref = get("worktree.baseRef")
if ref != "head":
    bad.append('worktree.baseRef is %r, expected "head"' % (ref,))
print("OK" if not bad else "BAD " + "; ".join(bad))
PY
)"
        case "$BR10_OUT" in
            OK)
                emit_pass "BR10. the entity's own critical config is intact: env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=\"1\" and worktree.baseRef=\"head\" in $BR10_SETTINGS" ;;
            BAD\ *)
                emit_fail "BR10. the entity's critical config is BROKEN: ${BR10_OUT#BAD }. In $BR10_SETTINGS. A missing AGENT_TEAMS flag makes the orchestrator see ZERO teammates at the next session start with no error shown; a wrong baseRef branches every teammate worktree from the wrong commit. The plugin cannot supply either — they are project-scope keys and they are the entity's own." ;;
            *)
                emit_fail "BR10. could not read the entity's critical config from $BR10_SETTINGS: ${BR10_OUT#ERR }." ;;
        esac
    fi

    # --- BR7 — the marketplace manifest reaches the next clone ---
    #
    # Layer N's argument, one level out. The marketplace manifest is what lets
    # an adopter run `claude plugin marketplace add <repo>` at all. Present on
    # disk but untracked, everything works for the operator who wrote it and
    # nobody else ever gets enforcement — and, as with settings.local.json, the
    # local probe passes the whole way.
    BR7_MARKET_ROOT=""
    # PHYSICAL path, not the pointer. When the engine is loaded by reference the
    # engine root is normally a SYMLINK (~/.claude/richos-engine -> the
    # checkout), and walking up from the link path climbs ~/.claude and then the
    # home directory — never reaching the repository that actually carries the
    # marketplace manifest. The manifest was present, committed and correct, and
    # BR7 reported it missing. BR6b resolves the same pointer two layers up; this
    # one did not, which is how the check that exists to prove an adopter can
    # load the engine went red on the machine where the engine loads fine.
    BR7_DIR="$( (cd "$ENGINE_ROOT" 2>/dev/null && pwd -P) || printf '%s' "$ENGINE_ROOT" )"
    while [ -n "$BR7_DIR" ] && [ "$BR7_DIR" != "/" ]; do
        if [ -f "$BR7_DIR/.claude-plugin/marketplace.json" ]; then
            BR7_MARKET_ROOT="$BR7_DIR"
            break
        fi
        BR7_DIR="$(dirname "$BR7_DIR")"
    done
    if [ -z "$BR7_MARKET_ROOT" ]; then
        emit_fail "BR7. no .claude-plugin/marketplace.json at or above the engine ($ENGINE_ROOT). Without one there is no marketplace to add, so the only way to load this engine is an ad-hoc --plugin-dir flag that no adopter will remember and no session records."
    else
        BR7_MF="$BR7_MARKET_ROOT/.claude-plugin/marketplace.json"
        if ! git -C "$BR7_MARKET_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
            emit_warn "BR7. marketplace manifest present at $BR7_MF but its directory is not a git work tree — cannot verify it reaches the next clone."
        elif git -C "$BR7_MARKET_ROOT" ls-files --error-unmatch ".claude-plugin/marketplace.json" >/dev/null 2>&1; then
            emit_pass "BR7. marketplace manifest is git-tracked at $BR7_MF — a fresh clone can register this engine"
        elif git -C "$BR7_MARKET_ROOT" check-ignore -q ".claude-plugin/marketplace.json" 2>/dev/null; then
            emit_fail "BR7. marketplace manifest at $BR7_MF is UNTRACKED and gitignored. It works on this machine and reaches nobody else — every future clone is unguarded with no error anywhere. Force-add it: git add -f .claude-plugin/marketplace.json"
        else
            emit_warn "BR7. marketplace manifest at $BR7_MF exists but is NOT YET COMMITTED. Until it is, this engine is loadable only on this machine."
        fi
    fi

    # --- BR8 — the announcement, on both channels, with its negative arm ---
    BR8_ADOPTED="$( { RICHOS_ENTITY_ROOT="$REPO_ROOT" bash "$ENGINE_ROOT/scripts/hooks/engine-status.sh" </dev/null; } 2>/dev/null || true)"
    BR8_SANDBOX="$(mktemp -d -t br8-notadopted.XXXXXX)"
    BR8_UNADOPTED="$( { cd "$BR8_SANDBOX" && env -u RICHOS_ENTITY_ROOT -u CLAUDE_PROJECT_DIR bash "$ENGINE_ROOT/scripts/hooks/engine-status.sh" </dev/null; } 2>/dev/null || true)"
    rm -rf "$BR8_SANDBOX"

    BR8_PROBLEMS=""
    case "$BR8_ADOPTED" in
        *'"systemMessage"'*) : ;;
        *) BR8_PROBLEMS="$BR8_PROBLEMS [no systemMessage — the announcement reaches the model and NOT the operator, which is how 'firing silently' and 'not firing' became indistinguishable]" ;;
    esac
    case "$BR8_ADOPTED" in
        *ENFORCEMENT\ ACTIVE*) : ;;
        *) BR8_PROBLEMS="$BR8_PROBLEMS [did not report ENFORCEMENT ACTIVE for $REPO_ROOT]" ;;
    esac
    case "$BR8_ADOPTED" in
        *"$REPO_ROOT"*) : ;;
        *) BR8_PROBLEMS="$BR8_PROBLEMS [did not name the governed repository]" ;;
    esac
    # THE NEGATIVE ARM. Without it every assertion above is satisfied by a hook
    # that hardcodes "ACTIVE" — mutation M9's exact failure, and the reason a
    # wall of green ticks is evidence of nothing on its own.
    case "$BR8_UNADOPTED" in
        *STOOD\ DOWN*) : ;;
        *) BR8_PROBLEMS="$BR8_PROBLEMS [an UNADOPTED directory did not get STOOD DOWN — the status hook is not actually deciding anything, so its ACTIVE means nothing either]" ;;
    esac
    if [ -n "$BR8_PROBLEMS" ]; then
        emit_fail "BR8. engine-status.sh announcement defective:$BR8_PROBLEMS"
    else
        emit_pass "BR8. engine-status.sh reports ENFORCEMENT ACTIVE for $REPO_ROOT on BOTH channels (operator systemMessage + model additionalContext), and reports STOOD DOWN for an unadopted directory"
    fi

    # --- BR9 — a guard actually blocks, by reference ---
    #
    # Paired canaries against a sandbox entity seeded with THIS entity's own
    # orchestration.config, driving the SHIPPED guard from the real engine root.
    # A sandbox rather than the live repository on purpose: the spawn guard
    # appends allowed names to a session name-ledger, and a probe that mutates
    # the thing it audits is not idempotent and not honest. HOME is redirected
    # for the same reason.
    #
    # Three arms, because two would not be enough:
    #   good spawn  -> ALLOWED  (a guard that blocks everything is useless)
    #   bad spawn   -> BLOCKED  (a guard that blocks nothing is absent)
    #   untruthful model token -> BLOCKED, and it can only know that by having
    #   read the ENTITY's roster, which is what "by reference" has to prove.
    #
    # What this does NOT prove: that the LIVE repository's own roster and config
    # produce these verdicts. It proves the shipped guard, driven from the real
    # engine root with an entity root that is not it, still resolves an entity,
    # reads its roster, and decides. BR8 covers the live root resolution.
    #
    # AND THE SANDBOX SESSION GETS A PROCESS TO BE, for the reason Layer MT's
    # canary does. The ALLOWED arm has to reach the end of the guard, and the
    # end of the guard is clause 7: a spawn is REGISTERED or it does not happen.
    # Registration refuses a session whose process identity cannot be read from
    # the operating system (point 12), and that identity is resolved by walking
    # the process ancestry for a process named `claude`. On a workstation the
    # probe is always a descendant of the live session, so the arm passed by
    # accident of where it was run; on a runner nothing in the ancestry is
    # `claude`, the arm was refused with rc=2, and BR9 read that as "a guard
    # that blocks everything". Measured at 2ed41109, same script, same commit:
    # macOS arm 1 rc=0 silent, ubuntu:24.04 arm 1 rc=2 naming point 12.
    # br9-canary-0000 is a synthetic session, so it gets a synthetic process.
    BR9_SB="$(mktemp -d -t br9-canary.XXXXXX)"
    mkdir -p "$BR9_SB/entity/.claude/agents" "$BR9_SB/home"
    printf 'PROTECTED_PATHS=""\nREADONLY_ALLOWLIST="Explore Plan"\nALLOWED_MODELS="opus sonnet haiku"\n' \
        >"$BR9_SB/entity/orchestration.config"
    printf -- '---\nname: probeteammate\nmodel: opus\n---\nA sandbox role that exists only for this canary.\n' \
        >"$BR9_SB/entity/.claude/agents/probeteammate.md"
    BR9_SESS_PID="$(sh -c 'sleep 600 >/dev/null 2>&1 & echo $!')"

    br9_spawn() { # <name> <isolation-json-or-empty>
        printf '{"tool_name":"Agent","cwd":"%s","session_id":"br9-canary-0000","tool_use_id":"br9-canary-tu","tool_input":{"subagent_type":"probeteammate","name":"%s","prompt":"canary"%s}}' \
            "$BR9_SB/entity" "$1" "$2"
    }
    br9_run() { # <payload> -> sets BR9_RC
        set +e
        printf '%s' "$1" | env HOME="$BR9_SB/home" RICHOS_ENTITY_ROOT="$BR9_SB/entity" RICHOS_WORKSPACES_DIR="$BR9_SB/ws" \
            RICHOS_SESSION_PID="$BR9_SESS_PID" \
            bash "$ENGINE_ROOT/scripts/hooks/guard-worktree-isolation.sh" >/dev/null 2>&1
        BR9_RC=$?
        set -e
    }

    BR9_PROBLEMS=""

    # The entity's OWN config still has to be loadable — every guard sources it,
    # and a config with a syntax error takes all of them down at once. Checked
    # here rather than driven into the canary, so a broken config is reported as
    # a broken config instead of surfacing as a mystifying canary failure.
    if [ -f "$CONFIG" ]; then
        set +e
        ( set -e; . "$CONFIG" ) >/dev/null 2>&1
        BR9_CFG_RC=$?
        set -e
        [ "$BR9_CFG_RC" -eq 0 ] || BR9_PROBLEMS="$BR9_PROBLEMS [this entity's orchestration.config does not load cleanly (rc=$BR9_CFG_RC) — every guard sources it, so all of them are affected]"
    else
        BR9_PROBLEMS="$BR9_PROBLEMS [no orchestration.config at $REPO_ROOT — this repository has not adopted the engine, so nothing here is enforcing anything]"
    fi

    br9_run "$(br9_spawn 'probeteammate-opus-canary' ',"isolation":"worktree"')"
    [ "$BR9_RC" -eq 0 ] || BR9_PROBLEMS="$BR9_PROBLEMS [a well-formed isolated spawn was REFUSED (rc=$BR9_RC) — a guard that blocks everything blocks nothing usefully]"

    br9_run "$(br9_spawn 'probeteammate-opus-canary2' '')"
    [ "$BR9_RC" -eq 2 ] || BR9_PROBLEMS="$BR9_PROBLEMS [a spawn with NO isolation was ALLOWED (rc=$BR9_RC) — the worktree-isolation contract is not being enforced]"

    br9_run "$(br9_spawn 'probeteammate-sonnet-canary3' ',"isolation":"worktree"')"
    [ "$BR9_RC" -eq 2 ] || BR9_PROBLEMS="$BR9_PROBLEMS [an UNTRUTHFUL model token was ALLOWED (rc=$BR9_RC) — the guard did not read the entity's roster, so it is not governing this repository]"

    kill "$BR9_SESS_PID" 2>/dev/null || true
    rm -rf "$BR9_SB"
    if [ -n "$BR9_PROBLEMS" ]; then
        emit_fail "BR9. by-reference guard canary FAILED:$BR9_PROBLEMS"
    else
        emit_pass "BR9. the shipped spawn guard, run by reference, ALLOWS a well-formed isolated spawn, BLOCKS an unisolated one, and BLOCKS an untruthful model token read from the entity's own roster"
    fi

    # Layer R is the one mode-independent check: the root-resolution contract
    # is what makes a by-reference engine possible at all.
    run_layer_R
    run_layer_AL
    run_layer_MT
    run_layer_MC

    if [ "$FAIL" -gt 0 ]; then
        cat >&2 <<'BREOF'

By-reference integrity probe FAILED. Most fixes:

  - BR1/BR2 "plugin manifest MISSING" / "hook table MISSING" / "NOT registered"
       -> the engine's .claude-plugin/plugin.json and hooks/hooks.json are the
          plugin route's entire wiring. Restore them from the engine checkout.
  - BR2 "registered Nx"
       -> a guard is wired twice in hooks/hooks.json and will fire twice per
          event. Remove the duplicate.
  - BR3 "NOT confined"
       -> rewrite the command as bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/<x>.sh
  - BR4 "MODIFIED since install"
       -> git diff the named script in the engine checkout; if the change is
          intended, re-run the engine's scripts/hooks/install.sh to re-mint.
  - BR5 "declared meta-role(s) do NOT resolve"
       -> fix the "agents" paths in .claude-plugin/plugin.json. NOTE: `claude
          plugin details` reports Agents (0) for FILE-declared agents even when
          they load correctly — do not "fix" a working manifest to satisfy it.
  - BR6 "NO enabled plugin ... resolves to this engine"
       -> THIS REPOSITORY IS UNGUARDED RIGHT NOW. Register the marketplace and
          enable the plugin at USER scope, then start a new session.
  - BR7 "UNTRACKED and gitignored"
       -> git add -f .claude-plugin/marketplace.json
  - BR8 "no systemMessage" / "did not report ENFORCEMENT ACTIVE"
       -> scripts/hooks/engine-status.sh has stopped announcing. Nothing else
          in the system tells an operator whether enforcement is on.
  - BR9 "was ALLOWED"
       -> the named guard has been gutted into a no-op. git diff it.

BREOF
        exit 2
    fi
    emit_verdict "the PLUGIN route into $REPO_ROOT (engine loaded BY REFERENCE from $ENGINE_ROOT)" \
        "Layers A-Q were NOT run: they audit the entity's own .claude/settings.local.json wiring, which the plugin route does not use. This exit 0 attests to the by-reference route and to nothing about that wiring — for that, run this probe from the engine's own checkout."
    exit 0
fi



# --- Layer A — canonical settings.local.json exists ---
if [ ! -f "$SETTINGS" ]; then
    emit_fail "A. .claude/settings.local.json missing (canonical hook source)"
    cat >&2 <<EOF

The enforcement hooks are wired through .claude/settings.local.json — the
COMMITTED source-of-truth (force-added; see .gitignore). It should always be
present in a valid checkout. If it is missing, your working tree is corrupt:
restore it with

    git checkout -- .claude/settings.local.json

Then re-run this probe.
EOF
    exit 2
fi
emit_pass "A. .claude/settings.local.json present (canonical hook source)"

# --- Layer B+C — extract wired hook commands via Python (json.parse) ---
EXTRACT_PY="$(mktemp -t contract-integrity-extract.XXXXXX.py)"
trap 'rm -f "$EXTRACT_PY"' EXIT
cat >"$EXTRACT_PY" <<'PY'
import json, os, re, sys
with open(sys.argv[1], "r", encoding="utf-8") as f:
    data = json.load(f)
hooks = data.get("hooks", {})

# --- THE DISPATCHER IS EXPANDED INTO THE RULES IT RUNS ---------------------
# scripts/hooks/dispatch-pretooluse.sh is ONE registered command that runs N
# rule modules in one process. Every functional layer below asks "is guard X
# wired on this matcher?" and answers it by comparing a wired command's path
# against the guard's canonical path. Left alone, those layers would report
# seven correctly-enforced guards as NOT WIRED, because their names moved out
# of the registration and into the dispatcher's manifest.
#
# So the expansion happens HERE, once, at the single point where every layer's
# view of the registration is built — not seven times in seven layers. A rule
# named in the manifest produces exactly the row it produced when it was
# registered on its own, so no layer below knows or needs to know that the
# shape changed. A rule the manifest does NOT name produces no row and is
# reported NOT WIRED, which is still the truth.
#
# The manifest is read from the SAME directory as the wired dispatcher, so a
# probe run against some other checkout reads that checkout's manifest.
_DISPATCH_RE = re.compile(r"(\S*scripts/hooks/dispatch-pretooluse\.sh)\s+(\S+)")


def expand(cmd):
    m = _DISPATCH_RE.search(cmd or "")
    if not m:
        return [cmd]
    wired_path, chain_key = m.group(1), m.group(2)
    resolved = wired_path
    for var, val in (("$CLAUDE_PROJECT_DIR", sys.argv[2]),
                     ("${CLAUDE_PROJECT_DIR}", sys.argv[2]),
                     ("$CLAUDE_PLUGIN_ROOT", sys.argv[2]),
                     ("${CLAUDE_PLUGIN_ROOT}", sys.argv[2])):
        resolved = resolved.replace(var, val)
    hooks_dir = os.path.dirname(resolved)
    manifest = os.path.join(hooks_dir, "dispatch-pretooluse.manifest")
    rows = []
    try:
        with open(manifest, encoding="utf-8") as mh:
            for line in mh:
                line = line.strip()
                if not line or line.startswith("#") or "|" not in line:
                    continue
                key, module = line.split("|", 1)
                if key.strip() != chain_key:
                    continue
                rows.append(cmd.replace(m.group(0),
                                        wired_path.replace("dispatch-pretooluse.sh",
                                                           module.strip())))
    except OSError:
        # No manifest: emit the dispatcher row unchanged. Every layer that
        # wants a specific guard then reports it NOT WIRED, which is exactly
        # what a missing manifest means — the dispatcher refuses to run at all.
        return [cmd]
    return rows or [cmd]


pre = hooks.get("PreToolUse", [])
for entry in pre:
    matcher = entry.get("matcher", "")
    for h in entry.get("hooks", []):
        cmd = h.get("command", "")
        # Print TAB-separated rows: matcher\tcommand
        for one in expand(cmd):
            print(f"{matcher}\t{one}")
# Stop is a DIFFERENT EVENT, not a matcher, and it was not extracted here at
# all -- so the two BLOCKING guards that live on it had no functional layer,
# only an entry in the registration inventory. A guard that is registered and
# dead looks exactly like a guard that is registered and working.
for entry in hooks.get("Stop", []):
    for h in entry.get("hooks", []):
        print(f"<event:Stop>\t{h.get('command','')}")
PY

# $2 is the root the seated surface's $CLAUDE_PROJECT_DIR resolves to; the
# extractor needs it only to locate the dispatcher's manifest on disk.
WIRED="$(python3 "$EXTRACT_PY" "$SETTINGS" "$ENGINE_ROOT" 2>/dev/null || true)"
GUARD_CMD="$(printf '%s\n' "$WIRED" | awk -F'\t' '!found && $1 ~ /Write/ && $1 ~ /Edit/ {print $2; found=1}')"

# ALL commands under the Write|Edit matcher (Layer B only cares about the
# FIRST entry above; Layer K below needs the full list since scan-secrets.sh
# is wired as a second hook in the same matcher entry).
WRITE_CMDS=()
while IFS= read -r line; do
    [ -n "$line" ] && WRITE_CMDS+=("$line")
done < <(printf '%s\n' "$WIRED" | awk -F'\t' '$1 ~ /Write/ && $1 ~ /Edit/ {print $2}')

# The PreToolUse[Agent] matcher wires a CHAIN of hooks, in order. It opens with
# four STRUCTURAL gates — isolation and truthful name, booted-definition
# freshness, reader routing, spawn-content — and continues with a policy tail
# whose order is deliberately not load-bearing.
#
# NO ROSTER HERE, ON PURPOSE. This comment used to name four hooks as though
# that were the whole chain; the chain had been eight for weeks. A literal
# inventory in a comment is the same defect as one in an array, minus the
# ability to fail — so the names are DERIVED below (CANONICAL_AGENT_CHAIN) and
# the rule they are judged against is declared once in
# scripts/lib/registered-hooks.sh.
AGENT_CMDS=()
while IFS= read -r line; do
    [ -n "$line" ] && AGENT_CMDS+=("$line")
done < <(printf '%s\n' "$WIRED" | awk -F'\t' '$1=="Agent" {print $2}')

# The PreToolUse[Bash] matcher wires the Bash-write guard (guard-bash-main-
# writes.sh), which auto-DENIES a raw Bash command that writes into a protected
# tree in the main checkout — the cwd-default drift vector the Write/Edit guard
# never sees. Capture every Bash-matcher command for the wiring layer below.
#
# THE NAME IS LOAD-BEARING: this array must NOT be called `BASH_CMDS`. That is
# a RESERVED special variable in bash >= 4.0 — the shell's own command hash
# table, and an ASSOCIATIVE array. `BASH_CMDS=()` does not convert it to an
# indexed array, and `BASH_CMDS+=("$line")` therefore appends under an empty
# key: every element reads back as the empty string. macOS ships bash 3.2,
# which has no such variable, so the collision was invisible on the machine
# this engine was developed on and only surfaced the first time CI ran the
# probe on Linux (bash 5.2, 2026-08-29). The damage was silent and specific:
# Layers O and S found no Bash-matcher command, and reported the Bash
# main-write guard and the worktree-removal guard as NOT WIRED on a checkout
# where both were correctly wired — two hard gates failing for a reason that
# had nothing to do with the property they exist to prove. Fail-closed, so
# nothing was let through; but a guard that cries wolf on every Linux adopter
# is a guard nobody keeps listening to.
BASH_MATCHER_CMDS=()
while IFS= read -r line; do
    [ -n "$line" ] && BASH_MATCHER_CMDS+=("$line")
done < <(printf '%s\n' "$WIRED" | awk -F'\t' '$1=="Bash" {print $2}')

# The Stop EVENT's commands. Marked with the pseudo-matcher `<event:Stop>` by
# the extractor above so they can never be confused with a PreToolUse matcher
# named "Stop" -- an adopter is free to name one that, and a collision here
# would quietly point a functional layer at the wrong hook. The angle brackets
# are not decoration: a matcher is a regex over TOOL NAMES, and no tool is named
# with them. A NUL byte was the first choice and is WRONG, because bash drops
# NUL from a variable without a word -- the marker would have collapsed back to
# "Stop" and rebuilt the collision it exists to prevent.
STOP_EVENT_CMDS=()
while IFS= read -r line; do
    [ -n "$line" ] && STOP_EVENT_CMDS+=("$line")
done < <(printf '%s\n' "$WIRED" | awk -F'\t' '$1=="<event:Stop>" {print $2}')

# Layer B must NOT rely on filename-substring matching. An adversarial shim
# with the right filename at any path on disk, hand-wired into settings.json,
# would pass a filename check while gutting the guard. So Layer B REQUIRES that
# the resolved wired path equals the canonical repo-root path AND that the
# file's content hash matches the committed manifest sidecar. Layer C applies
# the same requirement to EACH entry in the Agent chain.

CANONICAL_GUARD_HOOK="$REPO_ROOT/scripts/hooks/guard-main-checkout-writes.sh"
CANONICAL_SECRETS_HOOK="$REPO_ROOT/scripts/hooks/scan-secrets.sh"
CANONICAL_DIALECT_HOOK="$REPO_ROOT/scripts/hooks/guard-dialect.sh"
CANONICAL_DIALECT_DICT="$REPO_ROOT/scripts/lib/dialect-en-US.dict"
CANONICAL_BASHGUARD_HOOK="$REPO_ROOT/scripts/hooks/guard-bash-main-writes.sh"
CANONICAL_INTERACTIVE_HOOK="$REPO_ROOT/scripts/hooks/guard-interactive-prompt.sh"
CANONICAL_IDLELAND_HOOK="$REPO_ROOT/scripts/hooks/guard-idle-land.sh"
# The wrapper decides NOTHING. It resolves the two roots, reads config, and
# hands the entire verdict to this file — so hashing the .sh and not the .py is
# checking the lock and ignoring the key, for the fourth time in this probe.
CANONICAL_IDLELAND_PY="$REPO_ROOT/scripts/hooks/guard-idle-land.py"
# NOT under scripts/hooks/ — the guard above decides nothing itself; every
# shape it refuses and every fix it names comes out of this file, which is
# why Layer IP hashes it the way Layer T hashes the dialect vocabulary.
CANONICAL_INTERACTIVE_LIB="$REPO_ROOT/scripts/lib/interactive-prompt.py"
CANONICAL_DRIFTGUARD_HOOK="$REPO_ROOT/scripts/hooks/guard-definition-drift.sh"
CANONICAL_DEFSNAPSHOT_HOOK="$REPO_ROOT/scripts/hooks/snapshot-agent-definitions.sh"
# --- THE Agent CHAIN, DERIVED FROM THE REGISTRATION -------------------------
#
# THIS USED TO BE A TYPED ARRAY OF EIGHT PATHS, AND ON 2026-09-14 IT COST A RED
# `main`. guard-brief-scope.sh was registered as the ninth PreToolUse[Agent]
# hook in hooks/hooks.json. Nothing was broken from the author's seat — the hook
# loaded, fired, refused what it was built to refuse. This array still said
# eight, so Layer C reported "hook chain has 9 entries wired, expected 8" in
# scripts/demo.sh's sandbox, whose seated table IS derived from hooks.json.
#
# AND THE REMEDY IT PRINTED WAS DEAD. "run scripts/hooks/install.sh" appears in
# the message below; install.sh validates two config keys, migrates a stale
# settings.json and mints sha256 sidecars, and never writes a hook stanza or
# touches this array. An engineer who did as they were told got the identical
# failure back. A guard whose fix text does not fix it teaches its reader that
# the guard is noise, so the message now names the actual surfaces.
#
# The chain is therefore READ from the plugin registration, which is the file
# that determines what the host loads, through the one parser this engine keeps
# for it (scripts/lib/registered-hooks.sh). That makes this layer a CROSS-
# SURFACE check rather than a tautology: AGENT_CMDS comes from the SEATED table
# (.claude/settings.local.json), the expectation comes from the PLUGIN table
# (hooks/hooks.json), and the two are different files that must agree. The
# defect that has to be caught — a hook registered on one surface and not the
# other — is exactly the shape this comparison sees, and it was live on `main`
# while this array was typed, uncaught by anything in the seated route.
#
# THE ORDER INTENT DID NOT MOVE INTO THE DERIVATION, because it cannot: derived
# from hooks.json and checked against hooks.json it could never fail. It is
# typed, once, as AGENT_CHAIN_STRUCTURAL_PREFIX in that same library, as a
# PREFIX rule rather than a sequence — so a tenth hook appended tomorrow costs
# this layer nothing, while a hook inserted ahead of the structural four still
# fails. The paragraph-per-entry rationale that used to live here is in the
# library, beside the rule it justifies.
PLUGIN_HOOKS_JSON="$ENGINE_ROOT/hooks/hooks.json"
CANONICAL_AGENT_CHAIN=()
AGENT_CHAIN_NAMES=()
AGENT_CHAIN_SOURCE_ERROR=""
if ! command -v registered_agent_chain >/dev/null 2>&1; then
    AGENT_CHAIN_SOURCE_ERROR="the shared hook-inventory library is missing or unreadable at $_RH_LIB"
else
    _chain_out="$(registered_agent_chain "$PLUGIN_HOOKS_JSON" 2>/dev/null)"
    case "$?" in
        0) while IFS= read -r _n; do
               [ -n "$_n" ] || continue
               AGENT_CHAIN_NAMES+=("$_n")
               CANONICAL_AGENT_CHAIN+=("$REPO_ROOT/scripts/hooks/$_n")
           done <<CHAIN_EOF
$_chain_out
CHAIN_EOF
           ;;
        1) AGENT_CHAIN_SOURCE_ERROR="the plugin registration is not at $PLUGIN_HOOKS_JSON" ;;
        *) AGENT_CHAIN_SOURCE_ERROR="$PLUGIN_HOOKS_JSON is unreadable, registers nothing on PreToolUse[Agent], or there is no python3 to parse it with" ;;
    esac
fi

# Resolve settings.json $CLAUDE_PROJECT_DIR placeholder → absolute path.
RESOLVED_GUARD_CMD="${GUARD_CMD//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
RESOLVED_GUARD_CMD="${RESOLVED_GUARD_CMD//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"


# read_settings_local_value <dotted.path> — reads a nested string value out of
# .claude/settings.local.json (e.g. "env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"
# or "worktree.baseRef"). Prints the value if present and a string, else
# prints nothing. Never fails the caller — any parse error is swallowed and
# treated as "absent" (the caller's own check then fails the layer).
read_settings_local_value() {
    local dotted_path="$1"
    [ -f "$SETTINGS_LOCAL" ] || { echo ""; return; }
    python3 -c "
import json, sys
path, = sys.argv[1:2]
try:
    with open(sys.argv[2], 'r', encoding='utf-8') as f:
        data = json.load(f)
except Exception:
    sys.exit(0)
cur = data
for k in path.split('.'):
    if isinstance(cur, dict) and k in cur:
        cur = cur[k]
    else:
        sys.exit(0)
if isinstance(cur, str):
    print(cur)
" "$dotted_path" "$SETTINGS_LOCAL" 2>/dev/null || echo ""
}

# --- Layer B: write-guard hook path-confined + manifest-hash-matched ---
if [ -z "$GUARD_CMD" ]; then
    emit_fail "B. PreToolUse[Write|Edit|MultiEdit|NotebookEdit] write-guard NOT wired in settings.json"
else
    WIRED_PATH="${RESOLVED_GUARD_CMD%% *}"
    WIRED_REAL="$(realpath_of "$WIRED_PATH")"
    CANON_REAL="$(realpath_of "$CANONICAL_GUARD_HOOK")"
    if [ -z "$WIRED_REAL" ] || [ -z "$CANON_REAL" ]; then
        emit_fail "B. write-guard wired path could not be resolved (got: $GUARD_CMD)"
    elif [ "$WIRED_REAL" != "$CANON_REAL" ]; then
        emit_fail "B. write-guard wired path not confined to canonical location (wired: $WIRED_REAL, canonical: $CANON_REAL)"
    else
        WIRED_HASH="$(sha256_of "$WIRED_REAL")"
        MANIFEST_HASH="$(manifest_hash_of "$CANONICAL_GUARD_HOOK")"
        if [ -z "$WIRED_HASH" ]; then
            emit_fail "B. write-guard content hash could not be computed"
        elif [ -z "$MANIFEST_HASH" ]; then
            emit_fail "B. write-guard manifest missing or unreadable: $CANONICAL_GUARD_HOOK.sha256 — run scripts/hooks/install.sh to regenerate, then commit."
        elif [ "$WIRED_HASH" != "$MANIFEST_HASH" ]; then
            emit_fail "B. write-guard content hash mismatch — live hook differs from manifest (tamper or stale manifest). Expected $MANIFEST_HASH, got $WIRED_HASH. Run scripts/hooks/install.sh and review the diff."
        else
            emit_pass "B. write-guard -> $WIRED_PATH (path-confined, manifest-matched)"
        fi
    fi
fi

# --- Layer C: Agent hook CHAIN path-confined + manifest-hash-matched, IN ORDER ---
#
# WHAT THIS LAYER COMPARES, and why it is not a tautology: AGENT_CMDS is the
# SEATED chain, read out of .claude/settings.local.json. CANONICAL_AGENT_CHAIN
# is DERIVED from the PLUGIN registration, hooks/hooks.json. Two different files
# that must agree — a hook registered on one surface and not the other is the
# defect, and on 2026-09-14 it was live on `main` for hours with nothing in the
# seated route able to see it, because this expectation was a typed list of
# eight and the answer it should have given was nine.
#
# AMENDED 2026-09-15, because half of that sentence changed meaning and a
# comment that overstates a check is worse than no comment. The seated file is
# now GENERATED from the plugin table by scripts/hooks/install.sh, so the two
# no longer disagree because somebody forgot a hand edit — the only way they
# can disagree is a registration COMMITTED WITHOUT RUNNING THE GENERATOR, which
# is the same red with a one-command fix. This layer is still not a tautology
# by the rule stated at the top of this file: at check time it reads two
# different committed files and never re-runs the generator. What it is not,
# and never honestly was, is an INDEPENDENT second opinion — a hand-typed copy
# of the same intent was never independent evidence either.
#
# ORDER is load-bearing for the first four and only the first four. That intent
# is typed once, as AGENT_CHAIN_STRUCTURAL_PREFIX in scripts/lib/registered-
# hooks.sh, and applied below through agent_chain_order_violations. The
# per-position rationale lives beside it there.
if [ -n "$AGENT_CHAIN_SOURCE_ERROR" ]; then
    # NEVER a silent zero. An unreadable registration would otherwise derive an
    # EMPTY expectation, and an empty expectation makes every comparison below
    # vacuous while this layer still prints a tick — the absence of a check
    # wearing the costume of the absence of a finding.
    emit_fail "C. the PreToolUse[Agent] chain could not be DERIVED: $AGENT_CHAIN_SOURCE_ERROR. Nothing below was checked — not the count, not the order, not one path, not one hash."
elif [ "${#AGENT_CMDS[@]}" -eq 0 ]; then
    emit_fail "C. PreToolUse[Agent] hook chain NOT wired in settings.json"
elif [ "${#AGENT_CMDS[@]}" -ne "${#CANONICAL_AGENT_CHAIN[@]}" ]; then
    C_CHAIN_NAMES=""
    for _c in "${CANONICAL_AGENT_CHAIN[@]}"; do
        C_CHAIN_NAMES="${C_CHAIN_NAMES}${C_CHAIN_NAMES:+, then }$(basename "$_c")"
    done
    # THE REMEDY NAMES THE TWO SURFACES, because the one that stood here — "run
    # scripts/hooks/install.sh" — could not fix this and never could. install.sh
    # validates two config keys, migrates a stale settings.json and mints sha256
    # sidecars; it does not write a hook stanza. An engineer who followed it got
    # the identical failure back, which is how a guard teaches its reader to
    # route around it.
    emit_fail "C. PreToolUse[Agent] chain MISMATCH between the two registration surfaces: ${#AGENT_CMDS[@]} entries seated in $SETTINGS, ${#CANONICAL_AGENT_CHAIN[@]} registered in $PLUGIN_HOOKS_JSON (${C_CHAIN_NAMES}). Both surfaces register hooks and both must carry the same chain — seat the missing entry in .claude/settings.local.json, in the position it is wired in hooks/hooks.json."
else
    CHAIN_OK=1
    for i in "${!CANONICAL_AGENT_CHAIN[@]}"; do
        CANON_HOOK="${CANONICAL_AGENT_CHAIN[$i]}"
        HOOK_BASENAME="$(basename "$CANON_HOOK")"
        WIRED_RAW_CMD="${AGENT_CMDS[$i]}"
        RESOLVED_CMD_I="${WIRED_RAW_CMD//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
        RESOLVED_CMD_I="${RESOLVED_CMD_I//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
        WIRED_PATH_I="${RESOLVED_CMD_I%% *}"
        WIRED_REAL_I="$(realpath_of "$WIRED_PATH_I")"
        CANON_REAL_I="$(realpath_of "$CANON_HOOK")"
        if [ -z "$WIRED_REAL_I" ] || [ -z "$CANON_REAL_I" ]; then
            emit_fail "C. PreToolUse[Agent] position $((i+1)) (expected $HOOK_BASENAME) wired path could not be resolved (got: $WIRED_RAW_CMD)"
            CHAIN_OK=0
        elif [ "$WIRED_REAL_I" != "$CANON_REAL_I" ]; then
            emit_fail "C. PreToolUse[Agent] position $((i+1)) not confined to canonical $HOOK_BASENAME (wired: $WIRED_REAL_I, canonical: $CANON_REAL_I)"
            CHAIN_OK=0
        else
            WIRED_HASH_I="$(sha256_of "$WIRED_REAL_I")"
            MANIFEST_HASH_I="$(manifest_hash_of "$CANON_HOOK")"
            if [ -z "$WIRED_HASH_I" ]; then
                emit_fail "C. PreToolUse[Agent] $HOOK_BASENAME content hash could not be computed"
                CHAIN_OK=0
            elif [ -z "$MANIFEST_HASH_I" ]; then
                emit_fail "C. PreToolUse[Agent] $HOOK_BASENAME manifest missing or unreadable: $CANON_HOOK.sha256 — run scripts/hooks/install.sh to regenerate, then commit."
                CHAIN_OK=0
            elif [ "$WIRED_HASH_I" != "$MANIFEST_HASH_I" ]; then
                emit_fail "C. PreToolUse[Agent] $HOOK_BASENAME content hash mismatch — live hook differs from manifest."
                CHAIN_OK=0
            fi
        fi
    done

    # THE ORDER INTENT. The loop above proved the seated chain IS the registered
    # chain; this asks whether that chain is in an order anybody meant. The two
    # are different questions and the derivation can only answer the first —
    # hooks.json cannot tell you whether hooks.json is ordered correctly. A
    # PREFIX rule, so a tenth hook appended tomorrow needs no edit anywhere,
    # while a gate inserted ahead of the structural four fails here.
    C_ORDER_PROBLEMS="$(agent_chain_order_violations "${AGENT_CHAIN_NAMES[@]}")" || {
        while IFS= read -r _v; do
            [ -n "$_v" ] || continue
            emit_fail "C. PreToolUse[Agent] chain ORDER: $_v"
        done <<C_ORDER_EOF
$C_ORDER_PROBLEMS
C_ORDER_EOF
        CHAIN_OK=0
    }

    if [ "$CHAIN_OK" -eq 1 ]; then
        # DERIVED, never typed. This tick said "4 hooks" while verifying 5 for
        # exactly as long as it took to read it — a literal inventory inside a
        # green message is the stale-inventory defect this probe exists to
        # remove, wearing a checkmark.
        C_PASS_NAMES=""
        for _c in "${CANONICAL_AGENT_CHAIN[@]}"; do
            C_PASS_NAMES="${C_PASS_NAMES}${C_PASS_NAMES:+, }$(basename "$_c")"
        done
        emit_pass "C. PreToolUse[Agent] chain -> ${C_PASS_NAMES} (seated table matches the registration, path-confined, manifest-matched, structural gates first)"
    fi
fi

# --- Layer D — wired write-guard actually rejects a known-bad input ---
#
# Canary: a Write targeting the first configured protected tree in the main
# checkout. The guard must return exit 2 whenever it is functional.
if [ -n "$GUARD_CMD" ]; then
    GUARD_HOOK_EXE="${RESOLVED_GUARD_CMD%% *}"
    FIRST_PROTECTED=""
    for p in $PROTECTED_PATHS; do FIRST_PROTECTED="$p"; break; done
    if [ -z "$FIRST_PROTECTED" ]; then
        emit_fail "D. cannot run write-guard canary — PROTECTED_PATHS is empty in orchestration.config. Fill it so the guard has trees to protect."
    elif [ ! -x "$GUARD_HOOK_EXE" ]; then
        emit_fail "D. wired write-guard script not found / not executable: $GUARD_HOOK_EXE"
    else
        CANARY_PATH="$REPO_ROOT/$FIRST_PROTECTED/__integrity_canary__.tmp"
        set +e
        printf '{"tool_name":"Write","tool_input":{"file_path":"%s"}}' "$CANARY_PATH" \
            | "$GUARD_HOOK_EXE" >/dev/null 2>&1
        rc=$?
        set -e
        if [ "$rc" -ne 2 ]; then
            emit_fail "D. wired write-guard did NOT block a main-checkout source write (exit=$rc, expected 2)"
        else
            emit_pass "D. wired write-guard rejects main-checkout source writes (exit=2 canary)"
        fi
    fi
fi

# --- Layer E — wired Agent hook chain scripts exist + are executable ---
if [ "${#AGENT_CMDS[@]}" -gt 0 ]; then
    E_OK=1
    for raw_cmd in "${AGENT_CMDS[@]}"; do
        RESOLVED_E="${raw_cmd//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
        RESOLVED_E="${RESOLVED_E//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
        AGENT_HOOK_EXE="${RESOLVED_E%% *}"
        if [ ! -x "$AGENT_HOOK_EXE" ]; then
            emit_fail "E. wired Agent hook script not found / not executable: $AGENT_HOOK_EXE"
            E_OK=0
        fi
    done
    if [ "$E_OK" -eq 1 ]; then
        emit_pass "E. wired Agent hook chain scripts present + executable (${#AGENT_CMDS[@]} entries)"
    fi
fi

# --- Layer F — TeammateIdle handoff hook (WARN-ONLY) ---
#
# The TeammateIdle hook writes the durable idle-events.jsonl signal (log-only).
# It is intentionally NOT a hard gate: a broken idle-log hook must never block
# other work. So this layer SURFACES drift as a warning but never increments
# FAIL.
TIDLE_CMD="$(python3 - "$SETTINGS" <<'PY'
import json, sys
try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        data = json.load(f)
except Exception:
    sys.exit(0)
for entry in data.get("hooks", {}).get("TeammateIdle", []):
    for h in entry.get("hooks", []):
        cmd = h.get("command", "")
        if cmd:
            print(cmd)
            sys.exit(0)
PY
)"
CANONICAL_TIDLE_HOOK="$REPO_ROOT/scripts/hooks/teammate-idle-handoff.sh"
if [ -z "$TIDLE_CMD" ]; then
    emit_warn "F. TeammateIdle hook NOT wired in settings.json (run scripts/hooks/install.sh; handoff logging disabled)"
else
    RESOLVED_TIDLE_CMD="${TIDLE_CMD//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
    RESOLVED_TIDLE_CMD="${RESOLVED_TIDLE_CMD//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
    TIDLE_PATH="${RESOLVED_TIDLE_CMD%% *}"
    TIDLE_REAL="$(realpath_of "$TIDLE_PATH")"
    CANON_TIDLE_REAL="$(realpath_of "$CANONICAL_TIDLE_HOOK")"
    if [ ! -x "$TIDLE_PATH" ]; then
        emit_warn "F. TeammateIdle hook not found / not executable: $TIDLE_PATH"
    elif [ "$TIDLE_REAL" != "$CANON_TIDLE_REAL" ]; then
        emit_warn "F. TeammateIdle hook not confined to canonical location (wired: $TIDLE_REAL)"
    else
        TIDLE_HASH="$(sha256_of "$TIDLE_REAL")"
        TIDLE_MANIFEST="$(manifest_hash_of "$CANONICAL_TIDLE_HOOK")"
        if [ -n "$TIDLE_MANIFEST" ] && [ "$TIDLE_HASH" != "$TIDLE_MANIFEST" ]; then
            emit_warn "F. TeammateIdle hook content hash differs from manifest (run scripts/hooks/install.sh and review the diff)"
        else
            emit_pass "F. TeammateIdle hook wired + present (idle-event logging active)"
        fi
    fi
fi

# --- Layer G — TaskCompleted handoff-log hook (WARN-ONLY) ---
TCOMP_CMD="$(python3 - "$SETTINGS" <<'PY'
import json, sys
try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        data = json.load(f)
except Exception:
    sys.exit(0)
for entry in data.get("hooks", {}).get("TaskCompleted", []):
    for h in entry.get("hooks", []):
        cmd = h.get("command", "")
        if cmd:
            print(cmd)
            sys.exit(0)
PY
)"
CANONICAL_TCOMP_HOOK="$REPO_ROOT/scripts/hooks/task-completed-handoff.sh"
if [ -z "$TCOMP_CMD" ]; then
    emit_warn "G. TaskCompleted hook NOT wired in settings.json (run scripts/hooks/install.sh; completion logging disabled)"
else
    RESOLVED_TCOMP_CMD="${TCOMP_CMD//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
    RESOLVED_TCOMP_CMD="${RESOLVED_TCOMP_CMD//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
    TCOMP_PATH="${RESOLVED_TCOMP_CMD%% *}"
    TCOMP_REAL="$(realpath_of "$TCOMP_PATH")"
    CANON_TCOMP_REAL="$(realpath_of "$CANONICAL_TCOMP_HOOK")"
    if [ ! -x "$TCOMP_PATH" ]; then
        emit_warn "G. TaskCompleted hook not found / not executable: $TCOMP_PATH"
    elif [ "$TCOMP_REAL" != "$CANON_TCOMP_REAL" ]; then
        emit_warn "G. TaskCompleted hook not confined to canonical location (wired: $TCOMP_REAL)"
    else
        TCOMP_HASH="$(sha256_of "$TCOMP_REAL")"
        TCOMP_MANIFEST="$(manifest_hash_of "$CANONICAL_TCOMP_HOOK")"
        if [ -n "$TCOMP_MANIFEST" ] && [ "$TCOMP_HASH" != "$TCOMP_MANIFEST" ]; then
            emit_warn "G. TaskCompleted hook content hash differs from manifest (run scripts/hooks/install.sh and review the diff)"
        else
            emit_pass "G. TaskCompleted hook wired + present (completion logging active)"
        fi
    fi
fi

# --- Layer H — PostToolUse[Agent] detect-nonnative-worktree.sh (WARN-ONLY) ---
TNONNATIVE_CMD="$(python3 - "$SETTINGS" <<'PY'
import json, sys
try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        data = json.load(f)
except Exception:
    sys.exit(0)
for entry in data.get("hooks", {}).get("PostToolUse", []):
    if entry.get("matcher") != "Agent":
        continue
    for h in entry.get("hooks", []):
        cmd = h.get("command", "")
        if cmd:
            print(cmd)
            sys.exit(0)
PY
)"
CANONICAL_TNONNATIVE_HOOK="$REPO_ROOT/scripts/hooks/detect-nonnative-worktree.sh"
if [ -z "$TNONNATIVE_CMD" ]; then
    emit_warn "H. PostToolUse[Agent] detect-nonnative-worktree.sh NOT wired in settings.json (run scripts/hooks/install.sh; detector backstop disabled)"
else
    RESOLVED_TNONNATIVE_CMD="${TNONNATIVE_CMD//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
    RESOLVED_TNONNATIVE_CMD="${RESOLVED_TNONNATIVE_CMD//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
    TNONNATIVE_PATH="${RESOLVED_TNONNATIVE_CMD%% *}"
    TNONNATIVE_REAL="$(realpath_of "$TNONNATIVE_PATH")"
    CANON_TNONNATIVE_REAL="$(realpath_of "$CANONICAL_TNONNATIVE_HOOK")"
    if [ ! -x "$TNONNATIVE_PATH" ]; then
        emit_warn "H. detect-nonnative-worktree.sh not found / not executable: $TNONNATIVE_PATH"
    elif [ "$TNONNATIVE_REAL" != "$CANON_TNONNATIVE_REAL" ]; then
        emit_warn "H. detect-nonnative-worktree.sh not confined to canonical location (wired: $TNONNATIVE_REAL)"
    else
        TNONNATIVE_HASH="$(sha256_of "$TNONNATIVE_REAL")"
        TNONNATIVE_MANIFEST="$(manifest_hash_of "$CANONICAL_TNONNATIVE_HOOK")"
        if [ -n "$TNONNATIVE_MANIFEST" ] && [ "$TNONNATIVE_HASH" != "$TNONNATIVE_MANIFEST" ]; then
            emit_warn "H. detect-nonnative-worktree.sh content hash differs from manifest (run scripts/hooks/install.sh and review the diff)"
        else
            emit_pass "H. PostToolUse[Agent] detect-nonnative-worktree.sh wired + present (worktree-guard detector active)"
        fi
    fi
fi

# --- Layer I — critical config: env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS (HARD) ---
#
# HARD gate, not warn-only: see the header comment for the incident this
# guards against — its absence is otherwise SILENT until the next session.
if [ ! -f "$SETTINGS_LOCAL" ]; then
    emit_fail "I. .claude/settings.local.json missing — cannot verify env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS. Without this key the orchestrator sees/spawns ZERO teammates at the next session start, with NO error shown."
else
    TEAMS_FLAG="$(read_settings_local_value 'env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS')"
    if [ "$TEAMS_FLAG" != "1" ]; then
        emit_fail "I. env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS missing or not \"1\" in .claude/settings.local.json (got: '${TEAMS_FLAG:-<unset>}'). SYMPTOM: the orchestrator will see/spawn ZERO teammates at the NEXT session start, with NO error shown — this has happened before. Restore: \"env\": { \"CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS\": \"1\" } in .claude/settings.local.json, then re-run scripts/hooks/install.sh."
    else
        emit_pass "I. env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=\"1\" present in .claude/settings.local.json"
    fi
fi

# --- Layer J — critical config: worktree.baseRef (HARD) ---
#
# HARD gate: without this, native isolation worktrees stop branching from
# local HEAD and the land sequence's merge-base assumptions silently break.
if [ ! -f "$SETTINGS_LOCAL" ]; then
    emit_fail "J. .claude/settings.local.json missing — cannot verify worktree.baseRef."
else
    BASE_REF="$(read_settings_local_value 'worktree.baseRef')"
    if [ "$BASE_REF" != "head" ]; then
        emit_fail "J. worktree.baseRef missing or not \"head\" in .claude/settings.local.json (got: '${BASE_REF:-<unset>}'). Without this, native isolation worktrees stop branching from local HEAD and the land sequence's merge-base assumptions break. Restore: \"worktree\": { \"baseRef\": \"head\" } in .claude/settings.local.json, then re-run scripts/hooks/install.sh."
    else
        emit_pass "J. worktree.baseRef=\"head\" present in .claude/settings.local.json"
    fi
fi

# --- Layer K: secrets scanner wired + path-confined + manifest-matched +
# functionally rejects a planted secret AND passes clean content (HARD gate) ---
SECRETS_WIRED_CMD=""
for c in "${WRITE_CMDS[@]}"; do
    RESOLVED_C="${c//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
    RESOLVED_C="${RESOLVED_C//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
    WIRED_PATH_C="${RESOLVED_C%% *}"
    if [ "$(realpath_of "$WIRED_PATH_C")" = "$(realpath_of "$CANONICAL_SECRETS_HOOK")" ]; then
        SECRETS_WIRED_CMD="$RESOLVED_C"
        break
    fi
done

if [ -z "$SECRETS_WIRED_CMD" ]; then
    emit_fail "K. PreToolUse[Write|Edit|MultiEdit|NotebookEdit] secrets scanner (scan-secrets.sh) NOT wired in settings.json — run scripts/hooks/install.sh"
else
    SECRETS_HOOK_EXE="${SECRETS_WIRED_CMD%% *}"
    SECRETS_REAL="$(realpath_of "$SECRETS_HOOK_EXE")"
    SECRETS_HASH="$(sha256_of "$SECRETS_REAL")"
    SECRETS_MANIFEST="$(manifest_hash_of "$CANONICAL_SECRETS_HOOK")"
    if [ ! -x "$SECRETS_HOOK_EXE" ]; then
        emit_fail "K. wired secrets-scanner script not found / not executable: $SECRETS_HOOK_EXE"
    elif [ -z "$SECRETS_HASH" ]; then
        emit_fail "K. secrets-scanner content hash could not be computed"
    elif [ -z "$SECRETS_MANIFEST" ]; then
        emit_fail "K. secrets-scanner manifest missing or unreadable: $CANONICAL_SECRETS_HOOK.sha256 — run scripts/hooks/install.sh to regenerate, then commit."
    elif [ "$SECRETS_HASH" != "$SECRETS_MANIFEST" ]; then
        emit_fail "K. secrets-scanner content hash mismatch — live hook differs from manifest (tamper or stale manifest). Run scripts/hooks/install.sh and review the diff."
    else
        # TWO-SIDED CANARY, BECAUSE exit 2 IS AMBIGUOUS — the shape Layer T
        # already ships, arriving here late and at a cost.
        #
        # scan-secrets.sh REFUSES TO START without scripts/lib/resolve-roots.sh,
        # without scripts/lib/seat-jurisdiction.sh, or without python3, and it
        # refuses BY EXITING 2. This layer's canary asserted the scanner exits 2
        # on a planted secret. Same number, opposite meanings — so for as long as
        # the sandbox file lists in scripts/demo.sh and
        # scripts/hooks/contract-integrity.test.sh have existed, this layer
        # reported a working secrets scanner over a scanner that never executed.
        # In the guard whose entire job is to stop a live credential reaching a
        # commit. That is not a hypothetical and it is not a near miss: it was
        # green, in both sandboxes, for months.
        #
        # A negative test that only ever observes the failing case cannot tell
        # "refused because it caught something" from "refused because it never
        # started". So clean content must PASS in the same breath. A dead
        # scanner fails the second half; only a scanner that is READING satisfies
        # both.
        #
        # The clean payload is deliberately dull prose with no assignment
        # syntax, no vendor prefix and no high-entropy token — nothing for either
        # detector class to reach for. If it ever starts tripping the scanner,
        # that is a real finding about the detectors, not a canary to soften.
        #
        # The planted half below is an obvious, high-entropy vendor-shaped key.
        CANARY_SECRET_PATH="$REPO_ROOT/__secret_scan_canary__.tmp"
        CANARY_PAYLOAD="$(python3 -c 'import json,sys; print(json.dumps({"tool_name":"Write","tool_input":{"file_path":sys.argv[1],"content":"AWS_KEY=AKIAABCDEFGHIJKLMNOP"}}))' "$CANARY_SECRET_PATH" 2>/dev/null || true)"
        CANARY_CLEAN_SECRET_PAYLOAD="$(python3 -c 'import json,sys; print(json.dumps({"tool_name":"Write","tool_input":{"file_path":sys.argv[1],"content":"The runbook explains how to restore a backup and names no credentials."}}))' "$CANARY_SECRET_PATH" 2>/dev/null || true)"
        set +e
        printf '%s' "$CANARY_PAYLOAD" | "$SECRETS_HOOK_EXE" >/dev/null 2>&1
        secrets_rc=$?
        printf '%s' "$CANARY_CLEAN_SECRET_PAYLOAD" | "$SECRETS_HOOK_EXE" >/dev/null 2>&1
        secrets_clean_rc=$?
        set -e
        if [ "$secrets_rc" -eq 2 ] && [ "$secrets_clean_rc" -ne 0 ]; then
            emit_fail "K. the secrets scanner refused BOTH a planted secret and content with no secret in it (exit=$secrets_clean_rc on clean content). It is not scanning, it is failing to start — check scripts/lib/seat-jurisdiction.sh, scripts/lib/resolve-roots.sh and python3."
        elif [ "$secrets_rc" -ne 2 ]; then
            emit_fail "K. wired secrets scanner did NOT block a known-bad secret (exit=$secrets_rc, expected 2)"
        else
            emit_pass "K. secrets scanner wired + REJECTS a planted secret and PASSES clean content (path-confined, manifest-matched, two-sided canary)"
        fi
    fi
fi

# --- Layer T: dialect guard wired + path-confined + manifest-matched +
# functionally rejects a British spelling, AND its vocabulary is hashed
# (HARD gate) ---
#
# A HARD gate for Layer K's reason, plus one of its own. The CEO ruled on
# 2026-08-29 that American English is the language of this record; a sweep on
# 2026-08-30 fixed 654 sites and, having no chokepoint behind it, was undone in
# part WITHIN HOURS. This layer is the check that the chokepoint is on.
#
# THE DICTIONARY IS CHECKED TOO, and that half is the point: the guard decides
# nothing itself — every refusal it issues comes out of
# scripts/lib/dialect-en-US.dict. A hash-matched guard over an emptied
# vocabulary is a green tick over an enforcement outage, which is the exact
# "check the lock, ignore the key" failure install.sh's own hashed-file list is
# written about.
DIALECT_WIRED_CMD=""
for c in "${WRITE_CMDS[@]}"; do
    RESOLVED_C="${c//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
    RESOLVED_C="${RESOLVED_C//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
    WIRED_PATH_C="${RESOLVED_C%% *}"
    if [ "$(realpath_of "$WIRED_PATH_C")" = "$(realpath_of "$CANONICAL_DIALECT_HOOK")" ]; then
        DIALECT_WIRED_CMD="$RESOLVED_C"
        break
    fi
done

if [ -z "$DIALECT_WIRED_CMD" ]; then
    emit_fail "T. PreToolUse[Write|Edit|MultiEdit|NotebookEdit] dialect guard (guard-dialect.sh) NOT wired in settings.json — run scripts/hooks/install.sh"
else
    DIALECT_HOOK_EXE="${DIALECT_WIRED_CMD%% *}"
    DIALECT_REAL="$(realpath_of "$DIALECT_HOOK_EXE")"
    DIALECT_HASH="$(sha256_of "$DIALECT_REAL")"
    DIALECT_MANIFEST="$(manifest_hash_of "$CANONICAL_DIALECT_HOOK")"
    DICT_HASH="$(sha256_of "$CANONICAL_DIALECT_DICT" 2>/dev/null || true)"
    DICT_MANIFEST="$(manifest_hash_of "$CANONICAL_DIALECT_DICT" 2>/dev/null || true)"
    if [ ! -x "$DIALECT_HOOK_EXE" ]; then
        emit_fail "T. wired dialect-guard script not found / not executable: $DIALECT_HOOK_EXE"
    elif [ -z "$DIALECT_HASH" ]; then
        emit_fail "T. dialect-guard content hash could not be computed"
    elif [ -z "$DIALECT_MANIFEST" ]; then
        emit_fail "T. dialect-guard manifest missing or unreadable: $CANONICAL_DIALECT_HOOK.sha256 — run scripts/hooks/install.sh to regenerate, then commit."
    elif [ "$DIALECT_HASH" != "$DIALECT_MANIFEST" ]; then
        emit_fail "T. dialect-guard content hash mismatch — live hook differs from manifest (tamper or stale manifest). Run scripts/hooks/install.sh and review the diff."
    elif [ ! -f "$CANONICAL_DIALECT_DICT" ]; then
        emit_fail "T. the dialect vocabulary is MISSING: $CANONICAL_DIALECT_DICT. The guard decides nothing without it."
    elif [ -z "$DICT_MANIFEST" ]; then
        emit_fail "T. dialect vocabulary unhashed: $CANONICAL_DIALECT_DICT.sha256 missing — run scripts/hooks/install.sh to regenerate, then commit."
    elif [ -n "$DICT_HASH" ] && [ "$DICT_HASH" != "$DICT_MANIFEST" ]; then
        emit_fail "T. dialect vocabulary MODIFIED since install: $CANONICAL_DIALECT_DICT (sha256 $DICT_HASH != manifest $DICT_MANIFEST). Every refusal this guard issues comes out of this file — review the change, then re-run scripts/hooks/install.sh."
    else
        # Functional canary: a Write introducing an unambiguous British
        # spelling into a prose file. The wired guard must return exit 2.
        # DIALECT_TARGET is forced for the canary so the layer proves the
        # MECHANISM, not this repository's current configuration — and it is
        # forced through the entity root, exactly as a real session would.
        CANARY_DIALECT_PATH="$REPO_ROOT/__dialect_canary__.md"
        CANARY_DIALECT_PAYLOAD="$(python3 -c 'import json,sys; print(json.dumps({"tool_name":"Write","tool_input":{"file_path":sys.argv[1],"content":"the colour of it is a matter of judgement"}}))' "$CANARY_DIALECT_PATH" 2>/dev/null || true)"
        # AND A SECOND CANARY, CLEAN, BECAUSE exit 2 IS AMBIGUOUS. Every guard
        # in this family refuses to START — missing resolve-roots.sh, missing
        # seat-jurisdiction.sh, no python3 — by exiting 2. A one-sided canary
        # that only asserts "British content -> 2" is therefore satisfied by a
        # hook that is completely dead, and that is not a hypothetical: Layer
        # K's one-sided canary was green in scripts/demo.sh's sandbox over a
        # secrets scanner whose seat-jurisdiction.sh had never been copied
        # there. So clean American content must return 0 in the same breath.
        # Together the pair proves the guard is READING, not merely failing.
        CANARY_CLEAN_PAYLOAD="$(python3 -c 'import json,sys; print(json.dumps({"tool_name":"Write","tool_input":{"file_path":sys.argv[1],"content":"the color of it is a matter of judgment"}}))' "$CANARY_DIALECT_PATH" 2>/dev/null || true)"
        set +e
        printf '%s' "$CANARY_DIALECT_PAYLOAD" | "$DIALECT_HOOK_EXE" >/dev/null 2>&1
        dialect_rc=$?
        printf '%s' "$CANARY_CLEAN_PAYLOAD" | "$DIALECT_HOOK_EXE" >/dev/null 2>&1
        dialect_clean_rc=$?
        set -e
        if [ "$dialect_rc" -eq 2 ] && [ "$dialect_clean_rc" -ne 0 ]; then
            emit_fail "T. the dialect guard refused BOTH a British spelling and clean American prose (exit=$dialect_clean_rc on clean content). It is not enforcing, it is failing to start — check scripts/lib/seat-jurisdiction.sh, scripts/lib/resolve-roots.sh and python3."
        elif [ "$dialect_rc" -ne 2 ]; then
            if [ -z "${DIALECT_TARGET:-}" ]; then
                emit_warn "T. dialect guard wired + manifest-matched + vocabulary hashed, but DIALECT_TARGET is blank in orchestration.config so nothing is enforced here (exit=$dialect_rc). That is a valid configuration — set DIALECT_TARGET=\"en-US\" to turn it on."
            else
                emit_fail "T. wired dialect guard did NOT block a British spelling (exit=$dialect_rc, expected 2) with DIALECT_TARGET=\"$DIALECT_TARGET\""
            fi
        else
            emit_pass "T. dialect guard wired + REJECTS a British spelling and PASSES clean American prose + vocabulary hashed (path-confined, manifest-matched, two-sided canary)"
        fi
    fi
fi

# --- Layer IL: the idle-land gate wired on Stop + path-confined +
# manifest-matched + FUNCTIONALLY REFUSES a turn that completed work and started
# nothing, AND FUNCTIONALLY PASSES a legitimately declared stop (HARD gate) ---
#
# WHY THIS LAYER DID NOT EXIST UNTIL 2026-09-01, WHICH IS THE WHOLE ARGUMENT
# The Stop event carries two BLOCKING guards, and until now the probe knew only
# that they were REGISTERED. Registration is not enforcement. The idle-land gate
# shipped registered, hashed, executable and green on every layer of this probe
# — while standing itself down on 41% of the turns it governs, which nothing
# here could see. A functional layer is the difference between "the guard is
# wired" and "the guard refuses things".
#
# TWO-SIDED, for the reason Layer K and Layer T both carry: exit 2 is
# AMBIGUOUS. Every guard in this family exits non-zero when it cannot start.
# A one-sided canary asserting only "bad turn -> 2" is satisfied by a hook that
# is completely dead — and this gate has the mirror-image hazard too, because it
# FAILS OPEN by design: a one-sided canary asserting only "good turn -> 0" is
# satisfied by a hook that refuses nothing at all, which is precisely the state
# it shipped in. So both halves run, over the same sandbox, in the same breath.
#
# The sandbox is a real git repository with a real merge, because term 1
# confirms a landing BY IDENTITY and a fixture that only claims to have merged
# proves nothing.
CANONICAL_IDLELAND_WIRED=""
for c in "${STOP_EVENT_CMDS[@]}"; do
    RESOLVED_C="${c//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
    RESOLVED_C="${RESOLVED_C//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
    RESOLVED_C="${RESOLVED_C//\$CLAUDE_PLUGIN_ROOT/$REPO_ROOT}"
    RESOLVED_C="${RESOLVED_C//\$\{CLAUDE_PLUGIN_ROOT\}/$REPO_ROOT}"
    # Stop hooks are wired as `bash <path>`, so the path is the LAST word.
    WIRED_PATH_C="${RESOLVED_C##* }"
    if [ "$(realpath_of "$WIRED_PATH_C")" = "$(realpath_of "$CANONICAL_IDLELAND_HOOK")" ]; then
        CANONICAL_IDLELAND_WIRED="$WIRED_PATH_C"
        break
    fi
done

if [ -z "$CANONICAL_IDLELAND_WIRED" ]; then
    emit_fail "IL. Stop[guard-idle-land.sh] NOT wired in settings.json — the gate that refuses a turn which completed work and started nothing is not running. Run scripts/hooks/install.sh."
elif [ ! -x "$CANONICAL_IDLELAND_WIRED" ]; then
    emit_fail "IL. wired idle-land guard not executable: $CANONICAL_IDLELAND_WIRED"
elif [ ! -f "$CANONICAL_IDLELAND_PY" ]; then
    emit_fail "IL. the idle-land ANALYZER is missing: $CANONICAL_IDLELAND_PY. The wrapper decides nothing without it — it would announce itself and pass every turn."
else
    IL_HASH="$(sha256_of "$(realpath_of "$CANONICAL_IDLELAND_WIRED")")"
    IL_MANIFEST="$(manifest_hash_of "$CANONICAL_IDLELAND_HOOK")"
    IL_PY_HASH="$(sha256_of "$CANONICAL_IDLELAND_PY" 2>/dev/null || true)"
    IL_PY_MANIFEST="$(manifest_hash_of "$CANONICAL_IDLELAND_PY" 2>/dev/null || true)"
    if [ -z "$IL_MANIFEST" ]; then
        emit_fail "IL. idle-land manifest missing or unreadable: $CANONICAL_IDLELAND_HOOK.sha256 — run scripts/hooks/install.sh, then commit."
    elif [ "$IL_HASH" != "$IL_MANIFEST" ]; then
        emit_fail "IL. idle-land content hash mismatch — the live hook differs from its manifest (tamper or stale manifest). Run scripts/hooks/install.sh and review the diff."
    elif [ -z "$IL_PY_MANIFEST" ]; then
        emit_fail "IL. the idle-land analyzer is UNHASHED: $CANONICAL_IDLELAND_PY.sha256 missing — run scripts/hooks/install.sh, then commit. Every refusal this gate issues comes out of that file."
    elif [ -n "$IL_PY_HASH" ] && [ "$IL_PY_HASH" != "$IL_PY_MANIFEST" ]; then
        emit_fail "IL. the idle-land analyzer was MODIFIED since install: $CANONICAL_IDLELAND_PY. Review the change, then re-run scripts/hooks/install.sh."
    elif ! command -v python3 >/dev/null 2>&1 || ! command -v git >/dev/null 2>&1; then
        emit_warn "IL. idle-land guard wired + manifest-matched, but the functional canary needs python3 and git and one of them is absent — the MECHANISM was not exercised here."
    else
        IL_S="$(mktemp -d -t idleland-canary.XXXXXX)"
        IL_S="$(cd "$IL_S" && pwd -P)"
        IL_E="$IL_S/entity"
        mkdir -p "$IL_E" "$IL_S/nohooks"
        git -C "$IL_E" init -q -b main . >/dev/null 2>&1 || true
        git -C "$IL_E" config user.email probe@probe.invalid >/dev/null 2>&1 || true
        git -C "$IL_E" config user.name probe >/dev/null 2>&1 || true
        git -C "$IL_E" config core.hooksPath "$IL_S/nohooks" >/dev/null 2>&1 || true
        : > "$IL_E/orchestration.config"
        : > "$IL_E/.ceo-todos"
        printf '# Backlog\n\n## Next\n\n| # | Item | Blocked by |\n|---|---|---|\n| ~~1~~ | ~~**Done thing**~~ | done |\n| 2 | **The unstarted thing** | engine free |\n' > "$IL_E/RICH-TODOs.md"
        git -C "$IL_E" add -A >/dev/null 2>&1 || true
        git -C "$IL_E" commit -qm seed >/dev/null 2>&1 || true
        git -C "$IL_E" checkout -q -b canary-branch >/dev/null 2>&1 || true
        printf 'work\n' > "$IL_E/work.txt"
        git -C "$IL_E" add -A >/dev/null 2>&1 || true
        git -C "$IL_E" commit -qm work >/dev/null 2>&1 || true
        git -C "$IL_E" checkout -q main >/dev/null 2>&1 || true
        git -C "$IL_E" merge -q --no-ff canary-branch -m merged >/dev/null 2>&1 || true

        IL_PID="cafebabe-0000-4000-8000-000000000000"
        python3 - "$IL_S/t.jsonl" "$IL_PID" "$IL_E" <<'ILPY' 2>/dev/null
import json, sys
out, pid, entity = sys.argv[1:4]
rows = [
    {"type": "user", "promptId": pid, "cwd": entity, "promptSource": "user",
     "message": {"content": "land it"}},
    {"type": "assistant", "message": {"content": [
        {"type": "tool_use", "name": "Bash",
         "input": {"command": "cd %s && git merge --no-ff canary-branch -m merged" % entity}}]}},
    {"type": "user", "promptId": pid, "cwd": entity,
     "message": {"content": [{"type": "tool_result", "content": "ok"}]}},
]
with open(out, "w", encoding="utf-8") as fh:
    for r in rows:
        fh.write(json.dumps(r) + "\n")
ILPY
        il_payload() { # <final message>
            python3 - "$IL_S/t.jsonl" "$IL_E" "$IL_PID" "$1" <<'ILPY2' 2>/dev/null
import json, sys
tr, cwd, pid, msg = sys.argv[1:5]
print(json.dumps({"hook_event_name": "Stop", "session_id": "cafebabe",
                  "transcript_path": tr, "cwd": cwd, "prompt_id": pid,
                  "stop_hook_active": False, "last_assistant_message": msg,
                  "background_tasks": []}))
ILPY2
        }
        set +e
        il_payload "Landed and pushed. Nothing else from me tonight." \
            | RICHOS_ENTITY_ROOT="$IL_E" "$CANONICAL_IDLELAND_WIRED" >/dev/null 2>&1
        il_bad_rc=$?
        il_payload "Landed and pushed.

stop-declared: nothing-unblocked — row 2 cannot start until the CEO rules on the protocol shape, and every other row landed today." \
            | RICHOS_ENTITY_ROOT="$IL_E" "$CANONICAL_IDLELAND_WIRED" >/dev/null 2>&1
        il_good_rc=$?
        set -e
        rm -rf "$IL_S"
        if [ "$il_bad_rc" -ne 2 ]; then
            emit_fail "IL. the wired idle-land guard did NOT refuse a turn that merged a branch, started nothing and left an unblocked row (exit=$il_bad_rc, expected 2). Registered but not enforcing — which is exactly the state it shipped in."
        elif [ "$il_good_rc" -ne 0 ]; then
            emit_fail "IL. the wired idle-land guard refused BOTH the bad turn and a legitimately DECLARED stop (exit=$il_good_rc on the declared one). It is not deciding, it is refusing everything — the escape is unreachable and the gate will be unwired within a day."
        else
            emit_pass "IL. idle-land gate wired on Stop + REFUSES a completed-and-started-nothing turn and PASSES a declared stop + analyzer hashed (path-confined, manifest-matched, two-sided canary)"
        fi
    fi
fi

# --- Layer L — PreToolUse[SendMessage] guard-resume-isolation.sh (WARN-ONLY) ---
#
# The resume-guard (scripts/hooks/guard-resume-isolation.sh) is the
# PreToolUse[SendMessage] gate that blocks resuming a completed teammate whose
# worktree was already landed + removed (the spawn-side guard-worktree-isolation
# .sh only fires on Agent SPAWNS, never on resumes). It is intentionally NOT a
# hard gate — a broken resume-guard is an unrelated failure domain from the
# write/spawn/config hard gates above — so drift here is surfaced as a WARNING
# and never increments FAIL. install.sh keeps its sidecar in sync.
SENDMSG_CMD="$(python3 - "$SETTINGS" <<'PY'
import json, sys
try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        data = json.load(f)
except Exception:
    sys.exit(0)
for entry in data.get("hooks", {}).get("PreToolUse", []):
    if entry.get("matcher") != "SendMessage":
        continue
    for h in entry.get("hooks", []):
        cmd = h.get("command", "")
        if cmd:
            print(cmd)
            sys.exit(0)
PY
)"
CANONICAL_SENDMSG_HOOK="$REPO_ROOT/scripts/hooks/guard-resume-isolation.sh"
if [ -z "$SENDMSG_CMD" ]; then
    emit_warn "L. PreToolUse[SendMessage] guard-resume-isolation.sh NOT wired in settings.json (run scripts/hooks/install.sh; resume-guard disabled)"
else
    RESOLVED_SENDMSG_CMD="${SENDMSG_CMD//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
    RESOLVED_SENDMSG_CMD="${RESOLVED_SENDMSG_CMD//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
    SENDMSG_PATH="${RESOLVED_SENDMSG_CMD%% *}"
    SENDMSG_REAL="$(realpath_of "$SENDMSG_PATH")"
    CANON_SENDMSG_REAL="$(realpath_of "$CANONICAL_SENDMSG_HOOK")"
    if [ ! -x "$SENDMSG_PATH" ]; then
        emit_warn "L. guard-resume-isolation.sh not found / not executable: $SENDMSG_PATH"
    elif [ "$SENDMSG_REAL" != "$CANON_SENDMSG_REAL" ]; then
        emit_warn "L. guard-resume-isolation.sh not confined to canonical location (wired: $SENDMSG_REAL)"
    else
        SENDMSG_HASH="$(sha256_of "$SENDMSG_REAL")"
        SENDMSG_MANIFEST="$(manifest_hash_of "$CANONICAL_SENDMSG_HOOK")"
        if [ -n "$SENDMSG_MANIFEST" ] && [ "$SENDMSG_HASH" != "$SENDMSG_MANIFEST" ]; then
            emit_warn "L. guard-resume-isolation.sh content hash differs from manifest (run scripts/hooks/install.sh and review the diff)"
        else
            # Functional canary: a SendMessage to a specific recipient with a
            # session that cannot resolve any team config must fail closed
            # (exit 2). Force an empty teams dir so resolution can't succeed —
            # this proves the guard's block path has teeth, independent of the
            # live machine's actual team state.
            SENDMSG_CANARY_TEAMS="$(mktemp -d -t integrity-resume-canary.XXXXXX)"
            SENDMSG_CANARY_PAYLOAD='{"tool_name":"SendMessage","tool_input":{"to":"ghost-teammate","message":"are you there?"},"session_id":"00000000-no-such-session"}'
            set +e
            printf '%s' "$SENDMSG_CANARY_PAYLOAD" | RESUME_GUARD_TEAMS_DIR="$SENDMSG_CANARY_TEAMS" "$SENDMSG_PATH" >/dev/null 2>&1
            sendmsg_rc=$?
            set -e
            rm -rf "$SENDMSG_CANARY_TEAMS"
            if [ "$sendmsg_rc" -ne 2 ]; then
                emit_warn "L. wired resume-guard did NOT block an unresolvable resume (exit=$sendmsg_rc, expected 2) — resume-guard may be a no-op"
            else
                emit_pass "L. PreToolUse[SendMessage] guard-resume-isolation.sh wired + blocks an unresolvable resume (path-confined, manifest-matched, exit=2 canary)"
            fi
        fi
    fi
fi

# --- Layer M — REGISTRATION UNIQUENESS (single-fire) ---------------------
#
# THE INVISIBLE DEFECT THIS LAYER MAKES VISIBLE: Claude Code reads BOTH
# .claude/settings.json AND .claude/settings.local.json and MERGES their hook
# arrays ADDITIVELY. When the pre-fix install.sh wrote a resolved COPY of the
# hook stanzas into settings.json, every hook was registered TWICE and fired
# TWICE per matching tool event — silently, because the guards that only exit
# 0/2 are idempotent; only the append-loggers (resume-acks.log, and the sibling
# main-checkout/data-contract loggers) showed it, as byte-identical duplicate
# lines. This layer counts each canonical hook's registrations across BOTH
# files and FAILS if any is registered more than once, so the double-fire can
# never again hide. The migration is `scripts/hooks/install.sh` (removes/strips
# hooks from settings.json).
UNIQ_PY="$(python3 - "$SETTINGS" "$SETTINGS_JSON" "$REPO_ROOT" <<'PY'
import json, os, sys

local_path, json_path, repo_root = sys.argv[1:4]

# Canonical hook basenames — every enforcement/log hook install.sh manages.
CANON = [
    "guard-worktree-isolation.sh",
    "guard-definition-drift.sh",
    "snapshot-agent-definitions.sh",
    "verify-agent-prompt.sh",
    "guard-main-checkout-writes.sh",
    "guard-bash-main-writes.sh",
    # The interactive-prompt guard. A BLOCKING Bash-matcher guard, and the one
    # whose double-registration would be read as two separate hazards in one
    # command line — a refusal that names the same missing -P twice invites the
    # reader to conclude the guard is confused, which is how a correct refusal
    # gets argued with.
    "guard-interactive-prompt.sh",
    "scan-secrets.sh",
    # A BLOCKING Write/Edit-matcher guard: registered twice it would print the
    # same dialect refusal twice, which reads as two separate wrong words in one
    # sentence — and would double the cost of the only guard here that runs a
    # word list over every byte of every write.
    "guard-dialect.sh",
    # A BLOCKING Bash-matcher guard: registered twice it would run the CEO-TODOs
    # predicate twice per commit and print its refusal twice, which reads as two
    # separate defects in one record.
    "guard-ceo-todos-commits.sh",
    # Also a BLOCKING Bash-matcher guard, and the one with a cost worth counting:
    # it walks the whole published tree. Registered twice it would walk it twice
    # per commit AND print the same findings twice, which reads as two separate
    # defects in one tree.
    "guard-completeness-commits.sh",
    # Same argument, same event, same cost: registered twice it would refuse a
    # landing twice and print the same stale row as two separate defects.
    "guard-row-currency-commits.sh",
    # Same event, same class, and its cost is a git index read plus a registry
    # parse: registered twice it would name the same unrecorded vendoring twice,
    # which reads as two separate unregistered things in one commit.
    "guard-vendoring-commits.sh",
    # The hook-registration completeness gate. BLOCKING, same event, and its
    # cost is a hooks.json parse plus one `git grep`: registered twice it would
    # derive the same inventories twice and print the same owed places twice,
    # which reads as two separate incomplete registrations in one commit.
    "guard-hook-registration-commits.sh",
    "guard-resume-isolation.sh",
    "detect-nonnative-worktree.sh",
    "teammate-idle-handoff.sh",
    "task-completed-handoff.sh",
    # The Stop gate of the workspace spec. BLOCKING, and registered twice it
    # would print the same pending-work refusal twice, which reads as twice the
    # work. workspace-lifecycle.sh is registered once on EACH of six events by
    # design, so BR2 counts it per event and this list does not name it.
    "guard-workspace-gate.sh",
    # The CI turn gate. BLOCKING, and registered twice it would print the same
    # red-CI refusal twice at one turn-end — which reads as two separate broken
    # repositories, and would double the one cost this gate is careful to keep
    # small: its 2-second network budget, spent twice per turn.
    "guard-ci-turn-gate.sh",
    # The worker-lifecycle emitters. They are append-only loggers, which is
    # exactly the class that MAKES a double-registration visible (byte-identical
    # duplicate lines) — and exactly the class a consumer would then read as two
    # workers where there is one. Counted here so the duplicate is caught before
    # it can be believed.
    "worker-created-handoff.sh",
    "worker-started-handoff.sh",
    "worker-updated-handoff.sh",
    "worker-ended-handoff.sh",
    # The CEO-ask gate and its witness. The gate is BLOCKING: registered twice it
    # would print its refusal twice, which reads as two separate unasked items.
    # The witness is an append-only logger, the class this list already says
    # MAKES a double-registration visible — and here a duplicated line would be
    # read as the CEO having been asked the same question twice.
    "guard-ceo-ask-first.sh",
    "notice-ceo-asks.sh",
    # The cost ceiling. BLOCKING, and registered twice it would print its whole
    # refusal — the ruling, the examples, the ack line — twice per spawn, which
    # reads as two separate rulings and is exactly the noise that gets a guard
    # routed around.
    "guard-model-ceiling.sh",
    # The staging-staleness gate. BLOCKING, and registered twice it would
    # print its whole refusal — the undeployed commit list, the deploy
    # command, the ack line — twice per dispatch, which reads as two
    # separate stale environments.
    "guard-stale-staging.sh",
    # The standing-ownership gate. BLOCKING, and registered twice it would
    # print its whole refusal — the demanded system, its age, the evidence
    # verbatim, the ack line — twice per dispatch, which reads as two
    # separate systems standing and is the noise that gets a gate routed
    # around.
    "guard-owned-state.sh",
]

def load(p):
    try:
        with open(p, "r", encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return None

def commands(data):
    """Yield every hook command string across every event in a settings dict."""
    if not isinstance(data, dict):
        return
    for event, entries in (data.get("hooks") or {}).items():
        if not isinstance(entries, list):
            continue
        for entry in entries:
            for h in (entry.get("hooks") or []):
                cmd = h.get("command", "")
                if cmd:
                    yield cmd

def basename_of(cmd):
    path = cmd.replace("$CLAUDE_PROJECT_DIR", repo_root) \
              .replace("${CLAUDE_PROJECT_DIR}", repo_root)
    path = path.split()[0] if path.split() else path
    return os.path.basename(path)

local = load(local_path)
sjson = load(json_path) if os.path.exists(json_path) else None

# Count registrations per canonical hook across BOTH files, and per file.
counts = {name: 0 for name in CANON}
local_counts = {name: 0 for name in CANON}
json_counts = {name: 0 for name in CANON}
for cmd in commands(local):
    b = basename_of(cmd)
    if b in counts:
        counts[b] += 1
        local_counts[b] += 1
for cmd in commands(sjson):
    b = basename_of(cmd)
    if b in counts:
        counts[b] += 1
        json_counts[b] += 1

# settings.json total hook-command count (any value > 0 is the double-fire root)
sjson_total = sum(1 for _ in commands(sjson)) if sjson is not None else 0

# Emit machine-readable verdict lines.
print(f"sjson_present\t{1 if sjson is not None else 0}")
print(f"sjson_hook_total\t{sjson_total}")
print(f"resume_count\t{counts['guard-resume-isolation.sh']}")
for name in CANON:
    print(f"count\t{name}\t{counts[name]}\t{local_counts[name]}\t{json_counts[name]}")
PY
)"

# Parse the verdict.
SJSON_HOOK_TOTAL="$(printf '%s\n' "$UNIQ_PY" | awk -F'\t' '!found && $1=="sjson_hook_total"{print $2; found=1}')"
RESUME_COUNT="$(printf '%s\n' "$UNIQ_PY" | awk -F'\t' '!found && $1=="resume_count"{print $2; found=1}')"

M_OK=1
# M1 — settings.json must carry NO hook stanzas (the additive double-fire root).
if [ "${SJSON_HOOK_TOTAL:-0}" != "0" ]; then
    emit_fail "M. .claude/settings.json registers $SJSON_HOOK_TOTAL hook(s) — Claude Code MERGES it with settings.local.json, so every duplicated hook fires TWICE. Run scripts/hooks/install.sh to de-duplicate (canonical source is settings.local.json)."
    M_OK=0
fi
# M2 — no canonical hook registered more than once across the merged set.
while IFS=$'\t' read -r _tag _name _total _lc _jc; do
    [ "$_tag" = "count" ] || continue
    if [ "${_total:-0}" -gt 1 ]; then
        emit_fail "M. hook '$_name' registered ${_total}x across settings.local.json(${_lc}) + settings.json(${_jc}) — expected exactly 1 (double-fire). Run scripts/hooks/install.sh."
        M_OK=0
    fi
done < <(printf '%s\n' "$UNIQ_PY")

# M3 — LIVE single-fire canary: drive the resume-guard's append path exactly as
# many times as it is registered in the MERGED settings (post-fix: once) and
# assert the log holds EXACTLY ONE line. A single registration must yield a
# single append; the guard's own consecutive-duplicate dedup (defense-in-depth)
# stays underneath so even a regressed double registration would collapse to one
# line — but M1/M2 would already have failed on that. Warn-only if the sandbox
# can't be built (never blocks device QA on canary plumbing).
CANARY_GUARD="$REPO_ROOT/scripts/hooks/guard-resume-isolation.sh"
if [ "$M_OK" -eq 1 ] && [ -x "$CANARY_GUARD" ] && command -v mktemp >/dev/null 2>&1; then
    FIRES="${RESUME_COUNT:-1}"; [ "$FIRES" -ge 1 ] 2>/dev/null || FIRES=1
    CANARY_DIR="$(mktemp -d -t contract-integrity-canary.XXXXXX 2>/dev/null || true)"
    if [ -n "$CANARY_DIR" ]; then
        mkdir -p "$CANARY_DIR/scripts/hooks" "$CANARY_DIR/scripts/lib" "$CANARY_DIR/.claude" 2>/dev/null || true
        cp "$CANARY_GUARD" "$CANARY_DIR/scripts/hooks/guard-resume-isolation.sh" 2>/dev/null || true
        # The copied guard resolves its library relative to its own location and
        # refuses to start without it, so the canary sandbox has to be a
        # complete-enough engine, and has to DECLARE itself as the governed root
        # (below) rather than letting the guard inherit the probe's own repo.
        cp "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" \
            "$ENGINE_ROOT/scripts/lib/pause_protocol.py" "$CANARY_DIR/scripts/lib/" 2>/dev/null || true
        printf 'SESSION_TEAMS_DIR=""\n' >"$CANARY_DIR/orchestration.config" 2>/dev/null || true
        chmod +x "$CANARY_DIR/scripts/hooks/guard-resume-isolation.sh" 2>/dev/null || true
        CANARY_PAYLOAD='{"tool_name":"SendMessage","tool_input":{"to":"canary-done","message":"resume-ack: single-fire probe canary — no writes"}}'
        i=0
        while [ "$i" -lt "$FIRES" ]; do
            printf '%s' "$CANARY_PAYLOAD" | RICHOS_ENTITY_ROOT="$CANARY_DIR" "$CANARY_DIR/scripts/hooks/guard-resume-isolation.sh" >/dev/null 2>&1 || true
            i=$((i+1))
        done
        CANARY_LINES="$(wc -l < "$CANARY_DIR/.claude/state/resume-acks.log" 2>/dev/null | tr -d ' ' || echo 0)"
        if [ "${CANARY_LINES:-0}" = "1" ]; then
            emit_pass "M. registration uniqueness — each canonical hook wired exactly once; single-fire canary logged 1 line for $FIRES registration(s)"
        else
            emit_fail "M. single-fire canary logged ${CANARY_LINES:-0} lines for $FIRES registration(s), expected 1 — the resume-guard append path is not single-fire."
            M_OK=0
        fi
        rm -rf "$CANARY_DIR" 2>/dev/null || true
    else
        [ "$M_OK" -eq 1 ] && emit_warn "M. registration uniqueness verified, but the SINGLE-FIRE CANARY DID NOT RUN — no sandbox directory could be created. Nothing here proves the resume-guard append path is single-fire; a double-appending guard would not be caught by this run."
    fi
elif [ "$M_OK" -eq 1 ]; then
    emit_warn "M. registration uniqueness verified, but the SINGLE-FIRE CANARY DID NOT RUN — the guard is missing/non-executable, mktemp is unavailable, or a prior M check already failed. Wiring is verified; BEHAVIOR IS NOT."
fi

# --- Layer N — canonical settings.local.json is GIT-TRACKED --------------
#
# See the header comment for the silent global-gitignore stranding trap this
# guards against. Three outcomes inside a git work tree:
#   tracked                       -> PASS  (it reaches the next clone)
#   untracked AND ignore-matched  -> FAIL  (the trap: looks committed, isn't)
#   untracked AND not ignored     -> WARN  (likely just not committed yet)
# Outside a git work tree (or git unavailable) the check cannot apply, so it is
# skipped with a warning rather than failing — the probe's non-git sandboxes and
# any pre-init copy must not hard-fail here.
if command -v git >/dev/null 2>&1 && git -C "$REPO_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    if git -C "$REPO_ROOT" ls-files --error-unmatch -- .claude/settings.local.json >/dev/null 2>&1; then
        emit_pass "N. .claude/settings.local.json is git-tracked (will reach the next clone)"
    elif git -C "$REPO_ROOT" check-ignore -q -- .claude/settings.local.json 2>/dev/null; then
        emit_fail "N. .claude/settings.local.json exists on disk but is NOT git-tracked AND is matched by a gitignore rule (commonly a GLOBAL ~/.config/git/ignore '**/.claude/settings.local.json' entry — vanilla Claude Code treats that file as machine-local). 'git add -A' SILENTLY skipped it, so it looks committed but the NEXT clone/session will NOT receive it — the orchestrator will spawn ZERO teammates with no error. Fix: git add -f .claude/settings.local.json  (then commit)."
    else
        emit_warn "N. .claude/settings.local.json is present but not yet git-tracked (no ignore rule matches it — likely just not committed yet). It is committed BY DESIGN (the sole hook-registration source + the two critical keys); commit it before the next clone or the next session strands with no teammates: git add .claude/settings.local.json"
    fi
else
    emit_warn "N. git-tracked check skipped — $REPO_ROOT is not a git work tree (or git unavailable). In a real adopter repo .claude/settings.local.json MUST be committed; verify with: git ls-files .claude/settings.local.json"
fi

# --- Layer O: Bash-write guard wired + path-confined + manifest-matched +
# functionally denies a main-checkout source write (HARD gate) ---
#
# guard-bash-main-writes.sh (PreToolUse[Bash]) closes the cwd-default drift
# vector: a raw `cd <main> && mkdir <tree>/...` / absolute-path write that the
# Write/Edit guard (Layer B) never sees, and which otherwise surfaces as an
# interactive permission prompt to the human operator. HARD gate for the same
# reason as B/K: an unwired, shimmed, or gutted Bash-guard silently reopens the
# "agent scaffolds into the shared checkout" failure class. Layer O searches the
# full Bash-matcher list so wiring order is irrelevant.
BASHGUARD_WIRED_CMD=""
for c in "${BASH_MATCHER_CMDS[@]}"; do
    RESOLVED_C="${c//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
    RESOLVED_C="${RESOLVED_C//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
    WIRED_PATH_C="${RESOLVED_C%% *}"
    if [ "$(realpath_of "$WIRED_PATH_C")" = "$(realpath_of "$CANONICAL_BASHGUARD_HOOK")" ]; then
        BASHGUARD_WIRED_CMD="$RESOLVED_C"
        break
    fi
done

if [ -z "$BASHGUARD_WIRED_CMD" ]; then
    emit_fail "O. PreToolUse[Bash] main-write guard (guard-bash-main-writes.sh) NOT wired in settings.json — run scripts/hooks/install.sh (and confirm the Bash matcher stanza is present)"
else
    BASHGUARD_HOOK_EXE="${BASHGUARD_WIRED_CMD%% *}"
    BASHGUARD_REAL="$(realpath_of "$BASHGUARD_HOOK_EXE")"
    BASHGUARD_HASH="$(sha256_of "$BASHGUARD_REAL")"
    BASHGUARD_MANIFEST="$(manifest_hash_of "$CANONICAL_BASHGUARD_HOOK")"
    # First protected tree, for the functional canary.
    BASHGUARD_FIRST=""
    for p in $PROTECTED_PATHS; do BASHGUARD_FIRST="$p"; break; done
    if [ ! -x "$BASHGUARD_HOOK_EXE" ]; then
        emit_fail "O. wired Bash-guard script not found / not executable: $BASHGUARD_HOOK_EXE"
    elif [ -z "$BASHGUARD_HASH" ]; then
        emit_fail "O. Bash-guard content hash could not be computed"
    elif [ -z "$BASHGUARD_MANIFEST" ]; then
        emit_fail "O. Bash-guard manifest missing or unreadable: $CANONICAL_BASHGUARD_HOOK.sha256 — run scripts/hooks/install.sh to regenerate, then commit."
    elif [ "$BASHGUARD_HASH" != "$BASHGUARD_MANIFEST" ]; then
        emit_fail "O. Bash-guard content hash mismatch — live hook differs from manifest (tamper or stale manifest). Run scripts/hooks/install.sh and review the diff."
    elif [ -z "$BASHGUARD_FIRST" ]; then
        # No protected trees configured — the guard is intentionally inactive
        # (matches the write-guard's sensible-failure contract). Cannot exercise
        # the deny canary, so surface it as a WARN rather than a false PASS/FAIL.
        emit_warn "O. Bash-guard wired + manifest-matched, but PROTECTED_PATHS is empty in orchestration.config — guard is INACTIVE (deny canary skipped). Fill PROTECTED_PATHS to activate it."
    else
        # Functional canary: a Bash command that writes into the first protected
        # tree via a RELATIVE path while the Bash cwd IS the checkout (the real
        # drift vector). Chosen over an absolute-path canary deliberately: the
        # guard's abs-path branch exempts anything under .claude/worktrees/, and
        # when the probe itself runs from inside an isolated worktree REPO_ROOT
        # already contains that segment — so an abs canary would be self-exempted
        # and give a false PASS. The cwd==ROOT relative branch has no such
        # ambiguity. The wired guard must return exit 2.
        BASHGUARD_CANARY_PAYLOAD="$(python3 -c 'import json,sys; print(json.dumps({"tool_name":"Bash","tool_input":{"command":"mkdir -p "+sys.argv[1]+"/__bashguard_canary__"},"cwd":sys.argv[2]}))' "$BASHGUARD_FIRST" "$REPO_ROOT" 2>/dev/null || true)"
        set +e
        printf '%s' "$BASHGUARD_CANARY_PAYLOAD" | "$BASHGUARD_HOOK_EXE" >/dev/null 2>&1
        bashguard_rc=$?
        set -e
        if [ "$bashguard_rc" -ne 2 ]; then
            emit_fail "O. wired Bash-guard did NOT block a main-checkout '$BASHGUARD_FIRST' source write (exit=$bashguard_rc, expected 2)"
        else
            emit_pass "O. Bash-write guard wired + denies a main-checkout source write (path-confined, manifest-matched, exit=2 canary)"
        fi
    fi
fi

# --- Layer IP: interactive-prompt guard wired + path-confined + manifest-matched
# + REFUSES a command that can wait on a human and PASSES one that cannot, AND
# its shape table is hashed (HARD gate) ---
#
# A HARD gate, because the thing it proves is the thing that failed. At 02:01 on
# 2026-09-01 a macOS password window appeared on the CEO's screen: an agent ran
# `security import D.p12 -k <scratch>/t3.keychain-db -T /usr/bin/codesign` with
# no -P, macOS escalated to SecurityAgent, and the process blocked on a dialog
# nobody had asked for. Forty-one guards were registered and not one of them
# asked whether a command can wait on a human.
#
# THE ANALYZER IS HASHED TOO, and that half is the point, exactly as it is for
# Layer T's vocabulary: guard-interactive-prompt.sh decides nothing itself.
# Every shape it refuses and every fix it names comes out of
# scripts/lib/interactive-prompt.py. A hash-matched hook over a gutted shape
# table is a green tick over an enforcement outage.
#
# TWO-SIDED CANARY, NON-NEGOTIABLE HERE. This guard exits 2 to refuse, and 2
# when it cannot start — missing python3, missing resolve-roots.sh, missing the
# analyzer. Same number, opposite meanings; that ambiguity is what left Layer K
# green over a dead secrets scanner for as long as it had existed. So the
# incident command must return 2 AND the same command carrying its fix must
# return 0, in the same breath. A dead hook fails the second half.
INTERACTIVE_WIRED_CMD=""
for c in "${BASH_MATCHER_CMDS[@]}"; do
    RESOLVED_C="${c//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
    RESOLVED_C="${RESOLVED_C//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
    WIRED_PATH_C="${RESOLVED_C%% *}"
    if [ "$(realpath_of "$WIRED_PATH_C")" = "$(realpath_of "$CANONICAL_INTERACTIVE_HOOK")" ]; then
        INTERACTIVE_WIRED_CMD="$RESOLVED_C"
        break
    fi
done

if [ -z "$INTERACTIVE_WIRED_CMD" ]; then
    emit_fail "IP. PreToolUse[Bash] interactive-prompt guard (guard-interactive-prompt.sh) NOT wired in settings.json — run scripts/hooks/install.sh. Without it nothing asks whether a command can stop and wait for a human."
else
    IP_HOOK_EXE="${INTERACTIVE_WIRED_CMD%% *}"
    IP_REAL="$(realpath_of "$IP_HOOK_EXE")"
    IP_HASH="$(sha256_of "$IP_REAL")"
    IP_MANIFEST="$(manifest_hash_of "$CANONICAL_INTERACTIVE_HOOK")"
    IP_LIB_HASH="$(sha256_of "$CANONICAL_INTERACTIVE_LIB" 2>/dev/null || true)"
    IP_LIB_MANIFEST="$(manifest_hash_of "$CANONICAL_INTERACTIVE_LIB" 2>/dev/null || true)"
    if [ ! -x "$IP_HOOK_EXE" ]; then
        emit_fail "IP. wired interactive-prompt guard not found / not executable: $IP_HOOK_EXE"
    elif [ -z "$IP_HASH" ]; then
        emit_fail "IP. interactive-prompt guard content hash could not be computed"
    elif [ -z "$IP_MANIFEST" ]; then
        emit_fail "IP. interactive-prompt guard manifest missing or unreadable: $CANONICAL_INTERACTIVE_HOOK.sha256 — run scripts/hooks/install.sh to regenerate, then commit."
    elif [ "$IP_HASH" != "$IP_MANIFEST" ]; then
        emit_fail "IP. interactive-prompt guard content hash mismatch — live hook differs from manifest (tamper or stale manifest). Run scripts/hooks/install.sh and review the diff."
    elif [ ! -f "$CANONICAL_INTERACTIVE_LIB" ]; then
        emit_fail "IP. the shape table is MISSING: $CANONICAL_INTERACTIVE_LIB. The guard decides nothing without it."
    elif [ -z "$IP_LIB_MANIFEST" ]; then
        emit_fail "IP. shape table unhashed: $CANONICAL_INTERACTIVE_LIB.sha256 missing — run scripts/hooks/install.sh to regenerate, then commit."
    elif [ -n "$IP_LIB_HASH" ] && [ "$IP_LIB_HASH" != "$IP_LIB_MANIFEST" ]; then
        emit_fail "IP. shape table MODIFIED since install: $CANONICAL_INTERACTIVE_LIB (sha256 $IP_LIB_HASH != manifest $IP_LIB_MANIFEST). Every refusal this guard issues comes out of this file — review the change, then re-run scripts/hooks/install.sh."
    else
        # The incident command itself, verbatim apart from the scratch path.
        IP_CANARY_BLOCK="$(python3 -c 'import json,sys; print(json.dumps({"tool_name":"Bash","cwd":sys.argv[1],"tool_input":{"command":"security import D.p12 -k "+sys.argv[1]+"/t3.keychain-db -T /usr/bin/codesign"}}))' "$REPO_ROOT" 2>/dev/null || true)"
        # THE SAME COMMAND, CARRYING THE FIX THE REFUSAL NAMES. This half proves
        # two things at once that no single-sided canary can: the guard is
        # READING rather than merely failing to start, and the remedy it tells
        # people to use actually works. A guard whose named fix is still refused
        # leaves an author with nowhere to go but a waiver.
        IP_CANARY_CLEAN="$(python3 -c 'import json,sys; print(json.dumps({"tool_name":"Bash","cwd":sys.argv[1],"tool_input":{"command":"security import D.p12 -k "+sys.argv[1]+"/t3.keychain-db -P \"\" -T /usr/bin/codesign"}}))' "$REPO_ROOT" 2>/dev/null || true)"
        set +e
        printf '%s' "$IP_CANARY_BLOCK" | "$IP_HOOK_EXE" >/dev/null 2>&1
        ip_block_rc=$?
        printf '%s' "$IP_CANARY_CLEAN" | "$IP_HOOK_EXE" >/dev/null 2>&1
        ip_clean_rc=$?
        set -e
        if [ "$ip_block_rc" -eq 2 ] && [ "$ip_clean_rc" -ne 0 ]; then
            emit_fail "IP. the interactive-prompt guard refused BOTH the prompting command and the corrected one (exit=$ip_clean_rc on the fixed command). It is not enforcing, it is failing to start — check scripts/lib/interactive-prompt.py, scripts/lib/resolve-roots.sh and python3."
        elif [ "$ip_block_rc" -ne 2 ]; then
            emit_fail "IP. the wired interactive-prompt guard did NOT block \`security import\` with no -P (exit=$ip_block_rc, expected 2). This is the exact command that put a password window on the CEO's screen at 02:01 on 2026-09-01."
        else
            emit_pass "IP. interactive-prompt guard wired + REFUSES \`security import\` with no -P and PASSES the same command with -P '' + shape table hashed (path-confined, manifest-matched, two-sided canary)"
        fi
    fi
fi

# --- Layer P: DEFINITION-DRIFT GUARD PAIR wired exactly once + path-confined +
# manifest-matched + functionally blocks drift AND allows a clean spawn (HARD gate) ---
#
# THE FAILURE THIS PAIR CLOSES (upstream, 2026-08-06, 25 rejected deliverables):
# `.claude/agents/*.md` definitions load ONCE at SESSION START — like hooks — so a
# definition installed/updated mid-session never reaches a newly spawned agent's
# BOOTED prompt. A teammate's definition was upgraded (v2.0 -> v2.1) and the work
# dispatched in that same session; three of four batches drafted under the stale
# v2.0 contract, with nothing anywhere reporting the mismatch.
#
# The pair:
#   SessionStart      -> snapshot-agent-definitions.sh (records the baseline)
#   PreToolUse[Agent] -> guard-definition-drift.sh     (blocks a drifted spawn)
# Either half alone is useless: no snapshot means the guard has nothing to compare
# and fails OPEN by design, so an unwired snapshotter silently disables the guard
# without any block ever being seen. That silence is exactly why this is a HARD
# gate (like B/C/K/O) rather than a warn-only layer (F/G/H/L): a drift guard that
# fails open unnoticed is indistinguishable from no guard at all.
#
# Layer C already hard-checks guard-definition-drift.sh's chain POSITION and hash;
# this layer adds the exactly-once counts, the SessionStart half, and the paired
# functional canaries.
DRIFTGUARD_HITS=0
DRIFTGUARD_WIRED_CMD=""
for c in "${AGENT_CMDS[@]}"; do
    RESOLVED_C="${c//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
    RESOLVED_C="${RESOLVED_C//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
    WIRED_PATH_C="${RESOLVED_C%% *}"
    if [ "$(realpath_of "$WIRED_PATH_C")" = "$(realpath_of "$CANONICAL_DRIFTGUARD_HOOK")" ]; then
        [ -z "$DRIFTGUARD_WIRED_CMD" ] && DRIFTGUARD_WIRED_CMD="$RESOLVED_C"
        DRIFTGUARD_HITS=$((DRIFTGUARD_HITS+1))
    fi
done

# Every SessionStart command (the snapshotter shares the event with the
# knowledge-verification echo and the worktree reaper, so we search the full
# list).
SESSIONSTART_CMDS=()
while IFS= read -r line; do
    [ -n "$line" ] && SESSIONSTART_CMDS+=("$line")
done < <(python3 - "$SETTINGS" <<'PY'
import json, sys
try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        data = json.load(f)
except Exception:
    sys.exit(0)
for entry in data.get("hooks", {}).get("SessionStart", []):
    for h in entry.get("hooks", []):
        cmd = h.get("command", "")
        if cmd:
            print(cmd)
PY
)

DEFSNAPSHOT_HITS=0
DEFSNAPSHOT_WIRED_CMD=""
for c in "${SESSIONSTART_CMDS[@]}"; do
    RESOLVED_C="${c//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
    RESOLVED_C="${RESOLVED_C//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
    WIRED_PATH_C="${RESOLVED_C%% *}"
    if [ "$(realpath_of "$WIRED_PATH_C")" = "$(realpath_of "$CANONICAL_DEFSNAPSHOT_HOOK")" ]; then
        [ -z "$DEFSNAPSHOT_WIRED_CMD" ] && DEFSNAPSHOT_WIRED_CMD="$RESOLVED_C"
        DEFSNAPSHOT_HITS=$((DEFSNAPSHOT_HITS+1))
    fi
done

P_OK=1
# P1 — PreToolUse[Agent] drift guard wired exactly once.
if [ "$DRIFTGUARD_HITS" -eq 0 ]; then
    emit_fail "P. PreToolUse[Agent] definition-drift guard (guard-definition-drift.sh) NOT wired in settings.local.json — a mid-session agent-definition change can again reach a spawn silently (the stale-snapshot failure). Restore: git checkout -- .claude/settings.local.json"
    P_OK=0
elif [ "$DRIFTGUARD_HITS" -gt 1 ]; then
    emit_fail "P. guard-definition-drift.sh wired ${DRIFTGUARD_HITS}x under the Agent matcher — expected exactly 1 (double-fire). Remove the duplicate stanza from .claude/settings.local.json."
    P_OK=0
fi
# P2 — SessionStart snapshotter wired exactly once.
if [ "$DEFSNAPSHOT_HITS" -eq 0 ]; then
    emit_fail "P. SessionStart definition snapshotter (snapshot-agent-definitions.sh) NOT wired in settings.local.json — with no session-start baseline the drift guard fails OPEN and blocks nothing, silently. Restore: git checkout -- .claude/settings.local.json"
    P_OK=0
elif [ "$DEFSNAPSHOT_HITS" -gt 1 ]; then
    emit_fail "P. snapshot-agent-definitions.sh wired ${DEFSNAPSHOT_HITS}x under SessionStart — expected exactly 1 (double-fire). Remove the duplicate stanza from .claude/settings.local.json."
    P_OK=0
fi

# P3 — both scripts present + executable, sidecars current.
for pair in "drift-guard|$CANONICAL_DRIFTGUARD_HOOK" "definition-snapshotter|$CANONICAL_DEFSNAPSHOT_HOOK"; do
    P_LABEL="${pair%%|*}"
    P_HOOK="${pair#*|}"
    if [ ! -x "$P_HOOK" ]; then
        emit_fail "P. $P_LABEL not found / not executable: $P_HOOK"
        P_OK=0
        continue
    fi
    P_HASH="$(sha256_of "$(realpath_of "$P_HOOK")")"
    P_MANIFEST="$(manifest_hash_of "$P_HOOK")"
    if [ -z "$P_HASH" ]; then
        emit_fail "P. $P_LABEL content hash could not be computed"
        P_OK=0
    elif [ -z "$P_MANIFEST" ]; then
        emit_fail "P. $P_LABEL manifest missing or unreadable: $P_HOOK.sha256 — run scripts/hooks/install.sh to regenerate."
        P_OK=0
    elif [ "$P_HASH" != "$P_MANIFEST" ]; then
        emit_fail "P. $P_LABEL content hash mismatch — live hook differs from manifest (tamper or stale manifest). Expected $P_MANIFEST, got $P_HASH. Run scripts/hooks/install.sh and review the diff."
        P_OK=0
    fi
done

# P4 — PAIRED functional canaries in a throwaway sandbox: the guard must BLOCK a
# definition modified since the snapshot (exit 2) AND ALLOW an untouched one
# (exit 0). Both halves are required — a negative test alone passes for the wrong
# reason if the hook is broken into blocking everything, and a positive test alone
# passes if it is gutted into allowing everything. DEFINITION_DRIFT_ROOT pins both
# hooks to the sandbox so this NEVER reads or writes the real .claude/state.
if [ "$P_OK" -eq 1 ] && [ -x "$CANONICAL_DRIFTGUARD_HOOK" ] && [ -x "$CANONICAL_DEFSNAPSHOT_HOOK" ] && command -v mktemp >/dev/null 2>&1; then
    P_DIR="$(mktemp -d -t contract-integrity-driftcanary.XXXXXX 2>/dev/null || true)"
    if [ -n "$P_DIR" ]; then
        mkdir -p "$P_DIR/.claude/agents" "$P_DIR/.claude/state" 2>/dev/null || true
        # DEFINITION_DRIFT_ROOT now feeds the contract's DECLARED-root candidate,
        # and a declared root must be an adopted one — the resolver refuses to
        # substitute a different repository for a root somebody named. So the
        # canary sandbox carries the marker, exactly like a real governed repo.
        printf 'PROTECTED_PATHS="src"\n' >"$P_DIR/orchestration.config" 2>/dev/null || true
        printf 'canary definition v1\n' >"$P_DIR/.claude/agents/canaryagent.md" 2>/dev/null || true
        printf 'stable definition\n'    >"$P_DIR/.claude/agents/stableagent.md" 2>/dev/null || true
        P_SESSION="cafebabe-0000-4000-8000-000000000000"
        DEFINITION_DRIFT_ROOT="$P_DIR" "$CANONICAL_DEFSNAPSHOT_HOOK" --session "$P_SESSION" >/dev/null 2>&1 || true
        # Drift ONE definition only; the other must stay allowable.
        printf 'canary definition v2 — installed mid-session\n' >"$P_DIR/.claude/agents/canaryagent.md" 2>/dev/null || true
        P_PAYLOAD_DRIFT="$(python3 -c 'import json,sys; print(json.dumps({"tool_name":"Agent","tool_input":{"subagent_type":"canaryagent","name":"canary-sonnet-p1","prompt":"integrity probe canary","isolation":"worktree"},"session_id":sys.argv[1]}))' "$P_SESSION" 2>/dev/null || true)"
        P_PAYLOAD_CLEAN="$(python3 -c 'import json,sys; print(json.dumps({"tool_name":"Agent","tool_input":{"subagent_type":"stableagent","name":"stable-sonnet-p1","prompt":"integrity probe canary","isolation":"worktree"},"session_id":sys.argv[1]}))' "$P_SESSION" 2>/dev/null || true)"
        set +e
        printf '%s' "$P_PAYLOAD_DRIFT" | DEFINITION_DRIFT_ROOT="$P_DIR" "$CANONICAL_DRIFTGUARD_HOOK" >/dev/null 2>&1
        p_drift_rc=$?
        printf '%s' "$P_PAYLOAD_CLEAN" | DEFINITION_DRIFT_ROOT="$P_DIR" "$CANONICAL_DRIFTGUARD_HOOK" >/dev/null 2>&1
        p_clean_rc=$?
        set -e
        if [ "$p_drift_rc" -ne 2 ]; then
            emit_fail "P. wired definition-drift guard did NOT block a definition modified since the session-start snapshot (exit=$p_drift_rc, expected 2) — the guard has been gutted."
            P_OK=0
        elif [ "$p_clean_rc" -ne 0 ]; then
            emit_fail "P. definition-drift guard blocked an UNCHANGED definition (exit=$p_clean_rc, expected 0) — it is over-blocking; every spawn would be denied."
            P_OK=0
        else
            emit_pass "P. definition-drift pair wired exactly once (SessionStart snapshotter + PreToolUse[Agent] guard) + blocks drift (exit=2) + allows unchanged (exit=0) — path-confined, manifest-matched"
        fi
        rm -rf "$P_DIR" 2>/dev/null || true
    else
        emit_warn "P. definition-drift pair is wired and hashed, but the BLOCK/ALLOW CANARIES DID NOT RUN — no sandbox directory could be created. Nothing here proves the guard actually blocks a drifted definition or allows a clean one; a gutted guard would not be caught by this run."
    fi
elif [ "$P_OK" -eq 1 ]; then
    emit_warn "P. definition-drift pair is wired and hashed, but the BLOCK/ALLOW CANARIES DID NOT RUN — a half is missing/non-executable, mktemp is unavailable, or a prior P check already failed. Wiring is verified; BEHAVIOR IS NOT."
fi

# --- Layer Q: THE WORKSPACE SPEC is wired, hashed, and does what the page says
# (HARD gate) ---
#
# docs/plans/worktree-spec-2026-09-11.md, the CEO's workspace spec, has two
# events: a workspace is registered when its agent is spawned, and every
# workspace and branch it has is deleted when its work is landed or discarded.
# mega-lander/workspaces.py implements it; workspace-lifecycle.sh records the
# platform's facts on six events; guard-workspace-gate.sh is the Stop half of
# point 5. Until 2026-09-11 this layer proved a SessionStart reaper and a nightly
# reconciler; both are gone, and so are the in-event terminal reclaim, the
# adoption pass and the daily lane. So Layer Q now proves:
#   Q1  workspace-lifecycle.sh is registered exactly once on each of its six
#       events, and on PostToolUse for Agent, TaskStop AND SendMessage; the
#       Stop gate exactly once; and every RETIRED deleter hook is wired nowhere;
#   Q2  the registry, its command, the hook and the gate are present and match
#       their manifests (the registry is the only code that deletes a workspace);
#   Q3  a FUNCTIONAL canary, in a sandbox with HOME and CLAUDE_CONFIG_DIR
#       redirected: a spawned, committed, finished agent refuses the end of the
#       turn; once merged, the same gate lands it, and its workspace and branch
#       are gone; the finished agent is refused every tool;
#   Q7  the retired nightly reconciler is not loaded under launchd (macOS, the
#       operator's own account only; a sandboxed run cannot see it and says so).
Q_OK=1
CANONICAL_WS_LIB="$REPO_ROOT/mega-lander/workspaces.py"
CANONICAL_WS_CLI="$REPO_ROOT/mega-lander/workspaces.sh"
CANONICAL_WS_HOOK="$REPO_ROOT/scripts/hooks/workspace-lifecycle.sh"
CANONICAL_WS_GATE="$REPO_ROOT/scripts/hooks/guard-workspace-gate.sh"

Q_WIRING="$(python3 - "$SETTINGS" <<'PY' 2>/dev/null || true
import json, os, sys
try:
    d = json.load(open(sys.argv[1], encoding="utf-8"))
except Exception:
    print("ERROR\tsettings unparseable"); sys.exit(0)
hooks = d.get("hooks") or {}
life, gate, retired, post_matchers = {}, 0, [], []
RETIRED = ("session-start-reap-worktrees.sh", "terminalize-agent-worktrees.sh", "record-subagent-start.sh",
           "notice-land-disposition.sh", "agent-finished-reap-worktrees.sh")
for event, entries in hooks.items():
    for entry in entries or []:
        for h in entry.get("hooks") or []:
            base = os.path.basename((h.get("command") or "").split()[-1] if (h.get("command") or "").split() else "")
            if base == "workspace-lifecycle.sh":
                life[event] = life.get(event, 0) + 1
                if event == "PostToolUse":
                    post_matchers.append(entry.get("matcher") or "")
            elif base == "guard-workspace-gate.sh" and event == "Stop":
                gate += 1
            elif base in RETIRED:
                retired.append("%s on %s" % (base, event))
for event in ("SessionStart", "SessionEnd", "SubagentStart", "SubagentStop", "TaskCompleted", "PostToolUse"):
    print("LIFE\t%s\t%d" % (event, life.get(event, 0)))
import re
for tool in ("Agent", "TaskStop", "SendMessage"):
    hit = any(re.fullmatch(m, tool) for m in post_matchers if m)
    print("POST\t%s\t%d" % (tool, 1 if hit else 0))
print("GATE\t%d" % gate)
for r in retired:
    print("RETIRED\t%s" % r)
PY
)"
Q_WIRE_PROBLEMS=""
while IFS=$'\t' read -r _k _a _b; do
    case "$_k" in
        LIFE) [ "$_b" = "1" ] || Q_WIRE_PROBLEMS="$Q_WIRE_PROBLEMS [workspace-lifecycle.sh registered ${_b}x on $_a, expected 1]" ;;
        POST) [ "$_b" = "1" ] || Q_WIRE_PROBLEMS="$Q_WIRE_PROBLEMS [workspace-lifecycle.sh does not see PostToolUse[$_a]]" ;;
        GATE) [ "$_a" = "1" ] || Q_WIRE_PROBLEMS="$Q_WIRE_PROBLEMS [guard-workspace-gate.sh registered ${_a}x on Stop, expected 1]" ;;
        RETIRED) Q_WIRE_PROBLEMS="$Q_WIRE_PROBLEMS [a retired deleter is wired: $_a]" ;;
        ERROR) Q_WIRE_PROBLEMS="$Q_WIRE_PROBLEMS [$_a]" ;;
    esac
done <<Q_EOF
$Q_WIRING
Q_EOF
if [ -z "$Q_WIRING" ]; then
    Q_WIRE_PROBLEMS="$Q_WIRE_PROBLEMS [the wiring could not be read from $SETTINGS]"
fi
if [ -n "$Q_WIRE_PROBLEMS" ]; then
    emit_fail "Q. the workspace spec is not wired as it must be:$Q_WIRE_PROBLEMS. Restore: git checkout -- .claude/settings.local.json hooks/hooks.json"
    Q_OK=0
fi

for pair in "registry|$CANONICAL_WS_LIB" "command|$CANONICAL_WS_CLI" "lifecycle hook|$CANONICAL_WS_HOOK" "Stop gate|$CANONICAL_WS_GATE"; do
    Q_LABEL="${pair%%|*}"
    Q_FILE="${pair#*|}"
    if [ ! -f "$Q_FILE" ] || { [ "$Q_LABEL" != "registry" ] && [ ! -x "$Q_FILE" ]; }; then
        emit_fail "Q. $Q_LABEL not found / not executable: $Q_FILE"
        Q_OK=0
        continue
    fi
    Q_HASH="$(sha256_of "$(realpath_of "$Q_FILE")")"
    Q_MANIFEST="$(manifest_hash_of "$Q_FILE")"
    if [ -z "$Q_MANIFEST" ]; then
        emit_fail "Q. $Q_LABEL manifest missing or unreadable: $Q_FILE.sha256 — run scripts/hooks/install.sh to regenerate."
        Q_OK=0
    elif [ "$Q_HASH" != "$Q_MANIFEST" ]; then
        emit_fail "Q. $Q_LABEL content hash mismatch — live file differs from manifest (tamper or stale manifest). Expected $Q_MANIFEST, got $Q_HASH. Run scripts/hooks/install.sh and review the diff — the registry is the only code that deletes a workspace."
        Q_OK=0
    fi
done

if [ "$Q_OK" -eq 1 ] && command -v git >/dev/null 2>&1 && command -v mktemp >/dev/null 2>&1 \
   && command -v python3 >/dev/null 2>&1; then
    Q_DIR="$(cd "$(mktemp -d -t contract-integrity-workspaces.XXXXXX 2>/dev/null)" 2>/dev/null && pwd -P || true)"
    if [ -n "$Q_DIR" ]; then
        Q_PROBLEMS=""
        Q_ENT="$Q_DIR/entity"
        Q_SB=1
        mkdir -p "$Q_ENT" "$Q_DIR/home/.claude" 2>/dev/null || Q_SB=0
        git -C "$Q_ENT" init -q -b main >/dev/null 2>&1 || Q_SB=0
        cp "$REPO_ROOT/orchestration.config" "$Q_ENT/orchestration.config" 2>/dev/null || Q_SB=0
        printf '.claude/\n' >"$Q_ENT/.gitignore"
        git -C "$Q_ENT" add -A >/dev/null 2>&1 || Q_SB=0
        # The operator's global git config (and its commit hooks) must not reach a
        # sandbox: GIT_CONFIG_GLOBAL=/dev/null, as every q_env call below does too.
        GIT_CONFIG_GLOBAL=/dev/null git -C "$Q_ENT" -c user.name=probe -c user.email=probe@example.invalid commit -q -m seed >/dev/null 2>&1 || Q_SB=0
        Q_SESS_PID="$(sh -c 'sleep 600 >/dev/null 2>&1 & echo $!')"
        # The registry is PINNED, never inherited: an inherited RICHOS_WORKSPACES_DIR
        # would put this canary's registrations in a real registry, and its Stop
        # gate would then scan (and land in) that registry's real repositories.
        q_env() { env HOME="$Q_DIR/home" CLAUDE_CONFIG_DIR="$Q_DIR/home/.claude" RICHOS_ENTITY_ROOT="$Q_ENT" \
                      RICHOS_WORKSPACES_DIR="$Q_DIR/ws" \
                      RICHOS_SESSION_PID="$Q_SESS_PID" RICHOS_WORKSPACES_SPAWN_WINDOW=0 SEAL_WAIT_SECONDS=0 \
                      GIT_CONFIG_GLOBAL=/dev/null GIT_AUTHOR_NAME=probe GIT_AUTHOR_EMAIL=probe@example.invalid \
                      GIT_COMMITTER_NAME=probe GIT_COMMITTER_EMAIL=probe@example.invalid "$@"; }
        q_hook() { # <hook> <json> -> Q_RC
            set +e
            printf '%s' "$2" | q_env bash "$ENGINE_ROOT/scripts/hooks/$1" >/dev/null 2>"$Q_DIR/err"
            Q_RC=$?
            set -e
        }
        Q_AID="aprobecanary0001"
        Q_NP="$Q_ENT/.claude/worktrees/agent-$Q_AID"
        if [ "$Q_SB" -eq 1 ]; then
            q_hook workspace-lifecycle.sh "{\"hook_event_name\":\"SessionStart\",\"session_id\":\"probe-ws-0001\",\"cwd\":\"$Q_ENT\"}"
            # POINT 14, AT ITS OWN ORDERING: "the branch a body of work integrates
            # on is RECORDED when that work starts, BEFORE ITS FIRST AGENT IS
            # SPAWNED. Nothing infers it and nothing guesses it." Since round 7
            # (eedfbc7d) the spawn itself REFUSES without that record, so a canary
            # that spawns without recording is not testing the spec — it is
            # testing the operator step it skipped. One command, exactly as Rich
            # runs it, and it is part of what this canary proves: the recording
            # has to work for the spawn to be registered at all.
            q_env python3 "$ENGINE_ROOT/mega-lander/workspaces.py" --entity "$Q_ENT" \
                --session probe-ws-0001 integration --repo "$Q_ENT" --branch main \
                --why "the integrity probe's workspace canary" >/dev/null 2>&1 || Q_SB=0
            q_hook guard-worktree-isolation.sh "{\"hook_event_name\":\"PreToolUse\",\"session_id\":\"probe-ws-0001\",\"tool_use_id\":\"tu-probe\",\"tool_name\":\"Agent\",\"cwd\":\"$Q_ENT\",\"tool_input\":{\"name\":\"zach-opus-probe\",\"subagent_type\":\"zach\",\"isolation\":\"worktree\",\"prompt\":\"probe\"}}"
            [ "$Q_RC" -eq 0 ] || Q_PROBLEMS="$Q_PROBLEMS [a well-formed spawn was not registered: exit $Q_RC: $(head -c 300 "$Q_DIR/err" | tr '\n' ' ')]"
            git -C "$Q_ENT" worktree add -q "$Q_NP" -b "worktree-agent-$Q_AID" >/dev/null 2>&1 || Q_SB=0
            q_hook workspace-lifecycle.sh "{\"hook_event_name\":\"SubagentStart\",\"session_id\":\"probe-ws-0001\",\"agent_id\":\"$Q_AID\",\"agent_type\":\"zach\",\"cwd\":\"$Q_NP\"}"
            q_hook workspace-lifecycle.sh "{\"hook_event_name\":\"PostToolUse\",\"session_id\":\"probe-ws-0001\",\"tool_use_id\":\"tu-probe\",\"tool_name\":\"Agent\",\"cwd\":\"$Q_ENT\",\"tool_input\":{\"name\":\"zach-opus-probe\"},\"tool_response\":{\"agentId\":\"$Q_AID\"}}"
            printf 'work\n' >"$Q_NP/work.txt"
            q_env git -C "$Q_NP" add work.txt >/dev/null 2>&1 && q_env git -C "$Q_NP" commit -q -m work >/dev/null 2>&1 || Q_SB=0
            q_hook workspace-lifecycle.sh "{\"hook_event_name\":\"SubagentStop\",\"session_id\":\"probe-ws-0001\",\"agent_id\":\"$Q_AID\",\"cwd\":\"$Q_ENT\"}"
            q_hook guard-sealed-worktree.sh "{\"hook_event_name\":\"PreToolUse\",\"session_id\":\"probe-ws-0001\",\"agent_id\":\"$Q_AID\",\"agent_type\":\"zach\",\"tool_name\":\"Read\",\"cwd\":\"$Q_ENT\",\"tool_input\":{}}"
            [ "$Q_RC" -eq 2 ] || Q_PROBLEMS="$Q_PROBLEMS [point 9: a finished agent's Read was allowed (exit $Q_RC)]"
            q_hook guard-workspace-gate.sh "{\"hook_event_name\":\"Stop\",\"session_id\":\"probe-ws-0001\",\"cwd\":\"$Q_ENT\"}"
            [ "$Q_RC" -eq 2 ] || Q_PROBLEMS="$Q_PROBLEMS [point 5: the turn was allowed to end with finished work neither landed nor discarded (exit $Q_RC)]"
            q_env git -C "$Q_ENT" merge -q --no-edit "worktree-agent-$Q_AID" >/dev/null 2>&1 || Q_SB=0
            q_hook guard-workspace-gate.sh "{\"hook_event_name\":\"Stop\",\"session_id\":\"probe-ws-0001\",\"cwd\":\"$Q_ENT\"}"
            [ "$Q_RC" -eq 0 ] || Q_PROBLEMS="$Q_PROBLEMS [point 4: merged work was not landed on its own (exit $Q_RC: $(head -c 300 "$Q_DIR/err" | tr '\n' ' '))]"
            [ ! -e "$Q_NP" ] || Q_PROBLEMS="$Q_PROBLEMS [point 4: the landed agent's workspace still exists]"
            git -C "$Q_ENT" rev-parse --verify --quiet "refs/heads/worktree-agent-$Q_AID" >/dev/null 2>&1 \
                && Q_PROBLEMS="$Q_PROBLEMS [point 4: the landed agent's branch still exists]"
        fi
        kill "$Q_SESS_PID" 2>/dev/null || true
        if [ "$Q_SB" -ne 1 ]; then
            # A FAILURE, not a warning: a warning-only path here once concealed a
            # stale fixture for days (contract-integrity.test.sh case 41c), and a
            # canary that cannot run proves nothing about the spec's behavior.
            emit_fail "Q. FUNCTIONAL CANARY DID NOT RUN — a step of the sandbox repository (init, seed commit, worktree add, or the agent's commit) failed. Wiring and hashes are verified; BEHAVIOR IS NOT."
            Q_OK=0
        elif [ -n "$Q_PROBLEMS" ]; then
            emit_fail "Q. the workspace spec's FUNCTIONAL canary failed:$Q_PROBLEMS"
            Q_OK=0
        else
            emit_pass "Q. the workspace spec is wired on its six events and its Stop gate, every retired deleter is wired nowhere, all four files match their manifests, and a sandboxed spawn -> commit -> finish was locked out, held the turn, and — once merged — was landed with its workspace and branch gone"
        fi
        rm -rf "$Q_DIR" 2>/dev/null || true
    else
        emit_warn "Q. FUNCTIONAL CANARY DID NOT RUN — no sandbox directory could be created (mktemp)."
    fi
elif [ "$Q_OK" -eq 1 ]; then
    emit_warn "Q. FUNCTIONAL CANARY DID NOT RUN — git, mktemp or python3 unavailable. Wiring and hashes are verified; BEHAVIOR IS NOT."
fi

# Q7 — the retired nightly deleter is not loaded. Only on macOS, and only for the
# operator's own account and config: a sandboxed probe (a redirected HOME or
# CLAUDE_CONFIG_DIR, as every suite runs it) must not read the operator's
# launchd, and says so rather than passing.
if [ "$(uname -s 2>/dev/null)" = "Darwin" ] && command -v launchctl >/dev/null 2>&1; then
    _q7_home="$(python3 -c 'import os, pwd; print(pwd.getpwuid(os.getuid()).pw_dir)' 2>/dev/null || true)"
    if [ -n "$_q7_home" ] && [ "$( (cd "$HOME" 2>/dev/null && pwd -P) || echo "$HOME")" = "$( (cd "$_q7_home" 2>/dev/null && pwd -P) || echo "$_q7_home")" ] \
       && [ -z "${CLAUDE_CONFIG_DIR:-}" ]; then
        if launchctl print "gui/$(id -u)/com.richos.worktree-reconciler" >/dev/null 2>&1; then
            emit_fail "Q7. the RETIRED nightly reconciler (com.richos.worktree-reconciler) is still loaded under launchd. It deletes worktrees, and the workspace spec's only deleters are land and discard. Run scripts/hooks/install.sh from the engine's main checkout; it unloads and removes it."
        else
            emit_pass "Q7. the retired nightly reconciler is not loaded under launchd"
        fi
    fi
fi

# --- Layer Q6: THE LOCK-OUT FAILS CLOSED (HARD gate) ---
#
# guard-sealed-worktree.sh refuses a FINISHED agent every tool and an
# UNREGISTERED worker every potentially writing tool (docs/plans/worktree-spec-
# 2026-09-11.md, points 3 and 9). This layer proves, on the LIVE engine and a
# sandbox copy with the registry removed, that every condition in which the
# barrier cannot evaluate a worker's call REFUSES a potentially writing tool
# (exit 2), while a proven lead call and a proven read-only worker tool still
# pass. Every arm has its positive control beside it, so a guard that refuses
# everything fails the layer too.
#
# SIDE-EFFECT SAFETY: the registry is pinned inside the sandbox; the entity
# root is a throwaway directory; nothing here reads or writes ~/.claude/state.
Q6_GUARD="$ENGINE_ROOT/scripts/hooks/guard-sealed-worktree.sh"
if [ ! -x "$Q6_GUARD" ]; then
    emit_fail "Q6. the lock-out not found / not executable: $Q6_GUARD — every worker call is ungoverned."
elif ! command -v mktemp >/dev/null 2>&1 || ! command -v python3 >/dev/null 2>&1; then
    emit_warn "Q6. FAIL-CLOSED CANARY DID NOT RUN — mktemp or python3 unavailable. The lock-out's wiring is verified; its fail-closed BEHAVIOR IS NOT."
else
    Q6_DIR="$(cd "$(mktemp -d -t contract-integrity-q6.XXXXXX 2>/dev/null)" 2>/dev/null && pwd -P || true)"
    if [ -z "$Q6_DIR" ]; then
        emit_warn "Q6. FAIL-CLOSED CANARY DID NOT RUN — no sandbox directory could be created (mktemp). BEHAVIOR IS NOT verified."
    else
        mkdir -p "$Q6_DIR/nolib/scripts/hooks" "$Q6_DIR/nolib/scripts/lib" "$Q6_DIR/entity" "$Q6_DIR/ws" "$Q6_DIR/home" "$Q6_DIR/nopy"
        cp "$Q6_GUARD" "$Q6_DIR/nolib/scripts/hooks/" 2>/dev/null || true
        cp "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" "$Q6_DIR/nolib/scripts/lib/" 2>/dev/null || true
        chmod +x "$Q6_DIR/nolib/scripts/hooks/guard-sealed-worktree.sh" 2>/dev/null || true
        printf 'PROTECTED_PATHS=""\nREADONLY_ALLOWLIST="Explore Plan"\n' >"$Q6_DIR/entity/orchestration.config"
        for q6t in bash cat grep sed cut tr head env dirname basename date sleep; do
            q6p="$(command -v "$q6t" 2>/dev/null || true)"; [ -n "$q6p" ] && ln -sf "$q6p" "$Q6_DIR/nopy/$q6t"
        done
        q6_run() { # <engine-dir> <payload> [path] -> Q6_RC
            set +e
            if [ -n "${3:-}" ]; then
                printf '%s' "$2" | env HOME="$Q6_DIR/home" RICHOS_ENTITY_ROOT="$Q6_DIR/entity" RICHOS_WORKSPACES_DIR="$Q6_DIR/ws" SEAL_WAIT_SECONDS=0 PATH="$3" bash "$1/scripts/hooks/guard-sealed-worktree.sh" >/dev/null 2>&1
            else
                printf '%s' "$2" | env HOME="$Q6_DIR/home" RICHOS_ENTITY_ROOT="$Q6_DIR/entity" RICHOS_WORKSPACES_DIR="$Q6_DIR/ws" SEAL_WAIT_SECONDS=0 bash "$1/scripts/hooks/guard-sealed-worktree.sh" >/dev/null 2>&1
            fi
            Q6_RC=$?
            set -e
        }
        Q6_WW='{"session_id":"q6canary-0000","agent_id":"q6canary00000001","agent_type":"dev","tool_name":"Write","tool_input":{"file_path":"/tmp/x"}}'
        Q6_WR='{"session_id":"q6canary-0000","agent_id":"q6canary00000001","agent_type":"dev","tool_name":"Read","tool_input":{"file_path":"/tmp/x"}}'
        Q6_LW='{"session_id":"q6canary-0000","tool_name":"Write","tool_input":{"file_path":"/tmp/x"}}'
        Q6_PROBLEMS=""
        q6_expect() { # <arm> <want-rc>
            if [ "$Q6_RC" -ne "$2" ]; then Q6_PROBLEMS="$Q6_PROBLEMS [$1: exit $Q6_RC, expected $2]"; fi
        }
        q6_run "$ENGINE_ROOT" "$Q6_WW";                 q6_expect "healthy engine, UNREGISTERED worker Write" 2
        q6_run "$ENGINE_ROOT" "$Q6_WR";                 q6_expect "healthy engine, unregistered worker Read (read-only policy)" 0
        q6_run "$ENGINE_ROOT" "$Q6_LW";                 q6_expect "healthy engine, the lead's Write" 0
        q6_run "$Q6_DIR/nolib" "$Q6_WW";                q6_expect "registry LIBRARY MISSING, worker Write" 2
        q6_run "$Q6_DIR/nolib" "$Q6_WR";                q6_expect "library missing, worker Read (read-only policy)" 0
        q6_run "$Q6_DIR/nolib" "$Q6_LW";                q6_expect "library missing, the lead's Write" 0
        q6_run "$ENGINE_ROOT" "$Q6_WW" "$Q6_DIR/nopy";  q6_expect "NO python3, worker Write" 2
        q6_run "$ENGINE_ROOT" "$Q6_WR" "$Q6_DIR/nopy";  q6_expect "no python3, worker Read (read-only policy)" 0
        q6_run "$ENGINE_ROOT" "$Q6_LW" "$Q6_DIR/nopy";  q6_expect "no python3, the lead's Write" 0
        q6_run "$ENGINE_ROOT" "not json";               q6_expect "UNPARSEABLE payload" 2
        if [ -z "$Q6_PROBLEMS" ]; then
            emit_pass "Q6. the lock-out FAILS CLOSED: an unregistered worker, a missing registry, a missing python3 and an unparseable payload each REFUSE a worker's Write (exit 2), while the lead's own call and a worker's read-only tool pass — 10/10 arms"
        else
            emit_fail "Q6. the lock-out does NOT fail closed:$Q6_PROBLEMS. A worker whose registration the lock-out cannot evaluate must be refused every potentially writing or unknown tool. Restore: git checkout -- scripts/hooks/guard-sealed-worktree.sh"
        fi
        rm -rf "$Q6_DIR" 2>/dev/null || true
    fi
fi

# --- Layer S: the WORKTREE guard wired exactly once + path-confined +
# manifest-matched + refuses a raw removal and a raw creation AND allows a
# read, AND the sanctioned command is installed (HARD gate) ---
#
# THE PAIR: guard-worktree-removal.sh (PreToolUse[Bash], blocking) and
#           mega-lander/workspaces.sh (land and discard — the ONLY deleters,
#           docs/plans/worktree-spec-2026-09-11.md). The guard refuses every raw
#           `git worktree remove` / `prune` / `add`, every `branch -D` of an
#           agent's branch and every `rm -r` of a workspace, and names the
#           sanctioned commands.
#
# WHY BOTH HALVES ARE CHECKED HERE: a guard whose sanctioned route is not
# installed only blocks. They ship as a pair and they are verified as a pair.
#
# WHY A PAIRED CANARY: "does it block?" is satisfied by a guard gutted to
# `exit 2`, which would also block `git worktree list` and every other Bash
# call — useless, and worse than absent, because it gets disabled. Both arms run.
S_OK=1
CANONICAL_WTREMOVAL_HOOK="$REPO_ROOT/scripts/hooks/guard-worktree-removal.sh"
CANONICAL_WTREMOVAL_HELPER="$REPO_ROOT/mega-lander/workspaces.sh"

WTREMOVAL_WIRED_CMD=""
WTREMOVAL_WIRED_N=0
for c in "${BASH_MATCHER_CMDS[@]}"; do
    RESOLVED_C="${c//\$CLAUDE_PROJECT_DIR/$REPO_ROOT}"
    RESOLVED_C="${RESOLVED_C//\$\{CLAUDE_PROJECT_DIR\}/$REPO_ROOT}"
    WIRED_PATH_C="${RESOLVED_C%% *}"
    if [ "$(realpath_of "$WIRED_PATH_C")" = "$(realpath_of "$CANONICAL_WTREMOVAL_HOOK")" ]; then
        WTREMOVAL_WIRED_CMD="$RESOLVED_C"
        WTREMOVAL_WIRED_N=$((WTREMOVAL_WIRED_N + 1))
    fi
done

if [ "$WTREMOVAL_WIRED_N" -eq 0 ]; then
    emit_fail "S. PreToolUse[Bash] worktree guard (guard-worktree-removal.sh) NOT wired — a raw 'git worktree remove' of an agent's workspace is unguarded. Run scripts/hooks/install.sh and confirm the Bash matcher stanza carries it."
    S_OK=0
elif [ "$WTREMOVAL_WIRED_N" -gt 1 ]; then
    emit_fail "S. worktree guard wired ${WTREMOVAL_WIRED_N}x on PreToolUse[Bash] — hook sources merge additively, so it fires ${WTREMOVAL_WIRED_N} times per Bash call and duplicates every ack-log line."
    S_OK=0
else
    WTREMOVAL_EXE="${WTREMOVAL_WIRED_CMD%% *}"
    WTREMOVAL_HASH="$(sha256_of "$(realpath_of "$WTREMOVAL_EXE")")"
    WTREMOVAL_MANIFEST="$(manifest_hash_of "$CANONICAL_WTREMOVAL_HOOK")"
    if [ ! -x "$WTREMOVAL_EXE" ]; then
        emit_fail "S. wired worktree guard not found / not executable: $WTREMOVAL_EXE"
        S_OK=0
    elif [ -z "$WTREMOVAL_MANIFEST" ]; then
        emit_fail "S. worktree guard manifest missing or unreadable: $CANONICAL_WTREMOVAL_HOOK.sha256 — run scripts/hooks/install.sh to regenerate."
        S_OK=0
    elif [ "$WTREMOVAL_HASH" != "$WTREMOVAL_MANIFEST" ]; then
        emit_fail "S. worktree guard content hash mismatch — live hook differs from manifest (tamper or stale manifest). Run scripts/hooks/install.sh and review the diff."
        S_OK=0
    fi
fi

if [ "$S_OK" -eq 1 ] && [ ! -x "$CANONICAL_WTREMOVAL_HELPER" ]; then
    emit_fail "S. the sanctioned command is MISSING or not executable: $CANONICAL_WTREMOVAL_HELPER. The guard refuses every raw removal and names land and discard as the only way; without it nothing can land or discard."
    S_OK=0
fi

# Paired functional canary: REFUSE a raw removal and a raw creation, ALLOW a read.
if [ "$S_OK" -eq 1 ] && command -v python3 >/dev/null 2>&1; then
    S_BLOCK_PAYLOAD="$(python3 -c 'import json; print(json.dumps({"tool_name":"Bash","tool_input":{"command":"git worktree remove /x/.claude/worktrees/agent-canary"}}))' 2>/dev/null || true)"
    S_ADD_PAYLOAD="$(python3 -c 'import json; print(json.dumps({"tool_name":"Bash","tool_input":{"command":"git worktree add ../canary -b feature/canary"}}))' 2>/dev/null || true)"
    S_ALLOW_PAYLOAD="$(python3 -c 'import json; print(json.dumps({"tool_name":"Bash","tool_input":{"command":"git worktree list --porcelain"}}))' 2>/dev/null || true)"
    set +e
    printf '%s' "$S_BLOCK_PAYLOAD" | RICHOS_ENTITY_ROOT="$REPO_ROOT" "$WTREMOVAL_EXE" >/dev/null 2>&1
    s_block_rc=$?
    printf '%s' "$S_ADD_PAYLOAD" | RICHOS_ENTITY_ROOT="$REPO_ROOT" "$WTREMOVAL_EXE" >/dev/null 2>&1
    s_add_rc=$?
    printf '%s' "$S_ALLOW_PAYLOAD" | RICHOS_ENTITY_ROOT="$REPO_ROOT" "$WTREMOVAL_EXE" >/dev/null 2>&1
    s_allow_rc=$?
    set -e
    if [ "$s_block_rc" -ne 2 ]; then
        emit_fail "S. wired worktree guard did NOT block a raw 'git worktree remove' (exit=$s_block_rc, expected 2) — the guard is shimmed or gutted."
        S_OK=0
    elif [ "$s_add_rc" -ne 2 ]; then
        emit_fail "S. wired worktree guard did NOT refuse a raw 'git worktree add' of a non-cc/ workspace (exit=$s_add_rc, expected 2) — points 1 and 3 of the workspace spec are unenforced."
        S_OK=0
    elif [ "$s_allow_rc" -ne 0 ]; then
        emit_fail "S. wired worktree guard BLOCKED 'git worktree list' (exit=$s_allow_rc, expected 0) — a guard that blocks reads gets disabled, which is worse than no guard."
        S_OK=0
    else
        emit_pass "S. worktree guard wired exactly once + REFUSES a raw 'git worktree remove' and 'git worktree add' + ALLOWS 'git worktree list'; the sanctioned command (workspaces.sh) is present and executable"
    fi
elif [ "$S_OK" -eq 1 ]; then
    emit_warn "S. FUNCTIONAL CANARY DID NOT RUN — python3 unavailable, so nothing here proves the worktree guard still blocks a raw removal or still allows a read. Wiring and hashes are verified; BEHAVIOR IS NOT."
fi

# --- Layer OF: the operator fence agrees with its declaration ------------------
# richos-hq spec r3 e8 with Frank G12: the switch lives in the launcher that
# operator-fences.sh installs as each repository's reference-transaction hook,
# and OPERATOR_FENCES in orchestration.config is the DECLARATION this layer
# compares the launchers against. Declared off: every declared repository's
# launcher is off or absent. Declared on: every one is installed, on, running
# the engine's current fence program, reachable through core.hooksPath, and its
# chain (the forensic recorder, the repository's own hook) still intact. A fence
# that vanished, or was turned off with `operator-fences.sh off` while the
# declaration still says on, is reported here rather than discovered at a land.
# Not declared at all: nothing to check. The tool missing (a sandbox that carries
# only registered hooks) is announced, never a pass.
run_layer_OF() {
    local decl="${OPERATOR_FENCES:-}" tool="$ENGINE_ROOT/scripts/operator-fences.sh" args="" r out rc
    if [ -z "$decl" ]; then
        emit_pass "OF. no OPERATOR_FENCES declared in $CONFIG: the operator fence is not adopted here, nothing to compare"
        return 0
    fi
    if [ ! -f "$tool" ]; then
        emit_warn "OF. OPERATOR_FENCES=\"$decl\" is declared but $tool is not present, so nothing checked the launchers against it"
        return 0
    fi
    for r in ${OPERATOR_FENCES_REPOS:-}; do
        args="$args --repo ${r/#\~/$HOME}"
    done
    # shellcheck disable=SC2086
    out="$(bash "$tool" status --declaration-check --entity "$REPO_ROOT" $args 2>&1)" && rc=0 || rc=$?
    if [ "$rc" -eq 0 ]; then
        emit_pass "OF. OPERATOR_FENCES=\"$decl\" and the launchers of ${OPERATOR_FENCES_REPOS:-(no declared repository)} agree"
        # A declared holder whose executable an app update moved: the fence still
        # recognizes it inside its bundle, so nothing fails, but the declaration
        # is out of date and the session should hear it before a land does.
        if printf '%s\n' "$out" | grep '^NOTE' >/dev/null; then
            emit_warn "OF. $(printf '%s\n' "$out" | grep '^NOTE' | sed 's/^NOTE  //' | tr '\n' ';')"
        fi
    else
        emit_fail "OF. the operator fence disagrees with OPERATOR_FENCES=\"$decl\": $(printf '%s' "$out" | grep '^PROBLEM' | sed 's/^PROBLEM  //' | tr '\n' ';') Run $tool status --entity $REPO_ROOT for the whole report; 'operator-fences.sh install' and 'on'/'off' are the fixes, never an edit of a launcher by hand."
    fi
}

# Recovery is explicitly adopted per entity. No declaration means no new probe.
run_layer_RC() {
    local tool="$ENGINE_ROOT/scripts/recovery.sh" out rc
    [ -f "$REPO_ROOT/.claude/recovery.json" ] || return 0
    if [ ! -f "$tool" ]; then
        emit_fail "RC. recovery is declared but its host contract reader is absent: $tool"
        return 0
    fi
    out="$(bash "$tool" contract --entity "$REPO_ROOT" --installed-engine "$ENGINE_ROOT" 2>&1)" && rc=0 || rc=$?
    if [ "$rc" -eq 0 ]; then
        emit_pass "RC. recovery declaration and installed mode/profile/source contract agree; native capabilities require separate qualification"
    else
        emit_fail "RC. recovery contract unavailable: $out"
    fi
}

run_layer_R
run_layer_AL
run_layer_MT
run_layer_MC
run_layer_OF
run_layer_RC
# Layer EP lives in its own file, and this include is GUARDED rather than bare.
# A bare `.` of a missing file under `set -e` ends the probe HERE — after every
# ✗ has been printed and BEFORE the epilogue that turns them into exit 2 — so
# the probe reports success over its own findings. That is not hypothetical: the
# integrity suite's sandbox template carries only registered hooks, so cases 3
# and 4 (a hook unwired, a hook mis-pathed) went from exit 2 to exit 0 the moment
# a bare include was added, and the only visible trace was one line of shell
# error in the middle of a green run. NOTHING MAY BE ABLE TO TRUNCATE THIS
# PROBE. A missing layer is announced by name and the run continues; deleting it
# from a real engine is caught hard by scripts/entrypoint-currency-lint.test.sh,
# which asserts the file is present and executable.
if [ -f "$SCRIPT_DIR/contract-integrity-layer-ep.sh" ]; then
    # shellcheck source=contract-integrity-layer-ep.sh
    . "$SCRIPT_DIR/contract-integrity-layer-ep.sh"
    run_layer_EP
else
    emit_warn "EP. layer implementation not present at $SCRIPT_DIR/contract-integrity-layer-ep.sh — nothing here checked whether a standing instruction still names a superseded entrypoint. Expected in a synthetic sandbox that carries only registered hooks; in a real engine it means the layer was deleted, and scripts/entrypoint-currency-lint.test.sh fails hard on that."
fi

if [ "$FAIL" -gt 0 ]; then
    cat >&2 <<EOF

Integrity probe FAILED — $FAIL layer(s) broken. Most fixes:

  - "settings.local.json missing" / "hook NOT wired"
       -> restore it: git checkout -- .claude/settings.local.json
  - "settings.json registers N hook(s)" / "registered Nx" / "double-fire"
       -> a stale hook-duplicating .claude/settings.json is being merged.
          Run: scripts/hooks/install.sh   (removes/strips it)
  - "wired write-guard did NOT block"
       -> the hook file has been modified into a no-op.
          \`git diff scripts/hooks/guard-main-checkout-writes.sh\` to see
          what changed, revert if accidental.
  - "wired hook script not found"
       -> the path in settings.local.json doesn't resolve. Re-check the
          committed wiring; run: scripts/hooks/install.sh
  - "env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS missing" / "worktree.baseRef missing"
       -> restore the missing key(s) in .claude/settings.local.json (see
          README.md's "Critical configuration — never remove" section), then
          re-run: scripts/hooks/install.sh
  - "secrets scanner NOT wired" / "did NOT block a known-bad secret"
       -> run scripts/hooks/install.sh; if the scanner still doesn't block,
          \`git diff scripts/hooks/scan-secrets.sh\` to see what changed.
  - "settings.local.json ... NOT git-tracked" (Layer N)
       -> a gitignore rule (usually a GLOBAL ~/.config/git/ignore
          '**/.claude/settings.local.json' entry) silently kept the
          committed-by-design file untracked. Force-add it:
          git add -f .claude/settings.local.json   (then commit)
  - "Bash main-write guard NOT wired" / "did NOT block a main-checkout ... write" (Layer O)
       -> run scripts/hooks/install.sh; confirm the PreToolUse[Bash] matcher
          stanza is present in .claude/settings.local.json; if it still doesn't
          block, \`git diff scripts/hooks/guard-bash-main-writes.sh\`.
  - "definition-drift guard NOT wired" / "definition snapshotter NOT wired" /
    "did NOT block a definition modified since the session-start snapshot" (Layer P)
       -> restore both halves in .claude/settings.local.json (SessionStart ->
          snapshot-agent-definitions.sh, PreToolUse[Agent] position 2 ->
          guard-definition-drift.sh), then run scripts/hooks/install.sh. Either
          half missing silently disables the pair: with no session-start
          snapshot the guard fails OPEN and blocks nothing.
  - "the workspace spec is not wired" / "FUNCTIONAL canary failed" (Layer Q)
       -> the workspace lifecycle hook, its Stop gate or the registry has been
          unwired, gutted or altered. Restore them:
            git checkout -- .claude/settings.local.json hooks/hooks.json \\
            mega-lander/workspaces.py mega-lander/workspaces.sh \\
            scripts/hooks/workspace-lifecycle.sh scripts/hooks/guard-workspace-gate.sh
          then: scripts/hooks/install.sh

  - "MODEL_TIERS ... is blank" / "MODEL_TIERS and ALLOWED_MODELS disagree" /
    "quotes ... but orchestration.config declares" (Layer MT)
       -> declare the capability order ONCE in orchestration.config
          (MODEL_TIERS="fable > opus > sonnet > haiku", re-derived for your
          models), keep its alias set equal to ALLOWED_MODELS, and make
          CLAUDE.md quote that exact line. Never edit a consumer to fix it.

  - "A STANDING INSTRUCTION ... STILL NAMES A SUPERSEDED ENTRYPOINT" (Layer EP)
       -> a capability shipped and the thing that tells people what to do still
          names what it replaced, so the new path goes unused while every
          session reports success. The refusal names file:line, the superseded
          command and the current one. Change the INSTRUCTION to name the
          current command; do NOT retire the old script (spawn.sh calls the
          commands it supersedes). A genuinely historical mention takes
          \`entrypoint-exempt: <reason>\` on the line. Full report:
          scripts/entrypoint-currency-lint.sh --root <repo>

  - "MODEL_CEILING ... ranks it NOWHERE" / "did NOT refuse a spawn one tier
    ABOVE" / "was allowed but NOT SILENTLY" (Layer MC)
       -> the COST ceiling, which is a different order from the capability one.
          Declare MODEL_CEILING once in orchestration.config, naming an alias
          MODEL_TIERS ranks (shipped: "opus" — the normal ceiling for critical
          work, with the tier above reserved for super-critical work and extreme
          one-off cases). If the canary itself fails, the guard is dead or
          inverted: \`git diff scripts/hooks/guard-model-ceiling.sh\` and
          scripts/lib/resolve-model.sh, then run scripts/hooks/install.sh.

See README.md (First-time setup) and orchestration.config.
EOF
    exit 2
fi

emit_verdict "the engine SEATED in $REPO_ROOT" \
    "Every layer above was RUN against this repository's own .claude/settings.local.json wiring, functional canaries included."
exit 0
