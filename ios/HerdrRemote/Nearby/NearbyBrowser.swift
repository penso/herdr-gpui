import Foundation
import HerdrRemoteCore
import Network
import Observation

/// Computers advertising `_herdr-remote._tcp` on the local network, with
/// their TXT record (`pairing=1` while their pairing dialog is open). Runs
/// only while the app is in front.
@MainActor
@Observable
final class NearbyBrowser {
    private(set) var computers: [NearbyComputer] = []
    private var browser: NWBrowser?

    nonisolated static let service = "_herdr-remote._tcp"

    func start() {
        guard browser == nil else { return }
        let browser = NWBrowser(for: .bonjourWithTXTRecord(type: Self.service, domain: nil), using: .tcp)
        browser.browseResultsChangedHandler = { [weak self] results, _ in
            let found = results.compactMap(Self.computer)
            Task { @MainActor in self?.computers = found }
        }
        browser.start(queue: .main)
        self.browser = browser
    }

    func stop() {
        browser?.cancel()
        browser = nil
        computers = []
    }

    private nonisolated static func computer(_ result: NWBrowser.Result) -> NearbyComputer? {
        guard case let .service(name, _, _, _) = result.endpoint,
              case let .bonjour(txt) = result.metadata
        else { return nil }
        // The host and port resolve when pairing starts; until then the
        // service name stands for the route.
        return NearbyComputer.fromTXT(txt.dictionary, name: name, route: .network(host: name, port: 0))
    }

    /// The address a service resolves to, by opening a TCP connection to it.
    nonisolated static func resolve(_ name: String) async throws -> (host: String, port: Int) {
        let connection = NWConnection(to: .service(name: name, type: service, domain: "local.", interface: nil), using: .tcp)
        defer { connection.cancel() }
        return try await withCheckedThrowingContinuation { continuation in
            let resumed = Resumed()
            connection.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    if case let .hostPort(host, port) = connection.currentPath?.remoteEndpoint, resumed.once() {
                        var text = "\(host)"
                        // An IPv6 host keeps its brackets in a URL; the
                        // scope suffix is dropped.
                        if case .ipv6 = host {
                            text = "[\(text.split(separator: "%").first.map(String.init) ?? text)]"
                        }
                        continuation.resume(returning: (text, Int(port.rawValue)))
                    }
                case let .failed(error), let .waiting(error):
                    if resumed.once() { continuation.resume(throwing: error) }
                default: break
                }
            }
            connection.start(queue: .global())
        }
    }
}

/// Lets exactly one state callback resume a continuation.
private final class Resumed: @unchecked Sendable {
    // Guarded by `lock`; Network calls the handler on one queue, but the
    // lock keeps this correct whatever queue it uses.
    private let lock = NSLock()
    private var done = false

    func once() -> Bool {
        lock.withLock {
            defer { done = true }
            return !done
        }
    }
}
