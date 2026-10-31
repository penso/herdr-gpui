import HerdrRemoteCore
import SwiftUI
import UIKit

/// Chat B: the latest turn as a document (the prompt as a heading, steps
/// folded to one line, the answer, then its diffs folded to a line each),
/// the running step pinned under the header, earlier turns summed up, and
/// questions in a bottom sheet.
struct ChatScreen: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    let title: String
    @State private var store: ChatStore?
    @State private var showingSession = false
    @Environment(\.horizontalSizeClass) private var width
    @Environment(\.dismiss) private var dismiss

    /// The chat's name as the computer last sent it, so a rename shows at
    /// once; the route's title until the home arrives.
    private var name: String {
        model.fleet.chatItems.first { $0.key == key }?.chat.title ?? title
    }

    var body: some View {
        Group {
            if let store, let transcript = store.transcript {
                ChatDocument(key: key, store: store, transcript: model.fleet.chat(transcript, for: key))
            } else {
                ProgressView("Loading the chat…")
            }
        }
        // The principal item below draws the header; the title is what the
        // iPad's split view and the window's name fall back to.
        // Window titles show in Stage Manager and the app switcher.
        .navigationTitle(LockState.shared.hidesNames ? "Herdr Remote" : name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(removing: width == .regular ? .title : nil)
        .toolbar(.hidden, for: .tabBar)
        .toolbar {
            // On an iPad the bar's middle also spans the terminal
            // inspector, so the header leads there instead.
            ToolbarItem(placement: width == .regular ? .topBarLeading : .principal) {
                Button { showingSession = store != nil } label: {
                    VStack(spacing: 0) {
                        HStack(spacing: 4) {
                            Text(name).font(.headline).lineLimit(1)
                            Image(systemName: "chevron.down.circle.fill").font(.caption).foregroundStyle(.tertiary)
                        }
                        if let line = store?.transcript.map({ sessionLine(model.fleet.chat($0, for: key)) }), !line.isEmpty {
                            Text(line).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                        }
                    }
                }
                .buttonStyle(.plain)
                .frame(minWidth: width == .regular ? 280 : 0, maxWidth: 360, alignment: width == .regular ? .leading : .center)
                .accessibilityHint("Shows the session's settings")
            }
            if let usage = store?.transcript?.usage {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { showingSession = true } label: { ContextRing(percent: usage.percent) }
                }
            }
        }
        .sheet(isPresented: $showingSession) {
            if let store {
                SessionSheet(key: key, store: store, name: name) {
                    showingSession = false
                    dismiss()
                }
                .presentationDetents([.medium, .large])
            }
        }
        .onAppear {
            store = model.fleet.openChat(key)
            #if DEBUG
            // The simulator loop's `-session YES` opens the session sheet.
            if UserDefaults.standard.bool(forKey: "session") { showingSession = true }
            #endif
        }
        .onDisappear {
            if let store { model.fleet.closeChat(store) }
        }
    }

    /// "Opus 5.5 · High · Bypass permissions", from the session's settings.
    private func sessionLine(_ transcript: Transcript) -> String {
        let model = transcript.options.first { $0.category == .model }.map(currentName)
        let effort = transcript.options.first { $0.category == .thoughtLevel }.map(currentName)
        let mode = transcript.modes.flatMap { modes in modes.available.first { $0.id == modes.current }?.name }
        return [model, effort, mode].compactMap { $0 }.joined(separator: " · ")
    }

    private func currentName(_ option: ConfigOption) -> String {
        switch option.kind {
        case let .select(choices, current): choices.first { $0.id == current }?.name ?? current
        case let .toggle(current): current ? "On" : "Off"
        }
    }
}

struct ContextRing: View {
    var percent: Int
    var size: CGFloat = 20
    var lineWidth: CGFloat = 3

    var body: some View {
        ZStack {
            Circle().stroke(.quaternary, lineWidth: lineWidth)
            Circle().trim(from: 0, to: Double(percent) / 100)
                .stroke(percent > 80 ? AnyShapeStyle(.orange) : AnyShapeStyle(.tint), style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                .rotationEffect(.degrees(-90))
        }
        .frame(width: size, height: size)
        .accessibilityLabel("Context \(percent) percent full")
    }
}

private struct ChatDocument: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    let store: ChatStore
    let transcript: Transcript
    @State private var problem: String?
    @State private var dismissedQuestion: PermissionId?
    /// A link the agent wrote, shown in full before it opens.
    @State private var pendingLink: URL?

    /// Lines stay this wide at most on an iPad or an unfolded iPhone, so
    /// the answer reads like a page rather than a banner.
    static let readableWidth: CGFloat = 680

    var body: some View {
        let entries = transcript.entries
        let start = entries.lastIndex { if case .user = $0.kind { true } else { false } } ?? 0
        let earlier = entries[..<start]
        let latest = entries[start...]
        let tools = latest.compactMap { if case let .tool(tool) = $0.kind { tool } else { nil } }
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                if !earlier.isEmpty || transcript.truncated {
                    DisclosureGroup {
                        LazyVStack(alignment: .leading, spacing: 12) {
                            ForEach(Array(earlier)) { EntryView(entry: $0) }
                        }
                    } label: {
                        let turns = earlier.filter { if case .user = $0.kind { true } else { false } }.count
                        Text("Earlier in this chat · \(turns) turn\(turns == 1 ? "" : "s")")
                            .font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
                    }
                }
                if case let .user(text, _) = latest.first?.kind {
                    HStack(alignment: .top, spacing: 10) {
                        Rectangle().fill(.tint).frame(width: 3)
                        Text(text).font(.title3.weight(.semibold)).textSelection(.enabled)
                    }
                    .fixedSize(horizontal: false, vertical: true)
                }
                if !tools.isEmpty {
                    DisclosureGroup {
                        VStack(alignment: .leading, spacing: 8) {
                            ForEach(tools, id: \.callId) { ToolRow(tool: $0) }
                        }
                        .padding(.top, 6)
                    } label: {
                        Label(stepsLine(tools), systemImage: "list.bullet.indent")
                            .font(.subheadline).foregroundStyle(.secondary)
                    }
                    Divider()
                }
                ForEach(Array(latest.dropFirst())) { entry in
                    if case .tool = entry.kind {} else { EntryView(entry: entry) }
                }
                ForEach(tools.flatMap(diffs), id: \.path) { diff in
                    DiffBlock(path: diff.path, old: diff.old, new: diff.new)
                }
                if !transcript.queued.isEmpty {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Queued").font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
                        ForEach(transcript.queued, id: \.id) { queued in
                            HStack(alignment: .firstTextBaseline, spacing: 8) {
                                Image(systemName: "clock").foregroundStyle(.secondary)
                                Text(queued.text).font(.subheadline).lineLimit(3)
                                if !queued.attachments.isEmpty {
                                    Label("\(queued.attachments.count)", systemImage: "photo").font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Button { unqueue(queued) } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary) }
                                    .buttonStyle(.plain)
                                    .accessibilityLabel("Remove from the queue")
                            }
                            .padding(10)
                            .background(.fill.quinary, in: .rect(cornerRadius: 12))
                        }
                    }
                }
                ForEach(store.outgoing) { outgoing in
                    OutgoingRow(outgoing: outgoing, computer: computerName, agent: transcript.agent)
                }
                if let problem { Text(problem).font(.footnote).foregroundStyle(.red) }
            }
            .animation(.snappy, value: store.outgoing)
            .frame(maxWidth: Self.readableWidth, alignment: .leading)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 20)
            .padding(.top, 16)
            .padding(.bottom, 24)
        }
        .defaultScrollAnchor(.bottom)
        .opacity(store.stale ? 0.6 : 1)
        .safeAreaInset(edge: .bottom) {
            // The running turn's status sits where the turn grows, just
            // above the composer, not over the earlier answers at the top.
            VStack(spacing: 8) {
                if transcript.status == .working {
                    RunningCapsule(transcript: transcript, tools: tools)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
                Composer(key: key, store: store, transcript: transcript, problem: $problem)
                    .frame(maxWidth: Self.readableWidth + 40)
                    .frame(maxWidth: .infinity)
            }
            .animation(.snappy, value: transcript.status == .working)
        }
        .environment(\.openURL, OpenURLAction { url in
            guard MarkdownView.webSchemes.contains(url.scheme?.lowercased() ?? "") else { return .discarded }
            pendingLink = url
            return .handled
        })
        .confirmationDialog(
            "Open this link?",
            isPresented: Binding(get: { pendingLink != nil }, set: { if !$0 { pendingLink = nil } }),
            titleVisibility: .visible,
            presenting: pendingLink
        ) { url in
            Button("Open in Safari") { UIApplication.shared.open(url) }
        } message: { url in
            Text(url.absoluteString)
        }
        .sheet(item: question) { permission in
            QuestionSheet(key: key, permission: permission)
                .presentationDetents([.medium, .large])
        }
    }

    private var computerName: String {
        model.fleet.machines.first { $0.id == key.device }?.computer.name ?? "your computer"
    }

    /// The oldest question, shown until answered or swiped away.
    private var question: Binding<Permission?> {
        Binding(
            get: { transcript.permissions.first { $0.id != dismissedQuestion } },
            set: { if $0 == nil { dismissedQuestion = transcript.permissions.first?.id } }
        )
    }

    private func stepsLine(_ tools: [Tool]) -> String {
        let edited = Set(tools.filter { $0.kind == .edit }.flatMap(\.locations)).count
        let steps = "\(tools.count) step\(tools.count == 1 ? "" : "s")"
        return edited > 0 ? "\(steps) · \(edited) file\(edited == 1 ? "" : "s") changed" : steps
    }

    private func diffs(_ tool: Tool) -> [(path: String, old: String?, new: String)] {
        tool.content.compactMap { if case let .diff(path, old, new) = $0 { (path, old, new) } else { nil } }
    }

    private func unqueue(_ queued: Queued) {
        Task { problem = await model.fleet.send(.unqueue(queued.id), to: key) }
    }
}

/// A prompt on its way, until the transcript shows it as a turn or queued.
private struct OutgoingRow: View {
    let outgoing: Outgoing
    let computer: String
    let agent: String

    var body: some View {
        VStack(alignment: .trailing, spacing: 4) {
            VStack(alignment: .leading, spacing: 6) {
                if !outgoing.text.isEmpty { Text(outgoing.text).lineLimit(6) }
                let pictures = outgoing.uploads.count
                if pictures > 0 {
                    Label("\(pictures) picture\(pictures == 1 ? "" : "s")", systemImage: "photo")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 14).padding(.vertical, 10)
            .background(.tint.opacity(0.18), in: .rect(cornerRadius: 16))
            ThinProgress(value: outgoing.progress).frame(width: 140)
            Text(stageLine).font(.caption).foregroundStyle(.secondary).monospacedDigit()
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
        .transition(.move(edge: .bottom).combined(with: .opacity))
        .accessibilityElement(children: .combine)
    }

    private var stageLine: String {
        switch outgoing.stage {
        case .uploading:
            let pictures = outgoing.uploads.count == 1 ? "picture" : "pictures"
            return "Uploading \(pictures)… \(Int((outgoing.progress / 0.8 * 100).rounded()))%"
        case .toComputer: return "Sending to \(computer)…"
        case .toAgent: return "Sending to \(agent)…"
        }
    }
}

/// What the agent is doing now, pinned above the composer.
private struct RunningCapsule: View {
    let transcript: Transcript
    let tools: [Tool]

    var body: some View {
        let current = tools.last { $0.status == .inProgress } ?? tools.last { $0.status == .pending }
        HStack(spacing: 8) {
            ProgressView().controlSize(.small)
            Text(current?.title ?? "Thinking").font(.subheadline.weight(.medium)).lineLimit(1)
            Spacer()
            if let started = transcript.turn?.startedMs {
                Text(timerInterval: Date(timeIntervalSince1970: Double(started) / 1000)...Date.distantFuture, countsDown: false)
                    .font(.subheadline.monospacedDigit()).foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 9)
        .glassEffect(.regular, in: .capsule)
        .frame(maxWidth: ChatDocument.readableWidth)
        .padding(.horizontal, 16)
    }
}

/// Questions A: a bottom sheet in thumb reach, full-width options, the
/// "always" ones set apart.
private struct QuestionSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let key: PaneKey
    let permission: Permission

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(permission.title).font(.title3.weight(.semibold))
            let always = permission.options.filter { $0.kind == .allowAlways || $0.kind == .rejectAlways }
            ForEach(permission.options.filter { !always.contains($0) }) { option in
                if option.kind == .allowOnce {
                    Button { answer(option) } label: { Text(option.name).frame(maxWidth: .infinity) }
                        .buttonStyle(.glassProminent).controlSize(.large).tint(Home.AgentStatus.blocked.color)
                } else {
                    Button { answer(option) } label: { Text(option.name).frame(maxWidth: .infinity) }
                        .buttonStyle(.glass).controlSize(.large)
                }
            }
            if !always.isEmpty {
                Divider()
                ForEach(always) { option in
                    Button(option.name) { answer(option) }.font(.subheadline)
                }
            }
        }
        .padding(24)
    }

    /// The sheet leaves once the answer is allowed; a refused answer
    /// brings the question back, and the sync bar says why.
    private func answer(_ option: PermissionOption) {
        Task {
            guard await OwnerCheck.allows(option) else { return }
            let fleet = model.fleet
            Task { _ = await fleet.send(.answer(permission: permission.id, option: option.id), to: key) }
            dismiss()
        }
    }
}
