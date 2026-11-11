import CryptoKit
import Darwin
import Foundation

public struct DiskUsageRow: Identifiable, Sendable {
    public let url: URL
    public let isFolder: Bool
    public internal(set) var allocatedBytes: Int64 = 0
    public internal(set) var logicalBytes: Int64 = 0
    public internal(set) var files: Int = 0
    public var id: URL { url }
}

public struct DiskUsageReport: Sendable {
    public let rows: [DiskUsageRow]
    public let allocatedBytes: Int64
    public let unavailable: Int
    public let otherVolumes: Int
}

public struct DuplicateFileGroup: Identifiable, Sendable {
    public let sha256: String
    public let size: Int64
    public let files: [URL]
    public var id: String { sha256 }
}

public struct DuplicateFileReport: Sendable {
    public let groups: [DuplicateFileGroup]
    public let unavailable: Int
    public let otherVolumes: Int
}

private enum StorageReadError: LocalizedError {
    case notAFile, downloadRequired

    var errorDescription: String? {
        switch self {
        case .notAFile: return "Choose a regular file to calculate its checksum."
        case .downloadRequired: return "Download the file before calculating its checksum."
        }
    }
}

public enum StorageAnalysis {
    public static func checksum(
        of reference: FileReference, isCancelled: () -> Bool = { false }
    ) throws -> String {
        if isCancelled() { throw CancellationError() }
        let parent = try OpenFolder(reference.url.deletingLastPathComponent())
        let original = try parent.verifiedStatus(of: reference)
        return try digest(reference.url, expected: FileVersion(original), isCancelled: isCancelled)
    }

    public static func duplicates(
        in folder: URL, isCancelled: () -> Bool = { false }
    ) throws -> DuplicateFileReport {
        var sizes: [Int64: [(URL, FileVersion)]] = [:]
        var identities: Set<FileIdentity> = []
        var unavailable = 0
        let scan = try FileInventory.walk(folder, includePackages: false, isCancelled: isCancelled) { entry in
            guard entry.status.st_mode & S_IFMT == S_IFREG else { return }
            guard !TextContentReader.isDataless(entry.status) else { unavailable += 1; return }
            let version = FileVersion(entry.status)
            guard entry.status.st_nlink == 1 || identities.insert(version.identity).inserted else { return }
            sizes[version.size, default: []].append((entry.url, version))
        }

        var groups: [DuplicateFileGroup] = []
        for (size, candidates) in sizes where candidates.count > 1 {
            if isCancelled() { throw CancellationError() }
            var byHash: [String: [URL]] = [:]
            for (file, version) in candidates {
                do {
                    let hash = try digest(file, expected: version, isCancelled: isCancelled)
                    byHash[hash, default: []].append(file)
                } catch is CancellationError {
                    throw CancellationError()
                } catch {
                    unavailable += 1
                }
            }
            groups.append(contentsOf: byHash.compactMap { hash, files in
                guard files.count > 1 else { return nil }
                return DuplicateFileGroup(sha256: hash, size: size, files: files.sorted { $0.path < $1.path })
            })
        }
        groups.sort { $0.size == $1.size ? $0.sha256 < $1.sha256 : $0.size > $1.size }
        return DuplicateFileReport(groups: groups, unavailable: unavailable + scan.unavailable,
                                   otherVolumes: scan.otherVolumes)
    }

    /// Report allocated and logical bytes by immediate child. Links are counted as
    /// entries. Directory contents on other mounted filesystems are left out.
    public static func diskUsage(
        in folder: URL, isCancelled: () -> Bool = { false }
    ) throws -> DiskUsageReport {
        var rows: [String: DiskUsageRow] = [:]
        let scan = try FileInventory.walk(folder, isCancelled: isCancelled) { entry in
            guard let first = entry.relativePath.split(separator: "/", maxSplits: 1).first else { return }
            let name = String(first)
            var row = rows[name] ?? usageRow(entry, name: name)
            let (bytes, overflow) = Int64(entry.status.st_blocks).multipliedReportingOverflow(by: 512)
            row.allocatedBytes = addBytes(row.allocatedBytes, overflow ? .max : max(0, bytes))
            if entry.status.st_mode & S_IFMT == S_IFREG {
                row.logicalBytes = addBytes(row.logicalBytes, max(0, entry.status.st_size))
                row.files += 1
            }
            rows[name] = row
        }
        let ordered = rows.values.sorted {
            if $0.allocatedBytes != $1.allocatedBytes { return $0.allocatedBytes > $1.allocatedBytes }
            return $0.url.lastPathComponent.localizedStandardCompare($1.url.lastPathComponent) == .orderedAscending
        }
        return DiskUsageReport(rows: ordered, allocatedBytes: ordered.reduce(0) { addBytes($0, $1.allocatedBytes) },
                               unavailable: scan.unavailable, otherVolumes: scan.otherVolumes)
    }

    private static func usageRow(_ entry: FileInventory.Entry, name: String) -> DiskUsageRow {
        let isDirectory = entry.isDirectory || entry.relativePath != name
        let url = entry.root.appendingPathComponent(name, isDirectory: isDirectory)
        let browsable = isDirectory && (try? FileEntry(url: url).kind) == .directory
        return DiskUsageRow(url: url, isFolder: browsable)
    }

    private static func addBytes(_ a: Int64, _ b: Int64) -> Int64 {
        let (value, overflow) = a.addingReportingOverflow(b)
        return overflow ? .max : value
    }

    static func digest(_ url: URL, expected: FileVersion, isCancelled: () -> Bool) throws -> String {
        if isCancelled() { throw CancellationError() }
        let previous = try TextContentReader.disableDatalessMaterialization()
        defer { TextContentReader.restoreDatalessMaterialization(previous) }
        let descriptor = open(url.path, O_RDONLY | O_NOFOLLOW_ANY | O_NONBLOCK | O_CLOEXEC)
        guard descriptor >= 0 else {
            throw FileOperationError.system(operation: "Could not read the file", location: url, code: errno)
        }
        let file = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        defer { try? file.close() }

        var before = stat()
        guard fstat(descriptor, &before) == 0, before.st_mode & S_IFMT == S_IFREG else {
            throw StorageReadError.notAFile
        }
        guard !TextContentReader.isDataless(before) else { throw StorageReadError.downloadRequired }
        guard FileVersion(before) == expected else { throw FileOperationError.sourceChanged(url) }

        var hash = SHA256()
        var bytes: Int64 = 0
        while let block = try file.read(upToCount: 1 << 20), !block.isEmpty {
            if isCancelled() { throw CancellationError() }
            bytes += Int64(block.count)
            guard bytes <= expected.size else { throw FileOperationError.sourceChanged(url) }
            hash.update(data: block)
        }
        var after = stat()
        var named = stat()
        guard bytes == expected.size, fstat(descriptor, &after) == 0, lstat(url.path, &named) == 0,
              FileVersion(after) == expected, FileVersion(named) == expected else {
            throw FileOperationError.sourceChanged(url)
        }
        if isCancelled() { throw CancellationError() }
        return hash.finalize().map { String(format: "%02x", $0) }.joined()
    }
}

struct FileVersion: Equatable, Sendable {
    let identity: FileIdentity
    let size: Int64
    private let modified: Int
    private let modifiedNanoseconds: Int
    private let changed: Int
    private let changedNanoseconds: Int

    init(_ status: stat) {
        identity = FileIdentity(status)
        size = status.st_size
        modified = status.st_mtimespec.tv_sec
        modifiedNanoseconds = status.st_mtimespec.tv_nsec
        changed = status.st_ctimespec.tv_sec
        changedNanoseconds = status.st_ctimespec.tv_nsec
    }

    func hasSameModifiedDate(as other: Self) -> Bool {
        modified == other.modified && modifiedNanoseconds == other.modifiedNanoseconds
    }
}

enum FileInventory {
    struct Entry {
        let root: URL
        let url: URL
        let relativePath: String
        let status: stat

        var isDirectory: Bool { status.st_mode & S_IFMT == S_IFDIR }
    }

    struct Report {
        var unavailable = 0
        var otherVolumes = 0
    }

    static func walk(
        _ location: URL, includePackages: Bool = true, isCancelled: () -> Bool, receive: (Entry) throws -> Void
    ) throws -> Report {
        if isCancelled() { throw CancellationError() }
        let previous = try TextContentReader.disableDatalessMaterialization()
        defer { TextContentReader.restoreDatalessMaterialization(previous) }
        let folder = try OpenFolder(location)
        let path = folder.url.path
        let prefix = path == "/" ? "/" : path + "/"
        var report = Report()
        var rootError: Error?
        let options: FileManager.DirectoryEnumerationOptions = includePackages ? [] : [.skipsPackageDescendants]
        guard let enumerator = FileManager.default.enumerator(at: folder.url, includingPropertiesForKeys: [],
                                                               options: options, errorHandler: { url, error in
            if url.path == path { rootError = error; return false }
            report.unavailable += 1
            return true
        }) else { throw CocoaError(.fileReadUnknown) }

        while let item = enumerator.nextObject() as? URL {
            if isCancelled() { throw CancellationError() }
            var status = stat()
            guard item.path.hasPrefix(prefix), lstat(item.path, &status) == 0 else {
                report.unavailable += 1
                continue
            }
            let entry = Entry(root: folder.url, url: item, relativePath: String(item.path.dropFirst(prefix.count)),
                              status: status)
            if status.st_dev != folder.identity.device {
                if entry.isDirectory { enumerator.skipDescendants() }
                report.otherVolumes += 1
                continue
            }
            if entry.isDirectory && TextContentReader.isDataless(status) {
                enumerator.skipDescendants()
                report.unavailable += 1
            }
            try receive(entry)
        }
        if isCancelled() { throw CancellationError() }
        if let rootError { throw rootError }
        try folder.verifyLocation()
        return report
    }
}
