#!/usr/bin/env bash
#
# guard-worktree-isolation.test.sh — regression tests for
# scripts/hooks/guard-worktree-isolation.sh.
#
# Covers: (a) file-capable, no isolation, no marker -> exit 2 with the
# self-documenting remediation message; (b) with isolation:"worktree" ->
# exit 0; (c) read-only agent type -> exit 0 with NO isolation/name required;
# (d) additional file-capable role types with NO isolation and NO marker ->
# exit 2 (BLOCKED like any other type), WITH isolation:"worktree" -> exit 0,
# WITH the main-checkout-run: marker -> exit 0; (e) the main-checkout-run:
# marker present -> exit 0; (f) bad/bare name -> exit 2 (with isolation, with
# the marker — the name requirement is never relaxed); (g) unparseable Agent
# payload -> exit 2 (fail-closed); (h) non-Agent tool -> exit 0; (i) the
# marker use is best-effort logged to .claude/state/; (j) CLAUSE 3 — a name
# reused within a session (ledger / active roster / completed event-log) ->
# exit 2, a fresh name -> exit 0 and recorded, and the clause is inert without
# a resolvable session team dir; (k) the <model> token (middle segment of
# <role>-<model>-<identifier>) — old 2-part / garbage tokens BLOCKED (clause
# 2a); truthfulness (clause 2b): correct token via LIVE frontmatter default AND
# via explicit override, wrong token vs frontmatter AND vs override -> BLOCKED
# naming both models + the source, override wins over frontmatter (precedence),
# a non-live templates/ name never resolves (undeterminable), verbose-id
# override normalized to its alias, and undeterminable model (no live def /
# model:"inherit") accepts a valid alias while still enforcing the 2a floor.
# (m) CLAUSE 5 — the STAFFING gate: a generic/built-in agent type is refused
# unless the prompt carries a reasoned "generic-agent: <why>" line; the refusal
# names the roster alternative and why it matters; a well-formed reason passes
# and is logged; a bare/trivial/filler marker and a speed-or-convenience excuse
# are refused; harness utilities (statusline-setup, claude-code-guide) carry no
# hatch at all; general-purpose is closed via GENERIC_AGENT_TYPES; roster types
# are wholly unaffected; and the WORKTREE-ISOLATION exemption for allowlisted
# types still holds independently (block (c) proves it with the hatch present,
# so neither property can silently absorb the other).
#
# (n) CLAUSE 6 — the model capability order is DATA (orchestration.config
# MODEL_TIERS, parsed only by scripts/lib/model-tiers.sh): an explicit model:
# override to a LOWER tier than the definition's default is BLOCKED naming the
# teammate, both models, both ranks, the declaration and the exact line to add;
# a reasoned `model-downgrade-ack:` line permits it and is logged; a bare
# marker exempts nothing; equal-or-higher tier is SILENT (no output at all —
# the 2026-09-02 incident's own shapes: a Sonnet-default teammate on Fable, an
# Opus-default teammate on Fable); the clause fails OPEN with a notice on a
# missing parser / blank / malformed / unranking declaration and never
# announces on a spawn that needed no ranking; and a declaration that
# contradicts the alias names is OBEYED, proving nothing infers from a name.
#
# NOTE: the truthfulness cases resolve against the engine's REAL live agent defs
# (frank=opus); if that frontmatter default changes, update the expected tokens.
#
# Run directly: scripts/hooks/guard-worktree-isolation.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

# Internal mutation witnesses never return the full-suite success code. The
# harness supplies its existing red-case patterns, so there is one source of
# truth for each mutant's obligations. Missing/unknown witnesses fail closed.
WTI_SCOPE=""
WTI_WANTS=()
WTI_SEEN=()
if [ "$#" -gt 0 ]; then
    if [ "$#" -lt 3 ] || [ "$1" != --mutation-case ] || [ "${RICHOS_MUTATION_INNER:-}" != 1 ]; then
        echo 'ERROR: mutation witnesses require the internal harness and named cases' >&2; exit 2
    fi
    WTI_SCOPE="$2"; shift 2
    case "$WTI_SCOPE" in
        M[1-9]|M1[0-9]|M2[0-4]) ;;
        *) echo "ERROR: unknown mutation scope: $WTI_SCOPE" >&2; exit 2 ;;
    esac
    WTI_WANTS=("$@")
fi
wti_group() {
    [ -z "$WTI_SCOPE" ] && return 0
    local id
    for id in "$@"; do [ "$WTI_SCOPE" != "$id" ] || return 0; done
    return 1
}
wti_want() {
    [ -z "$WTI_SCOPE" ] && return 0
    local i pattern found=1
    for ((i=0; i<${#WTI_WANTS[@]}; i++)); do
        pattern="^${WTI_WANTS[$i]}"
        if [[ "$1" =~ $pattern ]]; then WTI_SEEN[$i]=1; found=0; fi
    done
    return "$found"
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# --- declare the root under test -------------------------------------------
# The hooks now resolve the governed repository from the SESSION (see
# scripts/lib/resolve-roots.sh), not from their own on-disk location. Run from
# a session seated in some OTHER repository, they would correctly resolve that
# repository, find no adoption marker, stand down — and every case below would
# pass by never running. Declaring the subject makes the suite independent of
# ambient session state, and exercises the env-override candidate for free.
#
# THE SUBJECT IS A SANDBOX COPY OF THE ENGINE'S ADOPTION, NOT THE ENGINE CHECKOUT (2026-10-05).
# Its acks (main-checkout-run, model-downgrade, hand-roll, generic-agent) and its unparseable
# payloads are appended to <entity>/.claude/state/. Seated on the engine, they landed in the
# engine's own state folder, which in the merge gate is the live plugin's audit record and an
# input of every engine check beside this suite (ci-shard.sh STATE-WRITTEN). The sandbox carries
# orchestration.config and the roster, and is a repository on `main` for the registry.
. "$SCRIPT_DIR/../lib/entity-sandbox.sh"
RICHOS_ENTITY_ROOT="$(entity_sandbox "$(cd "$SCRIPT_DIR/../.." && pwd)" --git)" \
    || { echo "FATAL: could not make the entity sandbox" >&2; exit 1; }
export RICHOS_ENTITY_ROOT
# CLAUDE_PROJECT_DIR is deliberately cleared: leaving the launching session's
# value in place would leave a second, lower-precedence candidate pointing
# somewhere irrelevant, and a future precedence change would then alter these
# results silently.
unset CLAUDE_PROJECT_DIR

HOOK="$SCRIPT_DIR/guard-worktree-isolation.sh"

# Clause 7 REGISTERS every allowed file-capable spawn in the workspace
# registry (docs/plans/worktree-spec-2026-09-11.md, point 3). The registry is
# pinned into a sandbox so no case can touch the operator's record, and the
# session is a real process of the suite's own, so no registration ever borrows
# the identity of the session that happens to be running this suite.
TX_SANDBOX="$(cd "$(mktemp -d -t guard-isolation-tx.XXXXXX)" && pwd -P)"
export RICHOS_WORKSPACES_DIR="$TX_SANDBOX/workspaces"
# Land-record lookups create their directory even when no land is performed.
# Keep it beside the private registry, outside both the caller HOME and registry.
export RICHOS_LAND_LOCKS_DIR="$TX_SANDBOX/land-locks"
# Point5 discards its worktree and consults the device collector. Its lock and
# failure record also belong to this fixture, not the caller's account state.
export RICHOS_TEST_DEVICES_DIR="$TX_SANDBOX/test-devices"
export TEST_DEVICE_FAILURES_STATE="$TX_SANDBOX/test-device-failures.json"
RICHOS_SESSION_PID="$(sh -c 'sleep 3600 >/dev/null 2>&1 & echo $!')"
export RICHOS_SESSION_PID
trap 'kill "$RICHOS_SESSION_PID" 2>/dev/null || true; rm -rf "$RICHOS_ENTITY_ROOT"' EXIT
TEST_SID="deadbeef-0000-4000-8000-000000000000"

# POINT 14, AT THE SPAWN. Since eedfbc7d (2026-09-12) registration REFUSES a
# repository with no current body of work, so a positive-control spawn into a
# real repository is exit 2 until the recording exists — "RECORDED when that
# work starts, before its first agent is spawned. Nothing infers it and nothing
# guesses it." Every case below that expects exit 0 needs it, so the suite does
# what Rich does, once, here, into its OWN pinned registry.
#
# The branch is DERIVED FROM THE REPOSITORY rather than typed, because this
# suite's subject is the engine's own checkout and that is a different branch on
# a runner, in a worktree and on a contributor's clone. An agent's own workspace
# branch (cc/, worktree-) is refused by record_integration by name, so those are
# never candidates.
WS_PY="$SCRIPT_DIR/../../mega-lander/workspaces.py"
ws_record() { python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get(sys.argv[2]) or "")' "$RICHOS_WORKSPACES_DIR/agents/$TEST_SID--$1.json" "$2" 2>/dev/null; }
ws_record_integration() { # <repo> <why>
    local repo="$1" why="$2" main b picked=""
    main="$(git -C "$repo" worktree list --porcelain 2>/dev/null | sed -n '1s/^worktree //p')"
    [ -n "$main" ] || return 0                  # not a repository: nothing to record
    for b in main master "$(git -C "$main" symbolic-ref --quiet --short HEAD 2>/dev/null)" \
             $(git -C "$main" for-each-ref --format='%(refname:short)' refs/heads 2>/dev/null); do
        case "$b" in ""|cc/*|worktree-*) continue ;; esac
        git -C "$main" rev-parse --verify --quiet "refs/heads/$b" >/dev/null 2>&1 || continue
        picked="$b"; break
    done
    if [ -z "$picked" ]; then
        printf '  FAIL  point-14 setup: no integration branch exists in %s\n' "$main"
        FAIL=$((FAIL + 1)); return 1
    fi
    python3 "$WS_PY" --entity "$repo" --session "$TEST_SID" integration \
        --repo "$repo" --branch "$picked" --why "$why" >/dev/null || {
        printf '  FAIL  point-14 setup: could not record %s on %s\n' "$repo" "$picked"
        FAIL=$((FAIL + 1)); return 1
    }
}

PASS=0
FAIL=0
ws_record_integration "$RICHOS_ENTITY_ROOT" "the spawn guard suite's body of work"

# run_case <name> <expected-exit> <json>
run_case() {
    local name="$1" expected="$2" json="$3"
    wti_want "$name" || return 0
    local actual
    printf '%s' "$json" | "$HOOK" >/dev/null 2>&1
    actual=$?
    if [ "$actual" -eq "$expected" ]; then
        printf '  PASS  %s\n' "$name"
        PASS=$((PASS + 1))
    else
        printf '  FAIL  %s (expected exit %s, got %s)\n' "$name" "$expected" "$actual"
        FAIL=$((FAIL + 1))
    fi
}

# run_case_silent <name> <json> — asserts exit 0 AND no output on either
# stream. "Silent" is a contract, not a nicety: clause 6 must say NOTHING on an
# equal-or-higher tier, and a notice there is a nag that becomes noise.
run_case_silent() {
    local name="$1" json="$2"
    wti_want "$name" || return 0
    local out actual
    out="$(printf '%s' "$json" | "$HOOK" 2>&1)"
    actual=$?
    if [ "$actual" -eq 0 ] && [ -z "$out" ]; then
        printf '  PASS  %s\n' "$name"
        PASS=$((PASS + 1))
    else
        printf '  FAIL  %s (expected exit 0 with NO output, got exit %s: %s)\n' "$name" "$actual" "${out:0:160}"
        FAIL=$((FAIL + 1))
    fi
}

# run_case_msg <name> <expected-substring> <json> — asserts stderr mentions it
run_case_msg() {
    local name="$1" needle="$2" json="$3"
    wti_want "$name" || return 0
    local out
    out="$(printf '%s' "$json" | "$HOOK" 2>&1 >/dev/null)"
    if printf '%s' "$out" | grep -qF "$needle"; then
        printf '  PASS  %s\n' "$name"
        PASS=$((PASS + 1))
    else
        printf '  FAIL  %s (stderr did not mention "%s")\n' "$name" "$needle"
        FAIL=$((FAIL + 1))
    fi
}

# make_fakebin_no_python3 — a PATH dir populated with symlinks to every
# external tool the hook needs EXCEPT python3, so `command -v python3` fails
# while everything else the hook shells out to still resolves normally.
# Mirrors the automation QA's fail-open repro (PATH lacking python3).
make_fakebin_no_python3() {
    local dir
    dir="$(mktemp -d -t fakebin-no-python3.XXXXXX)"
    local tools="cat grep sed cut tr date mkdir git mktemp basename dirname rm ln awk sort uniq wc head tail shasum sha256sum env"
    local t p
    for t in $tools; do
        p="$(command -v "$t" 2>/dev/null || true)"
        [ -n "$p" ] && ln -sf "$p" "$dir/$t"
    done
    echo "$dir"
}
BASH_BIN="$(command -v bash)"

# json_agent <subagent_type> <name> <isolation> <prompt>
json_agent() {
    local subagent="$1" name="$2" isolation="$3" prompt="$4"
    python3 - "$subagent" "$name" "$isolation" "$prompt" <<'PY'
import json, sys
subagent, name, isolation, prompt = sys.argv[1:5]
ti = {"prompt": prompt}
if subagent:
    ti["subagent_type"] = subagent
if name:
    ti["name"] = name
if isolation:
    ti["isolation"] = isolation
print(json.dumps({"tool_name": "Agent", "tool_input": ti, "session_id": "deadbeef-0000-4000-8000-000000000000", "tool_use_id": "toolu_test_agent"}))
PY
}

# json_agent_model <subagent> <name> <isolation> <model> <prompt> — as json_agent
# but also sets tool_input.model (the explicit per-spawn override that exercises
# clause 2b's override precedence).
json_agent_model() {
    python3 - "$1" "$2" "$3" "$4" "$5" <<'PY'
import json, sys
subagent, name, isolation, model, prompt = sys.argv[1:6]
ti = {"prompt": prompt}
if subagent: ti["subagent_type"] = subagent
if name: ti["name"] = name
if isolation: ti["isolation"] = isolation
if model: ti["model"] = model
print(json.dumps({"tool_name": "Agent", "tool_input": ti, "session_id": "deadbeef-0000-4000-8000-000000000000", "tool_use_id": "toolu_test_model"}))
PY
}

echo "=== guard-worktree-isolation tests ==="

if wti_group; then
# --- non-Agent tool passes through untouched ---
run_case "non-Agent tool (Bash)" 0 '{"tool_name":"Bash","tool_input":{"command":"ls"}}'
run_case "non-Agent tool (Read)" 0 '{"tool_name":"Read","tool_input":{"file_path":"/tmp/x"}}'

# --- (a) file-capable, no isolation, no marker -> exit 2 ---
run_case "no isolation, no marker, well-formed name, dev" 2 \
    "$(json_agent 'dev' 'dev-sonnet-1' '' 'Do the thing.')"
run_case_msg "block message is self-documenting: names isolation flag" 'isolation: "worktree"' \
    "$(json_agent 'dev' 'dev-sonnet-1' '' 'Do the thing.')"
run_case_msg "block message is self-documenting: names main-checkout-run fallback" 'main-checkout-run:' \
    "$(json_agent 'dev' 'dev-sonnet-1' '' 'Do the thing.')"

# --- (b) isolation:"worktree" -> exit 0 ---
run_case "isolation worktree, well-formed name" 0 \
    "$(json_agent 'dev' 'dev-sonnet-1' 'worktree' 'Do the thing.')"
run_case "isolation remote, well-formed name" 0 \
    "$(json_agent 'dev' 'dev-sonnet-r1' 'remote' 'Do the thing.')"
fi # mutation witness fixture group

# --- (c) read-only agent type -> exit 0, no isolation/name required ---
# THE ISOLATION EXEMPTION, PROVEN ON ITS OWN. Explore/Plan carry the clause-5
# staffing hatch here so that what these cases assert is exactly one thing: an
# allowlisted type needs NO isolation and NO name. Clause 5 is proven separately
# in block (m); if a future change collapsed the two, block (m) goes red.
if wti_group M5 M6; then
GA_OK='generic-agent: a read-only sweep across three repositories that no roster teammate has a seat for'
run_case "read-only type Explore, no isolation/name (isolation exemption holds)" 0 \
    "$(json_agent 'Explore' '' '' "Find where the login button is defined.
$GA_OK")"
run_case "read-only type Plan, no isolation/name (isolation exemption holds)" 0 \
    "$(json_agent 'Plan' '' '' "Plan the next sprint.
$GA_OK")"
run_case "read-only type claude-code-guide" 0 "$(json_agent 'claude-code-guide' '' '' 'Explain a feature.')"
run_case "read-only type statusline-setup" 0 "$(json_agent 'statusline-setup' '' '' 'Configure statusline.')"
fi # mutation witness fixture group

# --- (d) additional file-capable role types (e.g. device/visual QA) are
# blocked without isolation, exactly like any other file-capable type; WITH
# isolation:"worktree" or WITH the main-checkout-run: marker they pass.
if wti_group; then
run_case "deviceqa: no isolation, no marker -> BLOCKED" 2 \
    "$(json_agent 'deviceqa' 'deviceqa-sonnet-1' '' 'Run native device QA.')"
run_case "visualqa: no isolation, no marker -> BLOCKED" 2 \
    "$(json_agent 'visualqa' 'visualqa-sonnet-1' '' 'Adversarially verify the visual verdict.')"
run_case "funcqa: no isolation, no marker -> BLOCKED" 2 \
    "$(json_agent 'funcqa' 'funcqa-sonnet-1' '' 'Functional QA on staging.')"
run_case "deviceqa: isolation worktree -> exit 0" 0 \
    "$(json_agent 'deviceqa' 'deviceqa-sonnet-1' 'worktree' 'Run native device QA.')"
run_case "visualqa: isolation worktree -> exit 0" 0 \
    "$(json_agent 'visualqa' 'visualqa-sonnet-1' 'worktree' 'Adversarially verify the visual verdict.')"
run_case "funcqa: isolation worktree -> exit 0" 0 \
    "$(json_agent 'funcqa' 'funcqa-sonnet-1' 'worktree' 'Functional QA on staging.')"
run_case "deviceqa: main-checkout-run marker, no isolation -> exit 0" 0 \
    "$(json_agent 'deviceqa' 'deviceqa-sonnet-oneoff1' '' $'Run a one-off main-checkout task.\nmain-checkout-run: deliberate one-off main-checkout run.')"

# --- (e) main-checkout-run: marker present -> exit 0 ---
run_case "main-checkout-run marker, well-formed name, no isolation" 0 \
    "$(json_agent 'worker' 'worker-sonnet-oneoff1' '' $'Run a one-off main-checkout task.\nmain-checkout-run: needs main checkout HEAD for X.')"
run_case "marker mid-prompt on its own line still matches" 0 \
    "$(json_agent 'worker' 'worker-sonnet-oneoff2' '' $'Do the task.\n\nmain-checkout-run: precondition explained here.\n\nProceed once passing.')"

# --- (f) bad/bare name -> exit 2, regardless of isolation or marker ---
run_case "bare name with isolation set" 2 \
    "$(json_agent 'dev' 'dev' 'worktree' 'Do the thing.')"
run_case "run-together name with isolation set" 2 \
    "$(json_agent 'dev' 'dev1' 'worktree' 'Do the thing.')"
run_case "bare name with marker present (not relaxed)" 2 \
    "$(json_agent 'worker' 'worker' '' $'Do the task.\nmain-checkout-run: needs main checkout.')"
run_case "bare name for deviceqa, isolation set (not relaxed)" 2 \
    "$(json_agent 'deviceqa' 'deviceqa' 'worktree' 'Run device QA.')"
run_case_msg "malformed-name message present" 'missing/malformed name' \
    "$(json_agent 'dev' 'dev' 'worktree' 'Do the thing.')"

# --- (k) <model> token: format (2a) + truthfulness (2b) ---
# The <model> token (middle segment of <role>-<model>-<identifier>) must be a
# real alias AND match the model the instance boots on: an explicit
# tool_input.model override wins, else the model: frontmatter of the LIVE agent
# def .claude/agents/<subagent_type>.md (engine live defs: frank=opus). Generic
# roles here (dev, worker, ...) have NO live def -> undeterminable -> a valid
# alias token is accepted.
#
# Old 2-part name now BLOCKED — the model token is MISSING.
run_case "old 2-part name (missing model token), isolation set -> BLOCKED" 2 \
    "$(json_agent 'dev' 'dev-1' 'worktree' 'Do the thing.')"
run_case_msg "missing-model-token message names the 3-part form" '<role>-<model>-<identifier>' \
    "$(json_agent 'dev' 'dev-1' 'worktree' 'Do the thing.')"
# Garbage / non-alias model token -> BLOCKED (clause 2a).
run_case "garbage model token 'gpt' -> BLOCKED" 2 \
    "$(json_agent 'dev' 'dev-gpt-1' 'worktree' 'Do the thing.')"
run_case_msg "garbage-token message says 'not an allowed model'" 'is not an allowed model' \
    "$(json_agent 'dev' 'dev-gpt-1' 'worktree' 'Do the thing.')"

# --- (l) CLAUSE 2c — the CREATOR's stricter shape (scripts/lib/teammate-name.sh):
# lower-case only, role <=16 chars, identifier <=12 chars, EXACTLY three parts.
# 'zach-sonnet-multirepodemo1' is the real 2026-09-17 case: a 14-character
# identifier that passed this guard's own loose NAME_SHAPE_RE (clause 2, above)
# and was refused only by mega-lander/create-teammate-worktree.sh, one round
# trip later. It must be BLOCKED here too, in the creator's exact wording.
fi # mutation witness fixture group
if wti_group M24; then
run_case "C2C1 over-length identifier (14 chars) passes clause 2 but fails the creator's shape -> BLOCKED" 2 \
    "$(json_agent 'dev' 'zach-sonnet-multirepodemo1' 'worktree' 'Do the thing.')"
run_case_msg "C2C1m the refusal is the CREATOR's exact wording" \
    "'zach-sonnet-multirepodemo1' is not a teammate name of the form <role>-<model>-<identifier>" \
    "$(json_agent 'dev' 'zach-sonnet-multirepodemo1' 'worktree' 'Do the thing.')"
run_case "C2C2 POSITIVE CONTROL: a 12-character identifier is exactly within the shared shape -> allowed" 0 \
    "$(json_agent 'dev' 'dev-sonnet-multirepodem' 'worktree' 'Do the thing.')"

fi # mutation witness fixture group
if wti_group; then
# 'fable' is an allowed model alias too (orchestration.config's ALLOWED_MODELS
# default): a truthfully-named dev-fable-* spawn with an explicit model:"fable"
# override passes both set-membership (2a) and truthfulness (2b).
run_case "correct token via override (fable is an allowed alias, dev-fable-*)" 0 \
    "$(json_agent_model 'dev' 'dev-fable-1' 'worktree' 'fable' 'Do the thing.')"

# Correct token via LIVE frontmatter default (frank=opus).
run_case "correct token via live frontmatter default (frank->opus)" 0 \
    "$(json_agent 'frank' 'frank-opus-1' 'worktree' 'Stress-test the plan.')"
# Wrong token vs live frontmatter default -> BLOCKED (clause 2b).
run_case "wrong token vs frontmatter (frank is opus, name says sonnet) -> BLOCKED" 2 \
    "$(json_agent 'frank' 'frank-sonnet-2' 'worktree' 'Stress-test the plan.')"
run_case_msg "frontmatter-mismatch message names BOTH models" "claims 'sonnet' but this spawn boots on 'opus'" \
    "$(json_agent 'frank' 'frank-sonnet-2' 'worktree' 'Stress-test the plan.')"
run_case_msg "frontmatter-mismatch message is labelled 'untruthful model token'" 'untruthful model token' \
    "$(json_agent 'frank' 'frank-sonnet-2' 'worktree' 'Stress-test the plan.')"

# A non-live TEMPLATE name is NOT a spawnable type and must NOT resolve from
# .claude/agents/templates/ — so it is undeterminable and a valid alias passes
# regardless of the template's own frontmatter (templates/backend-engineer.md is
# sonnet; an opus token must still be accepted, proving templates never resolve).
run_case "template-named type does NOT resolve from templates/ (undeterminable)" 0 \
    "$(json_agent 'backend-engineer' 'be-opus-1' 'worktree' 'Build a feature.')"

# Explicit override WINS over frontmatter (precedence proof). frank=opus; override
# to sonnet, name claims sonnet -> exit 0. Since clause 6 (2026-09-02) that
# override is also a move to a LOWER tier, so the fixture carries the stated
# reason the clause demands — the precedence intent is unchanged, and block (n)
# below pins the no-reason refusal on its own.
run_case "correct token via override (override sonnet beats frontmatter opus)" 0 \
    "$(json_agent_model 'frank' 'frank-sonnet-ov1' 'worktree' 'sonnet' $'Stress-test.\nmodel-downgrade-ack: precedence fixture; the stated reason clause 6 requires for opus -> sonnet.')"
# Override sonnet, name claims opus (matches frontmatter but NOT the override) -> BLOCKED.
run_case "wrong token vs override (override sonnet, name says opus) -> BLOCKED" 2 \
    "$(json_agent_model 'frank' 'frank-opus-ov2' 'worktree' 'sonnet' 'Stress-test.')"
run_case_msg "override-mismatch message cites the override source" "explicit model override 'sonnet'" \
    "$(json_agent_model 'frank' 'frank-opus-ov2' 'worktree' 'sonnet' 'Stress-test.')"
# Verbose model id in the override is normalized to its alias.
run_case "verbose override id 'claude-opus-4-8' normalizes to opus" 0 \
    "$(json_agent_model 'dev' 'dev-opus-ov3' 'worktree' 'claude-opus-4-8' 'Do the thing.')"

# Undeterminable model -> accept a valid alias (never fail on unknowable info).
run_case "undeterminable (no live def, no override) accepts valid alias token" 0 \
    "$(json_agent 'worker' 'worker-haiku-u1' 'worktree' 'Do the thing.')"
# model:"inherit" override -> undeterminable even though a live def exists.
run_case "undeterminable via model:inherit accepts valid alias token" 0 \
    "$(json_agent_model 'frank' 'frank-haiku-u2' 'worktree' 'inherit' 'Stress-test.')"
# Undeterminable STILL requires a valid alias (garbage token -> BLOCKED).
run_case "undeterminable but garbage token still BLOCKED (2a floor holds)" 2 \
    "$(json_agent 'worker' 'worker-gpt-u3' 'worktree' 'Do the thing.')"

fi # mutation witness fixture group
if wti_group M12 M13 M14 M15 M18; then
# --- (n) CLAUSE 6 — a move to a LOWER capability tier is stated, never silent ---
# The order is DATA: this engine's orchestration.config declares
# MODEL_TIERS="fable > opus > sonnet > haiku", read only through
# scripts/lib/model-tiers.sh. Engine live defs: frank=opus, reed=sonnet. The
# first two cases are the REAL shapes from 2026-09-02 — the first is the one
# the killed guard had backwards (it would have refused it forever), the
# second is the spawn the CEO ordered himself.
run_case_silent "reed (sonnet default) on fable, no reason -> allowed, SILENT (an upgrade)" \
    "$(json_agent_model 'reed' 'reed-fable-c6a' 'worktree' 'fable' 'Read the sources.')"
run_case_silent "frank (opus default) on fable, no reason -> allowed, SILENT (same tier; the CEO's own spawn shape)" \
    "$(json_agent_model 'frank' 'frank-fable-c6b' 'worktree' 'fable' 'Stress-test.')"
run_case_silent "frank on opus, its own default, explicit -> SILENT" \
    "$(json_agent_model 'frank' 'frank-opus-c6c' 'worktree' 'opus' 'Stress-test.')"
run_case_silent "reed upgraded to opus -> SILENT" \
    "$(json_agent_model 'reed' 'reed-opus-c6d' 'worktree' 'opus' 'Read the sources.')"
run_case_silent "reed on sonnet, its own default, explicit -> SILENT" \
    "$(json_agent_model 'reed' 'reed-sonnet-c6e' 'worktree' 'sonnet' 'Read the sources.')"
run_case_silent "frank with NO override -> clause 6 never fires" \
    "$(json_agent 'frank' 'frank-opus-c6f' 'worktree' 'Stress-test.')"

C6_DOWN="$(json_agent_model 'frank' 'frank-sonnet-c6g' 'worktree' 'sonnet' 'Stress-test.')"
run_case "frank (opus default) on sonnet, no reason -> BLOCKED" 2 "$C6_DOWN"
run_case_msg "the tier refusal names the teammate" "'frank-sonnet-c6g' of 'frank'" "$C6_DOWN"
run_case_msg "the tier refusal names the definition default" "defaults to 'opus'" "$C6_DOWN"
run_case_msg "the tier refusal names the requested model" "requests model 'sonnet'" "$C6_DOWN"
run_case_msg "the tier refusal names the declaration it read" 'MODEL_TIERS="fable > opus > sonnet > haiku"' "$C6_DOWN"
run_case_msg "the tier refusal names the exact line to add" 'model-downgrade-ack: <why' "$C6_DOWN"
run_case_msg "the tier refusal says equal-or-higher never needs it" 'Equal or higher tier never needs it' "$C6_DOWN"
run_case "frank on haiku, no reason -> BLOCKED" 2 \
    "$(json_agent_model 'frank' 'frank-haiku-c6h' 'worktree' 'haiku' 'Stress-test.')"
run_case "reed (sonnet default) on haiku, no reason -> BLOCKED" 2 \
    "$(json_agent_model 'reed' 'reed-haiku-c6i' 'worktree' 'haiku' 'Read the sources.')"
run_case "frank on sonnet WITH a reasoned model-downgrade-ack: -> allowed" 0 \
    "$(json_agent_model 'frank' 'frank-sonnet-c6j' 'worktree' 'sonnet' $'Stress-test.\nmodel-downgrade-ack: a mechanical grep pass; the judgment seat is not needed for it.')"
run_case "a BARE model-downgrade-ack: exempts nothing -> BLOCKED" 2 \
    "$(json_agent_model 'frank' 'frank-sonnet-c6k' 'worktree' 'sonnet' $'Stress-test.\nmodel-downgrade-ack:')"
run_case "model-downgrade-ack: tolerates leading whitespace" 0 \
    "$(json_agent_model 'frank' 'frank-sonnet-c6l' 'worktree' 'sonnet' $'Stress-test.\n   model-downgrade-ack: leading-whitespace fixture, a real reason.')"
run_case "model-downgrade-ack: does NOT relax clause 2b (untruthful name still BLOCKED)" 2 \
    "$(json_agent_model 'frank' 'frank-opus-c6m' 'worktree' 'sonnet' $'Stress-test.\nmodel-downgrade-ack: a real reason, but the name lies about the model.')"
run_case "a verbose lower-tier override id is normalized before ranking -> BLOCKED" 2 \
    "$(json_agent_model 'frank' 'frank-sonnet-c6n' 'worktree' 'claude-sonnet-4-5' 'Stress-test.')"
fi # mutation witness fixture group

if wti_group M16 M17 M18 M19; then
# Sandbox entity: a judgment role defaulting to opus, and a declaration this
# block rewrites per case. Same shape as the (i) sandbox above.
C6SB="$(mktemp -d -t guard-c6.XXXXXX)"
mkdir -p "$C6SB/.claude/agents"
printf -- '---\nname: judge\nmodel: opus\n---\nA sandbox judgment role that exists only for these cases.\n' >"$C6SB/.claude/agents/judge.md"
c6_config() { printf 'ALLOWED_MODELS="fable opus sonnet haiku"\nMODEL_TIERS="%s"\n' "$1" >"$C6SB/orchestration.config"; }
c6_out() { printf '%s' "$1" | RICHOS_ENTITY_ROOT="$C6SB" "$HOOK" 2>&1; }
c6_case() { # <name> <expected-exit> <expected-substring-or-empty-for-silence> <json>
    local name="$1" expected="$2" needle="$3" json="$4" out rc
    wti_want "$name" || return 0
    out="$(c6_out "$json")"; rc=$?
    if [ "$rc" -eq "$expected" ] && { { [ -z "$needle" ] && [ -z "$out" ]; } || { [ -n "$needle" ] && printf '%s' "$out" | grep -qF "$needle"; }; }; then
        printf '  PASS  %s\n' "$name"; PASS=$((PASS + 1))
    else
        local want="with NO output"; [ -z "$needle" ] || want="mentioning \"$needle\""
        printf '  FAIL  %s (expected exit %s %s, got exit %s: %s)\n' "$name" "$expected" "$want" "$rc" "${out:0:160}"; FAIL=$((FAIL + 1))
    fi
}

# Accepted ack is logged, with both models; a refusal is never logged.
if wti_want "an accepted model-downgrade-ack: is logged to .claude/state/model-downgrade-acks.log with both models"; then
c6_config "fable > opus > sonnet > haiku"
C6_MARK="c6-log-canary-$$"
c6_out "$(json_agent_model 'judge' 'judge-sonnet-log1' 'worktree' 'sonnet' "Judge.
model-downgrade-ack: logging fixture $C6_MARK")" >/dev/null
C6_LOG="$C6SB/.claude/state/model-downgrade-acks.log"
if grep -qF "$C6_MARK" "$C6_LOG" 2>/dev/null && grep -qF "default=opus" "$C6_LOG" && grep -qF "requested=sonnet" "$C6_LOG"; then
    PASS=$((PASS + 1)); printf '  PASS  an accepted model-downgrade-ack: is logged to .claude/state/model-downgrade-acks.log with both models\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  an accepted model-downgrade-ack: is logged to .claude/state/model-downgrade-acks.log with both models\n'
fi
c6_out "$(json_agent_model 'judge' 'judge-sonnet-log2' 'worktree' 'sonnet' 'Judge, no reason.')" >/dev/null
if grep -qF "judge-sonnet-log2" "$C6_LOG" 2>/dev/null; then
    FAIL=$((FAIL + 1)); printf '  FAIL  a REFUSED downgrade was logged as if it were a waiver\n'
else
    PASS=$((PASS + 1)); printf '  PASS  a REFUSED downgrade is not logged (a refusal is not a waiver)\n'
fi
fi # mutation witness fixture group

# FAIL OPEN on the clause's own error — allowed, and announced.
c6_config "opus > sonnet"
c6_case "an alias MODEL_TIERS does not rank -> allowed (fail-open)" 0 "SKIPPED" \
    "$(json_agent_model 'judge' 'judge-haiku-fo1' 'worktree' 'haiku' 'Judge.')"
c6_case "the fail-open skip is announced, naming the unranked alias" 0 "'haiku'" \
    "$(json_agent_model 'judge' 'judge-haiku-fo1' 'worktree' 'haiku' 'Judge.')"
c6_case "the fail-open skip reaches the orchestrator as a systemMessage" 0 '"systemMessage"' \
    "$(json_agent_model 'judge' 'judge-haiku-fo1' 'worktree' 'haiku' 'Judge.')"
c6_config ""
c6_case "a BLANK MODEL_TIERS -> allowed (fail-open), announced as blank" 0 "blank" \
    "$(json_agent_model 'judge' 'judge-haiku-fo2' 'worktree' 'haiku' 'Judge.')"
c6_config "opus > > sonnet"
c6_case "a MALFORMED MODEL_TIERS -> allowed (fail-open), announced as malformed" 0 "empty" \
    "$(json_agent_model 'judge' 'judge-haiku-fo3' 'worktree' 'haiku' 'Judge.')"
c6_case "a broken declaration is NOT announced on a spawn that never needed it (no override)" 0 "" \
    "$(json_agent 'judge' 'judge-opus-fo4' 'worktree' 'Judge.')"
c6_case "a broken declaration is NOT announced on model:inherit" 0 "" \
    "$(json_agent_model 'judge' 'judge-opus-fo5' 'worktree' 'inherit' 'Judge.')"

# Parser library missing -> allowed (fail-open), announced.
if wti_want "parser library missing -> allowed (fail-open), announced"; then
C6NOLIB="$(mktemp -d -t guard-c6-nolib.XXXXXX)"
mkdir -p "$C6NOLIB/scripts/hooks" "$C6NOLIB/scripts/lib"
cp "$HOOK" "$C6NOLIB/scripts/hooks/guard-worktree-isolation.sh"
mkdir -p "$C6NOLIB/mega-lander"
cp "$SCRIPT_DIR/../../mega-lander/workspaces.py" "$C6NOLIB/mega-lander/"
# The registry's recovery adapter is part of the engine even with recovery off.
# Keep its production dependencies while omitting only the component under test.
cp "$SCRIPT_DIR/../lib/recovery.py" "$SCRIPT_DIR/../lib/recovery_controls.py" "$SCRIPT_DIR/../lib/operator_fences.py" "$SCRIPT_DIR/../lib/machine_settings.py" "$C6NOLIB/scripts/lib/"
cp "$SCRIPT_DIR/../lib/resolve-roots.sh" "$SCRIPT_DIR/../lib/resolve-model.sh" "$SCRIPT_DIR/../lib/resolve-main-checkout.sh" "$SCRIPT_DIR/../lib/teammate-name.sh" "$C6NOLIB/scripts/lib/"
cp "$SCRIPT_DIR/workspace-lifecycle.sh" "$SCRIPT_DIR/guard-sealed-worktree.sh" "$C6NOLIB/scripts/hooks/"
chmod +x "$C6NOLIB/scripts/hooks/guard-worktree-isolation.sh" "$C6NOLIB/scripts/hooks/workspace-lifecycle.sh" "$C6NOLIB/scripts/hooks/guard-sealed-worktree.sh"
c6_config "fable > opus > sonnet > haiku"
C6NL_OUT="$(printf '%s' "$(json_agent_model 'judge' 'judge-haiku-fo6' 'worktree' 'haiku' 'Judge.')" | RICHOS_ENTITY_ROOT="$C6SB" "$C6NOLIB/scripts/hooks/guard-worktree-isolation.sh" 2>&1)"
C6NL_RC=$?
if [ "$C6NL_RC" -eq 0 ] && printf '%s' "$C6NL_OUT" | grep -qF "model-tiers.sh is missing"; then
    PASS=$((PASS + 1)); printf '  PASS  parser library missing -> allowed (fail-open), announced\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  parser library missing -> allowed (fail-open), announced (got exit %s: %s)\n' "$C6NL_RC" "${C6NL_OUT:0:160}"
fi
rm -rf "$C6NOLIB"
fi # mutation witness fixture group

# DATA, NEVER INFERENCE. A declaration that contradicts the alias names is
# obeyed to the letter. If any consumer ranked by name, this block is the one
# that goes red — and it is the property the whole clause exists for.
c6_config "haiku > opus fable > sonnet"
c6_case "the declared order is obeyed even when it contradicts the alias names: opus-default on haiku -> SILENT" 0 "" \
    "$(json_agent_model 'judge' 'judge-haiku-dn1' 'worktree' 'haiku' 'Judge.')"
c6_case "the declared order is obeyed even when it contradicts the alias names: opus-default on fable (same tier) -> SILENT" 0 "" \
    "$(json_agent_model 'judge' 'judge-fable-dn2' 'worktree' 'fable' 'Judge.')"
c6_case "the declared order is obeyed even when it contradicts the alias names: opus-default on sonnet -> BLOCKED" 2 "model tier" \
    "$(json_agent_model 'judge' 'judge-sonnet-dn3' 'worktree' 'sonnet' 'Judge.')"
rm -rf "$C6SB"
fi # mutation witness fixture group

if wti_group; then
# --- (g) unparseable Agent payload -> exit 2 (fail-closed) ---
run_case "tool_input is not a dict (tool_name IS Agent)" 2 \
    '{"tool_name":"Agent","tool_input":"not-a-dict"}'
run_case "tool_input is null (tool_name IS Agent)" 2 \
    '{"tool_name":"Agent","tool_input":null}'

# --- (h) non-Agent tool_name entirely (structurally different payload) -> exit 0 ---
run_case "totally invalid JSON body (no Agent tool_name resolvable)" 0 'not json'

# --- (i) marker use is best-effort logged to .claude/state/ ---
SANDBOX="$(mktemp -d -t guard-worktree-isolation-log-test.XXXXXX)"
mkdir -p "$SANDBOX/scripts/hooks" "$SANDBOX/scripts/lib" "$SANDBOX/.claude"
cp "$HOOK" "$SANDBOX/scripts/hooks/guard-worktree-isolation.sh"
chmod +x "$SANDBOX/scripts/hooks/guard-worktree-isolation.sh"
# The copied hook resolves its library relative to its own location, and its
# root from the SESSION — so the sandbox needs both the library and an explicit
# declaration. Without the library it refuses to start; without the declaration
# it would write the log into the launching session's repository, which is the
# very behavior under repair.
cp "$SCRIPT_DIR/../lib/resolve-roots.sh" "$SCRIPT_DIR/../lib/resolve-model.sh" "$SCRIPT_DIR/../lib/resolve-main-checkout.sh" "$SCRIPT_DIR/../lib/teammate-name.sh" "$SANDBOX/scripts/lib/"
printf 'ALLOWED_MODELS="fable opus sonnet haiku"\n' >"$SANDBOX/orchestration.config"
printf '%s' "$(json_agent 'worker' 'worker-sonnet-log1' '' $'Do the task.\nmain-checkout-run: needs main checkout for X.')" \
    | RICHOS_ENTITY_ROOT="$SANDBOX" "$SANDBOX/scripts/hooks/guard-worktree-isolation.sh" >/dev/null 2>&1
if [ -f "$SANDBOX/.claude/state/main-checkout-runs.log" ] \
    && grep -qF "worker-sonnet-log1" "$SANDBOX/.claude/state/main-checkout-runs.log" \
    && grep -qF "main-checkout-run:" "$SANDBOX/.claude/state/main-checkout-runs.log"; then
    PASS=$((PASS + 1)); printf '  PASS  marker use logged to .claude/state/main-checkout-runs.log\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  marker use logged to .claude/state/main-checkout-runs.log\n'
fi
# A blocked spawn (no marker) is rejected outright — it never reaches the
# marker-logging path, so no log entry.
printf '%s' "$(json_agent 'deviceqa' 'deviceqa-sonnet-log2' '' 'Run device QA, no marker given.')" \
    | "$SANDBOX/scripts/hooks/guard-worktree-isolation.sh" >/dev/null 2>&1
if [ -f "$SANDBOX/.claude/state/main-checkout-runs.log" ] \
    && grep -qF "deviceqa-sonnet-log2" "$SANDBOX/.claude/state/main-checkout-runs.log"; then
    FAIL=$((FAIL + 1)); printf '  FAIL  blocked spawn (no marker) must NOT be logged as a marker use\n'
else
    PASS=$((PASS + 1)); printf '  PASS  blocked spawn (no marker) must NOT be logged as a marker use\n'
fi
rm -rf "$SANDBOX"

# --- python3 missing from PATH -> BLOCKED (fail-closed), loud stderr ---
# Mirrors the automation QA's repro: with no python3 resolvable on PATH, the guard must
# refuse (non-zero exit) rather than silently waving the spawn through.
FAKEBIN="$(make_fakebin_no_python3)"
NOPY_JSON="$(json_agent 'dev' 'dev-sonnet-1' 'worktree' 'Do the thing.')"
NOPY_OUT="$(printf '%s' "$NOPY_JSON" | PATH="$FAKEBIN" "$BASH_BIN" "$HOOK" 2>&1 1>/dev/null)"
NOPY_RC=$?
if [ "$NOPY_RC" -ne 0 ]; then
    PASS=$((PASS + 1)); printf '  PASS  python3 missing from PATH -> BLOCKS (exit %s)\n' "$NOPY_RC"
else
    FAIL=$((FAIL + 1)); printf '  FAIL  python3 missing from PATH -> expected non-zero exit, got 0 (FAIL-OPEN)\n'
fi
if printf '%s' "$NOPY_OUT" | grep -qF 'python3'; then
    PASS=$((PASS + 1)); printf '  PASS  python3-missing stderr names the missing interpreter\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  python3-missing stderr did not mention python3 (%s)\n' "$NOPY_OUT"
fi
rm -rf "$FAKEBIN"

# --- (j) CLAUSE 3 — name reuse is structurally impossible ---
# These use a controlled team dir via GUARD_ISOLATION_TEAMS_DIR so the reuse
# clause is active (the cases above use session "deadbeef…" with no team dir, so
# the clause is inert for them — that inertness is itself the proof the existing
# cases are unaffected).
REUSE_TEAMS="$(mktemp -d -t guard-isolation-reuse.XXXXXX)"
REUSE_SESSION="cafef00d-0000-4000-8000-000000000000"
REUSE_TEAM_DIR="$REUSE_TEAMS/session-cafef00d"
mkdir -p "$REUSE_TEAM_DIR"
export GUARD_ISOLATION_TEAMS_DIR="$REUSE_TEAMS"

# json_agent_sess <subagent> <name> <isolation> <prompt> <session_id>
json_agent_sess() {
    python3 - "$1" "$2" "$3" "$4" "$5" <<'PY'
import json, sys
subagent, name, isolation, prompt, sid = sys.argv[1:6]
ti = {"prompt": prompt}
if subagent: ti["subagent_type"] = subagent
if name: ti["name"] = name
if isolation: ti["isolation"] = isolation
print(json.dumps({"tool_name": "Agent", "tool_input": ti, "session_id": sid, "tool_use_id": "toolu_test_sess"}))
PY
}

# Seed the roster with an ACTIVE member and the event logs with a COMPLETED
# (roster-pruned) member — the two independent history sources.
cat >"$REUSE_TEAM_DIR/config.json" <<JSON
{ "name": "session-cafef00d",
  "members": [
    { "agentId": "team-lead@session-cafef00d", "name": "team-lead" },
    { "agentId": "dev-sonnet-active@session-cafef00d", "name": "dev-sonnet-active" }
  ] }
JSON
printf '{"event":"TaskCompleted","teammate":"eng-sonnet-done"}\n' >"$REUSE_TEAM_DIR/task-events.jsonl"
printf '{"event":"TeammateIdle","teammate":"qa-sonnet-gone"}\n' >"$REUSE_TEAM_DIR/idle-events.jsonl"

# (j1) fresh name -> exit 0, and THIS guard must NOT have written the ledger.
#
# THE NAME-BURN REGRESSION. This guard runs FIRST in a four-hook
# PreToolUse[Agent] chain, so anything it records is recorded before the later
# hooks have had their veto. Recording here burns the name of a spawn that a
# later hook then blocks — a teammate that never existed, whose corrected retry
# is refused as "reuse". Observed in production 2026-08-01 and reproduced
# against this engine 2026-08-28. The append moved to the PostToolUse partner
# (detect-nonnative-worktree.sh), which fires only for a call that ran.
#
# This case therefore asserts the ABSENCE of a write. Its positive counterpart
# — that the ledger is still WRITTEN, by the partner — is (j2), because
# "nobody writes it" would satisfy this case and destroy the reuse clause.
FRESH_JSON="$(json_agent_sess 'dev' 'dev-sonnet-fresh1' 'worktree' 'Do the thing.' "$REUSE_SESSION")"
printf '%s' "$FRESH_JSON" | "$HOOK" >/dev/null 2>&1
rc=$?
if [ "$rc" -eq 0 ] && ! grep -qxF "dev-sonnet-fresh1" "$REUSE_TEAM_DIR/spawned-names.log" 2>/dev/null; then
    PASS=$((PASS + 1)); printf '  PASS  reuse: fresh name allowed, and NOT burned into the ledger by this PreToolUse guard\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  reuse: fresh name allowed + not recorded here (exit %s)\n' "$rc"
fi

# (j1b) a spawn this guard allowed but a LATER hook blocked must be re-issuable
# under the SAME name. This is the burn, stated as a behavior.
printf '%s' "$FRESH_JSON" | "$HOOK" >/dev/null 2>&1
rc=$?
[ "$rc" -eq 0 ] && { PASS=$((PASS+1)); printf '  PASS  reuse: a name this guard approved but never executed is re-issuable\n'; } \
                || { FAIL=$((FAIL+1)); printf '  FAIL  reuse: name burned by an approval that never executed (exit %s)\n' "$rc"; }

# (j2) the ledger IS still consulted — write it the way the PostToolUse partner
# does, and the same name must now be refused. Without this arm, (j1) would
# also be satisfied by a guard that stopped reading the ledger entirely.
printf 'dev-sonnet-fresh1\n' >>"$REUSE_TEAM_DIR/spawned-names.log"
printf '%s' "$FRESH_JSON" | "$HOOK" >/dev/null 2>&1
rc=$?
[ "$rc" -eq 2 ] && { PASS=$((PASS+1)); printf '  PASS  reuse: ledger-recorded name reused -> block\n'; } \
                || { FAIL=$((FAIL+1)); printf '  FAIL  reuse: ledger-recorded name reused -> block (exit %s)\n' "$rc"; }

# (j3) ACTIVE roster name reused -> block
printf '%s' "$(json_agent_sess 'dev' 'dev-sonnet-active' 'worktree' 'x' "$REUSE_SESSION")" | "$HOOK" >/dev/null 2>&1
rc=$?
[ "$rc" -eq 2 ] && { PASS=$((PASS+1)); printf '  PASS  reuse: active roster name -> block\n'; } \
                || { FAIL=$((FAIL+1)); printf '  FAIL  reuse: active roster name -> block (exit %s)\n' "$rc"; }

# (j4) COMPLETED name absent from live roster but in event logs -> block (critical)
printf '%s' "$(json_agent_sess 'eng' 'eng-sonnet-done' 'worktree' 'x' "$REUSE_SESSION")" | "$HOOK" >/dev/null 2>&1
rc=$?
[ "$rc" -eq 2 ] && { PASS=$((PASS+1)); printf '  PASS  reuse: completed name (event-log only) -> block\n'; } \
                || { FAIL=$((FAIL+1)); printf '  FAIL  reuse: completed name (event-log only) -> block (exit %s)\n' "$rc"; }
printf '%s' "$(json_agent_sess 'qa' 'qa-sonnet-gone' 'worktree' 'x' "$REUSE_SESSION")" | "$HOOK" >/dev/null 2>&1
rc=$?
[ "$rc" -eq 2 ] && { PASS=$((PASS+1)); printf '  PASS  reuse: completed name (idle-log only) -> block\n'; } \
                || { FAIL=$((FAIL+1)); printf '  FAIL  reuse: completed name (idle-log only) -> block (exit %s)\n' "$rc"; }

# (j5) block message is self-documenting: names the collision + prescribes fresh id
REUSE_MSG="$(printf '%s' "$(json_agent_sess 'dev' 'dev-sonnet-active' 'worktree' 'x' "$REUSE_SESSION")" | "$HOOK" 2>&1 >/dev/null)"
if printf '%s' "$REUSE_MSG" | grep -qF "name 'dev-sonnet-active' has ALREADY been used" \
   && printf '%s' "$REUSE_MSG" | grep -qF "FRESH"; then
    PASS=$((PASS+1)); printf '  PASS  reuse: block message self-documenting (collision + fresh-id fix)\n'
else
    FAIL=$((FAIL+1)); printf '  FAIL  reuse: block message self-documenting\n'
fi

# (j6) reusing team-lead is NOT a collision (lead is not a spawnable name) — a
# fresh non-colliding name still passes even though team-lead is in roster
printf '%s' "$(json_agent_sess 'plan' 'plan-sonnet-1' 'worktree' 'x' "$REUSE_SESSION")" | "$HOOK" >/dev/null 2>&1
rc=$?
[ "$rc" -eq 0 ] && { PASS=$((PASS+1)); printf '  PASS  reuse: unrelated fresh name still allowed alongside roster\n'; } \
                || { FAIL=$((FAIL+1)); printf '  FAIL  reuse: unrelated fresh name still allowed (exit %s)\n' "$rc"; }

# (j7) inertness proof: with NO resolvable team dir (bad session), a name that
# WOULD collide in the seeded team is allowed (clause inert) — this is exactly
# why the un-sessioned cases above are unaffected.
printf '%s' "$(json_agent_sess 'dev' 'dev-sonnet-active' 'worktree' 'x' 'deadbeef-0000-4000-8000-000000000000')" | "$HOOK" >/dev/null 2>&1
rc=$?
[ "$rc" -eq 0 ] && { PASS=$((PASS+1)); printf '  PASS  reuse: clause inert without a resolvable team dir\n'; } \
                || { FAIL=$((FAIL+1)); printf '  FAIL  reuse: clause inert without a resolvable team dir (exit %s)\n' "$rc"; }

unset GUARD_ISOLATION_TEAMS_DIR
rm -rf "$REUSE_TEAMS"
fi # mutation witness fixture group

if wti_group M20 M21; then
# --- (l) CLAUSE 4 + CLAUSE 7 — a spawn is REGISTERED or it does not happen ---
#
# docs/plans/worktree-spec-2026-09-11.md, points 1-3: every non-native workspace
# is a cc/ workspace, registered when it is created (create-teammate-worktree.sh
# calls the registry first); a spawn naming one must be the spawn it was
# registered for; codex/ is never worked in; creating anything else is refused.
# Every refusal below sits beside the pass that differs from it in one fact.
WS_PY="$SCRIPT_DIR/../../mega-lander/workspaces.py"
CR="$(cd "$(mktemp -d -t guard-isolation-crossrepo.XXXXXX)" && pwd -P)"
CR_REPO="$CR/other"
mkdir -p "$CR_REPO"
git -C "$CR_REPO" init -q -b main
printf 'seed\n' >"$CR_REPO/seed.txt"
git -C "$CR_REPO" add -A
git -C "$CR_REPO" commit -q -m seed
# Point 14 again, for the second repository: register_cc refuses an unrecorded
# one exactly as register_spawn does, so the cc/ workspaces below cannot even be
# created until this body of work is recorded.
ws_record_integration "$CR_REPO" "the cross-repo cases' body of work"
register_cc() { # <session> <teammate> <path> <branch>
    python3 "$WS_PY" --session "$1" register-cc --name "$2" --repo "$CR_REPO" --path "$3" --branch "$4" >/dev/null \
        && git -C "$CR_REPO" worktree add -q -b "$4" "$3" \
        && python3 "$WS_PY" --session "$1" confirm-cc --name "$2" --path "$3" >/dev/null
}
# REGISTERED cc/ workspaces, one per teammate that is spawned into one below
register_cc "$TEST_SID" echo-opus-reg1 "$CR/other-wt/echo-opus-reg1" cc/echo-opus-reg1
register_cc "$TEST_SID" echo-opus-both1 "$CR/other-wt/echo-opus-both1" cc/echo-opus-both1
register_cc "feedface-1111-4000-8000-000000000001" echo-opus-other1 "$CR/other-wt/echo-opus-other1" cc/echo-opus-other1
# an UNREGISTERED cc/ workspace, a workspace NOT named cc/, and a codex/ one
git -C "$CR_REPO" worktree add -q -b cc/echo-opus-imp1 "$CR/other-wt/echo-opus-imp1"
git -C "$CR_REPO" worktree add -q -b human/x "$CR/other-wt/human-x"
git -C "$CR_REPO" worktree add -q -b codex/fix "$CR/other-wt/codex-fix"

json_cwd() { # <name> <isolation> <cwd> <prompt> [tool-use-id]
    python3 - "$1" "$2" "$3" "$4" "${5:-toolu_test_cwd}" <<'PY'
import json, sys
name, isolation, cwd, prompt, tuid = sys.argv[1:6]
ti = {"subagent_type": "dev", "name": name, "prompt": prompt}
if isolation: ti["isolation"] = isolation
if cwd: ti["cwd"] = cwd
print(json.dumps({"tool_name": "Agent", "tool_input": ti, "session_id": "deadbeef-0000-4000-8000-000000000000", "tool_use_id": tuid}))
PY
}

# (l1) cwd-only into a REGISTERED cc/ workspace -> BLOCKED at clause 7f
run_case "cwd into a REGISTERED cc/ workspace, no isolation -> BLOCKED (cwd-only)" 2 \
    "$(json_cwd 'echo-opus-reg1' '' "$CR/other-wt/echo-opus-reg1" 'Work there.')"
run_case_msg "cwd-only refusal names the cross-repo-worktree line to use instead" 'cross-repo-worktree: ' \
    "$(json_cwd 'echo-opus-reg1' '' "$CR/other-wt/echo-opus-reg1" 'Work there.')"
# (l2) cwd into a workspace not named cc/ -> BLOCKED (point 1)
run_case "cwd into a linked worktree NOT named cc/ -> BLOCKED" 2 \
    "$(json_cwd 'echo-opus-hum1' '' "$CR/other-wt/human-x" 'Work there.')"
run_case_msg "the refusal says every non-native workspace is named cc/" 'not a cc/ branch' \
    "$(json_cwd 'echo-opus-hum1' '' "$CR/other-wt/human-x" 'Work there.')"
# (l3) cwd = the MAIN checkout -> BLOCKED (a teammate never works in main)
run_case "cwd = a main checkout -> BLOCKED" 2 \
    "$(json_cwd 'echo-opus-main1' '' "$CR_REPO" 'Work there.')"
# (l4) cwd = nonexistent -> BLOCKED
run_case "cwd = nonexistent path -> BLOCKED" 2 \
    "$(json_cwd 'echo-opus-none1' '' "$CR/nowhere" 'Work there.')"
# (l5) cwd + isolation -> BLOCKED, named as mutually exclusive
run_case "cwd together with isolation:worktree -> BLOCKED (mutually exclusive)" 2 \
    "$(json_cwd 'echo-opus-both1' 'worktree' "$CR/other-wt/echo-opus-both1" 'Work there.')"
run_case_msg "cwd+isolation message says mutually exclusive" 'mutually exclusive' \
    "$(json_cwd 'echo-opus-both1' 'worktree' "$CR/other-wt/echo-opus-both1" 'Work there.')"
# (l6) a cwd spawn still needs a well-formed name (clause 2 not relaxed)
run_case "cwd spawn with a bare name -> BLOCKED (name contract not relaxed)" 2 \
    "$(json_cwd 'echo' '' "$CR/other-wt/echo-opus-reg1" 'Work there.')"
# (l7) the registered shape: isolation + cross-repo-worktree: line -> allowed and REGISTERED
run_case "cross-repo-worktree: REGISTERED cc/ path + isolation -> allowed" 0 \
    "$(json_cwd 'echo-opus-reg1' 'worktree' '' $'Do it.\ncross-repo-worktree: '"$CR/other-wt/echo-opus-reg1" toolu_test_reg1)"
if wti_group; then
if [ "$(ws_record echo-opus-reg1 tool_use_id)" = "toolu_test_reg1" ] && [ "$(ws_record echo-opus-reg1 isolation)" = "worktree" ]; then
    PASS=$((PASS + 1)); printf '  PASS  L07b the allowed spawn is REGISTERED: its cc/ registration now carries this spawn (point 3)\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  L07b the allowed spawn left no registration\n'
fi
fi # mutation witness fixture group
# (l8) an UNREGISTERED cc/ workspace -> BLOCKED, the spawn does not happen (point 3)
run_case "L08  cross-repo-worktree: UNREGISTERED cc/ path + isolation -> BLOCKED" 2 \
    "$(json_cwd 'echo-opus-imp1' 'worktree' '' $'Do it.\ncross-repo-worktree: '"$CR/other-wt/echo-opus-imp1")"
run_case_msg "L09  the refusal says the spawn could not be registered and names the helper" 'create-teammate-worktree.sh <repo> echo-opus-imp1' \
    "$(json_cwd 'echo-opus-imp1' 'worktree' '' $'Do it.\ncross-repo-worktree: '"$CR/other-wt/echo-opus-imp1")"
# (l10) codex/ is never worked in (point 2); a non-cc/ workspace is refused (point 1)
run_case_msg "L10  a codex/ workspace on the line -> BLOCKED, naming codex/" 'codex/ is never touched' \
    "$(json_cwd 'echo-opus-cx1' 'worktree' '' $'Do it.\ncross-repo-worktree: '"$CR/other-wt/codex-fix")"
run_case "L11  a workspace not named cc/ on the line -> BLOCKED" 2 \
    "$(json_cwd 'echo-opus-hum2' 'worktree' '' $'Do it.\ncross-repo-worktree: '"$CR/other-wt/human-x")"
# (l12) registered for ANOTHER session, or ANOTHER teammate -> BLOCKED
run_case_msg "L12  a cc/ workspace registered in ANOTHER session -> BLOCKED" 'no registration exists for echo-opus-other1 in this session' \
    "$(json_cwd 'echo-opus-other1' 'worktree' '' $'Do it.\ncross-repo-worktree: '"$CR/other-wt/echo-opus-other1")"
run_case_msg "L13  a cc/ workspace registered for a DIFFERENT teammate -> BLOCKED" 'no registration exists for echo-opus-stranger1' \
    "$(json_cwd 'echo-opus-stranger1' 'worktree' '' $'Do it.\ncross-repo-worktree: '"$CR/other-wt/echo-opus-both1")"
# (l14) the workspace DRIFTED off its registered branch -> BLOCKED; back on it -> allowed
git -C "$CR/other-wt/echo-opus-both1" checkout -q -b cc/drifted-away
run_case_msg "L14  a registered workspace now on a DIFFERENT branch -> BLOCKED, naming both" "not the registered branch 'cc/echo-opus-both1'" \
    "$(json_cwd 'echo-opus-both1' 'worktree' '' $'Do it.\ncross-repo-worktree: '"$CR/other-wt/echo-opus-both1" toolu_test_both1)"
git -C "$CR/other-wt/echo-opus-both1" checkout -q cc/echo-opus-both1
run_case "L15  ...and back on its registered branch -> allowed (positive control for L14)" 0 \
    "$(json_cwd 'echo-opus-both1' 'worktree' '' $'Do it.\ncross-repo-worktree: '"$CR/other-wt/echo-opus-both1" toolu_test_both1)"
# (l16) a prompt that instructs a hand-rolled worktree -> BLOCKED; hand-roll-ack: -> allowed + logged
run_case "prompt instructing 'git -C <repo> worktree add' -> BLOCKED" 2 \
    "$(json_cwd 'echo-opus-hr1' 'worktree' '' "Create a worktree: git -C $CR_REPO worktree add $CR/other-wt/x -b x, then work.")"
run_case_msg "hand-roll refusal names the helper and the ack" 'hand-roll-ack:' \
    "$(json_cwd 'echo-opus-hr1' 'worktree' '' "Create a worktree: git -C $CR_REPO worktree add $CR/other-wt/x -b x, then work.")"
run_case "prompt instructing 'git worktree add' with a hand-roll-ack: line -> allowed" 0 \
    "$(json_cwd 'echo-opus-hr2' 'worktree' '' $'Fix the helper that runs git worktree add.\nhand-roll-ack: this task is about the worktree tooling itself' toolu_test_hr2)"
run_case "an ordinary prompt mentioning neither -> allowed (no false positive)" 0 \
    "$(json_cwd 'echo-opus-plain1' 'worktree' '' 'Add a worktree-lifecycle section to the README and commit.' toolu_test_plain1)"
if wti_group; then
if grep -qF 'echo-opus-hr2' "$RICHOS_ENTITY_ROOT/.claude/state/hand-roll-acks.log" 2>/dev/null; then
    PASS=$((PASS + 1)); printf '  PASS  hand-roll-ack use is logged to .claude/state/hand-roll-acks.log\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  hand-roll-ack use not logged\n'
fi
fi # mutation witness fixture group

fi # mutation witness fixture group

# ---------------------------------------------------------------------------
# (q) CLAUSE 7 — REGISTRATION AT SPAWN, and point 5's new-work half.
# ---------------------------------------------------------------------------
M_GOOD_Q='generic-agent: a read-only sweep across three repositories that no roster teammate has a seat for'

# (q1) a native isolation spawn is registered: its name, this session, isolation worktree
if wti_want "Q01"; then
printf '%s' "$(json_agent 'dev' 'dev-sonnet-q1' 'worktree' 'Do the thing.')" | "$HOOK" >/dev/null 2>&1; rc=$?
if [ "$rc" -eq 0 ] && [ "$(ws_record dev-sonnet-q1 isolation)" = "worktree" ] && [ "$(ws_record dev-sonnet-q1 tool_use_id)" = "toolu_test_agent" ]; then
    PASS=$((PASS + 1)); printf '  PASS  Q01  an allowed native spawn is REGISTERED (name, session, tool_use_id, isolation)\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  Q01  native registration (rc=%s)\n' "$rc"
fi
fi # mutation witness fixture group

# (q7g) CLAUSE 7g — A DRY EVALUATION IS NOT A LIVE ONE, AND THE DIFFERENCE IS
# THE WRITE. scripts/spawn.sh evaluates this guard against a payload it has not
# sent yet, so a brief with three problems is fixed once instead of costing
# three dispatches. The danger such a marker always carries is that it becomes a
# way to spawn WITHOUT a registration, which is the one thing point 3 forbids.
# It cannot, and these cases are why: the dry path needs the marker AND no
# tool_use_id, and only the platform mints a tool_use_id.
json_check() { # <name> <tool-use-id|""> <marker: yes|no>
    python3 - "$1" "$2" "$3" <<'PY'
import json, sys
name, tuid, marker = sys.argv[1:4]
d = {"tool_name": "Agent", "session_id": "deadbeef-0000-4000-8000-000000000000",
     "tool_input": {"subagent_type": "dev", "name": name, "isolation": "worktree",
                    "prompt": "Do the thing."}}
if tuid:
    d["tool_use_id"] = tuid
if marker == "yes":
    d["richos_spawn_check"] = {"planned": []}
print(json.dumps(d))
PY
}

# A LIVE call that ALSO carries the marker is still REGISTERED: the marker alone
# decides nothing. This is the case that would go green for a guard that let the
# marker turn a real spawn into an unregistered one.
if wti_want "Q7G1"; then
printf '%s' "$(json_check dev-sonnet-g7a toolu_test_g7a yes)" | "$HOOK" >/dev/null 2>&1; rc=$?
if [ "$rc" -eq 0 ] && [ "$(ws_record dev-sonnet-g7a tool_use_id)" = "toolu_test_g7a" ]; then
    PASS=$((PASS + 1)); printf '  PASS  Q7G1  a LIVE payload carrying richos_spawn_check is still REGISTERED\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  Q7G1  a live payload carrying the marker must still be registered (rc=%s, tool_use_id=%s)\n' "$rc" "$(ws_record dev-sonnet-g7a tool_use_id)"
fi
fi # mutation witness fixture group

# A DRY payload — the marker and no tool_use_id — is evaluated and allowed, and
# writes NOTHING. Both halves matter: allowed proves the evaluation runs, and
# nothing written proves it is not a registration.
if wti_want "Q7G2"; then
printf '%s' "$(json_check dev-sonnet-g7b "" yes)" | "$HOOK" >/dev/null 2>&1; rc=$?
if [ "$rc" -eq 0 ] && [ ! -e "$RICHOS_WORKSPACES_DIR/agents/$TEST_SID--dev-sonnet-g7b.json" ]; then
    PASS=$((PASS + 1)); printf '  PASS  Q7G2  a DRY payload is evaluated, allowed, and writes NO registration\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  Q7G2  dry evaluation (rc=%s, record present=%s)\n' "$rc" "$( [ -e "$RICHOS_WORKSPACES_DIR/agents/$TEST_SID--dev-sonnet-g7b.json" ] && echo yes || echo no )"
fi
fi # mutation witness fixture group

# NEGATIVE CONTROL. Without the marker, a payload with no tool_use_id is what it
# has always been: unregisterable, so the spawn does not happen. Without this
# case Q7G2 would also be green for a guard that had simply stopped requiring a
# tool_use_id at all.
run_case "Q7G3  NEGATIVE CONTROL: no tool_use_id and NO marker -> BLOCKED (unregisterable)" 2 \
    "$(json_check dev-sonnet-g7c "" no)"

if wti_group; then
# (q11) SYNCHRONOUS file-writing spawn -> refused; async/unspecified -> allowed
SYNC_JSON="$(python3 -c '
import json
print(json.dumps({"tool_name": "Agent", "tool_input": {"subagent_type": "dev", "name": "dev-sonnet-sync1", "isolation": "worktree", "prompt": "Do it.", "run_in_background": False}, "session_id": "deadbeef-0000-4000-8000-000000000000", "tool_use_id": "toolu_test_sync"}))')"
run_case "Q11  run_in_background: false on a file-capable spawn -> BLOCKED" 2 "$SYNC_JSON"
run_case_msg "Q12  the refusal names the background form" 'background teammate' "$SYNC_JSON"
ASYNC_JSON="$(python3 -c '
import json
print(json.dumps({"tool_name": "Agent", "tool_input": {"subagent_type": "dev", "name": "dev-sonnet-async1", "isolation": "worktree", "prompt": "Do it.", "run_in_background": True}, "session_id": "deadbeef-0000-4000-8000-000000000000", "tool_use_id": "toolu_test_async"}))')"
run_case "Q13  run_in_background: true -> allowed (positive control for Q11)" 0 "$ASYNC_JSON"

# (q14) a main-checkout-run spawn is registered with no workspaces
printf '%s' "$(python3 -c '
import json
print(json.dumps({"tool_name": "Agent", "tool_input": {"subagent_type": "worker", "name": "worker-sonnet-mcr1", "prompt": "Do the task.\nmain-checkout-run: needs main checkout HEAD."}, "session_id": "deadbeef-0000-4000-8000-000000000000", "tool_use_id": "toolu_test_mcr"}))')" | "$HOOK" >/dev/null 2>&1; rc=$?
if [ "$rc" -eq 0 ] && [ "$(ws_record worker-sonnet-mcr1 tool_use_id)" = "toolu_test_mcr" ] && [ "$(ws_record worker-sonnet-mcr1 workspaces)" = "" ]; then
    PASS=$((PASS + 1)); printf '  PASS  Q14  a main-checkout-run spawn is registered, with no workspaces\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  Q14  main-checkout-run registration (rc=%s)\n' "$rc"
fi

# (q15) NO tool_use_id in the payload -> refused (nothing to key the registration by)
NOTU_JSON="$(python3 -c '
import json
print(json.dumps({"tool_name": "Agent", "tool_input": {"subagent_type": "dev", "name": "dev-sonnet-notu1", "isolation": "worktree", "prompt": "Do it."}, "session_id": "deadbeef-0000-4000-8000-000000000000"}))')"
run_case "Q15  a payload with NO tool_use_id -> BLOCKED (it cannot be registered)" 2 "$NOTU_JSON"

# (q16) A READ-ONLY TYPE IS EXEMPT FROM ISOLATION AND STILL REGISTERED.
# It used to reach neither clause 7 nor a registration, and the point-9 lock-out
# finds an agent THROUGH its registration — so it never fired for a read-only
# agent once. `Explore` carries every tool except Edit/Write/NotebookEdit, i.e.
# it carries Bash, so a restarted finished Explore could write. It is registered
# with NO workspaces (keyed by its tool_use_id, since it needs no name), and it
# is still allowed with no isolation and no name.
Q16_TUID="toolu_test_ro_q16"
Q16_JSON="$(python3 -c '
import json
print(json.dumps({"tool_name": "Agent",
                  "tool_input": {"subagent_type": "Explore", "prompt": "Find where the login button is defined.\ngeneric-agent: a read-only sweep across three repositories that no roster teammate has a seat for"},
                  "session_id": "deadbeef-0000-4000-8000-000000000000",
                  "tool_use_id": "toolu_test_ro_q16"}))')"
run_case "Q16  a read-only type is allowed with no isolation and no name" 0 "$Q16_JSON"
Q16_REC="$RICHOS_WORKSPACES_DIR/agents/deadbeef-0000-4000-8000-000000000000--ro-$Q16_TUID.json"
if [ -f "$Q16_REC" ] \
   && [ "$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))["workspaces"]))' "$Q16_REC")" = "0" ] \
   && [ "$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["readonly"])' "$Q16_REC")" = "True" ]; then
    PASS=$((PASS + 1)); printf '  PASS  Q17  ...and it IS registered, with no workspaces, so the lock-out can find it\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  Q17  a read-only spawn was not registered: the point-9 lock-out can never find it\n'
fi

# (q18) LIFECYCLE COMPONENT MISSING -> refused, naming it (fail-closed)
NOTX="$(cd "$(mktemp -d -t guard-isolation-notx.XXXXXX)" && pwd -P)"
mkdir -p "$NOTX/scripts/hooks" "$NOTX/scripts/lib" "$NOTX/.claude/agents"
cp "$HOOK" "$SCRIPT_DIR/guard-sealed-worktree.sh" "$SCRIPT_DIR/workspace-lifecycle.sh" "$NOTX/scripts/hooks/"
cp "$SCRIPT_DIR/../lib/resolve-roots.sh" "$SCRIPT_DIR/../lib/resolve-model.sh" "$SCRIPT_DIR/../lib/resolve-main-checkout.sh" "$SCRIPT_DIR/../lib/model-tiers.sh" "$SCRIPT_DIR/../lib/teammate-name.sh" "$NOTX/scripts/lib/"
cp "$RICHOS_ENTITY_ROOT/orchestration.config" "$NOTX/"
cp "$RICHOS_ENTITY_ROOT/.claude/agents/"*.md "$NOTX/.claude/agents/" 2>/dev/null || true
chmod +x "$NOTX/scripts/hooks/"*.sh
NOTX_OUT="$(printf '%s' "$(json_agent 'dev' 'dev-sonnet-notx1' 'worktree' 'Do it.')" | RICHOS_ENTITY_ROOT="$NOTX" "$NOTX/scripts/hooks/guard-worktree-isolation.sh" 2>&1 >/dev/null)"; rc=$?
if [ "$rc" -eq 2 ] && printf '%s' "$NOTX_OUT" | grep -qF 'lifecycle component MISSING: mega-lander/workspaces.py'; then
    PASS=$((PASS + 1)); printf '  PASS  Q18  a file-capable spawn with the workspace registry MISSING -> BLOCKED, naming it (fail-closed)\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  Q18  missing lifecycle component (exit %s): %s\n' "$rc" "${NOTX_OUT:0:200}"
fi
# (q18c) the lock-out present but not executable -> refused; (q18d) healthy -> allowed
mkdir -p "$NOTX/mega-lander"
cp "$SCRIPT_DIR/../../mega-lander/workspaces.py" "$NOTX/mega-lander/"
# The registry's recovery adapter is part of the engine even with recovery off.
# Keep its production dependencies while omitting only the component under test.
cp "$SCRIPT_DIR/../lib/recovery.py" "$SCRIPT_DIR/../lib/recovery_controls.py" "$SCRIPT_DIR/../lib/operator_fences.py" "$SCRIPT_DIR/../lib/machine_settings.py" "$NOTX/scripts/lib/"
chmod -x "$NOTX/scripts/hooks/guard-sealed-worktree.sh"
NOTX_OUT="$(printf '%s' "$(json_agent 'dev' 'dev-sonnet-brk2' 'worktree' 'Do it.')" | RICHOS_ENTITY_ROOT="$NOTX" "$NOTX/scripts/hooks/guard-worktree-isolation.sh" 2>&1 >/dev/null)"; rc=$?
if [ "$rc" -eq 2 ] && printf '%s' "$NOTX_OUT" | grep -qF 'lifecycle component NOT EXECUTABLE: scripts/hooks/guard-sealed-worktree.sh'; then
    PASS=$((PASS + 1)); printf '  PASS  Q18c a file-capable spawn with the lock-out NOT EXECUTABLE -> BLOCKED, naming it\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  Q18c non-executable lock-out (exit %s): %s\n' "$rc" "${NOTX_OUT:0:200}"
fi
chmod +x "$NOTX/scripts/hooks/guard-sealed-worktree.sh"
NOTX_OUT="$(printf '%s' "$(json_agent 'dev' 'dev-sonnet-brk3' 'worktree' 'Do it.')" | RICHOS_ENTITY_ROOT="$NOTX" "$NOTX/scripts/hooks/guard-worktree-isolation.sh" 2>&1 >/dev/null)"; rc=$?
if [ "$rc" -eq 0 ]; then
    PASS=$((PASS + 1)); printf '  PASS  Q18d ...and the same sandbox with every component healthy ALLOWS the spawn (positive control)\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  Q18d healthy sandbox refused (exit %s): %s\n' "$rc" "${NOTX_OUT:0:200}"
fi
rm -rf "$NOTX"
fi # mutation witness fixture group

if wti_group M1 M2 M3 M4 M5 M6 M7 M8 M9 M10 M11; then
# ---------------------------------------------------------------------------
# (m) CLAUSE 5 — THE STAFFING GATE.
# ---------------------------------------------------------------------------
# READONLY_ALLOWLIST answers "does this type need a worktree?". It never
# answered "may delegated work be STAFFED here?", and on 2026-09-02 the two
# were read as one permission: an engine-wide audit went to `Explore` because a
# roster teammate would have needed a worktree created first. These cases are a
# TWO-SIDED canary — a gate that refuses everything would satisfy "refused
# without a hatch" and is caught by the WITH-hatch and roster arms.

M_GOOD='generic-agent: a read-only sweep across three repositories that no roster teammate has a seat for'

# (m1) NO hatch -> BLOCKED, for every non-utility generic type.
run_case "Explore with NO generic-agent: line -> BLOCKED" 2 \
    "$(json_agent 'Explore' '' '' 'Audit every hook in the engine and report what is unwired.')"
run_case "Plan with NO generic-agent: line -> BLOCKED" 2 \
    "$(json_agent 'Plan' '' '' 'Draft the sprint plan.')"

# (m2) the refusal NAMES THE ALTERNATIVE and says why it matters — a refusal
# that only says no teaches nothing and gets worked around.
run_case_msg "refusal names the roster teammate as the fix" 'ROSTER TEAMMATE' \
    "$(json_agent 'Explore' '' '' 'Audit every hook in the engine.')"
run_case_msg "refusal says a generic agent is invisible in the team display" "team display" \
    "$(json_agent 'Explore' '' '' 'Audit every hook in the engine.')"
run_case_msg "refusal says a generic agent leaves no commit" 'leaves no commit' \
    "$(json_agent 'Explore' '' '' 'Audit every hook in the engine.')"
run_case_msg "refusal names the hatch line by its exact shape" 'generic-agent: <why no roster teammate fits this work>' \
    "$(json_agent 'Explore' '' '' 'Audit every hook in the engine.')"
run_case_msg "refusal separates the isolation exemption from the staffing question" 'has never been a permission to STAFF' \
    "$(json_agent 'Explore' '' '' 'Audit every hook in the engine.')"

# (m3) a WELL-FORMED reason PASSES (the other side of the canary).
run_case "Explore WITH a well-formed generic-agent: reason -> allowed" 0 \
    "$(json_agent 'Explore' '' '' "Audit every hook in the engine.
$M_GOOD")"
run_case "the hatch is recognized anywhere in the prompt, not only on line 1" 0 \
    "$(json_agent 'Explore' '' '' "$M_GOOD
Audit every hook in the engine.")"
run_case "the hatch tolerates leading whitespace" 0 \
    "$(json_agent 'Explore' '' '' "Audit.
   $M_GOOD")"

# (m4) A BARE OR TRIVIAL MARKER EXEMPTS NOTHING.
run_case "bare 'generic-agent:' with no reason -> BLOCKED" 2 \
    "$(json_agent 'Explore' '' '' 'Audit.
generic-agent:')"
run_case "'generic-agent: because' -> BLOCKED" 2 \
    "$(json_agent 'Explore' '' '' 'Audit.
generic-agent: because')"
run_case "'generic-agent: n/a' -> BLOCKED" 2 \
    "$(json_agent 'Explore' '' '' 'Audit.
generic-agent: n/a')"
run_case "'generic-agent: -' -> BLOCKED" 2 \
    "$(json_agent 'Explore' '' '' 'Audit.
generic-agent: -')"
run_case_msg "a too-short reason is refused BY LENGTH, named" 'A bare or token marker exempts nothing' \
    "$(json_agent 'Explore' '' '' 'Audit.
generic-agent: because')"
# long enough in characters, but filler: five stopwords carrying nothing.
run_case "a long-but-empty reason (stopwords only) -> BLOCKED" 2 \
    "$(json_agent 'Explore' '' '' 'Audit.
generic-agent: this is the one that will just do it and that is all there is')"
run_case_msg "the filler refusal says so in those terms" 'reads as filler' \
    "$(json_agent 'Explore' '' '' 'Audit.
generic-agent: this is the one that will just do it and that is all there is')"

# (m5) SPEED/CONVENIENCE is refused BY NAME — it is the incident's own reason.
run_case "reason 'faster to dispatch' -> BLOCKED" 2 \
    "$(json_agent 'Explore' '' '' 'Audit.
generic-agent: a roster teammate would need a worktree created first and this is faster to dispatch')"
run_case "reason 'saves time' -> BLOCKED" 2 \
    "$(json_agent 'Explore' '' '' 'Audit.
generic-agent: dispatching a built-in here saves time versus provisioning a teammate worktree')"
run_case "reason 'more convenient' -> BLOCKED" 2 \
    "$(json_agent 'Explore' '' '' 'Audit.
generic-agent: it is simply more convenient than provisioning a roster teammate right now')"
run_case_msg "the speed refusal names create-teammate-worktree.sh as the answer" 'create-teammate-worktree.sh' \
    "$(json_agent 'Explore' '' '' 'Audit.
generic-agent: a roster teammate would need a worktree created first and this is faster to dispatch')"
# NO FALSE POSITIVE: a genuine reason that happens to mention worktrees passes.
run_case "a genuine reason mentioning worktrees is NOT read as a speed excuse" 0 \
    "$(json_agent 'Explore' '' '' 'Audit.
generic-agent: this enumerates worktree tooling across repositories the roster has no seat in')"

# (m6) HARNESS UTILITIES DO NOT CARRY THE HATCH — an over-broad guard that
# fires on a statusline change is how a defense becomes a formality.
run_case "statusline-setup needs NO hatch (harness utility)" 0 \
    "$(json_agent 'statusline-setup' '' '' 'Configure the statusline.')"
run_case "claude-code-guide needs NO hatch (harness utility)" 0 \
    "$(json_agent 'claude-code-guide' '' '' 'Explain how hooks work.')"

# (m7) GENERIC_AGENT_TYPES closes the obvious detour: general-purpose is NOT on
# the read-only allowlist, so before clause 5 it sailed through the whole
# contract on isolation + a truthful name alone.
run_case "general-purpose, isolated + well-named, NO hatch -> BLOCKED" 2 \
    "$(json_agent 'general-purpose' 'gen-sonnet-d1' 'worktree' 'Do the work.')"
run_case "general-purpose, isolated + well-named, WITH hatch -> allowed" 0 \
    "$(json_agent 'general-purpose' 'gen-sonnet-d2' 'worktree' "Do the work.
$M_GOOD")"
# ...and the hatch does NOT relax the rest of the contract for a file-capable type.
run_case "general-purpose WITH hatch but NO isolation -> still BLOCKED (clauses 1-2 hold)" 2 \
    "$(json_agent 'general-purpose' 'gen-sonnet-d3' '' "Do the work.
$M_GOOD")"

# (m8) ROSTER TEAMMATES ARE WHOLLY UNAFFECTED — the gate must not tax the
# normal path, or it will be routed around.
run_case "roster-style type, isolated, no hatch -> allowed (clause 5 inert)" 0 \
    "$(json_agent 'dev' 'dev-sonnet-m8a' 'worktree' 'Fix the bug and commit.')"
run_case "roster-style type, isolated, no hatch, second name -> allowed" 0 \
    "$(json_agent 'deviceqa' 'deviceqa-sonnet-m8b' 'worktree' 'Run device QA.')"
run_case "roster-style type WITHOUT isolation is still blocked by clause 1, not 5" 2 \
    "$(json_agent 'dev' 'dev-sonnet-m8c' '' 'Fix the bug.')"
run_case_msg "a roster-type refusal is the ISOLATION message, never the staffing one" 'missing native isolation' \
    "$(json_agent 'dev' 'dev-sonnet-m8d' '' 'Fix the bug.')"

# (m9) CONFIG IS THE SWITCH, AND ITS DEFAULT IS DENY. A type declared a harness
# utility in the entity's own config is exempt; a type merely added to
# READONLY_ALLOWLIST is NOT.
if wti_want "undeclared allowlist type waved through"; then
M9="$(mktemp -d -t guard-isolation-clause5.XXXXXX)"
mkdir -p "$M9/scripts/hooks" "$M9/scripts/lib" "$M9/.claude"
cp "$HOOK" "$M9/scripts/hooks/guard-worktree-isolation.sh"; chmod +x "$M9/scripts/hooks/guard-worktree-isolation.sh"
# workspaces.py is in this list because a read-only spawn is now REGISTERED
# before it is exempted (point 9's lock-out finds an agent through its
# registration), and a guard that cannot register refuses the spawn — point 3.
mkdir -p "$M9/mega-lander"
cp "$SCRIPT_DIR/../../mega-lander/workspaces.py" "$M9/mega-lander/"
cp "$SCRIPT_DIR/../lib/resolve-roots.sh" "$SCRIPT_DIR/../lib/resolve-model.sh" "$SCRIPT_DIR/../lib/resolve-main-checkout.sh" "$SCRIPT_DIR/../lib/worktree-ledger.py" "$SCRIPT_DIR/../lib/teammate-name.sh" "$M9/scripts/lib/" 2>/dev/null || true
{
    printf 'ALLOWED_MODELS="fable opus sonnet haiku"\n'
    printf 'READONLY_ALLOWLIST="Explore Plan claude-code-guide statusline-setup housekeeping"\n'
    printf 'HARNESS_UTILITY_TYPES="claude-code-guide statusline-setup housekeeping"\n'
} >"$M9/orchestration.config"
m9_run() { # <payload> -> echoes rc
    printf '%s' "$1" | RICHOS_ENTITY_ROOT="$M9" "$M9/scripts/hooks/guard-worktree-isolation.sh" >/dev/null 2>&1
    echo $?
}
rc="$(m9_run "$(json_agent 'housekeeping' '' '' 'Tidy the config.')")"
[ "$rc" -eq 0 ] && { PASS=$((PASS + 1)); printf '  PASS  a type DECLARED in HARNESS_UTILITY_TYPES needs no hatch\n'; } \
                || { FAIL=$((FAIL + 1)); printf '  FAIL  declared harness utility was blocked (exit %s)\n' "$rc"; }
{
    printf 'ALLOWED_MODELS="fable opus sonnet haiku"\n'
    printf 'READONLY_ALLOWLIST="Explore Plan claude-code-guide statusline-setup housekeeping"\n'
    printf 'HARNESS_UTILITY_TYPES="claude-code-guide statusline-setup"\n'
} >"$M9/orchestration.config"
rc="$(m9_run "$(json_agent 'housekeeping' '' '' 'Tidy the config.')")"
[ "$rc" -eq 2 ] && { PASS=$((PASS + 1)); printf '  PASS  a type merely ADDED to READONLY_ALLOWLIST still needs the hatch (default is deny)\n'; } \
                || { FAIL=$((FAIL + 1)); printf '  FAIL  undeclared allowlist type waved through (exit %s)\n' "$rc"; }
rm -rf "$M9"
fi # mutation witness fixture group

# (m10) THE USE IS LOGGED, so a habit of waiving is visible rather than invisible.
if wti_want "accepted generic-agent: hatch was NOT logged"; then
GA_LOG="$RICHOS_ENTITY_ROOT/.claude/state/generic-agent-dispatches.log"
GA_MARK="clause5-log-canary-$$"
printf '%s' "$(json_agent 'Explore' '' '' "Audit.
generic-agent: no roster teammate covers this cross-repository enumeration $GA_MARK")" \
    | "$HOOK" >/dev/null 2>&1
if grep -qF "$GA_MARK" "$GA_LOG" 2>/dev/null; then
    PASS=$((PASS + 1)); printf '  PASS  an accepted generic-agent: hatch is logged to .claude/state/generic-agent-dispatches.log\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  accepted generic-agent: hatch was NOT logged to %s\n' "$GA_LOG"
fi
# and a REFUSED dispatch is never logged (a refusal is not a waiver).
GA_MARK2="clause5-refused-canary-$$"
printf '%s' "$(json_agent 'Explore' '' '' "Audit.
generic-agent: nope $GA_MARK2")" | "$HOOK" >/dev/null 2>&1
if grep -qF "$GA_MARK2" "$GA_LOG" 2>/dev/null; then
    FAIL=$((FAIL + 1)); printf '  FAIL  a REFUSED generic-agent dispatch was logged as if it were a waiver\n'
else
    PASS=$((PASS + 1)); printf '  PASS  a REFUSED generic-agent dispatch is not logged (a refusal is not a waiver)\n'
fi
fi # mutation witness fixture group
fi # mutation witness fixture group

if wti_group; then
# ---------------------------------------------------------------------------
# (p5) POINT 5, amended 2026-09-30: pending integration allows unrelated
# development. Registration, workspace isolation and explicit land helpers remain.
# ---------------------------------------------------------------------------
# echo-opus-reg1 was spawned above (L07). The platform acknowledges it, it
# commits in its cc/ workspace, and its run ends: finished, not landed.
printf '{"hook_event_name":"PostToolUse","session_id":"%s","tool_use_id":"toolu_test_reg1","tool_name":"Agent","tool_input":{"name":"echo-opus-reg1"},"tool_response":{"agentId":"aechoreg10000001"}}' "$TEST_SID" \
    | python3 "$WS_PY" --entity "$RICHOS_ENTITY_ROOT" hook >/dev/null
printf 'work\n' >"$CR/other-wt/echo-opus-reg1/work.txt"
git -C "$CR/other-wt/echo-opus-reg1" add work.txt
git -C "$CR/other-wt/echo-opus-reg1" commit -q -m work
printf '{"hook_event_name":"SubagentStop","session_id":"%s","agent_id":"aechoreg10000001"}' "$TEST_SID" \
    | python3 "$WS_PY" --entity "$RICHOS_ENTITY_ROOT" hook >/dev/null
run_case "P501 unrelated work while echo-opus-reg1 stays pending -> allowed" 0 \
    "$(json_cwd 'dev-sonnet-p5new' 'worktree' '' 'Unrelated new work.' toolu_test_p5new)"
run_case "P502 a second unrelated spawn also passes registration" 0 \
    "$(json_cwd 'dev-sonnet-p5new2' 'worktree' '' 'More unrelated work.' toolu_test_p5new2)"
run_case "P503 the work that lands it (lands-pending: echo-opus-reg1) -> allowed" 0 \
    "$(json_cwd 'dev-sonnet-p5fix' 'worktree' '' $'Resolve the clash and land it.\nlands-pending: echo-opus-reg1' toolu_test_p5fix)"
python3 "$WS_PY" --session "$TEST_SID" discard echo-opus-reg1 --reason "a fixture of this suite, done with" \
    --not-ceo-ordered "a test fixture made by this suite" >/dev/null 2>&1
run_case "P504 discarded: new work is allowed again (positive control)" 0 \
    "$(json_agent 'dev' 'dev-sonnet-p5after' 'worktree' 'New work.')"
fi # mutation witness fixture group

[ -z "${CR:-}" ] || rm -rf "$CR"
kill "$RICHOS_SESSION_PID" 2>/dev/null || true
rm -rf "$TX_SANDBOX"

if [ -n "$WTI_SCOPE" ]; then
    for ((i=0; i<${#WTI_WANTS[@]}; i++)); do
        if [ "${WTI_SEEN[$i]:-0}" != 1 ]; then
            echo "ERROR: mutation witness not executed: ${WTI_WANTS[$i]}" >&2; exit 2
        fi
    done
fi

echo ""
if [ "$FAIL" -gt 0 ]; then
    echo "=== guard-worktree-isolation tests: $FAIL FAILED, $PASS passed ==="
    exit 1
fi
if [ -n "$WTI_SCOPE" ]; then
    echo "=== guard-worktree-isolation mutation witness $WTI_SCOPE: $PASS passed (not full-suite coverage) ==="
    exit 3
fi
echo "=== guard-worktree-isolation tests: all $PASS passed ==="

# This suite's mutation harness (guard-worktree-isolation.mutation.sh) is run
# by contract-integrity.test.sh; clause 7's registration rules themselves are
# mutated by mega-lander/tests/workspaces.mutation.sh.
exit 0
