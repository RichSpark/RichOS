import Foundation

/// Everything the iPhone app shows, as data — independent of how it is drawn.
///
/// The SwiftUI screens render it; `bin/rios headless state` prints it; the Debug app's development
/// bridge reports it. Because all three read this one value, a headless run and a simulator run of
/// the same scenario can be compared for exact equality (`rios sim verify`). No behavior may live
/// only in a view: a view reads these fields and sends `Action`s.
///
/// Two kinds of field. PERSISTED fields survive a relaunch (`AppState.Persisted`, written by the
/// `.persist` effect). TRANSIENT fields describe this moment — a gesture, a sheet, a permission the
/// OS reports — and are rebuilt, never stored, so a touch-rate action never writes a file.
///
/// Named `AppState`, not `State`, so a SwiftUI file importing this module keeps `@State` unambiguous.
public struct AppState: Codable, Equatable, Sendable {
    /// Bumped whenever a stored field changes meaning. A NEWER stored schema is refused on load and
    /// the file is left untouched (it may hold unsent work) — the preserved client's rule.
    public static let schemaVersion = 2

    public var schema: Int = AppState.schemaVersion

    // MARK: pairing (round-12 group 1)
    public var pairing: Pairing = .unpaired
    /// The Mac this phone is paired with, or is pairing with.
    public var mac: MacLink?
    /// The six v2 words while `pairing` is `.confirming` or `.awaitingMac` (computed on the phone over
    /// the origin it dialed, the Mac's pairing value and its own key; `Fingerprint`).
    public var fingerprintWords: [String] = []
    /// The wait for "They match" ON THE MAC (pairing v2): its bound from the pair answer, then its
    /// deadline and schedule once the phone has said "They match". `nil` when not pairing.
    public var macWait: MacWait?
    /// Apple's explicit-consent screen (`pair-consent`) was answered with Continue. Persisted.
    public var consentGiven = false
    /// TRANSIENT. The QR scanner is on screen.
    public var scanner: Scanner?
    /// TRANSIENT. Why pairing cannot proceed right now.
    public var pairingProblem: PairingProblem?

    // MARK: conversation and composer (groups 2, 3)
    /// The conversation, oldest first: server-ordered history plus your unsent messages.
    public var messages: [Message] = []
    public var draft = ""
    /// Your messages the Mac has not accepted yet, in the order you sent them — each with the exact
    /// bytes it will be resent with (contract §5.2, §6.4). Persisted before the bubble shows.
    public var outbox: [OutboxItem] = []
    /// TRANSIENT. Rich is working on a reply.
    public var reply: ReplyActivity?
    /// TRANSIENT. Loading older messages, the start of the conversation, cached-only history.
    public var history = History()
    /// TRANSIENT. The conversation follows the newest message unless the person scrolled up.
    public var following = true
    public var readingAnchor: ReadingAnchor?
    /// TRANSIENT. A message opened from a notification, glowing once.
    public var focusedMessageID: String?
    /// A tapped notification's reply that is not loaded yet: the SHA-256 (lowercase hex) of its id,
    /// as the notification carries it (contract §7.3). Older history is fetched until a message with
    /// that reference appears, then it is focused and this clears. Saved, so a relaunch keeps looking.
    public var notifiedReply: String?
    /// TRANSIENT. The keyboard is up for the composer.
    public var composerFocused = false
    /// Photos and files chosen from the + menu, waiting in the composer's tray, in the order chosen.
    /// Persisted with the draft: the staged copies are on this phone until sent or removed.
    public var pendingAttachments: [OutboxFile] = []
    /// TRANSIENT. The + menu is open (`att-menu`).
    public var attachMenuOpen = false
    /// TRANSIENT. A card about photos and files (refused, permission off, Mac cannot take them).
    public var attachNotice: AttachNotice?
    /// TRANSIENT. The one reply being heard, if any.
    public var playback: Playback?
    /// TRANSIENT. The files Rich attached that were tapped on this phone, by id (round 36 screen 3).
    /// Never saved: a kept copy is found again on disk at the next tap.
    public var fileFetches: [String: FileFetch] = [:]

    // MARK: voice (groups 4, 5, 6)
    /// TRANSIENT. The recording gesture in progress.
    public var voice: VoiceSession?
    /// Recordings kept instead of sent (interrupted, ceiling reached, or never sent). Persisted.
    public var keptRecordings: [KeptRecording] = []
    /// TRANSIENT. Whether voice can be sent right now (Mac capability, update policy).
    public var voiceAvailability: VoiceAvailability = .available
    /// TRANSIENT MIRRORS of the OS permission state (PRD §3: no parallel permission state is stored).
    public var microphone: Permission = .unknown
    public var camera: Permission = .unknown
    /// TRANSIENT. A press found the microphone off: the microphone-off card is up (round-12
    /// `rec-mic-denied`, D03). Only a press raises it; "Not now", a grant or forgetting the pairing
    /// takes it down. Never saved, and printed only while up (Android's `microphoneCard`).
    public var microphoneCard = false

    // MARK: connection (group 7)
    /// TRANSIENT. `nil` while healthy — routine recovery is invisible, and a notice exists only for a
    /// persistent interruption (PRD §5, the CEO experience gate).
    public var connectionNotice: ConnectionNotice?
    /// TRANSIENT. When the current transport trouble began; the notice waits `ConnectionReducer.quietMs`.
    public var troubleSinceMs: Int64?
    /// TRANSIENT. The Mac's live stream is open: it answered the phone's signed request and has not
    /// dropped since. The outbox moves only while it is (the reference client's `online`, set from the
    /// link's `open`; Android's `session.online`): one connection owner and one retry policy, so a Mac
    /// that cannot answer is asked by the stream's back-off alone, never by a second loop of sends (I06).
    public var linkOpen = false
    /// TRANSIENT. The OS's word on whether this phone is on Tailscale (`TailscaleTunnel`): `nil` until
    /// the app has read it, and never guessed. Only the Tailscale route's notice reads it (D05).
    public var tunnelUp: Bool?

    // MARK: notifications, settings, updates (groups 8, 9, 10)
    public var notifications = Notifications()
    /// TRANSIENT. The sheet or dialog on top of the conversation.
    public var sheet: Sheet?
    /// TRANSIENT. The update policy's current notice, if any.
    public var update: UpdateNotice?
    /// TRANSIENT. What the paired Mac accepts for photos and files (`attachment_limits` in `hello`
    /// and the pairing answer); `nil` when it does not take attachments. Read, never hard-coded.
    public var attachmentLimits: AttachmentLimits?
    /// TRANSIENT. One calm line above the composer.
    public var toast: Toast?

    public var appearance: Appearance = .dark

    public init() {}

    /// A new install: not paired, dark (ceo-decisions §15: dark is what a new install opens in).
    public static let initial = AppState()

    /// The full-screen surface this state is on. Sheets, cards, notices and the voice gesture are
    /// drawn over `.conversation` from their own fields. Derived, never stored.
    public var screen: Screen {
        if pairingProblem == .sessionNeedsNewerApp { return .pairStale }
        if update?.prominence == .required { return .updateRequired }
        switch pairing {
        case .unpaired: return scanner == nil ? .pairIntro : .pairScanner
        // The viewfinder flashes closed on the code it found, then the progress screen follows.
        case .connecting: return scanner == .found ? .pairScanner : .pairProgress
        case .confirming: return .pairWords
        case .awaitingMac: return .pairAwaitingMac
        // "Pair again" opens the scanner over the removed screen; closing it comes back here.
        case .revoked: return scanner == nil ? .connectionRevoked : .pairScanner
        case .paired:
            if !consentGiven { return .pairConsent }
            return messages.isEmpty && voice == nil && keptRecordings.isEmpty ? .conversationEmpty : .conversation
        }
    }

    /// Your messages the Mac has not accepted yet. Pairing a different Mac and forgetting this one
    /// are refused while any exist (round-12 `pair-blocked`, `settings-forget-blocked`).
    public var unsentCount: Int { outbox.count }
}

// MARK: - the printed form

extension AppState {
    /// Every field, plus the derived `screen`, so a CLI reader sees the surface directly. Absent
    /// fields decode to a new install's values; `screen` is ignored on the way in.
    private enum CodingKeys: String, CodingKey {
        case readingAnchor, schema, pairing, mac, fingerprintWords, macWait, consentGiven, scanner, pairingProblem, messages, draft, outbox, reply, history, following, focusedMessageID, notifiedReply, composerFocused, playback, fileFetches, voice, keptRecordings, voiceAvailability, microphone, microphoneCard, camera, connectionNotice, troubleSinceMs, linkOpen, tunnelUp, notifications, sheet, update, attachmentLimits, toast, appearance, screen
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = AppState()
        self.init()
        schema = try c.decodeIfPresent(Int.self, forKey: .schema) ?? d.schema
        pairing = try c.decodeIfPresent(Pairing.self, forKey: .pairing) ?? d.pairing
        mac = try c.decodeIfPresent(MacLink.self, forKey: .mac)
        fingerprintWords = try c.decodeIfPresent([String].self, forKey: .fingerprintWords) ?? d.fingerprintWords
        macWait = try c.decodeIfPresent(MacWait.self, forKey: .macWait)
        consentGiven = try c.decodeIfPresent(Bool.self, forKey: .consentGiven) ?? d.consentGiven
        scanner = try c.decodeIfPresent(Scanner.self, forKey: .scanner)
        pairingProblem = try c.decodeIfPresent(PairingProblem.self, forKey: .pairingProblem)
        messages = try c.decodeIfPresent([Message].self, forKey: .messages) ?? d.messages
        draft = try c.decodeIfPresent(String.self, forKey: .draft) ?? d.draft
        outbox = try c.decodeIfPresent([OutboxItem].self, forKey: .outbox) ?? d.outbox
        reply = try c.decodeIfPresent(ReplyActivity.self, forKey: .reply)
        history = try c.decodeIfPresent(History.self, forKey: .history) ?? d.history
        readingAnchor = try c.decodeIfPresent(ReadingAnchor.self, forKey: .readingAnchor)
        following = try c.decodeIfPresent(Bool.self, forKey: .following) ?? d.following
        focusedMessageID = try c.decodeIfPresent(String.self, forKey: .focusedMessageID)
        notifiedReply = try c.decodeIfPresent(String.self, forKey: .notifiedReply)
        composerFocused = try c.decodeIfPresent(Bool.self, forKey: .composerFocused) ?? d.composerFocused
        playback = try c.decodeIfPresent(Playback.self, forKey: .playback)
        fileFetches = try c.decodeIfPresent([String: FileFetch].self, forKey: .fileFetches) ?? d.fileFetches
        voice = try c.decodeIfPresent(VoiceSession.self, forKey: .voice)
        keptRecordings = try c.decodeIfPresent([KeptRecording].self, forKey: .keptRecordings) ?? d.keptRecordings
        voiceAvailability = try c.decodeIfPresent(VoiceAvailability.self, forKey: .voiceAvailability) ?? d.voiceAvailability
        microphone = try c.decodeIfPresent(Permission.self, forKey: .microphone) ?? d.microphone
        microphoneCard = try c.decodeIfPresent(Bool.self, forKey: .microphoneCard) ?? d.microphoneCard
        camera = try c.decodeIfPresent(Permission.self, forKey: .camera) ?? d.camera
        connectionNotice = try c.decodeIfPresent(ConnectionNotice.self, forKey: .connectionNotice)
        troubleSinceMs = try c.decodeIfPresent(Int64.self, forKey: .troubleSinceMs)
        linkOpen = try c.decodeIfPresent(Bool.self, forKey: .linkOpen) ?? d.linkOpen
        tunnelUp = try c.decodeIfPresent(Bool.self, forKey: .tunnelUp)
        notifications = try c.decodeIfPresent(Notifications.self, forKey: .notifications) ?? d.notifications
        sheet = try c.decodeIfPresent(Sheet.self, forKey: .sheet)
        update = try c.decodeIfPresent(UpdateNotice.self, forKey: .update)
        attachmentLimits = try c.decodeIfPresent(AttachmentLimits.self, forKey: .attachmentLimits)
        toast = try c.decodeIfPresent(Toast.self, forKey: .toast)
        appearance = try c.decodeIfPresent(Appearance.self, forKey: .appearance) ?? d.appearance
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(schema, forKey: .schema)
        try c.encode(pairing, forKey: .pairing)
        try c.encode(mac, forKey: .mac)
        try c.encode(fingerprintWords, forKey: .fingerprintWords)
        try c.encodeIfPresent(macWait, forKey: .macWait)
        try c.encode(consentGiven, forKey: .consentGiven)
        try c.encode(scanner, forKey: .scanner)
        try c.encode(pairingProblem, forKey: .pairingProblem)
        try c.encode(messages, forKey: .messages)
        try c.encode(draft, forKey: .draft)
        try c.encode(outbox, forKey: .outbox)
        try c.encode(reply, forKey: .reply)
        try c.encode(history, forKey: .history)
        try c.encodeIfPresent(readingAnchor, forKey: .readingAnchor)
        try c.encode(following, forKey: .following)
        try c.encode(focusedMessageID, forKey: .focusedMessageID)
        try c.encode(notifiedReply, forKey: .notifiedReply)
        try c.encode(composerFocused, forKey: .composerFocused)
        try c.encode(playback, forKey: .playback)
        if !fileFetches.isEmpty { try c.encode(fileFetches, forKey: .fileFetches) }
        try c.encode(voice, forKey: .voice)
        try c.encode(keptRecordings, forKey: .keptRecordings)
        try c.encode(voiceAvailability, forKey: .voiceAvailability)
        try c.encode(microphone, forKey: .microphone)
        if microphoneCard { try c.encode(microphoneCard, forKey: .microphoneCard) }
        try c.encode(camera, forKey: .camera)
        try c.encode(connectionNotice, forKey: .connectionNotice)
        try c.encode(troubleSinceMs, forKey: .troubleSinceMs)
        try c.encode(linkOpen, forKey: .linkOpen)
        try c.encode(tunnelUp, forKey: .tunnelUp)
        try c.encode(notifications, forKey: .notifications)
        try c.encode(sheet, forKey: .sheet)
        try c.encode(update, forKey: .update)
        try c.encode(attachmentLimits, forKey: .attachmentLimits)
        try c.encode(toast, forKey: .toast)
        try c.encode(appearance, forKey: .appearance)
        try c.encode(screen, forKey: .screen)
    }
}

// MARK: - the durable part

extension AppState {
    /// Exactly what survives a relaunch. Everything else is rebuilt.
    public struct Persisted: Codable, Equatable, Sendable {
        public var schema: Int
        public var pairing: Pairing
        public var mac: MacLink?
        public var fingerprintWords: [String]
        public var consentGiven: Bool
        public var messages: [Message]
        public var draft: String
        public var outbox: [OutboxItem]
        public var keptRecordings: [KeptRecording]
        public var notifications: Notifications
        public var appearance: Appearance
        public var notifiedReply: String?
        /// Optional so a state saved before the tray existed still loads (same schema).
        public var pendingAttachments: [OutboxFile]?
        /// New writers keep reconstructable transcript history outside the durable user-work file.
        public var separateHistory: Bool? = nil
        public var following: Bool? = nil
        public var readingAnchor: ReadingAnchor? = nil
        public var activeRecording: KeptRecording? = nil
        /// Pairing v2's wait for the press on the Mac: its bound, its deadline once the phone has said
        /// "They match", and the probes already made, so a relaunch neither outlives the bound nor
        /// resets the 22-probe ceiling (the Android core keeps the same three). The schedule itself is
        /// rebuilt on return to the screen. Optional: older states have none.
        public var macWaitBoundMs: Int64? = nil
        public var macWaitDeadlineMs: Int64? = nil
        public var macWaitRequests: Int? = nil
        /// The Mac offered `pair-wait`, so a relaunch keeps asking it to hold (and keeps 7 s between
        /// asks). Absent means no, as in every state written before it.
        public var macWaitHolds: Bool? = nil
    }

    public var persisted: Persisted {
        // A pairing in flight is not a pairing: a relaunch starts it again from the link.
        Persisted(schema: schema, pairing: pairing == .connecting ? .unpaired : pairing, mac: pairing == .connecting ? nil : mac,
                  fingerprintWords: fingerprintWords, consentGiven: consentGiven, messages: messages, draft: draft,
                  outbox: outbox, keptRecordings: keptRecordings, notifications: notifications, appearance: appearance,
                  notifiedReply: notifiedReply, pendingAttachments: pendingAttachments.isEmpty ? nil : pendingAttachments,
                  following: following, readingAnchor: following ? nil : readingAnchor,
                  activeRecording: voice.flatMap { v in
                      guard v.phase == .held || v.phase == .locked, let start = v.recordingStartedAtMs else { return nil }
                      return KeptRecording(id: v.id, durationMs: 0, levels: [], reason: .interrupted, recordedAt: start)
                  },
                  macWaitBoundMs: macWait?.boundMs, macWaitDeadlineMs: macWait?.deadlineMs,
                  macWaitRequests: macWait.flatMap { $0.requests > 0 ? $0.requests : nil },
                  macWaitHolds: macWait.flatMap { $0.holds ? true : nil })
    }

    public init(restoring p: Persisted) throws {
        guard p.schema == Self.schemaVersion else { throw StoredSchemaError(found: p.schema) }
        self.init()
        pairing = p.pairing; mac = p.mac; fingerprintWords = p.fingerprintWords; consentGiven = p.consentGiven
        messages = p.messages; draft = p.draft; outbox = p.outbox; keptRecordings = p.keptRecordings
        notifications = p.notifications; appearance = p.appearance; notifiedReply = p.notifiedReply
        pendingAttachments = p.pendingAttachments ?? []
        following = p.following ?? true
        readingAnchor = p.readingAnchor
        if pairing == .confirming || pairing == .awaitingMac {
            // Restored paused: a launch is a return to the screen, and `foregrounded` resumes the wait
            // (or, past the bound, makes its last ask).
            macWait = MacWait(boundMs: p.macWaitBoundMs ?? MacWait.windowMs, deadlineMs: p.macWaitDeadlineMs,
                              requests: p.macWaitRequests ?? 0, paused: pairing == .awaitingMac,
                              holds: p.macWaitHolds ?? false)
        }
        // A message that was mid-send when the app stopped is re-sent at once on this launch: the
        // Mac's `client_id` receipt makes that free (the reference queue's rule 3).
        for i in outbox.indices where outbox[i].state == .sending {
            outbox[i].state = .waiting
            outbox[i].notBefore = 0
        }
        for i in messages.indices where messages[i].delivery == .sending { messages[i].delivery = .waiting }
        // Saved history is on screen before the Mac has confirmed it (round-12 `launch-cached`);
        // connecting clears this.
        history.cached = !messages.isEmpty
    }
}

/// A stored state from a different app version. `newer` is the case the person sees as `pair-stale`.
public struct StoredSchemaError: Error, Equatable, CustomStringConvertible, Sendable {
    public var found: Int
    public var newer: Bool { found > AppState.schemaVersion }
    public var description: String { "stored state has schema \(found); this build reads \(AppState.schemaVersion)" }
}

// MARK: - the domain types

public enum Pairing: String, Codable, Sendable {
    case unpaired
    /// The pairing request is on its way (round-12 `pair-progress`). Never persisted.
    case connecting
    /// The six-word check is on screen (`pair-words`).
    case confirming
    /// Pairing v2: "They match" was pressed on the phone; the Mac lets this phone in only once the
    /// person presses "They match" on the Mac too (`pair-awaiting-mac`). Persisted, with the wait's
    /// deadline, so a relaunch inside the bound goes back to waiting.
    case awaitingMac
    case paired
    /// The Mac removed this phone; terminal until paired again (`conn-revoked`).
    case revoked
}

public struct MacLink: Codable, Equatable, Sendable {
    public var questionAnswers: Bool?
    /// The Mac offers `audio`: it reads any reply aloud on request (`GET /api/audio/<id>`).
    public var replyAudio: Bool?
    /// The API base, fixed by the pairing link (contract §1.2).
    public var origin: String
    public var route: PairLink.Route
    /// `dev_` + 12 hex, derived from this phone's key and reported by the Mac (contract §2.2).
    public var deviceID: String?
    /// The Mac's current conversation.
    public var threadID: String?
    /// For Settings ("Paired with …"); `nil` until the Mac says.
    public var name: String?

    public init(origin: String, route: PairLink.Route, deviceID: String? = nil, threadID: String? = nil, name: String? = nil) {
        self.origin = origin; self.route = route; self.deviceID = deviceID; self.threadID = threadID; self.name = name
    }
}

public enum Scanner: String, Codable, Sendable {
    /// `pair-scanner`: the camera is looking for the code.
    case looking
    /// `pair-scanner-found`: a valid code was read; pairing begins.
    case found
}

public enum PairingProblem: Codable, Equatable, Sendable {
    /// `pair-refused`: the Mac did not accept the code. Nothing was paired.
    case refused
    /// `pair-blocked`: unsent messages were written for the Mac this phone is paired with now.
    case blockedByUnsentWork(count: Int)
    /// `pair-stale`: the saved session is from a newer app. The data is kept.
    case sessionNeedsNewerApp
    /// A pasted or scanned link is not a pairing link; the text is the reference client's message.
    case invalidLink(String)
    /// No Mac answered the pairing request: check that the Mac is awake and the route reachable.
    case macUnreachable
    /// Pairing v2: the Mac answered without `pair-v2`. It was told to forget this phone; nothing was paired.
    case macNeedsUpdate
    /// Pairing v2: while the phone waited for the press on the Mac, the Mac refused this phone
    /// ("They do not match" on the Mac, or its window closed). Nothing was paired.
    case notAcceptedByMac
    /// Pairing v2: the bound passed with no press on the Mac. Nothing was paired.
    case macAnswerExpired
    /// "They do not match" pressed on this phone (on the words, or while waiting for the Mac): the
    /// one security decision in pairing, so the screen says it stopped and nothing was paired
    /// (Urban's review, state 4; the Android core's `words-rejected`).
    case wordsRejected
}

/// Round-12 screen identifiers for the FULL-SCREEN surfaces, spelled as
/// `design/mockups/rounds/round-12/shared/screens.js` spells them.
public enum Screen: String, Codable, Sendable {
    case pairIntro = "pair-intro"
    case pairScanner = "pair-scanner"
    case pairProgress = "pair-progress"
    case pairWords = "pair-words"
    /// Pairing v2, not in round 12: the words stay up while the phone waits for the press on the Mac.
    case pairAwaitingMac = "pair-awaiting-mac"
    case pairConsent = "pair-consent"
    case pairStale = "pair-stale"
    case conversationEmpty = "conv-empty"
    case conversation = "conv-populated"
    case connectionRevoked = "conn-revoked"
    case updateRequired = "upd-blocking"
}

public struct Message: Codable, Equatable, Identifiable, Sendable {
    public enum Author: String, Codable, Sendable { case rich, me }
    public enum Kind: String, Codable, Sendable { case text, voice }
    /// Your own message's delivery, in place of read receipts (round-12 `conv-pending`: "three
    /// honest states"). `nil` for Rich's messages and for yours once the Mac has accepted them.
    public enum Delivery: String, Codable, Sendable {
        case sending
        case waiting
        case needsAttention = "needs-attention"
    }

    public var id: String
    public var author: Author
    public var kind: Kind
    /// The text, or a voice message's transcription once the Mac has one.
    public var text: String
    /// Milliseconds since 1970, from the `Clock` port — never read from the wall inside the core.
    public var sentAt: Int64
    public var delivery: Delivery?
    /// Voice only.
    public var durationMs: Int?
    /// Voice only: the recording's own level, 0…1, sampled every 100 ms (the view resamples to 42 bars).
    public var levels: [Double]?
    /// The idempotency key of one of your messages: the phone's own bubble carries it as its id, and
    /// the Mac's projected row carries it here, which is how the row replaces the bubble.
    public var clientID: String?
    /// The Mac's history position; the conversation is ordered by it. `nil` for the phone's own
    /// bubbles until the Mac's row replaces them.
    public var cursor: Int?
    /// Rows already visible when Send was pressed cannot acknowledge this new message by text.
    /// Optional for sessions written by older versions.
    public var echoAfterCursor: Int?
    /// The stable identity of that boundary; replay may renumber its cursor.
    public var echoAfterMessageID: String?
    /// Signed voice receipt: match the transcript before its projected row carries voice metadata.
    public var transcriptSHA256: String?
    /// Photos and files sent with this message (the text is then its caption). `nil` for none.
    public var question: QuestionCard?
    public var attachments: [AttachmentRef]?
    /// Rich's reply whose row said `has_audio: true`. `nil` (never `false`) otherwise, so stored
    /// conversations from earlier versions read the same.
    public var hasAudio: Bool?
    /// A file Rich attached (round 36 screen 3): the card drawn under his words. `nil` otherwise.
    public var richFile: RichFile?

    public init(id: String, author: Author, kind: Kind = .text, text: String, sentAt: Int64,
                delivery: Delivery? = nil, durationMs: Int? = nil, levels: [Double]? = nil, clientID: String? = nil, cursor: Int? = nil,
                attachments: [AttachmentRef]? = nil, echoAfterCursor: Int? = nil, echoAfterMessageID: String? = nil, transcriptSHA256: String? = nil) {
        self.id = id; self.author = author; self.kind = kind; self.text = text; self.sentAt = sentAt
        self.delivery = delivery; self.durationMs = durationMs; self.levels = levels; self.clientID = clientID; self.cursor = cursor
        self.attachments = attachments; self.echoAfterCursor = echoAfterCursor; self.echoAfterMessageID = echoAfterMessageID; self.transcriptSHA256 = transcriptSHA256
    }
}

/// One message on its way to the Mac (the reference outbox, `richos/web/web-app/lib/queue.js`).
public struct OutboxItem: Codable, Equatable, Sendable {
    public enum State: String, Codable, Sendable {
        /// On the phone, not yet accepted.
        case waiting
        /// In flight now.
        case sending
        /// The Mac gave a final answer; this needs the person, not a retry.
        case blocked
    }
    /// The idempotency key, chosen once (contract §3.4). Also the id of the message's bubble.
    public var clientID: String
    public var kind: Message.Kind
    /// Text: the exact request body, serialized ONCE at send and resent byte for byte on every
    /// retry — any change, even to an unread field, turns a safe retry into a 409 (contract §5.2).
    public var body: String?
    /// Voice: the kept recording this sends.
    public var recordingID: String?
    /// Path and query, fixed at queue time and resent unchanged (voice carries its metadata here).
    /// `nil` means `/api/messages`.
    public var target: String?
    public var queuedAt: Int64
    public var state: State
    public var attempts: Int
    /// No attempt before this instant (ms since 1970); 0 means now.
    public var notBefore: Int64
    /// Why it is blocked or was last refused, in the Mac's words when it gave any.
    public var lastReason: String?
    /// Photos and files: each is uploaded before `body` (then the commit's exact bytes) is sent.
    public var questionID: String?
    public var questionAnswerText: String?
    public var files: [OutboxFile]?

    public init(clientID: String, kind: Message.Kind, body: String?, recordingID: String? = nil, target: String? = nil, queuedAt: Int64,
                state: State = .waiting, attempts: Int = 0, notBefore: Int64 = 0, lastReason: String? = nil, files: [OutboxFile]? = nil) {
        self.clientID = clientID; self.kind = kind; self.body = body; self.recordingID = recordingID; self.target = target; self.queuedAt = queuedAt
        self.state = state; self.attempts = attempts; self.notBefore = notBefore; self.lastReason = lastReason; self.files = files
    }
}

public enum ReplyActivity: Codable, Equatable, Sendable {
    /// `conv-replying`: three gold dots, no sentence.
    case thinking
    /// `conv-streaming`: the words so far, with a caret.
    case streaming(text: String)
}

public struct History: Codable, Equatable, Sendable {
    /// `conv-older-loading`: chunked loading at the top; never pagination.
    public var loadingOlder = false
    /// `conv-beginning`: the first message is on screen.
    public var reachedBeginning = false
    /// `conn-cached`, `launch-cached`: showing saved history, not yet reconciled with the Mac.
    public var cached = false
    /// Whether the out-of-reach line shows: saved history not yet reconciled AND the Mac has been
    /// quiet long enough to be explained (`connectionNotice`, set after the quiet 3 s). Android's
    /// `cachedWhileOffline`: `!hasConnected && notice != null`. A healthy start never shows it.
    public func showsOutOfReachLine(connectionNotice: ConnectionNotice?) -> Bool {
        cached && connectionNotice != nil
    }
    public init(loadingOlder: Bool = false, reachedBeginning: Bool = false, cached: Bool = false) {
        self.loadingOlder = loadingOlder; self.reachedBeginning = reachedBeginning; self.cached = cached
    }
}

public struct Playback: Codable, Equatable, Sendable {
    public enum Phase: String, Codable, Sendable {
        /// `conv-preparing-reply`: the Mac is synthesizing the reply's audio.
        case preparing
        /// `conv-playing-reply`
        case playing
    }
    public var messageID: String
    public var phase: Phase
    /// 0…1 of the reply heard so far.
    public var progress: Double
    public init(messageID: String, phase: Phase, progress: Double = 0) {
        self.messageID = messageID; self.phase = phase; self.progress = progress
    }
}

/// One recording gesture (round-12 NOTES "What an engineer should know": `idle → pressed → held →
/// locked → ending → idle`). Every threshold is in `VoiceGesture`; the view only draws this.
public struct VoiceSession: Codable, Equatable, Sendable {
    /// Names the recording, and becomes its outbox `clientID` and bubble id if it is sent.
    public var id: String
    public enum Phase: Codable, Equatable, Sendable {
        /// The first 200 ms after touch-down (`voice-press`): the button squishes, nothing records.
        case pressed
        /// Recording while the finger is down (`voice-holding`, `voice-slide-left`, `voice-slide-up`).
        case held
        /// Recording hands-free (`voice-locked`).
        case locked
        /// The end animation is playing; `voiceSettled` returns to idle.
        case ending(VoiceEnding)
    }
    public var phase: Phase
    /// When the finger went down, and the latest time the core has been told about.
    public var startedAtMs: Int64
    public var nowMs: Int64
    /// When recording actually began (after the press delay); `nil` while `.pressed`.
    public var recordingStartedAtMs: Int64?
    /// Offset of the finger from the touch-down point, in points (negative = left / up).
    public var dx: Double
    public var dy: Double
    /// The composer's width, which sets the cancel distance.
    public var width: Double
    /// 0…1 toward cancel (`-dx / cancelDistance`) and toward lock (`-dy / 60`).
    public var cancelProgress: Double
    public var lockProgress: Double
    /// The recording's level so far, sampled every 100 ms, for the bubble it becomes.
    public var levels: [Double]
    /// The session reached `.locked` at some point: its ending animates from the locked layout
    /// (`voice-locked-cancel`, `voice-locked-send`) rather than from a held finger.
    public var wasLocked: Bool
    /// The one-minute warning has been shown for this recording (it is shown once).
    public var ceilingWarned: Bool

    /// What the timer shows, `M:SS.t` of this.
    public var elapsedMs: Int64 { recordingStartedAtMs.map { max(0, nowMs - $0) } ?? 0 }

    public init(id: String = "voice-1", phase: Phase, startedAtMs: Int64, nowMs: Int64, recordingStartedAtMs: Int64?, dx: Double = 0, dy: Double = 0,
                width: Double, cancelProgress: Double = 0, lockProgress: Double = 0, levels: [Double] = [], wasLocked: Bool = false, ceilingWarned: Bool = false) {
        self.id = id
        self.phase = phase; self.startedAtMs = startedAtMs; self.nowMs = nowMs; self.recordingStartedAtMs = recordingStartedAtMs
        self.dx = dx; self.dy = dy; self.width = width; self.cancelProgress = cancelProgress; self.lockProgress = lockProgress
        self.levels = levels; self.wasLocked = wasLocked; self.ceilingWarned = ceilingWarned
    }
}

public enum VoiceEnding: String, Codable, Sendable {
    /// `voice-sent`, `voice-locked-send`
    case sent
    /// `voice-bin`, `voice-locked-cancel`
    case canceled
    /// `voice-too-short`: a tap is not a message.
    case tooShort = "too-short"
    /// `voice-ceiling-reached`: stopped at 30:00 and kept.
    case ceiling
}

public struct KeptRecording: Codable, Equatable, Identifiable, Sendable {
    public enum Reason: String, Codable, Sendable {
        /// `voice-interrupted`: the app left the screen while recording.
        case interrupted
        /// `voice-ceiling-reached`
        case ceiling
        /// `rec-card`: kept and not sent, for any other reason (the Mac refused it, voice was off).
        case unsent
    }
    public var id: String
    public var durationMs: Int
    public var levels: [Double]
    public var reason: Reason
    public var recordedAt: Int64
    public init(id: String, durationMs: Int, levels: [Double], reason: Reason, recordedAt: Int64) {
        self.id = id; self.durationMs = durationMs; self.levels = levels; self.reason = reason; self.recordedAt = recordedAt
    }
}

public enum VoiceAvailability: String, Codable, Sendable {
    case available
    /// `upd-feature-off`: paused by the update policy; typing still works.
    case pausedByPolicy = "paused-by-policy"
    /// `rec-unsupported`: this Mac cannot accept voice yet.
    case unsupportedByMac = "unsupported-by-mac"
}

public enum Permission: String, Codable, Sendable { case unknown, granted, denied }

/// Persistent interruptions only (round-12 group 7). Never "connected": healthy is not announced.
public enum ConnectionNotice: String, Codable, Sendable {
    /// Shown only after 3 s of trouble (`conn-reconnecting`).
    case reconnecting
    case phoneOffline = "phone-offline"
    case serviceUnavailable = "service-unavailable"
    case macUnreachable = "mac-unreachable"
    /// `conn-incompatible`, `comp-disabled`: sending pauses; queued messages are kept.
    case incompatible
    /// D05 (`conn-tailscale-off`): paired over Tailscale, and this phone's own Tailscale is not up, so
    /// it cannot reconnect by itself. Said after the same quiet 3 s as "Reconnecting…", instead of it.
    case tailscaleOff = "tailscale-off"
}

public struct Notifications: Codable, Equatable, Sendable {
    public enum Status: String, Codable, Sendable {
        case notAsked = "not-asked"
        case turningOn = "turning-on"
        case on
        case off
        /// Refused in iPhone Settings.
        case denied
        case unsupported
        case appleUnavailable = "apple-unavailable"
        case serviceUnavailable = "service-unavailable"
    }
    public var status: Status = .notAsked
    /// `notif-offer` was answered "Not now"; it is asked once.
    public var offerDismissed = false
    /// Lock-screen previews of Rich's reply (decrypted on the phone). On by default.
    public var previews = true
    /// The Mac's push host id from the registration answer; a tapped notification from any other
    /// host opens nothing (contract §7.3).
    public var hostID: String?
    public init(status: Status = .notAsked, offerDismissed: Bool = false, previews: Bool = true, hostID: String? = nil) {
        self.status = status; self.offerDismissed = offerDismissed; self.previews = previews; self.hostID = hostID
    }
}

public enum Sheet: String, Codable, Sendable {
    case settings
    /// `settings-forget`
    case forget
    /// `settings-forget-blocked`
    case forgetBlocked = "forget-blocked"
    /// Settings → "Where your messages go" (the consent text, again).
    case whereMessagesGo = "where-messages-go"
    /// Paste a pairing link instead of scanning.
    case pairingLink = "pairing-link"
    /// `pair-camera-denied`
    case cameraDenied = "camera-denied"
    /// `voice-permission`: the system's one-time microphone question is up.
    case microphonePrompt = "microphone-prompt"
}

public struct UpdateNotice: Codable, Equatable, Sendable {
    public enum Prominence: String, Codable, Sendable {
        /// `upd-banner`
        case banner
        /// `upd-dialog`
        case dialog
        /// `upd-blocking`: cannot be dismissed.
        case required
    }
    public var prominence: Prominence
    public var version: String
    public var message: String
    public init(prominence: Prominence, version: String, message: String) {
        self.prominence = prominence; self.version = version; self.message = message
    }
}

public enum Toast: Codable, Equatable, Sendable {
    /// `voice-too-short`: shows for 1.8 s.
    case tooShort
    /// `voice-ceiling-warning`: at 29:00.
    case ceilingWarning
    /// `comp-too-long`, with the limit that matters.
    case tooLong(limit: Int)
    /// The phone holds the most unsent messages it will keep; resolve some before sending more.
    case outboxFull(limit: Int)
    /// `att-limit`: the tray holds the Mac's per-message count; the next one is refused.
    case attachLimit(limit: Int)
}

/// The two ruled palettes. Raw values are the CLI's and launch arguments' spelling.
public enum Appearance: String, Codable, CaseIterable, Sendable {
    /// "Sovereign", ceo-decisions §14 — the default.
    case dark
    /// "Daybreak", ceo-decisions §15 — switchable, never the default.
    case light
}

/// Limits the views need to state (round-12 `comp-too-long`: "with the number that matters").
public enum Limits {
    /// Characters in one text message.
    public static let messageCharacters = 4000
    /// A voice message's ceiling (30:00) and the warning one minute before it.
    public static let voiceCeilingMs: Int64 = 30 * 60 * 1000
    public static let voiceWarningMs: Int64 = 29 * 60 * 1000
}

/// A settled viewport, saved with user state without saving on every scrolling frame.
public struct ReadingAnchor: Codable, Equatable, Sendable {
    public var messageID: String
    public var offset: Double
    public init(messageID: String, offset: Double) { self.messageID = messageID; self.offset = offset }
}
