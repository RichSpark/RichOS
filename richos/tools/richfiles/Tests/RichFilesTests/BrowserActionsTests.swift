import AppKit
import Combine
import XCTest
@testable import RichFiles
@testable import RichFilesCore

final class BrowserActionsTests: XCTestCase {
    @MainActor
    func testRestoreSelectedKeepsOccupiedNameAndGroupsSuccessfulRestores() async throws {
        try await withFixture { root, trash, board in
            let names = ["Alpha.png", "Beta.jpg", "Gamma.pdf"]
            let records = try names.map { name in
                let original = try write("original " + name, named: name, in: root)
                let archived = try FileOperations.move(original, into: trash).moved
                let receipt = TrashReceipt(original: original, trashedAt: Date(), trashLocation: archived.url, trashed: archived)
                return TrashFixture.Item(id: UUID(), receipt: receipt)
            }
            let occupant = try write("late occupant", named: names[1], in: root)
            let recovery = root.appendingPathComponent("Recovery", isDirectory: true)
            try FileManager.default.createDirectory(at: recovery, withIntermediateDirectories: false,
                                                    attributes: [.posixPermissions: 0o700])
            let store = recovery.appendingPathComponent("Recovery.json")
            try JSONEncoder().encode(TrashFixture(trash: records)).write(to: store)
            let actions = BrowserActions(pasteboard: board, recoveryURL: store)
            let selection = actions.trashItems.map { item in
                TrashListEntry(reference: item.reference, known: item, dateAdded: item.trashedAt, canDelete: true)
            }
            XCTAssertEqual(selection.count, 3)

            try await perform(actions) { actions.restoreSelected(selection) }
            XCTAssertEqual(actions.undoName, "Restore Selected")
            XCTAssertEqual(actions.trashItems.map(\.originalURL.lastPathComponent), [names[1]])
            XCTAssertTrue(actions.status.contains("Restored 2 of 3 selected items."))
            XCTAssertEqual(actions.operationIssues.count, 1)
            XCTAssertTrue(actions.operationIssues.first?.contains(names[1]) == true)
            XCTAssertEqual(try contents(occupant.url), "late occupant")
            XCTAssertEqual(try contents(trash.appendingPathComponent(names[1])), "original " + names[1])
            for name in [names[0], names[2]] {
                XCTAssertEqual(try contents(root.appendingPathComponent(name)), "original " + name)
                XCTAssertFalse(FileManager.default.fileExists(atPath: trash.appendingPathComponent(name).path))
            }
            let saved = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: store)) as? [String: Any])
            let groups = try XCTUnwrap(saved["undo"] as? [[String: Any]])
            XCTAssertEqual(groups.count, 1)
            XCTAssertEqual((groups.first?["steps"] as? [Any])?.count, 2)
        }
    }

    @MainActor
    func testInterruptedMoveRecoversConfirmedUndoOnceAndLeavesPendingSourceAlone() async throws {
        try await withFixture { root, destination, board in
            let store = root.appendingPathComponent("Recovery/Recovery.json")
            let journalURL = store.deletingLastPathComponent().appendingPathComponent(".Recovery.json.transfer-journal")
            let first = try write("confirmed bytes", named: "First.txt", in: root)
            let pending = try write("pending bytes", named: "Pending.txt", in: root)
            do {
                let journal = try TransferJournal(recoveryURL: store, id: UUID(), destination: destination, move: true, total: 2)
                try journal.begin(first.url)
                let moved = try FileOperations.move(first, into: destination)
                try journal.finish(succeeded: true, move: .sameVolume(moved))
                try journal.begin(pending.url)
                XCTAssertNil(try TransferJournal.interrupted(at: store))
            }
            let interruptedFile = try FileHandle(forWritingTo: journalURL)
            try interruptedFile.seekToEnd()
            try interruptedFile.write(contentsOf: Data("{\"finished\":".utf8))
            try interruptedFile.close()
            let interruptedBytes = try Data(contentsOf: journalURL)
            try Data("unreadable prior history".utf8).write(to: store)

            let recovered = BrowserActions(pasteboard: board, recoveryURL: store)
            try await waitForRecovery(recovered)
            XCTAssertEqual(recovered.interruptedTransfer?.succeeded, 1)
            XCTAssertEqual(recovered.interruptedTransfer?.pendingSource, pending.url)
            XCTAssertEqual(recovered.interruptedTransfer?.incompleteTail, true)
            XCTAssertEqual(recovered.interruptedTransfer?.recoverableMoves, 1)
            XCTAssertEqual(recovered.undoName, "Move Before Interruption")
            XCTAssertFalse(FileManager.default.fileExists(atPath: journalURL.path))
            let preserved = try FileManager.default.contentsOfDirectory(
                at: store.deletingLastPathComponent(), includingPropertiesForKeys: nil
            ).filter { $0.lastPathComponent.hasPrefix("Recovery.json.unreadable-") }
            XCTAssertEqual(try contents(XCTUnwrap(preserved.first)), "unreadable prior history")
            XCTAssertEqual(try contents(pending.url), "pending bytes")
            XCTAssertFalse(FileManager.default.fileExists(atPath: destination.appendingPathComponent(pending.url.lastPathComponent).path))

            // Simulate an exit after the history was saved and before its journal was unlinked.
            XCTAssertTrue(FileManager.default.createFile(atPath: journalURL.path, contents: interruptedBytes,
                                                         attributes: [.posixPermissions: 0o600]))
            let reopened = BrowserActions(pasteboard: board, recoveryURL: store)
            try await waitForRecovery(reopened)
            XCTAssertFalse(FileManager.default.fileExists(atPath: journalURL.path))
            try await perform(reopened) { reopened.undo() }
            XCTAssertNil(reopened.undoName)
            XCTAssertEqual(try contents(first.url), "confirmed bytes")
            XCTAssertEqual(try contents(pending.url), "pending bytes")
        }
    }

    @MainActor
    func testRichFinderReceivesQueryTextWithoutTakingTheSystemClipboard() {
        let text = "builder's quote for the kitchen"
        let accepted = RichOSServices.search(text) { service, board in
            XCTAssertEqual(service, "Search with RichFinder")
            XCTAssertNotEqual(board.name, NSPasteboard.general.name)
            XCTAssertEqual(board.string(forType: .string), text)
            XCTAssertNil(board.propertyList(forType: RichOSServices.filenamesType))
            return true
        }
        XCTAssertTrue(accepted)
    }

    @MainActor
    func testRichCleanerReceivesOnlyTheSelectedPathsOnAnIsolatedPasteboard() async throws {
        try await withFixture { root, _, board in
            let first = try write("first", named: "Report one.txt", in: root)
            let second = try write("second", named: "Report two.txt", in: root)
            let accepted = try await RichOSServices.addSpokenNote(to: [first, second]) { service, sent in
                XCTAssertEqual(service, "Add spoken note with RichCleaner")
                XCTAssertNotEqual(sent.name, NSPasteboard.general.name)
                XCTAssertNotEqual(sent.name, board.name)
                XCTAssertEqual(sent.propertyList(forType: RichOSServices.filenamesType) as? [String],
                               [first.url.path, second.url.path])
                return true
            }
            XCTAssertTrue(accepted)
            _ = try FileOperations.rename(second, to: "Moved.txt")
            _ = try write("replacement", named: "Report two.txt", in: root)
            do {
                _ = try await RichOSServices.addSpokenNote(to: [second]) { _, _ in
                    XCTFail("The changed path was handed to RichCleaner.")
                    return true
                }
                XCTFail("The changed path should have been refused.")
            } catch FileOperationError.sourceChanged(let path) {
                XCTAssertEqual(path, second.url)
            }
        }
    }

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
    func testTransferRetainsOnlyUnstartedItemsWhenDestinationDisappears() async throws {
        try await withFixture { root, destination, _ in
            let first = try write("first bytes", named: "First.txt", in: root)
            let next = try write("next bytes", named: "Next.txt", in: root)
            let original = try FileReference(url: destination)
            let pinned = try BrowserTransferDestination(folder: destination)
            let request = BrowserTransferRequest(sources: [.captured(first), .captured(next)],
                                                 folder: destination, move: false, conflict: .skip,
                                                 clipboardGeneration: nil, destination: pinned)
            let progress = BrowserTransfer(destination: destination, move: false, total: 2)
            let result = await BrowserTransferJob.perform(request, progress: progress, recoveryURL: nil) { update in
                if update.currentSource == next.url {
                    _ = try? FileOperations.rename(original, to: "Parked")
                }
            }
            let parked = root.appendingPathComponent("Parked", isDirectory: true)
            XCTAssertEqual(try contents(parked.appendingPathComponent("First.txt")), "first bytes")
            XCTAssertEqual(try contents(next.url), "next bytes")
            XCTAssertEqual(result.progress.succeeded, 1)
            XCTAssertEqual(result.progress.completed, 1)
            XCTAssertEqual(result.retry.map(\.url), [next.url])
            XCTAssertEqual(result.destination?.identity, original.identity)
        }
    }

    @MainActor
    func testPinnedPasteCanRetryOnlyAfterTheOriginalFolderReturns() async throws {
        try await withFixture { root, destination, board in
            let source = try write("original", named: "Notes.txt", in: root)
            let pinned = try BrowserTransferDestination(folder: destination)
            let original = try FileReference(url: destination)
            let moved = try FileOperations.rename(original, to: "Moved Destination").moved
            let replacement = try FileOperations.createFolder(in: root, named: "Destination")
            let actions = BrowserActions(pasteboard: board)
            actions.copy([source], cut: true)

            try await perform(actions) { actions.paste(into: destination, destination: pinned) }
            XCTAssertTrue(actions.canRetryTransfer)
            try await perform(actions) { actions.retryTransfer() }
            XCTAssertTrue(actions.canRetryTransfer)
            XCTAssertEqual(try contents(source.url), "original")
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: moved.url.path).isEmpty)
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: destination.path).isEmpty)
            XCTAssertEqual(actions.transfer?.succeeded, 0)
            XCTAssertEqual(actions.cutURLs, [source.url])

            let unrelated = try FileOperations.rename(replacement, to: "Unrelated Destination").moved
            _ = try FileOperations.rename(moved, to: "Destination")
            try await perform(actions) { actions.retryTransfer() }
            let installed = destination.appendingPathComponent("Notes.txt")
            XCTAssertEqual(try contents(installed), "original")
            XCTAssertEqual(try FileReference(url: installed).identity, source.identity)
            XCTAssertFalse(FileManager.default.fileExists(atPath: source.url.path))
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: unrelated.url.path).isEmpty)
            XCTAssertEqual(actions.transfer?.succeeded, 1)
            XCTAssertTrue(actions.cutURLs.isEmpty)
            XCTAssertFalse(actions.canRetryTransfer)
        }
    }

    @MainActor
    func testShelfCollectsAcrossFoldersAndKeepsUnmovedItems() async throws {
        try await withFixture { root, destination, board in
            let secondFolder = root.appendingPathComponent("Second", isDirectory: true)
            try FileManager.default.createDirectory(at: secondFolder, withIntermediateDirectories: false)
            let first = try write("first", named: "One.txt", in: root)
            let second = try write("second", named: "Two.txt", in: secondFolder)
            let actions = BrowserActions(pasteboard: board)
            actions.addToShelf([first, first])
            actions.addToShelf([second])
            XCTAssertEqual(Set(actions.shelf.map(\.url)), [first.url, second.url])
            board.clearContents()
            XCTAssertTrue(board.setString("keep clipboard", forType: .string))

            _ = try FileOperations.rename(first, to: "Original.txt")
            let replacement = try write("replacement", named: "One.txt", in: root)
            let target = try BrowserTransferDestination(folder: destination)
            try await perform(actions) { actions.transferShelf(to: target, move: true) }
            XCTAssertEqual(actions.shelf, [first])
            XCTAssertEqual(try contents(replacement.url), "replacement")
            XCTAssertEqual(try contents(destination.appendingPathComponent("Two.txt")), "second")
            XCTAssertFalse(FileManager.default.fileExists(atPath: destination.appendingPathComponent("One.txt").path))
            XCTAssertEqual(board.string(forType: .string), "keep clipboard")

            actions.clearShelf()
            XCTAssertTrue(actions.shelf.isEmpty)
            XCTAssertEqual(try contents(replacement.url), "replacement")
        }
    }

    @MainActor
    func testInternalDragPinsTheWholeSelectionAndConsumesItsTokenOnce() async throws {
        try await withFixture { root, destination, board in
            let first = try write("first", named: "First.PNG", in: root)
            let second = try write("second", named: "Second.JPG", in: root)
            let actions = BrowserActions(pasteboard: board)
            let token = actions.beginDrag(primary: first, sources: [first, second, first])
            XCTAssertNil(actions.takeDrag(UUID(), matching: first.url))
            XCTAssertNil(actions.takeDrag(token, matching: second.url))
            let selection = try XCTUnwrap(actions.takeDrag(token, matching: first.url))
            XCTAssertEqual(selection.sources, [first, second])
            XCTAssertNil(actions.takeDrag(token, matching: first.url))

            let target = try BrowserTransferDestination(folder: destination)
            XCTAssertTrue(selection.moves(to: target, option: false))
            XCTAssertFalse(selection.moves(to: target, option: true))
            try await perform(actions) {
                actions.transfer(selection.sources, into: target.url, move: true, destination: target)
            }
            XCTAssertEqual(try contents(destination.appendingPathComponent("First.PNG")), "first")
            XCTAssertEqual(try contents(destination.appendingPathComponent("Second.JPG")), "second")
            XCTAssertFalse(FileManager.default.fileExists(atPath: first.url.path))
            XCTAssertFalse(FileManager.default.fileExists(atPath: second.url.path))
        }
    }

    @MainActor
    func testTransferQueuePausesForAConflictThenContinuesOnRequest() async throws {
        try await withFixture { root, destination, board in
            let first = try write("incoming", named: "Conflict.txt", in: root)
            let existing = try write("existing", named: "Conflict.txt", in: destination)
            let second = try write("next file", named: "Next.png", in: root)
            let actions = BrowserActions(pasteboard: board)
            try await perform(actions) {
                actions.transfer([first], into: destination, move: false)
                actions.transfer([second], into: destination, move: false)
                XCTAssertEqual(actions.queuedTransfers.count, 1)
            }
            XCTAssertTrue(actions.queuePaused)
            XCTAssertFalse(FileManager.default.fileExists(atPath: destination.appendingPathComponent("Next.png").path))
            XCTAssertEqual(try contents(existing.url), "existing")

            try await perform(actions) { actions.continueQueue() }
            XCTAssertTrue(actions.queuedTransfers.isEmpty)
            XCTAssertFalse(actions.queuePaused)
            XCTAssertEqual(try contents(destination.appendingPathComponent("Next.png")), "next file")
            XCTAssertEqual(try contents(second.url), "next file")
        }
    }

    @MainActor
    func testHostSwitchDropsWaitingTransfersAndPreservesTheirSources() async throws {
        try await withFixture { root, destination, board in
            let first = try write("incoming", named: "Conflict.txt", in: root)
            _ = try write("existing", named: "Conflict.txt", in: destination)
            let waiting = try write("waiting", named: "Waiting.png", in: root)
            let actions = BrowserActions(pasteboard: board)
            try await perform(actions) {
                actions.transfer([first], into: destination, move: false)
                actions.transfer([waiting], into: destination, move: true)
            }
            XCTAssertEqual(actions.queuedTransfers.count, 1)
            try await actions.settleForHostSwitch()
            XCTAssertTrue(actions.queuedTransfers.isEmpty)
            XCTAssertFalse(actions.busy)
            XCTAssertEqual(try contents(waiting.url), "waiting")
            XCTAssertFalse(FileManager.default.fileExists(atPath: destination.appendingPathComponent("Waiting.png").path))
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
            XCTAssertEqual(actions.transfer?.issues.first?.conflict?.incoming.size, Int64("selected".utf8.count))
            XCTAssertEqual(actions.transfer?.issues.first?.conflict?.existing.size, Int64("already here".utf8.count))
            let reviewed = try XCTUnwrap(actions.transfer?.issues.first?.conflict?.reviewed)
            XCTAssertEqual(reviewed.incoming.reference, source)
            XCTAssertEqual(reviewed.existing.reference, occupant)

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
    func testCopyConflictOffersTheExactReviewedFilesWithoutReplacingThem() async throws {
        try await withFixture { root, destination, board in
            let incoming = try write("new", named: "Report.txt", in: root)
            let existing = try write("prior version", named: "Report.txt", in: destination)
            let actions = BrowserActions(pasteboard: board)
            actions.copy([incoming])
            try await perform(actions) { actions.paste(into: destination) }
            let reviewed = try XCTUnwrap(actions.transfer?.issues.first?.conflict?.reviewed)
            XCTAssertEqual(reviewed.incoming.reference, incoming)
            XCTAssertEqual(reviewed.existing.reference, existing)
            XCTAssertEqual(try contents(incoming.url), "new")
            XCTAssertEqual(try contents(existing.url), "prior version")
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
            XCTAssertTrue(actions.operationIssues.isEmpty)
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
    private func waitForRecovery(_ actions: BrowserActions) async throws {
        let finished = expectation(description: "Interrupted transfer recovery finished")
        let observation = actions.$busy.filter { !$0 }.prefix(1).sink { _ in finished.fulfill() }
        defer { observation.cancel() }
        await fulfillment(of: [finished], timeout: 10)
        guard !actions.busy else { throw ActionTimeout() }
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

    private struct TrashFixture: Encodable {
        struct Item: Encodable {
            let id: UUID
            let receipt: TrashReceipt
        }

        let version = 1
        let trash: [Item]
        let undo: [String] = []
        let redo: [String] = []
    }
}
