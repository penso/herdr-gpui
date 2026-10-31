import HerdrRemoteCore
import SwiftUI

/// A thin bar along the top edge when something this phone sent has not
/// come back: an orange sweep while it is on its way, solid red when the
/// computer refused some or it never showed. Nothing while all is in.
/// A tap lists what is outstanding.
struct SyncBar: View {
    @Environment(AppModel.self) private var model
    @State private var showing = false
    /// Whether waiting has lasted long enough to show; a change that is
    /// back within a moment would only flash the bar.
    @State private var patient = false

    /// How long a command may be out before the bar shows.
    static let grace: Duration = .milliseconds(300)
    static let height: CGFloat = 3

    var body: some View {
        let state = shownState
        let waiting = if case .waiting = state { true } else { false }
        let failed = if case .failed = state { true } else { false }
        let visible = failed || (waiting && patient)
        ZStack(alignment: .top) {
            if visible {
                Button { showing = true } label: {
                    stripe(failed: failed)
                        .frame(height: Self.height)
                        // Taller than it looks, so a thumb can reach it.
                        .frame(maxWidth: .infinity, minHeight: 16, alignment: .top)
                        .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(label(state))
                .transition(.opacity)
                .popover(isPresented: $showing) {
                    SyncList().presentationCompactAdaptation(.popover)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .top)
        .animation(.easeInOut(duration: 0.2), value: visible)
        .animation(.easeInOut(duration: 0.2), value: failed)
        .task(id: waiting) {
            patient = false
            guard waiting else { return }
            try? await Task.sleep(for: Self.grace)
            patient = !Task.isCancelled
        }
    }

    private var shownState: SyncState {
        #if DEBUG
        // The simulator loop's `-syncBar waiting|failed` shows the bar.
        switch UserDefaults.standard.string(forKey: "syncBar") {
        case "waiting": return .waiting(1)
        case "failed": return .failed(1)
        default: break
        }
        #endif
        return model.fleet.pending.state
    }

    @ViewBuilder
    private func stripe(failed: Bool) -> some View {
        if failed {
            Color.red
        } else {
            // A highlight sweeping across, so waiting reads as progress.
            TimelineView(.animation) { context in
                let phase = context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 1.4) / 1.4
                LinearGradient(
                    stops: [
                        .init(color: .orange.opacity(0.35), location: 0),
                        .init(color: .orange, location: phase),
                        .init(color: .orange.opacity(0.35), location: min(phase + 0.35, 1)),
                    ],
                    startPoint: .leading,
                    endPoint: .trailing
                )
            }
        }
    }

    private func label(_ state: SyncState) -> String {
        switch state {
        case .synced: "Everything you sent is in"
        case let .waiting(count): "\(count) change\(count == 1 ? "" : "s") on the way"
        case let .failed(count): "\(count) change\(count == 1 ? "" : "s") did not go through"
        }
    }
}

/// What the sync bar is waiting for, chat by chat.
private struct SyncList: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let pending = model.fleet.pending
        VStack(alignment: .leading, spacing: 10) {
            if pending.items.isEmpty {
                Label("Everything you sent is in", systemImage: "checkmark.circle.fill")
                    .foregroundStyle(.green)
            }
            ForEach(pending.items) { item in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Image(systemName: item.failed ? "exclamationmark.circle.fill" : "clock")
                        .foregroundStyle(item.failed ? .red : .orange)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("\(item.expected?.label ?? "Stop") · \(title(item.key))").font(.subheadline.weight(.medium))
                        Text(detail(item)).font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            if pending.items.contains(where: \.failed) {
                Button("Dismiss") { pending.dismissFailures() }.buttonStyle(.glass).controlSize(.small)
            }
        }
        .padding(16)
        .frame(minWidth: 260, alignment: .leading)
    }

    private func title(_ key: PaneKey) -> String {
        model.fleet.chatItems.first { $0.key == key }?.chat.title ?? "Chat"
    }

    private func detail(_ item: PendingCommand) -> String {
        switch item.state {
        case .sending: "Sending to the computer…"
        case .sent: "Waiting for the chat to show it…"
        case let .failed(reason): "Undone: \(reason)"
        }
    }
}
