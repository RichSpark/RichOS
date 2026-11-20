import AppKit
import Darwin
import RichFilesCore
import SwiftUI
import UniformTypeIdentifiers

@MainActor
final class GlanceDefaults: ObservableObject {
    static let shared = GlanceDefaults()
    private nonisolated static let viewerID = "app.richos.richglance"

    struct Handler: Codable, Equatable {
        let url: URL
        let identifier: String?
        let name: String

        init(_ url: URL) {
            self.url = url.resolvingSymlinksInPath()
            identifier = Bundle(url: url)?.bundleIdentifier
            name = Bundle(url: url)?.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String
                ?? FileManager.default.displayName(atPath: url.path)
        }

        var isGlance: Bool { identifier == GlanceDefaults.viewerID }

        func installedURL(in workspace: NSWorkspace) -> URL? {
            if FileManager.default.fileExists(atPath: url.path),
               Bundle(url: url)?.bundleIdentifier == identifier {
                return url
            }
            guard let identifier, let relocated = workspace.urlForApplication(withBundleIdentifier: identifier),
                  Bundle(url: relocated)?.bundleIdentifier == identifier else { return nil }
            return relocated
        }
    }

    enum Ownership: String, Codable {
        case requested, confirmed, relinquished
    }

    struct Change: Codable {
        var previous: Handler?
        var ownership: Ownership
    }

    struct State: Codable {
        var enabled = false
        var changes: [String: Change] = [:]

        mutating func reconcile(_ actual: [GlanceFormat: Handler]) {
            for format in GlanceFormat.allCases {
                guard var change = changes[format.id], let handler = actual[format] else { continue }
                if handler.isGlance, change.ownership == .requested {
                    change.ownership = .confirmed
                } else if !handler.isGlance, change.ownership == .requested {
                    changes[format.id] = nil
                    continue
                } else if !handler.isGlance, change.ownership == .confirmed {
                    change.ownership = .relinquished
                }
                changes[format.id] = change
            }
        }
    }

    @Published private(set) var state = State()
    @Published private(set) var current: [GlanceFormat: Handler] = [:]
    @Published private(set) var messages: [GlanceFormat: String] = [:]
    @Published private(set) var busy = false
    @Published private(set) var problem: String?
    private let workspace = NSWorkspace.shared
    private let fileManager = FileManager.default

    var viewer: URL? { RichFilesInstallation.glanceApplication }

    private var directory: URL {
        fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("app.richos.richfiles", isDirectory: true)
    }

    private var stateFile: URL { directory.appendingPathComponent("RichGlanceDefaults.json") }

    func owns(_ format: GlanceFormat) -> Bool {
        state.changes[format.id]?.ownership == .confirmed && current[format]?.isGlance == true
    }

    func canRestore(_ format: GlanceFormat) -> Bool {
        state.changes[format.id]?.previous?.installedURL(in: workspace) != nil
    }

    func refresh() {
        guard !busy else { return }
        readCurrent()
        do {
            var saved = try readState()
            if let lock = try? lockState() {
                defer { Darwin.close(lock) }
                saved = try readState()
                let before = saved.changes.mapValues(\.ownership)
                saved.reconcile(current)
                if saved.changes.mapValues(\.ownership) != before { try writeState(saved) }
            }
            state = saved
            problem = nil
        } catch {
            problem = "RichFiles could not read your previous image viewer choices."
        }
    }

    func setEnabled(_ enabled: Bool, only: GlanceFormat? = nil, completion: ((String?) -> Void)? = nil) {
        let formats = only.map { [$0] } ?? GlanceFormat.allCases
        changeDefaults(completion: completion) { saved in
            saved.enabled = enabled
            try self.writeState(saved)
            for format in formats {
                if enabled, let viewer = self.viewer, format.associationProblem == nil {
                    try await self.apply(format, destination: viewer, enabling: true, saved: &saved)
                } else if !enabled, self.owns(format) {
                    guard let prior = saved.changes[format.id]?.previous?.installedURL(in: self.workspace) else {
                        self.messages[format] = "The previous app is unavailable."
                        continue
                    }
                    try await self.apply(format, destination: prior, enabling: false, saved: &saved)
                }
            }
        }
    }

    func usePreview(_ format: GlanceFormat) {
        guard let preview = workspace.urlForApplication(withBundleIdentifier: "com.apple.Preview") else {
            problem = "Preview is unavailable."
            return
        }
        changeDefaults { saved in try await self.apply(format, destination: preview, enabling: false, saved: &saved) }
    }

    func revealImageInFinder(_ format: GlanceFormat) {
        guard let type = format.contentType else { return }
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [type]
        panel.message = "Choose a \(format.title) image. In Finder, open Get Info to change its default app."
        panel.prompt = "Show in Finder"
        panel.begin { response in
            if response == .OK, let url = panel.url {
                NSWorkspace.shared.activateFileViewerSelecting([url])
            }
        }
    }

    private func changeDefaults(completion: ((String?) -> Void)? = nil,
                                _ operation: @escaping (inout State) async throws -> Void) {
        guard !busy else {
            completion?("RichFiles is already updating its image viewer choices.")
            return
        }
        busy = true
        problem = nil
        Task {
            defer {
                busy = false
                completion?(problem)
            }
            let start = ProcessInfo.processInfo.systemUptime
            let request = UUID()
            var outcome = RichFilesDiagnostics.Outcome.issues
            defer {
                RichFilesDiagnostics.record(.glanceDefaults, id: request, outcome: outcome, started: start,
                                            total: 1, confirmed: outcome == .complete ? 1 : 0)
            }
            do {
                let lock = try lockState()
                defer { Darwin.close(lock) }
                var saved = try readState()
                readCurrent()
                saved.reconcile(current)
                state = saved
                try await operation(&saved)
                readCurrent()
                saved.reconcile(current)
                try writeState(saved)
                state = saved
                let incomplete = saved.enabled
                    ? GlanceFormat.allCases.contains { current[$0]?.isGlance != true }
                    : GlanceFormat.allCases.contains(where: owns)
                outcome = incomplete ? .issues : .complete
                if incomplete, completion != nil {
                    problem = "Some picture formats kept their previous app. Check the image viewer choices in RichFiles Settings."
                }
            } catch {
                readCurrent()
                if let saved = try? readState() { state = saved }
                problem = "RichFiles could not finish saving your image viewer choices. Close any other RichFiles settings and try again."
            }
        }
    }

    private func apply(_ format: GlanceFormat, destination: URL, enabling: Bool, saved: inout State) async throws {
        guard let type = format.contentType else { return }
        let prior = workspace.urlForApplication(toOpen: type).map(Handler.init)
        current[format] = prior
        saved.reconcile(current)
        if enabling {
            guard prior?.isGlance != true else { return }
            saved.changes[format.id] = Change(previous: prior, ownership: .requested)
        } else {
            guard saved.changes[format.id]?.ownership == .confirmed, prior?.isGlance == true else {
                try writeState(saved)
                state = saved
                return
            }
        }
        try writeState(saved)
        state = saved
        let target = Handler(destination)
        var failure: String?
        do {
            try await workspace.setDefaultApplication(at: destination, toOpen: type)
        } catch {
            failure = "macOS did not change this association."
        }
        let actual = workspace.urlForApplication(toOpen: type).map(Handler.init)
        current[format] = actual
        if enabling, actual?.isGlance == true {
            saved.changes[format.id]?.ownership = .confirmed
            messages[format] = nil
        } else if !enabling, let actual,
                  actual.url == target.url || (target.identifier != nil && target.identifier == actual.identifier) {
            saved.changes[format.id] = nil
            messages[format] = "Back to \(actual.name)."
        } else if enabling, let actual {
            saved.changes[format.id] = nil
            messages[format] = actual == prior ? "You kept \(actual.name)." : "Now opens in \(actual.name)."
        } else {
            saved.reconcile(current)
            messages[format] = failure ?? "macOS could not confirm the change."
        }
        try writeState(saved)
        state = saved
    }

    private func readCurrent() {
        let observed: [GlanceFormat: Handler] = Dictionary(uniqueKeysWithValues: GlanceFormat.allCases.compactMap { format in
            guard let type = format.contentType, let url = workspace.urlForApplication(toOpen: type) else { return nil }
            return (format, Handler(url))
        })
        messages = messages.filter { current[$0.key] == observed[$0.key] }
        current = observed
    }

    private func readState() throws -> State {
        do {
            return try JSONDecoder().decode(State.self, from: Data(contentsOf: stateFile))
        } catch let error as CocoaError where error.code == .fileReadNoSuchFile {
            return State()
        }
    }

    private func writeState(_ saved: State) throws {
        try JSONEncoder().encode(saved).write(to: stateFile, options: .atomic)
    }

    private func lockState() throws -> Int32 {
        try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent("RichGlanceDefaults.lock")
        let descriptor = open(url.path, O_CREAT | O_RDWR | O_CLOEXEC | O_NOFOLLOW, 0o600)
        guard descriptor >= 0 else { throw CocoaError(.fileWriteNoPermission) }
        guard flock(descriptor, LOCK_EX | LOCK_NB) == 0 else {
            Darwin.close(descriptor)
            throw CocoaError(.fileLocking)
        }
        return descriptor
    }
}

@MainActor
struct GlanceDefaultPage: View {
    @ObservedObject private var defaults = GlanceDefaults.shared
    @Environment(\.colorScheme) private var scheme

    private var missing: [GlanceFormat] {
        GlanceFormat.allCases.filter { defaults.current[$0]?.isGlance != true && $0.associationProblem == nil }
    }

    private var owned: [GlanceFormat] {
        GlanceFormat.allCases.filter(defaults.owns)
    }

    var body: some View {
        let palette = BrowserPalette(scheme)
        VStack(alignment: .leading, spacing: 12) {
            Text("Default image viewer")
                .fontWeight(.semibold).foregroundStyle(palette.signal)
                .accessibilityAddTraits(.isHeader)
            HStack(alignment: .top, spacing: 12) {
                if let viewer = defaults.viewer {
                    Image(nsImage: NSWorkspace.shared.icon(forFile: viewer.path))
                        .resizable().frame(width: 44, height: 44).accessibilityHidden(true)
                }
                VStack(alignment: .leading, spacing: 5) {
                    Toggle("Use RichGlance as my default image viewer", isOn: Binding(
                        get: { defaults.state.enabled }, set: { defaults.setEnabled($0) }))
                        .toggleStyle(.switch).fontWeight(.semibold)
                        .accessibilityLabel("Use RichGlance as my default image viewer")
                        .disabled(defaults.busy || (defaults.viewer == nil && !defaults.state.enabled))
                    Text("Double-click a supported image in Finder, on the Desktop or in RichFiles to open it in RichGlance.")
                        .foregroundStyle(palette.hint)
                }
            }
            if defaults.busy {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("Checking image viewer choices with macOS…")
                }.foregroundStyle(palette.ink)
            } else if let problem = defaults.problem {
                Label(problem, systemImage: "exclamationmark.circle").foregroundStyle(palette.ink)
            } else if defaults.viewer == nil {
                Text("RichGlance is unavailable.").foregroundStyle(palette.hint)
            } else if defaults.state.enabled, !missing.isEmpty {
                let active = GlanceFormat.allCases.filter { defaults.current[$0]?.isGlance == true }
                VStack(alignment: .leading, spacing: 8) {
                    Text(active.isEmpty ? "No formats switched to RichGlance." : "RichGlance opens \(active.count) of \(active.count + missing.count) formats.")
                        .fontWeight(.semibold)
                    HStack {
                        Button("Use RichGlance for Remaining Formats") { defaults.setEnabled(true) }
                        if !owned.isEmpty {
                            Button("Undo RichFiles Changes") { defaults.setEnabled(false) }
                        }
                    }.buttonStyle(BrowserButtonStyle(bordered: true))
                }
                .padding(10).frame(maxWidth: .infinity, alignment: .leading)
                .overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(palette.controlLine))
            } else if !defaults.state.enabled, !owned.isEmpty {
                Label("Some formats still open in RichGlance. Review those formats below.",
                      systemImage: "exclamationmark.circle")
            }
            VStack(alignment: .leading, spacing: 0) {
                ForEach(GlanceFormat.allCases) { format in row(format, palette: palette) }
            }
            if !defaults.busy, defaults.problem == nil {
                Text("Current apps checked with macOS. Other file types keep their own defaults.")
                    .foregroundStyle(palette.hint)
            }
            Text("Open With opens a single image without changing its default. RichGlance can also reopen the image in Preview.")
                .foregroundStyle(palette.hint)
        }
        .foregroundStyle(palette.ink)
        .onAppear { defaults.refresh() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            defaults.refresh()
        }
    }

    private func row(_ format: GlanceFormat, palette: BrowserPalette) -> some View {
        let handler = defaults.current[format]
        let previous = defaults.state.changes[format.id]?.previous
        return VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: "photo").foregroundStyle(palette.signal).accessibilityHidden(true)
                Text(format.title).fontWeight(.semibold)
                Text(format.extensions.map { ".\($0)" }.joined(separator: ", ")).foregroundStyle(palette.hint)
                Spacer(minLength: 6)
                Text(handler?.name ?? "No default app").fontWeight(.medium)
            }
            if let issue = format.associationProblem {
                Text(issue).foregroundStyle(palette.hint)
            } else if defaults.owns(format) {
                if defaults.state.enabled {
                    Text(previous.map { "Set by RichFiles. Turning it off brings back \($0.name)." } ?? "Set by RichFiles.")
                        .foregroundStyle(palette.hint)
                } else {
                    Text(defaults.canRestore(format)
                         ? defaults.messages[format] ?? "RichFiles could not restore the previous app."
                         : previous.map { "\($0.name) is no longer on this Mac." } ?? "There was no previous default app.")
                        .foregroundStyle(palette.hint)
                    ViewThatFits(in: .horizontal) {
                        HStack { restoreButtons(format) }
                        VStack(alignment: .leading) { restoreButtons(format) }
                    }.buttonStyle(BrowserButtonStyle(bordered: true)).disabled(defaults.busy)
                }
            } else if defaults.state.changes[format.id]?.ownership == .relinquished {
                Text(handler.map { "Changed outside RichFiles to \($0.name)." } ?? "Changed outside RichFiles.")
                    .foregroundStyle(palette.hint)
            } else {
                Text(defaults.messages[format] ?? "Your Mac’s current choice.").foregroundStyle(palette.hint)
            }
            if defaults.state.enabled, defaults.viewer != nil, !defaults.busy, missing.contains(format) {
                Button("Use RichGlance for \(format.title)") { defaults.setEnabled(true, only: format) }
                    .buttonStyle(BrowserButtonStyle(bordered: true))
            }
        }
        .padding(.vertical, 9).padding(.horizontal, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .bottom) { Rectangle().fill(palette.line).frame(height: 1) }
    }

    @ViewBuilder
    private func restoreButtons(_ format: GlanceFormat) -> some View {
        if defaults.canRestore(format) {
            Button("Restore Previous App") { defaults.setEnabled(false, only: format) }
        } else {
            Button("Use Preview for \(format.title)") { defaults.usePreview(format) }
        }
        Button("Choose Another App in Finder…") { defaults.revealImageInFinder(format) }
    }
}
