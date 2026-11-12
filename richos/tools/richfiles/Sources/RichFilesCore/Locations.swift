import Darwin
import Foundation

public struct VolumeInfo: Sendable, Identifiable {
    public var id: URL { url }
    public let url: URL
    public let name: String
    public let availableBytes: Int64?
    public let totalBytes: Int64?
    public let isReadOnly: Bool?
    public let isLocal: Bool?
    public let isEjectable: Bool?
    public let supportsExclusiveRename: Bool?

    private static let keys: Set<URLResourceKey> = [
        .volumeURLKey, .volumeLocalizedNameKey, .volumeAvailableCapacityKey,
        .volumeTotalCapacityKey, .volumeIsReadOnlyKey, .volumeIsLocalKey, .volumeIsEjectableKey,
        .volumeSupportsExclusiveRenamingKey
    ]

    public init(containing location: URL) throws {
        try requireLocalFileURL(location)
        let values = try location.resourceValues(forKeys: Self.keys)
        guard let root = values.volume else { throw CocoaError(.fileReadUnknown) }
        url = root
        name = values.volumeLocalizedName ?? root.lastPathComponent
        availableBytes = values.volumeAvailableCapacity.map(Int64.init)
        totalBytes = values.volumeTotalCapacity.map(Int64.init)
        isReadOnly = values.volumeIsReadOnly
        isLocal = values.volumeIsLocal
        isEjectable = values.volumeIsEjectable
        supportsExclusiveRename = values.volumeSupportsExclusiveRenaming
    }

    /// Resolve each volume independently so a disconnected share cannot hide the other drives.
    public static func mounted() throws -> (volumes: [VolumeInfo], issues: [ScanIssue]) {
        guard let locations = FileManager.default.mountedVolumeURLs(
            includingResourceValuesForKeys: Array(keys),
            options: [.skipHiddenVolumes]
        ) else { throw CocoaError(.fileReadUnknown) }
        var volumes: [VolumeInfo] = []
        var issues: [ScanIssue] = []
        for location in locations {
            do {
                volumes.append(try VolumeInfo(containing: location))
            } catch {
                issues.append(ScanIssue(location: location, message: error.localizedDescription))
            }
        }
        return (volumes, issues)
    }
}

public struct FileProperties: Sendable {
    public let entry: FileEntry
    public let needsDownload: Bool
    public let permissions: Int?
    public let owner: String?
    public let group: String?

    public init(url: URL) throws {
        try requireLocalFileURL(url)
        let previous = try TextContentReader.disableDatalessMaterialization()
        defer { TextContentReader.restoreDatalessMaterialization(previous) }
        // A URL received from enumeration may still hold its earlier resource values.
        entry = try FileEntry(url: URL(fileURLWithPath: url.path))
        var status = stat()
        guard lstat(url.path, &status) == 0 else {
            throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
        }
        needsDownload = entry.needsDownload || TextContentReader.isDataless(status)
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        permissions = (attributes[.posixPermissions] as? NSNumber)?.intValue
        owner = attributes[.ownerAccountName] as? String
        group = attributes[.groupOwnerAccountName] as? String
    }
}
