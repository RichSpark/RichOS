#!/usr/bin/env bash
#
# hook-staleness.test.sh — regression tests for the mid-session hook-staleness
# PAIR:
#   scripts/hooks/snapshot-enforcing-hooks.sh  (SessionStart baseline)
#   scripts/hooks/notice-hook-staleness.sh     (Stop notice)
#
# Every case runs against an isolated sandbox root (HOOK_STALENESS_ROOT) with a
# sandbox registration surface (HOOK_STALENESS_SURFACE), so the real repo's
# .claude/state and the real hooks/hooks.json are never written and never
# depended on.
#
# THE TWO PROPERTIES THAT MATTER MOST, AND WHY THEY ARE CASES RATHER THAN CLAIMS
# -----------------------------------------------------------------------------
#   ZERO FALSE POSITIVES. A session in which the hook table did not change must
#   produce NOTHING — not a quiet OK, not an empty JSON object, nothing at all.
#   Cases 2, 2b, 2c and 8b assert byte-empty output under conditions that could
#   plausibly fool a weaker design: an unchanged table, a REORDERED table, an
#   edited guard BODY, and an edited settings surface. The last two are the
#   measured facts the whole design turns on — script bodies and settings hooks
#   both take effect immediately, so reporting either as "inert" would be a
#   false positive.
#
#   NEGATIVE CONTROLS. An empty corpus produces an empty delta, which is
#   indistinguishable from a clean run unless something checks. Cases 5a-5d
#   drive the baseline to zero rows, to a lying rows= header, to a foreign
#   engine, and to absent, and require the hook to say it CANNOT COMPARE rather
#   than pass quietly. Case 9 is the meta-control: it asserts the suite's own
#   "silent" assertion is load-bearing by breaking the hook and watching case 2
#   go red.
#
# Covers:
#   (1)  snapshotter writes a baseline: header, rows= count, latest symlink
#   (1b) baseline rows are DERIVED from the surface, not typed
#   (2)  unchanged table                       -> byte-empty output
#   (2b) REORDERED table, same set             -> byte-empty output
#   (2c) guard BODY edited mid-session         -> byte-empty (bodies are live)
#   (3)  guard landed mid-session              -> notice naming it, INERT, and
#                                                 RESTART as the operator's act
#   (3b) the three required statements are all present in the message
#   (4)  same drift a second time              -> silent (once per session)
#   (4b) drift GROWS                           -> speaks again, names both
#   (4c) drift shrinks back                    -> silent
#   (5a) baseline with zero rows               -> CANNOT CHECK
#   (5b) baseline whose rows= header lies      -> CANNOT CHECK
#   (5c) baseline from a different engine      -> CANNOT CHECK
#   (5d) no baseline at all                    -> CANNOT CHECK, naming restart
#   (5e) current surface unparseable           -> CANNOT CHECK
#   (6)  an already-wired script added to a NEW event -> reported as new wiring
#   (7)  a registration REMOVED mid-session    -> reported as still firing
#   (8)  not-adopted repository                -> byte-empty, always
#   (8b) the settings surface changing         -> byte-empty (it hot-reloads)
#   (9)  META: case 2's silence assertion is load-bearing
#   (10) never blocks: every case above exits 0
#   (11) BOTH registration surfaces carry the pair, verified by PARSING
#   (12) SessionStart source=clear/compact  -> baseline PRESERVED (same process)
#   (12b) an inert guard is STILL named after a compaction re-fire
#   (12c) source=startup/resume             -> baseline refreshed (new process)
#   (13) the notice names the inert guard on BOTH channels — systemMessage
#        (person) and additionalContext (model)
#
# Run directly: scripts/hooks/hook-staleness.test.sh
# Exit 0 = all pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# ../.. — this file lives at engine/scripts/hooks/, so the engine root is two
# levels up, not one. It was one level up in the first draft, every sandbox got
# no hooks.json, and 18 cases failed in a way that at least announced itself
# loudly (cp errors) rather than passing over an empty corpus.
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# The registration question has ONE answer in this engine, and it is not a
# grep: seventeen guards are wired by scripts/hooks/dispatch-pretooluse.manifest
# rather than by an entry of their own. hook_enforced_on_surface asks whether
# the surface causes the guard to RUN, which is the property this case is about.
# shellcheck source=../lib/registered-hooks.sh
. "$ENGINE_ROOT/scripts/lib/registered-hooks.sh"


# Declare the subject, for the reason spelled out in guard-definition-drift.
# test.sh: run from a session seated in some other repository these hooks would
# correctly resolve THAT repository, stand down, and every case below would pass
# by never running.
RICHOS_ENTITY_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
export RICHOS_ENTITY_ROOT
unset CLAUDE_PROJECT_DIR

SNAP="$SCRIPT_DIR/snapshot-enforcing-hooks.sh"
NOTE="$SCRIPT_DIR/notice-hook-staleness.sh"
REAL_HOOKS="$ENGINE_ROOT/hooks/hooks.json"

if [ ! -x "$SNAP" ] || [ ! -x "$NOTE" ]; then
    echo "FATAL: snapshot-enforcing-hooks.sh or notice-hook-staleness.sh missing/non-exec" >&2
    exit 1
fi
command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

PASS=0
FAIL=0
FAIL_NAMES=()
ok()  { PASS=$((PASS + 1)); printf '  PASS  %s\n' "$1"; }
bad() { FAIL=$((FAIL + 1)); FAIL_NAMES+=("$1"); printf '  FAIL  %s  %s\n' "$1" "${2:-}"; }

SESSION_ID="cafebabe-0000-4000-8000-000000000000"
SESSION_SHORT="cafebabe"
# The parent owns one scratch root. Factories may run in command substitutions
# without losing ownership of the children they allocate.
SUITE_SCRATCH="$(mktemp -d "${TMPDIR:?}/hook-staleness-suite.XXXXXX")"
cleanup() { rm -rf "$SUITE_SCRATCH"; }
trap cleanup EXIT

# make_sandbox — an adopted repo with a copy of the real registration surface.
# Adoption is DECLARED: without orchestration.config the hooks correctly treat
# the sandbox as an unadopted repo and stand down, and every case passes by
# doing nothing.
make_sandbox() {
    local root
    root="$(mktemp -d "$SUITE_SCRATCH/sandbox.XXXXXX")"
    mkdir -p "$root/.claude/state"
    printf 'PROTECTED_PATHS="src"\n' >"$root/orchestration.config"
    cp "$REAL_HOOKS" "$root/hooks.json"
    printf '%s' "$root"
}

take_baseline() { # <root> [session]
    local root="$1" sid="${2:-$SESSION_ID}"
    HOOK_STALENESS_ROOT="$root" HOOK_STALENESS_SURFACE="$root/hooks.json" \
        "$SNAP" --session "$sid" >/dev/null 2>&1
}

# run_notice <root> -> stdout of the Stop hook; RC in $NOTICE_RC
NOTICE_RC=0
run_notice() { # <root> [session]
    local root="$1" sid="${2:-$SESSION_ID}" out
    out="$(printf '{"session_id":"%s","hook_event_name":"Stop"}' "$sid" \
        | HOOK_STALENESS_ROOT="$root" HOOK_STALENESS_SURFACE="$root/hooks.json" \
          "$NOTE" 2>/dev/null)"
    NOTICE_RC=$?
    printf '%s' "$out"
}

# wire_hook <surface> <event> <script> — land a guard into the table, the way
# a merge does.
wire_hook() {
    python3 - "$1" "$2" "$3" <<'PY'
import json, sys
path, event, script = sys.argv[1], sys.argv[2], sys.argv[3]
d = json.load(open(path))
d.setdefault("hooks", {}).setdefault(event, []).append({
    "hooks": [{"type": "command",
               "command": "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/%s" % script,
               "timeout": 10}]
})
json.dump(d, open(path, "w"), indent=2)
PY
}

# unwire_hook <surface> <script> — remove every registration naming <script>.
unwire_hook() {
    python3 - "$1" "$2" <<'PY'
import json, sys
path, script = sys.argv[1], sys.argv[2]
d = json.load(open(path))
for event, entries in list(d.get("hooks", {}).items()):
    kept = []
    for entry in entries:
        hooks = [h for h in entry.get("hooks", []) or []
                 if script not in str(h.get("command", ""))]
        if hooks:
            entry["hooks"] = hooks
            kept.append(entry)
    d["hooks"][event] = kept
json.dump(d, open(path, "w"), indent=2)
PY
}

echo "=== landed mid-session means inert, and the operator is told to restart ==="
echo ""

# ===========================================================================
# 1. THE BASELINE
# ===========================================================================
R1="$(make_sandbox)"
take_baseline "$R1"
SNAPFILE="$R1/.claude/state/enforcing-hooks-${SESSION_SHORT}.snapshot"
if [ -f "$SNAPFILE" ]; then
    ok "1   snapshotter writes .claude/state/enforcing-hooks-<session8>.snapshot"
else
    bad "1   snapshotter writes a baseline" "(no file at $SNAPFILE)"
fi

HDR_ROWS="$(sed -n 's/^#.*[[:space:]]rows=\([0-9][0-9]*\).*$/\1/p' "$SNAPFILE" 2>/dev/null | head -1)"
REAL_ROWS="$(grep -vc '^#' "$SNAPFILE" 2>/dev/null || echo 0)"
if [ -n "$HDR_ROWS" ] && [ "$HDR_ROWS" -eq "$REAL_ROWS" ] && [ "$REAL_ROWS" -ge 1 ]; then
    ok "1a  the rows= header matches the rows actually written ($REAL_ROWS), and is non-zero"
else
    bad "1a  rows= header matches" "(header=$HDR_ROWS actual=$REAL_ROWS)"
fi

if [ -e "$R1/.claude/state/enforcing-hooks-latest.snapshot" ]; then
    ok "1b  the latest handle is refreshed"
else
    bad "1b  latest handle" "(absent)"
fi

# DERIVED, NOT TYPED: the count in the baseline must equal what an independent
# reading of the same surface produces. A typed list of 14 where the
# registration held 15 is the drift that started this whole sequence.
# The independent parse implements the SAME DECLARED RULE, independently —
# including the one added on 2026-09-15: a registered dispatch-pretooluse.sh
# stands for every rule its manifest gives it, on the dispatcher's own event and
# matcher. Copying the library's answer instead would make this a tautology; not
# implementing the rule at all would make it a permanent false alarm. So the
# manifest is read here, directly, by code that shares nothing with
# scripts/lib/registered-hooks.sh.
INDEP_N="$(python3 -c '
import json, os, re, sys
d = json.load(open(sys.argv[1]))
manifest = {}
mpath = os.path.join(sys.argv[2], "scripts", "hooks", "dispatch-pretooluse.manifest")
if os.path.isfile(mpath):
    for line in open(mpath, encoding="utf-8"):
        line = line.strip()
        if line and not line.startswith("#") and "|" in line:
            k, mod = line.split("|", 1)
            manifest.setdefault(k.strip(), []).append(mod.strip())
rows = set()
for ev, entries in d.get("hooks", {}).items():
    for e in entries:
        matcher = e.get("matcher", "") or "-"
        for h in e.get("hooks", []) or []:
            cmd = h.get("command", "")
            for m in re.findall(r"scripts/hooks/([A-Za-z0-9._+-]+\.sh)", cmd):
                rows.add((ev, matcher, m))
            hit = re.search(r"scripts/hooks/dispatch-pretooluse\.sh\s+(\S+)", cmd)
            if hit:
                for mod in manifest.get(hit.group(1), []):
                    rows.add((ev, matcher, mod))
print(len(rows))
' "$R1/hooks.json" "$ENGINE_ROOT")"
if [ "$REAL_ROWS" -eq "$INDEP_N" ]; then
    ok "1c  the baseline is DERIVED: $REAL_ROWS rows, matching an independent parse of the same surface"
else
    bad "1c  baseline is derived" "(baseline=$REAL_ROWS independent=$INDEP_N)"
fi

# ===========================================================================
# 2. ZERO FALSE POSITIVES — silence when nothing happened
# ===========================================================================
OUT="$(run_notice "$R1")"
if [ -z "$OUT" ]; then
    ok "2   nothing changed -> byte-empty output (the property the whole design turns on)"
else
    bad "2   unchanged session is silent" "(said: $OUT)"
fi

# A REORDER is the classic spurious-diff generator: same set, different file.
R2="$(make_sandbox)"
take_baseline "$R2"
python3 - "$R2/hooks.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
for event in d.get("hooks", {}):
    d["hooks"][event] = list(reversed(d["hooks"][event]))
d["hooks"] = dict(reversed(list(d["hooks"].items())))
json.dump(d, open(p, "w"), indent=2)
PY
OUT="$(run_notice "$R2")"
if [ -z "$OUT" ]; then
    ok "2b  table REORDERED, same set -> byte-empty (a set comparison, not a diff)"
else
    bad "2b  reorder is silent" "(said: $OUT)"
fi

# A guard BODY edit. MEASURED FACT: a registration names `bash <path>` and that
# path is executed afresh on every event, so a body edit is live immediately.
# Calling it inert would be a false positive.
R3="$(make_sandbox)"
mkdir -p "$R3/scripts/hooks"
printf '#!/usr/bin/env bash\nexit 0\n' >"$R3/scripts/hooks/guard-bash-main-writes.sh"
take_baseline "$R3"
printf '#!/usr/bin/env bash\n# edited mid-session\nexit 0\n' >"$R3/scripts/hooks/guard-bash-main-writes.sh"
OUT="$(run_notice "$R3")"
if [ -z "$OUT" ]; then
    ok "2c  guard BODY edited mid-session -> byte-empty (bodies re-execute per event; only registration is frozen)"
else
    bad "2c  body edit is silent" "(said: $OUT)"
fi

# ===========================================================================
# 3. THE FAILURE THIS EXISTS FOR — a guard landed mid-session
# ===========================================================================
# ---------------------------------------------------------------------------
# 0f. The fixture sentinels are NOT real registrations.
# ---------------------------------------------------------------------------
# Without this, a fixture silently stops testing anything the day someone lands
# a guard with the same name -- which is exactly what happened on 2026-08-30 and
# cost four red cases that blamed the mechanism.
_SENTINELS="zz-fixture-never-registered.sh zz-fixture-second-never-registered.sh"
_SENT_BAD=""
for _s in $_SENTINELS; do
    if hook_enforced_on_surface "$ENGINE_ROOT/hooks/hooks.json" "$_s"; then
        _SENT_BAD="$_SENT_BAD $_s"
    fi
done
if [ -z "$_SENT_BAD" ]; then
    ok "0f  fixture sentinels are absent from the shipped registration (a fixture that names a real hook tests nothing)"
else
    bad "0f  fixture sentinels" "these fixture names ARE registered, so their cases prove nothing:$_SENT_BAD"
fi

R4="$(make_sandbox)"
take_baseline "$R4"
# FIXTURE NAMES MUST NOT BE REAL REGISTRATIONS.
# This read "guard-idle-land.sh" until 2026-08-30, when that guard actually
# landed on Stop. The sandbox copies the shipped hooks.json, so wiring a name
# that is ALREADY in it changes nothing -- and this check compares SETS, by
# design -- so the notice correctly stayed silent and four cases went red
# describing the mechanism as broken when it was working perfectly.
# The sentinel below can never be a real hook, and case 0f asserts that.
wire_hook "$R4/hooks.json" "Stop" "zz-fixture-never-registered.sh"
OUT="$(run_notice "$R4")"
if [ -n "$OUT" ] && printf '%s' "$OUT" | grep -q 'zz-fixture-never-registered.sh'; then
    ok "3   a guard landed mid-session is NAMED in the notice"
else
    bad "3   landed guard is named" "(said: ${OUT:-<nothing>})"
fi

# The three required statements. Dropping any one of them rebuilds the original
# failure, so each is asserted separately rather than as one blob.
MISSING=""
printf '%s' "$OUT" | grep -qi 'ENFORCING NOTHING RIGHT NOW' || MISSING="$MISSING (2:inert-now)"
printf '%s' "$OUT" | grep -qi 'RESTART THIS SESSION'        || MISSING="$MISSING (3:restart)"
printf '%s' "$OUT" | grep -qi 'YOURS TO DO'                 || MISSING="$MISSING (3:actor)"
if [ -z "$MISSING" ]; then
    ok "3b  the notice states all three: WHICH guard, that it is inert NOW, and that RESTARTING is the operator's act"
else
    bad "3b  all three statements present" "missing:$MISSING"
fi

# It is a systemMessage, because that is the only channel that reaches the one
# party who can restart.
if printf '%s' "$OUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if isinstance(d.get("systemMessage"),str) and d["systemMessage"] else 1)'; then
    ok "3c  emitted as {\"systemMessage\":...} — the operator channel, not the model's"
else
    bad "3c  emitted as systemMessage" "(payload: $OUT)"
fi

# ===========================================================================
# 4. CADENCE — once, then only on growth
# ===========================================================================
OUT="$(run_notice "$R4")"
if [ -z "$OUT" ]; then
    ok "4   the same drift a second time -> silent (a notice every turn gets muted)"
else
    bad "4   second fire is silent" "(said: $OUT)"
fi

wire_hook "$R4/hooks.json" "Stop" "zz-fixture-second-never-registered.sh"
OUT="$(run_notice "$R4")"
if printf '%s' "$OUT" | grep -q 'zz-fixture-second-never-registered.sh' \
   && printf '%s' "$OUT" | grep -q 'zz-fixture-never-registered.sh'; then
    ok "4b  drift GROWS -> speaks again, naming both the new guard and the one already reported"
else
    bad "4b  growth speaks again" "(said: ${OUT:-<nothing>})"
fi

unwire_hook "$R4/hooks.json" "guard-turn-manifest.sh"
OUT="$(run_notice "$R4")"
if [ -z "$OUT" ]; then
    ok "4c  drift shrinks back -> silent (nothing new to say)"
else
    bad "4c  shrink is silent" "(said: $OUT)"
fi

# ===========================================================================
# 5. NEGATIVE CONTROLS — a green run must prove it read something
# ===========================================================================
neg_case() { # <name> <root> <expect-substring>
    local name="$1" root="$2" want="$3" out
    out="$(run_notice "$root")"
    if printf '%s' "$out" | grep -q 'CANNOT CHECK' && printf '%s' "$out" | grep -qi "$want"; then
        ok "$name"
    else
        bad "$name" "(said: ${out:-<nothing>})"
    fi
}

R5="$(make_sandbox)"
printf '# enforcing-hook snapshot\n# session=%s generated=x engine=%s surface=%s rows=0\n' \
    "$SESSION_ID" "$ENGINE_ROOT" "$R5/hooks.json" \
    >"$R5/.claude/state/enforcing-hooks-${SESSION_SHORT}.snapshot"
neg_case "5a  baseline with ZERO rows -> CANNOT CHECK, never 'no drift'" "$R5" "ZERO hook rows"

R6="$(make_sandbox)"
take_baseline "$R6"
sed -i.bak 's/rows=[0-9][0-9]*/rows=99/' "$R6/.claude/state/enforcing-hooks-${SESSION_SHORT}.snapshot"
neg_case "5b  baseline whose rows= header LIES -> CANNOT CHECK" "$R6" "truncated or corrupt"

R7="$(make_sandbox)"
take_baseline "$R7"
sed -i.bak "s|engine=[^ ]*|engine=/some/other/engine|" "$R7/.claude/state/enforcing-hooks-${SESSION_SHORT}.snapshot"
neg_case "5c  baseline recorded against a DIFFERENT engine -> CANNOT CHECK" "$R7" "two different engines"

R8="$(make_sandbox)"
neg_case "5d  no baseline at all -> CANNOT CHECK" "$R8" "no session-start baseline"
OUT="$(run_notice "$R8" "feedfeed-0000-4000-8000-000000000000")"
if printf '%s' "$OUT" | grep -qi 'RESTART' && printf '%s' "$OUT" | grep -qi 'yours to do'; then
    ok "5d2 the no-baseline message ALSO names restart as the operator's act"
else
    bad "5d2 no-baseline names the remedy" "(said: ${OUT:-<nothing>})"
fi

R9="$(make_sandbox)"
take_baseline "$R9"
printf 'not json at all\n' >"$R9/hooks.json"
neg_case "5e  current surface unparseable -> CANNOT CHECK" "$R9" "could not derive"

# ===========================================================================
# 6/7. THE OTHER TWO DELTA KINDS
# ===========================================================================
R10="$(make_sandbox)"
take_baseline "$R10"
# scan-secrets.sh is already wired on PreToolUse; wiring it onto PostToolUse is
# a new hook POINT for a script that was present at session start.
wire_hook "$R10/hooks.json" "PostToolUse" "scan-secrets.sh"
OUT="$(run_notice "$R10")"
if printf '%s' "$OUT" | grep -q 'newly wired on PostToolUse'; then
    ok "6   an already-wired script added to a NEW event is reported as a dead hook point"
else
    bad "6   new wiring reported" "(said: ${OUT:-<nothing>})"
fi

R11="$(make_sandbox)"
take_baseline "$R11"
unwire_hook "$R11/hooks.json" "guard-workflow-ban.sh"
OUT="$(run_notice "$R11")"
if printf '%s' "$OUT" | grep -q 'guard-workflow-ban.sh' \
   && printf '%s' "$OUT" | grep -qi 'inventory row(s) REMOVED'; then
    ok "7   removed inventory is reported without guessing its host or manifest origin"
else
    bad "7   removal reported" "(said: ${OUT:-<nothing>})"
fi

# ===========================================================================
# 8. STAND-DOWN AND THE SETTINGS SURFACE
# ===========================================================================
R12="$(mktemp -d "$SUITE_SCRATCH/unadopted.XXXXXX")"
mkdir -p "$R12/.claude/state"
cp "$REAL_HOOKS" "$R12/hooks.json"      # deliberately NO orchestration.config
# The not-adopted STATUS is reached through the candidate chain, never through
# the env override: an explicitly declared root that carries no marker is
# "broken", not "not adopted", and the resolver is right to say so. So this case
# clears the override and drives the real chain, which is also the only shape a
# real unadopted session ever has.
OUT="$(printf '{"session_id":"%s","hook_event_name":"Stop"}' "$SESSION_ID" \
    | ( cd "$R12" && env -u RICHOS_ENTITY_ROOT -u HOOK_STALENESS_ROOT \
        CLAUDE_PROJECT_DIR="$R12" HOOK_STALENESS_SURFACE="$R12/hooks.json" \
        "$NOTE" 2>/dev/null ) )"
R12_RC=$?
if [ -z "$OUT" ] && [ "$R12_RC" -eq 0 ]; then
    ok "8   unadopted repository -> byte-empty (the plugin loads everywhere; a notice in each would be noise)"
else
    bad "8   unadopted is silent" "(rc=$R12_RC said: $OUT)"
fi

# The sibling half of the same boundary: an EXPLICITLY declared root that is not
# adopted is a different situation and must NOT be silent — the operator said
# "govern this" and the engine cannot.
OUT="$(printf '{"session_id":"%s","hook_event_name":"Stop"}' "$SESSION_ID" \
    | HOOK_STALENESS_ROOT="$R12" HOOK_STALENESS_SURFACE="$R12/hooks.json" \
      "$NOTE" 2>/dev/null)"
if printf '%s' "$OUT" | grep -q 'CANNOT CHECK'; then
    ok "8a  a DECLARED root that is not adopted -> CANNOT CHECK, not silence"
else
    bad "8a  declared-but-unadopted speaks" "(said: ${OUT:-<nothing>})"
fi

# MEASURED FACT: .claude/settings.local.json HOT-RELOADS mid-session — a hook
# appended to it fired on the very next tool call. So a settings edit must NOT
# be reported as inert. This case exists because "check both surfaces for
# completeness" was a real temptation and would have been a guaranteed false
# positive.
R13="$(make_sandbox)"
take_baseline "$R13"
mkdir -p "$R13/.claude"
cat >"$R13/.claude/settings.local.json" <<'EOF'
{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"$CLAUDE_PROJECT_DIR/scripts/hooks/guard-landed-in-settings.sh"}]}]}}
EOF
OUT="$(run_notice "$R13")"
if [ -z "$OUT" ]; then
    ok "8b  a guard landed in the SETTINGS surface -> byte-empty (that surface hot-reloads; it is already enforcing)"
else
    bad "8b  settings edit is silent" "(said: $OUT)"
fi

# ===========================================================================
# 9. META-CONTROL — is case 2's silence assertion load-bearing?
#
# Every "must be silent" case above passes when the hook works AND when the
# hook is broken enough to print nothing ever. That is the shape of a check
# that passes for the wrong reason, and this operation has shipped two of them.
# So: break the comparison deliberately (a baseline that omits one row, i.e. a
# guard that WILL look landed) and require case 2's assertion to go red.
# ===========================================================================
R14="$(make_sandbox)"
take_baseline "$R14"
SNAP14="$R14/.claude/state/enforcing-hooks-${SESSION_SHORT}.snapshot"
python3 - "$SNAP14" <<'PY'
import re, sys
p = sys.argv[1]
lines = open(p).read().splitlines()
head = [l for l in lines if l.startswith("#")]
rows = [l for l in lines if not l.startswith("#") and l.strip()]
rows = rows[1:]                       # drop one row: it will look newly landed
head = [re.sub(r"rows=\d+", "rows=%d" % len(rows), h) for h in head]
open(p, "w").write("\n".join(head + rows) + "\n")
PY
OUT="$(run_notice "$R14")"
if [ -n "$OUT" ]; then
    ok "9   META: the silence assertion is load-bearing — a doctored baseline DOES produce a notice"
else
    bad "9   META: silence assertion load-bearing" "(a missing baseline row produced no notice, so every 'silent' case above may be passing for the wrong reason)"
fi

# ===========================================================================
# 10. NEVER BLOCKS
# ===========================================================================
BLOCKED=""
for r in "$R1" "$R4" "$R5" "$R8" "$R9" "$R12" "$R14"; do
    run_notice "$r" >/dev/null
    [ "$NOTICE_RC" -eq 0 ] || BLOCKED="$BLOCKED $r(rc=$NOTICE_RC)"
done
if [ -z "$BLOCKED" ]; then
    ok "10  every path exits 0 — this is information, never enforcement"
else
    bad "10  never blocks" "non-zero from:$BLOCKED"
fi

# ===========================================================================
# 11. REGISTRATION, VERIFIED BY PARSING BOTH SURFACES
#
# grep cannot see the defect this guards against: two "command" keys in ONE
# hook object is valid JSON, a duplicate key silently keeps the last, and the
# raw text still contains the name of the script that is no longer registered.
# Nor can a text scan count them — the literal string "command" also appears as
# the VALUE of "type", and the plugin surface's ${CLAUDE_PLUGIN_ROOT} braces
# defeat naive block matching. So: the JSON parser, with a duplicate-key hook.
# ===========================================================================
REG_OUT="$(python3 - "$ENGINE_ROOT" <<'PY'
import json, re, sys
engine = sys.argv[1]
surfaces = [("plugin", f"{engine}/hooks/hooks.json"),
            ("seated", f"{engine}/.claude/settings.local.json")]
want = {"snapshot-enforcing-hooks.sh": "SessionStart",
        "notice-hook-staleness.sh": "Stop"}
problems, sets = [], []

def pairs_hook(pairs):
    seen = set()
    for k, _ in pairs:
        if k in seen:
            problems.append("duplicate key %r in one object" % k)
        seen.add(k)
    return dict(pairs)

for label, path in surfaces:
    doc = json.load(open(path, encoding="utf-8"), object_pairs_hook=pairs_hook)
    parsed = {}
    for event, entries in doc.get("hooks", {}).items():
        for entry in entries:
            for h in entry.get("hooks", []) or []:
                for m in re.findall(r"scripts/hooks/([A-Za-z0-9._+-]+\.sh)",
                                    h.get("command", "")):
                    parsed.setdefault(m, []).append(event)
    sets.append(set(parsed))
    for script, event in want.items():
        sites = parsed.get(script, [])
        if len(sites) != 1:
            problems.append("%s: %s registered %d time(s), want exactly 1"
                            % (label, script, len(sites)))
        elif sites[0] != event:
            problems.append("%s: %s on %s, want %s" % (label, script, sites[0], event))
if sets[0] != sets[1]:
    problems.append("surfaces disagree: only-plugin=%s only-seated=%s"
                    % (sorted(sets[0] - sets[1]), sorted(sets[1] - sets[0])))
print("; ".join(problems) if problems else "OK %d" % len(sets[0]))
PY
)"
case "$REG_OUT" in
    OK\ *) ok "11  both registration surfaces carry the pair exactly once, no duplicate keys, identical sets (${REG_OUT#OK } scripts) — verified by PARSING" ;;
    *)     bad "11  registration verified by parsing" "$REG_OUT" ;;
esac

# ===========================================================================
# 12. THE BASELINE IS WRITE-ONCE PER OS PROCESS
# ===========================================================================
# SessionStart is not one event. The host fires it again on `/clear` and on
# every compaction, in the SAME process, and the host reads the plugin hook
# table only at process start. Rewriting the baseline on those re-fires made
# the comparison FORGIVE every guard that landed mid-session: the notice then
# reported no drift for the rest of a session in which those guards enforced
# nothing.
#
# This is the case that was missing on 2026-09-14, when
# enforcing-hooks-b7d89f44.snapshot was stamped generated=2026-09-14T19:10:17Z
# for a process that started 2026-09-13 23:59:44, and was then quoted as proof
# that a guard was live in a session it had never been loaded into.
R12="$(make_sandbox)"
take_baseline "$R12"
SNAP12="$R12/.claude/state/enforcing-hooks-${SESSION_SHORT}.snapshot"
BASE12_ROWS="$(grep -vc '^#' "$SNAP12" 2>/dev/null || echo 0)"

wire_hook "$R12/hooks.json" "Stop" "zz-fixture-never-registered.sh"

for src in clear compact; do
    HOOK_STALENESS_ROOT="$R12" HOOK_STALENESS_SURFACE="$R12/hooks.json" \
        "$SNAP" --session "$SESSION_ID" --source "$src" >/dev/null 2>&1 || true
    NOW12="$(grep -vc '^#' "$SNAP12" 2>/dev/null || echo 0)"
    if [ "$NOW12" -eq "$BASE12_ROWS" ] \
       && ! grep -q 'zz-fixture-never-registered.sh' "$SNAP12" 2>/dev/null; then
        ok "12  SessionStart source=$src does NOT rewrite the baseline (same process, same booted table)"
    else
        bad "12  source=$src preserves the baseline" \
            "(rows ${BASE12_ROWS}->${NOW12}; mid-session guard leaked into the boot baseline)"
    fi
done

# THE CONSEQUENCE, asserted separately from the mechanism: after a compaction
# the notice must STILL name the inert guard. A baseline that moved would make
# this silent, which is the whole damage.
OUT12="$(run_notice "$R12")"
if printf '%s' "$OUT12" | grep -q 'zz-fixture-never-registered.sh'; then
    ok "12b a guard landed mid-session is STILL named after a compaction re-fire"
else
    bad "12b inert guard survives compaction" "(said: ${OUT12:-<nothing>})"
fi

# A REAL process restart is the opposite case and must NOT be frozen: the host
# genuinely re-read the table, so the new registrations ARE live and the
# baseline has to catch up. Freezing here would invent permanent false
# positives, which is how a true notice gets trained into noise.
for src in startup resume; do
    R12B="$(make_sandbox)"
    take_baseline "$R12B"
    SNAP12B="$R12B/.claude/state/enforcing-hooks-${SESSION_SHORT}.snapshot"
    wire_hook "$R12B/hooks.json" "Stop" "zz-fixture-never-registered.sh"
    HOOK_STALENESS_ROOT="$R12B" HOOK_STALENESS_SURFACE="$R12B/hooks.json" \
        "$SNAP" --session "$SESSION_ID" --source "$src" >/dev/null 2>&1 || true
    if grep -q 'zz-fixture-never-registered.sh' "$SNAP12B" 2>/dev/null; then
        ok "12c SessionStart source=$src DOES refresh the baseline (new process really did re-read the table)"
    else
        bad "12c source=$src refreshes the baseline" "(new registration absent from the refreshed baseline)"
    fi
done


# ===========================================================================
# 13. THE NOTICE REACHES THE ASSISTANT, NOT ONLY THE OPERATOR
# ===========================================================================
# Measured channel table (this engine's own, 2026-09-14): on Stop,
# systemMessage renders to the PERSON and additionalContext reaches the MODEL.
# Emitting only systemMessage is why the orchestrator briefed an engineer on
# 2026-09-14 with "present in the booting session's hook set" while this hook
# had already named that very guard as inert. An assistant that cannot learn
# its guards are off writes the false premise into the next brief.
R13="$(make_sandbox)"
take_baseline "$R13"
wire_hook "$R13/hooks.json" "Stop" "zz-fixture-never-registered.sh"
OUT13="$(run_notice "$R13")"
CHAN13="$(printf '%s' "$OUT13" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
except Exception:
    print("UNPARSEABLE"); raise SystemExit
person = str(d.get("systemMessage") or "")
model = str((d.get("hookSpecificOutput") or {}).get("additionalContext") or "")
ev = str((d.get("hookSpecificOutput") or {}).get("hookEventName") or "")
missing = []
if "zz-fixture-never-registered.sh" not in person: missing.append("systemMessage")
if "zz-fixture-never-registered.sh" not in model: missing.append("additionalContext")
if ev != "Stop": missing.append("hookEventName=%r want Stop" % ev)
print("OK" if not missing else "MISSING:" + ",".join(missing))
' 2>/dev/null || echo UNPARSEABLE)"
if [ "$CHAN13" = "OK" ]; then
    ok "13  the notice names the inert guard on BOTH channels — systemMessage (person) and additionalContext (model)"
else
    bad "13  notice reaches both audiences" "($CHAN13)"
fi

# ===========================================================================
# 14. THE SEATED SURFACE IS GENERATED, NOT MAINTAINED
#
# Case 11 above asserts the two surfaces carry identical sets. Until
# 2026-09-15 the only thing keeping that true was every hook author
# remembering to hand-edit a 500-line JSON file, and it had already drifted:
# measured across 76 entries, 74 were an exact transform of the plugin table
# INCLUDING timeouts and the two that were not were drift, not intent
# (session-start-escalations.sh carried a timeout the real table does not).
# scripts/hooks/install.sh now DERIVES the seated `hooks` key from
# hooks/hooks.json, so case 11 cannot go red for a forgotten edit again.
#
# These cases live here, beside the assertion they protect, rather than in a
# suite of their own — and they also give install.sh its first coverage:
# `ci-affected-units.sh --paths scripts/hooks/install.sh` selected ZERO suites
# before this section existed, so the script that mints every sha256 sidecar
# was changed by nobody's test.
# ===========================================================================

# A sandbox that IS an engine: install.sh takes its repo root from its own
# ../.., so the whole tree is copied rather than stubbed. Scratch stays under
# the configured TMPDIR. CLAUDE_CONFIG_DIR is redirected so fixture installs
# cannot change the operator's engine pointer.
make_engine_sandbox() {
    local root
    root="$(mktemp -d "$SUITE_SCRATCH/engine.XXXXXX")"
    cp -R "$ENGINE_ROOT/." "$root/" 2>/dev/null || true
    mkdir -p "$root/fake-config"
    printf '%s' "$root"
}

# Register a fixture hook in the sandbox's PLUGIN table only.
seed_fixture_registration() {
    python3 - "$1" "$2" <<'PY'
import json, sys, collections
path, cmd = sys.argv[1:3]
d = json.load(open(path, encoding="utf-8"), object_pairs_hook=collections.OrderedDict)
d["hooks"].setdefault("Stop", []).append(
    {"hooks": [{"type": "command", "command": cmd}]})
open(path, "w", encoding="utf-8").write(json.dumps(d, indent=2) + "\n")
PY
}

seated_names() {   # seated_names <settings file> <basename> -> count
    python3 - "$1" "$2" <<'PY'
import json, sys
path, name = sys.argv[1:3]
d = json.load(open(path, encoding="utf-8"))
n = sum(1 for es in d.get("hooks", {}).values() for e in es
        for h in (e.get("hooks") or []) if name in h.get("command", ""))
print(n)
PY
}

FIXTURE="zz-fixture-generated-surface.sh"

# --- 14a. the whole point: one edit, in the registration, and nothing else ---
E1="$(make_engine_sandbox)"
printf '#!/usr/bin/env bash\nexit 0\n' >"$E1/scripts/hooks/$FIXTURE"
chmod +x "$E1/scripts/hooks/$FIXTURE"
seed_fixture_registration "$E1/hooks/hooks.json" "bash \${CLAUDE_PLUGIN_ROOT}/scripts/hooks/$FIXTURE"
SEATED_BEFORE="$(seated_names "$E1/.claude/settings.local.json" "$FIXTURE")"
CLAUDE_CONFIG_DIR="$E1/fake-config" bash "$E1/scripts/hooks/install.sh" >"$E1/install.out" 2>&1
INSTALL_RC=$?
SEATED_AFTER="$(seated_names "$E1/.claude/settings.local.json" "$FIXTURE")"
if [ "$INSTALL_RC" -eq 0 ] && [ "$SEATED_BEFORE" = "0" ] && [ "$SEATED_AFTER" = "1" ]; then
    ok "14a install.sh seats a newly registered hook — one edit, in hooks.json, and no hand-edit of the settings surface"
else
    bad "14a install.sh seats a newly registered hook" \
        "(rc=$INSTALL_RC before=$SEATED_BEFORE after=$SEATED_AFTER: $(tail -2 "$E1/install.out" | tr '\n' ' '))"
fi

# --- 14b. NEGATIVE CONTROL -------------------------------------------------
# 14a would pass identically if the seated surface named the fixture for some
# reason of its own. The same mutation WITHOUT install.sh must leave the
# surfaces disagreeing, which is also the exact red case 11 used to produce.
E2="$(make_engine_sandbox)"
seed_fixture_registration "$E2/hooks/hooks.json" "bash \${CLAUDE_PLUGIN_ROOT}/scripts/hooks/$FIXTURE"
if [ "$(seated_names "$E2/.claude/settings.local.json" "$FIXTURE")" = "0" ]; then
    ok "14b without install.sh the same registration is NOT seated — 14a's pass is caused by the generator"
else
    bad "14b negative control" "the fixture was seated with no generator run — 14a proves nothing"
fi

# --- 14c. FAIL LOUD, NEVER GUESS ------------------------------------------
# A command that does not carry the plugin-root placeholder has a shape the
# transform does not know. Seating a guessed translation would wire a hook at
# a path that does not exist, and the seated surface's whole job is to be an
# accurate description of the registration. install.sh must refuse and must
# leave the file untouched — asserted by sha256, not by eye.
E3="$(make_engine_sandbox)"
seed_fixture_registration "$E3/hooks/hooks.json" "bash /somewhere/else/$FIXTURE"
SUM_BEFORE="$(shasum -a 256 <"$E3/.claude/settings.local.json")"
CLAUDE_CONFIG_DIR="$E3/fake-config" bash "$E3/scripts/hooks/install.sh" >"$E3/install.out" 2>&1
RC3=$?
SUM_AFTER="$(shasum -a 256 <"$E3/.claude/settings.local.json")"
if [ "$RC3" -ne 0 ] && [ "$SUM_BEFORE" = "$SUM_AFTER" ] \
   && grep -q "do not carry the plugin-root placeholder" "$E3/install.out"; then
    ok "14c an untranslatable registration is REFUSED by name and the seated surface is left byte-identical"
else
    bad "14c untranslatable registration refused" \
        "(rc=$RC3 changed=$([ "$SUM_BEFORE" = "$SUM_AFTER" ] && echo no || echo yes))"
fi

echo ""
if [ "$FAIL" -eq 0 ]; then
    echo "hook-staleness: $PASS/$PASS cases pass"
    exit 0
fi
echo "hook-staleness: $PASS passed, $FAIL FAILED:" >&2
for n in "${FAIL_NAMES[@]}"; do printf '    - %s\n' "$n" >&2; done
exit 1
