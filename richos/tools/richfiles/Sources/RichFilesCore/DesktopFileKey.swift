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
        let pressed = event.flags.intersection(modifiers)
        let repeated = event.getIntegerValueField(.keyboardEventAutorepeat) != 0
        // Finder uses the layout's Command mapping, which can differ from
        // the character produced with only Shift held.
        if pressed == [.maskCommand, .maskShift], !repeated,
           NSEvent(cgEvent: event)?.characters(byApplyingModifiers: [.command, .shift])?.lowercased() == "n" {
            return .newFolder
        }
        guard pressed.isEmpty else { return .passThrough }
        switch Int(event.getIntegerValueField(.keyboardEventKeycode)) {
        case kVK_Delete:
            return event.flags.contains(.maskSecondaryFn) ? .passThrough : .backspace
        case kVK_ForwardDelete:
            return repeated ? .passThrough : .forwardDelete
        default:
            return .passThrough
        }
    }
}
