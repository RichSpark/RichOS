import Darwin
import Foundation

public enum GitReadError: LocalizedError {
    case noGit, noRepository, unreadable, noMain, tooLarge, binary, timedOut, changed

    public var errorDescription: String? {
        switch self {
        case .noGit: return "Git is unavailable in this installation."
        case .noRepository: return "This folder is outside a Git repository."
        case .unreadable: return "Git could not read this repository or version."
        case .noMain: return "This repository has no local main branch."
        case .tooLarge: return "This comparison is too large to display."
        case .binary: return "A text comparison is unavailable for this file."
        case .timedOut: return "Git took too long to read this data. Try refreshing."
        case .changed: return "The repository or file changed. Refresh and select it again."
        }
    }
}

public struct GitCommit: Identifiable, Sendable {
    public let id: String
    public let parents: [String]
    public let date: String
    public let author: String
    public let subject: String
    public let graph: [String]
}

public struct GitLandEvidence: Codable, Sendable, Equatable {
    public let id: String
    public let paths: [String]
    public let before: String?
    public let reviewed: String?
    public let at: UInt64

    public init(id: String, paths: [String], before: String?, reviewed: String?, at: UInt64) {
        self.id = id
        self.paths = paths
        self.before = before
        self.reviewed = reviewed
        self.at = at
    }
}

public struct GitComparison: Sendable {
    public enum Basis: String, Sendable {
        case land = "Recorded integration"
        case parent = "Parent comparison"
        case firstParent = "First-parent comparison"
        case root = "Initial commit"
        case unavailable = "Earlier version unavailable"
    }

    public let before: String?
    public let after: String
    public let basis: Basis
    public let files: [GitChangedFile]
    public let incompleteRenames: Bool
}

public struct GitChangedFile: Identifiable, Sendable {
    public enum Status: String, Sendable {
        case added = "Added"
        case modified = "Modified"
        case renamed = "Renamed"
        case deleted = "Deleted"
        case unresolved = "Earlier version unavailable"
    }

    public var id: String { path }
    public let path: String
    public let oldPath: String?
    public let status: Status
    public let before: String?
    public let after: String?
    fileprivate let regular: Bool

    fileprivate func unresolved() -> Self {
        Self(path: path, oldPath: oldPath, status: .unresolved, before: nil, after: nil, regular: false)
    }
}

public struct GitDiffHunk: Identifiable, Sendable {
    public struct Line: Sendable {
        public let number: Int?
        public let text: String
    }

    public struct Row: Identifiable, Sendable {
        public let id: Int
        public let before: Line?
        public let after: Line?
        public let changed: Bool
    }

    public let id: Int
    public let rows: [Row]
}

/// One read-only client shared by the two native hosts. Git selects the topology,
/// changed objects and patch. Repository text never becomes a command or a pathspec.
public struct GitRepository: Sendable {
    public let root: URL
    public let identity: FileReference
    private let administration: FileReference
    private let git: URL
    private static let patchLimit = 4 * 1024 * 1024
    private static let renameLimit = 500

    public init(folder: URL, git: URL, exactRoot: Bool = false) throws {
        guard folder.isFileURL, git.isFileURL, FileManager.default.isExecutableFile(atPath: git.path) else {
            throw GitReadError.noGit
        }
        let discovered: String
        do {
            discovered = try Self.run(git, folder, ["rev-parse", "--show-toplevel"]).string.dropFinalLineFeed()
        } catch let error as GitCommandFailed where error.code == 128 {
            if error.diagnostic.contains("not a git repository") { throw GitReadError.noRepository }
            throw GitReadError.unreadable
        }
        guard discovered.hasPrefix("/") else { throw GitReadError.unreadable }
        let location = URL(fileURLWithPath: discovered, isDirectory: true).resolvingSymlinksInPath().standardizedFileURL
        guard !exactRoot || location.path == folder.standardizedFileURL.path else { throw GitReadError.changed }
        root = location
        identity = try FileReference(url: location)
        let gitEntry = location.appendingPathComponent(".git")
        guard !Self.isLink(gitEntry) else { throw GitReadError.unreadable }
        administration = try FileReference(url: gitEntry)
        self.git = git
    }

    public var name: String { root.lastPathComponent }

    public func mainTip() throws -> String {
        let result = try call(["rev-parse", "--verify", "--quiet", "refs/heads/main^{commit}"], allow: [0, 1])
        guard result.code == 0 else { throw GitReadError.noMain }
        let id = result.string.dropFinalLineFeed()
        guard Self.isObjectID(id) else { throw GitReadError.unreadable }
        return id
    }

    public func isShallow() throws -> Bool {
        try call(["rev-parse", "--is-shallow-repository"]).string.dropFinalLineFeed() == "true"
    }

    public func checkoutName() throws -> String {
        let result = try call(["symbolic-ref", "--quiet", "--short", "HEAD"], allow: [0, 1])
        if result.code == 0 { return result.string.dropFinalLineFeed() }
        let head = try call(["rev-parse", "--verify", "--short", "HEAD"]).string.dropFinalLineFeed()
        return "Detached at " + head
    }

    public func history(tip: String, limit: Int) throws -> [GitCommit] {
        guard Self.isObjectID(tip), (1...100_000).contains(limit) else { throw GitReadError.unreadable }
        let reply = try call(["log", "--graph", "--no-color", "--no-notes", "--no-decorate", "--no-show-signature",
                              "--max-count=\(limit)", "--format=format:%x00%H%x00%P%x00%aI%x00%an%x00%s", tip, "--"],
                             cap: 32 * 1024 * 1024)
        var commits: [GitCommit] = []
        var continuation: [String] = []
        for line in reply.string.split(separator: "\n", omittingEmptySubsequences: false) {
            let fields = line.split(separator: "\0", maxSplits: 5, omittingEmptySubsequences: false)
            if fields.count == 6, Self.isObjectID(String(fields[1])) {
                if !continuation.isEmpty, let previous = commits.popLast() {
                    commits.append(GitCommit(id: previous.id, parents: previous.parents, date: previous.date,
                                             author: previous.author, subject: previous.subject,
                                             graph: previous.graph + continuation))
                }
                continuation = []
                commits.append(Self.commit(fields, graph: [String(fields[0])]))
            } else if !line.isEmpty {
                continuation.append(String(line))
            }
        }
        if !continuation.isEmpty, let previous = commits.popLast() {
            commits.append(GitCommit(id: previous.id, parents: previous.parents, date: previous.date,
                                     author: previous.author, subject: previous.subject,
                                     graph: previous.graph + continuation))
        }
        return commits
    }

    public func commit(_ id: String) throws -> GitCommit {
        guard Self.isObjectID(id) else { throw GitReadError.unreadable }
        let reply = try call(["log", "-1", "--no-notes", "--no-decorate", "--no-show-signature",
                              "--format=format:%x00%H%x00%P%x00%aI%x00%an%x00%s", id, "--"])
        let parts = reply.string.split(separator: "\0", maxSplits: 5, omittingEmptySubsequences: false)
        guard parts.count == 6, parts[1] == id else { throw GitReadError.unreadable }
        return Self.commit(parts, graph: [])
    }

    private static func commit(_ parts: [Substring], graph: [String]) -> GitCommit {
        GitCommit(id: String(parts[1]), parents: parts[2].split(separator: " ").map(String.init),
                  date: String(parts[3]), author: String(parts[4]), subject: String(parts[5]), graph: graph)
    }

    // Walk the current main ancestry once per requested chat, after its tip is fixed.
    public func recordedLands(_ lands: [GitLandEvidence], on tip: String) throws -> (reachable: Set<String>, offMain: Int) {
        guard Self.isObjectID(tip) else { throw GitReadError.unreadable }
        let all = try call(["rev-list", tip, "--"], cap: 32 * 1024 * 1024)
        let wanted = Set(lands.map(\.id))
        let reachable = Set(all.string.split(separator: "\n").compactMap { text -> String? in
            let id = String(text)
            return wanted.contains(id) ? id : nil
        })
        return (reachable, wanted.subtracting(reachable).count)
    }

    public func comparison(_ commit: GitCommit, land: GitLandEvidence? = nil) throws -> GitComparison {
        let base: String?
        let basis: GitComparison.Basis
        if let land {
            guard land.id == commit.id else { throw GitReadError.unreadable }
            if let before = land.before, let reviewed = land.reviewed,
               Self.isObjectID(before), Self.isObjectID(reviewed), before != commit.id,
               commit.parents == [before, reviewed] || (reviewed == commit.id && (try? isAncestor(before, of: reviewed)) == true) {
                base = before
                basis = .land
            } else if land.before == nil, land.reviewed == nil, !land.paths.isEmpty {
                let witnessed = Set(land.paths.filter(Self.isRelativePath))
                var candidates: [(String, [GitChangedFile], Bool)] = []
                if !witnessed.isEmpty {
                    for parent in commit.parents {
                        let (files, incomplete) = try changes(from: parent, to: commit.id)
                        let destinations = Set(files.filter { $0.status != .deleted }.map(\.path))
                        if !incomplete && destinations == witnessed { candidates.append((parent, files, incomplete)) }
                    }
                }
                if candidates.count == 1, let (parent, files, incomplete) = candidates.first {
                    return GitComparison(before: parent, after: commit.id, basis: .parent,
                                         files: files.filter { witnessed.contains($0.path) }, incompleteRenames: incomplete)
                }
                return unavailable(commit.id, paths: witnessed)
            } else {
                return unavailable(commit.id, paths: Set(land.paths.filter(Self.isRelativePath)))
            }
        } else {
            base = commit.parents.first
            basis = commit.parents.count > 1 ? .firstParent : base == nil ? .root : .parent
        }
        let (files, incomplete) = try changes(from: base, to: commit.id)
        return GitComparison(before: base, after: commit.id, basis: basis, files: files, incompleteRenames: incomplete)
    }

    private func unavailable(_ commit: String, paths: Set<String>) -> GitComparison {
        let files = paths.sorted().map {
            GitChangedFile(path: $0, oldPath: nil, status: .unresolved, before: nil, after: nil, regular: false)
        }
        return GitComparison(before: nil, after: commit, basis: .unavailable, files: files, incompleteRenames: false)
    }

    private func isAncestor(_ before: String, of after: String) throws -> Bool {
        try call(["merge-base", "--is-ancestor", before, after], allow: [0, 1]).code == 0
    }

    private func changes(from before: String?, to after: String) throws -> ([GitChangedFile], Bool) {
        guard Self.isObjectID(after), before.map(Self.isObjectID) ?? true else { throw GitReadError.unreadable }
        var args = ["diff-tree", "-r", "--no-commit-id", "--raw", "-z", "--no-abbrev",
                    "--no-ext-diff", "--no-textconv", "--find-renames=50%", "-l\(Self.renameLimit)"]
        if let before { args += [before, after, "--"] }
        else { args += ["--root", after, "--"] }
        let reply = try call(args, cap: 8 * 1024 * 1024)
        let records = reply.output.split(separator: 0)
        var index = 0
        var files: [GitChangedFile] = []
        let incomplete = reply.diagnostic.contains("rename detection was skipped")
        while index < records.count {
            guard let raw = String(data: records[index], encoding: .utf8), raw.hasPrefix(":") else {
                throw GitReadError.unreadable
            }
            let meta = raw.dropFirst().split(separator: " ")
            guard meta.count == 5, let status = meta[4].first else { throw GitReadError.unreadable }
            let prior = status == "R" || status == "C"
            let trailing = prior ? 2 : 1
            guard index + trailing < records.count else { throw GitReadError.unreadable }
            let oldPath = prior ? String(data: records[index + 1], encoding: .utf8) : nil
            if let path = String(data: records[index + trailing], encoding: .utf8), Self.isRelativePath(path) {
                let previous = String(meta[2])
                let next = String(meta[3])
                let kind: GitChangedFile.Status = status == "A" ? .added : status == "D" ? .deleted : status == "R" ? .renamed : .modified
                let file = GitChangedFile(path: path, oldPath: oldPath, status: kind,
                                          before: previous.allSatisfy { $0 == "0" } ? nil : previous,
                                          after: next.allSatisfy { $0 == "0" } ? nil : next,
                                          regular: [meta[0], meta[1]].allSatisfy { $0 == "000000" || $0.hasPrefix("100") })
                files.append(incomplete && (kind == .added || kind == .deleted) ? file.unresolved() : file)
            }
            index += trailing + 1
        }
        return (files, incomplete)
    }

    public func imageBytes(_ file: GitChangedFile, before: Bool) throws -> Data {
        guard file.regular, let id = before ? file.before : file.after else { throw GitReadError.binary }
        return try blob(id, cap: 12 * 1024 * 1024)
    }

    public func hunks(_ file: GitChangedFile) throws -> [GitDiffHunk] {
        guard file.regular, file.status != .unresolved else { throw GitReadError.binary }
        if let before = file.before, let after = file.after {
            guard Self.isObjectID(before), Self.isObjectID(after) else { throw GitReadError.unreadable }
            guard before != after else { return [] }
            try checkBlobSize(before, cap: Self.patchLimit)
            try checkBlobSize(after, cap: Self.patchLimit)
            let result = try call(["diff", "--no-color", "--no-ext-diff", "--no-textconv", "--no-indent-heuristic",
                                   "--diff-algorithm=myers", "-U3", before, after, "--"], cap: Self.patchLimit)
            guard let patch = String(data: result.output, encoding: .utf8) else { throw GitReadError.binary }
            if patch.contains("\nBinary files ") { throw GitReadError.binary }
            return try GitPatch.hunks(in: patch)
        }
        guard let id = file.after ?? file.before else { return [] }
        let bytes = try blob(id, cap: Self.patchLimit)
        guard !bytes.contains(0), let string = String(data: bytes, encoding: .utf8) else { throw GitReadError.binary }
        return try GitPatch.singleSide(string, added: file.before == nil)
    }

    /// A path read from Git may still name a link or an item swapped since the
    /// comparison opened. Resolve every component in the same checkout.
    public func liveFile(_ file: GitChangedFile) throws -> FileReference {
        try verifyIdentity()
        guard file.status != .unresolved, Self.isRelativePath(file.path) else { throw GitReadError.changed }
        var url = root
        for component in file.path.split(separator: "/") {
            url.appendPathComponent(String(component))
            guard !Self.isLink(url) else { throw GitReadError.changed }
        }
        let reference = try FileReference(url: url)
        _ = try ReviewedFile(reference)
        return reference
    }

    public func verifyIdentity() throws {
        guard try FileReference(url: root) == identity,
              try FileReference(url: administration.url) == administration else { throw GitReadError.changed }
    }

    private func blob(_ id: String, cap: Int) throws -> Data {
        guard Self.isObjectID(id) else { throw GitReadError.unreadable }
        try checkBlobSize(id, cap: cap)
        return try call(["cat-file", "blob", id], cap: cap).output
    }

    private func checkBlobSize(_ id: String, cap: Int) throws {
        let data = try call(["cat-file", "-s", id]).string.dropFinalLineFeed()
        guard let size = Int(data), size >= 0 else { throw GitReadError.unreadable }
        guard size <= cap else { throw GitReadError.tooLarge }
    }

    public static func isObjectID(_ value: String) -> Bool {
        [40, 64].contains(value.utf8.count) && value.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
    }

    public static func isRelativePath(_ path: String) -> Bool {
        !path.isEmpty && !path.hasPrefix("/") && !path.contains("\0")
            && path.split(separator: "/", omittingEmptySubsequences: false).allSatisfy {
                !$0.isEmpty && $0 != "." && $0 != ".." && $0.lowercased() != ".git"
            }
    }

    private static func isLink(_ url: URL) -> Bool {
        var status = stat()
        return lstat(url.path, &status) == 0 && status.st_mode & S_IFMT == S_IFLNK
    }

    private func call(_ args: [String], cap: Int = 1024 * 1024, allow: Set<Int32> = [0]) throws -> GitReply {
        try verifyIdentity()
        let reply = try Self.run(git, root, args, cap: cap, allow: allow)
        try verifyIdentity()
        return reply
    }

    private struct GitCommandFailed: Error {
        let code: Int32
        let diagnostic: String
    }

    private struct GitReply {
        let code: Int32
        let output: Data
        let diagnostic: String
        var string: String { String(decoding: output, as: UTF8.self) }
    }

    private static func run(_ git: URL, _ directory: URL, _ args: [String], cap: Int = 1024 * 1024,
                            allow: Set<Int32> = [0]) throws -> GitReply {
        try Task.checkCancellation()
        let process = Process()
        let output = Pipe()
        let errors = Pipe()
        process.executableURL = git
        process.currentDirectoryURL = directory
        process.arguments = ["--no-pager", "--no-replace-objects", "-c", "core.fsmonitor=false",
                             "-c", "core.hooksPath=/dev/null", "-c", "color.ui=false",
                             "-c", "diff.external=", "-c", "log.showSignature=false", "-c", "diff.submodule=short"] + args
        process.environment = [
            "PATH": git.deletingLastPathComponent().path + ":/usr/bin:/bin", "LC_ALL": "C",
            "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null",
            "GIT_OPTIONAL_LOCKS": "0", "GIT_TERMINAL_PROMPT": "0",
            "GIT_NO_LAZY_FETCH": "1", "GIT_ALLOW_PROTOCOL": ""
        ]
        process.standardOutput = output
        process.standardError = errors
        process.standardInput = FileHandle.nullDevice
        try process.run()
        defer {
            if process.isRunning { Darwin.kill(process.processIdentifier, SIGKILL) }
            process.waitUntilExit()
            try? output.fileHandleForReading.close()
            try? errors.fileHandleForReading.close()
        }
        let stop = DispatchTime.now().uptimeNanoseconds + 12_000_000_000
        var buffers = [Data(), Data()]
        var descriptors = [output, errors].map { pollfd(fd: $0.fileHandleForReading.fileDescriptor, events: Int16(POLLIN), revents: 0) }
        var block = [UInt8](repeating: 0, count: 32_768)
        while descriptors.contains(where: { $0.fd >= 0 }) {
            try Task.checkCancellation()
            guard DispatchTime.now().uptimeNanoseconds < stop else { throw GitReadError.timedOut }
            let ready = poll(&descriptors, nfds_t(descriptors.count), 100)
            if ready < 0, errno != EINTR { throw GitReadError.unreadable }
            if ready <= 0 { continue }
            for index in descriptors.indices where descriptors[index].revents != 0 {
                let count = Darwin.read(descriptors[index].fd, &block, block.count)
                if count < 0 {
                    if errno == EINTR { continue }
                    throw GitReadError.unreadable
                }
                if count == 0 {
                    descriptors[index].fd = -1
                } else if index == 0 {
                    guard buffers[0].count + count <= cap else { throw GitReadError.tooLarge }
                    buffers[0].append(contentsOf: block.prefix(count))
                } else if buffers[1].count < 32_768 {
                    buffers[1].append(contentsOf: block.prefix(min(count, 32_768 - buffers[1].count)))
                }
            }
        }
        while process.isRunning {
            try Task.checkCancellation()
            guard DispatchTime.now().uptimeNanoseconds < stop else { throw GitReadError.timedOut }
            Thread.sleep(forTimeInterval: 0.01)
        }
        let diagnostic = String(decoding: buffers[1], as: UTF8.self)
        guard allow.contains(process.terminationStatus) else {
            throw GitCommandFailed(code: process.terminationStatus, diagnostic: diagnostic)
        }
        return GitReply(code: process.terminationStatus, output: buffers[0], diagnostic: diagnostic)
    }
}

private extension String {
    func dropFinalLineFeed() -> String { hasSuffix("\n") ? String(dropLast()) : self }
}

private enum GitPatch {
    private static let maximumLines = 50_000

    static func singleSide(_ text: String, added: Bool) throws -> [GitDiffHunk] {
        guard !text.isEmpty else { return [] }
        let lines = text.dropFinalLineFeed().split(separator: "\n", omittingEmptySubsequences: false)
        guard lines.count <= maximumLines else { throw GitReadError.tooLarge }
        let rows = lines.enumerated().map { offset, text in
            let line = GitDiffHunk.Line(number: offset + 1, text: String(text))
            return GitDiffHunk.Row(id: offset, before: added ? nil : line, after: added ? line : nil, changed: true)
        }
        return [GitDiffHunk(id: 0, rows: rows)]
    }

    static func hunks(in patch: String) throws -> [GitDiffHunk] {
        var hunks: [GitDiffHunk] = []
        var rows: [GitDiffHunk.Row] = []
        var removed: [GitDiffHunk.Line] = []
        var added: [GitDiffHunk.Line] = []
        var old = 0
        var new = 0
        var inHunk = false
        var lineCount = 0

        func flushChanges() {
            for index in 0..<max(removed.count, added.count) {
                rows.append(GitDiffHunk.Row(id: rows.count,
                                            before: index < removed.count ? removed[index] : nil,
                                            after: index < added.count ? added[index] : nil, changed: true))
            }
            removed.removeAll(keepingCapacity: true)
            added.removeAll(keepingCapacity: true)
        }

        for line in patch.split(separator: "\n", omittingEmptySubsequences: false) {
            if line.hasPrefix("@@ ") {
                flushChanges()
                if inHunk { hunks.append(GitDiffHunk(id: hunks.count, rows: rows)) }
                rows = []
                let header = line.split(separator: " ")
                guard header.count >= 4, header[1].hasPrefix("-"), header[2].hasPrefix("+"),
                      let startOld = Int(header[1].dropFirst().prefix(while: { $0 != "," })),
                      let startNew = Int(header[2].dropFirst().prefix(while: { $0 != "," })) else {
                    throw GitReadError.unreadable
                }
                old = startOld
                new = startNew
                inHunk = true
            } else if inHunk, let sign = line.first {
                lineCount += 1
                guard lineCount <= maximumLines else { throw GitReadError.tooLarge }
                let text = String(line.dropFirst())
                switch sign {
                case " ":
                    flushChanges()
                    rows.append(GitDiffHunk.Row(id: rows.count,
                                                before: .init(number: old, text: text),
                                                after: .init(number: new, text: text), changed: false))
                    old += 1
                    new += 1
                case "-":
                    removed.append(.init(number: old, text: text))
                    old += 1
                case "+":
                    added.append(.init(number: new, text: text))
                    new += 1
                case "\\":
                    // Git's missing-final-newline note carries no line number.
                    continue
                default:
                    throw GitReadError.unreadable
                }
            }
        }
        flushChanges()
        if inHunk { hunks.append(GitDiffHunk(id: hunks.count, rows: rows)) }
        return hunks
    }
}
