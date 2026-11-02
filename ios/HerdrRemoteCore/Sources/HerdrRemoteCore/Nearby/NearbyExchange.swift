import Foundation

/// How the two nearby messages reach the computer: its HTTPS port found by
/// Bonjour, or the `pair` characteristic over Bluetooth.
public struct NearbyIO: Sendable {
    public var start: @Sendable (_ publicKey: String) async throws -> NearbySAS.Hello
    public var reveal: @Sendable (_ session: String, _ nonce: String) async throws -> NearbySAS.Reveal

    public init(
        start: @escaping @Sendable (String) async throws -> NearbySAS.Hello,
        reveal: @escaping @Sendable (String, String) async throws -> NearbySAS.Reveal
    ) {
        self.start = start
        self.reveal = reveal
    }
}

extension NearbySAS {
    /// Runs both steps: sends `PKa`, takes the commitment, and only then
    /// reveals `Na`.
    public func exchange(_ io: NearbyIO) async throws -> Result {
        let hello = try await io.start(publicKey)
        let reveal = try await io.reveal(hello.session, nonceText)
        return try finish(hello: hello, reveal: reveal)
    }
}

/// A computer advertising Herdr's remote control nearby.
public struct NearbyComputer: Hashable, Sendable, Identifiable {
    public enum Route: Hashable, Sendable {
        /// Its HTTPS port on the local network.
        case network(host: String, port: Int)
        /// Its Bluetooth peripheral.
        case bluetooth(identifier: UUID)
    }

    public var name: String
    /// The first 16 hex digits of its CA fingerprint, to tell paired
    /// computers apart from new ones before any key is exchanged.
    public var fingerprintPrefix: String
    /// Its pairing dialog is open, so it accepts a nearby pairing now.
    public var pairing: Bool
    public var route: Route

    public var id: String { "\(fingerprintPrefix)/\(route)" }

    public init(name: String, fingerprintPrefix: String, pairing: Bool, route: Route) {
        self.name = name
        self.fingerprintPrefix = fingerprintPrefix.uppercased()
        self.pairing = pairing
        self.route = route
    }

    /// Whether `fingerprint` is this computer's.
    public func matches(_ fingerprint: Fingerprint) -> Bool {
        !fingerprintPrefix.isEmpty && fingerprint.hex.hasPrefix(fingerprintPrefix)
    }

    /// Reads a Bonjour TXT record: `v=1`, `fp=`, `pairing=`.
    public static func fromTXT(_ record: [String: String], name: String, route: Route) -> NearbyComputer? {
        guard record["v"] == "1", let prefix = record["fp"], prefix.count == 16,
              prefix.allSatisfy(\.isHexDigit)
        else { return nil }
        return NearbyComputer(name: name, fingerprintPrefix: prefix, pairing: record["pairing"] == "1", route: route)
    }
}
