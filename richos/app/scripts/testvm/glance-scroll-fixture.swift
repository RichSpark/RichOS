// glance-scroll-fixture.swift — the source app and the input helper for glance-scroll-walk.sh. It is
// built on the host into "Scroll Fixture.app" (app.richos.test.capturepattern) and runs in the guest.
//
//   CaptureProbe sticky [inset]         show the sticky page (below) in a WebKit window; with `inset`,
//                                       its bottom bar stops 70 px short of each side of the page
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

/// A real web page: a translucent sticky site header (80% opaque) over 1820 px of lines of dark
/// blocks the size of words, a red end block, and a white bar fixed to the bottom of the view: dark
/// labels and one green square between blank margins, as an ordinary cookie notice. Every block sits
/// on whole pixels at a place glance-scroll-check.swift works out again from the same numbers, so the
/// saved PNG can be checked pixel by pixel. Keep the two in step. With `inset` the bar stops 70 px
/// short of each side, and the page scrolls past beside it.
func stickyPage(inset: Bool) -> String { """
<!doctype html><html><head><meta charset="utf-8"><style>
html,body{margin:0;background:#fff;font:15px/22px Helvetica,Arial,sans-serif;color:#1a1a1a}
header{position:sticky;top:0;z-index:2;height:56px;background:rgba(27,58,107,0.8);color:#fff;display:flex;align-items:center;gap:28px;padding:0 18px;font-weight:bold}
main{position:relative;height:1820px}
main i{position:absolute;height:12px;background:#1a1a1a}
#end{height:50px;background:#b0302a}
#spacer{height:64px}
footer{position:fixed;left:\(inset ? 70 : 0)px;right:\(inset ? 70 : 0)px;bottom:0;z-index:2;height:48px;background:#fff;display:flex;align-items:center;gap:24px;padding:0 18px 0 52px}
footer b{position:absolute;left:30px;top:18px;width:12px;height:12px;background:#2e7d4f}
</style></head><body>
<header><span>Home</span><span>Products</span><span>Pricing</span><span>Docs</span></header>
<main id="text"></main><div id="end"></div><div id="spacer"></div>
<footer><b></b><span>Cookie settings</span><span>Accept</span></footer>
<script>
let s=7;const r=n=>{s=s*48271%2147483647;return s%n};
const main=document.getElementById("text");let y=10;
for(let k=0;y+12<=1810;k++){const last=k%6==5;const end=last?150+r(300):572;let x=22;
for(;;){const w=18+r(70);if(x+w>end)break;const i=document.createElement("i");i.style.cssText="left:"+x+"px;top:"+y+"px;width:"+w+"px";main.appendChild(i);x+=w+8;}
y+=last?34:22;}
</script></body></html>
""" }

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
    web.loadHTMLString(stickyPage(inset: args.dropFirst().first == "inset"), baseURL: nil)
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
