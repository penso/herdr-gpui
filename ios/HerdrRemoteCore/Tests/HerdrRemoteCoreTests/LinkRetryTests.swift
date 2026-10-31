import Foundation
import Testing
@testable import HerdrRemoteCore

/// A computer that stays away keeps the link offline across retries, so
/// the phone's "offline" toast does not come and go with each attempt.
struct LinkRetryTests {
    @Test func aRetryThatFailsStaysOfflineWithoutConnectingInBetween() async {
        let computer = FakeComputer()
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        let first = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        #expect(first.first == .status(.connecting))
        await link.networkChanged()
        let retry = await collect(link) { $0.contains { if case .status(.offline) = $0 { true } else { false } } }
        #expect(!retry.contains(.status(.connecting)))
        await computer.setPing("https://lan", mac)
        await link.networkChanged()
        let back = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        #expect(!back.contains(.status(.connecting)))
        #expect(back.contains(.status(.online(origin: "https://lan"))))
        await link.stop()
    }
}

/// A daemon too old for this app answers pings but refuses the socket.
struct LinkOutdatedTests {
    @Test func aDaemonWithoutTheLiveSocketIsOutdatedNeverOnline() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        await computer.failConnects(LinkError.outdated)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        let first = await collect(link) { $0.contains(.status(.outdated)) }
        #expect(first.contains(.status(.outdated)))
        await link.networkChanged()
        await until { await computer.connects.count == 2 }
        let retry = await collect(link) { $0.contains(.status(.outdated)) }
        #expect(!retry.contains(.status(.connecting)))
        #expect(!retry.contains { if case .status(.online) = $0 { true } else { false } })
        // Once Herdr there is updated, the next retry gets through.
        await computer.failConnects(nil)
        await link.networkChanged()
        let back = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        #expect(back.contains(.status(.online(origin: "https://lan"))))
        await link.stop()
    }
}
