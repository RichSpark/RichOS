import AppKit

@MainActor
final class ScreenshotPin: NSPanel {
    let id = UUID()
    let captureID: UUID?
    private let picture = NSImageView()
    private let opacity = NSSlider(value: 1, minValue: 0.1, maxValue: 1, target: nil, action: nil)
    private let levelLabel = screenshotLabel("100%", weight: .semibold)
    private let actualSize = ScreenshotButton("Actual Size")
    private let through = ScreenshotButton("Click Through")
    private let closeButton = ScreenshotButton("Close", hint: "esc")
    private let layout: PinSurface
    private let pixelSize: CGSize
    private var origin: CGPoint?

    init(image: CGImage, anchoredAt frame: CGRect?, captureID: UUID?) {
        pixelSize = CGSize(width: image.width, height: image.height)
        self.captureID = captureID
        let point = frame.map { CGPoint(x: $0.midX, y: $0.midY) } ?? NSEvent.mouseLocation
        let screen = NSScreen.screens.first(where: { $0.frame.contains(point) }) ?? NSScreen.main
        let visible = screen?.visibleFrame ?? CGRect(x: 0, y: 0, width: 1280, height: 800)
        let scale = screen?.backingScaleFactor ?? 1
        let maxWidth = max(1, visible.width - 20)
        let maxHeight = max(1, visible.height - 56)
        let initial = frame?.size ?? CGSize(width: pixelSize.width / scale, height: pixelSize.height / scale)
        let factor = frame == nil ? min(1, maxWidth / initial.width, maxHeight / initial.height) : 1
        let art = CGSize(width: initial.width * factor, height: initial.height * factor)
        let width = max(490, art.width)
        let place = frame?.origin ?? CGPoint(x: min(point.x + 16, visible.maxX - art.width - 10),
                                             y: max(visible.minY + 10, point.y - art.height - 25))
        let content = CGRect(x: place.x, y: place.y, width: width, height: art.height + 46)
        layout = PinSurface(picture: picture)
        super.init(contentRect: content, styleMask: [.borderless, .nonactivatingPanel, .resizable],
                   backing: .buffered, defer: false)
        setFrame(content, display: false)
        layout.header = [opacity, levelLabel, actualSize, through, closeButton]
        layout.pictureWidth = art.width
        layout.windowDrag = { [weak self] event in self?.performDrag(with: event) }
        layout.resize = { [weak self] point, delta in self?.resize(from: point, by: delta) }
        layout.key = { [weak self] event in self?.handleKey(event) ?? false }
        contentView = layout
        isReleasedWhenClosed = false
        isFloatingPanel = true
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = true
        worksWhenModal = true
        acceptsMouseMovedEvents = true
        hasShadow = true
        isOpaque = false
        backgroundColor = .clear
        level = .floating
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        setAccessibilityTitle("Pinned image")
        title = "Pinned image"
        minSize = CGSize(width: 490, height: 74)
        picture.image = NSImage(cgImage: image, size: pixelSize)
        picture.imageScaling = .scaleAxesIndependently
        picture.setAccessibilityElement(true)
        picture.setAccessibilityRole(.image)
        picture.setAccessibilityLabel("Pinned image")
        opacity.target = self
        opacity.action = #selector(opacityChanged)
        opacity.setAccessibilityLabel("Pin opacity")
        actualSize.onPress = { [weak self] in self?.restoreActualSize() }
        through.onPress = { [weak self] in self?.toggleThrough() }
        closeButton.onPress = { [weak self] in self?.close() }
        layout.nextKeyView = opacity
        opacity.nextKeyView = actualSize
        actualSize.nextKeyView = through
        through.nextKeyView = closeButton
        closeButton.nextKeyView = layout
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    func showControls() {
        if ignoresMouseEvents { toggleThrough() }
        makeKeyAndOrderFront(nil)
        makeFirstResponder(layout)
    }

    @objc private func opacityChanged() {
        picture.alphaValue = opacity.doubleValue
        levelLabel.stringValue = "\(Int((opacity.doubleValue * 100).rounded()))%"
    }

    private func toggleThrough() {
        ignoresMouseEvents.toggle()
        through.isSelected = ignoresMouseEvents
        if ignoresMouseEvents { resignKey() }
    }

    private func restoreActualSize() {
        let scale = screen?.backingScaleFactor ?? 1
        adjust(to: CGSize(width: pixelSize.width / scale, height: pixelSize.height / scale))
    }

    private func adjust(to size: CGSize) {
        layout.pictureWidth = size.width
        var frame = frame
        frame.size = CGSize(width: max(490, size.width), height: size.height + 46)
        setFrame(frame, display: true)
    }

    private func resize(from point: CGPoint, by delta: CGPoint) {
        if origin != point { origin = point; layout.startWidth = layout.pictureWidth }
        let width = min(8_192, max(45, layout.startWidth + delta.x))
        adjust(to: CGSize(width: width, height: width * pixelSize.height / pixelSize.width))
    }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown, firstResponder === layout, handleKey(event) { return }
        super.sendEvent(event)
    }

    private func handleKey(_ event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection([.command, .option, .shift, .control])
        guard flags.isEmpty || flags == .shift else { return false }
        let move: CGFloat = flags == .shift ? 10 : 1
        var position = frame.origin
        switch event.keyCode {
        case 53: close(); return true
        case 123: position.x -= move
        case 124: position.x += move
        case 125: position.y -= move
        case 126: position.y += move
        default:
            switch event.charactersIgnoringModifiers?.lowercased() {
            case "t": toggleThrough()
            case "[", "]":
                let step = event.charactersIgnoringModifiers == "]" ? 0.05 : -0.05
                opacity.doubleValue = max(0.1, min(1, opacity.doubleValue + step))
                opacityChanged()
            case "-", "=":
                let ratio: CGFloat = event.charactersIgnoringModifiers == "=" ? 1.1 : 1 / 1.1
                let width = max(45, min(8_192, layout.pictureWidth * ratio))
                adjust(to: CGSize(width: width, height: width * pixelSize.height / pixelSize.width))
            default: return false
            }
            return true
        }
        setFrameOrigin(position)
        return true
    }
}

@MainActor
private final class PinSurface: NSView {
    let picture: NSImageView
    private let bar = ScreenshotSurface(.floating, radius: 9, outline: true)
    private let caption = screenshotLabel("Opacity")
    private let grip = NSView()
    var header: [NSView] = [] { didSet { header.forEach(bar.addSubview) } }
    var pictureWidth: CGFloat = 1 { didSet { needsLayout = true } }
    var startWidth: CGFloat = 1
    var windowDrag: ((NSEvent) -> Void)?
    var resize: ((CGPoint, CGPoint) -> Void)?
    var key: ((NSEvent) -> Bool)?
    private var anchor: CGPoint?

    init(picture: NSImageView) {
        self.picture = picture
        super.init(frame: .zero)
        addSubview(picture)
        addSubview(grip)
        grip.wantsLayer = true
        grip.layer?.backgroundColor = NSColor.white.cgColor
        grip.layer?.borderColor = NSColor.black.cgColor
        grip.layer?.borderWidth = 1
        addSubview(bar)
        bar.addSubview(caption)
        setAccessibilityLabel("Pinned image controls")
    }

    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }
    override var needsPanelToBecomeKey: Bool { true }
    override var mouseDownCanMoveWindow: Bool { false }

    override func layout() {
        super.layout()
        bar.frame = CGRect(x: 0, y: 0, width: 490, height: 46)
        caption.textColor = ScreenshotEditorPalette(self).ink
        caption.frame = CGRect(x: 10, y: 12, width: 62, height: 22)
        picture.frame = CGRect(x: 0, y: 46, width: pictureWidth, height: bounds.height - 46)
        grip.frame = CGRect(x: max(0, pictureWidth - 18), y: bounds.height - 18, width: 18, height: 18)
        let frames = [(75.0, 83.0), (163.0, 49.0), (217.0, 93.0), (316.0, 110.0), (432.0, 52.0)]
        for (view, frame) in zip(header, frames) { view.frame = CGRect(x: frame.0, y: 7, width: frame.1, height: 32) }
        (header[safe: 1] as? NSTextField)?.textColor = ScreenshotEditorPalette(self).ink
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        let local = convert(point, from: superview)
        if local.y >= 46 { return picture.frame.contains(local) ? self : nil }
        return super.hitTest(point)
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeKey()
        window?.makeFirstResponder(self)
        let local = convert(event.locationInWindow, from: nil)
        if local.x >= pictureWidth - 18, local.y >= bounds.height - 18 {
            anchor = NSEvent.mouseLocation
        } else { windowDrag?(event) }
    }

    override func mouseDragged(with event: NSEvent) {
        guard let anchor else { return }
        let point = NSEvent.mouseLocation
        resize?(anchor, CGPoint(x: point.x - anchor.x, y: point.y - anchor.y))
    }

    override func mouseUp(with event: NSEvent) { anchor = nil }
    override func keyDown(with event: NSEvent) { if key?(event) != true { super.keyDown(with: event) } }
}

private extension Array {
    subscript(safe index: Int) -> Element? { indices.contains(index) ? self[index] : nil }
}
