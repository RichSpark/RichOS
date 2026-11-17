import Darwin
import Foundation
import SQLite3

public struct GlanceMetadata: Sendable {
    public let note: String
    public let labels: [String]
    public let folder: String
    public let updated: Date
    public let embedded: Bool

    private struct Record: Decodable {
        struct Identity: Decodable, Equatable {
            let volume: UInt64
            let object: UInt64
            let size: UInt64
            let modified: String
            let changed: String

            init?(_ file: URL) {
                var info = stat()
                guard lstat(file.path, &info) == 0, info.st_mode & S_IFMT == S_IFREG, info.st_size >= 0 else { return nil }
                volume = UInt64(UInt32(bitPattern: info.st_dev))
                object = info.st_ino
                size = UInt64(info.st_size)
                modified = "\(info.st_mtimespec.tv_sec):\(info.st_mtimespec.tv_nsec)"
                changed = "\(info.st_ctimespec.tv_sec):\(info.st_ctimespec.tv_nsec)"
            }
        }

        struct Projection: Decodable {
            let mode: String
        }

        let path: String
        let identity: Identity
        let note: String
        let labels: [String]
        let folder: String
        let updated: UInt64
        let projection: Projection
    }

    public enum LookupError: Error { case catalogUnavailable }

    public static func read(for file: URL) throws -> Self? {
        guard let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
            throw LookupError.catalogUnavailable
        }
        return try read(for: file, catalog: support.appendingPathComponent("com.richos.app/RichCleaner/library.sqlite"))
    }

    static func read(for url: URL, catalog: URL) throws -> Self? {
        guard url.isFileURL else { return nil }
        let file = url.standardizedFileURL.resolvingSymlinksInPath()
        guard let fileIdentity = Record.Identity(file) else { return nil }
        var status = stat()
        guard lstat(catalog.path, &status) == 0 else {
            if errno == ENOENT { return nil }
            throw LookupError.catalogUnavailable
        }
        guard status.st_mode & S_IFMT == S_IFREG else { throw LookupError.catalogUnavailable }

        var database: OpaquePointer?
        defer { sqlite3_close(database) }
        guard sqlite3_open_v2(catalog.path, &database, SQLITE_OPEN_READONLY | SQLITE_OPEN_NOMUTEX, nil) == SQLITE_OK else {
            throw LookupError.catalogUnavailable
        }
        sqlite3_busy_timeout(database, 250)
        var statement: OpaquePointer?
        defer { sqlite3_finalize(statement) }
        // Same settled-record lookup as RichCleaner's Catalog.filed_context.
        let sql = """
            SELECT record FROM files WHERE volume = ?1 AND object = ?2 AND phase = '"filed"'
            """
        guard sqlite3_prepare_v2(database, sql, -1, &statement, nil) == SQLITE_OK else {
            throw LookupError.catalogUnavailable
        }
        let transient = unsafeBitCast(-1, to: sqlite3_destructor_type.self)
        guard sqlite3_bind_text(statement, 1, String(fileIdentity.volume), -1, transient) == SQLITE_OK,
              sqlite3_bind_text(statement, 2, String(fileIdentity.object), -1, transient) == SQLITE_OK else {
            throw LookupError.catalogUnavailable
        }
        let result = sqlite3_step(statement)
        if result == SQLITE_DONE { return nil }
        let bytes = Int(sqlite3_column_bytes(statement, 0))
        guard result == SQLITE_ROW, (1...512 * 1024).contains(bytes),
              let blob = sqlite3_column_blob(statement, 0) else { throw LookupError.catalogUnavailable }
        let record = try JSONDecoder().decode(Record.self, from: Data(bytes: blob, count: bytes))
        let savedFile = URL(fileURLWithPath: record.path).standardizedFileURL.resolvingSymlinksInPath()
        guard savedFile == file, record.identity == fileIdentity, Record.Identity(file) == fileIdentity else { return nil }
        return Self(note: record.note, labels: record.labels, folder: record.folder,
                    updated: Date(timeIntervalSince1970: Double(record.updated) / 1000),
                    embedded: record.projection.mode == "embedded")
    }
}
