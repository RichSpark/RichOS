// glance-scroll-fixture.swift — the source app and the input helper for glance-scroll-walk.sh. It is
// built on the host into "Scroll Fixture.app" (app.richos.test.capturepattern) and runs in the guest.
//
//   CaptureProbe sticky                 show the sticky page (below) in a WebKit window
//   CaptureProbe front | app-pid ID     the frontmost app's bundle ID | an app's process ID (0 when none)
//   CaptureProbe move|down|up X Y       a mouse event at a display point
//   CaptureProbe scroll X Y DY DX       one pixel scroll-wheel event at a display point
//   CaptureProbe press TITLE            press a control of the frontmost app by its accessible name
//   CaptureProbe selection              RichGlance's region selection value ("Crop 576 × 362 px")
//   CaptureProbe glance-has|glance-press TITLE    find | press a RichGlance control by its name
//   CaptureProbe clipboard-size | clipboard-write PATH    the PNG on the pasteboard
import AppKit
import ApplicationServices
import ImageIO
import WebKit

func require(_ ok: Bool, _ reason: String) {
    if !ok { FileHandle.standardError.write(Data((reason + "\n").utf8)); exit(1) }
}

func attr(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
    var value: CFTypeRef?
    return AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success ? value : nil
}

func cast(_ value: CFTypeRef?) -> AXUIElement? {
    guard let value, CFGetTypeID(value) == AXUIElementGetTypeID() else { return nil }
    return (value as! AXUIElement)
}

func find(_ root: AXUIElement, _ name: String) -> AXUIElement? {
    var queue = [root]
    var seen = Set<CFHashCode>()
    while !queue.isEmpty, seen.count < 2000 {
        let node = queue.removeFirst()
        guard seen.insert(CFHash(node)).inserted else { continue }
        if ["AXTitle", "AXDescription", "AXValue"].contains(where: { (attr(node, $0) as? String) == name }) { return node }
        queue += attr(node, "AXChildren") as? [AXUIElement] ?? []
    }
    return nil
}

/// A real web page: a translucent sticky site header (80% opaque) over 1820 px of text, a red end
/// marker, and a bar fixed to the bottom of the view. Header and bar colors appear nowhere else, so
/// the saved PNG can be checked for each (glance-scroll-check.swift).
let stickyPage = """
<!doctype html><html><head><meta charset="utf-8"><style>
html,body{margin:0;background:#fff;font:15px/22px Helvetica,Arial,sans-serif;color:#1a1a1a}
header{position:sticky;top:0;height:56px;background:rgba(27,58,107,0.8);color:#fff;display:flex;align-items:center;gap:28px;padding:0 18px;font-weight:bold}
main{padding:8px 22px}
p{margin:0 0 12px}
#end{height:50px;background:#b0302a;color:#fff;font-weight:bold;display:flex;align-items:center;padding-left:18px}
#spacer{height:64px}
footer{position:fixed;left:0;right:0;bottom:0;height:48px;background:#2e7d4f;color:#fff;display:flex;align-items:center;gap:24px;padding:0 18px;font-weight:bold}
</style></head><body>
<header><span>Home</span><span>Products</span><span>Pricing</span><span>Docs</span></header>
<main id="text"></main><div id="end">END OF PAGE</div><div id="spacer"></div>
<footer><span>Cookie settings</span><span>Accept</span></footer>
<script>
const words="the quarterly report shows revenue growth across regions and teams with margin notes for each customer segment plus forecast".split(" ");
let seed=7;const main=document.getElementById("text");
while(main.offsetHeight<1820){let s="";for(let i=0;i<40;i++){seed=(seed*1103515245+12345)%2147483648;s+=words[seed%words.length]+" ";}
const p=document.createElement("p");p.textContent=(main.children.length+1)+". "+s;main.appendChild(p);}
</script></body></html>
"""

let args = Array(CommandLine.arguments.dropFirst())
switch args.first {
case "sticky":
    let app = NSApplication.shared
    app.setActivationPolicy(.regular)
    // On the guest's 1680 × 1050 display the page spans y = 150...528, so the selection the walk
    // drags (y = 162...524) starts inside the sticky header and ends inside the bottom bar.
    let window = NSWindow(contentRect: CGRect(x: 180, y: 522, width: 620, height: 378),
                          styleMask: [.titled], backing: .buffered, defer: false)
    window.title = "Sticky Header Fixture"
    let web = WKWebView(frame: CGRect(x: 0, y: 0, width: 620, height: 378))
    web.loadHTMLString(stickyPage, baseURL: nil)
    window.contentView = web
    window.makeKeyAndOrderFront(nil)
    if #available(macOS 14.0, *) { app.activate() } else { app.activate(ignoringOtherApps: true) }
    withExtendedLifetime(window) { app.run() }
case "scroll":
    let point = CGPoint(x: Double(args[1])!, y: Double(args[2])!)
    let event = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2,
                        wheel1: Int32(args[3])!, wheel2: Int32(args[4])!, wheel3: 0)!
    CGWarpMouseCursorPosition(point)
    event.location = point
    event.post(tap: .cghidEventTap)
case "glance-press", "glance-has":
    guard let app = NSRunningApplication.runningApplications(withBundleIdentifier: "app.richos.richglance").first else { exit(2) }
    let element = find(AXUIElementCreateApplication(app.processIdentifier), args[1])
    if args[0] == "glance-has" { print(element == nil ? "no" : "yes") }
    else { require(element.map { AXUIElementPerformAction($0, kAXPressAction as CFString) } == .success, "control unavailable: \(args[1])") }
case "clipboard-size":
    guard let data = NSPasteboard.general.data(forType: .png),
          let source = CGImageSourceCreateWithData(data as CFData, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { exit(2) }
    print("\(image.width) × \(image.height) px")
case "clipboard-write":
    guard let data = NSPasteboard.general.data(forType: .png) else { exit(2) }
    try data.write(to: URL(fileURLWithPath: args[1]))
case "app-pid":
    print(NSRunningApplication.runningApplications(withBundleIdentifier: args[1]).first?.processIdentifier ?? 0)
case "front":
    print(NSWorkspace.shared.frontmostApplication?.bundleIdentifier ?? "none")
case "move", "down", "up":
    let point = CGPoint(x: Double(args[1])!, y: Double(args[2])!)
    let type: CGEventType = args[0] == "move" ? .mouseMoved : args[0] == "down" ? .leftMouseDown : .leftMouseUp
    if type == .mouseMoved { CGWarpMouseCursorPosition(point) }
    CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
case "press":
    let app = AXUIElementCreateApplication(NSWorkspace.shared.frontmostApplication!.processIdentifier)
    require(find(app, args[1]).map { AXUIElementPerformAction($0, kAXPressAction as CFString) } == .success, "action unavailable")
case "selection":
    let app = AXUIElementCreateApplication(NSWorkspace.shared.frontmostApplication!.processIdentifier)
    guard let window = cast(attr(app, "AXFocusedWindow")), let selector = find(window, "Region selection") else { exit(2) }
    print(attr(selector, "AXValue") as? String ?? "")
default:
    exit(2)
}
