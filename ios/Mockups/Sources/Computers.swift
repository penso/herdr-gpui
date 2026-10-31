import SwiftUI

// The Computers tab of home B. A: one grouped section per computer with
// its health. B: a carousel of computer cards over the chosen one's
// workspaces. C: an outline from computer to workspace to tab.

enum HomeTab: Hashable { case home, board, computers, search }

/// Home B's tab bar with one tab chosen, so each mockup sits in context.
struct HomeTabs<Content: View>: View {
    var selected: HomeTab
    @ViewBuilder var content: Content

    var body: some View {
        TabView(selection: .constant(selected)) {
            Tab("Home", systemImage: "house", value: HomeTab.home) {
                if selected == .home { content } else { AttentionList() }
            }
            Tab("Board", systemImage: "square.grid.2x2", value: HomeTab.board) { Text("Board") }
            Tab("Computers", systemImage: "desktopcomputer", value: HomeTab.computers) {
                if selected == .computers { content } else { Text("Computers") }
            }
            Tab(value: HomeTab.search, role: .search) { Text("Search") }
        }
    }
}

extension Computer {
    var cpu: Double { name == "studio" ? 0.71 : name == "build-01" ? 0.12 : 0.34 }
    var memory: Double { name == "studio" ? 0.58 : name == "build-01" ? 0.22 : 0.66 }
    var isSSH: Bool { via.hasPrefix("SSH") }
}

private struct LoadBar: View {
    var label: String
    var value: Double

    var body: some View {
        HStack(spacing: 6) {
            Text(label).font(.caption2.weight(.semibold)).foregroundStyle(.secondary).frame(width: 28, alignment: .leading)
            Gauge(value: value) {}.gaugeStyle(.accessoryLinearCapacity)
                .tint(value > 0.7 ? .orange : .green)
            Text("\(Int(value * 100))%").font(.caption2.monospacedDigit()).foregroundStyle(.secondary)
                .frame(width: 32, alignment: .trailing)
        }
    }
}

private struct Counts: View {
    var computer: Computer

    var body: some View {
        HStack(spacing: 6) {
            if computer.needsYou > 0 {
                Text("\(computer.needsYou) need you").font(.caption.weight(.semibold)).foregroundStyle(.white)
                    .padding(.horizontal, 8).padding(.vertical, 2).background(.orange, in: .capsule)
            }
            if computer.working > 0 {
                Text("\(computer.working) working").font(.caption.weight(.semibold)).foregroundStyle(.white)
                    .padding(.horizontal, 8).padding(.vertical, 2).background(.blue, in: .capsule)
            }
        }
    }
}

struct ComputersGrouped: View {
    var body: some View {
        HomeTabs(selected: .computers) {
            NavigationStack {
                List {
                    ForEach(Sample.computers) { computer in
                        Section {
                            VStack(alignment: .leading, spacing: 10) {
                                HStack(spacing: 12) {
                                    Image(systemName: computer.symbol).font(.title2)
                                        .frame(width: 48, height: 48)
                                        .background(.fill.tertiary, in: .rect(cornerRadius: 12))
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(computer.name).font(.headline)
                                        Label(computer.via, systemImage: computer.isSSH ? "arrow.triangle.branch" : "checkmark.circle.fill")
                                            .font(.caption).foregroundStyle(computer.isSSH ? Color.secondary : Color.green)
                                    }
                                    Spacer()
                                }
                                LoadBar(label: "CPU", value: computer.cpu)
                                LoadBar(label: "MEM", value: computer.memory)
                            }
                            .padding(.vertical, 4)
                            ForEach(computer.workspaces) { workspace in
                                NavigationLink {} label: {
                                    HStack {
                                        Image(systemName: "folder").foregroundStyle(.secondary)
                                        VStack(alignment: .leading, spacing: 1) {
                                            Text(workspace.name)
                                            Text(workspace.branch).font(.caption.monospaced()).foregroundStyle(.secondary)
                                        }
                                        Spacer()
                                        ForEach(workspace.chats.prefix(3)) { chat in
                                            AgentIcon(agent: chat.agent, size: 20)
                                        }
                                    }
                                }
                            }
                        } footer: {
                            Counts(computer: computer)
                        }
                    }
                }
                .navigationTitle("Computers")
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button {} label: { Image(systemName: "plus") }
                    }
                }
            }
        }
    }
}

struct ComputersCarousel: View {
    var body: some View {
        HomeTabs(selected: .computers) {
            NavigationStack {
                ScrollView {
                    VStack(alignment: .leading, spacing: 18) {
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 12) {
                                ForEach(Array(Sample.computers.enumerated()), id: \.element.id) { index, computer in
                                    ComputerCard(computer: computer, selected: index == 0)
                                }
                                VStack(spacing: 8) {
                                    Image(systemName: "plus").font(.title2)
                                    Text("Add").font(.caption)
                                }
                                .frame(width: 90, height: 190)
                                .glassEffect(.regular.interactive(), in: .rect(cornerRadius: 26))
                            }
                            .padding(.horizontal, 16)
                            .scrollTargetLayout()
                        }
                        .scrollTargetBehavior(.viewAligned)
                        Text("On MacBook Pro").font(.title3.bold()).padding(.horizontal, 16)
                        ForEach(Sample.computers[0].workspaces) { workspace in
                            VStack(alignment: .leading, spacing: 0) {
                                HStack {
                                    Text(workspace.name).font(.headline)
                                    Text(workspace.branch).font(.caption.monospaced()).foregroundStyle(.secondary)
                                    Spacer()
                                    Image(systemName: "plus.circle").foregroundStyle(.tint)
                                }
                                .padding(.horizontal, 14).padding(.top, 12).padding(.bottom, 4)
                                ForEach(workspace.chats) { chat in
                                    ChatRowView(chat: chat).padding(.horizontal, 14).padding(.vertical, 6)
                                }
                            }
                            .padding(.bottom, 6)
                            .background(.background.secondary, in: .rect(cornerRadius: 20))
                            .padding(.horizontal, 16)
                        }
                    }
                }
                .background(Color(.systemGroupedBackground))
                .navigationTitle("Computers")
            }
        }
    }
}

private struct ComputerCard: View {
    var computer: Computer
    var selected: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Image(systemName: computer.symbol).font(.title)
                Spacer()
                Circle().fill(computer.isSSH ? .secondary : Color.green).frame(width: 8, height: 8)
            }
            VStack(alignment: .leading, spacing: 1) {
                Text(computer.name).font(.headline)
                Text(computer.via).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            LoadBar(label: "CPU", value: computer.cpu)
            LoadBar(label: "MEM", value: computer.memory)
            Counts(computer: computer)
        }
        .padding(16)
        .frame(width: 240, height: 190, alignment: .topLeading)
        .background(.background.secondary, in: .rect(cornerRadius: 26))
        .overlay(RoundedRectangle(cornerRadius: 26).strokeBorder(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.clear), lineWidth: 2))
    }
}

struct ComputersTree: View {
    var body: some View {
        HomeTabs(selected: .computers) {
            NavigationStack {
                List {
                    ForEach(Sample.computers) { computer in
                        DisclosureGroup(isExpanded: .constant(!computer.isSSH)) {
                            ForEach(computer.workspaces) { workspace in
                                DisclosureGroup(isExpanded: .constant(workspace.name != "moltis")) {
                                    ForEach(workspace.chats) { chat in
                                        ChatRowView(chat: chat, showsDetail: false)
                                    }
                                    Label("New chat", systemImage: "plus").foregroundStyle(.tint)
                                } label: {
                                    Label {
                                        HStack {
                                            Text(workspace.name)
                                            Text(workspace.branch).font(.caption.monospaced()).foregroundStyle(.secondary)
                                        }
                                    } icon: {
                                        Image(systemName: "folder")
                                    }
                                }
                            }
                        } label: {
                            HStack {
                                Label(computer.name, systemImage: computer.symbol).font(.headline)
                                Spacer()
                                Text("\(Int(computer.cpu * 100))%").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                                Counts(computer: computer)
                            }
                        }
                    }
                }
                .listStyle(.sidebar)
                .navigationTitle("Computers")
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button {} label: { Image(systemName: "plus") }
                    }
                }
            }
        }
    }
}
