import AppKit
import Darwin
import RichFilesCore
import SwiftUI

@MainActor
final class DiskCleanupModel: ObservableObject {
    enum Filter: String, CaseIterable {
        case largest = "Largest here", files = "Large files", installers = "Old installers", duplicates = "Duplicates"
    }

    enum ScanState: Equatable {
        case scanning, complete, cancelled, unavailable(String)
    }

    struct Review: Identifiable {
        let candidate: CleanupCandidate
        let reviewed: ReviewedFile
        let keeper: ReviewedFile?
        var changed = false
        var id: URL { candidate.url }
    }

    struct Moved {
        let receipt: BrowserTrashItem
        let candidate: CleanupCandidate
    }

    struct Space: Sendable {
        let volume: VolumeInfo
        let identity: FileIdentity

        init(_ location: URL) throws {
            volume = try VolumeInfo(containing: location)
            // The mount root has no parent entry to pin with FileReference.
            identity = try FileEntry(url: volume.url).identity
        }
    }

    private enum ScanUpdate: Sendable {
        case progress(CleanupInventory)
        case complete(CleanupInventory)
        case unavailable(String)
    }

    @Published private(set) var folder: URL
    @Published private(set) var scan = ScanState.scanning
    @Published private(set) var inventory: CleanupInventory?
    @Published private(set) var space: Space?
    @Published private(set) var duplicateState: ScanState?
    @Published private(set) var duplicates: DuplicateFileReport?
    @Published private(set) var batch: [Review] = []
    @Published private(set) var moved: [Moved] = []
    @Published private(set) var deleted: [CleanupCandidate] = []
    @Published private(set) var trashEntries: [TrashListEntry] = []
    @Published private(set) var trashSizes: [String: Int64] = [:]
    @Published private(set) var trashStatus = ""
    @Published private(set) var observedChange: Int64?
    @Published private(set) var deletedAllocated: Int64?
    @Published private(set) var availableBeforeDeletion: Int64?
    @Published private(set) var measuringSpace = false
    @Published private(set) var preparing = false
    @Published private(set) var notice = ""
    @Published var selected: CleanupCandidate?
    @Published var filter: Filter = .largest
    @Published var minimumMB = 100
    @Published var minimumDays = 30
    @Published var reviewing = false
    @Published var inTrash = false
    @Published var trashSelection = Set<String>()
    @Published var confirmDeletion: [TrashListEntry] = []
    @Published var keepers: [String: URL] = [:]

    let origin: URL
    let actions: BrowserActions
    private var breadcrumbs: [URL] = []
    private var scanTask: Task<Void, Never>?
    private var scanID = UUID()
    private var duplicateTask: Task<Void, Never>?
    private var duplicateID = UUID()
    private var trashTask: Task<Void, Never>?
    private var preselectTrash = false
    private var queueTask: Task<Void, Never>?
    private var transactionSpace: Space?
    private var trashDevice: Int32?
    private var cache: [URL: CleanupInventory] = [:]
    private var pending: [URL: [URL]] = [:]
    private var reselect: URL?

    init(folder: URL, actions: BrowserActions) {
        origin = folder
        self.folder = folder
        self.actions = actions
    }

    var name: String {
        if folder.path == "/" || folder.pathComponents == ["/", "System", "Volumes", "Data"] {
            return space?.volume.name ?? "This Mac"
        }
        return folder.lastPathComponent
    }
    var canGoUp: Bool { !breadcrumbs.isEmpty }
    var volumeName: String { (inTrash ? transactionSpace : space)?.volume.name ?? "this volume" }
    var batchBytes: Int64 { Self.total(batch.map(\.candidate.allocated)) }
    var trashBytes: Int64 { Self.total(moved.map(\.candidate.allocated)) }
    var availableBytes: Int64? { (inTrash ? transactionSpace : space)?.volume.availableBytes }
    var selectedTrash: [TrashListEntry] {
        trashEntries.filter { trashSelection.contains($0.id) && $0.canDelete && $0.reference != nil }
    }
    var ready: Bool { !preparing && !actions.busy && !measuringSpace }

    func entries(_ filter: Filter) -> [CleanupCandidate] {
        guard let inventory else { return [] }
        let cutoff = Calendar.current.date(byAdding: .day, value: -minimumDays, to: Date()) ?? Date()
        let minimum = Int64(minimumMB) * 1_000_000
        switch filter {
        case .largest: return inventory.rows
        case .files:
            return inventory.largeFiles.filter { $0.allocated >= minimum && (minimumDays == 0 || $0.modified <= cutoff) }
        case .installers:
            return inventory.installers.filter { $0.allocated >= minimum && (minimumDays == 0 || $0.modified <= cutoff) }
        case .duplicates: return duplicates?.groups.flatMap(\.candidates) ?? []
        }
    }

    var displayed: [CleanupCandidate] { reviewing ? batch.map(\.candidate) : entries(filter) }

    func chooseFilter(_ filter: Filter) {
        self.filter = filter
        selected = nil
    }

    func chooseScope(_ url: URL) {
        breadcrumbs = []
        visit(url)
    }

    func drill(_ item: CleanupCandidate) {
        guard item.isFolder else { return }
        breadcrumbs.append(folder)
        visit(item.url)
    }

    func goUp() {
        guard let parent = breadcrumbs.popLast() else { return }
        visit(parent)
    }

    private func visit(_ url: URL) {
        guard url != folder else { return }
        folder = url
        selected = nil
        space = nil
        reviewing = false
        resetDuplicates()
        startScan()
    }

    func startScan(fresh: Bool = false) {
        scanTask?.cancel()
        let id = UUID()
        scanID = id
        let location = folder
        let prior = fresh ? nil : cache[location]
        let changed = pending[location] ?? []
        inventory = prior
        if prior != nil && changed.isEmpty {
            scan = .complete
            refreshSpace()
            return
        }
        scan = .scanning
        let started = ProcessInfo.processInfo.systemUptime
        scanTask = Task { [weak self] in
            let (updates, continuation) = AsyncStream<ScanUpdate>.makeStream(bufferingPolicy: .bufferingNewest(1))
            let worker = Task.detached(priority: .utility) {
                defer { continuation.finish() }
                do {
                    let result = try StorageAnalysis.cleanup(in: location, previous: prior, changed: changed,
                        isCancelled: { Task.isCancelled },
                        onProgress: { continuation.yield(.progress($0)) })
                    continuation.yield(.complete(result))
                } catch is CancellationError {
                    return
                } catch {
                    continuation.yield(.unavailable(error.localizedDescription))
                }
            }
            let volume = Task.detached(priority: .utility) { try? Space(location) }
            var firstRows: Int?
            var outcome = RichFilesDiagnostics.Outcome.cancelled
            var total = 0
            defer {
                RichFilesDiagnostics.record(.cleanupScan, id: id, outcome: outcome, started: started,
                                            total: total, confirmed: 0, firstRowsMS: firstRows)
            }
            await withTaskCancellationHandler {
                for await update in updates {
                    guard let self, !Task.isCancelled, self.scanID == id else { break }
                    switch update {
                    case .progress(let report):
                        self.inventory = report
                        total = report.visited
                        if firstRows == nil, !report.rows.isEmpty { firstRows = RichFilesDiagnostics.milliseconds(since: started) }
                    case .complete(let report):
                        if firstRows == nil, !report.rows.isEmpty { firstRows = RichFilesDiagnostics.milliseconds(since: started) }
                        total = report.visited
                        outcome = report.unavailable == 0 && report.otherVolumes == 0 ? .complete : .issues
                        self.inventory = report
                        if self.cache.count >= 8, self.cache[location] == nil, let oldest = self.cache.keys.first {
                            self.cache.removeValue(forKey: oldest)
                            self.pending.removeValue(forKey: oldest)
                        }
                        self.cache[location] = report
                        self.pending.removeValue(forKey: location)
                        if let url = self.reselect {
                            self.selected = report.rows.first { $0.url == url }
                            self.reselect = nil
                        }
                        self.scan = .complete
                    case .unavailable(let message):
                        outcome = .unavailable
                        self.scan = .unavailable(message)
                    }
                }
                let measured = await volume.value
                if let self, self.scanID == id, !Task.isCancelled { self.space = measured }
            } onCancel: {
                worker.cancel()
                volume.cancel()
                continuation.finish()
            }
        }
    }

    func cancelScan() {
        scanTask?.cancel()
        scanID = UUID()
        scan = .cancelled
    }

    func findDuplicates() {
        duplicateTask?.cancel()
        let id = UUID()
        duplicateID = id
        duplicateState = .scanning
        duplicates = nil
        keepers = [:]
        let location = folder
        let started = ProcessInfo.processInfo.systemUptime
        let worker = Task.detached(priority: .utility) {
            Result { try StorageAnalysis.duplicates(in: location, isCancelled: { Task.isCancelled }) }
        }
        duplicateTask = Task { [weak self] in
            let result = await withTaskCancellationHandler { await worker.value } onCancel: { worker.cancel() }
            var outcome = RichFilesDiagnostics.Outcome.cancelled
            var count = 0
            defer { RichFilesDiagnostics.record(.cleanupDuplicates, id: id, outcome: outcome, started: started,
                                                total: count, confirmed: 0) }
            guard let self, self.duplicateID == id, !Task.isCancelled else { return }
            switch result {
            case .success(let report):
                self.duplicates = report
                self.duplicateState = .complete
                count = report.groups.count
                outcome = report.unavailable == 0 && report.otherVolumes == 0 ? .complete : .issues
            case .failure(let error):
                self.duplicateState = error is CancellationError ? .cancelled : .unavailable(error.localizedDescription)
                outcome = error is CancellationError ? .cancelled : .unavailable
            }
        }
    }

    func cancelDuplicates() {
        duplicateTask?.cancel()
        duplicateID = UUID()
        duplicateState = .cancelled
    }

    private func resetDuplicates() {
        cancelDuplicates()
        duplicateState = nil
        duplicates = nil
        keepers = [:]
    }

    func keeper(_ group: DuplicateFileGroup) -> URL? {
        keepers[group.id] ?? group.candidates.first(where: { $0.canReview && !contains($0) })?.url
    }

    func keep(_ candidate: CleanupCandidate, in group: DuplicateFileGroup) {
        guard ready, candidate.canReview else { return }
        keepers[group.id] = candidate.url
        let members = Set(group.files)
        // Review every removal again against the newly chosen keeper.
        batch.removeAll { members.contains($0.id) }
    }

    func group(for candidate: CleanupCandidate) -> DuplicateFileGroup? {
        duplicates?.groups.first { $0.files.contains(candidate.url) }
    }

    func contains(_ candidate: CleanupCandidate) -> Bool { batch.contains { $0.id == candidate.url } }

    func canAdd(_ candidate: CleanupCandidate) -> Bool {
        guard candidate.canReview, ready else { return false }
        if batch.contains(where: { $0.keeper?.reference.url.path == candidate.url.path }) { return false }
        guard let group = group(for: candidate) else { return true }
        return keeper(group).map { $0 != candidate.url } ?? false
    }

    func toggle(_ candidate: CleanupCandidate) {
        if contains(candidate) { remove(candidate.url); return }
        guard canAdd(candidate) else { return }
        let group = group(for: candidate)
        let retained = group.flatMap { group in group.candidates.first { $0.url == keeper(group) } }
        preparing = true
        notice = ""
        let worker = Task.detached(priority: .userInitiated) {
            Result { Review(candidate: candidate, reviewed: try candidate.review(), keeper: try retained?.review()) }
        }
        queueTask = Task { [weak self] in
            let result = await worker.value
            guard let self, !Task.isCancelled else { return }
            self.preparing = false
            switch result {
            case .success(let entry):
                let device = self.batch.first?.reviewed.reference.identity.device ?? self.trashDevice
                guard device == nil || device == entry.reviewed.reference.identity.device else {
                    self.notice = "Finish this cleanup before adding files from another volume."
                    return
                }
                self.batch.append(entry)
                if let group, let retained { self.keepers[group.id] = retained.url }
            case .failure:
                self.notice = "This item changed or is unavailable. Scan again before reviewing it."
            }
        }
    }

    func remove(_ id: URL) {
        guard ready else { return }
        batch.removeAll { $0.id == id }
    }

    func reviewAgain(_ entry: Review) {
        remove(entry.id)
        selected = nil
        reviewing = false
        if entry.keeper != nil {
            filter = .duplicates
            findDuplicates()
            notice = "Choose the copy to keep and review the updated group."
        } else {
            filter = .largest
            let parent = entry.candidate.url.deletingLastPathComponent()
            reselect = entry.candidate.url
            if parent.path != folder.path {
                breadcrumbs.append(folder)
                folder = parent
                space = nil
                resetDuplicates()
            }
            startScan(fresh: true)
            notice = "Check the current file details before adding it again."
        }
    }

    func moveToTrash() {
        guard ready else { return }
        let next = batch.filter { !$0.changed }
        guard let first = next.first else { return }
        let queued = Set(next.map { $0.id.path })
        if duplicates?.groups.contains(where: { group in
            group.files.allSatisfy { queued.contains($0.path) }
        }) == true {
            notice = "Keep at least one copy from each duplicate group."
            filter = .duplicates
            reviewing = false
            return
        }
        preparing = true
        let parent = first.reviewed.reference.url.deletingLastPathComponent()
        let started = ProcessInfo.processInfo.systemUptime
        let read = Task.detached(priority: .utility) { try? Space(parent) }
        queueTask = Task { [weak self] in
            let before = await read.value
            guard let self, !Task.isCancelled else { return }
            self.preparing = false
            let keepers = Dictionary(uniqueKeysWithValues: next.compactMap { entry in
                entry.keeper.map { (entry.reviewed.reference.url, $0) }
            })
            let task = self.actions.trashReviewed(next.map(\.reviewed), keeping: keepers) { [weak self] items, failures in
                guard let self else { return }
                let originals = Dictionary(uniqueKeysWithValues: next.map { ($0.reviewed.reference.url, $0.candidate) })
                self.moved.append(contentsOf: items.compactMap { receipt in
                    originals[receipt.originalURL].map { Moved(receipt: receipt, candidate: $0) }
                })
                if !items.isEmpty {
                    self.trashDevice = first.reviewed.reference.identity.device
                    self.transactionSpace = before
                }
                let removed = Set(items.map(\.originalURL))
                let failed = Set(failures.map(\.url))
                let previousReview = self.batch.count
                self.batch.removeAll { removed.contains($0.reviewed.reference.url) }
                for index in self.batch.indices where failed.contains(self.batch[index].reviewed.reference.url) {
                    self.batch[index].changed = true
                }
                self.notice = items.isEmpty ? "No items moved to Trash." : "Moved \(items.count) of \(next.count) items to Trash."
                if self.batch.count != previousReview { self.refreshAffected(Array(removed)) }
                self.refreshSpace()
                RichFilesDiagnostics.record(.cleanupTrash, id: UUID(), outcome: failures.isEmpty ? .complete : .issues,
                                            started: started, total: next.count, confirmed: items.count)
            }
            if task == nil { self.notice = "Another file operation is running. Try again when it finishes." }
        }
    }

    func showTrash() {
        inTrash = true
        trashSelection = []
        loadTrash(preselect: true)
    }

    func leaveTrash() { inTrash = false; confirmDeletion = [] }

    func loadTrash(preselect: Bool = false) {
        trashTask?.cancel()
        preselectTrash = preselectTrash || preselect
        let known = actions.trashItems
        let peer = actions.peerRecoveryURL
        let device = trashDevice
        let ids = Set(moved.map(\.receipt.id))
        let parent = transactionSpace?.volume.url
        trashStatus = "Reading Trash…"
        let worker = Task.detached(priority: .utility) {
            Result {
                let index = try TrashIndex.read(known: known, peer: BrowserActions.readPeerTrash(from: peer))
                let entries = index.entries.filter {
                    if let id = $0.known?.id, ids.contains(id) { return true }
                    if let device { return $0.reference?.identity.device == device }
                    return true
                }
                let sizes = Dictionary(uniqueKeysWithValues: entries.compactMap { entry -> (String, Int64)? in
                    guard let ref = entry.reference, let bytes = Self.allocated(ref) else { return nil }
                    return (entry.id, bytes)
                })
                return (entries, sizes, index.inaccessible, parent.flatMap { try? Space($0) })
            }
        }
        trashTask = Task { [weak self] in
            let result = await withTaskCancellationHandler { await worker.value } onCancel: { worker.cancel() }
            guard let self, !Task.isCancelled else { return }
            switch result {
            case .success(let (entries, sizes, inaccessible, measured)):
                self.trashEntries = entries
                self.trashSizes = sizes
                self.trashStatus = inaccessible > 0 ? "Some Trash locations could not be read." : ""
                let eligible = entries.filter { $0.canDelete && $0.reference != nil }
                if self.preselectTrash {
                    self.trashSelection = Set(eligible.filter { $0.known.map { ids.contains($0.id) } == true }.map(\.id))
                    self.preselectTrash = false
                } else {
                    self.trashSelection.formIntersection(eligible.map(\.id))
                }
                if let measured, measured.identity == self.transactionSpace?.identity { self.transactionSpace = measured }
            case .failure(let error):
                self.trashEntries = []
                self.trashSelection = []
                self.trashStatus = error.localizedDescription
            }
        }
    }

    private nonisolated static func allocated(_ ref: FileReference) -> Int64? {
        guard (try? FileReference(url: ref.url)) == ref else { return nil }
        var status = stat()
        guard lstat(ref.url.path, &status) == 0, status.st_mode & S_IFMT == S_IFREG else { return nil }
        let (bytes, overflow) = Int64(status.st_blocks).multipliedReportingOverflow(by: 512)
        return overflow ? nil : max(0, bytes)
    }

    func restore(_ entries: [TrashListEntry]) {
        guard ready else { return }
        actions.restoreSelected(entries) { [weak self] restored in
            guard let self else { return }
            let files = self.moved.filter { restored.contains($0.receipt.id) }
            self.moved.removeAll { restored.contains($0.receipt.id) }
            self.refreshAffected(files.map(\.candidate.url))
            self.notice = self.actions.status
            if self.inTrash { self.loadTrash() }
        }
    }

    func undoMove() {
        guard ready else { return }
        let receipts = moved.compactMap { item -> TrashListEntry? in
            guard let ref = item.receipt.reference else { return nil }
            return TrashListEntry(reference: ref, known: item.receipt, dateAdded: item.receipt.trashedAt, canDelete: true)
        }
        restore(receipts)
    }

    func deleteConfirmed() {
        let entries = confirmDeletion
        confirmDeletion = []
        guard ready, !entries.isEmpty else { return }
        let refs = entries.compactMap(\.reference)
        // Confirmation captures a selection, not a live query of whichever
        // rows happen to be visible when the sheet is accepted.
        guard refs.count == entries.count, entries.allSatisfy(\.canDelete) else { return }
        let sizes = trashSizes
        let start = ProcessInfo.processInfo.systemUptime
        let volume = transactionSpace
        preparing = true
        let read = Task.detached(priority: .utility) { volume.flatMap { try? Space($0.volume.url) } }
        queueTask = Task { [weak self] in
            let before = await read.value
            guard let self, !Task.isCancelled else { return }
            self.preparing = false
            let deletion = self.actions.deletePermanently(refs, trash: entries.compactMap(\.known)) { [weak self] removed in
                guard let self else { return }
                let removed = Set(removed.map(\.url))
                let effect = entries.filter { $0.reference.map { removed.contains($0.url) } == true }
                let selectedSizes = effect.compactMap { sizes[$0.id] }
                if !effect.isEmpty {
                    self.deletedAllocated = selectedSizes.count == effect.count ? Self.total(selectedSizes) : nil
                    self.availableBeforeDeletion = before?.identity == volume?.identity ? before?.volume.availableBytes : nil
                }
                let records = Set(effect.compactMap { $0.known?.id })
                self.deleted.append(contentsOf: self.moved.filter { records.contains($0.receipt.id) }.map(\.candidate))
                self.moved.removeAll { records.contains($0.receipt.id) }
                self.notice = self.actions.status
                self.loadTrash()
                if effect.isEmpty {
                    RichFilesDiagnostics.record(.cleanupDelete, id: UUID(), outcome: .issues,
                                                started: start, total: refs.count, confirmed: 0)
                } else {
                    self.observedChange = nil
                    self.measureChange(before: before, started: start, count: refs.count, removed: effect.count)
                }
            }
            if deletion == nil { self.notice = "Another file operation is running. Try again when it finishes." }
        }
    }

    private func measureChange(before: Space?, started: TimeInterval, count: Int, removed: Int) {
        measuringSpace = true
        let worker = Task.detached(priority: .utility) { before.flatMap { try? Space($0.volume.url) } }
        Task { [weak self] in
            let after = await worker.value
            guard let self else { return }
            self.measuringSpace = false
            if let before, let after, before.identity == after.identity,
               after.identity == self.transactionSpace?.identity {
                self.transactionSpace = after
                if let first = before.volume.availableBytes, let last = after.volume.availableBytes {
                    self.observedChange = last - first
                }
            }
            RichFilesDiagnostics.record(.cleanupDelete, id: UUID(),
                outcome: removed < count ? .issues : self.observedChange == nil ? .unavailable : .complete,
                started: started, total: count, confirmed: removed)
        }
    }

    private func refreshSpace() {
        let location = folder
        let worker = Task.detached(priority: .utility) { try? Space(location) }
        Task { [weak self] in
            let measured = await worker.value
            guard let self, self.folder == location else { return }
            self.space = measured
        }
    }

    private func refreshAffected(_ urls: [URL]) {
        guard !urls.isEmpty else { return }
        for (location, report) in cache where report.affectedChildren(urls) != nil {
            pending[location, default: []].append(contentsOf: urls)
        }
        if pending[folder] != nil { startScan() }
        else if cache[folder] == nil { startScan(fresh: true) }
        resetDuplicates()
    }

    func close() {
        scanTask?.cancel()
        duplicateTask?.cancel()
        trashTask?.cancel()
        queueTask?.cancel()
    }

    static func total(_ values: [Int64]) -> Int64 {
        values.reduce(0) { sum, item in
            let result = sum.addingReportingOverflow(item)
            return result.overflow ? .max : result.partialValue
        }
    }
}
