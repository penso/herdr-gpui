import Foundation
import HerdrRemoteCore
import Observation

/// A command this phone sent to a chat, shown as done at once and kept
/// until the chat shows it, the computer refuses it, or it takes too long.
struct PendingCommand: Identifiable {
    enum State: Equatable {
        /// The request is on its way to the computer.
        case sending
        /// The computer took it; the chat has not shown the change yet.
        case sent
        /// Refused, unreachable, or never shown; the change is undone.
        case failed(String)
    }

    let id = UUID()
    let key: PaneKey
    /// What it changes; `nil` for a cancel, done once the computer takes it.
    let expected: Expected?
    var state: State

    var failed: Bool { if case .failed = state { true } else { false } }
}

/// How far what this phone sent has come back, for the sync bar.
enum SyncState: Equatable {
    /// Every command this phone sent shows in its chat.
    case synced
    /// This many are on their way or not shown yet.
    case waiting(Int)
    /// This many were refused or never came back.
    case failed(Int)
}

/// The commands sent to chats that have not come back yet. Views draw the
/// chat as if they had, until each is shown or fails.
@MainActor
@Observable
final class PendingCommands {
    private(set) var items: [PendingCommand] = []

    /// How long a sent change may take to show before it counts as failed.
    static let patience: Duration = .seconds(15)
    /// How long a failure stays listed under the sync bar.
    static let remembered: Duration = .seconds(30)

    /// Tracks `command`. A newer pick for the same setting replaces the
    /// older one, which no longer decides what is shown.
    func start(_ command: ChatCommand, to key: PaneKey) -> PendingCommand.ID {
        let expected = Expected(command)
        if let slot = expected?.slot {
            items.removeAll { $0.key == key && $0.expected?.slot == slot }
        }
        let item = PendingCommand(key: key, expected: expected, state: .sending)
        items.append(item)
        return item.id
    }

    /// The computer answered the request. `shown` says whether the chat
    /// already shows a change, since its echo can beat the answer.
    func finished(_ id: PendingCommand.ID, failure: String?, shown: (PendingCommand) -> Bool) {
        guard let index = items.firstIndex(where: { $0.id == id }) else { return }
        if let failure {
            fail(id, failure)
            return
        }
        guard items[index].expected != nil, !shown(items[index]) else {
            items.remove(at: index)
            return
        }
        items[index].state = .sent
        Task { [weak self] in
            try? await Task.sleep(for: Self.patience)
            guard let self, items.contains(where: { $0.id == id && $0.state == .sent }) else { return }
            fail(id, "The chat did not show it within \(Self.patience.components.seconds) seconds.")
        }
    }

    /// Drops the commands of `key` the chat now shows, including ones that
    /// timed out before their echo arrived.
    func settle(_ key: PaneKey, shown: (PendingCommand) -> Bool) {
        items.removeAll { $0.key == key && $0.state != .sending && shown($0) }
    }

    /// The chat as it will be once the computer catches up.
    func apply(to transcript: Transcript, for key: PaneKey) -> Transcript {
        var shown = transcript
        for item in items where item.key == key && !item.failed {
            item.expected?.apply(to: &shown)
        }
        return shown
    }

    /// Whether this phone answered the question and the answer has not
    /// failed.
    func answered(_ key: PaneKey, permission: PermissionId) -> Bool {
        items.contains { $0.key == key && $0.expected == .answer(permission) && !$0.failed }
    }

    func dismissFailures() {
        items.removeAll(where: \.failed)
    }

    var state: SyncState {
        let failed = items.count(where: \.failed)
        if failed > 0 { return .failed(failed) }
        return items.isEmpty ? .synced : .waiting(items.count)
    }

    private func fail(_ id: PendingCommand.ID, _ reason: String) {
        guard let index = items.firstIndex(where: { $0.id == id }) else { return }
        items[index].state = .failed(reason)
        Task { [weak self] in
            try? await Task.sleep(for: Self.remembered)
            self?.items.removeAll { $0.id == id && $0.failed }
        }
    }
}

extension Expected {
    /// What the sync bar lists it as.
    var label: String {
        switch self {
        case .mode: "Mode"
        case .option: "Setting"
        case let .autoAccept(on): on ? "Turning on auto-accept" : "Turning off auto-accept"
        case .answer: "Answer"
        case .unqueue: "Removing a queued prompt"
        }
    }
}
