import AppIntents
import HerdrRemoteCore
import SwiftUI

/// Live Activity B's answers: up to three of the waiting question's
/// options as buttons that answer without opening the app. "Always"
/// answers stay in the app, where they can ask for Face ID first.
struct AnswerButtons: View {
    let attributes: ChatActivityAttributes
    let state: ChatActivityState

    var body: some View {
        if state.phase == .question, let permission = state.permission, let question = state.question, !answers.isEmpty {
            HStack(spacing: 8) {
                ForEach(answers, id: \.id) { option in
                    Button(intent: AnswerIntent(computer: attributes.computerId, pane: attributes.pane, permission: permission, option: option, question: question)) {
                        Text(option.name)
                            .font(.subheadline.weight(.semibold))
                            .lineLimit(1)
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(option.kind.allows ? Palette.question : Color.white.opacity(0.2))
                    .foregroundStyle(option.kind.allows ? Color.black : Color.white)
                }
            }
        }
    }

    private var answers: [ChatActivityState.Option] {
        Array(state.options.filter { $0.kind != .allowAlways && $0.kind != .rejectAlways }.prefix(3))
    }
}

/// Live Activity B's step stack: the two steps before the current one,
/// fading upward.
struct StepStack: View {
    let state: ChatActivityState

    var body: some View {
        if state.phase == .working, !state.previous.isEmpty {
            VStack(alignment: .leading, spacing: 2) {
                ForEach(Array(state.previous.suffix(2).enumerated()), id: \.offset) { index, step in
                    Label(step, systemImage: "checkmark")
                        .font(.caption2)
                        .lineLimit(1)
                        .opacity(index == 0 && state.previous.count > 1 ? 0.35 : 0.6)
                }
            }
            .foregroundStyle(.secondary)
        }
    }
}
