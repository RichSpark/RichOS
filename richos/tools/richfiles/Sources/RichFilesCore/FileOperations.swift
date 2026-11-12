import Darwin
import Foundation

public struct FileIdentity: Hashable, Codable, Sendable {
    public let device: Int32
    public let inode: UInt64
    private let birthSeconds: Int
    private let birthNanoseconds: Int

    init(_ status: stat) {
        device = status.st_dev
        inode = status.st_ino
        birthSeconds = status.st_birthtimespec.tv_sec
        birthNanoseconds = status.st_birthtimespec.tv_nsec
    }
}

/// The entry selected by the caller, including the identity of its parent folder.
/// A symbolic link has its own identity and never stands for the item it points to.
public struct FileReference: Equatable, Codable, Sendable {
    public let url: URL
    public let identity: FileIdentity
    let parentIdentity: FileIdentity
    fileprivate var parent: URL { url.deletingLastPathComponent() }
    fileprivate var name: String { url.lastPathComponent }

    private enum CodingKeys: String, CodingKey {
        case url, identity, parentIdentity
    }

    public init(url: URL) throws {
        try requireLocalFileURL(url)
        let name = url.lastPathComponent
        try FileOperations.validateName(name)
        let parent = try OpenFolder(url.deletingLastPathComponent())
        self = try parent.reference(named: name)
    }

    fileprivate init(folder: OpenFolder, name: String, status: stat) {
        url = folder.childURL(name)
        identity = FileIdentity(status)
        parentIdentity = folder.identity
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        let location = try values.decode(URL.self, forKey: .url)
        try requireLocalFileURL(location)
        try FileOperations.validateName(location.lastPathComponent)
        guard location.path == location.standardizedFileURL.path else {
            throw CocoaError(.fileReadInvalidFileName)
        }
        url = location
        identity = try values.decode(FileIdentity.self, forKey: .identity)
        parentIdentity = try values.decode(FileIdentity.self, forKey: .parentIdentity)
    }
}

public struct MoveReceipt: Codable, Sendable {
    public let original: FileReference
    public let moved: FileReference
}

public struct CopyReceipt: Sendable {
    public let copied: FileReference
    /// If temporary cleanup failed, the copy is still complete and the source still exists.
    public let remainingStagingFolder: URL?
}

public enum CopyConflictPolicy: Sendable {
    case refuse
    case keepBoth
}

public struct TrashReceipt: Codable, Sendable {
    public let original: FileReference
    public let trashedAt: Date
    public let trashLocation: URL?
    /// Some volumes or providers cannot give us the identity of the trashed entry.
    public let trashed: FileReference?
}

public struct ReviewedFile: Codable, Sendable {
    public let reference: FileReference
    public let size: Int64
    public let modified: Date
    fileprivate let version: FileVersion

    public init(_ reference: FileReference) throws {
        let parent = try OpenFolder(reference.parent)
        let status = try parent.verifiedStatus(of: reference)
        guard status.st_mode & S_IFMT == S_IFREG else {
            throw FileOperationError.replacementRequiresFiles
        }
        self.reference = reference
        size = status.st_size
        modified = Date(timeIntervalSince1970: Double(status.st_mtimespec.tv_sec)
                        + Double(status.st_mtimespec.tv_nsec) / 1_000_000_000)
        version = FileVersion(status)
    }
}

public struct ReplaceReceipt: Sendable {
    public let installed: ReviewedFile
    public let previous: TrashReceipt
    public let remainingStagingFolder: URL?
}

public struct CrossVolumeMoveReceipt: Codable, Sendable {
    public let installed: ReviewedFile
    public let backup: TrashReceipt
    public var original: FileReference { backup.original }
}

public struct CrossVolumeMoveResult: Sendable {
    public let receipt: CrossVolumeMoveReceipt
    public let remainingStagingFolder: URL?
}

public enum FileOperationError: LocalizedError {
    case invalidName
    case sourceChanged(URL)
    case folderChanged(URL)
    case folderNotEmpty(URL)
    case destinationExists(URL)
    case noAvailableCopyName(URL)
    case moveNeedsReview(source: URL, destination: URL)
    case crossVolumeMove
    case crossVolumeMoveNeedsReview(source: URL, destination: URL)
    case copyVerificationFailed(URL)
    case destinationInsideSource
    case unsupportedCopySource(URL)
    case trashLocationUnknown
    case replacementRequiresFiles
    case replacementNeedsReview(URL)
    case system(operation: String, location: URL, code: Int32)
    case stagingCleanupFailed(folder: URL, originalError: Error, cleanupError: Error)

    public var errorDescription: String? {
        switch self {
        case .invalidName:
            return "Enter a single filename or folder name."
        case .sourceChanged(let url):
            return "The selected item changed at \(url.path). Select it again."
        case .folderNotEmpty(let url): return "The folder is no longer empty. Left it in place at \(url.path)."
        case .folderChanged(let url):
            return "The folder changed at \(url.path). Open it again."
        case .destinationExists(let url):
            return "An item already exists at \(url.path)."
        case .noAvailableCopyName(let url):
            return "Choose a different name for the copy at \(url.path)."
        case .moveNeedsReview(let source, let destination):
            return "The system accepted the move, but RichFiles could not verify the result. Check \(source.path) and \(destination.path) before retrying."
        case .crossVolumeMove:
            return "This item cannot be moved between volumes. Choose Copy to keep the source."
        case .crossVolumeMoveNeedsReview(let source, let destination):
            return "A verified file was placed at \(destination.path). Check \(source.path) and Trash before retrying the move."
        case .copyVerificationFailed(let url):
            return "The copy of \(url.path) did not match. The original stays where it was."
        case .destinationInsideSource:
            return "Choose a destination outside the source folder."
        case .unsupportedCopySource(let url):
            return "This type of item cannot be copied from \(url.path)."
        case .trashLocationUnknown:
            return "The system did not provide a verifiable location in Trash."
        case .replacementRequiresFiles:
            return "Choose two different regular files to replace one with the other."
        case .replacementNeedsReview(let url):
            return "RichFiles could not finish replacing \(url.path). Check that folder and Trash before retrying."
        case .system(let operation, let location, let code):
            return "\(operation) at \(location.path): \(String(cString: strerror(code)))"
        case .stagingCleanupFailed(let folder, let originalError, let cleanupError):
            return "\(originalError.localizedDescription) Temporary cleanup at \(folder.path) also failed: \(cleanupError.localizedDescription)"
        }
    }
}

public enum FileOperations {
    public static func validateName(_ name: String) throws {
        guard !name.isEmpty, name != ".", name != "..",
              !name.utf8.contains(0), !name.contains("/") else {
            throw FileOperationError.invalidName
        }
    }

    public static func createEmptyFile(in folder: URL, named name: String) throws -> FileReference {
        try validateName(name)
        let parent = try OpenFolder(folder)
        let descriptor = openat(parent.descriptor, name, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0o666)
        guard descriptor >= 0 else {
            throw destinationError("Could not create the file", parent.childURL(name), errno)
        }
        defer { _ = close(descriptor) }
        let status = try fileStatus(descriptor, parent.childURL(name))
        return FileReference(folder: parent, name: name, status: status)
    }

    public static func createFolder(in folder: URL, named name: String) throws -> FileReference {
        try createFolder(in: OpenFolder(folder), named: name)
    }

    static func createFolder(in parent: OpenFolder, named name: String) throws -> FileReference {
        try validateName(name)
        try parent.verifyLocation()
        guard mkdirat(parent.descriptor, name, 0o777) == 0 else {
            throw destinationError("Could not create the folder", parent.childURL(name), errno)
        }
        return try parent.reference(named: name)
    }

    /// Includes dangling symbolic links. Only a missing entry produces nil.
    public static func existingReference(in folder: URL, named name: String) throws -> FileReference? {
        try validateName(name)
        return try OpenFolder(folder).existingReference(named: name)
    }

    public static func rename(_ source: FileReference, to name: String) throws -> MoveReceipt {
        try validateName(name)
        let parent = try OpenFolder(source.parent)
        try parent.requireIdentity(source.parentIdentity)
        return try relocate(source, into: parent, named: name)
    }

    /// Move one entry on its current volume. The system atomically refuses an occupied target.
    public static func move(
        _ source: FileReference, into folder: URL, named name: String? = nil
    ) throws -> MoveReceipt {
        let name = name ?? source.name
        try validateName(name)
        return try relocate(source, into: OpenFolder(folder), named: name)
    }

    public static func undo(_ receipt: MoveReceipt) throws -> FileReference {
        try reverse(receipt).moved
    }

    /// A cross-volume move publishes a verified copy first and keeps the original
    /// in native Trash for undo. Ordinary directories still use the Copy action.
    public static func moveAcrossVolumes(
        _ source: FileReference, into folder: URL,
        expectedFolder: FileIdentity? = nil,
        conflictPolicy: CopyConflictPolicy = .refuse,
        isCancelled: () -> Bool = { false },
        onProgress: (Int64, Int64) -> Void = { _, _ in },
        onVerify: () -> Void = {}
    ) throws -> CrossVolumeMoveResult {
        if isCancelled() { throw CancellationError() }
        let parent = try OpenFolder(source.parent)
        let original = try parent.verifiedStatus(of: source)
        let destination = try OpenFolder(folder)
        if let expectedFolder { try destination.requireIdentity(expectedFolder) }
        guard original.st_mode & S_IFMT == S_IFREG, parent.identity.device != destination.identity.device else {
            throw FileOperationError.crossVolumeMove
        }
        if conflictPolicy == .refuse { try destination.requireFree(source.name) }

        let version = FileVersion(original)
        let (installed, remaining) = try stageCopy(
            source, from: parent, initial: original, into: destination, isCancelled: isCancelled, onProgress: onProgress
        ) { staged in
            onVerify()
            let sourceDigest = try StorageAnalysis.digest(source.url, expected: version, isCancelled: isCancelled)
            let stageFolder = try OpenFolder(staged.parent)
            let stageVersion = FileVersion(try stageFolder.verifiedStatus(of: staged))
            let copiedDigest = try StorageAnalysis.digest(staged.url, expected: stageVersion, isCancelled: isCancelled)
            guard stageVersion.size == version.size, sourceDigest == copiedDigest else {
                throw FileOperationError.copyVerificationFailed(source.url)
            }
            try parent.verifyLocation()
            guard FileVersion(try parent.verifiedStatus(of: source)) == version else {
                throw FileOperationError.sourceChanged(source.url)
            }
            return try publishCopy(staged, into: destination, named: source.name, policy: conflictPolicy,
                                   isCancelled: isCancelled)
        }

        // Once published, finish the move or report both locations for review.
        // Never remove a published file after a source-retirement failure.
        do {
            let reviewed = try ReviewedFile(installed)
            try destination.verifyLocation()
            guard FileVersion(try parent.verifiedStatus(of: source)) == version else {
                throw FileOperationError.sourceChanged(source.url)
            }
            let backup = try trash(source)
            guard backup.trashed != nil else { throw FileOperationError.trashLocationUnknown }
            return CrossVolumeMoveResult(receipt: CrossVolumeMoveReceipt(installed: reviewed, backup: backup),
                                         remainingStagingFolder: remaining)
        } catch {
            throw FileOperationError.crossVolumeMoveNeedsReview(source: source.url, destination: installed.url)
        }
    }

    public static func reverse(_ receipt: CrossVolumeMoveReceipt) throws -> CrossVolumeMoveReceipt {
        let current = receipt.installed.reference
        let folder = try OpenFolder(current.parent)
        try validateReviewed(receipt.installed, in: folder)
        let restored = try restore(receipt.backup)
        do {
            let original = try ReviewedFile(restored)
            try validateReviewed(receipt.installed, in: folder)
            let backup = try trash(current)
            guard backup.trashed != nil else { throw FileOperationError.trashLocationUnknown }
            return CrossVolumeMoveReceipt(installed: original, backup: backup)
        } catch {
            throw FileOperationError.crossVolumeMoveNeedsReview(source: current.url, destination: restored.url)
        }
    }

    /// Returns the verified inverse receipt so the operation can safely be replayed.
    public static func reverse(_ receipt: MoveReceipt) throws -> MoveReceipt {
        let originalFolder = try OpenFolder(receipt.original.parent)
        try originalFolder.requireIdentity(receipt.original.parentIdentity)
        return try relocate(receipt.moved, into: originalFolder, named: receipt.original.name)
    }

    /// Call on a worker. Ordinary files can cancel during writes. Every copy
    /// checks cancellation before its completed result is published.
    public static func copy(
        _ source: FileReference,
        into folder: URL,
        named name: String? = nil,
        conflictPolicy: CopyConflictPolicy = .refuse,
        isCancelled: () -> Bool = { false },
        onProgress: (Int64, Int64) -> Void = { _, _ in }
    ) throws -> CopyReceipt {
        try copy(source, into: OpenFolder(folder), named: name, conflictPolicy: conflictPolicy,
                        isCancelled: isCancelled, onProgress: onProgress)
    }

    static func copy(
        _ source: FileReference, into destination: OpenFolder, named name: String? = nil,
        conflictPolicy: CopyConflictPolicy = .refuse, expected: FileVersion? = nil,
        isCancelled: () -> Bool, onProgress: (Int64, Int64) -> Void = { _, _ in }
    ) throws -> CopyReceipt {
        if isCancelled() { throw CancellationError() }
        let name = name ?? source.name
        try validateName(name)
        let sourceFolder = try OpenFolder(source.parent)
        let initial = try sourceFolder.verifiedStatus(of: source)
        if let expected, FileVersion(initial) != expected { throw FileOperationError.sourceChanged(source.url) }
        let kind = initial.st_mode & S_IFMT
        guard kind == S_IFREG || kind == S_IFDIR || kind == S_IFLNK else {
            throw FileOperationError.unsupportedCopySource(source.url)
        }
        try checkAncestry(source: initial, destination: destination)
        if conflictPolicy == .refuse { try destination.requireFree(name) }

        let (copied, remaining) = try stageCopy(
            source, from: sourceFolder, initial: initial, into: destination, isCancelled: isCancelled, onProgress: onProgress
        ) { staged in
            try publishCopy(staged, into: destination, named: name, policy: conflictPolicy, isCancelled: isCancelled)
        }
        return CopyReceipt(copied: copied, remainingStagingFolder: remaining)
    }

    /// The incoming file is staged first. Keep the previous file in native Trash
    /// and publish exclusively, so a file arriving late keeps its place.
    public static func replaceCopy(
        _ source: ReviewedFile, replacing target: ReviewedFile,
        isCancelled: () -> Bool = { false }
    ) throws -> ReplaceReceipt {
        try replaceCopy(source, replacing: target, isCancelled: isCancelled, sendToTrash: trash)
    }

    static func replaceCopy(
        _ source: ReviewedFile, replacing target: ReviewedFile,
        isCancelled: () -> Bool, sendToTrash: (FileReference) throws -> TrashReceipt
    ) throws -> ReplaceReceipt {
        if isCancelled() { throw CancellationError() }
        let input = source.reference
        let occupant = target.reference
        guard input.identity != occupant.identity else { throw FileOperationError.replacementRequiresFiles }
        let parent = try OpenFolder(input.parent)
        let initial = try parent.verifiedStatus(of: input)
        let destination = try OpenFolder(occupant.parent)
        guard FileVersion(initial) == source.version else { throw FileOperationError.sourceChanged(input.url) }
        try validateReviewed(target, in: destination)

        let ((installed, previous), remaining) = try stageCopy(
            input, from: parent, initial: initial, into: destination, isCancelled: isCancelled, onProgress: { _, _ in }
        ) { staged in
            if isCancelled() { throw CancellationError() }
            return try replaceKeepingBackup(target, in: destination, sendToTrash: sendToTrash) {
                try relocate(staged, into: destination, named: occupant.name).moved
            }
        }
        return ReplaceReceipt(installed: installed, previous: previous, remainingStagingFolder: remaining)
    }

    public static func restoreReplacedFile(_ displaced: TrashReceipt, replacing installed: ReviewedFile) throws -> ReplaceReceipt {
        try restoreReplacedFile(displaced, replacing: installed, sendToTrash: trash)
    }

    static func restoreReplacedFile(
        _ displaced: TrashReceipt, replacing installed: ReviewedFile,
        sendToTrash: (FileReference) throws -> TrashReceipt
    ) throws -> ReplaceReceipt {
        let path = displaced.original.url.path
        guard path == installed.reference.url.path, let archived = displaced.trashed else {
            throw FileOperationError.trashLocationUnknown
        }
        let trashFolder = try OpenFolder(archived.parent)
        _ = try trashFolder.verifiedStatus(of: archived)
        let destination = try OpenFolder(displaced.original.parent)
        let (restored, backup) = try replaceKeepingBackup(installed, in: destination, sendToTrash: sendToTrash) {
            try restore(displaced)
        }
        return ReplaceReceipt(installed: restored, previous: backup, remainingStagingFolder: nil)
    }

    private static func replaceKeepingBackup(
        _ target: ReviewedFile, in destination: OpenFolder,
        sendToTrash: (FileReference) throws -> TrashReceipt, publish: () throws -> FileReference
    ) throws -> (ReviewedFile, TrashReceipt) {
        try validateReviewed(target, in: destination)
        let occupant = target.reference
        let previous = try sendToTrash(occupant)
        guard previous.trashed?.identity == occupant.identity else {
            throw FileOperationError.replacementNeedsReview(occupant.url)
        }
        let installed: FileReference
        do {
            installed = try publish()
        } catch FileOperationError.moveNeedsReview {
            throw FileOperationError.replacementNeedsReview(occupant.url)
        } catch {
            let publicationError = error
            do { _ = try restore(previous) }
            catch { throw FileOperationError.replacementNeedsReview(occupant.url) }
            throw publicationError
        }
        guard let verified = try? ReviewedFile(installed) else {
            throw FileOperationError.replacementNeedsReview(occupant.url)
        }
        return (verified, previous)
    }

    private static func validateReviewed(_ file: ReviewedFile, in parent: OpenFolder) throws {
        try parent.verifyLocation()
        let current = try parent.verifiedStatus(of: file.reference)
        guard FileVersion(current) == file.version else { throw FileOperationError.sourceChanged(file.reference.url) }
    }

    private static func stageCopy<Result>(
        _ source: FileReference, from sourceFolder: OpenFolder, initial: stat, into destination: OpenFolder,
        isCancelled: () -> Bool, onProgress: (Int64, Int64) -> Void, publish: (FileReference) throws -> Result
    ) throws -> (Result, URL?) {

        let manager = FileManager.default
        let stagingURL = try manager.url(
            for: .itemReplacementDirectory, in: .userDomainMask,
            appropriateFor: destination.url, create: true
        )
        let staging = try OpenFolder(stagingURL)
        let result: Result
        do {
            guard staging.identity.device == destination.identity.device else {
                throw FileOperationError.system(
                    operation: "Could not stage the copy on the destination volume",
                    location: destination.url, code: EXDEV
                )
            }
            try staging.verifyLocation()
            guard fchmod(staging.descriptor, 0o700) == 0 else {
                throw FileOperationError.system(
                    operation: "Could not secure the temporary folder", location: staging.url, code: errno
                )
            }
            try sourceFolder.verifyLocation()
            let stagedURL = staging.childURL(source.name)
            switch initial.st_mode & S_IFMT {
            case S_IFREG:
                try copyRegularFile(source, from: sourceFolder, into: staging, size: initial.st_size,
                                    isCancelled: isCancelled, onProgress: onProgress)
            case S_IFDIR:
                let flags = copyfile_flags_t(COPYFILE_ALL | COPYFILE_RECURSIVE | COPYFILE_NOFOLLOW
                                             | COPYFILE_EXCL | COPYFILE_DATA_SPARSE)
                try copyUsingNative(to: stagedURL, sourceDevice: initial.st_dev, isCancelled: isCancelled) { state in
                    copyfile(source.url.path, stagedURL.path, state, flags)
                }
            default:
                try manager.copyItem(at: source.url, to: stagedURL)
            }
            let staged = try staging.reference(named: source.name)
            if isCancelled() { throw CancellationError() }
            try sourceFolder.verifyLocation()
            let current = try sourceFolder.verifiedStatus(of: source)
            guard FileVersion(current) == FileVersion(initial) else {
                throw FileOperationError.sourceChanged(source.url)
            }
            result = try publish(staged)
        } catch {
            let originalError = error
            do {
                try removeStagingFolder(staging)
            } catch {
                throw FileOperationError.stagingCleanupFailed(
                    folder: staging.url, originalError: originalError, cleanupError: error
                )
            }
            throw originalError
        }
        do {
            try removeStagingFolder(staging)
            return (result, nil)
        } catch {
            return (result, staging.url)
        }
    }

    private static func copyRegularFile(
        _ source: FileReference, from parent: OpenFolder, into staging: OpenFolder, size: Int64,
        isCancelled: () -> Bool, onProgress: (Int64, Int64) -> Void
    ) throws {
        let input = openat(parent.descriptor, source.name, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard input >= 0 else {
            throw FileOperationError.system(operation: "Could not read the source", location: source.url, code: errno)
        }
        defer { _ = close(input) }
        let actual = try fileStatus(input, source.url)
        guard actual.st_mode & S_IFMT == S_IFREG, FileIdentity(actual) == source.identity else {
            throw FileOperationError.sourceChanged(source.url)
        }

        // APFS can clone an already verified file without streaming the bytes.
        if parent.identity.device == staging.identity.device,
           fclonefileat(input, staging.descriptor, source.name, UInt32(CLONE_ACL)) == 0 { return }

        let output = openat(staging.descriptor, source.name, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0o600)
        let stagedURL = staging.childURL(source.name)
        guard output >= 0 else {
            throw FileOperationError.system(operation: "Could not stage the copy", location: stagedURL, code: errno)
        }
        defer { _ = close(output) }
        try copyFileDescriptors(input, output, to: stagedURL, size: size,
                                isCancelled: isCancelled, onProgress: onProgress)
        if parent.identity.device != staging.identity.device, fsync(output) != 0 {
            throw FileOperationError.system(operation: "Could not flush the copy", location: stagedURL, code: errno)
        }
    }

    static func copyFileDescriptors(
        _ input: Int32, _ output: Int32, to location: URL, size: Int64,
        isCancelled: () -> Bool, onProgress: (Int64, Int64) -> Void
    ) throws {
        try copyUsingNative(to: location, size: size, isCancelled: isCancelled, onProgress: onProgress) {
            fcopyfile(input, output, $0, copyfile_flags_t(COPYFILE_ALL | COPYFILE_DATA_SPARSE))
        }
    }

    private static func copyUsingNative(
        to location: URL, size: Int64? = nil, sourceDevice: Int32? = nil,
        isCancelled: () -> Bool, onProgress: (Int64, Int64) -> Void = { _, _ in },
        copy: (copyfile_state_t) -> Int32
    ) throws {
        guard let state = copyfile_state_alloc() else {
            throw FileOperationError.system(operation: "Could not start the copy", location: location, code: ENOMEM)
        }
        defer { _ = copyfile_state_free(state) }
        try withoutActuallyEscaping(isCancelled) { cancel in
            try withoutActuallyEscaping(onProgress) { progress in
                let context = CopyCallbacks(size: size, sourceDevice: sourceDevice, cancel: cancel, progress: progress)
                let pointer = Unmanaged.passUnretained(context).toOpaque()
                let callback = unsafeBitCast(CopyCallbacks.status, to: UnsafeRawPointer.self)
                guard copyfile_state_set(state, UInt32(COPYFILE_STATE_STATUS_CTX), pointer) == 0,
                      copyfile_state_set(state, UInt32(COPYFILE_STATE_STATUS_CB), callback) == 0 else {
                    throw FileOperationError.system(operation: "Could not start the copy", location: location, code: errno)
                }
                let result = withExtendedLifetime(context) { copy(state) }
                let code = errno
                if context.cancelled { throw CancellationError() }
                if let rejected = context.rejected { throw FileOperationError.unsupportedCopySource(rejected) }
                if result != 0 {
                    throw FileOperationError.system(operation: "Could not complete the copy", location: location, code: code)
                }
            }
        }
    }

    private static func publishCopy(
        _ staged: FileReference,
        into destination: OpenFolder,
        named name: String,
        policy: CopyConflictPolicy,
        isCancelled: () -> Bool
    ) throws -> FileReference {
        if isCancelled() { throw CancellationError() }
        do {
            return try relocate(staged, into: destination, named: name).moved
        } catch FileOperationError.destinationExists where policy == .keepBoth {
            // The staged copy remains in place after a name collision, so try another
            // name without copying the source a second time.
        }
        let kind = try FileEntry(url: staged.url).kind
        let target = destination.childURL(name)
        let ext = kind == .directory ? "" : target.pathExtension
        let basename = ext.isEmpty ? name : target.deletingPathExtension().lastPathComponent
        let suffix = ext.isEmpty ? "" : "." + ext
        for index in 2...9_999 {
            if isCancelled() { throw CancellationError() }
            let candidate = "\(basename) (\(index))" + suffix
            if try destination.existingStatus(named: candidate) != nil { continue }
            do {
                return try relocate(staged, into: destination, named: candidate).moved
            } catch FileOperationError.destinationExists {
                continue
            }
        }
        throw FileOperationError.noAvailableCopyName(target)
    }

    public static func trash(_ source: FileReference) throws -> TrashReceipt {
        try trash(source, expected: nil)
    }

    static func trash(_ source: FileReference, expected: FileVersion?) throws -> TrashReceipt {
        let parent = try OpenFolder(source.parent)
        try parent.verifyLocation()
        let status = try parent.verifiedStatus(of: source)
        if let expected, FileVersion(status) != expected { throw FileOperationError.sourceChanged(source.url) }
        var resultingURL: NSURL?
        try FileManager.default.trashItem(at: source.url, resultingItemURL: &resultingURL)
        let location = resultingURL as URL?
        let reference = location.flatMap { try? FileReference(url: $0) }
        return TrashReceipt(
            original: source, trashedAt: Date(), trashLocation: location,
            trashed: reference?.identity == source.identity ? reference : nil
        )
    }

    public static func trashEmptyFolder(_ source: FileReference) throws -> TrashReceipt {
        let parent = try OpenFolder(source.parent)
        let before = try parent.verifiedStatus(of: source)
        guard before.st_mode & S_IFMT == S_IFDIR else { throw FileOperationError.sourceChanged(source.url) }
        let folder = try OpenFolder(source.url)
        try folder.requireIdentity(source.identity)
        guard try FileManager.default.contentsOfDirectory(atPath: folder.url.path).isEmpty else {
            throw FileOperationError.folderNotEmpty(source.url)
        }
        return try trash(source, expected: FileVersion(before))
    }

    /// Restores to the original folder identity, or to an alternate folder explicitly supplied
    /// by the caller. An occupied name remains occupied.
    public static func restore(_ receipt: TrashReceipt, into alternateFolder: URL? = nil) throws -> FileReference {
        guard let source = receipt.trashed else { throw FileOperationError.trashLocationUnknown }
        let folder = try OpenFolder(alternateFolder ?? receipt.original.parent)
        if alternateFolder == nil { try folder.requireIdentity(receipt.original.parentIdentity) }
        return try relocate(source, into: folder, named: receipt.original.name).moved
    }

    private static func relocate(
        _ source: FileReference, into destination: OpenFolder, named name: String
    ) throws -> MoveReceipt {
        let parent = try OpenFolder(source.parent)
        let status = try parent.verifiedStatus(of: source)
        try destination.verifyLocation()
        guard source.identity.device == destination.identity.device else {
            throw FileOperationError.crossVolumeMove
        }
        if parent.identity == destination.identity && source.name.utf8.elementsEqual(name.utf8) {
            return MoveReceipt(original: source, moved: source)
        }
        try checkAncestry(source: status, destination: destination)
        if let occupied = try destination.existingStatus(named: name) {
            // Darwin permits a case-only rename with RENAME_EXCL. An unrelated hard
            // link to the same inode is still an occupied name.
            guard parent.identity == destination.identity,
                  FileIdentity(occupied) == source.identity,
                  source.name.caseInsensitiveCompare(name) == .orderedSame,
                  fpathconf(parent.descriptor, _PC_CASE_SENSITIVE) == 0 else {
                throw FileOperationError.destinationExists(destination.childURL(name))
            }
        }
        guard renameatx_np(
            parent.descriptor, source.name, destination.descriptor, name, UInt32(RENAME_EXCL)
        ) == 0 else {
            throw destinationError("Could not move the item", destination.childURL(name), errno)
        }
        // A different process can change a name during the syscall. Confirm the object
        // before recording an undo. An uncertain result must never trigger a rollback.
        let uncertain = FileOperationError.moveNeedsReview(
            source: source.url, destination: destination.childURL(name)
        )
        guard let moved = try? destination.reference(named: name),
              moved.identity == source.identity else { throw uncertain }
        do {
            try destination.verifyLocation()
        } catch {
            throw uncertain
        }
        return MoveReceipt(original: source, moved: moved)
    }

    private static func checkAncestry(source: stat, destination: OpenFolder) throws {
        guard source.st_mode & S_IFMT == S_IFDIR else { return }
        let sourceIdentity = FileIdentity(source)
        var descriptor = dup(destination.descriptor)
        guard descriptor >= 0 else {
            throw FileOperationError.system(
                operation: "Could not inspect the destination", location: destination.url, code: errno
            )
        }
        defer { _ = close(descriptor) }
        var current = destination.identity
        var seen: Set<FileIdentity> = []
        while true {
            guard current != sourceIdentity, seen.insert(current).inserted else {
                throw FileOperationError.destinationInsideSource
            }
            let parent = openat(descriptor, "..", O_RDONLY | O_DIRECTORY | O_CLOEXEC)
            guard parent >= 0 else {
                throw FileOperationError.system(
                    operation: "Could not inspect a parent of the destination",
                    location: destination.url, code: errno
                )
            }
            _ = close(descriptor)
            descriptor = parent
            let next = FileIdentity(try fileStatus(descriptor, destination.url))
            if next == current { return }
            current = next
        }
    }

    private static func removeStagingFolder(_ staging: OpenFolder) throws {
        try staging.verifyLocation()
        try FileManager.default.removeItem(at: staging.url)
    }
}

private final class CopyCallbacks {
    let size: Int64?
    let sourceDevice: Int32?
    let cancel: () -> Bool
    let progress: (Int64, Int64) -> Void
    var cancelled = false
    var rejected: URL?

    init(size: Int64?, sourceDevice: Int32?, cancel: @escaping () -> Bool,
         progress: @escaping (Int64, Int64) -> Void) {
        self.size = size
        self.sourceDevice = sourceDevice
        self.cancel = cancel
        self.progress = progress
    }

    static let status: copyfile_callback_t = { what, stage, state, source, _, pointer in
        guard let pointer else { return COPYFILE_QUIT }
        let context = Unmanaged<CopyCallbacks>.fromOpaque(pointer).takeUnretainedValue()
        if context.cancelled || context.rejected != nil { return COPYFILE_QUIT }
        if context.cancel() {
            context.cancelled = true
            return COPYFILE_QUIT
        }
        if let sourceDevice = context.sourceDevice, let source, stage == COPYFILE_START,
           what == COPYFILE_RECURSE_FILE || what == COPYFILE_RECURSE_DIR {
            var status = stat()
            let read = lstat(source, &status)
            let kind = status.st_mode & S_IFMT
            if read != 0 || status.st_dev != sourceDevice || (kind != S_IFREG && kind != S_IFDIR && kind != S_IFLNK) {
                context.rejected = URL(fileURLWithPath: String(cString: source))
                return COPYFILE_QUIT
            }
        }
        if let size = context.size, what == COPYFILE_COPY_DATA && stage == COPYFILE_PROGRESS {
            var written: off_t = 0
            if copyfile_state_get(state, UInt32(COPYFILE_STATE_COPIED), &written) == 0 {
                context.progress(written, size)
            }
        }
        return COPYFILE_CONTINUE
    }
}

final class OpenFolder {
    let url: URL
    let descriptor: Int32
    let identity: FileIdentity

    init(_ url: URL) throws {
        let folder = try DirectoryReader.directoryURL(url)
        let fd = open(folder.path, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW)
        guard fd >= 0 else {
            throw FileOperationError.system(operation: "Could not open the folder", location: folder, code: errno)
        }
        do {
            identity = FileIdentity(try fileStatus(fd, folder))
        } catch {
            _ = close(fd)
            throw error
        }
        self.url = folder
        descriptor = fd
    }

    deinit { _ = close(descriptor) }

    func childURL(_ name: String) -> URL {
        url.appendingPathComponent(name, isDirectory: false)
    }

    func requireIdentity(_ expected: FileIdentity) throws {
        guard identity == expected else { throw FileOperationError.folderChanged(url) }
    }

    func verifyLocation() throws {
        var status = stat()
        guard lstat(url.path, &status) == 0, FileIdentity(status) == identity else {
            throw FileOperationError.folderChanged(url)
        }
    }

    func reference(named name: String) throws -> FileReference {
        FileReference(folder: self, name: name, status: try status(named: name))
    }

    func existingReference(named name: String) throws -> FileReference? {
        guard let status = try existingStatus(named: name) else { return nil }
        return FileReference(folder: self, name: name, status: status)
    }

    func verifiedStatus(of reference: FileReference) throws -> stat {
        try requireIdentity(reference.parentIdentity)
        let current: stat
        do {
            current = try status(named: reference.name)
        } catch FileOperationError.system(_, _, ENOENT) {
            throw FileOperationError.sourceChanged(reference.url)
        }
        guard FileIdentity(current) == reference.identity else {
            throw FileOperationError.sourceChanged(reference.url)
        }
        return current
    }

    func requireFree(_ name: String) throws {
        if try existingStatus(named: name) != nil {
            throw FileOperationError.destinationExists(childURL(name))
        }
    }

    func existingStatus(named name: String) throws -> stat? {
        do {
            return try status(named: name)
        } catch FileOperationError.system(_, _, ENOENT) {
            return nil
        }
    }

    private func status(named name: String) throws -> stat {
        var status = stat()
        guard fstatat(descriptor, name, &status, AT_SYMLINK_NOFOLLOW) == 0 else {
            throw FileOperationError.system(
                operation: "Could not inspect the item", location: childURL(name), code: errno
            )
        }
        return status
    }
}

private func fileStatus(_ descriptor: Int32, _ url: URL) throws -> stat {
    var status = stat()
    guard fstat(descriptor, &status) == 0 else {
        throw FileOperationError.system(operation: "Could not inspect the open item", location: url, code: errno)
    }
    return status
}

private func destinationError(_ operation: String, _ url: URL, _ code: Int32) -> FileOperationError {
    switch code {
    case EEXIST, ENOTEMPTY:
        return .destinationExists(url)
    case EXDEV:
        return .crossVolumeMove
    default:
        return .system(operation: operation, location: url, code: code)
    }
}
