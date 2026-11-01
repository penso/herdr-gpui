import Foundation
import Testing
@testable import HerdrRemoteCore

struct AnswerTimesTests {
    let now = Date(timeIntervalSince1970: 1_800_000_000)

    /// The fixture's home, its first chat working on `p1`, undated.
    func home() throws -> Home {
        let message = try JSONSerialization.jsonObject(with: WireTests.fixture("home")) as? [String: Any]
        let data = try JSONSerialization.data(withJSONObject: message?["home"] ?? [:])
        var home = try JSONDecoder().decode(Home.self, from: data)
        home.workspaces[0].chats[0].summary?.lastTurnEndedMs = nil
        return home
    }

    func with(_ home: Home, _ change: (inout Home.Summary) -> Void) -> Home {
        var changed = home
        if var summary = changed.workspaces[0].chats[0].summary {
            change(&summary)
            changed.workspaces[0].chats[0].summary = summary
        }
        return changed
    }

    func idle(_ summary: inout Home.Summary) {
        summary.status = .idle
        summary.turnStartedMs = nil
    }

    @Test func aTurnEndingWhileWatchedIsDatedNow() throws {
        let working = try home()
        var times = AnswerTimes()
        times.update(from: working, to: working, now: now)
        #expect(times["p1"] == nil)
        let done = with(working, idle)
        times.update(from: working, to: done, now: now)
        #expect(times["p1"] == now)
        // Later homes keep the time until the chat answers again.
        times.update(from: done, to: done, now: now.addingTimeInterval(60))
        #expect(times["p1"] == now)
        let answered = with(done) { $0.lastText = "Another answer." }
        times.update(from: done, to: answered, now: now.addingTimeInterval(120))
        #expect(times["p1"] == now.addingTimeInterval(120))
    }

    @Test func theComputersOwnTimeWinsAndGoneChatsAreForgotten() throws {
        let working = try home()
        let done = with(working, idle)
        var times = AnswerTimes()
        times.update(from: working, to: done, now: now)
        let dated = with(done) { $0.lastTurnEndedMs = 1_700_000_000_000 }
        times.update(from: done, to: dated, now: now)
        #expect(times["p1"] == nil)

        times.update(from: working, to: done, now: now)
        var gone = done
        gone.workspaces[0].chats.removeAll()
        times.update(from: done, to: gone, now: now)
        #expect(times["p1"] == nil)
    }
}
