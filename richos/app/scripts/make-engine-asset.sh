#!/usr/bin/env bash
# THE ENGINE, AS A RELEASE ASSET — deterministic, digested, and pinned into the build.
#
#   richos/app/scripts/make-engine-asset.sh [--out <dir>] [--tag <release-tag>]
#
# WHY THIS EXISTS
#
# `ceo-decisions.md` §19 rules the payload at four files and 8,754,980 B and lists the engine
# directory under "What is NOT bundled". The same day: "Where the download lives: THE PUBLIC
# GITHUB REPO'S RELEASES." The engine is already IN that repository, so it reaches a customer
# as a release asset. This script is the thing that makes that asset, and the digest that makes
# it verifiable.
#
# WHAT GOES IN: WHAT GIT TRACKS, NOT WHAT IS ON THE OPERATOR'S DISK
#
# Until 2026-09-04 this copied the engine WORKING TREE — `tar -cf - .` — so every gitignored
# artifact in `engine/` went into the customer's download. Measured on main that day: 119
# ignored files. Fifteen `.claude/state/agent-definitions-*.snapshot`, each carrying a SESSION
# UUID, a generation timestamp and the operator's absolute home path. 57
# `scripts/hooks/*.sha256` sidecars that `install.sh` re-mints on every run. `__pycache__`
# bytecode. Two defects wearing one coat:
#
#   * THE DIGEST MOVED, because a new session mints a new snapshot. The sha256 compiled into
#     the app was a property of WHEN the build ran, and a customer discovers that as a
#     DigestMismatch. Reproduced: build, one new snapshot file, build — 4fcb8b46... became
#     e30083c5....
#   * SESSION IDENTIFIERS AND A HOME DIRECTORY WERE PUBLISHED. The same class the 2026-09-04
#     shipped-artifact privacy pass removed from the executable, arriving through a different
#     door into a different artifact, with nothing watching that door.
#
# Source files come from `git ls-files`. Since 1.2.0 a separately built runtime
# also ships, checked against its complete digest inventory and tracked public-source recipe.
# There is deliberately NO exclusion list: an allowlist of harmless ignored files drifts from
# the ignored files that exist, and the next ignored artifact somebody invents has to be
# excluded by construction rather than by having been thought of. Two consequences worth
# stating out loud:
#
#   * OUTSIDE A GIT CHECKOUT THIS REFUSES rather than falling back to copying everything. A
#     silent fallback would be a fallback into the leak, which is the wrong failure direction.
#   * The archive is checked AFTER it is built, against git, by
#     `verify-engine-asset-members.sh` — because determinism catches this only by accident.
#     `--check` caught the snapshot only because the snapshot changed; AN IGNORED FILE THAT
#     NEVER CHANGES PASSES A DETERMINISM CHECK FOREVER. Proven: 205 ignored files planted and
#     left alone, all thirteen cases green, 208 leaked members in the archive.
#
# DETERMINISTIC, AND THAT IS THE WHOLE POINT
#
# The digest is compiled into the app (`RICHOS_ENGINE_SHA256`) and covered by the app's own
# Developer ID signature. That only means anything if building the tarball twice from the same
# tree produces the same bytes. Four things are normalized, each of which otherwise moves:
#
#   * ORDER    — `find | LC_ALL=C sort`, not the filesystem's directory order.
#   * TIME     — every mtime pinned to the engine's own commit time (or 0 outside a checkout).
#   * OWNER    — uid/gid 0, name `root`, via bsdtar's `--uid/--gid/--uname/--gname`.
#   * MODE     — 755 for directories, 755 for anything the source marks user-executable, 644
#                for everything else. See below; this one was found by running the script,
#                not by reading it.
#   * GZIP     — `gzip -n`, because gzip writes the source filename and an mtime into its own
#                header by default, which makes two identical tarballs differ.
#
# MODE, AND WHY IT IS NOT PARANOIA. Measured 2026-09-04, same tree, same machine, same second:
#
#     umask 022  ->  2,021,839 B  sha256 cbee8763588469de4f01f04a8f3aeaec1dc7bb1225327119c51dbdd7ebd7afb2
#     umask 077  ->  2,021,655 B  sha256 fdae7195cf0083426ead2749fb4e3f87678e29c45cf5ac77d7cce3088e073e33
#
# All 531 members differed, and they differed ONLY in mode — 0755 became 0700, 0644 became
# 0600 — because macOS's bsdtar applies the caller's umask when it extracts as a non-root
# user, so the staging copy inherited the umask of whoever happened to run this. The digest
# that gets compiled into the app and checked on a customer's Mac was a function of an
# operator's shell setting. Normalizing here makes the archive a function of CONTENT plus the
# one permission bit git actually tracks, which is the only determinism boundary that can
# hold across two machines.
#
# `--check` runs the whole thing twice and requires the two digests to match — and the second
# run is deliberately made in a DIFFERENT ENVIRONMENT (another umask, another TMPDIR, the C
# locale, UTC), because a second build under identical conditions cannot fail for any reason
# the first one would not have failed for. That is exactly how the umask defect above survived
# a green `--check`. A non-deterministic build would otherwise be discovered as a
# DigestMismatch on a customer's Mac, which is the one place it must never be discovered.
#
# WHAT IT DOES NOT DO
#
# It does not publish, tag, or upload. `gh release upload` is a human's decision and this
# script's output tells them exactly what to run. Nothing here touches the network.

set -uo pipefail
# macOS metadata is neither source nor a runtime dependency. Exclude resource
# forks, Finder attributes and AppleDouble entries from every tar invocation.
export COPYFILE_DISABLE=1

SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
PRODUCT_ROOT="$(cd "$SELF_DIR/../.." && pwd)"
REPO_ROOT="$(cd "$PRODUCT_ROOT/.." && pwd)"
ENGINE_DIR="$PRODUCT_ROOT/engine"

OUT_DIR="$PRODUCT_ROOT/app/target/engine-asset"
TAG=""
CHECK=0
# WHERE A PASSED DETERMINISM PROOF IS REMEMBERED, and why remembering one is sound.
#
# `--check` builds the archive a SECOND time in a different environment and compares the
# two digests (see the note above). Measured 2026-09-19 on this Mac, that second build is
# ~25 s of every nightly. What it proves is a property of (this source tree + this script):
# that the bytes coming out do not depend on the environment going in.
#
# So when the FIRST build produces a digest that a previous run already proved reproducible
# — with this same script — building it a second time cannot learn anything new. The key is
# the first build's own digest plus this file's sha256, and BOTH halves are load-bearing:
# the digest alone would let a script change that introduces umask-sensitivity slip through,
# because such a change produces identical bytes under the first environment and different
# ones only under the second. That is the exact defect `--check` was written for, so it is
# the exact thing the key must notice.
#
# Unset — the default, and what any caller that does not opt in gets — means the second
# build always runs. `nightly-local.py` sets it for `build` and NEVER for `release`.
CHECK_PROOF_DIR="${RICHOS_ENGINE_CHECK_PROOF_DIR:-}"
CHECK_KEY=""
RUNTIME_DIR="${RICHOS_RUNTIME_DIR:-}"

while [ $# -gt 0 ]; do
    case "$1" in
        --out)   OUT_DIR="$2"; shift 2 ;;
        --tag)   TAG="$2"; shift 2 ;;
        --check) CHECK=1; shift ;;
        --check-proof-dir) CHECK_PROOF_DIR="$2"; shift 2 ;;
        --runtime-dir) RUNTIME_DIR="$2"; shift 2 ;;
        -h|--help)
            sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
            exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

die() { echo "REFUSING — $*" >&2; exit 1; }

# ---------------------------------------------------------------------------------------
# The engine, and its version
# ---------------------------------------------------------------------------------------
[ -d "$ENGINE_DIR" ] || die "no engine directory at $ENGINE_DIR"
[ -d "$ENGINE_DIR/scripts/hooks" ] || die "$ENGINE_DIR has no scripts/hooks — it is not an engine"
[ -f "$ENGINE_DIR/VERSION" ] || die "$ENGINE_DIR has no VERSION — it is not an engine"

VERSION="$(tr -d '[:space:]' < "$ENGINE_DIR/VERSION")"
[ -n "$VERSION" ] || die "$ENGINE_DIR/VERSION is empty"
if [ "$VERSION" = "1.2.0" ]; then
    [ -n "$RUNTIME_DIR" ] || die "engine 1.2.0 requires --runtime-dir with a verified runtime build"
fi
if [ -n "$RUNTIME_DIR" ]; then
    RUNTIME_DIR="$(cd "$RUNTIME_DIR" && pwd)" || die "runtime directory is missing"
    python3 "$SELF_DIR/verify-runtime.py" "$RUNTIME_DIR" "$SELF_DIR/runtime-sources.json" \
        || die "runtime verification failed"
fi

# ---------------------------------------------------------------------------------------
# THE TRACKED SET, WHICH IS THE ONLY THING THAT MAY BE PACKAGED
# ---------------------------------------------------------------------------------------
# A checkout is a precondition, not a nice-to-have: outside one there is no tracked set, and
# the alternative — packaging whatever is on disk — is exactly the defect this refuses.
git -C "$ENGINE_DIR" rev-parse --is-inside-work-tree >/dev/null 2>&1 \
    || die "$ENGINE_DIR is not inside a git checkout. The asset is built from what git TRACKS, and outside a checkout there is no such thing — refusing rather than falling back to packaging the working tree"

# A tracked file that is missing from disk would make bsdtar fail somewhere down the pipe with
# nothing named. Say which one, here.
MISSING_ON_DISK=""
while IFS= read -r rel; do
    [ -n "$rel" ] || continue
    [ -e "$ENGINE_DIR/$rel" ] || [ -L "$ENGINE_DIR/$rel" ] || MISSING_ON_DISK="$MISSING_ON_DISK engine/$rel"
done <<EOF
$(git -C "$ENGINE_DIR" ls-files)
EOF
[ -z "$MISSING_ON_DISK" ] \
    || die "git tracks file(s) that are not on disk, so the checkout is incomplete:$MISSING_ON_DISK"

# The check that reads the FINISHED archive back and refuses any member git does not track.
# Its absence is a refusal rather than a skip — a check that can be removed by deleting a file
# is not a check.
MEMBERS_CHECK="$SELF_DIR/verify-engine-asset-members.sh"
[ -f "$MEMBERS_CHECK" ] \
    || die "no $MEMBERS_CHECK — refusing to build an asset nothing will check the contents of"

# ---------------------------------------------------------------------------------------
# THE LICENSE, WHICH LIVES OUTSIDE THE DIRECTORY BEING PACKAGED
# ---------------------------------------------------------------------------------------
#
# The canonical AGPL text is at the REPOSITORY root, and this script archives `engine/`.
# So until 2026-09-04 the standalone engine tarball shipped with no license text at all:
# an AGPL work distributed without its terms, to a customer who has no repository to look
# in. The fix is a copy made HERE, at packaging time, rather than a second copy committed
# inside engine/ - because a committed duplicate of a license text is a file that can drift
# from the canonical one with nothing to catch it. This copy cannot drift; it is made from
# the canonical file every time and read back out of the finished archive below.
LICENSE_SRC="$REPO_ROOT/LICENSE"
NOTICES_SRC="$REPO_ROOT/docs/legal/THIRD-PARTY-NOTICES.md"
[ -f "$LICENSE_SRC" ] || die "no LICENSE at $LICENSE_SRC - refusing to build an unlicensed asset"
[ -f "$NOTICES_SRC" ] || die "no third-party notices at $NOTICES_SRC - refusing to build an asset that cannot name what it bundles"

# The engine's own commit time, so the archive's mtimes are a property of the CONTENT and not
# of when somebody happened to run this. Outside a checkout, 0.
SOURCE_EPOCH="$(git -C "$REPO_ROOT" log -1 --format=%ct -- richos/engine 2>/dev/null || true)"
[ -n "$SOURCE_EPOCH" ] || SOURCE_EPOCH=0
SOURCE_SHA="$(git -C "$REPO_ROOT" rev-parse --short=12 HEAD 2>/dev/null || echo "not-a-checkout")"

[ -n "$TAG" ] || TAG="engine-v$VERSION"
ASSET="richos-engine-$VERSION.tar.gz"
URL="https://github.com/RichSpark/RichOS/releases/download/$TAG/$ASSET"

# ---------------------------------------------------------------------------------------
# Build one archive, deterministically, into $1
# ---------------------------------------------------------------------------------------
build_into() {
    local out="$1"
    local staging
    staging="$(mktemp -d "${TMPDIR:-/tmp}/richos-engine-asset.XXXXXX")" || return 1
    # shellcheck disable=SC2064
    trap "rm -rf '$staging'" RETURN

    # A COPY OF THE TRACKED FILES, so nothing here can touch the engine in the checkout and
    # nothing gitignored can reach the customer. `-z` on both `ls-files` and `tar --null`, so
    # a path with a space or a non-ASCII character is copied as itself rather than as git's
    # quoted rendering of it. bsdtar creates the intermediate directories on extraction and
    # the chmod pass below normalizes their modes, so listing files rather than directories
    # loses nothing — and an empty directory cannot be tracked in the first place.
    local tracked="$staging/tracked.z"
    git -C "$ENGINE_DIR" ls-files -z > "$tracked" || return 1
    [ -s "$tracked" ] || return 1
    mkdir -p "$staging/engine"
    ( cd "$ENGINE_DIR" && /usr/bin/tar -cf - --null -T "$tracked" ) \
        | ( cd "$staging/engine" && /usr/bin/tar -xf - ) || return 1

    # THE TERMS, COPIED IN. Before the manifest is built, so these files are ordered,
    # timestamped and owned exactly like every other member and the archive stays
    # reproducible. `cp` without `-p`: the mtime is pinned below for all members alike.
    cp "$LICENSE_SRC" "$staging/engine/LICENSE" || return 1
    cp "$NOTICES_SRC" "$staging/engine/THIRD-PARTY-NOTICES.md" || return 1
    if [ -n "$RUNTIME_DIR" ]; then
        # This directory was produced from pinned public distributions and checked
        # against its complete file/link inventory. It is the only generated payload.
        mkdir "$staging/engine/runtime" || return 1
        ( cd "$RUNTIME_DIR" && /usr/bin/tar -cf - . ) \
            | ( cd "$staging/engine/runtime" && /usr/bin/tar -xf - ) || return 1
    fi

    # MODE, fixed. Read the executable bit off the COPY (which preserves the source's u+x
    # under any sane umask) and then flatten everything to two permissions. `-perm -u+x` is
    # the discriminant rather than any group/other bit precisely because those are the bits
    # a umask moves; u+x is also the only permission git records.
    find "$staging/engine" -type d -exec chmod 755 {} + || return 1
    # BSD tar applies umask to symlink modes too; normalize the link, not its target.
    find "$staging/engine" -type l -exec chmod -h 755 {} + || return 1
    find "$staging/engine" -type f -perm -u+x -exec chmod 755 {} + || return 1
    find "$staging/engine" -type f ! -perm -u+x -exec chmod 644 {} + || return 1

    # ORDER, fixed. `-print` then sort, rather than trusting readdir.
    local manifest="$staging/manifest"
    ( cd "$staging" && find engine -print ) | LC_ALL=C sort > "$manifest" || return 1

    # TIME, fixed. `touch -h` so a symlink's own timestamp is set rather than its target's.
    #
    # TZ IS ON BOTH HALVES AND THAT IS THE WHOLE POINT. `date -u` prints the stamp in UTC,
    # but `touch -t` READS ITS ARGUMENT IN THE LOCAL ZONE — so on a Mac set to anything but
    # UTC the two disagreed by the offset and the archive's mtimes, and therefore its digest,
    # were a function of the operator's timezone. Measured 2026-09-04, same tree, same
    # machine, one variable:
    #
    #     TZ unset (Europe/Berlin)  ->  sha256 cbee8763588469de4f01f04a8f3aeaec1dc7bb12...
    #     TZ=UTC                    ->  sha256 bdb82559bce916dc8945142a1f85d02a2d831609...
    #
    # Pinning both ends to UTC makes the mtime a property of the engine's commit and of
    # nothing else.
    local stamp
    stamp="$(TZ=UTC date -u -r "$SOURCE_EPOCH" +%Y%m%d%H%M.%S 2>/dev/null || echo 197001010000.00)"
    ( cd "$staging" && LC_ALL=C TZ=UTC xargs -0 -n 200 touch -h -t "$stamp" < <(tr '\n' '\0' < "$manifest") ) || return 1

    # OWNER + GZIP, fixed. bsdtar writes the members in the order given on stdin.
    ( cd "$staging" \
        && /usr/bin/tar --no-xattrs --no-acls --uid 0 --gid 0 --uname root --gname root \
                        -c -f - -T "$manifest" -n \
        | gzip -n -9 > "$out" ) || return 1

    [ -s "$out" ] || return 1
}

sha256_of() { /usr/bin/shasum -a 256 "$1" | awk '{print $1}'; }

mkdir -p "$OUT_DIR" || die "cannot create $OUT_DIR"
# ONE RUN AT A TIME IN THIS OUTPUT DIRECTORY, from any checkout. The default one sits in
# `app/target`, which `shared-build-cache.sh` links to one cache for every checkout, and the
# tarball below is written, digested and pinned by name, where cargo's lock never reaches.
# Held until this script and everything it started have exited (hunt part 2, section 21).
. "$SELF_DIR/lib/output-lock.sh" || die "no $SELF_DIR/lib/output-lock.sh"
hold_output_lock "$OUT_DIR" 9 "make-engine-asset.sh" \
    || die "$OUT_DIR is another packaging run's until it finishes; nothing was written"
TARBALL="$OUT_DIR/$ASSET"

echo "=== the engine ==="
echo "  source   : $ENGINE_DIR"
echo "  version  : $VERSION"
echo "  repo HEAD: $SOURCE_SHA"
echo "  mtimes   : $SOURCE_EPOCH (the engine's own last commit time)"
echo

build_into "$TARBALL" || die "the archive could not be built"
DIGEST="$(sha256_of "$TARBALL")"
BYTES="$(/usr/bin/stat -f %z "$TARBALL")"

echo "=== the asset ==="
echo "  file  : $TARBALL"
echo "  bytes : $BYTES"
echo "  sha256: $DIGEST"
echo

# ---------------------------------------------------------------------------------------
# DETERMINISM, PROVED RATHER THAN CLAIMED
# ---------------------------------------------------------------------------------------
if [ "$CHECK" = "1" ] && [ -n "$CHECK_PROOF_DIR" ]; then
    # The key: these bytes, AND the script that produced them. See the note beside
    # CHECK_PROOF_DIR for why one without the other would be a hole.
    CHECK_KEY="$(printf '%s %s\n' "$DIGEST" "$(sha256_of "$0")" | /usr/bin/shasum -a 256 | awk '{print $1}')"
    CHECK_PROOF="$CHECK_PROOF_DIR/$CHECK_KEY.reproducible"
    if [ -f "$CHECK_PROOF" ]; then
        echo "=== --check: these exact bytes were already proved reproducible ==="
        echo "  sha256    : $DIGEST"
        echo "  proved by : $(sed -n 's/^run=//p' "$CHECK_PROOF" | head -1) on $(sed -n 's/^at=//p' "$CHECK_PROOF" | head -1)"
        echo "  A second build in a different environment tests whether THIS source and THIS"
        echo "  script depend on the environment. Both are byte-identical to the run above, so"
        echo "  it would ask a question that has already been answered. Edit either one and the"
        echo "  key changes and the second build comes back."
        echo
        CHECK=0
    fi
fi
if [ "$CHECK" = "1" ]; then
    echo "=== --check: building a second time, in a DIFFERENT environment, and comparing ==="
    CHECK_TMP="$(mktemp -d "${TMPDIR:-/tmp}/richos-engine-check.XXXXXX")"
    SECOND="$CHECK_TMP/second.tar.gz"
    echo "  second build runs under: umask 077, TMPDIR=$CHECK_TMP, LC_ALL=C, TZ=UTC"
    ( umask 077; TMPDIR="$CHECK_TMP" LC_ALL=C TZ=UTC build_into "$SECOND" ) || die "the second build failed"
    SECOND_DIGEST="$(sha256_of "$SECOND")"
    echo "  first : $DIGEST"
    echo "  second: $SECOND_DIGEST"
    if [ "$DIGEST" != "$SECOND_DIGEST" ]; then
        rm -rf "$CHECK_TMP"
        die "THE ARCHIVE IS NOT DETERMINISTIC. A pin over these bytes would fail on a customer's Mac."
    fi
    echo "  IDENTICAL, across two environments."
    rm -rf "$CHECK_TMP"
    # Recorded only now, after the proof actually held. A proof written before the
    # comparison would let a non-deterministic build excuse itself on the next run.
    if [ -n "$CHECK_PROOF_DIR" ] && mkdir -p "$CHECK_PROOF_DIR" 2>/dev/null; then
        {
            echo "sha256=$DIGEST"
            echo "script=$(sha256_of "$0")"
            echo "run=${RICHOS_NIGHTLY_RUN_ID:-manual}"
            echo "at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        } > "$CHECK_PROOF_DIR/$CHECK_KEY.reproducible" 2>/dev/null || true
    fi
    echo
fi

# The archive is what the app will check, so check it here too — the same shape check
# `install_engine` performs, run before anybody uploads anything.
VERIFY="$(mktemp -d "${TMPDIR:-/tmp}/richos-engine-verify.XXXXXX")"
/usr/bin/tar -x -z --no-same-owner -f "$TARBALL" -C "$VERIFY" || die "the archive does not open"
[ -d "$VERIFY/engine/scripts/hooks" ] || die "the archive has no engine/scripts/hooks"
[ -f "$VERIFY/engine/VERSION" ] || die "the archive has no engine/VERSION"
# Scan extracted shipped bytes. Fixture/history reasons apply only to source text;
# native/runtime binaries cannot waive embedded host paths.
python3 "$SELF_DIR/lib/no_host_paths.py" "$VERIFY/engine" --reviewed-text --home "$HOME" --upstream-policy "$SELF_DIR/runtime-sources.json" --upstream-runtime-root "$VERIFY/engine/runtime" \
    || die "the engine archive contains machine-specific storage paths"
[ "$(tr -d '[:space:]' < "$VERIFY/engine/VERSION")" = "$VERSION" ] || die "the archive's VERSION is not $VERSION"

# THE LICENSE CHECK IS READ OUT OF THE ARCHIVE, not off the staging directory. `cp` having
# exited 0 is a claim about a command; the bytes inside the tarball are the artifact, and
# the artifact is the only thing a customer ever sees.
[ -f "$VERIFY/engine/LICENSE" ] || die "the archive has no engine/LICENSE - it would ship an AGPL work with no terms"
[ -f "$VERIFY/engine/THIRD-PARTY-NOTICES.md" ] || die "the archive has no engine/THIRD-PARTY-NOTICES.md"
LICENSE_CANON="$(sha256_of "$LICENSE_SRC")"
LICENSE_SHIPPED="$(sha256_of "$VERIFY/engine/LICENSE")"
[ "$LICENSE_CANON" = "$LICENSE_SHIPPED" ] \
    || die "the archive's LICENSE is NOT the canonical text ($LICENSE_SHIPPED vs $LICENSE_CANON)"

# EVERY BUNDLED THIRD-PARTY DIRECTORY KEEPS ITS OWN TERMS, and neither half of that rule
# is a typed list. A hand-written inventory of directories to check is the defect this
# repository has already paid for five times, so both halves are derived from disk:
#
#   (a) NOTHING IS LOST IN PACKAGING. Every license file present under the source engine/
#       must be present at the same relative path inside the archive. This is the check
#       that actually catches a packaging change, and it covers all seven bundled items
#       without naming one of them.
#   (b) NOTHING ARRIVES WITHOUT ITS TERMS. A skill whose own frontmatter declares a
#       `license:` must have a license file beside it. This is the check that catches a
#       NEW vendored skill added with a claim and no text - the exact defect the
#       2026-09-04 audit found in three directories at once.
#
# THE INVENTORY IS THE TRACKED SET, the same one the archive is built from and the members
# check enforces. `find` on disk would also see an ignored vendor or scratch tree with a
# LICENSE in it, which was never meant to ship, and would then blame packaging for dropping it.
license_files_under() {
    git -C "$1" ls-files \
        | { grep -E '(^|/)(LICENSE|LICENSE\.txt|LICENSE\.md)$' || true; } \
        | { grep -v -x 'LICENSE' || true; } \
        | sed 's|^|./|' | LC_ALL=C sort
}

MISSING_TERMS=""
while IFS= read -r rel; do
    [ -n "$rel" ] || continue
    [ -f "$VERIFY/engine/$rel" ] || MISSING_TERMS="$MISSING_TERMS engine/${rel#./}"
done <<EOF
$(license_files_under "$ENGINE_DIR")
EOF
[ -z "$MISSING_TERMS" ] \
    || die "packaging DROPPED bundled license file(s) that exist in the source engine:$MISSING_TERMS"

CLAIMED_NO_TEXT=""
while IFS= read -r sk; do
    [ -n "$sk" ] || continue
    d="$(dirname "$sk")"
    sed -n '1,25p' "$sk" | grep -qi '^license:' || continue
    if [ ! -f "$d/LICENSE" ] && [ ! -f "$d/LICENSE.txt" ] && [ ! -f "$d/LICENSE.md" ]; then
        CLAIMED_NO_TEXT="$CLAIMED_NO_TEXT ${d#"$VERIFY"/}"
    fi
done <<EOF
$(find "$VERIFY/engine/skills" -maxdepth 2 -name SKILL.md 2>/dev/null | LC_ALL=C sort)
EOF
[ -z "$CLAIMED_NO_TEXT" ] \
    || die "skill(s) in the archive declare a license with no license file beside it:$CLAIMED_NO_TEXT"

# COUNTED FROM THE TRACKED SET, NOT THE UNPACKED DIRECTORY. The extraction has no .git, so
# `git ls-files` on it fails and the summary used to say "0 ... intact" under a fatal
# diagnostic. The MISSING_TERMS loop above has just proved every file in this inventory is
# present in the archive, so its length IS the number of bundled license files intact.
TP_COUNT="$(license_files_under "$ENGINE_DIR" | wc -l | tr -d ' ')"
FILES="$(find "$VERIFY/engine" -type f | wc -l | tr -d ' ')"
rm -rf "$VERIFY"

# ---------------------------------------------------------------------------------------
# EVERY MEMBER ACCOUNTED FOR, AGAINST GIT
# ---------------------------------------------------------------------------------------
# Building from `git ls-files` is the fix; this is the check that the fix held. They are
# separate on purpose — the build could grow a filter, a transform or a second copy step, and
# this reads the FINISHED archive back and compares its members to the tracked set in both
# directions. The two files packaging adds are declared as pairs, and each pair's repo half
# must itself be tracked, so a pair cannot become a way to smuggle something in.
echo "=== every member accounted for, against git ==="
RICHOS_RUNTIME_DIR="$RUNTIME_DIR" bash "$MEMBERS_CHECK" "$TARBALL" "$REPO_ROOT" \
    "LICENSE=${LICENSE_SRC#"$REPO_ROOT"/}" \
    "THIRD-PARTY-NOTICES.md=${NOTICES_SRC#"$REPO_ROOT"/}" \
    || die "the archive's contents are not what git tracks (above)"
echo

echo "=== it opens, and it is an engine ==="
echo "  engine/scripts/hooks  present"
echo "  engine/VERSION        $VERSION"
echo "  engine/LICENSE        canonical ($LICENSE_CANON)"
echo "  third-party notices   present, $TP_COUNT bundled license file(s) intact"
echo "  files                 $FILES"
echo

# ---------------------------------------------------------------------------------------
# The pin, ready to paste
# ---------------------------------------------------------------------------------------
PIN_FILE="$OUT_DIR/engine-pin.env"
cat > "$PIN_FILE" <<PINEOF
# THE ENGINE PIN — read at COMPILE time by richos-core (\`setup.rs::engine_pin\`), so the digest
# ends up inside the executable that the app's Developer ID signature covers. A runtime
# variable would be a value an attacker on the machine could set, which is the opposite of a
# pin. An unset or malformed pin makes the build refuse to install an engine rather than
# trusting whatever the URL returns.
#
# Built from engine $VERSION at repo $SOURCE_SHA.
export RICHOS_ENGINE_VERSION=$VERSION
export RICHOS_ENGINE_URL=$URL
export RICHOS_ENGINE_SHA256=$DIGEST
PINEOF

echo "=== the pin ==="
cat "$PIN_FILE"
echo
echo "=== what a human does next, and nothing here did it ==="
echo "  gh release create $TAG --repo RichSpark/RichOS --title 'RichOS engine $VERSION' --notes '...'"
echo "  gh release upload $TAG '$TARBALL' --repo RichSpark/RichOS"
echo "  source '$PIN_FILE' && richos/app/scripts/package-app.sh"
echo
echo "  The upload has to happen BEFORE a build carrying this pin ships, or every customer's"
echo "  first run gets a 404 and a named DownloadFailed. The pin is not a promise; it is a"
echo "  claim about bytes that must already be there."
