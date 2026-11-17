import Darwin
import Foundation
import SQLite3
import XCTest
@testable import RichFilesCore

final class GlanceMetadataTests: XCTestCase {
    func testReadsOnlyTheSettledRecordForTheCurrentFileVersion() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let image = root.appendingPathComponent("Saved.png")
        let catalog = root.appendingPathComponent("library.sqlite")
        try Data([1, 2, 3]).write(to: image)
        XCTAssertNil(try GlanceMetadata.read(for: image, catalog: catalog))
        XCTAssertFalse(FileManager.default.fileExists(atPath: catalog.path))
        var stat = stat()
        XCTAssertEqual(lstat(image.path, &stat), 0)
        let volume = UInt64(UInt32(bitPattern: stat.st_dev))
        let object = stat.st_ino
        let payload: [String: Any] = [
            "path": image.path,
            "identity": [
                "volume": volume, "object": object, "size": stat.st_size,
                "modified": "\(stat.st_mtimespec.tv_sec):\(stat.st_mtimespec.tv_nsec)",
                "changed": "\(stat.st_ctimespec.tv_sec):\(stat.st_ctimespec.tv_nsec)"
            ],
            "note": "Saved note", "labels": ["design", "whiteboard"],
            "folder": "Work/Design", "updated": 1_700_000_000_000,
            "projection": ["mode": "embedded"]
        ]

        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(catalog.path, &db), SQLITE_OK)
        defer { sqlite3_close(db) }
        func write(_ sql: String) {
            XCTAssertEqual(sqlite3_exec(db, sql, nil, nil, nil), SQLITE_OK)
        }
        write("PRAGMA journal_mode = WAL")
        write("CREATE TABLE files (volume TEXT, object TEXT, phase TEXT, record TEXT)")
        let record = String(decoding: try JSONSerialization.data(withJSONObject: payload), as: UTF8.self)
            .replacingOccurrences(of: "'", with: "''")
        write("""
            INSERT INTO files VALUES ('\(volume)', '\(object)', '"draft"', '\(record)')
            """)
        XCTAssertNil(try GlanceMetadata.read(for: image, catalog: catalog))

        write("""
            UPDATE files SET phase = '"filed"'
            """)
        let info = try XCTUnwrap(GlanceMetadata.read(for: image, catalog: catalog))
        XCTAssertEqual(info.note, "Saved note")
        XCTAssertEqual(info.labels, ["design", "whiteboard"])
        XCTAssertEqual(info.folder, "Work/Design")
        XCTAssertEqual(info.updated, Date(timeIntervalSince1970: 1_700_000_000))
        XCTAssertTrue(info.embedded)

        let opened = Data([1])
        let attributeStatus = opened.withUnsafeBytes {
            setxattr(image.path, "com.richos.test.opened", $0.baseAddress, $0.count, 0, 0)
        }
        XCTAssertEqual(attributeStatus, 0)
        XCTAssertEqual(try GlanceMetadata.read(for: image, catalog: catalog)?.note, "Saved note")

        let other = root.appendingPathComponent("Other.png")
        try FileManager.default.copyItem(at: image, to: other)
        XCTAssertNil(try GlanceMetadata.read(for: other, catalog: catalog))
        try Data([9, 8, 7, 6]).write(to: image)
        XCTAssertNil(try GlanceMetadata.read(for: image, catalog: catalog))
    }
}
