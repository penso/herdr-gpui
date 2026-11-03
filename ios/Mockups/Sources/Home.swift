import SwiftUI

// Home. A: the picked web layout (F) made native, every computer merged.
// B: attention first, with inline answers and a tab bar. C: one list
// sorted by attention, filtered by computer.

struct HomeGrouped: View {
    var body: some View {
        NavigationStack {
            List {
                Section {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 10) {
                            ForEach(Sample.questions) { question in
                                QuestionChip(question: question)
                            }
                        }
                        .padding(.horizontal, 16)
                    }
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(Color.clear)
                } header: {
                    Text("Needs you")
                }
                ForEach(Sample.computers) { computer in
                    Section {
                        ForEach(computer.workspaces) { workspace in
                            WorkspaceHeaderRow(workspace: workspace)
                            ForEach(workspace.chats) { chat in
                                ChatRowView(chat: chat).padding(.leading, 8)
                            }
                        }
                    } header: {
                        ComputerHeader(computer: computer)
                    }
                }
            }
            .listStyle(.insetGrouped)
            .navigationTitle("Herdr")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {} label: { Image(systemName: "magnifyingglass") }
                }
                ToolbarItemGroup(placement: .bottomBar) {
                    Button {} label: { Image(systemName: "gearshape") }
                    Button {} label: { Image(systemName: "eye") }
                }
                ToolbarSpacer(.flexible, placement: .bottomBar)
                ToolbarItem(placement: .bottomBar) {
                    Button {} label: { Label("New workspace", systemImage: "plus") }
                        .buttonStyle(.glassProminent)
                }
            }
        }
    }
}

struct QuestionChip: View {
    var question: Question

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                AgentIcon(agent: question.agent, size: 20)
                Text(question.chat).font(.caption.weight(.semibold)).lineLimit(1)
            }
            Text(question.prompt).font(.subheadline).lineLimit(2)
            Text("\(question.computer) · \(question.workspace)").font(.caption2).foregroundStyle(.secondary)
        }
        .frame(width: 220, alignment: .leading)
        .padding(12)
        .glassEffect(.regular.tint(.orange.opacity(0.18)), in: .rect(cornerRadius: 18))
    }
}

struct ComputerHeader: View {
    var computer: Computer

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: computer.symbol)
            VStack(alignment: .leading, spacing: 0) {
                Text(computer.name).font(.headline).foregroundStyle(.primary)
                Text(computer.via).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Image(systemName: "chevron.down").font(.caption.weight(.semibold))
        }
        .textCase(nil)
    }
}

struct WorkspaceHeaderRow: View {
    var workspace: Workspace

    var body: some View {
        HStack {
            Image(systemName: "folder").foregroundStyle(.secondary)
            Text(workspace.name).font(.subheadline.weight(.semibold))
            Text(workspace.branch).font(.caption.monospaced()).foregroundStyle(.secondary)
            Spacer()
            Image(systemName: "plus.circle").foregroundStyle(.tint)
        }
    }
}

struct HomeAttention: View {
    var body: some View {
        TabView {
            Tab("Home", systemImage: "house") { AttentionList() }
            Tab("Board", systemImage: "square.grid.2x2") { Text("Board") }
            Tab("Computers", systemImage: "desktopcomputer") { Text("Computers") }
            Tab(role: .search) { Text("Search") }
        }
    }
}

struct AttentionList: View {
    var body: some View {
        NavigationStack {
            ScrollView {
                AttentionBody()
                    .padding(.horizontal, 16)
            }
            .background(Color(.systemGroupedBackground))
            .navigationTitle("Herdr")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {} label: { Image(systemName: "plus") }
                }
            }
        }
    }
}

/// Home B's sections, for mockups that change what sits around them.
struct AttentionBody: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            SectionTitle(title: "Needs you", count: 2, tint: .orange)
            ForEach(Sample.questions) { question in
                AnswerCard(question: question)
            }
            SectionTitle(title: "Working", count: 2, tint: .blue)
            ForEach(Sample.allChats.filter { $0.chat.status == .working }, id: \.chat.id) { item in
                WorkingRow(computer: item.computer, workspace: item.workspace, chat: item.chat)
            }
            SectionTitle(title: "Recent", count: nil, tint: .secondary)
            VStack(spacing: 0) {
                ForEach(Sample.allChats.filter { [.unread, .done, .idle].contains($0.chat.status) }.prefix(4), id: \.chat.id) { item in
                    ChatRowView(chat: item.chat, label: item.computer.name)
                        .padding(.horizontal, 14).padding(.vertical, 8)
                    Divider().padding(.leading, 54)
                }
            }
            .background(.background.secondary, in: .rect(cornerRadius: 18))
        }
    }
}

private struct SectionTitle: View {
    var title: String
    var count: Int?
    var tint: Color

    var body: some View {
        HStack {
            Text(title).font(.title3.bold())
            if let count {
                Text("\(count)").font(.subheadline.bold()).foregroundStyle(.white)
                    .padding(.horizontal, 8).padding(.vertical, 2).background(tint, in: .capsule)
            }
        }
        .padding(.top, 8)
    }
}

struct AnswerCard: View {
    var question: Question

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                AgentIcon(agent: question.agent, size: 24)
                VStack(alignment: .leading, spacing: 0) {
                    Text(question.chat).font(.subheadline.weight(.semibold))
                    Text("\(question.computer) · \(question.workspace)").font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Text("2m").font(.caption).foregroundStyle(.secondary)
            }
            Text(question.prompt).font(.body)
            if question.isPermission {
                HStack {
                    Button("Allow once") {}.buttonStyle(.glassProminent).tint(.orange)
                    Button("Deny") {}.buttonStyle(.glass)
                    Spacer()
                    Button("Always…") {}.font(.footnote).buttonStyle(.borderless)
                }
            } else {
                VStack(spacing: 6) {
                    ForEach(question.options, id: \.self) { option in
                        Button {} label: { Text(option).frame(maxWidth: .infinity) }
                            .buttonStyle(.glass)
                    }
                }
            }
        }
        .padding(14)
        .background(.background.secondary, in: .rect(cornerRadius: 22))
        .overlay(RoundedRectangle(cornerRadius: 22).strokeBorder(.orange.opacity(0.35)))
    }
}

private struct WorkingRow: View {
    var computer: Computer
    var workspace: Workspace
    var chat: Chat

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ChatRowView(chat: chat, label: computer.name)
            ProgressView(value: chat.progress ?? 0).tint(chat.agent.tint)
        }
        .padding(14)
        .background(.background.secondary, in: .rect(cornerRadius: 18))
    }
}

struct HomeFiltered: View {
    private let filters = ["All", "MacBook Pro", "studio", "build-01"]

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ScrollView(.horizontal, showsIndicators: false) {
                        GlassEffectContainer {
                            HStack(spacing: 8) {
                                ForEach(filters, id: \.self) { filter in
                                    Text(filter).font(.subheadline.weight(.medium))
                                        .padding(.horizontal, 14).padding(.vertical, 8)
                                        .glassEffect(filter == "All" ? .regular.tint(.accentColor).interactive() : .regular.interactive(), in: .capsule)
                                        .foregroundStyle(filter == "All" ? .white : .primary)
                                }
                            }
                            .padding(.horizontal, 16)
                        }
                    }
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(Color.clear)
                }
                Section("Needs you") {
                    ForEach(Sample.allChats.filter { $0.chat.status == .needsYou }, id: \.chat.id) { item in
                        ChatRowView(chat: item.chat, label: "\(item.computer.name) · \(item.workspace.name)")
                            .swipeActions { Button("Deny", role: .destructive) {}; Button("Allow") {}.tint(.orange) }
                    }
                }
                Section("Working") {
                    ForEach(Sample.allChats.filter { $0.chat.status == .working }, id: \.chat.id) { item in
                        ChatRowView(chat: item.chat, label: "\(item.computer.name) · \(item.workspace.name)")
                    }
                }
                Section("Everything else") {
                    ForEach(Sample.allChats.filter { [.unread, .done, .idle].contains($0.chat.status) }, id: \.chat.id) { item in
                        ChatRowView(chat: item.chat, label: "\(item.computer.name) · \(item.workspace.name)")
                    }
                }
            }
            .navigationTitle("Herdr")
            .navigationSubtitle("3 computers · 2 need you")
            .searchable(text: .constant(""), prompt: "Chats, workspaces, computers")
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button {} label: { Image(systemName: "gearshape") }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button {} label: { Image(systemName: "plus") }
                }
            }
        }
    }
}
