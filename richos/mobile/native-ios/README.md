# RichOS native iPhone app

SwiftUI, iOS 17 and later. This is the new native app. The earlier iPhone app (`mobile/ios`, `mobile/ui`) was removed on 2026-10-31; its last
state is the tag `preserved/mobile-ios-and-pwa-2026-10-22`.

**The command line comes first.** Application behavior lives in `Core/`, a Swift package that builds
and tests on this Mac with no simulator. The SwiftUI app renders the core's `AppState` and dispatches
its `Action`s; the command line dispatches the same actions and prints the same state. A logic change
is proven in seconds with `bin/rios test` or `bin/rios headless`; only screens, gestures and platform
behavior need the simulator.

## Commands

Run from anywhere; `bin/rios` finds its own checkout.

| Loop | Command | What it proves |
|---|---|---|
| L1 logic | `richos/mobile/native-ios/bin/rios test` (add `--filter <name>` for one test) | The core's unit and conformance tests, on this Mac, no simulator. |
| L1′ headless | `richos/mobile/native-ios/bin/rios headless scenario pair-by-scan` | The real core runs a scenario and prints its trace. |
| L2 simulator | `bin/rios sim prepare conv-empty`, then `sim fixture <screen>`, `sim action '<json>'`, `sim state`, `sim screenshot`, `sim verify`, `sim stop` | The same commands inside the Debug app on a simulator the CLI created. |

`bin/rios --help` lists every command. Output is JSON: `{ok, mode, command, elapsedMs, result}` on
stdout or `{ok:false, error, elapsedMs}` on stderr with exit 1 — the preserved mobile CLI's shape
(`../cli/mobile.mjs`). Actions use the preserved CLI's names and fields wherever it already names
the action (`send`, `network`, `retry`, `discard`, `pair`, `confirm-pair`, `forget-pair`, `older`, …);
the voice gesture's names are new (`voice-press`, `voice-move`, `voice-release`, …). Time and id stamps
are optional: `{"type":"send"}` is stamped on arrival, `{"type":"send","clientId":"c","at":5}` replays.

**Fixtures** are named after the round-12 screen they show — one for each of the 63 screens that are
app screens on iPhone, plus pairing v2's six (`pair-awaiting-mac`, `pair-mac-update`,
`pair-mac-refused`, `pair-mac-expired`, `pair-words-rejected`, `pair-unreachable`), which round 12
predates (`bin/rios headless fixture nope` lists them).

**Pairing is v2 only** (`../conformance/README.md`, `pairing.json` `pair_v2`, `mac_confirmation` and
`pair_wait`, `fingerprint.json` `v2`). The six words are derived on the phone from the origin it
dialed, the Mac's `ca_fingerprint_sha256` and its own key; a Mac without `pair-v2` is refused, never
fallen back to. After "They match" on the phone, the phone waits for the same press on the Mac by
asking with its own signed "They match" again; the press on the phone is the first ask. A Mac that
offers `pair-wait` holds each ask (`Prefer: wait=14`) and answers it the moment it is pressed and two
asks never start less than 7 s apart; a Mac that does not keeps one bounded schedule (2, 3, 5, 8,
13 s, then every 15 s; at most 22 asks in five minutes). At the deadline the phone asks one last
time and that answer decides. Only while the app is on screen: leaving it cancels the ask in flight
(`Core/Sources/RichOSCore/Pairing/MacWait.swift`). `sim launch <fixture>` and the launch arguments
`-rios-fixture <name> -rios-appearance dark|light` open the app straight onto one; `-rios-notifications
not-asked|on|off|denied` sets the notification state it starts in (`not-asked` shows the offer card under
the empty conversation).

**Scenarios** (`compose-draft`, `pair-by-scan`, `pair-mac-wait`, `pair-mac-hold`, `pair-refused-and-rejected`, `outbox-retry`,
`outbox-refused-continues`, `offline-reconnect`, `voice-hold-send`, `voice-lock-send`, `voice-echo-before-receipt`,
`voice-interrupted`, `voice-mic-denied`, `revoked`) carry their own checks and run identically headless and in the
simulator (`sim verify` requires byte-identical results).

## The App Review walk (CEO §107)

`bin/rios review-walk --commit <sha>` does what the review notes tell Apple's reviewer to do, on the
Release app built from exactly that commit in a leased simulator, against the live review service
(`../review-mock/README.md`): sign in on the access page in headless Chromium, get a pairing link,
paste it into the app, compare the six words, press They match in the app and on the page, send a
message and see its "Demo reply", then reset the review host. It writes
`/Volumes/E1TB/state/richos/review-walk/ios/<sha>.json` (`../perf/reviewwalk.py`), pass or fail and
`Release/testflight.ts upload` refuses an archive whose stamped commit has no passing record. The app's
half is `UITests/ReviewWalkTests.swift`, which skips unless the walk runs it; the driver is
`Tools/review-walk.mjs`. A phone already on the review host is never removed: the walk refuses.

## Physical iPhone checks

`bin/rios device build` builds the `RichOSPhysical` Release scheme with development
signing and sandbox APNs. The runner rejects a mismatch between the packaged APNs
registration setting and the signed entitlement before installing or testing. Set `RICHOS_IOS_DEVICE` to the physical UDID and
`RICHOS_APPLE_TEAM` to the signing team. Build output stays in the external cache.

For `bin/rios device verify pairing`, `device verify text`, `device verify recording`,
`device verify voice`, `device verify notifications` or `device verify quiet`, also set
`RICHOS_MOBILE_TEST_CONFIG` to the isolated lab's external JSON with
`"isolatedLab": "true"`. Pairing requires its current HTTPS `pairLink` and exact
fingerprint `words`; recording requires that same isolated session already paired.
Pairing is v2: the `words` are the ones the phone shows (over the origin it dials) and after
"They match" on the phone the runner prints `PHYSICAL_PRESS_THEY_MATCH_ON_MAC`; press "They match"
on the lab Mac within 120 seconds.
These checks drive normal Release UI and real microphone capture. They never use
fixtures or pass the lab configuration to the app. Do not run them against a personal
conversation. Pairing starts unpaired. Recording discards its first test capture and
retains the recovered termination capture for independent WAV inspection.

The `voice` check needs a unique alphabetic `spokenPhrase`. When the runner prints
`PHYSICAL_SPEAK_NOW`, speak that phrase through the actual microphone during its
12-second recording window. The check requires the Mac's transcribed acknowledgement,
then exercises Play, Stop, playback after relaunch and audio cleanup on Home. Use a
new phrase for each invocation so old history cannot satisfy the reply assertion.

The `notifications` check requires `"delayedReplies": "true"` and an isolated Mac
actually configured with an eight-second reply delay. It enables notifications through
the normal permission prompt, checks registration, then requires visible APNs previews
after Home and termination. Each notification must open its own referenced reply.
These checks send only synthetic messages to the isolated conversation. They do not
change other apps' permissions or notification settings.

The `quiet` check scrolls actual history, leaves a 30-second settled foreground
window for an optional host profiler, then spends 90 seconds on Home before
checking reading-state return and Latest. Its sleep is in the test runner. It
does not itself measure battery, CPU or network use.

The runner checks host USB continuity, bounds each check, stops on failure and rejects
skipped or missing tests. It never resets USB or retries automatically. Each invocation
retains its log and result bundle; the private test specification is removed afterward.
Trust or verification failures before launch are not successful device tests. Local
signing does not itself prove device acceptance. Release timing and energy qualification
are separate from these functional checks.

## The protocol and the shared corpus

`Core/Sources/RichOSCore/Protocol/` speaks the Mac's phone protocol: request signing, the event
stream and thread filter, the API client with the challenge rule, the courier that sends outbox
items and the pairing, push and attachment bodies. The tests read the shared conformance corpus
(`../conformance/vectors/`) in place and must pass every case the Android core passes.

Photos and files (`Core/Sources/RichOSCore/Conversation/Attachments.swift`): an outbox item with
files uploads each one, then sends its commit's exact bytes. Only the commit's 200 marks it sent; a
422 naming missing files uploads those and resends the same bytes. The app takes shares from the
Share extension into the outbox (`App/App/ShareIntake.swift`) and removes them from the Share inbox
only after the state holding them is on disk.

## Layout and ownership (build plan §5.0)

| Path | What | Stream |
|---|---|---|
| `Core/` | Swift package: `RichOSCore` (state, actions, reducers, effects, ports, protocol), `RichOSFixtures` (fixtures, scenarios, the command envelope; `#if DEBUG` only), `RichOSCLI` (`rios-cli`) | I1 |
| `bin/rios` | The one command-line entry | I1 |
| `project.yml` | XcodeGen spec; the project is generated into the cache, never committed | I1 |
| `DevBridge/` | Debug-only command mailbox inside the app; excluded from Release | I1 |
| `App/App/` | App entry, `AppStore`, the effect-handler seam, taking shares into the outbox | I1 |
| `App/Features/`, `App/Design/`, `UITests/`, `UnitTests/` | Screens, design system, UI and app tests | I2 |
| `App/Platform/`, `ShareExtension/`, `NotificationService/`, `Release/` | Microphone, recorder, notifications, Keychain signer, the Share and notification extensions, release configuration | I3 |

## Where things are written

Everything generated goes under one per-checkout cache on the external SSD:
`/Volumes/E1TB/caches/richos-native-ios/<checkout-hash>/` (override with `RICHOS_NATIVE_IOS_CACHE`,
which must stay on `/Volumes/E1TB`): SwiftPM's scratch, the Clang module cache, the generated Xcode
project, DerivedData, screenshots, logs and the headless session. Nothing is written into the tree.

## Rules this app keeps

- Core types are `Sendable` values and actors, never `@MainActor`; the one `@Observable` store lives on
  the main actor in the app. Swift 5 language mode with complete strict-concurrency checking.
- No behavior lives only in a view. A touch-rate action writes nothing to disk.
- Fixtures, scenarios and the command envelope compile only in Debug; `bin/rios sim check-release`
  proves the Release app carries none of their markers (and that the Debug app carries all of them).
- The CLI addresses only the simulator it created (its UDID is recorded in the cache); it never uses
  `booted` and `sim stop` shuts it down and deletes it.
- Permanent bundle identifier `dev.richos.connect`, approved by the CEO.
