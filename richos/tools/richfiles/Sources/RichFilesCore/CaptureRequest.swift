import Darwin
import Foundation

public struct CaptureRequest {
    public enum Action: String, CaseIterable, Sendable {
        case capture, captureAgain, copyText, editor, settings, closePin

        public var startsCapture: Bool { self == .capture || self == .captureAgain || self == .copyText }
    }
    public let originPID: pid_t?
    public let action: Action
    public var pinID: UUID?

    private static var directory: URL {
        GlanceHelper.applicationSupport.appendingPathComponent("app.richos.richglance/Capture Requests", isDirectory: true)
    }

    public static func issue(_ action: Action, originPID: pid_t?, pinID: UUID? = nil) throws -> (url: URL, ticket: URL) {
        let directory = Self.directory
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true,
                                                attributes: [.posixPermissions: 0o700])
        guard privateDirectory(directory) else {
            throw GlanceHelper.Problem("RichGlance could not create a private capture request.")
        }
        let id = UUID().uuidString.lowercased()
        let ticket = directory.appendingPathComponent(id)
        let descriptor = open(ticket.path, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600)
        guard descriptor >= 0 else { throw GlanceHelper.Problem("RichGlance could not write a capture request.") }
        let file = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        do {
            let value: String
            if action == .closePin, let pinID { value = "\(action.rawValue):\(pinID.uuidString)" }
            else if action.startsCapture { value = "\(action.rawValue):\(originPID ?? 0)" }
            else { value = action.rawValue }
            try file.write(contentsOf: Data(value.utf8))
            try file.close()
        } catch {
            try? FileManager.default.removeItem(at: ticket)
            throw error
        }
        discardExpired(directory)
        return (URL(string: "richglance-capture://request/" + id)!, ticket)
    }

    private static func privateDirectory(_ directory: URL) -> Bool {
        var info = stat()
        return lstat(directory.path, &info) == 0 && info.st_mode & S_IFMT == S_IFDIR
            && info.st_uid == getuid() && info.st_mode & 0o077 == 0
    }

    private static func discardExpired(_ directory: URL) {
        let keys: [URLResourceKey] = [.contentModificationDateKey, .isRegularFileKey]
        guard let entries = FileManager.default.enumerator(at: directory, includingPropertiesForKeys: keys,
                                                            options: .skipsSubdirectoryDescendants) else { return }
        for case let entry as URL in entries.prefix(32) {
            guard UUID(uuidString: entry.lastPathComponent) != nil,
                  let values = try? entry.resourceValues(forKeys: Set(keys)),
                  values.isRegularFile == true,
                  let changed = values.contentModificationDate, Date().timeIntervalSince(changed) >= 30 else { continue }
            try? FileManager.default.removeItem(at: entry)
        }
    }

    /// The URL is only a wakeup. An unexpired, owner-only ticket is required to start capture.
    public static func consume(_ url: URL) -> Self? {
        guard url.scheme == "richglance-capture", url.host == "request",
              url.pathComponents.count == 2, let id = UUID(uuidString: url.lastPathComponent),
              url.query == nil, url.fragment == nil, url.user == nil, url.password == nil, url.port == nil,
              privateDirectory(directory) else { return nil }
        var info = stat()
        let ticket = directory.appendingPathComponent(id.uuidString.lowercased())
        let descriptor = open(ticket.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard descriptor >= 0 else { return nil }
        let file = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        defer { try? file.close() }
        guard fstat(descriptor, &info) == 0, info.st_mode & S_IFMT == S_IFREG,
              info.st_uid == getuid(), info.st_mode & 0o077 == 0, info.st_nlink == 1,
              (1...63).contains(info.st_size), unlink(ticket.path) == 0 else { return nil }
        let age = Date().timeIntervalSince1970 - Double(info.st_mtimespec.tv_sec)
        guard age >= 0, age < 30 else { return nil }
        let data = (try? file.read(upToCount: 64)) ?? Data()
        guard let value = String(data: data, encoding: .utf8) else { return nil }
        if let pid = pid_t(value) {
            return Self(originPID: pid > 0 ? pid : nil, action: .capture)
        }
        let fields = value.split(separator: ":", omittingEmptySubsequences: false)
        if fields.count == 2, let action = Action(rawValue: String(fields[0])), action.startsCapture,
           let pid = pid_t(fields[1]) {
            return Self(originPID: pid > 0 ? pid : nil, action: action)
        }
        if fields.count == 2, fields[0] == Action.closePin.rawValue,
           let id = UUID(uuidString: String(fields[1])) {
            return Self(originPID: nil, action: .closePin, pinID: id)
        }
        guard let action = Action(rawValue: value), !action.startsCapture, action != .closePin else { return nil }
        return Self(originPID: nil, action: action)
    }
}
