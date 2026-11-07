import XCTest
@testable import SheprdRemoteCore

/// Keep Generic ends what names a chat, on the phone, whatever a computer
/// that has not heard yet sent.
final class ActivityGenericTests: XCTestCase {
    private func facts(title: String) -> ChatActivityFacts {
        ChatActivityFacts(computer: "studio", computerId: "AB", workspace: "studio", chatTitle: title, agent: "Claude", pane: "p")
    }

    func testAnActivityWithATitleNamesItsChat() {
        XCTAssertTrue(facts(title: "remote plan").namesChat)
    }

    func testAGenericActivityHasNoTitleToHide() {
        XCTAssertFalse(facts(title: "").namesChat)
    }
}
