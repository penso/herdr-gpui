import HerdrRemoteCore
import SwiftUI

/// Herdr Remote on the wrist: what needs you, answered with a tap or a
/// dictated reply, and what is running. Everything goes through the phone.
@main
struct HerdrRemoteWatchApp: App {
    @State private var model = WatchModel()

    var body: some Scene {
        WindowGroup {
            NavigationStack { ChatList() }
                .environment(model)
                .task { model.start() }
        }
    }
}

private struct ChatList: View {
    @Environment(WatchModel.self) private var model

    var body: some View {
        let chats = model.summary.ranked
        List {
            if chats.isEmpty {
                Text(model.summary.updated == .distantPast ? "Open Herdr Remote on your iPhone." : "Nothing running.")
                    .foregroundStyle(.secondary)
            }
            let waiting = chats.filter(\.needsYou)
            if !waiting.isEmpty {
                Section("Needs you") {
                    ForEach(waiting) { chat in NavigationLink(value: chat) { ChatRow(chat: chat) } }
                }
            }
            let working = chats.filter { $0.working && !$0.needsYou }
            if !working.isEmpty {
                Section("Running") {
                    ForEach(working) { chat in NavigationLink(value: chat) { ChatRow(chat: chat) } }
                }
            }
        }
        .navigationTitle("Herdr")
        .navigationDestination(for: WidgetSummary.Chat.self) { ChatDetail(chat: $0) }
        #if DEBUG
        // The simulator loop: `-openFirst YES` opens the first chat.
        .navigationDestination(isPresented: .constant(UserDefaults.standard.bool(forKey: "openFirst") && !chats.isEmpty)) {
            if let first = chats.first { ChatDetail(chat: first) }
        }
        #endif
    }
}

private struct ChatRow: View {
    let chat: WidgetSummary.Chat

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 4) {
                Circle().fill(chat.needsYou ? Color.orange : Color.green).frame(width: 6, height: 6)
                Text(chat.title).font(.headline).lineLimit(2)
            }
            Text("\(chat.computer) · \(chat.workspace)").font(.caption2).foregroundStyle(.secondary).lineLimit(1)
        }
    }
}

/// One chat: its question with the answers the Watch may give, and a reply.
private struct ChatDetail: View {
    @Environment(WatchModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let chat: WidgetSummary.Chat
    @State private var reply = ""
    @State private var problem: String?
    @State private var sending = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                Text(chat.title).font(.headline)
                if let question = chat.question {
                    Text(question.title).font(.footnote)
                    ForEach(question.options.filter { WatchRequest.answersOnWatch($0.kind) }, id: \.id) { option in
                        Button(option.name) {
                            send(.answer(computer: chat.computerId, pane: chat.pane, permission: question.permission, option: option.id))
                        }
                        .tint(option.kind.allows ? .orange : .gray)
                    }
                    if question.options.contains(where: { !WatchRequest.answersOnWatch($0.kind) }) {
                        Text("“Always” answers are on your iPhone.").font(.caption2).foregroundStyle(.secondary)
                    }
                }
                TextField("Reply", text: $reply)
                    .onSubmit {
                        guard !reply.isEmpty else { return }
                        send(.reply(computer: chat.computerId, pane: chat.pane, text: reply))
                    }
                if sending { ProgressView() }
                if let problem { Text(problem).font(.caption2).foregroundStyle(.red) }
            }
        }
        .navigationTitle(chat.computer)
        .disabled(sending)
    }

    private func send(_ request: WatchRequest) {
        sending = true
        Task {
            problem = await model.send(request)
            sending = false
            if problem == nil { dismiss() }
        }
    }
}
