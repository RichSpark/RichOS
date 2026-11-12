import AppKit
import SwiftUI

enum BrowserPaneSide: String, Codable {
    case primary, secondary
}

@MainActor
final class WindowWorkspace: ObservableObject {
    static let maximumSessionBytes = 768 * 1024
    let primary: BrowserModel
    @Published private(set) var secondary: BrowserModel?
    @Published private(set) var isSplit = false
    @Published private(set) var active = BrowserPaneSide.primary
    @Published private(set) var previousDockedSession: Data?
    private let defaults: UserDefaults
    private var commands: [BrowserPaneSide: (owner: UUID, run: (BrowserCommand) -> Bool)] = [:]

    init(start: URL, defaults: UserDefaults = RichFilesInstallation.preferences) {
        self.defaults = defaults
        primary = BrowserModel(start: start, defaults: defaults)
    }

    var activeModel: BrowserModel {
        isSplit && active == .secondary ? secondary ?? primary : primary
    }

    func select(_ pane: BrowserPaneSide) {
        if active != pane && (pane == .primary || isSplit) { active = pane }
    }

    func toggleSplit() {
        if isSplit {
            active = .primary
            isSplit = false
        } else {
            if secondary == nil { secondary = BrowserModel(start: primary.folder, defaults: defaults) }
            isSplit = true
        }
    }

    func setCommands(for pane: BrowserPaneSide, owner: UUID, handler: @escaping (BrowserCommand) -> Bool) {
        commands[pane] = (owner, handler)
    }

    func clearCommands(for pane: BrowserPaneSide, owner: UUID) {
        if commands[pane]?.owner == owner { commands.removeValue(forKey: pane) }
    }

    func perform(_ command: BrowserCommand) {
        _ = commands[active]?.run(command)
    }

    func sessionData() -> Data? {
        guard let first = primary.sessionData() else { return nil }
        let second = secondary?.sessionData()
        guard secondary == nil || second != nil else { return nil }
        let snapshot = Session(primary: first, secondary: second, split: isSplit, active: active)
        guard let data = try? JSONEncoder().encode(snapshot), data.count <= Self.maximumSessionBytes else { return nil }
        return data
    }

    @discardableResult
    func restoreSession(_ data: Data) -> Bool {
        guard data.count <= Self.maximumSessionBytes else { return false }
        guard let snapshot = try? JSONDecoder().decode(Session.self, from: data) else {
            guard primary.restoreSession(data) else { return false }
            active = .primary
            isSplit = false
            secondary = nil
            return true
        }
        let second: BrowserModel?
        if let data = snapshot.secondary {
            let candidate = BrowserModel(start: primary.folder, defaults: defaults)
            guard candidate.restoreSession(data) else { return false }
            second = candidate
        } else {
            guard !snapshot.split, snapshot.active == .primary else { return false }
            second = nil
        }
        guard primary.restoreSession(snapshot.primary) else { return false }
        secondary = second
        isSplit = snapshot.split
        active = isSplit ? snapshot.active : .primary
        return true
    }

    func acceptDockedSession(_ data: Data) -> Bool {
        let previous = sessionData()
        guard restoreSession(data) else { return false }
        previousDockedSession = previous
        return true
    }

    func restorePreviousDockedSession() -> Bool {
        guard let previousDockedSession else { return false }
        return acceptDockedSession(previousDockedSession)
    }

    private struct Session: Codable {
        let primary: Data
        let secondary: Data?
        let split: Bool
        let active: BrowserPaneSide
    }
}

@MainActor
struct WindowBrowserView: View {
    @StateObject private var workspace: WindowWorkspace
    @ObservedObject private var actions: BrowserActions
    @ObservedObject private var openRequests: FolderOpenDelegate
    @Environment(\.openWindow) private var openWindow
    @Environment(\.scenePhase) private var scenePhase
    @AppStorage("richfiles.restoreSession") private var restoreSession = false
    @AppStorage("richfiles.lastBrowserSession.v1") private var lastBrowserSession = Data()
    @SceneStorage("richfiles.browserSession.v1") private var savedSession: Data?
    @State private var appeared = false
    @State private var cleanLaunchDeadline: TimeInterval?
    @State private var switching = false
    @State private var switchError = ""
    @State private var showSwitchError = false

    private let start: URL
    private let hostOpenWindow: ((URL) -> Void)?
    private let acceptsKeyboard: () -> Bool
    private let returnKeyboardFocus: @MainActor @Sendable () -> Void
    private let supportsSessionRestore: Bool
    private let handoffID: String?
    private let launchHandoffID: String?
    private let dockTarget: RichFilesDockTarget?
    private let hostSwitch: HostSwitch?

    struct HostSwitch {
        let target: WorkspaceHandoff.Target
        let application: URL
        var didUndock: (() -> Void)? = nil
    }

    init(start: URL, actions: BrowserActions, openRequests: FolderOpenDelegate,
         openWindow: ((URL) -> Void)? = nil, acceptsKeyboard: @escaping () -> Bool = { true },
         supportsSessionRestore: Bool = false, returnKeyboardFocus: @escaping @MainActor @Sendable () -> Void = {},
         handoffID: String? = nil, launchHandoffID: String? = nil,
         dockTarget: RichFilesDockTarget? = nil, hostSwitch: HostSwitch? = nil) {
        _workspace = StateObject(wrappedValue: WindowWorkspace(start: start))
        self.start = start
        self.actions = actions
        self.openRequests = openRequests
        hostOpenWindow = openWindow
        self.acceptsKeyboard = acceptsKeyboard
        self.returnKeyboardFocus = returnKeyboardFocus
        self.supportsSessionRestore = supportsSessionRestore
        self.handoffID = handoffID
        self.launchHandoffID = launchHandoffID
        self.dockTarget = dockTarget
        self.hostSwitch = hostSwitch
    }

    var body: some View {
        Group {
            if workspace.isSplit, let second = workspace.secondary {
                HSplitView {
                    pane(workspace.primary, side: .primary)
                    pane(second, side: .secondary)
                }
            } else {
                pane(workspace.primary, side: .primary)
            }
        }
        .disabled(switching)
        .focusedSceneValue(\.browserCommand) { command in
            if !switching { workspace.perform(command) }
        }
        .alert("Could not switch RichFiles", isPresented: $showSwitchError) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(switchError)
        }
        .onAppear {
            if !appeared {
                appeared = true
                dockTarget?.attach(workspace: workspace, actions: actions)
                if let handoffID, let id = UUID(uuidString: handoffID) {
                    openRequests.markHandoffOpened(id)
                    acceptHandoff(id)
                } else if supportsSessionRestore {
                    let firstWindow = openRequests.claimInitialBrowser()
                    if firstWindow, let launchHandoffID, let id = UUID(uuidString: launchHandoffID) {
                        openRequests.markHandoffOpened(id)
                        acceptHandoff(id)
                    } else if firstWindow, let id = openRequests.takeFirstHandoff() {
                        acceptHandoff(id)
                    } else if restoreSession, let session = savedSession ?? (firstWindow ? lastBrowserSession : nil),
                              !session.isEmpty {
                        workspace.restoreSession(session)
                    } else if firstWindow {
                        cleanLaunchDeadline = ProcessInfo.processInfo.systemUptime + 3
                    }
                }
            }
            receiveFolderOpen()
        }
        .onChange(of: workspace.isSplit) { _ in saveSession(rememberWindow: true) }
        .onChange(of: workspace.active) { _ in saveSession(rememberWindow: true) }
        .onChange(of: scenePhase) { if $0 != .active { saveSession() } }
        .onChange(of: restoreSession) { _ in saveSession(rememberWindow: true) }
        .onChange(of: openRequests.requestID) { _ in receiveFolderOpen() }
        .onDisappear { saveSession() }
    }

    private func pane(_ model: BrowserModel, side: BrowserPaneSide) -> some View {
        BrowserView(model: model, workspace: workspace, side: side, actions: actions, openWindow: hostOpenWindow,
                    acceptsKeyboard: { !switching && acceptsKeyboard() && workspace.active == side },
                    returnKeyboardFocus: returnKeyboardFocus, hostSwitchTitle: hostSwitch?.target.title,
                    switchHost: switchHost, sessionChanged: { saveSession(rememberWindow: true) })
            .frame(minWidth: workspace.isSplit ? 355 : nil)
            .accessibilityIdentifier(side == .primary ? "richfiles-primary-pane" : "richfiles-secondary-pane")
    }

    private func saveSession(rememberWindow: Bool = false) {
        guard supportsSessionRestore, appeared else { return }
        savedSession = restoreSession ? workspace.sessionData() : nil
        if rememberWindow || !restoreSession { lastBrowserSession = savedSession ?? Data() }
    }

    private func switchHost() {
        guard let hostSwitch, !switching else { return }
        let sourceWindow = NSApp.keyWindow
        switching = true
        Task {
            defer { switching = false }
            do {
                try await actions.settleForHostSwitch()
                guard let data = workspace.sessionData() else { throw WorkspaceHandoff.HandoffError.invalid }
                try await WorkspaceHandoff.send(data, to: hostSwitch.target, application: hostSwitch.application)
                if hostSwitch.target == .docked { sourceWindow?.performClose(nil) }
                else { hostSwitch.didUndock?() }
            } catch {
                switchError = error.localizedDescription
                showSwitchError = true
            }
        }
    }

    private func acceptHandoff(_ id: UUID) {
        do { try WorkspaceHandoff.accept(id, for: .standalone, into: workspace) }
        catch { switchError = error.localizedDescription; showSwitchError = true }
    }

    private func receiveFolderOpen() {
        if let cleanLaunchDeadline, ProcessInfo.processInfo.systemUptime < cleanLaunchDeadline,
           workspace.primary.folder == start, workspace.primary.tabs.count == 1,
           workspace.primary.selected.isEmpty, !workspace.isSplit,
           let id = openRequests.takeFirstHandoff() {
            self.cleanLaunchDeadline = nil
            acceptHandoff(id)
        }
        for id in openRequests.takeHandoffs() { openWindow(id: "richfiles-handoff", value: id.uuidString) }
        for (index, url) in openRequests.takePending().enumerated() {
            if index == 0 { workspace.activeModel.openFolder(url) }
            else { workspace.activeModel.newTab(url) }
        }
    }
}
