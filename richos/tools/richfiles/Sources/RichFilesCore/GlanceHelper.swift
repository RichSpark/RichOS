import Carbon
import Darwin
import Foundation

public enum GlanceHelper {
    public static let bundleID = "app.richos.richglance.capture-helper"
    public static let richOSID = "com.richos.app"
    public static let bundlePath = "Contents/Library/LoginItems/RichGlance Capture.app"
    public static let command = "--capture-helper"

    public struct Shortcut: Codable, Equatable, Sendable {
        public var code: String
        public var control: Bool
        public var option: Bool
        public var shift: Bool
        public var command: Bool

        public static let again = Self(code: "KeyA", control: true, option: false, shift: true, command: false)
        public static let copyText = Self(code: "KeyT", control: true, option: false, shift: true, command: false)
        public static let standard = Self(code: "KeyC", control: true, option: false, shift: true, command: false)
        private static let codes: [String: UInt32] = [
            "KeyA": 0, "KeyB": 11, "KeyC": 8, "KeyD": 2, "KeyE": 14, "KeyF": 3,
            "KeyG": 5, "KeyH": 4, "KeyI": 34, "KeyJ": 38, "KeyK": 40, "KeyL": 37,
            "KeyM": 46, "KeyN": 45, "KeyO": 31, "KeyP": 35, "KeyQ": 12, "KeyR": 15,
            "KeyS": 1, "KeyT": 17, "KeyU": 32, "KeyV": 9, "KeyW": 13, "KeyX": 7,
            "KeyY": 16, "KeyZ": 6, "Digit0": 29, "Digit1": 18, "Digit2": 19, "Digit3": 20,
            "Digit4": 21, "Digit5": 23, "Digit6": 22, "Digit7": 26, "Digit8": 28, "Digit9": 25,
        ]

        public var character: String { String(code.suffix(1)) }

        public var label: String {
            (control ? "⌃" : "") + (option ? "⌥" : "") + (shift ? "⇧" : "")
                + (command ? "⌘" : "") + character
        }

        public init?(keyCode: UInt16, control: Bool, option: Bool, shift: Bool, command: Bool) {
            guard let code = Self.codes.first(where: { $0.value == UInt32(keyCode) })?.key else { return nil }
            self.init(code: code, control: control, option: option, shift: shift, command: command)
        }

        private init(code: String, control: Bool, option: Bool, shift: Bool, command: Bool) {
            self.code = code
            self.control = control
            self.option = option
            self.shift = shift
            self.command = command
        }

        public func carbon() throws -> (key: UInt32, modifiers: UInt32) {
            let modifiers = (control ? UInt32(controlKey) : 0) | (option ? UInt32(optionKey) : 0)
                | (shift ? UInt32(shiftKey) : 0) | (command ? UInt32(cmdKey) : 0)
            guard let key = Self.codes[code], modifiers.nonzeroBitCount >= 2 else {
                throw Problem("Use two or more modifiers and a letter or number.")
            }
            if command && shift && !option && ["Digit3", "Digit4", "Digit5", "Digit6"].contains(code) {
                throw Problem("macOS uses that shortcut for screenshots. Choose another combination.")
            }
            return (key, modifiers)
        }
    }

    public struct Preferences: Codable, Equatable, Sendable {
        public var enabled: Bool
        public var shortcut: Shortcut
        public var againShortcut: Shortcut = .again
        public var textShortcut: Shortcut = .copyText
        public private(set) var background: Bool
        public var menuBar: Bool
        fileprivate var backgroundChosen = false

        public static let initial = Self(enabled: false, shortcut: .standard, background: true, menuBar: true)

        private enum CodingKeys: String, CodingKey {
            case enabled, shortcut, againShortcut, textShortcut, background, menuBar, backgroundChosen
        }

        private init(enabled: Bool, shortcut: Shortcut, background: Bool, menuBar: Bool) {
            self.enabled = enabled
            self.shortcut = shortcut
            self.background = background
            self.menuBar = menuBar
        }

        public init(from decoder: Decoder) throws {
            let values = try decoder.container(keyedBy: CodingKeys.self)
            enabled = try values.decode(Bool.self, forKey: .enabled)
            shortcut = try values.decode(Shortcut.self, forKey: .shortcut)
            againShortcut = try values.decodeIfPresent(Shortcut.self, forKey: .againShortcut) ?? .again
            textShortcut = try values.decodeIfPresent(Shortcut.self, forKey: .textShortcut) ?? .copyText
            background = try values.decodeIfPresent(Bool.self, forKey: .background) ?? Self.initial.background
            menuBar = try values.decodeIfPresent(Bool.self, forKey: .menuBar) ?? Self.initial.menuBar
            backgroundChosen = try values.decodeIfPresent(Bool.self, forKey: .backgroundChosen) ?? false
        }

        public subscript(action: CaptureRequest.Action) -> Shortcut {
            get {
                switch action {
                case .captureAgain: againShortcut
                case .copyText: textShortcut
                default: shortcut
                }
            }
            set {
                switch action {
                case .captureAgain: againShortcut = newValue
                case .copyText: textShortcut = newValue
                default: shortcut = newValue
                }
            }
        }

        public func settingBackground(_ enabled: Bool, firstUse: Bool, userChoice: Bool = true) -> Self {
            var next = self
            next.background = enabled
            if userChoice { next.backgroundChosen = true }
            if enabled && firstUse { next.enabled = true }
            return next
        }
    }

    public struct Problem: LocalizedError {
        public let errorDescription: String?
        public init(_ message: String) { errorDescription = message }
    }

    public struct Request: Codable {
        public enum Action: String, Codable { case status, configure, stopSession }
        public var action: Action
        public var preferences: Preferences?
        public var requiredShortcut: CaptureRequest.Action?

        public init(_ action: Action, preferences: Preferences? = nil, requiredShortcut: CaptureRequest.Action? = nil) {
            self.action = action
            self.preferences = preferences
            self.requiredShortcut = requiredShortcut
        }
    }

    public struct Response: Codable {
        public var registered: Bool
        public var session: Bool
        public var error: String?
        public var againRegistered: Bool?
        public var textRegistered: Bool?

        public init(registered: Bool, session: Bool, againRegistered: Bool? = nil, textRegistered: Bool? = nil) {
            self.registered = registered
            self.session = session
            self.againRegistered = againRegistered
            self.textRegistered = textRegistered
        }
    }

    public static var applicationSupport: URL {
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support", isDirectory: true)
    }

    /// Keep the old RichOS preference file so existing chords and explicit Off survive the change.
    public struct Store: Sendable {
        public struct Record: Sendable {
            public let preferences: Preferences
            public let firstUse: Bool
            public let migratedBackground: Bool
        }

        public let directory: URL
        private let preference: URL
        private let diagnostics: URL
        public var endpoint: URL { directory.appendingPathComponent("port") }

        public init(support: URL = GlanceHelper.applicationSupport) {
            directory = support.appendingPathComponent("app.richos.richglance/Helper", isDirectory: true)
            preference = support.appendingPathComponent("com.richos.app/richglance-shortcut.json")
            diagnostics = support.deletingLastPathComponent().appendingPathComponent("Logs/RichOS", isDirectory: true)
        }

        public func read() throws -> Record {
            do {
                let saved = try JSONDecoder().decode(Preferences.self, from: Data(contentsOf: preference))
                guard !saved.background, !saved.backgroundChosen else {
                    return Record(preferences: saved, firstUse: false, migratedBackground: false)
                }
                // Old shortcut saves wrote the default Off. Only the old background switch
                // recorded a login change, so that surviving receipt can identify a legacy opt-out.
                let optedOut = RichFilesDiagnostics.hasCompletedBackgroundChange(in: diagnostics)
                let migrated = saved.settingBackground(!optedOut, firstUse: false, userChoice: optedOut)
                return Record(preferences: migrated, firstUse: false, migratedBackground: true)
            } catch let error as CocoaError where error.code == .fileReadNoSuchFile {
                return Record(preferences: .initial, firstUse: true, migratedBackground: false)
            } catch {
                throw Problem("Could not read the RichGlance shortcut settings.")
            }
        }

        public func write(_ preferences: Preferences) throws {
            let folder = preference.deletingLastPathComponent()
            let pending = folder.appendingPathComponent(".richglance-shortcut-" + UUID().uuidString)
            defer { try? FileManager.default.removeItem(at: pending) }
            do {
                try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
                let fd = open(pending.path, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600)
                guard fd >= 0 else { throw CocoaError(.fileWriteNoPermission) }
                let file = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
                try file.write(contentsOf: JSONEncoder().encode(preferences))
                try file.close()
                guard rename(pending.path, preference.path) == 0 else { throw CocoaError(.fileWriteUnknown) }
            } catch {
                throw Problem("Could not save the RichGlance shortcut settings.")
            }
        }

        public func restore(_ record: Record) throws {
            if record.firstUse { try FileManager.default.removeItem(at: preference) }
            else { try write(record.preferences) }
        }

        public func lock(_ name: String, nonblocking: Bool = false) throws -> FileHandle? {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true,
                                                    attributes: [.posixPermissions: 0o700])
            var info = stat()
            guard lstat(directory.path, &info) == 0, info.st_mode & S_IFMT == S_IFDIR,
                  info.st_uid == getuid(), info.st_mode & 0o077 == 0 else {
                throw Problem("RichGlance settings folder is inaccessible.")
            }
            let fd = open(directory.appendingPathComponent(name).path, O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC, 0o600)
            guard fd >= 0 else { throw Problem("RichGlance settings folder is inaccessible.") }
            let file = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
            guard fstat(fd, &info) == 0, info.st_mode & S_IFMT == S_IFREG,
                  info.st_uid == getuid(), info.st_nlink == 1, info.st_mode & 0o077 == 0 else {
                throw Problem("RichGlance settings folder is inaccessible.")
            }
            if flock(fd, LOCK_EX | (nonblocking ? LOCK_NB : 0)) == 0 { return file }
            if nonblocking && errno == EWOULDBLOCK { return nil }
            throw Problem("RichGlance could not lock its settings.")
        }

        /// The port name lives in the owner-only directory so other users cannot attach to it.
        public func send(_ request: Request) throws -> Response? {
            guard let name = try? String(contentsOf: endpoint, encoding: .utf8),
                  UUID(uuidString: name) != nil,
                  let port = CFMessagePortCreateRemote(nil, name as CFString) else { return nil }
            let data = try JSONEncoder().encode(request)
            var reply: Unmanaged<CFData>?
            let result = CFMessagePortSendRequest(port, 0, data as CFData, 0.25, 1,
                                                  CFRunLoopMode.defaultMode.rawValue, &reply)
            if result == kCFMessagePortIsInvalid || result == kCFMessagePortBecameInvalidError { return nil }
            guard result == kCFMessagePortSuccess, let reply = reply?.takeRetainedValue() else {
                throw Problem("RichGlance Capture did not respond.")
            }
            let response = try JSONDecoder().decode(Response.self, from: reply as Data)
            if let error = response.error { throw Problem(error) }
            return response
        }
    }
}
