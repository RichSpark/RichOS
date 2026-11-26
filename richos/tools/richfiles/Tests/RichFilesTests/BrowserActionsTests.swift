import AppKit
import Combine
import XCTest
@testable import RichFiles
@testable import RichFilesCore

final class BrowserActionsTests: XCTestCase {
    @MainActor
    func testConfirmingDefaultNewFolderNameCreatesTheNextAvailableFolder() async throws {
        try await withFixture { _, destination, board in
            let existing = try FileOperations.createFolder(in: destination, named: "New Folder")
            let request = NameRequest(source: nil, folder: destination, directory: true, name: "New Folder")
            let actions = BrowserActions(pasteboard: board)
            let finished = expectation(description: "Folder action finished")
            let observer = actions.$busy.dropFirst().filter { !$0 }.prefix(1).sink { _ in finished.fulfill() }
            defer { observer.cancel() }

            var created: FileReference?
            actions.create(from: request, named: request.name) { created = $0 }
            await fulfillment(of: [finished], timeout: 5)

            XCTAssertEqual(created?.url.lastPathComponent, "New Folder (2)")
            XCTAssertEqual(try FileReference(url: existing.url).identity, existing.identity)
            XCTAssertEqual(Set(try FileManager.default.contentsOfDirectory(atPath: destination.path)),
                           ["New Folder", "New Folder (2)"])
        }
    }

    func testStallDeadlineFollowsProgressEvenWhenCopyIsSlow() {
        let item = URL(fileURLWithPath: "/fixture/slow.txt")
        var transfer = BrowserTransfer(destination: item.deletingLastPathComponent(), move: false, total: 2)
        transfer.lastAdvance = 10
        XCTAssertEqual(transfer.secondsUntilStall(at: 24), 1)
        XCTAssertEqual(transfer.secondsUntilStall(at: 25), 0)

        transfer.updateCopyProgress(.init(source: item, written: 1, total: 100, elapsed: 14), at: 24)
        XCTAssertEqual(transfer.secondsUntilStall(at: 38), 1)
        transfer.updateCopyProgress(.init(source: item, written: 1, total: 100, elapsed: 28), at: 38)
        XCTAssertEqual(transfer.secondsUntilStall(at: 39), 0, "A repeated byte count cannot keep a stalled copy active")
        transfer.updateCopyProgress(.init(source: item, written: 2, total: 100, elapsed: 30), at: 40)
        XCTAssertEqual(transfer.secondsUntilStall(at: 40), 15, "New bytes clear the stall immediately")

        let nested = item.deletingLastPathComponent().appendingPathComponent("next.txt")
        transfer.updateCopyProgress(.init(source: nested, written: 1, total: nil, elapsed: 1), at: 45)
        XCTAssertEqual(transfer.secondsUntilStall(at: 45), 15, "A new nested file starts its own byte count")
        transfer.running = false
        XCTAssertNil(transfer.secondsUntilStall(at: 100))
    }

    @MainActor
    func testRichFilesDiagnosticLogKeepsTheLatestStructuredRecordWithinItsCap() async throws {
        try await withFixture { _, logs, _ in
            let url = logs.appendingPathComponent(RichFilesDiagnostics.filename)
            let sample = RichFilesDiagnostics.Sample(
                atMS: 1_792_658_590_000, id: UUID(), build: "1", operation: .listing, outcome: .cancelled,
                workerMS: 145, firstRowsMS: 32, total: 300, confirmed: 290
            )
            try RichFilesDiagnostics.append(sample, in: logs)
            let file = try FileHandle(forWritingTo: url)
            try file.truncate(atOffset: 128 * 1024)
            try file.close()
            try RichFilesDiagnostics.append(sample, in: logs)
            let data = try Data(contentsOf: url)
            XCTAssertLessThan(data.count, 1024)
            let record = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
            XCTAssertEqual(record["id"] as? String, sample.id.uuidString)
            XCTAssertEqual(record["outcome"] as? String, "canceled")
            XCTAssertEqual(record["workerMS"] as? Int, 145)
            XCTAssertEqual(record["firstRowsMS"] as? Int, 32)
            XCTAssertEqual(Set(record.keys), ["atMS", "id", "build", "operation", "outcome",
                                              "workerMS", "firstRowsMS", "total", "confirmed"])
        }
    }

    @MainActor
    func testWindowGestureDiagnosticsRotateIndependentlyOfFileOperations() async throws {
        try await withFixture { _, logs, _ in
            let fileLog = logs.appendingPathComponent(RichFilesDiagnostics.filename)
            let gestureLog = logs.appendingPathComponent(RichFilesDiagnostics.gestureFilename)
            let listing = RichFilesDiagnostics.Sample(
                atMS: 1_792_658_590_000, id: UUID(), build: "1", operation: .listing, outcome: .complete,
                workerMS: 15, firstRowsMS: 10, total: 3, confirmed: 3
            )
            let gesture = RichFilesDiagnostics.Sample(
                atMS: 1_792_658_590_000, id: UUID(), build: "1", operation: .windowGestureRestore, outcome: .complete,
                workerMS: 10, firstRowsMS: nil, total: 1, confirmed: 1
            )
            try RichFilesDiagnostics.append(listing, in: logs)
            let original = try Data(contentsOf: fileLog)
            try RichFilesDiagnostics.append(gesture, in: logs)
            let file = try FileHandle(forWritingTo: gestureLog)
            try file.truncate(atOffset: 128 * 1024)
            try file.close()
            try RichFilesDiagnostics.append(gesture, in: logs)
            XCTAssertEqual(try Data(contentsOf: fileLog), original)
            let data = try Data(contentsOf: gestureLog)
            XCTAssertLessThan(data.count, 1024)
            let record = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
            XCTAssertEqual(record["operation"] as? String, "windowGestureRestore")
        }
    }

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
    func testTrashUsesLatestMatchingOriginFromOtherHostWithoutWritingItsHistory() async throws {
        try await withFixture { root, trash, board in
            let folder = root.appendingPathComponent("New Origin", isDirectory: true)
            let history = root.appendingPathComponent("History", isDirectory: true)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
            try FileManager.default.createDirectory(at: history, withIntermediateDirectories: false,
                                                    attributes: [.posixPermissions: 0o700])
            let old = try write("retained bytes", named: "Receipt.png", in: root)
            let original = try FileOperations.move(old, into: folder).moved
            let archived = try FileOperations.move(original, into: trash).moved
            let local = history.appendingPathComponent("Recovery.json")
            let peer = history.appendingPathComponent("DockedRecovery.json")
            func save(_ source: FileReference, time: TimeInterval, to url: URL) throws {
                let receipt = TrashReceipt(original: source, trashedAt: Date(timeIntervalSince1970: time),
                                           trashLocation: archived.url, trashed: archived)
                let state = TrashFixture(trash: [.init(id: UUID(), receipt: receipt)])
                try JSONEncoder().encode(state).write(to: url)
            }
            try save(old, time: 100, to: local)
            try save(original, time: 200, to: peer)
            let peerBefore = try Data(contentsOf: peer)
            let actions = BrowserActions(pasteboard: board, recoveryURL: local, peerRecoveryURL: peer)
            let peerItems = BrowserActions.readPeerTrash(from: actions.peerRecoveryURL)
            let index = try TrashIndex.read(locations: [trash], known: actions.trashItems, peer: peerItems)
            XCTAssertEqual(index.entries.count, 1)
            let entry = try XCTUnwrap(index.entries.first)
            XCTAssertEqual(entry.originalFolder, folder.path)
            XCTAssertEqual(entry.known?.id, peerItems.first?.id)

            try await perform(actions) { actions.restore(entry) }
            XCTAssertEqual(try contents(original.url), "retained bytes")
            XCTAssertFalse(FileManager.default.fileExists(atPath: old.url.path))
            XCTAssertEqual(actions.undoName, "Restore")
            XCTAssertEqual(try Data(contentsOf: peer), peerBefore)
            let refreshed = try TrashIndex.read(locations: [trash], known: actions.trashItems, peer: peerItems)
            XCTAssertTrue(refreshed.entries.isEmpty)
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
    func testUpdateRestartRequiresDurableUndoAndRedo() async throws {
        try await withFixture { root, _, board in
            let source = try write("original", named: "Original.txt", in: root)
            let ephemeral = BrowserActions(pasteboard: board)
            try await perform(ephemeral) { ephemeral.rename(source, to: "Ephemeral.txt") }
            XCTAssertNotNil(ephemeral.undoName)
            XCTAssertFalse(ephemeral.canRestartAfterUpdate)

            let recovery = root.appendingPathComponent("Private", isDirectory: true)
                .appendingPathComponent("Recovery.json")
            let saved = BrowserActions(pasteboard: board, recoveryURL: recovery)
            let renamed = try FileReference(url: root.appendingPathComponent("Ephemeral.txt"))
            try await perform(saved) { saved.rename(renamed, to: "Durable.txt") }
            XCTAssertTrue(saved.canRestartAfterUpdate)
            let restored = BrowserActions(pasteboard: board, recoveryURL: recovery)
            XCTAssertNotNil(restored.undoName)
            try await perform(restored) { restored.undo() }
            XCTAssertTrue(restored.canRestartAfterUpdate)
            let reopened = BrowserActions(pasteboard: board, recoveryURL: recovery)
            XCTAssertNotNil(reopened.redoName)
            try await perform(reopened) { reopened.redo() }
            XCTAssertEqual(try contents(root.appendingPathComponent("Durable.txt")), "original")
        }
    }

    @MainActor
    func testUpdateRestartAllowsLongHistoryWithSavedUndoAndRedo() async throws {
        try await withFixture { root, _, board in
            var source = try write("original", named: "Original.txt", in: root)
            let recovery = root.appendingPathComponent("Private", isDirectory: true)
                .appendingPathComponent("Recovery.json")
            let actions = BrowserActions(pasteboard: board, recoveryURL: recovery)
            for index in 1...42 {
                let name = "Rename \(index).txt"
                try await perform(actions) { actions.rename(source, to: name) }
                source = try FileReference(url: root.appendingPathComponent(name))
            }
            XCTAssertTrue(actions.canRestartAfterUpdate)
            try await perform(actions) { actions.undo() }
            XCTAssertTrue(actions.canRestartAfterUpdate)

            let reopened = BrowserActions(pasteboard: board, recoveryURL: recovery)
            XCTAssertNotNil(reopened.undoName)
            XCTAssertNotNil(reopened.redoName)
            try await perform(reopened) { reopened.redo() }
            XCTAssertEqual(try contents(root.appendingPathComponent("Rename 42.txt")), "original")
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
    func testLegacyRichFilesSupportDirectoryKeepsMoveUndoAfterRelaunch() async throws {
        try await withFixture { root, destination, board in
            let support = root.appendingPathComponent("app.richos.richfiles", isDirectory: true)
            try FileManager.default.createDirectory(at: support, withIntermediateDirectories: false,
                                                    attributes: [.posixPermissions: 0o755])
            let lock = support.appendingPathComponent("RichGlanceDefaults.lock")
            try Data().write(to: lock)
            let store = support.appendingPathComponent("Recovery.json")
            let source = try write("original", named: "Notes.txt", in: root)
            let actions = BrowserActions(pasteboard: board, recoveryURL: store)
            actions.copy([source], cut: true)
            try await perform(actions) { actions.paste(into: destination) }

            XCTAssertEqual(try contents(destination.appendingPathComponent("Notes.txt")), "original")
            XCTAssertFalse(FileManager.default.fileExists(atPath: source.url.path))
            XCTAssertTrue(FileManager.default.fileExists(atPath: store.path))
            XCTAssertTrue(FileManager.default.fileExists(atPath: lock.path))
            let attributes = try FileManager.default.attributesOfItem(atPath: support.path)
            XCTAssertEqual(attributes[.posixPermissions] as? Int, 0o700)
            let reopened = BrowserActions(pasteboard: board, recoveryURL: store)
            _ = try XCTUnwrap(reopened.undoName)
            try await perform(reopened) { reopened.undo() }
            XCTAssertEqual(try contents(source.url), "original")
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

            XCTAssertTrue(actions.canRestartAfterUpdate)
            try await perform(actions) {
                actions.paste(into: destination)
                XCTAssertFalse(actions.canRestartAfterUpdate, "The file copy must finish before an update can restart RichFiles")
            }
            XCTAssertTrue(actions.canRestartAfterUpdate, "A finished copy needs no user action before restarting")
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
            let localValue = BrowserDragSelection.pasteboardValue(for: token)
            let foreignValue = UUID().uuidString + ":" + token.uuidString
            XCTAssertEqual(BrowserDragSelection.localToken(from: localValue), token)
            XCTAssertNil(BrowserDragSelection.localToken(from: foreignValue))
            XCTAssertNil(BrowserDragSelection.localToken(from: token.uuidString))
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

    /// Catalog entry 1: Ctrl+C, then Ctrl+V in the same folder makes `name - Copy.ext`, then
    /// `name - Copy (2).ext`, for files and folders, each selectable and undoable as a Duplicate.
    @MainActor
    func testPasteIntoTheSameFolderDuplicatesAsCopyThenCopyTwo() async throws {
        try await withFixture { _, folder, board in
            let file = try write("original", named: "yet another test file.txt", in: folder)
            let actions = BrowserActions(pasteboard: board)
            actions.copy([file])

            try await perform(actions) { actions.paste(into: folder) }
            let first = folder.appendingPathComponent("yet another test file - Copy.txt")
            XCTAssertEqual(try contents(first), "original")
            XCTAssertEqual(actions.status, "Copied as “yet another test file - Copy.txt”.")
            XCTAssertEqual(actions.lastDuplicates?.items.map(\.url.lastPathComponent), ["yet another test file - Copy.txt"])
            XCTAssertEqual(actions.undoName, "Duplicate")

            try await perform(actions) { actions.paste(into: folder) }
            let second = folder.appendingPathComponent("yet another test file - Copy (2).txt")
            XCTAssertEqual(try contents(second), "original")
            XCTAssertEqual(try contents(file.url), "original")
            // Undo Duplicate moves the copy to the real Trash, so it is walked in the test VM, not here.
            XCTAssertEqual(actions.undoName, "Duplicate")

            let plans = try FileOperations.createFolder(in: folder, named: "Plans")
            _ = try write("inside", named: "inside.txt", in: plans.url)
            actions.copy([plans])
            try await perform(actions) { actions.paste(into: folder) }
            XCTAssertEqual(try contents(folder.appendingPathComponent("Plans - Copy/inside.txt")), "inside")
            XCTAssertEqual(actions.lastDuplicates?.items.map(\.url.lastPathComponent), ["Plans - Copy"])
        }
    }

    /// Catalog entry 11: each List column is its own longest name plus the row's chrome, so
    /// columns differ in width; a longer name widens only its column, and the cap holds.
    func testListColumnsFitTheirLongestNameEachOnItsOwn() {
        let measure = { (name: String) in CGFloat(name.count * 8) }
        let names = ["a", "_the longest file name in this column.html", "bb", "c", "dd"]
        let layout = ListColumnLayout(names: names, rows: 2, measure: measure)
        XCTAssertEqual(layout.widths, [CGFloat(43 + 42 * 8), CGFloat(43 + 2 * 8), CGFloat(43 + 2 * 8)])
        let editing = ListColumnLayout(names: names, rows: 2, extra: { $0 == 3 ? 40 : 0 }, measure: measure)
        XCTAssertEqual(editing.widths, [CGFloat(43 + 42 * 8), CGFloat(43 + 8 + 40), CGFloat(43 + 2 * 8)],
                       "Only the edited name's column moves")
        let capped = ListColumnLayout(names: [String(repeating: "x", count: 200)], rows: 30, measure: measure)
        XCTAssertEqual(capped.widths, [ListColumnLayout.maximum])
    }

    /// Catalog entry 15: renaming an open folder keeps it and the folders open inside it open
    /// under the new name, in every root it appears under, and leaves other folders alone.
    @MainActor
    func testRenamingAFolderKeepsItsOpenBranchesOpen() {
        let tree = FolderTreeState()
        for id in ["place:/Users/a/Desktop", "place:/Users/a/Desktop/Projects", "place:/Users/a/Desktop/Projects/Active",
                   "pin:/Users/a/Desktop/Projects", "place:/Users/a/Desktop/Projects Old"] {
            tree.setOpen(id, true)
        }
        tree.move(from: URL(fileURLWithPath: "/Users/a/Desktop/Projects"),
                  to: URL(fileURLWithPath: "/Users/a/Desktop/Client Projects"))
        XCTAssertEqual(tree.open, ["place:/Users/a/Desktop", "place:/Users/a/Desktop/Client Projects",
                                   "place:/Users/a/Desktop/Client Projects/Active", "pin:/Users/a/Desktop/Client Projects",
                                   "place:/Users/a/Desktop/Projects Old"])
    }

    /// Catalog entry 15: Undo Rename puts the open branches back under the old name, and Redo
    /// moves them again, as the rename itself did: the replay reports the move it made.
    @MainActor
    func testUndoAndRedoRenameKeepTheOpenSidebarBranchesOpen() async throws {
        try await withFixture { root, _, board in
            let projects = root.appendingPathComponent("Projects", isDirectory: true)
            try FileManager.default.createDirectory(at: projects.appendingPathComponent("Active"),
                                                    withIntermediateDirectories: true)
            let renamed = root.appendingPathComponent("Client Projects", isDirectory: true)
            let actions = BrowserActions(pasteboard: board)
            let tree = FolderTreeState()
            tree.setOpen("place:" + projects.path, true)
            tree.setOpen("place:" + projects.appendingPathComponent("Active").path, true)
            @MainActor func state(_ folder: URL) -> (Bool, Bool) {
                (tree.isOpen("place:" + folder.path), tree.isOpen("place:" + folder.appendingPathComponent("Active").path))
            }
            // What BrowserView does with each change: a rename moves the keys in its completion,
            // every change applies the moves the actions report.
            @MainActor func follow() { for move in actions.mutationMoves { tree.move(from: move.from, to: move.to) } }

            var moved: FileReference?
            try await perform(actions) { actions.rename(try! FileReference(url: projects), to: "Client Projects") { moved = $0 } }
            tree.move(from: projects, to: try XCTUnwrap(moved).url)
            follow()
            XCTAssertTrue(state(renamed) == (true, true), "renamed=\(state(renamed))")

            try await perform(actions) { actions.undo() }
            follow()
            XCTAssertTrue(FileManager.default.fileExists(atPath: projects.appendingPathComponent("Active").path))
            XCTAssertTrue(state(projects) == (true, true), "after Undo=\(state(projects))")

            try await perform(actions) { actions.redo() }
            follow()
            XCTAssertTrue(state(renamed) == (true, true), "after Redo=\(state(renamed))")
        }
    }

    /// Catalog entry 2: Enter keeps a new name, refuses one macOS cannot use or one already in the
    /// folder (ignoring case), and leaves an empty or unchanged name alone; the box selects the
    /// part before the extension.
    func testInlineRenameOutcomesAndTheSelectedStem() {
        let siblings = ["_short file name.html", "Notes.txt", "Plans"]
        XCTAssertEqual(InlineRename.outcome(for: "  ", original: "Notes.txt", siblings: siblings), .cancel)
        XCTAssertEqual(InlineRename.outcome(for: "Notes.txt", original: "Notes.txt", siblings: siblings), .cancel)
        XCTAssertEqual(InlineRename.outcome(for: "a/b.txt", original: "Notes.txt", siblings: siblings),
                       .refuse("A name can’t contain “/” or “:”."))
        XCTAssertEqual(InlineRename.outcome(for: "plans", original: "Notes.txt", siblings: siblings),
                       .refuse("An item named “plans” already exists here."))
        XCTAssertEqual(InlineRename.outcome(for: "notes.TXT", original: "Notes.txt", siblings: siblings),
                       .rename("notes.TXT"), "A change of case only is a rename of the same item")
        XCTAssertEqual(InlineRename.outcome(for: "Renamed.txt ", original: "Notes.txt", siblings: siblings), .rename("Renamed.txt"))
        XCTAssertEqual(InlineRename.stemLength("_the longest file name in this.html", directory: false), 30)
        XCTAssertEqual(InlineRename.stemLength("archive.tar.gz", directory: false), 11)
        XCTAssertEqual(InlineRename.stemLength("Plans.v2", directory: true), 8)
        XCTAssertEqual(InlineRename.stemLength("README", directory: false), 6)
    }

    /// Catalog entry 2: finishing an inline rename by clicking another file leaves that file
    /// selected; Enter (the renamed file still the only selection) selects the new name.
    func testClickingAnotherFileToFinishARenameKeepsThatFileSelected() {
        let folder = URL(fileURLWithPath: "/fixture", isDirectory: true)
        let report = folder.appendingPathComponent("report.txt")
        let renamed = folder.appendingPathComponent("report-final.txt")
        let budget = folder.appendingPathComponent("budget.txt")
        XCTAssertNil(InlineRename.selection(afterRenaming: report, to: renamed, current: [budget]),
                     "The click on budget.txt that finished the edit keeps budget.txt selected")
        XCTAssertNil(InlineRename.selection(afterRenaming: report, to: renamed, current: []),
                     "A click on empty space that finished the edit leaves nothing selected")
        XCTAssertEqual(InlineRename.selection(afterRenaming: report, to: renamed, current: [report]), [renamed])
    }

    /// Catalog entry 2: click, pause, click arms the name box; if the selection moves before it
    /// opens (Down pressed), the box never opens on the old item.
    @MainActor
    func testAPendingClickPauseClickRenameIsCanceledWhenTheSelectionMoves() async throws {
        let rename = InlineRename()
        var selected = true
        var opened = 0
        rename.arm(after: 0.05, stillSelected: { selected }) { opened += 1 }
        selected = false
        try await Task.sleep(for: .milliseconds(300))
        XCTAssertEqual(opened, 0, "The selection moved before the box opened")

        selected = true
        rename.arm(after: 0.05, stillSelected: { selected }) { opened += 1 }
        try await Task.sleep(for: .milliseconds(300))
        XCTAssertEqual(opened, 1, "The item still the only selection gets its box")
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
