import Darwin
import Foundation
import OSLog
import RichFilesCore

enum BrowserConflictPolicy: Equatable, Sendable {
    case skip
    case keepBoth
}

struct BrowserTransferIssue: Identifiable, Sendable {
    enum Kind: Sendable {
        case skipped
        case failed
        case warning
    }

    let id = UUID()
    let name: String
    let message: String
    let kind: Kind
    var conflict: BrowserFileConflict? = nil
}

struct BrowserFileConflict: Sendable {
    let incoming: FileEntry
    let existing: FileEntry
    let reviewed: Replacement?
    let merge: Merge?

    struct Merge: Sendable {
        let incoming: FileReference
        let existing: FileReference
    }

    struct Replacement: Sendable {
        let incoming: ReviewedFile
        let existing: ReviewedFile
    }
}

struct BrowserTransfer: Sendable {
    struct CopyProgress: Sendable {
        let source: URL?
        let written: Int64
        let total: Int64?
        let elapsed: TimeInterval

        var bytesPerSecond: Int64? {
            guard elapsed >= 1, written > 0 else { return nil }
            return Int64(exactly: (Double(written) / elapsed).rounded(.down))
        }

        var secondsRemaining: TimeInterval? {
            guard let total, let bytesPerSecond, bytesPerSecond > 0, total > written else { return nil }
            return Double(total - written) / Double(bytesPerSecond)
        }
    }

    let id = UUID()
    let destination: URL
    let move: Bool
    let total: Int
    var revision = 0
    var completed = 0
    var succeeded = 0
    var currentSource: URL?
    var copyProgress: CopyProgress?
    var verifyingCopy = false
    var issues: [BrowserTransferIssue] = []
    var running = true
    var cancelled = false

    var title: String { move ? "Moving files" : "Copying files" }
}

enum BrowserTransferSource: Sendable {
    case captured(FileReference)
    case clipboard(URL)

    var url: URL {
        switch self {
        case .captured(let reference): return reference.url
        case .clipboard(let url): return url
        }
    }

    func reference() throws -> FileReference {
        switch self {
        case .captured(let reference): return reference
        case .clipboard(let url): return try FileReference(url: url)
        }
    }
}

struct BrowserTransferRequest: Sendable {
    let sources: [BrowserTransferSource]
    let folder: URL
    let move: Bool
    let conflict: BrowserConflictPolicy
    let clipboardGeneration: Int?
    /// A retry should still apply to the same destination folder.
    let destination: BrowserTransferDestination?
}

struct BrowserQueuedTransfer: Identifiable, Sendable {
    enum State: Sendable {
        case preparing
        case ready(BrowserTransferRequest)
        case unavailable(String)
    }

    let id = UUID()
    let folder: URL
    let move: Bool
    let total: Int
    var state = State.preparing

    var title: String { move ? "Move \(total) items" : "Copy \(total) items" }
}

extension BrowserTransferRequest {
    func pinned() throws -> Self {
        // The worker checks this captured destination again when the queued transfer starts.
        let target = try destination ?? BrowserTransferDestination(folder: folder)
        let captured = try sources.map { source in
            try Task.checkCancellation()
            let selected = try source.reference()
            guard try FileReference(url: selected.url) == selected else {
                throw FileOperationError.sourceChanged(selected.url)
            }
            return BrowserTransferSource.captured(selected)
        }
        return Self(sources: captured, folder: target.url, move: move, conflict: conflict,
                    clipboardGeneration: clipboardGeneration, destination: target)
    }
}

struct BrowserDragSelection: Sendable {
    static let type = "app.richos.richfiles.local-drag"
    // Only this process can resolve the marker to its pinned selection.
    private static let pasteboardPrefix = UUID().uuidString + ":"

    let token = UUID()
    let primary: FileReference
    let sources: [FileReference]
    private let expires = ProcessInfo.processInfo.systemUptime + 180

    var expired: Bool { ProcessInfo.processInfo.systemUptime > expires }

    static func pasteboardValue(for token: UUID) -> String {
        pasteboardPrefix + token.uuidString
    }

    static func localToken(from value: String) -> UUID? {
        guard value.hasPrefix(pasteboardPrefix) else { return nil }
        return UUID(uuidString: String(value.dropFirst(pasteboardPrefix.count)))
    }

    func moves(to destination: BrowserTransferDestination, option: Bool) -> Bool {
        sources.allSatisfy { $0.identity.device == destination.device } != option
    }
}

struct BrowserTransferDestination: Sendable {
    let url: URL
    let device: Int32
    let inode: UInt64
    private let reference: FileReference?
    var identity: FileIdentity? { reference?.identity }

    init(folder: URL) throws {
        url = try DirectoryReader.directoryURL(folder)
        // The root has no named directory entry. Other destinations retain the
        // selected folder's identity while a batch waits or is retried.
        if url.path == "/" {
            reference = nil
            var status = stat()
            guard lstat(url.path, &status) == 0 else {
                throw FileOperationError.folderChanged(url)
            }
            device = status.st_dev
            inode = status.st_ino
        } else {
            let captured = try FileReference(url: url)
            reference = captured
            device = captured.identity.device
            inode = captured.identity.inode
        }
    }

    func verify() throws {
        guard let reference else { return }
        guard let current = try? FileReference(url: url), current == reference else {
            throw FileOperationError.folderChanged(url)
        }
    }
}

enum BrowserCompletedMove: Codable, Sendable {
    case sameVolume(MoveReceipt)
    case crossVolume(CrossVolumeMoveReceipt)

    var original: FileReference {
        switch self {
        case .sameVolume(let move): return move.original
        case .crossVolume(let move): return move.original
        }
    }

    var moved: FileReference {
        switch self {
        case .sameVolume(let move): return move.moved
        case .crossVolume(let move): return move.moved
        }
    }
}

struct BrowserTransferResult: Sendable {
    var progress: BrowserTransfer
    var moves: [BrowserCompletedMove] = []
    var retry: [BrowserTransferSource] = []
    var destination: BrowserTransferDestination?
    var journal: TransferJournal?
}

enum BrowserTransferJob {
    private static let logger = Logger(subsystem: "app.richos.richfiles", category: "transfer")

    static func perform(
        _ request: BrowserTransferRequest,
        progress: BrowserTransfer,
        recoveryURL: URL?,
        publish: @escaping @Sendable (BrowserTransfer) async -> Void
    ) async -> BrowserTransferResult {
        var result = BrowserTransferResult(progress: progress)
        let started = ProcessInfo.processInfo.systemUptime
        let operation = progress.move ? "move" : "copy"
        logger.info("transfer started id=\(progress.id.uuidString, privacy: .public) operation=\(operation, privacy: .public) count=\(progress.total)")
        defer {
            let milliseconds = Int((ProcessInfo.processInfo.systemUptime - started) * 1_000)
            let status = result.progress.cancelled ? "cancelled" : (result.progress.issues.isEmpty ? "complete" : "issues")
            logger.info("transfer finished id=\(progress.id.uuidString, privacy: .public) status=\(status, privacy: .public) elapsed_ms=\(milliseconds) succeeded=\(result.progress.succeeded) issues=\(result.progress.issues.count)")
            let outcome: RichFilesDiagnostics.Outcome = result.progress.cancelled ? .cancelled
                : result.progress.issues.isEmpty ? .complete : .issues
            RichFilesDiagnostics.record(progress.move ? .move : .copy, id: progress.id, outcome: outcome,
                                        started: started, total: progress.total, confirmed: result.progress.succeeded)
        }
        let destination: BrowserTransferDestination
        do {
            destination = try request.destination ?? BrowserTransferDestination(folder: request.folder)
            result.destination = destination
            try destination.verify()
            if let recoveryURL {
                do {
                    result.journal = try TransferJournal(recoveryURL: recoveryURL, id: progress.id,
                                                         destination: destination.url, move: request.move, total: progress.total)
                } catch {
                    logger.warning("recovery write refused id=\(progress.id.uuidString, privacy: .public) phase=begin")
                    let message = (error as? CocoaError)?.code == .fileWriteFileExists
                        ? "A previous transfer record is still open. Reopen RichFiles to check it before retrying."
                        : "Could not save the transfer recovery record. No files were changed."
                    result.progress.issues.append(BrowserTransferIssue(name: destination.url.lastPathComponent,
                                                                        message: message, kind: .failed))
                    result.progress.running = false
                    return result
                }
            }
        } catch {
            if result.destination != nil {
                retainUnstarted(request.sources[...], for: request.folder, in: &result)
            } else {
                result.progress.issues.append(BrowserTransferIssue(
                    name: request.folder.lastPathComponent, message: error.localizedDescription, kind: .failed
                ))
            }
            result.progress.running = false
            return result
        }

        for (index, input) in request.sources.enumerated() {
            if Task.isCancelled {
                result.progress.cancelled = true
                result.retry.append(contentsOf: request.sources[index...])
                break
            }
            let name = input.url.lastPathComponent
            result.progress.currentSource = input.url
            result.progress.copyProgress = nil
            result.progress.verifyingCopy = false
            result.progress.revision += 1
            await publish(result.progress)
            if Task.isCancelled {
                result.progress.cancelled = true
                result.retry.append(contentsOf: request.sources[index...])
                break
            }

            do {
                try destination.verify()
            } catch {
                retainUnstarted(request.sources[index...], for: destination.url, in: &result)
                break
            }
            do {
                try result.journal?.begin(input.url)
            } catch {
                logger.warning("recovery write refused id=\(progress.id.uuidString, privacy: .public) phase=item-start")
                result.progress.issues.append(BrowserTransferIssue(name: name,
                    message: "Could not record the next item. This item and the remaining items were left alone.", kind: .failed))
                result.retry.append(contentsOf: request.sources[index...])
                break
            }
            let previousSuccesses = result.progress.succeeded
            let previousMoves = result.moves.count
            var retrySource: BrowserTransferSource?
            do {
                let source = try input.reference()
                retrySource = .captured(source)
                let copyStarted = ProcessInfo.processInfo.systemUptime
                var lastSample: TimeInterval = 0
                func publishBytes(_ bytes: BrowserTransfer.CopyProgress) {
                    result.progress.copyProgress = bytes
                    result.progress.revision += 1
                    let snapshot = result.progress
                    Task { await publish(snapshot) }
                }
                func showBytes(_ written: Int64, _ total: Int64) {
                    let elapsed = ProcessInfo.processInfo.systemUptime - copyStarted
                    guard total > 0, elapsed - lastSample >= 0.15 else { return }
                    lastSample = elapsed
                    publishBytes(.init(source: nil, written: min(written, total), total: total, elapsed: elapsed))
                }
                func showNestedBytes(_ sample: NestedCopyProgress) {
                    publishBytes(.init(source: sample.source, written: sample.written, total: sample.total,
                                       elapsed: sample.elapsed))
                }
                if request.move {
                    do {
                        let receipt = try move(source, into: destination, conflict: request.conflict)
                        result.moves.append(.sameVolume(receipt))
                    } catch FileOperationError.crossVolumeMove {
                        let moved = try FileOperations.moveAcrossVolumes(
                            source, into: destination.url, expectedFolder: destination.identity,
                            conflictPolicy: request.conflict == .keepBoth ? .keepBoth : .refuse,
                            isCancelled: { Task.isCancelled }, onNestedProgress: showNestedBytes,
                            onProgress: showBytes, onVerify: {
                                result.progress.verifyingCopy = true
                                result.progress.copyProgress = nil
                                result.progress.revision += 1
                                let snapshot = result.progress
                                Task { await publish(snapshot) }
                            }
                        )
                        result.moves.append(.crossVolume(moved.receipt))
                        if let staging = moved.remainingStagingFolder {
                            result.progress.issues.append(BrowserTransferIssue(name: name,
                                message: "Moved. Temporary cleanup failed at \(staging.path).", kind: .warning))
                        }
                    }
                } else {
                    let receipt = try FileOperations.copy(
                        source,
                        into: destination.url,
                        expectedFolder: destination.identity,
                        conflictPolicy: request.conflict == .keepBoth ? .keepBoth : .refuse,
                        isCancelled: { Task.isCancelled }, onNestedProgress: showNestedBytes, onProgress: showBytes
                    )
                    if let staging = receipt.remainingStagingFolder {
                        result.progress.issues.append(BrowserTransferIssue(
                            name: name,
                            message: "Copied. Temporary cleanup failed at \(staging.path).",
                            kind: .warning
                        ))
                    }
                }
                result.progress.succeeded += 1
            } catch is CancellationError {
                result.progress.cancelled = true
                result.retry.append(retrySource ?? input)
                result.retry.append(contentsOf: request.sources.dropFirst(index + 1))
                break
            } catch FileOperationError.destinationExists(let occupied) where request.conflict == .skip {
                let conflict = retrySource.flatMap {
                    describeConflict($0, existing: occupied, move: request.move)
                }
                result.progress.issues.append(BrowserTransferIssue(
                    name: name,
                    message: "Skipped because \(occupied.lastPathComponent) already exists.",
                    kind: .skipped, conflict: conflict
                ))
                if let retrySource { result.retry.append(retrySource) }
            } catch {
                result.progress.issues.append(BrowserTransferIssue(
                    name: name, message: error.localizedDescription, kind: .failed
                ))
                if let retrySource, isRetryable(error) { result.retry.append(retrySource) }
            }
            result.progress.completed += 1
            do {
                let move = result.moves.count > previousMoves ? result.moves.last : nil
                try result.journal?.finish(succeeded: result.progress.succeeded > previousSuccesses, move: move)
            } catch {
                logger.warning("recovery write refused id=\(progress.id.uuidString, privacy: .public) phase=item-finish")
                result.progress.issues.append(BrowserTransferIssue(name: name,
                    message: "Could not finish the recovery record. The transfer stopped before the next item.", kind: .warning))
                result.retry.append(contentsOf: request.sources.dropFirst(index + 1))
                break
            }
            result.progress.copyProgress = nil
            result.progress.verifyingCopy = false
            result.progress.revision += 1
            await publish(result.progress)
        }
        result.progress.currentSource = nil
        result.progress.copyProgress = nil
        result.progress.verifyingCopy = false
        result.progress.running = false
        return result
    }

    private static func retainUnstarted(
        _ inputs: ArraySlice<BrowserTransferSource>, for destination: URL, in result: inout BrowserTransferResult
    ) {
        result.progress.issues.append(BrowserTransferIssue(
            name: destination.lastPathComponent,
            message: "The original destination is unavailable. Restore access to it or choose a folder for a new transfer.",
            kind: .failed
        ))
        for input in inputs {
            do {
                result.retry.append(.captured(try input.reference()))
            } catch {
                result.progress.issues.append(BrowserTransferIssue(
                    name: input.url.lastPathComponent, message: error.localizedDescription, kind: .failed
                ))
            }
        }
    }

    private static func describeConflict(
        _ source: BrowserTransferSource, existing: URL, move: Bool
    ) -> BrowserFileConflict? {
        guard let incoming = try? source.reference(), let occupant = try? FileReference(url: existing),
              let first = try? FileEntry(url: incoming.url), let second = try? FileEntry(url: occupant.url) else {
            return nil
        }
        let reviewed: BrowserFileConflict.Replacement?
        let targetFolder = existing.deletingLastPathComponent()
        let replacementFolders = incoming.identity.device != occupant.identity.device && move
            ? [incoming.url.deletingLastPathComponent(), targetFolder]
            : [targetFolder]
        let canReplace = replacementFolders.allSatisfy { folder in
            guard let values = try? folder.resourceValues(forKeys: [.volumeSupportsExclusiveRenamingKey, .volumeIsReadOnlyKey]) else {
                return false
            }
            return values.volumeSupportsExclusiveRenaming == true && values.volumeIsReadOnly != true
        }
        if canReplace, incoming.identity != occupant.identity,
           let a = try? ReviewedFile(incoming), let b = try? ReviewedFile(occupant) {
            reviewed = .init(incoming: a, existing: b)
        } else {
            reviewed = nil
        }
        let merge: BrowserFileConflict.Merge?
        if !move, first.kind == .directory, second.kind == .directory, incoming.identity != occupant.identity {
            merge = .init(incoming: incoming, existing: occupant)
        } else {
            merge = nil
        }
        return BrowserFileConflict(incoming: first, existing: second, reviewed: reviewed, merge: merge)
    }

    private static func isRetryable(_ error: Error) -> Bool {
        guard let error = error as? FileOperationError else { return true }
        switch error {
        case .moveNeedsReview, .replacementNeedsReview, .sourceChanged, .folderChanged, .crossVolumeMove, .crossVolumeMoveNeedsReview,
             .copyNeedsReview, .exclusiveRenameUnsupported, .destinationInsideSource, .stagingCleanupFailed:
            return false
        default:
            return true
        }
    }

    private static func move(
        _ source: FileReference, into destination: BrowserTransferDestination,
        conflict: BrowserConflictPolicy
    ) throws -> MoveReceipt {
        do {
            return try FileOperations.move(source, into: destination.url, expectedFolder: destination.identity)
        } catch FileOperationError.destinationExists where conflict == .keepBoth {
            // An exclusive move refused the original name. Try another name and
            // leave every occupant untouched.
        }
        let entry = try FileEntry(url: source.url)
        let ext = entry.kind == .directory ? "" : source.url.pathExtension
        let basename = ext.isEmpty ? entry.name : source.url.deletingPathExtension().lastPathComponent
        let suffix = ext.isEmpty ? "" : "." + ext
        for index in 2...9_999 {
            try Task.checkCancellation()
            try destination.verify()
            let name = "\(basename) (\(index))" + suffix
            do {
                return try FileOperations.move(source, into: destination.url, named: name, expectedFolder: destination.identity)
            } catch FileOperationError.destinationExists {
                continue
            }
        }
        throw BrowserTransferError.noFreeName
    }
}

private enum BrowserTransferError: LocalizedError {
    case noFreeName

    var errorDescription: String? { "Choose a different name for the item." }
}
