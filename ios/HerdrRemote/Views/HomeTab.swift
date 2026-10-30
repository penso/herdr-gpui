import HerdrRemoteCore
import SwiftUI

/// Home B: what needs you, answered in place; what is working; the rest.
struct HomeTab: View {
    @Environment(AppModel.self) private var model
    @Binding var pairing: Bool

    var body: some View {
        ChatList()
            .navigationBarTitleDisplayMode(.inline)
            .overlay {
                if model.computers.isEmpty {
                    ContentUnavailableView {
                        Label("No computers yet", systemImage: "desktopcomputer")
                    } description: {
                        Text("In Herdr, open Remote control > Pair device, then scan its code here.")
                    } actions: {
                        Button("Pair a computer") { pairing = true }.buttonStyle(.glassProminent)
                    }
                } else if model.fleet.chatItems.isEmpty, model.fleet.agentTerminals.isEmpty {
                    ContentUnavailableView("No chats", systemImage: "bubble.left.and.text.bubble.right",
                                           description: Text("Chats and agents running in Herdr on your computers appear here."))
                }
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    // Pairing is rare, so it lives at the bottom of Computers.
                    Button("New Workspace", systemImage: "plus") { model.creating = .workspace(device: nil) }
                        .disabled(model.fleet.machines.isEmpty)
                }
            }
    }
}
