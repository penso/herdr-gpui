import Testing
@testable import SheprdRemoteCore

@Suite struct AppAppearanceTests {
    /// The raw values are saved, so renaming one would lose a choice.
    @Test func savedValuesAreStable() {
        #expect(AppAppearance(rawValue: "system") == .system)
        #expect(AppAppearance(rawValue: "light") == .light)
        #expect(AppAppearance(rawValue: "dark") == .dark)
    }

    @Test func theSettingOffersEveryChoice() {
        #expect(AppAppearance.allCases.map(\.title) == ["System", "Light", "Dark"])
    }
}
