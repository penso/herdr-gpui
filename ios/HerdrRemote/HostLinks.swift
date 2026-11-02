import Foundation
import HerdrRemoteCore
import Observation
import os

private let log = Logger(subsystem: "so.pen.herdr.remote", category: "hosts")

/// Pairs the phone with the other hosts each paired computer's Herdr UI
/// knows, so the phone reaches their own daemons while that computer
/// sleeps. A computer is asked for its hosts right after pairing it and
/// when its link comes online (`HostSchedule`); each host the desktop
/// allowed gets a new key, and posted keys are followed until the host
/// holds them or fails (`HostPairings`).
@MainActor
@Observable
final class HostLinks {
    /// A host of one computer, for a key being made and posted.
    struct Posting: Hashable {
        let computer: Fingerprint
        let host: String
    }

    /// By the fingerprint of the computer that vouches for the hosts.
    private(set) var pairings: [Fingerprint: HostPairings] = [:]
    /// Computers asked for their hosts whose first answer, and the keys it
    /// starts, are not in yet.
    private(set) var checking: Set<Fingerprint> = []
    private(set) var posting: Set<Posting> = []
    /// The computers on this phone.
    @ObservationIgnored var computers: () -> [Computer] = { [] }
    /// Keeps a host the phone is now paired with.
    @ObservationIgnored var onPaired: (Computer) -> Void = { _ in }
    @ObservationIgnored private var schedule = HostSchedule()
    @ObservationIgnored private var runs: [Fingerprint: Task<Void, Never>] = [:]
    private let sessions: Sessions

    private static let pollInterval: Duration = .seconds(2)
    /// Failed lists in a row before polling stops until the next check.
    private static let listRetries = 15

    init(sessions: Sessions) {
        self.sessions = sessions
    }

    /// Asks `computer` for its hosts when it is due; `force` for a computer
    /// just paired.
    func check(_ computer: Computer, force: Bool = false) {
        guard schedule.due(computer, at: .now, force: force) else { return }
        run(computer, automatic: true)
    }

    /// Pairs `host` of `computer` by hand: an offered host asks the desktop
    /// to confirm, a failed one retries, a paired one gets a new key.
    func pair(_ host: HostLink, through computer: Computer) {
        Task {
            await post(host.host, through: computer)
            run(computer, automatic: false)
        }
    }

    func forget(_ computer: Computer) {
        runs.removeValue(forKey: computer.fingerprint)?.cancel()
        pairings[computer.fingerprint] = nil
        checking.remove(computer.fingerprint)
    }

    /// The hosts of `computer` whose keys were posted, as they stand.
    func progress(_ computer: Computer) -> [HostLink] {
        pairings[computer.fingerprint]?.progress ?? []
    }

    /// The rows the Computers tab shows under `computer`: hosts being
    /// paired, then the ones the person may pair.
    func rows(_ computer: Computer) -> [HostLink] {
        guard computer.via == nil, let pairings = pairings[computer.fingerprint] else { return [] }
        let waiting = pairings.hosts.filter { pairings.pending[$0.host] != nil }
        return waiting + pairings.offers(held: computers())
    }

    private func run(_ computer: Computer, automatic: Bool) {
        let fingerprint = computer.fingerprint
        guard runs[fingerprint] == nil else { return }
        if automatic { checking.insert(fingerprint) }
        runs[fingerprint] = Task {
            await follow(computer, automatic: automatic)
            checking.remove(fingerprint)
            runs[fingerprint] = nil
        }
    }

    /// Lists `computer`'s hosts, posting keys for the allowed ones on the
    /// first answer when `automatic`, then polls while a key is waiting.
    private func follow(_ computer: Computer, automatic: Bool) async {
        let fingerprint = computer.fingerprint
        var first = automatic
        var failures = 0
        while !Task.isCancelled {
            if let hosts = await list(computer) {
                failures = 0
                let added = pairings[fingerprint, default: HostPairings()]
                    .update(hosts, vouching: fingerprint, held: computers(), at: .now)
                added.forEach(onPaired)
                if first {
                    for host in pairings[fingerprint]?.automatic ?? [] {
                        await post(host.host, through: computer)
                    }
                    first = false
                }
                checking.remove(fingerprint)
            } else {
                failures += 1
                checking.remove(fingerprint)
                if failures >= Self.listRetries { return }
            }
            guard pairings[fingerprint]?.waiting == true else { return }
            do { try await Task.sleep(for: Self.pollInterval) } catch { return }
        }
    }

    private func list(_ computer: Computer) async -> [HostLink]? {
        do {
            let (client, token) = try await sessions.session(for: computer)
            return try await client.hosts(bearer: token)
        } catch {
            await expire(computer, after: error)
            // A daemon from before host pairing answers 404 here.
            log.info("\(computer.name, privacy: .public) listed no hosts: \(error.localizedDescription, privacy: .public)")
            return nil
        }
    }

    /// Makes a key for `host` alone and hands its public half to
    /// `computer`, which has Herdr UI enroll it on the host.
    private func post(_ host: String, through computer: Computer) async {
        let fingerprint = computer.fingerprint
        let posting = Posting(computer: fingerprint, host: host)
        guard self.posting.insert(posting).inserted else { return }
        defer { self.posting.remove(posting) }
        do {
            let key = try DeviceKey.create()
            let made = HostKey(publicKey: DeviceKeyEncoding.publicKey(key.publicKey), kind: key.kind, data: key.data)
            let (client, token) = try await sessions.session(for: computer)
            let answer = try await client.pairHost(host, publicKey: made.publicKey, bearer: token)
            pairings[fingerprint, default: HostPairings()].posted(host, key: made, answer: answer, at: .now)
        } catch {
            await expire(computer, after: error)
            log.error("\(computer.name, privacy: .public) refused a key for \(host, privacy: .public): \(error.localizedDescription, privacy: .public)")
            pairings[fingerprint, default: HostPairings()].refused(host, reason: Self.reason(error))
        }
    }

    /// A rejected bearer token is dropped so the next call signs a new one.
    private func expire(_ computer: Computer, after error: Error) async {
        guard case RemoteError.status(401, _) = error else { return }
        await sessions.forget(computer)
    }

    private static func reason(_ error: Error) -> String {
        switch error {
        case RemoteError.status(429, _): "Already being paired. Try again in a minute."
        case RemoteError.status(404, _): "The computer no longer knows this host."
        default: error.localizedDescription
        }
    }
}
