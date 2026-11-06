import CoreGraphics
import Foundation
import Testing
@testable import SheprdRemoteCore

/// A handle's touch target reaches a fingertip around it, over the cells
/// beside it; a tap there that never moves the handle is a tap on those
/// cells unless it lands on the handle itself.
@Suite struct TermHandleTapTests {
    func at(_ row: Int, _ column: Int) -> TermSelection.Position { .init(row: row, column: column) }

    /// The phone's cells at the fitted size.
    let cell = CGSize(width: 26.0 / 3, height: 17)

    func middle(_ row: Int, _ column: Int) -> CGPoint {
        CGPoint(x: (CGFloat(column) + 0.5) * cell.width, y: (CGFloat(row) + 0.5) * cell.height)
    }

    @Test func aTapBesideAHandleIsInItsTargetButOffIt() {
        // 好多 at columns 2 to 5 of row 3.
        let handles = TermHandles(TermSelection(anchor: at(3, 2), head: at(3, 5)), cell: cell)
        // The S of Shush, two cells right of the end, and a cell of 标题
        // above the start: the targets take them, the handles do not.
        for point in [middle(3, 7), middle(2, 4)] {
            #expect(handles.end(at: point) != nil)
            #expect(!handles.isOnHandle(at: point))
        }
        // The blank right against the end's bar is on it.
        #expect(handles.isOnHandle(at: middle(3, 6)))
    }

    @Test func aDragTheSelectionCannotFollowIsStillADrag() {
        // 斜体, the last word of row 2, its end handle dragged 40 pt right
        // into the blank past the text, or down into the next row: the end
        // stays on 体, yet the finger moved, so lifting it is no tap.
        let selected = TermSelection(anchor: at(2, 25), head: at(2, 28))
        let handles = TermHandles(selected, cell: cell)
        let knob = CGPoint(x: handles.knob(.end).midX, y: handles.knob(.end).midY)
        for lifted in [CGPoint(x: knob.x + 40, y: knob.y), CGPoint(x: knob.x, y: knob.y + 24)] {
            #expect(!TermHandles.isTap(from: knob, to: lifted))
        }
        // A finger that wobbles a few points still taps.
        #expect(TermHandles.isTap(from: knob, to: CGPoint(x: knob.x + 4, y: knob.y - 3)))
    }

    @Test func aTapByAKnobIsOnItsHandle() {
        // bold at columns 13 to 16 of row 2, tapped a little right of its
        // end's knob.
        let handles = TermHandles(TermSelection(anchor: at(2, 13), head: at(2, 16)), cell: cell)
        let knob = handles.knob(.end)
        #expect(handles.isOnHandle(at: CGPoint(x: knob.midX + 6, y: knob.midY)))
        #expect(handles.isOnHandle(at: CGPoint(x: handles.knob(.start).midX, y: handles.knob(.start).minY - 4)))
    }
}
