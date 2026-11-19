import AppKit

@MainActor
final class ScreenshotRecents: NSObject, NSWindowDelegate {
    let window: NSPanel
    var edit: ((ScreenshotStore.Entry) -> Void)?
    var closed: (() -> Void)?
    private let store: ScreenshotStore
    private let table = RecentTable()
    private let title = NSTextField(wrappingLabelWithString: "")
    private let empty = NSTextField(wrappingLabelWithString: "")
    private let toggle = NSSwitch()
    private let clear = NSButton(title: "Clear All…", target: nil, action: nil)
    private let scroll = NSScrollView()
    private var entries: [ScreenshotStore.Entry] = []

    init(store: ScreenshotStore) {
        self.store = store
        window = NSPanel(contentRect: CGRect(x: 0, y: 0, width: 650, height: 490),
                         styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        super.init()
        window.title = "Recent Captures"
        window.minSize = CGSize(width: 510, height: 360)
        window.isReleasedWhenClosed = false
        window.delegate = self
        let root = NSStackView()
        root.orientation = .vertical
        root.alignment = .width
        root.edgeInsets = NSEdgeInsets(top: 20, left: 22, bottom: 20, right: 22)
        root.spacing = 13
        let label = NSTextField(labelWithString: "Show Recent Captures")
        label.font = .systemFont(ofSize: 18, weight: .semibold)
        toggle.target = self
        toggle.action = #selector(toggleHistory)
        toggle.setAccessibilityLabel(label.stringValue)
        let header = NSStackView(views: [label, NSView(), toggle])
        header.spacing = 12
        root.addArrangedSubview(header)
        title.font = .systemFont(ofSize: 16)
        title.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        root.addArrangedSubview(title)
        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("Capture"))
        column.resizingMask = .autoresizingMask
        table.addTableColumn(column)
        table.headerView = nil
        table.rowHeight = 68
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.doubleAction = #selector(openSelected)
        table.setAccessibilityLabel("Recent Captures")
        table.key = { [weak self] code in
            guard let self else { return false }
            switch code {
            case 36, 76: openSelected()
            case 51, 117: deleteSelected()
            case 53: window.performClose(nil)
            case 123, 124:
                guard !entries.isEmpty else { return true }
                let delta = code == 123 ? -1 : 1
                let row = min(entries.count - 1, max(0, table.selectedRow + delta))
                table.selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
                table.scrollRowToVisible(row)
            default: return false
            }
            return true
        }
        scroll.documentView = table
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder
        scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 110).isActive = true
        root.addArrangedSubview(scroll)
        empty.font = .systemFont(ofSize: 16)
        empty.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        root.addArrangedSubview(empty)
        clear.font = .systemFont(ofSize: 16, weight: .medium)
        clear.controlSize = .large
        clear.bezelStyle = .rounded
        clear.target = self
        clear.action = #selector(clearAll)
        root.addArrangedSubview(NSStackView(views: [NSView(), clear]))
        window.contentView = root
    }

    func show() {
        refresh()
        if !window.isVisible { window.center() }
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(entries.isEmpty ? toggle : table)
    }

    func refresh() {
        let previous = selected?.id
        let row = max(0, table.selectedRow)
        let enabled = store.keepsHistory
        toggle.state = enabled ? .on : .off
        title.stringValue = enabled
            ? "Each capture holds the original plus your marks for seven days. You can remove a redaction in the editor. Captures stay on this Mac."
            : "Turn on Recent Captures to keep screenshots and editable marks on this Mac for seven days."
        do {
            entries = try store.entries()
            empty.stringValue = enabled ? "No recent captures yet." : "Recent Captures is off."
        } catch {
            entries = []
            empty.stringValue = "Could not read Recent Captures. Reopen this window to try again."
        }
        scroll.isHidden = entries.isEmpty
        empty.isHidden = !entries.isEmpty
        clear.isEnabled = !entries.isEmpty
        table.reloadData()
        if !entries.isEmpty {
            let index = entries.firstIndex { $0.id == previous } ?? min(row, entries.count - 1)
            table.selectRowIndexes(IndexSet(integer: index), byExtendingSelection: false)
        }
    }

    func windowWillClose(_ notification: Notification) { closed?() }

    private var selected: ScreenshotStore.Entry? {
        entries.indices.contains(table.selectedRow) ? entries[table.selectedRow] : nil
    }

    @objc private func openSelected() {
        guard let selected else { return }
        edit?(selected)
    }

    @objc private func deleteSelected() {
        guard let selected else { return }
        delete(selected.id)
    }

    private func delete(_ id: UUID) {
        do { try store.remove(id); refresh() }
        catch { showError("Could not delete this capture.") }
    }

    @objc private func deleteRow(_ sender: NSButton) {
        if let id = sender.identifier.flatMap({ UUID(uuidString: $0.rawValue) }) { delete(id) }
    }

    @objc private func editRow(_ sender: NSButton) {
        if let id = sender.identifier.flatMap({ UUID(uuidString: $0.rawValue) }),
           let entry = entries.first(where: { $0.id == id }) { edit?(entry) }
    }

    @objc private func clearAll() {
        confirm("Clear all recent captures?", details: "Deletes the local originals and saved marks in Recent Captures.", accept: "Clear All") { [weak self] in
            guard let self else { return }
            do { try store.clear(); refresh() }
            catch { showError("Could not clear Recent Captures.") }
        }
    }

    @objc private func toggleHistory() {
        if toggle.state == .on {
            do { try store.setHistory(true); refresh() }
            catch { refresh(); showError("Could not turn on Recent Captures.") }
        } else {
            toggle.state = .on
            confirm("Turn off Recent Captures?", details: "Turning this off deletes the local originals and saved marks in Recent Captures.", accept: "Turn Off and Delete") { [weak self] in
                guard let self else { return }
                do { try store.setHistory(false); refresh() }
                catch { refresh(); showError("Could not turn off Recent Captures.") }
            }
        }
    }

    private func confirm(_ title: String, details: String, accept: String, action: @escaping () -> Void) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = details
        alert.addButton(withTitle: "Cancel")
        alert.addButton(withTitle: accept)
        alert.beginSheetModal(for: window) { if $0 == .alertSecondButtonReturn { action() } }
    }

    private func showError(_ text: String) {
        let alert = NSAlert()
        alert.messageText = text
        alert.addButton(withTitle: "OK")
        alert.beginSheetModal(for: window)
    }
}

extension ScreenshotRecents: NSTableViewDataSource, NSTableViewDelegate {
    func numberOfRows(in tableView: NSTableView) -> Int { entries.count }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let entry = entries[row]
        let picture = NSImageView(image: store.thumbnail(entry.id) ?? NSImage())
        picture.imageScaling = .scaleProportionallyUpOrDown
        picture.widthAnchor.constraint(equalToConstant: 72).isActive = true
        picture.heightAnchor.constraint(equalToConstant: 53).isActive = true
        let caption = NSTextField(labelWithString: entry.created.formatted(date: .abbreviated, time: .shortened))
        caption.font = .systemFont(ofSize: 16, weight: .semibold)
        let detail = NSTextField(labelWithString: entry.redactions > 0
            ? "Holds the original under \(entry.redactions) \(entry.redactions == 1 ? "redaction" : "redactions")"
            : entry.edits.marks.isEmpty ? "Holds the original" : "Holds the original and your marks")
        detail.font = .systemFont(ofSize: 16)
        let labels = NSStackView(views: [caption, detail])
        labels.orientation = .vertical
        labels.alignment = .leading
        labels.spacing = 3
        let edit = NSButton(title: "Edit", target: self, action: #selector(editRow(_:)))
        let delete = NSButton(image: NSImage(systemSymbolName: "trash", accessibilityDescription: "Delete capture") ?? NSImage(),
                              target: self, action: #selector(deleteRow(_:)))
        for button in [edit, delete] {
            button.identifier = NSUserInterfaceItemIdentifier(entry.id.uuidString)
            button.font = .systemFont(ofSize: 16, weight: .medium)
            button.controlSize = .large
            button.bezelStyle = .rounded
        }
        delete.setAccessibilityLabel("Delete capture")
        let line = NSStackView(views: [picture, labels, NSView(), edit, delete])
        line.edgeInsets = NSEdgeInsets(top: 5, left: 8, bottom: 5, right: 8)
        line.spacing = 12
        return line
    }

}

private final class RecentTable: NSTableView {
    var key: ((UInt16) -> Bool)?
    override func keyDown(with event: NSEvent) {
        if key?(event.keyCode) != true { super.keyDown(with: event) }
    }
}
