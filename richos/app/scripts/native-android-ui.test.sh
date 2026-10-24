#!/usr/bin/env bash
# Native Android app, stream A2: every round-12 screen and attachment screen, by name, headless.
#
#   native-android-ui.test.sh                 the design-system and screen tests, then EVERY screen
#                                             rendered headless (Robolectric, native graphics, no
#                                             emulator) in dark and light, on a 360 dp and a 412 dp
#                                             phone, at 2x text on the small phone, plus the motion
#                                             filmstrips; each frame checked (names, 48 dp targets,
#                                             no clipped or broken text, composer on screen, the
#                                             timer clear of Cancel) and written as a PNG; and the
#                                             speckled ground: on the drawn screens in both themes
#                                             (SpeckleOnScreensTest), and SpeckleTest's pinned
#                                             numbers re-derived from the design system's own
#                                             engine by bin/speckle-golden.mjs (also in --fast)
#                                             In a teammate workspace (a cc/ branch in a linked
#                                             worktree) only the test classes the branch adds or
#                                             changes, plus those its changed files are claimed by
#                                             (lib/android_ui_scope.py), with one line saying so;
#                                             nothing mapped is a refusal (exit 64) naming the full
#                                             command. proof-run.py and the nightlies set
#                                             RICHOS_TEST_SCOPE=full and always get everything.
#   native-android-ui.test.sh --full          everything, wherever it runs
#   native-android-ui.test.sh <id|gN> …     only those screens (ids from round 12, or gN for a
#                                             whole group); prints where the PNGs went
#   native-android-ui.test.sh --list          every screen id, its group and title (no JVM)
#   native-android-ui.test.sh --fast          only the pure JVM tests (contrast, follow rule,
#                                             voice frames, catalog, taps), no rendering
#
# RICHOS_SHOTS overrides where the PNGs go. A host without the external SSD, a Java 21+ runtime,
# the Android SDK, Node, or the design system's speckle.js (RICHOS_DESIGN_SPECKLE, default richos-hq
# `design/system/speckle.js`, the path bin/speckle-golden.mjs defaults to) prints NOT RUN and exits 2
# (bin/randroid decides Java and Gradle); a failed check on a capable host exits 1.
# run-tests: no-host-screen: Robolectric renders on the JVM; nothing opens a window
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/engine/scripts/lib/machine-settings.sh richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/design richos/mobile/native-android/app/src/main/res/font richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/design richos/mobile/native-android/core/src/main richos/mobile/native-android/app/build.gradle.kts richos/mobile/native-android/gradle richos/mobile/native-android/bin/randroid richos/app/scripts/native-android-ui.test.sh richos/mobile/native-android/bin/speckle-golden.mjs
# run-tests: covers richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/CardsRoom.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/RefusalReasonAndCardsRoomTest.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/design/Components.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/design/Contrast.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/design/Press.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/design/RichColors.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/design/RichIcons.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/design/RichMotion.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/design/RichTheme.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/design/RichType.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/attach/AttachComposer.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/attach/AttachOverlays.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/attach/AttachViews.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/attach/PhotoArt.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/catalog/AttachCatalog.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/catalog/ScreenCatalog.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/composer/Cards.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/composer/Composer.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/composer/Orb.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/conversation/Bubbles.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/conversation/Follow.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/conversation/Header.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/conversation/Thread.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/model/Attachments.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/model/ScreenModel.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/overlays/Scanner.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/overlays/Sheets.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/overlays/SystemDrawings.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/overlays/Takeovers.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/RichApp.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/UiEvent.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/voice/VoiceFrame.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/design/ContrastTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/catalog/CatalogTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/conversation/FollowTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/InteractionTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/ScreenChecks.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/ScreensTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/voice/VoiceFrameTest.kt richos/app/scripts/native-android-ui.test.sh richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/pairing/CameraFeed.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/pairing/PairingEntry.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/pairing/PairingLinkSheet.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/pairing/QrDecoder.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/pairing/PairingEntryTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/pairing/PairingWiringTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/pairing/QrAnalyzerTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/pairing/QrDecoderTest.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/composer/DraftEditor.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/composer/ComposerTypingTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/composer/DraftEditorTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/MacNameTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/SettingsFootnoteTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/AttachWiringTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/IdleFramesTest.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/design/ScrollFade.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/MockupFidelityTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/conversation/SentMessageStaysTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/conversation/RestoredReplyTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/voice/MicrophoneDeniedTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/CardsInViewTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/OutOfReachLineTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/HeaderClearsWaitingCardTest.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/conversation/QuestionCard.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/conversation/QuestionCardTest.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/design/Speckle.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/design/SpeckleTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/design/SpeckleStoreTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/SpeckleOnScreensTest.kt richos/mobile/native-android/bin/speckle-golden.mjs richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/design/EdgeWindow.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/design/EdgeCuedRegion.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/design/EdgeWindowTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/CardsRegionRestsOnWholeLinesTest.kt richos/mobile/native-android/app/src/test/kotlin/dev/richos/android/ui/conversation/HearItTest.kt richos/mobile/native-android/app/src/main/kotlin/dev/richos/android/ui/attach/RichFileViews.kt
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
NA="$ROOT/richos/mobile/native-android"
RANDROID="$NA/bin/randroid"
CATALOGS=("$NA/app/src/main/kotlin/dev/richos/android/ui/catalog/ScreenCatalog.kt" "$NA/app/src/main/kotlin/dev/richos/android/ui/catalog/AttachCatalog.kt")

if [ "${1:-}" = "--list" ]; then
  # ScreenSpec("id", group, "inv", "title" … — read from the source, so the list needs no JVM.
  grep -ho 'ScreenSpec("[a-z0-9-]*", [0-9]*, "[^"]*", "[^"]*"' "${CATALOGS[@]}" \
    | sed -E 's/ScreenSpec\("([^"]*)", ([0-9]*), "[^"]*", "([^"]*)"/g\2  \1  \3/'
  exit 0
fi

PURE=(--tests 'dev.richos.android.design.*' --tests 'dev.richos.android.ui.catalog.*'
      --tests 'dev.richos.android.ui.conversation.*' --tests 'dev.richos.android.ui.voice.*'
      --tests 'dev.richos.android.ui.InteractionTest' --tests 'dev.richos.android.ui.pairing.*'
      --tests 'dev.richos.android.ui.MacNameTest' --tests 'dev.richos.android.ui.SettingsFootnoteTest'
      --tests 'dev.richos.android.ui.AttachWiringTest' --tests 'dev.richos.android.ui.MockupFidelityTest'
      --tests 'dev.richos.android.ui.composer.*' --tests 'dev.richos.android.ui.IdleFramesTest'
      --tests 'dev.richos.android.ui.CardsInViewTest' --tests 'dev.richos.android.ui.OutOfReachLineTest'
      --tests 'dev.richos.android.ui.HeaderClearsWaitingCardTest' --tests 'dev.richos.android.ui.RefusalReasonAndCardsRoomTest'
      --tests 'dev.richos.android.ui.CardsRegionRestsOnWholeLinesTest')
EVERYTHING=("${PURE[@]}" --tests 'dev.richos.android.ui.ScreensTest' --tests 'dev.richos.android.ui.SpeckleOnScreensTest')

# The workspace default (CEO, 2026-10-01): with no selection, a teammate workspace runs only the
# classes its branch touches. `--full` (or RICHOS_TEST_SCOPE=full) is everything, as before.
FULL=""
if [ "${1:-}" = "--full" ]; then FULL=1; shift; fi
NARROW=()
if [ "$#" -eq 0 ]; then
  if ! SCOPE_OUT="$(python3 "$DIR/lib/android_ui_scope.py" workspace "$ROOT" "$DIR/$(basename "${BASH_SOURCE[0]}")" \
       ${FULL:+--full} -- "${EVERYTHING[@]}")"; then
    echo "native-android-ui: could not decide whether this is a workspace run; nothing ran" >&2
    exit 64
  fi
  SCOPE=""; SCOPE_LINE=""; SCOPE_TESTS=()
  eval "$SCOPE_OUT"
  [ -z "$SCOPE_LINE" ] || echo "$SCOPE_LINE"
  [ "$SCOPE" != refuse ] || exit 64
  for c in ${SCOPE_TESTS[@]+"${SCOPE_TESTS[@]}"}; do NARROW+=(--tests "$c"); done
fi

MACHINE_LIB="$ROOT/richos/engine/scripts/lib"
# shellcheck source=../../engine/scripts/lib/machine-settings.sh
. "$MACHINE_LIB/machine-settings.sh"
richos_machine_settings || exit 2
CACHE_ROOT="$(python3 "$MACHINE_LIB/machine_settings.py" cache)" || exit 2
SDK="${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}"
if [ ! -d "$SDK/platforms" ]; then
  echo "  NOT RUN  native-android-ui: no Android SDK under $SDK"
  exit 2
fi

# The speckled ground's reference numbers: SpeckleTest pins the caps and the point field's hashes
# that the design system's engine produces; bin/speckle-golden.mjs is how they are produced. Run it
# against the engine and require every number it prints to be the one SpeckleTest pins, so a
# change to the design system (or to the script) fails here instead of drifting silently.
SPECKLE_ENGINE="${RICHOS_DESIGN_SPECKLE:-$HOME/ab/richos-hq/design/system/speckle.js}" # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
SPECKLE_TEST="$NA/app/src/test/kotlin/dev/richos/android/design/SpeckleTest.kt"
if ! command -v node >/dev/null 2>&1; then
  echo "  NOT RUN  native-android-ui: no node on PATH for bin/speckle-golden.mjs"
  exit 2
fi
if [ ! -f "$SPECKLE_ENGINE" ]; then
  echo "  NOT RUN  native-android-ui: no design-system speckle.js at $SPECKLE_ENGINE (set RICHOS_DESIGN_SPECKLE)"
  exit 2
fi
if ! golden="$(node "$NA/bin/speckle-golden.mjs" "$SPECKLE_ENGINE")"; then
  echo "  FAIL  native-android-ui: bin/speckle-golden.mjs could not run the design system's engine at $SPECKLE_ENGINE"
  exit 1
fi
if ! printf '%s' "$golden" | node -e '
  const fs = require("fs");
  const out = JSON.parse(fs.readFileSync(0, "utf8"));
  const pinned = fs.readFileSync(process.argv[1], "utf8");
  const want = [];
  for (const t of ["dark", "light"]) want.push([t + " caps", out.caps[t].map(String).join(", ")]);
  for (const c of out.cases) {
    const at = c.theme + " " + c.canvas[0] + " x " + c.canvas[1];
    want.push([at + " case", c.canvas[0] + " to " + c.canvas[1] + ", " + c.litPixels + ", \"" + c.sha256 + "\""]);
  }
  const missing = want.filter(([, v]) => !pinned.includes(v));
  for (const [what, v] of missing) console.log("  speckle-golden: " + what + " is " + v + "; SpeckleTest.kt does not pin it");
  process.exit(missing.length ? 1 : 0);
' "$SPECKLE_TEST"; then
  echo "  FAIL  native-android-ui: SpeckleTest.kt no longer pins what the design system's engine draws (engine $SPECKLE_ENGINE); re-run node bin/speckle-golden.mjs and update the test"
  exit 1
fi
echo "  ok  speckle-golden: the design system's engine gives exactly SpeckleTest's pinned caps and point fields"

if [ "${1:-}" = "--fast" ]; then
  "$RANDROID" test app -q "${PURE[@]}"
  code=$?
  [ "$code" -eq 2 ] && { echo "  NOT RUN  native-android-ui: bin/randroid reported a missing host tool"; exit 2; }
  [ "$code" -eq 0 ] || { echo "  FAIL  native-android-ui: JVM tests exited $code"; exit 1; }
  echo '  PASS  native-android-ui: contrast (both themes), follow rule, voice frames, catalog, taps, pairing entry'
  exit 0
fi

# Where the frames go: one stable place per checkout, on the external SSD, printed at the end.
HASH="$(printf '%s' "$NA" | shasum | cut -c1-10)"
export RICHOS_SHOTS="${RICHOS_SHOTS:-$CACHE_ROOT/richos-native-android/ui-shots/$HASH}"
rm -rf "$RICHOS_SHOTS" && mkdir -p "$RICHOS_SHOTS"
export RICHOS_SCREENS=""
if [ "$#" -gt 0 ]; then
  RICHOS_SCREENS="$(IFS=,; echo "$*")"
  for want in "$@"; do
    case "$want" in
      g[0-9]*) ;;
      *) grep -q "ScreenSpec(\"$want\"" "${CATALOGS[@]}" || { echo "  FAIL  native-android-ui: no screen named '$want' (see --list)"; exit 1; } ;;
    esac
  done
fi

started=$(date +%s)
if [ "$#" -gt 0 ]; then
  "$RANDROID" test app -q --rerun --tests 'dev.richos.android.ui.ScreensTest'
elif [ "${#NARROW[@]}" -gt 0 ]; then
  "$RANDROID" test app -q --rerun "${NARROW[@]}"
else
  "$RANDROID" test app -q --rerun "${EVERYTHING[@]}"
fi
code=$?
[ "$code" -eq 2 ] && { echo "  NOT RUN  native-android-ui: bin/randroid reported a missing host tool"; exit 2; }
frames=$(find "$RICHOS_SHOTS" -name '*.png' | wc -l | tr -d ' ')
echo "  $frames frames in $RICHOS_SHOTS ($(( $(date +%s) - started )) s)"
if [ "$code" -ne 0 ]; then
  echo "  FAIL  native-android-ui: a screen check or test failed (exit $code); each one is named above (FAILED TEST), with the screen and the problem"
  exit 1
fi
if [ "$#" -gt 0 ]; then
  echo "  PASS  native-android-ui: $* rendered and checked"
elif [ "${#NARROW[@]}" -gt 0 ]; then
  echo "  PASS  native-android-ui: the workspace's test classes (${SCOPE_TESTS[*]}); everything: --full"
else
  echo '  PASS  native-android-ui: every round-12 screen and attachment screen rendered and checked, both themes, two phones, 2x text'
fi
