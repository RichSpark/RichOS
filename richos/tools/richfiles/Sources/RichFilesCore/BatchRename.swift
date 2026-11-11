import Darwin
import Foundation

public enum BatchRenameRule: Sendable {
    case prefix(String)
    case suffix(String)
    case replace(search: String, replacement: String)
    case number(prefix: String, start: Int, minimumDigits: Int)
    case regex(pattern: String, replacement: String)
    case metadata(template: String)

    fileprivate typealias Transform = (String, Int, FileEntry) throws -> String

    fileprivate func prepare(count: Int) throws -> Transform {
        switch self {
        case .prefix(let value):
            return { basename, _, _ in value + basename }
        case .suffix(let value):
            return { basename, _, _ in basename + value }
        case .replace(let search, let replacement):
            guard !search.isEmpty else { throw BatchRenameError.invalidRule }
            return { basename, _, _ in basename.replacingOccurrences(of: search, with: replacement) }
        case .number(let prefix, let start, let digits):
            guard start >= 0, (1...12).contains(digits),
                  !start.addingReportingOverflow(max(0, count - 1)).overflow else {
                throw BatchRenameError.invalidRule
            }
            return { _, index, _ in
                let number = String(start + index)
                return prefix + String(repeating: "0", count: max(0, digits - number.count)) + number
            }
        case .regex(let pattern, let replacement):
            guard !pattern.isEmpty, pattern.utf8.count <= 256, replacement.utf8.count <= 256,
                  let regex = try? NSRegularExpression(pattern: pattern) else {
                throw BatchRenameError.invalidExpression
            }
            return { basename, _, _ in
                var matches: [NSTextCheckingResult] = []
                let deadline = ProcessInfo.processInfo.systemUptime + 0.1
                var expired = false
                regex.enumerateMatches(in: basename, options: .reportProgress,
                                       range: NSRange(basename.startIndex..., in: basename)) { match, _, stop in
                    if Task.isCancelled || ProcessInfo.processInfo.systemUptime > deadline {
                        stop.pointee = true
                        expired = true
                    } else if let match {
                        matches.append(match)
                    }
                }
                if Task.isCancelled { throw CancellationError() }
                if expired { throw BatchRenameError.expressionTimedOut }
                let result = NSMutableString(string: basename)
                for match in matches.reversed() {
                    let value = regex.replacementString(for: match, in: basename, offset: 0, template: replacement)
                    result.replaceCharacters(in: match.range, with: value)
                }
                return result as String
            }
        case .metadata(let template):
            return try Self.prepareTemplate(template)
        }
    }

    fileprivate var usesMetadata: Bool {
        if case .metadata = self { return true }
        return false
    }

    private enum Field: String {
        case name, created, modified, number
    }

    private static func prepareTemplate(_ template: String) throws -> Transform {
        guard !template.isEmpty, template.utf8.count <= 256 else { throw BatchRenameError.invalidRule }
        let expression = try NSRegularExpression(pattern: #"\{([^{}]+)\}"#)
        let pattern = template as NSString
        let fields = try expression.matches(in: template, range: NSRange(location: 0, length: pattern.length))
            .map { match -> (range: NSRange, field: Field) in
                guard let field = Field(rawValue: pattern.substring(with: match.range(at: 1))) else {
                    throw BatchRenameError.unknownMetadataField
                }
                return (match.range, field)
            }
        guard !fields.isEmpty else { throw BatchRenameError.unknownMetadataField }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd"
        return { basename, index, entry in
            let result = NSMutableString(string: template)
            for (range, field) in fields.reversed() {
                let value: String
                switch field {
                case .name: value = basename
                case .number: value = String(index + 1)
                case .created, .modified:
                    guard let date = field == .created ? entry.created : entry.modified else {
                        throw BatchRenameError.metadataUnavailable(entry.name)
                    }
                    value = formatter.string(from: date)
                }
                result.replaceCharacters(in: range, with: value)
            }
            return result as String
        }
    }
}

public enum BatchRenameError: LocalizedError {
    case invalidRule
    case invalidExpression
    case expressionTimedOut
    case unknownMetadataField
    case metadataUnavailable(String)
    case duplicateSelection
    case outsideFolder
    case conflicts

    public var errorDescription: String? {
        switch self {
        case .invalidRule: return "Enter a valid replacement or numbering rule."
        case .invalidExpression: return "Enter a valid regular expression up to 256 bytes."
        case .expressionTimedOut: return "The regular expression took too long. Simplify the pattern and preview again."
        case .unknownMetadataField: return "Use {name}, {created}, {modified} or {number} in the metadata template."
        case .metadataUnavailable(let name): return "The requested file date is unavailable for \(name)."
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
        fileprivate let version: FileVersion?
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
        try preview(sources, in: location, rules: [rule])
    }

    public static func preview(
        _ sources: [FileReference], in location: URL, rules: [BatchRenameRule]
    ) throws -> BatchRenamePlan {
        guard (1...12).contains(rules.count) else { throw BatchRenameError.invalidRule }
        let transforms = try rules.map { try $0.prepare(count: sources.count) }
        let usesMetadata = rules.contains(where: \.usesMetadata)
        let folder = try OpenFolder(location)
        let values = try folder.url.resourceValues(forKeys: [.volumeSupportsCaseSensitiveNamesKey])
        let caseSensitive = values.volumeSupportsCaseSensitiveNames == true
        var seen: Set<String> = []
        var proposed: [(source: FileReference, name: String, version: FileVersion?)] = []
        var nameCounts: [String: Int] = [:]
        for (index, source) in sources.enumerated() {
            try Task.checkCancellation()
            guard source.parentIdentity == folder.identity else { throw BatchRenameError.outsideFolder }
            let oldName = source.url.lastPathComponent
            guard seen.insert(comparisonKey(oldName, caseSensitive: caseSensitive)).inserted else {
                throw BatchRenameError.duplicateSelection
            }
            let before = try folder.verifiedStatus(of: source)
            let entry = try FileEntry(url: source.url)
            let ext = entry.kind == .directory ? "" : source.url.pathExtension
            let basename = ext.isEmpty ? oldName : source.url.deletingPathExtension().lastPathComponent
            let extensionSuffix = ext.isEmpty ? "" : "." + ext
            let renamed = try transforms.reduce(basename) { current, transform in
                guard current.utf8.count <= 1024 else { throw BatchRenameError.invalidRule }
                return try transform(current, index, entry)
            }
            let name = renamed + extensionSuffix
            let version = usesMetadata ? FileVersion(before) : nil
            if let version, try FileVersion(folder.verifiedStatus(of: source)) != version {
                throw FileOperationError.sourceChanged(source.url)
            }
            proposed.append((source, name, version))
            nameCounts[comparisonKey(name, caseSensitive: caseSensitive), default: 0] += 1
        }
        let proposals = try proposed.map { item in
            try Task.checkCancellation()
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
            return Proposal(source: item.source, name: item.name, status: status, version: item.version)
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
            try Self.verify(proposal, in: directory)
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
                try Self.verify(proposal, in: directory)
                return .renamed(try FileOperations.rename(proposal.source, to: proposal.name))
            } catch {
                stopped = .notAttempted
                return .failed(error.localizedDescription)
            }
        }
    }

    private static func verify(_ proposal: Proposal, in folder: OpenFolder) throws {
        let status = try folder.verifiedStatus(of: proposal.source)
        if let version = proposal.version, FileVersion(status) != version {
            throw FileOperationError.sourceChanged(proposal.source.url)
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
