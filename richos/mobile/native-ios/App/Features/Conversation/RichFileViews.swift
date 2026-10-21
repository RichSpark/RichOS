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
            ForEach(files, id: \.file.id) { RichFileRowView(row: $0, send: send) }
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
