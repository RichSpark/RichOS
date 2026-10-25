#!/usr/bin/env bash
#
# dialect-guard.mutation.sh — PROVES THE DIALECT SUITE CAN FAIL.
#
# A hundred and fifty green ticks are evidence of nothing until somebody shows
# them turning red for the right reason — and this hook has specific ways of
# being useless while looking healthy. A table that quietly stops matching, an
# exemption widened by one character, an `exit 2` that became `exit 0`, a fix
# that is computed and never emitted, a fragment judged without its file: every
# one of those leaves the hook wired, hash-matched, present, executable and
# PASSING, over zero enforcement. That is exactly the shape of the defect this
# hook was built for — the 2026-08-30 sweep reported success and constrained
# nothing.
#
# So: take the SHIPPED source, remove ONE property at a time, and assert that
#   1. dialect-guard.test.sh FAILS,
#   2. the SPECIFIC named case fails — not merely "something went red", and
#   3. the mutation actually applied (a replacement that matched nothing gives
#      a green run that looks like a green run, which is the same trap again).
#
# Every mutant is a throwaway copy of the engine subtree. Nothing here touches
# the real tree. Exempt from the hook by exact basename: its mutation targets
# are the British words the hook changes.
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

# --- concurrency ------------------------------------------------------------
# Each mutant builds its own sandbox under $SANDBOX/<name> and runs a whole
# suite against it, so no two mutants share a path. mutation-pool.sh bounds the
# fan-out, keeps the report in declaration order, and counts a KILLED mutant as
# a failure. RICHOS_MUTANT_JOBS overrides the degree.
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
    mkdir -p "$dir/scripts/hooks" "$dir/scripts/lib/dialect/fixtures" "$dir/hooks" "$dir/.claude"
    cp "$ENGINE_ROOT/scripts/hooks/dialect-guard.sh" \
       "$ENGINE_ROOT/scripts/hooks/dialect-guard.test.sh" \
       "$ENGINE_ROOT/scripts/hooks/install.sh" \
       "$ENGINE_ROOT/scripts/hooks/dispatch-pretooluse.manifest" \
       "$ENGINE_ROOT/scripts/hooks/contract-integrity-probe.sh" "$dir/scripts/hooks/"
    cp "$ENGINE_ROOT/scripts/lib/dialect-en-US.dict" \
       "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" \
       "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" \
       "$ENGINE_ROOT/scripts/lib/declaration-path.sh" \
       "$ENGINE_ROOT/scripts/lib/vendored-material.sh" \
       "$ENGINE_ROOT/scripts/lib/registered-hooks.sh" \
       "$ENGINE_ROOT/scripts/lib/unevaluated-notice.sh" \
       "$ENGINE_ROOT/scripts/lib/seat-jurisdiction.sh" "$dir/scripts/lib/"
    cp "$ENGINE_ROOT/scripts/lib/dialect/dialect-fix.py" \
       "$ENGINE_ROOT/scripts/lib/dialect/dialect-en-US.generated.dict" "$dir/scripts/lib/dialect/"
    cp "$ENGINE_ROOT/scripts/lib/dialect/fixtures/british-keyed.txt" "$dir/scripts/lib/dialect/fixtures/"
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
    # harness — one level is the proof. The run stops at the mutant's named FAIL
    # line (mutant-suite-run.sh).
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
mutant() {
    mut_pool_submit "$1" _mutant_body "$@"
}

echo "=== the dialect hook: every property, proven load-bearing by removing it ==="

G="scripts/hooks/dialect-guard.sh"
F="scripts/lib/dialect/dialect-fix.py"
D="scripts/lib/dialect/dialect-en-US.generated.dict"

# --- 1. IT REFUSES, AND IT FIXES -------------------------------------------
mutant refuses-to-refuse "R1. " "$G" \
    '"$BODY" >&2\n    exit 2\n    ;;\n  *)' \
    '"$BODY" >&2\n    exit 0\n    ;;\n  *)' \
    "the hook would find every violation it cannot fix, print it, and let the write through — a warning wearing a guard's clothes."

mutant fix-never-emitted "A1. " "$G" \
    'typed.\n    _dg_is_vendored && exit 0\n' \
    'typed.\n    _dg_is_vendored && exit 0\n    exit 0\n' \
    "the fixer would compute every American form and the hook would drop it, so British text lands with nothing said: the silent loss the one-hook design exists to prevent."

mutant envelope-approves "A20. " "$F" \
    '"hookEventName": "PreToolUse", "updatedInput": out.updated,' \
    '"hookEventName": "PreToolUse", "permissionDecision": "allow", "updatedInput": out.updated,' \
    "a fixing hook that also says allow approves the write and bypasses the permission flow."

mutant input-not-copied "A21. " "$F" \
    '            out.updated = dict(ti, content=fixed)' \
    '            out.updated = {"content": fixed}' \
    "updatedInput REPLACES the tool input, so a copy without file_path sends the write nowhere."

mutant no-change-report "A22. " "$F" \
    '        context = change_report(file_path, out.fixes)' \
    '        context = ""' \
    "a silent fixer removes the signal that found the 2026-08-30 damage, and the model's next Edit quotes text that is no longer on disk."

# --- 2. THE TABLE IS WHAT DECIDES ------------------------------------------
mutant empty-vocabulary "A1. " "$D" \
    'colour	color' \
    '# colour	color' \
    "a word silently dropped from the table is enforcement gone with no other symptom."

mutant analysis-is-british "B28. " "$D" \
    'analyse	analyze' \
    'analyses	analyzes' \
    "'analyses' is already American (the plural of analysis); adding it would make the hook corrupt correct English."

mutant enrollment-is-british "B36. " "$D" \
    'enrolment	enrollment' \
    'enrollment	enrolment' \
    "'enrollment' is already American; a reversed entry would make the hook the CAUSE of the defect it exists to stop."

# --- 3. FAIL-CLOSED ---------------------------------------------------------
mutant nodict-passes "D5. " "$G" \
    '        echo "  governed. Refusing rather than passing every write through."\n    } >&2\n    exit 2' \
    '        echo "  governed. Refusing rather than passing every write through."\n    } >&2\n    exit 0' \
    "a governed repo with no table would pass every write and report nothing."

mutant nofixer-passes "D5b. " "$G" \
    '        echo "  Every fix and every refusal this hook makes comes out of that file."\n    } >&2\n    exit 2' \
    '        echo "  Every fix and every refusal this hook makes comes out of that file."\n    } >&2\n    exit 0' \
    "a missing fixer would pass every write: enforcement off, nothing said."

mutant empty-table-passes "D5c. " "$G" \
    'A table that parses to nothing is an enforcement outage that looks like a clean run." >&2\n    exit 2' \
    'A table that parses to nothing is an enforcement outage that looks like a clean run." >&2\n    exit 0' \
    "a table that parses to nothing would pass every write and look like a clean record."

mutant crash-passes "D7. " "$G" \
    'Its own error is above. (hook: scripts/hooks/dialect-guard.sh)" >&2\n    exit 2' \
    'Its own error is above. (hook: scripts/hooks/dialect-guard.sh)" >&2\n    exit 0' \
    "a fixer that crashes on some input would wave that input through unjudged."

# --- 4. WHAT CANNOT BE FIXED IS REFUSED, NOT FIXED -------------------------
mutant no-ceo-queue-rule "R1. " "$F" \
    '    for s, e, matched in queue_hits:' \
    '    for s, e, matched in ():' \
    "the half of the ruling that is a RENAME rather than a spelling would go unenforced."

mutant single-token-literal-fixed "R3. " "$F" \
    '                elif not any(ch.isspace() for ch in content):' \
    '                elif False:' \
    "a one-word string can be a third-party wire value; fixing it breaks the call with nobody told."

mutant identifier-fixed "R4. " "$F" \
    '                action, why = "refuse", "code: renaming an identifier changes behavior"' \
    '                action, why = "fix", ""' \
    "renaming one occurrence of an identifier breaks the program."

mutant markup-fixed "R5. " "$F" \
    '            action, why = "refuse", "markup, styles or config: a fix could change behavior"' \
    '            action, why = _prose_decision(text, s, e, run, ls, t.para[idx])' \
    "a class name, a config key or a wire value in markup or config would be rewritten silently."

mutant quotation-fixed "R6. " "$F" \
    '    if _in_quote(text, quote_lo, s):' \
    '    if False:' \
    "somebody's quoted words would be rewritten."

mutant capital-mid-sentence-fixed "R7. " "$F" \
    '        if not _sentence_start(text, s, lo):' \
    '        if False:' \
    "names (a party, a ministry, a venue) would be respelled."

mutant kebab-in-string-fixed "R8. " "$F" \
    '                elif "-" in tok:' \
    '                elif False:' \
    "a class list in a string would have one class renamed and the style would break silently."

mutant interpolation-fixed "R9. " "$F" \
    '                elif _masked(content, s - rs):' \
    '                elif False:' \
    "a variable named inside a format string would be renamed in the string only."

# --- 5. THE FRAGMENT IS JUDGED IN ITS FILE (C4) ----------------------------
mutant edit-context-ignored "X1. " "$F" \
    '        virtual, lo = content[:k] + new + content[k + replace_len:], k' \
    '        virtual, lo = new, 0' \
    "an Edit inside a fenced block or a code string would read as prose and rewrite code."

mutant replace-all-mixed-fixed "X9. " "$F" \
    '        if actions == {"fix"} and len(items) == len(places):' \
    '        if "fix" in actions:' \
    "one replacement string landing in prose and in a fence would be fixed in both."

mutant multiedit-quote-guessed "X10. " "$F" \
    '            if quoted:' \
    '            if False:' \
    "a MultiEdit quoting a word its own earlier edit wrote would be applied by guess: an unrelated line rewritten, or the inserted text missed."

mutant regex-fixed "R17. " "$F" \
    '                if rkind == "regex":' \
    '                if False:' \
    "a regular expression's pattern would be respelled and would stop matching what it matched."

mutant jsx-attribute-fixed "R16. " "$F" \
    '                elif lang == "js" and _attribute_value(text, rs):' \
    '                elif False:' \
    "a JSX class list would have one class renamed and its selector would stop matching."

# --- 6. THE EXEMPTIONS ARE THE PRODUCT, NOT THE TRIMMINGS ------------------
mutant bare-marker-exempts "B12. " "$F" \
    'DECLARED_EXEMPT_RE = re.compile(r"dialect-exempt:\s*[A-Za-z0-9]")' \
    'DECLARED_EXEMPT_RE = re.compile(r"dialect-exempt:")' \
    "a bare 'dialect-exempt:' would exempt anything, so the escape hatch becomes an off switch anyone can type."

mutant soft-queue-blocks "C1. " "$F" \
    '        soft.append((matched.strip(), line.strip()[:110]))' \
    '        findings.append(Finding(s, e, matched, "CEO-TODOs", "refuse", "", idx + 1))' \
    "'the queue' would become a hard block on a collocation nobody can adjudicate from one line."

mutant no-identifier-exemption "B5. " "$F" \
    '        if tok != run and looks_like_identifier_or_path(tok):\n            continue' \
    '        if False:\n            continue' \
    "kebab-case identifiers, CSS custom properties, URLs and file paths would all be treated as prose."

mutant no-word-boundaries "B3b. " "$F" \
    '        if s > 0 and is_word_char(text[s - 1]):\n            continue' \
    '        if False:\n            continue' \
    "a table word glued to digits or an underscore is part of a token, not a word."

mutant no-fence-exemption "B10. " "$F" \
    '            elif in_fence:\n                self.skip.append(True)' \
    '            elif in_fence:\n                self.skip.append(False)' \
    "every fenced code block and captured output block in the record becomes a violation."

mutant no-blockquote-exemption "B9. " "$F" \
    '                self.skip.append(line.lstrip().startswith(">")' \
    '                self.skip.append(False' \
    "the hook would rewrite quoted external material and the CEO's own verbatim words."

mutant no-inline-code-exemption "B8. " "$F" \
    'def _in_code_span(text, lo, p):\n    return text.count("`", lo, p) % 2 == 1' \
    'def _in_code_span(text, lo, p):\n    return False' \
    "naming a British-spelled identifier in prose would be impossible, including in this hook's own documentation."

mutant no-evidence-path-exemption "B15. " "$F" \
    '        if seg in EVIDENCE_SEGMENTS:' \
    '        if seg in ():' \
    "captured evidence — raw pages, transcripts, corpora, fixtures, run logs — would be rewritten, which destroys the evidence."

mutant evidence-read-above-the-repository "D4b. " "$F" \
    '    segment = phys[len(gov) + 1:] if gov and phys.startswith(gov + "/") else None' \
    '    segment = None' \
    "a repository that lives under a directory named logs/ or fixtures/ would have every file exempted as evidence, and British prose would pass in silence (2026-10-01)."

mutant no-vendor-legal-exemption "B20. " "$F" \
    '    if low_base.split(".")[0] in VENDOR_LEGAL:' \
    '    if low_base.split(".")[0] in ():' \
    "someone else's LICENSE/NOTICE text would be edited to suit our dialect."

mutant no-ext-exempt "B13. " "$F" \
    '        if ext and ext in exempt_exts:\n            continue' \
    '        if False:\n            continue' \
    "'grey' is a legal CSS named color; without the per-word extension exemption every stylesheet using it is refused."

mutant no-rename-narration "B26. " "$F" \
    '        if RENAME_CUE_RE.search(line[max(0, col - 60):col]):\n            continue\n        findings.append(' \
    '        findings.append(' \
    "the hook would forbid the sentence that documents the rename — you cannot record a rename without naming the old name."

# --- 7. THE VENDORING REGISTRY, AND THE ONE-WAY DOOR IT CLOSES ------------
# THREE CALL SITES CARRY THE SUPPRESSION, one per verdict that says something.
mutant no-vendored-exemption "V1. " "$G" \
    'typed.\n    _dg_is_vendored && exit 0' \
    'typed.' \
    "somebody else's prose would be silently rewritten again, and a verbatim re-vendoring from upstream would be changed by the hook that caused the divergence."

mutant no-vendored-exemption-on-refusal "V5b. " "$G" \
    '  REFUSE)\n    _dg_is_vendored && exit 0' \
    '  REFUSE)' \
    "a verbatim re-vendoring would be refused for a word in somebody else's file."

mutant no-vendored-exemption-on-soft "V13. " "$G" \
    "    # write never pays for it. A note about somebody else's document is noise.\n    _dg_is_vendored && exit 0" \
    "    # write never pays for it. A note about somebody else's document is noise." \
    "a vendored file would draw a CEO-TODOs note on every write, about wording in a document nobody here may change."

mutant vendored-exemption-ignores-origin "V2. " "$G" \
    '    vm_is_third_party "$rel" || return 1' \
    '    vm_covering "$rel" || return 1' \
    "every path the registry names would be exempt, including our own work — an off switch with an inventory attached."

# Aimed at V15, not V6: on a REFUSING verdict both spellings refuse and the
# mutant is invisible; the fail-closed only has observable behavior on a
# REPORTING one.
mutant vendored-broken-fails-open "V15. " "$G" \
    '            echo "(hook: scripts/hooks/dialect-guard.sh)"\n        } >&2\n        exit 2 ;;' \
    '            echo "(hook: scripts/hooks/dialect-guard.sh)"\n        } >&2\n        return 1 ;;' \
    "a declared-but-unreadable registry would print a banner and then judge the file anyway, on the assumption that it is ours."

# --- the verdict ------------------------------------------------------------
mut_pool_drain
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
