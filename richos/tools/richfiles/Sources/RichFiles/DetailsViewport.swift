import AppKit
import Combine
import SwiftUI

@MainActor
final class DetailsViewport {
    private weak var table: NSTableView?
    private weak var model: BrowserModel?
    private var scrolling: AnyCancellable?
    private var pending: (item: URL, alignToTop: Bool)?

    func attach(from marker: NSView, to model: BrowserModel) {
        var ancestor = marker.superview
        while let view = ancestor, !(view is NSTableView) {
            ancestor = view.superview
        }
        guard let table = ancestor as? NSTableView, let clip = table.enclosingScrollView?.contentView else { return }
        self.model = model
        if table !== self.table {
            self.table = table
            clip.postsBoundsChangedNotifications = true
            scrolling = NotificationCenter.default.publisher(for: NSView.boundsDidChangeNotification, object: clip)
                .sink { [weak self] _ in self?.capture() }
        }
        scrollIfReady()
    }

    func show(_ item: URL?, alignToTop: Bool) {
        pending = item.map { ($0, alignToTop) }
        scrollIfReady()
    }

    func focusTable() {
        if let table { table.window?.makeFirstResponder(table) }
    }

    func item(at locationInWindow: NSPoint, in window: NSWindow) -> URL? {
        guard let table, table.window === window, let model,
              table.numberOfRows == model.entries.count else { return nil }
        let point = table.convert(locationInWindow, from: nil)
        guard table.visibleRect.contains(point) else { return nil }
        let row = table.row(at: point)
        return model.entries.indices.contains(row) ? model.entries[row].url : nil
    }

    func detach() {
        scrolling = nil
        pending = nil
        table = nil
        model = nil
    }

    private func capture() {
        guard let table, let model, table.numberOfRows == model.entries.count else { return }
        let visible = table.rows(in: table.visibleRect)
        guard visible.length > 0, model.entries.indices.contains(visible.location) else { return }
        model.recordViewport(model.entries[visible.location].url, in: model.folder, tab: model.currentTabID, mode: .details)
    }

    private func scrollIfReady() {
        guard let request = pending, let table, let model,
              let scrollView = table.enclosingScrollView,
              let row = model.entries.firstIndex(where: { $0.url == request.item }),
              row < table.numberOfRows else { return }
        pending = nil
        if request.alignToTop {
            let clip = scrollView.contentView
            let rowFrame = table.convert(table.rect(ofRow: row), to: clip)
            var target = clip.bounds
            target.origin.y = rowFrame.minY
            clip.scroll(to: clip.constrainBoundsRect(target).origin)
            scrollView.reflectScrolledClipView(clip)
        } else {
            table.scrollRowToVisible(row)
        }
        capture()
    }
}

@MainActor
struct DetailsCellMarker: NSViewRepresentable {
    let viewport: DetailsViewport
    let model: BrowserModel

    func makeNSView(context: Context) -> Marker { Marker() }

    func updateNSView(_ view: Marker, context: Context) {
        view.connect = { [weak viewport, weak model] marker in
            if let model { viewport?.attach(from: marker, to: model) }
        }
        view.connect?(view)
    }

    final class Marker: NSView {
        var connect: ((NSView) -> Void)?

        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if window != nil { connect?(self) }
        }
    }
}
