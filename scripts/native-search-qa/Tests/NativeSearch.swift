import XCTest

/// Real input for examples/native_search_probe (launched by
/// scripts/check-native-search.py with its stderr captured). The fixture
/// decides every verdict. Element queries do not answer while a presented
/// panel is up in this Simulator, so after the handshake tap this test acts
/// on a fixed schedule, each step later than the fixture needs to get ready.
/// Return goes through typeText, which types into the element with keyboard
/// focus: a Return press from typeKey did not reach the presented field's
/// textFieldShouldReturn.
final class NativeSearch: XCTestCase {
    let bundle = ProcessInfo.processInfo.environment["NATIVE_SEARCH_BUNDLE"]
        ?? "be.ohno.herdruikit.nativesearchprobe"

    func phase(_ app: XCUIApplication, _ value: String, _ timeout: TimeInterval) -> Bool {
        let query = app.staticTexts.matching(
            NSPredicate(format: "identifier == 'premise-phase' AND label == %@", value))
        let found = query.firstMatch.waitForExistence(timeout: timeout)
        print("NATIVE_SEARCH_QA phase \(value) found=\(found)")
        return found
    }

    func step(_ seconds: TimeInterval, _ name: String, _ action: () -> Void) {
        Thread.sleep(forTimeInterval: seconds)
        print("NATIVE_SEARCH_QA step \(name)")
        action()
    }

    func testProbe() {
        continueAfterFailure = true
        let app = XCUIApplication(bundleIdentifier: bundle)
        app.activate()
        guard phase(app, "await-tap", 300) else { return }
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.6)).tap()
        // s1: Shift-Command-B (the app's menu command) while the panel is up,
        // a sentinel, then Return. The tap lands on the popover's field (top
        // at y 62, field 48 to 84 pt below): on some runs the field's
        // software keyboard never appeared after the handshake tap, and no
        // typed key reached the app.
        step(6, "field-tap") {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.193, dy: 0.093)).tap()
        }
        step(4, "menu-shortcut-open") { app.typeKey("b", modifierFlags: [.command, .shift]) }
        // The software keyboard changes state for about 2 s after a hardware
        // key; text typed meanwhile was lost on some runs.
        step(4, "sentinel") { app.typeText("#") }
        step(2, "return") { app.typeText(XCUIKeyboardKey.return.rawValue) }
        // Positive control: Shift-Command-B after the panel closed.
        step(6, "menu-shortcut-closed") { app.typeKey("b", modifierFlags: [.command, .shift]) }
        step(8, "outside-tap") {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.97, dy: 0.97)).tap()
        }
        // A sheet opens at its medium detent (top at y 606 of 1376 pt on this
        // iPad) and rises to the large one (top at y 42, x 48 to 984) on some
        // runs, when the software keyboard appears. The fixture pins each
        // sheet to the large detent; every sheet gesture starts on its
        // grabber (y 50) or outside the sheet: a drag starting in the table
        // hands over from scrolling at an unpredictable point. A completed
        // swipe, then a tap on the dimmed margin left of a new sheet (it takes
        // no touches). Escape is not typed: typeKey's Escape never reached the
        // field (its pressesBegan counted no such press).
        // An abandoned swipe is not scheduled: when a drag begins the
        // keyboard hides and the sheet falls to the medium detent by itself,
        // so whether a short drag is abandoned or completed varied by run.
        step(8, "sheet-swipe") {
            let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.036))
            let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.99))
            start.press(forDuration: 0.1, thenDragTo: end, withVelocity: 1500,
                        thenHoldForDuration: 0)
        }
        step(8, "sheet-dimmed-tap") {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.02, dy: 0.5)).tap()
        }
        XCTAssertTrue(phase(app, "done", 120))
    }
}
