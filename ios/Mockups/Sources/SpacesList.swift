import SwiftUI

// The Spaces tab, laid out like the desktop sidebar. State 0 is the list;
// state 1 has a workspace pushed on top, drawn in SpacesDetail.swift.
// A: native sidebar rows with tree lines. B: the desktop's monospaced
// sidebar, almost verbatim. C: one computer at a time, health card on
// top, repositories as inset sections. D: computer chips with load, and
// repositories folded to one row each until opened.

/// Pushes the variant's detail when `state` is 1.
struct SpacesStack<List: View, Detail: View>: View {
    var state: Int
    var title: String
    var tab = "Spaces"
    @ViewBuilder var list: List
    @ViewBuilder var detail: Detail

    var body: some View {
        SpacesTabs(title: tab) {
            NavigationStack(path: .constant(state == 1 ? [0] : [])) {
                list
                    .navigationTitle(title)
                    .navigationDestination(for: Int.self) { _ in detail }
            }
        }
    }
}

/// The ├ or └ the desktop draws before a worktree.
struct TreeLine: View {
    var last: Bool

    var body: some View {
        GeometryReader { geo in
            Path { path in
                let x = geo.size.width / 2
                path.move(to: CGPoint(x: x, y: 0))
                path.addLine(to: CGPoint(x: x, y: last ? geo.size.height / 2 : geo.size.height))
                path.move(to: CGPoint(x: x, y: geo.size.height / 2))
                path.addLine(to: CGPoint(x: geo.size.width, y: geo.size.height / 2))
            }
            .stroke(.tertiary, lineWidth: 1)
        }
        .frame(width: 14)
    }
}

// MARK: A

struct SpacesSidebar: View {
    var state: Int

    var body: some View {
        SpacesStack(state: state, title: "Workspaces", tab: "Workspaces") {
            List {
                ForEach(SpaceSample.machines) { machine in
                    Section {
                        ForEach(machine.repos) { repo in
                            row(repo.main, repo: repo, last: nil)
                            ForEach(Array(repo.worktrees.enumerated()), id: \.element.id) { index, space in
                                row(space, repo: repo, last: index == repo.worktrees.count - 1)
                            }
                        }
                    } header: {
                        MachineHeader(machine: machine)
                    }
                }
            }
            .listStyle(.plain)
            .environment(\.defaultMinListRowHeight, 30)
            .searchable(text: .constant(""), prompt: "Workspaces")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) { Button {} label: { Image(systemName: "plus") } }
            }
        } detail: {
            SpaceDetailList()
        }
    }

    private func row(_ space: Space, repo: Repo, last: Bool?) -> some View {
        NavigationLink(value: 0) {
            HStack(spacing: 8) {
                space.mark.view.frame(width: 12)
                if let last {
                    TreeLine(last: last).padding(.leading, 6)
                    Text(space.name).lineLimit(1)
                } else {
                    RepoAvatar(repo: repo)
                    VStack(alignment: .leading, spacing: 0) {
                        Text(space.name).font(.body.weight(.medium)).lineLimit(1)
                        Text(space.branch).font(.caption.monospaced()).foregroundStyle(.secondary)
                    }
                }
                Spacer(minLength: 4)
                if space.note { Image(systemName: "pencil.circle").foregroundStyle(.brown) }
                if let pr = space.pr { PRBadge(pr: pr) }
            }
            .frame(minHeight: last == nil ? 44 : 34)
            .fontWeight(space.name == "mobile app preview" ? .semibold : nil)
        }
        .listRowInsets(EdgeInsets(top: 0, leading: 16, bottom: 0, trailing: 16))
        .listRowSeparator(.hidden)
    }
}

struct MachineHeader: View {
    var machine: Machine

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Image(systemName: machine.symbol)
                Text(machine.name).font(.headline)
                Text(machine.via).font(.caption).foregroundStyle(.secondary)
                Spacer()
                Circle().fill(.green).frame(width: 8, height: 8)
            }
            .foregroundStyle(.primary)
            MachineLoad(machine: machine)
        }
        .textCase(nil)
        .padding(.vertical, 6)
    }
}

// MARK: B

struct SpacesTerminal: View {
    var state: Int

    var body: some View {
        SpacesStack(state: state, title: "Spaces") {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    HStack {
                        Text("spaces").foregroundStyle(.tertiary)
                        Spacer()
                        Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 8)
                    ForEach(SpaceSample.machines) { machine in
                        VStack(alignment: .leading, spacing: 2) {
                            HStack {
                                Text("▾").foregroundStyle(.secondary)
                                Image(systemName: "desktopcomputer").font(.footnote)
                                Text(machine.name)
                                Spacer()
                                Circle().fill(.green).frame(width: 9, height: 9)
                            }
                            MachineLoad(machine: machine, mono: true).padding(.leading, 26)
                        }
                        .padding(.vertical, 6).padding(.horizontal, 8)
                        .background(.fill.tertiary, in: .rect(cornerRadius: 6))
                        ForEach(machine.repos) { repo in
                            repoRows(repo)
                        }
                    }
                }
                .font(.system(.callout, design: .monospaced))
                .padding(.horizontal, 16)
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) { Button {} label: { Image(systemName: "plus") } }
            }
        } detail: {
            SpaceDetailCards()
        }
    }

    private func repoRows(_ repo: Repo) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                repo.main.mark.view.frame(width: 12)
                RepoAvatar(repo: repo, size: 18)
                Text(repo.name)
                Spacer()
                if repo.main.note { Image(systemName: "pencil.circle").foregroundStyle(.brown) }
            }
            .frame(height: 28)
            Text(repo.main.branch).foregroundStyle(.secondary).padding(.leading, 20).frame(height: 24)
            ForEach(Array(repo.worktrees.enumerated()), id: \.element.id) { index, space in
                HStack(spacing: 4) {
                    TreeLine(last: index == repo.worktrees.count - 1)
                    space.mark.view.frame(width: 12)
                    Text(space.name).lineLimit(1)
                        .fontWeight(space.name == "mobile app preview" ? .bold : nil)
                    Spacer(minLength: 4)
                    if space.note { Image(systemName: "pencil.circle").foregroundStyle(.brown) }
                    if let pr = space.pr { PRBadge(pr: pr, mono: true) }
                }
                .padding(.leading, 20)
                .frame(height: 30)
                .background(space.name == "mobile app preview" ? AnyShapeStyle(.fill.tertiary) : AnyShapeStyle(.clear))
            }
        }
        .padding(.leading, 8)
    }
}

// MARK: C

struct SpacesOneComputer: View {
    var state: Int

    var body: some View {
        let machine = SpaceSample.local
        SpacesStack(state: state, title: "") {
            List {
                Section {
                    HealthCard(machine: machine)
                }
                ForEach(machine.repos) { repo in
                    Section {
                        row(repo.main, main: true)
                        ForEach(repo.worktrees) { row($0, main: false) }
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
            .listStyle(.insetGrouped)
            .listSectionSpacing(14)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    HStack(spacing: 4) {
                        Text(machine.name).font(.headline)
                        Image(systemName: "chevron.down").font(.caption.bold()).foregroundStyle(.secondary)
                    }
                }
                ToolbarItem(placement: .topBarTrailing) { Button {} label: { Image(systemName: "magnifyingglass") } }
            }
        } detail: {
            SpaceDetailLauncher()
        }
    }

    private func row(_ space: Space, main: Bool) -> some View {
        NavigationLink(value: 0) {
            HStack(spacing: 10) {
                space.mark.view.frame(width: 12)
                VStack(alignment: .leading, spacing: 1) {
                    Text(main ? space.branch : space.name).lineLimit(1)
                        .font(main ? .body.monospaced() : .body)
                    if space.agents > 0 || space.terminals > 1 {
                        Text(counts(space)).font(.caption).foregroundStyle(.secondary)
                    }
                }
                Spacer(minLength: 4)
                if let pr = space.pr {
                    Text(verbatim: "#\(pr.number)").font(.caption.weight(.semibold).monospacedDigit())
                        .foregroundStyle(pr.tone.color)
                        .padding(.horizontal, 7).padding(.vertical, 2)
                        .background(pr.tone.color.opacity(0.15), in: .capsule)
                }
            }
        }
    }

    private func counts(_ space: Space) -> String {
        let agents = space.agents == 1 ? "1 agent" : "\(space.agents) agents"
        let terms = space.terminals == 1 ? "1 terminal" : "\(space.terminals) terminals"
        return space.agents > 0 ? "\(agents) · \(terms)" : terms
    }
}

private struct HealthCard: View {
    var machine: Machine

    var body: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Label("Online", systemImage: "circle.fill").font(.caption.weight(.semibold))
                    .foregroundStyle(.green).labelStyle(.titleAndIcon).imageScale(.small)
                Text(machine.via).font(.caption).foregroundStyle(.secondary)
                Text("\(machine.spaceCount) workspaces").font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            ring("CPU", machine.cpu)
            ring("MEM", machine.memory)
            if let gpu = machine.gpu { ring("GPU", gpu) }
        }
        .padding(.vertical, 4)
    }

    private func ring(_ label: String, _ value: Double) -> some View {
        Gauge(value: value) {
            Text(label)
        } currentValueLabel: {
            Text("\(Int(value * 100))").font(.caption.monospacedDigit())
        }
        .gaugeStyle(.accessoryCircularCapacity)
        .tint(value > 0.75 ? .orange : .green)
        .scaleEffect(0.85)
        .overlay(alignment: .bottom) { Text(label).font(.system(size: 9, weight: .semibold)).foregroundStyle(.secondary).offset(y: 12) }
    }
}

// MARK: D

struct SpacesFolded: View {
    var state: Int

    var body: some View {
        SpacesStack(state: state, title: "Spaces") {
            List {
                Section {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 10) {
                            ForEach(Array(SpaceSample.machines.enumerated()), id: \.element.id) { index, machine in
                                chip(machine, selected: index == 0)
                            }
                            Image(systemName: "plus").frame(width: 44, height: 70)
                                .background(.fill.tertiary, in: .rect(cornerRadius: 16))
                        }
                    }
                    .listRowInsets(EdgeInsets(top: 0, leading: 16, bottom: 0, trailing: 16))
                    .listRowBackground(Color.clear)
                }
                Section("Needs you") {
                    NavigationLink(value: 0) { spaceLine(SpaceSample.detail, repo: "herdr-gpui") }
                }
                Section("m4max") {
                    ForEach(SpaceSample.local.repos) { repo in
                        DisclosureGroup(isExpanded: .constant(repo.name == "herdr-gpui")) {
                            NavigationLink(value: 0) { spaceLine(repo.main, repo: nil) }
                            ForEach(repo.worktrees) { space in
                                NavigationLink(value: 0) { spaceLine(space, repo: nil) }
                            }
                        } label: {
                            HStack(spacing: 10) {
                                RepoAvatar(repo: repo, size: 26)
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(repo.name).font(.body.weight(.medium))
                                    Text(repo.worktrees.isEmpty ? repo.main.branch : "\(repo.worktrees.count + 1) workspaces")
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                marks(repo)
                            }
                        }
                    }
                }
            }
            .listStyle(.insetGrouped)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) { Button {} label: { Image(systemName: "magnifyingglass") } }
            }
        } detail: {
            SpaceDetailSheet()
        }
    }

    private func chip(_ machine: Machine, selected: Bool) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 6) {
                Image(systemName: machine.symbol)
                Text(machine.name).font(.subheadline.weight(.semibold))
                Circle().fill(.green).frame(width: 7, height: 7)
            }
            LoadReadout(label: "CPU", value: machine.cpu)
            LoadReadout(label: machine.gpu == nil ? "MEM" : "GPU", value: machine.gpu ?? machine.memory)
        }
        .padding(10)
        .background(.fill.tertiary, in: .rect(cornerRadius: 16))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.clear), lineWidth: 2))
    }

    private func marks(_ repo: Repo) -> some View {
        let all = [repo.main] + repo.worktrees
        return HStack(spacing: 3) {
            ForEach(Array(all.filter { $0.mark != .idle }.enumerated()), id: \.offset) { _, space in space.mark.view }
        }
    }

    private func spaceLine(_ space: Space, repo: String?) -> some View {
        HStack(spacing: 10) {
            space.mark.view.frame(width: 12)
            VStack(alignment: .leading, spacing: 1) {
                Text(space.name).lineLimit(1)
                Text(repo.map { "\($0) · \(space.branch)" } ?? space.branch)
                    .font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
            }
            Spacer(minLength: 4)
            if let pr = space.pr { PRBadge(pr: pr) }
        }
    }
}
