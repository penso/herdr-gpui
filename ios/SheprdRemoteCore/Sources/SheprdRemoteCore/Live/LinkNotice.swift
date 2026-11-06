import Foundation

/// What the connection pill says about the computers that are not live,
/// once the person's closed notices are taken out. Closing one hides it
/// for those computers until each is online again, so a computer that
/// stays down never asks twice, and the next outage still shows.
public struct LinkNotice: Hashable, Sendable {
    /// Worst first: the order a pill picks in.
    public enum Kind: Int, Hashable, Sendable, Comparable {
        case revoked, outdated, offline, connecting

        public static func < (a: Kind, b: Kind) -> Bool { a.rawValue < b.rawValue }

        init?(_ status: LinkStatus) {
            switch status {
            case .revoked: self = .revoked
            case .outdated: self = .outdated
            case .offline: self = .offline
            case .connecting: self = .connecting
            case .online: return nil
            }
        }
    }

    /// One computer under one kind of notice, as closing it remembers.
    public struct Key: Hashable, Sendable {
        public var device: String
        public var kind: Kind

        public init(device: String, kind: Kind) {
            self.device = device
            self.kind = kind
        }
    }

    public var kind: Kind
    /// The computers it names, in the order given.
    public var devices: [String]

    /// What closing this notice hides.
    public var keys: Set<Key> { Set(devices.map { Key(device: $0, kind: kind) }) }

    /// The notice to show for `links`, or none. Connecting counts only once
    /// it has lasted (`patient`); kinds in `skipping` show elsewhere.
    public static func worst(
        _ links: [(device: String, status: LinkStatus)],
        patient: Bool,
        dismissed: Set<Key>,
        skipping: Set<Kind> = []
    ) -> LinkNotice? {
        var named: [Kind: [String]] = [:]
        for link in links {
            guard let kind = Kind(link.status), !skipping.contains(kind),
                  kind != .connecting || patient,
                  !dismissed.contains(Key(device: link.device, kind: kind))
            else { continue }
            named[kind, default: []].append(link.device)
        }
        guard let kind = named.keys.min(), let devices = named[kind] else { return nil }
        return LinkNotice(kind: kind, devices: devices)
    }

    /// `dismissed` without the computers now online, whose next problem
    /// is news again.
    public static func cleared(_ dismissed: Set<Key>, online: Set<String>) -> Set<Key> {
        dismissed.filter { !online.contains($0.device) }
    }
}
