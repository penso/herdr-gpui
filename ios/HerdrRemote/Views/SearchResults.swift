import HerdrRemoteCore
import SwiftUI

/// Workspaces, chats, and terminals across every computer, by name, and
/// the messages in chats that contain the query.
struct SearchResults: View {
    @Environment(AppModel.self) private var model
    let query: String
    @State private var messages: [MessageHit] = []
    @State private var searching = false

    var body: some View {
        let needle = query.lowercased()
        let chats = model.fleet.chatItems.filter {
            needle.isEmpty || $0.chat.title.lowercased().contains(needle) || $0.workspace.label.lowercased().contains(needle)
        }
        let terminals = model.fleet.terminalItems.filter {
            needle.isEmpty || $0.terminal.label.lowercased().contains(needle) || $0.workspace.label.lowercased().contains(needle)
        }
        let workspaces = model.fleet.machines.flatMap { machine in
            (machine.home?.workspaces ?? []).filter {
                $0.label.lowercased().contains(needle) || ($0.branch?.lowercased().contains(needle) ?? false)
            }
            .map { (key: WorkspaceKey(device: machine.id, workspace: $0.id), machine: machine, workspace: $0) }
        }
        List {
            if !workspaces.isEmpty {
                Section("Workspaces") {
                    ForEach(workspaces, id: \.key) { key, machine, workspace in
                        NavigationLink(value: key) {
                            HStack(spacing: 10) {
                                WorkspaceMark(workspace: workspace)
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(workspace.label).lineLimit(1)
                                    Text([model.displayName(machine.computer), workspace.branch].compactMap { $0 }.joined(separator: " · "))
                                        .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                }
                                Spacer(minLength: 4)
                                if let pullRequest = workspace.pullRequest { PullRequestPill(pullRequest: pullRequest) }
                            }
                        }
                    }
                }
            }
            if !chats.isEmpty {
                Section("Chats") {
                    ForEach(chats) { item in
                        NavigationLink(value: Route.chat(item.key, title: item.chat.title)) { ChatRowLabel(item: item) }
                    }
                }
            }
            if !messages.isEmpty {
                Section("Messages") {
                    ForEach(messages) { hit in
                        NavigationLink(value: Route.chat(hit.item.key, title: hit.item.chat.title)) {
                            MessageHitRow(hit: hit, query: query)
                        }
                    }
                }
            }
            if !terminals.isEmpty {
                Section("Terminals") {
                    ForEach(terminals) { item in
                        if let pane = item.terminal.firstPane {
                            NavigationLink(value: Route.term(PaneKey(device: item.computer.device, pane: pane.pane), title: item.terminal.label)) {
                                Label("\(item.terminal.label) · \(item.computer.name) · \(item.workspace.label)", systemImage: "apple.terminal")
                            }
                        }
                    }
                }
            }
        }
        .overlay {
            if workspaces.isEmpty, chats.isEmpty, terminals.isEmpty, messages.isEmpty, !searching { ContentUnavailableView.search(text: query) }
        }
        .task(id: query) {
            searching = true
            let found = await model.fleet.searchMessages(query)
            guard !Task.isCancelled else { return }
            messages = found
            searching = false
        }
    }
}
