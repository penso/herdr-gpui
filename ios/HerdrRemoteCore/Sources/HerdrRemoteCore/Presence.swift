import CryptoKit
import Foundation
import Security

/// Bluetooth presence after pairing (`docs/ios-app-plan.md`, "Presence,
/// exactly"): a phone near a paired computer proves itself on the
/// `presence` characteristic, then says whether it is near, and the
/// computer holds phone alerts while you are at it.
///
/// - The phone names itself only by a hint that changes daily, so a
///   peripheral cannot follow it by a stable id.
/// - The computer signs its challenge with its CA key and sends the CA
///   certificate; the phone signs nothing until that certificate hashes to
///   the fingerprint pinned at pairing and the signature holds.
/// - The phone's proof covers that fingerprint, so it counts for that
///   computer alone.
/// - A proof is answered with the computer's current addresses, sealed to
///   the phone, so a computer whose address changed is found again
///   without Bonjour.
public enum Presence {
    /// What the device key signs: this, the CA fingerprint, the nonce.
    public static let prefix = Data("herdr-presence-v1".utf8)
    /// What the computer's CA key signs: this, the nonce, the device id.
    public static let challengePrefix = Data("herdr-presence-challenge-v1".utf8)
    static let hintPrefix = Data("herdr-presence-hint-v1".utf8)
    /// Seals the addresses, apart from alerts (`NotificationSeal.info`).
    public static let addressesInfo = Data("herdr-remote-addresses-v1".utf8)
    /// The most addresses kept for a computer.
    public static let maxOrigins = 8

    public enum Failure: Error, Equatable {
        case nonce
    }

    /// The bytes the phone signs for computer `fingerprint`'s challenge.
    public static func message(fingerprint: Fingerprint, nonce: String) throws -> Data {
        prefix + fingerprint.bytes + (try decodeNonce(nonce))
    }

    static func decodeNonce(_ nonce: String) throws -> Data {
        guard let decoded = Base64URL.decode(nonce), decoded.count == 32 else { throw Failure.nonce }
        return decoded
    }

    /// The day `date` falls on, as hints count them: whole days since 1970, UTC.
    public static func day(_ date: Date) -> UInt32 {
        UInt32(clamping: Int(date.timeIntervalSince1970 / 86_400))
    }

    /// How `device` names itself in a hello on `day`: the first 16 bytes of
    /// SHA-256 over the hint prefix, the id, and the day, big-endian.
    public static func hint(device: String, day: UInt32) -> String {
        let dayBytes = withUnsafeBytes(of: day.bigEndian) { Data($0) }
        return Base64URL.encode(Data(SHA256.hash(data: hintPrefix + Data(device.utf8) + dayBytes).prefix(16)))
    }

    /// Whether a challenge comes from the computer pinned as `fingerprint`:
    /// its certificate hashes to the fingerprint, and that certificate's key
    /// signed the nonce and this phone's device id.
    public static func challengeIsFrom(
        _ fingerprint: Fingerprint,
        device: String,
        nonce: String,
        certificate: String,
        signature: String
    ) -> Bool {
        guard let der = Base64URL.decode(certificate),
              Fingerprint(certificate: der) == fingerprint,
              let nonceBytes = try? decodeNonce(nonce),
              let raw = Base64URL.decode(signature),
              let signature = try? P256.Signing.ECDSASignature(rawRepresentation: raw),
              let certificate = SecCertificateCreateWithData(nil, der as CFData),
              let key = SecCertificateCopyKey(certificate),
              let point = SecKeyCopyExternalRepresentation(key, nil) as Data?,
              let publicKey = try? P256.Signing.PublicKey(x963Representation: point)
        else { return false }
        return publicKey.isValidSignature(signature, for: challengePrefix + nonceBytes + Data(device.utf8))
    }

    /// Frames the phone writes.
    public enum Outgoing: Encodable, Hashable, Sendable {
        case hello(hint: String)
        case prove(nonce: String, signature: String)
        case near(Bool)

        enum CodingKeys: String, CodingKey { case type, hint, nonce, signature, near }

        public func encode(to encoder: Encoder) throws {
            var c = encoder.container(keyedBy: CodingKeys.self)
            switch self {
            case let .hello(hint):
                try c.encode("hello", forKey: .type)
                try c.encode(hint, forKey: .hint)
            case let .prove(nonce, signature):
                try c.encode("prove", forKey: .type)
                try c.encode(nonce, forKey: .nonce)
                try c.encode(signature, forKey: .signature)
            case let .near(near):
                try c.encode("near", forKey: .type)
                try c.encode(near, forKey: .near)
            }
        }
    }

    /// Frames the computer sends.
    public enum Incoming: Decodable, Hashable, Sendable {
        /// Signed by the computer's CA key (`challengeIsFrom`).
        case challenge(nonce: String, certificate: String, signature: String)
        /// Sealed addresses, or nil when the phone registered no key.
        case proven(addresses: String?)
        case error(String)
        case other

        enum CodingKeys: String, CodingKey { case type, nonce, certificate, signature, addresses, message }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            switch try c.decode(String.self, forKey: .type) {
            case "challenge":
                self = .challenge(
                    nonce: try c.decode(String.self, forKey: .nonce),
                    certificate: try c.decode(String.self, forKey: .certificate),
                    signature: try c.decode(String.self, forKey: .signature)
                )
            case "proven": self = .proven(addresses: try c.decodeIfPresent(String.self, forKey: .addresses))
            case "error": self = .error(try c.decodeIfPresent(String.self, forKey: .message) ?? "")
            default: self = .other
            }
        }
    }

    struct Addresses: Decodable {
        let origins: [String]
    }

    /// The computer's current addresses from a `proven` frame.
    public static func openAddresses(
        _ sealed: String,
        device: P256.KeyAgreement.PublicKey,
        agree: (P256.KeyAgreement.PublicKey) throws -> SharedSecret
    ) throws -> [String] {
        guard let bytes = Base64URL.decode(sealed) else { throw NotificationSeal.Failure.format }
        let plaintext = try NotificationSeal.open(bytes, device: device, info: addressesInfo, agree: agree)
        return try JSONDecoder().decode(Addresses.self, from: plaintext).origins
    }

    /// The addresses to keep: the ones the computer just sent first, as
    /// they are current, then the known ones, only `https` origins, once
    /// each, at most `maxOrigins`.
    public static func merged(known: [String], received: [String]) -> [String] {
        var seen = Set<String>()
        return (received + known)
            .filter { origin in
                guard let url = URL(string: origin), url.scheme == "https", url.host != nil, url.path.isEmpty || url.path == "/" else { return false }
                return seen.insert(origin).inserted
            }
            .prefix(maxOrigins)
            .map { $0 }
    }

    /// Near or not from signal strength, with a gap between the levels so a
    /// phone at the edge does not flap.
    public struct Nearness: Hashable, Sendable {
        public static let enter = -65
        public static let leave = -80
        public private(set) var near = false

        public init() {}

        /// Takes a reading; answers the new state when it changed.
        public mutating func update(rssi: Int) -> Bool? {
            // 127 means the radio could not read it.
            guard rssi != 127 else { return nil }
            if !near, rssi >= Self.enter {
                near = true
                return true
            }
            if near, rssi <= Self.leave {
                near = false
                return false
            }
            return nil
        }
    }
}
