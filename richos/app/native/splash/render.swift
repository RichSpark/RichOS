// Render the existing WebKit splash at deterministic 60 Hz sample times.
import AppKit
import WebKit

@MainActor
final class Exporter: NSObject, WKNavigationDelegate {
    let page: URL
    let destination: URL
    let view: WKWebView
    let window: NSWindow
    let width = 2800
    let height = 1704
    let id: String
    let version: String
    var count = 0
    var frame = 0

    init(page: URL, destination: URL, id: String, version: String) {
        self.page = page
        self.destination = destination
        self.id = id
        self.version = version
        let rect = NSRect(x: 0, y: 0, width: 1400, height: 852)
        view = WKWebView(frame: rect)
        window = NSWindow(contentRect: rect, styleMask: .borderless, backing: .buffered, defer: false)
        super.init()
        window.isReleasedWhenClosed = false
        window.contentView = view
        view.navigationDelegate = self
    }

    func load() {
        let args = try! JSONSerialization.data(withJSONObject: [id, version])
        let json = String(decoding: args, as: UTF8.self)
        let source = """
        const [id, version] = \(json);
        window.__RICHOS_LAUNCH__ = { kind: "fresh", splash: { id: "opening/" + id }, draw: true, exportScale: 2 };
        window.__RICHOS_OPENINGS__ = { version, sound: false };
        """
        let scripts = view.configuration.userContentController
        scripts.removeAllUserScripts()
        scripts.addUserScript(WKUserScript(source: source, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        try! FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)
        view.loadFileURL(page, allowingReadAccessTo: page.deletingLastPathComponent())
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        view.callAsyncJavaScript("await document.fonts.ready; return [window.RichSplash.state.variationId, window.RichSplash.state.seconds];",
                                 arguments: [:], in: nil, in: .page) { result in
            guard case let .success(values as [Any]) = result,
                  values.first as? String == "opening/" + self.id,
                  let seconds = values.last as? Int, (seconds == 3 || seconds == 5) else {
                fatalError("Wrong splash or duration: \(result)")
            }
            self.count = seconds * 60
            self.capture()
        }
    }

    func capture() {
        let milliseconds = Double(frame) * 1000 / 60
        view.evaluateJavaScript("window.RichSplash.holdAt(\(milliseconds))") { result, error in
            guard error == nil, result as? Bool == true else { fatalError("holdAt failed: \(String(describing: error))") }
            let config = WKSnapshotConfiguration()
            config.rect = self.view.bounds
            config.snapshotWidth = NSNumber(value: self.width)
            config.afterScreenUpdates = true
            self.view.takeSnapshot(with: config) { image, error in
                guard let image, let tiff = image.tiffRepresentation,
                      let bitmap = NSBitmapImageRep(data: tiff),
                      bitmap.pixelsWide == self.width, bitmap.pixelsHigh == self.height,
                      let png = bitmap.representation(using: .png, properties: [:]) else {
                    fatalError("Snapshot failed: \(String(describing: error))")
                }
                let file = self.destination.appendingPathComponent(String(format: "%04d.png", self.frame))
                try! png.write(to: file)
                self.frame += 1
                if self.frame % 60 == 0 { print("\(self.id): \(self.frame)"); fflush(stdout) }
                if self.frame < self.count {
                    self.capture()
                } else {
                    self.finish()
                }
            }
        }
    }

    func finish() {
        view.callAsyncJavaScript("""
                                 const bar = document.querySelector('#splash .splash-bar').getBoundingClientRect();
                                 return [bar.left, bar.top, bar.width, bar.height];
                                 """,
                                 arguments: [:], in: nil, in: .page) { result in
            guard case let .success(bar as [Double]) = result, bar.count == 4, bar[2] > 0, bar[3] > 0 else {
                fatalError("The last splash frame has no gold bar: \(result)")
            }
            let data = try! JSONSerialization.data(withJSONObject: bar)
            try! data.write(to: self.destination.appendingPathComponent("bar.json"), options: .atomic)
            NSApp.terminate(nil)
        }
    }
}

@main
struct Main {
    @MainActor
    static func main() {
        precondition(CommandLine.arguments.count == 5, "usage: render splash-opening.html frames-directory opening-id app-version")
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let exporter = Exporter(page: URL(fileURLWithPath: CommandLine.arguments[1]),
                                destination: URL(fileURLWithPath: CommandLine.arguments[2]),
                                id: CommandLine.arguments[3], version: CommandLine.arguments[4])
        DispatchQueue.main.async { exporter.load() }
        app.run()
    }
}
