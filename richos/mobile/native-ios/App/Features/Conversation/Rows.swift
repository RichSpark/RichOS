import SwiftUI

/// One entry in the conversation list: a message, or one of the quiet markers between them.
enum TranscriptItem: Equatable, Identifiable, Sendable {
    case loadingOlder
    case beginning
    /// `occurrence` counts the times this label has already started a run of rows above it.
    case day(String, occurrence: Int = 0)
    case row(ScreenModel.Row, tail: Bool, focused: Bool)

    var id: String {
        switch self {
        case .loadingOlder: return "~loading"
        case .beginning: return "~beginning"
        case .day(let label, let occurrence): return occurrence == 0 ? "~day-\(label)" : "~day-\(label)#\(occurrence + 1)"
        case .row(let row, _, _): return row.listID
        }
    }

    /// The list round 12 draws (`app.js` `renderThread`): the loading line or the beginning, the day
    /// markers, then the messages oldest first. The last message of a run by one speaker is its
    /// "tail" and squares its outer bottom corner.
    ///
    /// Every id is unique: the list's diffable data source throws on a repeated identifier, which
    /// ends the app (the iPhone walk of 2026-10-21, RichOSNative-2026-10-21-144802.ips). The rows
    /// are in the Mac's cursor order with the phone's own unsent bubbles after them
    /// (`ConversationReducer.ordered`), so a day can come back below a later one: a message held
    /// offline across midnight, or a Mac whose history cursors start again. That day's marker is
    /// drawn again under an id of its own. A second row with an id already drawn is the same line
    /// and is not drawn twice.
    static func build(_ t: ScreenModel.Transcript, calendar: Calendar, now: Date) -> [TranscriptItem] {
        var items: [TranscriptItem] = []
        if t.loadingOlder { items.append(.loadingOlder) } else if t.reachedBeginning { items.append(.beginning) }
        // `t.cached` is not a row: the out-of-reach line is pinned under the header (`OutOfReachLine`).
        var seen = Set<String>()
        let rows = t.rows.filter { seen.insert($0.listID).inserted }
        var lastDay: DateComponents?
        var daysShown: [String: Int] = [:]
        for (index, row) in rows.enumerated() {
            let date = Date(timeIntervalSince1970: TimeInterval(row.sentAt) / 1000)
            let day = calendar.dateComponents([.year, .month, .day], from: date)
            if day != lastDay {
                let label = DayLabel.text(for: date, calendar: calendar, now: now)
                items.append(.day(label, occurrence: daysShown[label, default: 0]))
                daysShown[label, default: 0] += 1
                lastDay = day
            }
            let next = index + 1 < rows.count ? rows[index + 1] : nil
            items.append(.row(row, tail: next?.author != row.author, focused: row.id == t.focusedID))
        }
        return items
    }
}

enum DayLabel {
    static func text(for date: Date, calendar: Calendar, now: Date) -> String {
        if calendar.isDate(date, inSameDayAs: now) { return "Today" }
        if let yesterday = calendar.date(byAdding: .day, value: -1, to: now),
           calendar.isDate(date, inSameDayAs: yesterday) { return "Yesterday" }
        let style = Date.FormatStyle(date: .complete, time: .omitted, locale: Locale(identifier: "en_US"),
                                     calendar: calendar, timeZone: calendar.timeZone)
        return date.formatted(style.weekday(.wide).month(.wide).day())
    }

    /// "8:02 AM", in the phone's time zone and American English (ceo-decisions §13).
    static func time(_ ms: Int64, calendar: Calendar) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
        let style = Date.FormatStyle(date: .omitted, time: .shortened, locale: Locale(identifier: "en_US"),
                                     calendar: calendar, timeZone: calendar.timeZone)
        return date.formatted(style)
    }

    /// "0:08" — a voice message's length.
    static func duration(_ ms: Int) -> String {
        let s = Int((Double(ms) / 1000).rounded())
        return "\(s / 60):" + String(format: "%02d", s % 60)
    }

    /// "8 seconds", "1 minute 5 seconds" — the same length, for VoiceOver.
    static func spokenDuration(_ ms: Int) -> String {
        let s = Int((Double(ms) / 1000).rounded())
        let m = s / 60, r = s % 60
        var parts: [String] = []
        if m > 0 { parts.append(m == 1 ? "1 minute" : "\(m) minutes") }
        if r > 0 || m == 0 { parts.append(r == 1 ? "1 second" : "\(r) seconds") }
        return parts.joined(separator: " ")
    }
}

/// Draws one list entry. Pure: everything it shows comes from the item.
struct TranscriptItemView: View {
    let item: TranscriptItem
    let width: CGFloat
    let calendar: Calendar
    let send: (Intent) -> Void
    @Environment(\.palette) private var palette

    var body: some View {
        switch item {
        case .loadingOlder:
            HStack(spacing: 10) {
                Spinner()
                Text("Loading earlier messages…")
            }
            .type(Typography.read)
            .foregroundStyle(palette.inkSoft)
            .frame(maxWidth: .infinity)
            .padding(.top, 18).padding(.bottom, 8)
            .accessibilityElement(children: .combine)
        case .beginning:
            VStack(spacing: 8) {
                MarkView().frame(width: 34, height: 34).opacity(0.9)
                Text("This is the beginning of your conversation with Rich.")
                    .type(Typography.read)
                    .foregroundStyle(palette.inkSoft)
                    .multilineTextAlignment(.center)
            }
            .frame(maxWidth: .infinity)
            .padding(.top, 22).padding(.bottom, 8)
        case .day(let label, _):
            DayMarker(text: label)
        case .row(let row, let tail, let focused):
            MessageRow(row: row, width: width, tail: tail, focused: focused, calendar: calendar, send: send)
        }
    }
}

/// `.daymark`: a small pill on the surface, centered.
struct DayMarker: View {
    let text: String
    @Environment(\.palette) private var palette
    var body: some View {
        Text(text)
            .type(Typography.read)
            .foregroundStyle(palette.inkSoft)
            .multilineTextAlignment(.center)
            .padding(.horizontal, 14).padding(.vertical, 5)
            .floatingSurface(palette, radius: 14)
            .frame(maxWidth: .infinity)
            .padding(.top, 14).padding(.bottom, 10)
            .accessibilityAddTraits(.isHeader)
    }
}

/// "Showing what was on this phone · your Mac is out of reach" (`conn-cached`, `launch-cached`),
/// pinned under the header while the conversation on screen is the copy kept on this phone.
///
/// Round 12 draws it as the first day marker of the list. There it sat under the floating header and
/// its fade whenever the list followed the newest message, which is how every launch opens: on the
/// physical iPhone it read at 1.38:1 (I02, native acceptance r1). Pinned, it is never under the header
/// and never scrolls away. Full ink on the surface, 4.5:1 or better in both themes, computed by
/// `native-ios-ui.test.sh`; the round-12 day marker's ink-soft would also pass, but this line is the
/// one explanation of why nothing new is arriving, so it is set in the reading ink.
struct OutOfReachLine: View {
    static let words = "Showing what was on this phone · your Mac is out of reach"
    @Environment(\.palette) private var palette
    var body: some View {
        Text(Self.words)
            .type(Typography.read)
            .foregroundStyle(palette.ink)
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityIdentifier("conversation.cachedNotice")
            .padding(.horizontal, 14).padding(.vertical, 6)
            .floatingSurface(palette, radius: 14)
            .frame(maxWidth: .infinity)
            // The header's own ceiling: the line reflows, and never crowds the conversation out.
            .dynamicTypeSize(...DynamicTypeSize.accessibility3)
    }
}

struct MessageRow: View {
    let row: ScreenModel.Row
    /// The list's content width (the screen less 12 pt each side).
    let width: CGFloat
    let tail: Bool
    let focused: Bool
    let calendar: Calendar
    let send: (Intent) -> Void
    @Environment(\.palette) private var palette
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    private var mine: Bool { row.author == .me }
    private var isQuestion: Bool { if case .question = row.body { return true }; return false }

    var body: some View {
        HStack(spacing: 0) {
            if mine { Spacer(minLength: 0) }
            VStack(alignment: .trailing, spacing: 6) {
                bubble
                if row.delivery == .needsAttention { attentionLine }
            }
            .frame(width: fixedWidth, alignment: mine ? .trailing : .leading)
            .frame(maxWidth: width * (isQuestion ? 0.92 : mine ? 0.78 : 0.84), alignment: mine ? .trailing : .leading)
            if !mine { Spacer(minLength: 0) }
        }
        .padding(.top, 0)
        .padding(.bottom, tail ? 6 : 2)
    }

    /// `.row > .cell`: Rich's 84% of the list, yours 78%; a voice message exactly 74%; an album 74% up to
    /// 290 pt; a file 78% up to 300 pt (round-12 `attach.css`).
    private var fixedWidth: CGFloat? {
        switch row.body {
        case .question: return width * 0.92
        case .voice: return width * 0.74
        case .album: return min(width * 0.74, 290)
        case .file: return min(width * 0.78, 300)
        default: return nil
        }
    }

    /// A bubble with its own controls keeps them reachable one by one; a text bubble reads as one.
    private var hasControls: Bool {
        switch row.body {
        case .voice, .album, .file, .question: return true
        default: return row.audio != nil || row.reference != nil || !row.richFiles.isEmpty
        }
    }

    @ViewBuilder private var bubble: some View {
        let shape = BubbleShape(tail: tail ? (mine ? .right : .left) : nil)
        Group {
            switch row.body {
            case .question(let q, let pending, let savedAnswer, let canEditLocal):
                QuestionCardView(question: q, pending: pending, savedAnswer: savedAnswer, canEditLocal: canEditLocal, send: send)
            case .text(let text):
                BubbleLayout(spacing: 4) {
                    if let reference = row.reference {
                        ReferenceChip(reference: reference, calendar: calendar, send: send)
                            .layoutValue(key: FullWidthInBubble.self, value: true)
                            .padding(.bottom, 4)
                    }
                    if !text.isEmpty || row.richFiles.isEmpty {
                        Text(text)
                            .type(mine ? Typography.body.lineHeight(1.5) : Typography.answer)
                            .foregroundStyle(palette.ink)
                            .fixedSize(horizontal: false, vertical: true)
                            .textSelection(.enabled)
                    }
                    // The files Rich attached, under his words (round 36 screen 3).
                    if !row.richFiles.isEmpty {
                        RichFileList(files: row.richFiles, send: send)
                            .layoutValue(key: FullWidthInBubble.self, value: true)
                    }
                    Meta(row: row, calendar: calendar)
                        .layoutValue(key: TrailingInBubble.self, value: true)
                    if let audio = row.audio {
                        RichAudio(audio: audio, id: row.id, send: send)
                            .layoutValue(key: FullWidthInBubble.self, value: true)
                    }
                }
                .padding(.top, 10).padding(.horizontal, 14).padding(.bottom, 8)
            case .voice(let duration, let levels):
                VoiceBubbleBody(row: row, durationMs: duration, levels: levels, calendar: calendar, send: send)
                    .padding(.vertical, 8).padding(.leading, 8).padding(.trailing, 12)
            case .replying:
                ThinkingDots()
                    .padding(.vertical, 10).padding(.horizontal, 14)
                    .frame(minHeight: 44)
            case .streaming(let text):
                StreamingText(text: text)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 10).padding(.horizontal, 14).padding(.bottom, 8)
            case .album(let photos, let caption):
                VStack(alignment: .leading, spacing: 0) {
                    ZStack(alignment: .bottomTrailing) {
                        AlbumView(photos: photos, width: (fixedWidth ?? 280) - 8) { index in
                            send(.openViewer(messageID: row.id, index: index))
                        }
                        .overlay {
                            if let delivery = row.delivery, delivery != .sent {
                                ZStack {
                                    Color(red: 8 / 255, green: 12 / 255, blue: 22 / 255).opacity(0.38)
                                    UploadDisc(delivery: delivery, progress: row.progress ?? 0) {
                                        send(delivery == .needsAttention ? .retryNow : .discard(id: row.id))
                                    }
                                }
                                .clipShape(RoundedRectangle(cornerRadius: 17, style: .continuous))
                            }
                        }
                        if caption?.isEmpty ?? true {
                            Meta(row: row, calendar: calendar)
                                .environment(\.palette, .sovereign)
                                .padding(.horizontal, 8).padding(.vertical, 3)
                                .background(Capsule().fill(Color(red: 8 / 255, green: 12 / 255, blue: 22 / 255).opacity(0.8)))
                                .padding(8)
                        }
                    }
                    if let caption, !caption.isEmpty {
                        Text(caption)
                            .type(Typography.body.lineHeight(1.5))
                            .foregroundStyle(palette.ink)
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.horizontal, 10).padding(.top, 8)
                        Meta(row: row, calendar: calendar)
                            .frame(maxWidth: .infinity, alignment: .trailing)
                            .padding(.trailing, 6).padding(.top, 4).padding(.bottom, 2)
                    }
                }
                .padding(4)
            case .file(let file, let caption):
                VStack(alignment: .leading, spacing: 6) {
                    FileCard(file: file) { send(.openViewer(messageID: row.id, index: 0)) }
                    if let caption, !caption.isEmpty {
                        Text(caption)
                            .type(Typography.body.lineHeight(1.5))
                            .foregroundStyle(palette.ink)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    Meta(row: row, calendar: calendar)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                }
                .padding(.top, 10).padding(.horizontal, 12).padding(.bottom, 8)
            }
        }
        .background { if !isQuestion { shape.fill(mine ? palette.mine : palette.surface).floatShadow(palette) } }
        .overlay(shape.stroke(mine ? palette.signalWash : Color.clear, lineWidth: 1))
        .overlay { if focused { FocusGlow(shape: shape) } }
        .overlay(alignment: .topTrailing) {
            if let from = row.sharedFrom {
                Text("Shared from \(from)")
                    .type(Typography.read)
                    .foregroundStyle(palette.inkSoft)
                    .offset(y: -24)
            }
        }
        .modifier(RowAccessibility(hasControls: hasControls, label: accessibilityText))
        .accessibilityIdentifier("row.\(row.id)")
    }

    /// "Not sent · needs attention  Discard", and under it the Mac's reason when it gave one (iPhone
    /// re-walk 4: a refused voice message did not say why).
    private var attentionLine: some View {
        VStack(alignment: .trailing, spacing: 4) {
            HStack(spacing: 8) {
                (Text("Not sent").run(Typography.read.weight(600), dynamicTypeSize).foregroundColor(palette.danger)
                 + Text(" · needs attention").foregroundColor(palette.ink))
                Button { send(.discard(id: row.id)) } label: { QuietLabel(text: "Discard") }
                    .buttonStyle(.plain)
                    .foregroundStyle(palette.ink)
                    .accessibilityLabel("Discard this message")
            }
            if let refusal = row.refusal {
                Text(refusal)
                    .foregroundStyle(palette.ink)
                    .multilineTextAlignment(.trailing)
                    .accessibilityIdentifier("row.\(row.id).reason")
            }
        }
        .type(Typography.read)
        .fixedSize(horizontal: false, vertical: true)
    }

    private var accessibilityText: String {
        let who = mine ? "You" : "Rich"
        let time = row.isRecent ? "now" : DayLabel.time(row.sentAt, calendar: calendar)
        let delivery: String
        switch row.delivery {
        case .none, .sent?: delivery = ""
        case .sending?: delivery = ", sending"
        case .waiting?: delivery = ", waiting to send"
        case .needsAttention?: delivery = ", not sent, needs attention"
        }
        switch row.body {
        case .question(let q, _, _, _): return q.text
        case .text(let text):
            let files = row.richFiles.map(\.file.name).joined(separator: ", ")
            return "\(who), \(time)\(delivery): \(text)" + (files.isEmpty ? "" : (text.isEmpty ? "" : ". ") + "Files: \(files)")
        case .voice(let ms, _): return "\(mine ? "Your" : "Rich's") voice message, \(DayLabel.spokenDuration(ms)), \(time)\(delivery)"
        case .replying: return "Rich is replying"
        case .streaming(let text): return "Rich, replying: \(text)"
        case .album(let photos, let caption):
            let what = photos.count == 1 ? "a photo" : "\(photos.count) photos"
            return "\(who) sent \(what), \(time)\(delivery)" + (caption.map { ": \($0)" } ?? "")
        case .file(let file, let caption):
            return "\(who) sent \(file.name), \(time)\(delivery)" + (caption.map { ": \($0)" } ?? "")
        }
    }
}

/// A text bubble reads as one sentence ("Rich, 8:02 AM: …"); a bubble with controls (Play, Hear it, a
/// photo, a reference) keeps each control reachable, and its first element says what it is.
private struct RowAccessibility: ViewModifier {
    let hasControls: Bool
    let label: String
    func body(content: Content) -> some View {
        if hasControls {
            content
                .accessibilityElement(children: .contain)
                .accessibilityLabel(label)
        } else {
            content
                .accessibilityElement(children: .combine)
                .accessibilityLabel(label)
        }
    }
}

/// A bubble: 20 pt corners, the last bubble of a run squares its outer bottom corner to 6 pt.
struct BubbleShape: InsettableShape {
    enum Tail: Equatable { case left, right }
    var tail: Tail?
    var inset: CGFloat = 0

    func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: inset, dy: inset)
        let big: CGFloat = 20 - inset, small: CGFloat = 6 - inset
        return Path(roundedRect: r, cornerRadii: RectangleCornerRadii(
            topLeading: big,
            bottomLeading: tail == .left ? small : big,
            bottomTrailing: tail == .right ? small : big,
            topTrailing: big), style: .continuous)
    }

    func inset(by amount: CGFloat) -> BubbleShape {
        var s = self
        s.inset += amount
        return s
    }
}

/// Time inside the bubble, and your message's delivery mark (`.meta`). The time is 14 pt — declared
/// skippable (round-12 NOTES "Type"); the delivery WORDS ("Sending…", "Waiting to send") are the same
/// size in round 12, and are repeated in full in the row's VoiceOver label.
struct Meta: View {
    let row: ScreenModel.Row
    let calendar: Calendar
    @Environment(\.palette) private var palette

    var body: some View {
        HStack(spacing: 6) {
            if let label { Text(label) }
            Text(row.isRecent ? "Now" : DayLabel.time(row.sentAt, calendar: calendar))
            if let delivery = row.delivery { mark(delivery) }
        }
        .type(Typography.skippable)
        .monospacedDigit()
        .foregroundStyle(palette.inkSoft)
        .padding(.trailing, -4)
        .accessibilityHidden(true)
    }

    private var label: String? {
        switch row.delivery {
        case .sending?: return "Sending…"
        case .waiting?: return "Waiting to send"
        default: return nil
        }
    }

    @ViewBuilder private func mark(_ delivery: ScreenModel.Row.Delivery) -> some View {
        switch delivery {
        case .sent: IconView(.check, size: 15, weight: 2.4)
        case .sending: SpinningIcon()
        case .waiting: IconView(.clock, size: 15, weight: 2.4)
        case .needsAttention: IconView(.alert, size: 15, weight: 2.4).foregroundStyle(palette.danger)
        }
    }
}

/// The "Sending…" mark: one turn every 1.1 s for its first 9.9 s (nine whole turns), then at rest
/// until the delivery changes (`SpinSchedule`); still under Reduce Motion.
private struct SpinningIcon: View {
    static let period: TimeInterval = 1.1
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var appeared = Date()
    @State private var resting = false

    var body: some View {
        Group {
            if resting || !SpinSchedule.animates(elapsed: 0, period: Self.period, reduceMotion: reduceMotion) {
                IconView(.ringc, size: 15, weight: 2.4)
            } else {
                TimelineView(.animation) { context in
                    IconView(.ringc, size: 15, weight: 2.4)
                        .rotationEffect(.degrees(SpinSchedule.turn(elapsed: context.date.timeIntervalSince(appeared), period: Self.period) * 360))
                }
            }
        }
        .task {
            try? await Task.sleep(nanoseconds: UInt64(SpinSchedule.restsAt(period: Self.period) * 1_000_000_000))
            if !Task.isCancelled { resting = true }
        }
    }
}

/// Three gold dots breathing, 1.2 s, each 150 ms behind the last (`.replying`).
struct ThinkingDots: View {
    @Environment(\.palette) private var palette
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var body: some View {
        TimelineView(.animation(paused: reduceMotion)) { context in
            let t = context.date.timeIntervalSinceReferenceDate
            HStack(spacing: 6) {
                ForEach(0..<3, id: \.self) { i in
                    let phase = ((t - Double(i) * 0.15) / 1.2).truncatingRemainder(dividingBy: 1)
                    let k = 0.5 - 0.5 * cos(phase * 2 * .pi)  // in-out 0 → 1 → 0
                    Circle().fill(palette.signal)
                        .frame(width: 7, height: 7)
                        .opacity(0.45 + 0.55 * k)
                        .offset(y: -3 * k)
                }
            }
            .frame(height: 27)
        }
        .accessibilityHidden(true)
    }
}

/// A reply still arriving: its words, with a 2 pt gold caret right after the last one, blinking once a
/// second (`.caret`). The caret is an inline image in the text run, so it sits after the last word on
/// whatever line that word lands, never in a corner of the bubble.
struct StreamingText: View {
    let text: String
    @Environment(\.palette) private var palette
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.5)) { context in
            let on = reduceMotion || Int(context.date.timeIntervalSinceReferenceDate * 2) % 2 == 0
            (Text(text + " ") + Text(Image(uiImage: Self.caret(on ? UIColor(palette.signal) : .clear))).baselineOffset(-3))
                .type(Typography.answer)
                .foregroundStyle(palette.ink)
        }
    }

    private static func caret(_ color: UIColor) -> UIImage {
        UIGraphicsImageRenderer(size: CGSize(width: 2, height: 20)).image { context in
            color.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 2, height: 20))
        }
    }
}

/// The gold ring on the reply a notification opened: it swells once over 2.2 s, then rests at 2 pt.
struct FocusGlow: View {
    let shape: BubbleShape
    @Environment(\.palette) private var palette
    @State private var glow = false
    var body: some View {
        shape.inset(by: glow ? -2 : -1)
            .stroke(palette.signal, lineWidth: glow ? 3 : 2)
            .onAppear {
                withAnimation(Motion.outQuint(660)) { glow = true }
                withAnimation(Motion.outQuint(1540).delay(0.66)) { glow = false }
            }
            .accessibilityHidden(true)
    }
}

/// A bubble's contents: as wide as its widest line (never wider than offered), text on the leading
/// edge, the time on the trailing edge (`.bubble` shrink-wraps in round 12; a stack that stretches
/// every bubble to its maximum width would not).
struct BubbleLayout: Layout {
    var spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let offered = ProposedViewSize(width: proposal.width, height: nil)
        let sizes = subviews.map { $0[FullWidthInBubble.self] ? CGSize.zero : $0.sizeThatFits(offered) }
        let width = min(proposal.width ?? .infinity, sizes.map(\.width).max() ?? 0)
        var height: CGFloat = 0
        for (i, sub) in subviews.enumerated() {
            let h = sub[FullWidthInBubble.self]
                ? sub.sizeThatFits(ProposedViewSize(width: width, height: nil)).height : sizes[i].height
            height += h + (i > 0 ? spacing : 0)
        }
        return CGSize(width: width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for sub in subviews {
            let full = sub[FullWidthInBubble.self]
            let size = sub.sizeThatFits(ProposedViewSize(width: bounds.width, height: nil))
            let w = full ? bounds.width : min(size.width, bounds.width)
            let x = sub[TrailingInBubble.self] ? bounds.maxX - w : bounds.minX
            sub.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(width: w, height: size.height))
            y += size.height + spacing
        }
    }
}

struct TrailingInBubble: LayoutValueKey { static let defaultValue = false }
struct FullWidthInBubble: LayoutValueKey { static let defaultValue = false }
