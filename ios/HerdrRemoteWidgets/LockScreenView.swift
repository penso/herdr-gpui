import ActivityKit
import HerdrRemoteCore
import SwiftUI
import WidgetKit

/// The lock screen banner (mockup 5): the agent and `workspace · chat` on
/// top, then the phase with its step, elapsed time, and cost, and a bar
/// while the turn works. Amber with an Open button when a question waits;
/// a check and the summary when the turn finished.
struct LockScreenView: View {
    let attributes: ChatActivityAttributes
    let state: ChatActivityState

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                AgentBadge(agent: attributes.agent)
                Text(attributes.facts.place)
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
                Spacer(minLength: 4)
                Text(attributes.agent)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            HStack(alignment: .center, spacing: 10) {
                PhaseMark(phase: state.phase)
                VStack(alignment: .leading, spacing: 2) {
                    Text(state.headline)
                        .font(.headline)
                        .lineLimit(1)
                    detail
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Spacer(minLength: 4)
                action
            }
            if state.phase == .working || state.phase == .finished {
                TurnProgress(state: state)
            }
        }
        .padding(14)
        .background(state.phase == .question ? Palette.question.opacity(0.18) : Color.clear)
    }

    @ViewBuilder
    private var detail: some View {
        switch state.phase {
        case .working:
            HStack(spacing: 4) {
                if let plan = state.planLabel {
                    Text(plan)
                    Text("\u{b7}")
                }
                Elapsed(since: state.startedAt)
                if let cost = state.cost {
                    Text("\u{b7}")
                    Text(cost)
                }
            }
        case .question:
            Text(state.question ?? state.step)
        case .finished, .failed:
            Text(state.summary ?? state.step)
        }
    }

    @ViewBuilder
    private var action: some View {
        switch state.phase {
        case .question:
            Link(destination: attributes.link.url) {
                Text("Open")
                    .font(.subheadline.weight(.semibold))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 6)
                    .background(Palette.question, in: Capsule())
                    .foregroundStyle(.black)
            }
        case .finished:
            Link(destination: attributes.link.url) {
                Text("Reply")
                    .font(.subheadline.weight(.semibold))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 6)
                    .background(.white.opacity(0.18), in: Capsule())
            }
        case .working, .failed:
            EmptyView()
        }
    }
}
