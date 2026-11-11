import Foundation

public enum FileKind: Equatable, Sendable {
    case directory
    case package
    case regularFile
    case symbolicLink
    case other
}

public struct FileEntry: Sendable, Identifiable {
    public var id: URL { url }
    public let url: URL
    public let name: String
    public let kind: FileKind
    public let typeDescription: String?
    public let size: Int64?
    public let modified: Date?
    public let created: Date?
    public let isHidden: Bool
    /// True when Foundation reports an undownloaded iCloud item.
    public let needsDownload: Bool

    fileprivate static let resourceKeys: Set<URLResourceKey> = [
        .isDirectoryKey, .isPackageKey, .isRegularFileKey, .isSymbolicLinkKey,
        .localizedTypeDescriptionKey, .fileSizeKey, .contentModificationDateKey,
        .creationDateKey, .isHiddenKey, .isUbiquitousItemKey, .ubiquitousItemDownloadingStatusKey
    ]

    public init(url: URL) throws {
        try requireLocalFileURL(url)
        let values = try url.resourceValues(forKeys: Self.resourceKeys)
        self.url = url
        name = url.lastPathComponent
        if values.isSymbolicLink == true {
            kind = .symbolicLink
        } else if values.isDirectory == true {
            kind = values.isPackage == true ? .package : .directory
        } else {
            kind = values.isRegularFile == true ? .regularFile : .other
        }
        typeDescription = values.localizedTypeDescription
        size = kind == .regularFile ? values.fileSize.map(Int64.init) : nil
        modified = values.contentModificationDate
        created = values.creationDate
        isHidden = values.isHidden == true || name.hasPrefix(".")
        needsDownload = values.isUbiquitousItem == true
            && values.ubiquitousItemDownloadingStatus == .notDownloaded
    }
}

public struct FileQuery: Sendable {
    public enum Name: Sendable {
        case contains(String)
        case wildcard(String)
    }

    public let name: Name?
    public let extensions: Set<String>
    public let size: ClosedRange<Int64>?
    public let modified: ClosedRange<Date>?

    public init(
        name: Name? = nil,
        extensions: Set<String> = [],
        size: ClosedRange<Int64>? = nil,
        modified: ClosedRange<Date>? = nil
    ) {
        self.name = name
        self.extensions = Set(extensions.map {
            let ext = $0.hasPrefix(".") ? String($0.dropFirst()) : $0
            return ext.lowercased().precomposedStringWithCanonicalMapping
        })
        self.size = size
        self.modified = modified
    }

    fileprivate func matches(_ entry: FileEntry) -> Bool {
        switch name {
        case .contains(let fragment):
            if !fragment.isEmpty && entry.name.range(of: fragment, options: .caseInsensitive) == nil {
                return false
            }
        case .wildcard(let pattern):
            guard wildcardMatches(pattern.lowercased(), entry.name.lowercased()) else {
                return false
            }
        case nil:
            break
        }
        let ext = entry.url.pathExtension.lowercased().precomposedStringWithCanonicalMapping
        if !extensions.isEmpty && !extensions.contains(ext) { return false }
        if let size, entry.size.map(size.contains) != true { return false }
        if let modified, entry.modified.map(modified.contains) != true { return false }
        return true
    }
}

/// Match * and ? by characters so one ? can match an accented letter or emoji.
/// After a mismatch, only the last * needs to try consuming one more character.
private func wildcardMatches(_ pattern: String, _ name: String) -> Bool {
    var token = pattern.startIndex
    var character = name.startIndex
    var star: (nextToken: String.Index, endOfMatch: String.Index)?
    while character < name.endIndex {
        if token < pattern.endIndex && pattern[token] == "*" {
            pattern.formIndex(after: &token)
            star = (token, character)
        } else if token < pattern.endIndex && (pattern[token] == "?" || pattern[token] == name[character]) {
            pattern.formIndex(after: &token)
            name.formIndex(after: &character)
        } else if var previousStar = star {
            name.formIndex(after: &previousStar.endOfMatch)
            character = previousStar.endOfMatch
            token = previousStar.nextToken
            star = previousStar
        } else {
            return false
        }
    }
    while token < pattern.endIndex && pattern[token] == "*" {
        pattern.formIndex(after: &token)
    }
    return token == pattern.endIndex
}

public struct ScanIssue: Sendable {
    public let location: URL
    public let message: String
}

public struct ScanReport: Sendable {
    public fileprivate(set) var examined = 0
    public fileprivate(set) var matched = 0
    public fileprivate(set) var inaccessible = 0
    public fileprivate(set) var issues: [ScanIssue] = []
    public fileprivate(set) var contentCoverage: TextContentCoverage?
}

public enum DirectoryReader {
    public enum Scope: Equatable, Sendable {
        case folder
        case descendants
    }

    /// Call on a background worker. The callback receives batches in filesystem order.
    /// A caller can show those immediately and then sort them using FileSort.
    public static func scan(
        _ location: URL,
        scope: Scope = .folder,
        showHidden: Bool = false,
        query: FileQuery = FileQuery(),
        content: TextContentQuery? = nil,
        isCancelled: () -> Bool = { false },
        receive: ([FileEntry]) -> Void
    ) throws -> ScanReport {
        if isCancelled() { throw CancellationError() }
        let previousIOPolicy = try content.map { _ in try TextContentReader.disableDatalessMaterialization() }
        defer {
            if let previousIOPolicy { TextContentReader.restoreDatalessMaterialization(previousIOPolicy) }
        }
        let folder = try directoryURL(location)
        let contentReader = try content.map { try TextContentReader(folder: folder, query: $0) }
        var report = ScanReport()
        var rootError: Error?
        func record(_ url: URL, _ error: Error) {
            report.inaccessible += 1
            if report.issues.count < 20 {
                report.issues.append(ScanIssue(location: url, message: error.localizedDescription))
            }
        }

        var options: FileManager.DirectoryEnumerationOptions = [.skipsPackageDescendants]
        if scope == .folder { options.insert(.skipsSubdirectoryDescendants) }
        if !showHidden { options.insert(.skipsHiddenFiles) }
        guard let enumerator = FileManager.default.enumerator(
            at: folder,
            includingPropertiesForKeys: Array(FileEntry.resourceKeys),
            options: options,
            errorHandler: { url, error in
                if url.path == folder.path {
                    rootError = error
                    return false
                }
                record(url, error)
                return true
            }
        ) else {
            throw CocoaError(.fileReadUnknown)
        }

        var batch: [FileEntry] = []
        batch.reserveCapacity(128)
        while true {
            if isCancelled() { throw CancellationError() }
            guard let value = enumerator.nextObject() as? URL else { break }
            report.examined += 1
            let entry: FileEntry
            do {
                entry = try FileEntry(url: value)
            } catch {
                record(value, error)
                continue
            }
            // Foundation already excludes package contents and does not follow links.
            // Calling skipDescendants on a leaf can skip the next folder instead.
            if !showHidden && entry.isHidden {
                if scope == .descendants && entry.kind == .directory { enumerator.skipDescendants() }
                continue
            }
            if let contentReader, entry.kind == .directory {
                if scope == .descendants {
                    do {
                        if try !contentReader.mayDescend(into: entry) { enumerator.skipDescendants() }
                    } catch {
                        enumerator.skipDescendants()
                        record(value, error)
                    }
                }
                continue
            }
            guard query.matches(entry) else { continue }
            if let contentReader {
                do {
                    guard try contentReader.matches(entry, isCancelled: isCancelled) else { continue }
                } catch is CancellationError {
                    throw CancellationError()
                } catch {
                    record(value, error)
                    continue
                }
            }
            report.matched += 1
            batch.append(entry)
            if batch.count == 128 {
                if isCancelled() { throw CancellationError() }
                receive(batch)
                batch.removeAll(keepingCapacity: true)
            }
        }
        if let rootError { throw rootError }
        if isCancelled() { throw CancellationError() }
        if !batch.isEmpty { receive(batch) }
        report.contentCoverage = contentReader?.coverage
        return report
    }

    /// The real folder URL becomes the navigation root after an explicit open.
    public static func directoryURL(_ url: URL) throws -> URL {
        try requireLocalFileURL(url)
        let resolved = URL(fileURLWithPath: url.path, isDirectory: true).resolvingSymlinksInPath()
        let entry = try FileEntry(url: resolved)
        guard entry.kind == .directory else {
            throw POSIXError(.ENOTDIR)
        }
        return resolved
    }

    public static func address(_ path: String, relativeTo folder: URL) throws -> URL {
        guard !path.isEmpty, !path.utf8.contains(0) else { throw CocoaError(.fileReadInvalidFileName) }
        try requireLocalFileURL(folder)
        let location: URL
        if path == "~" {
            location = FileManager.default.homeDirectoryForCurrentUser
        } else if path.hasPrefix("~/") {
            location = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(String(path.dropFirst(2)))
        } else if path.hasPrefix("/") {
            location = URL(fileURLWithPath: path)
        } else {
            location = folder.appendingPathComponent(path)
        }
        return try directoryURL(location)
    }
}

func requireLocalFileURL(_ url: URL) throws {
    guard url.isFileURL, url.host == nil || url.host == "" || url.host == "localhost",
          url.path.hasPrefix("/"), !url.path.utf8.contains(0) else {
        throw CocoaError(.fileReadUnsupportedScheme)
    }
}

public struct FileSort: Sendable {
    public enum Column: Sendable {
        case name, fileExtension, type, size, modified
    }

    public let column: Column
    public let ascending: Bool
    public let foldersFirst: Bool

    public init(column: Column = .name, ascending: Bool = true, foldersFirst: Bool = true) {
        self.column = column
        self.ascending = ascending
        self.foldersFirst = foldersFirst
    }

    public func precedes(_ left: FileEntry, _ right: FileEntry) -> Bool {
        if foldersFirst && (left.kind == .directory) != (right.kind == .directory) {
            return left.kind == .directory
        }
        let order: ComparisonResult
        switch column {
        case .name:
            order = left.name.localizedStandardCompare(right.name)
        case .fileExtension:
            order = left.url.pathExtension.localizedStandardCompare(right.url.pathExtension)
        case .type:
            order = (left.typeDescription ?? "").localizedStandardCompare(right.typeDescription ?? "")
        case .size:
            if (left.size == nil) != (right.size == nil) { return left.size != nil }
            order = compare(left.size ?? 0, right.size ?? 0)
        case .modified:
            if (left.modified == nil) != (right.modified == nil) { return left.modified != nil }
            order = compare(left.modified ?? .distantPast, right.modified ?? .distantPast)
        }
        if order != .orderedSame { return order == (ascending ? .orderedAscending : .orderedDescending) }
        let nameOrder = left.name.localizedStandardCompare(right.name)
        if nameOrder != .orderedSame { return nameOrder == .orderedAscending }
        return left.url.path.utf8.lexicographicallyPrecedes(right.url.path.utf8)
    }
}

private func compare<T: Comparable>(_ left: T, _ right: T) -> ComparisonResult {
    if left < right { return .orderedAscending }
    if left > right { return .orderedDescending }
    return .orderedSame
}
