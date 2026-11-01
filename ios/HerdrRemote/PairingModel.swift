import Foundation
import HerdrRemoteCore

/// One pairing, from the scanned QR to an approved, stored computer: claim
/// the token with a new device key, show the matching code, and wait for
/// Allow on the computer.
@MainActor
@Observable
final class PairingModel {
    enum Step: Equatable {
        case idle
        case working(String)
        /// The 6-digit code to compare with the computer's dialog.
        case code(String)
        case paired(String)
        case failed(String)
    }

    private(set) var step: Step = .idle
    /// The computer the last pairing kept, for the hosts it pairs next.
    private(set) var paired: Computer?
    private var task: Task<Void, Never>?

    /// How often and how long the claim's state is polled.
    private static let pollInterval: Duration = .milliseconds(1500)
    private static let pollLimit = 120

    func pair(_ text: String, deviceName: String, onPaired: @escaping (Computer) -> Void) {
        start(onPaired) { try await self.run(text, deviceName: deviceName) }
    }

    /// Pairs with a computer found nearby: the code exchange first, then
    /// the same claim as a scanned QR, pinned to the fingerprint the code
    /// vouched for.
    func pair(
        nearby computer: NearbyComputer,
        bluetooth: NearbyBluetooth,
        deviceName: String,
        onPaired: @escaping (Computer) -> Void
    ) {
        start(onPaired) { try await self.runNearby(computer, bluetooth: bluetooth, deviceName: deviceName) }
    }

    private func start(_ onPaired: @escaping (Computer) -> Void, _ work: @escaping () async throws -> Computer) {
        task?.cancel()
        task = Task {
            do {
                let computer = try await work()
                onPaired(computer)
                paired = computer
                step = .paired(computer.name)
            } catch is CancellationError {
                step = .idle
            } catch {
                step = .failed(Self.describe(error))
            }
        }
    }

    /// The error's message, plus the TLS status behind a secure connection
    /// failure: iOS words every TLS failure the same way, and the status
    /// (such as -9802, a fatal alert) is what tells them apart.
    private static func describe(_ error: Error) -> String {
        let message = error.localizedDescription
        guard let error = error as? URLError,
              let status = error.errorUserInfo["_kCFStreamErrorCodeKey"] as? Int
        else { return message }
        let host = error.failingURL?.host() ?? "?"
        return "\(message) (\(host), \(error.code.rawValue), TLS \(status))"
    }

    func cancel() {
        task?.cancel()
        paired = nil
        step = .idle
    }

    enum Failure: Error, LocalizedError {
        case link(PairingLink.ParseError)
        case denied
        case expired
        case timedOut
        case certificateChanged

        var errorDescription: String? {
            switch self {
            case .link: "That is not a Herdr pairing link. Scan the QR in Herdr's Remote control dialog."
            case .denied: "The computer denied this phone."
            case .expired: "The pairing expired. Show a new QR on the computer."
            case .timedOut: "Nobody answered on the computer."
            case .certificateChanged: "The computer's certificate changed during pairing."
            }
        }
    }

    private func run(_ text: String, deviceName: String) async throws -> Computer {
        step = .working("Connecting…")
        let link: PairingLink
        do {
            link = try PairingLink(text)
        } catch let error as PairingLink.ParseError {
            throw Failure.link(error)
        }
        let client = RemoteClient(origin: link.origin, fingerprint: link.fingerprint)
        return try await claim(at: client, token: link.token, fingerprint: link.fingerprint, deviceName: deviceName, expected: nil)
    }

    private func runNearby(_ nearby: NearbyComputer, bluetooth: NearbyBluetooth, deviceName: String) async throws -> Computer {
        step = .working("Connecting to \(nearby.name)…")
        let result: NearbySAS.Result
        var origins: [String]
        switch nearby.route {
        case let .network(name, _):
            var address = try await NearbyBrowser.resolve(name)
            #if DEBUG
            // The simulator loop's sandbox daemon is unsigned, so the macOS
            // firewall blocks it on LAN addresses: `-nearbyHost 127.0.0.1`.
            if let host = UserDefaults.standard.string(forKey: "nearbyHost") { address.host = host }
            #endif
            result = try await NearbySAS().exchange(NearbyClient(host: address.host, port: address.port).io)
            // The address Bonjour reached first, then the ones the computer
            // listed.
            origins = Computer.origins(pairedAt: "https://\(address.host):\(address.port)", served: result.offer.origins)
        case let .bluetooth(identifier):
            result = try await NearbySAS().exchange(bluetooth.io(for: identifier))
            origins = result.offer.origins
        }
        step = .code(result.code)
        // The claim pins the fingerprint the code vouched for.
        for origin in origins {
            let client = RemoteClient(origin: origin, fingerprint: result.fingerprint)
            guard await client.ping() else { continue }
            return try await claim(
                at: client, token: result.offer.token, fingerprint: result.fingerprint,
                deviceName: deviceName, expected: result.code
            )
        }
        throw RemoteError.unreachable
    }

    /// Claims `token` with a new device key, then waits for Allow. A nearby
    /// claim must carry the code this phone computed.
    private func claim(
        at client: RemoteClient,
        token: String,
        fingerprint: Fingerprint,
        deviceName: String,
        expected: String?
    ) async throws -> Computer {
        let key = try DeviceKey.create()
        let claim = try await client.claim(API.ClaimRequest(
            token: token,
            name: deviceName,
            publicKey: DeviceKeyEncoding.publicKey(key.publicKey)
        ))
        if let expected, claim.code != expected { throw NearbyFailure.codeMismatch }
        step = .code(claim.code)
        for _ in 0..<Self.pollLimit {
            try await Task.sleep(for: Self.pollInterval)
            switch try await client.claimStatus(claim.claim) {
            case .waiting:
                continue
            case .denied:
                throw Failure.denied
            case .expired:
                throw Failure.expired
            case let .approved(approval):
                guard approval.fingerprint == fingerprint else { throw Failure.certificateChanged }
                return Computer(
                    device: approval.device,
                    name: approval.computer,
                    origins: Computer.origins(pairedAt: client.origin, served: approval.origins),
                    fingerprint: fingerprint,
                    keyKind: key.kind,
                    keyData: key.data
                )
            }
        }
        throw Failure.timedOut
    }
}
