import AppKit
import Combine
import SwiftUI

enum BrowserPaneSide: String, Codable {
    case primary, secondary
}

@MainActor
final class WindowWorkspace: ObservableObject {
    static let maximumSessionBytes = 768 * 1024
    let primary: BrowserModel
    let git = GitPanelModel()
    @Published private(set) var secondary: BrowserModel?
    @Published private(set) var isSplit = false
    @Published private(set) var active = BrowserPaneSide.primary
    @Published var cleanupFolder: URL?
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

    func setHiddenFilesVisible(_ visible: Bool) {
        primary.showHidden = visible
        secondary?.showHidden = visible
    }

    func useDefaultView(for path: String) {
        var preferences = BrowserPreferences.load(from: defaults)
        preferences.folders.removeValue(forKey: path)
        preferences.save(to: defaults)
        for model in [primary, secondary].compactMap({ $0 }) where model.folder.path == path {
            model.setFolderViewOverride(false)
        }
    }

    func setCommands(for pane: BrowserPaneSide, owner: UUID, handler: @escaping (BrowserCommand) -> Bool) {
        commands[pane] = (owner, handler)
    }

    func clearCommands(for pane: BrowserPaneSide, owner: UUID) {
        if commands[pane]?.owner == owner { commands.removeValue(forKey: pane) }
    }

    @discardableResult
    func perform(_ command: BrowserCommand) -> Bool {
        guard cleanupFolder == nil else { return false }
        return commands[active]?.run(command) ?? false
    }

    func sessionData() -> Data? {
        guard let first = primary.sessionData() else { return nil }
        let second = secondary?.sessionData()
        guard secondary == nil || second != nil else { return nil }
        let snapshot = Session(primary: first, secondary: second, split: isSplit, active: active,
                               git: nil)
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
            git.close()
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
        if let state = snapshot.git {
            git.startSnapshot(state, folder: activeModel.folder)
        } else {
            git.close()
        }
        return true
    }

    static func gitSession(_ snapshot: GitPanelSnapshot) throws -> Data {
        let primary = try BrowserModel.sessionData(for: snapshot.repository.url)
        return try JSONEncoder().encode(Session(primary: primary, secondary: nil, split: false,
                                                active: .primary, git: snapshot))
    }

    private struct Session: Codable {
        let primary: Data
        let secondary: Data?
        let split: Bool
        let active: BrowserPaneSide
        let git: GitPanelSnapshot?
    }
}

@MainActor
struct WindowBrowserView: View {
    @StateObject private var workspace: WindowWorkspace
    @ObservedObject private var actions: BrowserActions
    @ObservedObject private var openRequests: FolderOpenDelegate
    @Environment(\.openWindow) private var openWindow
    @Environment(\.scenePhase) private var scenePhase
    @AppStorage("richfiles.sidebarVisible") private var sidebarVisible = true
    @State private var paneLayout = BrowserWindowPlacement.Layout()
    @AppStorage("richfiles.restoreSession") private var restoreSession = false
    @AppStorage("richfiles.lastBrowserSession.v1") private var lastBrowserSession = Data()
    @SceneStorage("richfiles.browserSession.v1") private var savedSession: Data?
    @SceneStorage("richfiles.windowPlacementSlot.v2") private var savedPlacementSlot = -1
    @State private var appeared = false
    @State private var nativeWindowNumber: Int?
    @State private var placementSlot: BrowserWindowPlacement.Slot?
    @State private var cleanLaunchDeadline: TimeInterval?
    @State private var switchError = ""
    @State private var showSwitchError = false

    private let start: URL
    private let supportsSessionRestore: Bool
    private let placementFamily: BrowserWindowPlacement.Family?
    private let handoffID: String?
    private let launchHandoffID: String?

    init(start: URL, actions: BrowserActions, openRequests: FolderOpenDelegate,
         supportsSessionRestore: Bool = false, placementFamily: BrowserWindowPlacement.Family? = nil,
         handoffID: String? = nil, launchHandoffID: String? = nil) {
        _workspace = StateObject(wrappedValue: WindowWorkspace(start: start))
        self.start = start
        self.actions = actions
        self.openRequests = openRequests
        self.supportsSessionRestore = supportsSessionRestore
        self.placementFamily = placementFamily
        self.handoffID = handoffID
        self.launchHandoffID = launchHandoffID
    }

    var body: some View {
        ZStack {
            GitBrowserSplit(git: workspace.git) { browserLayout }
                .opacity(workspace.cleanupFolder == nil ? 1 : 0)
                .allowsHitTesting(workspace.cleanupFolder == nil)
                .accessibilityHidden(workspace.cleanupFolder != nil)
            if let folder = workspace.cleanupFolder {
                DiskCleanupView(folder: folder, actions: actions, volumes: workspace.activeModel.volumes, close: {
                    workspace.cleanupFolder = nil
                }, openInBrowser: { url, directory in
                    workspace.cleanupFolder = nil
                    if directory { workspace.activeModel.openFolder(url) }
                    else { workspace.activeModel.revealFile(url) }
                }, titleChanged: { title in
                    if let number = nativeWindowNumber { NSApp.window(withWindowNumber: number)?.title = title }
                })
            }
        }
        .modifier(BrowserAppearance(setsWindowAppearance: true))
        .background { BrowserWindowReference(report: attachWindow) }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { notice in
            if let window = notice.object as? NSWindow, window.windowNumber == nativeWindowNumber {
                attachWindow(window)
                workspace.git.refreshOnWindowFocus()
            }
        }
        .onReceive(Publishers.Merge3(
            NotificationCenter.default.publisher(for: NSWindow.didMoveNotification),
            NotificationCenter.default.publisher(for: NSWindow.didResizeNotification),
            NotificationCenter.default.publisher(for: NSWindow.willCloseNotification)
        ), perform: saveWindowFrame)
        .focusedSceneValue(\.browserCommand) { command in
            workspace.perform(command)
        }
        .alert("Could not open Git in RichFiles", isPresented: $showSwitchError) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(switchError)
        }
        .onAppear(perform: appear)
        .onChange(of: workspace.isSplit) { _ in saveSession(rememberWindow: true) }
        .onChange(of: workspace.active) { _ in saveSession(rememberWindow: true) }
        .onChange(of: workspace.cleanupFolder) { folder in
            if folder == nil, let number = nativeWindowNumber {
                updateTitle(of: NSApp.window(withWindowNumber: number), folder: workspace.activeModel.folder)
            }
        }
        .onReceive(workspace.activeModel.$folder) { _ in
            // The folder publisher fires before its model stores the value.
            // A handoff restores the folder and Git selection together.
            Task { @MainActor in
                let folder = workspace.activeModel.folder
                if workspace.cleanupFolder == nil, let number = nativeWindowNumber {
                    updateTitle(of: NSApp.window(withWindowNumber: number), folder: folder)
                }
                workspace.git.follow(folder)
            }
        }
        .onChange(of: scenePhase) { if $0 != .active { saveSession() } }
        .onChange(of: restoreSession) { _ in saveSession(rememberWindow: true) }
        .onChange(of: openRequests.requestID) { _ in receiveFolderOpen() }
        .onDisappear { saveSession() }
    }

    private var browserLayout: some View {
        GeometryReader { geometry in
            let layout = BrowserWindowLayout(sidebarWidth: paneLayout.sidebarWidth, paneRatio: paneLayout.paneRatio)
            layout {
                pane(workspace.primary, side: .primary, size: geometry.size)
                if workspace.isSplit, let second = workspace.secondary {
                    pane(second, side: .secondary, size: geometry.size)
                    BrowserPaneDivider(name: "Split pane divider", resize: { position in
                        let side = sidebarVisible ? layout.sidebarWidth(in: geometry.size.width) + BrowserWindowLayout.gutter : 0
                        let width = max(1, geometry.size.width - side - BrowserWindowLayout.gutter)
                        paneLayout.paneRatio = min(0.75, max(0.25, (position - side) / width))
                    }, finished: savePaneLayout)
                    .layoutValue(key: BrowserLayoutKey.self, value: .splitDivider)
                }
                if sidebarVisible {
                    BrowserPaneDivider(name: "Sidebar divider", resize: {
                        paneLayout.sidebarWidth = min(340, max(150, $0))
                    }, finished: savePaneLayout)
                        .layoutValue(key: BrowserLayoutKey.self, value: .sidebarDivider)
                }
            }
            .coordinateSpace(name: "richfiles-window")
        }
    }

    private func appear() {
        if !appeared {
            appeared = true
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

    private func pane(_ model: BrowserModel, side: BrowserPaneSide, size: CGSize) -> some View {
        BrowserView(model: model, workspace: workspace, side: side, windowSize: size, actions: actions,
                    acceptsKeyboard: { workspace.active == side && workspace.cleanupFolder == nil },
                    sessionChanged: { saveSession(rememberWindow: true) })
    }

    private func saveSession(rememberWindow: Bool = false) {
        guard supportsSessionRestore, appeared else { return }
        savedSession = restoreSession ? workspace.sessionData() : nil
        if rememberWindow || !restoreSession { lastBrowserSession = savedSession ?? Data() }
    }

    private func updateTitle(of window: NSWindow?, folder: URL) {
        let name = folder.path == "/" ? "This Mac" : folder.lastPathComponent
        window?.title = "\(name) — RichFiles"
    }

    private func attachWindow(_ window: NSWindow?) {
        if nativeWindowNumber != window?.windowNumber || (placementFamily != nil && placementSlot == nil) {
            releasePlacement()
            if let placementFamily, let window {
                let slot = openRequests.windowPlacement.claim(placementFamily, preferred: savedPlacementSlot)
                paneLayout = openRequests.windowPlacement.savedLayout(for: slot)
                openRequests.windowPlacement.restore(slot, to: window)
                placementSlot = slot
                savedPlacementSlot = slot.index
            }
            nativeWindowNumber = window?.windowNumber
            updateTitle(of: window, folder: workspace.activeModel.folder)
        }
        receiveFolderOpen()
    }

    private func saveWindowFrame(_ notice: Notification) {
        guard let slot = placementSlot, let window = notice.object as? NSWindow,
              window.windowNumber == nativeWindowNumber else { return }
        if !window.isMiniaturized, !window.styleMask.contains(.fullScreen) {
            openRequests.windowPlacement.save(window.frame, in: slot)
        }
        if notice.name == NSWindow.willCloseNotification { releasePlacement() }
    }

    private func releasePlacement() {
        if let slot = placementSlot {
            savePaneLayout()
            openRequests.windowPlacement.release(slot)
        }
        placementSlot = nil
    }

    private func savePaneLayout() {
        guard let slot = placementSlot else { return }
        openRequests.windowPlacement.saveLayout(paneLayout, in: slot)
    }

    private func acceptHandoff(_ id: UUID) {
        do { try WorkspaceHandoff.accept(id, into: workspace) }
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
        guard let nativeWindowNumber,
              (NSApp.keyWindow ?? NSApp.mainWindow)?.windowNumber == nativeWindowNumber else { return }
        if openRequests.imageViewerSettingsRequested {
            // On cold launch the pane registers its handler in onAppear.
            Task { @MainActor in
                if openRequests.imageViewerSettingsRequested,
                   (NSApp.keyWindow ?? NSApp.mainWindow)?.windowNumber == nativeWindowNumber,
                   workspace.perform(.imageViewerSettings) {
                    openRequests.imageViewerSettingsOpened()
                }
            }
        }
        let folders = openRequests.takePending()
        if !folders.isEmpty { workspace.cleanupFolder = nil }
        if folders.count == 1, let location = folders.first {
            if location.hasDirectoryPath || (try? location.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true {
                workspace.activeModel.openFolder(location)
            } else {
                workspace.activeModel.revealFile(location)
            }
        } else {
            for folder in folders { workspace.activeModel.newTab(folder) }
        }
    }
}

private struct BrowserWindowReference: NSViewRepresentable {
    let report: (NSWindow?) -> Void

    func makeNSView(context: Context) -> Reporter { Reporter(report: report) }

    func updateNSView(_ view: Reporter, context: Context) { view.report = report }

    final class Reporter: NSView {
        var report: (NSWindow?) -> Void

        init(report: @escaping (NSWindow?) -> Void) {
            self.report = report
            super.init(frame: .zero)
        }

        required init?(coder: NSCoder) { nil }

        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.report(self.window)
            }
        }
    }
}
