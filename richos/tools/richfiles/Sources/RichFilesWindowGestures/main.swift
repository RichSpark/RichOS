import AppKit
import ApplicationServices

enum GesturePermission: Int32 {
    case granted = 0, accessibility = 10, inputMonitoring = 11

    static var current: Self {
        guard AXIsProcessTrusted() else { return .accessibility }
        return CGPreflightListenEventAccess() ? .granted : .inputMonitoring
    }
}

if CommandLine.arguments.dropFirst().first == "--check-gesture-permission" {
    exit(GesturePermission.current.rawValue)
}

let application = NSApplication.shared
application.setActivationPolicy(.accessory)
let controller = GestureController()
controller.refresh()
let timer = Timer(timeInterval: 1, repeats: true) { _ in controller.refresh() }
RunLoop.main.add(timer, forMode: .common)
application.run()
