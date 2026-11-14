#!/usr/bin/env bash
#
# gui-boot-launch-lines.test.sh: prove the seconds-fast launch-line check goes red where B2 would.
#
# THE INCIDENT. Nightly 78's VM boot proof failed gui-boot's B2 on one line,
# `[richos] Codex watch: watching 2 channel(s) (process 17902)`, added to main.rs with no rule in
# gui-boot.test.sh. The land did not run gui-boot (it needs the test VM), so the first run of B2
# against the new line was the nightly. gui-boot-launch-lines.py reads the same rules without a boot.
#
# THE SECOND INCIDENT. The check then read main.rs alone, and a later nightly's boot proof failed B2
# on `[richos] SwearMeter historical sources: read (0 ms)`, printed from swearmeter_host.rs with no
# rule. It now reads every app source file (src-tauri/src and crates/*/src); L11-L13 hold that.
#
# Cases (each negative has its positive probe, so "clean" cannot be passing for the wrong reason):
#   L1  the real app source, gui-boot.test.sh and list are clean
#   L2  the incident replayed: the Codex watcher's routine rule removed -> RED, the line named
#   L3  a new launch line with no rule and no list entry -> RED, the exact line to add printed
#   L4  ...the same line with a routine rule that accounts for it -> clean
#   L5  ...the same line appended to the list -> clean
#   L6  the same line inside a #[cfg(test)] module is never a launch line -> clean
#   L7  a listed line no app source file prints -> RED (STALE)
#   L8  a listed line a rule now accounts for -> RED (COVERED)
#   L9  an unreadable input is exit 2, never a pass over nothing
#   L10 the land selects this check when main.rs or gui-boot.test.sh changes (proof-for.sh)
#   L11 the second incident replayed: the SwearMeter rule removed -> RED, located in swearmeter_host.rs
#   L12 a new launch line in a crate's source file -> RED, located in that file
#   L13 the same line in a crate's src/bin program or in a test module kept in its own file -> clean
#
# run-tests: no-host-screen: reads source files and runs grep and bash on rule declarations; nothing is launched
# run-tests: inputs richos/app/scripts/gui-boot-launch-lines.test.sh richos/app/scripts/gui-boot-launch-lines.py richos/app/scripts/gui-boot-launch-lines.unaccounted richos/app/scripts/gui-boot.test.sh richos/app/src-tauri/src/main.rs richos/app/src-tauri richos/app/crates richos/app/scripts/proof-for.sh
# run-tests: covers richos/app/scripts/gui-boot-launch-lines.py richos/app/scripts/gui-boot-launch-lines.unaccounted richos/app/src-tauri/src/main.rs richos/app/scripts/gui-boot.test.sh
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="$(cd "$HERE/.." && pwd)"
CHECK="$HERE/gui-boot-launch-lines.py"
MAIN="$APP/src-tauri/src/main.rs"
RULES="$HERE/gui-boot.test.sh"
LIST="$HERE/gui-boot-launch-lines.unaccounted"
SCRATCH="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/gui-boot-lines-test.XXXXXX")" && pwd -P)"
trap 'rm -rf "$SCRATCH"' EXIT
PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "${2:-}"; FAIL=$((FAIL + 1)); }

run_check() { python3 -B "$CHECK" "$@" 2>&1; }

# --- L1
OUT="$(run_check)"; CODE=$?
if [ "$CODE" -eq 0 ] && printf '%s' "$OUT" | grep -q ' 0 finding(s)'; then
  ok "L1 the app source, gui-boot.test.sh and the list agree: $(printf '%s' "$OUT" | tail -1)"
else
  bad "L1 the real tree is clean" "exit $CODE: $(printf '%s' "$OUT" | head -8 | tr '\n' ' ')"
fi

# --- L2: the incident. Remove every routine rule that mentions the Codex watcher, statement by statement.
if ! python3 -B - "$RULES" "$SCRATCH/rules-without-codex.sh" <<'PY'
import sys
src, dst = sys.argv[1:3]
lines = open(src, encoding="utf-8").read().split("\n")
out, i, removed = [], 0, 0
while i < len(lines):
    line = lines[i]
    if line.startswith("  routine '") and "Codex watch: watching" in line:
        while lines[i].rstrip().endswith("\\"):
            i += 1
        i += 1
        removed += 1
        continue
    out.append(line)
    i += 1
open(dst, "w", encoding="utf-8").write("\n".join(out))
sys.exit(0 if removed else 3)
PY
then
  bad "L2 setup" "gui-boot.test.sh has no routine rule for the Codex watcher to remove; the incident cannot be replayed"
else
  OUT="$(run_check --rules "$SCRATCH/rules-without-codex.sh")"; CODE=$?
  if [ "$CODE" -eq 1 ] && printf '%s' "$OUT" | grep -q 'UNCLASSIFIED' \
     && printf '%s' "$OUT" | grep -q 'Codex watch: watching {} channel(s) (process {})' \
     && printf '%s' "$OUT" | grep -Eq 'main\.rs:[0-9]+'; then
    ok "L2 nightly 78's line without its rule is RED, named, and located in main.rs"
  else
    bad "L2 the incident replay is red" "exit $CODE: $(printf '%s' "$OUT" | head -6 | tr '\n' ' ')"
  fi
fi

# --- L3-L6: a new launch line
NEW='[richos] idle sessions: retired {} of {} after {} min'
python3 -B - "$MAIN" "$SCRATCH/main-new.rs" "$SCRATCH/main-test-only.rs" "$NEW" <<'PY'
import re, sys
src, with_line, test_only, new = sys.argv[1:5]
text = open(src, encoding="utf-8").read()
call = '    eprintln!("%s", a, b, c);\n' % new
marker = text.index("fn main()") if "fn main()" in text else 0
open(with_line, "w", encoding="utf-8").write(text[:marker] + "fn zz_probe() {\n" + call + "}\n" + text[marker:])
open(test_only, "w", encoding="utf-8").write(text + "\n#[cfg(test)]\nmod zz_probe_tests {\n    fn t() {\n" + call + "    }\n}\n")
PY
OUT="$(run_check --main "$SCRATCH/main-new.rs")"; CODE=$?
if [ "$CODE" -eq 1 ] && printf '%s' "$OUT" | grep -q "UNCLASSIFIED" && printf '%s' "$OUT" | grep -qF "$(printf '%s' "$NEW" | sed 's/\[/[/')" \
   && printf '%s' "$OUT" | grep -q 'append the line above'; then
  ok "L3 a new launch line with no rule and no list entry is RED and the line to add is printed"
else
  bad "L3 a new unclassified line is red" "exit $CODE: $(printf '%s' "$OUT" | head -6 | tr '\n' ' ')"
fi
# L4: a routine rule that accounts for it, added to a copy of gui-boot.test.sh just before the declared gaps.
python3 -B - "$RULES" "$SCRATCH/rules-with-new.sh" <<'PY'
import sys
src, dst = sys.argv[1:3]
text = open(src, encoding="utf-8").read()
marker = "  # -- the declared gaps"
rule = "  routine '^\\[richos\\] idle sessions: retired [0-9]+ of [0-9]+ after [0-9]+ min$' \\\n    'a probe rule'\n\n"
assert marker in text
open(dst, "w", encoding="utf-8").write(text.replace(marker, rule + marker, 1))
PY
OUT="$(run_check --main "$SCRATCH/main-new.rs" --rules "$SCRATCH/rules-with-new.sh")"; CODE=$?
if [ "$CODE" -eq 0 ]; then ok "L4 the same line with a routine rule that accounts for it is clean"; else bad "L4 a rule accounts for the line" "exit $CODE: $(printf '%s' "$OUT" | head -6 | tr '\n' ' ')"; fi
# L5: appended to the list
{ cat "$LIST"; printf '%s\n' "$NEW"; } > "$SCRATCH/list-with-new"
OUT="$(run_check --main "$SCRATCH/main-new.rs" --list "$SCRATCH/list-with-new")"; CODE=$?
if [ "$CODE" -eq 0 ]; then ok "L5 the same line appended to the list is clean"; else bad "L5 the list accounts for the line" "exit $CODE: $(printf '%s' "$OUT" | head -6 | tr '\n' ' ')"; fi
# L6: inside a cfg(test) module
OUT="$(run_check --main "$SCRATCH/main-test-only.rs")"; CODE=$?
if [ "$CODE" -eq 0 ]; then ok "L6 the same line inside a #[cfg(test)] module is not a launch line"; else bad "L6 test code is out of scope" "exit $CODE: $(printf '%s' "$OUT" | head -6 | tr '\n' ' ')"; fi

# --- L7, L8: the list cannot rot
{ cat "$LIST"; echo '[richos] a line the app stopped printing long ago'; } > "$SCRATCH/list-stale"
OUT="$(run_check --list "$SCRATCH/list-stale")"; CODE=$?
if [ "$CODE" -eq 1 ] && printf '%s' "$OUT" | grep -q 'STALE' && printf '%s' "$OUT" | grep -q 'stopped printing'; then
  ok "L7 a listed line no app source file prints is RED (STALE)"
else
  bad "L7 stale list entry" "exit $CODE: $(printf '%s' "$OUT" | head -6 | tr '\n' ' ')"
fi
{ cat "$LIST"; echo '[richos] Codex watch: watching {} channel(s) (process {})'; } > "$SCRATCH/list-covered"
OUT="$(run_check --list "$SCRATCH/list-covered")"; CODE=$?
if [ "$CODE" -eq 1 ] && printf '%s' "$OUT" | grep -q 'COVERED'; then
  ok "L8 a listed line a rule now accounts for is RED (COVERED)"
else
  bad "L8 covered list entry" "exit $CODE: $(printf '%s' "$OUT" | head -6 | tr '\n' ' ')"
fi

# --- L9: missing input
run_check --main "$SCRATCH/no-such-main.rs" >/dev/null; C1=$?
run_check --rules "$SCRATCH/no-such-rules.sh" >/dev/null; C2=$?
run_check --list "$SCRATCH/no-such-list" >/dev/null; C3=$?
printf 'not a rules file\n' > "$SCRATCH/wrong-shape.sh"; run_check --rules "$SCRATCH/wrong-shape.sh" >/dev/null; C4=$?
if [ "$C1" -eq 2 ] && [ "$C2" -eq 2 ] && [ "$C3" -eq 2 ] && [ "$C4" -eq 2 ]; then
  ok "L9 an unreadable main.rs, rules file or list, and a rules file of another shape, are exit 2"
else
  bad "L9 inputs that cannot be read" "exit codes: main $C1, rules $C2, list $C3, wrong shape $C4 (all want 2)"
fi

# --- L10: the land selects it
SEL="$(cd "$APP/.." && bash "$HERE/proof-for.sh" --paths richos/app/src-tauri/src/main.rs 2>&1 || true)"
SEL2="$(cd "$APP/.." && bash "$HERE/proof-for.sh" --paths richos/app/scripts/gui-boot.test.sh 2>&1 || true)"
if printf '%s' "$SEL" | grep -q 'gui-boot-launch-lines' && printf '%s' "$SEL2" | grep -q 'gui-boot-launch-lines'; then
  ok "L10 proof-for.sh selects this check for a change to main.rs and for a change to gui-boot.test.sh"
else
  bad "L10 selection" "main.rs: $(printf '%s' "$SEL" | grep -c launch-lines) hit(s); gui-boot.test.sh: $(printf '%s' "$SEL2" | grep -c launch-lines) hit(s)"
fi

# --- L11: the second incident. Every SwearMeter rule removed (none on a tree that never had one).
python3 -B - "$RULES" "$SCRATCH/rules-without-swearmeter.sh" <<'PY'
import sys
src, dst = sys.argv[1:3]
lines = open(src, encoding="utf-8").read().split("\n")
out, i = [], 0
while i < len(lines):
    if lines[i].startswith("  routine '") and "SwearMeter historical sources" in lines[i]:
        while lines[i].rstrip().endswith("\\"):
            i += 1
        i += 1
        continue
    out.append(lines[i])
    i += 1
open(dst, "w", encoding="utf-8").write("\n".join(out))
PY
OUT="$(run_check --rules "$SCRATCH/rules-without-swearmeter.sh")"; CODE=$?
if [ "$CODE" -eq 1 ] && printf '%s' "$OUT" | grep -q 'UNCLASSIFIED' \
   && printf '%s' "$OUT" | grep -qF '[richos] SwearMeter historical sources: {} ({} ms)' \
   && printf '%s' "$OUT" | grep -Eq 'src-tauri/src/swearmeter_host\.rs:[0-9]+'; then
  ok "L11 the SwearMeter line without its rule is RED, named, and located in swearmeter_host.rs, not main.rs"
else
  bad "L11 a launch line printed outside main.rs is caught" "exit $CODE: $(printf '%s' "$OUT" | head -6 | tr '\n' ' ')"
fi

# --- L12, L13: a copy of the app's source, so a crate file can be changed without touching the tree.
COPY="$SCRATCH/app"
mkdir -p "$COPY/src-tauri" && cp -R "$APP/src-tauri/src" "$COPY/src-tauri/src"
for d in "$APP"/crates/*/src; do
  c="$(basename "$(dirname "$d")")"; mkdir -p "$COPY/crates/$c" && cp -R "$d" "$COPY/crates/$c/src"
done
CORE="$COPY/crates/richos-core/src"
CALL="    eprintln!(\"$NEW\", a, b, c);"
printf 'fn zz_probe() {\n%s\n}\n' "$CALL" > "$CORE/zz_probe.rs"
OUT="$(run_check --app "$COPY")"; CODE=$?
if [ "$CODE" -eq 1 ] && printf '%s' "$OUT" | grep -q 'UNCLASSIFIED' \
   && printf '%s' "$OUT" | grep -q 'crates/richos-core/src/zz_probe\.rs:2'; then
  ok "L12 a new launch line in a crate's source file is RED and located in that file"
else
  bad "L12 a crate's launch line is caught" "exit $CODE: $(printf '%s' "$OUT" | head -6 | tr '\n' ' ')"
fi
rm -f "$CORE/zz_probe.rs"
mkdir -p "$CORE/bin" && printf 'fn main() {\n%s\n}\n' "$CALL" > "$CORE/bin/zz_probe.rs"
printf '\n#[cfg(test)]\nmod zz_probe_tests;\n' >> "$CORE/lib.rs"
printf 'fn t() {\n%s\n}\n' "$CALL" > "$CORE/zz_probe_tests.rs"
OUT="$(run_check --app "$COPY")"; CODE=$?
if [ "$CODE" -eq 0 ]; then
  ok "L13 the same line in a crate's src/bin program and in a test module kept in its own file is not a launch line"
else
  bad "L13 other programs and out-of-line test modules are out of scope" "exit $CODE: $(printf '%s' "$OUT" | head -6 | tr '\n' ' ')"
fi

printf '\n=== gui-boot-launch-lines.test.sh: %d FAILED, %d passed ===\n' "$FAIL" "$PASS"
[ "$FAIL" -eq 0 ]
