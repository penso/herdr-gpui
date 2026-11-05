import Foundation

/// Cells selected on a terminal screen, as terminals select: from where a
/// long press began (`anchor`) to where the finger is (`head`), in reading
/// order, by cell, so a wide character is selected by either of its cells.
public struct TermSelection: Equatable, Sendable {
    public struct Position: Comparable, Hashable, Sendable {
        public var row: Int
        public var column: Int

        public init(row: Int, column: Int) {
            self.row = row
            self.column = column
        }

        /// The cell under `point` on a grid of `cell`-sized cells; above
        /// or left of the grid is its first row or column.
        public init(at point: CGPoint, cell: CGSize) {
            row = max(0, Int((point.y / cell.height).rounded(.down)))
            column = max(0, Int((point.x / cell.width).rounded(.down)))
        }

        public static func < (lhs: Position, rhs: Position) -> Bool {
            (lhs.row, lhs.column) < (rhs.row, rhs.column)
        }
    }

    public var anchor: Position
    public var head: Position

    public init(anchor: Position, head: Position) {
        self.anchor = anchor
        self.head = head
    }

    /// Which end of a selection, and its handle.
    public enum End: Sendable {
        case start, end
    }

    public var start: Position { min(anchor, head) }
    public var end: Position { max(anchor, head) }

    /// The columns selected in `row`, whose last cell is `last`.
    public func columns(in row: Int, last: Int) -> ClosedRange<Int>? {
        guard (start.row...end.row).contains(row) else { return nil }
        let from = row == start.row ? start.column : 0
        let to = row == end.row ? end.column : last
        return from <= to ? from...to : nil
    }
}

extension TermScreen {
    /// A row's characters, each with the column it starts at and the cells
    /// it covers; hidden text (SGR 8) is blank.
    static func laidOut(_ row: [Run]) -> [(character: Character, column: Int, cells: Int)] {
        var column = 0
        var laid: [(character: Character, column: Int, cells: Int)] = []
        for run in row {
            for character in run.text {
                let cells = cells(String(character))
                laid.append((run.hidden ? " " : character, column, cells))
                column += cells
            }
        }
        return laid
    }

    /// The word under a long press: the non-blank characters touching its
    /// cell, or the cell alone.
    public func word(at position: TermSelection.Position) -> TermSelection {
        let alone = TermSelection(anchor: position, head: position)
        guard rows.indices.contains(position.row) else { return alone }
        let laid = Self.laidOut(rows[position.row])
        guard let index = laid.firstIndex(where: { position.column < $0.column + $0.cells }), laid[index].column <= position.column,
              !laid[index].character.isWhitespace
        else { return alone }
        var first = index
        while first > 0, !laid[first - 1].character.isWhitespace { first -= 1 }
        var last = index
        while last + 1 < laid.count, !laid[last + 1].character.isWhitespace { last += 1 }
        return TermSelection(anchor: .init(row: position.row, column: laid[first].column),
                             head: .init(row: position.row, column: laid[last].column + laid[last].cells - 1))
    }

    /// The word under a long press extended to `position` as its drag
    /// moves: back past the word it keeps the word's end, otherwise its
    /// start.
    public func extending(_ word: TermSelection, to position: TermSelection.Position) -> TermSelection {
        whole(position < word.start ? TermSelection(anchor: word.end, head: position)
            : TermSelection(anchor: word.start, head: max(position, word.end)))
    }

    /// `selection` on the screen's cells and on whole characters: its
    /// start back to the first cell of the character it is in, its end on
    /// to the last, keeping which end is the anchor. Ends in the blank past
    /// a row's text, where nothing is highlighted, move to the text between
    /// them: the end back to the last character before it, the start on to
    /// the first after it. With no text between them, both stay.
    public func whole(_ selection: TermSelection) -> TermSelection {
        let spans = rows.map { Self.laidOut($0).reduce(0) { $0 + $1.cells } }
        func onScreen(_ position: TermSelection.Position) -> TermSelection.Position {
            .init(row: min(max(position.row, 0), max(rows.count - 1, 0)),
                  column: min(max(position.column, 0), max(columns, spans.max() ?? 0, 1) - 1))
        }
        func snapped(_ position: TermSelection.Position, toLast: Bool) -> TermSelection.Position {
            guard rows.indices.contains(position.row),
                  let character = Self.laidOut(rows[position.row]).first(where: { position.column < $0.column + $0.cells })
            else { return position }
            return .init(row: position.row, column: toLast ? character.column + character.cells - 1 : character.column)
        }
        let anchor = onScreen(selection.anchor), head = onScreen(selection.head)
        var start = min(anchor, head), end = max(anchor, head)
        if !rows.isEmpty {
            // A blank row's last cell is -1: none.
            end.column = min(end.column, spans[end.row] - 1)
            while end.column < 0, end.row > start.row { end = .init(row: end.row - 1, column: spans[end.row - 1] - 1) }
            while start.column >= spans[start.row], start.row < end.row { start = .init(row: start.row + 1, column: 0) }
        }
        guard start <= end else {
            return TermSelection(anchor: snapped(anchor, toLast: anchor > head), head: snapped(head, toLast: head >= anchor))
        }
        let first = snapped(start, toLast: false), last = snapped(end, toLast: true)
        return head >= anchor ? TermSelection(anchor: first, head: last) : TermSelection(anchor: last, head: first)
    }

    /// Whether `position` is a highlighted cell of `selection`: a row
    /// between its ends is highlighted as far as its text.
    public func highlights(_ selection: TermSelection, at position: TermSelection.Position) -> Bool {
        guard rows.indices.contains(position.row) else { return false }
        let last = Self.laidOut(rows[position.row]).reduce(0) { $0 + $1.cells } - 1
        return selection.columns(in: position.row, last: last)?.contains(position.column) == true
    }

    /// Every cell on the screen, ending at the last row's last cell, where
    /// its highlight stops.
    public var everything: TermSelection {
        let last = rows.last.map { Self.laidOut($0).reduce(0) { $0 + $1.cells } } ?? 0
        return TermSelection(anchor: .init(row: 0, column: 0), head: .init(row: max(rows.count - 1, 0), column: max(last - 1, 0)))
    }

    /// The selected text: a line per row, each without the blanks that
    /// end it, and a character partly selected included whole. Its ends
    /// are on the screen's text as it is now (`whole`), so a row the
    /// highlight leaves out adds no empty line after the screen changes.
    public func text(_ selection: TermSelection) -> String {
        let selection = whole(selection)
        return rows.indices.compactMap { index -> String? in
            let laid = Self.laidOut(rows[index])
            guard let columns = selection.columns(in: index, last: Int.max - 1) else { return nil }
            let line = String(laid.filter { $0.column <= columns.upperBound && columns.lowerBound < $0.column + $0.cells }.map(\.character))
            return String(line.reversed().drop(while: \.isWhitespace).reversed())
        }
        .joined(separator: "\n")
    }
}
