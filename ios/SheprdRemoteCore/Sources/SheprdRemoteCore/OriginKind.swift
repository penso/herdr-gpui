import Foundation
import Network

/// What kind of address a computer's origin is, for its page to label.
public enum OriginKind: Hashable, Sendable {
    /// Tailscale's CGNAT range (100.64.0.0/10), its IPv6 prefix
    /// (fd7a:115c:a1e0::/48), or a MagicDNS name (`*.ts.net`).
    case tailscale
    /// A private or link-local address on the local network.
    case localNetwork
    /// A Bonjour name, `HOST.local`.
    case localName
    /// The computer itself (`127.0.0.1`, `::1`, `localhost`), which a phone
    /// cannot reach.
    case loopback
    case other

    public init(origin: String) {
        guard let host = URL(string: origin)?.host(percentEncoded: false) else {
            self = .other
            return
        }
        self.init(host: host)
    }

    public init(host: String) {
        let host = host.trimmingCharacters(in: CharacterSet(charactersIn: "[]")).lowercased()
        if host == "localhost" {
            self = .loopback
        } else if host.hasSuffix(".ts.net") {
            self = .tailscale
        } else if host.hasSuffix(".local") {
            self = .localName
        } else if let v4 = IPv4Address(host) {
            self = Self.kind(v4.rawValue)
        } else if let v6 = IPv6Address(host.split(separator: "%").first.map(String.init) ?? host) {
            self = Self.kind(v6)
        } else {
            self = .other
        }
    }

    /// The label the computer's page shows beside the address.
    public var label: String {
        switch self {
        case .tailscale: "Tailscale"
        case .localNetwork: "Local network"
        case .localName: "Local name"
        case .loopback: "This computer"
        case .other: "Address"
        }
    }

    private static func kind(_ bytes: Data) -> OriginKind {
        let (a, b) = (bytes[bytes.startIndex], bytes[bytes.startIndex + 1])
        switch (a, b) {
        case (127, _): return .loopback
        case (100, 64...127): return .tailscale
        case (10, _), (192, 168), (172, 16...31), (169, 254): return .localNetwork
        default: return .other
        }
    }

    private static func kind(_ address: IPv6Address) -> OriginKind {
        if address.isLoopback { return .loopback }
        if let v4 = address.asIPv4 { return kind(v4.rawValue) }
        let bytes = [UInt8](address.rawValue)
        if bytes.starts(with: [0xFD, 0x7A, 0x11, 0x5C, 0xA1, 0xE0]) { return .tailscale }
        // Unique local (fc00::/7) and link-local (fe80::/10).
        if bytes[0] & 0xFE == 0xFC || (bytes[0] == 0xFE && bytes[1] & 0xC0 == 0x80) { return .localNetwork }
        return .other
    }
}
