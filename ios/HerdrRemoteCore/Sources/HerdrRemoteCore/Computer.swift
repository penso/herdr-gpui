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

    public var id: String { device }

    public init(
        device: String,
        name: String,
        origins: [String],
        fingerprint: Fingerprint,
        keyKind: KeyKind,
        keyData: Data
    ) {
        self.device = device
        self.name = name
        self.origins = origins
        self.fingerprint = fingerprint
        self.keyKind = keyKind
        self.keyData = keyData
    }

    /// The addresses to try, the pairing address first, without repeats.
    public static func origins(pairedAt home: String, served: [String]) -> [String] {
        var seen = Set<String>()
        return ([home] + served).filter { seen.insert($0).inserted }
    }
}
