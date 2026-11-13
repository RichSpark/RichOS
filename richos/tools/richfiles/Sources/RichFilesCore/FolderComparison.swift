import Darwin
import Foundation

public enum FolderComparisonMode: String, CaseIterable, Identifiable, Sendable {
    case metadata = "Size and Modified Date"
    case contents = "File Contents"
    public var id: Self { self }
}

public struct FolderDifference: Identifiable, Sendable {
    public enum Kind: String, Sendable {
        case leftOnly = "Only in first folder"
        case rightOnly = "Only in second folder"
        case changed = "Different"
        case differentType = "Different item types"
        case unavailable = "Could not compare"
    }

    public let relativePath: String
    public let kind: Kind
    public let first: URL?
    public let second: URL?
    public var id: String { relativePath }
}

public struct FolderComparisonReport: Sendable {
    public let differences: [FolderDifference]
    public let matchingItems: Int
    public let unavailable: Int
    public let otherVolumes: Int
}

struct FolderInventorySnapshot {
    let folder: URL
    let identity: FileIdentity
    let entries: [String: FileInventory.Entry]
    let coverage: FileInventory.Report

    init(_ location: URL, includePackages: Bool = true, isCancelled: () -> Bool) throws {
        let root = try OpenFolder(location)
        var items: [String: FileInventory.Entry] = [:]
        coverage = try FileInventory.walk(root.url, includePackages: includePackages, isCancelled: isCancelled) {
            guard items.updateValue($0, forKey: $0.relativePath) == nil else {
                throw CocoaError(.fileReadInvalidFileName)
            }
        }
        try root.verifyLocation()
        folder = root.url
        identity = root.identity
        entries = items
    }
}

public extension StorageAnalysis {
    static func compareFolders(
        _ first: URL, _ second: URL, mode: FolderComparisonMode,
        isCancelled: () -> Bool = { false }
    ) throws -> FolderComparisonReport {
        let left = try FolderInventorySnapshot(first, isCancelled: isCancelled)
        let right = try FolderInventorySnapshot(second, isCancelled: isCancelled)
        return try compare(left, right, mode: mode, isCancelled: isCancelled)
    }

    internal static func compare(
        _ first: FolderInventorySnapshot, _ second: FolderInventorySnapshot,
        mode: FolderComparisonMode, isCancelled: () -> Bool
    ) throws -> FolderComparisonReport {
        let left = first.entries
        let right = second.entries
        let paths = Set(left.keys).union(right.keys).sorted {
            let order = $0.localizedStandardCompare($1)
            if order == .orderedSame { return $0.utf8.lexicographicallyPrecedes($1.utf8) }
            return order == .orderedAscending
        }
        var unavailable = first.coverage.unavailable + second.coverage.unavailable
        var matches = 0
        var differences: [FolderDifference] = []
        for path in paths {
            if isCancelled() { throw CancellationError() }
            let a = left[path]
            let b = right[path]
            let difference: FolderDifference.Kind?
            if let a, let b {
                do {
                    difference = try compare(a, b, mode: mode, isCancelled: isCancelled)
                } catch is CancellationError {
                    throw CancellationError()
                } catch {
                    difference = .unavailable
                }
            } else {
                difference = a == nil ? .rightOnly : .leftOnly
            }
            if let difference {
                if difference == .unavailable { unavailable += 1 }
                differences.append(FolderDifference(relativePath: path, kind: difference, first: a?.url, second: b?.url))
            } else {
                matches += 1
            }
        }
        return FolderComparisonReport(differences: differences, matchingItems: matches, unavailable: unavailable,
                                      otherVolumes: first.coverage.otherVolumes + second.coverage.otherVolumes)
    }

    private static func compare(
        _ left: FileInventory.Entry, _ right: FileInventory.Entry, mode: FolderComparisonMode,
        isCancelled: () -> Bool
    ) throws -> FolderDifference.Kind? {
        let type = left.status.st_mode & S_IFMT
        guard type == right.status.st_mode & S_IFMT else { return .differentType }
        if TextContentReader.isDataless(left.status) || TextContentReader.isDataless(right.status) {
            return .unavailable
        }
        switch type {
        case S_IFDIR:
            return nil
        case S_IFLNK:
            let leftTarget = try FileManager.default.destinationOfSymbolicLink(atPath: left.url.path)
            let rightTarget = try FileManager.default.destinationOfSymbolicLink(atPath: right.url.path)
            try checkVersion(left)
            try checkVersion(right)
            return leftTarget.utf8.elementsEqual(rightTarget.utf8) ? nil : .changed
        case S_IFREG:
            let a = FileVersion(left.status)
            let b = FileVersion(right.status)
            guard a.size == b.size else { return .changed }
            if mode == .metadata { return a.hasSameModifiedDate(as: b) ? nil : .changed }
            let firstHash = try digest(left.url, expected: a, isCancelled: isCancelled)
            let secondHash = try digest(right.url, expected: b, isCancelled: isCancelled)
            return firstHash == secondHash ? nil : .changed
        default:
            return .unavailable
        }
    }

    private static func checkVersion(_ entry: FileInventory.Entry) throws {
        var status = stat()
        guard lstat(entry.url.path, &status) == 0, FileVersion(status) == FileVersion(entry.status) else {
            throw FileOperationError.sourceChanged(entry.url)
        }
    }
}
