import XCTest

/// Real keys and taps for examples/input_mode_probe (launched by
/// scripts/check-native-search.py with its stderr captured). The fixture
/// decides every verdict. It asks for each action in its phase label as
/// `act-<n>-<verb>`; this test performs each numbered request once, until the
/// label says `closing`, `done` or `failed`. The keys reach the Simulator as
/// hardware keyboard input: Shift puts it in the hardware keyboard state,
/// and Control-Space switches the input source.
final class InputMode: XCTestCase {
    let bundle = ProcessInfo.processInfo.environment["NATIVE_SEARCH_BUNDLE"]
        ?? "be.ohno.herdruikit.inputmodeprobe"

    func label(_ app: XCUIApplication) -> String? {
        let element = app.staticTexts.matching(identifier: "premise-phase").firstMatch
        return element.exists ? element.label : nil
    }

    func testInputMode() {
        continueAfterFailure = true
        let app = XCUIApplication(bundleIdentifier: bundle)
        app.activate()
        var last = 0
        var finished = false
        let deadline = Date().addingTimeInterval(1500)
        while Date() < deadline {
            // The fixture exits after an assertion; nothing is left to do.
            if app.state == .notRunning {
                print("INPUT_MODE_QA fixture not running")
                break
            }
            guard let phase = label(app) else {
                Thread.sleep(forTimeInterval: 0.2)
                continue
            }
            if phase == "closing" || phase == "done" || phase == "failed" {
                print("INPUT_MODE_QA phase \(phase)")
                finished = phase != "failed"
                break
            }
            let parts = phase.split(separator: "-")
            guard parts.count == 3, parts[0] == "act", let n = Int(parts[1]), n > last else {
                Thread.sleep(forTimeInterval: 0.1)
                continue
            }
            last = n
            print("INPUT_MODE_QA act \(n) \(parts[2])")
            switch parts[2] {
            case "shift":
                app.typeKey(XCUIKeyboardKey.shift.rawValue, modifierFlags: [])
            case "switch":
                app.typeKey(" ", modifierFlags: .control)
            case "tap":
                // GPUI's view, away from the caret and the reference field.
                app.coordinate(withNormalizedOffset: CGVector(dx: 0.75, dy: 0.2)).tap()
            default:
                print("INPUT_MODE_QA unknown verb \(parts[2])")
            }
        }
        XCTAssertTrue(finished, "fixture finished its requests")
    }
}
