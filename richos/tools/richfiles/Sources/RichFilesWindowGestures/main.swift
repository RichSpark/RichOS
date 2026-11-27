import AppKit
import ApplicationServices
import RichFilesCore

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

// Older helpers can exec this binary before RichOS sends its service refresh.
// Only a process launched by launchd can replace the old restart contract.
if WindowGesturePreferences.didReplaceRunningHelper {
    WindowGesturePreferences.publish(.inputUnavailable)
    if !WindowGesturePreferences.restartHelper() {
        NSLog("RichFiles Window Gestures could not restart a replaced process")
    }
    exit(EXIT_FAILURE)
}

MainActor.assumeIsolated {
    let application = NSApplication.shared
    application.setActivationPolicy(.accessory)
    let controller = GestureController()
    controller.refresh()
    let timer = Timer(timeInterval: 1, repeats: true) { _ in controller.refresh() }
    RunLoop.main.add(timer, forMode: .common)
    let suiteRestart = SuiteRestart(operation: .gesturesSuiteRestart, restartLoginItem: true) {
        guard controller.canRestartAfterUpdate else { return nil }
        return try? JSONEncoder().encode(true)
    }
    WindowGesturePreferences.markRestartContract()
    withExtendedLifetime(suiteRestart) { application.run() }
}
