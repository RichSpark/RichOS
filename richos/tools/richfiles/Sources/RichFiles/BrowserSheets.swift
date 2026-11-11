import AppKit
import QuickLookUI
import RichFilesCore
import SwiftUI
import UniformTypeIdentifiers

struct NameRequest {
    let source: FileReference?
    let folder: URL
    let directory: Bool
    let name: String
}

enum BrowserSheet: Identifiable {
    case name(NameRequest)
    case properties(FileEntry)
    case preview(FileEntry)
    case openWith(FileEntry, WorkspaceApplications)
    case zip(ZipCatalog)
    case createZip([FileReference], URL)
    case diskUsage(URL)
    case checksum(FileReference)
    case duplicates(URL)
    case trash, transfers, searchOptions, batch([FileReference]), connect, workspaces

    var id: String {
        switch self {
        case .name: return "name"
        case .properties: return "properties"
        case .preview: return "preview"
        case .openWith: return "openWith"
        case .zip: return "zip"
        case .createZip: return "createZip"
        case .diskUsage: return "diskUsage"
        case .checksum: return "checksum"
        case .duplicates: return "duplicates"
        case .trash: return "trash"
        case .transfers: return "transfers"
        case .searchOptions: return "searchOptions"
        case .batch: return "batch"
        case .connect: return "connect"
        case .workspaces: return "workspaces"
        }
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
    let model: BrowserModel
    @Environment(\.dismiss) private var dismiss
    @State private var workspaces: [NamedWorkspace] = []
    @State private var selected: UUID?
    @State private var name = ""
    @State private var problem = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Workspaces").font(.headline)
            List(workspaces, selection: $selected) { workspace in
                Text(workspace.name).tag(workspace.id)
            }
            HStack {
                TextField("Name this set of tabs", text: $name)
                Button("Save Current Workspace") { save() }
                    .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            if !problem.isEmpty { Text(problem).font(.callout).foregroundStyle(.secondary) }
            HStack {
                Button("Remove Workspace") {
                    if let selected { model.removeWorkspace(selected) }
                    selected = nil
                    workspaces = model.namedWorkspaces()
                }.disabled(selected == nil)
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Open Here") {
                    guard let workspace = workspaces.first(where: { $0.id == selected }) else { return }
                    if model.restoreSession(workspace.session) { dismiss() }
                    else { problem = "RichFiles could not read that workspace." }
                }.keyboardShortcut(.defaultAction).disabled(selected == nil)
            }
        }.padding(16).frame(width: 470, height: 320)
            .onAppear { workspaces = model.namedWorkspaces() }
    }

    private func save() {
        do {
            try model.saveWorkspace(named: name)
            workspaces = model.namedWorkspaces()
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
        request.source != nil && !request.directory &&
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

    var body: some View {
        Group {
            if let thumbnail {
                Image(decorative: thumbnail, scale: 1).resizable().scaledToFit()
            } else {
                Image(systemName: fileSymbol(entry)).font(.system(size: max(20, side * 0.45)))
                    .foregroundStyle(entry.kind == .directory ? Color.accentColor : .secondary)
            }
        }.frame(width: side, height: side)
            .accessibilityHidden(true)
            .task(id: "\(entry.url.absoluteString):\(entry.modified?.timeIntervalSince1970 ?? 0):\(side)") {
                thumbnail = nil
                guard entry.kind == .regularFile, !entry.needsDownload else { return }
                let image = try? await WorkspaceServices.thumbnail(for: entry.url, size: CGSize(width: side, height: side), scale: 2)
                if !Task.isCancelled { thumbnail = image }
            }
    }
}

struct NativePreview: View {
    let entry: FileEntry

    var body: some View {
        if entry.needsDownload {
            Label("Download this file before previewing it.", systemImage: "icloud.and.arrow.down")
                .foregroundStyle(.secondary)
        } else if entry.kind == .regularFile {
            QuickLookView(url: entry.url)
        } else {
            FileThumbnail(entry: entry, side: 110)
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
                do { properties = try await Task.detached { try FileProperties(url: entry.url) }.value }
                catch { problem = error.localizedDescription }
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
    let apps: WorkspaceApplications
    @Environment(\.dismiss) private var dismiss
    @State private var problem = ""
    @State private var selected: WorkspaceApplication?
    @State private var confirmDefault = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Open With").font(.headline)
            Text(entry.name).lineLimit(1).help(entry.name)
            List {
                if let preferred = apps.defaultApplication { applicationRow(preferred, preferred: true) }
                ForEach(apps.alternatives) { applicationRow($0, preferred: false) }
                if apps.defaultApplication == nil && apps.alternatives.isEmpty {
                    Text("macOS found no associated apps.").foregroundStyle(.secondary)
                }
            }.frame(minHeight: 125)
            if !problem.isEmpty { Text(problem).foregroundStyle(.secondary) }
            HStack {
                Button("Set Default App…") { confirmDefault = true }.disabled(selected == nil)
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Open") { open() }.keyboardShortcut(.defaultAction).disabled(selected == nil)
            }
        }.padding(15).frame(width: 450, height: 330)
            .onAppear { selected = apps.defaultApplication ?? apps.alternatives.first }
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
    }

    private func open() {
        guard let selected else { return }
        Task {
            do { try await WorkspaceServices.open(entry.url, with: selected); dismiss() }
            catch { problem = error.localizedDescription }
        }
    }

    private func setDefault() {
        guard let selected else { return }
        Task {
            do {
                guard let type = try entry.url.resourceValues(forKeys: [.contentTypeKey]).contentType else {
                    problem = "macOS did not identify this file type."
                    return
                }
                try await NSWorkspace.shared.setDefaultApplication(at: selected.url, toOpen: type)
                let actual = NSWorkspace.shared.urlForApplication(toOpen: type)?.resolvingSymlinksInPath()
                problem = actual == selected.url.resolvingSymlinksInPath()
                    ? "Default app updated." : "macOS did not confirm the change."
            } catch { problem = error.localizedDescription }
        }
    }
}

@MainActor
struct TrashSheet: View {
    @ObservedObject var actions: BrowserActions
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Trash").font(.headline)
            if actions.trashItems.isEmpty {
                Text("Items moved to Trash by RichFiles appear here.")
                    .foregroundStyle(.secondary)
            }
            List(actions.trashItems) { item in
                VStack(alignment: .leading, spacing: 4) {
                    Text(item.originalURL.lastPathComponent).font(.body.weight(.medium))
                    Text(item.originalURL.deletingLastPathComponent().path).font(.caption).textSelection(.enabled)
                    Text(formattedDate(item.trashedAt)).font(.caption).foregroundStyle(.secondary)
                    HStack {
                        Button("Restore") { actions.restore(item) }
                        Button("Restore To…") { chooseFolder(for: item) }
                    }.disabled(actions.busy || !item.canRestore)
                }.padding(.vertical, 3)
            }
            if !actions.status.isEmpty { Text(actions.status).font(.callout).foregroundStyle(.secondary) }
            HStack {
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
        }.padding(14).frame(width: 560, height: 400)
    }

    private func chooseFolder(for item: BrowserTrashItem) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Restore Here"
        panel.begin { result in
            if result == .OK, let folder = panel.url { actions.restore(item, into: folder) }
        }
    }
}

@MainActor
struct TransferSheet: View {
    @ObservedObject var actions: BrowserActions
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Transfers").font(.headline)
            if let transfer = actions.transfer {
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
                if transfer.cancelled { Text("Transfer cancelled.") }
                List(transfer.issues) { issue in
                    VStack(alignment: .leading) {
                        Text(issue.name).font(.body.weight(.medium))
                        Text(issue.message).font(.callout).foregroundStyle(.secondary)
                    }
                }
                HStack {
                    if transfer.running { Button("Cancel Transfer") { actions.cancelTransfer() } }
                    if actions.canRetryTransfer && !actions.busy {
                        Button("Retry") { actions.retryTransfer() }
                        Button("Retry, Keep Both") { actions.retryTransfer(conflict: .keepBoth) }
                    }
                    Spacer()
                    Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
                }
            } else {
                Text("No transfers.").foregroundStyle(.secondary)
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
        }.padding(16).frame(width: 545, height: 345)
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
                    model.searchText = queryText
                    model.searchExtensions = extensions
                    model.searchRecursively = recursively
                    model.searchContent = content
                    model.searchMinimumBytes = Int64(minimum)
                    model.searchMaximumBytes = Int64(maximum)
                    let end = Calendar.current.date(byAdding: .day, value: 1, to: Calendar.current.startOfDay(for: through)) ?? through
                    model.searchModified = byDate ? Calendar.current.startOfDay(for: from)...end.addingTimeInterval(-0.000001) : nil
                    model.submitSearch()
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
}

@MainActor
struct BatchRenameSheet: View {
    let sources: [FileReference]
    let folder: URL
    @ObservedObject var actions: BrowserActions
    @Environment(\.dismiss) private var dismiss
    @State private var input = Input()
    @State private var plan: BatchRenamePlan?
    @State private var problem = ""
    @State private var reading = false

    private enum Method: String, CaseIterable, Identifiable {
        case number = "Number", prefix = "Prefix", suffix = "Suffix", replace = "Replace"
        var id: Self { self }
    }
    private struct Input: Equatable {
        var method = Method.number
        var text = ""
        var replacement = ""
        var start = 1
        var digits = 3
        var rule: BatchRenameRule {
            switch method {
            case .number: return .number(prefix: text, start: start, minimumDigits: digits)
            case .prefix: return .prefix(text)
            case .suffix: return .suffix(text)
            case .replace: return .replace(search: text, replacement: replacement)
            }
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Batch Rename").font(.headline)
            Picker("Method", selection: $input.method) {
                ForEach(Method.allCases) { Text($0.rawValue).tag($0) }
            }.pickerStyle(.segmented)
            TextField(input.method == .replace ? "Find text" : "Text", text: $input.text)
            if input.method == .replace { TextField("Replacement", text: $input.replacement) }
            if input.method == .number {
                HStack {
                    TextField("Start at", value: $input.start, format: .number)
                    TextField("Minimum digits", value: $input.digits, format: .number)
                }
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
            }
            if !problem.isEmpty { Text(problem).foregroundStyle(.secondary) }
            HStack {
                Button("Preview") { preview() }.disabled(reading)
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Apply") {
                    if let plan { actions.apply(plan); dismiss() }
                }.disabled(plan?.canApply != true || reading || actions.busy)
            }
        }.padding(16).frame(width: 650, height: 430)
            .onChange(of: input) { _ in plan = nil; problem = "" }
    }

    private func preview() {
        let proposedInput = input
        reading = true
        Task {
            defer { reading = false }
            do {
                let result = try await actions.previewBatch(sources, in: folder, rule: proposedInput.rule)
                if input == proposedInput { plan = result; problem = "" }
            } catch { if input == proposedInput { problem = error.localizedDescription } }
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
