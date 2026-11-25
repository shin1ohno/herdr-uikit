import XCTest

/// Real input for examples/native_inline_field_probe (launched by
/// scripts/check-native-search.py with its stderr captured). The fixture
/// decides every verdict. Element queries may not answer while a presented
/// controller is up, so after the handshake tap this test acts on a fixed
/// schedule, each step later than the fixture needs to get ready (the
/// fixture waits up to 150 s for each step's effect).
///
/// The field covers the fixture's label, which GPUI lays out at
/// (200, 322.5) pt, 99 x 22.5 pt, in the drawing view: the field is
/// (196, 320.5) 120 x 26.5 pt. On the 13-inch iPad Simulator (1032 x 1376 pt,
/// portrait) its middle is at about (0.248, 0.243). Escape is not typed: XCUITest's Escape
/// never reached a presented field (docs/native-search.md); the fixture sends
/// the field's own press selector instead.
final class InlineField: XCTestCase {
    let bundle = ProcessInfo.processInfo.environment["NATIVE_SEARCH_BUNDLE"]
        ?? "be.ohno.herdruikit.nativeinlinefieldprobe"

    func phase(_ app: XCUIApplication, _ value: String, _ timeout: TimeInterval) -> Bool {
        let query = app.staticTexts.matching(
            NSPredicate(format: "identifier == 'premise-phase' AND label == %@", value))
        let found = query.firstMatch.waitForExistence(timeout: timeout)
        print("INLINE_FIELD_QA phase \(value) found=\(found)")
        return found
    }

    func step(_ seconds: TimeInterval, _ name: String, _ action: () -> Void) {
        Thread.sleep(forTimeInterval: seconds)
        print("INLINE_FIELD_QA step \(name)")
        action()
    }

    func testProbe() {
        continueAfterFailure = true
        let app = XCUIApplication(bundleIdentifier: bundle)
        app.activate()
        guard phase(app, "await-tap", 400) else { return }
        // The handshake: GPUI's view, away from the label.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.6)).tap()
        let field = app.coordinate(withNormalizedOffset: CGVector(dx: 0.248, dy: 0.243))
        // s1: a tap on the field (keyboard focus), Command-A, a new name,
        // Return.
        step(6, "s1-field-tap") { field.tap() }
        step(3, "s1-select-all") { app.typeKey("a", modifierFlags: .command) }
        step(3, "s1-type") { app.typeText("abc") }
        step(2, "s1-return") { app.typeText(XCUIKeyboardKey.return.rawValue) }
        // s2: a tap outside the field, on GPUI's view above it (the lower
        // part of the screen is the software keyboard while the field is up).
        step(12, "s2-outside-tap") {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.7, dy: 0.15)).tap()
        }
        // s3: Japanese romaji. Control-Space switches the input source
        // (ja_JP-Romaji is installed by check-native-search.py); the first
        // Return commits a composition, the second the name.
        step(12, "s3-field-tap") { field.tap() }
        step(2, "s3-switch-input-source") { app.typeKey(" ", modifierFlags: .control) }
        step(3, "s3-type") { app.typeText("puro") }
        step(3, "s3-return-1") { app.typeText(XCUIKeyboardKey.return.rawValue) }
        step(3, "s3-return-2") { app.typeText(XCUIKeyboardKey.return.rawValue) }
        step(3, "s3-switch-back") { app.typeKey(" ", modifierFlags: .control) }
        XCTAssertTrue(phase(app, "done", 300))
    }
}
