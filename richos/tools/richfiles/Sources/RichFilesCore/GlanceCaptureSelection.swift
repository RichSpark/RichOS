import CoreGraphics

/// The points are boundaries between pixels, measured from the top left of the source frame.
public struct GlanceCaptureSelection {
    public struct Point: Equatable {
        public var x: Int
        public var y: Int

        public init(x: Int, y: Int) {
            self.x = x
            self.y = y
        }
    }

    public enum Phase: Equatable { case aiming, dragging, placing, refining }
    public enum Corner { case first, second }

    public let width: Int
    public let height: Int
    public private(set) var phase = Phase.aiming
    public private(set) var active = Corner.second
    public private(set) var first: Point
    public private(set) var second: Point
    public private(set) var showsLens = true
    private var previousPointer: CGPoint

    public init(width: Int, height: Int, pointer: CGPoint) {
        self.width = width
        self.height = height
        previousPointer = pointer
        let point = Point(x: Self.snap(pointer.x, to: width), y: Self.snap(pointer.y, to: height))
        first = point
        second = point
    }

    public var target: Point { active == .first ? first : second }
    public var crop: CGRect? {
        guard phase != .aiming else { return nil }
        return CGRect(x: min(first.x, second.x), y: min(first.y, second.y),
                      width: abs(second.x - first.x), height: abs(second.y - first.y))
    }

    public var canCapture: Bool { crop?.isEmpty == false }

    /// The pointer is in continuous source coordinates. A stationary pointer cannot undo a nudge.
    public mutating func movePointer(to point: CGPoint) {
        guard point != previousPointer else { return }
        previousPointer = point
        guard phase != .refining else { return }
        setTarget(Point(x: Self.snap(point.x, to: width), y: Self.snap(point.y, to: height)))
    }

    public mutating func nudge(dx: Int, dy: Int) {
        setTarget(Point(x: min(width, max(0, target.x + dx)),
                        y: min(height, max(0, target.y + dy))))
    }

    public mutating func mouseDown(at point: CGPoint) {
        guard phase == .aiming else { return }
        movePointer(to: point)
        anchor(.dragging)
    }

    public mutating func mouseUp() {
        guard phase == .dragging else { return }
        if canCapture {
            phase = .refining
        } else {
            first = second
            phase = .aiming
        }
    }

    public mutating func shiftReturn() {
        switch phase {
        case .aiming: anchor(.placing)
        case .placing: if canCapture { phase = .refining }
        case .dragging, .refining: break
        }
    }

    public mutating func switchCorner() {
        guard phase == .refining else { return }
        active = active == .first ? .second : .first
    }

    /// Replace a refined rectangle after a mouse handle drag. Preserve which diagonal is active.
    public mutating func refine(active point: CGPoint, fixed opposite: CGPoint) {
        guard phase == .refining else { return }
        let fixed = Point(x: Self.snap(opposite.x, to: width), y: Self.snap(opposite.y, to: height))
        if active == .first { second = fixed }
        else { first = fixed }
        setTarget(Point(x: Self.snap(point.x, to: width), y: Self.snap(point.y, to: height)))
    }

    /// Translate the entire crop, stopping at the first display boundary.
    public mutating func translate(dx: Int, dy: Int) {
        guard phase == .refining, let crop else { return }
        let x = max(-Int(crop.minX), min(width - Int(crop.maxX), dx))
        let y = max(-Int(crop.minY), min(height - Int(crop.maxY), dy))
        first.x += x
        first.y += y
        second.x += x
        second.y += y
    }

    /// Prefill a fixed-size repeat selection whose position remains adjustable.
    public mutating func restore(_ rect: CGRect) {
        first = Point(x: Self.snap(rect.minX, to: width), y: Self.snap(rect.minY, to: height))
        second = Point(x: Self.snap(rect.maxX, to: width), y: Self.snap(rect.maxY, to: height))
        phase = canRestore ? .refining : .aiming
        active = .second
    }

    private var canRestore: Bool { first.x != second.x && first.y != second.y }

    public mutating func toggleLens() { showsLens.toggle() }

    /// Discard a crop or its first corner without leaving the capture session.
    /// A false return means the overlay is already aiming and Escape can close it.
    public mutating func discardSelection(at pointer: CGPoint) -> Bool {
        guard phase != .aiming else { return false }
        let showsLens = self.showsLens
        self = Self(width: width, height: height, pointer: pointer)
        self.showsLens = showsLens
        return true
    }

    private mutating func anchor(_ phase: Phase) {
        first = second
        self.phase = phase
    }

    private mutating func setTarget(_ point: Point) {
        switch active {
        case .first: first = point
        case .second: second = point
        }
    }

    private static func snap(_ coordinate: CGFloat, to limit: Int) -> Int {
        Int(min(CGFloat(limit), max(0, coordinate.rounded())))
    }
}

public struct GlanceCaptureGeometry {
    public let frame: CGSize
    public let view: CGSize

    public init(frame: CGSize, view: CGSize) {
        self.frame = frame
        self.view = view
    }

    public func sourcePoint(_ viewPoint: CGPoint) -> CGPoint {
        CGPoint(x: viewPoint.x * frame.width / view.width,
                y: viewPoint.y * frame.height / view.height)
    }

    public func viewPoint(_ sourcePoint: GlanceCaptureSelection.Point) -> CGPoint {
        CGPoint(x: CGFloat(sourcePoint.x) * view.width / frame.width,
                y: CGFloat(sourcePoint.y) * view.height / frame.height)
    }

    public func sourceRect(_ viewRect: CGRect) -> CGRect {
        CGRect(x: viewRect.minX * frame.width / view.width,
               y: viewRect.minY * frame.height / view.height,
               width: viewRect.width * frame.width / view.width,
               height: viewRect.height * frame.height / view.height)
    }

    public func viewRect(_ sourceRect: CGRect) -> CGRect {
        CGRect(x: sourceRect.minX * view.width / frame.width,
               y: sourceRect.minY * view.height / frame.height,
               width: sourceRect.width * view.width / frame.width,
               height: sourceRect.height * view.height / frame.height)
    }
}
