import Foundation

/// What one computer lists: `GET /api/v2/home` and the live socket's
/// `home` message (`crates/herdr-remote-control/src/native/model.rs`).
public struct Home: Codable, Hashable, Sendable {
    public var computer: String
    public var fingerprint: String
    /// Whether the computer's Herdr is reachable.
    public var connected: Bool
    /// Permission requests waiting across its chats: the badge.
    public var waiting: Int
    /// The computer's CPU and memory; absent until it first read them, and
    /// from older computers.
    public var load: Load?
    public var workspaces: [Workspace]

    public init(computer: String, fingerprint: String, connected: Bool, waiting: Int, load: Load? = nil, workspaces: [Workspace]) {
        self.computer = computer
        self.fingerprint = fingerprint
        self.connected = connected
        self.waiting = waiting
        self.load = load
        self.workspaces = workspaces
    }

    /// Whole percentages, 0 to 100.
    public struct Load: Codable, Hashable, Sendable {
        public var cpu: Int
        public var memory: Int
    }

    public struct Workspace: Codable, Hashable, Sendable, Identifiable {
        public var id: String
        public var label: String
        public var branch: String?
        public var cwd: String
        /// The repository checkout it is; older computers leave it out.
        public var worktree: Worktree?
        public var pullRequest: PullRequest?
        public var chats: [ChatRow]
        public var terminals: [TerminalRow]

        enum CodingKeys: String, CodingKey {
            case id, label, branch, cwd, worktree, chats, terminals
            case pullRequest = "pull_request"
        }
    }

    /// The repository a workspace's checkout belongs to, as Herdr reports
    /// it: every checkout of one repository shares `key`.
    public struct Worktree: Codable, Hashable, Sendable {
        public var key: String
        /// The repository's name.
        public var label: String
        /// A linked worktree rather than the repository's own checkout.
        public var linked: Bool

        enum CodingKeys: String, CodingKey {
            case key, label
            case linked = "is_linked_worktree"
        }
    }

    public struct PullRequest: Codable, Hashable, Sendable {
        public var number: UInt64
        public var url: String
        public var tone: Tone

        /// What the badge says, before a theme colors it.
        public enum Tone: String, Codable, Hashable, Sendable {
            case attention, held, merged, ready, waiting, other

            public init(from decoder: Decoder) throws {
                self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .other
            }
        }
    }

    public struct ChatRow: Codable, Hashable, Sendable, Identifiable {
        public var pane: String
        public var tab: String
        public var title: String
        /// Herdr's name for the agent, such as `claude`.
        public var agent: String?
        public var status: AgentStatus
        /// Absent until the daemon has linked the chat.
        public var summary: Summary?

        public var id: String { pane }
    }

    /// A chat as a row shows it, from the daemon's live transcript.
    public struct Summary: Codable, Hashable, Sendable {
        public var connected: Bool
        public var status: Status
        /// The oldest question waiting.
        public var question: Permission?
        public var questions: Int
        /// The start of the agent's latest answer, on one line.
        public var lastText: String?
        public var turnStartedMs: UInt64?
        /// When the latest turn ended; absent from older computers.
        public var lastTurnEndedMs: UInt64?
        public var usagePercent: Int?

        enum CodingKeys: String, CodingKey {
            case connected, status, question, questions
            case lastText = "last_text"
            case turnStartedMs = "turn_started_ms"
            case lastTurnEndedMs = "last_turn_ended_ms"
            case usagePercent = "usage_percent"
        }
    }

    public struct TerminalRow: Codable, Hashable, Sendable, Identifiable {
        public var tab: String
        public var label: String
        public var cwd: String?
        public var zoomed: Bool
        public var panes: [Pane]
        /// The agent Herdr detects in it, the most pressing first.
        public var agent: DetectedAgent?

        public var id: String { tab }

        /// The pane a phone opens first: the focused one, else the first.
        public var firstPane: Pane? { panes.first { $0.focused } ?? panes.first }

        public struct Pane: Codable, Hashable, Sendable, Identifiable {
            public var pane: String
            public var label: String?
            public var cwd: String?
            public var focused: Bool
            public var id: String { pane }
        }

        public struct DetectedAgent: Codable, Hashable, Sendable {
            public var name: String?
            public var status: AgentStatus
            /// The pane it runs in; older computers leave it out.
            public var pane: String?
            /// What it works on, from its terminal title.
            public var title: String?
        }

        /// The pane a phone opens for the tab's agent: its own, else the
        /// first.
        public var agentPane: Pane? {
            agent?.pane.flatMap { pane in panes.first { $0.pane == pane } } ?? firstPane
        }
    }

    /// Herdr's own status for a pane's agent.
    public enum AgentStatus: String, Codable, Hashable, Sendable {
        case unknown, idle, working, blocked, done

        public init(from decoder: Decoder) throws {
            self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown
        }
    }
}
