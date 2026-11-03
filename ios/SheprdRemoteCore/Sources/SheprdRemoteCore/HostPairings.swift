import Foundation

/// One paired computer's hosts as the phone pairs with them: the last list
/// the computer sent, the keys posted and not yet enrolled, and which hosts
/// to pair without asking.
public struct HostPairings: Sendable {
    /// How long a posted key is waited for. Enrolling may install the
    /// daemon on the host over SSH; a key still waiting after this is left
    /// for the next check, which posts a new one.
    public static let patience: TimeInterval = 600

    /// A key posted for a host, waiting for the host's daemon to hold it.
    public struct Pending: Equatable, Sendable {
        public let key: HostKey
        let since: Date
    }

    public private(set) var hosts: [HostLink] = []
    /// Keys posted and not yet enrolled, by host id. Kept in memory only: a
    /// key lost when the app quits is never used, and the next key posted
    /// for that host replaces the device it would have been.
    public private(set) var pending: [String: Pending] = [:]
    /// The hosts keys were posted for, in order, for the pairing screen.
    public private(set) var started: [String] = []

    public init() {}

    /// Whether a posted key is still being enrolled.
    public var waiting: Bool { !pending.isEmpty }

    /// Allowed hosts not already waiting on a key: the desktop chose them
    /// when it allowed the phone, so they are paired without asking.
    public var automatic: [HostLink] {
        hosts.filter { $0.state == .allowed && pending[$0.host] == nil }
    }

    /// The hosts keys were posted for, as they stand now.
    public var progress: [HostLink] {
        started.compactMap { id in hosts.first { $0.host == id } }
    }

    /// Hosts the person may pair by hand: offered ones (the desktop then
    /// confirms), failed ones (pairing again retries), and paired ones no
    /// computer on this phone has the fingerprint of (forgotten here).
    public func offers(held: [Computer]) -> [HostLink] {
        hosts.filter { link in
            guard pending[link.host] == nil else { return false }
            switch link.state {
            case .offered, .failed: return true
            case let .paired(enrolled): return !held.contains { $0.fingerprint == enrolled.fingerprint }
            case .allowed, .asking, .pending: return false
            }
        }
    }

    /// Records the key posted for `host` and the state the computer
    /// answered.
    public mutating func posted(_ host: String, key: HostKey, answer: HostState, at now: Date) {
        pending[host] = Pending(key: key, since: now)
        if !started.contains(host) { started.append(host) }
        set(host, answer)
    }

    /// Shows why the computer refused a key for `host`, until the next list.
    public mutating func refused(_ host: String, reason: String) {
        pending[host] = nil
        set(host, .failed(reason: reason))
    }

    /// Takes a fresh list from the computer whose fingerprint is
    /// `vouching`. Answers the hosts now paired with a key this phone
    /// posted, as records to keep, in the list's order; a host the phone
    /// already holds as a directly paired computer is left alone.
    public mutating func update(
        _ hosts: [HostLink],
        vouching: Fingerprint,
        held: [Computer],
        at now: Date
    ) -> [Computer] {
        self.hosts = hosts
        // A host gone from the list ended its request too.
        pending = pending.filter { id, _ in hosts.contains { $0.host == id } }
        var paired: [Computer] = []
        for link in hosts {
            let id = link.host
            guard let waiting = pending[id] else { continue }
            guard now.timeIntervalSince(waiting.since) < Self.patience else {
                pending[id] = nil
                continue
            }
            switch link.state {
            // While the computer keeps a phone's request, the host reads
            // `asking`, `pending`, or `failed`; any other state means the
            // request ended, so `paired` names the key it enrolled.
            case .asking, .pending:
                continue
            case let .paired(enrolled):
                pending[id] = nil
                let direct = held.contains { $0.fingerprint == enrolled.fingerprint && $0.via == nil }
                if !direct { paired.append(Computer(host: enrolled, key: waiting.key, via: vouching)) }
            case .offered, .allowed, .failed:
                pending[id] = nil
            }
        }
        return paired
    }

    private mutating func set(_ host: String, _ state: HostState) {
        guard let index = hosts.firstIndex(where: { $0.host == host }) else { return }
        hosts[index].state = state
    }
}

/// When to ask a computer for its hosts: right after pairing it, then each
/// time its link comes online, at most once a minute.
public struct HostSchedule: Sendable {
    public static let interval: TimeInterval = 60
    private var asked: [Fingerprint: Date] = [:]

    public init() {}

    /// Whether to ask `computer` now, recording the ask when it is.
    /// `force` skips the minute (a new pairing). A computer paired through
    /// another is never asked, so pairings do not chain.
    public mutating func due(_ computer: Computer, at now: Date, force: Bool = false) -> Bool {
        guard computer.via == nil else { return false }
        if !force, let last = asked[computer.fingerprint], now.timeIntervalSince(last) < Self.interval {
            return false
        }
        asked[computer.fingerprint] = now
        return true
    }
}
