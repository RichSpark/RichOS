import AppKit
import UniformTypeIdentifiers

@MainActor
enum FolderAssociation {
    static let previousKey = "richfiles.previousFolderHandler"

    static var current: URL? { NSWorkspace.shared.urlForApplication(toOpen: UTType.folder) }

    static func change(to url: URL, replacing previous: URL, remember: Bool) async throws {
        guard current?.resolvingSymlinksInPath() == previous.resolvingSymlinksInPath() else {
            throw ChangeError.changed
        }
        guard FileManager.default.fileExists(atPath: url.path) else { throw ChangeError.unavailable }
        try await NSWorkspace.shared.setDefaultApplication(at: url, toOpen: UTType.folder)
        guard current?.resolvingSymlinksInPath() == url.resolvingSymlinksInPath() else {
            throw ChangeError.unconfirmed
        }
        RichFilesInstallation.preferences.set(remember ? previous.path : "", forKey: previousKey)
    }

    private enum ChangeError: LocalizedError {
        case changed, unavailable, unconfirmed

        var errorDescription: String? {
            switch self {
            case .changed: return "The folder association changed. Check the current app and try again."
            case .unavailable: return "The previous app is unavailable."
            case .unconfirmed: return "macOS did not confirm the folder association."
            }
        }
    }
}
