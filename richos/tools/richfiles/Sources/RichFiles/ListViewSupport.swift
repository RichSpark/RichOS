import AppKit
import RichFilesCore
import SwiftUI

/// List view columns as wide as their longest name (round 107.1, catalog entry 11). Each column
/// is measured on its own, so columns differ in width, and the next column starts after a gap.
struct ListColumnLayout: Equatable {
    /// Row padding 8 + icon 18 + gap 8 + name + right padding 8, plus 1 px so a name that
    /// measures exactly to the pixel is not cut. The 1 px line sits inside the paddings.
    static let chrome: CGFloat = 43
    static let gap: CGFloat = 12
    static let maximum: CGFloat = 640

    let widths: [CGFloat]

    /// `names` in list order, `rows` per column. `extra(i)` adds room for item `i` (the cut
    /// mark, or the rename box's padding); `measure` returns a name's drawn width.
    init(names: [String], rows: Int, extra: (Int) -> CGFloat = { _ in 0 }, measure: (String) -> CGFloat) {
        let rows = max(1, rows)
        var widths: [CGFloat] = []
        var start = 0
        while start < names.count {
            let end = min(start + rows, names.count)
            var widest: CGFloat = 0
            for index in start..<end { widest = max(widest, measure(names[index]) + extra(index)) }
            widths.append(min(Self.maximum, (widest + Self.chrome).rounded(.up)))
            start = end
        }
        self.widths = widths
    }
}

/// Drawn widths of names in the reading font, cached per size so a large folder is measured once.
@MainActor
enum NameWidths {
    private static var cache: [CGFloat: [String: CGFloat]] = [:]

    static func measure(_ name: String, size: CGFloat) -> CGFloat {
        if let width = cache[size]?[name] { return width }
        let font = BrowserTypography.readingNSFont(size)
        let width = (name as NSString).size(withAttributes: [.font: font]).width.rounded(.up)
        if cache[size, default: [:]].count > 50_000 { cache[size] = [:] }
        cache[size, default: [:]][name] = width
        return width
    }
}

/// Re-evaluates its content when `object` changes, so only that part of a large view redraws.
struct Observing<Object: ObservableObject, Content: View>: View {
    @ObservedObject var object: Object
    let content: (Object) -> Content

    var body: some View { content(object) }
}

// MARK: - Scroll bars and the mouse wheel (catalog entries 9 and 15)

/// Placed inside a SwiftUI `ScrollView`. It gives the enclosing scroll view a scroll bar that is
/// always shown while there is more to see, drawn in the app's colors, and, for the List view,
/// turns a vertical wheel or swipe into horizontal scrolling: wheel down moves the list right.
struct PaneScrollBridge: NSViewRepresentable {
    var horizontal: Bool
    var colors: PaneScroller.Colors
    var scrolled: () -> Void = {}

    func makeNSView(context: Context) -> Bridge { Bridge() }

    func updateNSView(_ view: Bridge, context: Context) {
        view.horizontal = horizontal
        view.colors = colors
        view.scrolled = scrolled
        view.applyStyle()
    }

    final class Bridge: NSView {
        var horizontal = true
        var colors = PaneScroller.Colors()
        var scrolled: () -> Void = {}
        private weak var scrollView: NSScrollView?
        private var monitor: Any?
        private var styleObserver: NSObjectProtocol?
        private var convertingGesture = false

        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            detach()
            guard window != nil else { return }
            DispatchQueue.main.async { [weak self] in self?.attach() }
        }

        deinit {
            if let monitor { NSEvent.removeMonitor(monitor) }
            if let styleObserver { NotificationCenter.default.removeObserver(styleObserver) }
        }

        private func detach() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
            if let styleObserver { NotificationCenter.default.removeObserver(styleObserver) }
            styleObserver = nil
        }

        private func attach() {
            guard window != nil, let found = enclosingScrollView else { return }
            scrollView = found
            applyStyle()
            styleObserver = NotificationCenter.default.addObserver(
                forName: NSScroller.preferredScrollerStyleDidChangeNotification, object: nil, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.applyStyle() }
            }
            monitor = NSEvent.addLocalMonitorForEvents(matching: .scrollWheel) { [weak self] event in
                guard let self else { return event }
                return self.handle(event) ? nil : event
            }
        }

        func applyStyle() {
            guard let scrollView else { return }
            if scrollView.scrollerStyle != .legacy { scrollView.scrollerStyle = .legacy }
            scrollView.autohidesScrollers = true
            if horizontal {
                scrollView.hasHorizontalScroller = true
                let scroller = scrollView.horizontalScroller as? PaneScroller ?? PaneScroller()
                scroller.colors = colors
                if scrollView.horizontalScroller !== scroller { scrollView.horizontalScroller = scroller }
            } else {
                scrollView.hasVerticalScroller = true
                let scroller = scrollView.verticalScroller as? PaneScroller ?? PaneScroller()
                scroller.colors = colors
                if scrollView.verticalScroller !== scroller { scrollView.verticalScroller = scroller }
            }
            scrollView.tile()
        }

        private func handle(_ event: NSEvent) -> Bool {
            guard let scrollView, let window, event.window === window, !isHiddenOrHasHiddenAncestor,
                  window.attachedSheet == nil,
                  scrollView.bounds.contains(scrollView.convert(event.locationInWindow, from: nil)) else { return false }
            scrolled()
            guard horizontal else { return false }
            // A wheel notch has no phase. A trackpad gesture decides its axis when it begins and
            // keeps it through its momentum, so a sideways swipe still scrolls natively.
            if event.phase.contains(.began) || (event.phase.isEmpty && event.momentumPhase.isEmpty) {
                convertingGesture = abs(event.scrollingDeltaY) > abs(event.scrollingDeltaX)
            }
            guard convertingGesture, let sideways = Self.sideways(event) else { return false }
            scrollView.scrollWheel(with: sideways)
            return true
        }

        /// The same event with its vertical deltas moved to the horizontal axis, the way macOS
        /// turns Shift and the wheel into horizontal scrolling, so smoothness and direction settings hold.
        static func sideways(_ event: NSEvent) -> NSEvent? {
            guard let copy = event.cgEvent?.copy() else { return nil }
            let integer: [(CGEventField, CGEventField)] = [
                (.scrollWheelEventDeltaAxis1, .scrollWheelEventDeltaAxis2),
                (.scrollWheelEventPointDeltaAxis1, .scrollWheelEventPointDeltaAxis2)
            ]
            for (vertical, horizontal) in integer {
                copy.setIntegerValueField(horizontal, value: copy.getIntegerValueField(vertical))
                copy.setIntegerValueField(vertical, value: 0)
            }
            let fixed = (CGEventField.scrollWheelEventFixedPtDeltaAxis1, CGEventField.scrollWheelEventFixedPtDeltaAxis2)
            copy.setDoubleValueField(fixed.1, value: copy.getDoubleValueField(fixed.0))
            copy.setDoubleValueField(fixed.0, value: 0)
            return NSEvent(cgEvent: copy)
        }
    }
}

/// A scroll bar drawn in the app's colors: an 8 px track and thumb, the thumb deeper while
/// pointed at or dragged. The thumb clears 3:1 against its track in both themes
/// (`BrowserAppearanceTests`); the system's own thumb does not.
final class PaneScroller: NSScroller {
    struct Colors: Equatable {
        var ground = NSColor.clear
        var track = NSColor.clear
        var thumb = NSColor.gray
        var thumbHover = NSColor.darkGray
    }

    var colors = Colors() { didSet { if colors != oldValue { needsDisplay = true } } }
    private var pointed = false
    private var dragging = false

    override class var isCompatibleWithOverlayScrollers: Bool { true }

    private var isHorizontal: Bool { bounds.width >= bounds.height }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeInActiveApp, .inVisibleRect],
                                       owner: self))
    }

    override func mouseEntered(with event: NSEvent) { pointed = true; needsDisplay = true }
    override func mouseExited(with event: NSEvent) { pointed = false; needsDisplay = true }

    override func trackKnob(with event: NSEvent) {
        dragging = true
        needsDisplay = true
        super.trackKnob(with: event)
        dragging = false
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        colors.ground.setFill()
        bounds.fill()
        drawKnobSlot(in: rect(for: .knobSlot), highlight: false)
        if usableParts == .allScrollerParts { drawKnob() }
    }

    private func bar(_ rect: NSRect) -> NSRect {
        isHorizontal
            ? NSRect(x: rect.minX, y: bounds.midY - 4, width: rect.width, height: 8)
            : NSRect(x: bounds.midX - 4, y: rect.minY, width: 8, height: rect.height)
    }

    override func drawKnobSlot(in slotRect: NSRect, highlight flag: Bool) {
        let track = bar(slotRect).insetBy(dx: isHorizontal ? 6 : 0, dy: isHorizontal ? 0 : 6)
        colors.track.setFill()
        NSBezierPath(roundedRect: track, xRadius: 4, yRadius: 4).fill()
    }

    override func drawKnob() {
        let slot = bar(rect(for: .knobSlot)).insetBy(dx: isHorizontal ? 6 : 0, dy: isHorizontal ? 0 : 6)
        let knob = bar(rect(for: .knob)).intersection(slot)
        guard !knob.isEmpty else { return }
        (pointed || dragging ? colors.thumbHover : colors.thumb).setFill()
        NSBezierPath(roundedRect: knob, xRadius: 4, yRadius: 4).fill()
    }
}

extension BrowserPalette {
    /// The file pane's bar has a track; the sidebar's bar is a thumb alone on the sidebar, as in
    /// round 107.1 (the light track there would leave the thumb at 2.99:1).
    func scrollerColors(on ground: Color, track: Bool = true) -> PaneScroller.Colors {
        PaneScroller.Colors(ground: NSColor(ground), track: track ? NSColor(scrollTrack) : .clear,
                            thumb: NSColor(scrollThumb), thumbHover: NSColor(scrollThumbHover))
    }
}

// MARK: - Quick info on hover (catalog entry 13)

struct QuickInfo: Equatable {
    struct Line: Equatable, Hashable {
        let label: String
        let value: String
    }

    var lines: [Line]
    /// A folder's "Contains" line is counted when the box is about to show.
    var countFolder: URL?
}

/// Rest the pointer on an item for about two-thirds of a second and a small box shows its Kind,
/// Size (a folder: Contains) and Modified, just below and right of the pointer. It hides when
/// the pointer leaves, a key is pressed, a button is pressed or the list scrolls.
@MainActor
final class ItemQuickInfo {
    static let delay: Duration = .milliseconds(650)

    var describe: (URL) -> QuickInfo? = { _ in nil }
    var showHidden = false
    var style = Style()
    private var pending: Task<Void, Never>?
    private var pendingFor: URL?
    private var shownFor: URL?
    private var point = NSPoint.zero
    private var panel: NSPanel?
    private var monitor: Any?

    struct Style {
        var scheme = ColorScheme.light
        var size: CGFloat = 16
    }

    func moved(over url: URL, at screenPoint: NSPoint) {
        if shownFor == url { return }
        if pendingFor == url, abs(screenPoint.x - point.x) < 3, abs(screenPoint.y - point.y) < 3 { return }
        hide()
        guard NSEvent.pressedMouseButtons == 0, let info = describe(url) else { return }
        pendingFor = url
        point = screenPoint
        installMonitor()
        let showHidden = showHidden
        pending = Task { [weak self] in
            try? await Task.sleep(for: Self.delay)
            guard !Task.isCancelled else { return }
            var lines = info.lines
            if let folder = info.countFolder {
                let count = await Task.detached(priority: .utility) {
                    try? FileManager.default.contentsOfDirectory(atPath: folder.path)
                        .filter { showHidden || !$0.hasPrefix(".") }.count
                }.value
                guard !Task.isCancelled else { return }
                if let count { lines.insert(.init(label: "Contains", value: count == 1 ? "1 item" : "\(count) items"), at: 1) }
            }
            self?.show(lines, for: url)
        }
    }

    func left(_ url: URL) {
        if pendingFor == url || shownFor == url { hide() }
    }

    func hide() {
        pending?.cancel()
        pending = nil
        pendingFor = nil
        shownFor = nil
        panel?.orderOut(nil)
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }

    private func installMonitor() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(
            matching: [.keyDown, .scrollWheel, .leftMouseDown, .rightMouseDown, .otherMouseDown]
        ) { [weak self] event in
            self?.hide()
            return event
        }
    }

    private func show(_ lines: [QuickInfo.Line], for url: URL) {
        pending = nil
        pendingFor = nil
        let panel = self.panel ?? Self.makePanel()
        self.panel = panel
        let palette = BrowserPalette(style.scheme)
        let hosting = NSHostingView(rootView: QuickInfoBox(lines: lines, palette: palette, size: style.size))
        hosting.frame.size = hosting.fittingSize
        panel.contentView = hosting
        panel.appearance = NSAppearance(named: style.scheme == .dark ? .darkAqua : .aqua)
        let size = hosting.fittingSize
        var origin = NSPoint(x: point.x + 12, y: point.y - 20 - size.height)
        if let screen = NSScreen.screens.first(where: { NSMouseInRect(point, $0.frame, false) }) ?? NSScreen.main {
            let visible = screen.visibleFrame
            if origin.x + size.width > visible.maxX - 8 { origin.x = visible.maxX - 8 - size.width }
            if origin.y < visible.minY + 8 { origin.y = point.y + 10 }
        }
        panel.setFrame(NSRect(origin: origin, size: size), display: true)
        panel.orderFront(nil)
        shownFor = url
    }

    private static func makePanel() -> NSPanel {
        let panel = NSPanel(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel],
                            backing: .buffered, defer: true)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.level = .popUpMenu
        panel.ignoresMouseEvents = true
        panel.hidesOnDeactivate = true
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.transient, .ignoresCycle]
        return panel
    }
}

/// The quick-info box: labels in the muted ink (6.38:1 light, 5.78:1 dark on the sheet color,
/// computed in `BrowserAppearanceTests`), values in full ink, 16 px like every readable text.
private struct QuickInfoBox: View {
    let lines: [QuickInfo.Line]
    let palette: BrowserPalette
    let size: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(lines, id: \.self) { line in
                (Text(line.label + ": ").foregroundColor(palette.muted) + Text(line.value).foregroundColor(palette.ink))
                    .lineLimit(1)
            }
        }
        .font(BrowserTypography.reading(size))
        .padding(.top, 6).padding(.bottom, 7).padding(.horizontal, 10)
        .background(palette.floating, in: RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(palette.controlLine, lineWidth: 1))
        .fixedSize()
    }
}

// MARK: - Inline rename (catalog entries 2 and 11)

/// The name being edited in place. Kept out of `BrowserView`'s own state so that typing redraws
/// only the list and the edited name, not the whole window.
@MainActor
final class InlineRename: ObservableObject {
    enum Outcome: Equatable {
        case cancel
        case refuse(String)
        case rename(String)
    }

    @Published private(set) var item: URL?
    @Published var text = ""
    @Published var problem = ""
    private(set) var original = ""
    private(set) var directory = false
    private(set) var source: FileReference?
    private var armed: Task<Void, Never>?

    /// Click, pause, click: the box opens after `delay` unless another press cancels it first,
    /// and only if the clicked item is still the only selection then (as in round 107.1, where
    /// the timer checks the item is still selected): a key that moved the selection in the
    /// meantime, Down for example, cancels it.
    func arm(after delay: TimeInterval, stillSelected: @escaping @MainActor () -> Bool,
             _ begin: @escaping @MainActor () -> Void) {
        armed?.cancel()
        armed = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            guard !Task.isCancelled, let self else { return }
            self.armed = nil
            if stillSelected() { begin() }
        }
    }

    func cancelArm() {
        armed?.cancel()
        armed = nil
    }

    func begin(_ entry: FileEntry, source: FileReference) {
        cancelArm()
        self.source = source
        original = entry.name
        directory = entry.kind == .directory
        text = entry.name
        problem = ""
        item = entry.url
    }

    func end() {
        item = nil
        problem = ""
        source = nil
    }

    /// What Enter does with `typed`: nothing when it is empty or unchanged, a short message when
    /// macOS cannot use it or an item in the same folder already has it, else the new name.
    nonisolated static func outcome(for typed: String, original: String, siblings: [String]) -> Outcome {
        let name = typed.trimmingCharacters(in: .whitespacesAndNewlines)
        if name.isEmpty || name == original { return .cancel }
        if name.contains("/") || name.contains(":") { return .refuse("A name can’t contain “/” or “:”.") }
        let folded = name.lowercased()
        if siblings.contains(where: { $0 != original && $0.lowercased() == folded }) {
            return .refuse("An item named “\(name)” already exists here.")
        }
        return .rename(name)
    }

    /// The selection once the disk has renamed `original` to `moved`: the new name takes the
    /// selection only if the renamed item is still the only selected one. When the user
    /// finished the edit by clicking another item, or empty space, that click's selection
    /// stays, as in round 107.1; `nil` leaves the selection as it is.
    nonisolated static func selection(afterRenaming original: URL, to moved: URL, current: Set<URL>) -> Set<URL>? {
        current == [original] ? [moved] : nil
    }

    /// The part of `name` before its extension, in UTF-16 units, which the box selects at first.
    nonisolated static func stemLength(_ name: String, directory: Bool) -> Int {
        let text = name as NSString
        guard !directory else { return text.length }
        let ext = text.pathExtension
        return ext.isEmpty ? text.length : text.length - (ext as NSString).length - 1
    }
}

/// The inline name box: a thin 1 px ink border where the name was, the part before the extension
/// selected. Enter keeps the name, Esc cancels, leaving the box keeps it.
struct InlineRenameField: NSViewRepresentable {
    @ObservedObject var rename: InlineRename
    var font: NSFont
    var ink: NSColor
    var ground: NSColor
    var commit: (_ quiet: Bool) -> Void
    var cancel: () -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField(string: rename.text)
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = true
        field.focusRingType = .none
        field.lineBreakMode = .byClipping
        field.cell?.usesSingleLineMode = true
        field.cell?.isScrollable = true
        field.wantsLayer = true
        field.layer?.borderWidth = 1
        field.layer?.cornerRadius = 2
        field.delegate = context.coordinator
        field.setAccessibilityLabel("New name for \(rename.original)")
        configure(field)
        let stem = InlineRename.stemLength(rename.original, directory: rename.directory)
        DispatchQueue.main.async { [weak field] in
            guard let field, let window = field.window else { return }
            window.makeFirstResponder(field)
            field.currentEditor()?.selectedRange = NSRange(location: 0, length: stem)
        }
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        context.coordinator.parent = self
        configure(field)
        if field.currentEditor() == nil, field.stringValue != rename.text { field.stringValue = rename.text }
    }

    private func configure(_ field: NSTextField) {
        field.font = font
        field.textColor = ink
        field.backgroundColor = ground
        field.layer?.borderColor = ink.cgColor
    }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: InlineRenameField

        init(_ parent: InlineRenameField) { self.parent = parent }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else { return }
            MainActor.assumeIsolated {
                parent.rename.text = field.stringValue
                parent.rename.problem = ""
            }
        }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            MainActor.assumeIsolated {
                switch selector {
                case #selector(NSResponder.insertNewline(_:)):
                    parent.commit(false)
                    return true
                case #selector(NSResponder.cancelOperation(_:)):
                    parent.cancel()
                    return true
                default:
                    return false
                }
            }
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            MainActor.assumeIsolated {
                if parent.rename.item != nil { parent.commit(true) }
            }
        }
    }
}

/// The file pane takes keyboard focus but draws no ring or border around itself (catalog
/// entry 5); keyboard focus shows only on the item. macOS 13 has no API to turn the ring off.
struct NoFocusRing: ViewModifier {
    func body(content: Content) -> some View {
        if #available(macOS 14.0, *) {
            content.focusEffectDisabled()
        } else {
            content
        }
    }
}

/// Whether the file pane holds keyboard focus on macOS 13, where SwiftUI's focusable pane would
/// draw a ring around itself.
@MainActor
final class PaneKeyFocus: ObservableObject {
    @Published fileprivate(set) var focused = false
    fileprivate weak var view: NSView?

    func take() {
        guard let view, let window = view.window, window.firstResponder !== view else { return }
        window.makeFirstResponder(view)
    }
}

/// An AppKit view behind the file pane that takes keyboard focus and draws no focus ring.
struct PaneKeyFocusSink: NSViewRepresentable {
    let focus: PaneKeyFocus

    func makeNSView(context: Context) -> Sink {
        let sink = Sink()
        sink.focus = focus
        focus.view = sink
        return sink
    }

    func updateNSView(_ sink: Sink, context: Context) {
        sink.focus = focus
        focus.view = sink
    }

    final class Sink: NSView {
        weak var focus: PaneKeyFocus?

        override var acceptsFirstResponder: Bool { true }
        override var focusRingType: NSFocusRingType { get { .none } set {} }
        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func becomeFirstResponder() -> Bool {
            MainActor.assumeIsolated { focus?.focused = true }
            return true
        }

        override func resignFirstResponder() -> Bool {
            MainActor.assumeIsolated { focus?.focused = false }
            return true
        }
    }
}
