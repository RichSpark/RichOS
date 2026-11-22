import CoreGraphics

public enum WindowTile: CaseIterable, Sendable, Equatable {
    case left, right, top, bottom, topLeft, topRight, bottomLeft, bottomRight

    public func frame(in usable: CGRect) -> CGRect {
        let middleX = floor(usable.midX)
        let middleY = floor(usable.midY)
        let left: CGFloat
        let right: CGFloat
        let top: CGFloat
        let bottom: CGFloat
        switch self {
        case .left, .topLeft, .bottomLeft: (left, right) = (usable.minX, middleX)
        case .right, .topRight, .bottomRight: (left, right) = (middleX, usable.maxX)
        case .top, .bottom: (left, right) = (usable.minX, usable.maxX)
        }
        switch self {
        case .top, .topLeft, .topRight: (top, bottom) = (usable.minY, middleY)
        case .bottom, .bottomLeft, .bottomRight: (top, bottom) = (middleY, usable.maxY)
        case .left, .right: (top, bottom) = (usable.minY, usable.maxY)
        }
        return CGRect(x: left, y: top, width: right - left, height: bottom - top)
    }

    public static func atEdge(_ point: CGPoint, of display: CGRect, distance: CGFloat = 12) -> Self? {
        guard display.contains(point) else { return nil }
        let left = point.x - display.minX <= distance
        let right = display.maxX - point.x <= distance
        let top = point.y - display.minY <= distance
        let bottom = display.maxY - point.y <= distance
        if left { return top ? .topLeft : bottom ? .bottomLeft : .left }
        if right { return top ? .topRight : bottom ? .bottomRight : .right }
        if top { return .top }
        if bottom { return .bottom }
        return nil
    }
}
