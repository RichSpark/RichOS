import AppKit
import Carbon.HIToolbox
@testable import RichFilesCore
import XCTest

final class DesktopFileKeyTests: XCTestCase {
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

    func testTaggedForwardDeleteProducesFindersCommandDelete() throws {
        for flags: CGEventFlags in [[], .maskSecondaryFn, .maskNumericPad, [.maskSecondaryFn, .maskNumericPad]] {
            let event = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(kVK_ForwardDelete), keyDown: true))
            event.flags = flags
            var forward = UniChar(NSDeleteFunctionKey)
            event.keyboardSetUnicodeString(stringLength: 1, unicodeString: &forward)

            guard case let .moveToTrash(replacement) = DesktopFileKey.action(for: event, desktopHasFocus: { true }) else {
                return XCTFail("The selected Desktop folder should receive Move to Trash")
            }
            XCTAssertFalse(event === replacement)
            XCTAssertEqual(replacement.getIntegerValueField(.keyboardEventKeycode), Int64(kVK_Delete))
            XCTAssertEqual(replacement.flags, .maskCommand)
            XCTAssertEqual(replacement.timestamp, event.timestamp)
            var length = 0
            var actual = UniChar(0)
            replacement.keyboardGetUnicodeString(maxStringLength: 1, actualStringLength: &length, unicodeString: &actual)
            XCTAssertNotEqual(actual, forward)
        }
    }

    func testBackspaceIsInertOnTheDesktopAndPassesThroughInOtherViews() throws {
        let key = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(kVK_Delete), keyDown: true))
        key.flags = []
        for repeated: Int64 in [0, 1] {
            key.setIntegerValueField(.keyboardEventAutorepeat, value: repeated)
            guard case .ignoreBackspace = DesktopFileKey.action(for: key, desktopHasFocus: { true }) else {
                return XCTFail("A Desktop Backspace should have no effect")
            }
            guard case .passThrough = DesktopFileKey.action(for: key, desktopHasFocus: { false }) else {
                return XCTFail("RichFiles and text editors handle their own Backspace")
            }
        }
    }

    func testModifiedAndRepeatedForwardDeleteNeverInspectSelection() throws {
        for (key, flags, repeated): (Int, CGEventFlags, Int64) in [
            (kVK_Delete, .maskCommand, 0), (kVK_ForwardDelete, .maskShift, 0), (kVK_ForwardDelete, [], 1)
        ] {
            let event = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(key), keyDown: true))
            event.flags = flags
            event.setIntegerValueField(.keyboardEventAutorepeat, value: repeated)
            let action = DesktopFileKey.action(for: event) { XCTFail("Unexpected selection check"); return true }
            guard case .passThrough = action else { return XCTFail("Expected the original shortcut") }
            XCTAssertEqual(event.getIntegerValueField(.keyboardEventKeycode), Int64(key))
            XCTAssertEqual(event.flags, flags)
        }
        let forward = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(kVK_ForwardDelete), keyDown: true))
        forward.flags = []
        guard case .passThrough = DesktopFileKey.action(for: forward, desktopHasFocus: { false }) else {
            return XCTFail("Forward Delete outside the Desktop should pass through")
        }
        XCTAssertEqual(forward.getIntegerValueField(.keyboardEventKeycode), Int64(kVK_ForwardDelete))
        XCTAssertTrue(forward.flags.isEmpty)
    }
}
