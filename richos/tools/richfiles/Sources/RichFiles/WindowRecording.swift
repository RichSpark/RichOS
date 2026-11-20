import AppKit
import Darwin
import Security

/// Serve pictures of our own active window to RichOS. The socket carries one
/// picture per connection and is never a screen-wide capture source.
final class WindowRecording {
    private let listener: DispatchSourceRead
    private let path: String

    init?() {
        guard let directory = Self.userTempDirectory(),
              let (peerRequirement, requireEnclosing) = Self.richOSRequirement() else { return nil }
        path = directory + "/richfiles-video." + String(getpid())
        guard let descriptor = Self.bind(path) else { return nil }

        let queue = DispatchQueue(label: "app.richos.richfiles.video", qos: .utility)
        listener = DispatchSource.makeReadSource(fileDescriptor: descriptor, queue: queue)
        var acceptedToken: Data?
        var takingPicture = false
        listener.setEventHandler {
            let peer = accept(descriptor, nil, nil)
            guard peer >= 0 else { return }
            guard !takingPicture, let token = Self.auditToken(for: peer),
                  token == acceptedToken || Self.isRichOS(token, requirement: peerRequirement, requireEnclosing: requireEnclosing) else {
                close(peer)
                return
            }
            acceptedToken = token
            takingPicture = true
            _ = fcntl(peer, F_SETFD, FD_CLOEXEC)
            var noSignal: Int32 = 1
            var timeout = timeval(tv_sec: 0, tv_usec: 400_000)
            setsockopt(peer, SOL_SOCKET, SO_NOSIGPIPE, &noSignal, socklen_t(MemoryLayout<Int32>.size))
            setsockopt(peer, SOL_SOCKET, SO_SNDTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))

            DispatchQueue.main.async {
                let capture = Self.picture()
                queue.async {
                    defer { takingPicture = false }
                    let handle = FileHandle(fileDescriptor: peer, closeOnDealloc: true)
                    if let (bitmap, cursor) = capture,
                       let jpeg = bitmap.representation(using: .jpeg, properties: [.compressionFactor: 0.65]),
                       jpeg.count <= 3_500_000 {
                        try? handle.write(contentsOf: cursor)
                        try? handle.write(contentsOf: jpeg)
                    }
                    try? handle.close()
                }
            }
        }
        listener.setCancelHandler { close(descriptor) }
        listener.resume()
    }

    deinit {
        unlink(path)
        listener.cancel()
    }

    private static func userTempDirectory() -> String? {
        let key = Int32(_CS_DARWIN_USER_TEMP_DIR)
        let count = confstr(key, nil, 0)
        guard count > 0 else { return nil }
        var bytes = [CChar](repeating: 0, count: count)
        guard confstr(key, &bytes, count) == count else { return nil }
        let path = String(cString: bytes)
        return path.hasSuffix("/") ? String(path.dropLast()) : path
    }

    private static func bind(_ path: String) -> Int32? {
        var address = sockaddr_un()
        let bytes = Array(path.utf8CString)
        let capacity = MemoryLayout.size(ofValue: address.sun_path)
        guard bytes.count <= capacity else { return nil }
        let size = socklen_t(MemoryLayout<sockaddr_un>.size)
        address.sun_len = UInt8(size)
        address.sun_family = sa_family_t(AF_UNIX)
        withUnsafeMutableBytes(of: &address.sun_path) { $0.copyBytes(from: bytes.map(UInt8.init(bitPattern:))) }

        // The system's per-user temporary directory is private. A pid in the
        // name keeps a terminated instance's socket from masking its successor.
        var prior = stat()
        if lstat(path, &prior) == 0, prior.st_mode & S_IFMT == S_IFSOCK, prior.st_uid == getuid() {
            unlink(path)
        }
        let descriptor = socket(AF_UNIX, SOCK_STREAM, 0)
        guard descriptor >= 0 else { return nil }
        _ = fcntl(descriptor, F_SETFD, FD_CLOEXEC)
        let result = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.bind(descriptor, $0, size) }
        }
        guard result == 0 else { close(descriptor); return nil }
        guard chmod(path, 0o600) == 0, listen(descriptor, 1) == 0 else {
            unlink(path)
            close(descriptor)
            return nil
        }
        return descriptor
    }

    private static func auditToken(for descriptor: Int32) -> Data? {
        var token = audit_token_t()
        var size = socklen_t(MemoryLayout.size(ofValue: token))
        guard getsockopt(descriptor, SOL_LOCAL, LOCAL_PEERTOKEN, &token, &size) == 0,
              size == MemoryLayout.size(ofValue: token) else { return nil }
        return withUnsafeBytes(of: &token) { Data($0) }
    }

    private static func richOSRequirement() -> (SecRequirement, requireEnclosing: Bool)? {
        var me: SecCode?
        guard SecCodeCopySelf([], &me) == errSecSuccess, let me else { return nil }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(me, [], &staticCode) == errSecSuccess, let staticCode else { return nil }
        var metadata: CFDictionary?
        guard SecCodeCopySigningInformation(staticCode, SecCSFlags(rawValue: kSecCSSigningInformation), &metadata) == errSecSuccess else { return nil }
        let info = metadata as? [String: Any]
        var condition = "identifier \"com.richos.app\""
        let team = info?[kSecCodeInfoTeamIdentifier as String] as? String
        if let team {
            guard !team.isEmpty, team.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) }) else { return nil }
            condition += " and anchor apple generic and certificate leaf[subject.OU] = \"\(team)\""
        }
        var requirement: SecRequirement?
        guard SecRequirementCreateWithString(condition as CFString, [], &requirement) == errSecSuccess,
              let requirement else { return nil }
        return (requirement, team == nil)
    }

    private static func isRichOS(_ token: Data, requirement: SecRequirement, requireEnclosing: Bool) -> Bool {
        let attributes = [kSecGuestAttributeAudit as String: token] as CFDictionary
        var peer: SecCode?
        guard SecCodeCopyGuestWithAttributes(nil, attributes, [], &peer) == errSecSuccess, let peer,
              SecCodeCheckValidity(peer, [], requirement) == errSecSuccess else { return false }

        // A development build has no signing team. In that case only the
        // running RichOS executable in this exact enclosing bundle may ask.
        if !requireEnclosing { return true }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(peer, [], &staticCode) == errSecSuccess, let staticCode else { return false }
        let enclosing = Bundle.main.bundleURL.deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().resolvingSymlinksInPath()
        var path: CFURL?
        guard SecCodeCopyPath(staticCode, [], &path) == errSecSuccess, let path else { return false }
        return (path as URL).resolvingSymlinksInPath() == enclosing
    }

    @MainActor
    private static func picture() -> (NSBitmapImageRep, Data)? {
        guard NSApp.isActive, let window = NSApp.keyWindow, window.isVisible, !window.isMiniaturized,
              let view = window.contentView else { return nil }
        let bounds = view.bounds
        guard !bounds.isEmpty else { return nil }
        let scale = min(1, 960 / bounds.width, 720 / bounds.height)
        guard let bitmap = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: max(1, Int(bounds.width * scale)),
            pixelsHigh: max(1, Int(bounds.height * scale)), bitsPerSample: 8, samplesPerPixel: 4,
            hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ) else { return nil }
        bitmap.size = bounds.size
        view.cacheDisplay(in: bounds, to: bitmap)
        let point = view.convert(window.mouseLocationOutsideOfEventStream, from: nil)
        var cursor = [Float.nan, Float.nan]
        if bounds.contains(point) {
            let y = (point.y - bounds.minY) / bounds.height
            cursor = [Float((point.x - bounds.minX) / bounds.width), Float(view.isFlipped ? y : 1 - y)]
        }
        return (bitmap, cursor.withUnsafeBytes { Data($0) })
    }
}
