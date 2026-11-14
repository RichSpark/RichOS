import AppKit
import RichFilesCore
import SwiftUI
import UniformTypeIdentifiers

private struct BrowserControlFocusKey: FocusedValueKey {
    typealias Value = Bool
}

private extension FocusedValues {
    var browserControl: Bool? {
        get { self[BrowserControlFocusKey.self] }
        set { self[BrowserControlFocusKey.self] = newValue }
    }
}

@MainActor
struct BrowserView: View {
    @ObservedObject private var model: BrowserModel
    @ObservedObject private var workspace: WindowWorkspace
    @ObservedObject private var actions: BrowserActions
    @Environment(\.openWindow) private var openWindow
    @Environment(\.colorScheme) private var scheme
    private let side: BrowserPaneSide
    private let windowSize: CGSize
    private let hostOpenWindow: ((URL) -> Void)?
    private let acceptsKeyboard: () -> Bool
    private let returnKeyboardFocus: @MainActor @Sendable () -> Void
    private let sessionChanged: () -> Void
    private let hostSwitchTitle: String?
    private let switchHost: () -> Void
    @AppStorage("richfiles.windowsKeys") private var windowsKeys = true
    @AppStorage("richfiles.textSize") private var textSize = BrowserTextSize.standard
    @AppStorage("richfiles.shortcuts.v1") private var shortcutData = Data()
    @AppStorage("richfiles.sidebarVisible") private var sidebarVisible = true
    @AppStorage("richfiles.inspectorVisible") private var inspectorVisible = false
    @FocusState private var focus: FocusTarget?
    @FocusedValue(\.browserControl) private var controlFocused
    @State private var editingAddress = false
    @State private var address = ""
    @State private var suggestions: [URL] = []
    @State private var sheet: BrowserSheet?
    @State private var transfersVisible = false
    @State private var feedback = ""
    @State private var rangeAnchor: URL?
    @State private var selectionCursor: URL?
    @State private var prefixText = ""
    @State private var prefixTime: TimeInterval = 0
    @State private var listRows = 12
    @State private var iconColumns = 4
    @State private var detailsViewport = DetailsViewport()
    @State private var dropOperation: DropOperation?
    @State private var dropDestination: BrowserTransferDestination?
    @State private var destinationRefresh = UUID()
    @State private var selectedReferences: [URL: FileReference] = [:]
    @State private var preparingAction = false
    @State private var rowFrames: [URL: CGRect] = [:]
    @State private var fileColumnFrame: CGRect = .null
    @State private var selectionRectangle: CGRect?
    @State private var folderReveal: FolderTreeReveal?
    @State private var commandOwner = UUID()
    @State private var paletteChoice: BrowserCommand?

    private var palette: BrowserPalette { BrowserPalette(scheme) }
    private var activePane: Bool { !workspace.isSplit || workspace.active == side }

    private var rule: some View { Rectangle().fill(palette.line).frame(height: 1) }

    private enum FocusTarget: Hashable { case address, search, items }

    private struct DestinationRequest: Hashable {
        let folder: URL
        let disconnected: Bool
        let refresh: UUID
    }

    init(model: BrowserModel, workspace: WindowWorkspace, side: BrowserPaneSide, windowSize: CGSize,
         actions: BrowserActions, openWindow: ((URL) -> Void)? = nil,
         acceptsKeyboard: @escaping () -> Bool, returnKeyboardFocus: @escaping @MainActor @Sendable () -> Void,
         hostSwitchTitle: String?, switchHost: @escaping () -> Void, sessionChanged: @escaping () -> Void) {
        self.model = model
        self.workspace = workspace
        self.side = side
        self.windowSize = windowSize
        self.actions = actions
        hostOpenWindow = openWindow
        self.acceptsKeyboard = acceptsKeyboard
        self.returnKeyboardFocus = returnKeyboardFocus
        self.sessionChanged = sessionChanged
        self.hostSwitchTitle = hostSwitchTitle
        self.switchHost = switchHost
    }

    var body: some View {
        Group {
            if activePane {
                VStack(spacing: 0) {
                    tabs.focusedValue(\.browserControl, true)
                    rule
                    navigationBar.focusedValue(\.browserControl, true)
                    if editingAddress && !suggestions.isEmpty {
                        addressSuggestions.focusedValue(\.browserControl, true)
                    }
                    commandBar.focusedValue(\.browserControl, true)
                    rule
                }
                .font(BrowserTypography.reading(16 * textSize.scale))
                .background(palette.pane)
                .layoutValue(key: BrowserLayoutKey.self, value: .toolbar)
                if sidebarVisible {
                    sidebar.font(BrowserTypography.reading(16 * textSize.scale))
                        .focusedValue(\.browserControl, true)
                        .layoutValue(key: BrowserLayoutKey.self, value: .sidebar)
                }
            }
            paneContent.layoutValue(key: BrowserLayoutKey.self, value: side == .primary ? .primary : .secondary)
            if activePane {
                VStack(spacing: 0) {
                    rule
                    statusBar.focusedValue(\.browserControl, true)
                }
                .layoutValue(key: BrowserLayoutKey.self, value: .status)
            }
        }
    }

    private var paneContent: some View {
        HSplitView {
            VStack(spacing: 0) {
                if workspace.isSplit { paneHeading }
                itemPane
                if transfersVisible, actions.transfer != nil {
                    rule
                    TransferPanel(actions: actions) { transfersVisible = false }
                        .focusedValue(\.browserControl, true)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            if inspectorVisible && !workspace.isSplit {
                inspector.frame(minWidth: 165, idealWidth: 320, maxWidth: 410)
                    .background(palette.chrome)
                    .focusedValue(\.browserControl, true)
            }
        }
        .font(BrowserTypography.reading(16 * textSize.scale))
        .background(palette.pane)
        .accessibilityIdentifier(side == .primary ? "richfiles-primary-pane" : "richfiles-secondary-pane")
        .background(KeyboardBridge(windowsKeys: windowsKeys, acceptsInput: acceptsKeyboard,
                                   controlFocused: controlFocused == true,
                                   shortcuts: BrowserShortcut.load(shortcutData),
                                   activate: { workspace.select(side) },
                                   perform: perform, prefix: selectPrefix))
        .onAppear { workspace.setCommands(for: side, owner: commandOwner, handler: perform) }
        .onDisappear { workspace.clearCommands(for: side, owner: commandOwner) }
        .onChange(of: model.tabs.map(\.folder)) { _ in sessionChanged() }
        .onChange(of: model.currentTabID) { _ in sessionChanged() }
        .onChange(of: model.scrollAnchor) { _ in sessionChanged() }
        .onChange(of: actions.mutationRevision) { _ in model.refresh() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            actions.refreshClipboard()
        }
        .onChange(of: model.folder) { _ in
            let returnFocus = editingAddress
            editingAddress = false
            focus = nil
            if returnFocus { DispatchQueue.main.async(execute: returnKeyboardFocus) }
            suggestions = []
            feedback = ""
            rangeAnchor = nil
            selectionCursor = nil
            prefixText = ""
            prefixTime = 0
        }
        .task(id: model.selected) { await captureSelection() }
        .task(id: DestinationRequest(folder: model.folder, disconnected: model.isDisconnected,
                                     refresh: destinationRefresh)) {
            await captureDropDestination()
        }
        .sheet(item: $sheet, onDismiss: runPaletteChoice) { value in
            sheetContents(value)
                .font(BrowserTypography.reading(16 * textSize.scale))
                .background(palette.floating)
        }
    }

    private func runPaletteChoice() {
        guard let command = paletteChoice else { return }
        paletteChoice = nil
        _ = perform(command)
    }

    private var navigationBar: some View {
        HStack(spacing: 7) {
            barButton("Back", "chevron.left") { model.goBack() }.disabled(!model.canGoBack)
            barButton("Forward", "chevron.right") { model.goForward() }.disabled(!model.canGoForward)
            barButton("Up", "arrow.up") { model.goUp() }.disabled(model.folder.path == "/")
            barButton("Refresh", "arrow.clockwise") { _ = perform(.refresh) }
            addressBar.layoutPriority(1)
            searchBar
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
    }

    private var addressBar: some View {
        HStack(spacing: 2) {
            if editingAddress {
                TextField("Folder path", text: $address)
                    .textFieldStyle(.plain)
                    .focused($focus, equals: .address)
                    .accessibilityIdentifier("folder-address")
                    .onSubmit { submitAddress(address) }
                    .onExitCommand { editingAddress = false; focus = nil }
                    .task(id: address) {
                        let requested = address
                        let result = (try? await BrowserModel.addressSuggestions(
                            requested, relativeTo: model.folder, showHidden: model.showHidden, limit: 8
                        )) ?? []
                        if !Task.isCancelled && editingAddress && address == requested { suggestions = result }
                    }
            } else {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(Array(pathCrumbs.enumerated()), id: \.element) { index, path in
                            if index > 0 {
                                Image(systemName: "chevron.right").font(.system(size: 9)).foregroundStyle(palette.muted)
                            }
                            Button { model.openFolder(path) } label: {
                                if path.path == "/" {
                                    Label("This Mac", systemImage: "desktopcomputer")
                                } else if path.path == FileManager.default.homeDirectoryForCurrentUser.path {
                                    Label("Home", systemImage: "house")
                                } else {
                                    Text(path.lastPathComponent)
                                }
                            }
                            .buttonStyle(.plain)
                            .fontWeight(path == model.breadcrumbs.last ? .semibold : .regular)
                            .foregroundStyle(path == model.breadcrumbs.last ? palette.ink : palette.muted)
                            .help(path.path)
                        }
                    }
                }
            }
            barButton("Edit path", "pencil") { editAddress() }
            barButton("Copy current path", "doc.on.doc") { copyPaths([model.folder]) }
        }
        .padding(.leading, 9).padding(.trailing, 3)
        .frame(minHeight: 36 * textSize.scale)
        .background(palette.chrome, in: RoundedRectangle(cornerRadius: 7))
        .overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(
            focus == .address ? palette.signal : palette.controlLine, lineWidth: focus == .address ? 2 : 1
        ).allowsHitTesting(false))
    }

    private var pathCrumbs: [URL] {
        let paths = model.breadcrumbs
        guard let home = paths.firstIndex(where: { $0.path == FileManager.default.homeDirectoryForCurrentUser.path }),
              let root = paths.first, home > 1 else { return paths }
        return [root] + Array(paths[home...])
    }

    private var searchBar: some View {
        HStack(spacing: 5) {
            Image(systemName: "magnifyingglass").foregroundStyle(palette.muted).accessibilityHidden(true)
            // The prompt is drawn in `palette.hint`, not by the field: the system placeholder color
            // cannot be set on macOS 13 and measured 1.80:1 light and 2.19:1 dark.
            ZStack(alignment: .leading) {
                if model.searchText.isEmpty {
                    Text(model.searchContent ? "Search text contents" : "Search " + model.folder.lastPathComponent)
                        .foregroundStyle(palette.hint)
                        .lineLimit(1)
                        .allowsHitTesting(false)
                        .accessibilityHidden(true)
                }
                TextField("", text: $model.searchText)
                    .textFieldStyle(.plain)
                    .focused($focus, equals: .search)
                    .accessibilityLabel(model.searchContent ? "Search text contents" : "Search this folder")
                    .accessibilityIdentifier("folder-search")
                    .onSubmit { startSearch() }
            }
            if model.searchActive {
                barButton("Clear Search", "xmark.circle") { model.clearSearch() }
            }
            Menu {
                Button("This Folder") { model.searchRecursively = false }
                Button("Subfolders") { model.searchRecursively = true }
                Divider()
                Button("Search Filters…") { sheet = .searchOptions }
            } label: {
                if windowSize.width >= 840 {
                    Text(model.searchRecursively ? "Subfolders" : "This Folder").fixedSize()
                } else {
                    Image(systemName: "line.3.horizontal.decrease")
                }
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .accessibilityLabel("Search Scope")
        }
        .padding(.horizontal, 8)
        .frame(width: min(365, max(160, windowSize.width * 0.26)))
        .frame(minHeight: 36 * textSize.scale)
        .background(palette.chrome, in: RoundedRectangle(cornerRadius: 7))
        .overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(
            focus == .search ? palette.signal : palette.controlLine, lineWidth: focus == .search ? 2 : 1
        ).allowsHitTesting(false))
    }

    private var addressSuggestions: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(suggestions, id: \.self) { suggestion in
                Button { submitAddress(suggestion.path) } label: {
                    Label(suggestion.path, systemImage: "folder").lineLimit(1).truncationMode(.middle)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.vertical, 4)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 14)
        .padding(.bottom, 6)
    }

    private var sidebar: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    VStack(alignment: .leading, spacing: 2) {
                        sideHeading("Quick Access")
                        ForEach(Array(model.favorites.enumerated()), id: \.element) { index, url in
                            FolderTreeRow(url: url, title: url.lastPathComponent, symbol: "pin",
                                          current: model.folder, showHidden: model.showHidden,
                                          reveal: reveal(for: url), open: model.openFolder, isPin: true)
                                .contextMenu {
                                    Button("Remove from Quick Access") { model.removeFavorite(url) }
                                    Button("Move Up") { model.moveFavorite(from: IndexSet(integer: index), to: index - 1) }
                                        .disabled(index == 0)
                                    Button("Move Down") { model.moveFavorite(from: IndexSet(integer: index), to: index + 2) }
                                        .disabled(index == model.favorites.count - 1)
                                }
                        }
                        if !model.favorites.contains(where: { $0.path == model.folder.path }) {
                            barButton("Pin Current Folder", "plus", showTitle: true) { model.addFavorite(model.folder) }
                        }
                    }
                    VStack(alignment: .leading, spacing: 2) {
                        HStack {
                            sideHeading("This Mac")
                            Spacer()
                            barButton("Expand to Current Folder", "list.bullet.indent") { expandToCurrentFolder() }
                        }
                        ForEach(model.places) { place in
                            FolderTreeRow(url: place.url, title: place.title, symbol: place.symbol,
                                          current: model.folder, showHidden: model.showHidden,
                                          reveal: reveal(for: place.url), open: model.openFolder)
                        }
                        ForEach(model.volumes) { volume in
                            HStack(alignment: .top, spacing: 1) {
                                FolderTreeRow(url: volume.url, title: volume.name,
                                              symbol: volume.isLocal == false ? "network" : "externaldrive",
                                              current: model.folder, showHidden: model.showHidden,
                                              reveal: reveal(for: volume.url), open: model.openFolder, volume: volume)
                                if volume.isEjectable == true {
                                    barButton("Eject \(volume.name)", "eject") { eject(volume) }
                                        .disabled(actions.busy)
                                }
                            }
                            .contextMenu { Button("Copy Path") { copyPaths([volume.url]) } }
                        }
                        if !model.volumeStatus.isEmpty {
                            Text(model.volumeStatus).font(textSize.font(size: 14))
                                .foregroundStyle(palette.muted)
                        }
                        barButton("Connect to Server", "network", showTitle: true) { sheet = .connect }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(8)
            }
            rule.padding(.horizontal, 8)
            Button { sheet = .trash } label: {
                Label("Trash", systemImage: "trash")
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.leading, 10)
            }
            .buttonStyle(BrowserButtonStyle())
            .padding(.horizontal, 8).padding(.vertical, 3)
        }
        .background(palette.sidebar)
        .accessibilityLabel("Folder sidebar")
    }

    private func reveal(for root: URL) -> FolderTreeReveal? {
        folderReveal?.root == root.standardizedFileURL.path ? folderReveal : nil
    }

    private func expandToCurrentFolder() {
        let roots = model.places.map(\.url) + model.favorites + model.volumes.map(\.url)
        guard let request = FolderTreeReveal(folder: model.folder, roots: roots) else {
            feedback = "The current folder is outside the folder tree."
            return
        }
        sidebarVisible = true
        folderReveal = request
    }

    private func sideHeading(_ label: String) -> some View {
        Text(label).fontWeight(.semibold).foregroundStyle(palette.muted)
            .padding(.leading, 5).padding(.vertical, 5).accessibilityAddTraits(.isHeader)
    }

    private var tabs: some View {
        HStack(spacing: 5) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(alignment: .bottom, spacing: 4) {
                    ForEach(model.tabs) { tab in
                        let current = tab.id == model.currentTabID
                        HStack(spacing: 6) {
                            Button {
                                workspace.select(side)
                                model.switchTab(tab.id)
                            } label: {
                                Label {
                                    Text(tab.title).lineLimit(1)
                                } icon: {
                                    Image(systemName: "folder.fill").foregroundStyle(palette.folder)
                                }
                                .fontWeight(current ? .semibold : .regular)
                            }
                            .buttonStyle(.plain)
                            .help(tab.folder.path)
                            .accessibilityAddTraits(current ? .isSelected : [])
                            if model.tabs.count > 1 {
                                Button { model.closeTab(tab.id) } label: {
                                    Image(systemName: "xmark").font(.system(size: 10)).padding(4)
                                }.buttonStyle(.plain).accessibilityLabel("Close \(tab.title) tab")
                            }
                        }
                        .padding(.horizontal, 12)
                        .frame(minHeight: 36 * textSize.scale)
                        .background(current ? palette.pane : .clear, in: RoundedRectangle(cornerRadius: 6))
                        .overlay(alignment: .top) {
                            if current {
                                Capsule().fill(palette.signal).frame(height: 3).padding(.horizontal, 7)
                                    .allowsHitTesting(false)
                            }
                        }
                    }
                    barButton("New Tab", "plus") { model.newTab() }
                }
            }
            barButton("Workspaces", "rectangle.on.rectangle", showTitle: true) { sheet = .workspaces }
            if let hostSwitchTitle {
                barButton(hostSwitchTitle, hostOpenWindow == nil ? "rectangle.portrait.and.arrow.forward" :
                            "arrow.up.right.square", showTitle: true, bordered: true, action: switchHost)
            }
            barButton("Find a Command", "chevron.right.square") { _ = perform(.actionPalette) }
            barButton("Settings", "gearshape") { sheet = .settings }
        }
        .padding(.horizontal, 10)
        .padding(.top, 5)
        .background(palette.chrome)
    }

    private var paneHeading: some View {
        HStack {
            Label(model.folder.lastPathComponent, systemImage: "folder.fill").lineLimit(1)
            Spacer(minLength: 4)
            if activePane { Text("Active pane").foregroundStyle(palette.signal) }
        }
        .fontWeight(.semibold)
        .padding(.horizontal, 10).padding(.vertical, 6)
        .overlay(alignment: .top) {
            if activePane { Rectangle().fill(palette.signal).frame(height: 2) }
        }
        .overlay(alignment: .bottom) { rule }
    }

    private func openFolderWindow(_ folder: URL) {
        if let hostOpenWindow { hostOpenWindow(folder) }
        else { openWindow(id: "richfiles-folder", value: FolderWindowRequest(folder: folder)) }
    }

    private var commandBar: some View {
        ViewThatFits(in: .horizontal) {
            commandBar(labeled: true)
            commandBar(labeled: false)
        }
        .padding(.horizontal, 10).padding(.bottom, 6)
    }

    private func commandBar(labeled: Bool) -> some View {
        HStack(spacing: 4) {
            HStack(spacing: 1) {
                Menu { creationMenu } label: { barLabel("New", "plus", labeled) }
                    .accessibilityLabel("New")
                    .disabled(actions.busy || !writableFolder)
                barButton("Cut", "scissors", showTitle: labeled) { _ = perform(.cut) }
                    .disabled(model.selected.isEmpty || actions.busy)
                barButton("Copy", "doc.on.doc", showTitle: labeled) { _ = perform(.copy) }
                    .disabled(model.selected.isEmpty)
                Menu {
                    Button("Paste, Skip Conflicts") { paste() }
                    Button("Paste, Keep Both") { paste(.keepBoth) }
                } label: { barLabel("Paste", "clipboard", labeled) }
                    .accessibilityLabel("Paste")
                    .disabled(!actions.canScheduleTransfer || !writableFolder)
                barButton("Rename", "pencil", showTitle: labeled) { _ = perform(.rename) }
                    .disabled(model.selected.count != 1 || actions.busy)
                barButton("Move to Trash", "trash", showTitle: labeled, visibleTitle: "Trash") { _ = perform(.trash) }
                    .disabled(model.selected.isEmpty || actions.busy)
                Menu { folderMenu } label: { barLabel("This Folder", "folder", labeled) }
                    .accessibilityLabel("Current folder actions")
                Menu {
                    Button("New Window") { openFolderWindow(model.folder) }
                    Button("Workspaces…") { sheet = .workspaces }
                    Divider()
                    Button("Compare Folders…") { sheet = .compare(model.folder) }
                    Button("Find Duplicate Files…") { sheet = .duplicates(model.folder) }
                    Button("Disk Usage…") { sheet = .diskUsage(model.folder) }
                    Button("Connect to Server…") { sheet = .connect }
                } label: { barLabel("Tools", "ellipsis", labeled) }
                    .accessibilityLabel("Tools")
            }
            .menuStyle(.borderlessButton)
            .fixedSize(horizontal: true, vertical: false)
            Spacer(minLength: 4)
            HStack(spacing: 2) {
                barButton("Sidebar", "sidebar.left", selected: sidebarVisible) { sidebarVisible.toggle() }
                barButton(workspace.isSplit ? "Close Split View" : "Split View", "rectangle.split.2x1",
                          selected: workspace.isSplit) { workspace.toggleSplit() }
                if !workspace.isSplit {
                    barButton("Preview Pane", "sidebar.right", selected: inspectorVisible) { inspectorVisible.toggle() }
                }
                if labeled {
                    viewButton(.list, "list.bullet", labeled: true)
                    viewButton(.details, "line.3.horizontal", labeled: true)
                    viewButton(.icons, "square.grid.2x2", labeled: true)
                }
                Menu {
                    ForEach([BrowserViewMode.list, .details, .icons, .columns, .gallery]) { mode in
                        Button {
                            model.viewMode = mode
                        } label: {
                            if model.viewMode == mode { Label(mode.title, systemImage: "checkmark") }
                            else { Text(mode.title) }
                        }
                    }
                } label: { Image(systemName: "square.grid.2x2") }
                    .accessibilityLabel("File view")
                if labeled || !workspace.isSplit {
                    barButton("File Shelf", "tray") { _ = perform(.fileShelf) }
                }
                sortMenu
            }
            .menuStyle(.borderlessButton)
            .fixedSize(horizontal: true, vertical: false)
        }
    }

    private func viewButton(_ mode: BrowserViewMode, _ symbol: String, labeled: Bool) -> some View {
        barButton(mode.title, symbol, showTitle: labeled, selected: model.viewMode == mode) { model.viewMode = mode }
            .accessibilityAddTraits(model.viewMode == mode ? .isSelected : [])
    }

    private var sortMenu: some View {
        Menu {
            sortButton("Name", .name)
            sortButton("Extension", .fileExtension)
            sortButton("Type", .type)
            sortButton("Size", .size)
            sortButton("Modified", .modified)
            Divider()
            Toggle("Ascending", isOn: Binding(get: { model.sort.ascending }, set: { ascending in
                model.sort = FileSort(column: model.sort.column, ascending: ascending, foldersFirst: model.sort.foldersFirst)
            }))
            Toggle("Folders First", isOn: Binding(get: { model.sort.foldersFirst }, set: { foldersFirst in
                model.sort = FileSort(column: model.sort.column, ascending: model.sort.ascending, foldersFirst: foldersFirst)
            }))
            Toggle("Show Hidden Files", isOn: $model.showHidden)
            Toggle("Remember View for This Folder", isOn: Binding(get: { model.remembersFolderView }, set: {
                model.setFolderViewOverride($0)
            }))
            if model.viewMode == .icons {
                Button("Larger Thumbnails") { model.iconSize = min(model.iconSize + 12, 180) }
                Button("Smaller Thumbnails") { model.iconSize = max(model.iconSize - 12, 48) }
            }
        } label: { Image(systemName: "arrow.up.arrow.down") }
            .accessibilityLabel("Sort and view options")
    }

    private func barLabel(_ title: String, _ symbol: String, _ showTitle: Bool) -> some View {
        HStack(spacing: 5) {
            Image(systemName: symbol).foregroundStyle(palette.muted)
            if showTitle { Text(title).lineLimit(1) }
        }
    }

    private func barButton(_ title: String, _ symbol: String, showTitle: Bool = false,
                           selected: Bool = false, bordered: Bool = false, visibleTitle: String? = nil,
                           action: @escaping () -> Void) -> some View {
        Button(action: action) { barLabel(visibleTitle ?? title, symbol, showTitle) }
            .buttonStyle(BrowserButtonStyle(selected: selected, bordered: bordered))
            .accessibilityLabel(title).help(title)
    }

    private func sortButton(_ title: String, _ column: FileSort.Column) -> some View {
        Button(title) {
            model.sort = FileSort(column: column, ascending: model.sort.ascending, foldersFirst: model.sort.foldersFirst)
        }
    }

    private var itemPane: some View {
        ZStack(alignment: .top) {
            Group {
                if model.isDisconnected {
                    emptyItems
                } else if model.viewMode == .columns {
                    columnBrowser
                } else if model.entries.isEmpty && !model.busy {
                    emptyItems
                } else {
                    switch model.viewMode {
                    case .details: details
                    case .list: compactList
                    case .icons: thumbnailGrid
                    case .gallery: gallery
                    case .columns: EmptyView()
                    }
                }
            }
            if model.busy {
                ProgressView().controlSize(.mini).padding(4)
                    .frame(maxWidth: .infinity, alignment: .trailing)
                    .accessibilityLabel("Reading files")
            }
            if let dropOperation {
                Text(dropOperation == .move ? "Move here" : "Copy here")
                    .padding(7).background(.regularMaterial, in: RoundedRectangle(cornerRadius: 6))
                    .padding(8).allowsHitTesting(false)
            }
            if let selectionRectangle {
                Rectangle().fill(palette.signal.opacity(0.12))
                    .overlay(Rectangle().stroke(palette.signal))
                    .frame(width: selectionRectangle.width, height: selectionRectangle.height)
                    .position(x: selectionRectangle.midX, y: selectionRectangle.midY)
                    .allowsHitTesting(false)
            }
        }
        .background(palette.pane)
        .coordinateSpace(name: "richfiles-items")
        .background(BrowserDragSurface(prepare: { point in
            guard let url = rowAt(point),
                  let entry = model.entries.first(where: { $0.url == url }) else { return nil }
            let entries = model.selected.contains(entry.url) ? model.selectedEntries : [entry]
            return { beginDrag(entry, entries: entries) }
        }, controlClick: { point, windowPoint, window in
            guard windowsKeys else { return false }
            let nativeRow = model.viewMode == .details ? detailsViewport.item(at: windowPoint, in: window) : nil
            guard let url = nativeRow ?? point.flatMap(rowAt),
                  let entry = model.entries.first(where: { $0.url == url }) else { return false }
            workspace.select(side)
            select(entry, modifiers: .control)
            if model.viewMode == .details { detailsViewport.focusTable() }
            else { focus = .items }
            return true
        }))
        .onPreferenceChange(RowFramesKey.self) { rowFrames = $0 }
        .onPreferenceChange(FileColumnFrameKey.self) { fileColumnFrame = $0 }
        .simultaneousGesture(DragGesture(minimumDistance: 5, coordinateSpace: .named("richfiles-items"))
            .onChanged { change in
                guard canSelectFromBlankSpace(at: change.startLocation) else { return }
                let rect = CGRect(
                    x: min(change.startLocation.x, change.location.x),
                    y: min(change.startLocation.y, change.location.y),
                    width: abs(change.startLocation.x - change.location.x),
                    height: abs(change.startLocation.y - change.location.y)
                )
                selectionRectangle = rect
                model.selected = Set(rowFrames.compactMap { item in
                    let frame = item.value
                    let intersects = model.viewMode == .details
                        ? frame.minY < rect.maxY && rect.minY < frame.maxY : rect.intersects(frame)
                    return intersects ? item.key : nil
                })
            }
            .onEnded { _ in
                if selectionRectangle != nil && model.viewMode != .details { focus = .items }
                selectionRectangle = nil
            })
        .onDrop(of: [UTType.fileURL.identifier, BrowserDragSelection.type], delegate: BrowserDropTarget(
            proposal: proposeDrop, exited: { dropOperation = nil }, receive: receiveDrop
        ))
        .contextMenu { folderMenu }
        .focusable(model.viewMode != .details)
        .focused($focus, equals: .items)
    }

    private func rowAt(_ point: CGPoint) -> URL? {
        rowFrames.first { row in
            let frame = row.value
            if model.viewMode == .details { return frame.minY <= point.y && point.y < frame.maxY }
            return frame.contains(point)
        }?.key
    }

    private func canSelectFromBlankSpace(at point: CGPoint) -> Bool {
        guard rowAt(point) == nil else { return false }
        switch model.viewMode {
        case .details:
            return rowFrames.values.allSatisfy { $0.maxY <= point.y }
        case .columns:
            return fileColumnFrame.contains(point)
        default:
            return true
        }
    }

    private var emptyItems: some View {
        VStack(spacing: 8) {
            Image(systemName: model.searchActive ? "magnifyingglass" : "folder").font(.system(size: 32))
            Text(model.isDisconnected ? "Volume disconnected" : (model.searchActive ? "No matching files" : "No items to show"))
            if !model.status.isEmpty { Text(model.status).font(textSize.font(size: 12)).multilineTextAlignment(.center) }
        }.foregroundStyle(.secondary).padding(24)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var columnBrowser: some View {
        GeometryReader { geometry in
            let path = model.breadcrumbs
            let currentWidth = max(240, min(350, geometry.size.width - (path.count > 1 ? 186 : 0)))
            ScrollViewReader { columns in
                ScrollView(.horizontal) {
                    LazyHStack(spacing: 0) {
                        ForEach(Array(zip(path.dropLast(), path.dropFirst())), id: \.0) { folder, child in
                            AncestorColumn(folder: folder, child: child, showHidden: model.showHidden,
                                           revision: actions.mutationRevision, open: model.openFolder)
                                .focusedValue(\.browserControl, true)
                                .frame(width: 185).id(folder)
                            Divider()
                        }
                        VStack(alignment: .leading, spacing: 0) {
                            Text(model.folder.lastPathComponent).font(textSize.font(size: 13).weight(.semibold)).lineLimit(1).padding(8)
                            Divider()
                            if model.entries.isEmpty && !model.busy {
                                emptyItems
                            } else {
                                ScrollViewReader { items in
                                    GeometryReader { viewport in
                                        ScrollView {
                                            LazyVStack(alignment: .leading, spacing: 1) {
                                                ForEach(model.entries) { entry in
                                                    selectable(entry) {
                                                        VStack(alignment: .leading, spacing: 1) {
                                                            HStack {
                                                                nameLabel(entry)
                                                                Spacer(minLength: 4)
                                                                if entry.kind == .directory {
                                                                    Image(systemName: "chevron.right").font(.caption)
                                                                        .foregroundStyle(.secondary)
                                                                }
                                                            }
                                                            if model.searchActive { resultLocation(entry) }
                                                        }.frame(minHeight: 23)
                                                    }
                                                }
                                            }.padding(4)
                                        }
                                        .onPreferenceChange(RowFramesKey.self, perform: viewportObserver(viewport, mode: .columns))
                                    }.onAppear { restoreScroll(items, anchor: .top) }
                                        .onChange(of: selectionCursor) { if let url = $0 { items.scrollTo(url) } }
                                        .onChange(of: model.busy) { if !$0 { restoreScroll(items, anchor: .top) } }
                                }
                            }
                        }
                        .frame(width: currentWidth)
                        .frame(maxHeight: .infinity, alignment: .top)
                        .background(GeometryReader { geometry in
                            Color.clear.preference(key: FileColumnFrameKey.self,
                                                   value: geometry.frame(in: .named("richfiles-items")))
                        })
                        .id(model.folder)
                        Spacer(minLength: 0)
                    }.frame(minWidth: geometry.size.width, maxHeight: .infinity)
                }
                .onAppear { columns.scrollTo(model.folder, anchor: .trailing) }
                .onChange(of: model.folder) { columns.scrollTo($0, anchor: .trailing) }
            }
        }.accessibilityLabel("Column view").accessibilityIdentifier("file-columns")
    }

    private var gallery: some View {
        VStack(spacing: 0) {
            Group {
                if model.selected.count == 1, let entry = model.selectedEntries.first {
                    VStack(spacing: 6) {
                        NativePreview(entry: entry).frame(maxWidth: .infinity, maxHeight: .infinity)
                        Text(entry.name).lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                        if model.searchActive { resultLocation(entry).textSelection(.enabled) }
                    }.padding(10)
                } else {
                    Text("Select one item to preview it.").foregroundStyle(.secondary)
                }
            }.frame(maxWidth: .infinity, maxHeight: .infinity)
                .accessibilityLabel("Gallery preview")
            Divider()
            ScrollViewReader { scroll in
                GeometryReader { viewport in
                    ScrollView(.horizontal) {
                        LazyHStack(alignment: .top, spacing: 4) {
                            ForEach(model.entries) { entry in
                                selectable(entry) {
                                    VStack(spacing: 4) {
                                        FileThumbnail(entry: entry, side: 62)
                                        Text(entry.name)
                                            .font(textSize.font(size: 11))
                                            .lineLimit(1).truncationMode(.middle)
                                        if model.searchActive { resultLocation(entry) }
                                    }.frame(width: 106).padding(3)
                                }
                            }
                        }.padding(6)
                    }
                    .onPreferenceChange(RowFramesKey.self, perform: viewportObserver(viewport, mode: .gallery))
                }.frame(height: (model.searchActive ? 126 : 109) + textSize.adjustment * 2)
                    .accessibilityLabel("Gallery thumbnails")
                    .onAppear { restoreScroll(scroll, anchor: .leading) }
                    .onChange(of: selectionCursor) { if let url = $0 { scroll.scrollTo(url) } }
                    .onChange(of: model.busy) { if !$0 { restoreScroll(scroll, anchor: .leading) } }
            }
        }.accessibilityIdentifier("file-gallery")
    }

    private var details: some View {
        Group {
            if #available(macOS 14.0, *) {
                Table(model.entries, selection: $model.selected, columnCustomization: detailsCustomization) {
                    detailsName.customizationID("name").disabledCustomizationBehavior(.visibility)
                    detailsModified.customizationID("modified")
                    detailsKind.customizationID("kind")
                    detailsSize.customizationID("size")
                    detailsFolder.customizationID("folder")
                }
            } else {
                Table(model.entries, selection: $model.selected) {
                    detailsName
                    detailsModified
                    detailsKind
                    detailsSize
                    detailsFolder
                }
            }
        }
        .accessibilityIdentifier("file-details")
        .onAppear { detailsViewport.show(model.scrollAnchor, alignToTop: true) }
        .onDisappear { detailsViewport.detach() }
        .onChange(of: model.busy) { if !$0 { detailsViewport.show(model.scrollAnchor, alignToTop: true) } }
        .onChange(of: selectionCursor) { if let item = $0 { detailsViewport.show(item, alignToTop: false) } }
        .contextMenu(forSelectionType: URL.self) { ids in
            selectionMenu(model.entries.filter { ids.contains($0.url) })
        } primaryAction: { ids in
            activate(model.entries.filter { ids.contains($0.url) })
        }
    }

    @available(macOS 14.0, *)
    private var detailsCustomization: Binding<TableColumnCustomization<FileEntry>> {
        Binding(get: {
            guard let data = model.detailsLayout, data.count <= 64 * 1024,
                  let layout = try? JSONDecoder().decode(TableColumnCustomization<FileEntry>.self, from: data) else {
                return TableColumnCustomization()
            }
            return layout
        }, set: { layout in
            if let data = try? JSONEncoder().encode(layout), data.count <= 64 * 1024 {
                model.detailsLayout = data
            }
        })
    }

    private var detailsName: some TableColumnContent<FileEntry, Never> {
        TableColumn("Name") { (entry: FileEntry) in
            nameLabel(entry)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(dragFrame(for: entry.url))
                .background(DetailsCellMarker(viewport: detailsViewport, model: model))
        }.width(min: 150, ideal: 260)
    }

    private var detailsSize: some TableColumnContent<FileEntry, Never> {
        TableColumn("Size") { (entry: FileEntry) in Text(entry.size.map(formattedBytes) ?? "–") }
            .width(min: 70, ideal: 82)
    }

    private var detailsKind: some TableColumnContent<FileEntry, Never> {
        TableColumn("Kind") { (entry: FileEntry) in Text(entry.typeDescription ?? "–").lineLimit(1) }
            .width(min: 80, ideal: 110)
    }

    private var detailsModified: some TableColumnContent<FileEntry, Never> {
        TableColumn("Modified") { (entry: FileEntry) in Text(entry.modified.map(formattedDate) ?? "–") }
            .width(min: 100, ideal: 130)
    }

    private var detailsFolder: some TableColumnContent<FileEntry, Never> {
        TableColumn("Folder") { (entry: FileEntry) in
            Text(model.searchActive ? entry.url.deletingLastPathComponent().path : "")
                .lineLimit(1).truncationMode(.middle)
        }.width(min: 70, ideal: 125)
    }

    private var compactList: some View {
        GeometryReader { geometry in
            let rowHeight = (model.searchActive ? 48 : 28) * textSize.scale
            let rows = max(1, Int((geometry.size.height - 12) / rowHeight))
            let width = max(1, geometry.size.width - 12)
            let visibleColumns = max(1, Int(width / (325 * textSize.scale)))
            let columnWidth = width / CGFloat(visibleColumns)
            let entries = model.entries
            let columns = (entries.count + rows - 1) / rows
            ScrollViewReader { scroll in
                ScrollView(.horizontal) {
                    LazyHStack(alignment: .top, spacing: 0) {
                        ForEach(0..<columns, id: \.self) { column in
                            let start = column * rows
                            let end = min(start + rows, entries.count)
                            VStack(spacing: 0) {
                                ForEach(entries[start..<end]) { entry in
                                    selectable(entry) {
                                        VStack(alignment: .leading, spacing: 1) {
                                            nameLabel(entry)
                                            if model.searchActive {
                                                resultLocation(entry, font: BrowserTypography.reading(14 * textSize.scale))
                                            }
                                        }.frame(maxWidth: .infinity, alignment: .leading)
                                    }.frame(height: rowHeight)
                                }
                            }
                            .padding(.horizontal, 5)
                            .frame(width: columnWidth, height: max(0, geometry.size.height - 12), alignment: .top)
                            .overlay(alignment: .trailing) {
                                if column + 1 < columns {
                                    Rectangle().fill(palette.line).frame(width: 1).allowsHitTesting(false)
                                }
                            }
                            .id(column)
                        }
                    }.padding(6)
                }
                .onAppear {
                    listRows = rows
                    scrollList(to: model.scrollAnchor, rows: rows, using: scroll, anchor: .leading)
                }
                .onChange(of: rows) {
                    listRows = $0
                    scrollList(to: model.scrollAnchor, rows: $0, using: scroll, anchor: .leading)
                }
                .onChange(of: selectionCursor) { scrollList(to: $0, rows: rows, using: scroll) }
                .onChange(of: model.busy) { busy in
                    if !busy { scrollList(to: model.scrollAnchor, rows: rows, using: scroll, anchor: .leading) }
                }
                .onPreferenceChange(RowFramesKey.self, perform: viewportObserver(geometry, mode: .list))
            }
        }
        .accessibilityIdentifier("file-list")
        .accessibilityLabel("List view, filenames and extensions")
    }

    private func scrollList(to url: URL?, rows: Int, using scroll: ScrollViewProxy, anchor: UnitPoint? = nil) {
        guard let url, let index = model.entries.firstIndex(where: { $0.url == url }) else { return }
        scroll.scrollTo(index / rows, anchor: anchor)
    }

    private var thumbnailGrid: some View {
        GeometryReader { geometry in
            let width = max(model.iconSize + 22, 90)
            let columns = max(1, Int((geometry.size.width - 14) / (width + 8)))
            ScrollViewReader { scroll in
                ScrollView {
                    LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: columns), spacing: 6) {
                        ForEach(model.entries) { entry in
                            selectable(entry) {
                                VStack(spacing: 3) {
                                    FileThumbnail(entry: entry, side: model.iconSize)
                                    Text(entry.name).lineLimit(2).truncationMode(.middle).multilineTextAlignment(.center)
                                    if model.searchActive { resultLocation(entry) }
                                }.frame(maxWidth: .infinity).padding(4)
                            }
                        }
                    }.padding(7)
                }
                .onAppear { iconColumns = columns; restoreScroll(scroll, anchor: .top) }
                .onChange(of: columns) { iconColumns = $0 }
                .onChange(of: selectionCursor) { value in if let value { scroll.scrollTo(value) } }
                .onChange(of: model.busy) { busy in if !busy { restoreScroll(scroll, anchor: .top) } }
                .onPreferenceChange(RowFramesKey.self, perform: viewportObserver(geometry, mode: .icons))
            }
        }
        .accessibilityIdentifier("file-thumbnails")
    }

    private func selectable<Content: View>(_ entry: FileEntry, @ViewBuilder content: () -> Content) -> some View {
        let selected = model.selected.contains(entry.url)
        let cut = actions.cutURLs.contains { $0.path == entry.url.path }
        let cursor = focus == .items && selectionCursor == entry.url
        let location = model.searchActive ? entry.url.deletingLastPathComponent().path : ""
        let outline = RoundedRectangle(cornerRadius: 5)
        let marker = activePane ? palette.signal : palette.muted
        return content()
            .padding(.horizontal, 8)
            .background(selected ? (activePane ? palette.selected : palette.inactiveSelection) : .clear, in: outline)
            .overlay {
                if selected || cursor || cut {
                    outline.strokeBorder(marker, style: StrokeStyle(lineWidth: cursor ? 2 : 1, dash: cut ? [4, 3] : []))
                        .allowsHitTesting(false)
                }
            }
            .overlay(alignment: .leading) {
                if selected {
                    Capsule().fill(marker).frame(width: 3).padding(.vertical, 2).allowsHitTesting(false)
                }
            }
            .background(dragFrame(for: entry.url))
            .contentShape(Rectangle())
            .gesture(TapGesture(count: 2).onEnded {
                focus = .items
                select(entry, modifiers: [])
                activate([entry])
            }.exclusively(before: TapGesture().onEnded {
                focus = .items
                select(entry, modifiers: NSEvent.modifierFlags)
            }))
            .contextMenu {
                selectionMenu(selected ? model.selectedEntries : [entry])
            }
            .help(model.searchActive ? entry.url.path : entry.name)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(entry.name)
            .accessibilityValue(location)
            .accessibilityAddTraits(selected ? [.isSelected, .isButton] : .isButton)
            .accessibilityAction { select(entry, modifiers: []) }
            .accessibilityAction(named: "Open") { activate([entry]) }
            .id(entry.url)
    }

    private func resultLocation(_ entry: FileEntry, font: Font = .caption) -> some View {
        let folder = entry.url.deletingLastPathComponent().path
        return Text(folder).font(font).lineLimit(1).truncationMode(.middle)
            .foregroundStyle(.secondary).help(folder)
    }

    private func dragFrame(for url: URL) -> some View {
        GeometryReader { geometry in
            Color.clear.preference(key: RowFramesKey.self,
                                   value: [url: geometry.frame(in: .named("richfiles-items"))])
        }
    }

    private func nameLabel(_ entry: FileEntry) -> some View {
        let cut = actions.cutURLs.contains { $0.path == entry.url.path }
        let font = BrowserTypography.reading(16 * textSize.scale)
        return HStack(spacing: 8) {
            Image(systemName: fileSymbol(entry))
                .foregroundStyle(entry.kind == .directory ? palette.folder : palette.muted)
                .frame(width: 18)
            Text(entry.name).lineLimit(1).truncationMode(.middle)
                .font(cut ? font.italic() : font)
            if cut { Image(systemName: "scissors").foregroundStyle(palette.muted).accessibilityLabel("Cut") }
        }
        .opacity(cut ? 0.65 : 1)
        .help(entry.name)
        .accessibilityLabel(entry.name)
    }

    @ViewBuilder
    private func selectionMenu(_ entries: [FileEntry]) -> some View {
        if let first = entries.first {
            Button("Open") { activate(entries) }
            if entries.count == 1 {
                if first.kind == .directory {
                    Button("Open in New Tab") { model.newTab(first.url) }
                    Button("Open in New Window") { openFolderWindow(first.url) }
                    Button("Pin to Quick Access") { model.addFavorite(first.url) }
                } else {
                    Button("Open With…") { chooseApplication(first) }
                    Button("Quick Look") { sheet = .preview(first) }
                }
                Button("Rename…") { requestRename(first) }.disabled(actions.busy)
                Button("Properties") { sheet = .properties(first) }
                if first.kind == .regularFile {
                    Button("Verify Checksum…") { checksum(first) }
                }
            } else {
                Button("Batch Rename…") { withReferences(entries) { sheet = .batch($0) } }.disabled(actions.busy)
            }
            Divider()
            Button("Cut") { withReferences(entries) { actions.copy($0, cut: true) } }
            Button("Copy") { withReferences(entries) { actions.copy($0) } }
            Button("Paste") { paste() }
                .disabled(!actions.canScheduleTransfer || !writableFolder)
            Button("Create ZIP…") {
                guard let destination = currentDropDestination else {
                    feedback = "The destination folder is unavailable. Refresh this folder and try again."
                    return
                }
                withReferences(entries) { sheet = .createZip($0, destination) }
            }.disabled(actions.busy || !writableFolder)
            Button("Copy Path") { copyPaths(entries.map(\.url)) }
            Button("Add to File Shelf") { withReferences(entries) { actions.addToShelf($0) } }
            ShareLink(items: entries.map(\.url)) { Label("Share…", systemImage: "square.and.arrow.up") }
            if entries.count == 1, first.kind == .regularFile,
               UTType(filenameExtension: first.url.pathExtension)?.conforms(to: .image) == true {
                Menu("Quick Actions") {
                    ForEach(ImageQuickAction.allCases) { action in
                        Button(action.rawValue) {
                            let destination = model.folder
                            withReferences([first]) { files in
                                if let source = files.first { actions.createImageCopy(action, of: source, in: destination) }
                            }
                        }
                    }
                }.disabled(actions.busy || !writableFolder)
            }
            if model.searchActive {
                Button("Open Containing Folder") { model.revealFile(first.url) }
            }
            if canSendToRichCleaner(entries) {
                Button(RichOSServices.cleanerTitle) { addSpokenNote(entries) }.disabled(actions.busy)
            }
            Button("Move to Trash", role: .destructive) {
                withReferences(entries) { actions.trash($0) }
            }.disabled(actions.busy)
        } else {
            folderMenu
        }
    }

    private func canSendToRichCleaner(_ entries: [FileEntry]) -> Bool {
        guard let desktop = FileManager.default.urls(for: .desktopDirectory, in: .userDomainMask).first?.path else {
            return false
        }
        return !entries.isEmpty && entries.allSatisfy {
            $0.kind == .regularFile && $0.url.deletingLastPathComponent().standardizedFileURL.path == desktop
        }
    }

    private func addSpokenNote(_ entries: [FileEntry]) {
        withReferences(entries) { references in
            Task {
                do {
                    if try await RichOSServices.addSpokenNote(to: references) {
                        feedback = "Selection sent to RichCleaner."
                    } else {
                        feedback = "RichCleaner could not start. Check RichCleaner and RichScribe in RichOS Settings."
                    }
                } catch { feedback = error.localizedDescription }
            }
        }
    }

    @ViewBuilder
    private var creationMenu: some View {
        Group {
            Button("New Folder") { newItem(directory: true, extension: nil) }
            Button("New Text File") { newItem(directory: false, extension: "txt") }
            Button("New Markdown File") { newItem(directory: false, extension: "md") }
            Button("New File from Template…") { chooseTemplate() }
        }.disabled(actions.busy || !writableFolder)
    }

    @ViewBuilder
    private var folderMenu: some View {
        creationMenu
        Divider()
        Button("Paste") { paste() }
                .disabled(!actions.canScheduleTransfer || !writableFolder)
        Button("Copy Current Path") { copyPaths([model.folder]) }
        Button("Expand to Current Folder") { expandToCurrentFolder() }
        Button("Workspaces…") { sheet = .workspaces }
        Button("File Shelf…") { _ = perform(.fileShelf) }
        Button("Disk Usage…") { sheet = .diskUsage(model.folder) }
        Button("Find Duplicate Files…") { sheet = .duplicates(model.folder) }
        Button("Compare Folders…") { sheet = .compare(model.folder) }
        if hostOpenWindow != nil {
            Button("RichFiles Settings…") { sheet = .settings }
        }
        Button("Refresh") { _ = perform(.refresh) }
    }

    private var inspector: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Preview").font(textSize.font(size: 13).weight(.semibold))
            if model.selectedEntries.count == 1, let entry = model.selectedEntries.first {
                if entry.kind == .directory {
                    FileThumbnail(entry: entry, side: 110)
                } else {
                    NativePreview(entry: entry).frame(minHeight: 150, maxHeight: .infinity)
                }
                Text(entry.name).font(textSize.font(size: 13).weight(.semibold)).textSelection(.enabled)
                Text(entry.typeDescription ?? "File").foregroundStyle(.secondary)
                if let size = entry.size { Text(formattedBytes(size)).foregroundStyle(.secondary) }
                Button("Properties") { sheet = .properties(entry) }
            } else {
                Text("Select one item to preview it.").foregroundStyle(.secondary)
                Spacer()
            }
        }.padding(10).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private var statusBar: some View {
        HStack(spacing: 10) {
            Text(model.searchActive ? "\(model.entries.count) results" : "\(model.entries.count) items")
            if !model.selected.isEmpty {
                Text("\(model.selected.count) selected")
                let sizes = model.selectedEntries.compactMap(\.size)
                if !sizes.isEmpty { Text(formattedBytes(sizes.reduce(0, safeAdd))) }
            }
            if model.currentVolume?.isReadOnly == true { Text("Read-only") }
            Spacer(minLength: 4)
            if let undo = actions.undoName {
                Button("Undo " + undo) { actions.undo() }
                    .buttonStyle(.borderless).disabled(actions.busy).lineLimit(1)
            }
            if let transfer = actions.transfer {
                let queued = actions.queuedTransfers.count
                let title = transfer.running ? "Transfers \(transfer.completed)/\(transfer.total)" : "Transfers"
                Button(queued > 0 ? title + " · \(queued) queued" : title) {
                    transfersVisible.toggle()
                }.buttonStyle(.borderless)
            }
            if actions.interruptedTransfer != nil {
                Button("Interrupted Transfer…") { sheet = .interruptedTransfer }.buttonStyle(.borderless)
            }
            if !actions.untrashed.isEmpty {
                Button("Review Trash Failures…") { sheet = .trashFailures(actions.untrashed) }.buttonStyle(.borderless)
            }
            if !actions.operationIssues.isEmpty {
                Button("Review Operation Issues…") { sheet = .operationIssues(actions.operationIssues) }
                    .buttonStyle(.borderless)
            }
            if model.busy && model.searchActive { Button("Stop Search") { model.stopSearch() }.buttonStyle(.borderless) }
            let itemCount = "\(model.entries.count) \(model.searchActive ? "results" : "items")"
            if !model.status.isEmpty && model.status != itemCount {
                Text(model.status).lineLimit(1).help(model.status)
            }
            let status = !feedback.isEmpty ? feedback : actions.status
            if !status.isEmpty { Text(status).lineLimit(1).truncationMode(.tail).help(status) }
            if let volume = model.currentVolume, let available = volume.availableBytes {
                Text("\(formattedBytes(available)) free on \(volume.name)").lineLimit(1)
            }
            Label(windowsKeys ? "Windows keys" : "Mac keys", systemImage: "keyboard").fixedSize()
        }
        .font(BrowserTypography.reading(14 * textSize.scale)).foregroundStyle(palette.muted)
        .padding(.horizontal, 12)
        .frame(minHeight: 33 * textSize.scale)
        .background(palette.chrome)
    }

    private func perform(_ command: BrowserCommand) -> Bool {
        feedback = ""
        let entries = model.selectedEntries
        switch command {
        case .open:
            guard !entries.isEmpty else { return false }
            activate(entries)
        case .rename:
            guard !actions.busy, let entry = entries.first else { return false }
            if entries.count == 1 { requestRename(entry) }
            else { withReferences(entries) { sheet = .batch($0) } }
        case .trash:
            guard !entries.isEmpty, !actions.busy else { return false }
            withReferences(entries) { actions.trash($0) }
        case .preview:
            guard entries.count == 1, let entry = entries.first else { return false }
            sheet = .preview(entry)
        case .properties:
            guard let entry = entries.first else { return false }
            sheet = .properties(entry)
        case .copy, .cut:
            guard !entries.isEmpty else { return false }
            withReferences(entries) { actions.copy($0, cut: command == .cut) }
        case .copyPath: copyPaths(entries.isEmpty ? [model.folder] : entries.map(\.url))
        case .paste: paste()
        case .undo: actions.undo()
        case .redo: actions.redo()
        case .newFolder: newItem(directory: true, extension: nil)
        case .newTab: model.newTab()
        case .closeTab:
            if model.tabs.count > 1 { model.closeTab(model.currentTabID) }
            else if hostOpenWindow == nil { NSApp.keyWindow?.performClose(nil) }
        case .selectAll: model.selectAll()
        case .back:
            guard model.canGoBack else { return false }
            model.goBack()
        case .forward:
            guard model.canGoForward else { return false }
            model.goForward()
        case .up: model.goUp()
        case .refresh:
            dropDestination = nil
            destinationRefresh = UUID()
            model.refreshVolumes()
            model.refresh()
        case .address: editAddress()
        case .revealFolder: expandToCurrentFolder()
        case .workspaces: sheet = .workspaces
        case .settings: sheet = .settings
        case .splitView: workspace.toggleSplit()
        case .switchPane:
            guard workspace.isSplit else { return false }
            workspace.select(side == .primary ? .secondary : .primary)
            focus = nil
            returnKeyboardFocus()
        case .addToShelf:
            guard !entries.isEmpty else { return false }
            withReferences(entries) { actions.addToShelf($0) }
        case .fileShelf:
            let folder = model.folder
            Task {
                do {
                    let destination = try await Task.detached { try BrowserTransferDestination(folder: folder) }.value
                    guard model.folder == folder, sheet == nil else { return }
                    sheet = .fileShelf(destination)
                } catch { feedback = error.localizedDescription }
            }
        case .diskUsage: sheet = .diskUsage(model.folder)
        case .duplicates: sheet = .duplicates(model.folder)
        case .compareFolders: sheet = .compare(model.folder)
        case .checksum:
            guard entries.count == 1, let file = entries.first, file.kind == .regularFile else { return false }
            checksum(file)
        case .actionPalette: sheet = .actionPalette
        case .search: focus = .search
        case .grid(let dx, let dy, let extend): return moveGrid(dx: dx, dy: dy, extend: extend)
        }
        return true
    }

    private func select(_ entry: FileEntry, modifiers: NSEvent.ModifierFlags) {
        if modifiers.contains(.shift), let anchor = rangeAnchor,
           let start = model.entries.firstIndex(where: { $0.url == anchor }),
           let end = model.entries.firstIndex(where: { $0.url == entry.url }) {
            model.selected = Set(model.entries[min(start, end)...max(start, end)].map(\.url))
        } else if modifiers.contains(.command) || (windowsKeys && modifiers.contains(.control)) {
            if model.selected.contains(entry.url) { model.selected.remove(entry.url) }
            else { model.selected.insert(entry.url) }
            rangeAnchor = entry.url
        } else {
            model.selected = [entry.url]
            rangeAnchor = entry.url
        }
        selectionCursor = entry.url
        model.scrollAnchor = entry.url
    }

    private func moveGrid(dx: Int, dy: Int, extend: Bool) -> Bool {
        guard model.viewMode != .details, !model.entries.isEmpty else { return false }
        let current = selectionCursor.flatMap { url in model.entries.firstIndex(where: { $0.url == url }) }
        let step: Int
        switch model.viewMode {
        case .list: step = dx * listRows + dy
        case .icons: step = dx + dy * iconColumns
        case .gallery: step = dx + dy
        case .columns:
            if dx != 0, !extend {
                if dx < 0 { model.goUp() }
                else if let entry = model.selectedEntries.first, entry.kind == .directory { model.openFolder(entry.url) }
                return true
            }
            step = dy
        case .details: return false
        }
        let index = current.map { max(0, min(model.entries.count - 1, $0 + step)) } ?? 0
        select(model.entries[index], modifiers: extend ? .shift : [])
        return true
    }

    private func selectPrefix(_ character: String) -> Bool {
        let now = ProcessInfo.processInfo.systemUptime
        prefixText = now - prefixTime < 0.8 ? prefixText + character : character
        prefixTime = now
        if let entry = model.entries.first(where: {
            $0.name.range(of: prefixText, options: [.anchored, .caseInsensitive]) != nil
        }) {
            select(entry, modifiers: [])
            return true
        }
        return false
    }

    private func viewportObserver(_ geometry: GeometryProxy, mode: BrowserViewMode) -> ([URL: CGRect]) -> Void {
        let folder = model.folder
        let tab = model.currentTabID
        let horizontal = mode == .list || mode == .gallery
        return { frames in
            let viewport = geometry.frame(in: .named("richfiles-items"))
            let first = frames.lazy.filter { item in
                let intersection = item.value.intersection(viewport)
                return intersection.width > 0 && intersection.height > 0
            }.min { left, right in
                let lhs = left.value
                let rhs = right.value
                if horizontal {
                    return lhs.minX == rhs.minX ? lhs.minY < rhs.minY : lhs.minX < rhs.minX
                }
                // Wrapped filenames change card heights while the cards share a row center.
                let leftRow = mode == .icons ? lhs.midY : lhs.minY
                let rightRow = mode == .icons ? rhs.midY : rhs.minY
                return leftRow == rightRow ? lhs.minX < rhs.minX : leftRow < rightRow
            }
            if let first { model.recordViewport(first.key, in: folder, tab: tab, mode: mode) }
        }
    }

    private func restoreScroll(_ scroll: ScrollViewProxy, anchor: UnitPoint) {
        if let item = model.scrollAnchor { scroll.scrollTo(item, anchor: anchor) }
    }

    private func editAddress() {
        address = model.folder.path
        suggestions = []
        editingAddress = true
        focus = .address
    }

    private func submitAddress(_ text: String) {
        guard !text.isEmpty else { return }
        model.openAddress(text)
        suggestions = []
    }

    private func startSearch() {
        feedback = ""
        model.submitSearch()
        focus = nil
    }

    private func activate(_ entries: [FileEntry]) {
        guard let first = entries.first else { return }
        if entries.count == 1 && first.kind == .directory {
            model.openFolder(first.url)
            return
        }
        if entries.count == 1 && first.kind == .regularFile && ArchiveTools.supports(first.url) {
            Task {
                do { sheet = .archive(try await ArchiveTools.list(first)) }
                catch { feedback = error.localizedDescription }
            }
            return
        }
        Task {
            for entry in entries {
                if entry.kind == .directory { model.newTab(entry.url) }
                else if entry.kind == .symbolicLink,
                        let target = try? DirectoryReader.directoryURL(entry.url) {
                    model.openFolder(target)
                } else {
                    do { try await WorkspaceServices.open(entry.url) }
                    catch { feedback = error.localizedDescription }
                }
            }
        }
    }

    private func chooseApplication(_ entry: FileEntry) {
        Task {
            do {
                let apps = try await Task.detached { try WorkspaceServices.applications(for: entry.url) }.value
                sheet = .openWith(entry, apps)
            } catch { feedback = error.localizedDescription }
        }
    }

    private func requestRename(_ entry: FileEntry) {
        withReferences([entry]) { references in
            guard let source = references.first else { return }
            sheet = .name(NameRequest(source: source, folder: model.folder,
                                      directory: entry.kind == .directory, name: entry.name))
        }
    }

    private func checksum(_ entry: FileEntry) {
        withReferences([entry]) { references in
            if let reference = references.first { sheet = .checksum(reference) }
        }
    }

    private func newItem(directory: Bool, extension ext: String?) {
        guard !actions.busy, let destination = currentDropDestination else {
            feedback = "The destination folder is unavailable."
            return
        }
        let name = directory ? "New Folder" : "New File" + (ext.map { "." + $0 } ?? "")
        let folder = model.folder
        let tab = model.currentTabID
        guard model.currentVolume?.supportsExclusiveRename == true else {
            sheet = .name(NameRequest(source: nil, folder: destination.url, directory: directory, name: name,
                                      expectedFolder: destination.identity))
            return
        }
        actions.create(in: destination.url, named: name, directory: directory,
                       expectedFolder: destination.identity, useAvailableName: true) { item in
            guard model.folder == folder, model.currentTabID == tab, workspace.active == side, sheet == nil else { return }
            model.selectCreatedItem(item)
            rangeAnchor = item.url
            selectionCursor = item.url
            sheet = .name(NameRequest(source: item, folder: folder, directory: directory,
                                      name: item.url.lastPathComponent))
        }
    }

    private func chooseTemplate() {
        guard let destination = currentDropDestination else {
            feedback = "The destination folder is unavailable."
            return
        }
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.resolvesAliases = false
        panel.directoryURL = destination.url
        panel.prompt = "Use Template"
        panel.begin { result in
            guard result == .OK, let url = panel.url else { return }
            Task {
                do {
                    let template = try await Task.detached { try ReviewedFile(FileReference(url: url)) }.value
                    sheet = .name(NameRequest(source: nil, folder: destination.url, directory: false,
                                              name: "New " + url.lastPathComponent,
                                              template: .init(file: template, destination: destination)))
                } catch { feedback = error.localizedDescription }
            }
        }
    }

    private func captureSelection() async {
        let selected = model.selected
        selectedReferences = selectedReferences.filter { selected.contains($0.key) }
        let entries = model.selectedEntries.filter { cachedReference(for: $0) == nil }
        if entries.isEmpty { return }
        let captured = try? await Self.capture(entries)
        guard !Task.isCancelled, model.selected == selected else { return }
        if let captured {
            for (entry, reference) in zip(entries, captured) { selectedReferences[entry.url] = reference }
        }
    }

    private func withReferences(_ entries: [FileEntry], action: @escaping ([FileReference]) -> Void) {
        guard !entries.isEmpty else { return }
        let cached = entries.compactMap { cachedReference(for: $0) }
        if cached.count == entries.count { action(cached); return }
        guard !preparingAction else { return }
        preparingAction = true
        Task {
            defer { preparingAction = false }
            do { action(try await Self.capture(entries)) }
            catch { feedback = error.localizedDescription }
        }
    }

    private func cachedReference(for entry: FileEntry) -> FileReference? {
        guard let reference = selectedReferences[entry.url], reference.identity == entry.identity else { return nil }
        return reference
    }

    nonisolated private static func capture(_ entries: [FileEntry]) async throws -> [FileReference] {
        try entries.map { entry in
            try Task.checkCancellation()
            let reference = try entry.operationReference()
            let current = try FileEntry(url: URL(fileURLWithPath: entry.url.path))
            guard current.identity == entry.identity,
                  current.created == entry.created, current.modified == entry.modified,
                  current.size == entry.size, current.kind == entry.kind else {
                throw FileOperationError.sourceChanged(entry.url)
            }
            return reference
        }
    }

    private func copyPaths(_ urls: [URL]) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(urls.map(\.path).joined(separator: "\n"), forType: .string)
        actions.refreshClipboard()
        feedback = urls.count == 1 ? "Path copied." : "Paths copied."
    }

    private func beginDrag(_ entry: FileEntry, entries: [FileEntry]) -> BrowserDragSurface.Payload? {
        let sources: [FileReference]
        if entries.count == 1, let source = try? entry.operationReference() {
            sources = [source]
        } else if entries.count > 1 {
            sources = entries.compactMap { cachedReference(for: $0) }
        } else {
            sources = []
        }
        guard sources.count == entries.count, let primary = sources.first(where: { $0.url.path == entry.url.path }) else {
            feedback = "The file selection changed. Refresh this folder and select the items again."
            return nil
        }
        let token = actions.beginDrag(primary: primary, sources: sources)
        let urls = [primary.url] + sources.filter { $0 != primary }.map(\.url)
        return BrowserDragSurface.Payload(token: token, urls: urls)
    }

    private func captureDropDestination() async {
        dropDestination = nil
        guard !model.isDisconnected else { return }
        let folder = model.folder
        let worker = Task.detached(priority: .utility) { try BrowserTransferDestination(folder: folder) }
        let result = await withTaskCancellationHandler { try? await worker.value } onCancel: { worker.cancel() }
        if !Task.isCancelled, model.folder == folder { dropDestination = result }
    }

    private var writableFolder: Bool {
        !model.isDisconnected && model.currentVolume?.isReadOnly != true
    }

    private func paste(_ conflict: BrowserConflictPolicy = .skip) {
        guard writableFolder, actions.canScheduleTransfer else { return }
        guard let destination = currentDropDestination else {
            feedback = "The destination folder is unavailable. Refresh this folder and try again."
            return
        }
        actions.paste(into: destination.url, conflict: conflict, destination: destination)
    }

    private var currentDropDestination: BrowserTransferDestination? {
        guard actions.canScheduleTransfer, writableFolder,
              let destination = dropDestination, destination.url.path == model.folder.path else { return nil }
        return destination
    }

    private func proposeDrop(_ info: DropInfo) -> DropProposal {
        guard let destination = currentDropDestination else {
            dropOperation = nil
            return DropProposal(operation: .forbidden)
        }
        let operation: DropOperation
        if let drag = actions.currentDrag, BrowserDragSurface.pasteboardToken == drag.token {
            operation = drag.moves(to: destination, option: NSEvent.modifierFlags.contains(.option)) ? .move : .copy
        } else {
            operation = .copy
        }
        dropOperation = operation
        return DropProposal(operation: operation)
    }

    private func receiveDrop(_ providers: [NSItemProvider]) -> Bool {
        let proposedOperation = dropOperation
        dropOperation = nil
        let fileProviders = providers.filter { $0.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) }
        guard let destination = currentDropDestination, !fileProviders.isEmpty else { return false }
        let option = NSEvent.modifierFlags.contains(.option)
        let local = BrowserDragSurface.pasteboardToken
        Task {
            do {
                var urls: [URL] = []
                for provider in fileProviders {
                    guard let url = await Self.fileURL(from: provider) else {
                        feedback = "Could not read all files in this drop. Select them again."
                        return
                    }
                    urls.append(url)
                }
                let provider = providers.first { $0.hasItemConformingToTypeIdentifier(BrowserDragSelection.type) }
                let token: UUID?
                if let local { token = local }
                else if let provider { token = await Self.dragToken(from: provider) }
                else { token = nil }
                if let token {
                    guard let primary = actions.currentDrag?.primary.url,
                          urls.contains(where: { $0.path == primary.path }),
                          let drag = actions.takeDrag(token, matching: primary) else {
                        feedback = "The drag selection expired. Select the items again."
                        return
                    }
                    let move = proposedOperation == .move && drag.moves(to: destination, option: option)
                    actions.transfer(drag.sources, into: destination.url, move: move, destination: destination)
                } else {
                    let sources = try await Task.detached { try urls.map { try FileReference(url: $0) } }.value
                    actions.transfer(sources, into: destination.url, move: false, destination: destination)
                }
            } catch { feedback = error.localizedDescription }
        }
        return true
    }

    private static func fileURL(from provider: NSItemProvider) async -> URL? {
        await withCheckedContinuation { continuation in
            _ = provider.loadObject(ofClass: NSURL.self) { object, _ in
                let url = object as? URL
                continuation.resume(returning: url?.isFileURL == true ? url : nil)
            }
        }
    }

    private static func dragToken(from provider: NSItemProvider) async -> UUID? {
        guard provider.hasItemConformingToTypeIdentifier(BrowserDragSelection.type) else { return nil }
        return await withCheckedContinuation { continuation in
            _ = provider.loadDataRepresentation(forTypeIdentifier: BrowserDragSelection.type) { data, _ in
                continuation.resume(returning: data.flatMap {
                    BrowserDragSelection.localToken(from: String(decoding: $0, as: UTF8.self))
                })
            }
        }
    }

    private func eject(_ volume: VolumeInfo) {
        do {
            try NSWorkspace.shared.unmountAndEjectDevice(at: volume.url)
            model.refreshVolumes()
        } catch { feedback = error.localizedDescription }
    }

    @ViewBuilder
    private func sheetContents(_ value: BrowserSheet) -> some View {
        switch value {
        case .name(let request):
            NameSheet(request: request) { name in
                if let source = request.source { actions.rename(source, to: name) }
                else if let template = request.template { actions.create(from: template, named: name) }
                else {
                    actions.create(in: request.folder, named: name, directory: request.directory,
                                   expectedFolder: request.expectedFolder)
                }
                sheet = nil
            }
        case .properties(let entry): PropertiesSheet(entry: entry)
        case .preview(let entry): PreviewSheet(entry: entry)
        case .openWith(let entry, let apps): OpenWithSheet(entry: entry, apps: apps)
        case .archive(let catalog): ArchiveBrowserSheet(catalog: catalog, changed: model.refresh)
        case .createZip(let sources, let destination):
            ZipCreateSheet(sources: sources, destination: destination, changed: model.refresh)
        case .diskUsage(let folder): DiskUsageSheet(folder: folder, openFolder: model.openFolder)
        case .duplicates(let folder): DuplicateSheet(folder: folder, openFolder: model.openFolder)
        case .compare(let folder): CompareFoldersSheet(first: folder, actions: actions, openFolder: model.openFolder)
        case .checksum(let reference): ChecksumSheet(file: reference)
        case .trash: TrashSheet(actions: actions)
        case .trashFailures(let sources): TrashFailuresSheet(sources: sources, actions: actions)
        case .operationIssues(let issues): OperationIssuesSheet(issues: issues)
        case .interruptedTransfer:
            if let notice = actions.interruptedTransfer {
                InterruptedTransferSheet(notice: notice, actions: actions, openFolder: model.openFolder)
            }
        case .settings:
            RichFilesSettings(workspace: workspace, windowSize: windowSize) { sheet = nil }
                .modifier(BrowserAppearance())
        case .searchOptions: SearchOptionsSheet(model: model)
        case .batch(let references): BatchRenameSheet(sources: references, folder: model.folder, actions: actions)
        case .connect: ConnectSheet()
        case .workspaces: WorkspacesSheet(window: workspace)
        case .fileShelf(let destination):
            FileShelfSheet(actions: actions, destination: destination, readOnly: !writableFolder)
        case .actionPalette:
            ActionPaletteSheet(commands: BrowserAction.all.filter { isAvailable($0.command) },
                               shortcutLabels: shortcutLabels()) { paletteChoice = $0 }
        }
    }

    private func shortcutLabels() -> [BrowserCommand: String] {
        let configured = BrowserShortcut.load(shortcutData)
        let preset = Dictionary(grouping: BrowserShortcut.preset(windowsKeys), by: { $0.value })
        var labels = preset.mapValues { $0.map { $0.key.label }.sorted().joined(separator: " / ") }
        for (action, chord) in configured { labels[action] = chord.label }
        return labels
    }

    private func isAvailable(_ command: BrowserCommand) -> Bool {
        let entries = model.selectedEntries
        switch command {
        case .open, .properties, .copy, .addToShelf: return !entries.isEmpty
        case .cut, .trash, .rename: return !entries.isEmpty && !actions.busy
        case .preview: return entries.count == 1
        case .checksum: return entries.count == 1 && entries.first?.kind == .regularFile
        case .back: return model.canGoBack
        case .forward: return model.canGoForward
        case .up: return model.folder.path != "/"
        case .undo: return actions.undoName != nil && !actions.busy
        case .redo: return actions.redoName != nil && !actions.busy
        case .paste: return actions.canScheduleTransfer && writableFolder
        case .newFolder: return !actions.busy && writableFolder
        case .closeTab: return model.tabs.count > 1 || hostOpenWindow == nil
        case .switchPane: return workspace.isSplit
        default: return true
        }
    }
}

private struct BrowserDropTarget: DropDelegate {
    let proposal: (DropInfo) -> DropProposal
    let exited: () -> Void
    let receive: ([NSItemProvider]) -> Bool

    func dropEntered(info: DropInfo) { _ = proposal(info) }
    func dropUpdated(info: DropInfo) -> DropProposal? { proposal(info) }
    func dropExited(info: DropInfo) { exited() }
    func performDrop(info: DropInfo) -> Bool {
        receive(info.itemProviders(for: [UTType.fileURL.identifier, BrowserDragSelection.type]))
    }
}

@MainActor
private struct AncestorColumn: View {
    let folder: URL
    let child: URL
    let showHidden: Bool
    let revision: UUID
    let open: (URL) -> Void

    @State private var children: [FileEntry] = []
    @State private var loading = true
    @State private var problem = ""

    private struct Request: Hashable {
        let folder: URL
        let child: URL
        let showHidden: Bool
        let revision: UUID
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button(folder.path == "/" ? "Computer" : folder.lastPathComponent) { open(folder) }
                .buttonStyle(.plain).font(.headline).lineLimit(1).padding(8)
                .accessibilityLabel("Open \(folder.path)")
            Divider()
            ScrollViewReader { scroll in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 1) {
                        ForEach(children) { entry in
                            let current = entry.url.path == child.path
                            Button { open(entry.url) } label: {
                                HStack(spacing: 5) {
                                    Label(entry.name, systemImage: "folder").lineLimit(1).truncationMode(.middle)
                                    Spacer(minLength: 3)
                                    Image(systemName: "chevron.right").font(.caption).foregroundStyle(.secondary)
                                }.padding(5).contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .background(current ? Color.accentColor.opacity(0.25) : .clear,
                                        in: RoundedRectangle(cornerRadius: 4))
                            .accessibilityAddTraits(current ? .isSelected : [])
                            .id(entry.url.path)
                        }
                        if loading { ProgressView().controlSize(.mini).accessibilityLabel("Reading folders") }
                        if !problem.isEmpty { Text(problem).font(.caption).foregroundStyle(.secondary) }
                    }.padding(4)
                }.onChange(of: children.map(\.url)) { _ in scroll.scrollTo(child.path) }
                    .onChange(of: child) { scroll.scrollTo($0.path) }
            }
        }.frame(maxHeight: .infinity, alignment: .top)
            .task(id: Request(folder: folder, child: child, showHidden: showHidden, revision: revision)) {
                loading = true
                problem = ""
                do {
                    let result = try await BrowserModel.folderChildren(
                        of: folder, showHidden: showHidden, revealing: child.lastPathComponent
                    )
                    if !Task.isCancelled { children = result; loading = false }
                } catch {
                    if !Task.isCancelled { children = []; problem = error.localizedDescription; loading = false }
                }
            }
    }
}

private struct FileColumnFrameKey: PreferenceKey {
    static var defaultValue: CGRect = .null
    static func reduce(value: inout CGRect, nextValue: () -> CGRect) {
        let next = nextValue()
        if !next.isNull { value = next }
    }
}

private struct RowFramesKey: PreferenceKey {
    static var defaultValue: [URL: CGRect] = [:]
    static func reduce(value: inout [URL: CGRect], nextValue: () -> [URL: CGRect]) {
        value.merge(nextValue(), uniquingKeysWith: { _, latest in latest })
    }
}

private struct FolderTreeRow: View {
    let url: URL
    let title: String
    let symbol: String
    let current: URL
    let showHidden: Bool
    let reveal: FolderTreeReveal?
    let open: (URL) -> Void
    var isPin = false
    var volume: VolumeInfo?
    @Environment(\.colorScheme) private var scheme
    @AppStorage("richfiles.textSize") private var textSize = BrowserTextSize.standard
    @State private var expanded = false
    @State private var children: [FileEntry] = []
    @State private var problem = ""
    @State private var missing = false

    private var childToReveal: String? { reveal?.nextChild[url.standardizedFileURL.path] }

    private struct LoadRequest: Hashable {
        let expanded: Bool
        let showHidden: Bool
        let child: String?
    }

    var body: some View {
        let palette = BrowserPalette(scheme)
        let here = current.standardizedFileURL.path == url.standardizedFileURL.path
        DisclosureGroup(isExpanded: $expanded) {
            ForEach(children) { entry in
                FolderTreeRow(url: entry.url, title: entry.name, symbol: "folder", current: current,
                              showHidden: showHidden, reveal: reveal, open: open)
            }
            if !problem.isEmpty { Text(problem).font(.caption).foregroundStyle(.secondary).lineLimit(2) }
        } label: {
            Button { open(url) } label: {
                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 6) {
                        Image(systemName: missing ? "exclamationmark.triangle" : symbol)
                            .foregroundStyle(missing ? palette.attention : palette.folder)
                        Text(title).lineLimit(1)
                        if missing {
                            Text("Missing").font(BrowserTypography.reading(13 * textSize.scale))
                                .foregroundStyle(palette.attention)
                        }
                    }
                    if let volume, let available = volume.availableBytes, let total = volume.totalBytes, total > 0 {
                        Text("\(formattedBytes(available)) free of \(formattedBytes(total))")
                            .font(BrowserTypography.reading(14 * textSize.scale))
                            .foregroundStyle(palette.muted).lineLimit(1)
                        GeometryReader { geometry in
                            Capsule().fill(palette.controlLine.opacity(0.35))
                            Capsule().fill(palette.folder)
                                .frame(width: geometry.size.width * min(1, max(0, Double(total - available) / Double(total))))
                        }.frame(height: 4).accessibilityHidden(true)
                    }
                }
                .fontWeight(here ? .semibold : .regular)
                .padding(.horizontal, 6)
                .padding(.vertical, volume == nil ? 3 : 5)
                .frame(maxWidth: .infinity, minHeight: 26 * textSize.scale, alignment: .leading)
                .background(here ? palette.selected : .clear, in: RoundedRectangle(cornerRadius: 5))
                .overlay(alignment: .leading) {
                    if here { Capsule().fill(palette.signal).frame(width: 3).allowsHitTesting(false) }
                }
            }
            .buttonStyle(.plain)
            .help(url.path)
        }
        .accessibilityLabel(title)
        .accessibilityAddTraits(here ? .isSelected : [])
        .accessibilityValue(missing ? "Missing" : "")
        .accessibilityAction(named: "Open Folder") { open(url) }
        .task(id: current) {
            guard isPin else { return }
            let location = url
            let available = await Task.detached(priority: .utility) {
                FileManager.default.fileExists(atPath: location.path)
            }.value
            if !Task.isCancelled { missing = !available }
        }
        .onAppear { if childToReveal != nil { expanded = true } }
        .onChange(of: reveal?.id) { _ in if childToReveal != nil { expanded = true } }
        .task(id: LoadRequest(expanded: expanded, showHidden: showHidden, child: childToReveal)) {
            guard expanded else { return }
            do {
                let loaded = try await BrowserModel.folderChildren(of: url, showHidden: showHidden,
                                                                   revealing: childToReveal)
                if !Task.isCancelled { children = loaded; problem = "" }
            } catch {
                if !Task.isCancelled { children = []; problem = error.localizedDescription }
            }
        }
    }
}

func control(_ title: String, _ symbol: String, action: @escaping () -> Void) -> some View {
    Button(action: action) { Image(systemName: symbol).frame(minWidth: 16, minHeight: 18) }
        .buttonStyle(.borderless).help(title).accessibilityLabel(title)
}

func formattedBytes(_ bytes: Int64) -> String {
    ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
}

func formattedDate(_ date: Date) -> String {
    date.formatted(date: .abbreviated, time: .shortened)
}

func fileSymbol(_ entry: FileEntry) -> String {
    if entry.needsDownload { return "icloud.and.arrow.down" }
    switch entry.kind {
    case .directory: return "folder.fill"
    case .package: return "app"
    case .symbolicLink: return "link"
    case .regularFile, .other: return "doc"
    }
}

private func safeAdd(_ left: Int64, _ right: Int64) -> Int64 {
    let (result, overflow) = left.addingReportingOverflow(right)
    return overflow ? Int64.max : result
}
