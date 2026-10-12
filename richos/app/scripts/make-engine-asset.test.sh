#!/usr/bin/env bash
#
# make-engine-asset.test.sh — the engine release asset carries its own license.
#
# THE DEFECT THIS EXISTS FOR, found by the 2026-09-04 pre-publication audit.
# `make-engine-asset.sh` archives the `engine/` directory. The canonical AGPL text is at
# the REPOSITORY root, one level above it. So the standalone engine tarball — the thing a
# customer's first run downloads from Releases, extracts, and installs — contained no
# license text of any kind. An AGPL work distributed without its terms, to the one reader
# who has no repository to go and look in.
#
# Three bundled skills had a second version of the same defect: `frontend-design/SKILL.md`
# said "Complete terms in LICENSE.txt" with no such file anywhere, and the vendored
# `marketing-strategy-pmm` and `resend` skills declared MIT with no MIT notice beside them.
# A claim pointing at nothing is worse than no claim, because it reads as diligence.
#
# WHAT IS CHECKED HERE AND WHAT IS CHECKED BY THE SCRIPT ITSELF. The script refuses at
# build time; this suite proves the refusals actually fire, using a synthetic repository
# it builds from scratch so a negative case is a real refusal rather than an assertion
# about one. The last three cases run against the REAL tree, because a fixture proving a
# refusal works says nothing about whether this repository would pass it.
#
# ONE BRANCH IS DELIBERATELY NOT COVERED, and saying so is better than a case that pretends.
# The script compares the LICENSE it read out of the finished archive against the canonical
# root file. Today those cannot differ: the same `cp` produced both. That comparison is a
# guard against a FUTURE packaging change — a filter, a transform, an exclusion list — that
# would otherwise ship a mangled license and exit 0. There is no way to trigger it from
# outside the script as written, so no case here claims to. The same is true of the
# "packaging dropped a license file" branch.
#
# Cases:
#   L1  no root LICENSE            → REFUSES, and names the file
#   L2  no third-party notices     → REFUSES, and names the file
#   L3  a complete fixture builds at all
#   L4  the archive contains engine/LICENSE
#   L5  ...byte-identical to that repository's root LICENSE
#   L6  the archive contains engine/THIRD-PARTY-NOTICES.md
#   L7  a bundled skill's own license file survives into the archive
#   L8  a skill DECLARING a license with no license file beside it → REFUSES
#   L9  the real repository has the root LICENSE, at the canonical sha256
#   L10 the real repository has docs/legal/THIRD-PARTY-NOTICES.md
#   L11 every real bundled directory that has a license file still has one
#   L12 the real archive builds, is deterministic, and carries the canonical LICENSE
#   L13 the digest survives a different umask and a different timezone
#   L14 no member of the real archive is a file git does not track
#   L15 an ignored file in the source engine does not reach the archive
#   L16 an archive carrying an untracked member is REFUSED, by name
#   L17 a missing members check is REFUSED, not skipped
#   L18 building outside a git checkout is REFUSED
#   L23 an ignored LICENSE in the source engine is not required in the archive
#
# THE SECOND DEFECT, found 2026-09-04 by the two cases added that morning failing on main
# after passing on their author's fresh worktree. The script copied the engine WORKING TREE,
# so all 119 gitignored files in `engine/` went into the customer's download: fifteen
# `.claude/state/agent-definitions-*.snapshot` carrying SESSION UUIDs and the operator's home
# path, 57 `scripts/hooks/*.sha256` sidecars, `__pycache__` bytecode. A fresh worktree has
# none of that, which is exactly why it looked clean there.
#
# L14–L18 exist because L12 and L13 CANNOT be trusted with this. They caught it by accident:
# a new session minted a new snapshot between two builds, so the digest moved. Measured here
# before the fix — with 205 ignored files planted and then left alone, all thirteen cases
# reported green while 208 untracked members sat inside the archive. A determinism check asks
# whether the bytes are the same twice. Only a CONTENT check asks what the bytes are.
#
# WHAT THIS SUITE READS, declared for `run-tests.sh --skip-unchanged`. Measured at 128.7 s
# of a 950 s build (2026-09-19), and every second of it is L9-L19 building the REAL archive
# out of the REAL engine tree — twice — and comparing it against the root LICENSE and the
# third-party notices. So its inputs are everything that can end up inside that archive,
# plus this directory, which holds the script it drives and the members check that script
# calls. A DELIBERATE SUPERSET: `richos/app/scripts` is taken whole rather than the three
# files named, because a helper moving between them must not be invisible to this line.
# The line lives here, not in run-tests.sh, because a suite is the only thing that knows
# what it reads, and a second place to edit is the defect run-tests.sh counts five of.
# run-tests: inputs richos/app/scripts richos/engine LICENSE docs/legal
# run-tests: covers richos/app/scripts/lib/no_host_paths.py richos/app/scripts/make-engine-asset.sh richos/app/scripts/verify-engine-asset-members.sh
set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="$(cd "$DIR/.." && pwd)"
ROOT="$(cd "$APP/../.." && pwd)"
SCRIPT="$DIR/make-engine-asset.sh"
MEMBERS="$DIR/verify-engine-asset-members.sh"

# The canonical AGPL v3 digest. Written here as a literal on purpose: if the root LICENSE
# is ever edited, this suite is one of the places that says so out loud. See
# docs/legal/LICENSING.md.
CANON_SHA="0d96a4ff68ad6d4b6f1f30f713b18d5184912ba8dd389f86aa7710db079abcb0"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "${2:-}"; FAIL=$((FAIL + 1)); }

[ -f "$SCRIPT" ]  || { echo "make-engine-asset.test.sh: no $SCRIPT — refusing to report a result." >&2; exit 2; }
[ -f "$MEMBERS" ] || { echo "make-engine-asset.test.sh: no $MEMBERS — refusing to report a result." >&2; exit 2; }

# Real archive cases use the same verified delivery as a release. Synthetic
# version-1.0.0 cases below deliberately clear this variable.
if [ -z "${RICHOS_RUNTIME_DIR:-}" ]; then
    echo 'Prerequisite: set RICHOS_RUNTIME_DIR to a prepared, verified runtime before running make-engine-asset.test.sh.' >&2
    exit 2
fi
if ! python3 "$DIR/verify-runtime.py" "$RICHOS_RUNTIME_DIR" "$DIR/runtime-sources.json" >/dev/null; then
    echo 'Prerequisite: RICHOS_RUNTIME_DIR must match the tracked runtime recipe. No runtime was downloaded.' >&2
    exit 2
fi

WORK="$(mktemp -d "${TMPDIR:-/tmp}/engine-asset-test.XXXXXX")" || exit 2
trap 'rm -rf "$WORK"' EXIT

# ---------------------------------------------------------------------------------------
# A synthetic repository, shaped like this one and nothing like this one's size.
# ---------------------------------------------------------------------------------------
# $1 = destination root. Builds a minimal but COMPLETE tree: the script's own preconditions
# are all satisfied, so any refusal in a later case is caused by what that case removed.
#
# THE FIXTURE IS A REAL CHECKOUT, and it has to be. The script builds the asset from
# `git ls-files` rather than from what is on disk, so a fixture that is merely a directory
# would exercise nothing but the refusal in L18. `fixture_commit` is separate because three
# cases remove a file and then need the index to say so.
#
# AND IT CARRIES AN IGNORED FILE, deliberately, shaped like the one that caused this: a
# session snapshot under `.claude/state/`. Every fixture build therefore happens with an
# ignored file sitting in the source engine, which is the condition a real checkout is always
# in and a fresh worktree never is.
fixture_commit() {
    local r="$1"
    env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE \
        git -C "$r" -c core.excludesFile=/dev/null add -A >/dev/null 2>&1 || return 1
    env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE \
        git -C "$r" -c user.name=fixture -c user.email=fixture@example.invalid \
            commit -q --no-gpg-sign --allow-empty -m 'fixture' >/dev/null 2>&1
}

make_fixture() {
    local r="$1"
    mkdir -p "$r/richos/app/scripts/lib" "$r/docs/legal" \
             "$r/richos/engine/scripts/hooks" "$r/richos/engine/skills/demo-skill" "$r/richos/engine/tools/demo-tool"
    cp "$SCRIPT" "$r/richos/app/scripts/make-engine-asset.sh"
    cp "$MEMBERS" "$r/richos/app/scripts/verify-engine-asset-members.sh"
    cp "$DIR/lib/no_host_paths.py" "$r/richos/app/scripts/lib/no_host_paths.py"
    cp "$DIR/runtime-sources.json" "$r/richos/app/scripts/runtime-sources.json"
    # The lock the script holds on its output directory (output-lock.test.sh is its suite).
    cp "$DIR/lib/output-lock.sh" "$r/richos/app/scripts/lib/output-lock.sh"
    cp "$ROOT/LICENSE" "$r/LICENSE"
    printf 'notices for the fixture\n' > "$r/docs/legal/THIRD-PARTY-NOTICES.md"
    printf '1.0.0\n' > "$r/richos/engine/VERSION"
    printf '/.claude/state/\n*.sha256\n' > "$r/richos/engine/.gitignore"
    printf '#!/usr/bin/env bash\ntrue\n' > "$r/richos/engine/scripts/hooks/guard-demo.sh"
    printf -- '---\nname: demo-skill\nlicense: MIT\n---\n\nbody\n' > "$r/richos/engine/skills/demo-skill/SKILL.md"
    printf 'MIT License\n\nCopyright (c) 2026 Somebody Else\n' > "$r/richos/engine/skills/demo-skill/LICENSE"
    printf 'MIT License\n\nCopyright (c) 2026 Somebody Else\n' > "$r/richos/engine/tools/demo-tool/LICENSE"

    env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE \
        git -c init.defaultBranch=main init -q "$r" >/dev/null 2>&1
    fixture_commit "$r"

    # The ignored artifacts, written AFTER the commit so nothing can accidentally track them.
    mkdir -p "$r/richos/engine/.claude/state"
    printf '# session=deadbeef-8e71-46f3-92d9-925de910870e generated=2026-09-04T00:00:00Z root=%s/engine\n' \
        "$r" > "$r/richos/engine/.claude/state/agent-definitions-deadbeef.snapshot"
    printf 'aaaa\n' > "$r/richos/engine/scripts/hooks/guard-demo.sh.sha256"
}

# Runs the fixture's copy of the script. Never touches the real tree.
run_fixture() { RICHOS_RUNTIME_DIR= bash "$1/richos/app/scripts/make-engine-asset.sh" --out "$1/out" 2>&1; }

# ---------------------------------------------------------------------------------------
# L1 / L2 — the two preconditions, each removed on its own
# ---------------------------------------------------------------------------------------
F="$WORK/no-license"; make_fixture "$F"; rm -f "$F/LICENSE"; fixture_commit "$F"
OUT="$(run_fixture "$F")"; CODE=$?
if [ "$CODE" -ne 0 ] && printf '%s' "$OUT" | grep -q 'no LICENSE at'; then
    ok "L1 a repository with no root LICENSE is REFUSED, by name"
else
    bad "L1 a repository with no root LICENSE is REFUSED, by name" "exit $CODE: $(printf '%s' "$OUT" | tail -1)"
fi

F="$WORK/no-notices"; make_fixture "$F"; rm -f "$F/docs/legal/THIRD-PARTY-NOTICES.md"; fixture_commit "$F"
OUT="$(run_fixture "$F")"; CODE=$?
if [ "$CODE" -ne 0 ] && printf '%s' "$OUT" | grep -q 'no third-party notices at'; then
    ok "L2 a repository with no third-party notices is REFUSED, by name"
else
    bad "L2 a repository with no third-party notices is REFUSED, by name" "exit $CODE: $(printf '%s' "$OUT" | tail -1)"
fi

# ---------------------------------------------------------------------------------------
# L3–L7 — the complete fixture, and what its archive actually contains
# ---------------------------------------------------------------------------------------
F="$WORK/complete"; make_fixture "$F"
OUT="$(run_fixture "$F")"; CODE=$?
TARBALL="$F/out/richos-engine-1.0.0.tar.gz"
# Held under their own names because `F` and `TARBALL` are reassigned by later cases, and
# L15/L16 come back to this archive.
F_COMPLETE_ROOT="$F"; F_COMPLETE_TARBALL="$TARBALL"
if [ "$CODE" -eq 0 ] && [ -s "$TARBALL" ]; then
    ok "L3 a complete fixture builds an archive"
else
    bad "L3 a complete fixture builds an archive" "exit $CODE: $(printf '%s' "$OUT" | tail -2)"
fi

X="$F/x"; mkdir -p "$X"
if [ -s "$TARBALL" ]; then
    /usr/bin/tar -x -z --no-same-owner -f "$TARBALL" -C "$X" 2>/dev/null
fi

if [ -f "$X/engine/LICENSE" ]; then
    ok "L4 the archive contains engine/LICENSE"
else
    bad "L4 the archive contains engine/LICENSE" "an AGPL work would ship with no terms"
fi

A="$(/usr/bin/shasum -a 256 "$F/LICENSE" 2>/dev/null | awk '{print $1}')"
B="$(/usr/bin/shasum -a 256 "$X/engine/LICENSE" 2>/dev/null | awk '{print $1}')"
if [ -n "$A" ] && [ "$A" = "$B" ]; then
    ok "L5 the archived LICENSE is byte-identical to the repository's root LICENSE"
else
    bad "L5 the archived LICENSE is byte-identical to the repository's root LICENSE" "$B vs $A"
fi

if [ -f "$X/engine/THIRD-PARTY-NOTICES.md" ]; then
    ok "L6 the archive contains engine/THIRD-PARTY-NOTICES.md"
else
    bad "L6 the archive contains engine/THIRD-PARTY-NOTICES.md" "the asset could not name what it bundles"
fi

if [ -f "$X/engine/skills/demo-skill/LICENSE" ] && [ -f "$X/engine/tools/demo-tool/LICENSE" ]; then
    ok "L7 bundled third-party license files survive into the archive"
else
    bad "L7 bundled third-party license files survive into the archive" "a bundled notice was lost in packaging"
fi

# L7b — the summary counts the files it just verified, without a git diagnostic. The license
# count used to be taken with `git ls-files` on the UNPACKED archive, which has no .git, so it
# printed "0 ... intact" under "fatal: not a git repository".
if printf '%s' "$OUT" | grep -q 'present, 2 bundled license file(s) intact' \
   && ! printf '%s' "$OUT" | grep -qi 'fatal:'; then
    ok "L7b the summary counts the 2 bundled license files, with no git diagnostic"
else
    bad "L7b the summary counts the 2 bundled license files, with no git diagnostic" \
        "$(printf '%s' "$OUT" | grep -iE 'fatal|license file' | head -3 | tr '\n' ' ')"
fi

# ---------------------------------------------------------------------------------------
# L8 — a claim with nothing behind it. THE defect the audit found, three times over.
# ---------------------------------------------------------------------------------------
# The removal is COMMITTED, because the archive is built from the index now: a skill added
# with a license claim and no license text would never have had one tracked in the first place.
F="$WORK/claim-no-text"; make_fixture "$F"; rm -f "$F/richos/engine/skills/demo-skill/LICENSE"; fixture_commit "$F"
OUT="$(run_fixture "$F")"; CODE=$?
if [ "$CODE" -ne 0 ] && printf '%s' "$OUT" | grep -q 'declare a license with no license file'; then
    ok "L8 a skill declaring a license with no license file beside it is REFUSED"
else
    bad "L8 a skill declaring a license with no license file beside it is REFUSED" "exit $CODE: $(printf '%s' "$OUT" | tail -1)"
fi

# ---------------------------------------------------------------------------------------
# Extracted source and binary bytes are checked, independently of app waivers.
F="$WORK/host-volume"; make_fixture "$F"
# machine-path-exempt: negative control that must refuse a shipped machine volume
printf 'ROOT="/Volumes/Fixture/cache"\n' > "$F/richos/engine/active.py"; fixture_commit "$F"
OUT="$(RICHOS_ALLOW_HOST_PATHS=1 run_fixture "$F")"; CODE=$?
if [ "$CODE" -ne 0 ] && printf '%s' "$OUT" | grep -q 'machine-specific storage paths'; then
    ok "L24 extracted engine machine path refuses even with app waiver"
else
    bad "L24 extracted engine path refusal" "exit $CODE: $(printf '%s' "$OUT" | tail -2)"
fi
F="$WORK/reviewed-history"; make_fixture "$F"
# machine-path-exempt: reviewed history fixture preserves its original path as text
printf '# machine-path-exempt: archived verification location\n/Volumes/Fixture/cache\n' > "$F/richos/engine/history.txt"; fixture_commit "$F"
OUT="$(run_fixture "$F")"; CODE=$?
if [ "$CODE" = 0 ]; then ok "L25 reviewed history source text remains publishable"
else bad "L25 reviewed history allowance" "exit $CODE: $(printf '%s' "$OUT" | tail -2)"; fi
F="$WORK/binary-host-volume"; make_fixture "$F"
# machine-path-exempt: negative binary control proves a text marker cannot waive binary bytes
printf 'machine-path-exempt: fixture\0/Volumes/Fixture/cache\0' > "$F/richos/engine/native.bin"; fixture_commit "$F"
OUT="$(run_fixture "$F")"; CODE=$?
if [ "$CODE" -ne 0 ] && printf '%s' "$OUT" | grep -q 'machine-specific storage paths'; then
    ok "L26 binary host path refuses despite text marker"
else bad "L26 binary marker cannot waive host path" "exit $CODE: $(printf '%s' "$OUT" | tail -2)"; fi

# # L9–L12 — the REAL tree. A fixture proving a refusal works says nothing about this one.
# ---------------------------------------------------------------------------------------
REAL_SHA="$(/usr/bin/shasum -a 256 "$ROOT/LICENSE" 2>/dev/null | awk '{print $1}')"
if [ "$REAL_SHA" = "$CANON_SHA" ]; then
    ok "L9 the real root LICENSE is the canonical AGPL v3 text"
else
    bad "L9 the real root LICENSE is the canonical AGPL v3 text" "sha256 $REAL_SHA, expected $CANON_SHA"
fi

if [ -f "$ROOT/docs/legal/THIRD-PARTY-NOTICES.md" ]; then
    ok "L10 the real repository has docs/legal/THIRD-PARTY-NOTICES.md"
else
    bad "L10 the real repository has docs/legal/THIRD-PARTY-NOTICES.md" "packaging would refuse"
fi

# Derived from disk, never typed: every directory under engine/ that carries its own
# license file today. If a vendored skill loses its notice, this count drops and this
# case says so without anyone having maintained a list.
REAL_TP="$( ( cd "$ROOT/richos/engine" && find . -type f \( -name 'LICENSE' -o -name 'LICENSE.txt' -o -name 'LICENSE.md' \) ) | wc -l | tr -d ' ')"
if [ "$REAL_TP" -ge 16 ]; then
    ok "L11 the real engine carries $REAL_TP bundled license files (16 known bundled works)"
else
    bad "L11 the real engine carries at least 7 bundled license files" \
        "found $REAL_TP — a vendored work has lost its notice; see docs/legal/THIRD-PARTY-NOTICES.md"
fi

# Two real builds cover both content and determinism. The --check control flow
# is separately exercised by L20-L22 on the small fixture below.
OUT="$(umask 022; TZ=Asia/Tokyo bash "$SCRIPT" --out "$WORK/real-out" 2>&1)"; CODE=$?
REAL_TARBALL="$(ls "$WORK/real-out"/richos-engine-*.tar.gz 2>/dev/null | head -1)"
RX="$WORK/real-x"; mkdir -p "$RX"
[ -n "$REAL_TARBALL" ] && /usr/bin/tar -x -z --no-same-owner -f "$REAL_TARBALL" -C "$RX" 2>/dev/null
SHIPPED="$(/usr/bin/shasum -a 256 "$RX/engine/LICENSE" 2>/dev/null | awk '{print $1}')"
if [ "$CODE" -eq 0 ] && [ "$SHIPPED" = "$CANON_SHA" ]; then
    ok "L12 the real archive builds and carries the canonical LICENSE"
else
    bad "L12 the real archive builds and carries the canonical LICENSE" \
        "exit $CODE, shipped sha256 ${SHIPPED:-<absent>}: $(printf '%s' "$OUT" | tail -1)"
fi

# L13 — THE ENVIRONMENT, WHICH IS WHERE THIS SCRIPT WAS ACTUALLY WRONG.
#
# `--check` builds twice and compares, and for months that proved nothing: both builds ran
# in the same shell, so neither could fail for any reason the other would not have failed
# for. Two variables the caller controls did move the bytes, and both were found by running
# the script rather than by reading it (2026-09-04):
#
#   umask  macOS bsdtar applies the caller's umask when it extracts as a non-root user, so
#          the staging copy inherited it. All 531 members differed, in mode and nothing else.
#   TZ     `date -u` printed the mtime stamp in UTC and `touch -t` read it back in the LOCAL
#          zone, so the archive's timestamps moved with wherever the Mac was.
#
# Compare L12's real build against a second real build with both variables
# opposed, independently of --check. Removing its second build cannot make
# this test pass over nondeterministic bytes.
E1="$WORK/real-out"; E2="$WORK/env-b"
( umask 077; TZ=UTC bash "$SCRIPT" --out "$E2" >/dev/null 2>&1 )
D1="$(/usr/bin/shasum -a 256 "$E1"/richos-engine-*.tar.gz 2>/dev/null | awk '{print $1}')"
D2="$(/usr/bin/shasum -a 256 "$E2"/richos-engine-*.tar.gz 2>/dev/null | awk '{print $1}')"
if [ -n "$D1" ] && [ "$D1" = "$D2" ]; then
    ok "L13 the digest survives a different umask and a different timezone ($D1)"
else
    bad "L13 the digest survives a different umask and a different timezone" \
        "umask 022 TZ=Asia/Tokyo gave ${D1:-<none>}; umask 077 TZ=UTC gave ${D2:-<none>}. The pin
         compiled into the app would be a property of whoever built it, and a customer would
         discover that as a DigestMismatch."
fi

# ---------------------------------------------------------------------------------------
# L14 — WHAT IS ACTUALLY IN THE REAL ARCHIVE. The check L12 and L13 cannot make.
# ---------------------------------------------------------------------------------------
# Computed here rather than by calling `verify-engine-asset-members.sh`, on purpose: a case
# that runs the checker and reports what the checker says proves the checker ran, not that
# the archive is clean. This derives the tracked set itself and compares.
if [ -n "$REAL_TARBALL" ] && [ -d "$RX/engine" ]; then
    ( cd "$ROOT/richos/engine" && git ls-files ) | LC_ALL=C sort > "$WORK/real-expected"
    printf 'LICENSE\nTHIRD-PARTY-NOTICES.md\n' >> "$WORK/real-expected"
    if [ -n "${RICHOS_RUNTIME_DIR:-}" ]; then
        ( cd "$RICHOS_RUNTIME_DIR" && find . \( -type f -o -type l \) -print ) | sed 's|^\./|runtime/|' >> "$WORK/real-expected"
    fi
    LC_ALL=C sort -u "$WORK/real-expected" -o "$WORK/real-expected"
    ( cd "$RX/engine" && find . \( -type f -o -type l \) -print ) | sed 's|^\./||' | LC_ALL=C sort > "$WORK/real-actual"
    LC_ALL=C comm -13 "$WORK/real-expected" "$WORK/real-actual" > "$WORK/real-extra"
    EXTRA_N="$(wc -l < "$WORK/real-extra" | tr -d ' ')"
else
    EXTRA_N="-1"
fi
if [ "$EXTRA_N" = "0" ]; then
    ok "L14 every member of the real archive is tracked source or declared runtime ($(wc -l < "$WORK/real-actual" | tr -d ' ') members)"
else
    bad "L14 every member of the real archive is tracked source or declared runtime" \
        "$EXTRA_N untracked member(s), starting with: $(head -3 "$WORK/real-extra" 2>/dev/null | tr '\n' ' ')
         Session snapshots, hook sha256 sidecars and __pycache__ are the known shapes; this
         case does not look for those names, it looks for anything git does not track."
fi

# ---------------------------------------------------------------------------------------
# L15 — END TO END: an ignored file in the source engine, and the archive without it
# ---------------------------------------------------------------------------------------
# `make_fixture` leaves a session snapshot and a hook sidecar in the fixture's engine, both
# matching the fixture's own `.gitignore`. This is the whole defect in miniature.
if [ -f "$F_COMPLETE_TARBALL" ] \
   && [ ! -e "$X/engine/.claude/state/agent-definitions-deadbeef.snapshot" ] \
   && [ ! -e "$X/engine/scripts/hooks/guard-demo.sh.sha256" ]; then
    ok "L15 gitignored files in the source engine do not reach the archive"
else
    bad "L15 gitignored files in the source engine do not reach the archive" \
        "a session snapshot carrying a UUID and an operator home path, or a regenerated
         sha256 sidecar, is inside the file a stranger downloads."
fi

# ---------------------------------------------------------------------------------------
# L16 — THE REFUSAL, PROVED BY A REAL ARCHIVE THAT DESERVES IT
# ---------------------------------------------------------------------------------------
# A negative case has to be able to go red for the right reason, so this poisons a genuine
# archive: unpack the fixture's own tarball, drop an ignored file into it, repack. If the
# member check ever stops looking, this case is the one that says so.
POISON="$WORK/poison"; mkdir -p "$POISON"
if [ -f "$F_COMPLETE_TARBALL" ]; then
    /usr/bin/tar -x -z --no-same-owner -f "$F_COMPLETE_TARBALL" -C "$POISON" 2>/dev/null
    mkdir -p "$POISON/engine/.claude/state"
    printf '# session=deadbeef-8e71-46f3-92d9-925de910870e root=/Users/somebody/ab/richos/engine\n'  > "$POISON/engine/.claude/state/agent-definitions-deadbeef.snapshot" # machine-path-exempt: isolated guest or negative fixture input; no storage default
    ( cd "$POISON" && /usr/bin/tar -c -z -f "$WORK/poisoned.tar.gz" engine ) 2>/dev/null
fi
OUT="$(RICHOS_RUNTIME_DIR= bash "$MEMBERS" "$WORK/poisoned.tar.gz" "$F_COMPLETE_ROOT" \
        LICENSE=LICENSE THIRD-PARTY-NOTICES.md=docs/legal/THIRD-PARTY-NOTICES.md 2>&1)"; CODE=$?
if [ "$CODE" -ne 0 ] \
   && printf '%s' "$OUT" | grep -q 'member(s) that git does not track' \
   && printf '%s' "$OUT" | grep -q 'agent-definitions-deadbeef.snapshot'; then
    ok "L16 an archive carrying an untracked member is REFUSED, and the member is named"
else
    bad "L16 an archive carrying an untracked member is REFUSED, and the member is named" \
        "exit $CODE: $(printf '%s' "$OUT" | tail -1)"
fi

# ---------------------------------------------------------------------------------------
# L23 - the license completeness check judges TRACKED files only
# ---------------------------------------------------------------------------------------
# An ignored vendor or scratch tree carrying a LICENSE is on disk but was never meant to ship.
# The archive is built from the tracked set, so the check that "no license file was dropped"
# must ask the same set, or it blames packaging for omitting a file that was correctly left out.
F="$WORK/ignored-license"; make_fixture "$F"
printf '/scratch-vendor/\n' >> "$F/richos/engine/.gitignore"; fixture_commit "$F"
mkdir -p "$F/richos/engine/scratch-vendor/pkg"
printf 'MIT License\n\nCopyright (c) 2026 Nobody\n' > "$F/richos/engine/scratch-vendor/pkg/LICENSE"
OUT="$(run_fixture "$F")"; CODE=$?
if [ "$CODE" -eq 0 ] && ! printf '%s' "$OUT" | grep -q 'packaging DROPPED'; then
    ok "L23 an ignored LICENSE in the source engine is not required in the archive"
else
    bad "L23 an ignored LICENSE in the source engine is not required in the archive" "exit $CODE: $(printf '%s' "$OUT" | tail -1)"
fi

# ---------------------------------------------------------------------------------------
# L17— the check cannot be quietly deleted
# ---------------------------------------------------------------------------------------
# A guard whose absence is a skip is not a guard. Removing the file must stop the build, not
# the checking.
F="$WORK/no-members-check"; make_fixture "$F"; rm -f "$F/richos/app/scripts/verify-engine-asset-members.sh"
OUT="$(run_fixture "$F")"; CODE=$?
if [ "$CODE" -ne 0 ] && printf '%s' "$OUT" | grep -q 'refusing to build an asset nothing will check'; then
    ok "L17 a missing members check REFUSES the build rather than skipping the check"
else
    bad "L17 a missing members check REFUSES the build rather than skipping the check" "exit $CODE: $(printf '%s' "$OUT" | tail -1)"
fi

# ---------------------------------------------------------------------------------------
# L18 — no checkout, no tracked set, no asset
# ---------------------------------------------------------------------------------------
# The failure direction that matters. Falling back to "copy whatever is on disk" outside a
# checkout would restore the defect silently, in the one situation where nobody is watching.
F="$WORK/no-checkout"; make_fixture "$F"; rm -rf "$F/.git"
OUT="$(run_fixture "$F")"; CODE=$?
if [ "$CODE" -ne 0 ] && printf '%s' "$OUT" | grep -q 'is not inside a git checkout'; then
    ok "L18 building outside a git checkout is REFUSED, not silently fallen back from"
else
    bad "L18 building outside a git checkout is REFUSED, not silently fallen back from" "exit $CODE: $(printf '%s' "$OUT" | tail -1)"
fi

# The actual archive must execute the component without source-checkout siblings.
VOICE_NODE="$RX/engine/runtime/bin/node"
if [ -x "$VOICE_NODE" ]; then
    VOICE_OUT="$(cd "$WORK" && "$VOICE_NODE" "$RX/engine/voice/tests/run.mjs" 2>&1)"; VOICE_RC=$?
    if [ "$VOICE_RC" -eq 0 ]; then
        ok "L19 extracted voice core runs with the delivered Node runtime"
    else
        bad "L19 extracted voice core runs with the delivered Node runtime" "$VOICE_OUT"
    fi
else
    bad "L19 extracted voice core runs with the delivered Node runtime" "no delivered Node"
fi

# ---------------------------------------------------------------------------------------
# L20–L22 — REMEMBERING A DETERMINISM PROOF, and the two ways that could go wrong
# ---------------------------------------------------------------------------------------
# `--check` builds the archive a second time in a different environment: ~25 s of every
# nightly (measured 2026-09-19). What it proves is a property of (this source + this
# script), so when the first build's bytes are ones a previous run already proved, the
# second build cannot learn anything new — and `--check-proof-dir` lets it be remembered.
#
# THE DANGEROUS VERSION KEYS ON THE ARCHIVE DIGEST ALONE, and it would be wrong in exactly
# the way `--check` exists to catch: a script change that makes the output depend on umask
# produces IDENTICAL bytes under the first environment and different ones only under the
# second. Keyed on the digest alone, such a change would inherit the old proof and the
# second build would never run again. So the key is the digest AND this script's own
# sha256, and L22 is what holds that to account.
#
# Run against the synthetic fixture: the real archive takes ~30 s a build and these three
# cases need four of them.
PROOFS="$WORK/check-proofs"
F="$WORK/repro"; make_fixture "$F"
FSCRIPT="$F/richos/app/scripts/make-engine-asset.sh"
repro_run() { RICHOS_RUNTIME_DIR= RICHOS_NIGHTLY_RUN_ID="$1" bash "$FSCRIPT" \
    --out "$F/out-$1" --check --check-proof-dir "$PROOFS" 2>&1; }

OUT="$(repro_run first)"; CODE=$?
if [ "$CODE" -eq 0 ] && printf '%s' "$OUT" | grep -q 'building a second time'; then
    ok "L20 the first run with no recorded proof still builds a second time"
else
    bad "L20 the first run with no recorded proof still builds a second time" \
        "exit $CODE — a proof must be EARNED before it can be spent: $(printf '%s' "$OUT" | tail -2 | tr '\n' ' ')"
fi

OUT="$(repro_run second)"; CODE=$?
if [ "$CODE" -ne 0 ]; then
    bad "L21 an already-proved archive does not rebuild" "exit $CODE"
elif printf '%s' "$OUT" | grep -q 'building a second time'; then
    bad "L21 an already-proved archive does not rebuild" \
        "it built again over byte-identical source and an unchanged script"
elif ! printf '%s' "$OUT" | grep -q 'already proved reproducible'; then
    bad "L21 an already-proved archive does not rebuild" \
        "it neither rebuilt nor said why: $(printf '%s' "$OUT" | tail -2 | tr '\n' ' ')"
elif ! printf '%s' "$OUT" | grep -q 'proved by : first'; then
    bad "L21 an already-proved archive does not rebuild" "the skip does not name the run that proved it"
else
    ok "L21 an already-proved archive does not rebuild, and the skip names the run that proved it"
fi

# L22 — THE CASE THE OBVIOUS IMPLEMENTATION FAILS. The script changes; the archive it
# produces does not. Keyed on the archive alone this would inherit the old proof forever.
printf '\n# a change that does not move the archive\n' >> "$FSCRIPT"
fixture_commit "$F"
OUT="$(repro_run third)"; CODE=$?
if [ "$CODE" -eq 0 ] && printf '%s' "$OUT" | grep -q 'building a second time'; then
    ok "L22 editing the script brings the second build back, even though the archive is unchanged"
else
    bad "L22 editing the script brings the second build back, even though the archive is unchanged" \
        "exit $CODE — a proof keyed on the ARCHIVE alone would never notice a script change that \
makes the output environment-dependent, which is the exact defect --check was written for: \
$(printf '%s' "$OUT" | tail -2 | tr '\n' ' ')"
fi

echo ""
if [ "$FAIL" -gt 0 ]; then
    echo "=== make-engine-asset tests: $FAIL FAILED, $PASS passed ==="
    exit 1
fi
echo "=== make-engine-asset tests: all $PASS passed ==="
