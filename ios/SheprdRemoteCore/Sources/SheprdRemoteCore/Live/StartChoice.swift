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

    /// Whether it runs without asking for permission.
    public var bypasses: Bool {
        [id, name].contains { $0.localizedCaseInsensitiveContains("bypass") }
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

    /// What to ask of a new chat, or `nil` when it starts as the agent does.
    public var start: ChatStart? {
        let picked = picked
        guard picked != nil || autoAccept else { return nil }
        return ChatStart(mode: picked?.id, option: picked?.option, autoAccept: autoAccept)
    }
}
