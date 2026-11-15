import Foundation
import XCTest
@testable import RichFilesCore

final class GitRepositoryTests: XCTestCase {
    func testPinnedHistoryComparisonAndRename() throws {
        let git = URL(fileURLWithPath: "/usr/bin/git")
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        @discardableResult func run(_ arguments: String...) throws -> String {
            let command = Process()
            let output = Pipe()
            command.executableURL = git
            command.currentDirectoryURL = root
            command.environment = ["PATH": "/usr/bin:/bin", "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null"]
            command.arguments = arguments
            command.standardOutput = output
            command.standardError = FileHandle.nullDevice
            try command.run()
            let data = output.fileHandleForReading.readDataToEndOfFile()
            command.waitUntilExit()
            guard command.terminationStatus == 0 else { throw CocoaError(.fileWriteUnknown) }
            return String(decoding: data, as: UTF8.self).trimmingCharacters(in: .newlines)
        }

        try run("init", "-b", "main")
        try run("config", "user.name", "Example")
        try run("config", "user.email", "example@example.test")
        let original = root.appendingPathComponent("original.md")
        let moved = root.appendingPathComponent("renamed.md")
        let stable = (0..<30).map { "unchanged line \($0)\n" }.joined()
        try (stable + "old\n").write(to: original, atomically: true, encoding: .utf8)
        try run("add", "--", "original.md")
        try run("commit", "-m", "First")
        let old = try run("rev-parse", "HEAD")
        try run("switch", "-c", "review")
        try run("mv", "original.md", "renamed.md")
        try (stable + "new\n").write(to: moved, atomically: true, encoding: .utf8)
        try run("commit", "-am", "Rename and update")
        let reviewed = try run("rev-parse", "HEAD")
        try run("switch", "main")
        try run("merge", "--no-ff", "review", "-m", "Land review")
        try run("config", "diff.renames", "false")
        let repo = try GitRepository(folder: root, git: git)
        let pinned = try repo.mainTip()
        let commits = try repo.history(tip: pinned, limit: 20)
        XCTAssertEqual(commits.first?.parents, [old, reviewed])
        XCTAssertEqual(Set(commits.map(\.id)), Set([old, reviewed, pinned]))
        let subsequent = try repo.history(tip: pinned, limit: 2, offset: 1)
        XCTAssertEqual(subsequent.map(\.id), commits.dropFirst().map(\.id))
        XCTAssertEqual(subsequent.map(\.graph), commits.dropFirst().map(\.graph))

        let marker = GitLandEvidence(id: pinned, paths: [], before: old, reviewed: reviewed, at: 0)
        let comparison = try repo.comparison(XCTUnwrap(commits.first), land: marker)
        XCTAssertEqual(comparison.before, old)
        XCTAssertEqual(comparison.basis, .land)
        let file = try XCTUnwrap(comparison.files.first)
        XCTAssertEqual(file.status, .renamed)
        XCTAssertEqual(file.oldPath, "original.md")
        let changed = try repo.hunks(file).flatMap(\.rows).filter(\.changed)
        XCTAssertEqual(changed.first?.before?.text, "old")
        XCTAssertEqual(changed.first?.after?.text, "new")
        XCTAssertEqual(changed.first?.after?.number, 31)

        let legacy = GitLandEvidence(id: pinned, paths: ["renamed.md"], before: nil, reviewed: nil, at: 0)
        XCTAssertEqual(try repo.comparison(XCTUnwrap(commits.first), land: legacy).before, old)
        let wrong = GitLandEvidence(id: pinned, paths: ["renamed.md"], before: reviewed, reviewed: old, at: 0)
        XCTAssertEqual(try repo.comparison(XCTUnwrap(commits.first), land: wrong).basis, .unavailable)
        try run("switch", "review")
        XCTAssertEqual(try repo.mainTip(), pinned)
        XCTAssertEqual(try repo.checkoutName(), "review")

        try FileManager.default.removeItem(at: moved)
        try FileManager.default.createSymbolicLink(at: moved, withDestinationURL: root)
        XCTAssertThrowsError(try repo.liveFile(file))
    }
}
