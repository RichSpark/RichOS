import AppKit
import ApplicationServices

extension AXUIElement {
    var processID: pid_t? {
        var pid: pid_t = 0
        return AXUIElementGetPid(self, &pid) == .success ? pid : nil
    }

    func value<T>(_ attribute: String) -> T? {
        var result: CFTypeRef?
        guard AXUIElementCopyAttributeValue(self, attribute as CFString, &result) == .success else { return nil }
        return result as? T
    }

    func element(_ attribute: String) -> AXUIElement? {
        guard let result: CFTypeRef = value(attribute), CFGetTypeID(result) == AXUIElementGetTypeID() else { return nil }
        return (result as! AXUIElement)
    }

    func settable(_ attribute: String) -> Bool {
        var result = DarwinBoolean(false)
        return AXUIElementIsAttributeSettable(self, attribute as CFString, &result) == .success && result.boolValue
    }

    @discardableResult func set(_ attribute: String, _ value: CFTypeRef) -> Bool {
        AXUIElementSetAttributeValue(self, attribute as CFString, value) == .success
    }

    func perform(_ action: String) -> Bool {
        AXUIElementPerformAction(self, action as CFString) == .success
    }

    var role: String? { value(kAXRoleAttribute) }
    var subrole: String? { value(kAXSubroleAttribute) }

    var frame: CGRect? {
        guard let position: CFTypeRef = value(kAXPositionAttribute),
              let dimensions: CFTypeRef = value(kAXSizeAttribute),
              CFGetTypeID(position) == AXValueGetTypeID(), CFGetTypeID(dimensions) == AXValueGetTypeID() else { return nil }
        var origin = CGPoint.zero
        var size = CGSize.zero
        guard AXValueGetValue(position as! AXValue, .cgPoint, &origin),
              AXValueGetValue(dimensions as! AXValue, .cgSize, &size),
              origin.x.isFinite, origin.y.isFinite, size.width.isFinite, size.height.isFinite,
              size.width > 0, size.height > 0 else { return nil }
        return CGRect(origin: origin, size: size)
    }

    var minimized: Bool? { value(kAXMinimizedAttribute) }

    var ordinaryWindow: Bool {
        guard role == kAXWindowRole, subrole == kAXStandardWindowSubrole,
              value("AXFullScreen") as Bool? == false, value(kAXModalAttribute) as Bool? != true,
              let children: [AXUIElement] = value(kAXChildrenAttribute) else { return false }
        return !children.contains { $0.role == kAXSheetRole || $0.subrole == kAXDialogSubrole }
    }

    func setFrame(_ frame: CGRect) -> Bool {
        var origin = frame.origin
        var size = frame.size
        guard let position = AXValueCreate(.cgPoint, &origin), let dimensions = AXValueCreate(.cgSize, &size) else { return false }
        // A move can be clamped while the window still has the old size.
        _ = set(kAXPositionAttribute, position)
        let resized = set(kAXSizeAttribute, dimensions)
        let positioned = set(kAXPositionAttribute, position)
        return resized && positioned
    }
}

struct GestureDisplay {
    let id: CGDirectDisplayID
    let frame: CGRect
    let usable: CGRect

    static func current() -> [Self] {
        let screens = NSScreen.screens
        guard let primaryHeight = screens.first?.frame.maxY else { return [] }
        return screens.compactMap { screen in
            guard let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber else { return nil }
            func global(_ frame: CGRect) -> CGRect {
                CGRect(x: frame.minX, y: primaryHeight - frame.maxY, width: frame.width, height: frame.height)
            }
            return Self(id: number.uint32Value, frame: global(screen.frame), usable: global(screen.visibleFrame))
        }
    }

    func canReachTitleBar(of window: CGRect) -> Bool {
        // Accessibility frames use a top-left origin across displays.
        let grabHeight = min(22, window.height)
        let titleBar = CGRect(x: window.minX, y: window.minY, width: window.width, height: grabHeight)
        let visible = usable.intersection(titleBar)
        return visible.width >= min(80, titleBar.width) && visible.height >= grabHeight
    }

    func reachable(_ original: CGRect) -> CGRect {
        let width = min(original.width, usable.width)
        let height = min(original.height, usable.height)
        return CGRect(x: min(max(original.minX, usable.minX), usable.maxX - width),
                      y: min(max(original.minY, usable.minY), usable.maxY - height),
                      width: width, height: height)
    }
}
