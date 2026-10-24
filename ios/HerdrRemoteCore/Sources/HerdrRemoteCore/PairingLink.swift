import Foundation

/// The pairing QR Herdr UI shows: `https://ADDRESS:PORT/pair#t=TOKEN&fp=FINGERPRINT`.
/// The token and fingerprint sit after `#`, so they never reach a server log.
public struct PairingLink: Equatable, Sendable {
    /// `https://ADDRESS:PORT`, the computer's address at pairing.
    public let origin: String
    /// The single-use pairing token, base64url.
    public let token: String
    /// The computer's CA, which the app pins.
    public let fingerprint: Fingerprint

    public enum ParseError: Error, Equatable {
        case notAURL
        case notHTTPS
        case notThePairingPage
        case missingToken
        case missingFingerprint
    }

    /// Reads a scanned or pasted link, with surrounding whitespace allowed.
    public init(_ text: String) throws {
        guard let url = URL(string: text.trimmingCharacters(in: .whitespacesAndNewlines)),
              let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
              let host = components.host, !host.isEmpty
        else { throw ParseError.notAURL }
        guard components.scheme?.lowercased() == "https",
              components.user == nil, components.password == nil
        else { throw ParseError.notHTTPS }
        guard components.path == "/pair" else { throw ParseError.notThePairingPage }

        var fragment = URLComponents()
        fragment.percentEncodedQuery = components.percentEncodedFragment ?? ""
        let items = fragment.queryItems ?? []
        let value = { (name: String) in items.first { $0.name == name }?.value }

        guard let token = value("t"), !token.isEmpty, Base64URL.decode(token) != nil
        else { throw ParseError.missingToken }
        guard let fingerprintText = value("fp"), let fingerprint = Fingerprint(fingerprintText)
        else { throw ParseError.missingFingerprint }

        // An IPv6 host keeps its brackets in an origin.
        let hostPart = host.contains(":") && !host.hasPrefix("[") ? "[\(host)]" : host
        let portPart = components.port.map { ":\($0)" } ?? ""
        origin = "https://\(hostPart)\(portPart)"
        self.token = token
        self.fingerprint = fingerprint
    }
}
