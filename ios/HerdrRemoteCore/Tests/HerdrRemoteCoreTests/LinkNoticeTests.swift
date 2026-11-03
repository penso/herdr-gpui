import Testing
@testable import HerdrRemoteCore

struct LinkNoticeTests {
    private let offline = LinkStatus.offline(reason: "No address answered")

    @Test func theWorstKindWinsAndNamesEveryComputerWithIt() {
        let notice = LinkNotice.worst(
            [("a", offline), ("b", .outdated), ("c", offline), ("d", .online(origin: "https://d"))],
            patient: true, dismissed: []
        )
        #expect(notice == LinkNotice(kind: .outdated, devices: ["b"]))
    }

    @Test func connectingWaitsUntilItHasLasted() {
        #expect(LinkNotice.worst([("a", .connecting)], patient: false, dismissed: []) == nil)
        #expect(LinkNotice.worst([("a", .connecting)], patient: true, dismissed: [])?.kind == .connecting)
    }

    /// Closing "2 computers are offline" keeps it closed while they stay
    /// offline; a third going offline is news.
    @Test func aClosedNoticeStaysClosedForItsComputers() throws {
        let links: [(device: String, status: LinkStatus)] = [("a", offline), ("b", offline)]
        let first = try #require(LinkNotice.worst(links, patient: true, dismissed: []))
        #expect(first.devices == ["a", "b"])
        let dismissed = first.keys
        #expect(LinkNotice.worst(links, patient: true, dismissed: dismissed) == nil)
        let third = LinkNotice.worst(links + [("c", offline)], patient: true, dismissed: dismissed)
        #expect(third == LinkNotice(kind: .offline, devices: ["c"]))
    }

    /// Closed for being offline does not hide the same computer when it
    /// turns out to be unpaired.
    @Test func aClosedKindDoesNotHideAnother() {
        let dismissed: Set<LinkNotice.Key> = [.init(device: "a", kind: .offline)]
        #expect(LinkNotice.worst([("a", .revoked)], patient: true, dismissed: dismissed)?.kind == .revoked)
    }

    @Test func aComputerBackOnlineMayNoticeAgain() {
        let dismissed: Set<LinkNotice.Key> = [.init(device: "a", kind: .offline), .init(device: "b", kind: .offline)]
        #expect(LinkNotice.cleared(dismissed, online: ["a"]) == [.init(device: "b", kind: .offline)])
    }

    @Test func skippedKindsShowElsewhere() {
        let notice = LinkNotice.worst([("a", .revoked), ("b", offline)], patient: true, dismissed: [], skipping: [.revoked])
        #expect(notice == LinkNotice(kind: .offline, devices: ["b"]))
    }
}
