import AppIntents

/// Opens the app on Home, from a Control or Shortcuts. The type keeps the
/// board's name, which Controls and shortcuts people added refer to. The widget
/// extension only needs the type to draw a Control; iOS runs `perform` in
/// the app, which it opens first.
struct OpenBoardIntent: AppIntent {
    static let title: LocalizedStringResource = "Open the Chats"
    static let description = IntentDescription("Shows every computer's chats by what they need.")
    static let openAppWhenRun = true

    @MainActor
    func perform() async throws -> some IntentResult {
        #if !WIDGET_EXTENSION
        AppModel.current?.show(.home)
        #endif
        return .result()
    }
}

/// Opens the app on the new workspace form.
struct NewWorkspaceIntent: AppIntent {
    static let title: LocalizedStringResource = "New Workspace"
    static let description = IntentDescription("Starts a workspace with an agent on one of your computers.")
    static let openAppWhenRun = true

    @MainActor
    func perform() async throws -> some IntentResult {
        #if !WIDGET_EXTENSION
        AppModel.current?.show(.newWorkspace)
        #endif
        return .result()
    }
}
