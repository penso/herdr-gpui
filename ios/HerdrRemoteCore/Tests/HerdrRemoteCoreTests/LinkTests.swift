import Foundation
import Testing
@testable import HerdrRemoteCore

/// A computer the link talks to, scripted per test.
actor FakeComputer {
    var pings: [String: String] = [:]
    var sessionError: Error?
    var connectError: Error?
    private(set) var connects: [String] = []
    private(set) var sent: [LiveRequest] = []
    private var sockets: [AsyncThrowingStream<LiveEvent, Error>.Continuation] = []

    func setPing(_ origin: String, _ fingerprint: String) { pings[origin] = fingerprint }
    func failSessions(_ error: Error?) { sessionError = error }
    func failConnects(_ error: Error?) { connectError = error }

    func io() -> LinkIO {
        LinkIO(
            ping: { origin in
                guard let answer = await self.pings[origin] else { throw URLError(.cannotConnectToHost) }
                return answer
            },
            session: { _ in
                if let error = await self.sessionError { throw error }
                return "token"
            },
            connect: { origin, _ in try await self.open(origin) },
            sleep: { duration in
                // Head starts pass at once; backoff waits are long enough
                // that a test only gets past one by waking the link.
                if duration > .seconds(0.5) { try await Task.sleep(for: .seconds(60)) }
            }
        )
    }

    private func open(_ origin: String) throws -> LiveConnection {
        connects.append(origin)
        if let connectError { throw connectError }
        let (events, continuation) = AsyncThrowingStream<LiveEvent, Error>.makeStream()
        sockets.append(continuation)
        return LiveConnection(
            events: events,
            send: { request in await self.record(request) },
            close: { continuation.finish() }
        )
    }

    private func record(_ request: LiveRequest) { sent.append(request) }

    func emit(_ event: LiveEvent) { sockets.last?.yield(event) }
    func drop() { sockets.last?.finish(throwing: URLError(.networkConnectionLost)) }
}

/// Collects a link's updates until a condition holds or time runs out.
func collect(_ link: ComputerLink, until done: @escaping ([LinkUpdate]) -> Bool) async -> [LinkUpdate] {
    var seen: [LinkUpdate] = []
    let deadline = ContinuousClock.now + .seconds(5)
    for await update in link.updates {
        seen.append(update)
        if done(seen) || ContinuousClock.now > deadline { break }
    }
    return seen
}

func until(_ condition: @escaping @Sendable () async -> Bool) async {
    for _ in 0..<200 where !(await condition()) {
        try? await Task.sleep(for: .milliseconds(10))
    }
}

let mac = String(repeating: "AB", count: 32)
let epoch = UUID()

struct LinkTests {
    @Test func picksTheAddressThatAnswersAsThisComputer() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", String(repeating: "CD", count: 32))
        await computer.setPing("https://tailscale", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan", "https://tailscale"], io: await computer.io())
        await link.start()
        let seen = await collect(link) { $0.contains(.status(.online(origin: "https://tailscale"))) }
        #expect(seen.contains(.status(.online(origin: "https://tailscale"))))
        #expect(await computer.connects == ["https://tailscale"])
        await link.stop()
    }

    /// The daemon's ping writes the fingerprint with colons.
    @Test func matchesAFingerprintWrittenWithColons() async {
        let computer = FakeComputer()
        let colons = stride(from: 0, to: mac.count, by: 2).map { index in
            String(mac[mac.index(mac.startIndex, offsetBy: index)...].prefix(2))
        }.joined(separator: ":")
        await computer.setPing("https://lan", colons.lowercased())
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        let seen = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        #expect(seen.contains(.status(.online(origin: "https://lan"))))
        await link.stop()
    }

    @Test func resubscribesWhatItWatchesFromTheCursorItReached() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.watch(.home)
        await link.watch(.chat(pane: "p1"), since: Cursor(epoch: epoch, seq: 4))
        await link.start()
        await until { await computer.sent.count == 2 }
        await computer.emit(.chatChanges(pane: "p1", changes: [
            CursorChange(cursor: Cursor(epoch: epoch, seq: 5), change: .clients(1)),
        ]))
        _ = await collect(link) { $0.contains { if case .event(.chatChanges) = $0 { true } else { false } } }
        await computer.drop()
        await link.networkChanged()
        await until { await computer.connects.count == 2 }
        await until { await computer.sent.count == 4 }
        let sent = await computer.sent
        #expect(sent.suffix(2).contains(.subscribe(.chat(pane: "p1"), since: Cursor(epoch: epoch, seq: 5))))
        #expect(sent.suffix(2).contains(.subscribe(.home)))
        await link.stop()
    }

    @Test func dropsChangesAfterAGapAndAsksAgain() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.watch(.chat(pane: "p1"), since: Cursor(epoch: epoch, seq: 4))
        await link.start()
        await until { await computer.sent.count == 1 }
        await computer.emit(.chatChanges(pane: "p1", changes: [
            CursorChange(cursor: Cursor(epoch: epoch, seq: 9), change: .clients(1)),
        ]))
        await computer.emit(.error("marker"))
        let seen = await collect(link) { $0.contains(.event(.error("marker"))) }
        #expect(!seen.contains { if case .event(.chatChanges) = $0 { true } else { false } })
        await until { await computer.sent.count == 2 }
        #expect(await computer.sent.last == .subscribe(.chat(pane: "p1"), since: Cursor(epoch: epoch, seq: 4)))
        await link.stop()
    }

    @Test func stopsWhenTheComputerRevokesThisPhone() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        await until { await computer.connects.count == 1 }
        await computer.emit(.bye(reason: "revoked"))
        let seen = await collect(link) { $0.last == .status(.revoked) }
        #expect(seen.last == .status(.revoked))
        #expect(await computer.connects.count == 1)
    }

    @Test func aRefusedSessionMeansRevoked() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        await computer.failSessions(LinkError.unauthorized)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        let seen = await collect(link) { $0.last == .status(.revoked) }
        #expect(seen.last == .status(.revoked))
    }

    @Test func waitsWhileNothingAnswersAndRetriesWhenTheNetworkChanges() async {
        let computer = FakeComputer()
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        let first = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        #expect(first.contains(.status(.offline(reason: "No address answered (1 tried)"))))
        await computer.setPing("https://lan", mac)
        await link.networkChanged()
        let next = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        #expect(next.contains(.status(.online(origin: "https://lan"))))
        await link.stop()
    }

    @Test func backoffGrowsToItsCap() {
        #expect(ComputerLink.backoff(attempt: 0) == .seconds(1))
        #expect(ComputerLink.backoff(attempt: 1) == .milliseconds(1500))
        #expect(ComputerLink.backoff(attempt: 40) == .seconds(30))
    }
}
