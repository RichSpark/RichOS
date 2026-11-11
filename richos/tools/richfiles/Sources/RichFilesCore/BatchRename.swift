import Darwin
import Foundation

public enum BatchRenameRule: Sendable {
    case prefix(String)
    case suffix(String)
    case replace(search: String, replacement: String)
    case number(prefix: String, start: Int, minimumDigits: Int)

    fileprivate func validate(count: Int) throws {
        switch self {
        case .replace(let search, _):
            guard !search.isEmpty else { throw BatchRenameError.invalidRule }
        case .number(_, let start, let digits):
            guard start >= 0, (1...12).contains(digits),
                  !start.addingReportingOverflow(max(0, count - 1)).overflow else {
                throw BatchRenameError.invalidRule
            }
        case .prefix, .suffix:
            break
        }
    }

    fileprivate func name(for basename: String, index: Int) -> String {
        switch self {
        case .prefix(let value):
            return value + basename
        case .suffix(let value):
            return basename + value
        case .replace(let search, let replacement):
            return basename.replacingOccurrences(of: search, with: replacement)
        case .number(let prefix, let start, let digits):
            let number = String(start + index)
            return prefix + String(repeating: "0", count: max(0, digits - number.count)) + number
        }
    }
}

public enum BatchRenameError: LocalizedError {
    case invalidRule
    case duplicateSelection
    case outsideFolder
    case conflicts

    public var errorDescription: String? {
        switch self {
        case .invalidRule: return "Enter a valid replacement or numbering rule."
        case .duplicateSelection: return "The same item appears more than once in the selection."
        case .outsideFolder: return "Choose items from one folder for this batch."
        case .conflicts: return "Resolve the invalid names and conflicts before renaming."
        }
    }
}

public struct BatchRenamePlan: Sendable {
    public enum Status: Equatable, Sendable {
        case ready
        case unchanged
        case invalidName
        case nameTaken
        case duplicateName
    }

    public struct Proposal: Sendable {
        public let source: FileReference
        public let name: String
        public let status: Status
    }

    public enum Outcome: Sendable {
        case renamed(MoveReceipt)
        case unchanged
        case failed(String)
        case cancelled
        case notAttempted
    }

    public let proposals: [Proposal]
    private let folder: URL
    private let folderIdentity: FileIdentity
    private let caseSensitive: Bool

    public var canApply: Bool {
        proposals.contains { $0.status == .ready }
            && proposals.allSatisfy { $0.status == .ready || $0.status == .unchanged }
    }

    /// Preserve file and package extensions. A period in an ordinary folder name is part of its
    /// name. Keep the caller's selection order so the numbering matches the preview.
    public static func preview(
        _ sources: [FileReference],
        in location: URL,
        rule: BatchRenameRule
    ) throws -> BatchRenamePlan {
        try rule.validate(count: sources.count)
        let folder = try OpenFolder(location)
        let values = try folder.url.resourceValues(forKeys: [.volumeSupportsCaseSensitiveNamesKey])
        let caseSensitive = values.volumeSupportsCaseSensitiveNames == true
        var seen: Set<String> = []
        var proposed: [(source: FileReference, name: String)] = []
        var nameCounts: [String: Int] = [:]
        for (index, source) in sources.enumerated() {
            guard source.parentIdentity == folder.identity else { throw BatchRenameError.outsideFolder }
            let oldName = source.url.lastPathComponent
            guard seen.insert(comparisonKey(oldName, caseSensitive: caseSensitive)).inserted else {
                throw BatchRenameError.duplicateSelection
            }
            _ = try folder.verifiedStatus(of: source)
            let entry = try FileEntry(url: source.url)
            let ext = entry.kind == .directory ? "" : source.url.pathExtension
            let basename = ext.isEmpty ? oldName : source.url.deletingPathExtension().lastPathComponent
            let extensionSuffix = ext.isEmpty ? "" : "." + ext
            let name = rule.name(for: basename, index: index) + extensionSuffix
            proposed.append((source, name))
            nameCounts[comparisonKey(name, caseSensitive: caseSensitive), default: 0] += 1
        }
        let proposals = try proposed.map { item in
            let status: Status
            if item.name.utf8.elementsEqual(item.source.url.lastPathComponent.utf8) {
                status = .unchanged
            } else if (try? FileOperations.validateName(item.name)) == nil {
                status = .invalidName
            } else if nameCounts[comparisonKey(item.name, caseSensitive: caseSensitive), default: 0] > 1 {
                status = .duplicateName
            } else {
                do {
                    status = try isOccupied(item.name, byOtherThan: item.source, in: folder,
                                            caseSensitive: caseSensitive) ? .nameTaken : .ready
                } catch FileOperationError.system(_, _, ENAMETOOLONG) {
                    status = .invalidName
                }
            }
            return Proposal(source: item.source, name: item.name, status: status)
        }
        return BatchRenamePlan(
            proposals: proposals, folder: folder.url, folderIdentity: folder.identity,
            caseSensitive: caseSensitive
        )
    }

    /// Recheck the entire preview before starting. Each successful rename uses the underlying
    /// exclusive move and returns its own undo receipt. An intervening error stops the rest.
    public func apply(isCancelled: () -> Bool = { false }) throws -> [Outcome] {
        guard canApply else { throw BatchRenameError.conflicts }
        if isCancelled() { throw CancellationError() }
        let directory = try OpenFolder(folder)
        try directory.requireIdentity(folderIdentity)
        for proposal in proposals where proposal.status == .ready {
            if isCancelled() { throw CancellationError() }
            _ = try directory.verifiedStatus(of: proposal.source)
            if try Self.isOccupied(proposal.name, byOtherThan: proposal.source, in: directory,
                                   caseSensitive: caseSensitive) {
                throw FileOperationError.destinationExists(directory.childURL(proposal.name))
            }
        }
        var stopped: Outcome?
        return proposals.map { proposal in
            if proposal.status == .unchanged { return .unchanged }
            if let stopped { return stopped }
            if isCancelled() {
                stopped = .cancelled
                return .cancelled
            }
            do {
                return .renamed(try FileOperations.rename(proposal.source, to: proposal.name))
            } catch {
                stopped = .notAttempted
                return .failed(error.localizedDescription)
            }
        }
    }

    private static func comparisonKey(_ name: String, caseSensitive: Bool) -> String {
        let normalized = name.precomposedStringWithCanonicalMapping
        return caseSensitive ? normalized : normalized.lowercased()
    }

    private static func isOccupied(
        _ name: String, byOtherThan source: FileReference, in folder: OpenFolder, caseSensitive: Bool
    ) throws -> Bool {
        guard let current = try folder.existingReference(named: name) else {
            return false
        }
        // A case-only change can find the source at the new spelling on this filesystem.
        return current.identity != source.identity
            || comparisonKey(name, caseSensitive: caseSensitive)
                != comparisonKey(source.url.lastPathComponent, caseSensitive: caseSensitive)
    }
}
