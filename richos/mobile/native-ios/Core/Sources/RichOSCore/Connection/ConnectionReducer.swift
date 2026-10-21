import Foundation

/// Connection states, round-12 group 7, under the CEO experience gate (PRD §5; `richos/mobile/AGENTS.md`):
/// healthy operation and routine recovery are invisible. A routine drop shows nothing for
/// `quietMs`; only a trouble that lasts gets the calm "Reconnecting…" line. States the phone KNOWS
/// — no network, the managed service down, the Mac unreachable on the evidence, an incompatible Mac —
/// are explicit at once. Revocation is the pairing's own state, not a notice.
public enum ConnectionReducer {
    /// Routine reconnects are presentation-silent for three seconds (preserved client, DEVELOPMENT.md).
    public static let quietMs: Int64 = 3000

    /// **D05: WHAT THE PHONE CAN SAY ABOUT TAILSCALE, FROM THE OS'S OWN WORD** (Android
    /// `Connections.cause`). Paired over the Tailscale route, with the OS showing no Tailscale tunnel on
    /// this phone, a phone that is merely away cannot reconnect by itself: "Reconnecting…" would not be
    /// true, and the fix is the person's to make. Only the quiet reasons are refined; offline, the
    /// service and incompatible say what they say. An unknown report (`nil`) is never guessed.
    static func cause(_ notice: ConnectionNotice, _ s: AppState) -> ConnectionNotice {
        guard s.pairing == .paired, s.mac?.route == .tailnet, s.tunnelUp == false else { return notice }
        switch notice {
        case .reconnecting, .macUnreachable: return .tailscaleOff
        default: return notice
        }
    }

    static func reduce(_ s: inout AppState, _ action: Action, _ effects: inout [Effect]) {
        switch action {
        case .networkChanged(let online, let at):
            if !online {
                s.connectionNotice = .phoneOffline
                s.linkOpen = false
                effects.append(.disconnect)
                s.troubleSinceMs = s.troubleSinceMs ?? at
            } else if s.connectionNotice == .phoneOffline {
                // Back online: say nothing yet; the transport reconnects, and the quiet period
                // starts again from now.
                s.connectionNotice = nil
                s.troubleSinceMs = at
                if s.pairing == .paired { effects.append(.connect) }
                ConversationReducer.pump(&s, at: at, &effects)
            } else if s.pairing == .paired, s.connectionNotice != .incompatible {
                // Online, and online before: the ROUTE changed (Wi-Fi to cellular, a network joined,
                // Tailscale coming or going; the app reports only real changes, `NetworkPath.isNews`).
                // A stream on the old route may be dead without a word, an open on it may hang, and
                // a back-off may be running: the owner replaces it at once. Before, nothing happened
                // here, and only the 20 s silence limit noticed (andy-opus-resume1's third wait).
                effects.append(.reconnect)
            }
        case .connectionLost(let at):
            s.troubleSinceMs = s.troubleSinceMs ?? at
            s.linkOpen = false
        case .connected(let at):
            s.troubleSinceMs = nil
            s.linkOpen = true
            if s.connectionNotice != .incompatible { s.connectionNotice = nil }
            // History on screen has now been reconciled with the Mac.
            s.history.cached = false
            ConversationReducer.pump(&s, at: at, &effects)
            // A file Rich attached that was tapped while the Mac was out of reach ("Opens when your
            // Mac is back online", round 36) comes over now.
            if let thread = s.mac?.threadID {
                for (id, fetch) in s.fileFetches where fetch.state == .waiting {
                    guard let file = s.messages.last(where: { $0.richFile?.id == id })?.richFile else { continue }
                    s.fileFetches[id] = FileFetch(id: id, state: .coming, total: file.bytes, open: fetch.open)
                    effects.append(.fetchRichFile(id: id, threadID: thread, name: file.name, total: file.bytes))
                }
            }
        case .connectionDiagnosed(let notice):
            // Evidence-based only (contract §6.3): the transport's probe says which.
            guard notice != .reconnecting, s.connectionNotice != .incompatible, s.connectionNotice != .phoneOffline else { return }
            s.connectionNotice = cause(notice, s)
        case .tunnelChanged(let up):
            // Came up: known down before (a first report is not a change the person made).
            let cameUp = up && s.tunnelUp == false
            s.tunnelUp = up
            switch s.connectionNotice {
            case .tailscaleOff? where up:
                // The fix is done: the line says what is true now, until the Mac answers.
                s.connectionNotice = .reconnecting
            case .reconnecting?, .macUnreachable?:
                s.connectionNotice = cause(s.connectionNotice!, s)
            default:
                break
            }
            // A tunnel that came up is a useful moment to try, now rather than at the end of the
            // back-off (Android: `if (up) wake()`); one that went down is not.
            if cameUp, s.pairing == .paired, !s.linkOpen, s.connectionNotice != .phoneOffline, s.connectionNotice != .incompatible {
                effects.append(.connect)
            }
        case .macQuestionCapability(let available): s.mac?.questionAnswers = available
        case .macAudioCapability(let available): s.mac?.replyAudio = available
        case .macCapabilities(let text, let voice):
            s.connectionNotice = text ? (s.connectionNotice == .incompatible ? nil : s.connectionNotice) : .incompatible
            if voice {
                if s.voiceAvailability == .unsupportedByMac { s.voiceAvailability = .available }
            } else {
                s.voiceAvailability = .unsupportedByMac
            }
        case .pairingRevoked:
            // Final for the pairing, not for his words: the conversation and anything unsent stay.
            s.pairing = .revoked
            s.troubleSinceMs = nil
            s.connectionNotice = nil
            s.linkOpen = false
            effects.append(.disconnect)
        case .macAttachmentLimits(let limits):
            s.attachmentLimits = limits
        case .pushRegistered(let hostID):
            s.notifications.status = .on
            s.notifications.hostID = hostID
            s.notifications.offerDismissed = true
        case .foregrounded(let at):
            guard s.pairing == .paired, s.connectionNotice != .phoneOffline else { return }
            effects.append(.connect)
            // Connecting is trouble until the Mac answers: a Mac that takes the connection and never
            // answers is explained after the same quiet 3 s as one that refuses it, not after the
            // stream's minute-long timeout (I06). The reference starts its 3 s at `opening`, Android
            // at `Link(OPENING)`. A healthy stream opens well inside the quiet period and clears it.
            if !s.linkOpen { s.troubleSinceMs = s.troubleSinceMs ?? at }
            ConversationReducer.pump(&s, at: at, &effects)
        case .backgrounded:
            // No stream in the background (build plan §3.2): APNs is for awareness, the foreground
            // reconciles. The quiet period restarts on return.
            s.troubleSinceMs = nil
            s.linkOpen = false
            if s.pairing == .paired { effects.append(.disconnect) }
        case .tick(let at):
            if let since = s.troubleSinceMs, s.connectionNotice == nil, at - since >= quietMs {
                s.connectionNotice = cause(.reconnecting, s)
            }
        default:
            break
        }
    }
}
