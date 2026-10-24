import Foundation

/// Everything a person — or the platform, or the CLI standing in for either — can do.
///
/// The screens dispatch these and nothing else; `bin/rios headless action '<json>'` dispatches the
/// same values, which is what lets the command line prove a change without a simulator. The JSON
/// spelling is `{"type": "<kebab-case>", …fields}`, the preserved mobile CLI's action envelope
/// (`richos/mobile/cli/mobile.mjs`). Cases arrive with the reducer logic that gives them meaning.
public enum Action: Equatable, Sendable {
    // composer
    case compose(text: String)
    case setAppearance(Appearance)
    // pairing (round-12 group 1)
    case openScanner
    case closeScanner
    /// The QR scanner read this text. It is only ever parsed, never followed.
    case scanned(text: String)
    /// Pasted into the pairing-link sheet.
    case submitPairingLink(text: String)
    /// The OS reports the camera permission (a mirror; nothing parallel is stored).
    case cameraPermission(Permission)
    /// The Mac answered the pairing request (from the transport, or a fixture).
    case pairingAnswered(PairAnswer)
    /// The Mac refused the pairing code (404 after the one retry, contract §2.3).
    case pairingRefused
    /// No Mac answered the pairing request.
    case pairingUnreachable
    /// The Mac answered 200 without `pair-v2`: refused, never fallen back to. The phone has already
    /// signed one "They do not match" so that Mac forgets the key; it now forgets it too.
    case pairingNeedsMacUpdate
    /// "They match" on the six-word check. Pairing v2: the phone then waits for the same press on the Mac.
    case confirmWords
    /// "They do not match" (on the words, or while waiting for the Mac): the Mac forgets this phone
    /// and the phone discards its key (§2.5).
    case rejectWords
    /// What the Mac said about the press ON THE MAC: its answer to one ask of the wait (`MacWait`),
    /// the press on the phone being the first. `at` is when the answer arrived; `askedAt`, when the
    /// ask was sent (`nil` in an older trace: the ask then counts as taking no time).
    case macConfirmation(MacConfirmation, at: Int64, askedAt: Int64? = nil)
    /// Continue on the consent screen (`pair-consent`).
    case acceptConsent
    case dismissPairingProblem
    /// `pair-blocked`'s "Discard and pair": the unsent messages go (never to whichever Mac is paired
    /// next), then the way to pair opens again.
    case discardUnsentAndPair
    // sheets
    case openSheet(Sheet)
    case closeSheet
    // conversation and outbox (groups 2, 3)
    /// Send the draft. `clientID` and `at` are stamped by the caller (`Action.sendDraftNow()` in the
    /// app, fixed values in a scenario), so the reducer stays pure and a trace replays exactly.
    case sendDraft(clientID: String, at: Int64)
    case answerQuestion(id: String, options: [String], text: String, revision: Int?, clientID: String, at: Int64)
    case questionAnswered(clientID: String, question: QuestionCard, at: Int64)
    /// The Mac accepted this message (200, including `duplicate: true`).
    case deliveryAccepted(clientID: String, at: Int64, textSHA256: String? = nil)
    case deliveryFailed(clientID: String, failure: DeliveryFailure, at: Int64)
    /// Time passed (the outbox's retry clock); cheap and never persisted by itself.
    case tick(at: Int64)
    /// "Try now" on the waiting card (`conv-retry`).
    case retryNow(at: Int64)
    /// Remove one of your unsent messages (never one in flight).
    case discardMessage(id: String)
    /// Rows from the live stream or a refresh, merged by id.
    case messagesArrived([Message])
    case replyStarted
    /// The reply's text so far.
    case replyDelta(text: String)
    case replyFinished(Message)
    /// Scrolled to the top: fetch older messages (chunked; never pagination).
    case loadOlder
    case olderLoaded([Message], reachedBeginning: Bool)
    case setFollowing(Bool)
    case rememberReading(ReadingAnchor)
    case setComposerFocus(Bool)
    case openedFromNotification(messageID: String)
    /// A tapped notification for this Mac and conversation, by its reply's reference (SHA-256 hex of
    /// the id). Focuses the reply, fetching older history until it appears (contract §7.3 step 3).
    /// `preview` is the reply's opening text, opened from the notification on the phone (previews on),
    /// and `at` the tap's time: the conversation then shows it at once as a provisional row, and the
    /// stream's full reply replaces it, with no wait on the Mac.
    case openedFromNotificationReference(String, preview: String? = nil, at: Int64 = 0)
    /// A share the Share extension saved, taken into the outbox with its own ids and commit bytes.
    case takeShare(SharedIntake, at: Int64)
    case clearFocus
    /// "Hear it" on one of Rich's replies.
    case hearReply(id: String)
    /// A file Rich attached (round 36 screen 3; Android's `file-open` and its siblings): tapped,
    /// its × while it comes over, how much has come, here and kept, refused, its viewer closed.
    case openRichFile(id: String)
    case stopRichFile(id: String)
    case richFileProgress(id: String, received: Int64)
    case richFileArrived(id: String, path: String)
    case richFileFailed(id: String, reason: String)
    case richFileViewed(id: String)
    /// The phone looked for its own copy of a tapped file and has none: now the Mac is asked.
    case richFileNotKept(id: String)
    case playbackStarted(id: String)
    case playbackProgress(id: String, progress: Double)
    case playbackEnded
    case stopPlayback
    case dismissToast
    // connection (group 7)
    /// The phone's own network came or went (a path monitor in the app).
    case networkChanged(online: Bool, at: Int64)
    /// The live stream or a request to the Mac failed at the transport level.
    case connectionLost(at: Int64)
    /// The Mac answered again (a stream opened, or a request succeeded).
    case connected(at: Int64)
    /// The transport's probe says why the Mac cannot be reached (evidence-based only).
    case connectionDiagnosed(ConnectionNotice)
    /// The OS's word on whether this phone is on Tailscale, read from its interfaces when the network
    /// path changes (D05; Android's `health` `vpn`). Evidence for the one line, never a probe.
    case tunnelChanged(up: Bool)
    /// What the Mac says it accepts (capability negotiation, contract §12).
    case macCapabilities(text: Bool, voice: Bool)
    case macQuestionCapability(Bool)
    /// The Mac offers `audio`: Rich's replies can be heard (`ReplyAudio.hearable`).
    case macAudioCapability(Bool)
    /// The Mac answered 403 `{"revoked":true}`: this phone was removed (round-12 `conn-revoked`).
    case pairingRevoked
    /// What the Mac accepts for attachments (from `hello` or the pairing answer); `nil` = none.
    case macAttachmentLimits(AttachmentLimits?)
    /// The Mac registered this phone for reply notifications; its push host id is kept.
    case pushRegistered(hostID: String?)
    /// The app came to the screen (or launched): reconnect and move the outbox.
    case foregrounded(at: Int64)
    /// The app left the screen: the live stream closes; a recording in progress is kept.
    case backgrounded(at: Int64)
    // voice (groups 4, 5, 6)
    /// Touch-down on the microphone. `id` names the recording (`Action.voicePressNow` in the app).
    case voicePress(id: String, width: Double, at: Int64)
    /// VoiceOver's "record hands-free": start a recording already locked.
    case voiceStartLocked(id: String, width: Double, at: Int64)
    /// The OS answered or reports the microphone permission (a mirror; nothing parallel is stored).
    case microphonePermission(Permission)
    /// The microphone-off card's "Not now": the card goes until the next press finds the microphone
    /// off (D03; Android's `dismiss-microphone-card`).
    case dismissMicrophoneCard
    /// The finger moved: offsets from the touch-down point, in points (negative = left / up).
    case voiceMove(dx: Double, dy: Double, at: Int64)
    case voiceRelease(at: Int64)
    /// Tap on the send circle while locked.
    case voiceLockedSend(at: Int64)
    /// Tap on Cancel while locked.
    case voiceLockedCancel(at: Int64)
    /// The system took the touch away (an alert over the app).
    case voiceTouchCanceled(at: Int64)
    case voiceStartFailed(id: String)
    /// The app left the screen or the OS took the audio: the recording is kept, never sent.
    case voiceInterrupted(at: Int64)
    /// The recording's level, 0 to 1, sampled every 100 ms, for the bubble it becomes.
    case voiceLevel(Double)
    /// The end animation finished.
    case voiceSettled
    case sendKept(id: String, at: Int64)
    case discardKept(id: String)
    /// Play one of your own recordings (a kept one or a sent voice bubble).
    case playRecording(id: String)
    // notifications and settings (groups 8, 9)
    case turnOnNotifications
    /// What the OS and the Mac said to a registration (or its absence).
    case notificationsResult(Notifications.Status)
    case turnOffNotifications
    /// "Not now" on the offer card: it is asked once.
    case dismissNotificationOffer
    case setPreviews(Bool)
    /// "Forget this phone" in Settings: opens the confirmation, or the refusal while work is unsent.
    case forgetPairing
    /// The filled, destructive button in the confirmation.
    case confirmForget
    /// "Discard and forget" in the refusal (`settings-forget-blocked`): the waiting messages go and the
    /// pairing is forgotten, so a phone whose Mac is gone for good can pair again.
    case discardUnsentAndForget
    case openSystemSettings
    // update notices (group 10)
    /// The hosted policy's verdict for this app version (contract §5.10).
    case updatePolicy(UpdateNotice?, voicePaused: Bool)
    case dismissUpdate
    case openAppStore
    case openSupport
    /// Settings, "Check for updates": on iPhone updates come from the App Store, so it opens the listing.
    case checkForUpdates
    /// Settings, "Privacy policy" (Apple 5.1.1(i): a link inside the app).
    case openPrivacyPolicy
    // photos and files (round 12.1 groups 12-17)
    case openAttachMenu
    case closeAttachMenu
    /// A row of the + menu: Photos, Camera or Files.
    case pickAttachments(AttachSource)
    /// What the platform staged from Apple's picker, in the order chosen.
    case attachmentsPicked([OutboxFile])
    /// The platform could not take a chosen item (over the Mac's per-file size, or a type it refuses).
    case attachmentRefused(name: String, bytes: Int?, tooLarge: Bool)
    /// The OS refused the camera (or the photo library).
    case attachPermissionDenied(AttachSource)
    /// The × on a tray item.
    case removePendingAttachment(id: String)
    case dismissAttachNotice
}

/// How a delivery attempt ended when it did not succeed (contract §4.2, the reference classification).
public enum DeliveryFailure: Codable, Equatable, Sendable {
    /// No Mac, a fault, or 429: the same bytes again after `afterMs` (the outbox's own clock when nil).
    case retryable(reason: String?, afterMs: Int64? = nil)
    /// A final answer for this item (409, 413, 422, 503 with `retry:false`): it needs the person; the
    /// rest of the queue carries on.
    case refused(reason: String?)
    /// A final answer about the phone, not the message (a final 404, a 403 that is not a revocation):
    /// this item is blocked and the queue stops.
    case refusedStopQueue(reason: String?)
    /// 403 `{"revoked":true}`: this phone was removed from the Mac.
    case revoked
}

extension Action {
    /// The app's touch-down on the microphone: a fresh recording id and the current time.
    public static func voicePressNow(width: Double, clock: any Clock = SystemClock()) -> Action {
        .voicePress(id: UUID().uuidString.lowercased(), width: width, at: clock.nowMs())
    }

    /// The app's way to send the draft: a fresh idempotency key and the current time.
    public static func sendDraftNow(clock: any Clock = SystemClock()) -> Action {
        .sendDraft(clientID: UUID().uuidString.lowercased(), at: clock.nowMs())
    }
}

/// What `POST /api/pair` answers with (contract §2.3), in the core's terms.
public struct PairAnswer: Codable, Equatable, Sendable {
    public var deviceID: String
    /// The Mac's `ca_fingerprint_sha256`, byte for byte as the Mac sent it (the v2 words hash it as
    /// text, so it is never reformatted).
    public var fingerprintHex: String
    public var threadID: String?
    public var macName: String?
    /// This phone's own public key for the pairing: the 65-byte uncompressed point, base64url without
    /// padding. The v2 words hash it; without it no words can be shown, and nothing is paired.
    public var devicePoint: String?
    /// The Mac's `confirm_within_seconds`: how long the person has to press "They match" on the Mac.
    public var confirmWithinSeconds: Double?
    /// The Mac named `pair-wait` beside `pair-v2`: it can hold the wait's ask until the press on the
    /// Mac (`MacWait`). Optional, so a trace written before it still decodes; absent means no.
    public var offersPairWait: Bool?
    public init(deviceID: String, fingerprintHex: String, threadID: String? = nil, macName: String? = nil,
                devicePoint: String? = nil, confirmWithinSeconds: Double? = nil, offersPairWait: Bool? = nil) {
        self.deviceID = deviceID; self.fingerprintHex = fingerprintHex; self.threadID = threadID; self.macName = macName
        self.devicePoint = devicePoint; self.confirmWithinSeconds = confirmWithinSeconds; self.offersPairWait = offersPairWait
    }
}

/// Work a reducer asks for and does not do itself. The reducer stays pure; the effect runner
/// performs these through the ports, so every one can be substituted in a test. An effect with no
/// port wired yet is recorded and skipped, never guessed at.
public enum Effect: Equatable, Sendable {
    /// Write `AppState.persisted` to durable storage.
    case persist
    /// Generate (or reuse) this origin's device key and send `POST /api/pair` with the code.
    case pair(PairLink)
    /// Send the signed six-word answer (contract §2.5). The reducer sends it only for "They do not
    /// match"; "They match" is the wait's first ask (`checkMacConfirmation`), whose answer comes back
    /// as `macConfirmation`.
    case confirmFingerprint(matches: Bool)
    /// One ask of the wait for the press on the Mac (`MacWait`, `pairing.json` `pair_wait`): the
    /// phone's own signed "They match" again, byte for byte, with `Prefer: wait=<waitSeconds>` when
    /// `waitSeconds` is above 0 (only to a Mac that offers `pair-wait`). Only ever sent on screen;
    /// leaving the screen cancels it.
    case checkMacConfirmation(waitSeconds: Int)
    /// Discard this phone's key for that origin.
    case forgetIdentity(origin: String)
    /// Open this app's page in iPhone Settings (camera or microphone turned off).
    case openSystemSettings
    /// POST the outbox item's exact bytes (contract §5.2).
    case deliver(clientID: String)
    /// Fetch older history before this message (contract §5.5).
    case loadOlder(before: String?)
    /// Fetch and play the reply's audio (contract §5.6).
    case fetchReplyAudio(messageID: String)
    /// Bring a file Rich attached over from the Mac (signed `GET /api/messages?kind=file`) and keep
    /// it; its progress and outcome come back as `richFileProgress`, `richFileArrived`, `richFileFailed`.
    case fetchRichFile(id: String, threadID: String, name: String, total: Int64?)
    /// A tapped file's own copy on this phone, looked for first, before the Mac is needed (a kept
    /// copy opens with the Mac offline or the original deleted): `richFileArrived` or `richFileNotKept`.
    case findRichFile(id: String, name: String)
    /// The × on a file coming over: stop it, keep nothing.
    case stopRichFile(id: String)
    case stopAudio
    /// Open (or keep) the live connection to the paired Mac.
    case connect
    /// The phone's network path changed with no loss reported: the live connection's owner replaces
    /// whatever it is doing (an open stream, an open in flight, a back-off wait) with a new open, now.
    case reconnect
    /// Close it (backgrounded, revoked, forgotten).
    case disconnect
    /// Ask the OS for the microphone (once, on the first deliberate press).
    case requestMicrophone
    case startRecording(id: String)
    /// Stop capturing; `keep` retains the file (sent or kept), otherwise it is removed.
    case stopRecording(id: String, keep: Bool)
    case deleteRecording(id: String)
    /// Remove the phone's copies of attached files (the Mac accepted the message, or it was discarded).
    case deleteAttachments(paths: [String])
    /// The light tick when the lock engages (`UIImpactFeedbackGenerator(.light)`).
    case hapticTick
    case playRecording(id: String)
    /// Ask the OS for notification permission and register with the Mac (native push, contract §7.2).
    case requestNotifications(previews: Bool)
    case unregisterNotifications
    /// Remove every RichOS reply notification still in Notification Center: notifications were turned
    /// off, or the Mac was forgotten (D04).
    case withdrawNotifications
    case openAppStore
    case openSupport
    case openPrivacyPolicy
    /// Present Apple's photo picker, camera or document picker, for at most `maxCount` items.
    case presentPicker(AttachSource, maxCount: Int)
}

/// The one place state changes. Pure: the same state and action always give the same result, so a
/// scenario replays exactly on this Mac and in the simulator.
public enum Reducer {
    public static func reduce(_ state: AppState, _ action: Action) -> (state: AppState, effects: [Effect]) {
        var next = state
        var effects: [Effect] = []
        switch action {
        case .compose(let text):
            next.draft = text
        case .setAppearance(let appearance):
            next.appearance = appearance
        case .openSheet(let sheet):
            next.sheet = sheet
        case .closeSheet:
            next.sheet = nil
        case .openScanner, .closeScanner, .scanned, .submitPairingLink, .cameraPermission, .pairingAnswered,
             .pairingRefused, .pairingUnreachable, .pairingNeedsMacUpdate, .confirmWords, .rejectWords, .macConfirmation,
             .acceptConsent, .dismissPairingProblem, .discardUnsentAndPair:
            PairingReducer.reduce(&next, action, &effects)
        case .networkChanged, .connectionLost, .connected, .connectionDiagnosed, .tunnelChanged, .macCapabilities, .macQuestionCapability, .macAudioCapability, .pairingRevoked,
             .macAttachmentLimits, .pushRegistered:
            ConnectionReducer.reduce(&next, action, &effects)
        case .foregrounded, .backgrounded:
            // The conversation first: a return clears the outbox's back-off before the connection's
            // pump looks at it (a stream that stayed open through an inactive spell pumps at once).
            ConversationReducer.reduce(&next, action, &effects)
            PairingReducer.reduce(&next, action, &effects)
            ConnectionReducer.reduce(&next, action, &effects)
            VoiceReducer.reduce(&next, action, &effects)
        case .tick:
            PairingReducer.reduce(&next, action, &effects)
            ConnectionReducer.reduce(&next, action, &effects)
            ConversationReducer.reduce(&next, action, &effects)
            VoiceReducer.reduce(&next, action, &effects)
        case .voicePress, .voiceStartLocked, .microphonePermission, .dismissMicrophoneCard, .voiceMove, .voiceRelease, .voiceLockedSend, .voiceLockedCancel,
             .voiceTouchCanceled, .voiceStartFailed, .voiceInterrupted, .voiceLevel, .voiceSettled, .sendKept, .discardKept, .playRecording:
            VoiceReducer.reduce(&next, action, &effects)
        case .turnOnNotifications, .notificationsResult, .turnOffNotifications, .dismissNotificationOffer, .setPreviews,
             .forgetPairing, .confirmForget, .discardUnsentAndForget, .openSystemSettings:
            SettingsReducer.reduce(&next, action, &effects)
        case .updatePolicy, .dismissUpdate, .openAppStore, .openSupport, .checkForUpdates, .openPrivacyPolicy:
            UpdateReducer.reduce(&next, action, &effects)
        case .openAttachMenu, .closeAttachMenu, .pickAttachments, .attachmentsPicked, .attachmentRefused, .attachPermissionDenied,
             .removePendingAttachment, .dismissAttachNotice:
            ConversationReducer.reduceAttachments(&next, action, &effects)
        default:
            ConversationReducer.reduce(&next, action, &effects)
        }
        // Whatever closes the stream (leaving the screen, no network, revocation, forgetting the Mac)
        // closes the link the outbox moves on.
        if effects.contains(.disconnect) { next.linkOpen = false }
        // Persist only when something durable changed: a touch-rate action never writes a file.
        if next.persisted != state.persisted { effects.insert(.persist, at: 0) }
        return (next, effects)
    }
}
