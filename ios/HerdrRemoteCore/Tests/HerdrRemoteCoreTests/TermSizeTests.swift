import Foundation
import Testing
@testable import HerdrRemoteCore

/// A terminal asks for the phone's size, and asks again after a reconnect.
struct TermSizeTests {
    @Test func encodesTheSizeWithATerminalSubscription() throws {
        let request = LiveRequest.subscribe(.term(pane: "p2"), size: TermSize(cols: 46, rows: 30))
        let json = try #require(try JSONSerialization.jsonObject(with: JSONEncoder().encode(request)) as? [String: Any])
        #expect(json["view"] as? String == "term")
        let size = try #require(json["size"] as? [String: Int])
        #expect(size == ["cols": 46, "rows": 30])
        let plain = try #require(try JSONSerialization.jsonObject(with: JSONEncoder().encode(LiveRequest.subscribe(.term(pane: "p2")))) as? [String: Any])
        #expect(plain["size"] == nil)
    }

    @Test func resubscribesATerminalAtItsSizeAfterAReconnect() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        let size = TermSize(cols: 46, rows: 30)
        await link.watch(.term(pane: "p2"), size: size)
        await link.start()
        await until { await computer.sent.count == 1 }
        #expect(await computer.sent.last == .subscribe(.term(pane: "p2"), size: size))
        await computer.drop()
        await link.networkChanged()
        await until { await computer.sent.count == 2 }
        #expect(await computer.sent.last == .subscribe(.term(pane: "p2"), size: size))
        // Unwatched, its size is forgotten with it.
        await link.unwatch(.term(pane: "p2"))
        await link.watch(.term(pane: "p2"))
        #expect(await computer.sent.last == .subscribe(.term(pane: "p2")))
        await link.stop()
    }

    @Test func encodesAScroll() throws {
        let json = try #require(try JSONSerialization.jsonObject(with: JSONEncoder().encode(LiveRequest.termScroll(pane: "p2", lines: -3))) as? [String: Any])
        #expect(json["type"] as? String == "term_scroll")
        #expect(json["pane"] as? String == "p2")
        #expect(json["lines"] as? Int == -3)
        #expect(json["view"] == nil)
    }

    @Test func scrollsOnlyAWatchedTerminal() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        await until { await computer.connects.count == 1 }
        await link.scroll(pane: "p2", lines: 3)
        await link.watch(.term(pane: "p2"))
        await link.scroll(pane: "p2", lines: 0)
        await link.scroll(pane: "p2", lines: 3)
        #expect(await computer.sent == [.subscribe(.term(pane: "p2")), .termScroll(pane: "p2", lines: 3)])
        await link.stop()
    }
}
