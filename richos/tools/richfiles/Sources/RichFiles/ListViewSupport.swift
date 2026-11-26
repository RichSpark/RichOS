import AppKit
import RichFilesCore
import SwiftUI

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
