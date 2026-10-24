import Foundation
import RichOSCore

/// Everything a person can do on a screen, as the views report it. `action` translates an intent into
/// the core's `Action` — the ONLY way a screen changes the app's state. An intent the core has no
/// action for yet returns `nil` and changes nothing: a screen never pretends something happened.
enum Intent: Equatable, Sendable {
    // Composer
    case compose(String)
    case answerQuestion(id: String, options: [String], text: String, revision: Int?)
    case sendText
    case setComposerFocus(Bool)
    // Voice: the finger and the frame clock, reported; the core's gesture decides (build plan §3.2).
    case voicePress(width: Double, atMs: Int64)
    case voiceMove(dx: Double, dy: Double, atMs: Int64)
    case voiceRelease(atMs: Int64)
    case voiceLockedSend(atMs: Int64)
    case voiceLockedCancel(atMs: Int64)
    /// The system took the touch away (not a release): never a send.
    case voiceTouchCanceled(atMs: Int64)
    case voiceInterrupted(atMs: Int64)
    case voiceTick(atMs: Int64)
    case voiceSettled
    /// VoiceOver's "Record hands-free": start a recording already locked.
    case voiceRecordHandsFree(width: Double, atMs: Int64)
    case sendKept(id: String)
    case discardKept(id: String)
    case playVoice(id: String)
    // Conversation
    case setFollowing(Bool)
    case rememberReading(ReadingAnchor)
    case loadOlder
    case hearReply(id: String)
    case stopReply
    /// A file Rich attached (round 36 screen 3): tapped, its × while it comes over, its viewer closed.
    case openRichFile(id: String)
    case stopRichFile(id: String)
    case closeRichFile(id: String)
    case discard(id: String)
    case retryNow
    case dismissToast
    case dismissCard(id: String)
    case clearFocus
    // Pairing
    case scan
    case scanned(String)
    case cameraDenied
    case closeScanner
    case usePairingLink
    case submitPairingLink(String)
    case confirmWords
    case rejectWords
    case acceptConsent
    case learnMore
    case pairAgain
    case sendWaitingFirst
    case discardAndPair
    case keepPairing
    // Settings
    case openSettings
    case closeSheet
    case openWhereMessagesGo
    case setNotifications(Bool)
    case setPreviews(Bool)
    case openSystemSettings
    case checkForUpdates
    case openSupport
    case openPrivacyPolicy
    case forget
    case confirmForget
    /// "Discard and forget" in the refusal: the way out for a phone whose Mac is gone for good.
    case discardAndForget
    case showWaiting
    case closeDialog
    case setAppearance(Appearance)
    // Notifications
    case turnOnNotifications
    case notificationsNotNow
    // Updates
    case openAppStore
    case updateLater
    case checkAgain
    // Attachments (round-12 attachments; the core's `AttachmentPicking`)
    case openAttachMenu
    case closeAttachMenu
    case openPicker(ScreenModel.Picker)
    case removePending(id: String)
    case jumpTo(id: String)
    case openViewer(messageID: String, index: Int)
    case viewerShow(index: Int)
    case closeViewer

    /// The core action for this intent, where the core has one. `now` stamps time-bearing actions.
    func action(now: Int64 = VoiceClock.nowMs()) -> Action? {
        switch self {
        case .compose(let text): return .compose(text: text)
        case .answerQuestion(let id, let options, let text, let revision): return .answerQuestion(id: id, options: options, text: text, revision: revision, clientID: UUID().uuidString.lowercased(), at: now)
        case .sendText: return .sendDraftNow()
        case .voicePress(let width, let at): return .voicePress(id: Self.recordingID(), width: width, at: at)
        case .voiceRecordHandsFree(let width, let at): return .voiceStartLocked(id: Self.recordingID(), width: width, at: at)
        case .voiceMove(let dx, let dy, let at): return .voiceMove(dx: dx, dy: dy, at: at)
        case .voiceRelease(let at): return .voiceRelease(at: at)
        case .voiceLockedSend(let at): return .voiceLockedSend(at: at)
        case .voiceLockedCancel(let at): return .voiceLockedCancel(at: at)
        case .voiceTouchCanceled(let at): return .voiceTouchCanceled(at: at)
        case .voiceInterrupted(let at): return .voiceInterrupted(at: at)
        case .voiceTick(let at): return .tick(at: at)
        case .voiceSettled: return .voiceSettled
        case .sendKept(let id): return .sendKept(id: id, at: now)
        case .discardKept(let id): return .discardKept(id: id)
        case .playVoice(let id): return .playRecording(id: id)
        case .setComposerFocus(let focused): return .setComposerFocus(focused)
        case .setFollowing(let following): return .setFollowing(following)
        case .rememberReading(let anchor): return .rememberReading(anchor)
        case .loadOlder: return .loadOlder
        case .hearReply(let id): return .hearReply(id: id)
        case .stopReply: return .stopPlayback
        case .openRichFile(let id): return .openRichFile(id: id)
        case .stopRichFile(let id): return .stopRichFile(id: id)
        case .closeRichFile(let id): return .richFileViewed(id: id)
        case .discard(let id): return .discardMessage(id: id)
        case .retryNow: return .retryNow(at: now)
        case .dismissToast: return .dismissToast
        case .clearFocus: return .clearFocus
        case .scan, .pairAgain: return .openScanner
        case .scanned(let text): return .scanned(text: text)
        case .cameraDenied: return .cameraPermission(.denied)
        case .closeScanner: return .closeScanner
        case .usePairingLink: return .openSheet(.pairingLink)
        case .submitPairingLink(let text): return .submitPairingLink(text: text)
        case .confirmWords: return .confirmWords
        case .rejectWords: return .rejectWords
        case .acceptConsent: return .acceptConsent
        case .keepPairing: return .dismissPairingProblem
        case .discardAndPair: return .discardUnsentAndPair
        case .sendWaitingFirst: return .retryNow(at: now)
        case .openSettings: return .openSheet(.settings)
        case .closeSheet, .closeDialog, .showWaiting: return .closeSheet
        case .openWhereMessagesGo, .learnMore: return .openSheet(.whereMessagesGo)
        case .forget: return .forgetPairing
        case .confirmForget: return .confirmForget
        case .discardAndForget: return .discardUnsentAndForget
        case .openSystemSettings: return .openSystemSettings
        case .turnOnNotifications: return .turnOnNotifications
        case .notificationsNotNow: return .dismissNotificationOffer
        case .setNotifications(let on): return on ? .turnOnNotifications : .turnOffNotifications
        case .setPreviews(let on): return .setPreviews(on)
        case .openAppStore: return .openAppStore
        case .openSupport: return .openSupport
        // On iPhone updates come from the App Store: both ask it (App Store listing drafts, blocker 4).
        case .checkForUpdates, .checkAgain: return .checkForUpdates
        case .openPrivacyPolicy: return .openPrivacyPolicy
        case .updateLater: return .dismissUpdate
        case .setAppearance(let appearance): return .setAppearance(appearance)
        case .openAttachMenu: return .openAttachMenu
        case .closeAttachMenu: return .closeAttachMenu
        case .openPicker(let picker):
            switch picker {
            case .photos: return .pickAttachments(.photos)
            case .camera: return .pickAttachments(.camera)
            case .files: return .pickAttachments(.files)
            }
        case .removePending(let id): return .removePendingAttachment(id: id)
        case .dismissCard(let id) where id.hasPrefix("attach-"): return .dismissAttachNotice
        // The microphone-off card's "Not now" (D03). Before, this id fell through to `nil` and the
        // button did nothing.
        case .dismissCard(let id) where id == ScreenModel.Card.microphoneDenied.id: return .dismissMicrophoneCard
        default: return nil
        }
    }

    /// A fresh name for a recording, stamped at the edge like `Action.sendDraftNow()`'s key, so the
    /// reducer stays pure.
    static func recordingID() -> String { UUID().uuidString.lowercased() }
}
