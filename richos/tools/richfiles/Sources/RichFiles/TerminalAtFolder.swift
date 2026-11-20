import AppKit
import RichFilesCore

enum TerminalAtFolder {
    @MainActor
    static func open(_ folder: FileEntry, inNewTab: Bool) throws {
        let current = try FileEntry(url: folder.url)
        guard current.kind == .directory, current.identity == folder.identity else {
            throw FileOperationError.sourceChanged(folder.url)
        }

        // Terminal's folder services take a file URL and handle quoting for
        // the user's shell. Leave the general clipboard alone.
        let pasteboard = NSPasteboard.withUniqueName()
        defer { pasteboard.releaseGlobally() }
        let service = inNewTab ? "New Terminal Tab at Folder" : "New Terminal at Folder"
        guard pasteboard.writeObjects([current.url as NSURL]),
              NSPerformService(service, pasteboard) else {
            throw ServiceUnavailable()
        }
    }

    private struct ServiceUnavailable: LocalizedError {
        var errorDescription: String? { "The Terminal folder service is unavailable." }
    }
}
