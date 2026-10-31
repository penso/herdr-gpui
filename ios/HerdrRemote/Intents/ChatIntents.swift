import AppIntents
import HerdrRemoteCore
import UIKit

/// Opens a chat: from Spotlight, Siri, or Shortcuts.
struct OpenChatIntent: OpenIntent {
    static let title: LocalizedStringResource = "Open Chat"

    @Parameter(title: "Chat") var target: ChatEntity

    @MainActor
    func perform() async throws -> some IntentResult {
        AppModel.current?.show(.chat(target.route))
        return .result()
    }
}

/// "What needs me?": the chats waiting on you, as the app last saw them.
struct WhatNeedsMeIntent: AppIntent {
    static let title: LocalizedStringResource = "What Needs Me"
    static let description = IntentDescription("Lists the chats waiting on you on every computer.")

    func perform() async throws -> some IntentResult & ProvidesDialog & ReturnsValue<[ChatEntity]> {
        let summary = WidgetSummary.load()
        let waiting = summary.chats.filter(\.needsYou)
        // On a locked phone, or with the app lock on, Siri says counts
        // only, never chat titles.
        let hidden = AppLock.isOn
        guard await UIApplication.shared.isProtectedDataAvailable, !hidden else {
            let unlock = hidden ? "Open Herdr Remote to see which." : "Unlock to see which."
            let dialog: IntentDialog = waiting.isEmpty ? "Nothing needs you." : "\(waiting.count) chat\(waiting.count == 1 ? "" : "s") need\(waiting.count == 1 ? "s" : "") you. \(unlock)"
            return .result(value: [], dialog: dialog)
        }
        let dialog: IntentDialog = switch waiting.count {
        case 0: summary.working > 0 ? "Nothing needs you. \(summary.working) running." : "Nothing needs you."
        case 1: "\(waiting[0].title) on \(waiting[0].computer) needs you."
        default: "\(waiting.count) chats need you: \(waiting.prefix(3).map(\.title).joined(separator: ", "))."
        }
        return .result(value: waiting.map { ChatEntity($0) }, dialog: dialog)
    }
}

/// Sends a prompt to a chat without opening the app.
struct AskChatIntent: AppIntent {
    static let title: LocalizedStringResource = "Ask a Chat"
    static let description = IntentDescription("Sends a prompt to a chat on one of your computers.")
    // A prompt can make an agent do anything: never from a locked phone.
    static let authenticationPolicy = IntentAuthenticationPolicy.requiresAuthentication

    @Parameter(title: "Chat") var chat: ChatEntity
    @Parameter(title: "Prompt", requestValueDialog: "What should it do?") var prompt: String

    static var parameterSummary: some ParameterSummary {
        Summary("Ask \(\.$chat) to \(\.$prompt)")
    }

    func perform() async throws -> some IntentResult & ProvidesDialog {
        try AppLockError.check()
        let text = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { throw $prompt.needsValueError() }
        try await BackgroundCommands.send(.prompt(text: text), computer: chat.computerId, pane: chat.pane)
        return .result(dialog: "Sent to \(chat.title).")
    }
}

/// Stops a chat's running turn.
struct StopChatIntent: AppIntent {
    static let title: LocalizedStringResource = "Stop a Chat"
    static let description = IntentDescription("Stops the running turn of a chat, declines its questions, and drops its queue.")
    static let authenticationPolicy = IntentAuthenticationPolicy.requiresAuthentication

    @Parameter(title: "Chat") var chat: ChatEntity

    func perform() async throws -> some IntentResult & ProvidesDialog {
        try AppLockError.check()
        try await requestConfirmation(actionName: .go, dialog: "Stop \(chat.title) on \(chat.computer)?")
        try await BackgroundCommands.send(.cancel, computer: chat.computerId, pane: chat.pane)
        return .result(dialog: "Stopped \(chat.title).")
    }
}

struct HerdrShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: WhatNeedsMeIntent(),
            phrases: ["What needs me in \(.applicationName)", "What's waiting in \(.applicationName)"],
            shortTitle: "What Needs Me",
            systemImageName: "hand.raised"
        )
        AppShortcut(
            intent: AskChatIntent(),
            phrases: ["Ask \(\.$chat) in \(.applicationName)", "Ask a chat in \(.applicationName)"],
            shortTitle: "Ask a Chat",
            systemImageName: "text.bubble"
        )
        AppShortcut(
            intent: StopChatIntent(),
            phrases: ["Stop \(\.$chat) in \(.applicationName)"],
            shortTitle: "Stop a Chat",
            systemImageName: "stop.circle"
        )
        AppShortcut(
            intent: OpenBoardIntent(),
            phrases: ["Open the \(.applicationName) chats"],
            shortTitle: "Chats",
            systemImageName: "house"
        )
    }
}

/// With the app locked (Settings > Privacy), Siri and Shortcuts do not act
/// on chats: they cannot ask for Face ID in the background, and the lock
/// promises that nothing reaches a chat without it.
enum AppLockError: Error, CustomLocalizedStringResourceConvertible {
    case locked

    var localizedStringResource: LocalizedStringResource {
        "Herdr Remote is locked. Open it to send to a chat."
    }

    static func check() throws {
        if AppLock.isOn { throw AppLockError.locked }
    }
}
