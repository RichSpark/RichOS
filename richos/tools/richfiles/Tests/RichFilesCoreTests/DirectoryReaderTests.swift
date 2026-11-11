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

    func testDiskUsageCountsNestedFilesAndHiddenItemsWithoutFollowingLinks() throws {
        try fixture { root in
            let folder = root.appendingPathComponent("Measure", isDirectory: true)
            let nested = folder.appendingPathComponent("Example.app/Contents", isDirectory: true)
            let outside = root.appendingPathComponent("Outside", isDirectory: true)
            try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)
            try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true)
            try write("code", in: nested, text: "twelve bytes")
            try write(".hidden", in: folder, text: "hidden")
            try write("other", in: outside, text: "outside the selected folder")
            try FileManager.default.createSymbolicLink(at: folder.appendingPathComponent("Link"), withDestinationURL: outside)

            let report = try StorageAnalysis.diskUsage(in: folder)
            XCTAssertEqual(report.unavailable, 0)
            XCTAssertEqual(report.otherVolumes, 0)
            XCTAssertEqual(report.allocatedBytes, report.rows.reduce(0) { $0 + $1.allocatedBytes })
            let sizes = Dictionary(uniqueKeysWithValues: report.rows.map { ($0.url.lastPathComponent, $0.logicalBytes) })
            XCTAssertEqual(sizes, ["Example.app": 12, ".hidden": 6, "Link": 0])
            XCTAssertThrowsError(try StorageAnalysis.diskUsage(in: folder, isCancelled: { true })) {
                XCTAssertTrue($0 is CancellationError)
            }
        }
    }

    func testChecksumsAndDuplicatesCompareContentsAndIgnoreHardLinksAndSymlinks() throws {
        try fixture { root in
            try write("first.txt", in: root, text: "abc")
            try write("second.txt", in: root, text: "abc")
            try write("same-size.txt", in: root, text: "xyz")
            let first = root.appendingPathComponent("first.txt")
            try FileManager.default.linkItem(at: first, to: root.appendingPathComponent("hard-link.txt"))
            try FileManager.default.createSymbolicLink(at: root.appendingPathComponent("symbolic.txt"),
                                                       withDestinationURL: first)
            let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            let reference = try FileReference(url: first)
            XCTAssertEqual(try StorageAnalysis.checksum(of: reference), expected)

            let report = try StorageAnalysis.duplicates(in: root)
            XCTAssertEqual(report.unavailable, 0)
            let group = try XCTUnwrap(report.groups.first)
            XCTAssertEqual(report.groups.count, 1)
            XCTAssertEqual(group.sha256, expected)
            XCTAssertEqual(group.files.count, 2)
            XCTAssertTrue(group.files.contains { $0.lastPathComponent == "second.txt" })
            XCTAssertFalse(group.files.contains { ["same-size.txt", "symbolic.txt"].contains($0.lastPathComponent) })

            _ = try FileOperations.rename(reference, to: "moved.txt")
            try write("first.txt", in: root, text: "abc")
            XCTAssertThrowsError(try StorageAnalysis.checksum(of: reference)) {
                guard case FileOperationError.sourceChanged = $0 else { return XCTFail("\($0)") }
            }
        }
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

    func testWildcardMatchesOneCharacterInUnicodeNames() throws {
        try fixture { root in
            for name in ["café.txt", "caf🍵.txt", "cafes.txt", "cafe.md"] {
                try write(name, in: root)
            }
            var entries: [FileEntry] = []
            _ = try DirectoryReader.scan(root, query: FileQuery(name: .wildcard("*CAF?.TXT"))) {
                entries.append(contentsOf: $0)
            }
            XCTAssertEqual(Set(entries.map(\.name)), ["café.txt", "caf🍵.txt"])
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
