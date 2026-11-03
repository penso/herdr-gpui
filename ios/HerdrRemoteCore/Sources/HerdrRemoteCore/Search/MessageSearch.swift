import Foundation

/// `GET /api/v2/search?q=`: what was said in a computer's chats.
public struct SearchAnswer: Decodable, Sendable {
    public var hits: [SearchHit]
    /// More matched than the computer sent.
    public var truncated: Bool
}

/// One user or agent message that matched, with the words around the
/// match on one line.
public struct SearchHit: Decodable, Hashable, Sendable {
    public var pane: String
    public var entry: EntryId
    public var author: Author
    public var snippet: String

    public init(pane: String, entry: EntryId, author: Author, snippet: String) {
        self.pane = pane
        self.entry = entry
        self.author = author
        self.snippet = snippet
    }
}

/// Matching the way the computer does: ignoring case, with any run of
/// whitespace as one space.
public enum MessageText {
    /// The fewest characters a search needs, as the computer requires.
    public static let minQuery = 2
    /// The most characters a search may have.
    public static let maxQuery = 100

    /// `text` on one line, lowercased, for comparing.
    public static func fold(_ text: String) -> String {
        line(text).lowercased()
    }

    /// `text` with every run of whitespace as one space.
    public static func line(_ text: String) -> String {
        text.split(whereSeparator: \.isWhitespace).joined(separator: " ")
    }

    /// The query to send, or nil when it is too short or too long.
    public static func query(_ text: String) -> String? {
        let query = line(text)
        return (minQuery...maxQuery).contains(query.count) ? query : nil
    }

    /// The words around the first match of `query` in `text` on one line:
    /// up to `before` characters ahead of it, `limit` in all, with an
    /// ellipsis where text was left out.
    public static func snippet(_ text: String, around query: String, before: Int = 40, limit: Int = 160) -> String {
        let line = line(text)
        let match = line.range(of: Self.line(query), options: .caseInsensitive)
        var start = line.startIndex
        if let match {
            start = line.index(match.lowerBound, offsetBy: -before, limitedBy: line.startIndex) ?? line.startIndex
        }
        var snippet = start > line.startIndex ? "…" : ""
        let rest = line[start...]
        let room = limit - snippet.count
        if rest.count > room {
            snippet += rest.prefix(room - 1) + "…"
        } else {
            snippet += rest
        }
        return snippet
    }
}
