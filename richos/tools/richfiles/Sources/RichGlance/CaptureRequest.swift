import Darwin
import Foundation

enum CaptureRequest {
    /// The URL is only a wakeup. An unexpired, owner-only ticket is required to start capture.
    static func consume(_ url: URL) -> Bool {
        guard url.scheme == "richglance-capture", url.host == "request",
              url.pathComponents.count == 2, let id = UUID(uuidString: url.lastPathComponent),
              url.query == nil, url.fragment == nil, url.user == nil, url.password == nil, url.port == nil,
              let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else { return false }
        let directory = support.appendingPathComponent("app.richos.richglance/Capture Requests", isDirectory: true)
        var info = stat()
        guard lstat(directory.path, &info) == 0, info.st_mode & S_IFMT == S_IFDIR,
              info.st_uid == getuid(), info.st_mode & 0o077 == 0 else { return false }
        let ticket = directory.appendingPathComponent(id.uuidString.lowercased())
        let descriptor = open(ticket.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard descriptor >= 0 else { return false }
        let file = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        defer { try? file.close() }
        guard fstat(descriptor, &info) == 0, info.st_mode & S_IFMT == S_IFREG,
              info.st_uid == getuid(), info.st_mode & 0o077 == 0, info.st_nlink == 1,
              info.st_size == 0, unlink(ticket.path) == 0 else { return false }
        let age = Date().timeIntervalSince1970 - Double(info.st_mtimespec.tv_sec)
        return age >= 0 && age < 30
    }
}
