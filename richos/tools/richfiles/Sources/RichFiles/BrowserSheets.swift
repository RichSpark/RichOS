import AppKit
import QuickLookUI
import RichFilesCore
import SwiftUI
import UniformTypeIdentifiers

struct NameRequest {
    struct Template: Sendable {
        let file: ReviewedFile
        let destination: BrowserTransferDestination
    }

    let source: FileReference?
    let folder: URL
    let directory: Bool
    let name: String
    var template: Template? = nil
    var expectedFolder: FileIdentity? = nil
}

enum BrowserSheet: Identifiable {
    case name(NameRequest)
    case properties(FileEntry)
    case preview(FileEntry)
    case openWith(FileEntry, WorkspaceApplications)
    case archive(ArchiveCatalog)
    case createZip([FileReference], URL)
    case diskUsage(URL)
    case checksum(FileReference)
    case duplicates(URL)
    case compare(URL)
    case fileShelf(BrowserTransferDestination)
    case trashFailures([FileReference])
    case operationIssues([String])
    case interruptedTransfer
    case trash, searchOptions, batch([FileReference]), connect, workspaces, actionPalette, settings

    var id: String {
        switch self {
        case .name: return "name"
        case .properties: return "properties"
        case .preview: return "preview"
        case .openWith: return "openWith"
        case .archive: return "zip"
        case .createZip: return "createZip"
        case .diskUsage: return "diskUsage"
        case .checksum: return "checksum"
        case .duplicates: return "duplicates"
        case .compare: return "compare"
        case .fileShelf: return "fileShelf"
        case .trashFailures: return "trashFailures"
        case .operationIssues: return "operationIssues"
        case .trash: return "trash"
        case .interruptedTransfer: return "interrupted-transfer"
        case .searchOptions: return "searchOptions"
        case .batch: return "batch"
        case .connect: return "connect"
        case .workspaces: return "workspaces"
        case .actionPalette: return "actionPalette"
        case .settings: return "settings"
        }
    }
}

@MainActor
struct ActionPaletteSheet: View {
    let commands: [BrowserAction]
    let shortcutLabels: [BrowserCommand: String]
    let run: (BrowserCommand) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var selection: BrowserCommand?
    @FocusState private var searchFocused: Bool

    private var results: [BrowserAction] {
        let words = query.split(whereSeparator: \.isWhitespace)
        return commands.filter { entry in
            words.allSatisfy { entry.title.localizedStandardContains(String($0)) }
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            TextField("Find a file action", text: $query).focused($searchFocused)
                .accessibilityLabel("Find a file action")
                .onSubmit(runSelection)
            List(results, selection: $selection) { action in
                HStack {
                    Text(action.title)
                    Spacer()
                    if let label = shortcutLabels[action.command] {
                        Text(label).font(.caption).foregroundStyle(.secondary)
                    }
                }.tag(action.id)
                    .onTapGesture(count: 2) { activate(action.command) }
            }.accessibilityLabel("File actions")
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Run", action: runSelection).keyboardShortcut(.defaultAction).disabled(results.isEmpty)
            }
        }.padding(14).frame(width: 440, height: 335)
            .onAppear { searchFocused = true; selection = results.first?.id }
            .onChange(of: query) { _ in selection = results.first?.id }
            .onMoveCommand { direction in
                guard direction == .up || direction == .down,
                      let first = results.first else { return }
                guard let index = results.firstIndex(where: { $0.id == selection }) else {
                    selection = first.id
                    return
                }
                let next = min(max(0, index + (direction == .down ? 1 : -1)), results.count - 1)
                selection = results[next].id
            }
    }

    private func runSelection() {
        if let selected = selection ?? results.first?.id { activate(selected) }
    }

    private func activate(_ action: BrowserCommand) {
        run(action)
        dismiss()
    }
}

@MainActor
struct FileShelfSheet: View {
    @ObservedObject var actions: BrowserActions
    let destination: BrowserTransferDestination
    let readOnly: Bool
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("File Shelf").font(.headline)
            if actions.shelf.isEmpty {
                Text("Use Add to File Shelf on files you want to collect.").foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List(actions.shelf, id: \.url) { item in
                    HStack {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(item.url.lastPathComponent)
                            Text(item.url.deletingLastPathComponent().path)
                                .font(.caption).foregroundStyle(.secondary)
                                .lineLimit(1).truncationMode(.head).textSelection(.enabled)
                        }
                        Spacer()
                        Button("Remove") { actions.removeFromShelf(item) }
                            .accessibilityLabel("Remove \(item.url.lastPathComponent) from File Shelf")
                    }
                }.accessibilityLabel("Collected files")
            }
            LabeledContent("Destination", value: destination.url.path)
                .lineLimit(1).truncationMode(.head)
            HStack {
                Button("Clear Shelf") { actions.clearShelf() }.disabled(actions.shelf.isEmpty)
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Copy Here") { transfer(move: false) }.disabled(transferDisabled)
                Button("Move Here") { transfer(move: true) }.disabled(transferDisabled)
            }
        }.padding(16).frame(width: 560, height: 365)
    }

    private var transferDisabled: Bool { actions.shelf.isEmpty || !actions.canScheduleTransfer || readOnly }

    private func transfer(move: Bool) {
        actions.transferShelf(to: destination, move: move)
        dismiss()
    }
}

@MainActor
struct CompareFoldersSheet: View {
    let first: URL
    @ObservedObject var actions: BrowserActions
    let openFolder: (URL) -> Void
    let mergeIssue: BrowserTransferIssue?
    @Environment(\.dismiss) private var dismiss
    @State private var second: URL?
    @State private var mode = FolderComparisonMode.metadata
    @State private var request: Request?
    @State private var report: FolderComparisonReport?
    @State private var problem = ""
    @State private var showSyncOptions = false
    @State private var syncOptions = FolderSyncPlan.Options()
    @State private var plan: FolderSyncPlan?
    @State private var confirmSync = false
    @State private var applying: Task<Void, Never>?
    @State private var applied = false

    private struct Request: Hashable {
        let id = UUID()
        let folder: URL
        let mode: FolderComparisonMode
        let sync: FolderSyncPlan.Options?
    }

    init(first: URL, actions: BrowserActions, openFolder: @escaping (URL) -> Void,
         mergeIssue: BrowserTransferIssue? = nil) {
        self.first = first
        self.actions = actions
        self.openFolder = openFolder
        self.mergeIssue = mergeIssue
        if let existing = mergeIssue?.conflict?.merge?.existing.url {
            _second = State(initialValue: existing)
            _request = State(initialValue: Request(folder: existing, mode: .contents, sync: .init()))
        }
    }

    private var merging: Bool { mergeIssue != nil }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(merging ? "Merge Folders" : "Compare Folders").font(.headline)
            LabeledContent("First Folder", value: first.path)
                .lineLimit(1).truncationMode(.middle)
            HStack {
                LabeledContent("Second Folder", value: second?.path ?? "Choose a folder")
                    .lineLimit(1).truncationMode(.middle)
                if !merging { Button("Choose…") { chooseFolder() }.disabled(applying != nil) }
            }
            if merging {
                Text("Copy missing items into the second folder. Existing files and the first folder stay in place.")
                    .font(.callout).foregroundStyle(.secondary)
            } else {
                HStack {
                    Picker("Compare by", selection: $mode) {
                        ForEach(FolderComparisonMode.allCases) { Text($0.rawValue).tag($0) }
                    }
                    Button("Compare") { preview(sync: nil) }.disabled(!canPreview)
                }.disabled(applying != nil)
                DisclosureGroup("Sync First Folder to Second", isExpanded: $showSyncOptions) {
                    HStack {
                        Toggle("Replace differing files", isOn: $syncOptions.replaceDifferentFiles)
                        Toggle("Trash destination extras", isOn: $syncOptions.trashDestinationExtras)
                        Spacer()
                        Button("Preview Sync") { preview(sync: syncOptions) }.disabled(!canPreview)
                    }.padding(.vertical, 3)
                }.disabled(applying != nil)
            }
            if let plan {
                Text("\(plan.changeCount) proposed changes in the second folder")
                List(plan.rows) { row in
                    VStack(alignment: .leading, spacing: 3) {
                        Text(row.path).lineLimit(1).truncationMode(.middle).help(row.path)
                        Text(row.action.rawValue).font(.caption).foregroundStyle(.secondary)
                        if !row.detail.isEmpty { Text(row.detail).font(.caption).foregroundStyle(.secondary) }
                    }
                }.accessibilityLabel(merging ? "Reviewed merge changes" : "Reviewed sync changes")
                if plan.unavailable > 0 || plan.otherVolumes > 0 {
                    Text("\(plan.unavailable) unavailable. \(plan.otherVolumes) other mounts skipped. Choose folders with full coverage to apply.")
                        .font(.caption).foregroundStyle(.secondary)
                }
            } else if let report {
                Text("\(report.differences.count) differences · \(report.matchingItems) match")
                List(report.differences) { item in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(item.relativePath).lineLimit(1).truncationMode(.middle).help(item.relativePath)
                        Text(item.kind.rawValue).font(.caption).foregroundStyle(.secondary)
                        HStack {
                            if let first = item.first { openButton("Show First", item: first) }
                            if let second = item.second { openButton("Show Second", item: second) }
                        }
                    }.padding(.vertical, 2)
                }
                if report.unavailable > 0 || report.otherVolumes > 0 {
                    Text("\(report.unavailable) items unavailable. \(report.otherVolumes) other mounts skipped.")
                        .font(.caption).foregroundStyle(.secondary)
                }
            } else {
                if applied {
                    Text(actions.status)
                    ForEach(actions.issues, id: \.self) { Text($0).foregroundStyle(.secondary).textSelection(.enabled) }
                } else if !problem.isEmpty {
                    Text(problem).foregroundStyle(.secondary)
                } else if let request {
                    ProgressView(merging ? "Reviewing merge…" : request.sync == nil ? "Comparing…" : "Reviewing sync…")
                }
                Spacer()
            }
            HStack {
                if let applying {
                    ProgressView().controlSize(.small).accessibilityLabel(merging ? "Merging folders" : "Applying folder sync")
                    Button("Stop") { applying.cancel() }
                } else if request != nil && report == nil && plan == nil && problem.isEmpty {
                    Button("Stop") { request = nil }
                }
                Spacer()
                if let plan, plan.canApply {
                    Button("Apply \(plan.changeCount) Changes…") { confirmSync = true }.disabled(actions.busy)
                }
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction).disabled(applying != nil)
            }
        }.padding(16).frame(width: 715, height: 515)
            .alert(merging ? "Merge the reviewed items?" : "Apply the reviewed sync?", isPresented: $confirmSync) {
                Button("Cancel", role: .cancel) {}
                Button(merging ? "Merge Folders" : "Apply Sync") { applySync() }
            } message: {
                Text(merging ? "Copy the listed missing items into the second folder."
                     : "Apply the listed changes to the second folder. Replaced or removed items go to Trash.")
            }
            .onChange(of: mode) { _ in reset() }
            .onChange(of: syncOptions) { _ in reset() }
            .onDisappear { applying?.cancel() }
            .task(id: request) {
                guard let requested = request else { return }
                let root = first
                do {
                    if let merge = mergeIssue?.conflict?.merge {
                        let result = try await readStorage {
                            try FolderSyncPlan.previewMerge(merge.incoming, into: merge.existing,
                                                            isCancelled: { Task.isCancelled })
                        }
                        if !Task.isCancelled { plan = result }
                    } else if let options = requested.sync {
                        let result = try await readStorage {
                            try FolderSyncPlan.preview(from: root, to: requested.folder, options: options,
                                                       isCancelled: { Task.isCancelled })
                        }
                        if !Task.isCancelled { plan = result }
                    } else {
                        let result = try await readStorage {
                            try StorageAnalysis.compareFolders(root, requested.folder, mode: requested.mode,
                                                               isCancelled: { Task.isCancelled })
                        }
                        if !Task.isCancelled { report = result }
                    }
                } catch {
                    if !Task.isCancelled { problem = error.localizedDescription }
                }
            }
    }

    private var canPreview: Bool { second != nil && second?.path != first.path && applying == nil }

    private func reset() {
        request = nil
        report = nil
        plan = nil
        problem = ""
        applied = false
    }

    private func preview(sync: FolderSyncPlan.Options?) {
        guard let second else { return }
        reset()
        request = Request(folder: second, mode: mode, sync: sync)
    }

    private func applySync() {
        guard let plan, let task = actions.applySync(plan, resolving: mergeIssue) else { return }
        applying = task
        Task {
            await task.value
            applying = nil
            reset()
            applied = true
        }
    }

    private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.directoryURL = second ?? first.deletingLastPathComponent()
        panel.prompt = "Compare With"
        panel.begin { result in
            if result == .OK, let folder = panel.url {
                second = folder
                reset()
            }
        }
    }

    private func openButton(_ title: String, item: URL) -> some View {
        Button(title) {
            openFolder(item.deletingLastPathComponent())
            dismiss()
        }.buttonStyle(.borderless)
    }
}

private func readStorage<Result: Sendable>(_ operation: @escaping @Sendable () throws -> Result) async throws -> Result {
    let worker = Task.detached(priority: .utility, operation: operation)
    return try await withTaskCancellationHandler { try await worker.value } onCancel: { worker.cancel() }
}

@MainActor
struct ChecksumSheet: View {
    let file: FileReference
    @Environment(\.dismiss) private var dismiss
    @State private var checksum: String?
    @State private var expected = ""
    @State private var problem = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("SHA-256 Checksum").font(.headline)
            Text(file.url.lastPathComponent).lineLimit(1).truncationMode(.middle)
            if let checksum {
                Text(checksum).font(.system(.body, design: .monospaced)).textSelection(.enabled)
                TextField("Paste expected SHA-256", text: $expected)
                if !expected.isEmpty { Text(verification(checksum)).font(.callout) }
            } else if !problem.isEmpty { Text(problem).foregroundStyle(.secondary) }
            else { ProgressView("Reading file…") }
            HStack {
                if let checksum {
                    Button("Copy Checksum") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(checksum, forType: .string)
                    }
                }
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
        }.padding(16).frame(width: 570)
            .task {
                let reference = file
                do {
                    checksum = try await readStorage {
                        try StorageAnalysis.checksum(of: reference, isCancelled: { Task.isCancelled })
                    }
                } catch {
                    if !Task.isCancelled { problem = error.localizedDescription }
                }
            }
    }

    private func verification(_ checksum: String) -> String {
        let normalized = expected.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard normalized.utf8.count == 64, normalized.utf8.allSatisfy({
            (48...57).contains($0) || (97...102).contains($0)
        }) else { return "A SHA-256 checksum has 64 hexadecimal characters." }
        return normalized == checksum ? "Checksum matches." : "Checksum does not match."
    }
}

@MainActor
struct DuplicateSheet: View {
    let folder: URL
    let openFolder: (URL) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var report: DuplicateFileReport?
    @State private var problem = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Duplicate Files").font(.headline)
            Text(folder.path).lineLimit(1).truncationMode(.middle).textSelection(.enabled)
            if let report {
                if report.groups.isEmpty {
                    Text("No duplicates found in the files checked.").foregroundStyle(.secondary)
                    Spacer()
                } else {
                    List(report.groups) { group in
                        Section("\(group.files.count) files · \(formattedBytes(group.size)) each") {
                            ForEach(group.files, id: \.self) { file in
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(file.lastPathComponent).lineLimit(1)
                                    Text(file.deletingLastPathComponent().path).font(.caption).foregroundStyle(.secondary)
                                        .lineLimit(1).truncationMode(.middle)
                                    Button("Open Containing Folder") {
                                        openFolder(file.deletingLastPathComponent())
                                        dismiss()
                                    }.buttonStyle(.borderless)
                                }.padding(.vertical, 3)
                            }
                        }
                    }
                }
                if report.unavailable > 0 || report.otherVolumes > 0 {
                    Text("\(report.unavailable) items unavailable. \(report.otherVolumes) other mounts skipped.")
                        .font(.caption).foregroundStyle(.secondary)
                }
            } else if !problem.isEmpty {
                Text(problem).foregroundStyle(.secondary)
                Spacer()
            } else {
                ProgressView("Comparing file contents…")
                Spacer()
            }
            HStack {
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
        }.padding(16).frame(width: 570, height: 430)
            .task {
                let requested = folder
                do {
                    report = try await readStorage {
                        try StorageAnalysis.duplicates(in: requested, isCancelled: { Task.isCancelled })
                    }
                } catch {
                    if !Task.isCancelled { problem = error.localizedDescription }
                }
            }
    }
}

@MainActor
struct DiskUsageSheet: View {
    let root: URL
    let openFolder: (URL) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var folder: URL
    @State private var report: DiskUsageReport?
    @State private var problem = ""

    init(folder: URL, openFolder: @escaping (URL) -> Void) {
        root = folder
        self.openFolder = openFolder
        _folder = State(initialValue: folder)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Disk Usage").font(.headline)
            Text(folder.path).lineLimit(1).truncationMode(.middle).textSelection(.enabled)
            if let report {
                Text("\(formattedBytes(report.allocatedBytes)) allocated")
                List(report.rows) { row in
                    HStack {
                        if row.isFolder {
                            Button { folder = row.url } label: {
                                Label(row.url.lastPathComponent, systemImage: "folder").lineLimit(1)
                            }.buttonStyle(.plain)
                        } else {
                            Label(row.url.lastPathComponent, systemImage: "doc").lineLimit(1)
                        }
                        Spacer(minLength: 10)
                        Text(formattedBytes(row.allocatedBytes)).monospacedDigit()
                            .help("\(formattedBytes(row.logicalBytes)) of file contents")
                    }
                }.accessibilityLabel("Items sorted by allocated size")
                if report.unavailable > 0 || report.otherVolumes > 0 {
                    Text("\(report.unavailable) items unavailable. \(report.otherVolumes) other mounts skipped.")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Text("Files that share disk blocks may count toward more than one folder.")
                    .font(.caption).foregroundStyle(.secondary)
            } else if !problem.isEmpty {
                Text(problem).foregroundStyle(.secondary)
                Spacer()
            } else {
                ProgressView("Measuring files…")
                Spacer()
            }
            HStack {
                Button("Parent Folder") { folder = folder.deletingLastPathComponent() }
                    .disabled(folder.path == root.path)
                Button("Open in Browser") { openFolder(folder); dismiss() }
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
        }.padding(16).frame(width: 570, height: 420)
            .task(id: folder) {
                let requested = folder
                report = nil
                problem = ""
                do {
                    let result = try await readStorage {
                        try StorageAnalysis.diskUsage(in: requested, isCancelled: { Task.isCancelled })
                    }
                    if !Task.isCancelled { report = result }
                } catch {
                    if !Task.isCancelled { problem = error.localizedDescription }
                }
            }
    }
}

@MainActor
struct WorkspacesSheet: View {
    @ObservedObject var window: WindowWorkspace
    @Environment(\.dismiss) private var dismiss
    @State private var workspaces: [NamedWorkspace] = []
    @State private var name = ""
    @State private var problem = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Workspaces").font(.headline)
            List(workspaces) { workspace in
                HStack {
                    Text(workspace.name)
                    Spacer()
                    Button("Open Here") {
                        if window.restoreSession(workspace.session) { dismiss() }
                        else { problem = "RichFiles could not read that workspace." }
                    }.accessibilityLabel("Open \(workspace.name)")
                    Button {
                        window.primary.removeWorkspace(workspace.id)
                        workspaces = window.primary.namedWorkspaces()
                    } label: {
                        Image(systemName: "trash")
                    }.accessibilityLabel("Remove \(workspace.name)")
                }.buttonStyle(.borderless)
            }.accessibilityLabel("Saved workspaces")
            HStack {
                TextField("Workspace name", text: $name)
                Button("Save Current Workspace") { save() }
                    .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            if window.previousDockedSession != nil {
                Button("Restore Previous Docked Workspace") {
                    if window.restorePreviousDockedSession() { dismiss() }
                    else { problem = "RichFiles could not reopen the previous workspace." }
                }
            }
            if !problem.isEmpty { Text(problem).font(.callout).foregroundStyle(.secondary) }
            HStack {
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
        }.padding(16).frame(width: 470, height: 320)
            .onAppear { workspaces = window.primary.namedWorkspaces() }
    }

    private func save() {
        do {
            guard let snapshot = window.sessionData() else { throw WorkspaceSaveError.cannotSave }
            try window.primary.saveWorkspace(named: name, session: snapshot)
            workspaces = window.primary.namedWorkspaces()
            name = ""
            problem = ""
        } catch { problem = error.localizedDescription }
    }
}

struct NameSheet: View {
    let request: NameRequest
    let save: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var name: String
    @State private var confirmExtension = false

    init(request: NameRequest, save: @escaping (String) -> Void) {
        self.request = request
        self.save = save
        _name = State(initialValue: request.name)
    }

    private var changesExtension: Bool {
        (request.source != nil || request.template != nil) && !request.directory &&
        (name as NSString).pathExtension.caseInsensitiveCompare((request.name as NSString).pathExtension) != .orderedSame
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(request.source != nil ? "Rename" : (request.directory ? "New Folder" : "New File")).font(.headline)
            NameField(text: $name, selectBasename: !request.directory, submit: submit)
                .frame(height: 24)
                .accessibilityLabel("Name")
            if changesExtension {
                Text("Changing the extension can change which app opens this file.")
                    .font(.callout).foregroundStyle(.secondary)
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button(request.source == nil ? "Create" : "Rename") {
                    submit()
                }
                .keyboardShortcut(.defaultAction)
                .disabled((try? FileOperations.validateName(name)) == nil)
            }
        }.padding(18).frame(width: 440)
            .alert("Change the File Extension?", isPresented: $confirmExtension) {
                Button("Cancel", role: .cancel) {}
                Button("Change Extension") { save(name) }
            } message: {
                Text("The new name is \(name). macOS may open it in a different app.")
            }
    }

    private func submit() {
        guard (try? FileOperations.validateName(name)) != nil else { return }
        if changesExtension { confirmExtension = true }
        else { save(name) }
    }
}

private struct NameField: NSViewRepresentable {
    @Binding var text: String
    var selectBasename: Bool
    var submit: () -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> TextFieldView {
        let field = TextFieldView()
        field.isEditable = true
        field.isSelectable = true
        field.isBezeled = true
        field.usesSingleLineMode = true
        field.stringValue = text
        field.selectBasename = selectBasename
        field.delegate = context.coordinator
        field.target = context.coordinator
        field.action = #selector(Coordinator.commit)
        field.setAccessibilityIdentifier("file-name-input")
        return field
    }

    func updateNSView(_ field: TextFieldView, context: Context) {
        context.coordinator.parent = self
        if field.stringValue != text { field.stringValue = text }
    }

    final class TextFieldView: NSTextField {
        var selectBasename = true
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard window != nil else { return }
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.selectText(nil)
                let fullName = self.stringValue as NSString
                let length = self.selectBasename ? (fullName.deletingPathExtension as NSString).length : fullName.length
                self.currentEditor()?.selectedRange = NSRange(location: 0, length: length)
            }
        }
    }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: NameField
        init(_ parent: NameField) { self.parent = parent }
        func controlTextDidChange(_ notification: Notification) {
            if let field = notification.object as? NSTextField { parent.text = field.stringValue }
        }
        @objc func commit() { parent.submit() }
    }
}

struct FileThumbnail: View {
    let entry: FileEntry
    let side: Double
    @State private var thumbnail: CGImage?
    @State private var downloadRequired = false

    var body: some View {
        Group {
            if let thumbnail {
                Image(decorative: thumbnail, scale: 1).resizable().scaledToFit()
            } else {
                Image(systemName: downloadRequired ? "icloud.and.arrow.down" : fileSymbol(entry))
                    .font(.system(size: max(20, side * 0.45)))
                    .foregroundStyle(entry.kind == .directory ? Color.accentColor : .secondary)
            }
        }.frame(width: side, height: side)
            .accessibilityHidden(true)
            .task(id: "\(entry.url.absoluteString):\(entry.modified?.timeIntervalSince1970 ?? 0):\(side)") {
                thumbnail = nil
                downloadRequired = false
                guard entry.kind == .regularFile, !entry.needsDownload else { return }
                do {
                    let image = try await WorkspaceServices.thumbnail(for: entry.url, size: CGSize(width: side, height: side), scale: 2)
                    if !Task.isCancelled { thumbnail = image }
                } catch ThumbnailError.downloadRequired {
                    if !Task.isCancelled { downloadRequired = true }
                } catch {}
            }
    }
}

struct NativePreview: View {
    let entry: FileEntry

    var body: some View {
        if entry.needsDownload {
            downloadRequiredLabel
        } else if entry.kind == .regularFile {
            CheckedQuickLook(file: entry.url).id(entry.url)
        } else {
            FileThumbnail(entry: entry, side: 110)
        }
    }
}

private var downloadRequiredLabel: some View {
    Label("Download this file before previewing it.", systemImage: "icloud.and.arrow.down")
        .foregroundStyle(.secondary)
}

private struct CheckedQuickLook: View {
    let file: URL
    private enum Availability { case checking, local, downloadRequired, unavailable }
    @State private var availability = Availability.checking

    var body: some View {
        Group {
            switch availability {
            case .checking: ProgressView().accessibilityLabel("Checking preview")
            case .local: QuickLookView(url: file)
            case .downloadRequired: downloadRequiredLabel
            case .unavailable: Text("Preview unavailable.").foregroundStyle(.secondary)
            }
        }.task {
            do {
                let requiresDownload = try await readStorage { try WorkspaceServices.previewRequiresDownload(for: file) }
                if !Task.isCancelled { availability = requiresDownload ? .downloadRequired : .local }
            } catch {
                if !Task.isCancelled { availability = .unavailable }
            }
        }
    }
}

private struct QuickLookView: NSViewRepresentable {
    let url: URL
    func makeNSView(context: Context) -> NSView {
        guard let preview = QLPreviewView(frame: .zero, style: .normal) else { return NSView() }
        preview.autostarts = true
        return preview
    }
    func updateNSView(_ view: NSView, context: Context) {
        guard let preview = view as? QLPreviewView else { return }
        if (preview.previewItem as? NSURL) as URL? != url { preview.previewItem = url as NSURL }
    }
    static func dismantleNSView(_ view: NSView, coordinator: ()) {
        (view as? QLPreviewView)?.close()
    }
}

struct PreviewSheet: View {
    let entry: FileEntry
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 8) {
            HStack {
                Text(entry.name).font(.headline).lineLimit(1)
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
            NativePreview(entry: entry).frame(maxWidth: .infinity, maxHeight: .infinity)
        }.padding(12).frame(minWidth: 530, idealWidth: 680, minHeight: 330, idealHeight: 480)
    }
}

struct PropertiesSheet: View {
    let entry: FileEntry
    @Environment(\.dismiss) private var dismiss
    @State private var properties: FileProperties?
    @State private var problem = ""
    @State private var folderSize = ""
    @State private var sizeTask: Task<Void, Never>?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Properties").font(.headline)
            Text(entry.name).font(.title3.weight(.medium)).textSelection(.enabled)
            Form {
                LabeledContent("Path", value: entry.url.path).textSelection(.enabled)
                LabeledContent("Kind", value: entry.typeDescription ?? "Unknown")
                if properties?.needsDownload ?? entry.needsDownload {
                    LabeledContent("Availability", value: "Download required")
                }
                LabeledContent("Size", value: entry.size.map(formattedBytes) ?? (folderSize.isEmpty ? "–" : folderSize))
                LabeledContent("Created", value: entry.created.map(formattedDate) ?? "–")
                LabeledContent("Modified", value: entry.modified.map(formattedDate) ?? "–")
                if let mode = properties?.permissions { LabeledContent("Permissions", value: String(mode, radix: 8)) }
                if let owner = properties?.owner { LabeledContent("Owner", value: owner) }
                if let group = properties?.group { LabeledContent("Group", value: group) }
            }.formStyle(.grouped)
            if !problem.isEmpty { Text(problem).foregroundStyle(.secondary) }
            HStack {
                if entry.kind == .directory {
                    Button("Calculate Folder Size") { measure() }.disabled(sizeTask != nil)
                    if sizeTask != nil { Button("Cancel Calculation") { sizeTask?.cancel() } }
                }
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
        }.padding(15).frame(width: 520, height: 375)
            .task {
                do {
                    let result = try await readStorage { try FileProperties(url: entry.url) }
                    if !Task.isCancelled { properties = result }
                } catch {
                    if !Task.isCancelled { problem = error.localizedDescription }
                }
            }
            .onDisappear { sizeTask?.cancel() }
    }

    private func measure() {
        folderSize = "Calculating…"
        sizeTask = Task {
            defer { sizeTask = nil }
            do {
                let result = try await Self.folderBytes(entry.url)
                folderSize = formattedBytes(result.bytes) + (result.incomplete ? " (partial)" : "")
            } catch is CancellationError {
                folderSize = "Cancelled"
            } catch {
                folderSize = "Unavailable"
                problem = error.localizedDescription
            }
        }
    }

    nonisolated private static func folderBytes(_ url: URL) async throws -> (bytes: Int64, incomplete: Bool) {
        let folder = try DirectoryReader.directoryURL(url)
        let keys: Set<URLResourceKey> = [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey]
        var incomplete = false
        guard let enumerator = FileManager.default.enumerator(at: folder, includingPropertiesForKeys: Array(keys), errorHandler: { _, _ in
            incomplete = true
            return true
        }) else { throw CocoaError(.fileReadUnknown) }
        var bytes: Int64 = 0
        while true {
            try Task.checkCancellation()
            guard let child = enumerator.nextObject() as? URL else { break }
            do {
                let values = try child.resourceValues(forKeys: keys)
                if values.isRegularFile == true && values.isSymbolicLink != true, let size = values.fileSize {
                    let (sum, overflow) = bytes.addingReportingOverflow(Int64(size))
                    bytes = overflow ? Int64.max : sum
                    incomplete = incomplete || overflow
                }
            } catch { incomplete = true }
        }
        return (bytes, incomplete)
    }
}

struct OpenWithSheet: View {
    let entry: FileEntry
    private let applications: [WorkspaceApplication]
    @Environment(\.dismiss) private var dismiss
    @State private var problem = ""
    @State private var selected: WorkspaceApplication?
    @State private var preferred: WorkspaceApplication?
    @State private var confirmDefault = false
    @State private var changingDefault = false

    init(entry: FileEntry, apps: WorkspaceApplications) {
        self.entry = entry
        applications = [apps.defaultApplication].compactMap { $0 } + apps.alternatives
        _preferred = State(initialValue: apps.defaultApplication)
        _selected = State(initialValue: applications.first)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Open With").font(.headline)
            Text(entry.name).lineLimit(1).help(entry.name)
            List {
                if let preferred { applicationRow(preferred, preferred: true) }
                ForEach(applications.filter { $0.id != preferred?.id }) { applicationRow($0, preferred: false) }
                if applications.isEmpty {
                    Text("macOS found no associated apps.").foregroundStyle(.secondary)
                }
            }.frame(minHeight: 125).disabled(changingDefault)
            if !problem.isEmpty { Text(problem).foregroundStyle(.secondary) }
            HStack {
                Button("Set Default App…") { confirmDefault = true }
                    .disabled(changingDefault || selected == nil)
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Open") { open() }.keyboardShortcut(.defaultAction).disabled(changingDefault || selected == nil)
            }
        }.padding(15).frame(width: 450, height: 330)
            .alert("Change the Default App for This File Type?", isPresented: $confirmDefault) {
                Button("Cancel", role: .cancel) {}
                Button("Change Default") { setDefault() }
            } message: { Text("macOS may ask for consent. The choice applies to other files of this type.") }
    }

    private func applicationRow(_ app: WorkspaceApplication, preferred: Bool) -> some View {
        Button {
            selected = app
        } label: {
            HStack {
                Text(app.name)
                if preferred { Text("Default").font(.caption).foregroundStyle(.secondary) }
                Spacer()
                if selected?.id == app.id { Image(systemName: "checkmark").foregroundStyle(Color.accentColor) }
            }.contentShape(Rectangle())
        }.buttonStyle(.plain)
            .accessibilityLabel(preferred ? "\(app.name), Default" : app.name)
    }

    private func open() {
        guard let selected else { return }
        Task {
            do { try await WorkspaceServices.open(entry.url, with: selected); dismiss() }
            catch { problem = error.localizedDescription }
        }
    }

    private func setDefault() {
        guard let selected, !changingDefault else { return }
        changingDefault = true
        Task {
            defer { changingDefault = false }
            do {
                guard let type = try entry.url.resourceValues(forKeys: [.contentTypeKey]).contentType else {
                    problem = "macOS did not identify this file type."
                    return
                }
                try await NSWorkspace.shared.setDefaultApplication(at: selected.url, toOpen: type)
                let actual = NSWorkspace.shared.urlForApplication(toOpen: type)?.resolvingSymlinksInPath()
                if actual == selected.url.resolvingSymlinksInPath() {
                    let defaultForFile = NSWorkspace.shared.urlForApplication(toOpen: entry.url)?.resolvingSymlinksInPath()
                    preferred = applications.first { $0.url.resolvingSymlinksInPath() == defaultForFile }
                    problem = "Default app updated."
                } else {
                    problem = "macOS did not confirm the change."
                }
            } catch { problem = error.localizedDescription }
        }
    }
}

@MainActor
struct TrashSheet: View {
    @ObservedObject var actions: BrowserActions
    @Environment(\.dismiss) private var dismiss
    @State private var index: TrashIndex?
    @State private var problem = ""
    @State private var refresh = UUID()
    @State private var selected: Set<TrashListEntry.ID> = []
    @State private var reviewedDeletion: [TrashListEntry] = []
    @State private var confirmingEmpty = false

    private var selectedEntries: [TrashListEntry] {
        index?.entries.filter { selected.contains($0.id) } ?? []
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Trash").font(.headline)
            if let index {
                if index.inaccessible > 0 {
                    Text("Some Trash locations could not be read.").font(.caption).foregroundStyle(.secondary)
                }
                if index.entries.isEmpty {
                    Text(index.inaccessible == 0 ? "Trash is empty." : "No readable items.")
                        .foregroundStyle(.secondary)
                }
                List(index.entries, selection: $selected) { entry in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(entry.name).font(.body.weight(.medium))
                        Text(entry.originalFolder ?? "Original folder unavailable").font(.caption).textSelection(.enabled)
                        if let date = entry.dateAdded {
                            Text(formattedDate(date)).font(.caption).foregroundStyle(.secondary)
                        }
                        HStack {
                            if let known = entry.known {
                                Button("Restore") { actions.restore(known) }.disabled(!known.canRestore)
                            }
                            Button("Restore To…") { chooseFolder(for: entry) }.disabled(entry.reference == nil)
                        }.disabled(actions.busy).buttonStyle(.borderless)
                    }.padding(.vertical, 3).tag(entry.id)
                }
            } else if problem.isEmpty {
                ProgressView("Reading Trash…")
                Spacer()
            } else {
                Text(problem).foregroundStyle(.secondary)
                Spacer()
            }
            if !actions.status.isEmpty { Text(actions.status).font(.callout).foregroundStyle(.secondary) }
            if actions.operationIssues.count > 1 {
                DisclosureGroup("Operation issues (\(actions.operationIssues.count))") {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 4) {
                            ForEach(Array(actions.operationIssues.enumerated()), id: \.offset) { _, issue in
                                Text(issue).textSelection(.enabled)
                            }
                        }.frame(maxWidth: .infinity, alignment: .leading)
                    }.frame(maxHeight: 80)
                }.font(.caption).foregroundStyle(.secondary)
            }
            HStack {
                Button("Refresh") { refresh = UUID() }.disabled(actions.busy)
                Button("Open in Finder") { openSystemTrash() }
                Button("Restore Selected") { actions.restoreSelected(selectedEntries) }
                    .disabled(actions.busy || !selectedEntries.contains { $0.known?.canRestore == true })
                Spacer()
                if let index, index.entries.contains(where: \.canDelete) {
                    Button(index.inaccessible == 0 ? "Empty Trash…" : "Delete Listed Items…") {
                        reviewedDeletion = index.entries.filter(\.canDelete)
                        confirmingEmpty = true
                    }.disabled(actions.busy)
                }
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
        }.padding(14).frame(width: 610, height: 425)
            .task(id: refresh) { await readTrash() }
            .onChange(of: actions.mutationRevision) { _ in refresh = UUID() }
            .alert("Permanently delete \(reviewedDeletion.count) Trash items?", isPresented: $confirmingEmpty) {
                Button("Cancel", role: .cancel) { reviewedDeletion = [] }
                Button("Delete Permanently", role: .destructive) {
                    actions.deletePermanently(reviewedDeletion.compactMap(\.reference), trash: reviewedDeletion.compactMap(\.known))
                    reviewedDeletion = []
                }
            } message: { Text("These items cannot be recovered from Trash.") }
    }

    private func readTrash() async {
        selected.removeAll()
        let receipts = actions.trashItems
        do {
            let current = try await readStorage { try TrashIndex.read(known: receipts) }
            if !Task.isCancelled { selected.removeAll(); index = current; problem = "" }
        } catch {
            if !Task.isCancelled { problem = error.localizedDescription }
        }
    }

    private func chooseFolder(for item: TrashListEntry) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Restore Here"
        panel.begin { result in
            if result == .OK, let folder = panel.url { actions.restoreFromTrash(item, into: folder) }
        }
    }

    private func openSystemTrash() {
        guard let folder = FileManager.default.urls(for: .trashDirectory, in: .userDomainMask).first,
              let finder = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.finder") else { return }
        NSWorkspace.shared.open([folder], withApplicationAt: finder,
                                configuration: NSWorkspace.OpenConfiguration(), completionHandler: nil)
    }
}

struct OperationIssuesSheet: View {
    let issues: [String]
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Operation Issues").font(.headline).accessibilityAddTraits(.isHeader)
            List(issues.indices, id: \.self) { index in
                Text(issues[index])
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
            }
            HStack {
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
        }.padding(16).frame(width: 530, height: 320)
    }
}

@MainActor
struct TrashFailuresSheet: View {
    let sources: [FileReference]
    @ObservedObject var actions: BrowserActions
    @Environment(\.dismiss) private var dismiss
    @State private var confirming = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Could Not Move to Trash").font(.headline)
            List(sources, id: \.url) { item in
                Text(item.url.path).lineLimit(1).truncationMode(.head).textSelection(.enabled)
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Delete Permanently…") { confirming = true }.disabled(actions.busy)
            }
        }.padding(14).frame(width: 560, height: 330)
            .alert("Permanently delete \(sources.count) items?", isPresented: $confirming) {
                Button("Cancel", role: .cancel) {}
                Button("Delete Permanently", role: .destructive) {
                    actions.deletePermanently(sources, afterTrashFailure: true)
                    dismiss()
                }
            } message: { Text("These items will be removed from their current folders and cannot be recovered from Trash.") }
    }
}

@MainActor
struct InterruptedTransferSheet: View {
    let notice: InterruptedTransferNotice
    @ObservedObject var actions: BrowserActions
    let openFolder: (URL) -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(notice.move ? "Interrupted Move" : "Interrupted Copy").font(.headline)
            Text("\(notice.succeeded) of \(notice.total) items completed before RichFiles closed.")
            location("Destination folder", notice.destination)
            if let pending = notice.pendingSource { location("Item to check", pending) }
            if let failed = notice.lastFailedSource { location("Last reported issue", failed) }
            if notice.incompleteTail { Text("The last recovery entry was incomplete.").font(.callout) }
            if notice.recoverableMoves > 0 {
                let label = notice.recoverableMoves == 1 ? "move" : "moves"
                Text("Undo history includes \(notice.recoverableMoves) completed \(label).").font(.callout)
            }
            HStack {
                Button("Open Destination") {
                    openFolder(notice.destination)
                    dismiss()
                }
                if let source = notice.pendingSource ?? notice.lastFailedSource {
                    Button("Open Source Folder") {
                        openFolder(source.deletingLastPathComponent())
                        dismiss()
                    }
                }
                Spacer()
                Button("Clear Notice") {
                    actions.acknowledgeInterruptedTransfer()
                    if actions.interruptedTransfer == nil { dismiss() }
                }.disabled(actions.busy)
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
        }.padding(16).frame(width: 565)
    }

    private func location(_ title: String, _ url: URL) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).font(.caption.weight(.medium))
            Text(url.path).font(.caption).lineLimit(2).truncationMode(.middle).textSelection(.enabled).help(url.path)
        }
    }
}

@MainActor
struct TransferPanel: View {
    @ObservedObject var actions: BrowserActions
    let close: () -> Void
    @State private var replacement: BrowserTransferIssue?
    @State private var mergeIssue: BrowserTransferIssue?
    @State private var confirmingReplacement = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("Transfers").font(.headline)
                Spacer()
                Button("Done", action: close)
            }
            if let transfer = actions.transfer {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 5) {
                        Text(transfer.destination.path).lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                        ProgressView(value: Double(transfer.completed), total: Double(max(transfer.total, 1)))
                        Text("\(transfer.completed) of \(transfer.total) processed, \(transfer.succeeded) completed")
                        if let source = transfer.currentSource {
                            Text("\(transfer.title) · \(source.lastPathComponent)").lineLimit(1)
                            Text(source.deletingLastPathComponent().path).font(.caption).foregroundStyle(.secondary)
                                .lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                        }
                        if let progress = transfer.copyProgress {
                            ProgressView(value: Double(progress.written), total: Double(progress.total))
                            HStack(spacing: 8) {
                                Text("\(formattedBytes(progress.written)) of \(formattedBytes(progress.total))")
                                if let rate = progress.bytesPerSecond { Text("\(formattedBytes(rate))/s") }
                                if let seconds = progress.secondsRemaining {
                                    Text("About \(formattedTransferTime(seconds)) left")
                                }
                            }.font(.caption).foregroundStyle(.secondary)
                        }
                        if transfer.verifyingCopy { ProgressView("Verifying the copy…").controlSize(.small) }
                        if actions.replacingFile {
                            ProgressView("Replacing file…").controlSize(.small)
                        } else if !transfer.running && !actions.status.isEmpty {
                            Text(actions.status).font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                        }
                        if transfer.cancelled { Text("Transfer cancelled.") }
                        ForEach(transfer.issues) { issue in
                            VStack(alignment: .leading, spacing: 5) {
                                Text(issue.name).font(.body.weight(.medium))
                                Text(issue.message).font(.callout).foregroundStyle(.secondary)
                                if let conflict = issue.conflict {
                                    HStack(alignment: .top, spacing: 16) {
                                        conflictDetails("Incoming", conflict.incoming)
                                        conflictDetails("Existing", conflict.existing)
                                    }
                                    if conflict.merge != nil {
                                        Button("Merge Folders…") { mergeIssue = issue }
                                            .disabled(actions.busy).buttonStyle(.borderless)
                                    }
                                    if conflict.reviewed != nil {
                                        Button("Replace File…") {
                                            replacement = issue
                                            confirmingReplacement = true
                                        }.disabled(actions.busy).buttonStyle(.borderless)
                                    }
                                }
                            }
                        }
                        if !actions.queuedTransfers.isEmpty {
                            VStack(alignment: .leading, spacing: 6) {
                                Text("Waiting").font(.caption.weight(.semibold))
                                ForEach(actions.queuedTransfers) { item in
                                    HStack {
                                        VStack(alignment: .leading, spacing: 3) {
                                            Text(item.title)
                                            Text(item.folder.path).font(.caption).foregroundStyle(.secondary)
                                                .lineLimit(1).truncationMode(.head)
                                            switch item.state {
                                            case .preparing: Text("Checking files…").font(.caption)
                                            case .unavailable(let problem): Text(problem).font(.caption)
                                            case .ready: EmptyView()
                                            }
                                        }
                                        Spacer()
                                        Button("Remove") { actions.removeQueuedTransfer(item.id) }
                                            .buttonStyle(.borderless)
                                    }
                                }
                            }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(maxHeight: 142)
                HStack {
                    if transfer.running || actions.replacingFile { Button("Cancel Transfer") { actions.cancelTransfer() } }
                    if actions.queuePaused && !actions.busy {
                        Button("Continue Queue") { actions.continueQueue() }
                    }
                    if actions.canRetryTransfer && !actions.busy {
                        Button("Retry") { actions.retryTransfer() }
                        if transfer.issues.contains(where: { $0.kind == .skipped }) {
                            Button("Retry, Keep Both") { actions.retryTransfer(conflict: .keepBoth) }
                        }
                    }
                }
            }
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 6)
        .background(.bar)
        .sheet(item: $mergeIssue) { issue in
            if let merge = issue.conflict?.merge {
                CompareFoldersSheet(first: merge.incoming.url, actions: actions, openFolder: { _ in }, mergeIssue: issue)
            }
        }
        .confirmationDialog("Replace the existing file?", isPresented: $confirmingReplacement, presenting: replacement) { issue in
            Button("Replace File", role: .destructive) { actions.replaceConflict(issue) }
        } message: { issue in
            if actions.transfer?.move == true, let pair = issue.conflict?.reviewed,
               pair.incoming.reference.identity.device != pair.existing.reference.identity.device {
                Text("RichFiles verifies the incoming copy before moving the original to Trash. The previous file also goes to Trash. Undo restores both locations.")
            } else {
                Text(actions.transfer?.move == true
                     ? "The incoming file moves here and the previous file goes to Trash. Undo restores both locations."
                     : "The previous file goes to Trash. You can undo this replacement in RichFiles.")
            }
        }
    }

    private func conflictDetails(_ label: String, _ entry: FileEntry) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.caption.weight(.semibold))
            Text(entry.name).lineLimit(1).truncationMode(.middle).help(entry.url.path)
            if let size = entry.size { Text(formattedBytes(size)) }
            if let modified = entry.modified { Text(formattedDate(modified)) }
        }.font(.caption).frame(maxWidth: .infinity, alignment: .leading)
    }

    private func formattedTransferTime(_ seconds: TimeInterval) -> String {
        let formatter = DateComponentsFormatter()
        formatter.allowedUnits = [.day, .hour, .minute, .second]
        formatter.unitsStyle = .abbreviated
        formatter.maximumUnitCount = 2
        return formatter.string(from: max(1, seconds)) ?? "1s"
    }
}

@MainActor
struct SearchOptionsSheet: View {
    @ObservedObject var model: BrowserModel
    @Environment(\.dismiss) private var dismiss
    @State private var queryText = ""
    @State private var extensions = ""
    @State private var recursively = true
    @State private var content = false
    @State private var minimum = ""
    @State private var maximum = ""
    @State private var byDate = false
    @State private var from = Calendar.current.date(byAdding: .day, value: -30, to: Date()) ?? Date()
    @State private var through = Date()
    @State private var finderUnavailable = false

    private var valid: Bool {
        let low = minimum.isEmpty ? 0 : (Int64(minimum) ?? -1)
        let high = maximum.isEmpty ? Int64.max : (Int64(maximum) ?? -1)
        return low >= 0 && high >= low && (!byDate || from <= through)
    }

    private var draft: BrowserModel.SearchState {
        let start = Calendar.current.startOfDay(for: from)
        let end = Calendar.current.date(byAdding: .day, value: 1, to: Calendar.current.startOfDay(for: through)) ?? through
        return BrowserModel.SearchState(
            text: queryText, extensions: extensions, minimumBytes: Int64(minimum), maximumBytes: Int64(maximum),
            modified: byDate && from <= through ? start...end.addingTimeInterval(-0.000001) : nil,
            recursive: recursively, contentMode: content
        )
    }

    private var hasQuery: Bool {
        !queryText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
            (!content && (!extensions.isEmpty || !minimum.isEmpty || !maximum.isEmpty || byDate))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Search Options").font(.headline)
            Form {
                Picker("Search", selection: $content) {
                    Text("Filenames").tag(false)
                    Text("File Contents").tag(true)
                }
                TextField(content ? "Text to find" : "Name or wildcard", text: $queryText)
                TextField("Extensions", text: $extensions, prompt: Text("png, pdf"))
                Toggle("Include Subfolders", isOn: $recursively)
                Button("Search Another Folder…") { chooseScope() }.disabled(!valid || !hasQuery)
                if content {
                    Text(BrowserModel.contentSearchCoverageDescription).font(.caption).foregroundStyle(.secondary)
                }
                TextField("Minimum bytes", text: $minimum)
                TextField("Maximum bytes", text: $maximum)
                Toggle("Filter by Modified Date", isOn: $byDate)
                if byDate {
                    DatePicker("From", selection: $from, displayedComponents: .date)
                    DatePicker("Through", selection: $through, displayedComponents: .date)
                }
            }.formStyle(.grouped)
            if finderUnavailable {
                Text("RichFinder could not open. Check that RichOS is installed and try again.")
                    .font(.callout).foregroundStyle(.secondary)
            }
            HStack {
                Button(RichOSServices.finderTitle) {
                    if RichOSServices.search(queryText) { dismiss() }
                    else { finderUnavailable = true }
                }
                .help("Search the sources configured in RichFinder.")
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Search") {
                    model.submitSearch(draft)
                    dismiss()
                }.keyboardShortcut(.defaultAction).disabled(!valid)
            }
        }.padding(15).frame(width: 465, height: 420)
            .onAppear {
                queryText = model.searchText
                extensions = model.searchExtensions
                recursively = model.searchRecursively
                content = model.searchContent
                minimum = model.searchMinimumBytes.map(String.init) ?? ""
                maximum = model.searchMaximumBytes.map(String.init) ?? ""
                if let range = model.searchModified {
                    byDate = true
                    from = range.lowerBound
                    through = range.upperBound
                }
            }
    }

    private func chooseScope() {
        let search = draft
        let panel = NSOpenPanel()
        panel.directoryURL = model.folder
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Search Folder"
        panel.begin { result in
            if result == .OK, let folder = panel.url {
                model.submitSearch(search, in: folder)
                dismiss()
            }
        }
    }
}

@MainActor
struct BatchRenameSheet: View {
    let sources: [FileReference]
    let folder: URL
    @ObservedObject var actions: BrowserActions
    @Environment(\.dismiss) private var dismiss
    @AppStorage("richfiles.batchRenamePresets.v1") private var savedRules = Data()
    @State private var operations = [Operation()]
    @State private var selectedOperation: UUID?
    @State private var plan: BatchRenamePlan?
    @State private var problem = ""
    @State private var presetName = ""
    @State private var previewTask: Task<Void, Never>?

    private enum Method: String, CaseIterable, Identifiable, Codable {
        case number = "Number", prefix = "Prefix", suffix = "Suffix", replace = "Replace"
        case regex = "Regular Expression", metadata = "File Metadata"
        var id: Self { self }
    }

    private struct Operation: Identifiable, Equatable, Codable {
        let id: UUID
        var method = Method.number
        var text = ""
        var replacement = ""
        var start = 1
        var digits = 3

        init(method: Method = .number) {
            id = UUID()
            self.method = method
        }

        var rule: BatchRenameRule {
            switch method {
            case .number: return .number(prefix: text, start: start, minimumDigits: digits)
            case .prefix: return .prefix(text)
            case .suffix: return .suffix(text)
            case .replace: return .replace(search: text, replacement: replacement)
            case .regex: return .regex(pattern: text, replacement: replacement)
            case .metadata: return .metadata(template: text)
            }
        }
    }

    private struct Preset: Identifiable, Codable {
        let name: String
        let operations: [Operation]
        var id: String { name }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Batch Rename").font(.headline)
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 6) {
                    List(operations, selection: $selectedOperation) { operation in
                        Text(operation.method.rawValue).tag(operation.id)
                    }.frame(height: 96).accessibilityLabel("Rename operations in order")
                    HStack {
                        Button("Add Operation") {
                            let item = Operation(method: .suffix)
                            operations.append(item)
                            selectedOperation = item.id
                        }.disabled(operations.count >= 12)
                        Button { reorder(-1) } label: { Image(systemName: "arrow.up") }
                            .accessibilityLabel("Move rename operation up")
                            .disabled(selectedIndex.map { $0 == 0 } ?? true)
                        Button { reorder(1) } label: { Image(systemName: "arrow.down") }
                            .accessibilityLabel("Move rename operation down")
                            .disabled(selectedIndex.map { $0 == operations.count - 1 } ?? true)
                        Button {
                            guard let index = selectedIndex else { return }
                            operations.remove(at: index)
                            selectedOperation = operations[min(index, operations.count - 1)].id
                        } label: { Image(systemName: "minus") }
                            .accessibilityLabel("Remove rename operation")
                            .disabled(operations.count < 2 || selectedIndex == nil)
                    }
                }.frame(width: 220)
                VStack(alignment: .leading, spacing: 8) {
                    ForEach($operations) { $operation in
                        if operation.id == selectedOperation {
                            editor($operation)
                        }
                    }
                }.frame(maxWidth: .infinity, alignment: .leading)
            }
            HStack {
                Menu("Saved Rules") {
                    ForEach(presets) { preset in
                        Button(preset.name) {
                            operations = preset.operations
                            selectedOperation = operations.first?.id
                        }
                    }
                    if !presets.isEmpty {
                        Divider()
                        Menu("Remove Saved Rule") {
                            ForEach(presets) { preset in
                                Button(preset.name) { writePresets(presets.filter { $0.id != preset.id }) }
                            }
                        }
                    }
                }.disabled(presets.isEmpty)
                TextField("Name for this rule", text: $presetName)
                    .accessibilityLabel("Name for this rule")
                Button("Save Rule") { savePreset() }
                    .disabled(plan == nil || presetName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            List {
                if let plan {
                    ForEach(Array(plan.proposals.enumerated()), id: \.offset) { _, proposal in
                        HStack {
                            Text(proposal.source.url.lastPathComponent).lineLimit(1)
                            Image(systemName: "arrow.right").foregroundStyle(.secondary)
                            Text(proposal.name).lineLimit(1)
                            Spacer()
                            Text(status(for: proposal.status)).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                } else {
                    Text("Preview names and conflicts before applying.").foregroundStyle(.secondary)
                }
            }.accessibilityLabel("Proposed names")
            if !problem.isEmpty { Text(problem).foregroundStyle(.secondary) }
            HStack {
                Button("Preview") { preview() }.disabled(previewTask != nil)
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Apply") {
                    if let plan { actions.apply(plan); dismiss() }
                }.disabled(plan?.canApply != true || previewTask != nil || actions.busy)
            }
        }.padding(16).frame(width: 675, height: 490)
            .onAppear { selectedOperation = operations.first?.id }
            .onDisappear { previewTask?.cancel() }
            .onChange(of: operations) { _ in
                previewTask?.cancel()
                plan = nil
                problem = ""
            }
    }

    @ViewBuilder
    private func editor(_ operation: Binding<Operation>) -> some View {
        let method = operation.wrappedValue.method
        Picker("Operation", selection: operation.method) {
            ForEach(Method.allCases) { Text($0.rawValue).tag($0) }
        }
        switch method {
        case .regex, .replace:
            TextField(method == .regex ? "Pattern" : "Find text", text: operation.text)
                .accessibilityLabel(method == .regex ? "Pattern" : "Find text")
            TextField("Replacement", text: operation.replacement)
                .accessibilityLabel("Replacement")
        case .metadata:
            TextField("Name template", text: operation.text, prompt: Text("{modified} {name}"))
                .accessibilityLabel("Name template")
            Text("{name}, {created}, {modified}, {number}").font(.caption).foregroundStyle(.secondary)
        case .number:
            TextField("Prefix", text: operation.text)
                .accessibilityLabel("Prefix")
            HStack {
                TextField("Start at", value: operation.start, format: .number)
                    .accessibilityLabel("Start at")
                TextField("Minimum digits", value: operation.digits, format: .number)
                    .accessibilityLabel("Minimum digits")
            }
        case .prefix, .suffix:
            TextField("Text", text: operation.text)
                .accessibilityLabel("Text")
        }
    }

    private var selectedIndex: Int? { operations.firstIndex { $0.id == selectedOperation } }

    private func reorder(_ direction: Int) {
        guard let index = selectedIndex, operations.indices.contains(index + direction) else { return }
        operations.swapAt(index, index + direction)
    }

    private var presets: [Preset] {
        guard savedRules.count <= 256 * 1024 else { return [] }
        return ((try? JSONDecoder().decode([Preset].self, from: savedRules)) ?? []).filter {
            (1...12).contains($0.operations.count) && Set($0.operations.map(\.id)).count == $0.operations.count
        }
    }

    private func savePreset() {
        let name = presetName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, name.count <= 80 else { problem = "Enter a rule name up to 80 characters."; return }
        let current = presets
        guard !current.contains(where: { $0.name.caseInsensitiveCompare(name) == .orderedSame }) else {
            problem = "A saved rule already has that name."
            return
        }
        if writePresets(current + [Preset(name: name, operations: operations)]) {
            presetName = ""
            problem = ""
        }
    }

    @discardableResult
    private func writePresets(_ presets: [Preset]) -> Bool {
        guard let data = try? JSONEncoder().encode(presets), data.count <= 256 * 1024 else {
            problem = "Remove an unused saved rule before adding another."
            return false
        }
        savedRules = data
        return true
    }

    private func preview() {
        let proposedOperations = operations
        previewTask = Task {
            defer { previewTask = nil }
            do {
                let result = try await actions.previewBatch(sources, in: folder, rules: proposedOperations.map(\.rule))
                if !Task.isCancelled, operations == proposedOperations { plan = result; problem = "" }
            } catch {
                if !Task.isCancelled, operations == proposedOperations { problem = error.localizedDescription }
            }
        }
    }

    private func status(for status: BatchRenamePlan.Status) -> String {
        switch status {
        case .ready: return "Ready"
        case .unchanged: return "Unchanged"
        case .invalidName: return "Invalid name"
        case .nameTaken: return "Name taken"
        case .duplicateName: return "Duplicate name"
        }
    }
}

struct ConnectSheet: View {
    @Environment(\.dismiss) private var dismiss
    @State private var address = ""
    @State private var problem = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Connect to Server").font(.headline)
            TextField("Server address", text: $address, prompt: Text("smb://server/share"))
            if !problem.isEmpty { Text(problem).foregroundStyle(.secondary) }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Connect") { connect() }.keyboardShortcut(.defaultAction)
            }
        }.padding(16).frame(width: 420)
    }

    private func connect() {
        guard let url = URL(string: address), let scheme = url.scheme?.lowercased(),
              ["smb", "afp", "nfs"].contains(scheme), url.host?.isEmpty == false else {
            problem = "Enter an SMB, AFP or NFS server address."
            return
        }
        if NSWorkspace.shared.open(url) { dismiss() }
        else { problem = "macOS could not open the server address." }
    }
}
