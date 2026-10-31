import HerdrRemoteCore
import UserNotifications

/// Keys a shown notification's `userInfo` carries once opened.
enum AlertKeys {
    static let computer = "computer"
    static let pane = "pane"
    static let permission = "permission"

    /// A chat's notification thread. Pane ids are only unique on one
    /// computer, so the thread names the computer too.
    static func thread(computer: String, pane: String) -> String {
        "\(computer)/\(pane)"
    }

    /// Whether `content` is one of the chat's notifications.
    static func belongs(_ content: UNNotificationContent, computer: String, pane: String) -> Bool {
        content.userInfo[Self.computer] as? String == computer && content.userInfo[Self.pane] as? String == pane
    }
}

/// Registers the category a notification's actions hang on. Questions get
/// one category per distinct set of answers, so the Notification Service
/// Extension adds each before showing its notification, keeping only the
/// most recent few.
enum NotificationCategories {
    static let maxQuestionCategories = 16

    static func register(_ presentation: AlertPresentation) async {
        guard let id = presentation.category else { return }
        let center = UNUserNotificationCenter.current()
        var categories = await center.notificationCategories()
        if categories.contains(where: { $0.identifier == id }) { return }
        let questions = categories.filter { $0.identifier.hasPrefix("question.") }
        if questions.count >= maxQuestionCategories, let oldest = questions.first {
            categories.remove(oldest)
        }
        categories.insert(category(id, presentation.actions))
        center.setNotificationCategories(categories)
    }

    static func category(_ id: String, _ actions: [AlertPresentation.Action]) -> UNNotificationCategory {
        UNNotificationCategory(
            identifier: id,
            actions: actions.map(action),
            intentIdentifiers: [],
            options: []
        )
    }

    private static func action(_ action: AlertPresentation.Action) -> UNNotificationAction {
        switch action.style {
        case let .textInput(placeholder):
            // A prompt can make an agent do anything, so a locked phone
            // asks for Face ID or the passcode first.
            UNTextInputNotificationAction(identifier: action.id, title: action.title, options: [.authenticationRequired], textInputButtonTitle: "Send", textInputPlaceholder: placeholder)
        case .destructive:
            UNNotificationAction(identifier: action.id, title: action.title, options: [.destructive])
        case .authenticated:
            UNNotificationAction(identifier: action.id, title: action.title, options: [.authenticationRequired])
        case .plain:
            UNNotificationAction(identifier: action.id, title: action.title, options: [])
        }
    }
}
