import AppKit
import Carbon.HIToolbox
import RichFilesCore
import XCTest

final class DesktopTrashShortcutTests: XCTestCase {
    func testTaggedForwardDeleteProducesFindersCommandDelete() throws {
        for flags: CGEventFlags in [[], .maskSecondaryFn, .maskNumericPad, [.maskSecondaryFn, .maskNumericPad]] {
            let event = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(kVK_ForwardDelete), keyDown: true))
            event.flags = flags
            var forward = UniChar(NSDeleteFunctionKey)
            event.keyboardSetUnicodeString(stringLength: 1, unicodeString: &forward)

            let replacement = try XCTUnwrap(DesktopTrashShortcut.replacement(for: event) { true })
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

    func testBackspaceModifiedForwardDeleteAndRepeatsNeverInspectSelection() throws {
        for (key, flags, repeated): (Int, CGEventFlags, Int64) in [
            (kVK_Delete, [], 0), (kVK_ForwardDelete, .maskShift, 0), (kVK_ForwardDelete, [], 1)
        ] {
            let event = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(key), keyDown: true))
            event.flags = flags
            event.setIntegerValueField(.keyboardEventAutorepeat, value: repeated)
            XCTAssertNil(DesktopTrashShortcut.replacement(for: event) { XCTFail("Unexpected selection check"); return true })
            XCTAssertEqual(event.getIntegerValueField(.keyboardEventKeycode), Int64(key))
            XCTAssertEqual(event.flags, flags)
        }
        let forward = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(kVK_ForwardDelete), keyDown: true))
        forward.flags = []
        XCTAssertNil(DesktopTrashShortcut.replacement(for: forward) { false })
        XCTAssertEqual(forward.getIntegerValueField(.keyboardEventKeycode), Int64(kVK_ForwardDelete))
        XCTAssertTrue(forward.flags.isEmpty)
    }
}
