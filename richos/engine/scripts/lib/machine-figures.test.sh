#!/usr/bin/env bash
#
# machine-figures.test.sh: the publication check for one machine's own figures in public code
# (scripts/lib/machine-figures.py, run by guard-publication-commits.sh at `git commit` in a
# repository that declares a publication boundary).
#
# A reviewed branch put one Mac's measurements into the public repository: a comment with its
# wired-memory figures and test fixtures named for "this Mac" and a nightly, carrying its total
# RAM, its free memory and its private memory floor. Nothing looked for a machine's own values.
#
# EVERY VALUE BELOW IS READ AT RUN TIME (this host's RAM from sysctl) OR SYNTHETIC. None is typed.
#
#   M1  an added fixture `total_mb=<this host's RAM in MB>` is refused; the same fixture with a
#       synthetic value passes (the CEO's test)
#   M2  the host's RAM in bytes and as "<N> GB" are refused; the MB value with no memory word
#       on the line passes (a number that happens to match is not a figure)
#   M3  a value from the private limits file (RICHOS_LIMITS_FILE) beside a memory word is
#       refused; another value passes; no limits file is not an error
#   M4  incident labels: "nightly NN" in a comment and in a test file, the this_mac fixture name,
#       "this Mac" beside a measurement in a comment, all refused
#   M5  kept: a user-facing string "only a restart of this Mac does", a comment with no figure
#       ("changes this Mac's display"), a test asserting that user-facing string, a document
#       (docs/, .md) with a nightly number
#   M6  only ADDED lines: a line already in HEAD is not judged again
#   M7  the output names path:line and the rule; an unreadable blob is BROKEN, not CLEAN
#
# Usage: scripts/lib/machine-figures.test.sh

set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY="$HERE/machine-figures.py"
PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2" | head -6; FAIL=$((FAIL + 1)); return 0; }
[ -f "$PY" ] || { echo "FATAL: missing $PY" >&2; exit 2; }

RAM_BYTES="$(/usr/sbin/sysctl -n hw.memsize 2>/dev/null || true)"
if ! printf '%s' "$RAM_BYTES" | grep -qE '^[0-9]+$'; then
    RAM_BYTES="$(awk '/^MemTotal:/ {print $2 * 1024}' /proc/meminfo 2>/dev/null || true)"
fi
printf '%s' "$RAM_BYTES" | grep -qE '^[0-9]+$' || { echo "FATAL: this host's RAM size could not be read" >&2; exit 2; }
RAM_MB=$((RAM_BYTES / 1048576))
RAM_GB=$((RAM_BYTES / 1073741824))
OTHER_MB=$((RAM_MB + 1536))   # synthetic: never this host's size

SB="$(mktemp -d "${TMPDIR:-/tmp}/machine-figures-test.XXXXXX")"
trap 'rm -rf "$SB"' EXIT
REPO="$SB/repo"
mkdir -p "$REPO/src" "$REPO/tests" "$REPO/docs"
git -C "$REPO" init -q
printf 'old line # nightly 80 kept from before\n' > "$REPO/src/old.py"
git -C "$REPO" add -A
git -C "$REPO" -c user.email=t@example.com -c user.name=T commit -qm base --no-verify
LIMITS="$SB/limits.env"
printf '# synthetic private limits\nFIXTURE_FLOOR_MB=3172\n' > "$LIMITS"
export RICHOS_LIMITS_FILE="$LIMITS"

# scan <path in repo> <content> -> the scanner's output for one staged file
scan() {
    mkdir -p "$(dirname "$REPO/$1")"
    printf '%s\n' "$2" > "$REPO/$1"
    printf '%s\t%s\n' "$1" "$REPO/$1" > "$SB/manifest"
    python3 -B "$PY" --scan-manifest "$SB/manifest" --repo "$REPO"
}
expect() { # <FOUND|CLEAN> <id> <description> <path> <content>
    local out; out="$(scan "$4" "$5")"
    if [ "$(printf '%s' "$out" | sed -n 1p)" = "$1" ]; then ok "$2 $3"; else bad "$2 $3" "$out"; fi
    LAST="$out"
}

echo "=== machine-figures: refused ==="
expect FOUND M1 "a fixture total_mb=<this host's RAM> is refused" tests/guest_memory.test.py "    total_mb=$RAM_MB, free_mb=4000,"
if printf '%s' "$LAST" | grep -qF "tests/guest_memory.test.py:1"; then ok "M7 the finding names path:line"; else bad "M7 the finding names path:line" "$LAST"; fi
if printf '%s' "$LAST" | grep -qF "RAM size in MB"; then ok "M7 the finding names the rule"; else bad "M7 the finding names the rule" "$LAST"; fi
expect FOUND M2 "the host's RAM in bytes" src/probe.rs "const FIXTURE: u64 = $RAM_BYTES;"
expect FOUND M2 "the host's RAM as <N> GB" src/notes.py "# a ${RAM_GB} GB host"
expect FOUND M3 "a private limit beside a memory word" tests/floor.test.py "floor_mb = 3172"
expect FOUND M4 "nightly NN in a comment" src/reserve.py "# nightly 80 showed the guest refused"
expect FOUND M4 "nightly_NN in a test fixture name" tests/slots.test.py "def test_nightly_80_guest_refused():"
expect FOUND M4 "the this_mac fixture name" tests/slots.test.py "def test_floor_for_this_mac():"
expect FOUND M4 "this Mac beside a measurement in a comment" src/reserve.py "# this Mac: 12.57 GiB wired"

echo "=== machine-figures: kept ==="
expect CLEAN M1 "the same fixture with a synthetic value passes" tests/guest_memory.test.py "    total_mb=$OTHER_MB, free_mb=4000,"
expect CLEAN M2 "the MB number with no memory word" src/table.py "ROWS = [$RAM_MB, 7, 9]"
expect CLEAN M3 "another value beside a memory word" tests/floor.test.py "floor_mb = 3000"
expect CLEAN M5 "a user-facing string about the user's Mac" src/notice.rs 'let text = "The memory is held by macOS; only a restart of this Mac does that.";'
expect CLEAN M5 "a comment with no figure" src/guard.sh "# refuses what changes this Mac's display or sleep"
expect CLEAN M5 "a test asserting the user-facing string" tests/notice.test.js 'assert.match(text, "only a restart of this Mac does that, 100%")'
expect CLEAN M5 "a document with a nightly number" docs/notes.md "nightly 80 failed its VM proof"
expect CLEAN M6 "a line already in HEAD" src/old.py "old line # nightly 80 kept from before"
RICHOS_LIMITS_FILE="$SB/no-such-file" expect CLEAN M3 "no limits file is not an error" tests/floor.test.py "floor_mb = 3172"

echo "=== unreadable ==="
printf 'src/gone.py\t%s\n' "$SB/no-such-blob" > "$SB/manifest"
out="$(python3 -B "$PY" --scan-manifest "$SB/manifest" --repo "$REPO")"
if [ "$(printf '%s' "$out" | sed -n 1p | cut -f1)" = "BROKEN" ]; then ok "M7 an unreadable blob is BROKEN, not CLEAN"; else bad "M7 an unreadable blob is BROKEN, not CLEAN" "$out"; fi

printf 'machine-figures: %d passed, %d FAILED\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
