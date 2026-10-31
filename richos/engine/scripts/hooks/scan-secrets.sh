#!/usr/bin/env bash
#
# scan-secrets.sh — PreToolUse guard (Write|Edit|MultiEdit|NotebookEdit).
#
# Closes the "will these autonomous agents leak my credentials into a commit?"
# objection — a top-three security question from any CEO evaluating an
# autonomous-agent team. Scans the CONTENT a Write/Edit/MultiEdit/NotebookEdit
# tool call is about to introduce for secret-shaped strings, and BLOCKS the
# write before it ever reaches disk — structurally, not by promise. This is
# the same "catch it at the moment of the write" posture as
# guard-main-checkout-writes.sh; it is wired as a SECOND hook under that same
# PreToolUse[Write|Edit|MultiEdit|NotebookEdit] matcher (see
# .claude/settings.local.json), not a separate git pre-commit hook — the engine
# has zero git-hook infrastructure today, and this slots into the existing
# Claude-Code-hook framework with no new installation mechanism, per the
# roadmap's own "no new infrastructure" constraint.
#
# WHAT IT CATCHES (two independent detector classes):
#
#   1. Vendor-prefix patterns — high-confidence because the prefix already
#      narrows the false-positive space enormously: AWS access keys
#      (AKIA/ASIA...), GitHub tokens (ghp_/gho_/ghu_/ghs_/ghr_/github_pat_...),
#      OpenAI keys (sk-.../sk-proj-...), Anthropic keys (sk-ant-...), Stripe
#      LIVE keys (sk_live_/pk_live_/rk_live_ — test-mode sk_test_/pk_test_
#      keys are deliberately NOT flagged, matching common practice for
#      non-production fixtures), and PEM private-key blocks
#      (-----BEGIN ... PRIVATE KEY-----).
#   2. Generic key=value literals — password=/passwd=/pwd=/api_key=/secret=/
#      token=/access_key=/private_key=, gated by Shannon entropy of the value
#      (real secrets are pseudo-random; placeholders are not) so a fixture
#      like `api_key = "re_xxxxxxxxx"` does NOT false-positive: a run of a
#      single repeated character has ~0 bits/char of entropy, far below the
#      real-secret threshold, so no allowlist entry is even required for that
#      specific shape — the entropy gate handles it structurally. Additional,
#      explicit exemptions: known placeholder words (placeholder, changeme,
#      example, dummy, redacted, ...), environment-variable-reference syntax
#      (${VAR}, process.env.X, os.environ[...], os.getenv(...) — the CORRECT
#      way to handle a real secret, never itself a leak), and
#      SECRET_SCAN_ALLOWLIST in orchestration.config (space-separated literal
#      substrings an adopter can add for their own known-safe fixtures).
#
# Findings are NEVER echoed in full in the block message — only a redacted
# preview (first few / last few characters) — so the block message itself
# never becomes a second place the secret leaks (terminal scrollback, CI
# logs).
#
# FAIL-CLOSED on a missing python3 (matches every other hook this session's
# audit hardened). FAILS OPEN (passes through, exit 0) on a malformed/
# unparseable JSON payload — matching guard-main-checkout-writes.sh's own
# convention for this same Write|Edit matcher class (a payload that can't be
# understood isn't itself a threat to react to; Agent-spawn guards fail
# closed on unparseable payloads because spawns are the harder security
# boundary — this hook follows its Write/Edit sibling's convention instead).

_hp_dir="${BASH_SOURCE[0]%/*}"; if [ -f "$_hp_dir/../lib/hook-python.sh" ]; then . "$_hp_dir/../lib/hook-python.sh"; fi  # python3 by path, never Apple's install-dialog stub
set -eo pipefail

command -v python3 >/dev/null 2>&1 || { echo "ERROR: scan-secrets.sh: python3 is required for payload parsing — refusing (fail-closed)" >&2; exit 2; }

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
  hook: scripts/hooks/scan-secrets.sh
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

# --- JURISDICTION ----------------------------------------------------------
# Deliberately BELOW the root-resolution bootstrap, never inside it: Layer R of
# contract-integrity-probe.sh extracts that block verbatim and asserts it is
# byte-identical across every rooted hook, so anything added inside it would
# read as divergence.
#
# The seat resolved above answers "am I governed?". It does NOT answer "does
# the artifact I was just handed belong to the repository I govern?" — and
# until 2026-08-30 nothing asked. See scripts/lib/seat-jurisdiction.sh.
_SJ_LIB="$SCRIPT_DIR/../lib/seat-jurisdiction.sh"
if [ ! -f "$_SJ_LIB" ]; then
    {
        echo "=== RICHOS ENGINE: BROKEN INSTALL — ENFORCEMENT IS NOT ACTIVE ==="
        echo "  hook: scripts/hooks/scan-secrets.sh"
        echo "  scripts/lib/seat-jurisdiction.sh is missing at: $_SJ_LIB"
        echo "  Without it this guard cannot tell whether the artifact it was"
        echo "  handed belongs to the repository it governs, and a guard that"
        echo "  cannot tell must not answer."
    } >&2
    exit 2
fi
# shellcheck source=../lib/seat-jurisdiction.sh
. "$_SJ_LIB"

# Read the payload BEFORE resolving, so the payload's `cwd` is available as a
# resolution candidate. It is the only candidate a subagent session is
# guaranteed to carry.
INPUT="$(cat)"

# Resolve the governed repository. Three outcomes, three different behaviors —
# see the contract for why "block everything unresolvable" is NOT the rule.
if resolve_entity_root "$INPUT"; then
    ENTITY_ROOT="$RICHOS_ENTITY_ROOT_RESOLVED"
elif [ "$RICHOS_ROOT_STATUS" = "not-adopted" ]; then
    # STAND DOWN — LOUDLY. And the loudness is the only thing that changed here,
    # deliberately.
    #
    # I first made this arm carry on scanning, reasoning that a leaked
    # credential is not less leaked because the directory it was written from
    # never adopted the engine. root-contract.test.sh case 8b went red and it
    # was right to. This plugin is enabled at USER SCOPE: it loads in EVERY
    # directory on this machine, so "scan anyway" does not mean "one more
    # repository protected", it means THIS ENGINE STARTS BLOCKING WRITES IN
    # PROJECTS THAT NEVER OPTED IN. That is not a bug fix, it is a policy
    # expansion, and it is the same class of decision as adoption itself —
    # not one to take inside a fix for something else.
    #
    # The mandate was to make standing down LOUD, not to make enforcement
    # universal. So: it stands down, and it says so, naming the repository.
    richos_announce_stand_down "scripts/hooks/scan-secrets.sh" \
        "this repository has not adopted the engine, so nothing written here is scanned for credentials"
    exit 0
else
    # BROKEN: this guard believes it is governing something and cannot. Block.
    root_failure_banner "scripts/hooks/scan-secrets.sh" >&2
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
    unevaluated_or_continue "scan-secrets.sh" "$INPUT" \
        "${ENTITY_ROOT:-${SEAT_ROOT:-${RICHOS_ENTITY_ROOT_RESOLVED:-}}}" \
        "whether the new content puts a live credential on disk"
fi

# The thresholds must come from the repository that governs the FILE, not from
# wherever the session sits — an allowlist written for one repository has no
# authority over another's secrets. This load covers the seat case; it is redone
# below against the governing root when the two differ.
CONFIG="$ENTITY_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
: "${SECRET_SCAN_MIN_LENGTH:=12}"
: "${SECRET_SCAN_MIN_ENTROPY:=3.0}"
: "${SECRET_SCAN_ALLOWLIST:=}"
: "${SECRET_SCAN_CODE_AWARE:=0}"
# Exported so the python3 subprocess below (a separate process, not a bash
# child that inherits unexported shell variables) can actually see them —
# without this, config values loaded from orchestration.config would silently
# never reach the scanner.
export SECRET_SCAN_MIN_LENGTH SECRET_SCAN_MIN_ENTROPY SECRET_SCAN_ALLOWLIST SECRET_SCAN_CODE_AWARE

# (payload already read above, before root resolution)

TOOL_NAME="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("tool_name",""))' 2>/dev/null || true)"
case "$TOOL_NAME" in
  Write|Edit|MultiEdit|NotebookEdit) ;;
  *) exit 0 ;;
esac

FILE_PATH="$(printf '%s' "$INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); ti=d.get("tool_input",{}) or {}; print(ti.get("file_path") or ti.get("notebook_path") or "")' 2>/dev/null || true)"

# --- JURISDICTION: ANNOUNCED, AND DELIBERATELY NOT SKIPPED -----------------
# Every other diverging guard exits 0 when the artifact is not its own. This
# one MUST NOT, and the asymmetry is the point.
#
# A secret written into someone else's repository is still a leaked secret. The
# rule is "an artifact outside the seat is announced, never silently allowed" —
# and here it is not allowed at all, it is scanned. Declining would move
# enforcement in the LESS SAFE direction, which no jurisdiction rule is allowed
# to do.
#
# What the announcement actually buys is the config mismatch: allowlist, minimum
# length and entropy floor were loaded from the SEAT's orchestration.config and
# are about to be applied to a file in a different repository, whose own
# thresholds may be nothing like them. That is worth saying out loud once.
richos_assert_jurisdiction "scripts/hooks/scan-secrets.sh" "$ENTITY_ROOT" "$FILE_PATH" "file" "proceeds" || true

# Re-resolve the thresholds against the repository that actually governs this
# file. No governing root is NOT a reason to stop scanning — it is a reason to
# scan on the built-in defaults, which is exactly what the ':=' fallbacks are.
SS_GOV=""
SS_GOV="$(richos_governing_root "$FILE_PATH" "${ENTITY_ROOT}" 2>/dev/null || true)"
# BOTH SIDES IN THE SAME SPELLING before the comparison. richos_governing_root
# answers physically; the seat arrives from the payload in whatever spelling the
# caller was handed (on macOS, routinely /var rather than /private/var). Compare
# those raw and this reads "a different repository governs this file" about the
# seat itself, and re-resolves thresholds it already had.
SS_SEAT_PHYS="$(richos_physical "${ENTITY_ROOT:-}" 2>/dev/null || printf '%s' "${ENTITY_ROOT:-}")"
if [ -n "$SS_GOV" ] && [ "$SS_GOV" != "$SS_SEAT_PHYS" ]; then
    SECRET_SCAN_ALLOWLIST=""
    # shellcheck disable=SC1090
    [ -f "$SS_GOV/orchestration.config" ] && . "$SS_GOV/orchestration.config"
    : "${SECRET_SCAN_MIN_LENGTH:=12}"
    : "${SECRET_SCAN_MIN_ENTROPY:=3.0}"
    : "${SECRET_SCAN_ALLOWLIST:=}"
    : "${SECRET_SCAN_CODE_AWARE:=0}"
    export SECRET_SCAN_MIN_LENGTH SECRET_SCAN_MIN_ENTROPY SECRET_SCAN_ALLOWLIST SECRET_SCAN_CODE_AWARE
fi

# The actual scan: extract every piece of NEW text this call would introduce
# (Write: content; Edit: new_string; MultiEdit: each edits[].new_string;
# NotebookEdit: new_source), then run both detector classes against it.
# Emits either "CLEAN" or "FOUND\n<label>\t<redacted-preview>\n..." on stdout.
SCAN_PY="$(mktemp -t scan-secrets.XXXXXX.py)"
trap 'rm -f "$SCAN_PY"' EXIT
cat >"$SCAN_PY" <<'PY'
import json, math, os, re, sys

ALLOWLIST = [a for a in os.environ.get("SECRET_SCAN_ALLOWLIST", "").split() if a]
MIN_LEN = int(os.environ.get("SECRET_SCAN_MIN_LENGTH", "12") or "12")
MIN_ENTROPY = float(os.environ.get("SECRET_SCAN_MIN_ENTROPY", "3.0") or "3.0")
CODE_AWARE = (os.environ.get("SECRET_SCAN_CODE_AWARE", "0") or "0").strip() == "1"

try:
    payload = json.loads(sys.stdin.read())
except Exception:
    print("PARSEFAIL")
    sys.exit(0)

if not isinstance(payload, dict):
    print("PARSEFAIL")
    sys.exit(0)

tool_name = payload.get("tool_name", "")
ti = payload.get("tool_input", {})
if not isinstance(ti, dict):
    ti = {}

texts = []
if tool_name == "Write":
    texts.append(ti.get("content") or "")
elif tool_name == "Edit":
    texts.append(ti.get("new_string") or "")
elif tool_name == "MultiEdit":
    for e in (ti.get("edits") or []):
        if isinstance(e, dict):
            texts.append(e.get("new_string") or "")
elif tool_name == "NotebookEdit":
    texts.append(ti.get("new_source") or "")

blob = "\n".join(t for t in texts if isinstance(t, str))

def shannon_entropy(s):
    if not s:
        return 0.0
    freq = {}
    for ch in s:
        freq[ch] = freq.get(ch, 0) + 1
    n = len(s)
    ent = 0.0
    for c in freq.values():
        p = c / n
        ent -= p * math.log2(p)
    return ent

PLACEHOLDER_WORDS = (
    "placeholder", "changeme", "change_me", "example", "dummy", "sample",
    "insert", "replace", "redacted", "notreal", "fake", "your_key",
    "yourkey", "your-key", "todo", "fixme", "test_only", "<your",
)

def is_placeholder(value):
    low = value.lower()
    if any(w in low for w in PLACEHOLDER_WORDS):
        return True
    # A run dominated by one or two distinct characters (e.g. "xxxxxxxxx",
    # "0000000000") is definitionally low-entropy — real secrets are not this.
    if len(value) >= 4 and len(set(value)) <= 2:
        return True
    # Environment-variable-reference syntax is the CORRECT way to handle a
    # secret, never itself a leak.
    if re.search(r'\$\{?[A-Z_][A-Z0-9_]*\}?', value):
        return True
    if 'process.env' in value or 'os.environ' in value or 'os.getenv' in value:
        return True
    for a in ALLOWLIST:
        if a in value:
            return True
    return False

CODE_MEMBER_PATH = re.compile(r'^[A-Za-z_$][A-Za-z0-9_$]*(\.[A-Za-z_$][A-Za-z0-9_$]*)+$')

def is_generic_nonsecret(value):
    # OPT-IN (SECRET_SCAN_CODE_AWARE=1). Applies to the GENERIC key=value
    # detector ONLY — the vendor-prefix and PEM patterns never consult it.
    #
    # This is a PRECISION setting, and it is not free. It exempts two shapes
    # that a real secret can also have, so it is off unless a repository asks
    # for it, and the trade is named here rather than buried:
    #   * a CODE reference — a value carrying structural code punctuation
    #     ( ) [ ] < > ? ! { } or shaped as a dotted member path. An expression,
    #     a type annotation (`token: NSObjectProtocol?`), a generic
    #     (`Array<String>`), a subscript (`dict[key]`). None of those
    #     characters appear in a base64/hex/token secret, so this half is
    #     close to free.
    #   * a DESCRIPTIVE literal — every '-'/'_'-separated segment is either
    #     purely alphabetic or purely numeric ("test-password",
    #     "BEARER_PROTOCOL_FUTURE_TOKEN", "verification-token"). Real generic
    #     secrets interleave digits and mixed case into dense tokens whose
    #     segments are neither ("aB3xQ9zM2kP7wR5vN8tL"). This half is where
    #     test-fixture and doc-example false positives concentrate — and it is
    #     also where the cost sits: measured, it lets through an all-alpha
    #     passphrase, an all-alpha random token, an all-digit long value, and
    #     an alpha-dash-alpha token that the strict scanner catches. Turn it on
    #     when fixture noise is costing you more than that risk.
    if CODE_MEMBER_PATH.match(value):
        return True
    if any(c in value for c in '()[]<>?!{}'):
        return True
    segs = [s for s in re.split(r'[-_]', value) if s]
    # P3-25: a DESCRIPTIVE literal has separators. A single unseparated run of
    # letters or digits (a passphrase, a long numeric secret) is not descriptive.
    # v3 re-check: and each segment is spelled like a WORD (lower, UPPER or
    # Capitalized) or a number. Random letters in mixed case joined by an
    # underscore ("QqWwEe..._OoPpAa...") are a password, not a name.
    if len(segs) >= 2 and all(WORD_SEGMENT.fullmatch(s) for s in segs):
        return True
    return False

WORD_SEGMENT = re.compile(r'[a-z]+|[A-Z]+|[A-Z][a-z]+|[0-9]+')

def redact(value):
    if len(value) <= 10:
        return value[:2] + "…" + value[-1:] if len(value) > 3 else "…"
    return value[:5] + "…redacted…" + value[-3:]

FINDINGS = []

VENDOR_PATTERNS = [
    ("AWS access key",              re.compile(r'\b(AKIA|ASIA)[0-9A-Z]{16}\b')),
    ("GitHub token",                re.compile(r'\b(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36}\b')),
    ("GitHub fine-grained PAT",     re.compile(r'\bgithub_pat_[A-Za-z0-9_]{22,}\b')),
    ("Anthropic API key",           re.compile(r'\bsk-ant-[A-Za-z0-9_-]{20,}\b')),
    ("OpenAI project API key",      re.compile(r'\bsk-proj-[A-Za-z0-9_-]{20,}\b')),
    ("OpenAI API key",              re.compile(r'\bsk-(?!ant-|proj-)[A-Za-z0-9]{20,}\b')),
    ("Stripe live secret key",      re.compile(r'\bsk_live_[A-Za-z0-9]{24,}\b')),
    ("Stripe live publishable key", re.compile(r'\bpk_live_[A-Za-z0-9]{24,}\b')),
    ("Stripe live restricted key",  re.compile(r'\brk_live_[A-Za-z0-9]{24,}\b')),
    ("PEM private key block",       re.compile(r'-----BEGIN (?:RSA |EC |DSA |OPENSSH |ENCRYPTED )?PRIVATE KEY-----')),
]

for label, pattern in VENDOR_PATTERNS:
    for m in pattern.finditer(blob):
        matched = m.group(0)
        if is_placeholder(matched):
            continue
        FINDINGS.append((label, redact(matched)))

GENERIC_RE = re.compile(
    r'(?i)\b(password|passwd|pwd|api[_-]?key|secret|token|access[_-]?key|private[_-]?key)\b'
    r'\s*[:=]\s*(["\']?)([^\s"\';,)]{4,})\2'
)
for m in GENERIC_RE.finditer(blob):
    field, _, value = m.group(1), m.group(2), m.group(3)
    if is_placeholder(value):
        continue
    if CODE_AWARE and is_generic_nonsecret(value):
        continue
    if len(value) < MIN_LEN:
        continue
    if shannon_entropy(value) < MIN_ENTROPY:
        continue
    FINDINGS.append(("%s= literal (high-entropy value)" % field, redact(value)))

if FINDINGS:
    print("FOUND")
    for label, preview in FINDINGS:
        print("%s\t%s" % (label, preview))
else:
    print("CLEAN")
PY

RESULT="$(printf '%s' "$INPUT" | python3 "$SCAN_PY" 2>/dev/null || printf 'PARSEFAIL')"

case "$(printf '%s' "$RESULT" | head -1)" in
  CLEAN|PARSEFAIL)
    exit 0
    ;;
  FOUND)
    {
      echo "=== Secret scan BLOCKED ==="
      echo "  Refusing to write '${FILE_PATH:-<unknown file>}' — the content looks like it"
      echo "  contains a live secret:"
      printf '%s\n' "$RESULT" | tail -n +2 | while IFS=$'\t' read -r label preview; do
        echo "    - $label (redacted: $preview)"
      done
      echo "  If this is a genuine placeholder or test fixture, add the exact substring to"
      echo "  SECRET_SCAN_ALLOWLIST in orchestration.config (space-separated, never flagged"
      echo "  again). NEVER commit a real secret — use an environment variable or a secrets"
      echo "  manager instead."
      echo "(hook: scripts/hooks/scan-secrets.sh)"
    } >&2
    exit 2
    ;;
  *)
    # Unexpected python3 output shape — fail closed rather than silently pass.
    echo "ERROR: scan-secrets.sh: unexpected scanner output — refusing (fail-closed)" >&2
    exit 2
    ;;
esac
