#!/usr/bin/env bash
#
# publication-completeness.test.sh — regression tests for the publication
# COMPLETENESS contract: scripts/publication-completeness.sh and its analyser.
#
# THE FOUR FIXTURES ARE THE FOUR REAL FAILURES OF 2026-08-29, replayed. None of
# them is invented; each is the pre-fix state of something that actually
# shipped, reduced to the smallest tree that still reproduces it:
#
#   (1) SHIPPED INERT — a guard gated on a declaration file, with no instance,
#       no template, and no word of it in anything an adopter reads. The real
#       one was the whole ceo-todos mechanism.
#   (2) MECHANISM IN THE PRIVATE TREE — an executable in the private repo that
#       reads a contract the public tree enforces. The real one was
#       render-ceo-todos.mjs.
#   (3) UNREACHABLE BY CONSTRUCTION — a workflow under a subdirectory, which
#       GitHub Actions never discovers. The real one had never executed once.
#   (4) A CLAIM WITH NOTHING BEHIND IT — a document citing a path that exists
#       nowhere. The real one was companion-windows/README.md's Windows CI.
#
# THE POSITIVE CONTROLS MATTER MORE THAN THE FOUR, and there are more of them
# here for that reason. A checker that fires on everything gets switched off and
# then protects nothing, so every fixture below is paired with the nearest thing
# that must NOT fire: the fixed version of itself, plus the legitimate
# neighbors that look similar — a citation into a foreign tree, a citation of a
# gitignored file, private INSTANCE DATA next to a private MECHANISM, a private
# mechanism coupled to nothing public, a subdirectory workflow that a root
# workflow does cover.
#
# Covers:
#   (a) STAND-DOWN. No .publication-boundary, no opinion. This is the precision
#       floor — get it wrong and the check fires in every repository on the box.
#   (b) A COMPLETE TREE PASSES, and an ordinary commit in one is untouched
#       (this contract adds no hook and no commit-time gate, by design).
#   (c) The four fixtures FAIL, each with its own check name.
#   (d) The fixed form of each of the four PASSES.
#   (e) MECHANISM vs INSTANCE DATA is decided structurally, not by opinion.
#   (f) EXEMPTIONS work, and a STALE exemption FAILS — the property that makes
#       the escape hatch safe to have at all.
#   (f2) THE VERDICT ON THE PUBLIC TREE DOES NOT MOVE WITH THE HOST. The excuse
#       for a private-tree mechanism lives IN THAT FILE (`instance-mechanism:
#       <reason>`), never in the published declaration, because a declaration
#       entry naming a private path is judged stale in any clone that does not
#       have that private tree — which made this gate pass on one machine and
#       fail on every other. The suite asserts the same public tree passes with
#       the private tree present AND with it gone, that a bare marker exempts
#       nothing, that a marker excusing a coupling that no longer exists FAILS,
#       and that the retired key is refused by name rather than ignored.
#   (g) NO SILENT DEGRADATION: unknown key, malformed line, empty tree and a
#       non-repository all exit 2, never 0.
#
# A NOTE ON THE HARNESS. Every assertion here matches with a HERE-STRING
# (`grep -q -- "$needle" <<<"$text"`), never `printf ... | grep -q`. The first
# version used pipes and was FLAKY: `grep -q` exits the instant it matches and
# closes the pipe, the writer takes SIGPIPE, and under `set -o pipefail` the
# pipeline's status is the writer's 141 rather than grep's 0. One case failed
# roughly one run in three, with a diagnostic that printed the very text it had
# just claimed not to find. A suite whose failures are not about its subject is
# worse than no suite — it teaches the reader to disbelieve red.
#
# Findings are also matched against a whitespace-FLATTENED copy of the output,
# because the checker wraps long messages at 96 columns for readability and a
# phrase assertion must not fail on where a line happened to break.
#
# Run directly: scripts/publication-completeness.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CHECK="$SCRIPT_DIR/publication-completeness.sh"

PASS=0
FAIL=0
# pwd -P: macOS symlinks /tmp and /var, and git always answers with a physical
# path. The same normalization every suite in this engine applies, for the same
# reason — a prefix comparison between a typed path and a git-derived one
# silently stops matching otherwise.
SCRATCH="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/pubcomplete.XXXXXX")" && pwd -P)"
trap 'rm -rf "$SCRATCH"' EXIT

ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

[ -x "$CHECK" ] || { echo "FATAL: $CHECK is missing or not executable." >&2; exit 1; }

# ---------------------------------------------------------------------------
# run <tree>  — the checker's stdout+stderr in $OUT, its exit code in $RC.
# ---------------------------------------------------------------------------
OUT=""; RC=0; FLAT=""
run() {
    OUT="$("$CHECK" --root "$1" 2>&1)"
    RC=$?
    # Findings are printed through `fold -s -w 96` so a long message stays
    # readable in a terminal. That wraps mid-phrase, so an assertion matching a
    # phrase against the raw output fails for a formatting reason and says
    # nothing true about the check. Match against a whitespace-flattened copy.
    FLAT="$(printf '%s' "$OUT" | tr '\n' ' ' | tr -s ' ')"
}

# assert_finding <label> <tree> <CHECK-NAME> <substring>
assert_finding() {
    local label="$1" tree="$2" check="$3" needle="$4"
    run "$tree"
    if [ "$RC" -ne 1 ]; then
        bad "$label (expected exit 1, got $RC)"; printf '%s\n' "$OUT" | sed 's/^/        /'; return
    fi
    if ! grep -q "\\[$check\\]" <<<"$OUT"; then
        bad "$label (no [$check] finding)"; printf '%s\n' "$OUT" | sed 's/^/        /'; return
    fi
    if [ -n "$needle" ] && ! grep -q -- "$needle" <<<"$FLAT"; then
        bad "$label (finding did not name '$needle')"; printf '%s\n' "$OUT" | sed 's/^/        /'; return
    fi
    ok "$label"
}

# assert_clean <label> <tree>
assert_clean() {
    run "$2"
    if [ "$RC" -eq 0 ]; then ok "$2 → clean: $1"
    else bad "$1 (expected exit 0, got $RC)"; printf '%s\n' "$OUT" | sed 's/^/        /'; fi
}

# assert_rc <label> <tree> <expected-rc> [substring]
assert_rc() {
    local label="$1" tree="$2" want="$3" needle="${4:-}"
    run "$tree"
    if [ "$RC" -ne "$want" ]; then
        bad "$label (expected exit $want, got $RC)"; printf '%s\n' "$OUT" | sed 's/^/        /'; return
    fi
    if [ -n "$needle" ] && ! grep -qi -- "$needle" <<<"$FLAT"; then
        bad "$label (message did not mention '$needle')"; printf '%s\n' "$OUT" | sed 's/^/        /'; return
    fi
    ok "$label"
}

# ---------------------------------------------------------------------------
# mktree <name> — a minimal COMPLETE publication-bound tree, then commit it.
#
# Complete means: it declares itself publication-bound; the one declaration-
# gated mechanism it ships has both a template and a README mention; its README
# cites only paths that resolve; and it has no workflows at all. Every fixture
# below is this tree with exactly one thing broken, so a failure can only be the
# thing the case is about.
# ---------------------------------------------------------------------------
mktree() {
    local t="$SCRATCH/$1"
    mkdir -p "$t/scripts/hooks" "$t/src"
    git -C "$t" init -q 2>/dev/null || git init -q "$t"
    printf 'build/\n*.generated.md\n' > "$t/.gitignore"

    cat > "$t/.publication-boundary" <<'EOF'
PRIVATE_RECORD="the private HQ repository"
PRIVATE_SOURCES=""
EOF

    # The shipped, declaration-gated mechanism. It names its own gate in the
    # convention the checker derives from — the same one both real guards use.
    cat > "$t/scripts/hooks/guard-widget.sh" <<'EOF'
#!/usr/bin/env bash
: "${WIDGET_DECLARATION:=.widget}"
[ -f "$PWD/$WIDGET_DECLARATION" ] || exit 0
echo "widget guard active"
EOF
    chmod +x "$t/scripts/hooks/guard-widget.sh"

    # Both arms satisfied: a copyable template, and a word of it where an
    # adopter reads.
    printf 'WIDGET_RECORD="wherever"\n' > "$t/.widget.example"
    cat > "$t/README.md" <<'EOF'
# Sample

Create a `.widget` file at your repository root to switch the widget guard on;
copy `.widget.example` and edit it. The guard itself is
`scripts/hooks/guard-widget.sh`.

Adopter note: in YOUR repository this drives `app/widgets/index.ts`, which is
not part of this tree.

The generated summary lands at `build/report.md` and is not committed.
EOF
    printf 'VERSION 1\n' > "$t/VERSION"
    printf 'console.log("hi");\n' > "$t/src/main.js"
    git -C "$t" add -A >/dev/null 2>&1
    git -C "$t" commit -qm "sample tree" >/dev/null 2>&1
    printf '%s' "$t"
}

commit_all() { git -C "$1" add -A >/dev/null 2>&1; git -C "$1" commit -qm "case" >/dev/null 2>&1; }

echo "=== publication-completeness.test.sh ==="

# ---------------------------------------------------------------------------
echo "--- (a) stand-down: adoption is DECLARED, never inferred"
# ---------------------------------------------------------------------------
T="$(mktree standdown)"
rm "$T/.publication-boundary"; commit_all "$T"
assert_rc "no .publication-boundary → NOT APPLICABLE, exit 2, never a quiet pass" \
    "$T" 2 "NOT APPLICABLE"

NOREPO="$SCRATCH/norepo"; mkdir -p "$NOREPO"
assert_rc "a directory that is not a git repository → exit 2" "$NOREPO" 2 "not inside a git repository"

# ---------------------------------------------------------------------------
echo "--- (b) the positive control that everything else is measured against"
# ---------------------------------------------------------------------------
BASE="$(mktree base)"
assert_clean "a complete tree passes" "$BASE"

# An ordinary commit is untouched. This contract deliberately adds NO hook and
# NO commit-time gate — it is a command and a CI step — so the proof is that a
# commit in a governed tree still succeeds and that nothing here is registered
# in the engine's hook table.
printf 'more\n' >> "$BASE/src/main.js"
commit_all "$BASE"; CRC=$?
BASE_LOG="$(git -C "$BASE" log --oneline)"
if [ "$CRC" -eq 0 ] && grep -q "case" <<<"$BASE_LOG"; then
    ok "an ordinary commit in a governed tree is unaffected"
else
    bad "an ordinary commit in a governed tree was blocked or lost (rc=$CRC) [$(git -C "$BASE" log --oneline | tr "\n" "; ")]"
fi
if grep -q 'publication-completeness' "$ENGINE_ROOT/hooks/hooks.json" 2>/dev/null; then
    bad "publication-completeness is registered as a hook — it must not be; a completeness check that blocks work in progress gets switched off"
else
    ok "publication-completeness registers no hook (a command + a CI step, by design)"
fi

# ---------------------------------------------------------------------------
echo "--- (c/d) FIXTURE 1 — SHIPPED INERT, and its fixed form"
# ---------------------------------------------------------------------------
T="$(mktree inert)"
rm "$T/.widget.example"
perl -0pi -e 's/Create a `\.widget`.*?`scripts\/hooks\/guard-widget\.sh`\.\n/The guard is `scripts\/hooks\/guard-widget.sh`.\n/s' "$T/README.md"
commit_all "$T"
assert_finding "enforcement with no declaration template AND no mention → INERT" \
    "$T" INERT "no copyable instance or template"
assert_finding "...and the same finding names the missing documentation" \
    "$T" INERT "named in no document an adopter reads"

T="$(mktree inert_template_only)"
perl -0pi -e 's/Create a `\.widget`.*?`scripts\/hooks\/guard-widget\.sh`\.\n/The guard is `scripts\/hooks\/guard-widget.sh`.\n/s' "$T/README.md"
commit_all "$T"
assert_finding "a template with no onboarding mention is STILL inert" \
    "$T" INERT "named in no document an adopter reads"

T="$(mktree inert_doc_only)"
rm "$T/.widget.example"
perl -pi -e 's/copy `\.widget\.example` and edit it\./write it by hand./' "$T/README.md"
commit_all "$T"
assert_finding "an onboarding mention with no copyable template is STILL inert" \
    "$T" INERT "no copyable instance or template"

# The fixed form is $BASE, already asserted clean above. Two more shapes of
# "fixed" that must also pass, so the arm is not accidentally exact-match:
T="$(mktree inert_fixed_instance)"
rm "$T/.widget.example"
printf 'WIDGET_RECORD="here"\n' > "$T/.widget"
perl -pi -e 's/copy `\.widget\.example` and edit it\./copy this repo.s own./' "$T/README.md"
commit_all "$T"
assert_clean "a real committed instance counts as copyable, not just a .example" "$T"

# A dotfile's template is usually shipped WITHOUT the dot, so a person
# browsing the tree can see it. The real `.ceo-todos` template shipped as
# reference/ceo-todos/ceo-todos.example, and this check called it missing.
T="$(mktree inert_fixed_undotted)"
rm "$T/.widget.example"
mkdir -p "$T/reference/widget"
printf 'WIDGET_RECORD="x"\n' > "$T/reference/widget/widget.example"
perl -pi -e 's/copy `\.widget\.example` and edit it\./copy reference\/widget\/widget.example./' "$T/README.md"
commit_all "$T"
assert_clean "a dot-stripped template (widget.example) counts as copyable" "$T"

# ...but only with an explicit template suffix. A file that merely shares the
# stem is not a template, and must not satisfy the arm.
T="$(mktree inert_stem_collision)"
rm "$T/.widget.example"
printf 'unrelated\n' > "$T/src/widget"
perl -pi -e 's/copy `\.widget\.example` and edit it\./write it by hand./' "$T/README.md"
commit_all "$T"
assert_finding "a bare stem collision (src/widget) is NOT a template" \
    "$T" INERT "no copyable instance or template"

T="$(mktree inert_fixed_skill)"
rm "$T/.widget.example"
printf 'WIDGET_RECORD="x"\n' > "$T/.widget.template"
perl -0pi -e 's/Create a `\.widget`.*?`scripts\/hooks\/guard-widget\.sh`\.\n/The guard is `scripts\/hooks\/guard-widget.sh`.\n/s' "$T/README.md"
mkdir -p "$T/skills/setup"
printf -- '---\nname: setup\n---\nRun the setup, then create `.widget` from `.widget.template`.\n' \
    > "$T/skills/setup/SKILL.md"
commit_all "$T"
assert_clean "a SKILL.md counts as onboarding (agents read skills, not just READMEs)" "$T"

# A CHANGELOG must NOT count as onboarding — the real .ceo-todos was named in
# exactly one published document, the one recording that it happened.
T="$(mktree inert_changelog)"
rm "$T/.widget.example"
perl -0pi -e 's/Create a `\.widget`.*?`scripts\/hooks\/guard-widget\.sh`\.\n/The guard is `scripts\/hooks\/guard-widget.sh`.\n/s' "$T/README.md"
printf '# Changelog\n\n- Added the `.widget` declaration and its guard.\n' > "$T/CHANGELOG.md"
printf '\nSee the [changelog](CHANGELOG.md).\n' >> "$T/README.md"
commit_all "$T"
assert_finding "a CHANGELOG mention does NOT satisfy onboarding" \
    "$T" INERT "named in no document an adopter reads"

# A declaration that exists only inside a TEST FIXTURE is not a shipped
# capability. This suite is itself the regression: its own fixtures declare
# `.widget`, and before this rule existed the first end-to-end ci-verify run
# reported the ENGINE as shipping inert `.widget` enforcement.
T="$(mktree fixture_decl)"
cat > "$T/scripts/hooks/other.test.sh" <<'EOF'
#!/usr/bin/env bash
# A suite that builds a synthetic tree declaring a synthetic gate.
: "${SYNTHETIC_DECLARATION:=.synthetic}"
echo "$SYNTHETIC_DECLARATION"
EOF
commit_all "$T"
assert_clean "a declaration appearing only in a *.test.sh is NOT a shipped capability" "$T"

# A COMMENT quoting the convention is documentation, not a definition. The
# analyser's own header quotes both real declarations verbatim, and before this
# rule it registered as a source of one of them.
T="$(mktree commented_decl)"
cat > "$T/scripts/hooks/doc-only.sh" <<'EOF'
#!/usr/bin/env bash
# Guards in this engine name their own gate, e.g.
#   : "${DOCUMENTED_DECLARATION:=.documented}"
# and the checker derives the set from that convention.
echo "no gate here"
EOF
commit_all "$T"
assert_clean "a declaration quoted only in a COMMENT is documentation, not a capability" "$T"

# ---------------------------------------------------------------------------
echo "--- (c/d) FIXTURE 4 — A CLAIM WITH NOTHING BEHIND IT, and its neighbors"
# ---------------------------------------------------------------------------
T="$(mktree dangling)"
mkdir -p "$T/.github/workflows"
cat > "$T/.github/workflows/real.yml" <<'EOF'
name: real
on: [push]
jobs:
  v:
    runs-on: ubuntu-latest
    steps:
      - run: scripts/ci-verify.sh
EOF
printf '\nCI runs exactly this (`.github/workflows/windows-ci.yml`).\n' >> "$T/README.md"
commit_all "$T"
assert_finding "a README citing a workflow that exists nowhere → CITATION" \
    "$T" CITATION "windows-ci.yml"

T="$(mktree cite_ok)"
cat >> "$T/README.md" <<'EOF'

Resolves: `scripts/hooks/guard-widget.sh` and `src/main.js`.
Foreign tree, deliberately not ours: `vendor/acme/thing.rb`, `src2/app/x.ts`.
Generated, gitignored, correctly absent: `build/report.md`.
Runtime argument, not a repo path: `./input.png`.
A filename TEMPLATE, not a file: `reports/AUDIT_YYYY-MM-DD.md`.
EOF
commit_all "$T"
assert_clean "resolving, foreign, gitignored, runtime-relative and placeholder citations are all clean" "$T"

# The private record is the sanctioned destination for a moved document, and
# naming it must not read as a broken claim — this is the fix five real
# citations took when the boundary moved their targets out.
T="$(mktree cite_private)"
printf '\nMoved out of the public tree: `private-hq/docs/plans/thing.md`.\n' >> "$T/README.md"
commit_all "$T"
assert_clean "a citation that names the private record by prefix is clean" "$T"

# An UNTRACKED file is not in the published tree, however present it is on the
# author's disk. This is the whole reason the tree is derived from git.
T="$(mktree cite_untracked)"
printf 'x\n' > "$T/src/uncommitted.js"
printf '\nSee `src/uncommitted.js`.\n' >> "$T/README.md"
git -C "$T" add README.md >/dev/null 2>&1
git -C "$T" commit -qm "cite an untracked sibling" >/dev/null 2>&1
assert_finding "a citation of a file present on disk but never committed → CITATION" \
    "$T" CITATION "uncommitted.js"

# ---------------------------------------------------------------------------
echo "--- (c/d) FIXTURE 3 — UNREACHABLE BY CONSTRUCTION, and its fixed form"
# ---------------------------------------------------------------------------
T="$(mktree wf_unreachable)"
mkdir -p "$T/sub/.github/workflows"
cat > "$T/sub/.github/workflows/verify.yml" <<'EOF'
name: verify
on: [push]
jobs:
  v:
    runs-on: ubuntu-latest
    steps:
      - run: scripts/ci-verify.sh
EOF
commit_all "$T"
assert_finding "a workflow under a subdirectory with NO root workflow → UNREACHABLE" \
    "$T" UNREACHABLE "never executed"

T="$(mktree wf_uncovered)"
mkdir -p "$T/sub/.github/workflows" "$T/.github/workflows"
cat > "$T/sub/.github/workflows/verify.yml" <<'EOF'
name: verify
on: [push]
jobs:
  v:
    runs-on: ubuntu-latest
    steps:
      - run: scripts/ci-verify.sh
EOF
cat > "$T/.github/workflows/other.yml" <<'EOF'
name: other
on: [push]
jobs:
  o:
    runs-on: ubuntu-latest
    steps:
      - run: scripts/something-else.sh
EOF
commit_all "$T"
assert_finding "a root workflow running something ELSE does not cover the template" \
    "$T" UNREACHABLE "no root-level workflow runs"

T="$(mktree wf_fixed)"
mkdir -p "$T/sub/.github/workflows" "$T/.github/workflows"
cat > "$T/sub/.github/workflows/verify.yml" <<'EOF'
name: verify
on: [push]
jobs:
  v:
    runs-on: ubuntu-latest
    steps:
      - run: scripts/ci-verify.sh
EOF
# Deliberately NOT byte-identical: the real root copy carries
# `working-directory`, because the engine lives in a subdirectory there. The
# two copies are compared on what they INVOKE, never on being the same file.
cat > "$T/.github/workflows/verify.yml" <<'EOF'
name: verify
on: [push]
jobs:
  v:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: engine
    steps:
      - run: scripts/ci-verify.sh
EOF
commit_all "$T"
assert_clean "a template covered by a root workflow running the same entry point passes" "$T"

# ---------------------------------------------------------------------------
echo "--- (c/d/e) FIXTURE 2 — MECHANISM IN THE PRIVATE TREE"
# ---------------------------------------------------------------------------
mkprivate() {
    local p="$SCRATCH/$1"
    mkdir -p "$p/scripts"
    printf '%s' "$p"
}

T="$(mktree misplaced)"
P="$(mkprivate hq_misplaced)"
cat > "$P/scripts/render-widget.mjs" <<'EOF'
#!/usr/bin/env node
// Renders the widget record. The format is defined by .widget in the public tree.
import fs from "node:fs";
console.log(fs.readFileSync(".widget", "utf8"));
EOF
printf 'PRIVATE_RECORD="hq"\n# machine-path-exempt: isolated publication fixture, not an operational default\nPRIVATE_SOURCES="%s"\n' "$P" > "$T/.publication-boundary"
commit_all "$T"
assert_finding "an executable in the private tree reading a public contract → MISPLACED" \
    "$T" MISPLACED "render-widget.mjs"

# INSTANCE DATA is not a mechanism, and the distinction is made STRUCTURALLY —
# by executable shape — not by anybody's opinion about the file.
P2="$(mkprivate hq_data)"
printf 'WIDGET_RECORD="one company"\n' > "$P2/.widget"
printf '# The Widget Queue\n\nOur own `.widget` items.\n' > "$P2/WIDGET.md"
T="$(mktree instance_data)"
printf 'PRIVATE_RECORD="hq"\n# machine-path-exempt: isolated publication fixture, not an operational default\nPRIVATE_SOURCES="%s"\n' "$P2" > "$T/.publication-boundary"
commit_all "$T"
assert_clean "private INSTANCE DATA naming the same contract is NOT misplaced" "$T"

# A private repo is allowed its own scripts. Only COUPLING to a public contract
# makes one a defect.
P3="$(mkprivate hq_uncoupled)"
cat > "$P3/scripts/normalize-exports.mjs" <<'EOF'
#!/usr/bin/env node
// One company's own export normaliser. Couples to nothing public.
console.log("normalizing");
EOF
T="$(mktree uncoupled)"
printf 'PRIVATE_RECORD="hq"\n# machine-path-exempt: isolated publication fixture, not an operational default\nPRIVATE_SOURCES="%s"\n' "$P3" > "$T/.publication-boundary"
commit_all "$T"
assert_clean "a private mechanism coupled to nothing public is NOT misplaced" "$T"

# ---------------------------------------------------------------------------
echo "--- (c2) a private tree that is a REPOSITORY is judged by what it COMMITTED"
# ---------------------------------------------------------------------------
# 2026-10-01: a teammate's commit in its own richos workspace was refused over
# a Codex audit probe STAGED, not committed, in richos-hq's main checkout. The
# walk read another repository's work in progress. A private root that is the
# top of its own repository is now read at its HEAD: uncommitted work there is
# nobody's finding, and a COMMITTED misplaced mechanism is still caught.
T="$(mktree private_repo)"
P6="$(mkprivate hq_repo)"
git -C "$P6" init -q 2>/dev/null || git init -q "$P6"
printf '# HQ\n' > "$P6/README.md"
git -C "$P6" add -A >/dev/null 2>&1
git -C "$P6" commit -qm "hq" >/dev/null 2>&1
printf 'PRIVATE_RECORD="hq"\n# machine-path-exempt: isolated publication fixture, not an operational default\nPRIVATE_SOURCES="%s"\n' "$P6" > "$T/.publication-boundary"
commit_all "$T"
cat > "$P6/scripts/probes-v2.py" <<'EOF'
#!/usr/bin/env python3
# An audit probe that reads .widget, work in progress in the private checkout.
print(open(".widget").read())
EOF
assert_clean "an UNTRACKED executable in the private repository is not judged" "$T"
git -C "$P6" add scripts/probes-v2.py >/dev/null 2>&1
assert_clean "a STAGED, uncommitted executable in the private repository is not judged (the 2026-10-01 case)" "$T"
git -C "$P6" commit -qm "probe" >/dev/null 2>&1
assert_finding "once COMMITTED to the private repository it is MISPLACED" "$T" MISPLACED "probes-v2.py"
# The committed state is the verdict in both directions: an uncommitted marker
# excuses nothing, and a committed one does.
printf '# instance-mechanism: one Codex audit window ran it against this checkout.\n' >> "$P6/scripts/probes-v2.py"
assert_finding "an UNCOMMITTED marker on a committed mechanism excuses nothing" "$T" MISPLACED "probes-v2.py"
git -C "$P6" commit -qam "excuse it" >/dev/null 2>&1
assert_clean "a COMMITTED marker excuses it" "$T"
# The commit arm's mode: Check 4 is left to the push arm and the land.
printf '#!/usr/bin/env bash\necho .widget\n' > "$P6/scripts/unexcused.sh"
git -C "$P6" add -A >/dev/null 2>&1
git -C "$P6" commit -qm "unexcused" >/dev/null 2>&1
assert_finding "a committed unexcused mechanism is MISPLACED" "$T" MISPLACED "unexcused.sh"
SKIP_OUT="$("$CHECK" --root "$T" --skip-private-trees 2>&1)"; SKIP_RC=$?
if [ "$SKIP_RC" -eq 0 ] && grep -q 'not checked here: --skip-private-trees' <<<"$SKIP_OUT"; then
    ok "--skip-private-trees leaves Check 4 out and says so"
else
    bad "--skip-private-trees should pass and say so (rc=$SKIP_RC)"; printf '%s\n' "$SKIP_OUT" | sed 's/^/        /'
fi
# ---------------------------------------------------------------------------
echo "--- (f) exemptions work, and a STALE exemption FAILS"
# ---------------------------------------------------------------------------
T="$(mktree exempt_ok)"
mkdir -p "$T/reference"
printf '# Reference\n\nAdopter path, will not resolve here: `scripts/deploy-staging.sh`.\n' \
    > "$T/reference/contract.md"
commit_all "$T"
assert_finding "without the exemption, the reference tree's adopter path is flagged" \
    "$T" CITATION "deploy-staging.sh"
printf 'CITATION_EXEMPT="reference/"\n' > "$T/.publication-completeness"
commit_all "$T"
assert_clean "a committed CITATION_EXEMPT prefix suppresses it" "$T"

T="$(mktree exempt_stale)"
printf 'CITATION_EXEMPT="reference/"\n' > "$T/.publication-completeness"
commit_all "$T"
assert_finding "an exemption that suppresses NOTHING fails and names itself for deletion" \
    "$T" STALE-EXEMPTION "suppresses nothing"

# ---------------------------------------------------------------------------
echo "--- (f2) the private-tree excuse lives IN the private file"
# ---------------------------------------------------------------------------
# Check 4's subject is a tree that exists only on the operator's machine, so its
# EXCUSE cannot live in the published declaration: in a clone with no private
# tree the check never runs, the entry therefore suppresses nothing, and the
# "an exemption that suppresses nothing FAILS" rule turns the gate red over a
# file that is not in the repository being checked. That is not the rule going
# wrong; it is an exemption being judged by a check that did not run. So the
# marker travels with the file it excuses, and everything below is about the
# marker being a real escape hatch and not an off switch.
T="$(mktree exempt_instance)"
P4="$(mkprivate hq_declared)"
cat > "$P4/scripts/one-off.sh" <<'EOF'
#!/usr/bin/env bash
# A one-off run against .widget, hardcoded to one operator's paths.
echo .widget
EOF
printf 'PRIVATE_RECORD="hq"\n# machine-path-exempt: isolated publication fixture, not an operational default\nPRIVATE_SOURCES="%s"\n' "$P4" > "$T/.publication-boundary"
commit_all "$T"
assert_finding "an unexcused private mechanism is MISPLACED, and the refusal names the marker" \
    "$T" MISPLACED "instance-mechanism: <reason>"

# A BARE marker is not an off switch anybody can type into a file — the same
# one-character property dialect-guard.sh's mutation suite pins for
# `dialect-exempt:`.
printf '# instance-mechanism:\n' >> "$P4/scripts/one-off.sh"
assert_finding "a BARE 'instance-mechanism:' with no reason exempts nothing" \
    "$T" MISPLACED "with no reason"

# With a reason, it does exempt — and the refusal above is what told the author
# how to write it.
printf '# instance-mechanism: one operator hand-ran this once, against paths only they have.\n' \
    >> "$P4/scripts/one-off.sh"
assert_clean "an 'instance-mechanism: <reason>' marker in the private file suppresses it" "$T"

# THE PROPERTY ALL OF THIS EXISTS FOR, asserted directly: the SAME public tree,
# byte for byte, gets the SAME verdict whether or not the private tree is on
# this machine. This is the fresh-clone case as a unit test — under the old
# declaration-entry form the second assertion here failed with a
# STALE-EXEMPTION over a path outside the tree.
mv "$P4" "$P4.away"
assert_clean "the same public tree passes when the private tree is NOT on this machine" "$T"
mv "$P4.away" "$P4"
assert_clean "and passes again with the private tree back — one tree, one verdict" "$T"

# THE MARKER IS ITSELF CHECKED, so it cannot outlive its reason any more than a
# declaration entry could. A marker on a mechanism that couples to nothing
# public excuses a finding that is not there, and says so.
T2="$(mktree marker_stale)"
P5="$(mkprivate hq_stale_marker)"
cat > "$P5/scripts/unrelated.sh" <<'EOF'
#!/usr/bin/env bash
# instance-mechanism: excuses a coupling this file no longer has.
echo "one company's own thing, coupled to nothing public"
EOF
printf 'PRIVATE_RECORD="hq"\n# machine-path-exempt: isolated publication fixture, not an operational default\nPRIVATE_SOURCES="%s"\n' "$P5" > "$T2/.publication-boundary"
commit_all "$T2"
assert_finding "a marker on a mechanism coupled to nothing public FAILS as a stale exemption" \
    "$T2" STALE-EXEMPTION "suppresses nothing"

# AN ISOLATED WORKTREE INSIDE THE PRIVATE TREE IS NOT A SECOND MECHANISM.
# Measured 2026-09-01: an agent's `.worktrees/<branch>/` copy of the private
# tree re-reported an already-excused mechanism, and
# guard-completeness-commits.sh then refused every commit in the public
# repository until that worktree was moved. The walk skips `.worktrees`, which
# removes duplicates and no coverage — the same bytes are still walked at their
# tracked path. Asserted on an UNEXCUSED file, so the case cannot pass merely
# because the copy carries the marker too: the finding must appear, and exactly
# once.
mkdir -p "$P4/.worktrees/agent-abc123/scripts"
cat > "$P4/scripts/undeclared.sh" <<'EOF'
#!/usr/bin/env bash
# Reads .widget and nobody excused it.
echo .widget
EOF
cp "$P4/scripts/undeclared.sh" "$P4/.worktrees/agent-abc123/scripts/undeclared.sh"
assert_finding "an UNEXCUSED private mechanism is still caught while a worktree copy exists" \
    "$T" MISPLACED "undeclared.sh"
run "$T"
DUPES="$(grep -c 'undeclared\.sh' <<<"$OUT")"
if [ "$DUPES" = "1" ]; then
    ok "the worktree copy is not reported as a SECOND finding (1 mention, not 2)"
else
    bad "worktree copy double-reported ($DUPES mentions of undeclared.sh, expected 1)"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
rm -f "$P4/scripts/undeclared.sh"
rm -rf "$P4/.worktrees"

# ---------------------------------------------------------------------------
echo "--- (g) no silent degradation"
# ---------------------------------------------------------------------------
T="$(mktree broken_key)"
printf 'CITATION_EXEMPTIONS="reference/"\n' > "$T/.publication-completeness"
commit_all "$T"
assert_rc "an unknown key in .publication-completeness → exit 2, not a quiet pass" \
    "$T" 2 "unknown key"

# THIS IS THE 2026-09-04 DEFECT, REPRODUCED. A published declaration carrying
# an INSTANCE_MECHANISMS entry, in a tree with no private source on the disk:
# the pre-fix checker accepted the key, found that Check 4 had not run, judged
# the entry stale and returned 1 — the fresh-clone red that made every green
# result on the author's machine prove less than it appeared to. The key is now
# refused BY NAME with the migration, so the whole class is unexpressible
# rather than fixed one instance at a time.
T="$(mktree retired_key)"
printf 'INSTANCE_MECHANISMS="hq/scripts/one-off.sh"\n' > "$T/.publication-completeness"
commit_all "$T"
assert_rc "the RETIRED INSTANCE_MECHANISMS key → exit 2, naming where the concept went" \
    "$T" 2 "instance-mechanism: <reason>"

T="$(mktree broken_line)"
printf 'this is not a key value line\n' > "$T/.publication-completeness"
commit_all "$T"
assert_rc "a malformed line in .publication-completeness → exit 2" \
    "$T" 2 "not KEY=value"

T="$(mktree broken_subst)"
printf 'CITATION_EXEMPT="$(pwd)/reference"\n' > "$T/.publication-completeness"
commit_all "$T"
assert_rc "shell substitution in a parsed-never-sourced value → exit 2" \
    "$T" 2 "shell substitution"

T="$(mktree broken_boundary)"
printf 'PRIVATE_RECORD="x"\nNOT_A_KEY="y"\n' > "$T/.publication-boundary"
commit_all "$T"
assert_rc "a malformed .publication-boundary → exit 2, never a completeness verdict" \
    "$T" 2 "malformed"

EMPTY="$SCRATCH/emptyrepo"
mkdir -p "$EMPTY"
git -C "$EMPTY" init -q 2>/dev/null || git init -q "$EMPTY"
printf 'PRIVATE_RECORD="x"\n' > "$EMPTY/.publication-boundary"
assert_rc "a declaring repository with NOTHING tracked → exit 2, not '0 findings'" \
    "$EMPTY" 2 "broken checkout"

# ---------------------------------------------------------------------------
echo "--- (h) the grouped declaration directory"
# ---------------------------------------------------------------------------
# The declaration may live at `.richos/publication-boundary`. Everything above
# proves the root form; these prove the move does not quietly turn a governed
# tree into a NOT-APPLICABLE one, which is the failure that reads as a pass.

T="$(mktree grouped_found)"
mkdir -p "$T/.richos"; mv "$T/.publication-boundary" "$T/.richos/publication-boundary"
commit_all "$T"
assert_clean "a boundary declared in .richos/ is FOUND and the tree is checked" "$T"

# ...and the check is still capable of failing over it. A NOT-APPLICABLE tree
# and a clean tree are different exit codes; a tree with a real defect must
# still report it under the grouped declaration.
T="$(mktree grouped_still_fails)"
mkdir -p "$T/.richos"; mv "$T/.publication-boundary" "$T/.richos/publication-boundary"
printf '\nThe entry point is `src/missing.js`.\n' >> "$T/README.md"
commit_all "$T"
assert_rc "a dangling citation is still reported under a grouped declaration" \
    "$T" 1 "src/missing.js"

# The exemption file is resolved the same way, and an exemption the checker
# cannot find is one that silently stops exempting.
T="$(mktree grouped_exempt)"
mkdir -p "$T/.richos"; mv "$T/.publication-boundary" "$T/.richos/publication-boundary"
mkdir -p "$T/reference"; printf 'See `reference/nope.md`.\n' > "$T/reference/README.md"
commit_all "$T"
assert_rc "without the exemption, the reference tree's dangling citation FAILS" \
    "$T" 1 "reference/nope.md"
printf 'CITATION_EXEMPT="reference/"\n' > "$T/.richos/publication-completeness"
commit_all "$T"
assert_clean "an exemption file at .richos/publication-completeness is honored" "$T"

# Two copies of one declaration is BROKEN, never a choice between them.
T="$(mktree grouped_both)"
mkdir -p "$T/.richos"; cp "$T/.publication-boundary" "$T/.richos/publication-boundary"
commit_all "$T"
assert_rc "declared in BOTH places → exit 2, never a quiet choice" \
    "$T" 2 "carries BOTH"

# A DECLARATION in .richos/ that nothing resolves is BROKEN too...
T="$(mktree grouped_stray)"
mkdir -p "$T/.richos"; mv "$T/.publication-boundary" "$T/.richos/publication-boundary"
printf 'TODO_RECORD="x"\n' > "$T/.richos/ceo-todos"
commit_all "$T"
assert_rc "an unresolved DECLARATION in .richos/ → exit 2, naming it" \
    "$T" 2 "is a declaration, and nothing reads a declaration from there"

# ...and ANYTHING ELSE in there is none of this resolver's business. `.richos/`
# is a shared RichOS directory: the ECS entity manifest has lived at
# `.richos/entity.json` in femcboost since 2026-08-27. A resolver that policed
# the whole directory would have taken that repository offline.
rm "$T/.richos/ceo-todos"
printf '{"entityId":"sample"}\n' > "$T/.richos/entity.json"
printf 'notes\n' > "$T/.richos/something-else.txt"
commit_all "$T"
assert_clean "an unrelated file in .richos/ is left alone (the ECS entity manifest case)" "$T"

# EVERY DECLARATION IS CLASSIFIED BY THE RESOLVER, and the check that says so
# is derived from shipped source rather than from a list somebody remembers to
# grow. Two-sided: unclassified is BROKEN, classified is silent.
mk_resolver() { # <tree> <adopted> <foreign>
    mkdir -p "$1/scripts/lib"
    {
        printf ': "${DECLARATION_DIR:=.richos}"\n'
        printf 'DECL_ADOPTED_STEMS="%s"\n' "$2"
        printf 'DECL_FOREIGN_STEMS="%s"\n' "$3"
    } > "$1/scripts/lib/declaration-path.sh"
}
T="$(mktree unclassified_decl)"
mk_resolver "$T" "publication-boundary" "ceo-todos"
commit_all "$T"
assert_rc "a declaration in NEITHER resolver list → BROKEN, naming it" \
    "$T" 2 "widget"
mk_resolver "$T" "publication-boundary widget" "ceo-todos"
commit_all "$T"
assert_clean "the same tree with .widget classified is silent" "$T"

# A MISSING RESOLVER MUST NOT ARRIVE AS "NOT APPLICABLE". This script's
# stand-down verdict is the sentence a reader takes to mean *this tree does not
# get published*, and an engine that could not load declaration-path.sh has no
# business saying it. Run against a COPY of the engine with exactly that one
# file removed, so the real one is never disturbed.
FE="$SCRATCH/fake-engine"
mkdir -p "$FE/scripts/lib"
cp "$ENGINE_ROOT/scripts/publication-completeness.sh" "$FE/scripts/"
cp "$ENGINE_ROOT/scripts/publication-completeness.py" "$FE/scripts/"
cp "$ENGINE_ROOT/scripts/lib/machine_paths.py" "$FE/scripts/lib/"
cp "$ENGINE_ROOT/scripts/lib/publication-boundary.sh" "$FE/scripts/lib/"
cp "$ENGINE_ROOT/scripts/lib/publication-boundary.py" "$FE/scripts/lib/"
cp "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" "$FE/scripts/lib/" 2>/dev/null || true
# ...and NO scripts/lib/declaration-path.sh.
T="$(mktree no_resolver)"
FE_OUT="$(bash "$FE/scripts/publication-completeness.sh" --root "$T" 2>&1)"; FE_RC=$?
if [ "$FE_RC" -eq 2 ] \
   && grep -q "declaration-path.sh is missing at:" <<<"$FE_OUT" \
   && ! grep -q "NOT APPLICABLE" <<<"$FE_OUT"; then
    ok "a missing declaration-path.sh → exit 2 naming the file, never NOT APPLICABLE"
else
    bad "a missing resolver must refuse loudly (rc=$FE_RC)"; printf '%s\n' "$FE_OUT" | sed 's/^/        /'
fi

# ---------------------------------------------------------------------------
echo ""
if [ "$FAIL" -eq 0 ]; then
    printf '✓ publication-completeness.test.sh: %s/%s cases passed.\n' "$PASS" "$((PASS + FAIL))"
    exit 0
fi
printf '✗ publication-completeness.test.sh: %s/%s cases passed, %s FAILED.\n' \
    "$PASS" "$((PASS + FAIL))" "$FAIL" >&2
exit 1
