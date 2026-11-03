import Foundation

/// What a chat command should change in the transcript, so the phone can
/// show it at once and know when the computer has caught up. A prompt has
/// its own outgoing row, and a cancel has nothing to show.
public enum Expected: Hashable, Sendable {
    case mode(String)
    case option(id: String, value: ChatCommand.OptionValue)
    case autoAccept(Bool)
    case answer(PermissionId)
    case unqueue(UUID)

    /// Which setting it changes: a newer command for the same one
    /// replaces an older one still on its way.
    public enum Slot: Hashable, Sendable {
        case mode
        case option(String)
        case autoAccept
        case answer(PermissionId)
        case unqueue(UUID)
    }

    public init?(_ command: ChatCommand) {
        switch command {
        case let .setMode(id): self = .mode(id)
        case let .setOption(id, value): self = .option(id: id, value: value)
        case let .setAutoAccept(on): self = .autoAccept(on)
        case let .answer(permission, _): self = .answer(permission)
        case let .unqueue(id): self = .unqueue(id)
        case .prompt, .cancel: return nil
        }
    }

    public var slot: Slot {
        switch self {
        case .mode: .mode
        case let .option(id, _): .option(id)
        case .autoAccept: .autoAccept
        case let .answer(permission): .answer(permission)
        case let .unqueue(id): .unqueue(id)
        }
    }

    /// Whether the transcript already shows the change.
    public func shown(in transcript: Transcript) -> Bool {
        switch self {
        case let .mode(id):
            return transcript.modes?.current == id
        case let .option(id, value):
            guard let option = transcript.options.first(where: { $0.id == id }) else { return false }
            switch (option.kind, value) {
            case let (.select(_, current), .choice(wanted)): return current == wanted
            case let (.toggle(current), .toggle(wanted)): return current == wanted
            default: return false
            }
        case let .autoAccept(on):
            return transcript.autoAccept == on
        case let .answer(permission):
            return !transcript.permissions.contains { $0.id == permission }
        case let .unqueue(id):
            return !transcript.queued.contains { $0.id == id }
        }
    }

    /// Shows the change in `transcript` before the computer confirms it.
    public func apply(to transcript: inout Transcript) {
        switch self {
        case let .mode(id):
            guard transcript.modes?.available.contains(where: { $0.id == id }) == true else { return }
            transcript.modes?.current = id
        case let .option(id, value):
            guard let index = transcript.options.firstIndex(where: { $0.id == id }) else { return }
            switch (transcript.options[index].kind, value) {
            case let (.select(choices, _), .choice(wanted)) where choices.contains(where: { $0.id == wanted }):
                transcript.options[index].kind = .select(choices: choices, current: wanted)
            case let (.toggle, .toggle(wanted)):
                transcript.options[index].kind = .toggle(current: wanted)
            default:
                break
            }
        case let .autoAccept(on):
            transcript.autoAccept = on
        case let .answer(permission):
            transcript.permissions.removeAll { $0.id == permission }
        case let .unqueue(id):
            transcript.queued.removeAll { $0.id == id }
        }
    }
}
