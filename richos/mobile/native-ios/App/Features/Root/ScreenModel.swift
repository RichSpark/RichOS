import Foundation
import RichOSCore

/// Everything a screen draws, as one value. Views render only this and report only `Intent`s; they
/// hold no protocol code and make no decisions about delivery, pairing or connection.
///
/// It is DERIVED from `AppState` by `init(state:)` — one pure function, so a CLI `state` and a
/// screenshot can never disagree about what is on screen, and a headless check proves every round-12
/// fixture draws its own screen (`native-ios-ui.test.sh`, part 1). Foundation-only: it compiles on this
/// Mac with no simulator.
struct ScreenModel: Equatable, Sendable {
    var appearance: Appearance = .dark
    /// A full-screen takeover (pairing, consent, removed, update required). Nothing else is interactive.
    var takeover: Takeover?
    var scanner: Scanner?
    var dialog: Dialog?
    var sheet: Sheet?
    var banner: UpdateBanner?
    var thread = Transcript()
    /// A file Rich attached, open in the phone's viewer (round 36 "Phone: opened"); nil otherwise.
    var fileViewer: FileViewer?
    /// The nameplate's second line: only for a persistent interruption (PRD §5 CEO experience gate).
    var connection: ConnectionLine?
    /// Cards above the composer, top to bottom.
    var cards: [Card] = []
    var recoveredQuestionAnswers: [String] = []
    var toast: Toast?
    var composer = Composer()
    /// The recording gesture, exactly as the core holds it.
    var voice: VoiceSession?
    /// Photos and files (round-12 `attachments.html`): the menu, the tray, the viewer, from the core's
    /// `attachMenuOpen` and `pendingAttachments`.
    var attach = Attach()
    /// The conversation's screen while the saved conversation is still being read at launch
    /// (`LaunchRoot`): its ground, header and composer, with no messages and no first-minute hello yet.
    var quiet = false

    /// The conversation's screen in its quiet state, in the phone's appearance (`LaunchRoot`).
    static func quiet(_ appearance: Appearance) -> ScreenModel {
        var model = ScreenModel()
        model.appearance = appearance
        model.quiet = true
        return model
    }

    // MARK: Parts

    enum Takeover: Equatable, Sendable {
        case pairIntro(problem: PairProblem?)
        case pairProgress
        case pairWords([String])
        /// Pairing v2: "They match" was pressed here; the words stay up while the Mac waits for its own press.
        case pairAwaitingMac([String])
        case pairStale
        case consent
        case removedFromMac
        case updateRequired(version: String, message: String)
    }

    enum PairProblem: Equatable, Sendable {
        case refused, invalidLink(String)
        /// Pairing v2's three endings without a pairing. The way on is the intro's own "Scan your
        /// Mac's code": the cards say "scan it again" (Urban's review, question 1).
        case macNeedsUpdate, notAcceptedByMac, macAnswerExpired
        /// "They do not match" pressed on this phone.
        case wordsRejected
        /// No Mac answered the pairing request.
        case macUnreachable
    }

    enum Scanner: Equatable, Sendable { case looking, found }

    enum Dialog: Equatable, Sendable {
        /// Said in the Forget dialog when a kept recording exists, because Forget discards it (D9).
        static let discardedRecordingNote = "The voice message you recorded but did not send will be discarded too."

        case cameraDenied
        /// `removed`: the Mac removed this phone, so the waiting messages can never be sent (Urban G2).
        case pairBlocked(waiting: Int, removed: Bool)
        case forget
        /// Forget while an unsent kept recording exists: the dialog says Forget discards it too (D9).
        case forgetDiscardsRecording
        case forgetBlocked(waiting: Int)
        case update(version: String, message: String)
    }

    enum Sheet: Equatable, Sendable {
        case settings(Settings)
        case whereMessagesGo
        case pairingLink(problem: String?)
    }

    struct Settings: Equatable, Sendable {
        var notifications: NotificationStatus = .on
        var previews = true
        var update: UpdateCheck = .upToDate
        var macName = "your Mac"
        /// "Check for updates" opens this app's App Store listing, so it is shown only once that is live.
        var showsUpdateCheck = AppLinks.appStoreListingLive
    }

    enum NotificationStatus: String, Equatable, Sendable {
        case notAsked, on, off, turningOn, denied, unsupported, appleUnavailable, serviceUnavailable

        /// What the Settings row holds. Never asked (the offer's "Not now") is an off switch, as
        /// Android's Settings draws it (`ScreenModel.kt`: `NOT_ASKED -> OFF`): its tap is where iOS's
        /// question is asked, so "Not now" is never a one-way door (I04). Once iOS has said no, the
        /// only true way on is iPhone Settings.
        enum Control: Equatable, Sendable { case toggle(isOn: Bool), openSettings, none }

        var control: Control {
            switch self {
            case .on: return .toggle(isOn: true)
            case .off, .notAsked: return .toggle(isOn: false)
            case .denied: return .openSettings
            case .turningOn, .unsupported, .appleUnavailable, .serviceUnavailable: return .none
            }
        }
    }

    enum UpdateCheck: Equatable, Sendable { case upToDate, available(String), couldNotCheck }

    struct UpdateBanner: Equatable, Sendable {
        var version: String
        var message: String
    }

    struct Transcript: Equatable, Sendable {
        var rows: [Row] = []
        var loadingOlder = false
        var reachedBeginning = false
        /// Showing history kept on this phone while the Mac is out of reach (`conn-cached`).
        var cached = false
        /// `false` while the reader has scrolled up to older messages.
        var following = true
        var readingAnchor: ReadingAnchor?
        /// The reply a notification opened; it glows once.
        var focusedID: String?
        var isEmpty: Bool { rows.isEmpty }
    }

    struct Row: Equatable, Identifiable, Sendable {
        enum Author: Equatable, Sendable { case rich, me }
        enum Body: Equatable, Sendable {
            case question(QuestionCard, pending: String?, savedAnswer: String?, canEditLocal: Bool)
            case text(String)
            case voice(durationMs: Int, levels: [Double])
            /// Rich is writing: three breathing dots, no sentence (`conv-replying`).
            case replying
            /// Words landing as they come, with a gold caret (`conv-streaming`).
            case streaming(String)
            /// Your photos, one album with an optional caption (`att-conv-album`).
            case album([AttachPhoto], caption: String?)
            /// A file, with an optional caption (`att-conv-file`).
            case file(AttachFile, caption: String?)
        }
        enum Delivery: Equatable, Sendable { case sent, sending, waiting, needsAttention }
        enum Audio: Equatable, Sendable { case ready, preparing, playing(progress: Double) }

        var id: String
        var author: Author
        var body: Body
        var sentAt: Int64
        /// `nil` for Rich; your messages carry their honest delivery state.
        var delivery: Delivery?
        /// Rich's reply read aloud (`conv-playing-reply`).
        var audio: Audio?
        /// "Now" instead of a clock time, for a message that has not reached the Mac yet.
        var isRecent = false
        /// Rich's answer quoting what you sent (`att-ref`): tapping it goes back to it.
        var reference: Reference? = nil
        /// "Shared from Photos" (`share-landed`).
        var sharedFrom: String? = nil
        /// Upload progress, 0…1, while an attachment is sending.
        var progress: Double? = nil
        /// The line's identity in the list when it differs from `id`: one of your messages keeps its
        /// client id from Send to the Mac's row for it (`Message.lineID`).
        var lineID: String? = nil
        var listID: String { lineID ?? id }
        /// Why the Mac refused this message, in its own words ("I could not hear speech in that
        /// recording. …"), shown under it beside Discard; nil when the refusal carried no sentence.
        var refusal: String? = nil
        /// The files Rich attached, drawn under his words in this bubble (round 36 screen 3).
        var richFiles: [RichFileRow] = []
    }

    /// One file Rich attached, and where it stands on this phone (core `fileFetches`), or nil if never tapped.
    struct RichFileRow: Equatable, Sendable {
        var file: RichFile
        var fetch: FileFetch?
    }

    struct ConnectionLine: Equatable, Sendable {
        enum Kind: Equatable, Sendable, CaseIterable {
            case reconnecting, phoneOffline, serviceUnavailable, macUnreachable, incompatible
            case voiceUnsupported, voicePaused, attachmentsUnsupported
            /// D05: on the Tailscale route, this phone is not on Tailscale.
            case tailscaleOff
        }
        var kind: Kind
    }

    enum Card: Equatable, Identifiable, Sendable {
        /// With no internet it is the phone that is offline; only otherwise is it the Mac that cannot be reached.
        static func waitingDetail(count: Int, phoneOffline: Bool) -> String {
            let messages = count == 1 ? "One message" : "\(count) messages"
            return phoneOffline
                ? "This phone is offline. \(messages) will go as soon as it is back online."
                : "Your Mac isn’t reachable from here. \(messages) will go as soon as it is."
        }

        /// `phoneOffline`: the phone has no internet, so the card says that, not that the Mac is unreachable.
        case waitingToSend(count: Int, phoneOffline: Bool = false)
        case notificationOffer
        case microphoneDenied
        case keptRecording(KeptRecording)
        case attachRefused(name: String, detail: String, tooLarge: Bool)
        case attachCameraDenied
        case attachPhotosDenied
        case attachMacUnsupported
        var id: String {
            switch self {
            case .waitingToSend: return "waiting"
            case .notificationOffer: return "notification-offer"
            case .microphoneDenied: return "mic-denied"
            case .keptRecording(let r): return "kept-\(r.id)"
            case .attachRefused: return "attach-refused"
            case .attachCameraDenied: return "attach-camera-denied"
            case .attachPhotosDenied: return "attach-photos-denied"
            case .attachMacUnsupported: return "attach-mac-unsupported"
            }
        }
    }

    struct KeptRecording: Equatable, Sendable {
        enum Reason: Equatable, Sendable { case unsent, interrupted, ceiling }
        var id: String
        var durationMs: Int
        var levels: [Double]
        var reason: Reason
    }

    enum Toast: Equatable, Sendable {
        case tooShort
        case ceilingWarning
        case tooLong(limit: Int)
        case outboxFull(limit: Int)
        case attachLimit
        var text: String {
            switch self {
            case .tooShort: return "Hold the button while you speak. Release to send."
            case .ceilingWarning: return "One minute left on this voice message."
            case .tooLong(let limit):
                return "That’s over the \(limit.formatted(.number.locale(Locale(identifier: "en_US"))))-character limit. Trim it, or send it as a voice message."
            case .outboxFull(let limit):
                return "\(limit) messages are already waiting on this phone. Send or discard some first."
            case .attachLimit: return "Up to 10 at a time. Send these, then add more."
            }
        }
    }

    struct Composer: Equatable, Sendable {
        var draft = ""
        /// Why sending is off, in one line, or `nil` (`comp-disabled`).
        var disabledReason: String?
        /// The keyboard is up and the field has focus (`comp-keyboard`).
        var focused = false
        /// Voice paused by policy or unsupported: the microphone dims and does nothing.
        var voiceAvailable = true
        /// Round 12's limit (`comp-too-long`), from the core.
        static let characterLimit = Limits.messageCharacters
        var isTooLong: Bool { draft.count > Self.characterLimit }
        var hasDraft: Bool { !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    }
}

extension ScreenModel {
    /// The one derivation from the core's state. Total: every `AppState` draws. `attachments` is the
    /// directory the staged photos live in (`ShareIntake.attachmentsDirectory()`), so the tray and your
    /// unsent albums can show them; without it they draw as tiles.
    init(state s: AppState, attachments: URL? = nil, cachedTranscript: Transcript? = nil) {
        self.init()
        appearance = s.appearance

        // The full-screen surface; the core decides which (`AppState.screen`).
        switch s.screen {
        case .pairIntro:
            switch s.pairingProblem {
            case .refused?: takeover = .pairIntro(problem: .refused)
            case .invalidLink(let why)? where s.sheet != .pairingLink: takeover = .pairIntro(problem: .invalidLink(why))
            case .macNeedsUpdate?: takeover = .pairIntro(problem: .macNeedsUpdate)
            case .notAcceptedByMac?: takeover = .pairIntro(problem: .notAcceptedByMac)
            case .macAnswerExpired?: takeover = .pairIntro(problem: .macAnswerExpired)
            case .wordsRejected?: takeover = .pairIntro(problem: .wordsRejected)
            case .macUnreachable?: takeover = .pairIntro(problem: .macUnreachable)
            default: takeover = .pairIntro(problem: nil)
            }
        case .pairScanner:
            takeover = .pairIntro(problem: nil)
            scanner = s.scanner == .found ? .found : .looking
        case .pairProgress: takeover = .pairProgress
        case .pairWords: takeover = .pairWords(s.fingerprintWords)
        case .pairAwaitingMac: takeover = .pairAwaitingMac(s.fingerprintWords)
        case .pairConsent: takeover = .consent
        case .pairStale: takeover = .pairStale
        case .connectionRevoked: takeover = .removedFromMac
        case .updateRequired:
            takeover = .updateRequired(version: s.update?.version ?? "", message: s.update?.message ?? "")
        case .conversation, .conversationEmpty:
            break
        }

        recoveredQuestionAnswers = s.outbox.filter { $0.questionID != nil && $0.state == .blocked }.map { ($0.lastReason ?? "This answer was not sent.") + "\n" + ($0.questionAnswerText ?? "") }

        // The conversation.
        if let cachedTranscript {
            thread = cachedTranscript
        } else {
        let staged = Dictionary(s.outbox.flatMap { $0.files ?? [] }.map { ($0.id, $0.path) }, uniquingKeysWith: { a, _ in a })
        let visible = RichOSCore.Transcript.visible(s)
        thread.rows = visible.map { Row(message: $0, questionsAvailable: s.mac?.questionAnswers == true, stagedPath: { staged[$0] }, directory: attachments) }
        // "Hear it" under each of Rich's finished replies the Mac can read aloud (round 12.1
        // `.rich-audio`); the reply being prepared or played is drawn so just below.
        let macOffersAudio = s.mac?.replyAudio == true
        for (i, message) in visible.enumerated() where ReplyAudio.hearable(message, macOffersAudio: macOffersAudio) {
            if case .text = thread.rows[i].body { thread.rows[i].audio = .ready }
        }
        for i in thread.rows.indices {
            if case .question(let q, _, _, _) = thread.rows[i].body,
               let pending = s.outbox.first(where: { $0.questionID == q.id }) {
                let status = pending.state == .blocked ? (pending.lastReason ?? "Your answer needs attention") : pending.state == .sending || s.connectionNotice == nil ? "On its way to Rich" : "Waiting to send · your Mac is out of reach"
                var displayed = q
                displayed.answer = pending.localQuestionAnswer ?? q.answer
                thread.rows[i].body = .question(displayed, pending: status, savedAnswer: pending.questionAnswerText, canEditLocal: pending.state == .waiting && pending.attempts == 0)
            }
        }
        // A message the Mac refused says why (iPhone re-walk 4, light-48): the Mac's answer (`reason`,
        // HTTP 422 `retry: false`) is kept on the outbox item that holds the message.
        for i in thread.rows.indices where thread.rows[i].delivery == .needsAttention {
            let row = thread.rows[i]
            if let item = s.outbox.first(where: { $0.clientID == row.id || $0.clientID == row.lineID }), item.state == .blocked {
                thread.rows[i].refusal = Self.refusalSentence(item.lastReason)
            }
        }
        if let playback = s.playback, let i = thread.rows.firstIndex(where: { $0.id == playback.messageID }) {
            thread.rows[i].audio = playback.phase == .preparing ? .preparing : .playing(progress: playback.progress)
        }
        // The files Rich attached: where each stands on this phone, then each under his words.
        for i in thread.rows.indices where !thread.rows[i].richFiles.isEmpty {
            thread.rows[i].richFiles = thread.rows[i].richFiles.map { RichFileRow(file: $0.file, fetch: s.fileFetches[$0.file.id]) }
        }
        thread.rows = Self.filesUnderWords(thread.rows)
        let replyAt = s.messages.last?.sentAt ?? 0
        switch s.reply {
        case .thinking?:
            thread.rows.append(Row(id: "~reply", author: .rich, body: .replying, sentAt: replyAt, isRecent: true))
        case .streaming(let text)?:
            thread.rows.append(Row(id: "~reply", author: .rich, body: .streaming(text), sentAt: replyAt, isRecent: true))
        case nil:
            break
        }
        }
        fileViewer = Self.fileViewer(s)
        thread.loadingOlder = s.history.loadingOlder
        thread.reachedBeginning = s.history.reachedBeginning
        thread.cached = s.history.showsOutOfReachLine(connectionNotice: s.connectionNotice)
        thread.following = s.following
        thread.readingAnchor = s.readingAnchor
        thread.focusedID = s.focusedMessageID

        // The nameplate's line: a persistent interruption first, then what voice cannot do now.
        if let notice = s.connectionNotice {
            switch notice {
            case .reconnecting: connection = ConnectionLine(kind: .reconnecting)
            case .phoneOffline: connection = ConnectionLine(kind: .phoneOffline)
            case .serviceUnavailable: connection = ConnectionLine(kind: .serviceUnavailable)
            case .macUnreachable: connection = ConnectionLine(kind: .macUnreachable)
            case .incompatible: connection = ConnectionLine(kind: .incompatible)
            case .tailscaleOff: connection = ConnectionLine(kind: .tailscaleOff)
            }
        } else if s.voiceAvailability == .unsupportedByMac {
            connection = ConnectionLine(kind: .voiceUnsupported)
        } else if s.voiceAvailability == .pausedByPolicy {
            connection = ConnectionLine(kind: .voicePaused)
        }

        // Dialogs and sheets.
        switch s.sheet {
        case .settings?:
            sheet = .settings(Settings(notifications: Self.status(s.notifications.status), previews: s.notifications.previews,
                                       update: s.update.map { .available($0.version) } ?? .upToDate,
                                       macName: s.mac?.name ?? "your Mac"))
        case .whereMessagesGo?:
            sheet = .whereMessagesGo
        case .pairingLink?:
            if case .invalidLink(let why)? = s.pairingProblem { sheet = .pairingLink(problem: why) } else { sheet = .pairingLink(problem: nil) }
        case .forget?:
            dialog = s.keptRecordings.isEmpty ? .forget : .forgetDiscardsRecording
        case .forgetBlocked?:
            dialog = .forgetBlocked(waiting: max(1, s.unsentCount))
        case .cameraDenied?:
            dialog = .cameraDenied
        case .microphonePrompt?, nil:
            break  // the microphone question is the system's own alert, not ours to draw
        }
        if case .blockedByUnsentWork(let count)? = s.pairingProblem {
            dialog = .pairBlocked(waiting: count, removed: s.pairing == .revoked)
        }
        if let update = s.update {
            switch update.prominence {
            case .banner: banner = UpdateBanner(version: update.version, message: update.message)
            case .dialog: if dialog == nil { dialog = .update(version: update.version, message: update.message) }
            case .required: break  // the takeover above
            }
        }

        // Cards above the composer, in round 12's order.
        let waiting = s.messages.filter { $0.delivery == .waiting }.count
        if waiting > 0, let notice = s.connectionNotice, notice != .incompatible {
            cards.append(.waitingToSend(count: waiting, phoneOffline: notice == .phoneOffline))
        }
        for r in s.keptRecordings {
            let reason: KeptRecording.Reason
            switch r.reason {
            case .interrupted: reason = .interrupted
            case .ceiling: reason = .ceiling
            case .unsent: reason = .unsent
            }
            cards.append(.keptRecording(KeptRecording(id: r.id, durationMs: r.durationMs, levels: r.levels, reason: reason)))
        }
        // Only after a press found the microphone off (D03), never from the OS's answer alone.
        if s.microphoneCard, s.microphone == .denied { cards.append(.microphoneDenied) }
        if s.pairing == .paired, s.notifications.status == .notAsked, !s.notifications.offerDismissed {
            cards.append(.notificationOffer)
        }
        switch s.attachNotice {
        case .refused(let name, let bytes, let tooLarge)?:
            let limit = (s.attachmentLimits?.maxFileBytes ?? 26_214_400) / (1024 * 1024)
            let detail = tooLarge
                ? "is \(bytes.map(AttachFile.size) ?? "too large"). Rich can take files up to \(limit) MB each."
                : "is a kind of file Rich can’t open yet. Send a photo, a PDF, or an Office or text file."
            cards.append(.attachRefused(name: name, detail: detail, tooLarge: tooLarge))
        case .cameraDenied?: cards.append(.attachCameraDenied)
        case .photosDenied?: cards.append(.attachPhotosDenied)
        case .macUnsupported?: cards.append(.attachMacUnsupported)
        case nil: break
        }

        // The + menu and the tray (round 12.1 groups 12, 13).
        attach.menuOpen = s.attachMenuOpen
        attach.pending = s.pendingAttachments.map { f in
            if f.isPhoto {
                return .photo(AttachPhoto(id: f.id, source: Self.photoSource(path: f.path, directory: attachments), label: f.name))
            }
            return .file(AttachFile(file: f.id, name: f.name, bytes: f.byteCount))
        }

        switch s.toast {
        case .tooShort?: toast = .tooShort
        case .ceilingWarning?: toast = .ceilingWarning
        case .tooLong(let limit)?: toast = .tooLong(limit: limit)
        case .outboxFull(let limit)?: toast = .outboxFull(limit: limit)
        case .attachLimit?: toast = .attachLimit
        case nil: break
        }

        composer.draft = s.draft
        composer.focused = s.composerFocused
        composer.voiceAvailable = s.voiceAvailability == .available
        if s.connectionNotice == .incompatible { composer.disabledReason = "Sending is off until your Mac updates" }
        voice = s.voice
    }

    /// The Mac's own sentence for a refusal, or nil. A refusal whose answer carried no `reason` keeps
    /// the phone's one-word classification instead (`APIError.Reason`: "refused", "fault"; "too large";
    /// a deferral such as "link-lost"), which is not for a person; the Mac's sentences start with a
    /// capital and are more than one word.
    static func refusalSentence(_ reason: String?) -> String? {
        guard let text = reason?.trimmingCharacters(in: .whitespacesAndNewlines),
              text.first?.isUppercase == true, text.contains(" ") else { return nil }
        return text
    }

    static func status(_ s: Notifications.Status) -> NotificationStatus {
        switch s {
        case .notAsked: return .notAsked
        case .turningOn: return .turningOn
        case .on: return .on
        case .off: return .off
        case .denied: return .denied
        case .unsupported: return .unsupported
        case .appleUnavailable: return .appleUnavailable
        case .serviceUnavailable: return .serviceUnavailable
        }
    }
}

extension ScreenModel.Card {
    /// The card the region above the composer brings into view after `before` became `after`: the
    /// last card that was not there before, or `nil` when none was added (I05). That region is at most
    /// 40% of the screen and scrolls, so a card raised under one already showing (the microphone-off
    /// card under "Waiting to send") was laid out behind the message field, and a press looked like
    /// it did nothing. A card that only changed (a new count) is not new, and nothing moves for it.
    static func raised(before: [ScreenModel.Card], after: [ScreenModel.Card]) -> String? {
        let shown = Set(before.map(\.id))
        return after.last { !shown.contains($0.id) }?.id
    }
}

extension ScreenModel {
    /// The files Rich attached sit in his bubble, under his words (round 36 "Phone: in the chat"):
    /// the Mac sends each as its own row right after his reply (`phone/rows.rs`), and a file row
    /// joins Rich's finished words just before it. One with no words of his before it (a file for a
    /// job) is a bubble of its own. Android's `withFilesUnderWords`.
    static func filesUnderWords(_ rows: [Row]) -> [Row] {
        var out: [Row] = []
        out.reserveCapacity(rows.count)
        for row in rows {
            if !row.richFiles.isEmpty, case .text(let text) = row.body, text.isEmpty,
               let last = out.last, last.author == .rich, case .text = last.body {
                out[out.count - 1].richFiles += row.richFiles
            } else {
                out.append(row)
            }
        }
        return out
    }

    /// The file whose viewer is open (round 36 "Phone: opened"): kept on this phone and tapped.
    static func fileViewer(_ s: AppState) -> FileViewer? {
        guard let fetch = s.fileFetches.values.first(where: { $0.state == .kept && $0.open && $0.path != nil }),
              let path = fetch.path else { return nil }
        return FileViewer(id: fetch.id, url: URL(fileURLWithPath: path))
    }

    struct FileViewer: Equatable, Sendable {
        var id: String
        var url: URL
    }

    /// A staged photo on this phone, or a plain tile when there is no copy to show.
    static func photoSource(path: String?, directory: URL?) -> AttachPhoto.Source {
        guard let path, let directory else { return .unavailable }
        return .file(directory.appendingPathComponent(path))
    }
}

extension ScreenModel.AttachFile {
    /// A tray chip or a bubble for one file: "PDF · 2.4 MB".
    init(file id: String, name: String, bytes: Int) {
        let ext = name.split(separator: ".").count > 1 ? String(name.split(separator: ".").last!).uppercased() : "FILE"
        self.init(id: id, name: name, ext: ext, bytes: bytes, pages: nil)
    }
}

extension ScreenModel.Row {
    /// `stagedPath` finds this phone's copy of an attachment still waiting to go (the outbox holds
    /// it); a photo the Mac already has draws as a tile.
    init(message m: Message, questionsAvailable: Bool = true, stagedPath: (String) -> String? = { _ in nil }, directory: URL? = nil) {
        let author: Author = m.author == .rich ? .rich : .me
        var body: Body
        switch m.kind {
        case .text: body = .text(m.text)
        case .voice: body = .voice(durationMs: m.durationMs ?? 0, levels: m.levels ?? [])
        }
        if questionsAvailable, let question = m.question { body = .question(question, pending: nil, savedAnswer: nil, canEditLocal: false) }
        // Photos and files: an album when every one is a photo, a file bubble for one file.
        if m.kind == .text, let refs = m.attachments, !refs.isEmpty {
            let caption = m.text.isEmpty ? nil : m.text
            if refs.allSatisfy(\.isPhoto) {
                body = .album(refs.map { ScreenModel.AttachPhoto(id: $0.id, source: ScreenModel.photoSource(path: stagedPath($0.id), directory: directory),
                                                               label: $0.name) }, caption: caption)
            } else if refs.count == 1 {
                body = .file(ScreenModel.AttachFile(file: refs[0].id, name: refs[0].name, bytes: refs[0].byteCount), caption: caption)
            }
        }
        let delivery: Delivery?
        if author == .rich {
            delivery = nil
        } else {
            switch m.delivery {
            case .none: delivery = .sent
            case .sending: delivery = .sending
            case .waiting: delivery = .waiting
            case .needsAttention: delivery = .needsAttention
            }
        }
        self.init(id: m.id, author: author, body: body, sentAt: m.sentAt, delivery: delivery,
                  isRecent: delivery == .sending || delivery == .waiting)
        if m.lineID != m.id { lineID = m.lineID }
        // A file Rich attached (round 36 screen 3): no words of its own, the card under his.
        if let file = m.richFile { richFiles = [ScreenModel.RichFileRow(file: file, fetch: nil)] }
    }
}

/// A composition or microphone level does not reparse every historical row. Invalidate only the
/// inputs to transcript projection; the small surrounding UI still reflects every current action.
final class ScreenProjectionCache {
    private var previous: AppState?
    private var directory: URL?
    private var transcript: ScreenModel.Transcript?
    func model(_ state: AppState, attachments: URL?) -> ScreenModel {
        let same = directory == attachments && previous.map {
            $0.messages == state.messages && $0.outbox == state.outbox &&
            $0.reply == state.reply && $0.playback == state.playback && $0.fileFetches == state.fileFetches &&
            $0.mac?.questionAnswers == state.mac?.questionAnswers && $0.mac?.replyAudio == state.mac?.replyAudio &&
            $0.connectionNotice == state.connectionNotice
        } == true
        let result = ScreenModel(state: state, attachments: attachments, cachedTranscript: same ? transcript : nil)
        previous = state
        directory = attachments
        transcript = result.thread
        return result
    }
}
