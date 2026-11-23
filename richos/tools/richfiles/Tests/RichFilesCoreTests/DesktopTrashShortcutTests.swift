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

            XCTAssertTrue(DesktopTrashShortcut.rewrite(event) { true })
            XCTAssertEqual(event.getIntegerValueField(.keyboardEventKeycode), Int64(kVK_Delete))
            XCTAssertEqual(event.flags, .maskCommand)
            var length = 0
            var actual = UniChar(0)
            event.keyboardGetUnicodeString(maxStringLength: 1, actualStringLength: &length, unicodeString: &actual)
            XCTAssertEqual(length, 1)
            XCTAssertEqual(actual, UniChar(NSDeleteCharacter))
        }
    }

    func testBackspaceModifiedForwardDeleteAndRepeatsNeverInspectSelection() throws {
        for (key, flags, repeated): (Int, CGEventFlags, Int64) in [
            (kVK_Delete, [], 0), (kVK_ForwardDelete, .maskShift, 0), (kVK_ForwardDelete, [], 1)
        ] {
            let event = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(key), keyDown: true))
            event.flags = flags
            event.setIntegerValueField(.keyboardEventAutorepeat, value: repeated)
            XCTAssertFalse(DesktopTrashShortcut.rewrite(event) { XCTFail("Unexpected selection check"); return true })
            XCTAssertEqual(event.getIntegerValueField(.keyboardEventKeycode), Int64(key))
            XCTAssertEqual(event.flags, flags)
        }
        let forward = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(kVK_ForwardDelete), keyDown: true))
        forward.flags = []
        XCTAssertFalse(DesktopTrashShortcut.rewrite(forward) { false })
        XCTAssertEqual(forward.getIntegerValueField(.keyboardEventKeycode), Int64(kVK_ForwardDelete))
        XCTAssertTrue(forward.flags.isEmpty)
    }
}
