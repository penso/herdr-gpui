import SwiftUI

// Small pieces every mockup draws: agent icons, status marks, the composer.

struct AgentIcon: View {
    var agent: Agent
    var size: CGFloat = 28

    var body: some View {
        Image(systemName: agent.symbol)
            .font(.system(size: size * 0.5, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(agent.tint.gradient, in: .rect(cornerRadius: size * 0.28))
    }
}

struct StatusMark: View {
    var status: Status

    var body: some View {
        switch status {
        case .working:
            HStack(spacing: 2) {
                ForEach(0..<3) { index in
                    Circle().frame(width: 4, height: 4).opacity(index == 2 ? 0.35 : 0.9)
                }
            }
            .foregroundStyle(.secondary)
        case .needsYou:
            Circle().fill(.orange).frame(width: 9, height: 9)
        case .unread:
            Circle().fill(.blue).frame(width: 9, height: 9)
        case .done:
            Image(systemName: "checkmark").font(.caption2.bold()).foregroundStyle(.green)
        case .idle:
            EmptyView()
        }
    }
}

struct ChatRowView: View {
    var chat: Chat
    var showsDetail = true
    var label: String? = nil

    var body: some View {
        HStack(spacing: 12) {
            AgentIcon(agent: chat.agent)
            VStack(alignment: .leading, spacing: 2) {
                Text(chat.title).font(.body).lineLimit(1)
                if showsDetail {
                    Text(label.map { "\($0) · \(chat.detail)" } ?? chat.detail)
                        .font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            Spacer(minLength: 8)
            if let elapsed = chat.elapsed {
                Text(elapsed).font(.footnote.monospacedDigit()).foregroundStyle(.secondary)
            }
            StatusMark(status: chat.status)
        }
        .padding(.vertical, 2)
    }
}

/// The context ring in the chat header: how full the agent's window is.
struct ContextRing: View {
    var fraction: Double

    var body: some View {
        ZStack {
            Circle().stroke(.quaternary, lineWidth: 3)
            Circle().trim(from: 0, to: fraction)
                .stroke(.tint, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                .rotationEffect(.degrees(-90))
        }
        .frame(width: 20, height: 20)
    }
}

struct Composer: View {
    var placeholder = "Message Claude"
    var working = true

    var body: some View {
        HStack(alignment: .bottom, spacing: 10) {
            Button {} label: { Image(systemName: "plus").font(.title3.weight(.medium)).frame(width: 36, height: 36) }
                .buttonStyle(.glass)
                .buttonBorderShape(.circle)
            HStack {
                Text(placeholder).foregroundStyle(.tertiary)
                Spacer()
                Image(systemName: "mic.fill").foregroundStyle(.secondary)
            }
            .padding(.horizontal, 16)
            .frame(height: 44)
            .glassEffect(.regular, in: .capsule)
            Button {} label: {
                Image(systemName: working ? "stop.fill" : "arrow.up")
                    .font(.body.weight(.bold)).frame(width: 36, height: 36)
            }
            .buttonStyle(.glassProminent)
            .buttonBorderShape(.circle)
        }
        .padding(.horizontal, 16)
        .padding(.bottom, 6)
    }
}

/// One step of a turn's timeline.
struct Step: Identifiable {
    let id = UUID()
    var symbol: String
    var title: String
    var detail: String
    var state: StepState
}

enum StepState { case done, running, pending }

extension Step {
    static let sample: [Step] = [
        Step(symbol: "doc.text.magnifyingglass", title: "Read 4 files", detail: "commands.rs, keymap.rs, +2", state: .done),
        Step(symbol: "pencil", title: "Edit commands.rs", detail: "+48 −3", state: .done),
        Step(symbol: "pencil", title: "Edit tests/equalize.rs", detail: "+92", state: .done),
        Step(symbol: "terminal", title: "Run just ci", detail: "clippy · 0:42", state: .running),
        Step(symbol: "checklist", title: "Update docs/cli.md", detail: "Planned", state: .pending),
    ]
}

struct StepIcon: View {
    var step: Step

    var body: some View {
        ZStack {
            Circle().fill(step.state == .running ? AnyShapeStyle(.tint) : AnyShapeStyle(.quaternary))
            if step.state == .running {
                ProgressView().controlSize(.mini).tint(.white)
            } else {
                Image(systemName: step.state == .done ? step.symbol : "circle.dotted")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(step.state == .done ? .primary : .tertiary)
            }
        }
        .frame(width: 24, height: 24)
    }
}

/// A fenced code block that scrolls sideways.
struct CodeBlock: View {
    var language: String
    var code: String

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language).font(.caption.weight(.medium)).foregroundStyle(.secondary)
                Spacer()
                Image(systemName: "doc.on.doc").font(.caption).foregroundStyle(.secondary)
            }
            .padding(.horizontal, 12).padding(.vertical, 8)
            ScrollView(.horizontal, showsIndicators: false) {
                Text(code).font(.system(.footnote, design: .monospaced))
                    .padding(.horizontal, 12).padding(.bottom, 10)
            }
        }
        .background(.fill.quinary, in: .rect(cornerRadius: 12))
    }
}

enum SampleAnswer {
    static let intro = "Added **Equalize Splits** to the command catalog. It walks the tab's split tree and gives every sibling an equal share, so nested splits even out too."
    static let code = """
    Command::EqualizeSplits => {
        let tree = snapshot.tab(tab)?.splits();
        Some(Request::SetRatios(tree.equalized()))
    }
    """
    static let outro = "Tests cover nested splits and a single pane. `just ci` is running now."
}
