import XCTest
@testable import HerdrRemoteCore

/// The fixtures are the payloads the daemon's tests produce
/// (`crates/herdr-remote-control/src/apns/tests/payload.rs`).
final class ActivityTests: XCTestCase {
    private func aps(_ name: String) throws -> [String: Any] {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
        let root = try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any]
        return try XCTUnwrap(root?["aps"] as? [String: Any])
    }

    private func decode<T: Decodable>(_ type: T.Type, _ object: Any?) throws -> T {
        let data = try JSONSerialization.data(withJSONObject: try XCTUnwrap(object))
        return try JSONDecoder().decode(type, from: data)
    }

    func testStartDecodesAttributesAndState() throws {
        let aps = try aps("start")
        XCTAssertEqual(aps["event"] as? String, "start")
        XCTAssertEqual(aps["attributes-type"] as? String, "ChatActivityAttributes")
        let facts = try decode(ChatActivityFacts.self, aps["attributes"])
        XCTAssertEqual(
            facts,
            ChatActivityFacts(
                computer: "studio",
                computerId: String(repeating: "AB", count: 32),
                workspace: "herdr-gpui",
                chatTitle: "remote plan",
                agent: "Claude",
                pane: "w1-p3"
            )
        )
        XCTAssertEqual(facts.place, "herdr-gpui \u{b7} remote plan")
        XCTAssertEqual(facts.fingerprint, Fingerprint(String(repeating: "AB", count: 32)))
        XCTAssertEqual(facts.link.fingerprint, facts.fingerprint)
        let state = try decode(ChatActivityState.self, aps["content-state"])
        XCTAssertEqual(state.phase, .working)
        XCTAssertEqual(state.startedAt, Date(timeIntervalSince1970: 1_799_999_990))
        XCTAssertEqual(state.working, 1)
        XCTAssertNil(state.planFraction)
        XCTAssertEqual(state.headline, "Working")
    }

    func testQuestionUpdateDecodes() throws {
        let state = try decode(ChatActivityState.self, aps("update")["content-state"])
        XCTAssertEqual(state.phase, .question)
        XCTAssertEqual(state.question, "Run just ci")
        XCTAssertEqual(state.cost, "$0.34")
        XCTAssertEqual(state.planFraction, 0.5)
        XCTAssertEqual(state.planLabel, "Step 3 of 4")
        XCTAssertEqual(state.waiting, 1)
        XCTAssertEqual(state.headline, "Needs an answer")
    }

    func testEndDecodes() throws {
        let aps = try aps("end")
        XCTAssertEqual(aps["dismissal-date"] as? Int, 1_800_001_262)
        let state = try decode(ChatActivityState.self, aps["content-state"])
        XCTAssertEqual(state.phase, .finished)
        XCTAssertEqual(state.summary, "Worked 6m 2s \u{b7} $0.48")
        XCTAssertEqual(state.headline, "Finished")
    }

    func testStateRoundTripsAsUnixSeconds() throws {
        let state = ChatActivityState(
            phase: .failed,
            step: "Run",
            startedAt: Date(timeIntervalSince1970: 1_800_000_000),
            summary: "Claude stopped"
        )
        let data = try JSONEncoder().encode(state)
        let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        XCTAssertEqual(object?["startedAt"] as? Double, 1_800_000_000)
        XCTAssertNil(object?["cost"])
        XCTAssertEqual(try JSONDecoder().decode(ChatActivityState.self, from: data), state)
    }

    func testMissingCountsDefaultToZero() throws {
        let state = try JSONDecoder().decode(
            ChatActivityState.self,
            from: Data(#"{"phase":"working","step":"x","startedAt":1}"#.utf8)
        )
        XCTAssertEqual(state.working, 0)
        XCTAssertEqual(state.waiting, 0)
    }

    func testChatLinkRoundTripsAndEscapesThePane() throws {
        let id = String(repeating: "0F", count: 32)
        let link = ChatLink(computer: "Fabien's Mac", computerId: id, pane: "w1 p/3")
        XCTAssertEqual(ChatLink(url: link.url), link)
        XCTAssertEqual(link.fingerprint?.hex, id)
        XCTAssertEqual(link.url.scheme, "herdr-remote")
        XCTAssertEqual(
            link.page(origin: "https://192.168.1.20:7443")?.absoluteString,
            "https://192.168.1.20:7443/chat/w1%20p%2F3"
        )
        XCTAssertNil(ChatLink(url: try XCTUnwrap(URL(string: "https://example.com/chat?pane=x&computer=y"))))
        XCTAssertNil(ChatLink(url: try XCTUnwrap(URL(string: "herdr-remote://chat?computer=y&id=\(id)"))))
        // The name alone never names a computer.
        XCTAssertNil(ChatLink(url: try XCTUnwrap(URL(string: "herdr-remote://chat?computer=y&pane=p"))))
    }
}
