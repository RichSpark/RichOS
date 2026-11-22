import AppKit
import RichFilesCore

enum ScreenshotDrag {
    static func write(_ document: ScreenshotDocument) throws -> URL {
        guard let image = ScreenshotRenderer.flattened(document),
              let data = ScreenshotRenderer.png(image) else { throw CocoaError(.fileWriteUnknown) }
        let store = GlanceHelper.Store()
        let lease = try store.lock("drag-exports")
        defer { try? lease?.close() }
        let root = store.directory.appendingPathComponent("Drag Exports", isDirectory: true)
        let manager = FileManager.default
        try manager.createDirectory(at: root, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
        if let copies = try? manager.contentsOfDirectory(at: root, includingPropertiesForKeys: [.creationDateKey]) {
            let expiry = Date().addingTimeInterval(-900)
            for item in copies where UUID(uuidString: item.lastPathComponent) != nil {
                if let date = try? item.resourceValues(forKeys: [.creationDateKey]).creationDate, date < expiry {
                    try? manager.removeItem(at: item)
                }
            }
        }
        let folder = root.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try manager.createDirectory(at: folder, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
        let stamp = DateFormatter()
        stamp.dateFormat = "yyyy-MM-dd 'at' HH.mm.ss"
        let url = folder.appendingPathComponent("RichGlance Capture \(stamp.string(from: document.created)).png")
        do {
            try data.write(to: url, options: .atomic)
            return url
        } catch {
            try? manager.removeItem(at: folder)
            throw error
        }
    }
}
