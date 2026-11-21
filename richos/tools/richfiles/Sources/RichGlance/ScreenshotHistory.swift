import AppKit
import Darwin
import ImageIO

/// RichOS Settings uses the same files and advisory lock. Screenshots are only written by RichGlance.
struct ScreenshotStore: Sendable {
    enum Action: String { case copy, editor
        var title: String { self == .copy ? "Copy Image" : "Open in Editor" }
    }

    enum Retention: String, CaseIterable {
        case week = "7"
        case forever

        var title: String { self == .week ? "7 days" : "Forever" }
        var duration: String { self == .week ? "for 7 days" : "until you delete it" }

        func expires(_ created: Date, at now: Date) -> Bool {
            self == .week && now.timeIntervalSince(created) >= 7 * 86_400
        }
    }

    struct Entry: Identifiable, Equatable {
        let id: UUID
        let created: Date
        let scale: CGFloat
        let kind: ScreenshotKind
        let name: String?
        let edits: ScreenshotEdits

        let edited: Bool

        var redactions: Int { edits.marks.filter { $0.tool == .redact }.count }
    }

    private struct Record: Codable {
        let originalSize: CGSize?
        let created: Date
        let scale: CGFloat
        let kind: ScreenshotKind?
        let name: String?
        let edits: ScreenshotEdits
    }

    private let root: URL
    private var history: URL { root.appendingPathComponent("Recent Captures", isDirectory: true) }
    private var files: FileManager { .default }

    init(root: URL? = nil) {
        self.root = root ?? FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/app.richos.richglance/Screenshots", isDirectory: true)
    }

    var action: Action { readSetting("return-action").flatMap(Action.init(rawValue:)) ?? .copy }
    var keepsHistory: Bool {
        do {
            return try String(contentsOf: root.appendingPathComponent("keep-history"), encoding: .utf8) == "on"
        } catch let error as CocoaError {
            return error.code == .fileReadNoSuchFile
        } catch {
            return false
        }
    }
    var retention: Retention { readSetting("retention").flatMap(Retention.init(rawValue:)) ?? .week }
    var showsShelf: Bool { readSetting("shelf-visibility") != "closed" }

    func setShelfVisible(_ visible: Bool) throws {
        try withLock { try writeSetting("shelf-visibility", visible ? "open" : "closed") }
    }

    func setRetention(_ value: Retention, now: Date = Date()) throws {
        try withLock {
            try writeSetting("retention", value.rawValue)
            if keepsHistory, value == .week { _ = try entriesWhileLocked(now: now) }
        }
    }
    var delay: Int { readSetting("delay").flatMap(Int.init).flatMap { [0, 3, 5, 10].contains($0) ? $0 : nil } ?? 0 }
    var includesPointer: Bool { readSetting("pointer") == "on" }

    func setAction(_ value: Action) throws { try withLock { try writeSetting("return-action", value.rawValue) } }
    func setDelay(_ value: Int) throws { try withLock { try writeSetting("delay", String(value)) } }
    func setPointer(_ value: Bool) throws { try withLock { try writeSetting("pointer", value ? "on" : "off") } }

    func setHistory(_ enabled: Bool) throws {
        try withLock {
            if !enabled, files.fileExists(atPath: history.path) { try files.removeItem(at: history) }
            try writeSetting("keep-history", enabled ? "on" : "off")
        }
    }

    func entries(now: Date = Date()) throws -> [Entry] {
        guard keepsHistory else { return [] }
        return try withLock {
            guard keepsHistory else { return [] }
            return try entriesWhileLocked(now: now)
        }
    }

    @discardableResult
    func retain(_ document: ScreenshotDocument, now: Date = Date(), existingOnly: Bool = false) throws -> Bool {
        guard keepsHistory else { return false }
        return try withLock {
            guard keepsHistory else { return false }
            try createPrivateDirectory(history)
            let existing = try entriesWhileLocked(now: now)
            guard !retention.expires(document.created, at: now) else { return false }
            let destination = folder(document.id)
            let record = Record(originalSize: document.bounds.size, created: document.created, scale: document.scale, kind: document.kind, name: document.name, edits: document.edits)
            let data = try JSONEncoder().encode(record)
            if existing.contains(where: { $0.id == document.id }) {
                try data.write(to: destination.appendingPathComponent("Edits.json"), options: .atomic)
            } else {
                guard !existingOnly else { return false }
                guard let png = ScreenshotRenderer.png(document.original) else { throw CocoaError(.fileWriteUnknown) }
                let staging = history.appendingPathComponent(".pending-\(UUID().uuidString)", isDirectory: true)
                try createPrivateDirectory(staging)
                defer { try? files.removeItem(at: staging) }
                try png.write(to: staging.appendingPathComponent("Original.png"), options: .atomic)
                try data.write(to: staging.appendingPathComponent("Edits.json"), options: .atomic)
                try files.moveItem(at: staging, to: destination)
            }
            return true
        }
    }

    func load(_ entry: Entry) throws -> ScreenshotDocument? {
        guard keepsHistory else { return nil }
        return try withLock {
            guard keepsHistory,
                  let fresh = try entriesWhileLocked(now: Date()).first(where: { $0.id == entry.id }),
                  let source = CGImageSourceCreateWithURL(folder(entry.id).appendingPathComponent("Original.png") as CFURL, nil),
                  let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return nil }
            return ScreenshotDocument(original: image, scale: fresh.scale, kind: fresh.kind, id: fresh.id, created: fresh.created, name: fresh.name, edits: fresh.edits)
        }
    }

    func thumbnail(_ entry: Entry) -> CGImage? {
        guard keepsHistory,
              let source = CGImageSourceCreateWithURL(folder(entry.id).appendingPathComponent("Original.png") as CFURL, nil),
              let sourceImage = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return nil }
        let document = ScreenshotDocument(original: sourceImage, scale: entry.scale, kind: entry.kind,
                                          id: entry.id, created: entry.created, edits: entry.edits)
        return ScreenshotRenderer.thumbnail(document)
    }

    func remove(_ id: UUID) throws {
        try withLock {
            let url = folder(id)
            if files.fileExists(atPath: url.path) { try files.removeItem(at: url) }
        }
    }

    func clear() throws {
        try withLock {
            if files.fileExists(atPath: history.path) { try files.removeItem(at: history) }
        }
    }

    private func entriesWhileLocked(now: Date) throws -> [Entry] {
        guard files.fileExists(atPath: history.path) else { return [] }
        try createPrivateDirectory(history)
        let policy = retention
        var entries: [Entry] = []
        for url in try files.contentsOfDirectory(at: history, includingPropertiesForKeys: nil) {
            if url.lastPathComponent.hasPrefix(".pending-") {
                try files.removeItem(at: url)
                continue
            }
            guard let id = UUID(uuidString: url.lastPathComponent),
                  let bytes = try? Data(contentsOf: url.appendingPathComponent("Edits.json")),
                  let record = try? JSONDecoder().decode(Record.self, from: bytes) else { continue }
            if policy.expires(record.created, at: now) {
                try files.removeItem(at: url)
            } else {
                entries.append(Entry(id: id, created: record.created, scale: record.scale, kind: record.kind ?? .region,
                                     name: record.name, edits: record.edits,
                                     edited: record.originalSize.map { CGRect(origin: .zero, size: $0) != record.edits.crop } ?? false))
            }
        }
        return entries.sorted { $0.created == $1.created ? $0.id.uuidString < $1.id.uuidString : $0.created > $1.created }
    }

    private func folder(_ id: UUID) -> URL { history.appendingPathComponent(id.uuidString, isDirectory: true) }

    private func readSetting(_ key: String) -> String? {
        try? String(contentsOf: root.appendingPathComponent(key), encoding: .utf8)
    }

    private func writeSetting(_ key: String, _ value: String) throws {
        try Data(value.utf8).write(to: root.appendingPathComponent(key), options: .atomic)
    }

    private func createPrivateDirectory(_ url: URL) throws {
        try files.createDirectory(at: url, withIntermediateDirectories: true,
                                  attributes: [.posixPermissions: 0o700])
        var info = stat()
        guard lstat(url.path, &info) == 0, info.st_mode & S_IFMT == S_IFDIR,
              info.st_uid == getuid(), info.st_mode & 0o077 == 0 else {
            throw CocoaError(.fileWriteNoPermission)
        }
    }

    private func withLock<T>(_ body: () throws -> T) throws -> T {
        try createPrivateDirectory(root)
        let descriptor = open(root.appendingPathComponent(".lock").path, O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC, 0o600)
        guard descriptor >= 0 else { throw CocoaError(.fileWriteNoPermission) }
        defer { close(descriptor) }
        guard flock(descriptor, LOCK_EX) == 0 else { throw CocoaError(.fileWriteNoPermission) }
        return try body()
    }
}
