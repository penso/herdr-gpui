import Foundation
import Testing
@testable import HerdrRemoteCore

struct ExpectedTests {
    static let queuedId = UUID(uuidString: "00000000-0000-0000-0000-000000000001")!

    func transcript() throws -> Transcript {
        let json = """
        {
          "agent": "Claude",
          "auto_accept": false,
          "modes": {
            "available": [{"id": "default", "name": "Default"}, {"id": "bypassPermissions", "name": "Bypass"}],
            "current": "default"
          },
          "options": [
            {"id": "effort", "name": "Effort", "category": "thought_level", "kind": {
              "type": "select", "current": "low",
              "choices": [{"id": "low", "name": "Low"}, {"id": "high", "name": "High"}]
            }},
            {"id": "fast", "name": "Fast", "kind": {"type": "toggle", "current": false}}
          ],
          "permissions": [{"id": 3, "title": "Run ls", "options": [{"id": "allow", "kind": "allow_once", "name": "Allow"}]}],
          "queued": [{"id": "\(Self.queuedId.uuidString)", "text": "next"}]
        }
        """
        return try JSONDecoder().decode(Transcript.self, from: Data(json.utf8))
    }

    @Test func eachChangeShowsOnlyOnceAppliedOrEchoed() throws {
        let changes: [Expected] = [
            .mode("bypassPermissions"),
            .option(id: "effort", value: .choice("high")),
            .option(id: "fast", value: .toggle(true)),
            .autoAccept(true),
            .answer(3),
            .unqueue(Self.queuedId),
        ]
        for change in changes {
            var shown = try transcript()
            #expect(!change.shown(in: shown), "\(change)")
            change.apply(to: &shown)
            #expect(change.shown(in: shown), "\(change)")
        }
    }

    @Test func applyingSomethingTheChatDoesNotOfferChangesNothing() throws {
        let before = try transcript()
        for change in [Expected.mode("plan"), .option(id: "effort", value: .choice("max")), .option(id: "fast", value: .choice("on"))] {
            var after = before
            change.apply(to: &after)
            #expect(after == before, "\(change)")
            #expect(!change.shown(in: after), "\(change)")
        }
    }

    @Test func commandsMapToWhatTheyChange() {
        #expect(Expected(.setMode("plan")) == .mode("plan"))
        #expect(Expected(.answer(permission: 3, option: "allow")) == .answer(3))
        #expect(Expected(.prompt(text: "hi")) == nil)
        #expect(Expected(.cancel) == nil)
        // A newer pick for the same setting replaces the older one.
        #expect(Expected.mode("a").slot == Expected.mode("b").slot)
        #expect(Expected.option(id: "x", value: .toggle(true)).slot != Expected.option(id: "y", value: .toggle(true)).slot)
    }
}
