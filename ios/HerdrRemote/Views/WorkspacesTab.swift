import HerdrRemoteCore
import SwiftUI

/// One workspace of one computer, which the Workspaces tab opens.
struct WorkspaceKey: Hashable {
    let device: String
    let workspace: String
}

/// The Workspaces tab, laid out like Herdr UI's sidebar: every computer
/// with its load, then its repositories, each with its own checkout and
/// its linked worktrees. A workspace opens on a screen of its own with its
/// tabs (`WorkspaceScreen`). Search covers workspaces, chats, their
/// messages, and terminals on every computer.
struct WorkspacesTab: View {
    @Environment(AppModel.self) private var model
    @Binding var pairing: Bool
    @State private var query = ""

    var body: some View {
        Group {
            if query.isEmpty { list } else { SearchResults(query: query) }
        }
        .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Workspaces, chats, terminals")
        // No title, as Home and Settings have none; the search names it.
        .navigationBarTitleDisplayMode(.inline)
        .navigationDestination(for: WorkspaceKey.self) { WorkspaceScreen(key: $0) }
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button("New Workspace", systemImage: "folder.badge.plus") { model.creating = .workspace(device: nil) }
                        .disabled(model.fleet.machines.isEmpty)
                    Button("Pair a Computer", systemImage: "laptopcomputer.and.iphone") { pairing = true }
                } label: { Image(systemName: "plus") }
                .accessibilityLabel("New")
            }
        }
    }

    private var list: some View {
        List {
            ForEach(model.fleet.machines) { machine in
                Section {
                    HostRows(computer: machine.computer)
                } header: {
                    MachineHeader(machine: machine)
                        .contextMenu {
                            Button("Forget \(model.displayName(machine.computer))", systemImage: "trash", role: .destructive) {
                                model.forget(machine.computer)
                            }
                        }
                }
                ForEach(machine.home?.repositories ?? []) { repository in
                    Section {
                        ForEach(repository.workspaces) { workspace in
                            NavigationLink(value: WorkspaceKey(device: machine.id, workspace: workspace.id)) {
                                WorkspaceRow(workspace: workspace, repository: repository)
                            }
                        }
                    } header: {
                        RepositoryHeader(repository: repository, device: machine.id)
                    }
                }
            }
            Section {
                Button { pairing = true } label: { Label("Add a computer", systemImage: "plus") }
            }
        }
        .listStyle(.insetGrouped)
        .listSectionSpacing(14)
        .refreshable { model.fleet.networkChanged() }
    }
}

/// A computer above its repositories: its name, how the phone reaches it,
/// whether it is connected, and its CPU and memory.
private struct MachineHeader: View {
    @Environment(AppModel.self) private var model
    var machine: Fleet.Machine

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Image(systemName: machine.computer.via == nil ? "laptopcomputer" : "server.rack")
                Text(model.displayName(machine.computer)).font(.headline).lineLimit(1)
                if let vouching = model.vouching(machine.computer) {
                    Text("via \(model.displayName(vouching))").font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer()
                MachineStatus(machine: machine)
            }
            .foregroundStyle(.primary)
            if let load = machine.home?.load {
                HStack(spacing: 12) {
                    LoadGauge(label: "CPU", percent: load.cpu)
                    LoadGauge(label: "MEM", percent: load.memory)
                }
            }
        }
        .textCase(nil)
        .padding(.vertical, 4)
    }
}

/// Where the phone stands with a computer, after its name.
struct MachineStatus: View {
    var machine: Fleet.Machine

    var body: some View {
        switch machine.status {
        case .online:
            HStack(spacing: 6) {
                if let waiting = machine.home?.waiting, waiting > 0 {
                    Text("\(waiting) need you").font(.caption.weight(.semibold)).foregroundStyle(.white)
                        .padding(.horizontal, 8).padding(.vertical, 2).background(Home.AgentStatus.blocked.color, in: .capsule)
                }
                Circle().fill(.green).frame(width: 8, height: 8).accessibilityLabel("Connected")
            }
        case .connecting:
            ProgressView().controlSize(.small)
        case .offline:
            Image(systemName: "wifi.slash").foregroundStyle(.secondary).accessibilityLabel("Offline")
        case .revoked:
            Image(systemName: "exclamationmark.triangle").foregroundStyle(.red).accessibilityLabel("Unpaired")
        case .outdated:
            Image(systemName: "arrow.up.circle").foregroundStyle(.orange).accessibilityLabel("Needs a newer Herdr")
        }
    }
}

/// A share of the computer in use, as Herdr UI's host rows draw it.
private struct LoadGauge: View {
    var label: String
    var percent: Int

    var body: some View {
        HStack(spacing: 5) {
            Text(label).font(.caption2.weight(.semibold))
            Capsule().fill(.quaternary).frame(width: 34, height: 5)
                .overlay(alignment: .leading) {
                    Capsule().fill(percent >= 80 ? AnyShapeStyle(.orange) : AnyShapeStyle(.secondary))
                        .frame(width: 34 * CGFloat(min(max(percent, 0), 100)) / 100, height: 5)
                }
            Text("\(percent)%").font(.caption2.monospacedDigit())
        }
        .foregroundStyle(.secondary)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(label) \(percent) percent")
    }
}

/// A repository's name over its workspaces, with a button for another
/// worktree of it.
private struct RepositoryHeader: View {
    @Environment(AppModel.self) private var model
    var repository: Repository
    var device: String

    var body: some View {
        HStack(spacing: 8) {
            RepositoryAvatar(label: repository.label)
            Text(repository.label).font(.subheadline.weight(.semibold)).foregroundStyle(.primary).lineLimit(1)
            Spacer()
            // Its own checkout is where a new worktree branches from.
            if let own = repository.workspaces.first(where: { $0.worktree?.linked != true }) {
                Button {
                    model.creating = .workspace(device: device, folder: own.cwd)
                } label: {
                    Image(systemName: "plus")
                }
                .accessibilityLabel("New worktree of \(repository.label)")
            }
        }
        .textCase(nil)
    }
}

/// A repository's initial on a tile, in a color its name picks.
struct RepositoryAvatar: View {
    var label: String
    var size: CGFloat = 20

    private static let tints: [Color] = [.green, .orange, .blue, .purple, .pink, .teal, .indigo, .brown]

    var body: some View {
        let sum = label.unicodeScalars.reduce(0) { $0 &+ Int($1.value) }
        Text(label.first.map { String($0).uppercased() } ?? "?")
            .font(.system(size: size * 0.55, weight: .bold, design: .rounded))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(Self.tints[sum % Self.tints.count].gradient, in: .rect(cornerRadius: size * 0.3))
            .accessibilityHidden(true)
    }
}

/// A workspace in its repository's section: the checkout's branch, or the
/// worktree's name, its agents, and its pull request.
private struct WorkspaceRow: View {
    var workspace: Home.Workspace
    var repository: Repository

    var body: some View {
        HStack(spacing: 10) {
            WorkspaceMark(workspace: workspace)
            VStack(alignment: .leading, spacing: 1) {
                if let branch = ownBranch {
                    Text(branch).font(.body.monospaced()).lineLimit(1).truncationMode(.middle)
                } else {
                    Text(workspace.label).lineLimit(1)
                }
                if let counts { Text(counts).font(.caption).foregroundStyle(.secondary) }
            }
            Spacer(minLength: 4)
            if let pullRequest = workspace.pullRequest { PullRequestPill(pullRequest: pullRequest) }
        }
    }

    /// A repository's own checkout named after it shows its branch, as the
    /// desktop shows `main` under the repository.
    private var ownBranch: String? {
        guard workspace.worktree?.linked != true, workspace.label == repository.label else { return nil }
        return workspace.branch
    }

    private var counts: String? {
        let agents = workspace.chats.count + workspace.terminals.filter { $0.agent != nil }.count
        guard agents > 0 else { return nil }
        let shells = workspace.terminals.filter { $0.agent == nil }.count
        let text = agents == 1 ? "1 agent" : "\(agents) agents"
        return switch shells {
        case 0: text
        case 1: "\(text) · 1 terminal"
        default: "\(text) · \(shells) terminals"
        }
    }
}

/// A workspace's most pressing agent, as the desktop's sidebar marks it:
/// in its colors (`Home.AgentStatus.color`): a dot when it needs you or
/// finished, an orbit while it works, a ring when idle.
struct WorkspaceMark: View {
    var workspace: Home.Workspace

    var body: some View {
        Group {
            if workspace.asking {
                Circle().fill(Home.AgentStatus.blocked.color).accessibilityLabel("Needs you")
            } else {
                switch workspace.status {
                case .blocked:
                    Circle().fill(Home.AgentStatus.blocked.color).accessibilityLabel("Needs you")
                case .working:
                    WorkingOrbit()
                case .done:
                    Circle().fill(Home.AgentStatus.done.color).accessibilityLabel("Done")
                case .idle:
                    Circle().strokeBorder(Home.AgentStatus.idle.color, lineWidth: 1.5).accessibilityLabel("Idle")
                case .unknown, nil:
                    Circle().strokeBorder(.tertiary, lineWidth: 1.5).accessibilityHidden(true)
                }
            }
        }
        .frame(width: 10, height: 10)
    }
}

/// A pull request's number, colored as Herdr UI colors its state.
struct PullRequestPill: View {
    var pullRequest: Home.PullRequest

    var body: some View {
        let color = pullRequest.tone.color
        Text(verbatim: "#\(pullRequest.number)")
            .font(.caption.weight(.semibold).monospacedDigit())
            .foregroundStyle(color)
            .padding(.horizontal, 7).padding(.vertical, 2)
            .background(color.opacity(0.15), in: .capsule)
            .accessibilityLabel("Pull request \(pullRequest.number), \(pullRequest.tone.label)")
    }
}

extension Home.PullRequest.Tone {
    var color: Color {
        switch self {
        case .attention: .red
        case .held: .orange
        case .merged: .purple
        case .ready: .green
        case .waiting: .yellow
        case .other: .secondary
        }
    }

    /// The state as an icon; `label` says it to VoiceOver.
    var symbol: String {
        switch self {
        case .attention: "xmark.circle.fill"
        case .held: "exclamationmark.triangle.fill"
        case .merged: "arrow.triangle.merge"
        case .ready: "checkmark.circle.fill"
        case .waiting: "clock.fill"
        case .other: "pencil.circle.fill"
        }
    }

    var label: String {
        switch self {
        case .attention: "needs attention"
        case .held: "blocked or behind"
        case .merged: "merged"
        case .ready: "ready to merge"
        case .waiting: "checks or review pending"
        case .other: "draft"
        }
    }
}
