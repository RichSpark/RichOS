#!/usr/bin/env bash
#
# build-myrichos.sh — materialize the CEO's central folder from the seed in this repository.
#
# WHAT THIS IS. `docs/plans/richos-central-folder-2026-09-06.md` §6 names the smallest useful
# first commit: the folder, the six `companies/<id>/company.md` files, and the six copied guard
# fragments. This script is how that folder comes into existence, and how it can be brought back
# if it is ever lost. It changes NO application behavior — nothing in `app/**` reads any of it
# yet. Every remaining step in that document is a wiring change with a target that now exists.
#
# ---------------------------------------------------------------------------------------------
# THE PARENT DIRECTORY IS DECLARED ONCE, ON ONE LINE, AND NOWHERE ELSE.
#
# §8 open question 1: `~/myrichos` or `~/ab/myrichos`? Sage marked it `unverified:`, designed for
# `~/myrichos`, and said the choice is one line to change. It is the CEO's call and he has not
# been asked. So it is ONE LINE — the assignment below — and it is honored by every other file
# in this tool through substitution rather than repetition. No seed file names a parent. Changing
# his mind costs this line, or one invocation with MYRICHOS_ROOT set.
# ---------------------------------------------------------------------------------------------
MYRICHOS_ROOT="${MYRICHOS_ROOT:-$HOME/myrichos}"

# The `richos` main checkout, for the ONE pointer that leaves this folder: the person-layer
# doctrine template, which is version-controlled and rendered by `doctrine.rs` and is therefore
# NOT copied here (§1.3 — the only copy stays where it is; this folder holds a checked pointer).
#
# It deliberately does NOT derive from this script's own location. A teammate runs this from a
# worktree that gets removed at land time, and a pointer into a removed worktree is precisely the
# dangling-symlink failure §1.3 exists to prevent. It defaults to the checkout the entity registry
# already names as a `richos` root.
if [ -z "${RICHOS_ROOT:-}" ]; then
    RICHOS_COMMON="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --path-format=absolute --git-common-dir)" || exit 2
    RICHOS_ROOT="$(dirname "$RICHOS_COMMON")"
fi

# WHY IT NEVER OVERWRITES. `myrichos` is the CEO's folder and §1.3 rules that it holds the ONLY
# copy of anything it holds. A seed that clobbered his edits would make this repository the real
# master and the folder a cache, which is the opposite of the design. So every write is
# create-if-absent. Re-running this is safe and is the intended way to add a company later.
#
# WHY THE GUARDS ARE COPIED RATHER THAN MOVED. §5.2 step 2: copy, do not move. The originals
# keep firing until step 4 has positively observed the replacements firing. During that window
# the copies are a deliberate, temporary second copy — the one exception to §1.3 — so each one
# is recorded in `guards/SOURCES` with its origin and its SHA-256, and `check-myrichos.sh`
# reports drift. A copy nobody can check is how the 13 stale forks in §3.2 happened.
#
# USAGE
#   tools/myrichos/build-myrichos.sh              # build at the declared root
#   MYRICHOS_ROOT=/tmp/scratch tools/myrichos/build-myrichos.sh
#
# EXIT
#   0  the folder is present and every seeded file is in place
#   1  a source this script must copy from is missing (never a silent skip)

set -euo pipefail

SEED_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/seed"

created=0
kept=0

# Copy one seed file into the folder, substituting the single root token. Never overwrites.
place() {
    local rel="$1" dest="$MYRICHOS_ROOT/$1"
    if [ -e "$dest" ]; then
        kept=$((kept + 1))
        return 0
    fi
    mkdir -p "$(dirname "$dest")"
    # @MYRICHOS@ and @RICHOS@ are the ONLY path tokens any seed file may contain. This is the
    # single point where either declared root reaches the materialized folder. @DOCTRINE_SHA@
    # is resolved here too, so the pointer's digest is measured at build time rather than
    # transcribed by hand into a file that would then quietly disagree with the template.
    sed -e "s|@MYRICHOS@|$MYRICHOS_ROOT|g" \
        -e "s|@RICHOS@|$RICHOS_ROOT|g" \
        -e "s|@DOCTRINE_SHA@|$DOCTRINE_SHA|g" \
        "$SEED_DIR/$rel" > "$dest"
    created=$((created + 1))
}

# Copy a live company hook into its company's guards directory, and record where it came from.
# A missing source is a hard failure: this script must never report a guard it did not place.
copy_guard() {
    local company="$1" src="$2"
    local name dest_dir dest
    name="$(basename "$src")"
    dest_dir="$MYRICHOS_ROOT/companies/$company/guards"
    dest="$dest_dir/$name"

    if [ ! -f "$src" ]; then
        printf 'ERROR: build-myrichos.sh: company hook not found: %s\n' "$src" >&2
        printf '       Refusing to report a guard that was not copied.\n' >&2
        exit 1
    fi

    mkdir -p "$dest_dir"
    if [ ! -e "$dest" ]; then
        cp "$src" "$dest"
        chmod +x "$dest"
        created=$((created + 1))
    else
        kept=$((kept + 1))
    fi

    # The drift record. `check-myrichos.sh` re-reads this and compares both sides.
    printf '%s  %s  %s\n' "$(shasum -a 256 "$src" | cut -d' ' -f1)" "$name" "$src" \
        >> "$dest_dir/SOURCES.tmp"
}

# The digest the doctrine pointer records. Measured now, from the file itself. A missing template
# is a hard failure rather than a pointer written with a blank where its identity belongs.
DOCTRINE_TEMPLATE="$RICHOS_ROOT/richos/app/crates/richos-core/doctrine/inner-doctrine.md"
if [ ! -f "$DOCTRINE_TEMPLATE" ]; then
    printf 'ERROR: build-myrichos.sh: person-layer doctrine template not found:\n' >&2
    printf '       %s\n' "$DOCTRINE_TEMPLATE" >&2
    printf '       Set RICHOS_ROOT to the richos checkout. Refusing to write an unverifiable pointer.\n' >&2
    exit 1
fi
DOCTRINE_SHA="$(shasum -a 256 "$DOCTRINE_TEMPLATE" | cut -d' ' -f1)"

mkdir -p "$MYRICHOS_ROOT"

# --- the shape (§1.1) ------------------------------------------------------------------------
# `team/` and `memory/` are created empty on purpose: §6 lists the team definitions as a separate
# buildable item and this commit is deliberately the smallest useful one. An empty directory that
# the design names is a target for the next step; an absent one is a surprise for it.
for c in femcboost deeply prospects richos gpt-exporter webinar-booster; do
    mkdir -p "$MYRICHOS_ROOT/companies/$c/team" "$MYRICHOS_ROOT/companies/$c/memory"
done
mkdir -p "$MYRICHOS_ROOT/me" "$MYRICHOS_ROOT/registry" "$MYRICHOS_ROOT/inbox"

# --- the seeded files ------------------------------------------------------------------------
place README.md
place me/doctrine.source.md
place me/identity.config
for c in femcboost deeply prospects richos gpt-exporter webinar-booster; do
    place "companies/$c/company.md"
    place "companies/$c/guards/settings.json"
done

# --- the copied guards (§5.2 step 2 — copy, never move) ---------------------------------------
for c in femcboost deeply prospects richos gpt-exporter webinar-booster; do
    rm -f "$MYRICHOS_ROOT/companies/$c/guards/SOURCES.tmp"
done

FEMCBOOST_HOOKS="${FEMCBOOST_ROOT:-$(dirname "$RICHOS_ROOT")/femcboost}/scripts/hooks"
DEEPLY_HOOKS="${DEEPLY_ROOT:-$(dirname "$RICHOS_ROOT")/deeply}/scripts/hooks"

copy_guard femcboost "$FEMCBOOST_HOOKS/ecs-session-start.sh"
copy_guard femcboost "$FEMCBOOST_HOOKS/ecs-user-prompt-submit.sh"
copy_guard femcboost "$FEMCBOOST_HOOKS/ecs-stop.sh"
copy_guard femcboost "$FEMCBOOST_HOOKS/ecs-teammate-idle.sh"
copy_guard femcboost "$FEMCBOOST_HOOKS/ecs-task-completed.sh"
copy_guard deeply    "$DEEPLY_HOOKS/deeply-design-brief-gate.sh"

for c in femcboost deeply prospects richos gpt-exporter webinar-booster; do
    tmp="$MYRICHOS_ROOT/companies/$c/guards/SOURCES.tmp"
    if [ -f "$tmp" ]; then
        mv "$tmp" "$MYRICHOS_ROOT/companies/$c/guards/SOURCES"
    fi
done

printf 'myrichos: %s\n' "$MYRICHOS_ROOT"
printf 'created: %d file(s)   kept existing: %d file(s)\n' "$created" "$kept"
printf 'Now run: %s/check-myrichos.sh\n' "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
