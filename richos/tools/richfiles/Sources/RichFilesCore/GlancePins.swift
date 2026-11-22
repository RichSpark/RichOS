import AppKit
import Darwin
import Foundation

/// Only process identity and random pin IDs are shared with the menu bar helper.
public enum GlancePins {
    public static let changed = Notification.Name("app.richos.richglance.pins-changed")

    private struct Record: Codable {
        let pid: pid_t
        let ids: [UUID]
    }

    public static func publish(_ ids: [UUID]) {
        let store = GlanceHelper.Store()
        do {
            let lock = try store.lock("pins-lock")
            defer { try? lock?.close() }
            let file = store.directory.appendingPathComponent("pins")
            if ids.isEmpty {
                try? FileManager.default.removeItem(at: file)
            } else {
                let record = Record(pid: getpid(), ids: Array(ids.prefix(24)))
                try JSONEncoder().encode(record).write(to: file, options: .atomic)
            }
        } catch { return }
        DistributedNotificationCenter.default().postNotificationName(changed, object: nil, userInfo: nil, deliverImmediately: true)
    }

    @MainActor
    public static func active() -> [UUID] {
        let store = GlanceHelper.Store()
        let file = store.directory.appendingPathComponent("pins")
        let fd = open(file.path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK)
        guard fd >= 0 else { return [] }
        let handle = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
        defer { try? handle.close() }
        var info = stat()
        guard fstat(fd, &info) == 0, info.st_uid == getuid(), info.st_mode & S_IFMT == S_IFREG,
              (1...2048).contains(info.st_size),
              let data = try? handle.read(upToCount: 2049), data.count <= 2048,
              let record = try? JSONDecoder().decode(Record.self, from: data),
              let app = NSRunningApplication(processIdentifier: record.pid),
              !app.isTerminated, app.bundleIdentifier == "app.richos.richglance" else { return [] }
        return Array(record.ids.prefix(24))
    }
}
