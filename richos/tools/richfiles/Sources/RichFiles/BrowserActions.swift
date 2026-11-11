import AppKit
import Combine
import Darwin
import RichFilesCore

struct BrowserTrashItem: Identifiable, Codable, Sendable {
    let id: UUID
    fileprivate let receipt: TrashReceipt

    fileprivate init(receipt: TrashReceipt) {
        id = UUID()
        self.receipt = receipt
    }

    var originalURL: URL { receipt.original.url }
    var trashedAt: Date { receipt.trashedAt }
    var trashURL: URL? { receipt.trashLocation }
    var canRestore: Bool { receipt.trashed != nil }
}

@MainActor
final class BrowserActions: ObservableObject {
    @Published private(set) var status = ""
    @Published private(set) var issues: [String] = []
    @Published private(set) var busy = false
    @Published private(set) var transfer: BrowserTransfer?
    @Published private(set) var canRetryTransfer = false
    @Published private(set) var cutURLs: Set<URL> = []
    @Published private(set) var trashItems: [BrowserTrashItem] = []
    @Published private(set) var undoName: String?
    @Published private(set) var redoName: String?
    @Published private(set) var lastCreatedURL: URL?
    @Published private(set) var mutationRevision = UUID()

    var didMutate: (() -> Void)?

    private let pasteboard: NSPasteboard
    private let recoveryURL: URL?
    private var clipboard: ClipboardSelection?
    private var transferTask: Task<Void, Never>?
    private var retryRequest: BrowserTransferRequest?
    private var undoHistory: [HistoryGroup] = []
    private var redoHistory: [HistoryGroup] = []

    private struct ClipboardSelection {
        let sources: [FileReference]
        let cut: Bool
        let generation: Int
    }

    init(pasteboard: NSPasteboard? = nil, recoveryURL: URL? = nil) {
        self.pasteboard = pasteboard ?? .general
        self.recoveryURL = recoveryURL
        if let recoveryURL {
            do {
                if let saved = try RecoveryState.load(from: recoveryURL) {
                    trashItems = saved.trash
                    undoHistory = saved.undo
                    redoHistory = saved.redo
                    updateHistoryNames()
                }
            } catch {
                status = "Could not read saved recovery history."
            }
        }
    }

    static var recoveryLocation: URL? {
        try? FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                     appropriateFor: nil, create: false)
            .appendingPathComponent("app.richos.richfiles", isDirectory: true)
            .appendingPathComponent("Recovery.json")
    }

    func refreshClipboard() {
        if let clipboard, clipboard.generation != pasteboard.changeCount {
            self.clipboard = nil
            cutURLs.removeAll()
        }
    }

    func isCut(_ url: URL) -> Bool {
        cutURLs.contains { $0.path == url.path }
    }

    /// Write standard file URLs. The cut intent stays local to this app and is
    /// valid only for this pasteboard generation and these selected identities.
    func copy(_ sources: [FileReference], cut: Bool = false) {
        let sources = unique(sources)
        guard !sources.isEmpty else { return }
        let board = pasteboard
        board.clearContents()
        clipboard = nil
        cutURLs.removeAll()
        guard board.writeObjects(sources.map { $0.url as NSURL }) else {
            status = "The clipboard could not accept those files."
            return
        }
        setClipboard(sources, cut: cut, generation: board.changeCount)
        status = cut ? "Cut \(itemCount(sources.count))." : "Copied \(itemCount(sources.count)) to the clipboard."
    }

    func paste(into folder: URL, conflict: BrowserConflictPolicy = .skip) {
        guard !busy else { return }
        refreshClipboard()
        if let clipboard {
            startTransfer(BrowserTransferRequest(
                sources: clipboard.sources.map(BrowserTransferSource.captured),
                folder: folder, move: clipboard.cut, conflict: conflict,
                clipboardGeneration: clipboard.cut ? clipboard.generation : nil, destination: nil
            ))
            return
        }
        let board = pasteboard
        let objects = board.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true])
        var seen: Set<String> = []
        let urls: [URL] = (objects ?? []).compactMap { object in
            guard let object = object as? NSURL else { return nil }
            let url = object as URL
            return url.isFileURL && seen.insert(url.path).inserted ? url : nil
        }
        guard !urls.isEmpty else {
            status = "The clipboard contains no files."
            return
        }
        startTransfer(BrowserTransferRequest(
            sources: urls.map(BrowserTransferSource.clipboard), folder: folder,
            move: false, conflict: conflict, clipboardGeneration: nil, destination: nil
        ))
    }

    func transfer(
        _ sources: [FileReference], into folder: URL, move: Bool,
        conflict: BrowserConflictPolicy = .skip
    ) {
        startTransfer(BrowserTransferRequest(
            sources: unique(sources).map(BrowserTransferSource.captured),
            folder: folder, move: move, conflict: conflict,
            clipboardGeneration: nil, destination: nil
        ))
    }

    func cancelTransfer() {
        guard transfer?.running == true else { return }
        status = "Cancelling transfer…"
        transferTask?.cancel()
    }

    func retryTransfer(conflict: BrowserConflictPolicy? = nil) {
        guard let request = retryRequest, !busy else { return }
        startTransfer(BrowserTransferRequest(
            sources: request.sources, folder: request.folder, move: request.move,
            conflict: conflict ?? request.conflict,
            clipboardGeneration: request.clipboardGeneration, destination: request.destination
        ))
    }

    func dismissTransfer() {
        guard transfer?.running != true else { return }
        transfer = nil
        retryRequest = nil
        canRetryTransfer = false
    }

    private func setClipboard(_ sources: [FileReference], cut: Bool, generation: Int) {
        clipboard = ClipboardSelection(sources: sources, cut: cut, generation: generation)
        cutURLs = cut ? Set(sources.map(\.url)) : []
    }

    private func startTransfer(_ request: BrowserTransferRequest) {
        guard !request.sources.isEmpty, !busy else { return }
        busy = true
        retryRequest = nil
        canRetryTransfer = false
        issues = []
        let progress = BrowserTransfer(
            destination: request.folder, move: request.move, total: request.sources.count
        )
        transfer = progress
        status = progress.title + "…"
        transferTask = Task.detached(priority: .userInitiated) {
            let result = await BrowserTransferJob.perform(request) { progress in
                await self.setTransfer(progress)
            }
            await self.finishTransfer(result, request: request)
        }
    }

    private func setTransfer(_ progress: BrowserTransfer) {
        transfer = progress
    }

    private func finishTransfer(_ result: BrowserTransferResult, request: BrowserTransferRequest) {
        transferTask = nil
        transfer = result.progress
        busy = false
        let changed = result.moves.filter { $0.original.url.absoluteString != $0.moved.url.absoluteString }
        let generation = consumeCut(request.clipboardGeneration, moves: result.moves)
        if !result.retry.isEmpty, let destination = result.destination {
            retryRequest = BrowserTransferRequest(
                sources: result.retry, folder: request.folder, move: request.move, conflict: request.conflict,
                clipboardGeneration: generation, destination: destination
            )
            canRetryTransfer = true
        }
        let errors = result.progress.issues.map { "\($0.name): \($0.message)" }
        let succeeded = request.move ? changed.count : result.progress.succeeded
        let unchanged = result.moves.count - changed.count
        let noMove = unchanged > 0 ? " \(itemCount(unchanged)) already in the destination." : ""
        let operation = request.move ? "Moved" : "Copied"
        let lead = result.progress.cancelled ? "Transfer cancelled. " : ""
        finishMutation(MutationResult(
            message: lead + "\(operation) \(succeeded) of \(request.sources.count) items." + noMove,
            issues: errors,
            changed: request.move ? !changed.isEmpty : succeeded > 0,
            undo: changed.reversed().map { .undoMove($0, canRedoRename: false) }
        ), title: "Move")
    }

    private func consumeCut(_ generation: Int?, moves: [MoveReceipt]) -> Int? {
        refreshClipboard()
        guard let generation, let clipboard,
              clipboard.cut, clipboard.generation == generation else { return nil }
        guard !moves.isEmpty else { return generation }
        let moved = Set(moves.map { $0.original.url.path })
        let remaining = clipboard.sources.filter { !moved.contains($0.url.path) }
        let board = pasteboard
        guard board.changeCount == generation else {
            refreshClipboard()
            return nil
        }
        board.clearContents()
        self.clipboard = nil
        cutURLs.removeAll()
        guard !remaining.isEmpty, board.writeObjects(remaining.map { $0.url as NSURL }) else { return nil }
        setClipboard(remaining, cut: true, generation: board.changeCount)
        return board.changeCount
    }

    func create(in folder: URL, named name: String, directory: Bool) {
        runMutation(title: directory ? "New Folder" : "New File") {
            let newItem = directory
                ? try FileOperations.createFolder(in: folder, named: name)
                : try FileOperations.createEmptyFile(in: folder, named: name)
            return MutationResult(
                message: "Created \(newItem.url.lastPathComponent).", changed: true,
                createdURL: newItem.url
            )
        }
    }

    func rename(_ source: FileReference, to name: String) {
        runMutation(title: "Rename") {
            let receipt = try FileOperations.rename(source, to: name)
            guard receipt.original.url.absoluteString != receipt.moved.url.absoluteString else {
                return MutationResult(message: "The name is unchanged.")
            }
            return MutationResult(
                message: "Renamed to \(receipt.moved.url.lastPathComponent).", changed: true,
                undo: [.undoMove(receipt, canRedoRename: true)]
            )
        }
    }

    func trash(_ sources: [FileReference]) {
        let sources = unique(sources)
        guard !sources.isEmpty else { return }
        runMutation(title: "Move to Trash") {
            var items: [BrowserTrashItem] = []
            var issues: [String] = []
            for source in sources {
                do {
                    let item = BrowserTrashItem(receipt: try FileOperations.trash(source))
                    items.append(item)
                    if !item.canRestore {
                        issues.append("\(source.url.lastPathComponent): Use Finder to restore this item from Trash.")
                    }
                } catch {
                    issues.append("\(source.url.lastPathComponent): \(error.localizedDescription)")
                }
            }
            return MutationResult(
                message: "Moved \(items.count) of \(sources.count) items to Trash.",
                issues: issues, changed: !items.isEmpty,
                undo: items.reversed().filter(\.canRestore).map(HistoryStep.restore),
                trashChanges: items.map(TrashChange.add)
            )
        }
    }

    func restore(_ item: BrowserTrashItem, into alternateFolder: URL? = nil) {
        guard trashItems.contains(where: { $0.id == item.id }) else { return }
        runMutation(title: "Restore") {
            let restored = try FileOperations.restore(item.receipt, into: alternateFolder)
            return MutationResult(
                message: "Restored \(restored.url.lastPathComponent).", changed: true,
                undo: [.trash(restored)], trashChanges: [.remove(item.id)]
            )
        }
    }

    func previewBatch(
        _ sources: [FileReference], in folder: URL, rule: BatchRenameRule
    ) async throws -> BatchRenamePlan {
        try await Task.detached(priority: .userInitiated) {
            try BatchRenamePlan.preview(sources, in: folder, rule: rule)
        }.value
    }

    func apply(_ plan: BatchRenamePlan) {
        runMutation(title: "Batch Rename") {
            let outcomes = try plan.apply()
            var changed: [MoveReceipt] = []
            var issues: [String] = []
            for (proposal, outcome) in zip(plan.proposals, outcomes) {
                switch outcome {
                case .renamed(let receipt): changed.append(receipt)
                case .failed(let message): issues.append("\(proposal.source.url.lastPathComponent): \(message)")
                case .unchanged, .cancelled, .notAttempted: break
                }
            }
            return MutationResult(
                message: "Renamed \(itemCount(changed.count)).", issues: issues, changed: !changed.isEmpty,
                undo: changed.reversed().map { .undoMove($0, canRedoRename: true) }
            )
        }
    }

    func undo() { replay(undo: true) }
    func redo() { replay(undo: false) }

    private func runMutation(
        title: String, _ operation: @escaping @Sendable () throws -> MutationResult
    ) {
        guard !busy else { return }
        busy = true
        issues = []
        status = title + "…"
        Task {
            let result = await Task.detached(priority: .userInitiated) {
                Result { try operation() }
            }.value
            busy = false
            switch result {
            case .success(let mutation):
                finishMutation(mutation, title: title)
            case .failure(let error):
                status = error.localizedDescription
                issues = [status]
                notifyMutation()
            }
        }
    }

    private func finishMutation(_ mutation: MutationResult, title: String) {
        applyTrashChanges(mutation.trashChanges)
        if mutation.changed {
            redoHistory.removeAll()
            let restoredIDs = Set(mutation.trashChanges.compactMap { change -> UUID? in
                if case .remove(let id) = change { return id }
                return nil
            })
            if !restoredIDs.isEmpty {
                undoHistory = undoHistory.compactMap { group in
                    let remaining = group.steps.filter { step in
                        if case .restore(let item) = step { return !restoredIDs.contains(item.id) }
                        return true
                    }
                    return remaining.isEmpty ? nil : HistoryGroup(title: group.title, steps: remaining)
                }
            }
            if !mutation.undo.isEmpty {
                undoHistory.append(HistoryGroup(title: title, steps: mutation.undo))
            }
            updateHistoryNames()
        }
        lastCreatedURL = mutation.createdURL
        issues = mutation.issues
        status = mutation.message
        if let issue = mutation.issues.first { status += " " + issue }
        notifyMutation()
    }

    private func replay(undo: Bool) {
        guard !busy, let group = undo ? undoHistory.last : redoHistory.last else { return }
        busy = true
        issues = []
        status = "\(undo ? "Undoing" : "Redoing") \(group.title)…"
        Task {
            let result = await Task.detached(priority: .userInitiated) { group.perform() }.value
            busy = false
            applyTrashChanges(result.trashChanges)
            let pending = HistoryGroup(title: group.title, steps: result.remaining)
            let reverse = HistoryGroup(title: group.title, steps: result.inverse)
            if undo {
                undoHistory.removeLast()
                if !pending.steps.isEmpty { undoHistory.append(pending) }
                if !reverse.steps.isEmpty { redoHistory.append(reverse) }
            } else {
                redoHistory.removeLast()
                if !pending.steps.isEmpty { redoHistory.append(pending) }
                if !reverse.steps.isEmpty { undoHistory.append(reverse) }
            }
            updateHistoryNames()
            let verb = undo ? "Undid" : "Redid"
            status = "\(verb) \(result.completed) of \(group.steps.count) changes."
            if let issue = result.issue {
                status += " " + issue
                issues = [issue]
            }
            notifyMutation()
        }
    }

    private func notifyMutation() {
        if let recoveryURL {
            do {
                try RecoveryState(trash: Array(trashItems.prefix(2_000)),
                                  undo: Array(undoHistory.suffix(40)), redo: Array(redoHistory.suffix(40)))
                    .save(to: recoveryURL)
            } catch {
                let message = "Could not save recovery history for the next launch."
                issues.append(message)
                status += " " + message
            }
        }
        mutationRevision = UUID()
        didMutate?()
    }

    private func applyTrashChanges(_ changes: [TrashChange]) {
        for change in changes {
            switch change {
            case .add(let item): trashItems.insert(item, at: 0)
            case .remove(let id): trashItems.removeAll { $0.id == id }
            }
        }
    }

    private func updateHistoryNames() {
        undoName = undoHistory.last?.title
        redoName = redoHistory.last?.title
    }

    private func unique(_ sources: [FileReference]) -> [FileReference] {
        var seen: Set<String> = []
        return sources.filter { seen.insert($0.url.path).inserted }
    }
}

private func itemCount(_ count: Int) -> String { count == 1 ? "1 item" : "\(count) items" }

private struct MutationResult: Sendable {
    let message: String
    var issues: [String] = []
    var changed = false
    var undo: [HistoryStep] = []
    var trashChanges: [TrashChange] = []
    var createdURL: URL?
}

private enum TrashChange: Sendable {
    case add(BrowserTrashItem)
    case remove(UUID)
}

private enum HistoryStep: Codable, Sendable {
    // Keep the serialized fields so earlier recovery snapshots still open.
    case undoMove(MoveReceipt, canRedoRename: Bool)
    case rename(FileReference, String)
    case restore(BrowserTrashItem)
    case trash(FileReference)

    func perform() throws -> (inverse: HistoryStep?, trashChange: TrashChange?) {
        switch self {
        case .undoMove(let receipt, let canRedoRename):
            let inverse = try FileOperations.reverse(receipt)
            return (.undoMove(inverse, canRedoRename: canRedoRename), nil)
        case .rename(let source, let name):
            let receipt = try FileOperations.rename(source, to: name)
            return (.undoMove(receipt, canRedoRename: true), nil)
        case .restore(let item):
            let restored = try FileOperations.restore(item.receipt)
            return (.trash(restored), .remove(item.id))
        case .trash(let source):
            let item = BrowserTrashItem(receipt: try FileOperations.trash(source))
            return (item.canRestore ? .restore(item) : nil, .add(item))
        }
    }
}

private struct HistoryGroup: Codable, Sendable {
    let title: String
    let steps: [HistoryStep]

    struct Result: Sendable {
        var inverse: [HistoryStep] = []
        var remaining: [HistoryStep] = []
        var trashChanges: [TrashChange] = []
        var completed = 0
        var issue: String?
    }

    func perform() -> Result {
        var result = Result()
        for (index, step) in steps.enumerated() {
            do {
                let applied = try step.perform()
                if let inverse = applied.inverse { result.inverse.append(inverse) }
                if let change = applied.trashChange { result.trashChanges.append(change) }
                result.completed += 1
            } catch {
                result.issue = error.localizedDescription
                if let operationError = error as? FileOperationError,
                   case .moveNeedsReview = operationError {
                    result.remaining = Array(steps.dropFirst(index + 1))
                } else {
                    result.remaining = Array(steps.dropFirst(index))
                }
                break
            }
        }
        result.inverse.reverse()
        return result
    }
}

private struct RecoveryState: Codable {
    var version = 1
    let trash: [BrowserTrashItem]
    let undo: [HistoryGroup]
    let redo: [HistoryGroup]

    private static let maxBytes = 4 * 1024 * 1024

    static func load(from url: URL) throws -> RecoveryState? {
        let descriptor = Darwin.open(url.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard descriptor >= 0 else {
            if errno == ENOENT { return nil }
            throw CocoaError(.fileReadNoPermission)
        }
        let file = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        defer { try? file.close() }
        var info = stat()
        guard fstat(descriptor, &info) == 0, info.st_mode & S_IFMT == S_IFREG,
              info.st_size >= 0, info.st_size <= maxBytes else { throw CocoaError(.fileReadCorruptFile) }
        guard let bytes = try file.read(upToCount: maxBytes + 1), bytes.count <= maxBytes else {
            throw CocoaError(.fileReadCorruptFile)
        }
        let state = try JSONDecoder().decode(RecoveryState.self, from: bytes)
        guard state.version == 1 else { throw CocoaError(.fileReadCorruptFile) }
        return state
    }

    func save(to url: URL) throws {
        let bytes = try JSONEncoder().encode(self)
        guard bytes.count <= Self.maxBytes else { throw CocoaError(.fileWriteOutOfSpace) }
        let parent = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true,
                                                attributes: [.posixPermissions: 0o700])
        var info = stat()
        guard lstat(parent.path, &info) == 0, info.st_mode & S_IFMT == S_IFDIR,
              info.st_uid == getuid(), info.st_mode & 0o077 == 0 else {
            throw CocoaError(.fileWriteNoPermission)
        }
        try bytes.write(to: url, options: .atomic)
    }
}
