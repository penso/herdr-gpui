import Foundation
import Testing
@testable import HerdrRemoteCore

@Suite struct TextShortcutTests {
    @Test func aNewInstallOffersProceed() {
        let shortcuts = TextShortcut.load(nil)
        #expect(shortcuts == [.proceed])
        #expect(TextShortcut.proceed.isUsable)
    }

    @Test func savedListsRoundTripIncludingAnEmptyOne() throws {
        let custom = TextShortcut(title: "Tests", text: "Run the tests and fix what fails.")
        #expect(TextShortcut.load(try TextShortcut.save([custom, .proceed])) == [custom, .proceed])
        // Deleting every shortcut sticks; the default does not come back.
        #expect(TextShortcut.load(try TextShortcut.save([])).isEmpty)
    }

    @Test func unreadableDataFallsBackToTheDefaults() {
        #expect(TextShortcut.load(Data("not json".utf8)) == TextShortcut.defaults)
    }

    @Test func aBlankTitleShowsTheTextsFirstLine() {
        #expect(TextShortcut(title: "  ", text: "Keep going\nand commit").label == "Keep going")
        let long = TextShortcut(title: "", text: String(repeating: "a", count: 50))
        #expect(long.label == String(repeating: "a", count: 40) + "…")
        #expect(TextShortcut(title: " Go ", text: "x").label == "Go")
        #expect(!TextShortcut(title: "Empty", text: " \n").isUsable)
    }
}
