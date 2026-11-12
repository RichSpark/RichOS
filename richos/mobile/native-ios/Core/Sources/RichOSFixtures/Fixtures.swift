#if DEBUG
import Foundation
import RichOSCore

/// A named starting state. Each name is the round-12 screen it shows
/// (`design/mockups/rounds/round-12/shared/screens.js`), so `rios sim fixture voice-locked` puts the
/// app straight onto that design with no navigation, and a screenshot can be compared to the mockup
/// of the same name. All 67 round-12 ids are here except four that are not app screens on iPhone:
/// `pair-pwa-storage`, `notif-pwa-install` (web app only) and `notif-lock-preview`,
/// `notif-lock-generic` (Apple's lock screen; the card is the notification extension's). Pairing v2
/// adds six that round 12 predates: `pair-awaiting-mac`, `pair-mac-update`, `pair-mac-refused`,
/// `pair-mac-expired`, `pair-words-rejected` and `pair-unreachable`.
///
/// The words are round 12's own synthetic conversation — one person and Rich, nothing real.
public struct Fixture: Sendable {
    public var name: String
    public var state: AppState

    public static func named(_ name: String?) throws -> Fixture {
        guard let name, let fixture = all.first(where: { $0.name == name }) else {
            throw CoreError("unknown fixture '\(name ?? "")'; known: \(all.map(\.name).joined(separator: ", "))")
        }
        return fixture
    }

    public static let all: [Fixture] = pairing + conversation + composer + voiceHold + voiceLock + recovery
        + connection + notifications + settings + updates + launch + questions

    // Round 13 can be opened through the same CLI and native fixture bridge.
    static let questions: [Fixture] = [Fixture(name: "ask-open", state: paired { s in
        s.messages = []
        s.mac?.questionAnswers = true
        s.connectionNotice = .phoneOffline
        let wire = #"{"id":"release-question","thread_id":"thr_5c1e","text":"When should the release ship?","options":[{"id":"today","label":"Ship today","description":"Earlier fixes"},{"id":"tomorrow","label":"Ship tomorrow","description":"More testing"}],"multiple":false,"free_answer":true,"recommended":"tomorrow","state":"open","revision":0,"delivered":false,"asker":"Rich"}"#
        let q = try! JSONDecoder().decode(QuestionCard.self, from: Data(wire.utf8))
        var m = Message(id: q.id, author: .rich, text: q.text, sentAt: Conversation.at(9, 41))
        m.question = q
        s.messages = [m]
    })]

    // MARK: builders

    static let tailnet = MacLink(origin: "https://mm1.tail1a2b3c.ts.net:8443", route: .tailnet)
    static let pairedMac = MacLink(origin: "https://mm1.tail1a2b3c.ts.net:8443", route: .tailnet,
                                   deviceID: "dev_8d4c57b7ff82", threadID: "thr_5c1e", name: "Sam’s Mac")
    static let composerWidth = 386.0  // the 16 Pro capsule: 402 pt less the 8 pt margins

    static func make(_ edit: (inout AppState) -> Void) -> AppState {
        var s = AppState()
        edit(&s)
        return s
    }

    /// Paired, consent given, with round 12's `CONVO` (or `OLDER + CONVO` when `full`).
    static func paired(full: Bool = false, extra: [Message] = [], _ edit: (inout AppState) -> Void = { _ in }) -> AppState {
        make { s in
            s.pairing = .paired
            s.mac = pairedMac
            s.consentGiven = true
            s.notifications.status = .on
            s.messages = (full ? Conversation.older : []) + Conversation.round12 + extra
            edit(&s)
            // A still frame of a phone talking to its Mac: the stream is open unless the frame shows
            // trouble (`AppState.linkOpen`).
            s.linkOpen = s.pairing == .paired && s.connectionNotice == nil
            // Every unsent bubble is backed by its outbox item, exactly as a real send leaves it.
            for m in s.messages where m.delivery != nil && !s.outbox.contains(where: { $0.clientID == m.id }) {
                let state: OutboxItem.State = m.delivery == .sending ? .sending : m.delivery == .waiting ? .waiting : .blocked
                s.outbox.append(OutboxItem(
                    clientID: m.id, kind: m.kind,
                    body: m.kind == .text ? ConversationReducer.textBody(clientID: m.id, threadID: s.mac?.threadID, text: m.text, sentAt: m.sentAt) : nil,
                    recordingID: m.kind == .voice ? "rec_\(m.id)" : nil, queuedAt: m.sentAt, state: state,
                    lastReason: state == .blocked ? "The Mac could not take this message." : nil))
            }
        }
    }

    static func me(_ id: String, _ text: String, _ at: Int64, _ delivery: Message.Delivery? = nil) -> Message {
        Message(id: id, author: .me, text: text, sentAt: at, delivery: delivery)
    }

    static func rich(_ id: String, _ text: String, _ at: Int64) -> Message {
        Message(id: id, author: .rich, text: text, sentAt: at)
    }

    static let now = Conversation.at(9, 41)

    static func voice(_ phase: VoiceSession.Phase, elapsed: Int64, dx: Double = 0, dy: Double = 0,
                      wasLocked: Bool = false) -> VoiceSession {
        let started = now - elapsed - 200
        let cancelDistance = VoiceGeometry.cancelDistance(width: composerWidth)
        let recording: Int64? = phase == .pressed ? nil : now - elapsed
        return VoiceSession(phase: phase, startedAtMs: started, nowMs: now, recordingStartedAtMs: recording,
                            dx: dx, dy: dy, width: composerWidth,
                            cancelProgress: min(1, max(0, -dx / cancelDistance)),
                            lockProgress: min(1, max(0, -dy / VoiceGeometry.lockDistance)),
                            levels: Conversation.wave(count: Int(elapsed / 100), seed: 11), wasLocked: wasLocked || phase == .locked)
    }

    static func kept(_ reason: KeptRecording.Reason, _ ms: Int) -> KeptRecording {
        KeptRecording(id: "rec_\(reason.rawValue)", durationMs: ms, levels: Conversation.wave(count: 42, seed: 5), reason: reason, recordedAt: now - Int64(ms))
    }

    // MARK: 1 · pairing

    static let pairing: [Fixture] = [
        Fixture(name: "pair-intro", state: .initial),
        Fixture(name: "pair-scanner", state: make { $0.scanner = .looking; $0.camera = .granted }),
        Fixture(name: "pair-scanner-found", state: make { $0.scanner = .found; $0.camera = .granted; $0.pairing = .connecting; $0.mac = tailnet }),
        Fixture(name: "pair-camera-denied", state: make { $0.camera = .denied; $0.sheet = .cameraDenied }),
        Fixture(name: "pair-progress", state: make { $0.pairing = .connecting; $0.mac = tailnet }),
        Fixture(name: "pair-words", state: make {
            $0.pairing = .confirming; $0.mac = tailnet
            $0.fingerprintWords = ["harbor", "velvet", "copper", "meadow", "lantern", "quartz"]
            // A current Mac, which offers `pair-wait`: "They match" asks it to hold the answer.
            $0.macWait = MacWait(boundMs: MacWait.windowMs, holds: true)
        }),
        Fixture(name: "pair-refused", state: make { $0.pairingProblem = .refused }),
        Fixture(name: "pair-blocked", state: paired(extra: [me("q1", "Move the Friday review to 3 PM.", now, .waiting)]) {
            $0.pairingProblem = .blockedByUnsentWork(count: 1)
        }),
        Fixture(name: "pair-stale", state: make { $0.pairingProblem = .sessionNeedsNewerApp }),
        Fixture(name: "pair-consent", state: paired { $0.consentGiven = false }),
        // Pairing v2, not in round 12 (the PWA's wording, `web/web-app/app.js`): the words stay up
        // while the phone waits for "They match" on the Mac, and the three ways a v2 pairing ends
        // without one. The wait is posed mid-schedule: three asks after the press, the fourth due in
        // 8 s, with a current Mac, which offers `pair-wait`.
        Fixture(name: "pair-awaiting-mac", state: make {
            $0.pairing = .awaitingMac
            $0.mac = MacLink(origin: tailnet.origin, route: .tailnet, deviceID: "dev_8d4c57b7ff82", threadID: "thr_5c1e")
            $0.fingerprintWords = ["harbor", "velvet", "copper", "meadow", "lantern", "quartz"]
            $0.macWait = MacWait(boundMs: MacWait.windowMs, deadlineMs: now - 10_000 + MacWait.windowMs, requests: 3,
                                 nextAskAtMs: now + 8_000, holds: true)
        }),
        Fixture(name: "pair-mac-update", state: make { $0.pairingProblem = .macNeedsUpdate }),
        Fixture(name: "pair-mac-refused", state: make { $0.pairingProblem = .notAcceptedByMac }),
        Fixture(name: "pair-mac-expired", state: make { $0.pairingProblem = .macAnswerExpired }),
        // "They do not match" pressed on this phone (Urban's review, state 4), and no Mac answering
        // the pairing request (the Android app's card; round 12 draws only the refusal).
        Fixture(name: "pair-words-rejected", state: make { $0.pairingProblem = .wordsRejected }),
        Fixture(name: "pair-unreachable", state: make { $0.pairingProblem = .macUnreachable }),
    ]

    // MARK: 2 · the conversation

    static let portland = "Here it is. Portland team: thank you for the sprint on the Henderson bid. It landed, and it landed because of you."

    static let conversation: [Fixture] = [
        Fixture(name: "conv-empty", state: make { $0.pairing = .paired; $0.mac = pairedMac; $0.consentGiven = true; $0.notifications.status = .on; $0.linkOpen = true }),
        Fixture(name: "conv-populated", state: paired(full: true)),
        Fixture(name: "conv-pending", state: paired(extra: [
            me("p1", "Book the 7:10 to Denver, aisle.", now, .sending),
            Message(id: "p2", author: .me, kind: .voice, text: "", sentAt: now, delivery: .waiting, durationMs: 14000,
                    levels: Conversation.wave(count: 42, seed: 21)),
            me("p3", "And cancel the car.", Conversation.at(8, 40), .needsAttention),
        ])),
        Fixture(name: "conv-replying", state: paired(extra: [me("t1", "What does my Thursday look like?", now)]) { $0.reply = .thinking }),
        Fixture(name: "conv-streaming", state: paired(extra: [me("t1", "What does my Thursday look like?", now)]) {
            $0.reply = .streaming(text: "Light. Two things: Dana at 2:00 for board prep, and the dentist at")
        }),
        Fixture(name: "conv-playing-reply", state: paired(extra: [me("h1", "Read me the Portland note.", now), rich("h2", portland, now)]) {
            $0.playback = Playback(messageID: "h2", phase: .playing, progress: 0.35)
        }),
        Fixture(name: "conv-preparing-reply", state: paired(extra: [me("h1", "Read me the Portland note.", now), rich("h2", portland, now)]) {
            $0.playback = Playback(messageID: "h2", phase: .preparing)
        }),
        Fixture(name: "conv-older-loading", state: paired(full: true) { $0.history.loadingOlder = true; $0.following = false }),
        Fixture(name: "conv-beginning", state: paired { $0.history.reachedBeginning = true; $0.following = false }),
        Fixture(name: "conv-scrolled", state: paired(full: true) { $0.following = false }),
        Fixture(name: "conv-focused", state: paired(extra: [
            me("f1", "Did the lease amendment go through?", Conversation.at(9, 2)),
            rich("f2", "Signed and countersigned at 9:18. The landlord’s copy is in your inbox.", Conversation.at(9, 20)),
        ]) { $0.focusedMessageID = "f2" }),
        Fixture(name: "conv-retry", state: paired(extra: [me("q1", "Book the 7:10 to Denver, aisle.", now, .waiting)]) {
            $0.connectionNotice = .reconnecting
        }),
    ]

    // MARK: 3 · the composer

    static let longDraft: String = {
        let paragraph = "Here is the full agenda for the offsite, with every session, owner and the questions I want answered by the end of each block. Start with the morning: strategy review, then the Henderson debrief, then… "
        var text = ""
        while text.count <= Limits.messageCharacters { text += paragraph }
        return text
    }()

    static let composer: [Fixture] = [
        Fixture(name: "comp-idle", state: paired()),
        Fixture(name: "media-ledger", state: paired { state in
            let row = #"{"id":"mm_ledger","thread_id":"general","cursor":90,"role":"rich","kind":"media","created_at":"2026-09-22T09:41:00.000Z","file":{"id":"mm_ledger","name":"Board interview","downloadName":"recording.m4a","mediaType":"audio","moments":{"durationMs":60000,"points":[{"startMs":13000,"label":"The decision","quote":"We chose the new plan."}]}}}"#
            if let message = try? CoreJSON.decode(StreamRow.self, from: Data(row.utf8)).message {
                state.messages.append(message)
            }
            state.linkOpen = false
        }),
        Fixture(name: "comp-typing", state: paired { $0.draft = "Move the Friday review to 3 PM and let the Portland team know it’s optional." }),
        Fixture(name: "comp-keyboard", state: paired { $0.draft = "Move the Friday review to 3"; $0.composerFocused = true }),
        Fixture(name: "comp-disabled", state: paired { $0.connectionNotice = .incompatible }),
        Fixture(name: "comp-too-long", state: paired { $0.draft = longDraft; $0.toast = .tooLong(limit: Limits.messageCharacters) }),
    ]

    // MARK: 4 · voice, hold to record

    static let sentVoice = Message(id: "v1", author: .me, kind: .voice, text: "", sentAt: now, delivery: .sending,
                                   durationMs: 2600, levels: Conversation.wave(count: 26, seed: 11))

    static let voiceHold: [Fixture] = [
        Fixture(name: "voice-press", state: paired { $0.microphone = .granted; $0.voice = voice(.pressed, elapsed: 0) }),
        Fixture(name: "voice-permission", state: paired { $0.voice = voice(.pressed, elapsed: 0); $0.sheet = .microphonePrompt }),
        Fixture(name: "voice-holding", state: paired { $0.microphone = .granted; $0.voice = voice(.held, elapsed: 5500) }),
        Fixture(name: "voice-slide-left", state: paired {
            $0.microphone = .granted
            $0.voice = voice(.held, elapsed: 11800, dx: -0.72 * VoiceGeometry.cancelDistance(width: composerWidth))
        }),
        Fixture(name: "voice-bin", state: paired { $0.microphone = .granted; $0.voice = voice(.ending(.canceled), elapsed: 900) }),
        Fixture(name: "voice-sent", state: paired(extra: [sentVoice]) { $0.microphone = .granted; $0.voice = voice(.ending(.sent), elapsed: 2600) }),
        Fixture(name: "voice-too-short", state: paired { $0.microphone = .granted; $0.voice = voice(.ending(.tooShort), elapsed: 180); $0.toast = .tooShort }),
    ]

    // MARK: 5 · voice, slide up to lock

    static let voiceLock: [Fixture] = [
        Fixture(name: "voice-slide-up", state: paired { $0.microphone = .granted; $0.voice = voice(.held, elapsed: 3200, dy: -40) }),
        Fixture(name: "voice-lock-transition", state: paired { $0.microphone = .granted; $0.voice = voice(.locked, elapsed: 700) }),
        Fixture(name: "voice-locked", state: paired { $0.microphone = .granted; $0.voice = voice(.locked, elapsed: 27900) }),
        Fixture(name: "voice-locked-scrolled", state: paired(full: true) { $0.microphone = .granted; $0.voice = voice(.locked, elapsed: 41300); $0.following = false }),
        Fixture(name: "voice-locked-cancel", state: paired { $0.microphone = .granted; $0.voice = voice(.ending(.canceled), elapsed: 14200, wasLocked: true) }),
        Fixture(name: "voice-locked-send", state: paired(extra: [sentVoice]) { $0.microphone = .granted; $0.voice = voice(.ending(.sent), elapsed: 22600, wasLocked: true) }),
        Fixture(name: "voice-ceiling-warning", state: paired {
            $0.microphone = .granted
            $0.voice = voice(.locked, elapsed: Limits.voiceWarningMs + 400)
            $0.toast = .ceilingWarning
        }),
        Fixture(name: "voice-ceiling-reached", state: paired { $0.microphone = .granted; $0.keptRecordings = [kept(.ceiling, Int(Limits.voiceCeilingMs))] }),
        Fixture(name: "voice-interrupted", state: paired { $0.microphone = .granted; $0.keptRecordings = [kept(.interrupted, 42000)] }),
    ]

    // MARK: 6 · recording recovery

    static let recovery: [Fixture] = [
        Fixture(name: "rec-card", state: paired { $0.keptRecordings = [kept(.unsent, 42000)] }),
        Fixture(name: "rec-unsupported", state: paired { $0.voiceAvailability = .unsupportedByMac; $0.keptRecordings = [kept(.unsent, 42000)] }),
        // The card answers a press while the microphone is off (D03); a denial alone raises nothing.
        Fixture(name: "rec-mic-denied", state: paired { $0.microphone = .denied; $0.microphoneCard = true }),
    ]

    // MARK: 7 · connection

    static let connection: [Fixture] = [
        Fixture(name: "conn-reconnecting", state: paired { $0.connectionNotice = .reconnecting }),
        Fixture(name: "conn-offline", state: paired { $0.connectionNotice = .phoneOffline }),
        Fixture(name: "conn-service", state: paired { $0.connectionNotice = .serviceUnavailable; $0.mac?.route = .connect }),
        Fixture(name: "conn-mac", state: paired { $0.connectionNotice = .macUnreachable }),
        // D05, not in round 12 (Android `conn-tailscale-off`): round 12's nameplate line, adapted.
        Fixture(name: "conn-tailscale-off", state: paired(extra: [me("q1", "Move the Friday review to 3 PM.", now, .waiting)]) {
            $0.connectionNotice = .tailscaleOff
            $0.tunnelUp = false
        }),
        Fixture(name: "conn-revoked", state: paired { $0.pairing = .revoked }),
        Fixture(name: "conn-incompatible", state: paired(extra: [me("q1", "Send the Q4 deck to the board.", now, .waiting)]) {
            $0.connectionNotice = .incompatible
        }),
        Fixture(name: "conn-cached", state: paired(full: true) { $0.history.cached = true; $0.connectionNotice = .phoneOffline }),
    ]

    // MARK: 8 · notifications

    static let notifications: [Fixture] = [
        Fixture(name: "notif-offer", state: paired { $0.notifications.status = .notAsked }),
        Fixture(name: "notif-settings", state: paired { $0.notifications.status = .denied; $0.sheet = .settings }),
    ]

    // MARK: 9 · settings

    static let settings: [Fixture] = [
        Fixture(name: "settings", state: paired { $0.sheet = .settings }),
        Fixture(name: "settings-forget", state: paired { $0.sheet = .forget }),
        Fixture(name: "settings-forget-blocked", state: paired(extra: [me("q1", "Book the 7:10 to Denver, aisle.", now, .waiting)]) {
            $0.sheet = .forgetBlocked
        }),
    ]

    // MARK: 10 · update notices

    static let updates: [Fixture] = [
        Fixture(name: "upd-banner", state: paired { $0.update = UpdateNotice(prominence: .banner, version: "1.1", message: "Faster voice messages. Update in one tap.") }),
        Fixture(name: "upd-dialog", state: paired { $0.update = UpdateNotice(prominence: .dialog, version: "1.1", message: "Version 1.1 fixes voice messages that were cut off on cellular.") }),
        Fixture(name: "upd-blocking", state: paired { $0.update = UpdateNotice(prominence: .required, version: "1.1", message: "Version 1.1 is in the App Store now. Updating takes about a minute.") }),
        Fixture(name: "upd-feature-off", state: paired { $0.voiceAvailability = .pausedByPolicy }),
    ]

    // MARK: 11 · launch

    static let launch: [Fixture] = [
        Fixture(name: "launch-cached", state: paired(full: true) { $0.history.cached = true; $0.connectionNotice = .reconnecting }),
    ]
}

/// Round 12's synthetic conversation (`shared/app.js` `CONVO`, `OLDER`), with times as fixed instants
/// on 2026-09-22 (UTC) so every run is identical.
public enum Conversation {
    static let day: Int64 = 1_790_035_200_000  // 2026-09-22T00:00:00Z
    public static func at(_ hour: Int64, _ minute: Int64, dayOffset: Int64 = 0) -> Int64 {
        day + dayOffset * 86_400_000 + (hour * 60 + minute) * 60_000
    }

    public static let round12: [Message] = [
        Message(id: "r1", author: .rich, text: "Morning. Three things moved overnight: the Henderson proposal came back signed, payroll cleared, and the offsite venue is confirmed for the 14th. Nothing needs you before ten.", sentAt: at(8, 2)),
        Message(id: "m1", author: .me, text: "Push my 10:30 with Dana to Thursday and tell her why.", sentAt: at(8, 14)),
        Message(id: "r2", author: .rich, text: "Done. Dana has Thursday at 2:00 PM and knows it’s board prep. I also moved your prep block to Wednesday afternoon so it isn’t the night before.", sentAt: at(8, 15)),
        Message(id: "m2", author: .me, kind: .voice, text: "", sentAt: at(8, 31), durationMs: 8000, levels: wave(count: 42, seed: 3)),
        Message(id: "r3", author: .rich, text: "Got it. I’ll draft the note to the Portland team tonight and have it in your inbox by seven tomorrow. Want me to copy Priya?", sentAt: at(8, 32)),
        Message(id: "m3", author: .me, text: "Yes. And keep it short.", sentAt: at(8, 33)),
        Message(id: "r4", author: .rich, text: "Short it is.", sentAt: at(8, 33)),
    ]

    public static let older: [Message] = [
        Message(id: "o1", author: .me, text: "Did the insurance renewal go out?", sentAt: at(18, 10, dayOffset: -1)),
        Message(id: "o2", author: .rich, text: "Yes, at 4:52 PM, with the updated headcount. The broker confirmed receipt.", sentAt: at(18, 11, dayOffset: -1)),
        Message(id: "o3", author: .rich, text: "One thing for tomorrow: the lease amendment still needs your signature. I’ve put it first in your inbox.", sentAt: at(21, 40, dayOffset: -1)),
    ]

    /// Round 12's `waveFor(n, seed)`, reproduced exactly — including JavaScript's double-precision
    /// arithmetic in its generator — so a fixture's bubble draws the mockup's own waveform.
    public static func wave(count: Int, seed: Int) -> [Double] {
        var x = Int64(seed)
        var out: [Double] = []
        for i in 0..<count {
            let product = Double(x) * 1_103_515_245 + 12_345   // rounded like a JS number
            x = Int64(product.truncatingRemainder(dividingBy: 4_294_967_296)) & 0x7fff_ffff
            let r = Double(x >> 8) / 8_388_608
            let value = 0.15 + abs(sin(Double(i) * 0.9 + Double(seed))) * 0.6 * r + r * 0.25
            out.append(min(1, max(0.05, value)))
        }
        return out
    }
}

/// A deterministic sequence of commands with the checks that make it a test, in the preserved
/// runtime's shape (`richos/mobile/dev/runtime.js` `scenario`). The same steps run headless and inside
/// the Debug app; `rios sim verify` requires identical results.
public struct Scenario: Sendable {
    public var name: String
    public var steps: [Command]
    public var check: @Sendable ([AppState]) throws -> Void

    static func require(_ ok: Bool, _ why: String) throws { if !ok { throw CoreError("Scenario failed: \(why)") } }

    static let link = "https://mm1.tail1a2b3c.ts.net:8443/#pair=K7M2QX9H"
    /// The corpus's pair answer and test key (`conformance/vectors/pairing.json`, `keys.json`): over
    /// this link's origin they give `fingerprint.json` v2's "tailnet origin, the corpus key" words.
    static let answer = PairAnswer(deviceID: "dev_8d4c57b7ff82",
                                   fingerprintHex: "55:A7:F7:36:2C:07:CD:F4:EE:17:5B:6A:47:D1:0A:0D:62:9B:E7:AF:5F:99:0C:15:B6:5A:88:D1:33:2D:63:87",
                                   threadID: "thr_5c1e",
                                   devicePoint: "BHeD5OMhLyZtmo_jItzPIy19OQHRTz-GzSkelcn1CcsBLXff5lTM1wQ1t7Qk_lxDP4s_Z_YF0lCzliqloS3zmdw",
                                   confirmWithinSeconds: 240)
    static let answerWords = "gazelle coral lettuce grape kayak camera"
    /// The same Mac, current: it offers `pair-wait` beside `pair-v2`.
    static let holdingAnswer: PairAnswer = {
        var a = answer
        a.offersPairWait = true
        return a
    }()

    static let t0 = Conversation.at(9, 41)

    public static let all: [Scenario] = [
        Scenario(name: "question-offline-edit", steps: [
            Command(.fixture, name: "ask-open"),
            Command(.action, action: .compose(text: "Keep this draft")),
            Command(.action, action: .answerQuestion(id: "release-question", options: ["today"], text: "", revision: nil, clientID: "first-answer", at: t0)),
            Command(.restart),
            Command(.action, action: .answerQuestion(id: "release-question", options: ["tomorrow"], text: "", revision: 0, clientID: "edited-answer", at: t0 + 1)),
            Command(.restart)
        ], check: { s in
            try require(s[2].outbox.count == 1 && s[2].draft == "Keep this draft", "answer saves without consuming the composer")
            try require(s[3].outbox[0].body == s[2].outbox[0].body, "restart preserves exact answer bytes")
            try require(s[4].outbox.count == 1 && s[4].outbox[0].questionAnswerText == "Ship tomorrow", "explicit edit replaces only the unattempted answer")
            try require(s[5].outbox[0].body == s[4].outbox[0].body && s[5].draft == "Keep this draft", "edited answer and composer survive restart")
        }),
        // A send that fails, waits its backoff, is interrupted by a relaunch mid-flight, and is
        // accepted on the resend — with the SAME body bytes throughout (contract §5.2, §6.4).
        Scenario(name: "outbox-retry", steps: [
            Command(.fixture, name: "conv-empty"),
            Command(.action, action: .compose(text: "Book the 7:10 to Denver, aisle.")),
            Command(.action, action: .sendDraft(clientID: "c1", at: t0)),
            Command(.action, action: .deliveryFailed(clientID: "c1", failure: .retryable(reason: "unreachable"), at: t0 + 50)),
            Command(.action, action: .tick(at: t0 + 1049)),
            Command(.action, action: .tick(at: t0 + 1050)),
            Command(.restart),
            Command(.action, action: .tick(at: t0 + 1100)),
            Command(.action, action: .connected(at: t0 + 1150)),
            Command(.action, action: .deliveryAccepted(clientID: "c1", at: t0 + 1200)),
        ], check: { s in
            try require(s[2].outbox.count == 1 && s[2].outbox[0].state == .sending && s[2].draft.isEmpty, "a send is persisted and in flight at once")
            try require(s[2].messages.last?.delivery == .sending && s[2].following, "its bubble shows Sending and the conversation follows")
            try require(s[3].outbox[0].state == .waiting && s[3].outbox[0].notBefore == t0 + 1050, "a retryable failure waits 1 s")
            try require(s[4] == s[3], "nothing is attempted before the backoff ends")
            try require(s[5].outbox[0].state == .sending && s[5].outbox[0].attempts == 1, "then it is sent again")
            try require(s[6].outbox[0].state == .waiting && s[6].outbox[0].notBefore == 0, "a relaunch mid-send owes the resend at once")
            try require(s[7].outbox[0].state == .waiting && !s[7].linkOpen, "but nothing goes before the Mac's stream answers")
            try require(s[8].outbox[0].state == .sending, "and it goes the moment it does")
            try require(Set(s.prefix(9).dropFirst(2).map { $0.outbox[0].body }).count == 1, "every attempt carries the same bytes")
            try require(s[9].outbox.isEmpty && s[9].messages.last?.delivery == nil, "accepted: delivered, nothing left to send")
        }),
        // First in, first out: a final refusal blocks that message only; the next one goes.
        Scenario(name: "outbox-refused-continues", steps: [
            Command(.fixture, name: "conv-empty"),
            Command(.action, action: .compose(text: "First")),
            Command(.action, action: .sendDraft(clientID: "a", at: t0)),
            Command(.action, action: .compose(text: "Second")),
            Command(.action, action: .sendDraft(clientID: "b", at: t0 + 10)),
            Command(.action, action: .deliveryFailed(clientID: "a", failure: .refused(reason: "conflict"), at: t0 + 20)),
            Command(.action, action: .deliveryAccepted(clientID: "b", at: t0 + 30)),
            Command(.action, action: .discardMessage(id: "a")),
        ], check: { s in
            try require(s[4].outbox.map(\.state) == [.sending, .waiting], "one in flight; the second waits its turn")
            try require(s[5].outbox.map(\.state) == [.blocked, .sending], "a final refusal blocks only that item")
            try require(s[5].messages.first(where: { $0.id == "a" })?.delivery == .needsAttention, "it shows Not sent")
            try require(s[6].outbox.map(\.clientID) == ["a"], "the second is delivered")
            try require(s[7].outbox.isEmpty && !s[7].messages.contains { $0.id == "a" }, "the blocked one can be discarded")
        }),
        // Offline, a message queued, back online, a lost acknowledgement retried, a quiet drop that
        // never shows, and one that lasts long enough to say "Reconnecting…".
        Scenario(name: "offline-reconnect", steps: [
            Command(.fixture, name: "conv-empty"),
            Command(.action, action: .networkChanged(online: false, at: t0)),
            Command(.action, action: .compose(text: "A message queued offline")),
            Command(.action, action: .sendDraft(clientID: "o1", at: t0 + 100)),
            Command(.restart),
            Command(.action, action: .networkChanged(online: false, at: t0 + 150)),
            Command(.action, action: .networkChanged(online: true, at: t0 + 200)),
            Command(.action, action: .connected(at: t0 + 250)),
            Command(.action, action: .deliveryFailed(clientID: "o1", failure: .retryable(reason: "unreachable"), at: t0 + 300)),
            Command(.action, action: .tick(at: t0 + 1300)),
            Command(.action, action: .deliveryAccepted(clientID: "o1", at: t0 + 1400)),
            Command(.action, action: .connectionLost(at: t0 + 2000)),
            Command(.action, action: .tick(at: t0 + 4999)),
            Command(.action, action: .connected(at: t0 + 4999)),
            Command(.action, action: .connectionLost(at: t0 + 6000)),
            Command(.action, action: .tick(at: t0 + 9000)),
            Command(.action, action: .connected(at: t0 + 9500)),
        ], check: { s in
            try require(s[1].connectionNotice == .phoneOffline, "no network is said at once")
            try require(s[3].outbox.first?.state == .waiting && s[3].messages.last?.delivery == .waiting, "a send offline is kept, not attempted")
            try require(s[4].outbox == s[3].outbox, "the queued message survives a relaunch")
            try require(s[6].connectionNotice == nil && s[6].outbox.first?.state == .waiting, "back online: nothing is announced, and it waits for the Mac")
            try require(s[7].outbox.first?.state == .sending, "the Mac's stream answers: it goes")
            try require(s[8].outbox.first?.notBefore == t0 + 1300, "a lost acknowledgement waits its backoff")
            try require(s[9].outbox.first?.state == .sending, "then resends")
            try require(s[10].outbox.isEmpty, "delivered once")
            try require(s[12].connectionNotice == nil && s[13].connectionNotice == nil, "a drop shorter than 3 s shows nothing")
            try require(s[15].connectionNotice == .reconnecting, "a drop of 3 s or more says Reconnecting")
            try require(s[16].connectionNotice == nil, "and it clears when the Mac answers")
        }),
        // Hold to record: press, the 200 ms delay, levels, slide toward cancel and back, release to send.
        Scenario(name: "voice-hold-send", steps: [
            Command(.fixture, name: "comp-idle"),
            Command(.action, action: .microphonePermission(.granted)),
            Command(.action, action: .voicePress(id: "v1", width: 386, at: t0)),
            Command(.action, action: .tick(at: t0 + 200)),
            Command(.action, action: .voiceLevel(0.4)),
            Command(.action, action: .voiceMove(dx: -60, dy: 0, at: t0 + 1200)),
            Command(.action, action: .voiceMove(dx: -10, dy: 0, at: t0 + 2000)),
            Command(.action, action: .voiceRelease(at: t0 + 2800)),
            Command(.action, action: .voiceSettled),
        ], check: { s in
            try require(s[2].voice?.phase == .pressed && s[3].voice?.phase == .held, "recording starts after the press delay")
            try require(abs((s[5].voice?.cancelProgress ?? 0) - 60 / 135.1) < 1e-9, "sliding left reports progress toward cancel")
            try require(s[7].voice?.phase == .ending(.sent) && s[7].messages.last?.durationMs == 2600, "release sends a 2.6 s voice message")
            try require(s[8].voice == nil && s[8].outbox.first?.recordingID == "v1", "the bubble is backed by the outbox")
        }),
        // Slide up to lock, scroll away (following off), then send from the locked layout.
        Scenario(name: "voice-lock-send", steps: [
            Command(.fixture, name: "comp-idle"),
            Command(.action, action: .microphonePermission(.granted)),
            Command(.action, action: .voicePress(id: "v2", width: 386, at: t0)),
            Command(.action, action: .tick(at: t0 + 200)),
            Command(.action, action: .voiceMove(dx: 0, dy: -60, at: t0 + 700)),
            Command(.action, action: .voiceRelease(at: t0 + 800)),
            Command(.action, action: .setFollowing(false)),
            Command(.action, action: .voiceLockedSend(at: t0 + 14_200)),
        ], check: { s in
            try require(s[4].voice?.phase == .locked, "60 pt up locks")
            try require(s[5].voice?.phase == .locked, "lifting the finger keeps recording")
            try require(s[7].voice?.phase == .ending(.sent) && s[7].following, "sending resumes following")
        }),
        // A voice message whose echo beats its receipt is ONE line, the recording's, at every step.
        // The Mac's row names the recording by the phone's own id (`client_id`, `phone/rows.rs`
        // `client_id_of`); before the Mac kept it, this showed the recording and its transcript as
        // two lines until the receipt came (`EchoBeforeAcceptanceTests`, known since `bf08f0a64`).
        Scenario(name: "voice-echo-before-receipt", steps: [
            Command(.fixture, name: "conv-empty"),
            Command(.action, action: .messagesArrived([
                Message(id: "t1:user", author: .me, text: "Where are we on the Portland note?", sentAt: t0 - 60_000, cursor: 1),
                Message(id: "t1:text:0", author: .rich, text: "Drafted. It goes out at noon.", sentAt: t0 - 59_000, cursor: 2),
            ])),
            Command(.action, action: .microphonePermission(.granted)),
            Command(.action, action: .voicePress(id: "v4", width: 386, at: t0)),
            Command(.action, action: .tick(at: t0 + 200)),
            Command(.action, action: .voiceRelease(at: t0 + 2800)),
            Command(.action, action: .voiceSettled),
            // The Mac's `ceo-message` row for it, in the shape the stream decodes, before the receipt.
            Command(.action, action: .messagesArrived([
                StreamRow(id: "t2:user", threadID: "thr_5c1e", cursor: 3, role: "ceo", kind: "text", text: "Call the bank at four.",
                          createdAt: "2026-10-01T09:41:03.000Z", clientID: "v4", complete: true).message,
            ])),
            Command(.action, action: .deliveryAccepted(clientID: "v4", at: t0 + 3100,
                                                       textSHA256: ConversationReducer.notificationReference("Call the bank at four."))),
        ], check: { s in
            let mine = { (i: Int) in Transcript.visible(s[i]).filter { $0.author == .me }.map(\.lineID) }
            try require(s[6].outbox.first?.recordingID == "v4" && mine(6) == ["t1:user", "v4"], "the recording is one line, on its way")
            try require(s[7].outbox.count == 1 && mine(7) == ["t1:user", "v4"], "the Mac's row beat the receipt: still one line, the recording's")
            try require(s[8].outbox.isEmpty && mine(8) == ["t1:user", "v4"], "the receipt: one line, nothing left to send")
            try require(s[8].messages.last?.kind == .voice && s[8].messages.last?.durationMs == 2600, "and it is the 2.6 s voice message")
        }),
        // Interrupted while locked: kept, never sent; then sent from the recovery card.
        Scenario(name: "voice-interrupted", steps: [
            Command(.fixture, name: "comp-idle"),
            Command(.action, action: .microphonePermission(.granted)),
            Command(.action, action: .voicePress(id: "v3", width: 386, at: t0)),
            Command(.action, action: .tick(at: t0 + 200)),
            Command(.action, action: .voiceMove(dx: 0, dy: -60, at: t0 + 700)),
            Command(.action, action: .voiceInterrupted(at: t0 + 42_200)),
            Command(.restart),
            Command(.action, action: .sendKept(id: "v3", at: t0 + 60_000)),
        ], check: { s in
            try require(s[5].voice == nil && s[5].keptRecordings.first?.durationMs == 42_000 && s[5].outbox.isEmpty, "kept, not sent")
            try require(s[6].keptRecordings == s[5].keptRecordings, "the kept recording survives a relaunch")
            try require(s[7].keptRecordings.isEmpty && s[7].outbox.first?.recordingID == "v3", "Send on the card queues it")
        }),
        // D03: the microphone is off. The OS saying so raises nothing; a press raises the card and
        // records nothing; Not now takes it down; the next press raises it again; a grant (Settings,
        // then back to the app) takes it down and the next press records.
        Scenario(name: "voice-mic-denied", steps: [
            Command(.fixture, name: "comp-idle"),
            Command(.action, action: .microphonePermission(.denied)),
            Command(.action, action: .voicePress(id: "d1", width: 386, at: t0)),
            Command(.action, action: .voiceRelease(at: t0 + 900)),
            Command(.action, action: .dismissMicrophoneCard),
            Command(.action, action: .microphonePermission(.denied)),
            Command(.action, action: .voicePress(id: "d2", width: 386, at: t0 + 5_000)),
            Command(.action, action: .microphonePermission(.granted)),
            Command(.action, action: .voicePress(id: "d3", width: 386, at: t0 + 9_000)),
        ], check: { s in
            try require(s[1].microphone == .denied && !s[1].microphoneCard, "a denial alone raises no card")
            try require(s[2].microphoneCard && s[2].voice == nil && s[2].sheet == nil, "a press raises the card and records nothing")
            try require(s[3].microphoneCard && s[3].voice == nil && s[3].outbox.isEmpty, "the release sends nothing and the card stays")
            try require(!s[4].microphoneCard, "Not now takes the card down")
            try require(!s[5].microphoneCard, "coming back with the microphone still off does not bring it back")
            try require(s[6].microphoneCard, "the next press raises it again")
            try require(!s[7].microphoneCard && s[7].microphone == .granted, "a grant takes the card down")
            try require(s[8].voice?.phase == .pressed && !s[8].microphoneCard, "and the next press records")
        }),
        // Removed from the Mac: final for the pairing, never for his words.
        Scenario(name: "revoked", steps: [
            Command(.fixture, name: "conv-empty"),
            Command(.action, action: .compose(text: "Do not retry a revoked device")),
            Command(.action, action: .sendDraft(clientID: "r", at: t0)),
            Command(.action, action: .deliveryFailed(clientID: "r", failure: .revoked, at: t0 + 10)),
            Command(.action, action: .tick(at: t0 + 60_000)),
        ], check: { s in
            try require(s[3].screen == .connectionRevoked && s[3].outbox.count == 1, "revoked: the takeover shows and the message is kept")
            try require(s[4] == s[3], "nothing is retried while revoked")
        }),
        // The composer takes a draft and gives it back across a theme change and a restart.
        Scenario(name: "compose-draft", steps: [
            Command(.fixture, name: "conv-empty"),
            Command(.action, action: .compose(text: "Hello Rich")),
            Command(.action, action: .setAppearance(.light)),
            Command(.restart),
            Command(.action, action: .compose(text: "")),
            Command(.action, action: .setAppearance(.dark)),
        ], check: { s in
            try require(s[0].screen == .conversationEmpty, "starts on conv-empty")
            try require(s[1].draft == "Hello Rich", "compose sets the draft")
            try require(s[2].appearance == .light && s[2].draft == "Hello Rich", "theme change keeps the draft")
            // A relaunch has no stream open until the Mac answers again (`linkOpen` is transient).
            var relaunched = s[2], fixture = s[0]
            relaunched.linkOpen = false; fixture.linkOpen = false
            try require(s[3] == relaunched, "restart restores the persisted draft and theme")
            try require(s[5] == fixture, "clearing the draft and theme returns to the fixture")
        }),
        // Scan → found → progress → six v2 words (computed on the phone) → match → the press on the
        // Mac → consent → empty chat.
        Scenario(name: "pair-by-scan", steps: [
            Command(.reset),
            Command(.action, action: .cameraPermission(.granted)),
            Command(.action, action: .openScanner),
            Command(.action, action: .scanned(text: "https://example.com/not-a-pairing-link")),
            Command(.action, action: .scanned(text: link)),
            Command(.action, action: .closeScanner),
            Command(.action, action: .pairingAnswered(answer)),
            Command(.action, action: .confirmWords),
            Command(.action, action: .macConfirmation(.awaiting, at: t0)),
            Command(.action, action: .tick(at: t0 + 2_000)),
            Command(.action, action: .macConfirmation(.confirmed, at: t0 + 2_150)),
            Command(.restart),
            Command(.action, action: .acceptConsent),
        ], check: { s in
            try require(s[2].screen == .pairScanner && s[2].scanner == .looking, "the scanner opens")
            try require(s[3] == s[2], "a code that is not a pairing link keeps the camera looking")
            try require(s[4].screen == .pairScanner && s[4].scanner == .found && s[4].pairing == .connecting, "a pairing link is found")
            try require(s[5].screen == .pairProgress, "then pairing is in progress")
            try require(s[6].screen == .pairWords && s[6].fingerprintWords.joined(separator: " ") == answerWords,
                        "the six v2 words are computed on the phone over the dialed origin, the Mac's value and its own key")
            try require(s[7].screen == .pairAwaitingMac && s[7].fingerprintWords == s[6].fingerprintWords,
                        "they match on the phone; the words stay up while it waits for the press on the Mac")
            try require(s[8].macWait?.deadlineMs == t0 + 240_000 && s[8].macWait?.nextAskAtMs == t0 + 2_000,
                        "the Mac's answer starts its 240 s bound; the first probe is owed 2 s later")
            try require(s[9].macWait?.asking == true && s[9].macWait?.requests == 1, "the tick asks once")
            try require(s[10].screen == .pairConsent && s[10].mac?.deviceID == "dev_8d4c57b7ff82" && s[10].macWait == nil,
                        "pressed on the Mac: paired, and consent comes before the first message")
            try require(s[11].screen == .pairConsent, "the pairing survives a restart")
            try require(s[12].screen == .conversationEmpty, "Continue opens the conversation")
        }),
        // Pairing v2's wait for the press on the Mac, foreground only: off screen nothing is owed, a
        // late answer is dropped, the return asks at once, a relaunch resumes it, and past the bound it
        // asks one last time and then ends truthfully. Then a Mac that refuses this phone, and "They do
        // not match" while waiting. This Mac does not offer `pair-wait`: today's schedule.
        Scenario(name: "pair-mac-wait", steps: [
            Command(.reset),
            Command(.action, action: .submitPairingLink(text: link)),
            Command(.action, action: .pairingAnswered(answer)),
            Command(.action, action: .confirmWords),
            Command(.action, action: .macConfirmation(.awaiting, at: t0)),
            Command(.action, action: .backgrounded(at: t0 + 1_000)),
            Command(.action, action: .tick(at: t0 + 60_000)),
            Command(.action, action: .foregrounded(at: t0 + 60_000)),
            Command(.action, action: .macConfirmation(.awaiting, at: t0 + 60_200)),
            Command(.restart),
            Command(.action, action: .foregrounded(at: t0 + 240_000)),
            Command(.action, action: .macConfirmation(.awaiting, at: t0 + 240_100)),
            Command(.action, action: .submitPairingLink(text: link)),
            Command(.action, action: .pairingAnswered(answer)),
            Command(.action, action: .confirmWords),
            Command(.action, action: .macConfirmation(.refused, at: t0 + 300_000)),
            Command(.action, action: .submitPairingLink(text: link)),
            Command(.action, action: .pairingAnswered(answer)),
            Command(.action, action: .confirmWords),
            Command(.action, action: .rejectWords),
        ], check: { s in
            try require(s[4].macWait?.nextAskAtMs == t0 + 2_000, "waiting on screen: the first probe is owed at 2 s")
            try require(s[5].macWait?.paused == true && s[5].macWait?.nextAskAtMs == nil && TickSchedule.nextTick(s[5]) == nil,
                        "off screen: nothing is scheduled")
            try require(s[6] == s[5], "a tick off screen changes nothing")
            try require(s[7].macWait?.asking == true && s[7].macWait?.requests == 1, "back on screen: one probe at once")
            try require(s[8].macWait?.nextAskAtMs == t0 + 63_200, "not yet: the next probe on the schedule (3 s)")
            try require(s[9].screen == .pairAwaitingMac && s[9].macWait?.deadlineMs == t0 + 240_000 && s[9].macWait?.paused == true,
                        "a relaunch keeps the wait and its deadline, paused until the app is on screen")
            try require(s[10].screen == .pairAwaitingMac && s[10].macWait?.finalAsk == true && s[10].macWait?.asking == true,
                        "past the bound the phone asks one last time: the press may have happened while it was away")
            try require(s[11].screen == .pairIntro && s[11].pairingProblem == .macAnswerExpired && s[11].mac == nil && s[11].macWait == nil,
                        "the last ask heard no yes: the wait ends truthfully and nothing is kept")
            try require(s[15].screen == .pairIntro && s[15].pairingProblem == .notAcceptedByMac && s[15].mac == nil,
                        "the Mac refused this phone: nothing is kept")
            try require(s[18].screen == .pairAwaitingMac, "waiting again")
            try require(s[19].screen == .pairIntro && s[19].pairingProblem == .wordsRejected && s[19].mac == nil && s[19].macWait == nil,
                        "They do not match while waiting: nothing is kept, and the screen says it stopped")
        }),
        // `pair-wait` (Sage's pair-v2 hypotheses review §1): a Mac that holds the ask. The press is the
        // first ask and is held; a held answer is followed by the next ask at once; leaving the screen
        // cancels the ask in flight; the return waits until 7 s after the previous ask; the press on
        // the Mac, heard during a hold, pairs.
        Scenario(name: "pair-mac-hold", steps: [
            Command(.reset),
            Command(.action, action: .submitPairingLink(text: link)),
            Command(.action, action: .pairingAnswered(holdingAnswer)),
            Command(.action, action: .confirmWords),
            Command(.action, action: .macConfirmation(.awaiting, at: t0 + 14_000, askedAt: t0)),
            Command(.action, action: .tick(at: t0 + 14_000)),
            Command(.action, action: .backgrounded(at: t0 + 16_000)),
            Command(.action, action: .foregrounded(at: t0 + 17_000)),
            Command(.action, action: .tick(at: t0 + 21_000)),
            Command(.action, action: .macConfirmation(.confirmed, at: t0 + 23_500, askedAt: t0 + 21_000)),
        ], check: { s in
            try require(s[2].macWait?.holds == true, "the Mac named pair-wait beside pair-v2")
            try require(s[3].screen == .pairAwaitingMac && s[3].macWait?.asking == true && s[3].macWait?.requests == 0,
                        "They match is the first ask, and it is held")
            try require(s[4].macWait?.deadlineMs == t0 + 240_000 && s[4].macWait?.nextAskAtMs == t0 + 14_000,
                        "held for 14 s: the bound counts from the press, and the next ask goes at once")
            try require(s[5].macWait?.asking == true && s[5].macWait?.requests == 1, "the tick asks, held again")
            try require(s[6].macWait?.paused == true && s[6].macWait?.asking == false && TickSchedule.nextTick(s[6]) == nil,
                        "off screen: the ask in flight is dropped and nothing is scheduled")
            try require(s[7].macWait?.asking == false && s[7].macWait?.nextAskAtMs == t0 + 21_000,
                        "back 3 s after the ask started: the next ask waits for the 7 s spacing")
            try require(s[8].macWait?.asking == true && s[8].macWait?.requests == 2, "then it asks")
            try require(s[9].screen == .pairConsent && s[9].macWait == nil, "pressed on the Mac during the hold: paired")
        }),
        // A refused code, a bad pasted link, and "they do not match" each leave nothing paired.
        Scenario(name: "pair-refused-and-rejected", steps: [
            Command(.reset),
            Command(.action, action: .openSheet(.pairingLink)),
            Command(.action, action: .submitPairingLink(text: "http://mm1.tail1a2b3c.ts.net:8443/#pair=K7M2QX9H")),
            Command(.action, action: .submitPairingLink(text: link)),
            Command(.action, action: .pairingRefused),
            Command(.action, action: .submitPairingLink(text: link)),
            Command(.action, action: .pairingAnswered(answer)),
            Command(.action, action: .rejectWords),
        ], check: { s in
            try require(s[2].pairingProblem == .invalidLink("Pairing requires an HTTPS origin"), "an http link is refused with the reference message")
            try require(s[3].screen == .pairProgress && s[3].sheet == nil, "a valid pasted link starts pairing and closes the sheet")
            try require(s[4].screen == .pairIntro && s[4].pairingProblem == .refused && s[4].mac == nil, "a refused code pairs nothing")
            try require(s[7].screen == .pairIntro && s[7].mac == nil && s[7].fingerprintWords.isEmpty && s[7].pairingProblem == .wordsRejected,
                        "they do not match: nothing is kept, and the screen says it stopped")
        }),
    ]

    public static func named(_ name: String?) throws -> Scenario {
        guard let name, let scenario = all.first(where: { $0.name == name }) else {
            throw CoreError("unknown scenario '\(name ?? "")'; known: \(all.map(\.name).joined(separator: ", "))")
        }
        return scenario
    }
}
#endif
