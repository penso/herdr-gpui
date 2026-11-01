import CryptoKit
import Foundation
import HerdrRemoteCore
import Security

/// The keys sealed alerts are opened with (`NotificationSeal`), one per
/// paired computer: a Secure Enclave key-agreement key where there is one
/// (the keychain holds its device-bound blob), a software key on the
/// simulator. An alert counts as from the computer whose key opens it,
/// never the computer its text names, so one paired computer cannot send
/// an alert whose actions reach another. They live in the App Group's
/// keychain group, so the Notification Service Extension opens alerts
/// with the keys the app registered.
enum NotifyKey {
    static let group = "group.so.pen.herdr.remote"

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

        /// The public half as `POST /api/apns` takes it.
        var registered: String { Base64URL.encode(publicKey.x963Representation) }

        fileprivate var data: Data {
            switch self {
            case let .secureEnclave(key): Data([1]) + key.dataRepresentation
            case let .software(key): Data([0]) + key.rawRepresentation
            }
        }

        fileprivate init(_ data: Data) throws {
            let body = data.dropFirst()
            self = data.first == 1
                ? .secureEnclave(try SecureEnclave.P256.KeyAgreement.PrivateKey(dataRepresentation: body))
                : .software(try P256.KeyAgreement.PrivateKey(rawRepresentation: body))
        }
    }

    enum Failure: Error {
        case keychain(OSStatus)
    }

    /// An opened alert and the computer whose key opened it.
    struct Opened {
        let computer: Fingerprint
        let alert: SealedAlert
    }

    /// Opens an alert's `sealed` field with whichever computer's key fits,
    /// and only when the alert names that same computer.
    static func open(_ sealed: String) throws -> Opened? {
        for (computer, key) in try loadAll() {
            guard let alert = try? SealedAlert.open(sealed, device: key.publicKey, agree: { try key.agree($0) }) else { continue }
            return Fingerprint(alert.computer) == computer ? Opened(computer: computer, alert: alert) : nil
        }
        return nil
    }

    /// The key alerts from `computer` are sealed to, made on first use.
    static func loadOrCreate(for computer: Fingerprint) throws -> Stored {
        var keys = try loadAll()
        if let key = keys[computer] { return key }
        let key: Stored = SecureEnclave.isAvailable
            ? .secureEnclave(try SecureEnclave.P256.KeyAgreement.PrivateKey())
            : .software(P256.KeyAgreement.PrivateKey())
        keys[computer] = key
        try save(keys)
        return key
    }

    /// Forgets an unpaired computer's key, so its alerts no longer open.
    static func forget(_ computer: Fingerprint) throws {
        var keys = try loadAll()
        guard keys.removeValue(forKey: computer) != nil else { return }
        try save(keys)
    }

    private static func query() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "so.pen.herdr.remote",
            kSecAttrAccount as String: "notify-keys",
            kSecAttrAccessGroup as String: group,
        ]
    }

    static func loadAll() throws -> [Fingerprint: Stored] {
        var query = query()
        query[kSecReturnData as String] = true
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status != errSecItemNotFound else { return [:] }
        guard status == errSecSuccess, let data = result as? Data else { throw Failure.keychain(status) }
        let stored = try JSONDecoder().decode([String: Data].self, from: data)
        var keys: [Fingerprint: Stored] = [:]
        for (hex, blob) in stored {
            if let computer = Fingerprint(hex) { keys[computer] = try Stored(blob) }
        }
        return keys
    }

    private static func save(_ keys: [Fingerprint: Stored]) throws {
        let data = try JSONEncoder().encode(Dictionary(uniqueKeysWithValues: keys.map { ($0.key.hex, $0.value.data) }))
        let attributes: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        let status = SecItemUpdate(query() as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            let added = SecItemAdd(query().merging(attributes) { $1 } as CFDictionary, nil)
            guard added == errSecSuccess else { throw Failure.keychain(added) }
            return
        }
        guard status == errSecSuccess else { throw Failure.keychain(status) }
    }
}
