import AppKit
import RichFilesCore
import ServiceManagement
import SwiftUI

@MainActor
final class BackgroundCaptureSettings: NSWindowController {
    init() {
        let controller = NSHostingController(rootView: BackgroundCaptureSettingsView())
        let window = NSWindow(contentViewController: controller)
        window.title = "RichGlance Capture Settings"
        window.styleMask = [.titled, .closable]
        window.isReleasedWhenClosed = false
        window.center()
        super.init(window: window)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { nil }
}

private struct BackgroundCaptureSettingsView: View {
    @State private var status: GlanceHelperControl.Status?
    @State private var busy = false
    @State private var feedback = ""
    @State private var monitor: Any?
    @State private var recording: CaptureRequest.Action?

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 5) {
                Toggle("Capture when RichOS is quit", isOn: Binding(get: { status?.background == true },
                    set: { value in update { try GlanceHelperControl.setBackground(value) } }))
                Text("Start at login and capture from any app.")
                    .foregroundStyle(.secondary)
            }
            Toggle("Show RichGlance in the menu bar", isOn: Binding(get: { status?.background == true && status?.menuBar == true },
                    set: { value in update { try GlanceHelperControl.setMenuBar(value) } }))
                .disabled(status?.background != true)
            Divider()
            Toggle("Enable capture shortcut", isOn: Binding(get: { status?.enabled == true }, set: { value in
                guard let shortcut = status?.shortcut else { return }
                update { try GlanceHelperControl.setShortcut(shortcut, enabled: value) }
            }))
            shortcutRow("Capture Region", action: .capture, shortcut: status?.shortcut)
            shortcutRow("Capture Again", action: .captureAgain, shortcut: status?.againShortcut)
            shortcutRow("Copy Text", action: .copyText, shortcut: status?.textShortcut)
            if status?.service == "approval" || status?.service == "unavailable" {
                Text("Allow RichGlance Capture in Login Items to use it when RichOS is quit.")
                Button("Open Login Items Settings") { SMAppService.openSystemSettingsLoginItems() }
            } else if status?.background == true && status?.running != true && feedback.isEmpty {
                Text("RichGlance Capture is stopped. Turn it off and on again.")
                    .foregroundStyle(.secondary)
            } else if status?.enabled == true && status?.registered != true && status?.background == true && feedback.isEmpty {
                Text("Shortcut unavailable. Choose another combination.")
                    .foregroundStyle(.secondary)
            }
            if !feedback.isEmpty { Text(feedback).foregroundStyle(.secondary) }
        }
        .toggleStyle(.switch)
        .disabled(busy || status?.available != true)
        .padding(24)
        .frame(width: 445)
        .onAppear { refresh() }
        .onDisappear { stopRecording() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in refresh() }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didResignKeyNotification)) { _ in stopRecording() }
    }

    private func refresh() {
        if !busy { update { try GlanceHelperControl.status() } }
    }

    private func update(_ work: @escaping @Sendable () throws -> GlanceHelperControl.Status) {
        guard !busy else { return }
        busy = true
        feedback = ""
        Task { @MainActor in
            do { status = try await Task.detached(priority: .userInitiated, operation: work).value }
            catch { feedback = error.localizedDescription }
            busy = false
        }
    }

    private func shortcutRow(_ title: String, action: CaptureRequest.Action,
                              shortcut: GlanceHelper.Shortcut?) -> some View {
        HStack {
            Text(title)
            Spacer()
            if status?.enabled == true, status?.running == true, !registered(action), recording != action {
                Text("Unavailable").foregroundStyle(.secondary)
            }
            Text(recording == action ? "Press a shortcut" : shortcut?.label ?? "")
                .monospacedDigit()
            Button(recording == action ? "Cancel" : "Change…") {
                if recording == action { stopRecording() }
                else { stopRecording(); recordShortcut(for: action) }
            }
            .accessibilityLabel(recording == action ? "Cancel recording" : "Change \(title) shortcut")
        }
    }

    private func registered(_ action: CaptureRequest.Action) -> Bool {
        switch action {
        case .capture: status?.registered == true
        case .captureAgain: status?.againRegistered == true
        case .copyText: status?.textRegistered == true
        default: false
        }
    }

    private func recordShortcut(for action: CaptureRequest.Action) {
        recording = action
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            let keyCode = event.keyCode
            let repeatKey = event.isARepeat
            let modifiers = event.modifierFlags
            MainActor.assumeIsolated {
                if keyCode == 53 { stopRecording(); return }
                if repeatKey { return }
                guard let shortcut = GlanceHelper.Shortcut(keyCode: keyCode,
                        control: modifiers.contains(.control), option: modifiers.contains(.option),
                        shift: modifiers.contains(.shift), command: modifiers.contains(.command)) else {
                    feedback = "Use two or more modifiers and a letter or number."
                    return
                }
                do { _ = try shortcut.carbon() }
                catch { feedback = error.localizedDescription; return }
                let enabled = status?.enabled == true
                stopRecording()
                update { try GlanceHelperControl.setShortcut(shortcut, enabled: enabled, for: action) }
            }
            return nil
        }
    }

    private func stopRecording() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        recording = nil
    }
}
