import Darwin
import Foundation

enum RichFilesDiagnostics {
    enum Operation: String, Codable, Sendable { case listing, copy, move, recovery }
    enum Outcome: String, Codable, Sendable {
        case complete, issues, unavailable, interrupted
        case cancelled = "canceled"
    }

    struct Sample: Encodable, Sendable {
        let atMS: Int64
        let id: UUID
        let build: String
        let operation: Operation
        let outcome: Outcome
        let workerMS: Int
        let firstRowsMS: Int?
        let total: Int
        let confirmed: Int
    }

    static let filename = "richfiles-timing.jsonl"
    private static let maximumBytes: off_t = 128 * 1024
    private static let writer = DispatchQueue(label: "app.richos.richfiles.diagnostics", qos: .utility)

    static func record(
        _ operation: Operation, id: UUID, outcome: Outcome, started: TimeInterval,
        total: Int, confirmed: Int, firstRowsMS: Int? = nil
    ) {
        let bundle = Bundle.main
        guard bundle.bundleIdentifier == "app.richos.richfiles" || bundle.bundleIdentifier == "com.richos.app",
              let library = FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask).first else { return }
        let version = bundle.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "unknown"
        let build = String(version.filter { $0.isASCII && ($0.isLetter || $0.isNumber || ".-+".contains($0)) }.prefix(40))
        let sample = Sample(atMS: Int64(Date().timeIntervalSince1970 * 1_000), id: id, build: build,
                            operation: operation, outcome: outcome,
                            workerMS: milliseconds(since: started), firstRowsMS: firstRowsMS,
                            total: total, confirmed: confirmed)
        let folder = library.appendingPathComponent("Logs/RichOS", isDirectory: true)
        writer.async { try? append(sample, in: folder) }
    }

    static func milliseconds(since started: TimeInterval) -> Int {
        max(0, Int((ProcessInfo.processInfo.systemUptime - started) * 1_000))
    }

    static func append(_ sample: Sample, in folder: URL) throws {
        var bytes = try JSONEncoder().encode(sample)
        bytes.append(10)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let descriptor = open(folder.appendingPathComponent(filename).path,
                              O_WRONLY | O_CREAT | O_APPEND | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC, 0o600)
        guard descriptor >= 0 else { throw CocoaError(.fileWriteNoPermission) }
        let file = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        guard flock(descriptor, LOCK_EX | LOCK_NB) == 0 else { throw CocoaError(.fileWriteUnknown) }
        var info = stat()
        guard fstat(descriptor, &info) == 0, info.st_mode & S_IFMT == S_IFREG,
              info.st_uid == getuid(), info.st_mode & 0o077 == 0, info.st_nlink == 1 else {
            throw CocoaError(.fileWriteNoPermission)
        }
        if info.st_size > maximumBytes - off_t(bytes.count), ftruncate(descriptor, 0) != 0 {
            throw CocoaError(.fileWriteUnknown)
        }
        try file.write(contentsOf: bytes)
    }
}
