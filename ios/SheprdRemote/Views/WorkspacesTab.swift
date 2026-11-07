import SheprdRemoteCore
import SwiftUI

/// One workspace of one computer, which the Workspaces tab opens.
struct WorkspaceKey: Codable, Hashable {
    let device: String
    let workspace: String
}

extension EnvironmentValues {
    /// Whether a tab offers a button to its workspace: on Home, whose
    /// lists open tabs straight away, so nothing behind them leads there.
    @Entry var offersWorkspace = false
}

/// The Workspaces tab, laid out like Sheprd's sidebar: every computer
/// with its load, then its repositories, each with its own checkout and
/// its linked worktrees. A computer running more than one Herdr session
/// lists its repositories under each session's name. A workspace opens on
/// a screen of its own with its tabs (`WorkspaceScreen`). Search covers
/// workspaces, chats, their messages, and terminals on every computer,
/// collapsed or not.
///
/// With many computers the list gets long, so each collapses to its header,
/// all of them at once from the toolbar, and a long list gets a section
/// index on its edge, as Contacts has.
struct WorkspacesTab: View {
    @Environment(AppModel.self) private var model
    @State private var query = ""
    /// The computer whose page is open from its header menu.
    @State private var details: String?
    @State private var forgetting: Computer?

    /// Workspaces open at once before the list needs its index: about a
    /// screenful on a phone.
    private static let indexedFrom = 12

    var body: some View {
        Group {
            if query.isEmpty { list } else { SearchResults(query: query) }
        }
        .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Workspaces, chats, terminals")
        // No title, as Home and Settings have none; the search names it.
        .navigationBarTitleDisplayMode(.inline)
        .navigationDestination(for: WorkspaceKey.self) { WorkspaceScreen(key: $0) }
        .navigationDestination(item: $details) { ComputerScreen(device: $0) }
        .forgetConfirmation($forgetting)
        #if DEBUG
        .task { if UserDefaults.standard.bool(forKey: "openComputer") { details = model.computers.first?.device } }
        #endif
        .toolbar {
            if query.isEmpty, machines.count > 1 {
                ToolbarItem(placement: .topBarTrailing) {
                    let shown = machines.map(\.id)
                    let folded = model.collapsed.computers.allCollapsed(shown)
                    Button(
                        folded ? "Expand All" : "Collapse All",
                        systemImage: folded ? "rectangle.expand.vertical" : "rectangle.compress.vertical"
                    ) {
                        withAnimation(.snappy) { model.collapsed.computers.toggleAll(shown) }
                    }
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button("New Workspace", systemImage: "plus") { model.creating = .workspace(device: nil) }
                    .disabled(model.fleet.machines.isEmpty)
            }
        }
    }

    private var list: some View {
        List {
            TagFilterBar()
            ForEach(machines) { machine in
                let collapsed = model.collapsed.computers.contains(machine.id)
                Section {
                    if !collapsed { HostRows(computer: machine.computer) }
                } header: {
                    MachineHeader(machine: machine, collapsed: collapsed, toggle: {
                        withAnimation(.snappy) { model.collapsed.computers.toggle(machine.id) }
                    }) {
                        MachineActions(machine: machine, details: { details = machine.id }, forget: { forgetting = machine.computer })
                    }
                    .contextMenu {
                        MachineActions(machine: machine, details: { details = machine.id }, forget: { forgetting = machine.computer })
                    }
                }
                .sectionIndexLabel(model.displayName(machine.computer).first.map { String($0).uppercased() })
                ForEach(collapsed ? [] : machine.home?.sessionGroups ?? []) { group in
                    if let name = group.name {
                        Section {} header: { SessionHeader(name: name) }
                            .sectionIndexLabel("•")
                    }
                    ForEach(group.repositories) { repository in
                        Section {
                            ForEach(repository.workspaces) { workspace in
                                NavigationLink(value: WorkspaceKey(device: machine.id, workspace: workspace.id)) {
                                    WorkspaceRow(workspace: workspace, repository: repository)
                                }
                            }
                        } header: {
                            RepositoryHeader(repository: repository, device: machine.id)
                        }
                        // A dot, as repositories repeat across computers: the
                        // letters say where each computer starts.
                        .sectionIndexLabel("•")
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
        .listSectionSpacing(14)
        .listSectionIndexVisibility(openWorkspaces >= Self.indexedFrom ? .visible : .hidden)
        .refreshable { model.fleet.networkChanged() }
        .nearbyBanner()
        .overlay {
            if model.computers.isEmpty {
                ContentUnavailableView("No computers yet", systemImage: "desktopcomputer",
                                       description: Text("Pair one in Settings to see its workspaces here."))
            }
        }
    }

    /// The computers the tag filter leaves.
    private var machines: [Fleet.Machine] {
        let tag = ComputerTags.active(model.tagFilter.tag, among: model.computers)
        return model.fleet.machines.filter { $0.computer.matches(tag: tag) }
    }

    private var openWorkspaces: Int {
        machines
            .filter { !model.collapsed.computers.contains($0.id) }
            .reduce(0) { $0 + ($1.home?.workspaces.count ?? 0) }
    }
}

/// A computer above its repositories: its name, how the phone reaches it,
/// whether it is connected, and its CPU and memory. A tap on it collapses
/// or opens the computer; collapsed, it says what it holds, and its status
/// still says when an agent needs you.
private struct MachineHeader<Actions: View>: View {
    @Environment(AppModel.self) private var model
    var machine: Fleet.Machine
    var collapsed: Bool
    var toggle: () -> Void
    @ViewBuilder var actions: Actions

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            summary
                .contentShape(.rect)
                .onTapGesture(perform: toggle)
            // Beside the tap, not inside it: a tap here would do both.
            Menu { actions } label: {
                Image(systemName: "ellipsis.circle").font(.title3)
            }
            .foregroundStyle(.primary)
            .accessibilityLabel("\(model.displayName(machine.computer)) actions")
        }
        .textCase(nil)
        .padding(.vertical, 4)
    }

    private var summary: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                // The header's tap is for touch; this is what VoiceOver finds.
                Button(action: toggle) {
                    Image(systemName: "chevron.right")
                        .font(.caption.weight(.bold))
                        .foregroundStyle(.secondary)
                        .rotationEffect(.degrees(collapsed ? 0 : 90))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("\(collapsed ? "Expand" : "Collapse") \(model.displayName(machine.computer))")
                Image(systemName: machine.computer.via == nil ? "laptopcomputer" : "server.rack")
                Text(model.displayName(machine.computer)).font(.headline).lineLimit(1)
                if let vouching = model.vouching(machine.computer) {
                    Text("via \(model.displayName(vouching))").font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer()
                MachineStatus(machine: machine)
            }
            .foregroundStyle(.primary)
            HStack(spacing: 12) {
                if let load = machine.home?.load {
                    LoadGauge(label: "CPU", percent: load.cpu)
                    LoadGauge(label: "MEM", percent: load.memory)
                }
                if collapsed, let held {
                    Spacer(minLength: 0)
                    Text(held).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            if !machine.computer.tags.isEmpty {
                PillFlow {
                    ForEach(machine.computer.tags, id: \.self) { TagPill(tag: $0) }
                }
            }
        }
    }

    private var held: String? {
        guard let count = machine.home?.workspaces.count, count > 0 else { return nil }
        return count == 1 ? "1 workspace" : "\(count) workspaces"
    }
}

/// A Herdr session above its repositories, when the computer runs more
/// than one.
private struct SessionHeader: View {
    var name: String

    var body: some View {
        Label(name, systemImage: "rectangle.stack")
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(.secondary)
            .textCase(nil)
            .lineLimit(1)
            .accessibilityLabel("Herdr session \(name)")
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
            Image(systemName: "arrow.up.circle").foregroundStyle(.orange).accessibilityLabel("Needs a newer Sheprd")
        }
    }
}

/// A share of the computer in use, as Sheprd's host rows draw it.
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
        // The row shares its width with the workspace count: without this
        // the row squeezes these texts until "CPU" breaks across two lines.
        .fixedSize(horizontal: true, vertical: false)
        .foregroundStyle(.secondary)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(label) \(percent) percent")
    }
}

/// A repository's name over its workspaces, with a button for another
/// worktree of it, in the same Herdr session.
private struct RepositoryHeader: View {
    @Environment(AppModel.self) private var model
    var repository: Repository
    var device: String

    var body: some View {
        HStack(spacing: 8) {
            RepositoryAvatar(label: repository.label)
            Text(repository.label).font(.subheadline.weight(.semibold)).foregroundStyle(.primary).lineLimit(1)
            Spacer()
            if let own = repository.ownCheckout {
                Button {
                    model.creating = .workspace(device: device, folder: own.cwd, session: own.session)
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

    /// The color `label` picks, here and on tag pills.
    static func tint(_ label: String) -> Color {
        tints[label.unicodeScalars.reduce(0) { $0 &+ Int($1.value) } % tints.count]
    }

    var body: some View {
        Text(label.first.map { String($0).uppercased() } ?? "?")
            .font(.system(size: size * 0.55, weight: .bold, design: .rounded))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(Self.tint(label).gradient, in: .rect(cornerRadius: size * 0.3))
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
            WorkspacePullRequestBadge(workspace: workspace)
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

/// What a workspace's row says of its pull request: the one it has, or that
/// its commits still need one.
struct WorkspacePullRequestBadge: View {
    var workspace: Home.Workspace

    var body: some View {
        if let pullRequest = workspace.pullRequest {
            PullRequestPill(pullRequest: pullRequest)
        } else if let commits = workspace.unopenedCommits {
            UnopenedPill(commits: commits)
        }
    }
}

/// Commits on a branch with no pull request yet, in the number's place and
/// the color Sheprd's sidebar gives it.
struct UnopenedPill: View {
    var commits: Int

    var body: some View {
        Text(verbatim: "+PR")
            .font(.caption.weight(.semibold).monospacedDigit())
            .foregroundStyle(.yellow)
            .padding(.horizontal, 7).padding(.vertical, 2)
            .background(Color.yellow.opacity(0.15), in: .capsule)
            .accessibilityLabel(commits == 1 ? "1 commit, no pull request" : "\(commits) commits, no pull request")
    }
}

/// A pull request's number, colored as Sheprd colors its state.
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

/// What a computer's header menu and long press offer.
private struct MachineActions: View {
    @Environment(AppModel.self) private var model
    @Environment(WindowState.self) private var window: WindowState?
    var machine: Fleet.Machine
    var details: () -> Void
    var forget: () -> Void

    var body: some View {
        let computer = machine.computer
        Toggle("Hold Alerts at the Desk", systemImage: "bell.badge", isOn: Binding(
            get: { computer.presence },
            set: { model.setPresence($0, for: computer) }
        ))
        Button("Computer Details", systemImage: "info.circle", action: details)
        if machine.status == .revoked {
            Button("Pair Again", systemImage: "arrow.triangle.2.circlepath") { window?.pairing = true }
        }
        Divider()
        Button("Forget \(model.displayName(computer))…", systemImage: "trash", role: .destructive, action: forget)
    }
}
