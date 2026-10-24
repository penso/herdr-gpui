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
    private var task: Task<Void, Never>?

    /// How often and how long the claim's state is polled.
    private static let pollInterval: Duration = .milliseconds(1500)
    private static let pollLimit = 120

    func pair(_ text: String, deviceName: String, onPaired: @escaping (Computer) -> Void) {
        task?.cancel()
        task = Task {
            do {
                let computer = try await run(text, deviceName: deviceName)
                onPaired(computer)
                step = .paired(computer.name)
            } catch is CancellationError {
                step = .idle
            } catch {
                step = .failed(error.localizedDescription)
            }
        }
    }

    func cancel() {
        task?.cancel()
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
        let key = try DeviceKey.create()
        let client = RemoteClient(origin: link.origin, fingerprint: link.fingerprint)
        let claim = try await client.claim(API.ClaimRequest(
            token: link.token,
            name: deviceName,
            publicKey: DeviceKeyEncoding.publicKey(key.publicKey)
        ))
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
                guard approval.fingerprint == link.fingerprint else { throw Failure.certificateChanged }
                return Computer(
                    device: approval.device,
                    name: approval.computer,
                    origins: Computer.origins(pairedAt: link.origin, served: approval.origins),
                    fingerprint: link.fingerprint,
                    keyKind: key.kind,
                    keyData: key.data
                )
            }
        }
        throw Failure.timedOut
    }
}
