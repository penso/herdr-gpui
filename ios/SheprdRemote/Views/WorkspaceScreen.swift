import SheprdRemoteCore
import SwiftUI

/// One workspace, opened from the Workspaces tab: its branch and pull
/// request, its tabs, and buttons to start another tab: a terminal and the
/// agent last started, with the other agents folded below them.
struct WorkspaceScreen: View {
    @Environment(AppModel.self) private var model
    let key: WorkspaceKey
    @State private var agents: [AgentChoice] = []
    /// What a tap is starting: an agent's id, or `terminal`.
    @State private var starting: String?
    @State private var problem: String?
    /// The agents beyond the two large buttons are unfolded.
    @State private var showsOthers = false
    /// A tab just started, shown as soon as it exists.
    @State private var opened: Route?
    /// Files changed and not committed; `nil` until Sheprd says.
    @State private var changedFiles: Int?
    @State private var renaming = false
    @State private var newName = ""
    /// Why the last rename failed, shown apart from a tab that did not start.
    @State private var renameProblem: String?

    private static let terminal = "terminal"

    var body: some View {
        let machine = model.fleet.machines.first { $0.id == key.device }
        if let machine, let workspace = machine.home?.workspaces.first(where: { $0.id == key.workspace }) {
            content(machine, workspace)
        } else {
            ContentUnavailableView("This workspace is gone", systemImage: "folder.badge.questionmark",
                                   description: Text("It was closed in Sheprd, or its computer is not connected."))
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
                if workspace.branch != nil {
                    NavigationLink(value: Route.changes(key)) { ChangesRow(count: changedFiles) }
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
            launcher(workspace, online: isOnline(machine))
        }
        .navigationTitle(workspace.label)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.hidden, for: .tabBar)
        .toolbar {
            ToolbarItem(placement: .principal) {
                Button {
                    newName = workspace.label
                    renaming = true
                } label: {
                    HStack(spacing: 4) {
                        Text(workspace.label).font(.headline).lineLimit(1)
                        Image(systemName: "pencil.circle.fill").font(.caption).foregroundStyle(.tertiary)
                    }
                }
                .buttonStyle(.plain)
                .accessibilityHint("Renames the workspace")
            }
        }
        .onAppear {
            model.recentDevice = key.device
            #if DEBUG
            // The simulator loop's `-otherAgents YES` unfolds the agents,
            // and `-renameWorkspace YES` opens the rename alert.
            if UserDefaults.standard.bool(forKey: "otherAgents") { showsOthers = true }
            // `-openChanges YES` opens the workspace's changes.
            if UserDefaults.standard.bool(forKey: "openChanges"), opened == nil { opened = .changes(key) }
            if UserDefaults.standard.bool(forKey: "renameWorkspace") {
                newName = workspace.label
                renaming = true
            }
            #endif
        }
        // This screen is right behind what it opens, so Back is the way here.
        .navigationDestination(item: $opened) { $0.screen.environment(\.offersWorkspace, false) }
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
        // Again on each return, as an agent may have changed more since.
        .task(id: machine.status) {
            guard isOnline(machine), workspace.branch != nil else { return }
            let (device, id) = (key.device, key.workspace)
            changedFiles = try? await model.fleet.call(device) { client, token in
                try await client.changes(workspace: id, scope: .uncommitted, bearer: token)
            }.total
        }
        .alert("The tab did not start", isPresented: Binding(get: { problem != nil }, set: { if !$0 { problem = nil } })) {
        } message: {
            Text(problem ?? "")
        }
        .alert("Rename Workspace", isPresented: $renaming) {
            TextField("Name", text: $newName)
                .textInputAutocapitalization(.never)
            Button("Cancel", role: .cancel) {}
            Button("Rename") { rename(to: newName) }
                .disabled(newName.trimmingCharacters(in: .whitespaces).isEmpty)
        } message: {
            Text("The name shows in Sheprd and in your lists.")
        }
        .alert("The workspace was not renamed", isPresented: Binding(get: { renameProblem != nil }, set: { if !$0 { renameProblem = nil } })) {
        } message: {
            Text(renameProblem ?? "")
        }
    }

    private func rename(to label: String) {
        Task {
            do {
                try await model.fleet.renameWorkspace(key, to: label)
            } catch {
                renameProblem = error.localizedDescription
            }
        }
    }

    /// One tap starts a terminal or the agent last started, as the two
    /// most likely tabs, with the other agents folded below them. A long
    /// press on an agent opens the form with a first prompt.
    private func launcher(_ workspace: Home.Workspace, online: Bool) -> some View {
        let recent = agents.first { $0.id == model.recentAgent } ?? agents.first
        let others = agents.filter { $0.id != recent?.id }
        return Section("New tab") {
            HStack(spacing: 10) {
                bigButton("Terminal", id: Self.terminal) {
                    Image(systemName: "apple.terminal").font(.system(size: 19, weight: .semibold))
                        .frame(width: 40, height: 40).background(.fill.secondary, in: .rect(cornerRadius: 40 * 0.28))
                } action: {
                    openTerminal(in: workspace)
                }
                .disabled(workspace.anyPane == nil)
                if let recent {
                    bigButton(recent.name, id: recent.id) { AgentIcon(agent: recent.id, size: 40) } action: {
                        start(recent, in: workspace)
                    }
                    .contextMenu { firstPrompt(recent, in: workspace) }
                }
            }
            .listRowInsets(EdgeInsets(top: 12, leading: 12, bottom: 12, trailing: 12))
            .listRowSeparator(.hidden, edges: .bottom)
            if let recent { StartChoiceRow(agent: recent.name) }
            if !others.isEmpty {
                Button {
                    withAnimation { showsOthers.toggle() }
                } label: {
                    HStack(spacing: 12) {
                        Text("Other agents")
                        Spacer()
                        if !showsOthers {
                            if others.contains(where: { model.startChoice(for: $0.name).bypasses }) { Self.unasked }
                            HStack(spacing: 4) { ForEach(others) { AgentIcon(agent: $0.id, size: 22) } }
                                .transition(.opacity)
                                .accessibilityHidden(true)
                        }
                        Image(systemName: "chevron.down").font(.footnote.weight(.semibold)).foregroundStyle(.tertiary)
                            .rotationEffect(.degrees(showsOthers ? 180 : 0))
                    }
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .accessibilityValue(showsOthers ? "Shown" : "Hidden")
                if showsOthers {
                    ForEach(others) { agent in
                        Button {
                            start(agent, in: workspace)
                        } label: {
                            HStack(spacing: 12) {
                                AgentIcon(agent: agent.id, size: 28)
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(agent.name)
                                    if let label = model.startChoice(for: agent.name).label {
                                        Text(label).font(.caption).foregroundStyle(.secondary)
                                    }
                                }
                                Spacer()
                                if model.startChoice(for: agent.name).bypasses { Self.unasked }
                                if starting == agent.id { ProgressView() }
                            }
                            .contentShape(.rect)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("New \(agent.name) tab")
                        .accessibilityValue(model.startChoice(for: agent.name).label ?? "")
                        .contextMenu { firstPrompt(agent, in: workspace) }
                    }
                }
            }
        }
        .disabled(!online || starting != nil)
    }

    /// Says a new tab would run tools without asking.
    private static var unasked: some View {
        Image(systemName: "bolt.shield.fill").foregroundStyle(.red).accessibilityLabel("Runs without asking")
    }

    private func firstPrompt(_ agent: AgentChoice, in workspace: Home.Workspace) -> some View {
        Button("With a First Prompt…", systemImage: "text.bubble") {
            model.creating = .chat(device: key.device, workspace: workspace.id, label: workspace.label, agent: agent.id)
        }
    }

    private func bigButton(_ name: String, id: String, @ViewBuilder icon: () -> some View, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(spacing: 8) {
                if starting == id { ProgressView().frame(width: 40, height: 40) } else { icon() }
                Text(name).font(.subheadline.weight(.semibold)).lineLimit(1)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 14)
            .background(.fill.quinary, in: .rect(cornerRadius: 16))
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("New \(name) tab")
    }

    private func isOnline(_ machine: Fleet.Machine) -> Bool {
        if case .online = machine.status { true } else { false }
    }

    private func start(_ agent: AgentChoice, in workspace: Home.Workspace) {
        let (device, id) = (key.device, workspace.id)
        let start = model.startChoice(for: agent.name).start
        starting = agent.id
        Task {
            defer { starting = nil }
            do {
                let created = try await model.fleet.call(device) { client, token in
                    try await client.startChat(NewChatRequest(agent: agent.id, start: start), workspace: id, bearer: token)
                }
                model.recentAgent = agent.id
                if let problem = model.startProblem(created, start: start, device: device) { model.notice = problem }
                if let pane = created.pane { opened = .chat(PaneKey(device: device, pane: pane), title: agent.name) }
            } catch {
                problem = error.localizedDescription
            }
        }
    }

    /// Sheprd opens a tab in the workspace of the pane an action names, so
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

/// Opens the workspace's changes, saying how many files are uncommitted.
private struct ChangesRow: View {
    var count: Int?

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "plusminus").font(.system(size: 14, weight: .semibold))
                .frame(width: 28, height: 28).background(.fill.secondary, in: .rect(cornerRadius: 28 * 0.28))
            Text("Changes").frame(maxWidth: .infinity, alignment: .leading)
            if let count {
                Text(count, format: .number).foregroundStyle(.secondary).monospacedDigit()
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
    }

    private var label: String {
        switch count {
        case nil: "Changes"
        case 1?: "Changes, 1 file"
        case let count?: "Changes, \(count) files"
        }
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
