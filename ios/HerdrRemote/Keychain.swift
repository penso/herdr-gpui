import Foundation
import HerdrRemoteCore
import Security

/// Generic password items of this app, readable after the first unlock so
/// push tokens can be sent while the phone is locked, and never synced or
/// restored to another device.
enum Keychain {
    static let service = "so.pen.herdr-remote"

    enum Failure: Error, LocalizedError {
        case status(OSStatus)

        var errorDescription: String? {
            switch self {
            case let .status(status):
                let message = SecCopyErrorMessageString(status, nil) as String?
                return "Keychain: \(message ?? "error \(status)")"
            }
        }
    }

    private static func query(_ account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    static func read(_ account: String) throws -> Data? {
        var query = query(account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        switch status {
        case errSecSuccess: return result as? Data
        case errSecItemNotFound: return nil
        default: throw Failure.status(status)
        }
    }

    static func write(_ data: Data, account: String) throws {
        let attributes: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        let status = SecItemUpdate(query(account) as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            let added = SecItemAdd(query(account).merging(attributes) { $1 } as CFDictionary, nil)
            guard added == errSecSuccess else { throw Failure.status(added) }
            return
        }
        guard status == errSecSuccess else { throw Failure.status(status) }
    }
}

/// The paired computers, kept as one JSON item. Each record holds its
/// device key: a Secure Enclave blob only this phone can use, or, on the
/// simulator, the software key itself.
enum ComputerStore {
    private static let account = "computers"

    static func load() throws -> [Computer] {
        guard let data = try Keychain.read(account) else { return [] }
        return try JSONDecoder().decode([Computer].self, from: data)
    }

    static func save(_ computers: [Computer]) throws {
        try Keychain.write(JSONEncoder().encode(computers), account: account)
    }
}
