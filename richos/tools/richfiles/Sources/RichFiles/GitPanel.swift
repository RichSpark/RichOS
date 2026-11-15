import AppKit
import RichFilesCore
import SwiftUI

private struct GitPanelFocusKey: FocusedValueKey {
    typealias Value = Bool
}

extension FocusedValues {
    var gitPanelFocus: Bool? {
        get { self[GitPanelFocusKey.self] }
        set { self[GitPanelFocusKey.self] = newValue }
    }
}

@MainActor
struct GitBrowserSplit<Browser: View>: View {
    @ObservedObject var git: GitPanelModel
    @ViewBuilder let browser: () -> Browser

    var body: some View {
        VSplitView {
            browser().frame(minHeight: git.open ? 180 : 0)
            if git.open {
                RichFilesGitPanel(model: git, dismiss: git.close)
                    .frame(minHeight: 190, idealHeight: 340)
            }
        }
    }
}

/// The same read-only panel is hosted under the RichFiles browser and beside a
/// chat. Only the host supplies the entry point and what closing it returns to.
@MainActor
public struct RichFilesGitPanel: View {
    @ObservedObject private var model: GitPanelModel
    @Environment(\.colorScheme) private var scheme
    @AppStorage("richfiles.textSize") private var textSize = BrowserTextSize.standard
    @FocusState private var scopeFocused: Bool
    @State private var currentHunk = 0
    private let dismiss: () -> Void
    private let handoff: ((GitPanelSnapshot) -> Void)?
    private let refresh: (() -> Void)?

    public init(model: GitPanelModel, dismiss: @escaping () -> Void,
                handoff: ((GitPanelSnapshot) -> Void)? = nil, refresh: (() -> Void)? = nil) {
        self.model = model
        self.dismiss = dismiss
        self.handoff = handoff
        self.refresh = refresh
    }

    private var palette: BrowserPalette { BrowserPalette(scheme) }

    public var body: some View {
        GeometryReader { geometry in
            VStack(spacing: 0) {
                header
                rule
                if geometry.size.width >= 1320 {
                    HStack(spacing: 0) {
                        history.frame(idealWidth: 320, maxWidth: 370)
                        verticalRule
                        if model.commit != nil {
                            detail.frame(idealWidth: 280, maxWidth: 330)
                            verticalRule
                        }
                        if model.selectedFile != nil { comparison }
                        else { Spacer(minLength: 0) }
                    }
                } else if geometry.size.width >= 960 && model.commit != nil && model.selectedFile == nil {
                    HStack(spacing: 0) {
                        history.frame(width: geometry.size.width * 0.44)
                        verticalRule
                        detail
                    }
                } else if model.selectedFile != nil {
                    comparison
                } else if model.commit != nil {
                    detail
                } else {
                    history
                }
            }
            .foregroundStyle(palette.ink)
            .background(palette.pane)
        }
        .font(BrowserTypography.reading(16 * textSize.scale))
        .focusedValue(\.gitPanelFocus, true)
        .onAppear { scopeFocused = true }
        .onExitCommand { if !model.back() { dismiss() } }
        .accessibilityIdentifier("richfiles-git-panel")
        .onChange(of: model.selectedFile?.id) { _ in currentHunk = 0 }
        .onChange(of: model.commit?.id) { _ in currentHunk = 0 }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 6) {
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 8) {
                    repositoryPicker
                    scopeButtons
                    Spacer(minLength: 4)
                    headerActions(compact: false)
                }
                VStack(alignment: .leading, spacing: 5) {
                    HStack(spacing: 6) {
                        repositoryPicker
                        Spacer(minLength: 2)
                        headerActions(compact: true)
                    }
                    scopeButtons
                }
            }
            if let snapshot = model.snapshot, snapshot.threadId != nil {
                Text("Snapshot from \(snapshot.chatName) · \(snapshot.capturedAt.formatted(date: .abbreviated, time: .shortened)). Refresh from the source chat’s Output.")
                    .foregroundStyle(palette.hint)
                    .fixedSize(horizontal: false, vertical: true)
            } else if let repository = model.repository, !model.isOutput {
                Text(repository.root.path).foregroundStyle(palette.hint)
                    .lineLimit(1).truncationMode(.middle).help(repository.root.path)
            }
            if model.scope == .chat && model.isOutput {
                HStack(alignment: .top, spacing: 6) {
                    Image(systemName: "info.circle").foregroundStyle(palette.signal).accessibilityHidden(true)
                    Text("Shows lands RichOS recorded for this chat. For manual or unrecorded work, open All on main.")
                        .foregroundStyle(palette.hint)
                }
            } else if model.scope == .all, let checkout = model.repositoryCheckout, checkout != "main" {
                Text("Showing local main. The current checkout is \(checkout).")
                    .foregroundStyle(palette.hint).lineLimit(1)
            }
            if let commit = model.commit {
                Button {
                    _ = model.back()
                } label: {
                    Label(model.selectedFile == nil ? "Back to history" : commit.subject, systemImage: "chevron.left")
                        .lineLimit(1)
                }
                .buttonStyle(BrowserButtonStyle())
                .accessibilityLabel(model.selectedFile == nil ? "Back to history" : "Back to changed files")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(palette.chrome)
    }

    @ViewBuilder private var repositoryPicker: some View {
        if model.repositories.count > 1 {
            Menu {
                ForEach(model.repositories, id: \.path) { repo in
                    Button {
                        model.chooseRepository(repo.path)
                    } label: {
                        let name = URL(fileURLWithPath: repo.path).lastPathComponent
                        if model.selectedRepository == repo.path { Label(name, systemImage: "checkmark") }
                        else { Text(name) }
                    }
                    .help(repo.path)
                }
            } label: {
                Label(model.repository?.name ?? "Choose Repository", systemImage: "folder").lineLimit(1)
            }
            .accessibilityLabel("Repository")
        } else if let repository = model.repository {
            Label(repository.name, systemImage: "folder").lineLimit(1).help(repository.root.path)
        }
    }

    private var scopeButtons: some View {
        HStack(spacing: 3) {
            if model.isOutput || model.snapshot?.threadId != nil || model.snapshotUnavailable {
                let label = model.isOutput ? "This Chat" : "Chat snapshot"
                Button(label) { model.useScope(.chat) }
                    .buttonStyle(BrowserButtonStyle(selected: model.scope == .chat))
            }
            Button("All on main") { model.useScope(.all) }
                .buttonStyle(BrowserButtonStyle(selected: model.scope == .all))
                .focused($scopeFocused)
        }
        .padding(2)
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(palette.controlLine).allowsHitTesting(false))
    }

    private func headerActions(compact: Bool) -> some View {
        HStack(spacing: 4) {
            button("Refresh Git history", "arrow.clockwise") { if let refresh { refresh() } else { model.refresh() } }
            if let handoff {
                Button {
                    if let snapshot = model.makeSnapshot() { handoff(snapshot) }
                } label: {
                    if compact { Image(systemName: "arrow.up.right.square") }
                    else { Label("Open in RichFiles", systemImage: "arrow.up.right.square") }
                }
                .buttonStyle(BrowserButtonStyle(bordered: true))
                .accessibilityLabel("Open in RichFiles").help("Open in RichFiles")
                .disabled(model.repository == nil)
            }
            button("Close Git", "xmark", action: dismiss)
        }
    }

    private var history: some View {
        ScrollViewReader { scroll in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if model.shallow { message("This shallow repository contains only part of the history.") }
                    if model.scope == .chat && model.offMain > 0 {
                        message("\(model.offMain) recorded \(model.offMain == 1 ? "land has" : "lands have") yet to reach local main.")
                    }
                    if model.aboveLost { message("Showing the current folder’s repository.") }
                    if let reason = model.sectionMessage { message(reason) }
                    else if model.commits.isEmpty && !model.loading {
                        message(model.scope == .chat ? "No recorded lands on local main." : "No local history to show.")
                    }
                    ForEach(model.commits) { commit in
                        historyRow(commit).id(commit.id)
                    }
                    if model.loading { ProgressView().controlSize(.small).padding(12).accessibilityLabel("Loading Git history") }
                    if model.hasMore && !model.loading {
                        Button("Load More History") { model.loadMore() }
                            .buttonStyle(BrowserButtonStyle(bordered: true))
                            .padding(10)
                    }
                }
            }
            .onChange(of: model.commit?.id) { id in
                if let id { scroll.scrollTo(id, anchor: .center) }
            }
        }
        .accessibilityLabel("Git history")
    }

    private func historyRow(_ commit: GitCommit) -> some View {
        Button {
            model.select(commit)
        } label: {
            HStack(alignment: .top, spacing: 6) {
                if model.scope == .all {
                    Text(verbatim: graphGlyphs(commit.graph))
                        .font(.system(size: 16 * textSize.scale, design: .monospaced))
                        .foregroundStyle(palette.folder)
                        .accessibilityHidden(true)
                }
                VStack(alignment: .leading, spacing: 3) {
                    Text(verbatim: commit.subject.isEmpty ? "Untitled Commit" : commit.subject)
                        .lineLimit(1)
                    Text(verbatim: "\(String(commit.id.prefix(8))) · \(commit.author) · \(commit.date.prefix(10))")
                        .foregroundStyle(palette.hint)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
                if model.commit?.id == commit.id { Image(systemName: "checkmark").foregroundStyle(palette.signal) }
            }
            .padding(.horizontal, 10).padding(.vertical, 7)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background(model.commit?.id == commit.id ? palette.selected : Color.clear)
        .accessibilityAddTraits(model.commit?.id == commit.id ? [.isSelected] : [])
    }

    private var detail: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if let commit = model.commit {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(verbatim: commit.subject.isEmpty ? "Untitled Commit" : commit.subject)
                            .font(BrowserTypography.heading(23 * textSize.scale))
                        Text(verbatim: "\(commit.author) · \(commit.date.prefix(10))")
                        Text(verbatim: commit.id).textSelection(.enabled)
                        if model.scope == .chat {
                            Button("Show in All on main") { model.useScope(.all, reveal: true) }
                                .buttonStyle(BrowserButtonStyle(bordered: true))
                        }
                    }
                    .padding(12)
                }
                if let detail = model.comparison {
                    rule
                    message("\(detail.files.count) changed \(detail.files.count == 1 ? "file" : "files") · \(detail.basis.rawValue)")
                    if detail.incompleteRenames { message("Rename matching is incomplete for this comparison.") }
                    if detail.basis == .unavailable { message("A verified earlier version is unavailable for this land.") }
                    ForEach(grouped(detail.files), id: \.key) { group in
                        Text(verbatim: group.key).fontWeight(.semibold).padding(.horizontal, 12).padding(.vertical, 5)
                        ForEach(group.value) { file in
                            Button {
                                model.selectFile(file)
                            } label: {
                                HStack {
                                    Image(systemName: statusIcon(file.status)).accessibilityHidden(true)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(verbatim: URL(fileURLWithPath: file.path).lastPathComponent)
                                        Text(file.status.rawValue).foregroundStyle(palette.hint)
                                        if let oldPath = file.oldPath {
                                            Text(verbatim: oldPath + " → " + file.path).foregroundStyle(palette.hint)
                                        }
                                    }
                                    Spacer(minLength: 0)
                                }
                                .padding(.horizontal, 12).padding(.vertical, 6)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .background(model.selectedFile?.id == file.id ? palette.selected : Color.clear)
                            .accessibilityAddTraits(model.selectedFile?.id == file.id ? [.isSelected] : [])
                        }
                    }
                } else if let reason = model.detailMessage {
                    message(reason)
                } else {
                    ProgressView().controlSize(.small).padding(12).accessibilityLabel("Loading changed files")
                }
            }
        }
        .accessibilityLabel("Changed files")
    }

    private var comparison: some View {
        VStack(spacing: 0) {
            if let file = model.selectedFile {
                comparisonHeader(file)
                rule
            }
            if model.fileLoading {
                ProgressView().controlSize(.small).padding(12).accessibilityLabel("Loading comparison")
                Spacer(minLength: 0)
            } else if let reason = model.fileMessage {
                message(reason)
                Spacer(minLength: 0)
            } else if !model.images.isEmpty {
                HStack(alignment: .top) {
                    ForEach(model.images.indices, id: \.self) { index in
                        if let image = model.images[index] {
                            Image(nsImage: image).resizable().scaledToFit().padding(8)
                                .accessibilityLabel(index == 0 ? "Earlier image" : "Later image")
                        } else {
                            Color.clear.accessibilityLabel("No image in this version")
                        }
                    }
                }
                Spacer(minLength: 0)
            } else if model.hunks.isEmpty {
                message("No text changes to display.")
                Spacer(minLength: 0)
            } else {
                diffRows
            }
            if !model.fileLoading {
                rule
                editorFooter
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityLabel("Read-only comparison")
    }

    private func comparisonHeader(_ file: GitChangedFile) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(verbatim: file.path).fontWeight(.semibold).lineLimit(1)
                    .truncationMode(.middle).help(file.path)
                Spacer(minLength: 0)
                Text(file.status.rawValue).foregroundStyle(palette.hint).lineLimit(1)
                if model.alternateLands.count > 1 {
                    Menu("Recorded comparisons") {
                        ForEach(model.alternateLands) { land in
                            Button("\(land.date.prefix(10)) · \(land.subject)") { model.select(land, path: file.path) }
                        }
                    }
                }
            }
            HStack {
                versionLabel(before: true)
                versionLabel(before: false)
            }
        }
        .padding(10)
        .background(palette.chrome)
    }

    private var editorFooter: some View {
        HStack(alignment: .center, spacing: 8) {
            if let live = model.live {
                editorButton
                editorDescription(live)
            } else {
                Text("The current file is unavailable.").foregroundStyle(palette.hint)
            }
            Spacer(minLength: 0)
        }
        .padding(8)
        .background(palette.chrome)
    }

    private func editorDescription(_ file: FileReference) -> some View {
        Text(verbatim: "Opens \(file.url.lastPathComponent) in \(model.editorName ?? "the preferred editor"), on \(model.checkout ?? "the current checkout").")
            .lineLimit(2).truncationMode(.middle).help(file.url.path)
            .foregroundStyle(palette.hint)
    }

    private var editorButton: some View {
        Button("Edit Current File") { model.openCurrent() }
            .buttonStyle(BrowserButtonStyle(bordered: true))
            .fixedSize(horizontal: true, vertical: false)
    }

    private func versionLabel(before: Bool) -> some View {
        let compare = model.comparison
        let title = before
            ? (compare?.basis == .land ? "Before this land" : compare?.basis.rawValue ?? "Earlier version")
            : (compare?.basis == .land ? "After this land" : "Selected version")
        let id = before ? compare?.before : compare?.after
        return Text(verbatim: title + (id.map { " · " + String($0.prefix(8)) } ?? ""))
            .frame(maxWidth: .infinity, alignment: .leading)
            .lineLimit(1)
            .help(id ?? title)
    }

    private var diffRows: some View {
        ScrollViewReader { scroll in
            HStack {
                Button("Previous Change") { moveHunk(-1, scroll: scroll) }
                    .buttonStyle(BrowserButtonStyle())
                    .disabled(currentHunk == 0)
                    .keyboardShortcut(KeyEquivalent(Character(UnicodeScalar(0xF70A)!)), modifiers: [.shift])
                Button("Next Change") { moveHunk(1, scroll: scroll) }
                    .buttonStyle(BrowserButtonStyle())
                    .disabled(currentHunk + 1 >= model.hunks.count)
                    .keyboardShortcut(KeyEquivalent(Character(UnicodeScalar(0xF70A)!)), modifiers: [])
                Spacer(minLength: 0)
                Text("Change \(currentHunk + 1) of \(model.hunks.count)").foregroundStyle(palette.hint)
            }
            .padding(.horizontal, 8).padding(.vertical, 3)
            rule
            GeometryReader { size in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(model.hunks) { hunk in
                            Text("Change \(hunk.id + 1)")
                                .foregroundStyle(palette.hint)
                                .padding(5)
                                .id(hunk.id)
                            ForEach(hunk.rows) { row in
                                HStack(alignment: .top, spacing: 0) {
                                    sourceLine(row.before, changed: row.changed, before: true, width: (size.size.width - 1) / 2)
                                    verticalRule
                                    sourceLine(row.after, changed: row.changed, before: false, width: (size.size.width - 1) / 2)
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    private func sourceLine(_ line: GitDiffHunk.Line?, changed: Bool, before: Bool, width: CGFloat) -> some View {
        let color = before ? palette.danger : palette.signal
        return HStack(alignment: .top, spacing: 8) {
            Text(line?.number.map(String.init) ?? "")
                .foregroundStyle(palette.hint)
                .frame(minWidth: 36, alignment: .trailing)
            Text(changed && line != nil ? (before ? "−" : "+") : " ")
                .foregroundStyle(color)
                .accessibilityHidden(true)
            Text(verbatim: line?.text ?? "")
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        }
        .font(.system(size: 16 * textSize.scale, design: .monospaced))
        .padding(.horizontal, 5).padding(.vertical, 2)
        .frame(width: max(1, width), alignment: .leading)
        .background(changed && line != nil ? color.opacity(0.13) : Color.clear)
        .accessibilityLabel(before ? "Earlier version" : "Later version")
        .accessibilityValue(line.map {
            let number = $0.number.map { "Line \($0). " } ?? ""
            let status = changed ? (before ? "Removed. " : "Added. ") : ""
            return number + status + $0.text
        } ?? "Blank")
    }

    private func grouped(_ files: [GitChangedFile]) -> [(key: String, value: [GitChangedFile])] {
        Dictionary(grouping: files) { file in
            file.path.lastIndex(of: "/").map { String(file.path[..<$0]) } ?? "Repository root"
        }.sorted { $0.key.localizedStandardCompare($1.key) == .orderedAscending }
    }

    private func statusIcon(_ status: GitChangedFile.Status) -> String {
        switch status {
        case .added: return "plus"
        case .modified: return "pencil"
        case .renamed: return "arrow.right"
        case .deleted: return "minus"
        case .unresolved: return "questionmark.circle"
        }
    }

    private func graphGlyphs(_ rows: [String]) -> String {
        let glyphs: [Character: Character] = ["*": "●", "|": "│", "/": "╱", "\\": "╲", "_": "─", "-": "─"]
        return rows.map { row in String(row.map { glyphs[$0] ?? $0 }) }.joined(separator: "\n")
    }

    private func moveHunk(_ amount: Int, scroll: ScrollViewProxy) {
        let next = currentHunk + amount
        guard model.hunks.indices.contains(next) else { return }
        currentHunk = next
        scroll.scrollTo(model.hunks[next].id, anchor: .top)
    }

    private func button(_ label: String, _ symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) { Image(systemName: symbol) }
            .buttonStyle(BrowserButtonStyle())
            .accessibilityLabel(label).help(label)
    }

    private func message(_ text: String) -> some View {
        Text(text).foregroundStyle(palette.hint).fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading).padding(12)
    }

    private var rule: some View { Rectangle().fill(palette.line).frame(height: 1) }
    private var verticalRule: some View { Rectangle().fill(palette.line).frame(width: 1) }
}
