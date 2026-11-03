import HerdrRemoteCore
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
    @State private var options: NewOptions?
    /// The computer `options` came from. The form reappears after the
    /// folder picker, which restarts `load`; this keeps the chosen folder.
    @State private var loadedFrom: String?
    @State private var folder: String?
    @State private var worktree = false
    @State private var branch = ""
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
                Section("Computer") {
                    Picker("Computer", selection: $device) {
                        ForEach(model.fleet.machines) { machine in
                            Text(model.displayName(machine.computer)).tag(Optional(machine.id))
                        }
                    }
                    .pickerStyle(.menu)
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
                if device == nil { device = model.fleet.machines.first { if case .online = $0.status { true } else { false } }?.id }
            }
            .task(id: device) { await load() }
        }
        .interactiveDismissDisabled(creating)
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
        options = nil
        folder = nil
        problem = nil
        do {
            let loaded = try await model.fleet.call(device) { client, token in try await client.newOptions(bearer: token) }
            options = loaded
            loadedFrom = device
            folder = (preset?.device == device ? preset?.folder : nil) ?? loaded.recent.first
            if !loaded.agents.contains(where: { $0.id == agent }), let first = loaded.agents.first { agent = first.id }
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
                    if creating { ProgressView() } else { Button("Start") { start() }.disabled(agents.isEmpty) }
                }
            }
            .task {
                do {
                    agents = try await model.fleet.call(device) { client, token in try await client.newOptions(bearer: token) }.agents
                } catch {
                    problem = error.localizedDescription
                }
            }
        }
        .interactiveDismissDisabled(creating)
    }

    private func start() {
        let request = NewChatRequest(agent: agent, prompt: prompt.trimmingCharacters(in: .whitespacesAndNewlines))
        let (device, workspace) = (device, workspace)
        let title = agents.first { $0.id == agent }?.name ?? "Chat"
        creating = true
        Task {
            defer { creating = false }
            do {
                let created = try await model.fleet.call(device) { client, token in
                    try await client.startChat(request, workspace: workspace, bearer: token)
                }
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
