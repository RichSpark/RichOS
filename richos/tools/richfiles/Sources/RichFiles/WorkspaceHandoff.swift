import AppKit
import Darwin
import Foundation

@MainActor
enum WorkspaceHandoff {
    enum Target: String, Codable {
        case standalone, docked

        var scheme: String { self == .standalone ? "richfiles-workspace" : "richos-files-workspace" }
        var title: String { self == .standalone ? "Undock" : "Dock in RichOS" }
    }

    private struct Ticket: Codable {
        let id: UUID
        let target: Target
        let expires: Date
        let workspace: Data
    }

    private static let maximumTicketBytes = 2 * 1024 * 1024

    static func ticketID(from url: URL, for target: Target) -> UUID? {
        guard url.scheme == target.scheme, url.host == "receive", url.user == nil, url.password == nil,
              url.port == nil, url.query == nil, url.fragment == nil, url.pathComponents.count == 2 else { return nil }
        return UUID(uuidString: url.lastPathComponent)
    }

    static func hasPendingTicket(_ id: UUID) -> Bool {
        guard let folder = try? ticketFolder(create: false) else { return false }
        return FileManager.default.fileExists(atPath: ticketURL(id, in: folder).path)
    }

    static func send(_ workspace: Data, to target: Target, application: URL) async throws {
        let id = UUID()
        let ticket = Ticket(id: id, target: target, expires: Date().addingTimeInterval(20), workspace: workspace)
        let folder = try ticketFolder(create: true)
        let request = ticketURL(id, in: folder)
        let reply = ackURL(id, in: folder)
        let data = try JSONEncoder().encode(ticket)
        guard data.count <= maximumTicketBytes else { throw HandoffError.invalid }
        try writePrivate(data, to: request)
        defer {
            try? FileManager.default.removeItem(at: request)
            try? FileManager.default.removeItem(at: reply)
        }
        guard let event = URL(string: "\(target.scheme)://receive/\(id.uuidString)") else { throw HandoffError.invalid }
        let configuration = NSWorkspace.OpenConfiguration()
        if target == .standalone {
            configuration.arguments = ["--receive-workspace", id.uuidString]
        }
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            NSWorkspace.shared.open([event], withApplicationAt: application, configuration: configuration) { app, error in
                if let error { continuation.resume(throwing: error) }
                else if app == nil { continuation.resume(throwing: HandoffError.noApp) }
                else { continuation.resume() }
            }
        }
        while ticket.expires > Date() {
            try Task.checkCancellation()
            if FileManager.default.fileExists(atPath: reply.path),
               let contents = try? readPrivate(reply, maximum: 36),
               contents == Data(id.uuidString.utf8) { return }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        throw HandoffError.noReply
    }

    static func accept(_ id: UUID, for target: Target, into workspace: WindowWorkspace) throws {
        let folder = try ticketFolder(create: false)
        let request = ticketURL(id, in: folder)
        let reply = ackURL(id, in: folder)
        let data = try readPrivate(request, maximum: maximumTicketBytes)
        let ticket = try JSONDecoder().decode(Ticket.self, from: data)
        guard ticket.id == id, ticket.target == target, ticket.expires > Date(),
              ticket.expires.timeIntervalSinceNow <= 20,
              !FileManager.default.fileExists(atPath: reply.path) else { throw HandoffError.invalid }
        let restored = target == .docked ? workspace.acceptDockedSession(ticket.workspace)
            : workspace.restoreSession(ticket.workspace)
        guard restored else { throw HandoffError.invalid }
        try writePrivate(Data(id.uuidString.utf8), to: reply)
    }

    private static func ticketFolder(create: Bool) throws -> URL {
        let support = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                                   appropriateFor: nil, create: create)
        let root = support.appendingPathComponent("app.richos.richfiles", isDirectory: true)
        let folder = root.appendingPathComponent("Handoff", isDirectory: true)
        if create {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true,
                                                    attributes: [.posixPermissions: 0o700])
        }
        var status = stat()
        guard lstat(folder.path, &status) == 0, status.st_mode & S_IFMT == S_IFDIR,
              status.st_uid == getuid(), status.st_mode & 0o077 == 0 else { throw HandoffError.invalid }
        return folder
    }

    private static func ticketURL(_ id: UUID, in folder: URL) -> URL {
        folder.appendingPathComponent(id.uuidString + ".json")
    }

    private static func ackURL(_ id: UUID, in folder: URL) -> URL {
        folder.appendingPathComponent(id.uuidString + ".accepted")
    }

    private static func writePrivate(_ data: Data, to url: URL) throws {
        let descriptor = open(url.path, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600)
        guard descriptor >= 0 else { throw HandoffError.invalid }
        let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        do {
            try handle.write(contentsOf: data)
            try handle.close()
        } catch {
            try? handle.close()
            try? FileManager.default.removeItem(at: url)
            throw error
        }
    }

    private static func readPrivate(_ url: URL, maximum: Int) throws -> Data {
        let descriptor = open(url.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard descriptor >= 0 else { throw HandoffError.invalid }
        let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        defer { try? handle.close() }
        var status = stat()
        guard fstat(descriptor, &status) == 0, status.st_mode & S_IFMT == S_IFREG, status.st_uid == getuid(),
              status.st_mode & 0o077 == 0, status.st_size > 0, status.st_size <= maximum,
              let data = try handle.read(upToCount: maximum + 1), data.count == status.st_size else {
            throw HandoffError.invalid
        }
        return data
    }

    enum HandoffError: LocalizedError {
        case invalid, noApp, noReply, busy

        var errorDescription: String? {
            switch self {
            case .invalid: return "RichFiles could not read the workspace to move."
            case .noApp: return "The other app could not open."
            case .noReply: return "The other app did not open this workspace. It is still available here."
            case .busy: return "Finish the current file action before switching apps."
            }
        }
    }
}

@MainActor
public final class RichFilesDockTarget {
    private weak var workspace: WindowWorkspace?
    private weak var actions: BrowserActions?

    public init() {}

    func attach(workspace: WindowWorkspace, actions: BrowserActions) {
        self.workspace = workspace
        self.actions = actions
    }

    public func reveal(_ location: URL) async throws {
        try await attachedWorkspace().activeModel.revealFile(location)
    }

    public func accept(_ id: UUID) async throws {
        let workspace = try await attachedWorkspace()
        guard actions?.busy == false else { throw WorkspaceHandoff.HandoffError.busy }
        try WorkspaceHandoff.accept(id, for: .docked, into: workspace)
    }

    // The webview can become visible before SwiftUI appears.
    private func attachedWorkspace() async throws -> WindowWorkspace {
        for _ in 0..<20 {
            if let workspace { return workspace }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        throw WorkspaceHandoff.HandoffError.noApp
    }
}
