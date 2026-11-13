import CryptoKit
import Darwin
import Foundation

struct TreeInventory {
    let root: FileReference
    let rootVersion: FileVersion
    let descendants: String
    private let entries: [FileInventory.Entry]

    init(_ root: FileReference, expected: FileVersion? = nil,
         isCancelled: () -> Bool = { false }) throws {
        self.root = root
        let parent = try OpenFolder(root.url.deletingLastPathComponent())
        let status = try parent.verifiedStatus(of: root)
        rootVersion = FileVersion(status)
        guard status.st_mode & S_IFMT == S_IFDIR, expected == nil || rootVersion == expected else {
            throw FileOperationError.sourceChanged(root.url)
        }
        let snapshot = try FolderInventorySnapshot(root.url, isCancelled: isCancelled)
        guard snapshot.identity == root.identity, snapshot.coverage.unavailable == 0, snapshot.coverage.otherVolumes == 0 else {
            throw FileOperationError.copyVerificationFailed(root.url)
        }
        entries = snapshot.entries.values.sorted { $0.relativePath.utf8.lexicographicallyPrecedes($1.relativePath.utf8) }
        var hash = SHA256()
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        for entry in entries {
            if isCancelled() { throw CancellationError() }
            let kind = entry.status.st_mode & S_IFMT
            guard !TextContentReader.isDataless(entry.status),
                  kind == S_IFDIR || kind == S_IFREG || kind == S_IFLNK else {
                throw FileOperationError.unsupportedCopySource(entry.url)
            }
            Self.add(Data(entry.relativePath.utf8), to: &hash)
            Self.add(try encoder.encode(FileVersion(entry.status)), to: &hash)
        }
        descendants = Self.finish(hash)
        guard try FileVersion(parent.verifiedStatus(of: root)) == rootVersion else {
            throw FileOperationError.sourceChanged(root.url)
        }
    }

    func verify(isCancelled: () -> Bool = { false }) throws {
        let current = try Self(root, expected: rootVersion, isCancelled: isCancelled)
        guard current.descendants == descendants else { throw FileOperationError.sourceChanged(root.url) }
    }

    func contentDigest(isCancelled: () -> Bool) throws -> String {
        var hash = SHA256()
        // Use equivalent Unicode names across filesystem normalization rules.
        let ordered = entries.sorted {
            $0.relativePath.precomposedStringWithCanonicalMapping.utf8
                .lexicographicallyPrecedes($1.relativePath.precomposedStringWithCanonicalMapping.utf8)
        }
        for entry in ordered {
            if isCancelled() { throw CancellationError() }
            Self.add(Data(entry.relativePath.precomposedStringWithCanonicalMapping.utf8), to: &hash)
            let version = FileVersion(entry.status)
            let value: String
            switch entry.status.st_mode & S_IFMT {
            case S_IFDIR: value = "directory"
            case S_IFLNK:
                let target = try FileManager.default.destinationOfSymbolicLink(atPath: entry.url.path)
                var status = stat()
                guard lstat(entry.url.path, &status) == 0, FileVersion(status) == version else {
                    throw FileOperationError.sourceChanged(entry.url)
                }
                value = "link:" + target
            default:
                value = "file:" + (try StorageAnalysis.digest(entry.url, expected: version, isCancelled: isCancelled))
            }
            Self.add(Data(value.utf8), to: &hash)
        }
        return Self.finish(hash)
    }

    private static func add(_ bytes: Data, to hash: inout SHA256) {
        var count = UInt64(bytes.count).bigEndian
        withUnsafeBytes(of: &count) { hash.update(bufferPointer: $0) }
        hash.update(data: bytes)
    }

    private static func finish(_ hash: SHA256) -> String {
        hash.finalize().map { String(format: "%02x", $0) }.joined()
    }
}

struct ReviewedTree: Codable, Sendable {
    let reference: FileReference
    let rootVersion: FileVersion
    let descendants: String

    init(_ reference: FileReference, descendants: String) throws {
        let parent = try OpenFolder(reference.url.deletingLastPathComponent())
        rootVersion = FileVersion(try parent.verifiedStatus(of: reference))
        self.reference = reference
        self.descendants = descendants
    }

    func verify(isCancelled: () -> Bool = { false }) throws {
        let actual = try TreeInventory(reference, expected: rootVersion, isCancelled: isCancelled)
        guard actual.descendants == descendants else { throw FileOperationError.sourceChanged(reference.url) }
    }
}
