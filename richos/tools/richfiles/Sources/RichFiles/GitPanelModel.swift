import AppKit
import Foundation
import RichFilesCore

public struct GitPanelContext: Codable, Sendable, Equatable {
    public struct Repository: Codable, Sendable, Equatable {
        let path: String
        let lands: [GitLandEvidence]
        let device: UInt64?
        let inode: UInt64?
    }

    public let threadId: String
    let chatName: String
    let git: String?
    let repositories: [Repository]
    let selectedPath: String?
}

public struct GitPanelSnapshot: Codable, Sendable, Equatable {
    public let threadId: String?
    let chatName: String
    let capturedAt: Date
    let repository: FileReference
    let administration: FileReference
    let lands: [GitLandEvidence]?
    let allOnMain: Bool
    let commit: String?
    let path: String?
}

@MainActor
public final class GitPanelModel: ObservableObject {
    enum Scope: Equatable { case chat, all }

    @Published public private(set) var open = false
    @Published private(set) var repository: GitRepository?
    @Published private(set) var loading = false
    @Published private(set) var sectionMessage: String?
    @Published public private(set) var directComparisonUnavailable = false
    @Published private(set) var commits: [GitCommit] = []
    @Published private(set) var comparison: GitComparison?
    @Published private(set) var commit: GitCommit?
    @Published private(set) var selectedFile: GitChangedFile?
    @Published private(set) var hunks: [GitDiffHunk] = []
    @Published private(set) var images: [NSImage?] = []
    @Published private(set) var fileMessage: String?
    @Published private(set) var fileLoading = false
    @Published private(set) var detailMessage: String?
    @Published private(set) var live: FileReference?
    @Published private(set) var editorName: String?
    @Published private(set) var checkout: String?
    @Published private(set) var hasMore = false
    @Published private(set) var shallow = false
    @Published private(set) var offMain = 0
    @Published private(set) var scope: Scope = .all
    @Published private(set) var selectedRepository = ""
    @Published private(set) var snapshot: GitPanelSnapshot?
    @Published private(set) var snapshotUnavailable = false
    @Published private(set) var alternateLands: [GitCommit] = []
    @Published private(set) var aboveLost = false
    @Published private(set) var repositoryCheckout: String?

    private(set) var context: GitPanelContext?
    private var folder: URL?
    private var tip: String?
    private var maximum = 160
    private var verifiedLands: [String: GitLandEvidence] = [:]
    private var historyTask: Task<Void, Never>?
    private var detailTask: Task<Void, Never>?
    private var fileTask: Task<Void, Never>?
    private var historyRevision = UUID()
    private var detailRevision = UUID()
    private var fileRevision = UUID()

    public init() {}

    var repositories: [GitPanelContext.Repository] { context?.repositories ?? [] }
    var isOutput: Bool { context != nil }
    var sourceChat: Bool { isOutput || snapshot != nil || snapshotUnavailable }

    // The native rail uses the Rust runtime resolver. The same installed Git is
    // also available to RichFiles when it is launched directly.
    static func localGit() -> URL? {
        let support = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/RichOS")
        let candidates = [
            support.appendingPathComponent("engine/runtime/bin/git").path,
            "/opt/homebrew/bin/git", "/usr/local/bin/git",
            "/Library/Developer/CommandLineTools/usr/bin/git",
            "/Applications/Xcode.app/Contents/Developer/usr/bin/git"
        ]
        return candidates.first(where: FileManager.default.isExecutableFile(atPath:)).map { URL(fileURLWithPath: $0) }
    }

    public func beginOutput(_ newContext: GitPanelContext, compare: Bool, refresh: Bool = false) {
        let newChat = context?.threadId != newContext.threadId
        let wasOpen = open
        context = newContext
        directComparisonUnavailable = false
        open = true
        if newChat || !wasOpen {
            scope = .chat
            selectedRepository = ""
        }
        let path = newContext.selectedPath
        if compare, let path, let repo = newContext.repositories.first(where: { Self.relative(path, in: $0.path) != nil }) {
            scope = .chat
            selectedRepository = repo.path
        }
        if !newContext.repositories.contains(where: { $0.path == selectedRepository }) {
            selectedRepository = newContext.repositories.count == 1 ? newContext.repositories[0].path : ""
        }
        if !newChat && wasOpen && !compare && !refresh && repository?.root.path == selectedRepository { return }
        loadHistory(newTip: true, preferredCommit: refresh ? commit?.id : nil,
                    preferredFile: compare ? path : refresh ? selectedFile?.path : nil, direct: compare)
    }

    func follow(_ newFolder: URL) {
        guard context == nil, folder?.path != newFolder.path else { return }
        folder = newFolder
        guard open else { return }
        if let held = repository, Self.relative(newFolder.path, in: held.root.path) != nil,
           !FileManager.default.fileExists(atPath: newFolder.appendingPathComponent(".git").path) {
            // A descendant can itself be inside a nested repo. Resolve it off
            // the main thread and keep the old selection only for the same root.
            checkFolder(newFolder, preserving: held)
        } else {
            snapshot = nil
            snapshotUnavailable = false
            scope = .all
            loadHistory(newTip: true)
        }
    }

    private func checkFolder(_ newFolder: URL, preserving held: GitRepository) {
        let candidate = Self.localGit()
        Task {
            let resolved = await Task.detached(priority: .userInitiated) {
                candidate.flatMap { try? GitRepository(folder: newFolder, git: $0) }
            }.value
            guard self.folder == newFolder, open, repository?.identity == held.identity else { return }
            if resolved?.identity != held.identity {
                snapshot = nil
                snapshotUnavailable = false
                scope = .all
                aboveLost = true
                loadHistory(newTip: true)
            }
        }
    }

    func toggle(at folder: URL) {
        if open { close() }
        else {
            self.folder = folder
            context = nil
            snapshot = nil
            snapshotUnavailable = false
            scope = .all
            open = true
            loadHistory(newTip: true)
        }
    }

    public func close() {
        open = false
        historyRevision = UUID()
        historyTask?.cancel()
        clearDetails()
        repository = nil
        commits = []
    }

    public func startSnapshot(_ value: GitPanelSnapshot, folder: URL, currentChat: String?, docked: Bool = false) {
        context = nil
        self.folder = folder
        open = true
        snapshot = !docked || value.threadId == nil || value.threadId == currentChat ? value : nil
        snapshotUnavailable = snapshot == nil || (value.threadId != nil && value.lands == nil)
        scope = value.allOnMain && snapshot != nil ? .all : .chat
        loadHistory(newTip: true, preferredCommit: snapshot?.commit, preferredFile: snapshot?.path)
    }

    public func chatChanged(to thread: String?) {
        guard let snapshot, let source = snapshot.threadId, source != thread else { return }
        self.snapshot = nil
        snapshotUnavailable = false
        scope = .all
        if open { loadHistory(newTip: true) }
    }

    public func makeSnapshot() -> GitPanelSnapshot? {
        guard open, let repository else { return nil }
        if let context {
            let lands = verifiedLands.values.sorted(by: { ($0.at, $0.id) > ($1.at, $1.id) })
            let capacity = 200
            let size = (try? JSONEncoder().encode(lands).count) ?? Int.max
            return GitPanelSnapshot(threadId: context.threadId, chatName: context.chatName, capturedAt: Date(),
                                    repository: repository.identity, administration: repository.administration,
                                    lands: lands.count <= capacity && size <= 256 * 1024 ? lands : nil,
                                    allOnMain: scope == .all, commit: commit?.id, path: selectedFile?.path)
        }
        return GitPanelSnapshot(threadId: snapshot?.threadId, chatName: snapshot?.chatName ?? "",
                                capturedAt: snapshot?.capturedAt ?? Date(), repository: repository.identity,
                                administration: repository.administration,
                                lands: snapshot?.lands, allOnMain: scope == .all,
                                commit: commit?.id, path: selectedFile?.path)
    }

    func chooseRepository(_ path: String) {
        guard repositories.contains(where: { $0.path == path }), selectedRepository != path else { return }
        selectedRepository = path
        loadHistory(newTip: true)
    }

    func useScope(_ value: Scope, reveal selection: Bool = false) {
        guard value != scope else { return }
        let focus = selection ? commit?.id : nil
        scope = value
        loadHistory(newTip: false, preferredCommit: focus)
    }

    func refresh() {
        loadHistory(newTip: true, preferredCommit: commit?.id, preferredFile: selectedFile?.path)
    }

    func loadMore() {
        guard hasMore, !loading else { return }
        maximum = min(100_000, maximum * 2)
        loadHistory(newTip: false, nextPage: true)
    }

    private struct HistoryResult: Sendable {
        let repo: GitRepository
        let tip: String
        let shallow: Bool
        let commits: [GitCommit]
        let reachable: [String: GitLandEvidence]
        let offMain: Int
        let checkout: String?
        let hasMore: Bool
        let preferredCommit: String?
    }

    private func loadHistory(newTip: Bool, nextPage: Bool = false, preferredCommit: String? = nil,
                             preferredFile: String? = nil, direct: Bool = false) {
        historyTask?.cancel()
        historyRevision = UUID()
        let revision = historyRevision
        if !nextPage {
            clearDetails()
            commits = []
            repository = nil
            verifiedLands = [:]
            maximum = 160
            repositoryCheckout = nil
        }
        if newTip { tip = nil; hasMore = false; offMain = 0; shallow = false }
        sectionMessage = nil
        if snapshotUnavailable && scope == .chat {
            loading = false
            sectionMessage = "The chat snapshot is unavailable. Open All on main to browse this repository."
            return
        }
        let chosen = repositories.first { $0.path == selectedRepository }
        let exact = isOutput || snapshot != nil
        let root = isOutput ? chosen.map { URL(fileURLWithPath: $0.path) } : snapshot?.repository.url ?? folder
        let git = context?.git.map { URL(fileURLWithPath: $0) } ?? Self.localGit()
        guard let root else {
            loading = false
            repository = nil
            sectionMessage = repositories.isEmpty ? "This chat has no linked Git repository." : "Choose a repository."
            directComparisonUnavailable = direct
            return
        }
        guard let git else {
            loading = false
            sectionMessage = GitReadError.noGit.localizedDescription
            directComparisonUnavailable = direct
            return
        }
        loading = true
        let expected = snapshot
        let evidence = chosen?.lands ?? snapshot?.lands ?? []
        let scope = scope
        let oldTip = tip
        let limit = maximum
        let started = ProcessInfo.processInfo.systemUptime
        historyTask = Task {
            do {
                let result = try await Self.worker {
                    let repo = try GitRepository(folder: root, git: git, exactRoot: exact)
                    if let expected, repo.identity != expected.repository || repo.administration != expected.administration {
                        throw GitReadError.changed
                    }
                    if let chosen, UInt64(exactly: repo.identity.identity.device) != chosen.device ||
                       repo.identity.identity.inode != chosen.inode { throw GitReadError.changed }
                    let tip = try oldTip ?? repo.mainTip()
                    let shallow = try repo.isShallow()
                    var reachable: [String: GitLandEvidence] = [:]
                    var offMain = 0
                    if !evidence.isEmpty {
                        let verified = try repo.recordedLands(evidence, on: tip)
                        for land in evidence where verified.reachable.contains(land.id) { reachable[land.id] = land }
                        offMain = verified.offMain
                    }
                    var actual = limit
                    var commits: [GitCommit] = []
                    if scope == .all {
                        repeat {
                            commits = try repo.history(tip: tip, limit: min(actual + 1, 100_000))
                            if let preferredCommit, !commits.contains(where: { $0.id == preferredCommit }),
                               commits.count > actual, actual < 100_000 {
                                actual = min(100_000, actual * 2)
                            } else {
                                break
                            }
                        } while true
                    } else {
                        let order = reachable.values.sorted(by: { ($0.at, $0.id) > ($1.at, $1.id) }).map(\.id)
                        commits = try repo.commits(order)
                    }
                    let path = preferredFile.flatMap { $0.hasPrefix("/") ? Self.relative($0, in: repo.root.path) : $0 }
                    let chosenCommit = preferredCommit ?? path.flatMap { path in
                        commits.first { reachable[$0.id]?.paths.contains(path) == true }?.id
                    }
                    return HistoryResult(repo: repo, tip: tip, shallow: shallow,
                                         commits: scope == .all ? Array(commits.prefix(actual)) : commits,
                                         reachable: reachable, offMain: offMain, checkout: try? repo.checkoutName(),
                                         hasMore: scope == .all && commits.count > actual, preferredCommit: chosenCommit)
                }
                guard !Task.isCancelled, revision == historyRevision, open else { return }
                repository = result.repo
                tip = result.tip
                shallow = result.shallow
                commits = result.commits
                verifiedLands = result.reachable
                offMain = result.offMain
                repositoryCheckout = result.checkout
                hasMore = result.hasMore
                maximum = max(limit, commits.count)
                loading = false
                if let id = result.preferredCommit, let row = commits.first(where: { $0.id == id }) {
                    select(row, path: preferredFile, direct: direct)
                } else if preferredFile != nil {
                    sectionMessage = "There is no verified earlier version for this file."
                    directComparisonUnavailable = direct
                }
                RichFilesDiagnostics.record(.gitHistory, id: revision, outcome: .complete, started: started,
                                            total: result.commits.count, confirmed: result.commits.count)
            } catch {
                guard !Task.isCancelled, revision == historyRevision, open else { return }
                loading = false
                if !nextPage { repository = nil }
                let reason = error as? GitReadError
                if expected != nil && reason == .changed { snapshotUnavailable = true }
                sectionMessage = reason?.localizedDescription ?? GitReadError.unreadable.localizedDescription
                directComparisonUnavailable = direct
                RichFilesDiagnostics.record(.gitHistory, id: revision, outcome: .unavailable, started: started, total: 0, confirmed: 0)
            }
        }
    }

    func select(_ row: GitCommit, path: String? = nil, direct: Bool = false) {
        guard let repository else { return }
        clearDetails()
        commit = row
        let revision = detailRevision
        let land = scope == .chat ? verifiedLands[row.id] : nil
        let preferred = path.flatMap { $0.hasPrefix("/") ? Self.relative($0, in: repository.root.path) : $0 }
        if let preferred, scope == .chat {
            alternateLands = commits.filter { verifiedLands[$0.id]?.paths.contains(preferred) == true || $0.id == row.id }
        }
        detailTask = Task {
            do {
                let result = try await Self.worker { try repository.comparison(row, land: land) }
                guard !Task.isCancelled, revision == detailRevision else { return }
                comparison = result
                if let preferred {
                    if let file = result.files.first(where: { $0.path == preferred }), result.basis != .unavailable,
                       file.status != .unresolved { selectFile(file) }
                    else {
                        detailMessage = "There is no verified earlier version for this file."
                        directComparisonUnavailable = direct
                    }
                }
            } catch {
                guard !Task.isCancelled, revision == detailRevision else { return }
                detailMessage = (error as? GitReadError)?.localizedDescription ?? GitReadError.unreadable.localizedDescription
            }
        }
    }

    func selectFile(_ file: GitChangedFile) {
        guard let repository else { return }
        clearFile()
        selectedFile = file
        fileLoading = true
        let revision = fileRevision
        let started = ProcessInfo.processInfo.systemUptime
        let image = ["png", "jpg", "jpeg", "gif", "webp", "tiff", "heic"].contains(URL(fileURLWithPath: file.path).pathExtension.lowercased())
        fileTask = Task {
            let result = try? await Self.worker {
                let reference = try? repository.liveFile(file)
                let checkout = try? repository.checkoutName()
                if image {
                    return (reference, checkout, [GitDiffHunk](),
                            [try? repository.imageBytes(file, before: true), try? repository.imageBytes(file, before: false)], nil as String?)
                }
                do {
                    return (reference, checkout, try repository.hunks(file), [Data?](), nil as String?)
                } catch {
                    return (reference, checkout, [GitDiffHunk](), [Data?](),
                            (error as? GitReadError)?.localizedDescription ?? GitReadError.unreadable.localizedDescription)
                }
            }
            guard !Task.isCancelled, revision == fileRevision, let result else { return }
            fileLoading = false
            live = result.0
            checkout = result.1
            if let live {
                editorName = NSWorkspace.shared.urlForApplication(toOpen: live.url)
                    .map { FileManager.default.displayName(atPath: $0.path) }
            }
            hunks = result.2
            images = result.3.map { $0.flatMap(NSImage.init(data:)) }
            if image && !images.contains(where: { $0 != nil }) {
                fileMessage = "The historical image preview is unavailable."
            } else {
                fileMessage = result.4
            }
            RichFilesDiagnostics.record(.gitComparison, id: revision, outcome: fileMessage == nil ? .complete : .unavailable,
                                        started: started, total: hunks.count, confirmed: hunks.count)
        }
    }

    func openCurrent() {
        guard let repo = repository, let file = selectedFile, let live else { return }
        let revision = fileRevision
        let frozenCheckout = checkout
        Task {
            let stillCurrent = (try? await Self.worker {
                try repo.checkoutName() == frozenCheckout && repo.liveFile(file) == live
            }) == true
            guard revision == fileRevision else { return }
            guard stillCurrent, (try? FileReference(url: live.url)) == live,
                  NSWorkspace.shared.open(live.url) else {
                self.live = nil
                return
            }
        }
    }

    func back() -> Bool {
        if selectedFile != nil { clearFile(); return true }
        if commit != nil { clearDetails(); return true }
        return false
    }

    private func clearDetails() {
        detailRevision = UUID()
        detailTask?.cancel()
        commit = nil
        comparison = nil
        detailMessage = nil
        alternateLands = []
        clearFile()
    }

    private func clearFile() {
        fileRevision = UUID()
        fileTask?.cancel()
        selectedFile = nil
        hunks = []
        images = []
        live = nil
        checkout = nil
        editorName = nil
        fileMessage = nil
        fileLoading = false
    }

    nonisolated static func relative(_ path: String, in root: String) -> String? {
        let prefix = root.hasSuffix("/") ? root : root + "/"
        guard path.hasPrefix(prefix) else { return nil }
        let relative = String(path.dropFirst(prefix.count))
        return GitRepository.isRelativePath(relative) ? relative : nil
    }

    private static func worker<T: Sendable>(_ execute: @escaping @Sendable () throws -> T) async throws -> T {
        let task = Task.detached(priority: .userInitiated, operation: execute)
        return try await withTaskCancellationHandler(operation: { try await task.value }, onCancel: { task.cancel() })
    }
}
