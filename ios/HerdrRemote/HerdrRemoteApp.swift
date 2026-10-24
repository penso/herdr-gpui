import SwiftUI

/// Herdr Remote: the companion to Herdr's web app that adds Live Activities
/// and the Dynamic Island. Chats themselves stay in the web app; this app
/// pairs with computers, keeps them supplied with push tokens, and opens a
/// chat's page when an activity is tapped.
@main
struct HerdrRemoteApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(model)
                .onOpenURL { url in
                    Task { await model.open(url) }
                }
                .task {
                    model.tokens.start()
                }
        }
    }
}
