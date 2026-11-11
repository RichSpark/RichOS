import Combine
import Foundation
import XCTest
@testable import RichFiles

final class BrowserModelTests: XCTestCase {
    func testFolderRevealChoosesNearestRootAndIncludesOnlyTheNeededHiddenChild() async throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("RichFilesTree-\(UUID().uuidString)", isDirectory: true)
        let project = root.appendingPathComponent("Project", isDirectory: true)
        let hidden = project.appendingPathComponent(".Private", isDirectory: true)
        let current = hidden.appendingPathComponent("Notes", isDirectory: true)
        try FileManager.default.createDirectory(at: current, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        for child in [".Other", "Visible"] {
            try FileManager.default.createDirectory(at: project.appendingPathComponent(child),
                                                    withIntermediateDirectories: false)
        }

        let reveal = try XCTUnwrap(FolderTreeReveal(folder: current, roots: [root, project]))
        XCTAssertEqual(reveal.root, project.path)
        XCTAssertEqual(reveal.nextChild, [project.path: hidden.lastPathComponent, hidden.path: current.lastPathComponent])
        let children = try await BrowserModel.folderChildren(of: project, revealing: reveal.nextChild[project.path])
        XCTAssertEqual(Set(children.map(\.name)), [".Private", "Visible"])
        let revealed = try XCTUnwrap(children.first { $0.name == hidden.lastPathComponent })
        let next = reveal.nextChild[revealed.url.standardizedFileURL.path]
        XCTAssertEqual(next, current.lastPathComponent)
        let grandchildren = try await BrowserModel.folderChildren(of: revealed.url, revealing: next)
        XCTAssertEqual(grandchildren.map(\.name), [current.lastPathComponent])
        let visible = try await BrowserModel.folderChildren(of: project)
        XCTAssertEqual(visible.map(\.name), ["Visible"])
    }

    @MainActor
    func testListDefaultNavigationSearchAndPreferences() async throws {
        let temporary = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        let root = URL(fileURLWithPath: temporary, isDirectory: true)
            .resolvingSymlinksInPath()
            .appendingPathComponent("RichFilesBrowser-\(UUID().uuidString)", isDirectory: true)
        let projects = root.appendingPathComponent("Projects", isDirectory: true)
        try FileManager.default.createDirectory(at: projects, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        for url in [
            root.appendingPathComponent("Report.TXT"),
            projects.appendingPathComponent("Photo.PNG"),
            projects.appendingPathComponent(".hidden.png"),
            projects.appendingPathComponent("Notes.md")
        ] {
            try Data("synthetic browser fixture".utf8).write(to: url)
        }

        let domain = "app.richos.richfiles.tests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: domain))
        defer { defaults.removePersistentDomain(forName: domain) }
        let model = BrowserModel(start: root, defaults: defaults)
        let firstTab = model.currentTabID
        try await waitForListing(model, at: root)
        XCTAssertEqual(model.viewMode, .list)
        XCTAssertFalse(model.showHidden)
        XCTAssertEqual(Set(model.entries.map(\.name)), ["Projects", "Report.TXT"])
        let report = try XCTUnwrap(model.entries.first { $0.name == "Report.TXT" })
        model.selected = [report.id]
        model.scrollAnchor = report.id

        let suggestions = try await BrowserModel.addressSuggestions("Pro", relativeTo: root)
        XCTAssertEqual(suggestions.map(\.path), [projects.path])
        let children = try await BrowserModel.folderChildren(of: root)
        XCTAssertEqual(children.map(\.name), ["Projects"])
        model.openAddress("Projects")
        try await waitForListing(model, at: projects)
        XCTAssertTrue(model.canGoBack)
        XCTAssertTrue(model.selected.isEmpty)
        XCTAssertEqual(Set(model.entries.map(\.name)), ["Notes.md", "Photo.PNG"])
        let photo = try XCTUnwrap(model.entries.first { $0.name == "Photo.PNG" })
        model.selected = [photo.id]

        model.goBack()
        try await waitForListing(model, at: root)
        XCTAssertTrue(model.canGoForward)
        XCTAssertEqual(model.selected, [report.id])
        XCTAssertEqual(model.scrollAnchor, report.id)
        model.goForward()
        try await waitForListing(model, at: projects)
        XCTAssertEqual(model.selected, [photo.id])

        model.newTab(root)
        let searchTab = model.currentTabID
        XCTAssertNotEqual(searchTab, firstTab)
        XCTAssertEqual(model.tabs.count, 2)
        try await waitForListing(model, at: root)
        XCTAssertFalse(model.canGoBack)
        model.searchRecursively = false
        model.searchText = "*.png"
        model.submitSearch()
        try await waitForListing(model, at: root)
        XCTAssertTrue(model.searchActive)
        XCTAssertTrue(model.entries.isEmpty)
        model.searchRecursively = true
        try await waitForListing(model, at: root)
        XCTAssertEqual(model.entries.map(\.name), ["Photo.PNG"])
        model.showHidden = true
        try await waitForListing(model, at: root)
        XCTAssertEqual(Set(model.entries.map(\.name)), [".hidden.png", "Photo.PNG"])
        model.searchContent = true
        model.searchText = "synthetic browser fixture"
        model.submitSearch()
        try await waitForListing(model, at: root)
        XCTAssertEqual(Set(model.entries.map(\.name)), ["Notes.md", "Report.TXT"])
        XCTAssertEqual(model.scanReport?.contentCoverage?.searchedFiles, 2)
        XCTAssertEqual(model.scanReport?.contentCoverage?.skippedItems, 2)

        model.switchTab(firstTab)
        try await waitForListing(model, at: projects)
        XCTAssertEqual(model.selected, [photo.id])
        XCTAssertFalse(model.searchActive)
        XCTAssertFalse(model.searchContent)
        model.setFolderViewOverride(true)
        model.viewMode = .icons
        model.iconSize = 96
        model.addFavorite(projects)

        model.switchTab(searchTab)
        try await waitForListing(model, at: root)
        XCTAssertEqual(model.viewMode, .list)
        XCTAssertTrue(model.searchActive)
        XCTAssertTrue(model.searchContent)
        XCTAssertEqual(model.searchText, "synthetic browser fixture")
        XCTAssertEqual(Set(model.entries.map(\.name)), ["Notes.md", "Report.TXT"])

        let session = try XCTUnwrap(model.sessionData())
        let restored = BrowserModel(start: projects, defaults: defaults)
        XCTAssertTrue(restored.restoreSession(session))
        try await waitForListing(restored, at: root)
        XCTAssertEqual(restored.tabs.map { $0.folder.path }, [projects.path, root.path])
        XCTAssertEqual(restored.currentTabID, restored.tabs.last?.id)
        XCTAssertTrue(restored.searchContent)
        XCTAssertEqual(restored.searchText, "synthetic browser fixture")
        XCTAssertEqual(Set(restored.entries.map(\.name)), ["Notes.md", "Report.TXT"])
        restored.switchTab(try XCTUnwrap(restored.tabs.first?.id))
        try await waitForListing(restored, at: projects)
        XCTAssertEqual(restored.selected, [photo.id])
        XCTAssertTrue(restored.canGoBack)
        XCTAssertFalse(restored.restoreSession(Data("corrupt session".utf8)))

        model.closeTab(searchTab)
        try await waitForListing(model, at: projects)
        XCTAssertEqual(model.tabs.count, 1)
        XCTAssertEqual(model.currentTabID, firstTab)
        XCTAssertEqual(model.viewMode, .icons)

        let reopened = BrowserModel(start: projects, defaults: defaults)
        try await waitForListing(reopened, at: projects)
        XCTAssertTrue(reopened.remembersFolderView)
        XCTAssertEqual(reopened.viewMode, .icons)
        XCTAssertEqual(reopened.iconSize, 96)
        XCTAssertTrue(reopened.showHidden)
        XCTAssertEqual(reopened.favorites.map(\.path), [projects.path])
        reopened.goUp()
        try await waitForListing(reopened, at: root)
        XCTAssertFalse(reopened.remembersFolderView)
        XCTAssertEqual(reopened.viewMode, .list)
    }

    @MainActor
    private func waitForListing(
        _ model: BrowserModel,
        at folder: URL,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async throws {
        func complete() -> Bool {
            model.folder.path == folder.path && !model.busy && model.scanReport != nil
        }
        if complete() { return }

        // A published scan report proves completion. The deadline only stops a hung fixture.
        let finished = expectation(description: "Listing finished for \(folder.lastPathComponent)")
        var reached = false
        let observation = model.objectWillChange.sink { _ in
            Task { @MainActor in
                guard !reached, complete() else { return }
                reached = true
                finished.fulfill()
            }
        }
        defer { observation.cancel() }
        await fulfillment(of: [finished], timeout: 10)
        guard reached else {
            XCTFail("Listing did not finish. Browser status was \(model.status).", file: file, line: line)
            throw ListingTimeout()
        }
    }

    private struct ListingTimeout: Error {}
}
