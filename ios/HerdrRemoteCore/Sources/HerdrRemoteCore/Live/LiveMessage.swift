import Foundation

/// What the phone may watch on the live socket.
public enum LiveView: Hashable, Sendable {
    case home
    case chat(pane: String)
    case term(pane: String)
}

/// The columns and rows a terminal fits on this screen. A terminal
/// subscribed with one is resized to it while the phone watches.
public struct TermSize: Codable, Hashable, Sendable {
    public var cols: Int
    public var rows: Int

    public init(cols: Int, rows: Int) {
        self.cols = cols
        self.rows = rows
    }
}

/// A message the phone sends on `/api/v2/live`.
public enum LiveRequest: Encodable, Hashable, Sendable {
    /// `since` is the cursor of a cached transcript, so an unchanged chat
    /// answers `chat_resumed` instead of a snapshot; `size` asks for a
    /// terminal at the phone's size.
    case subscribe(LiveView, since: Cursor? = nil, size: TermSize? = nil)
    case unsubscribe(LiveView)
    /// Scrolls a terminal held at the phone's size, as a mouse wheel
    /// would: `lines` above zero go back into its history.
    case termScroll(pane: String, lines: Int)

    enum CodingKeys: String, CodingKey { case type, view, pane, since, size, lines }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        let view: LiveView
        switch self {
        case let .subscribe(subscribed, since, size):
            try c.encode("subscribe", forKey: .type)
            view = subscribed
            try c.encodeIfPresent(since, forKey: .since)
            try c.encodeIfPresent(size, forKey: .size)
        case let .unsubscribe(unsubscribed):
            try c.encode("unsubscribe", forKey: .type)
            view = unsubscribed
        case let .termScroll(pane, lines):
            try c.encode("term_scroll", forKey: .type)
            try c.encode(pane, forKey: .pane)
            try c.encode(lines, forKey: .lines)
            return
        }
        switch view {
        case .home: try c.encode("home", forKey: .view)
        case let .chat(pane):
            try c.encode("chat", forKey: .view)
            try c.encode(pane, forKey: .pane)
        case let .term(pane):
            try c.encode("term", forKey: .view)
            try c.encode(pane, forKey: .pane)
        }
    }
}

/// One change with the cursor it leads to.
public struct CursorChange: Codable, Hashable, Sendable {
    public var cursor: Cursor
    public var change: Change
}

/// A chat's whole transcript: the `chat_snapshot` message and
/// `GET /api/v2/chat/{pane}`. `cursor` is nil until the daemon has heard
/// from the chat program once.
public struct ChatSnapshot: Codable, Hashable, Sendable {
    public var pane: String
    public var cursor: Cursor?
    public var connected: Bool
    public var transcript: Transcript
}

/// A message the daemon sends on `/api/v2/live`.
public enum LiveEvent: Decodable, Hashable, Sendable {
    case home(Home)
    case chatSnapshot(ChatSnapshot)
    /// The cached transcript at `cursor` is current.
    case chatResumed(pane: String, cursor: Cursor, connected: Bool)
    case chatChanges(pane: String, changes: [CursorChange])
    case chatConnection(pane: String, connected: Bool)
    case termScreen(TermScreen)
    /// The phone lagged or the view went away: fetch it again.
    case reload(LiveView)
    case error(String)
    /// The session expired (`expired`) or the device was revoked (`revoked`).
    case bye(reason: String)

    enum CodingKeys: String, CodingKey {
        case type, home, pane, cursor, connected, changes, view, message, reason
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .type) {
        case "home": self = .home(try c.decode(Home.self, forKey: .home))
        case "chat_snapshot": self = .chatSnapshot(try ChatSnapshot(from: decoder))
        case "chat_resumed":
            self = .chatResumed(
                pane: try c.decode(String.self, forKey: .pane),
                cursor: try c.decode(Cursor.self, forKey: .cursor),
                connected: try c.decode(Bool.self, forKey: .connected)
            )
        case "chat_changes":
            self = .chatChanges(pane: try c.decode(String.self, forKey: .pane), changes: try c.decode([CursorChange].self, forKey: .changes))
        case "chat_connection":
            self = .chatConnection(pane: try c.decode(String.self, forKey: .pane), connected: try c.decode(Bool.self, forKey: .connected))
        case "term_screen": self = .termScreen(try TermScreen(from: decoder))
        case "reload":
            let pane = try c.decodeIfPresent(String.self, forKey: .pane)
            switch (try c.decode(String.self, forKey: .view), pane) {
            case let ("chat", pane?): self = .reload(.chat(pane: pane))
            case let ("term", pane?): self = .reload(.term(pane: pane))
            default: self = .reload(.home)
            }
        case "error": self = .error(try c.decode(String.self, forKey: .message))
        case "bye": self = .bye(reason: try c.decode(String.self, forKey: .reason))
        case let other:
            throw DecodingError.dataCorruptedError(forKey: .type, in: c, debugDescription: "unknown live message \(other)")
        }
    }
}

/// A terminal pane's visible screen as styled runs, sent when it changed.
public struct TermScreen: Codable, Hashable, Sendable {
    public var pane: String
    public var columns: Int
    public var rows: [[Run]]

    public struct Run: Codable, Hashable, Sendable {
        public var text: String
        /// One of the 16 ANSI names (`red`, `bright_blue`) or `#rrggbb`.
        public var fg: String?
        public var bg: String?
        public var bold: Bool
        public var dim: Bool
        public var italic: Bool
        public var underline: Bool
        public var strike: Bool
        public var inverse: Bool
        public var hidden: Bool

        enum CodingKeys: String, CodingKey { case text, fg, bg, bold, dim, italic, underline, strike, inverse, hidden }

        public init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            text = try c.decode(String.self, forKey: .text)
            fg = try c.decodeIfPresent(String.self, forKey: .fg)
            bg = try c.decodeIfPresent(String.self, forKey: .bg)
            let flag = { (key: CodingKeys) in try c.decodeIfPresent(Bool.self, forKey: key) ?? false }
            bold = try flag(.bold)
            dim = try flag(.dim)
            italic = try flag(.italic)
            underline = try flag(.underline)
            strike = try flag(.strike)
            inverse = try flag(.inverse)
            hidden = try flag(.hidden)
        }
    }
}
