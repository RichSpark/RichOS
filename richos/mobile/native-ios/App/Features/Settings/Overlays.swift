import RichOSCore
import SwiftUI

/// A centered dialog over a scrim (`.dialog`): a serif heading, plain words, two clearly different
/// actions. It is modal to VoiceOver, and the escape gesture takes the safe way out.
struct DialogView: View {
    let dialog: ScreenModel.Dialog
    let send: (Intent) -> Void
    @Environment(\.palette) private var palette
    @State private var shown = false

    var body: some View {
        ZStack {
            palette.scrim.ignoresSafeArea()
                .onTapGesture { send(safeWayOut) }
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 0) {
                Text(title)
                    .type(Typography.dialogTitle)
                    .foregroundStyle(palette.ink)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.bottom, 10)
                    .accessibilityAddTraits(.isHeader)
                ForEach(Array(paragraphs.enumerated()), id: \.offset) { index, p in
                    Text(p.text)
                        .type(p.soft ? Typography.read : Typography.body)
                        .foregroundStyle(p.soft ? palette.inkSoft : palette.ink)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, index == 0 ? 0 : 8)
                }
                VStack(spacing: 6) { actions }
                    .padding(.top, 18)
            }
            .padding(.top, 24).padding(.horizontal, 22).padding(.bottom, 18)
            .background(RoundedRectangle(cornerRadius: 24, style: .continuous).fill(palette.ground)
                .shadow(color: .black.opacity(palette.appearance == .dark ? 0.6 : 0.25), radius: 30, y: 30))
            .overlay(RoundedRectangle(cornerRadius: 24, style: .continuous).strokeBorder(palette.lineFaint, lineWidth: 1))
            .padding(.horizontal, 22)
            .scaleEffect(shown ? 1 : 0.9)
            .opacity(shown ? 1 : 0)
            .accessibilityElement(children: .contain)
            .accessibilityAddTraits(.isModal)
            .accessibilityAction(.escape) { send(safeWayOut) }
            .accessibilityIdentifier("dialog")
        }
        .onAppear { withAnimation(Motion.dialog) { shown = true } }
    }

    private var title: String {
        switch dialog {
        case .cameraDenied: return "The camera is off for RichConnect"
        case .pairBlocked(let n, _): return n == 1 ? "One message is still waiting" : "\(Self.count(n)) messages are still waiting"
        case .forget, .forgetDiscardsRecording: return "Forget this pairing?"
        case .forgetBlocked: return "Not yet"
        case .update: return "A new RichConnect is ready"
        }
    }

    private var paragraphs: [(text: String, soft: Bool)] {
        switch dialog {
        case .cameraDenied:
            return [("Turn it on in Settings to scan the code, or paste a pairing link instead.", false)]
        case .pairBlocked(let n, let removed):
            if removed {
                // Urban's copy (UX audit §4.1): no Send, and no pairing that no longer exists.
                return [("\(n == 1 ? "It was" : "They were") written for the Mac that removed this phone, so \(n == 1 ? "it" : "they") can’t be sent now. Discard \(n == 1 ? "it" : "them"), then pair again.", false)]
            }
            return [("\(n == 1 ? "It was" : "They were") written for the Mac this phone is paired with now. Send \(n == 1 ? "it" : "them") or discard \(n == 1 ? "it" : "them"), then pair with the new Mac.", false)]
        case .forget, .forgetDiscardsRecording:
            return [("This phone will stop reaching your Mac. Your conversation stays on the Mac.", false)]
                + (dialog == .forgetDiscardsRecording ? [(ScreenModel.Dialog.discardedRecordingNote, false)] : [])
                + [("To pair again later, scan the code on your Mac.", true)]
        case .forgetBlocked(let n):
            let lead = n == 1 ? "One message is" : "\(Self.count(n)) messages are"
            return [("\(lead) still waiting to be sent. Send \(n == 1 ? "it" : "them") or discard \(n == 1 ? "it" : "them") first, then this phone can forget the pairing.", false)]
        case .update(_, let message):
            return [(message, false),
                    ("Your drafts, queued messages and recordings stay on this phone.", true)]
        }
    }

    @ViewBuilder private var actions: some View {
        switch dialog {
        case .cameraDenied:
            Button { send(.openSystemSettings) } label: { Text("Open Settings") }
                .buttonStyle(RButtonStyle(kind: .primary, wide: true))
            Button { send(.usePairingLink) } label: { Text("Use a pairing link") }
                .buttonStyle(RButtonStyle(kind: .ghost, wide: true))
        case .pairBlocked(_, true):
            // The only way forward is filled; "Not now" keeps the messages and returns to the
            // removed screen.
            Button { send(.discardAndPair) } label: { Text("Discard and pair") }
                .buttonStyle(RButtonStyle(kind: .primary, wide: true))
                .accessibilityIdentifier("pairBlocked.discard")
            Button { send(.keepPairing) } label: { QuietLabel(text: "Not now") }
                .buttonStyle(RButtonStyle(kind: .quiet, wide: true))
                .accessibilityIdentifier("pairBlocked.notNow")
        case .pairBlocked(_, false):
            Button { send(.sendWaitingFirst) } label: { Text("Send it first") }
                .buttonStyle(RButtonStyle(kind: .primary, wide: true))
            Button { send(.discardAndPair) } label: { Text("Discard and pair") }
                .buttonStyle(RButtonStyle(kind: .ghost, wide: true))
            Button { send(.keepPairing) } label: { QuietLabel(text: "Keep this pairing") }
                .buttonStyle(RButtonStyle(kind: .quiet, wide: true))
        case .forget, .forgetDiscardsRecording:
            // The safe action is the filled one (round-12 `settings-forget`).
            Button { send(.confirmForget) } label: { Text("Forget pairing on this phone") }
                .buttonStyle(RButtonStyle(kind: .danger, wide: true))
                .accessibilityIdentifier("forget.confirm")
            Button { send(.closeDialog) } label: { Text("Keep pairing") }
                .buttonStyle(RButtonStyle(kind: .primary, wide: true))
                .accessibilityIdentifier("forget.keep")
        case .forgetBlocked:
            Button { send(.showWaiting) } label: { Text("Show the waiting messages") }
                .buttonStyle(RButtonStyle(kind: .primary, wide: true))
            // The way out when the Mac is gone for good (reinstalled, replaced): the waiting messages
            // go and the pairing is forgotten, so this phone can pair again.
            Button { send(.discardAndForget) } label: { Text("Discard and forget") }
                .buttonStyle(RButtonStyle(kind: .ghost, wide: true))
                .accessibilityIdentifier("forgetBlocked.discard")
            Button { send(.closeDialog) } label: { QuietLabel(text: "Keep pairing") }
                .buttonStyle(RButtonStyle(kind: .quiet, wide: true))
        case .update:
            Button { send(.openAppStore) } label: { IconLabel(icon: .down, text: "Update in App Store") }
                .buttonStyle(RButtonStyle(kind: .primary, wide: true))
                .accessibilityIdentifier("update.store")
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 6) { laterAndCheck }
                VStack(spacing: 6) { laterAndCheck }
            }
            Button { send(.openSupport) } label: { QuietLabel(text: "Support") }
                .buttonStyle(RButtonStyle(kind: .quiet, wide: true))
        }
    }

    @ViewBuilder private var laterAndCheck: some View {
        Button { send(.updateLater) } label: { Text("Later") }
            .buttonStyle(RButtonStyle(kind: .ghost, wide: true))
        Button { send(.checkAgain) } label: { Text("Check again") }
            .buttonStyle(RButtonStyle(kind: .ghost, wide: true))
    }

    private var safeWayOut: Intent {
        switch dialog {
        case .cameraDenied, .forget, .forgetDiscardsRecording, .forgetBlocked: return .closeDialog
        case .pairBlocked: return .keepPairing
        case .update: return .updateLater
        }
    }

    static func count(_ n: Int) -> String {
        let words = ["Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten"]
        return n < words.count ? words[n] : "\(n)"
    }
}

/// The settings sheet (`settings`): notifications, this iPhone, connection and privacy, and the one red
/// row at the bottom. Every status is a sentence in the row, never a code (`notif-settings`).
struct SettingsSheet: View {
    let settings: ScreenModel.Settings
    let send: (Intent) -> Void
    @Environment(\.palette) private var palette

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text("Settings")
                    .type(Typography.sheetTitle)
                    .foregroundStyle(palette.ink)
                    .padding(.top, 20).padding(.bottom, 16)
                    .accessibilityAddTraits(.isHeader)
                section("Reply notifications")
                notificationsRow
                row(icon: .shield, title: "Show reply previews", detail: "Off shows only “Rich has replied.”", control: true) {
                    Toggle("Show reply previews", isOn: Binding(get: { settings.previews }, set: { send(.setPreviews($0)) }))
                        .labelsHidden()
                        .toggleStyle(RToggleStyle())
                        .accessibilityIdentifier("settings.previews")
                }
                section("This iPhone")
                row(icon: .phone, title: "iPhone permissions", detail: "Microphone and camera", chevron: true) { EmptyView() }
                    .onTapGesture { send(.openSystemSettings) }
                    .accessibilityAddTraits(.isButton)
                if settings.showsUpdateCheck {
                    row(icon: .refresh, title: "Check for updates", detail: nil) {
                        Text(updateValue)
                            .type(Typography.read)
                            .foregroundStyle(isUpdateAvailable ? palette.ink : palette.inkSoft)
                    }
                    .onTapGesture { send(.checkForUpdates) }
                    .accessibilityAddTraits(.isButton)
                    .accessibilityIdentifier("settings.updates")
                }
                row(icon: .life, title: "Support", detail: nil, chevron: true) { EmptyView() }
                    .onTapGesture { send(.openSupport) }
                    .accessibilityAddTraits(.isButton)
                    .accessibilityIdentifier("settings.support")
                section("Connection and privacy")
                row(icon: .mac, title: "Paired with \(settings.macName)",
                    detail: "Messages go to your Mac. Rich on your Mac writes the replies with its AI provider.") { EmptyView() }
                row(icon: .cloud, title: "Where your messages go", detail: nil, chevron: true) { EmptyView() }
                    .onTapGesture { send(.openWhereMessagesGo) }
                    .accessibilityAddTraits(.isButton)
                // Not in round 12.1: Apple 5.1.1(i) requires the policy to be reachable inside the app.
                row(icon: .lock, title: "Privacy policy", detail: nil, chevron: true) { EmptyView() }
                    .onTapGesture { send(.openPrivacyPolicy) }
                    .accessibilityAddTraits(.isButton)
                    .accessibilityIdentifier("settings.privacy")
                Button { send(.forget) } label: {
                    HStack(spacing: 12) {
                        IconView(.close, size: 22).foregroundStyle(palette.danger)
                        Text("Forget this pairing")
                            .type(Typography.body.weight(600))
                            .foregroundStyle(palette.danger)
                        Spacer(minLength: 0)
                    }
                    .rowSurface(palette)
                }
                .buttonStyle(PressScale(scale: 0.98))
                .accessibilityIdentifier("settings.forget")
                // The space below the last row that the removed footer kept (rehearsal row 15).
                .padding(.bottom, 14)
            }
            .padding(.horizontal, 20)
        }
        .background(palette.ground.ignoresSafeArea())
        .accessibilityIdentifier("settings.sheet")
    }

    private var isUpdateAvailable: Bool { if case .available = settings.update { return true } else { return false } }

    private var updateValue: String {
        switch settings.update {
        case .upToDate: return "Up to date"
        case .available(let v): return "\(v) is available"
        case .couldNotCheck: return "Could not check"
        }
    }

    private func section(_ title: String) -> some View {
        Text(title)
            .type(Typography.read.weight(600))
            .foregroundStyle(palette.inkSoft)
            .padding(.top, 18).padding(.bottom, 8)
            .accessibilityAddTraits(.isHeader)
    }

    @ViewBuilder private var notificationsRow: some View {
        let s = settings.notifications
        row(icon: .bell, title: "Notify me when Rich replies", detail: notificationDetail(s), control: true) {
            switch s.control {
            case .toggle(let isOn):
                Toggle("Notify me when Rich replies",
                       isOn: Binding(get: { isOn }, set: { send(.setNotifications($0)) }))
                    .labelsHidden()
                    .toggleStyle(RToggleStyle())
                    .accessibilityIdentifier("settings.notifications")
            case .openSettings:
                Button { send(.openSystemSettings) } label: { Text("Open Settings") }
                    .buttonStyle(RButtonStyle(kind: .ghost, compact: true))
            case .none:
                EmptyView()
            }
        }
    }

    private func notificationDetail(_ s: ScreenModel.NotificationStatus) -> String? {
        switch s {
        case .on: return nil
        case .off, .notAsked: return "Rich cannot reach you when the app is closed."
        case .turningOn: return "Turning on…"
        case .denied: return "Off in iPhone Settings. Turn it on there to hear back when the app is closed."
        case .unsupported: return "Not available on this phone."
        case .appleUnavailable: return "Apple’s notification service is unavailable right now. Rich will try again."
        case .serviceUnavailable: return "Unavailable right now. Rich will try again."
        }
    }

    /// `control`: the row holds its own switch or button, which stays a separate VoiceOver element.
    private func row<Trailing: View>(icon: Icon?, title: String, detail: String?, chevron: Bool = false,
                                     control: Bool = false,
                                     @ViewBuilder trailing: () -> Trailing) -> some View {
        HStack(spacing: 12) {
            if let icon { IconView(icon, size: 22).foregroundStyle(palette.ink) }
            VStack(alignment: .leading, spacing: 2) {
                Text(title).type(Typography.body).foregroundStyle(palette.ink)
                if let detail {
                    Text(detail).type(Typography.read.lineHeight(1.3)).foregroundStyle(palette.inkSoft)
                }
            }
            .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
            trailing()
            if chevron { IconView(.chevR, size: 18).foregroundStyle(palette.inkSoft) }
        }
        .rowSurface(palette)
        .accessibilityElement(children: control ? .contain : .combine)
    }
}

extension View {
    /// `.srow`: 56 pt minimum, the surface, a faint hairline, 16 pt corners.
    func rowSurface(_ palette: Palette) -> some View {
        padding(.horizontal, 14).padding(.vertical, 10)
            .frame(minHeight: 56)
            .background(RoundedRectangle(cornerRadius: 16, style: .continuous).fill(palette.surface))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(palette.lineFaint, lineWidth: 1))
            .contentShape(Rectangle())
            .padding(.bottom, 8)
    }
}

/// "Where your messages go", reachable later from Settings: the consent screen's three lines again.
struct WhereMessagesGoSheet: View {
    let send: (Intent) -> Void
    @Environment(\.palette) private var palette
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text("Where your words go")
                    .type(Typography.sheetTitle)
                    .foregroundStyle(palette.ink)
                    .padding(.top, 20)
                    .accessibilityAddTraits(.isHeader)
                ConsentRows().padding(.top, 20)
            }
            .padding(.horizontal, 20).padding(.bottom, 20)
        }
        .background(palette.ground.ignoresSafeArea())
    }
}

/// Prominence 1: a floating card under the header, one tap to the App Store (`upd-banner`).
struct UpdateBannerView: View {
    let banner: ScreenModel.UpdateBanner
    let send: (Intent) -> Void
    @Environment(\.palette) private var palette

    var body: some View {
        HStack(spacing: 12) {
            IconView(.down, size: 22)
                .foregroundStyle(palette.accentGlyph)
                .frame(width: 40, height: 40)
                .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(palette.ground))
            VStack(alignment: .leading, spacing: 0) {
                Text("RichConnect \(banner.version) is ready").type(Typography.body.weight(600)).foregroundStyle(palette.ink)
                Text(banner.message).type(Typography.read.lineHeight(1.3)).foregroundStyle(palette.inkSoft)
            }
            .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
            Button { send(.openAppStore) } label: { Text("Update") }
                .buttonStyle(RButtonStyle(kind: .primary, compact: true))
                .accessibilityLabel("Update RichConnect in the App Store")
                .accessibilityIdentifier("update.banner.store")
        }
        .padding(.horizontal, 14).padding(.vertical, 12)
        .floatingSurface(palette, radius: 18)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("update.banner")
    }
}
