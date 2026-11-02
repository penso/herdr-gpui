import Foundation

// Transcript entries, tool calls, permission requests, and session
// settings, as `herdr_chat_protocol` writes them.

public struct Entry: Codable, Hashable, Sendable, Identifiable {
    public var id: EntryId
    public var kind: EntryKind

    public init(id: EntryId, kind: EntryKind) {
        self.id = id
        self.kind = kind
    }
}

public enum EntryKind: Codable, Hashable, Sendable {
    /// What the user said; the last `files` lines name attached files.
    case user(text: String, files: Int)
    case agent(text: String)
    case thought(text: String)
    case image(author: Author, image: ChatImage)
    case tool(Tool)
    /// How a prompt turn ended.
    case turn(TurnSummary)
    case notice(Notice)

    enum CodingKeys: String, CodingKey { case type, text, files, author, image }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .type) {
        case "user":
            self = .user(text: try c.decode(String.self, forKey: .text), files: try c.decodeIfPresent(Int.self, forKey: .files) ?? 0)
        case "agent": self = .agent(text: try c.decode(String.self, forKey: .text))
        case "thought": self = .thought(text: try c.decode(String.self, forKey: .text))
        case "image":
            self = .image(author: try c.decode(Author.self, forKey: .author), image: try c.decode(ChatImage.self, forKey: .image))
        case "tool": self = .tool(try Tool(from: decoder))
        case "turn": self = .turn(try TurnSummary(from: decoder))
        case "notice": self = .notice(try Notice(from: decoder))
        case let other:
            throw DecodingError.dataCorruptedError(forKey: .type, in: c, debugDescription: "unknown entry type \(other)")
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .user(text, files):
            try c.encode("user", forKey: .type)
            try c.encode(text, forKey: .text)
            if files != 0 { try c.encode(files, forKey: .files) }
        case let .agent(text):
            try c.encode("agent", forKey: .type)
            try c.encode(text, forKey: .text)
        case let .thought(text):
            try c.encode("thought", forKey: .type)
            try c.encode(text, forKey: .text)
        case let .image(author, image):
            try c.encode("image", forKey: .type)
            try c.encode(author, forKey: .author)
            try c.encode(image, forKey: .image)
        case let .tool(tool):
            try c.encode("tool", forKey: .type)
            try tool.encode(to: encoder)
        case let .turn(summary):
            try c.encode("turn", forKey: .type)
            try summary.encode(to: encoder)
        case let .notice(notice):
            try c.encode("notice", forKey: .type)
            try notice.encode(to: encoder)
        }
    }

    /// The text of a user, agent, thought, or notice entry.
    public var text: String? {
        switch self {
        case let .user(text, _), let .agent(text), let .thought(text): text
        case let .notice(notice): notice.text
        case .image, .tool, .turn: nil
        }
    }

    /// The same entry with `more` after its text; nil when it has none.
    func appending(_ more: String) -> EntryKind? {
        switch self {
        case let .user(text, files): return .user(text: text + more, files: files)
        case let .agent(text): return .agent(text: text + more)
        case let .thought(text): return .thought(text: text + more)
        case var .notice(notice):
            notice.text += more
            return .notice(notice)
        case .image, .tool, .turn: return nil
        }
    }
}

public enum Author: String, Codable, Hashable, Sendable {
    case user, agent
}

/// A picture in the conversation, base64 as the program sent it.
public struct ChatImage: Codable, Hashable, Sendable {
    public var data: String
    public var mimeType: String

    enum CodingKeys: String, CodingKey {
        case data
        case mimeType = "mime_type"
    }
}

public struct Notice: Codable, Hashable, Sendable {
    public var answers: [QuestionAnswer]
    public var details: String?
    public var level: NoticeLevel
    public var text: String

    enum CodingKeys: String, CodingKey { case answers, details, level, text }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        answers = try c.decodeIfPresent([QuestionAnswer].self, forKey: .answers) ?? []
        details = try c.decodeIfPresent(String.self, forKey: .details)
        level = try c.decode(NoticeLevel.self, forKey: .level)
        text = try c.decode(String.self, forKey: .text)
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        if !answers.isEmpty { try c.encode(answers, forKey: .answers) }
        try c.encodeIfPresent(details, forKey: .details)
        try c.encode(level, forKey: .level)
        try c.encode(text, forKey: .text)
    }
}

public struct QuestionAnswer: Codable, Hashable, Sendable {
    public var question: String
    public var answer: String

    /// The bounds the program keeps a notice's answers within.
    public static let maxCount = 32
    public static let maxText = 128 * 1024

    public static func valid(_ answers: [QuestionAnswer]) -> Bool {
        var bytes = 0
        return answers.count <= maxCount && answers.allSatisfy {
            bytes += $0.question.utf8.count + $0.answer.utf8.count
            return bytes <= maxText
        }
    }
}

public enum NoticeLevel: String, Codable, Hashable, Sendable {
    case info, warning, error
}

public struct Tool: Codable, Hashable, Sendable {
    public var callId: String
    public var content: [ToolContent]
    public var kind: ToolKind
    /// Files the call touches.
    public var locations: [String]
    public var status: ToolStatus
    public var title: String

    enum CodingKeys: String, CodingKey {
        case content, kind, locations, status, title
        case callId = "call_id"
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        callId = try c.decode(String.self, forKey: .callId)
        content = try c.decodeIfPresent([ToolContent].self, forKey: .content) ?? []
        kind = try c.decodeIfPresent(ToolKind.self, forKey: .kind) ?? .other
        locations = try c.decodeIfPresent([String].self, forKey: .locations) ?? []
        status = try c.decodeIfPresent(ToolStatus.self, forKey: .status) ?? .pending
        title = try c.decode(String.self, forKey: .title)
    }
}

public enum ToolKind: String, Codable, Hashable, Sendable {
    case read, edit, delete, move, search, execute, think, fetch
    case switchMode = "switch_mode"
    case other

    public init(from decoder: Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .other
    }
}

public enum ToolStatus: String, Codable, Hashable, Sendable {
    case pending
    case inProgress = "in_progress"
    case completed, failed
}

public enum ToolContent: Codable, Hashable, Sendable {
    case text(String)
    /// A file change; `old` is nil for a new file.
    case diff(path: String, old: String?, new: String)
    case image(ChatImage)
    /// A terminal the agent ran through the client, shown by id only.
    case terminal(id: String)

    enum CodingKeys: String, CodingKey { case type, text, path, old, new, image, id }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .type) {
        case "text": self = .text(try c.decode(String.self, forKey: .text))
        case "diff":
            self = .diff(
                path: try c.decode(String.self, forKey: .path),
                old: try c.decodeIfPresent(String.self, forKey: .old),
                new: try c.decode(String.self, forKey: .new)
            )
        case "image": self = .image(try c.decode(ChatImage.self, forKey: .image))
        case "terminal": self = .terminal(id: try c.decode(String.self, forKey: .id))
        case let other:
            throw DecodingError.dataCorruptedError(forKey: .type, in: c, debugDescription: "unknown tool content \(other)")
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .text(text):
            try c.encode("text", forKey: .type)
            try c.encode(text, forKey: .text)
        case let .diff(path, old, new):
            try c.encode("diff", forKey: .type)
            try c.encode(new, forKey: .new)
            try c.encodeIfPresent(old, forKey: .old)
            try c.encode(path, forKey: .path)
        case let .image(image):
            try c.encode("image", forKey: .type)
            try c.encode(image, forKey: .image)
        case let .terminal(id):
            try c.encode("terminal", forKey: .type)
            try c.encode(id, forKey: .id)
        }
    }
}

public struct TurnSummary: Codable, Hashable, Sendable {
    public var durationMs: UInt64
    public var end: TurnEnd
    public var tokens: Tokens?

    enum CodingKeys: String, CodingKey {
        case end, tokens
        case durationMs = "duration_ms"
    }
}

public enum TurnEnd: String, Codable, Hashable, Sendable {
    case done, cancelled, limit, refused, failed, other

    public init(from decoder: Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .other
    }
}

public struct Tokens: Codable, Hashable, Sendable {
    public var cached: UInt64
    public var input: UInt64
    public var output: UInt64
    public var thought: UInt64
    public var total: UInt64

    enum CodingKeys: String, CodingKey { case cached, input, output, thought, total }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        cached = try c.decodeIfPresent(UInt64.self, forKey: .cached) ?? 0
        input = try c.decode(UInt64.self, forKey: .input)
        output = try c.decode(UInt64.self, forKey: .output)
        thought = try c.decodeIfPresent(UInt64.self, forKey: .thought) ?? 0
        total = try c.decode(UInt64.self, forKey: .total)
    }
}

/// A question the agent asked before running a tool call.
public struct Permission: Codable, Hashable, Sendable, Identifiable {
    public var callId: String?
    public var id: PermissionId
    public var options: [PermissionOption]
    public var title: String

    enum CodingKeys: String, CodingKey {
        case id, options, title
        case callId = "call_id"
    }

    /// What auto-accepting picks: allowing once, else always.
    public var allowOption: PermissionOption? {
        options.first { $0.kind == .allowOnce } ?? options.first { $0.kind == .allowAlways }
    }
}

public struct PermissionOption: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var kind: PermissionKind
    public var name: String

    enum CodingKeys: String, CodingKey { case id, kind, name }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        kind = try c.decodeIfPresent(PermissionKind.self, forKey: .kind) ?? .other
        name = try c.decode(String.self, forKey: .name)
    }
}

public enum PermissionKind: String, Codable, Hashable, Sendable {
    case allowOnce = "allow_once"
    case allowAlways = "allow_always"
    case rejectOnce = "reject_once"
    case rejectAlways = "reject_always"
    case other

    public init(from decoder: Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .other
    }

    public var allows: Bool { self == .allowOnce || self == .allowAlways }
}

/// A session setting the agent lets clients change.
public struct ConfigOption: Codable, Hashable, Sendable, Identifiable {
    public var category: OptionCategory
    public var description: String?
    public var id: String
    public var kind: OptionKind
    public var name: String

    enum CodingKeys: String, CodingKey { case category, description, id, kind, name }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        category = try c.decodeIfPresent(OptionCategory.self, forKey: .category) ?? .other
        description = try c.decodeIfPresent(String.self, forKey: .description)
        id = try c.decode(String.self, forKey: .id)
        kind = try c.decode(OptionKind.self, forKey: .kind)
        name = try c.decode(String.self, forKey: .name)
    }
}

public enum OptionCategory: String, Codable, Hashable, Sendable {
    case mode, model
    case modelConfig = "model_config"
    case thoughtLevel = "thought_level"
    case other

    public init(from decoder: Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .other
    }
}

public enum OptionKind: Codable, Hashable, Sendable {
    case select(choices: [Choice], current: String)
    case toggle(current: Bool)

    enum CodingKeys: String, CodingKey { case type, choices, current }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .type) {
        case "toggle": self = .toggle(current: try c.decode(Bool.self, forKey: .current))
        default: self = .select(choices: try c.decode([Choice].self, forKey: .choices), current: try c.decode(String.self, forKey: .current))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .select(choices, current):
            try c.encode("select", forKey: .type)
            try c.encode(choices, forKey: .choices)
            try c.encode(current, forKey: .current)
        case let .toggle(current):
            try c.encode("toggle", forKey: .type)
            try c.encode(current, forKey: .current)
        }
    }
}

public struct Choice: Codable, Hashable, Sendable, Identifiable {
    public var description: String?
    /// The heading of the group the choice is listed under.
    public var group: String?
    public var id: String
    public var name: String
}
