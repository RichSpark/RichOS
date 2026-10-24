#!/usr/bin/env bash
#
# contract-integrity.test.sh — self-test harness for install.sh +
# contract-integrity-probe.sh.
#
# CANONICAL SOURCE (single-registration root-fix): hooks are registered in
# exactly ONE file, `.claude/settings.local.json` (committed). install.sh no
# longer writes hook stanzas into `.claude/settings.json`; instead it MIGRATES
# any stale hook-duplicating settings.json out of the way (removing a pure
# duplicate, or stripping hooks from a machine-specific one). The probe reads
# the wiring from settings.local.json, and its new Layer M asserts the wiring is
# not duplicated across both files (the additive-merge double-fire). Cases below
# target settings.local.json as the canonical source.
#
# The hard-gated hooks are the main-checkout write guard
# (guard-main-checkout-writes.sh), the secrets scanner (scan-secrets.sh —
# wired as a SECOND hook in the same Write|Edit|MultiEdit|NotebookEdit
# matcher entry), and the PreToolUse[Agent] chain's structural prefix
# (guard-worktree-isolation.sh FIRST, then guard-definition-drift.sh, then
# verify-agent-prompt.sh). Adversarial coverage:
# shim attacks (path confinement) and manifest tampering (sidecar hash closure).
#
# The definition-drift guard PAIR is part of the managed set —
# guard-definition-drift.sh at Agent-chain position 2, snapshot-agent-
# definitions.sh under SessionStart — so both sandbox fixtures wire them and
# probe Layer P has something to verify. The pair's own BEHAVIORAL suite lives
# in scripts/hooks/guard-definition-drift.test.sh; this harness covers wiring.
#
# THE WORKSPACE SPEC (docs/plans/worktree-spec-2026-09-11.md) is part of the
# managed set: workspace-lifecycle.sh on its six events, guard-workspace-gate.sh
# on Stop, and the registry (mega-lander/workspaces.py) with its one command
# (mega-lander/workspaces.sh), the only code that deletes a workspace. Every
# fixture carries them with minted sidecars so probe Layer Q has something to
# assert. Their behavior lives in mega-lander/tests/workspaces.test.sh and
# mega-lander/tests/workspaces-e2e.test.sh; this harness covers wiring plus the tamper,
# retired-deleter and functional-canary axes Layer Q is responsible for.
#
# The harness builds an isolated sandbox per case (a committed-fresh-clone
# skeleton: canonical settings.local.json + all hook scripts + their .sha256
# sidecars + an orchestration.config, and NO settings.json) so the real repo is
# never mutated.

set -o pipefail

# Drain predicate input before returning. grep -q can close its pipe early and
# turn a successful match into SIGPIPE from printf under pipefail. The
# redirected grep predicates below keep the same patterns without that race.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL="$SCRIPT_DIR/install.sh"
PROBE="$SCRIPT_DIR/contract-integrity-probe.sh"
REAL_REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# ---------------------------------------------------------------------------
# SCOPED RUNS — a development lever, and never a substitute for a full pass.
#
# Every case in this file used to cost a full run: one line changed in one
# guard, and the developer paid for all of them. That is the cost the CEO named
# on 2026-09-03 -- "waiting hours for every little fart" -- and it is a cost of
# the HARNESS, not of the rigor.
#
# So the file is divided into SECTIONS, each wrapped in `if _section <id>; then
# ... fi  # _section`. With no --only, every section runs and the exit code is
# what it always was. With --only, the unselected sections do not run, and the
# run is marked as scoped in three independent ways, because a scoped run that
# can be mistaken for a full pass is worse than no scoped run at all:
#
#   * a banner before the first case,
#   * a banner in the exit summary naming EVERY case that did not run,
#   * and EXIT CODE 3 when a scoped run is green. Not 0. A land gate, a CI job
#     or run-all-tests.sh reads exit codes, not banners, and 0 is the one thing
#     a scoped run must never be able to say.
#
# The section list is read from this file's own markers, so it cannot drift
# from the sections that exist.
# ---------------------------------------------------------------------------
SELF="$SCRIPT_DIR/$(basename "${BASH_SOURCE[0]}")"
SCOPED=0
SCOPE_RUN=""

# Every section id, in file order, read from the markers themselves.
_scope_all_sections() {
    awk '/^if _section [A-Za-z0-9_.-]+; then$/ { s=$3; sub(/;$/, "", s); print s }' "$SELF"
}

# Every case name a section can emit, read statically from its body. Covers
# both shapes in this file: emit_case "<name>", and the hand-rolled
# PASS/FAIL_NAMES pairs (cases 18, 19b, 32b, 33b and friends).
_scope_cases_of() { # <section-id>
    awk -v want="$1" '
        /^if _section [A-Za-z0-9_.-]+; then$/ { cur=$3; sub(/;$/, "", cur); next }
        /^fi  # _section/ { cur=""; next }
        cur != want { next }
        {
            line = $0
            while (match(line, /emit_case "[^"]+"/)) {
                print substr(line, RSTART + 11, RLENGTH - 12)
                line = substr(line, RSTART + RLENGTH)
            }
            line = $0
            while (match(line, /FAIL_NAMES\+=\("[^"$]+"\)/)) {
                print substr(line, RSTART + 14, RLENGTH - 16)
                line = substr(line, RSTART + RLENGTH)
            }
        }
    ' "$SELF" | sort -u
}

_scope_usage() {
    cat <<'USAGE'
usage: contract-integrity.test.sh [--only <selector>[,<selector>...]] [--list]

  (no arguments)   THE FULL PASS. Every section, every case. This is the only
                   run a land may rely on, and the only one that can exit 0.

  --only <sel>     Run only the named sections. A DEVELOPMENT LEVER: it exists
                   so a one-line change to one guard does not cost the whole
                   suite. <sel> is either a section id (see --list) or a case
                   name / case-name prefix, in which case the section that
                   CONTAINS that case is selected -- a single case cannot be
                   run alone, because its section builds the fixtures it needs.
                   Comma-separated or repeated. Matching is case-insensitive.
                   An unmatched selector is FATAL: a scoped run that silently
                   selected nothing would report a green empty suite.

  --list           Print every section id with the cases it carries, and exit.

  --help           This text.

EXIT CODES
  0   full pass, no failures                 -- the only green a land accepts
  1   failures (scoped or full)
  2   bad usage / the harness could not start
  3   SCOPED run, no failures. Deliberately NOT 0, so no gate, script or CI job
      can read a partial run as a full one.

ENVIRONMENT
  CI_TEST_TIMING=<file>  append per-phase and per-case wall-clock records.
  CI_PROBE_DEBUG=1       echo the probe's own failure lines to stderr.
USAGE
}

_SCOPE_ARGS=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --help|-h) _scope_usage; exit 0 ;;
        --list) _SCOPE_LIST=1; shift ;;
        --only) [ "$#" -ge 2 ] || { echo "FATAL: --only needs a selector" >&2; exit 2; }
                _SCOPE_ARGS="$_SCOPE_ARGS,$2"; shift 2 ;;
        --only=*) _SCOPE_ARGS="$_SCOPE_ARGS,${1#--only=}"; shift ;;
        *) echo "FATAL: unrecognized argument '$1' — see --help" >&2; exit 2 ;;
    esac
done

if [ -n "${_SCOPE_LIST:-}" ]; then
    for _sid in $(_scope_all_sections); do
        printf '%s\n' "$_sid"
        _scope_cases_of "$_sid" | sed 's/^/    /'
    done
    exit 0
fi

if [ -n "$_SCOPE_ARGS" ]; then
    SCOPED=1
    _ALL_SECTIONS="$(_scope_all_sections)"
    [ -n "$_ALL_SECTIONS" ] || { echo "FATAL: no section markers found in $SELF — refusing to run a scoped suite whose sections cannot be enumerated" >&2; exit 2; }
    _OLD_IFS="$IFS"; IFS=','
    for _sel in $_SCOPE_ARGS; do
        IFS="$_OLD_IFS"
        [ -n "$_sel" ] || continue
        _sel_lc="$(printf '%s' "$_sel" | tr '[:upper:]' '[:lower:]')"
        _hit=""
        for _sid in $_ALL_SECTIONS; do
            _sid_lc="$(printf '%s' "$_sid" | tr '[:upper:]' '[:lower:]')"
            if [ "$_sid_lc" = "$_sel_lc" ]; then _hit="$_hit $_sid"; fi
        done
        if [ -z "$_hit" ]; then
            for _sid in $_ALL_SECTIONS; do
                if _scope_cases_of "$_sid" | tr '[:upper:]' '[:lower:]' | grep >/dev/null "^$_sel_lc"; then
                    _hit="$_hit $_sid"
                fi
            done
        fi
        if [ -z "$_hit" ]; then
            echo "FATAL: --only '$_sel' matches no section and no case. Sections are:" >&2
            printf '%s\n' "$_ALL_SECTIONS" | sed 's/^/  /' >&2
            echo "  (--list also prints the cases each one carries)" >&2
            exit 2
        fi
        for _sid in $_hit; do
            case " $SCOPE_RUN " in *" $_sid "*) ;; *) SCOPE_RUN="$SCOPE_RUN $_sid" ;; esac
        done
        IFS=','
    done
    IFS="$_OLD_IFS"
    SCOPE_RUN="$(printf '%s' "$SCOPE_RUN" | sed 's/^ *//')"
fi

# Is this section in scope? Unscoped runs answer yes to everything.
_section() { # <section-id>
    [ "$SCOPED" -eq 1 ] || return 0
    case " $SCOPE_RUN " in *" $1 "*) return 0 ;; esac
    return 1
}

# SANDBOX THE OPERATOR CONFIG DIR, for the whole suite, before anything runs.
# install.sh mints the entity-facing engine pointer into
# ${CLAUDE_CONFIG_DIR:-$HOME/.claude}, and this suite runs it against ~20
# throwaway sandboxes. Without this line every one of those repoints the REAL
# operator's pointer at a temp directory that is deleted seconds later, leaving
# a dangling symlink on the machine. Observed, once, before the variable
# existed — which is also how BR6b's dangling-pointer branch got written.
CLAUDE_CONFIG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/contract-integrity-cfg.XXXXXX")"
export CLAUDE_CONFIG_DIR
trap 'rm -rf "$CLAUDE_CONFIG_DIR"' EXIT

if [ ! -x "$INSTALL" ] || [ ! -x "$PROBE" ]; then
    echo "FATAL: install.sh or contract-integrity-probe.sh missing/non-exec" >&2
    exit 1
fi

PASS=0
FAIL=0
FAIL_NAMES=()
NOT_RUN=0
NOT_RUN_NAMES=()

# ---------------------------------------------------------------------------
# TIMING INSTRUMENTATION — opt-in, and a NO-OP unless CI_TEST_TIMING names a
# file. It exists because the record asserted where this suite's 42 minutes go
# ("SETUP repeated 81 times") without ever measuring it. An optimization aimed
# at an unmeasured cost center is a guess with a commit message.
#
# It records absolute start/end stamps rather than durations, so the analyzer
# can attribute a sandbox build to the case whose segment contains it — no call
# site has to know which case it belongs to. Nothing here touches stdout;
# make_sandbox's stdout IS its return value.
# ---------------------------------------------------------------------------
CI_TEST_TIMING="${CI_TEST_TIMING:-}"
if [ -n "$CI_TEST_TIMING" ]; then
    : >"$CI_TEST_TIMING" || { echo "FATAL: cannot write CI_TEST_TIMING=$CI_TEST_TIMING" >&2; exit 1; }
    export CI_TEST_TIMING
fi
# perl, not python3: measured at ~7ms per call against python3's ~20ms, and
# this is called several hundred times. BSD date has no sub-second format and
# bash 3.2 has no EPOCHREALTIME.
_t() {
    [ -n "$CI_TEST_TIMING" ] || { printf '0\n'; return 0; }
    perl -MTime::HiRes=time -e 'printf "%.4f\n", time'
}
_trec() { # <kind> <start> <end-or-dash> <label>
    [ -n "$CI_TEST_TIMING" ] || return 0
    printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" >>"$CI_TEST_TIMING"
}

# ---------------------------------------------------------------------------
# _sed_inplace <script> <file> — because `sed -i` means two different things
# ---------------------------------------------------------------------------
# BSD/macOS `sed -i` requires the backup suffix as a SEPARATE ARGUMENT, so the
# in-place idiom is `sed -i '' 's/a/b/' file`. GNU/Linux `sed -i` takes the
# suffix ATTACHED, reads that empty string as the SCRIPT, and then treats the
# real script as a FILENAME:
#
#     sed: can't read s/^MODEL_TIERS=.*/MODEL_TIERS=""/: No such file or directory
#
# MEASURED, 2026-09-04, on ubuntu:24.04 as an unprivileged user: this suite ran
# 139 cases and then ABORTED at the MT section on exactly that, under `set -e`,
# exit 2, with the MT and MC sections never reached. `engine-self-verify.yml`
# runs on `ubuntu-latest`, so the first public run of this repository would have
# been red here — and the engine is developed on macOS, where all four sites are
# correct, which is why nothing local ever saw it.
#
# It is the third instance of this class in this file's history:
# `docs/ci-portability-notes.md` records `mktemp -t` with an X-less template
# (hard failure on GNU) and `BASH_CMDS` (a reserved name on bash >= 4). Both
# were found by the engine's first and only Linux execution on 2026-08-29. The
# four sites below arrived after it, in the MT and MC sections, and nothing has
# run this suite on Linux since.
#
# A temp file means the same thing on every host, and `cat >` rather than `mv`
# keeps the target's inode and mode — which matters here, because the files
# being edited live in sandboxes whose contents this suite hashes.
_sed_inplace() { # <sed-script> <file>
    local _t
    _t="$(mktemp "${TMPDIR:-/tmp}/ci-sed.XXXXXX")" || return 1
    sed "$1" "$2" >"$_t" || { rm -f "$_t"; return 1; }
    cat "$_t" >"$2" || { rm -f "$_t"; return 1; }
    rm -f "$_t"
}

# All canonical hook scripts the probe/install manage — DERIVED FROM
# hooks/hooks.json, NEVER TYPED.
#
# This was a hand-typed list of 22 names, in the suite whose entire subject is
# whether the engine's guards are actually wired. It was wrong in two ways at
# once and both were invisible:
#
#   * it was MISSING FOUR REGISTERED HOOKS (the worker-*-handoff.sh family).
#     Every sandbox this suite built was therefore an engine that cannot exist,
#     and the four were never copied, never hashed and never checked here.
#   * adding a 23rd guard made every sandbox miss it, so the probe's Layer R
#     reported it ABSENT and fourteen cases went red at once — with an error
#     message about the new guard rather than about this list. That is the
#     failure mode of a typed inventory exactly: the symptom points away from
#     the cause.
#
# scripts/lib/registered-hooks.sh exists because the guard COUNT drifted twice
# in two days for this reason, and its own header says a list a human must
# remember to update is not a mitigation for drift but a source of it. The
# suite that checks that contract was still carrying one. It is derived now,
# from the same file the host actually loads, through the same parser
# install.sh and engine-status.sh use.
#
# NO SILENT DEGRADATION: deriving nothing is fatal here, not an empty sandbox
# that passes every case by having nothing in it to check.
_RH_LIB="$SCRIPT_DIR/../lib/registered-hooks.sh"
if [ ! -f "$_RH_LIB" ]; then
    echo "FATAL: scripts/lib/registered-hooks.sh missing — the hook inventory cannot be derived, and this suite will not fall back to a typed list" >&2
    exit 1
fi
# shellcheck source=../lib/registered-hooks.sh
. "$_RH_LIB"
ALL_HOOKS=()
while IFS= read -r _h; do
    [ -n "$_h" ] || continue
    ALL_HOOKS+=("$_h")
done <<REGISTERED_EOF
$(registered_hook_scripts "$SCRIPT_DIR/../../hooks/hooks.json" || true)
REGISTERED_EOF
if [ "${#ALL_HOOKS[@]}" -eq 0 ]; then
    echo "FATAL: derived ZERO hooks from hooks/hooks.json — refusing to build sandboxes with nothing in them and report them green" >&2
    exit 1
fi

# The completeness check for the lists below. It is FATAL if absent for the same
# reason the hook inventory is: this suite's cases are only worth their exit
# codes if the sandbox they run in can assemble the engine, and skipping the
# question is how it stopped being asked in the first place.
_SC_LIB="$SCRIPT_DIR/../lib/sandbox-completeness.sh"
if [ ! -f "$_SC_LIB" ]; then
    echo "FATAL: scripts/lib/sandbox-completeness.sh missing — the sandbox file lists below cannot be checked against the engine they claim to model, and this suite will not report green without asking" >&2
    exit 1
fi
# shellcheck source=../lib/sandbox-completeness.sh
. "$_SC_LIB"

# The files the registered hooks themselves reach for, DERIVED. The list below
# is a hand-maintained statement of why each file is carried, and it stays that
# way; this adds the half that has no registration surface to be derived from
# and therefore goes stale unnoticed. On 2026-09-14 that half cost demo.sh its
# whole run, and the same omission was live HERE, unfixed, the day this was
# written: guard-hook-registration-commits.sh was copied into every sandbox and
# scripts/hook-registration-completeness.sh -- the helper it exits 2 without --
# was copied into none.
#
# SC1 was green over it, and the reason was the harness rather than the check:
# SC1 asks whether a hook can START, by running it, and the SANDBOX every case
# is built from was NOT A GIT REPOSITORY, so the guard exited 0 at its
# jurisdiction test and never reached the helper. A dependency loaded past a
# precondition the harness does not satisfy is never reached.
#
# (The claim first written here was that "`git init` appears nowhere in this
# file". That was a grep result, not a fact about the harness: make_git_main
# below has built real repositories for the `worktree` and `N` cases all along,
# spelled `git -C "$root" init -q -b main`, which a search for those two words
# cannot see. The sandbox conclusion held; the reason given for it did not.)
#
# AMENDED 2026-09-14, later the same day: the sandbox is a repository now
# (init_sandbox_repo, below), so that particular blindness is closed and SC1
# names this exact omission when it is planted -- measured both ways. Twelve of
# the seventy registered hooks execute further for it; the census is at
# init_sandbox_repo.
#
# THE LIMIT ITSELF DOES NOT GO AWAY, and it is the reason both halves are kept:
# "can it start?" is still not "can it run?", and the next dependency to sit
# behind a precondition this harness does not satisfy will be just as invisible
# to SC1 as this one was. The derivation sees it whether or not any payload
# does; SC1 sees what no scan can read. The two are complements.
_HD_LIB="$SCRIPT_DIR/../lib/hook-dependencies.sh"
if [ ! -f "$_HD_LIB" ]; then
    echo "FATAL: scripts/lib/hook-dependencies.sh missing — the files the registered hooks depend on cannot be derived, and this suite will not assemble a sandbox from a hand list alone and report it green" >&2
    exit 1
fi
# shellcheck source=../lib/hook-dependencies.sh
. "$_HD_LIB"
HOOK_DEPENDENCIES=()
if ! _HD_OUT="$(richos_hook_dependency_closure "$REAL_REPO_ROOT")"; then
    echo "FATAL: could not derive what the registered hooks depend on (rc $?) — refusing to build sandboxes whose hooks start and decide nothing" >&2
    exit 1
fi
while IFS= read -r _d; do
    [ -n "$_d" ] || continue
    HOOK_DEPENDENCIES+=("$_d")
done <<HOOK_DEPS_EOF
$_HD_OUT
HOOK_DEPS_EOF

# Managed scripts living OUTSIDE scripts/hooks/, relative to the repo root.
# install.sh mints a sidecar for each, so every sandbox must carry them.
ALL_ROOT_SCRIPTS=(
    # THE WORKSPACE REGISTRY and its one command. Every workspace hook hands its
    # facts and verdicts to the registry, clause 7 of guard-worktree-isolation.sh
    # REFUSES a file-capable spawn it cannot register there, and
    # guard-worktree-removal.sh names the command as the only way a workspace is
    # deleted, so Layers Q and S verify both and every sandbox must carry both.
    mega-lander/workspaces.py
    mega-lander/workspaces.sh
    # The container reaper the registry calls before it deletes a landed
    # workspace. Since hunt part 4 finding 11 (ad4a051a6) a reaper that cannot
    # run HOLDS the workspace and its branch for retry, exactly like a process
    # that survived its stop. The registry reaches it with `import containers`
    # after putting scripts/lib on sys.path, a shape hook-dependencies.py cannot
    # see (it reads quoted path tokens), so the derived half never carried it.
    # Without it, Layer Q's canary lands the merged agent and then keeps its
    # workspace and branch, and the layer fails for a sandbox defect rather than
    # an engine one. On a machine without Docker the reaper answers
    # "unavailable", which deletes normally, so carrying it costs nothing there.
    scripts/lib/containers.py
    # The root-resolution contract is a managed, sidecar-hashed file: it is not
    # a hook, and it decides something no hook can second-guess — which
    # repository every guard is protecting.
    scripts/lib/resolve-roots.sh
    # The publication-boundary predicate. Two registered guards refuse to start
    # without it, so a sandbox missing it would model an engine that cannot run.
    scripts/lib/publication-boundary.sh
    scripts/lib/publication-boundary.py
    # The declaration resolver, which publication-boundary.sh and
    # row-currency.sh both source before they can answer whether a repository
    # declares anything at all. A sandbox missing it would model an engine
    # whose guards report BROKEN on every lookup — and, worse, would leave
    # install.sh minting a sidecar for a file that is not there.
    scripts/lib/declaration-path.sh
    # The CEO-TODOs predicate. guard-ceo-todos-commits.sh refuses to start
    # without ceo-todos.sh, so a sandbox missing it would model an engine that
    # cannot run — the same reason the publication pair is on this list.
    scripts/lib/ceo-todos.sh
    scripts/lib/ceo-todos.py
    # The row-currency predicate, for the same reason again:
    # guard-row-currency-commits.sh refuses to start without row-currency.sh.
    scripts/lib/row-currency.sh
    scripts/lib/row-currency.py
    # The Stop-hook notice channel. It is on this list for a DIFFERENT reason
    # from the predicates above, and the difference is worth stating. Those
    # guards REFUSE TO START without their predicate, so a sandbox missing one
    # models an engine that cannot run. The Stop hooks do not refuse — absent
    # this file they fall back to announcing every turn, undeduplicated. So a
    # sandbox without it would not look broken; it would look FINE while
    # modelling a different engine from the one that ships. That is the harder
    # failure to notice, which is why it is carried too.
    scripts/lib/stop-hook-notice.sh
    # The CEO-ask predicate. On this list for the FIRST reason, not the
    # stop-hook-notice one: notice-ceo-unasked.sh fails OPEN without it, so a
    # sandbox missing it would model an engine whose CEO-ask notice is silently
    # off — and it would look fine doing it.
    scripts/lib/ceo-asks.sh
    scripts/lib/ceo-asks.py
    # The CEO-RULED predicate and its operator CLI. On this list for the FIRST
    # reason, and for a second one the SC1 check cannot see. FIRST:
    # guard-ceo-ruled-ask.sh fails OPEN without ceo-ruled.sh and announces it,
    # so a sandbox missing it models an engine whose newest blocking guard is
    # silently off. SECOND, the soft half: ceo-ruled.py loads its tokenizer out
    # of ceo-asks.py, so a sandbox carrying one and not the other assembles an
    # engine that STARTS FINE and reads questions differently from the one that
    # ships. ceo-ruled-exempt.sh is here because the refusal names it as the
    # only way through — a sandbox with a gate and no escape hatch models a
    # wall, which is the thing this gate was designed not to be.
    scripts/lib/ceo-ruled.sh
    scripts/lib/ceo-ruled.py
    scripts/ceo-ruled-exempt.sh
    # The agent-liveness resolver, both halves plus its operator CLI. Probe
    # Layer AL fails outright without it, and the Stop-time claim check takes
    # its entire answer from it, so a sandbox missing it models an engine whose
    # claim check is off.
    scripts/lib/agent-liveness.py
    scripts/lib/agent-liveness.sh
    scripts/agent-liveness.sh
    # THE OWNERSHIP LEDGER and the cross-repository worktree helper (2026-09-02).
    # The ledger fails SOFT: the three handoff hooks record their advisory
    # signals in it and say nothing a sandbox reads without it. The helper is
    # carried because it is how a cross-repository workspace is registered
    # before it is created (points 1, 3), and the spawn guard's refusal names
    # it — a gate with no escape route models a wall.
    scripts/lib/worktree-ledger.py
    mega-lander/create-teammate-worktree.sh
    # The jurisdiction predicate — and it was MISSING from this list, silently,
    # for as long as the list has existed. scan-secrets.sh and guard-dialect.sh
    # both REFUSE TO START without it, by exiting 2, and Layer K's canary
    # asserts the scanner exits 2 on a planted secret. Same number, opposite
    # meanings: the layer was green here over a scanner that never ran. Layer T
    # now carries a clean-content canary a dead hook cannot satisfy, which is
    # what turned this omission from invisible into a failing suite.
    scripts/lib/seat-jurisdiction.sh
    # The git-jurisdiction resolver — WHICH REPOSITORY a `git commit`/`git push`
    # on the Bash tool is actually talking to. On this list for the FIRST
    # reason: all five commit/push guards REFUSE TO START without it, by exiting
    # 2, exactly as the predicates above do. It is one file rather than five
    # copies BECAUSE it was five copies, and four of them missed
    # `cd <repo> && git commit` — the form a worktree-isolated agent types by
    # default. SC1 below is what stops this list falling behind it.
    scripts/lib/git-jurisdiction.sh
    # The NAMED-PERSON predicate, both halves plus the operator/release front
    # end. On this list for the FIRST reason: guard-named-persons-writes.sh and
    # guard-named-persons-commands.sh both REFUSE TO START without
    # scripts/lib/named-persons.sh, by exiting 2 — which SC1 caught the moment
    # they were registered, and is why they are here rather than discovered
    # later. The front end is carried because the release chokepoint in
    # app/scripts/make-release.sh names it and DIES when it is absent, so a
    # sandbox with the guards and no front end models an engine whose last line
    # of defense refuses every release.
    scripts/lib/named-persons.sh
    scripts/lib/named-persons.py
    scripts/named-persons.sh
    # The VENDORED-MATERIAL reader — whose work is this? On this list for the
    # FIRST reason twice over: guard-vendoring-commits.sh REFUSES TO START
    # without it, and so does guard-dialect.sh, which is the more interesting
    # half. The dialect guard used to have no such dependency, and on 2026-08-30
    # it rewrote fourteen lines across two vendored MIT-licensed skills because
    # it could not tell whose bytes they were. A sandbox missing this file would
    # model precisely that engine — a dialect guard that starts, runs, reports,
    # and is blind to the only question that stops it editing other people's
    # prose. It refuses instead.
    scripts/lib/vendored-material.sh
    # The global-state witness, and it is on this list for the READING reason
    # rather than the running one — stated plainly, because SC1 below cannot see
    # it. No registered hook touches it: it is what the suites that run
    # install.sh use to prove they gave back the operator's engine pointer. A
    # sandbox without it assembles an engine whose test surface cannot answer
    # "did this run leave the pointer moved?", which is the question a red-run
    # fixture answered wrongly on 2026-09-01 while reporting success.
    scripts/lib/global-state-witness.sh
    # The dialect vocabulary. guard-dialect.sh decides nothing without it and
    # Layer T fails loudly when it is absent — first reason on this list, same
    # as the predicates above.
    scripts/lib/dialect-en-US.dict
    # The in-flight notice predicate, both halves. guard-inflight-notify.sh
    # REFUSES TO START without inflight.sh — first reason on this list again —
    # and inflight.sh itself decides nothing without inflight.py.
    #
    # This one was NOT found by reading the diff of what landed. It was found by
    # the completeness case below, which starts every registered hook in a
    # sandbox and reports the ones that cannot. That is the difference between
    # fixing the instance and closing the class, and it is why the case exists.
    scripts/lib/inflight.sh
    scripts/lib/inflight.py
    # The teammate-identity module, and it is here for the STOP-HOOK-NOTICE
    # REASON above rather than the refuse-to-start one — which is worth being
    # precise about, because it marks the edge of what SC1 below can see.
    #
    # inflight.sh and inflight.py do NOT refuse without it. They degrade: the
    # teams directory resolves to nothing, every name resolves to empty, and
    # inflight.py's own wrapper says "teammate-identity.py — MISSING" into a
    # structure nobody reads in a sandbox. So a sandbox without it starts every
    # hook cleanly, passes SC1, and models an engine whose in-flight sweep can
    # no longer name a single teammate.
    #
    # SC1 answers "can every hook START". It does not answer "does every hook
    # DECIDE the same way it would in a real engine", and it cannot: a
    # dependency that fails soft has, by construction, no announcement to read.
    # That half stays a human judgment, made at this list, which is why each
    # entry states its own reason.
    scripts/lib/teammate-identity.py
    # The interactive-prompt shape table. FIRST reason on this list, in its
    # strongest form: guard-interactive-prompt.sh REFUSES TO START without it,
    # by exiting 2 and naming it — and Layer IP's canary asserts that guard
    # exits 2 on a prompting command. Same number, opposite meanings, which is
    # exactly the shape that left Layer K green over a dead scanner. A sandbox
    # missing this file would model an engine whose newest blocking guard is
    # off, and Layer IP would report it as on.
    scripts/lib/interactive-prompt.py
    # THE STOP-HOOK ANALYZERS. Five registered Stop hooks decide NOTHING
    # themselves: the .sh resolves the two roots, reads config, and hands the
    # whole verdict to a sibling .py. None of them was on this list, and the
    # reason that went unnoticed for as long as the list has existed is the
    # sharpest version of the soft-failure problem this file documents further
    # up: a wrapper without its analyzer STARTS PERFECTLY. It emits a "NOT
    # RUNNING: the analyzer is missing" notice into a channel nothing reads in a
    # sandbox, exits 0, and passes SC1's can-every-hook-start question.
    #
    # guard-idle-land.py is the one that made this visible, because Layer IL
    # runs the gate FOR REAL and a missing analyzer makes the two-sided canary
    # fail hard rather than quietly. The other four are carried for the reason a
    # sandbox carries stop-hook-notice.sh: without them it does not look broken,
    # it looks FINE while standing in for a different engine.
    scripts/hooks/guard-idle-land.py
    scripts/hooks/guard-unresolved-claims.py
    scripts/hooks/guard-agent-state-claims.py
    scripts/hooks/guard-unasked-deferral.py
    scripts/hooks/turn-manifest.py
    # The waiver-repetition analyzer, on this list for the stop-hook-notice
    # reason rather than the refuse-to-start one. notice-waiver-repetition.sh
    # does NOT refuse without it: it starts, announces "WAIVER-REPETITION WATCH
    # IS OFF: the analyzer is missing" into a channel a sandbox has no reader
    # for, and exits 0 — so SC1's can-every-hook-start question passes over a
    # sandbox modelling an engine whose newest Stop notice reads no ledger.
    scripts/hooks/notice-waiver-repetition.py
    # The mechanical-findings sweep, both halves, on this list for the
    # stop-hook-notice reason: notice-mechanical-findings.sh does NOT refuse
    # without them — it starts, announces "MECHANICAL SWEEP IS OFF" into a
    # channel a sandbox has no reader for, and exits 0. A sandbox missing them
    # models an engine whose newest Stop hook writes no row.
    scripts/lib/mechanical-findings.sh
    scripts/lib/mechanical-findings.py
    # The model RESOLVER — which model does this spawn boot on? Carried for the
    # HARD reason, in its strongest form: guard-worktree-isolation.sh REFUSES TO
    # START without it, by exiting 2 and naming it, because neither its
    # truthful-name clause nor its capability-tier clause can be evaluated
    # without a resolved model. Layer C's canary asserts that guard is present
    # and hash-matched, and a sandbox missing this file would model an engine in
    # which EVERY spawn is blocked — every spawn-shaped case in this file would
    # go red for a reason having nothing to do with what it tests.
    #
    # The SOFT half, stated because SC1 cannot see it: guard-model-ceiling.sh
    # does NOT refuse without it. It announces "the ceiling is off" into a
    # channel a sandbox has no reader for and allows the spawn, so a sandbox
    # carrying one consumer and not the other models an engine whose newest
    # blocking guard is silently unenforced.
    scripts/lib/resolve-model.sh
    # The model-tier parser. Carried for the DECIDING reason, and SC1 below
    # cannot see it: guard-worktree-isolation.sh does not refuse without it —
    # clause 6 fails OPEN by design, announces the skip into a channel a
    # sandbox has no reader for, and allows the spawn. A sandbox without it
    # starts every hook and models an engine in which a move to a lower
    # capability tier is checked by nobody. Layer MT fails loudly when it is
    # absent, which is how a sandbox missing it stops looking healthy.
    scripts/lib/model-tiers.sh
)

# Sandbox orchestration.config: protected trees for the write-guard + canary.
write_sandbox_config() {
    cat >"$1/orchestration.config" <<'CFG'
PROTECTED_PATHS="app packages"
READONLY_ALLOWLIST="Explore Plan claude-code-guide statusline-setup"
CREATOR_TEAMMATE="dean"
# Declared, so Layer T exercises the dialect guard instead of taking its
# "declared nothing, enforcing nothing" WARN branch. A sandbox that models the
# engine with a guard stood down is modelling a different engine.
DIALECT_TARGET="en-US"
# Declared, so Layer MT exercises the tier gate instead of failing on a blank
# declaration — and so the sandbox models the engine with the capability order
# where it belongs: in data, beside the alias set it must equal.
ALLOWED_MODELS="fable opus sonnet haiku"
MODEL_TIERS="fable > opus > sonnet > haiku"
# Declared, so Layer MC exercises the COST ceiling instead of taking its
# "no ceiling declared" WARN branch. Capability and cost are two different
# orders, and a sandbox declaring only the first models an engine whose newest
# blocking guard is standing down.
MODEL_CEILING="opus"
CFG
}

# gen_sidecars <root> — write a committed-style <hook>.sha256 next to each hook,
# so the sandbox faithfully mirrors the state after install.sh has minted the
# sidecars (the engine keeps them gitignored + regenerated, so a fresh clone runs
# install.sh once — the sandbox seeds them directly).
gen_sidecars() {
    local root="$1" f
    for h in "${ALL_HOOKS[@]}"; do
        f="$root/scripts/hooks/$h"
        [ -f "$f" ] || continue
        shasum -a 256 "$f" | awk '{print $1}' > "$f.sha256"
    done
    for s in "${ALL_ROOT_SCRIPTS[@]}"; do
        f="$root/$s"
        [ -f "$f" ] || continue
        shasum -a 256 "$f" | awk '{print $1}' > "$f.sha256"
    done
    # The derived half, hashed on the same terms. install.sh mints a sidecar for
    # every file it manages and several of these are on that list; a sandbox
    # carrying the file without its sidecar would make the installer's first run
    # a write, and "install is a no-op on a fresh clone" is a case here.
    for s in "${HOOK_DEPENDENCIES[@]}"; do
        f="$root/$s"
        [ -f "$f" ] || continue
        [ -f "$f.sha256" ] && continue
        shasum -a 256 "$f" | awk '{print $1}' > "$f.sha256"
    done
}

# copy_root_scripts <root> — mirror the managed non-hooks/ scripts
# into a sandbox, executable, exactly as a clone would carry them.
copy_root_scripts() {
    local root="$1" s
    for s in "${ALL_ROOT_SCRIPTS[@]}"; do
        mkdir -p "$root/$(dirname "$s")"
        cp "$REAL_REPO_ROOT/$s" "$root/$s"
        chmod +x "$root/$s"
    done
}

# copy_hook_dependencies <root> — the derived half of the same job. A UNION with
# the list above, never a replacement: a file named in both is copied once and
# the hand-written reason for it survives. Directories are created here because
# the derivation may reach a path the skeleton does not already have.
copy_hook_dependencies() {
    local root="$1" d
    for d in "${HOOK_DEPENDENCIES[@]}"; do
        [ -f "$REAL_REPO_ROOT/$d" ] || continue
        mkdir -p "$root/$(dirname "$d")"
        cp "$REAL_REPO_ROOT/$d" "$root/$d"
        case "$d" in
            *.sh|*.py) chmod +x "$root/$d" ;;
        esac
    done
}

# write_sandbox_settings_local <root> <teams-flag-or-empty> <baseref-or-empty>
#
# Writes the sandbox's canonical .claude/settings.local.json wiring EVERY event
# (mirroring the real repo): SessionStart echo + agent-definition snapshotter,
# the whole PreToolUse[Agent] chain DERIVED from hooks/hooks.json in registration
# order (this line said "four-hook chain" while the chain was eight, which is the
# small version of the defect agent_chain() below was written for),
# PreToolUse[Write...] write-guard + secrets
# scanner, PreToolUse[SendMessage] resume-guard, PostToolUse[Agent] detector,
# TeammateIdle + TaskCompleted loggers — PLUS the two critical config keys
# (env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS, worktree.baseRef) parameterized so
# Layer I/J cases can omit either by passing an empty string. Placeholders
# ($CLAUDE_PROJECT_DIR) exactly as committed.
write_sandbox_settings_local() {
    local root="$1" teams_flag="$2" base_ref="$3"
    python3 - "$root/.claude/settings.local.json" "$teams_flag" "$base_ref" \
             "$SCRIPT_DIR/../../hooks/hooks.json" <<'PY'
import json, re, sys
out_path, teams_flag, base_ref, plugin_hooks = sys.argv[1:5]
P = "$CLAUDE_PROJECT_DIR/scripts/hooks"


def agent_chain():
    """The PreToolUse[Agent] chain, DERIVED from hooks/hooks.json.

    THIS USED TO BE EIGHT TYPED ENTRIES, each with a comment explaining that it
    was LAST and that Layer C compares the chain position by position, so a
    sandbox that stopped short would model an engine that cannot pass its own
    probe. Every word of that was true, and it is exactly why the list could not
    stay typed: on 2026-09-14 a ninth hook was registered, this fixture still
    said eight, and the sandbox it built would have modeled an engine that
    cannot pass its own probe.

    The hook SCRIPT SET in this file has been derived from hooks.json since it
    was written ("NEVER TYPED", further up). The chain is the same fact at one
    more level of detail — order — and it is read the same way. A tenth hook
    registered tomorrow is in this sandbox with no edit here.
    """
    with open(plugin_hooks, encoding="utf-8") as fh:
        doc = json.load(fh)
    out = []
    for entry in (doc.get("hooks") or {}).get("PreToolUse", []):
        if "Agent" not in (entry.get("matcher") or ""):
            continue
        for h in entry.get("hooks", []) or []:
            m = re.search(r"scripts/hooks/([A-Za-z0-9._+-]+\.sh)", h.get("command", ""))
            if m:
                out.append({"type": "command",
                            "command": P + "/" + m.group(1),
                            "timeout": h.get("timeout", 10)})
    if not out:
        sys.stderr.write("FATAL: derived an EMPTY PreToolUse[Agent] chain from %s — a "
                         "sandbox with no spawn gates would report every Layer C case "
                         "green against nothing.\n" % plugin_hooks)
        sys.exit(1)
    return out

data = {}
if teams_flag:
    data["env"] = {"CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": teams_flag}
if base_ref:
    data["worktree"] = {"baseRef": base_ref}
data["hooks"] = {
    "SessionStart": [
        {"hooks": [{"type": "command", "command": "echo hi", "timeout": 5}]},
        {"hooks": [{"type": "command", "command": P + "/engine-status.sh", "timeout": 10}]},
        {"hooks": [{"type": "command", "command": P + "/workspace-lifecycle.sh", "timeout": 60}]},
        {"hooks": [{"type": "command", "command": P + "/snapshot-agent-definitions.sh", "timeout": 15}]},
    ],
    # THE WORKSPACE SPEC'S FACTS, on every event the shipped table records them
    # on (docs/plans/worktree-spec-2026-09-11.md). Layer Q1 counts exactly one
    # workspace-lifecycle.sh on each of six events, so a sandbox that wired
    # fewer would model an engine that cannot pass its own probe.
    "SessionEnd": [
        {"hooks": [{"type": "command", "command": P + "/workspace-lifecycle.sh", "timeout": 30}]},
    ],
    "SubagentStart": [
        {"hooks": [{"type": "command", "command": P + "/workspace-lifecycle.sh", "timeout": 30}]},
    ],
    "SubagentStop": [
        {"hooks": [{"type": "command", "command": P + "/workspace-lifecycle.sh", "timeout": 60}]},
    ],
    "PreToolUse": [
        # DERIVED, never typed — see agent_chain() above for what the eight
        # hand-written entries here cost on 2026-09-14.
        {"matcher": "Agent", "hooks": agent_chain()},
        {"matcher": "Write|Edit|MultiEdit|NotebookEdit", "hooks": [
            {"type": "command", "command": P + "/guard-main-checkout-writes.sh", "timeout": 10},
            {"type": "command", "command": P + "/scan-secrets.sh", "timeout": 10},
            # Layer T's subject. Wired LAST here for the same reason it is wired
            # last in the shipped table: the two above decide whether the write
            # is ALLOWED AT ALL, and this one decides what the words say.
            {"type": "command", "command": P + "/guard-dialect.sh", "timeout": 10},
        ]},
        {"matcher": "SendMessage", "hooks": [
            {"type": "command", "command": P + "/guard-resume-isolation.sh", "timeout": 10},
        ]},
        {"matcher": "Bash", "hooks": [
            # FIRST, mirroring the shipped table. Layer IP scans the whole Bash
            # matcher list so order does not decide the layer — it is first
            # because a command that can hang the session should be refused
            # before anything slower has spent time on it.
            {"type": "command", "command": P + "/guard-interactive-prompt.sh", "timeout": 10},
            {"type": "command", "command": P + "/guard-bash-main-writes.sh", "timeout": 10},
            {"type": "command", "command": P + "/guard-worktree-removal.sh", "timeout": 10},
        ]},
        {"matcher": "Workflow", "hooks": [
            {"type": "command", "command": P + "/guard-workflow-ban.sh", "timeout": 10},
        ]},
    ],
    "PostToolUse": [
        {"matcher": "Agent", "hooks": [
            {"type": "command", "command": P + "/detect-nonnative-worktree.sh", "timeout": 10},
        ]},
        {"matcher": "Agent|TaskStop|SendMessage", "hooks": [
            {"type": "command", "command": P + "/workspace-lifecycle.sh", "timeout": 60},
        ]},
    ],
    # THE STOP EVENT, WHICH THIS SANDBOX DID NOT WIRE AT ALL until 2026-09-01.
    # The comment above this function says it mirrors EVERY event; it did not
    # mirror this one, and the cost was invisible in exactly the way a missing
    # sandbox entry always is. The Stop event carries the engine's two BLOCKING
    # turn guards. Neither had a functional probe layer, so nothing noticed that
    # the sandbox modeled an engine whose Stop hooks were copied to disk and
    # registered nowhere -- and a layer written against them would have failed
    # for a reason that has nothing to do with the property it tests.
    #
    # Wired as the FULL shipped list rather than only Layer IL's subject: a
    # sandbox that wires one of nine is a different engine from the one that
    # ships, and the next Stop layer somebody writes would have to discover that
    # for itself.
    "Stop": [
        {"hooks": [{"type": "command", "command": P + "/guard-unresolved-claims.sh", "timeout": 20}]},
        {"hooks": [{"type": "command", "command": P + "/turn-manifest.sh", "timeout": 15}]},
        {"hooks": [{"type": "command", "command": P + "/notice-hook-staleness.sh", "timeout": 15}]},
        {"hooks": [{"type": "command", "command": P + "/notice-inflight-acks.sh", "timeout": 15}]},
        {"hooks": [{"type": "command", "command": P + "/notice-mechanical-findings.sh", "timeout": 15}]},
        {"hooks": [{"type": "command", "command": P + "/notice-unstarted-rows.sh", "timeout": 15}]},
        {"hooks": [{"type": "command", "command": P + "/notice-ceo-unasked.sh", "timeout": 15}]},
        {"hooks": [{"type": "command", "command": P + "/notice-unasked-deferral.sh", "timeout": 15}]},
        {"hooks": [{"type": "command", "command": P + "/guard-agent-state-claims.sh", "timeout": 15}]},
        {"hooks": [{"type": "command", "command": P + "/guard-idle-land.sh", "timeout": 20}]},
        # Point 5's turn-end half. Layer Q1 counts exactly one on Stop.
        {"hooks": [{"type": "command", "command": P + "/guard-workspace-gate.sh", "timeout": 60}]},
    ],
    "TeammateIdle": [
        {"hooks": [{"type": "command", "command": P + "/teammate-idle-handoff.sh", "timeout": 15}]}
    ],
    "TaskCompleted": [
        {"hooks": [{"type": "command", "command": P + "/task-completed-handoff.sh", "timeout": 15}]},
        {"hooks": [{"type": "command", "command": P + "/workspace-lifecycle.sh", "timeout": 30}]},
    ],
}
with open(out_path, "w", encoding="utf-8") as f:
    json.dump(data, f, indent=2)
PY
}

# ---------------------------------------------------------------------------
# THE SANDBOX IS A REPOSITORY, because the hooks in it test that it is one.
#
# "CAN IT START?" IS NOT "CAN IT RUN?" — and until 2026-09-14 this harness only
# ever asked the first question. The sandbox was a plain mktemp directory, `git
# init` appeared nowhere in this file, and nine registered guards answer their
# FIRST question with a variant of
#
#     REPO="$(git -C "$ANCHOR" rev-parse --show-toplevel 2>/dev/null || true)"
#     [ -n "$REPO" ] || exit 0
#
# so in this harness they exited 0 at that line and were never asked anything
# else. SC1 — "every registered hook starts in a sandbox" — was green over
# exactly that, and green is what it printed while
# guard-hook-registration-commits.sh sat in every sandbox without
# scripts/hook-registration-completeness.sh, the file it exits 2 without. The
# guard declined before it could miss it. The demo caught what SC1 could not,
# only because demo.sh performs a real commit in a real repository.
#
# MEASURED, not assumed (2026-09-14, 70 registered hooks, SC1's own six
# payloads, counting distinct traced source lines per hook):
#
#     hooks that execute FURTHER when the sandbox is a repository   12 of 70
#     the other 58                                     decline for reasons of
#                                                      their own, or need no
#                                                      repository at all
#
# and the 12, with the gain in distinct lines reached:
#
#     guard-hook-registration-commits.sh  +17    guard-named-persons-writes.sh  +4
#     guard-ceo-todos-commits.sh           +6    guard-vendoring-commits.sh     +4
#     guard-completeness-commits.sh        +6    notice-unlanded-branches.sh    +3
#     guard-publication-commits.sh         +6    detect-nonnative-worktree.sh   +2
#     guard-publication-writes.sh          +6    teammate-idle-handoff.sh       +1
#     guard-row-currency-commits.sh        +6
#     guard-named-persons-commands.sh      +4
#
# AND IT IS NOT ONLY THE HOOKS. Probe Layer N — ".claude/settings.local.json is
# git-tracked, so the next clone receives it" — answers with a WARN and its own
# skip line ("not a git work tree") outside a repository, which is what it
# emitted in every one of the ~100 sandboxes built here. Sections `worktree` and
# `N` were the only cases that ever exercised it, through make_git_main below.
# Every other probe-running case now puts Layer N on its real branch.
#
# WHAT IS DELIBERATELY NOT HERE. A remote and an upstream were measured too, and
# changed the reach of ZERO hooks of the 70 — so no fake `origin` is invented
# here to make the sandbox look more real than the cases need. An empty repo
# with no commit reached the same lines as one with a commit; the commit is kept
# anyway, because a repository with an unborn HEAD is a state no engine ever
# runs in, and a sandbox that models an impossible repository is the same class
# of error as one that models no repository at all. Both facts are recorded so
# the next reader does not have to re-measure them to widen this.
#
# HERMETIC, because the operator's git configuration is a third-party default
# and this suite runs on workstations and on CI runners. --template= (empty)
# refuses any init template the machine has installed, core.hooksPath is pointed
# at nothing, and identity and branch are pinned locally rather than inherited —
# so a runner with no user.name and a workstation with a global commit template
# build the same sandbox.
#
# FATAL ON FAILURE, for the reason everything else in this file is: a sandbox
# that quietly failed to become a repository would put all 12 of those guards
# straight back behind their jurisdiction test, and every case would go green
# again while testing less. There is no fallback.
# ---------------------------------------------------------------------------
init_sandbox_repo() { # <root>
    local root="$1" top out
    # Captured rather than discarded: git's own message is the only thing that
    # distinguishes "no identity on this runner" from "TMPDIR is not writable",
    # and a harness that swallows it sends whoever is debugging to rebuild the
    # sandbox by hand.
    out="$(
        cd "$root" || exit 1
        # The operator's git configuration is a third-party default, and this
        # suite runs on workstations and on bare CI runners. Both files are
        # replaced rather than merged, and the two machine-level defaults that
        # survive that -- the init template and the XDG ignore file, neither of
        # which lives in the config being replaced -- are refused by name.
        export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
        git -c init.defaultBranch=main init -q --template= . || exit 1
        git config user.email sandbox@richos.invalid || exit 1
        git config user.name "RichOS Sandbox" || exit 1
        git config commit.gpgsign false || exit 1
        # RELATIVE, deliberately: this config is cloned into ~100 sandboxes at
        # other paths, and git resolves a relative hooksPath against the working
        # tree it is run in. An absolute path here would point every clone back
        # at the template, which is deleted when the suite exits.
        git config core.hooksPath .git/no-hooks || exit 1
        # ~/.config/git/ignore is consulted whenever core.excludesFile is unset,
        # so emptying the global CONFIG does not disarm it. This machine's copy
        # carries the '**/.claude/settings.local.json' line the vanilla client
        # suggests, and with it in force `git add -A` skipped the canonical
        # settings file in silence and probe Layer N hard-failed in every
        # sandbox — the exact trap Layer N exists to catch, planted by the
        # harness rather than by the engine.
        git config core.excludesFile /dev/null || exit 1
        git add -A || exit 1
        # Belt and braces, and the same remedy make_git_main uses: whatever
        # ignore rule survives, the canonical source is tracked.
        git add -f .claude/settings.local.json || exit 1
        git commit -q -m "sandbox baseline" || exit 1
    ) 2>&1)" || {
        echo "FATAL: the sandbox template could not be made a git repository — refusing to run a suite whose guards would all decline at their jurisdiction test and report that as green" >&2
        printf '%s\n' "$out" | sed 's/^/  /' >&2
        exit 1
    }
    top="$(git -C "$root" rev-parse --show-toplevel 2>/dev/null || true)"
    if [ "$(cd "$top" 2>/dev/null && pwd -P)" != "$(cd "$root" && pwd -P)" ]; then
        echo "FATAL: the sandbox at $root does not answer as its own repository root (got '${top:-nothing}') — refusing to run a suite that cannot tell which repository its guards are protecting" >&2
        exit 1
    fi
    # ASSERTED, NOT ASSUMED. Layer N is one of the layers this change takes off
    # its skip branch, and it reads git tracking. A sandbox whose canonical
    # settings file failed to be tracked would turn every probe-passes case in
    # this file red for a reason that has nothing to do with what it tests, and
    # the message would point at the engine instead of at this function.
    if ! git -C "$root" ls-files --error-unmatch .claude/settings.local.json >/dev/null 2>&1; then
        echo "FATAL: the sandbox at $root committed without tracking .claude/settings.local.json — probe Layer N would hard-fail in every case, naming the engine for a defect that belongs to this harness" >&2
        exit 1
    fi
}

# ---------------------------------------------------------------------------
# Build a sandbox repo skeleton mirroring a committed fresh clone with the
# sidecars minted: canonical settings.local.json (all events, placeholders) +
# all hook scripts + their .sha256 sidecars + orchestration.config, and NO
# generated settings.json. Returns the absolute path of the new sandbox.
#
# It is a real git repository (init_sandbox_repo above), because twelve of the
# seventy registered hooks decide nothing at all outside one.
# ---------------------------------------------------------------------------
build_sandbox_template() {
    local root _ts0 _ts1 _ts2 _ts3 _ts4 _ts5
    _ts0="$(_t)"
    root="$(mktemp -d "${TMPDIR:-/tmp}/contract-integrity-template.XXXXXX")"
    mkdir -p "$root/scripts/hooks" "$root/scripts/lib" "$root/.claude/state"
    cp "$SCRIPT_DIR/install.sh" "$root/scripts/hooks/"
    cp "$SCRIPT_DIR/contract-integrity-probe.sh" "$root/scripts/hooks/"
    for h in "${ALL_HOOKS[@]}"; do
        cp "$SCRIPT_DIR/$h" "$root/scripts/hooks/"
    done
    cp "$SCRIPT_DIR/../lib/resolve-main-checkout.sh" "$root/scripts/lib/"
    # The guard inventory and the table it derives from. Every real engine
    # ships hooks/hooks.json — a seated engine registers its guards in
    # .claude/settings.local.json AND publishes the same set here for adopters
    # loading it as a plugin, which is exactly the pair Layer R's R4 compares.
    # A sandbox without it was modelling an engine that cannot exist, and it
    # went unnoticed only while nothing read the file.
    mkdir -p "$root/hooks"
    cp "$SCRIPT_DIR/../../hooks/hooks.json" "$root/hooks/hooks.json"
    cp "$SCRIPT_DIR/../lib/registered-hooks.sh" "$root/scripts/lib/"
    cp "$SCRIPT_DIR/../lib/hook_command.py" "$root/scripts/lib/"
    # The sandbox is its own engine root, so it identifies itself like one.
    cp "$SCRIPT_DIR/../../VERSION" "$root/VERSION" 2>/dev/null || printf '0.0.0-sandbox\n' >"$root/VERSION"
    _ts1="$(_t)"
    copy_root_scripts "$root"
    copy_hook_dependencies "$root"
    chmod +x "$root/scripts/hooks/"*.sh 2>/dev/null || true
    _ts2="$(_t)"
    write_sandbox_config "$root"
    write_sandbox_settings_local "$root" "1" "head"
    _ts3="$(_t)"
    gen_sidecars "$root"
    _ts4="$(_t)"
    init_sandbox_repo "$root"
    _ts5="$(_t)"
    _trec template.hooks     "$_ts0" "$_ts1" copy-hooks
    _trec template.root      "$_ts1" "$_ts2" copy-root-scripts
    _trec template.settings  "$_ts2" "$_ts3" write-config-and-settings
    _trec template.sidecars  "$_ts3" "$_ts4" gen-sidecars
    _trec template.git       "$_ts4" "$_ts5" init-repository
    _trec template           "$_ts0" "$_ts5" TOTAL
    echo "$root"
}

# ---------------------------------------------------------------------------
# ONE assembly, eighty-one clones.
#
# Assembling a sandbox costs about 105 subprocess spawns -- one `cp` per hook,
# one per managed root script, one `shasum` per sidecar -- and it was paid once
# per case. Measured on 2026-09-03: 1.05 s a sandbox, ~85 s a run, against 24 ms
# for a copy-on-write clone of the finished article.
#
# NOTHING ABOUT WHAT IS TESTED CHANGES. The clone is byte-identical to the
# template (verified: file hashes, exec bits, the empty .claude/state directory
# and the directory mode all survive), so every sidecar still matches the file
# it was minted from, and APFS clones are independent on write -- a case that
# tampers with its sandbox cannot reach the template or any sibling. That is
# asserted, not assumed: `cp -c` is TRIED once here and the suite says out loud
# which mode it got, because a silent fall back to a slow copy is the kind of
# degradation this repository keeps having to relearn.
#
# The template is built at top level rather than lazily inside make_sandbox,
# because make_sandbox is called in a command substitution -- a subshell -- and
# a template memoized there would be rebuilt, and leaked, 81 times.
# ---------------------------------------------------------------------------
SANDBOX_TEMPLATE="$(build_sandbox_template)"
[ -d "$SANDBOX_TEMPLATE" ] || { echo "FATAL: the sandbox template was not built — refusing to run a suite whose cases would each verify an empty directory" >&2; exit 1; }
trap 'rm -rf "$CLAUDE_CONFIG_DIR" "$SANDBOX_TEMPLATE"' EXIT

_CLONE_PROBE="$(mktemp -d "${TMPDIR:-/tmp}/contract-integrity-clonetest.XXXXXX")"
if cp -c -R "$SANDBOX_TEMPLATE/." "$_CLONE_PROBE/" 2>/dev/null; then
    SANDBOX_COPY_FLAGS="-c -R"
else
    cp -R "$SANDBOX_TEMPLATE/." "$_CLONE_PROBE/"
    SANDBOX_COPY_FLAGS="-R"
    echo "note: copy-on-write clones are unavailable under ${TMPDIR:-/tmp} — using a plain recursive copy. Identical sandboxes, slower run." >&2
fi
rm -rf "$_CLONE_PROBE"

# Clone the template into an EXISTING directory (so the caller keeps the
# mktemp -d name it already holds, and there is no unlink/recreate window).
clone_sandbox_into() { # <existing-directory>
    cp $SANDBOX_COPY_FLAGS "$SANDBOX_TEMPLATE/." "$1/"
}

make_sandbox() {
    local root _ts0 _ts1
    _ts0="$(_t)"
    root="$(mktemp -d "${TMPDIR:-/tmp}/contract-integrity.XXXXXX")"
    clone_sandbox_into "$root"
    _ts1="$(_t)"
    _trec sandbox "$_ts0" "$_ts1" TOTAL
    echo "$root"
}

# Build a resolved, hook-DUPLICATING settings.json at <root> (the OLD broken
# state the pre-fix install.sh produced): a $CLAUDE_PROJECT_DIR-resolved copy of
# settings.local.json, which Claude Code would merge → every hook fires twice.
build_dup_settings_json() { # <root>
    python3 - "$1" <<'PY'
import json, sys
root = sys.argv[1]
with open(f"{root}/.claude/settings.local.json") as f: d = json.load(f)
def walk(o):
    if isinstance(o, dict): return {k: walk(v) for k, v in o.items()}
    if isinstance(o, list): return [walk(v) for v in o]
    if isinstance(o, str): return o.replace("$CLAUDE_PROJECT_DIR", root)
    return o
with open(f"{root}/.claude/settings.json", "w") as f:
    json.dump(walk(d), f, indent=2)
PY
}

# Both helpers DECLARE the sandbox as the repository under verification.
#
# Before the root-resolution contract, running "$ROOT/scripts/hooks/<x>.sh"
# implicitly meant "verify $ROOT", because the script derived its root from its
# own location. It no longer does — deliberately — so the intent has to be
# stated. Without it these cases verified whatever repository the harness
# happened to be launched from, and every wired path would be compared against
# the wrong canonical location.
# The `shift` is load-bearing: without it "$@" still holds $1, so the sandbox
# ROOT was passed to install.sh as a positional argument on every call. It was
# harmless only because install.sh ignored arguments entirely — and it stopped
# being harmless the moment install.sh grew --force-engine-pointer and began
# REFUSING unrecognized ones, which is exactly what strict argument parsing is
# for. The forwarding itself is kept, since forwarding extra flags is plainly
# what this line was reaching for.
run_install_in() {
    local _root="$1"; shift
    RICHOS_ENTITY_ROOT="$_root" "$_root/scripts/hooks/install.sh" "$@" 2>&1
}
run_probe_in() {
    # CI_PROBE_DEBUG=1 echoes the probe's own ✗ lines to stderr. A harness that
    # reports "expected exit=0 got=2" and nothing else forces whoever is
    # debugging to rebuild the sandbox by hand — which is how a real finding
    # gets written off as "the harness is fiddly".
    if [ -n "${CI_PROBE_DEBUG:-}" ]; then
        RICHOS_ENTITY_ROOT="$1" "$1/scripts/hooks/contract-integrity-probe.sh" 2>&1 | tee /dev/stderr
    else
        RICHOS_ENTITY_ROOT="$1" "$1/scripts/hooks/contract-integrity-probe.sh" 2>&1
    fi
}

# emit_mutation_case <name> <rc> <log> — a mutation harness's verdict. A harness
# that exits 0 having printed NOT RUN (RICHOS_MUTATION_PASSES=0, the merge gate)
# proved nothing: it is reported NOT RUN and counted apart, never as a PASS that
# says every property is load-bearing (hunt part 3 v3, finding 14).
emit_mutation_case() {
    local name="$1" actual="$2" log="$3"
    if [ "$actual" = "0" ] && grep -q '^NOT RUN' "$log" 2>/dev/null; then
        _trec case "$(_t)" - "$name"
        NOT_RUN=$((NOT_RUN+1))
        NOT_RUN_NAMES+=("$name")
        printf '  NOT RUN  %s  (%s)\n' "$name" "$(grep -m1 '^NOT RUN' "$log")"
        return 0
    fi
    emit_case "$name" 0 "$actual"
}

emit_case() {
    local name="$1" expected="$2" actual="$3"
    _trec case "$(_t)" - "$name"
    if [ "$expected" = "$actual" ]; then
        PASS=$((PASS+1))
        printf '  PASS  %s\n' "$name"
    else
        FAIL=$((FAIL+1))
        FAIL_NAMES+=("$name")
        printf '  FAIL  %s  (expected exit=%s got=%s)\n' "$name" "$expected" "$actual"
    fi
}

# suite_discovered <case-name> <engine-relative suite path> — the suite is a
# file AND the engine runner's own discovery lists it.
#
# This replaces nine NESTED RUNS of whole suites (hunt 2026-09-29 part 3,
# finding 10). Each was added when nothing else ran that suite: "a harness
# nothing runs is a harness that rots". That reason still holds and is kept here
# as the thing actually checked, but the means changed: scripts/run-all-tests.sh
# and scripts/ci-units.sh now discover EVERY *.test.sh under the engine and run
# each as its own unit (ci-units.test.sh proves the two lists are identical). So
# the question this suite can still answer that the runner cannot is "is this
# suite ON that list", and running it a second time, inside a 39-minute
# sectioned suite, answers nothing the suite's own unit has not already.
suite_discovered() {
    local name="$1" rel="$2" rc=1
    if [ -z "${CI_DISCOVERED_SUITES+x}" ]; then
        CI_DISCOVERED_SUITES="$(bash "$SCRIPT_DIR/../ci-units.sh" suites 2>/dev/null || true)"
    fi
    if [ -f "$SCRIPT_DIR/../../$rel" ] && printf '%s\n' "$CI_DISCOVERED_SUITES" | grep -qxF "$rel"; then
        rc=0
    fi
    emit_case "$name" 0 "$rc"
}

# ---------------------------------------------------------------------------
# diag_or_tail — the FLOOR under every capture filter in this file.
#
# Every filter below is a PREDICTION about the shape its harness fails in, and
# an unpredicted failure is precisely the thing that violates it. CL2 and SA2
# were moved onto the exclusion form for that reason. The filters that remain
# POSITIVE cannot follow them: MT5/MC5 read probe output, which is 114 lines of
# ✓/✗ with no PASS row anywhere in it, so "print everything that is not a PASS
# row" would tip all 114 lines into the report and bury the one finding it was
# supposed to surface. Flooding hides a cause exactly as well as silence does,
# which is why this is not a fifteen-site unification: the harnesses genuinely
# differ, and each one keeps its own filter.
#
# Measured, not assumed — each of these run through the filters as they stand:
#
#     FATAL: python3 required
#     FATAL: mut_pool_init: could not create a pool directory
#     MUTATION TARGET ABSENT — the source has drifted:
#     <script>: line 42: yq: command not found
#     <script>: line 99: MUT_POOL_DIR: unbound variable
#     Traceback (most recent call last):
#     <script>: line 12: syntax error near unexpected token
#     WARNING: RICHOS_MUTANT_JOBS='x' is not a positive integer
#
# The exclusion form prints 8 of 8. The `^ *(FAIL|PASS|UNPROVEN)|...` allowlist
# prints 0 of 8, and so do `grep -E 'MT\.'` and `grep -E 'MC\.'`. Note that
# `FATAL` does not match `^ *FAIL` — the two words differ at the third letter,
# which is the entire reason this class of failure reports nothing.
#
# So: keep the narrow filter, and put a bounded floor under it. This CANNOT
# change any output that exists today. When the filter matches anything at all
# this prints exactly that and nothing else; the fallback reaches only the case
# whose output is empty right now — the silent one.
#
#   $1  the filtered diagnostic (may be empty)
#   $2  the raw captured output that the filter ran over
DIAG_TAIL_N=8
diag_or_tail() {
    if [ -n "$1" ]; then
        printf '%s\n' "$1" | sed 's/^/        /'
        return 0
    fi
    if [ -n "$2" ]; then
        printf '        (filter matched nothing — last %s lines of raw output)\n' "$DIAG_TAIL_N"
        printf '%s\n' "$2" | tail -n "$DIAG_TAIL_N" | sed 's/^/        /'
    else
        printf '        (the harness produced no output at all)\n'
    fi
}

# Helper: mutate ONLY the write-guard's (first) command in the Write|Edit
# matcher entry, leaving scan-secrets.sh's (second) command untouched — the
# matcher wires TWO hooks, and these cases are specifically about Layer B's
# write-guard, not Layer K's secrets scanner.
set_guard_command() { # <settings-file> <new-command>
    python3 - "$1" "$2" <<'PY'
import json, sys
p, cmd = sys.argv[1], sys.argv[2]
with open(p) as f: d = json.load(f)
for entry in d["hooks"]["PreToolUse"]:
    if "Write" in entry.get("matcher", ""):
        if entry["hooks"]:
            entry["hooks"][0]["command"] = cmd
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
}

_trec run "$(_t)" - START
echo "=== contract-integrity.test.sh ==="
if [ "$SCOPED" -eq 1 ]; then
    echo "################################################################################"
    echo "#  SCOPED RUN — THIS IS NOT A FULL PASS AND MUST NOT BE READ AS ONE."
    echo "#  sections in scope:$([ -n "$SCOPE_RUN" ] && printf ' %s' "$SCOPE_RUN")"
    echo "#  A land needs the full suite. Run with no arguments for that."
    echo "################################################################################"
fi
echo ""

# ---------------------------------------------------------------------------
# Canonical-source baseline
# ---------------------------------------------------------------------------

if _section base; then
# Case 1 — committed fresh clone with sidecars minted (settings.local.json +
# hooks + sidecars, NO settings.json, NO install.sh run this case) → probe
# PASSES. The committed source alone wires enforcement; the sidecars are seeded.
ROOT="$(make_sandbox)"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "1.committed-source-passes" 0 "$rc"
rm -rf "$ROOT"

# ---------------------------------------------------------------------------
# C10 — A TENTH PreToolUse[Agent] HOOK IS NOT A DEFECT.
#
# Every other Layer C case in this file mutates the chain and asserts a
# REFUSAL. None of them asked the question an engine that expects to grow has
# to answer: DOES A CORRECTLY ADDED HOOK PASS? On 2026-09-14 the answer was no.
# A ninth Agent hook was registered, correctly, and Layer C refused it — its
# expectation was a typed array of eight paths, and the remedy it printed ("run
# scripts/hooks/install.sh") could not change either number.
#
# So: register a tenth on BOTH surfaces the way a tenth is really added, and
# require a green. Layer C now derives its expectation from hooks/hooks.json,
# so it has nothing to fall behind.
#
# C10b is the negative control, and without it C10 proves nothing: a chain the
# layer never looked at is also green. It seats the tenth hook WITHOUT
# registering it, which is the half-registration that was live on `main` for
# hours with nothing in the seated route able to see it, and requires Layer C to
# name the disagreement.
# ---------------------------------------------------------------------------
add_tenth_agent_hook() { # <sandbox root> <seat?> <register?>
    local root="$1" seat="$2" register="$3"
    # THE FIXTURE CARRIES A REAL ROOT BOOTSTRAP, copied from a real rooted hook
    # rather than retyped. It used to be a bare `exit 0`, which was fine while
    # Layer R walked a typed list this synthetic name was never in. Layer R now
    # derives the hooks it walks from the registration, so a registered hook
    # that resolves its root any other way is named — correctly, and by this
    # fixture too. Copying rather than retyping matters because R3 compares the
    # block byte for byte.
    {
        echo '#!/usr/bin/env bash'
        echo '# A synthetic TENTH PreToolUse[Agent] gate. It decides nothing — this case is'
        echo '# about whether the engine'"'"'s own inventories tolerate its existence.'
        echo 'set -euo pipefail'
        echo ''
        sed -n '/^# --- ROOT RESOLUTION ---/,/^ENGINE_ROOT="\$(resolve_engine_root/p' \
            "$root/scripts/hooks/notice-unlanded-branches.sh" \
            | sed 's|scripts/hooks/notice-unlanded-branches\.sh|scripts/hooks/guard-tenth-example.sh|'
        echo 'exit 0'
    } >"$root/scripts/hooks/guard-tenth-example.sh"
    chmod +x "$root/scripts/hooks/guard-tenth-example.sh"
    shasum -a 256 "$root/scripts/hooks/guard-tenth-example.sh" \
        | awk '{print $1}' >"$root/scripts/hooks/guard-tenth-example.sh.sha256"
    python3 - "$root" "$seat" "$register" <<'PY'
import json, sys
root, seat, register = sys.argv[1:4]


def append(path, command, placeholder_ok):
    with open(path, encoding="utf-8") as fh:
        d = json.load(fh)
    for entry in d["hooks"]["PreToolUse"]:
        if "Agent" in (entry.get("matcher") or ""):
            entry["hooks"].append({"type": "command", "command": command, "timeout": 10})
            break
    else:
        sys.stderr.write("C10: no Agent matcher in %s\n" % path)
        sys.exit(3)
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(d, fh, indent=2)


if seat == "1":
    append(root + "/.claude/settings.local.json",
           "$CLAUDE_PROJECT_DIR/scripts/hooks/guard-tenth-example.sh", True)
if register == "1":
    append(root + "/hooks/hooks.json",
           "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/guard-tenth-example.sh", True)
PY
}

ROOT="$(make_sandbox)"
add_tenth_agent_hook "$ROOT" 1 1
set +e; C10_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
if [ "$rc" -ne 0 ]; then
    printf '        a correctly added tenth Agent hook was refused:\n%s\n' \
        "$(printf '%s\n' "$C10_OUT" | grep '✗' | head -3)" >&2
fi
emit_case "C10.a-tenth-agent-hook-seated-and-registered-passes" 0 "$rc"
# Green is not enough: a layer that skipped the new hook is also green. It has
# to have been SEEN.
if printf '%s\n' "$C10_OUT" | grep >/dev/null "guard-tenth-example.sh"; then
    emit_case "C10a.the-tenth-hook-is-named-by-Layer-C-not-silently-skipped" 0 0
else
    printf '        the probe was green but never named guard-tenth-example.sh\n' >&2
    emit_case "C10a.the-tenth-hook-is-named-by-Layer-C-not-silently-skipped" 0 1
fi
rm -rf "$ROOT"

ROOT="$(make_sandbox)"
add_tenth_agent_hook "$ROOT" 1 0
set +e; C10_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
if [ "$rc" -eq 2 ] && printf '%s\n' "$C10_OUT" | grep >/dev/null "C\..*MISMATCH between the two registration surfaces"; then
    emit_case "C10b.NEGATIVE-CONTROL-seated-but-unregistered-is-named-by-Layer-C" 0 0
else
    printf '        expected Layer C to report a surface mismatch, rc=%s, got: [%s]\n' \
        "$rc" "$(printf '%s\n' "$C10_OUT" | grep '✗' | head -3)" >&2
    emit_case "C10b.NEGATIVE-CONTROL-seated-but-unregistered-is-named-by-Layer-C" 0 1
fi
rm -rf "$ROOT"

# ---------------------------------------------------------------------------
# R10 — A NEWLY REGISTERED ROOTED HOOK IS WALKED BY LAYER R WITH NO EDIT TO IT.
#
# Layer R used to hold a TYPED R_ROOTED_HOOKS and walk only the names in it, so
# a hook absent from that list was not red — it was UNCHECKED. That is not a
# hypothetical: measured 2026-09-14 against hooks/hooks.json, 57 registered
# hooks assign ENGINE_ROOT from resolve_engine_root and the list named 52, and
# one of the five it missed (guard-stop-live-work.sh) had been carrying a
# bootstrap that diverges from its siblings the whole time. Nothing was red.
#
# The membership is now derived from the registration, so this case asks the
# question the old shape could not answer: does a correctly added rooted hook
# get CHECKED, rather than merely tolerated?
#
# R10b IS THE CASE THAT PROVES IT AND R10 ALONE PROVES NOTHING. A probe that
# never looked at the new hook is green too — which is exactly what the typed
# list did for five hooks. So R10b breaks the new hook's bootstrap the way a
# real regression does, reverting it to trusting its own on-disk location, and
# REQUIRES Layer R to name it. Run against the typed list this same mutant is
# silent, because the mutant's name was never in it.
# ---------------------------------------------------------------------------
add_rooted_stop_hook() { # <sandbox root> <register?> <bootstrap?>
    local root="$1" register="$2" bootstrap="$3"
    local donor="$root/scripts/hooks/notice-unlanded-branches.sh"
    local target="$root/scripts/hooks/notice-rooted-example.sh"

    if [ ! -f "$donor" ]; then
        echo "R10: donor hook missing from the sandbox: $donor" >&2
        return 3
    fi

    {
        echo '#!/usr/bin/env bash'
        echo '# A synthetic rooted Stop hook. It decides nothing — this case is about'
        echo '# whether Layer R walks a hook nobody added to a list.'
        echo 'set -euo pipefail'
        echo ''
        if [ "$bootstrap" = "1" ]; then
            # Copied from a real rooted hook rather than retyped, because R3
            # compares the block BYTE FOR BYTE and a hand-copy that drifts would
            # make this case fail for a reason unrelated to what it tests. Only
            # the hook's own name in the diagnostic changes, which is the one
            # substitution Layer R normalizes out before comparing.
            sed -n '/^# --- ROOT RESOLUTION ---/,/^ENGINE_ROOT="\$(resolve_engine_root/p' "$donor" \
                | sed 's|scripts/hooks/notice-unlanded-branches\.sh|scripts/hooks/notice-rooted-example.sh|'
        else
            # THE MUTANT: the same hook with the resolver dropped, back to
            # deriving its root from where its own file happens to sit. This is
            # the regression Layer R exists to catch and it has no other symptom.
            echo 'SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"'
            echo 'ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"'
        fi
        echo 'exit 0'
    } >"$target"

    chmod +x "$target"
    shasum -a 256 "$target" | awk '{print $1}' >"$target.sha256"

    [ "$register" = "1" ] || return 0
    python3 - "$root" <<'PY'
import json, sys

root = sys.argv[1]
for path, command in (
    (root + "/.claude/settings.local.json",
     "$CLAUDE_PROJECT_DIR/scripts/hooks/notice-rooted-example.sh"),
    (root + "/hooks/hooks.json",
     "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/notice-rooted-example.sh"),
):
    with open(path, encoding="utf-8") as fh:
        d = json.load(fh)
    stop = d["hooks"].get("Stop")
    if not stop:
        sys.stderr.write("R10: no Stop entries in %s\n" % path)
        sys.exit(3)
    stop[-1]["hooks"].append({"type": "command", "command": command, "timeout": 10})
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(d, fh, indent=2)
PY
}

ROOT="$(make_sandbox)"
add_rooted_stop_hook "$ROOT" 1 1
set +e; R10_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
if [ "$rc" -ne 0 ]; then
    printf '        a correctly added rooted Stop hook was refused:\n%s\n' \
        "$(printf '%s\n' "$R10_OUT" | grep '✗' | head -3)" >&2
fi
emit_case "R10.a-newly-registered-rooted-hook-passes-with-no-edit-to-Layer-R" 0 "$rc"
rm -rf "$ROOT"

ROOT="$(make_sandbox)"
add_rooted_stop_hook "$ROOT" 1 0
set +e; R10_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
if [ "$rc" -eq 2 ] && printf '%s\n' "$R10_OUT" \
        | grep >/dev/null "R\..*do NOT source the root-resolution contract.*notice-rooted-example"; then
    emit_case "R10b.NEGATIVE-CONTROL-an-unlisted-rooted-hook-that-drops-the-resolver-is-NAMED" 0 0
else
    printf '        expected Layer R to name notice-rooted-example, rc=%s, got: [%s]\n' \
        "$rc" "$(printf '%s\n' "$R10_OUT" | grep '✗' | head -3)" >&2
    emit_case "R10b.NEGATIVE-CONTROL-an-unlisted-rooted-hook-that-drops-the-resolver-is-NAMED" 0 1
fi
rm -rf "$ROOT"

# ---------------------------------------------------------------------------
# SANDBOX COMPLETENESS — the case that makes the list above unable to fall
# behind silently.
#
# EVERY case in this file builds its sandbox from ALL_HOOKS + ALL_ROOT_SCRIPTS,
# and every case's exit code is only worth reading if the engine in that sandbox
# can actually be assembled. On 2026-08-31 it could not: five guards landed and
# the list did not grow, so scan-secrets.sh and guard-main-checkout-writes.sh
# could not start, and BOTH refuse by exiting 2 — the same number Layers K and D
# were asserting as proof that they had run and caught something. Two dead
# guards, two green layers, for as long as the list had existed.
#
# So the list is no longer trusted to be complete: it is ASKED. SC1 starts every
# registered hook in a real sandbox and fails naming any that announced a
# missing file. It is answered by RUNNING the hooks, so it holds for a
# dependency expressed in any form — sourced library, sibling .py, data file,
# path assembled at runtime. It found scripts/lib/inflight.sh, which reading the
# night's diff had not.
#
# SC2 is its negative control, and it is not optional: SC1 reports a problem by
# printing NOTHING, which is the same output a check that scanned zero hooks
# produces. SC2 deletes seat-jurisdiction.sh — the exact file whose absence
# started this — and requires SC1's machinery to name the guard that then cannot
# start. Without it, SC1 is a clean bill of health signed by nobody.
# ---------------------------------------------------------------------------
ROOT="$(make_sandbox)"
set +e; SC_OUT="$(richos_sandbox_start_failures "$ROOT" "${ALL_HOOKS[@]}")"; set -e
if [ -z "$SC_OUT" ]; then
    emit_case "SC1.every-registered-hook-starts-in-a-sandbox" 0 0
else
    printf '        sandbox is missing what these hooks need:\n%s\n' "$SC_OUT" >&2
    emit_case "SC1.every-registered-hook-starts-in-a-sandbox" 0 1
fi
rm -rf "$ROOT"

ROOT="$(make_sandbox)"
rm -f "$ROOT/scripts/lib/seat-jurisdiction.sh"
set +e; SC_OUT="$(richos_sandbox_start_failures "$ROOT" "${ALL_HOOKS[@]}")"; set -e
if printf '%s' "$SC_OUT" | grep >/dev/null '^scan-secrets\.sh'; then
    emit_case "SC2.NEGATIVE-CONTROL-deleted-lib-is-named-by-SC1" 0 0
else
    printf '        expected scan-secrets.sh to be reported unable to start, got: [%s]\n' "$SC_OUT" >&2
    emit_case "SC2.NEGATIVE-CONTROL-deleted-lib-is-named-by-SC1" 0 1
fi
rm -rf "$ROOT"

# Case 2 — install.sh (no settings.json to migrate) + probe → both exit 0.
ROOT="$(make_sandbox)"
set +e; run_install_in "$ROOT" >/dev/null; rc1=$?; set -e
set +e; run_probe_in "$ROOT" >/dev/null; rc2=$?; set -e
emit_case "2.install-noop-rc-0" 0 "$rc1"
emit_case "2.post-install-probe-rc-0" 0 "$rc2"
rm -rf "$ROOT"

# Case 3 — drop the write-guard entry from the CANONICAL file → probe exits 2.
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
pre = d.get("hooks", {}).get("PreToolUse", [])
d["hooks"]["PreToolUse"] = [e for e in pre if "Write" not in e.get("matcher", "")]
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "3.hook-removed-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 4 — canonical wiring points at a non-existent script → probe exits 2.
ROOT="$(make_sandbox)"
set_guard_command "$ROOT/.claude/settings.local.json" "/nonexistent/path/to/guard-main-checkout-writes.sh"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "4.wrong-path-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 5 — install.sh is idempotent (sidecars byte-stable across runs) and the
# probe still passes.
ROOT="$(make_sandbox)"
"$ROOT/scripts/hooks/install.sh" >/dev/null
HASH1="$(shasum -a 256 "$ROOT/scripts/hooks/guard-main-checkout-writes.sh.sha256" | awk '{print $1}')"
"$ROOT/scripts/hooks/install.sh" >/dev/null
HASH2="$(shasum -a 256 "$ROOT/scripts/hooks/guard-main-checkout-writes.sh.sha256" | awk '{print $1}')"
if [ "$HASH1" = "$HASH2" ]; then
    PASS=$((PASS+1)); printf '  PASS  5.double-install-idempotent\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("5.double-install-idempotent"); printf '  FAIL  5.double-install-idempotent  (sidecar hash drifted)\n'
fi
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "5.double-install-probe-still-rc-0" 0 "$rc"
rm -rf "$ROOT"

# Case 6 — install.sh fails when canonical settings.local.json is absent.
ROOT="$(make_sandbox)"
rm "$ROOT/.claude/settings.local.json"
set +e; run_install_in "$ROOT" >/dev/null 2>&1; rc=$?; set -e
emit_case "6.install-source-missing-rc-2" 2 "$rc"
rm -rf "$ROOT"

# Case 7 — install.sh may write the `hooks` KEY of the canonical
# settings.local.json, and NOTHING ELSE about that file.
#
# ===========================================================================
# WHY THIS CASE CHANGED, 2026-09-15, AND WHY THE OLD ONE WAS NOT WRONG
# ===========================================================================
# It used to assert that install.sh leaves settings.local.json BYTE-IDENTICAL.
# That was correct for every install.sh that had ever existed, and `3b76bf86`
# ("The seated hook surface is derived from the registration, not retyped")
# deliberately ended it: the seated surface's `hooks` key is now GENERATED from
# hooks/hooks.json, because it was a 500-line hand-maintained second copy of a
# table already in the tree, 74 of whose 76 entries were an exact transform of
# it and whose other two were drift rather than intent.
#
# So this suite was asserting a property install.sh no longer has, on purpose,
# and `main` was red for a contradiction rather than for a defect. The case is
# OBSOLETE, not violated — and the fix is not to delete it or to weaken it to
# "install.sh may write", which would stop protecting the thing it was written
# to protect.
#
# THE EVIDENCE THIS REPLACEMENT IS BUILT ON, measured rather than assumed. A
# plain copy of the real engine, install.sh run over it: output `✓ seated hook
# surface already matches hooks/hooks.json`, and the before/after diff of
# settings.local.json IDENTICAL. The generator is a no-op against a correct
# tree. This SANDBOX differs because build_sandbox_template synthesizes an
# approximate settings.local.json while copying the REAL hooks/hooks.json, so
# the two disagree by construction. Dumping both sides of that write:
#
#   top-level keys before: env, hooks, worktree
#   top-level keys after : env, hooks, worktree      (none added, none removed)
#   keys whose value changed: hooks                  (env and worktree untouched)
#
# and the `hooks` change was a CORRECTION — the fixture had no UserPromptSubmit
# event at all and fewer PreToolUse/PostToolUse/SessionStart groups than the
# registration it is supposed to mirror.
#
# ===========================================================================
# THE REPLACEMENT IS STRICTLY STRONGER THAN WHAT IT REPLACES
# ===========================================================================
# The old case pinned one property: nothing changes. This pins five, and a
# future install.sh that rewrites anything OTHER than the generated hooks key
# still fails here:
#
#   7a  the key SET is unchanged — install.sh may not add or drop a key.
#   7b  every key except `hooks` is deep-equal. `env.CLAUDE_CODE_EXPERIMENTAL_
#       AGENT_TEAMS` and `worktree.baseRef` are called out in this engine's own
#       doctrine as being as load-bearing as any hook; an install.sh that
#       reformatted them away would have been invisible to "byte-identical
#       except hooks" stated loosely, and is caught here by name.
#   7c  the $CLAUDE_PROJECT_DIR placeholders SURVIVE. This is the specific
#       property the old case's comment named, and it is the one a generator is
#       most likely to break: writing a resolved absolute path produces a file
#       that works on the machine that generated it and nowhere else.
#   7d  the generated hooks AGREE with hooks/hooks.json — the same script set,
#       derived from the registration rather than from a list typed here, so a
#       generator that silently dropped a hook fails. Compared as SETS OF
#       SCRIPT NAMES and not as strings, deliberately: re-implementing the
#       transform here would make this case agree with install.sh by
#       construction, which is the shape of test that cannot fail.
#   7e  it is IDEMPOTENT. A second run changes nothing. Without this a
#       non-deterministic generator would leave every checkout dirty after
#       every install and nothing would say so.
ROOT="$(make_sandbox)"
cp "$ROOT/.claude/settings.local.json" "$ROOT/settings.before.json"
"$ROOT/scripts/hooks/install.sh" >/dev/null 2>&1
cp "$ROOT/.claude/settings.local.json" "$ROOT/settings.after1.json"
"$ROOT/scripts/hooks/install.sh" >/dev/null 2>&1
cp "$ROOT/.claude/settings.local.json" "$ROOT/settings.after2.json"
C7_OUT="$(python3 - "$ROOT" <<'PY'
import json, os, sys

root = sys.argv[1]


def load(name):
    with open(os.path.join(root, name), encoding="utf-8") as fh:
        return json.load(fh)


before, after1, after2 = load("settings.before.json"), load("settings.after1.json"), \
    load("settings.after2.json")
with open(os.path.join(root, "hooks", "hooks.json"), encoding="utf-8") as fh:
    plugin = json.load(fh)

bad = []

# 7a — the key set.
if set(before) != set(after1):
    bad.append("install.sh changed the KEY SET of settings.local.json: added %s, removed %s"
               % (sorted(set(after1) - set(before)), sorted(set(before) - set(after1))))

# 7b — everything except `hooks` is untouched.
for key in sorted(set(before) | set(after1)):
    if key == "hooks":
        continue
    if before.get(key) != after1.get(key):
        bad.append("install.sh rewrote %r, which is NOT the generated hooks key. Only `hooks` is "
                   "derived; every other key is committed source." % key)


def commands(doc):
    out = []
    for entries in (doc.get("hooks") or {}).values():
        for group in entries:
            for hook in group.get("hooks") or []:
                if hook.get("command"):
                    out.append(hook["command"])
    return out


# 7c — the placeholder survives, and no absolute path leaks in.
#
# APPLIED ONLY TO COMMANDS THAT INVOKE A SCRIPT. Not every registered hook is a
# script invocation — several are inline `echo '{"hookSpecificOutput":...}'`
# one-liners that carry no path at all and correctly have no placeholder. The
# first version of this check asserted the placeholder on EVERY command and
# failed on one of those, which is a test failing for a reason that is not a
# defect. The property being protected is narrower and exact: wherever a hook
# names a script under scripts/hooks, that path is reached through
# $CLAUDE_PROJECT_DIR and never through a resolved one.
for cmd in commands(after1):
    for tok in cmd.split():
        if "scripts/hooks/" not in tok:
            continue
        if not tok.startswith("$CLAUDE_PROJECT_DIR") and "$CLAUDE_PROJECT_DIR" not in tok:
            bad.append("a generated hook reaches a script WITHOUT the $CLAUDE_PROJECT_DIR "
                       "placeholder, so it is pinned to one machine: %s" % tok[:120])
            break
    if bad and bad[-1].startswith("a generated hook reaches"):
        break
for cmd in commands(after1):
    if root in cmd:
        bad.append("a generated hook command RESOLVED the placeholder to this checkout's absolute "
                   "path, which would not work anywhere else: %s" % cmd[:120])
        break


# 7d — the generated set agrees with the registration it is derived from.
def scripts_of(cmds):
    return {os.path.basename(tok) for cmd in cmds for tok in cmd.split()
            if tok.endswith(".sh") or tok.endswith(".py")}


seated, registered = scripts_of(commands(after1)), scripts_of(commands(plugin))
if seated != registered:
    bad.append("the generated hooks do not agree with hooks/hooks.json — missing from the seated "
               "surface: %s; present there but not registered: %s"
               % (sorted(registered - seated) or "none", sorted(seated - registered) or "none"))

# 7e — idempotent.
#
# THE DIFFERENCE IS PRINTED, not just asserted. "not idempotent" sends the
# reader back to run the thing again by hand to find out what moved, and a
# 130-second suite is a bad instrument for that. The keys that differ are
# named here, where the failure already is.
if after1 != after2:
    moved = []
    h1, h2 = after1.get("hooks") or {}, after2.get("hooks") or {}
    for ev in sorted(set(h1) | set(h2)):
        if h1.get(ev) != h2.get(ev):
            moved.append("hooks.%s (%d -> %d matcher group(s))"
                         % (ev, len(h1.get(ev) or []), len(h2.get(ev) or [])))
    for key in sorted(set(after1) | set(after2)):
        if key != "hooks" and after1.get(key) != after2.get(key):
            moved.append(key)
    bad.append("install.sh is NOT idempotent: a second run changed settings.local.json again, so "
               "every checkout would be left dirty by every install. What moved: %s"
               % (", ".join(moved) or "nothing this check can name"))

print("\n".join(bad))
PY
)"
if [ -z "$C7_OUT" ]; then
    PASS=$((PASS+1)); printf '  PASS  7.install-writes-only-the-generated-hooks-key\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("7.install-writes-only-the-generated-hooks-key")
    printf '  FAIL  7.install-writes-only-the-generated-hooks-key\n'
    printf '%s\n' "$C7_OUT" | sed 's/^/          /'
fi
rm -rf "$ROOT"

# Case 7m — and case 7 above can still FAIL, clause by clause.
#
# The moment a case stops asserting one thing and starts asserting five, it
# also becomes much easier to write in a way that cannot fail — and a case that
# flags nothing passes on every build, looks like coverage, and is worth less
# than no case because it REPORTS coverage that is not there. The harness
# extracts case 7's own assertion block out of this file and drives it over
# fixtures that violate exactly one clause each, with a positive control so an
# assertion that refused everything could not pass itself off as thorough.
#
# Under a second, because it needs no sandbox: the clauses are about the
# CONTENT of the file install.sh writes, and content can be crafted.
C7M_LOG="$(mktemp "${TMPDIR:-/tmp}/case7-mutations.XXXXXX")"
set +e; "$SCRIPT_DIR/contract-integrity-case7.mutation.sh" >"$C7M_LOG" 2>&1; rc=$?; set -e
emit_mutation_case "7m.install-write-scope-mutations-all-load-bearing" "$rc" "$C7M_LOG"
if [ "$rc" -ne 0 ]; then
    diag_or_tail "$(grep -E '^ *(FAIL|PASS)|^=== |NOT load-bearing|FATAL' "$C7M_LOG" \
        | grep -vE '^ *PASS' || true)" "$(cat "$C7M_LOG" 2>/dev/null || true)"
fi
rm -f "$C7M_LOG"

# ---------------------------------------------------------------------------
# Layer M — registration uniqueness (the double-fire root-fix)
# ---------------------------------------------------------------------------

fi  # _section — base

if _section M; then
# Case M1 — a stale hook-duplicating settings.json (the OLD broken state) is
# MERGED with settings.local.json → probe exits 2 (Layer M detects double-fire).
ROOT="$(make_sandbox)"
build_dup_settings_json "$ROOT"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "M1.duplicated-registration-fails" 2 "$rc"
rm -rf "$ROOT"

# Case M2 — install.sh MIGRATES the duplicated state: it removes the pure-
# duplicate settings.json, and the probe then PASSES (converges to single-fire).
ROOT="$(make_sandbox)"
build_dup_settings_json "$ROOT"
"$ROOT/scripts/hooks/install.sh" >/dev/null
if [ ! -f "$ROOT/.claude/settings.json" ]; then
    PASS=$((PASS+1)); printf '  PASS  M2.migration-removes-duplicate-settings-json\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("M2.migration-removes-duplicate-settings-json"); printf '  FAIL  M2.migration-removes-duplicate-settings-json  (settings.json survived install.sh)\n'
fi
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "M2.post-migration-probe-passes" 0 "$rc"
# M2 idempotence — a SECOND install.sh run is a no-op (settings.json stays gone).
"$ROOT/scripts/hooks/install.sh" >/dev/null
if [ ! -f "$ROOT/.claude/settings.json" ]; then
    PASS=$((PASS+1)); printf '  PASS  M2b.migration-second-run-noop\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("M2b.migration-second-run-noop"); printf '  FAIL  M2b.migration-second-run-noop  (settings.json reappeared)\n'
fi
rm -rf "$ROOT"

# Case M3 — a settings.json carrying MACHINE-SPECIFIC non-hook config plus a
# duplicated hooks block: install.sh strips the hooks but KEEPS the file (and
# its unique key), and the probe then passes.
ROOT="$(make_sandbox)"
build_dup_settings_json "$ROOT"
python3 - "$ROOT/.claude/settings.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
d["env2"] = {"MACHINE_SPECIFIC_KNOB": "1"}   # a key NOT present in settings.local.json
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
"$ROOT/scripts/hooks/install.sh" >/dev/null
KEPT=0; HAS_HOOKS=1; HAS_KNOB=0
if [ -f "$ROOT/.claude/settings.json" ]; then
    KEPT=1
    HAS_HOOKS="$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(1 if d.get('hooks') else 0)" "$ROOT/.claude/settings.json")"
    HAS_KNOB="$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(1 if d.get('env2',{}).get('MACHINE_SPECIFIC_KNOB')=='1' else 0)" "$ROOT/.claude/settings.json")"
fi
if [ "$KEPT" = "1" ] && [ "$HAS_HOOKS" = "0" ] && [ "$HAS_KNOB" = "1" ]; then
    PASS=$((PASS+1)); printf '  PASS  M3.machine-specific-settings-json-hooks-stripped-config-kept\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("M3.machine-specific-settings-json-hooks-stripped-config-kept"); printf '  FAIL  M3.machine-specific-settings-json  (kept=%s hooks=%s knob=%s)\n' "$KEPT" "$HAS_HOOKS" "$HAS_KNOB"
fi
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "M3.machine-specific-probe-passes" 0 "$rc"
rm -rf "$ROOT"

# Case M4 — INTRA-FILE duplication: the same hook listed twice inside
# settings.local.json alone → probe exits 2 (Layer M2 counts the total).
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
for entry in d["hooks"]["PreToolUse"]:
    if "Write" in entry.get("matcher", ""):
        # Append a byte-identical second copy of the write-guard hook.
        entry["hooks"].append(dict(entry["hooks"][0]))
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "M4.intra-file-duplication-fails" 2 "$rc"
rm -rf "$ROOT"

# ---------------------------------------------------------------------------
# Shim attacks — Layer B path-confinement + hashing, targeting the CANONICAL
# settings.local.json.
# ---------------------------------------------------------------------------

fi  # _section — M

if _section shim; then
# Case 8 — Shim at non-canonical path with matching filename → FAIL.
ROOT="$(make_sandbox)"
SHIMDIR="$(mktemp -d "${TMPDIR:-/tmp}/shim.XXXXXX")"
SHIM="$SHIMDIR/guard-main-checkout-writes.sh"
cat >"$SHIM" <<'SHIM_SH'
#!/usr/bin/env bash
INPUT="$(cat)"
if printf '%s' "$INPUT" | grep >/dev/null '__integrity_canary__'; then
    exit 2
fi
exit 0
SHIM_SH
chmod +x "$SHIM"
set_guard_command "$ROOT/.claude/settings.local.json" "$SHIM"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "8.shim-at-wrong-path-fails" 2 "$rc"
rm -rf "$ROOT" "$SHIMDIR"

# Case 9 — Canonical path but modified content → hash mismatch → FAIL.
ROOT="$(make_sandbox)"
cat >"$ROOT/scripts/hooks/guard-main-checkout-writes.sh" <<'NOOP_SH'
#!/usr/bin/env bash
exit 0
NOOP_SH
chmod +x "$ROOT/scripts/hooks/guard-main-checkout-writes.sh"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "9.canonical-path-wrong-hash-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 10 — Hook command points to symlink whose target is outside repo → FAIL.
ROOT="$(make_sandbox)"
OUTSIDE="$(mktemp -d "${TMPDIR:-/tmp}/outside.XXXXXX")"
cat >"$OUTSIDE/g.sh" <<'OUT_SH'
#!/usr/bin/env bash
exit 0
OUT_SH
chmod +x "$OUTSIDE/g.sh"
rm -f "$ROOT/scripts/hooks/guard-main-checkout-writes.sh"
ln -s "$OUTSIDE/g.sh" "$ROOT/scripts/hooks/guard-main-checkout-writes.sh"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "10.symlink-to-outside-fails" 2 "$rc"
rm -rf "$ROOT" "$OUTSIDE"

# Case 11 — trailing args on the canonical path are benign → PASS;
# a shim path with the same trailing args → FAIL.
ROOT="$(make_sandbox)"
GUARD_ABS="$ROOT/scripts/hooks/guard-main-checkout-writes.sh"
set_guard_command "$ROOT/.claude/settings.local.json" "$GUARD_ABS --flag"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "11.canonical-with-trailing-args-passes" 0 "$rc"
SHIMDIR="$(mktemp -d "${TMPDIR:-/tmp}/shim.XXXXXX")"
SHIM="$SHIMDIR/guard-main-checkout-writes.sh"
cp "$GUARD_ABS" "$SHIM"
set_guard_command "$ROOT/.claude/settings.local.json" "$SHIM --flag"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "11b.shim-with-trailing-args-fails" 2 "$rc"
rm -rf "$ROOT" "$SHIMDIR"

# Case 12 — Fresh committed clone (no modification) → all layers PASS.
ROOT="$(make_sandbox)"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "12.canonical-fresh-clone-passes" 0 "$rc"
rm -rf "$ROOT"

# ---------------------------------------------------------------------------
# Manifest tampering — sidecar hash closure.
# ---------------------------------------------------------------------------

fi  # _section — shim

if _section manifest; then
# Case 13 — tamper the canonical guard hook → Layer B FAILS.
ROOT="$(make_sandbox)"
printf '\n' >> "$ROOT/scripts/hooks/guard-main-checkout-writes.sh"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "13.tamper-canonical-guard-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 14 — tamper the agent hook → Layer C FAILS.
ROOT="$(make_sandbox)"
printf '\n# tampered\n' >> "$ROOT/scripts/hooks/verify-agent-prompt.sh"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "14.tamper-canonical-agent-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 14a — tamper the FIRST chain member (guard-worktree-isolation.sh) → FAIL.
ROOT="$(make_sandbox)"
printf '\n# tampered\n' >> "$ROOT/scripts/hooks/guard-worktree-isolation.sh"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "14a.tamper-guard-worktree-isolation-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 14b — tamper the MIDDLE chain member (guard-definition-drift.sh) → FAIL.
ROOT="$(make_sandbox)"
printf '\n# tampered\n' >> "$ROOT/scripts/hooks/guard-definition-drift.sh"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "14b.tamper-guard-definition-drift-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 14c — chain entry count mismatch (only verify-agent-prompt.sh wired) → FAIL.
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") == "Agent":
        entry["hooks"] = [h for h in entry["hooks"] if "verify-agent-prompt" in h.get("command", "")]
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "14c.chain-entry-missing-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 14d — chain entries wired OUT OF ORDER (reversed) → Layer C FAILS (order is load-bearing).
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") == "Agent":
        entry["hooks"] = list(reversed(entry["hooks"]))
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "14d.chain-out-of-order-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 15 — inline shim-replace of the canonical guard body → hash mismatch FAIL.
ROOT="$(make_sandbox)"
cat >"$ROOT/scripts/hooks/guard-main-checkout-writes.sh" <<'NOOP_SH'
#!/usr/bin/env bash
exit 0
NOOP_SH
chmod +x "$ROOT/scripts/hooks/guard-main-checkout-writes.sh"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "15.inline-shim-replace-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 16 — manifest file missing → probe FAILS.
ROOT="$(make_sandbox)"
rm -f "$ROOT/scripts/hooks/guard-main-checkout-writes.sh.sha256"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "16.manifest-missing-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 17 — manifest file stale (wrong hash) → FAIL.
ROOT="$(make_sandbox)"
printf 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef\n' \
    > "$ROOT/scripts/hooks/guard-main-checkout-writes.sh.sha256"
set +e; run_probe_in "$ROOT" >/dev/null; rc=$?; set -e
emit_case "17.manifest-wrong-hash-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 18 — install.sh is idempotent for manifests too.
ROOT="$(make_sandbox)"
"$ROOT/scripts/hooks/install.sh" >/dev/null
HASH1_G="$(shasum -a 256 "$ROOT/scripts/hooks/guard-main-checkout-writes.sh.sha256" | awk '{print $1}')"
HASH1_A="$(shasum -a 256 "$ROOT/scripts/hooks/verify-agent-prompt.sh.sha256" | awk '{print $1}')"
HASH1_R="$(shasum -a 256 "$ROOT/scripts/hooks/guard-definition-drift.sh.sha256" | awk '{print $1}')"
HASH1_W="$(shasum -a 256 "$ROOT/scripts/hooks/guard-worktree-isolation.sh.sha256" | awk '{print $1}')"
"$ROOT/scripts/hooks/install.sh" >/dev/null
HASH2_G="$(shasum -a 256 "$ROOT/scripts/hooks/guard-main-checkout-writes.sh.sha256" | awk '{print $1}')"
HASH2_A="$(shasum -a 256 "$ROOT/scripts/hooks/verify-agent-prompt.sh.sha256" | awk '{print $1}')"
HASH2_R="$(shasum -a 256 "$ROOT/scripts/hooks/guard-definition-drift.sh.sha256" | awk '{print $1}')"
HASH2_W="$(shasum -a 256 "$ROOT/scripts/hooks/guard-worktree-isolation.sh.sha256" | awk '{print $1}')"
if [ "$HASH1_G" = "$HASH2_G" ] && [ "$HASH1_A" = "$HASH2_A" ] && [ "$HASH1_R" = "$HASH2_R" ] && [ "$HASH1_W" = "$HASH2_W" ]; then
    PASS=$((PASS+1)); printf '  PASS  18.install-idempotent-manifests\n'
else
    FAIL=$((FAIL+1))
    FAIL_NAMES+=("18.install-idempotent-manifests")
    printf '  FAIL  18.install-idempotent-manifests  (manifest hash drifted across runs)\n'
fi
rm -rf "$ROOT"

fi  # _section — manifest

# ---------------------------------------------------------------------------
# Worktree-resolution cases — the probe must resolve the TRUE main checkout
# whether its own copy runs from the main checkout OR from a linked
# `git worktree`.
# ---------------------------------------------------------------------------
make_git_main() {
    local root
    root="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/contract-integrity-git.XXXXXX")" && pwd -P)"
    # The same skeleton make_sandbox produces, from the same template. This
    # function used to carry its own copy of the inventory -- a second hand-
    # maintained list of the engine's files, in the suite whose entire subject
    # is lists that fall behind the engine.
    clone_sandbox_into "$root"
    # AND THROW THE TEMPLATE'S REPOSITORY AWAY. Since 2026-09-14 the sandbox
    # template is itself a git repository (init_sandbox_repo), and this fixture
    # is not a sandbox: it builds its OWN repository, on its own terms, three
    # of which the template's baseline would quietly overwrite. It ignores the
    # sidecars so cases 20/21 model a linked worktree that has none until
    # install.sh runs in it — and files already tracked in an inherited
    # baseline commit are not untracked by a later .gitignore. It inherits the
    # operator's real identity on purpose (see below). And it is the fixture
    # whose FIRST commit the cases care about. Starting from no repository at
    # all is what keeps this function's behavior exactly what it was.
    rm -rf "$root/.git"
    # Gitignore the generated settings.json + nested worktrees so a linked
    # worktree does NOT receive its own settings.json — mirroring the real repo,
    # where a probe run from a worktree reaches back to the MAIN checkout. The
    # .sha256 sidecars are gitignored per engine convention; the probe reads MAIN's
    # ON-DISK sidecars (minted by gen_sidecars) regardless of git tracking.
    cat >"$root/.gitignore" <<'GI'
/.claude/settings.json
/.claude/worktrees/
scripts/hooks/*.sha256
scripts/*.sha256
ass-kicker/*.sha256
GI
    git -C "$root" init -q -b main
    # NO local identity override. These throwaway fixtures inherit the
    # operator's real global identity, which is what a machine-wide pre-commit
    # identity guard requires. With a fake identity the seed commit is REFUSED,
    # the fixture has no branch, `git worktree add` fails, and cases 19-21
    # report exit 127 (missing path) rather than anything about the probe.
    git -C "$root" add -A
    # Force-add the canonical settings file so Layer N sees it tracked. A GLOBAL
    # gitignore convention ('**/.claude/settings.local.json') on the test
    # machine would otherwise make `git add -A` silently skip it (exactly the
    # trap Layer N exists to catch) — mirror the real remedy, `git add -f`.
    git -C "$root" add -f .claude/settings.local.json
    # Force-add the sidecar for scripts/agent-liveness.sh, for the same reason
    # one line up and a different gitignore rule. Cases 20/21 probe FROM the
    # linked worktree, and the layers that resolve against ENGINE_ROOT rather
    # than REPO_ROOT — R and AL — therefore read the WORKTREE's copy of the
    # engine, not MAIN's. Every other ENGINE_ROOT-read sidecar in this fixture
    # lives under scripts/lib/, which the .gitignore above does not cover, so
    # they arrive in the worktree by accident rather than by intent. This one
    # lives at scripts/ and would not, leaving the worktree carrying a managed
    # file with no sidecar — a real finding in a real engine, and a fixture
    # artifact here. Named rather than papered over: a REAL linked worktree of
    # this engine has NO sidecars at all until install.sh is run in it, which
    # is why an engine land is red until it is.
    git -C "$root" add -f scripts/agent-liveness.sh.sha256 2>/dev/null || true
    git -C "$root" commit -q -m init
    echo "$root"
}

if _section worktree; then
# Case 19 — probe passes from the MAIN checkout of a real git repo, and Layer N
# emits its explicit git-tracked PASS line (positive shape).
MAIN="$(make_git_main)"
set +e; PROBE_OUT="$(RICHOS_ENTITY_ROOT="$MAIN" "$MAIN/scripts/hooks/contract-integrity-probe.sh" 2>&1 1>/dev/null)"; rc=$?; set -e
emit_case "19.git-main-checkout-passes" 0 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'N. .claude/settings.local.json is git-tracked'; then
    PASS=$((PASS+1)); printf '  PASS  19b.layer-N-tracked-pass-line-present\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("19b.layer-N-tracked-pass-line-present"); printf '  FAIL  19b.layer-N-tracked-pass-line-present  (got: %s)\n' "$PROBE_OUT"
fi

# Case 20 — probe passes when invoked FROM A LINKED WORKTREE (resolve_main_
# checkout resolves REPO_ROOT back to MAIN; the worktree has no settings.json).
WT="$MAIN/.claude/worktrees/agent-test"
git -C "$MAIN" worktree add -q -b wt-test "$WT" >/dev/null 2>&1
set +e; RICHOS_ENTITY_ROOT="$WT" "$WT/scripts/hooks/contract-integrity-probe.sh" >/dev/null 2>&1; rc=$?; set -e
emit_case "20.git-worktree-invocation-passes" 0 "$rc"

# Case 21 — proof the worktree run TRULY targets MAIN's settings: planting a
# hook-duplicating settings.json in MAIN flips the worktree-invoked probe to
# FAIL (exit 2, Layer M), proving it reads MAIN's files, not a worktree-local
# copy or a stale fallback.
build_dup_settings_json "$MAIN"
set +e; RICHOS_ENTITY_ROOT="$WT" "$WT/scripts/hooks/contract-integrity-probe.sh" >/dev/null 2>&1; rc=$?; set -e
emit_case "21.worktree-probe-targets-main-settings" 2 "$rc"

git -C "$MAIN" worktree remove --force "$WT" >/dev/null 2>&1 || rm -rf "$WT"
rm -rf "$MAIN"

# ---------------------------------------------------------------------------
# Layer N — git-tracked canonical settings.local.json (the silent global-
# gitignore stranding trap). Positive shape lives in case 19b (tracked -> PASS
# line). These two cover the HARD-fail trap and the not-yet-committed WARN.
#
# Determinism: git's default excludesFile is the XDG path (~/.config/git/ignore),
# consulted EVEN when GIT_CONFIG_GLOBAL is neutralized — and on a dev box it
# commonly DOES carry the '**/.claude/settings.local.json' rule (the automation QA's live-fire
# F2). So we point core.excludesFile at /dev/null via a throwaway global config
# and null out the system config, making the "is it ignored?" answer depend ONLY
# on each case's own repo-local .gitignore, not the test machine.
# ---------------------------------------------------------------------------
fi  # _section — worktree

if _section N; then
NOIGNORE_CFG="$(mktemp "${TMPDIR:-/tmp}/contract-integrity-noignore.XXXXXX")"
# AND AN IDENTITY, because this throwaway config REPLACES the global one for
# the whole probe invocation below, not just for the ignore lookup.
#
# Case 33 was RED ON LINUX AND GREEN ON macOS for two days for exactly this,
# and the failure pointed nowhere near the cause. With GIT_CONFIG_GLOBAL
# pointed here and GIT_CONFIG_SYSTEM at /dev/null there is no user.name and no
# user.email anywhere, so `git commit` inside the probe's own Layer Q fixture
# exits 128, Q reports "FUNCTIONAL CANARY DID NOT RUN", the probe exits 2 — and
# case 33, which is about Layer N WARNING rather than hard-failing, saw 2
# instead of 0 for a reason that has nothing to do with Layer N. Case 32 was
# worse: it EXPECTS 2, so it kept passing while testing two things at once.
#
# It survived on macOS by accident of the host. git falls back to
# user@<canonicalized-hostname> and refuses only when that has no domain part;
# a Mac's hostname canonicalizes to something with a dot and a container's
# ('dec5b7ba7942') does not. Measured on both, 2026-09-10.
#
# So the config now replaces ONLY what it means to replace. The identity is the
# same shape the workflow declares on the runner, and it is confined to this
# section's two probe invocations.
printf '[core]\n\texcludesFile = /dev/null\n[user]\n\tname = contract-integrity\n\temail = contract-integrity@example.invalid\n' > "$NOIGNORE_CFG"

# Case 32 — THE TRAP: settings.local.json present on disk but untracked AND
# matched by a (repo-local) gitignore rule -> Layer N HARD-fails and names the
# git-add-f fix.
MAIN="$(make_git_main)"
git -C "$MAIN" rm --cached -q .claude/settings.local.json
printf '/.claude/settings.local.json\n' >> "$MAIN/.gitignore"
git -C "$MAIN" add .gitignore
git -C "$MAIN" commit -q -m "untrack + ignore settings.local.json (simulate the F2 trap)"
set +e
PROBE_OUT="$(GIT_CONFIG_GLOBAL="$NOIGNORE_CFG" GIT_CONFIG_SYSTEM=/dev/null RICHOS_ENTITY_ROOT="$MAIN" "$MAIN/scripts/hooks/contract-integrity-probe.sh" 2>&1 1>/dev/null)"
rc=$?
set -e
emit_case "32.git-untracked-ignored-settings-local-fails" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'git add -f .claude/settings.local.json'; then
    PASS=$((PASS+1)); printf '  PASS  32b.layer-N-names-the-git-add-f-fix\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("32b.layer-N-names-the-git-add-f-fix"); printf '  FAIL  32b.layer-N-names-the-git-add-f-fix  (got: %s)\n' "$PROBE_OUT"
fi
rm -rf "$MAIN"

# Case 33 — NOT-YET-COMMITTED (no ignore rule matches): untracked but not
# ignored -> Layer N WARNS, never HARD-fails (avoids the pre-first-commit false
# positive). Probe still exits 0.
MAIN="$(make_git_main)"
git -C "$MAIN" rm --cached -q .claude/settings.local.json
git -C "$MAIN" commit -q -m "untrack settings.local.json, no ignore rule"
set +e
PROBE_OUT="$(GIT_CONFIG_GLOBAL="$NOIGNORE_CFG" GIT_CONFIG_SYSTEM=/dev/null RICHOS_ENTITY_ROOT="$MAIN" "$MAIN/scripts/hooks/contract-integrity-probe.sh" 2>&1 1>/dev/null)"
rc=$?
set -e
emit_case "33.git-untracked-not-ignored-warns-rc-0" 0 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'N. .claude/settings.local.json is present but not yet git-tracked'; then
    PASS=$((PASS+1)); printf '  PASS  33b.layer-N-warns-when-not-yet-committed\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("33b.layer-N-warns-when-not-yet-committed"); printf '  FAIL  33b.layer-N-warns-when-not-yet-committed  (got: %s)\n' "$PROBE_OUT"
fi
rm -rf "$MAIN"
rm -f "$NOIGNORE_CFG"
fi  # _section — N


# ---------------------------------------------------------------------------
# python3-missing-from-PATH cases — mirrors the automation QA's fail-open repro. Both
# install.sh and contract-integrity-probe.sh must refuse (non-zero exit, loud
# stderr) rather than silently degrading when python3 is unresolvable.
# ---------------------------------------------------------------------------

# make_fakebin_no_python3 — a PATH dir populated with symlinks to every
# external tool these scripts need EXCEPT python3.
make_fakebin_no_python3() {
    local dir
    dir="$(mktemp -d "${TMPDIR:-/tmp}/fakebin-no-python3.XXXXXX")"
    local tools="cat grep sed cut tr date mkdir git mktemp basename dirname rm ln awk sort uniq wc head tail shasum sha256sum env mv"
    local t p
    for t in $tools; do
        p="$(command -v "$t" 2>/dev/null || true)"
        [ -n "$p" ] && ln -sf "$p" "$dir/$t"
    done
    echo "$dir"
}
BASH_BIN="$(command -v bash)"

if _section python3; then
# Case 22 — install.sh with no python3 on PATH -> refuses (non-zero), names
# python3 in its diagnostic.
ROOT="$(make_sandbox)"
FAKEBIN="$(make_fakebin_no_python3)"
set +e
NOPY_OUT="$(PATH="$FAKEBIN" "$BASH_BIN" "$ROOT/scripts/hooks/install.sh" 2>&1 1>/dev/null)"
rc=$?
set -e
emit_case "22.install-no-python3-refuses" 2 "$rc"
if printf '%s' "$NOPY_OUT" | grep -F >/dev/null 'python3'; then
    PASS=$((PASS+1)); printf '  PASS  22b.install-no-python3-stderr-names-python3\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("22b.install-no-python3-stderr-names-python3"); printf '  FAIL  22b.install-no-python3-stderr-names-python3  (got: %s)\n' "$NOPY_OUT"
fi
rm -rf "$FAKEBIN" "$ROOT"

# Case 23 — probe with no python3 on PATH -> refuses (non-zero), names python3
# in its diagnostic. Confirms the probe itself does not silently degrade its
# JSON-extraction / path-confinement checks when its interpreter is missing.
ROOT="$(make_sandbox)"
FAKEBIN="$(make_fakebin_no_python3)"
set +e
NOPY_OUT="$(PATH="$FAKEBIN" RICHOS_ENTITY_ROOT="$ROOT" "$BASH_BIN" "$ROOT/scripts/hooks/contract-integrity-probe.sh" 2>&1 1>/dev/null)"
rc=$?
set -e
emit_case "23.probe-no-python3-refuses" 2 "$rc"
if printf '%s' "$NOPY_OUT" | grep -F >/dev/null 'python3'; then
    PASS=$((PASS+1)); printf '  PASS  23b.probe-no-python3-stderr-names-python3\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("23b.probe-no-python3-stderr-names-python3"); printf '  FAIL  23b.probe-no-python3-stderr-names-python3  (got: %s)\n' "$NOPY_OUT"
fi
rm -rf "$FAKEBIN" "$ROOT"

# ---------------------------------------------------------------------------
# Critical-config cases (env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS,
# worktree.baseRef) — the "ghost team" incident this pair guards against.
# Cover both install.sh's own preserve-or-fail refusal (key missing BEFORE any
# install has run) and the probe's Layer I/J (key removed AFTER a successful
# install). Case 28 is the required positive-shape probe.
# ---------------------------------------------------------------------------

fi  # _section — python3

if _section config; then
# Case 24 — install.sh refuses when settings.local.json is missing the env
# flag from the start (never migrates/refreshes on a broken source).
ROOT="$(make_sandbox)"
write_sandbox_settings_local "$ROOT" "" "head"
set +e; INSTALL_OUT="$("$ROOT/scripts/hooks/install.sh" 2>&1 1>/dev/null)"; rc=$?; set -e
emit_case "24.install-missing-env-flag-refuses" 2 "$rc"
if printf '%s' "$INSTALL_OUT" | grep -F >/dev/null 'CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS'; then
    PASS=$((PASS+1)); printf '  PASS  24b.install-missing-env-flag-names-the-key\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("24b.install-missing-env-flag-names-the-key"); printf '  FAIL  24b.install-missing-env-flag-names-the-key  (got: %s)\n' "$INSTALL_OUT"
fi
if [ -f "$ROOT/.claude/settings.json" ]; then
    FAIL=$((FAIL+1)); FAIL_NAMES+=("24c.install-missing-env-flag-must-not-write-settings-json"); printf '  FAIL  24c.install-missing-env-flag-must-not-write-settings-json  (settings.json was written despite refusal)\n'
else
    PASS=$((PASS+1)); printf '  PASS  24c.install-missing-env-flag-must-not-write-settings-json\n'
fi
rm -rf "$ROOT"

# Case 25 — install.sh refuses when settings.local.json is missing
# worktree.baseRef from the start.
ROOT="$(make_sandbox)"
write_sandbox_settings_local "$ROOT" "1" ""
set +e; INSTALL_OUT="$("$ROOT/scripts/hooks/install.sh" 2>&1 1>/dev/null)"; rc=$?; set -e
emit_case "25.install-missing-baseref-refuses" 2 "$rc"
if printf '%s' "$INSTALL_OUT" | grep -F >/dev/null 'worktree.baseRef'; then
    PASS=$((PASS+1)); printf '  PASS  25b.install-missing-baseref-names-the-key\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("25b.install-missing-baseref-names-the-key"); printf '  FAIL  25b.install-missing-baseref-names-the-key  (got: %s)\n' "$INSTALL_OUT"
fi
rm -rf "$ROOT"

# Case 26 — probe Layer I catches the env flag being removed AFTER a
# successful install (the true incident shape: hand-edit settings.local.json,
# no re-install yet).
ROOT="$(make_sandbox)"
set +e; "$ROOT/scripts/hooks/install.sh" >/dev/null 2>&1; set -e
python3 -c "
import json
p = '$ROOT/.claude/settings.local.json'
d = json.load(open(p))
del d['env']
json.dump(d, open(p, 'w'), indent=2)
"
set +e; PROBE_OUT="$(RICHOS_ENTITY_ROOT="$ROOT" "$ROOT/scripts/hooks/contract-integrity-probe.sh" 2>&1 1>/dev/null)"; rc=$?; set -e
emit_case "26.probe-catches-env-flag-removed-post-install" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS'; then
    PASS=$((PASS+1)); printf '  PASS  26b.probe-names-the-missing-env-flag\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("26b.probe-names-the-missing-env-flag"); printf '  FAIL  26b.probe-names-the-missing-env-flag  (got: %s)\n' "$PROBE_OUT"
fi
rm -rf "$ROOT"

# Case 27 — probe Layer J catches worktree.baseRef being removed AFTER a
# successful install (same incident shape, the other critical key).
ROOT="$(make_sandbox)"
set +e; "$ROOT/scripts/hooks/install.sh" >/dev/null 2>&1; set -e
python3 -c "
import json
p = '$ROOT/.claude/settings.local.json'
d = json.load(open(p))
del d['worktree']
json.dump(d, open(p, 'w'), indent=2)
"
set +e; PROBE_OUT="$(RICHOS_ENTITY_ROOT="$ROOT" "$ROOT/scripts/hooks/contract-integrity-probe.sh" 2>&1 1>/dev/null)"; rc=$?; set -e
emit_case "27.probe-catches-baseref-removed-post-install" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'worktree.baseRef'; then
    PASS=$((PASS+1)); printf '  PASS  27b.probe-names-the-missing-baseref\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("27b.probe-names-the-missing-baseref"); printf '  FAIL  27b.probe-names-the-missing-baseref  (got: %s)\n' "$PROBE_OUT"
fi
rm -rf "$ROOT"

# Case 28 — POSITIVE-SHAPE: both keys correct -> the probe emits an explicit
# PASS line for EACH of Layer I, J, K (not just an overall rc=0).
ROOT="$(make_sandbox)"
set +e; "$ROOT/scripts/hooks/install.sh" >/dev/null 2>&1; set -e
set +e; PROBE_OUT="$(RICHOS_ENTITY_ROOT="$ROOT" "$ROOT/scripts/hooks/contract-integrity-probe.sh" 2>&1 1>/dev/null)"; rc=$?; set -e
emit_case "28.both-keys-correct-probe-passes" 0 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'I. env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS="1" present'; then
    PASS=$((PASS+1)); printf '  PASS  28b.layer-I-explicit-pass-line-present\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("28b.layer-I-explicit-pass-line-present"); printf '  FAIL  28b.layer-I-explicit-pass-line-present  (got: %s)\n' "$PROBE_OUT"
fi
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'J. worktree.baseRef="head" present'; then
    PASS=$((PASS+1)); printf '  PASS  28c.layer-J-explicit-pass-line-present\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("28c.layer-J-explicit-pass-line-present"); printf '  FAIL  28c.layer-J-explicit-pass-line-present  (got: %s)\n' "$PROBE_OUT"
fi
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'K. secrets scanner wired + REJECTS a planted secret and PASSES clean content'; then
    PASS=$((PASS+1)); printf '  PASS  28d.layer-K-explicit-pass-line-present\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("28d.layer-K-explicit-pass-line-present"); printf '  FAIL  28d.layer-K-explicit-pass-line-present  (got: %s)\n' "$PROBE_OUT"
fi
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'M. registration uniqueness'; then
    PASS=$((PASS+1)); printf '  PASS  28e.layer-M-explicit-pass-line-present\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("28e.layer-M-explicit-pass-line-present"); printf '  FAIL  28e.layer-M-explicit-pass-line-present  (got: %s)\n' "$PROBE_OUT"
fi
rm -rf "$ROOT"

# ---------------------------------------------------------------------------
# Layer K (secrets scanner) cases — missing wiring, tampered content, and the
# genuinely-broken-then-reinstalled case that hash-matching ALONE cannot catch
# (only the functional canary can).
# ---------------------------------------------------------------------------

fi  # _section — config

if _section K; then
# Case 29 — scan-secrets.sh NOT wired (dropped from the CANONICAL file, guard-
# main-checkout-writes.sh left alone) -> Layer K fails, names the scanner.
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
for entry in d["hooks"]["PreToolUse"]:
    if "Write" in entry.get("matcher", ""):
        entry["hooks"] = [h for h in entry["hooks"] if "scan-secrets" not in h.get("command", "")]
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; PROBE_OUT="$(RICHOS_ENTITY_ROOT="$ROOT" "$ROOT/scripts/hooks/contract-integrity-probe.sh" 2>&1 1>/dev/null)"; rc=$?; set -e
emit_case "29.secrets-scanner-not-wired-fails" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'K. PreToolUse[Write|Edit|MultiEdit|NotebookEdit] secrets scanner (scan-secrets.sh) NOT wired'; then
    PASS=$((PASS+1)); printf '  PASS  29b.probe-names-the-missing-secrets-scanner\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("29b.probe-names-the-missing-secrets-scanner"); printf '  FAIL  29b.probe-names-the-missing-secrets-scanner  (got: %s)\n' "$PROBE_OUT"
fi
rm -rf "$ROOT"

# Case 30 — scan-secrets.sh tampered (sidecar now stale) -> Layer K
# content-hash-mismatch fails.
ROOT="$(make_sandbox)"
printf '\n# tampered\n' >> "$ROOT/scripts/hooks/scan-secrets.sh"
set +e; PROBE_OUT="$(RICHOS_ENTITY_ROOT="$ROOT" "$ROOT/scripts/hooks/contract-integrity-probe.sh" 2>&1 1>/dev/null)"; rc=$?; set -e
emit_case "30.secrets-scanner-tampered-fails" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'K. secrets-scanner content hash mismatch'; then
    PASS=$((PASS+1)); printf '  PASS  30b.probe-names-the-hash-mismatch\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("30b.probe-names-the-hash-mismatch"); printf '  FAIL  30b.probe-names-the-hash-mismatch  (got: %s)\n' "$PROBE_OUT"
fi
rm -rf "$ROOT"

# Case 31 — scan-secrets.sh genuinely broken (edited into a no-op) THEN
# install.sh re-run, which regenerates the sidecar to MATCH the broken content.
# Hash-matching alone would now wrongly pass — only the functional canary
# (Layer K feeding it a known-bad secret) catches this.
ROOT="$(make_sandbox)"
cat >"$ROOT/scripts/hooks/scan-secrets.sh" <<'NOOP_SH'
#!/usr/bin/env bash
# Adversarial no-op: accepts everything, even an obvious secret.
exit 0
NOOP_SH
chmod +x "$ROOT/scripts/hooks/scan-secrets.sh"
"$ROOT/scripts/hooks/install.sh" >/dev/null
set +e; PROBE_OUT="$(RICHOS_ENTITY_ROOT="$ROOT" "$ROOT/scripts/hooks/contract-integrity-probe.sh" 2>&1 1>/dev/null)"; rc=$?; set -e
emit_case "31.secrets-scanner-broken-then-reinstalled-fails" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'K. wired secrets scanner did NOT block a known-bad secret'; then
    PASS=$((PASS+1)); printf '  PASS  31b.functional-canary-catches-broken-scanner-despite-matching-hash\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("31b.functional-canary-catches-broken-scanner-despite-matching-hash"); printf '  FAIL  31b.functional-canary-catches-broken-scanner-despite-matching-hash  (got: %s)\n' "$PROBE_OUT"
fi
rm -rf "$ROOT"

# Case 31c/31d — THE DEAD SCANNER. The case Layer K could not see until it grew
# a second side, and the reason it exists.
#
# scan-secrets.sh refuses to start without scripts/lib/seat-jurisdiction.sh by
# EXITING 2. Layer K's canary asserted the scanner exits 2 on a planted secret.
# Same number, opposite meanings — so a scanner that never executed satisfied
# the layer completely, and did so in both engine sandboxes for as long as those
# sandbox file lists had existed. Case 31 above cannot catch it: that scanner is
# a no-op that exits 0, this one never runs at all.
#
# 31c requires the probe to FAIL, and 31d requires it to say the true thing —
# "it is not scanning, it is failing to start" — rather than the misleading
# "did NOT block a known-bad secret", which would send a reader to audit
# detectors in a scanner that had not run a line.
#
# Note what this deliberately does NOT touch: the hook is byte-identical and its
# sidecar matches. Every hash check in Layer K passes. Only the second canary
# separates a guard that is READING from one that is merely refusing.
ROOT="$(make_sandbox)"
rm -f "$ROOT/scripts/lib/seat-jurisdiction.sh"
set +e; PROBE_OUT="$(RICHOS_ENTITY_ROOT="$ROOT" "$ROOT/scripts/hooks/contract-integrity-probe.sh" 2>&1 1>/dev/null)"; rc=$?; set -e
emit_case "31c.dead-secrets-scanner-fails-layer-K" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'K. the secrets scanner refused BOTH a planted secret and content with no secret in it'; then
    PASS=$((PASS+1)); printf '  PASS  31d.layer-K-says-failing-to-start-not-failing-to-catch\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("31d.layer-K-says-failing-to-start-not-failing-to-catch"); printf '  FAIL  31d.layer-K-says-failing-to-start-not-failing-to-catch  (got: %s)\n' "$PROBE_OUT"
fi
rm -rf "$ROOT"

# ---------------------------------------------------------------------------
# Layer P — the definition-drift guard PAIR. Either half alone is useless: with
# no SessionStart snapshot the PreToolUse guard has nothing to compare against,
# fails OPEN by design, and never blocks — silently. So the probe must FAIL when
# EITHER half is unwired, tampered, gutted, or duplicated, and must visibly PASS
# when both are intact.
# ---------------------------------------------------------------------------

fi  # _section — K

if _section P; then
# Case 34 — POSITIVE probe: an intact sandbox emits the Layer P PASS line.
ROOT="$(make_sandbox)"
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "34.definition-drift-pair-intact-probe-rc-0" 0 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'P. definition-drift pair wired exactly once'; then
    PASS=$((PASS+1)); printf '  PASS  34b.layer-P-pass-line-present\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("34b.layer-P-pass-line-present"); printf '  FAIL  34b.layer-P-pass-line-present  (got: %s)\n' "$PROBE_OUT"
fi
rm -rf "$ROOT"

# Case 35 — ONLY the SessionStart snapshotter unwired (the Agent chain is
# untouched, so Layer C still passes) -> Layer P alone must fail. The isolation
# case: it proves Layer P catches the silent half of the pair.
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
d["hooks"]["SessionStart"] = [
    e for e in d["hooks"]["SessionStart"]
    if not any("snapshot-agent-definitions" in h.get("command", "") for h in e.get("hooks", []))
]
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "35.definition-snapshotter-unwired-fails-layerP" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'snapshot-agent-definitions.sh) NOT wired'; then
    PASS=$((PASS+1)); printf '  PASS  35b.layer-P-names-the-missing-snapshotter\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("35b.layer-P-names-the-missing-snapshotter"); printf '  FAIL  35b.layer-P-names-the-missing-snapshotter  (got: %s)\n' "$PROBE_OUT"
fi
rm -rf "$ROOT"

# Case 36 — the PreToolUse[Agent] drift guard unwired -> probe fails (Layers C+P).
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") == "Agent":
        entry["hooks"] = [h for h in entry["hooks"]
                          if "guard-definition-drift" not in h.get("command", "")]
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; run_probe_in "$ROOT" >/dev/null 2>&1; rc=$?; set -e
emit_case "36.definition-drift-guard-unwired-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 37 — drift guard tampered (hash != sidecar) -> fails.
ROOT="$(make_sandbox)"
printf '\n# tamper — no longer matches the minted sidecar\n' >> "$ROOT/scripts/hooks/guard-definition-drift.sh"
set +e; run_probe_in "$ROOT" >/dev/null 2>&1; rc=$?; set -e
emit_case "37.definition-drift-guard-tampered-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 37b — SessionStart snapshotter tampered (hash != sidecar) -> fails. The
# snapshotter is not in the Agent chain, so ONLY Layer P can catch this.
ROOT="$(make_sandbox)"
printf '\n# tamper — no longer matches the minted sidecar\n' >> "$ROOT/scripts/hooks/snapshot-agent-definitions.sh"
set +e; run_probe_in "$ROOT" >/dev/null 2>&1; rc=$?; set -e
emit_case "37b.definition-snapshotter-tampered-fails-layerP" 2 "$rc"
rm -rf "$ROOT"

# Case 38 — drift guard gutted into an always-allow no-op AND its sidecar
# refreshed to match, so ONLY the functional block-canary can catch it.
ROOT="$(make_sandbox)"
printf '#!/usr/bin/env bash\nexit 0\n' > "$ROOT/scripts/hooks/guard-definition-drift.sh"
chmod +x "$ROOT/scripts/hooks/guard-definition-drift.sh"
shasum -a 256 "$ROOT/scripts/hooks/guard-definition-drift.sh" | awk '{print $1}' > "$ROOT/scripts/hooks/guard-definition-drift.sh.sha256"
set +e; run_probe_in "$ROOT" >/dev/null 2>&1; rc=$?; set -e
emit_case "38.definition-drift-guard-gutted-fails-layerP-canary" 2 "$rc"
rm -rf "$ROOT"

# Case 38b — drift guard turned into an always-BLOCK no-op (sidecar refreshed):
# the ALLOW-canary must catch the over-block, which a negative-only test never
# would. The "negative tests need a positive probe" pairing.
ROOT="$(make_sandbox)"
printf '#!/usr/bin/env bash\nexit 2\n' > "$ROOT/scripts/hooks/guard-definition-drift.sh"
chmod +x "$ROOT/scripts/hooks/guard-definition-drift.sh"
shasum -a 256 "$ROOT/scripts/hooks/guard-definition-drift.sh" | awk '{print $1}' > "$ROOT/scripts/hooks/guard-definition-drift.sh.sha256"
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "38b.definition-drift-guard-overblocks-fails-layerP-canary" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'blocked an UNCHANGED definition'; then
    PASS=$((PASS+1)); printf '  PASS  38c.layer-P-names-the-over-block\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("38c.layer-P-names-the-over-block"); printf '  FAIL  38c.layer-P-names-the-over-block  (got: %s)\n' "$PROBE_OUT"
fi
rm -rf "$ROOT"

# Case 39 — snapshotter registered TWICE (double-fire) -> fails (Layers M+P).
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
ss = d["hooks"]["SessionStart"]
dup = [e for e in ss
       if any("snapshot-agent-definitions" in h.get("command", "") for h in e.get("hooks", []))]
d["hooks"]["SessionStart"] = ss + [json.loads(json.dumps(dup[0]))]
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; run_probe_in "$ROOT" >/dev/null 2>&1; rc=$?; set -e
emit_case "39.definition-snapshotter-duplicated-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 40 — the pair's OWN behavioral suite (block/allow/ack/created/missing/
# cross-session/dedup/e2e) passes against the live scripts.
# ITS OUTPUT IS KEPT ON FAILURE, and so is every other behavioral suite's
# below, for IP7's reason and at the same cost: `(expected exit=0 got=1)` is
# all a red run could say, so the only way to learn WHICH of a suite's cases
# went red was to re-run it by hand.
#
# The filter is NOT IP7's, because a behavioral suite's output is not a
# mutation harness's. These suites print one `  PASS  <case>` row per green
# case, and EVERYTHING ELSE they print is the report -- the failing rows, the
# detail lines indented beneath them, the section headers that locate them, and
# the verdict, whose spelling differs in all seven (`=== ... FAILED ... ===`,
# `workspaces-e2e: N passed, M failed`, a tick-or-cross line, a unittest
# traceback). A keyword list would have to predict every one of those and would
# silently drop the line it failed to predict. So the rule is the one that
# cannot: keep every line that is not a PASS row. Measured at this commit, that
# leaves 2-14 non-blank lines per suite on a GREEN log (IP6, at 113 cases, is
# the 14), so the floor is small and a red run adds its failures to it.
#
# Blank lines are dropped too, so the indent does not print trailing spaces.
#
# The trailing `|| true` is IP7's and is not decoration: errexit is live here,
# a grep that matches nothing exits 1, and a no-match must not take the run
# down before the summary that names the red case.
# (Case 40 ran guard-definition-drift.test.sh here; it is now run once, as its
# own unit, and this case checks it is on the runner's list. See suite_discovered.)
suite_discovered "40.definition-drift-guard-suite-is-discovered" "scripts/hooks/guard-definition-drift.test.sh"

fi  # _section — P

# q_mutate_registry <root> <old> <new> — edit the sandbox registry and re-mint
# its sidecar, so only the FUNCTIONAL canary can see the change.
q_mutate_registry() {
    python3 - "$1/mega-lander/workspaces.py" "$2" "$3" <<'PY'
import sys
p, old, new = sys.argv[1:4]
s = open(p).read()
assert s.count(old) == 1, "registry anchor drifted — this mutation no longer mutates anything: %r" % old
open(p, "w").write(s.replace(old, new))
PY
    shasum -a 256 "$1/mega-lander/workspaces.py" | awk '{print $1}' > "$1/mega-lander/workspaces.py.sha256"
}

if _section Q; then
# ---------------------------------------------------------------------------
# LAYER Q — THE WORKSPACE SPEC (docs/plans/worktree-spec-2026-09-11.md).
#
# Until 2026-09-11 this section proved a SessionStart reaper, its inventory and
# a nightly reconciler. All three are removed with every deleter the page does
# not have, so every case below is about what replaced them: the lifecycle hook
# on its six events, the Stop gate, the registry and its one command, and the
# probe's sandboxed functional canary (spawn -> commit -> finish -> locked out
# -> turn held -> merged -> landed -> gone).
# ---------------------------------------------------------------------------
# Case 41 — POSITIVE: an intact sandbox emits the Layer Q PASS line.
ROOT="$(make_sandbox)"
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "41.workspace-spec-intact-probe-rc-0" 0 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'Q. the workspace spec is wired on its six events'; then
    PASS=$((PASS+1)); printf '  PASS  41b.layer-Q-pass-line-present\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("41b.layer-Q-pass-line-present"); printf '  FAIL  41b.layer-Q-pass-line-present  (got: %s)\n' "$(printf '%s' "$PROBE_OUT" | grep -F 'Q.' | head -5)"
fi
rm -rf "$ROOT"

# 41e-41g — LAYER Q'S FIXTURE IS HERMETIC AGAINST THE OPERATOR'S GIT CONFIG.
# A global config whose pre-commit hook refuses every commit and which declares
# no identity is both platforms' hostile shape at once (the 2026-09-10 incident:
# an identity guard in the operator's core.hooksPath refused the fixture's seed
# commit). The canary commits with GIT_CONFIG_GLOBAL=/dev/null; 41f removes that
# from the seed commit and the probe must fail AND say the canary did not run.
HOSTILE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/contract-integrity-hostile.XXXXXX")"
mkdir -p "$HOSTILE_DIR/hooks"
printf '#!/bin/sh\necho "ambient pre-commit hook refuses this commit" >&2\nexit 1\n' > "$HOSTILE_DIR/hooks/pre-commit"
chmod +x "$HOSTILE_DIR/hooks/pre-commit"
printf '[core]\n\thooksPath = %s\n\texcludesFile = /dev/null\n' "$HOSTILE_DIR/hooks" > "$HOSTILE_DIR/gitconfig"
run_probe_hostile() { # <sandbox root>
    env GIT_CONFIG_GLOBAL="$HOSTILE_DIR/gitconfig" GIT_CONFIG_SYSTEM=/dev/null \
        RICHOS_ENTITY_ROOT="$1" "$1/scripts/hooks/contract-integrity-probe.sh" 2>&1
}
ROOT="$(make_sandbox)"
set +e; PROBE_OUT="$(run_probe_hostile "$ROOT")"; rc=$?; set -e
emit_case "41e.layer-Q-survives-a-hostile-ambient-git-environment" 0 "$rc"
rm -rf "$ROOT"
ROOT="$(make_sandbox)"
python3 - "$ROOT/scripts/hooks/contract-integrity-probe.sh" <<'PY'
import sys
p = sys.argv[1]
text = open(p).read()
needle = '        GIT_CONFIG_GLOBAL=/dev/null git -C "$Q_ENT" -c user.name=probe'
assert text.count(needle) == 1, "the hermetic seed commit is no longer where this mutant expects it"
open(p, 'w').write(text.replace(needle, '        git -C "$Q_ENT" -c user.name=probe'))
PY
set +e; PROBE_OUT="$(run_probe_hostile "$ROOT")"; rc=$?; set -e
emit_case "41f.hermetic-seed-commit-removed-fails-under-a-hostile-environment" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'Q. FUNCTIONAL CANARY DID NOT RUN'; then
    PASS=$((PASS+1)); printf '  PASS  41g.layer-Q-says-its-canary-did-not-run\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("41g.layer-Q-says-its-canary-did-not-run")
    printf '  FAIL  41g.layer-Q-says-its-canary-did-not-run\n'
fi
rm -rf "$ROOT"
rm -rf "$HOSTILE_DIR"

# 41c/41d — a canary that could not run cannot leave a green probe. A former
# warning-only path concealed a stale fixture for days.
ROOT="$(make_sandbox)"
python3 - "$ROOT/scripts/hooks/contract-integrity-probe.sh" <<'PY'
import sys
p = sys.argv[1]
text = open(p).read()
assert text.count('        Q_SB=1\n') == 1
open(p, 'w').write(text.replace('        Q_SB=1\n', '        Q_SB=0\n'))
PY
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "41c.canary-that-could-not-run-fails-probe" 2 "$rc"
if printf '%s\n' "$PROBE_OUT" | grep -F 'Q. FUNCTIONAL CANARY DID NOT RUN' >/dev/null; then
    PASS=$((PASS+1)); printf '  PASS  41d.canary-that-could-not-run-is-explained\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("41d.canary-that-could-not-run-is-explained"); printf '  FAIL  41d.canary-that-could-not-run-is-explained\n'
fi
rm -rf "$ROOT"

# q_unwire <settings> <event> <script> — drop one script from one event.
q_unwire() {
    python3 - "$1" "$2" "$3" <<'PY'
import json, sys
p, event, script = sys.argv[1:4]
with open(p) as f: d = json.load(f)
for e in d["hooks"].get(event, []):
    e["hooks"] = [h for h in e.get("hooks", []) if script not in h.get("command", "")]
d["hooks"][event] = [e for e in d["hooks"].get(event, []) if e.get("hooks")]
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
}

# Case 42 — the lifecycle hook unwired from SessionStart -> Layer Q names it.
ROOT="$(make_sandbox)"
q_unwire "$ROOT/.claude/settings.local.json" SessionStart workspace-lifecycle.sh
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "42.lifecycle-unwired-from-SessionStart-fails-layerQ" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'workspace-lifecycle.sh registered 0x on SessionStart'; then
    PASS=$((PASS+1)); printf '  PASS  42b.layer-Q-names-the-missing-event\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("42b.layer-Q-names-the-missing-event"); printf '  FAIL  42b.layer-Q-names-the-missing-event\n'
fi
rm -rf "$ROOT"

# Case 42c — the PostToolUse matcher stops seeing SendMessage, so a pause is
# never recorded (point 11) -> Layer Q names the tool.
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
for e in d["hooks"]["PostToolUse"]:
    if e.get("matcher") == "Agent|TaskStop|SendMessage":
        e["matcher"] = "Agent|TaskStop"
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "42c.lifecycle-blind-to-SendMessage-fails-layerQ" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'does not see PostToolUse[SendMessage]'; then
    PASS=$((PASS+1)); printf '  PASS  42d.layer-Q-names-the-unseen-tool\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("42d.layer-Q-names-the-unseen-tool"); printf '  FAIL  42d.layer-Q-names-the-unseen-tool\n'
fi
rm -rf "$ROOT"

# Case 42e — the Stop gate unwired: point 5 is enforced nowhere -> Layer Q.
ROOT="$(make_sandbox)"
q_unwire "$ROOT/.claude/settings.local.json" Stop guard-workspace-gate.sh
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "42e.stop-gate-unwired-fails-layerQ" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'guard-workspace-gate.sh registered 0x on Stop'; then
    PASS=$((PASS+1)); printf '  PASS  42f.layer-Q-names-the-missing-gate\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("42f.layer-Q-names-the-missing-gate"); printf '  FAIL  42f.layer-Q-names-the-missing-gate\n'
fi
rm -rf "$ROOT"

# Case 43 — the lifecycle hook tampered (hash != sidecar) -> fails.
ROOT="$(make_sandbox)"
printf '\n# tamper — no longer matches the minted sidecar\n' >> "$ROOT/scripts/hooks/workspace-lifecycle.sh"
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "43.lifecycle-hook-tampered-fails-layerQ" 2 "$rc"
rm -rf "$ROOT"

# Case 44 — THE REGISTRY tampered. It is the only code that deletes a
# workspace, and it lives outside scripts/hooks/.
ROOT="$(make_sandbox)"
printf '\n# tamper — no longer matches the minted sidecar\n' >> "$ROOT/mega-lander/workspaces.py"
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "44.registry-tampered-fails-layerQ" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'Q. registry content hash mismatch'; then
    PASS=$((PASS+1)); printf '  PASS  44a.layer-Q-names-the-tampered-registry\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("44a.layer-Q-names-the-tampered-registry"); printf '  FAIL  44a.layer-Q-names-the-tampered-registry\n'
fi
rm -rf "$ROOT"

# Case 44b — the registry's sidecar MISSING entirely must be loud, not silent.
ROOT="$(make_sandbox)"
rm -f "$ROOT/mega-lander/workspaces.py.sha256"
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "44b.registry-sidecar-missing-fails-layerQ" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'Q. registry manifest missing'; then
    PASS=$((PASS+1)); printf '  PASS  44c.layer-Q-names-the-missing-manifest\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("44c.layer-Q-names-the-missing-manifest"); printf '  FAIL  44c.layer-Q-names-the-missing-manifest\n'
fi
rm -rf "$ROOT"

# Case 45 — the Stop gate tells the lead nothing (sidecar refreshed): finished work
# would sit neither landed nor discarded and unseen (point 5, as amended 2026-10-24: the
# turn end is allowed and the work is told). Only the canary sees it.
ROOT="$(make_sandbox)"
q_mutate_registry "$ROOT" '    return True, "\n".join(person), fallback_notice(sid, items)' '    return True, "\n".join(person), ""'
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "45.gate-tells-the-lead-nothing-fails-layerQ-canary" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'point 5: finished work neither landed nor discarded was not told'; then
    PASS=$((PASS+1)); printf '  PASS  45b.layer-Q-names-point-5\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("45b.layer-Q-names-point-5"); printf '  FAIL  45b.layer-Q-names-point-5\n'
fi
rm -rf "$ROOT"

# Case 45c — a land that deletes nothing (sidecar refreshed): the workspace and
# branch outlive the land (point 4).
ROOT="$(make_sandbox)"
q_mutate_registry "$ROOT" '    allw = [(r, w) for r in chain for w in live_workspaces(r) if w.get("path")]' '    return'
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "45c.land-deletes-nothing-fails-layerQ-canary" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null "point 4: the landed agent's workspace still exists"; then
    PASS=$((PASS+1)); printf '  PASS  45d.layer-Q-names-the-surviving-workspace\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("45d.layer-Q-names-the-surviving-workspace"); printf '  FAIL  45d.layer-Q-names-the-surviving-workspace\n'
fi
rm -rf "$ROOT"

# Case 46 — the lifecycle hook registered TWICE on SessionStart -> fails.
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
ss = d["hooks"]["SessionStart"]
dup = [e for e in ss if any("workspace-lifecycle" in h.get("command", "") for h in e.get("hooks", []))]
d["hooks"]["SessionStart"] = ss + [json.loads(json.dumps(dup[0]))]
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "46.lifecycle-duplicated-fails" 2 "$rc"
rm -rf "$ROOT"

# Case 47 — install.sh mints sidecars for all four files of the spec, including
# the two that do not live under scripts/hooks/.
ROOT="$(make_sandbox)"
rm -f "$ROOT/scripts/hooks/workspace-lifecycle.sh.sha256" "$ROOT/scripts/hooks/guard-workspace-gate.sh.sha256" \
      "$ROOT/mega-lander/workspaces.py.sha256" "$ROOT/mega-lander/workspaces.sh.sha256"
"$ROOT/scripts/hooks/install.sh" >/dev/null
MINTED=1
for f in "$ROOT/scripts/hooks/workspace-lifecycle.sh" "$ROOT/scripts/hooks/guard-workspace-gate.sh" \
         "$ROOT/mega-lander/workspaces.py" "$ROOT/mega-lander/workspaces.sh"; do
    [ -f "$f.sha256" ] || MINTED=0
    [ "$(shasum -a 256 "$f" | awk '{print $1}')" = "$(awk 'NR==1{print $1}' "$f.sha256" 2>/dev/null)" ] || MINTED=0
done
if [ "$MINTED" = "1" ]; then
    PASS=$((PASS+1)); printf '  PASS  47.install-mints-workspace-spec-sidecars\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("47.install-mints-workspace-spec-sidecars"); printf '  FAIL  47.install-mints-workspace-spec-sidecars\n'
fi
set +e; run_probe_in "$ROOT" >/dev/null 2>&1; rc=$?; set -e
emit_case "47b.post-install-probe-passes" 0 "$rc"
rm -rf "$ROOT"

# Case 48 — the registry's OWN suite (a test per numbered point) passes.
# Output KEPT on failure, with case 40's filter and for its reason.
suite_discovered "48.workspace-registry-suite-is-discovered" "mega-lander/tests/workspaces.test.sh"

fi  # _section — Q

if _section Qscope; then
# ---------------------------------------------------------------------------
# Layer Q, the RETIRED-DELETER and LOCK-OUT half — cases 49-53.
#
# Every deleter the page does not have must stay off the path. A stanza that
# brings one back is the defect, and Q1 names it by file and event. And the
# two properties only the canary can see — a finished agent refused every tool
# (point 9), a spawn registered before it happens (point 3) — each get a
# mutation of the registry that leaves every hash matching.
# ---------------------------------------------------------------------------
# q_revive <root> <event> <script> — wire a retired deleter back.
q_revive() {
    cat > "$1/scripts/hooks/$3" <<'REVIVED'
#!/usr/bin/env bash
exit 0
REVIVED
    chmod +x "$1/scripts/hooks/$3"
    python3 - "$1/.claude/settings.local.json" "$2" "$3" <<'PY'
import json, sys
p, event, script = sys.argv[1:4]
with open(p) as f: d = json.load(f)
d["hooks"].setdefault(event, []).append({"hooks": [{"type": "command",
    "command": "$CLAUDE_PROJECT_DIR/scripts/hooks/" + script, "timeout": 60}]})
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
}
for q_pair in "49|TeammateIdle|agent-finished-reap-worktrees.sh" \
              "50|SessionStart|session-start-reap-worktrees.sh" \
              "50c|SubagentStop|terminalize-agent-worktrees.sh" \
              "50e|SubagentStart|record-subagent-start.sh" \
              "50g|Stop|notice-land-disposition.sh"; do
    q_id="${q_pair%%|*}"; q_rest="${q_pair#*|}"; q_ev="${q_rest%%|*}"; q_sc="${q_rest#*|}"
    ROOT="$(make_sandbox)"
    q_revive "$ROOT" "$q_ev" "$q_sc"
    set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
    emit_case "$q_id.retired-deleter-$q_sc-revived-fails-layerQ" 2 "$rc"
    if printf '%s' "$PROBE_OUT" | grep -F >/dev/null "a retired deleter is wired: $q_sc on $q_ev"; then
        PASS=$((PASS+1)); printf '  PASS  %s\n' "${q_id}b.layer-Q-names-$q_sc"
    else
        FAIL=$((FAIL+1)); FAIL_NAMES+=("${q_id}b.layer-Q-names-$q_sc"); printf '  FAIL  %s\n' "${q_id}b.layer-Q-names-$q_sc"
    fi
    rm -rf "$ROOT"
done

# Case 51 — THE ONE THAT MATTERS. The lock-out stops refusing a finished agent
# (sidecar refreshed): a finished agent could write again (point 9). Every
# hash matches; only the canary's finished-agent arm can see it.
ROOT="$(make_sandbox)"
q_mutate_registry "$ROOT" '    if fin:
        return "FINISHED", "agent %s (%s) is finished: %s" % (aid, rec.get("name"), why)' '    if False:
        return "FINISHED", "agent %s (%s) is finished: %s" % (aid, rec.get("name"), why)'
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "51.finished-agent-not-locked-out-fails-layerQ-canary" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null "point 9: a finished agent's Read was allowed"; then
    PASS=$((PASS+1)); printf '  PASS  51b.layer-Q-names-point-9\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("51b.layer-Q-names-point-9"); printf '  FAIL  51b.layer-Q-names-point-9\n'
fi
rm -rf "$ROOT"

# Case 52 — registration at spawn broken (sidecar refreshed): no spawn can be
# registered, so none may happen (point 3), and the canary's spawn is refused.
#
# THE ANCHOR CARRIES THE LINE ABOVE IT, AND THAT IS NOT DECORATION. The bare
# `if not sid or not tuid:` was unique to register_spawn until e41e00f9
# (2026-09-12) gave the native-start registration the same two lines; from then
# on q_mutate_registry's uniqueness assertion fired and this case died with
# "registry anchor drifted" — a mutation that stopped mutating, reported as a
# suite failure rather than silently passing, which is the assertion working.
# `isolation = ...` belonged to register_spawn alone, so the pair named the
# function this case is about.
#
# IT DRIFTED AGAIN on 2026-09-13, and the second time says something the first
# one did not. register_spawn learned a DRY mode — every refusal evaluated by
# the same code with the write removed, so a spawn can be checked before it is
# made instead of costing one refusal per dispatch — and the condition became
# `if not sid or (not tuid and not dry):`. That spelling is now unique in the
# file on its own (register_readonly still carries the bare two-condition
# form), so the anchor is one line again and no longer needs a neighbor to
# disambiguate it. Both drifts are the uniqueness assertion doing its job: the
# mutation stopped mutating and SAID SO, rather than passing over code it no
# longer touched.
ROOT="$(make_sandbox)"
q_mutate_registry "$ROOT" '    if not sid or (not tuid and not dry):' '    if True:'
set +e; PROBE_OUT="$(run_probe_in "$ROOT")"; rc=$?; set -e
emit_case "52.registration-at-spawn-broken-fails-layerQ-canary" 2 "$rc"
if printf '%s' "$PROBE_OUT" | grep -F >/dev/null 'a well-formed spawn was not registered'; then
    PASS=$((PASS+1)); printf '  PASS  52b.layer-Q-names-the-unregistered-spawn\n'
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("52b.layer-Q-names-the-unregistered-spawn"); printf '  FAIL  52b.layer-Q-names-the-unregistered-spawn\n'
fi
rm -rf "$ROOT"

# Case 53 — the end-to-end demonstration through the real hooks passes.
# Output KEPT on failure, with case 40's filter and for its reason.
suite_discovered "53.workspace-spec-end-to-end-is-discovered" "mega-lander/tests/workspaces-e2e.test.sh"

# ---------------------------------------------------------------------------
# Layer S — the worktree guard and the one command it names
# ---------------------------------------------------------------------------
# Four ways the pair can be broken, each of which leaves an agent's workspace
# removable by a raw command, or leaves the guard with no way through. The
# positive baseline is already covered by cases 1/2.

fi  # _section — Qscope

if _section S; then
# S1 — the guard is not wired at all.
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") == "Bash":
        entry["hooks"] = [h for h in entry["hooks"]
                          if "guard-worktree-removal.sh" not in h.get("command", "")]
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; S_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "S1.worktree-removal-guard-not-wired-fails" 2 "$rc"
if printf '%s' "$S_OUT" | grep >/dev/null 'S\. PreToolUse\[Bash\] worktree guard'; then
    PASS=$((PASS+1)); printf '  PASS  %s\n' "S1b.layer-S-is-the-layer-that-caught-it"
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("S1b.layer-S-is-the-layer-that-caught-it")
    printf '  FAIL  %s  (some other layer absorbed it)\n' "S1b.layer-S-is-the-layer-that-caught-it"
fi
rm -rf "$ROOT"

# S2 — wired TWICE. Hook sources merge additively, so it fires twice per Bash
# call and every ack-log line is duplicated.
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") == "Bash":
        dup = [h for h in entry["hooks"] if "guard-worktree-removal.sh" in h.get("command", "")]
        entry["hooks"].extend(dup)
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; run_probe_in "$ROOT" >/dev/null 2>&1; rc=$?; set -e
emit_case "S2.worktree-removal-guard-wired-twice-fails" 2 "$rc"
rm -rf "$ROOT"

# S3 — the guard is gutted into a no-op. It is still wired, still present, and
# its hash no longer matches; the canary would also catch it if the hash did not.
ROOT="$(make_sandbox)"
cat >"$ROOT/scripts/hooks/guard-worktree-removal.sh" <<'NOOP_SH'
#!/usr/bin/env bash
exit 0
NOOP_SH
chmod +x "$ROOT/scripts/hooks/guard-worktree-removal.sh"
set +e; run_probe_in "$ROOT" >/dev/null 2>&1; rc=$?; set -e
emit_case "S3.gutted-worktree-removal-guard-fails" 2 "$rc"
rm -rf "$ROOT"

# S4 — THE PAIR SPLIT. The guard is perfect; the one command it names is gone.
# This is the case the layer exists for and the one a hooks-directory-shaped
# sweep would miss entirely: mega-lander/workspaces.sh is not a hook and does not
# live in scripts/hooks/. A guard whose only way through does not exist stops
# being a gate and becomes a prompt to reach for a raw removal.
ROOT="$(make_sandbox)"
rm -f "$ROOT/mega-lander/workspaces.sh"
set +e; S_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "S4.sanctioned-helper-missing-fails" 2 "$rc"
if printf '%s' "$S_OUT" | grep >/dev/null 'the sanctioned command is MISSING'; then
    PASS=$((PASS+1)); printf '  PASS  %s\n' "S4b.layer-S-names-the-missing-helper"
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("S4b.layer-S-names-the-missing-helper")
    printf '  FAIL  %s\n' "S4b.layer-S-names-the-missing-helper"
fi
rm -rf "$ROOT"

# ---------------------------------------------------------------------------
# Layer IP — the interactive-prompt guard and its shape table
# ---------------------------------------------------------------------------
# Six ways the pair can be broken, each of which reopens the hole that put a
# macOS password window on the CEO's screen at 02:01 on 2026-09-01. The positive
# baseline is covered by cases 1/2, which now include Layer IP.
#
# IP5 is the case worth reading. This guard exits 2 to refuse and 2 when it
# cannot start, so a one-sided canary would be satisfied by a corpse — the exact
# way Layer K stayed green over a dead secrets scanner. The two-sided canary is
# what makes IP5 possible, and IP5 is what proves the two-sided canary is real.

fi  # _section — S

if _section IP; then
# A long, unrelated Read hook must not make the first-write-hook selector
# terminate the whole probe with SIGPIPE. Exercise both a valid inventory and
# a genuinely missing interactive guard against the same large input.
pad_hook_inventory() {
    python3 - "$1/.claude/settings.local.json" <<'PY'
import json, sys
from pathlib import Path
p = Path(sys.argv[1])
d = json.loads(p.read_text())
d["hooks"]["PreToolUse"].append({"matcher": "Read", "hooks": [
    {"type": "command", "command": "true # " + "padding" * 10000}
]})
p.write_text(json.dumps(d))
PY
}
ROOT="$(make_sandbox)"
pad_hook_inventory "$ROOT"
set +e; IP_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "IP0.large-hook-inventory-passes" 0 "$rc"
rm -rf "$ROOT"

# IP1 — the guard is not wired at all.
ROOT="$(make_sandbox)"
pad_hook_inventory "$ROOT"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") == "Bash":
        entry["hooks"] = [h for h in entry["hooks"]
                          if "guard-interactive-prompt.sh" not in h.get("command", "")]
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; IP_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "IP1.interactive-prompt-guard-not-wired-fails" 2 "$rc"
if printf '%s' "$IP_OUT" | grep >/dev/null 'IP\. PreToolUse\[Bash\] interactive-prompt guard'; then
    PASS=$((PASS+1)); printf '  PASS  %s\n' "IP1b.layer-IP-is-the-layer-that-caught-it"
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("IP1b.layer-IP-is-the-layer-that-caught-it")
    printf '  FAIL  %s  (some other layer absorbed it)\n' "IP1b.layer-IP-is-the-layer-that-caught-it"
fi
rm -rf "$ROOT"

# IP2 — wired TWICE. Hook sources merge additively, so the same missing -P is
# reported as two separate hazards in one command line.
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
for entry in d["hooks"]["PreToolUse"]:
    if entry.get("matcher") == "Bash":
        dup = [h for h in entry["hooks"] if "guard-interactive-prompt.sh" in h.get("command", "")]
        entry["hooks"].extend(dup)
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; run_probe_in "$ROOT" >/dev/null 2>&1; rc=$?; set -e
emit_case "IP2.interactive-prompt-guard-wired-twice-fails" 2 "$rc"
rm -rf "$ROOT"

# IP3 — the guard is gutted into a no-op. Still wired, still present; the hash
# catches it, and the canary would too if the hash did not.
ROOT="$(make_sandbox)"
cat >"$ROOT/scripts/hooks/guard-interactive-prompt.sh" <<'NOOP_SH'
#!/usr/bin/env bash
exit 0
NOOP_SH
chmod +x "$ROOT/scripts/hooks/guard-interactive-prompt.sh"
set +e; run_probe_in "$ROOT" >/dev/null 2>&1; rc=$?; set -e
emit_case "IP3.gutted-interactive-prompt-guard-fails" 2 "$rc"
rm -rf "$ROOT"

# IP4 — THE PAIR SPLIT. The guard is perfect; the shape table it takes every
# decision from is gone. Like Layer S's helper and Layer T's vocabulary, the
# table is not a hook and does not live in scripts/hooks/, so a directory-shaped
# sweep misses it entirely.
ROOT="$(make_sandbox)"
rm -f "$ROOT/scripts/lib/interactive-prompt.py" "$ROOT/scripts/lib/interactive-prompt.py.sha256"
set +e; IP_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "IP4.shape-table-missing-fails" 2 "$rc"
if printf '%s' "$IP_OUT" | grep >/dev/null 'the shape table is MISSING'; then
    PASS=$((PASS+1)); printf '  PASS  %s\n' "IP4b.layer-IP-names-the-missing-shape-table"
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("IP4b.layer-IP-names-the-missing-shape-table")
    printf '  FAIL  %s\n' "IP4b.layer-IP-names-the-missing-shape-table"
fi
rm -rf "$ROOT"

# IP5 — A DEAD GUARD, WHOSE CORPSE EXITS 2. This is the failure a one-sided
# canary cannot see: refusing everything looks identical to refusing the right
# thing. Layer IP must say it is failing to START, not failing to catch.
ROOT="$(make_sandbox)"
cat >"$ROOT/scripts/hooks/guard-interactive-prompt.sh" <<'DEAD_SH'
#!/usr/bin/env bash
exit 2
DEAD_SH
chmod +x "$ROOT/scripts/hooks/guard-interactive-prompt.sh"
shasum -a 256 "$ROOT/scripts/hooks/guard-interactive-prompt.sh" | awk '{print $1}' \
    > "$ROOT/scripts/hooks/guard-interactive-prompt.sh.sha256"
set +e; IP_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "IP5.dead-guard-that-refuses-everything-fails-despite-matching-hash" 2 "$rc"
if printf '%s' "$IP_OUT" | grep >/dev/null 'refused BOTH the prompting command and the corrected one'; then
    PASS=$((PASS+1)); printf '  PASS  %s\n' "IP5b.layer-IP-says-failing-to-start-not-failing-to-catch"
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("IP5b.layer-IP-says-failing-to-start-not-failing-to-catch")
    printf '  FAIL  %s\n' "IP5b.layer-IP-says-failing-to-start-not-failing-to-catch"
fi
rm -rf "$ROOT"

# IP6 — the guard's OWN behavioral suite (113 cases, two-sided throughout) and
# the mutation harness that proves that suite can fail, both against the live
# scripts. Case 48 does the same for the reaper wrapper.
# Output KEPT on failure, with case 40's filter and for its reason.
suite_discovered "IP6.interactive-prompt-guard-suite-is-discovered" "scripts/hooks/guard-interactive-prompt.test.sh"
# ITS OUTPUT IS KEPT ON FAILURE, and so is every other mutation harness's
# below, for the reason written above CL2 and paid for a second time on
# 2026-09-13: a harness that can only say `(expected exit=0 got=1)` is a
# harness somebody re-runs by hand, and the WTI1 red cost several hours of
# guesswork because no one could ask the red run WHICH mutant had survived.
#
# The anchor is `^ *` and not `^  `, for the reason spelled out at WTI1: when a
# mutant's suite goes red at the WRONG case, the harness prints its verdict AND
# the inner suite's own failing cases indented further beneath it, and those
# deeper lines are the ones that name what actually broke. A two-space anchor
# drops exactly them.
#
# The trailing `|| true` is not decoration either: errexit is live here, a grep
# that matches nothing exits 1, and a no-match must not take the run down
# before the summary that names the red case.
IP7_LOG="$(mktemp "${TMPDIR:-/tmp}/interactive-prompt-mutations.XXXXXX")"
set +e; "$SCRIPT_DIR/interactive-prompt.mutation.sh" >"$IP7_LOG" 2>&1; rc=$?; set -e
emit_mutation_case "IP7.interactive-prompt-mutations-all-load-bearing" "$rc" "$IP7_LOG"
if [ "$rc" -ne 0 ]; then
    diag_or_tail "$(grep -E '^ *(FAIL|PASS|UNPROVEN)|^=== |survived or misfired|SURVIVED|NOT proven|NOT load-bearing' "$IP7_LOG" \
        | grep -vE '^ *PASS' || true)" "$(cat "$IP7_LOG" 2>/dev/null || true)"
fi
rm -f "$IP7_LOG"

# ---------------------------------------------------------------------------
# LAYER IL — THE IDLE-LAND GATE, THE FIRST FUNCTIONAL LAYER ON THE Stop EVENT
# ---------------------------------------------------------------------------
# Until 2026-09-01 the Stop event's two BLOCKING guards were known to this probe
# only as REGISTRATION. That is the gap these cases close, and the reason it
# matters is on the record rather than hypothetical: the idle-land gate shipped
# registered, hashed, executable and green on every layer here, while standing
# itself down on 41% of the turns it governs. Registration is not enforcement.
#
# IL5 is the case worth reading. This gate FAILS OPEN by design — a Stop guard
# that fails closed refuses to let the SESSION end — so a corpse exits 0, and a
# one-sided canary asserting only "declared stop -> 0" is satisfied by a hook
# that refuses nothing at all. That is precisely the state it shipped in. The
# two-sided canary is what makes IL5 possible, and IL5 is what proves the
# two-sided canary is real.

fi  # _section — IP

if _section IL; then
# IL1 — the gate is not wired on Stop at all.
ROOT="$(make_sandbox)"
python3 - "$ROOT/.claude/settings.local.json" <<'PY'
import json, sys
p = sys.argv[1]
with open(p) as f: d = json.load(f)
d["hooks"]["Stop"] = [e for e in d["hooks"]["Stop"]
                      if not any("guard-idle-land.sh" in h.get("command", "")
                                 for h in e.get("hooks", []))]
with open(p, "w") as f: json.dump(d, f, indent=2)
PY
set +e; IL_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "IL1.idle-land-gate-not-wired-fails" 2 "$rc"
if printf '%s' "$IL_OUT" | grep >/dev/null 'IL. Stop\[guard-idle-land.sh\] NOT wired'; then
    PASS=$((PASS+1)); printf '  PASS  %s\n' "IL1b.layer-IL-is-the-layer-that-caught-it"
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("IL1b.layer-IL-is-the-layer-that-caught-it")
    printf '  FAIL  %s  (some other layer absorbed it)\n' "IL1b.layer-IL-is-the-layer-that-caught-it"
fi
rm -rf "$ROOT"

# IL2 — THE PAIR SPLIT, and the case this whole layer was built to make
# possible. The wrapper is perfect: wired, executable, hash-matched. Its
# ANALYZER is gone — and the wrapper decides nothing without it. It starts
# cleanly, announces "NOT RUNNING: the analyzer is missing" into a channel
# nothing reads in a sandbox, and exits 0 on every turn. SC1 passes it, because
# SC1 asks whether a hook can START.
ROOT="$(make_sandbox)"
rm -f "$ROOT/scripts/hooks/guard-idle-land.py" "$ROOT/scripts/hooks/guard-idle-land.py.sha256"
set +e; IL_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "IL2.missing-analyzer-fails" 2 "$rc"
if printf '%s' "$IL_OUT" | grep >/dev/null 'the idle-land ANALYZER is missing'; then
    PASS=$((PASS+1)); printf '  PASS  %s\n' "IL2b.layer-IL-names-the-missing-analyzer"
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("IL2b.layer-IL-names-the-missing-analyzer")
    printf '  FAIL  %s\n' "IL2b.layer-IL-names-the-missing-analyzer"
fi
rm -rf "$ROOT"

# IL3 — the analyzer is REPLACED. Still present, still importable, and it
# decides the opposite of what it should. The hash catches it; the canary would
# too if the hash did not.
ROOT="$(make_sandbox)"
cat >"$ROOT/scripts/hooks/guard-idle-land.py" <<'NOOP_PY'
import sys
sys.exit(0)
NOOP_PY
set +e; run_probe_in "$ROOT" >/dev/null 2>&1; rc=$?; set -e
emit_case "IL3.replaced-analyzer-fails-on-the-hash" 2 "$rc"
rm -rf "$ROOT"

# IL4 — the wrapper is gutted into a no-op, hash regenerated so the manifest
# agrees. Only the functional canary can see this.
ROOT="$(make_sandbox)"
cat >"$ROOT/scripts/hooks/guard-idle-land.sh" <<'NOOP_SH'
#!/usr/bin/env bash
exit 0
NOOP_SH
chmod +x "$ROOT/scripts/hooks/guard-idle-land.sh"
shasum -a 256 "$ROOT/scripts/hooks/guard-idle-land.sh" | awk '{print $1}' \
    > "$ROOT/scripts/hooks/guard-idle-land.sh.sha256"
set +e; IL_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "IL4.gutted-gate-fails-despite-matching-hash" 2 "$rc"
if printf '%s' "$IL_OUT" | grep >/dev/null 'did NOT refuse a turn that merged a branch'; then
    PASS=$((PASS+1)); printf '  PASS  %s\n' "IL4b.layer-IL-says-it-is-registered-but-not-enforcing"
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("IL4b.layer-IL-says-it-is-registered-but-not-enforcing")
    printf '  FAIL  %s\n' "IL4b.layer-IL-says-it-is-registered-but-not-enforcing"
fi
rm -rf "$ROOT"

# IL5 — A GATE THAT REFUSES EVERYTHING, hash regenerated. The mirror image of
# IL4, and the half a one-sided canary cannot see: an always-2 hook satisfies
# "bad turn is refused" perfectly while making the declared escape unreachable —
# and an escape nobody can reach is a guard somebody unwires.
ROOT="$(make_sandbox)"
cat >"$ROOT/scripts/hooks/guard-idle-land.sh" <<'DEAD_SH'
#!/usr/bin/env bash
exit 2
DEAD_SH
chmod +x "$ROOT/scripts/hooks/guard-idle-land.sh"
shasum -a 256 "$ROOT/scripts/hooks/guard-idle-land.sh" | awk '{print $1}' \
    > "$ROOT/scripts/hooks/guard-idle-land.sh.sha256"
set +e; IL_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "IL5.gate-that-refuses-everything-fails-despite-matching-hash" 2 "$rc"
if printf '%s' "$IL_OUT" | grep >/dev/null 'refused BOTH the bad turn and a legitimately DECLARED stop'; then
    PASS=$((PASS+1)); printf '  PASS  %s\n' "IL5b.layer-IL-says-refusing-everything-not-deciding"
else
    FAIL=$((FAIL+1)); FAIL_NAMES+=("IL5b.layer-IL-says-refusing-everything-not-deciding")
    printf '  FAIL  %s\n' "IL5b.layer-IL-says-refusing-everything-not-deciding"
fi
rm -rf "$ROOT"

# IL6 — the gate's OWN behavioral suite and the mutation harness that proves
# that suite can fail, both against the live scripts. The mutation harness is
# not optional decoration here: this gate's previous suite was 38 green cases
# over a term that disarmed it, and a mutation run is the only thing that asks
# whether a green tick is load-bearing.
# Output KEPT on failure, with case 40's filter and for its reason.
suite_discovered "IL6.idle-land-gate-suite-is-discovered" "scripts/hooks/guard-idle-land.test.sh"
# Output KEPT on failure, with IP7's anchor and for IP7's reason.
IL7_LOG="$(mktemp "${TMPDIR:-/tmp}/idle-land-mutations.XXXXXX")"
set +e; "$SCRIPT_DIR/idle-land.mutation.sh" >"$IL7_LOG" 2>&1; rc=$?; set -e
emit_mutation_case "IL7.idle-land-mutations-all-load-bearing" "$rc" "$IL7_LOG"
if [ "$rc" -ne 0 ]; then
    diag_or_tail "$(grep -E '^ *(FAIL|PASS|UNPROVEN)|^=== |survived or misfired|SURVIVED|NOT proven|NOT load-bearing' "$IL7_LOG" \
        | grep -vE '^ *PASS' || true)" "$(cat "$IL7_LOG" 2>/dev/null || true)"
fi
rm -f "$IL7_LOG"

fi  # _section — IL

if _section CL; then
# CL1/CL2 — the CLAIM gate's own behavioral suite and mutation harness, for the
# reason IL6/IL7 exist, arriving late and after a cost. Both files were in this
# repository and were run by NOTHING: the only automatic check on
# guard-unresolved-claims.sh was that it starts (SC1) and that its hash matches
# (Layer BR). On 2026-09-01 that gate blocked one of the night's three false
# reports and passed the other two, and nothing in CI would have noticed if a
# refactor had silenced all three.
#
# The mutation half is not decoration here either. This gate's newest arm is a
# two-sided canary, and the property most easily satisfied by a corpse is
# "a false claim is refused" — a gate that refuses everything passes it. One of
# the mutations empties the integration ref list precisely so a TRUE landing
# claim gets refused, and asserts that the POSITIVE case goes red.
suite_discovered "CL1.claim-gate-suite-is-discovered" "scripts/hooks/guard-unresolved-claims.test.sh"
# Output KEPT on failure. IL7 discards it, and when CL2 first went red inside
# this suite while passing standalone, "exit=1" was the entire diagnosis. A
# mutation harness that can only say "something survived" is a harness you end
# up re-running by hand anyway.
#
# The filter is case 40's EXCLUSION form -- print every line that is not a PASS
# row -- and deliberately NOT a positive allowlist. An allowlist prints only the
# failures somebody predicted, and this harness's own fatal paths are not among
# them: `FATAL: python3 required` (claim-roles.mutation.sh line 38), the
# `FATAL: mut_pool_*` and `WARNING:` exits in scripts/lib/mutation-pool.sh, and
# the embedded python's `MUTATION TARGET ABSENT -- the source has drifted`
# match neither `^  FAIL` nor `survived or misfired`. Reproduced end-to-end
# against a python3-less harness before this line changed: the report said
#
#       FAIL  CL2.claim-gate-mutations-all-load-bearing  (expected exit=0 got=1)
#
# and the harness's own message appeared ZERO times in it. The run survived and
# named the red case -- and still did not say what broke, which is the whole
# point of keeping the output. The exclusion form also keeps two things the old
# anchor dropped on an ORDINARY failure: the 10-space `why` continuation under
# each FAIL row, and the closing `N mutant(s) killed, M survived or misfired`.
CL2_LOG="$(mktemp "${TMPDIR:-/tmp}/claim-mutations.XXXXXX")"
set +e; "$SCRIPT_DIR/claim-roles.mutation.sh" >"$CL2_LOG" 2>&1; rc=$?; set -e
emit_mutation_case "CL2.claim-gate-mutations-all-load-bearing" "$rc" "$CL2_LOG"
if [ "$rc" -ne 0 ]; then
    grep -vE '^ *PASS|^[[:space:]]*$' "$CL2_LOG" | sed 's/^/        /' || true
fi
rm -f "$CL2_LOG"

fi  # _section — CL

if _section RI; then
# RI1/RI2/IN2 — the resume-isolation guard's suite and mutation harness, and the
# in-flight notify mutations. Registered here for the reason CL1/CL2 are: a
# harness nothing runs is a harness that rots. IN2 earned its row the hard way —
# at pristine HEAD it killed 11 of 18, because five mutant sandboxes lacked
# scripts/lib/git-jurisdiction.sh and so ran a guard that REFUSED TO START, which
# reads exactly like a guard that caught the mutation.
# Output KEPT on failure, with case 40's filter and for its reason.
suite_discovered "RI1.resume-isolation-suite-is-discovered" "scripts/hooks/guard-resume-isolation.test.sh"
# Output KEPT on failure, with IP7's anchor and for IP7's reason.
RI2_LOG="$(mktemp "${TMPDIR:-/tmp}/resume-isolation-mutations.XXXXXX")"
set +e; "$SCRIPT_DIR/guard-resume-isolation.mutation.sh" >"$RI2_LOG" 2>&1; rc=$?; set -e
emit_mutation_case "RI2.resume-isolation-mutations-all-load-bearing" "$rc" "$RI2_LOG"
if [ "$rc" -ne 0 ]; then
    diag_or_tail "$(grep -E '^ *(FAIL|PASS|UNPROVEN)|^=== |survived or misfired|SURVIVED|NOT proven|NOT load-bearing' "$RI2_LOG" \
        | grep -vE '^ *PASS' || true)" "$(cat "$RI2_LOG" 2>/dev/null || true)"
fi
rm -f "$RI2_LOG"
# Output KEPT on failure, with IP7's anchor and for IP7's reason.
IN2_LOG="$(mktemp "${TMPDIR:-/tmp}/inflight-notify-mutations.XXXXXX")"
set +e; "$SCRIPT_DIR/inflight-notify.mutation.sh" >"$IN2_LOG" 2>&1; rc=$?; set -e
emit_mutation_case "IN2.inflight-notify-mutations-all-load-bearing" "$rc" "$IN2_LOG"
if [ "$rc" -ne 0 ]; then
    diag_or_tail "$(grep -E '^ *(FAIL|PASS|UNPROVEN)|^=== |survived or misfired|SURVIVED|NOT proven|NOT load-bearing' "$IN2_LOG" \
        | grep -vE '^ *PASS' || true)" "$(cat "$IN2_LOG" 2>/dev/null || true)"
fi
rm -f "$IN2_LOG"

fi  # _section — RI

if _section WTR; then
# WTR1 — the worktree-removal guard's mutation harness, for the reason CL1/CL2
# exist and one step further along the same road. The guard's own behavioral
# suite IS run, because run-all-tests.sh discovers every *.test.sh from disk.
# Its mutation harness is a *.mutation.sh, so that discovery never saw it, and
# on 2026-09-02 nothing in this engine ran it at all.
#
# That matters more than usual for this guard, because the fix landed that day
# was a fix to a FALSE POSITIVE — and the cheapest way to stop a guard
# false-firing is to stop it firing. The harness's M6 mutant is the arm that
# refuses that shortcut. An unrun mutant refuses nothing.
# Output KEPT on failure, with IP7's anchor and for IP7's reason.
WTR1_LOG="$(mktemp "${TMPDIR:-/tmp}/worktree-removal-mutations.XXXXXX")"
set +e; "$SCRIPT_DIR/guard-worktree-removal.mutation.sh" >"$WTR1_LOG" 2>&1; rc=$?; set -e
emit_mutation_case "WTR1.worktree-removal-mutations-all-load-bearing" "$rc" "$WTR1_LOG"
if [ "$rc" -ne 0 ]; then
    diag_or_tail "$(grep -E '^ *(FAIL|PASS|UNPROVEN)|^=== |survived or misfired|SURVIVED|NOT proven|NOT load-bearing' "$WTR1_LOG" \
        | grep -vE '^ *PASS' || true)" "$(cat "$WTR1_LOG" 2>/dev/null || true)"
fi
rm -f "$WTR1_LOG"

# ---------------------------------------------------------------------------
# LAYER MT — THE MODEL CAPABILITY ORDER IS DATA, AND THE PROSE IS HELD TO IT
# ---------------------------------------------------------------------------
# Every case below is a way the 2026-09-02 defect could come back: the order
# undeclared (MT1), declared but disagreeing with the alias set (MT2), stated
# in prose that quotes a DIFFERENT order than the data (MT3), and the original
# shape — "don't downgrade" in prose with no data behind it at all (MT4). MT5
# is the control: a sandbox with the declaration, the parser and a doctrine
# file that quotes it passes the layer, so the four refusals above are refusals
# and not a layer that fails on everything.

fi  # _section — WTR

if _section MT; then
# MT1 — MODEL_TIERS blank: the order is not declared.
ROOT="$(make_sandbox)"
_sed_inplace 's/^MODEL_TIERS=.*/MODEL_TIERS=""/' "$ROOT/orchestration.config"
set +e; MT_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "MT1.model-tiers-undeclared-fails" 2 "$rc"
if printf '%s' "$MT_OUT" | grep >/dev/null 'MT\. MODEL_TIERS in .*: MODEL_TIERS is blank'; then
    emit_case "MT1b.the-refusal-names-the-blank-declaration" 0 0
else
    emit_case "MT1b.the-refusal-names-the-blank-declaration" 0 1
fi
rm -rf "$ROOT"

# MT2 — the alias sets disagree: ALLOWED_MODELS names an alias the order does
# not rank. A spawn on that alias is one the guard cannot rank.
ROOT="$(make_sandbox)"
_sed_inplace 's/^MODEL_TIERS=.*/MODEL_TIERS="fable > opus > sonnet"/' "$ROOT/orchestration.config"
set +e; MT_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "MT2.model-tiers-alias-set-drifts-from-allowed-models-fails" 2 "$rc"
if printf '%s' "$MT_OUT" | grep >/dev/null 'MT\. MODEL_TIERS and ALLOWED_MODELS disagree.*haiku'; then
    emit_case "MT2b.the-refusal-names-the-unranked-alias" 0 0
else
    emit_case "MT2b.the-refusal-names-the-unranked-alias" 0 1
fi
rm -rf "$ROOT"

# MT3 — the doctrine file quotes a DIFFERENT order than the declaration.
ROOT="$(make_sandbox)"
printf '**Models:** the order is `MODEL_TIERS="opus > fable > sonnet > haiku"` and never move a teammate down it.\n' >"$ROOT/CLAUDE.md"
set +e; MT_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "MT3.doctrine-quotes-a-different-order-fails" 2 "$rc"
if printf '%s' "$MT_OUT" | grep >/dev/null 'MT\. .*CLAUDE.md quotes MODEL_TIERS="opus > fable > sonnet > haiku" but orchestration.config declares MODEL_TIERS="fable > opus > sonnet > haiku"'; then
    emit_case "MT3b.the-refusal-names-both-orders" 0 0
else
    emit_case "MT3b.the-refusal-names-both-orders" 0 1
fi
rm -rf "$ROOT"

# MT4 — the original defect: "don't downgrade" in prose, no data behind it.
ROOT="$(make_sandbox)"
printf '**Models:** definitions default to Opus for the judgment-critical roles (do not downgrade) and Sonnet for the rest.\n' >"$ROOT/CLAUDE.md"
set +e; MT_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "MT4.doctrine-says-downgrade-without-quoting-the-data-fails" 2 "$rc"
if printf '%s' "$MT_OUT" | grep >/dev/null 'MT\. .*says "downgrade" but never quotes the declaration'; then
    emit_case "MT4b.the-refusal-names-the-missing-quotation" 0 0
else
    emit_case "MT4b.the-refusal-names-the-missing-quotation" 0 1
fi
rm -rf "$ROOT"

# MT5 — CONTROL: declaration + parser + a doctrine file that quotes it -> the
# layer passes, two-sided canary included. Without this the four above could
# be a layer that fails on everything.
ROOT="$(make_sandbox)"
printf '**Models:** the order is `MODEL_TIERS="fable > opus > sonnet > haiku"`; do not move a teammate to a lower tier without saying why.\n' >"$ROOT/CLAUDE.md"
set +e; MT_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
# The probe styles its check mark (C_GREEN ... C_RESET) so "✓ MT." never
# appears as plain text; match the layer's own words, as MT1-MT4 do.
if printf '%s' "$MT_OUT" | grep >/dev/null 'MT\. capability order declared as data'; then
    emit_case "MT5.control-declared-and-quoted-passes-the-layer" 0 0
else
    emit_case "MT5.control-declared-and-quoted-passes-the-layer" 0 1
    diag_or_tail "$(printf '%s\n' "$MT_OUT" | grep -E 'MT\.' || true)" "$MT_OUT"
fi
rm -rf "$ROOT"

# MT6: a real refusal can exceed the pipe buffer. Preserve the real guard's
# behavior and append diagnostic padding only when it refuses. Matching the
# remedy must not turn the producer's SIGPIPE into a missing-remedy verdict.
ROOT="$(make_sandbox)"
python3 - "$ROOT/scripts/hooks/guard-worktree-isolation.sh" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
source = p.read_text()
needle = "set -eo pipefail\n"
assert source.count(needle) == 1
padding = '''trap 'rc=$?; if [ "$rc" -eq 2 ]; then printf "%65536s\\n" diagnostic-padding >&2; fi' EXIT
'''
p.write_text(source.replace(needle, needle + padding, 1))
PY
gen_sidecars "$ROOT"
set +e; MT_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
if [ "$rc" -eq 0 ] && printf '%s' "$MT_OUT" | grep >/dev/null 'MT\. capability order declared as data'; then
    emit_case "MT6.large-refusal-still-names-the-remedy" 0 0
else
    emit_case "MT6.large-refusal-still-names-the-remedy" 0 1
    diag_or_tail "$(printf '%s\n' "$MT_OUT" | grep -E 'MT\.' || true)" "$MT_OUT"
fi
# Negative control: padding cannot satisfy a remedy that is actually absent.
_sed_inplace 's/model-downgrade-ack:/missing-remedy:/g' "$ROOT/scripts/hooks/guard-worktree-isolation.sh"
gen_sidecars "$ROOT"
set +e; MT_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
if [ "$rc" -eq 2 ] && printf '%s' "$MT_OUT" | grep >/dev/null 'MT\. .*did NOT name the remedy line'; then
    emit_case "MT6b.large-refusal-without-remedy-still-fails" 0 0
else
    emit_case "MT6b.large-refusal-without-remedy-still-fails" 0 1
fi
rm -rf "$ROOT"

# ---------------------------------------------------------------------------
# LAYER MC — THE COST CEILING IS DATA, THE PROSE IS HELD TO IT, AND THE GUARD
#            THAT READS IT IS PROVEN ALIVE
# ---------------------------------------------------------------------------
# Layer MT's cases are the ways the CAPABILITY order could go wrong. These are
# the ways the SPEND order could, and one of them is a state MT has no analogue
# for: MC1, where the ceiling is simply NOT DECLARED. That is a WARN and not a
# failure on purpose — an adopter who has not chosen a ceiling is unconfigured,
# not broken, and failing the probe over it would make the first thing every
# adopter does be switching a layer off. The guard is loud about it per spawn,
# which is where that belongs.
#
# MC4 is the case this group exists for. Everything else here reads a config
# file; MC4 breaks the SHIPPED GUARD, re-mints its sidecar so the hash check
# cannot mask the result, and requires the layer to notice. Without it, a dead
# ceiling guard and a working one look identical to this suite — which is the
# exact false green this engine has shipped twice.

fi  # _section — MT

if _section MC; then
# MC1 — the ceiling is not declared: WARN, never a failure, and it says so.
ROOT="$(make_sandbox)"
_sed_inplace '/^MODEL_CEILING=/d' "$ROOT/orchestration.config"
set +e; MC_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "MC1.no-ceiling-declared-warns-and-does-not-fail" 0 "$rc"
if printf '%s' "$MC_OUT" | grep >/dev/null 'MC\. no MODEL_CEILING declared'; then
    emit_case "MC1b.the-warning-names-the-missing-declaration" 0 0
else
    emit_case "MC1b.the-warning-names-the-missing-declaration" 0 1
fi
rm -rf "$ROOT"

# MC2 — a ceiling MODEL_TIERS ranks nowhere. It is not a ceiling, and it looks
# exactly like one.
ROOT="$(make_sandbox)"
_sed_inplace 's/^MODEL_CEILING=.*/MODEL_CEILING="mythic"/' "$ROOT/orchestration.config"
set +e; MC_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "MC2.unrankable-ceiling-fails" 2 "$rc"
if printf '%s' "$MC_OUT" | grep >/dev/null 'MC\. MODEL_CEILING="mythic" is declared .* ranks it NOWHERE'; then
    emit_case "MC2b.the-refusal-names-the-unrankable-ceiling" 0 0
else
    emit_case "MC2b.the-refusal-names-the-unrankable-ceiling" 0 1
fi
rm -rf "$ROOT"

# MC3 — the doctrine file quotes a DIFFERENT ceiling than the declaration.
ROOT="$(make_sandbox)"
printf '**Models:** the cost ceiling is `MODEL_CEILING="haiku"` and the tier above it is for one-offs.\n' >"$ROOT/CLAUDE.md"
set +e; MC_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
emit_case "MC3.doctrine-quotes-a-different-ceiling-fails" 2 "$rc"
if printf '%s' "$MC_OUT" | grep >/dev/null 'MC\. .*CLAUDE.md quotes MODEL_CEILING="haiku" but orchestration.config declares MODEL_CEILING="opus"'; then
    emit_case "MC3b.the-refusal-names-both-ceilings" 0 0
else
    emit_case "MC3b.the-refusal-names-both-ceilings" 0 1
fi
rm -rf "$ROOT"

# MC4 — THE DEAD-GUARD CASE. The comparison is removed from the shipped guard
# and its sidecar re-minted, so the hash check passes and the ONLY thing left
# that can notice is the canary. A layer that reads config and never runs the
# guard passes MC1-MC3 and fails here.
ROOT="$(make_sandbox)"
MC_G="$ROOT/scripts/hooks/guard-model-ceiling.sh"
python3 - "$MC_G" <<'MCPY'
import sys
p = sys.argv[1]
src = open(p, encoding="utf-8").read()
old = '[ "$RESOLVED_RANK" -lt "$CEILING_RANK" ] || exit 0'
if old not in src:
    sys.stderr.write("MC4 FIXTURE STALE: the comparison line has moved\n")
    sys.exit(3)
open(p, "w", encoding="utf-8").write(src.replace(old, "exit 0", 1))
MCPY
MC4_MUTATED=$?
shasum -a 256 "$MC_G" | awk '{print $1}' > "$MC_G.sha256"
set +e; MC_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
if [ "$MC4_MUTATED" -ne 0 ]; then
    # The fixture could not be applied, so the case below would pass or fail for
    # a reason that has nothing to do with the layer. Say so, loudly.
    emit_case "MC4.dead-ceiling-guard-is-caught" 0 1
    emit_case "MC4b.the-refusal-says-the-guard-did-not-refuse" 0 1
else
    emit_case "MC4.dead-ceiling-guard-is-caught" 2 "$rc"
    if printf '%s' "$MC_OUT" | grep >/dev/null 'MC\. the guard did NOT refuse a spawn one tier ABOVE'; then
        emit_case "MC4b.the-refusal-says-the-guard-did-not-refuse" 0 0
    else
        emit_case "MC4b.the-refusal-says-the-guard-did-not-refuse" 0 1
    fi
fi
rm -rf "$ROOT"

# MC5 — CONTROL: declaration + guard + a doctrine file that quotes it -> the
# layer passes. Without it, MC2-MC4 could be a layer that fails on everything.
ROOT="$(make_sandbox)"
printf '**Models:** the cost ceiling is `MODEL_CEILING="opus"`; the tier above it is for super-critical one-offs.\n' >"$ROOT/CLAUDE.md"
set +e; MC_OUT="$(run_probe_in "$ROOT" 2>&1)"; rc=$?; set -e
if printf '%s' "$MC_OUT" | grep >/dev/null 'MC\. cost ceiling declared as data'; then
    emit_case "MC5.control-declared-and-quoted-passes-the-layer" 0 0
else
    emit_case "MC5.control-declared-and-quoted-passes-the-layer" 0 1
    diag_or_tail "$(printf '%s\n' "$MC_OUT" | grep -E 'MC\.' || true)" "$MC_OUT"
fi
rm -rf "$ROOT"

fi  # _section — MC

if _section MC6; then
# MC6 — the ceiling guard's own suite and its mutation harness, registered here
# for the reason WTR1 and WTI1 are: run-all-tests.sh discovers *.test.sh from
# disk, so the behavioral suite IS run, but a *.mutation.sh is invisible to that
# discovery. Every property of this guard is of the shape "it allowed the spawn
# and said nothing", which a hook that never ran also satisfies — so the mutants
# are the only thing standing between this guard and a permanent false green.
# Output KEPT on failure, with IP7's anchor and for IP7's reason.
MC6_LOG="$(mktemp "${TMPDIR:-/tmp}/model-ceiling-mutations.XXXXXX")"
set +e; "$SCRIPT_DIR/guard-model-ceiling.mutation.sh" >"$MC6_LOG" 2>&1; rc=$?; set -e
emit_mutation_case "MC6.model-ceiling-mutations-all-load-bearing" "$rc" "$MC6_LOG"
if [ "$rc" -ne 0 ]; then
    diag_or_tail "$(grep -E '^ *(FAIL|PASS|UNPROVEN)|^=== |survived or misfired|SURVIVED|NOT proven|NOT load-bearing' "$MC6_LOG" \
        | grep -vE '^ *PASS' || true)" "$(cat "$MC6_LOG" 2>/dev/null || true)"
fi
rm -f "$MC6_LOG"

fi  # _section — MC6

if _section WTI; then
# WTI1 — the spawn guard's CLAUSE 5 (staffing) mutation harness, registered for
# the reason WTR1 is: run-all-tests.sh discovers every *.test.sh from disk, so
# the guard's behavioral suite IS run, but a *.mutation.sh is invisible to that
# discovery and nothing would run this one.
#
# Clause 5's arms are the two-sided kind a corpse satisfies: "a generic dispatch
# without a hatch is refused" is passed by a guard that refuses everything, and
# "a statusline change needs no hatch" is passed by a guard that refuses
# nothing. M5, M6, M7 and M11 are the over-blocking mutants that refuse both
# shortcuts. An unrun mutant refuses nothing.
#
# ITS OUTPUT IS KEPT ON FAILURE, for the reason written above CL2 and proved
# again on 2026-09-13: "a mutation harness that can only say 'something
# survived' is a harness you end up re-running by hand anyway." This case went
# red on the runner and green on every machine anyone had. The CI log carried
# one line -- `(expected exit=0 got=1)` -- and discarding the harness's own
# report is what made the next several hours guesswork: the run was reproduced
# green on Linux five times over (10-core and 4-core and 2-core, at the runner's
# own job degree, inside the whole shard, and with $HOME absent) with no way to
# ask the red run which of the 23 mutants had survived. Diagnosis is not a fix
# and this is not one; it is the difference between a red CI that says what
# happened and a red CI that says only that it is red.
WTI1_LOG="$(mktemp "${TMPDIR:-/tmp}/staffing-mutations.XXXXXX")"
set +e; "$SCRIPT_DIR/guard-worktree-isolation.mutation.sh" >"$WTI1_LOG" 2>&1; rc=$?; set -e
emit_mutation_case "WTI1.staffing-gate-mutations-all-load-bearing" "$rc" "$WTI1_LOG"
if [ "$rc" -ne 0 ]; then
    # The anchor is `^ *` and not `^  `, deliberately. When a mutant's suite
    # goes red at the WRONG case, the harness prints the verdict AND then the
    # inner suite's own failing cases indented under it -- and those are the
    # lines that name which case actually broke, which is the whole question.
    # A two-space anchor drops exactly them.
    diag_or_tail "$(grep -E '^ *(FAIL|PASS|UNPROVEN)|survived or misfired|summary:' "$WTI1_LOG" \
        | grep -vE '^ *PASS' || true)" "$(cat "$WTI1_LOG" 2>/dev/null || true)"
fi
rm -f "$WTI1_LOG"

fi  # _section — WTI

if _section MF; then
# MF1 — the mechanical-findings mutation harness, registered for the reason
# WTR1 and WTI1 are: a *.mutation.sh is invisible to run-all-tests.sh's
# discovery. Unregistered, it is itself an `unrun-harness` finding of the very
# sweep it proves, and the sweep will write that row.
# Output KEPT on failure, with IP7's anchor and for IP7's reason.
MF1_LOG="$(mktemp "${TMPDIR:-/tmp}/mechanical-findings-mutations.XXXXXX")"
set +e; "$SCRIPT_DIR/mechanical-findings.mutation.sh" >"$MF1_LOG" 2>&1; rc=$?; set -e
emit_mutation_case "MF1.mechanical-findings-mutations-all-load-bearing" "$rc" "$MF1_LOG"
if [ "$rc" -ne 0 ]; then
    diag_or_tail "$(grep -E '^ *(FAIL|PASS|UNPROVEN)|^=== |survived or misfired|SURVIVED|NOT proven|NOT load-bearing' "$MF1_LOG" \
        | grep -vE '^ *PASS' || true)" "$(cat "$MF1_LOG" 2>/dev/null || true)"
fi
rm -f "$MF1_LOG"

fi  # _section — MF

if _section SA; then
# SA1/SA2 — the stated-actions gate's behavioral suite and its mutation
# harness, registered here for the reason IL6/IL7 are and with the same
# two-sided risk: "a stated-but-untaken action is refused" is satisfied by a
# gate that refuses everything, and "a same-turn dispatch is let through" by
# one that refuses nothing. The harness carries both kinds of mutant, and an
# unrun mutant refuses nothing. Output KEPT on failure, for CL2's reason.
# Output KEPT on failure, with case 40's filter and for its reason.
suite_discovered "SA1.stated-actions-gate-suite-is-discovered" "ass-kicker/tests/ass-kicker.test.sh"
# Output KEPT on failure, with case 40's EXCLUSION filter -- the same one SA1
# uses three lines above -- and for CL2's reason. The old `^  FAIL` anchor was
# the narrowest in this file: it printed nothing at all when the harness died
# BEFORE it could print a FAIL row, and SA2 is the last case before the summary,
# so that silence lands exactly where the report is all anyone has. Reproduced
# with a python3-less harness: `FATAL: python3 required` reached the report zero
# times. The anchor also dropped the `why` continuation and the closing
# `x N/M properties are NOT load-bearing` line on an ordinary failure.
SA2_LOG="$(mktemp "${TMPDIR:-/tmp}/stated-actions-mutations.XXXXXX")"
set +e; "$SCRIPT_DIR/../../ass-kicker/tests/stated-actions.mutation.sh" >"$SA2_LOG" 2>&1; rc=$?; set -e
emit_mutation_case "SA2.stated-actions-mutations-all-load-bearing" "$rc" "$SA2_LOG"
if [ "$rc" -ne 0 ]; then
    grep -vE '^ *PASS|^[[:space:]]*$' "$SA2_LOG" | sed 's/^/        /' || true
fi
rm -f "$SA2_LOG"

fi  # _section — SA

if _section SCR; then
# ---------------------------------------------------------------------------
# LAYER SCR — THE SCRATCH REAPER, AND THE DECLARATIONS IT DELETES BY
# ---------------------------------------------------------------------------
# SCR1 runs the reaper's own suite, which runs its mutation harness in turn.
# Both are needed and the second is the load-bearing one: nearly every case in
# that suite is a thing that must NOT happen — a live session's scratch
# survives, a registered workspace survives, a checkout survives — and every
# one of them passes perfectly against a reaper that deletes nothing at all,
# which is the state this mechanism was ordered to end.
#
# SCR2 is the cross-surface check, and it is the one that could not be written
# inside either file: the thresholds the CODE requires are read off
# scripts/lib/scratch-reaper.py, and every one of them must be DECLARED in
# orchestration.config. Two surfaces, so it has teeth — a threshold added to
# the code and never declared makes the reaper refuse to run at all (exit 2),
# on a schedule, at 04:40, where nobody is watching. The reverse direction is
# deliberately NOT checked: a declared value the code has stopped reading is
# dead config, not a deleter that stopped working.
suite_discovered "SCR1.scratch-reaper-suite-is-discovered" "scripts/scratch-reaper.test.sh"

SCR2_OUT="$(SCR_ENGINE="$SCRIPT_DIR/../.." python3 - <<'PY' 2>&1
import os, re, sys
eng = os.environ["SCR_ENGINE"]
src = open(os.path.join(eng, "scripts", "lib", "scratch-reaper.py"), encoding="utf-8").read()
# The names the code REFUSES to run without: need("X") / number("X"), plus the
# one read directly from the environment.
required = set(re.findall(r'(?:need|number)\("(SCRATCH_[A-Z_]+)"\)', src))
required |= set(re.findall(r'os\.environ\.get\("(SCRATCH_SESSION_PROCESS_NAMES)"\)', src))
cfg = open(os.path.join(eng, "orchestration.config"), encoding="utf-8").read()
declared = set(re.findall(r'^(SCRATCH_[A-Z_]+)=', cfg, re.M))
missing = sorted(required - declared)
if not required:
    print("NO required threshold could be read out of scratch-reaper.py — the "
          "derivation has gone blind and this check is asserting nothing.")
    sys.exit(1)
if missing:
    print("required by scripts/lib/scratch-reaper.py and NOT declared in "
          "orchestration.config: " + ", ".join(missing))
    sys.exit(1)
print("%d required threshold(s), all declared" % len(required))
PY
)"
rc=$?
emit_case "SCR2.every-threshold-the-code-needs-is-declared" 0 "$rc"
[ "$rc" -eq 0 ] || printf '        %s\n' "$SCR2_OUT"

fi  # _section — SCR

_trec run "$(_t)" - END
echo ""
echo "=== summary ==="
echo "passed: $PASS"
echo "failed: $FAIL"
if [ "$FAIL" -gt 0 ]; then
    echo "failing cases:"
    for n in "${FAIL_NAMES[@]}"; do echo "  - $n"; done
fi
if [ "$NOT_RUN" -gt 0 ]; then
    echo "not run (proved nothing, not counted as passed): $NOT_RUN"
    for n in "${NOT_RUN_NAMES[@]}"; do echo "  - $n"; done
fi

# The scoped banner goes LAST, under the verdict, because that is where an eye
# leaving a terminal lands. It names every case that did not run: a partial run
# whose gaps are not enumerated is indistinguishable from a full pass by
# reading, and this suite has twice shipped green over something that never ran.
if [ "$SCOPED" -eq 1 ]; then
    _skipped_total=0
    _skipped_report=""
    for _sid in $(_scope_all_sections); do
        case " $SCOPE_RUN " in *" $_sid "*) continue ;; esac
        _names="$(_scope_cases_of "$_sid" | tr '\n' ' ')"
        _n=$(printf '%s' "$_names" | wc -w | tr -d ' ')
        _skipped_total=$((_skipped_total + _n))
        _skipped_report="$_skipped_report
  $_sid ($_n): $_names"
    done
    echo ""
    echo "################################################################################"
    echo "#  SCOPED RUN — NOT A FULL PASS. $_skipped_total case(s) DID NOT RUN."
    echo "#  ran:$([ -n "$SCOPE_RUN" ] && printf ' %s' "$SCOPE_RUN")"
    echo "#  did NOT run:$_skipped_report"
    echo "#"
    echo "#  Exit code 3 means scoped-and-green. It is not 0 and must not be treated"
    echo "#  as one. Re-run with no arguments before landing anything."
    echo "################################################################################"
    [ "$FAIL" -gt 0 ] && exit 1
    exit 3
fi

[ "$FAIL" -gt 0 ] && exit 1
exit 0
