import AppKit
import CoreText
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
final class ScreenshotEditor: NSObject, NSWindowDelegate, NSSearchFieldDelegate, NSPopoverDelegate, NSOpenSavePanelDelegate {
    typealias LeaveSheetPresenter = @MainActor (NSAlert, NSWindow, @escaping (NSApplication.ModalResponse) -> Void) -> Void
    let window: ScreenshotEditorWindow
    var closed: (() -> Void)?
    var capture: (() -> Void)?
    var captureAgain: (() -> Void)?
    var pinImage: ((CGImage, UUID) -> Void)?
    var discardedOriginal: ((UUID) -> Void)?
    private var comparison: ScreenshotCompareView?
    private var chatSheet: ScreenshotChat?
    private var incomingBaselines: [UUID: ScreenshotDocument] = [:]
    private let moreButton = ScreenshotButton("More", symbol: "ellipsis")
    private let searchField = NSSearchField()
    private let searchStatus = screenshotLabel()
    private let keepRedactedButton = ScreenshotButton("Keep Only Redacted Copy…", symbol: "checkmark.shield")
    private var search: Task<Void, Never>?
    private var copyTextTask: Task<Void, Never>?
    private var searchMatches: [UUID: [ScreenshotText.Line]] = [:]
    private var cachedText: [UUID: TextIndexEntry] = [:]
    private var indexedCurrentEdits: ScreenshotEdits?

    private struct TextIndexEntry: Sendable {
        let entry: ScreenshotStore.Entry
        let lines: [ScreenshotText.Line]
    }

    private let compareButton = ScreenshotButton("Compare", symbol: "rectangle.split.2x1")
    private let againButton = ScreenshotButton("Capture Again", symbol: "viewfinder.circle")
    private(set) var canvas: ScreenshotCanvas?
    var hasDocument: Bool { canvas != nil }

    private let store: ScreenshotStore
    private let presentLeaveSheet: LeaveSheetPresenter
    private let root = ScreenshotSurface(.chrome)
    private let titleBar = ScreenshotSurface(.bar)
    private let bottom = ScreenshotSurface(.bar)
    private let rail = ScreenshotSurface(.chrome)
    private let work = ScreenshotSurface(.ground)
    private let titleRule = ScreenshotRule()
    private let titleDivider = ScreenshotRule()
    private let railRule = ScreenshotRule()
    private let cropRule = ScreenshotRule()
    private let bottomRule = ScreenshotRule()
    private let inspector = ScreenshotSurface(.floating, radius: 14, outline: true)
    private let toast = ScreenshotSurface(.floating, radius: 22, outline: true)
    private let scroll = NSScrollView()
    private let empty = ScreenshotSurface(.ground)
    private let shelf: ScreenshotRecents
    private let name = screenshotLabel(weight: .semibold)
    private let meta = screenshotLabel()
    private let edited = screenshotLabel("Edited", weight: .semibold)
    private let privacy = screenshotLabel()
    private let zoomReadout = screenshotLabel(weight: .semibold)
    private let inspection = screenshotLabel(weight: .semibold)
    private let hint = screenshotLabel()
    private let toastText = screenshotLabel(weight: .semibold)
    private let emptyIcon = NSImageView()
    private let emptyTitle = screenshotLabel("Capture something to mark it up")
    private let emptyMessage = NSTextField(wrappingLabelWithString: "A region, a window, a whole display or a long scrolling page. It opens here, ready for arrows, steps, callouts and redaction.")
    private let newCapture = ScreenshotButton("Capture…", symbol: "viewfinder")
    private let emptyCapture = ScreenshotButton("Capture…", style: .primary)
    private let captureShortcut = screenshotLabel()
    private var shortcutHint: String?
    private let undoButton = ScreenshotButton("Undo", symbol: "arrow.uturn.backward", style: .plain)
    private let redoButton = ScreenshotButton("Redo", symbol: "arrow.uturn.forward", style: .plain)
    private let saveButton = ScreenshotButton("Save PNG…", hint: "⌘S")
    private let copyButton = ScreenshotButton("Copy Image", hint: "⌘C", style: .primary)
    private let openNewCapture = ScreenshotButton("Open New Capture", style: .primary)
    private let shelfToggle = ScreenshotButton("Recent Captures", symbol: "clock", style: .plain)
    private let zoomModes = ScreenshotChoice(["Fit", "100%"], label: "Zoom mode")
    private let zoomOutButton = ScreenshotButton("Zoom out", symbol: "minus")
    private let zoomInButton = ScreenshotButton("Zoom in", symbol: "plus")
    private let size = ScreenshotChoice(ScreenshotMarkSize.allCases.map { $0.rawValue.capitalized }, label: "Mark size")
    private let textSize = ScreenshotChoice(["Regular", "Large"], label: "Text size")
    private let thickness = ScreenshotButton("Thickness", symbol: "lineweight")
    private lazy var thicknessMenu: NSPopover = {
        let popover = NSPopover()
        popover.behavior = .transient
        popover.animates = false
        popover.delegate = self
        return popover
    }()
    private let edit = ScreenshotButton("Edit Text", hint: "↩")
    private let delete = ScreenshotButton("Delete", hint: "⌫")
    private let earlier = ScreenshotButton("Make Earlier")
    private let later = ScreenshotButton("Make Later")
    private let reveal = ScreenshotButton("Selected mark is out of view · Show")
    private let acceptCrop = ScreenshotButton("Crop", hint: "↩", style: .primary)
    private let cancelCrop = ScreenshotButton("Cancel", hint: "esc")
    private var toolButtons: [ScreenshotTool: ScreenshotButton] = [:]
    private var swatches: [ScreenshotButton] = []
    private var entries: [ScreenshotStore.Entry] = []
    // Keep incoming captures in the editor session until the user can switch to them.
    private var pendingCaptures: [ScreenshotDocument] = []
    private var zoom = ScreenshotCanvas.Zoom.fit
    private var autoZoom = true
    private var persisted: ScreenshotEdits?
    private var lastShared: ScreenshotEdits?
    private var persistSoon: DispatchWorkItem?
    private var toastTimer: DispatchWorkItem?
    private var closeConfirmed = false
    private var sheetOpen = false
    private var changingDocument = false
    private var shelfOpen: Bool
    private var sessions: [UUID: EditingSession] = [:]

    private struct EditingSession {
        let edits: ScreenshotEdits
        let revisions: ScreenshotCanvas.Revisions
        let zoom: ScreenshotCanvas.Zoom
        let autoZoom: Bool
        let scrollPosition: CGPoint
    }

    init(store: ScreenshotStore, presentLeaveSheet: @escaping LeaveSheetPresenter = { alert, window, completion in
        alert.beginSheetModal(for: window, completionHandler: completion)
    }) {
        self.store = store
        self.presentLeaveSheet = presentLeaveSheet
        shelfOpen = store.showsShelf
        shelf = ScreenshotRecents(store: store)
        window = ScreenshotEditorWindow(contentRect: CGRect(x: 0, y: 0, width: 1360, height: 810),
                                        styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                                        backing: .buffered, defer: false)
        super.init()
        window.title = "RichGlance Screenshot Editor"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isMovableByWindowBackground = true
        window.minSize = CGSize(width: 690, height: 590)
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.setAccessibilityTitle("RichGlance Screenshot Editor")
        window.intercept = { [weak self] in self?.handle($0) ?? false }
        build()
        refreshShelf()
        refresh()
    }

    func show(activate: Bool = true) {
        if !window.isVisible {
            let visible = (NSScreen.main ?? window.screen)?.visibleFrame ?? window.frame
            window.setContentSize(CGSize(width: min(1360, visible.width - 32), height: min(810, visible.height - 28)))
            window.center()
        }
        if !activate {
            window.orderFront(nil)
            refresh()
            return
        }
        if #available(macOS 14, *) { NSApp.activate() }
        else { NSApp.activate(ignoringOtherApps: true) }
        if NSApp.isActive {
            focus()
        } else {
            // A scrolling result can leave the source app active.
            window.orderFrontRegardless()
            let configuration = NSWorkspace.OpenConfiguration()
            configuration.activates = true
            configuration.addsToRecentItems = false
            NSWorkspace.shared.openApplication(at: Bundle.main.bundleURL, configuration: configuration) { [weak self] _, _ in
                Task { @MainActor in
                    guard let self, self.window.isVisible else { return }
                    self.focus()
                }
            }
        }
        refresh()
    }

    func focus() {
        window.makeKeyAndOrderFront(nil)
        focusPicture()
    }

    private func focusPicture() {
        if let comparison { window.makeFirstResponder(comparison) }
        else if let canvas { window.makeFirstResponder(canvas) }
        else { window.makeFirstResponder(emptyCapture) }
    }

    private func build() {
        window.contentView = root
        for view in [titleBar, rail, work, bottom, shelf] { root.addSubview(view) }
        for view in [titleRule, titleDivider, newCapture, againButton, name, meta, edited, undoButton, redoButton, compareButton, moreButton, saveButton, copyButton] {
            titleBar.addSubview(view)
        }
        rail.addSubview(railRule)
        rail.addSubview(cropRule)
        bottom.addSubview(bottomRule)
        for button in [undoButton, redoButton, zoomInButton, zoomOutButton] { button.iconOnly = true }
        undoButton.toolTip = "Undo (⌘Z)"
        redoButton.toolTip = "Redo (⇧⌘Z)"
        for tool in ScreenshotTool.allCases {
            let button = ScreenshotButton(tool.title, symbol: tool.symbol, style: .tool)
            button.toolTip = "\(tool.title) (\(tool.key.uppercased()))"
            button.onPress = { [weak self] in self?.select(tool) }
            toolButtons[tool] = button
            rail.addSubview(button)
        }
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = true
        scroll.drawsBackground = false
        scroll.contentView.postsBoundsChangedNotifications = true
        scroll.setAccessibilityLabel("Editable screenshot")
        NotificationCenter.default.addObserver(self, selector: #selector(scrolled),
                                               name: NSView.boundsDidChangeNotification, object: scroll.contentView)
        for view in [scroll, inspector, empty, reveal, toast] { work.addSubview(view) }
        for view in [inspection, hint, size, textSize, thickness, edit, delete, earlier, later, acceptCrop, cancelCrop] { inspector.addSubview(view) }
        swatches = ScreenshotInk.allCases.map { ink in
            let button = ScreenshotButton(ink.rawValue.capitalized, style: .swatch)
            button.swatch = ink.color
            button.toolTip = ink.rawValue.capitalized
            button.onPress = { [weak self] in self?.canvas?.setStyle(ink: ink); self?.focusPicture() }
            inspector.addSubview(button)
            return button
        }
        for view in [emptyIcon, emptyTitle, emptyMessage, emptyCapture, captureShortcut] { empty.addSubview(view) }
        emptyIcon.image = NSApp.applicationIconImage
        emptyIcon.imageScaling = .scaleProportionallyUpOrDown
        emptyIcon.setAccessibilityElement(false)
        emptyTitle.font = Self.emptyTitleFont
        emptyTitle.alignment = .center
        emptyMessage.font = .systemFont(ofSize: 16)
        emptyMessage.alignment = .center
        emptyMessage.maximumNumberOfLines = 3
        toast.addSubview(toastText)
        toast.isHidden = true
        for view in [shelfToggle, searchField, searchStatus, openNewCapture, privacy, keepRedactedButton, zoomModes, zoomOutButton, zoomReadout, zoomInButton] { bottom.addSubview(view) }

        newCapture.onPress = { [weak self] in self?.startCapture() }
        againButton.onPress = { [weak self] in self?.captureAgain?() }
        compareButton.onPress = { [weak self] in self?.toggleCompare() }
        moreButton.onPress = { [weak self] in self?.showMore() }
        searchField.delegate = self
        searchField.font = .systemFont(ofSize: 16)
        searchField.placeholderString = "Search text in captures"
        searchField.setAccessibilityLabel("Search text in captures")
        emptyCapture.onPress = newCapture.onPress
        undoButton.onPress = { [weak self] in self?.canvas?.undo(); self?.focusPicture() }
        redoButton.onPress = { [weak self] in self?.canvas?.redo(); self?.focusPicture() }
        copyButton.onPress = { [weak self] in self?.copyImage() }
        saveButton.onPress = { [weak self] in self?.saveImage() }
        shelfToggle.onPress = { [weak self] in self?.toggleShelf() }
        openNewCapture.onPress = { [weak self] in self?.openNextCapture() }
        keepRedactedButton.onPress = { [weak self] in self?.keepCurrentRedacted() }
        zoomModes.changed = { [weak self] in self?.setZoom($0 == 0 ? .fit : .actual) }
        zoomOutButton.onPress = { [weak self] in self?.changeZoom(direction: -1) }
        zoomInButton.onPress = { [weak self] in self?.changeZoom(direction: 1) }
        thickness.trailingSymbol = "chevron.down"
        thickness.onPress = { [weak self] in self?.showThicknessMenu() }
        size.changed = { [weak self] in
            self?.canvas?.setStyle(size: ScreenshotMarkSize.allCases[$0]); self?.focusPicture()
        }
        textSize.changed = { [weak self] in self?.canvas?.setStyle(size: $0 == 1 ? .medium : .small); self?.focusPicture() }
        edit.onPress = { [weak self] in self?.canvas?.startCallout() }
        delete.onPress = { [weak self] in self?.canvas?.deleteSelected(); self?.focusPicture() }
        earlier.onPress = { [weak self] in self?.canvas?.reorderSelected(by: -1); self?.focusPicture() }
        later.onPress = { [weak self] in self?.canvas?.reorderSelected(by: 1); self?.focusPicture() }
        acceptCrop.onPress = { [weak self] in self?.commitCrop() }
        cancelCrop.onPress = { [weak self] in self?.canvas?.cancelCrop(); self?.focusPicture() }
        reveal.onPress = { [weak self] in self?.canvas?.revealSelection(); self?.focusPicture() }
        shelf.open = { [weak self] in self?.open($0) }
        shelf.action = { [weak self] action, entry in self?.perform(action, for: entry) }
        shelf.drag = { [weak self] in self?.makeDrag($0) }
        shelf.remove = { [weak self] in self?.deleteCapture($0) }
        shelf.hide = { [weak self] in self?.setShelf(false); self?.window.makeFirstResponder(self?.shelfToggle) }
        shelf.clearAll = { [weak self] in self?.clearAll() }
        shelf.turnOff = { [weak self] in self?.turnOff() }
        shelf.turnOn = { [weak self] in self?.turnOn() }
        shelf.changeRetention = { [weak self] in self?.changeRetention($0) }
        shelf.focusCanvas = { [weak self] in self?.focusPicture() }
        root.changedAppearance = { [weak self] in self?.refresh() }
    }

    private static let emptyTitleFont: NSFont = {
        if let font = Bundle.main.url(forResource: "Newsreader-Regular", withExtension: "ttf") {
            CTFontManagerRegisterFontsForURL(font as CFURL, .process, nil)
        }
        return NSFont(name: "Newsreader-Regular", size: 34) ?? NSFont(name: "Georgia", size: 34) ?? .systemFont(ofSize: 34)
    }()

    func changeDocument(_ document: ScreenshotDocument, newCapture: Bool = false, baseline: ScreenshotDocument? = nil) {
        if let baseline { incomingBaselines[document.id] = baseline }
        if canvas?.document.id == document.id { focusPicture(); return }
        if newCapture {
            pendingCaptures.append(document)
            if pendingCaptures.count == 1 { openNextCapture() }
            layoutBottom(width: bottom.bounds.width)
        } else {
            leaveCurrent { [weak self] in self?.install(document) }
        }
    }

    private func openNextCapture() {
        guard let document = pendingCaptures.first else { return }
        leaveCurrent { [weak self] in self?.install(document, newCapture: true) }
    }

    private func install(_ document: ScreenshotDocument?, newCapture: Bool = false) {
        if let document { pendingCaptures.removeAll { $0.id == document.id } }
        comparison?.stop()
        comparison?.removeFromSuperview()
        comparison = nil
        shelf.setComparison(before: nil, after: nil)
        changingDocument = true
        persistSoon?.cancel()
        let previous = canvas
        if let previous, previous.document.id != document?.id, entries.contains(where: { $0.id == previous.document.id }) {
            sessions[previous.document.id] = EditingSession(edits: previous.document.edits, revisions: previous.revisions,
                                                            zoom: zoom, autoZoom: autoZoom,
                                                            scrollPosition: scroll.contentView.bounds.origin)
        }
        thicknessMenu.performClose(nil)
        canvas = document.map { ScreenshotCanvas(document: $0, store: store) }
        canvas?.saveStyle = { [weak self] changes, tool in
            guard let self else { return }
            do { try store.rememberAnnotationStyle(changes, for: tool) }
            catch { announce("Could not save the \(tool.title) defaults.") }
        }
        indexedCurrentEdits = canvas?.document.edits
        if let previous, let canvas {
            if previous.tool != .crop { canvas.tool = previous.tool }
        }
        scroll.documentView = canvas
        canvas?.changed = { [weak self] in self?.canvasChanged() }
        persisted = document?.edits
        lastShared = nil
        let previousSession = document.flatMap { sessions.removeValue(forKey: $0.id) }
        let session = previousSession?.edits == document?.edits ? previousSession : nil
        if let session { canvas?.revisions = session.revisions }
        autoZoom = session?.autoZoom ?? true
        zoom = session?.zoom ?? (document?.kind == .scrolling ? .actual : .fit)
        changingDocument = false
        refreshShelf(newCapture: newCapture)
        refresh()
        scroll.contentView.scroll(to: session?.scrollPosition ?? .zero)
        scroll.reflectScrolledClipView(scroll.contentView)
        if let document, let before = incomingBaselines.removeValue(forKey: document.id) {
            beginCompare(before)
            announce("Captured the same area again. Comparing with the \(before.created.formatted(date: .omitted, time: .shortened)) capture.")
        }
        focusPicture()
    }

    func savedDocument(_ id: UUID) -> ScreenshotDocument? {
        if let document = canvas?.document, document.id == id { return document }
        guard let entry = entries.first(where: { $0.id == id }) else { return nil }
        return try? store.load(entry)
    }

    private func toggleCompare() {
        if comparison != nil { endCompare(); return }
        guard let canvas else { return }
        guard let candidate = entries.first(where: { $0.id != canvas.document.id }),
              let before = try? store.load(candidate) else {
            announce("Take another capture to compare it with this one.")
            return
        }
        beginCompare(before)
    }

    private func beginCompare(_ before: ScreenshotDocument) {
        guard let canvas, before.id != canvas.document.id else { return }
        canvas.finishCallout()
        if let comparison { comparison.setBefore(before, entries: entries) }
        else {
            guard let view = ScreenshotCompareView(before: before, after: canvas.document, entries: entries) else {
                announce("These captures could not be opened for comparison.")
                return
            }
            comparison = view
            view.leave = { [weak self] in self?.endCompare() }
            view.choose = { [weak self] id in
                if let self, let document = savedDocument(id) { beginCompare(document) }
            }
            view.zoomChanged = { [weak self] value in self?.zoomReadout.stringValue = "\(value)%" }
            work.addSubview(view, positioned: .below, relativeTo: toast)
        }
        shelf.setComparison(before: before.id, after: canvas.document.id)
        refresh()
        focusPicture()
    }

    private func endCompare() {
        comparison?.stop()
        comparison?.removeFromSuperview()
        comparison = nil
        shelf.setComparison(before: nil, after: nil)
        refresh()
        focusPicture()
    }

    private func open(_ entry: ScreenshotStore.Entry) {
        if let comparison {
            if entry.id != comparison.afterID, let before = try? store.load(entry) { beginCompare(before) }
            return
        }
        guard canvas?.document.id != entry.id else { focusPicture(); return }
        do {
            guard let document = try store.load(entry) else {
                refreshShelf()
                announce("This capture could not be opened.")
                return
            }
            changeDocument(document)
        } catch { announce("This capture could not be opened.") }
    }

    func showShelf() { setShelf(true) }

    func refreshShelf(newCapture: Bool = false) {
        shortcutHint = CaptureShortcutHint.current()
        if canvas == nil { layoutEmpty() }
        do {
            entries = try store.entries()
            let retained = Set(entries.map(\.id))
            sessions = sessions.filter { retained.contains($0.key) }
            cachedText = cachedText.filter { retained.contains($0.key) }
            searchField.isEnabled = store.keepsHistory
            if !store.keepsHistory {
                search?.cancel()
                searchField.stringValue = ""
                searchMatches.removeAll()
                cachedText.removeAll()
                canvas?.textMatches = []
                searchStatus.stringValue = ""
            }
            if searchQuery.isEmpty {
                shelf.update(entries: entries, selected: canvas?.document.id, enabled: store.keepsHistory,
                             retention: store.retention, newCapture: newCapture)
            } else { runSearch() }
        } catch {
            entries = []
            shelf.update(entries: [], selected: canvas?.document.id, enabled: store.keepsHistory,
                         retention: store.retention, unavailable: true)
        }
        let count = entries.count
        shelfToggle.counter = count
        shelfToggle.setAccessibilityLabel("Recent Captures, \(count) \(count == 1 ? "capture" : "captures")")
        shelfToggle.setAccessibilityValue(shelfOpen ? "Expanded" : "Collapsed")
    }

    private var searchQuery: String {
        let text = searchField.currentEditor()?.string ?? searchField.stringValue
        return text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    func controlTextDidChange(_ notification: Notification) {
        if notification.object as? NSSearchField === searchField { runSearch() }
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        guard control === searchField else { return false }
        if commandSelector == #selector(NSResponder.cancelOperation(_:)) {
            textView.string = ""
            searchField.stringValue = ""
            runSearch()
            return true
        }
        if commandSelector == #selector(NSResponder.moveDown(_:)) || commandSelector == #selector(NSResponder.insertNewline(_:)) {
            shelf.focusFirstMatch()
            return true
        }
        return false
    }

    private func runSearch() {
        search?.cancel()
        searchMatches.removeAll()
        canvas?.textMatches = []
        let query = searchQuery
        guard !query.isEmpty, store.keepsHistory else {
            searchStatus.stringValue = ""
            shelf.update(entries: entries, selected: canvas?.document.id, enabled: store.keepsHistory, retention: store.retention)
            layoutBottom(width: bottom.bounds.width)
            return
        }
        searchStatus.stringValue = "Searching…"
        shelf.update(entries: [], selected: canvas?.document.id, enabled: true, retention: store.retention,
                     matches: [:], searching: true)
        let entries = self.entries
        let store = self.store
        let cache = cachedText
        let current = canvas?.document
        let dirty = current?.edits != persisted
        let started = ProcessInfo.processInfo.systemUptime
        search = Task { [weak self] in
            do { try await Task.sleep(for: .milliseconds(100)) } catch { return }
            var indexed: [UUID: TextIndexEntry] = [:]
            var hits: [UUID: [ScreenshotText.Line]] = [:]
            for entry in entries {
                guard !Task.isCancelled else { return }
                let isCurrent = entry.id == current?.id
                let lines: [ScreenshotText.Line]
                if let previous = cache[entry.id], previous.entry == entry, !(isCurrent && dirty) {
                    lines = previous.lines
                    indexed[entry.id] = previous
                } else {
                    let document: ScreenshotDocument?
                    if isCurrent { document = current }
                    else {
                        let loader = Task.detached(priority: .utility) { try? store.load(entry) }
                        document = await loader.value
                    }
                    guard !Task.isCancelled else { return }
                    guard let document,
                          let recognized = try? await ScreenshotText.read(preparing: {
                              ScreenshotText.Input(document: document)
                          }) else { continue }
                    lines = recognized
                    if !(isCurrent && dirty) { indexed[entry.id] = TextIndexEntry(entry: entry, lines: lines) }
                }
                let found = lines.filter { $0.matches(query) }
                if !found.isEmpty { hits[entry.id] = found }
            }
            guard !Task.isCancelled, let self, store.keepsHistory else { return }
            guard searchQuery == query else { runSearch(); return }
            cachedText = indexed
            searchMatches = hits
            let matches = entries.filter { hits[$0.id] != nil }
            let captions = hits.mapValues { $0.first?.text ?? "" }
            searchStatus.stringValue = matches.isEmpty ? "No matches" : "\(matches.count) of \(entries.count) match"
            shelf.update(entries: matches, selected: canvas?.document.id, enabled: true, retention: store.retention, matches: captions)
            if let id = canvas?.document.id { canvas?.textMatches = hits[id]?.map(\.bounds) ?? [] }
            layoutBottom(width: bottom.bounds.width)
            RichFilesDiagnostics.record(.glanceSearch, id: UUID(), outcome: .complete, started: started,
                                        total: entries.count, confirmed: matches.count)
        }
    }

    private func showMore() {
        guard canvas != nil else { return }
        let menu = NSMenu()
        menu.addItem(withTitle: "Copy Text", action: #selector(copyCurrentText), keyEquivalent: "").target = self
        menu.addItem(withTitle: "Pin to Screen", action: #selector(pinCurrentImage), keyEquivalent: "").target = self
        menu.addItem(withTitle: "Attach to RichOS Chat", action: #selector(attachCurrent), keyEquivalent: "").target = self
        if let document = canvas?.document, document.hasRedactions, entries.contains(where: { $0.id == document.id }) {
            menu.addItem(.separator())
            menu.addItem(withTitle: keepRedactedButton.title, action: #selector(keepCurrentRedacted), keyEquivalent: "").target = self
        }
        menu.popUp(positioning: nil, at: CGPoint(x: 0, y: moreButton.bounds.maxY), in: moreButton)
    }

    @objc private func attachCurrent() {
        canvas?.finishCallout()
        if let document = canvas?.document { attach(document) }
    }

    private func attach(_ document: ScreenshotDocument) {
        guard chatSheet == nil, window.attachedSheet == nil else { return }
        guard let image = ScreenshotRenderer.flattened(document) else {
            announce("Could not prepare this capture for attaching.")
            return
        }
        let sheet = ScreenshotChat(parent: window, image: image, name: document.title)
        sheet.closed = { [weak self] in self?.chatSheet = nil }
        chatSheet = sheet
        sheet.show()
    }

    @objc private func keepCurrentRedacted() {
        canvas?.finishCallout()
        if let document = canvas?.document { keepRedacted(document) }
    }

    private func keepRedacted(_ document: ScreenshotDocument) {
        guard document.hasRedactions, entries.contains(where: { $0.id == document.id }) else { return }
        let crop = document.crop
        let count = document.edits.marks.filter { $0.tool == .redact }.count
        let marks = document.edits.marks.count
        confirm("Keep only the redacted copy?",
                detail: "RichGlance will keep one flat picture, \(Int(crop.width)) × \(Int(crop.height)) px, with \(count) redacted \(count == 1 ? "area" : "areas") covered. It will discard the original, \(marks) editable \(marks == 1 ? "mark" : "marks") and this capture’s undo history. This cannot be undone.",
                action: "Discard Original and Keep Redacted Copy", restore: { [weak self] in self?.focusPicture() }) { [weak self] in
            guard let self else { return }
            let started = ProcessInfo.processInfo.systemUptime
            do {
                let clean = try store.keepOnlyRedacted(document)
                discardedOriginal?(document.id)
                copyTextTask?.cancel()
                search?.cancel()
                cachedText.removeValue(forKey: document.id)
                sessions.removeValue(forKey: document.id)
                if canvas?.document.id == document.id { install(clean) }
                else {
                    if comparison?.beforeID == document.id { endCompare() }
                    refreshShelf()
                }
                announce("Holds only the redacted copy.")
                RichFilesDiagnostics.record(.glanceRedactedCopy, id: UUID(), outcome: .complete, started: started,
                                            total: 1, confirmed: 1)
            } catch {
                refreshShelf()
                announce("Could not replace the retained capture. Reopen it to check its current state.")
                RichFilesDiagnostics.record(.glanceRedactedCopy, id: UUID(), outcome: .issues, started: started,
                                            total: 1, confirmed: 0)
            }
        }
    }

    @objc private func pinCurrentImage() {
        canvas?.finishCallout()
        if let document = canvas?.document { pinDocument(document) }
    }

    private func pinDocument(_ document: ScreenshotDocument) {
        if let image = ScreenshotRenderer.flattened(document) { pinImage?(image, document.id) }
        else { announce("This image could not be pinned.") }
    }

    private func makeDrag(_ entry: ScreenshotStore.Entry) -> (URL, NSImage)? {
        if canvas?.document.id == entry.id { canvas?.finishCallout() }
        let start = ProcessInfo.processInfo.systemUptime
        do {
            guard let document = savedDocument(entry.id), let preview = ScreenshotRenderer.thumbnail(document) else {
                throw CocoaError(.fileNoSuchFile)
            }
            let file = try ScreenshotDrag.write(document)
            RichFilesDiagnostics.record(.glanceShare, id: UUID(), outcome: .complete, started: start, total: 1, confirmed: 1)
            return (file, NSImage(cgImage: preview, size: .zero))
        } catch {
            announce("Could not prepare this capture for dragging.")
            RichFilesDiagnostics.record(.glanceShare, id: UUID(), outcome: .issues, started: start, total: 1, confirmed: 0)
            return nil
        }
    }

    private func perform(_ action: ScreenshotShelfAction, for entry: ScreenshotStore.Entry) {
        if canvas?.document.id == entry.id { canvas?.finishCallout() }
        guard let document = savedDocument(entry.id) else { announce("This capture is no longer available."); return }
        switch action {
        case .text: copyText(document)
        case .pin: pinDocument(document)
        case .attach: attach(document)
        case .compare: beginCompare(document)
        case .keepRedacted: keepRedacted(document)
        }
    }

    @objc private func copyCurrentText() {
        canvas?.finishCallout()
        if let document = canvas?.document { copyText(document) }
    }

    private func copyText(_ document: ScreenshotDocument) {
        guard let input = ScreenshotText.Input(document: document) else { announce("Could not read this capture."); return }
        copyTextTask?.cancel()
        let started = ProcessInfo.processInfo.systemUptime
        copyTextTask = Task { [weak self] in
            let lines = try? await ScreenshotText.read(input)
            guard let self, !Task.isCancelled, window.isVisible else { return }
            if let lines, ScreenshotText.copy(lines) {
                announce("Copied \(lines.count) \(lines.count == 1 ? "line" : "lines") of text")
                RichFilesDiagnostics.record(.glanceOCR, id: UUID(), outcome: .complete,
                                            started: started, total: 1, confirmed: 1)
            } else {
                announce(lines == nil ? "Could not read text in this capture." : "No text found in this capture.")
                RichFilesDiagnostics.record(.glanceOCR, id: UUID(), outcome: lines == nil ? .issues : .unavailable,
                                            started: started, total: 1, confirmed: 0)
            }
        }
    }

    private func setShelf(_ open: Bool) {
        guard shelfOpen != open else { return }
        do {
            try store.setShelfVisible(open)
            shelfOpen = open
            refresh()
            if open { refreshShelf() }
        } catch { announce("Could not save the shelf setting.") }
    }

    private func toggleShelf() {
        setShelf(!shelfOpen)
        window.makeFirstResponder(shelfToggle)
    }

    private func changeRetention(_ value: ScreenshotStore.Retention) {
        guard value != store.retention else { return }
        do {
            try store.setRetention(value)
            refreshShelf()
            announce(value == .week ? "Captures are kept on this Mac for 7 days."
                                    : "Captures are kept on this Mac forever, until you delete them.")
        } catch {
            refreshShelf()
            announce("Could not change how long captures are kept.")
        }
    }

    private func turnOn() {
        do {
            try store.setHistory(true)
            refreshShelf()
            announce("Recent Captures is on. Each capture stays on this Mac \(store.retention.duration).")
            focusPicture()
        } catch { announce("Could not turn on Recent Captures.") }
    }

    private func deleteCapture(_ entry: ScreenshotStore.Entry) {
        do {
            let selectedIndex = entries.firstIndex { $0.id == entry.id }
            try store.remove(entry.id)
            if canvas?.document.id == entry.id {
                let remaining = try store.entries()
                if let selectedIndex, !remaining.isEmpty {
                    let replacement = try store.load(remaining[min(selectedIndex, remaining.count - 1)])
                    install(replacement)
                } else { install(nil) }
            } else { refreshShelf() }
            announce("Capture deleted from this Mac.")
        } catch { announce("Could not delete this capture.") }
    }

    private func clearAll() {
        let count = entries.count
        guard count > 0 else { return }
        confirm("Delete all \(count) recent captures?",
                detail: "Their originals and marks are removed from this Mac. Pictures you already copied or saved are not affected.",
                action: "Delete All", restore: { [weak self] in self?.shelf.focusClear() }) { [weak self] in
            guard let self else { return }
            do { try store.clear(); install(nil) }
            catch { announce("Could not clear Recent Captures.") }
        }
    }

    private func turnOff() {
        let count = entries.count
        let detail = "The \(count) \(count == 1 ? "capture" : "captures") kept on this Mac will be deleted. The capture you have open stays open until you close it.\n\nFrom then on a capture is not kept after you close it."
        confirm("Turn off Recent Captures?", detail: detail, action: "Turn Off and Delete",
                restore: { [weak self] in self?.shelf.focusOptions() }) { [weak self] in
            guard let self else { return }
            do { try store.setHistory(false); refreshShelf(); focusPicture() }
            catch { announce("Could not turn off Recent Captures.") }
        }
    }

    private func confirm(_ title: String, detail: String, action: String, restore: @escaping () -> Void,
                         perform: @escaping () -> Void) {
        guard !sheetOpen else { return }
        canvas?.finishCallout()
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = detail
        alert.addButton(withTitle: "Cancel")
        alert.addButton(withTitle: action).hasDestructiveAction = true
        sheetOpen = true
        alert.beginSheetModal(for: window) { [weak self] response in
            self?.sheetOpen = false
            if response == .alertSecondButtonReturn { perform() }
            else { restore() }
        }
    }

    private func startCapture() {
        canvas?.finishCallout()
        _ = saveEditsNow()
        capture?()
    }

    private func canvasChanged() {
        guard !changingDocument else { return }
        if indexedCurrentEdits != canvas?.document.edits {
            indexedCurrentEdits = canvas?.document.edits
            if let id = canvas?.document.id { cachedText.removeValue(forKey: id) }
            copyTextTask?.cancel()
            if !searchQuery.isEmpty { runSearch() }
        }
        refresh()
        guard let canvas, !canvas.isInteracting, canvas.document.edits != persisted else { return }
        persistSoon?.cancel()
        let task = DispatchWorkItem { [weak self] in
            guard let self else { return }
            if saveEditsNow() { refreshShelf() }
        }
        persistSoon = task
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2, execute: task)
    }

    @discardableResult
    private func saveEditsNow() -> Bool {
        persistSoon?.cancel()
        persistSoon = nil
        guard let canvas else { return true }
        let document = canvas.document
        guard entries.contains(where: { $0.id == document.id }), store.keepsHistory else { return false }
        if document.edits == persisted { return true }
        let started = ProcessInfo.processInfo.systemUptime
        do {
            guard try store.retain(document, existingOnly: true) else { return false }
            persisted = document.edits
            window.isDocumentEdited = false
            record(.complete, for: document, from: started)
            return true
        } catch {
            record(.issues, for: document, from: started)
            announce("Could not keep changes in Recent Captures.")
            return false
        }
    }

    private func leaveCurrent(_ completion: @escaping () -> Void) {
        guard !sheetOpen else { return }
        guard let canvas else { completion(); return }
        canvas.finishCallout()
        if saveEditsNow() || !canvas.hasChanges || lastShared == canvas.document.edits { completion(); return }
        askBeforeLeaving("Switch without keeping your edits?", completion: completion)
    }

    private func askBeforeLeaving(_ title: String, closing: Bool = false, completion: @escaping () -> Void) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = canvas?.document.sourceFile != nil
            ? "Edits to opened images are kept in this window. You can copy the edited image before leaving."
            : "Recent Captures is off or this screenshot could not be kept. You can copy the edited image before leaving."
        alert.addButton(withTitle: "Cancel")
        alert.addButton(withTitle: closing ? "Close" : "Discard Changes").hasDestructiveAction = true
        alert.addButton(withTitle: closing ? "Copy Image and Close" : "Copy Image and Continue")
        sheetOpen = true
        presentLeaveSheet(alert, window) { [weak self] response in
            guard let self else { return }
            sheetOpen = false
            if response == .alertThirdButtonReturn, copyFlattened() { completion() }
            else if response == .alertSecondButtonReturn { completion() }
            else { focusPicture() }
        }
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        guard !sheetOpen else { return false }
        canvas?.finishCallout()
        if closeConfirmed || saveEditsNow() || canvas?.hasChanges != true { return true }
        if let document = canvas?.document, document.sourceFile != nil, lastShared == document.edits { return true }
        askBeforeLeaving("Close without keeping your edits?", closing: true) { [weak self] in
            guard let self else { return }
            closeConfirmed = true
            window.performClose(nil)
        }
        return false
    }

    var restartDocument: UUID? { canvas?.document.id }

    var canRestartAfterUpdate: Bool {
        guard !sheetOpen, pendingCaptures.isEmpty, comparison == nil else { return false }
        guard let canvas else { return true }
        return !canvas.isInteracting && canvas.previewCrop == nil && canvas.document.edits == persisted
            && store.keepsHistory && entries.contains(where: { $0.id == canvas.document.id })
    }

    func allowApplicationQuit() -> Bool {
        if windowShouldClose(window) { return true }
        window.makeKeyAndOrderFront(nil)
        return false
    }

    func windowWillClose(_ notification: Notification) { closed?() }
    func windowDidResize(_ notification: Notification) { refresh() }
    func windowDidChangeBackingProperties(_ notification: Notification) { layoutCanvas() }

    func copyImage() {
        guard !sheetOpen else { return }
        canvas?.finishCallout()
        if canvas?.previewCrop != nil { canvas?.commitCrop() }
        _ = copyFlattened()
        focusPicture()
    }

    private func copyFlattened() -> Bool {
        guard let document = canvas?.document else { return false }
        let started = ProcessInfo.processInfo.systemUptime
        guard let image = ScreenshotRenderer.flattened(document), let png = ScreenshotRenderer.png(image),
              ScreenshotRenderer.copy(png) else {
            announce("Could not copy the picture. Try again.")
            record(.issues, for: document, from: started)
            return false
        }
        lastShared = document.edits
        announce("Copied \(image.width) × \(image.height) px as one flat picture")
        record(.complete, for: document, from: started)
        return true
    }

    func saveImage() {
        guard !sheetOpen, let canvas else { return }
        canvas.finishCallout()
        let document = canvas.document
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.png]
        let date = DateFormatter()
        date.dateFormat = "yyyy-MM-dd 'at' HH.mm.ss"
        let filename = document.title.replacingOccurrences(of: "/", with: " ").replacingOccurrences(of: ":", with: " ")
        if let sourceFile = document.sourceFile {
            panel.directoryURL = sourceFile.deletingLastPathComponent()
            panel.nameFieldStringValue = "\(filename) Edited.png"
            panel.delegate = self
        } else {
            panel.nameFieldStringValue = "\(filename) \(date.string(from: document.created)).png"
        }
        sheetOpen = true
        panel.beginSheetModal(for: window) { [weak self] response in
            guard let self else { return }
            sheetOpen = false
            window.makeFirstResponder(saveButton)
            guard response == .OK, let url = panel.url else { return }
            let started = ProcessInfo.processInfo.systemUptime
            do {
                guard let image = ScreenshotRenderer.flattened(document), let data = ScreenshotRenderer.png(image) else {
                    throw CocoaError(.fileWriteUnknown)
                }
                guard !document.isSourceFile(url) else { throw CocoaError(.fileWriteFileExists) }
                try data.write(to: url, options: .atomic)
                lastShared = document.edits
                announce("Saved \(image.width) × \(image.height) px as one flat picture")
                record(.complete, for: document, from: started)
            } catch {
                announce("Could not save the PNG. Choose a different location and try again.")
                record(.issues, for: document, from: started)
            }
        }
    }

    func panel(_ sender: Any, validate url: URL) throws {
        if canvas?.document.isSourceFile(url) == true {
            throw CocoaError(.fileWriteFileExists, userInfo: [
                NSLocalizedDescriptionKey: "Choose a different name for the edited image.",
                NSLocalizedRecoverySuggestionErrorKey: "Save the edited image as a separate PNG."
            ])
        }
    }

    private func announce(_ message: String) {
        toastText.stringValue = message
        toast.isHidden = false
        layoutToast()
        NSAccessibility.post(element: toastText, notification: .announcementRequested,
                             userInfo: [.announcement: message, .priority: NSAccessibilityPriorityLevel.medium.rawValue])
        toastTimer?.cancel()
        let task = DispatchWorkItem { [weak self] in self?.toast.isHidden = true }
        toastTimer = task
        DispatchQueue.main.asyncAfter(deadline: .now() + 3.2, execute: task)
    }

    private func record(_ outcome: RichFilesDiagnostics.Outcome, for document: ScreenshotDocument, from start: TimeInterval) {
        RichFilesDiagnostics.record(.glanceCaptureOutput, id: document.id, outcome: outcome,
                                    started: start, total: 1, confirmed: outcome == .complete ? 1 : 0)
    }

    private func select(_ tool: ScreenshotTool) {
        guard let canvas else { return }
        thicknessMenu.performClose(nil)
        canvas.finishCallout()
        canvas.selected = nil
        canvas.tool = tool
        focusPicture()
    }

    private func showThicknessMenu() {
        guard let canvas, !thickness.isHidden else { return }
        if thicknessMenu.isShown {
            thicknessMenu.performClose(nil)
            return
        }

        let choices = ScreenshotMark.thicknessOptions
        let width: CGFloat = 236, rowHeight: CGFloat = 38, padding: CGFloat = 6
        let frame = CGRect(x: 0, y: 0, width: width, height: rowHeight * CGFloat(choices.count) + 2 * padding)
        let panel = ScreenshotSurface(.floating)
        panel.frame = frame
        for (index, value) in choices.enumerated() {
            let row = ScreenshotButton("\(value) px", style: .segment)
            row.sampleWidth = CGFloat(value)
            row.isSelected = canvas.thickness == CGFloat(value)
            row.frame = CGRect(x: padding, y: padding + CGFloat(index) * rowHeight,
                               width: width - 2 * padding, height: rowHeight)
            row.onHover = { [weak canvas] hovering in
                if hovering { canvas?.previewThickness = value }
                else if canvas?.previewThickness == value { canvas?.previewThickness = nil }
            }
            row.onPress = { [weak self, weak canvas] in
                guard let self, let canvas else { return }
                thicknessMenu.performClose(nil)
                canvas.setStyle(thickness: value)
                focusPicture()
            }
            panel.addSubview(row)
        }
        let controller = NSViewController()
        controller.view = panel
        thicknessMenu.contentViewController = controller
        thicknessMenu.contentSize = frame.size
        thicknessMenu.show(relativeTo: thickness.bounds, of: thickness, preferredEdge: .maxY)
    }

    func popoverWillClose(_ notification: Notification) {
        canvas?.previewThickness = nil
    }

    private func commitCrop() {
        guard let canvas, let crop = canvas.previewCrop, !crop.isEmpty else { return }
        canvas.commitCrop()
        announce("Cropped to \(Int(crop.width)) × \(Int(crop.height)) px. The whole capture is kept; Crop brings it back.")
        focusPicture()
    }

    private func setZoom(_ value: ScreenshotCanvas.Zoom) {
        if let comparison {
            if case .actual = value { comparison.setActualSize(true) }
            else { comparison.setActualSize(false) }
            focusPicture()
            return
        }
        autoZoom = false
        zoom = value
        layoutCanvas()
        focusPicture()
    }

    private func changeZoom(direction: Int) {
        if let comparison { comparison.changeZoom(direction); focusPicture(); return }
        guard let canvas else { return }
        let current = canvas.factor * window.backingScaleFactor
        let steps: [CGFloat] = [0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1, 1.5, 2, 3, 4]
        let next = direction > 0 ? steps.first { $0 > current + 0.005 } : steps.last { $0 < current - 0.005 }
        guard let next else { return }
        setZoom(next == 1 ? .actual : .custom(next))
    }

    private func handle(_ event: NSEvent) -> Bool {
        guard !sheetOpen else { return false }
        let flags = event.modifierFlags.intersection([.command, .option, .shift, .control])
        let key = event.charactersIgnoringModifiers?.lowercased() ?? ""
        if canvas?.isTyping == true { return false }
        if flags == [.command, .option], event.keyCode == 15 {
            toggleShelf()
            return true
        }
        if flags == .command, key == "f", searchField.isEnabled {
            setShelf(true)
            window.makeFirstResponder(searchField)
            searchField.selectText(nil)
            return true
        }
        if let field = searchField.currentEditor(), field === window.firstResponder { return false }
        if flags == .option, key == "t" { copyCurrentText(); return true }
        if flags == .option, key == "p" { pinCurrentImage(); return true }
        if flags == .option, key == "a" { attachCurrent(); return true }
        if flags == .option, key == "k" { toggleCompare(); return true }
        if shelfOpen, shelf.handle(event) { return true }
        if let comparison, comparison.handle(event) { return true }
        guard let canvas else { return false }
        switch (key, flags) {
        case ("z", .command): canvas.undo(); focusPicture(); return true
        case ("z", [.command, .shift]): canvas.redo(); focusPicture(); return true
        case ("c", .command): copyImage(); return true
        case ("s", .command): saveImage(); return true
        case ("0", .command): setZoom(.fit); return true
        case ("1", .command): setZoom(.actual); return true
        case ("-", []), ("=", []):
            if (canvas.selectedMark?.tool ?? canvas.tool).hasThickness {
                canvas.stepThickness(by: key == "-" ? -1 : 1)
                return true
            }
        case ("[", []), ("]", []):
            if let index = entries.firstIndex(where: { $0.id == canvas.document.id }) {
                let next = index + (key == "[" ? 1 : -1)
                if entries.indices.contains(next) { open(entries[next]) }
            }
            return true
        default: break
        }
        guard flags.isEmpty || flags == .shift else { return false }
        if let tool = ScreenshotTool.allCases.first(where: { $0.key == key }) { select(tool); return true }
        guard window.firstResponder === canvas else { return false }
        let step: CGFloat = flags.contains(.shift) ? 10 : 1
        switch event.keyCode {
        case 123 where canvas.selectedMark != nil || canvas.previewCrop != nil: canvas.nudge(dx: -step, dy: 0)
        case 124 where canvas.selectedMark != nil || canvas.previewCrop != nil: canvas.nudge(dx: step, dy: 0)
        case 125 where canvas.selectedMark != nil || canvas.previewCrop != nil: canvas.nudge(dx: 0, dy: step)
        case 126 where canvas.selectedMark != nil || canvas.previewCrop != nil: canvas.nudge(dx: 0, dy: -step)
        case 48 where canvas.previewCrop == nil && !canvas.document.edits.marks.isEmpty:
            canvas.selectNext(backwards: flags.contains(.shift))
        case 51, 117: canvas.deleteSelected()
        case 53:
            if canvas.previewCrop != nil { canvas.cancelCrop() }
            else { canvas.selected = nil; canvas.refresh() }
        case 36, 76:
            if canvas.previewCrop != nil { commitCrop() }
            else { canvas.startCallout() }
        default: return false
        }
        return true
    }

    private func refresh() {
        let palette = ScreenshotEditorPalette(root)
        name.textColor = palette.ink
        meta.textColor = palette.muted
        edited.textColor = palette.gold
        privacy.textColor = palette.muted
        inspection.textColor = palette.ink
        hint.textColor = palette.muted
        emptyTitle.textColor = palette.ink
        emptyMessage.textColor = palette.muted
        toastText.textColor = palette.ink
        zoomReadout.textColor = palette.ink
        zoomReadout.font = .monospacedDigitSystemFont(ofSize: 16, weight: .semibold)
        let document = canvas?.document
        let mark = canvas?.selectedMark
        let tool = canvas?.tool ?? .select
        name.stringValue = document?.title ?? "RichGlance"
        if let document {
            let time = Calendar.current.isDateInToday(document.created)
                ? "Today " + document.created.formatted(date: .omitted, time: .shortened)
                : document.created.formatted(.dateTime.weekday(.abbreviated).hour().minute())
            meta.stringValue = "\(document.kind.title) · \(time)"
        } else {
            meta.stringValue = "Screenshot editor"
        }
        let hasEdits = canvas?.hasChanges == true
        window.isDocumentEdited = canvas?.document.edits != persisted
        let isRedactedCopy = document?.redactedGeneration != nil
        edited.stringValue = isRedactedCopy ? "Redacted copy" : "Edited"
        edited.isHidden = !hasEdits && !isRedactedCopy
        for (item, button) in toolButtons {
            button.isEnabled = document != nil && comparison == nil
            button.isSelected = document != nil && item == tool
        }
        for control in [copyButton, saveButton, zoomInButton, zoomOutButton] + zoomModes.buttons {
            control.isEnabled = document != nil
        }
        compareButton.isEnabled = document != nil
        moreButton.isEnabled = document != nil
        compareButton.isSelected = comparison != nil
        undoButton.isEnabled = comparison == nil && canvas?.canUndo == true
        redoButton.isEnabled = comparison == nil && canvas?.canRedo == true
        empty.isHidden = document != nil
        inspector.isHidden = document == nil || comparison != nil
        scroll.isHidden = document == nil || comparison != nil
        shelf.isHidden = !shelfOpen
        scrolled()
        shelfToggle.symbol = "clock"
        shelfToggle.trailingSymbol = shelfOpen ? "chevron.down" : "chevron.up"
        shelfToggle.counter = entries.count
        shelfToggle.title = "Recent Captures"
        shelfToggle.setAccessibilityValue(shelfOpen ? "Expanded" : "Collapsed")
        if let document {
            privacy.stringValue = document.hasRedactions
                ? "Copy and Save make one flat picture with the redacted pixels replaced. The original stays only on this Mac."
                : "Copy and Save make one flat picture of what is inside the frame. Your marks and the original stay on this Mac."
        } else {
            privacy.stringValue = "Captures stay on this Mac."
        }
        if document?.redactedGeneration != nil, document?.hasRedactions == false {
            privacy.stringValue = "Holds only the redacted copy."
        }
        privacy.toolTip = privacy.stringValue

        let cropping = canvas?.previewCrop != nil
        let selectedTool = mark?.tool ?? tool
        let styled = !cropping && selectedTool.defaultInk != nil
        for (button, color) in zip(swatches, ScreenshotInk.allCases) {
            button.isHidden = !styled
            button.isSelected = (mark?.ink ?? canvas?.ink) == color
        }
        thickness.isHidden = !styled || !selectedTool.hasThickness
        if !thickness.isHidden, let canvas {
            let width = "\(canvas.thickness.formatted()) px"
            thickness.title = "Thickness · " + width
            thickness.setAccessibilityValue(width)
        }
        let chosenSize = mark?.markSize ?? canvas?.size ?? .small
        size.isHidden = !styled || !selectedTool.hasSizeChoice || selectedTool == .text
        textSize.isHidden = !styled || selectedTool != .text
        if !size.isHidden {
            size.selection = ScreenshotMarkSize.allCases.firstIndex(of: chosenSize) ?? 0
            size.setAccessibilityLabel(selectedTool == .arrow ? "Arrow size" : "Step size")
            // Arrow: a bar half the shaft (3, 5, 12 pt). Steps: a dot (6, 9, 14 pt). As in round 108.
            let samples: [ScreenshotButton.SizeSample] = selectedTool == .arrow
                ? [.bar(3), .bar(5), .bar(12)] : [.dot(6), .dot(9), .dot(14)]
            for (button, sample) in zip(size.buttons, samples) { button.sizeSample = sample }
        }
        textSize.selection = chosenSize == .small ? 0 : 1
        edit.isHidden = cropping || mark?.tool != .text
        delete.isHidden = cropping || mark == nil
        earlier.isHidden = cropping || mark?.tool != .steps
        later.isHidden = earlier.isHidden
        acceptCrop.isHidden = !cropping
        cancelCrop.isHidden = !cropping
        let steps = document?.edits.marks.filter { $0.tool == .steps }.count ?? 0
        if let crop = canvas?.previewCrop {
            inspection.stringValue = "Crop · \(Int(crop.width)) × \(Int(crop.height)) px"
            hint.stringValue = "· drag the handles, then press Return"
            acceptCrop.isEnabled = !crop.isEmpty
        } else if let mark {
            hint.stringValue = ""
            if let index = document?.edits.number(for: mark) {
                inspection.stringValue = "Step \(index) of \(steps)"
                earlier.isEnabled = index > 1
                later.isEnabled = index < steps
            } else if mark.tool == .line {
                inspection.stringValue = "Line · \(Int(mark.length.rounded())) px · \(mark.angle)°"
                if canvas?.isInteracting == true {
                    hint.stringValue = canvas?.isSnappingLine == true ? "· snapped" : "· Shift snaps"
                }
            } else if mark.tool == .box {
                inspection.stringValue = "Box · \(Int(mark.rect.width)) × \(Int(mark.rect.height)) px"
            } else if mark.tool == .redact {
                inspection.stringValue = "Redaction · \(Int(mark.rect.integral.width)) × \(Int(mark.rect.integral.height)) px"
                hint.stringValue = "· resize it until nothing shows"
            } else {
                inspection.stringValue = mark.tool == .text ? "Text callout" : mark.tool.title
            }
        } else {
            inspection.stringValue = tool.title
            let instructions: String
            switch tool {
            case .select: instructions = "click a mark to choose it, drag it to move it"
            case .arrow: instructions = "drag from the tail to the point"
            case .text: instructions = "click to add a callout, then type"
            case .line: instructions = "drag to draw · Shift snaps"
            case .box: instructions = "drag around what you want to show"
            case .highlight: instructions = "drag across what you want to highlight"
            case .steps: instructions = "click to place step \(steps + 1)"
            case .redact: instructions = "drag over what must not be seen"
            case .crop: instructions = "drag the handles, then press Return"
            }
            hint.stringValue = "· " + instructions
        }
        layout()
    }

    private func layout() {
        let width = root.bounds.width, height = root.bounds.height
        guard width > 0, height > 246 else { return }
        let shelfHeight: CGFloat = shelfOpen ? 142 : 0
        let workHeight = height - 58 - 46 - shelfHeight
        titleBar.frame = CGRect(x: 0, y: 0, width: width, height: 58)
        rail.frame = CGRect(x: 0, y: 58, width: 92, height: workHeight)
        work.frame = CGRect(x: 92, y: 58, width: width - 92, height: workHeight)
        bottom.frame = CGRect(x: 0, y: work.frame.maxY, width: width, height: 46)
        shelf.frame = CGRect(x: 0, y: bottom.frame.maxY, width: width, height: shelfHeight)
        layoutTitle(width: width)
        layoutBottom(width: width)
        titleRule.frame = CGRect(x: 0, y: 57, width: width, height: 1)
        titleDivider.frame = CGRect(x: saveButton.frame.minX - 8, y: 16, width: 1, height: 26)
        railRule.frame = CGRect(x: 91, y: 0, width: 1, height: workHeight)
        bottomRule.frame = CGRect(x: 0, y: 0, width: width, height: 1)
        let tile = min(58, (workHeight - 35) / CGFloat(ScreenshotTool.allCases.count))
        for (index, tool) in ScreenshotTool.allCases.enumerated() {
            toolButtons[tool]?.frame = CGRect(x: 8, y: 7 + CGFloat(index) * (tile + 2) + (tool == .crop ? 7 : 0),
                                              width: 76, height: tile)
        }
        cropRule.frame = CGRect(x: 14, y: (toolButtons[.crop]?.frame.minY ?? 0) - 4, width: 64, height: 1)
        layoutInspector()
        let y = inspector.isHidden ? 0 : inspector.frame.maxY + 4
        scroll.frame = CGRect(x: 0, y: y, width: work.bounds.width, height: max(0, workHeight - y))
        empty.frame = work.bounds
        comparison?.frame = work.bounds
        layoutEmpty()
        reveal.frame = CGRect(x: max(4, work.bounds.width - reveal.naturalWidth() - 20),
                              y: y + 9, width: reveal.naturalWidth(), height: 34)
        layoutToast()
        layoutCanvas()
    }

    private func layoutTitle(width: CGFloat) {
        let narrow = width <= 900
        newCapture.iconOnly = narrow
        againButton.iconOnly = narrow
        compareButton.iconOnly = narrow
        moreButton.iconOnly = narrow
        saveButton.keyHint = narrow ? nil : "⌘S"
        copyButton.keyHint = narrow ? nil : "⌘C"
        newCapture.frame = CGRect(x: 87, y: 10, width: newCapture.naturalWidth(), height: 38)
        againButton.frame = CGRect(x: newCapture.frame.maxX + 8, y: 10, width: againButton.naturalWidth(), height: 38)
        var right = width - 12
        for button in [copyButton, saveButton, moreButton, compareButton, redoButton, undoButton] {
            right -= button.naturalWidth()
            button.frame = CGRect(x: right, y: 9, width: button.naturalWidth(), height: 40)
            right -= button === saveButton ? 17 : 8
        }
        let left = againButton.frame.maxX + 10
        let total = max(0, right - left)
        let editedWidth: CGFloat = edited.isHidden ? 0 : labelWidth(edited)
        meta.isHidden = narrow
        let metaWidth = narrow ? 0 : min(labelWidth(meta), total * 0.43)
        let nameWidth = min(labelWidth(name), max(0, total - editedWidth - metaWidth))
        let x = narrow ? left : left + max(0, (total - nameWidth - metaWidth - editedWidth) / 2)
        name.frame = CGRect(x: x, y: 19, width: nameWidth, height: 22)
        name.toolTip = name.stringValue
        meta.frame = CGRect(x: name.frame.maxX + 1, y: 19, width: metaWidth, height: 22)
        edited.frame = CGRect(x: meta.frame.maxX, y: 19, width: editedWidth, height: 22)
    }

    private func layoutBottom(width: CGFloat) {
        shelfToggle.frame = CGRect(x: 10, y: 6, width: shelfToggle.naturalWidth(), height: 34)
        zoomInButton.frame = CGRect(x: width - 42, y: 7, width: 32, height: 32)
        zoomReadout.frame = CGRect(x: width - 111, y: 13, width: 64, height: 22)
        zoomReadout.alignment = .center
        zoomOutButton.frame = CGRect(x: width - 147, y: 7, width: 32, height: 32)
        zoomModes.frame = CGRect(x: width - 263, y: 5, width: 110, height: 36)
        openNewCapture.isHidden = pendingCaptures.isEmpty
        let searchX = shelfToggle.frame.maxX + 8
        let searchWidth = min(270, max(0, zoomModes.frame.minX - searchX - 10) * 0.72)
        searchField.frame = CGRect(x: searchX, y: 7, width: searchWidth, height: 32)
        searchStatus.textColor = ScreenshotEditorPalette(root).muted
        searchStatus.frame = CGRect(x: searchField.frame.maxX + 8, y: 13,
                                    width: min(145, max(0, zoomModes.frame.minX - searchField.frame.maxX - 14)), height: 22)
        searchStatus.isHidden = searchQuery.isEmpty || !openNewCapture.isHidden
        let nextX = searchStatus.isHidden ? searchField.frame.maxX + 8 : searchStatus.frame.maxX + 8
        let nextWidth = min(openNewCapture.naturalWidth(), max(0, zoomModes.frame.minX - nextX - 6))
        openNewCapture.frame = CGRect(x: nextX, y: 6, width: nextWidth, height: 34)
        let privacyX = (openNewCapture.isHidden ? nextX : openNewCapture.frame.maxX) + 8
        let available = max(0, width - 271 - privacyX)
        let canKeepRedacted = canvas.map { value in
            value.document.hasRedactions && entries.contains(where: { $0.id == value.document.id })
        } ?? false
        keepRedactedButton.isHidden = !canKeepRedacted || available < keepRedactedButton.naturalWidth()
        keepRedactedButton.frame = CGRect(x: privacyX, y: 6, width: keepRedactedButton.naturalWidth(), height: 34)
        privacy.isHidden = width <= 900 || !keepRedactedButton.isHidden
        privacy.frame = CGRect(x: privacyX, y: 13, width: available, height: 22)
    }

    private func labelWidth(_ field: NSTextField) -> CGFloat {
        let font = field.font ?? .systemFont(ofSize: 16)
        return ceil((field.stringValue as NSString).size(withAttributes: [.font: font]).width) + 12
    }

    private func layoutInspector() {
        guard !inspector.isHidden else { return }
        let available = work.bounds.width - 28
        hint.isHidden = work.bounds.width <= 700 || hint.stringValue.isEmpty
        for button in [edit, delete, cancelCrop, acceptCrop] {
            if work.bounds.width <= 700 { button.keyHint = nil }
        }
        if work.bounds.width > 700 {
            edit.keyHint = "↩"; delete.keyHint = "⌫"; cancelCrop.keyHint = "esc"; acceptCrop.keyHint = "↩"
        }

        var groups: [[(NSView, CGFloat)]] = [[(inspection, labelWidth(inspection))]]
        if !hint.isHidden { groups[0].append((hint, labelWidth(hint))) }
        let colors = swatches.filter { !$0.isHidden }.map { ($0 as NSView, CGFloat(34)) }
        if !colors.isEmpty { groups.append(colors) }
        if !size.isHidden { groups.append([(size, size.naturalWidth)]) }
        if !textSize.isHidden { groups.append([(textSize, textSize.naturalWidth)]) }
        if !thickness.isHidden { groups.append([(thickness, thickness.naturalWidth())]) }
        for button in [earlier, later, edit, delete, cancelCrop, acceptCrop] where !button.isHidden {
            groups.append([(button, button.naturalWidth())])
        }
        let padding: CGFloat = 10
        var x = padding, y: CGFloat = 6, right: CGFloat = 0
        for group in groups {
            let needed = group.reduce(CGFloat(0)) { $0 + $1.1 + 4 } - 4
            if x > padding, x + needed + padding > available {
                x = padding
                y += 40
            }
            for (view, width) in group {
                let isLabel = view is NSTextField
                view.frame = CGRect(x: x, y: y + (isLabel ? 7 : 0),
                                    width: min(width, max(0, available - x - padding)), height: isLabel ? 21 : 34)
                right = max(right, view.frame.maxX)
                x += width + 4
            }
            x += 6
        }
        let total = min(available, right + padding)
        inspector.frame = CGRect(x: (work.bounds.width - total) / 2, y: 10, width: total, height: y + 40)
    }

    private func layoutEmpty() {
        let center = CGPoint(x: empty.bounds.midX, y: empty.bounds.midY)
        let width = max(1, min(520, empty.bounds.width - 32))
        emptyIcon.frame = CGRect(x: center.x - 42, y: max(6, center.y - 129), width: 84, height: 84)
        emptyTitle.frame = CGRect(x: center.x - width / 2, y: emptyIcon.frame.maxY + 12, width: width, height: 45)
        emptyMessage.frame = CGRect(x: center.x - width / 2, y: emptyTitle.frame.maxY + 5, width: width, height: 46)
        captureShortcut.stringValue = shortcutHint.map { "or press \($0) in any app" } ?? ""
        captureShortcut.textColor = ScreenshotEditorPalette(empty).muted
        let hintWidth = captureShortcut.intrinsicContentSize.width
        let showShortcut = shortcutHint != nil && width >= hintWidth + 146
        captureShortcut.isHidden = !showShortcut
        let x = center.x - (showShortcut ? (116 + 14 + hintWidth) / 2 : 58)
        let y = emptyMessage.frame.maxY + 12
        emptyCapture.frame = CGRect(x: x, y: y, width: 116, height: 40)
        captureShortcut.frame = CGRect(x: x + 130, y: y + 10, width: hintWidth + 4, height: 22)
    }

    private func layoutToast() {
        let width = max(1, min(work.bounds.width - 24, toastText.intrinsicContentSize.width + 44))
        toast.frame = CGRect(x: (work.bounds.width - width) / 2, y: work.bounds.height - 62, width: width, height: 44)
        toastText.frame = CGRect(x: 21, y: 12, width: width - 41, height: 21)
    }

    private func layoutCanvas() {
        guard comparison == nil else { return }
        guard let canvas, scroll.contentSize != .zero else {
            zoomReadout.stringValue = ""
            return
        }
        if autoZoom {
            let area = canvas.tool == .crop ? canvas.document.bounds : canvas.document.crop
            if area.height > 2.2 * area.width {
                let width = max(1, scroll.contentSize.width - 78)
                let ratio = width * window.backingScaleFactor / area.width
                zoom = ratio >= 1 ? .actual : .custom(ratio)
            } else {
                zoom = .fit
            }
        }
        canvas.layoutImage(in: scroll.contentSize, zoom: zoom)
        if case .fit = zoom { zoomModes.selection = 0 }
        else if case .actual = zoom { zoomModes.selection = 1 }
        else { zoomModes.selection = -1 }
        zoomReadout.stringValue = "\(Int((canvas.factor * window.backingScaleFactor * 100).rounded()))%"
        scrolled()
    }

    @objc private func scrolled() { reveal.isHidden = comparison != nil || canvas?.selectionIsOffscreen != true }
}


@MainActor
private enum CaptureShortcutHint {
    private struct Preference: Decodable {
        struct Shortcut: Decodable {
            let code: String
            let control, option, shift, command: Bool

            var label: String {
                let mods = (control ? "⌃" : "") + (option ? "⌥" : "") + (shift ? "⇧" : "") + (command ? "⌘" : "")
                let key = code.hasPrefix("Key") ? String(code.dropFirst(3))
                    : code.hasPrefix("Digit") ? String(code.dropFirst(5)) : code
                return mods + key
            }
        }
        let enabled: Bool
        let shortcut: Shortcut
    }

    static func current() -> String? {
        guard !NSRunningApplication.runningApplications(withBundleIdentifier: "com.richos.app").isEmpty,
              let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first,
              let bytes = try? Data(contentsOf: support.appendingPathComponent("com.richos.app/richglance-shortcut.json")),
              let preference = try? JSONDecoder().decode(Preference.self, from: bytes), preference.enabled else { return nil }
        return preference.shortcut.label
    }
}
