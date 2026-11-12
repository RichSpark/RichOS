import Darwin
import Foundation
import XCTest
@testable import RichFilesCore

final class TextContentSearchTests: XCTestCase {
    func testSearchUsesLiteralContentsAndReportsFilesOutsideCoverage() throws {
        try withFixture { fixture in
            let root = fixture.appendingPathComponent("Search", isDirectory: true)
            let nested = root.appendingPathComponent("Nested", isDirectory: true)
            try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)
            try write("CASE-FOLDED *? phrase", in: root, as: "Visible.TXT")
            try write("a filename match alone", in: root, as: "Case-Folded *? Phrase.txt")
            try write("case-folded *? phrase", in: nested, as: "Details.JSON")
            try write("case-folded *? phrase", in: root, as: "Other.bin")
            try write("case-folded *? phrase", in: root, as: ".Hidden.txt")
            try Data([0xFF, 0xFE]).write(to: root.appendingPathComponent("Invalid.txt"))
            let oversized = try write("", in: root, as: "Large.txt")
            let handle = try FileHandle(forWritingTo: oversized)
            try handle.truncate(atOffset: UInt64(TextContentQuery.maximumFileBytes + 1))
            try handle.close()
            let outside = try write("case-folded *? phrase", in: fixture, as: "Outside.txt")
            try FileManager.default.createSymbolicLink(
                at: root.appendingPathComponent("Link.txt"), withDestinationURL: outside
            )
            let fifo = root.appendingPathComponent("Pipe.txt")
            XCTAssertEqual(mkfifo(fifo.path, 0o600), 0)

            let query = try XCTUnwrap(TextContentQuery(phrase: "case-folded *? phrase"))
            var names: [String] = []
            let shallow = try DirectoryReader.scan(root, content: query) { names += $0.map(\.name) }
            XCTAssertEqual(names, ["Visible.TXT"])
            XCTAssertEqual(shallow.contentCoverage?.searchedFiles, 2)
            XCTAssertEqual(shallow.contentCoverage?.skippedItems, 5)
            XCTAssertEqual(shallow.inaccessible, 0)

            names.removeAll()
            let recursive = try DirectoryReader.scan(root, scope: .descendants, content: query) {
                names += $0.map(\.name)
            }
            XCTAssertEqual(Set(names), ["Details.JSON", "Visible.TXT"])
            XCTAssertEqual(recursive.contentCoverage?.searchedFiles, 3)
            XCTAssertEqual(recursive.contentCoverage?.skippedItems, 5)
            XCTAssertEqual(recursive.inaccessible, 0)

            names.removeAll()
            let filtered = try DirectoryReader.scan(
                root, scope: .descendants, query: FileQuery(extensions: ["json"]), content: query
            ) { names += $0.map(\.name) }
            XCTAssertEqual(names, ["Details.JSON"])
            XCTAssertEqual(filtered.contentCoverage?.searchedFiles, 1)
            XCTAssertEqual(filtered.contentCoverage?.skippedItems, 0)
            XCTAssertEqual(filtered.inaccessible, 0)

            names.removeAll()
            let filename = try DirectoryReader.scan(
                root, scope: .descendants, query: FileQuery(extensions: ["json"])
            ) { names += $0.map(\.name) }
            XCTAssertEqual(names, ["Details.JSON"])
            XCTAssertNil(filename.contentCoverage)
        }
    }

    func testReadBudgetAndThreadPolicyAreBoundedToTheScan() throws {
        try withFixture { root in
            try write("needle0123456789", in: root, as: "One.txt")
            try write("needle0123456789", in: root, as: "Two.txt")
            let query = try XCTUnwrap(TextContentQuery(phrase: "needle", byteBudget: 24))
            let type = IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES
            let scope = IOPOL_SCOPE_THREAD
            let previous = getiopolicy_np(type, scope)
            var received = 0
            let report = try DirectoryReader.scan(root, content: query) { entries in
                XCTAssertEqual(getiopolicy_np(type, scope), IOPOL_MATERIALIZE_DATALESS_FILES_OFF)
                received += entries.count
            }
            XCTAssertEqual(received, 1)
            XCTAssertEqual(report.contentCoverage?.searchedFiles, 1)
            XCTAssertEqual(report.contentCoverage?.skippedItems, 1)
            XCTAssertEqual(report.contentCoverage?.bytesRead, 16)
            XCTAssertEqual(getiopolicy_np(type, scope), previous)
        }
    }

    func testReadStopsWhenCancelledBetweenChunks() throws {
        try withFixture { root in
            let url = root.appendingPathComponent("Long.txt")
            let size = 70_000
            try Data(repeating: 0x41, count: size).write(to: url)
            let entry = try FileEntry(url: url)
            let query = try XCTUnwrap(TextContentQuery(phrase: "a"))
            let reader = try TextContentReader(folder: root, query: query)

            XCTAssertThrowsError(try reader.matches(entry, isCancelled: { reader.coverage.bytesRead > 0 })) {
                XCTAssertTrue($0 is CancellationError)
            }
            XCTAssertGreaterThan(reader.coverage.bytesRead, 0)
            XCTAssertLessThan(reader.coverage.bytesRead, size)
            XCTAssertEqual(reader.coverage.searchedFiles, 0)
        }
    }

    func testReadRefusesAnAncestorReplacedWithALink() throws {
        try withFixture { fixture in
            let root = fixture.appendingPathComponent("Search", isDirectory: true)
            let nested = root.appendingPathComponent("Nested", isDirectory: true)
            let outside = fixture.appendingPathComponent("Outside", isDirectory: true)
            try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)
            try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: false)
            let initial = try write("selected folder", in: nested, as: "Notes.txt")
            try write("needle", in: outside, as: "Notes.txt")
            let entry = try FileEntry(url: initial)
            let query = try XCTUnwrap(TextContentQuery(phrase: "needle"))
            let reader = try TextContentReader(folder: root, query: query)
            try FileManager.default.moveItem(at: nested, to: fixture.appendingPathComponent("Moved"))
            try FileManager.default.createSymbolicLink(at: nested, withDestinationURL: outside)

            XCTAssertFalse(try reader.matches(entry, isCancelled: { false }))
            XCTAssertEqual(reader.coverage.bytesRead, 0)
            XCTAssertEqual(reader.coverage.skippedItems, 1)
        }
    }

    @discardableResult
    private func write(_ text: String, in folder: URL, as name: String) throws -> URL {
        let url = folder.appendingPathComponent(name)
        try Data(text.utf8).write(to: url)
        return url
    }

    private func withFixture(_ body: (URL) throws -> Void) throws {
        let temporary = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        let fixture = URL(fileURLWithPath: temporary, isDirectory: true)
            .resolvingSymlinksInPath()
            .appendingPathComponent("RichFilesContent-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: fixture, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: fixture) }
        try body(fixture)
    }
}
