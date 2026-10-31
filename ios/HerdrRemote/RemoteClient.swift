import CryptoKit
import Foundation
import HerdrRemoteCore
import Security

/// What the terminal's action sheet asks of Herdr.
enum TermAction: String, Encodable, CaseIterable {
    case splitRight = "split_right"
    case splitDown = "split_down"
    case zoom
    case newTab = "new_tab"
    case closePane = "close_pane"
    case closeTab = "close_tab"
}

/// Trusts a server only through the computer's own CA: the presented chain
/// must contain the certificate whose SHA-256 the pairing QR carried, and
/// the chain must then verify with that certificate as its only anchor
/// (host name and validity included). No profile install is needed, and no
/// other CA, public or installed, is accepted for these addresses.
final class PinnedTrust: NSObject, URLSessionDelegate, Sendable {
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
final class RemoteClient: Sendable {
    let origin: String
    let fingerprint: Fingerprint
    let session: URLSession

    init(origin: String, fingerprint: Fingerprint) {
        self.origin = origin
        self.fingerprint = fingerprint
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
        (try? await fingerprint()) != nil
    }

    /// The fingerprint the computer at this address reports, within three
    /// seconds: an address saved on one network may be another computer
    /// on the next.
    func fingerprint() async throws -> String {
        struct Ping: Decodable { let fingerprint: String }
        let ping: Ping = try await send("GET", "/api/ping", timeout: 3)
        return ping.fingerprint
    }

    /// A chat's whole transcript (`GET /api/v2/chat/{pane}`).
    func chat(_ pane: String, bearer: String) async throws -> ChatSnapshot {
        try await send("GET", "/api/v2/chat/\(Self.segment(pane))", bearer: bearer)
    }

    /// Sends a chat command; answers why the chat refused it, if it did.
    func command(_ command: ChatCommand, pane: String, bearer: String) async throws -> String? {
        struct Answer: Decodable { let rejected: String? }
        let answer: Answer = try await send(
            "POST", "/api/chat/\(Self.segment(pane))/command", body: command, bearer: bearer
        )
        return answer.rejected
    }

    /// Uploads a picture for a prompt (JPEG, PNG, GIF, or WebP, at most
    /// 3.5 MB); answers the id a prompt's attachments name it by, and
    /// reports the share of its bytes sent as they go.
    func upload(
        _ image: Data,
        pane: String,
        bearer: String,
        progress: @escaping @Sendable (Double) -> Void = { _ in }
    ) async throws -> String {
        struct Answer: Decodable { let id: String }
        guard let url = URL(string: origin + "/api/chat/\(Self.segment(pane))/upload") else { throw RemoteError.unreachable }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 60
        request.setValue("application/octet-stream", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
        let (data, response) = try await session.upload(for: request, from: image, delegate: UploadWatcher(report: progress))
        let answer: Answer = try decode(data, response)
        return answer.id
    }

    /// The agents and folders a new workspace or chat may use.
    func newOptions(bearer: String) async throws -> NewOptions {
        try await send("GET", "/api/v2/new", bearer: bearer)
    }

    /// Creating waits up to ten seconds for the chat's program to report in.
    func createWorkspace(_ request: NewWorkspaceRequest, bearer: String) async throws -> Created {
        try await send("POST", "/api/v2/workspaces", body: request, bearer: bearer, timeout: 30)
    }

    func startChat(_ request: NewChatRequest, workspace: String, bearer: String) async throws -> Created {
        try await send("POST", "/api/v2/workspaces/\(Self.segment(workspace))/chats", body: request, bearer: bearer, timeout: 30)
    }

    // MARK: Hosts

    /// The other hosts this computer may pair the phone with.
    func hosts(bearer: String) async throws -> [HostLink] {
        let answer: API.Hosts = try await send("GET", "/api/v2/hosts", bearer: bearer)
        return answer.hosts
    }

    /// Hands over the key the phone made for `host`; answers where the
    /// phone now stands with it.
    func pairHost(_ host: String, publicKey: String, bearer: String) async throws -> HostState {
        try await send(
            "POST", "/api/v2/hosts/\(Self.segment(host))/pair",
            body: API.HostPairRequest(publicKey: publicKey), bearer: bearer
        )
    }

    // MARK: Terminals

    /// Keys in Herdr's syntax (`TermInput`), at most 64.
    func termKeys(_ keys: [String], pane: String, bearer: String) async throws {
        struct Body: Encodable { let keys: [String] }
        let _: EmptyAnswer = try await send("POST", "/api/term/\(Self.segment(pane))/keys", body: Body(keys: keys), bearer: bearer)
    }

    func termPaste(_ text: String, pane: String, bearer: String) async throws {
        struct Body: Encodable { let text: String }
        let _: EmptyAnswer = try await send("POST", "/api/term/\(Self.segment(pane))/paste", body: Body(text: text), bearer: bearer)
    }

    /// A picture the computer stores and types the path of, as a file
    /// dropped on a terminal would.
    func termUpload(_ image: Data, pane: String, bearer: String) async throws {
        guard let url = URL(string: origin + "/api/term/\(Self.segment(pane))/upload") else { throw RemoteError.unreachable }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 60
        request.httpBody = image
        request.setValue("application/octet-stream", forHTTPHeaderField: "Content-Type")
        request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
        let _: EmptyAnswer = try await perform(request)
    }

    /// Runs an action from the sheet; answers the pane to show next, or
    /// `nil` when the pane or tab is gone.
    func termAction(_ action: TermAction, pane: String, bearer: String) async throws -> String? {
        struct Body: Encodable { let action: TermAction }
        struct Answer: Decodable { let pane: String? }
        let answer: Answer = try await send("POST", "/api/term/\(Self.segment(pane))/action", body: Body(action: action), bearer: bearer)
        return answer.pane
    }

    /// Names the chat `pane` runs, or the terminal tab it is in; an empty
    /// name gives a chat its agent's title back.
    func rename(_ label: String, pane: String, bearer: String) async throws {
        struct Body: Encodable { let label: String }
        let _: EmptyAnswer = try await send("POST", "/api/term/\(Self.segment(pane))/rename", body: Body(label: label), bearer: bearer)
    }

    /// What was said in this computer's chats; nil from a daemon that
    /// cannot search, so the phone's index answers instead.
    func search(_ query: String, bearer: String) async throws -> SearchAnswer? {
        var components = URLComponents()
        components.queryItems = [URLQueryItem(name: "q", value: query)]
        // A query string reads `+` as a space.
        let encoded = (components.percentEncodedQuery ?? "").replacingOccurrences(of: "+", with: "%2B")
        do {
            return try await send("GET", "/api/v2/search?\(encoded)", bearer: bearer)
        } catch RemoteError.status(404, _) {
            return nil
        }
    }

    /// The live socket, authenticated like every other call.
    func live(bearer: String) -> URLSessionWebSocketTask {
        let url = URL(string: origin.replacingOccurrences(of: "https://", with: "wss://") + "/api/v2/live")
        var request = URLRequest(url: url ?? URL(filePath: "/"))
        request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
        let task = session.webSocketTask(with: request)
        // A snapshot of a long transcript is larger than the 1 MiB default.
        task.maximumMessageSize = 16 << 20
        return task
    }

    static func segment(_ text: String) -> String {
        text.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed.subtracting(CharacterSet(charactersIn: "/"))) ?? text
    }

    /// A bearer token for `device`, signing the challenge with `key`.
    func startSession(device: String, key: DeviceKey) async throws -> API.Session {
        let challenge: API.Challenge
        do {
            challenge = try await send("POST", "/api/session/challenge", body: API.ChallengeRequest(device: device))
        } catch RemoteError.status(403, _) {
            // The computer forgot this device: unpaired or revoked there.
            throw LinkError.unauthorized
        }
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
        bearer: String? = nil,
        timeout: TimeInterval? = nil
    ) async throws -> Answer {
        try await send(method, path, body: Optional<NoBody>.none, bearer: bearer, timeout: timeout)
    }

    private func send<Answer: Decodable, Body: Encodable>(
        _ method: String,
        _ path: String,
        body: Body?,
        bearer: String? = nil,
        timeout: TimeInterval? = nil
    ) async throws -> Answer {
        guard let url = URL(string: origin + path) else { throw RemoteError.unreachable }
        var request = URLRequest(url: url)
        request.httpMethod = method
        if let timeout { request.timeoutInterval = timeout }
        if let body {
            request.httpBody = try JSONEncoder().encode(body)
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        if let bearer {
            request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
        }
        return try await perform(request)
    }

    private func perform<Answer: Decodable>(_ request: URLRequest) async throws -> Answer {
        let (data, response) = try await session.data(for: request)
        return try decode(data, response)
    }

    private func decode<Answer: Decodable>(_ data: Data, _ response: URLResponse) throws -> Answer {
        guard let http = response as? HTTPURLResponse else { throw RemoteError.notHTTP }
        guard (200..<300).contains(http.statusCode) else {
            let text = String(decoding: data.prefix(300), as: UTF8.self)
            throw RemoteError.status(http.statusCode, text)
        }
        return try JSONDecoder().decode(Answer.self, from: data)
    }
}

/// Reports an upload's share of bytes sent; the session's delegate still
/// answers the server trust challenge.
private final class UploadWatcher: NSObject, URLSessionTaskDelegate, Sendable {
    let report: @Sendable (Double) -> Void

    init(report: @escaping @Sendable (Double) -> Void) {
        self.report = report
    }

    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didSendBodyData bytesSent: Int64,
        totalBytesSent: Int64,
        totalBytesExpectedToSend: Int64
    ) {
        guard totalBytesExpectedToSend > 0 else { return }
        report(Double(totalBytesSent) / Double(totalBytesExpectedToSend))
    }
}

/// Bearer tokens per paired computer, from whichever of its addresses
/// answers first in order, kept until shortly before they expire. Callers
/// that ask at once share one signing round.
actor Sessions {
    private typealias Minted = (client: RemoteClient, token: String, expires: Date)
    private var cached: [String: Minted] = [:]
    private var pending: [String: Task<Minted, Error>] = [:]

    func session(for computer: Computer) async throws -> (client: RemoteClient, token: String) {
        let minted = try await mint(computer, at: computer.origins)
        return (minted.client, minted.token)
    }

    /// A token from `origin` alone, for a link that already raced the
    /// addresses; reuses a fresh one from any address.
    func token(for computer: Computer, at origin: String) async throws -> String {
        try await mint(computer, at: [origin]).token
    }

    func forget(_ computer: Computer) {
        cached[computer.device] = nil
    }

    private func mint(_ computer: Computer, at origins: [String]) async throws -> Minted {
        if let hit = cached[computer.device], hit.expires > Date().addingTimeInterval(60) {
            return hit
        }
        if let running = pending[computer.device] {
            return try await running.value
        }
        let task = Task { try await Self.sign(computer, at: origins) }
        pending[computer.device] = task
        defer { pending[computer.device] = nil }
        let minted = try await task.value
        cached[computer.device] = minted
        return minted
    }

    private static func sign(_ computer: Computer, at origins: [String]) async throws -> Minted {
        let key = try DeviceKey(kind: computer.keyKind, data: computer.keyData)
        var lastError: Error = RemoteError.unreachable
        for origin in origins {
            let client = RemoteClient(origin: origin, fingerprint: computer.fingerprint)
            do {
                let session = try await client.startSession(device: computer.device, key: key)
                return (client, session.token, Date(timeIntervalSince1970: Double(session.expiresMs) / 1000))
            } catch LinkError.unauthorized {
                throw LinkError.unauthorized
            } catch {
                lastError = error
            }
        }
        throw lastError
    }
}
