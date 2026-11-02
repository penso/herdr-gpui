import Foundation

/// What a phone asks of a chat (`POST /api/chat/{pane}/command`):
/// `herdr_chat_protocol::Command` as serde writes it.
public enum ChatCommand: Encodable, Hashable, Sendable {
    /// `attachments` are ids the daemon gave uploaded pictures, never paths.
    case prompt(text: String, attachments: [String] = [])
    /// Ends the running turn, declines its questions, and drops the queue.
    case cancel
    case answer(permission: PermissionId, option: String)
    case setMode(String)
    case setOption(id: String, value: OptionValue)
    case unqueue(UUID)
    case setAutoAccept(Bool)

    public enum OptionValue: Hashable, Sendable {
        case choice(String)
        case toggle(Bool)
    }

    enum CodingKeys: String, CodingKey { case command, text, attachments, option, permission, id, value, on }
    enum ValueKeys: String, CodingKey { case type, id, value }
    enum AttachmentKeys: String, CodingKey { case path }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .prompt(text, attachments):
            try c.encode("prompt", forKey: .command)
            try c.encode(text, forKey: .text)
            var list = c.nestedUnkeyedContainer(forKey: .attachments)
            for id in attachments {
                var item = list.nestedContainer(keyedBy: AttachmentKeys.self)
                try item.encode(id, forKey: .path)
            }
        case .cancel:
            try c.encode("cancel", forKey: .command)
        case let .answer(permission, option):
            try c.encode("answer", forKey: .command)
            try c.encode(option, forKey: .option)
            try c.encode(permission, forKey: .permission)
        case let .setMode(id):
            try c.encode("set_mode", forKey: .command)
            try c.encode(id, forKey: .id)
        case let .setOption(id, value):
            try c.encode("set_option", forKey: .command)
            try c.encode(id, forKey: .id)
            var nested = c.nestedContainer(keyedBy: ValueKeys.self, forKey: .value)
            switch value {
            case let .choice(choice):
                try nested.encode("choice", forKey: .type)
                try nested.encode(choice, forKey: .id)
            case let .toggle(on):
                try nested.encode("toggle", forKey: .type)
                try nested.encode(on, forKey: .value)
            }
        case let .unqueue(id):
            try c.encode("unqueue", forKey: .command)
            try c.encode(id, forKey: .id)
        case let .setAutoAccept(on):
            try c.encode("set_auto_accept", forKey: .command)
            try c.encode(on, forKey: .on)
        }
    }
}
