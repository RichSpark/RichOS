#!/usr/bin/env bash
# qualification-pins.test.sh — qualification-pins.py renews the SHA-256 pins in the qualification
# record for changed files, in place and byte-for-byte otherwise, and the commit hook's refusal
# message names it. Throwaway directory only; nothing builds, boots or opens a window.
#   Q1  --all is clean on matching pins and exits 1 naming a stale one
#   Q2  --renew PATH moves only that file's pin; other stale pins and all other bytes stay
#   Q3  --renew with no PATH renews every stale pin
#   Q4  a pin for a removed file is refused, never dropped
#   Q5  a PATH that is not pinned is reported, and nothing changes
#   Q6  the hook's refusal message names the tool
#   Q7  --note records the renewal in the renewed unit's own review text and nowhere else
# run-tests: no-host-screen: a throwaway directory only; nothing is launched on any screen
# run-tests: inputs richos/app/scripts/qualification-pins.test.sh richos/app/scripts/autocheck/qualification-pins.py
# run-tests: covers richos/app/scripts/autocheck/qualification-pins.py
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOL="$here/autocheck/qualification-pins.py"
SANDBOX="$(mktemp -d "${TMPDIR:-/tmp}/qualification-pins.XXXXXX")"
trap 'rm -rf "$SANDBOX"' EXIT
FAILS=0
ok()  { printf '  PASS  %s\n' "$1"; }
bad() { printf '  FAIL  %s\n' "$1"; FAILS=$((FAILS + 1)); }
sum() { shasum -a 256 "$1" | cut -d' ' -f1; }

R="$SANDBOX/repo"
mkdir -p "$R/docs/development" "$R/src"
printf 'one\n' > "$R/src/a.sh"
printf 'two\n' > "$R/src/b.sh"
write_record() { # <digest-a> <digest-b>
    cat > "$R/docs/development/verification-input-qualifications.json" <<EOF
{
  "units": {
    "unit one": {
      "review": "reads a.sh and b.sh; text with a \"quote\" that must survive",
      "sources": {
        "src/a.sh":   "$1",
        "src/b.sh": "$2"
      }
    }
  }
}
EOF
}
run() { QUALIFICATION_PINS_TOP="$R" python3 "$TOOL" "$@"; }

write_record "$(sum "$R/src/a.sh")" "$(sum "$R/src/b.sh")"
if run --all >/dev/null 2>&1; then ok "Q1a --all is clean when every pin matches"; else bad "Q1a --all failed on matching pins"; fi

OLD_A="$(sum "$R/src/a.sh")"
OLD_B="$(sum "$R/src/b.sh")"
printf 'changed\n' >> "$R/src/a.sh"
printf 'changed\n' >> "$R/src/b.sh"
OUT="$(run --all 2>&1)"; RC=$?
if [ "$RC" = 1 ] && grep -q 'src/a.sh' <<<"$OUT" && grep -q 'src/b.sh' <<<"$OUT"; then
    ok "Q1b --all exits 1 and names each stale pin"
else bad "Q1b rc=$RC: $OUT"; fi

cp "$R/docs/development/verification-input-qualifications.json" "$SANDBOX/before.json"
run --renew src/a.sh >/dev/null 2>&1
if grep -q "$(sum "$R/src/a.sh")" "$R/docs/development/verification-input-qualifications.json" \
   && grep -q "$OLD_B" "$R/docs/development/verification-input-qualifications.json" \
   && ! grep -q "$OLD_A" "$R/docs/development/verification-input-qualifications.json" \
   && diff <(sed -E 's/[0-9a-f]{64}/H/' "$SANDBOX/before.json") \
           <(sed -E 's/[0-9a-f]{64}/H/' "$R/docs/development/verification-input-qualifications.json") >/dev/null; then
    ok "Q2  --renew PATH moves only that pin; formatting and review text are byte-identical"
else bad "Q2  --renew src/a.sh changed the wrong bytes"; fi

run --renew >/dev/null 2>&1
if run --all >/dev/null 2>&1; then ok "Q3  --renew with no PATH renews every stale pin"; else bad "Q3  a pin is still stale"; fi

rm "$R/src/b.sh"
cp "$R/docs/development/verification-input-qualifications.json" "$SANDBOX/before.json"
OUT="$(run --renew src/b.sh 2>&1)"; RC=$?
if [ "$RC" != 0 ] && grep -q 'no longer exists' <<<"$OUT" && cmp -s "$SANDBOX/before.json" "$R/docs/development/verification-input-qualifications.json"; then
    ok "Q4  a pin for a removed file is refused and the record is untouched"
else bad "Q4  rc=$RC: $OUT"; fi

printf 'x\n' > "$R/src/c.sh"
OUT="$(run --renew src/c.sh 2>&1)"; RC=$?
if [ "$RC" = 0 ] && grep -q 'is not pinned' <<<"$OUT" && cmp -s "$SANDBOX/before.json" "$R/docs/development/verification-input-qualifications.json"; then
    ok "Q5  a path that is not pinned is reported and nothing changes"
else bad "Q5  rc=$RC: $OUT"; fi

if grep -q 'qualification-pins.py --renew' "$here/autocheck/autocheck.py"; then
    ok "Q6  the commit hook's refusal message names the renewal tool"
else bad "Q6  autocheck.py does not name qualification-pins.py --renew"; fi

# Q7: two units, one pinning the changed file; the note lands in that unit's review only, and
# a unit whose review text is identical but pins nothing that moved keeps it byte for byte.
ZEROS="0000000000000000000000000000000000000000000000000000000000000000"
cat > "$R/docs/development/verification-input-qualifications.json" <<EOF
{
  "units": {
    "pins c": {
      "review": "the same text",
      "sources": {"src/c.sh": "$ZEROS"}
    },
    "pins a": {
      "review": "the same text",
      "sources": {"src/a.sh": "$(sum "$R/src/a.sh")"}
    }
  }
}
EOF
OUT="$(run --renew --note "reads nothing new" src/c.sh 2>&1)"; RC=$?
review() { python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["units"][sys.argv[2]]["review"])' \
    "$R/docs/development/verification-input-qualifications.json" "$1"; }
NOTE_C="$(review "pins c")"
NOTE_A="$(review "pins a")"
if [ "$RC" = 0 ] && grep -Eq '^the same text Renewed [0-9-]+: src/c.sh 00000000 to [0-9a-f]{8}\. reads nothing new$' <<<"$NOTE_C" \
   && [ "$NOTE_A" = "the same text" ] && run --all >/dev/null 2>&1; then
    ok "Q7  --note records the renewal in the renewed unit's own review text and nowhere else"
else bad "Q7  rc=$RC c=[$NOTE_C] a=[$NOTE_A]: $OUT"; fi

if [ "$FAILS" = 0 ]; then echo "=== qualification-pins tests: all passed ==="; else echo "=== qualification-pins tests: $FAILS FAILED ==="; exit 1; fi
