import RichFilesCore
import XCTest
@testable import RichGlance

final class GlanceHelperSessionTests: XCTestCase {
    func testDisabledLoginItemFallsBackToTheRichOSSession() {
        let optedIn = GlanceHelper.Preferences.initial.settingBackground(true, firstUse: true)
        XCTAssertTrue(GlanceHelperControl.shouldRunSession(optedIn, loginEnabled: false))
        XCTAssertFalse(GlanceHelperControl.shouldRunSession(optedIn, loginEnabled: true))

        var shortcutOff = optedIn
        shortcutOff.enabled = false
        XCTAssertFalse(GlanceHelperControl.shouldRunSession(shortcutOff, loginEnabled: false))
        XCTAssertTrue(GlanceHelperControl.shouldRunSession(optedIn.settingBackground(false, firstUse: false),
                                                         loginEnabled: false))
    }
}
