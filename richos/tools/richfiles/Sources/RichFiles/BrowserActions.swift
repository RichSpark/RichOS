import AppKit
import Combine
import Darwin
import OSLog
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
    var reference: FileReference? { receipt.trashed }
    var canRestore: Bool { reference != nil }
}

@MainActor
final class BrowserActions: ObservableObject {
    private static let recoveryLog = Logger(subsystem: "app.richos.richfiles", category: "transfer")
    @Published private(set) var status = ""
    @Published private(set) var issues: [String] = []
    @Published private(set) var busy = false
    @Published private(set) var transfer: BrowserTransfer?
    @Published private(set) var replacingFile = false
    @Published private(set) var interruptedTransfer: InterruptedTransferNotice?
    @Published private(set) var canRetryTransfer = false
    @Published private(set) var queuedTransfers: [BrowserQueuedTransfer] = []
    @Published private(set) var queuePaused = false
    @Published private(set) var shelf: [FileReference] = []
    @Published private(set) var cutURLs: Set<URL> = []
    @Published private(set) var trashItems: [BrowserTrashItem] = []
    @Published private(set) var untrashed: [FileReference] = []
    @Published private(set) var undoName: String?
    @Published private(set) var redoName: String?
    @Published private(set) var mutationRevision = UUID()

    var didMutate: (() -> Void)?
    var operationIssues: [String] { issuesAreFromTransfer ? [] : issues }

    private let pasteboard: NSPasteboard
    private let recoveryURL: URL?
    private var clipboard: ClipboardSelection?
    private var localDrag: BrowserDragSelection?
    private var transferTask: Task<Void, Never>?
    private var preparingQueue: [UUID: Task<Void, Never>] = [:]
    private var settlingTransfer = false
    private var issuesAreFromTransfer = false
    private var retryRequest: BrowserTransferRequest?
    private var settledTransferID: UUID?
    private var unreadableRecoveryStore = false
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
                    interruptedTransfer = saved.interruptedTransfer
                    settledTransferID = saved.settledTransferID
                    updateHistoryNames()
                }
            } catch {
                unreadableRecoveryStore = true
                status = "Could not read saved recovery history."
            }
            if TransferJournal.exists(at: recoveryURL) { readInterruptedTransfer(at: recoveryURL) }
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

    func acknowledgeInterruptedTransfer() {
        guard !busy, let notice = interruptedTransfer else { return }
        interruptedTransfer = nil
        if !persistRecovery() { interruptedTransfer = notice }
    }

    private func readInterruptedTransfer(at url: URL) {
        busy = true
        status = "Checking transfer recovery…"
        let read = Task.detached(priority: .utility) { Result { try TransferJournal.interrupted(at: url) } }
        Task { [weak self] in
            let result = await read.value
            self?.finishInterruptedTransfer(result)
        }
    }

    private func finishInterruptedTransfer(_ result: Result<InterruptedTransfer?, Error>) {
        busy = false
        status = ""
        switch result {
        case .failure:
            Self.recoveryLog.warning("interrupted transfer record unreadable")
            status = "Could not read the interrupted transfer record."
        case .success(nil):
            break
        case .success(.some(let interrupted)):
            if settledTransferID == interrupted.notice.id {
                clearSettledTransfer { try interrupted.discard() }
                return
            }
            interruptedTransfer = interrupted.notice
            settledTransferID = interrupted.notice.id
            let history = moveHistory(for: interrupted.moves)
            applyTrashChanges(history.trash)
            redoHistory.removeAll()
            if !history.undo.isEmpty {
                undoHistory.append(HistoryGroup(title: "Move Before Interruption", steps: history.undo))
            }
            updateHistoryNames()
            status = "An interrupted transfer needs review."
            Self.recoveryLog.info("interrupted transfer found id=\(interrupted.notice.id.uuidString, privacy: .public) processed=\(interrupted.notice.processed) confirmed=\(interrupted.notice.succeeded) undoable=\(history.undo.count) incomplete_tail=\(interrupted.notice.incompleteTail)")
            if persistRecovery() { clearSettledTransfer { try interrupted.discard() } }
        }
    }

    private func clearSettledTransfer(_ discard: () throws -> Void) {
        do { try discard() }
        catch {
            let message = "Recovery history was saved. The transfer record could not be removed."
            issues.append(message)
            status += status.isEmpty ? message : " " + message
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

    var canScheduleTransfer: Bool { !settlingTransfer && !replacingFile && (!busy || transfer?.running == true) }

    func paste(into folder: URL, conflict: BrowserConflictPolicy = .skip, destination: BrowserTransferDestination? = nil) {
        guard canScheduleTransfer else { return }
        refreshClipboard()
        if let clipboard {
            submitTransfer(BrowserTransferRequest(
                sources: clipboard.sources.map(BrowserTransferSource.captured),
                folder: folder, move: clipboard.cut, conflict: conflict,
                clipboardGeneration: clipboard.cut ? clipboard.generation : nil, destination: destination
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
        submitTransfer(BrowserTransferRequest(
            sources: urls.map(BrowserTransferSource.clipboard), folder: folder,
            move: false, conflict: conflict, clipboardGeneration: nil, destination: destination
        ))
    }

    func transfer(
        _ sources: [FileReference], into folder: URL, move: Bool,
        conflict: BrowserConflictPolicy = .skip, destination: BrowserTransferDestination? = nil
    ) {
        submitTransfer(BrowserTransferRequest(
            sources: unique(sources).map(BrowserTransferSource.captured),
            folder: folder, move: move, conflict: conflict,
            clipboardGeneration: nil, destination: destination
        ))
    }

    var currentDrag: BrowserDragSelection? {
        guard let localDrag, !localDrag.expired else { return nil }
        return localDrag
    }

    func beginDrag(primary: FileReference, sources: [FileReference]) -> UUID {
        let selection = BrowserDragSelection(primary: primary, sources: unique(sources))
        localDrag = selection
        return selection.token
    }

    func takeDrag(_ token: UUID, matching url: URL) -> BrowserDragSelection? {
        guard let drag = currentDrag, drag.token == token, drag.primary.url.path == url.path else { return nil }
        localDrag = nil
        return drag
    }

    func addToShelf(_ sources: [FileReference]) {
        shelf = unique(sources + shelf)
        status = "\(itemCount(shelf.count)) on the file shelf."
    }

    func removeFromShelf(_ source: FileReference) {
        shelf.removeAll { $0 == source }
    }

    func clearShelf() {
        shelf.removeAll()
    }

    func transferShelf(to destination: BrowserTransferDestination, move: Bool) {
        submitTransfer(BrowserTransferRequest(
            sources: shelf.map(BrowserTransferSource.captured), folder: destination.url,
            move: move, conflict: .skip, clipboardGeneration: nil, destination: destination
        ))
    }

    func cancelTransfer() {
        guard transfer?.running == true || replacingFile else { return }
        status = "Cancelling transfer…"
        transferTask?.cancel()
    }

    func settleForHostSwitch() async throws {
        guard !busy || transferTask != nil else { throw WorkspaceHandoff.HandoffError.busy }
        settlingTransfer = true
        defer { settlingTransfer = false }
        let pendingCount = queuedTransfers.count
        for task in preparingQueue.values { task.cancel() }
        preparingQueue.removeAll()
        queuedTransfers.removeAll()
        queuePaused = false
        if let running = transferTask {
            cancelTransfer()
            await running.value
        }
        if pendingCount > 0 { status += " Stopped \(itemCount(pendingCount)) in the transfer queue." }
    }

    func removeQueuedTransfer(_ id: UUID) {
        preparingQueue.removeValue(forKey: id)?.cancel()
        queuedTransfers.removeAll { $0.id == id }
        if queuedTransfers.isEmpty { queuePaused = false }
        startNextQueuedTransfer()
    }

    func continueQueue() {
        queuePaused = false
        startNextQueuedTransfer()
    }

    private func submitTransfer(_ request: BrowserTransferRequest) {
        guard canScheduleTransfer, !request.sources.isEmpty else { return }
        if !busy && queuedTransfers.isEmpty {
            startTransfer(request)
            return
        }
        let item = BrowserQueuedTransfer(folder: request.folder, move: request.move, total: request.sources.count)
        queuedTransfers.append(item)
        status = "Transfer queued."
        preparingQueue[item.id] = Task { [weak self] in
            let worker = Task.detached(priority: .utility) { Result { try request.pinned() } }
            let result = await withTaskCancellationHandler { await worker.value } onCancel: { worker.cancel() }
            guard !Task.isCancelled, let self,
                  let index = self.queuedTransfers.firstIndex(where: { $0.id == item.id }) else { return }
            self.preparingQueue.removeValue(forKey: item.id)
            switch result {
            case .success(let ready): self.queuedTransfers[index].state = .ready(ready)
            case .failure(let error): self.queuedTransfers[index].state = .unavailable(error.localizedDescription)
            }
            self.startNextQueuedTransfer()
        }
    }

    private func startNextQueuedTransfer() {
        guard !busy, !queuePaused, !settlingTransfer, let next = queuedTransfers.first else { return }
        switch next.state {
        case .preparing: return
        case .unavailable:
            queuePaused = true
            status = "A queued transfer needs attention."
        case .ready(let request):
            queuedTransfers.removeFirst()
            startTransfer(request)
        }
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
        guard transfer?.running != true, !replacingFile else { return }
        transfer = nil
        retryRequest = nil
        canRetryTransfer = false
    }

    func replaceConflict(_ issue: BrowserTransferIssue) {
        guard let current = transfer, !current.running, current.issues.contains(where: { $0.id == issue.id }),
              let reviewed = issue.conflict?.reviewed, let destination = retryRequest?.destination else { return }
        let knownTrash = trashItems
        let source = reviewed.incoming.reference
        let task = runMutation(title: "Replace", onSuccess: { [weak self] mutation in
            if mutation.completed {
                self?.resolveConflict(issue, transferID: current.id, source: source.url,
                                      moved: current.move ? source : nil)
            } else if mutation.changed {
                self?.resolveConflict(issue, transferID: current.id, source: source.url,
                                      partialFailure: mutation.issues.first ?? mutation.message)
            }
        }, onFinish: { [weak self] in
            self?.replacingFile = false
            self?.transferTask = nil
        }) {
            try Task.checkCancellation()
            try destination.verify()
            if current.move, source.identity.device != reviewed.existing.reference.identity.device {
                do {
                    let result = try FileOperations.replaceMoveAcrossVolumes(
                        reviewed.incoming, replacing: reviewed.existing, isCancelled: { Task.isCancelled }
                    )
                    let receipt = result.receipt
                    let original = BrowserTrashItem(receipt: receipt.sourceBackup)
                    let previous = BrowserTrashItem(receipt: receipt.previousTarget)
                    let warnings = result.remainingStagingFolder.map { ["Temporary cleanup failed at \($0.path)."] } ?? []
                    return MutationResult(
                        message: "Moved \(receipt.installed.reference.url.lastPathComponent). Both originals are recoverable in Trash.",
                        issues: warnings, changed: true,
                        undo: [.undoCrossVolumeReplacement(receipt, original, previous)],
                        trashChanges: [.add(original), .add(previous)]
                    )
                } catch let partial as CrossVolumeReplacementNeedsReview {
                    return MutationResult(message: "Replacement needs review.", issues: [partial.localizedDescription],
                                          changed: true, trashChanges: changes(for: partial, known: knownTrash),
                                          completed: false)
                } catch is CancellationError {
                    return MutationResult(message: "Replacement cancelled.", completed: false)
                }
            }
            let receipt = current.move
                ? try FileOperations.replaceMove(reviewed.incoming, replacing: reviewed.existing)
                : try FileOperations.replaceCopy(reviewed.incoming, replacing: reviewed.existing,
                                                 isCancelled: { Task.isCancelled })
            let previous = BrowserTrashItem(receipt: receipt.previous)
            let undo: HistoryStep = current.move
                ? .undoMoveReplacement(reviewed.incoming.reference, receipt.installed, previous)
                : .replacement(receipt.installed, previous)
            let warnings = receipt.remainingStagingFolder.map {
                ["Temporary cleanup failed at \($0.path)."]
            } ?? []
            return MutationResult(message: "Replaced \(receipt.installed.reference.url.lastPathComponent). The previous file is in Trash.",
                                  issues: warnings, changed: true,
                                  undo: [undo], trashChanges: [.add(previous)])
        }
        if let task {
            replacingFile = true
            transferTask = task
        }
    }

    private func resolveConflict(_ issue: BrowserTransferIssue, transferID: UUID, source: URL,
                                 moved: FileReference? = nil, partialFailure: String? = nil) {
        guard var current = transfer, current.id == transferID,
              let index = current.issues.firstIndex(where: { $0.id == issue.id }) else { return }
        if let partialFailure {
            current.issues[index] = BrowserTransferIssue(name: issue.name, message: partialFailure, kind: .failed)
        } else {
            current.issues.remove(at: index)
            current.succeeded += 1
        }
        transfer = current
        if let moved { shelf.removeAll { $0 == moved } }
        if let request = retryRequest {
            let generation: Int?
            if let moved { generation = consumeCut(request.clipboardGeneration, originals: [moved]) }
            else { generation = request.clipboardGeneration }
            let remaining = request.sources.filter { $0.url.path != source.path }
            retryRequest = remaining.isEmpty ? nil : BrowserTransferRequest(
                sources: remaining, folder: request.folder, move: request.move, conflict: request.conflict,
                clipboardGeneration: generation, destination: request.destination
            )
            canRetryTransfer = retryRequest != nil
        }
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
        issuesAreFromTransfer = true
        issues = []
        let progress = BrowserTransfer(
            destination: request.folder, move: request.move, total: request.sources.count
        )
        transfer = progress
        status = progress.title + "…"
        let recoveryURL = recoveryURL
        transferTask = Task.detached(priority: .userInitiated) {
            let result = await BrowserTransferJob.perform(request, progress: progress, recoveryURL: recoveryURL) { progress in
                await self.setTransfer(progress)
            }
            await self.finishTransfer(result, request: request)
        }
    }

    private func setTransfer(_ progress: BrowserTransfer) {
        guard let previous = transfer, previous.running, previous.id == progress.id,
              previous.revision < progress.revision else { return }
        transfer = progress
    }

    private func finishTransfer(_ result: BrowserTransferResult, request: BrowserTransferRequest) {
        transferTask = nil
        transfer = result.progress
        busy = false
        let changed = result.moves.filter { $0.original.url.absoluteString != $0.moved.url.absoluteString }
        if !changed.isEmpty, !shelf.isEmpty {
            let originals = Dictionary(uniqueKeysWithValues: changed.map { ($0.original.url.path, $0.original) })
            shelf.removeAll { originals[$0.url.path] == $0 }
        }
        let generation = consumeCut(request.clipboardGeneration, originals: result.moves.map(\.original))
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
        let history = moveHistory(for: changed)
        let backupNote = history.trash.isEmpty ? "" : " The cross-volume originals are in Trash for undo."
        if result.journal != nil { settledTransferID = result.progress.id }
        let saved = finishMutation(MutationResult(
            message: lead + "\(operation) \(succeeded) of \(request.sources.count) items." + noMove + backupNote,
            issues: errors,
            changed: request.move ? !changed.isEmpty : succeeded > 0,
            undo: history.undo, trashChanges: history.trash
        ), title: "Move")
        if saved, let journal = result.journal { clearSettledTransfer { try journal.discard() } }
        queuePaused = !queuedTransfers.isEmpty && (result.progress.cancelled || !result.progress.issues.isEmpty)
        startNextQueuedTransfer()
    }

    private func consumeCut(_ generation: Int?, originals: [FileReference]) -> Int? {
        refreshClipboard()
        guard let generation, let clipboard,
              clipboard.cut, clipboard.generation == generation else { return nil }
        guard !originals.isEmpty else { return generation }
        let moved = Set(originals.map { $0.url.path })
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

    func create(in folder: URL, named name: String, directory: Bool, expectedFolder: FileIdentity?,
                useAvailableName: Bool = false, onCreated: ((FileReference) -> Void)? = nil) {
        runMutation(title: directory ? "New Folder" : "New File", onSuccess: { mutation in
            if let item = mutation.createdItem { onCreated?(item) }
        }) {
            let newItem = try FileOperations.createItem(in: folder, named: name, directory: directory,
                                                        expectedFolder: expectedFolder, useAvailableName: useAvailableName)
            return MutationResult(
                message: "Created \(newItem.url.lastPathComponent).", changed: true,
                createdItem: newItem
            )
        }
    }

    func create(from template: NameRequest.Template, named name: String) {
        runMutation(title: "New File from Template") {
            let copy = try FileOperations.copyTemplate(
                template.file, into: template.destination.url, expectedFolder: template.destination.identity,
                named: name, isCancelled: { Task.isCancelled }
            )
            let warnings = copy.remainingStagingFolder.map { ["Temporary cleanup failed at \($0.path)."] } ?? []
            return MutationResult(message: "Created \(copy.copied.url.lastPathComponent).", issues: warnings, changed: true)
        }
    }

    func createImageCopy(_ action: ImageQuickAction, of source: FileReference, in folder: URL) {
        runMutation(title: "Save Image Copy") {
            let copy = try action.apply(to: source, in: folder)
            let warnings = copy.remainingStagingFolder.map { ["Temporary cleanup failed at \($0.path)."] } ?? []
            return MutationResult(message: "Saved \(copy.copied.url.lastPathComponent).", issues: warnings, changed: true)
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
            var failures: [FileReference] = []
            for source in sources {
                do {
                    let item = BrowserTrashItem(receipt: try FileOperations.trash(source))
                    items.append(item)
                    if !item.canRestore {
                        issues.append("\(source.url.lastPathComponent): Use Finder to restore this item from Trash.")
                    }
                } catch {
                    failures.append(source)
                    issues.append("\(source.url.lastPathComponent): \(error.localizedDescription)")
                }
            }
            return MutationResult(
                message: "Moved \(items.count) of \(sources.count) items to Trash.",
                issues: issues, changed: !items.isEmpty,
                undo: items.reversed().filter(\.canRestore).map(HistoryStep.restore),
                trashChanges: items.map(TrashChange.add), untrashed: failures
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

    func restoreSelected(_ entries: [TrashListEntry]) {
        guard !entries.isEmpty else { return }
        let knownIDs = Set(trashItems.map(\.id))
        runMutation(title: "Restore Selected") {
            var restored: [(BrowserTrashItem, FileReference)] = []
            var issues: [String] = []
            for entry in entries {
                guard let item = entry.known, knownIDs.contains(item.id), item.canRestore else {
                    let nextStep = entry.reference == nil
                        ? "Check Finder’s Trash for this item."
                        : "Use Restore To… to choose a folder."
                    issues.append("\(entry.name): Original location unavailable. \(nextStep)")
                    continue
                }
                do {
                    restored.append((item, try FileOperations.restore(item.receipt)))
                } catch {
                    issues.append("\(entry.name): \(error.localizedDescription)")
                }
            }
            return MutationResult(
                message: "Restored \(restored.count) of \(entries.count) selected items.",
                issues: issues, changed: !restored.isEmpty,
                undo: restored.reversed().map { .trash($0.1) },
                trashChanges: restored.map { .remove($0.0.id) }
            )
        }
    }

    func restoreFromTrash(_ entry: TrashListEntry, into folder: URL) {
        if let known = entry.known {
            restore(known, into: folder)
            return
        }
        guard let source = entry.reference else { return }
        runMutation(title: "Restore") {
            let moved = try FileOperations.move(source, into: folder)
            return MutationResult(message: "Restored \(moved.moved.url.lastPathComponent).", changed: true,
                                  undo: [.undoMove(moved, canRedoRename: false)])
        }
    }

    func deletePermanently(_ confirmed: [FileReference], trash: [BrowserTrashItem] = [], afterTrashFailure: Bool = false) {
        let sources = unique(confirmed)
        let previousFailures = untrashed
        guard !sources.isEmpty else { return }
        runMutation(title: "Delete Permanently") {
            var removed: [FileReference] = []
            var changes: [TrashChange] = []
            var issues: [String] = []
            for source in sources {
                do {
                    let remaining = try FileOperations.deletePermanently(source)
                    removed.append(source)
                    for item in trash where item.reference == source { changes.append(.remove(item.id)) }
                    if let remaining { issues.append("Temporary cleanup failed at \(remaining.path).") }
                } catch {
                    issues.append("\(source.url.lastPathComponent): \(error.localizedDescription)")
                }
            }
            let deleted = Dictionary(uniqueKeysWithValues: removed.map { ($0.url.path, $0) })
            let remaining = afterTrashFailure ? previousFailures.filter { deleted[$0.url.path] != $0 } : nil
            return MutationResult(message: "Permanently deleted \(removed.count) of \(sources.count) items.",
                                  issues: issues, changed: !removed.isEmpty, trashChanges: changes, untrashed: remaining)
        }
    }

    func previewBatch(
        _ sources: [FileReference], in folder: URL, rules: [BatchRenameRule]
    ) async throws -> BatchRenamePlan {
        let work = Task.detached(priority: .userInitiated) {
            try BatchRenamePlan.preview(sources, in: folder, rules: rules)
        }
        return try await withTaskCancellationHandler { try await work.value } onCancel: { work.cancel() }
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

    func applySync(_ plan: FolderSyncPlan, resolving mergeIssue: BrowserTransferIssue? = nil) -> Task<Void, Never>? {
        let activeTransfer = transfer?.id
        if let mergeIssue {
            guard let transfer, !transfer.running, transfer.issues.contains(where: { $0.id == mergeIssue.id }) else {
                return nil
            }
        }
        let operation = mergeIssue == nil ? "Sync" : "Merge"
        return runMutation(title: "Folder " + operation, onSuccess: { [weak self] mutation in
            guard let issue = mergeIssue, let activeTransfer, let source = issue.conflict?.merge?.incoming.url,
                  mutation.completed else { return }
            self?.resolveConflict(issue, transferID: activeTransfer, source: source)
        }) {
            let result = try plan.apply(isCancelled: { Task.isCancelled })
            var undo: [HistoryStep] = []
            var changes: [TrashChange] = []
            var issues = result.problem.map { [$0] } ?? []
            for action in result.changes {
                switch action {
                case .copied(let copy):
                    undo.append(.trash(copy.copied))
                    if let stage = copy.remainingStagingFolder { issues.append("Temporary cleanup failed at \(stage.path).") }
                case .createdFolder(let folder):
                    undo.append(.trashEmpty(folder))
                case .replaced(let receipt):
                    let previous = BrowserTrashItem(receipt: receipt.previous)
                    changes.append(.add(previous))
                    undo.append(.replacement(receipt.installed, previous))
                    if let stage = receipt.remainingStagingFolder { issues.append("Temporary cleanup failed at \(stage.path).") }
                case .trashed(let receipt, let emptyFolder):
                    let item = BrowserTrashItem(receipt: receipt)
                    changes.append(.add(item))
                    if item.canRestore { undo.append(emptyFolder ? .restoreEmpty(item) : .restore(item)) }
                    else { issues.append("Moved to Trash. Open macOS Trash to find \(item.originalURL.path).") }
                }
            }
            let complete = !result.cancelled && result.problem == nil && result.changes.count == plan.changeCount
            let outcome = result.cancelled ? "stopped" : complete ? "finished" : "paused on an issue"
            return MutationResult(message: "\(operation) \(outcome). Applied \(result.changes.count) of \(plan.changeCount) changes.",
                                  issues: issues, changed: !result.changes.isEmpty,
                                  undo: Array(undo.reversed()), trashChanges: changes, completed: complete)
        }
    }

    func undo() { replay(undo: true) }
    func redo() { replay(undo: false) }

    @discardableResult
    private func runMutation(
        title: String, onSuccess: ((MutationResult) -> Void)? = nil, onFinish: (() -> Void)? = nil,
        _ operation: @escaping @Sendable () throws -> MutationResult
    ) -> Task<Void, Never>? {
        guard !busy else { return nil }
        busy = true
        issuesAreFromTransfer = false
        issues = []
        status = title + "…"
        let worker = Task.detached(priority: .userInitiated) { Result { try operation() } }
        return Task {
            let result = await withTaskCancellationHandler { await worker.value } onCancel: { worker.cancel() }
            busy = false
            switch result {
            case .success(let mutation):
                onSuccess?(mutation)
                finishMutation(mutation, title: title)
            case .failure(let error) where error is CancellationError:
                status = "Operation cancelled."
                notifyMutation()
            case .failure(let error):
                status = error.localizedDescription
                issues = [status]
                notifyMutation()
            }
            onFinish?()
            startNextQueuedTransfer()
        }
    }

    @discardableResult
    private func finishMutation(_ mutation: MutationResult, title: String) -> Bool {
        applyTrashChanges(mutation.trashChanges)
        if let untrashed = mutation.untrashed { self.untrashed = untrashed }
        if mutation.changed {
            redoHistory.removeAll()
            let restoredIDs = Set(mutation.trashChanges.compactMap { change -> UUID? in
                if case .remove(let id) = change { return id }
                return nil
            })
            if !restoredIDs.isEmpty {
                undoHistory = undoHistory.compactMap { group in
                    let remaining = group.steps.filter { step in
                        switch step {
                        case .restore(let item), .restoreEmpty(let item), .replacement(_, let item), .crossVolumeMove(_, let item):
                            return !restoredIDs.contains(item.id)
                        case .undoCrossVolumeReplacement(_, let source, let previous):
                            return !restoredIDs.contains(source.id) && !restoredIDs.contains(previous.id)
                        case .redoCrossVolumeReplacement(_, let archived):
                            return !restoredIDs.contains(archived.id)
                        default: return true
                        }
                    }
                    return remaining.isEmpty ? nil : HistoryGroup(title: group.title, steps: remaining)
                }
            }
            if !mutation.undo.isEmpty {
                undoHistory.append(HistoryGroup(title: title, steps: mutation.undo))
            }
            updateHistoryNames()
        }
        issues = mutation.issues
        status = mutation.message
        if let issue = mutation.issues.first { status += " " + issue }
        return notifyMutation()
    }

    private func replay(undo: Bool) {
        guard !busy, let group = undo ? undoHistory.last : redoHistory.last else { return }
        busy = true
        issuesAreFromTransfer = false
        issues = []
        status = "\(undo ? "Undoing" : "Redoing") \(group.title)…"
        let knownTrash = trashItems
        Task {
            let result = await Task.detached(priority: .userInitiated) { group.perform(knownTrash: knownTrash) }.value
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

    @discardableResult
    private func notifyMutation() -> Bool {
        let saved = persistRecovery()
        mutationRevision = UUID()
        didMutate?()
        return saved
    }

    private func persistRecovery() -> Bool {
        guard let recoveryURL else { return true }
        do {
            if unreadableRecoveryStore {
                let unreadable = try FileReference(url: recoveryURL)
                let name = recoveryURL.lastPathComponent + ".unreadable-" + UUID().uuidString
                _ = try FileOperations.move(unreadable, into: recoveryURL.deletingLastPathComponent(), named: name)
                unreadableRecoveryStore = false
            }
            try RecoveryState(trash: trashItems, undo: undoHistory, redo: redoHistory,
                              interruptedTransfer: interruptedTransfer, settledTransferID: settledTransferID)
                .save(to: recoveryURL)
            return true
        } catch {
            let message = "Could not save recovery history for the next launch."
            issues.append(message)
            status += status.isEmpty ? message : " " + message
            return false
        }
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

private func moveHistory(for moves: [BrowserCompletedMove]) -> (undo: [HistoryStep], trash: [TrashChange]) {
    var trash: [TrashChange] = []
    let undo = moves.reversed().map { move -> HistoryStep in
        switch move {
        case .sameVolume(let receipt):
            return .undoMove(receipt, canRedoRename: false)
        case .crossVolume(let receipt):
            let item = BrowserTrashItem(receipt: receipt.backup)
            trash.append(.add(item))
            return .crossVolumeMove(receipt, item)
        }
    }
    return (undo, trash)
}

private func itemCount(_ count: Int) -> String { count == 1 ? "1 item" : "\(count) items" }

private struct MutationResult: Sendable {
    let message: String
    var issues: [String] = []
    var changed = false
    var undo: [HistoryStep] = []
    var trashChanges: [TrashChange] = []
    var createdItem: FileReference?
    var untrashed: [FileReference]?
    var completed = true
}

private enum TrashChange: Sendable {
    case add(BrowserTrashItem)
    case remove(UUID)
}

private func changes(for partial: CrossVolumeReplacementNeedsReview, known: [BrowserTrashItem]) -> [TrashChange] {
    let restored = known.filter { item in
        guard let reference = item.reference else { return false }
        return partial.restoredTrash.contains(reference)
    }
    return restored.map { .remove($0.id) } + partial.addedTrash.map { .add(BrowserTrashItem(receipt: $0)) }
}

private enum HistoryStep: Codable, Sendable {
    // Keep the serialized fields so earlier recovery snapshots still open.
    case undoMove(MoveReceipt, canRedoRename: Bool)
    case rename(FileReference, String)
    case restore(BrowserTrashItem)
    case trash(FileReference)
    case trashEmpty(FileReference)
    case restoreEmpty(BrowserTrashItem)
    case replacement(ReviewedFile, BrowserTrashItem)
    case undoMoveReplacement(FileReference, ReviewedFile, BrowserTrashItem)
    case redoMoveReplacement(ReviewedFile, ReviewedFile)
    case crossVolumeMove(CrossVolumeMoveReceipt, BrowserTrashItem)
    case undoCrossVolumeReplacement(CrossVolumeReplacementReceipt, BrowserTrashItem, BrowserTrashItem)
    case redoCrossVolumeReplacement(ReversedCrossVolumeReplacementReceipt, BrowserTrashItem)

    func perform() throws -> (inverse: HistoryStep?, trashChanges: [TrashChange]) {
        switch self {
        case .undoMove(let receipt, let canRedoRename):
            let inverse = try FileOperations.reverse(receipt)
            return (.undoMove(inverse, canRedoRename: canRedoRename), [])
        case .rename(let source, let name):
            let receipt = try FileOperations.rename(source, to: name)
            return (.undoMove(receipt, canRedoRename: true), [])
        case .restore(let item):
            let restored = try FileOperations.restore(item.receipt)
            return (.trash(restored), [.remove(item.id)])
        case .trash(let source):
            let item = BrowserTrashItem(receipt: try FileOperations.trash(source))
            return (item.canRestore ? .restore(item) : nil, [.add(item)])
        case .trashEmpty(let source):
            let item = BrowserTrashItem(receipt: try FileOperations.trashEmptyFolder(source))
            return (item.canRestore ? .restoreEmpty(item) : nil, [.add(item)])
        case .restoreEmpty(let item):
            let restored = try FileOperations.restore(item.receipt)
            return (.trashEmpty(restored), [.remove(item.id)])
        case .replacement(let installed, let displaced):
            let restored = try FileOperations.restoreReplacedFile(displaced.receipt, replacing: installed)
            let previous = BrowserTrashItem(receipt: restored.previous)
            return (.replacement(restored.installed, previous), [.remove(displaced.id), .add(previous)])
        case .undoMoveReplacement(let original, let installed, let previous):
            let restored = try FileOperations.undoMoveReplacement(original, installed: installed, previous: previous.receipt)
            return (.redoMoveReplacement(restored.source, restored.destination), [.remove(previous.id)])
        case .redoMoveReplacement(let source, let destination):
            let receipt = try FileOperations.replaceMove(source, replacing: destination)
            let previous = BrowserTrashItem(receipt: receipt.previous)
            return (.undoMoveReplacement(source.reference, receipt.installed, previous), [.add(previous)])
        case .crossVolumeMove(let move, let archived):
            let reversed = try FileOperations.reverse(move)
            let previous = BrowserTrashItem(receipt: reversed.backup)
            return (.crossVolumeMove(reversed, previous), [.remove(archived.id), .add(previous)])
        case .undoCrossVolumeReplacement(let receipt, let original, let previous):
            let reversed = try FileOperations.reverse(receipt)
            let archived = BrowserTrashItem(receipt: reversed.archivedCopy)
            return (.redoCrossVolumeReplacement(reversed, archived),
                    [.remove(original.id), .remove(previous.id), .add(archived)])
        case .redoCrossVolumeReplacement(let receipt, let archived):
            let redone = try FileOperations.reverse(receipt)
            let original = BrowserTrashItem(receipt: redone.sourceBackup)
            let previous = BrowserTrashItem(receipt: redone.previousTarget)
            return (.undoCrossVolumeReplacement(redone, original, previous),
                    [.remove(archived.id), .add(original), .add(previous)])
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

    func perform(knownTrash: [BrowserTrashItem]) -> Result {
        var result = Result()
        for (index, step) in steps.enumerated() {
            do {
                let applied = try step.perform()
                if let inverse = applied.inverse { result.inverse.append(inverse) }
                result.trashChanges.append(contentsOf: applied.trashChanges)
                result.completed += 1
            } catch let partial as CrossVolumeReplacementNeedsReview {
                result.issue = partial.localizedDescription
                result.trashChanges.append(contentsOf: changes(for: partial, known: knownTrash))
                result.remaining = Array(steps.dropFirst(index + 1))
                break
            } catch {
                result.issue = error.localizedDescription
                switch error as? FileOperationError {
                case .moveNeedsReview, .replacementNeedsReview, .crossVolumeMoveNeedsReview:
                    result.remaining = Array(steps.dropFirst(index + 1))
                default:
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
    let interruptedTransfer: InterruptedTransferNotice?
    let settledTransferID: UUID?

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
        let parent = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true,
                                                attributes: [.posixPermissions: 0o700])
        var info = stat()
        guard lstat(parent.path, &info) == 0, info.st_mode & S_IFMT == S_IFDIR,
              info.st_uid == getuid(), info.st_mode & 0o077 == 0 else {
            throw CocoaError(.fileWriteNoPermission)
        }
        let encoder = JSONEncoder()
        for budget in [512, 256, 128, 64, 32, 16, 8, 0] {
            let recent = RecoveryState(trash: Array(trash.prefix(min(2_000, 4 * budget))),
                                       undo: Self.recentGroups(undo, stepBudget: budget),
                                       redo: Self.recentGroups(redo, stepBudget: budget),
                                       interruptedTransfer: interruptedTransfer, settledTransferID: settledTransferID)
            let bytes = try encoder.encode(recent)
            if bytes.count <= Self.maxBytes {
                try bytes.write(to: url, options: .atomic)
                return
            }
        }
        throw CocoaError(.fileWriteOutOfSpace)
    }

    private static func recentGroups(_ groups: [HistoryGroup], stepBudget: Int) -> [HistoryGroup] {
        var remaining = stepBudget
        var result: [HistoryGroup] = []
        for group in groups.suffix(40).reversed() {
            guard remaining > 0 else { break }
            let steps = Array(group.steps.prefix(remaining))
            result.append(HistoryGroup(title: group.title, steps: steps))
            remaining -= steps.count
        }
        return result.reversed()
    }
}
