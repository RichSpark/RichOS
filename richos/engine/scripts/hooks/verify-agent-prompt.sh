#!/usr/bin/env bash
#
# verify-agent-prompt.sh — PreToolUse gate on Agent spawns.
#
# Six always-on checks plus one OPT-IN check (the QA install-fresh gate,
# toggled by ENABLE_QA_INSTALL_FRESH_GATE in orchestration.config — OFF by
# default):
#
#   1. duplicate-teammate        — reject spawning a name that already exists
#                                  (not shutdown) in the session team. The
#                                  session team is derived from the hook
#                                  input's session_id when team_name is absent.
#   2. agent-not-found           — every in-repo `.claude/agents/<name>.md`
#                                  referenced in the prompt must exist. Skipped
#                                  for the creator meta-role (CREATOR_TEAMMATE,
#                                  whose job is to create the file);
#                                  out-of-repo absolute references are never
#                                  evaluated.
#   3. subagent-as-spawner       — reject prompts that ask a subagent to spawn
#                                  or dispatch agents; only the orchestrator
#                                  spawns.
#   4. missing-worktree-isolation— prompt claims native isolation created the
#                                  worktree but the call didn't set
#                                  isolation:"worktree".
#   5. qa-install-fresh (OPT-IN) — prompts that test/audit/render/capture the
#                                  local app must cite an install-fresh script
#                                  as a precondition, or carry an auditable
#                                  `data-contract-bypass:` line. Part of the
#                                  advanced "identity-or-refuse" tier; enable
#                                  only if you adopt a device/install-fresh
#                                  pipeline (see reference/advanced-tier/).
#                                  RE-SPECIFIED 2026-09-15: it now fires only
#                                  on a dispatch that can REACH this repo's app
#                                  and names an app RUNTIME (or is a QA role).
#                                  App-free work passes in silence and declares
#                                  nothing — 78 fires → 11 over 968 real
#                                  prompts, none of the 67 silenced ones app
#                                  work. The measurement and the reason it had
#                                  to move: the long comment at check 5.
#   6. ack-contract-missing      — a spawn that gets a worktree must carry the
#                                  in-flight ack contract in its PROMPT (the
#                                  helper name OR the ack path), or an auditable
#                                  `no-inflight-ack:` line. The instruction
#                                  cannot travel in the message it exists to
#                                  make verifiable.
#   7. concealment-clause        — reject a brief that orders anyone to reduce
#                                  WHAT THE CEO SEES (hide it from him, keep it
#                                  off his screen, make it invisible to him)
#                                  instead of removing the condition that makes
#                                  him see it. Auditable `conceal-ack:` line for
#                                  the rare true case; a bare marker exempts
#                                  nothing. Measured against 1,871 real spawn
#                                  prompts — see scripts/hooks/conceal.corpus.md.

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail
# Payloads can exceed a pipe buffer. Matching pipelines must consume EOF:
# grep -q or head can give the producer SIGPIPE and reverse the verdict.

# Fail-closed, not fail-open: every check below (duplicate-teammate,
# agent-not-found, subagent-as-spawner, missing-worktree-isolation, the
# opt-in qa-install-fresh gate) reads PROMPT/SUBAGENT_TYPE/ISOLATION/etc via
# `python3 ... || true`. If python3 is missing, those swallowed failures yield
# empty values, and `[ -n "$PROMPT" ]`-guarded checks below would then just
# skip — i.e. every content check in this gate would silently no-op. Refuse
# outright instead.
command -v python3 >/dev/null 2>&1 || { echo "ERROR: verify-agent-prompt.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

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
  hook: scripts/hooks/verify-agent-prompt.sh
  scripts/lib/resolve-roots.sh is missing at: $_RR_LIB
  Without it this guard cannot tell WHICH REPOSITORY it governs.
  It will not guess, and it will not carry on quietly — a defense
  that reports 'on' while protecting nothing is worse than none."
    printf '%s\n' "$_RR_MSG" >&2
    _RR_J="${_RR_MSG//\\/\\\\}"; _RR_J="${_RR_J//\"/\\\"}"; _RR_J="${_RR_J//$'\n'/\\n}"
    printf '{"systemMessage":"%s"}\n' "$_RR_J"
    exit 2
fi
# shellcheck source=../lib/resolve-roots.sh
. "$_RR_LIB"
ENGINE_ROOT="$(resolve_engine_root "$SCRIPT_DIR")"

INPUT="$(cat)"

# Test affordance, preserved: VERIFY_REPO_ROOT_OVERRIDE now feeds the
# contract's own declared-root candidate rather than assigning a root behind
# the resolver's back, so a sandbox root is validated like any other.
[ -n "${VERIFY_REPO_ROOT_OVERRIDE:-}" ] && RICHOS_ENTITY_ROOT="$VERIFY_REPO_ROOT_OVERRIDE"

# Resolve the governed repository. Three outcomes, three different behaviors —
# see the contract for why "block everything unresolvable" is NOT the rule.
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # This repository never adopted the engine, so there is no enforcement to
    # lose here. Stand down. NOT a silent skip: engine-status.sh announces the
    # stand-down into the orchestrator's own context at every session start.
    exit 0
else
    # BROKEN: this guard believes it is governing something and cannot. Block.
    root_failure_banner "scripts/hooks/verify-agent-prompt.sh" >&2
    exit 2
fi
# --- UNEVALUATED-PAYLOAD NOTICE --------------------------------------------
# On a payload it cannot read, this guard takes the SAME silent exit 0 that a
# well-formed payload for a DIFFERENT tool takes: the tool-name extraction ends
# in `|| true`, so "this call is not mine" and "I could not tell whose call this
# is" are one exit. That is why 17 of 25 PreToolUse guards were measured passing
# a call in complete silence on 2026-09-05. This separates the two. NO VERDICT
# CHANGES — the exit is the one already taken — only the silence does. The
# measurement, the channel and the argument: scripts/lib/unevaluated-notice.sh.
_UE_LIB="$SCRIPT_DIR/../lib/unevaluated-notice.sh"
if [ -f "$_UE_LIB" ]; then
    # shellcheck source=../lib/unevaluated-notice.sh
    . "$_UE_LIB"
    unevaluated_or_continue "verify-agent-prompt.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "every precondition this spawn prompt is required to carry, including the install-fresh citation the data contract depends on"
fi

CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
: "${CREATOR_TEAMMATE:=dean}"
: "${ENABLE_QA_INSTALL_FRESH_GATE:=0}"
: "${INSTALL_FRESH_SCRIPTS:=android-install-fresh.sh ios-install-fresh.sh}"
: "${QA_TRIGGER_RE:=(\btest\b|\baudit\b|\bverify\b|\bQA\b|\brender\b|\bscreenshot\b|install-fresh)}"
: "${LOCAL_APP_CONTEXT_RE:=(install-fresh|emulator|simulator|the app|local app|BUILD_SHA)}"
# APP_RUNTIME_RE / QA_ROLE_AGENTS — the two halves of the 2026-09-15 inversion
# (see check 5). RUNTIME is evidence the dispatch will EXERCISE a built app;
# QA_ROLE_AGENTS are the roles whose job IS the app, for whom naming an app
# surface at all is evidence enough.
: "${APP_RUNTIME_RE:=(\badb\b|simctl|xcrun|xcodebuild|\./gradlew|[Ee]mulator|[Ss]imulator|\.apk\b|\.ipa\b|BUILD_SHA|client-data-check|MainActivity|HomeView|HomeScreen|Health[[:space:]]*Connect|HealthKit|on[- ]device|physical device|installed[[:space:]]+app|home[ -]?state)}"
: "${QA_ROLE_AGENTS:=}"
# Test/per-invocation override for the opt-in gate toggle.
[ -n "${VERIFY_QA_GATE_OVERRIDE:-}" ] && ENABLE_QA_INSTALL_FRESH_GATE="$VERIFY_QA_GATE_OVERRIDE"

# (payload already read above, before root resolution)

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
[ "$TOOL_NAME" = "Agent" ] || exit 0

PROMPT="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_input",{}).get("prompt",""))' 2>/dev/null || true)"
SUBAGENT_TYPE="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_input",{}).get("subagent_type",""))' 2>/dev/null || true)"
ISOLATION="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_input",{}).get("isolation",""))' 2>/dev/null || true)"
AGENT_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_input",{}).get("name",""))' 2>/dev/null || true)"
TEAM_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_input",{}).get("team_name",""))' 2>/dev/null || true)"
SESSION_ID="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("session_id",""))' 2>/dev/null || true)"

FAIL=0
FAIL_REASONS=()

# ---------------------------------------------------------------------------
# 1. duplicate-teammate
# ---------------------------------------------------------------------------
# team_name is accepted-but-ignored by the harness (one implicit session
# team), so derive the team dir from session_id when it's absent:
# ~/.claude/teams/session-<first8-of-session-id>/config.json.
if [ -z "$TEAM_NAME" ] && [ -n "$SESSION_ID" ]; then
  TEAM_NAME="session-$(printf '%s' "$SESSION_ID" | cut -c1-8)"
fi

if [ -n "$TEAM_NAME" ] && [ -n "$AGENT_NAME" ]; then
  TEAM_CONFIG="$HOME/.claude/teams/$TEAM_NAME/config.json"
  # Override lets the test harness point at a sandbox teams/ directory.
  if [ -n "${V8_TEAMS_DIR_OVERRIDE:-}" ]; then
    TEAM_CONFIG="$V8_TEAMS_DIR_OVERRIDE/$TEAM_NAME/config.json"
  fi

  if [ -f "$TEAM_CONFIG" ]; then
    V8_RESULT="$(AGENT_NAME_ARG="$AGENT_NAME" python3 - "$TEAM_CONFIG" <<'PY'
import json
import os
import sys

name = os.environ.get("AGENT_NAME_ARG", "")
try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        data = json.load(f)
except Exception:
    print("NONE")
    raise SystemExit

for member in data.get("members", []) or []:
    if member.get("name") == name:
        status = (member.get("status") or "").lower()
        if status == "shutdown":
            print("SHUTDOWN")
        else:
            print("MATCH|%s|%s" % (member.get("agentId", ""), status or "present"))
        raise SystemExit
print("NONE")
PY
)"
    case "$V8_RESULT" in
      MATCH\|*)
        REST="${V8_RESULT#MATCH|}"
        DUP_AGENT_ID="${REST%%|*}"
        DUP_STATUS="${REST#*|}"
        FAIL=1
        FAIL_REASONS+=("duplicate-teammate: ${AGENT_NAME} already exists in ${TEAM_NAME} (agent_id: ${DUP_AGENT_ID}, status: ${DUP_STATUS}). Resume it with SendMessage, or explicitly shut it down and wait for the shutdown confirmation before spawning a replacement — under a NEW name, never a reused one.")
        ;;
    esac
  fi
fi

# ---------------------------------------------------------------------------
# 2–4. prompt-content checks
# ---------------------------------------------------------------------------
if [ -n "$PROMPT" ]; then
  # 2. agent-not-found
  #
  # Skip entirely for the creator meta-role: its job IS to create the
  # definition file a hiring brief references, so "read
  # .claude/agents/<new-role>.md" in its prompt is a to-be-created target, not
  # a broken reference.
  #
  # For everyone else, only evaluate references that resolve INSIDE this repo.
  # An absolute path like /some/other/repo/.claude/agents/foo.md is a
  # sibling-repo reference model — the trailing ".claude/agents/foo.md"
  # substring looks like an in-repo reference but isn't one, so we must not
  # stat it against $ENTITY_ROOT.
  # Compare on the BARE role: a namespaced `richos-engine:dean` is still dean,
  # and matching the raw string would have silently re-enabled the
  # agent-not-found check for the one role whose entire job is to create the
  # file the check would flag.
  if [ "$(strip_agent_namespace "$SUBAGENT_TYPE")" != "$CREATOR_TEAMMATE" ]; then
    CANDIDATE_REFS=()
    while IFS= read -r line; do
      [ -n "$line" ] && CANDIDATE_REFS+=("$line")
    done < <(
      printf '%s' "$PROMPT" \
        | grep -oE '[^[:space:]]*\.claude/agents/[a-z-]+\.md' \
        | sort -u || true
    )

    for full_ref in "${CANDIDATE_REFS[@]}"; do
      if [[ "$full_ref" == /* ]] && [[ "$full_ref" != "$ENTITY_ROOT"/* ]]; then
        continue  # absolute path outside this repo — not our reference to check
      fi
      agent="${full_ref##*.claude/agents/}"
      agent="${agent%.md}"
      # Search the entity roster first, then the engine's own — a prompt may
      # legitimately cite a plugin-supplied meta-role's definition, which does
      # not live in the entity's repository at all. Resolving only against the
      # entity root would have turned every such citation into a false
      # "agent-not-found" block the moment the engine became a plugin.
      if ! resolve_agent_def "$ENTITY_ROOT" "$ENGINE_ROOT" "$agent" >/dev/null 2>&1; then
        FAIL=1
        FAIL_REASONS+=("agent-not-found: .claude/agents/${agent}.md")
      fi
    done
  fi

  # 3. subagent-as-spawner
  #
  # Split into a case-insensitive keyword pass and a case-SENSITIVE TitleCase
  # pass, deliberately:
  #   - The "Agent tool" phrase is boundary-guarded: `(^|[^A-Za-z-])Agent`
  #     requires a word-start immediately before it, so a hyphen-prefixed
  #     compound like "non-Agent tool" no longer matches, while a genuine
  #     standalone reference ("Use the Agent tool...") still does.
  #   - The launch-verb alternative runs as a SEPARATE, case-sensitive grep so
  #     `[A-Z]` means what it says — only a genuinely capitalized token
  #     following the verb (a real teammate name) fires it.
  if [ -n "$SUBAGENT_TYPE" ]; then
    SPAWNER_CI_RE='(^|[^A-Za-z-])Agent[[:space:]]+tool|(spawn|dispatch)[[:space:]]+(agents?|teammates?|subagents?)\b|dispatch[[:space:]]+(these|the[[:space:]]+following)\b|TeamCreate'
    SPAWNER_CS_RE='\b(spawn|dispatch)[[:space:]]+[A-Z][a-z]+\b'
    if printf '%s' "$PROMPT" | grep -iE >/dev/null "$SPAWNER_CI_RE" \
       || printf '%s' "$PROMPT" | grep -E >/dev/null "$SPAWNER_CS_RE"; then
      FAIL=1
      FAIL_REASONS+=("subagent-as-spawner: prompt appears to ask subagent '${SUBAGENT_TYPE}' to spawn or dispatch agents. Only the orchestrator makes Agent tool calls — keep briefs to plain build instructions and don't narrate the orchestration around them.")
    fi
  fi

  # 4. missing-worktree-isolation: the prompt tells the teammate that native
  # isolation ALREADY created its worktree, but the Agent call didn't set
  # isolation:"worktree". That contradiction means the flag was forgotten and
  # the teammate will actually land in the MAIN checkout. Fail fast at the
  # spawn.
  if [ "$ISOLATION" != "worktree" ]; then
    if printf '%s' "$PROMPT" | grep -iE >/dev/null 'native isolation (has )?already|(isolation|worktree) (has )?already (given|created|placed|put)|already (given|placed|dropped) you (a|an|one|in)( native)?[[:space:]]*(claude code )?worktree'; then
      FAIL=1
      FAIL_REASONS+=("missing-worktree-isolation: the prompt tells the subagent that native isolation already created its worktree, but this Agent call set isolation='${ISOLATION:-unset}' (not \"worktree\"). Add isolation:\"worktree\" to the spawn. If you are deliberately hand-rolling a worktree, remove the 'native isolation already' wording and give the explicit worktree path instead.")
    fi
  fi
fi

# ---------------------------------------------------------------------------
# sanitized_prompt — the prompt with fenced code, HTML comments, blockquotes
# and indented code blocks stripped out.
#
# HOISTED out of check 5 on 2026-08-30, byte-identical, because check 6 needs
# exactly the same treatment for exactly the same reason: an opt-out marker a
# prompt merely QUOTES — from a file, an example, another agent's report — must
# never activate the opt-out. Two copies of this stripper would be the drift
# this engine keeps finding in itself. There is one, it is computed at most
# once per spawn, and both checks read the same answer.
PROMPT_SANITIZED=""
PROMPT_SANITIZED_DONE=0
sanitized_prompt() {
  if [ "$PROMPT_SANITIZED_DONE" -eq 1 ]; then
    printf '%s' "$PROMPT_SANITIZED"
    return 0
  fi
  PROMPT_SANITIZED_DONE=1
    BYPASS_STRIPPER_PY="$(mktemp -t verify-agent-prompt-strip.XXXXXX.py)"
    cat >"$BYPASS_STRIPPER_PY" <<'STRIP_PY'
import re, sys, unicodedata
text = sys.stdin.read()
# Strip HTML comments (multi-line, greedy across lines — not per-line).
text = re.sub(r'<!--[\s\S]*?-->', '', text)

_WS_CATEGORIES = ('Zs', 'Cf', 'Zl', 'Zp')
def _normalize_leading_ws(line):
    """Replace each leading Unicode-whitespace char with an ASCII space,
    leaving the rest of the line untouched. Stops at the first
    non-whitespace character."""
    i = 0
    n = len(line)
    out = []
    while i < n:
        ch = line[i]
        if ch == ' ' or ch == '\t':
            out.append(ch)
            i += 1
            continue
        cat = unicodedata.category(ch)
        if cat in _WS_CATEGORIES:
            out.append(' ')
            i += 1
            continue
        break
    out.append(line[i:])
    return ''.join(out)

fence_re = re.compile(r'^[ \t]{0,3}(`{3,}|~{3,})([^\n]*)$')
out_lines = []
in_fence = False
fence_marker = None   # '`' or '~'
fence_len = 0         # marker-char run length at opening
for raw_line in text.split('\n'):
    line = _normalize_leading_ws(raw_line)
    m = fence_re.match(line)
    if m:
        marker_run = m.group(1)
        marker_char = marker_run[0]
        info = (m.group(2) or '').strip()
        if not in_fence:
            in_fence = True
            fence_marker = marker_char
            fence_len = len(marker_run)
            continue
        # In fence: only close on same marker char, length >= opener,
        # AND no info string (markdown spec for code fences).
        if (marker_char == fence_marker and
            len(marker_run) >= fence_len and
            info == ''):
            in_fence = False
            fence_marker = None
            fence_len = 0
        # Either way, drop the marker line.
        continue
    if in_fence:
        continue
    stripped = line.lstrip()
    if stripped.startswith('>'):
        continue
    if line.startswith('    ') or line.startswith('\t'):
        continue
    out_lines.append(raw_line)
# If the prompt ended mid-fence, everything after the last open fence was
# dropped — safe direction (bypass lines inside unterminated fences remain
# stripped, not leaked).
print('\n'.join(out_lines))
STRIP_PY
    PROMPT_FOR_BYPASS="$(printf '%s' "$PROMPT" | python3 "$BYPASS_STRIPPER_PY" 2>/dev/null || printf '%s' "$PROMPT")"
    rm -f "$BYPASS_STRIPPER_PY"
  PROMPT_SANITIZED="$PROMPT_FOR_BYPASS"
  printf '%s' "$PROMPT_SANITIZED"
}

# ---------------------------------------------------------------------------
# 5. qa-install-fresh precondition (OPT-IN — advanced identity-or-refuse tier)
# ---------------------------------------------------------------------------
# Any spawn that tests / audits / renders / captures the LOCAL APP must either
# (a) cite an install-fresh script as a precondition or (b) opt out with an
# auditable `data-contract-bypass:` line. Runs ONLY when the gate is enabled.
#
# --- THE 2026-09-15 INVERSION, AND THE MEASUREMENT THAT ORDERED IT ----------
#
# This gate used to fire on (a loose QA verb) AND (a loose platform noun), and
# that put it on the WRONG SIDE of its own predicate: the common case — a
# dispatch that never goes near the app — was the one made to pay, in a typed
# paragraph of justification, every single time.
#
# Replayed over every Agent spawn prompt recoverable from this machine's
# session transcripts (968 prompts, 51 transcripts, 2026-08-01 → 2026-09-15;
# scripts/hooks/data-contract.corpus.md has the method and the commands):
#
#     the gate fired on                       78 prompts
#     of those, cited an install-fresh script  0
#     of those, carried a bypass line         69   (88.5% of fires)
#     of those, were REFUSED outright          9   (each retried with a bypass)
#
# Not one of the 78 was a dispatch that tests or renders the app. The
# satisfaction path — cite the script — was taken ZERO times, because the
# prompts that tripped the gate were never app work in the first place. A gate
# whose escape hatch is its only exit is charging a toll, and this project's
# own record says habitual waiving is how a guard dies (g11/g12/g13).
#
# Worse, it tripped on ITSELF: `install-fresh` sits in BOTH regexes but is not
# what INSTALL_FRESH_SCRIPTS accepts, so merely DISCUSSING the contract was an
# automatic refusal — 29 of the 78 fires matched their app-context on that one
# word. The brief that ordered this fix was refused by this gate for quoting
# the gate, and the only way through was the hatch taken 486 times.
#
# So the default moves to the other side. Silence is now the default, and the
# case that must produce evidence is the one that touches the app:
#
#   REACHABILITY  the dispatch must be able to reach this repository's app at
#                 all. A `cross-repo-worktree:` line naming a workspace in a
#                 DIFFERENT repository is structural proof it cannot — that is
#                 a fact about the filesystem, not a claim in prose. 38% of
#                 dispatches measured (370/968) are exactly this. Undecidable
#                 (no git, path gone) is treated as reachable: fail closed.
#   EVIDENCE      the prompt must name an app RUNTIME (APP_RUNTIME_RE: adb,
#                 simctl, the emulator, the Simulator, an .apk, BUILD_SHA, a
#                 screen class) — naming a platform is not exercising an app —
#                 OR be a dispatch to a role whose job IS the app
#                 (QA_ROLE_AGENTS), for whom any app surface is evidence.
#
# The predicate is a STRICT NARROWING of the old one (both original conjuncts
# are still required), so it cannot fire where the old gate was silent: 0 new
# fires across the 968. Fires drop 78 → 11, i.e. −85.9%, and every one of the
# 67 silenced fires is a prompt that was bypassed or refused-then-bypassed in
# life. The protection is unchanged for real app work, and the test suite
# mutates each clause to prove that a dispatch which DOES render the app is
# still refused without its citation.
if [ "$ENABLE_QA_INSTALL_FRESH_GATE" = "1" ] && [ -n "$SUBAGENT_TYPE" ] && [ -n "$PROMPT" ]; then
  # Cheap regex conjuncts first; the reachability probe shells out to git and
  # only runs on a prompt that has already matched both.
  QA_GATE_APPLIES=0
  if printf '%s' "$PROMPT" | grep -iE >/dev/null "$QA_TRIGGER_RE" \
     && printf '%s' "$PROMPT" | grep -E >/dev/null "$LOCAL_APP_CONTEXT_RE"; then
    # EVIDENCE: an app runtime named, or a role whose job is the app.
    QA_GATE_EVIDENCE=0
    if printf '%s' "$PROMPT" | grep -E >/dev/null "$APP_RUNTIME_RE"; then
      QA_GATE_EVIDENCE=1
    fi
    if [ "$QA_GATE_EVIDENCE" -eq 0 ] && [ -n "$QA_ROLE_AGENTS" ]; then
      for _role in $QA_ROLE_AGENTS; do
        if [ "$_role" = "$SUBAGENT_TYPE" ]; then QA_GATE_EVIDENCE=1; break; fi
      done
    fi
    # REACHABILITY: a workspace in another repository cannot touch this app.
    QA_GATE_REACHABLE=1
    if [ "$QA_GATE_EVIDENCE" -eq 1 ]; then
      XREPO_PATH="$(printf '%s' "$(sanitized_prompt)" \
        | grep -oE '^[[:space:]]*cross-repo-worktree:[[:space:]]*[^[:space:]]+' \
        | sed -n '1p' | sed -E 's/.*cross-repo-worktree:[[:space:]]*//')" || XREPO_PATH=""
      if [ -n "$XREPO_PATH" ]; then
        if [ -d "$XREPO_PATH" ] && command -v git >/dev/null 2>&1; then
          XREPO_COMMON="$(git -C "$XREPO_PATH" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
          ENTITY_COMMON="$(git -C "$ENTITY_ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
          if [ -n "$XREPO_COMMON" ] && [ -n "$ENTITY_COMMON" ] && [ "$XREPO_COMMON" != "$ENTITY_COMMON" ]; then
            QA_GATE_REACHABLE=0
          fi
        else
          # THE WORKSPACE USUALLY DOES NOT EXIST YET WHEN THIS RUNS, and a clause
          # that needed it to exist would be inert exactly where it matters:
          # spawn.sh evaluates every PreToolUse[Agent] guard BEFORE it creates
          # anything ("creates the isolated workspace only once nothing can
          # refuse it"), so at the deciding evaluation the path is a promise,
          # not a directory. Measured: requiring -d leaves 33 of the 78 historical
          # fires standing; deciding the absent path leaves 11.
          #
          # An absolute path OUTSIDE this repository is another repository's
          # workspace — a workspace for THIS repository is either already there
          # (branch above) or lives under this root. Anything else (relative,
          # empty, malformed) stays reachable: fail closed.
          case "$XREPO_PATH" in
            "$ENTITY_ROOT"/*) : ;;
            /*) QA_GATE_REACHABLE=0 ;;
            *) : ;;
          esac
        fi
      fi
    fi
    if [ "$QA_GATE_EVIDENCE" -eq 1 ] && [ "$QA_GATE_REACHABLE" -eq 1 ]; then
      QA_GATE_APPLIES=1
    fi
  fi

  if [ "$QA_GATE_APPLIES" -eq 1 ]; then

    # Bypass detection runs against a SANITIZED copy of the prompt: fenced
    # code, HTML comments, blockquotes, and indented code blocks are stripped
    # first, so a forged bypass inside any of those never activates the opt-out.
    BYPASS_LINE=""
    PROMPT_FOR_BYPASS="$(sanitized_prompt)"
    if printf '%s' "$PROMPT_FOR_BYPASS" | grep -E >/dev/null '^[[:space:]]*data-contract-bypass:[[:space:]]*.+'; then
      BYPASS_LINE="$(printf '%s' "$PROMPT_FOR_BYPASS" | grep -oE '^[[:space:]]*data-contract-bypass:[[:space:]]*.+' | sed -n '1p' | sed -E 's/^[[:space:]]*//')"
    fi

    if [ -n "$BYPASS_LINE" ]; then
      # Audit log under .claude/state/. Best-effort — never fail the spawn
      # because logging failed; the bypass itself is the audit.
      BYPASS_LOG_DIR="$ENTITY_ROOT/.claude/state"
      mkdir -p "$BYPASS_LOG_DIR" 2>/dev/null || true
      {
        printf '%s\tagent=%s\t%s\n' \
          "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
          "$SUBAGENT_TYPE" \
          "$BYPASS_LINE"
      } >>"$BYPASS_LOG_DIR/data-contract-bypasses.log" 2>/dev/null || true
      # Allow the spawn.
    else
      CITES_INSTALL_FRESH=0
      for s in $INSTALL_FRESH_SCRIPTS; do
        if printf '%s' "$PROMPT" | grep -F >/dev/null "$s"; then
          CITES_INSTALL_FRESH=1
          break
        fi
      done
      if [ "$CITES_INSTALL_FRESH" -eq 0 ]; then
        FAIL=1
        FAIL_REASONS+=("qa-install-fresh-precondition-missing: prompt assigns a task to agent \`${SUBAGENT_TYPE}\` that tests / audits / renders / captures the local app but does NOT cite an install-fresh script (${INSTALL_FRESH_SCRIPTS}) as a precondition. Every testing operation against the local app MUST start from a fresh install that passes the data-render contract — otherwise the result is meaningless. Either add 'Precondition: <install-fresh script> <sha> exits 0.' to the prompt, or — if the task is genuinely app-free — add a live-prose line starting with 'data-contract-bypass:' and the reason (logged to .claude/state/data-contract-bypasses.log).")
      fi
    fi
  fi
fi

# ---------------------------------------------------------------------------
# 6. ack-contract-missing — a file-writing spawn must be TOLD how to acknowledge
# ---------------------------------------------------------------------------
# You cannot bootstrap reliability from an unreliable channel. If the
# instruction "here is how you acknowledge" travels in the follow-up message,
# it is lost with the message it rode in on — and the lead is then left waiting
# for an ack the teammate was never told to write. That is not hypothetical: on
# 2026-08-30 the lead sent two receipt checks to a teammate and sat waiting for
# replies that never came, having never told it how to reply durably.
#
# So the ack contract belongs in the SPAWN PROMPT, which is one of the four
# substrates the doctrine calls durable, and this is the chokepoint where every
# spawn prompt passes exactly once.
#
# WHO IT APPLIES TO: spawns that get a worktree — native isolation, or a
# hand-rolled worktree named in the prompt. Those are the teammates who can be
# left behind by a land, because they are the ones holding a snapshot. A
# read-only or synchronous subagent holds nothing and is owed nothing.
#
# HOW TO SATISFY IT: name either the helper (`inflight-ack.sh`) or the ack path
# (`inflight-acks/`) in the prompt. One line:
#
#   If I message you saying main moved, acknowledge it durably — I cannot rely
#   on a reply reaching me. Run: scripts/inflight-ack.sh --sha <sha> --impact
#   <conflict|stale-record|grew-scope|none> --detail "<your own words>"
#   --paths "<paths or none>"
#
# BOTH FORMS ARE ACCEPTED because the FORMAT is the contract, not the script.
# The engine is loaded by reference, so `scripts/inflight-ack.sh` is not a path
# that exists inside the governed repository — a teammate reaches the helper at
# ~/.claude/richos-engine/scripts/inflight-ack.sh, and only if the operator has
# installed it. A prompt that spells out the ack FILE instead
# (<worktree>/.claude/inflight-acks/<sha12>.<teammate>.ack and its keys) has satisfied
# the requirement completely, and a check that insisted on the script name would
# be refusing the more robust of the two.
#
# HOW TO OPT OUT: a live prompt line starting  no-inflight-ack: <reason>
# Auditable, visible in the prompt itself, and never silent — the same shape as
# main-checkout-run: and data-contract-bypass:.
if [ -n "$SUBAGENT_TYPE" ] && [ -n "$PROMPT" ]; then
  ACK_APPLIES=0
  [ "$ISOLATION" = "worktree" ] && ACK_APPLIES=1
  if [ "$ACK_APPLIES" -eq 0 ] && printf '%s' "$PROMPT" | grep -iE >/dev/null 'worktree'; then
    ACK_APPLIES=1
  fi
  if [ "$ACK_APPLIES" -eq 1 ]; then
    # The merge names the branch the work lands on: main, or a release branch
    # such as v1.3.0 for a teammate bound to one (prepare-agent-spawn.py).
    if ! printf '%s' "$PROMPT" | grep -E >/dev/null 'inflight-ack\.sh|inflight-acks/|git merge main|`git merge [A-Za-z0-9][A-Za-z0-9._/-]*`' \
       && ! printf '%s' "$(sanitized_prompt)" | grep -iE >/dev/null '^[[:space:]]*no-inflight-ack:[[:space:]]*[^[:space:]]'; then
      FAIL=1
      FAIL_REASONS+=("ack-contract-missing: this spawn gets a worktree (isolation='${ISOLATION:-unset}'), so main can move under it. The prompt must tell it to run 'git merge main' (or the release branch its work lands on) in its own worktree as its last step before hand over (scripts/prepare-agent-spawn.py adds that line), or carry the older ack contract (scripts/inflight-ack.sh). If this teammate genuinely writes nothing and reads nothing that can go stale, opt out on the record with a live prompt line: 'no-inflight-ack: <reason>'.")
    fi
  fi
fi

# ---------------------------------------------------------------------------
# 7. concealment-clause — a brief may not order anyone to reduce WHAT THE CEO
#    SEES in place of fixing what makes him see it
# ---------------------------------------------------------------------------
# WHY THIS EXISTS, and it is one sentence of history rather than a principle.
# On 2026-09-10 a brief left this orchestrator carrying the instruction
#
#     Make the surviving demand invisible to him.
#
# A guard was firing a warning at the CEO. The warning was TRUE. The brief did
# not ask anyone to remove the condition that made it fire; it asked for the
# warning to be carried on a channel he could not read. He found it himself —
# he reads the tool calls — and said: "That is hiding, not fixing." He then
# asked the only question that matters about a promise: how does he know it
# will never happen again.
#
# A note-to-self is not an answer to that question. This check is.
#
# WHAT IT REFUSES: a suppression verb whose GRAMMATICAL TARGET is the CEO or a
# surface he reads. Not a suppression verb. Not the word "hide". The pairing,
# in one sentence, with the CEO on the receiving end of a directional
# preposition:  hide X FROM HIM, keep X OFF HIS SCREEN, make X INVISIBLE TO
# THE CEO, stop HIM FROM SEEING X, so HE NEVER SEES X, strip X FROM every
# USER-VISIBLE field, route X AWAY FROM HIS SCREEN.
#
# ===========================================================================
# THE PRECISION/RECALL TRADE, CHOSEN AGAINST A MEASURED CORPUS
# ===========================================================================
# THIS PROJECT HAS KILLED THREE BLOCKING GUARDS BY BUILDING THEM TOO BROAD
# (g11/g12/g13, all in one day). The fix on the day is always to waive, and
# habitual waiving is how a guard dies. So the shape below was not reasoned
# out and shipped — it was measured against every Agent spawn prompt in every
# Claude Code transcript on this machine, 1,871 unique real briefs, and the
# first two drafts were thrown away because the corpus said they were wrong.
#
#   draft 1  verb and CEO anywhere in one sentence   16 hits, 15 of them false
#   draft 3  verb GOVERNING the CEO (this one)        1 hit, and it is the
#                                                      2026-09-10 brief itself
#
# The whole measurement, every cut shape, and the command to reproduce it:
# scripts/hooks/conceal.corpus.md.
#
# CHOSEN DELIBERATELY IN FAVOR OF PRECISION — what this does NOT catch:
#   * Anything inside a code fence, an inline code span, a blockquote, an HTML
#     comment, or a QUOTED span. Every brief that describes this incident
#     quotes it, and a guard that refuses briefs about itself is a guard that
#     cannot be worked on. Cost: an order written entirely inside quotation
#     marks is missed. Nobody writes an order that way; somebody evading this
#     check would. It guards honest drift, not an adversary.
#   * A sentence carrying a PROHIBITION or a contrast ("do not hide it from
#     him", "concealing it from him INSTEAD OF fixing it", "nothing here may
#     keep it off his screen"). Matching a prohibition as an order is the
#     exact defect that broke the sibling brief gate hours after it landed
#     (femcboost 34d23cf6d). Bare "never" is deliberately NOT a prohibition
#     marker, because "so he never sees it" is the canonical concealment
#     phrasing — only "never again"/"never happen" are.
#   * Indirection with no directional object: "carry the repair instruction on
#     a channel that reaches the MODEL and not the user", "put the full
#     instruction where only the model reads it". Both are from the original
#     brief and both are missed. Catching them needs intent, not grammar, and
#     every draft that tried cost more false positives than it bought.
#   * Suppression aimed at ANY OTHER audience — an app user, a coach, an
#     athlete, a log, a test. That is ordinary product work, it is frequent,
#     and a gate that ate it would be waived within a week. 18 such briefs are
#     pinned as must-allow cases in verify-agent-prompt.test.sh.
#
# THE ESCAPE HATCH IS REAL, because unlike the interactive-prompt guard this
# one CAN be right to refuse and wrong to insist: a live credential, PII, or a
# secret genuinely should not reach a screen. `conceal-ack: <reason>` on a live
# prompt line, logged to .claude/state/conceal-acks.log. A bare marker exempts
# nothing — the reason must say something.
if [ -n "$PROMPT" ]; then
  CONCEAL_PY="$(mktemp -t verify-agent-prompt-conceal.XXXXXX.py)"
  cat >"$CONCEAL_PY" <<'CONCEAL_PY_EOF'
import re, sys

text = sys.stdin.read()

# Inline code spans and quoted spans are somebody else's words being shown,
# not this brief's instruction. Single-line only: a quote character that never
# closes must not swallow the rest of the prompt.
text = re.sub(r'`[^`\n]+`', ' ', text)
text = re.sub(r'"[^"\n]{1,200}"', ' ', text)
text = re.sub(u'“[^”\n]{1,200}”', ' ', text)
text = re.sub(r"(?<![A-Za-z0-9])'[^'\n]{1,200}'(?![A-Za-z0-9])", ' ', text)
text = re.sub(u"‘[^’\n]{1,200}’", ' ', text)

# A sentence that FORBIDS concealment, or contrasts it with the real fix, is
# not an order to conceal. Bare "never" is absent on purpose (see the header).
PROHIB = re.compile(
    r"\b(?:do not|do n.t|don.t|never again|never happen|must not|may not"
    r"|cannot|can.t|shall not|no need to|instead of|rather than|without"
    r"|nothing|forbidden|prohibited|withdrawn|refus(?:e|es|ed|ing)|not to)\b", re.I)
# "refus(e|es|ed|ing)" is the VERB, a prohibition ("refuses to hide it"). The
# NOUN is not: "Suppress the guard refusal so the CEO never sees it" is an
# order to conceal, and used to survive only because an all-prohibition prompt
# fell back to being judged unscrubbed (which also condemned "Do not hide
# warnings from him" — hunt 2026-09-29 part 3, finding 5).

# The CEO as the OBJECT of a preposition, and only there. "the CEO" must end
# its noun phrase: "from the CEO." matches, "from the CEO handoff" does not,
# "from the CEO-TODOs" and "from the CEO's briefing" do not.
_END = (r"(?=\s*(?:[.,;:!?)—-]|$|\b(?:and|or|so|to|as|if|by|at|in|on|for|with|from|per"
        r"|that|this|until|unless|while|because|when|before|after|during|is|was|would"
        r"|will|can|has|have|had|does|did|only|ever|right)\b))")
T = (r"(?:him(?![\w'’])%s"
     r"|his\s+(?:screen|terminal|view|sight|eyes|report|inbox|window|transcript)\b"
     r"|the\s+CEO'?s\s+(?:screen|terminal|view|sight|eyes|report|inbox)\b"
     r"|the\s+CEO(?![\w'’-])%s)") % (_END, _END)
T_MID = r"(?:him|the\s+CEO)"
V = (r"(?:hid(?:e|es|ing|den)|conceal(?:s|ing|ed)?|suppress(?:es|ing|ed)?"
     r"|mut(?:e|es|ing|ed)|silenc(?:e|es|ing|ed)|obscur(?:e|es|ing|ed)"
     r"|bur(?:y|ies|ying|ied)|mask(?:s|ing|ed)?|withhold(?:s|ing)?"
     r"|redact(?:s|ing|ed)?|downgrad(?:e|es|ing|ed)|strip(?:s|ping|ped)?"
     r"|shorten(?:s|ing|ed)?|re-?rout(?:e|es|ing|ed)|drop(?:s|ping|ped)?"
     r"|remov(?:e|es|ing|ed))")

CONSTRUCTIONS = [
 ("verb-from-him",
  r"\b%s\b[^.\n]{0,60}?\b(?:from|off|out of|away from)\s+%s" % (V, T)),
 ("make-it-invisible-to-him",
  r"\bmak\w*\b[^.\n]{0,60}?\b(?:invisible|less visible|not visible|unreadable|imperceptible)\b"
  r"[^.\n]{0,25}?\bto\s+%s" % T),
 ("keep-it-off-his-screen",
  r"\b(?:keep|keeps|keeping|kept|stay|stays|staying|remain|remains|remaining|sit|sits|leave|leaves)\b"
  r"[^.\n]{0,60}?\b(?:off|out of|away from)\s+(?:his|the\s+CEO'?s)\s*"
  r"(?:screen|terminal|view|sight|report|eyes|face|inbox)"),
 ("stop-him-from-seeing",
  r"\b(?:stop|stops|stopping|prevent|prevents|preventing|block|blocks|blocking"
  r"|keep|keeps|keeping)\s+%s\s+from\s+(?:seeing|reading|noticing|hearing|learning)" % T_MID),
 ("so-he-never-sees-it",
  r"\bso\s+(?:he|the\s+CEO)\s+(?:never|does not|doesn.t|will not|won.t|cannot|can.t)"
  r"\s+(?:see|read|notice|hear|find)"),
 ("strip-from-user-visible",
  r"\b(?:strip|remov|drop|shorten|downgrad|suppress|hid|conceal|mut|silenc)\w*\b"
  r"[^.\n]{0,60}?\b(?:from|out of)\s+(?:every |the |all |any )?user-visible\b"),
 ("divert-away-from-him",
  r"\b(?:rout|re-rout|divert|redirect|channel|funnel)\w*\b[^.\n]{0,60}?\baway from\s+%s" % T),
 ("so-it-never-reaches-him",
  r"\bso\b[^.\n]{0,40}?\b(?:does not|doesn.t|never|will not|won.t|cannot|can.t)\s+"
  r"(?:reach\w*|surfaces? to|gets? to|makes? it to|appears? to|shows? up for|be seen by)\s+%s" % T),
]

kept = [s for s in re.split(r'(?<=[.\n])', text) if not PROHIB.search(s)]
# The scrubbed text is ALWAYS what is judged. A prompt made only of
# prohibitions scrubs to nothing and so matches nothing; falling back to the
# original text there turned "Do not hide warnings from him." into the very
# order it forbids (hunt 2026-09-29 part 3, finding 5).
text = ''.join(kept)

for name, pattern in CONSTRUCTIONS:
    m = re.search(pattern, text, re.I)
    if m:
        sys.stdout.write("%s\t%s\n" % (name, re.sub(r'\s+', ' ', m.group(0)).strip()))
CONCEAL_PY_EOF

  # FAIL-CLOSED, exactly as the python3-missing check at the top of this file
  # is. A judgment that cannot run must not wave the spawn through: the whole
  # class of defect this engine keeps finding in itself is a guard that reports
  # "on" while deciding nothing. `set -e` is on, so the rc is captured rather
  # than allowed to abort the hook.
  CONCEAL_HITS=""
  set +e
  CONCEAL_HITS="$(printf '%s' "$(sanitized_prompt)" | python3 "$CONCEAL_PY" 2>/dev/null)"
  CONCEAL_RC=$?
  set -e
  rm -f "$CONCEAL_PY"

  if [ "$CONCEAL_RC" -ne 0 ]; then
    FAIL=1
    FAIL_REASONS+=("concealment-clause-unevaluable: the concealment matcher failed to run (exit ${CONCEAL_RC}), so this brief was NOT checked for an instruction to reduce what the CEO sees. Refusing rather than passing it through unjudged — a guard that cannot judge must not wave things past. Fix scripts/hooks/verify-agent-prompt.sh and re-run.")
  elif [ -n "$CONCEAL_HITS" ]; then
    CONCEAL_NAME="$(printf '%s' "$CONCEAL_HITS" | sed -n '1p' | cut -f1)"
    CONCEAL_PHRASE="$(printf '%s' "$CONCEAL_HITS" | sed -n '1p' | cut -f2-)"

    # The opt-out, read off the SANITIZED prompt so a marker this brief merely
    # quotes can never activate it, and REQUIRED TO SAY SOMETHING. A bare
    # marker, or a reason made of filler, exempts nothing.
    CONCEAL_ACK_LINE=""
    if printf '%s' "$(sanitized_prompt)" | grep -E >/dev/null '^[[:space:]]*conceal-ack:[[:space:]]*.+'; then
      CONCEAL_ACK_LINE="$(printf '%s' "$(sanitized_prompt)" | grep -oE '^[[:space:]]*conceal-ack:[[:space:]]*.+' | sed -n '1p' | sed -E 's/^[[:space:]]*//')"
    fi
    CONCEAL_ACK_REASON="${CONCEAL_ACK_LINE#conceal-ack:}"
    CONCEAL_ACK_REASON="$(printf '%s' "$CONCEAL_ACK_REASON" | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//')"
    CONCEAL_ACK_OK=0
    if [ -n "$CONCEAL_ACK_REASON" ]; then
      # Substantive: at least three words AND twelve characters. "yes", "ok",
      # "n/a" and "it is" all fail it; a sentence that names what is being
      # withheld and why cannot.
      #
      # A BLACKLIST OF FILLER PHRASES WAS WRITTEN HERE AND THEN DELETED,
      # because the mutation harness proved it decided nothing: every filler it
      # listed ("yes", "see above", "as discussed") was already refused by the
      # word count, so removing the blacklist turned no case red. Dead code in a
      # guard is worse than absent code — it reads like a second line of defense
      # and is not one. The bar is the word count, and the word count is
      # provable (conceal.mutation.sh / filler-ack-exempts).
      CONCEAL_ACK_WORDS="$(printf '%s' "$CONCEAL_ACK_REASON" | wc -w | tr -d '[:space:]')"
      CONCEAL_ACK_CHARS="${#CONCEAL_ACK_REASON}"
      if [ "$CONCEAL_ACK_WORDS" -ge 3 ] && [ "$CONCEAL_ACK_CHARS" -ge 12 ]; then
        CONCEAL_ACK_OK=1
      fi
    fi

    if [ "$CONCEAL_ACK_OK" -eq 1 ]; then
      CONCEAL_LOG_DIR="$ENTITY_ROOT/.claude/state"
      mkdir -p "$CONCEAL_LOG_DIR" 2>/dev/null || true
      {
        printf '%s\tagent=%s\tmatched=%s\tphrase=%s\t%s\n' \
          "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
          "${SUBAGENT_TYPE:-unknown}" \
          "$CONCEAL_NAME" \
          "$CONCEAL_PHRASE" \
          "$CONCEAL_ACK_LINE"
      } >>"$CONCEAL_LOG_DIR/conceal-acks.log" 2>/dev/null || true
      # Allow the spawn — on the record.
    else
      FAIL=1
      FAIL_REASONS+=("concealment-clause: this brief instructs someone to reduce WHAT THE CEO SEES rather than to fix what makes him see it. Matched (${CONCEAL_NAME}): \"${CONCEAL_PHRASE}\". His words, 2026-09-10, on reading a brief that said to make a surviving warning invisible to him: THAT IS HIDING, NOT FIXING. THE CORRECT FIX IS THE OPPOSITE ONE — remove the CONDITION that makes the warning fire, and leave the warning visible. A warning that is still true and no longer shown is the same defect with the evidence deleted, and the next person to look will believe it was fixed. If this is one of the rare true cases — a live credential, PII, an actual secret that should not reach any screen — say so on the record with a live prompt line: 'conceal-ack: <reason>' (logged to .claude/state/conceal-acks.log). A bare marker exempts nothing; the reason has to say what is being withheld and why.")
    fi
  fi
fi

# ---------------------------------------------------------------------------
# Emit result
# ---------------------------------------------------------------------------
if [ "$FAIL" -eq 1 ]; then
  echo "=== Agent prompt verification FAILED ===" >&2
  for reason in "${FAIL_REASONS[@]}"; do
    echo "  - $reason" >&2
  done
  # WHAT TO RUN NEXT. This is read at exactly the moment the one-command path
  # is worth most — a brief has just been refused, and spawn.sh would have
  # reported THIS failure together with every other one before anything was
  # created. Until 2026-09-13 this named only the four-step path, which is how
  # spawn.sh went unused the night after it landed. Naming it here changes
  # nothing about what this guard accepts: the old path is still allowed, and
  # is still named, second.
  echo "  For a NEW isolated spawn, start the teammate with ONE command:" >&2
  echo "    $ENGINE_ROOT/scripts/spawn.sh <teammate-name> --repo <repo> --type <subagent-type> --brief <file> [--model <alias>]" >&2
  echo "  It creates and registers the workspace, assembles the payload with native isolation and the acknowledgement contract, and evaluates EVERY PreToolUse[Agent] guard from EVERY surface against it BEFORE anything is created — reporting ALL failures together, including the one above. On refusal it exits 1 and leaves nothing behind." >&2
  echo "  FALLBACK ONLY (still accepted, four steps, guards evaluated only at the tool call):" >&2
  echo "    python3 \"$ENGINE_ROOT/scripts/prepare-agent-spawn.py\" --file <input.json>" >&2
  echo "  It supplies both native isolation and the acknowledgement contract. Neither grants any exemption, and neither is a resume helper." >&2
  echo "(hook: scripts/hooks/verify-agent-prompt.sh)" >&2
  exit 2
fi

exit 0
