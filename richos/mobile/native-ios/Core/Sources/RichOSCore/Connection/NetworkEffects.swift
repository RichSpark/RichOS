import Foundation

/// The network half of the effect handler: pairing, the signed six-word answer, deliveries, older
/// history and the live connection. The platform half (microphone, recorder, notifications, opening
/// Settings) is stream I3's; `CompositeEffectHandler` puts the two together.
public actor NetworkEffects: EffectHandler {
    private let transport: any HTTPTransport
    private let stream: any EventStreamTransport
    private let identities: any IdentityStore
    private let recordings: (any RecordingStore)?
    private let attachments: (any AttachmentStore)?
    /// Where the files Rich attached are kept once they came over; `nil` keeps none.
    private let richFiles: URL?
    private let clock: any Clock
    private let deviceName: String
    private let sleep: LiveConnection.Sleep
    private let silence: LiveConnection.Sleep
    private var sink: LiveConnection.Sink?
    private var api: APIClient?
    private var live: LiveConnection?
    private var replay: Task<LiveConnection.Replay, Never>?
    private var replayIdentity: String?

    private func connectionIdentity(_ state: AppState) -> String {
        [state.mac?.origin ?? "", state.mac?.deviceID ?? "", state.mac?.threadID ?? ""].joined(separator: "\n")
    }
    /// The app's latest word on the live stream. Every `connect` and `disconnect` (and a confirmed
    /// pairing, which connects) takes the next number. A connect that had to wait for the Keychain
    /// lets this actor take other effects meanwhile, so it goes ahead only if nothing was said after
    /// it: a `disconnect` that arrived in the wait wins (no stream in the background), and two
    /// connects make one owner, never an orphaned second stream no `disconnect` can reach.
    private var lifecycle = 0
    /// The APNs token (lowercase hex) and whether this build uses the sandbox, from the app.
    private var pushToken: (hex: String, sandbox: Bool)?
    /// Registration waits for the token when the person turned notifications on before iOS gave one.
    private var registrationWanted: Bool?
    /// The key the Mac seals previews with, per origin, and the person's choice (the app's Keychain
    /// item the notification extension reads; stream I3).
    private var previewKey: (@Sendable (_ origin: String, _ previews: Bool) async -> Data?)?

    public init(transport: any HTTPTransport, stream: any EventStreamTransport, identities: any IdentityStore,
                recordings: (any RecordingStore)? = nil, attachments: (any AttachmentStore)? = nil,
                clock: any Clock = SystemClock(), richFiles: URL? = nil, deviceName: String = "iPhone",
                sleep: @escaping LiveConnection.Sleep = { try await Task.sleep(nanoseconds: UInt64(max(0, $0)) * 1_000_000) },
                silence: @escaping LiveConnection.Sleep = { try await Task.sleep(nanoseconds: UInt64(max(0, $0)) * 1_000_000) }) {
        self.transport = transport; self.stream = stream; self.identities = identities; self.recordings = recordings
        self.attachments = attachments
        self.richFiles = richFiles
        self.clock = clock; self.deviceName = deviceName; self.sleep = sleep; self.silence = silence
    }

    /// Where the live connection's actions go (the store). Set once, before the first `connect`.
    public func setSink(_ sink: @escaping LiveConnection.Sink) { self.sink = sink }

    public func setPreviewKeyProvider(_ provider: @escaping @Sendable (_ origin: String, _ previews: Bool) async -> Data?) {
        previewKey = provider
    }

    /// iOS gave (or changed) the APNs token. Registers at once when registration is wanted, and
    /// re-registers a changed token when notifications are on.
    public func setPushToken(_ hex: String, sandbox: Bool, state: AppState) async -> [Action] {
        let changed = pushToken?.hex != hex
        pushToken = (hex, sandbox)
        if let previews = registrationWanted {
            registrationWanted = nil
            return await register(previews: previews, state: state)
        }
        if changed, state.notifications.status == .on {
            return await register(previews: state.notifications.previews, state: state)
        }
        return []
    }

    /// Signed `POST /api/pair` with the APNs registration (contract §7.2); the answer's `host_id`
    /// is kept for checking taps.
    private func register(previews: Bool, state: AppState) async -> [Action] {
        guard let token = pushToken, let mac = state.mac, let api = await client(for: state) else {
            registrationWanted = previews
            return []
        }
        let key = await previewKey?(mac.origin, previews)
        let body = PairingWire.pushRegistrationBody(tokenHex: token.hex, sandbox: token.sandbox, previewKey: key, previews: previews,
                                                  topic: PairingWire.apnsTopic(forBundleID: Bundle.main.bundleIdentifier))
        guard let response = try? await api.signed("POST", "/api/pair", body: body, contentType: "application/json") else {
            return [.notificationsResult(.serviceUnavailable)]
        }
        let json = (try? JSONSerialization.jsonObject(with: response.body)) as? [String: Any]
        switch response.status {
        case 200:
            let host = json?["host_id"] as? String
            let valid = host.map { $0.count == 32 && $0.allSatisfy { $0.isHexDigit && !$0.isUppercase } } ?? false
            return [.pushRegistered(hostID: valid ? host : nil)]
        case 422 where json?["reason"] as? String == "unsupported":
            return [.notificationsResult(.unsupported)]
        case 503:
            return [.notificationsResult(.serviceUnavailable)]
        default:
            return [.notificationsResult(.off)]
        }
    }

    public nonisolated func handles(_ effect: Effect) -> Bool {
        switch effect {
        case .pair, .confirmFingerprint, .checkMacConfirmation, .forgetIdentity, .deliver, .connect, .reconnect, .disconnect, .loadOlder,
             .requestNotifications, .unregisterNotifications, .deleteAttachments, .fetchRichFile, .findRichFile, .stopRichFile: return true
        default: return false
        }
    }

    public func handle(_ effect: Effect, state: AppState) async -> [Action] {
        switch effect {
        case .pair(let link):
            do {
                let signer = try await identities.signer(for: link.origin)
                let point = try await signer.publicPoint()
                let identity = DeviceIdentity(publicPoint: point)
                switch await PairingExchange.pair(link: link, identity: identity, deviceName: deviceName, transport: transport) {
                case .paired(let paired):
                    api = APIClient(origin: link.origin, deviceID: paired.answer.deviceID, challenge: paired.challenge, signer: signer, transport: transport)
                    var actions: [Action] = [.pairingAnswered(PairAnswer(deviceID: paired.answer.deviceID, fingerprintHex: paired.answer.caFingerprint,
                                                                         threadID: paired.answer.threadID, devicePoint: Base64URL.encode(point),
                                                                         confirmWithinSeconds: paired.answer.confirmWithinSeconds,
                                                                         offersPairWait: paired.answer.offersPairWait))]
                    if let capabilities = paired.answer.capabilities, !capabilities.isEmpty {
                        actions.append(.macCapabilities(text: capabilities.contains("text"), voice: capabilities.contains("voice")))
                actions.append(.macQuestionCapability(capabilities.contains("questions")))
                        actions.append(.macAudioCapability(capabilities.contains("audio")))
                        actions.append(.macAttachmentLimits(capabilities.contains("attachments") ? paired.answer.attachmentLimits : nil))
                    }
                    return actions
                case .macNeedsUpdate(let paired):
                    // A Mac without `pair-v2` is refused, never fallen back to (review §3.5). It has
                    // just registered this key, so it is told to forget it with the one signed request
                    // this phone can still make; the reducer then forgets the key on this side.
                    let refusing = APIClient(origin: link.origin, deviceID: paired.answer.deviceID, challenge: paired.challenge,
                                             signer: signer, transport: transport)
                    _ = try? await refusing.signed("POST", "/api/pair",
                                                   body: PairingWire.confirmationBody(deviceID: paired.answer.deviceID, matches: false),
                                                   contentType: "application/json")
                    return [.pairingNeedsMacUpdate]
                case .failed(let error):
                    return [error.reason == .unreachable ? .pairingUnreachable : .pairingRefused]
                }
            } catch {
                return [.pairingRefused]
            }
        case .confirmFingerprint(let matches):
            // The reducer sends this only for "They do not match" now: "They match" is the wait's
            // first ask (`checkMacConfirmation`). Kept whole, so a "They match" sent this way is still
            // answered with the Mac's word on its own press (`macConfirmation`); the live connection
            // opens only once the Mac has let this phone in (`.connect`).
            guard let api = await client(for: state) else {
                return matches ? [.macConfirmation(.awaiting, at: clock.nowMs())] : []
            }
            let deviceID = state.mac?.deviceID ?? api.deviceID
            let response = try? await api.signed("POST", "/api/pair", body: PairingWire.confirmationBody(deviceID: deviceID, matches: matches),
                                                 contentType: "application/json")
            guard matches else { return [] }
            // No answer is not a refusal: the wait asks the Mac itself.
            return [.macConfirmation(response.map(MacConfirmation.ofConfirmation) ?? .awaiting, at: clock.nowMs())]
        case .checkMacConfirmation(let waitSeconds):
            // One ask of the wait: the phone's own signed "They match" again (`pair_wait`), held by a
            // Mac that offers it for up to `waitSeconds`. The request timeout outlasts the hold
            // (`URLSessionTransport.defaultRequestTimeout`, `request_timeout_must_exceed_ms`).
            let askedAt = clock.nowMs()
            let answer: MacConfirmation
            switch await lookup(for: state) {
            case .keyMissing:
                // Nothing to sign with, ever: this pairing cannot complete.
                answer = .refused
            case .unavailable:
                answer = .awaiting
            case .ready(let api):
                let response = try? await PairingWire.askForTheMacsPress(api, deviceID: state.mac?.deviceID ?? api.deviceID,
                                                                          waitSeconds: waitSeconds)
                // No answer is not a refusal: the wait asks again (and, at the last ask, has expired).
                answer = response.map(MacConfirmation.ofConfirmation) ?? .awaiting
            }
            // The app left the screen meanwhile: the request was canceled and its answer is not one.
            if Task.isCancelled { return [] }
            return [.macConfirmation(answer, at: clock.nowMs(), askedAt: askedAt)]
        case .forgetIdentity(let origin):
            await stopLive()
            replay = nil
            replayIdentity = nil
            api = nil
            try? await identities.forget(origin: origin)
            return []
        case .deliver(let clientID):
            if case .keyMissing = await lookup(for: state) {
                // Final until the phone is paired again; the message stays in the outbox (the reducer's rule).
                return [.deliveryFailed(clientID: clientID, failure: .revoked, at: clock.nowMs())]
            }
            guard let item = state.outbox.first(where: { $0.clientID == clientID }), let api = await client(for: state) else {
                return [.deliveryFailed(clientID: clientID, failure: .retryable(reason: "unreachable", afterMs: nil), at: clock.nowMs())]
            }
            let result = await Courier(api: api, recordings: recordings, attachments: attachments).deliver(item, at: clock.nowMs())
            // Stamped when the answer came, not when the attempt began: a retry's wait counts from the
            // failure (Android `Outbox.kt`: `notBefore = clock.now() + retryDelayMs(...)`). From the
            // start, a request that hung for its 30 s timeout left every wait (1 s to 16 s) already in
            // the past, and the next attempt began the moment it timed out (I06).
            return [Self.answered(result.action, at: clock.nowMs())]
        case .deleteAttachments(let paths):
            for path in paths { await attachments?.remove(path: path) }
            return []
        case .requestNotifications(let previews):
            return await register(previews: previews, state: state)
        case .unregisterNotifications:
            registrationWanted = nil
            if let api = await client(for: state) {
                _ = try? await api.signed("POST", "/api/pair", body: PairingWire.pushUnregistrationBody, contentType: "application/json")
            }
            return []
        case .connect:
            // The key check waits on the Keychain; a `disconnect` taken in that wait must win, so the
            // lifecycle is read BEFORE it (the missing-key check of I-2 and the one-owner rule meet here).
            let asked = lifecycle
            if state.pairing == .paired, case .keyMissing = await lookup(for: state) { return [.pairingRevoked] }
            guard asked == lifecycle else { return [] }
            // Already an owner: this is "try now" ("Try now", a route that came up, a return to the
            // screen while on it). The owner skips what is left of its back-off; never a second owner.
            if let live { await live.wake(); return [] }
            await startLive(state)
            return []
        case .reconnect:
            // The route changed: the owner replaces what it is doing, now. No owner means the app is
            // not on screen (or its connect is still on its way, and will open on the new route):
            // nothing is started here, so a route change can never open a stream off screen.
            await live?.reconnect()
            return []
        case .disconnect:
            await stopLive()
            return []
        case .loadOlder:
            let asked = lifecycle
            guard let api = await client(for: state), let oldest = await live?.oldestCursor(), oldest > 0 else {
                return [.olderLoaded([], reachedBeginning: live != nil)]
            }
            var path = "/api/events?"
            if let thread = state.mac?.threadID { path += "thread_id=\(Delivery.formEncode(thread))&" }
            path += "before=\(oldest)&limit=50"
            guard asked == lifecycle, !Task.isCancelled else { return [] }
            guard let response = try? await api.signed("GET", path, credential: .query), response.status == 200,
                  let page = try? CoreJSON.decode(OlderPage.self, from: response.body) else {
                return [.olderLoaded([], reachedBeginning: false)]
            }
            guard asked == lifecycle, !Task.isCancelled else { return [] }
            await live?.prepend(page.messages, more: page.more)
            return [.olderLoaded(page.messages.filter(\.complete).map(\.message), reachedBeginning: !page.more)]
        case .fetchRichFile(let id, let threadID, let name, let total):
            return await fetchRichFile(id: id, threadID: threadID, name: name, total: total, state: state)
        case .stopRichFile(let id):
            fileTasks.removeValue(forKey: id)?.cancel()
            return []
        case .findRichFile(let id, let name):
            // Disk only, never the Mac: a copy that came over earlier opens whatever the Mac says.
            if let directory = richFiles, let kept = RichFileRule.keptURL(directory: directory, id: id, name: name),
               FileManager.default.fileExists(atPath: kept.path) {
                return [.richFileArrived(id: id, path: kept.path)]
            }
            return [.richFileNotKept(id: id)]
        default:
            return []
        }
    }

    private struct OlderPage: Decodable { var messages: [StreamRow]; var more: Bool }

    // MARK: a file Rich attached (round 36 screen 3)

    /// The taps whose file is coming over, so × can stop them.
    private var fileTasks: [String: Task<Void, Never>] = [:]

    /// A tap on one of Rich's files: a copy already kept opens at once; otherwise its bytes come over
    /// on ONE signed `GET /api/messages?kind=file`, read in parts so the ring fills, written beside
    /// their final name and moved into place only when every byte arrived. It runs on its own task,
    /// so a long file never holds this actor; its progress and outcome reach the store through the sink.
    private func fetchRichFile(id: String, threadID: String, name: String, total: Int64?, state: AppState) async -> [Action] {
        guard let directory = richFiles, let target = RichFileRule.keptURL(directory: directory, id: id, name: name) else {
            return [.richFileFailed(id: id, reason: RichFileRule.failed)]
        }
        if FileManager.default.fileExists(atPath: target.path) { return [.richFileArrived(id: id, path: target.path)] }
        guard let api = await client(for: state), let sink else { return [.richFileFailed(id: id, reason: RichFileRule.failed)] }
        fileTasks[id]?.cancel()
        let stream = self.stream
        let path = RichFileRule.path(fileID: id, threadID: threadID)
        fileTasks[id] = Task { [weak self] in
            let outcome = await Self.download(api: api, stream: stream, path: path, target: target, id: id, total: total, sink: sink)
            await self?.fileTaskEnded(id)
            if let outcome { await sink(outcome) }
        }
        return []
    }

    private func fileTaskEnded(_ id: String) { fileTasks[id] = nil }

    /// The download itself; `nil` when it was stopped (the reducer has already forgotten the tap).
    private static func download(api: APIClient, stream: any EventStreamTransport, path: String, target: URL, id: String,
                                 total: Int64?, sink: @escaping LiveConnection.Sink) async -> Action? {
        let part = target.appendingPathExtension("part")
        let files = FileManager.default
        func failed(_ status: Int) -> Action {
            try? files.removeItem(at: part)
            switch status {
            case 410: return .richFileFailed(id: id, reason: RichFileRule.gone)
            case 413: return .richFileFailed(id: id, reason: RichFileRule.tooLarge)
            default: return .richFileFailed(id: id, reason: RichFileRule.failed)
            }
        }
        do {
            try files.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
            _ = files.createFile(atPath: part.path, contents: nil)
            let handle = try FileHandle(forWritingTo: part)
            defer { try? handle.close() }
            // After a relaunch no challenge is held: ask for one first, as `signed` does.
            if await api.challenge == nil { try await api.probeChallenge() }
            let request = try await api.signedRequest("GET", path)
            let (response, bytes) = try await stream.open(request, origin: api.origin)
            if response.status == 404 {
                // The held challenge was stale: the reference client's one re-sign, in one answer.
                let again = try await api.signed("GET", path)
                guard again.status == 200 else { return failed(again.status) }
                try handle.write(contentsOf: again.body)
            } else if response.status != 200 {
                return failed(response.status)
            } else {
                var received: Int64 = 0
                var shown: Int64 = 0
                let step = max(16_000, (total ?? 0) / 50)
                for try await chunk in bytes {
                    try Task.checkCancellation()
                    try handle.write(contentsOf: chunk)
                    received += Int64(chunk.count)
                    if received - shown >= step {
                        shown = received
                        await sink(.richFileProgress(id: id, received: received))
                    }
                }
            }
            try Task.checkCancellation()
            try handle.close()
            if files.fileExists(atPath: target.path) { try files.removeItem(at: target) }
            try files.moveItem(at: part, to: target)
            return .richFileArrived(id: id, path: target.path)
        } catch {
            try? files.removeItem(at: part)
            return Task.isCancelled ? nil : .richFileFailed(id: id, reason: RichFileRule.failed)
        }
    }

    /// One reply's audio from the paired Mac (`GET /api/audio/<message_id>`, signed in the header as
    /// the Mac requires), or `nil` when there is none to play: no pairing or key, no answer, a refusal
    /// (the Mac's 503 "Audio playback is unavailable"), or bytes over the Mac's own ceiling. Asked
    /// once per press of "Hear it"; never retried, so nothing runs that the person did not ask for.
    public func replyAudio(messageID: String, state: AppState) async -> Data? {
        guard let api = await client(for: state) else { return nil }
        let path = ReplyAudio.path(messageID: messageID, threadID: state.mac?.threadID)
        guard let response = try? await api.signed("GET", path), response.status == 200,
              !response.body.isEmpty, response.body.count <= ReplyAudio.maxBytes else { return nil }
        return response.body
    }

    /// A delivery's answer, restamped with the time it arrived.
    static func answered(_ action: Action, at: Int64) -> Action {
        switch action {
        case .deliveryFailed(let clientID, let failure, _): return .deliveryFailed(clientID: clientID, failure: failure, at: at)
        case .deliveryAccepted(let clientID, _, let hash): return .deliveryAccepted(clientID: clientID, at: at, textSHA256: hash)
        default: return action
        }
    }

    /// The client for the paired Mac, rebuilt after a relaunch from the persisted pairing. With no
    /// challenge held, its first signed request probes for one.
    private func client(for state: AppState) async -> APIClient? {
        if case .ready(let api) = await lookup(for: state) { return api }
        return nil
    }

    private enum Lookup {
        case ready(APIClient)
        /// Nothing to sign with now (no pairing yet, or the Keychain is locked until first unlock).
        case unavailable
        /// Paired, and this phone holds no key for that Mac (security review I-2). Pairing again is
        /// the only way on; a new key is never minted here, because the Mac has never seen it.
        case keyMissing
    }

    private func lookup(for state: AppState) async -> Lookup {
        guard let mac = state.mac, let deviceID = mac.deviceID ?? api?.deviceID else { return api.map(Lookup.ready) ?? .unavailable }
        if let api, api.origin == mac.origin, api.deviceID == deviceID { return .ready(api) }
        let signer: (any Signer)?
        do {
            signer = try await identities.existingSigner(for: mac.origin)
        } catch {
            // A Keychain that cannot be read right now is not a missing key.
            return .unavailable
        }
        guard let signer else { return .keyMissing }
        let fresh = APIClient(origin: mac.origin, deviceID: deviceID, challenge: nil, signer: signer, transport: transport)
        api = fresh
        return .ready(fresh)
    }

    private func startLive(_ state: AppState) async {
        lifecycle += 1
        let mine = lifecycle
        guard live == nil, state.pairing == .paired, let sink, let api = await client(for: state),
              mine == lifecycle, live == nil else { return }
        let identity = connectionIdentity(state)
        let checkpoint = replayIdentity == identity ? await replay?.value : nil
        guard mine == lifecycle, live == nil else { return }
        let connection = LiveConnection(api: api, stream: stream, threadID: state.mac?.threadID, clock: clock, sleep: sleep,
                                        silence: silence, sink: sink)
        // Record ownership before actor hops; a later disconnect still wins over start.
        live = connection
        replayIdentity = identity
        if let checkpoint { await connection.restore(checkpoint) }
        guard mine == lifecycle else { await connection.stop(); return }
        await connection.start()
    }

    /// Forgets the owner BEFORE waiting for it to stop, so a `connect` taken during the wait starts
    /// a new one instead of finding the old one still recorded and doing nothing.
    private func stopLive() async {
        lifecycle += 1
        let ending = live
        live = nil
        if let ending {
            let saved = Task { await ending.stopAndCheckpoint() }
            replay = saved
            _ = await saved.value
        }
    }
}

/// Routes each effect to the first handler that takes it; anything no handler takes is recorded.
public actor CompositeEffectHandler: EffectHandler {
    private let handlers: [any EffectHandler]
    public private(set) var unhandled: [Effect] = []

    public init(_ handlers: [any EffectHandler]) { self.handlers = handlers }

    public nonisolated func handles(_ effect: Effect) -> Bool { handlers.contains { $0.handles(effect) } }

    public func handle(_ effect: Effect, state: AppState) async -> [Action] {
        guard let handler = handlers.first(where: { $0.handles(effect) }) else {
            unhandled.append(effect)
            return []
        }
        return await handler.handle(effect, state: state)
    }
}
