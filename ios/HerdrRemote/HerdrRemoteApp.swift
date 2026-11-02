import SwiftUI
import UserNotifications

/// Herdr Remote: the chats and terminals running in Herdr on your
/// computers, with Live Activities on the lock screen. Each paired
/// computer keeps one live socket while the app is in front; launch draws
/// the cached state at once.
@main
struct HerdrRemoteApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    private var model: AppModel { delegate.model }
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .modifier(AppLock())
                .onOpenURL { model.open($0) }
                .task { model.tokens.start() }
                .onChange(of: model.fleet.waiting, initial: true) { _, _ in
                    // While the app is in front, the badge follows the live
                    // counts; pushes set it otherwise, from the same counts.
                    if phase == .active { UNUserNotificationCenter.current().setBadgeCount(model.fleet.recordBadge()) }
                }
                .onChange(of: phase, initial: true) { _, phase in
                    switch phase {
                    case .active:
                        model.fleet.activate()
                        model.nearby.start()
                    case .background:
                        model.fleet.deactivate()
                        model.nearby.stop()
                    default: break
                    }
                }
        }
        // A chat or terminal in a window of its own (iPad, Stage Manager).
        WindowGroup("Chat", for: Route.self) { $route in
            NavigationStack {
                switch route {
                case let .chat(key, title)?: ChatScreen(key: key, title: title).withRoutes()
                case let .term(key, title)?: TerminalScreen(key: key, title: title)
                case nil: ContentUnavailableView("Nothing to show", systemImage: "questionmark.bubble")
                }
            }
            .environment(model)
            .modifier(AppLock())
        }
    }
}
