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
        if phase == .dragging { phase = .refining }
    }

    public mutating func shiftReturn() {
        switch phase {
        case .aiming: anchor(.placing)
        case .placing: phase = .refining
        case .dragging, .refining: break
        }
    }

    public mutating func switchCorner() {
        guard phase == .refining else { return }
        active = active == .first ? .second : .first
    }

    public mutating func toggleLens() { showsLens.toggle() }

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

    public func viewRect(_ sourceRect: CGRect) -> CGRect {
        CGRect(x: sourceRect.minX * view.width / frame.width,
               y: sourceRect.minY * view.height / frame.height,
               width: sourceRect.width * view.width / frame.width,
               height: sourceRect.height * view.height / frame.height)
    }
}
