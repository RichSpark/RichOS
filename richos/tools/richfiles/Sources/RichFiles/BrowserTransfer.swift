import Foundation
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
}

struct BrowserTransfer: Sendable {
    let destination: URL
    let move: Bool
    let total: Int
    var completed = 0
    var succeeded = 0
    var currentName: String?
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

struct BrowserTransferDestination: Sendable {
    let url: URL
    private let reference: FileReference?

    init(folder: URL) throws {
        url = try DirectoryReader.directoryURL(folder)
        // The root has no named directory entry. Other destinations retain the
        // selected folder's identity while a batch waits or is retried.
        reference = url.path == "/" ? nil : try FileReference(url: url)
    }

    func verify() throws {
        guard let reference else { return }
        guard let current = try? FileReference(url: url), current == reference else {
            throw FileOperationError.folderChanged(url)
        }
    }
}

struct BrowserTransferResult: Sendable {
    var progress: BrowserTransfer
    var moves: [MoveReceipt] = []
    var retry: [BrowserTransferSource] = []
    var destination: BrowserTransferDestination?
}

enum BrowserTransferJob {
    static func perform(
        _ request: BrowserTransferRequest,
        publish: (BrowserTransfer) async -> Void
    ) async -> BrowserTransferResult {
        var result = BrowserTransferResult(progress: BrowserTransfer(
            destination: request.folder, move: request.move, total: request.sources.count
        ))
        let destination: BrowserTransferDestination
        do {
            destination = try request.destination ?? BrowserTransferDestination(folder: request.folder)
            try destination.verify()
        } catch {
            result.progress.issues.append(BrowserTransferIssue(
                name: request.folder.lastPathComponent, message: error.localizedDescription, kind: .failed
            ))
            result.progress.running = false
            return result
        }
        result.destination = destination

        for (index, input) in request.sources.enumerated() {
            if Task.isCancelled {
                result.progress.cancelled = true
                result.retry.append(contentsOf: request.sources[index...])
                break
            }
            let name = input.url.lastPathComponent
            result.progress.currentName = name
            await publish(result.progress)
            if Task.isCancelled {
                result.progress.cancelled = true
                result.retry.append(contentsOf: request.sources[index...])
                break
            }

            var retrySource: BrowserTransferSource?
            do {
                try destination.verify()
                let source = try input.reference()
                retrySource = .captured(source)
                if request.move {
                    let receipt = try move(source, into: destination, conflict: request.conflict)
                    result.moves.append(receipt)
                } else {
                    let receipt = try FileOperations.copy(
                        source,
                        into: destination.url,
                        conflictPolicy: request.conflict == .keepBoth ? .keepBoth : .refuse,
                        isCancelled: { Task.isCancelled }
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
                result.progress.issues.append(BrowserTransferIssue(
                    name: name,
                    message: "Skipped because \(occupied.lastPathComponent) already exists.",
                    kind: .skipped
                ))
                if let retrySource { result.retry.append(retrySource) }
            } catch {
                result.progress.issues.append(BrowserTransferIssue(
                    name: name, message: error.localizedDescription, kind: .failed
                ))
                if let retrySource, isRetryable(error) { result.retry.append(retrySource) }
            }
            result.progress.completed += 1
            await publish(result.progress)
        }
        result.progress.currentName = nil
        result.progress.running = false
        return result
    }

    private static func isRetryable(_ error: Error) -> Bool {
        guard let error = error as? FileOperationError else { return true }
        switch error {
        case .moveNeedsReview, .sourceChanged, .folderChanged, .crossVolumeMove,
             .destinationInsideSource, .stagingCleanupFailed:
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
            return try FileOperations.move(source, into: destination.url)
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
                return try FileOperations.move(source, into: destination.url, named: name)
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
