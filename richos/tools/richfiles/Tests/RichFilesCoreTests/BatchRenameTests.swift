import Foundation
import RichFilesCore
import XCTest

final class BatchRenameTests: XCTestCase {
    private func fixture(_ use: (URL, [FileReference]) throws -> Void) throws {
        let temporary = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        let root = URL(fileURLWithPath: temporary, isDirectory: true)
            .appendingPathComponent("RichFilesBatch-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let names = ["a.txt", "b.txt"]
        for name in names {
            try Data(name.utf8).write(to: root.appendingPathComponent(name))
        }
        try use(root, names.map { try FileReference(url: root.appendingPathComponent($0)) })
    }

    func testPreviewAndUndoNumberedRename() throws {
        try fixture { root, files in
            let plan = try BatchRenamePlan.preview(
                Array(files.reversed()), in: root, rule: .number(prefix: "scan ", start: 7, minimumDigits: 3)
            )
            XCTAssertEqual(plan.proposals.map(\.name), ["scan 007.txt", "scan 008.txt"])
            XCTAssertTrue(plan.canApply)
            XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("scan 007.txt").path))

            let results = try plan.apply()
            XCTAssertEqual(results.count, 2)
            for outcome in results.reversed() {
                guard case .renamed(let receipt) = outcome else {
                    return XCTFail("Expected a completed rename and its undo receipt")
                }
                _ = try FileOperations.undo(receipt)
            }
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path).sorted(),
                           ["a.txt", "b.txt"])
            XCTAssertEqual(try Data(contentsOf: root.appendingPathComponent("a.txt")), Data("a.txt".utf8))
            XCTAssertEqual(try Data(contentsOf: root.appendingPathComponent("b.txt")), Data("b.txt".utf8))
        }
    }

    func testPreviewCannotOverwriteAnExistingName() throws {
        try fixture { root, files in
            let plan = try BatchRenamePlan.preview(files, in: root, rule: .replace(search: "a", replacement: "b"))
            XCTAssertEqual(plan.proposals.map(\.status), [.duplicateName, .unchanged])
            XCTAssertFalse(plan.canApply)
            XCTAssertThrowsError(try plan.apply())
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path).sorted(),
                           ["a.txt", "b.txt"])
        }
    }

    func testNewCollisionAfterPreviewLeavesAllOriginalsInPlace() throws {
        try fixture { root, files in
            let plan = try BatchRenamePlan.preview(files, in: root, rule: .prefix("file-"))
            XCTAssertTrue(plan.canApply)
            let occupant = root.appendingPathComponent("file-b.txt")
            try Data("existing".utf8).write(to: occupant)
            XCTAssertThrowsError(try plan.apply())
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path).sorted(),
                           ["a.txt", "b.txt", "file-b.txt"])
            XCTAssertEqual(try Data(contentsOf: occupant), Data("existing".utf8))
        }
    }
}
