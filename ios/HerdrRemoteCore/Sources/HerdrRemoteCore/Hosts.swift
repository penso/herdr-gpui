import Foundation

/// One of the other hosts (SSH and cloud machines) a paired computer's
/// Herdr UI knows, and where this phone stands with it: `GET /api/v2/hosts`
/// (`crates/herdr-remote-control/src/web/hosts.rs`). Pairing one gives the
/// phone that host's own daemon, so it stays reachable while the computer
/// that vouched for it sleeps.
public struct HostLink: Decodable, Equatable, Identifiable, Sendable {
    /// Herdr UI's endpoint id, such as `ssh:devbox` or `coder:ID`.
    public let host: String
    public let name: String
    public var state: HostState

    public var id: String { host }

    public init(host: String, name: String, state: HostState) {
        self.host = host
        self.name = name
        self.state = state
    }

    enum CodingKeys: String, CodingKey { case host, name }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        host = try container.decode(String.self, forKey: .host)
        name = try container.decode(String.self, forKey: .name)
        // The state is flattened beside the host's id and name.
        state = try HostState(from: decoder)
    }
}

/// Where the phone stands with one host, tagged by `state`.
public enum HostState: Decodable, Equatable, Sendable {
    /// Known, not allowed for this phone: pairing it asks the desktop first.
    case offered
    /// Allowed on the desktop; waiting for the phone's key.
    case allowed
    /// The phone asked for an offered host; the desktop has not answered.
    case asking
    /// Herdr UI is enrolling the phone's key on the host.
    case pending
    /// The host's daemon holds the phone's key.
    case paired(Enrolled)
    /// The last attempt failed; pairing again retries.
    case failed(reason: String)

    /// What the phone needs to reach a paired host directly.
    public struct Enrolled: Decodable, Equatable, Sendable {
        /// The device id the host's daemon gave this phone.
        public let device: String
        /// The host's name, as its own daemon reports it.
        public let computer: String
        /// The host's CA.
        public let fingerprint: Fingerprint
        /// The host's addresses, in the order to try them.
        public let origins: [String]

        public init(device: String, computer: String, fingerprint: Fingerprint, origins: [String]) {
            self.device = device
            self.computer = computer
            self.fingerprint = fingerprint
            self.origins = origins
        }
    }

    enum CodingKeys: String, CodingKey { case state, reason }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let state = try container.decode(String.self, forKey: .state)
        switch state {
        case "offered": self = .offered
        case "allowed": self = .allowed
        case "asking": self = .asking
        case "pending": self = .pending
        case "paired": self = .paired(try Enrolled(from: decoder))
        case "failed": self = .failed(reason: try container.decode(String.self, forKey: .reason))
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .state, in: container, debugDescription: "unknown host state \(state)"
            )
        }
    }
}

extension API {
    /// `GET /api/v2/hosts`.
    public struct Hosts: Decodable, Equatable, Sendable {
        public let hosts: [HostLink]
    }

    /// `POST /api/v2/hosts/{host}/pair`, answered with a bare `HostState`.
    public struct HostPairRequest: Encodable, Equatable, Sendable {
        /// The key made for this host alone, encoded as a claim's.
        public let publicKey: String

        public init(publicKey: String) { self.publicKey = publicKey }

        enum CodingKeys: String, CodingKey { case publicKey = "public_key" }
    }
}

extension Computer {
    /// The record for a host `vouching` paired this phone with. The phone
    /// trusts the host's fingerprint because `vouching`, reached over its
    /// own pinned CA, reported it.
    public init(host enrolled: HostState.Enrolled, key: HostKey, via vouching: Fingerprint) {
        var seen = Set<String>()
        self.init(
            device: enrolled.device,
            name: enrolled.computer,
            origins: enrolled.origins.filter { seen.insert($0).inserted },
            fingerprint: enrolled.fingerprint,
            keyKind: key.kind,
            keyData: key.data,
            via: vouching
        )
    }
}

/// A device key made for one host, as the pairing record will keep it.
public struct HostKey: Equatable, Sendable {
    /// The public half, encoded as a claim's `public_key`.
    public let publicKey: String
    public let kind: Computer.KeyKind
    public let data: Data

    public init(publicKey: String, kind: Computer.KeyKind, data: Data) {
        self.publicKey = publicKey
        self.kind = kind
        self.data = data
    }
}
