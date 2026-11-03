import Foundation

// The conversation a chat program projects from ACP, as
// `herdr_chat_protocol::Transcript` writes it in JSON. Every field a later
// version adds has a default, so decoding never needs all of them, and
// enums with a catch-all variant on the Rust side fall back the same way
// here.

public struct EntryId: Codable, Hashable, Comparable, Sendable, ExpressibleByIntegerLiteral {
    public var value: UInt64
    public init(_ value: UInt64) { self.value = value }
    public init(integerLiteral value: UInt64) { self.value = value }
    public init(from decoder: Decoder) throws { value = try decoder.singleValueContainer().decode(UInt64.self) }
    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(value)
    }
    public static func < (lhs: EntryId, rhs: EntryId) -> Bool { lhs.value < rhs.value }
}

public struct PermissionId: Codable, Hashable, Sendable, ExpressibleByIntegerLiteral {
    public var value: UInt64
    public init(_ value: UInt64) { self.value = value }
    public init(integerLiteral value: UInt64) { self.value = value }
    public init(from decoder: Decoder) throws { value = try decoder.singleValueContainer().decode(UInt64.self) }
    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(value)
    }
}

/// A position in one run's change stream.
public struct Cursor: Codable, Hashable, Sendable {
    /// Changes whenever the program starts or reloads its session.
    public var epoch: UUID
    /// Changes applied since the epoch began.
    public var seq: UInt64

    public init(epoch: UUID, seq: UInt64) {
        self.epoch = epoch
        self.seq = seq
    }

    /// Whether `next` is the change right after this one.
    public func isFollowed(by next: Cursor) -> Bool {
        next.epoch == epoch && next.seq == seq + 1
    }
}

public struct Transcript: Codable, Hashable, Sendable {
    public var agent: String
    public var autoAccept = false
    public var clients: UInt32 = 0
    public var commands: [SlashCommand] = []
    public var entries: [Entry] = []
    public var modes: Modes?
    public var options: [ConfigOption] = []
    /// Waiting for an answer, oldest first.
    public var permissions: [Permission] = []
    public var plan: [PlanEntry] = []
    public var queued: [Queued] = []
    public var readOnly = false
    public var session: SessionInfo?
    public var status: Status = .starting
    public var truncated = false
    public var turn: Turn?
    public var usage: Usage?

    public init(agent: String) { self.agent = agent }

    enum CodingKeys: String, CodingKey {
        case agent, clients, commands, entries, modes, options, permissions, plan, queued, session, status
        case truncated, turn, usage
        case autoAccept = "auto_accept"
        case readOnly = "read_only"
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        agent = try c.decode(String.self, forKey: .agent)
        autoAccept = try c.decodeIfPresent(Bool.self, forKey: .autoAccept) ?? false
        clients = try c.decodeIfPresent(UInt32.self, forKey: .clients) ?? 0
        commands = try c.decodeIfPresent([SlashCommand].self, forKey: .commands) ?? []
        entries = try c.decodeIfPresent([Entry].self, forKey: .entries) ?? []
        modes = try c.decodeIfPresent(Modes.self, forKey: .modes)
        options = try c.decodeIfPresent([ConfigOption].self, forKey: .options) ?? []
        permissions = try c.decodeIfPresent([Permission].self, forKey: .permissions) ?? []
        plan = try c.decodeIfPresent([PlanEntry].self, forKey: .plan) ?? []
        queued = try c.decodeIfPresent([Queued].self, forKey: .queued) ?? []
        readOnly = try c.decodeIfPresent(Bool.self, forKey: .readOnly) ?? false
        session = try c.decodeIfPresent(SessionInfo.self, forKey: .session)
        status = try c.decodeIfPresent(Status.self, forKey: .status) ?? .starting
        truncated = try c.decodeIfPresent(Bool.self, forKey: .truncated) ?? false
        turn = try c.decodeIfPresent(Turn.self, forKey: .turn)
        usage = try c.decodeIfPresent(Usage.self, forKey: .usage)
    }

    public func entry(_ id: EntryId) -> Entry? { position(of: id).map { entries[$0] } }

    /// The index of `id`; entries are kept in increasing id order.
    func position(of id: EntryId) -> Int? {
        var low = 0
        var high = entries.count
        while low < high {
            let mid = (low + high) / 2
            if entries[mid].id < id { low = mid + 1 } else { high = mid }
        }
        return low < entries.count && entries[low].id == id ? low : nil
    }
}

/// Where the conversation is between turns.
public enum Status: Codable, Hashable, Sendable {
    case starting
    case idle
    case working
    case failed(message: String)
    case exited(message: String)

    enum CodingKeys: String, CodingKey { case state, message }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .state) {
        case "idle": self = .idle
        case "working": self = .working
        case "failed": self = .failed(message: try c.decode(String.self, forKey: .message))
        case "exited": self = .exited(message: try c.decode(String.self, forKey: .message))
        default: self = .starting
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .starting: try c.encode("starting", forKey: .state)
        case .idle: try c.encode("idle", forKey: .state)
        case .working: try c.encode("working", forKey: .state)
        case let .failed(message):
            try c.encode("failed", forKey: .state)
            try c.encode(message, forKey: .message)
        case let .exited(message):
            try c.encode("exited", forKey: .state)
            try c.encode(message, forKey: .message)
        }
    }
}

public struct Turn: Codable, Hashable, Sendable {
    public var startedMs: UInt64
    public init(startedMs: UInt64) { self.startedMs = startedMs }
    enum CodingKeys: String, CodingKey { case startedMs = "started_ms" }
}

public struct Usage: Codable, Hashable, Sendable {
    public var cost: Cost?
    public var size: UInt64
    public var used: UInt64

    /// How full the context window is, from 0 to 100.
    public var percent: Int {
        guard size > 0 else { return 0 }
        let (scaled, overflow) = used.multipliedReportingOverflow(by: 100)
        return overflow ? 100 : Int(min(scaled / size, 100))
    }
}

/// Millionths of `currency`, so amounts compare exactly.
public struct Cost: Codable, Hashable, Sendable {
    public var currency: String
    public var micros: UInt64
}

public struct SessionInfo: Codable, Hashable, Sendable {
    public var cwd: String?
    public var id: String
    public var title: String?
}

public struct SlashCommand: Codable, Hashable, Sendable {
    public var description: String
    public var hint: String?
    public var name: String

    enum CodingKeys: String, CodingKey { case description, hint, name }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        hint = try c.decodeIfPresent(String.self, forKey: .hint)
        name = try c.decode(String.self, forKey: .name)
    }
}

public struct Queued: Codable, Hashable, Sendable {
    public var attachments: [Attachment]
    public var id: UUID
    public var text: String

    enum CodingKeys: String, CodingKey { case attachments, id, text }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        attachments = try c.decodeIfPresent([Attachment].self, forKey: .attachments) ?? []
        id = try c.decode(UUID.self, forKey: .id)
        text = try c.decode(String.self, forKey: .text)
    }
}

public struct Attachment: Codable, Hashable, Sendable {
    public var path: String
}

public struct PlanEntry: Codable, Hashable, Sendable {
    public var content: String
    public var priority: PlanPriority
    public var status: PlanStatus

    enum CodingKeys: String, CodingKey { case content, priority, status }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        content = try c.decode(String.self, forKey: .content)
        priority = try c.decodeIfPresent(PlanPriority.self, forKey: .priority) ?? .medium
        status = try c.decodeIfPresent(PlanStatus.self, forKey: .status) ?? .pending
    }
}

public enum PlanPriority: String, Codable, Hashable, Sendable {
    case high, low, medium
    public init(from decoder: Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .medium
    }
}

public enum PlanStatus: String, Codable, Hashable, Sendable {
    case inProgress = "in_progress", completed, pending
    public init(from decoder: Decoder) throws {
        self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .pending
    }
}

public struct Modes: Codable, Hashable, Sendable {
    public var available: [Mode]
    public var current: String
}

public struct Mode: Codable, Hashable, Sendable {
    public var description: String?
    public var id: String
    public var name: String
}
