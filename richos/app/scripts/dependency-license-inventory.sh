#!/usr/bin/env bash
#
# dependency-license-inventory.sh — the compiled-dependency license inventory,
# derived from the tracked lockfiles rather than typed.
#
# ===========================================================================
# WHY THIS EXISTS
# ===========================================================================
# `docs/legal/THIRD-PARTY-NOTICES.md` makes a per-package dependency inventory
# a GATE on distributing a RichOS binary. Until 2026-09-04 that gate could not
# be discharged, and the notices file said so in as many words: both
# `Cargo.lock` files were gitignored, so an inventory generated on any given
# morning would have been keyed to whatever versions resolved that morning on
# one machine. The notices file called that "a document that looks like
# evidence", which is exactly right.
#
# The lockfiles are now tracked. This script is the other half: it reads the
# RESOLVED graph — `cargo metadata --locked`, which refuses to run at all if a
# lockfile would have to change — and writes the inventory keyed to the sha256
# of the lockfiles it was generated from.
#
# ===========================================================================
# NOTHING HERE IS TYPED
# ===========================================================================
# The workspace list is not a list. It is every `Cargo.lock` that git tracks,
# discovered with `git ls-files`, because tracked-and-not-ignored is not an
# approximation of what somebody receives — it IS what they receive. Add a
# fourth Rust workspace to this repository and commit its lockfile, and it
# appears here with no edit to this file. That is deliberate: a hand-maintained
# inventory of things to inventory is the defect this repository has already
# paid for several times over.
#
# Per-package license identifiers come from cargo's own metadata, which reads
# each package's manifest out of the registry source it actually resolved to —
# not from a hand-copied table, and not from crates.io at generation time.
#
# ===========================================================================
# WHAT IT CANNOT TELL YOU — said here so nobody has to find out
# ===========================================================================
#   * Whether a declared license is TRUE. A crate that declares MIT while
#     vendoring something else is invisible to every tool of this kind. The
#     declaration is the publisher's claim and this file reproduces it.
#   * Which alternative you chose. Most of this tree is "MIT OR Apache-2.0" —
#     a choice, not a conjunction. The document records the offer; the
#     licensing analysis beside it records what RichOS relies on.
#   * Anything about the ENGINE's own bundled skills, fonts or tools. Those are
#     not compiled dependencies and they live in
#     `docs/legal/THIRD-PARTY-NOTICES.md`, which is hand-verified against
#     upstream because provenance is not a thing a resolver knows.
#
# ===========================================================================
# USAGE
# ===========================================================================
#   richos/app/scripts/dependency-license-inventory.sh            # write the document
#   richos/app/scripts/dependency-license-inventory.sh --check    # fail if it is stale
#
# `--check` regenerates into a temporary file and compares. It is the answer to
# "somebody bumped a dependency and the legal document still describes the old
# graph", which is the only way this document can lie.
set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
OUT="$ROOT/docs/legal/THIRD-PARTY-RUST-DEPENDENCIES.md"

MODE="write"
if [ "${1:-}" = "--check" ]; then
  MODE="check"
elif [ $# -gt 0 ]; then
  echo "dependency-license-inventory.sh: unknown argument '$1' (expected --check or nothing)." >&2
  exit 2
fi

# cargo is frequently absent from a non-login shell's PATH even where it is
# installed. Same rule package-app.sh uses, for the same reason.
if ! command -v cargo >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/cargo" ]; then
  PATH="$HOME/.cargo/bin:$PATH"
  export PATH
fi
if ! command -v cargo >/dev/null 2>&1; then
  echo "dependency-license-inventory.sh: cargo not found on PATH. Install Rust (https://rustup.rs)." >&2
  echo "                                 Refusing to write an inventory with no resolver behind it." >&2
  exit 3
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "dependency-license-inventory.sh: python3 not found on PATH." >&2
  exit 3
fi

# Every tracked lockfile, in sorted order, relative to the repository root — except one a
# vendored crate under richos/app/third-party/ was published with. That crate is a path
# dependency of a RichOS workspace, and Cargo never reads a dependency's own lockfile: what it
# really brings in is resolved in that workspace's lockfile and inventoried there, with the
# vendored crate itself as a row (lib/dependency_license_inventory.py, VENDORED).
LOCKS=()
while IFS= read -r f; do
  case "$f" in richos/app/third-party/*) continue ;; esac
  [ -n "$f" ] && LOCKS+=("$f")
done <<EOF
$(cd "$ROOT" && git ls-files '*Cargo.lock' | LC_ALL=C sort)
EOF

if [ "${#LOCKS[@]}" -eq 0 ]; then
  echo "dependency-license-inventory.sh: git tracks NO Cargo.lock in $ROOT." >&2
  echo "                                 An inventory over an unpinned graph is not evidence;" >&2
  echo "                                 track the lockfiles first. Refusing to write one." >&2
  exit 4
fi

TMP="$(mktemp -t richos-dep-inventory)"
trap 'rm -f "$TMP"' EXIT

if ! python3 "$DIR/lib/dependency_license_inventory.py" "$ROOT" "$TMP" "${LOCKS[@]}"; then
  echo "dependency-license-inventory.sh: generation failed — see the error above." >&2
  exit 5
fi

if [ "$MODE" = "check" ]; then
  if [ ! -f "$OUT" ]; then
    echo "dependency-license-inventory.sh: $OUT does not exist. Run without --check." >&2
    exit 1
  fi
  if diff -u "$OUT" "$TMP" > /dev/null; then
    echo "dependency-license-inventory.sh: $OUT matches the tracked lockfiles."
    exit 0
  fi
  echo "dependency-license-inventory.sh: STALE. The committed inventory no longer describes" >&2
  echo "                                 the resolved graph. Diff (committed vs regenerated):" >&2
  diff -u "$OUT" "$TMP" >&2
  echo "" >&2
  echo "Fix: run this script with no arguments and commit the result." >&2
  exit 1
fi

mkdir -p "$(dirname "$OUT")"
cp "$TMP" "$OUT"
echo "dependency-license-inventory.sh: wrote $OUT"
