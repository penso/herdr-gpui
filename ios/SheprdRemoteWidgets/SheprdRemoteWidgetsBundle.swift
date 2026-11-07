import SwiftUI
import WidgetKit

/// The Live Activity, the needs-you widget, and the Controls.
@main
struct SheprdRemoteWidgetsBundle: WidgetBundle {
    var body: some Widget {
        ChatLiveActivity()
        AttentionWidget()
        BoardControl()
        NewWorkspaceControl()
        TalkControl()
    }
}
