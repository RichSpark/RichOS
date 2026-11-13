import AppKit
import RichFilesCore

@MainActor
enum RichOSServices {
    static let cleanerTitle = "Add spoken note with RichCleaner"
    static let finderTitle = "Search with RichFinder"
    static let filenamesType = NSPasteboard.PasteboardType("NSFilenamesPboardType")

    static func addSpokenNote(
        to sources: [FileReference], invoke: (String, NSPasteboard) -> Bool = { NSPerformService($0, $1) }
    ) async throws -> Bool {
        let paths = try await Task.detached(priority: .userInitiated) {
            try sources.map { source in
                guard (try? FileReference(url: source.url)) == source else {
                    throw FileOperationError.sourceChanged(source.url)
                }
                return source.url.path
            }
        }.value
        guard !paths.isEmpty else { return false }

        // The installed RichCleaner service accepts Finder's list-of-paths type.
        return send(cleanerTitle, write: { $0.setPropertyList(paths, forType: filenamesType) }, invoke: invoke)
    }

    static func search(
        _ query: String, invoke: (String, NSPasteboard) -> Bool = { NSPerformService($0, $1) }
    ) -> Bool {
        send(finderTitle, write: { $0.setString(query, forType: .string) }, invoke: invoke)
    }

    private static func send(
        _ title: String, write: (NSPasteboard) -> Bool, invoke: (String, NSPasteboard) -> Bool
    ) -> Bool {
        let board = NSPasteboard.withUniqueName()
        defer { board.releaseGlobally() }
        return write(board) && invoke(title, board)
    }
}
