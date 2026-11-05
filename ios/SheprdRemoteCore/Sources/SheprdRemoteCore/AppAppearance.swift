/// Whether the app follows the system's light or dark setting or keeps one.
public enum AppAppearance: String, CaseIterable, Identifiable, Sendable {
    case system, light, dark

    /// Where the choice is kept.
    public static let key = "appearance"

    public var id: Self { self }

    public var title: String {
        switch self {
        case .system: "System"
        case .light: "Light"
        case .dark: "Dark"
        }
    }
}
