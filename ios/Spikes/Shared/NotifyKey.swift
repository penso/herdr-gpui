import CryptoKit
import Foundation
import Security

/// The key sealed notifications are opened with: a Secure Enclave
/// key-agreement key where there is one (its device-bound blob is what the
/// keychain holds), a software key on the simulator. Kept in the App
/// Group's keychain access group so the Notification Service Extension can
/// use the key the app made.
enum NotifyKey {
    enum Stored {
        case secureEnclave(SecureEnclave.P256.KeyAgreement.PrivateKey)
        case software(P256.KeyAgreement.PrivateKey)

        var publicKey: P256.KeyAgreement.PublicKey {
            switch self {
            case let .secureEnclave(key): key.publicKey
            case let .software(key): key.publicKey
            }
        }

        func agree(_ other: P256.KeyAgreement.PublicKey) throws -> SharedSecret {
            switch self {
            case let .secureEnclave(key): try key.sharedSecretFromKeyAgreement(with: other)
            case let .software(key): try key.sharedSecretFromKeyAgreement(with: other)
            }
        }
    }

    enum Failure: Error {
        case keychain(OSStatus)
    }

    private static func query() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "so.pen.herdr-spikes",
            kSecAttrAccount as String: SpikeIDs.notifyKeyAccount,
            kSecAttrAccessGroup as String: SpikeIDs.keychainGroup,
        ]
    }

    /// The stored key, or nil before the app made one.
    static func load() throws -> Stored? {
        var query = query()
        query[kSecReturnData as String] = true
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status != errSecItemNotFound else { return nil }
        guard status == errSecSuccess, let data = result as? Data, let kind = data.first else {
            throw Failure.keychain(status)
        }
        let body = data.dropFirst()
        return kind == 1
            ? .secureEnclave(try SecureEnclave.P256.KeyAgreement.PrivateKey(dataRepresentation: body))
            : .software(try P256.KeyAgreement.PrivateKey(rawRepresentation: body))
    }

    /// Makes and stores a new key, replacing any old one.
    static func create() throws -> Stored {
        let stored: Stored
        let data: Data
        if SecureEnclave.isAvailable {
            let key = try SecureEnclave.P256.KeyAgreement.PrivateKey()
            stored = .secureEnclave(key)
            data = Data([1]) + key.dataRepresentation
        } else {
            let key = P256.KeyAgreement.PrivateKey()
            stored = .software(key)
            data = Data([0]) + key.rawRepresentation
        }
        SecItemDelete(query() as CFDictionary)
        var add = query()
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw Failure.keychain(status) }
        return stored
    }
}
