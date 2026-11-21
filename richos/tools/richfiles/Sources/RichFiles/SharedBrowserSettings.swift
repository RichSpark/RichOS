import AppKit
import RichFilesCore

/// The controls on the RichOS information page share the standalone app's
/// settings and use the same macOS folder and picture association operations.
@MainActor
public enum SharedBrowserSettings {
    private static let keysPreference = "richfiles.windowsKeys"
    private static var applying = false

    private struct State: Encodable {
        let folders: Bool
        let pictures: Bool
        let picturesOpened: Int
        let pictureFormats: Int
        let windowsKeys: Bool
        let message: String?
    }

    public static func update(_ option: Int32, enabled: Bool) async -> Data? {
        let preferences = RichFilesInstallation.preferences
        let glance = GlanceDefaults.shared
        var message: String?
        if applying {
            message = "RichFiles is already updating its settings."
        } else {
            applying = true
            defer { applying = false }
            switch option {
            case 1:
                let old = FolderAssociation.current
                if isRichFiles(old) != enabled {
                    let saved = preferences.string(forKey: FolderAssociation.previousKey)
                    let previous = saved.flatMap { $0.isEmpty ? nil : URL(fileURLWithPath: $0) }
                    let destination = enabled ? RichFilesInstallation.bundledApplication
                        : previous ?? NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.finder")
                    if let old, let destination {
                        do { try await FolderAssociation.change(to: destination, replacing: old, remember: enabled) }
                        catch { message = error.localizedDescription }
                    } else {
                        message = "The folder association is unavailable."
                    }
                }
            case 2:
                glance.refresh()
                if enabled && glance.viewer == nil {
                    message = "RichGlance is unavailable."
                } else {
                    message = await withCheckedContinuation { continuation in
                        glance.setEnabled(enabled) { continuation.resume(returning: $0) }
                    }
                }
            case 3:
                preferences.set(enabled, forKey: keysPreference)
            default: break
            }
        }
        glance.refresh()
        let opened = GlanceFormat.allCases.filter { glance.current[$0]?.isGlance == true }.count
        return try? JSONEncoder().encode(State(
            folders: isRichFiles(FolderAssociation.current),
            pictures: glance.state.enabled,
            picturesOpened: opened,
            pictureFormats: GlanceFormat.allCases.count,
            windowsKeys: preferences.object(forKey: keysPreference) as? Bool ?? true,
            message: message ?? glance.problem
        ))
    }

    private static func isRichFiles(_ url: URL?) -> Bool {
        url.flatMap(Bundle.init(url:))?.bundleIdentifier == "app.richos.richfiles"
    }
}
