import CryptoKit
import Foundation
import Testing
@testable import HerdrRemoteCore

@Suite struct PresenceTests {
    @Test func theSignedMessageCoversTheFingerprintAndNonce() throws {
        let nonce = Data(repeating: 7, count: 32)
        let fingerprint = try #require(Fingerprint(String(repeating: "AB", count: 32)))
        let message = try Presence.message(fingerprint: fingerprint, nonce: Base64URL.encode(nonce))
        #expect(message == Data("herdr-presence-v1".utf8) + fingerprint.bytes + nonce)
        #expect(throws: Presence.Failure.nonce) { try Presence.message(fingerprint: fingerprint, nonce: Base64URL.encode(Data(count: 31))) }
    }

    @Test func hintsChangeDailyAndHideTheDevice() {
        let today = Presence.hint(device: "dev-1", day: 20_000)
        #expect(today == Presence.hint(device: "dev-1", day: 20_000))
        #expect(today != Presence.hint(device: "dev-1", day: 20_001))
        #expect(today != Presence.hint(device: "dev-2", day: 20_000))
        #expect(Base64URL.decode(today)?.count == 16)
        #expect(!today.contains("dev"))
        #expect(Presence.day(Date(timeIntervalSince1970: 86_400 * 3 + 5)) == 3)
    }

    @Test func framesUseTheContractsKeys() throws {
        let hello = try JSONSerialization.jsonObject(with: JSONEncoder().encode(Presence.Outgoing.hello(hint: "h1"))) as? [String: String]
        #expect(hello == ["type": "hello", "hint": "h1"])
        let prove = try JSONSerialization.jsonObject(with: JSONEncoder().encode(Presence.Outgoing.prove(nonce: "n", signature: "s"))) as? [String: String]
        #expect(prove == ["type": "prove", "nonce": "n", "signature": "s"])
        let near = try JSONSerialization.jsonObject(with: JSONEncoder().encode(Presence.Outgoing.near(true))) as? [String: AnyHashable]
        #expect(near == ["type": "near", "near": true])
        let decode = { (json: String) in try JSONDecoder().decode(Presence.Incoming.self, from: Data(json.utf8)) }
        #expect(try decode(#"{"type":"challenge","nonce":"abc","certificate":"c","signature":"s"}"#) == .challenge(nonce: "abc", certificate: "c", signature: "s"))
        #expect(try decode(#"{"type":"proven","addresses":null}"#) == .proven(addresses: nil))
        #expect(try decode(#"{"type":"error","message":"unknown device"}"#) == .error("unknown device"))
        #expect(try decode(#"{"type":"later"}"#) == .other)
    }

    @Test func nearnessHasAGapBetweenItsLevels() {
        var nearness = Presence.Nearness()
        #expect(nearness.update(rssi: -70) == nil)
        #expect(nearness.update(rssi: -60) == true)
        #expect(nearness.update(rssi: -75) == nil)
        #expect(nearness.update(rssi: 127) == nil)
        #expect(nearness.update(rssi: -85) == false)
        #expect(nearness.update(rssi: -70) == nil)
    }

    @Test func receivedAddressesComeFirstAndOnlyHTTPSOrigins() {
        let merged = Presence.merged(
            known: ["https://10.0.0.2:7443", "https://mac.local:7443"],
            received: ["https://192.168.1.9:7443", "http://evil:80", "https://10.0.0.2:7443", "https://x:1/path"]
        )
        #expect(merged == ["https://192.168.1.9:7443", "https://10.0.0.2:7443", "https://mac.local:7443"])
    }

    @Test func oldComputerRecordsDecodeWithPresenceOff() throws {
        let fingerprint = String(repeating: "AB", count: 32)
        let json = """
        {"device":"d","name":"Mac","origins":["https://a:1"],"fingerprint":"\(fingerprint)","keyKind":"software","keyData":""}
        """
        let computer = try JSONDecoder().decode(Computer.self, from: Data(json.utf8))
        #expect(!computer.presence)
    }
}

/// The daemon's own presence fixture (`Fixtures/presence/prove.json`,
/// written by `auth/tests/presence.rs`): the phone names itself by the
/// same hint, accepts the computer's signed challenge only for the pinned
/// CA and its own device, signs the same bytes, and opens the addresses.
@Suite struct PresenceFixtureTests {
    struct Fixture: Decodable {
        let device_id: String
        let hint_day: UInt32
        let hint: String
        let ca_certificate: String
        let ca_fingerprint: String
        let challenge_signature: String
        let device_private_key: String
        let device_public_key: String
        let nonce: String
        let message: String
        let signature: String
        let addresses: Addresses
        struct Addresses: Decodable {
            let device_notify_private_key: String
            let sealed: String
            let plaintext: Plaintext
        }
        struct Plaintext: Decodable { let origins: [String] }
    }

    /// The fixture writes the fingerprint's raw bytes as base64url.
    static func pinned(_ fixture: Fixture) throws -> Fingerprint {
        let bytes = try #require(Base64URL.decode(fixture.ca_fingerprint))
        let pinned = try #require(Fingerprint(bytes.map { String(format: "%02X", $0) }.joined()))
        #expect(pinned == Fingerprint(certificate: try #require(Base64URL.decode(fixture.ca_certificate))))
        return pinned
    }

    static func fixture() throws -> Fixture {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/presence/prove", withExtension: "json"))
        return try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: url))
    }

    @Test func theHintMatchesTheDaemons() throws {
        let fixture = try Self.fixture()
        #expect(Presence.hint(device: fixture.device_id, day: fixture.hint_day) == fixture.hint)
    }

    @Test func theChallengeIsAcceptedOnlyFromThePinnedComputerForThisDevice() throws {
        let fixture = try Self.fixture()
        let pinned = try Self.pinned(fixture)
        let check = { (fingerprint: Fingerprint, device: String) in
            Presence.challengeIsFrom(fingerprint, device: device, nonce: fixture.nonce, certificate: fixture.ca_certificate, signature: fixture.challenge_signature)
        }
        #expect(check(pinned, fixture.device_id))
        #expect(!check(try #require(Fingerprint(String(repeating: "00", count: 32))), fixture.device_id))
        #expect(!check(pinned, fixture.device_id + "x"))
    }

    @Test func thePhoneSignsTheBytesTheDaemonChecks() throws {
        let fixture = try Self.fixture()
        let pinned = try Self.pinned(fixture)
        let message = try Presence.message(fingerprint: pinned, nonce: fixture.nonce)
        #expect(Base64URL.encode(message) == fixture.message)
        let key = try P256.Signing.PrivateKey(rawRepresentation: #require(Base64URL.decode(fixture.device_private_key)))
        #expect(Base64URL.encode(key.publicKey.x963Representation) == fixture.device_public_key)
        let theirs = try P256.Signing.ECDSASignature(rawRepresentation: #require(Base64URL.decode(fixture.signature)))
        #expect(key.publicKey.isValidSignature(theirs, for: message))
    }

    @Test func theSealedAddressesOpenOnlyAsAddresses() throws {
        let fixture = try Self.fixture()
        let key = try P256.KeyAgreement.PrivateKey(rawRepresentation: #require(Base64URL.decode(fixture.addresses.device_notify_private_key)))
        let origins = try Presence.openAddresses(fixture.addresses.sealed, device: key.publicKey) { try key.sharedSecretFromKeyAgreement(with: $0) }
        #expect(origins == fixture.addresses.plaintext.origins)
        #expect(throws: (any Error).self) {
            try SealedAlert.open(fixture.addresses.sealed, device: key.publicKey) { try key.sharedSecretFromKeyAgreement(with: $0) }
        }
    }
}
