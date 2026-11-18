import AppKit

let application = NSApplication.shared
application.setActivationPolicy(.prohibited)
let controller = GestureController()
controller.refresh()
let timer = Timer(timeInterval: 1, repeats: true) { _ in controller.refresh() }
RunLoop.main.add(timer, forMode: .common)
application.run()
