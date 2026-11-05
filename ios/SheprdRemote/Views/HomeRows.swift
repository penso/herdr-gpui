import SheprdRemoteCore
import SwiftUI

/// A row of an agent list: a chat, or an agent in a terminal tab.
enum HomeRow: Identifiable {
    case chat(ChatItem)
    case agent(TerminalItem)

    /// Where the person's choice of order is kept.
    static let sortKey = "agentSort"

    var id: String {
        switch self {
        case let .chat(item): "chat/\(item.id)"
        case let .agent(item): "agent/\(item.id)"
        }
    }

    /// `chats` and `agents` together, in `sort`'s order.
    static func sections(_ sort: AgentSort, chats: [ChatItem], agents: [TerminalItem]) -> [AgentSort.Section<HomeRow>] {
        sort.sections(of: chats.map(HomeRow.chat) + agents.map(HomeRow.agent), facts: \.facts)
    }

    private var facts: AgentSort.Facts {
        switch self {
        case let .chat(item):
            // An answered question is on its way, so it no longer waits.
            let status: Home.AgentStatus = item.needsYou ? .blocked
                : item.working || item.chat.status == .blocked ? .working : item.chat.status
            // Set only during a turn, which a question does not end.
            let started = item.chat.summary?.turnStartedMs.map { Date(timeIntervalSince1970: Double($0) / 1000) }
            return AgentSort.Facts(
                status: status,
                date: [started, item.lastAnswer].compactMap { $0 }.max(),
                title: item.chat.title,
                group: "\(item.computer.device)/\(item.workspace.id)",
                groupTitle: "\(item.workspace.label) · \(item.computer.name)"
            )
        case let .agent(item):
            return AgentSort.Facts(
                status: item.agent?.status ?? .unknown,
                date: item.changed,
                title: item.title,
                group: "\(item.computer.device)/\(item.workspace.id)",
                groupTitle: "\(item.workspace.label) · \(item.computer.name)"
            )
        }
    }
}

/// Chooses how the agent lists are ordered, as the desktop's agent panel
/// does.
struct AgentSortMenu: View {
    @Binding var sort: AgentSort

    var body: some View {
        Menu {
            Picker("Sort Agents", selection: $sort.animation()) {
                ForEach(AgentSort.allCases) { sort in
                    switch sort {
                    case .priority: Label("Priority", systemImage: "exclamationmark.circle").tag(sort)
                    case .recent: Label("Recent", systemImage: "clock").tag(sort)
                    case .workspace: Label("Workspace", systemImage: "square.stack.3d.up").tag(sort)
                    }
                }
            }
        } label: {
            Image(systemName: "arrow.up.arrow.down")
        }
        .accessibilityLabel("Sort agents")
    }
}
