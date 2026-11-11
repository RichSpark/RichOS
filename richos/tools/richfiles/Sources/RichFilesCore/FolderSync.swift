import Darwin
import Foundation

public struct FolderSyncPlan: Sendable {
    public struct Options: Hashable, Sendable {
        public var replaceDifferentFiles = false
        public var trashDestinationExtras = false

        public init(replaceDifferentFiles: Bool = false, trashDestinationExtras: Bool = false) {
            self.replaceDifferentFiles = replaceDifferentFiles
            self.trashDestinationExtras = trashDestinationExtras
        }
    }

    public struct Row: Identifiable, Sendable {
        public enum Action: String, Sendable {
            case createFolder = "Create folder"
            case copy = "Copy"
            case replace = "Replace and keep previous in Trash"
            case trash = "Move to Trash"
            case keep = "Keep"
            case conflict = "Needs manual review"
        }

        public let path: String
        public let action: Action
        public let detail: String
        public var id: String { path }
    }

    public enum Change: Sendable {
        case copied(CopyReceipt)
        case createdFolder(FileReference)
        case replaced(ReplaceReceipt)
        case trashed(TrashReceipt, emptyFolder: Bool)
    }

    public struct Outcome: Sendable {
        public fileprivate(set) var changes: [Change] = []
        public fileprivate(set) var problem: String?
        public fileprivate(set) var cancelled = false
    }

    public let rows: [Row]
    public let unavailable: Int
    public let otherVolumes: Int
    private let source: Root
    private let destination: Root
    private let steps: [Step]

    public var changeCount: Int { steps.count }
    public var canApply: Bool { changeCount > 0 && unavailable == 0 && otherVolumes == 0 }

    public static func preview(
        from first: URL, to second: URL, options: Options, isCancelled: () -> Bool = { false }
    ) throws -> Self {
        let left = try FolderInventorySnapshot(first, includePackages: false, isCancelled: isCancelled)
        let right = try FolderInventorySnapshot(second, includePackages: false, isCancelled: isCancelled)
        guard left.identity != right.identity,
              !containsFolder(left.folder, right.folder), !containsFolder(right.folder, left.folder) else {
            throw SyncError.overlap
        }
        let report = try StorageAnalysis.compare(left, right, mode: .contents, isCancelled: isCancelled)
        let caseSensitive = try right.folder.resourceValues(
            forKeys: [.volumeSupportsCaseSensitiveNamesKey]
        ).volumeSupportsCaseSensitiveNames == true
        let sourceNames = Set(left.entries.keys.map { normalized($0, caseSensitive: caseSensitive) })
        let destinationNames = Set(right.entries.keys.map { normalized($0, caseSensitive: caseSensitive) })
        let blockedPaths = Set(report.differences.compactMap { row -> String? in
            let key = normalized(row.relativePath, caseSensitive: caseSensitive)
            switch row.kind {
            case .differentType: return row.relativePath
            case .leftOnly: return destinationNames.contains(key) ? row.relativePath : nil
            case .rightOnly: return sourceNames.contains(key) ? row.relativePath : nil
            default: return nil
            }
        })
        var steps: [Step] = []
        var rows: [Row] = []
        var keepDirectories: Set<String> = []

        for difference in report.differences {
            if isCancelled() { throw CancellationError() }
            let path = difference.relativePath
            let folded = normalized(path, caseSensitive: caseSensitive)
            let ancestors = parents(of: path)
            let item = left.entries[path] ?? right.entries[path]
            let decision: (Row.Action, String, Operation?)
            if ancestors.contains(where: blockedPaths.contains) {
                decision = (.conflict, "A parent path has a name or item type conflict.", nil)
            } else if let item, item.isDirectory,
                      try item.url.resourceValues(forKeys: [.isPackageKey]).isPackage == true {
                decision = (.conflict, "Package contents are outside this sync preview.", nil)
            } else {
                switch difference.kind {
                case .leftOnly:
                    if destinationNames.contains(folded) {
                        decision = (.conflict, "The destination has an equivalent name.", nil)
                    } else if let item, canCopy(item) {
                        let entry = try PinnedEntry(item)
                        decision = item.isDirectory ? (.createFolder, "", .create(entry)) : (.copy, "", .copy(entry))
                    } else {
                        decision = (.conflict, "The source item is unavailable or uses an unsupported type.", nil)
                    }
                case .rightOnly:
                    if sourceNames.contains(folded) {
                        decision = (.conflict, "The source has an equivalent name.", nil)
                    } else if !options.trashDestinationExtras {
                        decision = (.keep, "Only in destination.", nil)
                    } else if let item, canCopy(item) {
                        decision = (.trash, "", .trash(try PinnedEntry(item)))
                    } else {
                        decision = (.conflict, "The destination item is unavailable or uses an unsupported type.", nil)
                    }
                case .changed:
                    if !options.replaceDifferentFiles {
                        decision = (.keep, "Different contents.", nil)
                    } else if let a = left.entries[path], let b = right.entries[path],
                              a.status.st_mode & S_IFMT == S_IFREG, b.status.st_mode & S_IFMT == S_IFREG {
                        let incoming = try PinnedEntry(a)
                        let existing = try PinnedEntry(b)
                        decision = (.replace, "", .replace(try incoming.reviewedFile(), try existing.reviewedFile()))
                    } else {
                        decision = (.conflict, "Both items must be regular files for replacement.", nil)
                    }
                case .differentType:
                    decision = (.conflict, "Different item types.", nil)
                case .unavailable:
                    decision = (.conflict, "Could not compare this path.", nil)
                }
            }
            let (action, detail, operation) = decision
            if let operation { steps.append(Step(path: path, operation: operation)) }
            if operation == nil, difference.kind == .rightOnly { keepDirectories.formUnion(ancestors) }
            rows.append(Row(path: path, action: action, detail: detail))
        }
        let skippedDirectories = Set(steps.filter { step in
            if case .trash(let entry) = step.operation, entry.isDirectory {
                return keepDirectories.contains(step.path)
            }
            return false
        }.map(\.path))
        let ordered = steps.filter { !skippedDirectories.contains($0.path) }.sorted(by: Step.precedes)
        rows = rows.map {
            guard skippedDirectories.contains($0.path) else { return $0 }
            return Row(path: $0.path, action: .keep, detail: "The folder contains items left for review.")
        }
        return Self(rows: rows, unavailable: report.unavailable, otherVolumes: report.otherVolumes,
                    source: Root(left), destination: Root(right), steps: ordered)
    }

    public func apply(isCancelled: () -> Bool = { false }) throws -> Outcome {
        guard canApply else { throw SyncError.unavailable }
        // Review the exact inventories again before mutating anything. Steps also
        // check their inputs just before acting, as other apps can still change them.
        try source.verify(isCancelled: isCancelled)
        try destination.verify(isCancelled: isCancelled)
        var outcome = Outcome()
        var folders = destination.directories
        for step in steps {
            do {
                if isCancelled() { throw CancellationError() }
                try source.verifyLocation()
                try destination.verifyLocation()
                switch step.operation {
                case .create(let entry):
                    try entry.verify()
                    let parent = try destination.parent(of: step.path, identities: folders)
                    let created = try FileOperations.createFolder(in: parent, named: entry.reference.url.lastPathComponent)
                    folders[step.path] = created.identity
                    outcome.changes.append(.createdFolder(created))
                case .copy(let entry):
                    let parent = try destination.parent(of: step.path, identities: folders)
                    let copy = try FileOperations.copy(entry.reference, into: parent, expected: entry.version,
                                                       isCancelled: isCancelled)
                    outcome.changes.append(.copied(copy))
                case .replace(let incoming, let existing):
                    let receipt = try FileOperations.replaceCopy(incoming, replacing: existing, isCancelled: isCancelled)
                    outcome.changes.append(.replaced(receipt))
                case .trash(let entry):
                    let receipt = try entry.isDirectory
                        ? FileOperations.trashEmptyFolder(entry.reference)
                        : FileOperations.trash(entry.reference, expected: entry.version)
                    outcome.changes.append(.trashed(receipt, emptyFolder: entry.isDirectory))
                }
            } catch is CancellationError {
                outcome.cancelled = true
                break
            } catch {
                outcome.problem = "\(step.path): \(error.localizedDescription)"
                break
            }
        }
        return outcome
    }

    private struct Root: Sendable {
        let url: URL
        let identity: FileIdentity
        let versions: [String: FileVersion]
        let directories: [String: FileIdentity]

        init(_ snapshot: FolderInventorySnapshot) {
            url = snapshot.folder
            identity = snapshot.identity
            versions = snapshot.entries.mapValues { FileVersion($0.status) }
            var folders = snapshot.entries.filter { $0.value.isDirectory }.mapValues { FileIdentity($0.status) }
            folders[""] = identity
            directories = folders
        }

        func verify(isCancelled: () -> Bool) throws {
            let current = try FolderInventorySnapshot(url, includePackages: false, isCancelled: isCancelled)
            guard current.identity == identity, current.coverage.unavailable == 0, current.coverage.otherVolumes == 0,
                  current.entries.mapValues({ FileVersion($0.status) }) == versions else {
                throw SyncError.changed(url)
            }
        }

        func verifyLocation() throws {
            try OpenFolder(url).requireIdentity(identity)
        }

        func parent(of relative: String, identities: [String: FileIdentity]) throws -> OpenFolder {
            let path = parents(of: relative).first ?? ""
            let folder = try OpenFolder(path.isEmpty ? url : url.appendingPathComponent(path, isDirectory: true))
            guard let expected = identities[path] else { throw SyncError.changed(folder.url) }
            try folder.requireIdentity(expected)
            return folder
        }
    }

    private struct PinnedEntry: Sendable {
        let reference: FileReference
        let version: FileVersion
        let isDirectory: Bool

        init(_ entry: FileInventory.Entry) throws {
            reference = try FileReference(url: entry.url)
            version = FileVersion(entry.status)
            isDirectory = entry.isDirectory
            try verify()
        }

        func verify() throws {
            let parent = try OpenFolder(reference.url.deletingLastPathComponent())
            guard try FileVersion(parent.verifiedStatus(of: reference)) == version else {
                throw FileOperationError.sourceChanged(reference.url)
            }
        }

        func reviewedFile() throws -> ReviewedFile {
            let result = try ReviewedFile(reference)
            try verify()
            return result
        }
    }

    private enum Operation: Sendable {
        case create(PinnedEntry)
        case copy(PinnedEntry)
        case replace(ReviewedFile, ReviewedFile)
        case trash(PinnedEntry)

        var priority: Int {
            switch self {
            case .create: return 0
            case .copy, .replace: return 1
            case .trash(let entry): return entry.isDirectory ? 3 : 2
            }
        }
    }

    private struct Step: Sendable {
        let path: String
        let operation: Operation

        static func precedes(_ a: Step, _ b: Step) -> Bool {
            let left = a.operation.priority
            let right = b.operation.priority
            if left != right { return left < right }
            let aDepth = a.path.filter { $0 == "/" }.count
            let bDepth = b.path.filter { $0 == "/" }.count
            if aDepth != bDepth { return left == 3 ? aDepth > bDepth : aDepth < bDepth }
            return a.path.utf8.lexicographicallyPrecedes(b.path.utf8)
        }
    }

    private enum SyncError: LocalizedError {
        case overlap, unavailable, changed(URL)

        var errorDescription: String? {
            switch self {
            case .overlap: return "Choose two separate folders. One cannot contain the other."
            case .unavailable: return "There is no complete sync preview to apply."
            case .changed(let url): return "\(url.path) changed. Preview the sync again before applying."
            }
        }
    }

    private static func canCopy(_ item: FileInventory.Entry) -> Bool {
        let kind = item.status.st_mode & S_IFMT
        return !TextContentReader.isDataless(item.status) && (kind == S_IFREG || kind == S_IFDIR || kind == S_IFLNK)
    }

    private static func containsFolder(_ parent: URL, _ child: URL) -> Bool {
        child.path.hasPrefix(parent.path == "/" ? "/" : parent.path + "/")
    }

    private static func normalized(_ path: String, caseSensitive: Bool) -> String {
        let name = path.precomposedStringWithCanonicalMapping
        return caseSensitive ? name : name.lowercased()
    }

    private static func parents(of path: String) -> [String] {
        var result: [String] = []
        var end = path.endIndex
        while let separator = path[..<end].lastIndex(of: "/") {
            result.append(String(path[..<separator]))
            end = separator
        }
        return result
    }
}
