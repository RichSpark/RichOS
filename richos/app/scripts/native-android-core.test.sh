#!/usr/bin/env bash
# Native Android app, stream A1 core: the plain Kotlin/JVM core's tests, the headless CLI's
# tests, and the headless CLI run as separate processes through bin/randroid (build plan §3.1
# loops L1 and L1′). No emulator and no Android SDK. A host without a Java 21+ runtime or
# without the external SSD prints NOT RUN and exits 2; a build or test failure exits 1.
# run-tests: no-host-screen: JVM unit tests and short-lived headless CLI processes; nothing is launched on any screen
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/engine/scripts/lib/machine-settings.sh richos/mobile/native-android/core richos/mobile/native-android/cli richos/mobile/native-android/bin richos/mobile/native-android/gradle richos/mobile/native-android/gradlew richos/mobile/native-android/settings.gradle.kts richos/mobile/native-android/build.gradle.kts richos/mobile/native-android/gradle.properties richos/app/scripts/native-android-core.test.sh richos/mobile/conformance/vectors
# run-tests: covers richos/mobile/native-android/cli/src/main/kotlin/dev/richos/android/cli/Main.kt richos/mobile/native-android/cli/src/test/kotlin/dev/richos/android/cli/CliTest.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/Action.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/Connection.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/ConnectionOwner.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/CoreJson.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/MacTransport.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/Outbox.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/Ports.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/RichCore.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/Settings.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/State.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/Time.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/Voice.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/dev/DevMac.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/dev/DevRuntime.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/dev/Fixtures.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/protocol/Fingerprint.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/protocol/MacApi.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/protocol/NotificationPreview.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/protocol/PairLink.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/protocol/Signing.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/protocol/Sse.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/ConnectionOwnerTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/ConnectionTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/ConversationTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/CoreTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/MacTransportTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/OutboxTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/PairingTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/SettingsTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/VoiceTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/protocol/NotificationPreviewTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/protocol/ProtocolTest.kt richos/mobile/native-android/bin/randroid richos/mobile/native-android/gradlew richos/mobile/native-android/gradle/libs.versions.toml richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/AppLinks.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/Attach.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/AttachTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/MacWaitLifecycleTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/conformance/PairV2ConformanceTest.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/Echoes.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/Transcript.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/EchoReconciliationTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/MicrophoneCardTest.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/Questions.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/QuestionsTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/ReopenAfterHelloTest.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/ReturnWaitsTest.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/DefaultNetwork.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/ProvisionalReply.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/ProvisionalReplyTest.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/ReplyAudio.kt richos/mobile/native-android/core/src/main/kotlin/dev/richos/android/core/RichFiles.kt richos/mobile/native-android/core/src/test/kotlin/dev/richos/android/core/RichFileTest.kt
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
NA="$ROOT/richos/mobile/native-android"
RANDROID="$NA/bin/randroid"

MACHINE_LIB="$ROOT/richos/engine/scripts/lib"
# shellcheck source=../../engine/scripts/lib/machine-settings.sh
. "$MACHINE_LIB/machine-settings.sh"
richos_machine_settings || exit 2
python3 "$MACHINE_LIB/machine_settings.py" cache >/dev/null || exit 2
# Java: bin/randroid picks a 21+ runtime (RANDROID_JAVA_HOME, JAVA_HOME, Android Studio's) and
# exits 2 with NOT RUN when there is none; that exit is passed through below.

SCRATCH_PARENT="${TMPDIR:-$(python3 -c 'import tempfile; print(tempfile.gettempdir())')}"
mkdir -p "$SCRATCH_PARENT"
SCRATCH="$(mktemp -d "$SCRATCH_PARENT/native-android-core.XXXXXX")" || { echo "  FAIL  native-android-core: no scratch"; exit 1; }
trap 'rm -rf "$SCRATCH"' EXIT HUP INT TERM
export RANDROID_SESSION="$SCRATCH/headless.json"

fails=0
bad() { echo "  FAIL  native-android-core: $1"; fails=$((fails + 1)); }

# L1: the core's and the CLI's JVM tests.
"$RANDROID" test core -q
code=$?
if [ "$code" -eq 2 ]; then echo "  NOT RUN  native-android-core: bin/randroid reported a missing host tool"; exit 2; fi
[ "$code" -eq 0 ] || bad "JVM tests (:core:test :cli:test) exited $code"

# L1′: the installed CLI, as separate processes sharing one session file.
if out="$("$RANDROID" headless scenario draft-survives-restart)"; then
  python3 - "$out" <<'PY' || bad "the scenario result is not the preserved envelope"
import json, sys
r = json.loads(sys.argv[1])
assert r["ok"] is True and r["mode"] == "headless" and r["command"] == "scenario", r
assert r["result"]["name"] == "draft-survives-restart" and len(r["result"]["trace"]) == 4, r
PY
else
  bad "headless scenario draft-survives-restart failed"
fi

"$RANDROID" headless fixture online >/dev/null || bad "headless fixture online failed"
"$RANDROID" headless action '{"type":"compose","text":"between processes"}' >/dev/null || bad "headless compose failed"
if out="$("$RANDROID" headless state)"; then
  python3 - "$out" <<'PY' || bad "a draft did not survive into the next process"
import json, sys
s = json.loads(sys.argv[1])["result"]["state"]
assert s["draft"] == "between processes" and s["online"] is True, s
PY
else
  bad "headless state failed"
fi

if err="$("$RANDROID" headless action '{"type":"select-thread","threadId":"nope"}' 2>&1 >/dev/null)"; then
  bad "a refused action exited 0"
else
  python3 - "$err" <<'PY' || bad "a refused action did not print the preserved error shape"
import json, sys
e = json.loads(sys.argv[1].strip().splitlines()[-1])
assert e["ok"] is False and e["error"] == "Unknown conversation" and "elapsedMs" in e, e
PY
fi

if [ "$fails" -gt 0 ]; then exit 1; fi
echo '  PASS  native-android-core: JVM core + CLI tests, headless scenario, cross-process state, error shape'
