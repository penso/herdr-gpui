import ActivityKit
import HerdrRemoteCore
import SwiftUI
import WidgetKit

/// One running chat on the lock screen and in the Dynamic Island. Tapping
/// anywhere opens `herdr-remote://chat?...`, which the app turns into the
/// chat's page in the web app.
struct ChatLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: ChatActivityAttributes.self) { context in
            LockScreenView(attributes: context.attributes, state: context.state)
                .activityBackgroundTint(Color.black.opacity(0.55))
                .activitySystemActionForegroundColor(.white)
                .widgetURL(context.attributes.link.url)
        } dynamicIsland: { context in
            let state = context.state
            let attributes = context.attributes
            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    AgentBadge(agent: attributes.agent, size: 28)
                        .padding(.leading, 4)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    if state.phase == .working {
                        Elapsed(since: state.startedAt)
                            .font(.caption.weight(.semibold))
                            .frame(maxWidth: 56, alignment: .trailing)
                    } else {
                        PhaseMark(phase: state.phase)
                    }
                }
                DynamicIslandExpandedRegion(.center) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(title(state, attributes))
                            .font(.subheadline.weight(.semibold))
                            .lineLimit(1)
                        Text(attributes.facts.place)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    ExpandedBottom(attributes: attributes, state: state)
                }
            } compactLeading: {
                HStack(spacing: 4) {
                    Circle()
                        .fill(Palette.working)
                        .frame(width: 8, height: 8)
                    Text("\(max(state.working, 1))")
                        .font(.caption2.weight(.bold))
                        .monospacedDigit()
                }
            } compactTrailing: {
                if state.phase == .question || state.waiting > 0 {
                    HStack(spacing: 3) {
                        Circle()
                            .fill(Palette.question)
                            .frame(width: 8, height: 8)
                        Text("\(max(state.waiting, 1))")
                            .font(.caption2.weight(.bold))
                            .monospacedDigit()
                    }
                    .foregroundStyle(Palette.question)
                } else {
                    Elapsed(since: state.startedAt)
                        .font(.caption2.weight(.semibold))
                        .frame(maxWidth: 40)
                }
            } minimal: {
                PhaseMark(phase: state.phase, size: 9)
            }
            .widgetURL(attributes.link.url)
            .keylineTint(Palette.phase(state.phase))
        }
    }

    private func title(_ state: ChatActivityState, _ attributes: ChatActivityAttributes) -> String {
        switch state.phase {
        case .working: "\(attributes.agent) is working"
        case .question: "\(attributes.agent) needs an answer"
        case .finished: "\(attributes.agent) finished"
        case .failed: "\(attributes.agent) stopped"
        }
    }
}

/// The expanded island's lower half: the question (or step, or summary) in
/// a dark block, and Open.
private struct ExpandedBottom: View {
    let attributes: ChatActivityAttributes
    let state: ChatActivityState

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(text)
                .font(.system(.footnote, design: state.phase == .question ? .monospaced : .default))
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(8)
                .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            HStack {
                if state.phase == .working, let plan = state.planLabel {
                    Text(plan)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Link(destination: attributes.link.url) {
                    Text("Open")
                        .font(.subheadline.weight(.semibold))
                        .padding(.horizontal, 16)
                        .padding(.vertical, 6)
                        .background(
                            state.phase == .question ? Palette.question : Color.white.opacity(0.18),
                            in: Capsule()
                        )
                        .foregroundStyle(state.phase == .question ? Color.black : Color.white)
                }
            }
        }
    }

    private var text: String {
        switch state.phase {
        case .working: state.step
        case .question: state.question ?? state.step
        case .finished, .failed: state.summary ?? state.step
        }
    }
}
