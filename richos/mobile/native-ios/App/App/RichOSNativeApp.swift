import SwiftUI
import UIKit
import RichOSCore

/// The app entry. It loads the store, wires the effect handlers, ticks the core's clock when time is
/// owed while the app is on screen, tells the core when the app comes and goes, starts the Debug development
/// bridge, and hands the store to the screens.
///
/// COMPOSITION SEAM: the screens stream's (I2) `RootView` (`App/Features/Root/`) takes the state and
/// a send function, never the store type, so the screens depend only on the core.
/// EFFECT SEAM: the platform adapter (stream I3, `App/Platform/`) handles the microphone, recorder,
/// notifications and Settings, and hands every network effect to the core's `NetworkEffects`.
@main
struct RichOSNativeApp: App {
    @UIApplicationDelegateAdaptor(PlatformAppDelegate.self) private var appDelegate
    @Environment(\.scenePhase) private var scenePhase
    @State private var store: AppStore?
    @State private var platform: PlatformEffects?
    @State private var networkMonitor = NetworkMonitor()

    /// The app's first line of code: `LaunchTiming` notes the process's start here, only on a phone
    /// being measured (one file-existence check otherwise); and the saved state starts loading here
    /// (`Boot`), while UIKit and SwiftUI build the scene, instead of after the first frame.
    init() {
        LaunchTiming.start()
        Boot.start()
    }

    /// When the core is next owed a `tick`, while the app is on screen; `nil` otherwise.
    private var nextTick: Int64? {
        guard scenePhase == .active, let store, store.ticking else { return nil }
        return TickSchedule.nextTick(store.state)
    }

    /// What the Share extension is told, derived from the state (`SharePlatform.context`); `nil`
    /// before the store has loaded. Small and cheap to compare, unlike the whole state.
    private var shareContext: ShareContext? {
        guard let state = store?.state else { return nil }
        return SharePlatform.context(for: state, macAcceptsAttachments: state.attachmentLimits != nil, limits: state.attachmentLimits)
    }

    /// The replies the person has read in the open conversation, only while the app is on screen
    /// (`ReadReplies`, D04); `nil` otherwise, so every return to the front is a change.
    private var readReplies: ReadReplies.Read? {
        guard scenePhase == .active, let state = store?.state else { return nil }
        return ReadReplies.of(state)
    }

    private func usefulMarker(_ store: AppStore) -> some View {
        let state = store.state
        let revision = UsefulFrameMarker.Revision(count: state.messages.count, last: state.messages.last, reply: state.reply)
        return UsefulFrameMarker(active: scenePhase == .active, revision: revision)
            .frame(width: 1, height: 1).allowsHitTesting(false).accessibilityHidden(true)
    }

    private func mirrorPermissions(into store: AppStore) {
        #if DEBUG
        if DevBridge.interactiveFixture { return }
        #endif
        PlatformEffects.permissionMirror().forEach { store.send($0) }
        // Notifications: iOS's answer read once per return to the front, so a change made in iPhone
        // Settings is the truth here too (I04). An OS report, so it goes through `receive`.
        Task { @MainActor in
            let system = await NotificationPlatform.shared.systemPermission()
            if let action = NotificationPermissionCheck.action(system: system, state: store.state) { store.receive(action) }
        }
    }

    var body: some Scene {
        WindowGroup {
            Group {
                if let store {
                    RootView(state: store.state, send: { store.send($0) })
                        // Install lifecycle observation only once the store exists. The launch
                        // task can suspend across activation, so its captured scenePhase is stale.
                        .onChange(of: scenePhase, initial: true) { _, phase in
                            switch phase {
                            case .active:
                                networkMonitor.start(store: store)
                                mirrorPermissions(into: store)
                                store.followPhone(SystemAppearance.current())
                                store.becameActive(at: SystemClock().nowMs())
                                Task { await ShareIntake.takeWaiting(into: store, nowMs: SystemClock().nowMs()) }
                            case .background:
                                networkMonitor.stop()
                                store.wentToBackground(at: SystemClock().nowMs())
                            default: break
                            }
                        }
                        .overlay(alignment: .topLeading) { usefulMarker(store) }
                        .safeAreaInset(edge: .top) {
                            if let problem = store.persistenceProblem ?? store.recordingProblem {
                                Text(problem)
                                    .font(.body)
                                    .padding()
                                    .frame(maxWidth: .infinity)
                                    .background(.regularMaterial)
                                    .accessibilityIdentifier("storage.problem")
                            }
                        }
                } else {
                    // Until the saved state is read: the launch screen (the app icon on the launch
                    // ground), drawn live, so the first frame continues it exactly until the conversation
                    // appears; and never longer than Boot's bound on the logo: past it the conversation's
                    // own screen shows, quiet, until the read lands (LaunchRoot, CEO §105).
                    LaunchRoot(quietAfter: Boot.quietBudget())
                }
            }
            .modifier(PhoneAppearanceMirror { appearance in store?.followPhone(appearance) })
            .task {
                guard store == nil else { return }
                // Started at the app's first line of code (Boot), so usually already loaded here.
                let boot = await Boot.loaded()
                let loaded = boot.store, platform = boot.platform, network = boot.network
                // iOS's few seconds after Home for a message already sent (iPhone walk D3).
                loaded.backgroundContinuation = BackgroundSendTime()
                platform.dispatch = { loaded.receive($0) }
                await network.setSink { action in await MainActor.run { loaded.receive(action) } }
                await network.setPreviewKeyProvider { origin, previews in
                    await MainActor.run { try? NotificationPlatform.shared.configurePreviews(origin: origin, previews: previews).key }
                }
                NotificationPlatform.shared.onToken = { token in
                    guard let environment = PushRegistration.buildEnvironment else {
                        loaded.receive(.notificationsResult(.appleUnavailable))
                        return
                    }
                    Task {
                        let actions = await network.setPushToken(token, sandbox: environment == .sandbox, state: loaded.state)
                        for action in actions { loaded.receive(action) }
                    }
                }
                NotificationPlatform.shared.onOpen = { target in
                    let state = loaded.state
                    if let action = NotificationTapRouter.action(for: target, state: state, hostID: state.notifications.hostID) {
                        loaded.send(action)
                    } else if target.belongs(toHost: state.notifications.hostID, thread: state.mac?.threadID) {
                        // The reply is not loaded yet: the core fetches older history until it appears.
                        loaded.send(.openedFromNotificationReference(target.event, preview: target.preview,
                                                                     at: Int64(Date().timeIntervalSince1970 * 1000)))
                    }
                }
                PlatformEffects.permissionMirror().forEach { loaded.send($0) }
                NotificationPlatform.shared.deliverPending()
                self.platform = platform
                #if DEBUG
                await DevBridge.start(store: loaded)
                #endif
                // Before the first frame, so a phone set to light never flashes dark.
                loaded.followPhone(SystemAppearance.current())
                LaunchTiming.record("store-ready")
                store = loaded
                await ShareIntake.takeWaiting(into: loaded, nowMs: SystemClock().nowMs())
            }
            .task(id: nextTick) {
                // The core's clock, only when time is owed: a running recording, the end of the quiet
                // period before "Reconnecting…", an outbox retry (`TickSchedule`). One sleep to that
                // moment, one tick; a new state that moves the moment restarts it. At rest, or off
                // screen, nothing is owed and nothing wakes (the CEO's battery rule, 2026-09-24).
                guard let due = nextTick else { return }
                let wait = due - SystemClock().nowMs()
                if wait > 0 {
                    do { try await Task.sleep(nanoseconds: UInt64(wait) * 1_000_000) } catch { return }
                }
                if let store, store.ticking { store.send(.tick(at: SystemClock().nowMs())) }
            }
            .onChange(of: readReplies) { _, read in
                // D04: read replies leave Notification Center, however the app was opened. Only when
                // what is read changes, only on screen: no timer, nothing in the background.
                guard let read, let store else { return }
                NotificationPlatform.shared.withdrawRead(read, hostID: store.state.notifications.hostID)
            }
            .onChange(of: shareContext) { _, _ in
                // Only when what the Share extension reads changed (pairing, the Mac, appearance,
                // limits). Keyed on the whole state, this read and decoded the context file on the
                // main thread on every keystroke, streamed word and voice tick (Sage's review T3).
                guard let state = store?.state else { return }
                SharePlatform.mirror(state, macAcceptsAttachments: state.attachmentLimits != nil, limits: state.attachmentLimits)
            }
        }
    }
}

/// The saved state's load, started at the app's first line of code (`RichOSNativeApp.init`).
///
/// It used to start in the root's `.task`, which SwiftUI runs once the first frame is drawn, so the disk
/// read and the decode of the whole saved conversation came after everything else, in series. Now they
/// run on the core's actor while UIKit and SwiftUI build the scene on the main thread (on the test
/// iPhone's 2026-10-02 profiles, about 90 ms from the app's first line to its first frame). The effect
/// handlers are the same objects, built in the same order, only sooner; the store is handed to the
/// screens in the same `.task`, with every wiring step before it unchanged.
@MainActor
enum Boot {
    struct Loaded {
        let store: AppStore
        let platform: PlatformEffects
        let network: NetworkEffects
    }

    private static var load: Task<Loaded, Never>?
    /// When the load started (the app's first line), on the uptime clock.
    private static var startedUptime: TimeInterval?
    /// How long the logo may stay up from the app's first line while the saved state is read. The read
    /// is local and normally done within a few tens of milliseconds of the first frame (the test iPhone
    /// opens to an input-ready conversation about 0.4 s after its process starts), so a normal launch
    /// never reaches it; a read that does (a phone busy after a restart, a slow disk) shows the
    /// conversation's quiet screen instead of the logo.
    static let logoBound: Duration = .milliseconds(500)

    /// What is left of `logoBound` now; zero once it has passed.
    static func quietBudget(now: TimeInterval = ProcessInfo.processInfo.systemUptime) -> Duration {
        guard let startedUptime else { return logoBound }
        let left = logoBound - .milliseconds(Int64((now - startedUptime) * 1000))
        return left > .zero ? left : .zero
    }

    /// Where the files Rich attached are kept once they came over (round 36 screen 3): the app's own
    /// folder (`LocalOnlyStorage.appRoot`, beside `Attachments/` and `Recordings/`), so a second tap
    /// opens one at once, the system never purges it, and it never goes into an iCloud or Finder backup
    /// (security review I-2). Not `Application Support` itself: on the test iPhone that belongs to root
    /// after the start-time measurement seeds the app's state (perf/ios.py `DeviceState`), and the
    /// folder could not be made there (NSCocoaErrorDomain 513, 2026-10-22).
    static func richFilesDirectory() -> URL? {
        LocalOnlyStorage.appRoot.appendingPathComponent("RichFiles", isDirectory: true)
    }

    static func start() {
        guard load == nil else { return }
        startedUptime = ProcessInfo.processInfo.systemUptime
        LaunchTiming.record("state-load-start")
        let transport = URLSessionTransport()
        // The courier reads voice messages from the same files the recorder writes.
        let network = NetworkEffects(transport: transport, stream: transport, identities: PlatformEffects.identityStore(),
                                     recordings: FileRecordingStore(directory: VoiceRecorder.defaultDirectory()),
                                     attachments: FileAttachmentStore(directory: ShareIntake.attachmentsDirectory()),
                                     richFiles: richFilesDirectory())
        let platform = PlatformEffects(network: network, attachments: ShareIntake.attachmentsDirectory(),
                                       replyAudio: { id, state in await network.replyAudio(messageID: id, state: state) })
        let effects: (any EffectHandler)?
        #if DEBUG
        // Gesture fixtures use the real core and storage with controlled
        // non-storage effects, just as the headless CLI does; `-rios-fixture-mac` adds a
        // stand-in Mac for the wait for the press on the Mac (`DevBridgeFixtureMac`).
        effects = DevBridge.interactiveFixture ? DevBridgeFixtureMac.fromLaunchArguments() : platform
        #else
        effects = platform
        #endif
        load = Task { @MainActor in
            // Creating the directory and marking it out of backups is disk work: off the main thread,
            // so a slow disk can never hold the frames the window draws meanwhile (LaunchRoot).
            let storage = await Task.detached(priority: .userInitiated) { AppStore.defaultStorage() }.value
            let store = await AppStore.launch(storage: storage, effects: effects, performance: PerformanceMarks.record)
            LaunchTiming.record("state-loaded")
            return Loaded(store: store, platform: platform, network: network)
        }
    }

    static func loaded() async -> Loaded {
        start()
        return await load!.value
    }
}

/// The phone's light or dark setting, read from the screen, for the first frame and each return to
/// the front.
@MainActor
enum SystemAppearance {
    static func current() -> Appearance {
        let scene = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first
        let style = scene?.screen.traitCollection.userInterfaceStyle ?? UITraitCollection.current.userInterfaceStyle
        return style == .light ? .light : .dark
    }
}

/// Tells the core when the phone switches between light and dark while the app is on screen (Control
/// Center, or the automatic schedule). The app sets no scheme of its own, so the environment's color
/// scheme IS the phone's, and SwiftUI delivers each change: no polling, no timer, no registration.
private struct PhoneAppearanceMirror: ViewModifier {
    @Environment(\.colorScheme) private var scheme
    let follow: (Appearance) -> Void

    func body(content: Content) -> some View {
        content.onChange(of: scheme, initial: true) { _, scheme in follow(scheme == .light ? .light : .dark) }
    }
}
