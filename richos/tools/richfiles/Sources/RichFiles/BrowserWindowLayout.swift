import SwiftUI

enum BrowserLayoutPart: Equatable, Sendable {
    case toolbar, sidebar, sidebarDivider, primary, splitDivider, secondary, status
}

struct BrowserLayoutKey: LayoutValueKey {
    static let defaultValue = BrowserLayoutPart.primary
}

/// Keep both pane views alive so switching the shared toolbar does not reset
/// either pane's selection, scroll position or keyboard state.
struct BrowserWindowLayout: Layout {
    static let gutter: CGFloat = 5
    let sidebarWidth: CGFloat
    let paneRatio: CGFloat

    func sidebarWidth(in windowWidth: CGFloat) -> CGFloat {
        min(max(150, sidebarWidth), min(340, windowWidth * 0.33))
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        proposal.replacingUnspecifiedDimensions(by: CGSize(width: 1120, height: 700))
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        func view(_ part: BrowserLayoutPart) -> LayoutSubview? {
            subviews.first { $0[BrowserLayoutKey.self] == part }
        }
        func place(_ part: BrowserLayoutPart, at point: CGPoint, width: CGFloat, height: CGFloat) {
            view(part)?.place(at: point, anchor: .topLeading,
                              proposal: ProposedViewSize(width: width, height: height))
        }

        let row = ProposedViewSize(width: bounds.width, height: nil)
        let top = view(.toolbar)?.sizeThatFits(row).height ?? 0
        let bottom = view(.status)?.sizeThatFits(row).height ?? 0
        let y = bounds.minY + top
        let height = max(0, bounds.height - top - bottom)
        place(.toolbar, at: bounds.origin, width: bounds.width, height: top)
        place(.status, at: CGPoint(x: bounds.minX, y: bounds.maxY - bottom), width: bounds.width, height: bottom)

        var left = bounds.minX
        if view(.sidebar) != nil {
            let width = sidebarWidth(in: bounds.width)
            place(.sidebar, at: CGPoint(x: left, y: y), width: width, height: height)
            left += width
            place(.sidebarDivider, at: CGPoint(x: left, y: y), width: Self.gutter, height: height)
            left += Self.gutter
        }
        let split = view(.secondary) != nil
        let width = max(0, bounds.maxX - left - (split ? Self.gutter : 0))
        let minimum = min(width / 2, 190)
        let primary = split ? min(width - minimum, max(minimum, width * paneRatio)) : width
        place(.primary, at: CGPoint(x: left, y: y), width: primary, height: height)
        if split {
            left += primary
            place(.splitDivider, at: CGPoint(x: left, y: y), width: Self.gutter, height: height)
            left += Self.gutter
            place(.secondary, at: CGPoint(x: left, y: y), width: width - primary, height: height)
        }
    }
}

struct BrowserPaneDivider: View {
    let name: String
    let resize: (CGFloat) -> Void
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        let palette = BrowserPalette(scheme)
        Rectangle().fill(palette.pane)
            .overlay { Rectangle().fill(palette.line).frame(width: 1) }
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 2, coordinateSpace: .named("richfiles-window"))
                .onChanged { resize($0.location.x) })
            .accessibilityLabel(name)
    }
}
