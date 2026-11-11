import AppKit
import QuickLookThumbnailing

public struct WorkspaceApplication: Identifiable, Sendable {
    public let url: URL
    public let name: String
    public var id: URL { url }

    fileprivate init(url: URL) {
        self.url = url
        name = Bundle(url: url)?.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String
            ?? FileManager.default.displayName(atPath: url.path)
    }
}

public struct WorkspaceApplications: Sendable {
    public let defaultApplication: WorkspaceApplication?
    public let alternatives: [WorkspaceApplication]
}

public enum ThumbnailError: LocalizedError {
    case invalidSize
    case downloadRequired

    public var errorDescription: String? {
        switch self {
        case .invalidSize:
            return "Choose a positive thumbnail size and scale."
        case .downloadRequired:
            return "Download this file before previewing it."
        }
    }
}

public enum WorkspaceServices {
    /// Query the system for a selected item. Call on a background worker.
    public static func applications(for file: URL) throws -> WorkspaceApplications {
        try requireLocalFileURL(file)
        let workspace = NSWorkspace.shared
        let preferred = workspace.urlForApplication(toOpen: file)
        var seen = Set<URL>()
        if let preferred {
            seen.insert(preferred.resolvingSymlinksInPath())
        }
        let alternatives = workspace.urlsForApplications(toOpen: file)
            .filter { seen.insert($0.resolvingSymlinksInPath()).inserted }
            .map(WorkspaceApplication.init)
            .sorted {
                let order = $0.name.localizedStandardCompare($1.name)
                return order == .orderedSame ? $0.url.path < $1.url.path : order == .orderedAscending
            }
        return WorkspaceApplications(
            defaultApplication: preferred.map(WorkspaceApplication.init),
            alternatives: alternatives
        )
    }

    /// Call from a user-initiated Open or Open With action.
    public static func open(_ file: URL, with application: WorkspaceApplication? = nil) async throws {
        try requireLocalFileURL(file)
        try Task.checkCancellation()
        let workspace = NSWorkspace.shared
        let configuration = NSWorkspace.OpenConfiguration()
        if let application {
            _ = try await workspace.open([file], withApplicationAt: application.url, configuration: configuration)
        } else {
            _ = try await workspace.open(file, configuration: configuration)
        }
    }

    /// Request the actual thumbnail pixels. The caller can display an icon while this runs.
    public static func thumbnail(for file: URL, size: CGSize, scale: CGFloat = 1) async throws -> CGImage {
        try requireLocalFileURL(file)
        try Task.checkCancellation()
        guard size.width > 0, size.height > 0, scale > 0,
              (size.width * scale).isFinite, (size.height * scale).isFinite else {
            throw ThumbnailError.invalidSize
        }
        // Quick Look may otherwise materialize a known iCloud placeholder just by browsing.
        let values = try file.resourceValues(forKeys: [.isUbiquitousItemKey, .ubiquitousItemDownloadingStatusKey])
        if values.isUbiquitousItem == true && values.ubiquitousItemDownloadingStatus == .notDownloaded {
            throw ThumbnailError.downloadRequired
        }

        let request = QLThumbnailGenerator.Request(
            fileAt: file,
            size: size,
            scale: scale,
            representationTypes: .thumbnail
        )
        let generator = QLThumbnailGenerator.shared
        do {
            return try await withTaskCancellationHandler {
                try Task.checkCancellation()
                let thumbnail = try await generator.generateBestRepresentation(for: request)
                try Task.checkCancellation()
                return thumbnail.cgImage
            } onCancel: {
                generator.cancel(request)
            }
        } catch {
            try Task.checkCancellation()
            throw error
        }
    }
}
