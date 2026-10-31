import AppIntents
import SwiftUI
import WidgetKit

/// Opens Home from Control Center, the Lock Screen, or the Action button.
struct BoardControl: ControlWidget {
    var body: some ControlWidgetConfiguration {
        // The kind keeps the board's name, so Controls already added stay.
        StaticControlConfiguration(kind: "so.pen.herdr.remote.board") {
            ControlWidgetButton(action: OpenBoardIntent()) {
                Label("Herdr Chats", systemImage: "house")
            }
        }
        .displayName("Herdr Chats")
        .description("Every computer's chats, by what they need.")
    }
}

/// Opens the new workspace form.
struct NewWorkspaceControl: ControlWidget {
    var body: some ControlWidgetConfiguration {
        StaticControlConfiguration(kind: "so.pen.herdr.remote.new-workspace") {
            ControlWidgetButton(action: NewWorkspaceIntent()) {
                Label("New Workspace", systemImage: "folder.badge.plus")
            }
        }
        .displayName("New Herdr Workspace")
        .description("Starts a workspace with an agent.")
    }
}
