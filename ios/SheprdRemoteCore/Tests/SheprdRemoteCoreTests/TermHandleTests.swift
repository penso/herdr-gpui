import CoreGraphics
import Foundation
import Testing
@testable import SheprdRemoteCore

@Suite struct TermHandleTests {
    func screen(_ rows: [String]) throws -> TermScreen {
        let json = rows.map { #"[{"text":"\#($0)"}]"# }.joined(separator: ",")
        return try JSONDecoder().decode(TermScreen.self, from: Data(#"{"pane":"p","columns":20,"rows":[\#(json)]}"#.utf8))
    }

    func at(_ row: Int, _ column: Int) -> TermSelection.Position { .init(row: row, column: column) }

    /// Cells 10 points wide and 20 tall.
    let cell = CGSize(width: 10, height: 20)

    /// The middle of a cell, where a finger lands on it.
    func middle(_ row: Int, _ column: Int) -> CGPoint {
        CGPoint(x: (CGFloat(column) + 0.5) * cell.width, y: (CGFloat(row) + 0.5) * cell.height)
    }

    @Test func aPointIsInTheCellUnderIt() {
        #expect(TermSelection.Position(at: CGPoint(x: 29.9, y: 40), cell: cell) == at(2, 2))
        // Above or left of the first cell is the first cell.
        #expect(TermSelection.Position(at: CGPoint(x: -4, y: -30), cell: cell) == at(0, 0))
    }

    @Test func handlesStandAtTheEndsOnTheCellGrid() {
        let handles = TermHandles(TermSelection(anchor: at(1, 4), head: at(0, 2)), cell: cell)
        // The start's bar is left of its first cell, its knob above.
        #expect(handles.bar(.start) == CGRect(x: 20, y: 0, width: 0, height: 20))
        #expect(handles.knob(.start).midX == 20 && handles.knob(.start).maxY == 0)
        // The end's bar is right of its last cell, its knob below.
        #expect(handles.bar(.end) == CGRect(x: 50, y: 20, width: 0, height: 20))
        #expect(handles.knob(.end).midX == 50 && handles.knob(.end).minY == 40)
    }

    @Test func aHandleIsAtLeastAFingerWide() {
        let handles = TermHandles(TermSelection(anchor: at(0, 2), head: at(0, 4)), cell: CGSize(width: 4, height: 8))
        for end in [TermSelection.End.start, .end] {
            let target = handles.target(end)
            #expect(target.width >= 44 && target.height >= 44)
            #expect(target.contains(handles.knob(end)) && target.contains(CGPoint(x: handles.bar(end).midX, y: handles.bar(end).midY)))
        }
    }

    @Test func aTouchGrabsTheNearerHandle() {
        // One cell selected: the handles' targets overlap.
        let handles = TermHandles(TermSelection(anchor: at(3, 5), head: at(3, 5)), cell: cell)
        #expect(handles.end(at: CGPoint(x: 50, y: 52)) == .start)
        #expect(handles.end(at: CGPoint(x: 60, y: 88)) == .end)
        #expect(handles.end(at: CGPoint(x: 44, y: 70)) == .start)
        #expect(handles.end(at: CGPoint(x: 66, y: 70)) == .end)
        // Away from both, it grabs neither and selects or scrolls instead.
        #expect(handles.end(at: CGPoint(x: 150, y: 70)) == nil)
        #expect(handles.end(at: CGPoint(x: 55, y: 200)) == nil)
    }

    @Test func draggingTheEndMovesItCellByCellAndAcrossRows() throws {
        let screen = try screen(["hello world", "second line"])
        let word = TermSelection(anchor: at(0, 0), head: at(0, 4))
        // The finger lands on the end's knob, below the row.
        let grabbed = CGPoint(x: 50, y: 25)
        let drag = try #require(TermHandleDrag(TermHandles(word, cell: cell), at: grabbed))
        // Letting go where it was grabbed keeps the selection.
        #expect(drag.selection(at: grabbed, cell: cell, in: screen) == word)
        // Under half a cell stays; past it moves to the next cell.
        #expect(drag.selection(at: CGPoint(x: 54, y: 25), cell: cell, in: screen).end == at(0, 4))
        #expect(drag.selection(at: CGPoint(x: 56, y: 25), cell: cell, in: screen).end == at(0, 5))
        let moved = drag.selection(at: CGPoint(x: 70, y: 45), cell: cell, in: screen)
        #expect(moved.start == at(0, 0) && moved.end == at(1, 6))
        #expect(screen.text(moved) == "hello world\nsecond")
    }

    @Test func draggingTheStartKeepsTheEnd() throws {
        let screen = try screen(["hello world"])
        let word = TermSelection(anchor: at(0, 6), head: at(0, 10))
        let handles = TermHandles(word, cell: cell)
        let grabbed = CGPoint(x: handles.knob(.start).midX, y: handles.knob(.start).midY)
        let drag = try #require(TermHandleDrag(handles, at: grabbed))
        let moved = drag.selection(at: CGPoint(x: grabbed.x - 40, y: grabbed.y), cell: cell, in: screen)
        #expect(moved.start == at(0, 2) && moved.end == at(0, 10))
        #expect(screen.text(moved) == "llo world")
    }

    @Test func aHandleDraggedPastTheOtherEndSwapsThem() throws {
        let screen = try screen(["abcdefghij"])
        let selection = TermSelection(anchor: at(0, 4), head: at(0, 6))
        let drag = try #require(TermHandleDrag(TermHandles(selection, cell: cell), at: CGPoint(x: 40, y: -5)))
        // The start dragged to column 8: the old end is now the start.
        let crossed = drag.selection(at: CGPoint(x: 80, y: -5), cell: cell, in: screen)
        #expect(crossed.start == at(0, 6) && crossed.end == at(0, 8))
        #expect(screen.text(crossed) == "ghi")
        // Its handles follow: the start's now stands at the old end's cell.
        #expect(TermHandles(crossed, cell: cell).bar(.start).minX == 60)
    }

    @Test func aHandleSelectsWholeCharacters() throws {
        // ❯ and a space, then five characters two cells each from column 2.
        let screen = try screen(["❯ 好的回电话 ok"])
        let first = TermSelection(anchor: at(0, 2), head: at(0, 3))
        let end = try #require(TermHandleDrag(TermHandles(first, cell: cell), at: middle(0, 3)))
        // Into the first cell of 的: all of it.
        #expect(end.selection(at: middle(0, 4), cell: cell, in: screen) == TermSelection(anchor: at(0, 2), head: at(0, 5)))
        let last = TermSelection(anchor: at(0, 10), head: at(0, 11))
        let start = try #require(TermHandleDrag(TermHandles(last, cell: cell), at: middle(0, 10)))
        // Into the second cell of 回: all of it.
        let back = start.selection(at: middle(0, 7), cell: cell, in: screen)
        #expect(back.start == at(0, 6) && back.end == at(0, 11))
        #expect(screen.text(back) == "回电话")
        // Past the other end, the character it held stays whole.
        let crossed = start.selection(at: middle(0, 13), cell: cell, in: screen)
        #expect(crossed.start == at(0, 10) && crossed.end == at(0, 13))
        #expect(screen.text(crossed) == "话 o")
    }

    @Test func aSelectionStaysOnTheScreen() throws {
        let screen = try screen(["one", "two"])
        let whole = screen.whole(TermSelection(anchor: at(0, 1), head: at(9, 99)))
        // The end stops at the last row's text, where its highlight does.
        #expect(whole.start == at(0, 1) && whole.end == at(1, 2))
    }

    @Test func aLongPressDragExtendsFromTheWord() throws {
        let screen = try screen(["ls 好的 -la /tmp"])
        let word = screen.word(at: at(0, 9))
        #expect(word.start == at(0, 8) && word.end == at(0, 10))
        // Back past the word, it keeps the word's end.
        let back = screen.extending(word, to: at(0, 4))
        #expect(back.start == at(0, 3) && back.end == at(0, 10))
        #expect(screen.text(back) == "好的 -la")
        // Forward, it keeps the word's start; inside it, the word stays.
        #expect(screen.extending(word, to: at(0, 13)) == TermSelection(anchor: at(0, 8), head: at(0, 13)))
        #expect(screen.extending(word, to: at(0, 9)) == word)
    }

    @Test func aTapLandsInsideOnlyOnHighlightedCells() throws {
        let screen = try screen(["first line", "ab", "third"])
        let selection = TermSelection(anchor: at(0, 6), head: at(2, 2))
        #expect(screen.highlights(selection, at: at(0, 7)))
        #expect(!screen.highlights(selection, at: at(0, 5)))
        // A row between the ends is highlighted as far as its text.
        #expect(screen.highlights(selection, at: at(1, 1)))
        #expect(!screen.highlights(selection, at: at(1, 4)))
        #expect(!screen.highlights(selection, at: at(2, 3)))
        #expect(!screen.highlights(selection, at: at(5, 0)))
    }

    @Test func theMenuKeepsClearOfTheHandles() {
        let handles = TermHandles(TermSelection(anchor: at(1, 4), head: at(0, 2)), cell: cell)
        // From the start's knob above row 0 to the end's below row 1.
        #expect(handles.bounds == CGRect(x: 16, y: -8, width: 38, height: 56))
    }

    @Test func aKnobFitsInTheRowsPadding() {
        // The rows have 8 points around them inside a scroll view that
        // clips: a knob above row 0 or below the last row shows whole.
        let handles = TermHandles(TermSelection(anchor: at(0, 0), head: at(1, 3)), cell: cell)
        #expect(handles.knob(.start).minY >= -8)
        #expect(handles.knob(.end).maxY <= 2 * cell.height + 8)
    }

    @Test func theMenuPointsAboveTheSelection() {
        // One row: above its middle.
        let word = TermHandles(TermSelection(anchor: at(2, 3), head: at(2, 6)), cell: cell)
        #expect(word.menu == CGPoint(x: 50, y: 40))
        // Several: above the cell the finger last moved.
        let lines = TermHandles(TermSelection(anchor: at(0, 3), head: at(2, 6)), cell: cell)
        #expect(lines.menu == CGPoint(x: 65, y: 40))
    }
}
