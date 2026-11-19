// mac-webview.swift — a page run in THE MAC'S OWN WEBVIEW (WKWebView, the one the app's window
// is), for a check that Playwright's WebKit cannot make: Playwright ships its own build of WebKit,
// and the Mac's own differs from it where the video recorder is concerned (`video-whole.js`).
// Compiled and run by `lib/mac-webview.js`.
//
//   mac-webview <page.html> <init.js> <out dir> <seconds allowed>
//
// `init.js` runs in the page before anything of the page's own. It talks to this program with
// `window.webkit.messageHandlers.suite.postMessage(JSON.stringify(...))`:
//   { say: ... }                      printed as one line of JSON
//   { file: { name, base64 } }        written into <out dir>
//   { done: ... }                     printed, and the program ends with exit 0
//
// NOTHING OF IT IS ON THE SCREEN, IT NEVER TAKES THE KEYBOARD, AND IT PLAYS NO SOUND.
// - Its window stands far outside every display (`screen` is nil), so a page in it paints at
//   full rate and nobody sees it.
// - Its activation policy is `.prohibited`: such an app has no Dock icon and cannot be activated.
//   On 2026-11-17 an earlier version with the `.accessory` policy became the front app each time
//   it started and took the keyboard from the terminal the CEO was typing in; with `.prohibited`
//   the front app stayed Terminal through a run (read six times with `lsappinfo front`). Should it
//   ever become active all the same, it ends at once with exit 4.
// - Sound is the page's business: the recorder's made-up microphone goes into the file only.
// - Its site data is not kept (a store that is gone when the program ends).
import AppKit
import WebKit

setvbuf(stdout, nil, _IOLBF, 0)
let args = CommandLine.arguments
guard args.count == 5, let allowed = Double(args[4]), allowed.isFinite, allowed > 0 else {
    FileHandle.standardError.write("usage: mac-webview <page.html> <init.js> <out dir> <seconds allowed>\n".data(using: .utf8)!)
    exit(2)
}
// The parent also watches the build and pipes. Here, include native setup
// and use an independent queue in case AppKit or WebKit blocks the main one.
DispatchQueue.global(qos: .userInitiated).asyncAfter(deadline: .now() + allowed) {
    _exit(3)
}
let page = URL(fileURLWithPath: args[1])
let outDir = URL(fileURLWithPath: args[3], isDirectory: true)
guard let initScript = try? String(contentsOfFile: args[2], encoding: .utf8) else {
    FileHandle.standardError.write("mac-webview: cannot read \(args[2])\n".data(using: .utf8)!)
    exit(2)
}

func line(_ object: Any) {
    if let data = try? JSONSerialization.data(withJSONObject: object), let text = String(data: data, encoding: .utf8) { print(text) }
}
func leaveIfActive() {
    if NSApplication.shared.isActive {
        line(["became_active": true])
        exit(4)
    }
}

final class Suite: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        guard let text = message.body as? String, let data = text.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        if let file = object["file"] as? [String: Any], let name = file["name"] as? String,
           let base64 = file["base64"] as? String, let bytes = Data(base64Encoded: base64) {
            let safe = URL(fileURLWithPath: name).lastPathComponent
            do {
                try bytes.write(to: outDir.appendingPathComponent(safe))
                line(["wrote": safe, "bytes": bytes.count])
            } catch {
                line(["could_not_write": safe])
            }
            return
        }
        line(object)
        if object["done"] != nil { exit(0) }
    }
    func webView(_ view: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        line(["load_failed": "\(error)"])
        exit(5)
    }
    func webView(_ view: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        line(["load_failed": "\(error)"])
        exit(5)
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
NotificationCenter.default.addObserver(forName: NSApplication.didBecomeActiveNotification, object: nil, queue: nil) { _ in
    line(["became_active": true])
    exit(4)
}
leaveIfActive()

let configuration = WKWebViewConfiguration()
configuration.websiteDataStore = .nonPersistent()
// the recorder's made-up microphone is a tone made in the page; it starts with no press of a real mouse
configuration.mediaTypesRequiringUserActionForPlayback = []
let suite = Suite()
configuration.userContentController.add(suite, name: "suite")
configuration.userContentController.addUserScript(WKUserScript(source: initScript, injectionTime: .atDocumentStart, forMainFrameOnly: true))
let frame = NSRect(x: 0, y: 0, width: 1440, height: 900)
let view = WKWebView(frame: frame, configuration: configuration)
view.navigationDelegate = suite
let window = NSWindow(contentRect: frame, styleMask: [.borderless], backing: .buffered, defer: true)
window.contentView = view
window.ignoresMouseEvents = true
window.setFrameOrigin(NSPoint(x: -30000, y: -30000))
// The window is on no display, which the webview would take for "covered" and slow its page down.
let occlusion = NSSelectorFromString("_setWindowOcclusionDetectionEnabled:")
if view.responds(to: occlusion) {
    typealias Setter = @convention(c) (AnyObject, Selector, Bool) -> Void
    unsafeBitCast(view.method(for: occlusion), to: Setter.self)(view, occlusion, false)
}
window.orderBack(nil)
leaveIfActive()
line(["window_on_a_display": window.screen != nil, "active": app.isActive])
if window.screen != nil {
    // it must never be seen: if the Mac put it on a display after all, nothing is run
    exit(6)
}
view.loadFileURL(page, allowingReadAccessTo: page.deletingLastPathComponent())
app.run()
