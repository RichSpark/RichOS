import AppKit
import RichFilesCore
import SwiftUI

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
