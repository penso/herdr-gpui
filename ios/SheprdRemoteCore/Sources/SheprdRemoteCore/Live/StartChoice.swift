import Foundation

/// A mode an agent offered in a chat this phone saw, kept so the new tab
/// screen can offer it before any chat with that agent is open.
public struct OfferedMode: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var name: String
    public var description: String?
    /// The agent's mode setting, for an agent that has one rather than the
    /// older list of modes.
    public var option: String?

    public init(id: String, name: String, description: String? = nil, option: String? = nil) {
        self.id = id
        self.name = name
        self.description = description
        self.option = option
    }

    /// Whether it runs without asking for permission, by the words agents
    /// use for it: Claude's Bypass Permissions, Codex's Full Access.
    public var bypasses: Bool {
        let words = ["bypass", "full-access", "full access", "yolo"]
        return [id, name].contains { text in words.contains { text.localizedCaseInsensitiveContains($0) } }
    }
}

/// What a request asks a new chat to start with.
public struct ChatStart: Codable, Hashable, Sendable {
    public var mode: String?
    public var option: String?
    public var autoAccept: Bool

    enum CodingKeys: String, CodingKey {
        case mode, option
        case autoAccept = "auto_accept"
    }

    public init(mode: String? = nil, option: String? = nil, autoAccept: Bool = false) {
        self.mode = mode
        self.option = option
        self.autoAccept = autoAccept
    }
}

/// What new chats with one agent start with: the mode and auto-accepting
/// last picked on this phone, and the modes it knows the agent offers.
public struct StartChoice: Codable, Hashable, Sendable {
    public var autoAccept = false
    public var mode: String?
    public var offered: [OfferedMode] = []

    public init(autoAccept: Bool = false, mode: String? = nil, offered: [OfferedMode] = []) {
        self.autoAccept = autoAccept
        self.mode = mode
        self.offered = offered
    }

    /// The agent's modes in `transcript`: its mode setting when it has one,
    /// else its older list.
    public static func offered(in transcript: Transcript) -> [OfferedMode] {
        if let option = transcript.options.first(where: { $0.category == .mode }) {
            guard case let .select(choices, _) = option.kind else { return [] }
            return choices.map { OfferedMode(id: $0.id, name: $0.name, description: $0.description, option: option.id) }
        }
        return transcript.modes?.available.map { OfferedMode(id: $0.id, name: $0.name, description: $0.description) } ?? []
    }

    /// The mode picked, while the agent still offers it.
    public var picked: OfferedMode? { offered.first { $0.id == mode } }

    /// Whether a new chat would run tools without asking.
    public var bypasses: Bool { autoAccept || picked?.bypasses == true }

    /// What a new chat starts with beyond the agent's own default, to show
    /// beside its name, or `nil` when it starts as the agent does.
    public var label: String? {
        let parts = [picked?.name, autoAccept ? "Auto-accept" : nil].compactMap(\.self)
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// The setting `command` picks, if it picks one new chats follow.
    public func setting(of command: ChatCommand) -> StartPicks.Setting? {
        switch command {
        case .setMode: .mode
        case let .setOption(id, _) where offered.contains(where: { $0.option == id }): .mode
        case .setAutoAccept: .autoAccept
        default: nil
        }
    }

    /// Keeps the modes `transcript` lists; a transcript that lists none,
    /// such as one still starting, says nothing about them.
    public mutating func learn(from transcript: Transcript) {
        let listed = Self.offered(in: transcript)
        if !listed.isEmpty { offered = listed }
    }

    /// Follows what the person picked in a chat. Only a mode or
    /// auto-accepting counts, and an option only when it is the mode setting.
    public mutating func follow(_ command: ChatCommand) {
        switch command {
        case let .setMode(id):
            mode = id
        case let .setOption(id, .choice(choice)) where offered.contains(where: { $0.option == id }):
            mode = choice
        case let .setAutoAccept(on):
            autoAccept = on
        default:
            break
        }
    }

    /// What a chat started without a screen may start with: the mode picked,
    /// unless it runs without asking, and never auto-accepting. The person
    /// chose those for chats they were watching, and a misheard sentence
    /// must not run tools nobody saw.
    public var unattendedStart: ChatStart? {
        guard let picked, !picked.bypasses else { return nil }
        return ChatStart(mode: picked.id, option: picked.option, autoAccept: false)
    }

    /// What to ask of a new chat, or `nil` when it starts as the agent does.
    public var start: ChatStart? {
        let picked = picked
        guard picked != nil || autoAccept else { return nil }
        return ChatStart(mode: picked?.id, option: picked?.option, autoAccept: autoAccept)
    }
}

/// Which of an agent's picks is the latest, by when the person made them.
/// Each pick is its own request and they may finish in any order, so only
/// the latest of a setting may be saved.
public struct StartPicks: Sendable {
    public enum Setting: Hashable, Sendable { case mode, autoAccept }

    private struct Key: Hashable {
        var agent: String
        var setting: Setting
    }

    private var latest: [Key: Int] = [:]
    private var count = 0

    public init() {}

    /// Notes a pick being made, and returns its place to check later.
    public mutating func begin(agent: String, setting: Setting) -> Int {
        count += 1
        latest[Key(agent: agent, setting: setting)] = count
        return count
    }

    /// Whether no later pick of `setting` was made since `pick`.
    public func isLatest(_ pick: Int, agent: String, setting: Setting) -> Bool {
        latest[Key(agent: agent, setting: setting)] == pick
    }
}
