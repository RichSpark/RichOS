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

    public func application(_ application: NSApplication, open urls: [URL]) {
        pending.append(contentsOf: urls.filter(\.isFileURL))
        requestID = UUID()
    }

    func takePending() -> [URL] {
        defer { pending.removeAll() }
        return pending
    }
}

enum BrowserCommand: Equatable {
    case open, rename, trash, preview, address, search, back, forward, up, refresh, revealFolder
    case selectAll, copy, cut, paste, undo, redo, newFolder, newTab, closeTab, properties, workspaces, diskUsage
    case duplicates, checksum, compareFolders
    case grid(Int, Int, Bool)
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
            action("New Tab", .newTab)
            action("Close Tab", .closeTab)
            action("New Folder", .newFolder)
        }
        CommandMenu("Files") {
            action("Open", .open)
            action("Rename", .rename)
            action("Move to Trash", .trash)
            action("Quick Look", .preview)
            action("Properties", .properties)
            Divider()
            action("Cut Files", .cut)
            action("Copy Files", .copy)
            action("Paste Files", .paste)
            action("Undo File Action", .undo)
            action("Redo File Action", .redo)
            action("Disk Usage…", .diskUsage)
            action("Find Duplicate Files…", .duplicates)
            action("Verify Checksum…", .checksum)
            action("Compare Folders…", .compareFolders)
        }
        CommandMenu("Go") {
            action("Back", .back)
            action("Forward", .forward)
            action("Up", .up)
            action("Refresh", .refresh)
            action("Go to Path", .address)
            action("Search Folder", .search)
            action("Expand to Current Folder", .revealFolder)
            action("Workspaces…", .workspaces)
        }
    }

    private func action(_ title: String, _ command: BrowserCommand) -> some View {
        Button(title) { perform?(command) }.disabled(perform == nil)
    }
}

struct RichFilesSettings: View {
    @AppStorage("richfiles.windowsKeys") private var windowsKeys = true
    @AppStorage("richfiles.restoreSession") private var restoreSession = false
    @AppStorage("richfiles.previousFolderHandler") private var previousHandler = ""
    @State private var currentHandler: URL?
    @State private var updating = false
    @State private var status = ""

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
        .onAppear { readHandler() }
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
