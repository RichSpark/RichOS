#!/usr/bin/env bash
# native-ios-ui — the native iPhone app's screens (stream I2): every round-12 screen from the core's
# fixture of the same name, the visible controls, accessibility at the largest text size, contrast in
# both appearances.
#
# Two parts, fastest first:
#   1. HEADLESS (seconds, this Mac, no simulator), compiled from the app's own sources:
#      * every one of the core's 63 round-12 fixtures derives the screen round 12 names (ScreenModel),
#        and pairing v2's 6 (the wait for the press on the Mac and the ways pairing ends) theirs;
#      * the adopted T3 viewport geometry, with round 12's 80-point threshold;
#      * WCAG contrast of every text and indicator pairing, from the app's own Palette values.
#   2. SIMULATOR (minutes): builds the app and its UI tests from `native-ios/project.yml`, then runs
#      them on an iPhone SE (3rd generation) and an iPhone 16 Pro Max — each device's tests split
#      serially on a prepared simulator of each type, booted headless (never Simulator.app),
#      exclusively leased across checkouts and shut down by UDID after the run — in UTC and en_US so every picture matches the mockup's 8:02 AM.
#      Screenshots are exported per device.
#
#   native-ios-ui.test.sh                        both parts; in a teammate workspace (a cc/ branch in a
#                                                linked worktree) part 2 runs only the cases the branch
#                                                adds or changes, plus the cases its changed files are
#                                                claimed by (lib/ios_ui_scope.py), on the iPhone SE
#                                                only, and says so in one line; nothing mapped is a
#                                                refusal (exit 64) naming the full command. The merge
#                                                gate, proof-run.py and the nightlies set
#                                                RICHOS_TEST_SCOPE=full, so they always get both parts
#                                                in full (engine/scripts/lib/workspace_scope.py).
#   native-ios-ui.test.sh --full                 everything on both devices, wherever it runs
#   native-ios-ui.test.sh --headless             part 1 only
#   native-ios-ui.test.sh --only <Class/test>    part 2 scoped (xcodebuild -only-testing), repeatable,
#                                                e.g. --only ScreenshotTests/testComposerDark; the unit
#                                                bundle is named whole, --only RichOSNativeTests[/Suite]
#   native-ios-ui.test.sh --device se|pm|pro     part 2 on one device (pro: iPhone 16 Pro)
#
# Missing Xcode, the iOS runtime, xcodegen or `native-ios/project.yml` exits 2 with NOT RUN; a failure
# on a capable host is red.
# run-tests: no-host-screen: simctl and XCUITest run on simulators this suite creates, booted headless without Simulator.app
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/engine/scripts/lib/machine-settings.sh richos/mobile/native-ios/TestSupport richos/app/scripts/lib/simulator_budget.py richos/app/scripts/native-ios-ui.test.sh richos/app/scripts/lib/ios_ui_shards.py richos/engine/scripts/lib/worker_tokens.py richos/mobile/native-ios/App/Design richos/mobile/native-ios/App/Features richos/mobile/native-ios/UITests richos/mobile/native-ios/UnitTests richos/mobile/native-ios/Core/Sources/RichOSCore richos/mobile/native-ios/Core/Sources/RichOSFixtures richos/mobile/native-ios/project.yml richos/engine/scripts/lib/proc_tree.py richos/engine/scripts/lib/testdevices.py richos/app/scripts/testvm/reserve.py richos/mobile/native-ios/App/App/ShareIntake.swift richos/mobile/native-ios/App/Platform/Shared/PlatformIdentity.swift richos/mobile/native-ios/DevBridge/DevBridgeFixtureMac.swift richos/mobile/native-ios/App/Platform/NetworkMonitor.swift richos/mobile/native-ios/App richos/mobile/native-ios/Core/Sources richos/mobile/native-ios/Core/Package.swift richos/mobile/native-ios/DevBridge richos/mobile/native-ios/Release/platform.yml richos/mobile/native-ios/ShareExtension richos/mobile/native-ios/NotificationService richos/mobile/native-ios/Release/App-Info.plist richos/mobile/native-ios/Release/RichOSNative.entitlements
# run-tests: select-inputs richos/mobile/native-ios/App richos/mobile/native-ios/Core/Sources richos/mobile/native-ios/Core/Package.swift richos/mobile/native-ios/DevBridge richos/mobile/native-ios/project.yml richos/mobile/native-ios/Release/platform.yml richos/mobile/native-ios/ShareExtension richos/mobile/native-ios/NotificationService richos/mobile/native-ios/UITests richos/mobile/native-ios/UnitTests richos/mobile/native-ios/TestSupport richos/mobile/native-ios/Release/App-Info.plist richos/mobile/native-ios/Release/RichOSNative.entitlements
# run-tests: covers richos/mobile/native-ios/App/Design/Palette.swift richos/mobile/native-ios/App/Design/RoundSpec.swift richos/mobile/native-ios/App/Features/Conversation/PulseSchedule.swift richos/mobile/native-ios/App/Design/Typography.swift richos/mobile/native-ios/App/Design/Motion.swift richos/mobile/native-ios/App/Design/SVGPath.swift richos/mobile/native-ios/App/Design/Icons.swift richos/mobile/native-ios/App/Design/Mark.swift richos/mobile/native-ios/App/Design/Components.swift richos/mobile/native-ios/App/Features/Root/ScreenModel.swift richos/mobile/native-ios/App/Features/Root/Intent.swift richos/mobile/native-ios/App/Features/Root/RootView.swift richos/mobile/native-ios/App/Features/Conversation/Rows.swift richos/mobile/native-ios/App/Features/Conversation/VoiceBubble.swift richos/mobile/native-ios/App/Features/Conversation/TranscriptView.swift richos/mobile/native-ios/App/Features/Conversation/TranscriptViewportGeometry.swift richos/mobile/native-ios/App/Features/Conversation/ConversationChrome.swift richos/mobile/native-ios/App/Features/Conversation/EdgeCuedScroll.swift richos/mobile/native-ios/App/Features/Composer/ComposerView.swift richos/mobile/native-ios/App/Features/Voice/VoiceChrome.swift richos/mobile/native-ios/App/Features/Voice/TooShortLine.swift richos/mobile/native-ios/App/Features/Pairing/Takeovers.swift richos/mobile/native-ios/App/Features/Pairing/Scanner.swift richos/mobile/native-ios/App/Features/Pairing/PairingLinkSheet.swift richos/mobile/native-ios/App/Features/Settings/Overlays.swift richos/mobile/native-ios/App/Features/Attachments/AttachmentModel.swift richos/mobile/native-ios/App/Features/Attachments/AttachmentViews.swift richos/mobile/native-ios/App/Features/Attachments/PhotoScene.swift richos/mobile/native-ios/UITests/Support.swift richos/mobile/native-ios/UITests/ScreenshotTests.swift richos/mobile/native-ios/UITests/InteractionTests.swift richos/mobile/native-ios/UITests/AccessibilityLayoutTests.swift richos/mobile/native-ios/UITests/PairWaitInteractionTests.swift richos/mobile/native-ios/DevBridge/DevBridgeFixtureMac.swift richos/mobile/native-ios/UnitTests/TranscriptViewportGeometryTests.swift richos/mobile/native-ios/UnitTests/TranscriptLaunchTests.swift richos/mobile/native-ios/UnitTests/TranscriptItemIDTests.swift richos/mobile/native-ios/App/Features/Root/LaunchShell.swift richos/mobile/native-ios/App/Platform/LaunchScreen.storyboard richos/mobile/native-ios/App/Platform/Assets.xcassets/LaunchGround.colorset/Contents.json richos/mobile/native-ios/App/Platform/Assets.xcassets/LaunchIcon.imageset/Contents.json richos/mobile/native-ios/UnitTests/LaunchTimingTests.swift richos/mobile/native-ios/App/Platform/LaunchTiming.swift richos/mobile/native-ios/UnitTests/ShareIntakeTests.swift richos/mobile/native-ios/UnitTests/TooShortLineTests.swift richos/mobile/native-ios/App/App/ShareIntake.swift richos/mobile/native-ios/App/Design/SpinSchedule.swift richos/mobile/native-ios/UnitTests/ShareContextMirrorTests.swift richos/mobile/native-ios/App/Features/Conversation/ConnectionWords.swift richos/mobile/native-ios/App/Platform/NetworkMonitor.swift richos/mobile/native-ios/UnitTests/NetworkMonitorTests.swift richos/mobile/native-ios/TestSupport/BuildStamp.swift richos/mobile/native-ios/TestSupport/TestBundle-Info.plist richos/mobile/native-ios/App/Features/Conversation/QuestionCardView.swift richos/mobile/native-ios/UITests/QuestionTests.swift richos/mobile/native-ios/App/Platform/BackgroundSendTime.swift richos/mobile/native-ios/App/Design/EdgeGuard.swift richos/mobile/native-ios/UnitTests/EdgeWindowTests.swift richos/mobile/native-ios/UnitTests/RefusalReasonTests.swift richos/mobile/native-ios/UnitTests/LaunchRootTests.swift richos/mobile/native-ios/App/Features/Conversation/RichFileViews.swift
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
if [ -z "${RICHOS_WORKER_TOKENS:-}" ]; then
  exec python3 "$ROOT/richos/engine/scripts/lib/worker_tokens.py" machine -- bash "${BASH_SOURCE[0]}" "$@"
fi
NATIVE="$ROOT/richos/mobile/native-ios"
MACHINE_LIB="$ROOT/richos/engine/scripts/lib"
# shellcheck source=../../engine/scripts/lib/machine-settings.sh
. "$MACHINE_LIB/machine-settings.sh"
richos_machine_settings || exit 2
CACHE_ROOT="$(python3 "$MACHINE_LIB/machine_settings.py" cache)" || exit 2

SUITE_ARGS=("$@")
MODE=all
ONLY=()
DEVICES=("iPhone SE (3rd generation)" "iPhone 16 Pro Max")
FULL=""
HAVE_DEVICE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --headless) MODE=headless; shift ;;
    --full) FULL=1; shift ;;
    --only)
      # The UI bundle is implied; the unit bundle (UnitTests/) is named in full, so a scoped run can
      # still reach it (a full device run is longer than one foreground call).
      ONLY+=("$(python3 "$DIR/lib/ios_ui_shards.py" selector "$2")")
      shift 2 ;;
    --device)
      HAVE_DEVICE=1
      case "$2" in
        se) DEVICES=("iPhone SE (3rd generation)") ;;
        pm) DEVICES=("iPhone 16 Pro Max") ;;
        # The middle size `rios` and native-ios-app use, whose prepared OS is usually already in the
        # pool: a scoped proof on it creates no new simulator.
        pro) DEVICES=("iPhone 16 Pro") ;;
        *) echo "native-ios-ui: --device se|pm|pro" >&2; exit 64 ;;
      esac
      shift 2 ;;
    *) echo "native-ios-ui: unknown argument $1" >&2; exit 64 ;;
  esac
done

not_run() { echo "  NOT RUN  native-ios-ui: $*"; exit 2; }

command -v xcrun >/dev/null 2>&1 || not_run "Xcode command-line tools are unavailable"

KEY="$(printf '%s' "$NATIVE" | shasum -a 256 | cut -c1-10)"
CACHE="${RICHOS_NATIVE_IOS_UI_CACHE:-$CACHE_ROOT/richos-native-ios-ui/$KEY}"
CACHE="$(python3 "$MACHINE_LIB/machine_settings.py" validate "$CACHE")" || exit 2
CACHE="$(python3 -c 'import os,sys;print(os.path.realpath(sys.argv[1]))' "$CACHE")"
if [ "${RICHOS_SIMULATOR_CACHE_HELD:-}" != "$CACHE" ]; then
  exec python3 "$DIR/lib/simulator_budget.py" cache "$CACHE" -- bash "${BASH_SOURCE[0]}" ${SUITE_ARGS[@]+"${SUITE_ARGS[@]}"}
fi
mkdir -p "$CACHE"

# The workspace default (CEO, 2026-10-01: an engineer ran this with no arguments seven times for
# one layout test, 828 s each on two devices). Decided once, here, inside the cache lock, so the
# line is printed once: what you did not choose is narrowed to the branch's cases on the iPhone SE.
if [ "$MODE" != headless ]; then
  SCOPE_FLAGS=()
  [ -z "$FULL" ] || SCOPE_FLAGS+=(--full)
  [ "${#ONLY[@]}" -eq 0 ] || SCOPE_FLAGS+=(--have-only)
  [ -z "$HAVE_DEVICE" ] || SCOPE_FLAGS+=(--have-device)
  if ! SCOPE_OUT="$(python3 "$DIR/lib/ios_ui_scope.py" workspace "$ROOT" "$DIR/$(basename "${BASH_SOURCE[0]}")" \
       ${SCOPE_FLAGS[@]+"${SCOPE_FLAGS[@]}"})"; then
    echo "native-ios-ui: could not decide whether this is a workspace run; nothing ran" >&2
    exit 64
  fi
  SCOPE=""; SCOPE_LINE=""; SCOPE_DEVICE=""; SCOPE_ONLY=()
  eval "$SCOPE_OUT"
  [ -z "$SCOPE_LINE" ] || echo "$SCOPE_LINE"
  [ "$SCOPE" != refuse ] || exit 64
  if [ "$SCOPE" = narrow ]; then
    [ "$SCOPE_DEVICE" != se ] || DEVICES=("iPhone SE (3rd generation)")
    for c in ${SCOPE_ONLY[@]+"${SCOPE_ONLY[@]}"}; do
      ONLY+=("$(python3 "$DIR/lib/ios_ui_shards.py" selector "$c")")
    done
  fi
fi

WORK="$(mktemp -d "$CACHE/run.XXXXXX")"
CREATED=()
udid=""
# However the run ends: every simulator this run leased is shut down, and the work
# directory removed. Inline, as in native-ios-app.test.sh, so no trap-only function trips SC2329.
trap 'rc=$?; for udid in "${CREATED[@]:-}"; do
  [ -n "$udid" ] || continue
  python3 "$ROOT/richos/engine/scripts/lib/testdevices.py" release-ios --id "$udid" --owner-pid $$ --if-ours >/dev/null || rc=1
done
if [ "$rc" -eq 0 ]; then rm -rf "$WORK"; else echo "native-ios-ui: failure evidence retained at $WORK"; fi' EXIT

# ------------------------------------------------------------------------------------------------
# 1. Headless. The core, its fixtures and the app's Foundation-only screen sources compile into one
#    module named RichOSCore, so their `import RichOSCore` resolves to the files beside them.
# ------------------------------------------------------------------------------------------------
cat > "$WORK/main.swift" <<'SWIFT'
import Foundation
import SwiftUI

var failures = 0
func check(_ ok: Bool, _ what: String) {
    if ok { print("  ok    \(what)") } else { failures += 1; print("  FAIL  \(what)") }
}

// Every round-12 fixture draws the screen round 12 names. `expect` is keyed by id; an id the core
// adds without an expectation here is a failure, so a new fixture cannot go unchecked.
typealias M = ScreenModel
let expect: [String: (M) -> Bool] = [
    "pair-intro": { $0.takeover == .pairIntro(problem: nil) && $0.scanner == nil && $0.dialog == nil },
    "pair-scanner": { $0.scanner == .looking },
    "pair-scanner-found": { $0.scanner == .found },
    "pair-camera-denied": { $0.dialog == .cameraDenied },
    "pair-progress": { $0.takeover == .pairProgress },
    "pair-words": { if case .pairWords(let w)? = $0.takeover { return w.count == 6 } else { return false } },
    "pair-refused": { $0.takeover == .pairIntro(problem: .refused) },
    "pair-blocked": { if case .pairBlocked(let n, false)? = $0.dialog { return n >= 1 } else { return false } },
    "pair-stale": { $0.takeover == .pairStale },
    "pair-consent": { $0.takeover == .consent },
    // Pairing v2, which round 12 predates: the wait for the press on the Mac keeps the six words up,
    // and every ending, the phone's own "They do not match" and an unreachable Mac explain themselves
    // on the intro, under its own "Scan your Mac's code".
    "pair-awaiting-mac": { if case .pairAwaitingMac(let w)? = $0.takeover { return w.count == 6 } else { return false } },
    "pair-mac-update": { $0.takeover == .pairIntro(problem: .macNeedsUpdate) },
    "pair-mac-refused": { $0.takeover == .pairIntro(problem: .notAcceptedByMac) },
    "pair-mac-expired": { $0.takeover == .pairIntro(problem: .macAnswerExpired) },
    "pair-words-rejected": { $0.takeover == .pairIntro(problem: .wordsRejected) },
    "pair-unreachable": { $0.takeover == .pairIntro(problem: .macUnreachable) },
    "conv-empty": { $0.takeover == nil && $0.thread.isEmpty && $0.cards.isEmpty },
    "conv-populated": { $0.takeover == nil && $0.thread.rows.count == 10 && $0.thread.following },
    "conv-pending": { m in
        let d = m.thread.rows.compactMap(\.delivery)
        return d.contains(.sending) && d.contains(.waiting) && d.contains(.needsAttention) },
    "conv-replying": { $0.thread.rows.last?.body == .replying },
    "conv-streaming": { if case .streaming? = $0.thread.rows.last?.body { return true } else { return false } },
    "conv-playing-reply": { $0.thread.rows.contains { if case .playing? = $0.audio { return true } else { return false } } },
    "conv-preparing-reply": { $0.thread.rows.contains { $0.audio == .preparing } },
    "conv-older-loading": { $0.thread.loadingOlder && !$0.thread.following },
    "conv-beginning": { $0.thread.reachedBeginning && !$0.thread.following },
    "conv-scrolled": { !$0.thread.following && $0.thread.rows.count == 10 },
    "conv-focused": { $0.thread.focusedID != nil },
    "conv-retry": { $0.connection?.kind == .reconnecting && $0.cards.contains(.waitingToSend(count: 1)) },
    "comp-idle": { $0.takeover == nil && $0.composer.draft.isEmpty && $0.voice == nil },
    "comp-typing": { $0.composer.hasDraft && !$0.composer.focused },
    "comp-keyboard": { $0.composer.hasDraft && $0.composer.focused },
    "comp-disabled": { $0.composer.disabledReason != nil && $0.connection?.kind == .incompatible },
    "comp-too-long": { m in if case .tooLong? = m.toast { return m.composer.isTooLong } else { return false } },
    "voice-press": { $0.voice?.phase == .pressed },
    "voice-permission": { $0.voice?.phase == .pressed && $0.dialog == nil },
    "voice-holding": { $0.voice?.phase == .held && ($0.voice?.elapsedMs ?? 0) > 0 },
    "voice-slide-left": { $0.voice?.phase == .held && ($0.voice?.cancelProgress ?? 0) > 0.5 },
    "voice-bin": { $0.voice?.phase == .ending(.canceled) },
    "voice-sent": { $0.voice?.phase == .ending(.sent) },
    "voice-too-short": { $0.voice?.phase == .ending(.tooShort) && $0.toast == .tooShort },
    "voice-slide-up": { $0.voice?.phase == .held && ($0.voice?.lockProgress ?? 0) > 0.5 },
    "voice-lock-transition": { $0.voice?.phase == .locked },
    "voice-locked": { $0.voice?.phase == .locked },
    "voice-locked-scrolled": { $0.voice?.phase == .locked && !$0.thread.following },
    "voice-locked-cancel": { $0.voice?.phase == .ending(.canceled) && $0.voice?.wasLocked == true },
    "voice-locked-send": { $0.voice?.phase == .ending(.sent) && $0.voice?.wasLocked == true },
    "voice-ceiling-warning": { $0.voice?.phase == .locked && $0.toast == .ceilingWarning },
    "voice-ceiling-reached": { $0.cards.contains { if case .keptRecording(let r) = $0 { return r.reason == .ceiling } else { return false } } },
    "voice-interrupted": { $0.cards.contains { if case .keptRecording(let r) = $0 { return r.reason == .interrupted } else { return false } } },
    "rec-card": { $0.cards.contains { if case .keptRecording(let r) = $0 { return r.reason == .unsent } else { return false } } },
    "rec-unsupported": { $0.connection?.kind == .voiceUnsupported && !$0.composer.voiceAvailable },
    "rec-mic-denied": { $0.cards.contains(.microphoneDenied) },
    "conn-reconnecting": { $0.connection?.kind == .reconnecting },
    "conn-offline": { $0.connection?.kind == .phoneOffline },
    "conn-service": { $0.connection?.kind == .serviceUnavailable },
    "conn-mac": { $0.connection?.kind == .macUnreachable },
    // D05 (Android `conn-tailscale-off`): the line names the fix, the waiting message keeps its card.
    "conn-tailscale-off": { $0.connection?.kind == .tailscaleOff && $0.cards.contains(.waitingToSend(count: 1)) },
    "conn-revoked": { $0.takeover == .removedFromMac },
    "conn-incompatible": { $0.connection?.kind == .incompatible && $0.composer.disabledReason != nil },
    "conn-cached": { $0.thread.cached && $0.connection?.kind == .phoneOffline },
    "notif-offer": { $0.cards.contains(.notificationOffer) },
    "notif-settings": { if case .settings(let s)? = $0.sheet { return s.notifications == .denied } else { return false } },
    "settings": { if case .settings? = $0.sheet { return true } else { return false } },
    "settings-forget": { $0.dialog == .forget },
    "settings-forget-blocked": { if case .forgetBlocked? = $0.dialog { return true } else { return false } },
    "upd-banner": { $0.banner != nil && $0.dialog == nil && $0.takeover == nil },
    "upd-dialog": { if case .update? = $0.dialog { return true } else { return false } },
    "upd-blocking": { if case .updateRequired? = $0.takeover { return true } else { return false } },
    "upd-feature-off": { $0.connection?.kind == .voicePaused && !$0.composer.voiceAvailable },
    "launch-cached": { $0.thread.cached && $0.thread.rows.count == 10 },
    "ask-open": { if case .question(let q, nil, nil, false)? = $0.thread.rows.last?.body { return q.state == "open" && q.options.count == 2 } else { return false } },
]
check(Fixture.all.count == 63 + 6 + 2, "the core has all 63 in-app round-12 fixtures, pairing v2's 6 D05's Tailscale line and round-13's question (it has \(Fixture.all.count))")
// D05: the Tailscale line names the fix, in the words Android shows, and nothing pulses beside it.
do {
    let w = ConnectionWords.words(.tailscaleOff)
    check(w.lead == "This phone is not on Tailscale. Turn Tailscale on to reach your Mac." && w.reassurance == "Messages stay on this phone.",
          "conn-tailscale-off says: \(w.lead) \(w.reassurance)")
    check(ConnectionWords.words(.reconnecting).lead == "Reconnecting…", "Reconnecting… keeps its words")
}
// The SE fit: AccessibilityLayoutTests poses conn-tailscale-off's line because no line the header can say
// is longer. A new, longer line fails here until that test poses it.
do {
    let length = { (k: ScreenModel.ConnectionLine.Kind) -> Int in
        let w = ConnectionWords.words(k); return (w.lead + " " + w.reassurance).count
    }
    let longest = ScreenModel.ConnectionLine.Kind.allCases.max { length($0) < length($1) }
    check(longest == .tailscaleOff,
          "the header's layout test poses its longest line: conn-tailscale-off, \(length(.tailscaleOff)) characters (longest: \(String(describing: longest)))")
}
for f in Fixture.all {
    guard let rule = expect[f.name] else { check(false, "\(f.name): no expectation for this fixture"); continue }
    check(rule(ScreenModel(state: f.state)), "\(f.name) draws its round-12 screen")
}
check(Set(expect.keys) == Set(Fixture.all.map(\.name)), "every expectation names a fixture the core has")

// R1 (the iPhone re-walk, 2026-10-01): with no internet the waiting card says the phone is offline,
// not that the Mac is unreachable (the header says the same); and Forget says it also discards an
// unsent kept recording, only when there is one.
do {
    func waitingDetail(_ notice: ConnectionNotice) -> String {
        var s = try! Fixture.named("conv-retry").state
        s.connectionNotice = notice
        for case .waitingToSend(let n, let off) in ScreenModel(state: s).cards { return ScreenModel.Card.waitingDetail(count: n, phoneOffline: off) }
        return ""
    }
    let offline = waitingDetail(.phoneOffline)
    check(offline.contains("This phone is offline") && !offline.contains("Mac"), "no internet: the waiting card says the phone is offline (\(offline))")
    let unreachable = waitingDetail(.macUnreachable)
    check(unreachable.contains("Your Mac isn’t reachable"), "Mac unreachable: the waiting card still says so (\(unreachable))")
    var forget = try! Fixture.named("settings-forget").state
    check(ScreenModel(state: forget).dialog == .forget, "Forget with nothing kept: the plain dialog")
    forget.keptRecordings = [Fixture.kept(.unsent, 5_000)]
    check(ScreenModel(state: forget).dialog == .forgetDiscardsRecording, "Forget with a kept recording: the dialog that says it discards it too")
    check(ScreenModel.Dialog.discardedRecordingNote.contains("recorded but did not send"), "the note says the unsent recording is discarded")
}

// The adopted T3 viewport geometry, with round 12's 80-point threshold.
do {
    let g = TranscriptViewportGeometry(contentHeight: 1_200, viewportHeight: 400, topInset: 20, bottomInset: 100)
    check(g.bottomOffset == 900, "bottom offset includes the bottom inset")
    check(g.showsScrollToBottom(at: 820), "80 points up shows Latest")
    check(!g.showsScrollToBottom(at: 821), "79 points up does not")
    let short = TranscriptViewportGeometry(contentHeight: 100, viewportHeight: 400, topInset: 20, bottomInset: 0)
    check(!short.showsScrollToBottom(at: -20), "a short conversation never shows Latest")
    check(g.restoredBottomOffset(after: nil, maintainsBottomAnchor: true, isInteracting: false) == 900, "follows on first layout")
    check(g.restoredBottomOffset(after: g, maintainsBottomAnchor: true, isInteracting: false) == nil, "nothing changed: no jump")
    let grown = TranscriptViewportGeometry(contentHeight: 1_300, viewportHeight: 400, topInset: 20, bottomInset: 100)
    check(grown.restoredBottomOffset(after: g, maintainsBottomAnchor: true, isInteracting: false) == 1_000, "a growing reply keeps following")
    check(grown.restoredBottomOffset(after: g, maintainsBottomAnchor: false, isInteracting: false) == nil, "not following: stays put")
    check(grown.restoredBottomOffset(after: g, maintainsBottomAnchor: true, isInteracting: true) == nil, "never fights a finger")
    let keyboard = TranscriptViewportGeometry(contentHeight: 1_200, viewportHeight: 400, topInset: 20, bottomInset: 400)
    check(keyboard.restoredBottomOffset(after: g, maintainsBottomAnchor: true, isInteracting: false) == 1_200, "the keyboard keeps the newest message in view")
}

// WCAG 2 contrast of every pairing, from the Palette the app draws with.
func linear(_ c: Double) -> Double { c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4) }
func rgb(_ color: Color, over: (Double, Double, Double)? = nil) -> (Double, Double, Double) {
    let r = color.resolve(in: EnvironmentValues())
    let a = Double(r.opacity)
    let (fr, fg, fb) = (Double(r.red), Double(r.green), Double(r.blue))
    guard let b = over, a < 1 else { return (fr, fg, fb) }
    return (fr * a + b.0 * (1 - a), fg * a + b.1 * (1 - a), fb * a + b.2 * (1 - a))
}
func lum(_ c: (Double, Double, Double)) -> Double { 0.2126 * linear(c.0) + 0.7152 * linear(c.1) + 0.0722 * linear(c.2) }
func ratio(_ fg: Color, _ bg: Color, over: Color? = nil) -> Double {
    let base = rgb(bg, over: over.map { rgb($0) })
    let f = rgb(fg, over: base)
    let (l1, l2) = (lum(f), lum(base))
    return (max(l1, l2) + 0.05) / (min(l1, l2) + 0.05)
}

for p in [Palette.sovereign, Palette.daybreak] {
    let name = p.appearance == .dark ? "dark" : "light"
    func text(_ label: String, _ fg: Color, _ bg: Color, over: Color? = nil) {
        let r = ratio(fg, bg, over: over)
        check(r >= 4.5, String(format: "%@ text  %-58@ %5.2f:1 (floor 4.5)", name, label as NSString, r))
    }
    func indicator(_ label: String, _ fg: Color, _ bg: Color, over: Color? = nil) {
        let r = ratio(fg, bg, over: over)
        check(r >= 3.0, String(format: "%@ ind.  %-58@ %5.2f:1 (floor 3.0)", name, label as NSString, r))
    }
    text("ink on ground (headings, ledes, dialogs, words)", p.ink, p.ground)
    text("ink on surface (Rich's bubbles, cards, rows, Latest)", p.ink, p.surface)
    text("ink on your bubble", p.ink, p.mine)
    text("ink-soft on ground (hints, steps' details, footers)", p.inkSoft, p.ground)
    text("ink-soft on surface (placeholder, timestamps, status line)", p.inkSoft, p.surface)
    text("ink-soft on your bubble (timestamps, Sending…)", p.inkSoft, p.mine)
    text("on-signal on signal (primary buttons, Hear it, Update)", p.onSignal, p.signal)
    text("danger on ground (Not sent)", p.danger, p.ground)
    text("danger on surface (Forget this pairing)", p.danger, p.surface)
    text("eyebrow on ground (gold dark, ink light)", p.accentGlyph, p.ground)
    indicator("signal on surface (the microphone inside the capsule)", p.signal, p.surface)
    indicator("signal on ground (thinking dots, caret, reference bar)", p.signal, p.ground)
    // The play button on your bubble: gold alone in dark; with its 1 pt ink edge in light, where gold
    // on the gold-washed plane is under 3:1 (printed below).
    indicator("play button edge on your bubble", p.playEdgeOnMine ?? p.signal, p.mine)
    indicator("on-signal glyph on signal (mic, send, play)", p.onSignal, p.signal)
    indicator("danger on surface (recording dot, bin, alert mark)", p.danger, p.surface)
    indicator("ink-soft ring on surface (the +)", p.inkSoft, p.surface)
    indicator("ink knob 72% on surface (a switch that is off)", p.ink.opacity(0.72), p.surface)
    indicator("accent glyph on surface (consent and menu icons)", p.accentGlyph, p.surface)
    indicator("ink icons on surface (settings rows, lock pill)", p.ink, p.surface)
    // D8 (the iPhone walk, 2026-10-01): the outline of "Forget pairing on this phone" and of the
    // pairing-link field is `line`; in dark it was `trim` at 2.94:1.
    indicator("line on ground (the pairing-link field's outline, the sheet)", p.line, p.ground)
    indicator("line on surface (Forget pairing's outline, ghost buttons, off boxes)", p.line, p.surface)
}
// The scanner and the photo overlays are always dark (round 12 `.scanner`; attachments NOTES A5).
do {
    let ink = Palette.sovereign.ink, scene = Color(hex: 0x060A12)
    let chip = Color(hex: 0x141E34).opacity(0.85)
    let disc = Color(red: 8 / 255, green: 12 / 255, blue: 22 / 255).opacity(0.8)
    check(ratio(ink, scene) >= 4.5, String(format: "scanner caption on the scene %5.2f:1", ratio(ink, scene)))
    check(ratio(ink.opacity(0.78), scene) >= 4.5, String(format: "scanner second line on the scene %5.2f:1", ratio(ink.opacity(0.78), scene)))
    check(ratio(ink, chip, over: scene) >= 4.5, String(format: "scanner buttons %5.2f:1", ratio(ink, chip, over: scene)))
    check(ratio(Color(hex: 0xC2A35C), scene) >= 3, String(format: "gold viewfinder on the scene %5.2f:1", ratio(Color(hex: 0xC2A35C), scene)))
    check(ratio(ink, disc, over: .white) >= 4.5, String(format: "time and glyphs on a photo's disc, over white %5.2f:1", ratio(ink, disc, over: .white)))
    check(ratio(Color(hex: 0xC2A35C), disc, over: .white) >= 3, String(format: "upload ring on the disc, over white %5.2f:1", ratio(Color(hex: 0xC2A35C), disc, over: .white)))
}
// Declared, not asserted (round-12 NOTES "Declared exemptions"): the dark hairline `trim` on the
// ground and surface (GAP 1; no control is identified by a line alone), the light-mode gold halo
// behind the recording circle, and the waveform bars (a picture of the audio; its length is text).
print(String(format: "  info  dark trim line on ground %.2f:1, on surface %.2f:1 (GAP 1, declared)",
             ratio(Palette.sovereign.trim, Palette.sovereign.ground), ratio(Palette.sovereign.trim, Palette.sovereign.surface)))
print(String(format: "  info  gold alone on your bubble in light %.2f:1, so it wears the ink edge",
             ratio(Palette.daybreak.signal, Palette.daybreak.mine)))
print(String(format: "  info  waveform bar 32%% ink on your bubble: dark %.2f:1, light %.2f:1 (decoration, declared)",
             ratio(Palette.sovereign.ink.opacity(0.32), Palette.sovereign.mine), ratio(Palette.daybreak.ink.opacity(0.32), Palette.daybreak.mine)))

// The Reconnecting dot breathes as round 12 draws it for 10 seconds, then rests at full opacity and
// asks for no more frames (the lead's battery decision, the same as Android's 0b4fabba).
do {
    let P = PulseSchedule.self
    check(abs(P.opacity(elapsed: 0) - 0.35) < 1e-9, "pulse starts at 0.35")
    check(abs(P.opacity(elapsed: 0.7) - 1) < 1e-9, "pulse peaks at full opacity after 700 ms")
    check(abs(P.opacity(elapsed: 1.4) - 0.35) < 1e-9, "pulse is back at 0.35 after 1.4 s")
    check(abs(P.restsAt - 10.5) < 1e-9, "pulse rests at 10.5 s, on a peak (Android: fifteen 700 ms legs)")
    check(P.animates(elapsed: 9.9, reduceMotion: false), "pulse still breathes at 9.9 s")
    check(!P.animates(elapsed: P.restsAt, reduceMotion: false), "pulse asks for no frames once it rests")
    check(!P.animates(elapsed: 3 * 3600, reduceMotion: false), "pulse asks for no frames three hours into a sleeping Mac")
    check(P.opacity(elapsed: 3 * 3600) == 1, "a resting pulse is at full opacity")
    check(!P.animates(elapsed: 0, reduceMotion: true), "Reduce Motion: the dot never animates")
    // No discontinuity when it stops: the last frame drawn before rest is already at the peak.
    check(abs(P.opacity(elapsed: P.restsAt - 1e-6) - 1) < 1e-6, "pulse meets its rest without a jump")
}

// The spinners (`.spin` and the Sending mark) follow the dot's rule (Sage's review T4): they turn
// for at most 10 s, rest where they started, and never turn under Reduce Motion. Before, both looped
// at full rate for as long as a request lasted.
do {
    let S = SpinSchedule.self
    check(abs(S.restsAt(period: 1) - 10) < 1e-9, "the .spin ring rests after ten whole turns, at 10 s")
    check(abs(S.restsAt(period: 1.1) - 9.9) < 1e-9, "the Sending mark rests after nine whole turns, at 9.9 s")
    for period in [1.0, 1.1] {
        check(S.animates(elapsed: 5, period: period, reduceMotion: false), "a spinner (\(period) s) turns at 5 s")
        check(!S.animates(elapsed: 10, period: period, reduceMotion: false), "a spinner (\(period) s) asks for no frames at 10 s")
        check(!S.animates(elapsed: 60, period: period, reduceMotion: false), "a spinner (\(period) s) asks for no frames a minute into a slow request")
        check(!S.animates(elapsed: 0, period: period, reduceMotion: true), "Reduce Motion: a spinner (\(period) s) never turns")
        check(S.turn(elapsed: 60, period: period) == 0, "a resting spinner (\(period) s) is where it started")
        // No jump when it stops: the last frame before rest is a whole turn (within a frame).
        let last = S.turn(elapsed: S.restsAt(period: period) - 1e-6, period: period)
        check(last > 0.999, "a spinner (\(period) s) meets its rest without a jump")
    }
    check(abs(S.turn(elapsed: 0.25, period: 1) - 0.25) < 1e-9, "the .spin ring turns once a second")
}

// G11 (Urban's audit): a fixture is a still frame. The app's launch and return-to-front reports go
// through `receive`, so `conv-retry` keeps its waiting message and its "Waiting to send" card; in a
// live app the same report still reconnects and tries the outbox at once. Top-level code runs on
// the main thread, and the store is the main actor's.
MainActor.assumeIsolated {
    let retry = try! Fixture.named("conv-retry").state
    let frozen = AppStore(state: retry, runner: EffectRunner(storage: MemoryStorage()))
    frozen.effectsSuspended = true
    frozen.becameActive(at: Fixture.now + 1_000)
    let drawn = ScreenModel(state: frozen.state)
    check(drawn.cards.contains(.waitingToSend(count: 1)), "G11 conv-retry after launch: the Waiting to send card is drawn")
    check(drawn.thread.rows.last?.delivery == .waiting, "G11 conv-retry after launch: the message still reads Waiting to send")
    frozen.wentToBackground(at: Fixture.now + 2_000)
    check(frozen.state == retry, "G11 a fixture is unchanged by coming to the front and leaving it")
    let live = AppStore(state: retry, runner: EffectRunner(storage: MemoryStorage()))
    live.becameActive(at: Fixture.now + 1_000)
    // I06: the outbox moves on an open stream. Coming to the front reconnects; the message waits,
    // still, until the Mac's stream answers, and goes the moment it does.
    check(live.state.messages.last?.delivery == .waiting, "G11 a live app coming to the front reconnects and the message waits for the Mac")
    live.receive(.connected(at: Fixture.now + 1_200))
    check(live.state.messages.last?.delivery == .sending, "G11 the Mac's stream answers: the waiting message goes at once")
}

// G2 (Urban's audit §4.1): after the Mac removed this phone, "Pair again" with a message waiting asks
// the removed-state question, which never offers Send.
do {
    var removed = try! Fixture.named("pair-blocked").state
    removed.pairingProblem = nil
    removed.pairing = .revoked
    let asked = Reducer.reduce(removed, .openScanner).state
    check(ScreenModel(state: asked).dialog == .pairBlocked(waiting: 1, removed: true), "G2 removed + Pair again: the removed-state dialog")
    check(ScreenModel(state: asked).takeover == .removedFromMac, "G2 the removed screen stays under the dialog")
    let paired = try! Fixture.named("pair-blocked").state
    check(ScreenModel(state: paired).dialog == .pairBlocked(waiting: 1, removed: false), "G2 still paired: pair-blocked unchanged")
}

// G9 (the CEO, 2026-09-24: "Follow the phone"): the app's appearance mirrors the phone's, and a
// Debug fixture keeps the theme it is photographed in.
MainActor.assumeIsolated {
    let live = AppStore(state: try! Fixture.named("conv-populated").state, runner: EffectRunner(storage: MemoryStorage()))
    live.followPhone(.light)
    check(live.state.appearance == .light && ScreenModel(state: live.state).appearance == .light, "G9 a phone in light draws the app in light")
    live.followPhone(.dark)
    check(live.state.appearance == .dark, "G9 a phone back in dark draws the app in dark")
    var photographed = try! Fixture.named("conv-populated").state
    photographed.appearance = .light
    let frozen = AppStore(state: photographed, runner: EffectRunner(storage: MemoryStorage()))
    frozen.effectsSuspended = true
    frozen.followPhone(.dark)
    check(frozen.state.appearance == .light, "G9 a fixture keeps its -rios-appearance theme")
}
// The too-short line (Android `withTooShortLine`): the core clears its toast when the ending
// settles, 150 ms in, so the screen holds "Hold the button while you speak." its full 1.8 s.
do {
    var ending = try! Fixture.named("comp-idle").state
    ending.voice = VoiceSession(id: "v1", phase: .ending(.tooShort), startedAtMs: 0, nowMs: 100, recordingStartedAtMs: nil, width: 386)
    ending.toast = .tooShort
    let settled = Reducer.reduce(ending, .voiceSettled).state
    check(settled.voice == nil && settled.toast == nil, "too short: the settle frees the microphone and clears the core's toast")
    check(TooShortLine.ending(ending.voice, posed: false) == "v1", "too short: a live too-short ending latches the line")
    check(TooShortLine.ending(ending.voice, posed: true) == nil, "too short: a posed fixture never latches")
    check(TooShortLine.apply("v1", to: ScreenModel(state: settled)).toast == .tooShort, "too short: the line stays after the settle while latched")
    check(TooShortLine.apply(nil, to: ScreenModel(state: settled)).toast == nil, "too short: the line goes when the latch ends")
    var next = settled
    next.voice = VoiceSession(id: "v2", phase: .held, startedAtMs: 0, nowMs: 300, recordingStartedAtMs: 200, width: 386)
    check(TooShortLine.apply("v1", to: ScreenModel(state: next)).toast == nil, "too short: a new recording takes the line's place")
    var warned = settled
    warned.toast = .ceilingWarning
    check(TooShortLine.apply("v1", to: ScreenModel(state: warned)).toast == .ceilingWarning, "too short: another notice is never covered")
}
// G9: the Settings sheet has no appearance control (round 12.1 has none).
do {
    let source = (try? String(contentsOfFile: CommandLine.arguments[1] + "/App/Features/Settings/Overlays.swift", encoding: .utf8)) ?? "unreadable"
    check(!source.contains("Light appearance") && !source.contains(".setAppearance("), "G9 Settings draws no appearance row")
}

// App Review rehearsal 2026-10-04 row 2: "Hear it" under each of Rich's finished replies the Mac can
// read aloud (round 12.1 `.rich-audio`): every reply when the Mac offers `audio` (a real Mac's way),
// one reply when only its row says `has_audio` (the review host's demo reply); never under yours.
do {
    func isText(_ r: M.Row) -> Bool { if case .text = r.body { return true } else { return false } }
    let base = try! Fixture.named("conv-populated").state
    check(base.mac != nil, "row 2: the populated conversation is paired")
    check(!ScreenModel(state: base).thread.rows.contains { $0.audio != nil }, "row 2: a Mac that offers no audio: no Hear it")
    var offered = base
    offered.mac?.replyAudio = true
    let rows = ScreenModel(state: offered).thread.rows
    let replies = rows.filter { $0.author == .rich && isText($0) }
    check(!replies.isEmpty && replies.allSatisfy { $0.audio == .ready }, "row 2: the Mac offers audio: Hear it under every reply of Rich's (\(replies.count))")
    check(rows.filter { $0.author == .me }.allSatisfy { $0.audio == nil }, "row 2: never under your own messages")
    var one = base
    let last = one.messages.lastIndex { $0.author == .rich && $0.kind == .text && $0.question == nil }!
    one.messages[last].hasAudio = true
    let heard = ScreenModel(state: one).thread.rows.filter { $0.audio == .ready }
    check(heard.map(\.id) == [one.messages[last].id], "row 2: has_audio alone: Hear it under that reply only")
    one.playback = Playback(messageID: one.messages[last].id, phase: .playing, progress: 0.4)
    check(ScreenModel(state: one).thread.rows.first { $0.id == one.messages[last].id }?.audio == .playing(progress: 0.4),
          "row 2: pressed, the same reply plays")
}

// App Review rehearsal 2026-10-04 row 7: until the App Store listing is live it answers 404, so
// Settings draws no "Check for updates" row that opens it.
do {
    if case .settings(let s)? = ScreenModel(state: try! Fixture.named("settings").state).sheet {
        check(!AppLinks.appStoreListingLive && !s.showsUpdateCheck, "row 7: before release, Settings has no Check for updates")
    } else { check(false, "row 7: the settings fixture opens Settings") }
    let overlays = (try? String(contentsOfFile: CommandLine.arguments[1] + "/App/Features/Settings/Overlays.swift", encoding: .utf8)) ?? ""
    let guarded = overlays.components(separatedBy: "if settings.showsUpdateCheck {").dropFirst().first ?? ""
    check(guarded.prefix(120).contains("title: \"Check for updates\""), "row 7: the Check for updates row is drawn only when it is shown")
}

// App Review rehearsal 2026-10-04 rows 13 and 15: the consent screen says what the privacy policy
// says (not kept by us; Cloudflare carries the messages), and Settings has no web-app footer.
do {
    let takeovers = (try? String(contentsOfFile: CommandLine.arguments[1] + "/App/Features/Pairing/Takeovers.swift", encoding: .utf8)) ?? ""
    let consent = takeovers.components(separatedBy: "struct ConsentRows").dropFirst().first ?? ""
    check(!consent.contains("stored nowhere"), "row 13: the consent screen no longer says stored nowhere")
    check(consent.contains("not kept by us") && consent.contains("Cloudflare"), "row 13: it says not kept by us and names Cloudflare")
    let overlays = (try? String(contentsOfFile: CommandLine.arguments[1] + "/App/Features/Settings/Overlays.swift", encoding: .utf8)) ?? "unreadable"
    check(!overlays.contains("web app"), "row 15: Settings says nothing about a web app")
}

// I02 (native acceptance r1): the out-of-reach line is no longer a row under the header's fade; it is
// pinned below the header in full ink on the floating surface, computed here in both themes.
do {
    let rows = (try? String(contentsOfFile: CommandLine.arguments[1] + "/App/Features/Conversation/Rows.swift", encoding: .utf8)) ?? ""
    let line = rows.components(separatedBy: "struct OutOfReachLine").dropFirst().first ?? ""
    check(!rows.contains("cachedMarker"), "I02 the out-of-reach line is not a row of the list")
    check(line.contains(".foregroundStyle(palette.ink)") && line.contains(".floatingSurface(palette"), "I02 the line is ink on the floating surface")
    for p in [Palette.sovereign, Palette.daybreak] {
        let r = ratio(p.ink, p.surface)
        check(r >= 4.5, String(format: "I02 %@: the out-of-reach line, ink on surface %5.2f:1 (floor 4.5)", p.appearance == .dark ? "dark" : "light", r))
    }
}

// I04 (native acceptance r1): after "Not now" the offer is answered for good and Settings still holds
// the off switch whose tap asks iOS; once iOS has said no, Open Settings; while asking, nothing.
do {
    let offered = try! Fixture.named("notif-offer").state
    var answered = Reducer.reduce(offered, .dismissNotificationOffer).state
    check(!ScreenModel(state: answered).cards.contains(.notificationOffer), "I04 Not now takes the offer down")
    answered = Reducer.reduce(answered, .openSheet(.settings)).state
    if case .settings(let s)? = ScreenModel(state: answered).sheet {
        check(s.notifications.control == .toggle(isOn: false), "I04 after Not now, Settings holds an off switch (\(s.notifications.control))")
    } else { check(false, "I04 Settings opens after Not now") }
    check(M.NotificationStatus.on.control == .toggle(isOn: true) && M.NotificationStatus.off.control == .toggle(isOn: false),
          "I04 on and off keep their switch")
    check(M.NotificationStatus.denied.control == .openSettings, "I04 denied by iOS: Open Settings, not a switch that cannot work")
    check(M.NotificationStatus.turningOn.control == .none && M.NotificationStatus.unsupported.control == .none,
          "I04 asking, or impossible: no control")
}

// The + menu (App Store blocker 5): the menu, the tray, the refusal cards and the bubbles are drawn
// from the core's attachment state.
do {
    var s = try! Fixture.named("conv-populated").state
    s.attachmentLimits = try! JSONDecoder().decode(AttachmentLimits.self, from: Data(#"{"max_file_bytes":26214400,"max_files_per_message":10,"max_message_bytes":104857600,"upload_seconds":300,"media_types":["image/jpeg","application/pdf"]}"#.utf8))
    s = Reducer.reduce(s, .openAttachMenu).state
    check(ScreenModel(state: s).attach.menuOpen, "attach: + opens the menu")
    let photo = OutboxFile(id: "p1", name: "IMG_1.jpg", mediaType: "image/jpeg", byteCount: 900_000, sha256: "a", path: "pending/p1-IMG_1.jpg")
    let pdf = OutboxFile(id: "d1", name: "Brief.pdf", mediaType: "application/pdf", byteCount: 2_400_000, sha256: "b", path: "pending/d1-Brief.pdf")
    s = Reducer.reduce(s, .attachmentsPicked([photo, pdf])).state
    let dir = URL(fileURLWithPath: "/tmp/attachments")
    let tray = ScreenModel(state: s, attachments: dir).attach.pending
    check(tray.map(\.id) == ["p1", "d1"], "attach: the tray shows what was picked, in order")
    if case .photo(let p)? = tray.first { check(p.source == .file(dir.appendingPathComponent("pending/p1-IMG_1.jpg")), "attach: a tray photo shows its staged copy") }
    else { check(false, "attach: a tray photo shows its staged copy") }
    if case .file(let f)? = tray.last { check(f.summary == "PDF · 2.4 MB", "attach: a tray file reads PDF · 2.4 MB") } else { check(false, "attach: a tray file reads PDF · 2.4 MB") }
    s.draft = "For the offsite."
    s = Reducer.reduce(s, .sendDraft(clientID: "c1", at: 5_000)).state
    let rows = ScreenModel(state: s, attachments: dir).thread.rows.suffix(2)
    if case .album(let photos, let caption)? = rows.first?.body { check(photos.count == 1 && caption == "For the offsite.", "attach: sent photos are an album with the words") }
    else { check(false, "attach: sent photos are an album with the words") }
    if case .file(let f, nil)? = rows.last?.body { check(f.name == "Brief.pdf", "attach: a sent file is a file bubble") } else { check(false, "attach: a sent file is a file bubble") }
    check(rows.allSatisfy { $0.delivery == .waiting || $0.delivery == .sending }, "attach: the bubbles show their honest delivery state")
    let big = OutboxFile(id: "z", name: "Lease.pdf", mediaType: "application/pdf", byteCount: 31_000_000, sha256: "c", path: "pending/z-Lease.pdf")
    let refused = ScreenModel(state: Reducer.reduce(s, .attachmentsPicked([big])).state)
    check(refused.cards.contains(.attachRefused(name: "Lease.pdf", detail: "is 31 MB. Rich can take files up to 25 MB each.", tooLarge: true)),
          "attach: too large is refused in one card with the size and the limit")
    check(ScreenModel(state: Reducer.reduce(s, .attachPermissionDenied(.camera)).state).cards.contains(.attachCameraDenied), "attach: camera denied shows its card")
    var noMac = s
    noMac.attachmentLimits = nil
    check(ScreenModel(state: Reducer.reduce(noMac, .pickAttachments(.files)).state).cards.contains(.attachMacUnsupported), "attach: a Mac without attachments says so")
}

// D03 (native acceptance r1): the microphone-off card is the answer to a press, never the OS's answer
// alone; Not now and a grant take it down. (Not now's button is proved end to end by InteractionTests.)
do {
    let off = Reducer.reduce(try! Fixture.named("comp-idle").state, .microphonePermission(.denied)).state
    check(!ScreenModel(state: off).cards.contains(.microphoneDenied), "D03 a denial alone, before any press, draws no microphone card")
    let pressed = Reducer.reduce(off, .voicePress(id: "v", width: 386, at: 1)).state
    check(ScreenModel(state: pressed).cards.contains(.microphoneDenied), "D03 a press with the microphone off draws the card")
    check(!ScreenModel(state: Reducer.reduce(pressed, .dismissMicrophoneCard).state).cards.contains(.microphoneDenied), "D03 Not now takes the card down")
    check(!ScreenModel(state: Reducer.reduce(pressed, .microphonePermission(.granted)).state).cards.contains(.microphoneDenied), "D03 a grant takes the card down")
}
// I05 (native acceptance r1, the physical iPhone): with "Waiting to send" already showing, the
// microphone-off card stacks under it, and the region above the composer brings it into view.
// (The scroll itself is proved on a simulator by InteractionTests.)
do {
    let off = Reducer.reduce(try! Fixture.named("conv-retry").state, .microphonePermission(.denied)).state
    let before = ScreenModel(state: off).cards
    let after = ScreenModel(state: Reducer.reduce(off, .voicePress(id: "v", width: 386, at: 1)).state).cards
    check(before == [.waitingToSend(count: 1)], "I05 conv-retry with the microphone off shows only Waiting to send")
    check(after == [.waitingToSend(count: 1), .microphoneDenied], "I05 a press stacks the microphone-off card under Waiting to send")
    check(ScreenModel.Card.raised(before: before, after: after) == ScreenModel.Card.microphoneDenied.id,
          "I05 the region brings the microphone-off card into view")
    check(ScreenModel.Card.raised(before: after, after: before) == nil, "I05 a card going away moves nothing")
    check(ScreenModel.Card.raised(before: after, after: after) == nil, "I05 an unchanged stack moves nothing")
    check(ScreenModel.Card.raised(before: [.waitingToSend(count: 1)], after: [.waitingToSend(count: 2)]) == nil,
          "I05 a new count on a card already showing is not a new card")
}

// G7 (Urban's audit): the CEO's how-to paragraph on conv-empty is full ink under a hairline, at a
// 300 pt measure with 22 + 20 pt around the rule, and reads at AA in both themes.
for p in [Palette.sovereign, Palette.daybreak] {
    let theme = p.appearance == .dark ? "dark" : "light"
    check(RoundSpec.howToInk(p) == p.ink, "G7 \(theme): the how-to paragraph is full ink, not ink-soft")
    check(RoundSpec.howToRule(p) == p.lineFaint, "G7 \(theme): the hairline is line-faint")
    check(ratio(RoundSpec.howToInk(p), p.ground) >= 4.5, String(format: "G7 \(theme): how-to paragraph on ground %5.2f:1", ratio(RoundSpec.howToInk(p), p.ground)))
}
check(RoundSpec.howToMeasure == 300 && RoundSpec.howToRuleGap == 22 && RoundSpec.howToTextGap == 20 && RoundSpec.howToLineHeight == 1.55,
      "G7 measure 300, 22 above the rule, 20 below it, line height 1.55 (app.css .mic-how)")

// G13 (Urban's audit): the disabled orb is round 12.1's gold orb dimmed to 45%, not an outlined
// ring. Declared exemption, printed rather than asserted: an inactive control (WCAG 1.4.11).
for p in [Palette.sovereign, Palette.daybreak] {
    let theme = p.appearance == .dark ? "dark" : "light"
    check(RoundSpec.disabledOrbFill(p) == p.signal && RoundSpec.disabledOrbOpacity == 0.45, "G13 \(theme): the disabled orb is gold at 45% (app.js setDisabled)")
    print(String(format: "  info  G13 \(theme): dimmed orb on surface %.2f:1 (inactive control, declared exemption)",
                 ratio(p.signal.opacity(RoundSpec.disabledOrbOpacity), p.surface)))
}

// THE LAUNCH SCREEN (the CEO, 2026-10-02: "From the moment any part of the app is visible, there must be
// something useful or pleasing on screen; a blank or white screen never is"). iOS's generated launch
// screen (`UILaunchScreen_Generation`, or an empty `UILaunchScreen`) is an empty window, white in light
// mode, shown from the tap to the app's first frame: refused here, as Android's SpeckleTest refuses a
// launch window that is not its launch ground. The launch screen is LaunchScreen.storyboard, ported
// from Android's: the app icon centered on the ground of each look, and the app's first frame
// (LaunchShell.swift) draws the same picture at the same size.
do {
    let root = CommandLine.arguments[1]
    func read(_ path: String) -> String { (try? String(contentsOfFile: root + "/" + path, encoding: .utf8)) ?? "" }
    let project = read("project.yml") + read("Release/platform.yml")
    check(!project.contains("UILaunchScreen_Generation") && !project.contains("UILaunchScreen:"),
          "launch screen: not iOS's generated, empty one (no UILaunchScreen_Generation, no UILaunchScreen)")
    check(project.contains("INFOPLIST_KEY_UILaunchStoryboardName: LaunchScreen"),
          "launch screen: the app names LaunchScreen.storyboard")
    let board = read("App/Platform/LaunchScreen.storyboard")
    check(board.contains("launchScreen=\"YES\""), "launch screen: App/Platform/LaunchScreen.storyboard is a launch screen")
    check(board.contains("<color key=\"backgroundColor\" name=\"LaunchGround\"/>"), "launch screen: its window is the ground")
    // What the storyboard draws, without its comments (which may name what it never draws).
    let drawn = board.replacingOccurrences(of: "<!--[\\s\\S]*?-->", with: "", options: .regularExpression)
    check(!["<label", "<textView", "<textField", "<button", "speckle", "Speckle"].contains { drawn.contains($0) },
          "launch screen: no text, no controls (HIG), never the speckle")
    let images = drawn.components(separatedBy: "image=\"").dropFirst().compactMap { $0.split(separator: "\"").first.map(String.init) }
    check(images == ["LaunchIcon"], "launch screen: draws one image, the app icon LaunchIcon (drew \(images))")
    let iconSet = "App/Platform/Assets.xcassets/LaunchIcon.imageset/"
    let iconFiles = ["LaunchIcon-light@2x.png", "LaunchIcon-dark@2x.png", "LaunchIcon-light@3x.png", "LaunchIcon-dark@3x.png"]
    check(iconFiles.allSatisfy { read(iconSet + "Contents.json").contains("\"\($0)\"") && FileManager.default.fileExists(atPath: root + "/" + iconSet + $0) },
          "launch screen: LaunchIcon has its light and dark art at 2x and 3x (\(iconFiles.joined(separator: ", ")); Tools/launch-icon.py renders it from AppIcon-1024.png)")
    // The icon is drawn at its own size, fixed as Android's is: LaunchShell.iconPoints, the size the
    // storyboard declares, and every rendering's pixels over its scale. Sized by a multiplier on the
    // screen's width, the test iPhone's launch picture came out without it (2026-10-02).
    func number(_ text: String, _ pattern: String) -> String? {
        guard let r = text.range(of: pattern, options: .regularExpression) else { return nil }
        return text[r].components(separatedBy: CharacterSet(charactersIn: "0123456789.").inverted).filter { !$0.isEmpty }.last
    }
    func pngWidth(_ path: String) -> Int? {
        guard let d = FileManager.default.contents(atPath: root + "/" + path), d.count > 24 else { return nil }
        return d[16..<20].reduce(0) { $0 << 8 | Int($1) }
    }
    let points = number(read("App/Features/Root/LaunchShell.swift"), "iconPoints: CGFloat = [0-9]+").flatMap { Int($0) }
    let declared = number(drawn, "<image name=\"LaunchIcon\" width=\"[0-9]+").flatMap { Int($0) }
    let rendered = iconFiles.map { f in pngWidth(iconSet + f).map { $0 / (f.contains("@3x") ? 3 : 2) } }
    check(points != nil && declared == points && rendered.allSatisfy { $0 == points },
          "launch screen: the icon is LaunchShell.iconPoints (\(points.map(String.init) ?? "none") pt) as the storyboard declares it (\(declared.map(String.init) ?? "none")) and as its art is rendered (\(rendered.map { $0.map(String.init) ?? "none" }))")
    check(!drawn.contains("firstItem=\"LSi-cn-ivw\" firstAttribute=\"width\"") && !drawn.contains("firstItem=\"LSi-cn-ivw\" firstAttribute=\"height\"")
          && !drawn.contains("multiplier="),
          "launch screen: the icon has no size constraint and nothing is sized by a multiplier (the image's own size)")
    check(drawn.contains("firstAttribute=\"centerX\" secondItem=\"LSr-oo-tvw\" secondAttribute=\"centerX\"")
          && drawn.contains("firstAttribute=\"centerY\" secondItem=\"LSr-oo-tvw\" secondAttribute=\"centerY\""),
          "launch screen: the icon is centered on the whole screen, as Android centers it")
    // The ground of each look is the palette's ground, to the level.
    let colorset = read("App/Platform/Assets.xcassets/LaunchGround.colorset/Contents.json")
    let colors = ((try? JSONSerialization.jsonObject(with: Data(colorset.utf8))) as? [String: Any])?["colors"] as? [[String: Any]] ?? []
    func level(_ value: Any?) -> Int? {
        guard let s = value as? String else { return nil }
        return s.hasPrefix("0x") ? Int(s.dropFirst(2), radix: 16) : Double(s).map { Int(($0 * 255).rounded()) }
    }
    for (look, palette) in [("light", Palette.daybreak), ("dark", Palette.sovereign)] {
        let entry = colors.first { (($0["appearances"] as? [[String: String]])?.first?["value"] ?? "light") == look }
        let c = ((entry?["color"] as? [String: Any])?["components"] as? [String: Any]) ?? [:]
        let want = rgb(palette.ground)
        let ok = [("red", want.0), ("green", want.1), ("blue", want.2)].allSatisfy { key, v in
            level(c[key]) == Int((v * 255).rounded()) }
        check(ok && (entry?["color"] as? [String: Any])?["color-space"] as? String == "srgb",
              "launch screen: the \(look) window is the \(look) ground \(c) (Palette.\(look == "light" ? "daybreak" : "sovereign").ground)")
    }
}

if failures > 0 { print("  \(failures) headless check(s) failed"); exit(1) }
print("  headless: all checks passed")
SWIFT

HEADLESS_BIN="$WORK/headless"
START=$(date +%s)
CORE_SOURCES=()
while IFS= read -r f; do CORE_SOURCES+=("$f"); done < <(find "$NATIVE/Core/Sources/RichOSCore" "$NATIVE/Core/Sources/RichOSFixtures" -name '*.swift' | sort)
if ! xcrun swiftc -Onone -D DEBUG -module-name RichOSCore -target "$(uname -m)-apple-macos14" \
    -module-cache-path "$CACHE/module-cache" -suppress-warnings \
    "${CORE_SOURCES[@]}" \
    "$NATIVE/App/Design/Palette.swift" \
    "$NATIVE/App/Design/RoundSpec.swift" \
    "$NATIVE/App/Features/Root/ScreenModel.swift" \
    "$NATIVE/App/Features/Conversation/ConnectionWords.swift" \
    "$NATIVE/App/Features/Attachments/AttachmentModel.swift" \
    "$NATIVE/App/Features/Conversation/TranscriptViewportGeometry.swift" \
    "$NATIVE/App/Features/Conversation/PulseSchedule.swift" \
    "$NATIVE/App/Features/Voice/TooShortLine.swift" \
    "$NATIVE/App/Design/SpinSchedule.swift" \
    "$NATIVE/App/Platform/Shared/PlatformIdentity.swift" \
    "$WORK/main.swift" -o "$HEADLESS_BIN" > "$WORK/headless-build.log" 2>&1; then
  tail -30 "$WORK/headless-build.log"
  echo "  FAIL  native-ios-ui: the headless checks did not compile"
  exit 1
fi
echo "native-ios-ui: headless (compiled in $(( $(date +%s) - START )) s)"
"$HEADLESS_BIN" "$NATIVE"
# The split and the by-name proof the simulator part relies on, against fixtures (no simulator).
if ! python3 "$DIR/lib/ios_ui_shards.py" selftest; then
  echo "  FAIL  native-ios-ui: the shard split or its proof"; exit 1
fi

[ "$MODE" = headless ] && exit 0
python3 "$ROOT/richos/engine/scripts/lib/cpu_guard.py" check-ios || exit 2

# ------------------------------------------------------------------------------------------------
# 2. Simulator
# ------------------------------------------------------------------------------------------------
[ -f "$NATIVE/project.yml" ] || not_run "native-ios/project.yml is not in this tree yet (stream I1)"
command -v xcodegen >/dev/null 2>&1 || not_run "xcodegen is not installed"
RUNTIME="$(xcrun simctl list runtimes available | sed -n 's/.*\(com\.apple\.CoreSimulator\.SimRuntime\.iOS-[0-9-]*\).*/\1/p' | tail -1)"
[ -n "$RUNTIME" ] || not_run "no iOS simulator runtime is installed"

PROJECT_DIR="$CACHE/project"
mkdir -p "$PROJECT_DIR"
# Generated exactly as `bin/rios` generates it: no --project-root, and RICHOS_NATIVE_IOS_ROOT set, which
# I3's Release/platform.yml expands into the extensions' entitlement paths.
export RICHOS_NATIVE_IOS_ROOT="$NATIVE"
xcodegen generate --spec "$NATIVE/project.yml" --project "$PROJECT_DIR" --quiet
DERIVED="$CACHE/derived"
SHOTS="$CACHE/screenshots/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$SHOTS"

START=$(date +%s)
# The retained result bundles must match this checkout's actual source, selection,
# toolchain and device profile. A matching HEAD alone never permits case reuse.
SELECT=("-only-testing:RichOSNativeUITests" "-only-testing:RichOSNativeTests")
[ "${#ONLY[@]}" -gt 0 ] && SELECT=("${ONLY[@]}")
DEVICE_JSON="$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1:]))' "${DEVICES[@]}")"
BUILD_STAMP="$(python3 "$DIR/lib/ios_ui_shards.py" identity "$ROOT" "$RUNTIME" "$DEVICE_JSON" "${SELECT[@]}")"
printf '%s\n' "$BUILD_STAMP" > "$WORK/build-stamp.txt"
PREVIOUS=""
if [ -f "$CACHE/last-work.txt" ]; then PREVIOUS="$(cat "$CACHE/last-work.txt")"; fi
printf '%s\n' "$WORK" > "$CACHE/last-work.txt"
# CEO, 2026-09-22: native builds run at lowered priority for this build window.
nice -n 10 python3 "$ROOT/richos/engine/scripts/lib/native-work.py" -- xcodebuild -project "$PROJECT_DIR/RichOSNative.xcodeproj" -scheme RichOSNative \
  -destination 'generic/platform=iOS Simulator' -derivedDataPath "$DERIVED" \
  -clonedSourcePackagesDirPath "$CACHE/SourcePackages" CODE_SIGN_IDENTITY=- RICHOS_BUILD_STAMP="$BUILD_STAMP" \
  build-for-testing > "$WORK/build.log" 2>&1 || {
    { grep -E ' error: |error:' "$WORK/build.log" || tail -30 "$WORK/build.log"; } | head -30
    echo "  FAIL  native-ios-ui: build"; exit 1; }
echo "native-ios-ui: built for testing in $(( $(date +%s) - START )) s"

STATUS=0
# ------------------------------------------------------------------------------------------------
# Reuse a prepared OS per device/runtime, with one active simulator lease.
# Run the complete test selection serially on each requested device. Retaining
# the OS avoids paying first-boot indexing on every proof and every checkout.
SHARDS=1.
# ------------------------------------------------------------------------------------------------
SHARDS=1
# Reuse one prepared device at a time; splitting must not create cold simulators.
XCTESTRUN="$(find "$DERIVED/Build/Products" -maxdepth 1 -name '*.xctestrun' | head -1)"
[ -n "$XCTESTRUN" ] || { echo "  FAIL  native-ios-ui: build-for-testing left no .xctestrun in $DERIVED/Build/Products"; exit 1; }
# Per-test seconds from the last run of this checkout, so the split is balanced by what each test
# costs rather than by how many there are. Missing on a first run: the split is then by count.
TIMES="$CACHE/test-seconds.tsv"

# Create destinations for enumeration; boot only after acquiring a worker lease.
SIM_DEV=(); SIM_UDID=(); SIM_TYPE=()
for DEVICE in "${DEVICES[@]}"; do
  TYPE="com.apple.CoreSimulator.SimDeviceType.$(printf '%s' "$DEVICE" | sed 's/[() ]/-/g; s/--*/-/g; s/-$//')"
  s=1
  while [ "$s" -le "$SHARDS" ]; do
    SIM_DEV+=("$DEVICE"); SIM_UDID+=(""); SIM_TYPE+=("$TYPE")
    s=$((s + 1))
  done
done
if ! python3 "$DIR/lib/ios_ui_shards.py" split "$WORK" "$SHARDS" "$TIMES" "${SELECT[@]}"; then
  echo "  FAIL  native-ios-ui: the test list could not be split"; exit 1
fi

# One lease covers boot, UI execution and shutdown. Idle booted simulators are not free.
cat > "$WORK/run-simulator.sh" <<'SIMULATOR'
#!/usr/bin/env bash
set -euo pipefail
work="$1"; xctestrun="$2"; i="$3"; udid="$4"; shift 4
trap 'rc=$?; python3 "$RICHOS_TESTDEVICES" release-ios --id "$udid" --owner-pid "$RICHOS_TEST_DEVICE_OWNER_PID" >/dev/null || rc=1
  exit "$rc"' EXIT
python3 "$RICHOS_TESTDEVICES" boot-ios --id "$udid"
xcrun simctl spawn "$udid" defaults write .GlobalPreferences AppleLocale -string en_US
xcrun simctl spawn "$udid" defaults write .GlobalPreferences AppleLanguages -array en-US
xcrun simctl privacy "$udid" grant microphone dev.richos.connect >/dev/null 2>&1 || true
TZ=UTC python3 "$RICHOS_NATIVE_WORK" -- xcodebuild test-without-building -xctestrun "$xctestrun" -destination "id=$udid" \
  -derivedDataPath "$work/dd-$i" -resultBundlePath "$work/result-$i.xcresult" \
  "$@"
SIMULATOR
export RICHOS_TESTDEVICES="$ROOT/richos/engine/scripts/lib/testdevices.py"
export RICHOS_TEST_DEVICE_OWNER_PID=$$
export RICHOS_NATIVE_WORK="$ROOT/richos/engine/scripts/lib/native-work.py"
START=$(date +%s)
for i in "${!SIM_UDID[@]}"; do
  s=$(( i % SHARDS + 1 ))
  ARGS=()
  while IFS= read -r a; do [ -n "$a" ] && ARGS+=("$a"); done < "$WORK/shard-$s.args"
  # A completed device needs no simulator on an unchanged retry. Its original
  # enumeration and result bundles are checked against the same input identity.
  if [ -n "$PREVIOUS" ] && [ -f "$PREVIOUS/tests-$i.json" ] && \
     cmp -s "$WORK/build-stamp.txt" "$PREVIOUS/build-stamp.txt"; then
    cp "$PREVIOUS/tests-$i.json" "$WORK/tests-$i.json"
    python3 "$DIR/lib/ios_ui_shards.py" retry "$WORK" "$PREVIOUS" "$i"
    if [ ! -s "$WORK/retry-$i.args" ]; then
      touch "$WORK/reused-$i"
      echo 0 > "$WORK/test-$i.rc"
      continue
    fi
  fi
  # A declared UI-suite lease: its whole selection runs on one device, 787-884 s under load on
  # 2026-09-24, so its lifetime is the engine's declared ui-suite ceiling rather than the default.
  SIM_UDID[i]="$(python3 "$RICHOS_TESTDEVICES" acquire-ios --type "${SIM_TYPE[$i]}" --runtime "$RUNTIME" \
    --owner-pid $$ --purpose ui-suite 2>"$WORK/lease.err")" || {
    cat "$WORK/lease.err" >&2
    if grep -qF 'prepared simulator is leased by another run' "$WORK/lease.err"; then
      not_run "prepared simulator is leased by another run; no UI test ran on this device"
    fi
    echo "  FAIL  native-ios-ui: simulator acquisition"; exit 1
  }
  CREATED+=("${SIM_UDID[$i]}")
  # Enumeration does not execute tests. It proves even unrun/missing cases remain
  # obligations, rather than relying on whatever a failed bundle happened to list.
  python3 "$RICHOS_NATIVE_WORK" -- xcodebuild test-without-building -xctestrun "$XCTESTRUN" \
    -destination "id=${SIM_UDID[$i]}" "${SELECT[@]}" -enumerate-tests \
    -test-enumeration-style flat -test-enumeration-format json \
    -test-enumeration-output-path "$WORK/tests-$i.json" > "$WORK/enumerate-$i.log" 2>&1 || {
      cat "$WORK/enumerate-$i.log"; echo "  FAIL  native-ios-ui: test enumeration"; exit 1; }
  python3 "$DIR/lib/ios_ui_shards.py" retry "$WORK" "$PREVIOUS" "$i"
  ARGS=()
  while IFS= read -r a; do [ -n "$a" ] && ARGS+=("$a"); done < "$WORK/retry-$i.args"
  if [ "${#ARGS[@]}" -eq 0 ]; then
    touch "$WORK/reused-$i"
    echo 0 > "$WORK/test-$i.rc"
    python3 "$RICHOS_TESTDEVICES" release-ios --id "${SIM_UDID[$i]}" --owner-pid $$ --if-ours >/dev/null
    CREATED=("${CREATED[@]/${SIM_UDID[$i]}}")
    continue
  fi

  # Under proof-run.py (RICHOS_WORKER_TOKENS set), the simulators count against the run the way a
  # mutation pool's workers do (richos/engine/scripts/lib/worker_tokens.py): one runs on this
  # suite's own free slot, whichever simulator holds it, and every other on a token of the budget.
  TOKEN=()
  if [ -n "${RICHOS_WORKER_TOKENS:-}" ]; then
    TOKEN=(python3 "${RICHOS_WORKER_TOKENS_TOOL:-$ROOT/richos/engine/scripts/lib/worker_tokens.py}" run
           "$RICHOS_WORKER_TOKENS" --free "$WORK/free.lock" --)
  fi
  ( T0=$(date +%s)
    # run-active renews the lease's inactivity clock for exactly as long as this device's boot and
    # xcodebuild run live (esc-20260924T220236Z-52fae3ec: nothing renewed it, so the collector shut
    # the simulator down five minutes into the suite). It never extends the lease's lifetime.
    # A lease that ends mid-run ends the run at once (exit 75, test-$i.lost): NOT RUN, never a verdict.
    if ${TOKEN[@]+"${TOKEN[@]}"} python3 "$RICHOS_TESTDEVICES" run-active --kind ios-simulator \
         --id "${SIM_UDID[$i]}" --owner-pid $$ --lost-file "$WORK/test-$i.lost" -- bash "$WORK/run-simulator.sh" "$WORK" "$XCTESTRUN" "$i" \
         "${SIM_UDID[$i]}" "${ARGS[@]}" > "$WORK/test-$i.log" 2>&1; then rc=0; else rc=$?; fi
    echo "$rc" > "$WORK/test-$i.rc"; echo $(( $(date +%s) - T0 )) > "$WORK/test-$i.secs" )
done
wait

# Per device: every shard green, and the tests its bundles report are exactly the tests listed.
# 0 every device green; 2 a device NOT RUN (its lease ended mid-run) and nothing failed; 1 a failure.
if python3 "$DIR/lib/ios_ui_shards.py" verify "$WORK" "$SHARDS" "$TIMES" "${DEVICES[@]}"; then STATUS=0; else STATUS=$?; fi
[ "$STATUS" -eq 0 ] || [ "$STATUS" -eq 2 ] || STATUS=1
for i in "${!SIM_UDID[@]}"; do
  if [ "$(cat "$WORK/test-$i.rc" 2>/dev/null)" != 0 ]; then
    { grep -E "error:|Test Case .* failed" "$WORK/test-$i.log" || true; } | head -40
  fi
done
echo "native-ios-ui: every simulator finished in $(( $(date +%s) - START )) s"

# Screenshots, per device, named after their screen (`se-dark-conv-populated.png`) so each sits
# beside the mockup of the same name; the failure evidence keeps its test's name.
for i in "${!SIM_UDID[@]}"; do
  OUT="$SHOTS/${SIM_TYPE[$i]##*.}"
  mkdir -p "$OUT/shard-$i"
  xcrun xcresulttool export attachments --path "$WORK/result-$i.xcresult" --output-path "$OUT/shard-$i" > /dev/null 2>&1 || true
  python3 "$DIR/lib/ios_ui_shards.py" name-shots "$OUT/shard-$i" "$OUT" || true
  rm -rf "$OUT/shard-$i"
done
# A failure (STATUS 1; 2 is NOT RUN, a lost lease, with nothing failed): every failing test by
# name, and each simulator's bundle and log copied to the run's results (lib/test_results.py),
# after the screenshots above were taken from the bundles.
if [ "$STATUS" -eq 1 ]; then
  KEEP=()
  for i in "${!SIM_UDID[@]}"; do
    KEEP+=(--collect "test-$i.log=$WORK/test-$i.log")
    [ -d "$WORK/result-$i.xcresult" ] && KEEP+=(--collect "result-$i.xcresult=$WORK/result-$i.xcresult")
  done
  python3 "$DIR/lib/test_results.py" report --label native-ios-ui \
    ${RICHOS_TEST_RESULTS_DIR:+--into "$RICHOS_TEST_RESULTS_DIR"} "${KEEP[@]}"
fi
# Each device's own run released it in run-simulator.sh's trap, so the pool may already have
# leased it to another run: --if-ours leaves that run's device alone.
for i in "${!SIM_UDID[@]}"; do
  python3 "$RICHOS_TESTDEVICES" release-ios --id "${SIM_UDID[$i]}" --owner-pid $$ --if-ours >/dev/null
  CREATED=("${CREATED[@]/${SIM_UDID[$i]}}")
done
if [ "$STATUS" -eq 0 ]; then rm -f "$CACHE/last-work.txt"; fi
echo "native-ios-ui: screenshots in $SHOTS"
exit "$STATUS"
