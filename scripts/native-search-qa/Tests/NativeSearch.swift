import XCTest

/// Real input for examples/native_search_probe (launched by
/// scripts/check-native-search.py with its stderr captured). The fixture
/// decides every verdict. Element queries do not answer while a presented
/// panel is up in this Simulator, so after the handshake tap this test acts
/// on a fixed schedule, each step later than the fixture needs to get ready.
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
        // s1: Command-B while the panel is up, a sentinel, then Return.
        step(8, "command-b-open") { app.typeKey("b", modifierFlags: .command) }
        step(1, "sentinel") { app.typeText("#") }
        step(2, "return") { app.typeKey(XCUIKeyboardKey.return, modifierFlags: []) }
        // Positive control: Command-B after the panel closed.
        step(6, "command-b-closed") { app.typeKey("b", modifierFlags: .command) }
        step(8, "outside-tap") {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.97, dy: 0.97)).tap()
        }
        step(8, "sheet-swipe") {
            let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.53))
            let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.99))
            start.press(forDuration: 0.1, thenDragTo: end)
        }
        step(8, "escape") { app.typeKey(XCUIKeyboardKey.escape, modifierFlags: []) }
        XCTAssertTrue(phase(app, "done", 120))
    }
}
