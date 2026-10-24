import Foundation

/// A file Rich put in front of the user (round 36 screen 3, "A file from Rich"; the Mac's
/// `phone/rows.rs` `rich_file` row). The row carries an id to ask the Mac for, never a path: the
/// bytes come over only when the person taps the file. Android's core `RichFiles.kt`, same names.
public struct RichFile: Codable, Equatable, Sendable {
    public var id: String
    public var name: String
    public var ext: String
    public var type: String
    /// The card's own line, "PDF · 184 KB", as the Mac wrote it.
    public var meta: String
    public var bytes: Int64?
    /// `false` when the file was moved or deleted on the Mac.
    public var exists: Bool

    public init(id: String, name: String, ext: String = "FILE", type: String = "", meta: String = "", bytes: Int64? = nil, exists: Bool = true) {
        self.id = id; self.name = name; self.ext = ext; self.type = type; self.meta = meta; self.bytes = bytes; self.exists = exists
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decode(String.self, forKey: .name)
        ext = try c.decodeIfPresent(String.self, forKey: .ext) ?? "FILE"
        type = try c.decodeIfPresent(String.self, forKey: .type) ?? ""
        meta = try c.decodeIfPresent(String.self, forKey: .meta) ?? ""
        bytes = try c.decodeIfPresent(Int64.self, forKey: .bytes)
        exists = try c.decodeIfPresent(Bool.self, forKey: .exists) ?? true
    }
}

/// Where one of Rich's files stands on this phone. TRANSIENT: after a relaunch a tap looks on disk
/// for the kept copy first (`finding`), so it opens with the Mac offline or the original deleted.
public struct FileFetch: Codable, Equatable, Sendable {
    public enum State: String, Codable, Sendable {
        /// Tapped: the phone looks for its own copy first (a moment, on disk), before the Mac is needed.
        case finding
        /// Coming over from the Mac: the ring fills.
        case coming
        /// Tapped while the Mac was out of reach: the clock, until it is back.
        case waiting
        /// On this phone: a tap opens it at once.
        case kept
        /// The Mac said why it could not send it; `reason` is its sentence.
        case failed
    }
    public var id: String
    public var state: State
    public var received: Int64 = 0
    public var total: Int64?
    public var reason: String?
    /// The kept copy on this phone, once `.kept`.
    public var path: String?
    /// The person tapped it: the viewer opens as soon as it is here.
    public var open = false

    public init(id: String, state: State, received: Int64 = 0, total: Int64? = nil, reason: String? = nil, path: String? = nil, open: Bool = false) {
        self.id = id; self.state = state; self.received = received; self.total = total; self.reason = reason; self.path = path; self.open = open
    }
}

public enum RichFileRule {
    /// The Mac capability that says it sends the files Rich attached.
    public static let capability = "files"
    /// The Mac's own sentence for a file that is gone (`opener.rs` FILE_GONE, the route's 410).
    public static let gone = "No longer on this Mac. It was moved or deleted."
    public static let tooLarge = "This file is too large to send to your phone. Open it on your Mac."
    public static let failed = "Your Mac could not send this file. Tap to try again."
    /// The line while the Mac is out of reach (round 36, "Phone: Mac is offline").
    public static let waitingLine = "Opens when your Mac is back online"

    /// The signed request: the file's id and its conversation, on the message route.
    public static func path(fileID: String, threadID: String) -> String {
        "/api/messages?kind=file&file_id=\(RequestSigning.encodeURIComponent(fileID))&thread_id=\(RequestSigning.encodeURIComponent(threadID))"
    }

    /// "62 of 184 KB from your Mac", the line while the ring fills (round 36).
    public static func progress(received: Int64, total: Int64?) -> String {
        let kb = { (b: Int64) -> Int64 in max(0, Int64((Double(b) / 1000).rounded())) }
        if let total, total > 0 { return "\(kb(received)) of \(kb(total)) KB from your Mac" }
        return "\(kb(received)) KB from your Mac"
    }

    /// How far the ring is filled, 0…1.
    public static func fraction(_ f: FileFetch) -> Double {
        guard let total = f.total, total > 0 else { return 0 }
        return min(1, max(0, Double(f.received) / Double(total)))
    }

    /// Where a file is kept: `<directory>/<id>/<name>`, or nil for a part that would leave its folder.
    public static func keptURL(directory: URL, id: String, name: String) -> URL? {
        func safe(_ s: String) -> Bool { !s.isEmpty && s.count <= 255 && !s.contains("/") && !s.contains("\\") && s != "." && s != ".." }
        guard safe(id), safe(name) else { return nil }
        return directory.appendingPathComponent(id, isDirectory: true).appendingPathComponent(name)
    }
}
