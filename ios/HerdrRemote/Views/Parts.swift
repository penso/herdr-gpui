import HerdrRemoteCore
import SwiftUI

/// An agent's mark: its logo, the one Herdr UI and the web app draw
/// (`assets/icons/agent-*.svg`), on its brand's color, or on a plain tile
/// for marks without one.
struct AgentIcon: View {
    var agent: String?
    var size: CGFloat = 28

    private var look: (mark: String, tile: AnyShapeStyle, ink: Color) {
        let name = (agent ?? "").lowercased()
        let plain = AnyShapeStyle(.fill.secondary)
        // Copilot before Pi: "copilot" holds "pi".
        if name.contains("claude") { return ("claude", AnyShapeStyle(Color(red: 0xD9 / 255, green: 0x64 / 255, blue: 0x3A / 255)), .white) }
        if name.contains("codex") { return ("codex", AnyShapeStyle(Color(red: 0x11 / 255, green: 0x11 / 255, blue: 0x11 / 255)), .white) }
        if name.contains("gemini") { return ("gemini", AnyShapeStyle(Color(red: 0x3B / 255, green: 0x7C / 255, blue: 0xF0 / 255)), .white) }
        if name.contains("copilot") { return ("copilot", AnyShapeStyle(Color(red: 0x6E / 255, green: 0x40 / 255, blue: 0xC9 / 255)), .white) }
        if name.contains("opencode") { return ("opencode", plain, .primary) }
        if name.contains("amp") { return ("amp", plain, .primary) }
        if name.contains("pi") { return ("pi", plain, .primary) }
        return ("generic", plain, .primary)
    }

    var body: some View {
        let look = look
        Image("Agents/\(look.mark)")
            .resizable()
            .renderingMode(.template)
            .scaledToFit()
            .foregroundStyle(look.ink)
            .frame(width: size * 0.62, height: size * 0.62)
            .frame(width: size, height: size)
            .background(look.tile, in: .rect(cornerRadius: size * 0.28))
            .accessibilityLabel(agent ?? "Agent")
    }
}

/// Herdr UI's working orbit: a faint ring with an arc turning on it once
/// every `period`. Every orbit reads the same clock, so all working
/// agents turn in step, as on the desktop. Held still under Reduce Motion.
struct WorkingOrbit: View {
    var color: Color = Home.AgentStatus.working.color
    var lineWidth: CGFloat = 1.8
    @Environment(\.accessibilityReduceMotion) private var still

    /// One turn, as the desktop's spinner takes.
    static let period = 0.9

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 30, paused: still)) { context in
            let turn = still ? 0 : context.date.timeIntervalSinceReferenceDate
                .truncatingRemainder(dividingBy: Self.period) / Self.period
            ZStack {
                Circle().stroke(color.opacity(0.3), lineWidth: lineWidth)
                // A quarter of the way round, as `status-orbit.svg` draws it.
                Circle().trim(from: 0, to: 0.25)
                    .stroke(color, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                    .rotationEffect(.degrees(-90 + 360 * turn))
            }
        }
        .accessibilityLabel("Working")
    }
}

extension Home.AgentStatus {
    /// The color Herdr UI's sidebar gives this status in its default
    /// palette: red for blocked, yellow for working, teal for done, green
    /// for idle.
    var color: Color {
        switch self {
        case .blocked: .red
        case .working: .yellow
        case .done: .teal
        case .idle: .green
        case .unknown: .secondary
        }
    }
}

/// The status a row shows after its title.
struct StatusMark: View {
    var status: Home.AgentStatus
    var asking = false

    var body: some View {
        Group {
            if asking || status == .blocked {
                Circle().fill(Home.AgentStatus.blocked.color).frame(width: 9, height: 9).accessibilityLabel("Needs you")
            } else {
                switch status {
                case .working:
                    Image(systemName: "ellipsis").foregroundStyle(status.color).symbolEffect(.variableColor.iterative)
                        .accessibilityLabel("Working")
                case .done:
                    Image(systemName: "checkmark").font(.caption2.bold()).foregroundStyle(status.color).accessibilityLabel("Done")
                default:
                    EmptyView()
                }
            }
        }
    }
}

/// An agent's status as a filled round icon, centered on a row as the
/// pull request's state is.
struct StatusBadge: View {
    var status: Home.AgentStatus

    var body: some View {
        Group {
            switch status {
            case .blocked:
                Image(systemName: "exclamationmark.circle.fill").foregroundStyle(status.color).accessibilityLabel("Needs you")
            case .working:
                Image(systemName: "ellipsis.circle.fill").foregroundStyle(status.color).symbolEffect(.pulse)
                    .accessibilityLabel("Working")
            case .done:
                Image(systemName: "checkmark.circle.fill").foregroundStyle(status.color).accessibilityLabel("Done")
            case .idle, .unknown:
                EmptyView()
            }
        }
        .imageScale(.large)
    }
}

struct ChatRowLabel: View {
    var item: ChatItem
    var showsComputer = true
    /// Off on the workspace's own screen, which names it already.
    var showsWorkspace = true

    var body: some View {
        // The timer and status share the title's line, so the detail
        // line below runs the full width.
        HStack(spacing: 12) {
            AgentIcon(agent: item.chat.agent)
            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(item.chat.title).lineLimit(1)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if item.working, let started = item.chat.summary?.turnStartedMs {
                        Text(timerInterval: Date(timeIntervalSince1970: Double(started) / 1000)...Date.distantFuture, countsDown: false)
                            .font(.footnote.monospacedDigit()).foregroundStyle(.secondary)
                            .fixedSize()
                    } else if !item.needsYou, let date = item.lastAnswer {
                        LastAnswer(date: date)
                    }
                    StatusMark(status: item.chat.status, asking: item.question != nil)
                }
                Text(detail).font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private var detail: String {
        let place = [showsComputer ? item.computer.name : nil, showsWorkspace ? item.workspace.label : nil].compactMap { $0 }
        let said = item.question?.title ?? item.chat.summary?.lastText
        return (place + [said].compactMap { $0 }).joined(separator: " · ")
    }
}

/// A terminal tab with an agent in it, such as `claude` run in a shell,
/// drawn like a chat row; it opens the terminal.
struct AgentTerminalRowLabel: View {
    var item: TerminalItem
    var showsComputer = true
    /// Off on the workspace's own screen, which shows the tab's name
    /// instead.
    var showsWorkspace = true

    var body: some View {
        HStack(spacing: 12) {
            AgentIcon(agent: item.agent?.name)
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title).lineLimit(1)
                Text(place)
                    .font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            // Centered on the row, beside the chevron.
            StatusBadge(status: item.agent?.status ?? .unknown)
        }
    }

    private var place: String {
        guard showsWorkspace else { return tabName }
        return showsComputer ? "\(item.computer.name) · \(item.workspace.label)" : item.workspace.label
    }

    /// The tab's name, unless Herdr only numbered it or it repeats the
    /// title: then the agent's name says more.
    private var tabName: String {
        let label = item.terminal.label
        let agent = item.agent?.name ?? "Agent"
        if label.allSatisfy(\.isNumber) || label == item.title { return agent }
        return "\(agent) · \(label)"
    }
}

/// How long ago a chat last answered, as a list shows it: "now", "5m",
/// "3h", a weekday within the week, then the date.
struct LastAnswer: View {
    let date: Date

    var body: some View {
        TimelineView(.everyMinute) { context in
            Text(Self.label(date, now: context.date))
                .font(.footnote.monospacedDigit()).foregroundStyle(.secondary)
                .fixedSize()
                .accessibilityLabel("Last answered \(date.formatted(.relative(presentation: .named)))")
        }
    }

    static func label(_ date: Date, now: Date) -> String {
        let seconds = now.timeIntervalSince(date)
        if seconds < 60 { return "now" }
        if seconds < 3600 { return "\(Int(seconds / 60))m" }
        if seconds < 86400 { return "\(Int(seconds / 3600))h" }
        if seconds < 6 * 86400 { return date.formatted(.dateTime.weekday(.abbreviated)) }
        return date.formatted(.dateTime.month(.abbreviated).day())
    }
}

/// A question with its options as buttons, answered in place.
struct AnswerCard: View {
    @Environment(AppModel.self) private var model
    var item: ChatItem
    var question: Permission
    @State private var sending = false
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                AgentIcon(agent: item.chat.agent, size: 24)
                VStack(alignment: .leading, spacing: 0) {
                    Text(item.chat.title).font(.subheadline.weight(.semibold)).lineLimit(1)
                    Text("\(item.computer.name) · \(item.workspace.label)").font(.caption).foregroundStyle(.secondary)
                }
            }
            Text(question.title)
            // Permissions fit on one row; a question's answers get a row each.
            let compact = question.options.allSatisfy { $0.kind != .other } && question.options.count <= 4
            if compact {
                HStack {
                    ForEach(question.options) { option in
                        if option.kind == .allowOnce {
                            Button(option.name) { answer(option) }.buttonStyle(.glassProminent).tint(Home.AgentStatus.blocked.color).lineLimit(1)
                        } else {
                            Button(option.name) { answer(option) }.buttonStyle(.glass).lineLimit(1)
                        }
                    }
                }
            } else {
                ForEach(question.options) { option in
                    Button { answer(option) } label: { Text(option.name).frame(maxWidth: .infinity) }
                        .buttonStyle(.glass)
                }
            }
            if let problem { Text(problem).font(.footnote).foregroundStyle(.red) }
        }
        .disabled(sending)
        .padding(14)
        .background(.background.secondary, in: .rect(cornerRadius: 22))
        .overlay(RoundedRectangle(cornerRadius: 22).strokeBorder(Home.AgentStatus.blocked.color.opacity(0.35)))
    }

    private func answer(_ option: PermissionOption) {
        sending = true
        Task {
            guard await OwnerCheck.allows(option) else {
                sending = false
                return
            }
            problem = await model.fleet.send(.answer(permission: question.id, option: option.id), to: item.key)
            sending = false
        }
    }
}

extension Home.AgentStatus {
    var label: String {
        switch self {
        case .unknown: "Unknown"
        case .idle: "Idle"
        case .working: "Working"
        case .blocked: "Needs you"
        case .done: "Done"
        }
    }
}
