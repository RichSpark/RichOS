import AppKit

@MainActor
final class ScreenshotRecents: NSObject, NSWindowDelegate {
    let window: NSPanel
    var edit: ((ScreenshotStore.Entry) -> Void)?
    var closed: (() -> Void)?
    private let store: ScreenshotStore
    private let title = NSTextField(wrappingLabelWithString: "")
    private let empty = NSTextField(labelWithString: "")
    private let toggle = NSSwitch()
    private let clear = NSButton(title: "Clear All…", target: nil, action: nil)
    private let scroll = NSScrollView()
    private let cards = RecentStrip()
    private var entries: [ScreenshotStore.Entry] = []
    private var selected = 0

    init(store: ScreenshotStore) {
        self.store = store
        window = RecentPanel(contentRect: CGRect(x: 0, y: 0, width: 840, height: 445),
                             styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        super.init()
        window.title = "Recent Captures"
        window.minSize = CGSize(width: 520, height: 360)
        window.isReleasedWhenClosed = false
        window.delegate = self
        (window as? RecentPanel)?.handle = { [weak self] event in self?.handle(event) ?? false }

        let root = NSStackView()
        root.orientation = .vertical
        root.alignment = .width
        root.spacing = 15
        title.font = .systemFont(ofSize: 16)
        title.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        toggle.target = self
        toggle.action = #selector(toggleHistory)
        toggle.setAccessibilityLabel("Show Recent Captures")
        style(clear, for: #selector(clearAll))
        let header = NSStackView(views: [toggle, title, clear])
        header.spacing = 14
        header.alignment = .top
        root.addArrangedSubview(header)

        cards.spacing = 12
        cards.alignment = .top
        cards.edgeInsets = NSEdgeInsets(top: 6, left: 4, bottom: 6, right: 4)
        scroll.documentView = cards
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        scroll.drawsBackground = false
        scroll.setAccessibilityLabel("Recent capture cards")
        scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 255).isActive = true
        scroll.setContentHuggingPriority(.defaultLow, for: .vertical)
        root.addArrangedSubview(scroll)

        empty.font = .systemFont(ofSize: 16)
        empty.alignment = .center
        empty.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        root.addArrangedSubview(empty)
        let content = NSView()
        content.addSubview(root)
        root.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            root.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 18),
            root.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -18),
            root.topAnchor.constraint(equalTo: content.topAnchor, constant: 18),
            root.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -18),
        ])
        window.contentView = content
    }

    func show() {
        refresh()
        if !window.isVisible { window.center() }
        window.makeKeyAndOrderFront(nil)
        if entries.isEmpty { window.makeFirstResponder(toggle) }
        else { focusSelected() }
    }

    func refresh() {
        let selectedID = entries.indices.contains(selected) ? entries[selected].id : nil
        let enabled = store.keepsHistory
        toggle.state = enabled ? .on : .off
        title.stringValue = enabled
            ? "On. Captures are kept on this Mac for 7 days. Each capture holds the original and your marks, including pixels under any redaction."
            : "Off. Turn on Recent Captures to keep screenshots and editable marks on this Mac for 7 days."
        do {
            entries = try store.entries()
            empty.stringValue = enabled ? "No recent captures yet." : "Recent Captures is off."
        } catch {
            entries = []
            empty.stringValue = "Could not read Recent Captures. Reopen this window to try again."
        }
        scroll.isHidden = entries.isEmpty
        empty.isHidden = !entries.isEmpty
        clear.isEnabled = !entries.isEmpty
        selected = entries.firstIndex { $0.id == selectedID } ?? min(selected, max(0, entries.count - 1))
        for card in cards.arrangedSubviews { card.removeFromSuperview() }
        for (index, entry) in entries.enumerated() { cards.addArrangedSubview(card(for: entry, index: index)) }
        if !entries.isEmpty { cards.frame.size = cards.fittingSize }
    }

    func windowWillClose(_ notification: Notification) { closed?() }

    private func handle(_ event: NSEvent) -> Bool {
        guard event.modifierFlags.intersection([.shift, .command, .option, .control]).isEmpty else { return false }
        if event.keyCode == 53 {
            if !event.isARepeat { window.performClose(nil) }
            return true
        }
        guard entries.indices.contains(selected) else { return false }
        switch event.keyCode {
        case 123, 124:
            selected = min(entries.count - 1, max(0, selected + (event.keyCode == 123 ? -1 : 1)))
            focusSelected()
        case 36, 76:
            if let button = window.firstResponder as? NSButton, button.action != #selector(editRow(_:)) { return false }
            if window.firstResponder is NSSwitch { return false }
            if !event.isARepeat { edit?(entries[selected]) }
        case 51, 117:
            if !event.isARepeat { delete(entries[selected].id) }
        default: return false
        }
        return true
    }

    private func focusSelected() {
        guard cards.arrangedSubviews.indices.contains(selected),
              let card = cards.arrangedSubviews[selected] as? NSStackView,
              let actions = card.arrangedSubviews.last as? NSStackView,
              let button = actions.arrangedSubviews.first as? NSButton else { return }
        cards.scrollToVisible(card.frame)
        window.makeFirstResponder(button)
    }

    private func delete(_ id: UUID) {
        do {
            try store.remove(id)
            refresh()
            if entries.isEmpty { window.makeFirstResponder(toggle) }
            else { focusSelected() }
        } catch { showError("Could not delete this capture.") }
    }

    @objc private func deleteRow(_ sender: NSButton) {
        if entries.indices.contains(sender.tag) { delete(entries[sender.tag].id) }
    }

    @objc private func editRow(_ sender: NSButton) {
        if entries.indices.contains(sender.tag) {
            selected = sender.tag
            edit?(entries[selected])
        }
    }

    @objc private func clearAll() {
        confirm("Clear all recent captures?", details: "Deletes the local originals and saved marks in Recent Captures.", accept: "Clear All") { [weak self] in
            guard let self else { return }
            do { try store.clear(); refresh(); window.makeFirstResponder(toggle) }
            catch { showError("Could not clear Recent Captures.") }
        }
    }

    @objc private func toggleHistory() {
        if toggle.state == .on {
            do { try store.setHistory(true); refresh() }
            catch { refresh(); showError("Could not turn on Recent Captures.") }
        } else {
            toggle.state = .on
            confirm("Turn off Recent Captures?", details: "Turning this off deletes the local originals and saved marks in Recent Captures.", accept: "Turn Off and Delete") { [weak self] in
                guard let self else { return }
                do { try store.setHistory(false); refresh() }
                catch { refresh(); showError("Could not turn off Recent Captures.") }
            }
        }
    }

    private func confirm(_ title: String, details: String, accept: String, action: @escaping () -> Void) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = details
        alert.addButton(withTitle: "Cancel")
        alert.addButton(withTitle: accept)
        alert.beginSheetModal(for: window) { if $0 == .alertSecondButtonReturn { action() } }
    }

    private func showError(_ text: String) {
        let alert = NSAlert()
        alert.messageText = text
        alert.addButton(withTitle: "OK")
        alert.beginSheetModal(for: window)
    }

    private func style(_ button: NSButton, for action: Selector) {
        button.font = .systemFont(ofSize: 16, weight: .medium)
        button.controlSize = .large
        button.bezelStyle = .rounded
        button.target = self
        button.action = action
    }

    private func card(for entry: ScreenshotStore.Entry, index: Int) -> NSView {
        let picture = NSImageView(image: store.thumbnail(entry) ?? NSImage())
        picture.imageScaling = .scaleProportionallyUpOrDown
        picture.imageAlignment = .alignTop
        picture.setAccessibilityElement(false)
        picture.widthAnchor.constraint(equalToConstant: 170).isActive = true
        picture.heightAnchor.constraint(equalToConstant: 100).isActive = true
        let origin = NSTextField(labelWithString: entry.kind.title + " · " + entry.created.formatted(date: .omitted, time: .shortened))
        origin.font = .systemFont(ofSize: 16, weight: .semibold)
        let crop = entry.edits.crop.integral
        let dimensions = NSTextField(labelWithString: "\(Int(crop.width)) × \(Int(crop.height)) px" + (index == 0 ? " · last" : ""))
        dimensions.font = .monospacedDigitSystemFont(ofSize: 16, weight: .regular)
        let detail = NSTextField(wrappingLabelWithString: entry.redactions > 0
            ? "Holds the original under \(entry.redactions) \(entry.redactions == 1 ? "redaction" : "redactions")"
            : entry.edits.marks.isEmpty ? "Holds the original" : "Holds the original and your marks")
        detail.font = .systemFont(ofSize: 16)
        detail.preferredMaxLayoutWidth = 170
        detail.widthAnchor.constraint(equalToConstant: 170).isActive = true
        let open = NSButton(title: "Edit", target: nil, action: nil)
        let delete = NSButton(image: NSImage(systemSymbolName: "trash", accessibilityDescription: nil) ?? NSImage(),
                              target: nil, action: nil)
        style(open, for: #selector(editRow(_:)))
        style(delete, for: #selector(deleteRow(_:)))
        open.tag = index
        delete.tag = index
        delete.setAccessibilityLabel("Delete capture")
        let actions = NSStackView(views: [open, delete])
        actions.spacing = 8
        let card = RecentCard(views: [picture, origin, dimensions, detail, actions])
        card.orientation = .vertical
        card.alignment = .leading
        card.edgeInsets = NSEdgeInsets(top: 9, left: 10, bottom: 9, right: 10)
        card.spacing = 6
        card.wantsLayer = true
        card.style()
        return card
    }
}

private final class RecentPanel: NSPanel {
    var handle: ((NSEvent) -> Bool)?
    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown, attachedSheet == nil, handle?(event) == true { return }
        super.sendEvent(event)
    }
}

private final class RecentStrip: NSStackView {
    override var isFlipped: Bool { true }
}

private final class RecentCard: NSStackView {
    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        style()
    }

    func style() {
        let colors = ScreenshotChrome.colors(for: self)
        layer?.backgroundColor = colors.surface.cgColor
        layer?.borderColor = colors.gold.cgColor
        layer?.borderWidth = 1
        layer?.cornerRadius = 9
    }
}
