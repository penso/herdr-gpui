import SheprdRemoteCore
import SwiftUI

/// Applies the appearance chosen in Settings to a window.
struct Appearance: ViewModifier {
    @AppStorage(AppAppearance.key) private var appearance = AppAppearance.system

    func body(content: Content) -> some View {
        content.preferredColorScheme(scheme)
    }

    private var scheme: ColorScheme? {
        switch appearance {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }
}
