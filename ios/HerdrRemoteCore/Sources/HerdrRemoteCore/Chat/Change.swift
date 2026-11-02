import Foundation

/// One step from one transcript to the next: `herdr_chat_protocol::Change`.
public enum Change: Codable, Hashable, Sendable {
    case push(Entry)
    case append(id: EntryId, text: String)
    case replace(Entry)
    /// Entries before `before` were dropped to keep the transcript bounded.
    case truncate(before: EntryId)
    case status(Status)
    case permissionAsked(Permission)
    case permissionResolved(PermissionId)
    case plan([PlanEntry])
    case modes(Modes?)
    case currentMode(String)
    case options([ConfigOption])
    case turn(Turn?)
    case queue([Queued])
    case clients(UInt32)
    case session(SessionInfo?)
    case commands([SlashCommand])
    case usage(Usage?)
    case autoAccept(Bool)

    enum CodingKeys: String, CodingKey {
        case change, entry, id, text, before, status, permission, plan, modes, options, turn, queued, count
        case session, commands, usage, on
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .change) {
        case "push": self = .push(try c.decode(Entry.self, forKey: .entry))
        case "append": self = .append(id: try c.decode(EntryId.self, forKey: .id), text: try c.decode(String.self, forKey: .text))
        case "replace": self = .replace(try c.decode(Entry.self, forKey: .entry))
        case "truncate": self = .truncate(before: try c.decode(EntryId.self, forKey: .before))
        case "status": self = .status(try c.decode(Status.self, forKey: .status))
        case "permission_asked": self = .permissionAsked(try c.decode(Permission.self, forKey: .permission))
        case "permission_resolved": self = .permissionResolved(try c.decode(PermissionId.self, forKey: .id))
        case "plan": self = .plan(try c.decode([PlanEntry].self, forKey: .plan))
        case "modes": self = .modes(try c.decodeIfPresent(Modes.self, forKey: .modes))
        case "current_mode": self = .currentMode(try c.decode(String.self, forKey: .id))
        case "options": self = .options(try c.decode([ConfigOption].self, forKey: .options))
        case "turn": self = .turn(try c.decodeIfPresent(Turn.self, forKey: .turn))
        case "queue": self = .queue(try c.decode([Queued].self, forKey: .queued))
        case "clients": self = .clients(try c.decode(UInt32.self, forKey: .count))
        case "session": self = .session(try c.decodeIfPresent(SessionInfo.self, forKey: .session))
        case "commands": self = .commands(try c.decode([SlashCommand].self, forKey: .commands))
        case "usage": self = .usage(try c.decodeIfPresent(Usage.self, forKey: .usage))
        case "auto_accept": self = .autoAccept(try c.decode(Bool.self, forKey: .on))
        case let other:
            throw DecodingError.dataCorruptedError(forKey: .change, in: c, debugDescription: "unknown change \(other)")
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .push(entry): try c.encode("push", forKey: .change); try c.encode(entry, forKey: .entry)
        case let .append(id, text):
            try c.encode("append", forKey: .change); try c.encode(id, forKey: .id); try c.encode(text, forKey: .text)
        case let .replace(entry): try c.encode("replace", forKey: .change); try c.encode(entry, forKey: .entry)
        case let .truncate(before): try c.encode("truncate", forKey: .change); try c.encode(before, forKey: .before)
        case let .status(status): try c.encode("status", forKey: .change); try c.encode(status, forKey: .status)
        case let .permissionAsked(permission):
            try c.encode("permission_asked", forKey: .change); try c.encode(permission, forKey: .permission)
        case let .permissionResolved(id): try c.encode("permission_resolved", forKey: .change); try c.encode(id, forKey: .id)
        case let .plan(plan): try c.encode("plan", forKey: .change); try c.encode(plan, forKey: .plan)
        case let .modes(modes): try c.encode("modes", forKey: .change); try c.encode(modes, forKey: .modes)
        case let .currentMode(id): try c.encode("current_mode", forKey: .change); try c.encode(id, forKey: .id)
        case let .options(options): try c.encode("options", forKey: .change); try c.encode(options, forKey: .options)
        case let .turn(turn): try c.encode("turn", forKey: .change); try c.encode(turn, forKey: .turn)
        case let .queue(queued): try c.encode("queue", forKey: .change); try c.encode(queued, forKey: .queued)
        case let .clients(count): try c.encode("clients", forKey: .change); try c.encode(count, forKey: .count)
        case let .session(session): try c.encode("session", forKey: .change); try c.encode(session, forKey: .session)
        case let .commands(commands): try c.encode("commands", forKey: .change); try c.encode(commands, forKey: .commands)
        case let .usage(usage): try c.encode("usage", forKey: .change); try c.encode(usage, forKey: .usage)
        case let .autoAccept(on): try c.encode("auto_accept", forKey: .change); try c.encode(on, forKey: .on)
        }
    }
}

/// Why a change did not fit the transcript; named as the Rust reducer's
/// errors are in the shared fixtures.
public enum ChangeError: String, Error, Hashable, Sendable {
    case questionAnswersLimit = "question_answers_limit"
    case entryOrder = "entry_order"
    case unknownEntry = "unknown_entry"
    case notText = "not_text"
    case duplicatePermission = "duplicate_permission"
    case unknownPermission = "unknown_permission"
    case noModes = "no_modes"
}

extension Transcript {
    /// Applies `change`, or leaves the transcript as it was and throws why
    /// it does not fit: the reducer every side runs (`change.rs`).
    public mutating func apply(_ change: Change) throws(ChangeError) {
        switch change {
        case let .push(entry), let .replace(entry):
            if case let .notice(notice) = entry.kind, !QuestionAnswer.valid(notice.answers) {
                throw .questionAnswersLimit
            }
        default: break
        }
        switch change {
        case let .push(entry):
            if let last = entries.last?.id, entry.id <= last { throw .entryOrder }
            entries.append(entry)
        case let .append(id, text):
            guard let index = position(of: id) else { throw .unknownEntry }
            guard let kind = entries[index].kind.appending(text) else { throw .notText }
            entries[index].kind = kind
        case let .replace(entry):
            guard let index = position(of: entry.id) else { throw .unknownEntry }
            entries[index] = entry
        case let .truncate(before):
            let keep = entries.firstIndex { $0.id >= before } ?? entries.count
            if keep > 0 {
                entries.removeFirst(keep)
                truncated = true
            }
        case let .status(status): self.status = status
        case let .permissionAsked(permission):
            if permissions.contains(where: { $0.id == permission.id }) { throw .duplicatePermission }
            permissions.append(permission)
        case let .permissionResolved(id):
            guard let index = permissions.firstIndex(where: { $0.id == id }) else { throw .unknownPermission }
            permissions.remove(at: index)
        case let .plan(plan): self.plan = plan
        case let .modes(modes): self.modes = modes
        case let .currentMode(id):
            guard modes != nil else { throw .noModes }
            modes?.current = id
        case let .options(options): self.options = options
        case let .turn(turn): self.turn = turn
        case let .queue(queued): self.queued = queued
        case let .clients(count): clients = count
        case let .session(session): self.session = session
        case let .commands(commands): self.commands = commands
        case let .usage(usage): self.usage = usage
        case let .autoAccept(on): autoAccept = on
        }
    }
}
