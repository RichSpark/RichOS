import AppKit
import RichFilesCore
import UniformTypeIdentifiers

@MainActor
enum ScreenshotChrome {
    static func colors(for view: NSView) -> (ground: NSColor, surface: NSColor, ink: NSColor, gold: NSColor) {
        if view.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua {
            return (ScreenshotRenderer.ink, rgb(24, 36, 64), rgb(223, 228, 238), rgb(194, 163, 92))
        }
        return (rgb(234, 230, 221), rgb(253, 252, 248), ScreenshotRenderer.ink, rgb(113, 87, 21))
    }

    private static func rgb(_ red: CGFloat, _ green: CGFloat, _ blue: CGFloat) -> NSColor {
        NSColor(srgbRed: red / 255, green: green / 255, blue: blue / 255, alpha: 1)
    }
}

@MainActor
final class ScreenshotEditor: NSObject, NSWindowDelegate {
    let window: NSWindow
    let canvas: ScreenshotCanvas
    var closed: (() -> Void)?
    var retainEdits: ((ScreenshotDocument) -> Bool)?
    private let scroll = NSScrollView()
    private let tools = NSStackView()
    private let properties = NSStackView()
    private let info = NSTextField(wrappingLabelWithString: "")
    private let status = NSTextField(labelWithString: "")
    private let dimensions = NSTextField(labelWithString: "")
    private let zoomReadout = NSTextField(labelWithString: "100%")
    private let selection = NSTextField(labelWithString: "")
    private let ink = NSPopUpButton()
    private let size = NSPopUpButton()
    private var toolButtons: [ScreenshotTool: NSButton] = [:]
    private var undoButton: NSButton!
    private var redoButton: NSButton!
    private var editButton: NSButton!
    private var deleteButton: NSButton!
    private var earlierButton: NSButton!
    private var laterButton: NSButton!
    private var revealButton: NSButton!
    private var cropButton: NSButton!
    private var cancelCropButton: NSButton!
    private var zoom = ScreenshotCanvas.Zoom.fit
    private var initialLayout = true
    private var laidOutCrop = CGRect.null
    private var laidOutTool = ScreenshotTool.select
    private var closeConfirmed = false
    private var panelOpen = false
    private var lastShared: ScreenshotEdits?
    private let began = ProcessInfo.processInfo.systemUptime

    init(document: ScreenshotDocument) {
        canvas = ScreenshotCanvas(document: document)
        window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 1120, height: 760),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable],
                          backing: .buffered, defer: false)
        super.init()
        let time = document.created.formatted(date: .omitted, time: .shortened)
        window.title = document.kind.title + " · " + time
        window.setAccessibilityTitle("RichGlance Screenshot Editor")
        window.minSize = CGSize(width: 520, height: 420)
        window.isReleasedWhenClosed = false
        window.delegate = self
        build()
        canvas.changed = { [weak self] in self?.refresh() }
        canvas.keys = { [weak self] in self?.handle($0) ?? false }
        canvas.editText = { [weak self] in self?.showTextEntry() }
        NotificationCenter.default.addObserver(self, selector: #selector(scrolled),
                                               name: NSView.boundsDidChangeNotification, object: scroll.contentView)
    }

    deinit { NotificationCenter.default.removeObserver(self) }

    func show() {
        let visible = (NSScreen.main ?? window.screen)?.visibleFrame ?? window.frame
        window.setContentSize(CGSize(width: min(1120, visible.width - 32), height: min(760, visible.height - 48)))
        window.center()
        if #available(macOS 14, *) { NSApp.activate() }
        else { NSApp.activate(ignoringOtherApps: true) }
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(canvas)
        window.contentView?.layoutSubtreeIfNeeded()
        layoutCanvas()
        refresh()
        record(.complete, from: began)
    }

    private func build() {
        let root = NSStackView()
        root.orientation = .vertical
        root.alignment = .width
        root.spacing = 0
        root.wantsLayer = true
        root.layer?.backgroundColor = ScreenshotChrome.colors(for: root).surface.cgColor
        window.contentView = root
        tools.spacing = 4
        tools.edgeInsets = NSEdgeInsets(top: 7, left: 10, bottom: 7, right: 10)
        for tool in ScreenshotTool.allCases {
            let control = button(tool.title, action: #selector(chooseTool(_:)))
            control.tag = ScreenshotTool.allCases.firstIndex(of: tool) ?? 0
            control.image = NSImage(systemSymbolName: tool.symbol, accessibilityDescription: nil)?
                .withSymbolConfiguration(.init(pointSize: 22, weight: .medium))
            control.isBordered = false
            control.wantsLayer = true
            control.layer?.cornerRadius = 9
            control.focusRingType = .default
            control.imagePosition = .imageAbove
            control.setButtonType(.pushOnPushOff)
            control.toolTip = "\(tool.title) (\(tool.key.uppercased()))"
            control.heightAnchor.constraint(equalToConstant: 54).isActive = true
            control.widthAnchor.constraint(greaterThanOrEqualToConstant: 62).isActive = true
            toolButtons[tool] = control
            tools.addArrangedSubview(control)
        }
        undoButton = button("Undo", action: #selector(undo))
        redoButton = button("Redo", action: #selector(redo))
        let save = button("Save PNG…  ⌘S", action: #selector(saveImage))
        let copy = button("Copy Image  ⌘C", action: #selector(copyImage))
        save.setAccessibilityLabel("Save PNG…")
        copy.setAccessibilityLabel("Copy Image")
        copy.bezelColor = ScreenshotInk.yellow.color
        for control in [undoButton!, redoButton!, save, copy] {
            tools.addArrangedSubview(control)
        }
        root.addArrangedSubview(strip(tools, height: 70))
        properties.spacing = 8
        properties.edgeInsets = NSEdgeInsets(top: 5, left: 16, bottom: 5, right: 16)
        selection.font = .systemFont(ofSize: 16, weight: .semibold)
        ink.addItems(withTitles: ScreenshotInk.allCases.map { $0.rawValue.capitalized })
        ink.target = self
        ink.action = #selector(changeInk)
        ink.setAccessibilityLabel("Mark color")
        size.addItems(withTitles: ["Regular", "Large"])
        size.target = self
        size.action = #selector(changeSize)
        size.setAccessibilityLabel("Mark size")
        for control in [ink, size] {
            control.font = .systemFont(ofSize: 16)
            control.controlSize = .large
        }
        editButton = button("Edit Text", action: #selector(showTextEntry))
        deleteButton = button("Delete", action: #selector(deleteMark))
        earlierButton = button("Make Earlier", action: #selector(earlier))
        laterButton = button("Make Later", action: #selector(later))
        revealButton = button("Show", action: #selector(reveal))
        cropButton = button("Crop", action: #selector(commitCrop))
        cancelCropButton = button("Cancel", action: #selector(cancelCrop))
        for view in [selection, ink, size, editButton!, earlierButton!, laterButton!, deleteButton!,
                     revealButton!, cancelCropButton!, cropButton!] as [NSView] {
            properties.addArrangedSubview(view)
        }
        root.addArrangedSubview(strip(properties, height: 42))
        info.font = .systemFont(ofSize: 16)
        info.maximumNumberOfLines = 3
        info.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let guidance = NSStackView(views: [info])
        guidance.edgeInsets = NSEdgeInsets(top: 8, left: 16, bottom: 10, right: 16)
        root.addArrangedSubview(guidance)
        scroll.documentView = canvas
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = true
        scroll.borderType = .noBorder
        scroll.drawsBackground = false
        scroll.contentView.postsBoundsChangedNotifications = true
        scroll.setAccessibilityLabel("Editable screenshot")
        root.addArrangedSubview(scroll)
        scroll.setContentHuggingPriority(.defaultLow, for: .vertical)
        scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 110).isActive = true
        status.font = .systemFont(ofSize: 16)
        status.lineBreakMode = .byTruncatingTail
        status.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        dimensions.font = .monospacedDigitSystemFont(ofSize: 16, weight: .medium)
        zoomReadout.font = .monospacedDigitSystemFont(ofSize: 16, weight: .medium)
        let bottom = NSStackView(views: [
            dimensions, status, NSView(), button("Fit", action: #selector(fit)), button("100%", action: #selector(actual)),
            button("−", action: #selector(zoomOut)), zoomReadout, button("+", action: #selector(zoomIn)),
        ])
        bottom.edgeInsets = NSEdgeInsets(top: 6, left: 10, bottom: 6, right: 10)
        bottom.spacing = 10
        root.addArrangedSubview(bottom)
    }

    private func strip(_ stack: NSStackView, height: CGFloat) -> NSScrollView {
        let strip = NSScrollView()
        strip.drawsBackground = false
        strip.hasHorizontalScroller = true
        strip.autohidesScrollers = true
        strip.scrollerStyle = .overlay
        let host = NSView()
        host.addSubview(stack)
        stack.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: host.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: host.trailingAnchor),
            stack.centerYAnchor.constraint(equalTo: host.centerYAnchor),
        ])
        strip.documentView = host
        host.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            host.leadingAnchor.constraint(equalTo: strip.contentView.leadingAnchor),
            host.topAnchor.constraint(equalTo: strip.contentView.topAnchor),
            host.heightAnchor.constraint(equalToConstant: height),
            strip.heightAnchor.constraint(equalToConstant: height),
        ])
        strip.setContentHuggingPriority(.defaultLow, for: .horizontal)
        return strip
    }

    private func button(_ title: String, action: Selector) -> NSButton {
        let button = NSButton(title: title, target: self, action: action)
        button.bezelStyle = .rounded
        button.font = .systemFont(ofSize: 16, weight: .medium)
        button.controlSize = .large
        button.setContentCompressionResistancePriority(.required, for: .horizontal)
        return button
    }

    private func layoutCanvas() {
        let size = scroll.contentView.bounds.size
        guard size.width > 1, size.height > 1 else { return }
        if initialLayout {
            initialLayout = false
            let crop = canvas.document.crop
            if crop.height / crop.width * size.width > size.height * 2 {
                let backing = window.backingScaleFactor
                zoom = .custom(min(1, max(1, size.width - 78) * backing / crop.width))
            }
        }
        canvas.layoutImage(in: size, zoom: zoom)
        zoomReadout.stringValue = "\(Int((canvas.factor * window.backingScaleFactor * 100).rounded()))%"
    }

    private func refresh() {
        if laidOutCrop != canvas.document.crop || laidOutTool != canvas.tool {
            laidOutCrop = canvas.document.crop
            laidOutTool = canvas.tool
            layoutCanvas()
        }
        let mark = canvas.selectedMark
        let crop = canvas.document.crop
        window.isDocumentEdited = canvas.hasChanges
        dimensions.stringValue = "\(Int(crop.width)) × \(Int(crop.height)) px"
        info.stringValue = canvas.document.hasRedactions
            ? "Copy and Save make one flat picture. Under the redaction the pixels are replaced. The original stays only on this Mac."
            : "Copy and Save make one flat picture of what is inside the frame. Your marks and the original stay on this Mac."
        let colors = ScreenshotChrome.colors(for: canvas)
        window.contentView?.layer?.backgroundColor = colors.surface.cgColor
        for (tool, control) in toolButtons {
            let active = tool == canvas.tool
            let ink = active ? ScreenshotRenderer.ink : colors.ink
            control.state = active ? .on : .off
            control.contentTintColor = ink
            control.layer?.backgroundColor = (active ? ScreenshotInk.yellow.color : NSColor.clear).cgColor
            control.attributedTitle = NSAttributedString(string: tool.title, attributes: [
                .font: NSFont.systemFont(ofSize: 16, weight: active ? .bold : .medium),
                .foregroundColor: ink,
                .underlineStyle: active ? NSUnderlineStyle.thick.rawValue : 0,
            ])
        }
        let cropping = canvas.tool == .crop
        let styled = !cropping && (mark?.tool ?? canvas.tool) != .redact && (mark != nil || canvas.tool != .select)
        selection.stringValue = cropping ? "Crop" : (mark?.tool ?? canvas.tool).title
        ink.isHidden = !styled
        size.isHidden = !styled
        ink.selectItem(withTitle: (mark?.ink ?? canvas.ink).rawValue.capitalized)
        size.selectItem(at: (mark?.large ?? canvas.large) ? 1 : 0)
        editButton.isHidden = cropping || mark?.tool != .text
        earlierButton.isHidden = cropping || mark?.tool != .steps
        laterButton.isHidden = earlierButton.isHidden
        if let mark, let step = canvas.document.edits.number(for: mark) {
            earlierButton.isEnabled = step > 1
            laterButton.isEnabled = step < canvas.document.edits.marks.filter { $0.tool == .steps }.count
        }
        deleteButton.isHidden = cropping || mark == nil
        undoButton.isEnabled = canvas.canUndo
        redoButton.isEnabled = canvas.canRedo
        cropButton.isHidden = !cropping
        cancelCropButton.isHidden = !cropping
        scrolled()
    }

    @objc private func scrolled() {
        revealButton.isHidden = canvas.tool == .crop || !canvas.selectionIsOffscreen
        if !revealButton.isHidden, let mark = canvas.selectedMark { selection.stringValue = mark.tool.title + " · Out of view" }
        else if canvas.tool != .crop { selection.stringValue = (canvas.selectedMark?.tool ?? canvas.tool).title }
    }

    private func select(_ tool: ScreenshotTool) {
        canvas.selected = nil
        canvas.tool = tool
        focusCanvas()
    }

    private func focusCanvas() { window.makeFirstResponder(canvas) }
    @objc private func chooseTool(_ sender: NSButton) { select(ScreenshotTool.allCases[sender.tag]) }
    @objc private func changeInk() { canvas.setStyle(ink: ScreenshotInk.allCases[max(0, ink.indexOfSelectedItem)]); focusCanvas() }
    @objc private func changeSize() { canvas.setStyle(large: size.indexOfSelectedItem == 1); focusCanvas() }
    @objc private func deleteMark() { canvas.deleteSelected(); focusCanvas() }
    @objc private func earlier() { canvas.reorderSelected(by: -1); focusCanvas() }
    @objc private func later() { canvas.reorderSelected(by: 1); focusCanvas() }
    @objc private func reveal() { canvas.revealSelection(); focusCanvas() }
    @objc private func undo() { canvas.undo(); focusCanvas() }
    @objc private func redo() { canvas.redo(); focusCanvas() }
    @objc private func commitCrop() { canvas.commitCrop(); focusCanvas() }
    @objc private func cancelCrop() { canvas.cancelCrop(); focusCanvas() }
    @objc private func fit() { zoom = .fit; layoutCanvas(); focusCanvas() }
    @objc private func actual() { zoom = .actual; layoutCanvas(); focusCanvas() }
    @objc private func zoomIn() { changeZoom(by: 1.25) }
    @objc private func zoomOut() { changeZoom(by: 0.8) }

    private func changeZoom(by multiplier: CGFloat) {
        zoom = .custom(min(6, max(0.005, canvas.factor * window.backingScaleFactor * multiplier)))
        layoutCanvas()
        focusCanvas()
    }

    @objc private func showTextEntry() {
        guard !panelOpen, let mark = canvas.selectedMark, mark.tool == .text else { return }
        let field = NSTextField(string: mark.text)
        field.font = .systemFont(ofSize: 16)
        field.placeholderString = "Callout text"
        field.frame.size = CGSize(width: 320, height: 28)
        let alert = NSAlert()
        alert.messageText = "Edit Callout"
        alert.accessoryView = field
        alert.addButton(withTitle: "Done")
        alert.addButton(withTitle: "Cancel")
        alert.window.initialFirstResponder = field
        panelOpen = true
        alert.beginSheetModal(for: window) { [weak self] response in
            guard let self else { return }
            panelOpen = false
            if response == .alertFirstButtonReturn { canvas.editSelected { $0.text = field.stringValue } }
            else if mark.text.isEmpty { canvas.deleteSelected() }
            focusCanvas()
        }
    }

    @objc func copyImage() {
        if canvas.tool == .crop { canvas.commitCrop() }
        let start = ProcessInfo.processInfo.systemUptime
        if let image = ScreenshotRenderer.flattened(canvas.document),
           let png = ScreenshotRenderer.png(image), ScreenshotRenderer.copy(png) {
            lastShared = canvas.document.edits
            announce("Copied \(image.width) × \(image.height) px as one flat picture")
            record(.complete, from: start)
        } else {
            announce("Could not copy the picture. Try again.")
            record(.issues, from: start)
        }
        focusCanvas()
    }

    @objc func saveImage() {
        guard !panelOpen else { return }
        if canvas.tool == .crop { canvas.commitCrop() }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.png]
        panel.nameFieldStringValue = "RichGlance Screenshot.png"
        panelOpen = true
        panel.beginSheetModal(for: window) { [weak self] response in
            guard let self else { return }
            panelOpen = false
            defer { focusCanvas() }
            guard response == .OK, let url = panel.url else { return }
            let start = ProcessInfo.processInfo.systemUptime
            do {
                guard let image = ScreenshotRenderer.flattened(canvas.document),
                      let data = ScreenshotRenderer.png(image) else { throw CocoaError(.fileWriteUnknown) }
                try data.write(to: url, options: .atomic)
                lastShared = canvas.document.edits
                announce("Saved \(image.width) × \(image.height) px as one flat picture")
                record(.complete, from: start)
            } catch {
                announce("Could not save the PNG. Choose a different location and try again.")
                record(.issues, from: start)
            }
        }
    }

    private func announce(_ text: String) {
        status.stringValue = text
        status.toolTip = text
        NSAccessibility.post(element: status, notification: .valueChanged)
    }

    private func handle(_ event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask).subtracting([.numericPad, .function, .capsLock])
        let key = event.charactersIgnoringModifiers?.lowercased() ?? ""
        if flags == .command || flags == [.command, .shift] {
            switch (key, flags) {
            case ("z", .command): undo()
            case ("z", _): redo()
            case ("c", .command): copyImage()
            case ("s", .command): saveImage()
            case ("0", .command): fit()
            case ("1", .command): actual()
            default: return false
            }
            return true
        }
        guard flags.isEmpty || flags == .shift else { return false }
        let step: CGFloat = flags.contains(.shift) ? 10 : 1
        switch event.keyCode {
        case 123: canvas.nudge(dx: -step, dy: 0)
        case 124: canvas.nudge(dx: step, dy: 0)
        case 125: canvas.nudge(dx: 0, dy: step)
        case 126: canvas.nudge(dx: 0, dy: -step)
        case 48: canvas.selectNext(backwards: flags.contains(.shift))
        case 51, 117: deleteMark()
        case 53:
            if canvas.tool == .crop { cancelCrop() }
            else { canvas.selected = nil; canvas.refresh() }
        case 36, 76:
            if canvas.tool == .crop { commitCrop() }
            else { showTextEntry() }
        default:
            guard let tool = ScreenshotTool.allCases.first(where: { $0.key == key }) else { return false }
            select(tool)
        }
        return true
    }

    func windowDidResize(_ notification: Notification) { layoutCanvas() }
    func windowDidChangeBackingProperties(_ notification: Notification) { layoutCanvas() }

    func allowApplicationQuit() -> Bool {
        if windowShouldClose(window) { return true }
        window.makeKeyAndOrderFront(nil)
        return false
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        guard !panelOpen else { return false }
        if closeConfirmed { return true }
        if retainEdits?(canvas.document) == true || !canvas.hasChanges { return true }
        let alert = NSAlert()
        alert.messageText = "Close without keeping your edits?"
        alert.informativeText = "Recent Captures is off or this screenshot could not be kept. You can copy the edited image before closing."
        alert.addButton(withTitle: "Cancel")
        alert.addButton(withTitle: "Close")
        alert.addButton(withTitle: "Copy Image and Close")
        panelOpen = true
        alert.beginSheetModal(for: window) { [weak self] response in
            guard let self else { return }
            panelOpen = false
            if response == .alertThirdButtonReturn {
                copyImage()
                guard lastShared == canvas.document.edits else { return }
            } else if response != .alertSecondButtonReturn { return }
            closeConfirmed = true
            window.performClose(nil)
        }
        return false
    }

    func windowWillClose(_ notification: Notification) { closed?() }

    private func record(_ outcome: RichFilesDiagnostics.Outcome, from start: TimeInterval) {
        RichFilesDiagnostics.record(.glanceCaptureOutput, id: canvas.document.id, outcome: outcome,
                                    started: start, total: 1, confirmed: outcome == .complete ? 1 : 0)
    }
}
