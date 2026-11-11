import Foundation
import RichFilesCore
import XCTest

final class DirectoryReaderTests: XCTestCase {
    private func fixture(_ use: (URL) throws -> Void) throws {
        let temporary = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        let root = URL(fileURLWithPath: temporary, isDirectory: true)
            .appendingPathComponent("RichFilesBrowse-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        try use(root)
    }

    private func write(_ path: String, in root: URL, text: String = "test") throws {
        try Data(text.utf8).write(to: root.appendingPathComponent(path))
    }

    func testFolderAndRecursiveSearchKeepExplicitScope() throws {
        try fixture { root in
            let manager = FileManager.default
            for path in ["sub", ".hidden", "Example.app/Contents"] {
                try manager.createDirectory(at: root.appendingPathComponent(path), withIntermediateDirectories: true)
            }
            try write("file10.PNG", in: root)
            try write("file2.png", in: root)
            try write("sub/file1.png", in: root)
            try write("sub/file20.png", in: root, text: "a different size")
            try write(".hidden/file3.png", in: root)
            try write("Example.app/Contents/file4.png", in: root)
            try manager.createSymbolicLink(at: root.appendingPathComponent("loop"), withDestinationURL: root)

            var entries: [FileEntry] = []
            let list = try DirectoryReader.scan(root) { entries.append(contentsOf: $0) }
            XCTAssertEqual(list.inaccessible, 0)
            XCTAssertEqual(entries.sorted(by: FileSort().precedes).prefix(3).map(\.name),
                           ["sub", "Example.app", "file2.png"])
            XCTAssertEqual(entries.first { $0.name == "Example.app" }?.kind, .package)
            XCTAssertEqual(entries.first { $0.name == "loop" }?.kind, .symbolicLink)
            XCTAssertFalse(entries.contains { $0.name == "file1.png" || $0.name == ".hidden" })

            entries.removeAll()
            let query = FileQuery(name: .wildcard("file*.png"), extensions: [".PNG"], size: 4...4)
            let search = try DirectoryReader.scan(root, scope: .descendants, query: query) {
                entries.append(contentsOf: $0)
            }
            XCTAssertEqual(search.matched, 3)
            XCTAssertEqual(search.inaccessible, 0)
            XCTAssertEqual(entries.sorted(by: FileSort().precedes).map(\.name),
                           ["file1.png", "file2.png", "file10.PNG"])
            XCTAssertEqual(entries.first { $0.name == "file1.png" }?.url.deletingLastPathComponent().lastPathComponent,
                           "sub")

            entries.removeAll()
            _ = try DirectoryReader.scan(root, scope: .descendants, showHidden: true, query: query) {
                entries.append(contentsOf: $0)
            }
            XCTAssertTrue(entries.contains { $0.name == "file3.png" })
            XCTAssertFalse(entries.contains { $0.name == "file4.png" })
        }
    }

    func testListingPublishesBatchesAndCanBeCancelled() throws {
        try fixture { root in
            for index in 0..<270 {
                try write("entry-\(index)", in: root)
            }
            var sizes: [Int] = []
            let report = try DirectoryReader.scan(root) { sizes.append($0.count) }
            XCTAssertEqual(sizes, [128, 128, 14])
            XCTAssertEqual(report.matched, 270)
            XCTAssertEqual(report.inaccessible, 0)

            var calls = 0
            XCTAssertThrowsError(try DirectoryReader.scan(root, isCancelled: { calls > 0 }) { _ in
                calls += 1
            }) { error in
                XCTAssertTrue(error is CancellationError)
            }
            XCTAssertEqual(calls, 1)
        }
    }

    func testAddressAndPropertiesUseTheActualFile() throws {
        try fixture { root in
            let sub = root.appendingPathComponent("Research notes", isDirectory: true)
            try FileManager.default.createDirectory(at: sub, withIntermediateDirectories: false)
            try write("brief.txt", in: sub)
            let file = sub.appendingPathComponent("brief.txt")
            let folder = try DirectoryReader.address("Research notes", relativeTo: root)
            XCTAssertEqual(folder.path, sub.resolvingSymlinksInPath().path)
            XCTAssertThrowsError(try DirectoryReader.address("Research notes/brief.txt", relativeTo: root))
            XCTAssertThrowsError(try DirectoryReader.directoryURL(XCTUnwrap(URL(string: "https://example.invalid/"))))
            let properties = try FileProperties(url: file)
            XCTAssertEqual(properties.entry.name, "brief.txt")
            XCTAssertEqual(properties.entry.size, 4)
            XCTAssertNotNil(properties.permissions)
            let volume = try VolumeInfo(containing: file)
            XCTAssertNotNil(volume.totalBytes)
            XCTAssertEqual(volume.isReadOnly, false)
        }
    }
}
