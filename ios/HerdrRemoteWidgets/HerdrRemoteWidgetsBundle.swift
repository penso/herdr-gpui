import SwiftUI
import WidgetKit

/// The widget extension holds only the Live Activity; Herdr Remote has no
/// home screen widgets.
@main
struct HerdrRemoteWidgetsBundle: WidgetBundle {
    var body: some Widget {
        ChatLiveActivity()
    }
}
