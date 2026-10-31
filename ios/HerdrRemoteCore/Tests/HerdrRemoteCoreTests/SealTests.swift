import CryptoKit
import Foundation
import Testing
@testable import HerdrRemoteCore

/// The daemon's Rust tests seal these exact bytes (`apns/tests/seal.rs`).
struct SealTests {
    struct Fixture: Decodable {
        let device_private_key: String
        let device_public_key: String
        let plaintext: String
        let sealed: String
    }

    func fixture() throws -> Fixture {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/seal", withExtension: "json"))
        return try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: url))
    }

    func key(_ fixture: Fixture) throws -> P256.KeyAgreement.PrivateKey {
        try P256.KeyAgreement.PrivateKey(rawRepresentation: #require(Base64URL.decode(fixture.device_private_key)))
    }

    @Test func opensWhatTheDaemonSealed() throws {
        let fixture = try fixture()
        let key = try key(fixture)
        #expect(Base64URL.encode(key.publicKey.x963Representation) == fixture.device_public_key)
        let sealed = try #require(Base64URL.decode(fixture.sealed))
        let opened = try NotificationSeal.open(sealed, device: key.publicKey) {
            try key.sharedSecretFromKeyAgreement(with: $0)
        }
        #expect(String(decoding: opened, as: UTF8.self) == fixture.plaintext)
    }

    @Test func refusesAChangedByte() throws {
        let fixture = try fixture()
        let key = try key(fixture)
        var sealed = try #require(Base64URL.decode(fixture.sealed))
        sealed[sealed.count - 1] ^= 1
        #expect(throws: NotificationSeal.Failure.open) {
            try NotificationSeal.open(sealed, device: key.publicKey) { try key.sharedSecretFromKeyAgreement(with: $0) }
        }
    }

    @Test func refusesAnotherDevicesKey() throws {
        let fixture = try fixture()
        let other = P256.KeyAgreement.PrivateKey()
        let sealed = try #require(Base64URL.decode(fixture.sealed))
        #expect(throws: NotificationSeal.Failure.open) {
            try NotificationSeal.open(sealed, device: other.publicKey) { try other.sharedSecretFromKeyAgreement(with: $0) }
        }
    }

    @Test func refusesATruncatedPayload() {
        #expect(throws: NotificationSeal.Failure.format) {
            try NotificationSeal.open(Data(count: 40), device: P256.KeyAgreement.PrivateKey().publicKey) { _ in
                throw NotificationSeal.Failure.open
            }
        }
    }
}
