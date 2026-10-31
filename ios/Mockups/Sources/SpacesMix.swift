import SwiftUI

// The mix picked from round "spaces": A's search and computer headers with
// every computer on one screen, C's repositories as inset sections, and
// C's workspace screen with one-tap launchers.

struct SpacesMix: View {
    var state: Int

    var body: some View {
        SpacesStack(state: state, title: "Workspaces", tab: "Workspaces") {
            List {
                ForEach(SpaceSample.machines) { machine in
                    Section {
                        EmptyView()
                    } header: {
                        MachineHeader(machine: machine)
                    }
                    ForEach(machine.repos) { repo in
                        Section {
                            MixRow(space: repo.main, main: true)
                            ForEach(repo.worktrees) { MixRow(space: $0, main: false) }
                        } header: {
                            HStack(spacing: 8) {
                                RepoAvatar(repo: repo, size: 20)
                                Text(repo.name).font(.subheadline.weight(.semibold)).foregroundStyle(.primary)
                                Spacer()
                                Image(systemName: "plus").foregroundStyle(.tint)
                            }
                            .textCase(nil)
                        }
                    }
                }
            }
            .listStyle(.insetGrouped)
            .listSectionSpacing(14)
            .searchable(text: .constant(""), prompt: "Workspaces")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) { Button {} label: { Image(systemName: "plus") } }
            }
        } detail: {
            SpaceDetailLauncher()
        }
    }
}

private struct MixRow: View {
    var space: Space
    var main: Bool

    var body: some View {
        NavigationLink(value: 0) {
            HStack(spacing: 10) {
                space.mark.view.frame(width: 12)
                VStack(alignment: .leading, spacing: 1) {
                    Text(main ? space.branch : space.name).lineLimit(1)
                        .font(main ? .body.monospaced() : .body)
                    if space.agents > 0 || space.terminals > 1 {
                        Text(counts).font(.caption).foregroundStyle(.secondary)
                    }
                }
                Spacer(minLength: 4)
                if space.note { Image(systemName: "pencil.circle").foregroundStyle(.brown) }
                if let pr = space.pr {
                    Text(verbatim: "#\(pr.number)").font(.caption.weight(.semibold).monospacedDigit())
                        .foregroundStyle(pr.tone.color)
                        .padding(.horizontal, 7).padding(.vertical, 2)
                        .background(pr.tone.color.opacity(0.15), in: .capsule)
                }
            }
        }
    }

    private var counts: String {
        let agents = space.agents == 1 ? "1 agent" : "\(space.agents) agents"
        let terms = space.terminals == 1 ? "1 terminal" : "\(space.terminals) terminals"
        return space.agents > 0 ? "\(agents) · \(terms)" : terms
    }
}
