#!/usr/bin/env bash
#
# guard-bash-main-writes.test.sh — regression tests for the Bash-write guard.
# Feeds synthetic PreToolUse[Bash] JSON to the hook and asserts exit codes.
# Protected trees are read from orchestration.config so the fixtures track
# whatever an adopter configures. Run directly:
#   scripts/hooks/guard-bash-main-writes.test.sh
#
# Exit 0 = all cases pass; exit 1 = at least one failure.
#
# DETERMINISM NOTE: the hook resolves REPO_ROOT to the TRUE main checkout (via
# scripts/lib/resolve-main-checkout.sh). To make the suite independent of where
# it is run (main checkout OR a linked worktree whose path contains
# "/.claude/worktrees/"), it builds a synthetic temp root, seeds it with an
# orchestration.config carrying the SAME PROTECTED_PATHS the adopter configured,
# copies the hook + resolver lib in, and drives every case against paths under
# that temp root. The temp root is a plain (non-git) tmpdir, so the resolver
# takes its no-git fallback = the temp root.

set -uo pipefail

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_HOOK="$SRC_DIR/guard-bash-main-writes.sh"
ENGINE_ROOT="$(cd "$SRC_DIR/../.." && pwd)"

# Read the adopter's protected trees from the real config.
CONFIG="$ENGINE_ROOT/orchestration.config"
# shellcheck disable=SC1090
[ -f "$CONFIG" ] && . "$CONFIG"
: "${PROTECTED_PATHS:=}"

# Build the synthetic main checkout.
TMPROOT="$(mktemp -d -t guard-bash-main-writes.XXXXXX)"
trap 'rm -rf "$TMPROOT"' EXIT
mkdir -p "$TMPROOT/scripts/hooks" "$TMPROOT/scripts/lib"
cp "$SRC_HOOK" "$TMPROOT/scripts/hooks/guard-bash-main-writes.sh"
chmod +x "$TMPROOT/scripts/hooks/guard-bash-main-writes.sh"
cp "$SRC_DIR/../lib/resolve-main-checkout.sh" "$TMPROOT/scripts/lib/" 2>/dev/null || true
# The hook's bootstrap resolves its library relative to its OWN location, so a
# sandbox hosting a copy of the hook must host the library too — otherwise the
# hook correctly refuses to start ("BROKEN INSTALL") and every case below would
# fail for that reason rather than the one under test.
cp "$SRC_DIR/../lib/resolve-roots.sh" "$TMPROOT/scripts/lib/"
cp "$SRC_DIR/../lib/unguarded_rm.py" "$TMPROOT/scripts/lib/"
cp "$SRC_DIR/../lib/host_sleep.py" "$TMPROOT/scripts/lib/"
printf 'PROTECTED_PATHS="%s"\n' "$PROTECTED_PATHS" > "$TMPROOT/orchestration.config"
HOOK="$TMPROOT/scripts/hooks/guard-bash-main-writes.sh"
ROOT="$TMPROOT"
# Declare the synthetic checkout as the governed root. Without this the hook
# would resolve the LAUNCHING session's repository, which is not what any case
# below is about.
RICHOS_ENTITY_ROOT="$TMPROOT"
export RICHOS_ENTITY_ROOT
WT="$ROOT/.claude/worktrees/agent-x"

set -- $PROTECTED_PATHS
FIRST_PROTECTED="${1:-src}"

PASS=0
FAIL=0

# run_case <name> <expected-exit> <json>
run_case() {
    local name="$1" expected="$2" json="$3"
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

# JSON builders (python3 does the escaping so command strings stay literal).
json_cmd() { # <command> [cwd]
    python3 -c '
import json, sys
d = {"tool_name": "Bash", "tool_input": {"command": sys.argv[1]}}
if len(sys.argv) > 2 and sys.argv[2]:
    d["cwd"] = sys.argv[2]
print(json.dumps(d))
' "$1" "${2:-}"
}

echo "=== guard-bash-main-writes tests ==="
echo "  (protected trees from orchestration.config: ${PROTECTED_PATHS:-<none>})"

# --- BLOCKED: writes into a protected tree in the main checkout ---
if [ -z "${PROTECTED_PATHS// /}" ]; then
    printf '  FAIL  no PROTECTED_PATHS configured — cannot exercise the block path\n'
    FAIL=$((FAIL + 1))
else
    for p in $PROTECTED_PATHS; do
        run_case "abs-path mkdir into main $p"      2 "$(json_cmd "mkdir -p $ROOT/$p/module/x")"
        run_case "cd-compound + relative $p write"  2 "$(json_cmd "cd $ROOT && mkdir $p/foo")"
        run_case "cwd-main + relative $p write"     2 "$(json_cmd "rm $p/foo" "$ROOT")"
    done
    run_case "abs redirect into main $FIRST_PROTECTED" 2 "$(json_cmd "echo hi > $ROOT/$FIRST_PROTECTED/z")"
    run_case "cwd-empty + relative $FIRST_PROTECTED tee" 2 "$(json_cmd "tee $FIRST_PROTECTED/x <<<hi")"
fi

# --- ALLOWED: worktree-scoped writes ---
run_case "abs write inside a worktree"          0 "$(json_cmd "mkdir -p $WT/$FIRST_PROTECTED/module/x")"
run_case "cwd=worktree + relative write"        0 "$(json_cmd "mkdir -p $FIRST_PROTECTED/foo" "$WT")"
run_case "cd-into-worktree then relative write" 0 "$(json_cmd "cd $WT && mkdir $FIRST_PROTECTED/foo" "$ROOT")"

# --- ALLOWED: reads / git ops / non-write commands ---
run_case "read protected (cat, no write token)" 0 "$(json_cmd "cat $FIRST_PROTECTED/foo" "$ROOT")"
run_case "git add protected (no write token)"   0 "$(json_cmd "git add $FIRST_PROTECTED/foo" "$ROOT")"
run_case "git commit"                           0 "$(json_cmd "git commit -m wip" "$ROOT")"

# --- ALLOWED: the false positives the target-anchoring closed ---
#
# These are all READS. Under the old co-occurrence rule (a write-ish token
# ANYWHERE on the line + a protected path ANYWHERE on the line) four of the
# five were BLOCKED — measured by reverting this guard to the co-occurrence
# version and re-running this suite, which turns those four cases red while
# the three "still blocks" arms below stay green.
#
# The FIRST case is the exception and is labelled honestly: `ls … && ls *.csv`
# passes under BOTH versions, because the old write-token regex never matched
# `ls`. It is kept as a regression pin, not as evidence of the fix.
run_case "read: listing + unrelated glob (pin)" 0 "$(json_cmd "ls $FIRST_PROTECTED/ && ls *.csv" "$ROOT")"
run_case "read: interpreter with 2>&1 tail"     0 "$(json_cmd "python3 $FIRST_PROTECTED/gate.py --check 2>&1 | head -40" "$ROOT")"
run_case "read: stderr merge on a cat"          0 "$(json_cmd "cat $FIRST_PROTECTED/x.py 2>&1" "$ROOT")"
run_case "read: >&2 fd-dup, protected mention"  0 "$(json_cmd "echo checking $FIRST_PROTECTED/x.py >&2" "$ROOT")"
run_case "unrelated write clause + protected read" 0 "$(json_cmd "mkdir -p /tmp/scratch && cat $FIRST_PROTECTED/x.py" "$ROOT")"
run_case "read: sed -n on a protected file"     0 "$(json_cmd "sed -n '1,20p' $FIRST_PROTECTED/x.py" "$ROOT")"
run_case "read: git show a protected blob"      0 "$(json_cmd "git show HEAD:$FIRST_PROTECTED/x.py" "$ROOT")"
run_case "read: protected grep -> unprotected redirect" 0 "$(json_cmd "grep -rn foo $FIRST_PROTECTED/ > /tmp/guard-results.txt" "$ROOT")"

# --- NEGATIVE ARMS for the five above: the same shapes with a REAL target ---
# Without these, "the anchoring works" would also be satisfied by a guard that
# stopped blocking anything at all.
run_case "still blocks: real write after a read"  2 "$(json_cmd "ls $FIRST_PROTECTED/ && mkdir $FIRST_PROTECTED/new" "$ROOT")"
run_case "still blocks: redirect into protected"  2 "$(json_cmd "python3 gen.py 2>&1 > $FIRST_PROTECTED/out.txt" "$ROOT")"
run_case "still blocks: unrelated read + real write" 2 "$(json_cmd "cat /tmp/x && touch $FIRST_PROTECTED/y" "$ROOT")"

# --- Hunt part 3 v3, finding 39: a write verb that is only PRINTED or sits in a
# COMMENT writes nothing; the verb must be the command word of a statement.
run_case "P3-39: printf of a touch command is not a write" 0 "$(json_cmd "printf \"%s\\n\" touch $ROOT/$FIRST_PROTECTED/file" "$ROOT")"
run_case "P3-39: a touch in a comment is not a write"       0 "$(json_cmd "true # touch $ROOT/$FIRST_PROTECTED/file" "$ROOT")"
run_case "P3-39: echo naming rm and a protected path"       0 "$(json_cmd "echo rm -rf $FIRST_PROTECTED/x" "$ROOT")"
# The same verbs as real command words still block, through every wrapper.
run_case "P3-39 still blocks: write on the line after a comment" 2 "$(json_cmd "true # a note
touch $ROOT/$FIRST_PROTECTED/file" "$ROOT")"
run_case "P3-39 still blocks: sudo rm"                 2 "$(json_cmd "sudo rm $ROOT/$FIRST_PROTECTED/x" "$ROOT")"
run_case "P3-39 still blocks: env VAR=1 touch"         2 "$(json_cmd "env A=1 touch $ROOT/$FIRST_PROTECTED/x" "$ROOT")"
run_case "P3-39 still blocks: xargs -I {} rm"          2 "$(json_cmd "ls | xargs -I {} rm $ROOT/$FIRST_PROTECTED/{}" "$ROOT")"
run_case "P3-39 still blocks: rm inside a for loop"    2 "$(json_cmd "for d in a b; do rm $ROOT/$FIRST_PROTECTED/\$d; done" "$ROOT")"
run_case "P3-39 still blocks: touch in \$(...)"        2 "$(json_cmd "echo \$(touch $ROOT/$FIRST_PROTECTED/x)" "$ROOT")"
run_case "P3-39 still blocks: git rm of a protected path" 2 "$(json_cmd "git rm $FIRST_PROTECTED/x" "$ROOT")"
run_case "P3-39 still blocks: unclosed quote falls back"  2 "$(json_cmd "touch $ROOT/$FIRST_PROTECTED/x; echo it's" "$ROOT")"

# --- Hunt part 3 v3, finding 41: a relative write made FROM INSIDE a protected
# directory of the main checkout lands in it, the same as its absolute spelling.
run_case "P3-41: cwd inside protected + touch file"        2 "$(json_cmd "touch file" "$ROOT/$FIRST_PROTECTED")"
run_case "P3-41: cwd deeper inside protected + redirect"    2 "$(json_cmd "echo hi > out.txt" "$ROOT/$FIRST_PROTECTED/sub")"
run_case "P3-41 allowed: read from inside protected"        0 "$(json_cmd "cat file" "$ROOT/$FIRST_PROTECTED")"
run_case "P3-41 allowed: absolute scratch write from inside" 0 "$(json_cmd "rm -rf /tmp/scratch/x" "$ROOT/$FIRST_PROTECTED")"
run_case "P3-41 allowed: cd to scratch first"               0 "$(json_cmd "cd /tmp/scratch && touch file" "$ROOT/$FIRST_PROTECTED")"

# --- cp/rsync only READ their sources: the destination alone decides ---
run_case "cp OUT of protected abs into scratch"  0 "$(json_cmd "cp $ROOT/$FIRST_PROTECTED/a /tmp/scratch/a" "$ROOT")"
run_case "cp OUT of protected rel into scratch"  0 "$(json_cmd "cp -r $FIRST_PROTECTED/a /tmp/scratch/a" "$ROOT")"
run_case "rsync OUT of protected into scratch"   0 "$(json_cmd "rsync -a $ROOT/$FIRST_PROTECTED/ /tmp/scratch/" "$ROOT")"
run_case "cp -t scratch from protected"          0 "$(json_cmd "cp -t /tmp/scratch $ROOT/$FIRST_PROTECTED/a" "$ROOT")"
run_case "still blocks: cp INTO protected abs"   2 "$(json_cmd "cp /tmp/a $ROOT/$FIRST_PROTECTED/a" "$ROOT")"
run_case "still blocks: cp -t protected"         2 "$(json_cmd "cp -t $ROOT/$FIRST_PROTECTED /tmp/a" "$ROOT")"
run_case "still blocks: rsync INTO protected"    2 "$(json_cmd "rsync -a /tmp/scratch/ $ROOT/$FIRST_PROTECTED/" "$ROOT")"
run_case "P3-33: rsync --remove-source-files OUT of protected is blocked (deletes source)" 2 "$(json_cmd "rsync -a --remove-source-files $ROOT/$FIRST_PROTECTED/ /tmp/scratch/" "$ROOT")"
run_case "P3-33 v3: a QUOTED --remove-source-files is the same flag and is blocked" 2 "$(json_cmd "rsync '--remove-source-files' $ROOT/$FIRST_PROTECTED/a /tmp/scratch/" "$ROOT")"
run_case "still blocks: mv OUT of protected (removes source)" 2 "$(json_cmd "mv $ROOT/$FIRST_PROTECTED/a /tmp/scratch/a" "$ROOT")"

# --- ALLOWED: scratchpad + non-source writes in main ---
run_case "scratchpad write"                     0 "$(json_cmd "mkdir -p /tmp/x && echo hi > /tmp/x/y" "$ROOT")"
run_case "docs write in main (not source)"      0 "$(json_cmd "echo x > $ROOT/docs/foo.md")"
run_case "scripts write in main (not source)"   0 "$(json_cmd "mkdir -p $ROOT/scripts/gen")"

# --- ALLOWED: non-Bash payloads pass straight through ---
run_case "non-Bash tool (Write)"                0 '{"tool_name":"Write","tool_input":{"file_path":"'"$ROOT/$FIRST_PROTECTED"'/x"}}'
run_case "malformed JSON"                        0 'this is not json'
run_case "empty command"                         0 "$(json_cmd "" "$ROOT")"

# --- POSITIVE-SHAPE probe -------------------------------------------------
# A PASS case could pass because the guard silently no-ops. Prove the guard is
# actually evaluating: take a genuine PASS command (scratchpad write) and swap
# only the target into a protected tree — it MUST flip from allow to block.
run_case "positive-shape: scratchpad->protected flips to BLOCK" 2 \
    "$(json_cmd "mkdir -p /tmp/x && echo hi > $ROOT/$FIRST_PROTECTED/y" "$ROOT")"

# --- PROTECTED_PATHS empty -> guard INACTIVE (allow + loud note) ----------
# The generic engine's sensible-failure contract: unconfigured -> visible no-op.
EMPTY_ROOT="$(mktemp -d -t guard-bash-empty.XXXXXX)"
mkdir -p "$EMPTY_ROOT/scripts/hooks" "$EMPTY_ROOT/scripts/lib"
cp "$SRC_HOOK" "$EMPTY_ROOT/scripts/hooks/guard-bash-main-writes.sh"
chmod +x "$EMPTY_ROOT/scripts/hooks/guard-bash-main-writes.sh"
cp "$SRC_DIR/../lib/resolve-main-checkout.sh" "$EMPTY_ROOT/scripts/lib/" 2>/dev/null || true
cp "$SRC_DIR/../lib/resolve-roots.sh" "$EMPTY_ROOT/scripts/lib/"
printf 'PROTECTED_PATHS=""\n' > "$EMPTY_ROOT/orchestration.config"
EMPTY_OUT="$(printf '{"tool_name":"Bash","tool_input":{"command":"cd %s && mkdir src/foo"}}' "$EMPTY_ROOT" | RICHOS_ENTITY_ROOT="$EMPTY_ROOT" "$EMPTY_ROOT/scripts/hooks/guard-bash-main-writes.sh" 2>&1)"
EMPTY_RC=$?
if [ "$EMPTY_RC" -eq 0 ]; then
    PASS=$((PASS + 1)); printf '  PASS  empty PROTECTED_PATHS -> guard inactive (exit 0)\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  empty PROTECTED_PATHS -> expected exit 0, got %s\n' "$EMPTY_RC"
fi
if printf '%s' "$EMPTY_OUT" | grep -qF 'INACTIVE'; then
    PASS=$((PASS + 1)); printf '  PASS  empty PROTECTED_PATHS -> loud INACTIVE note on stderr\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  empty PROTECTED_PATHS -> no INACTIVE note (%s)\n' "$EMPTY_OUT"
fi
rm -rf "$EMPTY_ROOT"

# --- python3 missing from PATH -> BLOCKED (fail-closed), loud stderr ---
make_fakebin_no_python3() {
    local dir
    dir="$(mktemp -d -t fakebin-no-python3.XXXXXX)"
    local tools="cat grep sed cut tr date mkdir git mktemp basename dirname rm ln awk sort uniq wc head tail shasum sha256sum env cp chmod"
    local t p
    for t in $tools; do
        p="$(command -v "$t" 2>/dev/null || true)"
        [ -n "$p" ] && ln -sf "$p" "$dir/$t"
    done
    echo "$dir"
}
BASH_BIN="$(command -v bash)"
FAKEBIN="$(make_fakebin_no_python3)"
NOPY_JSON="$(printf '{"tool_name":"Bash","tool_input":{"command":"cd %s && mkdir %s/foo"}}' "$ROOT" "$FIRST_PROTECTED")"
NOPY_OUT="$(printf '%s' "$NOPY_JSON" | PATH="$FAKEBIN" "$BASH_BIN" "$HOOK" 2>&1 1>/dev/null)"
NOPY_RC=$?
if [ "$NOPY_RC" -ne 0 ]; then
    PASS=$((PASS + 1)); printf '  PASS  python3 missing from PATH -> refuses (exit %s, fail-closed)\n' "$NOPY_RC"
else
    FAIL=$((FAIL + 1)); printf '  FAIL  python3 missing from PATH -> expected non-zero exit, got 0 (FAIL-OPEN)\n'
fi
if printf '%s' "$NOPY_OUT" | grep -qF 'python3'; then
    PASS=$((PASS + 1)); printf '  PASS  python3-missing stderr names the missing interpreter\n'
else
    FAIL=$((FAIL + 1)); printf '  FAIL  python3-missing stderr did not mention python3 (%s)\n' "$NOPY_OUT"
fi
rm -rf "$FAKEBIN"

echo ""
if [ "$FAIL" -gt 0 ]; then
    echo "=== guard-bash-main-writes tests: $FAIL FAILED, $PASS passed ==="
    exit 1
else
    echo "=== guard-bash-main-writes tests: all $PASS passed ==="
    exit 0
fi
