import AVKit
import Combine
import QuickLook
import RichOSCore
import SwiftUI

/// The files Rich attached, under his words in his bubble (round 36 screen 3, "Phone: in the chat";
/// round 12.1's file style): the page with its type, the name, and one line. Tap one: it comes over
/// from the Mac (the gold ring fills, the line counts the kilobytes, × stops it), then opens in the
/// phone's viewer with Share. A second tap opens it at once. With the Mac out of reach the ring is a
/// clock and the line says "Opens when your Mac is back online". Android's `RichFileRows`.
struct RichFileList: View {
    let files: [ScreenModel.RichFileRow]
    let send: (Intent) -> Void

    var body: some View {
        VStack(spacing: 6) {
            ForEach(files, id: \.file.id) { row in
                if let moments = row.file.moments {
                    RichMediaCard(row: row, moments: moments, send: send)
                } else {
                    RichFileRowView(row: row, send: send)
                }
            }
        }
        .padding(.top, 4)
    }
}

private struct RichFileRowView: View {
    let row: ScreenModel.RichFileRow
    let send: (Intent) -> Void
    @Environment(\.palette) private var palette

    private var line: String {
        switch row.fetch?.state {
        case .coming?: return RichFileRule.progress(received: row.fetch?.received ?? 0, total: row.fetch?.total ?? row.file.bytes)
        case .waiting?: return RichFileRule.waitingLine
        case .failed?: return row.fetch?.reason ?? RichFileRule.failed
        default: return row.file.exists ? row.file.meta : RichFileRule.gone
        }
    }

    var body: some View {
        Button {
            send(row.fetch?.state == .coming ? .stopRichFile(id: row.file.id) : .openRichFile(id: row.file.id))
        } label: {
            HStack(spacing: 10) {
                ZStack {
                    FileGlyph(ext: row.file.ext)
                    switch row.fetch?.state {
                    case .coming?: disc(icon: .close, progress: RichFileRule.fraction(row.fetch!))
                    case .waiting?: disc(icon: .clock, progress: nil)
                    default: EmptyView()
                    }
                }
                .frame(width: 48, height: 52)
                VStack(alignment: .leading, spacing: 2) {
                    Text(row.file.name).type(Typography.body.weight(600).lineHeight(1.25)).foregroundStyle(palette.ink)
                        .lineLimit(2).truncationMode(.middle)
                    Text(line).type(Typography.read).foregroundStyle(palette.inkSoft).monospacedDigit()
                }
                .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
            }
            .padding(.vertical, 6).padding(.leading, 6).padding(.trailing, 12)
            .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(palette.ground))
            .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(palette.lineFaint, lineWidth: 1))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(row.file.name), \(line)")
        .accessibilityHint(row.fetch?.state == .coming ? "Stops it" : "Opens it")
        .accessibilityIdentifier("richfile.\(row.file.id)")
    }

    private func disc(icon: Icon, progress: Double?) -> some View {
        ZStack {
            Circle().fill(palette.surface)
            Circle().stroke(palette.lineFaint, lineWidth: 1)
            if let progress {
                Circle().stroke(palette.lineFaint, lineWidth: 3).padding(3)
                Circle().trim(from: 0, to: progress)
                    .stroke(palette.signal, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                    .padding(3)
                    .animation(.linear(duration: 0.15), value: progress)
            }
            IconView(icon, size: 16, weight: 2.4).foregroundStyle(palette.ink)
        }
        .frame(width: 40, height: 40)
    }
}

private struct RichMediaCard: View {
    let row: ScreenModel.RichFileRow
    let moments: MediaMoments
    let send: (Intent) -> Void
    @Environment(\.palette) private var palette

    private var status: String? {
        if !row.file.exists { return "The recording was moved, deleted or changed." }
        switch row.fetch?.state {
        case .coming?, .finding?: return "Coming over from your Mac"
        case .waiting?: return RichFileRule.waitingLine
        case .failed?: return row.fetch?.reason ?? RichFileRule.failed
        default: return nil
        }
    }

    var body: some View {
        let file = row.file
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: file.youtubeId == nil ? "waveform" : "play.rectangle")
                    .foregroundStyle(palette.ink)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 2) {
                    Text(file.name).type(Typography.body.weight(600)).foregroundStyle(palette.ink)
                    Text([file.youtubeId != nil ? "YouTube" : file.mediaType == "video" ? "Video" : "Audio",
                          MediaMoments.time(moments.durationMs), file.youtubeId == nil ? "on your Mac" : nil]
                        .compactMap { $0 }.joined(separator: " · "))
                        .type(Typography.body).foregroundStyle(palette.inkSoft)
                }
            }
            if let status {
                HStack(spacing: 8) {
                    if let fetch = row.fetch, fetch.state == .coming {
                        ProgressView(value: RichFileRule.fraction(fetch))
                            .progressViewStyle(.circular).tint(palette.signal)
                            .frame(width: 24, height: 24).accessibilityHidden(true)
                    }
                    Text(status).type(Typography.body).foregroundStyle(palette.inkSoft)
                }
            }
            Canvas { context, size in
                let levels = moments.waveform?.compactMap(\.wholeNumberValue) ?? []
                if levels.isEmpty {
                    context.fill(Path(roundedRect: CGRect(x: 0, y: size.height / 2 - 1, width: size.width, height: 2), cornerRadius: 1), with: .color(palette.inkSoft))
                } else {
                    let step = size.width / CGFloat(levels.count)
                    for (index, level) in levels.enumerated() {
                        let height = max(2, size.height * CGFloat(level) / 10)
                        let bar = CGRect(x: step * CGFloat(index), y: (size.height - height) / 2, width: max(1, step - 1), height: height)
                        context.fill(Path(roundedRect: bar, cornerRadius: 1), with: .color(palette.inkSoft))
                    }
                }
                for point in moments.points {
                    let x = size.width * CGFloat(point.startMs) / CGFloat(max(1, moments.durationMs))
                    context.fill(Path(roundedRect: CGRect(x: x - 2, y: 3, width: 4, height: size.height - 6), cornerRadius: 2), with: .color(palette.signal))
                }
            }
            .frame(height: 32)
            .opacity(file.exists ? 1 : 0.45)
            .accessibilityHidden(true)
            ForEach(moments.points, id: \.startMs) { point in
                if !file.exists {
                    line(point)
                } else if let youtube = file.youtubeId, let url = MediaMoments.youtube(youtube, at: point.startMs) {
                    Link(destination: url) { line(point) }.buttonStyle(.plain)
                } else if file.youtubeId == nil {
                    Button {
                        send(.openRichFile(id: file.id, momentMs: point.startMs))
                    } label: {
                        line(point)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("media.\(file.id).\(point.startMs)")
                }
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 12).fill(palette.ground))
        .overlay(RoundedRectangle(cornerRadius: 12).stroke(palette.lineFaint, lineWidth: 1))
    }

    private func line(_ point: MediaMoments.Point) -> some View {
        HStack(alignment: .top, spacing: 8) {
            HStack(spacing: 4) {
                if row.file.exists {
                    Image(systemName: row.file.youtubeId != nil ? "arrow.up.right" : "play.fill")
                        .font(.system(size: 11, weight: .semibold)).accessibilityHidden(true)
                }
                Text(MediaMoments.time(point.startMs)).type(Typography.body.weight(600)).monospacedDigit()
            }
            .foregroundStyle(palette.ink)
            .padding(.vertical, 3).padding(.horizontal, 6)
            .background(Capsule().stroke(row.file.exists ? palette.signal : .clear, lineWidth: 1))
            VStack(alignment: .leading, spacing: 2) {
                Text(point.label).type(Typography.body.weight(600)).foregroundStyle(palette.ink)
                Text("“" + point.quote + "”").type(Typography.body).foregroundStyle(palette.inkSoft)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(minHeight: 44, alignment: .topLeading)
        .contentShape(Rectangle())
    }
}

struct RichMediaPlayer: View {
    let viewer: ScreenModel.MediaViewer
    let close: () -> Void
    @Environment(\.palette) private var palette
    @Environment(\.scenePhase) private var phase
    @State private var player = AVPlayer()
    @State private var running = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if viewer.file.mediaType == "video" {
                VideoPlayer(player: player).frame(height: 110)
                    .accessibilityLabel(viewer.file.name)
            }
            HStack(spacing: 8) {
                Button { toggle() } label: {
                    Image(systemName: running ? "pause.fill" : "play.fill")
                        .frame(width: 44, height: 44)
                }
                .accessibilityLabel(running ? "Pause recording" : "Play recording")
                TimelineView(.animation(minimumInterval: 0.25, paused: !running)) { _ in
                    let current = milliseconds
                    let point = viewer.file.moments?.points.last { $0.startMs <= current }
                    let duration = max(1, viewer.file.moments?.durationMs ?? 1)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(point.map { MediaMoments.time($0.startMs) + " · " + $0.label } ?? viewer.file.name)
                            .type(Typography.body.weight(600)).lineLimit(1)
                        ProgressView(value: min(1, Double(current) / Double(duration))).tint(palette.ink)
                        Text(MediaMoments.time(current) + " / " + MediaMoments.time(duration))
                            .type(Typography.body).foregroundStyle(palette.inkSoft).monospacedDigit()
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                Button(action: close) { Image(systemName: "xmark").frame(width: 44, height: 44) }
                    .accessibilityLabel("Close recording")
            }
            .buttonStyle(.plain)
        }
        .foregroundStyle(palette.ink)
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 12).fill(palette.surface))
        .overlay(RoundedRectangle(cornerRadius: 12).stroke(palette.lineFaint, lineWidth: 1))
        .onAppear {
            player.replaceCurrentItem(with: AVPlayerItem(url: viewer.url))
            seek(viewer.selection.startMs)
        }
        .onChange(of: viewer.selection) { _, selection in
            if viewer.file.moments?.points.last(where: { $0.startMs <= milliseconds })?.startMs == selection.startMs {
                toggle()
            } else {
                seek(selection.startMs)
            }
        }
        .onReceive(player.publisher(for: \.timeControlStatus)) { running = $0 != .paused }
        .onChange(of: phase) { _, phase in if phase != .active { player.pause() } }
        .onDisappear { player.pause(); player.replaceCurrentItem(with: nil) }
    }

    private var milliseconds: Int {
        let seconds = player.currentTime().seconds
        return seconds.isFinite ? max(0, Int(seconds * 1000)) : 0
    }

    private func seek(_ milliseconds: Int) {
        let time = CMTime(value: Int64(milliseconds), timescale: 1000)
        player.seek(to: time, toleranceBefore: .zero, toleranceAfter: .zero)
        if phase == .active { player.play() }
    }

    private func toggle() {
        if player.timeControlStatus == .paused, phase == .active { player.play() }
        else { player.pause() }
    }
}

/// The phone's viewer for a kept file (round 36 "Phone: opened"): Apple's own Quick Look, which
/// draws PDFs, pictures, Office and iWork documents on black and carries Share (Save to Files,
/// Mail, Messages, …). Closing it tells the core, so a later arrival does not reopen it by itself.
struct RichFileViewer: UIViewControllerRepresentable {
    let url: URL
    let onClose: () -> Void

    func makeCoordinator() -> Coordinator { Coordinator(url: url, onClose: onClose) }

    func makeUIViewController(context: Context) -> UINavigationController {
        let preview = QLPreviewController()
        preview.dataSource = context.coordinator
        preview.delegate = context.coordinator
        preview.navigationItem.leftBarButtonItem = UIBarButtonItem(systemItem: .close, primaryAction: UIAction { _ in onClose() })
        let nav = UINavigationController(rootViewController: preview)
        nav.overrideUserInterfaceStyle = .dark
        return nav
    }

    func updateUIViewController(_ controller: UINavigationController, context: Context) {}

    final class Coordinator: NSObject, QLPreviewControllerDataSource, QLPreviewControllerDelegate {
        let url: URL
        let onClose: () -> Void
        init(url: URL, onClose: @escaping () -> Void) { self.url = url; self.onClose = onClose }
        func numberOfPreviewItems(in controller: QLPreviewController) -> Int { 1 }
        func previewController(_ controller: QLPreviewController, previewItemAt index: Int) -> QLPreviewItem { url as NSURL }
        func previewControllerDidDismiss(_ controller: QLPreviewController) { onClose() }
    }
}
