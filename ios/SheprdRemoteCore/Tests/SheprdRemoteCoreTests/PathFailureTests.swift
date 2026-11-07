import Foundation
import Testing
@testable import SheprdRemoteCore

/// A path can stop carrying traffic while the live socket still looks
/// open, so a call that gets no answer is what sends the link looking.
struct LinkPathFailureTests {
    @Test func aDeadPathMovesTheLinkToAnAddressThatAnswers() async {
        let computer = FakeComputer()
        await computer.setPing("https://tailscale", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://tailscale", "https://lan"], io: await computer.io())
        await link.start()
        _ = await collect(link) { $0.contains(.status(.online(origin: "https://tailscale"))) }
        await computer.setPing("https://tailscale", String(repeating: "CD", count: 32))
        await computer.setPing("https://lan", mac)
        await link.pathFailed("https://tailscale", started: .now)
        // The fake's backoff lasts a minute: this only arrives if the link
        // retries at once.
        let moved = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        #expect(moved.contains(.status(.online(origin: "https://lan"))))
        #expect(await computer.connects == ["https://tailscale", "https://lan"])
        await link.stop()
    }

    /// A slow call can fail after the link has already moved on.
    @Test func aFailureAtAnAddressTheLinkLeftKeepsTheSocket() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        _ = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        await link.pathFailed("https://tailscale", started: .now)
        await computer.emit(.error("still here"))
        let seen = await collect(link) { updates in
            updates.contains { update in
                switch update {
                case .event(.error), .status(.offline): true
                default: false
                }
            }
        }
        #expect(seen.contains(.event(.error("still here"))))
        #expect(await computer.connects == ["https://lan"])
        await link.stop()
    }

    /// Two calls can fail on one dead path: the second must not take down
    /// the socket the first one's failure got the link back on.
    @Test func aCallOlderThanTheSocketKeepsItEvenAtTheSameAddress() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        _ = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        let older = ContinuousClock.now
        await link.pathFailed("https://lan", started: .now)
        _ = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        await link.pathFailed("https://lan", started: older)
        await computer.emit(.error("still here"))
        let seen = await collect(link) { updates in
            updates.contains { update in
                switch update {
                case .event(.error), .status(.offline): true
                default: false
                }
            }
        }
        #expect(seen.contains(.event(.error("still here"))))
        #expect(await computer.connects == ["https://lan", "https://lan"])
        await link.stop()
    }
}

struct UnreportedFailuresTests {
    @Test func aLineStaysUntilTheComputerAcceptsIt() {
        var failures = UnreportedFailures()
        failures.add("first")
        failures.add("second")
        failures.accepted("first")
        #expect(failures.lines == ["second"])
    }

    @Test func theOldestLinesGoPastTheLimit() {
        var failures = UnreportedFailures()
        for index in 0...UnreportedFailures.limit { failures.add("\(index)") }
        #expect(failures.lines.count == UnreportedFailures.limit)
        #expect(failures.lines.first == "1")
    }

    /// Newer failures can push out the line a report is carrying.
    @Test func acceptingALineAlreadyPushedOutRemovesNothingElse() {
        var failures = UnreportedFailures()
        failures.add("old")
        for index in 0..<UnreportedFailures.limit { failures.add("\(index)") }
        failures.accepted("old")
        #expect(failures.lines.count == UnreportedFailures.limit)
    }
}

struct PathFailureTests {
    @Test(arguments: [
        URLError.Code.cannotConnectToHost, .cannotFindHost, .dnsLookupFailed,
        .networkConnectionLost, .notConnectedToInternet, .timedOut,
    ])
    func noAnswerMeansTheAddressIsDead(code: URLError.Code) {
        #expect(PathFailure.isDead(URLError(code)))
    }

    @Test func aCancelledCallARefusedCertificateOrAnAnswerIsNot() {
        #expect(!PathFailure.isDead(URLError(.cancelled)))
        #expect(!PathFailure.isDead(URLError(.serverCertificateUntrusted)))
        #expect(!PathFailure.isDead(LinkError.outdated))
    }

    @Test(arguments: [
        ("https://mac:7443/api/chat/p1/upload", "/api/chat/{id}/upload"),
        ("https://mac:7443/api/v2/chat/p1", "/api/v2/chat/{id}"),
        ("https://mac:7443/api/pair/claim/secret", "/api/pair/claim/{id}"),
        ("https://mac:7443/api/v2/workspaces", "/api/v2/workspaces"),
        ("https://mac:7443/api/v2/search?q=private", "/api/v2/search"),
    ])
    func logsNameARouteWithoutItsIdentifiers(url: String, route: String) {
        #expect(PathFailure.route(URL(string: url)) == route)
    }

    @Test func aSummaryCarriesTheCodeAndTheTime() {
        let summary = PathFailure.summary(URLError(.networkConnectionLost), after: .milliseconds(1250))
        #expect(summary.contains("NSURLErrorDomain -1005"))
        #expect(summary.hasSuffix("after 1.2s") || summary.hasSuffix("after 1.3s"))
    }
}
