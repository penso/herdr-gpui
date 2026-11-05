import Foundation

/// What a new workspace or chat may start with (`GET /api/v2/new`): the
/// agents Sheprd can run, and the only folders a workspace may open in.
public struct NewOptions: Codable, Hashable, Sendable {
    public var agents: [AgentChoice]
    /// Folders workspaces already use, most useful first.
    public var recent: [String]
    /// Project roots, each with the folders in it.
    public var roots: [Root]

    public struct Root: Codable, Hashable, Sendable, Identifiable {
        public var path: String
        public var folders: [String]
        public var id: String { path }
    }

    /// Keeps the chosen agent when offered, otherwise selects the first
    /// available one. An empty list has no valid selection.
    public func agent(preferring id: String) -> AgentChoice? {
        agents.first { $0.id == id } ?? agents.first
    }

    /// Every folder offered, recent ones first, once each.
    public var folders: [String] {
        var seen = Set<String>()
        return (recent + roots.flatMap(\.folders)).filter { seen.insert($0).inserted }
    }
}

public struct AgentChoice: Codable, Hashable, Sendable, Identifiable {
    /// What requests name it by, such as `claude`.
    public var id: String
    public var name: String
}

/// `POST /api/v2/workspaces`: a workspace in `cwd`, on a new worktree
/// when `branch` is not empty, with a chat running `agent`.
public struct NewWorkspaceRequest: Codable, Hashable, Sendable {
    public var cwd: String
    public var branch: String
    public var agent: String
    /// Sent once the chat is ready; empty sends nothing.
    public var prompt: String

    public init(cwd: String, branch: String = "", agent: String, prompt: String = "") {
        self.cwd = cwd
        self.branch = branch
        self.agent = agent
        self.prompt = prompt
    }

    /// The branch proposed for `seed`, the same one the desktop's new
    /// worktree dialog proposes (`generated_branch_slug` in
    /// `sheprd-gpui`'s `worktree.rs`), so the form can be sent as it opens.
    public static func proposedBranch(seed: UInt64) -> String {
        let adjectives = ["brave", "calm", "clear", "green", "lucky", "quiet", "rapid", "silver"]
        let nouns = ["river", "cloud", "field", "forest", "harbor", "meadow", "stone", "valley"]
        let adjective = adjectives[Int(seed % UInt64(adjectives.count))]
        let noun = nouns[Int(seed / UInt64(adjectives.count) % UInt64(nouns.count))]
        let suffix = String(seed & 0xffff, radix: 16)
        return "worktree/\(adjective)-\(noun)-\(String(repeating: "0", count: 4 - suffix.count))\(suffix)"
    }

    /// A proposal seeded from the clock in microseconds, as the desktop's is.
    public static func proposedBranch(at date: Date = Date()) -> String {
        proposedBranch(seed: UInt64(max(date.timeIntervalSince1970, 0) * 1_000_000))
    }
}

/// `POST /api/v2/workspaces/{workspace}/chats`.
public struct NewChatRequest: Codable, Hashable, Sendable {
    public var agent: String
    public var prompt: String

    public init(agent: String, prompt: String = "") {
        self.agent = agent
        self.prompt = prompt
    }
}

/// What creating answers: the chat's pane once its program reported in,
/// or `nil` when it was slow to start (it then appears on home later).
public struct Created: Codable, Hashable, Sendable {
    public var workspace: String?
    public var pane: String?
    /// Whether the first prompt went to the chat.
    public var prompted: Bool

    enum CodingKeys: String, CodingKey { case workspace, pane, prompted }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        workspace = try c.decodeIfPresent(String.self, forKey: .workspace)
        pane = try c.decodeIfPresent(String.self, forKey: .pane)
        prompted = try c.decodeIfPresent(Bool.self, forKey: .prompted) ?? false
    }
}
