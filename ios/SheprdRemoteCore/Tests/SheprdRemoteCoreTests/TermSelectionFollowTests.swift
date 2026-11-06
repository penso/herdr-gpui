import Foundation
import Testing
@testable import SheprdRemoteCore

@Suite struct TermSelectionFollowTests {
    func screen(_ rows: [String]) throws -> TermScreen {
        let json = rows.map { #"[{"text":"\#($0)"}]"# }.joined(separator: ",")
        return try JSONDecoder().decode(TermScreen.self, from: Data(#"{"pane":"p","columns":40,"rows":[\#(json)]}"#.utf8))
    }

    func at(_ row: Int, _ column: Int) -> TermSelection.Position { .init(row: row, column: column) }

    @Test func aSelectionStaysWhileOtherRowsChange() throws {
        // A spinner redraws its row while the selected one stays.
        let old = try screen(["ls -la", "⠋ Working"])
        let new = try screen(["ls -la", "⠙ Working"])
        let selected = TermSelection(anchor: at(0, 3), head: at(0, 5))
        #expect(new.following(selected, from: old) == selected)
    }

    @Test func aSelectionMovesWithItsTextWhenOutputScrollsIt() throws {
        let old = try screen(["one", "好的 two", "three", ""])
        let new = try screen(["好的 two", "three", "", "four"])
        let selected = TermSelection(anchor: at(2, 2), head: at(1, 0))
        let moved = try #require(new.following(selected, from: old))
        #expect(moved == TermSelection(anchor: at(1, 2), head: at(0, 0)))
        #expect(new.text(moved) == "好的 two\nthr")
    }

    @Test func aSelectionGoesWhenItsTextChangesOrLeavesTheScreen() throws {
        let old = try screen(["one", "two"])
        let selected = TermSelection(anchor: at(0, 0), head: at(0, 2))
        #expect(try screen(["uno", "two"]).following(selected, from: old) == nil)
        // Scrolled off the top.
        #expect(try screen(["two", "three"]).following(selected, from: old) == nil)
    }

    @Test func theNearestMoveWinsAmongRepeatedRows() throws {
        let old = try screen(["──", "a", "──", "b"])
        let new = try screen(["a", "──", "b", "──"])
        // The selected rule matches a row up and a row down; output
        // scrolls rows up, so up wins.
        let selected = TermSelection(anchor: at(2, 0), head: at(2, 1))
        #expect(new.following(selected, from: old) == TermSelection(anchor: at(1, 0), head: at(1, 1)))
    }
}
