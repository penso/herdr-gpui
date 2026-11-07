import Foundation

/// The link a Live Activity opens:
/// `herdr-remote://chat?id=FINGERPRINT&computer=NAME&pane=PANE`.
/// The app finds the paired computer whose CA fingerprint is `id` (never by
/// `computer`, a display name any computer can choose), and opens the chat in
/// the web app, where it lives.
public struct ChatLink: Equatable, Sendable {
    public static let scheme = "herdr-remote"

    /// The computer's name, for messages only.
    public let computer: String
    /// The computer's CA fingerprint, as `ChatActivityFacts.computerId`.
    public let computerId: String
    public let pane: String

    public init(computer: String, computerId: String, pane: String) {
        self.computer = computer
        self.computerId = computerId
        self.pane = pane
    }

    /// Reads a link the app was opened with; `nil` for any other URL.
    public init?(url: URL) {
        guard url.scheme == Self.scheme, url.host == "chat",
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
              let computerId = items.first(where: { $0.name == "id" })?.value,
              let pane = items.first(where: { $0.name == "pane" })?.value,
              !pane.isEmpty
        else { return nil }
        self.computer = items.first(where: { $0.name == "computer" })?.value ?? ""
        self.computerId = computerId
        self.pane = pane
    }

    /// The computer the link names, when its id is a fingerprint.
    public var fingerprint: Fingerprint? {
        Fingerprint(computerId)
    }

    public var url: URL {
        var components = URLComponents()
        components.scheme = Self.scheme
        components.host = "chat"
        components.queryItems = [
            URLQueryItem(name: "id", value: computerId),
            URLQueryItem(name: "computer", value: computer),
            URLQueryItem(name: "pane", value: pane),
        ]
        // Every value is set above, so this cannot fail.
        return components.url ?? URL(fileURLWithPath: "/")
    }

    /// The web app's page for this chat at `origin`: `ORIGIN/chat/PANE`, the
    /// pane escaped as one path segment the way the daemon escapes it.
    public func page(origin: String) -> URL? {
        URL(string: "\(origin)/chat/\(Self.pathSegment(pane))")
    }

    /// Only unreserved characters stay as they are, as in the daemon's
    /// `path_segment`.
    static func pathSegment(_ text: String) -> String {
        text.utf8.map { byte in
            let char = Character(UnicodeScalar(byte))
            let unreserved = (byte < 0x80 && (char.isLetter || char.isNumber)) || "-._~".utf8.contains(byte)
            return unreserved ? String(char) : String(format: "%%%02X", byte)
        }.joined()
    }
}
