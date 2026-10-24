import CryptoKit
import Foundation
import HerdrRemoteCore
import Security

/// Trusts a server only through the computer's own CA: the presented chain
/// must contain the certificate whose SHA-256 the pairing QR carried, and
/// the chain must then verify with that certificate as its only anchor
/// (host name and validity included). No profile install is needed, and no
/// other CA, public or installed, is accepted for these addresses.
final class PinnedTrust: NSObject, URLSessionDelegate {
    let fingerprint: Fingerprint

    init(fingerprint: Fingerprint) {
        self.fingerprint = fingerprint
    }

    func urlSession(
        _ session: URLSession,
        didReceive challenge: URLAuthenticationChallenge
    ) async -> (URLSession.AuthChallengeDisposition, URLCredential?) {
        guard challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust,
              let trust = challenge.protectionSpace.serverTrust
        else { return (.performDefaultHandling, nil) }
        guard Self.evaluate(trust, pinned: fingerprint) else {
            return (.cancelAuthenticationChallenge, nil)
        }
        return (.useCredential, URLCredential(trust: trust))
    }

    static func evaluate(_ trust: SecTrust, pinned: Fingerprint) -> Bool {
        let chain = (SecTrustCopyCertificateChain(trust) as? [SecCertificate]) ?? []
        let der = chain.map { SecCertificateCopyData($0) as Data }
        guard let index = pinned.match(in: der) else { return false }
        guard SecTrustSetAnchorCertificates(trust, [chain[index]] as CFArray) == errSecSuccess,
              SecTrustSetAnchorCertificatesOnly(trust, true) == errSecSuccess
        else { return false }
        var error: CFError?
        return SecTrustEvaluateWithError(trust, &error)
    }
}

enum RemoteError: Error, LocalizedError {
    case status(Int, String)
    case notHTTP
    case unreachable

    var errorDescription: String? {
        switch self {
        case let .status(code, body): "The computer answered \(code): \(body)"
        case .notHTTP: "The computer's answer was not HTTP."
        case .unreachable: "None of the computer's addresses answered."
        }
    }
}

/// The daemon's API at one address, trusting only the pinned CA.
final class RemoteClient {
    let origin: String
    private let session: URLSession

    init(origin: String, fingerprint: Fingerprint) {
        self.origin = origin
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 10
        configuration.waitsForConnectivity = false
        session = URLSession(
            configuration: configuration,
            delegate: PinnedTrust(fingerprint: fingerprint),
            delegateQueue: nil
        )
    }

    deinit {
        session.finishTasksAndInvalidate()
    }

    func claim(_ request: API.ClaimRequest) async throws -> API.ClaimResponse {
        try await send("POST", "/api/pair/claim", body: request)
    }

    func claimStatus(_ claim: String) async throws -> API.ClaimStatus {
        let escaped = claim.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? claim
        return try await send("GET", "/api/pair/claim/\(escaped)")
    }

    /// Whether this address answers at all, within three seconds.
    func ping() async -> Bool {
        guard let url = URL(string: origin + "/api/ping") else { return false }
        var request = URLRequest(url: url)
        request.timeoutInterval = 3
        let response = try? await session.data(for: request).1 as? HTTPURLResponse
        return response?.statusCode == 200
    }

    /// A bearer token for `device`, signing the challenge with `key`.
    func startSession(device: String, key: DeviceKey) async throws -> API.Session {
        let challenge: API.Challenge = try await send(
            "POST", "/api/session/challenge", body: API.ChallengeRequest(device: device)
        )
        let message = try SessionMessage.bytes(nonce: challenge.nonce)
        let signature = DeviceKeyEncoding.signature(try key.signature(for: message))
        return try await send(
            "POST", "/api/session",
            body: API.SessionRequest(challenge: challenge.challenge, signature: signature)
        )
    }

    func registerAPNs(_ registration: API.APNsRegistration, bearer: String) async throws {
        let _: EmptyAnswer = try await send("POST", "/api/apns", body: registration, bearer: bearer)
    }

    private struct EmptyAnswer: Decodable {}
    private struct NoBody: Encodable {}

    private func send<Answer: Decodable>(
        _ method: String,
        _ path: String,
        bearer: String? = nil
    ) async throws -> Answer {
        try await send(method, path, body: Optional<NoBody>.none, bearer: bearer)
    }

    private func send<Answer: Decodable, Body: Encodable>(
        _ method: String,
        _ path: String,
        body: Body?,
        bearer: String? = nil
    ) async throws -> Answer {
        guard let url = URL(string: origin + path) else { throw RemoteError.unreachable }
        var request = URLRequest(url: url)
        request.httpMethod = method
        if let body {
            request.httpBody = try JSONEncoder().encode(body)
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        if let bearer {
            request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
        }
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else { throw RemoteError.notHTTP }
        guard (200..<300).contains(http.statusCode) else {
            let text = String(decoding: data.prefix(300), as: UTF8.self)
            throw RemoteError.status(http.statusCode, text)
        }
        return try JSONDecoder().decode(Answer.self, from: data)
    }
}

/// Bearer tokens per paired computer, from whichever of its addresses
/// answers first in order, kept until shortly before they expire.
actor Sessions {
    private var cached: [String: (client: RemoteClient, token: String, expires: Date)] = [:]

    func session(for computer: Computer) async throws -> (client: RemoteClient, token: String) {
        if let hit = cached[computer.device], hit.expires > Date().addingTimeInterval(60) {
            return (hit.client, hit.token)
        }
        let key = try DeviceKey(kind: computer.keyKind, data: computer.keyData)
        var lastError: Error = RemoteError.unreachable
        for origin in computer.origins {
            let client = RemoteClient(origin: origin, fingerprint: computer.fingerprint)
            do {
                let session = try await client.startSession(device: computer.device, key: key)
                let expires = Date(timeIntervalSince1970: Double(session.expiresMs) / 1000)
                cached[computer.device] = (client, session.token, expires)
                return (client, session.token)
            } catch {
                lastError = error
            }
        }
        throw lastError
    }

    func forget(_ computer: Computer) {
        cached[computer.device] = nil
    }
}
