import CoreGraphics
import Foundation

/// A selection's two handles on the cell grid, as iOS text selection draws
/// them: a bar a row tall at each end, the start's left of its first cell
/// with a knob above, the end's right of its last cell with a knob below.
public struct TermHandles: Equatable, Sendable {
    /// A knob's diameter: no more than the rows' padding, so a knob above
    /// the first row or below the last shows whole.
    public static let knob: CGFloat = 8
    /// The least a handle's touch target measures each way: a fingertip.
    public static let reach: CGFloat = 44

    public var selection: TermSelection
    public var cell: CGSize

    public init(_ selection: TermSelection, cell: CGSize) {
        self.selection = selection
        self.cell = cell
    }

    /// The handle's bar, as a rectangle no wide.
    public func bar(_ end: TermSelection.End) -> CGRect {
        let at = end == .start ? selection.start : selection.end
        let x = CGFloat(end == .start ? at.column : at.column + 1) * cell.width
        return CGRect(x: x, y: CGFloat(at.row) * cell.height, width: 0, height: cell.height)
    }

    /// The handle's knob, at the far end of its bar.
    public func knob(_ end: TermSelection.End) -> CGRect {
        let bar = bar(end)
        return CGRect(x: bar.midX - Self.knob / 2, y: end == .start ? bar.minY - Self.knob : bar.maxY,
                      width: Self.knob, height: Self.knob)
    }

    /// Where a touch grabs the handle: its bar and knob with room around
    /// them, at least `reach` each way.
    public func target(_ end: TermSelection.End) -> CGRect {
        let handle = bar(end).union(knob(end))
        let height = max(Self.reach, handle.height + Self.knob * 2)
        return CGRect(x: handle.midX - Self.reach / 2, y: handle.midY - height / 2, width: Self.reach, height: height)
    }

    /// The handle a touch at `point` grabs: of those whose target holds
    /// it, the one whose bar and knob are nearer.
    public func end(at point: CGPoint) -> TermSelection.End? {
        func distance(_ end: TermSelection.End) -> CGFloat {
            let handle = bar(end).union(knob(end))
            return hypot(point.x - handle.midX, max(handle.minY - point.y, 0, point.y - handle.maxY))
        }
        return [TermSelection.End.start, .end].filter { target($0).contains(point) }.min { distance($0) < distance($1) }
    }

    /// Whether `point` is on a handle itself, a knob's width around its bar
    /// and knob, rather than on the cells its target reaches over: a tap
    /// there that never moves the handle is a tap on those cells.
    public func isOnHandle(at point: CGPoint) -> Bool {
        [TermSelection.End.start, .end].contains { bar($0).union(knob($0)).insetBy(dx: -Self.knob, dy: -Self.knob).contains(point) }
    }

    /// The selection's rows with both handles, which the copy menu keeps
    /// clear of so they can still be grabbed.
    public var bounds: CGRect {
        bar(.start).union(knob(.start)).union(bar(.end)).union(knob(.end))
    }

    /// Where the copy menu points: above the middle of a one-row
    /// selection, or above the cell the finger last moved to.
    public var menu: CGPoint {
        if selection.start.row == selection.end.row {
            return CGPoint(x: (bar(.start).minX + bar(.end).minX) / 2, y: bar(.start).minY)
        }
        return CGPoint(x: (CGFloat(selection.head.column) + 0.5) * cell.width, y: CGFloat(selection.head.row) * cell.height)
    }
}

/// A handle the finger holds: the end it grabbed follows the finger cell
/// by cell, the other stays, and the two swap if they cross.
public struct TermHandleDrag: Equatable, Sendable {
    /// The end that stays.
    public var fixed: TermSelection.Position
    /// How far the finger is from the middle of the cell it moves, so the
    /// handle does not jump when grabbed by its knob.
    public var offset: CGSize

    /// Grabs the handle under `point`, if any.
    public init?(_ handles: TermHandles, at point: CGPoint) {
        guard let end = handles.end(at: point) else { return nil }
        let selection = handles.selection
        let moving = end == .start ? selection.start : selection.end
        fixed = end == .start ? selection.end : selection.start
        offset = CGSize(width: point.x - (CGFloat(moving.column) + 0.5) * handles.cell.width,
                        height: point.y - (CGFloat(moving.row) + 0.5) * handles.cell.height)
    }

    /// The selection with the grabbed end where the finger is now.
    public func selection(at point: CGPoint, cell: CGSize, in screen: TermScreen) -> TermSelection {
        let moved = TermSelection.Position(at: CGPoint(x: point.x - offset.width, y: point.y - offset.height), cell: cell)
        return screen.whole(TermSelection(anchor: fixed, head: moved))
    }
}
