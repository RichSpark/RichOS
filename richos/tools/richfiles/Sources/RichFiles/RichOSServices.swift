import AppKit
import RichFilesCore

@MainActor
enum RichOSServices {
    static let cleanerTitle = "Add spoken note with RichCleaner"
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
        let board = NSPasteboard.withUniqueName()
        defer { board.releaseGlobally() }
        guard board.setPropertyList(paths, forType: filenamesType) else { return false }
        return invoke(cleanerTitle, board)
    }
}
