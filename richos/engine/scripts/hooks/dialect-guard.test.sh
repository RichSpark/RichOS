#!/usr/bin/env bash
#
# dialect-guard.test.sh — regression tests for scripts/hooks/dialect-guard.sh
# and the fixer it runs, scripts/lib/dialect/dialect-fix.py.
#
#   A. FIXES — British spelling the hook can fix mechanically comes back as an
#      updatedInput envelope with the American text, on all four tool shapes,
#      re-cased, with every change reported and no permissionDecision.
#   R. REFUSALS — what cannot be fixed mechanically still exits 2: `CEO queue`,
#      code identifiers, single-token string literals, markup/styles/config,
#      quotations, a capital mid-sentence, a hyphenated token or an
#      interpolation inside a code string.
#   B. NEGATIVES — nothing at all on clean text, identifiers, CSS custom
#      properties, URLs and paths, fenced code, inline code spans, blockquotes,
#      captured-evidence paths, vendor legal files, the legacy `.ceo-queue`
#      file name, rename narration, or American words that merely LOOK British.
#   X. EDIT CONTEXT (plan catch C4) — a fragment is judged where it lands in
#      the file: inside a fence, a quotation or a code string it is not prose.
#   F. FILE BYTES (plan catch C18) — the envelope applied the way the host
#      applies it, and the bytes read back from disk.
#   K. THE KEYED SAMPLE — no must-leave word is ever fixed, and no must-fix
#      word passes in silence.
#   C, D, E, V — the soft `queue` note, configuration and failure modes,
#      structure and registration, and vendored material.
#
# NOTE: this file is exempt from the hook by exact basename (dialect-fix.py's
# OWN_FILES) — it has to be, since it is made of the words the hook changes.
#
# Run directly: scripts/hooks/dialect-guard.test.sh
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
# shellcheck source=../lib/registered-hooks.sh
. "$ENGINE_ROOT/scripts/lib/registered-hooks.sh"

# Declare the governed repository rather than inheriting the launching
# session's: run from a session seated elsewhere, the hook would resolve THAT
# repository, find no adoption marker, stand down, and every case below would
# pass by never running.
RICHOS_ENTITY_ROOT="$ENGINE_ROOT"
export RICHOS_ENTITY_ROOT
unset CLAUDE_PROJECT_DIR

HOOK="$SCRIPT_DIR/dialect-guard.sh"
FIXER="$ENGINE_ROOT/scripts/lib/dialect/dialect-fix.py"
TABLE="$ENGINE_ROOT/scripts/lib/dialect/dialect-en-US.generated.dict"
DICT="$ENGINE_ROOT/scripts/lib/dialect-en-US.dict"

PASS=0
FAIL=0
SANDBOX="$(cd "$(mktemp -d -t dialect-guard-test.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s%s\n' "$1" "${2:+ — $2}"; FAIL=$((FAIL + 1)); }

# payload <tool> <file_path> <text...>
#   Write        content=<text1>
#   Edit         old_string=<text1> new_string=<text2> [replace_all if text3=all]
#   MultiEdit    pairs: <old1> <new1> <old2> <new2> ...
#   NotebookEdit new_source=<text1> [cell_type=<text2>]
#   anything else: a Write-shaped input under that tool name
payload() {
    python3 -c '
import json, sys
tool, fp = sys.argv[1], sys.argv[2]
v = sys.argv[3:]
if tool == "Edit":
    ti = {"file_path": fp, "old_string": v[0], "new_string": v[1]}
    if len(v) > 2 and v[2] == "all":
        ti["replace_all"] = True
elif tool == "MultiEdit":
    ti = {"file_path": fp, "edits": [{"old_string": v[i], "new_string": v[i + 1]} for i in range(0, len(v), 2)]}
elif tool == "NotebookEdit":
    ti = {"notebook_path": fp, "new_source": v[0], "cell_id": "c1"}
    if len(v) > 1:
        ti["cell_type"] = v[1]
else:
    ti = {"file_path": fp, "content": v[0]}
print(json.dumps({"tool_name": tool, "tool_input": ti}))
' "$@"
}

# hook_run <tool> <file_path> <text...> -> RC, OUT (stdout), ERR (stderr)
hook_run() {
    payload "$@" > "$SANDBOX/payload.json"
    "$HOOK" < "$SANDBOX/payload.json" >"$SANDBOX/out" 2>"$SANDBOX/err"
    RC=$?
    OUT="$(cat "$SANDBOX/out")"
    ERR="$(cat "$SANDBOX/err")"
}

# field <python expression over o, the hookSpecificOutput> -> printed value
field() {
    printf '%s' "$OUT" | python3 -c '
import json, sys
try:
    o = json.load(sys.stdin)["hookSpecificOutput"]
except Exception:
    print("<no envelope>"); raise SystemExit
print(eval(sys.argv[1]))' "$1" 2>/dev/null
}

# case_exit <name> <expected-exit> <tool> <file_path> <text...>
case_exit() {
    local name="$1" want="$2"; shift 2
    hook_run "$@"
    [ "$RC" -eq "$want" ] && ok "$name" || bad "$name" "expected exit $want, got $RC"
}

# case_quiet <name> <tool> <file_path> <text...>: exit 0 and NOTHING on stdout
case_quiet() {
    local name="$1"; shift
    hook_run "$@"
    if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then ok "$name"
    else bad "$name" "expected exit 0 and no envelope, got exit $RC, stdout: ${OUT:0:160}"; fi
}

# case_fixed <name> <python expr> <expected> <tool> <file_path> <text...>
case_fixed() {
    local name="$1" expr="$2" want="$3"; shift 3
    hook_run "$@"
    local got
    got="$(field "$expr")"
    if [ "$RC" -eq 0 ] && [ "$got" = "$want" ]; then ok "$name"
    else bad "$name" "exit $RC, $expr = [$got], expected [$want]${ERR:+; stderr: ${ERR:0:200}}"; fi
}

# case_msg <name> <needle> <tool> <file_path> <text...>: stderr carries needle
case_msg() {
    local name="$1" needle="$2"; shift 2
    hook_run "$@"
    printf '%s' "$ERR" | grep -qF -- "$needle" && ok "$name" \
        || bad "$name" "stderr did not mention \"$needle\""
}

# case_refused <name> <reason needle> <tool> <file_path> <text...>: exit 2 AND the
# refusal names why this class cannot be fixed (main refused everything alike).
case_refused() {
    local name="$1" needle="$2"; shift 2
    hook_run "$@"
    if [ "$RC" -eq 2 ] && printf '%s' "$ERR" | grep -qF -- "$needle"; then ok "$name"
    else bad "$name" "expected exit 2 naming \"$needle\", got exit $RC"; fi
}

MD="$SANDBOX/subject.md"
C='o["updatedInput"]["content"]'
N='o["updatedInput"]["new_string"]'

echo "=== A. FIXES — British spelling comes back American ==="

case_fixed "A1. Write: 'colour' in prose is fixed"              "$C" "The color of the button is wrong." Write "$MD" "The colour of the button is wrong."
printf 'intro\nThis is the expected behaviour.\n' > "$SANDBOX/a2.md"
case_fixed "A2. Edit: 'behaviour' is fixed"                     "$N" "the expected behavior" Edit "$SANDBOX/a2.md" "intro" "the expected behaviour"
printf 'one\ntwo\nthree\n' > "$SANDBOX/a3.md"
case_fixed "A3. MultiEdit: only the dirty edit changes"         'o["updatedInput"]["edits"][1]["new_string"] + "|" + o["updatedInput"]["edits"][0]["new_string"]' "the license says so|all clean here" MultiEdit "$SANDBOX/a3.md" "one" "all clean here" "two" "the licence says so"
case_fixed "A4. NotebookEdit markdown cell: 'defence' is fixed" 'o["updatedInput"]["new_source"]' "the only defense available" NotebookEdit "$SANDBOX/x.ipynb" "the only defence available" markdown
case_fixed "A5. 'judgement' is fixed"                           "$C" "offered as engineering judgment rather than fact" Write "$MD" "offered as engineering judgement rather than fact"
case_fixed "A6. 'whilst' is fixed"                              "$C" "while the deploy runs" Write "$MD" "whilst the deploy runs"
case_fixed "A7. 'towards' is fixed"                             "$C" "one step toward the answer" Write "$MD" "one step towards the answer"
case_fixed "A8. 'analysed' is fixed"                            "$C" "we analyzed the payload" Write "$MD" "we analysed the payload"
case_fixed "A9. 'catalogue' is fixed"                           "$C" "a catalog of failures" Write "$MD" "a catalogue of failures"
case_fixed "A10. 'artefacts' is fixed"                          "$C" "applies to artifacts, not promises" Write "$MD" "applies to artefacts, not promises"
case_fixed "A11. 'organisation' is fixed"                       "$C" "the organization of the record" Write "$MD" "the organisation of the record"
case_fixed "A12. 'grey' in a .md file is fixed"                 "$C" "a gray line under the header" Write "$MD" "a grey line under the header"
case_fixed "A13. re-cased at a sentence start: Colour -> Color" "$C" "Color is a product decision." Write "$MD" "Colour is a product decision."
case_fixed "A14. re-cased in capitals: LICENCE -> LICENSE"      "$C" "THE LICENSE OF RECORD" Write "$MD" "THE LICENCE OF RECORD"
case_fixed "A15. a word from VarCon's table, not the old 173"   "$C" "the aluminum casing" Write "$MD" "the aluminium casing"
case_fixed "A16. a hyphenated compound in prose is fixed"       "$C" "a gray-box test" Write "$MD" "a grey-box test"
case_fixed "A17. code: a comment is fixed"                      "$C" "// the color of it
let x = 1;" Write "$SANDBOX/a.rs" "// the colour of it
let x = 1;"
case_fixed "A18. code: a sentence in a string literal is fixed" "$C" 'label = "Pick a nice color"' Write "$SANDBOX/a.py" 'label = "Pick a nice colour"'
case_fixed "A19. code: a shell comment and a quoted sentence"   "$C" '# the behavior of it
echo "the color is wrong"' Write "$SANDBOX/a.sh" '# the behaviour of it
echo "the colour is wrong"'
case_fixed "A20. the envelope never sets permissionDecision"    '"permissionDecision" in o' "False" Write "$MD" "the colour"
case_fixed "A21. the whole tool_input is copied, file_path kept" 'o["updatedInput"]["file_path"]' "$MD" Write "$MD" "the colour"
case_fixed "A22. the change report names each change, with a count and lines" \
    '"colour → color ×2 (lines 1, 3)" in o["additionalContext"] and "behaviour → behavior (line 2)" in o["additionalContext"]' "True" \
    Write "$MD" "the colour
the behaviour
the colour"
case_fixed "A23. the report tells the model to quote the American form" '"old_string" in o["additionalContext"]' "True" Write "$MD" "the colour"

echo
echo "=== R. REFUSALS — what cannot be fixed mechanically still blocks ==="

case_refused "R1. 'CEO queue' is refused"                       "a rename, not a spelling" Write "$MD" "13 items are in the CEO queue right now."
case_refused "R2. \"CEO's queue\" is refused"                   "a rename, not a spelling" Write "$MD" "It sat in the CEO's queue for a week."
case_msg  "R2b. the queue refusal names CEO-TODOs as the fix"   "CEO-TODOs" Write "$MD" "13 items are in the CEO queue."
case_refused "R3. a single-token string literal is refused"     "single-token string literal" Write "$SANDBOX/a.js" 'const k = "colour";'
case_refused "R4. a code identifier is refused"                 "renaming an identifier changes behavior" Write "$SANDBOX/a.rs" 'let colour = 1;'
case_refused "R5. markup is refused"                            "markup, styles or config" Write "$SANDBOX/a.html" '<p>the colour of it</p>'
case_refused "R5b. config is refused"                           "markup, styles or config" Write "$SANDBOX/a.json" '{"label": "the colour of it"}'
case_refused "R5c. a style is refused"                          "markup, styles or config" Write "$SANDBOX/a.css" '.x { content: "the colour" }'
case_refused "R6. a quotation in prose is refused"              "inside a quotation" Write "$MD" 'He wrote "the colour of it is wrong" and left.'
case_refused "R7. a capital mid-sentence is refused (a name)"   "capitalized mid-sentence" Write "$MD" "We met the Labour Party today."
case_refused "R8. a hyphenated token in a code string is refused" "hyphenated token inside a string" Write "$SANDBOX/a.js" 'el.className = "btn text-colour";'
case_refused "R9. an interpolation in a code string is refused" "interpolation or a format field" Write "$SANDBOX/a.py" 'msg = f"the {colour} value"'
case_refused "R10. markup inside a code string is refused"      "markup inside a string" Write "$SANDBOX/a.js" 'html = "<span>the colour</span>";'
case_msg  "R11. the refusal is re-cased: Colour -> Color"       "Colour -> Color" Write "$MD" "We met the Colour Party today."
case_msg  "R12. the refusal names the file"                     "$MD" Write "$MD" "the CEO queue"
case_msg  "R13. the refusal names the declared-exemption route" "dialect-exempt:" Write "$MD" "the CEO queue"
case_msg  "R14. the refusal says the rest will be fixed on retry" "1 other spelling here will be fixed automatically" Write "$MD" "the colour of the CEO queue"

echo
echo "=== B. NEGATIVES — nothing at all ==="

case_quiet "B1. clean American prose"                          Write "$MD" "The color of the button is fine and the behavior is correct."
case_quiet "B2. American 'license' is never touched"           Write "$MD" "the license of record is a documentation page"
case_quiet "B3. camelCase identifier 'colourPicker'"           Write "$SANDBOX/x.js" "const colourPicker = makeThing();"
case_quiet "B3b. a word glued to a digit is not a word"        Write "$MD" "the 2colour code"
case_quiet "B4. snake_case identifier 'text_colour'"           Write "$SANDBOX/x.py" "text_colour = 3"
case_quiet "B5. CSS custom property '--brand-colour'"          Write "$SANDBOX/x.js" "el.style.setProperty('--brand-colour', v)"
case_quiet "B6. a path token containing 'licence'"             Write "$MD" "see docs/reference/licence-snapshots/page.txt for the capture"
case_quiet "B7. a URL containing 'licence'"                    Write "$MD" "fetched from https://example.com/legal/licence today"
case_quiet "B8. inline code span \`colour\`"                   Write "$MD" "the CSS token is \`colour\` in that vendor's theme"
case_quiet "B9. blockquote line (quoted external material)"    Write "$MD" "He wrote:
> the colour of it is not our judgement to make
and that settles it."
case_quiet "B10. fenced code block"                            Write "$MD" "before
\`\`\`js
const colour = 1; // behaviour
\`\`\`
after"
case_quiet "B11. declared 'dialect-exempt: <reason>'"          Write "$MD" "the colour of it   <!-- dialect-exempt: verbatim from the vendor's own page -->"
case_fixed "B12. a BARE 'dialect-exempt:' exempts nothing"     "$C" "the color of it   <!-- dialect-exempt: -->" Write "$MD" "the colour of it   <!-- dialect-exempt: -->"
case_quiet "B13. 'grey' in a .css file is a legal CSS color"   Write "$SANDBOX/x.css" "a { color: grey; }"
case_quiet "B14. 'grey' in a .scss file"                       Write "$SANDBOX/x.scss" "\$line: grey;"
case_quiet "B15. captured evidence under wiki/raw/"            Write "$SANDBOX/wiki/raw/page.md" "the colour of the licence"
case_quiet "B16. captured transcript under docs/cold-open/"    Write "$SANDBOX/docs/cold-open/2026-08-31.md" "not part of the 13-item queue; the behaviour is odd"
case_quiet "B16b. a QUALIFIED evidence dir: license-snapshots/" Write "$SANDBOX/docs/reference/license-snapshots/page.txt" "the licence of record, as they wrote it"
case_fixed "B16c. ...but 'raw-notes/' is still prose, not raw" "$C" "the color of it" Write "$SANDBOX/docs/raw-notes/thoughts.md" "the colour of it"
case_quiet "B17. a .log run output"                            Write "$SANDBOX/out/run.log" "recognised: colour behaviour licence"
case_quiet "B18. a .jsonl corpus"                              Write "$SANDBOX/data/rows.jsonl" "{\"t\":\"colour\"}"
case_quiet "B19. anything under node_modules/"                 Write "$SANDBOX/node_modules/pkg/readme.md" "the colour option"
case_quiet "B20. vendor LICENSE.md"                            Write "$SANDBOX/LICENSE.md" "This licence is granted under..."
case_quiet "B21. vendor NOTICE"                                Write "$SANDBOX/NOTICE" "licence terms, unmodified"
case_quiet "B22. package-lock.json"                            Write "$SANDBOX/package-lock.json" "{\"licence\": \"x\"}"
case_quiet "B23. the old hand-kept dictionary"                 Write "$DICT" "colour	color"
DIALECT_DATA="$ENGINE_ROOT/scripts/lib/dialect"
case_quiet "B23b. the generated table"                         Write "$DIALECT_DATA/dialect-en-US.generated.dict" "colour	color"
case_quiet "B23c. the table's override list"                   Write "$DIALECT_DATA/dialect-en-US.overrides.dict" "colour	color"
case_quiet "B23d. the table's leave list"                      Write "$DIALECT_DATA/dialect-en-US.leave.tsv" "colour	a reason"
case_quiet "B23f. this suite"                                  Write "$SCRIPT_DIR/dialect-guard.test.sh" "# the colour"
case_quiet "B23g. the mutation script"                         Write "$SCRIPT_DIR/dialect-guard.mutation.sh" "# the colour"
case_fixed "B23e. ...but a lookalike name is still checked"    "$C" "the color of it" Write "$DIALECT_DATA/notes-dialect-en-US.generated.md" "the colour of it"
case_quiet "B24. legacy '.ceo-queue' file name (identifier)"   Write "$MD" "a pre-rename .ceo-queue is still read"
case_quiet "B25. 'ceo_queue' identifier"                       Write "$SANDBOX/x.py" "ceo_queue = load()"
case_quiet "B26. rename narration: 'called the CEO queue'"     Write "$MD" "It was called the CEO queue; it is now CEO-TODOs."
case_quiet "B27. rename narration: 'renamed the CEO queue'"    Write "$MD" "the instruction that also renamed the CEO queue"
case_quiet "B28. 'analyses' is already American"               Write "$MD" "three analyses of the same page"
case_quiet "B29. 'dialogue' is American too"                   Write "$MD" "a dialogue between the two of them"
case_quiet "B30. 'practice' the noun"                          Write "$MD" "common practice in this repository"
case_quiet "B31. 'cancellation' keeps its double L"            Write "$MD" "a cancellation policy"
case_quiet "B32. 'controlled' keeps its double L"              Write "$MD" "a controlled rollout"
case_quiet "B33. 'surprise'/'advertise' are -ise in both"      Write "$MD" "no surprise; we advertise nothing"
case_quiet "B34. a non-Write/Edit tool is ignored"             Bash  "$MD" "the colour is wrong"
case_quiet "B36. 'enrollment'/'enrolled' are ALREADY American" Write "$MD" "Apple Developer enrollment; he enrolled last year"
case_fixed "B37. ...but British 'enrolment' is fixed"          "$C" "the enrollment is pending" Write "$MD" "the enrolment is pending"
case_quiet "B38. a leave-list word stays: echo canceller"      Write "$MD" "the echo canceller runs first"

# Malformed payload: FAIL OPEN. Governed by a sandbox COPY of the engine's
# adoption: an unreadable payload appends a line to <entity>/.claude/state/
# unevaluated-payloads.log, which must not land in the checkout under test.
ENT_MALFORMED="$SANDBOX/ent-malformed"
mkdir -p "$ENT_MALFORMED"
cp "$ENGINE_ROOT/orchestration.config" "$ENT_MALFORMED/orchestration.config"
printf '%s' 'not json at all {{{' | RICHOS_ENTITY_ROOT="$ENT_MALFORMED" "$HOOK" >/dev/null 2>&1
[ $? -eq 0 ] && ok "B35. malformed payload fails OPEN (exit 0)" \
             || bad "B35. malformed payload fails OPEN (exit 0)"

echo
echo "=== X. EDIT CONTEXT — a fragment is judged where it lands (C4) ==="

printf 'intro\n```\nx = 1\n```\nafter\n' > "$SANDBOX/x1.md"
case_quiet "X1. Edit INSIDE a fenced block is left alone"      Edit "$SANDBOX/x1.md" "x = 1" "colour = 1"
case_fixed "X2. ...and the same file outside the fence is fixed" "$N" "after the color" Edit "$SANDBOX/x1.md" "after" "after the colour"
printf 'He said "the old text" and left.\n' > "$SANDBOX/x3.md"
case_refused "X3. Edit inside a quotation is refused, not fixed" "inside a quotation" Edit "$SANDBOX/x3.md" "the old text" "the colour"
printf 'const k = "KEY";\n// a note\n' > "$SANDBOX/x4.js"
case_refused "X4. Edit inside a single-token JS string is refused" "single-token string literal" Edit "$SANDBOX/x4.js" "KEY" "colour"
case_fixed "X5. Edit inside a JS comment is fixed"             "$N" "a color note" Edit "$SANDBOX/x4.js" "a note" "a colour note"
case_quiet "X6. old_string NOT in the file: nothing is done"   Edit "$SANDBOX/x4.js" "not in the file" "the colour"
case_quiet "X7. the file does not exist: nothing is done"      Edit "$SANDBOX/missing.md" "anything" "the colour"
case_fixed "X8. old_string empty on a new file: judged as a Write" "$N" "the color" Edit "$SANDBOX/new-file.md" "" "the colour"
printf 'A: TOKEN\n```\nTOKEN\n```\n' > "$SANDBOX/x9.md"
case_refused "X9. replace_all landing in prose AND a fence is refused" "needs different treatment" Edit "$SANDBOX/x9.md" "TOKEN" "the colour" all
printf 'one\ntwo\n' > "$SANDBOX/x10.md"
case_fixed "X10. MultiEdit quoting an earlier edit's fixed text" 'o["updatedInput"]["edits"][1]["old_string"]' "the color" MultiEdit "$SANDBOX/x10.md" "one" "the colour" "the colour" "the colour, again"
printf 'top\n```\nTOKEN\n```\n' > "$SANDBOX/x11.md"
case_quiet "X11. MultiEdit inside a fence is left alone"       MultiEdit "$SANDBOX/x11.md" "TOKEN" "colour = 1"
case_fixed "X12. NotebookEdit code cell: a comment is fixed"   'o["updatedInput"]["new_source"]' "# the color
x = 1" NotebookEdit "$SANDBOX/x.ipynb" "# the colour
x = 1" code
case_refused "X13. NotebookEdit code cell: code is refused"   "renaming an identifier changes behavior" NotebookEdit "$SANDBOX/x.ipynb" "colour = 1" code
printf '{"cells": [{"id": "c1", "cell_type": "markdown", "source": []}]}' > "$SANDBOX/nb.ipynb"
case_fixed "X14. NotebookEdit with no cell_type reads the cell's own type" 'o["updatedInput"]["new_source"]' "the color" NotebookEdit "$SANDBOX/nb.ipynb" "the colour"

echo
echo "=== F. FILE BYTES — the envelope applied as the host applies it (C18) ==="
# The host replaces tool_input with updatedInput and runs the tool. This applies
# the returned input the same way (Write writes content; Edit and MultiEdit
# replace old_string with new_string in order) and reads the bytes back. Run 5
# of the plan proves the same thing through a real Claude Code session.
apply_and_read() { # <payload.json> <hook stdout> -> the file's bytes after the tool runs
    python3 - "$1" "$2" <<'APPLY'
import json, sys
payload = json.load(open(sys.argv[1]))
raw = open(sys.argv[2]).read().strip()
ti = json.loads(raw)["hookSpecificOutput"]["updatedInput"] if raw else payload["tool_input"]
tool, path = payload["tool_name"], ti.get("file_path")
if tool == "Write":
    text = ti["content"]
else:
    text = open(path).read()
    for ed in (ti["edits"] if tool == "MultiEdit" else [ti]):
        assert ed["old_string"] in text, "old_string not found: %r" % ed["old_string"]
        text = text.replace(ed["old_string"], ed["new_string"], -1 if ed.get("replace_all") else 1)
open(path, "w").write(text)
sys.stdout.write(open(path, "rb").read().decode())
APPLY
}
bytes_case() { # <name> <expected bytes> <tool> <file> <text...>
    local name="$1" want="$2"; shift 2
    hook_run "$@"
    local got
    got="$(apply_and_read "$SANDBOX/payload.json" "$SANDBOX/out" 2>&1)"
    [ "$RC" -eq 0 ] && [ "$got" = "$want" ] && ok "$name" || bad "$name" "exit $RC, bytes: [$got]"
}
bytes_case "F1. Write: the file on disk is American" "The color of the behavior." Write "$SANDBOX/f1.md" "The colour of the behaviour."
printf 'text\n```\nTOKEN\n```\nend\n' > "$SANDBOX/f2.md"
bytes_case "F2. Edit inside a fence: the bytes are exactly as typed" "$(printf 'text\n```\ncolour = 1\n```\nend')" Edit "$SANDBOX/f2.md" "TOKEN" "colour = 1"
printf 'one\ntwo\n' > "$SANDBOX/f3.md"
bytes_case "F3. MultiEdit chain: every edit applies, the bytes are American" "$(printf 'the color, again\nthe behavior')" MultiEdit "$SANDBOX/f3.md" "one" "the colour" "the colour" "the colour, again" "two" "the behaviour"

echo
echo "=== K. THE KEYED SAMPLE — never a wrong fix, never a silent pass ==="
KEYED="$ENGINE_ROOT/scripts/lib/dialect/fixtures/british-keyed.txt"
if [ -s "$KEYED" ]; then
    KOUT="$(python3 - "$FIXER" "$TABLE" "$KEYED" <<'KEYED_PY'
import importlib.util, re, sys
spec = importlib.util.spec_from_file_location("dialect_fix", sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
table = m.load_table(sys.argv[2])
keyed = open(sys.argv[3], encoding="utf-8").read()
items = list(re.finditer(r"\{([^|{}]+)\|([^{}]*)\}", keyed))
plain, spans, pos = [], [], 0
for it in items:
    plain.append(keyed[pos:it.start()])
    start = sum(len(p) for p in plain)
    plain.append(it.group(1))
    spans.append((start, start + len(it.group(1)), it.group(1), it.group(2)))
    pos = it.end()
plain.append(keyed[pos:])
text = "".join(plain)
found, _ = m.scan(text, "prose", None, "md", table)
by_start = {f.start: f for f in found}
wrong, silent, fixed, refused = [], [], 0, 0
for s, e, word, key in spans:
    f = by_start.get(s)
    if key == "=":
        if f and f.action == "fix":
            wrong.append(word)
    elif f is None:
        silent.append(word)
    elif f.action == "fix":
        fixed += 1
    else:
        refused += 1
print("%d %d %d %s|%s" % (fixed, refused, len(spans), ",".join(wrong), ",".join(silent)))
KEYED_PY
)"
    K_FIXED=""; K_REFUSED=""; K_TOTAL=""; K_REST=""
    read -r K_FIXED K_REFUSED K_TOTAL K_REST <<<"$KOUT"
    K_WRONG="${K_REST%%|*}"; K_SILENT="${K_REST#*|}"
    # NOT EVALUATED IS NOT CLEAN: with no fixer to import the scan prints nothing,
    # and both checks below would pass over it (found by running this suite on main).
    if ! [ "${K_TOTAL:-0}" -ge 100 ] 2>/dev/null || ! [ "${K_FIXED:-0}" -ge 1 ] 2>/dev/null; then
        bad "K0. the keyed sample was evaluated" "scan output: [${KOUT:0:160}]"
    fi
    [ -n "$K_TOTAL" ] && [ -z "$K_WRONG" ] && ok "K1. no must-leave word of the keyed sample is fixed" \
                      || bad "K1. no must-leave word of the keyed sample is fixed" "fixed: $K_WRONG"
    [ -n "$K_TOTAL" ] && [ -z "$K_SILENT" ] && ok "K2. no must-fix word passes in silence ($K_FIXED fixed, $K_REFUSED refused, of $K_TOTAL items)" \
                       || bad "K2. no must-fix word passes in silence" "silent: $K_SILENT"
else
    bad "K1. the keyed sample is present" "missing: $KEYED"
fi

echo
echo "=== C. THE 'queue' COLLOCATION — reported, never blocked ==="

case_exit  "C1. 'the queue' near CEO is REPORTED, not blocked" 0 Write "$MD" "The CEO reads it first; nothing unprepared sits in the queue."
case_fixed "C2. ...and the model is told why it was not blocked" '"not blocked" in o["additionalContext"] and "updatedInput" not in o' "True" Write "$MD" "The CEO reads it first; nothing unprepared sits in the queue."
case_quiet "C3. 'his queue' with no CEO/TODO context is clean" Write "$MD" "the worker drains his queue every second"
case_exit  "C4. a hard finding + a soft one still exits 2"     2 Write "$MD" "The CEO queue is the queue."
case_fixed "C5. a fix + a soft note travel in one envelope"    '"not blocked" in o["additionalContext"] and o["updatedInput"]["content"].startswith("The color")' "True" Write "$MD" "The colour: the CEO reads the queue."

echo
echo "=== D. CONFIGURATION AND FAILURE MODES ==="

mk_entity() {   # mk_entity <dir> <config-lines>
    mkdir -p "$1"
    printf '%s\n' "$2" > "$1/orchestration.config"
}

ENT_BLANK="$SANDBOX/entity-blank"
mk_entity "$ENT_BLANK" 'DIALECT_TARGET=""'
payload Write "$MD" "the colour is wrong" \
    | RICHOS_ENTITY_ROOT="$ENT_BLANK" "$HOOK" >"$SANDBOX/d1.out" 2>"$SANDBOX/d1.err"
D1_RC=$?
if [ "$D1_RC" -eq 0 ] && [ ! -s "$SANDBOX/d1.err" ] && [ ! -s "$SANDBOX/d1.out" ]; then
    ok "D1. blank DIALECT_TARGET is a SILENT no-op (exit 0, nothing on either stream)"
else
    bad "D1. blank DIALECT_TARGET is a SILENT no-op" "rc=$D1_RC stdout=$(cat "$SANDBOX/d1.out") stderr=$(cat "$SANDBOX/d1.err")"
fi

ENT_FR="$SANDBOX/entity-fr"
mk_entity "$ENT_FR" 'DIALECT_TARGET="fr-FR"'
payload Write "$MD" "the colour is wrong" \
    | RICHOS_ENTITY_ROOT="$ENT_FR" "$HOOK" >"$SANDBOX/d2.out" 2>"$SANDBOX/d2.err"
D2_RC=$?
if [ "$D2_RC" -eq 0 ] && grep -qF 'fr-FR' "$SANDBOX/d2.err" && [ ! -s "$SANDBOX/d2.out" ]; then
    ok "D2. a foreign DIALECT_TARGET is an ANNOUNCED no-op, never a pretend enforcement"
else
    bad "D2. a foreign DIALECT_TARGET is an ANNOUNCED no-op" "rc=$D2_RC stderr=$(cat "$SANDBOX/d2.err")"
fi

ENT_ALLOW="$SANDBOX/entity-allow"
mk_entity "$ENT_ALLOW" 'DIALECT_TARGET="en-US"
DIALECT_SCAN_ALLOWLIST="Colour Party"'
payload Write "$MD" "the Colour Party is a proper noun" \
    | RICHOS_ENTITY_ROOT="$ENT_ALLOW" "$HOOK" >"$SANDBOX/d3.out" 2>/dev/null
[ $? -eq 0 ] && [ ! -s "$SANDBOX/d3.out" ] && ok "D3. DIALECT_SCAN_ALLOWLIST exempts a literal substring" \
             || bad "D3. DIALECT_SCAN_ALLOWLIST exempts a literal substring"

ENT_PATHS="$SANDBOX/entity-paths"
mk_entity "$ENT_PATHS" 'DIALECT_TARGET="en-US"
DIALECT_EXEMPT_PATHS="/imported/"'
payload Write "$SANDBOX/imported/doc.md" "the colour is wrong" \
    | RICHOS_ENTITY_ROOT="$ENT_PATHS" "$HOOK" >"$SANDBOX/d4.out" 2>/dev/null
[ $? -eq 0 ] && [ ! -s "$SANDBOX/d4.out" ] && ok "D4. DIALECT_EXEMPT_PATHS exempts a path substring" \
             || bad "D4. DIALECT_EXEMPT_PATHS exempts a path substring"

# D4b/D4c — EVIDENCE IS A PLACE INSIDE THE REPOSITORY, NOT ABOVE IT. A governed
# repository that LIVES under directories named logs/ and fixtures/ is still
# governed (2026-10-01: Layer T's canary passed in silence inside the merge
# gate's private profile). D4c keeps the exemption itself honest.
ENT_NESTED="$SANDBOX/logs/fixtures/run-1/tmp/entity-nested"
mk_entity "$ENT_NESTED" 'DIALECT_TARGET="en-US"'
mkdir -p "$ENT_NESTED/docs" "$ENT_NESTED/logs"
payload Write "$ENT_NESTED/docs/page.md" "the colour is wrong" \
    | RICHOS_ENTITY_ROOT="$ENT_NESTED" "$HOOK" >"$SANDBOX/d4b.out" 2>/dev/null
D4B_RC=$?
if [ "$D4B_RC" -eq 0 ] && grep -qF 'the color is wrong' "$SANDBOX/d4b.out"; then
    ok "D4b. a repository living under logs/fixtures/ directories is still checked"
else
    bad "D4b. a repository living under logs/fixtures/ directories is still checked" "rc=$D4B_RC, no fix emitted"
fi
payload Write "$ENT_NESTED/logs/notes.md" "the colour is wrong" \
    | RICHOS_ENTITY_ROOT="$ENT_NESTED" "$HOOK" >"$SANDBOX/d4c.out" 2>/dev/null
D4C_RC=$?
[ "$D4C_RC" -eq 0 ] && [ ! -s "$SANDBOX/d4c.out" ] && ok "D4c. ...and a logs/ directory INSIDE that repository is still captured evidence" \
                   || bad "D4c. ...and a logs/ directory INSIDE that repository is still captured evidence" "rc=$D4C_RC stdout=$(cat "$SANDBOX/d4c.out")"

# D5 — a governed repository with no table is FAIL-CLOSED, never a quiet pass.
EMPTY_ENGINE="$SANDBOX/empty-engine"
mkdir -p "$EMPTY_ENGINE/scripts/lib"
payload Write "$MD" "the colour is wrong" \
    | RICHOS_ENGINE_ROOT="$EMPTY_ENGINE" "$HOOK" >/dev/null 2>&1
[ $? -eq 2 ] && ok "D5. a missing table is FAIL-CLOSED (exit 2)" \
             || bad "D5. a missing table is FAIL-CLOSED (exit 2)"

# D5b/D5c/D7 — a sandbox copy of the hook and its libraries, so the fixer and
# the table can be broken without touching the tree under test.
mk_engine_copy() { # <dir>
    mkdir -p "$1/scripts/hooks" "$1/scripts/lib/dialect"
    cp "$HOOK" "$1/scripts/hooks/"
    cp "$ENGINE_ROOT/scripts/lib/resolve-roots.sh" "$ENGINE_ROOT/scripts/lib/resolve-main-checkout.sh" \
       "$ENGINE_ROOT/scripts/lib/declaration-path.sh" "$ENGINE_ROOT/scripts/lib/seat-jurisdiction.sh" \
       "$ENGINE_ROOT/scripts/lib/vendored-material.sh" "$1/scripts/lib/"
    cp "$FIXER" "$TABLE" "$1/scripts/lib/dialect/"
    cp "$ENGINE_ROOT/orchestration.config" "$1/"
}
NOFIX="$SANDBOX/engine-nofixer"
mk_engine_copy "$NOFIX"
rm -f "$NOFIX/scripts/lib/dialect/dialect-fix.py"
payload Write "$MD" "the colour is wrong" \
    | RICHOS_ENTITY_ROOT="$NOFIX" RICHOS_ENGINE_ROOT="$NOFIX" "$NOFIX/scripts/hooks/dialect-guard.sh" >/dev/null 2>&1
[ $? -eq 2 ] && ok "D5b. a missing fixer is FAIL-CLOSED (exit 2)" \
             || bad "D5b. a missing fixer is FAIL-CLOSED (exit 2)"
EMPTYTAB="$SANDBOX/engine-emptytable"
mk_engine_copy "$EMPTYTAB"
printf '# nothing here\n' > "$EMPTYTAB/scripts/lib/dialect/dialect-en-US.generated.dict"
payload Write "$MD" "the colour is wrong" \
    | RICHOS_ENTITY_ROOT="$EMPTYTAB" RICHOS_ENGINE_ROOT="$EMPTYTAB" "$EMPTYTAB/scripts/hooks/dialect-guard.sh" >/dev/null 2>&1
[ $? -eq 2 ] && ok "D5c. a table that parses to nothing is FAIL-CLOSED (exit 2)" \
             || bad "D5c. a table that parses to nothing is FAIL-CLOSED (exit 2)"
CRASH="$SANDBOX/engine-crash"
mk_engine_copy "$CRASH"
printf 'raise RuntimeError("broken fixer")\n' > "$CRASH/scripts/lib/dialect/dialect-fix.py"
payload Write "$MD" "the colour is wrong" \
    | RICHOS_ENTITY_ROOT="$CRASH" RICHOS_ENGINE_ROOT="$CRASH" "$CRASH/scripts/hooks/dialect-guard.sh" >/dev/null 2>&1
[ $? -eq 2 ] && ok "D7. a fixer that CRASHES refuses (exit 2), never passes the write" \
             || bad "D7. a fixer that CRASHES refuses (exit 2), never passes the write"

# D6 — no python3 on PATH: fail closed, like every sibling.
BINLESS="$SANDBOX/binless"
mkdir -p "$BINLESS"
payload Write "$MD" "the colour is wrong" > "$SANDBOX/d6.json"
env PATH="$BINLESS" /bin/bash "$HOOK" < "$SANDBOX/d6.json" >/dev/null 2>&1
[ $? -eq 2 ] && ok "D6. a missing python3 is FAIL-CLOSED (exit 2)" \
             || bad "D6. a missing python3 is FAIL-CLOSED (exit 2)"

echo
echo "=== E. STRUCTURE — wired on its own, hashed, single-sourced ==="

grep -q '\. "\$_RR_LIB"' "$HOOK" \
    && ok "E1. dialect-guard.sh sources the root-resolution contract" \
    || bad "E1. dialect-guard.sh sources the root-resolution contract"

hook_enforced_on_surface "$ENGINE_ROOT/hooks/hooks.json" dialect-guard.sh \
    && ok "E2. registered on the plugin surface (hooks/hooks.json)" \
    || bad "E2. registered on the plugin surface (hooks/hooks.json)"

hook_enforced_on_surface "$ENGINE_ROOT/.claude/settings.local.json" dialect-guard.sh \
    && ok "E3. registered on the seated surface (.claude/settings.local.json)" \
    || bad "E3. registered on the seated surface (.claude/settings.local.json)"

# E2b-E2d — HOW it is registered is the point. An updatedInput emitter folded
# into the Write dispatcher is the merge dispatch-pretooluse.sh refuses, and a
# second emitter on Bash would race shell-evidence.sh for the one command.
REG="$(python3 - "$ENGINE_ROOT/hooks/hooks.json" "$ENGINE_ROOT/.claude/settings.local.json" <<'REG_PY'
import json, sys
out = []
for path in sys.argv[1:]:
    d = json.load(open(path))
    for entry in d.get("hooks", {}).get("PreToolUse", []):
        for h in entry.get("hooks", []):
            if "dialect-guard.sh" in h.get("command", ""):
                out.append(entry.get("matcher", ""))
print(" ".join(sorted(out)))
REG_PY
)"
[ "$REG" = "Write|Edit|MultiEdit|NotebookEdit Write|Edit|MultiEdit|NotebookEdit" ] \
    && ok "E2b. registered DIRECTLY on Write|Edit|MultiEdit|NotebookEdit, once per surface, never on Bash" \
    || bad "E2b. registered DIRECTLY on Write|Edit|MultiEdit|NotebookEdit, once per surface" "matchers: [$REG]"
if grep -v '^#' "$SCRIPT_DIR/dispatch-pretooluse.manifest" | grep -q 'dialect-guard\.sh'; then
    bad "E2c. not a module of the Write dispatcher (it emits updatedInput)"
else
    ok "E2c. not a module of the Write dispatcher (it emits updatedInput)"
fi

grep -q 'scripts/lib/dialect/dialect-en-US\.generated\.dict' "$ENGINE_ROOT/scripts/hooks/install.sh" \
    && grep -q 'scripts/lib/dialect/dialect-fix\.py' "$ENGINE_ROOT/scripts/hooks/install.sh" \
    && ok "E4. the table and the fixer are sidecar-hashed by install.sh" \
    || bad "E4. the table and the fixer are sidecar-hashed by install.sh"

# E5 — ONE vocabulary. The generated table is the data; the hand-kept
# dictionary is one of its inputs (dialect-table.test.py fails when the table
# drifts from a fresh regeneration); fixtures/ is the declared place for
# British test inputs. Everything else under scripts/ carries no vocabulary.
DUP="$(grep -rlE '^(colour|behaviour|licence)\b' "$ENGINE_ROOT/scripts" 2>/dev/null \
        | grep -v 'dialect-en-US.dict' | grep -v 'dialect-guard' \
        | grep -vxF "$ENGINE_ROOT/scripts/lib/dialect/dialect-en-US.generated.dict" \
        | grep -vF "$ENGINE_ROOT/scripts/lib/dialect/fixtures/" || true)"
[ -z "$DUP" ] && ok "E5. the vocabulary lives in exactly one place" \
              || bad "E5. the vocabulary lives in exactly one place" "also in: $DUP"

# E6/E7 — FILES THAT HOLD BRITISH TEST INPUTS WITHOUT BEING OWN FILES (plan
# catch C8). The fixer itself, and the probe whose Layer T canary is British on
# purpose: a Write of either must change nothing, or a later edit would rewrite
# the canary and Layer T would pass vacuously.
SELF="$(python3 - "$FIXER" "$TABLE" "$ENGINE_ROOT/scripts/hooks/contract-integrity-probe.sh" <<'SELF_PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("dialect_fix", sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
table = m.load_table(sys.argv[2])
for path in (sys.argv[1], sys.argv[3]):
    labels = m.findings_for_file(path, open(path, encoding="utf-8").read(), table)
    print("%s=%s" % (path.rsplit("/", 1)[1], ",".join(labels)))
SELF_PY
)"
printf '%s\n' "$SELF" | grep -qx 'dialect-fix.py=' && ok "E6. the fixer carries no word it would change" \
    || bad "E6. the fixer carries no word it would change" "$(printf '%s\n' "$SELF" | grep dialect-fix)"
printf '%s\n' "$SELF" | grep -qx 'contract-integrity-probe.sh=' && ok "E7. the probe's British canaries are declared exempt" \
    || bad "E7. the probe's British canaries are declared exempt" "$(printf '%s\n' "$SELF" | grep probe)"

# E8 — C22: WHICH CONFIGURATION GOVERNS a write into richos/app/** from a
# session seated at the richos checkout root. richos declares DIALECT_TARGET
# only in richos/engine/orchestration.config; the checkout root carries none.
# Measured 2026-10-24: the hook resolves the seat to richos/engine and governs
# the write en-US. Asserted so a change to root resolution cannot quietly
# leave the app's files ungoverned. (A session seated INSIDE richos/app stands
# down "not adopted" today; that is root resolution's question, recorded in
# the stream B handover, not this hook's.)
CHECKOUT="$(git -C "$ENGINE_ROOT" rev-parse --show-toplevel 2>/dev/null || true)"
if [ -n "${RICHOS_MUTATION_INNER:-}" ] || [ -z "$CHECKOUT" ] || [ ! -d "$CHECKOUT/richos/app" ] \
        || [ "$(cd "$CHECKOUT/richos/engine" 2>/dev/null && pwd -P)" != "$ENGINE_ROOT" ]; then
    echo "  (E8 skipped: not a RichOS checkout with richos/app beside this engine)"
else
    E8_PAYLOAD="$(python3 -c 'import json,sys; print(json.dumps({"tool_name":"Write","cwd":sys.argv[1],"tool_input":{"file_path":sys.argv[2],"content":"the colour of it"}}))' "$CHECKOUT" "$CHECKOUT/richos/app/dialect-c22-probe.md")"
    E8_OUT="$(cd "$CHECKOUT" && printf '%s' "$E8_PAYLOAD" | env -u RICHOS_ENTITY_ROOT -u RICHOS_ENGINE_ROOT "$HOOK" 2>/dev/null)"
    E8_RC=$?
    if [ "$E8_RC" -eq 0 ] && printf '%s' "$E8_OUT" | grep -qF 'the color of it'; then
        ok "E8. a write into richos/app/ from a session seated at the checkout root is governed en-US (C22)"
    else
        bad "E8. a write into richos/app/ from a session seated at the checkout root is governed en-US (C22)" "rc=$E8_RC stdout=${E8_OUT:0:160}"
    fi
fi

echo
echo "=== V. VENDORED MATERIAL — whose prose is this? ==="
#
# On 2026-08-30 this guard rewrote fourteen lines across TWO vendored,
# MIT-licensed skills (commit 06f4a8221a61), found by accident six weeks later.
# The answer comes from a recorded FACT — `.richos/vendored-material`, read
# through scripts/lib/vendored-material.sh. The three directions of that one-way
# door are pinned here: it must not EDIT vendored prose (V1, V8, V9); RE-VENDORING
# VERBATIM must SUCCEED (V10); QUOTING a foreign spelling to document the
# difference must be possible (V5, V12). And the exemption is NARROW: material
# recorded as `origin=richos` is STILL FIXED (V2).

TABX=$'\t'
mkvendrepo() { # <dir> <registry-line...>
    local d="$1"; shift
    mkdir -p "$d/.richos" "$d/engine/skills/vendored/references" \
             "$d/engine/skills/mine" "$d/engine/skills/unregistered" "$d/docs"
    printf 'DIALECT_TARGET="en-US"\n' >"$d/orchestration.config"
    printf '%s\n' "$@" >"$d/.richos/vendored-material"
}

VS="$SANDBOX/vendrepo"
mkvendrepo "$VS" \
    'REDISTRIBUTABLE_PATHS="engine/skills"' \
    "engine/skills/vendored${TABX}third-party${TABX}MIT${TABX}Someone${TABX}up/stream${TABX}abc123${TABX}2026-01-01 x${TABX}certain${TABX}verbatim${TABX}docs/x.md" \
    "engine/skills/mine${TABX}richos${TABX}AGPL-3.0-only${TABX}RichOS${TABX}-${TABX}-${TABX}2026-01-01 x${TABX}high${TABX}verbatim${TABX}docs/x.md"

BRITISH="Emphasising the point, and the colour of the behaviour."
AMERICAN="Emphasizing the point, and the color of the behavior."

case_quiet "V1. a file recorded as third-party is left alone"  Write "$VS/engine/skills/vendored/references/natural.md" "$BRITISH"
case_fixed "V2. a file recorded as origin=richos is STILL fixed" "$C" "$AMERICAN" Write "$VS/engine/skills/mine/SKILL.md" "$BRITISH"
case_fixed "V3. an UNRECORDED path under skills/ is still fixed" "$C" "$AMERICAN" Write "$VS/engine/skills/unregistered/SKILL.md" "$BRITISH"
case_fixed "V4. an ungoverned path is still fixed"             "$C" "$AMERICAN" Write "$VS/docs/note.md" "$BRITISH"
case_quiet "V5. a MODIFICATIONS notice INSIDE a vendored dir is exempt" Write "$VS/engine/skills/vendored/MODIFICATIONS.md" "$BRITISH"
case_exit  "V5b. a refusal inside a vendored dir is suppressed too" 0 Write "$VS/engine/skills/vendored/references/q.md" "the CEO queue, as upstream wrote it"

# THE SOFT RULE IS SUPPRESSED TOO: a note about somebody else's document at
# every write is the cries-wolf half of the same defect.
SOFTQ="The CEO looked at the queue this morning."
case_quiet "V13. a vendored file draws no CEO-TODOs note either" Write "$VS/engine/skills/vendored/references/soft.md" "$SOFTQ"
case_fixed "V14. ...and our own file still draws one (the note is not simply gone)" '"not blocked" in o["additionalContext"]' "True" Write "$VS/engine/skills/mine/soft.md" "$SOFTQ"

# FAIL CLOSED when the registry is declared and unreadable.
VB="$SANDBOX/vendrepo-broken"
mkvendrepo "$VB" \
    'REDISTRIBUTABLE_PATHS="engine/skills"' \
    "engine/skills/vendored${TABX}third-party${TABX}MIT${TABX}S${TABX}u${TABX}r${TABX}2026${TABX}certain${TABX}verbatim"
case_exit "V6. a DECLARED but unreadable registry is fail-closed" 2 Write "$VB/engine/skills/vendored/references/x.md" "The colour is wrong."
case_msg  "V7. ...and it says the registry is why"              "DECLARED BUT UNREADABLE" Write "$VB/engine/skills/vendored/references/x.md" "The colour is wrong."
# THE FAIL-CLOSED IS AN `exit`, NOT A "TREAT IT AS OURS": on a REPORTING verdict
# the difference is the whole behavior.
case_exit "V15. a broken registry also refuses a REPORTING verdict" \
    2 Write "$VB/engine/skills/vendored/references/x.md" "The CEO looked at the list this morning, and the queue was long."

# --- THE REAL FILES, IN THE REAL TREE -------------------------------------
if [ -n "${RICHOS_MUTATION_INNER:-}" ]; then
    echo "  (V8-V12 skipped inside a mutation sandbox — they are about the SHIPPED tree)"
else
    SHIPPED_SKILLS="$ENGINE_ROOT/skills"
    CHECKOUT_ROOT="$(git -C "$ENGINE_ROOT" rev-parse --show-toplevel 2>/dev/null || true)"
    REPO_TOP="$CHECKOUT_ROOT"
    if [ "$CHECKOUT_ROOT" = "$ENGINE_ROOT" ] || [ -z "$CHECKOUT_ROOT" ]; then
        # An adopter copies engine/ to its own root, without RichOS's parent
        # registry: exercise the shipped skill paths under an explicit isolated
        # registry instead of guessing that absent provenance exempts them.
        REPO_TOP="$SANDBOX/shipped-skill-registry"
        SHIPPED_SKILLS="$REPO_TOP/engine/skills"
        mkvendrepo "$REPO_TOP" \
            'REDISTRIBUTABLE_PATHS="engine/skills"' \
            "engine/skills/copywriting${TABX}third-party${TABX}MIT${TABX}2025 Corey Haines${TABX}coreyhaines31/marketingskills${TABX}68f5eaf64e85${TABX}2026-03-04${TABX}certain${TABX}test fixture${TABX}docs/fixture-provenance.md" \
            "engine/skills/landing-page-taste${TABX}third-party${TABX}MIT${TABX}2026 Leonxlnx${TABX}Leonxlnx/taste-skill${TABX}72e299530e2e${TABX}2026-08-23${TABX}certain${TABX}test fixture${TABX}docs/fixture-provenance.md" \
            "engine/skills/rich-lander${TABX}richos${TABX}AGPL-3.0-only${TABX}RichOS${TABX}-${TABX}-${TABX}2026${TABX}high${TABX}test fixture${TABX}docs/fixture-provenance.md"
        for skill_file in copywriting/references/natural-transitions.md landing-page-taste/SKILL.md rich-lander/SKILL.md; do
            mkdir -p "$SHIPPED_SKILLS/$(dirname "$skill_file")"
            cp "$ENGINE_ROOT/skills/$skill_file" "$SHIPPED_SKILLS/$skill_file" \
                || { echo "FATAL: missing shipped skill $skill_file" >&2; exit 1; }
        done
        mkdir -p "$REPO_TOP/docs/legal"
        echo "  (standalone engine: real hook, shipped skill files, isolated explicit provenance registry)"
    elif [ ! -f "$REPO_TOP/.richos/vendored-material" ]; then
        echo "FATAL: RichOS engine/ checkout requires its real parent vendored-material registry" >&2
        exit 1
    fi
    case_quiet "V8. THE DAMAGED FILE accepts upstream's own spelling, unchanged" \
        Write "$SHIPPED_SKILLS/copywriting/references/natural-transitions.md" "$BRITISH"
    case_quiet "V9. the SECOND damaged file (landing-page-taste) too" \
        Write "$SHIPPED_SKILLS/landing-page-taste/SKILL.md" "$BRITISH"

    # RE-VENDORING VERBATIM: the WHOLE upstream file, the bytes the 2026-08-30
    # sweep replaced, pinned with its license in the shipped test fixtures.
    UP="$SCRIPT_DIR/fixtures/third_party/copywriting/natural-transitions.md"
    if [ -s "$UP" ] && python3 - "$UP" <<'UPSTREAM_HASH'
import hashlib, sys
raise SystemExit(0 if hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest() == "4ff23f8943af2f65b072f26f1c53ce55f19cc26d7be211c11cae8e34b43e859f" else 1)
UPSTREAM_HASH
    then
        UPJSON="$(python3 -c '
import json, sys
print(json.dumps({"tool_name": "Write", "tool_input": {
    "file_path": sys.argv[1],
    "content": open(sys.argv[2], encoding="utf-8").read()}}))' \
            "$SHIPPED_SKILLS/copywriting/references/natural-transitions.md" "$UP")"
        V10_OUT="$(printf '%s' "$UPJSON" | "$HOOK" 2>/dev/null)"
        V10_RC=$?
        [ "$V10_RC" -eq 0 ] && [ -z "$V10_OUT" ] && ok "V10. RE-VENDORING the file VERBATIM from upstream succeeds, unchanged" \
                     || bad "V10. RE-VENDORING the file VERBATIM from upstream succeeds, unchanged" \
                            "exit $V10_RC, stdout: ${V10_OUT:0:120}"
        # The same bytes must actually engage the hook on our own material,
        # rather than pass because the fixture no longer exercises it.
        FIRSTPARTY_JSON="$(python3 - "$SHIPPED_SKILLS/rich-lander/SKILL.md" "$UP" <<'FIRSTPARTY_PAYLOAD'
import json, sys
print(json.dumps({"tool_name": "Write", "tool_input": {
    "file_path": sys.argv[1], "content": open(sys.argv[2], encoding="utf-8").read()}}))
FIRSTPARTY_PAYLOAD
)"
        V10B_OUT="$(printf '%s' "$FIRSTPARTY_JSON" | "$HOOK" 2>/dev/null)"
        V10B_RC=$?
        if [ "$V10B_RC" -eq 2 ] || printf '%s' "$V10B_OUT" | grep -q '"updatedInput"'; then
            ok "V10b. the SAME upstream bytes are fixed or refused for first-party material, never passed"
        else
            bad "V10b. the SAME upstream bytes are fixed or refused for first-party material" \
                "exit $V10B_RC with no envelope: the fixture passed without exercising the ownership exemption"
        fi
    else
        bad "V10. RE-VENDORING the file VERBATIM from upstream succeeds" \
            "the pinned upstream fixture is missing or has changed — this check cannot be skipped"
    fi

    case_fixed "V11. a RichOS-authored skill in the real tree is still fixed" "$C" "$AMERICAN" \
        Write "$SHIPPED_SKILLS/rich-lander/SKILL.md" "$BRITISH"

    # DOCUMENTING THE DIVERGENCE, in the tree's own legal record.
    case_quiet "V12. quoting a foreign spelling in the notices document is possible" \
        Write "$REPO_TOP/docs/legal/THIRD-PARTY-NOTICES.md" "Upstream writes Emphasising; our copy writes Emphasizing."
fi

echo
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -x "$SCRIPT_DIR/dialect-guard.mutation.sh" ]; then
    # THE HARNESS RUNS FROM THE SUITE IT MUTATES: run-all-tests.sh discovers
    # *.test.sh, and a harness nobody runs proves nothing about anything.
    echo "=== running the mutation harness ==="
    "$SCRIPT_DIR/dialect-guard.mutation.sh" || FAIL=$((FAIL + 1))
    echo
fi

if [ "$FAIL" -eq 0 ]; then
    printf '\n  %d/%d cases passed\n' "$PASS" "$((PASS + FAIL))"
    exit 0
fi
printf '\n  %d/%d cases passed, %d FAILED\n' "$PASS" "$((PASS + FAIL))" "$FAIL"
exit 1
