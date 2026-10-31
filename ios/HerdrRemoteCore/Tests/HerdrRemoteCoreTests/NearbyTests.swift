import CryptoKit
import Foundation
import Testing
@testable import HerdrRemoteCore

struct NearbyFramingTests {
    @Test(arguments: [0, 1, 18, 19, 300, 4096])
    func reassemblesWhatItCut(length: Int) {
        let message = Data((0..<length).map { UInt8($0 % 251) })
        var assembly = NearbyFraming.Reassembly()
        var outcome = NearbyFraming.Reassembly.Outcome.partial
        for chunk in NearbyFraming.chunks(message, size: 20) {
            #expect(outcome == .partial)
            outcome = assembly.add(chunk)
        }
        #expect(outcome == .complete(message))
    }

    @Test func refusesAnOversizedOrOverlongMessage() {
        var oversized = NearbyFraming.Reassembly()
        #expect(oversized.add(Data([0x10, 0x01])) == .invalid)
        var overlong = NearbyFraming.Reassembly()
        #expect(overlong.add(Data([0x00, 0x01, 0x41, 0x42])) == .invalid)
    }
}

struct NearbyComputerTests {
    @Test func readsTheTXTRecord() {
        let route = NearbyComputer.Route.network(host: "m4max", port: 0)
        let computer = NearbyComputer.fromTXT(["v": "1", "fp": "abcdef0123456789", "pairing": "1"], name: "m4max", route: route)
        #expect(computer?.pairing == true)
        #expect(computer?.matches(Fingerprint(String(repeating: "ABCDEF0123456789", count: 4))!) == true)
        #expect(NearbyComputer.fromTXT(["v": "2", "fp": "abcdef0123456789"], name: "x", route: route) == nil)
        #expect(NearbyComputer.fromTXT(["v": "1", "fp": "short"], name: "x", route: route) == nil)
        #expect(NearbyComputer.fromTXT(["v": "1", "fp": "abcdef0123456789", "pairing": "0"], name: "x", route: route)?.pairing == false)
    }
}

/// The daemon's Rust tests write these vectors (`auth/tests/nearby_vectors.rs`).
struct NearbySASTests {
    struct Side: Decodable {
        let code: String
        let commit: String
        let daemon_nonce: String
        let daemon_public_key: String
        let fingerprint: String
        let key: String
        let phone_nonce: String
        let phone_private_key: String
        let phone_public_key: String
        let plaintext: String
        let sealed: String
    }

    static func load<Value: Decodable>(_ name: String, as type: Value.Type) throws -> Value {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/nearby/\(name)", withExtension: "json"))
        return try JSONDecoder().decode(type, from: Data(contentsOf: url))
    }

    static func sas(_ side: Side) throws -> NearbySAS {
        let key = try P256.KeyAgreement.PrivateKey(rawRepresentation: #require(Base64URL.decode(side.phone_private_key)))
        return NearbySAS(privateKey: key, nonce: try #require(Base64URL.decode(side.phone_nonce)))
    }

    static func hello(_ side: Side, commit: String? = nil) -> NearbySAS.Hello {
        NearbySAS.Hello(session: "s", computer: "m4max", fingerprint: side.fingerprint, publicKey: side.daemon_public_key, commit: commit ?? side.commit)
    }

    @Test func reachesTheDaemonsCodeAndOpensItsOffer() throws {
        let side = try Self.load("session", as: Side.self)
        let sas = try Self.sas(side)
        #expect(sas.publicKey == side.phone_public_key)
        let result = try sas.finish(hello: Self.hello(side), reveal: .init(nonce: side.daemon_nonce, sealed: side.sealed))
        #expect(result.code == side.code)
        let expected = try JSONDecoder().decode(NearbySAS.Offer.self, from: Data(side.plaintext.utf8))
        #expect(result.offer == expected)
        #expect(result.fingerprint.hex == side.fingerprint)
    }

    @Test func refusesANonceThatBreaksTheCommitment() throws {
        let side = try Self.load("session", as: Side.self)
        let other = Base64URL.encode(Data(repeating: 7, count: 32))
        #expect(throws: NearbySAS.Failure.commit) {
            try Self.sas(side).finish(hello: Self.hello(side), reveal: .init(nonce: other, sealed: side.sealed))
        }
    }

    @Test func refusesATamperedOffer() throws {
        let side = try Self.load("session", as: Side.self)
        var sealed = try #require(Base64URL.decode(side.sealed))
        sealed[sealed.count - 1] ^= 1
        #expect(throws: NearbySAS.Failure.sealed) {
            try Self.sas(side).finish(hello: Self.hello(side), reveal: .init(nonce: side.daemon_nonce, sealed: Base64URL.encode(sealed)))
        }
    }

    /// A relay that runs its own keys toward each side leaves the two
    /// screens showing different codes.
    @Test func aRelayShowsDifferentCodes() throws {
        struct Relay: Decodable { let phone_side: Side; let daemon_side: Side }
        let relay = try Self.load("relay", as: Relay.self)
        let phone = try Self.sas(relay.phone_side).finish(
            hello: Self.hello(relay.phone_side),
            reveal: .init(nonce: relay.phone_side.daemon_nonce, sealed: relay.phone_side.sealed)
        )
        #expect(phone.code == relay.phone_side.code)
        #expect(phone.code != relay.daemon_side.code)
    }
}
