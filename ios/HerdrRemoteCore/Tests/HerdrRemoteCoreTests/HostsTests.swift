import Foundation
import Testing
@testable import HerdrRemoteCore

/// Pairing the phone with the hosts a paired computer knows: the wire the
/// daemon's Rust tests write (`native/tests/hosts.rs`), and which hosts the
/// phone pairs, offers, and keeps.
struct HostsTests {
    static let laptop = Fingerprint(String(repeating: "11", count: 32))!
    static let devbox = Fingerprint(String(repeating: "ABCDEF0123456789", count: 4))!
    static let key = HostKey(publicKey: "BPub", kind: .software, data: Data([1, 2, 3]))
    static let start = Date(timeIntervalSince1970: 1_000_000)

    static func hosts() throws -> [HostLink] {
        try JSONDecoder().decode(API.Hosts.self, from: WireTests.fixture("hosts")).hosts
    }

    static func computer(_ fingerprint: Fingerprint, via: Fingerprint? = nil) -> Computer {
        Computer(
            device: "d-\(fingerprint.hex.prefix(4))", name: "c", origins: ["https://a:7443"],
            fingerprint: fingerprint, keyKind: .software, keyData: Data(), via: via
        )
    }

    static func link(_ host: String, _ state: HostState) -> HostLink {
        HostLink(host: host, name: host, state: state)
    }

    static func paired(device: String = "d-devbox", fingerprint: Fingerprint = devbox) -> HostState {
        .paired(.init(device: device, computer: "devbox", fingerprint: fingerprint, origins: ["https://b:7443", "https://c:7443", "https://b:7443"]))
    }

    // MARK: Wire

    @Test func decodesEveryHostState() throws {
        let hosts = try Self.hosts()
        #expect(hosts.map(\.host) == ["ssh:devbox", "ssh:gpu", "coder:ws1", "ssh:ci", "ssh:lab", "ssh:old"])
        #expect(hosts[1].name == "gpu box")
        guard case let .paired(enrolled) = hosts[0].state else { Issue.record("not paired"); return }
        #expect(enrolled.device == "d-devbox")
        #expect(enrolled.computer == "devbox")
        #expect(enrolled.fingerprint == Self.devbox)
        #expect(enrolled.origins == ["https://100.64.0.7:7443"])
        #expect(hosts[1...4].map(\.state) == [.allowed, .offered, .pending, .asking])
        #expect(hosts[5].state == .failed(reason: "No build for Linux riscv64 in this app"))
    }

    @Test func decodesThePairAnswer() throws {
        #expect(try JSONDecoder().decode(HostState.self, from: WireTests.fixture("host_state")) == .pending)
    }

    @Test func refusesAnUnknownState() {
        let json = Data(#"{"host":"ssh:x","name":"x","state":"later"}"#.utf8)
        #expect(throws: DecodingError.self) { try JSONDecoder().decode(HostLink.self, from: json) }
    }

    @Test func encodesThePairRequestLikeAClaim() throws {
        let json = try JSONEncoder().encode(API.HostPairRequest(publicKey: "BPub"))
        #expect(String(decoding: json, as: UTF8.self) == #"{"public_key":"BPub"}"#)
    }

    // MARK: Records

    @Test func buildsTheHostsRecordFromWhatTheComputerReported() {
        guard case let .paired(enrolled) = Self.paired() else { Issue.record("not paired"); return }
        let computer = Computer(host: enrolled, key: Self.key, via: Self.laptop)
        #expect(computer.device == "d-devbox")
        #expect(computer.name == "devbox")
        #expect(computer.origins == ["https://b:7443", "https://c:7443"])
        #expect(computer.fingerprint == Self.devbox)
        #expect(computer.keyKind == .software && computer.keyData == Data([1, 2, 3]))
        #expect(!computer.presence)
        #expect(computer.via == Self.laptop)
    }

    @Test func readsRecordsSavedBeforeVia() throws {
        let old = Data("""
        {"device":"d1","name":"studio","origins":["https://a:7443"],"fingerprint":"\(Self.laptop.hex)",\
        "keyKind":"software","keyData":"AQI=","presence":true}
        """.utf8)
        let computer = try JSONDecoder().decode(Computer.self, from: old)
        #expect(computer.via == nil)
        #expect(computer.presence)
        var host = computer
        host.via = Self.devbox
        let again = try JSONDecoder().decode(Computer.self, from: JSONEncoder().encode(host))
        #expect(again == host)
    }

    // MARK: Decisions

    @Test func pairsAllowedHostsAloneAndOffersTheRest() throws {
        var pairings = HostPairings()
        let added = pairings.update(try Self.hosts(), vouching: Self.laptop, held: [], at: Self.start)
        #expect(added.isEmpty, "nothing was posted, so nothing is the phone's")
        #expect(pairings.automatic.map(\.host) == ["ssh:gpu"])
        // Devbox is paired with a key this phone does not hold.
        #expect(pairings.offers(held: []).map(\.host) == ["ssh:devbox", "coder:ws1", "ssh:old"])
        #expect(pairings.offers(held: [Self.computer(Self.devbox)]).map(\.host) == ["coder:ws1", "ssh:old"])
    }

    @Test func followsAPostedKeyUntilTheHostHoldsIt() {
        var pairings = HostPairings()
        _ = pairings.update([Self.link("ssh:devbox", .allowed)], vouching: Self.laptop, held: [], at: Self.start)
        pairings.posted("ssh:devbox", key: Self.key, answer: .pending, at: Self.start)
        #expect(pairings.automatic.isEmpty)
        #expect(pairings.progress == [Self.link("ssh:devbox", .pending)])
        #expect(pairings.offers(held: []).isEmpty)

        let later = Self.start.addingTimeInterval(30)
        let none = pairings.update([Self.link("ssh:devbox", .pending)], vouching: Self.laptop, held: [], at: later)
        #expect(none.isEmpty)
        #expect(pairings.waiting)

        let added = pairings.update([Self.link("ssh:devbox", Self.paired())], vouching: Self.laptop, held: [], at: later)
        #expect(added.map(\.device) == ["d-devbox"])
        #expect(added.first?.via == Self.laptop)
        #expect(added.first?.keyData == Self.key.data)
        #expect(!pairings.waiting)
        #expect(pairings.progress.map(\.state) == [Self.paired()])
    }

    @Test func leavesAHostPairedDirectlyAlone() {
        var pairings = HostPairings()
        _ = pairings.update([Self.link("ssh:devbox", .allowed)], vouching: Self.laptop, held: [], at: Self.start)
        pairings.posted("ssh:devbox", key: Self.key, answer: .pending, at: Self.start)
        let direct = Self.computer(Self.devbox)
        let kept = pairings.update([Self.link("ssh:devbox", Self.paired())], vouching: Self.laptop, held: [direct], at: Self.start)
        #expect(kept.isEmpty)
        #expect(!pairings.waiting)
        // One paired through another computer is replaced.
        pairings.posted("ssh:devbox", key: Self.key, answer: .pending, at: Self.start)
        let through = Self.computer(Self.devbox, via: Self.laptop)
        let replaced = pairings.update([Self.link("ssh:devbox", Self.paired())], vouching: Self.laptop, held: [through], at: Self.start)
        #expect(replaced.count == 1)
    }

    @Test func dropsAKeyWhoseRequestEnded() {
        let ends: [HostState] = [.failed(reason: "no"), .allowed, .offered, Self.paired()]
        for end in ends {
            var pairings = HostPairings()
            pairings.posted("ssh:devbox", key: Self.key, answer: .asking, at: Self.start)
            let added = pairings.update([Self.link("ssh:devbox", end)], vouching: Self.laptop, held: [], at: Self.start)
            #expect(!pairings.waiting, "\(end)")
            #expect(added.isEmpty == (end != Self.paired()), "\(end)")
        }
        var gone = HostPairings()
        gone.posted("ssh:devbox", key: Self.key, answer: .pending, at: Self.start)
        _ = gone.update([], vouching: Self.laptop, held: [], at: Self.start)
        #expect(!gone.waiting)
    }

    @Test func givesUpOnAKeyAfterTenMinutes() {
        var pairings = HostPairings()
        pairings.posted("ssh:devbox", key: Self.key, answer: .pending, at: Self.start)
        let late = Self.start.addingTimeInterval(HostPairings.patience)
        let added = pairings.update([Self.link("ssh:devbox", Self.paired())], vouching: Self.laptop, held: [], at: late)
        #expect(added.isEmpty)
        #expect(!pairings.waiting)
    }

    @Test func showsARefusalUntilTheNextList() {
        var pairings = HostPairings()
        _ = pairings.update([Self.link("ssh:gpu", .allowed)], vouching: Self.laptop, held: [], at: Self.start)
        pairings.refused("ssh:gpu", reason: "busy")
        #expect(pairings.hosts.first?.state == .failed(reason: "busy"))
        #expect(pairings.automatic.isEmpty)
        _ = pairings.update([Self.link("ssh:gpu", .allowed)], vouching: Self.laptop, held: [], at: Self.start)
        #expect(pairings.automatic.map(\.host) == ["ssh:gpu"])
    }

    @Test func asksAComputerAtMostOnceAMinuteAndNeverAHost() {
        var schedule = HostSchedule()
        let laptop = Self.computer(Self.laptop)
        let host = Self.computer(Self.devbox, via: Self.laptop)
        let answers = [
            schedule.due(laptop, at: Self.start),
            schedule.due(laptop, at: Self.start.addingTimeInterval(59)),
            schedule.due(laptop, at: Self.start.addingTimeInterval(59), force: true),
            schedule.due(laptop, at: Self.start.addingTimeInterval(120)),
            schedule.due(host, at: Self.start),
            schedule.due(host, at: Self.start, force: true),
        ]
        #expect(answers == [true, false, true, true, false, false])
    }
}
