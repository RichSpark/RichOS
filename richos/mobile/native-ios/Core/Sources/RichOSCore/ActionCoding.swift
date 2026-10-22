import Foundation

/// The JSON spelling of every `Action` — the command line's and the Debug bridge's language.
///
/// Where the preserved mobile CLI already names an action (`richos/mobile/core/app.js`,
/// `core/client.js`), that name and its field names are used exactly — `send`, `network`
/// (`online`), `retry`, `discard` (`clientId`), `pair` (`link`), `scan-pair`, `confirm-pair`
/// (`matched`), `forget-pair` (`confirm`), `older`, `notifications-enable|disable|previews`
/// (`enabled`), `reply-play`, `record-play`, `playback-stop`, `playback-ended`, `update-dismiss`,
/// `update-open`, `support`, `settings` — so QA's habits and the Android core (Rich's decision,
/// build plan §9 item 4) share one grammar. The voice gesture is new; its names are the standard.
/// Also new: `share-take` (`intake`, a share from the Share extension) and `notification-open` with
/// `event` instead of `id` (the reply's SHA-256 reference, for a reply not loaded yet). Pairing v2
/// adds `pairing-needs-mac-update` and `mac-confirmation` (`confirmation`: `confirmed`, `awaiting` or
/// `refused`, with `at` and optionally `askedAt`), the Mac's word on the press ON THE MAC.
///
/// Time and idempotency stamps (`at`, `clientId` on `send`) are optional on the wire: omitted, they
/// are stamped from the clock when the command arrives; given, a scenario replays exactly.
extension Action: Codable {
    private struct Wire: Codable {
        var type: String
        var anchor: ReadingAnchor?
        var text: String?
        var link: String?
        var appearance: Appearance?
        var permission: Permission?
        var sheet: Sheet?
        var answer: PairAnswer?
        var options: [String]?
        var revision: Int?
        var question: QuestionCard?
        var clientId: String?
        var textSHA256: String?
        var id: String?
        var at: Int64?
        var failure: DeliveryFailure?
        var messages: [Message]?
        var message: Message?
        var reachedBeginning: Bool?
        var value: Bool?
        var online: Bool?
        var vpn: Bool?
        var matched: Bool?
        var confirm: Bool?
        var enabled: Bool?
        var progress: Double?
        var notice: ConnectionNotice?
        var acceptsText: Bool?
        var acceptsVoice: Bool?
        var acceptsQuestions: Bool?
        var acceptsAudio: Bool?
        var width: Double?
        var dx: Double?
        var dy: Double?
        var level: Double?
        var status: Notifications.Status?
        var update: UpdateNotice?
        var voicePaused: Bool?
        var limits: AttachmentLimits?
        var hostId: String?
        var intake: SharedIntake?
        var event: String?
        var source: AttachSource?
        var files: [OutboxFile]?
        var name: String?
        var bytes: Int?
        var tooLarge: Bool?
        var confirmation: MacConfirmation?
        var askedAt: Int64?
        var received: Int64?
        var path: String?
        var reason: String?

        init(_ type: String) { self.type = type }
    }

    public static let knownTypes = [
        "compose", "set-appearance",
        "scan-pair", "close-scanner", "scanned", "pair", "camera-permission", "pairing-answered", "pairing-refused", "pairing-unreachable",
        "pairing-needs-mac-update", "confirm-pair", "mac-confirmation", "accept-consent", "dismiss-pairing-problem", "discard-and-pair", "open-sheet", "close-sheet",
        "send", "answer-question", "question-answered", "delivery-accepted", "delivery-failed", "tick", "retry", "discard", "messages-arrived",
        "reply-started", "reply-delta", "reply-finished", "older", "older-loaded", "remember-reading", "set-following", "set-composer-focus",
        "notification-open", "notification-focused", "share-take", "reply-play", "playback-started", "playback-progress",
        "file-open", "file-stop", "file-progress", "file-arrived", "file-failed", "file-viewed", "file-not-kept",
        "playback-ended", "playback-stop", "dismiss-toast",
        "network", "health", "connection-lost", "connected", "connection-diagnosed", "mac-capabilities", "mac-question-capability", "mac-audio-capability", "pairing-revoked", "foregrounded", "backgrounded", "mac-attachment-limits", "push-registered",
        "voice-press", "voice-start-locked", "microphone-permission", "dismiss-microphone-card", "voice-move", "voice-release", "voice-locked-send",
        "voice-locked-cancel", "voice-touch-canceled", "voice-interrupted", "voice-level", "voice-settled",
        "send-kept", "discard-kept", "record-play",
        "notifications-enable", "notifications-result", "notifications-disable", "dismiss-notification-offer",
        "notifications-previews", "forget-request", "forget-pair", "discard-and-forget", "settings",
        "update-policy", "update-dismiss", "update-open", "support", "check-updates", "privacy-policy",
        "attach-menu-open", "attach-menu-close", "attach-pick", "attach-picked", "attach-refused", "attach-denied",
        "attach-remove", "attach-notice-dismiss",
    ]

    public init(from decoder: Decoder) throws {
        let w = try Wire(from: decoder)
        func need<T>(_ value: T?, _ field: String) throws -> T {
            guard let value else {
                throw DecodingError.dataCorrupted(.init(codingPath: [], debugDescription: "action '\(w.type)' needs '\(field)'"))
            }
            return value
        }
        // Omitted stamps are taken now; given ones make a replay exact.
        let now = w.at ?? SystemClock().nowMs()
        switch w.type {
        case "compose": self = .compose(text: try need(w.text, "text"))
        case "set-appearance": self = .setAppearance(try need(w.appearance, "appearance"))
        case "scan-pair": self = .openScanner
        case "close-scanner": self = .closeScanner
        case "scanned": self = .scanned(text: try need(w.text, "text"))
        case "pair": self = .submitPairingLink(text: try need(w.link ?? w.text, "link"))
        case "camera-permission": self = .cameraPermission(try need(w.permission, "permission"))
        case "pairing-answered": self = .pairingAnswered(try need(w.answer, "answer"))
        case "pairing-refused": self = .pairingRefused
        case "pairing-unreachable": self = .pairingUnreachable
        case "pairing-needs-mac-update": self = .pairingNeedsMacUpdate
        case "confirm-pair": self = try need(w.matched, "matched") ? .confirmWords : .rejectWords
        case "mac-confirmation": self = .macConfirmation(try need(w.confirmation, "confirmation"), at: now, askedAt: w.askedAt)
        case "accept-consent": self = .acceptConsent
        case "dismiss-pairing-problem": self = .dismissPairingProblem
        case "discard-and-pair": self = .discardUnsentAndPair
        case "open-sheet": self = .openSheet(try need(w.sheet, "sheet"))
        case "close-sheet": self = .closeSheet
        case "answer-question": self = .answerQuestion(id: try need(w.id, "id"), options: w.options ?? [], text: w.text ?? "", revision: w.revision, clientID: w.clientId ?? UUID().uuidString.lowercased(), at: now)
        case "question-answered": self = .questionAnswered(clientID: try need(w.clientId, "clientId"), question: try need(w.question, "question"), at: now)
        case "send": self = .sendDraft(clientID: w.clientId ?? UUID().uuidString.lowercased(), at: now)
        case "delivery-accepted": self = .deliveryAccepted(clientID: try need(w.clientId, "clientId"), at: now, textSHA256: w.textSHA256)
        case "delivery-failed": self = .deliveryFailed(clientID: try need(w.clientId, "clientId"), failure: try need(w.failure, "failure"), at: now)
        case "tick": self = .tick(at: now)
        case "retry": self = .retryNow(at: now)
        case "discard": self = .discardMessage(id: try need(w.clientId ?? w.id, "clientId"))
        case "messages-arrived": self = .messagesArrived(try need(w.messages, "messages"))
        case "reply-started": self = .replyStarted
        case "reply-delta": self = .replyDelta(text: try need(w.text, "text"))
        case "reply-finished": self = .replyFinished(try need(w.message, "message"))
        case "older": self = .loadOlder
        case "older-loaded": self = .olderLoaded(try need(w.messages, "messages"), reachedBeginning: try need(w.reachedBeginning, "reachedBeginning"))
        case "remember-reading": self = .rememberReading(try need(w.anchor, "anchor"))
        case "set-following": self = .setFollowing(try need(w.value, "value"))
        case "set-composer-focus": self = .setComposerFocus(try need(w.value, "value"))
        case "notification-open":
            if let event = w.event, w.id == nil { self = .openedFromNotificationReference(event, preview: w.text, at: w.at ?? 0) } else {
                self = .openedFromNotification(messageID: try need(w.id, "id"))
            }
        case "share-take": self = .takeShare(try need(w.intake, "intake"), at: w.at ?? now)
        case "notification-focused": self = .clearFocus
        case "reply-play": self = .hearReply(id: try need(w.id, "id"))
        case "file-open": self = .openRichFile(id: try need(w.id, "id"))
        case "file-stop": self = .stopRichFile(id: try need(w.id, "id"))
        case "file-progress": self = .richFileProgress(id: try need(w.id, "id"), received: try need(w.received, "received"))
        case "file-arrived": self = .richFileArrived(id: try need(w.id, "id"), path: try need(w.path, "path"))
        case "file-failed": self = .richFileFailed(id: try need(w.id, "id"), reason: try need(w.reason, "reason"))
        case "file-viewed": self = .richFileViewed(id: try need(w.id, "id"))
        case "file-not-kept": self = .richFileNotKept(id: try need(w.id, "id"))
        case "playback-started": self = .playbackStarted(id: try need(w.id, "id"))
        case "playback-progress": self = .playbackProgress(id: try need(w.id, "id"), progress: try need(w.progress, "progress"))
        case "playback-ended": self = .playbackEnded
        case "playback-stop": self = .stopPlayback
        case "dismiss-toast": self = .dismissToast
        case "network": self = .networkChanged(online: try need(w.online, "online"), at: now)
        case "health": self = .tunnelChanged(up: try need(w.vpn, "vpn"))
        case "connection-lost": self = .connectionLost(at: now)
        case "connected": self = .connected(at: now)
        case "connection-diagnosed": self = .connectionDiagnosed(try need(w.notice, "notice"))
        case "mac-question-capability": self = .macQuestionCapability(try need(w.acceptsQuestions, "acceptsQuestions"))
        case "mac-audio-capability": self = .macAudioCapability(try need(w.acceptsAudio, "acceptsAudio"))
        case "mac-capabilities": self = .macCapabilities(text: try need(w.acceptsText, "acceptsText"), voice: try need(w.acceptsVoice, "acceptsVoice"))
        case "pairing-revoked": self = .pairingRevoked
        case "mac-attachment-limits": self = .macAttachmentLimits(w.limits)
        case "push-registered": self = .pushRegistered(hostID: w.hostId)
        case "foregrounded": self = .foregrounded(at: now)
        case "backgrounded": self = .backgrounded(at: now)
        case "voice-press": self = .voicePress(id: w.id ?? UUID().uuidString.lowercased(), width: try need(w.width, "width"), at: now)
        case "voice-start-locked": self = .voiceStartLocked(id: w.id ?? UUID().uuidString.lowercased(), width: try need(w.width, "width"), at: now)
        case "microphone-permission": self = .microphonePermission(try need(w.permission, "permission"))
        case "dismiss-microphone-card": self = .dismissMicrophoneCard
        case "voice-move": self = .voiceMove(dx: try need(w.dx, "dx"), dy: try need(w.dy, "dy"), at: now)
        case "voice-release": self = .voiceRelease(at: now)
        case "voice-locked-send": self = .voiceLockedSend(at: now)
        case "voice-locked-cancel": self = .voiceLockedCancel(at: now)
        case "voice-touch-canceled": self = .voiceTouchCanceled(at: now)
        case "voice-start-failed": self = .voiceStartFailed(id: try need(w.id, "id"))
        case "voice-interrupted": self = .voiceInterrupted(at: now)
        case "voice-level": self = .voiceLevel(try need(w.level, "level"))
        case "voice-settled": self = .voiceSettled
        case "send-kept": self = .sendKept(id: try need(w.id, "id"), at: now)
        case "discard-kept": self = .discardKept(id: try need(w.id, "id"))
        case "record-play": self = .playRecording(id: try need(w.id, "id"))
        case "notifications-enable": self = .turnOnNotifications
        case "notifications-result": self = .notificationsResult(try need(w.status, "status"))
        case "notifications-disable": self = .turnOffNotifications
        case "dismiss-notification-offer": self = .dismissNotificationOffer
        case "notifications-previews": self = .setPreviews(try need(w.enabled, "enabled"))
        case "forget-request": self = .forgetPairing
        case "forget-pair":
            // The preserved grammar requires the explicit confirmation, and so does this core.
            guard try need(w.confirm, "confirm") else {
                throw DecodingError.dataCorrupted(.init(codingPath: [], debugDescription: "forget-pair needs \"confirm\": true"))
            }
            self = .confirmForget
        case "discard-and-forget": self = .discardUnsentAndForget
        case "settings": self = .openSystemSettings
        case "update-policy": self = .updatePolicy(w.update, voicePaused: try need(w.voicePaused, "voicePaused"))
        case "update-dismiss": self = .dismissUpdate
        case "update-open": self = .openAppStore
        case "support": self = .openSupport
        case "check-updates": self = .checkForUpdates
        case "privacy-policy": self = .openPrivacyPolicy
        case "attach-menu-open": self = .openAttachMenu
        case "attach-menu-close": self = .closeAttachMenu
        case "attach-pick": self = .pickAttachments(try need(w.source, "source"))
        case "attach-picked": self = .attachmentsPicked(try need(w.files, "files"))
        case "attach-refused": self = .attachmentRefused(name: try need(w.name, "name"), bytes: w.bytes, tooLarge: try need(w.tooLarge, "tooLarge"))
        case "attach-denied": self = .attachPermissionDenied(try need(w.source, "source"))
        case "attach-remove": self = .removePendingAttachment(id: try need(w.id, "id"))
        case "attach-notice-dismiss": self = .dismissAttachNotice
        default:
            throw DecodingError.dataCorrupted(.init(codingPath: [], debugDescription:
                "unknown action '\(w.type)'; known: \(Self.knownTypes.joined(separator: ", "))"))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var w: Wire
        switch self {
        case .compose(let text): w = Wire("compose"); w.text = text
        case .setAppearance(let a): w = Wire("set-appearance"); w.appearance = a
        case .openScanner: w = Wire("scan-pair")
        case .closeScanner: w = Wire("close-scanner")
        case .scanned(let text): w = Wire("scanned"); w.text = text
        case .submitPairingLink(let link): w = Wire("pair"); w.link = link
        case .cameraPermission(let p): w = Wire("camera-permission"); w.permission = p
        case .pairingAnswered(let a): w = Wire("pairing-answered"); w.answer = a
        case .pairingRefused: w = Wire("pairing-refused")
        case .pairingUnreachable: w = Wire("pairing-unreachable")
        case .pairingNeedsMacUpdate: w = Wire("pairing-needs-mac-update")
        case .confirmWords: w = Wire("confirm-pair"); w.matched = true
        case .rejectWords: w = Wire("confirm-pair"); w.matched = false
        case .macConfirmation(let c, let at, let askedAt): w = Wire("mac-confirmation"); w.confirmation = c; w.at = at; w.askedAt = askedAt
        case .acceptConsent: w = Wire("accept-consent")
        case .dismissPairingProblem: w = Wire("dismiss-pairing-problem")
        case .discardUnsentAndPair: w = Wire("discard-and-pair")
        case .openSheet(let s): w = Wire("open-sheet"); w.sheet = s
        case .closeSheet: w = Wire("close-sheet")
        case .answerQuestion(let id, let options, let text, let revision, let c, let at): w = Wire("answer-question"); w.id = id; w.options = options; w.text = text; w.revision = revision; w.clientId = c; w.at = at
        case .questionAnswered(let c, let q, let at): w = Wire("question-answered"); w.clientId = c; w.question = q; w.at = at
        case .sendDraft(let c, let at): w = Wire("send"); w.clientId = c; w.at = at
        case .deliveryAccepted(let c, let at, let hash): w = Wire("delivery-accepted"); w.clientId = c; w.at = at; w.textSHA256 = hash
        case .deliveryFailed(let c, let f, let at): w = Wire("delivery-failed"); w.clientId = c; w.failure = f; w.at = at
        case .tick(let at): w = Wire("tick"); w.at = at
        case .retryNow(let at): w = Wire("retry"); w.at = at
        case .discardMessage(let id): w = Wire("discard"); w.clientId = id
        case .messagesArrived(let m): w = Wire("messages-arrived"); w.messages = m
        case .replyStarted: w = Wire("reply-started")
        case .replyDelta(let text): w = Wire("reply-delta"); w.text = text
        case .replyFinished(let m): w = Wire("reply-finished"); w.message = m
        case .loadOlder: w = Wire("older")
        case .olderLoaded(let m, let r): w = Wire("older-loaded"); w.messages = m; w.reachedBeginning = r
        case .rememberReading(let anchor): w = Wire("remember-reading"); w.anchor = anchor
        case .setFollowing(let v): w = Wire("set-following"); w.value = v
        case .setComposerFocus(let v): w = Wire("set-composer-focus"); w.value = v
        case .openedFromNotification(let id): w = Wire("notification-open"); w.id = id
        case .openedFromNotificationReference(let event, let preview, let at):
            w = Wire("notification-open"); w.event = event; w.text = preview; w.at = preview == nil ? nil : at
        case .takeShare(let intake, let at): w = Wire("share-take"); w.intake = intake; w.at = at
        case .clearFocus: w = Wire("notification-focused")
        case .hearReply(let id): w = Wire("reply-play"); w.id = id
        case .openRichFile(let id): w = Wire("file-open"); w.id = id
        case .stopRichFile(let id): w = Wire("file-stop"); w.id = id
        case .richFileProgress(let id, let received): w = Wire("file-progress"); w.id = id; w.received = received
        case .richFileArrived(let id, let path): w = Wire("file-arrived"); w.id = id; w.path = path
        case .richFileFailed(let id, let reason): w = Wire("file-failed"); w.id = id; w.reason = reason
        case .richFileViewed(let id): w = Wire("file-viewed"); w.id = id
        case .richFileNotKept(let id): w = Wire("file-not-kept"); w.id = id
        case .playbackStarted(let id): w = Wire("playback-started"); w.id = id
        case .playbackProgress(let id, let p): w = Wire("playback-progress"); w.id = id; w.progress = p
        case .playbackEnded: w = Wire("playback-ended")
        case .stopPlayback: w = Wire("playback-stop")
        case .dismissToast: w = Wire("dismiss-toast")
        case .networkChanged(let online, let at): w = Wire("network"); w.online = online; w.at = at
        case .tunnelChanged(let up): w = Wire("health"); w.vpn = up
        case .connectionLost(let at): w = Wire("connection-lost"); w.at = at
        case .connected(let at): w = Wire("connected"); w.at = at
        case .connectionDiagnosed(let n): w = Wire("connection-diagnosed"); w.notice = n
        case .macQuestionCapability(let v): w = Wire("mac-question-capability"); w.acceptsQuestions = v
        case .macAudioCapability(let v): w = Wire("mac-audio-capability"); w.acceptsAudio = v
        case .macCapabilities(let t, let v): w = Wire("mac-capabilities"); w.acceptsText = t; w.acceptsVoice = v
        case .pairingRevoked: w = Wire("pairing-revoked")
        case .macAttachmentLimits(let l): w = Wire("mac-attachment-limits"); w.limits = l
        case .pushRegistered(let h): w = Wire("push-registered"); w.hostId = h
        case .foregrounded(let at): w = Wire("foregrounded"); w.at = at
        case .backgrounded(let at): w = Wire("backgrounded"); w.at = at
        case .voicePress(let id, let width, let at): w = Wire("voice-press"); w.id = id; w.width = width; w.at = at
        case .voiceStartLocked(let id, let width, let at): w = Wire("voice-start-locked"); w.id = id; w.width = width; w.at = at
        case .microphonePermission(let p): w = Wire("microphone-permission"); w.permission = p
        case .dismissMicrophoneCard: w = Wire("dismiss-microphone-card")
        case .voiceMove(let dx, let dy, let at): w = Wire("voice-move"); w.dx = dx; w.dy = dy; w.at = at
        case .voiceRelease(let at): w = Wire("voice-release"); w.at = at
        case .voiceLockedSend(let at): w = Wire("voice-locked-send"); w.at = at
        case .voiceLockedCancel(let at): w = Wire("voice-locked-cancel"); w.at = at
        case .voiceTouchCanceled(let at): w = Wire("voice-touch-canceled"); w.at = at
        case .voiceStartFailed(let id): w = Wire("voice-start-failed"); w.id = id
        case .voiceInterrupted(let at): w = Wire("voice-interrupted"); w.at = at
        case .voiceLevel(let l): w = Wire("voice-level"); w.level = l
        case .voiceSettled: w = Wire("voice-settled")
        case .sendKept(let id, let at): w = Wire("send-kept"); w.id = id; w.at = at
        case .discardKept(let id): w = Wire("discard-kept"); w.id = id
        case .playRecording(let id): w = Wire("record-play"); w.id = id
        case .turnOnNotifications: w = Wire("notifications-enable")
        case .notificationsResult(let st): w = Wire("notifications-result"); w.status = st
        case .turnOffNotifications: w = Wire("notifications-disable")
        case .dismissNotificationOffer: w = Wire("dismiss-notification-offer")
        case .setPreviews(let v): w = Wire("notifications-previews"); w.enabled = v
        case .forgetPairing: w = Wire("forget-request")
        case .confirmForget: w = Wire("forget-pair"); w.confirm = true
        case .discardUnsentAndForget: w = Wire("discard-and-forget")
        case .openSystemSettings: w = Wire("settings")
        case .updatePolicy(let n, let paused): w = Wire("update-policy"); w.update = n; w.voicePaused = paused
        case .dismissUpdate: w = Wire("update-dismiss")
        case .openAppStore: w = Wire("update-open")
        case .openSupport: w = Wire("support")
        case .checkForUpdates: w = Wire("check-updates")
        case .openPrivacyPolicy: w = Wire("privacy-policy")
        case .openAttachMenu: w = Wire("attach-menu-open")
        case .closeAttachMenu: w = Wire("attach-menu-close")
        case .pickAttachments(let source): w = Wire("attach-pick"); w.source = source
        case .attachmentsPicked(let files): w = Wire("attach-picked"); w.files = files
        case .attachmentRefused(let name, let bytes, let tooLarge): w = Wire("attach-refused"); w.name = name; w.bytes = bytes; w.tooLarge = tooLarge
        case .attachPermissionDenied(let source): w = Wire("attach-denied"); w.source = source
        case .removePendingAttachment(let id): w = Wire("attach-remove"); w.id = id
        case .dismissAttachNotice: w = Wire("attach-notice-dismiss")
        }
        try w.encode(to: encoder)
    }
}
