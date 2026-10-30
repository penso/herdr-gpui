import HerdrRemoteCore
import SwiftUI

/// One workspace, opened from the Workspaces tab: its branch and pull
/// request, its tabs, and a row of agents and a terminal to start another
/// tab with one tap.
struct WorkspaceScreen: View {
    @Environment(AppModel.self) private var model
    let key: WorkspaceKey
    @State private var agents: [AgentChoice] = []
    /// What a tap is starting: an agent's id, or `terminal`.
    @State private var starting: String?
    @State private var problem: String?
    /// A tab just started, shown as soon as it exists.
    @State private var opened: Route?

    private static let terminal = "terminal"

    var body: some View {
        let machine = model.fleet.machines.first { $0.id == key.device }
        if let machine, let workspace = machine.home?.workspaces.first(where: { $0.id == key.workspace }) {
            content(machine, workspace)
        } else {
            ContentUnavailableView("This workspace is gone", systemImage: "folder.badge.questionmark",
                                   description: Text("It was closed in Herdr, or its computer is not connected."))
        }
    }

    private func content(_ machine: Fleet.Machine, _ workspace: Home.Workspace) -> some View {
        let chats = model.fleet.chatItems.filter { $0.computer.device == key.device && $0.workspace.id == key.workspace }
        let terminals = model.fleet.terminalItems.filter { $0.computer.device == key.device && $0.workspace.id == key.workspace }
        let repository = machine.home?.repositories.first { $0.workspaces.contains { $0.id == key.workspace } }
        return List {
            Section {
                Summary(workspace: workspace, place: [model.displayName(machine.computer), repository?.label].compactMap { $0 }.joined(separator: " · "))
                if let pullRequest = workspace.pullRequest, let url = URL(string: pullRequest.url) {
                    // Plain text and a chevron, like the rows below, not a
                    // tinted web link.
                    Link(destination: url) { PullRequestRow(pullRequest: pullRequest) }.tint(.primary)
                }
            }
            Section {
                ForEach(chats) { item in
                    NavigationLink(value: Route.chat(item.key, title: item.chat.title)) { ChatRowLabel(item: item, showsComputer: false, showsWorkspace: false) }
                }
                ForEach(terminals.filter { $0.agent != nil }) { item in
                    if let route = item.route {
                        NavigationLink(value: route) { AgentTerminalRowLabel(item: item, showsComputer: false, showsWorkspace: false) }
                    }
                }
                ForEach(terminals.filter { $0.agent == nil }) { item in
                    if let pane = item.terminal.firstPane {
                        NavigationLink(value: Route.term(PaneKey(device: key.device, pane: pane.pane), title: item.terminal.label)) {
                            TerminalRowLabel(terminal: item.terminal)
                        }
                    }
                }
            } header: {
                let count = chats.count + terminals.count
                Text(count == 1 ? "1 tab" : "\(count) tabs")
            }
        }
        .navigationTitle(workspace.label)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.hidden, for: .tabBar)
        .safeAreaInset(edge: .bottom) { launcher(workspace, online: isOnline(machine)) }
        .navigationDestination(item: $opened) { $0.screen }
        // Keyed on the whole status, so a link that comes back at another
        // address loads again: a phone waking from sleep still shows the
        // address it had, and the first try there fails.
        .task(id: machine.status) {
            guard isOnline(machine) else { return }
            while agents.isEmpty, !Task.isCancelled {
                if let loaded = try? await model.fleet.call(key.device, { client, token in try await client.newOptions(bearer: token) }).agents {
                    agents = loaded
                    return
                }
                try? await Task.sleep(for: .seconds(3))
            }
        }
        .alert("The tab did not start", isPresented: Binding(get: { problem != nil }, set: { if !$0 { problem = nil } })) {
        } message: {
            Text(problem ?? "")
        }
    }

    /// One tap starts a terminal, or an agent with no prompt; a long press
    /// on an agent opens the form with a first prompt.
    private func launcher(_ workspace: Home.Workspace, online: Bool) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("New tab").font(.caption.weight(.semibold)).foregroundStyle(.secondary).padding(.leading, 18)
            // More agents than fit scroll sideways inside the card, inset
            // from its rounded edges, never past them.
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    tile("Terminal", id: Self.terminal) {
                        Image(systemName: "apple.terminal").font(.system(size: 14, weight: .semibold))
                            .frame(width: 30, height: 30).background(.fill.secondary, in: .rect(cornerRadius: 30 * 0.28))
                    } action: {
                        openTerminal(in: workspace)
                    }
                    .disabled(workspace.anyPane == nil)
                    ForEach(agents) { agent in
                        tile(agent.name, id: agent.id) { AgentIcon(agent: agent.id, size: 30) } action: {
                            start(agent, in: workspace)
                        }
                        .contextMenu {
                            Button("With a First Prompt…", systemImage: "text.bubble") {
                                model.creating = .chat(device: key.device, workspace: workspace.id, label: workspace.label, agent: agent.id)
                            }
                        }
                    }
                }
            }
            .contentMargins(.horizontal, 12, for: .scrollContent)
        }
        .disabled(!online || starting != nil)
        .padding(.vertical, 12)
        .clipShape(.rect(cornerRadius: 26))
        .glassEffect(.regular, in: .rect(cornerRadius: 26))
        .padding(.horizontal, 16)
    }

    private func tile(_ name: String, id: String, @ViewBuilder icon: () -> some View, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(spacing: 4) {
                if starting == id { ProgressView().frame(width: 30, height: 30) } else { icon() }
                Text(name).font(.caption2.weight(.medium)).lineLimit(1)
            }
            .frame(width: 58)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("New \(name) tab")
    }

    private func isOnline(_ machine: Fleet.Machine) -> Bool {
        if case .online = machine.status { true } else { false }
    }

    private func start(_ agent: AgentChoice, in workspace: Home.Workspace) {
        let (device, id) = (key.device, workspace.id)
        starting = agent.id
        Task {
            defer { starting = nil }
            do {
                let created = try await model.fleet.call(device) { client, token in
                    try await client.startChat(NewChatRequest(agent: agent.id), workspace: id, bearer: token)
                }
                if let pane = created.pane { opened = .chat(PaneKey(device: device, pane: pane), title: agent.name) }
            } catch {
                problem = error.localizedDescription
            }
        }
    }

    /// Herdr opens a tab in the workspace of the pane an action names, so
    /// any of its panes will do.
    private func openTerminal(in workspace: Home.Workspace) {
        guard let pane = workspace.anyPane else { return }
        let device = key.device
        starting = Self.terminal
        Task {
            defer { starting = nil }
            do {
                let created = try await model.fleet.call(device) { client, token in
                    try await client.termAction(.newTab, pane: pane, bearer: token)
                }
                if let created { opened = .term(PaneKey(device: device, pane: created), title: "Terminal") }
            } catch {
                problem = error.localizedDescription
            }
        }
    }
}

/// The workspace's branch, and where it is.
private struct Summary: View {
    var workspace: Home.Workspace
    var place: String

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "arrow.triangle.branch").font(.system(size: 14, weight: .semibold))
                .foregroundStyle(.tint)
                .frame(width: 28, height: 28).background(.tint.opacity(0.15), in: .rect(cornerRadius: 28 * 0.28))
            VStack(alignment: .leading, spacing: 2) {
                Text(workspace.branch ?? workspace.label)
                    .font(.body.monospaced()).lineLimit(1).truncationMode(.middle)
                Text(place).font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
            }
        }
    }
}

/// The workspace's pull request: its number, its state as an icon, and
/// a link out to it.
private struct PullRequestRow: View {
    var pullRequest: Home.PullRequest

    var body: some View {
        let tone = pullRequest.tone
        HStack(spacing: 12) {
            Image(systemName: "arrow.triangle.pull").font(.system(size: 14, weight: .semibold))
                .foregroundStyle(tone.color)
                .frame(width: 28, height: 28).background(tone.color.opacity(0.15), in: .rect(cornerRadius: 28 * 0.28))
            Text(verbatim: "Pull request #\(pullRequest.number)").foregroundStyle(.primary)
                .frame(maxWidth: .infinity, alignment: .leading)
            Image(systemName: tone.symbol).foregroundStyle(tone.color).imageScale(.large)
            Image(systemName: "chevron.forward").font(.footnote.weight(.semibold)).foregroundStyle(.tertiary)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Pull request \(pullRequest.number), \(tone.label)")
        .accessibilityAddTraits(.isLink)
    }
}

/// A terminal tab without an agent: its name and folder.
private struct TerminalRowLabel: View {
    var terminal: Home.TerminalRow

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "apple.terminal").font(.system(size: 14, weight: .semibold))
                .frame(width: 28, height: 28).background(.fill.secondary, in: .rect(cornerRadius: 28 * 0.28))
            VStack(alignment: .leading, spacing: 2) {
                Text(terminal.label).lineLimit(1)
                if let cwd = terminal.firstPane?.cwd ?? terminal.cwd {
                    Text(cwd).font(.subheadline).foregroundStyle(.secondary)
                        .lineLimit(1).truncationMode(.head)
                }
            }
        }
    }
}
