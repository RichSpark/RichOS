import RichOSCore
import SwiftUI

/// The app's one root view: it renders `AppState` and sends `Action`s, and nothing else.
///
/// COMPOSITION SEAM (stream I1 applies it in `App/App/RichOSNativeApp.swift`):
/// `RootView(state: store.state, send: store.send)`. It takes no store type, so the screens never
/// depend on how the core is wired. Every round-12 screen is reached by putting the core in that
/// state — `-rios-fixture <id>` at launch or `bin/rios sim fixture <id>` — never by navigating.
struct RootView: View {
    let state: AppState
    let send: (Action) -> Void
    /// The recording whose "Hold the button while you speak." is still up (`TooShortLine`).
    @State private var tooShortLine: String?
    @State private var projection = ScreenProjectionCache()

    var body: some View {
        let model = TooShortLine.apply(tooShortLine, to: projection.model(state, attachments: Self.attachments))
        ScreenView(model: model) { intent in
            // An intent the core has no action for yet changes nothing: a screen never pretends.
            if let action = intent.action() { send(action) }
        }
        .environment(\.screenClock, Self.clock)
        .onChange(of: TooShortLine.ending(state.voice, posed: state.voice.map(VoiceClock.isPose) ?? false), initial: true) { _, id in
            if let id { tooShortLine = id }
        }
        .task(id: tooShortLine) {
            // One bounded sleep while a line is up; nothing runs otherwise.
            guard tooShortLine != nil else { return }
            try? await Task.sleep(for: .milliseconds(Motion.tooShortLineMs))
            if !Task.isCancelled { tooShortLine = nil }
        }
        #if DEBUG
        // Debug-only probe for the UI tests: the core's mirror of the OS microphone permission, so a
        // test of the voice gestures can tell "the gesture is broken" from "the app has not been
        // told the permission yet" (a platform wiring step, not a screen).
        .overlay(alignment: .topLeading) {
            Color.clear.frame(width: 1, height: 1)
                .accessibilityElement()
                .accessibilityLabel("microphone \(state.microphone.rawValue)")
                .accessibilityIdentifier("debug.microphone.\(state.microphone.rawValue)")
        }
        #endif
    }

    /// Where the staged photos live, so the tray and unsent albums can show them.
    private static let attachments = ShareIntake.attachmentsDirectory()

    /// Debug builds pin the clock to the fixtures' morning (`-rios-now <ms>`), so "Today" and "8:02 AM"
    /// read the same on any day; Release always draws against now.
    private static var clock: ScreenClock {
        #if DEBUG
        if let raw = UserDefaults.standard.string(forKey: "rios-now"), let ms = Double(raw) {
            return ScreenClock(now: Date(timeIntervalSince1970: ms / 1000), calendar: .autoupdatingCurrent)
        }
        #endif
        return .live
    }
}

/// Draws a `ScreenModel`. Everything visible comes from the model; everything tapped leaves as an
/// `Intent`.
struct ScreenView: View {
    let model: ScreenModel
    let send: (Intent) -> Void

    @Environment(\.screenClock) private var clock
    @State private var headerBottom: CGFloat = 120
    @State private var zoneTop: CGFloat = 700
    @State private var showsLatest = false
    @State private var jumpToken = 0
    @State private var questionJumpToken = 0
    @State private var questionJumpID: String?
    @State private var aboveHeight: CGFloat = 0
    /// The cards' region's height as laid out, so the cards can be told how much more room they have.
    @State private var cardsHeight: CGFloat = 0
    /// The empty conversation's own height, measured, so it can sit above the cards instead of under them.
    @State private var emptyHeight: CGFloat = 300
    /// Whether the CURRENT model has a sheet. SwiftUI may call the setter of the binding it was first
    /// given, whose captured model is stale; this box always holds the latest answer.
    @State private var sheetBox = SheetBox()

    final class SheetBox { var hasSheet = false }

    var body: some View {
        let palette = Palette.for(model.appearance)
        let _ = { sheetBox.hasSheet = model.sheet != nil }()
        GeometryReader { root in
            let safeTop = root.safeAreaInsets.top
            let small = root.size.width < 380 || root.size.height < 700
            ZStack(alignment: .top) {
                GroundBackground()
                // A takeover REPLACES the conversation, as Android's RichApp draws one or the other
                // (`RichApp.kt` `when`: a pairing step, removed, a required update, else `Conversation`).
                // Drawn beneath the opaque takeover and only marked hidden, the empty conversation, the
                // message field, Attach and Record stayed in VoiceOver's tree on first launch (I01).
                if model.takeover == nil {
                    conversation(root: root, safeTop: safeTop)
                        .accessibilityHidden(model.dialog != nil || model.scanner != nil || model.attach.viewer != nil)
                }
                if let dialog = model.dialog {
                    DialogView(dialog: dialog, send: send)
                        .transition(.opacity)
                        .zIndex(5)
                }
                if let takeover = model.takeover {
                    TakeoverView(takeover: takeover, smallDevice: small, send: send)
                        .transition(.opacity)
                        .zIndex(4)
                        .accessibilityHidden(model.dialog != nil || model.scanner != nil)
                }
                if let scanner = model.scanner {
                    ScannerView(state: scanner, send: send)
                        .transition(.opacity)
                        .zIndex(6)
                }
                if let viewer = model.attach.viewer, let content = viewerContent(viewer) {
                    content.transition(.opacity).zIndex(7)
                }
                // A file Rich attached, once it is on this phone and was tapped (round 36 "Phone: opened").
                if let file = model.fileViewer {
                    RichFileViewer(url: file.url) { send(.closeRichFile(id: file.id)) }
                        .ignoresSafeArea()
                        .transition(.opacity)
                        .zIndex(8)
                }
            }
            .frame(width: root.size.width, height: root.size.height)
        }
        .palette(palette)
        // Only a person's swipe closes the sheet here; when the core replaces it (Forget → its dialog),
        // the model already has no sheet and nothing is sent, so the dialog is not closed with it.
        .sheet(isPresented: Binding(get: { model.sheet != nil },
                                    set: { [box = sheetBox] in if !$0, box.hasSheet { send(.closeSheet) } })) {
            sheetContent(palette: palette)
        }
        .animation(Motion.outQuint(320), value: model.takeover)
        .animation(Motion.outQuint(250), value: model.dialog)
        .animation(Motion.outQuint(250), value: model.scanner)
    }

    // MARK: The conversation

    private func conversation(root: GeometryProxy, safeTop: CGFloat) -> some View {
        let palette = Palette.for(model.appearance)
        let fullHeight = root.size.height + root.safeAreaInsets.top + root.safeAreaInsets.bottom
        return ZStack(alignment: .top) {
            if model.quiet {
                // The saved conversation is still being read (`LaunchRoot`): nothing between the header
                // and the composer yet, neither its messages nor the first-minute hello, which would be
                // untrue for a person who has a conversation.
                Color.clear
            } else if model.thread.isEmpty {
                // It scrolls between its top and the composer zone, as Android's `EmptyConversation`
                // does (`verticalScroll`, bottom-padded by the zone): at the largest text size the
                // paragraph is taller than an iPhone SE, and unscrolled it pushed Settings and the
                // composer off the screen.
                //
                // D1 (the iPhone walk, the SE): it sits 30% down, but no lower than leaves its whole
                // height above the zone (a notification offer is a card there), and no higher than the
                // header's bottom; where it still does not fit, it scrolls.
                let regionBottom = root.size.height - (max(0, safeTop + root.size.height - zoneTop) + 20)
                let lowest = regionBottom - emptyHeight
                let highest = max(0, headerBottom - safeTop) + 8
                EdgeCuedScroll(id: "empty") {
                    EmptyConversation()
                        .frame(maxWidth: .infinity)
                        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { emptyHeight = $0 }
                }
                .padding(.top, max(highest, min(safeTop + root.size.height * 0.30, lowest)))
                .padding(.bottom, max(0, safeTop + root.size.height - zoneTop) + 20)
                .frame(maxHeight: .infinity, alignment: .top)
            } else {
                TranscriptView(
                    transcript: model.thread, palette: palette,
                    topInset: headerBottom + 16, bottomInset: max(0, fullHeight - zoneTop) + 20,
                    calendar: clock.calendar, now: clock.now, jumpToken: jumpToken, questionJumpToken: questionJumpToken, questionJumpID: questionJumpID, send: send,
                    onShowsLatest: { shows in
                        DispatchQueue.main.async { withAnimation(Motion.latest) { showsLatest = shows } }
                    })
                    // With the out-of-reach line up, the fade follows the taller block down to it, so the
                    // list fades under the line as it does under the header (I02). Otherwise, as before.
                    .mask(fadeUnderHeader(safeTop: model.thread.cached ? max(safeTop, headerBottom - 60) : safeTop,
                                          height: fullHeight))
                    .ignoresSafeArea()
            }
            if model.attach.menuOpen {
                palette.scrim.opacity(0.55)
                    .ignoresSafeArea()
                    .onTapGesture { send(.closeAttachMenu) }
                    .transition(.opacity)
                    .accessibilityHidden(true)
            }
            VStack(spacing: 0) {
                // The out-of-reach line belongs to the header's measured block, so the list starts
                // below it and it is never drawn under the header's fade (I02). The header holds it, so
                // when the block is taller than the room left, it scrolls with the connection line.
                ConversationHeader(connection: model.connection,
                                   outOfReach: model.thread.cached && !model.thread.isEmpty, send: send)
                .background(GeometryReader { p in
                    Color.clear.onAppear { headerBottom = p.frame(in: .global).maxY }
                        .onChange(of: p.frame(in: .global).maxY) { _, y in headerBottom = y }
                })
                // Who gives way when the keyboard, a card and a long connection line share a small
                // screen (the SE fit, 2026-09-28). The header is laid out first, from what the composer
                // zone's own minimum leaves it (the composer keeps its place: accessibility audit F1);
                // then the banner and the composer zone, whose notes region is the one part that
                // shrinks and scrolls; the space between them last. Before, the notes region kept its
                // full height and the header was squeezed below its own content.
                .layoutPriority(2)
                if let banner = model.banner {
                    UpdateBannerView(banner: banner, send: send)
                        .padding(.horizontal, 12).padding(.top, 8)
                        .transition(.opacity.combined(with: .offset(y: 12)))
                        .layoutPriority(1)
                }
                Spacer(minLength: 0)
                bottomZone(height: root.size.height)
                    .layoutPriority(1)
            }
        }
        // Exactly the screen, whatever it holds. A ZStack is as large as its largest layer and lays
        // every layer out at that size: when this layer was taller than the screen (the empty
        // conversation at the accessibility sizes, 1340 pt on a 667 pt iPhone SE), the takeover above
        // it became 1340 pt tall and centered on the window, so its top ran under the status bar and
        // its buttons stayed below the screen however far it scrolled (I03).
        .frame(width: root.size.width, height: root.size.height)
    }

    /// The composer has priority over everything above it at every text size (accessibility audit F1,
    /// F11): cards and the toast share a region of at most 40% of the screen that scrolls when they
    /// are taller, and the compose row never scrolls away.
    private func bottomZone(height: CGFloat) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            if showsLatest || !model.thread.following, !model.thread.isEmpty, !model.attach.menuOpen {
                LatestPill {
                    jumpToken += 1
                    send(.setFollowing(true))
                }
                .frame(maxWidth: .infinity)
                .transition(.opacity.combined(with: .offset(y: 20)))
            }
            if model.attach.menuOpen {
                AttachMenu(send: send)
                    .padding(.leading, 10)
                    .transition(.opacity)
            }
            VStack(spacing: 8) {
                let openQuestions = model.thread.rows.filter { if case .question(let q, _, let saved, _) = $0.body { return q.state == "open" && saved == nil }; return false }
                if let oldest = openQuestions.first {
                    Button("\(openQuestions.count) question\(openQuestions.count == 1 ? "" : "s") open") {
                        send(.setFollowing(false))
                        questionJumpID = oldest.listID
                        questionJumpToken += 1
                    }
                    .type(Typography.read.weight(500)).foregroundStyle(Palette.for(model.appearance).ink)
                    .padding(.horizontal, 16).frame(minHeight: 44)
                    .background(Capsule().fill(Palette.for(model.appearance).surface))
                    .overlay(Capsule().stroke(model.appearance == .dark ? Palette.sovereign.ink.opacity(0.46) : Palette.daybreak.trim, lineWidth: 1))
                    .buttonStyle(.plain).accessibilityIdentifier("questions.open")
                }
                if !model.recoveredQuestionAnswers.isEmpty {
                    ScrollView {
                        ForEach(model.recoveredQuestionAnswers, id: \.self) { answer in Text(answer).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading) }
                    }.frame(maxHeight: height * 0.2)
                }
                if !model.cards.isEmpty || model.toast != nil {
                    ScrollViewReader { region in
                        // R1 (the iPhone re-walk): the cards' height whole when it fits in 60% of the
                        // screen, and never less than that share just because there are more cards (it
                        // used to drop to 40% the moment they were a point taller than 60%, which cut the
                        // kept voice card's buttons); where they are taller, or the header needs the
                        // room (the SE fit), the region scrolls as a whole and the edge that hides more
                        // fades and carries an arrow (EdgeCuedScroll), so nothing is cut unannounced.
                        // R2 at the floor (re-walk 4): and nothing is cut at all: the edge that hides more
                        // keeps a strip for its arrow alone, and every line and button shows whole or not
                        // at all (`wholeLines`).
                        EdgeCuedScroll(id: "cards", wholeLines: true) {
                            AboveComposer(cards: model.cards, toast: model.toast, send: send)
                                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { aboveHeight = $0 }
                        }
                        // R2 (the iPhone re-walk): and the conversation keeps `conversationMinimum` between
                        // the header and the cards, so with the keyboard up the message just sent is not
                        // behind the header. The cards yield (they scroll, with their cue) before the
                        // conversation does, down to a floor where a card's title still shows. One card
                        // yields as several do (R3).
                        .frame(maxHeight: min(max(aboveHeight, 1), height * 0.6, cardsRoom()))
                        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { if Self.moved(cardsHeight, $0) { cardsHeight = $0 } }
                        // I05: a card raised while another shows is brought into view, the least
                        // scroll that shows it, once, when it is raised: never a timer or a redraw.
                        .onChange(of: model.cards) { before, after in
                            guard let raised = ScreenModel.Card.raised(before: before, after: after) else { return }
                            // After this update lays the new card out, so the reader can find it.
                            DispatchQueue.main.async { withAnimation(Motion.card) { region.scrollTo(raised) } }
                        }
                    }
                }
                ComposerView(composer: model.composer, voice: model.voice, pending: model.attach.pending,
                             attachMenuOpen: model.attach.menuOpen, send: send)
                    .layoutPriority(1)
            }
            .padding(.horizontal, 8)
            .padding(.bottom, 8)
            .background(GeometryReader { p in
                Color.clear.onAppear { zoneTop = p.frame(in: .global).minY }
                    .onChange(of: p.frame(in: .global).minY) { _, y in if Self.moved(zoneTop, y) { zoneTop = y } }
            })
        }
        .animation(Motion.spring(320), value: model.attach.menuOpen)
    }

    /// The least height, in points, the conversation keeps between the header and the cards: the last
    /// message (a row is about 61 pt) or the empty state: its mark, title and one line, whole (about 130 pt), plus its own 28 pt of margin above and below.
    private static let conversationMinimum: CGFloat = 175

    /// What the cards may take: what they take now, plus what the gap between the header and the zone
    /// holds beyond the conversation's minimum (negative when the gap is already short of it). It reads
    /// only measured screen positions, so it holds with the keyboard up or down at any text size, and it
    /// settles where the gap is the minimum. The same for one card as for several (R3, iPhone re-walk 4:
    /// the kept voice card alone, kept whole, left the conversation 66 pt with the keyboard up and the
    /// sender's own last message behind the header). What the conversation needs does not depend on how
    /// many cards there are; the cards are the part that can scroll, with a cue at the edge that hides
    /// more and nothing cut (D10's defect was a card cut unannounced under the field), down to a floor
    /// of 80 pt. Where there is room, as with the keyboard down, one card still shows whole.
    private func cardsRoom() -> CGFloat {
        guard cardsHeight > 0, zoneTop > headerBottom else { return .infinity }
        return max(80, cardsHeight + zoneTop - headerBottom - Self.conversationMinimum)
    }

    /// Whether `zoneTop` or `cardsHeight` moved enough to be taken. The cards' region is laid out from
    /// `cardsRoom()`, which reads both, so the two measure the layout they set. Their sum stays put as
    /// the region grows or shrinks, but only to the last bit of a `CGFloat`: at the largest text size
    /// with the "Waiting to send" card on the iPhone 16 Pro Max, each layout handed the next a room one
    /// bit different, `zoneTop` flipped between 550.66666666666674 and 550.66666666666652 and
    /// `cardsHeight` between 269.25520833333326 and 269.25520833333337, and the screen never settled:
    /// 99% CPU, the conversation never drawn (2026-10-23). A move of half a point or less is not one
    /// anybody sees; ignoring it lets the layout come to rest.
    private static func moved(_ old: CGFloat, _ new: CGFloat) -> Bool { abs(new - old) > 0.5 }

    /// The thread fades out under the header (`.thread` `mask-image`). `safeTop` is where the header
    /// block's 60 pt begin; the stops sit 30, 64 and 100 pt below it.
    private func fadeUnderHeader(safeTop: CGFloat, height: CGFloat) -> some View {
        let h = max(height, 1)
        return LinearGradient(stops: [
            .init(color: .clear, location: 0),
            .init(color: .clear, location: (safeTop + 30) / h),
            .init(color: .black.opacity(0.35), location: (safeTop + 64) / h),
            .init(color: .black, location: (safeTop + 100) / h),
            .init(color: .black, location: 1),
        ], startPoint: .top, endPoint: .bottom)
        .ignoresSafeArea()
    }

    private func viewerContent(_ viewer: ScreenModel.Viewer) -> AttachViewer? {
        guard let row = model.thread.rows.first(where: { $0.id == viewer.messageID }) else { return nil }
        switch row.body {
        case .album(let photos, let caption):
            return AttachViewer(photos: photos, file: nil, index: viewer.index, caption: caption, sentAt: row.sentAt,
                                calendar: clock.calendar, send: send)
        case .file(let file, let caption):
            return AttachViewer(photos: [], file: file, index: 0, caption: caption, sentAt: row.sentAt,
                                calendar: clock.calendar, send: send)
        default:
            return nil
        }
    }

    @ViewBuilder private func sheetContent(palette: Palette) -> some View {
        Group {
            switch model.sheet {
            case .settings(let settings)?:
                SettingsSheet(settings: settings, send: send)
            case .whereMessagesGo?:
                WhereMessagesGoSheet(send: send)
            case .pairingLink(let problem)?:
                PairingLinkSheet(problem: problem, send: send)
            case nil:
                EmptyView()
            }
        }
        .palette(palette)
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
        .presentationCornerRadius(28)
        .presentationBackground(palette.ground)
    }
}

/// The time and calendar the conversation's day markers and clock times are drawn against. Live: now,
/// in the phone's time zone. Debug: `-rios-now <ms>` pins it.
struct ScreenClock: Sendable {
    var now: Date
    var calendar: Calendar

    static var live: ScreenClock { ScreenClock(now: Date(), calendar: .autoupdatingCurrent) }
}

private struct ScreenClockKey: EnvironmentKey {
    static var defaultValue: ScreenClock { .live }
}

extension EnvironmentValues {
    var screenClock: ScreenClock {
        get { self[ScreenClockKey.self] }
        set { self[ScreenClockKey.self] = newValue }
    }
}
