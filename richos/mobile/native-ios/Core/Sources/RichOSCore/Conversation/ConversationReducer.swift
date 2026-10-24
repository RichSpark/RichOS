import CryptoKit
import Foundation

/// The conversation and the outbox, round-12 groups 2 and 3.
///
/// The outbox rules are the reference queue's (`richos/web/web-app/lib/queue.js`, phone protocol
/// contract §6.4), each one a test: persisted before the bubble shows; strict first-in first-out,
/// one in flight, and the flush STOPS at the first retryable failure; a final refusal blocks that
/// item only and the queue continues; retries wait 1 s doubling to 16 s; the exact body bytes are
/// fixed at send and resent unchanged; nothing is attempted while the phone is offline.
public enum ConversationReducer {
    public static let firstRetryMs: Int64 = 1000
    public static let maxRetryMs: Int64 = 16000
    /// The reference client's cap (`richos/mobile/core/client.js`).
    public static let outboxLimit = 100

    public static func retryDelayMs(attempt: Int) -> Int64 {
        min(firstRetryMs << Int64(max(0, attempt - 1)), maxRetryMs)
    }

    /// Retryable reasons that are the phone's own deferral (leaving the screen, the background
    /// budget, storage, a lost stream), not a failed attempt at the Mac: they add no back-off.
    static let deferralReasons: Set<String> = ["background", "background-budget", "local-storage", "link-lost"]

    /// The reason a delivery answered 429 carries (`Courier.reason(for:)`): the wait is the Mac's own
    /// (`Retry-After`), not the phone's back-off, so a return to the screen keeps it.
    public static let rateLimitedReason = "rate-limited"

    /// **BACK ON SCREEN, A WAITING MESSAGE GOES AT ONCE.** A message whose sends failed waits out the
    /// back-off stamped at its last failure (`notBefore`, 1 s doubling to 16 s), and the return did not
    /// clear it: the stream opened and `connected` pumped, but the head stayed put until its time, up to
    /// 16 s after he came back (andy-opus-resume1's first Android wait, the same rule here). The
    /// wait is cleared, never the count of failures, so while the app stays on screen and failing the
    /// next failure waits the next step as before. A wait the Mac asked for (429 `Retry-After`) stays.
    static func resumeAfterReturn(_ s: inout AppState, at: Int64) {
        for i in s.outbox.indices where s.outbox[i].state == .waiting && s.outbox[i].notBefore > at
            && s.outbox[i].lastReason != rateLimitedReason {
            s.outbox[i].notBefore = 0
        }
    }

    static func reduce(_ s: inout AppState, _ action: Action, _ effects: inout [Effect]) {
        switch action {
        case .answerQuestion(let id, let options, let text, let revision, let clientID, let at):
            questionAnswer(&s, id: id, options: options, text: text, revision: revision, clientID: clientID, at: at, &effects)
        case .questionAnswered(let clientID, let q, let at):
            if let i = s.messages.firstIndex(where: { $0.question?.id == q.id }) { s.messages[i].question = q }
            reduce(&s, .deliveryAccepted(clientID: clientID, at: at), &effects)
        case .sendDraft(let clientID, let at):
            send(&s, clientID: clientID, at: at, &effects)
        case .deliveryAccepted(let clientID, let at, let transcriptHash):
            guard let i = s.outbox.firstIndex(where: { $0.clientID == clientID }) else { return }
            if s.outbox[i].kind == .voice, let transcriptHash,
               transcriptHash.utf8.count == 64, transcriptHash.allSatisfy({ $0.isHexDigit && $0.isASCII }),
               let local = s.messages.firstIndex(where: { $0.id == clientID }) {
                s.messages[local].transcriptSHA256 = transcriptHash.lowercased()
            }
            releaseFiles(of: s.outbox.remove(at: i), &effects)
            setDelivery(&s, clientID, nil)
            reconcile(&s)
            // The Mac just answered: the batch goes on, as Android's drain loop does inside one flush,
            // including the bounded completion after the app leaves the screen (PRD §6).
            pump(&s, at: at, &effects, answered: true)
        case .deliveryFailed(let clientID, let failure, let at):
            guard let i = s.outbox.firstIndex(where: { $0.clientID == clientID }) else { return }
            switch failure {
            case .retryable(let reason, let afterMs):
                // Lifecycle/storage deferral is not a failed network attempt. Do not make the
                // next real transient failure wait longer merely because the user switched apps.
                // Nor does a request given up because the Mac's stream was lost (`link-lost`): it
                // goes again the moment the stream reopens.
                if !deferralReasons.contains(reason ?? "") {
                    s.outbox[i].attempts += 1
                }
                s.outbox[i].state = .waiting
                s.outbox[i].notBefore = at + (afterMs ?? retryDelayMs(attempt: s.outbox[i].attempts))
                s.outbox[i].lastReason = reason
                setDelivery(&s, clientID, .waiting)
            case .refused(let reason):
                s.outbox[i].state = .blocked
                s.outbox[i].lastReason = reason
                setDelivery(&s, clientID, .needsAttention)
                pump(&s, at: at, &effects, answered: true)
            case .refusedStopQueue(let reason):
                s.outbox[i].state = .blocked
                s.outbox[i].lastReason = reason
                setDelivery(&s, clientID, .needsAttention)
            case .revoked:
                // Final for the pairing, not for his words: they stay on the phone ("Nothing here
                // was lost", round-12 `conn-revoked`).
                s.outbox[i].state = .waiting
                setDelivery(&s, clientID, .waiting)
                s.pairing = .revoked
            }
        case .tick(let at):
            pump(&s, at: at, &effects)
        case .retryNow(let at):
            for i in s.outbox.indices where s.outbox[i].state == .waiting { s.outbox[i].notBefore = 0 }
            // "Send it first" in `pair-blocked` is this action: the choice is made, the dialog goes.
            if case .blockedByUnsentWork = s.pairingProblem { s.pairingProblem = nil }
            // "Try now" with the Mac out of reach: the one connection owner tries now, skipping what is
            // left of its back-off (never resetting it); the message goes once the Mac answers.
            if s.pairing == .paired, !s.linkOpen, s.connectionNotice != .phoneOffline, s.connectionNotice != .incompatible {
                effects.append(.connect)
            }
            pump(&s, at: at, &effects)
        case .discardMessage(let id):
            guard let i = s.outbox.firstIndex(where: { $0.clientID == id }), s.outbox[i].state != .sending else { return }
            releaseFiles(of: s.outbox.remove(at: i), &effects)
            s.messages.removeAll { $0.id == id }
            if case .blockedByUnsentWork = s.pairingProblem {
                s.pairingProblem = s.outbox.isEmpty ? nil : .blockedByUnsentWork(count: s.outbox.count)
            }
            if s.sheet == .forgetBlocked, s.outbox.isEmpty { s.sheet = .forget }
        case .messagesArrived(let arrived):
            for q in arrived.compactMap(\.question) where !s.messages.contains(where: { $0.question?.id == q.id }) {
                let id = "seen-question:" + q.id
                if s.pairing == .paired, !s.outbox.contains(where: { $0.clientID == id }),
                   let body = try? JSONSerialization.data(withJSONObject: ["kind": "question_seen", "question_id": q.id, "thread_id": q.thread_id, "client_id": id], options: [.sortedKeys]) {
                    s.outbox.append(OutboxItem(clientID: id, kind: .text, body: String(decoding: body, as: UTF8.self), queuedAt: 0))
                }
            }
            merge(&s, arrived)
            pump(&s, at: 0, &effects)
            seekNotified(&s, &effects, progressed: true)
        case .replyStarted:
            s.reply = .thinking
        case .replyDelta(let text):
            s.reply = .streaming(text: text)
        case .replyFinished(let message):
            s.reply = nil
            merge(&s, [message])
            seekNotified(&s, &effects, progressed: true)
        case .loadOlder:
            guard !s.history.loadingOlder, !s.history.reachedBeginning else { return }
            s.history.loadingOlder = true
            effects.append(.loadOlder(before: s.messages.first?.id))
        case .olderLoaded(let older, let reachedBeginning):
            s.history.loadingOlder = false
            s.history.reachedBeginning = reachedBeginning
            let known = Set(s.messages.map(\.id))
            merge(&s, older)
            // A page that brought nothing new ends this search (a failed or stale page must not loop);
            // the next messages from the Mac start it again.
            seekNotified(&s, &effects, progressed: older.contains { !known.contains($0.id) })
        case .rememberReading(let anchor):
            s.readingAnchor = anchor
            s.following = false
        case .setFollowing(let following):
            s.following = following
            if following { s.readingAnchor = nil }
        case .setComposerFocus(let focused):
            s.composerFocused = focused
        case .openedFromNotification(let messageID):
            s.notifiedReply = nil
            s.focusedMessageID = messageID
            s.following = true
        case .openedFromNotificationReference(let reference, let preview, let at):
            s.notifiedReply = reference
            if let preview, !preview.isEmpty, !s.messages.contains(where: { notificationReference($0.id) == reference }) {
                // The tapped notification already carries the reply's opening: show it now, as the
                // newest row, and let the stream's full reply replace it (`merge`). The preview is
                // the reply's first `previewLimit` characters, so a full-length one ends in "…".
                let id = provisionalPrefix + reference
                if !s.messages.contains(where: { $0.id == id }) {
                    let text = preview.count >= previewLimit ? preview + "…" : preview
                    s.messages.append(Message(id: id, author: .rich, text: text, sentAt: at))
                    s.messages = ordered(s.messages)
                }
            }
            seekNotified(&s, &effects, progressed: true)
        case .takeShare(let intake, let at):
            take(intake, &s, at: at, &effects)
        case .clearFocus:
            s.focusedMessageID = nil
        case .hearReply(let id):
            guard s.messages.contains(where: { $0.id == id && $0.author == .rich }) else { return }
            if s.playback != nil { effects.append(.stopAudio) }
            s.playback = Playback(messageID: id, phase: .preparing)
            effects.append(.fetchReplyAudio(messageID: id))
        // A file Rich attached (round 36 screen 3; Android's `RichCore.openRichFile`, same rules).
        case .openRichFile(let id):
            guard let file = s.messages.last(where: { $0.richFile?.id == id })?.richFile else { return }
            if let fetch = s.fileFetches[id] {
                if fetch.state == .coming || fetch.state == .finding { return }
                if fetch.state == .kept { s.fileFetches[id]?.open = true; return }
            }
            // The phone's own copy first: `fileFetches` does not survive a relaunch, and a copy that
            // came over earlier opens with the Mac offline or the original deleted there.
            s.fileFetches[id] = FileFetch(id: id, state: .finding, total: file.bytes, open: true)
            effects.append(.findRichFile(id: id, name: file.name))
        case .richFileNotKept(let id):
            guard s.fileFetches[id]?.state == .finding,
                  let file = s.messages.last(where: { $0.richFile?.id == id })?.richFile else { return }
            guard file.exists else {
                s.fileFetches[id] = FileFetch(id: id, state: .failed, reason: RichFileRule.gone)
                return
            }
            guard s.linkOpen, let thread = s.mac?.threadID else {
                // The Mac is out of reach: the clock, and the tap is asked again when the link opens.
                s.fileFetches[id] = FileFetch(id: id, state: .waiting, total: file.bytes, open: true)
                return
            }
            s.fileFetches[id] = FileFetch(id: id, state: .coming, total: file.bytes, open: true)
            effects.append(.fetchRichFile(id: id, threadID: thread, name: file.name, total: file.bytes))
        case .stopRichFile(let id):
            guard let state = s.fileFetches[id]?.state, state == .coming || state == .waiting || state == .finding else { return }
            s.fileFetches[id] = nil
            if state == .coming { effects.append(.stopRichFile(id: id)) }
        case .richFileProgress(let id, let received):
            guard s.fileFetches[id]?.state == .coming else { return }
            s.fileFetches[id]?.received = received
        case .richFileArrived(let id, let path):
            // A × meanwhile wins: the tap it stopped leaves nothing behind.
            guard let fetch = s.fileFetches[id] else { return }
            s.fileFetches[id] = FileFetch(id: id, state: .kept, received: fetch.total ?? fetch.received, total: fetch.total, path: path, open: fetch.open)
        case .richFileFailed(let id, let reason):
            guard s.fileFetches[id] != nil else { return }
            s.fileFetches[id] = FileFetch(id: id, state: .failed, reason: reason)
        case .richFileViewed(let id):
            s.fileFetches[id]?.open = false
        case .playbackStarted(let id):
            // Audio that arrives for a reply no longer asked for never starts (the preserved rule).
            guard s.playback?.messageID == id else { effects.append(.stopAudio); return }
            s.playback = Playback(messageID: id, phase: .playing)
        case .playbackProgress(let id, let progress):
            guard s.playback?.messageID == id, s.playback?.phase == .playing else { return }
            s.playback?.progress = min(1, max(0, progress))
        case .playbackEnded, .stopPlayback:
            if action == .stopPlayback, s.playback != nil { effects.append(.stopAudio) }
            s.playback = nil
        case .dismissToast:
            s.toast = nil
        case .foregrounded(let at):
            resumeAfterReturn(&s, at: at)
        case .backgrounded:
            if s.playback != nil { effects.append(.stopAudio) }
            s.playback = nil
            s.history.loadingOlder = false
            // The stream carrying the reply closes with the app. The next one re-announces a reply
            // still being written and sends a finished one as history, never as "finished", so a
            // reply kept from here would stay "thinking" under its own answer (and animate).
            s.reply = nil
        default:
            break
        }
    }

    private static func send(_ s: inout AppState, clientID: String, at: Int64, _ effects: inout [Effect]) {
        let text = s.draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard s.pairing == .paired, s.consentGiven, !text.isEmpty || !s.pendingAttachments.isEmpty,
              s.connectionNotice != .incompatible else { return }
        guard s.draft.count <= Limits.messageCharacters else {
            s.toast = .tooLong(limit: Limits.messageCharacters)
            return
        }
        // Photos or files in the tray: they and the draft's words go together (Android `send`).
        if sendAttachments(&s, clientID: clientID, text: text, at: at, &effects) { return }
        guard s.outbox.count < outboxLimit else {
            s.toast = .outboxFull(limit: outboxLimit)
            return
        }
        guard !s.outbox.contains(where: { $0.clientID == clientID }) else { return }
        s.outbox.append(OutboxItem(clientID: clientID, kind: .text,
                                   body: textBody(clientID: clientID, threadID: s.mac?.threadID, text: text, sentAt: at),
                                   queuedAt: at))
        s.messages.append(Message(id: clientID, author: .me, text: text, sentAt: at, delivery: .waiting, clientID: clientID, echoAfterCursor: s.messages.compactMap(\.cursor).max() ?? 0,
                                      echoAfterMessageID: s.messages.last(where: { $0.cursor != nil })?.id))
        s.draft = ""
        s.toast = nil
        // Both send gestures resume following immediately (PRD §5).
        s.following = true
        pump(&s, at: at, &effects)
    }

    /// Starts the next delivery if the queue may move: one in flight, first in first out, nothing
    /// while offline, revoked or incompatible, and nothing before the head's retry time.
    ///
    /// And nothing while the Mac's stream is not open (`linkOpen`), unless the Mac has just `answered`
    /// the delivery before it. The stream's owner is the one that asks a Mac out of reach, on its own
    /// back-off; a second loop of sends beside it kept two requests in flight at a hung Mac, each
    /// replaced as it timed out, and drew "Sending…" on an idle screen (I06). The message waits,
    /// saying so, and goes the moment the stream opens (`connected` pumps).
    static func pump(_ s: inout AppState, at: Int64, _ effects: inout [Effect], answered: Bool = false) {
        guard s.pairing == .paired, s.linkOpen || answered, !s.outbox.contains(where: { $0.state == .sending }) else { return }
        switch s.connectionNotice {
        case .phoneOffline?, .incompatible?: return
        default: break
        }
        guard let head = s.outbox.firstIndex(where: { $0.state == .waiting }), s.outbox[head].notBefore <= at else { return }
        s.outbox[head].state = .sending
        setDelivery(&s, s.outbox[head].clientID, .sending)
        effects.append(.deliver(clientID: s.outbox[head].clientID))
    }

    /// SHA-256 (lowercase hex) of a message id: how a notification names its reply (contract §7.3).
    public static func notificationReference(_ id: String) -> String {
        SHA256.hash(data: Data(id.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    /// Focuses the notified reply once it is loaded; otherwise asks for the next older page, while the
    /// last step made progress and the beginning has not been reached (`web/lib/notification-target.js`).
    static func seekNotified(_ s: inout AppState, _ effects: inout [Effect], progressed: Bool) {
        guard let reference = s.notifiedReply else { return }
        if let found = s.messages.last(where: { notificationReference($0.id) == reference }) {
            s.notifiedReply = nil
            s.focusedMessageID = found.id
            s.following = true
            return
        }
        // A provisional row from the notification is on screen: no wait on older history. The
        // stream's reply replaces it, and the next seek focuses that.
        if s.messages.contains(where: { $0.id == provisionalPrefix + reference }) {
            s.focusedMessageID = provisionalPrefix + reference
            s.following = true
            return
        }
        guard progressed, s.pairing == .paired, !s.history.loadingOlder, !s.history.reachedBeginning else { return }
        s.history.loadingOlder = true
        effects.append(.loadOlder(before: s.messages.first?.id))
    }

    private static func setDelivery(_ s: inout AppState, _ id: String, _ delivery: Message.Delivery?) {
        if let i = s.messages.firstIndex(where: { $0.id == id }) { s.messages[i].delivery = delivery }
    }

    /// Reconcile accepted local messages one-to-one, regardless of whether the HTTP receipt or
    /// stream echo arrived first. Remember the association on the server row so a replay cannot
    /// consume another Send with identical words. The outbox remains authoritative until accepted.
    private static func reconcile(_ s: inout AppState) {
        let locals = s.messages.filter { local in local.author == .me && local.clientID == local.id && local.cursor == nil
            && local.delivery == nil && !s.outbox.contains(where: { $0.clientID == local.id }) }
        for local in locals {
            let candidates = s.messages.indices.filter { isEcho(s.messages[$0], of: local, in: s.messages) }
            guard let index = candidates.min(by: { (s.messages[$0].cursor ?? 0) < (s.messages[$1].cursor ?? 0) }) else { continue }
            s.messages[index].clientID = local.id
            if local.kind == .voice {
                s.messages[index].kind = .voice
                s.messages[index].durationMs = local.durationMs
                s.messages[index].levels = local.levels
            }
            if let files = local.attachments { s.messages[index].attachments = files }
            s.messages.removeAll { $0.id == local.id }
        }
    }

    private static func merge(_ s: inout AppState, _ arrived: [Message]) {
        for var message in arrived {
            if let i = s.messages.firstIndex(where: { $0.id == message.id }) {
                // The wire may omit the client ID and use server file references on a replay.
                if let clientID = s.messages[i].clientID, clientID != message.id {
                    message.clientID = clientID
                    if s.messages[i].kind == .voice {
                        message.kind = .voice
                        message.durationMs = message.durationMs ?? s.messages[i].durationMs
                        message.levels = s.messages[i].levels
                    }
                    if let files = s.messages[i].attachments { message.attachments = files }
                }
                s.messages[i] = message
            } else {
                s.messages.append(message)
            }
        }
        reconcile(&s)
        // The real reply has come: the notification's provisional row is done.
        let real = Set(s.messages.filter { !$0.id.hasPrefix(provisionalPrefix) }.map { notificationReference($0.id) })
        s.messages.removeAll { $0.id.hasPrefix(provisionalPrefix) && real.contains(String($0.id.dropFirst(provisionalPrefix.count))) }
        s.messages = ordered(s.messages)
    }

    /// The id prefix of a reply shown from its notification before the stream has delivered it.
    static let provisionalPrefix = "notified:"
    /// How many characters of a reply the Mac seals into a notification (`NotificationPreview`).
    static let previewLimit = 240

    /// The Mac's rows in history-cursor order; the phone's own bubbles (no cursor yet) after them in
    /// the order they were sent. Also the order a relaunch restores, so the screen does not rearrange
    /// when the next frame arrives.
    static func ordered(_ messages: [Message]) -> [Message] {
        messages.enumerated().sorted { a, b in
            switch (a.element.cursor, b.element.cursor) {
            case let (x?, y?) where x != y: return x < y
            case (_?, nil): return true
            case (nil, _?): return false
            default:
                return a.element.sentAt != b.element.sentAt ? a.element.sentAt < b.element.sentAt : a.offset < b.offset
            }
        }.map(\.element)
    }

    /// The text request body (contract §5.2), in the reference's field order, serialized once.
    public static func textBody(clientID: String, threadID: String?, text: String, sentAt: Int64) -> String {
        func quoted(_ value: String) -> String {
            let encoder = JSONEncoder()
            encoder.outputFormatting = .withoutEscapingSlashes
            return String(decoding: (try? encoder.encode(value)) ?? Data("\"\"".utf8), as: UTF8.self)
        }
        var fields = ["\"client_id\":\(quoted(clientID))"]
        if let threadID { fields.append("\"thread_id\":\(quoted(threadID))") }
        fields += ["\"kind\":\"text\"", "\"text\":\(quoted(text))", "\"sent_at\":\(quoted(isoMillis(sentAt)))"]
        return "{" + fields.joined(separator: ",") + "}"
    }

    /// `2026-09-22T13:00:00.000Z`, as JavaScript's `toISOString` writes it.
    public static func isoMillis(_ ms: Int64) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        formatter.timeZone = TimeZone(identifier: "UTC")
        return formatter.string(from: Date(timeIntervalSince1970: Double(ms) / 1000))
    }
}
