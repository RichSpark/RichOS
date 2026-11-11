import AppKit
import Foundation
import RichFilesCore
import SwiftUI

struct ZipItem: Decodable, Identifiable, Sendable {
    let index: Int
    let path: String
    let isDirectory: Bool
    let size: UInt64
    var id: Int { index }
}

struct ZipCatalog: Sendable {
    let archive: FileReference
    let modified: Date?
    let size: Int64?
    let items: [ZipItem]

    func verify() throws {
        let entry = try FileEntry(url: URL(fileURLWithPath: archive.url.path))
        guard try FileReference(url: entry.url).identity == archive.identity,
              entry.modified == modified, entry.size == size else {
            throw FileOperationError.sourceChanged(archive.url)
        }
    }
}

private enum ZipToolError: LocalizedError {
    case helperUnavailable
    case failed(String)

    var errorDescription: String? {
        switch self {
        case .helperUnavailable: return "The ZIP helper is missing from this RichFiles build."
        case .failed(let message): return message.isEmpty ? "The ZIP operation failed." : message
        }
    }
}

enum ArchiveTools {
    private struct PublishedItem: Decodable { let path: String }

    static func list(_ entry: FileEntry) async throws -> ZipCatalog {
        let reference = try FileReference(url: entry.url)
        let snapshot = ZipCatalog(archive: reference, modified: entry.modified, size: entry.size, items: [])
        try snapshot.verify()
        let data = try await run(["list", entry.url.path])
        try snapshot.verify()
        return ZipCatalog(archive: reference, modified: entry.modified, size: entry.size,
                          items: try JSONDecoder().decode([ZipItem].self, from: data))
    }

    static func create(_ sources: [FileReference], in destination: URL, named name: String) async throws -> URL {
        try FileOperations.validateName(name)
        guard !sources.isEmpty else { throw CocoaError(.fileNoSuchFile) }
        for source in sources {
            guard try FileReference(url: source.url).identity == source.identity else {
                throw FileOperationError.sourceChanged(source.url)
            }
        }
        let data = try await run(["create", destination.path, name, "--"] + sources.map { $0.url.path })
        return try publishedURL(data)
    }

    static func extract(_ catalog: ZipCatalog, indices: [Int]?, in destination: URL, named name: String) async throws -> URL {
        try FileOperations.validateName(name)
        try catalog.verify()
        var arguments = ["extract", catalog.archive.url.path, destination.path, name]
        if let indices, !indices.isEmpty { arguments += ["--"] + indices.map(String.init) }
        let data = try await run(arguments)
        return try publishedURL(data)
    }

    private static func publishedURL(_ data: Data) throws -> URL {
        let item = try JSONDecoder().decode(PublishedItem.self, from: data)
        guard item.path.hasPrefix("/") else { throw CocoaError(.fileReadInvalidFileName) }
        return URL(fileURLWithPath: item.path)
    }

    private static func run(_ arguments: [String]) async throws -> Data {
        guard let executable = Bundle.main.executableURL?.deletingLastPathComponent()
            .appendingPathComponent("richfiles-archive"), FileManager.default.isExecutableFile(atPath: executable.path) else {
            throw ZipToolError.helperUnavailable
        }
        let runner = ZipProcess()
        let worker = Task.detached(priority: .userInitiated) { try runner.run(executable, arguments: arguments) }
        return try await withTaskCancellationHandler {
            try await worker.value
        } onCancel: {
            runner.cancel()
        }
    }
}

private final class ZipProcess: @unchecked Sendable {
    private let lock = NSLock()
    private var process: Process?
    private var cancelled = false

    func cancel() {
        lock.lock()
        defer { lock.unlock() }
        cancelled = true
        if process?.isRunning == true { process?.terminate() }
    }

    func run(_ executable: URL, arguments: [String]) throws -> Data {
        let manager = FileManager.default
        let directory = manager.temporaryDirectory.appendingPathComponent("RichFiles-ZIP-\(UUID().uuidString)", isDirectory: true)
        try manager.createDirectory(at: directory, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
        defer { try? manager.removeItem(at: directory) }
        let outputURL = directory.appendingPathComponent("out")
        let errorURL = directory.appendingPathComponent("error")
        try Data().write(to: outputURL, options: .withoutOverwriting)
        try Data().write(to: errorURL, options: .withoutOverwriting)
        let output = try FileHandle(forWritingTo: outputURL)
        let error = try FileHandle(forWritingTo: errorURL)
        defer { try? output.close(); try? error.close() }
        let child = Process()
        child.executableURL = executable
        child.arguments = arguments
        child.standardOutput = output
        child.standardError = error
        try launch(child)
        child.waitUntilExit()
        lock.lock()
        process = nil
        let wasCancelled = cancelled
        lock.unlock()
        if child.terminationStatus != 0 {
            if wasCancelled { throw CancellationError() }
            let details = try FileHandle(forReadingFrom: errorURL)
            defer { try? details.close() }
            let message = String(decoding: (try details.read(upToCount: 4096)) ?? Data(), as: UTF8.self)
            throw ZipToolError.failed(message.trimmingCharacters(in: .whitespacesAndNewlines))
        }
        let result = try FileHandle(forReadingFrom: outputURL)
        defer { try? result.close() }
        let limit = 32 * 1024 * 1024
        let data = (try result.read(upToCount: limit + 1)) ?? Data()
        guard data.count <= limit else { throw CocoaError(.fileReadTooLarge) }
        return data
    }

    private func launch(_ child: Process) throws {
        lock.lock()
        defer { lock.unlock() }
        guard !cancelled else { throw CancellationError() }
        try child.run()
        process = child
    }
}

struct ZipBrowserSheet: View {
    let catalog: ZipCatalog
    let changed: () -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var selected: Set<Int> = []
    @State private var destination: URL
    @State private var name: String
    @State private var task: Task<Void, Never>?
    @State private var status = ""

    init(catalog: ZipCatalog, changed: @escaping () -> Void) {
        self.catalog = catalog
        self.changed = changed
        _destination = State(initialValue: catalog.archive.url.deletingLastPathComponent())
        _name = State(initialValue: catalog.archive.url.deletingPathExtension().lastPathComponent)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(catalog.archive.url.lastPathComponent).font(.headline)
            List(catalog.items, selection: $selected) { item in
                HStack {
                    Label(item.path, systemImage: item.isDirectory ? "folder" : "doc").lineLimit(1).truncationMode(.middle)
                    Spacer()
                    if !item.isDirectory { Text(formattedBytes(Int64(clamping: item.size))).foregroundStyle(.secondary) }
                }.tag(item.index)
            }
            HStack {
                Text(destination.path).lineLimit(1).truncationMode(.middle)
                Spacer()
                Button("Choose Destination…") { chooseDestination() }.disabled(task != nil)
            }
            TextField("New folder name", text: $name).disabled(task != nil)
            if !status.isEmpty { Text(status).foregroundStyle(.secondary) }
            HStack {
                if task != nil { Button("Cancel Extraction") { task?.cancel() } }
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Extract Selected") { extract(Array(selected.sorted())) }.disabled(task != nil || selected.isEmpty)
                Button("Extract All") { extract(nil) }.disabled(task != nil || catalog.items.isEmpty)
            }
        }.padding(15).frame(width: 640, height: 460)
            .onDisappear { task?.cancel() }
    }

    private func chooseDestination() {
        let picker = NSOpenPanel()
        picker.canChooseFiles = false
        picker.canChooseDirectories = true
        picker.allowsMultipleSelection = false
        picker.directoryURL = destination
        picker.prompt = "Choose Folder"
        picker.begin { result in
            if result == .OK, let url = picker.url { destination = url }
        }
    }

    private func extract(_ indices: [Int]?) {
        let folder = destination
        let folderName = name
        status = "Extracting into a new folder…"
        task = Task {
            defer { task = nil }
            do {
                let result = try await ArchiveTools.extract(catalog, indices: indices, in: folder, named: folderName)
                status = "Extracted to \(result.path)"
                changed()
            } catch is CancellationError { status = "Extraction cancelled." }
            catch { status = error.localizedDescription }
        }
    }
}

struct ZipCreateSheet: View {
    let sources: [FileReference]
    let destination: URL
    let changed: () -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var name = "Archive.zip"
    @State private var status = ""
    @State private var task: Task<Void, Never>?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Create ZIP").font(.headline)
            Text("\(sources.count) selected in \(destination.path)").lineLimit(2)
            TextField("ZIP filename", text: $name).disabled(task != nil)
            if !status.isEmpty { Text(status).foregroundStyle(.secondary) }
            HStack {
                if task != nil { Button("Cancel Compression") { task?.cancel() } }
                Spacer()
                Button("Close") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Create") { create() }.keyboardShortcut(.defaultAction)
                    .disabled(task != nil || name.isEmpty)
            }
        }.padding(16).frame(width: 445)
            .onAppear {
                if sources.count == 1, let source = sources.first {
                    name = source.url.deletingPathExtension().lastPathComponent + ".zip"
                }
            }
            .onDisappear { task?.cancel() }
    }

    private func create() {
        let archiveName = name.lowercased().hasSuffix(".zip") ? name : name + ".zip"
        status = "Creating ZIP…"
        task = Task {
            defer { task = nil }
            do {
                let result = try await ArchiveTools.create(sources, in: destination, named: archiveName)
                status = "Created \(result.lastPathComponent)."
                changed()
            } catch is CancellationError { status = "Compression cancelled." }
            catch { status = error.localizedDescription }
        }
    }
}
