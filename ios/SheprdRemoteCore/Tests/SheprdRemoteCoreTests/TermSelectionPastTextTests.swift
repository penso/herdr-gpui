import CoreGraphics
import Foundation
import Testing
@testable import SheprdRemoteCore

/// A selection's ends stay on the text the highlight shows: a row is
/// highlighted only as far as its text, so an end dragged into the blank
/// space past it stops at the nearest character, as a text view's does.
@Suite struct TermSelectionPastTextTests {
    func screen(_ rows: [String]) throws -> TermScreen {
        let json = rows.map { #"[{"text":"\#($0)"}]"# }.joined(separator: ",")
        return try JSONDecoder().decode(TermScreen.self, from: Data(#"{"pane":"p","columns":40,"rows":[\#(json)]}"#.utf8))
    }

    func at(_ row: Int, _ column: Int) -> TermSelection.Position { .init(row: row, column: column) }

    let cell = CGSize(width: 10, height: 20)

    /// Rows 11, 13, 28, and 25 cells wide, the last ending in the cursor's
    /// blank cell, with 好多 at columns 2 to 5 of the last.
    func pane() throws -> TermScreen {
        try screen(["ab│cd│ef|gh", "│ ab │ 中文 │", "好标题好标题 bold plain 斜体", "❯ 好多 Shush, Khan. 中文 "])
    }

    @Test func anEndDraggedPastARowsTextStopsAtItsLastCell() throws {
        let screen = try pane()
        let word = TermSelection(anchor: at(3, 2), head: at(3, 5))
        let handles = TermHandles(word, cell: cell)
        let grabbed = CGPoint(x: handles.knob(.end).midX, y: handles.knob(.end).midY)
        let drag = try #require(TermHandleDrag(handles, at: grabbed))
        // To column 28, three cells past the row's last.
        let moved = drag.selection(at: CGPoint(x: grabbed.x + 230, y: grabbed.y), cell: cell, in: screen)
        #expect(moved == TermSelection(anchor: at(3, 2), head: at(3, 24)))
        // Its handle stands right of the last highlighted cell.
        #expect(TermHandles(moved, cell: cell).bar(.end).minX == 250)
        #expect(screen.text(moved) == "好多 Shush, Khan. 中文")
    }

    @Test func aStartDraggedPastARowsTextStartsTheNextRow() throws {
        let screen = try pane()
        let word = TermSelection(anchor: at(3, 2), head: at(3, 5))
        let handles = TermHandles(word, cell: cell)
        let grabbed = CGPoint(x: handles.knob(.start).midX, y: handles.knob(.start).midY)
        let drag = try #require(TermHandleDrag(handles, at: grabbed))
        // Up to row 0, column 20: past its text, which ends at column 10.
        let moved = drag.selection(at: CGPoint(x: grabbed.x + 180, y: grabbed.y - 60), cell: cell, in: screen)
        #expect(moved.start == at(1, 0) && moved.end == at(3, 5))
        #expect(TermHandles(moved, cell: cell).bar(.start) == CGRect(x: 0, y: 20, width: 0, height: 20))
        // No empty first line for the row it selects nothing of.
        #expect(screen.text(moved) == "│ ab │ 中文 │\n好标题好标题 bold plain 斜体\n❯ 好多")
    }

    @Test func endsInBlankRowsMoveToTheTextBetweenThem() throws {
        let screen = try screen(["one", "", "three", ""])
        let between = screen.whole(TermSelection(anchor: at(1, 3), head: at(3, 5)))
        #expect(between == TermSelection(anchor: at(2, 0), head: at(2, 4)))
        #expect(screen.text(between) == "three")
        // A blank row between the ends still copies as an empty line.
        let across = TermSelection(anchor: at(0, 1), head: at(2, 1))
        #expect(screen.whole(across) == across)
        #expect(screen.text(across) == "ne\n\nth")
        // A long press dragged down into a blank row stops at the text.
        #expect(screen.extending(screen.word(at: at(0, 1)), to: at(1, 6)) == TermSelection(anchor: at(0, 0), head: at(0, 2)))
    }

    @Test func aPressOnBlankSpaceKeepsItsCell() throws {
        // No text in reach: the blank cell stays under the finger, for
        // the menu's Paste.
        let screen = try screen(["one", "", "two"])
        #expect(screen.whole(screen.word(at: at(1, 4))) == TermSelection(anchor: at(1, 4), head: at(1, 4)))
        #expect(screen.whole(screen.word(at: at(0, 9))) == TermSelection(anchor: at(0, 9), head: at(0, 9)))
    }

    @Test func copyAfterTheScreenChangesHasNoLineTheHighlightLeavesOut() throws {
        // Selected from column 8 of a row the screen then shortens.
        let selection = TermSelection(anchor: at(0, 8), head: at(1, 2))
        #expect(try screen(["first line", "second"]).text(selection) == "ne\nsec")
        #expect(try screen(["fi", "second"]).text(selection) == "sec")
    }
}
