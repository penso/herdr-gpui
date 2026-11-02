import Foundation
import HerdrRemoteCore

/// The two nearby pairing calls over the network. They run before the
/// phone knows the computer's CA, so this session accepts any certificate:
/// nothing here is secret or trusted until the six-digit code both screens
/// show matches, which authenticates the exchange (and the fingerprint the
/// claim then pins). No other call uses this session.
final class NearbyClient: NSObject, URLSessionDelegate, Sendable {
    let origin: String
    private let session: URLSession

    init(host: String, port: Int) {
        origin = "https://\(host):\(port)"
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 10
        session = URLSession(configuration: configuration, delegate: TrustForCode(), delegateQueue: nil)
    }

    var io: NearbyIO {
        NearbyIO(
            start: { [self] key in try await post("/api/nearby/start", ["public_key": key]) },
            reveal: { [self] session, nonce in try await post("/api/nearby/reveal", ["session": session, "nonce": nonce]) }
        )
    }

    private func post<Answer: Decodable>(_ path: String, _ body: [String: String]) async throws -> Answer {
        guard let url = URL(string: origin + path) else { throw RemoteError.unreachable }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.httpBody = try JSONEncoder().encode(body)
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else { throw RemoteError.notHTTP }
        guard http.statusCode == 200 else {
            if http.statusCode == 409 { throw NearbyFailure.notPairing }
            throw RemoteError.status(http.statusCode, String(decoding: data.prefix(300), as: UTF8.self))
        }
        return try JSONDecoder().decode(Answer.self, from: data)
    }
}

enum NearbyFailure: Error, LocalizedError {
    case notPairing
    case codeMismatch

    var errorDescription: String? {
        switch self {
        case .notPairing: "Open Remote control > Pair device on the computer first."
        case .codeMismatch: "The computer answered with another code. Pairing stopped."
        }
    }
}

/// Accepts the certificate for the code-authenticated nearby calls only.
private final class TrustForCode: NSObject, URLSessionDelegate, Sendable {
    func urlSession(
        _ session: URLSession,
        didReceive challenge: URLAuthenticationChallenge
    ) async -> (URLSession.AuthChallengeDisposition, URLCredential?) {
        guard let trust = challenge.protectionSpace.serverTrust else { return (.performDefaultHandling, nil) }
        return (.useCredential, URLCredential(trust: trust))
    }
}
