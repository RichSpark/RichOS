import AppKit
import RichFilesCore
import SwiftUI

struct CleanupRingSlice: Equatable, Identifiable {
    enum Disposition: Equatable { case live, reviewed, trash, deleted }

    let id: URL
    let name: String
    let kind: CleanupCandidate.Kind
    let bytes: Int64
    let days: Int
    let folder: Bool
    let disposition: Disposition

    var interactive: Bool { disposition == .live || disposition == .reviewed }

    static func color(_ kind: CleanupCandidate.Kind, dark: Bool) -> NSColor {
        let value: UInt32 = switch kind {
        case .folder: dark ? 0x7fb2f0 : 0x2f6fb5
        case .video: dark ? 0xc59af0 : 0x7a4fb5
        case .installer: dark ? 0x6fd0c4 : 0x1f7f76
        case .archive: dark ? 0xee8aa0 : 0xb04a66
        case .image: dark ? 0x63c7e8 : 0x1d7391
        case .document: dark ? 0x9bd67a : 0x437326
        }
        return rgb(value)
    }

    static func rgb(_ value: UInt32) -> NSColor {
        NSColor(srgbRed: Double((value >> 16) & 255) / 255, green: Double((value >> 8) & 255) / 255,
                blue: Double(value & 255) / 255, alpha: 1)
    }
}

struct DiskCleanupRing: NSViewRepresentable {
    let slices: [CleanupRingSlice]
    let raised: Bool
    let dark: Bool
    let selected: URL?
    let resetZoom: UUID
    let available: String
    let allocation: String
    let reduceMotion: Bool
    let onSelect: (URL) -> Void
    let onOpen: (URL) -> Void
    let onUp: () -> Void
    let onPreview: (URL) -> Void
    let onReview: (URL) -> Void
    let onZoom: (URL?, Bool) -> Void

    func makeNSView(context: Context) -> CleanupRingView { CleanupRingView() }

    func updateNSView(_ view: CleanupRingView, context: Context) {
        view.update(self)
    }
}

/// One renderer owns both the flat ring and the raised blocks. Faces, pointer
/// targets and the age ruler use the same projection at every window size.
final class CleanupRingView: NSView {
    private struct Sector {
        let slice: CleanupRingSlice
        let start: Double
        let end: Double
    }

    private struct Tile {
        let sector: Sector
        let index: Int
        let count: Int
        let start: Double
        let end: Double
        let depth: Double
        let height: Double
    }

    private struct Target {
        let id: URL
        let face: NSBezierPath
    }

    private var input: DiskCleanupRing?
    private var sectors: [Sector] = []
    private var hitTargets: [Target] = []
    private var hover: URL?
    private var zoomTarget: URL?
    private var keyboardTarget: URL?
    private var tracking: NSTrackingArea?
    private var dragStart: CGPoint = .zero
    private var dragRotation = 0.0
    private var didDrag = false
    private var rotation = -0.48
    private var zoom = 1.0
    private var lift = 0.0
    private var liftTarget = 0.0
    private var sink = 1.0
    private var latestReset: UUID?
    private var previousDeleted = Set<URL>()
    private var animation: Timer?
    private var tickTime = 0.0
    private var filterOffset = CGPoint.zero

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        if newWindow == nil { animation?.invalidate(); animation = nil }
        super.viewWillMove(toWindow: newWindow)
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let tracking { removeTrackingArea(tracking) }
        let area = NSTrackingArea(rect: .zero, options: [.inVisibleRect, .activeInKeyWindow, .mouseMoved, .mouseEnteredAndExited],
                                  owner: self)
        addTrackingArea(area)
        tracking = area
    }

    func update(_ next: DiskCleanupRing) {
        let changed = input?.slices != next.slices
        input = next
        if latestReset != next.resetZoom {
            latestReset = next.resetZoom
            zoom = 1
            rotation = -0.48
            zoomTarget = nil
            keyboardTarget = nil
            filterOffset = .zero
        }
        if changed {
            let deleted = Set(next.slices.filter { $0.disposition == .deleted }.map(\.id))
            if !deleted.subtracting(previousDeleted).isEmpty { sink = next.reduceMotion ? 1 : 0 }
            previousDeleted = deleted
            sectors = Self.layout(next.slices)
        }
        liftTarget = next.raised ? 1 : 0
        if next.reduceMotion { lift = liftTarget; sink = 1 }
        else if lift != liftTarget || sink < 1 { animate() }
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel("Disk usage ring. Use arrow keys to select blocks.")
        setAccessibilityValue(next.selected.flatMap { url in next.slices.first { $0.id == url }?.name } ?? next.available)
        needsDisplay = true
    }

    private static func layout(_ slices: [CleanupRingSlice]) -> [Sector] {
        let total = slices.reduce(0.0) { $0 + Double(max(1, $1.bytes)) }
        guard total > 0 else { return [] }
        let tau = Double.pi * 2
        let minimum = min(0.07, tau / Double(slices.count) * 0.28)
        let spread = tau - minimum * Double(slices.count)
        var angle = -Double.pi / 2
        return slices.map { slice in
            let width = Double(max(1, slice.bytes)) / total * spread + minimum
            let gap = min(width * 0.23, 0.017)
            let sector = Sector(slice: slice, start: angle + gap, end: angle + width - gap)
            angle += width
            return sector
        }
    }

    private func animate() {
        guard animation == nil else { return }
        tickTime = ProcessInfo.processInfo.systemUptime
        let timer = Timer(timeInterval: 1 / 60, target: self, selector: #selector(tick), userInfo: nil, repeats: true)
        RunLoop.main.add(timer, forMode: .common)
        animation = timer
    }

    @objc private func tick() {
        let now = ProcessInfo.processInfo.systemUptime
        let dt = min(0.07, now - tickTime)
        tickTime = now
        if lift < liftTarget { lift = min(liftTarget, lift + dt / 0.94) }
        else { lift = max(liftTarget, lift - dt / 0.68) }
        sink = min(1, sink + dt / 0.9)
        needsDisplay = true
        if lift == liftTarget && sink == 1 { animation?.invalidate(); animation = nil }
    }

    private func ease(_ progress: Double) -> Double { progress * progress * (3 - 2 * progress) }

    override func draw(_ dirtyRect: NSRect) {
        guard let input, !bounds.isEmpty else { return }
        hitTargets = []
        let raised = ease(lift)
        let dark = input.dark
        let body = CleanupRingSlice.rgb(dark ? 0x182440 : 0xe8e9e6)
        let signal = CleanupRingSlice.rgb(dark ? 0xc2a35c : 0x715715)
        let ink = CleanupRingSlice.rgb(dark ? 0xdfe4ee : 0x0c1322)
        let muted = CleanupRingSlice.rgb(dark ? 0xa0a6b1 : 0x53575e)
        let outline = CleanupRingSlice.rgb(dark ? 0x7e92b8 : 0x5d7199)
        let tilt = 0.92 * raised
        let angle = rotation * raised
        let flatScale = min(bounds.width, bounds.height) / 226
        let tiltedScale = min(bounds.width / 233, bounds.height / 180)
        let scale = (flatScale + (tiltedScale - flatScale) * raised) * (1 + (zoom - 1) * raised)
        let offset = zoom > 1.02 ? filterOffset : .zero
        let center = CGPoint(x: bounds.midX + offset.x * raised,
                             y: bounds.midY + min(bounds.height * 0.09, 44) * raised + offset.y * raised)
        func project(_ radius: Double, _ a: Double, _ height: Double) -> CGPoint {
            let a = a + angle
            return CGPoint(x: center.x + radius * cos(a) * scale,
                           y: center.y + (radius * sin(a) * cos(tilt) - height * sin(tilt)) * scale)
        }

        if raised > 0 {
            let floor = NSBezierPath(ovalIn: NSRect(x: center.x - 109 * scale,
                y: center.y - 109 * scale * cos(tilt), width: 218 * scale, height: 218 * scale * cos(tilt)))
            body.withAlphaComponent(0.74 * raised).setFill()
            floor.fill()
            outline.withAlphaComponent(0.42 * raised).setStroke()
            floor.stroke()
        }
        guard !sectors.isEmpty else {
            text("No measured items", at: center, width: bounds.width - 20, ink: muted)
            return
        }

        var tiles: [Tile] = []
        for (index, sector) in sectors.enumerated() {
            let steps = max(1, Int(ceil((sector.end - sector.start) / 0.07)))
            let increment = (sector.end - sector.start) / Double(steps)
            let rise = ease(max(0, min(1, lift * 1.65 - 0.65 * Double(index) / Double(sectors.count))))
            let days = Double(max(0, min(420, sector.slice.days)))
            let height = (4 + days / 420 * 62) * rise * (sector.slice.disposition == .deleted ? 1 - ease(sink) : 1)
            for index in 0..<steps {
                let start = sector.start + Double(index) * increment
                let end = start + increment
                tiles.append(Tile(sector: sector, index: index, count: steps, start: start, end: end,
                                  depth: sin((start + end) / 2 + angle), height: height))
            }
        }
        tiles.sort { $0.depth < $1.depth }
        var chips: [URL: CGPoint] = [:]
        for tile in tiles {
            let slice = tile.sector.slice
            let base = slice.disposition == .reviewed ? raised * 6 : 0
            let radius = slice.folder ? 101.0 : 95.0
            let topZ = base + tile.height
            let inner = 53.0
            let viewer = slice.id == input.selected || slice.id == hover
            let faded = slice.disposition == .trash
            let freed = slice.disposition == .deleted && sink > 0.95
            let hue = switch slice.disposition {
            case .live: CleanupRingSlice.color(slice.kind, dark: dark)
            case .reviewed, .deleted: signal
            case .trash: outline
            }
            let tint = viewer ? hue.blended(withFraction: 0.18, of: dark ? .white : .black) ?? hue : hue
            func face(_ points: [CGPoint], shade: Double, alpha: Double = 1) {
                let path = polygon(points)
                let color = tint.blended(withFraction: shade, of: .black) ?? tint
                color.withAlphaComponent(alpha).setFill()
                path.fill()
                if slice.interactive { hitTargets.append(Target(id: slice.id, face: path)) }
            }
            let a = tile.start, b = tile.end
            let top = [project(inner, a, topZ), project(radius, a, topZ),
                       project(radius, b, topZ), project(inner, b, topZ)]
            if tile.index == tile.count / 2 { chips[slice.id] = project(radius + 5, (a + b) / 2, topZ) }
            if freed {
                face(top, shade: 0, alpha: 0.12)
                stroke([top[1], top[2]], color: signal, width: 1.6, dashed: true)
                stroke([top[0], top[3]], color: signal, width: 1.6, dashed: true)
                continue
            }
            let alpha = faded ? 0.42 : 1.0
            if tile.height > 0.1 {
                let r = tile.depth < 0 ? inner : radius
                face([project(r, a, base), project(r, b, base), project(r, b, topZ), project(r, a, topZ)],
                     shade: tile.depth < 0 ? 0.48 : 0.26, alpha: alpha)
                if tile.index == 0 || tile.index == tile.count - 1 {
                    let cut = tile.index == 0 ? a : b
                    face([project(inner, cut, base), project(radius, cut, base),
                          project(radius, cut, topZ), project(inner, cut, topZ)], shade: 0.22, alpha: alpha)
                }
            }
            face(top, shade: raised * 0.06 * (1 - tile.depth), alpha: alpha)
            if faded || viewer || slice.disposition == .reviewed {
                stroke([top[1], top[2]], color: viewer ? ink : faded ? outline : signal,
                       width: viewer ? 2 : 1.3, dashed: faded)
            }
            if base > 1 {
                stroke([project(radius, a, 0), project(radius, b, 0)], color: signal, width: 1.3, dashed: true)
            }
        }

        if raised < 0.4 {
            text(input.available, at: CGPoint(x: center.x, y: center.y - 11), width: 130, ink: ink, size: 23, bold: true)
            text(input.allocation, at: CGPoint(x: center.x, y: center.y + 15), width: 158, ink: muted)
        }
        if raised > 0.6 && zoom < 1.1 {
            let start = project(0, 0, 0)
            let x = CGFloat(13)
            stroke([CGPoint(x: x, y: start.y), CGPoint(x: x, y: start.y - 72 * sin(tilt) * scale)], color: outline, width: 1)
            for (days, label) in [(30.0, "30 d"), (180, "6 mo"), (365, "1 yr")] {
                let y = project(0, 0, 4 + days / 420 * 62).y
                stroke([CGPoint(x: x - 5, y: y), CGPoint(x: x + 5, y: y)], color: ink, width: 1)
                if bounds.width > 320 { text(label, at: CGPoint(x: x + 26, y: y), width: 40, ink: muted) }
            }
        }
        if bounds.width > 350 {
            let prominent = input.slices.filter(\.interactive).prefix(3).map(\.id)
            let shown = input.slices.filter { $0.id == input.selected || $0.id == hover ||
                (raised > 0.65 && zoom < 1.08 && (prominent.contains($0.id) || !$0.interactive)) }
            var labels: [CGRect] = []
            for slice in shown.prefix(6) {
                guard let anchor = chips[slice.id], bounds.insetBy(dx: 10, dy: 16).contains(anchor) else { continue }
                let x = min(bounds.width - 95, max(95, anchor.x))
                var y = anchor.y - 10
                var rect = CGRect(x: x - 88, y: y - 22, width: 176, height: 43)
                while labels.contains(where: { $0.intersects(rect) }) && y > 70 {
                    y -= 44
                    rect.origin.y = y - 22
                }
                guard bounds.insetBy(dx: 2, dy: 2).contains(rect) else { continue }
                labels.append(rect)
                let badge = NSBezierPath(roundedRect: rect, xRadius: 6, yRadius: 6)
                body.setFill(); badge.fill()
                (slice.disposition == .reviewed ? signal : outline).setStroke(); badge.stroke()
                let detail = switch slice.disposition {
                case .trash: "In Trash · \(formattedBytes(slice.bytes))"
                case .deleted: "Deleted · \(formattedBytes(slice.bytes))"
                default: "\(formattedBytes(slice.bytes)) · \(slice.days) days"
                }
                text(slice.name, at: CGPoint(x: x, y: y - 9), width: 165, ink: ink, bold: true)
                text(detail, at: CGPoint(x: x, y: y + 10), width: 165, ink: muted)
            }
        }
    }

    private func polygon(_ points: [CGPoint]) -> NSBezierPath {
        let path = NSBezierPath()
        if let first = points.first {
            path.move(to: first)
            points.dropFirst().forEach { path.line(to: $0) }
            path.close()
        }
        return path
    }

    private func stroke(_ points: [CGPoint], color: NSColor, width: CGFloat, dashed: Bool = false) {
        guard let first = points.first else { return }
        let line = NSBezierPath()
        line.move(to: first)
        points.dropFirst().forEach { line.line(to: $0) }
        color.setStroke()
        line.lineWidth = width
        if dashed { line.setLineDash([3, 3], count: 2, phase: 0) }
        line.stroke()
    }

    private func text(_ label: String, at point: CGPoint, width: CGFloat, ink: NSColor,
                      size: CGFloat = 14, bold: Bool = false) {
        guard width > 0 else { return }
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .center
        paragraph.lineBreakMode = .byTruncatingTail
        let font = NSFont.systemFont(ofSize: size, weight: bold ? .semibold : .regular)
        (label as NSString).draw(with: CGRect(x: point.x - width / 2, y: point.y - size / 2 - 1,
            width: width, height: size + 4), options: [.truncatesLastVisibleLine],
            attributes: [.foregroundColor: ink, .font: font, .paragraphStyle: paragraph])
    }

    private func target(at point: CGPoint) -> URL? {
        hitTargets.reversed().first { $0.face.contains(point) }?.id
    }

    override func mouseMoved(with event: NSEvent) {
        let next = target(at: convert(event.locationInWindow, from: nil))
        if hover != next { hover = next; needsDisplay = true }
    }

    override func mouseExited(with event: NSEvent) {
        if hover != nil { hover = nil; needsDisplay = true }
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        dragStart = convert(event.locationInWindow, from: nil)
        dragRotation = rotation
        didDrag = false
    }

    override func mouseDragged(with event: NSEvent) {
        guard liftTarget > 0 else { return }
        let point = convert(event.locationInWindow, from: nil)
        if abs(point.x - dragStart.x) + abs(point.y - dragStart.y) > 4 { didDrag = true }
        if didDrag { rotation = dragRotation + Double(point.x - dragStart.x) / 120; needsDisplay = true }
    }

    override func mouseUp(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        guard !didDrag, let id = target(at: point) else { return }
        keyboardTarget = id
        if event.clickCount > 1 && input?.slices.first(where: { $0.id == id })?.folder == true {
            input?.onOpen(id)
        } else { input?.onSelect(id) }
    }

    override func scrollWheel(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        let delta = event.scrollingDeltaY
        guard liftTarget > 0, delta != 0 else { super.scrollWheel(with: event); return }
        let id = target(at: point) ?? zoomTarget
        if zoom <= 1.01 && delta < -6 && (id != nil || event.modifierFlags.contains(.control)) {
            input?.onUp()
            return
        }
        guard id != nil || zoom > 1.01 || event.modifierFlags.contains(.control) else {
            super.scrollWheel(with: event)
            return
        }
        if zoom > 2.62, delta > 3, event.momentumPhase.isEmpty, let id,
           input?.slices.first(where: { $0.id == id })?.folder == true {
            input?.onOpen(id)
            return
        }
        applyZoom(exp(Double(delta) * (event.hasPreciseScrollingDeltas ? 0.014 : 0.075)), at: point, id: id)
    }

    override func magnify(with event: NSEvent) {
        guard liftTarget > 0 else { return }
        let point = convert(event.locationInWindow, from: nil)
        applyZoom(1 + event.magnification, at: point, id: target(at: point) ?? zoomTarget)
    }

    private func applyZoom(_ factor: Double, at point: CGPoint, id: URL?) {
        let previous = zoom
        zoom = min(2.7, max(1, zoom * factor))
        let effectiveRatio = zoom / previous
        if zoom > 1.02 {
            let anchor = CGPoint(x: bounds.midX, y: bounds.midY + min(bounds.height * 0.09, 44))
            let limit = bounds.width * 0.8
            filterOffset.x = max(-limit, min(limit, filterOffset.x * effectiveRatio + (1 - effectiveRatio) * (point.x - anchor.x)))
            filterOffset.y = max(-limit, min(limit, filterOffset.y * effectiveRatio + (1 - effectiveRatio) * (point.y - anchor.y)))
            zoomTarget = id
        } else {
            filterOffset = .zero
            zoomTarget = nil
        }
        input?.onZoom(zoom >= 1.38 ? zoomTarget : nil, zoom > 1.02)
        needsDisplay = true
    }

    override func keyDown(with event: NSEvent) {
        guard let input else { super.keyDown(with: event); return }
        let candidates = input.slices.filter(\.interactive)
        guard !candidates.isEmpty else { super.keyDown(with: event); return }
        let index = candidates.firstIndex { $0.id == (keyboardTarget ?? input.selected) }
        let current = candidates[index ?? 0]
        switch event.keyCode {
        case 125, 126:
            let next = index.map { ($0 + candidates.count + (event.keyCode == 125 ? 1 : -1)) % candidates.count } ?? 0
            keyboardTarget = candidates[next].id
            input.onSelect(candidates[next].id)
        case 124: if current.folder { input.onOpen(current.id) }
        case 123: input.onUp()
        case 49: input.onPreview(current.id)
        case 36:
            if current.folder && current.id == input.selected { input.onOpen(current.id) }
            else if current.id == input.selected { input.onReview(current.id) }
            else { input.onSelect(current.id) }
        default: super.keyDown(with: event)
        }
    }
}
