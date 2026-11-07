import SheprdRemoteCore
import SwiftUI

/// Voice A: the sentence above the plan. What was understood is colored
/// in the sentence and fills its row as it is said; the same card is then
/// what is confirmed, by tap, and can be changed by tap or by voice.
struct VoiceSheet: View {
    @Environment(AppModel.self) private var model
    @Bindable var session: VoiceSession

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            content
        }
        .padding(20)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .presentationDetents([.fraction(0.7), .large])
        .presentationDragIndicator(.visible)
        .animation(.snappy, value: session.phase)
        .animation(.snappy, value: session.draft)
        .sensoryFeedback(.start, trigger: session.phase == .listening)
        .onDisappear { session.cancel() }
    }

    @ViewBuilder private var content: some View {
        switch session.phase {
        case let .done(done):
            VoiceDoneView(done: done) { model.voice = nil } open: { route in
                model.voice = nil
                model.pendingRoute = route
            }
        case let .failed(reason):
            VoiceProblem(title: "That did not work", detail: reason, session: session)
        case let .answered(words):
            VoiceAnswered(words: words, waiting: session.waiting)
        default:
            // Scrolls rather than truncates: the sentence is what is checked.
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    VoiceSaid(draft: session.draft)
                    if !session.reply.isEmpty {
                        Text(session.reply).font(.title2.weight(.semibold))
                    }
                    plan
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollBounceBehavior(.basedOnSize)
            footer
        }
    }

    @ViewBuilder private var plan: some View {
        switch session.plan {
        case let .workspace(plan):
            VoiceWorkspaceCard(plan: plan, session: session)
        case let .chat(plan):
            VoiceChatCard(plan: plan)
        case let .fanOut(plan):
            VoiceFanOutCard(plan: plan)
        case .needsMe, .unknown:
            EmptyView()
        }
    }

    @ViewBuilder private var footer: some View {
        switch session.phase {
        case .preparing, .listening:
            HStack {
                VoiceWave(levels: session.listener.levels)
                Spacer()
                if session.awaitingYes { Text("Say yes to start").font(.headline) }
                Spacer()
                Button("Stop listening", systemImage: "stop.fill") { session.stopListening() }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.glassProminent)
                    .buttonBorderShape(.circle)
                    .controlSize(.large)
                    .disabled(session.phase != .listening)
            }
        case .thinking:
            ProgressView().frame(maxWidth: .infinity, minHeight: 44)
        case .running:
            VoiceRunning(since: session.runningSince) { model.voice = nil }
        default:
            VoiceConfirm(session: session) { model.voice = nil }
        }
    }
}

/// The sentence so far, with what was understood in color.
private struct VoiceSaid: View {
    let draft: VoiceDraft

    var body: some View {
        if draft.text.isEmpty {
            Text("Try “New worktree on studio about the login bug, and ask Claude to fix it.”")
                .font(.title2)
                .foregroundStyle(.secondary)
        } else {
            Text(colored).font(.title2).textSelection(.enabled)
        }
    }

    private var colored: AttributedString {
        var whole = AttributedString()
        var position = draft.text.startIndex
        for mark in draft.marks.sorted(by: { $0.range.lowerBound < $1.range.lowerBound }) {
            guard mark.range.lowerBound >= position, mark.range.upperBound <= draft.text.endIndex else { continue }
            whole.append(AttributedString(draft.text[position..<mark.range.lowerBound]))
            var piece = AttributedString(draft.text[mark.range])
            piece.foregroundColor = mark.slot.tint
            piece.font = .title2.weight(.semibold)
            whole.append(piece)
            position = mark.range.upperBound
        }
        whole.append(AttributedString(draft.text[position...]))
        return whole
    }
}

extension VoiceSlot {
    var tint: Color {
        switch self {
        case .agent: .orange
        case .chat, .computer: .blue
        case .project: .teal
        case .task: .green
        case .topic: .purple
        }
    }
}

/// The microphone's level, moving as the person speaks.
private struct VoiceWave: View {
    let levels: [Float]

    var body: some View {
        HStack(spacing: 3) {
            ForEach(levels.indices, id: \.self) { index in
                Capsule().fill(Color.accentColor.gradient)
                    .frame(width: 3, height: max(4, 28 * CGFloat(levels[index])))
            }
        }
        .frame(height: 28)
        .accessibilityHidden(true)
    }
}

/// One detail of the plan, with why it has that value.
private struct VoiceRow<Choices: View>: View {
    let symbol: String
    let label: String
    let value: String?
    var detail: String?
    var tint: Color = .secondary
    var asking = false
    @ViewBuilder var choices: Choices

    var body: some View {
        Menu {
            choices
        } label: {
            HStack(spacing: 12) {
                Image(systemName: symbol)
                    .foregroundStyle(value == nil ? AnyShapeStyle(.tertiary) : AnyShapeStyle(tint))
                    .frame(width: 26)
                VStack(alignment: .leading, spacing: 1) {
                    Text(label).font(.caption).foregroundStyle(.secondary)
                    Text(value ?? (asking ? "Which one?" : "Listening…"))
                        .font(value == nil ? .body : .body.weight(.medium))
                        .foregroundStyle(value != nil ? AnyShapeStyle(.primary) : asking ? AnyShapeStyle(.orange) : AnyShapeStyle(.tertiary))
                }
                Spacer(minLength: 8)
                if value != nil {
                    if let detail { Text(detail).font(.caption).foregroundStyle(.secondary) }
                    Image(systemName: "chevron.up.chevron.down").font(.caption2).foregroundStyle(.tertiary)
                }
            }
            .padding(.vertical, 7)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
    }
}

/// A line of text in the plan, edited in place.
private struct VoiceTextRow: View {
    let symbol: String
    let label: String
    let placeholder: String
    let tint: Color
    var monospaced = false
    @Binding var text: String

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .foregroundStyle(text.isEmpty ? AnyShapeStyle(.tertiary) : AnyShapeStyle(tint))
                .frame(width: 26)
            VStack(alignment: .leading, spacing: 1) {
                Text(label).font(.caption).foregroundStyle(.secondary)
                TextField(placeholder, text: $text, axis: .vertical)
                    .font(monospaced ? .body.monospaced() : .body.weight(.medium))
                    .textInputAutocapitalization(monospaced ? .never : .sentences)
                    .autocorrectionDisabled(monospaced)
                    .lineLimit(1...4)
            }
        }
        .padding(.vertical, 7)
    }
}

private struct VoiceWorkspaceCard: View {
    let plan: VoicePlan.Workspace
    let session: VoiceSession

    /// While the sentence is under way, a row shows only what was said.
    private func shown(_ value: String?, said: Bool) -> String? {
        session.firstSentence && !said ? nil : value
    }

    private var asksProject: Bool {
        if case .project = plan.question { true } else { false }
    }

    var body: some View {
        VStack(spacing: 0) {
            VoiceRow(
                symbol: "desktopcomputer",
                label: "Computer",
                value: shown(plan.computer?.name, said: session.draft.computer != nil),
                detail: computerDetail,
                tint: VoiceSlot.computer.tint
            ) {
                ForEach(session.catalog.computers.filter(\.online)) { computer in
                    Button(computer.name) { session.choose(computer: computer) }
                }
            }
            Divider().padding(.leading, 38)
            VoiceRow(
                symbol: "folder",
                label: "Project",
                value: shown(plan.project?.name, said: session.draft.project != nil),
                detail: plan.project == nil ? nil : detail(plan.projectSource),
                tint: VoiceSlot.project.tint,
                asking: asksProject
            ) {
                ForEach(plan.computer?.projects ?? []) { project in
                    Button(project.name) { session.choose(project: project) }
                }
            }
            Divider().padding(.leading, 38)
            VoiceTextRow(
                symbol: "arrow.triangle.branch",
                label: "New worktree",
                placeholder: "None, in the checkout itself",
                tint: VoiceSlot.topic.tint,
                monospaced: true,
                text: Binding(get: { plan.branch }, set: { session.edit(branch: $0) })
            )
            Divider().padding(.leading, 38)
            VoiceRow(
                symbol: "sparkle",
                label: "Agent",
                value: shown(plan.agent?.name, said: session.draft.agent != nil),
                detail: plan.agent == nil ? nil : detail(plan.agentSource),
                tint: VoiceSlot.agent.tint,
                asking: plan.agent == nil && !session.firstSentence
            ) {
                ForEach(plan.computer?.agents ?? []) { agent in
                    Button(agent.name) { session.choose(agent: agent) }
                }
            }
            Divider().padding(.leading, 38)
            VoiceTextRow(
                symbol: "text.bubble",
                label: "First prompt",
                placeholder: session.firstSentence ? "Listening…" : "None",
                tint: VoiceSlot.task.tint,
                text: Binding(get: { plan.prompt }, set: { session.edit(prompt: $0) })
            )
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 6)
        .background(Color(.secondarySystemGroupedBackground), in: .rect(cornerRadius: 18))
        if let question = plan.question, !session.firstSentence {
            Text(question.words).font(.headline)
            if case let .project(_, _, choices) = question, !choices.isEmpty {
                HStack(spacing: 8) {
                    ForEach(choices) { project in
                        Button(project.name) { session.choose(project: project) }.buttonStyle(.glass)
                    }
                }
            }
            if case .agent = question, let agents = plan.computer?.agents {
                HStack(spacing: 8) {
                    ForEach(agents.prefix(4)) { agent in
                        Button(agent.name) { session.choose(agent: agent) }.buttonStyle(.glass)
                    }
                }
            }
        }
    }

    private var computerDetail: String? {
        guard plan.computer != nil else { return nil }
        if plan.computerSource == .said, let load = plan.computer?.home?.load {
            return "\(Int(load.spareCores)) cores spare"
        }
        return detail(plan.computerSource)
    }

    private func detail(_ source: VoicePlan.Source) -> String? {
        switch source {
        case .leastBusy: "least busy"
        case .recent: "last used"
        case .said: nil
        }
    }
}

/// One line per computer, and those it cannot run on.
private struct VoiceFanOutCard: View {
    let plan: VoicePlan.FanOut

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(plan.plans, id: \.computer?.id) { one in
                HStack(spacing: 12) {
                    Image(systemName: "desktopcomputer").foregroundStyle(VoiceSlot.computer.tint).frame(width: 26)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(one.computer?.name ?? "").font(.body.weight(.medium))
                        Text([one.project?.name, one.agent?.name].compactMap(\.self).joined(separator: " · "))
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            if let prompt = plan.plans.first?.prompt, !prompt.isEmpty {
                Divider()
                Text(prompt).font(.body.weight(.medium))
            }
            if !plan.skipped.isEmpty {
                Text("Skipping \(plan.skipped.joined(separator: ", "))").font(.caption).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(Color(.secondarySystemGroupedBackground), in: .rect(cornerRadius: 18))
    }
}

/// Starting can take several seconds. Closing leaves it going; once the
/// agent runs, the computer's Live Activity carries on from here.
private struct VoiceRunning: View {
    let since: Date?
    let close: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            ProgressView()
            VStack(alignment: .leading, spacing: 1) {
                Text("Starting…").font(.headline)
                if let since {
                    Text(since, style: .timer).font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                }
            }
            Spacer()
            Button("Close", action: close).buttonStyle(.glass)
        }
        .frame(minHeight: 44)
    }
}

private struct VoiceChatCard: View {
    let plan: VoicePlan.Chat

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(headline).font(.caption).foregroundStyle(.secondary)
            Text(plan.chat.title).font(.body.weight(.medium))
            Text(plan.chat.computer).font(.caption).foregroundStyle(.secondary)
            if let message { Text(message).padding(.top, 6) }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(Color(.secondarySystemGroupedBackground), in: .rect(cornerRadius: 18))
    }

    private var headline: String {
        switch plan.action {
        case .answer: "Answer"
        case .ask: "Send to"
        case .open: "Open"
        case .stop: "Stop"
        case .unanswerable: "Nothing to answer in"
        }
    }

    private var message: String? {
        switch plan.action {
        case let .answer(option, _): option.name
        case let .ask(text): text
        case .open, .stop, .unanswerable: nil
        }
    }
}

private struct VoiceConfirm: View {
    let session: VoiceSession
    let close: () -> Void

    private var runnable: Bool {
        switch session.plan {
        case let .workspace(plan): plan.question == nil
        case let .fanOut(plan): !plan.plans.isEmpty
        case let .chat(plan): plan.action != .unanswerable
        case .needsMe, .unknown: false
        }
    }

    /// The button says what it will do.
    private var action: String {
        guard case let .chat(plan) = session.plan else { return "Start" }
        return switch plan.action {
        case .answer: "Answer"
        case .ask: "Send"
        case .stop: "Stop"
        case .open, .unanswerable: "Start"
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if session.plan == .unknown {
                Text(session.draft.text.isEmpty ? "I did not hear anything." : "I cannot do that from here.").font(.headline)
                if !session.draft.text.isEmpty {
                    Button("Ask an agent instead", systemImage: "sparkle") { session.handOff() }
                        .buttonStyle(.glass)
                }
            }
            HStack(spacing: 10) {
                Button("Change by voice", systemImage: "mic.fill") { session.listenAgain() }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.glass)
                    .buttonBorderShape(.circle)
                    .controlSize(.large)
                Button { close() } label: { Text("Cancel").frame(maxWidth: .infinity).frame(height: 36) }
                    .buttonStyle(.glass)
                if runnable {
                    Button { Task { await session.confirm() } } label: {
                        Text(action).frame(maxWidth: .infinity).frame(height: 36)
                    }
                    .buttonStyle(.glassProminent)
                }
            }
        }
    }
}

private struct VoiceDoneView: View {
    let done: VoiceSession.Done
    let close: () -> Void
    let open: (Route) -> Void

    var body: some View {
        VStack(spacing: 14) {
            Spacer()
            Image(systemName: "checkmark.circle.fill").font(.system(size: 44)).foregroundStyle(.green)
            Text(done.title).font(.title3.weight(.semibold))
            Text(done.detail).font(.subheadline).foregroundStyle(.secondary).multilineTextAlignment(.center)
            Spacer()
            HStack(spacing: 10) {
                Button { close() } label: { Text("Done").frame(maxWidth: .infinity).frame(height: 36) }
                    .buttonStyle(.glass)
                if let route = done.route {
                    Button { open(route) } label: { Text("Open chat").frame(maxWidth: .infinity).frame(height: 36) }
                        .buttonStyle(.glassProminent)
                }
            }
        }
        .frame(maxWidth: .infinity)
    }
}

private struct VoiceAnswered: View {
    @Environment(AppModel.self) private var model
    let words: String
    let waiting: [VoiceSession.Waiting]

    var body: some View {
        Text(words).font(.title3.weight(.semibold))
        List(waiting) { item in
            Button {
                model.voice = nil
                model.pendingRoute = item.route
            } label: {
                VStack(alignment: .leading) {
                    Text(item.title)
                    Text(item.computer).font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        .listStyle(.plain)
    }
}

private struct VoiceProblem: View {
    @Environment(AppModel.self) private var model
    let title: String
    let detail: String
    let session: VoiceSession

    var body: some View {
        Text(title).font(.headline)
        Text(detail).foregroundStyle(.secondary)
        Spacer()
        Button { model.voice = nil } label: { Text("Close").frame(maxWidth: .infinity).frame(height: 36) }
            .buttonStyle(.glass)
    }
}
