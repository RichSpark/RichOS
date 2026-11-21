import AppKit

@MainActor
final class ScreenshotRecents: NSView {
    var open: ((ScreenshotStore.Entry) -> Void)?
    var remove: ((ScreenshotStore.Entry) -> Void)?
    var hide: (() -> Void)?
    var clearAll: (() -> Void)?
    var turnOff: (() -> Void)?
    var turnOn: (() -> Void)?
    var changeRetention: ((ScreenshotStore.Retention) -> Void)?
    var focusCanvas: (() -> Void)?

    private let store: ScreenshotStore
    private let background = ScreenshotSurface(.shelf)
    private let rule = ScreenshotRule()
    private let divider = ScreenshotRule()
    private let scroller = NSScrollView()
    private let row = ScreenshotSurface(.shelf)
    private let descriptionLabel = NSTextField(wrappingLabelWithString: "")
    private let retentionLabel = screenshotLabel("Keep on this Mac for")
    private let inlineRetention = ScreenshotChoice(["7 days", "Forever"], label: "Keep captures for")
    private let menuRetention = ScreenshotChoice(["7 days", "Forever"], label: "Keep captures for")
    private let clear = ScreenshotButton("Clear All…")
    private let more = ScreenshotButton("Recent Captures options", symbol: "ellipsis")
    private let enable = ScreenshotButton("Turn On", style: .primary)
    private let popover = NSPopover()
    private let menuDescription = NSTextField(wrappingLabelWithString: "")
    private var cards: [ScreenshotShelfCard] = []
    private var selected: UUID?
    private var keepsHistory = true
    private var policy = ScreenshotStore.Retention.week
    private var observer: NSObjectProtocol?

    init(store: ScreenshotStore) {
        self.store = store
        super.init(frame: .zero)
        setAccessibilityElement(false)
        setAccessibilityLabel("Recent Captures")
        addSubview(background)
        for view in [scroller, rule, divider, descriptionLabel, retentionLabel, inlineRetention, clear, more, enable] {
            background.addSubview(view)
        }
        scroller.documentView = row
        scroller.hasHorizontalScroller = true
        scroller.autohidesScrollers = true
        scroller.scrollerStyle = .overlay
        scroller.drawsBackground = false
        scroller.setAccessibilityLabel("Recent Captures, newest first")
        scroller.contentView.postsBoundsChangedNotifications = true
        descriptionLabel.font = .systemFont(ofSize: 16)
        descriptionLabel.maximumNumberOfLines = 3
        menuDescription.font = .systemFont(ofSize: 16)
        more.iconOnly = true
        more.onPress = { [weak self] in self?.showOptions() }
        clear.onPress = { [weak self] in self?.clearAll?() }
        enable.onPress = { [weak self] in self?.turnOn?() }
        inlineRetention.changed = { [weak self] in self?.changeRetention?(ScreenshotStore.Retention.allCases[$0]) }
        menuRetention.changed = inlineRetention.changed
        observer = NotificationCenter.default.addObserver(forName: NSView.boundsDidChangeNotification,
                                                          object: scroller.contentView, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.loadVisibleThumbnails() }
        }
        background.changedAppearance = { [weak self] in self?.updateWords() }
        buildOptions()
        updateWords()
    }

    required init?(coder: NSCoder) { nil }

    deinit {
        if let observer { NotificationCenter.default.removeObserver(observer) }
    }

    override var isFlipped: Bool { true }

    func update(entries: [ScreenshotStore.Entry], selected: UUID?, enabled: Bool, retention: ScreenshotStore.Retention,
                unavailable: Bool = false, newCapture: Bool = false) {
        let previousSelection = self.selected
        self.selected = selected
        keepsHistory = enabled
        policy = retention
        let existing = Dictionary(uniqueKeysWithValues: cards.map { ($0.entry.id, $0) })
        let focused = cards.firstIndex { $0.button === window?.firstResponder }
        let nextCards = entries.map { entry -> ScreenshotShelfCard in
            if let card = existing[entry.id], card.entry == entry { return card }
            let card = ScreenshotShelfCard(entry)
            card.button.onPress = { [weak self] in self?.open?(entry) }
            card.delete.onPress = { [weak self] in self?.remove?(entry) }
            row.addSubview(card)
            return card
        }
        for card in cards where !nextCards.contains(where: { $0 === card }) { card.removeFromSuperview() }
        cards = nextCards
        for card in cards { card.selected = card.entry.id == selected }
        clear.isEnabled = !entries.isEmpty
        descriptionLabel.isHidden = !entries.isEmpty
        scroller.isHidden = entries.isEmpty
        if unavailable {
            descriptionLabel.stringValue = "Could not read Recent Captures. Reopen the editor to try again."
        } else {
            updateWords()
        }
        needsLayout = true
        layoutSubtreeIfNeeded()
        if previousSelection != selected, let card = cards.first(where: { $0.entry.id == selected }) {
            row.scrollToVisible(card.frame)
            if newCapture && !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
                card.alphaValue = 0
                NSAnimationContext.runAnimationGroup { context in
                    context.duration = 0.25
                    card.animator().alphaValue = 1
                }
            }
        } else if let focused, !cards.contains(where: { $0.button === window?.firstResponder }) {
            if cards.isEmpty { focusCanvas?() }
            else { focusCard(min(cards.count - 1, focused)) }
        }
    }

    private func updateWords() {
        let palette = ScreenshotEditorPalette(self)
        retentionLabel.textColor = palette.muted
        descriptionLabel.textColor = palette.muted
        menuDescription.textColor = palette.muted
        let words = keepsHistory
            ? "Your captures appear here, newest first. Each one keeps its original and your marks on this Mac \(policy.duration), so you can change them later."
            : "Recent Captures is off. Turn on to keep each capture and its editable marks on this Mac \(policy.duration)."
        let bold = keepsHistory ? "Your captures appear here, newest first." : "Recent Captures is off."
        descriptionLabel.attributedStringValue = {
            let text = NSMutableAttributedString(string: words, attributes: [.font: NSFont.systemFont(ofSize: 16),
                                                                             .foregroundColor: palette.muted])
            text.addAttributes([.font: NSFont.systemFont(ofSize: 16, weight: .semibold), .foregroundColor: palette.ink],
                               range: NSRange(location: 0, length: (bold as NSString).length))
            return text
        }()
        menuDescription.stringValue = "On: each capture keeps its original and your marks on this Mac \(policy.duration), so a redaction can be taken off again here. Delete a capture to remove it for good."
        inlineRetention.selection = policy == .week ? 0 : 1
        menuRetention.selection = inlineRetention.selection
    }

    override func layout() {
        super.layout()
        let narrow = bounds.width <= 900
        let side: CGFloat = keepsHistory ? (narrow ? 125 : 236) : 0
        let listWidth = max(0, bounds.width - side)
        background.frame = bounds
        rule.frame = CGRect(x: 0, y: 0, width: bounds.width, height: 1)
        divider.isHidden = side == 0
        divider.frame = CGRect(x: listWidth, y: 0, width: 1, height: bounds.height)
        scroller.frame = CGRect(x: 0, y: 1, width: listWidth, height: bounds.height - 1)
        let start: CGFloat = keepsHistory ? 22 : 140
        descriptionLabel.frame = CGRect(x: start, y: 34, width: max(0, listWidth - start - 22), height: 74)
        enable.frame = CGRect(x: 22, y: 54, width: 101, height: 34)
        enable.isHidden = keepsHistory
        retentionLabel.isHidden = !keepsHistory || narrow
        inlineRetention.isHidden = retentionLabel.isHidden
        clear.isHidden = !keepsHistory
        more.isHidden = !keepsHistory
        retentionLabel.frame = CGRect(x: listWidth + 16, y: 10, width: 205, height: 21)
        inlineRetention.frame = CGRect(x: listWidth + 16, y: 36, width: 205, height: 36)
        clear.frame = CGRect(x: listWidth + 12, y: narrow ? 14 : 82, width: narrow ? 101 : 100, height: 34)
        more.frame = CGRect(x: listWidth + (narrow ? 12 : 120), y: narrow ? 55 : 82,
                            width: narrow ? 101 : 42, height: 34)
        var x: CGFloat = 14
        for card in cards {
            card.frame = CGRect(x: x, y: 10, width: card.requiredWidth, height: 115)
            x = card.frame.maxX + 6
        }
        row.frame = CGRect(x: 0, y: 0, width: max(listWidth, x + 8), height: bounds.height - 1)
        loadVisibleThumbnails()
    }

    private func loadVisibleThumbnails() {
        guard !isHidden, scroller.frame.width > 0, !scroller.isHidden else { return }
        let region = scroller.documentVisibleRect.insetBy(dx: -120, dy: 0)
        for card in cards where card.frame.intersects(region) && !card.requested {
            card.requested = true
            let store = self.store
            let entry = card.entry
            DispatchQueue.global(qos: .utility).async { [weak card] in
                let bitmap = store.thumbnail(entry)
                DispatchQueue.main.async { [weak card] in
                    guard let card else { return }
                    card.image = bitmap.map { NSImage(cgImage: $0, size: .zero) }
                }
            }
        }
    }

    func handle(_ event: NSEvent) -> Bool {
        guard let window, let index = cards.firstIndex(where: {
            $0.button === window.firstResponder || $0.delete === window.firstResponder
        }), event.modifierFlags.intersection([.command, .option, .control, .shift]).isEmpty else { return false }
        switch event.keyCode {
        case 123: focusCard(max(0, index - 1))
        case 124: focusCard(min(cards.count - 1, index + 1))
        case 51, 117:
            let selected = cards[index].entry
            remove?(selected)
            if !cards.isEmpty { focusCard(min(index, cards.count - 1)) }
        case 53: focusCanvas?()
        default: return false
        }
        return true
    }

    private func focusCard(_ index: Int) {
        let card = cards[index]
        row.scrollToVisible(card.frame)
        window?.makeFirstResponder(card.button)
    }

    private func buildOptions() {
        let controller = NSViewController()
        let content = ScreenshotSurface(.floating)
        let hide = ScreenshotButton("Hide Recent Captures", style: .plain)
        let off = ScreenshotButton("Turn Off Recent Captures…", style: .plain)
        let keep = screenshotLabel("Keep captures for")
        for view in [hide, off, keep, menuRetention, menuDescription] { content.addSubview(view) }
        hide.frame = CGRect(x: 8, y: 8, width: 218, height: 36)
        off.frame = CGRect(x: 8, y: 46, width: 252, height: 36)
        keep.frame = CGRect(x: 17, y: 96, width: 143, height: 21)
        menuRetention.frame = CGRect(x: 159, y: 88, width: 183, height: 36)
        menuDescription.frame = CGRect(x: 17, y: 134, width: 326, height: 91)
        hide.onPress = { [weak self] in self?.popover.performClose(nil); self?.hide?() }
        off.onPress = { [weak self] in self?.popover.performClose(nil); self?.turnOff?() }
        controller.view = content
        popover.contentSize = CGSize(width: 360, height: 228)
        popover.contentViewController = controller
        popover.behavior = .transient
    }

    private func showOptions() {
        if popover.isShown { popover.performClose(nil) }
        else { popover.show(relativeTo: more.bounds, of: more, preferredEdge: .minY) }
    }

    func focusOptions() { window?.makeFirstResponder(more) }
    func focusClear() { window?.makeFirstResponder(clear) }
}

@MainActor
private final class ScreenshotShelfCard: NSView {
    let entry: ScreenshotStore.Entry
    let button = ScreenshotShelfEntryButton()
    let delete = ScreenshotButton("Delete capture", symbol: "xmark", style: .deleteCapture)
    var requested = false
    var image: NSImage? { didSet { needsDisplay = true } }
    var selected = false {
        didSet {
            button.setAccessibilityValue(selected ? "Selected" : "")
            needsDisplay = true
        }
    }
    var requiredWidth: CGFloat { max(112, ScreenshotRenderer.thumbnailWidth(for: entry.edits.crop) + 12) }
    private var hovering = false
    private var tracker: NSTrackingArea?
    private var edited: Bool { entry.edited || !entry.edits.marks.isEmpty }

    init(_ entry: ScreenshotStore.Entry) {
        self.entry = entry
        super.init(frame: .zero)
        addSubview(button)
        addSubview(delete)
        button.isBordered = false
        button.focusRingType = .none
        button.target = button
        button.action = #selector(ScreenshotShelfEntryButton.pressed)
        let name = entry.name ?? (entry.kind == .scrolling ? "Scrolling screenshot" : "\(entry.kind.title) screenshot")
        let crop = entry.edits.crop
        let held = entry.redactions > 0 ? "Holds the original under \(entry.redactions) \(entry.redactions == 1 ? "redaction" : "redactions")"
            : edited ? "Holds the original and your marks" : "Holds the original"
        let tooltip = "\(name)\n\(entry.kind.title) · \(Int(crop.width)) × \(Int(crop.height)) px · \(entry.created.formatted(date: .abbreviated, time: .shortened))\n\(held)"
        button.toolTip = tooltip
        button.setAccessibilityLabel(tooltip.replacingOccurrences(of: "\n", with: ", "))
        button.focusChanged = { [weak self] in self?.updateDelete() }
        delete.toolTip = "Delete capture"
        delete.iconOnly = true
        delete.isHidden = true
    }

    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }
    override func viewDidChangeEffectiveAppearance() { super.viewDidChangeEffectiveAppearance(); needsDisplay = true }

    override func layout() {
        super.layout()
        button.frame = bounds
        delete.frame = CGRect(x: bounds.maxX - 29, y: 1, width: 28, height: 28)
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let tracker { removeTrackingArea(tracker) }
        let area = NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
                                  owner: self, userInfo: nil)
        tracker = area
        addTrackingArea(area)
    }

    override func mouseEntered(with event: NSEvent) { hovering = true; updateDelete() }
    override func mouseExited(with event: NSEvent) { hovering = false; updateDelete() }

    private func updateDelete() {
        delete.isHidden = !(hovering || window?.firstResponder === button || window?.firstResponder === delete)
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        let p = ScreenshotEditorPalette(self)
        let frame = NSBezierPath(roundedRect: bounds.insetBy(dx: 1, dy: 1), xRadius: 11, yRadius: 11)
        if selected || hovering {
            (selected ? p.floating : p.hover).setFill()
            frame.fill()
        }
        if selected || window?.firstResponder === button {
            (window?.firstResponder === button ? p.gold : p.control).setStroke()
            frame.stroke()
        }
        let width = ScreenshotRenderer.thumbnailWidth(for: entry.edits.crop)
        let thumbnail = CGRect(x: (bounds.width - width) / 2, y: 7, width: width, height: 78)
        if selected {
            p.gold.setStroke()
            let ring = NSBezierPath(roundedRect: thumbnail.insetBy(dx: -3, dy: -3), xRadius: 6, yRadius: 6)
            ring.lineWidth = 3
            ring.stroke()
        }
        let picture = NSBezierPath(roundedRect: thumbnail, xRadius: 4, yRadius: 4)
        p.ground.setFill()
        picture.fill()
        NSGraphicsContext.saveGraphicsState()
        picture.addClip()
        image?.draw(in: thumbnail, from: .zero, operation: .sourceOver, fraction: 1, respectFlipped: true, hints: nil)
        NSGraphicsContext.restoreGraphicsState()
        p.control.setStroke()
        picture.lineWidth = 0.5
        picture.stroke()
        let clock = Calendar.current.isDateInToday(entry.created)
            ? entry.created.formatted(date: .omitted, time: .shortened)
            : entry.created.formatted(.dateTime.weekday(.abbreviated).hour().minute())
        let ink = selected ? p.ink : p.muted
        let label = NSAttributedString(string: clock, attributes: [
            .font: NSFont.monospacedDigitSystemFont(ofSize: 16, weight: selected ? .semibold : .regular),
            .foregroundColor: ink
        ])
        let available = bounds.width - (edited ? 32 : 20)
        let textWidth = min(label.size().width, available)
        let left = max(4, bounds.midX - (textWidth + (edited ? 30 : 20)) / 2)
        screenshotSymbol(entry.kind.symbol, color: ink)?.draw(in: CGRect(x: left, y: 94, width: 16, height: 16))
        label.draw(with: CGRect(x: left + 19, y: 92, width: textWidth, height: 21),
                   options: [.truncatesLastVisibleLine, .usesLineFragmentOrigin])
        if edited {
            p.gold.setFill()
            NSBezierPath(ovalIn: CGRect(x: left + 23 + textWidth, y: 99, width: 7, height: 7)).fill()
        }
    }
}

@MainActor
private final class ScreenshotShelfEntryButton: NSButton {
    var onPress: (() -> Void)?
    var focusChanged: (() -> Void)?
    @objc func pressed() { onPress?() }
    override func draw(_ dirtyRect: NSRect) {}
    override var acceptsFirstResponder: Bool { isEnabled }
    override func becomeFirstResponder() -> Bool {
        defer { DispatchQueue.main.async { [weak self] in self?.focusChanged?() } }
        return super.becomeFirstResponder()
    }
    override func resignFirstResponder() -> Bool {
        defer { DispatchQueue.main.async { [weak self] in self?.focusChanged?() } }
        return super.resignFirstResponder()
    }
    override func keyDown(with event: NSEvent) {
        if [36, 76].contains(event.keyCode) { performClick(nil) }
        else { super.keyDown(with: event) }
    }
}
