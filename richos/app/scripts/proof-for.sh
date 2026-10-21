#!/usr/bin/env bash
#
# proof-for.sh — the smallest set of tests that can prove a change, and the exact
# commands to run them. It PRINTS; it never runs anything.
#
# =========================================================================================
# WHY THIS EXISTS
# =========================================================================================
# After an edit an engineer in this tree has three options and all three are bad: run the
# browser suites, run `run-tests.sh`, or guess. Guessing is what usually happens, and a guess
# is how a fix reaches a candidate without the one suite that would have caught it.
#
# THE TWO COSTS, WITH THE FILE EACH IS READ FROM, because a number in a header with no source
# beside it is a number nobody can check:
#
#   * `ui/tests/suite-weights.tsv`, its own line 10: "55 suite(s), 1510 s serial, measured
#     2026-09-20" — twenty-five minutes, re-measured on this Mac. (Summing the rows gives
#     1512; the file's header is rounded. Either way it is twenty-five minutes.)
#   * `run-tests.sh`, its own header: `make-release.test.sh` and `make-engine-asset.test.sh`
#     are "286 s between them, 30% of a whole build".
#
# AND THE SAVING IS NOT UNIFORM, which is the honest half. Against those weights, summed over
# the suites this script names: the harness1 land (e7facc99) costs ONE suite that measures
# under a second, instead of 1512; the phonelive3 land (315baf03) costs 1076 s of 1512,
# because it touched `ui/mock.js` and the whole-manifest gates read everything (`contrast.js`
# alone is 150 s); the shots1 land (62e5affd) costs all 1512, and the output says which one
# file made it so. A tool that claimed a uniform saving would be lying about the third case.
#
# T3 Code's agent contract is the reference here — *"smallest proof that the change works"*
# and *"do not run repo-wide checks"* (adoption ledger §2.5, ADOPT AS-IS). What that
# contract needs and does not itself supply is a map from a touched file to the suites that
# read it. This script is that map, for the `app/` tree.
#
# =========================================================================================
# THE DESIGN IS NOT NEW, AND DELIBERATELY SO
# =========================================================================================
# `engine/scripts/ci-affected-units.sh` already solved this for the ENGINE tree — path is a
# suite, sibling suite, a suite that names the basename, and an unmapped executable is a
# FAILURE rather than a quiet exit 0 over an empty set. Its own comment says why the app is
# not its business: *"A change to app/ or tools/ has its own workflows and is not this
# gate's business."* So the gap is real, and the answer is its rules applied to a tree whose
# suites are shaped differently — not a second design. Engine paths are not re-implemented
# here at all: they are handed to that script.
#
# =========================================================================================
# WHAT EACH FAMILY'S MAP IS BUILT FROM, AND WHICH PART IS MEASURED
# =========================================================================================
#
#   SCRIPT SUITES   `# run-tests: inputs <paths>` selects suites and feeds the runner's
#                   skip digest. Directory inputs deliberately include future files.
#                   `# run-tests: covers <files>` separately claims behavioral coverage
#                   for exact repository-relative paths, never a scanned tree. Write
#                   `# run-tests: covers -` explicitly when there is no such claim.
#                   Both rows are required exactly once. Each covered file must also
#                   be among its suite's inputs, and a covered file always selects the
#                   suite that covers it (under --gate too, where a directory input
#                   does not select for a tooling path).
#                   Adding an input never silences UNCOVERED. Claims describe assertions
#                   in the suite, not full correctness of every behavior in the file.
#
#   UI SUITES       four rules, three of them derived off disk:
#                     * `ui/tests/<x>.js`        -> that suite (it IS the suite);
#                     * `ui/tests/lib/<x>.js`    -> every suite that requires it, through
#                                                   lib-to-lib requires (`png.js` and
#                                                   `shot-stability.js` are required by no
#                                                   suite directly, only by `harness.js`);
#                     * `ui/tests/shots-<d>/...` -> the suite(s) naming that directory. All
#                                                   13 shot directories have an owner;
#                     * `ui/<shipped source>`    -> DECLARED, in `proof-for.ui-inputs`.
#
#                   THE FOURTH RULE IS DECLARED BECAUSE THE DERIVATION WAS MEASURED AND
#                   DOES NOT WORK. A browser suite does not name the file it exercises; it
#                   opens `index.html` and drives the rendered surface. Measured on this
#                   tree: a basename grep over the 55 suites gives `main.js` 33 suites and
#                   `index.html` 37 — and with comments stripped it gives `home.js` ZERO,
#                   because every mention of `home.js` in `home.js`'s own suite is prose.
#                   Neither number is a map. `lib/ui-sources.js` does not close the gap
#                   either: it is the manifest of what SHIPS, derived from `index.html`,
#                   and it says nothing about which suite reads what. So the relation is
#                   declared — and reconciled in both directions on every run, which is the
#                   difference between a list that IS the inventory and a list checked
#                   against one.
#
#   RUST            `cargo test` for the module: the crate's own unit tests when the file
#                   carries `#[cfg(test)]`, plus every integration target under `tests/`
#                   that names `<crate>::<module>`. Measured: 41 of richos-core's 56 source
#                   modules are named by at least one of its 56 targets that way, and the
#                   other 15 have inline unit tests, so nothing is left without a command.
#
#   PHONE WEB APP   `web/web-app/test/<x>.test.js` is a sibling suite, and the other tests
#                   that name the touched file. `node --test` takes the files directly.
#
#   ENGINE          handed to `engine/scripts/ci-affected-units.sh --paths`.
#
# =========================================================================================
# A FILE THAT MAPS TO NOTHING IS PRINTED, NEVER DROPPED
# =========================================================================================
# The dangerous output is not a diff that maps to too much. It is a diff that maps to
# nothing and exits 0, because then "smallest proof" means "no proof" and it looks the
# same. So every changed path that no rule reaches is printed by name, under one of three
# headings (REMOVED, a path the change deleted, is the third; see `gone()` below):
#
#   UNCOVERED                      code, and no suite claims it. Exit 1. The remedy is a
#                                  suite, or a declaration that is true — never an
#                                  exclusion list here, which would BE the untested surface.
#   NOT PROVEN BY A SUITE          prose, documentation, fixture data, images. Printed with
#                                  its count so it is visible, exit 0. A docs-only change
#                                  mapping to nothing is the correct answer.
#   REMOVED                        deleted by the change: no code is left to prove. Printed
#                                  with its count, exit 0.
#
# Usage:
#   proof-for.sh                      working tree vs the merge-base with origin/main
#   proof-for.sh --staged             what is staged
#   proof-for.sh <sha>                that commit against its first parent
#   proof-for.sh <base>..<head>       that range
#   proof-for.sh --paths <p>[,<p>…]   an explicit list (this is how the suite drives it)
#   options: --explain   print the mapping, path by path, on stderr
#            --quiet     the commands only, nothing else on stdout
#            --gate      the merge gate's selection (see THE MERGE GATE'S SELECTION below)
#            --commit-pending  with <base>..<head> only: <head> stands for a commit not made
#                        yet (autocheck's pre-commit writes the index as one), so it has no
#                        message and the battery question for it belongs to the commit-msg
#                        hook, which reads the real message. Every commit before it is asked.
#
# Exit codes:
#   0  mapped (the command list may legitimately be empty — it says so)
#   1  at least one changed CODE path is covered by nothing
#   2  usage, a diff that could not be read, or a declaration that does not reconcile
#   3  a commit in the range touches richos/mobile/ without `Battery-check: NO — <evidence>`
#      (CEO ruling §81; battery-check.py names each one). Nothing is mapped then.
#
# =========================================================================================
# THE MERGE GATE'S SELECTION (--gate, 2026-09-30)
# =========================================================================================
# A DIRECTORY input (`# run-tests: inputs richos/engine`, a `ui/tests/...` row ending in `/`)
# says "this suite reads the whole tree". For a change to the PRODUCT (the Rust crates, the
# desktop shell, the shipped UI, the phone web app, the phone apps) that is exactly right: the
# suite reads what ships. For a change to TOOLING (scripts, the engine, the test harness,
# documents) it selected whole-product suites: every engine change ran make-engine-asset
# (packaging), proof-for and lint.test.sh, and the merge of 3 mega-lander files ran 45 checks
# for 24 minutes on 2026-09-30. Under --gate a directory input selects its suite only for a
# changed product path; a tooling path selects the suites that cover it, are it, or name it
# exactly. Coverage is unchanged (a directory input never covered anything), so UNCOVERED is
# exactly what it is without --gate. Without --gate nothing changes: the commit check, an
# engineer's own proof and the nightly still get the full selection. A suite that checks every
# file in a tooling directory says so with `# run-tests: gate-inputs <dir>`, and any file under
# it selects that suite under --gate too (proof-qualification and the UI tests, 2026-10-20).
# =========================================================================================

set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="$(cd "$DIR/.." && pwd)"
ROOT="$(git -C "$APP" rev-parse --show-toplevel 2>/dev/null || printf '%s' "$APP")"
APP_REL="$(git -C "$APP" rev-parse --show-prefix 2>/dev/null | sed 's:/$::')"
[ -n "$APP_REL" ] || APP_REL="richos/app"
# The override exists so the suite can point this at a deliberately broken declaration and
# watch it REFUSE. A reconciliation nobody has seen fail is a reconciliation nobody has
# tested.
UI_DECL="${PROOF_FOR_UI_INPUTS:-$DIR/proof-for.ui-inputs}"
SCRIPT_DECL_DIR="${PROOF_FOR_SCRIPT_DIR:-$DIR}"
CI_AFFECTED="$ROOT/richos/engine/scripts/ci-affected-units.sh"

die() { echo "ERROR: proof-for.sh: $1" >&2; exit "${2:-2}"; }

MODE=""; REF=""; PATHS_INLINE=""; EXPLAIN=0; QUIET=0; GATE=0; PENDING=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --gate)    GATE=1; shift ;;
    --commit-pending) PENDING=1; shift ;;
    --staged)  MODE=staged; shift ;;
    --working) MODE=working; shift ;;
    --paths)   [ "$#" -ge 2 ] || die "--paths needs a comma-separated list"
               PATHS_INLINE="$PATHS_INLINE,$2"; MODE=paths; shift 2 ;;
    --explain) EXPLAIN=1; shift ;;
    --quiet)   QUIET=1; shift ;;
    -h|--help) sed -n '/^# Usage:/,/^# =\{10,\}$/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*)        die "unrecognized argument '$1' — see --help" ;;
    *)         [ -z "$REF" ] || die "one commit or range, not two ('$REF' and '$1')"
               REF="$1"; MODE="${MODE:-ref}"; shift ;;
  esac
done

WORK="$(mktemp -d "${TMPDIR:-/tmp}/proof-for.XXXXXX")" || die "could not make a scratch directory"
# §54: the scratch goes, however this ends — including a signal.
trap 'rm -rf "$WORK"' EXIT HUP INT TERM
CHANGED="$WORK/changed"; UNCOVERED="$WORK/uncovered"; PROSE="$WORK/prose"; REMOVED="$WORK/removed"
: > "$CHANGED"; : > "$UNCOVERED"; : > "$PROSE"; : > "$REMOVED"
for f in script ui rust web engine nested ui-index ui-dir-index ui-cause; do : > "$WORK/$f"; done

# --- the changed paths, repository-relative ----------------------------------------------
case "${MODE:-working}" in
  paths)
    printf '%s\n' "${PATHS_INLINE#,}" | tr ',' '\n' > "$CHANGED"
    SOURCE_LABEL="an explicit path list" ;;
  staged)
    git -C "$ROOT" diff --name-only --cached > "$CHANGED" 2>/dev/null \
      || die "could not read the staged diff"
    SOURCE_LABEL="the staged changes" ;;
  ref)
    if printf '%s' "$REF" | grep -q '\.\.'; then
      git -C "$ROOT" diff --name-only "$REF" > "$CHANGED" 2>/dev/null \
        || die "could not read the diff for '$REF'"
      SOURCE_LABEL="$REF"
    else
      git -C "$ROOT" rev-parse --verify --quiet "$REF^{commit}" >/dev/null \
        || die "'$REF' is not a commit in this repository"
      git -C "$ROOT" diff --name-only "$REF^1" "$REF" > "$CHANGED" 2>/dev/null \
        || die "could not read the diff for '$REF' against its first parent"
      SOURCE_LABEL="$REF against its first parent"
    fi ;;
  *)
    BASE="$(git -C "$ROOT" merge-base HEAD origin/main 2>/dev/null \
            || git -C "$ROOT" merge-base HEAD main 2>/dev/null || true)"
    if [ -z "$BASE" ]; then
      git -C "$ROOT" diff --name-only HEAD > "$CHANGED" 2>/dev/null \
        || die "could not read the working diff"
      SOURCE_LABEL="the working tree vs HEAD (no origin/main to compare with)"
    else
      # Uncommitted AND committed-since-the-base: an engineer mid-change has both, and a
      # proof that covered only one half would be a proof of half the change.
      { git -C "$ROOT" diff --name-only "$BASE"
        git -C "$ROOT" ls-files --others --exclude-standard; } > "$CHANGED" 2>/dev/null \
        || die "could not read the working diff"
      SOURCE_LABEL="the working tree vs ${BASE:0:12} (merge-base with main)"
    fi ;;
esac
sed 's/^[[:space:]]*//; s/[[:space:]]*$//' "$CHANGED" | grep -v '^$' | LC_ALL=C sort -u > "$WORK/c2" || true
mv "$WORK/c2" "$CHANGED"

# --- CEO ruling §81: every commit touching richos/mobile/ answers the battery question -------
# Run for a commit, a range (the land: `proof-run.py main..<branch>`) and the committed half of
# the working mode, so an unanswered mobile commit is refused before any suite is selected,
# whoever wrote it. `--paths` and `--staged` name no commits, so there is nothing to read.
# The override exists so the suite can watch this refuse; the land never sets it.
BATTERY_CHECK="${PROOF_FOR_BATTERY_CHECK:-$DIR/battery-check.py}"
BATTERY_RANGE=""
case "${MODE:-working}" in
  ref)   BATTERY_RANGE="$REF" ;;
  paths|staged) ;;
  *)     [ -z "${BASE:-}" ] || BATTERY_RANGE="$BASE..HEAD" ;;
esac
# --commit-pending (2026-10-01): autocheck's pre-commit asks the merge gate's question over
# <base>..<the index written as a commit>. That stand-in has a fixed message and can never carry
# a trailer, so from 50a5bd1bd every commit touching richos/mobile/ was refused at pre-commit
# whatever its message said. Its battery answer is the commit-msg hook's to read (autocheck.py
# commit_msg, battery-check.py --message), so here the range stops at the stand-in's parent and
# every commit already made is still asked. Only a one-parent head qualifies: the flag names one
# commit that does not exist yet, never a merge or a range of real commits.
if [ "$PENDING" = 1 ]; then
  case "${MODE:-}:$REF" in
    ref:*..*) ;;
    *) die "--commit-pending needs a <base>..<head> range whose head is the commit not made yet" ;;
  esac
  PENDING_HEAD="${REF##*..}"
  PENDING_PARENTS="$(git -C "$ROOT" rev-list --parents -n 1 "$PENDING_HEAD" 2>/dev/null | wc -w | tr -d ' ')"
  [ "$PENDING_PARENTS" = 2 ] || die "--commit-pending: $PENDING_HEAD is not a one-parent commit"
  BATTERY_RANGE="${REF%%..*}..$PENDING_HEAD^"
fi
if [ -n "$BATTERY_RANGE" ]; then
  python3 "$BATTERY_CHECK" "$BATTERY_RANGE" > "$WORK/battery" 2>&1; BATTERY_RC=$?
  if [ "$BATTERY_RC" -eq 1 ]; then
    cat "$WORK/battery" >&2
    echo "proof-for.sh: refused — a commit touching richos/mobile/ has not answered the battery question (exit 3)." >&2
    exit 3
  elif [ "$BATTERY_RC" -ne 0 ]; then
    cat "$WORK/battery" >&2
    die "the battery check could not read $BATTERY_RANGE (exit $BATTERY_RC)"
  fi
fi

# --- inventories, read off disk once ------------------------------------------------------
SCRIPT_SUITES="$WORK/script-suites"
find "$SCRIPT_DECL_DIR" -maxdepth 1 -type f -name '*.test.sh' 2>/dev/null \
  | sed "s:^$SCRIPT_DECL_DIR/::" | LC_ALL=C sort > "$SCRIPT_SUITES" || true

UI_TESTS="$APP/ui/tests"
UI_SUITES="$WORK/ui-suites"
if [ -d "$UI_TESTS" ]; then
  find "$UI_TESTS" -maxdepth 1 -type f -name '*.js' 2>/dev/null \
    | sed "s:^$UI_TESTS/::" | grep -v '^run\.js$' | LC_ALL=C sort > "$UI_SUITES" || true
else
  : > "$UI_SUITES"
fi

# --- the declared UI map, reconciled in both directions before it is trusted --------------
# `@allui` is COMPUTED from the tree, not typed: every shipped file under `ui/`, which is
# what the eight manifest-reading suites genuinely read.
ALLUI="$WORK/allui"
if [ -d "$APP/ui" ]; then
  find "$APP/ui" -type f \( -name '*.js' -o -name '*.css' -o -name '*.html' \) \
    -not -path "$APP/ui/tests/*" -not -path '*/node_modules/*' 2>/dev/null \
    | sed "s:^$ROOT/::" | LC_ALL=C sort > "$ALLUI" || true
else
  : > "$ALLUI"
fi

# Parsed into FILES, not associative arrays: the bash every Mac ships is 3.2, where
# `declare -A` is a syntax error. Every other script in this directory is portable to it and
# this one is not allowed to be the exception that only runs on the author's shell.
ALIASDIR="$WORK/alias"; ROWDIR="$WORK/row"; mkdir -p "$ALIASDIR" "$ROWDIR"
ROWORDER="$WORK/row-order"; : > "$ROWORDER"
[ -f "$UI_DECL" ] || die "$UI_DECL is missing — the UI half of the map lives there and cannot be guessed"
# NO SUBPROCESS IN THIS PARSER, AND THE REASON IS MEASURED. The first draft trimmed each
# line with `$(… | sed …)` and read each alias with `$(cat …)` — about 200 forks before a
# single changed path was looked at, which cost 7 SECONDS of fixed startup on a loaded Mac.
# A tool whose whole purpose is to save an engineer a 19-minute decision cannot spend seven
# seconds deciding. `read` splits and trims for free; a plain redirect reads a file without
# forking. Same answer, and the startup is now under a second.
while IFS= read -r line; do
  line="${line%%#*}"
  read -r key rest <<< "$line"
  [ -n "${key:-}" ] || continue
  case "$key" in
    @*) printf '%s\n' "$rest" > "$ALIASDIR/${key#@}" ;;
    *)  printf '%s\t%s\n' "$key" "$rest" >> "$ROWORDER" ;;
  esac
done < "$UI_DECL"

expand_alias() {  # $1 = a row's token list; prints repo-relative paths, one per line
  local tok x val
  for tok in $1; do
    case "$tok" in
      @allui) while IFS= read -r x; do printf '%s\n' "$x"; done < "$ALLUI" ;;
      @*)     if [ -f "$ALIASDIR/${tok#@}" ]; then
                IFS= read -r val < "$ALIASDIR/${tok#@}"; expand_alias "$val"
              else echo "__UNKNOWN_ALIAS__$tok"; fi ;;
      *)      printf '%s\n' "$tok" ;;
    esac
  done
}

# --- the caches --------------------------------------------------------------------------
# EVERY INVENTORY IS READ ONCE. The shots1 fixture is 110 changed paths, and a rule that
# greps 55 suites per path is 6,050 processes — a targeting tool slower than the suite it
# targets is not a targeting tool. Each of the three expensive relations is precomputed or
# memoized below, and the answer is identical either way.
DECLS="$WORK/decls"
python3 "$DIR/lib/proof_declarations.py" "$ROOT" "$SCRIPT_DECL_DIR" > "$DECLS" || exit 2

SHOTSDIR="$WORK/shots"; LIBDIR="$WORK/libs"; mkdir -p "$SHOTSDIR" "$LIBDIR"

UI_SUITES_STR=" "
while IFS= read -r s; do [ -n "$s" ] && UI_SUITES_STR="$UI_SUITES_STR$s "; done < "$UI_SUITES"

RECON="$WORK/recon"; : > "$RECON"
TAB="$(printf '\t')"
while IFS="$TAB" read -r s paths; do
  [ -n "$s" ] || continue
  case "$UI_SUITES_STR" in
    *" $s "*) ;;
    *) echo "  a row names '$s', which is not a suite in ui/tests/" >> "$RECON" ;;
  esac
  if [ -f "$ROWDIR/$s" ]; then echo "  '$s' has more than one row" >> "$RECON"; fi
  expand_alias "$paths" | LC_ALL=C sort -u > "$ROWDIR/$s"
  while IFS= read -r p; do
    [ -n "$p" ] || continue
    case "$p" in
      */)
        # A DIRECTORY INPUT (a token ending in `/`): the suite reads everything under it,
        # including files that do not exist yet. It SELECTS the suite and COVERS nothing,
        # the same rule as `run-tests: inputs` — see the header of proof-for.ui-inputs.
        [ -d "$ROOT/${p%/}" ] || echo "  '$s' names the directory '$p', which is not in the tree" >> "$RECON"
        printf '%s\t%s\n' "$p" "$s" >> "$WORK/ui-dir-index"
        continue ;;
    esac
    printf '%s\t%s\n' "$p" "$s" >> "$WORK/ui-index"
    case "$p" in
      __UNKNOWN_ALIAS__*) echo "  '$s' uses ${p#__UNKNOWN_ALIAS__}, which no line defines" >> "$RECON" ;;
      *) [ -e "$ROOT/$p" ] || echo "  '$s' names '$p', which is not in the tree" >> "$RECON" ;;
    esac
  done < "$ROWDIR/$s"
done < "$ROWORDER"
while IFS= read -r s; do
  [ -n "$s" ] || continue
  [ -s "$ROWDIR/$s" ] || echo "  '$s' is a suite with NO row — it would map to nothing" >> "$RECON"
done < "$UI_SUITES"
if [ -s "$RECON" ]; then
  echo "proof-for.sh: $UI_DECL does not reconcile with the tree:" >&2
  cat "$RECON" >&2
  echo "  A map that has gone stale is worse than no map: it answers, and the answer is wrong." >&2
  exit 2
fi

# --- helpers ------------------------------------------------------------------------------
note() { [ "$EXPLAIN" -eq 1 ] && printf '    %s\n' "$1" >&2; return 0; }

# Every UI hit is recorded WITH THE PATH THAT CAUSED IT, so that when the answer is "all of
# them" the output can say which one file did that. "Run everything" and "run everything
# because you edited the harness 54 suites require" are the same list and different
# information, and the second is the one an engineer can act on.
add_ui() { printf '%s\n' "$1" >> "$WORK/ui"; printf '%s\t%s\n' "$p" "$1" >> "$WORK/ui-cause"; }

ui_suites_requiring() {  # $1 = a lib basename without .js; prints suite file names
  local seed="$1" libs next changed=1 l s
  libs=" $seed "
  while [ "$changed" -eq 1 ]; do
    changed=0
    for l in "$UI_TESTS"/lib/*.js; do
      [ -f "$l" ] || continue
      local b; b="$(basename "$l" .js)"
      case "$libs" in *" $b "*) continue ;; esac
      for seedname in $libs; do
        if grep -qE "require\(\"\./$seedname(\.js)?\"\)|require\('\./$seedname(\.js)?'\)" "$l" 2>/dev/null; then
          libs="$libs$b "; changed=1; break
        fi
      done
    done
  done
  for s in $(cat "$UI_SUITES"); do
    for l in $libs; do
      if grep -qE "require\(\"\./lib/$l(\.js)?\"\)|require\('\./lib/$l(\.js)?'\)" "$UI_TESTS/$s" 2>/dev/null; then
        printf '%s\n' "$s"; break
      fi
    done
  done
}

crate_underscore() { printf '%s' "$1" | tr '-' '_'; }

# What ships (THE MERGE GATE'S SELECTION, above): a directory input selects for these under
# --gate. Everything else (scripts, the engine, the UI test harness, documents) is tooling.
is_product() {  # $1 = repo-relative path
  case "$1" in
    "$APP_REL"/ui/tests/*) return 1 ;;
    "$APP_REL"/crates/*|"$APP_REL"/src-tauri/*|"$APP_REL"/ui/*|"$APP_REL"/third-party/*) return 0 ;;
    "$APP_REL"/updater/*|"$APP_REL"/icon-source/*|"$APP_REL"/Cargo.toml|"$APP_REL"/Cargo.lock) return 0 ;;
    "$APP_REL"/style.css|richos/web/*|richos/mobile/*) return 0 ;;
  esac
  return 1
}
# 0 when a directory input may select its suite for this path.
dir_selects() { [ "$GATE" -eq 0 ] || is_product "$1"; }

is_code() {  # $1 = repo-relative path
  case "$1" in
    *.md|*.txt|*.png|*.jpg|*.jpeg|*.gif|*.svg|*.tsv|*.csv|*.lock|*.webmanifest|*.ttf|*.otf|*.woff*|*.icns|*.wav|*.mp3) return 1 ;;
    *.sh|*.bash|*.py|*.rs|*.js|*.mjs|*.cjs|*.ts|*.css|*.html|*.json|*.toml|*.yml|*.yaml|*.plist|*.kt|*.swift|*.rb) return 0 ;;
  esac
  [ -f "$ROOT/$1" ] || return 1
  head -c 2 "$ROOT/$1" 2>/dev/null | grep -q '^#!' && return 0
  return 1
}

# A PATH THE CHANGE DELETED has no code left to prove, so it is never UNCOVERED (hunt
# 2026-09-29, part 2, section 03: deleting an uncovered script refused every commit and land).
# It is printed under REMOVED, never dropped. "Deleted" is judged against the tree the
# selection reads, and it means the path existed on the base side and is absent now:
#   a commit or range   absent from its head commit (the diff lists only paths the base had);
#   --staged            absent from the index;
#   --paths, working    absent from disk AND the index, and present at HEAD or at a merge-base
#                       with main. A path that never existed (a typo, or a new file named
#                       before it is written) is not a deletion and stays UNCOVERED.
# A deleted path that a suite still declares is refused earlier, by the reconciliation: a real
# deletion edits that declaration in the same change. PROOF_FOR_DELETION_BASE adds one more
# base commit so the suite can watch the --paths rule accept; it can only name a path that is
# already absent from disk and index, so it cannot excuse a file that exists.
case "${MODE:-working}" in
  ref) case "$REF" in *..*) GONE_REV="${REF##*..}" ;; *) GONE_REV="$REF" ;; esac
       GONE_REV="${GONE_REV:-HEAD}" ;;
esac
GONE_BASES=""; GONE_BASES_READ=0
# Called inside the mapping loop, which reads the changed list on stdin: git never gets it.
in_rev() { git -C "$ROOT" cat-file -e "$1:$2" < /dev/null 2>/dev/null; }
in_index() { [ -n "$(git -C "$ROOT" ls-files --cached -- ":(literal)$1" < /dev/null 2>/dev/null)" ]; }
gone() {  # $1 = repo-relative path; 0 when the change deleted it
  local b
  case "${MODE:-working}" in
    ref)    in_rev "$GONE_REV" "$1" && return 1; return 0 ;;
    staged) in_index "$1" && return 1; return 0 ;;
  esac
  { [ -e "$ROOT/$1" ] || [ -L "$ROOT/$1" ] || in_index "$1"; } && return 1
  if [ "$GONE_BASES_READ" -eq 0 ]; then
    GONE_BASES_READ=1
    GONE_BASES="HEAD $(git -C "$ROOT" merge-base HEAD refs/heads/main < /dev/null 2>/dev/null) \
$(git -C "$ROOT" merge-base HEAD origin/main < /dev/null 2>/dev/null) ${PROOF_FOR_DELETION_BASE:-}"
  fi
  for b in $GONE_BASES; do in_rev "$b" "$1" && return 0; done
  return 1
}

# --- the mapping ---------------------------------------------------------------------------
N_CHANGED=0
while IFS= read -r p; do
  [ -n "$p" ] || continue
  N_CHANGED=$((N_CHANGED + 1))
  [ "$EXPLAIN" -eq 1 ] && printf '  %s\n' "$p" >&2
  MATCHED=0
  BASE_NAME="${p##*/}"

  # ---- the engine's own tree: handed over, never re-implemented here ----
  case "$p" in
    richos/engine/*)
      printf '%s\n' "$p" >> "$WORK/engine"; MATCHED=1
      note "engine tree -> ci-affected-units.sh" ;;
  esac

  # ---- teammate definitions: richos-core reads every one of them ----
  # native_driver_tests::a_teammate_s_session_lists_every_skill_its_definition_names walks
  # engine/agents, engine/team/shelf and engine/mega-lander/duties, so a change to any file in
  # them can fail it. The engine's own mapping reaches none of it (a .md maps to nothing), which
  # let a shelf file land on main with the test red (2026-10-21).
  case "$p" in
    richos/engine/agents/*|richos/engine/team/shelf/*|richos/engine/mega-lander/duties/*)
      printf '%s\t%s\t--lib native::native_driver_tests::a_teammate_s_session_lists_every_skill_its_definition_names\n' \
        "richos-core" "unit" >> "$WORK/rust"; MATCHED=1
      note "richos-core reads every teammate definition" ;;
  esac

  # ---- script suites: the suite's own declaration, and the suite itself ----
  case "$p" in
    "$APP_REL"/scripts/*.test.sh)
      # A deleted suite has nothing to run, and `run-tests.sh --only` refuses a name that
      # matches no suite, so selecting it would refuse the land over the deletion itself.
      if gone "$p"; then note "a deleted suite: nothing to run"
      elif grep -q '^# run-tests: select-inputs ' "$SCRIPT_DECL_DIR/$BASE_NAME"; then
        note "runtime suite: its harness has a separate fixture check"
      else printf '%s\n' "$BASE_NAME" >> "$WORK/script"; MATCHED=1; note "is a script suite"; fi ;;
    "$APP_REL"/scripts/*.test.py)
      w="${BASE_NAME%.py}.sh"
      if [ -f "$DIR/$w" ]; then
        printf '%s\n' "$w" >> "$WORK/script"; MATCHED=1; note "wrapped by $w"
      fi ;;
  esac
  # ---- a NESTED runner, which `run-tests.sh` structurally cannot see ----
  # Its inventory is `find -maxdepth 1`, so `scripts/testvm/test/run-tests.sh` — the suite
  # that actually proves the testvm scripts — is in no `--only` list and in no full pass.
  # FOUND BY THIS SCRIPT'S OWN FIXTURES: the harness1 land (e7facc99) changed five testvm
  # scripts, and without this rule they mapped only to two build suites that happen to
  # declare `richos/app/scripts` wholesale — coverage by coincidence, which is the shape of
  # answer this whole script exists to stop giving.
  case "$p" in
    "$APP_REL"/scripts/testvm/output-walk.py)
      # This standalone walk has its own offline boundary checks below. The nested
      # harness never exercises it; shared testvm files keep the full harness rule.
      note "output walk: its declared owning suite, not the unrelated nested harness" ;;
    "$APP_REL"/scripts/*/*)
      sub="${p#"$APP_REL"/scripts/}"; sub="${sub%%/*}"
      if [ -x "$DIR/$sub/test/run-tests.sh" ]; then
        printf '%s\n' "NESTED $sub" >> "$WORK/nested"; MATCHED=1
        note "nested runner scripts/$sub/test/run-tests.sh"
      fi ;;
  esac

  while IFS="$TAB" read -r kind s decl; do
    if [ "$kind" = covers ]; then
      # A covered file also SELECTS the suite that covers it. Without --gate its inputs already
      # do; under --gate a directory input does not select for a tooling path, so before
      # 2026-10-05 a covered file under one (autocheck/install.sh, inside the input
      # `scripts/autocheck`) selected nothing: 480bf8dd5 changed it, the merge never ran
      # autocheck.test.sh, and the nightly found the broken case.
      if [ "$p" = "$decl" ]; then
        printf '%s\n' "$s" >> "$WORK/script"; MATCHED=1; note "behavior covered by $s ($decl)"
      fi
    elif [ "$kind" = gate ]; then
      # `# run-tests: gate-inputs`: the suite checks every file under this directory, so any of
      # them selects it under --gate too (lib/proof_declarations.py, suite_gate_inputs).
      case "$p" in
        "$decl"/*) printf '%s\n' "$s" >> "$WORK/script"; note "a file $s checks ($decl, gate-inputs)" ;;
      esac
    else
      for d in $decl; do
        case "$p" in
          "$d") printf '%s\n' "$s" >> "$WORK/script"; note "input of $s ($d)"; break ;;
          "$d"/*)
            if dir_selects "$p"; then
              printf '%s\n' "$s" >> "$WORK/script"; note "input of $s ($d)"; break
            fi
            note "under a directory $s reads ($d); not selected at the gate for a tooling path" ;;
        esac
      done
    fi
  done < "$DECLS"

  # ---- UI ----
  case "$p" in
    "$APP_REL"/ui/tests/lib/*.js)
      lb="${p##*/}"; lb="${lb%.js}"
      [ -f "$LIBDIR/$lb" ] || ui_suites_requiring "$lb" > "$LIBDIR/$lb"
      while IFS= read -r s; do
        [ -n "$s" ] || continue
        add_ui "$s"; MATCHED=1
      done < "$LIBDIR/$lb"
      note "library -> the suites that require it" ;;
    "$APP_REL"/ui/tests/shots-*/*)
      d="${p%/*}"; d="${d##*/}"
      if [ ! -f "$SHOTSDIR/$d" ]; then
        : > "$SHOTSDIR/$d"
        while IFS= read -r s; do
          [ -n "$s" ] || continue
          # Whole folder name only: shots-5 must not match shots-5b (hunt part 2, R46).
          grep -qE -- "$d([^A-Za-z0-9_-]|\$)" "$UI_TESTS/$s" 2>/dev/null && printf '%s\n' "$s" >> "$SHOTSDIR/$d"
        done < "$UI_SUITES"
      fi
      while IFS= read -r s; do
        [ -n "$s" ] || continue
        add_ui "$s"; MATCHED=1; note "reference shot -> $s"
      done < "$SHOTSDIR/$d" ;;
    "$APP_REL"/ui/tests/*.js)
      case "$UI_SUITES_STR" in *" $BASE_NAME "*) IS_SUITE=1 ;; *) IS_SUITE=0 ;; esac
      if [ "$IS_SUITE" -eq 1 ]; then
        add_ui "$BASE_NAME"; MATCHED=1; note "is a UI suite"
        # Three suites in this directory are also modules of another suite
        # (`waiting-lifecycle` requires `./waiting-state`), so touching one moves two.
        stem="${BASE_NAME%.js}"
        while IFS= read -r s; do
          [ -n "$s" ] || continue
          if grep -qE "require\(\"\./$stem(\.js)?\"\)|require\('\./$stem(\.js)?'\)" "$UI_TESTS/$s" 2>/dev/null; then
            add_ui "$s"; note "required by $s"
          fi
        done < "$UI_SUITES"
      fi ;;
  esac
  # A declared row can name ANY path, not only one under `ui/` — the two suites that read
  # documents rather than the screen are the reason. One indexed lookup covers all of them.
  # `awk` on the whole FIELD, not `grep` on a substring: a substring match would let one
  # declared path whose tail happens to equal another's claim a row that is not its own.
  while IFS= read -r s; do
    [ -n "$s" ] || continue
    add_ui "$s"; MATCHED=1; note "declared by $s"
  done < <(awk -F"$TAB" -v q="$p" '$1 == q { print $2 }' "$WORK/ui-index" 2>/dev/null)
  # A directory input SELECTS and never sets MATCHED: a suite that reads a whole tree (the
  # document-vs-tree claims) must run when anything in it changes, and must not thereby
  # make every file in that tree look covered. Found 2026-09-29: a Rust test file added in
  # 6f5cfb03 made app/README.md's counts false, and docs-claims.js, whose row named eight
  # exact files, was selected by nothing in that land.
  if dir_selects "$p"; then
    while IFS= read -r s; do
      [ -n "$s" ] || continue
      add_ui "$s"; note "under a directory $s reads (selects; covers nothing)"
    done < <(awk -F"$TAB" -v q="$p" 'index(q, $1) == 1 { print $2 }' "$WORK/ui-dir-index" 2>/dev/null)
  fi

  # ---- Rust tests that READ a non-Rust file with include_str! ----
  # A test that scans a shipped file as text (feedback_no_outbound_tests.rs reads ui/main.js,
  # timeline.js, index.html, theme-boot.js and settings-button.js) is invisible to the module
  # rules below, which only look at `.rs` paths: the land of 47ee47308 changed ui/main.js and
  # never ran it, so only the nightly's whole-crate run saw it fail. The relation IS
  # greppable here (the test names the file), so it is derived, not declared: every
  # `include_str!("../../../<path>")` in a crates/*/tests/*.rs, with the leading `../`
  # stripped, is a path relative to app/; a change to that path selects that test target.
  case "$p" in
    "$APP_REL"/crates/*|"$APP_REL"/src-tauri/*) ;;
    "$APP_REL"/*)
      rel="${p#"$APP_REL"/}"
      for t in "$APP"/crates/*/tests/*.rs; do
        [ -f "$t" ] || continue
        if grep -E 'include_str!\("(\.\./)+' "$t" 2>/dev/null \
             | sed -E 's/.*include_str!\("(\.\.\/)+([^"]*)".*/\2/' | grep -qxF -- "$rel"; then
          c="$(basename "$(dirname "$(dirname "$t")")")"
          printf '%s\t%s\t--test %s\n' "$c" "integration" "$(basename "$t" .rs)" >> "$WORK/rust"
          MATCHED=1; note "text-scanned by $(basename "$t") (include_str!)"
        fi
      done ;;
  esac

  # ---- Rust ----
  case "$p" in
    "$APP_REL"/crates/*/tests/*.rs)
      c="$(printf '%s' "$p" | sed "s:^$APP_REL/crates/::; s:/tests/.*::")"
      rel="${p#"$APP_REL/crates/$c/tests/"}"
      if gone "$p"; then
        printf '%s\n' "$p" >> "$REMOVED"
        MATCHED=1; note "deleted integration source: no target remains at this path"
      fi
      case "$rel" in
        */*)
          # Shared modules are compiled by their integration consumers, never as
          # a target named mod. Include Cargo's directory/main.rs layout too.
          testdir="$APP/crates/$c/tests"
          for t in "$testdir"/*.rs "$testdir"/*/main.rs; do
            [ -f "$t" ] || continue
            case "$t" in
              "$testdir"/*/main.rs) target="$(basename "$(dirname "$t")")" ;;
              *) target="$(basename "$t" .rs)" ;;
            esac
            printf '%s\t%s\t--test %s\n' "$c" "integration" "$target" >> "$WORK/rust"
          done
          MATCHED=1; note "integration helper: compile the surviving integration targets of $c" ;;
        *) if ! gone "$p"; then
             printf '%s\t%s\t--test %s\n' "$c" "integration" "$(basename "$p" .rs)" >> "$WORK/rust"
             MATCHED=1; note "is an integration target of $c"
           fi ;;
      esac ;;
    "$APP_REL"/crates/*/src/*.rs)
      c="$(printf '%s' "$p" | sed "s:^$APP_REL/crates/::; s:/src/.*::")"
      m="$(printf '%s' "$p" | sed "s:^$APP_REL/crates/$c/src/::; s:\.rs$::" | awk '{gsub("/","::"); print}')"
      m="${m%::mod}"
      if [ "$m" != "lib" ]; then
        if grep -qE '#\[cfg\(test\)\]' "$ROOT/$p" 2>/dev/null; then
          printf '%s\t%s\t--lib %s::\n' "$c" "unit" "$m" >> "$WORK/rust"; MATCHED=1
          note "$c unit tests for $m"
        fi
        cu="$(crate_underscore "$c")"; top="${m%%::*}"
        for t in "$APP/crates/$c/tests"/*.rs; do
          [ -f "$t" ] || continue
          if grep -qE "${cu}::(${m}|${top})\b" "$t" 2>/dev/null; then
            printf '%s\t%s\t--test %s\n' "$c" "integration" "$(basename "$t" .rs)" >> "$WORK/rust"
            MATCHED=1; note "named by $(basename "$t")"
          fi
        done
      else
        printf '%s\t%s\t--lib\n' "$c" "unit" >> "$WORK/rust"; MATCHED=1; note "$c crate root"
      fi ;;
    "$APP_REL"/crates/*/Cargo.toml)
      c="$(printf '%s' "$p" | sed "s:^$APP_REL/crates/::; s:/Cargo.toml$::")"
      printf '%s\t%s\t\n' "$c" "all" >> "$WORK/rust"; MATCHED=1; note "$c manifest -> the whole crate" ;;
    "$APP_REL"/src-tauri/src/*.rs)
      m="$(printf '%s' "$p" | sed "s:^$APP_REL/src-tauri/src/::; s:\.rs$::" | awk '{gsub("/","::"); print}')"
      m="${m%::mod}"
      if [ "$m" = "main" ]; then
        printf '%s\t%s\t--bin richos-tauri\n' "src-tauri" "unit" >> "$WORK/rust"
      else
        # A `<module>::` filter runs the tests under that module path. A module that declares
        # no test and has no child module matches zero tests, and cargo exits 0 over zero
        # tests, so claiming it would record a pass that proves nothing (hunt 2026-09-29,
        # part 2, section 14). Claim it only when the file declares a test or the module has
        # children (a directory of that name) whose tests the filter also selects; otherwise
        # fall through to UNCOVERED, like the core-crate branch above.
        sfile="$ROOT/$p"; sdir="${sfile%.rs}"; sdir="${sdir%/mod}"
        if grep -qE '#\[(cfg\(test\)|test|tokio::test)\]' "$sfile" 2>/dev/null \
           || [ -d "$sdir" ]; then
          printf '%s\t%s\t--bin richos-tauri %s::\n' "src-tauri" "unit" "$m" >> "$WORK/rust"
          MATCHED=1; note "the shell's own unit tests for $m"
        fi
      fi
      [ "$m" != "main" ] || { MATCHED=1; note "the shell's own unit tests for $m"; } ;;
    "$APP_REL"/src-tauri/Cargo.toml|"$APP_REL"/src-tauri/build.rs)
      printf '%s\t%s\t--bin richos-tauri\n' "src-tauri" "all" >> "$WORK/rust"; MATCHED=1 ;;
  esac

  # ---- the phone web app ----
  case "$p" in
    richos/web/web-app/test/*.test.js)
      printf '%s\n' "test/$BASE_NAME" >> "$WORK/web"; MATCHED=1; note "is a web-app suite" ;;
    richos/web/web-app/*)
      for t in "$ROOT"/richos/web/web-app/test/*.test.js; do
        [ -f "$t" ] || continue
        if grep -qF -- "$BASE_NAME" "$t" 2>/dev/null; then
          printf '%s\n' "test/$(basename "$t")" >> "$WORK/web"; MATCHED=1; note "named by $(basename "$t")"
        fi
      done ;;
  esac

  if [ "$MATCHED" -eq 0 ]; then
    if gone "$p"; then printf '%s\n' "$p" >> "$REMOVED"; note "deleted by this change: nothing left to prove"
    elif is_code "$p"; then printf '%s\n' "$p" >> "$UNCOVERED"; note "NO SUITE COVERS IT (code)"
    else printf '%s\n' "$p" >> "$PROSE"; note "prose or data — no suite by design"; fi
  fi
done < "$CHANGED"

# --- output ---------------------------------------------------------------------------------
say() { [ "$QUIET" -eq 1 ] || printf '%s\n' "$1"; }
cmd() { printf '  %s\n' "$1"; }

for f in script ui web nested; do
  LC_ALL=C sort -u "$WORK/$f" > "$WORK/$f.u" 2>/dev/null || : > "$WORK/$f.u"
  mv "$WORK/$f.u" "$WORK/$f"
done
LC_ALL=C sort -u "$WORK/rust" > "$WORK/rust.u" 2>/dev/null || : > "$WORK/rust.u"
mv "$WORK/rust.u" "$WORK/rust"

N_UI="$(grep -c . "$WORK/ui" || true)"; N_SCRIPT="$(grep -c . "$WORK/script" || true)"
N_RUST="$(grep -c . "$WORK/rust" || true)"; N_WEB="$(grep -c . "$WORK/web" || true)"
N_UNCOV="$(grep -c . "$UNCOVERED" || true)"; N_PROSE="$(grep -c . "$PROSE" || true)"
N_REMOVED="$(grep -c . "$REMOVED" || true)"

say ""
say "$N_CHANGED changed path(s) — $SOURCE_LABEL"
say ""

if [ "${N_RUST:-0}" -gt 0 ]; then
  say "RUST — $N_RUST target(s)"
  while IFS="$(printf '\t')" read -r c kind args; do
    [ -n "$c" ] || continue
    case "$c" in
      src-tauri) cmd "cd $APP_REL/src-tauri && cargo test $args" ;;
      richos-core|richos-voice)
        if [ -n "$args" ]; then cmd "cd $APP_REL && cargo test -p $c $args"
        else cmd "cd $APP_REL && cargo test -p $c"; fi ;;
      *)
        if [ -n "$args" ]; then cmd "cd $APP_REL && cargo test --manifest-path crates/$c/Cargo.toml $args"
        else cmd "cd $APP_REL && cargo test --manifest-path crates/$c/Cargo.toml"; fi ;;
    esac
  done < "$WORK/rust"
  say ""
fi

if [ "${N_UI:-0}" -gt 0 ]; then
  say "UI SUITES — $N_UI of $(grep -c . "$UI_SUITES" || echo 0)"
  TOTAL_UI="$(grep -c . "$UI_SUITES" || echo 0)"
  if [ "$N_UI" -gt $((TOTAL_UI / 2)) ]; then
    WIDEST="$(awk -F"\t" '{print $1}' "$WORK/ui-cause" | LC_ALL=C sort | uniq -c \
              | LC_ALL=C sort -rn | head -1)"
    say "  (widest cause:$(printf '%s' "$WIDEST" | awk '{print " " $2 " reaches " $1}'))"
  fi
  while IFS= read -r s; do
    [ -n "$s" ] || continue
    cmd "cd $APP_REL/ui/tests && node $s"
  done < "$WORK/ui"
  say ""
fi

if [ "${N_WEB:-0}" -gt 0 ]; then
  say "PHONE WEB APP — $N_WEB"
  cmd "cd richos/web/web-app && node --test $(tr '\n' ' ' < "$WORK/web" | sed 's/[[:space:]]*$//')"
  say ""
fi

if [ -s "$WORK/nested" ]; then
  say "NESTED SUITE — $(grep -c . "$WORK/nested")"
  say "  (its own runner; run-tests.sh discovers at maxdepth 1 and never reaches it)"
  while IFS=' ' read -r _ sub; do
    [ -n "$sub" ] && cmd "cd $APP_REL && bash scripts/$sub/test/run-tests.sh"
  done < "$WORK/nested"
  say ""
fi

if [ "${N_SCRIPT:-0}" -gt 0 ]; then
  say "BUILD AND PACKAGING SUITES — $N_SCRIPT of $(grep -c . "$SCRIPT_SUITES" || echo 0)"
  say "  (heavier; these are the proof a candidate needs, not the proof an edit needs)"
  ONLY=""
  while IFS= read -r s; do
    [ -n "$s" ] || continue
    if [ "$s" = "make-engine-asset.test.sh" ]; then
      say '  Prerequisite: RICHOS_RUNTIME_DIR points to a prepared runtime matching scripts/runtime-sources.json.'
      cmd "cd $APP_REL && bash scripts/make-engine-asset.test.sh"
    elif [ "$s" = "native-ios-ui.test.sh" ]; then
      CASES="$(python3 "$DIR/lib/ios_ui_scope.py" "$ROOT" "$CHANGED")" || die "could not scope iPhone UI cases"
      cmd "cd $APP_REL && bash scripts/native-ios-ui.test.sh$([ -z "$CASES" ] || printf ' %s' "$CASES")"
    else
      ONLY="$ONLY --only $s"
    fi
  done < "$WORK/script"
  [ -z "$ONLY" ] || cmd "cd $APP_REL && scripts/run-tests.sh --no-host-screen$ONLY"
  say ""
fi

if [ -s "$WORK/engine" ]; then
  say "ENGINE — $(grep -c . "$WORK/engine") path(s), mapped by the engine's own script"
  if [ -x "$CI_AFFECTED" ]; then
    LIST="$(tr '\n' ',' < "$WORK/engine" | sed 's/,$//')"
    ENGINE_ARGS=(--paths "$LIST" --strict)
    case "${MODE:-working}" in
      staged) ENGINE_ARGS+=(--staged) ;;
      ref) ENGINE_ARGS+=(--range "$REF") ;;
      paths) ;;
      *) ENGINE_ARGS+=(--working --base "${BASE:-HEAD}") ;;
    esac
    [ "$EXPLAIN" -eq 0 ] || ENGINE_ARGS+=(--explain)
    UNITS="$("$CI_AFFECTED" "${ENGINE_ARGS[@]}")" || die "engine selection failed; no executable plan was accepted"
    if [ -n "$UNITS" ]; then
      printf '%s\n' "$UNITS" | while IFS= read -r u; do
        [ -n "$u" ] && cmd "cd richos/engine && bash scripts/ci-shard.sh --only-units $u"
      done
    else
      say "  (that script maps these to no unit — see it with --explain)"
    fi
  else
    say "  $CI_AFFECTED is not here; run it yourself against these paths."
  fi
  say ""
fi

if [ "${N_UI:-0}" -eq 0 ] && [ "${N_SCRIPT:-0}" -eq 0 ] && [ "${N_RUST:-0}" -eq 0 ] \
   && [ "${N_WEB:-0}" -eq 0 ] && [ ! -s "$WORK/engine" ] && [ ! -s "$WORK/nested" ]; then
  say "NOTHING TO RUN — no suite in this tree reads anything this diff touches."
  say "For a documentation-only change that is the right answer, and it is printed"
  say "rather than left implicit."
  say ""
fi

if [ "${N_UNCOV:-0}" -eq 0 ] && { [ "${N_UI:-0}" -gt 0 ] || [ "${N_SCRIPT:-0}" -gt 0 ] || [ "${N_RUST:-0}" -gt 0 ] \
   || [ "${N_WEB:-0}" -gt 0 ] || [ -s "$WORK/engine" ] || [ -s "$WORK/nested" ]; }; then
  # This script prints; `proof-run.py` owns admission, deadlines and cancellation for the plan.
  # One prose line, deliberately not an indented `cd` line: every such line above is a
  # command, and proof-for.test.sh I1 parses each of them as one.
  say "TO RUN ALL OF IT AT ONCE (admitted by CPU, one log per check, one summary): from $APP_REL,"
  say "scripts/proof-run.py with the same arguments you gave this script."
  say ""
fi

if [ "${N_PROSE:-0}" -gt 0 ]; then
  say "NOT PROVEN BY A SUITE — $N_PROSE (prose, documentation, fixture data, images)"
  [ "$QUIET" -eq 1 ] || sed 's/^/  /' "$PROSE"
  say ""
fi

if [ "${N_REMOVED:-0}" -gt 0 ]; then
  say "REMOVED — $N_REMOVED (deleted by this change; no code left to prove)"
  [ "$QUIET" -eq 1 ] || sed 's/^/  /' "$REMOVED"
  say ""
fi

if [ "${N_UNCOV:-0}" -gt 0 ]; then
  echo "UNCOVERED — $N_UNCOV changed code path(s) that no suite claims:" >&2
  sed 's/^/  /' "$UNCOVERED" >&2
  echo "  Give one a suite, or make a declaration name it truthfully." >&2
  echo "  Do NOT add it to an exclusion list in this script: the list would be the" >&2
  echo "  untested surface, wearing the word 'covered'." >&2
  exit 1
fi
exit 0
