#!/usr/bin/env bash
# run-tests: no-host-screen: runs the production C4 logic against synthetic accessibility dumps and input
# run-tests: inputs richos/app/scripts/front-door-c4.test.sh richos/app/scripts/front-door.test.sh
# run-tests: covers richos/app/scripts/front-door.test.sh
set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/front-door-c4.XXXXXX")" || exit 2
trap 'rm -rf "$TMP"' EXIT
export TMP

# Extract only the AX predicates and C4. The launcher and native input commands are never
# part of this fixture. Each case supplies its own sequence of snapshots and click/key sink.
{
  cat <<'FIXTURE'
set -uo pipefail
FAIL=0
FIRST_RUN='Which company is this copy of Rich for?'
mode="$1"
input=""
events=0
ticks=0
event_tick=0
sleep() {
  ticks=$((ticks + 1))
  # Advance the hang guard without spending wall time.
  SECONDS=$((SECONDS + 20))
}
ok() { printf 'PASS %s\n' "$*"; }
bad() { FAIL=$((FAIL + 1)); printf 'FAIL %s\n' "$*"; }
note() { printf '%s\n' "$*"; }
record_input() {
  input="${input:+$input }$1"
  events=$((events + 1))
  event_tick=$ticks
}
send() { record_input "click:$1"; }
send_key() { record_input "key:$1"; }
ax() {
  local stage="$1" center="100,100" panel=no
  {
    if [ "$mode" = lost-window ] && [ "$events" = 2 ]; then
      printf 'windows:0\nERR the window disappeared\n'
      return
    fi
    printf 'windows:1\n'
    if [ "$mode" = desk-delayed ] && [ "$ticks" -lt 2 ]; then
      printf 'AXComboBox | 10,10 770x46 | name=Search | value=\n'
      center="200,100"
    fi
    printf 'AXPopUpButton | %s 40x40 | name=Settings | value=\n' "$center"
    if [ "$events" = 1 ] && [ "$mode" != never-opens ]; then
      if [ "$mode" != open-delayed ] || [ "$((ticks - event_tick))" -ge 2 ]; then
        panel=yes
      fi
    elif [ "$events" = 2 ]; then
      if [ "$mode" = escape-ignored ] || {
        [ "$mode" = close-delayed ] && [ "$((ticks - event_tick))" -lt 2 ]
      }; then
        panel=yes
      fi
    fi
    if [ "$panel" = yes ]; then
      printf 'AXGroup | 455,159 1070x764 | name=Settings | value=\n'
      printf 'AXStaticText | 478,510 114x20 | name=Technical view | value=Technical view\n'
      printf 'AXCheckBox | 735,509 39x22 | name=Technical view | value=0\n'
    fi
  } > "$TMP/ax-$stage.txt"
}
FIXTURE
  awk '
    /^windows_in\(\)/ { reading = 1 }
    reading && /^# C0 / { reading = 0 }
    /^search_open\(\)/ { desk = 1 }
    /^# C4 / { c4 = 1 }
    c4 && /^# Z / { exit }
    reading || desk || c4 { print }
    desk && /^}/ { desk = 0 }
  ' "$DIR/front-door.test.sh"
  cat <<'FIXTURE'
printf 'input=%s\n' "$input"
[ "$FAIL" -eq 0 ]
FIXTURE
} > "$TMP/c4-fixture"

fail=0
check() {
  local mode="$1" expected="$2" verdict="$3" input="$4" status=0 output
  output="$(/bin/bash "$TMP/c4-fixture" "$mode")" || status=$?
  if [ "$status" = "$expected" ] && [[ "$output" == *"$verdict"* ]] &&
     [[ "$output" == *$'\n'"input=$input" ]]; then
    printf '  PASS  %s\n' "$mode"
  else
    printf '  FAIL  %s (exit %s, expected %s)\n%s\n' "$mode" "$status" "$expected" "$output"
    fail=$((fail + 1))
  fi
}

check immediate 0 'PASS C4  Escape closed' 'click:c:120,120 key:53'
check desk-delayed 0 'PASS C4  Escape closed' 'click:c:120,120 key:53'
check open-delayed 0 'PASS C4  Escape closed' 'click:c:120,120 key:53'
check close-delayed 0 'PASS C4  Escape closed' 'click:c:120,120 key:53'
check never-opens 1 'FAIL C4  the settings menu did not open' 'click:c:120,120'
check escape-ignored 1 'FAIL C4  the settings menu survived Escape' 'click:c:120,120 key:53'
check lost-window 1 'FAIL C4' 'click:c:120,120 key:53'
if [ "$fail" -gt 0 ]; then
  printf '=== front-door C4: %s FAILED ===\n' "$fail"
  exit 1
fi
printf '=== front-door C4: all 7 passed ===\n'
