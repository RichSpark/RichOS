import AppKit
import ApplicationServices
import Carbon.HIToolbox
import RichFilesCore

private final class WindowSwitcherPanel: NSPanel {
    var dismissPicker: (() -> Void)?
    var openSelection: (() -> Void)?

    override var canBecomeKey: Bool { true }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown,
           event.modifierFlags.intersection([.command, .option, .control, .shift]).isEmpty {
            if event.keyCode == UInt16(kVK_Escape) { dismissPicker?(); return }
            if [UInt16(kVK_Return), UInt16(kVK_ANSI_KeypadEnter)].contains(event.keyCode) {
                openSelection?()
                return
            }
        }
        super.sendEvent(event)
    }
}

final class WindowSwitcher: NSObject, NSTableViewDataSource, NSTableViewDelegate, NSWindowDelegate {
    private struct Choice: @unchecked Sendable {
        let window: AXUIElement
        let pid: pid_t
        let application: String
        let title: String
        let minimized: Bool
    }

    private let panel = WindowSwitcherPanel(
        contentRect: NSRect(x: 0, y: 0, width: 560, height: 380),
        styleMask: [.titled, .closable, .utilityWindow, .nonactivatingPanel], backing: .buffered, defer: true
    )
    private let table = NSTableView()
    private let message = NSTextField(labelWithString: "")
    private lazy var openButton = NSButton(title: "Switch to Window", target: self, action: #selector(openSelection))
    private var choices: [Choice] = []
    private var loadTask: Task<Void, Never>?
    private var loading: (id: UUID, started: TimeInterval)?
    private var activation = UUID()

    override init() {
        super.init()
        panel.title = "Switch Windows"
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = false
        panel.level = .floating
        panel.collectionBehavior = [.moveToActiveSpace, .fullScreenAuxiliary]
        panel.delegate = self
        panel.dismissPicker = { [weak self] in self?.dismiss() }
        panel.openSelection = { [weak self] in self?.openSelection() }

        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("Window"))
        column.title = "Open windows"
        column.resizingMask = .autoresizingMask
        table.addTableColumn(column)
        table.columnAutoresizingStyle = .firstColumnOnlyAutoresizingStyle
        table.headerView = nil
        table.rowHeight = 42
        table.usesAlternatingRowBackgroundColors = true
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.doubleAction = #selector(openSelection)
        table.setAccessibilityLabel("Open windows")
        let scroll = NSScrollView()
        scroll.documentView = table
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder

        message.textColor = .secondaryLabelColor
        let cancel = NSButton(title: "Cancel", target: self, action: #selector(dismiss))
        let buttons = NSStackView(views: [cancel, openButton])
        let layout = NSStackView(views: [message, scroll, buttons])
        layout.orientation = .vertical
        layout.alignment = .leading
        layout.spacing = 10
        layout.edgeInsets = NSEdgeInsets(top: 14, left: 14, bottom: 14, right: 14)
        layout.translatesAutoresizingMaskIntoConstraints = false
        let content = NSView()
        content.addSubview(layout)
        NSLayoutConstraint.activate([
            layout.topAnchor.constraint(equalTo: content.topAnchor),
            layout.bottomAnchor.constraint(equalTo: content.bottomAnchor),
            layout.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            layout.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            scroll.widthAnchor.constraint(equalTo: layout.widthAnchor, constant: -28)
        ])
        panel.contentView = content
    }

    func toggle() {
        if panel.isVisible { dismiss(); return }
        let started = ProcessInfo.processInfo.systemUptime
        let id = UUID()
        activation = id
        let activePID = NSWorkspace.shared.frontmostApplication?.processIdentifier
        let apps = NSWorkspace.shared.runningApplications
            .filter { !$0.isTerminated && $0.activationPolicy == .regular }
            .map { ($0.processIdentifier, $0.localizedName ?? "Application") }
            .sorted { left, right in
                if left.0 == activePID { return right.0 != activePID }
                if right.0 == activePID { return false }
                let order = left.1.localizedStandardCompare(right.1)
                return order == .orderedSame ? left.0 < right.0 : order == .orderedAscending
            }
        choices = []
        openButton.isEnabled = false
        table.reloadData()
        message.stringValue = "Reading open windows…"
        loading = (id, started)
        panel.center()
        // The login item may not receive app activation from a global event tap.
        // The nonactivating panel can take keyboard focus while the current app stays active.
        panel.orderFrontRegardless()
        panel.makeKey()
        panel.makeFirstResponder(table)
        loadTask = Task { @MainActor [weak self] in
            let worker = Task.detached(priority: .userInitiated) { Self.readWindows(apps) }
            let results = await withTaskCancellationHandler { await worker.value } onCancel: { worker.cancel() }
            guard !Task.isCancelled, let self, self.loading?.id == id else { return }
            self.choices = results
            self.message.stringValue = results.isEmpty ? "No open windows available." : "Choose a window"
            self.table.reloadData()
            if !results.isEmpty { self.table.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false) }
            self.openButton.isEnabled = !results.isEmpty
            self.finishLoading(results.isEmpty ? .unavailable : .complete)
        }
    }

    private static func readWindows(_ apps: [(pid_t, String)]) -> [Choice] {
        var choices: [Choice] = []
        for (pid, name) in apps {
            if Task.isCancelled { break }
            let app = AXUIElementCreateApplication(pid)
            AXUIElementSetMessagingTimeout(app, 0.08)
            let windows: [AXUIElement] = app.value(kAXWindowsAttribute) ?? []
            for window in windows {
                if Task.isCancelled { return choices }
                guard window.role == kAXWindowRole, window.subrole == kAXStandardWindowSubrole else { continue }
                let title: String? = window.value(kAXTitleAttribute)
                choices.append(Choice(window: window, pid: pid, application: name,
                                      title: title.flatMap { $0.isEmpty ? nil : $0 } ?? "Untitled window",
                                      minimized: window.minimized == true))
            }
        }
        return choices
    }

    func numberOfRows(in tableView: NSTableView) -> Int { choices.count }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        guard choices.indices.contains(row) else { return nil }
        let choice = choices[row]
        let title = NSTextField(labelWithString: choice.title)
        title.font = .systemFont(ofSize: 13, weight: .medium)
        title.lineBreakMode = .byTruncatingMiddle
        let detail = NSTextField(labelWithString: choice.application + (choice.minimized ? " · Minimized" : ""))
        detail.font = .systemFont(ofSize: 11)
        detail.textColor = .secondaryLabelColor
        detail.lineBreakMode = .byTruncatingTail
        let rowView = NSStackView(views: [title, detail])
        rowView.orientation = .vertical
        rowView.alignment = .leading
        rowView.spacing = 2
        rowView.edgeInsets = NSEdgeInsets(top: 4, left: 8, bottom: 3, right: 8)
        return rowView
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        openButton.isEnabled = choices.indices.contains(table.selectedRow)
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        dismiss()
        return false
    }

    @objc func dismiss() {
        guard panel.isVisible else { return }
        hide()
    }

    private func hide() {
        loadTask?.cancel()
        loadTask = nil
        finishLoading(.cancelled)
        choices = []
        table.reloadData()
        panel.orderOut(nil)
    }

    private func finishLoading(_ outcome: RichFilesDiagnostics.Outcome) {
        guard let sample = loading else { return }
        loading = nil
        RichFilesDiagnostics.record(.windowSwitcherList, id: sample.id, outcome: outcome,
                                    started: sample.started, total: choices.count, confirmed: choices.count)
    }

    @objc private func openSelection() {
        guard choices.indices.contains(table.selectedRow) else { return }
        let choice = choices[table.selectedRow]
        let started = ProcessInfo.processInfo.systemUptime
        guard let app = NSRunningApplication(processIdentifier: choice.pid), !app.isTerminated,
              choice.window.role == kAXWindowRole,
              choice.window.minimized != true || choice.window.set(kAXMinimizedAttribute, kCFBooleanFalse) else {
            message.stringValue = "That window could not be opened. Choose another window or reopen the picker."
            recordActivation(.unavailable, started: started)
            return
        }
        hide()
        app.activate()
        raise(choice, started: started, request: activation)
    }

    private func raise(_ choice: Choice, started: TimeInterval, request: UUID) {
        guard request == activation, !panel.isVisible else { return }
        let app = AXUIElementCreateApplication(choice.pid)
        AXUIElementSetMessagingTimeout(app, 0.08)
        if NSWorkspace.shared.frontmostApplication?.processIdentifier == choice.pid {
            _ = choice.window.perform(kAXRaiseAction)
            _ = choice.window.set(kAXMainAttribute, kCFBooleanTrue)
            if app.element(kAXFocusedWindowAttribute) == choice.window {
                recordActivation(.complete, started: started)
                return
            }
        }
        guard ProcessInfo.processInfo.systemUptime - started < 0.85 else {
            recordActivation(.issues, started: started)
            return
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(80)) { [weak self] in
            self?.raise(choice, started: started, request: request)
        }
    }

    private func recordActivation(_ outcome: RichFilesDiagnostics.Outcome, started: TimeInterval) {
        RichFilesDiagnostics.record(.windowSwitcherActivate, id: UUID(), outcome: outcome,
                                    started: started, total: 1, confirmed: outcome == .complete ? 1 : 0)
    }
}
