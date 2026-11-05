import SheprdRemoteCore
import SwiftUI

/// What the app is creating, shown as a sheet over every tab.
enum Creating: Identifiable, Hashable {
    /// A workspace, on `device` when the person came from that computer,
    /// as a new worktree of the checkout in `folder` when they came from
    /// its repository.
    case workspace(device: String?, folder: String? = nil)
    /// A chat in an existing workspace, with `agent` chosen when the
    /// person picked one before asking for the form.
    case chat(device: String, workspace: String, label: String, agent: String? = nil)

    var id: String {
        switch self {
        case let .workspace(device, folder): "workspace/\(device ?? "")/\(folder ?? "")"
        case let .chat(device, workspace, _, _): "chat/\(device)/\(workspace)"
        }
    }
}

/// New workspace A: one form for the computer, the project, a new
/// worktree with its branch, the agent, and an optional first prompt.
struct NewWorkspaceSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var device: String?
    /// The computer was chosen for its load. It stays the one chosen, as
    /// the desktop's ranking settles, so the form does not move under the
    /// person with every sample.
    @State private var automatic = false
    @State private var options: NewOptions?
    /// The computer `options` came from. The form reappears after the
    /// folder picker, which restarts `load`; this keeps the chosen folder.
    @State private var loadedFrom: String?
    @State private var folder: String?
    @State private var worktree = false
    /// Proposed up front, as on the desktop, so Create needs no typing.
    @State private var branch = NewWorkspaceRequest.proposedBranch()
    @State private var agent = "claude"
    @State private var prompt = ""
    @State private var problem: String?
    @State private var creating = false

    /// The checkout the person came from, chosen once its computer's
    /// folders load, unless they pick another computer.
    private let preset: (device: String, folder: String)?

    init(device: String?, folder: String? = nil) {
        _device = State(initialValue: device)
        _worktree = State(initialValue: folder != nil)
        preset = device.flatMap { device in folder.map { (device, $0) } }
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker("Computer", selection: computerChoice) {
                        if offersLeastBusy {
                            Text(leastBusyName).tag(String?.none)
                        }
                        ForEach(model.fleet.machines) { machine in
                            Text(model.displayName(machine.computer)).tag(Optional(machine.id))
                        }
                    }
                    .pickerStyle(.menu)
                } header: {
                    Text("Computer")
                } footer: {
                    if automatic, let load = model.fleet.machines.first(where: { $0.id == device })?.home?.load {
                        Text("Chosen for its room: \(load.spareCores, format: .number.precision(.fractionLength(0...1))) cores spare, \(load.memory)% memory in use.")
                    }
                }
                Section("Project") {
                    NavigationLink {
                        FolderPicker(options: options, selection: $folder)
                    } label: {
                        if let folder {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(URL(filePath: folder).lastPathComponent).font(.headline)
                                Text(folder).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1).truncationMode(.head)
                            }
                        } else if options == nil, problem == nil {
                            HStack { Text("Loading folders…").foregroundStyle(.secondary); Spacer(); ProgressView() }
                        } else {
                            Text("Choose a folder").foregroundStyle(.secondary)
                        }
                    }
                    .disabled(options == nil)
                    Toggle("New worktree", isOn: $worktree.animation())
                    if worktree {
                        TextField("Branch", text: $branch)
                            .textInputAutocapitalization(.never).autocorrectionDisabled()
                            .font(.body.monospaced())
                    }
                }
                Section("Agent") {
                    AgentGrid(agents: options?.agents ?? [], selection: $agent)
                }
                Section {
                    TextField("What should it do first? (optional)", text: $prompt, axis: .vertical).lineLimit(2...6)
                } header: {
                    Text("First prompt")
                } footer: {
                    if let problem { Text(problem).foregroundStyle(.red) }
                }
            }
            .navigationTitle("New Workspace")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    if creating {
                        ProgressView()
                    } else {
                        Button("Create") { create() }.disabled(!ready)
                    }
                }
            }
            .onAppear {
                guard device == nil else { return }
                let online = model.fleet.machines.filter { if case .online = $0.status { true } else { false } }
                device = online.first { $0.id == model.recentDevice }?.id ?? online.first?.id
            }
            .task(id: device) { await load() }
        }
        .interactiveDismissDisabled(creating)
    }

    /// The online computers that can take a workspace, best first.
    private var ranked: [Home.Candidate] {
        let candidates = model.fleet.machines.compactMap { machine -> Home.Candidate? in
            guard case .online = machine.status, let home = machine.home else { return nil }
            return Home.Candidate(id: machine.id, label: model.displayName(machine.computer), home: home)
        }
        return Home.leastBusy(candidates, current: device, repository: repository)
    }

    /// The repository of the chosen folder, by the name every computer
    /// knows it by, else the folder's own name.
    private var repository: String? {
        guard let folder else { return nil }
        let homes = model.fleet.machines.lazy.compactMap(\.home)
        return homes.compactMap { $0.workspaces.first { $0.cwd == folder }?.worktree?.label }.first
            ?? URL(filePath: folder).lastPathComponent
    }

    /// One computer alone is no choice.
    private var offersLeastBusy: Bool { ranked.count > 1 }

    private var leastBusyName: String {
        guard automatic, let machine = model.fleet.machines.first(where: { $0.id == device }) else { return "Least busy" }
        return "Least busy: \(model.displayName(machine.computer))"
    }

    /// The picker's selection: a computer, or nil for the least busy one.
    private var computerChoice: Binding<String?> {
        Binding {
            automatic && offersLeastBusy ? nil : device
        } set: { choice in
            automatic = choice == nil
            if let chosen = choice ?? ranked.first?.id { device = chosen }
        }
    }

    private var ready: Bool {
        folder != nil && !agent.isEmpty && (!worktree || Self.plausibleBranch(branch))
    }

    /// The daemon checks the branch properly; this only catches typos
    /// before the round trip.
    static func plausibleBranch(_ name: String) -> Bool {
        let name = name.trimmingCharacters(in: .whitespaces)
        return !name.isEmpty && !name.contains(" ") && !name.contains("..") && !name.hasPrefix("-")
    }

    private func load() async {
        // Choosing the computer here would change the task's id and
        // cancel this load; `onAppear` chooses it first.
        guard let device, device != loadedFrom else { return }
        // Moving to another computer keeps the project when it has it;
        // paths differ between computers.
        let project = repository
        let name = folder.map { URL(filePath: $0).lastPathComponent }
        options = nil
        folder = nil
        problem = nil
        do {
            let loaded = try await model.fleet.call(device) { client, token in try await client.newOptions(bearer: token) }
            options = loaded
            loadedFrom = device
            let home = model.fleet.machines.first { $0.id == device }?.home
            let checkout = project.flatMap { home?.checkout(ofRepository: $0) }
            folder = (preset?.device == device ? preset?.folder : nil)
                ?? checkout.flatMap { loaded.folders.contains($0) ? $0 : nil }
                ?? loaded.folders.first { URL(filePath: $0).lastPathComponent == name }
                ?? loaded.recent.first
            agent = loaded.agent(preferring: agent)?.id ?? ""
        } catch where !Task.isCancelled {
            problem = error.localizedDescription
        } catch {}
    }

    private func create() {
        guard let device, let folder else { return }
        let request = NewWorkspaceRequest(
            cwd: folder,
            branch: worktree ? branch.trimmingCharacters(in: .whitespaces) : "",
            agent: agent,
            prompt: prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        )
        let title = options?.agents.first { $0.id == agent }?.name ?? "Chat"
        creating = true
        Task {
            defer { creating = false }
            do {
                let created = try await model.fleet.call(device) { client, token in
                    try await client.createWorkspace(request, bearer: token)
                }
                model.recentAgent = request.agent
                dismiss()
                model.opened(created, prompt: request.prompt, device: device, title: title)
            } catch {
                problem = error.localizedDescription
            }
        }
    }
}

/// A chat with another agent in a workspace that already exists.
struct NewChatSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let device: String
    let workspace: String
    let label: String
    @State private var agents: [AgentChoice] = []
    @State private var agent: String
    @State private var prompt = ""
    @State private var problem: String?
    @State private var creating = false

    init(device: String, workspace: String, label: String, agent: String? = nil) {
        self.device = device
        self.workspace = workspace
        self.label = label
        _agent = State(initialValue: agent ?? "claude")
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("Agent") { AgentGrid(agents: agents, selection: $agent) }
                Section {
                    TextField("What should it do first? (optional)", text: $prompt, axis: .vertical).lineLimit(2...6)
                } header: {
                    Text("First prompt")
                } footer: {
                    if let problem { Text(problem).foregroundStyle(.red) }
                }
            }
            .navigationTitle("New Chat in \(label)")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    if creating { ProgressView() } else { Button("Start") { start() }.disabled(!agents.contains { $0.id == agent }) }
                }
            }
            .task {
                do {
                    let loaded = try await model.fleet.call(device) { client, token in try await client.newOptions(bearer: token) }
                    guard !Task.isCancelled else { return }
                    agents = loaded.agents
                    agent = loaded.agent(preferring: agent)?.id ?? ""
                } catch {
                    problem = error.localizedDescription
                }
            }
        }
        .interactiveDismissDisabled(creating)
    }

    private func start() {
        guard let selected = agents.first(where: { $0.id == agent }) else { return }
        let request = NewChatRequest(agent: agent, prompt: prompt.trimmingCharacters(in: .whitespacesAndNewlines))
        let (device, workspace) = (device, workspace)
        let title = selected.name
        creating = true
        Task {
            defer { creating = false }
            do {
                let created = try await model.fleet.call(device) { client, token in
                    try await client.startChat(request, workspace: workspace, bearer: token)
                }
                model.recentAgent = request.agent
                dismiss()
                model.opened(created, prompt: request.prompt, device: device, title: title)
            } catch {
                problem = error.localizedDescription
            }
        }
    }
}

/// The agents as a grid of their marks.
struct AgentGrid: View {
    let agents: [AgentChoice]
    @Binding var selection: String

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 92), spacing: 10)], spacing: 10) {
            ForEach(agents) { agent in
                let selected = agent.id == selection
                Button {
                    selection = agent.id
                } label: {
                    VStack(spacing: 6) {
                        AgentIcon(agent: agent.id, size: 32)
                        Text(agent.name).font(.caption.weight(.medium)).lineLimit(1)
                    }
                    .frame(maxWidth: .infinity).padding(.vertical, 10)
                    .background(selected ? AnyShapeStyle(.tint.opacity(0.16)) : AnyShapeStyle(.fill.quinary), in: .rect(cornerRadius: 14))
                    .overlay {
                        RoundedRectangle(cornerRadius: 14).strokeBorder(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.clear), lineWidth: 1.5)
                    }
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(selected ? .isSelected : [])
            }
        }
        .padding(.vertical, 4)
        .sensoryFeedback(.selection, trigger: selection)
    }
}

/// The folders a workspace may open in, searchable: recent ones, then
/// each project root's.
struct FolderPicker: View {
    @Environment(\.dismiss) private var dismiss
    let options: NewOptions?
    @Binding var selection: String?
    @State private var query = ""

    var body: some View {
        List {
            let recent = filter(options?.recent ?? [])
            if !recent.isEmpty {
                Section("Recent") { ForEach(recent, id: \.self, content: row) }
            }
            ForEach(options?.roots ?? []) { root in
                let folders = filter(root.folders)
                if !folders.isEmpty {
                    Section(root.path) { ForEach(folders, id: \.self, content: row) }
                }
            }
        }
        .searchable(text: $query, prompt: "Folders")
        .navigationTitle("Project")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func filter(_ folders: [String]) -> [String] {
        query.isEmpty ? folders : folders.filter { $0.localizedCaseInsensitiveContains(query) }
    }

    private func row(_ folder: String) -> some View {
        Button {
            selection = folder
            dismiss()
        } label: {
            HStack {
                Label(URL(filePath: folder).lastPathComponent, systemImage: "folder")
                Spacer()
                if folder == selection { Image(systemName: "checkmark").foregroundStyle(.tint) }
            }
        }
        .foregroundStyle(.primary)
    }
}

extension AppModel {
    /// Opens a chat just created. A first prompt that did not go, because
    /// the agent was slow to start or refused it, waits in the chat's
    /// composer, and the person is told rather than finding it gone.
    func opened(_ created: Created, prompt: String, device: String, title: String) {
        let unsent = !prompt.isEmpty && !created.prompted
        if let pane = created.pane {
            if unsent { fleet.cache.save(draft: Cache.Draft(text: prompt, pictures: []), device: device, pane: pane) }
            pendingRoute = .chat(PaneKey(device: device, pane: pane), title: title)
        }
        guard unsent else { return }
        notice = created.pane == nil
            ? "The chat started, but your first prompt was not sent. Open the chat from Home and send it again:\n\n\(prompt)"
            : "The chat started, but your first prompt was not sent. It is in the chat's message box: send it when the agent is ready."
    }
}
