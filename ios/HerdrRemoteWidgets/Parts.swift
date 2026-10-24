import HerdrRemoteCore
import SwiftUI

/// Colors from the mockups: green while working, amber while a question
/// waits, red when the turn failed.
enum Palette {
    static let working = Color(red: 0.19, green: 0.82, blue: 0.35)
    static let question = Color(red: 1.0, green: 0.62, blue: 0.04)
    static let failed = Color(red: 1.0, green: 0.27, blue: 0.23)

    static func phase(_ phase: ChatActivityState.Phase) -> Color {
        switch phase {
        case .working, .finished: working
        case .question: question
        case .failed: failed
        }
    }
}

/// The agent's mark: its initial on its color. Widgets cannot load images
/// from the network, and the daemon names agents by display name.
struct AgentBadge: View {
    let agent: String
    var size: CGFloat = 22

    var body: some View {
        Text(agent.first.map { String($0).uppercased() } ?? "?")
            .font(.system(size: size * 0.55, weight: .bold, design: .rounded))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(color, in: RoundedRectangle(cornerRadius: size * 0.28, style: .continuous))
            .accessibilityLabel(agent)
    }

    private var color: Color {
        switch agent.lowercased() {
        case let name where name.hasPrefix("claude"): Color(red: 0.85, green: 0.47, blue: 0.34)
        case let name where name.hasPrefix("codex"): Color(white: 0.2)
        case let name where name.hasPrefix("gemini"): Color(red: 0.26, green: 0.52, blue: 0.96)
        case let name where name.hasPrefix("pi"): Color(red: 0.55, green: 0.36, blue: 0.96)
        default: Color.gray
        }
    }
}

/// A dot while working or asking, a check when finished, a cross when failed.
struct PhaseMark: View {
    let phase: ChatActivityState.Phase
    var size: CGFloat = 10

    var body: some View {
        switch phase {
        case .working, .question:
            Circle()
                .fill(Palette.phase(phase))
                .frame(width: size, height: size)
        case .finished:
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(Palette.working)
                .font(.system(size: size * 1.8))
        case .failed:
            Image(systemName: "xmark.circle.fill")
                .foregroundStyle(Palette.failed)
                .font(.system(size: size * 1.8))
        }
    }
}

/// Time since the turn started, counted by the system so it moves without
/// updates.
struct Elapsed: View {
    let since: Date

    var body: some View {
        Text(timerInterval: since...Date.distantFuture, countsDown: false)
            .monospacedDigit()
    }
}

/// The working bar: plan progress when the agent keeps a plan, else time
/// against a quarter of an hour, which the system animates.
struct TurnProgress: View {
    let state: ChatActivityState

    var body: some View {
        Group {
            if state.phase == .finished {
                ProgressView(value: 1)
            } else if let fraction = state.planFraction {
                ProgressView(value: fraction)
            } else {
                ProgressView(
                    timerInterval: state.startedAt...state.startedAt.addingTimeInterval(15 * 60),
                    countsDown: false,
                    label: { EmptyView() },
                    currentValueLabel: { EmptyView() }
                )
            }
        }
        .progressViewStyle(.linear)
        .tint(Palette.working)
    }
}
