import SheprdRemoteCore
import SwiftUI

/// What a new chat with `agent` starts in, on the screens that open one, so
/// a mode carried over from the last chat is seen and can be changed first.
struct StartChoiceRow: View {
    @Environment(AppModel.self) private var model
    /// The agent's name, as chats report it.
    let agent: String

    var body: some View {
        let choice = model.startChoice(for: agent)
        Menu {
            if !choice.offered.isEmpty {
                Picker("Mode", selection: mode(choice)) {
                    Text("Agent's Default").tag(String?.none)
                    ForEach(choice.offered) { Text($0.name).tag(Optional($0.id)) }
                }
            }
            Toggle("Auto-accept", isOn: autoAccept(choice))
        } label: {
            HStack(spacing: 12) {
                Image(systemName: choice.bypasses ? "bolt.shield.fill" : "shield")
                    .foregroundStyle(choice.bypasses ? AnyShapeStyle(.red) : AnyShapeStyle(.secondary))
                    .frame(width: 28)
                VStack(alignment: .leading, spacing: 2) {
                    Text("New \(agent) tabs start in").font(.caption).foregroundStyle(.secondary)
                    Text(summary(choice)).font(.subheadline.weight(.semibold)).foregroundStyle(.primary)
                    if choice.offered.isEmpty {
                        Text("Its modes show here after your first chat with it.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                Spacer(minLength: 0)
                Image(systemName: "chevron.up.chevron.down").font(.footnote.weight(.semibold)).foregroundStyle(.tertiary)
            }
            .contentShape(.rect)
        }
        // A menu tints its label; this row reads as text, like the others here.
        .buttonStyle(.plain)
        .accessibilityLabel("New \(agent) tabs start in \(summary(choice))")
        .accessibilityHint("Changes what new tabs start with")
    }

    private func summary(_ choice: StartChoice) -> String {
        let mode = choice.picked?.name ?? "Agent's default"
        return choice.autoAccept ? "\(mode) · Auto-accept" : mode
    }

    private func mode(_ choice: StartChoice) -> Binding<String?> {
        Binding {
            choice.picked?.id
        } set: { id in
            model.chooseStart(.mode, for: agent) { $0.mode = id }
        }
    }

    private func autoAccept(_ choice: StartChoice) -> Binding<Bool> {
        Binding {
            choice.autoAccept
        } set: { on in
            model.chooseStart(.autoAccept, for: agent) { $0.autoAccept = on }
        }
    }
}
