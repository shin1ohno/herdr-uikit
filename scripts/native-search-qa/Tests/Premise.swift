import XCTest

/// Drives the already-running premise fixture (launched by
/// scripts/check-native-search.py with its stderr captured) with real
/// synthesized key events. The fixture decides every verdict; this test only
/// types when the fixture's phase label asks for it.
final class Premise: XCTestCase {
    let bundle = ProcessInfo.processInfo.environment["NATIVE_SEARCH_BUNDLE"]
        ?? "be.ohno.herdruikit.nativesearchpremise"

    func phase(_ app: XCUIApplication, _ value: String, _ timeout: TimeInterval = 90) -> Bool {
        let query = app.staticTexts.matching(
            NSPredicate(format: "identifier == 'premise-phase' AND label == %@", value))
        let found = query.firstMatch.waitForExistence(timeout: timeout)
        print("NATIVE_SEARCH_QA phase \(value) found=\(found)")
        return found
    }

    /// A phase shown in the presented field's placeholder.
    func fieldPhase(_ app: XCUIApplication, _ value: String, _ timeout: TimeInterval = 90) -> XCUIElement? {
        let field = app.searchFields.matching(
            NSPredicate(format: "identifier == 'premise-search-field' AND placeholderValue == %@", value)).firstMatch
        let found = field.waitForExistence(timeout: timeout)
        print("NATIVE_SEARCH_QA field phase \(value) found=\(found)")
        if !found {
            dump(app)
        }
        return found ? field : nil
    }

    /// Prints the application's accessibility snapshot, one element per line.
    func dump(_ app: XCUIApplication) {
        guard let root = try? app.snapshot() else {
            print("NATIVE_SEARCH_QA tree unavailable")
            return
        }
        func walk(_ element: XCUIElementSnapshot, _ depth: Int) {
            print("NATIVE_SEARCH_QA tree \(String(repeating: " ", count: depth)) type=\(element.elementType.rawValue) id=\(element.identifier) label=\(element.label) placeholder=\(element.placeholderValue ?? "nil") value=\(String(describing: element.value)) focus=\(element.hasFocus)")
            for child in element.children { walk(child, depth + 1) }
        }
        walk(root, 0)
    }

    func testPremise() {
        continueAfterFailure = true
        let app = XCUIApplication(bundleIdentifier: bundle)
        app.activate()
        if phase(app, "await-tap", 300) {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.6)).tap()
        }
        guard phase(app, "await-keys-gpui") else { return }
        // Typing "g" makes the fixture present the popover. While a presented
        // controller is up, element queries do not answer in this Simulator,
        // so the rest types into whatever has keyboard focus, on a schedule
        // longer than the fixture's own steps; the fixture waits for '#'.
        app.typeText("g")
        for wait in [8.0, 12.0] {
            Thread.sleep(forTimeInterval: wait)
            UIPasteboard.general.string = "UCLIP"
            app.typeText("pro")
            app.typeKey("v", modifierFlags: .command)
            app.typeText("#")
        }
        Thread.sleep(forTimeInterval: 4)
        app.typeText("nihon")
        Thread.sleep(forTimeInterval: 2)
        app.typeText("#")
        XCTAssertTrue(phase(app, "done", 120))
    }
}
