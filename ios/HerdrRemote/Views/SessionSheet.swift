import HerdrRemoteCore
import SwiftUI

/// Session C: the chat's name, cards for the context, the mode, effort,
/// the session's other settings, and auto-accept, which asks for Face ID
/// before it turns on, then closing the chat.
struct SessionSheet: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    let store: ChatStore
    /// The chat's name as the lists show it.
    let name: String
    /// The chat's pane is gone; the chat screen leaves with the sheet.
    let onClosed: () -> Void
    @State private var problem: String?
    @State private var effort: Double?
    @State private var confirmingClose = false
    @State private var closing = false
    /// The name being typed; `nil` until the field is edited.
    @State private var draft: String?
    @State private var renaming = false
    @FocusState private var naming: Bool

    var body: some View {
        NavigationStack {
            ScrollView {
                // Picks show at once; the sync bar says when they are not in yet.
                if let transcript = store.transcript.map({ model.fleet.chat($0, for: key) }) {
                    VStack(spacing: 14) {
                        nameCard()
                        contextCard(transcript)
                        if let modes = transcript.modes, !modes.available.isEmpty { modeTiles(modes) }
                        if let option = transcript.options.first(where: { $0.category == .thoughtLevel }),
                           case let .select(choices, current) = option.kind, choices.count > 1 {
                            effortCard(option, choices: choices, current: current)
                        }
                        let others = transcript.options.filter { $0.category != .thoughtLevel }
                        if !others.isEmpty { optionsCard(others) }
                        autoAcceptCard(transcript)
                        closeButton(transcript)
                        if let problem { Text(problem).font(.footnote).foregroundStyle(.red) }
                    }
                    .padding(16)
                }
            }
            .navigationTitle("Session")
            .navigationBarTitleDisplayMode(.inline)
            .overlay(alignment: .top) { SyncBar().ignoresSafeArea(edges: .top) }
        }
    }

    // MARK: Cards

    private func nameCard() -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Name").font(.subheadline.weight(.semibold)).foregroundStyle(.secondary)
            HStack {
                TextField("Name", text: Binding(get: { draft ?? name }, set: { draft = $0 }))
                    .font(.headline)
                    .submitLabel(.done)
                    .focused($naming)
                    .onSubmit { rename(draft ?? name) }
                if renaming {
                    ProgressView()
                } else if naming {
                    Button("Save") { rename(draft ?? name) }.font(.subheadline.weight(.semibold))
                }
            }
            Button("Use the Agent's Title") { rename("") }
                .font(.caption)
                .disabled(renaming)
        }
        .card()
    }

    private func contextCard(_ transcript: Transcript) -> some View {
        HStack(spacing: 18) {
            ZStack {
                ContextRing(percent: transcript.usage?.percent ?? 0, size: 76, lineWidth: 8)
                Text("\(transcript.usage?.percent ?? 0)%").font(.title3.weight(.semibold).monospacedDigit())
            }
            VStack(alignment: .leading, spacing: 4) {
                Text(currentName(transcript.options.first { $0.category == .model }) ?? transcript.agent)
                    .font(.headline)
                if let usage = transcript.usage {
                    Text("\(tokens(usage.used)) of \(tokens(usage.size)) tokens").font(.subheadline).foregroundStyle(.secondary)
                } else {
                    Text("No usage reported yet").font(.subheadline).foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 0)
        }
        .card()
    }

    private func modeTiles(_ modes: Modes) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Mode").font(.subheadline.weight(.semibold)).foregroundStyle(.secondary)
            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 10) {
                ForEach(modes.available, id: \.id) { mode in
                    let selected = mode.id == modes.current
                    Button {
                        send(.setMode(mode.id))
                    } label: {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(mode.name).font(.subheadline.weight(.semibold))
                            if let description = mode.description {
                                Text(description).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                                    .multilineTextAlignment(.leading)
                            }
                        }
                        .frame(maxWidth: .infinity, minHeight: 64, alignment: .topLeading)
                        .padding(12)
                        .background(selected ? AnyShapeStyle(.tint.opacity(0.18)) : AnyShapeStyle(.fill.quinary), in: .rect(cornerRadius: 14))
                        .overlay {
                            RoundedRectangle(cornerRadius: 14).strokeBorder(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.clear), lineWidth: 1.5)
                        }
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(selected ? .isSelected : [])
                    .sensoryFeedback(.selection, trigger: modes.current)
                }
            }
        }
        .card()
    }

    private func effortCard(_ option: ConfigOption, choices: [Choice], current: String) -> some View {
        let index = Double(choices.firstIndex { $0.id == current } ?? 0)
        let shown = Int((effort ?? index).rounded())
        return VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(option.name).font(.subheadline.weight(.semibold)).foregroundStyle(.secondary)
                Spacer()
                Text(choices[min(max(shown, 0), choices.count - 1)].name).font(.subheadline.weight(.semibold))
            }
            Slider(
                value: Binding(get: { effort ?? index }, set: { effort = $0 }),
                in: 0...Double(choices.count - 1),
                step: 1
            ) { editing in
                guard !editing, let effort else { return }
                let choice = choices[min(max(Int(effort.rounded()), 0), choices.count - 1)]
                self.effort = nil
                if choice.id != current { send(.setOption(id: option.id, value: .choice(choice.id))) }
            }
            .accessibilityLabel(option.name)
            HStack {
                Text(choices.first?.name ?? "").font(.caption2).foregroundStyle(.tertiary)
                Spacer()
                Text(choices.last?.name ?? "").font(.caption2).foregroundStyle(.tertiary)
            }
        }
        .card()
    }

    private func optionsCard(_ options: [ConfigOption]) -> some View {
        VStack(spacing: 12) {
            ForEach(options) { option in
                switch option.kind {
                case let .select(choices, current):
                    HStack {
                        Text(option.name)
                        Spacer()
                        Menu {
                            ForEach(choices, id: \.id) { choice in
                                Button {
                                    send(.setOption(id: option.id, value: .choice(choice.id)))
                                } label: {
                                    if choice.id == current { Label(choice.name, systemImage: "checkmark") } else { Text(choice.name) }
                                }
                            }
                        } label: {
                            HStack(spacing: 4) {
                                Text(choices.first { $0.id == current }?.name ?? current).lineLimit(1)
                                Image(systemName: "chevron.up.chevron.down").font(.caption)
                            }
                        }
                    }
                case let .toggle(current):
                    Toggle(option.name, isOn: Binding(
                        get: { current },
                        set: { send(.setOption(id: option.id, value: .toggle($0))) }
                    ))
                }
            }
        }
        .card()
    }

    private func autoAcceptCard(_ transcript: Transcript) -> some View {
        Toggle(isOn: Binding(get: { transcript.autoAccept }, set: { setAutoAccept($0) })) {
            VStack(alignment: .leading, spacing: 2) {
                Label("Auto-accept", systemImage: "bolt.shield").font(.headline)
                Text("Allows every tool call without asking. Turning it on asks for Face ID.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .tint(.red)
        .card(tint: transcript.autoAccept ? .red : nil)
    }

    private func closeButton(_ transcript: Transcript) -> some View {
        Button(role: .destructive) { confirmingClose = true } label: {
            HStack {
                Label("Close Chat", systemImage: "xmark.circle")
                Spacer()
                if closing { ProgressView() }
            }
            .font(.headline)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(16)
            .background(.red.opacity(0.12), in: .rect(cornerRadius: 20))
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .foregroundStyle(.red)
        .disabled(closing)
        .confirmationDialog("Close this chat?", isPresented: $confirmingClose, titleVisibility: .visible) {
            Button("Close Chat", role: .destructive) { close() }
        } message: {
            Text("Its pane closes in Herdr and \(transcript.agent) stops.")
        }
    }

    // MARK: Actions

    private func close() {
        closing = true
        Task {
            do {
                try await model.fleet.closeChatPane(key)
                onClosed()
            } catch {
                problem = error.localizedDescription
            }
            closing = false
        }
    }

    /// An empty name clears the chat's own, so its title is the agent's.
    private func rename(_ label: String) {
        naming = false
        renaming = true
        Task {
            do {
                try await model.fleet.rename(key, to: label)
                draft = nil
                problem = nil
            } catch {
                problem = error.localizedDescription
            }
            renaming = false
        }
    }

    private func setAutoAccept(_ on: Bool) {
        Task {
            if on, !(await OwnerCheck.confirm("Allow every tool call without asking")) { return }
            problem = await model.fleet.send(.setAutoAccept(on), to: key)
        }
    }

    private func send(_ command: ChatCommand) {
        Task { problem = await model.fleet.send(command, to: key) }
    }

    private func currentName(_ option: ConfigOption?) -> String? {
        guard let option, case let .select(choices, current) = option.kind else { return nil }
        return choices.first { $0.id == current }?.name ?? current
    }

    private func tokens(_ count: UInt64) -> String {
        count >= 1000 ? "\(count / 1000)k" : "\(count)"
    }
}

private extension View {
    func card(tint: Color? = nil) -> some View {
        padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(tint.map { AnyShapeStyle($0.opacity(0.12)) } ?? AnyShapeStyle(.background.secondary), in: .rect(cornerRadius: 20))
    }
}
