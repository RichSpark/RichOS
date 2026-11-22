import AppKit
import RichFilesCore
import SwiftUI

@MainActor
struct DiskCleanupView: View {
    @StateObject private var model: DiskCleanupModel
    @ObservedObject private var actions: BrowserActions
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage("richfiles.textSize") private var textSize = BrowserTextSize.standard
    @State private var raised = false
    @State private var flatRequested = false
    @State private var zoomTarget: URL?
    @State private var zoomed = false
    @State private var resetRing = UUID()
    @State private var preview: FileEntry?
    @State private var inspectorEntry: FileEntry?
    @State private var previewProblem = ""
    @State private var narrowDetail = false
    @State private var previewTask: Task<Void, Never>?
    @FocusState private var rowsFocused: Bool
    let volumes: [VolumeInfo]
    let close: () -> Void
    let openInBrowser: (URL, Bool) -> Void
    let titleChanged: (String) -> Void

    init(folder: URL, actions: BrowserActions, volumes: [VolumeInfo], close: @escaping () -> Void,
         openInBrowser: @escaping (URL, Bool) -> Void, titleChanged: @escaping (String) -> Void) {
        _model = StateObject(wrappedValue: DiskCleanupModel(folder: folder, actions: actions))
        self.actions = actions
        self.volumes = volumes
        self.close = close
        self.openInBrowser = openInBrowser
        self.titleChanged = titleChanged
    }

    private var palette: BrowserPalette { BrowserPalette(scheme) }
    private var rule: some View { Rectangle().fill(palette.line).frame(height: 1) }
    private var separator: some View { Rectangle().fill(palette.line).frame(width: 1) }
    private var title: String { model.inTrash ? "Trash" : "Disk Usage: \(model.name)" }
    private var available: String { model.availableBytes.map(formattedBytes) ?? "Unavailable" }
    private var allocated: String { model.inventory.map { formattedBytes($0.allocated) } ?? "" }
    private var scale: CGFloat { textSize.scale }
    private var canUse3D: Bool { model.scan == .complete || model.inTrash }

    var body: some View {
        GeometryReader { geometry in
            let wide = geometry.size.width >= 1080
            VStack(spacing: 0) {
                titleBar(wide: wide)
                rule
                HStack(spacing: 0) {
                    leftPanel(wide: wide)
                        .frame(width: wide ? min(425, geometry.size.width * 0.30) : geometry.size.width * 0.63)
                    separator
                    visualPanel(wide: wide)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                    if wide {
                        separator
                        rightPanel.frame(width: min(350, geometry.size.width * 0.25))
                    }
                }
                rule
                statusLine(wide: wide)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(palette.pane)
        .foregroundStyle(palette.ink)
        .font(BrowserTypography.reading(16 * scale))
        .accessibilityIdentifier("richfiles-disk-cleanup")
        .onAppear { model.startScan(); titleChanged(title) }
        .onDisappear { model.close(); previewTask?.cancel() }
        .onChange(of: title) { titleChanged($0) }
        .onChange(of: model.scan) { value in
            if value == .scanning { raised = false }
            if value == .complete && !flatRequested { raised = true }
        }
        .onChange(of: model.folder) { _ in clearZoom() }
        .onChange(of: model.filter) { _ in clearZoom() }
        .onChange(of: actions.mutationRevision) { _ in
            if model.inTrash { model.loadTrash() }
        }
        .task(id: model.selected?.id) {
            inspectorEntry = nil
            if let url = model.selected?.url {
                let worker = Task.detached(priority: .utility) { try? FileEntry(url: url) }
                let entry = await worker.value
                if !Task.isCancelled { inspectorEntry = entry }
            }
        }
        .onExitCommand {
            if model.inTrash { model.leaveTrash() }
            else { close() }
        }
        .sheet(item: $preview) { PreviewSheet(entry: $0).modifier(BrowserAppearance()) }
        .sheet(isPresented: $narrowDetail) {
            VStack(spacing: 0) {
                HStack {
                    Text(model.inTrash ? "Cleanup result" : "File details").font(.headline)
                    Spacer()
                    Button("Done") { narrowDetail = false }.keyboardShortcut(.cancelAction)
                }.padding(12)
                rule
                ScrollView { rightContents.padding(14) }
            }.frame(minWidth: 420, minHeight: 360).modifier(BrowserAppearance())
        }
        .alert("Preview unavailable", isPresented: Binding(get: { !previewProblem.isEmpty }, set: {
            if !$0 { previewProblem = "" }
        })) {
            Button("OK", role: .cancel) { previewProblem = "" }
        } message: { Text(previewProblem) }
        .sheet(isPresented: Binding(get: { !model.confirmDeletion.isEmpty }, set: {
            if !$0 { model.confirmDeletion = [] }
        })) { deletionConfirmation }
    }

    private func titleBar(wide: Bool) -> some View {
        HStack(spacing: 14) {
            Button {
                if model.inTrash { model.leaveTrash() }
                else if model.canGoUp { model.goUp() }
                else { close() }
            } label: {
                Label(model.inTrash ? "Disk Usage" : model.canGoUp ? "Parent folder" : originName,
                      systemImage: "chevron.left")
            }.buttonStyle(.plain)
                .foregroundStyle(palette.signal)
                .accessibilityIdentifier("cleanup-back")
            if wide || model.inTrash {
                Text(model.inTrash ? "Trash" : "Disk Usage")
                    .font(.system(size: 24 * scale, design: .serif))
            }
            if !model.inTrash { scopeMenu.layoutPriority(1) }
            Spacer(minLength: 5)
            if !model.inTrash, !allocated.isEmpty {
                Text("\(allocated) in \(model.name)")
                    .lineLimit(1).truncationMode(.middle).foregroundStyle(palette.hint)
                    .help("\(allocated) allocated in \(model.folder.path)")
            }
            if !wide && (model.inTrash || model.selected != nil) {
                Button(model.inTrash ? "Result" : "Details") { narrowDetail = true }
            }
        }.padding(.horizontal, 16).padding(.vertical, 12)
            .background(palette.chrome)
    }

    private var originName: String { model.origin.path == "/" ? "This Mac" : model.origin.lastPathComponent }

    private var scopeMenu: some View {
        Menu {
            Button("Current browser folder") { model.chooseScope(model.origin) }
            let home = FileManager.default.homeDirectoryForCurrentUser
            Button("Home") { model.chooseScope(home) }
            Button("Downloads") { model.chooseScope(home.appendingPathComponent("Downloads", isDirectory: true)) }
            Button("Desktop") { model.chooseScope(home.appendingPathComponent("Desktop", isDirectory: true)) }
            if !volumes.isEmpty { Divider() }
            ForEach(volumes) { volume in
                Button(volume.name) { model.chooseScope(volume.url) }
            }
            Divider()
            Button("Choose Folder…", action: chooseFolder)
        } label: {
            Label(model.name, systemImage: "folder").lineLimit(1)
        }.fixedSize()
            .accessibilityLabel("Scan location")
            .accessibilityIdentifier("cleanup-scope")
    }

    private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.directoryURL = model.folder
        panel.prompt = "Scan"
        guard let window = NSApp.keyWindow else { return }
        panel.beginSheetModal(for: window) { response in
            if response == .OK, let url = panel.url { model.chooseScope(url) }
        }
    }

    private func leftPanel(wide: Bool) -> some View {
        VStack(spacing: 0) {
            if model.inTrash {
                trashPanel
            } else {
                HStack(spacing: 0) {
                    tab("Results", selected: !model.reviewing, id: "cleanup-results") { model.reviewing = false }
                    tab("Review \(model.batch.count)", selected: model.reviewing, id: "cleanup-review") { model.reviewing = true }
                    Spacer()
                }.padding(10)
                rule
                if model.reviewing {
                    HStack {
                        reviewTotal
                        Spacer()
                        moveButton
                    }.padding(10)
                    rule
                } else {
                    filters
                    rule
                    scanLine
                    rule
                }
                if !model.notice.isEmpty {
                    Text(model.notice).foregroundStyle(palette.hint)
                        .frame(maxWidth: .infinity, alignment: .leading).padding(10)
                    rule
                }
                if !model.inTrash && !model.moved.isEmpty {
                    HStack {
                        Button("Undo Move") { model.undoMove() }.disabled(!model.ready)
                        Spacer()
                        Button("Review in Trash") { model.showTrash() }
                            .accessibilityIdentifier("cleanup-review-trash")
                    }.padding(10)
                    rule
                }
                if model.filter == .duplicates && !model.reviewing { duplicatePanel }
                else { resultList }
                if !wide, let selected = model.selected {
                    rule
                    HStack {
                        Button("Preview") { showPreview(selected) }.disabled(selected.isCloudOnly)
                        Button("Open in Browser") { openItem(selected) }
                        Spacer()
                    }.padding(10)
                }
            }
        }.frame(maxHeight: .infinity)
    }

    private func tab(_ text: String, selected: Bool, id: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(text).lineLimit(1).padding(.horizontal, 10).padding(.vertical, 6)
                .frame(maxWidth: .infinity)
                .background(selected ? palette.selected : palette.chrome)
                .overlay(alignment: .bottom) {
                    Rectangle().fill(selected ? palette.signal : palette.controlLine.opacity(0.55)).frame(height: selected ? 2 : 1)
                }
        }.buttonStyle(.plain)
            .accessibilityAddTraits(selected ? .isSelected : [])
            .accessibilityIdentifier(id)
    }

    private var filters: some View {
        VStack(alignment: .leading, spacing: 9) {
            VStack(spacing: 1) {
                ForEach([[DiskCleanupModel.Filter.largest, .files], [.installers, .duplicates]], id: \.self) { pair in
                    HStack(spacing: 1) {
                        ForEach(pair, id: \.self) { filter in
                            let count = model.entries(filter).count
                            let label = filter == .duplicates || model.inventory == nil ? filter.rawValue : "\(filter.rawValue) \(count)"
                            tab(label, selected: model.filter == filter, id: "cleanup-filter-\(filter.rawValue)") {
                                model.chooseFilter(filter)
                            }
                        }
                    }
                }
            }.background(palette.controlLine)
                .clipShape(RoundedRectangle(cornerRadius: 7))
                .overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(palette.controlLine))
            if model.filter != .duplicates {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 5) { sizeFilter; ageFilter }
                    VStack(alignment: .leading, spacing: 5) { sizeFilter; ageFilter }
                }
            }
        }.padding(10)
    }

    private var sizeFilter: some View {
        Picker("Minimum size", selection: $model.minimumMB) {
            Text("Larger than 10 MB").tag(10)
            Text("Larger than 100 MB").tag(100)
            Text("Larger than 1 GB").tag(1000)
        }.labelsHidden().fixedSize().accessibilityLabel("Minimum size")
    }

    private var ageFilter: some View {
        Picker("Last modified", selection: $model.minimumDays) {
            Text("Modified any time").tag(0)
            Text("Not modified in 30 days").tag(30)
            Text("Not modified in 6 months").tag(180)
            Text("Not modified in 1 year").tag(365)
        }.labelsHidden().fixedSize().accessibilityLabel("Last modified")
    }

    private var scanLine: some View {
        HStack(alignment: .top, spacing: 6) {
            Group {
                switch model.scan {
                case .scanning:
                    ProgressView().controlSize(.small).accessibilityHidden(true)
                    Text(model.inventory.map { "Measuring… \($0.visited) items" } ?? "Measuring files…")
                case .complete:
                    Image(systemName: "checkmark").foregroundStyle(palette.signal)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(model.inventory.map { "Scan complete · \($0.visited) items" } ?? "Scan complete")
                        if let report = model.inventory {
                            if report.unavailable > 0 {
                                Text("\(report.unavailable) unavailable or cloud-only items skipped")
                            }
                            if report.otherVolumes > 0 {
                                Text("\(report.otherVolumes) mounts on other volumes skipped")
                            }
                        }
                    }
                case .cancelled:
                    Text("Scan canceled. Results are partial.")
                case .unavailable:
                    Image(systemName: "exclamationmark.triangle").foregroundStyle(palette.attention)
                    Text("This location is unavailable.")
                }
            }.foregroundStyle(palette.hint)
            Spacer(minLength: 0)
            if model.scan == .scanning {
                Button("Cancel") { model.cancelScan() }
            } else {
                Button("Scan Again") { model.startScan(fresh: true) }
                    .accessibilityIdentifier("cleanup-rescan")
            }
        }.padding(10)
    }

    private var reviewTotal: some View {
        Text("\(formattedBytes(model.batchBytes)) · \(model.batch.count) in review")
            .fontWeight(.semibold).lineLimit(1)
    }

    private var moveButton: some View {
        Button { model.moveToTrash() } label: { Label("Move to Trash", systemImage: "trash") }
            .buttonStyle(.borderedProminent)
            .disabled(!model.ready || !model.batch.contains { !$0.changed })
            .accessibilityIdentifier("cleanup-move-trash")
    }

    private var resultList: some View {
        ScrollViewReader { reader in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if case .unavailable(let reason) = model.scan, !model.reviewing {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("\(model.name) is unavailable").fontWeight(.semibold)
                            Text(reason).foregroundStyle(palette.hint).textSelection(.enabled)
                            Button("Choose Folder…", action: chooseFolder)
                        }.padding(13)
                    } else if model.displayed.isEmpty {
                        Text(emptyResults).foregroundStyle(palette.hint).padding(13)
                    }
                    ForEach(model.displayed) { item in
                        resultRow(item).id(item.url)
                        rule
                    }
                    if model.inventory?.limited == true && !model.reviewing && model.filter != .largest {
                        Text("Showing the largest measured files. Choose a smaller folder to see more.")
                            .foregroundStyle(palette.hint).padding(13)
                    }
                }.frame(maxWidth: .infinity, alignment: .leading)
            }
            .focusable()
            .focused($rowsFocused)
            .onMoveCommand { direction in
                switch direction {
                case .up: moveSelection(-1)
                case .down: moveSelection(1)
                case .left: model.goUp()
                case .right: if let selected = model.selected { model.drill(selected) }
                @unknown default: break
                }
            }
            .onChange(of: model.selected?.id) { id in
                if let id { reader.scrollTo(id) }
            }
            .background {
                if rowsFocused, let selected = model.selected {
                    HStack {
                        Button("Preview selected item") { showPreview(selected) }
                            .keyboardShortcut(.space, modifiers: [])
                        Button("Toggle selected item in review") { model.toggle(selected) }
                            .keyboardShortcut(.return, modifiers: [])
                    }.frame(width: 0, height: 0).opacity(0).accessibilityHidden(true)
                }
            }
        }
    }

    private var emptyResults: String {
        if model.reviewing { return "Items you add to review appear here." }
        if model.scan == .scanning { return "Waiting for the first measured items…" }
        if model.filter == .largest { return "No items were measured in this folder." }
        return model.filter == .installers ? "No installers match these filters." : "No files match these filters."
    }

    private func moveSelection(_ offset: Int) {
        let rows = model.displayed
        guard !rows.isEmpty else { return }
        let next = model.selected.flatMap { item in rows.firstIndex { $0.id == item.id } }
            .map { max(0, min(rows.count - 1, $0 + offset)) } ?? 0
        model.selected = rows[next]
    }

    private func resultRow(_ item: CleanupCandidate) -> some View {
        let entry = model.batch.first { $0.id == item.id }
        return HStack(spacing: 9) {
            Button {
                model.selected = item
                rowsFocused = true
            } label: {
                HStack(alignment: .top, spacing: 9) {
                    Image(systemName: symbol(item.kind))
                        .foregroundStyle(Color(nsColor: CleanupRingSlice.color(item.kind, dark: scheme == .dark)))
                        .frame(width: 18).accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(item.url.lastPathComponent).fontWeight(.semibold).lineLimit(1)
                            .truncationMode(.middle)
                        Text(item.url.deletingLastPathComponent().path).foregroundStyle(palette.hint)
                            .lineLimit(1).truncationMode(.middle)
                        if model.reviewing {
                            Text(reason(item)).foregroundStyle(palette.hint).lineLimit(2)
                        }
                    }
                    Spacer(minLength: 0)
                    Text(formattedBytes(item.allocated)).monospacedDigit().fontWeight(.semibold)
                }.frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
            }.buttonStyle(.plain)
                .help(item.url.path)
                .accessibilityAddTraits(model.selected?.id == item.id ? .isSelected : [])
                .accessibilityIdentifier("cleanup-row-\(item.url.lastPathComponent)")
            if let entry, entry.changed {
                Button("Review Again") { model.reviewAgain(entry) }.disabled(!model.ready)
            } else if item.isFolder {
                iconButton("Look inside \(item.url.lastPathComponent)", "chevron.right") { model.drill(item) }
            }
            if item.canReview {
                let added = entry != nil
                iconButton(added ? "Remove \(item.url.lastPathComponent) from review" :
                    "Add \(item.url.lastPathComponent) to review", added ? "xmark" : "plus") { model.toggle(item) }
                    .disabled(added ? !model.ready : !model.canAdd(item))
            }
        }.padding(.horizontal, 11).padding(.vertical, 8)
            .background(model.selected?.id == item.id ? palette.inactiveSelection : Color.clear)
    }

    private func iconButton(_ title: String, _ symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) { Image(systemName: symbol).frame(width: 16, height: 16) }
            .help(title).accessibilityLabel(title)
    }

    private var duplicatePanel: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 10) {
                if model.duplicateState == .scanning {
                    HStack {
                        ProgressView().controlSize(.small)
                        Text("Comparing file contents…")
                        Spacer()
                        Button("Cancel") { model.cancelDuplicates() }
                    }
                } else {
                    Button(model.duplicateState == nil ? "Find Duplicates" : "Scan Duplicates Again") { model.findDuplicates() }
                        .accessibilityIdentifier("cleanup-find-duplicates")
                }
                if case .unavailable(let reason) = model.duplicateState { Text(reason).foregroundStyle(palette.hint) }
                if model.duplicateState == .cancelled { Text("Duplicate scan canceled.").foregroundStyle(palette.hint) }
                if let report = model.duplicates {
                    if report.groups.isEmpty { Text("No exact duplicates found in the files checked.") }
                    if report.unavailable + report.otherVolumes > 0 {
                        Text("\(report.unavailable + report.otherVolumes) items or mounts were skipped.")
                            .foregroundStyle(palette.hint)
                    }
                    ForEach(report.groups) { group in
                        VStack(alignment: .leading, spacing: 4) {
                            Text("\(group.files.count) identical copies · \(formattedBytes(group.size)) each").fontWeight(.semibold)
                            ForEach(group.candidates) { candidate in
                                resultRow(candidate)
                                HStack {
                                    if model.keeper(group) == candidate.url {
                                        Label("Kept", systemImage: "checkmark.shield").foregroundStyle(palette.signal)
                                    } else if candidate.canReview {
                                        Button("Keep This Copy") { model.keep(candidate, in: group) }.disabled(!model.ready)
                                    }
                                }.padding(.leading, 12)
                            }
                        }.padding(8).overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(palette.line))
                    }
                }
            }.padding(10)
        }
    }

    private var trashPanel: some View {
        VStack(alignment: .leading, spacing: 0) {
            if !model.notice.isEmpty {
                Text(model.notice).foregroundStyle(palette.hint).padding(10)
                rule
            }
            if model.batch.contains(where: \.changed) {
                Button("Review Changed Items") { model.leaveTrash(); model.reviewing = true }.padding(10)
                rule
            }
            HStack {
                Label("Trash on \(model.volumeName)", systemImage: "trash").fontWeight(.semibold)
                Spacer()
                Button("Refresh") { model.loadTrash() }.disabled(!model.ready)
            }.padding(10)
            rule
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if !model.trashStatus.isEmpty { Text(model.trashStatus).foregroundStyle(palette.hint).padding(10) }
                    let recent = model.trashEntries.filter(isFromCleanup)
                    let other = model.trashEntries.filter { !isFromCleanup($0) }
                    Text(recent.isEmpty ? "Nothing from this cleanup is left in Trash." : "From this cleanup")
                        .foregroundStyle(palette.hint).padding(10)
                    ForEach(recent) { trashRow($0); rule }
                    if !other.isEmpty {
                        Text("Already in Trash").foregroundStyle(palette.hint).padding(10)
                        ForEach(other) { trashRow($0); rule }
                    }
                }
            }
            rule
            VStack(alignment: .leading, spacing: 7) {
                Text("\(model.selectedTrash.count) selected\(sizeOf(model.selectedTrash).map { " · \(formattedBytes($0)) allocated" } ?? "")")
                    .fontWeight(.semibold)
                Button("Delete Selected…", role: .destructive) { model.confirmDeletion = model.selectedTrash }
                    .foregroundStyle(palette.danger)
                    .disabled(model.selectedTrash.isEmpty || !model.ready)
                    .accessibilityIdentifier("cleanup-delete-selected")
            }.padding(10)
        }
    }

    private func isFromCleanup(_ entry: TrashListEntry) -> Bool {
        model.moved.contains { $0.receipt.id == entry.known?.id }
    }

    private func trashRow(_ entry: TrashListEntry) -> some View {
        HStack(alignment: .center, spacing: 9) {
            Toggle(isOn: Binding(get: { model.trashSelection.contains(entry.id) }, set: { selected in
                if selected { model.trashSelection.insert(entry.id) }
                else { model.trashSelection.remove(entry.id) }
            })) { Text(entry.name).lineLimit(1).truncationMode(.middle) }
                .toggleStyle(.checkbox)
                .disabled(!entry.canDelete || entry.reference == nil || !model.ready)
                .accessibilityIdentifier("cleanup-trash-\(entry.name)")
            Spacer(minLength: 0)
            if let bytes = model.trashSizes[entry.id] { Text(formattedBytes(bytes)).monospacedDigit() }
            if entry.known?.canRestore == true {
                Button("Put Back") { model.restore([entry]) }.disabled(!model.ready)
                    .accessibilityLabel("Put back \(entry.name)")
            } else if entry.reference != nil {
                Button("Restore To…") { chooseRestoreFolder(entry) }.disabled(!model.ready)
            } else {
                Text("Find in Finder Trash").foregroundStyle(palette.hint)
            }
        }.padding(10)
            .help(entry.originalFolder ?? "Original folder unavailable")
            .background(model.trashSelection.contains(entry.id) ? palette.inactiveSelection : Color.clear)
    }

    private func chooseRestoreFolder(_ entry: TrashListEntry) {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.prompt = "Restore"
        guard let window = NSApp.keyWindow else { return }
        panel.beginSheetModal(for: window) { response in
            if response == .OK, let url = panel.url { actions.restoreFromTrash(entry, into: url) }
        }
    }

    private func sizeOf(_ entries: [TrashListEntry]) -> Int64? {
        let sizes = entries.compactMap { model.trashSizes[$0.id] }
        return sizes.count == entries.count ? DiskCleanupModel.total(sizes) : nil
    }

    private var deletionConfirmation: some View {
        VStack(alignment: .leading, spacing: 12) {
            let chosen = model.confirmDeletion
            Text("Permanently delete \(chosen.count) selected \(chosen.count == 1 ? "item" : "items")?")
                .font(.system(size: 21, weight: .semibold))
            if let bytes = sizeOf(chosen) {
                Text("\(formattedBytes(bytes)) allocated on \(model.volumeName)")
            }
            Text("These items cannot be recovered from Trash. \(max(0, model.trashEntries.count - chosen.count)) other items will stay in Trash.")
                .foregroundStyle(palette.hint)
            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(chosen) { entry in
                        Text(entry.name).lineLimit(1).help(entry.originalFolder ?? entry.name)
                    }
                }.frame(maxWidth: .infinity, alignment: .leading)
            }.frame(minHeight: 40, idealHeight: 100, maxHeight: 170)
            HStack {
                Spacer()
                Button("Cancel") { model.confirmDeletion = [] }.keyboardShortcut(.cancelAction)
                Button("Delete Permanently", role: .destructive) { model.deleteConfirmed() }
                    .foregroundStyle(palette.danger)
                    .accessibilityIdentifier("cleanup-confirm-delete")
            }
        }.padding(20).frame(width: 510)
            .font(BrowserTypography.reading(16 * scale))
            .background(palette.floating)
    }

    private func visualPanel(wide: Bool) -> some View {
        VStack(spacing: 6) {
            HStack(alignment: .top, spacing: 5) {
                HStack(spacing: 0) {
                    tab("2D", selected: !raised, id: "cleanup-ring-2d") { flatRequested = true; raised = false; clearZoom() }
                    tab("3D", selected: raised, id: "cleanup-ring-3d") { flatRequested = false; raised = true }
                        .disabled(!canUse3D)
                }.fixedSize()
                Spacer(minLength: 0)
                VStack(alignment: .trailing, spacing: 2) {
                    Text(available).font(.system(size: (wide ? 31 : 20) * scale, weight: .semibold)).monospacedDigit()
                        .lineLimit(1).minimumScaleFactor(0.65)
                        .accessibilityIdentifier("cleanup-available-space")
                    if wide {
                        if let change = model.observedChange, model.inTrash {
                            Text("available · \(signedBytes(change)) observed").foregroundStyle(palette.hint)
                        } else if let total = model.space?.volume.totalBytes, !model.inTrash {
                            Text("available of \(formattedBytes(total))").foregroundStyle(palette.hint)
                        } else { Text("available").foregroundStyle(palette.hint) }
                    }
                }
            }.padding(.horizontal, 12).padding(.top, 12)
            DiskCleanupRing(slices: ringSlices, raised: raised && canUse3D, dark: scheme == .dark,
                            selected: model.selected?.id, resetZoom: resetRing,
                            available: available, allocation: allocated.isEmpty ? "Available" : "\(allocated) measured",
                            reduceMotion: reduceMotion,
                            onSelect: { if let item = find($0) { model.selected = item } },
                            onOpen: { if let item = find($0) { model.drill(item) } },
                            onUp: model.goUp,
                            onPreview: { if let item = find($0) { showPreview(item) } },
                            onReview: { if let item = find($0) { model.toggle(item) } },
                            onZoom: { zoomTarget = $0; zoomed = $1 })
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(RadialGradient(colors: [palette.floating, palette.pane], center: .center,
                                           startRadius: 8, endRadius: wide ? 395 : 175))
                .overlay(alignment: .bottomTrailing) {
                    if zoomed {
                        VStack(alignment: .leading, spacing: 6) {
                            if let zoomTarget, let item = find(zoomTarget) {
                                Text(item.url.lastPathComponent).fontWeight(.semibold).lineLimit(1)
                                Button("Preview") { showPreview(item) }.disabled(item.isCloudOnly)
                                if item.isFolder { Button("Look Inside") { model.drill(item) } }
                                else {
                                    Button(model.contains(item) ? "Remove from Review" : "Add to Review") { model.toggle(item) }
                                        .disabled(!model.contains(item) && !model.canAdd(item))
                                }
                                Button("Open in Browser") { openItem(item) }
                            }
                            Button("Reset Zoom", action: clearZoom)
                        }.padding(10).background(palette.floating, in: RoundedRectangle(cornerRadius: 7))
                            .overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(palette.controlLine))
                            .padding(10)
                    }
                }
            if model.inTrash {
                Text("Blocks in Trash still stand. Deleted blocks sink.")
                    .fontWeight(.semibold).multilineTextAlignment(.center).padding(10)
            } else if wide {
                VStack(spacing: 6) {
                    Text("Wide is big. Tall is old. Color is kind.").fontWeight(.semibold)
                    ViewThatFits(in: .horizontal) {
                        HStack(spacing: 7) { legend([.folder, .video, .installer, .archive, .document, .image]) }
                        VStack(spacing: 5) {
                            HStack(spacing: 7) { legend([.folder, .video, .installer]) }
                            HStack(spacing: 7) { legend([.archive, .document, .image]) }
                        }
                    }
                    if raised {
                        Text("Drag to turn. Scroll over a block to zoom. Scroll farther to look inside a folder.")
                            .foregroundStyle(palette.hint).multilineTextAlignment(.center)
                    }
                }.padding(10)
            }
        }
    }

    private func legend(_ kinds: [CleanupCandidate.Kind]) -> some View {
        ForEach(kinds, id: \.self) { kind in
            HStack(spacing: 4) {
                RoundedRectangle(cornerRadius: 2)
                    .fill(Color(nsColor: CleanupRingSlice.color(kind, dark: scheme == .dark)))
                    .frame(width: 10, height: 10).accessibilityHidden(true)
                Text(kind.title).foregroundStyle(palette.hint)
            }
        }
    }

    private var ringSlices: [CleanupRingSlice] {
        let special = model.inTrash ? model.moved.map { slice($0.candidate, .trash) } +
            model.deleted.map { slice($0, .deleted) } : []
        let specialIDs = Set(special.map(\.id))
        let live = model.entries(model.filter == .duplicates ? .largest : model.filter).filter { !specialIDs.contains($0.id) }
        var seen = specialIDs
        let priority = model.batch.map(\.candidate) + (model.selected.map { [$0] } ?? []) + Array(live.prefix(17))
        let visible = priority.filter { seen.insert($0.id).inserted }.prefix(22)
        return special + visible.sorted { $0.allocated > $1.allocated }
            .map { slice($0, model.contains($0) ? .reviewed : .live) }
    }

    private func slice(_ candidate: CleanupCandidate, _ state: CleanupRingSlice.Disposition) -> CleanupRingSlice {
        CleanupRingSlice(id: candidate.id, name: candidate.url.lastPathComponent, kind: candidate.kind,
                         bytes: candidate.allocated, days: age(candidate), folder: candidate.isFolder, disposition: state)
    }

    private func find(_ id: URL) -> CleanupCandidate? {
        model.displayed.first { $0.id == id }
            ?? model.batch.first { $0.id == id }?.candidate
            ?? model.entries(model.filter == .duplicates ? .largest : model.filter).first { $0.id == id }
    }

    private func clearZoom() {
        resetRing = UUID()
        zoomTarget = nil
        zoomed = false
    }

    private var rightPanel: some View {
        VStack(spacing: 0) {
            ScrollView { rightContents.padding(13) }
            if !model.inTrash {
                Rectangle().fill(palette.signal).frame(height: 1)
                VStack(alignment: .leading, spacing: 8) {
                    reviewTotal
                    if model.batch.isEmpty {
                        Text("Select a file or block, then add it to review.").foregroundStyle(palette.hint)
                    } else {
                        ForEach(Array(model.batch.prefix(4))) { entry in
                            HStack {
                        Text(entry.candidate.url.lastPathComponent).lineLimit(1).truncationMode(.middle)
                                Spacer(minLength: 2)
                                iconButton("Remove \(entry.candidate.url.lastPathComponent) from review", "xmark") { model.remove(entry.id) }
                            }
                        }
                    }
                    ViewThatFits(in: .horizontal) {
                        HStack {
                            Button(model.reviewing ? "Back to Results" : "Paths and Reasons") { model.reviewing.toggle() }
                            moveButton
                        }
                        VStack(alignment: .leading) {
                            Button(model.reviewing ? "Back to Results" : "Paths and Reasons") { model.reviewing.toggle() }
                            moveButton
                        }
                    }
                }.padding(12).frame(maxWidth: .infinity, alignment: .leading).background(palette.floating)
            }
        }
    }

    private var rightContents: some View {
        VStack(alignment: .leading, spacing: 10) {
            if model.inTrash {
                Text(model.deletedAllocated != nil || model.observedChange != nil ? "Space released" : "In Trash")
                    .font(.system(size: 23 * scale, design: .serif))
                if let bytes = model.deletedAllocated { fact("Deleted (allocated)", formattedBytes(bytes)) }
                if !model.moved.isEmpty { fact("In Trash, still on the disk", formattedBytes(model.trashBytes)) }
                if let before = model.availableBeforeDeletion {
                    fact("Available before clearing", formattedBytes(before))
                    fact("Available now", available)
                }
                rule
                fact("Observed change", model.measuringSpace ? "Measuring…" : model.observedChange.map(signedBytes) ?? "None measured yet")
                if model.observedChange != nil {
                    Text("Shared blocks, snapshots and other disk activity can affect the measured change.")
                        .foregroundStyle(palette.hint)
                } else {
                    Text("Moving items to Trash keeps them recoverable and leaves them on the disk.")
                        .foregroundStyle(palette.hint)
                }
                if model.moved.contains(where: { $0.receipt.canRestore }) {
                    Button("Undo Move") { model.undoMove() }.disabled(!model.ready)
                }
                Button("Back to Disk Usage") { narrowDetail = false; model.leaveTrash() }
            } else if let item = model.selected {
                HStack {
                    if let inspectorEntry, inspectorEntry.url == item.url {
                        FileThumbnail(entry: inspectorEntry, side: 88)
                    } else {
                        Image(systemName: symbol(item.kind)).font(.system(size: 54))
                            .foregroundStyle(Color(nsColor: CleanupRingSlice.color(item.kind, dark: scheme == .dark)))
                            .accessibilityHidden(true)
                    }
                    Spacer()
                    Button("Preview") { showPreview(item) }.disabled(item.isCloudOnly)
                }.padding(15).frame(maxWidth: .infinity, minHeight: 117).background(palette.floating,
                    in: RoundedRectangle(cornerRadius: 8))
                Text(item.url.lastPathComponent).fontWeight(.semibold).textSelection(.enabled)
                fact("Path", item.url.path)
                fact("Allocated", formattedBytes(item.allocated))
                fact("Modified", item.modified.formatted(date: .abbreviated, time: .omitted))
                fact("Why it’s here", reason(item))
                if item.isFolder { Button("Look Inside") { model.drill(item) } }
                if item.canReview {
                    Button(model.contains(item) ? "Remove from Review" : "Add to Review") { model.toggle(item) }
                        .disabled(model.contains(item) ? !model.ready : !model.canAdd(item))
                }
                Button("Open in Browser") { openItem(item) }
            } else {
                Text("Select a result to see its preview, full path, allocated size and why it surfaced.")
                    .foregroundStyle(palette.hint)
            }
        }.frame(maxWidth: .infinity, alignment: .leading)
    }

    private func fact(_ name: String, _ value: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(name).foregroundStyle(palette.hint)
            Spacer(minLength: 0)
            Text(value).multilineTextAlignment(.trailing).textSelection(.enabled).layoutPriority(1)
        }
    }

    private func statusLine(wide: Bool) -> some View {
        HStack {
            Text(model.inTrash ? "Trash · \(model.volumeName)" : model.folder.path)
                .lineLimit(1).truncationMode(.middle).textSelection(.enabled)
            Spacer()
            if wide && !model.inTrash {
                Text("↑↓ Move · Space Preview · Return Review · → Look Inside · Esc Close").lineLimit(1)
            }
        }.font(BrowserTypography.reading(14 * scale)).foregroundStyle(palette.hint)
            .padding(.horizontal, 12).padding(.vertical, 6).background(palette.chrome)
    }

    private func reason(_ item: CleanupCandidate) -> String {
        if item.isCloudOnly { return "Cloud-only item. Local contents were skipped." }
        if item.isFolder { return "Folder total. Look inside to review individual files." }
        if !item.canReview { return "Measured for context. Use the owning app or location to manage it." }
        if model.group(for: item) != nil { return "An exact copy of a file in another location." }
        if item.kind == .installer { return "Installer · modified \(age(item)) days ago" }
        return "\(formattedBytes(item.allocated)) allocated · modified \(age(item)) days ago"
    }

    private func age(_ candidate: CleanupCandidate) -> Int {
        max(0, Calendar.current.dateComponents([.day], from: candidate.modified, to: Date()).day ?? 0)
    }

    private func signedBytes(_ bytes: Int64) -> String {
        if bytes == 0 { return "0 bytes" }
        return (bytes > 0 ? "+" : "−") + formattedBytes(abs(bytes))
    }

    private func symbol(_ kind: CleanupCandidate.Kind) -> String {
        switch kind {
        case .folder: "folder"
        case .video: "film"
        case .installer: "shippingbox"
        case .archive: "archivebox"
        case .image: "photo"
        case .document: "doc"
        }
    }

    private func openItem(_ item: CleanupCandidate) {
        openInBrowser(item.url, item.isFolder)
    }

    private func showPreview(_ item: CleanupCandidate) {
        guard !item.isCloudOnly else { return }
        previewTask?.cancel()
        let worker = Task.detached(priority: .userInitiated) { Result { try FileEntry(url: item.url) } }
        previewTask = Task {
            let result = await worker.value
            guard !Task.isCancelled else { return }
            switch result {
            case .success(let entry): preview = entry
            case .failure(let error): previewProblem = error.localizedDescription
            }
        }
    }
}
