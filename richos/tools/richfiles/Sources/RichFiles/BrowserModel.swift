import AppKit
import Combine
import Foundation
import RichFilesCore

struct BrowserTab: Identifiable {
    let id: UUID
    var folder: URL

    var title: String {
        folder.path == "/" ? "Computer" : folder.lastPathComponent
    }
}

struct BrowserPlace: Identifiable {
    let title: String
    let symbol: String
    let url: URL

    var id: URL { url }
}

struct NamedWorkspace: Identifiable, Codable {
    let id: UUID
    let name: String
    let session: Data
}

enum WorkspaceSaveError: LocalizedError {
    case emptyName, nameInUse, cannotSave

    var errorDescription: String? {
        switch self {
        case .emptyName: return "Enter a workspace name."
        case .nameInUse: return "A workspace already uses that name."
        case .cannotSave: return "RichFiles could not save this workspace."
        }
    }
}

struct FolderTreeReveal: Equatable {
    let id = UUID()
    let root: String
    let nextChild: [String: String]

    init?(folder: URL, roots: [URL]) {
        let paths = folder.standardizedFileURL.pathComponents
        guard let root = roots.map(\.standardizedFileURL).filter({
            paths.starts(with: $0.pathComponents)
        }).max(by: { $0.pathComponents.count < $1.pathComponents.count }) else { return nil }

        self.root = root.path
        var parent = root
        var nextChild: [String: String] = [:]
        for component in paths.dropFirst(root.pathComponents.count) {
            let child = parent.appendingPathComponent(component, isDirectory: true)
            nextChild[parent.path] = component
            parent = child
        }
        self.nextChild = nextChild
    }
}

enum BrowserViewMode: String, Codable, CaseIterable, Identifiable {
    case details = "Details"
    case list = "List"
    case icons = "Icons"
    case columns = "Columns"
    case gallery = "Gallery"

    var id: Self { self }
    var title: String { self == .icons ? "Thumbnails" : rawValue }
}

@MainActor
final class BrowserModel: ObservableObject {
    static let contentSearchCoverageDescription = TextContentQuery.coverageDescription

    @Published private(set) var folder: URL
    @Published private(set) var entries: [FileEntry] = []
    @Published var selected: Set<URL> = [] {
        didSet {
            if !settingSelection && selected != oldValue {
                selectionToRestore.removeAll()
            }
        }
    }
    @Published var scrollAnchor: URL?
    @Published var status = ""
    @Published private(set) var busy = false
    @Published private(set) var scanReport: ScanReport?
    @Published private(set) var currentVolume: VolumeInfo?
    @Published private(set) var volumes: [VolumeInfo] = []
    @Published private(set) var volumeStatus = ""
    @Published private(set) var tabs: [BrowserTab]
    @Published private(set) var currentTabID: UUID
    @Published private(set) var canGoBack = false
    @Published private(set) var canGoForward = false
    @Published private(set) var favorites: [URL]
    @Published private(set) var searchActive = false
    @Published var searchText = ""
    @Published var searchContent = false
    @Published var searchExtensions = ""
    @Published var searchMinimumBytes: Int64?
    @Published var searchMaximumBytes: Int64?
    @Published var searchModified: ClosedRange<Date>?
    @Published var searchRecursively = true {
        didSet {
            if oldValue != searchRecursively && !restoringVisit && searchActive {
                reloadPreservingSelection()
            }
        }
    }
    @Published var showHidden: Bool {
        didSet {
            guard !restoringView, oldValue != showHidden else { return }
            changePreferences { $0.showHidden = showHidden }
            reloadPreservingSelection()
        }
    }
    @Published var sort: FileSort {
        didSet {
            guard !restoringView else { return }
            guard ViewPreferences.sameSort(oldValue, sort) == false else { return }
            changeView { $0.fileSort = sort }
            reloadPreservingSelection()
        }
    }
    @Published var viewMode: BrowserViewMode {
        didSet { if !restoringView && oldValue != viewMode { changeView { $0.mode = viewMode } } }
    }
    @Published var iconSize: Double {
        didSet { if !restoringView && oldValue != iconSize { changeView { $0.iconSize = iconSize } } }
    }
    @Published var detailsLayout: Data? {
        didSet { if !restoringView && oldValue != detailsLayout { changeView { $0.detailsLayout = detailsLayout } } }
    }
    @Published private(set) var remembersFolderView: Bool

    let places: [BrowserPlace]

    var selectedEntries: [FileEntry] {
        entries.filter { selected.contains($0.id) }
    }

    var breadcrumbs: [URL] {
        var result = [folder]
        var location = folder
        while location.path != "/" {
            let parent = location.deletingLastPathComponent()
            guard parent.path.hasPrefix("/"), parent.path != location.path else { break }
            result.append(parent)
            location = parent
        }
        return Array(result.reversed())
    }

    private struct SearchState: Codable, Sendable {
        var text = ""
        var extensions = ""
        var minimumBytes: Int64?
        var maximumBytes: Int64?
        var modified: ClosedRange<Date>?
        var recursive = true
        var contentMode = false
        var query: FileQuery?
        var content: TextContentQuery?
    }

    private struct Visit: Codable, Sendable {
        let folder: URL
        var selection: Set<URL> = []
        var anchor: URL?
        var search = SearchState()
    }

    private struct TabState: Codable {
        let visit: Visit
        var back: [Visit] = []
        var forward: [Visit] = []
    }

    private struct Session: Codable {
        static let maximumBytes = 256 * 1024
        let activeTab: Int
        let tabs: [TabState]
    }

    private static let workspaceKeyPrefix = "browser-workspace-v1."

    private enum Navigation {
        case open, back, forward
        case reveal(String)
    }

    private var savedTabs: [UUID: TabState] = [:]
    private var back: [Visit] = []
    private var forward: [Visit] = []
    private var query: FileQuery?
    private var contentQuery: TextContentQuery?
    private var restoringVisit = false
    private var restoringView = false
    private var settingSelection = false
    private var selectionToRestore: Set<URL> = []
    private var scanTask: Task<Void, Never>?
    private var scanID: UUID?
    private var navigationTask: Task<Void, Never>?
    private var navigationID: UUID?
    private var volumeTask: Task<Void, Never>?
    private var volumeChanges: AnyCancellable?
    private let defaults: UserDefaults

    init(start: URL, defaults: UserDefaults = RichFilesInstallation.preferences) {
        let id = UUID()
        let location = start.standardizedFileURL
        let preferences = BrowserPreferences.load(from: defaults)
        let view = preferences.folders[location.path] ?? preferences.global
        self.defaults = defaults
        folder = location
        currentTabID = id
        tabs = [BrowserTab(id: id, folder: location)]
        favorites = preferences.favorites
        showHidden = preferences.showHidden
        sort = view.fileSort
        viewMode = view.mode
        iconSize = view.iconSize
        detailsLayout = view.detailsLayout
        remembersFolderView = preferences.folders[location.path] != nil
        let home = FileManager.default.homeDirectoryForCurrentUser
        places = [
            BrowserPlace(title: "Home", symbol: "house", url: home),
            BrowserPlace(title: "Desktop", symbol: "desktopcomputer", url: home.appendingPathComponent("Desktop", isDirectory: true)),
            BrowserPlace(title: "Documents", symbol: "doc", url: home.appendingPathComponent("Documents", isDirectory: true)),
            BrowserPlace(title: "Downloads", symbol: "arrow.down.circle", url: home.appendingPathComponent("Downloads", isDirectory: true))
        ]
        let notifications = NSWorkspace.shared.notificationCenter
        volumeChanges = notifications.publisher(for: NSWorkspace.didMountNotification)
            .merge(with: notifications.publisher(for: NSWorkspace.didUnmountNotification))
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.refreshVolumes() }
        load()
        refreshVolumes()
    }

    deinit {
        scanTask?.cancel()
        navigationTask?.cancel()
        volumeTask?.cancel()
    }

    func openFolder(_ url: URL) {
        navigate(.open) { try DirectoryReader.directoryURL(url) }
    }

    func revealFile(_ location: URL) {
        navigate(.reveal(location.lastPathComponent)) {
            try FileReference(url: location).url.deletingLastPathComponent()
        }
    }

    func openAddress(_ text: String) {
        let base = folder
        navigate(.open) { try DirectoryReader.address(text, relativeTo: base) }
    }

    func refresh() {
        reloadPreservingSelection()
    }

    func goUp() {
        guard folder.path != "/" else { return }
        openFolder(folder.deletingLastPathComponent())
    }

    func goBack() {
        guard let target = back.last else { return }
        navigate(.back) { try DirectoryReader.directoryURL(target.folder) }
    }

    func goForward() {
        guard let target = forward.last else { return }
        navigate(.forward) { try DirectoryReader.directoryURL(target.folder) }
    }

    func newTab(_ url: URL? = nil) {
        let id = UUID()
        let location = url ?? folder
        tabs.append(BrowserTab(id: id, folder: location))
        savedTabs[id] = TabState(visit: Visit(folder: location))
        switchTab(id)
    }

    func switchTab(_ id: UUID) {
        guard id != currentTabID, let state = savedTabs.removeValue(forKey: id) else { return }
        savedTabs[currentTabID] = TabState(visit: captureVisit(), back: back, forward: forward)
        cancelNavigation()
        currentTabID = id
        back = state.back
        forward = state.forward
        restore(state.visit)
    }

    func closeTab(_ id: UUID) {
        guard tabs.count > 1, let index = tabs.firstIndex(where: { $0.id == id }) else { return }
        if id == currentTabID {
            let replacement = tabs[index == 0 ? 1 : index - 1].id
            switchTab(replacement)
        }
        tabs.remove(at: index)
        savedTabs.removeValue(forKey: id)
    }

    func sessionData() -> Data? {
        guard let index = tabs.firstIndex(where: { $0.id == currentTabID }) else { return nil }
        let current = TabState(visit: captureVisit(), back: back, forward: forward)
        let states = tabs.compactMap { $0.id == currentTabID ? current : savedTabs[$0.id] }
        guard states.count == tabs.count,
              let data = try? JSONEncoder().encode(Session(activeTab: index, tabs: states)),
              data.count <= Session.maximumBytes else { return nil }
        return data
    }

    static func sessionData(for folder: URL) throws -> Data {
        guard isSessionURL(folder) else { throw WorkspaceHandoff.HandoffError.invalid }
        return try JSONEncoder().encode(Session(activeTab: 0, tabs: [TabState(visit: Visit(folder: folder))]))
    }

    func namedWorkspaces() -> [NamedWorkspace] {
        defaults.dictionaryRepresentation().compactMap { key, value in
            guard key.hasPrefix(Self.workspaceKeyPrefix), let data = value as? Data,
                  let workspace = try? JSONDecoder().decode(NamedWorkspace.self, from: data),
                  key == Self.workspaceKeyPrefix + workspace.id.uuidString else { return nil }
            return workspace
        }.sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }

    func saveWorkspace(named draft: String, session snapshot: Data? = nil) throws {
        let name = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { throw WorkspaceSaveError.emptyName }
        guard !namedWorkspaces().contains(where: { $0.name.localizedCaseInsensitiveCompare(name) == .orderedSame }) else {
            throw WorkspaceSaveError.nameInUse
        }
        guard let session = snapshot ?? sessionData() else { throw WorkspaceSaveError.cannotSave }
        let workspace = NamedWorkspace(id: UUID(), name: name, session: session)
        let data = try JSONEncoder().encode(workspace)
        defaults.set(data, forKey: Self.workspaceKeyPrefix + workspace.id.uuidString)
    }

    func removeWorkspace(_ id: UUID) {
        defaults.removeObject(forKey: Self.workspaceKeyPrefix + id.uuidString)
    }

    @discardableResult
    func restoreSession(_ data: Data) -> Bool {
        guard data.count <= Session.maximumBytes,
              let session = try? JSONDecoder().decode(Session.self, from: data),
              session.tabs.indices.contains(session.activeTab), session.tabs.allSatisfy({ state in
                  ([state.visit] + state.back + state.forward).allSatisfy { visit in
                      Self.isSessionURL(visit.folder) && visit.selection.allSatisfy(Self.isSessionURL)
                          && (visit.anchor.map(Self.isSessionURL) ?? true)
                  }
              }) else { return false }

        cancelNavigation()
        let restoredTabs = session.tabs.map { BrowserTab(id: UUID(), folder: $0.visit.folder) }
        let current = session.tabs[session.activeTab]
        tabs = restoredTabs
        currentTabID = restoredTabs[session.activeTab].id
        savedTabs = Dictionary(uniqueKeysWithValues: zip(restoredTabs, session.tabs).compactMap { tab, state in
            tab.id == currentTabID ? nil : (tab.id, state)
        })
        back = current.back
        forward = current.forward
        restore(current.visit)
        return true
    }

    private static func isSessionURL(_ url: URL) -> Bool {
        url.isFileURL && (url.host == nil || url.host == "" || url.host == "localhost")
            && url.path.hasPrefix("/") && !url.path.utf8.contains(0)
    }

    func selectAll() {
        selected = Set(entries.map(\.id))
    }

    func setSort(_ value: FileSort) {
        sort = value
    }

    func submitSearch(query explicitQuery: FileQuery? = nil) {
        let text = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        let extensions = Set(searchExtensions.split { $0 == "," || $0.isWhitespace }.map(String.init))
        let lower = searchMinimumBytes ?? 0
        let upper = searchMaximumBytes ?? Int64.max
        guard lower >= 0, upper >= lower else {
            status = "Enter a valid size range."
            return
        }
        let size: ClosedRange<Int64>? = searchMinimumBytes == nil && searchMaximumBytes == nil ? nil : lower...upper
        if explicitQuery == nil && text.isEmpty && extensions.isEmpty && size == nil && searchModified == nil {
            clearSearch()
            return
        }
        let contents = searchContent && explicitQuery == nil
        if contents {
            guard let content = TextContentQuery(phrase: text) else {
                status = "Enter text to search in file contents."
                return
            }
            contentQuery = content
        } else {
            contentQuery = nil
        }
        let name: FileQuery.Name? = text.isEmpty || contents ? nil : (
            text.contains("*") || text.contains("?") ? .wildcard(text) : .contains(text)
        )
        query = explicitQuery ?? FileQuery(name: name, extensions: extensions, size: size, modified: searchModified)
        searchActive = true
        scrollAnchor = nil
        load()
    }

    func clearSearch() {
        let wasSearching = searchActive
        setSearch(SearchState(recursive: searchRecursively, contentMode: searchContent))
        if wasSearching { reloadPreservingSelection() }
    }

    func stopSearch() {
        guard busy, searchActive else { return }
        scanTask?.cancel()
        scanID = nil
        busy = false
        status = "Search stopped. \(entries.count) results so far."
    }

    func addFavorite(_ url: URL) {
        guard url.isFileURL else { return }
        let location = url.standardizedFileURL
        favorites = changePreferences { preferences in
            if !preferences.favorites.contains(where: { $0.path == location.path }) {
                preferences.favorites.append(location)
            }
        }.favorites
    }

    func removeFavorite(_ url: URL) {
        favorites = changePreferences { $0.favorites.removeAll { $0.path == url.path } }.favorites
    }

    func moveFavorite(from offsets: IndexSet, to destination: Int) {
        guard (0...favorites.count).contains(destination),
              offsets.allSatisfy(favorites.indices.contains) else { return }
        favorites = changePreferences { preferences in
            guard preferences.favorites == favorites else { return }
            let moving = offsets.map { favorites[$0] }
            var remaining = favorites.enumerated().filter { !offsets.contains($0.offset) }.map(\.element)
            let adjusted = destination - offsets.filter { $0 < destination }.count
            remaining.insert(contentsOf: moving, at: adjusted)
            preferences.favorites = remaining
        }.favorites
    }

    func setFolderViewOverride(_ enabled: Bool) {
        guard enabled != remembersFolderView else { return }
        changePreferences { preferences in
            if enabled { preferences.folders[folder.path] = currentView }
            else { preferences.folders.removeValue(forKey: folder.path) }
        }
        applyView(for: folder)
        reloadPreservingSelection()
    }

    func refreshVolumes() {
        volumeTask?.cancel()
        volumeTask = Task { [weak self] in
            let worker = Task.detached(priority: .utility) { try VolumeInfo.mounted() }
            do {
                let result = try await withTaskCancellationHandler {
                    try await worker.value
                } onCancel: { worker.cancel() }
                guard !Task.isCancelled, let self else { return }
                self.volumes = result.volumes.sorted {
                    $0.name.localizedStandardCompare($1.name) == .orderedAscending
                }
                self.volumeStatus = result.issues.isEmpty ? "" : "\(result.issues.count) volumes unavailable."
            } catch {
                guard !Task.isCancelled, let self else { return }
                self.volumeStatus = error.localizedDescription
            }
        }
    }

    nonisolated static func folderChildren(
        of folder: URL, showHidden: Bool = false, revealing child: String? = nil
    ) async throws -> [FileEntry] {
        let worker = Task.detached(priority: .utility) {
            let entries = try readChildFolders(folder, showHidden: showHidden || child != nil)
            guard !showHidden, let child else { return entries }
            return entries.filter { !$0.isHidden || $0.name == child }
        }
        return try await withTaskCancellationHandler {
            try await worker.value
        } onCancel: { worker.cancel() }
    }

    nonisolated static func addressSuggestions(
        _ draft: String, relativeTo folder: URL, showHidden: Bool = false, limit: Int = 12
    ) async throws -> [URL] {
        guard limit > 0, !draft.isEmpty else { return [] }
        let worker = Task.detached(priority: .utility) {
            if draft == "~" {
                return [try DirectoryReader.address(draft, relativeTo: folder)]
            }
            let base: URL
            let fragment: String
            if let slash = draft.lastIndex(of: "/") {
                let prefix = String(draft[...slash])
                base = try DirectoryReader.address(prefix, relativeTo: folder)
                fragment = String(draft[draft.index(after: slash)...])
            } else {
                base = folder
                fragment = draft
            }
            let children = try readChildFolders(base, showHidden: showHidden || fragment.hasPrefix("."))
            return Array(children.lazy.filter {
                fragment.isEmpty || $0.name.range(of: fragment, options: [.anchored, .caseInsensitive]) != nil
            }.prefix(limit).map(\.url))
        }
        return try await withTaskCancellationHandler {
            try await worker.value
        } onCancel: { worker.cancel() }
    }

    private nonisolated static func readChildFolders(_ folder: URL, showHidden: Bool) throws -> [FileEntry] {
        var folders: [FileEntry] = []
        _ = try DirectoryReader.scan(folder, showHidden: showHidden, isCancelled: { Task.isCancelled }) {
            folders.append(contentsOf: $0.filter { $0.kind == .directory })
        }
        return folders.sorted(by: FileSort().precedes)
    }

    private func navigate(_ direction: Navigation, resolve: @escaping @Sendable () throws -> URL) {
        cancelNavigation()
        let id = UUID()
        navigationID = id
        navigationTask = Task { [weak self] in
            let worker = Task.detached(priority: .userInitiated) { try resolve() }
            do {
                let url = try await withTaskCancellationHandler {
                    try await worker.value
                } onCancel: { worker.cancel() }
                guard !Task.isCancelled, let self, self.navigationID == id else { return }
                self.navigationID = nil
                self.completeNavigation(direction, to: url)
            } catch {
                guard !Task.isCancelled, let self, self.navigationID == id else { return }
                self.navigationID = nil
                self.status = error.localizedDescription
            }
        }
    }

    private func completeNavigation(_ direction: Navigation, to url: URL) {
        let leaving = captureVisit()
        switch direction {
        case .open:
            if url.path == folder.path {
                reloadPreservingSelection()
                return
            }
            back.append(leaving)
            forward.removeAll()
            restore(Visit(folder: url, search: SearchState(recursive: searchRecursively)))
        case .reveal(let name):
            if url.path != folder.path || searchActive {
                back.append(leaving)
                forward.removeAll()
            }
            let item = url.appendingPathComponent(name).standardizedFileURL
            restore(Visit(folder: url, selection: [item], anchor: item,
                          search: SearchState(recursive: searchRecursively)))
        case .back:
            guard let target = back.popLast() else { return }
            forward.append(leaving)
            restore(Visit(folder: url, selection: target.selection, anchor: target.anchor, search: target.search))
        case .forward:
            guard let target = forward.popLast() else { return }
            back.append(leaving)
            restore(Visit(folder: url, selection: target.selection, anchor: target.anchor, search: target.search))
        }
    }

    private func cancelNavigation() {
        navigationID = nil
        navigationTask?.cancel()
    }

    private func captureVisit() -> Visit {
        Visit(folder: folder, selection: selected.union(selectionToRestore), anchor: scrollAnchor, search: SearchState(
            text: searchText, extensions: searchExtensions, minimumBytes: searchMinimumBytes,
            maximumBytes: searchMaximumBytes, modified: searchModified, recursive: searchRecursively,
            contentMode: searchContent, query: query, content: contentQuery
        ))
    }

    private func restore(_ visit: Visit) {
        folder = visit.folder
        if let index = tabs.firstIndex(where: { $0.id == currentTabID }) {
            tabs[index].folder = folder
        }
        canGoBack = !back.isEmpty
        canGoForward = !forward.isEmpty
        scrollAnchor = visit.anchor
        setSearch(visit.search)
        applyView(for: folder)
        load(restoring: visit.selection)
    }

    private func setSearch(_ state: SearchState) {
        restoringVisit = true
        searchText = state.text
        searchContent = state.contentMode
        searchExtensions = state.extensions
        searchMinimumBytes = state.minimumBytes
        searchMaximumBytes = state.maximumBytes
        searchModified = state.modified
        searchRecursively = state.recursive
        query = state.query
        contentQuery = state.content
        searchActive = query != nil
        restoringVisit = false
    }

    private func reloadPreservingSelection() {
        load(restoring: selected.union(selectionToRestore))
    }

    private func load(restoring selection: Set<URL> = []) {
        scanTask?.cancel()
        let id = UUID()
        scanID = id
        busy = true
        if contentQuery != nil {
            status = "Searching file contents…"
        } else {
            status = searchActive ? "Searching…" : "Reading folder…"
        }
        scanReport = nil
        currentVolume = nil
        entries = []
        setSelection([])
        selectionToRestore = selection
        let location = folder
        let ordering = sort
        let hidden = showHidden
        let scope: DirectoryReader.Scope = searchActive && searchRecursively ? .descendants : .folder
        let filter = query ?? FileQuery()
        let contents = contentQuery
        scanTask = Task { [weak self] in
            let updates = Self.scan(location, scope: scope, query: filter, content: contents, showHidden: hidden, sort: ordering)
            do {
                for try await update in updates {
                    guard !Task.isCancelled, let self, self.scanID == id else { return }
                    self.apply(update)
                }
            } catch {
                guard !Task.isCancelled, let self, self.scanID == id else { return }
                self.busy = false
                self.status = error.localizedDescription
                self.selectionToRestore.removeAll()
            }
        }
    }

    private func setSelection(_ selection: Set<URL>) {
        settingSelection = true
        selected = selection
        settingSelection = false
    }

    private func apply(_ update: ListingUpdate) {
        entries = update.entries
        if !selectionToRestore.isEmpty {
            let requested = Set(selectionToRestore.map(\.path))
            let available = entries.map(\.id).filter { requested.contains($0.path) }
            if !available.isEmpty {
                let restored = Set(available.map(\.path))
                selectionToRestore = selectionToRestore.filter { !restored.contains($0.path) }
                setSelection(selected.union(available))
            }
        }
        if let anchor = scrollAnchor, let listed = entries.first(where: { $0.id.path == anchor.path })?.id, listed != anchor {
            scrollAnchor = listed
        }
        guard let report = update.report else {
            status = "\(entries.count) \(searchActive ? "results" : "items") loaded…"
            return
        }
        scanReport = report
        currentVolume = update.volume
        selectionToRestore.removeAll()
        busy = false
        let label = searchActive ? "results" : "items"
        let unavailable = report.inaccessible == 0 ? "" : " · \(report.inaccessible) unavailable"
        let coverage = report.contentCoverage.map {
            " · \($0.searchedFiles) files read · \($0.skippedItems) items or folders unsearched"
        } ?? ""
        status = "\(report.matched) \(label)\(coverage)\(unavailable)"
    }

    private struct ListingUpdate: Sendable {
        let entries: [FileEntry]
        var report: ScanReport?
        var volume: VolumeInfo?
    }

    private nonisolated static func scan(
        _ folder: URL, scope: DirectoryReader.Scope, query: FileQuery, content: TextContentQuery?,
        showHidden: Bool, sort: FileSort
    ) -> AsyncThrowingStream<ListingUpdate, Error> {
        AsyncThrowingStream(bufferingPolicy: .bufferingNewest(1)) { continuation in
            let worker = Task.detached(priority: .userInitiated) {
                do {
                    var entries: [FileEntry] = []
                    var pending: [FileEntry] = []
                    var nextEmission: UInt64 = 0
                    func finishBatch() {
                        guard !pending.isEmpty else { return }
                        let added = pending.sorted(by: sort.precedes)
                        entries = mergeSortedEntries(entries, added, sort: sort)
                        pending.removeAll(keepingCapacity: true)
                    }
                    let report = try DirectoryReader.scan(
                        folder, scope: scope, showHidden: showHidden, query: query, content: content,
                        isCancelled: { Task.isCancelled }
                    ) { batch in
                        pending.append(contentsOf: batch)
                        let now = DispatchTime.now().uptimeNanoseconds
                        // Show the first rows promptly. Each later snapshot adds at least as
                        // many rows as the last one published to avoid re-merging a large
                        // sorted prefix for every small batch.
                        if now >= nextEmission && pending.count >= entries.count {
                            finishBatch()
                            continuation.yield(ListingUpdate(entries: entries))
                            nextEmission = DispatchTime.now().uptimeNanoseconds + 100_000_000
                        }
                    }
                    try Task.checkCancellation()
                    finishBatch()
                    let volume = try? VolumeInfo(containing: folder)
                    try Task.checkCancellation()
                    continuation.yield(ListingUpdate(entries: entries, report: report, volume: volume))
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in worker.cancel() }
        }
    }

    private nonisolated static func mergeSortedEntries(_ first: [FileEntry], _ second: [FileEntry], sort: FileSort) -> [FileEntry] {
        guard !first.isEmpty else { return second }
        var merged: [FileEntry] = []
        merged.reserveCapacity(first.count + second.count)
        var left = 0
        var right = 0
        while left < first.count && right < second.count {
            if sort.precedes(second[right], first[left]) {
                merged.append(second[right])
                right += 1
            } else {
                merged.append(first[left])
                left += 1
            }
        }
        merged.append(contentsOf: first[left...])
        merged.append(contentsOf: second[right...])
        return merged
    }

    private var currentView: ViewPreferences {
        ViewPreferences(mode: viewMode, sort: sort, iconSize: iconSize, detailsLayout: detailsLayout)
    }

    private func applyView(for location: URL) {
        restoringView = true
        let preferences = BrowserPreferences.load(from: defaults)
        let override = preferences.folders[location.path]
        let view = override ?? preferences.global
        remembersFolderView = override != nil
        sort = view.fileSort
        viewMode = view.mode
        iconSize = view.iconSize
        detailsLayout = view.detailsLayout
        showHidden = preferences.showHidden
        if favorites != preferences.favorites { favorites = preferences.favorites }
        restoringView = false
    }

    private func changeView(_ change: (inout ViewPreferences) -> Void) {
        changePreferences { preferences in
            if var override = preferences.folders[folder.path] {
                change(&override)
                preferences.folders[folder.path] = override
            } else {
                change(&preferences.global)
            }
        }
    }

    @discardableResult
    private func changePreferences(_ change: (inout BrowserPreferences) -> Void) -> BrowserPreferences {
        var preferences = BrowserPreferences.load(from: defaults)
        change(&preferences)
        preferences.save(to: defaults)
        return preferences
    }

}

private struct ViewPreferences: Codable {
    private enum Column: String, Codable {
        case name, fileExtension, type, size, modified

        init(_ column: FileSort.Column) {
            switch column {
            case .name: self = .name
            case .fileExtension: self = .fileExtension
            case .type: self = .type
            case .size: self = .size
            case .modified: self = .modified
            }
        }

        var fileColumn: FileSort.Column {
            switch self {
            case .name: return .name
            case .fileExtension: return .fileExtension
            case .type: return .type
            case .size: return .size
            case .modified: return .modified
            }
        }
    }

    var mode: BrowserViewMode
    private var column: Column
    private var ascending: Bool
    private var foldersFirst: Bool
    var iconSize: Double
    var detailsLayout: Data?

    init(mode: BrowserViewMode = .list, sort: FileSort = FileSort(), iconSize: Double = 64, detailsLayout: Data? = nil) {
        self.mode = mode
        column = Column(sort.column)
        ascending = sort.ascending
        foldersFirst = sort.foldersFirst
        self.iconSize = iconSize
        self.detailsLayout = detailsLayout
    }

    var fileSort: FileSort {
        get { FileSort(column: column.fileColumn, ascending: ascending, foldersFirst: foldersFirst) }
        set {
            column = Column(newValue.column)
            ascending = newValue.ascending
            foldersFirst = newValue.foldersFirst
        }
    }

    static func sameSort(_ left: FileSort, _ right: FileSort) -> Bool {
        Column(left.column) == Column(right.column)
            && left.ascending == right.ascending
            && left.foldersFirst == right.foldersFirst
    }
}

private struct BrowserPreferences: Codable {
    private static let key = "browser-preferences-v1"

    var global = ViewPreferences()
    var folders: [String: ViewPreferences] = [:]
    var favorites: [URL] = []
    var showHidden = false

    static func load(from defaults: UserDefaults) -> Self {
        guard let data = defaults.data(forKey: key),
              let decoded = try? JSONDecoder().decode(Self.self, from: data) else { return Self() }
        return decoded
    }

    func save(to defaults: UserDefaults) {
        guard let data = try? JSONEncoder().encode(self) else { return }
        defaults.set(data, forKey: Self.key)
    }
}
