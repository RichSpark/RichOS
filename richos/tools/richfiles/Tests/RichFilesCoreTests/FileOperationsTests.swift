import Darwin
import Foundation
import XCTest
@testable import RichFilesCore

final class FileOperationsTests: XCTestCase {
    func testExclusiveCreationPreservesExistingItems() throws {
        try withFixture { root in
            let folder = try FileOperations.createFolder(in: root, named: "Notes")
            let empty = try FileOperations.createEmptyFile(in: folder.url, named: "first.md")
            XCTAssertEqual(try Data(contentsOf: empty.url), Data())
            try Data("keep this".utf8).write(to: empty.url)

            assertCollision(try FileOperations.createEmptyFile(in: folder.url, named: "first.md"))
            assertCollision(try FileOperations.createFolder(in: root, named: "Notes"))
            XCTAssertEqual(try String(contentsOf: empty.url, encoding: .utf8), "keep this")
            for name in ["", ".", "..", "bad/path", "bad\0name"] {
                XCTAssertThrowsError(try FileOperations.createEmptyFile(in: root, named: name)) {
                    guard case FileOperationError.invalidName = $0 else {
                        return XCTFail("Expected a name error, got \($0)")
                    }
                }
            }
        }
    }

    func testNewItemChoosesAnUnusedNameInTheOriginalFolder() throws {
        try withFixture { root in
            let folder = try FileOperations.createFolder(in: root, named: "Target")
            let existing = try write("keep me", named: "New File.txt", in: folder.url)
            assertCollision(try FileOperations.createItem(in: folder.url, named: "New File.txt", directory: false,
                                                          expectedFolder: folder.identity, useAvailableName: false))
            let created = try FileOperations.createItem(in: folder.url, named: "New File.txt", directory: false,
                                                        expectedFolder: folder.identity, useAvailableName: true)
            XCTAssertEqual(created.url.lastPathComponent, "New File (2).txt")
            XCTAssertEqual(try Data(contentsOf: created.url), Data())
            XCTAssertEqual(try contents(existing), "keep me")

            _ = try FileOperations.rename(folder, to: "Moved Target")
            let replacement = try FileOperations.createFolder(in: root, named: "Target")
            XCTAssertThrowsError(try FileOperations.createItem(
                in: replacement.url, named: "New Folder", directory: true,
                expectedFolder: folder.identity, useAvailableName: true
            )) {
                guard case FileOperationError.folderChanged = $0 else {
                    return XCTFail("Expected the changed folder to be refused, got \($0)")
                }
            }
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: replacement.url.path).isEmpty)
        }
    }

    func testMoveReplacementUndoAndRedoPreserveBothFiles() throws {
        try withFixture { root in
            let target = try FileOperations.createFolder(in: root, named: "Target")
            let archive = try FileOperations.createFolder(in: root, named: "Test Archive")
            let incoming = try write("incoming", named: "Photo.png", in: root)
            let previous = try write("existing", named: "Photo.png", in: target.url)
            func archivePrevious(_ file: FileReference) throws -> TrashReceipt {
                let parked = try FileOperations.move(file, into: archive.url).moved
                return TrashReceipt(original: file, trashedAt: Date(), trashLocation: parked.url, trashed: parked)
            }

            let replacement = try FileOperations.replaceMove(ReviewedFile(incoming), replacing: ReviewedFile(previous),
                                                             sendToTrash: archivePrevious)
            XCTAssertFalse(FileManager.default.fileExists(atPath: incoming.url.path))
            XCTAssertEqual(replacement.installed.reference.identity, incoming.identity)
            XCTAssertEqual(try contents(replacement.installed.reference), "incoming")
            XCTAssertEqual(try contents(XCTUnwrap(replacement.previous.trashed)), "existing")

            let blocker = try write("new occupant", named: "Photo.png", in: root)
            XCTAssertThrowsError(try FileOperations.undoMoveReplacement(incoming, installed: replacement.installed,
                                                                        previous: replacement.previous))
            XCTAssertEqual(try contents(blocker), "new occupant")
            XCTAssertEqual(try contents(replacement.installed.reference), "incoming")
            try FileManager.default.removeItem(at: blocker.url)

            let restored = try FileOperations.undoMoveReplacement(incoming, installed: replacement.installed,
                                                                   previous: replacement.previous)
            XCTAssertEqual(restored.source.reference, incoming)
            XCTAssertEqual(restored.destination.reference, previous)
            XCTAssertEqual(try contents(restored.source.reference), "incoming")
            XCTAssertEqual(try contents(restored.destination.reference), "existing")
            let repeated = try FileOperations.replaceMove(restored.source, replacing: restored.destination,
                                                          sendToTrash: archivePrevious)
            XCTAssertEqual(repeated.installed.reference.identity, incoming.identity)
            XCTAssertEqual(try contents(XCTUnwrap(repeated.previous.trashed)), "existing")
        }
    }

    func testTemplateCopyKeepsTheSourceAndRefusesAnExistingName() throws {
        try withFixture { root in
            let source = try write("template contents", named: "Plan.md", in: root)
            let destination = try FileOperations.createFolder(in: root, named: "New Project")
            let template = try ReviewedFile(source)
            let copy = try FileOperations.copyTemplate(template, into: destination.url,
                                                       expectedFolder: destination.identity, named: "Client.md")
            XCTAssertEqual(try contents(copy.copied), "template contents")
            XCTAssertEqual(try contents(source), "template contents")
            assertCollision(try FileOperations.copyTemplate(template, into: destination.url,
                                                            expectedFolder: destination.identity, named: "Client.md"))
            try Data("edited template".utf8).write(to: source.url)
            XCTAssertThrowsError(try FileOperations.copyTemplate(template, into: destination.url,
                                                                expectedFolder: destination.identity, named: "Other.md"))
            XCTAssertEqual(try contents(copy.copied), "template contents")
            XCTAssertEqual(try contents(source), "edited template")
        }
    }

    func testPermanentDeletionRejectsReplacedSelection() throws {
        try withFixture { root in
            let original = try write("original", named: "item.png", in: root)
            let parked = try FileOperations.rename(original, to: "parked.png").moved
            let replacement = try write("replacement", named: "item.png", in: root)

            XCTAssertThrowsError(try FileOperations.deletePermanently(original))
            XCTAssertEqual(try contents(parked), "original")
            XCTAssertEqual(try contents(replacement), "replacement")

            XCTAssertNil(try FileOperations.deletePermanently(replacement))
            XCTAssertFalse(FileManager.default.fileExists(atPath: replacement.url.path))
            XCTAssertEqual(try contents(parked), "original")
        }
    }

    func testMoveAndUndoPreserveIdentityAndRefuseOccupiedNames() throws {
        try withFixture { root in
            let source = try write("original", named: "plan.txt", in: root)
            let target = try FileOperations.createFolder(in: root, named: "Target")
            let occupant = try write("existing", named: "plan.txt", in: target.url)
            assertCollision(try FileOperations.move(source, into: target.url))
            XCTAssertEqual(try contents(source), "original")
            XCTAssertEqual(try contents(occupant), "existing")

            let receipt = try FileOperations.move(source, into: target.url, named: "moved.txt")
            XCTAssertFalse(FileManager.default.fileExists(atPath: source.url.path))
            XCTAssertEqual(receipt.moved.identity, source.identity)
            let newOccupant = try write("new file", named: "plan.txt", in: root)
            assertCollision(try FileOperations.undo(receipt))
            XCTAssertEqual(try contents(receipt.moved), "original")
            XCTAssertEqual(try contents(newOccupant), "new file")

            try FileManager.default.removeItem(at: newOccupant.url)
            let restored = try FileOperations.undo(receipt)
            XCTAssertEqual(restored.url, source.url)
            XCTAssertEqual(restored.identity, source.identity)
            XCTAssertEqual(try contents(restored), "original")
        }
    }

    func testUndoAndRenameRefuseAReplacementAtTheSelectedPath() throws {
        try withFixture { root in
            let source = try write("original", named: "item.txt", in: root)
            let receipt = try FileOperations.rename(source, to: "renamed.txt")
            let parked = try FileOperations.rename(receipt.moved, to: "parked.txt")
            let replacement = try write("replacement", named: "renamed.txt", in: root)
            for operation in [
                { try FileOperations.undo(receipt) },
                { try FileOperations.rename(receipt.moved, to: "another.txt").moved }
            ] {
                XCTAssertThrowsError(try operation()) {
                    guard case FileOperationError.sourceChanged = $0 else {
                        return XCTFail("Expected stale source, got \($0)")
                    }
                }
            }
            XCTAssertEqual(try contents(replacement), "replacement")
            XCTAssertEqual(try contents(parked.moved), "original")
        }
    }

    func testCopyAndMoveRefuseAReplacementOfThePinnedDestination() throws {
        try withFixture { root in
            let source = try write("original", named: "file.txt", in: root)
            let destination = try FileOperations.createFolder(in: root, named: "Destination")
            let moved = try FileOperations.rename(destination, to: "Moved Destination").moved
            let replacement = try FileOperations.createFolder(in: root, named: "Destination")

            let operations: [() throws -> Void] = [
                { _ = try FileOperations.copy(source, into: destination.url, expectedFolder: destination.identity) },
                { _ = try FileOperations.move(source, into: destination.url, expectedFolder: destination.identity) }
            ]
            for operation in operations {
                XCTAssertThrowsError(try operation()) {
                    guard case FileOperationError.folderChanged = $0 else {
                        return XCTFail("Expected the changed folder to be refused, got \($0)")
                    }
                }
            }
            XCTAssertEqual(try contents(source), "original")
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: moved.url.path), [])
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: replacement.url.path), [])
        }
    }

    func testUndoRefusesAReplacementOfTheOriginalFolder() throws {
        try withFixture { root in
            let origin = try FileOperations.createFolder(in: root, named: "Origin")
            let destination = try FileOperations.createFolder(in: root, named: "Destination")
            let source = try write("original", named: "item.txt", in: origin.url)
            let receipt = try FileOperations.move(source, into: destination.url)
            _ = try FileOperations.rename(origin, to: "Parked")
            let replacement = try FileOperations.createFolder(in: root, named: "Origin")
            XCTAssertThrowsError(try FileOperations.undo(receipt)) {
                guard case FileOperationError.folderChanged = $0 else {
                    return XCTFail("Expected a changed folder, got \($0)")
                }
            }
            XCTAssertEqual(try contents(receipt.moved), "original")
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: replacement.url.path), [])
        }
    }

    func testCaseOnlyRenameAndAnOccupiedHardLink() throws {
        try withFixture { root in
            let source = try write("original", named: "plan.txt", in: root)
            let renamed = try FileOperations.rename(source, to: "Plan.txt")
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["Plan.txt"])
            XCTAssertEqual(renamed.moved.identity, source.identity)

            let hardLink = root.appendingPathComponent("alias.txt")
            try FileManager.default.linkItem(at: renamed.moved.url, to: hardLink)
            assertCollision(try FileOperations.rename(renamed.moved, to: "alias.txt"))
            XCTAssertEqual(
                Set(try FileManager.default.contentsOfDirectory(atPath: root.path)),
                ["Plan.txt", "alias.txt"]
            )
        }
    }

    func testSymlinksAreCopiedAndMovedAsEntries() throws {
        try withFixture { root in
            let link = root.appendingPathComponent("Missing item link")
            let target = "unavailable-item.txt"
            try FileManager.default.createSymbolicLink(atPath: link.path, withDestinationPath: target)
            let reference = try FileReference(url: link)
            let directory = try FileOperations.createFolder(in: root, named: "Destination")
            let copy = try FileOperations.copy(reference, into: directory.url)
            XCTAssertNil(copy.remainingStagingFolder)
            XCTAssertEqual(
                try FileManager.default.destinationOfSymbolicLink(atPath: copy.copied.url.path), target
            )
            let moved = try FileOperations.move(reference, into: directory.url, named: "Moved link")
            XCTAssertEqual(moved.moved.identity, reference.identity)
            XCTAssertEqual(
                try FileManager.default.destinationOfSymbolicLink(atPath: moved.moved.url.path), target
            )
            XCTAssertThrowsError(try FileReference(url: link))
        }
    }

    func testCopyPreservesFileDataPermissionsAndExtendedAttribute() throws {
        try withFixture { root in
            let sourceURL = root.appendingPathComponent("plan.txt")
            try Data("bytes".utf8).write(to: sourceURL, options: .withoutOverwriting)
            let folder = try FileOperations.createFolder(in: root, named: "Destination")
            let modified = Date(timeIntervalSince1970: 1_600_000_000)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o640, .modificationDate: modified], ofItemAtPath: sourceURL.path
            )
            let attribute = "com.richfiles.fixture"
            let bytes: [UInt8] = [0x66, 0x69, 0x6C, 0x65]
            let wrote = bytes.withUnsafeBytes { value in
                setxattr(sourceURL.path, attribute, value.baseAddress, value.count, 0, XATTR_NOFOLLOW)
            }
            XCTAssertEqual(wrote, 0)

            // Setting an earlier modification date on APFS can also change the birth time.
            let source = try FileReference(url: sourceURL)
            let copy = try FileOperations.copy(source, into: folder.url)
            XCTAssertNil(copy.remainingStagingFolder)
            XCTAssertNotEqual(copy.copied.identity, source.identity)
            XCTAssertEqual(try contents(copy.copied), "bytes")
            XCTAssertEqual(try contents(source), "bytes")
            let attrs = try FileManager.default.attributesOfItem(atPath: copy.copied.url.path)
            XCTAssertEqual((attrs[.posixPermissions] as? NSNumber)?.intValue, 0o640)
            XCTAssertEqual(attrs[.modificationDate] as? Date, modified)
            var read = [UInt8](repeating: 0, count: bytes.count)
            let count = read.withUnsafeMutableBytes { value in
                getxattr(copy.copied.url.path, attribute, value.baseAddress, value.count, 0, XATTR_NOFOLLOW)
            }
            XCTAssertEqual(count, bytes.count)
            XCTAssertEqual(read, bytes)
        }
    }

    func testStreamCopyFailsWhenAnExtendedAttributeCannotBeWritten() throws {
        try XCTSkipIf(geteuid() == 0, "Requires ordinary file access permissions")
        try withFixture { root in
            let source = root.appendingPathComponent("Source.txt")
            let target = root.appendingPathComponent("Staged.txt")
            let bytes = Data("source data".utf8)
            let attribute = "com.richfiles.fixture"
            let metadata = Data("source metadata".utf8)
            try bytes.write(to: source, options: .withoutOverwriting)
            XCTAssertEqual(metadata.withUnsafeBytes {
                setxattr(source.path, attribute, $0.baseAddress, $0.count, 0, 0)
            }, 0)
            try Data().write(to: target, options: .withoutOverwriting)

            let denyAttributeWrites = Process()
            denyAttributeWrites.executableURL = URL(fileURLWithPath: "/bin/chmod")
            denyAttributeWrites.arguments = ["+a", "everyone deny writeextattr", target.path]
            try denyAttributeWrites.run()
            denyAttributeWrites.waitUntilExit()
            XCTAssertEqual(denyAttributeWrites.terminationStatus, 0)

            let input = open(source.path, O_RDONLY | O_CLOEXEC)
            let output = open(target.path, O_RDWR | O_CLOEXEC)
            defer { _ = close(input); _ = close(output) }
            XCTAssertGreaterThanOrEqual(input, 0)
            XCTAssertGreaterThanOrEqual(output, 0)
            XCTAssertEqual(bytes.withUnsafeBytes { pwrite(output, $0.baseAddress, 1, 0) }, 1)
            XCTAssertEqual(ftruncate(output, 0), 0)
            let (attributeWrite, denied) = metadata.withUnsafeBytes {
                let result = fsetxattr(output, attribute, $0.baseAddress, $0.count, 0, 0)
                return (result, errno)
            }
            XCTAssertEqual(attributeWrite, -1)
            XCTAssertEqual(denied, EACCES)

            XCTAssertThrowsError(try FileOperations.copyFileDescriptors(
                input, output, from: source, to: target, size: Int64(bytes.count),
                isCancelled: { false }, onProgress: { _, _ in }
            )) {
                guard case FileOperationError.system(_, let location, let code) = $0 else {
                    return XCTFail("Expected the native metadata error, got \($0)")
                }
                XCTAssertEqual(location.path, source.path)
                XCTAssertEqual(code, denied)
            }
            XCTAssertTrue(try Data(contentsOf: target).isEmpty)
            XCTAssertEqual(try Data(contentsOf: source), bytes)
            var retainedMetadata = Data(count: metadata.count)
            let read = retainedMetadata.withUnsafeMutableBytes {
                getxattr(source.path, attribute, $0.baseAddress, $0.count, 0, 0)
            }
            XCTAssertEqual(read, metadata.count)
            XCTAssertEqual(retainedMetadata, metadata)
        }
    }

    func testFolderCannotBeMovedOrCopiedIntoAChildViaASymlink() throws {
        try withFixture { root in
            let tree = try FileOperations.createFolder(in: root, named: "Tree")
            let child = try FileOperations.createFolder(in: tree.url, named: "Child")
            let alias = root.appendingPathComponent("Shortcut")
            try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: child.url)
            XCTAssertThrowsError(try FileOperations.copy(tree, into: alias)) {
                guard case FileOperationError.destinationInsideSource = $0 else {
                    return XCTFail("Expected descendant refusal, got \($0)")
                }
            }
            XCTAssertThrowsError(try FileOperations.move(tree, into: alias)) {
                guard case FileOperationError.destinationInsideSource = $0 else {
                    return XCTFail("Expected descendant refusal, got \($0)")
                }
            }
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: tree.url.path), ["Child"])
        }
    }

    func testCancellationAfterStagingLeavesNoPublishedCopy() throws {
        try withFixture { root in
            let source = try write("source", named: "item.txt", in: root)
            let target = try FileOperations.createFolder(in: root, named: "Destination")
            var checks = 0
            XCTAssertThrowsError(try FileOperations.copy(source, into: target.url, isCancelled: {
                checks += 1
                return checks == 2
            })) {
                XCTAssertTrue($0 is CancellationError)
            }
            XCTAssertEqual(checks, 2)
            XCTAssertEqual(try contents(source), "source")
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: target.url.path), [])
        }
    }

    func testDirectoryCopyCanStopInsideTheTreeAndPreservesLinks() throws {
        try withFixture { root in
            let source = try FileOperations.createFolder(in: root, named: "Source")
            let child = try FileOperations.createFolder(in: source.url, named: "Child")
            let image = try write("synthetic image", named: "Photo.PNG", in: child.url)
            let outside = try write("outside", named: "Outside.txt", in: root)
            let link = source.url.appendingPathComponent("Shortcut")
            try FileManager.default.createSymbolicLink(at: link, withDestinationURL: outside.url)
            let target = try FileOperations.createFolder(in: root, named: "Destination")

            var checks = 0
            XCTAssertThrowsError(try FileOperations.copy(source, into: target.url, isCancelled: {
                checks += 1
                return checks >= 5
            })) { XCTAssertTrue($0 is CancellationError, "\($0)") }
            XCTAssertEqual(checks, 5)
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: target.url.path), [])
            XCTAssertEqual(try contents(image), "synthetic image")
            XCTAssertEqual(try contents(outside), "outside")

            let copied = try FileOperations.copy(source, into: target.url).copied
            XCTAssertEqual(try Data(contentsOf: copied.url.appendingPathComponent("Child/Photo.PNG")),
                           Data("synthetic image".utf8))
            XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: copied.url.appendingPathComponent("Shortcut").path),
                           outside.url.path)
        }
    }

    func testDirectoryCopyDoesNotPublishWhenANestedFileCannotBeRead() throws {
        try XCTSkipIf(geteuid() == 0, "Requires ordinary file access permissions")
        try withFixture { root in
            let source = try FileOperations.createFolder(in: root, named: "Source")
            let child = try FileOperations.createFolder(in: source.url, named: "Child")
            let readable = try write("readable bytes", named: "Readable.txt", in: child.url)
            let blocked = try write("blocked bytes", named: "Blocked.txt", in: child.url)
            let destination = try FileOperations.createFolder(in: root, named: "Destination")
            XCTAssertEqual(chmod(blocked.url.path, 0), 0)
            defer { _ = chmod(blocked.url.path, 0o600) }

            XCTAssertThrowsError(try FileOperations.copy(source, into: destination.url)) {
                guard case FileOperationError.system(_, let location, let code) = $0 else {
                    return XCTFail("Expected the original access error, got \($0)")
                }
                XCTAssertEqual(location.path, blocked.url.path)
                XCTAssertEqual(code, EACCES)
            }
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: destination.url.path).isEmpty)
            XCTAssertEqual(try contents(readable), "readable bytes")
            XCTAssertEqual(chmod(blocked.url.path, 0o600), 0)
            XCTAssertEqual(try contents(blocked), "blocked bytes")
        }
    }

    func testDirectoryCopyReportsNestedBytesAndCancelsBeforePublication() throws {
        try withFixture { root in
            let source = try FileOperations.createFolder(in: root, named: "Source")
            let child = try FileOperations.createFolder(in: source.url, named: "Child")
            let file = child.url.appendingPathComponent("Nested.bin")
            let data = Data(repeating: 0x5a, count: 2 * 1024 * 1024)
            try data.write(to: file, options: .withoutOverwriting)
            let destination = try FileOperations.createFolder(in: root, named: "Destination")

            var sample: NestedCopyProgress?
            XCTAssertThrowsError(try FileOperations.copy(
                source, into: destination.url, isCancelled: { sample != nil }, onNestedProgress: { sample = $0 }
            )) { XCTAssertTrue($0 is CancellationError, "\($0)") }
            let reported = try XCTUnwrap(sample)
            XCTAssertEqual(reported.source.path, file.path)
            XCTAssertGreaterThan(reported.written, 0)
            XCTAssertLessThanOrEqual(reported.written, Int64(data.count))
            XCTAssertEqual(reported.total, Int64(data.count))
            XCTAssertGreaterThanOrEqual(reported.elapsed, 0)
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: destination.url.path).isEmpty)
            XCTAssertEqual(try Data(contentsOf: file), data)
        }
    }

    func testVerifiedTreeChecksNestedContentAndRefusesAnEditedDescendant() throws {
        try withFixture { root in
            let source = try FileOperations.createFolder(in: root, named: "Project")
            let nested = try FileOperations.createFolder(in: source.url, named: "Nested")
            let note = try write("original", named: "Note.txt", in: nested.url)
            let link = source.url.appendingPathComponent("Shortcut")
            try FileManager.default.createSymbolicLink(atPath: link.path, withDestinationPath: "Nested/Note.txt")
            let destination = try FileOperations.createFolder(in: root, named: "Destination")
            let original = try TreeInventory(source)
            let copied = try FileOperations.copy(source, into: destination.url).copied
            let copy = try TreeInventory(copied)
            XCTAssertEqual(try original.contentDigest(isCancelled: { false }), try copy.contentDigest(isCancelled: { false }))
            let reviewed = try ReviewedTree(copied, descendants: copy.descendants)
            XCTAssertNoThrow(try reviewed.verify())

            let copiedNote = copied.url.appendingPathComponent("Nested/Note.txt")
            try Data("modified".utf8).write(to: copiedNote)
            XCTAssertThrowsError(try reviewed.verify())
            XCTAssertEqual(try contents(note), "original")
            XCTAssertEqual(try Data(contentsOf: copiedNote), Data("modified".utf8))
        }
    }

    func testStreamCopyReportsBytesAndStopsBetweenWrites() throws {
        try withFixture { root in
            let source = root.appendingPathComponent("Large.bin")
            let destination = root.appendingPathComponent("Staged.bin")
            let data = Data(repeating: 0x5a, count: 8 * 1024 * 1024)
            try data.write(to: source, options: .withoutOverwriting)
            let input = open(source.path, O_RDONLY | O_CLOEXEC)
            XCTAssertGreaterThanOrEqual(input, 0)
            defer { _ = close(input) }
            let output = open(destination.path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0o600)
            XCTAssertGreaterThanOrEqual(output, 0)
            defer { _ = close(output) }

            var reported: Int64 = 0
            XCTAssertThrowsError(try FileOperations.copyFileDescriptors(
                input, output, to: destination, size: Int64(data.count),
                isCancelled: { reported > 0 }, onProgress: { written, total in
                    XCTAssertEqual(total, Int64(data.count))
                    reported = written
                }
            )) { XCTAssertTrue($0 is CancellationError, "\($0)") }
            XCTAssertGreaterThan(reported, 0)
            XCTAssertLessThan(reported, Int64(data.count))
            XCTAssertEqual(try Data(contentsOf: source), data)
        }
    }

    func testReviewedReplacementRetainsBothVersionsAndCanBeUndone() throws {
        try withFixture { root in
            let targetFolder = try FileOperations.createFolder(in: root, named: "Destination")
            let trash = try FileOperations.createFolder(in: root, named: "Test Trash")
            let source = try write("new contents", named: "Report.txt", in: root)
            let target = try write("prior contents", named: "Report.txt", in: targetFolder.url)
            func sendToTrash(_ file: FileReference) throws -> TrashReceipt {
                let moved = try FileOperations.move(file, into: trash.url, named: UUID().uuidString).moved
                return TrashReceipt(original: file, trashedAt: Date(), trashLocation: moved.url, trashed: moved)
            }

            let replaced = try FileOperations.replaceCopy(ReviewedFile(source), replacing: ReviewedFile(target),
                                                         isCancelled: { false }, sendToTrash: sendToTrash)
            XCTAssertEqual(try contents(replaced.installed.reference), "new contents")
            XCTAssertEqual(try contents(XCTUnwrap(replaced.previous.trashed)), "prior contents")
            XCTAssertEqual(try contents(source), "new contents")

            let undone = try FileOperations.restoreReplacedFile(replaced.previous, replacing: replaced.installed,
                                                              sendToTrash: sendToTrash)
            XCTAssertEqual(undone.installed.reference.identity, target.identity)
            XCTAssertEqual(try contents(undone.installed.reference), "prior contents")
            XCTAssertEqual(try contents(XCTUnwrap(undone.previous.trashed)), "new contents")
        }
    }

    func testReviewedReplacementRejectsEditsAndKeepsALateArrival() throws {
        try withFixture { root in
            let targetFolder = try FileOperations.createFolder(in: root, named: "Destination")
            let trash = try FileOperations.createFolder(in: root, named: "Test Trash")
            let source = try write("new contents", named: "Report.txt", in: root)
            let target = try write("prior contents", named: "Report.txt", in: targetFolder.url)
            let stale = try ReviewedFile(target)
            try Data("edited after review".utf8).write(to: target.url)
            XCTAssertThrowsError(try FileOperations.replaceCopy(ReviewedFile(source), replacing: stale,
                                                                isCancelled: { false }, sendToTrash: { _ in
                XCTFail("The edited target must not go to Trash.")
                throw CancellationError()
            })) { error in
                guard case FileOperationError.sourceChanged = error else { return XCTFail("\(error)") }
            }

            let current = try ReviewedFile(FileReference(url: target.url))
            var backup: FileReference?
            XCTAssertThrowsError(try FileOperations.replaceCopy(ReviewedFile(source), replacing: current,
                                                                isCancelled: { false }, sendToTrash: { file in
                let moved = try FileOperations.move(file, into: trash.url).moved
                backup = moved
                _ = try write("arrived late", named: "Report.txt", in: targetFolder.url)
                return TrashReceipt(original: file, trashedAt: Date(), trashLocation: moved.url, trashed: moved)
            })) { error in
                guard case FileOperationError.replacementNeedsReview = error else { return XCTFail("\(error)") }
            }
            XCTAssertEqual(try contents(try XCTUnwrap(backup)), "edited after review")
            XCTAssertEqual(try contents(FileReference(url: target.url)), "arrived late")
            XCTAssertEqual(try contents(source), "new contents")
        }
    }

    func testCopyRefusesAnItemThatArrivesBeforePublication() throws {
        try withFixture { root in
            let source = try write("source", named: "item.txt", in: root)
            let target = try FileOperations.createFolder(in: root, named: "Destination")
            let occupiedURL = target.url.appendingPathComponent(source.url.lastPathComponent)
            var checks = 0
            var arrivalError: Error?
            assertCollision(try FileOperations.copy(source, into: target.url, isCancelled: {
                checks += 1
                if checks == 2 {
                    do {
                        try Data("arrival".utf8).write(to: occupiedURL, options: .withoutOverwriting)
                    } catch {
                        arrivalError = error
                    }
                }
                return false
            }))
            XCTAssertNil(arrivalError)
            XCTAssertEqual(try String(contentsOf: occupiedURL, encoding: .utf8), "arrival")
            XCTAssertEqual(try contents(source), "source")
        }
    }

    func testRestoreUsesOriginalNameAndAnExplicitAlternateFolder() throws {
        try withFixture { root in
            let source = try write("original", named: "plan.txt", in: root)
            let holding = try FileOperations.createFolder(in: root, named: "Synthetic Trash")
            let moved = try FileOperations.move(source, into: holding.url, named: "plan 1.txt").moved
            let receipt = TrashReceipt(
                original: source, trashedAt: Date(), trashLocation: moved.url, trashed: moved
            )
            _ = try write("occupant", named: "plan.txt", in: root)
            assertCollision(try FileOperations.restore(receipt))
            let alternative = try FileOperations.createFolder(in: root, named: "Alternative")
            let restored = try FileOperations.restore(receipt, into: alternative.url)
            XCTAssertEqual(restored.url.lastPathComponent, "plan.txt")
            XCTAssertEqual(restored.identity, source.identity)
            XCTAssertEqual(try contents(restored), "original")
        }
    }

    func testKeepBothFindsAFreeNameWithoutReplacingAnOccupant() throws {
        try withFixture { root in
            let source = try write("source", named: "plan.txt", in: root)
            let folder = try FileOperations.createFolder(in: root, named: "Destination")
            let first = try write("first", named: "plan.txt", in: folder.url)
            let second = try write("second", named: "plan (2).txt", in: folder.url)

            let copy = try FileOperations.copy(source, into: folder.url, conflictPolicy: .keepBoth)
            XCTAssertEqual(copy.copied.url.lastPathComponent, "plan (3).txt")
            XCTAssertEqual(try contents(copy.copied), "source")
            XCTAssertEqual(try contents(source), "source")
            XCTAssertEqual(try contents(first), "first")
            XCTAssertEqual(try contents(second), "second")
        }
    }

    private func withFixture(_ test: (URL) throws -> Void) throws {
        let base = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        let folder = URL(fileURLWithPath: base, isDirectory: true)
            .appendingPathComponent("richfiles-operations-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: folder) }
        try test(folder)
    }

    private func write(_ text: String, named name: String, in folder: URL) throws -> FileReference {
        let url = folder.appendingPathComponent(name)
        try Data(text.utf8).write(to: url, options: .withoutOverwriting)
        return try FileReference(url: url)
    }

    private func contents(_ reference: FileReference) throws -> String {
        try String(contentsOf: reference.url, encoding: .utf8)
    }

    private func assertCollision<T>(
        _ action: @autoclosure () throws -> T,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        XCTAssertThrowsError(try action(), file: file, line: line) {
            guard case FileOperationError.destinationExists = $0 else {
                return XCTFail("Expected a collision, got \($0)", file: file, line: line)
            }
        }
    }
}
