import Foundation
import Testing
@testable import RichOSCore
@testable import RichOSFixtures

// Loop L1 (build plan §3.1): these run on this Mac with `bin/rios test`, no simulator.

/// The repository root, found from this file so the tests can read the shared JavaScript sources.
let repositoryRoot: URL = {
    var url = URL(fileURLWithPath: #filePath)
    for _ in 0..<7 { url.deleteLastPathComponent() }  // …/Core/Tests/RichOSCoreTests/CoreTests.swift → repo
    return url
}()

@Suite struct StateTests {
    @Test func aNewInstallIsUnpairedAndDark() {
        #expect(AppState.initial.pairing == .unpaired)
        #expect(AppState.initial.appearance == .dark)
        #expect(AppState.initial.screen == .pairIntro)
    }

    @Test func theScreenIsDerivedFromState() {
        var s = AppState()
        s.pairing = .paired
        #expect(s.screen == .pairConsent)  // consent comes before the first message
        s.consentGiven = true
        #expect(s.screen == .conversationEmpty)
        s.messages = Conversation.round12
        #expect(s.screen == .conversation)
        s.update = UpdateNotice(prominence: .required, version: "1.1", message: "")
        #expect(s.screen == .updateRequired)
        s.pairingProblem = .sessionNeedsNewerApp
        #expect(s.screen == .pairStale)
    }

    @Test func stateRoundTripsThroughJSONAndPrintsItsScreen() throws {
        let state = try Fixture.named("voice-locked").state
        let data = try CoreJSON.encode(state)
        #expect(try CoreJSON.decode(AppState.self, from: data) == state)
        let object = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        #expect(object["screen"] as? String == "conv-populated")
    }

    @Test func onlyDurableFieldsArePersistedAndTransientOnesAreRebuilt() throws {
        let locked = try Fixture.named("voice-locked").state
        let restored = try AppState(restoring: locked.persisted)
        #expect(restored.voice == nil && restored.microphone == .unknown && restored.sheet == nil)
        #expect(restored.messages == locked.messages && restored.draft == locked.draft && restored.mac == locked.mac)
    }

    @Test func aMessageMidSendIsWaitingAfterARelaunch() throws {
        let pending = try Fixture.named("conv-pending").state
        let restored = try AppState(restoring: pending.persisted)
        #expect(!restored.messages.contains { $0.delivery == .sending })
        #expect(restored.unsentCount == pending.unsentCount)
    }

    @Test func aPairingInFlightIsNotPersistedAsAPairing() throws {
        let restored = try AppState(restoring: try Fixture.named("pair-progress").state.persisted)
        #expect(restored.pairing == .unpaired && restored.mac == nil)
    }

    @Test func fixtureTimesAreFixedInstantsOnTheRound12Day() {
        #expect(Date(timeIntervalSince1970: Double(Conversation.at(0, 0)) / 1000).ISO8601Format() == "2026-09-22T00:00:00Z")
    }

    @Test func theWaveformIsRound12sOwn() {
        // Values printed by Node running round 12's `waveFor` verbatim.
        let wave = Conversation.wave(count: 42, seed: 3)
        #expect(wave.count == 42)
        #expect(Array(wave.prefix(3)) == [0.3312579152606533, 0.29374138837623553, 0.8287466993687326])
        #expect(Array(Conversation.wave(count: 42, seed: 21).suffix(2)) == [0.36256801394150956, 0.6717289962131785])
    }
}

@Suite struct ActionTests {
    @Test func composeSetsTheDraftAndAsksToPersist() {
        let (next, effects) = Reducer.reduce(.initial, .compose(text: "Hello Rich"))
        #expect(next.draft == "Hello Rich")
        #expect(effects == [.persist])
    }

    @Test func aTransientChangeWritesNothing() {
        let (next, effects) = Reducer.reduce(.initial, .openSheet(.settings))
        #expect(next.sheet == .settings)
        #expect(effects.isEmpty)
    }

    @Test func actionsUseThePreservedCLIsJSONSpelling() throws {
        let compose = try CoreJSON.decode(Action.self, from: Data(#"{"type":"compose","text":"Hi"}"#.utf8))
        #expect(compose == .compose(text: "Hi"))
        let theme = try CoreJSON.decode(Action.self, from: Data(#"{"type":"set-appearance","appearance":"light"}"#.utf8))
        #expect(theme == .setAppearance(.light))
        #expect(String(decoding: try CoreJSON.encode(compose), as: UTF8.self) == #"{"text":"Hi","type":"compose"}"#)
    }

    @Test func everyActionRoundTripsThroughJSON() throws {
        let question = try #require(Fixture.named("ask-open").state.messages.first?.question)
        let all: [Action] = [
            .answerQuestion(id: question.id, options: ["today"], text: "", revision: 0, clientID: "answer", at: 1),
            .questionAnswered(clientID: "answer", question: question, at: 2), .macQuestionCapability(true), .macAudioCapability(true),
            .compose(text: "x"), .setAppearance(.light), .openScanner, .closeScanner, .scanned(text: "x"),
            .submitPairingLink(text: "x"), .cameraPermission(.denied), .pairingAnswered(Scenario.answer), .pairingAnswered(Scenario.holdingAnswer),
            .pairingRefused, .confirmWords, .rejectWords, .acceptConsent, .dismissPairingProblem, .openSheet(.forget), .closeSheet,
            .pairingNeedsMacUpdate, .macConfirmation(.awaiting, at: 22), .macConfirmation(.confirmed, at: 23), .macConfirmation(.refused, at: 24),
            .macConfirmation(.awaiting, at: 25, askedAt: 11),
            .sendDraft(clientID: "c", at: 1), .deliveryAccepted(clientID: "c", at: 2),
            .deliveryFailed(clientID: "c", failure: .retryable(reason: "x"), at: 3), .deliveryFailed(clientID: "c", failure: .revoked, at: 3),
            .tick(at: 4), .retryNow(at: 5), .discardMessage(id: "c"), .messagesArrived(Conversation.round12), .replyStarted,
            .replyDelta(text: "Light."), .replyFinished(Conversation.round12[0]), .loadOlder,
            .olderLoaded(Conversation.older, reachedBeginning: true), .setFollowing(false), .rememberReading(ReadingAnchor(messageID: "m", offset: 24)), .setComposerFocus(true),
            .openedFromNotification(messageID: "r1"), .openedFromNotificationReference(String(repeating: "ab", count: 32)), .takeShare(SharedIntake(messages: [.init(clientID: "s1", commitBody: "{}", text: "Hi", files: [OutboxFile(id: "p", name: "a.jpg", mediaType: "image/jpeg", byteCount: 1, sha256: "00", path: "s1/p-a.jpg")])], createdAt: 1, alreadyAccepted: false), at: 2), .clearFocus, .hearReply(id: "r1"), .playbackStarted(id: "r1"),
            .playbackProgress(id: "r1", progress: 0.5), .playbackEnded, .stopPlayback, .dismissToast,
            .networkChanged(online: false, at: 6), .connectionLost(at: 7), .connected(at: 8),
            .connectionDiagnosed(.macUnreachable), .tunnelChanged(up: false), .macCapabilities(text: true, voice: false), .pairingRevoked, .pairingUnreachable, .foregrounded(at: 20), .backgrounded(at: 21), .pushRegistered(hostID: "h"), .macAttachmentLimits(nil),
            .voicePress(id: "v", width: 386, at: 9), .voiceStartLocked(id: "v", width: 386, at: 9), .microphonePermission(.granted), .dismissMicrophoneCard, .voiceMove(dx: -10, dy: -5, at: 10),
            .voiceRelease(at: 11), .voiceLockedSend(at: 12), .voiceLockedCancel(at: 13), .voiceTouchCanceled(at: 14),
            .voiceInterrupted(at: 15), .voiceLevel(0.4), .voiceSettled, .sendKept(id: "k", at: 16), .discardKept(id: "k"),
            .playRecording(id: "k"),
            .turnOnNotifications, .notificationsResult(.denied), .turnOffNotifications, .dismissNotificationOffer,
            .setPreviews(false), .forgetPairing, .confirmForget, .openSystemSettings,
            .updatePolicy(UpdateNotice(prominence: .banner, version: "1.1", message: "x"), voicePaused: true),
            .updatePolicy(nil, voicePaused: false), .dismissUpdate, .openAppStore, .openSupport,
            .checkForUpdates, .openPrivacyPolicy, .discardUnsentAndPair,
            .openAttachMenu, .closeAttachMenu, .pickAttachments(.photos), .attachmentsPicked([OutboxFile(id: "p", name: "a.jpg", mediaType: "image/jpeg", byteCount: 1, sha256: "00", path: "pending/p-a.jpg")]),
            .attachmentRefused(name: "a.zip", bytes: 3, tooLarge: false), .attachPermissionDenied(.camera), .removePendingAttachment(id: "p"), .dismissAttachNotice,
        ]
        #expect(Set(try all.map { try #require(JSONSerialization.jsonObject(with: CoreJSON.encode($0)) as? [String: Any])["type"] as? String })
                == Set(Action.knownTypes))
        for action in all {
            #expect(try CoreJSON.decode(Action.self, from: CoreJSON.encode(action)) == action)
        }
    }

    @Test func thePreservedCLIsNamesAndFieldsAreUnderstood() throws {
        func decode(_ json: String) throws -> Action { try CoreJSON.decode(Action.self, from: Data(json.utf8)) }
        if case .sendDraft(let id, _) = try decode(#"{"type":"send"}"#) { #expect(!id.isEmpty, "an omitted clientId is stamped") } else { Issue.record("send") }
        #expect(try decode(#"{"type":"send","clientId":"c","at":5}"#) == .sendDraft(clientID: "c", at: 5))
        #expect(try decode(#"{"type":"network","online":false,"at":1}"#) == .networkChanged(online: false, at: 1))
        #expect(try decode(#"{"type":"retry","at":2}"#) == .retryNow(at: 2))
        #expect(try decode(#"{"type":"discard","clientId":"c"}"#) == .discardMessage(id: "c"))
        #expect(try decode(#"{"type":"pair","link":"https://m.ts.net/#pair=K"}"#) == .submitPairingLink(text: "https://m.ts.net/#pair=K"))
        #expect(try decode(#"{"type":"confirm-pair","matched":true}"#) == .confirmWords)
        #expect(try decode(#"{"type":"confirm-pair","matched":false}"#) == .rejectWords)
        #expect(try decode(#"{"type":"forget-pair","confirm":true}"#) == .confirmForget)
        #expect(throws: DecodingError.self) { try decode(#"{"type":"forget-pair","confirm":false}"#) }
        #expect(try decode(#"{"type":"notifications-previews","enabled":false}"#) == .setPreviews(false))
        #expect(try decode(#"{"type":"older"}"#) == .loadOlder)
        #expect(try decode(#"{"type":"reply-play","id":"r1"}"#) == .hearReply(id: "r1"))
    }

    @Test func anUnknownActionNamesTheKnownOnes() throws {
        do {
            _ = try CoreJSON.decode(Action.self, from: Data(#"{"type":"fly"}"#.utf8))
            Issue.record("an unknown action decoded")
        } catch let DecodingError.dataCorrupted(context) {
            #expect(context.debugDescription.contains("known: compose, set-appearance"))
        }
    }
}

@Suite struct PairingTests {
    let link = "https://mm1.tail1a2b3c.ts.net:8443/#pair=K7M2QX9H"

    @Test func theReferenceClientsLinksParseToTheSameOriginAndCode() throws {
        // Contract fixtures/pairing.json "links": a tailnet link and a Connect link.
        let tailnet = try PairLink.parse(link)
        #expect(tailnet == PairLink(origin: "https://mm1.tail1a2b3c.ts.net:8443", code: "K7M2QX9H"))
        #expect(tailnet.route == .tailnet)
        let connect = try PairLink.parse("https://c-0123456789abcdef0123456789abcdef-g2.richos.ceo/#pair=K7M2QX9H")
        #expect(connect.origin == "https://c-0123456789abcdef0123456789abcdef-g2.richos.ceo" && connect.route == .connect)
        #expect(try PairLink.parse("https://MM1.tail1a2b3c.ts.net:443/#pair=K7M2QX9H").origin == "https://mm1.tail1a2b3c.ts.net")
    }

    @Test func theReferenceClientsRefusalsAreRefusedWithItsMessages() {
        // Contract fixtures/pairing.json "links_refused_by_reference_client", plus the whitespace rule.
        let refused = [
            ("http://mm1.tail1a2b3c.ts.net:8443/#pair=K7M2QX9H", PairLink.needsHTTPS),
            ("https://mm1.tail1a2b3c.ts.net:8443/x#pair=K7M2QX9H", PairLink.needsHTTPS),
            ("https://mm1.tail1a2b3c.ts.net:8443/#pair=A&pair=B", PairLink.needsOneCode),
            ("https://mm1.tail1a2b3c.ts.net:8443/?q=1#pair=K7M2QX9H", PairLink.needsHTTPS),
            ("https://mm1.tail1a2b3c.ts.net:8443/#pair=K7M2QX9H&x=1", PairLink.needsOneCode),
            ("https://user@mm1.tail1a2b3c.ts.net:8443/#pair=K7M2QX9H", PairLink.needsHTTPS),
            ("https://mm1.tail1a2b3c.ts.net:8443/#pair=", PairLink.needsOneCode),
            ("https://mm1.tail1a2b3c.ts.net:8443/#pair=K7M2 QX9H", PairLink.pasteWholeLink),
            ("not a link", PairLink.pasteWholeLink),
        ]
        for (text, message) in refused {
            #expect(throws: PairLink.Refusal(description: message), "\(text)") { try PairLink.parse(text) }
        }
    }

    /// The six words are v2 words over the origin the phone DIALED: the same Mac answer and key give
    /// different words through a relay, and the words never depend on the answer's `api_base`.
    @Test func theSixWordsAreV2WordsOverTheDialedOrigin() throws {
        var s = try Fixture.named("pair-progress").state
        s = Reducer.reduce(s, .pairingAnswered(Scenario.answer)).state
        #expect(s.screen == .pairWords && s.fingerprintWords.joined(separator: " ") == Scenario.answerWords)
        var relayed = try Fixture.named("pair-progress").state
        relayed.mac = MacLink(origin: "https://relay.example", route: .other)
        relayed = Reducer.reduce(relayed, .pairingAnswered(Scenario.answer)).state
        #expect(relayed.fingerprintWords == ["pocket", "pebble", "carbon", "candle", "compass", "lumber"])
    }

    @Test func theWordListIsTheWebAppsListByteForByte() throws {
        // `richos/web/web-app/lib/wordlist.js` is the source the Mac also mirrors (`phone/ca.rs`).
        let js = try String(contentsOf: repositoryRoot.appendingPathComponent("richos/web/web-app/lib/wordlist.js"), encoding: .utf8)
        let body = try #require(js.range(of: #"WORDS\s*=\s*(Object\.freeze\()?\["#, options: .regularExpression))
        let tail = js[body.upperBound...]
        let list = tail[..<tail.firstIndex(of: "]")!]
        let words = list.split(separator: ",").map { $0.trimmingCharacters(in: CharacterSet(charactersIn: " \n\t'\"")) }.filter { !$0.isEmpty }
        #expect(words == WordList.words)
        #expect(WordList.words.count == 256)
    }

    @Test func pairingRefusesToStrandUnsentWorkForAnotherMac() throws {
        var s = try Fixture.named("pair-blocked").state
        s.pairingProblem = nil
        let (next, effects) = Reducer.reduce(s, .submitPairingLink(text: "https://other.tail1a2b3c.ts.net:8443/#pair=K7M2QX9H"))
        #expect(next.pairingProblem == .blockedByUnsentWork(count: 1))
        #expect(next.pairing == .paired && effects.isEmpty)
    }

    @Test func aDeniedCameraOpensItsExplanationNotTheScanner() {
        var s = AppState()
        s.camera = .denied
        let (next, _) = Reducer.reduce(s, .openScanner)
        #expect(next.scanner == nil && next.sheet == .cameraDenied)
    }

    /// No value from the Mac, or no key to name: no v2 words can be shown, and a v2 phone never falls
    /// back to the old ones. Nothing is trusted and the key is forgotten.
    @Test func aMacThatCannotStateItsFingerprintIsNotTrusted() throws {
        var s = try Fixture.named("pair-progress").state
        s.pairingProblem = nil
        for answer in [PairAnswer(deviceID: "dev_x", fingerprintHex: "", devicePoint: Scenario.answer.devicePoint),
                       PairAnswer(deviceID: "dev_x", fingerprintHex: Scenario.answer.fingerprintHex)] {
            let (next, effects) = Reducer.reduce(s, .pairingAnswered(answer))
            #expect(next.pairing == .unpaired && next.pairingProblem == .refused && next.fingerprintWords.isEmpty
                    && effects.contains(.forgetIdentity(origin: "https://mm1.tail1a2b3c.ts.net:8443")))
        }
    }

    /// `refused_for_missing_pair_v2`: the network effect has signed the one "They do not match"; the
    /// reducer ends the pairing, says the Mac needs an update and forgets the key.
    @Test func aMacWithoutPairV2EndsThePairingAndSaysItNeedsAnUpdate() throws {
        let s = try Fixture.named("pair-progress").state
        let (next, effects) = Reducer.reduce(s, .pairingNeedsMacUpdate)
        #expect(next.screen == .pairIntro && next.pairingProblem == .macNeedsUpdate && next.mac == nil)
        #expect(effects.contains(.forgetIdentity(origin: "https://mm1.tail1a2b3c.ts.net:8443")))
    }
}

@Suite struct ConversationTests {
    func paired() throws -> AppState { try Fixture.named("conv-empty").state }

    @Test func theRetryClockDoublesFromOneSecondToSixteen() {
        #expect((1...7).map { ConversationReducer.retryDelayMs(attempt: $0) } == [1000, 2000, 4000, 8000, 16000, 16000, 16000])
    }

    @Test func theBodyIsTheReferenceShapeAndFixedAtSend() throws {
        var s = try paired()
        s.draft = "  where are we on the proposal?  "
        let (next, effects) = Reducer.reduce(s, .sendDraft(clientID: "01J8FIXTURE0000000000000001", at: 1_790_082_000_000))
        #expect(effects == [.persist, .deliver(clientID: "01J8FIXTURE0000000000000001")])
        // The contract's text vector (fixtures/signing.json) has this exact shape and field order.
        #expect(next.outbox[0].body == #"{"client_id":"01J8FIXTURE0000000000000001","thread_id":"thr_5c1e","kind":"text","text":"where are we on the proposal?","sent_at":"2026-09-22T13:00:00.000Z"}"#)
    }

    @Test func nothingIsSentWhileOfflineAndItGoesWhenTheNoticeClears() throws {
        var s = try paired()
        s.connectionNotice = .phoneOffline
        s.draft = "Hello"
        let (queued, effects) = Reducer.reduce(s, .sendDraft(clientID: "c", at: 1))
        #expect(effects == [.persist] && queued.outbox[0].state == .waiting)
    }

    @Test func anOverlongDraftIsNotSentAndSaysTheLimit() throws {
        var s = try paired()
        s.draft = String(repeating: "x", count: Limits.messageCharacters + 1)
        let (next, _) = Reducer.reduce(s, .sendDraft(clientID: "c", at: 1))
        #expect(next.outbox.isEmpty && next.toast == .tooLong(limit: 4000))
    }

    @Test func sendingResumesFollowing() throws {
        var s = try paired()
        s.following = false
        s.draft = "Hi"
        #expect(Reducer.reduce(s, .sendDraft(clientID: "c", at: 1)).state.following)
    }

    @Test func lateAudioForAReplyNoLongerAskedForNeverStarts() throws {
        var s = try Fixture.named("conv-preparing-reply").state
        s = Reducer.reduce(s, .hearReply(id: "r1")).state
        let (next, effects) = Reducer.reduce(s, .playbackStarted(id: "h2"))
        #expect(next.playback == Playback(messageID: "r1", phase: .preparing) && effects == [.stopAudio])
    }

    @Test func discardingTheLastUnsentMessageUnblocksPairing() throws {
        let s = try Fixture.named("pair-blocked").state
        let next = Reducer.reduce(s, .discardMessage(id: "q1")).state
        #expect(next.pairingProblem == nil && next.outbox.isEmpty)
    }
}

@Suite struct ConnectionTests {
    func paired() throws -> AppState { try Fixture.named("conv-empty").state }

    @Test func anIncompatibleMacPausesSendingAndKeepsTheQueue() throws {
        var s = try Fixture.named("conv-retry").state
        s.connectionNotice = nil
        s = Reducer.reduce(s, .macCapabilities(text: false, voice: false)).state
        #expect(s.connectionNotice == .incompatible && s.voiceAvailability == .unsupportedByMac)
        let (after, effects) = Reducer.reduce(s, .retryNow(at: 1))
        #expect(!effects.contains(.deliver(clientID: "q1")) && after.outbox.count == 1)
        s = Reducer.reduce(s, .connected(at: 2)).state
        #expect(s.connectionNotice == .incompatible, "a reconnect does not clear an incompatible Mac")
    }

    @Test func aDiagnosisNeverOverridesWhatThePhoneKnows() throws {
        var s = try paired()
        s = Reducer.reduce(s, .networkChanged(online: false, at: 1)).state
        s = Reducer.reduce(s, .connectionDiagnosed(.macUnreachable)).state
        #expect(s.connectionNotice == .phoneOffline)
    }

    @Test func connectingReconcilesCachedHistory() throws {
        let s = Reducer.reduce(try Fixture.named("launch-cached").state, .connected(at: 1)).state
        #expect(!s.history.cached)
    }
}

@Suite struct VoiceTests {
    let t: Int64 = 1_000_000
    let width = 386.0
    var cancelDistance: Double { VoiceGeometry.cancelDistance(width: width) }

    func ready() throws -> AppState {
        var s = try Fixture.named("comp-idle").state
        s.microphone = .granted
        return s
    }

    /// Press, wait `holdMs` past the press delay, and return the state.
    func held(_ holdMs: Int64) throws -> AppState {
        var s = try ready()
        s = Reducer.reduce(s, .voicePress(id: "v", width: width, at: t)).state
        s = Reducer.reduce(s, .tick(at: t + VoiceGeometry.pressDelayMs)).state
        return Reducer.reduce(s, .tick(at: t + VoiceGeometry.pressDelayMs + holdMs)).state
    }

    @Test func theNotesThresholds() {
        #expect(VoiceGeometry.cancelDistance(width: 386) == 135.1)
        #expect(VoiceGeometry.cancelDistance(width: 440) == 140)   // capped at 140 pt
        #expect(VoiceGeometry.lockDistance == 60 && VoiceGeometry.pressDelayMs == 200 && VoiceGeometry.tooShortMs == 500)
    }

    @Test func nothingRecordsForTheFirst200ms() throws {
        var s = try ready()
        let (pressed, e1) = Reducer.reduce(s, .voicePress(id: "v", width: width, at: t))
        #expect(pressed.voice?.phase == .pressed && e1.isEmpty)
        s = Reducer.reduce(pressed, .tick(at: t + 199)).state
        #expect(s.voice?.phase == .pressed)
        let (recording, e2) = Reducer.reduce(s, .tick(at: t + 200))
        #expect(recording.voice?.phase == .held && e2 == [.persist, .startRecording(id: "v")])
    }

    @Test func aTapIsNotAMessage() throws {
        var s = try ready()
        s = Reducer.reduce(s, .voicePress(id: "v", width: width, at: t)).state
        let (tapped, effects) = Reducer.reduce(s, .voiceRelease(at: t + 150))
        #expect(tapped.voice?.phase == .ending(.tooShort) && tapped.toast == .tooShort && tapped.outbox.isEmpty && effects.isEmpty)
        let short = Reducer.reduce(try held(499), .voiceRelease(at: t + 699)).state
        #expect(short.voice?.phase == .ending(.tooShort) && short.outbox.isEmpty)
    }

    @Test func holdingAndReleasingSendsAVoiceMessage() throws {
        var s = try held(2600)
        for level in [0.2, 0.5, 0.8] { s = Reducer.reduce(s, .voiceLevel(level)).state }
        let (sent, effects) = Reducer.reduce(s, .voiceRelease(at: t + 2800))
        #expect(sent.voice?.phase == .ending(.sent))
        #expect(sent.outbox.last?.kind == .voice && sent.outbox.last?.recordingID == "v")
        #expect(sent.messages.last == Message(id: "v", author: .me, kind: .voice, text: "", sentAt: t + 2800, delivery: .sending, durationMs: 2600, levels: [0.2, 0.5, 0.8], clientID: "v"))
        #expect(effects.contains(.stopRecording(id: "v", keep: true)) && effects.contains(.deliver(clientID: "v")))
        #expect(Reducer.reduce(sent, .voiceSettled).state.voice == nil)
    }

    @Test func slidingLeftCancelsAtTheThresholdWithoutARelease() throws {
        var s = try held(3000)
        s = Reducer.reduce(s, .voiceMove(dx: -0.99 * cancelDistance, dy: 0, at: t + 3300)).state
        #expect(s.voice?.phase == .held)
        let (canceled, effects) = Reducer.reduce(s, .voiceMove(dx: -cancelDistance, dy: 0, at: t + 3350))
        #expect(canceled.voice?.phase == .ending(.canceled) && canceled.outbox.isEmpty && effects == [.persist, .stopRecording(id: "v", keep: false)])
    }

    @Test func aReleasePast55PercentCancelsAndBelowItSends() throws {
        let past = Reducer.reduce(Reducer.reduce(try held(3000), .voiceMove(dx: -0.56 * cancelDistance, dy: 0, at: t + 3300)).state, .voiceRelease(at: t + 3400)).state
        #expect(past.voice?.phase == .ending(.canceled))
        let before = Reducer.reduce(Reducer.reduce(try held(3000), .voiceMove(dx: -0.54 * cancelDistance, dy: 0, at: t + 3300)).state, .voiceRelease(at: t + 3400)).state
        #expect(before.voice?.phase == .ending(.sent))
    }

    @Test func slidingUp60ptLocksAndLockIsRefusedAfter30PercentSlide() throws {
        let locked = Reducer.reduce(try held(1000), .voiceMove(dx: 0, dy: -60, at: t + 1300))
        #expect(locked.state.voice?.phase == .locked && locked.effects == [.hapticTick])
        var s = Reducer.reduce(try held(1000), .voiceMove(dx: -0.31 * cancelDistance, dy: -80, at: t + 1300)).state
        #expect(s.voice?.phase == .held && s.voice?.lockProgress == 0)
        s = Reducer.reduce(try held(1000), .voiceMove(dx: 0, dy: -40, at: t + 1300)).state
        #expect(abs((s.voice?.lockProgress ?? 0) - 40.0 / 60.0) < 1e-9)
    }

    @Test func lockedCancelAndLockedSend() throws {
        let locked = Reducer.reduce(try held(1000), .voiceMove(dx: 0, dy: -60, at: t + 1300)).state
        #expect(Reducer.reduce(locked, .voiceRelease(at: t + 1400)).state.voice?.phase == .locked, "lifting the finger does not end a locked recording")
        let canceled = Reducer.reduce(locked, .voiceLockedCancel(at: t + 5000)).state
        #expect(canceled.voice?.phase == .ending(.canceled) && canceled.voice?.wasLocked == true && canceled.outbox.isEmpty)
        let sent = Reducer.reduce(locked, .voiceLockedSend(at: t + 5000)).state
        #expect(sent.voice?.phase == .ending(.sent) && sent.messages.last?.durationMs == 4800)
    }

    @Test func anInterruptionKeepsTheRecordingAndNeverSends() throws {
        let (kept, effects) = Reducer.reduce(try held(42000), .voiceInterrupted(at: t + 42200))
        #expect(kept.voice == nil && kept.outbox.isEmpty)
        #expect(kept.keptRecordings == [KeptRecording(id: "v", durationMs: 42000, levels: [], reason: .interrupted, recordedAt: t + 200)])
        #expect(effects.contains(.stopRecording(id: "v", keep: true)))
        #expect(try AppState(restoring: kept.persisted).keptRecordings == kept.keptRecordings, "and it survives a relaunch")
    }

    @Test func aTouchTakenByTheSystemLocksUnder30PercentElseCancels() throws {
        #expect(Reducer.reduce(try held(1000), .voiceTouchCanceled(at: t + 1300)).state.voice?.phase == .locked)
        let slid = Reducer.reduce(try held(1000), .voiceMove(dx: -0.4 * cancelDistance, dy: 0, at: t + 1250)).state
        #expect(Reducer.reduce(slid, .voiceTouchCanceled(at: t + 1300)).state.voice?.phase == .ending(.canceled))
    }

    @Test func theCeilingWarnsOnceAt29AndStopsAndKeepsAt30() throws {
        let locked = Reducer.reduce(try held(1000), .voiceMove(dx: 0, dy: -60, at: t + 1300)).state
        var s = Reducer.reduce(locked, .tick(at: t + 200 + Limits.voiceWarningMs)).state
        #expect(s.toast == .ceilingWarning && s.voice?.ceilingWarned == true)
        s = Reducer.reduce(s, .dismissToast).state
        s = Reducer.reduce(s, .tick(at: t + 200 + Limits.voiceWarningMs + 1000)).state
        #expect(s.toast == nil, "the warning shows once")
        s = Reducer.reduce(s, .tick(at: t + 200 + Limits.voiceCeilingMs)).state
        #expect(s.voice?.phase == .ending(.ceiling) && s.keptRecordings.first?.reason == .ceiling && s.outbox.isEmpty)
    }

    @Test func theFirstPressAsksForTheMicrophoneAndNeverRecords() throws {
        var s = try Fixture.named("comp-idle").state
        let (asking, effects) = Reducer.reduce(s, .voicePress(id: "v", width: width, at: t))
        #expect(asking.sheet == .microphonePrompt && effects == [.requestMicrophone])
        s = Reducer.reduce(asking, .voiceRelease(at: t + 900)).state
        s = Reducer.reduce(s, .tick(at: t + 1000)).state
        #expect(s.voice?.phase == .pressed && s.toast == nil, "the release under the system question is not a gesture")
        s = Reducer.reduce(s, .microphonePermission(.granted)).state
        #expect(s.voice == nil && s.sheet == nil && s.microphone == .granted && s.outbox.isEmpty)
    }

    @Test func voiceIsRefusedWhenOffByPolicyOrDenied() throws {
        var s = try ready()
        s.voiceAvailability = .pausedByPolicy
        #expect(Reducer.reduce(s, .voicePress(id: "v", width: width, at: t)).state.voice == nil)
        s = try ready()
        s.microphone = .denied
        #expect(Reducer.reduce(s, .voicePress(id: "v", width: width, at: t)).state.voice == nil)
    }

    @Test func aKeptRecordingCanBeSentOrDiscarded() throws {
        let kept = try Fixture.named("rec-card").state
        let sent = Reducer.reduce(kept, .sendKept(id: "rec_unsent", at: t))
        #expect(sent.state.keptRecordings.isEmpty && sent.state.outbox.last?.recordingID == "rec_unsent" && sent.effects.contains(.deliver(clientID: "rec_unsent")))
        let discarded = Reducer.reduce(kept, .discardKept(id: "rec_unsent"))
        #expect(discarded.state.keptRecordings.isEmpty && discarded.effects == [.persist, .deleteRecording(id: "rec_unsent")])
        let unsupported = try Fixture.named("rec-unsupported").state
        #expect(Reducer.reduce(unsupported, .sendKept(id: "rec_unsent", at: t)).state.keptRecordings.count == 1, "not while the Mac cannot take voice")
    }

    @Test func handsFreeRecordingStartsLockedForVoiceOver() throws {
        let (s, effects) = Reducer.reduce(try ready(), .voiceStartLocked(id: "v", width: width, at: t))
        #expect(s.voice?.phase == .locked && s.voice?.recordingStartedAtMs == t && effects == [.persist, .startRecording(id: "v")])
    }

    @Test func aMoveAtTouchRateWritesNothing() throws {
        let (_, effects) = Reducer.reduce(try held(1000), .voiceMove(dx: -3, dy: -2, at: t + 1216))
        #expect(effects.isEmpty)
    }
}

@Suite struct SettingsTests {
    @Test func notificationsAreAskedOnceAndEveryStatusIsKept() throws {
        var s = try Fixture.named("notif-offer").state
        let (turning, effects) = Reducer.reduce(s, .turnOnNotifications)
        #expect(turning.notifications.status == .turningOn && effects == [.persist, .requestNotifications(previews: true)])
        s = Reducer.reduce(turning, .notificationsResult(.denied)).state
        #expect(s.notifications.status == .denied)
        s = Reducer.reduce(try Fixture.named("notif-offer").state, .dismissNotificationOffer).state
        #expect(s.notifications.offerDismissed && s.notifications.status == .notAsked)
    }

    @Test func forgettingIsRefusedWhileWorkIsUnsent() throws {
        let s = Reducer.reduce(try Fixture.named("conv-retry").state, .forgetPairing).state
        #expect(s.sheet == .forgetBlocked)
        #expect(Reducer.reduce(s, .confirmForget).state.pairing == .paired, "the refusal cannot be confirmed through")
        let cleared = Reducer.reduce(s, .discardMessage(id: "q1")).state
        #expect(cleared.sheet == .forget, "once the work is resolved the confirmation takes its place")
    }

    @Test func forgettingTurnsNotificationsOffFirstAndDiscardsKeptRecordings() throws {
        var s = try Fixture.named("rec-card").state
        let keptID = try #require(s.keptRecordings.first?.id)
        s = Reducer.reduce(s, .forgetPairing).state
        let (forgotten, effects) = Reducer.reduce(s, .confirmForget)
        #expect(effects == [.persist, .unregisterNotifications, .withdrawNotifications, .disconnect, .forgetIdentity(origin: "https://mm1.tail1a2b3c.ts.net:8443"), .deleteRecording(id: keptID)])
        #expect(forgotten.screen == .pairIntro && forgotten.messages.isEmpty && forgotten.mac == nil && !forgotten.consentGiven)
        #expect(forgotten.keptRecordings.isEmpty, "a recording made for one Mac is never offered to another (D9)")
    }

    @Test func aRequiredUpdateCannotBeDismissedAndThePolicyCanPauseVoice() throws {
        var s = try Fixture.named("upd-blocking").state
        #expect(Reducer.reduce(s, .dismissUpdate).state.update != nil)
        s = Reducer.reduce(try Fixture.named("upd-banner").state, .dismissUpdate).state
        #expect(s.update == nil)
        s = Reducer.reduce(s, .updatePolicy(nil, voicePaused: true)).state
        #expect(s.voiceAvailability == .pausedByPolicy)
        s = Reducer.reduce(s, .updatePolicy(nil, voicePaused: false)).state
        #expect(s.voiceAvailability == .available)
    }

    @Test func aRelaunchShowsSavedHistoryAsCachedUntilTheMacAnswers() throws {
        let restored = try AppState(restoring: try Fixture.named("conv-populated").state.persisted)
        #expect(restored.history.cached)
        #expect(!Reducer.reduce(restored, .connected(at: 1)).state.history.cached)
    }

    /// Tapping a reply notification whose reply is not on the phone opens on its preview at once,
    /// with no "loading older" and no request to the Mac; the stream's reply then replaces it.
    @Test func aReplyNotificationOpensOnItsPreviewWithoutAskingTheMac() throws {
        let s0 = try AppState(restoring: try Fixture.named("conv-populated").state.persisted)
        let id = "turn_new:text:0"
        let ref = ConversationReducer.notificationReference(id)
        let tapped = Reducer.reduce(s0, .openedFromNotificationReference(ref, preview: "Here is the plan", at: 9_000))
        #expect(!tapped.effects.contains { if case .loadOlder = $0 { return true } else { return false } })
        #expect(!tapped.state.history.loadingOlder)
        let row = try #require(tapped.state.messages.first { $0.id.hasPrefix("notified:") })
        #expect(row.author == .rich && row.text == "Here is the plan" && tapped.state.focusedMessageID == row.id)
        // A preview at the sealed limit is only the start of the reply: the row says so.
        let long = Reducer.reduce(s0, .openedFromNotificationReference(ref, preview: String(repeating: "a", count: 240), at: 9_000))
        #expect(long.state.messages.first { $0.id.hasPrefix("notified:") }?.text.hasSuffix("…") == true)
        // The stream's full reply replaces the provisional row.
        let full = Message(id: id, author: .rich, text: "Here is the plan, in full.", sentAt: 9_500, cursor: 1_000_000)
        let arrived = Reducer.reduce(tapped.state, .messagesArrived([full])).state
        #expect(arrived.messages.filter { $0.author == .rich && $0.text.hasPrefix("Here is the plan") }.count == 1)
        #expect(arrived.messages.contains { $0.id == id } && !arrived.messages.contains { $0.id.hasPrefix("notified:") }
                && arrived.focusedMessageID == id && arrived.notifiedReply == nil)
    }

    /// Android's `!hasConnected && notice != null`: a healthy start never shows the out-of-reach
    /// line; it comes only after the quiet 3 s without the Mac, and goes when the Mac answers.
    @Test func theOutOfReachLineWaitsForTheQuietPeriodLikeAndroid() throws {
        var s = try AppState(restoring: try Fixture.named("conv-populated").state.persisted)
        #expect(s.history.cached && !s.history.showsOutOfReachLine(connectionNotice: s.connectionNotice))
        s = Reducer.reduce(s, .foregrounded(at: 1_000)).state
        s = Reducer.reduce(s, .tick(at: 3_999)).state
        #expect(!s.history.showsOutOfReachLine(connectionNotice: s.connectionNotice))
        s = Reducer.reduce(s, .tick(at: 4_000)).state
        #expect(s.history.showsOutOfReachLine(connectionNotice: s.connectionNotice))
        s = Reducer.reduce(s, .connected(at: 5_000)).state
        #expect(!s.history.showsOutOfReachLine(connectionNotice: s.connectionNotice))
    }
}

/// A pretend platform: answers the microphone question and records what it was asked.
actor FakePlatform: EffectHandler {
    var handled: [Effect] = []
    let microphone: Permission
    init(microphone: Permission) { self.microphone = microphone }
    func handle(_ effect: Effect, state: AppState) async -> [Action] {
        handled.append(effect)
        return effect == .requestMicrophone ? [.microphonePermission(microphone)] : []
    }
}

@Suite struct EffectSeamTests {
    @Test func anEffectsAnswerGoesBackThroughTheReducer() async throws {
        let platform = FakePlatform(microphone: .granted)
        let host = try await HeadlessHost(storage: MemoryStorage(), handler: platform)
        _ = try await CommandRunner.execute(Command(.fixture, name: "comp-idle"), on: host)
        let after = try await host.dispatch(.voicePress(id: "v", width: 386, at: 1))
        #expect(after.microphone == .granted && after.sheet == nil && after.voice == nil, "the OS answer closed the question")
        let handled = await platform.handled
        #expect(handled == [.requestMicrophone])
        let pressed = try await host.dispatch(.voicePress(id: "v2", width: 386, at: 2))
        #expect(pressed.voice?.phase == .pressed, "the next press records")
    }

    @Test func withoutAHandlerEffectsAreRecordedNotGuessed() async throws {
        let runner = EffectRunner(storage: MemoryStorage())
        let followUps = try await runner.run([.persist, .requestMicrophone], state: .initial)
        let skipped = await runner.skipped
        #expect(followUps.isEmpty && skipped == [.requestMicrophone])
    }
}

@Suite struct StorageTests {
    func scratch() -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("rios-core-tests-\(UUID().uuidString)")
    }

    @Test func fileStorageWritesAtomicallyAndReadsBack() async throws {
        let dir = scratch()
        defer { try? FileManager.default.removeItem(at: dir) }
        let storage = FileStorage(directory: dir)
        #expect(try await storage.read("state.json") == nil)
        try await storage.write("state.json", Data("x".utf8))
        #expect(try await storage.read("state.json") == Data("x".utf8))
    }

    @Test func fileStorageRefusesAKeyThatEscapesItsDirectory() async {
        let storage = FileStorage(directory: scratch())
        await #expect(throws: CoreError.self) { try await storage.write("../escape", Data()) }
        await #expect(throws: CoreError.self) { try await storage.write(".hidden", Data()) }
    }

    @Test func aCorruptStoredStateThrowsAndIsLeftIntact() async throws {
        let storage = MemoryStorage()
        await storage.write(EffectRunner.stateKey, Data("not json".utf8))
        await #expect(throws: (any Error).self) { try await EffectRunner(storage: storage).load() }
        #expect(await storage.read(EffectRunner.stateKey) == Data("not json".utf8))
    }

    @Test func aStateSavedByANewerAppIsRecognizedAsNewer() async throws {
        let storage = MemoryStorage()
        await storage.write(EffectRunner.stateKey, Data(#"{"schema":3,"somethingNew":true}"#.utf8))
        do {
            _ = try await EffectRunner(storage: storage).load()
            Issue.record("a newer state loaded")
        } catch let error as StoredSchemaError {
            #expect(error.newer)
        }
    }
}

@Suite struct CommandTests {
    /// Every round-12 id that is an app screen on iPhone (67 less the two web-only and two lock-screen ids).
    static let round12AppScreens = """
    pair-intro pair-scanner pair-scanner-found pair-camera-denied pair-progress pair-words pair-refused pair-blocked \
    pair-stale pair-consent conv-empty conv-populated conv-pending conv-replying conv-streaming conv-playing-reply \
    conv-preparing-reply conv-older-loading conv-beginning conv-scrolled conv-focused conv-retry comp-idle comp-typing \
    comp-keyboard comp-disabled comp-too-long voice-press voice-permission voice-holding voice-slide-left voice-bin \
    voice-sent voice-too-short voice-slide-up voice-lock-transition voice-locked voice-locked-scrolled voice-locked-cancel \
    voice-locked-send voice-ceiling-warning voice-ceiling-reached voice-interrupted rec-card rec-unsupported rec-mic-denied \
    conn-reconnecting conn-offline conn-service conn-mac conn-revoked conn-incompatible conn-cached notif-offer \
    notif-settings settings settings-forget settings-forget-blocked upd-banner upd-dialog upd-blocking upd-feature-off \
    launch-cached
    """.split(separator: " ").map(String.init)

    /// Pairing v2's states, which round 12 predates (the PWA's waiting screen and its three endings).
    static let pairingV2Screens = ["pair-awaiting-mac", "pair-mac-update", "pair-mac-refused", "pair-mac-expired",
                                   "pair-words-rejected", "pair-unreachable"]

    /// States found in native acceptance that round 12 has no picture of (Android's catalog ids).
    static let acceptanceScreens = ["conn-tailscale-off"]

    @Test func thereIsOneFixturePerRound12AppScreen() {
        #expect(Self.round12AppScreens.count == 63)
        let added = Self.pairingV2Screens + Self.acceptanceScreens + ["ask-open"]
        #expect(Fixture.all.map(\.name).filter { !added.contains($0) } == Self.round12AppScreens)
        #expect(Fixture.all.map(\.name).filter(Self.pairingV2Screens.contains) == Self.pairingV2Screens)
        #expect(Fixture.all.map(\.name).filter(Self.acceptanceScreens.contains) == Self.acceptanceScreens)
        #expect(Fixture.all.filter { $0.name == "ask-open" }.count == 1)
    }

    @Test func everyFixtureIsOnTheFullScreenSurfaceItsDesignShows() throws {
        let surfaces: [String: Screen] = [
            "pair-intro": .pairIntro, "pair-scanner": .pairScanner, "pair-scanner-found": .pairScanner,
            "pair-camera-denied": .pairIntro, "pair-progress": .pairProgress, "pair-words": .pairWords,
            "pair-refused": .pairIntro, "pair-stale": .pairStale, "pair-consent": .pairConsent,
            "pair-awaiting-mac": .pairAwaitingMac, "pair-mac-update": .pairIntro, "pair-mac-refused": .pairIntro,
            "pair-mac-expired": .pairIntro, "pair-words-rejected": .pairIntro, "pair-unreachable": .pairIntro,
            "conv-empty": .conversationEmpty, "conn-revoked": .connectionRevoked, "upd-blocking": .updateRequired,
        ]
        for fixture in Fixture.all {
            #expect(fixture.state.screen == surfaces[fixture.name] ?? .conversation, "\(fixture.name)")
        }
    }

    @Test func tooLongMeansOverTheLimit() throws {
        #expect(try Fixture.named("comp-too-long").state.draft.count > Limits.messageCharacters)
    }

    @Test func theDraftSurvivesARestartThroughStorage() async throws {
        let storage = MemoryStorage()
        let host = try await HeadlessHost(storage: storage)
        _ = try await CommandRunner.execute(Command(.fixture, name: "conv-empty"), on: host)
        _ = try await CommandRunner.execute(Command(.action, action: .compose(text: "Kept")), on: host)
        let fresh = try await HeadlessHost(storage: storage)
        #expect(try await fresh.currentState().draft == "Kept")
    }

    @Test func everyScenarioPassesItsOwnChecks() async throws {
        for scenario in Scenario.all {
            let host = try await HeadlessHost(storage: MemoryStorage())
            let result = try await CommandRunner.execute(Command(.scenario, name: scenario.name), on: host)
            #expect(result.name == scenario.name)
            #expect(result.trace?.count == scenario.steps.count)
        }
    }

    @Test func aScenarioTraceIsIdenticalOnEveryRun() async throws {
        func run() async throws -> Data {
            let host = try await HeadlessHost(storage: MemoryStorage())
            return try CoreJSON.encode(try await CommandRunner.execute(Command(.scenario, name: "pair-by-scan"), on: host))
        }
        #expect(try await run() == run())
    }

    @Test func aMalformedRequestIsAStructuredRefusalNotACrash() async throws {
        let host = try await HeadlessHost(storage: MemoryStorage())
        for request in [#"{"command":"fixture","name":"nope"}"#, #"{"command":"teleport"}"#, "not json", #"{"command":"action"}"#] {
            let response = try CoreJSON.decode(CommandResponse.self, from: await CommandRunner.respond(to: Data(request.utf8), on: host))
            #expect(!response.ok)
            #expect(response.error != nil)
        }
        let unknown = try CoreJSON.decode(CommandResponse.self, from: await CommandRunner.respond(to: Data(#"{"command":"fixture","name":"nope"}"#.utf8), on: host))
        #expect(unknown.error?.contains("known: pair-intro") == true)
    }

    // MARK: a tapped reply that is not loaded yet (contract §7.3 step 3)

    func loadsOlder(_ effects: [Effect]) -> Bool { effects.contains { if case .loadOlder = $0 { return true } else { return false } } }

    func conversation() throws -> AppState {
        var s = try Fixture.named("conv-empty").state
        s.messages = Conversation.round12
        s.history.reachedBeginning = false
        return s
    }

    @Test func aTappedReplyNotLoadedYetIsFetchedPageByPageUntilFound() throws {
        let target = Conversation.older[1]
        let reference = ConversationReducer.notificationReference(target.id)
        #expect(reference == "\(reference.lowercased())" && reference.count == 64)
        var (s, effects) = Reducer.reduce(try conversation(), .openedFromNotificationReference(reference))
        #expect(loadsOlder(effects) && s.history.loadingOlder && s.notifiedReply == reference && s.focusedMessageID == nil)
        #expect(effects.contains(.persist), "the search survives a relaunch")
        #expect(try AppState(restoring: s.persisted).notifiedReply == reference)

        (s, effects) = Reducer.reduce(s, .olderLoaded(Array(Conversation.older.suffix(1)), reachedBeginning: false))
        #expect(loadsOlder(effects) && s.focusedMessageID == nil, "not on that page: the next one")
        (s, effects) = Reducer.reduce(s, .olderLoaded(Array(Conversation.older.prefix(2)), reachedBeginning: false))
        #expect(s.focusedMessageID == target.id && s.notifiedReply == nil && !loadsOlder(effects), "found, focused, and the search ends")
    }

    @Test func aPageWithNothingNewStopsTheSearchUntilTheMacSendsMore() throws {
        let reference = ConversationReducer.notificationReference("turn_99:text:0")
        var (s, _) = Reducer.reduce(try conversation(), .openedFromNotificationReference(reference))
        var effects: [Effect]
        (s, effects) = Reducer.reduce(s, .olderLoaded([], reachedBeginning: false))
        #expect(!loadsOlder(effects) && s.notifiedReply == reference, "a failed or stale page never loops")
        (s, effects) = Reducer.reduce(s, .messagesArrived([Message(id: "turn_98:text:0", author: .rich, text: "Later.", sentAt: 1, cursor: 98)]))
        #expect(loadsOlder(effects), "new messages from the Mac start it again")
        (s, effects) = Reducer.reduce(s, .olderLoaded(Conversation.older, reachedBeginning: true))
        #expect(!loadsOlder(effects) && s.history.reachedBeginning && s.notifiedReply == reference, "at the beginning: no more pages")
        (s, effects) = Reducer.reduce(s, .messagesArrived([Message(id: "turn_99:text:0", author: .rich, text: "The reply.", sentAt: 2, cursor: 99)]))
        #expect(s.focusedMessageID == "turn_99:text:0" && s.notifiedReply == nil, "a reply that arrives live is found too")
    }

    @Test func aTappedReplyAlreadyLoadedIsFocusedAtOnce() throws {
        let (s, effects) = Reducer.reduce(try conversation(), .openedFromNotificationReference(ConversationReducer.notificationReference("r2")))
        #expect(s.focusedMessageID == "r2" && s.notifiedReply == nil && !loadsOlder(effects))
        let wire = try CoreJSON.decode(Action.self, from: Data(#"{"type":"notification-open","event":"\#(String(repeating: "0", count: 64))"}"#.utf8))
        #expect(wire == .openedFromNotificationReference(String(repeating: "0", count: 64)))
    }
}
