import CryptoKit
import Foundation

/// The phone's side of nearby pairing (`docs/ios-app-plan.md`, "Nearby
/// pairing, exactly"): a numeric comparison with commitments, so a man in
/// the middle gets one guess in a million, and the code also vouches for
/// the computer's CA fingerprint.
public struct NearbySAS: Sendable {
    /// The computer's first answer.
    public struct Hello: Codable, Hashable, Sendable {
        public var session: String
        public var computer: String
        /// 64 hex digits.
        public var fingerprint: String
        public var publicKey: String
        public var commit: String

        enum CodingKeys: String, CodingKey {
            case session, computer, fingerprint, commit
            case publicKey = "public_key"
        }
    }

    /// The computer's second answer: its nonce and the sealed offer.
    public struct Reveal: Codable, Hashable, Sendable {
        public var nonce: String
        public var sealed: String
    }

    /// What the sealed offer holds: a claim token and where to claim it.
    public struct Offer: Codable, Hashable, Sendable {
        public var token: String
        public var origins: [String]
    }

    /// The finished exchange.
    public struct Result: Hashable, Sendable {
        /// The six digits both screens show.
        public var code: String
        public var offer: Offer
        public var fingerprint: Fingerprint
        public var computer: String
    }

    public enum Failure: Error, Hashable, Sendable {
        /// A value is not the length or encoding the protocol says.
        case format
        /// The computer's nonce does not match what it committed to.
        case commit
        /// The offer did not open under the agreed key.
        case sealed
    }

    static let commitLabel = Data("herdr-sas-commit-v1".utf8)
    static let codeLabel = Data("herdr-sas-code-v1".utf8)
    static let keyLabel = Data("herdr-sas-key-v1".utf8)

    let privateKey: P256.KeyAgreement.PrivateKey
    /// `Na`, revealed only after the computer committed.
    let nonce: Data

    public init() {
        self.init(privateKey: P256.KeyAgreement.PrivateKey(), nonce: Data((0..<32).map { _ in UInt8.random(in: .min ... .max) }))
    }

    init(privateKey: P256.KeyAgreement.PrivateKey, nonce: Data) {
        self.privateKey = privateKey
        self.nonce = nonce
    }

    /// `PKa`, for `start`.
    public var publicKey: String { Base64URL.encode(privateKey.publicKey.x963Representation) }
    /// `Na`, for `reveal`.
    public var nonceText: String { Base64URL.encode(nonce) }

    /// Checks the commitment, derives the code and key, and opens the offer.
    public func finish(hello: Hello, reveal: Reveal) throws(Failure) -> Result {
        guard let theirs = Base64URL.decode(hello.publicKey), theirs.count == 65,
              let commit = Base64URL.decode(hello.commit), commit.count == 32,
              let theirNonce = Base64URL.decode(reveal.nonce), theirNonce.count == 32,
              let sealed = Base64URL.decode(reveal.sealed), sealed.count > 12 + 16,
              let fingerprint = Fingerprint(hello.fingerprint),
              let theirKey = try? P256.KeyAgreement.PublicKey(x963Representation: theirs)
        else { throw .format }
        let mine = privateKey.publicKey.x963Representation
        guard Self.commitment(theirKey: theirs, myKey: mine, theirNonce: theirNonce) == commit else { throw .commit }
        let transcript = mine + theirs + nonce + theirNonce + fingerprint.bytes
        guard let shared = try? privateKey.sharedSecretFromKeyAgreement(with: theirKey) else { throw .format }
        let key = Self.key(shared: shared, transcript: transcript)
        guard let opened = try? AES.GCM.open(
            AES.GCM.SealedBox(combined: sealed),
            using: key,
            authenticating: Self.keyLabel
        ), let offer = try? JSONDecoder().decode(Offer.self, from: opened)
        else { throw .sealed }
        return Result(code: Self.code(transcript: transcript), offer: offer, fingerprint: fingerprint, computer: hello.computer)
    }

    static func commitment(theirKey: Data, myKey: Data, theirNonce: Data) -> Data {
        Data(SHA256.hash(data: commitLabel + theirKey + myKey + theirNonce))
    }

    static func code(transcript: Data) -> String {
        let digest = Array(SHA256.hash(data: codeLabel + transcript))
        let value = UInt32(digest[0]) << 24 | UInt32(digest[1]) << 16 | UInt32(digest[2]) << 8 | UInt32(digest[3])
        return String(format: "%06u", value % 1_000_000)
    }

    static func key(shared: SharedSecret, transcript: Data) -> SymmetricKey {
        shared.hkdfDerivedSymmetricKey(
            using: SHA256.self,
            salt: Data(SHA256.hash(data: transcript)),
            sharedInfo: keyLabel,
            outputByteCount: 32
        )
    }
}
