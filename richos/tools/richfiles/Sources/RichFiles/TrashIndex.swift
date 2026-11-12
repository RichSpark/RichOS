import Darwin
import Foundation
import RichFilesCore

struct TrashListEntry: Identifiable, Sendable {
    let reference: FileReference?
    let known: BrowserTrashItem?
    let dateAdded: Date?
    let canDelete: Bool

    var id: String { reference?.url.path ?? known?.id.uuidString ?? "" }
    var name: String { known?.originalURL.lastPathComponent ?? reference?.url.lastPathComponent ?? "Unavailable item" }
    var originalFolder: String? { known?.originalURL.deletingLastPathComponent().path }
}

struct TrashIndex: Sendable {
    let entries: [TrashListEntry]
    let inaccessible: Int

    static func read(known: [BrowserTrashItem]) throws -> Self {
        let manager = FileManager.default
        var locations = manager.urls(for: .trashDirectory, in: .userDomainMask)
        if let mounts = try? VolumeInfo.mounted() {
            for volume in mounts.volumes where volume.url.path != "/" && volume.isLocal == true {
                let shared = volume.url.appendingPathComponent(".Trashes", isDirectory: true)
                var status = stat()
                if lstat(shared.path, &status) == 0, status.st_mode & S_IFMT == S_IFDIR {
                    locations.append(shared.appendingPathComponent(String(getuid()), isDirectory: true))
                }
            }
        }
        var knownByPath: [String: BrowserTrashItem] = [:]
        for item in known {
            if let reference = item.reference, knownByPath[reference.url.path] == nil {
                knownByPath[reference.url.path] = item
            }
        }
        var completedFolders: Set<String> = []
        var scannedFolders: Set<String> = []
        var seen: Set<UUID> = []
        var rows: [TrashListEntry] = []
        var inaccessible = 0
        for location in locations where scannedFolders.insert(location.standardizedFileURL.path).inserted {
            try Task.checkCancellation()
            var status = stat()
            if lstat(location.path, &status) != 0 {
                if errno == ENOENT { completedFolders.insert(location.standardizedFileURL.path) }
                else { inaccessible += 1 }
                continue
            }
            guard status.st_mode & S_IFMT == S_IFDIR, status.st_uid == getuid() else {
                inaccessible += 1
                continue
            }
            let items: [URL]
            do {
                items = try manager.contentsOfDirectory(at: location, includingPropertiesForKeys: [.addedToDirectoryDateKey])
            } catch {
                inaccessible += 1
                continue
            }
            var complete = true
            for url in items where url.lastPathComponent != ".DS_Store" {
                try Task.checkCancellation()
                guard let reference = try? FileReference(url: url) else { complete = false; continue }
                let recorded = knownByPath[reference.url.path].flatMap { $0.reference == reference ? $0 : nil }
                if let recorded { seen.insert(recorded.id) }
                let date = recorded?.trashedAt ?? (try? url.resourceValues(forKeys: [.addedToDirectoryDateKey]).addedToDirectoryDate)
                rows.append(TrashListEntry(reference: reference, known: recorded, dateAdded: date, canDelete: true))
            }
            if complete { completedFolders.insert(location.standardizedFileURL.path) }
            else { inaccessible += 1 }
        }
        for item in known where !seen.contains(item.id) {
            if let location = item.trashURL, completedFolders.contains(location.deletingLastPathComponent().standardizedFileURL.path) {
                continue
            }
            rows.append(TrashListEntry(reference: item.reference, known: item, dateAdded: item.trashedAt, canDelete: false))
        }
        var listed: Set<String> = []
        let sorted = rows.filter { listed.insert($0.id).inserted }.sorted { first, second in
            if first.dateAdded != second.dateAdded {
                return (first.dateAdded ?? .distantPast) > (second.dateAdded ?? .distantPast)
            }
            return first.name.localizedStandardCompare(second.name) == .orderedAscending
        }
        return Self(entries: sorted, inaccessible: inaccessible)
    }
}
