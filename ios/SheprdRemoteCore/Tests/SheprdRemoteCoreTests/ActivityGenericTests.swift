import XCTest
@testable import SheprdRemoteCore

/// Keep Generic ends what names a chat, on the phone, whatever a computer
/// that has not heard yet sent.
final class ActivityGenericTests: XCTestCase {
    private func facts(title: String, workspace: String = "studio") -> ChatActivityFacts {
        ChatActivityFacts(computer: "studio", computerId: "AB", workspace: workspace, chatTitle: title, agent: "Claude", pane: "p")
    }

    func testAnActivityWithATitleNamesItsChat() {
        XCTAssertTrue(facts(title: "remote plan").namesChat)
    }

    func testAnUntitledActivityInAWorkspaceStillNamesItsChat() {
        XCTAssertTrue(facts(title: "", workspace: "herdr-gpui").namesChat)
    }

    func testAGenericActivityHasNoTitleToHide() {
        XCTAssertFalse(facts(title: "").namesChat)
    }
}
