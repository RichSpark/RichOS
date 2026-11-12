import AppKit
import RichFilesCore
import SwiftUI
import UniformTypeIdentifiers

public enum RichFilesInstallation {
    static let preferences = UserDefaults(suiteName: "app.richos.richfiles") ?? .standard

    public static var embeddedApplication: URL {
        Bundle.main.bundleURL.appendingPathComponent("Contents/Applications/RichFiles.app", isDirectory: true)
    }

    @MainActor static var richOSApplication: URL? {
        let contents = Bundle.main.bundleURL.deletingLastPathComponent().deletingLastPathComponent()
        let candidates = [contents.lastPathComponent == "Contents" ? contents.deletingLastPathComponent() : nil,
                          NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.richos.app")]
        return candidates.compactMap { $0 }.first {
            Bundle(url: $0)?.bundleIdentifier == "com.richos.app"
                && FileManager.default.fileExists(atPath: $0.appendingPathComponent("Contents/Frameworks/libRichFilesRail.dylib").path)
        }
    }

    @MainActor public static var dockedRecoveryStore: URL? {
        BrowserActions.recoveryLocation?.deletingLastPathComponent().appendingPathComponent("DockedRecovery.json")
    }

    @MainActor public static func openStandaloneWindow(at folder: URL) async throws {
        let session = try BrowserModel.sessionData(for: folder)
        try await WorkspaceHandoff.send(session, to: .standalone, application: embeddedApplication)
    }
}

struct FolderWindowRequest: Codable, Hashable {
    var id = UUID()
    let folder: URL
}

@MainActor
public struct RichFilesScenes: Scene {
    private static let browserEvents: Set<String> = ["file://", "richfiles-workspace://"]
    private let delegate: FolderOpenDelegate
    @StateObject private var actions = BrowserActions(recoveryURL: BrowserActions.recoveryLocation,
                                                      peerRecoveryURL: RichFilesInstallation.dockedRecoveryStore)

    public init(delegate: FolderOpenDelegate) {
        self.delegate = delegate
    }

    public var body: some Scene {
        WindowGroup("RichFiles", id: "browser") {
            WindowBrowserView(start: Self.requestedFolder ?? FileManager.default.homeDirectoryForCurrentUser,
                        actions: actions, openRequests: delegate,
                        supportsSessionRestore: Self.requestedFolder == nil,
                        remembersWindowFrame: true,
                        launchHandoffID: Self.requestedHandoff, hostSwitch: Self.dockSwitch)
                .frame(minWidth: 720, minHeight: 420)
                .defaultAppStorage(RichFilesInstallation.preferences)
                .handlesExternalEvents(preferring: Self.browserEvents, allowing: Self.browserEvents)
        }
        .defaultSize(width: 1120, height: 700)
        .commands {
            BrowserCommands()
        }

        WindowGroup("RichFiles", id: "richfiles-folder", for: FolderWindowRequest.self) { $request in
            WindowBrowserView(start: request?.folder ?? FileManager.default.homeDirectoryForCurrentUser,
                              actions: actions, openRequests: delegate, hostSwitch: Self.dockSwitch)
                .frame(minWidth: 720, minHeight: 420)
                .defaultAppStorage(RichFilesInstallation.preferences)
                .handlesExternalEvents(preferring: Self.browserEvents, allowing: Self.browserEvents)
        }
        .defaultSize(width: 1120, height: 700)

        WindowGroup("RichFiles", id: "richfiles-handoff", for: String.self) { $ticket in
            WindowBrowserView(start: FileManager.default.homeDirectoryForCurrentUser, actions: actions,
                              openRequests: delegate, supportsSessionRestore: true,
                              handoffID: ticket, hostSwitch: Self.dockSwitch)
                .frame(minWidth: 720, minHeight: 420)
                .defaultAppStorage(RichFilesInstallation.preferences)
                .handlesExternalEvents(preferring: Self.browserEvents, allowing: Self.browserEvents)
        }
        .defaultSize(width: 1120, height: 700)

    }

    private static var dockSwitch: WindowBrowserView.HostSwitch? {
        RichFilesInstallation.richOSApplication.map { .init(target: .docked, application: $0) }
    }

    private static var requestedHandoff: String? {
        let arguments = CommandLine.arguments
        guard let flag = arguments.firstIndex(of: "--receive-workspace"),
              arguments.indices.contains(flag + 1),
              let id = UUID(uuidString: arguments[flag + 1]) else { return nil }
        return id.uuidString
    }

    private static var requestedFolder: URL? {
        let arguments = CommandLine.arguments
        if let index = arguments.firstIndex(of: "--folder"),
           arguments.indices.contains(index + 1), arguments[index + 1].hasPrefix("/") {
            return URL(fileURLWithPath: arguments[index + 1], isDirectory: true)
        }
        return nil
    }
}

/// The complete browser for a host that already owns its application window.
@MainActor
public struct RichFilesPane: View {
    @StateObject private var actions: BrowserActions
    @StateObject private var openRequests = FolderOpenDelegate()
    private let folder: URL
    private let openWindow: (URL) -> Void
    private let acceptsKeyboard: () -> Bool
    private let returnKeyboardFocus: @MainActor @Sendable () -> Void
    private let dockTarget: RichFilesDockTarget?
    private let didUndock: (() -> Void)?

    public init(
        folder: URL,
        recoveryStore: URL?,
        openWindow: @escaping (URL) -> Void,
        acceptsKeyboard: @escaping () -> Bool,
        returnKeyboardFocus: @escaping @MainActor @Sendable () -> Void = {},
        dockTarget: RichFilesDockTarget? = nil, didUndock: (() -> Void)? = nil
    ) {
        self.folder = folder
        self.openWindow = openWindow
        self.acceptsKeyboard = acceptsKeyboard
        self.returnKeyboardFocus = returnKeyboardFocus
        self.dockTarget = dockTarget
        self.didUndock = didUndock
        let peerStore = recoveryStore == RichFilesInstallation.dockedRecoveryStore ? BrowserActions.recoveryLocation : nil
        _actions = StateObject(wrappedValue: BrowserActions(recoveryURL: recoveryStore, peerRecoveryURL: peerStore))
    }

    public var body: some View {
        WindowBrowserView(start: folder, actions: actions, openRequests: openRequests,
                    openWindow: openWindow, acceptsKeyboard: acceptsKeyboard, returnKeyboardFocus: returnKeyboardFocus,
                    dockTarget: dockTarget, hostSwitch: dockTarget.map { _ in
                        .init(target: .standalone, application: RichFilesInstallation.embeddedApplication, didUndock: didUndock)
                    })
            .defaultAppStorage(RichFilesInstallation.preferences)
    }
}

@MainActor
public final class FolderOpenDelegate: NSObject, NSApplicationDelegate, ObservableObject {
    @Published private(set) var requestID = UUID()
    private var pending: [URL] = []
    private var handoffs: [UUID] = []
    private var openedHandoffs: Set<UUID> = []
    private var openedInitialBrowser = false
    private weak var primaryPlacementWindow: NSWindow?

    func claimPrimaryPlacement(for window: NSWindow) -> Bool {
        if primaryPlacementWindow == nil { primaryPlacementWindow = window }
        return primaryPlacementWindow === window
    }

    func savesPrimaryPlacement(for window: NSWindow) -> Bool {
        primaryPlacementWindow === window
    }

    func releasePrimaryPlacement(for window: NSWindow) {
        if primaryPlacementWindow === window { primaryPlacementWindow = nil }
    }

    public func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.servicesProvider = self
        NSUpdateDynamicServices()
    }

    @objc(openRichFiles:userData:error:)
    public func openRichFiles(
        _ pasteboard: NSPasteboard, userData: String?, error: AutoreleasingUnsafeMutablePointer<NSString?>?
    ) {
        NSWorkspace.shared.openApplication(at: Bundle.main.bundleURL,
                                           configuration: NSWorkspace.OpenConfiguration(), completionHandler: nil)
    }

    public func application(_ application: NSApplication, open urls: [URL]) {
        pending.append(contentsOf: urls.filter(\.isFileURL))
        for url in urls {
            if let id = WorkspaceHandoff.ticketID(from: url, for: .standalone),
               !openedHandoffs.contains(id), !handoffs.contains(id), WorkspaceHandoff.hasPendingTicket(id) {
                handoffs.append(id)
            }
        }
        requestID = UUID()
    }

    func takePending() -> [URL] {
        defer { pending.removeAll() }
        return pending
    }

    func markHandoffOpened(_ id: UUID) {
        openedHandoffs.insert(id)
        handoffs.removeAll { $0 == id }
    }

    func takeFirstHandoff() -> UUID? {
        guard let id = handoffs.first else { return nil }
        markHandoffOpened(id)
        return id
    }

    func takeHandoffs() -> [UUID] {
        let pending = handoffs
        openedHandoffs.formUnion(pending)
        handoffs.removeAll()
        return pending
    }

    func claimInitialBrowser() -> Bool {
        guard !openedInitialBrowser else { return false }
        openedInitialBrowser = true
        return true
    }
}

enum BrowserCommand: Codable, Hashable {
    case open, rename, trash, preview, address, search, back, forward, up, refresh, revealFolder
    case selectAll, copy, cut, paste, undo, redo, newFolder, newTab, closeTab, properties, workspaces, diskUsage
    case duplicates, checksum, compareFolders, settings
    case copyPath, actionPalette, addToShelf, fileShelf, splitView, switchPane
    case grid(Int, Int, Bool)
}

struct BrowserAction: Identifiable {
    let title: String
    let command: BrowserCommand
    var id: BrowserCommand { command }

    init(_ title: String, _ command: BrowserCommand) {
        self.title = title
        self.command = command
    }

    static let fileActions: [Self] = [
        .init("Open", .open), .init("Rename", .rename), .init("Move to Trash", .trash),
        .init("Quick Look", .preview), .init("Properties", .properties), .init("Add to File Shelf", .addToShelf)
    ]
    static let clipboard: [Self] = [
        .init("Cut Files", .cut), .init("Copy Files", .copy), .init("Paste Files", .paste),
        .init("Copy Path", .copyPath), .init("Undo File Action", .undo), .init("Redo File Action", .redo)
    ]
    static let tools: [Self] = [
        .init("Disk Usage", .diskUsage), .init("Find Duplicate Files", .duplicates),
        .init("Verify Checksum", .checksum), .init("Compare Folders", .compareFolders), .init("File Shelf", .fileShelf)
    ]
    static let navigation: [Self] = [
        .init("Back", .back), .init("Forward", .forward), .init("Up", .up), .init("Refresh", .refresh),
        .init("Go to Path", .address), .init("Search Folder", .search),
        .init("Expand to Current Folder", .revealFolder), .init("Workspaces", .workspaces),
        .init("Toggle Split View", .splitView), .init("Switch Pane", .switchPane)
    ]
    static let creation: [Self] = [
        .init("New Tab", .newTab), .init("Close Tab", .closeTab), .init("New Folder", .newFolder)
    ]
    static let all = fileActions + clipboard + tools + navigation + creation
        + [.init("Select All", .selectAll), .init("Settings", .settings)]
}

struct BrowserCommandKey: FocusedValueKey {
    typealias Value = (BrowserCommand) -> Void
}

extension FocusedValues {
    var browserCommand: ((BrowserCommand) -> Void)? {
        get { self[BrowserCommandKey.self] }
        set { self[BrowserCommandKey.self] = newValue }
    }
}

struct BrowserCommands: Commands {
    @FocusedValue(\.browserCommand) private var perform

    var body: some Commands {
        CommandGroup(replacing: .appSettings) {
            Button("Settings…") { perform?(.settings) }
                .keyboardShortcut(",")
                .disabled(perform == nil)
        }
        CommandGroup(after: .newItem) {
            ForEach(BrowserAction.creation) { action($0.title, $0.command) }
        }
        CommandMenu("Files") {
            ForEach(BrowserAction.fileActions) { action($0.title, $0.command) }
            Divider()
            ForEach(BrowserAction.clipboard) { action($0.title, $0.command) }
            Divider()
            ForEach(BrowserAction.tools) { action($0.title + "…", $0.command) }
            action("File Actions…", .actionPalette)
        }
        CommandMenu("Go") {
            ForEach(BrowserAction.navigation) { action($0.title + ($0.command == .workspaces ? "…" : ""), $0.command) }
        }
    }

    private func action(_ title: String, _ command: BrowserCommand) -> some View {
        Button(title) { perform?(command) }.disabled(perform == nil)
    }
}

enum BrowserTextSize: Int, CaseIterable {
    case standard = 0
    case large = 3
    case extraLarge = 6

    var title: String {
        switch self {
        case .standard: return "100%"
        case .large: return "125%"
        case .extraLarge: return "150%"
        }
    }

    var adjustment: CGFloat { CGFloat(rawValue) * 4 / 3 }
    var scale: CGFloat { 1 + adjustment / 16 }

    func font(size: CGFloat) -> Font {
        BrowserTypography.reading(size * scale)
    }
}

@MainActor
struct RichFilesSettings: View {
    @ObservedObject var workspace: WindowWorkspace
    let close: () -> Void
    @Environment(\.colorScheme) private var scheme
    @AppStorage(BrowserTheme.preferenceKey) private var theme = BrowserTheme.defaultTheme
    @State private var page = Page.general
    @State private var display = BrowserPreferences()

    private enum Page: String, CaseIterable, Identifiable {
        case general = "General"
        case keyboard = "Keyboard"
        case association = "Default file manager"
        case launch = "Launch and window"
        case workspaces = "Workspaces"

        var id: Self { self }
    }
    @AppStorage("richfiles.windowsKeys") private var windowsKeys = true
    @AppStorage("richfiles.textSize") private var textSize = BrowserTextSize.standard
    @AppStorage("richfiles.restoreSession") private var restoreSession = false
    @AppStorage("richfiles.shortcuts.v1") private var shortcutData = Data()
    @AppStorage("richfiles.previousFolderHandler") private var previousHandler = ""
    @State private var currentHandler: URL?
    @State private var updating = false
    @State private var status = ""
    @State private var shortcutAction = BrowserCommand.copyPath
    @State private var shortcutText = ""
    @State private var shortcutProblem = ""

    private var appURL: URL {
        let bundle = Bundle.main
        let app = bundle.bundleIdentifier == "app.richos.richfiles" ? bundle.bundleURL : RichFilesInstallation.embeddedApplication
        return app.resolvingSymlinksInPath()
    }
    private var installedBundle: Bool { Bundle(url: appURL)?.bundleIdentifier == "app.richos.richfiles" }
    private var isCurrent: Bool { currentHandler?.resolvingSymlinksInPath() == appURL }

    var body: some View {
        let palette = BrowserPalette(scheme)
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 12) {
                Image(systemName: "gearshape").foregroundStyle(palette.signal)
                Text("RichFiles Settings").font(BrowserTypography.heading(24 * textSize.scale))
                    .accessibilityAddTraits(.isHeader)
                Spacer()
                Button(action: close) { Image(systemName: "xmark") }
                    .buttonStyle(BrowserButtonStyle()).accessibilityLabel("Close Settings")
            }
            .padding(.horizontal, 20).padding(.vertical, 12)
            HStack(alignment: .top, spacing: 18) {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(Page.allCases) { choice in
                        Button { page = choice } label: {
                            Text(choice.rawValue)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.vertical, 2)
                        }
                        .buttonStyle(BrowserButtonStyle(selected: page == choice))
                        .accessibilityAddTraits(page == choice ? .isSelected : [])
                    }
                }
                .frame(width: 164 * textSize.scale)
                if page == .workspaces {
                    WorkspacesSheet(window: workspace, embedded: true)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 12) {
                            switch page {
                            case .general: general
                            case .keyboard: keyboard
                            case .association: association
                            case .launch: launch
                            case .workspaces: EmptyView()
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.trailing, 8)
                        .padding(.bottom, 12)
                    }
                }
            }
            .padding(.horizontal, 20).padding(.bottom, 8)
            Rectangle().fill(palette.line).frame(height: 1)
            HStack {
                Spacer()
                Button("Done", action: close)
                    .buttonStyle(BrowserButtonStyle(selected: true))
                    .keyboardShortcut(.cancelAction)
            }
            .padding(.horizontal, 20).padding(.vertical, 10)
        }
        .font(BrowserTypography.reading(16 * textSize.scale))
        .frame(minWidth: 600, idealWidth: 900, minHeight: 390, idealHeight: 670)
        .background(palette.floating)
        .onAppear { readDisplay(); readHandler(); readShortcut() }
        .onChange(of: shortcutAction) { _ in readShortcut() }
    }

    private var general: some View {
        Group {
            heading("Appearance")
            settingRow("Theme", "System follows the Mac’s light or dark appearance.") {
                segments(BrowserTheme.allCases, selection: $theme, title: \.title, label: "Theme")
            }
            settingRow("Text size", "100% is 16-point reading text.") {
                segments(BrowserTextSize.allCases, selection: $textSize, title: \.title, label: "Text size")
            }
            heading("Default view for every folder")
            LabeledContent("View", value: display.global.mode.title)
            LabeledContent("Sorted by", value: sortDescription(display.global.fileSort))
            LabeledContent("Details columns", value: "Name, Size, Kind, Modified")
            LabeledContent("Thumbnail size", value: "\(Int(display.global.iconSize)) points")
            Text("Changing the view in a folder changes the default, unless that folder has its own view.")
                .foregroundStyle(.secondary)
            heading("Folders with their own view")
            if display.folders.isEmpty {
                Text("Turn on Remember View for This Folder in the sort and view menu.")
                    .foregroundStyle(.secondary)
            }
            ForEach(display.folders.keys.sorted(), id: \.self) { path in
                HStack {
                    Label(URL(fileURLWithPath: path).lastPathComponent, systemImage: "pin")
                        .lineLimit(1).help(path)
                    Spacer()
                    Text(display.folders[path]?.mode.title ?? "")
                    Button("Use Default") {
                        workspace.useDefaultView(for: path)
                        readDisplay()
                    }
                    .buttonStyle(BrowserButtonStyle(bordered: true))
                }
            }
            heading("Names")
            Toggle("Always show filename extensions", isOn: .constant(true)).disabled(true)
            Toggle("Show hidden files", isOn: Binding(get: { display.showHidden }, set: { value in
                workspace.setHiddenFilesVisible(value)
                readDisplay()
            }))
            Toggle("Ask before a rename changes the extension", isOn: .constant(true)).disabled(true)
        }
    }

    private var keyboard: some View {
        Group {
            heading("Shortcut preset")
            Picker("Shortcut preset", selection: $windowsKeys) {
                Text("Windows (Control)").tag(true)
                Text("Mac (Command)").tag(false)
            }
            .pickerStyle(.radioGroup)
            Text("Enter opens, F2 renames, Delete moves to Trash and Space previews.")
                .foregroundStyle(.secondary)
            heading("Individual shortcuts")
            Picker("File Action", selection: $shortcutAction) {
                ForEach(BrowserAction.all + [.init("File Actions", .actionPalette)]) {
                    Text($0.title).tag($0.command)
                }
            }
            TextField("Shortcut", text: $shortcutText, prompt: Text("Control+Shift+J"))
                .accessibilityLabel("Shortcut")
            HStack {
                Button("Save Shortcut") { saveShortcut() }
                Button("Use Default") { resetShortcut() }
                    .disabled(BrowserShortcut.load(shortcutData)[shortcutAction] == nil)
            }
            .buttonStyle(BrowserButtonStyle(bordered: true))
            if !shortcutProblem.isEmpty { Text(shortcutProblem).foregroundStyle(.secondary) }
        }
    }

    private var association: some View {
        Group {
            heading("Use RichFiles as my default file manager")
            LabeledContent("Current app", value: currentHandler.map {
                FileManager.default.displayName(atPath: $0.path)
            } ?? "Unavailable")
            HStack {
                Button("Use RichFiles for Folders") { change(to: appURL, remember: true) }
                    .disabled(!installedBundle || isCurrent || updating || currentHandler == nil)
                Button("Restore Previous App") {
                    change(to: URL(fileURLWithPath: previousHandler), remember: false)
                }
                .disabled(!isCurrent || previousHandler.isEmpty || updating)
            }
            .buttonStyle(BrowserButtonStyle(bordered: true))
            Text("Finder still handles Desktop and Reveal in Finder. macOS may ask for consent.")
                .foregroundStyle(.secondary)
            if !status.isEmpty { Text(status).foregroundStyle(.secondary) }
        }
    }

    private var launch: some View {
        Group {
            heading("Launch RichFiles")
            settingRow("Dock and Applications", "Open RichFiles from Applications or keep it in the Dock.") {
                Image(nsImage: NSWorkspace.shared.icon(forFile: appURL.path))
                    .resizable().frame(width: 52, height: 52).accessibilityHidden(true)
            }
            Text("Assign Open RichFiles in System Settings → Keyboard → Keyboard Shortcuts → Services.")
            heading("Window")
            Toggle("Restore tabs when RichFiles reopens", isOn: $restoreSession)
        }
    }

    private func heading(_ title: String) -> some View {
        Text(title).fontWeight(.semibold).foregroundStyle(BrowserPalette(scheme).signal)
            .accessibilityAddTraits(.isHeader)
    }

    private func settingRow<Content: View>(
        _ title: String, _ description: String, @ViewBuilder content: () -> Content
    ) -> some View {
        let label = VStack(alignment: .leading, spacing: 3) {
            Text(title).fontWeight(.semibold)
            Text(description).foregroundStyle(.secondary)
        }
        return ViewThatFits(in: .horizontal) {
            HStack { label; Spacer(minLength: 12); content() }
            VStack(alignment: .leading, spacing: 7) { label; content() }
        }
        .padding(.vertical, 6)
        .overlay(alignment: .bottom) { Rectangle().fill(BrowserPalette(scheme).line).frame(height: 1) }
    }

    private func segments<Value: Hashable>(
        _ values: [Value], selection: Binding<Value>, title: KeyPath<Value, String>, label: String
    ) -> some View {
        HStack(spacing: 1) {
            ForEach(values, id: \.self) { value in
                Button(value[keyPath: title]) { selection.wrappedValue = value }
                    .buttonStyle(BrowserButtonStyle(selected: selection.wrappedValue == value))
                    .accessibilityAddTraits(selection.wrappedValue == value ? .isSelected : [])
            }
        }
        .fixedSize(horizontal: true, vertical: false)
        .padding(2)
        .overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(BrowserPalette(scheme).controlLine))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(label)
    }

    private func readDisplay() {
        display = BrowserPreferences.load(from: RichFilesInstallation.preferences)
    }

    private func sortDescription(_ sort: FileSort) -> String {
        let name: String
        switch sort.column {
        case .name: name = "Name"
        case .fileExtension: name = "Extension"
        case .type: name = "Kind"
        case .size: name = "Size"
        case .modified: name = "Date modified"
        }
        return name + (sort.ascending ? ", ascending" : ", descending") + (sort.foldersFirst ? ", folders first" : "")
    }

    private func readShortcut() {
        shortcutText = BrowserShortcut.load(shortcutData)[shortcutAction]?.label ?? ""
        shortcutProblem = ""
    }

    private func saveShortcut() {
        do {
            let updated = try BrowserShortcut.assign(shortcutText, to: shortcutAction, in: BrowserShortcut.load(shortcutData))
            shortcutData = try JSONEncoder().encode(updated)
            readShortcut()
        } catch { shortcutProblem = error.localizedDescription }
    }

    private func resetShortcut() {
        var updated = BrowserShortcut.load(shortcutData)
        updated.removeValue(forKey: shortcutAction)
        do {
            shortcutData = try JSONEncoder().encode(updated)
            readShortcut()
        } catch { shortcutProblem = error.localizedDescription }
    }

    private func readHandler() {
        currentHandler = NSWorkspace.shared.urlForApplication(toOpen: UTType.folder)
    }

    private func change(to url: URL, remember: Bool) {
        readHandler()
        guard remember ? currentHandler != nil && !isCurrent : isCurrent else {
            status = "The folder association changed. Check the current app and try again."
            return
        }
        guard FileManager.default.fileExists(atPath: url.path) else {
            status = "The previous app is unavailable."
            return
        }
        let prior = currentHandler?.path
        updating = true
        Task { @MainActor in
            defer { updating = false }
            do {
                try await NSWorkspace.shared.setDefaultApplication(at: url, toOpen: UTType.folder)
                readHandler()
                guard currentHandler?.resolvingSymlinksInPath() == url.resolvingSymlinksInPath() else {
                    status = "macOS did not confirm the folder association."
                    return
                }
                if remember, let prior { previousHandler = prior }
                if !remember { previousHandler = "" }
                status = "Folder association updated."
            } catch {
                readHandler()
                status = error.localizedDescription
            }
        }
    }
}
