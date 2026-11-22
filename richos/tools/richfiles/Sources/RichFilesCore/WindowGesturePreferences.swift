import Foundation

public enum WindowGesturePreferences {
    public static let helperID = "app.richos.richfiles.window-gestures"
    public static let helperPath = "Contents/Library/LoginItems/RichFiles Window Gestures.app"
    public static let command = "--window-gesture-service"

    private static let defaults = UserDefaults(suiteName: "app.richos.richfiles") ?? .standard
    private static let selectionKey = "windowGestures.selection"
    private static let desktopDeleteKey = "desktopDelete.enabled"
    private static let statusKey = "windowGestures.status"
    private static let processKey = "windowGestures.process"

    public struct Selection: OptionSet {
        public let rawValue: Int
        public init(rawValue: Int) { self.rawValue = rawValue }
        public static let titleBar = Self(rawValue: 1)
        public static let dock = Self(rawValue: 2)
        public static let desktopDelete = Self(rawValue: 4)
        public static let tiling = Self(rawValue: 8)
    }

    public enum Status: String {
        case off, running, accessibility, inputMonitoring, inputUnavailable
    }

    public static var selection: Selection {
        let gestures = Selection(rawValue: defaults.integer(forKey: selectionKey)).subtracting(.desktopDelete)
        return defaults.object(forKey: desktopDeleteKey) as? Bool == false ? gestures : gestures.union(.desktopDelete)
    }

    public static func setEnabled(_ enabled: Bool, for gesture: Selection) {
        if gesture == .desktopDelete {
            defaults.set(enabled, forKey: desktopDeleteKey)
            return
        }
        let gestures = selection.subtracting(.desktopDelete)
        defaults.set((enabled ? gestures.union(gesture) : gestures.subtracting(gesture)).rawValue, forKey: selectionKey)
    }

    public static func status(process: pid_t) -> Status? {
        guard process > 0, defaults.integer(forKey: processKey) == Int(process) else { return nil }
        return defaults.string(forKey: statusKey).flatMap(Status.init(rawValue:))
    }

    public static func publish(_ status: Status) {
        defaults.set(status.rawValue, forKey: statusKey)
        defaults.set(Int(ProcessInfo.processInfo.processIdentifier), forKey: processKey)
    }

    public static var nativeTitleBarIsOff: Bool {
        // Missing and unknown values can mean the OS's own default action.
        let action = UserDefaults.standard.persistentDomain(forName: UserDefaults.globalDomain)?["AppleActionOnDoubleClick"] as? String
        return action?.caseInsensitiveCompare("None") == .orderedSame
    }
}
