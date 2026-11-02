import Foundation
import HerdrRemoteCore

/// What the Watch asks the phone over WatchConnectivity. The Watch holds no
/// keys: the phone checks each request against the chats it last saw and
/// sends it with its own device key.
enum WatchRequest: Codable, Hashable, Sendable {
    case answer(computer: String, pane: String, permission: PermissionId, option: String)
    case reply(computer: String, pane: String, text: String)

    /// The command to send, when the summary shows that chat waiting on
    /// that question with that answer. "Always" answers stay on the phone,
    /// where they ask for Face ID first.
    func command(in summary: WidgetSummary) -> (command: ChatCommand, computer: String, pane: String)? {
        switch self {
        case let .answer(computer, pane, permission, option):
            guard let chat = summary.chats.first(where: { $0.computerId == computer && $0.pane == pane }),
                  let question = chat.question, question.permission == permission,
                  let chosen = question.options.first(where: { $0.id == option }),
                  WatchRequest.answersOnWatch(chosen.kind)
            else { return nil }
            return (.answer(permission: permission, option: option), computer, pane)
        case let .reply(computer, pane, text):
            let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !text.isEmpty, text.count <= 4_000,
                  summary.chats.contains(where: { $0.computerId == computer && $0.pane == pane })
            else { return nil }
            return (.prompt(text: text), computer, pane)
        }
    }

    static func answersOnWatch(_ kind: PermissionKind) -> Bool {
        kind != .allowAlways && kind != .rejectAlways
    }
}

/// The phone's answer to a `WatchRequest`.
struct WatchResponse: Codable, Hashable, Sendable {
    var sent: Bool
    var message: String?
}
