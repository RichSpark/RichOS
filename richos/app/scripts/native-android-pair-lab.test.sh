#!/usr/bin/env bash
# Native Android: the emulator pairing helpers (richos/mobile/native-android/bin/emu-pair-front.mjs,
# emu-ui.py), with no emulator and no test Mac. A stand-in upstream on loopback proves the TLS
# front: it presents a localhost certificate that verifies against its own throwaway CA and
# nothing else, forwards requests and streamed bodies, and deletes its keys on SIGTERM. `qr`
# draws a pairing link as a PNG of the size its module count says. Both helpers refuse any
# serial that is not an emulator's before running adb. A host without node, openssl, curl, a
# Java runtime or the ZXing jar in the Gradle cache prints NOT RUN and exits 2.
# run-tests: no-host-screen: loopback servers and files only; no window, no device
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/engine/scripts/lib/machine-settings.sh richos/mobile/native-android/bin/emu-pair-front.mjs richos/mobile/native-android/bin/emu-ui.py richos/app/scripts/native-android-pair-lab.test.sh
# run-tests: covers richos/mobile/native-android/bin/emu-pair-front.mjs richos/mobile/native-android/bin/emu-ui.py
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
BIN="$ROOT/richos/mobile/native-android/bin"
MACHINE_LIB="$ROOT/richos/engine/scripts/lib"
# shellcheck source=../../engine/scripts/lib/machine-settings.sh
. "$MACHINE_LIB/machine-settings.sh"
richos_machine_settings || exit 2
CACHE_ROOT="$(python3 "$MACHINE_LIB/machine_settings.py" cache)" || exit 2

for tool in node openssl curl python3; do
  command -v "$tool" >/dev/null 2>&1 || { echo "  NOT RUN  native-android-pair-lab: no $tool"; exit 2; }
done
GRADLE_HOME="${GRADLE_USER_HOME:-$CACHE_ROOT/richos-native-android/gradle}"
if [ -z "$(find "$GRADLE_HOME/caches/modules-2/files-2.1/com.google.zxing/core" -name core-3.5.4.jar 2>/dev/null | head -1)" ]; then
  echo "  NOT RUN  native-android-pair-lab: no ZXing core 3.5.4 in $GRADLE_HOME (build the Android app once)"; exit 2
fi

mkdir -p "${TMPDIR:-$(python3 -c 'import tempfile; print(tempfile.gettempdir())')}"
SCRATCH="$(mktemp -d "${TMPDIR:-$(python3 -c 'import tempfile; print(tempfile.gettempdir())')}/native-android-pair-lab.XXXXXX")" || { echo "  FAIL  native-android-pair-lab: no scratch"; exit 1; }
UP_PID=""; FRONT_PID=""
cleanup() {
  # Only the two processes this suite started, by the PIDs captured when it started them.
  [ -n "$FRONT_PID" ] && kill -TERM "$FRONT_PID" 2>/dev/null
  [ -n "$UP_PID" ] && kill -TERM "$UP_PID" 2>/dev/null
  wait 2>/dev/null
  rm -rf "$SCRATCH"
}
trap cleanup EXIT HUP INT TERM

fails=0
bad() { echo "  FAIL  native-android-pair-lab: $1"; fails=$((fails + 1)); }

# A stand-in upstream: /hello answers, /stream sends two chunks apart (as the Mac's event stream does).
node -e '
const http = require("http");
const s = http.createServer((q, r) => {
  if (q.url === "/stream") { r.writeHead(200, {"content-type": "text/event-stream"}); r.write("data: one\n\n"); setTimeout(() => r.end("data: two\n\n"), 200); return; }
  let body = ""; q.on("data", c => body += c); q.on("end", () => { r.writeHead(200, {"x-upstream": "yes"}); r.end("hello " + q.method + " " + body); });
});
s.listen(0, "127.0.0.1", () => console.log(s.address().port));
process.on("SIGTERM", () => process.exit(0));
' > "$SCRATCH/up.port" &
UP_PID=$!
for _ in $(seq 1 50); do [ -s "$SCRATCH/up.port" ] && break; sleep 0.1; done
UP="$(cat "$SCRATCH/up.port")"
[ -n "$UP" ] || { bad "the stand-in upstream did not start"; exit 1; }

node "$BIN/emu-pair-front.mjs" serve --upstream "$UP" --dir "$SCRATCH/front" > "$SCRATCH/front.json" &
FRONT_PID=$!
for _ in $(seq 1 100); do [ -s "$SCRATCH/front.json" ] && break; sleep 0.1; done
PORT="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["port"])' "$SCRATCH/front.json" 2>/dev/null)"
[ -n "$PORT" ] || { bad "the front did not report its port: $(cat "$SCRATCH/front.json")"; exit 1; }

got="$(curl -sS --cacert "$SCRATCH/front/ca.pem" -X POST --data 'ping' "https://localhost:$PORT/hello" 2>&1)"
[ "$got" = "hello POST ping" ] || bad "a request through the front came back as '$got'"
stream="$(curl -sS -N --cacert "$SCRATCH/front/ca.pem" "https://localhost:$PORT/stream" 2>&1 | tr -d '\n')"
[ "$stream" = "data: onedata: two" ] || bad "a streamed body through the front came back as '$stream'"
if curl -sS -o /dev/null "https://localhost:$PORT/hello" 2>/dev/null; then bad "the front's certificate verified without its throwaway CA"; fi
[ "$(stat -f %Lp "$SCRATCH/front")" = "700" ] || bad "the certificate folder is not mode 0700"
[ "$(stat -f %Lp "$SCRATCH/front/ca.key")" = "600" ] || bad "the CA key is not mode 0600"

kill -TERM "$FRONT_PID"; wait "$FRONT_PID" 2>/dev/null; FRONT_PID=""
[ ! -e "$SCRATCH/front/ca.key" ] && [ ! -e "$SCRATCH/front/localhost.key" ] || bad "the front left its private keys behind after SIGTERM"
[ -f "$SCRATCH/front/ca.pem" ] || bad "the front removed its public CA certificate"

out="$(node "$BIN/emu-pair-front.mjs" trust --serial 0123456789ABCDEF --ca "$SCRATCH/front/ca.pem" 2>&1)"; code=$?
[ "$code" -eq 1 ] && [[ "$out" == *"emulator only"* ]] || bad "trust accepted a serial that is not an emulator's (exit $code: $out)"

printf 'https://mac-7f3a.example.ts.net/#pair=Zm9yLXRlc3Qtb25seQ' > "$SCRATCH/link.txt"
out="$(node "$BIN/emu-pair-front.mjs" qr --text-file "$SCRATCH/link.txt" --out "$SCRATCH/qr.png" --scale 4 2>&1)"; code=$?
if [ "$code" -ne 0 ]; then
  bad "qr failed (exit $code: $out)"
else
  python3 - "$SCRATCH/qr.png" "$out" <<'PY' || bad "qr drew the wrong picture: $out"
import json, struct, sys
data = open(sys.argv[1], "rb").read()
modules = json.loads(sys.argv[2])["modules"]
assert data[:8] == b"\x89PNG\r\n\x1a\n"
width, height = struct.unpack(">II", data[16:24])
assert modules >= 21 and width == height == modules * 4, (modules, width, height)
PY
  [ ! -e "$SCRATCH/QrPng.java" ] || bad "qr left its generator source behind"
fi

out="$(python3 "$BIN/emu-ui.py" --serial 0123456789ABCDEF texts 2>&1)"; code=$?
[ "$code" -eq 2 ] && [[ "$out" == *"emulators only"* ]] || bad "emu-ui.py accepted a serial that is not an emulator's (exit $code: $out)"

if [ "$fails" -gt 0 ]; then exit 1; fi
echo '  PASS  native-android-pair-lab: TLS front (verify, forward, stream, keys removed), qr, emulator-only refusals'
