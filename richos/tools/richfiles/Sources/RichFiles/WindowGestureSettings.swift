import AppKit
import Darwin
import RichFilesCore
import ServiceManagement
import SwiftUI

public enum WindowGestureService {
    private static let registrationLock = NSLock()
    private static var service: SMAppService { .loginItem(identifier: WindowGesturePreferences.helperID) }

    /// ServiceManagement uses the calling application's bundle to find its login item.
    public static func runCommandIfRequested() -> Bool {
        let arguments = CommandLine.arguments
        guard arguments.count == 3, arguments[1] == WindowGesturePreferences.command else { return false }
        guard Bundle.main.bundleIdentifier == "app.richos.richfiles" else { exit(2) }
        print(update(arguments[2]))
        return true
    }

    private static func update(_ action: String) -> String {
        registrationLock.lock()
        defer { registrationLock.unlock() }

        let item = service
        do {
            switch action {
            case "register", "register-if-desktop-enabled":
                if action == "register-if-desktop-enabled",
                   !WindowGesturePreferences.selection.contains(.desktopDelete) { break }
                if item.status == .notRegistered || item.status == .notFound { try item.register() }
            case "unregister":
                if item.status == .enabled || item.status == .requiresApproval { try item.unregister() }
            case "refresh-after-update":
                // An app-bundle swap leaves the old helper alive. Keep macOS's
                // approval and ask launchd to start that same job from the new copy.
                if item.status == .enabled && !WindowGesturePreferences.selection.isEmpty {
                    refreshInstalledHelper()
                }
            case "status":
                break
            default:
                return "unavailable"
            }
        } catch {
            let failure = error as NSError
            NSLog("RichFiles Window Gestures service %@ failed: %@ (%ld)", action, failure.domain, failure.code)
            return item.status == .requiresApproval ? "approval" : "unavailable"
        }
        switch item.status {
        case .enabled: return "enabled"
        case .requiresApproval: return "approval"
        case .notRegistered: return "off"
        case .notFound: return action == "register" ? "unavailable" : "off"
        default: return "unavailable"
        }
    }

    private static func refreshInstalledHelper() {
        let helper = Bundle.main.bundleURL.appendingPathComponent(WindowGesturePreferences.helperPath)
        guard let installed = Bundle(url: helper)?.executableURL?.resolvingSymlinksInPath() else { return }
        let running = NSRunningApplication.runningApplications(withBundleIdentifier: WindowGesturePreferences.helperID)
            .first { !$0.isTerminated }
        if let running {
            var path = [CChar](repeating: 0, count: 4 * Int(MAXPATHLEN))
            if proc_pidpath(running.processIdentifier, &path, UInt32(path.count)) > 0,
               URL(fileURLWithPath: String(cString: path)).resolvingSymlinksInPath() == installed {
                return
            }
        }
        let command = Process()
        command.executableURL = URL(fileURLWithPath: "/bin/launchctl")
        command.arguments = ["kickstart"] + (running == nil ? [] : ["-k"])
            + ["gui/\(getuid())/\(WindowGesturePreferences.helperID)"]
        do {
            try command.run()
            command.waitUntilExit()
            if command.terminationStatus == 0 { NSLog("RichFiles Window Gestures restarted after update") }
        } catch {
            NSLog("RichFiles Window Gestures could not restart after update: %@", error.localizedDescription)
        }
    }

    static func request(_ action: String, in app: URL) async -> String {
        let executable = app.appendingPathComponent("Contents/MacOS/RichFiles")
        return await Task.detached(priority: .userInitiated) {
            if Bundle.main.bundleIdentifier == "app.richos.richfiles" { return update(action) }
            let command = Process()
            let output = Pipe()
            command.executableURL = executable
            command.arguments = [WindowGesturePreferences.command, action]
            command.standardOutput = output
            command.standardError = FileHandle.nullDevice
            do {
                try command.run()
                let result = output.fileHandleForReading.readDataToEndOfFile()
                command.waitUntilExit()
                guard command.terminationStatus == 0 else { return "unavailable" }
                return String(decoding: result, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
            } catch {
                return "unavailable"
            }
        }.value
    }
}

@MainActor
struct WindowGestureSettings: View {
    private typealias Preferences = WindowGesturePreferences

    let appURL: URL
    var desktopShortcut = false
    @Environment(\.colorScheme) private var colorScheme
    @State private var selection = Preferences.selection
    @State private var runtime: Preferences.Status?
    @State private var registration = ""
    @State private var updating = false
    @State private var nativeTitleBarIsOff = Preferences.nativeTitleBarIsOff

    private var helper: URL { appURL.appendingPathComponent(Preferences.helperPath) }
    private var guidanceColor: Color { BrowserPalette(colorScheme).hint }

    private var enabled: Bool {
        let choices: Preferences.Selection = desktopShortcut ? .desktopDelete : [.titleBar, .dock]
        return !selection.intersection(choices).isEmpty
    }

    var body: some View {
        if desktopShortcut {
            Toggle("Press Delete to move selected desktop items to Trash", isOn: toggle(.desktopDelete))
                .disabled(updating)
        } else {
            Toggle("Double-click a title bar to maximize or restore", isOn: toggle(.titleBar))
                .disabled(updating)
            Toggle("Click the active app’s Dock icon to minimize or restore", isOn: toggle(.dock))
                .disabled(updating)
        }
        if enabled {
            Text(desktopShortcut
                ? "This shortcut works on the macOS desktop even after you quit RichFiles."
                : "These gestures work in other apps and continue after you quit RichOS and RichFiles.")
                .foregroundStyle(guidanceColor)
            if !desktopShortcut && selection.contains(.titleBar) && !nativeTitleBarIsOff {
                Text("In Desktop & Dock, set “Double-click a window’s title bar to” to Do Nothing. Choose your previous action there to restore native behavior.")
                    .foregroundStyle(guidanceColor)
                Button("Open Desktop & Dock Settings") { open("com.apple.Desktop-Settings.extension") }
                    .buttonStyle(BrowserButtonStyle(bordered: true))
            }
            if registration == "approval" {
                Text("Allow RichFiles Window Gestures in Login Items to run it in the background.")
                    .foregroundStyle(guidanceColor)
                Button("Open Login Items Settings") { SMAppService.openSystemSettingsLoginItems() }
                    .buttonStyle(BrowserButtonStyle(bordered: true))
            } else if registration == "unavailable" || registration == "off" {
                Text("RichFiles Window Gestures could not start. Try again from the installed copy of RichFiles.")
                    .foregroundStyle(guidanceColor)
                Button(desktopShortcut ? "Retry Desktop Shortcut" : "Retry Window Gestures") { synchronize("register") }
                    .buttonStyle(BrowserButtonStyle(bordered: true)).disabled(updating)
            } else if let runtime {
                permissions(runtime)
            } else {
                Text("Waiting for RichFiles Window Gestures to start.")
                    .foregroundStyle(guidanceColor)
                Button("Open Login Items Settings") { SMAppService.openSystemSettingsLoginItems() }
                    .buttonStyle(BrowserButtonStyle(bordered: true))
            }
        } else if selection.isEmpty && registration == "unavailable" {
            Text("macOS could not remove the RichFiles background item.")
                .foregroundStyle(guidanceColor)
            Button("Open Login Items Settings") { SMAppService.openSystemSettingsLoginItems() }
                .buttonStyle(BrowserButtonStyle(bordered: true))
        }
        Color.clear.frame(height: 0).accessibilityHidden(true)
            .onAppear { refresh(); synchronize("status") }
            .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
                refresh()
                synchronize("status")
            }
            .onReceive(Timer.publish(every: 1, on: .main, in: .common).autoconnect()) { _ in refresh() }
    }

    @ViewBuilder private func permissions(_ status: Preferences.Status) -> some View {
        switch status {
        case .accessibility, .inputMonitoring:
            let accessibility = status == .accessibility
            Text(permissionGuidance(accessibility: accessibility))
                .foregroundStyle(guidanceColor)
            HStack {
                Button(accessibility ? "Open Accessibility Settings" : "Open Input Monitoring Settings") {
                    open(accessibility ? "com.apple.preference.security?Privacy_Accessibility"
                                       : "com.apple.preference.security?Privacy_ListenEvent")
                }
                Button("Show Helper in Finder") { NSWorkspace.shared.activateFileViewerSelecting([helper]) }
            }
            .buttonStyle(BrowserButtonStyle(bordered: true))
        case .inputUnavailable:
            Text("macOS is currently blocking input monitoring. Check Input Monitoring or sign out and back in.")
                .foregroundStyle(guidanceColor)
            Button("Open Input Monitoring Settings") { open("com.apple.preference.security?Privacy_ListenEvent") }
                .buttonStyle(BrowserButtonStyle(bordered: true))
        case .running, .off:
            EmptyView()
        }
    }

    private func permissionGuidance(accessibility: Bool) -> String {
        if desktopShortcut {
            return accessibility
                ? "In Accessibility, add and allow RichFiles Window Gestures to check whether the desktop is focused."
                : "In Input Monitoring, add and allow RichFiles Window Gestures to detect the Delete key."
        }
        return accessibility
            ? "In Accessibility, add and allow RichFiles Window Gestures to move and minimize other apps’ windows."
            : "In Input Monitoring, add and allow RichFiles Window Gestures to detect mouse clicks."
    }

    private func toggle(_ gesture: Preferences.Selection) -> Binding<Bool> {
        Binding(get: { selection.contains(gesture) }, set: { enabled in
            Preferences.setEnabled(enabled, for: gesture)
            selection = Preferences.selection
            synchronize(selection.isEmpty ? "unregister" : "register")
        })
    }

    private func refresh() {
        if !updating { selection = Preferences.selection }
        nativeTitleBarIsOff = selection.contains(.titleBar) && Preferences.nativeTitleBarIsOff
        let process = NSRunningApplication.runningApplications(withBundleIdentifier: Preferences.helperID)
            .first { !$0.isTerminated && $0.bundleURL?.resolvingSymlinksInPath() == helper.resolvingSymlinksInPath() }
        runtime = process.flatMap { Preferences.status(process: $0.processIdentifier) }
    }

    private func synchronize(_ action: String) {
        guard !updating else { return }
        updating = true
        Task {
            registration = await WindowGestureService.request(action, in: appURL)
            updating = false
            refresh()
        }
    }

    private func open(_ destination: String) {
        guard let url = URL(string: "x-apple.systempreferences:\(destination)") else { return }
        NSWorkspace.shared.open(url)
    }
}
