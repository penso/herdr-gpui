import Foundation

/// A computer's workspaces by repository, as Sheprd's sidebar lists them:
/// a repository's own checkouts, then its linked worktrees.
public struct Repository: Hashable, Sendable, Identifiable {
    /// The worktree key, or the workspace's id when Sheprd names no
    /// repository for it.
    public var id: String
    public var label: String
    public var workspaces: [Home.Workspace]
}

/// A computer's workspaces in one Herdr session, by repository.
public struct SessionGroup: Hashable, Sendable, Identifiable {
    /// The session's name, or nil for the workspaces in none, such as
    /// Herdr 2's.
    public var name: String?
    public var repositories: [Repository]

    public var id: String { name.map { "session/\($0)" } ?? "none" }
}

extension Home {
    /// Whether `other` differs from this home in its load alone.
    public func sameApartFromLoad(_ other: Home) -> Bool {
        var other = other
        other.load = load
        return other == self
    }

    /// The workspaces grouped by repository, each group where its first
    /// workspace is in Sheprd's order. A workspace whose checkout Sheprd does
    /// not know is a group of its own, named by its label.
    public var repositories: [Repository] { Self.repositories(of: workspaces) }

    /// The workspaces by Herdr session, each by repository, when they are in
    /// more than one named session: those in none first, then each session
    /// where its first workspace is. A computer running one session is one
    /// unnamed group, listed as before sessions were named.
    public var sessionGroups: [SessionGroup] {
        var names: [String] = []
        for name in workspaces.compactMap(\.session) where !names.contains(name) {
            names.append(name)
        }
        guard names.count > 1 else { return [SessionGroup(name: nil, repositories: repositories)] }
        return ([nil] + names).compactMap { name in
            let members = workspaces.filter { $0.session == name }
            return members.isEmpty ? nil : SessionGroup(name: name, repositories: Self.repositories(of: members))
        }
    }

    private static func repositories(of workspaces: [Workspace]) -> [Repository] {
        var order: [String] = []
        var groups: [String: (label: String, own: [Workspace], linked: [Workspace])] = [:]
        for workspace in workspaces {
            let id = workspace.worktree?.key ?? "workspace/\(workspace.id)"
            if groups[id] == nil {
                order.append(id)
                groups[id] = (workspace.worktree?.label ?? workspace.label, [], [])
            }
            if workspace.worktree?.linked == true {
                groups[id]?.linked.append(workspace)
            } else {
                groups[id]?.own.append(workspace)
            }
        }
        return order.compactMap { id in
            groups[id].map { Repository(id: id, label: $0.label, workspaces: $0.own + $0.linked) }
        }
    }
}

extension Home.Workspace {
    /// The most pressing status among its chats and the agents in its
    /// terminal tabs, or nil when it runs none.
    public var status: Home.AgentStatus? {
        let statuses = chats.map(\.status) + terminals.compactMap(\.agent?.status)
        return statuses.max { $0.urgency < $1.urgency }
    }

    /// A chat in it waits on a question.
    public var asking: Bool {
        chats.contains { ($0.summary?.questions ?? 0) > 0 }
    }

    /// A pane in it, which actions on the whole workspace go through.
    public var anyPane: String? {
        terminals.lazy.compactMap(\.firstPane?.pane).first ?? chats.first?.pane
    }
}

extension Home.AgentStatus {
    /// A question first, then work in progress, then finished work, as the
    /// daemon ranks a tab's agents.
    var urgency: Int {
        switch self {
        case .unknown: 0
        case .idle: 1
        case .done: 2
        case .working: 3
        case .blocked: 4
        }
    }
}
