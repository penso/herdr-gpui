import CryptoKit
import Foundation
import HerdrRemoteCore
import Security

/// The key that proves this phone to a computer: created at pairing, its
/// public half sent with the claim, and used to sign every session
/// challenge. In the Secure Enclave when the phone has one, so it cannot
/// leave the device; a software key on the simulator.
enum DeviceKey {
    case secureEnclave(SecureEnclave.P256.Signing.PrivateKey)
    case software(P256.Signing.PrivateKey)

    enum Failure: Error, LocalizedError {
        case accessControl

        var errorDescription: String? {
            "Could not set up the Secure Enclave key's access control."
        }
    }

    /// A new key for a new pairing.
    static func create() throws -> DeviceKey {
        guard SecureEnclave.isAvailable else { return .software(P256.Signing.PrivateKey()) }
        guard let access = SecAccessControlCreateWithFlags(
            nil,
            // Signing in the background, after the first unlock, keeps push
            // token updates flowing while the phone is locked.
            kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
            .privateKeyUsage,
            nil
        ) else { throw Failure.accessControl }
        return .secureEnclave(try SecureEnclave.P256.Signing.PrivateKey(accessControl: access))
    }

    /// The key a stored pairing names.
    init(kind: Computer.KeyKind, data: Data) throws {
        switch kind {
        case .secureEnclave:
            self = .secureEnclave(try SecureEnclave.P256.Signing.PrivateKey(dataRepresentation: data))
        case .software:
            self = .software(try P256.Signing.PrivateKey(rawRepresentation: data))
        }
    }

    var kind: Computer.KeyKind {
        switch self {
        case .secureEnclave: .secureEnclave
        case .software: .software
        }
    }

    /// What a pairing record keeps to rebuild the key.
    var data: Data {
        switch self {
        case let .secureEnclave(key): key.dataRepresentation
        case let .software(key): key.rawRepresentation
        }
    }

    var publicKey: P256.Signing.PublicKey {
        switch self {
        case let .secureEnclave(key): key.publicKey
        case let .software(key): key.publicKey
        }
    }

    /// ECDSA P-256 with SHA-256 over `message`.
    func signature(for message: Data) throws -> P256.Signing.ECDSASignature {
        switch self {
        case let .secureEnclave(key): try key.signature(for: message)
        case let .software(key): try key.signature(for: message)
        }
    }
}
