#!/usr/bin/env bash
#
# qa.test.sh — the acceptance suite for the walk toolkit in scripts/qa/.
#
# WHY IT IS HERE AND NOT AT scripts/qa/qa.test.sh
# ===============================================
# `run-tests.sh` builds its inventory with `find -maxdepth 1`. A suite one
# directory down runs in no build and appears in no `--only` list — which is
# how `scripts/testvm/test/run-tests.sh` came to be invisible, recorded in
# `proof-for.sh`'s own comment. A toolkit whose tests never run is a toolkit
# whose next user rewrites it, so the suite sits where the harness looks and
# the tools sit next to each other.
#
# WHAT IT PROVES, AND WHAT IT DELIBERATELY DOES NOT
# =================================================
# Every case runs against committed fixtures in seconds. Nothing here boots a
# VM, opens a window, photographs a screen or presses a key: the two tools
# that CAN do those things are proved by driving them into their refusal.
#
# The refusals are the point of half of this file. Each tool replaces a
# scratch script that reported success in a state where it had measured
# nothing — a wait that timed out and exited 0, a gate whose reader was blind,
# a redactor that never re-read its output, a contrast estimator that took a
# single antialiasing pixel as the ink. Those are the cases with the long
# names.
#
# Cases:
#   H1-H9   every tool answers --help and exits 0
#   C1-C9   contrast: the arithmetic, the spellings, the thresholds, refusals
#   P1-P3   the PNG layer: the no-Pillow decoder agrees with Pillow, round trip
#   F1-F6   frame: pixels, extent, inset, motion, and mismatched sizes refused
#   O1-O6   the OCR gate and the finder, including a blind reader and an
#           empty frame set
#   R1-R4   redact: it covers the address, it keeps the evidence, it re-reads
#   W1-W8   wait-for: it succeeds, and it FAILS on timeout
#   T1-T6   timeline: first change, no change, no baseline, stats, refusals
#   N1-N5   phone-client: a foreign Mac that refuses the credential is the
#           answer, one that ACCEPTS it exits non-zero; argument refusals
#   K1-K5   flake-rate: counts, interleaving, a kept failing log, refusals
#   R5-R8   redact --phones: the gate flags a phone-shaped fixture, the redactor
#           covers it, the gate then passes the output, the evidence survives
#   R9-R12  redact --homes: the same four for a /Users/<name> home path
#   A1-A19  phone-android: taps by the words on screen, refuses an unnamed or
#           unattached phone, times out as a failure, reads a closure state and
#           an idle-frame rhythm, types through a key map, reads the editable
#           field, runs one closure cell, finds unnamed controls, all against a
#           scripted adb (no phone); A22-A23 refuses unless randroid device started
#           it, and refuses a phone holding a debuggable RichConnect
#   L1-L4   lab-ledger: exactly once passes; missing, duplicated or altered
#           fails; a timeline without the isolated lab's owner marker is refused
#   LP1-LP4 lab-pause: the lab's listener is held stopped between two marks and
#           continued; a lab without the owner marker and a pid that is not the
#           listener are refused; a missing mark times out and still continues
#   SM1-SM2 step-mark: one timestamped line appended to a step log; no words refused
#   HS1-HS3 hidden-send-try: tap Send, Home, Wi-Fi off, release, Wi-Fi on in order
#           against a scripted adb; Wi-Fi put back on when the switch failed half
#           way; a malformed Send position refused; HS6-HS7 a debuggable build or a
#           start outside randroid device refused before the first mark
#   TR1-TR4 tunnel-requests: the lab tunnel's request counter sampled from a scripted
#           helper and counted between two instants (two rises); an unreadable
#           reading in the window, a window the readings miss and a port without
#           the counter are each refused
#   I0      phone-ios: a command that touches the phone refuses unless rios device ran it
#   I1-I16  phone-ios: a step list validated before any build, refusals, the
#           per-step log read once each, no picture of the person's account,
#           a reused build trusted only against its stamp, no lock without a
#           person to open it, a bounded log capture kept on the SSD; I35-I36 a
#           list the XCUITest allowance cannot hold is refused before the phone
#           is touched (later shots would be lost); I37 a reused build is the
#           products its stamp names (a shared-store entry included); I17-I18
#           pair-steps for pairing v2 waits for each word and holds before the press;
#           I19-I23 appearance and orientation steps, summary of a finished run;
#           I24-I26 the phone runner's step dispatcher (PhysicalDeviceTests.perform)
#           knows every step the host validates, a drifted runner FAILS; I27-I28
#           syslog-rate counts one process's log entries per bucket, refuses a non-log;
#           I31-I32 Settings and Safari places and an https-only open step; I33-I34
#           another app launched fresh, a switch by kind and label, SpringBoard never closed
#   FC1-FC4 phone-ios forecast: prompt-free runs, other id, unknown, run ledger
#   J1-J7   phone-ios approval: the UI-automation approval forecast from the phone's
#           own sessions, and a run expected to ask refused until the CEO was told
#   V1-V10  pair-words: the phone corpus's own v2 words, the origin written as a
#           browser writes it, the lab's words from its own files, a mismatch
#           that FAILS, a half-read phone log, a non-lab directory, a v1 phone
#           and a lab no phone has reached, each refused
#   US1-US4 usage-shape: a get_usage answer RichOS's reader gets no figures from (rate_limits
#           null) exits 1 naming the rule; a readable one exits 0; no binary and a
#           claude that never answers are refused, against a scripted claude
#   X1      the committed fixtures still match their generator
#
# run-tests: no-host-screen: its only capture/keystroke references are the strings it asserts those two tools REFUSE to act on, and its frames are committed PNG fixtures
# run-tests: inputs richos/app/scripts/qa.test.sh richos/app/scripts/qa richos/mobile/conformance/vectors/fingerprint.json richos/mobile/native-ios/UITests/PhysicalDeviceTests.swift
# run-tests: covers richos/app/scripts/qa/usage-shape.sh richos/app/scripts/qa/pair-words.py richos/app/scripts/qa/contrast.py richos/app/scripts/qa/frame.py richos/app/scripts/qa/lib/qaimg.py richos/app/scripts/qa/lib/qaocr.py richos/app/scripts/qa/ocr-find.py richos/app/scripts/qa/ocr-find.sh richos/app/scripts/qa/ocr-gate.sh richos/app/scripts/qa/ocr-read.py richos/app/scripts/qa/ocr-watch.sh richos/app/scripts/qa/redact.py richos/app/scripts/qa/timeline.py richos/app/scripts/qa/timeline-bounds.test.py richos/app/scripts/qa/wait-for.sh richos/app/scripts/qa/phone-client.mjs richos/app/scripts/qa/flake-rate.sh richos/app/scripts/qa/phone-android.py richos/app/scripts/qa/lab-ledger.py richos/app/scripts/qa/lab-pause.py richos/app/scripts/qa/step-mark.py richos/app/scripts/qa/hidden-send-try.py richos/app/scripts/qa/tunnel-requests.py richos/app/scripts/qa/fixtures/make-fixtures.py richos/app/scripts/qa/phone-ios.py richos/app/scripts/qa/stall-run.py richos/app/scripts/qa/under-load.py richos/app/scripts/qa/busy-sample.py richos/app/scripts/qa/lib/busy-sample/sitecustomize.py richos/app/scripts/qa/slow-shutdown.py richos/app/scripts/qa/ocr-cache.test.py richos/app/scripts/qa/child-lifetime.test.py richos/app/scripts/qa/trust-reading.test.py richos/app/scripts/qa/device-launch.test.py richos/app/scripts/qa/lab-pause.test.py richos/app/scripts/qa/tunnel-requests.test.py richos/app/scripts/qa/phone-ios-run.test.py richos/mobile/native-ios/UITests/PhysicalDeviceTests.swift
set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
QA="$DIR/qa"
FIX="$QA/fixtures"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/qa-toolkit-test.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
# phone-ios.py raises a needs=ceo-hands escalation at its approval refusal
# (2026-10-01). Every case here writes to this fixture ledger, never to the
# operator's ~/.claude/state/escalations.jsonl, where a fixture row would wake
# the lead every 10 minutes about a phone that does not exist.
export RICHOS_ESCALATION_LEDGER="$TMP/escalations.jsonl"
REAL_LEDGER="$HOME/.claude/state/escalations.jsonl"
# A path that is off /Volumes/E1TB whatever TMPDIR is (agents keep TMPDIR on the SSD). The refusals under test happen before anything is written there.
OFFSSD="/private/var/empty/qa-toolkit-off-ssd"

PASS=0; FAIL=0; SKIP=0
ok()   { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad()  { printf '  FAIL  %s\n         %s\n' "$1" "${2:-}"; FAIL=$((FAIL + 1)); }
skip() { printf '  SKIP  %s\n         %s\n' "$1" "${2:-}"; SKIP=$((SKIP + 1)); }
run()  { OUT="$("$@" 2>&1)"; CODE=$?; return 0; }

expect() {  # expect <name> <exit> <substring>
  local name="$1" want="$2" needle="$3"
  if [ "$CODE" != "$want" ]; then
    bad "$name" "exit $CODE, wanted $want. Output: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"
  elif ! printf '%s' "$OUT" | grep -Fq -- "$needle"; then
    bad "$name" "exit $want as wanted, but the output never said '$needle'. Output: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"
  else
    ok "$name"
  fi
}

TESS="${RICHOS_QA_TESSERACT:-}"
[ -n "$TESS" ] || TESS="$(command -v tesseract || true)"
for c in /opt/homebrew/bin/tesseract /usr/local/bin/tesseract; do
  [ -n "$TESS" ] && break
  [ -x "$c" ] && TESS="$c"
done
HAVE_TESS=0; [ -n "$TESS" ] && HAVE_TESS=1

HAVE_PIL=0
python3 -c 'import PIL' >/dev/null 2>&1 && HAVE_PIL=1

echo ""
echo "=== H. every tool answers for itself ==="
for t in contrast.py frame.py redact.py timeline.py ocr-gate.sh ocr-find.sh \
         ocr-watch.sh wait-for.sh phone-client.mjs flake-rate.sh phone-android.py lab-ledger.py lab-pause.py step-mark.py hidden-send-try.py phone-ios.py pair-words.py usage-shape.sh fixtures/make-fixtures.py; do
  if [ ! -x "$QA/$t" ]; then
    bad "H $t is executable" "not present or not executable at $QA/$t"
    continue
  fi
  run "$QA/$t" --help
  if [ "$CODE" != 0 ]; then
    bad "H $t --help exits 0" "exit $CODE"
  elif [ "$(printf '%s' "$OUT" | wc -c)" -lt 80 ]; then
    bad "H $t --help says something" "only $(printf '%s' "$OUT" | wc -c) bytes of help"
  else
    ok "H $t --help exits 0 and describes the tool"
  fi
done

echo ""
echo "=== C. contrast: the arithmetic, once, for everyone ==="

run "$QA/contrast.py" '#767676' '#FFFFFF'
expect "C1 a known pair computes to its published ratio" 0 "4.54:1"

run "$QA/contrast.py" '#A0A0A0' '#FFFFFF'
expect "C2 a failing pair FAILS and exits non-zero" 1 "2.61:1"

# 3.03:1 — under the 4.5 floor for normal text, over the 3.0 floor for large.
run "$QA/contrast.py" '#949494' '#FFFFFF'
expect "C3 a mid pair fails as normal text" 1 "FAIL"
run "$QA/contrast.py" '#949494' '#FFFFFF' --large
expect "C3b the same pair passes as large text" 0 "PASS"

A="$("$QA/contrast.py" '#767676' '#ffffff' | sed -n 's/.*RATIO \([0-9.]*:1\).*/\1/p')"
B="$("$QA/contrast.py" '767676' 'FFFFFF' | sed -n 's/.*RATIO \([0-9.]*:1\).*/\1/p')"
C="$("$QA/contrast.py" '118,118,118' 'rgb(255,255,255)' | sed -n 's/.*RATIO \([0-9.]*:1\).*/\1/p')"
if [ "$A" = "$B" ] && [ "$B" = "$C" ] && [ -n "$A" ]; then
  ok "C4 hex, bare hex and rgb() spellings of one value agree ($A)"
else
  bad "C4 the three spellings of one value must agree" "got '$A' '$B' '$C'"
fi

run "$QA/contrast.py" 'not-a-color' '#fff'
expect "C5 an unparseable value is refused with a sentence" 1 "hex"

run "$QA/contrast.py" "$FIX/pair-pass.png" --box 0 0 240 80
expect "C6 a region just over the floor passes" 0 "4.54:1"

run "$QA/contrast.py" "$FIX/pair-fail.png" --box 0 0 240 80
expect "C7 a region under the floor fails and exits non-zero" 1 "2.61:1"

run "$QA/contrast.py" "$FIX/pair-pass.png" "off:5000,5000,5100,5100"
expect "C8 a region outside the frame is refused, never clamped to a guess" 1 "does not overlap"

# The whole reason this file exists: a single stray pixel must not set the figure.
python3 - "$QA/lib" "$TMP/stray.png" <<'PY'
import sys
sys.path.insert(0, sys.argv[1])
import qaimg
img = qaimg.render_blocks(240, 80, (255, 255, 255), (0x76, 0x76, 0x76), (40, 20, 160, 40))
img.fill_box(0, 0, 2, 2, (0, 0, 0))          # four black pixels, 0.02% of the frame
qaimg.save(img, sys.argv[2])
PY
run "$QA/contrast.py" "$TMP/stray.png" --box 0 0 240 80
if [ "$CODE" = 0 ] && printf '%s' "$OUT" | grep -Fq "4.54:1"; then
  ok "C9 four stray black pixels do not become the text color"
else
  bad "C9 a sub-floor artifact set the reported ratio" \
      "$(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"
fi

echo ""
echo "=== P. the PNG layer ==="

if [ "$HAVE_PIL" = 1 ]; then
  run python3 - "$QA/lib" "$FIX/pair-pass.png" "$FIX/pair-fail.png" "$FIX/ocr-control.png" <<'PY'
import sys
sys.path.insert(0, sys.argv[1])
import qaimg
if not qaimg.HAVE_PIL:
    print("Pillow is not importable, so there is nothing to compare against")
    raise SystemExit(3)
bad = 0
for path in sys.argv[2:]:
    with_pil = qaimg.load(path)
    with open(path, "rb") as fh:
        without = qaimg._decode_png(fh.read())
    if (with_pil.w, with_pil.h) != (without.w, without.h):
        print("SIZE DIFFERS for %s" % path); bad += 1; continue
    if bytes(with_pil.px) != bytes(without.px):
        n = sum(1 for a, b in zip(with_pil.px, without.px) if a != b)
        print("PIXELS DIFFER for %s in %d bytes" % (path, n)); bad += 1
    else:
        print("identical %s" % path)
raise SystemExit(1 if bad else 0)
PY
  expect "P1 the no-Pillow decoder agrees with Pillow, byte for byte" 0 "identical"
else
  skip "P1 the no-Pillow decoder agrees with Pillow" "Pillow is not installed, so there is no reference to compare against"
fi

run python3 - "$QA/lib" "$TMP/roundtrip.png" <<'PY'
import sys
sys.path.insert(0, sys.argv[1])
import qaimg
src = qaimg.render_blocks(37, 19, (10, 200, 30), (250, 4, 128), (5, 3, 20, 11))
qaimg.save(src, sys.argv[2])
back = qaimg.load(sys.argv[2])
assert (back.w, back.h) == (37, 19), "size changed"
assert bytes(back.px) == bytes(src.px), "pixels changed"
print("round trip exact at 37x19")
PY
expect "P2 save then load returns the same pixels" 0 "round trip exact"

printf 'this is not a PNG at all' > "$TMP/notapng.png"
run "$QA/frame.py" px "$TMP/notapng.png" 0 0
expect "P3 a file that is not a PNG is refused by name" 1 "could not read"

echo ""
echo "=== F. frame: where things actually are ==="

run "$QA/frame.py" px "$FIX/pair-pass.png" 120 40 5 5
if [ "$CODE" = 0 ] \
   && printf '%s' "$OUT" | grep -Fq "#767676" \
   && printf '%s' "$OUT" | grep -Fq "#FFFFFF"; then
  ok "F1 px reads the block color and the ground"
else
  bad "F1 px read the wrong colors" "$(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"
fi

run "$QA/frame.py" px "$FIX/pair-pass.png" 9999 0
expect "F2 a point outside the frame is refused" 1 "outside the"

run "$QA/frame.py" extent "$FIX/pair-pass.png" 0 0 240 80
if [ "$CODE" = 0 ] \
   && printf '%s' "$OUT" | grep -Fq "content x 40..199" \
   && printf '%s' "$OUT" | grep -Fq "content y 20..59"; then
  ok "F3 extent finds the block the fixture was built with"
else
  bad "F3 extent reported the wrong box" "$(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"
fi

run "$QA/frame.py" inset "$FIX/pair-pass.png" 0 0 240 80
if [ "$CODE" = 0 ] \
   && printf '%s' "$OUT" | grep -Eq "inset left +40" \
   && printf '%s' "$OUT" | grep -Eq "inset top +20" \
   && printf '%s' "$OUT" | grep -Eq "inset right +40" \
   && printf '%s' "$OUT" | grep -Eq "inset bottom +20"; then
  ok "F4 inset measures all four gaps (the 18/18 settings-button check)"
else
  bad "F4 inset reported the wrong gaps" "$(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"
fi

run "$QA/frame.py" motion "$FIX/pair-pass.png" "$FIX/pair-pass.png"
expect "F5 a frame against itself shows no motion" 0 "changed 0/"

run "$QA/frame.py" motion "$FIX/pair-pass.png" "$FIX/ocr-control.png"
expect "F6 frames of different sizes are refused, not compared" 1 "cannot be compared"

echo ""
echo "=== O. the OCR gate and the finder ==="

# The gate also reads a walk record's text files (step lists, the record). No reader needed.
mkdir -p "$TMP/textgate"
printf '[{"op": "push", "src": "/Users/qafixture/wt/frame.py", "dest": "/Users/admin/qa/frame.py"}]\n' > "$TMP/textgate/leak.json"
printf '[{"op": "push", "src": "richos/app/scripts/qa/frame.py", "dest": "/Users/admin/qa/frame.py"}]\n' > "$TMP/textgate/clean.json"
run "$QA/ocr-gate.sh" --text "$TMP/textgate/leak.json"
expect "O7 the text gate refuses a step list carrying an operator home path" 1 "TEXT GATE: 1 of 1"
if printf '%s' "$OUT" | grep -Fq "qafixture"; then
  bad "O8 the text gate does not echo the home path" "the match was printed verbatim"
else
  ok "O8 the text gate names the hit by shape and length, never echoing it"
fi
run "$QA/ocr-gate.sh" --text "$TMP/textgate/clean.json"
expect "O9 the text gate passes a portable step list (the guest's /Users/admin is not a leak)" 0 "TEXT GATE: 0 of 1"
mkdir -p "$TMP/textgate-empty"
run "$QA/ocr-gate.sh" --text "$TMP/textgate-empty"
expect "O10 an empty text set is refused, never called clean" 2 "found NO text files"

if [ "$HAVE_TESS" = 0 ]; then
  skip "O1-O6 the OCR cases" "no tesseract on this machine (brew install tesseract) — the gate, the finder and the redactor's re-scan were NOT exercised"
else
  run "$QA/ocr-gate.sh" "$FIX/ocr-control.png"
  expect "O1 the gate finds an address-shaped string and exits non-zero" 1 "OCR GATE: 1 of 1"

  if printf '%s' "$OUT" | grep -Fq "qa-fixture@example.invalid"; then
    bad "O2 the gate does not echo the thing it is protecting" \
        "the match was printed verbatim into the log"
  else
    ok "O2 the gate names the hit by shape and length, never echoing it"
  fi

  run "$QA/ocr-gate.sh" "$FIX/pair-pass.png"
  expect "O3 a frame with no readable text passes the gate" 0 "0 of 1"

  run env RICHOS_QA_TESSERACT=/usr/bin/true "$QA/ocr-gate.sh" "$FIX/ocr-control.png"
  expect "O4 a blind reader is caught by the positive control, never reported clean" 2 "POSITIVE CONTROL FAILED"

  mkdir -p "$TMP/emptydir"
  run "$QA/ocr-gate.sh" "$TMP/emptydir"
  expect "O5 an empty frame set is refused, never called clean" 2 "found NO frames"

  run "$QA/ocr-find.sh" 'FIXTURE CARD' "$FIX/ocr-control.png"
  expect "O6 the finder finds text that is there" 0 "HIT"

  run "$QA/ocr-find.sh" 'a string that is definitely not on this card' "$FIX/ocr-control.png"
  expect "O6b the finder EXITS NON-ZERO when the text never appears" 1 "0 of 1"
fi

echo ""
echo "=== R. redact: covered, and proved covered ==="

if [ "$HAVE_TESS" = 0 ]; then
  skip "R1-R4 the redaction cases" "no tesseract, so neither the finding nor the re-scan can run"
else
  run "$QA/redact.py" "$FIX/ocr-control.png" "$TMP/red.png"
  expect "R1 an address is found and covered, and the output re-read" 0 "no address-shaped string survives"

  LEFT="$("$TESS" "$TMP/red.png" stdout 2>/dev/null || true)"
  if printf '%s' "$LEFT" | grep -qi 'example.invalid'; then
    bad "R2 the address is actually gone from the output" "it is still legible"
  elif ! printf '%s' "$LEFT" | grep -qi 'nothing here is real'; then
    bad "R2 the rest of the evidence survives" \
        "over-redaction: the unrelated lines were destroyed too. Read: $(printf '%s' "$LEFT" | tr '\n' ' ' | cut -c1-160)"
  else
    ok "R2 the address is gone and the surrounding evidence survives"
  fi

  run "$QA/redact.py" "$FIX/ocr-control.png" "$TMP/red2.png" --no-verify --box 0,0,1,1
  expect "R3 --no-verify says outright that it proves nothing" 0 "proves the output is clean"

  # A box in the wrong place must not be reported as a redaction.
  run "$QA/redact.py" "$FIX/ocr-control.png" "$TMP/red3.png" --box 0,0,4,4
  expect "R4 a box that misses leaves the address legible and exits non-zero" 1 "STILL LEGIBLE"

  # The gate and the redactor must agree on what a phone number looks like: a
  # gate that flags a frame the redactor cannot clean leaves the walker with a
  # frame that can never ship, which is how hand-drawn boxes creep back in.
  run "$QA/ocr-gate.sh" "$FIX/phone-control.png"
  expect "R5 the gate flags the phone-shaped fixture (it is a real positive)" 1 "OCR GATE: 1 of 1"
  run "$QA/redact.py" "$FIX/phone-control.png" "$TMP/phone-red.png" --phones
  expect "R6 --phones covers the number and re-reads the output" 0 "no address-shaped or phone-shaped string survives"
  run "$QA/ocr-gate.sh" "$TMP/phone-red.png"
  expect "R7 the gate passes what the redactor produced" 0 "0 of 1"
  LEFT="$("$TESS" "$TMP/phone-red.png" stdout 2>/dev/null || true)"
  if printf '%s' "$LEFT" | grep -qi 'nothing here is real'; then
    ok "R8 the rest of the phone card survives the redaction"
  else
    bad "R8 the rest of the phone card survives the redaction" "over-redaction. Read: $(printf '%s' "$LEFT" | tr '\n' ' ' | cut -c1-160)"
  fi

  # The same agreement for a home path (the gate's second shape): the candidate 38 walk's
  # frames carried the guest's /Users/admin/... in attachment lines and work receipts.
  run "$QA/ocr-gate.sh" "$FIX/home-control.png"
  expect "R9 the gate flags the home-path fixture (it is a real positive)" 1 "OCR GATE: 1 of 1"
  run "$QA/redact.py" "$FIX/home-control.png" "$TMP/home-red.png" --homes
  expect "R10 --homes covers the path and re-reads the output" 0 "or home-path-shaped string survives"
  run "$QA/ocr-gate.sh" "$TMP/home-red.png"
  expect "R11 the gate passes what the redactor produced" 0 "0 of 1"
  LEFT="$("$TESS" "$TMP/home-red.png" stdout 2>/dev/null || true)"
  if printf '%s' "$LEFT" | grep -qi 'nothing here is real'; then
    ok "R12 the rest of the home card survives the redaction"
  else
    bad "R12 the rest of the home card survives the redaction" "over-redaction. Read: $(printf '%s' "$LEFT" | tr '\n' ' ' | cut -c1-160)"
  fi
fi

echo ""
echo "=== W. wait-for: and, above all, wait-for failing ==="

touch "$TMP/present"
run "$QA/wait-for.sh" --file "$TMP/present" --timeout 5
expect "W1 a file that is already there returns at once" 0 "appeared"

run "$QA/wait-for.sh" --file "$TMP/never" --timeout 2 --poll 1
expect "W2 a file that never appears is a FAILURE, not a quiet success" 1 "TIMEOUT"

printf 'boot\nready to serve\n' > "$TMP/a.log"
run "$QA/wait-for.sh" --log "$TMP/a.log" 'ready to serve' --timeout 5
expect "W3 a log line already present matches at once" 0 "matched after"

run "$QA/wait-for.sh" --log "$TMP/a.log" 'never logged this' --timeout 2 --poll 1
expect "W4 a log line that never arrives EXITS NON-ZERO" 1 "TIMEOUT"

# The fixture repository is built with PLUMBING, never `git commit`. This
# machine's global hooks refuse a commit whose identity is not the owner's, and
# a throwaway repository under mktemp has no business borrowing that identity —
# `commit-tree` and `update-ref` write the same objects with no hook in the way.
REPO="$TMP/repo"
mkdir -p "$REPO"
git -C "$REPO" init -q
EMPTY_TREE="$(git -C "$REPO" hash-object -t tree -w /dev/null)"
SHA1="$(git -C "$REPO" commit-tree "$EMPTY_TREE" -m one)"
SHA2="$(git -C "$REPO" commit-tree "$EMPTY_TREE" -p "$SHA1" -m two)"
git -C "$REPO" update-ref refs/heads/qa-fixture "$SHA2"
run "$QA/wait-for.sh" --ref "$REPO" qa-fixture --from "$SHA1" --timeout 5
expect "W5 a ref that has moved is reported with both SHAs" 0 "moved $SHA1"

run "$QA/wait-for.sh" --ref "$REPO" no-such-branch --timeout 300
expect "W6 a branch that does not exist is refused at once, not after the timeout" 2 "does not exist"

run "$QA/wait-for.sh" --gone "$TMP/never" --timeout 5
expect "W7 a path that is already gone returns at once" 0 "was gone"

mkdir -p "$TMP/held-workspace"
run "$QA/wait-for.sh" --gone "$TMP/held-workspace" --timeout 2 --poll 1
expect "W8 a directory that is never removed is a FAILURE, not a quiet success" 1 "still there"

echo ""
echo "=== T. timeline ==="

# A synthetic capture: nine frames on a clock, the region changing at frame 5.
python3 - "$QA/lib" "$TMP/tl" <<'PY'
import os
import sys
sys.path.insert(0, sys.argv[1])
import qaimg
out = sys.argv[2]
os.makedirs(out, exist_ok=True)
base = 1_700_000_000_000.0
rows = []
for n in range(9):
    color = (255, 255, 255) if n < 5 else (0, 90, 200)
    qaimg.save(qaimg.render_blocks(24, 12, color, color, (0, 0, 1, 1)),
               os.path.join(out, "%04d.png" % n))
    rows.append((n, base + n * 100.0))
with open(os.path.join(out, "meta.tsv"), "w") as fh:
    fh.write("action\tkey 36\n")
    fh.write("region\t0,0,24,12\n")
    fh.write("t_action_before\t%.1f\n" % (base + 250.0))
    fh.write("t_action_after\t%.1f\n" % (base + 260.0))
    for n, t in rows:
        fh.write("frame\t%d\t%.1f\n" % (n, t))
print("wrote 9 frames, change at 5")
PY

run "$QA/timeline.py" report "$TMP/tl"
if [ "$CODE" = 0 ] \
   && printf '%s' "$OUT" | grep -Fq "first change    : frame 0005" \
   && printf '%s' "$OUT" | grep -Fq "+250.0 ms"; then
  ok "T1 the first change is dated from the action, against the frame before it"
else
  bad "T1 the first change was mis-dated" "$(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-280)"
fi

cp -R "$TMP/tl" "$TMP/tl-flat"
for n in 5 6 7 8; do cp "$TMP/tl/0000.png" "$TMP/tl-flat/000$n.png"; done
run "$QA/timeline.py" report "$TMP/tl-flat"
expect "T2 a region that never changes is reported as NO CHANGE, not as a latency" 1 "NO CHANGE"

run "$QA/timeline.py" report "$TMP"
expect "T3 a directory with no meta.tsv is refused — those frames have no clock" 1 "no meta.tsv"

run "$QA/timeline.py" stats a=100 b=400 c=250 d=900 e=150
if [ "$CODE" = 0 ] \
   && printf '%s' "$OUT" | grep -Eq "min +100.0 ms" \
   && printf '%s' "$OUT" | grep -Eq "median +250.0 ms" \
   && printf '%s' "$OUT" | grep -Eq "max +900.0 ms"; then
  ok "T4 stats reports min, median and max over the samples given"
else
  bad "T4 stats computed the wrong summary" "$(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"
fi

run "$QA/timeline.py" stats a=100 b=400
expect "T5 fewer than three samples are not summarized as a median" 0 "never as a median"

run env -u RICHOS_QA_CAPTURE "$QA/timeline.py" capture "$TMP/cap" 1 --region 0,0,10,10 --wait-only
expect "T6 capture refuses to photograph this machine's screen" 1 "never a test surface"

run env -u RICHOS_QA_CAPTURE "$QA/ocr-watch.sh" "$TMP/watch" --region 0,0,10,10 --count 1
expect "T7 ocr-watch refuses to photograph this machine's screen" 2 "never a test surface"

# ocr-watch must FAIL (exit 2) when the capture or the reader fails; it used to turn both into
# empty text and report an unchanged screen, exit 0. Stand-ins first on PATH: no screen is read.
mkdir -p "$TMP/failbin"
printf '#!/bin/sh\nexit 7\n' > "$TMP/failbin/screencapture"
printf '#!/bin/sh\nexit 9\n' > "$TMP/failbin/tess-fail"
cat > "$TMP/failbin/screencapture-ok" <<'SH'
#!/bin/sh
for a in "$@"; do o="$a"; done
printf x > "$o"
SH
chmod +x "$TMP/failbin/screencapture" "$TMP/failbin/tess-fail" "$TMP/failbin/screencapture-ok"
run env PATH="$TMP/failbin:$PATH" RICHOS_QA_CAPTURE=allow RICHOS_QA_TESSERACT="$TMP/failbin/tess-fail" \
    "$QA/ocr-watch.sh" "$TMP/watchf" --region 0,0,10,10 --count 1 --stop-on-repeat 1
expect "T12 ocr-watch: a failed capture is a failure, not an unchanged screen" 2 "capture failed"
mkdir -p "$TMP/failbin2"
cp "$TMP/failbin/screencapture-ok" "$TMP/failbin2/screencapture"
run env PATH="$TMP/failbin2:$PATH" RICHOS_QA_CAPTURE=allow RICHOS_QA_TESSERACT="$TMP/failbin/tess-fail" \
    "$QA/ocr-watch.sh" "$TMP/watchg" --region 0,0,10,10 --count 1
expect "T13 ocr-watch: a failed text reader is a failure, not empty text" 2 "reader failed"

run "$QA/timeline.py" at "$TMP/tl" 0007.png
expect "T10 a frame chosen by reading is dated off the capture's own clock" 0 "+450.0 ms"

# The frame's path is checked, not just its number: capture B's 0007.png must not be dated
# against capture A's clock. A copy of the capture stands in for "another capture".
mkdir -p "$TMP/tl-other"
cp "$TMP/tl/0007.png" "$TMP/tl-other/0007.png" 2>/dev/null || : > "$TMP/tl-other/0007.png"
run "$QA/timeline.py" at "$TMP/tl" "$TMP/tl-other/0007.png"
expect "T14 a frame path from another capture is refused, not dated off this clock" 1 "another capture"
run "$QA/timeline.py" at "$TMP/tl" "$TMP/tl/0007.png"
expect "T15 a frame path inside the capture is still dated" 0 "+450.0 ms"

run "$QA/timeline.py" at "$TMP/tl" 99
expect "T11 a frame that clock does not cover is refused, not dated anyway" 1 "not in"

run "$QA/timeline.py" capture "$TMP/cap2" 1 --region 0,0,10,10 --also-region 1,2,3 --wait-only
expect "T8 --also-region without a full rectangle is refused, not silently dropped" 1 "takes a rectangle"

# Two regions, one action, one clock — driven against a FAKE `screencapture`
# first on PATH, so nothing photographs this machine. What is under test is the
# pair of calls `capture` BUILDS and the two meta.tsv files it writes; the
# guarantee that matters is that BOTH carry the SAME action instant, because
# two separate `capture` runs stamping their own is the defect this replaces.
mkdir -p "$TMP/fakebin"
cat > "$TMP/fakebin/screencapture" <<'SH'
#!/bin/sh
for a in "$@"; do case "$a" in -R*) echo "${a#-R}" >> "$TMPDIR_RECT" ;; esac; done
out=""; for a in "$@"; do out="$a"; done
printf 'x' > "$out"
SH
chmod +x "$TMP/fakebin/screencapture"
: > "$TMP/rects"
run env PATH="$TMP/fakebin:$PATH" TMPDIR_RECT="$TMP/rects" RICHOS_QA_CAPTURE=allow \
    "$QA/timeline.py" capture "$TMP/cap3" 1 --region 0,25,100,50 \
    --also-region 200,25,60,50 --wait-only --baseline 0.3 --interval 0.05
A_BEFORE="$(awk -F'\t' '$1=="t_action_before"{print $2}' "$TMP/cap3/meta.tsv" 2>/dev/null || true)"
B_BEFORE="$(awk -F'\t' '$1=="t_action_before"{print $2}' "$TMP/cap3/b/meta.tsv" 2>/dev/null || true)"
B_REGION="$(awk -F'\t' '$1=="region"{print $2}' "$TMP/cap3/b/meta.tsv" 2>/dev/null || true)"
if [ "$CODE" = 0 ] \
   && [ -n "$A_BEFORE" ] && [ "$A_BEFORE" = "$B_BEFORE" ] \
   && [ "$B_REGION" = "200,25,60,50" ] \
   && grep -Fq "200,25,60,50" "$TMP/rects" && grep -Fq "0,25,100,50" "$TMP/rects"; then
  ok "T9 --also-region captures the second rectangle on the SAME action instant"
else
  bad "T9 the second region was not captured against the same action" \
      "primary=$A_BEFORE also=$B_BEFORE region=$B_REGION code=$CODE"
fi

echo ""
run python3 "$QA/ocr-cache.test.py"
expect "OCR cache invalidation, reader failure, fresh control and multi-pattern timeline" 0 "OK"

run python3 "$QA/child-lifetime.test.py"
expect "A phone tool's child ends after kill -9 of the tool (outer limit 60 s)" 0 "OK"

run python3 "$QA/trust-reading.test.py"
expect "rios device trust reads a launch's log lines into the right cause" 0 "OK"

run python3 "$QA/device-launch.test.py"
expect "rios device launch: not connected = 0 launches; refused then ok = 2 launches 1 reboot; never a third" 0 "OK"

run python3 "$QA/lab-pause.test.py"
expect "lab-pause continues only a listener it stopped, never a pid it did not (R35)" 0 "OK"

run python3 "$QA/tunnel-requests.test.py"
expect "tunnel-requests: a reading without the request counter is a gap, never zero requests (R39)" 0 "OK"

run python3 "$QA/phone-ios-run.test.py"
expect "phone-ios run verdicts: exported evidence (N07)" 0 "OK"

run python3 "$QA/timeline-bounds.test.py"
expect "Monotonic visibility bounds and native input refusal" 0 "OK"

# The two tools for reproducing a loaded nightly (qa/README.md). What a caller relies on: the
# command runs, its own exit status comes back, and bad usage is refused rather than run. One
# worker and no warmup keep under-load.py from loading the Mac it is being tested on.
run python3 "$QA/stall-run.py" --run-ms 5 --stop-ms 5 -- sh -c 'echo stalled-child-ran; exit 7'
expect "S1 stall-run.py runs the command and hands back its own exit status" 7 "stalled-child-ran"
run python3 "$QA/stall-run.py" --run-ms 5
expect "S2 stall-run.py with no command is refused as bad usage" 2 "usage"
run python3 "$QA/under-load.py" --workers 1 --warmup 0 -- sh -c 'echo loaded-child-ran; exit 5'
expect "U1 under-load.py runs the command and hands back its own exit status" 5 "loaded-child-ran"
run python3 "$QA/under-load.py" --workers 1 --warmup 0
expect "U2 under-load.py with no command is refused as bad usage" 2 "usage"
# busy-sample.py fixes what reserve.py's admission sample reads, adding no load: the same
# admission refuses at the busy it was given and admits under the line.
run python3 "$QA/busy-sample.py" -- sh -c 'echo busy-child-ran; exit 6'
expect "B1 busy-sample.py runs the command and hands back its own exit status" 6 "busy-child-ran"
run python3 "$QA/busy-sample.py" --busy 100 -- python3 "$DIR/testvm/reserve.py" -- true
expect "B2 under busy-sample.py --busy 100 a reserve.py admission reads 100% and refuses" 75 "total CPU is 100.0%"
run python3 "$QA/busy-sample.py" --busy 10 -- python3 "$DIR/testvm/reserve.py" -- true
expect "B3 under busy-sample.py --busy 10 the same admission reads 10% and admits" 0 "user CPU 10.0%"
run python3 "$QA/busy-sample.py" --busy 100
expect "B4 busy-sample.py with no command is refused as bad usage" 2 "usage"
# slow-shutdown.py makes a program start its shutdown a known time after it is signaled (the
# extension browser suites' Chrome on a busy Mac, made certain). The delay check waits for the
# child to exist, so the signal never lands before the handlers are in place, and bounds the
# delay from below only: a loaded Mac can only make the shutdown later.
run python3 "$QA/slow-shutdown.py" --ms 0 -- sh -c 'echo slow-child-ran; exit 4'
expect "SL1 slow-shutdown.py runs the program and hands back its own exit status" 4 "slow-child-ran"
run env SLOW_SHUTDOWN_TARGET=/bin/sh SLOW_SHUTDOWN_MS=0 python3 "$QA/slow-shutdown.py" -c 'echo env-child-ran; exit 3'
expect "SL2 slow-shutdown.py takes the program from SLOW_SHUTDOWN_TARGET (the CHROME_PATH form)" 3 "env-child-ran"
run python3 "$QA/slow-shutdown.py"
expect "SL3 slow-shutdown.py with no program is refused as bad usage" 2 "usage"
run python3 -c '
import subprocess, sys, time
p = subprocess.Popen([sys.executable, sys.argv[1], "--ms", "300", "--", "sleep", "30"])
for _ in range(600):
    if subprocess.run(["pgrep", "-P", str(p.pid)], capture_output=True).returncode == 0:
        break
    time.sleep(0.05)
t = time.monotonic(); p.terminate(); rc = p.wait(); dt = time.monotonic() - t
print("OK" if rc == 143 and dt >= 0.29 else "BAD rc=%s after %.3f s" % (rc, dt))
' "$QA/slow-shutdown.py"
expect "SL4 slow-shutdown.py --ms 300 forwards SIGTERM no sooner than 300 ms and exits 128+15" 0 "OK"

echo "=== N. phone-client: the headless phone ==="

if ! command -v node >/dev/null 2>&1; then
  skip "N1-N5 phone-client" "node is not installed, and the tool is the production mobile client, which is JavaScript"
else
  # A stub Mac on loopback: it issues a challenge like the real listener, then answers every
  # signed request with the status it was started with. 404 is what a RichOS listener says to
  # a device it never paired; 200 is the defect the tool exists to catch.
  cat > "$TMP/stub-mac.mjs" <<'JS'
import { createServer } from 'node:http';
import { writeFileSync } from 'node:fs';
const [status, portFile] = process.argv.slice(2);
const server = createServer((req, res) => {
  if (req.url === '/api/challenge') { res.writeHead(404, { 'X-RichOS-Challenge': 'stub-challenge' }); res.end(); return; }
  res.writeHead(Number(status), { 'Content-Type': 'application/json' }); res.end(status === '200' ? '{"accepted":true}' : '');
});
server.listen(0, '127.0.0.1', () => writeFileSync(portFile, String(server.address().port)));
setTimeout(() => process.exit(0), 20000).unref?.();
JS
  node -e '
    const { generateKeyPairSync } = require("node:crypto");
    const { privateKey } = generateKeyPairSync("ec", { namedCurve: "prime256v1" });
    require("node:fs").writeFileSync(process.argv[1], JSON.stringify({ key: privateKey.export({ format: "jwk" }),
      disk: { schema: 2, confirmed: true, api: { apiBase: "https://vm.example", deviceId: "dev_stub" } } }));
  ' "$TMP/phone.json"
  STUB_PID=""
  stub_start() {  # stub_start <status>: sets STUB_PID and ORIGIN (never in a subshell, so the pid is ours)
    rm -f "$TMP/stub.port"
    node "$TMP/stub-mac.mjs" "$1" "$TMP/stub.port" &
    STUB_PID=$!
    for _ in $(seq 1 100); do [ -s "$TMP/stub.port" ] && break; sleep 0.05; done
    ORIGIN="http://127.0.0.1:$(cat "$TMP/stub.port" 2>/dev/null)"
  }
  stub_stop() { [ -n "$STUB_PID" ] && kill "$STUB_PID" 2>/dev/null; wait "$STUB_PID" 2>/dev/null; STUB_PID=""; }

  stub_start 404
  run node "$QA/phone-client.mjs" foreign --state "$TMP/phone.json" --origin "$ORIGIN"
  stub_stop
  expect "N1 a foreign Mac that refuses this phone's credential is the answer, exit 0" 0 '"ok":true'

  stub_start 200
  run node "$QA/phone-client.mjs" foreign --state "$TMP/phone.json" --origin "$ORIGIN"
  stub_stop
  expect "N2 a foreign Mac that ACCEPTS another Mac's phone EXITS NON-ZERO" 1 '"accepted":true'

  run node "$QA/phone-client.mjs" foreign --state "$TMP/phone.json" --origin "http://127.0.0.1:9"
  expect "N3 an unreachable origin is 'could not ask', never a clean isolation verdict" 2 "cannot reach"

  run node "$QA/phone-client.mjs" pair --state "$TMP/new-phone.json" --link "https://vm.example:8443/#pair=ABCDEFGH"
  expect "N4 pair without a decision file is refused before anything is sent" 2 "--decision"

  run node "$QA/phone-client.mjs" send --state "$TMP/absent.json" --text hello
  expect "N5 a send with no paired phone state is refused by name" 2 "no phone state"
fi

echo ""
echo "=== K. flake-rate: a failure rate counted the same way every time ==="

run "$QA/flake-rate.sh" --runs 3 --log-dir "$TMP/fr1" good 'true' broken 'false'
expect "K1 counts every run of every label and exits 0 — failures are the answer" 0 "broken: pass=0 fail=3 not-admitted=0 of 3"

run "$QA/flake-rate.sh" --runs 2 --log-dir "$TMP/fr2" a "echo a >> '$TMP/order'" b "echo b >> '$TMP/order'"
if [ "$(tr -d '\n' < "$TMP/order" 2>/dev/null)" = "abab" ]; then ok "K2 rounds are interleaved, so a before/after pair sees the same load"
else bad "K2 rounds are interleaved" "order was '$(tr -d '\n' < "$TMP/order" 2>/dev/null)', wanted abab"; fi

# Fails on its second run only: a real flake, and the failing log must survive with its words.
run "$QA/flake-rate.sh" --runs 3 --log-dir "$TMP/fr3" flaky "n=\$(cat '$TMP/count' 2>/dev/null || echo 0); n=\$((n+1)); echo \$n > '$TMP/count'; [ \$n -ne 2 ] || { echo 'second run broke'; exit 1; }"
if [ "$CODE" = 0 ] && printf '%s' "$OUT" | grep -Fq "flaky: pass=2 fail=1" && grep -Fq "second run broke" "$TMP/fr3/flaky-2.log" 2>/dev/null && [ ! -e "$TMP/fr3/flaky-1.log" ]; then
  ok "K3 a one-in-three failure is counted, its log kept and named, passing logs removed"
else bad "K3 a one-in-three failure is counted and kept" "exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"; fi

run "$QA/flake-rate.sh" --runs 0 x true
expect "K4 zero runs is refused, never reported as a clean rate" 2 "positive whole number"

run "$QA/flake-rate.sh" --runs 2 lonely
expect "K5 a label without its command is refused" 2 "LABEL"

echo ""
echo "=== A. phone-android: a physical phone, driven by what is on its screen ==="

# A scripted adb: it answers the handful of reads the tool makes and logs every
# input it is asked to inject. No phone, no emulator.
FAKE="$TMP/fake-adb"
cat > "$TMP/ui.xml" <<'XML'
<?xml version='1.0' encoding='UTF-8' standalone='yes' ?><hierarchy rotation="0"><node index="0" text="" content-desc="Message Rich" class="android.view.View" package="dev.richos.connect" bounds="[16,1412][704,1516]" focused="false" clickable="false" /><node index="1" text="" content-desc="Send message" class="android.view.View" package="dev.richos.connect" bounds="[604,828][700,924]" focused="false" clickable="true" /><node index="2" text="draft words" content-desc="" class="android.widget.EditText" package="dev.richos.connect" bounds="[16,1412][604,1516]" focused="true" clickable="true" /><node index="3" text="" content-desc="" class="android.view.View" package="dev.richos.connect" bounds="[0,0][40,40]" focused="false" clickable="true" /></hierarchy>
XML
cat > "$TMP/framestats.txt" <<'TXT'
Window: dev.richos.connect/dev.richos.android.app.MainActivity
Stats since: 1ns
Total frames rendered: 3
---PROFILEDATA---
Flags,FrameTimelineVsyncId,IntendedVsync,Vsync
0,1,1000000000,1000000000
0,2,1500000000,1500000000
0,3,2000000000,2000000000
---PROFILEDATA---
TXT
cat > "$FAKE" <<SH
#!/usr/bin/env bash
LOG="$TMP/adb.log"
[ "\$1" = devices ] && { printf 'List of devices attached\nFAKE123\tdevice\n'; exit 0; }
[ "\$1" = -s ] || exit 9
shift 2
case "\$1 \$2" in
  "exec-out cat") cat "$TMP/ui.xml"; exit 0 ;;
  "exec-out screencap") cat "$FIX/pair-pass.png"; exit 0 ;;
esac
cmd="\$2"
# FAKE_FAIL_READS: every read after the package and uptime ones fails, as on a phone whose adb
# shell is denied them. FAKE_FAIL_PACKAGE: the package dump itself fails.
if [ -n "\${FAKE_FAIL_PACKAGE:-}" ]; then
  case "\$cmd" in "dumpsys package "*) echo "permission denied" >&2; exit 1 ;; esac
fi
if [ -n "\${FAKE_FAIL_READS:-}" ]; then
  case "\$cmd" in
    "dumpsys package "*|"cat /proc/uptime"|"input "*|"getprop ro.kernel.qemu"*) ;;
    *) echo "permission denied" >&2; exit 1 ;;
  esac
fi
# FAKE_FAIL_DUMP: uiautomator dump fails (idle-state error) while the file from an earlier read is
# still on the phone, so the cat below would return a layout that is no longer on screen.
if [ -n "\${FAKE_FAIL_DUMP:-}" ]; then
  case "\$cmd" in "uiautomator dump "*) echo "ERROR: could not get idle state." >&2; exit 1 ;; esac
fi
case "\$cmd" in
  # The gate's probe (richos/mobile/physical.py): not an emulator, and the installed build's flags.
  "getprop ro.kernel.qemu"*) printf '\\n\\n--richos--\\n    versionName=1.0.0\\n    pkgFlags=[ %s ]\\n' "\${FAKE_PKG_FLAGS:-HAS_CODE ALLOW_CLEAR_USER_DATA}" ;;
  "input "*) echo "\$cmd" >> "\$LOG" ;;
  "dumpsys package "*) printf '    appId=10359\n    User 0: installed=true stopped=false\n' ;;
  "cat /proc/uptime") echo "1000.00 2000.00" ;;
  "am get-standby-bucket "*) echo 10 ;;
  "dumpsys power") printf 'mWakefulness=Awake\n  PARTIAL_WAKE_LOCK  "richos" ACQ=-1s (uid=10359 pid=42)\n' ;;
  "dumpsys window") printf 'isKeyguardShowing=false\n  mCurrentFocus=Window{1 u0 dev.richos.connect/dev.richos.android.app.MainActivity}\n' ;;
  "dumpsys battery") printf '  USB powered: true\n  level: 100\n' ;;
  "ps -A -o PID,NAME") printf 'PID NAME\n42 dev.richos.connect\n' ;;
  "cat /proc/42/stat") echo "42 (.richos.connect) S 1 1 0 0 -1 0 0 0 0 0 100 50 0 0 20 0 50 0 \${FAKE_BIRTH:-12345} 0 0" ;;
  "for t in /proc/42/task"*) printf '42|main|5 3\n43|RenderThread|1 0\n' ;;
  "dumpsys activity activities") echo "  topResumedActivity=ActivityRecord{1 u0 dev.richos.connect/.MainActivity t1}" ;;
  "dumpsys activity services "*) echo "  * ServiceRecord{abc u0 dev.richos.connect/dev.richos.android.platform.RichMessagingService}" ;;
  "cat /proc/net/"*) printf '  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid\n   0: 0100007F:1F90 0100007F:0050 01 00000000:00000000 00:00000000 00000000 10359 0 1\n' ;;
  "dumpsys notification") echo "  NotificationRecord(0x1: pkg=dev.richos.connect user=UserHandle{0} id=1)" ;;
  "dumpsys gfxinfo "*framestats) cat "$TMP/framestats.txt" ;;
  *) : ;;
esac
exit 0
SH
chmod +x "$FAKE"
PA="$QA/phone-android.py"
# randroid device starts phone-android.py with this set; without it the tool refuses (A20).
export RICHOS_DEVICE_VERB=randroid

run env -u RICHOS_DEVICE_VERB "$PA" --adb "$FAKE" --serial FAKE123 texts
expect "A22 started without randroid device, it refuses before touching the phone" 3 "only through \`randroid device"
: > "$TMP/adb.log"
run env FAKE_PKG_FLAGS="DEBUGGABLE HAS_CODE" "$PA" --adb "$FAKE" --serial FAKE123 tap "Send message" --timeout 1
if [ "$CODE" = 3 ] && printf '%s' "$OUT" | grep -q "DEBUGGABLE RichConnect" && [ ! -s "$TMP/adb.log" ]; then
  ok "A23 a phone holding a debuggable RichConnect is refused, nothing tapped: only the release build is tested"
else bad "A23 a debuggable build is refused" "exit $CODE, log: $(tr '\n' ' ' < "$TMP/adb.log"): $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"; fi

run "$PA" texts
expect "A1 no --serial is refused: a bare adb reaches whatever is attached" 2 "name the phone with --serial"
run "$PA" --adb "$FAKE" --serial NOTHERE texts
expect "A2 a serial adb does not list is refused, never guessed" 2 "is not an attached"
: > "$TMP/adb.log"
run "$PA" --adb "$FAKE" --serial FAKE123 tap "Send message" --timeout 1
if [ "$CODE" = 0 ] && grep -Fxq "input tap 652 876" "$TMP/adb.log"; then
  ok "A3 tap finds the control by its words and taps its center"
else bad "A3 tap finds the control by its words and taps its center" "exit $CODE, log: $(tr '\n' ' ' < "$TMP/adb.log")"; fi
run "$PA" --adb "$FAKE" --serial FAKE123 wait "Nowhere on this screen" --timeout 1
expect "A4 a wait that runs out is a failure, exit 1" 1 '"missing"'
printf "it's" > "$TMP/quote.txt"
run "$PA" --adb "$FAKE" --serial FAKE123 type-file "$TMP/quote.txt"
expect "A5 a character that input text cannot carry exactly is refused before typing" 2 "type-file types letters"
printf 'ab c' > "$TMP/abc.txt"; : > "$TMP/adb.log"
run "$PA" --adb "$FAKE" --serial FAKE123 type-file "$TMP/abc.txt"
if [ "$CODE" = 0 ] && [ "$(grep -c '^input text' "$TMP/adb.log")" = 4 ] && grep -Fxq "input text %s" "$TMP/adb.log"; then
  ok "A6 type-file sends one input per character, a space as %s"
else bad "A6 type-file sends one input per character, a space as %s" "exit $CODE, log: $(tr '\n' ' ' < "$TMP/adb.log")"; fi
# A read that FAILED is null with its reason, never an empty list, a zero or a false: "no
# processes, no foreground service, microphone not running" would otherwise read as a clean closure.
run env FAKE_FAIL_READS=1 "$PA" --adb "$FAKE" --serial FAKE123 state --package dev.richos.connect --out "$TMP/s-fail.json"
if [ "$CODE" = 0 ] && python3 - "$TMP/s-fail.json" <<'PY'
import json, sys
s = json.load(open(sys.argv[1]))
for key in ("processes", "services", "foregroundServices", "wakeLocks", "microphoneRunning", "microphoneAppOp",
            "jobs", "pendingAlarms", "ownNotifications", "resumedActivity", "recentRecordingEvents", "charger"):
    assert s[key] is None, (key, s[key])
assert "processes" in s["unreadable"] and "microphone" in s["unreadable"] and "permission denied" in s["unreadable"]["jobs"], s.get("unreadable")
PY
then ok "A18 state: reads that failed are null with the reason, never empty, zero or false"
else bad "A18 state: reads that failed are null with the reason" "exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"; fi
run env FAKE_FAIL_PACKAGE=1 "$PA" --adb "$FAKE" --serial FAKE123 state --package dev.richos.connect
expect "A19 state: a package dump that failed cannot answer, it is not 'not installed'" 2 "cannot tell whether"
run "$PA" --adb "$FAKE" --serial FAKE123 state --package dev.richos.connect --out "$TMP/s1.json"
if [ "$CODE" = 0 ] && python3 - "$TMP/s1.json" <<'PY'
import json, sys
s = json.load(open(sys.argv[1]))
p = s["processes"][0]
assert s["uid"] == 10359 and s["stoppedFlag"] is False and s["standbyBucket"] == "10", s
assert p["pid"] == 42 and p["cpuTicks"] == 150 and p["birth"] == "12345" and p["threadCount"] == 2, p
assert s["resumedActivity"] and s["appHasFocus"] and s["screen"] == "Awake" and s["keyguardShowing"] is False
assert s["services"] == ["dev.richos.connect/dev.richos.android.platform.RichMessagingService"], s["services"]
assert len(s["wakeLocks"]) == 1 and s["openSockets"] == {"ESTABLISHED": 1} and s["ownNotifications"] == 1, s
PY
then ok "A7 state reads process, birth, ticks, services, wake locks, sockets and notifications"
else bad "A7 state reads process, birth, ticks, services, wake locks, sockets and notifications" "exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"; fi
run env FAKE_BIRTH=12345 "$PA" --adb "$FAKE" --serial FAKE123 state --package dev.richos.connect --out "$TMP/s2.json"
run "$PA" compare "$TMP/s1.json" "$TMP/s2.json"
expect "A8 compare subtracts CPU for the same process" 0 '"cpuTicks": 0'
run env FAKE_BIRTH=99999 "$PA" --adb "$FAKE" --serial FAKE123 state --package dev.richos.connect --out "$TMP/s3.json"
run "$PA" compare "$TMP/s1.json" "$TMP/s3.json"
expect "A9 a restarted process (new birth) is never subtracted from the old one" 0 '"sameProcess": []'
run "$PA" --adb "$FAKE" --serial FAKE123 idle-frames --package dev.richos.connect --seconds 0.1
if [ "$CODE" = 0 ] && printf '%s' "$OUT" | grep -q '"totalFramesRendered": 3' && printf '%s' "$OUT" | tr -d ' \n' | grep -q '"gapsMs":\[500.0,500.0\]'; then
  ok "A10 idle-frames counts the frames and reports their rhythm"
else bad "A10 idle-frames counts the frames and reports their rhythm" "exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"; fi
run "$PA" --adb "$FAKE" --serial FAKE123 field
if [ "$CODE" = 0 ] && printf '%s' "$OUT" | grep -q '"text": "draft words"'; then ok "A11 field reads the editable text back"
else bad "A11 field reads the editable text back" "exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"; fi
printf '{"1":[40,1105]," ":[395,1470]}' > "$TMP/keymap.json"
printf '1 x' > "$TMP/keys-bad.txt"
run "$PA" --adb "$FAKE" --serial FAKE123 keys "$TMP/keys-bad.txt" --keymap "$TMP/keymap.json"
expect "A12 keys refuses a character the key map cannot type, before tapping anything" 2 "no key for"
printf '1 1' > "$TMP/keys-ok.txt"; : > "$TMP/adb.log"
run "$PA" --adb "$FAKE" --serial FAKE123 keys "$TMP/keys-ok.txt" --keymap "$TMP/keymap.json"
if [ "$CODE" = 0 ] && [ "$(grep -c 'input tap 40 1105' "$TMP/adb.log")" = 2 ] && grep -Fxq "input tap 395 1470" "$TMP/adb.log"; then
  ok "A13 keys taps the mapped key for every character"
else bad "A13 keys taps the mapped key for every character" "exit $CODE, log: $(tr '\n' ' ' < "$TMP/adb.log")"; fi
: > "$TMP/adb.log"
run "$PA" --adb "$FAKE" --serial FAKE123 observe --package dev.richos.connect --label cell --out-dir "$TMP/obs" --settle 0 --seconds 0 --action home
if [ "$CODE" = 0 ] && grep -Fxq "input keyevent KEYCODE_HOME" "$TMP/adb.log" && [ -f "$TMP/obs/cell-start.json" ] && [ -f "$TMP/obs/cell-end.json" ] && [ -f "$TMP/obs/cell-compare.json" ]; then
  ok "A14 observe performs the closure action and keeps both readings and the comparison"
else bad "A14 observe performs the closure action and keeps both readings and the comparison" "exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"; fi
run "$PA" --adb "$FAKE" --serial FAKE123 observe --package dev.richos.connect --label x --out-dir "$TMP/obs" --settle 0 --seconds 0 --action reboot
expect "A15 observe refuses an action it does not know rather than guessing" 2 "--action must be one of"
run "$PA" --adb "$FAKE" --serial FAKE123 unlabeled --package dev.richos.connect
expect "A16 unlabeled finds the one clickable control with no name and exits 1" 1 '"bounds": [
        0,'
run "$PA" --adb "$FAKE" --serial FAKE123 unlabeled --package com.example.other
expect "A17 unlabeled is clean for a package with no unnamed controls" 0 '"unnamed": []'
run env FAKE_FAIL_DUMP=1 "$PA" --adb "$FAKE" --serial FAKE123 texts
expect "A20 a failed fresh dump never returns the layout left by an earlier read" 2 "the screen could not be read"

echo ""
echo "=== L. lab-ledger: exactly once, word for word ==="
mkdir -p "$TMP/lab"
printf 'richos-mobile-isolated-v1' > "$TMP/lab/lab-owner"
cat > "$TMP/lab/timeline.json" <<'JSON'
{"items":[{"kind":"user_message","text":"one"},{"kind":"rich_message","text":"ack: one"},{"kind":"user_message","text":"two"},{"kind":"user_message","text":"two"},{"kind":"rich_message","text":"ack: two"}]}
JSON
run "$QA/lab-ledger.py" --timeline "$TMP/lab/timeline.json" one
expect "L1 a message and its reply there exactly once pass" 0 '"exactlyOnce": 1'
run "$QA/lab-ledger.py" --timeline "$TMP/lab/timeline.json" two
expect "L2 a duplicated message FAILS, exit 1" 1 '"userRows": 2'
run "$QA/lab-ledger.py" --timeline "$TMP/lab/timeline.json" "one "
expect "L3 an altered message (one character) FAILS, exit 1" 1 '"userRows": 0'
mkdir -p "$TMP/notlab"; cp "$TMP/lab/timeline.json" "$TMP/notlab/timeline.json"
run "$QA/lab-ledger.py" --timeline "$TMP/notlab/timeline.json" one
expect "L4 a timeline that is not the isolated lab's is refused, never read" 2 "owner marker"
echo "=== P. lab-pause: the isolated lab Mac held silent between two phone marks ==="
# A stand-in listener: a python sleeper whose command line carries the listener's test name and
# whose environment carries this lab's RICHOS_MOBILE_MAC_DIR (as mac-server.mjs gives the real one),
# the two things the tool checks. (Not /bin/sleep: a system binary hides its environment from ps.)
# It is this suite's own child, stopped here.
mkdir -p "$TMP/lp/data"
printf 'richos-mobile-isolated-v1' > "$TMP/lp/data/lab-owner"
RICHOS_MOBILE_MAC_DIR="$TMP/lp/data" python3 -c 'import time; time.sleep(60)' 'mobile_mac_server::serve' &
LPPID=$!
mkdir -p "$TMP/lp-foreign/data"
RICHOS_MOBILE_MAC_DIR="$TMP/lp-foreign/data" python3 -c 'import time; time.sleep(60)' 'mobile_mac_server::serve' &
LPFOREIGN=$!
# load-bound: a fresh child shows its environment to ps only once it has finished starting
for LPWHO in "$LPPID" "$LPFOREIGN"; do
  n=0; until ps -E -p "$LPWHO" -o command= | grep -q 'RICHOS_MOBILE_MAC_DIR=' || [ "$n" -ge 200 ]; do sleep 0.05; n=$((n + 1)); done
done
printf '{"pid": %s, "data": "%s"}' "$LPPID" "$TMP/lp/data" > "$TMP/lp/mac.json"
: > "$TMP/lp/run-test.log"
( echo 'PHONE_STEP {"detail":{"label":"PAUSE MAC 1"}}' >> "$TMP/lp/run-test.log"
  n=0
  # load-bound: polls for the stopped state itself; the 200 polls are only a hang guard
  until case "$(ps -p "$LPPID" -o stat= | tr -d ' ')" in T*) true ;; *) false ;; esac || [ "$n" -ge 200 ]; do sleep 0.05; n=$((n + 1)); done
  ps -p "$LPPID" -o stat= > "$TMP/lp/held"
  echo 'PHONE_STEP {"detail":{"label":"hidden 1"}}' >> "$TMP/lp/run-test.log" ) &
run python3 "$QA/lab-pause.py" --lab "$TMP/lp" --log "$TMP/lp/run-test.log" --stop-at 'PAUSE MAC {n}' --cont-at 'hidden {n}' --timeout 20
HELD="$(tr -d ' \n' < "$TMP/lp/held" 2>/dev/null)"; NOW="$(ps -p "$LPPID" -o stat= | tr -d ' ')"
if [ "$CODE" = 0 ] && printf '%s' "$OUT" | grep -q '"continued"' && case "$HELD" in T*) true ;; *) false ;; esac \
   && case "$NOW" in T*) false ;; *) true ;; esac; then
  ok "LP1 the listener is stopped at the first mark (state $HELD) and running again after the second (state $NOW)"
else
  bad "LP1 the listener is stopped at the first mark and running again after the second" "exit $CODE, held '$HELD', now '$NOW': $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"
fi
mkdir -p "$TMP/lp-notlab/data"; sed "s#$TMP/lp/data#$TMP/lp-notlab/data#" "$TMP/lp/mac.json" > "$TMP/lp-notlab/mac.json"
run python3 "$QA/lab-pause.py" --lab "$TMP/lp-notlab" --log "$TMP/lp/run-test.log" --stop-at x --cont-at y --timeout 2
expect "LP2 a cache without the isolated lab's owner marker is refused: nothing is paused" 2 "no isolated lab owner marker"
printf '{"pid": %s, "data": "%s"}' "$$" "$TMP/lp/data" > "$TMP/lp/other.json"
mkdir -p "$TMP/lp-other"; cp "$TMP/lp/other.json" "$TMP/lp-other/mac.json"
run python3 "$QA/lab-pause.py" --lab "$TMP/lp-other" --log "$TMP/lp/run-test.log" --stop-at x --cont-at y --timeout 2
expect "LP3 a pid that is not the lab's listener is refused, never paused" 2 "is not the lab's listener"
# R35: a listener of ANOTHER lab (same command name) now holds the pid the cache recorded.
mkdir -p "$TMP/lp-stale"; printf '{"pid": %s, "data": "%s"}' "$LPFOREIGN" "$TMP/lp/data" > "$TMP/lp-stale/mac.json"
run python3 "$QA/lab-pause.py" --lab "$TMP/lp-stale" --log "$TMP/lp/run-test.log" --stop-at x --cont-at y --timeout 2
FSTAT="$(ps -p "$LPFOREIGN" -o stat= | tr -d ' ')"
if [ "$CODE" = 2 ] && printf '%s' "$OUT" | grep -q 'listener of another lab' && case "$FSTAT" in T*) false ;; *) true ;; esac; then
  ok "LP5 a pid that is another lab's listener (same command name, other data directory) is refused and never paused"
else
  bad "LP5 another lab's listener is refused, never paused" "exit $CODE, state '$FSTAT': $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"
fi
kill "$LPFOREIGN" 2>/dev/null; wait "$LPFOREIGN" 2>/dev/null
echo 'PHONE_STEP {"detail":{"label":"PAUSE MAC 2"}}' >> "$TMP/lp/run-test.log"
run python3 "$QA/lab-pause.py" --lab "$TMP/lp" --log "$TMP/lp/run-test.log" --stop-at 'PAUSE MAC 2' --cont-at 'never written' --timeout 3
NOW="$(ps -p "$LPPID" -o stat= | tr -d ' ')"
if [ "$CODE" = 1 ] && printf '%s' "$OUT" | grep -q 'continued anyway' && case "$NOW" in T*) false ;; *) true ;; esac; then
  ok "LP4 a continue mark that never comes times out, exit 1, and the listener runs again (state $NOW)"
else
  bad "LP4 a continue mark that never comes times out and the listener runs again" "exit $CODE, now '$NOW': $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"
fi
kill "$LPPID" 2>/dev/null; wait "$LPPID" 2>/dev/null

echo ""
echo "=== S. step-mark and hidden-send-try: a physical Android phone's send kept in flight across Home ==="
run python3 "$QA/step-mark.py" "$TMP/steps.log" PAUSE MAC 1
if [ "$CODE" = 0 ] && grep -Eq '^PAUSE MAC 1 [0-9]+\.[0-9]{3}$' "$TMP/steps.log"; then
  ok "SM1 step-mark appends one line: the words, then the epoch seconds on this Mac's clock"
else
  bad "SM1 step-mark appends one line" "exit $CODE, log: $(tr '\n' ' ' < "$TMP/steps.log" 2>/dev/null)"
fi
run python3 "$QA/step-mark.py" "$TMP/steps.log"
expect "SM2 a mark with no words is refused, nothing appended" 2 "step-mark.py FILE WORDS"
# A scripted adb on PATH: it records each call and can be told to fail one.
mkdir -p "$TMP/fakebin"
cat > "$TMP/fakebin/adb" <<'FAKE'
#!/bin/sh
case "$*" in *"getprop ro.kernel.qemu"*) printf '\n\n--richos--\n    pkgFlags=[ %s ]\n' "${FAKE_PKG_FLAGS:-HAS_CODE}"; exit 0 ;; esac
echo "$*" >> "$FAKE_ADB_LOG"
[ -n "$FAKE_ADB_FAIL" ] && case "$*" in *"$FAKE_ADB_FAIL"*) echo "scripted failure" >&2; exit 1 ;; esac
exit 0
FAKE
chmod +x "$TMP/fakebin/adb"
: > "$TMP/hs.log"; : > "$TMP/hs-adb.log"
FAKE_ADB_LOG="$TMP/hs-adb.log" PATH="$TMP/fakebin:$PATH" run python3 "$QA/hidden-send-try.py" --serial TESTSERIAL --log "$TMP/hs.log" --n 2 --send 652,896 --fault wifi --fault-after 0 --restore-after 0
ORDER="$(awk '{printf "%s ", $1 ($1=="WIFI"||$1=="RELEASE"?"-"$2:"")}' "$TMP/hs.log")"
CALLS="$(sed -e 's/^-s TESTSERIAL shell //' "$TMP/hs-adb.log" | tr '\n' ';')"
if [ "$CODE" = 0 ] && [ "$ORDER" = "PAUSE HOME WIFI-OFF RELEASE-MAC WIFI-ON " ] \
   && [ "$CALLS" = "input tap 652 896;input keyevent KEYCODE_HOME;svc wifi disable;svc wifi enable;" ]; then
  ok "HS1 one try: PAUSE, tap Send, Home, Wi-Fi off, RELEASE, Wi-Fi on, in that order, power key never used"
else
  bad "HS1 one try runs its steps in order" "exit $CODE, marks '$ORDER', adb '$CALLS'"
fi
: > "$TMP/hs.log"; : > "$TMP/hs-adb.log"
FAKE_ADB_FAIL="wifi disable" FAKE_ADB_LOG="$TMP/hs-adb.log" PATH="$TMP/fakebin:$PATH" run python3 "$QA/hidden-send-try.py" --serial TESTSERIAL --log "$TMP/hs.log" --n 1 --send 652,896 --fault wifi --fault-after 0 --restore-after 0
if [ "$CODE" != 0 ] && [ "$(tail -n 1 "$TMP/hs-adb.log")" = "-s TESTSERIAL shell svc wifi enable" ]; then
  ok "HS2 a Wi-Fi switch that failed half way is still put back on: the last adb call is the enable"
else
  bad "HS2 Wi-Fi is put back on however the try ends" "exit $CODE, last adb call '$(tail -n 1 "$TMP/hs-adb.log")'"
fi
: > "$TMP/hs.log"; : > "$TMP/hs-adb.log"
FAKE_PKG_FLAGS="DEBUGGABLE HAS_CODE" FAKE_ADB_LOG="$TMP/hs-adb.log" PATH="$TMP/fakebin:$PATH" run python3 "$QA/hidden-send-try.py" --serial TESTSERIAL --log "$TMP/hs.log" --n 1 --send 652,896
if [ "$CODE" = 3 ] && [ ! -s "$TMP/hs-adb.log" ] && [ ! -s "$TMP/hs.log" ]; then
  ok "HS6 a debuggable RichConnect on the phone is refused before the first mark or tap"
else bad "HS6 a debuggable build is refused before anything" "exit $CODE, adb '$(tr '\n' ';' < "$TMP/hs-adb.log")': $(printf '%s' "$OUT" | cut -c1-200)"; fi
FAKE_ADB_LOG="$TMP/hs-adb.log" PATH="$TMP/fakebin:$PATH" run env -u RICHOS_DEVICE_VERB python3 "$QA/hidden-send-try.py" --serial TESTSERIAL --log "$TMP/hs.log" --n 1 --send 652,896
expect "HS7 started without randroid device, it refuses" 3 "only through \`randroid device"
# R39: an unreadable socket table is recorded as unreadable, never as "none".
: > "$TMP/hs.log"; : > "$TMP/hs-adb.log"; rm -f "$TMP/hs-sock.txt"
cat > "$TMP/fakebin/adb" <<'FAKE'
#!/bin/sh
case "$*" in *"getprop ro.kernel.qemu"*) printf '\n\n--richos--\n    pkgFlags=[ HAS_CODE ]\n'; exit 0 ;; esac
case "$*" in *"cat /proc/net"*) [ -n "$FAKE_PROC_DENIED" ] && { echo "Permission denied" >&2; exit 1; }
  echo "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode"
  echo "   0: 00000000:0000 00000000:01BB 01 0:0 00:0 0 10001 0 0"; exit 0 ;; esac
echo "$*" >> "$FAKE_ADB_LOG"
exit 0
FAKE
FAKE_PROC_DENIED=1 FAKE_ADB_LOG="$TMP/hs-adb.log" PATH="$TMP/fakebin:$PATH" run python3 "$QA/hidden-send-try.py" --serial TESTSERIAL --log "$TMP/hs.log" --n 1 --send 1,2 --sockets-uid 10001 --sockets-file "$TMP/hs-sock.txt" --observe 1
if [ "$CODE" = 0 ] && grep -q ' unreadable Permission denied' "$TMP/hs-sock.txt" && ! grep -q ' none$' "$TMP/hs-sock.txt"; then
  ok "HS4 a denied /proc/net read is recorded 'unreadable Permission denied', never 'none'"
else
  bad "HS4 an unreadable socket table is not 'none'" "exit $CODE: $(tr '\n' ';' < "$TMP/hs-sock.txt" 2>/dev/null | cut -c1-200)"
fi
rm -f "$TMP/hs-sock.txt"
FAKE_ADB_LOG="$TMP/hs-adb.log" PATH="$TMP/fakebin:$PATH" run python3 "$QA/hidden-send-try.py" --serial TESTSERIAL --log "$TMP/hs.log" --n 1 --send 1,2 --sockets-uid 10001 --sockets-file "$TMP/hs-sock.txt" --observe 1
if [ "$CODE" = 0 ] && grep -q ' ESTABLISHED:01BB$' "$TMP/hs-sock.txt"; then
  ok "HS5 a readable table still records the app's socket (control)"
else
  bad "HS5 a readable socket table is recorded" "exit $CODE: $(tr '\n' ';' < "$TMP/hs-sock.txt" 2>/dev/null | cut -c1-200)"
fi
run python3 "$QA/hidden-send-try.py" --serial TESTSERIAL --log "$TMP/hs.log" --n 1 --send nonsense
expect "HS3 a Send position that is not X,Y is refused before any adb call" 2 "X,Y integers"
HSC="$(python3 - "$QA/hidden-send-try.py" <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("hst", sys.argv[1]); m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
print(m.counter("cloudflared_tunnel_total_requests 7"), m.counter('cloudflared_tunnel_response_by_code{status_code="200"} 7'))
PY
)"
if [ "$HSC" = "total_requests=7 response_by_code[200]=7" ]; then
  ok "HS4 an unlabeled and a labeled counter both write name=value, the name alone left of the ="
else
  bad "HS4 counter lines parse as name=value" "got '$HSC'"
fi

echo ""
echo "=== TR. tunnel-requests: the lab tunnel's request counter, sampled on this Mac and counted between two instants ==="
# A scripted tunnel helper: /metrics answers from a file the case rewrites, so the counter rises on cue.
cat > "$TMP/tr-metrics.txt" <<'M'
# HELP cloudflared_tunnel_total_requests Amount of requests proxied through all the tunnels
cloudflared_tunnel_total_requests 3
cloudflared_tunnel_request_errors 0
cloudflared_tunnel_response_by_code{status_code="200"} 3
M
cat > "$TMP/tr-server.py" <<'PY'
import http.server, sys
body_file, port_file = sys.argv[1], sys.argv[2]
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        data = open(body_file, "rb").read() if self.path == "/metrics" else b""
        self.send_response(200 if self.path == "/metrics" else 404); self.end_headers(); self.wfile.write(data)
    def log_message(self, *a): pass
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(port_file, "w").write(str(s.server_address[1]))
s.serve_forever()
PY
python3 "$TMP/tr-server.py" "$TMP/tr-metrics.txt" "$TMP/tr-port" & TRPID=$!
for _ in $(seq 50); do [ -s "$TMP/tr-port" ] && break; sleep 0.1; done
TRPORT="$(cat "$TMP/tr-port" 2>/dev/null)"
python3 "$QA/tunnel-requests.py" sample --port "$TRPORT" --out "$TMP/tr.log" --seconds 3 --interval 0.1 & TRS=$!
sleep 0.8; T1="$(python3 -c 'import time; print("%.3f" % time.time())')"; sleep 0.3
sed -i '' -e 's/total_requests 3/total_requests 4/' -e 's/"200"} 3/"200"} 4/' "$TMP/tr-metrics.txt"; sleep 0.5
sed -i '' -e 's/total_requests 4/total_requests 5/' -e 's/"200"} 4/"200"} 5/' "$TMP/tr-metrics.txt"; sleep 0.5
T2="$(python3 -c 'import time; print("%.3f" % time.time())')"
wait "$TRS"; CODE=$?
run python3 "$QA/tunnel-requests.py" between "$TMP/tr.log" --from "$T1" --to "$T2"
if [ "$CODE" = 0 ] && printf '%s' "$OUT" | grep -q '"requests": 2' && [ "$(printf '%s' "$OUT" | grep -c '"by"')" = 2 ] \
   && grep -Eq '^[0-9]+\.[0-9]{3} request_errors=0 response_by_code\[200\]=3 total_requests=3$' "$TMP/tr.log"; then
  ok "TR1 two requests that crossed the tunnel between two instants are counted as two rises, each bracketed by its readings"
else
  bad "TR1 two requests between two instants are counted" "exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"
fi
printf '%s unreadable connection refused\n' "$(python3 -c 'import time; print("%.3f" % (time.time() - 0.05))')" >> "$TMP/tr.log"
printf '%s total_requests=5\n' "$(python3 -c 'import time; print("%.3f" % (time.time() + 1))')" >> "$TMP/tr.log"
run python3 "$QA/tunnel-requests.py" between "$TMP/tr.log" --from "$T1" --to "$(python3 -c 'import time; print("%.3f" % (time.time() + 0.5))')"
expect "TR2 an unreadable reading inside the window is refused: a gap is not a zero" 2 "a gap is not a zero"
run python3 "$QA/tunnel-requests.py" between "$TMP/tr.log" --from 1000 --to 2000
expect "TR3 a window the readings do not cover is refused, no count printed" 2 "do not cover"
printf 'not metrics\n' > "$TMP/tr-metrics.txt"
run python3 "$QA/tunnel-requests.py" sample --port "$TRPORT" --out "$TMP/tr-wrong.log" --seconds 1
if [ "$CODE" = 2 ] && printf '%s' "$OUT" | grep -q 'not a tunnel helper' && [ ! -e "$TMP/tr-wrong.log" ]; then
  ok "TR4 a port that answers without the tunnel's request counter is refused before anything is written"
else
  bad "TR4 a port without the tunnel counter is refused" "exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-200)"
fi
kill "$TRPID" 2>/dev/null; wait "$TRPID" 2>/dev/null

echo "=== I. phone-ios: a physical iPhone's real controls, validated before any build ==="
run env -u RICHOS_DEVICE_VERB python3 "$QA/phone-ios.py" procs --device x
expect "I0 a command that touches the phone, started without rios device, is refused" 3 "rios device procs"
# rios device starts phone-ios.py with this set (bin/rios); check, summary and the readers need none.
export RICHOS_DEVICE_VERB=rios

printf '%s' '[{"do":"launch"},{"do":"tap","id":"composer.send"},{"do":"sleep","seconds":2},{"do":"alert","button":"Allow","in":"springboard"}]' > "$TMP/ios-ok.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-ok.json"
expect "I1 a well-formed step list validates without a device" 0 '"steps": 4'

printf '%s' '[{"do":"launch"},{"do":"reboot"}]' > "$TMP/ios-unknown.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-unknown.json"
expect "I2 an unknown step is refused by name before anything is built" 2 "unknown step 'reboot'"

printf '%s' '[{"do":"tap"}]' > "$TMP/ios-notarget.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-notarget.json"
expect "I3 a tap that names no element is refused" 2 "names no id or label"

printf '%s' '[{"do":"wait","id":"pair.link","timout":5}]' > "$TMP/ios-typo.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-typo.json"
expect "I4 a misspelled key is refused, never silently ignored" 2 "unexpected keys ['timout']"

printf '%s' '[{"do":"activate","in":"tailscale"},{"do":"value","kind":"switch","in":"tailscale"},{"do":"shot","screen":true}]' > "$TMP/ios-account.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-account.json"
expect "I8 a list that touches the person's Tailscale account may keep no picture of it" 2 "may not keep a shot, tree or audit"

printf '%s' '[{"do":"activate","in":"tailscale"},{"do":"value","kind":"switch","in":"tailscale"}]' > "$TMP/ios-switch.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-switch.json"
expect "I9 reading the route's switch by kind alone validates" 0 '"steps": 2'

printf '%s' '[{"do":"activate","in":"settings"},{"do":"tap","label":"Wi-Fi","in":"settings"},{"do":"tree","name":"s","in":"settings"},{"do":"open","url":"https://example.com/i.png"},{"do":"press","label":"i.png","in":"safari"},{"do":"shot","name":"x","in":"safari"}]' > "$TMP/ios-places.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-places.json"
expect "I31 Settings and Safari (the share-sheet source) and an https open validate" 0 '"steps": 6'

printf '%s' '[{"do":"open","url":"tel:5550100"}]' > "$TMP/ios-scheme.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-scheme.json"
expect "I32 open refuses anything but an https link: no other app's own scheme" 2 "needs an https url"

printf '%s' '[{"do":"launch","in":"settings"},{"do":"tap","kind":"switch","label":"Camera","in":"settings"},{"do":"value","kind":"switch","label":"Camera","in":"settings","equals":"1"},{"do":"terminate","in":"settings"}]' > "$TMP/ios-perm.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-perm.json"
expect "I33 Settings starts fresh, and one permission switch is named by kind and label" 0 '"steps": 4'

printf '%s' '[{"do":"terminate","in":"springboard"}]' > "$TMP/ios-sbkill.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-sbkill.json"
expect "I34 the system UI is never launched or terminated" 2 "never launched or terminated"

python3 -c 'import json; print(json.dumps([{"do": "launch"}] + [{"do": "sleep", "seconds": 1}] * 200))' > "$TMP/ios-long.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-long.json"
expect "I35 check states a list's expected time at the measured median step times and the allowance it needs" 0 '"allowanceNeeded": 254'
run env RICHOS_IOS_DEVICE=x RICHOS_APPLE_TEAM=y python3 "$QA/phone-ios.py" run "$TMP/ios-long.json" --out /Volumes/E1TB/nonexistent-qa-test
expect "I36 a list the default allowance cannot hold is refused before the phone is touched" 2 "Pass --allowance 254"

run env RICHOS_IOS_DEVICE=x RICHOS_APPLE_TEAM=y python3 "$QA/phone-ios.py" run "$TMP/ios-ok.json" --out /Volumes/E1TB/nonexistent-qa-test --prebuilt
expect "I10 reusing an earlier build without its stamp is refused" 2 "--prebuilt needs --stamp"

mkdir -p "$TMP/Fake.app" && printf 'bytes' > "$TMP/Fake.app/RichOSNative"
printf '{"artifact":"%s","sha256":"0000000000000000","commit":"abc","dirty":false}' "$TMP/Fake.app" > "$TMP/fake-stamp.json"
run env RICHOS_IOS_DEVICE=x RICHOS_APPLE_TEAM=y python3 "$QA/phone-ios.py" run "$TMP/ios-ok.json" --out /Volumes/E1TB/nonexistent-qa-test --prebuilt --stamp "$TMP/fake-stamp.json"
expect "I11 a reused app whose bytes differ from its stamp is refused: identity or refuse" 2 "freshness mismatch"

mkdir -p "$TMP/Store/Products/Release-iphoneos/RichOSNative.app" && printf 'bytes' > "$TMP/Store/Products/Release-iphoneos/RichOSNative.app/RichOSNative"
run python3 - "$QA/phone-ios.py" "$TMP/Store/Products/Release-iphoneos/RichOSNative.app" "$TMP/store-stamp.json" <<'PY'
import importlib.util, json, sys
spec = importlib.util.spec_from_file_location("phone_ios", sys.argv[1])
tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tool)
sys.path.insert(0, str(tool.ROOT / "richos/mobile/perf"))
import perfcore
with open(sys.argv[3], "w") as f:
    json.dump({"artifact": sys.argv[2], "sha256": perfcore.tree_sha256(sys.argv[2]), "commit": "abc", "dirty": False}, f)
print(json.dumps(tool.stamped_identity(sys.argv[3])))
PY
expect "I37 a reused build hands the phone the products its stamp names (a store entry's stamp names the store)" 0 "\"products\": \"$(cd "$TMP/Store/Products" && pwd -P)\""

printf '%s' '[{"do":"launch"},{"do":"lock"}]' > "$TMP/ios-lock.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-lock.json"
expect "I12 locking the phone without a person there to open it is refused" 2 "only a person can open the phone again"

printf '%s' '[{"do":"unlock"}]' > "$TMP/ios-unlock.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-unlock.json"
expect "I13 there is no scripted unlock: XCTest's Home press does not open an iOS 26 lock screen" 2 "unknown step 'unlock'"

run python3 "$QA/phone-ios.py" syslog --device x --seconds 0 --out /Volumes/E1TB/nonexistent-qa-test.txt
expect "I14 a log capture with no interval is refused" 2 "--seconds must be 1 to 3600"

run python3 "$QA/phone-ios.py" syslog --device x --seconds 5 --out "$OFFSSD/syslog.txt"
expect "I15 a log capture off the external SSD is refused before the relay starts" 2 "--out must be on /Volumes/E1TB"

run python3 "$QA/phone-ios.py" syslog --device x --seconds 5 --out /Volumes/E1TB/nonexistent-qa-test.txt --keep IOHID
expect "I15b a --keep key short enough to keep the whole log is refused before the relay starts" 2 "--keep needs at least 6 characters"

if [ -d /Volumes/E1TB/tmp ] && SSDTMP="$(mktemp -d /Volumes/E1TB/tmp/qa-syslog-keep.XXXXXX)"; then
  mkdir -p "$TMP/fakesyslog"
  cat > "$TMP/fakesyslog/idevicesyslog" <<'SH'
#!/bin/sh
echo 'Oct  1 08:00:00.000000 backboardd(IOKit)[71] <Debug>: 0x1: set property:IOHIDEventSystemClientIsUnresponsive value:1 client:00000000-FAKE'
echo 'Oct  1 08:00:00.100000 SpringBoard[35] <Notice>: a line nobody asked for'
echo 'Oct  1 08:00:00.200000 RichOSNative[600] <Notice>: the app'
SH
  chmod +x "$TMP/fakesyslog/idevicesyslog"
  run env PATH="$TMP/fakesyslog:$PATH" python3 "$QA/phone-ios.py" syslog --device 00008030-0000000000000001 --seconds 2 \
    --out "$SSDTMP/keep.txt" --keep IOHIDEventSystem
  if [ "$CODE" = 0 ] && printf '%s' "$OUT" | grep -Fq '"kept": 2' && grep -Fq 'IsUnresponsive' "$SSDTMP/keep.txt" \
     && ! grep -Fq 'SpringBoard' "$SSDTMP/keep.txt"; then
    ok "I15c syslog --keep adds the named system lines and still drops every other line"
  else bad "I15c syslog --keep keeps the named lines only" "exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"; fi
  rm -rf "$SSDTMP"
else skip "I15c syslog --keep keeps the named lines only" "/Volumes/E1TB/tmp is not there to write the capture"; fi

run python3 "$QA/phone-ios.py" battery --device 00000000-NOT-A-PHONE --network
expect "I16 a battery reading from a phone that is not there is refused, never a number" 2 "did not report the battery"

run env -u RICHOS_IOS_DEVICE python3 "$QA/phone-ios.py" run "$TMP/ios-ok.json" --out /Volumes/E1TB/nonexistent-qa-test
expect "I5 run refuses without a named phone" 2 "set RICHOS_IOS_DEVICE"

run env RICHOS_IOS_DEVICE=x RICHOS_APPLE_TEAM=y python3 "$QA/phone-ios.py" run "$TMP/ios-ok.json" --out "$OFFSSD/ios-out"
expect "I6 run refuses an output directory off the external SSD" 2 "must be on /Volumes/E1TB"

case "$OFFSSD" in
  /Volumes/E1TB/*) bad "I30 the off-SSD fixture path is off the SSD whatever TMPDIR is" "OFFSSD=$OFFSSD" ;;
  *) case "$TMP" in
       /Volumes/E1TB/*) ok "I30 the off-SSD fixture path stays off the SSD while TMPDIR is on it" ;;
       *) ok "I30 the off-SSD fixture path is off the SSD" ;;
     esac ;;
esac

{
  echo 'Test Case started'
  echo 'PHONE_STEP {"i": 0, "do": "launch", "ok": true}'
  echo '    t =  1.00s PHONE_STEP {"i": 0, "do": "launch", "ok": true}'
  echo 'PHONE_STEP {"i": 1, "do": "wait", "ok": false, "error": "not on screen'
  echo 'PHONE_STEP {"i": 1, "do": "wait", "ok": false, "error": "not on screen within 5 s"}'
} > "$TMP/ios-test.log"
run python3 "$QA/phone-ios.py" parse-log "$TMP/ios-test.log"
if [ "$CODE" = 0 ] && [ "$(printf '%s' "$OUT" | grep -c '"do"')" = 2 ] && printf '%s' "$OUT" | grep -Fq "not on screen within 5 s"; then
  ok "I7 parse-log keeps each step once and skips a line cut in half"
else bad "I7 parse-log keeps each step once" "exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"; fi

printf '{"pairLink":"https://c-x-g7.richos.ceo/#pair=abc","words":"orbit horizon cushion cabin mirror kitchen"}' > "$TMP/ios-lab.json"
run python3 "$QA/phone-ios.py" pair-steps "$TMP/ios-lab.json" --v2-hold 45
if [ "$CODE" = 0 ] && printf '%s' "$OUT" | grep -Fq '"label": "Word 6: "' && printf '%s' "$OUT" | grep -Fq '"seconds": 45' \
   && ! printf '%s' "$OUT" | grep -Fq 'orbit' && ! printf '%s' "$OUT" | grep -Fq '"do": "count"'; then
  ok "I17 pair-steps --v2-hold waits for each word by label and holds before the press, never counting the v1 words"
else bad "I17 pair-steps --v2-hold" "exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"; fi

run python3 "$QA/phone-ios.py" pair-steps "$TMP/ios-lab.json" --v2-hold 900
expect "I18 a v2 hold past the pairing code's five minutes is refused" 2 "--v2-hold must be 20 to 300 seconds"

printf '%s' '[{"do":"appearance"},{"do":"appearance","set":"dark"},{"do":"orientation","set":"landscapeLeft"},{"do":"orientation","set":"portrait"},{"do":"appearance","set":"light"}]' > "$TMP/ios-theme.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-theme.json"
expect "I19 reading and setting the phone's light/dark and turning it validate" 0 '"steps": 5'

printf '%s' '[{"do":"appearance","set":"sepia"}]' > "$TMP/ios-sepia.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-sepia.json"
expect "I20 an appearance the phone does not have is refused before anything is built" 2 "may only set light or dark"

printf '%s' '[{"do":"orientation","set":"sideways"}]' > "$TMP/ios-sideways.json"
run python3 "$QA/phone-ios.py" check "$TMP/ios-sideways.json"
expect "I21 an orientation XCTest cannot set is refused" 2 "may only set portrait"

mkdir -p "$TMP/ios-run/attachments"
{
  echo '{"i": 0, "do": "launch", "ok": true, "start": 10.0, "end": 12.5, "appState": "foreground", "detail": {}}'
  echo '{"i": 1, "do": "audit", "ok": true, "start": 12.5, "end": 14.0, "appState": "foreground", "detail": {"issues": ["1|Contrast failed||Hello"]}}'
  echo '{"i": 2, "do": "wait", "ok": false, "start": 14.0, "end": 19.0, "appState": "foreground", "error": "not on screen within 5 s", "detail": {}}'
} > "$TMP/ios-run/steps.jsonl"
printf '%s' '[{"attachments":[{"exportedFileName":"AAA.png","suggestedHumanReadableName":"01-first_0_7B3105CE-0805-41FF-93EF-D0BE0BB415E9.png"}]}]' > "$TMP/ios-run/attachments/manifest.json"
run python3 "$QA/phone-ios.py" summary "$TMP/ios-run"
if [ "$CODE" = 0 ] && printf '%s' "$OUT" | grep -Fq '"01-first.png"' && printf '%s' "$OUT" | grep -Fq 'Contrast failed||Hello' \
   && printf '%s' "$OUT" | grep -Fq '"failed": 1' && printf '%s' "$OUT" | grep -Fq '"seconds": 2.5'; then
  ok "I22 summary reads a finished run back: each step's outcome and time, the audit's issues, each shot by its name"
else bad "I22 summary reads a finished run back" "exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"; fi

mkdir -p "$TMP/ios-notrun"
run python3 "$QA/phone-ios.py" summary "$TMP/ios-notrun"
expect "I23 summary of a directory that is not a finished run is refused, never an empty table" 2 "not a finished phone-ios.py run"

RUNNER_SWIFT="$DIR/../../mobile/native-ios/UITests/PhysicalDeviceTests.swift"
run python3 "$QA/phone-ios.py" vocabulary "$RUNNER_SWIFT"
expect "I24 the phone runner (PhysicalDeviceTests.perform) has a case for every step phone-ios.py validates" 0 '"same": true'

grep -v '^        case "appearance":' "$RUNNER_SWIFT" > "$TMP/runner-drift.swift"
run python3 "$QA/phone-ios.py" vocabulary "$TMP/runner-drift.swift"
expect "I25 a step the host accepts and the phone runner does not know FAILS, before any build" 1 '"appearance"'

{
  echo 'Oct  1 08:20:03.100000 RichOSNative(Network)[6007] <Debug>: endpoint has associations'
  echo '    a continuation line with no header'
  echo 'Oct  1 08:20:04.200000 RichOSNativeUITests-Runner[5885] <Notice>: the runner, not the app'
  echo 'Oct  1 08:20:15.300000 RichOSNative[6007] <Notice>: one more'
  echo 'Oct  1 08:20:16.000000 SpringBoard[60] <Notice>: someone else'
} > "$TMP/ios-syslog.txt"
run python3 "$QA/phone-ios.py" syslog-rate "$TMP/ios-syslog.txt" --bucket 10
if [ "$CODE" = 0 ] && printf '%s' "$OUT" | grep -Fq '"entries": 2,' && printf '%s' "$OUT" | grep -Fq '"ofAllEntries": 4' \
   && printf '%s' "$OUT" | grep -Fq '"at": "08:20:10"'; then
  ok "I27 syslog-rate counts only the named process's entries per bucket: not the test runner, not continuation lines"
else bad "I27 syslog-rate counts one process" "exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"; fi

printf 'not a phone log\n' > "$TMP/ios-notsyslog.txt"
run python3 "$QA/phone-ios.py" syslog-rate "$TMP/ios-notsyslog.txt"
expect "I28 a file that is not a phone log capture is refused, never counted as zero" 2 "holds no log entries"

printf 'final class Nothing {}\n' > "$TMP/runner-none.swift"
run python3 "$QA/phone-ios.py" vocabulary "$TMP/runner-none.swift"
expect "I26 a runner with no step dispatcher is refused, never read as agreeing" 2 "no perform(...) step dispatcher"

# J1-J7: the UI-automation approval is forecast BEFORE a run, from the phone's own sessions.
# Fixture logs are shaped like xcodebuild's: the phone named on the command line, the runner's
# start and the first suite's start on the phone clock. Their age is their modification time.
SESS="$TMP/sessions"; mkdir -p "$SESS"
export RICHOS_IOS_SESSION_LEDGER="$SESS/ledger.jsonl"   # never the operator's real run ledger
PHONE="00000000-0000000000000000"
session_log() {  # session_log <file> <runner hh:mm:ss.mmm> <suite hh:mm:ss.mmm or TIMEOUT> <seconds ago>
  {
    echo "Command line invocation:"
    echo "    /usr/bin/xcodebuild test-without-building -xctestrun /x.xctestrun -destination id=$PHONE -resultBundlePath /x.xcresult"
    echo "2026-10-01 ${2}123+0100 RichOSNativeUITests-Runner[5881:2976689] [Default] Running tests..."
    if [ "$3" = TIMEOUT ]; then
      echo "	RichOSNativeUITests-Runner (5881) encountered an error (The test runner failed to initialize for UI testing. (Underlying Error: Timed out while enabling automation mode.))"
    else
      echo "Test Suite 'Selected tests' started at 2026-10-01 $3."
    fi
  } > "$SESS/$1"
  python3 -c 'import os,sys,time; t=time.time()-float(sys.argv[2]); os.utime(sys.argv[1],(t,t))' "$SESS/$1" "$4"
}
forecast() { RICHOS_IOS_SESSION_LOGS="$SESS/*.log" python3 "$QA/phone-ios.py" approval --device "$PHONE"; }

session_log a.log 07:47:23.244 07:47:24.000 120
run forecast
expect "J1 two minutes after the last session no approval is expected" 0 '"approvalExpected": false'

session_log a.log 07:47:23.244 07:47:24.000 36000
run forecast
expect "J2 ten hours after the last session an approval is expected, and the forecast says why" 0 'iOS asks again after idle'

run env RICHOS_IOS_DEVICE="$PHONE" RICHOS_APPLE_TEAM=y RICHOS_IOS_SESSION_LOGS="$SESS/*.log" python3 "$QA/phone-ios.py" run "$TMP/ios-ok.json" --out /Volumes/E1TB/nonexistent-qa-test
expect "J3 a run expected to ask is refused BEFORE any build until the CEO was told (--approval-announced)" 2 "Tell Rich so the CEO hears it BEFORE the run"
if [ -e /Volumes/E1TB/nonexistent-qa-test ]; then bad "J3b the refused run wrote nothing" "it created /Volumes/E1TB/nonexistent-qa-test"
else ok "J3b the refused run wrote nothing"; fi
if printf '%s' "$OUT" | grep -qF "placeholder device id" && [ ! -s "$RICHOS_ESCALATION_LEDGER" ]; then
  ok "J3c the all-zeros placeholder phone raises no escalation, and says so"
else bad "J3c placeholder phone raises nothing" "ledger: $(cat "$RICHOS_ESCALATION_LEDGER" 2>/dev/null | head -c 200) out: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"; fi

session_log b.log 08:00:00.000 TIMEOUT 60
run forecast
expect "J4 a session nobody approved means the next one asks too" 0 'nobody approved it'

rm -f "$SESS"/*.log
session_log a.log 07:47:23.244 07:47:33.128 50000
session_log c.log 07:47:23.244 07:47:23.900 3600
run forecast
expect "J5 after the passcode is removed, a cold session that did not ask means none is expected" 0 'started in 0.7 s without asking'

session_log d.log 07:47:23.244 07:47:33.128 36000
rm -f "$SESS/c.log"; run forecast
expect "J6 a cold session that waited for approval (the passcode is still set) keeps the forecast at expected" 0 '"approvalExpected": true'

# J7: the phone's own reading (PhysicalDeviceTests prints PHONE_PASSCODE) decides, idle or not.
echo 'PHONE_PASSCODE {"configured":false,"error":"com.apple.LocalAuthentication -5"}' >> "$SESS/d.log"
python3 -c 'import os,sys,time; t=time.time()-36000; os.utime(sys.argv[1],(t,t))' "$SESS/d.log"
run forecast
expect "J7 ten hours idle, but the phone said it has no passcode: no approval is expected" 0 'the phone has no passcode (the phone itself at a session'

# FC1-FC4 (2026-10-02): the forecast must not send the CEO to the phone for nothing.
# K1: back-to-back runs that enabled automation without a prompt, the last one 50 min ago (past the
# 10 min quiet window, short of 9.3 h): no approval expected. Red on main: it said "expected".
rm -f "$SESS"/*.log "$SESS/ledger.jsonl"
session_log a.log 07:00:00.000 07:00:00.700 20000
session_log b.log 07:47:23.244 07:47:24.000 3000
run forecast
expect "FC1 recent prompt-free sessions, passcode unreadable: no approval is expected" 0 '"approvalExpected": false'
# K2: the same phone under its other spelling (CoreDevice UUID vs hardware UDID) finds its sessions.
run env RICHOS_IOS_DEVICE_ALIASES="$PHONE" RICHOS_IOS_SESSION_LOGS="$SESS/*.log" python3 "$QA/phone-ios.py" approval --device 111AA1A1-11AA-1111-1A11-A111111A1A1A
expect "FC2 a phone named by its other id still has its sessions on record" 0 '"sessionsOnRecord": 2'
# K3: nothing known: unknown, never "expected", and it says what settles it.
rm -f "$SESS"/*.log "$SESS/ledger.jsonl"
run forecast
expect "FC3 no passcode reading and no session: unknown, not expected" 0 '"approvalExpected": "unknown"'
expect "FC3b the unknown forecast says what would settle it" 0 'settledBy'
# K4: a run writes its session record where `approval` reads it (fake rios; the log is shaped like xcodebuild's).
KOUT="/Volumes/E1TB/tmp/claude/quint-forecast1-qa-$$"
mkdir -p "$KOUT/physical"
cat > "$TMP/fake-rios" <<SH
#!/bin/sh
printf '%s\n' "$KOUT/physical/script-123.xcresult"
{
  echo "-destination id=$PHONE"
  echo "2026-10-01 07:47:23.244123+0100 RichOSNativeUITests-Runner[5881:1] [Default] Running tests..."
  echo "Test Suite 'Selected tests' started at 2026-10-01 07:47:24.000."
} > "$KOUT/physical/verify-script-123-test.log"
exit 0
SH
chmod +x "$TMP/fake-rios"
session_log a.log 07:47:23.244 07:47:24.000 120
run env RICHOS_IOS_DEVICE="$PHONE" RICHOS_APPLE_TEAM=y RICHOS_IOS_RIOS="$TMP/fake-rios" RICHOS_IOS_SESSION_LOGS="$SESS/*.log" python3 "$QA/phone-ios.py" run "$TMP/ios-ok.json" --out "$KOUT/out"
if [ -s "$SESS/ledger.jsonl" ] && grep -qF '"enableWaitSeconds": 0.8' "$SESS/ledger.jsonl" && grep -qF '"approvalNeeded": false' "$SESS/ledger.jsonl"; then
  ok "FC4 a run records its session (start, end, enable wait, approval needed) in the ledger"
else bad "FC4 a run records its session" "ledger: $(cat "$SESS/ledger.jsonl" 2>/dev/null | head -c 300) out: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-300)"; fi
rm -f "$SESS"/*.log
run forecast
expect "FC4b the recorded run alone is a session on record that approval reads" 0 '"sessionsOnRecord": 1'
rm -rf "$KOUT"
rm -f "$SESS"/*.log "$SESS/ledger.jsonl"
# the P tests need a phone whose forecast IS expected: a 10 h old session of PHONE2 with no reading.
PHONE_SAVE="$PHONE"; PHONE="00008030-0000FEEDFACE0001"
session_log p2.log 07:47:23.244 07:47:24.000 36000
PHONE="$PHONE_SAVE"

# P01-P05: the approval refusal raises its own needs=ceo-hands escalation (2026-10-01, richos-hq
# docs/operations/2026-10-01-escalation-wakes-the-lead.md item C, and Sage's review items 7 and 11).
# A fake, non-placeholder phone id with no session on record forecasts an approval; the runs start
# in a fixture git workspace so the escalation names that workspace; the ledger is the fixture one.
PHONE2="00008030-0000FEEDFACE0001"
PWS="$TMP/phone-ws"; mkdir -p "$PWS"; git -C "$PWS" init -q
PWS_REAL="$(cd "$PWS" && pwd -P)"
ledger_rows() {  # ledger_rows <ledger> <event> [needs]: rows of that event for the fixture workspace
  [ -f "$1" ] || { echo 0; return; }
  python3 - "$1" "$2" "$PWS_REAL" "${3:-}" <<'PY'
import json, sys
path, event, ws, needs = sys.argv[1:5]
n = 0
for line in open(path, encoding="utf-8", errors="replace"):
    try:
        r = json.loads(line)
    except ValueError:
        continue
    if r.get("event") != event:
        continue
    if event == "Escalation" and (r.get("worktree") != ws or (needs and r.get("needs") != needs)):
        continue
    n += 1
print(n)
PY
}
REAL_BEFORE="$(ledger_rows "$REAL_LEDGER" Escalation)"
phone_run() { (cd "$PWS" && env RICHOS_IOS_DEVICE="$PHONE2" RICHOS_APPLE_TEAM=y RICHOS_IOS_SESSION_LOGS="$SESS/*.log" \
  python3 "$QA/phone-ios.py" run "$@" --out /Volumes/E1TB/nonexistent-qa-test); }
run phone_run "$TMP/ios-ok.json"
P01_ID="$(printf '%s' "$OUT" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("escalation") or "")' 2>/dev/null)"
if [ "$CODE" = 2 ] && [ -n "$P01_ID" ] && [ "$(ledger_rows "$RICHOS_ESCALATION_LEDGER" Escalation ceo-hands)" = 1 ] \
   && printf '%s' "$OUT" | grep -qF "Raised $P01_ID with needs=ceo-hands" && printf '%s' "$OUT" | grep -qF "do not raise another" \
   && [ ! -e /Volumes/E1TB/nonexistent-qa-test ]; then
  ok "P01 the approval refusal still refuses, and raises exactly one needs=ceo-hands escalation for its workspace"
else bad "P01 the refusal raises one ceo-hands escalation" "exit $CODE id='$P01_ID' rows=$(ledger_rows "$RICHOS_ESCALATION_LEDGER" Escalation ceo-hands) out: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-300)"; fi
run phone_run "$TMP/ios-ok.json"
if [ "$CODE" = 2 ] && [ "$(ledger_rows "$RICHOS_ESCALATION_LEDGER" Escalation)" = 1 ] && [ -n "$P01_ID" ] \
   && printf '%s' "$OUT" | grep -qF "Already raised as $P01_ID"; then
  ok "P02 a second refusal in the same workspace adds no row and names the first id"
else bad "P02 one need, one id" "exit $CODE rows=$(ledger_rows "$RICHOS_ESCALATION_LEDGER" Escalation) first='$P01_ID' out: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-300)"; fi
if [ "$(ledger_rows "$REAL_LEDGER" Escalation)" = "$REAL_BEFORE" ]; then
  ok "P03 the operator's own ledger gained no row for the fixture workspace"
else bad "P03 the real ledger is untouched" "rows for $PWS_REAL in $REAL_LEDGER went $REAL_BEFORE -> $(ledger_rows "$REAL_LEDGER" Escalation)"; fi
run python3 - "$QA/phone-ios.py" "$PWS" <<'PY'
import importlib.util, json, sys
spec = importlib.util.spec_from_file_location("phone_ios", sys.argv[1])
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
print(json.dumps({"closed": m.close_approval_escalations(sys.argv[2], "/fixture/verify-script-1-test.log")}))
PY
if [ "$CODE" = 0 ] && printf '%s' "$OUT" | grep -qF "\"closed\": [\"$P01_ID\"]" && [ -n "$P01_ID" ] \
   && [ "$(ledger_rows "$RICHOS_ESCALATION_LEDGER" EscalationAck)" = 1 ] \
   && grep -qF "UI automation was allowed at the phone" "$RICHOS_ESCALATION_LEDGER"; then
  ok "P04 a run that reached its first step closes the approval escalation itself, saying so"
else bad "P04 the approval closes its escalation" "exit $CODE out: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-300)"; fi
python3 - "$TMP/ios-pair.json" <<'PY'
import json, sys
json.dump([{"do": "launch"}, {"do": "tap", "id": "pair.link"},
           {"do": "type", "id": "pairlink.field", "text": "https://lab.invalid/#pair=fixturecode"},
           {"do": "tap", "id": "pairlink.submit"}], open(sys.argv[1], "w"))
PY
run phone_run "$TMP/ios-pair.json" --approval-announced
if [ "$CODE" = 2 ] && printf '%s' "$OUT" | grep -qF "pairing window lasts five minutes" \
   && printf '%s' "$OUT" | grep -qF '[{\"do\": \"state\"}]' && [ ! -e /Volumes/E1TB/nonexistent-qa-test ]; then
  ok "P05 a pairing list while an approval is expected is refused even when announced: spend the approval first"
else bad "P05 pairing waits for the approval" "exit $CODE out: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-300)"; fi

echo ""
unset RICHOS_DEVICE_VERB
echo "=== V. pair-words: the six v2 words on both sides, checked before They match ==="
PW="$QA/pair-words.py"
CORPUS_FP="$DIR/../../mobile/conformance/vectors/fingerprint.json"
for i in 0 1; do
  eval "$(python3 -c "
import json, shlex, sys
c = json.load(open(sys.argv[1]))['v2']['cases'][int(sys.argv[2])]
for k in ('origin', 'ca_fingerprint_sha256', 'device_point_b64url', 'phrase'):
    print(f'V_{k.upper()}={shlex.quote(c[k])}')
" "$CORPUS_FP" "$i")"
  run python3 "$PW" compute --origin "$V_ORIGIN" --ca "$V_CA_FINGERPRINT_SHA256" --key "$V_DEVICE_POINT_B64URL"
  if [ "$CODE" = 0 ] && [ "$(printf '%s' "$OUT" | python3 -c 'import json,sys; print(" ".join(json.load(sys.stdin)["words"]))')" = "$V_PHRASE" ]; then
    ok "V$((i + 1)) compute gives the phone corpus's own words for case $i ($V_PHRASE)"
  else bad "V$((i + 1)) compute matches the phone corpus case $i" "wanted '$V_PHRASE'; exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"; fi
done
run python3 "$PW" compute --origin "HTTPS://C-5DE0DBE0862DD461AE305AF5CEF35202-G2.RICHOS.CEO:443/" --ca "$V_CA_FINGERPRINT_SHA256" --key "$V_DEVICE_POINT_B64URL"
expect "V3 the origin is written the way a browser writes it (case, default port, slash)" 0 '"castle"'

mkdir -p "$TMP/pwlab/data/phone"
printf 'richos-mobile-isolated-v1' > "$TMP/pwlab/data/lab-owner"
printf '%s\n' '-----BEGIN CERTIFICATE-----' "$(printf 'stand-in authority DER' | base64)" '-----END CERTIFICATE-----' > "$TMP/pwlab/data/phone/ca.crt"
printf '{"pairing_version":2,"public_key":"%s","fingerprint_confirmed":false}' "$V_DEVICE_POINT_B64URL" > "$TMP/pwlab/data/phone/device.json"
printf '{"origin":"https://c-x-g7.richos.ceo","data":"%s","ca":"%s"}' "$TMP/pwlab/data" "$TMP/pwlab/data/phone/ca.crt" > "$TMP/pwlab/mac.json"
STANDIN_FP="$(python3 -c "import hashlib; print(':'.join(f'{b:02X}' for b in hashlib.sha256(b'stand-in authority DER').digest()))")"
run python3 "$PW" compute --origin https://c-x-g7.richos.ceo --ca "$STANDIN_FP" --key "$V_DEVICE_POINT_B64URL"
WANT="$(printf '%s' "$OUT" | python3 -c 'import json,sys; print(" ".join(json.load(sys.stdin)["words"]))')"
run python3 "$PW" mac "$TMP/pwlab"
if [ "$CODE" = 0 ] && [ "$(printf '%s' "$OUT" | python3 -c 'import json,sys; print(" ".join(json.load(sys.stdin)["words"]))')" = "$WANT" ]; then
  ok "V4 mac reads the lab's origin, CA (DER hash) and registered key: the same words compute gives"
else bad "V4 mac from the lab's own files" "wanted '$WANT'; exit $CODE: $(printf '%s' "$OUT" | tr '\n' ' ' | cut -c1-240)"; fi

read -r -a want_words <<<"$WANT"
set -- "${want_words[@]}"
{
  echo 'PHONE_STEP {"i": 5, "do": "wait", "ok": true, "detail": {"label": "Word 1: '"$1"'"}}'
  echo 'PHONE_STEP {"i": 6, "do": "wait", "ok": true, "detail": {"label": "Word 2: '"$2"'"}}'
  echo 'PHONE_STEP {"i": 7, "do": "wait", "ok": true, "detail": {"label": "Word 3: '"$3"'"}}'
  echo 'PHONE_STEP {"i": 8, "do": "wait", "ok": true, "detail": {"label": "Word 4: '"$4"'"}}'
  echo 'PHONE_STEP {"i": 9, "do": "wait", "ok": true, "detail": {"label": "Word 5: '"$5"'"}}'
} > "$TMP/pw-partial.log"
cp "$TMP/pw-partial.log" "$TMP/pw-same.log"
echo 'PHONE_STEP {"i": 10, "do": "wait", "ok": true, "detail": {"label": "Word 6: '"$6"'"}}' >> "$TMP/pw-same.log"
cp "$TMP/pw-partial.log" "$TMP/pw-differ.log"
echo 'PHONE_STEP {"i": 10, "do": "wait", "ok": true, "detail": {"label": "Word 6: zzz"}}' >> "$TMP/pw-differ.log"
run python3 "$PW" check "$TMP/pwlab" "$TMP/pw-same.log"
expect "V5 the phone's six words equal the Mac's: check passes" 0 '"same": true'
run python3 "$PW" check "$TMP/pwlab" "$TMP/pw-differ.log"
expect "V6 one differing word FAILS the check, exit 1: do not press They match" 1 '"same": false'
run python3 "$PW" check "$TMP/pwlab" "$TMP/pw-partial.log"
expect "V7 a phone log that has not shown all six words cannot answer, never a pass" 2 "has not shown word(s) [6]"

rm "$TMP/pwlab/data/lab-owner"
run python3 "$PW" mac "$TMP/pwlab"
expect "V8 a data directory without the isolated lab's owner marker is refused, never read" 2 "owner marker"
printf 'richos-mobile-isolated-v1' > "$TMP/pwlab/data/lab-owner"
printf '{"pairing_version":1,"public_key":"%s"}' "$V_DEVICE_POINT_B64URL" > "$TMP/pwlab/data/phone/device.json"
run python3 "$PW" mac "$TMP/pwlab"
expect "V9 a phone that paired with v1 is refused: its words are ready.json's" 2 "not 2"
rm "$TMP/pwlab/data/phone/device.json"
run python3 "$PW" mac "$TMP/pwlab"
expect "V10 before any phone has sent the link there are no Mac words to give" 2 "no phone has registered a key"

echo ""
echo "=== US. usage-shape: the shape of Claude Code's get_usage answer, read as RichOS reads it ==="
US="$QA/usage-shape.sh"
# A scripted claude: answers initialize with {}, get_usage with the payload in $FAKE_USAGE,
# or nothing at all when FAKE_SILENT is set (a claude that never answers).
cat > "$TMP/fake-usage-claude" <<'FAKE'
#!/bin/sh
[ "${1:-}" = "--version" ] && { echo "9.9.9 (scripted)"; exit 0; }
[ -n "${FAKE_SILENT:-}" ] && { cat >/dev/null; exit 0; }
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"initialize"'*) printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":{}}}\n' "$id" ;;
    *'"get_usage"'*) printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":%s}}\n' "$id" "$(cat "$FAKE_USAGE")" ;;
  esac
done
FAKE
chmod 755 "$TMP/fake-usage-claude"
printf '%s' '{"rate_limits_available":true,"rate_limits":null,"session":{"secret":"never printed"},"subscription_type":"max"}' > "$TMP/usage-null.json"
printf '%s' '{"rate_limits_available":true,"rate_limits":{"five_hour":{"utilization":41,"resets_at":"2099-01-01T00:00:00Z"},"seven_day":{"utilization":28}},"session":{"secret":"never printed"}}' > "$TMP/usage-ok.json"
export FAKE_USAGE="$TMP/usage-null.json"; run "$US" --claude "$TMP/fake-usage-claude" --wait 2
expect "US1 rate_limits null is NO READING for the app's reader (NoReading, not an error), exit 1, naming the rule" 1 "verdict: NO READING: rate_limits is null (the reader calls this NoReading"
if printf '%s' "$OUT" | grep -q "never printed"; then
  bad "US1b the session field is never printed" "the output carried the session value"
else
  ok "US1b the session field is never printed"
fi
export FAKE_USAGE="$TMP/usage-ok.json"; run "$US" --claude "$TMP/fake-usage-claude" --wait 2
expect "US2 a five-hour and weekly window with numeric utilization is readable, exit 0" 0 "verdict: readable by RichOS quota reader"
run "$US" --claude "$TMP/no-such-claude" --wait 2
expect "US3 no claude binary is refused, exit 2" 2 "no claude to ask"
export FAKE_SILENT=1; run "$US" --claude "$TMP/fake-usage-claude" --wait 2; unset FAKE_SILENT FAKE_USAGE
expect "US4 a claude that never answers get_usage is refused, exit 2" 2 "no get_usage response"

echo ""
echo "=== X. the fixtures are still the fixtures ==="

if [ "$HAVE_PIL" = 0 ]; then
  skip "X1 the committed fixtures match their generator" \
       "Pillow is not installed, so they cannot be re-rendered here. The committed bytes were used by every case above, which is what they are for."
else
  run "$QA/fixtures/make-fixtures.py" --check
  expect "X1 the committed fixtures still match their generator" 0 "same     ocr-control.png"
fi

echo ""
if [ "$FAIL" -gt 0 ]; then
  echo "=== qa toolkit tests: $FAIL FAILED, $PASS passed, $SKIP skipped ==="
  exit 1
fi
if [ "$SKIP" -gt 0 ]; then
  # Exit 2, never 0: a skipped case did not run, and exit 0 is recorded as `passed` (hunt R18 pattern).
  echo "=== qa toolkit tests: all $PASS passed, $SKIP NOT RUN (skipped cases named above) ==="
  exit 2
fi
echo "=== qa toolkit tests: all $PASS passed ==="
