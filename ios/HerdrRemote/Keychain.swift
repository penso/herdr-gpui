import Foundation
import HerdrRemoteCore
import Security

/// Generic password items in the App Group's keychain group, so the share
/// extension signs in as this phone too. Readable after the first unlock,
/// so push tokens can be sent while the phone is locked, and never synced
/// or restored to another device.
enum Keychain {
    static let service = "so.pen.herdr.remote"
    static let group = "group.so.pen.herdr.remote"

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

    private static func query(_ account: String, group: String? = Keychain.group) -> [String: Any] {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        if let group { query[kSecAttrAccessGroup as String] = group }
        return query
    }

    static func read(_ account: String) throws -> Data? {
        try read(query(account))?.data
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

    /// Moves an item saved before the App Group held it: copied into the
    /// group first, then removed from its old group by name, so a failure
    /// between the two leaves a copy rather than none.
    static func migrate(_ account: String) throws {
        guard try read(account) == nil else { return }
        var legacy = query(account, group: nil)
        legacy[kSecReturnAttributes as String] = true
        guard let found = try read(legacy), let oldGroup = found.group, oldGroup != group else { return }
        try write(found.data, account: account)
        var old = query(account, group: oldGroup)
        old[kSecAttrAccessGroup as String] = oldGroup
        SecItemDelete(old as CFDictionary)
    }

    private static func read(_ query: [String: Any]) throws -> (data: Data, group: String?)? {
        var query = query
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        switch status {
        case errSecSuccess:
            if let data = result as? Data { return (data, nil) }
            guard let attributes = result as? [String: Any], let data = attributes[kSecValueData as String] as? Data else { return nil }
            return (data, attributes[kSecAttrAccessGroup as String] as? String)
        case errSecItemNotFound: return nil
        default: throw Failure.status(status)
        }
    }
}

/// The paired computers, kept as one JSON item. Each record holds its
/// device key: a Secure Enclave blob only this phone can use, or, on the
/// simulator, the software key itself.
enum ComputerStore {
    private static let account = "computers"

    static func load() throws -> [Computer] {
        try Keychain.migrate(account)
        guard let data = try Keychain.read(account) else { return [] }
        return try JSONDecoder().decode([Computer].self, from: data)
    }

    static func save(_ computers: [Computer]) throws {
        try Keychain.write(JSONEncoder().encode(computers), account: account)
    }
}
