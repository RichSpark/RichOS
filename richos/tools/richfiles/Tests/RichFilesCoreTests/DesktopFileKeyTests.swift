import AppKit
import Carbon.HIToolbox
@testable import RichFilesCore
import XCTest

final class DesktopFileKeyTests: XCTestCase {
    func testOnlyTheInitialNewFolderShortcutStartsTracking() throws {
        let key = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(kVK_ANSI_N), keyDown: true))
        key.flags = [.maskCommand, .maskShift]
        XCTAssertEqual(DesktopFileKey.classify(key), .newFolder)

        key.setIntegerValueField(.keyboardEventAutorepeat, value: 1)
        XCTAssertEqual(DesktopFileKey.classify(key), .passThrough)
        key.setIntegerValueField(.keyboardEventAutorepeat, value: 0)
        key.flags = [.maskCommand, .maskShift, .maskControl]
        XCTAssertEqual(DesktopFileKey.classify(key), .passThrough)
        key.flags = [.maskCommand]
        XCTAssertEqual(DesktopFileKey.classify(key), .passThrough)
    }

    func testDesktopDefaultsAndIndependentOptOutsSurviveUpgradeAndRelaunch() throws {
        typealias Preferences = WindowGesturePreferences
        let domain = "richfiles.tests.desktop." + UUID().uuidString
        let store = try XCTUnwrap(UserDefaults(suiteName: domain))
        defer { store.removePersistentDomain(forName: domain) }

        XCTAssertEqual(Preferences.selection(in: store), .desktop)
        // An existing installation may have its own window gestures and Delete preference.
        store.set(Preferences.Selection.dock.rawValue, forKey: "windowGestures.selection")
        store.set(false, forKey: "desktopDelete.enabled")
        XCTAssertEqual(Preferences.selection(in: store), [.dock, .desktopFolder])
        Preferences.setEnabled(false, for: .desktopFolder, in: store)
        Preferences.setEnabled(true, for: .tiling, in: store)
        let reopened = try XCTUnwrap(UserDefaults(suiteName: domain))
        XCTAssertEqual(Preferences.selection(in: reopened), [.dock, .tiling])
        Preferences.setEnabled(true, for: .desktopDelete, in: reopened)
        XCTAssertEqual(Preferences.selection(in: reopened), [.dock, .tiling, .desktopDelete])
        Preferences.setEnabled(true, for: .desktopFolder, in: reopened)
        XCTAssertEqual(Preferences.selection(in: reopened), [.dock, .tiling, .desktopFolder, .desktopDelete])
    }

    func testForwardDeleteWorksWithPhysicalKeyboardFlagsAndPayload() throws {
        for flags: CGEventFlags in [[], .maskSecondaryFn, .maskNumericPad, [.maskSecondaryFn, .maskNumericPad]] {
            let event = try key(kVK_ForwardDelete)
            event.flags = flags
            var forward = UniChar(NSDeleteFunctionKey)
            event.keyboardSetUnicodeString(stringLength: 1, unicodeString: &forward)
            XCTAssertEqual(DesktopFileKey.classify(event), .forwardDelete)
        }
    }

    func testBackspaceRemainsDistinctFromForwardDelete() throws {
        let event = try key(kVK_Delete)
        XCTAssertEqual(DesktopFileKey.classify(event), .backspace)
        event.flags = .maskSecondaryFn
        XCTAssertEqual(DesktopFileKey.classify(event), .passThrough)
    }

    func testModifiedAndRepeatedForwardDeletePassThrough() throws {
        let event = try key(kVK_ForwardDelete)
        event.flags = .maskShift
        XCTAssertEqual(DesktopFileKey.classify(event), .passThrough)
        event.flags = []
        event.setIntegerValueField(.keyboardEventAutorepeat, value: 1)
        XCTAssertEqual(DesktopFileKey.classify(event), .passThrough)
        XCTAssertEqual(DesktopFileKey.classify(try key(kVK_ForwardDelete, down: false)), .passThrough)
    }

    private func key(_ code: Int, down: Bool = true) throws -> CGEvent {
        let event = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(code), keyDown: down))
        event.flags = []
        return event
    }
}
