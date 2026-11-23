import Foundation
import XCTest
@testable import RichFilesCore

final class GlanceHelperTests: XCTestCase {
    func testBackgroundAndMenuBarDefaultOnAndSavedChoicesSurvive() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let store = GlanceHelper.Store(support: folder)
        let initial = try store.read()
        XCTAssertTrue(initial.firstUse)
        XCTAssertTrue(initial.preferences.background)
        XCTAssertTrue(initial.preferences.menuBar)
        XCTAssertFalse(initial.preferences.enabled)
        let optedIn = initial.preferences.settingBackground(true, firstUse: initial.firstUse)
        XCTAssertTrue(optedIn.enabled)
        XCTAssertEqual(optedIn.shortcut.label, "⌃⇧C")
        try store.write(optedIn)
        XCTAssertEqual(try store.read().preferences, optedIn)
        try store.restore(initial)
        XCTAssertTrue(try store.read().firstUse)

        let legacy = folder.appendingPathComponent("com.richos.app/richglance-shortcut.json")
        try Data(#"{"enabled":false,"shortcut":{"code":"KeyP","control":true,"option":true,"shift":false,"command":false}}"#.utf8)
            .write(to: legacy)
        let existing = try store.read()
        XCTAssertFalse(existing.firstUse)
        XCTAssertTrue(existing.preferences.background)
        let migrated = existing.preferences.settingBackground(true, firstUse: existing.firstUse)
        XCTAssertFalse(migrated.enabled)
        XCTAssertTrue(migrated.menuBar)
        XCTAssertEqual(migrated.shortcut.label, "⌃⌥P")
        var optedOut = existing.preferences.settingBackground(false, firstUse: false)
        optedOut.menuBar = false
        try store.write(optedOut)
        XCTAssertEqual(try store.read().preferences, optedOut)
    }

    func testOnlyOneHelperCanOwnTheShortcutLease() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let store = GlanceHelper.Store(support: folder)
        let owner = try XCTUnwrap(store.lock("owner", nonblocking: true))
        XCTAssertNil(try store.lock("owner", nonblocking: true))
        try owner.close()
        let replacement = try XCTUnwrap(store.lock("owner", nonblocking: true))
        try replacement.close()
    }

    func testShortcutMappingRejectsPlainKeysAndMacScreenshotChords() throws {
        let standard = try GlanceHelper.Shortcut.standard.carbon()
        XCTAssertEqual(standard.key, 8)
        XCTAssertEqual(standard.modifiers, (1 << 12) | (1 << 9))
        let plainC = try XCTUnwrap(GlanceHelper.Shortcut(keyCode: 8, control: false, option: false,
                                                       shift: false, command: false))
        XCTAssertThrowsError(try plainC.carbon())
        let screenshot = try XCTUnwrap(GlanceHelper.Shortcut(keyCode: 21, control: false, option: false,
                                                            shift: true, command: true))
        XCTAssertThrowsError(try screenshot.carbon())
    }
}
