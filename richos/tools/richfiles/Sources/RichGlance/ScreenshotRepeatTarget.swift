import AppKit
import ColorSync

struct ScreenshotRepeatTarget: Codable, Equatable, Sendable {
    let id: UUID
    let display: String
    let pixels: CGRect
    let scale: CGFloat

    static func displayIdentifier(_ id: CGDirectDisplayID) -> String {
        guard let uuid = CGDisplayCreateUUIDFromDisplayID(id)?.takeRetainedValue() else { return String(id) }
        return CFUUIDCreateString(nil, uuid) as String
    }

    func crop(at newScale: CGFloat, within size: CGSize) -> CGRect? {
        guard scale > 0, scale.isFinite, newScale > 0, newScale.isFinite,
              !pixels.isEmpty, !pixels.isInfinite,
              pixels.minX.isFinite, pixels.minY.isFinite, pixels.maxX.isFinite, pixels.maxY.isFinite else { return nil }
        let ratio = newScale / scale
        let rounded = CGRect(x: (pixels.minX * ratio).rounded(), y: (pixels.minY * ratio).rounded(),
                             width: (pixels.width * ratio).rounded(), height: (pixels.height * ratio).rounded())
        let full = CGRect(origin: .zero, size: size)
        return !rounded.isEmpty && full.contains(rounded) ? rounded : nil
    }
}
