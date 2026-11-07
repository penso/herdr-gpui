import Foundation

/// One paired computer, as the app keeps it in the Keychain.
public struct Computer: Codable, Hashable, Identifiable, Sendable {
    /// Which kind of device key signs for this pairing.
    public enum KeyKind: String, Codable, Hashable, Sendable {
        /// A Secure Enclave key; `keyData` is its opaque, device-bound blob.
        case secureEnclave
        /// A software key, where there is no Secure Enclave (the simulator);
        /// `keyData` is its raw private scalar.
        case software
    }

    /// The device id the daemon gave this phone.
    public var device: String
    /// The computer's name, as Live Activities name it.
    public var name: String
    /// Every address the computer answers on, the pairing address first.
    public var origins: [String]
    /// The computer's CA, the only one trusted for these addresses.
    public var fingerprint: Fingerprint
    public var keyKind: KeyKind
    public var keyData: Data
    /// Whether this phone proves it is near the computer over Bluetooth, so
    /// the computer holds alerts while you are at it. Off until chosen.
    public var presence: Bool
    /// The paired computer that vouched for this one's fingerprint when it
    /// paired the phone with one of its hosts; `nil` when paired directly.
    public var via: Fingerprint?
    /// The person's labels for it, which the computer itself keeps for all
    /// its clients; this is the copy to show while it is away (`ComputerTags`).
    public var tags: [String]
    /// When `tags` were last edited, in milliseconds since the epoch, as
    /// `updated_ms` on the computer: 0 for tags never edited.
    public var tagsUpdatedMs: UInt64

    public var id: String { device }

    enum CodingKeys: String, CodingKey { case device, name, origins, fingerprint, keyKind, keyData, presence, via, tags, tagsUpdatedMs }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        device = try c.decode(String.self, forKey: .device)
        name = try c.decode(String.self, forKey: .name)
        origins = try c.decode([String].self, forKey: .origins)
        fingerprint = try c.decode(Fingerprint.self, forKey: .fingerprint)
        keyKind = try c.decode(KeyKind.self, forKey: .keyKind)
        keyData = try c.decode(Data.self, forKey: .keyData)
        // Records saved before presence existed have it off.
        presence = try c.decodeIfPresent(Bool.self, forKey: .presence) ?? false
        // Records saved before host pairing were all paired directly.
        via = try c.decodeIfPresent(Fingerprint.self, forKey: .via)
        // Records saved before tags existed have none.
        tags = try c.decodeIfPresent([String].self, forKey: .tags) ?? []
        // Tags saved before they synced are older than any edit made on the
        // computer, but newer than none, so they reach it once.
        tagsUpdatedMs = try c.decodeIfPresent(UInt64.self, forKey: .tagsUpdatedMs) ?? (tags.isEmpty ? 0 : 1)
    }

    public init(
        device: String,
        name: String,
        origins: [String],
        fingerprint: Fingerprint,
        keyKind: KeyKind,
        keyData: Data,
        presence: Bool = false,
        via: Fingerprint? = nil,
        tags: [String] = [],
        tagsUpdatedMs: UInt64 = 0
    ) {
        self.device = device
        self.name = name
        self.origins = origins
        self.fingerprint = fingerprint
        self.keyKind = keyKind
        self.keyData = keyData
        self.presence = presence
        self.via = via
        self.tags = tags
        self.tagsUpdatedMs = tagsUpdatedMs
    }

    /// The addresses to try, the pairing address first, without repeats.
    public static func origins(pairedAt home: String, served: [String]) -> [String] {
        var seen = Set<String>()
        return ([home] + served).filter { isOrigin($0) && seen.insert($0).inserted }
    }

    /// Whether `origin` is a bare `https://HOST[:PORT]` this app may load.
    /// ATS is off (`project.yml`), so this is what keeps a served `http`
    /// address from carrying session tokens in the clear.
    public static func isOrigin(_ origin: String) -> Bool {
        guard let url = URL(string: origin) else { return false }
        return url.scheme == "https" && url.host != nil && (url.path.isEmpty || url.path == "/")
    }
}
