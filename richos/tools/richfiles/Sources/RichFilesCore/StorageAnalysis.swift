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

public enum StorageAnalysis {
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
        _ location: URL, isCancelled: () -> Bool, receive: (Entry) throws -> Void
    ) throws -> Report {
        if isCancelled() { throw CancellationError() }
        let folder = try OpenFolder(location)
        let path = folder.url.path
        let prefix = path == "/" ? "/" : path + "/"
        var report = Report()
        var rootError: Error?
        guard let enumerator = FileManager.default.enumerator(at: folder.url, includingPropertiesForKeys: [],
                                                               options: [], errorHandler: { url, error in
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
            try receive(entry)
        }
        if isCancelled() { throw CancellationError() }
        if let rootError { throw rootError }
        try folder.verifyLocation()
        return report
    }
}
