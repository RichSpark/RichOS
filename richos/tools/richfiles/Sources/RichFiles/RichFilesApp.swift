import AppKit
import RichFilesCore
import SwiftUI
import UniformTypeIdentifiers

@MainActor
public struct RichFilesScenes: Scene {
    private let delegate: FolderOpenDelegate
    @StateObject private var actions = BrowserActions(recoveryURL: BrowserActions.recoveryLocation)

    public init(delegate: FolderOpenDelegate) {
        self.delegate = delegate
    }

    public var body: some Scene {
        WindowGroup("RichFiles", id: "browser") {
            BrowserView(start: Self.requestedFolder ?? FileManager.default.homeDirectoryForCurrentUser,
                        actions: actions, openRequests: delegate,
                        supportsSessionRestore: Self.requestedFolder == nil)
                .frame(minWidth: 720, minHeight: 420)
        }
        .defaultSize(width: 1120, height: 700)
        .commands {
            BrowserCommands()
        }

        Settings {
            RichFilesSettings()
        }
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
    private let openWindow: () -> Void
    private let acceptsKeyboard: () -> Bool

    public init(
        folder: URL,
        recoveryStore: URL?,
        openWindow: @escaping () -> Void,
        acceptsKeyboard: @escaping () -> Bool
    ) {
        self.folder = folder
        self.openWindow = openWindow
        self.acceptsKeyboard = acceptsKeyboard
        _actions = StateObject(wrappedValue: BrowserActions(recoveryURL: recoveryStore))
    }

    public var body: some View {
        BrowserView(start: folder, actions: actions, openRequests: openRequests,
                    openWindow: openWindow, acceptsKeyboard: acceptsKeyboard)
    }
}

@MainActor
public final class FolderOpenDelegate: NSObject, NSApplicationDelegate, ObservableObject {
    @Published private(set) var requestID = UUID()
    private var pending: [URL] = []
    private var openedInitialBrowser = false

    public func application(_ application: NSApplication, open urls: [URL]) {
        pending.append(contentsOf: urls.filter(\.isFileURL))
        requestID = UUID()
    }

    func takePending() -> [URL] {
        defer { pending.removeAll() }
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
    case duplicates, checksum, compareFolders
    case copyPath, actionPalette, addToShelf, fileShelf
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
        .init("Expand to Current Folder", .revealFolder), .init("Workspaces", .workspaces)
    ]
    static let creation: [Self] = [
        .init("New Tab", .newTab), .init("Close Tab", .closeTab), .init("New Folder", .newFolder)
    ]
    static let all = fileActions + clipboard + tools + navigation + creation + [.init("Select All", .selectAll)]
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

struct RichFilesSettings: View {
    @AppStorage("richfiles.windowsKeys") private var windowsKeys = true
    @AppStorage("richfiles.restoreSession") private var restoreSession = false
    @AppStorage("richfiles.shortcuts.v1") private var shortcutData = Data()
    @AppStorage("richfiles.previousFolderHandler") private var previousHandler = ""
    @State private var currentHandler: URL?
    @State private var updating = false
    @State private var status = ""
    @State private var shortcutAction = BrowserCommand.copyPath
    @State private var shortcutText = ""
    @State private var shortcutProblem = ""

    private var appURL: URL { Bundle.main.bundleURL.resolvingSymlinksInPath() }
    private var installedBundle: Bool { appURL.pathExtension == "app" }
    private var isCurrent: Bool { currentHandler?.resolvingSymlinksInPath() == appURL }

    var body: some View {
        Form {
            Toggle("Restore tabs when RichFiles reopens", isOn: $restoreSession)
            Section("Keyboard") {
                Picker("Shortcuts", selection: $windowsKeys) {
                    Text("Windows (Control)").tag(true)
                    Text("Mac (Command)").tag(false)
                }
                Text("Enter opens, F2 renames, Delete moves to Trash and Space previews.")
                    .foregroundStyle(.secondary)
            }
            Section("Custom Shortcuts") {
                Picker("File Action", selection: $shortcutAction) {
                    ForEach(BrowserAction.all + [.init("File Actions", .actionPalette)]) {
                        Text($0.title).tag($0.command)
                    }
                }
                TextField("Shortcut", text: $shortcutText, prompt: Text("Control+Shift+J"))
                HStack {
                    Button("Save Shortcut") { saveShortcut() }
                    Button("Use Default") { resetShortcut() }
                        .disabled(BrowserShortcut.load(shortcutData)[shortcutAction] == nil)
                }
                if !shortcutProblem.isEmpty { Text(shortcutProblem).foregroundStyle(.secondary) }
            }
            Section("Folder association") {
                LabeledContent("Current app", value: currentHandler.map {
                    FileManager.default.displayName(atPath: $0.path)
                } ?? "Unavailable")
                Button("Use RichFiles for Folders") { change(to: appURL, remember: true) }
                    .disabled(!installedBundle || isCurrent || updating || currentHandler == nil)
                Button("Restore Previous App") {
                    change(to: URL(fileURLWithPath: previousHandler), remember: false)
                }
                .disabled(!isCurrent || previousHandler.isEmpty || updating)
                Text("Finder still handles Desktop and Reveal in Finder. macOS may ask for consent.")
                    .foregroundStyle(.secondary)
                if !status.isEmpty { Text(status).foregroundStyle(.secondary) }
            }
        }
        .padding(20)
        .frame(width: 520)
        .onAppear { readHandler(); readShortcut() }
        .onChange(of: shortcutAction) { _ in readShortcut() }
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
