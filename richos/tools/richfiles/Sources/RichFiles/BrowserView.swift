import AppKit
import RichFilesCore
import SwiftUI
import UniformTypeIdentifiers

@MainActor
struct BrowserView: View {
    @StateObject private var model: BrowserModel
    @ObservedObject private var actions: BrowserActions
    @ObservedObject private var openRequests: FolderOpenDelegate
    @Environment(\.openWindow) private var openWindow
    @Environment(\.scenePhase) private var scenePhase
    private let hostOpenWindow: (() -> Void)?
    private let acceptsKeyboard: () -> Bool
    private let supportsSessionRestore: Bool
    @AppStorage("richfiles.windowsKeys") private var windowsKeys = true
    @AppStorage("richfiles.shortcuts.v1") private var shortcutData = Data()
    @AppStorage("richfiles.sidebarVisible") private var sidebarVisible = true
    @AppStorage("richfiles.inspectorVisible") private var inspectorVisible = false
    @AppStorage("richfiles.restoreSession") private var restoreSession = false
    @SceneStorage("richfiles.browserSession.v1") private var savedSession: Data?
    @FocusState private var focus: Field?
    @State private var editingAddress = false
    @State private var address = ""
    @State private var suggestions: [URL] = []
    @State private var sheet: BrowserSheet?
    @State private var feedback = ""
    @State private var rangeAnchor: URL?
    @State private var selectionCursor: URL?
    @State private var prefixText = ""
    @State private var prefixTime: TimeInterval = 0
    @State private var listRows = 12
    @State private var iconColumns = 4
    @State private var dropTarget = false
    @State private var localDrag: (token: UUID, reference: FileReference)?
    @State private var selectedReferences: [URL: FileReference] = [:]
    @State private var preparingAction = false
    @State private var rowFrames: [URL: CGRect] = [:]
    @State private var selectionRectangle: CGRect?
    @State private var folderReveal: FolderTreeReveal?
    @State private var appeared = false
    @State private var paletteChoice: BrowserCommand?

    private enum Field: Hashable { case address, search }

    init(start: URL, actions: BrowserActions, openRequests: FolderOpenDelegate,
         openWindow: (() -> Void)? = nil, acceptsKeyboard: @escaping () -> Bool = { true },
         supportsSessionRestore: Bool = false) {
        _model = StateObject(wrappedValue: BrowserModel(start: start))
        self.actions = actions
        self.openRequests = openRequests
        hostOpenWindow = openWindow
        self.acceptsKeyboard = acceptsKeyboard
        self.supportsSessionRestore = supportsSessionRestore
    }

    var body: some View {
        VStack(spacing: 0) {
            navigationBar
            if editingAddress && !suggestions.isEmpty { addressSuggestions }
            Divider()
            HSplitView {
                if sidebarVisible { sidebar.frame(minWidth: 160, idealWidth: 200, maxWidth: 340) }
                VStack(spacing: 0) {
                    tabs
                    Divider()
                    commandBar
                    Divider()
                    itemPane
                    Divider()
                    statusBar
                }
                .frame(minWidth: 430, maxWidth: .infinity, maxHeight: .infinity)
                if inspectorVisible {
                    inspector.frame(minWidth: 190, idealWidth: 260, maxWidth: 410)
                }
            }
        }
        .background(KeyboardBridge(windowsKeys: windowsKeys, acceptsInput: acceptsKeyboard,
                                   shortcuts: BrowserShortcut.load(shortcutData),
                                   perform: perform, prefix: selectPrefix)
            .frame(width: 0, height: 0))
        .focusedSceneValue(\.browserCommand) { _ = perform($0) }
        .onAppear {
            if !appeared {
                appeared = true
                if supportsSessionRestore, restoreSession, let savedSession {
                    model.restoreSession(savedSession)
                }
            }
            receiveFolderOpen()
        }
        .onChange(of: model.tabs.map(\.folder)) { _ in saveSession() }
        .onChange(of: model.currentTabID) { _ in saveSession() }
        .onChange(of: scenePhase) { if $0 != .active { saveSession() } }
        .onChange(of: restoreSession) { _ in saveSession() }
        .onDisappear { saveSession() }
        .onChange(of: actions.mutationRevision) { _ in model.refresh() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            actions.refreshClipboard()
        }
        .onChange(of: openRequests.requestID) { _ in receiveFolderOpen() }
        .onChange(of: model.folder) { _ in
            editingAddress = false
            focus = nil
            suggestions = []
            feedback = ""
            rangeAnchor = nil
            selectionCursor = nil
        }
        .task(id: model.selected) { await captureSelection() }
        .sheet(item: $sheet, onDismiss: runPaletteChoice) { value in sheetContents(value) }
    }

    private func saveSession() {
        guard supportsSessionRestore, appeared else { return }
        savedSession = restoreSession ? model.sessionData() : nil
    }

    private func runPaletteChoice() {
        guard let command = paletteChoice else { return }
        paletteChoice = nil
        _ = perform(command)
    }

    private var navigationBar: some View {
        HStack(spacing: 8) {
            control("Sidebar", "sidebar.left") { sidebarVisible.toggle() }
            control("Back", "chevron.left") { model.goBack() }.disabled(!model.canGoBack)
            control("Forward", "chevron.right") { model.goForward() }.disabled(!model.canGoForward)
            control("Up", "arrow.up") { model.goUp() }.disabled(model.folder.path == "/")
            control("Refresh", "arrow.clockwise") { model.refreshVolumes(); model.refresh() }
            Group {
                if editingAddress {
                    TextField("Folder path", text: $address)
                        .textFieldStyle(.plain)
                        .focused($focus, equals: .address)
                        .accessibilityLabel("Folder path")
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
                            ForEach(Array(model.breadcrumbs.enumerated()), id: \.element) { index, path in
                                if index > 0 { Image(systemName: "chevron.right").font(.system(size: 8)).foregroundStyle(.secondary) }
                                Button(path.path == "/" ? "Computer" : path.lastPathComponent) { model.openFolder(path) }
                                    .buttonStyle(.plain)
                                    .help(path.path)
                            }
                        }
                    }
                }
            }
            .frame(maxWidth: .infinity, minHeight: 24, alignment: .leading)
            .padding(.horizontal, 8)
            .background(.quaternary, in: RoundedRectangle(cornerRadius: 6))
            control("Edit path", "pencil") { editAddress() }
            control("Copy current path", "doc.on.doc") { copyPaths([model.folder]) }
            TextField(model.searchContent ? "Search text contents" : "Search this folder", text: $model.searchText)
                .textFieldStyle(.roundedBorder)
                .focused($focus, equals: .search)
                .accessibilityLabel(model.searchContent ? "Search text contents" : "Search this folder")
                .accessibilityIdentifier("folder-search")
                .frame(minWidth: 130, idealWidth: 190, maxWidth: 240)
                .onSubmit { startSearch() }
            if model.searchActive {
                control("Clear Search", "xmark.circle") { model.clearSearch() }
            }
            control("Search Filters", "line.3.horizontal.decrease.circle") { sheet = .searchOptions }
        }
        .padding(9)
        .background(.bar)
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
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    sideHeading("Quick Access")
                    ForEach(model.places) { place in
                        FolderTreeRow(url: place.url, title: place.title, symbol: place.symbol,
                                      current: model.folder, showHidden: model.showHidden,
                                      reveal: reveal(for: place.url), open: model.openFolder)
                    }
                    ForEach(Array(model.favorites.enumerated()), id: \.element) { index, url in
                        FolderTreeRow(url: url, title: url.lastPathComponent, symbol: "pin",
                                      current: model.folder, showHidden: model.showHidden,
                                      reveal: reveal(for: url), open: model.openFolder)
                            .contextMenu {
                                Button("Remove from Quick Access") { model.removeFavorite(url) }
                                Button("Move Up") { model.moveFavorite(from: IndexSet(integer: index), to: index - 1) }
                                    .disabled(index == 0)
                                Button("Move Down") { model.moveFavorite(from: IndexSet(integer: index), to: index + 2) }
                                    .disabled(index == model.favorites.count - 1)
                            }
                    }
                    Button { model.addFavorite(model.folder) } label: {
                        Label("Pin Current Folder", systemImage: "plus").font(.callout)
                    }
                    .buttonStyle(.plain)
                    .padding(.vertical, 5)
                }
                VStack(alignment: .leading, spacing: 2) {
                    sideHeading("Computer")
                    ForEach(model.volumes) { volume in
                        FolderTreeRow(url: volume.url, title: volume.name,
                                      symbol: volume.isLocal == false ? "network" : "externaldrive",
                                      current: model.folder, showHidden: model.showHidden,
                                      reveal: reveal(for: volume.url), open: model.openFolder)
                            .help(volume.availableBytes.map { "\(formattedBytes($0)) available" } ?? volume.name)
                            .contextMenu {
                                if volume.isEjectable == true {
                                    Button("Eject") { eject(volume) }.disabled(actions.busy)
                                }
                                Button("Copy Path") { copyPaths([volume.url]) }
                            }
                    }
                    if !model.volumeStatus.isEmpty {
                        Text(model.volumeStatus).font(.caption).foregroundStyle(.secondary)
                    }
                    Button { sheet = .connect } label: {
                        Label("Connect to Server", systemImage: "network").font(.callout)
                    }
                    .buttonStyle(.plain)
                    .padding(.vertical, 5)
                }
                Button { sheet = .trash } label: { Label("Trash", systemImage: "trash") }
                    .buttonStyle(.plain)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(10)
        }
        .background(.bar)
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
        Text(label).font(.caption.weight(.semibold)).foregroundStyle(.secondary)
            .padding(.bottom, 3).accessibilityAddTraits(.isHeader)
    }

    private var tabs: some View {
        HStack(spacing: 5) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 4) {
                    ForEach(model.tabs) { tab in
                        HStack(spacing: 5) {
                            Button(tab.title) { model.switchTab(tab.id) }
                                .buttonStyle(.plain)
                                .lineLimit(1)
                                .help(tab.folder.path)
                                .accessibilityAddTraits(tab.id == model.currentTabID ? .isSelected : [])
                            if model.tabs.count > 1 {
                                Button { model.closeTab(tab.id) } label: {
                                    Image(systemName: "xmark").font(.system(size: 9))
                                }.buttonStyle(.plain).accessibilityLabel("Close \(tab.title) tab")
                            }
                        }
                        .padding(.horizontal, 8).padding(.vertical, 5)
                        .background(tab.id == model.currentTabID ? Color.accentColor.opacity(0.13) : Color.clear,
                                    in: RoundedRectangle(cornerRadius: 5))
                    }
                }
            }
            control("New Tab", "plus") { model.newTab() }
            control("New Window", "macwindow.badge.plus") {
                if let hostOpenWindow { hostOpenWindow() }
                else { openWindow(id: "browser") }
            }
        }
        .padding(.horizontal, 8).padding(.vertical, 4)
    }

    private var commandBar: some View {
        HStack(spacing: 9) {
            Menu {
                creationMenu
            } label: { Label("New", systemImage: "plus") }
                .disabled(actions.busy || model.currentVolume?.isReadOnly == true)
            control("Cut", "scissors") { _ = perform(.cut) }.disabled(model.selected.isEmpty || actions.busy)
            control("Copy", "doc.on.doc") { _ = perform(.copy) }.disabled(model.selected.isEmpty)
            Menu {
                Button("Paste, Skip Conflicts") { actions.paste(into: model.folder) }
                Button("Paste, Keep Both") { actions.paste(into: model.folder, conflict: .keepBoth) }
            } label: { Label("Paste", systemImage: "clipboard") }
                .disabled(actions.busy || model.currentVolume?.isReadOnly == true)
            control("Move to Trash", "trash") { _ = perform(.trash) }.disabled(model.selected.isEmpty || actions.busy)
            Menu {
                folderMenu
            } label: { Image(systemName: "ellipsis.circle") }
                .accessibilityLabel("Current folder actions")
                .help("Current folder actions")
            if let undo = actions.undoName {
                control("Undo " + undo, "arrow.uturn.backward") { actions.undo() }.disabled(actions.busy)
            }
            Spacer(minLength: 4)
            Picker("View", selection: $model.viewMode) {
                Text("List").tag(BrowserViewMode.list)
                Text("Details").tag(BrowserViewMode.details)
                Text("Thumbnails").tag(BrowserViewMode.icons)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(width: 204)
            .accessibilityLabel("File view")
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
            control("Preview Pane", "sidebar.right") { inspectorVisible.toggle() }
        }
        .buttonStyle(.borderless)
        .padding(.horizontal, 8).padding(.vertical, 6)
    }

    private func sortButton(_ title: String, _ column: FileSort.Column) -> some View {
        Button(title) {
            model.sort = FileSort(column: column, ascending: model.sort.ascending, foldersFirst: model.sort.foldersFirst)
        }
    }

    private var itemPane: some View {
        ZStack(alignment: .top) {
            Group {
                if model.entries.isEmpty && !model.busy {
                    VStack(spacing: 8) {
                        Image(systemName: model.searchActive ? "magnifyingglass" : "folder").font(.system(size: 32))
                        Text(model.searchActive ? "No matching files" : "No items to show")
                        if !model.status.isEmpty { Text(model.status).font(.callout).multilineTextAlignment(.center) }
                    }.foregroundStyle(.secondary).padding(28)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    switch model.viewMode {
                    case .details: details
                    case .list: compactList
                    case .icons: thumbnailGrid
                    }
                }
            }
            if model.busy {
                ProgressView().controlSize(.mini).padding(4)
                    .frame(maxWidth: .infinity, alignment: .trailing)
                    .accessibilityLabel("Reading files")
            }
            if dropTarget {
                Text("Drop files here").padding(7).background(.regularMaterial, in: RoundedRectangle(cornerRadius: 6))
                    .padding(8).allowsHitTesting(false)
            }
            if let selectionRectangle {
                Rectangle().fill(Color.accentColor.opacity(0.12))
                    .overlay(Rectangle().stroke(Color.accentColor.opacity(0.8)))
                    .frame(width: selectionRectangle.width, height: selectionRectangle.height)
                    .position(x: selectionRectangle.midX, y: selectionRectangle.midY)
                    .allowsHitTesting(false)
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
        .coordinateSpace(name: "richfiles-items")
        .onPreferenceChange(RowFramesKey.self) { rowFrames = $0 }
        .simultaneousGesture(DragGesture(minimumDistance: 5, coordinateSpace: .named("richfiles-items"))
            .onChanged { change in
                guard model.viewMode != .details,
                      !rowFrames.values.contains(where: { $0.contains(change.startLocation) }) else { return }
                let rect = CGRect(
                    x: min(change.startLocation.x, change.location.x),
                    y: min(change.startLocation.y, change.location.y),
                    width: abs(change.startLocation.x - change.location.x),
                    height: abs(change.startLocation.y - change.location.y)
                )
                selectionRectangle = rect
                model.selected = Set(rowFrames.compactMap { rect.intersects($0.value) ? $0.key : nil })
            }
            .onEnded { _ in selectionRectangle = nil })
        .onDrop(of: [UTType.fileURL], isTargeted: $dropTarget, perform: receiveDrop)
        .contextMenu { folderMenu }
    }

    private var details: some View {
        Table(model.entries, selection: $model.selected) {
            TableColumn("Name") { entry in
                nameLabel(entry).onDrag { beginDrag(entry) }
            }.width(min: 150, ideal: 260)
            TableColumn("Size") { entry in Text(entry.size.map(formattedBytes) ?? "–") }
                .width(min: 70, ideal: 82)
            TableColumn("Kind") { entry in Text(entry.typeDescription ?? "–").lineLimit(1) }
                .width(min: 80, ideal: 110)
            TableColumn("Modified") { entry in Text(entry.modified.map(formattedDate) ?? "–").lineLimit(1) }
                .width(min: 100, ideal: 130)
            TableColumn("Folder") { entry in
                Text(model.searchActive ? entry.url.deletingLastPathComponent().path : "")
                    .lineLimit(1).truncationMode(.middle)
            }.width(min: 70, ideal: 125)
        }
        .accessibilityIdentifier("file-details")
        .contextMenu(forSelectionType: URL.self) { ids in
            selectionMenu(model.entries.filter { ids.contains($0.url) })
        } primaryAction: { ids in
            activate(model.entries.filter { ids.contains($0.url) })
        }
    }

    private var compactList: some View {
        GeometryReader { geometry in
            let rows = max(1, Int((geometry.size.height - 10) / 22))
            ScrollViewReader { scroll in
                ScrollView(.horizontal) {
                    LazyHGrid(rows: Array(repeating: GridItem(.fixed(22), spacing: 0, alignment: .leading), count: rows),
                              alignment: .top, spacing: 8) {
                        ForEach(model.entries) { entry in
                            selectable(entry) {
                                VStack(alignment: .leading, spacing: 0) {
                                    nameLabel(entry)
                                    if model.searchActive {
                                        Text(entry.url.deletingLastPathComponent().lastPathComponent)
                                            .font(.system(size: 9)).lineLimit(1).foregroundStyle(.secondary)
                                    }
                                }
                                .frame(width: 240, alignment: .leading)
                            }
                        }
                    }
                    .padding(5)
                }
                .onAppear { listRows = rows; restoreScroll(scroll) }
                .onChange(of: rows) { listRows = $0 }
                .onChange(of: selectionCursor) { value in
                    if let value { scroll.scrollTo(value) }
                }
                .onChange(of: model.busy) { busy in if !busy { restoreScroll(scroll) } }
            }
        }
        .accessibilityIdentifier("file-list")
        .accessibilityLabel("List view, filenames and extensions")
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
                                    if model.searchActive {
                                        Text(entry.url.deletingLastPathComponent().lastPathComponent)
                                            .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                    }
                                }.frame(maxWidth: .infinity).padding(4)
                            }
                        }
                    }.padding(7)
                }
                .onAppear { iconColumns = columns; restoreScroll(scroll) }
                .onChange(of: columns) { iconColumns = $0 }
                .onChange(of: selectionCursor) { value in if let value { scroll.scrollTo(value) } }
                .onChange(of: model.busy) { busy in if !busy { restoreScroll(scroll) } }
            }
        }
        .accessibilityIdentifier("file-thumbnails")
    }

    private func selectable<Content: View>(_ entry: FileEntry, @ViewBuilder content: () -> Content) -> some View {
        let selected = model.selected.contains(entry.url)
        return content()
            .padding(.horizontal, 4)
            .background(selected ? Color.accentColor.opacity(0.25) : Color.clear, in: RoundedRectangle(cornerRadius: 4))
            .background(GeometryReader { geometry in
                Color.clear.preference(key: RowFramesKey.self, value: [entry.url: geometry.frame(in: .named("richfiles-items"))])
            })
            .contentShape(Rectangle())
            .gesture(TapGesture(count: 2).onEnded {
                select(entry, modifiers: [])
                activate([entry])
            }.exclusively(before: TapGesture().onEnded {
                select(entry, modifiers: NSEvent.modifierFlags)
            }))
            .onDrag { beginDrag(entry) }
            .contextMenu {
                selectionMenu(selected ? model.selectedEntries : [entry])
            }
            .help(entry.name)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(entry.name)
            .accessibilityAddTraits(selected ? [.isSelected, .isButton] : .isButton)
            .accessibilityAction { select(entry, modifiers: []) }
            .accessibilityAction(named: "Open") { activate([entry]) }
            .id(entry.url)
    }

    private func nameLabel(_ entry: FileEntry) -> some View {
        Label {
            Text(entry.name).lineLimit(1).truncationMode(.middle)
        } icon: {
            Image(systemName: fileSymbol(entry)).foregroundStyle(entry.kind == .directory ? Color.accentColor : .secondary)
        }
        .opacity(actions.cutURLs.contains(where: { $0.path == entry.url.path }) ? 0.45 : 1)
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
            Button("Paste") { actions.paste(into: model.folder) }.disabled(actions.busy)
            Button("Create ZIP…") {
                let destination = model.folder
                withReferences(entries) { sheet = .createZip($0, destination) }
            }.disabled(actions.busy)
            Button("Copy Path") { copyPaths(entries.map(\.url)) }
            if model.searchActive {
                Button("Open Containing Folder") { model.openFolder(first.url.deletingLastPathComponent()) }
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
        }.disabled(actions.busy || model.currentVolume?.isReadOnly == true)
    }

    @ViewBuilder
    private var folderMenu: some View {
        creationMenu
        Divider()
        Button("Paste") { actions.paste(into: model.folder) }.disabled(actions.busy)
        Button("Copy Current Path") { copyPaths([model.folder]) }
        Button("Expand to Current Folder") { expandToCurrentFolder() }
        Button("Workspaces…") { sheet = .workspaces }
        Button("Disk Usage…") { sheet = .diskUsage(model.folder) }
        Button("Find Duplicate Files…") { sheet = .duplicates(model.folder) }
        Button("Compare Folders…") { sheet = .compare(model.folder) }
        Button("Refresh") { _ = perform(.refresh) }
    }

    private var inspector: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Preview").font(.headline)
            if model.selectedEntries.count == 1, let entry = model.selectedEntries.first {
                if entry.kind == .directory {
                    FileThumbnail(entry: entry, side: 110)
                } else {
                    NativePreview(entry: entry).frame(minHeight: 150, maxHeight: .infinity)
                }
                Text(entry.name).font(.headline).textSelection(.enabled)
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
            if let available = model.currentVolume?.availableBytes { Text("\(formattedBytes(available)) free") }
            if model.currentVolume?.isReadOnly == true { Text("Read-only") }
            Spacer(minLength: 4)
            if let transfer = actions.transfer {
                Button(transfer.running ? "Transfers \(transfer.completed)/\(transfer.total)" : "Transfers") {
                    sheet = .transfers
                }.buttonStyle(.borderless)
            }
            if model.busy && model.searchActive { Button("Stop Search") { model.stopSearch() }.buttonStyle(.borderless) }
            let itemCount = "\(model.entries.count) \(model.searchActive ? "results" : "items")"
            if !model.status.isEmpty && model.status != itemCount {
                Text(model.status).lineLimit(1).help(model.status)
            }
            let status = !feedback.isEmpty ? feedback : actions.status
            if !status.isEmpty { Text(status).lineLimit(1).truncationMode(.tail).help(status) }
        }
        .font(.caption).foregroundStyle(.secondary)
        .padding(.horizontal, 9).padding(.vertical, 5)
        .background(.bar)
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
        case .paste: actions.paste(into: model.folder)
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
        case .refresh: model.refreshVolumes(); model.refresh()
        case .address: editAddress()
        case .revealFolder: expandToCurrentFolder()
        case .workspaces: sheet = .workspaces
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
        let step = model.viewMode == .list ? dx * listRows + dy : dx + dy * iconColumns
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

    private func restoreScroll(_ scroll: ScrollViewProxy) {
        if let anchor = model.scrollAnchor { scroll.scrollTo(anchor) }
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

    private func receiveFolderOpen() {
        for (index, url) in openRequests.takePending().enumerated() {
            if index == 0 { model.openFolder(url) }
            else { model.newTab(url) }
        }
    }

    private func activate(_ entries: [FileEntry]) {
        guard let first = entries.first else { return }
        if entries.count == 1 && first.kind == .directory {
            model.openFolder(first.url)
            return
        }
        if entries.count == 1 && first.kind == .regularFile && first.url.pathExtension.lowercased() == "zip" {
            Task {
                do { sheet = .zip(try await ArchiveTools.list(first)) }
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
        guard !actions.busy else { return }
        let name = directory ? "New Folder" : "New File" + (ext.map { "." + $0 } ?? "")
        sheet = .name(NameRequest(source: nil, folder: model.folder, directory: directory, name: name))
    }

    private func captureSelection() async {
        let selected = model.selected
        selectedReferences = selectedReferences.filter { selected.contains($0.key) }
        let entries = model.selectedEntries.filter { selectedReferences[$0.url] == nil }
        if entries.isEmpty { return }
        let captured = try? await Self.capture(entries)
        guard !Task.isCancelled, model.selected == selected else { return }
        if let captured {
            for (entry, reference) in zip(entries, captured) { selectedReferences[entry.url] = reference }
        }
    }

    private func withReferences(_ entries: [FileEntry], action: @escaping ([FileReference]) -> Void) {
        guard !entries.isEmpty else { return }
        let cached = entries.compactMap { selectedReferences[$0.url] }
        if cached.count == entries.count { action(cached); return }
        guard !preparingAction else { return }
        preparingAction = true
        Task {
            defer { preparingAction = false }
            do { action(try await Self.capture(entries)) }
            catch { feedback = error.localizedDescription }
        }
    }

    nonisolated private static func capture(_ entries: [FileEntry]) async throws -> [FileReference] {
        try entries.map { entry in
            try Task.checkCancellation()
            let url = URL(fileURLWithPath: entry.url.path)
            let reference = try FileReference(url: url)
            let current = try FileEntry(url: url)
            guard current.created == entry.created, current.modified == entry.modified,
                  current.size == entry.size, current.kind == entry.kind else {
                throw FileOperationError.sourceChanged(entry.url)
            }
            guard try FileReference(url: url).identity == reference.identity else {
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

    private func beginDrag(_ entry: FileEntry) -> NSItemProvider {
        let provider = NSItemProvider(object: entry.url as NSURL)
        if let reference = selectedReferences[entry.url] {
            let token = UUID()
            localDrag = (token, reference)
            provider.registerDataRepresentation(forTypeIdentifier: Self.dragTokenType, visibility: .ownProcess) { complete in
                complete(Data(token.uuidString.utf8), nil)
                return nil
            }
        } else {
            localDrag = nil
        }
        return provider
    }

    private func receiveDrop(_ providers: [NSItemProvider]) -> Bool {
        guard !providers.isEmpty, !actions.busy else { return false }
        let destination = model.folder
        let dragged = localDrag
        localDrag = nil
        let option = NSEvent.modifierFlags.contains(.option)
        Task {
            do {
                var urls: [URL] = []
                for provider in providers {
                    if let url = await Self.fileURL(from: provider) { urls.append(url) }
                }
                guard !urls.isEmpty else { feedback = "No local files in this drop."; return }
                let sameVolume = urls.allSatisfy { Self.sameVolume($0, destination) }
                if providers.count == 1, let dragged, urls[0].path == dragged.reference.url.path,
                   await Self.dragToken(from: providers[0]) == dragged.token {
                    actions.transfer([dragged.reference], into: destination, move: sameVolume != option)
                } else {
                    let sources = try await Task.detached { try urls.map { try FileReference(url: $0) } }.value
                    actions.transfer(sources, into: destination, move: false)
                }
            } catch { feedback = error.localizedDescription }
        }
        return true
    }

    private static func sameVolume(_ file: URL, _ folder: URL) -> Bool {
        guard let a = (try? file.resourceValues(forKeys: [.volumeIdentifierKey]))?.volumeIdentifier as? AnyHashable,
              let b = (try? folder.resourceValues(forKeys: [.volumeIdentifierKey]))?.volumeIdentifier as? AnyHashable else {
            return false
        }
        return a == b
    }

    private static func fileURL(from provider: NSItemProvider) async -> URL? {
        await withCheckedContinuation { continuation in
            _ = provider.loadObject(ofClass: NSURL.self) { object, _ in
                let url = object as? URL
                continuation.resume(returning: url?.isFileURL == true ? url : nil)
            }
        }
    }

    private static let dragTokenType = "app.richos.richfiles.local-drag"

    private static func dragToken(from provider: NSItemProvider) async -> UUID? {
        guard provider.hasItemConformingToTypeIdentifier(dragTokenType) else { return nil }
        return await withCheckedContinuation { continuation in
            _ = provider.loadDataRepresentation(forTypeIdentifier: dragTokenType) { data, _ in
                continuation.resume(returning: data.flatMap { UUID(uuidString: String(decoding: $0, as: UTF8.self)) })
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
                else { actions.create(in: request.folder, named: name, directory: request.directory) }
                sheet = nil
            }
        case .properties(let entry): PropertiesSheet(entry: entry)
        case .preview(let entry): PreviewSheet(entry: entry)
        case .openWith(let entry, let apps): OpenWithSheet(entry: entry, apps: apps)
        case .zip(let catalog): ZipBrowserSheet(catalog: catalog, changed: model.refresh)
        case .createZip(let sources, let destination):
            ZipCreateSheet(sources: sources, destination: destination, changed: model.refresh)
        case .diskUsage(let folder): DiskUsageSheet(folder: folder, openFolder: model.openFolder)
        case .duplicates(let folder): DuplicateSheet(folder: folder, openFolder: model.openFolder)
        case .compare(let folder): CompareFoldersSheet(first: folder, openFolder: model.openFolder)
        case .checksum(let reference): ChecksumSheet(file: reference)
        case .trash: TrashSheet(actions: actions)
        case .transfers: TransferSheet(actions: actions)
        case .searchOptions: SearchOptionsSheet(model: model)
        case .batch(let references): BatchRenameSheet(sources: references, folder: model.folder, actions: actions)
        case .connect: ConnectSheet()
        case .workspaces: WorkspacesSheet(model: model)
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
        case .open, .properties, .copy: return !entries.isEmpty
        case .cut, .trash, .rename: return !entries.isEmpty && !actions.busy
        case .preview: return entries.count == 1
        case .checksum: return entries.count == 1 && entries.first?.kind == .regularFile
        case .back: return model.canGoBack
        case .forward: return model.canGoForward
        case .up: return model.folder.path != "/"
        case .undo: return actions.undoName != nil && !actions.busy
        case .redo: return actions.redoName != nil && !actions.busy
        case .paste, .newFolder: return !actions.busy && model.currentVolume?.isReadOnly != true
        case .closeTab: return model.tabs.count > 1 || hostOpenWindow == nil
        default: return true
        }
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
    @State private var expanded = false
    @State private var children: [FileEntry] = []
    @State private var problem = ""

    private var childToReveal: String? { reveal?.nextChild[url.standardizedFileURL.path] }

    private struct LoadRequest: Hashable {
        let expanded: Bool
        let showHidden: Bool
        let child: String?
    }

    var body: some View {
        DisclosureGroup(isExpanded: $expanded) {
            ForEach(children) { entry in
                FolderTreeRow(url: entry.url, title: entry.name, symbol: "folder", current: current,
                              showHidden: showHidden, reveal: reveal, open: open)
            }
            if !problem.isEmpty { Text(problem).font(.caption).foregroundStyle(.secondary).lineLimit(2) }
        } label: {
            Button { open(url) } label: {
                Label(title, systemImage: symbol)
                    .lineLimit(1)
                    .padding(.vertical, 3)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(current.standardizedFileURL.path == url.standardizedFileURL.path
                                ? Color.accentColor.opacity(0.15) : Color.clear,
                                in: RoundedRectangle(cornerRadius: 4))
            }
            .buttonStyle(.plain)
            .help(url.path)
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
    case .directory: return "folder"
    case .package: return "app"
    case .symbolicLink: return "link"
    case .regularFile, .other: return "doc"
    }
}

private func safeAdd(_ left: Int64, _ right: Int64) -> Int64 {
    let (result, overflow) = left.addingReportingOverflow(right)
    return overflow ? Int64.max : result
}
