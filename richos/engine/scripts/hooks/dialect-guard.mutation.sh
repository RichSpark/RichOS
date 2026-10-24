#!/usr/bin/env bash
#
# dialect-guard.mutation.sh — PROVES THE DIALECT SUITE CAN FAIL.
#
# 71 green ticks are evidence of nothing until somebody shows them turning red
# for the right reason — and this guard has a specific way of being useless
# while looking healthy. A word list that quietly stops matching, an exemption
# widened by one character, a `exit 2` that became `exit 0`: every one of those
# leaves the hook wired, hash-matched, present, executable and PASSING, over
# zero enforcement. That is exactly the shape of the defect this guard was
# built for — the 2026-08-30 sweep reported success and constrained nothing.
#
# So: take the SHIPPED source, remove ONE property at a time, and assert that
#   1. dialect-guard.test.sh FAILS,
#   2. the SPECIFIC named case fails — not merely "something went red", and
#   3. the mutation actually applied (a replacement that matched nothing gives
#      a green run that looks like a green run, which is the same trap again).
#
# Every mutant is a throwaway copy of the engine subtree. Nothing here touches
# the real tree.
#
# Run directly: scripts/hooks/dialect-guard.mutation.sh
# Exit 0 = every property is proven load-bearing.

# THE MERGE GATE LEAVES THIS PASS TO THE NIGHTLY (richos/app/scripts/autocheck/README.md): the
# gate runs the suite with RICHOS_MUTATION_PASSES=0; nightly-engine.py runs every pass.
if [ "${RICHOS_MUTATION_PASSES:-}" = 0 ]; then echo "NOT RUN: $(basename "$0") (RICHOS_MUTATION_PASSES=0: the merge gate leaves this mutation pass to the nightly)"; exit 0; fi

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

PASS=0
FAIL=0
SANDBOX="$(cd "$(mktemp -d -t dialect-guard-mutation.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

cat >"$SANDBOX/mutate.py" <<'PYEOF'
import sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
old = old.replace("\\n", "\n")
new = new.replace("\\n", "\n")
with open(path, encoding="utf-8") as fh:
    src = fh.read()
if old not in src:
    sys.stderr.write("MUTATION TARGET ABSENT — the source has drifted:\n  %s\n" % old)
    sys.exit(3)
with open(path, "w", encoding="utf-8") as fh:
    fh.write(src.replace(old, new, 1))
PYEOF

# mutant <name> <expected-failing-case> <rel-file> <old> <new> <why>
# --- concurrency ------------------------------------------------------------
# Each mutant builds its own sandbox under $SANDBOX/<name> and runs a whole
# suite against it, so no two mutants share a path and none of them is ordered
# against another. They ran one at a time only because a `for` loop is what this
# harness was first written as. mutation-pool.sh bounds the fan-out, keeps the
# report in declaration order, and counts a KILLED mutant as a failure rather
# than letting it vanish from the tally. RICHOS_MUTANT_JOBS overrides the degree.
# shellcheck source=../lib/stopwatch.sh
. "$ENGINE_ROOT/scripts/lib/stopwatch.sh"
# shellcheck source=../lib/mutation-pool.sh
. "$ENGINE_ROOT/scripts/lib/mutation-pool.sh"
# shellcheck source=../lib/mutant-suite-run.sh
. "$ENGINE_ROOT/scripts/lib/mutant-suite-run.sh"
mut_pool_init
MUT_WALL_T0="$(sw_now_ms)"

_mutant_body() {
    local name="$1" want="$2" rel="$3" old="$4" new="$5" why="$6"
    local dir="$SANDBOX/$name"
    mkdir -p "$dir/scripts/hooks" "$dir/scripts/lib" "$dir/hooks" "$dir/.claude"
    cp "$ENGINE_ROOT/scripts/hooks/dialect-guard.sh" \
       "$ENGINE_ROOT/scripts/hooks/dialect-guard.test.sh" \
       "$ENGINE_ROOT/scripts/hooks/install.sh" "$dir/scripts/hooks/"
    cp "$ENGINE_ROOT/scripts/lib/dialect-en-US.dict" \
       "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" \
       "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" \
       "$ENGINE_ROOT/scripts/lib/declaration-path.sh" \
       "$ENGINE_ROOT/scripts/lib/vendored-material.sh" \
       "$ENGINE_ROOT/scripts/lib/seat-jurisdiction.sh" "$dir/scripts/lib/"
    cp "$ENGINE_ROOT/hooks/hooks.json" "$dir/hooks/"
    cp "$ENGINE_ROOT/.claude/settings.local.json" "$dir/.claude/"
    cp "$ENGINE_ROOT/orchestration.config" "$dir/"
    chmod +x "$dir/scripts/hooks/"*.sh

    if ! python3 "$SANDBOX/mutate.py" "$dir/$rel" "$old" "$new" 2>"$dir/mutate.err"; then
        printf '  FAIL  %s — the mutation did not apply\n' "$name"
        sed 's/^/          /' "$dir/mutate.err"
        return 1
    fi

    # RICHOS_MUTATION_INNER: the suite under test must not recurse into THIS
    # harness — one level is the proof, and a mutant running mutants is a fork
    # bomb with a green tick at the bottom. Same flag the shared harness uses.
    # The run stops at the mutant's named FAIL line (mutant-suite-run.sh): in this
    # suite a printed FAIL line always ends the run red, and the rest of the run
    # buys nothing the verdict reads.
    mutant_suite_run "$dir/out.txt" "$want" env RICHOS_MUTATION_INNER=1 bash "$dir/scripts/hooks/dialect-guard.test.sh"
    if [ "$MUTANT_STOPPED" = 1 ]; then
        printf '  PASS  %s — removing it turns %s red (stopped at that line)\n' "$name" "$want"
        return 0
    fi
    local rc=$MUTANT_RC
    if [ "$rc" -eq 0 ]; then
        printf '  FAIL  %s — the suite still PASSED without this property.\n' "$name"
        printf '          %s\n' "$why"
        return 1
    fi
    if ! grep -qF "FAIL  $want" "$dir/out.txt"; then
        printf '  FAIL  %s — the suite went red, but NOT at %s (so the red is unrelated).\n' "$name" "$want"
        grep '  FAIL' "$dir/out.txt" | sed 's/^/          /'
        return 1
    fi
    printf '  PASS  %s — removing it turns %s red\n' "$name" "$want"
    return 0
}
# mutant <name> ... — a SUBMISSION, not an execution. The body above is unchanged
# except that it returns its verdict instead of incrementing a counter: it runs
# in a pool worker, which is a subshell, so an increment in there would mutate a
# copy and be discarded. The pool prints the bodies in DECLARATION order, so this
# harness's report is byte-identical to the serial one but for the durations.
mutant() {
    mut_pool_submit "$1" _mutant_body "$@"
}

echo "=== the dialect guard: every property, proven load-bearing by removing it ==="

G="scripts/hooks/dialect-guard.sh"
D="scripts/lib/dialect-en-US.dict"

# --- 1. IT BLOCKS AT ALL ---------------------------------------------------
mutant refuses-to-refuse "A1. " "$G" \
    '      echo "(hook: scripts/hooks/dialect-guard.sh)"\n    } >&2\n    exit 2\n    ;;\n  *)' \
    '      echo "(hook: scripts/hooks/dialect-guard.sh)"\n    } >&2\n    exit 0\n    ;;\n  *)' \
    "the guard would find every violation, print it, and let the write through — a warning wearing a guard's clothes."

# --- 2. THE VOCABULARY IS WHAT DECIDES -------------------------------------
mutant empty-vocabulary "A1. " "$D" \
    'colour	color' \
    '# colour	color' \
    "a word silently dropped from the dictionary is enforcement gone with no other symptom."

# --- 3. FAIL-CLOSED ON A VOCABULARY THAT PARSES TO NOTHING -----------------
mutant nodict-passes "D5. " "$G" \
    '        echo "  DIALECT_TARGET is declared, so this repository believes it is"\n        echo "  governed. Refusing rather than passing every write through."\n    } >&2\n    exit 2' \
    '        echo "  DIALECT_TARGET is declared, so this repository believes it is"\n        echo "  governed. Refusing rather than passing every write through."\n    } >&2\n    exit 0' \
    "a governed repo with no word list would pass every write and report nothing — an enforcement outage that looks like a clean record."

# --- 4. THE DECLARED EXEMPTION MUST CARRY A REASON -------------------------
mutant bare-marker-exempts "B12. " "$G" \
    'DECLARED_EXEMPT_RE = re.compile(r"dialect-exempt:\s*[A-Za-z0-9]")' \
    'DECLARED_EXEMPT_RE = re.compile(r"dialect-exempt:")' \
    "a bare 'dialect-exempt:' would exempt anything, so the escape hatch becomes an off switch anyone can type."

# --- 5. THE 'CEO queue' COLLOCATION ----------------------------------------
# The needle is the USE site, not the pattern: the pattern literal contains an
# apostrophe (CEO's) and cannot survive a single-quoted shell argument.
mutant no-ceo-queue-rule "A17. " "$G" \
    '    for m in CEO_QUEUE_RE.finditer(line):' \
    '    for m in ():' \
    "the half of the ruling that is a RENAME rather than a spelling would go unenforced."

# --- 6. THE SOFT 'queue' RULE MUST NOT BLOCK -------------------------------
mutant soft-queue-blocks "C1. " "$G" \
    '        SOFT.append((m.group(0).strip(), line.strip()[:110]))' \
    '        FINDINGS.append((m.group(0).strip(), line.strip()[:110]))' \
    "'the queue' would become a hard block on a collocation nobody can adjudicate from one line — the cries-wolf failure that gets a guard switched off."

# --- 7. THE EXEMPTIONS ARE THE PRODUCT, NOT THE TRIMMINGS ------------------
# Each of these, removed, turns the guard into something an engineer disables.
# Expected case is B5, not B3, and the difference is the finding: camelCase and
# snake_case are exempt for FREE via \b word boundaries, so removing this
# function does not touch them. What it costs is CSS custom properties, file
# paths and URLs — which is why the next mutant exists separately.
mutant no-identifier-exemption "B5. " "$G" \
    '        if tok != observed and looks_like_identifier_or_path(tok):\n            continue' \
    '        if False:\n            continue' \
    "kebab-case identifiers, CSS custom properties, URLs and file paths would all be flagged as prose."

mutant no-fence-exemption "B10. " "$G" \
    '    if in_fence:\n        continue' \
    '    if False:\n        continue' \
    "every fenced code block and captured output block in the record becomes a violation."

mutant no-blockquote-exemption "B9. " "$G" \
    '    if stripped.startswith(">"):\n        continue' \
    '    if False:\n        continue' \
    "the guard would demand that quoted external material — and the CEO's own verbatim words — be rewritten."

mutant no-inline-code-exemption "B8. " "$G" \
    'def inside_inline_code(line, col):\n    return line.count("`", 0, col) % 2 == 1' \
    'def inside_inline_code(line, col):\n    return False' \
    "naming a British-spelled identifier in prose would be impossible, including in this guard's own documentation."

mutant no-evidence-path-exemption "B15. " "$G" \
    '        if seg in EVIDENCE_SEGMENTS:' \
    '        if seg in ():' \
    "captured evidence — raw pages, transcripts, corpora, fixtures, run logs — would be rewritten, which destroys the evidence."

mutant evidence-read-above-the-repository "D4b. " "$G" \
    '    for seg in segment_path.split("/"):' \
    '    for seg in low_path.split("/"):' \
    "a repository that lives under a directory named logs/ or fixtures/ (a checkout under ~/work/logs/, or every test repository in a verification run's private profile) would have every file exempted as evidence, and British prose would pass in silence; that is what turned eight contract-integrity sections red only inside the merge gate on 2026-10-01."

mutant no-vendor-legal-exemption "B20. " "$G" \
    '    if stem in VENDOR_LEGAL:' \
    '    if stem in ():' \
    "someone else's LICENSE/NOTICE text would be edited to suit our dialect."

mutant no-ext-exempt "B13. " "$G" \
    '        if ext and ext in exempt_exts:\n            continue' \
    '        if False:\n            continue' \
    "'grey' is a legal CSS named color; without the per-word extension exemption every stylesheet using it is blocked."

mutant no-rename-narration "B26. " "$G" \
    '        if RENAME_CUE_RE.search(line[max(0, m.start() - 60):m.start()]):\n            continue\n        if any(a in line for a in ALLOWLIST):\n            continue\n        FINDINGS.append(("%s -> CEO-TODOs" % m.group(0), line.strip()[:110]))' \
    '        if any(a in line for a in ALLOWLIST):\n            continue\n        FINDINGS.append(("%s -> CEO-TODOs" % m.group(0), line.strip()[:110]))' \
    "the guard would forbid the sentence that documents the rename — you cannot record a rename without naming the old name."

# --- 8. WORD BOUNDARIES, AND THE WORDS THAT ONLY LOOK BRITISH -------------
mutant no-word-boundaries "B3. " "$G" \
    'WORD_RE = re.compile(r"\b(%s)\b" % "|".join(sorted(RULES, key=len, reverse=True)), re.IGNORECASE)' \
    'WORD_RE = re.compile(r"(%s)" % "|".join(sorted(RULES, key=len, reverse=True)), re.IGNORECASE)' \
    "camelCase and snake_case identifiers stop being exempt for free, and every code symbol containing a dictionary word is flagged."

# --- 9. THE VENDORING REGISTRY, AND THE ONE-WAY DOOR IT CLOSES ------------
# On 2026-08-30 this guard rewrote 14 lines across two vendored MIT-licensed
# skills, and then refused the commit that tried to quote the difference back.
# Both halves of that are properties now, so both are proven load-bearing.
#
# TWO CALL SITES CARRY THE SUPPRESSION, one per verdict, and each has its own
# mutant. A single mutant on the FOUND site would leave the REPORTONLY site
# untested, and the two are not the same assertion: FOUND decides whether
# somebody else's prose is REWRITTEN, REPORTONLY decides whether somebody else's
# prose is NARRATED about at every write. Only the first is a correctness bug;
# the second is the noise that gets a guard switched off.
mutant no-vendored-exemption "V1. " "$G" \
    '  FOUND)\n    _dg_is_vendored && exit 0' \
    '  FOUND)' \
    "somebody else's prose would be silently rewritten again, and a verbatim re-vendoring from upstream would be REFUSED by the guard that caused the divergence."

mutant no-vendored-exemption-on-soft "V13. " "$G" \
    "    # write never pays for it. See _dg_is_vendored's header.\n    _dg_is_vendored && exit 0" \
    "    # write never pays for it. See _dg_is_vendored's header." \
    "a vendored file would draw a CEO-TODOs note on every write, about wording in a document nobody here may change — the cries-wolf half of the same defect."

mutant vendored-exemption-ignores-origin "V2. " "$G" \
    '    vm_is_third_party "$rel" || return 1' \
    '    vm_covering "$rel" || return 1' \
    "every path the registry names would be exempt, including our own work — an off switch with an inventory attached, and the dialect ruling would stop applying to the tree it was written for."

# Aimed at V15, not V6. On a BLOCKING finding both spellings refuse and the
# mutant is invisible; the fail-closed only has observable behavior on a
# REPORTING one, where `exit 2` refuses and `return 1` emits a CEO-TODOs note
# about a file whose ownership the guard just admitted it cannot establish.
mutant vendored-broken-fails-open "V15. " "$G" \
    '            echo "(hook: scripts/hooks/dialect-guard.sh)"\n        } >&2\n        exit 2 ;;' \
    '            echo "(hook: scripts/hooks/dialect-guard.sh)"\n        } >&2\n        return 1 ;;' \
    "a declared-but-unreadable registry would print a banner and then judge the file anyway, on the assumption that it is ours — which is the assumption that cost 14 lines of somebody else's prose."

mutant analysis-is-british "B28. " "$D" \
    'analyse	analyze' \
    'analyses	analyzes' \
    "'analyses' is already American (the plural of analysis); adding it would make the guard corrupt correct English."

mutant enrollment-is-british "B36. " "$D" \
    'enrolment	enrollment' \
    'enrollment	enrolment' \
    "'enrollment' is already American; a reversed entry would make the guard the CAUSE of the defect it exists to stop."

# --- the verdict ------------------------------------------------------------
# Drained rather than accumulated: PASS/FAIL below come from the workers' exit
# codes, and a worker that left no exit code is counted as a FAILURE. The tally
# that follows is unchanged and still decides this harness's exit status.
# ADDITIVE, NOT ASSIGNMENT, and that is a correction found by measurement rather
# than by reading: mechanical-findings.mutation.sh scores one case BEFORE its
# mutants, and `PASS="$MUT_POOL_PASS"` silently discarded it. A tally that
# overwrites an earlier verdict is a green fraction over a case that ran and was
# thrown away. Harnesses that score nothing beforehand are unaffected, because
# adding to zero is assignment.
mut_pool_drain
# A harness that declared mutants and ran NONE must not exit 0 -- see
# mut_pool_require_submissions for the run where exactly that happened.
mut_pool_require_submissions "$(basename "$0")"
PASS=$(( PASS + MUT_POOL_PASS ))
FAIL=$(( FAIL + MUT_POOL_FAIL ))
mut_pool_report_line "$(( $(sw_now_ms) - MUT_WALL_T0 ))"
mut_pool_cleanup

echo
if [ "$FAIL" -eq 0 ]; then
    printf '\n  %d/%d properties proven load-bearing\n' "$PASS" "$((PASS + FAIL))"
    exit 0
fi
printf '\n  %d/%d properties proven load-bearing, %d NOT\n' "$PASS" "$((PASS + FAIL))" "$FAIL"
exit 1
