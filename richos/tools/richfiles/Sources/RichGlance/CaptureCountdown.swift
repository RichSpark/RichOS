import AppKit

@MainActor
final class CaptureCountdown: NSObject {
    let window: NSPanel
    var cancel: (() -> Void)?
    private let label = NSTextField(labelWithString: "")
    private let number = NSTextField(labelWithString: "")

    init(display: CGRect) {
        let frame = CGRect(x: display.midX - 238, y: display.midY - 53, width: 476, height: 106)
        window = NSPanel(contentRect: frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        super.init()
        window.isReleasedWhenClosed = false
        window.hidesOnDeactivate = false
        window.level = .floating
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        window.setAccessibilityTitle("Capture countdown")
        let details = NSTextField(wrappingLabelWithString: "Set up the screen now. This panel stays out of the picture.")
        label.font = .systemFont(ofSize: 18, weight: .semibold)
        details.font = .systemFont(ofSize: 16)
        number.font = .monospacedDigitSystemFont(ofSize: 28, weight: .bold)
        number.alignment = .center
        number.widthAnchor.constraint(equalToConstant: 56).isActive = true
        let copy = NSStackView(views: [label, details])
        copy.orientation = .vertical
        copy.alignment = .leading
        copy.spacing = 4
        let button = NSButton(title: "Cancel", target: self, action: #selector(cancelPressed))
        button.font = .systemFont(ofSize: 16)
        button.bezelStyle = .rounded
        button.keyEquivalent = "\u{1b}"
        let root = NSStackView(views: [number, copy, button])
        root.edgeInsets = NSEdgeInsets(top: 12, left: 12, bottom: 12, right: 12)
        root.spacing = 12
        root.wantsLayer = true
        let colors = ScreenshotChrome.colors(for: root)
        root.layer?.backgroundColor = colors.surface.cgColor
        root.layer?.cornerRadius = 14
        root.layer?.borderColor = colors.gold.cgColor
        root.layer?.borderWidth = 1
        number.textColor = colors.gold
        label.textColor = colors.ink
        details.textColor = colors.ink
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.contentView = root
    }

    func update(seconds: Int) {
        label.stringValue = "Capturing in \(seconds) \(seconds == 1 ? "second" : "seconds")"
        number.stringValue = String(seconds)
        NSAccessibility.post(element: label, notification: .valueChanged)
    }

    func show(seconds: Int) {
        update(seconds: seconds)
        window.orderFrontRegardless()
    }

    @objc private func cancelPressed() { cancel?() }
}
