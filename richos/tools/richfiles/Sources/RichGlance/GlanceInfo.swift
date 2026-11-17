import AppKit
import RichFilesCore

struct GlanceInfoShortcut: Codable, Equatable {
    static let modifiers: NSEvent.ModifierFlags = [.control, .option, .shift, .command]
    static let initial = Self(key: "i", rawModifiers: 0)

    let key: String
    let rawModifiers: UInt

    var flags: NSEvent.ModifierFlags { NSEvent.ModifierFlags(rawValue: rawModifiers) }

    var label: String {
        let named = key.unicodeScalars.first.flatMap { scalar in
            (0xF704...0xF726).contains(scalar.value) ? "F\(scalar.value - 0xF704 + 1)" : nil
        }
        return (flags.contains(.control) ? "⌃" : "") + (flags.contains(.option) ? "⌥" : "")
            + (flags.contains(.shift) ? "⇧" : "") + (flags.contains(.command) ? "⌘" : "") + (named ?? key.uppercased())
    }

    init(key: String, rawModifiers: UInt) {
        self.key = key
        self.rawModifiers = rawModifiers
    }

    init?(event: NSEvent) {
        guard let key = event.charactersIgnoringModifiers?.precomposedStringWithCanonicalMapping, key.count == 1 else { return nil }
        self.init(key: key.lowercased(), rawModifiers: event.modifierFlags.intersection(Self.modifiers).rawValue)
    }

    var conflict: String? {
        if key == "\u{F702}" || key == "\u{F703}" { return "The arrow keys change images." }
        if key == "\t" || key == " " || key == "\r" || key == "\n" || key == "\u{0019}" {
            return "That key moves focus or activates a control."
        }
        if flags == .command && ["w", "q", "h", "m", "o"].contains(key) {
            return "\(label) is already used by RichGlance."
        }
        if key.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) {
            return "Choose a printable key or a function key."
        }
        return nil
    }
}

@MainActor
final class GlanceInfoPreferences {
    private let defaults: UserDefaults
    private(set) var shortcut: GlanceInfoShortcut

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        if let data = defaults.data(forKey: "glance.infoShortcut"),
           let saved = try? JSONDecoder().decode(GlanceInfoShortcut.self, from: data), saved.conflict == nil {
            shortcut = saved
        } else {
            shortcut = .initial
        }
    }

    var showByDefault: Bool {
        get { defaults.bool(forKey: "glance.showInfoByDefault") }
        set { defaults.set(newValue, forKey: "glance.showInfoByDefault") }
    }

    func setShortcut(_ value: GlanceInfoShortcut) {
        guard let data = try? JSONEncoder().encode(value) else { return }
        defaults.set(data, forKey: "glance.infoShortcut")
        shortcut = value
    }
}

@MainActor
final class GlanceInfoView: NSView {
    private final class FocusButton: NSButton {
        var focusChanged: (() -> Void)?
        override var acceptsFirstResponder: Bool { true }
        override func becomeFirstResponder() -> Bool {
            let accepted = super.becomeFirstResponder()
            if accepted { focusChanged?() }
            return accepted
        }
        override func resignFirstResponder() -> Bool {
            let accepted = super.resignFirstResponder()
            if accepted { focusChanged?() }
            return accepted
        }
    }

    private final class InfoScroll: NSScrollView {
        override func scrollWheel(with event: NSEvent) {
            if event.modifierFlags.contains(.control) { nextResponder?.scrollWheel(with: event) }
            else { super.scrollWheel(with: event) }
        }
    }

    private let preferences: GlanceInfoPreferences
    private let icon = FocusButton()
    private let showDefault = FocusButton()
    private let shortcutButton = FocusButton()
    private let controlsMaterial = NSVisualEffectView()
    private let note = NSTextView(frame: .zero)
    private let scroll = InfoScroll()
    private let status = NSTextField(labelWithString: "")
    private var tracking: NSTrackingArea?
    private var metadata: GlanceMetadata?
    private var loaded = false
    private var unavailable = false
    private var overrideVisibility: Bool?
    private var peek = false
    private var suppressed = false
    private var controlsHovered = false
    private var recording = false

    var announcement: ((String) -> Void)?
    var preferencesChanged: (() -> Void)?
    var explicitlyShown: Bool { overrideVisibility ?? (preferences.showByDefault && metadata != nil) }
    override var isFlipped: Bool { true }

    init(preferences: GlanceInfoPreferences) {
        self.preferences = preferences
        super.init(frame: .zero)
        isHidden = true
        icon.image = NSImage(systemSymbolName: "info", accessibilityDescription: nil)
        icon.isBordered = false
        icon.imagePosition = .imageOnly
        icon.wantsLayer = true
        icon.layer?.cornerRadius = 13
        icon.contentTintColor = .white
        icon.setAccessibilityLabel("Show RichCleaner Info")
        icon.toolTip = "Show RichCleaner Info"
        icon.target = self
        icon.action = #selector(toggleInfo)
        controlsMaterial.material = .popover
        controlsMaterial.blendingMode = .withinWindow
        controlsMaterial.state = .active
        controlsMaterial.wantsLayer = true
        controlsMaterial.layer?.cornerRadius = 10
        controlsMaterial.layer?.masksToBounds = true
        addSubview(controlsMaterial)
        configure(showDefault, action: #selector(toggleDefault))
        showDefault.setButtonType(.switch)
        configure(shortcutButton, action: #selector(beginRecording))
        for button in [icon, showDefault, shortcutButton] {
            button.focusChanged = { [weak self] in
                DispatchQueue.main.async { self?.updateVisibility() }
            }
            addSubview(button)
        }
        icon.nextKeyView = showDefault
        showDefault.nextKeyView = shortcutButton
        note.isEditable = false
        note.isSelectable = true
        note.drawsBackground = false
        note.textContainerInset = CGSize(width: 4, height: 5)
        note.isVerticallyResizable = true
        note.isHorizontallyResizable = false
        note.textContainer?.widthTracksTextView = true
        note.textContainer?.heightTracksTextView = false
        note.setAccessibilityLabel("RichCleaner metadata")
        scroll.documentView = note
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.drawsBackground = false
        scroll.borderType = .noBorder
        addSubview(scroll, positioned: .below, relativeTo: icon)
        status.maximumNumberOfLines = 2
        status.lineBreakMode = .byWordWrapping
        status.font = .systemFont(ofSize: 14, weight: .medium)
        status.textColor = .labelColor
        status.setAccessibilityLabel("Shortcut status")
        addSubview(status)
        updateVisibility()
    }

    required init?(coder: NSCoder) { nil }

    func linkKeyboard(after view: NSView) {
        view.nextKeyView = icon
        shortcutButton.nextKeyView = view
    }

    private func configure(_ button: FocusButton, action: Selector) {
        button.target = self
        button.action = action
        button.bezelStyle = .rounded
        button.font = .systemFont(ofSize: 16, weight: .medium)
    }

    func reset(showIcon: Bool) {
        isHidden = !showIcon
        metadata = nil
        loaded = false
        unavailable = false
        overrideVisibility = nil
        peek = false
        suppressed = false
        controlsHovered = false
        recording = false
        status.stringValue = ""
        updateText()
        updateVisibility()
    }

    func finishLookup(_ metadata: GlanceMetadata?, unavailable: Bool) {
        self.metadata = metadata
        self.unavailable = unavailable
        loaded = true
        updateText()
        updateVisibility()
    }

    @objc func toggleInfo() {
        let previous = explicitlyShown
        overrideVisibility = !previous
        suppressed = previous && peek
        updateVisibility()
    }

    @objc func toggleDefault() {
        preferences.showByDefault.toggle()
        overrideVisibility = nil
        suppressed = false
        updateVisibility()
        preferencesChanged?()
    }

    @objc func beginRecording() {
        recording = true
        status.stringValue = ""
        updateVisibility()
        window?.makeFirstResponder(shortcutButton)
    }

    func handleKey(_ event: NSEvent) -> Bool {
        if recording {
            if event.isARepeat { return true }
            if event.keyCode == 53 {
                endRecording()
            } else if let shortcut = GlanceInfoShortcut(event: event) {
                if let reason = shortcut.conflict {
                    status.stringValue = reason
                    updateVisibility()
                    NSAccessibility.post(element: status, notification: .valueChanged)
                } else {
                    preferences.setShortcut(shortcut)
                    endRecording()
                    preferencesChanged?()
                    announcement?("Info shortcut is now \(shortcut.label)")
                }
            } else {
                status.stringValue = "Choose a single key, with optional modifiers."
                updateVisibility()
            }
            return true
        }
        guard !isHidden, GlanceInfoShortcut(event: event) == preferences.shortcut else { return false }
        if !event.isARepeat { toggleInfo() }
        return true
    }

    private func endRecording() {
        recording = false
        status.stringValue = ""
        updateVisibility()
    }

    private func updateVisibility() {
        let focus = window?.firstResponder
        let controlsFocused = [icon, showDefault, shortcutButton].contains { focus === $0 }
        let showControls = recording || controlsHovered || controlsFocused
        showDefault.isHidden = !showControls
        controlsMaterial.isHidden = !showControls
        showDefault.title = "Show by default"
        showDefault.state = preferences.showByDefault ? .on : .off
        shortcutButton.isHidden = !showControls
        shortcutButton.title = recording ? "Press the new shortcut" : "Shortcut  \(preferences.shortcut.label)"
        status.isHidden = !recording || status.stringValue.isEmpty
        icon.layer?.backgroundColor = (explicitlyShown ? NSColor.white : NSColor.black.withAlphaComponent(0.65)).cgColor
        icon.contentTintColor = explicitlyShown ? NSColor.black : NSColor.white
        icon.setAccessibilityLabel(explicitlyShown ? "Hide RichCleaner Info" : "Show RichCleaner Info")
        scroll.isHidden = !(explicitlyShown || (peek && !suppressed))
        needsLayout = true
    }

    override func layout() {
        super.layout()
        icon.frame = CGRect(x: 12, y: 12, width: 26, height: 26)
        let width = max(1, min(bounds.width - 24, 550))
        let switchWidth = max(154, showDefault.intrinsicContentSize.width)
        let shortcutWidth = min(width, max(100, shortcutButton.intrinsicContentSize.width))
        showDefault.frame = CGRect(x: 48, y: 10, width: switchWidth, height: 30)
        shortcutButton.frame = CGRect(x: 56 + switchWidth, y: 10, width: shortcutWidth, height: 30)
        if shortcutButton.frame.maxX > bounds.width - 12 {
            shortcutButton.frame.origin = CGPoint(x: 48, y: 44)
        }
        let rowBottom = showDefault.isHidden ? icon.frame.maxY : max(shortcutButton.frame.maxY, showDefault.frame.maxY)
        var materialBounds = showDefault.frame.union(shortcutButton.frame)
        status.frame = CGRect(x: 53, y: rowBottom + 4, width: max(1, materialBounds.width - 10), height: 35)
        if !status.isHidden { materialBounds = materialBounds.union(status.frame) }
        controlsMaterial.frame = materialBounds.insetBy(dx: -5, dy: -3)
        let top = (status.isHidden ? rowBottom : status.frame.maxY) + 6
        let limit = max(1, min(bounds.height - top - 12, bounds.height * 0.62))
        scroll.frame = CGRect(x: 12, y: top, width: width, height: limit)
        let textWidth = max(1, scroll.contentSize.width)
        note.frame.size.width = textWidth
        if let layoutManager = note.layoutManager, let container = note.textContainer {
            layoutManager.ensureLayout(for: container)
            let height = ceil(layoutManager.usedRect(for: container).height) + 12
            note.frame.size.height = height
            scroll.frame.size.height = min(limit, height)
        }
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        let hit = super.hitTest(point)
        return hit === self ? nil : hit
    }

    override func updateTrackingAreas() {
        if let tracking { removeTrackingArea(tracking) }
        let area = NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .mouseMoved, .activeInKeyWindow, .inVisibleRect],
                                  owner: self, userInfo: nil)
        addTrackingArea(area)
        tracking = area
        super.updateTrackingAreas()
    }

    override func mouseEntered(with event: NSEvent) { track(event) }
    override func mouseMoved(with event: NSEvent) { track(event) }

    override func mouseExited(with event: NSEvent) {
        controlsHovered = false
        peek = false
        suppressed = false
        updateVisibility()
    }

    private func track(_ event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        let controls = [icon, showDefault, shortcutButton].filter { !$0.isHidden }.map(\.frame)
        let buttonArea = controls.reduce(icon.frame) { $0.union($1) }.insetBy(dx: -4, dy: -4)
        let inControls = buttonArea.contains(point)
        let textArea = scroll.frame.insetBy(dx: -3, dy: -8)
        let inText = (explicitlyShown || peek || suppressed) && textArea.contains(point)
        let stillPeeking = inControls || inText
        if stillPeeking == peek, inControls == controlsHovered { return }
        if !stillPeeking { suppressed = false }
        peek = stillPeeking
        controlsHovered = inControls
        updateVisibility()
    }

    private func updateText() {
        let lines = NSMutableAttributedString(string: "")
        func append(_ text: String, size: CGFloat, weight: NSFont.Weight = .medium) {
            if lines.length > 0 { lines.append(NSAttributedString(string: "\n")) }
            let shadow = NSShadow()
            shadow.shadowColor = NSColor.black
            shadow.shadowBlurRadius = 5
            shadow.shadowOffset = CGSize(width: 0, height: -1)
            let paragraph = NSMutableParagraphStyle()
            paragraph.paragraphSpacing = 6
            lines.append(NSAttributedString(string: text, attributes: [
                .font: NSFont.systemFont(ofSize: size, weight: weight), .foregroundColor: NSColor.white,
                .strokeColor: NSColor.black, .strokeWidth: -3.5, .shadow: shadow, .paragraphStyle: paragraph
            ]))
        }
        if let metadata {
            if !metadata.note.isEmpty { append(metadata.note, size: 22, weight: .semibold) }
            if !metadata.labels.isEmpty { append("Keywords  \(metadata.labels.joined(separator: " · "))", size: 16) }
            if !metadata.folder.isEmpty { append("Folder  \(metadata.folder)", size: 16) }
            append("Saved  \(metadata.updated.formatted(date: .abbreviated, time: .shortened))", size: 16)
            if metadata.embedded { append("The note and keywords are inside the file.", size: 16) }
        } else {
            append(!loaded ? "Checking RichCleaner…" : unavailable
                   ? "RichCleaner metadata is unavailable."
                   : "RichCleaner has saved nothing for this image.", size: 16)
        }
        note.textStorage?.setAttributedString(lines)
        scroll.contentView.scroll(to: .zero)
        needsLayout = true
    }
}
