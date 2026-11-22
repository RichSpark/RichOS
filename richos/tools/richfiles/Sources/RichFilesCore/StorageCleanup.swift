import Darwin
import Foundation

public struct CleanupCandidate: Sendable, Identifiable {
    public enum Kind: String, Sendable, CaseIterable {
        case folder, video, installer, archive, image, document

        public var title: String { rawValue.capitalized }

        static func classify(_ url: URL, directory: Bool) -> Self {
            let ext = url.pathExtension.lowercased()
            switch ext {
            case "dmg", "pkg", "mpkg", "iso", "xip": return .installer
            default: break
            }
            if directory { return .folder }
            switch ext {
            case "mov", "mp4", "mkv", "webm", "m4v", "avi": return .video
            case "zip", "tar", "gz", "bz2", "xz", "7z", "rar", "tgz": return .archive
            case "png", "jpeg", "jpg", "heic", "heif", "gif", "webp", "tiff", "avif", "bmp": return .image
            default: return .document
            }
        }
    }

    public let url: URL
    public let kind: Kind
    public let modified: Date
    public let isFolder: Bool
    public let canReview: Bool
    public let isCloudOnly: Bool
    public internal(set) var allocated: Int64
    public var id: URL { url }
    let version: FileVersion

    init(_ entry: FileInventory.Entry, package: Bool = false) {
        url = entry.url
        kind = Kind.classify(url, directory: entry.isDirectory && !package)
        modified = Date(timeIntervalSince1970: Double(entry.status.st_mtimespec.tv_sec)
                        + Double(entry.status.st_mtimespec.tv_nsec) / 1_000_000_000)
        isCloudOnly = TextContentReader.isDataless(entry.status)
        isFolder = entry.isDirectory && !package && !isCloudOnly
        canReview = Self.eligible(entry.status, at: url)
        allocated = Self.allocatedBytes(entry.status)
        version = FileVersion(entry.status)
    }

    static func allocatedBytes(_ status: stat) -> Int64 {
        let (bytes, overflow) = Int64(status.st_blocks).multipliedReportingOverflow(by: 512)
        return overflow ? .max : max(0, bytes)
    }

    static func eligible(_ status: stat, at url: URL) -> Bool {
        guard status.st_mode & S_IFMT == S_IFREG, status.st_uid == getuid(),
              !TextContentReader.isDataless(status) else { return false }
        var path = url.standardizedFileURL.path
        let data = "/System/Volumes/Data/"
        if path.hasPrefix(data) { path = "/" + path.dropFirst(data.count) }
        let parts = path.split(separator: "/")
        guard let root = parts.first,
              !["System", "Library", "Applications", "private", "dev", "bin", "sbin", "usr", "etc", "var"].contains(root)
        else { return false }
        if root == "Users" {
            let home = FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
            guard path.hasPrefix(home + "/"), !path.hasPrefix(home + "/Library/") else { return false }
        }
        return !parts.dropLast().contains { part in
            let name = part.lowercased()
            return [".trash", ".trashes", ".git"].contains(name) ||
                [".app", ".bundle", ".framework", ".photoslibrary", ".photolibrary", ".pkg", ".mpkg"].contains {
                    name.hasSuffix($0)
                }
        }
    }

    public func review() throws -> ReviewedFile {
        guard canReview else { throw CleanupReviewError.unavailable }
        let reference = try FileReference(url: url)
        let reviewed = try ReviewedFile(reference)
        guard reviewed.version == version else { throw FileOperationError.sourceChanged(url) }
        try FileOperations.verifyCleanupReview(reviewed)
        return reviewed
    }
}

public enum CleanupReviewError: LocalizedError {
    case unavailable
    case keepOneCopy

    public var errorDescription: String? {
        switch self {
        case .unavailable: "This item cannot be moved to Trash from Disk Usage."
        case .keepOneCopy: "Keep one unchanged copy before removing duplicates."
        }
    }
}

public struct CleanupInventory: Sendable {
    public let folder: URL
    public let rows: [CleanupCandidate]
    public let largeFiles: [CleanupCandidate]
    public let installers: [CleanupCandidate]
    public let allocated: Int64
    public let visited: Int
    public let unavailable: Int
    public let otherVolumes: Int
    public let limited: Bool
    let identity: FileIdentity
    fileprivate let branches: [String: CleanupBranch]

    /// Returns nil when no affected path is in this scan. A path at or above
    /// the root invalidates the entire inventory.
    public func affectedChildren(_ paths: [URL]) -> Set<String>? {
        let prefix = folder.path == "/" ? "/" : folder.path + "/"
        var children = Set<String>()
        for url in paths {
            let path = url.standardizedFileURL.path
            if path == folder.path || folder.path.hasPrefix(path == "/" ? "/" : path + "/") {
                return Set(branches.keys)
            }
            if path.hasPrefix(prefix), let child = path.dropFirst(prefix.count).split(separator: "/").first {
                children.insert(String(child))
            }
        }
        return children.isEmpty ? nil : children
    }
}

private struct CleanupBranch: Sendable {
    var row: CleanupCandidate?
    var files: [CleanupCandidate] = []
    var installers: [CleanupCandidate] = []
    var allocated: Int64 = 0
    var visited = 0
    var limited = false

    mutating func append(_ item: CleanupCandidate, installer: Bool) {
        if installer { installers.append(item) }
        else { files.append(item) }
        if installers.count > StorageAnalysis.cleanupLimit * 2 || files.count > StorageAnalysis.cleanupLimit * 2 {
            trim()
        }
    }

    mutating func trim() {
        if files.count > StorageAnalysis.cleanupLimit {
            limited = true
            files = Array(files.sorted(by: StorageAnalysis.cleanupOrder).prefix(StorageAnalysis.cleanupLimit))
        }
        if installers.count > StorageAnalysis.cleanupLimit {
            limited = true
            installers = Array(installers.sorted(by: StorageAnalysis.cleanupOrder).prefix(StorageAnalysis.cleanupLimit))
        }
    }
}

extension StorageAnalysis {
    static let cleanupLimit = 500
    static let minimumCleanupBytes: Int64 = 10_000_000

    static func cleanupOrder(_ first: CleanupCandidate, _ second: CleanupCandidate) -> Bool {
        if first.allocated != second.allocated { return first.allocated > second.allocated }
        let order = first.url.lastPathComponent.localizedStandardCompare(second.url.lastPathComponent)
        return order == .orderedSame ? first.url.path < second.url.path : order == .orderedAscending
    }

    public static func cleanup(
        in location: URL, previous: CleanupInventory? = nil, changed: [URL] = [],
        isCancelled: () -> Bool = { false }, onProgress: (CleanupInventory) -> Void = { _ in }
    ) throws -> CleanupInventory {
        let root = try OpenFolder(location)
        let refresh = previous.flatMap { $0.identity == root.identity ? $0.affectedChildren(changed) : nil }
        var branches = refresh == nil ? [:] : (previous?.branches ?? [:])
        for key in refresh ?? [] { branches.removeValue(forKey: key) }
        var visited = 0
        var lastUpdate = ProcessInfo.processInfo.systemUptime
        let scan = try FileInventory.walk(root.url, includePackages: false, onlyChildren: refresh,
                                          isCancelled: isCancelled) { entry in
            let parts = entry.relativePath.split(separator: "/", maxSplits: 1)
            guard let key = parts.first.map(String.init) else { return }
            let bytes = CleanupCandidate.allocatedBytes(entry.status)
            if parts.count == 1 {
                let package = entry.isDirectory && (try? entry.url.resourceValues(forKeys: [.isPackageKey]).isPackage) == true
                branches[key, default: CleanupBranch()].row = CleanupCandidate(entry, package: package)
            }
            let sum = branches[key, default: CleanupBranch()].allocated.addingReportingOverflow(bytes)
            branches[key, default: CleanupBranch()].allocated = sum.overflow ? .max : sum.partialValue
            branches[key, default: CleanupBranch()].visited += 1
            if bytes >= minimumCleanupBytes && entry.status.st_mode & S_IFMT == S_IFREG {
                let candidate = CleanupCandidate(entry)
                if !candidate.isCloudOnly {
                    branches[key, default: CleanupBranch()].append(candidate, installer: false)
                    if candidate.kind == .installer { branches[key, default: CleanupBranch()].append(candidate, installer: true) }
                }
            }
            visited += 1
            let now = ProcessInfo.processInfo.systemUptime
            if visited == 1 || (visited.isMultiple(of: 256) && now - lastUpdate >= 0.2) {
                lastUpdate = now
                onProgress(cleanupReport(root, branches: branches, unavailable: 0, otherVolumes: 0))
            }
        }
        try root.verifyLocation()
        for key in Array(branches.keys) { branches[key]?.trim() }
        return cleanupReport(root, branches: branches,
            unavailable: max(scan.unavailable, refresh == nil ? 0 : (previous?.unavailable ?? 0)),
            otherVolumes: max(scan.otherVolumes, refresh == nil ? 0 : (previous?.otherVolumes ?? 0)))
    }

    private static func cleanupReport(
        _ root: OpenFolder, branches: [String: CleanupBranch], unavailable: Int, otherVolumes: Int
    ) -> CleanupInventory {
        let rows = branches.values.compactMap { branch -> CleanupCandidate? in
            guard var row = branch.row else { return nil }
            row.allocated = branch.allocated
            return row
        }.sorted(by: cleanupOrder)
        let files = branches.values.flatMap(\.files).sorted(by: cleanupOrder)
        let installers = branches.values.flatMap(\.installers).sorted(by: cleanupOrder)
        let allocated = rows.reduce(Int64(0)) { total, row in
            let sum = total.addingReportingOverflow(row.allocated)
            return sum.overflow ? .max : sum.partialValue
        }
        return CleanupInventory(folder: root.url, rows: rows, largeFiles: Array(files.prefix(cleanupLimit)),
            installers: Array(installers.prefix(cleanupLimit)), allocated: allocated,
            visited: branches.values.reduce(0) { $0 + $1.visited }, unavailable: unavailable, otherVolumes: otherVolumes,
            limited: files.count > cleanupLimit || installers.count > cleanupLimit || branches.values.contains(where: \.limited),
            identity: root.identity, branches: branches)
    }
}
