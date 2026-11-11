import Darwin
import Foundation

public struct TextContentQuery: Codable, Sendable {
    public static let maximumFileBytes = 1 << 20
    public static let maximumSearchBytes = 128 * maximumFileBytes
    public static let coverageDescription = "Searches UTF-8 text such as TXT, Markdown, CSV, logs and source files on the selected local volume. Reads up to 1 MiB per file and 128 MiB per search. Cloud placeholders, packages and links are skipped."

    let phrase: String
    let byteBudget: Int

    public init?(phrase: String) {
        self.init(phrase: phrase, byteBudget: Self.maximumSearchBytes)
    }

    init?(phrase: String, byteBudget: Int) {
        let text = phrase.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, byteBudget > 0 else { return nil }
        self.phrase = text
        self.byteBudget = byteBudget
    }

    private enum CodingKeys: String, CodingKey { case phrase }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        let text = try values.decode(String.self, forKey: .phrase)
        guard let query = Self(phrase: text) else {
            throw DecodingError.dataCorruptedError(forKey: .phrase, in: values, debugDescription: "Empty search phrase")
        }
        self = query
    }
}

public struct TextContentCoverage: Sendable, Equatable {
    public internal(set) var searchedFiles = 0
    /// Items or folder branches excluded by the supported formats and read limits.
    public internal(set) var skippedItems = 0
    public internal(set) var bytesRead = 0
}

enum TextContentSearchError: LocalizedError {
    case nonLocalVolume
    case downloadRequired
    case itemChanged

    var errorDescription: String? {
        switch self {
        case .nonLocalVolume: return "Content search requires a local volume."
        case .downloadRequired: return "Download this folder before searching its contents."
        case .itemChanged: return "An item changed while its contents were being read."
        }
    }
}

/// Owned for the duration of one synchronous directory scan. All reads are relative to the
/// opened root and disallow links in every path component.
final class TextContentReader {
    private static let extensions: Set<String> = [
        "txt", "text", "md", "markdown", "rst", "log", "csv", "tsv", "json", "jsonl", "ndjson",
        "yaml", "yml", "toml", "ini", "conf", "xml", "html", "htm", "css", "scss", "js", "mjs",
        "ts", "tsx", "jsx", "swift", "c", "h", "cc", "cpp", "hpp", "m", "mm", "rs", "go", "java",
        "kt", "kts", "py", "rb", "sh", "zsh", "fish", "sql", "tex", "srt", "vtt"
    ]
    private static let extensionlessNames: Set<String> = [
        "readme", "license", "licence", "changelog", "authors", "makefile", "dockerfile"
    ]

    private let descriptor: Int32
    private let pathPrefix: String
    private let device: dev_t
    private let query: TextContentQuery
    private(set) var coverage = TextContentCoverage()

    init(folder: URL, query: TextContentQuery) throws {
        let fd = open(folder.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW_ANY | O_CLOEXEC)
        guard fd >= 0 else { throw currentPOSIXError() }
        var keepDescriptor = false
        defer { if !keepDescriptor { close(fd) } }

        var information = stat()
        guard fstat(fd, &information) == 0 else { throw currentPOSIXError() }
        guard !Self.isDataless(information) else { throw TextContentSearchError.downloadRequired }
        var filesystem = statfs()
        guard fstatfs(fd, &filesystem) == 0 else { throw currentPOSIXError() }
        guard filesystem.f_flags & UInt32(MNT_LOCAL) != 0 else { throw TextContentSearchError.nonLocalVolume }

        descriptor = fd
        pathPrefix = folder.path.hasSuffix("/") ? folder.path : folder.path + "/"
        device = information.st_dev
        self.query = query
        keepDescriptor = true
    }

    deinit {
        close(descriptor)
    }

    // The scan has no suspension points, so this temporary policy applies to its worker
    // thread only. See Apple's TN3150 on avoiding implicit File Provider materialization.
    static func disableDatalessMaterialization() throws -> Int32 {
        let type = IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES
        let scope = IOPOL_SCOPE_THREAD
        let previous = getiopolicy_np(type, scope)
        guard previous >= 0 else { throw currentPOSIXError() }
        guard setiopolicy_np(type, scope, IOPOL_MATERIALIZE_DATALESS_FILES_OFF) == 0 else {
            throw currentPOSIXError()
        }
        return previous
    }

    static func restoreDatalessMaterialization(_ previous: Int32) {
        _ = setiopolicy_np(IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES, IOPOL_SCOPE_THREAD, previous)
    }

    func mayDescend(into entry: FileEntry) throws -> Bool {
        guard !entry.needsDownload, let status = try safeStatus(for: relativePath(entry.url)),
              status.st_mode & S_IFMT == S_IFDIR else { return skip() }
        return true
    }

    func matches(_ entry: FileEntry, isCancelled: () -> Bool) throws -> Bool {
        if isCancelled() { throw CancellationError() }
        guard entry.kind == .regularFile, !entry.needsDownload,
              Self.extensions.contains(entry.url.pathExtension.lowercased())
                || Self.extensionlessNames.contains(entry.name.lowercased()) else { return skip() }

        let path = try relativePath(entry.url)
        guard let before = try safeStatus(for: path), before.st_mode & S_IFMT == S_IFREG,
              let count = Int(exactly: before.st_size), count >= 0,
              count <= TextContentQuery.maximumFileBytes,
              count <= query.byteBudget - coverage.bytesRead else { return skip() }

        let fd = openat(descriptor, path, O_RDONLY | O_NOFOLLOW_ANY | O_NONBLOCK | O_NOCTTY | O_CLOEXEC)
        guard fd >= 0 else {
            if errno == ELOOP { return skip() }
            throw currentPOSIXError()
        }
        defer { close(fd) }

        var opened = stat()
        guard fstat(fd, &opened) == 0 else { throw currentPOSIXError() }
        guard opened.st_mode & S_IFMT == S_IFREG, !Self.isDataless(opened),
              sameFileVersion(before, opened) else { return skip() }

        var data = Data(count: count)
        try data.withUnsafeMutableBytes { (buffer: UnsafeMutableRawBufferPointer) in
            guard let address = buffer.baseAddress else { return }
            var offset = 0
            while offset < count {
                if isCancelled() { throw CancellationError() }
                let amount = read(fd, address.advanced(by: offset), min(32_768, count - offset))
                if amount < 0 {
                    if errno == EINTR { continue }
                    throw currentPOSIXError()
                }
                guard amount > 0 else { throw TextContentSearchError.itemChanged }
                offset += amount
                coverage.bytesRead += amount
            }
        }
        if isCancelled() { throw CancellationError() }
        var after = stat()
        guard fstat(fd, &after) == 0 else { throw currentPOSIXError() }
        guard sameFileVersion(opened, after) else { throw TextContentSearchError.itemChanged }
        guard !data.contains(0), let text = String(data: data, encoding: .utf8) else { return skip() }

        coverage.searchedFiles += 1
        return text.range(of: query.phrase, options: .caseInsensitive) != nil
    }

    private func relativePath(_ location: URL) throws -> String {
        try requireLocalFileURL(location)
        guard location.path.hasPrefix(pathPrefix) else { throw TextContentSearchError.itemChanged }
        let relative = String(location.path.dropFirst(pathPrefix.count))
        let components = relative.split(separator: "/", omittingEmptySubsequences: false)
        guard !relative.isEmpty,
              components.allSatisfy({ !$0.isEmpty && $0 != "." && $0 != ".." }) else {
            throw TextContentSearchError.itemChanged
        }
        return relative
    }

    private func safeStatus(for path: String) throws -> stat? {
        var information = stat()
        guard fstatat(descriptor, path, &information, AT_SYMLINK_NOFOLLOW_ANY) == 0 else {
            if errno == ELOOP { return nil }
            throw currentPOSIXError()
        }
        guard information.st_dev == device, !Self.isDataless(information) else { return nil }
        return information
    }

    private func skip() -> Bool {
        coverage.skippedItems += 1
        return false
    }

    private static func isDataless(_ status: stat) -> Bool {
        status.st_flags & UInt32(SF_DATALESS) != 0
    }
}

private func sameFileVersion(_ first: stat, _ second: stat) -> Bool {
    first.st_dev == second.st_dev && first.st_ino == second.st_ino && first.st_size == second.st_size
        && first.st_birthtimespec.tv_sec == second.st_birthtimespec.tv_sec
        && first.st_birthtimespec.tv_nsec == second.st_birthtimespec.tv_nsec
        && first.st_mtimespec.tv_sec == second.st_mtimespec.tv_sec
        && first.st_mtimespec.tv_nsec == second.st_mtimespec.tv_nsec
        && first.st_ctimespec.tv_sec == second.st_ctimespec.tv_sec
        && first.st_ctimespec.tv_nsec == second.st_ctimespec.tv_nsec
}

private func currentPOSIXError() -> POSIXError {
    POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
}
