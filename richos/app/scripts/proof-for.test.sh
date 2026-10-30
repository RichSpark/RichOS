#!/usr/bin/env bash
#
# proof-for.test.sh — what `proof-for.sh` answers, checked against THREE REAL LANDS.
#
# =========================================================================================
# WHY THE FIXTURES ARE REAL COMMITS AND NOT SYNTHETIC DIFFS
# =========================================================================================
# A synthetic diff proves the rules fire. It cannot prove the rules are RIGHT, because the
# author of the fixture and the author of the rule are the same person and they agree by
# construction. So the three fixtures here are lands that actually happened in this
# repository, each chosen because its commit message says in plain words what it changed —
# which is an independent statement of what the proof should be:
#
#   315baf03  phonelive3  the phone's send, its outbox, the pairing screen, the web app
#   e7facc99  harness1    one UI check, one core module's ordering, five testvm scripts
#   62e5affd  shots1      104 regenerated reference shots and the stability library
#
# Each case therefore asserts what the LAND says its own subject was, not what the script
# happens to print today.
#
# THE THIRD ONE IS THE HONEST CASE AND IT IS KEPT DELIBERATELY. shots1 maps to all 55 UI
# suites — no reduction at all — because it touched `lib/shot-stability.js`, which every
# suite reaches through `harness.js`. A targeting tool that quietly pretended otherwise
# would be worse than none, so the assertion is that it says 55 AND names the one file that
# caused it.
#
# run-tests: inputs richos/app/scripts/lib/ios_ui_scope.py richos/app/scripts/proof-for.test.sh richos/app/scripts/proof-for.sh richos/app/scripts/battery-check.py richos/app/scripts/proof-for.ui-inputs richos/app/ui/tests/docs-claims.js richos/app/scripts/lib/proof_declarations.py richos/app/scripts/proof-for-declarations.test.py richos/app/scripts/proof-for-engine.test.py richos/engine docs/development/verification-input-qualifications.json richos/mobile richos/app/scripts/mobile-headless.test.sh richos/app/scripts/mobile-pwa.test.sh richos/app/scripts/mobile-ios.test.sh
# run-tests: covers richos/app/scripts/proof-for.sh richos/app/scripts/lib/proof_declarations.py richos/app/scripts/proof-for-declarations.test.py richos/app/scripts/proof-for-engine.test.py
set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="$(cd "$DIR/.." && pwd)"
ROOT="$(git -C "$APP" rev-parse --show-toplevel 2>/dev/null || printf '%s' "$APP")"
PF="$DIR/proof-for.sh"

PASS=0; FAIL=0; NOTRUN=0
ok()  { PASS=$((PASS + 1)); echo "  PASS  $1"; }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL  $1"; [ -n "${2:-}" ] && echo "        $2"; }
# NOT RUN is neither, and it is NAMED — the one thing this repository refuses to let a
# harness do is report green over a case that never executed.
notrun() { NOTRUN=$((NOTRUN + 1)); echo "  NOT RUN  $1 — $2"; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/proof-for-test.XXXXXX")"
# §54: the scratch goes however this ends, including on a signal.
trap 'rm -rf "$WORK"' EXIT HUP INT TERM

# The output goes to a FILE and the exit code is read from the call itself. A command
# substitution runs in a subshell, so `RC=$?` set inside one never reaches here — which is
# how the first draft of this suite read an unbound variable instead of an exit code.
RC=0
run_pf() {  # $@ -> proof-for.sh; leaves the output in $2's file, sets RC
  local dest="$1"; shift
  "$PF" "$@" > "$dest" 2>&1; RC=$?
}

have_commit() { git -C "$ROOT" rev-parse --verify --quiet "$1^{commit}" >/dev/null 2>&1; }

if PYTHONDONTWRITEBYTECODE=1 python3 "$DIR/proof-for-declarations.test.py"; then
  ok "J1 script dependencies and behavioral coverage reconcile independently"
else
  bad "J1 script declaration contract"
fi

if PYTHONDONTWRITEBYTECODE=1 python3 "$DIR/proof-for-engine.test.py"; then
  ok "J2 generated engine commands execute and preserve scoped verdicts"
else
  bad "J2 generated engine command contract"
fi

echo "=== proof-for: the three real lands ==="

# -----------------------------------------------------------------------------------------
# A. 315baf03 — the phone path. Its message names the send/outbox (Rust + web app), the
#    pairing screen (ui/phone.js) and the service worker's eviction (web app).
# -----------------------------------------------------------------------------------------
if have_commit 315baf03; then
  run_pf "$WORK/a.out" 315baf03; A_RC=$RC
  [ "$A_RC" -eq 0 ] && ok "A0 phonelive3 maps cleanly (exit 0 — nothing uncovered)" \
                    || bad "A0 phonelive3 maps cleanly" "exit $A_RC; see $WORK/a.out"

  if grep -q 'cargo test --bin richos-tauri phone::device::' "$WORK/a.out" \
  && grep -q 'cargo test --bin richos-tauri phone::secrets::' "$WORK/a.out"; then
    ok "A1 the shell's phone modules name their own unit tests"
  else
    bad "A1 the shell's phone modules name their own unit tests" "$(grep -c 'cargo test' "$WORK/a.out" || true) cargo lines"
  fi

  if grep -q 'node phone.js' "$WORK/a.out" && grep -q 'node no-home-network.js' "$WORK/a.out"; then
    ok "A2 the pairing screen's own two suites are named"
  else
    bad "A2 the pairing screen's own two suites are named"
  fi

  if grep -q 'test/queue.test.js' "$WORK/a.out" && grep -q 'test/api.test.js' "$WORK/a.out"; then
    ok "A3 the outbox's web-app suites are named"
  else
    bad "A3 the outbox's web-app suites are named"
  fi

  # The land's own escalation record is prose. It must be VISIBLE, not dropped.
  if grep -q 'NOT PROVEN BY A SUITE' "$WORK/a.out" \
  && grep -q '2026-09-19-echo-opus-phonelive3' "$WORK/a.out"; then
    ok "A4 the prose in the land is printed by name rather than silently dropped"
  else
    bad "A4 the prose in the land is printed by name rather than silently dropped"
  fi

  # 27 of 55 is the reduction that matters: it is less than the whole inventory.
  N="$(sed -n 's/^UI SUITES — \([0-9]*\) of \([0-9]*\)$/\1 \2/p' "$WORK/a.out")"
  set -- $N
  if [ "${1:-0}" -gt 0 ] && [ "${1:-0}" -lt "${2:-0}" ]; then
    ok "A5 the UI answer is a SUBSET (${1:-?} of ${2:-?}), not the whole inventory"
  else
    bad "A5 the UI answer is a SUBSET, not the whole inventory" "got '$N'"
  fi
else
  bad "A  315baf03 is not in this clone — the phone-path fixture could not run"
fi

# -----------------------------------------------------------------------------------------
# A2. A teammate definition is READ by a richos-core test (2026-10-21: Newt's shelf file landed
#     naming no media skill, the selector mapped it to nothing, and the test failed on main).
# -----------------------------------------------------------------------------------------
A2_TEST="native::native_driver_tests::"
A2_TEST="${A2_TEST}a_teammate_s_session_lists_every_skill_its_definition_names"
for d in agents team/shelf mega-lander/duties; do
  run_pf "$WORK/a2.out" --paths "richos/engine/$d/zz-fixture.md"
  if grep -qF "richos-core --lib $A2_TEST" "$WORK/a2.out"; then
    ok "A2 a file under engine/$d selects the test that reads every teammate definition"
  else
    bad "A2 a file under engine/$d selects the definition-reading test" "$(cat "$WORK/a2.out")"
  fi
done

# -----------------------------------------------------------------------------------------
# B. e7facc99 — the land whose three subjects are named one by one in its own message.
#    This is the case that found the nested-runner blind spot.
# -----------------------------------------------------------------------------------------
if have_commit e7facc99; then
  run_pf "$WORK/b.out" e7facc99; B_RC=$RC
  [ "$B_RC" -eq 0 ] && ok "B0 harness1 maps cleanly (exit 0)" \
                    || bad "B0 harness1 maps cleanly" "exit $B_RC; see $WORK/b.out"

  grep -q 'node no-home-network.js' "$WORK/b.out" \
    && ok "B1 the one UI check it changed is named" \
    || bad "B1 the one UI check it changed is named"

  grep -q 'cargo test -p richos-core --lib work_host::' "$WORK/b.out" \
    && ok "B2 work_host's own unit tests are named — and no integration target is, because none reads it" \
    || bad "B2 work_host's own unit tests are named"

  grep -q 'bash scripts/testvm/test/run-tests.sh' "$WORK/b.out" \
    && ok "B3 the NESTED testvm runner is named, which run-tests.sh's maxdepth 1 cannot reach" \
    || bad "B3 the NESTED testvm runner is named"

  # ONE browser suite (the land's subject, B1) plus every suite whose row in
  # proof-for.ui-inputs DECLARES that it reads a path the land changed — the Rust file
  # work_host.rs (affordances.js, dialect.js, realbytes.js, ...) or the tests folder
  # (docs-claims.js, gate-honesty.js) — and nothing else. Not the 19-minute run.
  #
  # The readers are DERIVED from the declarations, not typed here. This list was typed until
  # 2026-10-20 and every suite newly declared as a reader had to be added by hand: richscribe.js
  # on 2026-10-19 (1efbab003) and gate-honesty.js on 2026-10-20 (27a6a46ff), each after B4 went
  # red on a correct selection (verification reliability PRD T04). A declaration is the
  # requirement and proof-for.sh is the code that honors it, so this still fails when
  # proof-for.sh drops a declared reader or adds a suite
  # nobody declared. It does not judge whether a row is RIGHT: R48 and docs-claims.js's last
  # check do that. Premise: the land changed no shipped UI file, so `@allui` (computed by
  # proof-for.sh, not declared) cannot select anything here; if that ever stops being true
  # this case says so rather than guessing.
  UI_B="$(sed -n 's/^  cd richos\/app\/ui\/tests && node \(.*\.js\)$/\1/p' "$WORK/b.out" | LC_ALL=C sort | tr '\n' ' ')"
  B_PATHS="$(git -C "$ROOT" diff --name-only e7facc99^1 e7facc99)"
  B_EXPECT="$(B_PATHS="$B_PATHS" python3 - "$DIR/proof-for.ui-inputs" <<'PY'
import os, sys
paths = os.environ["B_PATHS"].split()
shipped_ui = [p for p in paths if p.startswith("richos/app/ui/") and not p.startswith("richos/app/ui/tests/")]
if shipped_ui:
    print("PREMISE: the land changed shipped UI " + " ".join(shipped_ui))
    sys.exit(0)
aliases, rows = {}, {}
for line in open(sys.argv[1]):
    words = line.split("#", 1)[0].split()
    if not words:
        continue
    if words[0].startswith("@"):
        aliases[words[0][1:]] = words[1:]
    else:
        rows[words[0]] = words[1:]
def expand(tokens, seen=()):
    for t in tokens:
        if t == "@allui":
            continue
        if t.startswith("@"):
            if t not in seen:
                yield from expand(aliases.get(t[1:], []), seen + (t,))
        else:
            yield t
expect = {p.rsplit("/", 1)[1] for p in paths if p.startswith("richos/app/ui/tests/") and p.count("/") == 4 and p.endswith(".js")}
for suite, tokens in rows.items():
    for t in expand(tokens):
        if any(p == t or (t.endswith("/") and p.startswith(t)) for p in paths):
            expect.add(suite)
            break
print(" ".join(sorted(expect)) + " ")
PY
)"
  case "$B_EXPECT" in
    PREMISE:*) bad "B4 a one-check UI change costs ONE suite plus its declared readers" "$B_EXPECT" ;;
    *) [ "$UI_B" = "$B_EXPECT" ] \
         && ok "B4 a one-check UI change costs ONE suite plus the suites declared to read what it changed, not the 19-minute run ($(printf '%s' "$UI_B" | wc -w | tr -d ' ') suites)" \
         || bad "B4 a one-check UI change costs ONE suite plus its declared readers" "selected: ${UI_B:-none}; declared: $B_EXPECT" ;;
  esac
else
  bad "B  e7facc99 is not in this clone — the testvm fixture could not run"
fi

# -----------------------------------------------------------------------------------------
# C. 62e5affd — the honest case. 110 paths, and the right answer is "everything", with the
#    reason said out loud.
# -----------------------------------------------------------------------------------------
if have_commit 62e5affd; then
  run_pf "$WORK/c.out" 62e5affd; C_RC=$RC
  [ "$C_RC" -eq 0 ] && ok "C0 shots1 maps cleanly (exit 0)" \
                    || bad "C0 shots1 maps cleanly" "exit $C_RC; see $WORK/c.out"

  N="$(sed -n 's/^UI SUITES — \([0-9]*\) of \([0-9]*\)$/\1 \2/p' "$WORK/c.out")"
  set -- $N
  if [ -n "${1:-}" ] && [ "${1:-0}" -eq "${2:-0}" ]; then
    ok "C1 a shared-library change maps to every suite, and says so (${1} of ${2})"
  else
    bad "C1 a shared-library change maps to every suite" "got '$N'"
  fi

  grep -q 'widest cause:.*shot-stability.js' "$WORK/c.out" \
    && ok "C2 ...and NAMES the one file that made it every suite" \
    || bad "C2 ...and NAMES the one file that made it every suite" "$(grep 'widest cause' "$WORK/c.out" || echo 'no cause line')"

  # Every regenerated shot is claimed by the suite that owns its directory. If the shots
  # mapped to nothing, 104 images would be sitting in UNCOVERED or in the prose list.
  grep -q 'node memory-strategy.js' "$WORK/c.out" \
    && ok "C3 a regenerated shot reaches the suite that owns its directory" \
    || bad "C3 a regenerated shot reaches the suite that owns its directory"
else
  bad "C  62e5affd is not in this clone — the shots fixture could not run"
fi

echo
echo "=== the refusals, which are the part that must not be decorative ==="

# -----------------------------------------------------------------------------------------
# D. A changed CODE path nobody covers is a FAILURE, by name. This is the whole point: a
#    targeting tool that exits 0 over an empty set has certified nothing.
# -----------------------------------------------------------------------------------------
run_pf "$WORK/d.out" --paths richos/app/.proof-for-probe/orphan.sh; D_RC=$RC
if [ "$D_RC" -eq 1 ] && grep -q 'UNCOVERED' "$WORK/d.out" && grep -q 'orphan.sh' "$WORK/d.out"; then
  ok "D1 a code path no suite claims exits 1 and is named"
else
  bad "D1 a code path no suite claims exits 1 and is named" "exit $D_RC"
fi

# The POSITIVE PROBE for D1: the same shape of path, but prose, must NOT fail. Without this
# the case above would pass just as well if the script failed on everything.
run_pf "$WORK/dp.out" --paths docs/a-file-that-is-only-prose.md; DP_RC=$RC
if [ "$DP_RC" -eq 0 ] && grep -q 'NOT PROVEN BY A SUITE' "$WORK/dp.out"; then
  ok "D2 (positive probe) prose in the same position exits 0 and is listed, not failed"
else
  bad "D2 (positive probe) prose exits 0 and is listed" "exit $DP_RC"
fi

# D3/D4. A UI FIXTURE is owned only by a row that names it (proof-for.ui-inputs, "FIXTURES").
#    The automatic land check refuses UNCOVERED, so a new fixture nobody loads must still be
#    refused by name; one a suite loads maps to that suite. Found 2026-09-29: three of the
#    last four lands carried fixtures no row named (esc-20260929T082408Z-c96cefea).
run_pf "$WORK/d3.out" --paths richos/app/ui/tests/fixtures/a-new-fixture-no-suite-loads.json; D3_RC=$RC
if [ "$D3_RC" -eq 1 ] && grep -q 'UNCOVERED' "$WORK/d3.out" && grep -q 'a-new-fixture-no-suite-loads.json' "$WORK/d3.out"; then
  ok "D3 a new UI fixture with no owner exits 1 and is named"
else
  bad "D3 a new UI fixture with no owner exits 1 and is named" "exit $D3_RC"
fi
run_pf "$WORK/d4.out" --paths richos/app/ui/tests/fixtures/loro-proposal-asked-again.json; D4_RC=$RC
if [ "$D4_RC" -eq 0 ] && grep -q 'node corrections.js' "$WORK/d4.out" && ! grep -q 'UNCOVERED' "$WORK/d4.out"; then
  ok "D4 (positive probe) a fixture a suite loads maps to that suite"
else
  bad "D4 (positive probe) a fixture a suite loads maps to that suite" "exit $D4_RC"
fi

# -----------------------------------------------------------------------------------------
# E. The reconciliation. Each of the four ways the declaration can go stale must REFUSE,
#    with exit 2, naming what is wrong. A map that answers from a stale row is worse than
#    no map, because it answers.
# -----------------------------------------------------------------------------------------
DECL="$DIR/proof-for.ui-inputs"
mutate() { cp "$DECL" "$WORK/decl"; printf '%s\n' "$1" >> "$WORK/decl"; \
           PROOF_FOR_UI_INPUTS="$WORK/decl" "$PF" --paths richos/app/ui/main.js 2>&1; }

OUT="$(mutate 'a-suite-that-does-not-exist.js richos/app/ui/main.js')"
printf '%s' "$OUT" | grep -q 'not a suite in ui/tests' \
  && ok "E1 a row naming a suite that is not there refuses" \
  || bad "E1 a row naming a suite that is not there refuses" "$OUT"

OUT="$(mutate 'home.js richos/app/ui/a-file-that-is-not-there.js')"
printf '%s' "$OUT" | grep -q 'not in the tree' \
  && ok "E2 a row naming a path that is not there refuses" \
  || bad "E2 a row naming a path that is not there refuses" "$OUT"

OUT="$(mutate 'home.js @nosuchalias')"
printf '%s' "$OUT" | grep -q 'which no line defines' \
  && ok "E3 a row using an alias nobody defines refuses" \
  || bad "E3 a row using an alias nobody defines refuses" "$OUT"

# A suite with NO row is the stale case that matters most: a suite lands, nobody declares
# it, and from that moment the map silently under-answers for every file that suite reads.
grep -v '^home.js ' "$DECL" > "$WORK/decl-missing"
OUT="$(PROOF_FOR_UI_INPUTS="$WORK/decl-missing" "$PF" --paths richos/app/ui/main.js 2>&1)"; E_RC=$?
if [ "$E_RC" -eq 2 ] && printf '%s' "$OUT" | grep -q "home.js' is a suite with NO row"; then
  ok "E4 a suite that landed with no row refuses — the stale case that under-answers"
else
  bad "E4 a suite that landed with no row refuses" "exit $E_RC"
fi

# A shared integration module is compiled through its consumers, not cargo --test mod.
run_pf "$WORK/helper.out" --paths richos/app/crates/richos-core/tests/support/mod.rs
if [ "$RC" -eq 0 ] && grep -q -- '--test action_ledger_tests' "$WORK/helper.out" \
   && grep -q -- '--test questions' "$WORK/helper.out" \
   && ! grep -qE -- '--test (mod|support)( |$)' "$WORK/helper.out"; then
  ok "K0 shared Rust integration helpers select surviving consumer targets"
else
  bad "K0 a helper was selected as a nonexistent standalone Cargo target" "$(tr '\n' ' ' < "$WORK/helper.out")"
fi

# The positive probe for E: unmutated, the same invocation must succeed. Four refusals that
# fire on everything would pass E1-E4 and mean nothing.
OUT="$("$PF" --paths richos/app/ui/main.js 2>&1)"; E0_RC=$?
[ "$E0_RC" -eq 0 ] \
  && ok "E5 (positive probe) the real declaration reconciles and answers" \
  || bad "E5 (positive probe) the real declaration reconciles" "exit $E0_RC: $OUT"

# -----------------------------------------------------------------------------------------
# K. A WHOLE-TREE CLAIM IS SELECTED BY ANY CHANGE THAT CAN FALSIFY IT (2026-09-29). Nightly
#    20260929T101720Z-bee22332 failed docs-claims.js: 6f5cfb03 added tests to
#    correction_forward_compat_tests.rs, which made app/README.md's counts false, and the land
#    ran lint, richos-core and corrections.js, never docs-claims.js, whose row named eight
#    exact files. A directory input (`<dir>/`) selects for everything under it and covers
#    nothing; docs-claims.js checks its own row against what it actually read.
# -----------------------------------------------------------------------------------------
run_pf "$WORK/k1.out" --paths richos/app/crates/richos-core/tests/correction_forward_compat_tests.rs; K1_RC=$RC
if [ "$K1_RC" -eq 0 ] && grep -q 'node docs-claims.js' "$WORK/k1.out"; then
  ok "K1 a change to a Rust test file selects docs-claims.js, whose counts it can falsify"
else
  bad "K1 a change to a Rust test file selects docs-claims.js" "exit $K1_RC: $(tr '\n' ' ' < "$WORK/k1.out")"
fi

run_pf "$WORK/k2.out" --paths richos/app/crates/richos-core/src/a_module_no_test_names.rs; K2_RC=$RC
if [ "$K2_RC" -eq 1 ] && grep -q 'node docs-claims.js' "$WORK/k2.out" \
   && grep -q 'UNCOVERED' "$WORK/k2.out" && grep -q 'a_module_no_test_names.rs' "$WORK/k2.out"; then
  ok "K2 a directory input selects its suite and covers nothing: an untested module is still UNCOVERED"
else
  bad "K2 a directory input selects and covers nothing" "exit $K2_RC: $(tr '\n' ' ' < "$WORK/k2.out")"
fi

OUT="$(mutate 'home.js richos/app/a-directory-that-is-not-there/')"
if printf '%s' "$OUT" | grep -q "names the directory 'richos/app/a-directory-that-is-not-there/', which is not in the tree"; then
  ok "K3 a row naming a directory that is not there refuses"
else
  bad "K3 a row naming a directory that is not there refuses" "$OUT"
fi

# The row as it was on 5ddcce1c: the same row with its directory inputs taken out, derived
# from the real one so this case never keeps a stale copy of it.
awk '$1 == "docs-claims.js" { o = $1; for (i = 2; i <= NF; i++) if ($i !~ /\/$/) o = o " " $i; print o; next } { print }' \
  "$DECL" > "$WORK/decl-narrow"
if command -v node >/dev/null 2>&1; then
  OUT="$(cd "$APP/ui/tests" && PROOF_FOR_UI_INPUTS="$WORK/decl-narrow" node docs-claims.js 2>&1)"
  REAL="$(cd "$APP/ui/tests" && node docs-claims.js 2>&1)"
  # The refusal must name a read the narrowed row no longer reaches. Until 2026-10-20 that was
  # correction_forward_compat_tests.rs, read for its #[test] count; the counts are gone
  # (6e3c33002), so the witness is the crates directory every test file is listed from.
  if printf '%s' "$OUT" | grep -q 'FAIL  proof-for.ui-inputs selects this suite' \
     && printf '%s' "$OUT" | grep -q '"richos/app/crates/"' \
     && printf '%s' "$REAL" | grep -q 'PASS  proof-for.ui-inputs selects this suite'; then
    ok "K4 docs-claims.js refuses a row that does not reach what it reads, and accepts the real one"
  else
    bad "K4 docs-claims.js checks its own row against its reads" "$(printf '%s' "$OUT" | grep -A3 'proof-for.ui-inputs selects' | tr '\n' ' ')"
  fi
  # K5 (2026-10-20). docs-claims.js ends by letting Node exit, never through process.exit().
  # Node 26.10's process.exit() can deadlock at shutdown (nodejs/node#54918's class: a background
  # Maglev compile waits for a GC the exiting main thread never runs). Measured on this suite,
  # 0.3 s of work: 3 of 120 runs hung for good, 0 of 400 that exited naturally. K4 runs it twice,
  # so this suite timed out at a land (603 s against its 600 s budget) with nothing wrong in the tree.
  printf '%s\n' 'const real = process.exit; process.exit = (c) => { process.stderr.write("K5: process.exit called\n"); real(c); };' \
    > "$WORK/k5-exit.js"
  K5_OUT="$(cd "$APP/ui/tests" && node --require "$WORK/k5-exit.js" docs-claims.js 2>&1)"; K5_RC=$?
  if [ "$K5_RC" -eq 0 ] && printf '%s' "$K5_OUT" | grep -q 'PASS  proof-for.ui-inputs selects this suite' \
     && ! printf '%s' "$K5_OUT" | grep -q 'K5: process.exit called'; then
    ok "K5 docs-claims.js passes and exits naturally, never through Node's process.exit() shutdown"
  else
    bad "K5 docs-claims.js must end by setting process.exitCode, not by calling process.exit()" \
        "exit $K5_RC: $(printf '%s' "$K5_OUT" | grep -E 'K5: |FAIL' | head -3 | tr '\n' ' ')"
  fi
else
  notrun "K4 docs-claims.js checks its own row against its reads" "node is not on PATH"
fi

# K6 (2026-10-20). No ui test script reports its result through process.exit(): set
# process.exitCode and let Node exit (the reason is the comment at the end of docs-claims.js).
# Only a literal exit (process.exit(1), process.exit(2)) stays allowed: those are error and usage
# paths where a browser may still be open and exiting at once is the point. run.js is a runner
# that exits with its children's codes, and tests/fixtures holds child scripts that the gate
# suites run on purpose to see a hang or an exit code, so neither is scanned.
K6_HITS="$(cd "$APP/ui/tests" && grep -rnE 'process\.exit\(' --include='*.js' . \
  | grep -v -E -e '^(\./)?run\.js:' -e '^(\./)?fixtures/' -e '^(\./)?node_modules/' \
  | grep -v -E -e 'process\.exit\([0-9]\)' -e '^[^:]*:[0-9]+:[[:space:]]*//' || true)"
if [ -z "$K6_HITS" ]; then
  ok "K6 no ui test script ends with process.exit(expression); results go through process.exitCode"
else
  bad "K6 ui test scripts must set process.exitCode, not call process.exit(expression) (see the comment at the end of docs-claims.js)" \
      "$(printf '%s' "$K6_HITS" | head -3 | tr '\n' ' ')"
fi

# -----------------------------------------------------------------------------------------
# L. A PATH THE CHANGE DELETED IS NEVER UNCOVERED (hunt 2026-09-29, part 2, section 03).
#    Deleting a script no suite covers refused the commit and the land as UNCOVERED, though
#    no code was left to prove, and deleting a suite selected a suite that is gone. Two
#    commits are built from HEAD's tree in a scratch index: ADD holds an uncovered script and
#    a suite nobody declares and a Rust target, DEL is ADD with all three deleted. Neither the working tree, the real
#    index nor any ref is touched; the objects are unreferenced and git collects them.
# -----------------------------------------------------------------------------------------
L_PROBE=richos/app/.proof-for-probe/deleted-orphan.sh
L_SUITE=richos/app/scripts/a-deleted-probe.test.sh
L_RUST=richos/app/crates/richos-core/tests/a_deleted_probe.rs
lgit() { GIT_INDEX_FILE="$WORK/l-index" git -C "$ROOT" -c user.name='proof-for fixture' \
           -c user.email=proof-for@example.invalid "$@"; }
L_ADD=""; L_DEL=""
if L_BLOB="$(printf '#!/bin/sh\nexit 0\n' | git -C "$ROOT" hash-object -w --stdin)" \
   && lgit read-tree HEAD \
   && lgit update-index --add --cacheinfo "100755,$L_BLOB,$L_PROBE" \
   && lgit update-index --add --cacheinfo "100755,$L_BLOB,$L_SUITE" \
   && lgit update-index --add --cacheinfo "100644,$L_BLOB,$L_RUST" \
   && L_ADD="$(lgit commit-tree "$(lgit write-tree)" -p HEAD -m 'proof-for fixture: add')" \
   && lgit update-index --force-remove "$L_PROBE" "$L_SUITE" "$L_RUST" \
   && L_DEL="$(lgit commit-tree "$(lgit write-tree)" -p "$L_ADD" -m 'proof-for fixture: delete')"; then
  run_pf "$WORK/l1.out" "$L_DEL"; L1_RC=$RC
  if [ "$L1_RC" -eq 0 ] && grep -q '^REMOVED — 3' "$WORK/l1.out" && grep -q 'deleted-orphan.sh' "$WORK/l1.out" \
     && ! grep -q 'UNCOVERED' "$WORK/l1.out" && ! grep -q 'a-deleted-probe.test.sh' <(grep -- '--only' "$WORK/l1.out") \
     && ! grep -q -- '--test a_deleted_probe' "$WORK/l1.out"; then
    ok "L1 a commit that deletes an uncovered script and a suite maps cleanly, listed under REMOVED"
  else
    bad "L1 a commit that deletes an uncovered script and a suite maps cleanly" "exit $L1_RC: $(tr '\n' ' ' < "$WORK/l1.out")"
  fi
  run_pf "$WORK/l5.out" "$L_ADD"
  if grep -q -- '--test a_deleted_probe' "$WORK/l5.out"; then
    ok "L5 a Rust integration target that exists in the selected revision is still selected"
  else
    bad "L5 an existing Rust integration target disappeared from selection"
  fi
  # The land's form: a range whose head lacks the paths.
  run_pf "$WORK/l2.out" "$L_ADD..$L_DEL"; L2_RC=$RC
  if [ "$L2_RC" -eq 0 ] && grep -q '^REMOVED — 3' "$WORK/l2.out" && ! grep -q 'UNCOVERED' "$WORK/l2.out"; then
    ok "L2 the same deletion as a land's range maps cleanly"
  else
    bad "L2 the same deletion as a land's range maps cleanly" "exit $L2_RC: $(tr '\n' ' ' < "$WORK/l2.out")"
  fi
  # The commit check's form (--paths): absent from disk and index, present at a base commit.
  # D1 above is the other half: a path that never existed is not a deletion and stays UNCOVERED.
  PROOF_FOR_DELETION_BASE="$L_ADD" "$PF" --paths "$L_PROBE" > "$WORK/l3.out" 2>&1; L3_RC=$?
  if [ "$L3_RC" -eq 0 ] && grep -q '^REMOVED — 1' "$WORK/l3.out" && ! grep -q 'UNCOVERED' "$WORK/l3.out"; then
    ok "L3 the commit check's path list accepts a path deleted since its base"
  else
    bad "L3 the commit check's path list accepts a path deleted since its base" "exit $L3_RC: $(tr '\n' ' ' < "$WORK/l3.out")"
  fi
  # The positive control: the same script while it EXISTS is still refused, by name.
  run_pf "$WORK/l4.out" "$L_ADD"; L4_RC=$RC
  if [ "$L4_RC" -eq 1 ] && grep -q 'UNCOVERED' "$WORK/l4.out" && grep -q 'deleted-orphan.sh' "$WORK/l4.out"; then
    ok "L4 (positive control) the same script, present and uncovered, still exits 1 by name"
  else
    bad "L4 (positive control) the same script, present, is still UNCOVERED" "exit $L4_RC: $(tr '\n' ' ' < "$WORK/l4.out")"
  fi
else
  bad "L  the deletion fixture commits could not be built from HEAD"
fi

echo
echo "=== the declarations this script reads ==="

# -----------------------------------------------------------------------------------------
# F. Every script suite declares its inputs, and every declared path is in the tree. This
#    is the inventory check that keeps the script-suite half from drifting: a new suite
#    with no declaration is a suite `proof-for.sh` can never name.
# -----------------------------------------------------------------------------------------
N_SUITES=0; N_DECL=0; MISSING=""
for f in "$DIR"/*.test.sh; do
  [ -f "$f" ] || continue
  N_SUITES=$((N_SUITES + 1))
  d="$(sed -n 's/^# run-tests: inputs[[:space:]][[:space:]]*//p' "$f" | head -1)"
  if [ -z "$d" ]; then MISSING="$MISSING $(basename "$f")"; continue; fi
  N_DECL=$((N_DECL + 1))
  for p in $d; do
    [ -e "$ROOT/$p" ] || bad "F  $(basename "$f") declares '$p', which is not in the tree"
  done
done
if [ -z "$MISSING" ]; then
  ok "F1 all $N_SUITES script suites declare their inputs, and every declared path exists"
else
  bad "F1 every script suite declares its inputs" "no declaration:$MISSING"
fi

# -----------------------------------------------------------------------------------------
# G. It runs on the bash every Mac ships. `declare -A` is a syntax error in 3.2, and a
#    targeting script that only works on the author's shell targets nothing.
# -----------------------------------------------------------------------------------------
if /bin/bash -n "$PF" 2>/dev/null; then
  V="$(/bin/bash -c 'echo $BASH_VERSION')"
  ok "G1 parses under /bin/bash ($V)"
else
  bad "G1 parses under /bin/bash"
fi
if /bin/bash "$PF" --paths richos/app/ui/main.js >/dev/null 2>"$WORK/g.err"; then
  ok "G2 ...and runs under it with no shell errors"
else
  bad "G2 ...and runs under it" "$(head -3 "$WORK/g.err")"
fi
[ -s "$WORK/g.err" ] && bad "G3 nothing on stderr from a clean run" "$(head -3 "$WORK/g.err")" \
                     || ok "G3 nothing on stderr from a clean run"

echo
echo "=== the cargo targets it prints are real, per cargo itself ==="

# -----------------------------------------------------------------------------------------
# H. The Rust commands are ASSERTED AGAINST CARGO, not against the author's belief about
#    how cargo names a target. `--test <name>` is derived here from a file stem; if that
#    derivation were wrong every Rust line this script prints would be a command that
#    cannot run, and nothing else in this suite would notice.
# -----------------------------------------------------------------------------------------
CARGO=""
command -v cargo >/dev/null 2>&1 && CARGO=cargo
[ -z "$CARGO" ] && [ -x "$HOME/.cargo/bin/cargo" ] && CARGO="$HOME/.cargo/bin/cargo"
if [ -n "$CARGO" ]; then
  run_pf "$WORK/h.out" --paths richos/app/crates/richos-core/src/loro.rs
  sed -n 's/.*cargo test -p richos-core --test \([A-Za-z0-9_]*\).*/\1/p' "$WORK/h.out" \
    | LC_ALL=C sort -u > "$WORK/h.want"
  ( cd "$APP" && "$CARGO" metadata --no-deps --format-version 1 --offline 2>/dev/null ) \
    > "$WORK/meta.json"
  python3 - "$WORK/meta.json" "$WORK/h.want" > "$WORK/h.check" 2>&1 <<'PY'
import json, sys
meta = json.load(open(sys.argv[1]))
real = set()
for p in meta["packages"]:
    for t in p["targets"]:
        if "test" in t["kind"]:
            real.add(t["name"])
want = [l.strip() for l in open(sys.argv[2]) if l.strip()]
missing = [w for w in want if w not in real]
print("want=%d missing=%d" % (len(want), len(missing)))
if missing:
    print("MISSING: " + ", ".join(missing))
PY
  if [ -s "$WORK/h.want" ] && grep -q ' missing=0$' "$WORK/h.check"; then
    ok "H1 every --test target it printed is a real cargo target ($(head -1 "$WORK/h.check"))"
  else
    bad "H1 every --test target it printed is a real cargo target" "$(cat "$WORK/h.check")"
  fi

  ( cd "$APP/src-tauri" && "$CARGO" metadata --no-deps --format-version 1 --offline 2>/dev/null ) \
    | grep -q '"richos-tauri"' \
    && ok "H2 the shell's --bin name is the one cargo knows (detached nested workspace)" \
    || bad "H2 the shell's --bin name is the one cargo knows"
else
  notrun "H  the cargo target names" "no cargo on this machine or on PATH"
fi

# Execute the generated command's dispatch contract. Its real target must
# refuse a missing prerequisite; a controlled target proves cwd, environment
# and exit propagation. The owning asset suite proves real packaging and is
# selected independently. Recursing into that entire suite here duplicates its
# builds without exercising another selector behavior.
run_pf "$WORK/engine.out" --paths richos/app/scripts/make-engine-asset.test.sh
if python3 - "$ROOT" "$WORK/engine.out" <<'PYTEST'
import os, subprocess, sys, tempfile
from pathlib import Path
lines = Path(sys.argv[2]).read_text().splitlines()
commands = [line.strip() for line in lines if line.startswith("  cd ")]
for command in commands:
    subprocess.run(["bash", "-n", "-c", command], check=True)
selected = [c for c in commands if c.endswith("&& bash scripts/make-engine-asset.test.sh")]
assert len(selected) == 1, selected
assert "RICHOS_RUNTIME_DIR" in "\n".join(lines)
command = selected[0]
assert "proof-for.test.sh" not in command
missing = dict(os.environ)
missing.pop("RICHOS_RUNTIME_DIR", None)
# No clock of this check's own around the generated commands (audit R13, 2026-09-29): a
# fixed timeout on real work is a verdict on the Mac's speed. A hang is caught by the
# enclosing runner's per-suite deadline, which names this suite.
result = subprocess.run(["bash", "-c", command], cwd=sys.argv[1], env=missing,
                        capture_output=True, text=True)
assert result.returncode == 2 and "Prerequisite: set RICHOS_RUNTIME_DIR" in result.stderr
with tempfile.TemporaryDirectory(prefix="proof-generated-command-") as temporary:
    root = Path(temporary).resolve()
    scripts = root / "richos/app/scripts"
    scripts.mkdir(parents=True)
    target = scripts / "make-engine-asset.test.sh"
    target.write_text('#!/bin/bash\n'
                      '[ "$PWD" = "$EXPECTED_APP" ] || exit 91\n'
                      '[ "$RICHOS_RUNTIME_DIR" = "$EXPECTED_RUNTIME" ] || exit 92\n'
                      'echo generated-target-ran\nexit "$TARGET_EXIT"\n')
    for code in (0, 7):
        env = dict(os.environ, EXPECTED_APP=str(scripts.parent),
                   RICHOS_RUNTIME_DIR=str(root / "runtime fixture"),
                   EXPECTED_RUNTIME=str(root / "runtime fixture"), TARGET_EXIT=str(code))
        result = subprocess.run(["bash", "-c", command], cwd=root, env=env,
                                capture_output=True, text=True)
        assert result.returncode == code and result.stdout.strip() == "generated-target-ran", result

PYTEST
then
  ok "I1 generated command preserves cwd, prerequisite environment and target exit; the real target refuses a missing prerequisite"
else
  bad "I1 generated engine command and prerequisite contract"
fi

# -----------------------------------------------------------------------------------------
# L. A SHELL MODULE WITH NO TEST IS NOT COVERED BY ITS OWN EMPTY FILTER (hunt 2026-09-29,
#    part 2, section 14). `--bin richos-tauri events::` matches zero tests and exits 0.
# -----------------------------------------------------------------------------------------
run_pf "$WORK/l1.out" --paths richos/app/src-tauri/src/events.rs; L1_RC=$RC
if [ "$L1_RC" -eq 1 ] && grep -q 'UNCOVERED' "$WORK/l1.out" && ! grep -q 'richos-tauri events::' "$WORK/l1.out"; then
  ok "L1 a shell module with no test is UNCOVERED, not covered by an empty filter"
else
  bad "L1 an untested shell module is UNCOVERED" "exit $L1_RC: $(tr '\n' ' ' < "$WORK/l1.out")"
fi

# -----------------------------------------------------------------------------------------
# M. THE MERGE GATE'S SELECTION (--gate, 2026-09-30). The land of zach-opus-lander1 (88943d1b)
#    changed three mega-lander files and its merge ran make-engine-asset (packaging), gui-boot
#    and proof-for because they read `richos/engine` wholesale. Under --gate a tooling path
#    selects only what covers or names it; a product path still selects every whole-tree reader.
# -----------------------------------------------------------------------------------------
if have_commit 88943d1b; then
  run_pf "$WORK/g-full.out" --quiet 88943d1b^1..88943d1b; G_FULL=$RC
  run_pf "$WORK/g-gate.out" --quiet --gate 88943d1b^1..88943d1b; G_GATE=$RC
  if [ "$G_FULL" -eq 0 ] && [ "$G_GATE" -eq 0 ] && grep -q 'make-engine-asset.test.sh' "$WORK/g-full.out" \
     && ! grep -qE 'make-engine-asset|run-tests.sh' "$WORK/g-gate.out" \
     && [ "$(grep -c 'ci-shard.sh --only-units' "$WORK/g-gate.out")" -eq "$(grep -c 'ci-shard.sh --only-units' "$WORK/g-full.out")" ] \
     && grep -q 'only-units mega-lander/tests/app.test.sh' "$WORK/g-gate.out"; then
    ok "M1 an engine change at the gate runs its engine units and no whole-tree app suite"
  else
    bad "M1 an engine change at the gate runs its engine units and no whole-tree app suite" \
        "full exit $G_FULL, gate exit $G_GATE: $(tr '\n' ' ' < "$WORK/g-gate.out")"
  fi
else
  bad "M1 88943d1b is not in this clone — the gate fixture could not run"
fi
run_pf "$WORK/g2.out" --quiet --gate --paths richos/app/crates/richos-core/src/assignment.rs; G2_RC=$RC
if [ "$G2_RC" -eq 0 ] && grep -q 'only work-walk.test.sh\|--only work-walk.test.sh' "$WORK/g2.out" \
   && grep -q 'cargo test -p richos-core --lib assignment::' "$WORK/g2.out"; then
  ok "M2 a product change at the gate still selects the suites that read its whole tree"
else
  bad "M2 a product change at the gate still selects the suites that read its whole tree" \
      "exit $G2_RC: $(tr '\n' ' ' < "$WORK/g2.out")"
fi
run_pf "$WORK/g3.out" --quiet --gate --paths richos/app/scripts/lib/proof_slots.py; G3_RC=$RC
if [ "$G3_RC" -eq 0 ] && grep -q -- '--only proof-run.test.sh' "$WORK/g3.out" \
   && ! grep -qE 'make-engine-asset|--only (lint|not-run-verdict|no-foreign-app-data)\.test\.sh' "$WORK/g3.out"; then
  ok "M3 a script at the gate selects the suite that covers it, not every reader of scripts/"
else
  bad "M3 a script at the gate selects the suite that covers it" "exit $G3_RC: $(tr '\n' ' ' < "$WORK/g3.out")"
fi
# M4. A COVERED FILE UNDER A DIRECTORY INPUT (2026-10-05). autocheck.test.sh covers
#    autocheck/install.sh and reads it through the directory input `scripts/autocheck`, which
#    --gate does not follow for a tooling path; 480bf8dd5 changed install.sh, the merge selected
#    7 suites without autocheck.test.sh, and only the nightly found its broken case. A range that
#    changes ONLY that file, built from HEAD in a scratch index (no ref, no working tree touched).
M4_FILE=richos/app/scripts/autocheck/install.sh
mgit() { GIT_INDEX_FILE="$WORK/m4-index" git -C "$ROOT" -c user.name='proof-for fixture' \
           -c user.email=proof-for@example.invalid "$@"; }
M4_HEAD=""
if M4_BLOB="$( { git -C "$ROOT" show "HEAD:$M4_FILE"; printf '# proof-for M4 fixture\n'; } \
               | git -C "$ROOT" hash-object -w --stdin)" \
   && mgit read-tree HEAD \
   && mgit update-index --cacheinfo "100755,$M4_BLOB,$M4_FILE" \
   && M4_HEAD="$(mgit commit-tree "$(mgit write-tree)" -p HEAD -m 'proof-for fixture: covered file')"; then
  run_pf "$WORK/m4.out" --quiet --gate "HEAD..$M4_HEAD"; M4_RC=$RC
  if [ "$M4_RC" -eq 0 ] && grep -q -- '--only autocheck.test.sh' "$WORK/m4.out"; then
    ok "M4 a range changing only a covered tooling file selects the suite that covers it at the gate"
  else
    bad "M4 a range changing only a covered tooling file selects the suite that covers it at the gate" \
        "exit $M4_RC: $(tr '\n' ' ' < "$WORK/m4.out")"
  fi
else
  bad "M4 the covered-file fixture commit could not be built from HEAD"
fi
# M5. A SUITE THAT CHECKS EVERY FILE IN A TOOLING DIRECTORY (2026-10-20). proof-qualification
#    checks every UI test file for the environment names it reads, through the directory input
#    `richos/app/ui/tests`. The land of echo-opus-firstsend2 (0fe37b286) added a UI test reading
#    a new name; --gate did not follow the directory for a tooling path, the merge ran 84 checks
#    without proof-qualification, and nightly 69 failed it. Its `gate-inputs` row selects it.
if have_commit 0fe37b286; then
  run_pf "$WORK/m5.out" --quiet --gate 0fe37b286^1..0fe37b286; M5_RC=$RC
  if [ "$M5_RC" -eq 0 ] && grep -q -- '--only proof-qualification.test.sh' "$WORK/m5.out"; then
    ok "M5 a new UI test file at the gate selects the check that reads every UI test file"
  else
    bad "M5 a new UI test file at the gate selects the check that reads every UI test file" \
        "exit $M5_RC: $(tr '\n' ' ' < "$WORK/m5.out")"
  fi
else
  bad "M5 0fe37b286 is not in this clone — the gate fixture could not run"
fi

# R46 — A REFERENCE SHOT SELECTS BY WHOLE FOLDER NAME (hunt part 2, R46). shots-5 is a prefix of
# shots-5b and shots-5c; a suite that only reads those must not be selected for a shots-5 file.
R46_SHOT=""
for f in "$APP"/ui/tests/shots-5/*.png; do [ -e "$f" ] && { R46_SHOT="${f##*/}"; break; }; done
R46_REL="${APP#"$ROOT"/}/ui/tests/shots-5/$R46_SHOT"
run_pf "$WORK/r46.out" --paths "$R46_REL"
R46_BAD=""
for f in "$APP"/ui/tests/*.js; do
  b="${f##*/}"
  if grep -q "reference shot -> $b" "$WORK/r46.out" \
     && ! grep -qE -- 'shots-5([^A-Za-z0-9_-]|$)' "$f"; then R46_BAD="$R46_BAD $b"; fi
done
if [ -z "$R46_SHOT" ]; then
  bad "R46 a shots-5 file selects only suites that name shots-5" "no shots-5 reference shot in this checkout"
elif [ -n "$R46_BAD" ]; then
  bad "R46 a shots-5 file selects only suites that name shots-5" "also selected:$R46_BAD"
else
  ok "R46 a shots-5 file selects only suites that name shots-5, not shots-5b or shots-5c readers"
fi

# R47. affordances.js derives its states from the Rust command layer and crates as well as ui/
# (RUST_ROOTS in ui/tests/lib/state-strings.js). A change to a Rust file that adds a message
# must select it: bf304c664 added one in nav.rs, proof-for picked only docs-claims.js, and the
# nightly refused the build.
run_pf "$WORK/r47.out" --paths richos/app/src-tauri/src/nav.rs; R47_RC=$RC
if grep -q 'node affordances.js' "$WORK/r47.out"; then
  ok "R47 a Rust file the state inventory scrapes selects affordances.js"
else
  bad "R47 a Rust file the state inventory scrapes selects affordances.js" "exit $R47_RC: $(head -c 300 "$WORK/r47.out" | tr '\n' ' ')"
fi

# R48. A FILE A UI SUITE READS SELECTS THAT SUITE (2026-10-11). echo-opus-spend3 found
# attachments.js reading src-tauri/src/output_files.rs, and dialect.js, interruption.js and
# outage.js reading every Rust file under src-tauri/src/ and crates/, none of it declared, so a
# change there selected none of them. One pair per read the suite-by-suite audit found missing:
# the file (or one file under the directory) the suite reads, and the suite.
R48_MISSED=""
while read -r r48_path r48_suite; do
  [ -n "$r48_path" ] || continue
  run_pf "$WORK/r48.out" --paths "$r48_path"
  grep -q "node $r48_suite\$" "$WORK/r48.out" || R48_MISSED="$R48_MISSED $r48_path->$r48_suite"
done <<'PAIRS'
richos/app/src-tauri/src/output_files.rs attachments.js
richos/app/crates/richos-core/src/launch.rs dialect.js
richos/app/src-tauri/src/nav.rs dialect.js
richos/app/ui/home.css dialect.js
richos/app/crates/richos-core/src/launch.rs interruption.js
richos/app/crates/richos-core/src/launch.rs outage.js
richos/app/src-tauri/tauri.conf.json appearance.js
richos/app/crates/richos-core/src/config.rs appearance.js
richos/app/ui/timeline.js background-work.js
richos/app/ui/home.css bug-report.js
richos/app/crates/richos-core/src/timeline.rs compaction-notice.js
docs/verification/compaction-notice-2026-09-06/raw/cellT1.jsonl compaction-notice.js
docs/verification/inner-doctrine-opens-2026-09-06/raw/cellC0-control-no-doctrine.jsonl compaction-progress.js
richos/app/crates/richos-core/src/machinery.rs compaction-progress.js
richos/app/src-tauri/src/phone/attachments.rs no-home-network.js
richos/web/web-app/lib/api.js no-home-network.js
richos/app/src-tauri/src/phone/mod.rs phone.js
richos/tools/phone-probe/lib/qr.js phone.js
richos/app/src-tauri/examples/timeline_payload.rs realbytes.js
richos/app/crates/richos-core/src/timeline.rs realbytes.js
richos/app/crates/richos-core/src/journal.rs retention.js
richos/app/src-tauri/src/setup_view.rs setup.js
richos/app/crates/richos-core/src/setup.rs setup.js
richos/app/crates/richos-core/src/launch.rs splash.js
richos/app/scripts/qa/contrast.py thread-switch.js
richos/app/src-tauri/src/updates.rs updates.js
docs/verification/pr-trust-gate-2026-09-05/raw/vouch-pin-resolution.txt vouch-template.js
PAIRS
if [ -z "$R48_MISSED" ]; then
  ok "R48 every file a UI suite reads selects that suite (output_files.rs -> attachments.js, crates/ -> dialect.js, and 25 more)"
else
  bad "R48 every file a UI suite reads selects that suite" "not selected:$R48_MISSED"
fi

echo
TAIL=""
[ "$NOTRUN" -gt 0 ] && TAIL=", $NOTRUN NOT RUN"
if [ "$FAIL" -gt 0 ]; then
  echo "=== proof-for tests: $FAIL FAILED, $PASS passed$TAIL ==="
  exit 1
fi
# NOT RUN IS EXIT 2, NEVER 0 (hunt part 2, finding 18). Until 2026-09-30 a host without node
# (K4) or cargo (H) printed NOT RUN here and still exited 0, which run-tests.sh records as
# passed. Exit 2 is its "this host cannot answer": red unless a caller declares the gap.
if [ "$NOTRUN" -gt 0 ]; then
  echo "=== proof-for tests: $PASS passed, $NOTRUN NOT RUN, which is not a pass ==="
  exit 2
fi
echo "=== proof-for tests: all $PASS passed ==="
exit 0
