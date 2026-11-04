import Foundation

// The desktop's weights (`dispatch.rs`), in cores.

/// Cores an agent at work is expected to take on top of today's load.
private let agentCores = 0.5
/// Staying on the computer already chosen wins a close call.
private let stayCores = 0.25
/// Memory use from which a computer's spare cores count half: a new
/// checkout's build would push it into swap.
private let memoryTight = 90
/// Assumed for a computer too old to send its core count.
private let assumedCores = 10

extension Home.Load {
    /// Cores with nothing to run. The 5 minute load average smooths a
    /// momentary spike, so it is preferred over the latest CPU share.
    public var spareCores: Double {
        let cores = Double(cores ?? assumedCores)
        let busy = load5Tenths.map { Double($0) / 10 } ?? cores * Double(cpu) / 100
        return max(cores - busy, 0)
    }
}

extension Home {
    /// One computer a new workspace could go to.
    public struct Candidate: Sendable {
        public var id: String
        public var label: String
        public var home: Home

        public init(id: String, label: String, home: Home) {
            self.id = id
            self.label = label
            self.home = home
        }
    }

    /// Agents working on this computer now.
    var workingAgents: Int {
        workspaces.reduce(0) { count, workspace in
            let statuses = workspace.chats.map(\.status) + workspace.terminals.compactMap(\.agent?.status)
            return count + statuses.filter { $0 == .working }.count
        }
    }

    /// How much room the computer has for one more workspace, in cores.
    /// An unread load counts as none spare.
    var room: Double {
        var spare = load?.spareCores ?? 0
        if let load, load.memory >= memoryTight { spare /= 2 }
        return spare - agentCores * Double(workingAgents)
    }

    /// Whether a workspace here is a checkout of the repository `label`.
    public func hasRepository(_ label: String) -> Bool {
        workspaces.contains { $0.worktree?.label == label }
    }

    /// Where the repository `label` is checked out, its own checkout first.
    public func checkout(ofRepository label: String) -> String? {
        let checkouts = workspaces.filter { $0.worktree?.label == label }
        return (checkouts.first { $0.worktree?.linked == false } ?? checkouts.first)?.cwd
    }

    /// The computers that can take a workspace, best first: those whose
    /// daemon answers, by room, ties to `current` and then by label.
    /// Computers with `repository` come before any without: the desktop
    /// copies a repository over at a cost, which a phone cannot do.
    public static func leastBusy(_ candidates: [Candidate], current: String? = nil, repository: String? = nil) -> [Candidate] {
        func score(_ candidate: Candidate) -> Double {
            candidate.home.room + (candidate.id == current ? stayCores : 0)
        }
        func has(_ candidate: Candidate) -> Bool {
            repository.map(candidate.home.hasRepository) ?? false
        }
        return candidates.filter(\.home.connected).sorted { a, b in
            if has(a) != has(b) { return has(a) }
            return score(a) != score(b) ? score(a) > score(b) : a.label < b.label
        }
    }
}
