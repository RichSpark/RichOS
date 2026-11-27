import AppKit
import Carbon.HIToolbox

public enum DesktopFileKey {
    case passThrough
    case newFolder
    case backspace
    case forwardDelete

    public static func classify(_ event: CGEvent) -> Self {
        let modifiers: CGEventFlags = [.maskCommand, .maskAlternate, .maskControl, .maskShift]
        guard event.type == .keyDown else { return .passThrough }
        let key = Int(event.getIntegerValueField(.keyboardEventKeycode))
        let pressed = event.flags.intersection(modifiers)
        let repeated = event.getIntegerValueField(.keyboardEventAutorepeat) != 0
        if key == kVK_ANSI_N, pressed == [.maskCommand, .maskShift], !repeated { return .newFolder }
        guard pressed.isEmpty else { return .passThrough }
        switch key {
        case kVK_Delete:
            return event.flags.contains(.maskSecondaryFn) ? .passThrough : .backspace
        case kVK_ForwardDelete:
            return repeated ? .passThrough : .forwardDelete
        default:
            return .passThrough
        }
    }
}
