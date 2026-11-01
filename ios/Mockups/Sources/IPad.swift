import SwiftUI

// iPad (portrait: simctl cannot rotate a headless simulator). A: three columns (sections, attention list, chat B).
// B: two columns with the chat's terminal in an inspector beside it.
// C: the board as columns across the screen.

private struct Sidebar: View {
    var body: some View {
        List {
            Section {
                Label("Home", systemImage: "house")
                Label("Board", systemImage: "square.grid.2x2")
                Label("Search", systemImage: "magnifyingglass")
            }
            Section("Computers") {
                ForEach(Sample.computers) { computer in
                    HStack {
                        Label(computer.name, systemImage: computer.symbol)
                        Spacer()
                        if computer.needsYou > 0 {
                            Text("\(computer.needsYou)").font(.caption.bold()).foregroundStyle(.white)
                                .padding(.horizontal, 7).padding(.vertical, 1).background(.orange, in: .capsule)
                        }
                    }
                }
            }
        }
        .navigationTitle("Herdr")
    }
}

private struct AttentionColumn: View {
    var body: some View {
        List(selection: .constant(Sample.computers[0].workspaces[0].chats[0].id)) {
            Section("Needs you") {
                ForEach(Sample.allChats.filter { $0.chat.status == .needsYou }, id: \.chat.id) { item in
                    ChatRowView(chat: item.chat, label: item.computer.name)
                }
            }
            Section("Working") {
                ForEach(Sample.allChats.filter { $0.chat.status == .working }, id: \.chat.id) { item in
                    ChatRowView(chat: item.chat, label: item.computer.name).tag(item.chat.id)
                }
            }
            Section("Recent") {
                ForEach(Sample.allChats.filter { [.unread, .done, .idle].contains($0.chat.status) }, id: \.chat.id) { item in
                    ChatRowView(chat: item.chat, label: item.computer.name)
                }
            }
        }
        .navigationTitle("Home")
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { Button {} label: { Image(systemName: "plus") } }
        }
    }
}

struct IPadColumns: View {
    var body: some View {
        NavigationSplitView(columnVisibility: .constant(.all)) {
            Sidebar()
        } content: {
            AttentionColumn()
        } detail: {
            NavigationStack { ChatDocumentContent() }
        }
    }
}

struct IPadInspector: View {
    var body: some View {
        NavigationSplitView(columnVisibility: .constant(.all)) {
            AttentionColumn()
        } detail: {
            NavigationStack {
                ChatDocumentContent()
                    .inspector(isPresented: .constant(true)) {
                        VStack(spacing: 0) {
                            HStack {
                                Label("zsh — just ci", systemImage: "apple.terminal").font(.subheadline.weight(.semibold))
                                Spacer()
                                Image(systemName: "arrow.up.left.and.arrow.down.right")
                            }
                            .padding(12)
                            TerminalScreen(fontSize: 11)
                        }
                        .foregroundStyle(.white)
                        .background(Color(red: 0.07, green: 0.07, blue: 0.09))
                        .environment(\.colorScheme, .dark)
                        .inspectorColumnWidth(min: 320, ideal: 420)
                    }
            }
        }
    }
}

struct IPadBoard: View {
    var body: some View {
        NavigationStack {
            HStack(alignment: .top, spacing: 16) {
                BoardColumn(title: "Needs you", tint: .orange) {
                    ForEach(Sample.questions) { question in AnswerCard(question: question) }
                }
                BoardColumn(title: "Working", tint: .blue) {
                    ForEach(Sample.allChats.filter { $0.chat.status == .working }, id: \.chat.id) { item in
                        VStack(alignment: .leading, spacing: 8) {
                            ChatRowView(chat: item.chat, label: item.computer.name)
                            ProgressView(value: item.chat.progress ?? 0).tint(item.chat.agent.tint)
                            Text("Now: \(item.chat.detail)").font(.caption).foregroundStyle(.secondary)
                        }
                        .padding(14)
                        .background(.background.secondary, in: .rect(cornerRadius: 18))
                    }
                }
                BoardColumn(title: "Recent", tint: .secondary) {
                    ForEach(Sample.allChats.filter { [.unread, .done, .idle].contains($0.chat.status) }, id: \.chat.id) { item in
                        ChatRowView(chat: item.chat, label: item.computer.name)
                            .padding(12)
                            .background(.background.secondary, in: .rect(cornerRadius: 16))
                    }
                }
            }
            .padding(20)
            .frame(maxHeight: .infinity, alignment: .top)
            .background(Color(.systemGroupedBackground))
            .navigationTitle("Board")
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Picker("Computer", selection: .constant("All")) {
                        ForEach(["All", "MacBook Pro", "studio", "build-01"], id: \.self) { Text($0) }
                    }
                    .pickerStyle(.segmented)
                    .frame(width: 380)
                }
                ToolbarItem(placement: .topBarTrailing) { Button {} label: { Image(systemName: "plus") } }
            }
        }
    }
}

private struct BoardColumn<Content: View>: View {
    var title: String
    var tint: Color
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Circle().fill(tint).frame(width: 10, height: 10)
                Text(title).font(.title3.bold())
            }
            ScrollView { VStack(spacing: 12) { content } }
        }
        .frame(maxWidth: .infinity)
    }
}
