import Darwin
import Foundation

struct InterruptedTransferNotice: Codable, Sendable {
    let id: UUID
    let destination: URL
    let move: Bool
    let total: Int
    let processed: Int
    let succeeded: Int
    let pendingSource: URL?
    let lastFailedSource: URL?
    let incompleteTail: Bool
    let recoverableMoves: Int
}

struct InterruptedTransfer: Sendable {
    let notice: InterruptedTransferNotice
    let moves: [BrowserCompletedMove]
    fileprivate let journal: LockedTransferJournal

    /// Keep the journal locked until the recovery state that includes its receipts is saved.
    func discard() throws { try journal.discard() }
}

/// One transfer worker owns the writer. Its open file description holds the lock
/// until the result is saved or the process exits.
final class TransferJournal: Sendable {
    private struct Header: Codable {
        var version = 1
        let id: UUID
        let destination: URL
        let move: Bool
        let total: Int
    }

    private enum Entry: Codable {
        case started(URL)
        case finished(succeeded: Bool, move: BrowserCompletedMove?)
    }

    private static let maximumBytes = 64 * 1024 * 1024
    private static let maximumLineBytes = 64 * 1024
    private static let maximumMoveReceipts = 256
    private static let newline: UInt8 = 10

    private let journal: LockedTransferJournal

    static func exists(at recoveryURL: URL) -> Bool {
        let url = recoveryURL.deletingLastPathComponent().appendingPathComponent(filename(for: recoveryURL))
        var status = stat()
        return lstat(url.path, &status) == 0
    }

    fileprivate static func filename(for recoveryURL: URL) -> String {
        "." + recoveryURL.lastPathComponent + ".transfer-journal"
    }

    init(recoveryURL: URL, id: UUID, destination: URL, move: Bool, total: Int) throws {
        guard total > 0, Self.isFilePath(destination),
              let journal = try LockedTransferJournal.open(for: recoveryURL, create: true) else {
            throw CocoaError(.fileWriteInvalidFileName)
        }
        self.journal = journal
        do {
            try append(Header(id: id, destination: destination, move: move, total: total))
        } catch {
            try? journal.discard()
            throw error
        }
    }

    func begin(_ source: URL) throws {
        guard Self.isFilePath(source) else { throw CocoaError(.fileWriteInvalidFileName) }
        try append(Entry.started(source), reserving: Self.maximumLineBytes)
    }

    func finish(succeeded: Bool, move: BrowserCompletedMove? = nil) throws {
        try append(Entry.finished(succeeded: succeeded, move: move))
    }

    func discard() throws { try journal.discard() }

    private func append(_ value: some Encodable, reserving: Int = 0) throws {
        var bytes = try JSONEncoder().encode(value)
        bytes.append(Self.newline)
        guard bytes.count <= Self.maximumLineBytes,
              try journal.size() <= Self.maximumBytes - bytes.count - reserving else {
            throw CocoaError(.fileWriteOutOfSpace)
        }
        try journal.file.write(contentsOf: bytes)
    }

    static func interrupted(at recoveryURL: URL) throws -> InterruptedTransfer? {
        guard let journal = try LockedTransferJournal.open(for: recoveryURL, create: false) else { return nil }
        let size = try journal.size()
        let bytes = try journal.file.read(upToCount: maximumBytes) ?? Data()
        guard let headerEnd = bytes.firstIndex(of: newline) else {
            // The writer cannot start a mutation until a complete header has been written.
            guard bytes.count < maximumLineBytes else { throw CocoaError(.fileReadCorruptFile) }
            try journal.discard()
            return nil
        }
        let decoder = JSONDecoder()
        guard headerEnd < maximumLineBytes,
              let header = try? decoder.decode(Header.self, from: bytes[..<headerEnd]),
              header.version == 1, header.total > 0, isFilePath(header.destination) else {
            throw CocoaError(.fileReadCorruptFile)
        }

        var pendingSource: URL?
        var lastFailedSource: URL?
        var processed = 0
        var succeeded = 0
        var moves: [BrowserCompletedMove] = []
        var cursor = bytes.index(after: headerEnd)
        records: while cursor < bytes.endIndex {
            guard let end = bytes[cursor...].firstIndex(of: newline),
                  end - cursor < maximumLineBytes,
                  let entry = try? decoder.decode(Entry.self, from: bytes[cursor..<end]) else { break }
            switch entry {
            case .started(let source):
                guard pendingSource == nil, processed < header.total, isFilePath(source) else { break records }
                pendingSource = source
            case .finished(let success, let move):
                guard let source = pendingSource, (move != nil) == (success && header.move) else { break records }
                if success { succeeded += 1 }
                else { lastFailedSource = source }
                if let move, move.original.url.absoluteString != move.moved.url.absoluteString {
                    if moves.count == maximumMoveReceipts { moves.removeFirst() }
                    moves.append(move)
                }
                processed += 1
                pendingSource = nil
            }
            cursor = bytes.index(after: end)
        }
        let incompleteTail = cursor < bytes.endIndex || size > bytes.count
        let notice = InterruptedTransferNotice(id: header.id, destination: header.destination, move: header.move,
                                               total: header.total, processed: processed, succeeded: succeeded,
                                               pendingSource: pendingSource, lastFailedSource: lastFailedSource,
                                               incompleteTail: incompleteTail, recoverableMoves: moves.count)
        return InterruptedTransfer(notice: notice, moves: moves, journal: journal)
    }

    private static func isFilePath(_ url: URL) -> Bool {
        url.isFileURL && url.path.hasPrefix("/")
    }
}

fileprivate struct LockedTransferJournal: Sendable {
    let file: FileHandle
    private let directory: FileHandle
    private let name: String

    static func open(for recoveryURL: URL, create: Bool) throws -> Self? {
        let parent = recoveryURL.deletingLastPathComponent()
        guard recoveryURL.isFileURL, parent.path.hasPrefix("/"), !recoveryURL.lastPathComponent.isEmpty else {
            throw CocoaError(.fileReadInvalidFileName)
        }
        if create {
            try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true,
                                                    attributes: [.posixPermissions: 0o700])
        }
        let directoryDescriptor = Darwin.open(parent.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard directoryDescriptor >= 0 else {
            if !create && errno == ENOENT { return nil }
            throw CocoaError(create ? .fileWriteNoPermission : .fileReadNoPermission)
        }
        let directory = FileHandle(fileDescriptor: directoryDescriptor, closeOnDealloc: true)
        var parentStatus = stat()
        guard fstat(directoryDescriptor, &parentStatus) == 0, parentStatus.st_uid == getuid(),
              parentStatus.st_mode & 0o077 == 0 else {
            throw CocoaError(create ? .fileWriteNoPermission : .fileReadNoPermission)
        }

        let name = TransferJournal.filename(for: recoveryURL)
        let access = create ? O_WRONLY | O_CREAT | O_EXCL | O_APPEND : O_RDONLY | O_NONBLOCK
        let descriptor = openat(directoryDescriptor, name, access | O_NOFOLLOW | O_CLOEXEC, 0o600)
        guard descriptor >= 0 else {
            if !create && errno == ENOENT { return nil }
            if create && errno == EEXIST { throw CocoaError(.fileWriteFileExists) }
            throw CocoaError(create ? .fileWriteNoPermission : .fileReadNoPermission)
        }
        let file = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        // A reader can acquire the new inode in the instant before the writer's
        // lock. It releases an empty journal and the writer rechecks the name.
        guard flock(descriptor, create ? LOCK_EX : LOCK_EX | LOCK_NB) == 0 else {
            if !create && errno == EWOULDBLOCK { return nil }
            throw CocoaError(create ? .fileWriteNoPermission : .fileReadNoPermission)
        }
        let journal = Self(file: file, directory: directory, name: name)
        guard journal.matchesNamedInode() else {
            var current = stat()
            if !create, fstatat(directoryDescriptor, name, &current, AT_SYMLINK_NOFOLLOW) != 0,
               errno == ENOENT { return nil }
            throw CocoaError(create ? .fileWriteNoPermission : .fileReadNoPermission)
        }
        return journal
    }

    func size() throws -> Int {
        var info = stat()
        guard fstat(file.fileDescriptor, &info) == 0, info.st_size >= 0 else {
            throw CocoaError(.fileReadCorruptFile)
        }
        return Int(info.st_size)
    }

    func discard() throws {
        guard matchesNamedInode(), unlinkat(directory.fileDescriptor, name, 0) == 0 else {
            throw CocoaError(.fileWriteNoPermission)
        }
    }

    private func matchesNamedInode() -> Bool {
        var opened = stat()
        var named = stat()
        return fstat(file.fileDescriptor, &opened) == 0 &&
            opened.st_mode & S_IFMT == S_IFREG && opened.st_uid == getuid() &&
            opened.st_mode & 0o077 == 0 && opened.st_nlink == 1 &&
            fstatat(directory.fileDescriptor, name, &named, AT_SYMLINK_NOFOLLOW) == 0 &&
            named.st_dev == opened.st_dev && named.st_ino == opened.st_ino
    }
}
