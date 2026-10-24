import XCTest

/// The visible controls, driven the way a finger drives them (build plan §5.1 I2): compose and send,
/// hold and release, slide left to cancel, slide up to lock then send, slide up to lock then cancel —
/// and the conversation following the newest message unless the reader scrolls up, with every send
/// resuming it (PRD §5; ledger §2.8 M2 with RichOS's resume-on-send).
final class InteractionTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// Every row on screen, read from ONE snapshot of the app's hierarchy. `allElementsBoundByIndex`
    /// resolves each element with its own query, so a list that moves between two resolutions (a row
    /// inserted, then a card raising the bottom inset and scrolling rows off the top on the iPhone SE)
    /// fails with "No matches found for Element at index N". A snapshot's values are all taken at once.
    private func rowSnapshots(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) -> [XCUIElementSnapshot] {
        let root: XCUIElementSnapshot
        do {
            root = try app.snapshot()
        } catch {
            XCTFail("could not read the screen: \(error)", file: file, line: line)
            return []
        }
        var found: [XCUIElementSnapshot] = []
        var pending = root.children
        while let node = pending.popLast() {
            if node.identifier.hasPrefix("row.") { found.append(node) }
            pending.append(contentsOf: node.children)
        }
        return found
    }

    private func rows(_ app: XCUIApplication) -> Set<String> {
        Set(rowSnapshots(app).map(\.identifier))
    }

    /// The lowest row on screen: the newest message, while the list follows it.
    private func newestRow(_ app: XCUIApplication) -> String? {
        rowSnapshots(app).max { $0.frame.maxY < $1.frame.maxY }?.identifier
    }

    /// Nothing was sent: no row the list did not show before, and the newest message is still the one
    /// it was. Not "the same rows on screen": a hint or card above the composer raises the list's
    /// bottom inset and the following list keeps its newest message in view (R3), so on the iPhone SE
    /// the oldest row on screen scrolls off the top, which sends nothing.
    private func assertNothingSent(_ app: XCUIApplication, rows before: Set<String>, newest: String?, _ message: String,
                                   file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(rows(app).subtracting(before), [], message, file: file, line: line)
        XCTAssertEqual(newestRow(app), newest, message, file: file, line: line)
    }

    /// Once the hint or card is gone, the list shows exactly the rows it showed before: what scrolled
    /// off the top was moved, not lost.
    private func assertRowsReturn(_ app: XCUIApplication, to before: Set<String>, _ message: String,
                                  file: StaticString = #filePath, line: UInt = #line) {
        let deadline = Date().addingTimeInterval(3)
        while Date() < deadline, rows(app) != before { Thread.sleep(forTimeInterval: 0.1) }
        XCTAssertEqual(rows(app), before, message, file: file, line: line)
    }

    private func assertNewVoice(_ app: XCUIApplication, after before: Set<String>,
                                file: StaticString = #filePath, line: UInt = #line) {
        // UICollectionView recycles off-screen rows. Its visible count need not grow on Send.
        let deadline = Date().addingTimeInterval(3)
        while Date() < deadline, rows(app).subtracting(before).isEmpty { Thread.sleep(forTimeInterval: 0.1) }
        let added = rows(app).subtracting(before)
        XCTAssertEqual(added.count, 1, "expected one new message identity", file: file, line: line)
        guard let id = added.first else { return }
        let message = app.descendants(matching: .any)[id]
        XCTAssertTrue(message.label.hasPrefix("Your voice message"), "new row is not a voice message", file: file, line: line)
        XCTAssertTrue(message.isHittable, "the conversation did not follow the sent voice message", file: file, line: line)
    }

    private func orb(_ app: XCUIApplication) -> XCUIElement { app.descendants(matching: .any)["composer.mic"] }

    /// Interactive fixtures supply a controlled microphone answer and recording effects.
    /// These checks prove real gestures into the production core, not physical audio capture.
    /// A broken fixture fails the test instead of silently skipping the required gesture.
    private func requireMicrophoneKnownToCore(_ app: XCUIApplication) throws {
        let granted = app.descendants(matching: .any)["debug.microphone.granted"]
        guard granted.waitForExistence(timeout: 3) else {
            let seen = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'debug.microphone.'")).firstMatch.identifier
            XCTFail("interactive fixture microphone is '\(seen)', expected granted")
            return
        }
    }

    func testComposeAndSendAddsTheMessageAndFollowsIt() {
        let app = Screen.launch("comp-idle")
        let field = app.textFields["composer.field"].exists ? app.textFields["composer.field"] : app.textViews["composer.field"]
        assertOnScreenAndHittable(field, in: app, "the message field")
        field.tap()
        field.typeText("Move the Friday review to 3 PM.")
        let send = app.descendants(matching: .any)["composer.send"]
        assertOnScreenAndHittable(send, in: app, "Send")
        send.tap()
        let sent = app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS 'Move the Friday review to 3 PM.'")).firstMatch
        XCTAssertTrue(sent.waitForExistence(timeout: 3), "the sent message is not in the conversation")
        XCTAssertTrue(sent.isHittable, "the conversation did not follow the message just sent")
        XCTAssertTrue(orb(app).waitForExistence(timeout: 2), "the microphone did not come back after sending")
    }

    func testHoldAndReleaseSendsAVoiceMessage() throws {
        let app = Screen.launch("comp-idle", interactive: true)
        try requireMicrophoneKnownToCore(app)
        let before = rows(app)
        let mic = orb(app)
        assertOnScreenAndHittable(mic, in: app, "the microphone")
        mic.press(forDuration: 1.4)
        assertNewVoice(app, after: before)
    }

    func testATapIsTooShortAndSendsNothing() throws {
        let app = Screen.launch("comp-idle", interactive: true)
        try requireMicrophoneKnownToCore(app)
        let before = rows(app)
        let newest = newestRow(app)
        orb(app).tap()
        let line = app.staticTexts["composer.toast"]
        XCTAssertTrue(line.waitForExistence(timeout: 2), "no hint after a tap")
        assertNothingSent(app, rows: before, newest: newest, "a tap sent something")
        // XCTest's accessibility lookup can consume most of the display interval.
        // TooShortLineTests covers the latch; this check proves appearance and disappearance.
        let gone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: line)
        XCTAssertEqual(XCTWaiter.wait(for: [gone], timeout: 3), .completed, "the too-short line never left")
        assertRowsReturn(app, to: before, "the rows the hint moved did not come back after it left")
    }

    func testSlideLeftCancelsAndSendsNothing() throws {
        let app = Screen.launch("comp-idle", interactive: true)
        try requireMicrophoneKnownToCore(app)
        let before = rows(app)
        let mic = orb(app)
        let start = mic.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        // The positive probe: a hold alone records (the timer appears), so "nothing was sent" below
        // cannot pass merely because no recording ever began.
        mic.press(forDuration: 0.9)
        Thread.sleep(forTimeInterval: 1.0)
        assertNewVoice(app, after: before)
        let afterHold = rows(app)
        start.press(forDuration: 0.6, thenDragTo: start.withOffset(CGVector(dx: -200, dy: 0)))
        Thread.sleep(forTimeInterval: 1.2)  // the bin ritual
        XCTAssertEqual(rows(app), afterHold, "sliding left sent a message")
        XCTAssertTrue(orb(app).exists, "the microphone did not come back after the cancel")
    }

    func testSlideUpLocksThenSendSends() throws {
        let app = Screen.launch("comp-idle", interactive: true)
        try requireMicrophoneKnownToCore(app)
        let before = rows(app)
        let start = orb(app).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        start.press(forDuration: 0.6, thenDragTo: start.withOffset(CGVector(dx: 0, dy: -110)))
        let cancel = app.buttons["voice.cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 2), "sliding up did not lock")
        // Cancel sits at exactly the center of the capsule (round-12 NOTES "Locked layout").
        let windowMid = app.windows.firstMatch.frame.midX
        XCTAssertEqual(cancel.frame.midX, windowMid, accuracy: 2)
        Thread.sleep(forTimeInterval: 1.0)  // still recording, hands free
        XCTAssertEqual(rows(app), before, "lifting the finger after locking sent the message")
        let send = app.descendants(matching: .any)["voice.send"]
        assertOnScreenAndHittable(send, in: app, "the locked Send circle")
        send.tap()
        assertNewVoice(app, after: before)
    }

    func testSlideUpLocksThenCancelSendsNothing() throws {
        let app = Screen.launch("comp-idle", interactive: true)
        try requireMicrophoneKnownToCore(app)
        let before = rows(app)
        let start = orb(app).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        start.press(forDuration: 0.6, thenDragTo: start.withOffset(CGVector(dx: 0, dy: -110)))
        let cancel = app.buttons["voice.cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 2), "sliding up did not lock")
        cancel.tap()
        Thread.sleep(forTimeInterval: 1.3)  // the locked-cancel ritual, 950 ms
        XCTAssertEqual(rows(app), before, "Cancel sent a message")
        XCTAssertFalse(app.buttons["voice.cancel"].exists)
    }

    func testBackgroundKeepsLockedVoiceAndReturnAllowsANewRecording() throws {
        let app = Screen.launch("comp-idle", interactive: true)
        try requireMicrophoneKnownToCore(app)
        let before = rows(app)
        let start = orb(app).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        start.press(forDuration: 0.6, thenDragTo: start.withOffset(CGVector(dx: 0, dy: -110)))
        XCTAssertTrue(app.buttons["voice.cancel"].waitForExistence(timeout: 2))
        XCUIDevice.shared.press(.home)
        XCTAssertTrue(app.wait(for: .runningBackground, timeout: 5))
        app.activate()
        XCTAssertTrue(app.buttons["kept.send"].waitForExistence(timeout: 3), "backgrounding lost the unsent recording")
        XCTAssertFalse(app.buttons["voice.cancel"].exists, "capture restarted on return")
        XCTAssertTrue(rows(app).subtracting(before).isEmpty, "backgrounding or returning sent the recording")
        app.buttons["kept.discard"].tap()
        XCTAssertFalse(app.buttons["kept.send"].exists)
        let returned = rows(app)
        orb(app).press(forDuration: 1.4)
        assertNewVoice(app, after: returned)
    }

    func testReadingOlderShowsLatestAndSendingResumesFollowing() {
        let app = Screen.launch("conv-scrolled")
        let latest = app.buttons["conversation.latest"]
        XCTAssertTrue(latest.waitForExistence(timeout: 3), "no Latest pill while reading older messages")
        let field = app.textFields["composer.field"].exists ? app.textFields["composer.field"] : app.textViews["composer.field"]
        field.tap()
        field.typeText("Thanks.")
        app.descendants(matching: .any)["composer.send"].tap()
        let sent = app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS 'Thanks.'")).firstMatch
        XCTAssertTrue(sent.waitForExistence(timeout: 3))
        let deadline = Date().addingTimeInterval(3)
        while Date() < deadline, !sent.isHittable { Thread.sleep(forTimeInterval: 0.1) }
        XCTAssertTrue(sent.isHittable, "sending did not return to the newest message")
    }

    func testScrollingUpShowsLatestAndLatestReturns() {
        let app = Screen.launch("conv-populated")
        let list = app.collectionViews["conversation.list"]
        XCTAssertTrue(list.waitForExistence(timeout: 3))
        list.swipeDown()
        let latest = app.buttons["conversation.latest"]
        let showsLatest = latest.waitForExistence(timeout: 3)
        if !showsLatest, app.state == .runningForeground {
            keepScreenshot(app, name: "scrolling-up-missing-latest")
            print(app.debugDescription)
        }
        XCTAssertTrue(showsLatest, "scrolling up did not offer Latest")
        latest.tap()
        let newest = app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS 'Short it is.'")).firstMatch
        let deadline = Date().addingTimeInterval(3)
        while Date() < deadline, !newest.isHittable { Thread.sleep(forTimeInterval: 0.1) }
        XCTAssertTrue(newest.isHittable, "Latest did not return to the newest message")
    }

    // MARK: pairing v2 — the press on the phone, then the wait for the press on the Mac

    /// "They match" is a real tap into the production core: the words stay up, the button that was
    /// pressed goes, the screen names the press still missing, and "They do not match" is the way out.
    func testTheyMatchWaitsForTheMacAndTheyDoNotMatchLeaves() {
        let app = Screen.launch("pair-words", interactive: true)
        let match = app.buttons["pair.match"]
        assertOnScreenAndHittable(match, in: app, "They match")
        match.tap()
        let waiting = app.descendants(matching: .any)["takeover.pair-awaiting-mac"]
        XCTAssertTrue(waiting.waitForExistence(timeout: 3), "They match did not lead to the wait for the Mac")
        XCTAssertTrue(app.staticTexts["Now press They match on your Mac"].exists, "the screen does not name the missing press")
        XCTAssertTrue(app.descendants(matching: .any)["pair.words"].exists, "the words left the screen while waiting")
        XCTAssertFalse(app.buttons["pair.match"].exists, "They match can be pressed twice")
        assertEveryButtonNamed(app)
        let noMatch = app.buttons["pair.noMatch"]
        assertOnScreenAndHittable(noMatch, in: app, "They do not match while waiting")
        noMatch.tap()
        XCTAssertTrue(app.descendants(matching: .any)["takeover.pair-intro"].waitForExistence(timeout: 3),
                      "They do not match did not end the pairing")
        XCTAssertFalse(app.descendants(matching: .any)["pair.words"].exists, "the words outlived the pairing")
        // The one security decision in pairing ends on a card that says so, not a plain intro.
        let card = app.descendants(matching: .any)["pair.error"]
        XCTAssertTrue(card.waitForExistence(timeout: 3) && card.label.contains("Stopped, and nothing was paired"),
                      "They do not match looks like a reset: \(card.exists ? card.label : "no card")")
        XCTAssertTrue(app.buttons["pair.scan"].exists, "pairing afresh is not offered")
    }

    /// Coming to the front is what resumes the wait, so it is where a wait whose bound has passed
    /// makes its last ask. Interactive fixtures run the real clock; the fixture's deadline is the
    /// fixtures' morning (2026-09-22), long past, so the launch asks once more, the stand-in Mac
    /// cannot be reached, and the wait ends truthfully. "Scan your Mac's code" is a real tap that
    /// opens the scanner, and backing out leaves the card where it was.
    func testAWaitPastItsBoundSaysSoAndTheScannerIsTheWayBack() {
        let app = Screen.launch("pair-awaiting-mac", interactive: true, mac: "unreachable")
        let card = app.descendants(matching: .any)["pair.error"]
        XCTAssertTrue(card.waitForExistence(timeout: 5), "the ended wait is not explained")
        XCTAssertTrue(card.label.contains("Pairing timed out"), "unexpected explanation: \(card.label)")
        XCTAssertFalse(app.descendants(matching: .any)["takeover.pair-awaiting-mac"].exists, "still claims to be waiting")
        XCTAssertFalse(app.buttons["pair.again"].exists, "\"Pair again\" on a phone that was never paired")
        let scan = app.buttons["pair.scan"]
        assertOnScreenAndHittable(scan, in: app, "Scan your Mac's code")
        scan.tap()
        XCTAssertTrue(app.descendants(matching: .any)["scanner"].waitForExistence(timeout: 3), "the scan button did not open the scanner")
        // D7 (the iPhone walk, 2026-10-01): the scanner's identifier replaced its children's, so these
        // were `scanner`. Each control answers to its own identifier and keeps its spoken name.
        let close = app.buttons["scanner.close"]
        assertOnScreenAndHittable(close, in: app, "Close the scanner")
        XCTAssertEqual(close.label, "Close the scanner")
        let link = app.buttons["scanner.link"]
        assertOnScreenAndHittable(link, in: app, "Use a pairing link instead")
        XCTAssertEqual(link.label, "Use a pairing link instead")
        keepScreenshot(app, name: "d7-scanner")
        close.tap()
        XCTAssertTrue(card.waitForExistence(timeout: 3) && card.label.contains("Pairing timed out"),
                      "backing out of the scanner lost the explanation")
    }

    /// Leaving for the Home Screen and coming back is a real round trip through the app's lifecycle:
    /// the return ends a wait whose bound has passed, and nothing is left saying "waiting".
    func testLeavingAndReturningWhileWaitingIsTruthful() {
        let app = Screen.launch("pair-words", interactive: true)
        app.buttons["pair.match"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["takeover.pair-awaiting-mac"].waitForExistence(timeout: 3))
        XCUIDevice.shared.press(.home)
        XCTAssertTrue(app.wait(for: .runningBackground, timeout: 5))
        app.activate()
        // No Mac answers in an interactive fixture, so the wait is still inside its bound: it resumes
        // and still says which press is missing.
        XCTAssertTrue(app.descendants(matching: .any)["takeover.pair-awaiting-mac"].waitForExistence(timeout: 3),
                      "the wait did not resume on return")
        XCTAssertTrue(app.buttons["pair.noMatch"].isHittable)
    }

    /// D03 (native acceptance r1): with the microphone off, the card is the answer to a press and to
    /// nothing else; "Not now" takes it down; the next press brings it back. A real press on the real
    /// orb into the production core; the microphone's answer is the interactive fixture's stand-in.
    func testAPressWithTheMicrophoneOffShowsTheCardAndNotNowDismissesIt() {
        let app = Screen.launch("comp-idle", interactive: true, microphone: "denied")
        XCTAssertTrue(app.descendants(matching: .any)["debug.microphone.denied"].waitForExistence(timeout: 3),
                      "the core was not told the microphone is off")
        let card = app.descendants(matching: .any)["card.micDenied"]
        XCTAssertFalse(card.exists, "the card is up before any press")
        let before = rows(app)
        let newest = newestRow(app)
        orb(app).press(forDuration: 0.4)
        XCTAssertTrue(card.waitForExistence(timeout: 3), "a press with the microphone off shows nothing")
        XCTAssertEqual(card.label, "The microphone is off for RichConnect, Turn it on in iPhone Settings to send voice messages, or type instead.")
        assertOnScreenAndHittable(app.buttons["card.openSettings"], in: app, "Open Settings")
        assertNothingSent(app, rows: before, newest: newest, "a press with the microphone off sent something")
        let notNow = app.buttons["card.micNotNow"]
        assertOnScreenAndHittable(notNow, in: app, "Not now")
        notNow.tap()
        XCTAssertTrue(card.waitForNonExistence(timeout: 3), "Not now did not take the card down")
        assertRowsReturn(app, to: before, "the rows the card moved did not come back after Not now")
        orb(app).press(forDuration: 0.4)
        XCTAssertTrue(card.waitForExistence(timeout: 3), "the next press did not bring the card back")
        keepScreenshot(app, name: "d03-mic-denied-after-press")
    }

    /// I05 (native acceptance r1, the physical iPhone): with another card already showing, the
    /// microphone-off card a press raises was laid out behind the message field. The region above the
    /// composer holds at most 40% of the screen and scrolls; on the iPhone SE the kept-recording card
    /// and this one do not fit together, so the new card must be brought into view.
    func testTheMicrophoneOffCardIsInViewWhenAnotherCardIsShowing() {
        let app = Screen.launch("rec-card", interactive: true, microphone: "denied")
        XCTAssertTrue(app.descendants(matching: .any)["debug.microphone.denied"].waitForExistence(timeout: 3),
                      "the core was not told the microphone is off")
        XCTAssertTrue(app.buttons["kept.send"].waitForExistence(timeout: 3), "the kept recording's card is not showing first")
        orb(app).press(forDuration: 0.4)
        let settings = app.buttons["card.openSettings"]
        let notNow = app.buttons["card.micNotNow"]
        XCTAssertTrue(settings.waitForExistence(timeout: 3), "a press with the microphone off shows nothing")
        let composerTop = app.textFields["composer.field"].frame.minY
        // Let the bring-into-view settle (Motion.card), then measure where the card's actions are.
        let deadline = Date().addingTimeInterval(3)
        while Date() < deadline, notNow.frame.maxY > composerTop { Thread.sleep(forTimeInterval: 0.1) }
        XCTAssertLessThanOrEqual(settings.frame.maxY, composerTop, "Open Settings is behind the message field")
        XCTAssertLessThanOrEqual(notNow.frame.maxY, composerTop, "Not now is behind the message field")
        assertOnScreenAndHittable(settings, in: app, "Open Settings")
        assertOnScreenAndHittable(notNow, in: app, "Not now")
        keepScreenshot(app, name: "i05-mic-denied-under-a-second-card")
    }

    func testSettingsOpensAndForgetAsksFirst() {
        let app = Screen.launch("comp-idle")
        app.buttons["header.settings"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["settings.sheet"].waitForExistence(timeout: 3))
        app.buttons["settings.forget"].tap()
        XCTAssertTrue(app.buttons["forget.keep"].waitForExistence(timeout: 3), "Forget did not ask first")
        app.buttons["forget.keep"].tap()
        XCTAssertFalse(app.buttons["forget.confirm"].exists)
    }

    /// I04 (native acceptance r1, the physical iPhone SE): "Not now" on the notification offer left
    /// Settings with the words "Rich cannot reach you when the app is closed." and no control, and the
    /// offer never returns, so nothing in the app could turn notifications on. "Not now" is still
    /// respected (the offer does not come back), and Settings keeps an off switch whose real tap asks
    /// for notifications at that moment, the way Android's Settings does. The interactive fixture runs
    /// the production core with the platform's answers withheld, so the tap ends at "Turning on…",
    /// the state in which iOS's own question is asked.
    func testNotNowLeavesASettingsSwitchThatTurnsNotificationsOn() {
        let app = Screen.launch("notif-offer", interactive: true)
        let notNow = app.buttons["card.notNow"]
        assertOnScreenAndHittable(notNow, in: app, "the offer's Not now")
        notNow.tap()
        XCTAssertTrue(app.buttons["card.notificationsOn"].waitForNonExistence(timeout: 3), "Not now did not take the offer down")
        app.buttons["header.settings"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["settings.sheet"].waitForExistence(timeout: 3), "Settings did not open")
        let toggle = app.switches["settings.notifications"]
        assertOnScreenAndHittable(toggle, in: app, "Notify me when Rich replies")
        XCTAssertEqual(toggle.value as? String, "0", "the switch after Not now must read off")
        XCTAssertEqual(toggle.label, "Notify me when Rich replies")
        keepScreenshot(app, name: "i04-settings-after-not-now")
        toggle.tap()
        let turning = app.staticTexts.matching(NSPredicate(format: "label CONTAINS 'Turning on'")).firstMatch
        XCTAssertTrue(turning.waitForExistence(timeout: 3), "the switch's tap did not ask for notifications")
    }
}
