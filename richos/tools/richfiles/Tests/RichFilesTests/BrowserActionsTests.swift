import AppKit
import RichFilesCore
import XCTest
@testable import RichFiles

final class BrowserActionsTests: XCTestCase {
    @MainActor
    func testCompletedRenameCanBeUndoneAfterReopeningTheRecoveryStore() async throws {
        try await withFixture { root, _, board in
            let source = try write("original", named: "Original.txt", in: root)
            let store = root.appendingPathComponent("Private", isDirectory: true)
                .appendingPathComponent("Recovery.json")
            let initial = BrowserActions(pasteboard: board, recoveryURL: store)
            try await perform(initial) { initial.rename(source, to: "Renamed.txt") }
            XCTAssertTrue(FileManager.default.fileExists(atPath: store.path))
            XCTAssertEqual(try contents(root.appendingPathComponent("Renamed.txt")), "original")

            let reopened = BrowserActions(pasteboard: board, recoveryURL: store)
            XCTAssertEqual(reopened.undoName, "Rename")
            try await perform(reopened) { reopened.undo() }
            XCTAssertEqual(try FileReference(url: source.url).identity, source.identity)
            XCTAssertEqual(try contents(source.url), "original")
            XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("Renamed.txt").path))
        }
    }

    @MainActor
    func testCopyUsesNativeFileURLsAndRetainsTheSource() async throws {
        try await withFixture { root, destination, board in
            let source = try write("original", named: "Notes.txt", in: root)
            let actions = BrowserActions(pasteboard: board)
            actions.copy([source])

            let objects = try XCTUnwrap(board.readObjects(
                forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]
            ))
            let url = try XCTUnwrap(objects.first as? NSURL) as URL
            XCTAssertEqual(url.path, source.url.path)

            try await perform(actions) { actions.paste(into: destination) }
            XCTAssertEqual(try contents(source.url), "original")
            XCTAssertEqual(try contents(destination.appendingPathComponent("Notes.txt")), "original")
            XCTAssertEqual(actions.transfer?.succeeded, 1)
            XCTAssertTrue(actions.cutURLs.isEmpty)
        }
    }

    @MainActor
    func testCutSkipKeepBothAndUndoPreserveTheExistingItem() async throws {
        try await withFixture { root, destination, board in
            let source = try write("selected", named: "Report.txt", in: root)
            let occupant = try write("already here", named: "Report.txt", in: destination)
            let actions = BrowserActions(pasteboard: board)
            actions.copy([source], cut: true)

            try await perform(actions) { actions.paste(into: destination, conflict: .skip) }
            XCTAssertEqual(try contents(source.url), "selected")
            XCTAssertEqual(try contents(occupant.url), "already here")
            XCTAssertEqual(actions.transfer?.succeeded, 0)
            XCTAssertTrue(actions.canRetryTransfer)
            XCTAssertTrue(actions.isCut(source.url))

            try await perform(actions) { actions.retryTransfer(conflict: .keepBoth) }
            let movedURL = destination.appendingPathComponent("Report (2).txt")
            let moved = try FileReference(url: movedURL)
            XCTAssertEqual(moved.identity, source.identity)
            XCTAssertEqual(try contents(movedURL), "selected")
            XCTAssertEqual(try contents(occupant.url), "already here")
            XCTAssertFalse(FileManager.default.fileExists(atPath: source.url.path))
            XCTAssertTrue(actions.cutURLs.isEmpty)
            XCTAssertFalse(actions.canRetryTransfer)
            XCTAssertEqual(actions.undoName, "Move")

            try await perform(actions) { actions.undo() }
            XCTAssertEqual(try FileReference(url: source.url).identity, source.identity)
            XCTAssertEqual(try contents(source.url), "selected")
            XCTAssertEqual(try contents(occupant.url), "already here")
            XCTAssertFalse(FileManager.default.fileExists(atPath: movedURL.path))

            XCTAssertEqual(actions.redoName, "Move")
            let lateOccupant = try write("arrived after undo", named: "Report (2).txt", in: destination)
            try await perform(actions) { actions.redo() }
            XCTAssertEqual(try contents(source.url), "selected")
            XCTAssertEqual(try contents(lateOccupant.url), "arrived after undo")
            XCTAssertEqual(actions.redoName, "Move")
            _ = try FileOperations.rename(lateOccupant, to: "Arrived Later.txt")

            try await perform(actions) { actions.redo() }
            XCTAssertEqual(try FileReference(url: movedURL).identity, source.identity)
            XCTAssertEqual(try contents(movedURL), "selected")
            XCTAssertEqual(try contents(occupant.url), "already here")
            XCTAssertFalse(FileManager.default.fileExists(atPath: source.url.path))
        }
    }

    @MainActor
    func testAnExternalClipboardReplacementDoesNotMoveTheOldCutFile() async throws {
        try await withFixture { root, destination, board in
            let cut = try write("keep at source", named: "Cut.txt", in: root)
            let external = try write("copy me", named: "External.txt", in: root)
            let actions = BrowserActions(pasteboard: board)
            actions.copy([cut], cut: true)
            XCTAssertTrue(actions.isCut(cut.url))

            board.clearContents()
            XCTAssertTrue(board.writeObjects([external.url as NSURL]))
            try await perform(actions) { actions.paste(into: destination) }

            XCTAssertEqual(try contents(cut.url), "keep at source")
            XCTAssertEqual(try contents(external.url), "copy me")
            XCTAssertEqual(try contents(destination.appendingPathComponent("External.txt")), "copy me")
            XCTAssertFalse(FileManager.default.fileExists(atPath: destination.appendingPathComponent("Cut.txt").path))
            XCTAssertTrue(actions.cutURLs.isEmpty)
        }
    }

    @MainActor
    func testCutRejectsAReplacementAtTheOriginalPath() async throws {
        try await withFixture { root, destination, board in
            let original = try write("original", named: "Selected.txt", in: root)
            let actions = BrowserActions(pasteboard: board)
            actions.copy([original], cut: true)
            let displaced = try FileOperations.rename(original, to: "Displaced.txt").moved
            let replacement = try write("replacement", named: "Selected.txt", in: root)

            try await perform(actions) { actions.paste(into: destination) }
            XCTAssertEqual(try contents(displaced.url), "original")
            XCTAssertEqual(try contents(replacement.url), "replacement")
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: destination.path).isEmpty)
            XCTAssertEqual(actions.transfer?.succeeded, 0)
            XCTAssertFalse(actions.issues.isEmpty)
            XCTAssertFalse(actions.canRetryTransfer)
        }
    }

    @MainActor
    private func withFixture(_ use: (URL, URL, NSPasteboard) async throws -> Void) async throws {
        let temporary = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        let root = URL(fileURLWithPath: temporary, isDirectory: true)
            .resolvingSymlinksInPath()
            .appendingPathComponent("RichFilesActions-\(UUID().uuidString)", isDirectory: true)
        let destination = root.appendingPathComponent("Destination", isDirectory: true)
        try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)
        let board = NSPasteboard.withUniqueName()
        defer {
            board.releaseGlobally()
            try? FileManager.default.removeItem(at: root)
        }
        try await use(root, destination, board)
    }

    @MainActor
    private func perform(_ actions: BrowserActions, _ action: () -> Void) async throws {
        let complete = expectation(description: "File action finished")
        let revision = actions.mutationRevision
        var reached = false
        actions.didMutate = {
            reached = true
            complete.fulfill()
        }
        defer { actions.didMutate = nil }
        action()
        // The callback proves completion. The deadline only stops a hung fixture.
        await fulfillment(of: [complete], timeout: 10)
        guard reached else { throw ActionTimeout() }
        XCTAssertNotEqual(actions.mutationRevision, revision)
        XCTAssertFalse(actions.busy)
    }

    private func write(_ text: String, named name: String, in folder: URL) throws -> FileReference {
        let url = folder.appendingPathComponent(name)
        try Data(text.utf8).write(to: url, options: .withoutOverwriting)
        return try FileReference(url: url)
    }

    private func contents(_ url: URL) throws -> String {
        try String(contentsOf: url, encoding: .utf8)
    }

    private struct ActionTimeout: Error {}
}
