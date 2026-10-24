import Foundation
import Testing
@testable import RichOSCore
@testable import RichOSFixtures

/// After the Mac removed this phone (Urban's UX audit G2, §4.1): "Pair again" works, and messages
/// written for the Mac that removed the phone are never sent to whichever Mac is paired next.
@Suite struct PairingRemovedTests {
    /// `pair-blocked`'s own state, then the Mac removes the phone: one message waiting, revoked.
    func removedWithWork() -> AppState {
        var s = try! Fixture.named("pair-blocked").state
        s.pairingProblem = nil
        s.pairing = .revoked
        return s
    }

    @Test func pairAgainWithNothingWaitingOpensTheScannerOverTheRemovedScreen() {
        var s = try! Fixture.named("conn-revoked").state
        s.camera = .granted
        let next = Reducer.reduce(s, .openScanner).state
        #expect(next.scanner == .looking)
        #expect(next.screen == .pairScanner)
        // Closing it comes back to the removed screen, not to a first-run intro.
        #expect(Reducer.reduce(next, .closeScanner).state.screen == .connectionRevoked)
    }

    @Test func pairAgainWithAMessageWaitingAsksFirstAndOpensNothing() {
        let next = Reducer.reduce(removedWithWork(), .openScanner).state
        #expect(next.pairingProblem == .blockedByUnsentWork(count: 1))
        #expect(next.scanner == nil)
        #expect(next.screen == .connectionRevoked)
    }

    @Test func notNowKeepsTheMessageAndTheRemovedScreen() {
        let asked = Reducer.reduce(removedWithWork(), .openScanner).state
        let kept = Reducer.reduce(asked, .dismissPairingProblem).state
        #expect(kept.pairingProblem == nil)
        #expect(kept.outbox.count == 1)
        #expect(kept.messages.contains { $0.id == "q1" })
        #expect(kept.screen == .connectionRevoked)
    }

    @Test func discardAndPairDiscardsTheMessageThenOpensTheScanner() {
        var s = removedWithWork()
        s.camera = .granted
        let asked = Reducer.reduce(s, .openScanner).state
        let (next, effects) = Reducer.reduce(asked, .discardUnsentAndPair)
        #expect(next.outbox.isEmpty)
        #expect(!next.messages.contains { $0.id == "q1" })
        #expect(next.pairingProblem == nil)
        #expect(next.scanner == .looking)
        // Nothing is sent: the Mac that removed this phone never gets it, and neither does the next.
        #expect(!effects.contains { if case .deliver = $0 { return true } else { return false } })
    }

    @Test func discardAndPairWithTheCameraOffShowsTheCameraDialog() {
        var s = removedWithWork()
        s.camera = .denied
        let asked = Reducer.reduce(s, .openScanner).state
        #expect(Reducer.reduce(asked, .discardUnsentAndPair).state.sheet == .cameraDenied)
    }

    @Test func aScannedCodeAfterRemovalStartsPairing() throws {
        var s = try Fixture.named("conn-revoked").state
        s.camera = .granted
        s = Reducer.reduce(s, .openScanner).state
        let (next, effects) = Reducer.reduce(s, .scanned(text: "https://mm2.tail1a2b3c.ts.net:8443/#pair=K7M2QX9H"))
        #expect(next.pairing == .connecting)
        #expect(effects.contains { if case .pair = $0 { return true } else { return false } })
    }

    /// Android's fault of 2026-10-22 (cc/andy-opus-honorfile1 e76bb7887): a stored REVOKED reason
    /// outlived the new pairing and kept "This phone was removed from your Mac" over the six words.
    /// Here the removed screen is derived from `pairing` alone, so a pasted link and the Mac's answer
    /// bring the words up, and nothing of the removal is left to cover them.
    @Test func aRemovedPhonePairedAgainByLinkShowsTheSixWords() throws {
        var s = try Fixture.named("conn-revoked").state
        #expect(s.screen == .connectionRevoked)
        s = Reducer.reduce(s, .submitPairingLink(text: "https://mm2.tail1a2b3c.ts.net:8443/#pair=K7M2QX9H")).state
        #expect(s.screen == .pairProgress)
        s = Reducer.reduce(s, .pairingAnswered(Scenario.answer)).state
        #expect(s.screen == .pairWords)
        #expect(s.fingerprintWords.count == 6)
    }

    /// A Mac that is gone for good (reinstalled, replaced) with a message still waiting for it: the
    /// iPhone TEST app on 2026-10-22 had no way out. Forget was refused while the message waited, the
    /// message had no Discard (only one that needs attention has), and a paired phone has no
    /// pairing link. "Discard and forget" in the refusal is that way out.
    @Test func discardAndForgetLetsAPhoneWhoseMacIsGonePairAgain() throws {
        var s = removedWithWork()
        s.pairing = .paired
        s.connectionNotice = .macUnreachable
        s = Reducer.reduce(s, .forgetPairing).state
        #expect(s.sheet == .forgetBlocked)
        let (forgotten, effects) = Reducer.reduce(s, .discardUnsentAndForget)
        #expect(forgotten.outbox.isEmpty)
        #expect(forgotten.mac == nil)
        #expect(forgotten.screen == .pairIntro)
        #expect(effects.contains(.forgetIdentity(origin: "https://mm1.tail1a2b3c.ts.net:8443")))
        // Nothing is sent: the Mac it was written for never gets it, and neither does the next.
        #expect(!effects.contains { if case .deliver = $0 { return true } else { return false } })
        let pairing = Reducer.reduce(forgotten, .submitPairingLink(text: "https://mm2.tail1a2b3c.ts.net:8443/#pair=K7M2QX9H")).state
        #expect(pairing.pairing == .connecting)
    }

    @Test func discardAndForgetDoesNothingOutsideTheRefusal() throws {
        var s = removedWithWork()
        s.pairing = .paired
        #expect(Reducer.reduce(s, .discardUnsentAndForget).state == s)
    }

    @Test func discardAndPairDoesNothingWithoutTheQuestion() {
        let s = removedWithWork()
        #expect(Reducer.reduce(s, .discardUnsentAndPair).state == s)
    }

    @Test func sendItFirstInThePairedDialogSendsAndClosesIt() throws {
        let s = try Fixture.named("pair-blocked").state
        let next = Reducer.reduce(s, .retryNow(at: Fixture.now + 1)).state
        #expect(next.pairingProblem == nil)
    }
}
